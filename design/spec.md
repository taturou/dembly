# Dembly PoC 仕様書

## 1. 目的

Demblyは、Docker Composeで定義した開発コンテナへ、再利用可能な開発ツール群をCardとして組み込む設定コンパイラである。

Demblyはコンテナを操作せず、利用者が管理するComposeファイルをDocker Composeから直接利用できる形へ更新する。

コンテナの作成、状態確認、ログ取得、コマンド実行、停止にはDocker Composeを使用する。

Cardを追加または更新するために、基礎イメージを再ビルドする必要はない。

## 2. スコープ

### 2.1 対象

- Linux x86_64
- Docker Engine
- Docker Compose v2
- カーネルのSquashFS機能
- Composeを直接使用する開発環境
- VS Code Dev ContainersからComposeを使用する開発環境

### 2.2 対象外

- イメージだけを基礎環境とする動作方式
- FUSE
- rootless Docker
- Dockerの名前付きボリューム
- Kubernetes
- Podman
- 信頼できないCardを隔離するサンドボックス
- 本番環境向けの最小権限設計

Host側のマウント名前空間には、CardのSquashFSをマウントしない。

## 3. DeckとCard

### 3.1 Deck

**Deck**は、1つのComposeサービスへ組み込むCard、永続領域、Host Bind、環境変数をまとめた開発環境の定義である。

Deckの正本は`.dembly/config.toml`である。

`.dembly/`ディレクトリをDeck rootと呼ぶ。

設定内の相対パスと`${DECK_ROOT}`はDeck rootを基準に解決する。

### 3.2 Card

**Card**は、開発ツールのファイルシステムと、その組み込み方法をまとめた不変の成果物である。

1つのCardは次の2ファイルからなる。

- `card.toml`：Cardの識別情報、検証値、マウント先、環境設定を定義する。
- `rootfs.squashfs`：開発ツールのファイルシステムを保持する。

通常運用では、Cardを`/var/lib/dembly/cards/`などのプロジェクト外の絶対パスへ配置する。

Deck rootからの相対パスは、Cardをプロジェクト内で開発するときに使用する。

## 4. 基本アーキテクチャ

Demblyは、同一の実行ファイルをHost側とコンテナ側で使用する。

実行場所によって、Host DemblyとRuntime Demblyに役割を分ける。

```text
.dembly/config.toml ─┐
card.toml ───────────┼─> Host Dembly ─> 管理対象Composeファイル
devcontainer.json ───┘          │              │
                                ├─> Runtime計画 │ docker compose / Dev Containers
                                └─> Runtime実行ファイル
                                               │
                                               ▼
                                      Runtime Dembly（root）
                                               │
                              Cardをマウントし、環境を初期化
                                               │
                                               ▼
                                    元のプロセス（指定利用者）
```

### 4.1 Host Dembly

Host Demblyは設定を解決し、Lockを作成し、管理対象ComposeファイルへRuntime Demblyの起動設定を適用する。

Host DemblyはDockerコンテナを作成、起動、停止、削除せず、実行中のコンテナ内でコマンドを実行しない。

### 4.2 Runtime Dembly

Runtime Demblyはコンテナの`entrypoint`として常にrootで起動する。

Runtime DemblyはCardのSquashFSをマウントし、Volume、Host Bind、export、hook、環境変数を設定する。

初期化後は権限を指定利用者へ変更し、Compose適用前の`entrypoint`と`command`から得た元のプロセスを`exec`する。

### 4.3 Docker Compose

Docker Composeはコンテナのライフサイクルを管理する唯一の公開インターフェースである。

`dembly apply`後のComposeファイルは、`docker compose up`、`ps`、`logs`、`exec`、`run`、`down`から直接使用できる。

### 4.4 VS Code Dev Containers

Dev Containersは任意の追加機能であり、Docker Composeとは別のDembly動作方式ではない。

Dev ContainersはDembly適用済みの同じComposeファイルを使用してコンテナを起動する。

人間とAIが使用する必須の開発環境は、Composeファイル、Dockerfile、Cardに定義する。

