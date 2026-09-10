# Dembly PoC 仕様書

- **文書種別**: PoC 実装仕様
- **正式名称**: Dembly
- **実装言語**: Rust
- **対象ホスト**: Linux x86_64
- **コンテナランタイム**: Docker Engine / Docker Compose v2
- **設定形式**: TOML
- **Card filesystem**: kernel SquashFS
- **本仕様の主な実装利用者**: Codex
- **仕様記述方式**: EARS（Easy Approach to Requirements Syntax）
- **ステータス**: 実装開始可能

---

# 1. 目的

Dembly は、Docker / OCI Image を Base とし、事前ビルド済み immutable filesystem を持つ複数の Card と永続 RW データである Volume を組み合わせ、Docker Image を Card 構成ごとに再ビルドせずに開発環境を生成するオーケストレータである。

概念モデル:

```text
Deck
├── Base
│   ├── Docker / OCI Image
│   └── または compose.yaml 内の選択された service
├── Cards
│   ├── emcos-sdk
│   ├── clang
│   └── tis
├── Volumes
└── Host Binds
       │
       ▼
     Dembly
       │
       ▼
    Runtime
 Docker Container
```

PoC の主要な成立条件は、**Card の選択を変更しても Base Image を再ビルドする必要がなく、Card の SquashFS を Host では mount せず Runtime 内だけで kernel mount して、通常の Linux filesystem / process semantics で実ツールを利用できること**である。

---

# 2. 用語

| 用語 | 定義 |
|---|---|
| **Dembly** | オーケストレータ／プロダクト |
| **Base** | Docker / OCI Image、または Docker Compose project 内で Dembly Runtime 化する選択 service |
| **Card** | manifest と pre-built immutable filesystem からなる機能単位。PoC では filesystem は SquashFS |
| **Deck** | Base、Cards、Volumes、Host Binds 等からなる環境定義 |
| **Runtime** | Deck を実体化した Docker Container。Compose Base の場合は、選択 service の Container が Dembly Runtime となる |
| **Volume** | Deck root 配下に保存する永続 RW データ |
| **Host Bind** | Deck root 内外の Host file / directory を Runtime へ bind mount する定義 |
| **Deck root** | 解決された `deck.toml` の親 directory |
| **Card root** | `card.toml` の親 directory |
| **Lock file** | `deck.lock`。Deck が使用する immutable artifact の解決結果を固定するファイル |

名称の由来:

> **DE**ck + asse**MBLY** → **Dembly**

PoC の初期 Card は以下とする。

- `emcos-sdk`
- `clang`
- `tis`

Dembly の仕組み上、これらはすべて同列の Card として扱う。

---

# 3. EARS 記述規約

本仕様の要求事項は、原則として以下の EARS パターンで記載する。

| 種別 | 形式 |
|---|---|
| Ubiquitous | `Dembly は、〜しなければならない。` |
| Event-driven | `〜したとき、Dembly は、〜しなければならない。` |
| State-driven | `〜である間、Dembly は、〜しなければならない。` |
| Unwanted behavior | `もし〜なら、Dembly は、〜しなければならない。` |
| Optional feature | `〜を使用する場合、Dembly は、〜しなければならない。` |

規範レベル:

- **MUST**: 必須
- **MUST NOT**: 禁止
- **SHOULD**: 推奨
- **MAY**: 任意

各要求には一意な Requirement ID を付与する。

---

# 4. PoC スコープ

## 4.1 Goals

### REQ-GEN-001 [Ubiquitous][MUST]
Dembly は、Rust で実装しなければならない。

### REQ-GEN-002 [Ubiquitous][MUST]
Dembly は、配布成果物として単一 executable `dembly` を生成しなければならない。

### REQ-GEN-003 [Ubiquitous][MUST]
Dembly は、Host CLI と Runtime initialization で同一の `dembly` executable を使用しなければならない。

### REQ-GEN-004 [Ubiquitous][MUST]
Dembly は、Card の選択変更だけを理由として Base Image を再ビルドしてはならない。

### REQ-GEN-005 [Ubiquitous][MUST]
Dembly は、PoC において Card filesystem として kernel SquashFS を使用しなければならない。

### REQ-GEN-006 [Ubiquitous][MUST NOT]
Dembly は、Card の SquashFS filesystem を Host mount namespace に mount してはならない。

### REQ-GEN-007 [Ubiquitous][MUST]
Dembly は、Card の SquashFS filesystem を Runtime mount namespace 内で mount しなければならない。

### REQ-GEN-008 [Ubiquitous][MUST]
Dembly は、Image Base と Compose Base の双方をサポートしなければならない。

### REQ-GEN-009 [Ubiquitous][MUST]
Dembly は、`emcos-sdk`、`clang`、`tis` を generic Dembly code 上で同列の Card として扱わなければならない。

### REQ-GEN-010 [Ubiquitous][MUST NOT]
Dembly は、`emcos-sdk` を Dembly 内部の特別な必須 Card として hard-code してはならない。

---

## 4.2 Non-goals

PoC では以下を実装対象外とする。

- Dembly Card Repository Server
- Card remote pull / push
- semantic-version dependency solver
- Card dependency resolver
- Card conflict solver
- cryptographic signing
- trust policy / approval UI
- SBOM
- OCI Artifact publishing
- FUSE
- EROFS
- rootless Docker
- production-grade least privilege
- Kubernetes
- Podman
- Windows containers
- Docker Desktop compatibility guarantee
- Dev Container integration
- GUI / TUI
- untrusted Card sandbox
- production-grade snapshot consistency
- `card.toml` + `rootfs.squashfs` を 1 ファイルへ包む独自 package format

### REQ-SCP-001 [Ubiquitous][MUST NOT]
Dembly は、PoC において FUSE を Card mount backend として使用してはならない。

### REQ-SCP-002 [Ubiquitous][MUST NOT]
Dembly は、PoC において Docker named volume を Dembly Volume の永続化方式として使用してはならない。

---

# 5. リポジトリ構成

初期状態:

```text
.
├── .git/                 # 空の README.md のみ commit 済み
├── design/
│   └── spec.md
└── README.md
```

目標構成:

```text
.
├── .git/
├── Cargo.toml
├── Cargo.lock
├── README.md
├── mise/
│   └── config.toml
├── crates/
│   ├── dembly-cli/
│   ├── dembly-core/
│   ├── dembly-docker/
│   ├── dembly-runtime/
│   └── dembly-card/
├── scripts/
│   ├── setup-dev.sh
│   └── check-linux.sh
├── design/
│   └── spec.md
├── examples/
│   ├── image-base/
│   └── compose-base/
└── tests/
    ├── fixtures/
    └── integration/
```

### REQ-REP-001 [Ubiquitous][MUST]
Dembly の repository は、少なくとも上記責務に相当する Rust workspace 構造を持たなければならない。

### REQ-REP-002 [Ubiquitous][MAY]
実装者は、crate 数や内部 module 構造を変更してもよい。ただし、本仕様で定義した責務境界を保持しなければならない。

---

# 6. Rust アーキテクチャ

## 6.1 Single executable

### REQ-RST-001 [Ubiquitous][MUST]
Dembly は、最終的に `dembly` という名前の単一 executable を生成しなければならない。

Host-side commands:

```text
dembly validate
dembly lock
dembly up
dembly down
dembly run -- <command...>
dembly exec -- <command...>
dembly inspect
dembly check
dembly card build <tool-root> <cards-root>
```

Runtime internal command:

```text
/run/dembly/bin/dembly __runtime init /run/dembly/runtime.toml
```

### REQ-RST-002 [Ubiquitous][MUST]
`__runtime` command namespace は internal interface とし、通常の end-user help に表示してはならない。

---

## 6.2 Host executable injection

### REQ-RST-003 [Event-driven][MUST]
Runtime を生成するとき、Dembly は、Rust の `std::env::current_exe()` 相当を使用して現在実行中の `dembly` executable の実体 path を取得しなければならない。

### REQ-RST-004 [Event-driven][MUST]
Runtime を生成するとき、Dembly は、取得した executable を Runtime の `/run/dembly/bin/dembly` へ read-only bind mount しなければならない。

### REQ-RST-005 [Ubiquitous][MUST NOT]
Dembly は、Base Image に Dembly executable が事前インストールされていることを要求してはならない。

---

## 6.3 Static build

### REQ-RST-006 [Ubiquitous][MUST]
PoC で使用する `dembly` executable は、Linux x86_64 向け static executable として build できなければならない。

### REQ-RST-007 [Ubiquitous][SHOULD]
static build target は `x86_64-unknown-linux-musl` を第一候補とする。

### REQ-RST-008 [Event-driven][MUST]
PoC acceptance test を実行するとき、Host で使用した同一 `dembly` binary が Base Runtime 内でも実行できることを確認しなければならない。

---

## 6.4 Crate responsibility

### `dembly-core`

- Deck/Card/Lock model
- TOML parsing
- schema validation
- variable expansion
- path normalization
- Volume resolution
- Host Bind resolution
- collision detection
- environment planning
- Card ordering
- lock comparison
- domain error

### `dembly-docker`

- Docker / Docker Compose invocation
- image inspect / pull / build
- container create/start/stop/remove
- exec
- Compose project control
- generated Compose override
- Docker labels
- Runtime user probe
- image identity resolution

### `dembly-runtime`

- internal Runtime initialization
- SquashFS mount
- export creation
- hook execution
- environment preparation
- UID/GID drop
- final `exec()`

### `dembly-card`

