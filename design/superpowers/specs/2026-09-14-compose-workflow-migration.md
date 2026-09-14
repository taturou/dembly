# Docker Compose運用への移行仕様

## 1. 文書の位置付け

この文書は、従来実装から[正本の仕様](../../spec.md)へ移行するための仕様差分を定義する。

現在の完成形は`design/spec.md`だけで定義し、この文書を実装後の動作仕様として参照しない。

具体的な実装手順、変更ファイル、テストの追加順序は`design/superpowers/plans/`に別途作成する。

## 2. 移行の目的

従来のDemblyは、Deckの解決に加えてコンテナの作成、起動、停止、削除、コマンド実行を担当している。

移行後のHost Demblyは設定コンパイラに限定し、コンテナのライフサイクルをDocker Composeへ移す。

Runtime Demblyは引き続きコンテナ内でCardをマウントし、環境を初期化してから利用者のプロセスを起動する。

## 3. 維持する仕様

- Cardは`card.toml`と`rootfs.squashfs`から構成する。
- Cardのmanifest checksumとfilesystem checksumを検証する。
- Card名、マウント先、export先、環境変数の競合を検出する。
- SquashFSはRuntimeのマウント名前空間だけにマウントする。
- Cardの選択変更では基礎イメージを再ビルドしない。
- DeckおよびCardのVolumeとHost Bindを維持する。
- 環境変数、`PATH`、export、post-mount hook、Card checkの意味を維持する。
- Runtime初期化とpost-mount hookはrootで実行する。
- 元のプロセスは解決済みの指定利用者で実行する。
- HostとRuntimeでは同一の静的Dembly実行ファイルを使用する。

## 4. 廃止する仕様

| 従来仕様 | 移行後 | 理由 |
|---|---|---|
| Image Base | 対象外 | コンテナ起動方式をCompose Baseへ統一するため |
| `dembly up` | `dembly apply`後の`docker compose up` | コンテナ操作をDocker Composeへ移すため |
| `dembly down` | `docker compose down` | 同上 |
| `dembly run` | `docker compose run` | 同上 |
| `dembly exec` | `docker compose exec` | 同上 |
| `dembly check`による一時コンテナ起動 | `docker compose run`によるRuntime check | Host Demblyからコンテナ操作を除くため |
| Dembly生成の一時Compose override | 利用者の管理対象Composeファイルを直接更新 | 標準Composeコマンドから同じ定義を参照するため |
| `deck.toml` | `.dembly/config.toml` | Dembly関連設定を`.dembly/`へ集約するため |
| `deck.lock` | 管理対象Composeファイルの`x-dembly.lock` | Lockと適用状態をCompose定義と同じ正本へ保存するため |
| Host実行ファイルの元パスをbind mount | `.dembly/runtime/bin/dembly`へコピー | Host側更新後も適用時のRuntimeを再現するため |
| 実行時状態を一時ディレクトリへ保存 | `x-dembly.state`と`.dembly/runtime/` | 適用状態と再生成物を分離するため |

`up`、`down`、`run`、`exec`は公開CLIから削除する。

旧コマンドを互換用の別名として残さない。

## 5. 設定モデルの変更

### 5.1 Deck設定

従来の`deck.toml`にあるCompose Base、Card、環境変数、Volume、Host Bindの定義を`.dembly/config.toml`へ移す。

`[base]`の分岐は廃止し、`[compose]`を必須にする。

```toml
[compose]
path = "../.devcontainer/compose.yaml"
service = "dev"
```

| 従来 | 移行後 |
|---|---|
| カレントディレクトリの`deck.toml`または位置引数 | カレントディレクトリの`.dembly/config.toml`または`--config <path>` |
| `deck.toml`の親をDeck rootとする | `config.toml`の親である`.dembly/`をDeck rootとする |
| 親ディレクトリを探索しない | 維持する |

### 5.2 初期設定の生成

新たに`dembly init`を追加する。

`init`はカレントディレクトリを基準に、`.dembly/cards/*/card.toml`、`.dembly-cards/*/card.toml`、`.cards/*/card.toml`、`cards/*/card.toml`だけをCard候補として探索する。

Dev Containers設定は、`.devcontainer.json`、`.devcontainer/devcontainer.json`、`.devcontainer/*/devcontainer.json`だけを候補として探索する。

各探索ルートの規定より深い階層は探索せず、シンボリックリンクであるディレクトリと候補ファイルは除外する。

候補は正規化した相対パスの辞書順で表示し、1件の場合も人間に採用を確認させる。

同じCard名の候補はパスで区別し、選択後のCard名が重複する場合はエラーとする。

既存の`.dembly/config.toml`は上書きしない。

`--config`を指定した場合は指定先へ設定を生成し、指定先の親をDeck rootとする。