Dev Containers固有の機能だけに必須環境を依存させない。

### 4.5 所有権

| 対象 | 所有者 | Demblyの扱い |
|---|---|---|
| `.dembly/config.toml` | 利用者 | `init`による初回生成後は読み取り専用として扱う |
| `card.toml`と`rootfs.squashfs` | Card作成者 | 読み取りと検証だけを行う |
| `devcontainer.json` | 利用者 | 読み取りと検証だけを行う |
| 管理対象Composeファイルの通常フィールド | 利用者 | Dembly管理領域を除いて保持する |
| 管理対象Composeファイルの`x-dembly` | Dembly | Lockと適用状態を更新する |
| 選択サービスのDembly管理フィールド | Dembly | `apply`と`unapply`で更新する |
| `.dembly/runtime/` | Dembly | 再生成可能な成果物として更新する |
| `.dembly/volumes/` | 利用者の実行データ | 自動削除しない |
| コンテナ、ネットワーク、Composeプロジェクト | Docker Compose | Demblyから操作しない |

## 5. 代表的な利用方法

### 5.1 AIがDocker Composeを使用する場合

```bash
dembly validate
dembly lock
dembly apply
docker compose -f .devcontainer/compose.yaml up -d
docker compose -f .devcontainer/compose.yaml ps
docker compose -f .devcontainer/compose.yaml exec --user vscode dev sh
docker compose -f .devcontainer/compose.yaml down
```

複数のComposeファイルを使用する場合は、Dev Containersの`dockerComposeFile`と同じ順序ですべての`-f`を指定する。

`dembly apply`を再実行した後は、`docker compose up -d`によって必要なコンテナを再作成する。

### 5.2 人間がDev Containersを使用する場合

`devcontainer.json`の`initializeCommand`は、利用者が管理するHost初期化スクリプトを呼び出す。

```json
{
  "dockerComposeFile": ["compose.base.yaml", "compose.yaml"],
  "service": "dev",
  "initializeCommand": ".devcontainer/initialize-host.sh",
  "overrideCommand": false,
  "containerUser": "root",
  "remoteUser": "vscode"
}
```

Host初期化スクリプトは、Dembly以外の処理と`dembly apply`を必要な順序で実行できる。

```bash
#!/usr/bin/env bash
set -euo pipefail

# 利用者固有のHost初期化処理
dembly validate
dembly lock
dembly apply
```

Demblyは`initializeCommand`とHost初期化スクリプトを書き換えない。

Dev Containersは初期化スクリプトの完了後に、Dembly適用済みのComposeファイルからコンテナを起動する。

### 5.3 Cardを開発する場合

CardをDeck内の相対パスに置き、`dembly card build`で`rootfs.squashfs`と`card.toml`を生成する。

Cardを変更した後は、`dembly lock`と`dembly apply`を再実行し、Composeからコンテナを再作成する。

基礎イメージの再ビルドは行わない。

## 6. 状態遷移

```text
未初期化
  │ dembly init
  ▼
設定済み
  │ dembly lock
  ▼
Lock済み
  │ dembly apply
  ▼
適用済み
  │ docker compose up
  ▼
実行中
```

`docker compose down`はコンテナとネットワークを削除するが、Demblyの適用状態とVolumeを変更しない。

`dembly unapply`はComposeファイルをLock済みの状態へ戻すが、コンテナを停止しない。

適用済みまたは実行中にCard、対象サービス、対象イメージを変更した場合は、`dembly lock`と`dembly apply`を順に再実行する。

`dembly lock`は適用状態を削除しないが、保存された適用状態のLock digestと新しいLockが一致しなくなるまで、その適用状態を最新とはみなさない。

`apply`と`check`は有効なLockを要求し、Lockを暗黙に更新しない。

## 7. 入力ファイルと生成物

### 7.1 ファイルの関係