- `dembly card build`
- `mksquashfs` invocation
- SHA-256 calculation
- Card manifest generation

### `dembly-cli`

- clap command definition
- command dispatch
- user-facing output
- exit status

### REQ-RST-009 [Ubiquitous][MUST]
Rust crate dependency graph は循環依存を持ってはならない。

### REQ-RST-010 [Ubiquitous][MUST]
`dembly-core` は Docker command を直接実行してはならない。

---

# 7. 開発環境管理

## 7.1 mise

### REQ-DEV-001 [Ubiquitous][MUST]
Dembly 開発に必要な version-managed user-space tool は、`mise/config.toml` で管理しなければならない。

### REQ-DEV-002 [Ubiquitous][MUST]
Rust toolchain / rustc も `mise/config.toml` で管理しなければならない。

### REQ-DEV-003 [Ubiquitous][MUST]
Dembly の開発環境セットアップ前にインストール済みと仮定してよい tool は、`git` と `mise` のみとする。

### REQ-DEV-004 [Ubiquitous][MAY]
Docker daemon、Linux kernel filesystem support 等、通常の per-project mise tool として provision できないものは Host prerequisite として扱ってよい。

### REQ-DEV-005 [Ubiquitous][MUST]
repository は、少なくとも以下を実行可能にしなければならない。

```bash
cargo build
cargo test
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
```

---

## 7.2 `scripts/setup-dev.sh`

### REQ-DEV-010 [Event-driven][MUST]
`scripts/setup-dev.sh` を実行したとき、script は repository root を自動解決しなければならない。

### REQ-DEV-011 [Event-driven][MUST]
`scripts/setup-dev.sh` を実行したとき、script は `mise/config.toml` を使用して project tool を install しなければならない。

### REQ-DEV-012 [Event-driven][MUST]
`scripts/setup-dev.sh` を実行したとき、script は必要な Rust target / component を準備しなければならない。

### REQ-DEV-013 [Ubiquitous][MUST]
`scripts/setup-dev.sh` は idempotent でなければならない。

### REQ-DEV-014 [Ubiquitous][MUST NOT]
`scripts/setup-dev.sh` は `sudo` を暗黙実行してはならない。

### REQ-DEV-015 [Unwanted behavior][MUST]
必要な system prerequisite が不足している場合、`scripts/setup-dev.sh` は不足内容を表示して non-zero で終了しなければならない。

### REQ-DEV-016 [Event-driven][MUST]
`scripts/setup-dev.sh` を実行したとき、script は `scripts/check-linux.sh` を実行しなければならない。

---

## 7.3 `scripts/check-linux.sh`

### REQ-DEV-020 [Ubiquitous][MUST]
`scripts/check-linux.sh` は Host を変更してはならない。

### REQ-DEV-021 [Ubiquitous][MUST NOT]
`scripts/check-linux.sh` は `sudo` を実行してはならない。

### REQ-DEV-022 [Event-driven][MUST]
`scripts/check-linux.sh` を実行したとき、少なくとも以下を検証しなければならない。

- Linux host
- x86_64 architecture
- `git`
- `mise`
- Docker CLI
- running Docker daemon への access
- Docker Compose v2 (`docker compose`)
- kernel SquashFS support
- loop-device support
- `mount`
- `mksquashfs`
- configured static Rust target を build 可能な環境
- 実装が追加で依存する system prerequisite

### REQ-DEV-023 [Unwanted behavior][MUST]
必須 prerequisite が 1 つでも満たされない場合、`scripts/check-linux.sh` は non-zero で終了しなければならない。

### REQ-DEV-024 [Event-driven][MUST]
すべての必須 prerequisite が満たされる場合、`scripts/check-linux.sh` は exit 0 を返さなければならない。

推奨出力例:

```text
[OK] Linux x86_64
[OK] git
[OK] mise
[OK] Docker CLI
[OK] Docker daemon
[OK] Docker Compose
[OK] kernel SquashFS
[OK] loop support
[FAIL] mksquashfs not found
```

---

# 8. Deck discovery

### REQ-DCK-001 [Event-driven][MUST]
Deck command に明示的な `deck.toml` path が指定されたとき、Dembly はその file を使用しなければならない。

### REQ-DCK-002 [Event-driven][MUST]
Deck command に `deck.toml` path が指定されなかったとき、Dembly は `<current-working-directory>/deck.toml` のみを探索しなければならない。

### REQ-DCK-003 [Ubiquitous][MUST NOT]
Dembly は、`deck.toml` を parent directory へ遡って探索してはならない。

### REQ-DCK-004 [Unwanted behavior][MUST]
`deck.toml` path が省略され、current working directory に `deck.toml` が存在しない場合、Dembly は error を返さなければならない。

### REQ-DCK-005 [Event-driven][MUST]
`deck.toml` を解決したとき、Dembly はその親 directory を `DECK_ROOT` と定義しなければならない。

---

# 9. 設定形式

### REQ-CFG-001 [Ubiquitous][MUST]
Dembly 独自設定は TOML 形式でなければならない。

対象:

```text
deck.toml
deck.lock
card.toml
runtime.toml
```

### REQ-CFG-002 [Ubiquitous][MUST NOT]
Dembly 独自設定 format として YAML を使用してはならない。

### REQ-CFG-003 [Optional feature][MUST]
Docker Compose を使用する場合、外部標準である `compose.yaml` / `compose.yml` は YAML のまま扱わなければならない。

### REQ-CFG-004 [Unwanted behavior][MUST]
Dembly TOML に未知 field が存在する場合、Dembly はその field を無視せず validation error にしなければならない。

---

# 10. Card artifact

PoC の Card layout:

```text
<card-root>/
├── card.toml
└── rootfs.squashfs
```

### REQ-CRD-001 [Ubiquitous][MUST]
PoC の Card は、`card.toml` と `rootfs.squashfs` の 2 file で構成しなければならない。

### REQ-CRD-002 [Ubiquitous][MUST NOT]
PoC では `card.toml` と `rootfs.squashfs` を独自単一 package file に包む機能を必須実装してはならない。

### REQ-CRD-003 [Event-driven][MUST]
`card.toml` 内の relative path を解決するとき、Dembly はその `card.toml` の親 directory を基準にしなければならない。ただし、別途本仕様で基準を定義する field を除く。

---

# 11. Card manifest

例:

```toml
schema_version = 1
name = "clang"
version = "20.1.0"

[filesystem]
type = "squashfs"
file = "rootfs.squashfs"
sha256 = "<lowercase hex sha256>"

[mount]
target = "/opt/dembly/cards/clang"

[environment]
CLANG_RESOURCE = "example"

[environment_path]
prepend = ["bin"]

[[exports]]
source = "bin/clang"
target = "/usr/local/bin/clang"

[[volumes]]
name = "cache"
target = "/var/cache/clang"
shared = false

[[binds]]
source = "${HOST_HOME}/.example/config"
target = "${HOME}/.example/config"
mode = "ro"
required = false

[[hooks.post_mount]]
exec = "setup/post-mount.sh"
args = ["--example"]

[check]
exec = "bin/clang"
args = ["--version"]
```

## 11.1 Required fields

### REQ-CRD-010 [Ubiquitous][MUST]
`card.toml` は少なくとも以下を含まなければならない。

```text
schema_version
name
version
filesystem.type
filesystem.file
filesystem.sha256
mount.target
```

### REQ-CRD-011 [Unwanted behavior][MUST]
`filesystem.type` が `squashfs` 以外の場合、PoC の Dembly は validation error にしなければならない。

---

## 11.2 Card name

### REQ-CRD-020 [Ubiquitous][MUST]
Card name は `[A-Za-z0-9_-]+` に一致しなければならない。

### REQ-CRD-021 [Ubiquitous][MAY]
Card name は `-` を含んでもよい。

### REQ-CRD-022 [Unwanted behavior][MUST]
同一 Deck 内で選択された複数 Card が同一 `name` を持つ場合、Dembly は validation error にしなければならない。

---

## 11.3 Card mount target

### REQ-CRD-030 [Ubiquitous][MUST]
`mount.target` は absolute Runtime path でなければならない。

### REQ-CRD-031 [Ubiquitous][MUST NOT]
`mount.target` は `..` を含んではならない。

### REQ-CRD-032 [Ubiquitous][MUST NOT]
`mount.target` は variable expansion を使用してはならない。

### REQ-CRD-033 [Unwanted behavior][MUST]
複数 Card が同一 `mount.target` を使用する場合、Dembly は validation error にしなければならない。

推奨 convention:

```text
/opt/dembly/cards/<card-name>
```

---

## 11.4 Checksum

### REQ-CRD-040 [Event-driven][MUST]
Runtime を生成する前に、Dembly は `rootfs.squashfs` の SHA-256 を計算し、`filesystem.sha256` と一致することを確認しなければならない。

### REQ-CRD-041 [Unwanted behavior][MUST]
SquashFS SHA-256 が `filesystem.sha256` と一致しない場合、Dembly は Runtime を生成してはならない。

---

# 12. Deck manifest

## 12.1 Image Base

例:

```toml
schema_version = 1
name = "emcos-dev"

[base]
image = "ubuntu:24.04"

[[cards]]
path = "cards/emcos-sdk/card.toml"

[[cards]]
path = "cards/clang/card.toml"

[[cards]]
path = "cards/tis/card.toml"

[environment]
PROJECT_MODE = "development"

[environment_path]
prepend = ["/project/bin"]

[[volumes]]
name = "build"
target = "/workspace/build"
shared = false

[[binds]]
source = "."
target = "/workspace"
mode = "rw"
required = true
```

