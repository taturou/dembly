# Dembly

Dembly が Compose 環境へ Card filesystem を組み込むための用語集です。

## Language

**Host Dembly**:
Deck と Card の定義から Compose 設定を生成・更新するホスト側の実行主体。
_Avoid_: コンテナオーケストレータ、Compose ラッパー

**Runtime Dembly**:
コンテナ内で Card filesystem を mount し、実行環境を初期化する実行主体。
_Avoid_: Host Dembly、コンテナ操作 CLI

**Compose 運用**:
生成済みの Compose 設定に対して、利用者が Docker Compose CLI でコンテナの状態確認・起動・停止・実行を行う運用形態。
_Avoid_: Dembly lifecycle

**管理対象 Compose ファイル**:
Host Dembly が直接更新し、Runtime Dembly を起動する定義を含むユーザー作成の `compose.yaml`。
_Avoid_: 生成専用 Compose ファイル、ephemeral override

**Dembly 管理領域**:
`dembly apply` が所有して冪等に更新する Compose 定義の部分。利用者が管理する定義とは分離され、変更競合は失敗として扱う。
_Avoid_: service 全体の上書き、one-shot patch

**Dembly 適用状態**:
`compose.yaml` の `x-dembly` に保存する、管理領域の再適用・競合検出に用いる状態。元の field、前回適用値、lock を含む。
_Avoid_: ignore 対象の state file、ephemeral override

**Dembly lock**:
`compose.yaml` の `x-dembly.lock` に保存する、Base image と Card artifact の不変解決結果。
_Avoid_: `.dembly/lock.toml`、mutable runtime state

**Lock 更新**:
`dembly lock` だけが Dembly lock を更新し、`dembly apply` は最新かつ有効な lock を要求する契約。
_Avoid_: apply による暗黙 lock 更新、起動時の artifact 再解決

**適用基準値**:
管理 field ごとに `x-dembly` へ保持する初回の利用者定義 `original` と、前回 Dembly が書込んだ値 `applied` の組。Dembly は時系列履歴を保持しない。
_Avoid_: state history、Compose 全文 hash

**Runtime 計画**:
`.dembly/runtime/<service>.toml` に保存する、Runtime Dembly 用の解決済み実行計画。Compose はこの file を read-only bind mount する。
_Avoid_: user-authored configuration、ephemeral runtime metadata

**Dembly ignore 対象**:
`.dembly/runtime/` と `.dembly/volumes/` のような再生成可能または mutable な path。`dembly apply` が `.gitignore` へ追記する。
_Avoid_: `.dembly/` 全体の ignore、tracked file の自動 untrack

**Runtime binary**:
`dembly apply` が `.dembly/runtime/bin/dembly` へコピーする、当該 Runtime 計画と同世代の static Dembly executable。Compose はこれを read-only bind mount する。
_Avoid_: Base image への事前インストール、Host executable path の直接 bind

**Native Compose run**:
`docker compose run <service> <command...>` が Runtime Dembly の初期化後に実行する一時的な最終 process。`up` は Runtime 計画に保存した original process を実行する。
_Avoid_: Host Dembly の run command、original command への append

**Runtime 初期化権限**:
Runtime Dembly は常に root で開始し、Card の mount と setup を完了した後、元の entrypoint / command を解決済み intended user へ privilege drop して実行する契約。
_Avoid_: original process の root 実行、非 root initializer

**Dev Containers 接続 user**:
`.devcontainer/devcontainer.json` の必須 `remoteUser`。Dembly は Runtime の intended user と一致することを検証し、VS Code の terminal・server process を application process と同じ user にする。
_Avoid_: remoteUser の暗黙 root、Dembly による devcontainer.json 書換え

**Dev Containers container user**:
`.devcontainer/devcontainer.json` の `containerUser`。Dembly 環境では省略または root に限定し、Runtime Dembly の root 初期化を維持する。
_Avoid_: non-root containerUser、remoteUser との混同

**共通開発環境**:
人間の Dev Containers 起動と AI の native Compose 起動の双方で必要な環境。Compose 定義、Dockerfile、Dembly Card にのみ置き、Dev Containers 専用機能には依存しない。
_Avoid_: VS Code の一時 override への依存、AI 専用 Compose file

**AI の実行 user**:
AI が native Compose の `exec` で指定する intended user。Dev Containers の `remoteUser` と同じ値を使い、root は管理操作でだけ明示する。
_Avoid_: root を既定にした AI の exec、application process と異なる user

**Runtime 権限**:
Dembly 管理 Compose file の selected service にだけ設定する `privileged: true`。Runtime Dembly が root で kernel SquashFS を mount するための PoC 権限。
_Avoid_: 非 selected service への権限追加、production least privilege