| 対象 | 種別 | Git管理 | 参照または生成する操作 |
|---|---|---:|---|
| `.dembly/config.toml` | 入力 | する | `init`が生成し、Hostコマンドが参照する |
| `card.toml` | 入力 | 配置方法による | `card build`が生成し、Hostコマンドが参照する |
| `rootfs.squashfs` | 入力 | 配置方法による | `card build`が生成し、Runtimeがマウントする |
| `devcontainer.json` | 任意入力 | する | `init`、`validate`、`apply`が参照する |
| 管理対象Composeファイル | 入出力 | する | `lock`、`apply`、`unapply`が更新する |
| `.dembly/runtime/<service>.toml` | 生成物 | しない | `apply`が生成し、Runtimeが参照する |
| `.dembly/runtime/bin/dembly` | 生成物 | しない | `apply`が現在の実行ファイルをコピーする |
| `.dembly/volumes/` | 実行データ | しない | `apply`が必要なディレクトリを作成する |
| `.gitignore` | 利用者ファイル | する | `apply`が不足する除外規則だけを追記する |

### 7.2 `.dembly/config.toml`

`.dembly/config.toml`はDeck設定の正本である。

`--config <path>`を省略した場合は、カレントディレクトリの`.dembly/config.toml`だけを使用する。

親ディレクトリは探索しない。

`--config`で指定した場合は、指定ファイルの親ディレクトリをDeck rootとする。

`init`以外のコマンドで指定ファイルが存在しない場合はエラーとする。

```toml
schema_version = 1

[compose]
path = "../.devcontainer/compose.yaml"
service = "dev"

[devcontainer]
path = "../.devcontainer/devcontainer.json"

[[cards]]
path = "/var/lib/dembly/cards/clang/card.toml"

[environment]
MODE = "development"

[environment_path]
prepend = ["/workspace/bin"]

[[volumes]]
name = "build"
target = "/workspace/build"
shared = false

[[binds]]
source = "${DECK_ROOT}/.."
target = "/workspace"
mode = "rw"
required = true
```

`schema_version`、`compose.path`、`compose.service`は必須である。

`devcontainer.path`は任意であり、指定した場合だけDev Containers固有の検証を有効にする。

未知のフィールドはエラーとする。

### 7.3 `card.toml`

```toml
schema_version = 1
name = "clang"
version = "20.1.0"

[filesystem]
type = "squashfs"
file = "rootfs.squashfs"
sha256 = "<sha256>"

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
source = "${HOST_HOME}/.config/clang"
target = "${HOME}/.config/clang"
mode = "ro"
required = false

[[hooks.post_mount]]
exec = "setup/post-mount.sh"
args = []

[check]
exec = "bin/clang"
args = ["--version"]
```

`filesystem.file`は`card.toml`の親ディレクトリを基準に解決する。

`mount.target`、Volumeの`target`、Host Bindの`target`、exportの`target`は絶対パスとする。

未知のフィールドはエラーとする。

### 7.4 `devcontainer.json`

`devcontainer.path`を指定した場合、次の条件を満たす必要がある。

- `service`は`compose.service`と一致する。
- `dockerComposeFile`は`compose.path`を含む。
- 管理対象Composeファイルは`dockerComposeFile`配列の最後にある。
- `overrideCommand`は`false`または未指定である。
- `containerUser`は`root`または未指定である。
- `remoteUser`は必須であり、指定利用者と一致する。

`dockerComposeFile`内の相対パスは、`devcontainer.json`の親ディレクトリを基準に解決する。

管理対象以外のComposeファイルに`x-dembly`が存在する場合はエラーとする。

Demblyは`devcontainer.json`を書き換えない。

### 7.5 管理対象Composeファイル

`compose.path`で指定したファイルだけをDemblyの管理対象とする。

管理対象Composeファイルには、トップレベルの`name`が必要である。

`name`は利用者が管理し、Demblyは存在と妥当性を検証するが変更しない。

複数のComposeファイルを使用する場合、管理対象Composeファイルを最後に置くことで、Demblyが注入する設定を最終的な有効値にする。

### 7.6 `x-dembly`

`x-dembly`は、管理対象Composeファイルに保存するDemblyの永続状態である。