## 12.2 Compose Base

例:

```toml
schema_version = 1
name = "emcos-dev"

[base]
compose = "compose.yaml"
service = "dev"

[[cards]]
path = "cards/emcos-sdk/card.toml"

[[cards]]
path = "cards/clang/card.toml"

[[cards]]
path = "cards/tis/card.toml"
```

### REQ-DCK-010 [Ubiquitous][MUST]
`deck.toml` 内の relative path は Deck root を基準に解決しなければならない。

対象:

- `base.compose`
- `cards[].path`
- relative `binds[].source`

### REQ-DCK-011 [Ubiquitous][MUST]
`base.image` と `base.compose` は mutually exclusive でなければならない。

### REQ-DCK-012 [Unwanted behavior][MUST]
`base.image` と `base.compose` が同時に指定された場合、Dembly は validation error にしなければならない。

### REQ-DCK-013 [Optional feature][MUST]
Compose Base を使用する場合、`base.service` を必須としなければならない。

---

# 13. Compose Base

## 13.1 Semantics

`compose.yaml` 全体を Base filesystem とは扱わない。Compose project 内の 1 service を Dembly Runtime service とする。

例:

```yaml
services:
  dev:
    build: .
  database:
    image: postgres:18
```

```toml
[base]
compose = "compose.yaml"
service = "dev"
```

### REQ-CMP-001 [Optional feature][MUST]
Compose Base を使用する場合、Dembly は `base.service` で指定された service のみを Dembly Runtime 化しなければならない。

### REQ-CMP-002 [Optional feature][MUST]
Compose Base を使用する場合、指定されていない他 service は通常の Compose service として維持しなければならない。

---

## 13.2 Generated override

### REQ-CMP-010 [Ubiquitous][MUST NOT]
Dembly は user-authored `compose.yaml` を書き換えてはならない。

### REQ-CMP-011 [Optional feature][MUST]
Compose Base を使用する場合、Dembly は Dembly 用の ephemeral Compose override file を生成しなければならない。

override には必要に応じて以下のみを追加する。

- Dembly binary read-only bind
- Card SquashFS read-only file binds
- Deck/Card Volume binds
- Deck/Card Host binds
- runtime configuration bind
- PoC privileged mode
- Dembly internal entrypoint
- Runtime initialization 用 root user
- Dembly labels

概念コマンド:

```bash
docker compose \
  -p dembly-<deck-name> \
  -f <original-compose.yaml> \
  -f <generated-override.yaml> \
  up ...
```

---

## 13.3 Compose lifecycle

### REQ-CMP-020 [Event-driven][MUST]
Compose Base に対して `dembly up` を実行したとき、Dembly は Compose project 全体を起動しなければならない。

### REQ-CMP-021 [Event-driven][MUST]
Compose Base に対して `dembly down` を実行したとき、Dembly は Dembly が所有する Compose project を down しなければならない。

### REQ-CMP-022 [Ubiquitous][MUST NOT]
`dembly down` は Deck root 配下の Volume data を削除してはならない。

### REQ-CMP-023 [Optional feature][MAY]
選択 service が `build:` を使用する場合、Dembly は必要に応じて Docker Compose build を発生させてもよい。

### REQ-CMP-024 [Ubiquitous][MUST NOT]
Card 選択変更のみを理由として、Dembly は Base の rebuild を要求してはならない。

---

## 13.4 Original process preservation

### REQ-CMP-030 [Event-driven][MUST]
Compose Runtime を生成する前に、Dembly は選択 service の effective original entrypoint と effective command を解決しなければならない。

### REQ-CMP-031 [Event-driven][MUST]
Dembly は、解決した original argv を `runtime.toml` に保存しなければならない。

### REQ-CMP-032 [Optional feature][MUST]
Compose Base で `dembly up` を実行する場合、Dembly は selected service の effective entrypoint を Dembly internal entrypoint に置換しなければならない。

Internal entrypoint:

```text
/run/dembly/bin/dembly __runtime init /run/dembly/runtime.toml
```

### REQ-CMP-033 [Ubiquitous][MUST]
generated override は original command が Dembly internal entrypoint に重複 append されないようにしなければならない。

### REQ-CMP-034 [Event-driven][MUST]
Runtime initialization 完了後、Dembly は保存済み original process を `exec()` しなければならない。

---

# 14. Runtime identity

### REQ-ID-001 [Ubiquitous][MUST]
Deck name `X` に対する human-readable Runtime identity は `dembly-X` としなければならない。

Image Base:

```text
Container name = dembly-<deck-name>
```

Compose Base:

```text
Compose project name = dembly-<deck-name>
```

### REQ-ID-002 [Ubiquitous][MUST]
Dembly は Runtime ownership の authoritative identification に Docker label を使用しなければならない。

最低限:

```text
io.dembly.managed=true
io.dembly.deck=<deck-name>
io.dembly.schema=1
```

### REQ-ID-003 [Unwanted behavior][MUST]
同一 Deck の managed Runtime がすでに存在する状態で `dembly up` が実行された場合、Dembly は error を返し、暗黙 replace してはならない。

---

# 15. Runtime temporary state

Dembly は以下の ephemeral Host-side metadata を必要とする。

- `runtime.toml`
- generated Compose override

これは許可する。禁止対象は Host-side **SquashFS filesystem mount** である。

### REQ-RTS-001 [Event-driven][MUST]
Runtime temporary state を生成するとき、Dembly は `${XDG_RUNTIME_DIR}/dembly/...` を利用可能なら優先して使用しなければならない。

### REQ-RTS-002 [Unwanted behavior][MUST]
`${XDG_RUNTIME_DIR}` が利用できない場合、Dembly は user と Deck に scope された OS temporary directory を使用しなければならない。

### REQ-RTS-003 [Ubiquitous][MUST]
Runtime temporary directory は restrictive permission で作成しなければならない。

### REQ-RTS-004 [Ubiquitous][MUST NOT]
Runtime metadata に Host bind source の secret file 内容をコピーしてはならない。

### REQ-RTS-005 [Event-driven][MUST]
正常な `dembly down` を実行したとき、Dembly は当該 Runtime の ephemeral metadata を削除しなければならない。

### REQ-RTS-006 [Ubiquitous][MUST NOT]
Runtime temporary state を Deck `volumes/` 配下へ保存してはならない。

---

# 16. Runtime user resolution

解決優先順位:

1. Compose service `user`
2. Image `Config.User`
3. `root`

解決結果:

```text
USER
UID
GID
HOME
```

### REQ-USR-001 [Event-driven][MUST]
Runtime を生成する前に、Dembly は intended Runtime user を上記優先順位で解決しなければならない。

### REQ-USR-002 [Event-driven][MUST]
intended Runtime user を決定したとき、Dembly は Base filesystem 内の user database を使用して USER / UID / GID / HOME を解決しなければならない。

### REQ-USR-003 [Ubiquitous][SHOULD]
user resolution probe は shell や `getent` が Base に存在することを前提にせず、static `dembly` executable の internal probe command を使用することを推奨する。

推奨方式:

1. effective Base image から lightweight temporary container を作成
2. Host の同一 static `dembly` binary を read-only bind
3. internal probe command を実行
4. Base container の user database を Dembly 自身で読み、USER / UID / GID / HOME を返す
5. probe container を削除

### REQ-USR-004 [Unwanted behavior][MUST]
configured user / UID を必要情報へ解決できない場合、Dembly は actionable error を返さなければならない。

### REQ-USR-005 [State-driven][MUST]
Runtime initialization 中、Dembly internal initializer は PoC では root として動作しなければならない。

### REQ-USR-006 [Event-driven][MUST]
Card setup 完了後に final application process を実行するとき、Dembly は intended UID/GID へ privilege drop しなければならない。

### REQ-USR-007 [Event-driven][MUST]
`dembly exec` を実行するとき、Dembly は intended Runtime user として requested command を実行しなければならない。

---

# 17. Host Bind variable expansion

## 17.1 Source variables

source で使用可能:

```text
${HOST_HOME}
${DECK_ROOT}
```

### REQ-BND-001 [Ubiquitous][MUST]
`${HOST_HOME}` は Dembly を実行した Host user の HOME directory を表さなければならない。

### REQ-BND-002 [Ubiquitous][MUST]
`${DECK_ROOT}` は解決済み Deck root absolute path を表さなければならない。

## 17.2 Target variables

target で使用可能:

```text
${USER}
${HOME}
```

### REQ-BND-003 [Ubiquitous][MUST]
`${USER}` は intended Runtime username を表さなければならない。

### REQ-BND-004 [Ubiquitous][MUST]
`${HOME}` は intended Runtime user's HOME directory を表さなければならない。

## 17.3 Syntax

### REQ-BND-005 [Ubiquitous][MUST]
PoC の variable syntax は `${VARIABLE}` のみをサポートしなければならない。

以下はサポートしない。

```text
$VARIABLE
${VARIABLE:-default}
${VARIABLE/foo/bar}
```

### REQ-BND-006 [Unwanted behavior][MUST]
未定義 variable が指定された場合、Dembly は空文字列へ展開せず validation error にしなければならない。

### REQ-BND-007 [Ubiquitous][MUST NOT]
Dembly は任意 Host environment variable を暗黙 import して展開してはならない。

---

# 18. Host Bind semantics

例:

```toml
[[binds]]
source = "${HOST_HOME}/.codex/auth.json"
target = "${HOME}/.codex/auth.json"
mode = "ro"
required = true
```

