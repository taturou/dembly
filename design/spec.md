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

## 9. Command の契約

### `dembly init`

`init` は `.dembly/config.toml` が存在しない場合だけ生成する。

検出した Card のうち採用するものを人間に選択させる。

Compose Base 単独 mode では Compose file と service を入力させる。

Dev Containers を選択した場合は、宣言済み service を表示して採用確認だけを行う。

Compose file と lock は書き込まない。

### `dembly validate`

`validate` は config.toml、選択済み Card manifest、管理 Compose file、任意の devcontainer.json を読む。

TOML schema、正規化済み path、Card checksum、競合、Volume layout、Host Bind variable、selected service、任意の Dev Containers 制約を検証する。

file の書込み、Card hook の実行、container の起動をしてはならない。

### `dembly lock`

`lock` は検証を実行し、selected service の image identity を解決する。

管理 Compose file の `x-dembly.lock` だけを書き込む。

lock は管理 Compose path、selected service、解決済み image identity、全 selected Card identity を記録する。

lock は Compose file 全体の hash を含んではならない。
同じ file に保存すると自己参照になるためである。

### `dembly apply`

`apply` は valid lock を要求する。

`.dembly/runtime/<service>.toml` と `.dembly/runtime/bin/dembly` を生成する。

必要な Volume directory を `.dembly/volumes/` 配下に生成する。

既存 file を untrack せず、生成 directory の不足 ignore pattern を追加する。

管理 Compose file の selected service だけを更新する。

管理 field は `entrypoint`、`command`、`user`、`privileged`、Dembly label、Dembly 所有 mount entry とする。

Dembly 所有 mount entry は `/run/dembly/` 配下の Runtime target、または宣言済み Deck/Card Volume と Host Bind の target である。

管理対象外 service と管理対象外 field を保持する。

### `dembly unapply`

`unapply` は x-dembly state を要求する。

`original` を復元する前に、全管理 field を `applied` と比較する。

config.toml、Card、Volume、利用者管理の initialize script、.gitignore entry は保持する。

### `dembly inspect`

`inspect` は実行中 container を要求せず configuration を読む。

Deck root、selected service、管理 Compose file、lock state、Card identity、mount target、export、マージ済み PATH、Volume path、Host Bind、intended user、予定 Compose change を表示する。

### `dembly check`

`check` は valid lock を要求する。

同じ適用済み configuration で temporary Compose Runtime を生成し、全 selected Card check を実行してから temporary Runtime だけを削除する。

Volume data を保持する。

## 10. Docker Compose の契約

apply 成功後は Docker Compose だけを container lifecycle interface とする。

```bash
docker compose -f <managed-compose-file> up -d
docker compose -f <managed-compose-file> ps
docker compose -f <managed-compose-file> logs <service>
docker compose -f <managed-compose-file> exec --user <intended-user> <service> sh
docker compose -f <managed-compose-file> run --rm <service> <command...>
docker compose -f <managed-compose-file> down
```

`up` は Runtime 初期化後に保存済み original entrypoint と command を起動する。

`ps`、`logs`、`exec` は Runtime と同じ Compose project を観測または操作する。

`--user` を指定しない `exec` は root となる。
initializer が root であるためである。

`run` は command を Runtime Dembly へ渡し、保存済み original process を置換する。

`down` は container と network を削除するが、`.dembly/volumes/` を削除してはならない。

top-level Compose `name` は利用者管理であり必須とする。

Dembly はこれを検証するが変更してはならない。

`name` と異なる明示的な `-p` または `COMPOSE_PROJECT_NAME` は supported invocation contract の対象外とする。

## 11. Runtime plan format

```toml
schema_version = 1
lock_digest = "sha256:..."

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
PATH = "/opt/dembly/cards/clang/bin:/usr/local/bin"

[process]
argv = ["/usr/local/bin/start", "--watch"]
```

plan は解決済み Runtime path だけを含む。

未解決 variable と Host Bind secret content を含んではならない。

## 12. Card、mount、environment の規則

全 SquashFS file は Runtime 初期化前に `/run/dembly/cards/` へ read-only bind する。

Runtime mount 順序は Card SquashFS、Volume、Host Bind、export とする。

Deck private Volume は `.dembly/volumes/<name>` とする。

Card private Volume は `.dembly/volumes/<card>/<name>` とする。

shared Volume は `.dembly/volumes/<name>` とする。

Volume physical path は symlink であってはならない。

required Host Bind は存在しなければならない。

存在しない optional Host Bind は warning を出して skip する。

environment precedence は Base image、Compose service、config.toml、Card の順とする。

同じ normal environment variable を定義する 2 Card は error とする。

PATH は config order の Card entry、config entry、effective Base/Compose PATH の順に prepend する。

export target は `/usr/local/bin/` に限定し、競合してはならない。

hook は Card mount 後、final process 前に実行する。

hook failure は final process の起動を妨げなければならない。