時系列履歴は保存せず、Lockと管理フィールドごとの初期値および前回適用値だけを保存する。

```yaml
x-dembly:
  schema_version: 1
  lock:
    compose_path: ../.devcontainer/compose.yaml
    service: dev
    image: sha256:0123456789abcdef
    cards:
      - name: clang
        version: 20.1.0
        manifest_sha256: sha256:1111
        filesystem_sha256: sha256:2222
  state:
    service: dev
    lock_digest: sha256:3333
    fields:
      entrypoint:
        original: ["/usr/local/bin/start"]
        applied: ["/run/dembly/bin/dembly", "__runtime", "init", "/run/dembly/runtime/dev.toml"]
      command:
        original: ["--watch"]
        applied: []
```

実装は「フィールドが存在しない状態」と明示的な`null`を区別して保存する。

`lock`は`x-dembly.lock`だけを更新する。

`apply`は`x-dembly.state`とDembly管理フィールドを更新する。

`unapply`は`x-dembly.state`を削除するが、`x-dembly.lock`は保持する。

### 7.7 Runtime計画

`.dembly/runtime/<service>.toml`は、Host Demblyが生成するRuntime Dembly専用の解決済み計画である。

```toml
schema_version = 1
lock_digest = "sha256:3333"

[runtime_user]
name = "vscode"
uid = 1000
gid = 1000
home = "/home/vscode"

[[cards]]
name = "clang"
image = "/run/dembly/cards/clang.squashfs"
mount_target = "/opt/dembly/cards/clang"

[[exports]]
source = "/opt/dembly/cards/clang/bin/clang"
target = "/usr/local/bin/clang"

[environment]
PATH = "/opt/dembly/cards/clang/bin:/workspace/bin:/usr/local/bin"

[process]
argv = ["/usr/local/bin/start", "--watch"]
```

Runtime計画には、コンテナ内の解決済みパスだけを保存する。

未解決の変数とHost Bind先のファイル内容は保存しない。

Runtime Demblyは`.dembly/config.toml`と`card.toml`を再解釈しない。

### 7.8 `.gitignore`

`apply`は次の規則が不足している場合だけ追記する。

```gitignore
/.dembly/runtime/
/.dembly/volumes/
```

`.dembly/config.toml`、Host初期化スクリプト、管理対象Composeファイルは除外しない。

追跡済みファイルを自動的にGitの管理対象から外さない。

## 8. 同一実行ファイルの役割分担

公開コマンドとして起動したDemblyはHost Demblyとして動作する。

Composeの`entrypoint`から`__runtime`を指定して起動したDemblyはRuntime Demblyとして動作する。

```text
dembly <公開コマンド>             Host Dembly
dembly __runtime init <計画>       Runtime Dembly
dembly __runtime check <計画>      Runtime DemblyのCard検査
```

`__runtime`は内部インターフェースであり、通常のヘルプには公開しない。

`apply`は現在実行中のDemblyを`.dembly/runtime/bin/dembly`へコピーする。

これにより、Host側のDemblyを更新した後も、適用時のRuntime計画と同世代の実行ファイルを使用できる。

## 9. Hostコマンド

### 9.1 一覧

```text
dembly init [--config <path>]
dembly validate [--config <path>]
dembly lock [--config <path>]
dembly apply [--config <path>]
dembly unapply [--config <path>]
dembly inspect [--config <path>]
dembly check [--config <path>]
dembly card build <tool-root> <cards-root> [options]
```

すべてのHostコマンドはコンテナを操作しない。

### 9.2 `dembly init`

`init`は、`.dembly/config.toml`が存在しない場合だけ設定を生成する。

`--config`を省略した場合はカレントディレクトリの`.dembly/config.toml`を生成し、`.dembly/`がなければ作成する。

`--config`を指定した場合は指定先へ生成し、親ディレクトリがなければ作成する。

カレントディレクトリ内の`card.toml`と`devcontainer.json`を再帰的に探索し、候補を人間に選択させる。

`.git/`、`.dembly/`、シンボリックリンクであるディレクトリは探索しない。