### REQ-BND-010 [Ubiquitous][MUST]
`binds[].mode` は `ro` または `rw` のいずれかを明示指定しなければならない。

### REQ-BND-011 [Ubiquitous][MUST]
`required` 省略時は `true` と解釈しなければならない。

### REQ-BND-012 [Ubiquitous][MAY]
Host Bind source は file または directory のどちらでもよい。

### REQ-BND-013 [Ubiquitous][MAY]
Host Bind source は symlink でもよい。

### REQ-BND-014 [Ubiquitous][MUST]
relative `binds[].source` は、Deck 宣言か Card 宣言かに関係なく Deck root 基準で解決しなければならない。

### REQ-BND-015 [Ubiquitous][MUST]
`binds[].target` は variable 展開後に absolute Runtime path でなければならない。

### REQ-BND-016 [Unwanted behavior][MUST]
`required = true` の source が存在しない場合、Dembly は fatal error にしなければならない。

### REQ-BND-017 [Unwanted behavior][MUST]
`required = false` の source が存在しない場合、Dembly は warning を表示し、その bind を skip しなければならない。

### REQ-BND-018 [Event-driven][MUST]
`dembly inspect` を実行したとき、Dembly は Host Bind の declared source と、canonicalize が成功した場合の canonical source を表示しなければならない。

### REQ-BND-019 [Unwanted behavior][MUST]
Host Bind target が他の mount resource の exact target と衝突する場合、Dembly は validation error にしなければならない。

Card Host Bind は trusted Card の機能として PoC で許可する。

典型例:

```toml
[[binds]]
source = "${HOST_HOME}/.codex/auth.json"
target = "${HOME}/.codex/auth.json"
mode = "ro"
required = true
```

---

# 19. Volume

Dembly Volume は Docker named volume ではなく、Deck root 配下の通常 directory を Runtime へ RW bind mount する。

## 19.1 Volume name

### REQ-VOL-001 [Ubiquitous][MUST]
`volumes[].name` は `[A-Za-z0-9_]+` に一致しなければならない。

### REQ-VOL-002 [Ubiquitous][MUST NOT]
`volumes[].name` は `-` を含んではならない。

Card name は `-` を含んでもよい。

---

## 19.2 Physical layout

`shared = false` を default とする。

### Deck private volume

```text
<deck-root>/volumes/<volume-name>/
```

### Card private volume

```text
<deck-root>/volumes/<card-name>/<volume-name>/
```

### Shared volume

Deck / Card どちらから宣言しても:

```text
<deck-root>/volumes/<volume-name>/
```

### REQ-VOL-010 [Ubiquitous][MUST]
`shared` が省略された場合、Dembly は `shared = false` と解釈しなければならない。

### REQ-VOL-011 [Event-driven][MUST]
Deck で `shared = false` の Volume を解決するとき、Dembly は physical path を `<deck-root>/volumes/<volume-name>/` としなければならない。

### REQ-VOL-012 [Event-driven][MUST]
Card で `shared = false` の Volume を解決するとき、Dembly は physical path を `<deck-root>/volumes/<card-name>/<volume-name>/` としなければならない。

### REQ-VOL-013 [Event-driven][MUST]
Deck または Card で `shared = true` の Volume を解決するとき、Dembly は physical path を `<deck-root>/volumes/<volume-name>/` としなければならない。

例:

```text
volumes/
├── build/                 # Deck private
├── cache/                 # shared
├── tis/
│   └── database/          # tis private
└── emcos-sdk/
    └── cache/             # emcos-sdk private
```

---

## 19.3 Shared/private consistency

### REQ-VOL-020 [Unwanted behavior][MUST]
同一 resolved Deck 内で同じ `volume.name` に `shared = true` と `shared = false` が混在する場合、Dembly は validation error にしなければならない。

### REQ-VOL-021 [Event-driven][MUST]
同じ `volume.name` を持つ複数宣言がすべて `shared = true` の場合、Dembly はそれらを同一 physical directory `<deck-root>/volumes/<volume-name>/` へ解決しなければならない。

### REQ-VOL-022 [Ubiquitous][MAY]
同一 shared Volume は複数の異なる Runtime target へ mount してよい。

---

## 19.4 Lifecycle

### REQ-VOL-030 [Event-driven][MUST]
`up`、`run`、または `check` で必要な Volume directory が存在しない場合、Dembly は自動作成しなければならない。

### REQ-VOL-031 [Event-driven][MUST]
必要な Volume directory が既に存在する場合、Dembly は内容を保持したまま再利用しなければならない。

### REQ-VOL-032 [Event-driven][MUST NOT]
`dembly down` を実行したとき、Dembly は Volume directory を削除してはならない。

### REQ-VOL-033 [Unwanted behavior][MUST]
resolved Volume physical path 自体が symlink の場合、Dembly は validation error にしなければならない。

### REQ-VOL-034 [Event-driven][MUST]
Volume directory 作成時、Dembly は必要な parent directory も作成しなければならない。

---

# 20. Mount collision

mount resource:

- Card mount target
- Deck Volume target
- Card Volume target
- Deck Host Bind target
- Card Host Bind target

### REQ-MNT-001 [Unwanted behavior][MUST]
異なる mount resource が同一 exact Runtime target を使用する場合、Dembly は validation error にしなければならない。

### REQ-MNT-002 [Ubiquitous][MAY]
nested mount target は許可してよい。

### REQ-MNT-003 [Event-driven][MUST]
Runtime mount/setup を行うとき、Dembly は以下の順序を使用しなければならない。

1. Card SquashFS mount
2. Volume
3. Host Bind
4. Export

---

# 21. Environment merge

例:

```toml
[environment]
FOO = "bar"

[environment_path]
prepend = ["bin", "tools/bin"]
```

### REQ-ENV-001 [Event-driven][MUST]
normal environment を生成するとき、Dembly は以下の precedence を使用しなければならない。

```text
Base image environment
    ↓
Compose service environment
    ↓
deck.toml environment
    ↓
Card environment
```

### REQ-ENV-002 [Unwanted behavior][MUST]
2 つ以上の selected Card が同じ normal environment variable 名を定義する場合、Dembly は validation error にしなければならない。

### REQ-ENV-003 [Ubiquitous][MUST NOT]
Card-to-Card environment conflict を last-wins で暗黙解決してはならない。

### REQ-ENV-004 [Ubiquitous][MAY]
Card environment は Base / Compose / Deck の同名 environment variable を override してよい。

## 21.1 PATH

### REQ-ENV-010 [Event-driven][MUST]
PATH を生成するとき、Dembly は以下の順序で prepend しなければならない。

```text
<Card 1 prepend>
:<Card 2 prepend>
:...
:<Deck prepend>
:<effective Base/Compose PATH>
```

### REQ-ENV-011 [Ubiquitous][MUST]
Card の relative PATH entry は当該 Card mount root 基準に解決しなければならない。

### REQ-ENV-012 [Ubiquitous][MUST]
Card order は `deck.toml` に記載された順序を使用しなければならない。

---

# 22. Exports

例:

```toml
[[exports]]
source = "bin/clang"
target = "/usr/local/bin/clang"
```

### REQ-EXP-001 [Ubiquitous][MUST]
`exports[].source` は Card mount root 基準の relative path としなければならない。

### REQ-EXP-002 [Ubiquitous][MUST NOT]
`exports[].source` は `..` を含んではならない。

### REQ-EXP-003 [Ubiquitous][MUST]
`exports[].target` は absolute Runtime path でなければならない。

### REQ-EXP-004 [Ubiquitous][MUST]
PoC では export target を `/usr/local/bin/` 配下に限定しなければならない。

### REQ-EXP-005 [Event-driven][MUST]
Export を生成するとき、Dembly は次の symbolic link を生成しなければならない。

```text
<target> -> <card-mount-root>/<source>
```

### REQ-EXP-006 [Unwanted behavior][MUST]
複数 export が同一 target を使用する場合、Dembly は validation error にしなければならない。

### REQ-EXP-007 [Unwanted behavior][MUST]
export target に既存の non-Dembly file が存在する場合、Dembly は上書きせず error にしなければならない。

---

# 23. Card hook

PoC がサポートする lifecycle hook は `post_mount` のみとする。

例:

```toml
[[hooks.post_mount]]
exec = "setup/post-mount.sh"
args = ["--foo", "bar"]
```

### REQ-HOK-001 [Ubiquitous][MUST]
PoC は `post_mount` hook をサポートしなければならない。

### REQ-HOK-002 [Ubiquitous][MUST NOT]
PoC は lifecycle を一般化する目的だけで不要な `pre_mount` / `pre_start` / `post_start` 等を必須実装してはならない。

### REQ-HOK-003 [Ubiquitous][MUST]
`exec` は Card mount root 基準の relative executable path としなければならない。

### REQ-HOK-004 [Ubiquitous][MUST NOT]
`exec` は `..` を含んではならない。

### REQ-HOK-005 [Ubiquitous][MUST]
`args` は argv array として扱わなければならない。

### REQ-HOK-006 [Ubiquitous][MUST NOT]
hook を任意 shell command string として表現してはならない。

### REQ-HOK-007 [Event-driven][MUST]
複数 Card の hook を実行するとき、Dembly は Deck Card order で実行しなければならない。

### REQ-HOK-008 [Event-driven][MUST]
1 Card 内の複数 hook は manifest declaration order で実行しなければならない。

### REQ-HOK-009 [State-driven][MUST]
hook 実行中の current working directory は Card mount root でなければならない。