### 5.3 Dev Containers

`.dembly/config.toml`の任意フィールド`devcontainer.path`でDev Containers連携を有効にする。

Demblyは`devcontainer.json`を書き換えず、対象サービス、Composeファイルの順序、`overrideCommand`、`containerUser`、`remoteUser`を検証する。

## 6. Lockと適用状態の変更

### 6.1 Lock

`dembly lock`は`deck.lock`を生成せず、管理対象Composeファイルの`x-dembly.lock`だけを更新する。

LockからComposeファイル全体のhashを削除する。

代わりに、管理対象Composeファイルの正規化済みパス、対象サービス、イメージID、Cardの識別情報とchecksumを保存する。

Composeファイル全体のhashは、同じComposeファイル内へLockを保存すると自己参照になるため使用しない。

### 6.2 適用状態

管理フィールドごとの`original`と`applied`を`x-dembly.state`へ保存する。

時系列履歴を持つ`state.toml`は作成しない。

再適用と適用解除では、Composeファイルの現在値が`applied`と一致することを確認する。

不一致の場合は競合として終了し、利用者の変更を上書きしない。

### 6.3 `unapply`

新たに`dembly unapply`を追加する。

`unapply`は管理フィールドを`original`へ戻し、`x-dembly.state`と`.dembly/runtime/`を削除する。

`x-dembly.lock`と`.dembly/volumes/`は保持する。

利用者は`unapply`の前に`docker compose down`を実行し、`unapply`自体はコンテナを検出または停止しない。

## 7. Compose適用方式の変更

### 7.1 管理対象ファイル

Host Demblyが更新できるComposeファイルを、`.dembly/config.toml`の`compose.path`で1つだけ指定する。

Dev Containersが複数のComposeファイルを使う場合でも、他のファイルは更新しない。

他の`dockerComposeFile`に`x-dembly`が存在する場合はエラーとする。

管理対象Composeファイルを`dockerComposeFile`配列の最後に置く。

### 7.2 管理フィールド

従来の一時override生成を廃止し、対象サービスの`entrypoint`、`command`、`user`、`privileged`、Dembly label、RuntimeとCardとVolumeとHost Bindのmountを直接管理する。

対象外サービスと管理対象外フィールドは保持する。

### 7.3 Runtime成果物

`apply`は`.dembly/runtime/<service>.toml`、`.dembly/runtime/bin/dembly`、`.dembly/volumes/`以下の必要なディレクトリを生成する。

`apply`は`.dembly/runtime/`と`.dembly/volumes/`を`.gitignore`へ追加する。

`.dembly/config.toml`、Composeファイル、`devcontainer.json`、Host初期化スクリプトはGit管理を維持する。

## 8. Runtime起動方式の変更

対象サービスの`entrypoint`を次の内部コマンドへ置き換える。

```text
/run/dembly/bin/dembly __runtime init /run/dembly/runtime/<service>.toml
```

対象サービスの`command`は空にする。

Runtime DemblyはrootでCardをマウントし、Volume、Host Bind、export、hook、環境変数を設定する。

Host BindはComposeから`/run/dembly/binds/`へ一旦マウントし、Runtime Demblyが利用者解決後に`${HOME}`と`${USER}`を展開して最終マウント先へbind mountする。

元の`entrypoint`と`command`はRuntime計画と`x-dembly.state`へ保存する。

Host Demblyは、適用前のComposeサービスの`user`、イメージのDockerfile `USER`、`root`の順でDocker形式の利用者指定を選び、Runtime計画の`runtime_user.spec`へ保存する。

従来のHost側ユーザープローブは削除し、Runtime Demblyがコンテナ内の`/etc/passwd`と`/etc/group`からUID、GID、ホームディレクトリを解決する。

`user`、`uid`、`user:group`、`uid:gid`を受け付けるが、ユーザー部分に対応する`/etc/passwd`のエントリを必須とする。

利用者またはグループを解決できない場合は、Cardをマウントせず、元のプロセスを起動しない。

利用者を解決した後はrootで環境を初期化し、指定利用者へ権限を変更して元のプロセスを実行する。

`docker compose run`から追加された引数がある場合は、元のプロセスの代わりにその引数を実行する。

## 9. コマンド移行

| 従来の操作 | 移行後の操作 |
|---|---|
| `dembly validate [deck.toml]` | `dembly validate [--config <path>]` |
| `dembly lock [deck.toml]` | `dembly lock [--config <path>]` |
| `dembly inspect [deck.toml]` | `dembly inspect [--config <path>]` |
| `dembly check [deck.toml]` | Host整合性は`dembly check`、Card checkは`docker compose run` |
| `dembly up [deck.toml]` | `dembly apply`後に`docker compose up -d` |
| `dembly down [deck.toml]` | `docker compose down` |
| `dembly run [deck.toml] -- <command...>` | `docker compose run --rm <service> <command...>` |
| `dembly exec [deck.toml] -- <command...>` | `docker compose exec --user <intended-user> <service> <command...>` |
| 該当なし | `dembly init [--config <path>]` |
| 該当なし | `dembly apply [--config <path>]` |
| 該当なし | `dembly unapply [--config <path>]` |

