# Dembly PoC 仕様書

## 1. 目的

Dembly は Docker Compose service に immutable Card を組み込む Rust 製設定コンパイラである。

Host Dembly は Compose file を更新する。

Runtime Dembly は container 内で Card SquashFS を mount し、元の process を実行する。

Card の変更だけを理由に Base image を再 build してはならない。

## 2. スコープ

対象は Linux x86_64、Docker Engine、Docker Compose v2、kernel SquashFS とする。

Image Base、FUSE、rootless Docker、Docker named volume、Kubernetes、Podman は対象外とする。

Host mount namespace で SquashFS を mount してはならない。

## 3. 設定

`.dembly/config.toml` が Dembly 設定の正本である。

`--config <path>` がなければ current directory の同 file だけを探索する。

親 directory を探索してはならない。

相対 path と `${DECK_ROOT}` は `.dembly/` を基準に解決する。

```toml
schema_version = 1

[compose]
path = "../.devcontainer/compose.dembly.yaml"
service = "dev"

[[cards]]
path = "/var/lib/dembly/cards/clang/card.toml"
```

`compose.path` と `compose.service` は必須である。

Card は `card.toml` と `rootfs.squashfs` からなる。

Card manifest は SHA-256、absolute mount target、environment、export、hook、Volume、Host Bind を定義できる。

Card name、mount target、export target、exact mount target、normal environment の競合は validation error とする。

## 4. Host CLI

```text
dembly init|validate|lock|apply|unapply|inspect|check [--config <path>]
dembly card build <tool-root> <cards-root>
```

`up`、`down`、`run`、`exec` は提供してはならない。

Host Dembly は Docker container を create、start、stop、remove、exec してはならない。

`init` は existing config を上書きしてはならない。

`init` は current directory 内の `card.toml` と `devcontainer.json` を再帰探索して候補を表示する。

`.git/`、`.dembly/`、symlink directory は探索してはならない。

候補の採用は人間が選択する。

## 5. Lock と適用

`dembly lock` だけが管理 Compose file の `x-dembly.lock` を更新する。

lock は selected service、resolved image identity、Card name、version、manifest SHA-256、filesystem SHA-256 を保持する。

`apply` と `check` は valid lock を要求し、暗黙更新してはならない。

`apply` は `compose.path` の file だけを更新する。

他の Compose file に `x-dembly` があれば validation error とする。

`x-dembly` は schema version、lock、管理 field ごとの `original` と `applied` を保持する。

現在値が `applied` と異なる場合、`apply` と `unapply` は conflict error を返す。

`unapply` は conflict がない場合だけ original field を復元し、管理 field、`x-dembly`、`.dembly/runtime/` を削除する。

`apply` は selected service に Runtime binary、runtime plan、Card SquashFS、Volume、Host Bind、`privileged: true`、root user、Runtime Dembly entrypoint、labels を注入する。

`apply` は current executable を `.dembly/runtime/bin/dembly` へコピーする。

`.dembly/runtime/` と `.dembly/volumes/` は `.gitignore` へ追加する。

## 6. Runtime

`apply` は original entrypoint と command を runtime plan と `x-dembly` に保存する。

entrypoint は次へ置換し、command は空にする。

```text
/run/dembly/bin/dembly __runtime init /run/dembly/runtime/<service>.toml
```

Runtime Dembly は root で SquashFS を mount し、Volume、Host Bind、export、hook、environment を設定する。

最終 process は Dockerfile `USER`、次に Compose `user` の解決結果で exec する。

`docker compose run <service> <command...>` の追加 argv は original process を置換する。

runtime plan は schema version、runtime user、Card mount、export、hook、merged environment、original argv を持つ。

Runtime Dembly は config.toml と card.toml を再解釈してはならない。

## 7. 起動経路

AI は `dembly apply` 後に native `docker compose up/exec/down` を使用する。

AI の通常 exec は intended user を明示する。

人間は VS Code Dev Containers を使用できる。

`devcontainer.path` がある場合、Dembly は service 一致、管理 Compose file が `dockerComposeFile` の最後であることを検証する。

`overrideCommand` は false または未指定とする。

`containerUser` は root または未指定とする。

`remoteUser` は intended user と一致しなければならない。

`initializeCommand` は利用者管理の Host 初期化 script を呼び、その script は `dembly apply` を実行する。

## 8. 受入条件

- init が候補選択で config を生成できる。
- lock、apply、unapply が conflict を検出する。
- Card 変更は Base image rebuild なしで Compose 再作成に反映される。
- SquashFS mount は Runtime 内だけに存在する。
- 元の process は intended user で実行される。
- AI の native Compose と人間の Dev Containers は同じ Compose、Dockerfile、Card を必須環境として使う。