### REQ-HOK-010 [State-driven][MUST]
hook は final merged Runtime environment を継承しなければならない。

### REQ-HOK-011 [State-driven][MUST]
PoC の `post_mount` hook は root で実行しなければならない。

### REQ-HOK-012 [Unwanted behavior][MUST]
hook が non-zero で終了した場合、Dembly は Runtime normal startup を失敗させなければならない。

### REQ-HOK-013 [Ubiquitous][MAY]
PoC の hook には timeout を設けなくてもよい。

---

# 24. Card check

例:

```toml
[check]
exec = "bin/clang"
args = ["--version"]
```

### REQ-CHK-001 [Ubiquitous][MAY]
Card は `[check]` を持ってよい。

### REQ-CHK-002 [Ubiquitous][MUST]
`check.exec` は Card mount root 基準の relative executable path としなければならない。

### REQ-CHK-003 [Event-driven][MUST NOT]
`dembly validate` は Card check を実行してはならない。

### REQ-CHK-004 [Event-driven][MUST]
`dembly check` を実行したとき、Dembly は selected Card の check を Deck Card order で実行しなければならない。

### REQ-CHK-005 [Unwanted behavior][MUST]
1 つ以上の Card check が失敗した場合、`dembly check` は non-zero で終了しなければならない。

### REQ-CHK-006 [Event-driven][SHOULD]
複数 Card を check するとき、Dembly は可能な限り全 Card の結果を表示した後に non-zero を返すことを推奨する。

初期実 Card acceptance:

#### clang

- `clang --version`
- 最小 C source の compile
- 可能なら link / run

#### tis

- TIS version/basic command
- 最小 representative analysis input を 1 件解析

#### emcos-sdk

- representative SDK command または既知 file access
- 可能なら最小 eMCOS build

---

# 25. Kernel SquashFS mount

## 25.1 Host behavior

### REQ-SQF-001 [Ubiquitous][MUST]
Host は Card SquashFS を通常 file として保持しなければならない。

### REQ-SQF-002 [Ubiquitous][MUST NOT]
Dembly は Host 側で SquashFS を mount してはならない。

禁止:

```text
Host mount SquashFS
    ↓
Host mount point
    ↓
bind mounted directory
```

要求:

```text
Host .squashfs file
    ↓ read-only file bind
Runtime
    ↓ kernel SquashFS mount
Card mount target
```

## 25.2 Runtime transport

### REQ-SQF-010 [Event-driven][MUST]
Runtime を生成するとき、Dembly は各 selected Card SquashFS file を Runtime の以下へ read-only bind しなければならない。

```text
/run/dembly/cards/<card-name>.squashfs
```

## 25.3 PoC mount implementation

### REQ-SQF-020 [State-driven][MUST]
PoC Runtime は Card mount feasibility 検証のため privileged mode で起動しなければならない。

### REQ-SQF-021 [Event-driven][MUST]
Card を mount するとき、Runtime 内の `dembly __runtime init` は Runtime の external `mount` command を使用しなければならない。

概念コマンド:

```bash
mount -t squashfs -o loop,ro \
  /run/dembly/cards/<card-name>.squashfs \
  <card.mount.target>
```

### REQ-SQF-022 [Ubiquitous][MUST NOT]
PoC では Rust code から `mount(2)` や loop-device ioctl を直接実装することを必須としてはならない。

### REQ-SQF-023 [Unwanted behavior][MUST]
SquashFS mount に失敗した場合、Dembly は Runtime startup を失敗させなければならない。

### REQ-SQF-024 [State-driven][MUST]
Card filesystem は Runtime 内で read-only mount されていなければならない。

### REQ-SQF-025 [Event-driven][MUST]
Integration test では、Runtime 実行中に Card mount が Host mount namespace に存在しないことを確認しなければならない。

---

# 26. Runtime initialization

`runtime-init` は別 script でも別 executable でもない。

実体:

```bash
/run/dembly/bin/dembly __runtime init /run/dembly/runtime.toml
```

### REQ-RUN-001 [Ubiquitous][MUST]
Runtime initialization は Host CLI と同一の Rust `dembly` executable の internal subcommand として実装しなければならない。

### REQ-RUN-002 [Ubiquitous][MUST NOT]
Runtime initialization を独立した shell script として実装してはならない。

## 26.1 Initialization sequence

### REQ-RUN-010 [Event-driven][MUST]
`dembly __runtime init` が起動したとき、Dembly は次の順序で処理しなければならない。

1. `runtime.toml` read / validate
2. privileged initializer user であることを確認
3. Card mount directory 作成
4. Card SquashFS mount
5. Docker が供給済みの Volume / Host Bind target を確認
6. Export 作成
7. final environment 構築
8. Card `post_mount` hook 実行
9. final process 選択
10. intended UID/GID へ privilege drop
11. final process を `exec()`

### REQ-RUN-011 [Event-driven][MUST]
final process を起動するとき、Dembly は unmanaged child process として spawn せず `exec()` semantics を使用しなければならない。

---

## 26.2 `up` final process

### REQ-RUN-020 [Event-driven][MUST]
`dembly up` で Runtime initialization が完了したとき、Dembly は保存済み effective original Base / Compose process を final process として実行しなければならない。

---

## 26.3 `run` final process

### REQ-RUN-030 [Event-driven][MUST]
`dembly run -- <command...>` を実行したとき、Dembly は `<command...>` を final process argv として使用しなければならない。

### REQ-RUN-031 [Event-driven][MUST]
`dembly run` の requested command は Base/Compose original entrypoint と command を置換しなければならない。

### REQ-RUN-032 [Event-driven][MUST]
`dembly run` の command 完了後、Dembly は temporary Runtime を削除しなければならない。

### REQ-RUN-033 [Event-driven][MUST NOT]
`dembly run` の command 完了後、Dembly は persistent Volume directory を削除してはならない。

### REQ-RUN-034 [Event-driven][MUST]
`dembly run` は final command の exit status を caller へ返さなければならない。

---

# 27. runtime.toml

`runtime.toml` は user-authored file ではない。Host-side Dembly が生成する internal configuration である。

概念例:

```toml
schema_version = 1
deck_name = "emcos-dev"

[runtime_user]
name = "developer"
uid = 1000
gid = 1000
home = "/home/developer"

[[cards]]
name = "clang"
image = "/run/dembly/cards/clang.squashfs"
mount_target = "/opt/dembly/cards/clang"

[[exports]]
source = "/opt/dembly/cards/clang/bin/clang"
target = "/usr/local/bin/clang"

[[hooks]]
card = "clang"
exec = "/opt/dembly/cards/clang/setup/post-mount.sh"
args = []

[environment]
FOO = "bar"
PATH = "/opt/dembly/cards/clang/bin:/usr/local/sbin:..."

[process]
argv = ["/original/entrypoint", "arg"]
```

### REQ-RTC-001 [Event-driven][MUST]
Runtime を生成するとき、Host-side Dembly は internal `runtime.toml` を生成しなければならない。

### REQ-RTC-002 [Ubiquitous][MUST]
`runtime.toml` は versioned schema を持たなければならない。

### REQ-RTC-003 [Ubiquitous][MUST]
`runtime.toml` は Runtime initialization に必要な resolved absolute Runtime data を保持しなければならない。

### REQ-RTC-004 [Ubiquitous][MUST NOT]
Runtime-side Dembly は `deck.toml` や Card manifest を再解釈することを前提としてはならない。

### REQ-RTC-005 [Ubiquitous][MUST NOT]
`runtime.toml` に unresolved `${...}` variable を残してはならない。

---

# 28. deck.lock

役割:

```text
deck.toml = desired configuration
deck.lock = resolved immutable artifacts
volumes/  = mutable persistent state
```

## 28.1 `dembly lock`

### REQ-LCK-001 [Event-driven][MUST]
`dembly lock` を実行したとき、Dembly は current Deck を resolve し `deck.lock` を生成または更新しなければならない。

### REQ-LCK-002 [Ubiquitous][MUST]
`dembly lock` は通常の Deck discovery rule を使用しなければならない。

## 28.2 Contents

最低限:

- lock schema version
- Base source/reference
- resolved immutable Base image identity
- selected Card name
- Card version
- Card manifest SHA-256
- Card SquashFS SHA-256
- local Card source path（Deck root relative にできる場合）

Image Base 例:

```toml
schema_version = 1

[base]
kind = "image"
reference = "ubuntu:24.04"
resolved_image_id = "sha256:..."

[[cards]]
name = "clang"
version = "20.1.0"
source = "cards/clang/card.toml"
manifest_sha256 = "sha256:..."
filesystem_sha256 = "sha256:..."
```

Compose Base では追加で以下を記録する。

- Compose file path
- selected service
- Compose file SHA-256
- selected Dembly service の resolved image identity

### REQ-LCK-010 [Event-driven][MUST]
Image Base を lock するとき、Dembly は immutable Docker image identity を解決し記録しなければならない。

### REQ-LCK-011 [Event-driven][MUST]
Compose Base を lock するとき、Dembly は selected service の effective image identity を解決し記録しなければならない。

### REQ-LCK-012 [Event-driven][MUST]
Compose Base を lock するとき、Dembly は Compose file SHA-256 と selected service name を記録しなければならない。

### REQ-LCK-013 [Ubiquitous][MAY]
Base image が local に存在しない場合、`dembly lock` は必要に応じて Docker pull を行ってよい。

### REQ-LCK-014 [Optional feature][MAY]
Compose selected service が `build:` を使用する場合、`dembly lock` は必要に応じて build を実行して image identity を確定してよい。