Card checkは次の形でRuntime Demblyへ委譲する。

```bash
docker compose run --rm <service> \
  /run/dembly/bin/dembly __runtime check /run/dembly/runtime/<service>.toml
```

通常のRuntime初期化とmountはrootで行い、Card checkは初期化後に指定利用者で実行する。

## 10. Dev Containers起動への対応

Dev Containersの`initializeCommand`には、利用者管理のHost初期化スクリプトを指定する。

Host初期化スクリプトから`dembly validate`、必要な場合の`dembly lock`、`dembly apply`を順番に実行する。

Demblyは`initializeCommand`へコマンド列を自動合成しない。

`containerUser`はrootまたは未指定とし、Runtime Demblyをrootで開始する。

`remoteUser`はRuntime計画へ保存する利用者指定のユーザー部分と一致させる。

`updateRemoteUserUID`によるUIDとGIDの同期は許可し、Runtime Demblyは同期後のコンテナ内利用者情報を解決する。

AIがDocker Composeを直接使用する場合も、Dev Containersの`dockerComposeFile`と同じファイル列、同じトップレベル`name`、同じ対象サービスを使用する。

## 11. 実装領域への影響

| 実装領域 | 主な変更 |
|---|---|
| CLI解析 | `init`に固定ルートの探索を追加し、`apply`と`unapply`を追加して、`up`、`down`、`run`、`exec`を削除する |
| 設定モデル | `DeckDocument`を`.dembly/config.toml`のCompose専用モデルへ移行する |
| Lock | `deck.lock`の読み書きを`x-dembly.lock`のYAML読み書きへ置換する |
| Compose処理 | 一時override生成とライフサイクル操作を、管理対象ファイルの競合検出付き編集へ置換する |
| Runtime計画 | 一時ディレクトリではなく`.dembly/runtime/<service>.toml`へ決定的に生成し、解決済みUIDなどの代わりに`runtime_user.spec`とHost Bindの最終マウント先を保存する |
| Runtime実行ファイル | Hostパスのbind mountを、適用時コピーのbind mountへ置換する |
| Runtime起動 | コンテナ内で利用者を解決し、保存した元のプロセスと`docker compose run`の追加引数を選択できるようにする |
| Card check | Host CLIによるコンテナ起動を削除し、Runtime内部コマンドとして実行する |
| Dev Containers | `devcontainer.json`の読み取りと整合性検証を追加する |
| テスト | HostがDockerを操作しないこと、Compose標準コマンドとの連携、競合、冪等性、復元を追加する |
| 例とREADME | `deck.toml`とDemblyライフサイクルコマンドを、新設定とDocker Compose操作へ置換する |

## 12. 移行時の削除対象

- Image Baseの設定、解決、Lock、Runtime操作
- `deck.toml`と`deck.lock`
- `dembly up`、`down`、`run`、`exec`
- DemblyからのDockerコンテナ操作
- DemblyからのDocker Composeライフサイクル操作
- 一時Compose override
- 一時runtime stateディレクトリ
- 旧Composeプロジェクト名の暗黙生成
- 旧仕様だけを検証するテストとトレーサビリティ項目

## 13. 移行完了条件

- `design/spec.md`の全公開Hostコマンドがコンテナを操作しない。
- 旧公開コマンド`up`、`down`、`run`、`exec`がCLIとヘルプから消えている。
- `deck.toml`と`deck.lock`を必要とするコードパスが残っていない。
- `init`が規定したCardとDev Containersの位置だけを決定的な順序で探索する。
- 管理対象ComposeファイルだけがDemblyから更新される。
- `docker compose ps`がDembly適用済みコンテナを同じComposeプロジェクトとして表示する。
- Docker Composeの`up`、`logs`、`exec`、`run`、`down`がDembly Runtimeと併用できる。
- Dev ContainersとDocker Composeの直接実行が同じ対象サービスとRuntime設定を使用する。
- Runtime Demblyがrootで初期化し、元のプロセスを指定利用者で実行する。
- Runtime Demblyが任意のDocker形式の利用者指定をコンテナ内で解決し、解決失敗時にCardと元のプロセスを起動しない。
- Card変更が基礎イメージの再ビルドなしで反映される。
- `apply`と`unapply`が競合を検出し、同じ入力への再実行が冪等である。
- 正本、移行仕様、実装計画、README、例、テストの用語とコマンドが一致する。