Dev Containersを選択した場合は、`devcontainer.json`の`service`とComposeファイルを表示し、人間に採用を確認させる。

Dev Containersを使用しない場合は、Composeファイルとサービスを人間に選択させる。

既存の設定、Composeファイル、Lockは書き換えない。

### 9.3 `dembly validate`

`validate`は、設定、選択Card、管理対象Composeファイル、任意の`devcontainer.json`を読み取る。

次の項目を検証する。

- TOMLとJSONのスキーマ
- パスの正規化と許可された変数
- Cardのmanifest checksumとfilesystem checksum
- Card名、マウント先、export先、Volume、Host Bind、環境変数の競合
- 対象サービスとイメージの解決
- Dev Containersを使用する場合の整合性
- Dembly管理フィールドと保存済み適用値の競合

ファイルを書き換えず、Card hookを実行せず、コンテナを起動しない。

### 9.4 `dembly lock`

`lock`は`validate`相当の検証を行い、対象サービスが使用するイメージの不変IDを解決する。

管理対象Composeファイルの`x-dembly.lock`だけを更新する。

Lockは管理対象Composeファイルの正規化済みパス、対象サービス、解決済みイメージID、選択Cardの名前、バージョン、manifestとfilesystemのSHA-256を保持する。

Composeファイル全体のhashは自己参照を生じるため保存しない。

同じ入力に対するLockの直列化順序は決定的とする。

### 9.5 `dembly apply`

`apply`は有効なLockを要求する。

Lockの入力を再計算し、`x-dembly.lock`と一致しない場合は、`dembly lock`の実行を要求して終了する。

`apply`はRuntime計画、Runtime実行ファイル、必要なVolumeディレクトリ、`.gitignore`、対象サービス、`x-dembly.state`を生成または更新する。

対象サービスでは、`entrypoint`、`command`、`user`、`privileged`、Dembly label、Demblyが追加するmountを管理領域とする。

`entrypoint`は次の値へ置き換え、`command`は空配列にする。

```text
/run/dembly/bin/dembly __runtime init /run/dembly/runtime/<service>.toml
```

`user`は`root`、`privileged`は`true`とする。

適用したLock digestをlabelへ含め、Card変更後の`docker compose up`がサービス設定の変更を検出できるようにする。

対象外のサービスと管理対象外のフィールドは保持する。

初回適用では管理フィールドの現在値を`original`として保存する。

再適用では現在値を前回の`applied`と比較し、一致した場合だけ更新する。

同じ入力への再適用では、Composeファイルと生成物に差分を発生させない。

### 9.6 `dembly unapply`

`unapply`は`x-dembly.state`を要求する。

すべての管理フィールドについて、Composeファイルの現在値と保存済みの`applied`を比較する。

すべて一致した場合だけ`original`を復元し、Demblyが追加した管理フィールドと`x-dembly.state`を削除する。

`x-dembly.lock`、`.dembly/config.toml`、Card、Volume、Host初期化スクリプト、`.gitignore`の規則は保持する。

`.dembly/runtime/`は削除する。

利用者は`unapply`の前に`docker compose down`を実行する。

`unapply`は実行中のコンテナを検出または停止しない。

### 9.7 `dembly inspect`

`inspect`はコンテナを必要とせず、Deck root、対象サービス、管理対象Composeファイル、プロジェクト名、Lock、適用状態、Card、マウント、環境変数、指定利用者、次回の変更内容を表示する。

### 9.8 `dembly check`

Host側の`check`は、Lock、適用状態、生成済みRuntime計画、Runtime実行ファイル、Compose管理フィールドの整合性を検査する。

Cardの`[check]`は実行しない。

Cardの実行時checkは、10.6節のDocker Compose操作を使用する。

### 9.9 `dembly card build`

`card build`は`<tool-root>`の内容からSquashFSを生成し、`<cards-root>/<name>/`へ`rootfs.squashfs`と`card.toml`を出力する。

入力となるtool rootは変更しない。

成果物は一時ファイルへ生成し、SHA-256を計算した後に置き換える。