**Host Dembly CLI**:
Deck の解決・検証・lock・Compose 適用を担う `validate`、`lock`、`apply`、`inspect`、`check`、`card build` の command 群。container lifecycle command は持たない。
_Avoid_: Host Dembly による container lifecycle command、Docker Compose の wrapper

**Dembly 初期化**:
`dembly init` が初期 `.dembly/config.toml` を生成する支援操作。設定の正本は生成後も人間が編集・Git 管理し、init は既存 config を上書きしない。
_Avoid_: 対話 I/F を唯一の設定正本にすること、既存 config の上書き

**初期化候補探索**:
`dembly init` が current directory 配下の `card.toml` と `devcontainer.json` を再帰探索して表示する操作。symlink directory、`.git/`、`.dembly/` はたどらず、候補の採用は人間が選択する。
_Avoid_: symlink の自動追跡、候補の暗黙選択

**Dev Containers service の採用**:
Dev Containers integration を採用する `dembly init` が、`devcontainer.json` に指定された service を表示して採用確認する操作。Dembly は別 service を選択・書換えない。
_Avoid_: Dembly と VS Code の異なる selected service、devcontainer.json の service 自動変更

**Compose Base core**:
Compose を唯一の基礎環境とする Dembly の core mode。native Docker Compose で container lifecycle を実行し、Dev Containers は追加の起動・検証 integration として扱う。
_Avoid_: Compose を使わない基礎環境、Dev Containers 必須

**Dembly 適用解除**:
`dembly unapply` が、適用値との競合がない場合に `x-dembly` の `original` を復元し、Dembly 管理 field と生成 Runtime file を削除する操作。
_Avoid_: 手動での部分復元、利用者変更の暗黙上書き

**Dev Containers 起動**:
VS Code Dev Containers が `.devcontainer/devcontainer.json` と Dembly 適用済み Compose 定義を用いて Runtime を作成する正規の起動経路。
_Avoid_: Dembly による container lifecycle 操作、VS Code の一時 override の管理

**Dev Containers integration**:
`.dembly/config.toml` の optional `devcontainer.path` で明示有効化する、Dev Containers 固有の Compose・user・lifecycle 設定の検証。
_Avoid_: devcontainer.json の自動探索、毎回の command line path 指定

**Host 初期化 script**:
利用者が管理する `.devcontainer/initialize-host.sh`。Dev Containers の `initializeCommand` から host 上で実行され、`dembly apply` と必要な前後処理を順序付きで実行する。
_Avoid_: Dembly による initializeCommand の自動合成、並列 lifecycle command

**Compose project name**:
`.devcontainer/compose.yaml` の top-level `name:` にだけ定義する Compose project の識別子。Dembly は値を変更せず、存在と妥当性を検証する。
_Avoid_: `.dembly/config.toml` との二重管理、directory 名からの暗黙導出

**Compose Base 選択**:
`.dembly/config.toml` の Compose file path と service が定める Dembly Runtime 化の対象。Dev Containers 使用時は `devcontainer.json` の `service` と一致し、設定した Compose file は `dockerComposeFile` に含まれなければならない。
_Avoid_: Dembly 管理 file の曖昧な選択、全 Compose file の書換え

**Dembly 管理 Compose file**:
`.dembly/config.toml` で明示した、`x-dembly` と Dembly 管理領域を置ける唯一の Compose file。`dockerComposeFile` の他 file に `x-dembly` があれば validation error とする。
_Avoid_: 配列最後の file の暗黙選択、複数 file への状態分散

**Dembly 管理 Compose file の優先順位**:
Dembly 管理 Compose file は `dockerComposeFile` 配列の最後に置く規則。Dembly が注入する selected service の設定を final effective configuration にする。
_Avoid_: 後続 Compose file による Runtime entrypoint の上書き、field 単位の merge 競合解析

**Deck root**:
`.dembly/` directory。Dembly 設定内の相対 path と `${DECK_ROOT}` の解決基準であり、プロジェクト root とは区別する。
_Avoid_: プロジェクト root、config.toml の親以外の directory

**設定探索**:
設定 path が省略された Dembly command は、current directory の `.dembly/config.toml` だけを使用する規則。任意位置の設定は `--config` で明示する。
_Avoid_: 親 directory 探索、曖昧な Deck 自動選択

**Card 配置**:
通常運用では `/var/lib/dembly/cards/` 等の絶対 path にある Card artifact。Deck root 相対の Card は Card 開発時の配置である。
_Avoid_: 常にプロジェクト内の Card
