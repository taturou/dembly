# Task 10 実装報告

## 結論

- 非公開Runtimeコマンドを`runtime.rs`へ分離し、`init`と`check`を実装しました。
- `RuntimeSystem`境界とrecording fakeにより、root setup、利用者切替、最終execの厳密な順序を検証しました。
- Compose `run`の追加argvは保存済みargvへ追加せず、完全に置き換えます。
- `__runtime probe`を削除し、公開helpには内部コマンドを表示しません。

## 実装内容

### Runtime `init`

- 実効UIDを最初に確認し、非root時は計画、passwd/group、mountへ進みません。
- Runtime計画のschemaとLock digestを検証してから、コンテナ内の`/etc/passwd`と`/etc/group`で指定利用者を解決します。
- Card mount、mount target確認、Host Bind、export、post-mount hookをrootで順次実行します。
- 計画済み環境を設定後、`setgid`、`setuid`の順に権限を変更します。
- Compose `run` argvがある場合はそれを、ない場合は`process.argv`を`CommandExt::exec`で最終processにします。
- setupの各エラーにはoperationとCard名またはsource/target pathを付加し、権限変更とexecを抑止します。

### Runtime `check`

- outer `init`がsetupと権限変更を完了済みであることを前提とし、再mountと再度のprivilege dropを行いません。
- 計画を再検証し、利用者をコンテナ内account databaseから解決します。
- Card設定順、各Card内のcheck宣言順で実行し、最初の失敗で停止します。
- check command、Card名、原因をエラーへ含めます。

### CLI境界

- `main.rs`の旧Runtime実装、`__runtime probe`、passwd/group probe helper、旧privilege-drop helperを削除しました。
- `__runtime init <plan> [compose-run-argv...]`と`__runtime check <plan>`以外はexit 2にします。
- 公開helpは`__runtime`を表示しません。
- production sourceにDocker user probe実装が残っていないことを検索で確認しました。

## TDD証跡

### RED

```text
cargo test -p dembly-cli --test runtime
error[E0432]: unresolved import `dembly_cli::runtime`
error[E0609]: no field `uid` on type `RuntimeUserSpec`
error[E0609]: no field `gid` on type `RuntimeUserSpec`
```

Runtime orchestrationの注入境界が存在せず、旧CLIが新しいspec-only user計画と非互換であることを確認しました。

### GREEN

```text
cargo test -p dembly-cli --test runtime
7 passed; 0 failed
```

次のmutationを検出する期待値を持ちます。

- user解決をmount後へ移動する。
- Card/Bind/export/hook失敗後にprivilege dropまたはexecを続行する。
- `setuid`を`setgid`より先に実行する。
- Compose `run` argvを保存済みargvへ追加する。
- Runtime `check`でCardを再mountする、またはCard順を無視する。

## 自己レビュー

- `RuntimeSystem`はoperation単位の細粒度な境界です。API面積は増えますが、mountやsetuidを実行せず失敗順序を決定的に検証できます。
- Native adapterのcheckは既存`dembly_runtime::run_checks`へ1件ずつ委譲します。設定cloneのコストはありますが、check process起動に比べて無視でき、実行順序の所有者をCLI orchestrationへ固定できます。
- 環境変数はsingle-threadのRuntime初期化processへ設定し、直後にprivilege dropと`exec`を行います。Host CLI経路からは呼ばれません。
- Native `exec`の成功時はprocessが置換されるため戻りません。戻る経路は失敗だけで、共通CLI境界がexit 2へ変換します。
- Task 9のRuntime TOML生成形式は変更していません。

## 検証

```text
cargo test -p dembly-runtime
17 passed; 0 failed

cargo test -p dembly-cli --test runtime
7 passed; 0 failed

cargo test -p dembly-cli --test help
5 passed; 0 failed

cargo clippy -p dembly-cli --tests -- -D warnings
Finished successfully

cargo fmt --all -- --check
exit 0

git diff --check
exit 0
```

## 根拠

- `design/spec.md` 8、10.1から10.6
- `design/superpowers/plans/2026-09-18-compose-workflow-migration.md` Task 10