## 10. Runtime DemblyとDocker Compose

### 10.1 Runtimeの起動

Docker ComposeはDembly適用後の対象サービスをrootで起動する。

Runtime DemblyはRuntime計画のスキーマとLock digestを検証してから初期化を始める。

### 10.2 初期化順序

Runtime Demblyは次の順序で処理する。

1. CardのSquashFSを読み取り専用でマウントする。
2. Volumeを配置する。
3. Host Bindを確認する。
4. exportを作成する。
5. 環境変数を設定する。
6. Cardのpost-mount hookを実行する。
7. 指定利用者へ権限を変更する。
8. 元のプロセスまたはCompose `run`で指定されたプロセスを`exec`する。

いずれかの必須処理が失敗した場合は、最終プロセスを起動せず、Runtime Demblyを非ゼロで終了する。

### 10.3 指定利用者

指定利用者は、Composeサービスの`user`、イメージのDockerfile `USER`、rootの順で解決する。

Runtime Dembly自体とCardのpost-mount hookはrootで実行する。

元の`entrypoint`と`command`は指定利用者で実行する。

### 10.4 Card、Volume、Host Bind

すべてのCard filesystemは、コンテナ作成時に`/run/dembly/cards/`へ読み取り専用でbind mountする。

Deck private Volumeは`.dembly/volumes/<name>`、Card private Volumeは`.dembly/volumes/<card>/<name>`、shared Volumeは`.dembly/volumes/<name>`へ配置する。

Volumeの物理パスがシンボリックリンクである場合はエラーとする。

必須のHost Bindが存在しない場合はエラーとする。

任意のHost Bindが存在しない場合は警告を表示し、そのBindだけを省略する。

### 10.5 環境変数、export、hook

通常の環境変数は、基礎イメージ、Composeサービス、`.dembly/config.toml`、Cardの順で上書きする。

複数のCardが同じ通常環境変数を定義した場合はエラーとする。

`PATH`は、設定順のCard、`.dembly/config.toml`、基礎イメージとComposeの有効値の順で先頭へ追加する。

export先は`/usr/local/bin/`以下に限定し、重複またはDembly管理外の既存ファイルとの競合をエラーとする。

hookはCardの設定順と宣言順でrootとして実行し、各Cardのマウント先を作業ディレクトリとする。

hookが失敗した場合は、最終プロセスを起動しない。

### 10.6 Docker Composeコマンドとの対応

```bash
docker compose -f <compose-file> up -d
docker compose -f <compose-file> ps
docker compose -f <compose-file> logs <service>
docker compose -f <compose-file> exec --user <intended-user> <service> sh
docker compose -f <compose-file> run --rm <service> <command...>
docker compose -f <compose-file> run --rm <service> \
  /run/dembly/bin/dembly __runtime check /run/dembly/runtime/<service>.toml
docker compose -f <compose-file> down
```

| Docker Compose操作 | Demblyとの関係 |
|---|---|
| `up` | Runtime初期化後、保存済みの元のプロセスを起動する |
| `ps` | Dembly適用済みサービスを含む同じComposeプロジェクトの状態を表示する |
| `logs` | Runtime初期化と元のプロセスの出力を表示する |
| `exec` | 実行中のコンテナへ接続する |
| `run` | Runtime初期化後、元のプロセスの代わりに指定コマンドを実行する |
| Runtime `check` | Runtime初期化後、選択Cardの`[check]`を設定順に実行する |
| `down` | コンテナとネットワークを削除し、Dembly設定とVolumeは保持する |

`docker compose exec`は、`--user`を省略すると適用後のサービス定義に従ってrootで動作する。

通常の開発作業では指定利用者を明示し、管理操作だけをrootで実行する。

`docker compose run <service> <command...>`の追加引数は、保存済みの元のプロセスを置き換える。

Runtime `check`は、rootによる通常のRuntime初期化が完了した後、指定利用者で各Cardの`[check]`を実行する。

トップレベルの`name`と異なる`-p`または`COMPOSE_PROJECT_NAME`を指定する運用は対象外とする。