## 28.3 Enforcement

### REQ-LCK-020 [Event-driven][MUST]
`up`、`run`、`check` を実行するとき、Dembly は valid `deck.lock` の存在を要求しなければならない。

### REQ-LCK-021 [Unwanted behavior][MUST]
`deck.lock` が存在しない場合、`up`、`run`、`check` は失敗し、user に `dembly lock` 実行を案内しなければならない。

### REQ-LCK-022 [Unwanted behavior][MUST]
以下のいずれかが lock と異なる場合、`up`、`run`、`check` は stale lock error にしなければならない。

- selected Card set
- Card version
- Card manifest SHA-256
- Card filesystem SHA-256
- Base source/reference
- resolved Base image identity
- Compose file SHA-256
- selected Compose service

### REQ-LCK-023 [Ubiquitous][MUST NOT]
`up`、`run`、`check` は `deck.lock` を暗黙更新してはならない。

---

# 29. Snapshot semantics

Deck directory 例:

```text
emcos-dev/
├── deck.toml
├── deck.lock
├── cards/
│   ├── emcos-sdk/
│   │   └── card.toml
│   ├── clang/
│   │   └── card.toml
│   └── tis/
│       └── card.toml
├── compose.yaml
├── Dockerfile
└── volumes/
    ├── build/
    ├── cache/
    └── tis/
        └── database/
```

Card SquashFS は PoC では Deck 内に置いてもよいし、外部 path を参照してもよい。将来は Dembly Repository / local cache から resolve してよい。

Docker Base image も Docker Hub 等の外部 registry に依存してよい。

Snapshot model:

```text
Mutable:
  volumes/* を byte として保持

Immutable:
  Base / Card を deck.lock の immutable identity で固定
```

### REQ-SNP-001 [Ubiquitous][MUST]
Dembly の snapshot semantics は、immutable dependency の全 byte を Deck root 内へ必ず格納することを要求してはならない。

### REQ-SNP-002 [Ubiquitous][MUST]
Deck snapshot の再現性は、mutable state の実データと immutable dependency identity の組み合わせとして扱わなければならない。

### REQ-SNP-003 [Ubiquitous][MUST]
Host Bind で Deck root 外を参照する file / directory は snapshot 対象外の external dependency として扱わなければならない。

### REQ-SNP-004 [Ubiquitous][MUST NOT]
PoC は Runtime 稼働中の application-consistent snapshot を保証してはならない。

---

# 30. `dembly card build`

## 30.1 CLI

Primary form:

```bash
dembly card build <path/to/tool> <path/to/cards>
```

例:

```bash
dembly card build /opt/clang ./cards
```

Output:

```text
<path/to/cards>/<card-name>/
├── card.toml
└── rootfs.squashfs
```

### REQ-CBL-001 [Event-driven][MUST]
`dembly card build <tool-root> <cards-root>` を実行したとき、Dembly は `<tool-root>` の内容を直接 SquashFS 化しなければならない。

### REQ-CBL-002 [Ubiquitous][MUST NOT]
Dembly は Card build のためだけに `<tool-root>` 全体を `<cards-root>` 配下へ staging copy してはならない。

### REQ-CBL-003 [Event-driven][MUST]
Card build が成功したとき、Dembly は `<cards-root>/<card-name>/card.toml` と `<cards-root>/<card-name>/rootfs.squashfs` を生成しなければならない。

## 30.2 Interactive mode

### REQ-CBL-010 [State-driven][MUST]
`--non-interactive` が指定されていない場合、Dembly は `npm init` に類する対話形式で Card manifest 情報を stdin から取得できなければならない。

推奨 prompt:

```text
Card name [clang]:
Version:
Mount target [/opt/dembly/cards/clang]:
Add PATH entry? [Y/n]:
PATH relative path [bin]:
Add export? [y/N]:
Add volume? [y/N]:
Add host bind? [y/N]:
Add post-mount hook? [y/N]:
Configure check? [Y/n]:
```

### REQ-CBL-011 [Event-driven][MUST]
Card name の default は `basename(<tool-root>)` でなければならない。

### REQ-CBL-012 [Ubiquitous][MUST NOT]
filesystem SHA-256 等の Dembly が計算可能な値を user に手入力させてはならない。

## 30.3 Non-interactive mode

例:

```bash
dembly card build /opt/clang ./cards \
  --name clang \
  --version 20.1.0 \
  --mount-target /opt/dembly/cards/clang \
  --path-prepend bin \
  --non-interactive
```

### REQ-CBL-020 [Ubiquitous][MUST]
interactive mode で入力可能な必須情報は automation 用 CLI option でも指定できなければならない。

### REQ-CBL-021 [Unwanted behavior][MUST]
`--non-interactive` 指定時に必須情報が不足している場合、Dembly は prompt を出さず error にしなければならない。

## 30.4 SquashFS production

### REQ-CBL-030 [Event-driven][MUST]
Card filesystem を build するとき、Dembly は `mksquashfs` を使用しなければならない。

### REQ-CBL-031 [Ubiquitous][MUST]
Dembly は append ではなく新規 SquashFS を生成しなければならない。

### REQ-CBL-032 [Ubiquitous][SHOULD]
compression option は high-throughput development workload に適した設定を選び、README に明記することを推奨する。

### REQ-CBL-033 [Event-driven][MUST]
SquashFS 生成成功後、Dembly は SHA-256 を計算しなければならない。

### REQ-CBL-034 [Event-driven][MUST]
Dembly は temporary output を作成し、完成後に atomic rename することで、途中失敗した `rootfs.squashfs` を valid artifact に見せないようにしなければならない。

### REQ-CBL-035 [Ubiquitous][MUST NOT]
Dembly は Card build source directory を変更してはならない。

### REQ-CBL-036 [Ubiquitous][MAY]
source directory 内に symlink が存在してもよい。

### REQ-CBL-037 [Ubiquitous][MUST NOT]
source directory 内 symlink が Card 外を指す場合、Dembly はその target 内容を自動取り込みして self-contained 化してはならない。

---

# 31. CLI

Minimum PoC CLI:

```text
dembly
├── validate [deck.toml]
├── lock [deck.toml]
├── up [deck.toml]
├── down [deck.toml]
├── run [deck.toml] -- <command...>
├── exec [deck.toml] -- <command...>
├── inspect [deck.toml]
├── check [deck.toml]
├── card
│   └── build <tool-root> <cards-root> [options]
└── __runtime
    ├── init <runtime.toml>
    └── <internal probe command(s)>
```

---

## 31.1 `validate`

### REQ-CLI-001 [Event-driven][MUST]
`dembly validate` を実行したとき、Dembly は final Runtime を生成せず Deck/Card configuration を検証しなければならない。

最低限の検証:

- Deck TOML schema
- Card TOML schema
- unknown field
- Base selection
- Card manifest existence
- SquashFS existence
- SquashFS SHA-256
- duplicate Card name
- Card mount target collision
- export collision
- exact mount target collision
- Volume naming
- shared/private consistency
- Volume symlink rejection
- required Host Bind existence
- bind mode
- variable syntax
- Host-side variable resolution
- local path normalization
- Card-to-Card environment conflict
- lock consistency（lock が存在する場合）
- `${USER}` / `${HOME}` が必要な場合の Runtime user resolve

### REQ-CLI-002 [Event-driven][MUST NOT]
`dembly validate` は Card `[check]` を実行してはならない。

---

## 31.2 `lock`

### REQ-CLI-010 [Event-driven][MUST]
`dembly lock` を実行したとき、Dembly は immutable dependency を resolve し `deck.lock` を明示的に生成/更新しなければならない。

---

## 31.3 `up`

### REQ-CLI-020 [Event-driven][MUST]
`dembly up` を実行したとき、Dembly は以下を実行しなければならない。

1. Deck resolve
2. validation
3. valid lock requirement
4. Runtime user resolution
5. Volume directory 作成
6. runtime state 生成
7. persistent Runtime background start

### REQ-CLI-021 [Optional feature][MUST]
Image Base の場合、`dembly up` は Dembly-managed Docker container を 1 つ生成/起動しなければならない。

### REQ-CLI-022 [Optional feature][MUST]
Compose Base の場合、`dembly up` は Dembly-owned Compose project を起動しなければならない。

---

## 31.4 `down`

### REQ-CLI-030 [Event-driven][MUST]
`dembly down` を実行したとき、Dembly は Dembly label を使用して対象 Runtime ownership を確認しなければならない。

### REQ-CLI-031 [Optional feature][MUST]
Image Base の場合、`down` は managed Runtime を stop/remove しなければならない。

### REQ-CLI-032 [Optional feature][MUST]
Compose Base の場合、`down` は Dembly-owned Compose project を down しなければならない。

### REQ-CLI-033 [Event-driven][MUST]
`down` は ephemeral Dembly runtime metadata を削除しなければならない。

### REQ-CLI-034 [Ubiquitous][MUST NOT]
`down` は Deck Volume data や Card artifact を削除してはならない。

---

## 31.5 `run`

### REQ-CLI-040 [Event-driven][MUST]
`dembly run -- <command...>` を実行したとき、Dembly は full Deck を materialize した temporary Runtime 内で requested command を実行しなければならない。

### REQ-CLI-041 [Event-driven][MUST]
`run` は requested command の exit code を caller へ返さなければならない。

### REQ-CLI-042 [Event-driven][MUST]
`run` 完了後、temporary Runtime を削除しなければならない。

