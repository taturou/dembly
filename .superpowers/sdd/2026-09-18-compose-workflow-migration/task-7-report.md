# Task 7 実装報告

## 結論

`validate`、`inspect`、Host側の`check`を実装しました。

3コマンドはすべて`HostPlan::resolve`を唯一の解決経路として使用し、入力ファイルを更新しません。

## 実装内容

- `validate`
  - Deck、Card filesystem、Compose、Dev Containers、Docker Compose/imageの読み取り専用検証を実行します。
  - optional Host Bindの警告を標準エラーへ出力し、成功時は解決済み設定の検証結果を標準出力へ出力します。
- `inspect`
  - config、Compose project/path/service、intended user、Card、Card/Volume/Bind mount、環境変数、Lock、apply/conflict状態を決定的なテキストとして出力します。
  - 保存済み適用状態の管理field競合は失敗させず、`apply: conflict`と`conflicts:`へ可視化します。
- Host `check`
  - 現在解決したLock inputと埋込みLockを照合し、欠落またはstale Lockを`dembly lock`へ誘導して拒否します。
  - applied stateの存在と、entrypoint/command/user/privileged/label/mountの全管理field一致を検証します。
  - Runtime TOML、Lock digest、intended user、Card名/マウント先、実行可能なRuntime binaryを読み取り専用で検証します。
  - Cardの`[check]`は実行しません。成功時にnative `docker compose run <service> __runtime check ...`を案内します。
- `ManagedCompose::validate_applied_state`
  - apply/unapplyとHost checkが同じ管理field照合ロジックを使用できる読み取り専用APIを追加しました。

## TDD 証跡

最初に`host_commands`を追加し、未実装dispatcherに対して以下を確認しました。

```text
cargo test -p dembly-cli --test host_commands
9 failed
dembly: command is not implemented yet
```

次にhelp文言の回帰テストを追加し、旧文言がHost checkによるCard実行を誤って示すことを確認しました。

```text
cargo test -p dembly-cli --test help public_help_lists_host_commands_without_legacy_lifecycle_commands -- --exact
FAILED
check must not promise to execute Runtime Card checks
```

## 検証結果

```text
cargo fmt --all -- --check
exit 0

cargo test -p dembly-cli --test host_commands
9 passed; 0 failed

cargo test -p dembly-cli --test host_plan
7 passed; 0 failed

cargo test -p dembly-cli --test help
5 passed; 0 failed

cargo test -p dembly-docker
24 passed; 0 failed

cargo clippy -p dembly-cli --all-targets -- -D warnings
exit 0

git diff --check
exit 0
```

`host_commands`は各成功・失敗ケースの前後でプロジェクト領域を再帰スナップショットし、変更がないことを確認します。
fake Dockerのargvも`docker compose -f <managed> config --format json`と`docker image inspect <reference>`だけであることを確認します。

## 設計上の境界

- Lockのcanonical digest生成とLock更新はTask 8の責務です。Task 7は既存の埋込みLockが現在の解決結果と一致するかだけを判定します。
- Runtime成果物の生成・更新はTask 9の責務です。Task 7は存在する成果物を読み取り専用で検証します。
- Card checkの実行はRuntimeの責務です。Host checkはコンテナ生成・起動・exec・Card command実行を行いません。