複数のComposeファイルを使用する場合は、すべてのコマンドで同じ`-f`の列を使用する。

## 11. 競合、失敗、再実行

### 11.1 管理フィールドの競合

`apply`または`unapply`の実行時に、管理フィールドの現在値が保存済みの`applied`と異なる場合は競合として終了する。

Demblyは利用者の変更を上書きしない。

エラーには、競合したサービス、フィールド、期待値、現在値を含める。

### 11.2 原子的な更新

Host Demblyはすべての入力と競合を検証してから書き込みを開始する。

各ファイルは同じディレクトリの一時ファイルへ書き、同期後にrenameして置き換える。

`apply`はRuntime成果物と`.gitignore`を先に更新し、管理対象Composeファイルを最後に置き換える。

Composeファイルの置き換え前に失敗した場合、以前のCompose設定は有効なままとする。

参照されない新しいRuntime成果物が残った場合は、次回の`apply`で上書きする。

### 11.3 冪等性と復元

同じ入力に対する`lock`と`apply`は、2回目以降に追跡対象ファイルの差分を発生させない。

利用者が管理フィールドを変更していない場合、`apply`の直後に`unapply`すると初回適用前の値へ戻る。

`unapply`後もLockを保持するため、入力が変わっていなければ再度`apply`できる。

## 12. セキュリティと権限

対象サービスには、カーネルのSquashFSをマウントするため`privileged: true`を設定する。

この権限は対象サービスだけに設定する。

Runtime DemblyとCardのpost-mount hookはrootで動作するため、Cardは信頼済みコードとして扱う。

Host Bindは宣言したHostファイルをコンテナへ公開するため、sourceとmodeを利用者が確認する。

Demblyは信頼できないCard、hook、Host Bindを隔離しない。

元のプロセス、`docker compose run`の指定コマンド、Dev Containersの端末とVS Code Serverは指定利用者で動作させる。

## 13. スキーマと互換性

`.dembly/config.toml`、`card.toml`、`x-dembly`、Runtime計画は、それぞれ`schema_version`を持つ。

このPoCが受け付ける値は`1`だけとする。

未対応の`schema_version`と未知のフィールドはエラーとし、ファイルを変更しない。

Host Demblyと`.dembly/runtime/bin/dembly`は`apply`時のコピーによって同じバージョンにそろえる。

Runtime計画のスキーマを解釈できないRuntime Demblyは、Cardをマウントせずに終了する。

## 14. 受入条件

- `init`が候補を人間に選択させ、`.dembly/config.toml`を生成できる。
- Host Demblyの公開コマンドがコンテナを作成、起動、停止、削除せず、コンテナ内でコマンドを実行しない。
- `lock`だけが`x-dembly.lock`を更新する。
- Lockが欠落または無効な場合、`apply`と`check`が失敗する。
- `apply`が対象サービスだけを更新し、対象外サービスと管理対象外フィールドを保持する。
- `apply`と`unapply`が管理フィールドの外部変更を検出し、利用者の変更を上書きしない。
- 同じ入力で`lock`と`apply`を再実行しても、追跡対象ファイルに差分が生じない。
- `apply`の直後に`unapply`すると、管理フィールドが適用前の値へ戻る。
- Card変更が基礎イメージの再ビルドなしでComposeのコンテナ再作成へ反映される。
- SquashFSのマウントがRuntimeのマウント名前空間だけに存在する。
- Runtime Demblyとpost-mount hookがrootで動作する。
- 元のプロセス、Compose `run`の指定コマンド、Dev Containersの接続プロセスが指定利用者で動作する。
- `docker compose ps`、`logs`、`exec`、`run`、`down`がDembly適用済みの同じComposeプロジェクトへ直接作用する。
- AIのDocker Compose運用と人間のDev Containers運用が、同じComposeファイル、Dockerfile、Cardを使用する。
- Runtime初期化またはCard checkが失敗した場合、対象コマンドが非ゼロで終了する。
- 未対応のスキーマまたは未知のフィールドを検出した場合、入力ファイルを変更せずに終了する。