### REQ-CLI-043 [Ubiquitous][MUST NOT]
`run` 完了後、persistent Volume を削除してはならない。

---

## 31.6 `exec`

### REQ-CLI-050 [Event-driven][MUST]
`dembly exec -- <command...>` を実行したとき、Dembly は `up` 済み selected Dembly Runtime 内で command を実行しなければならない。

### REQ-CLI-051 [Unwanted behavior][MUST]
対象 Runtime が起動していない場合、`dembly exec` は error を返さなければならない。

### REQ-CLI-052 [Event-driven][MUST]
`dembly exec` は intended Runtime user として command を実行しなければならない。

---

## 31.7 `inspect`

### REQ-CLI-060 [Event-driven][MUST]
`dembly inspect` を実行したとき、Runtime が起動していなくても Deck configuration を表示できなければならない。

最低限表示:

- Deck name/root
- Base type/reference
- selected Compose service
- lock state
- Card names/versions
- Card artifact paths
- Card mount targets
- exports
- merged/planned PATH
- Volumes:
  - declaration owner
  - name
  - shared
  - physical Host path
  - Runtime target
- Host Binds:
  - owner
  - declared source
  - canonical source（可能なら）
  - target
  - ro/rw
  - required/optional
- running Runtime identity

---

## 31.8 `check`

### REQ-CLI-070 [Event-driven][MUST]
`dembly check` を実行したとき、Dembly は valid lock を要求しなければならない。

### REQ-CLI-071 [Event-driven][MUST]
`dembly check` は temporary Runtime を materialize し selected Card check を実行しなければならない。

### REQ-CLI-072 [Event-driven][MUST]
`check` 完了後、temporary Runtime を cleanup しなければならない。

### REQ-CLI-073 [Ubiquitous][MUST NOT]
`check` 完了後、persistent Volume を削除してはならない。

---

# 32. Image Base lifecycle

### REQ-IMG-001 [Optional feature][MUST]
Image Base を使用する場合、Dembly は Docker CLI / Engine operation で Runtime を直接構築しなければならない。

### REQ-IMG-002 [Event-driven][MUST]
Image Base Runtime を生成する前に、Dembly は Base image の original `Config.Entrypoint` と `Config.Cmd` を解決しなければならない。

### REQ-IMG-003 [Event-driven][MUST]
Image Base Runtime には少なくとも以下を設定しなければならない。

- Dembly binary RO bind
- selected Card SquashFS RO file binds
- Deck/Card Host Binds
- Deck/Card Volume binds
- PoC privileged mode
- runtime metadata
- Dembly labels
- Dembly internal entrypoint
- root initializer user

### REQ-IMG-004 [Event-driven][MUST]
Runtime initialization 完了後、application process は intended Base user として実行しなければならない。

---

# 33. Collision / validation summary

以下は fatal validation error とする。

1. duplicate Card name
2. duplicate Card mount target
3. different mount resources with exact same Runtime target
4. duplicate export target
5. export overwriting existing non-Dembly destination
6. same Volume name with mixed `shared=true` / `shared=false`
7. Volume name containing `-`
8. Volume physical path being symlink
9. missing required Host Bind
10. undefined bind variable
11. two Cards defining same normal environment variable
12. stale/missing lock where lock is required
13. invalid Base definition
14. missing Compose service
15. user resolution failure

### REQ-VAL-001 [Ubiquitous][MUST]
Dembly は上記 conflict を last-wins で暗黙解決してはならない。

### REQ-VAL-002 [Ubiquitous][MUST]
Dembly は mount/export plan を Runtime 生成前に可能な限り決定的に構築しなければならない。

---

# 34. Logging / error handling

### REQ-LOG-001 [Ubiquitous][MUST]
user-visible failure は non-zero exit status を返さなければならない。

### REQ-LOG-002 [Ubiquitous][MUST]
少なくとも以下を user が区別可能な error message にしなければならない。

- invalid Deck manifest
- invalid Card manifest
- unknown TOML field
- missing Card manifest
- missing SquashFS
- checksum mismatch
- missing/stale lock
- invalid Base
- missing Compose service
- Runtime user resolution failure
- mount failure
- export collision
- Volume conflict
- Host Bind error
- hook failure
- Card check failure
- Docker failure
- Runtime already exists
- Runtime not running

### REQ-LOG-003 [Ubiquitous][MUST]
通常出力は簡潔でなければならない。

### REQ-LOG-004 [Optional feature][MUST]
verbose/debug mode を使用する場合、Dembly は Docker command または同等の orchestration detail を安全に表示しなければならない。

### REQ-LOG-005 [Ubiquitous][MUST NOT]
Dembly は secret file 内容を log に出力してはならない。

---

# 35. Security assumptions

PoC では Card を trusted artifact とする。

- Card hook は root arbitrary code
- Card は Host Bind を要求可能
- Runtime は privileged
- Card sandboxing はしない
- per-bind approval UI は実装しない

### REQ-SEC-001 [Ubiquitous][MUST]
README は上記 security limitation を明示しなければならない。

### REQ-SEC-002 [Event-driven][MUST]
`dembly inspect` を実行したとき、Card が要求する Host Bind を明示しなければならない。

### REQ-SEC-003 [Ubiquitous][SHOULD]
将来の Card signing / trust / permission policy を阻害しない責務分離を維持することを推奨する。

---

# 36. Performance evaluation

比較対象:

```text
A. Tool を通常の Docker Image 内に直接配置
B. 同一 Tool を kernel SquashFS Card として供給
```

最低限測定:

- Runtime startup
- eMCOS clean build
- eMCOS incremental build
- Clang representative compile
- TIS representative analysis

可能なら:

- wall-clock
- CPU time
- filesystem I/O
- page faults
- context switches
- peak CPU utilization

### REQ-PER-001 [Event-driven][MUST]
Performance benchmark を実行するとき、各 workload を最低 5 回実行し median を比較しなければならない。

### REQ-PER-002 [Ubiquitous][MUST]
PoC の performance target は representative workload で Card filesystem overhead `<= 10%` としなければならない。

### REQ-PER-003 [Ubiquitous][MUST]
`<= 10%` は PoC target であり production product guarantee ではないことを文書化しなければならない。

---

# 37. Disk usage evaluation

従来構成の例:

```text
Base + emcos-sdk
Base + emcos-sdk + clang
Base + emcos-sdk + tis
Base + emcos-sdk + clang + tis
```

Dembly:

```text
Base
emcos-sdk Card
clang Card
tis Card
```

### REQ-DSK-001 [Event-driven][MUST]
Disk usage evaluation を行うとき、Dembly は conventional pre-composed image 群と Dembly artifact 群の storage requirement を比較可能な形で記録しなければならない。

使用候補:

```text
docker system df
docker image inspect
du
SquashFS file size
```

---

# 38. Unit test requirements

最低限:

```text
UT-001  Deck TOML parser
UT-002  Card TOML parser
UT-003  lock parser/writer
UT-004  unknown-field rejection
UT-005  Deck discovery
UT-006  relative path resolution
UT-007  variable expansion
UT-008  undefined-variable rejection
UT-009  Card name validation
UT-010  Volume name validation
UT-011  Volume physical path resolution
UT-012  shared/private consistency
UT-013  mount collision detection
UT-014  export collision detection
UT-015  environment conflict detection
UT-016  PATH planner
UT-017  checksum verification
UT-018  Docker argument/override planning where practical
```

### REQ-TST-001 [Ubiquitous][MUST]
Docker execution を伴わない domain logic は可能な限り unit test しなければならない。

### REQ-TST-002 [Ubiquitous][MAY]
Docker invocation は unit test では mock/fake してよい。

---

# 39. Integration test requirements

最低限:

```text
IT-001  Image Base + one Card starts
IT-002  Multiple Cards mount
IT-003  Executable runs directly from Card
IT-004  Shared library / mmap-backed executable path works
IT-005  Card symlink works
IT-006  Export symlink works
IT-007  PATH prepend works in Deck order
IT-008  post_mount hook executes
IT-009  hook failure prevents normal startup
IT-010  filesystem checksum mismatch is rejected
IT-011  duplicate Card name is rejected
IT-012  duplicate export target is rejected
IT-013  exact mount target collision is rejected
IT-014  private Deck Volume persists across recreation
IT-015  private Card Volume uses volumes/<card>/<name>
IT-016  shared Volume uses volumes/<name>
IT-017  shared=true/false same-name mixture is rejected
IT-018  Volume path symlink is rejected
IT-019  Host file bind RO works
IT-020  Host directory bind RW works
IT-021  ${HOST_HOME} resolves correctly
IT-022  ${DECK_ROOT} resolves correctly
IT-023  ${USER} resolves correctly
IT-024  ${HOME} resolves correctly
IT-025  required=false missing bind warns and skips
IT-026  required=true missing bind fails
IT-027  Card mount does not appear in Host mount namespace
IT-028  changing selected Cards requires no Base image rebuild
IT-029  same Host dembly executable runs inside Runtime
IT-030  static binary runs in selected Base
IT-031  non-root intended user runs final process as intended user
IT-032  dembly exec runs as intended user
IT-033  deck.lock is generated
IT-034  stale lock is rejected
IT-035  down removes Runtime and preserves Volumes
IT-036  Compose project starts with selected service Dembly-enabled
IT-037  non-selected Compose service remains functional
IT-038  original Compose selected-service process is preserved by up
IT-039  dembly run replaces original final process
IT-040  current-directory deck discovery does not search parents
```

### REQ-TST-010 [Ubiquitous][MUST]
Integration test は proprietary eMCOS/TIS artifact がなくても実行できる synthetic fixture Card を用意しなければならない。

