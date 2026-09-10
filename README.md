# dembly
# Dembly

Dembly は Docker/OCI Image を Base とし、immutable な SquashFS Card と Deck root 配下の永続 Volume を組み合わせる Linux 向け開発環境 PoC です。

Deck は Base、Card、Volume、Host Bind の宣言です。
Card は `card.toml` と `rootfs.squashfs` で構成し、`emcos-sdk`、`clang`、`tis` を特別扱いしません。
Runtime は Host の同一 `dembly` binary を read-only bind し、container 内だけで Card SquashFS を kernel mount します。
Host は SquashFS file を mount しません。

## 前提条件

Linux x86_64、Docker Engine と Compose v2、kernel SquashFS/loop support、`mount`、`mksquashfs`、`git`、`mise` が必要です。
rootless Docker は PoC 対象外です。

```bash
./scripts/check-linux.sh
./scripts/setup-dev.sh
mise run install
```

`mise run install` と `mise run upgrade` は、proxy が HTTP 407 を返した Rust/mise host を既存の `NO_PROXY` と `no_proxy` に追記して実行します。

開発時の品質ゲートは次のとおりです。

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release --target x86_64-unknown-linux-musl -p dembly-cli
```

## Card

Card は以下の形式です。

```text
cards/clang/
├── card.toml
└── rootfs.squashfs
```

`filesystem.sha256` は build 時に計算されます。

```bash
dembly card build /opt/clang ./cards \
  --name clang --version 20.1.0 \
  --mount-target /opt/dembly/cards/clang \
  --path-prepend bin --non-interactive
```

interactive mode では Card name、version、mount target を入力します。
`mksquashfs -noappend -comp zstd` で temporary artifact を作成後、atomic rename します。

## Image Base Deck

```toml
schema_version = 1
name = "example"

[base]
image = "example-base:local"

[[cards]]
path = "cards/clang/card.toml"

[[volumes]]
name = "build"
target = "/workspace/build"

[[binds]]
source = "."
target = "/workspace"
mode = "rw"
```

Deck-relative path は `deck.toml` の親 directory を基準に解決します。
Deck discovery は current directory の `deck.toml` だけを対象にし、親 directory を探索しません。

```bash
dembly validate
dembly lock
dembly up
dembly inspect
dembly down
```

`lock` は Docker image ID と Card manifest/SquashFS checksum を `deck.lock` に固定します。
`up` は lock の存在と一致を要求し、暗黙更新しません。
`down` は Dembly ownership label を検証して Runtime metadata を削除しますが、`volumes/` と Card artifact は削除しません。

Volume は Docker named volume ではありません。
Deck private Volume は `volumes/<name>`、Card private Volume は `volumes/<card>/<name>`、`shared = true` は `volumes/<name>` です。
同じ name で shared/private を混在させると validation error です。

Host Bind source には `${HOST_HOME}` と `${DECK_ROOT}`、target には `${USER}` と `${HOME}` を使用できます。
`required` の default は `true` です。
optional source が存在しない場合は warning を表示して skip します。

## Architecture and limitations

`dembly-core` は TOML model、path/variable/resource planning、validation を担当します。
`dembly-card` は SquashFS artifact を作ります。
`dembly-docker` は Docker command planning/invocation を担当します。
`dembly-runtime` は container 内 mount、export、hook、privilege drop を担当します。
`dembly-cli` は単一 executable `dembly` の command dispatch を担当します。

Runtime は privileged mode で起動します。
Card は trusted artifact とし、root hook、Host Bind、container 内の mount capability を許可します。
これは untrusted Card の sandbox ではありません。
secret source の内容を Runtime metadata や log へコピーしません。

ローカル Alpine fixture による Image/Compose lifecycle integration は `cargo test --workspace` で検証済みです。
`emcos-sdk`、`clang`、`tis` の実 artifact acceptance と performance measurement は、対象 artifact を利用できる環境で別途実行します。
FUSE、Docker named volume、remote Card repository、dependency solver、署名、rootless Docker は PoC scope 外です。

## Compose Base

Compose Deck の宣言は TOML の `[base] compose` と `service` で行い、user-authored `compose.yaml` を変更せず generated override を使う設計です。
`lock`、`up`、`down`、`run`、`exec`、`check` は generated override を使用します。
selected service だけを Runtime 化し、non-selected service の compose definition は変更しません。

## Evaluation procedure

performance evaluation は同一 workload を通常 Image と Card Runtime で各 5 回以上実行し、median wall-clock を比較します。
clean/incremental build、Clang compile、TIS analysis を記録対象とします。
Card filesystem overhead `<= 10%` は PoC target であり production guarantee ではありません。

disk evaluation は pre-composed image 群と Base + Card artifact 群について、`docker system df`、`docker image inspect`、`du`、SquashFS file size を併記して比較します。