### REQ-TST-011 [Unwanted behavior][MUST]
Docker/SquashFS prerequisite が current test environment にない場合、実装者は test を未実装にせず、実装済みだが environment blocked であることを明確に区別しなければならない。

### REQ-TST-012 [Ubiquitous][MUST NOT]
environment blocked の test を pass したと報告してはならない。

---

# 40. Fixture Card

推奨 tree:

```text
tests/fixtures/hello-card/rootfs/
├── bin/
│   └── hello
├── lib/
└── setup/
    └── post-mount.sh
```

### REQ-FIX-001 [Ubiquitous][MUST]
fixture Card は少なくとも以下を test 可能でなければならない。

- executable access
- symlink
- PATH
- hook
- Volume
- Host Bind
- Card check

### REQ-FIX-002 [Ubiquitous][SHOULD]
large binary artifact を repository に commit せず、integration-test setup で fixture SquashFS を生成することを推奨する。

---

# 41. README

### REQ-DOC-001 [Ubiquitous][MUST]
README は少なくとも以下を説明しなければならない。

1. Dembly concept / terminology
2. architecture summary
3. Linux-only PoC prerequisite
4. development setup
5. prerequisite checker
6. build/test/lint
7. Card format
8. `dembly card build`
9. Deck examples
10. Image Base
11. Compose Base
12. Volume / shared Volume
13. Host Bind variables
14. lock workflow
15. privileged PoC security limitation
16. Non-goals / known limitations
17. performance benchmark procedure

### REQ-DOC-002 [Ubiquitous][MUST]
README の CLI example は実装済み CLI behavior と一致しなければならない。

---

# 42. 推奨 Rust library

version は `Cargo.lock` で固定し、本仕様では特定 version を要求しない。

推奨:

- `clap` — CLI
- `serde` — serialization
- `toml` — TOML
- `sha2` — SHA-256
- `thiserror` — domain error
- `tracing`
- `tracing-subscriber`
- `tempfile`
- `nix` または narrowly-scoped libc call — UID/GID drop

### REQ-LIB-001 [Ubiquitous][SHOULD]
Docker integration は、PoC では Docker SDK より Docker CLI / `docker compose` subprocess を優先することを推奨する。

### REQ-LIB-002 [Ubiquitous][MUST]
unsafe code を導入する場合、必要箇所へ限定し safety assumption を文書化しなければならない。

---

# 43. 実装順序

Codex は原則として以下の順序で実装する。

1. Rust workspace
2. mise
3. `setup-dev.sh` / `check-linux.sh`
4. TOML model / validation
5. Card builder
6. Image Base orchestration
7. single-binary Runtime injection
8. Runtime user probe
9. Runtime kernel SquashFS mount
10. environment / exports / hooks
11. Volume / Host Bind planning
12. `validate` / `inspect` / `up` / `down` / `run` / `exec`
13. `deck.lock`
14. `check`
15. Compose Base override integration
16. integration tests
17. real Card acceptance
18. performance / disk evaluation documentation

### REQ-IMP-001 [Ubiquitous][MUST]
実装者は、Compose integration より先に Image Base で core Card mechanism を成立させなければならない。

### REQ-IMP-002 [Ubiquitous][MUST NOT]
実装者は、PoC core Card mechanism が未成立の段階で Compose 固有処理を主実装対象にしてはならない。

---

# 44. 実装上の禁止事項

### REQ-CNS-001 [Ubiquitous][MUST NOT]
Dembly は Host 側で SquashFS を mount してはならない。

### REQ-CNS-002 [Ubiquitous][MUST NOT]
Dembly は PoC で FUSE を使用してはならない。

### REQ-CNS-003 [Ubiquitous][MUST NOT]
Card selection 変更だけを理由に Base を rebuild してはならない。

### REQ-CNS-004 [Ubiquitous][MUST NOT]
Dembly Volume に Docker named volume を使用してはならない。

### REQ-CNS-005 [Ubiquitous][MUST NOT]
`deck.toml` を parent directory へ探索してはならない。

### REQ-CNS-006 [Ubiquitous][MUST NOT]
`up` / `run` / `check` で `deck.lock` を暗黙更新してはならない。

### REQ-CNS-007 [Ubiquitous][MUST NOT]
Card-to-Card environment conflict を last-wins で解決してはならない。

### REQ-CNS-008 [Ubiquitous][MUST NOT]
mount/export conflict を last-wins で解決してはならない。

### REQ-CNS-009 [Ubiquitous][MUST NOT]
`emcos-sdk` を generic code 内で special Card にしてはならない。

### REQ-CNS-010 [Ubiquitous][MUST NOT]
Base Image に Dembly を事前 install することを要求してはならない。

### REQ-CNS-011 [Ubiquitous][MUST NOT]
Host CLI と Runtime initializer を別配布 executable に分割してはならない。

### REQ-CNS-012 [Ubiquitous][MUST NOT]
PoC local Card resolution に network/repository dependency を追加してはならない。

### REQ-CNS-013 [Ubiquitous][MUST NOT]
user-authored Compose file を変更してはならない。

### REQ-CNS-014 [Ubiquitous][MUST NOT]
Dembly は `sudo` を自動実行してはならない。

---

# 45. Definition of Done

PoC は以下をすべて満たしたとき完了とする。

### REQ-DOD-001 [Ubiquitous][MUST]
`git` と `mise` が既に存在する Host 上で、documented system prerequisite を満たせば development setup が完了しなければならない。

### REQ-DOD-002 [Ubiquitous][MUST]
`scripts/check-linux.sh` が prerequisite を正しく検出しなければならない。

### REQ-DOD-003 [Ubiquitous][MUST]
`scripts/setup-dev.sh` が idempotent でなければならない。

### REQ-DOD-004 [Ubiquitous][MUST]
Rust workspace が build できなければならない。

### REQ-DOD-005 [Ubiquitous][MUST]
以下が pass しなければならない。

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

### REQ-DOD-006 [Ubiquitous][MUST]
supported Linux Docker Host 上で applicable integration tests が pass しなければならない。

### REQ-DOD-007 [Ubiquitous][MUST]
Host と Runtime で同一 static `dembly` executable が動作しなければならない。

### REQ-DOD-008 [Ubiquitous][MUST]
Image Base が動作しなければならない。

### REQ-DOD-009 [Ubiquitous][MUST]
Compose Base が動作しなければならない。

### REQ-DOD-010 [Ubiquitous][MUST]
Card selection を変更しても Card 変更だけを理由とした Base rebuild が発生してはならない。

### REQ-DOD-011 [Ubiquitous][MUST]
SquashFS mount は Runtime 内だけに存在しなければならない。

### REQ-DOD-012 [Ubiquitous][MUST]
Deck/Card Volume は本仕様どおり永続化されなければならない。

### REQ-DOD-013 [Ubiquitous][MUST]
Host Bind variable expansion は本仕様どおり動作しなければならない。

### REQ-DOD-014 [Ubiquitous][MUST]
`deck.lock` は生成・検証・enforce されなければならない。

### REQ-DOD-015 [Ubiquitous][MUST]
`dembly card build` は interactive と non-interactive の双方で動作しなければならない。

### REQ-DOD-016 [Ubiquitous][MUST]
real artifact が利用可能な環境では、`emcos-sdk`、`clang`、`tis` Card の representative acceptance test が実行できなければならない。

### REQ-DOD-017 [Ubiquitous][MUST]
representative performance measurement method と結果記録方法が文書化されなければならない。

---

# 46. PoC 最終 acceptance statement

### REQ-ACC-001 [Ubiquitous][MUST]

Dembly PoC は、**同一の unchanged Base と事前ビルド済み immutable Card を使用し、Base を Card 構成ごとに再ビルドせずに異なる Card combination を選択でき、Card file を read-only で Runtime へ渡し、各 SquashFS を Runtime 内だけで kernel mount し、通常の Linux filesystem / process semantics で Card tool を実行し、Deck root 配下に宣言済み persistent state を保持し、eMCOS / Clang / TIS の representative workload を実用性能で実行できなければならない。**

---

# 47. 将来拡張に関するアーキテクチャ方針

以下は PoC 実装要求ではないが、現在の設計を壊さず将来拡張可能であることを意図する。

```text
deck.toml
    ↓ desired configuration

deck.lock
    ↓ resolved immutable identities

Dembly Resolver
    ├── local Card path (PoC)
    ├── local cache (future)
    └── Dembly Repository (future)
             ↓
        immutable Card

Docker Registry
    ↓
immutable Base

volumes/
    ↓
mutable Deck state
```

将来的に想定する CLI:

```text
dembly card pull
dembly card push
dembly repo login
dembly repo search
```

Repository client は同一 `dembly` CLI に追加可能とする。

Repository Server が authentication / database / object storage / HA 等を持つ本格 service になった場合のみ、将来別 executable への分離を検討してよい。

---

# 48. トレーサビリティ方針

### REQ-TRC-001 [Ubiquitous][SHOULD]
実装 code / test では、対応する Requirement ID を test name、comment、test table 等から追跡可能にすることを推奨する。

### REQ-TRC-002 [Ubiquitous][SHOULD]
1 つの requirement を複数 test で検証する場合でも、Requirement ID と test の対応関係が repository 内で確認可能であることを推奨する。

### REQ-TRC-003 [Ubiquitous][MUST]
本仕様と実装に差異が生じた場合、実装者は差異を黙って吸収せず、差異内容を明示しなければならない。
