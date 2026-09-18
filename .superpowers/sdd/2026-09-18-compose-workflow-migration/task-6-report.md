# Task 6 実装報告

## 結論

- `dembly-runtime`のRuntime計画をstrict Serde TOMLへ移行しました。
- 利用者指定は`runtime_user.spec`だけを保存し、コンテナ内の`/etc/passwd`と`/etc/group`から解決します。
- staged Host Bind、Card check、Card/Volume mount target検証のhelperを実装しました。
- CLI orchestrationとprivilege dropの配線はTask 10の範囲として実装していません。

## 実装内容

### Runtime計画

- `RuntimeConfig`を`schema_version`、`lock_digest`、`runtime_user`、`cards`、`binds`、`exports`、`hooks`、`checks`、`environment`、`process_argv`の構成へ変更しました。
- `#[serde(deny_unknown_fields)]`を全階層へ適用しました。
- `schema_version = 1`と空でない`lock_digest`をload/renderの双方で検証します。
- `BTreeMap`と構造体の宣言順によって決定的なTOMLを生成し、round-tripを検証します。

### container-local user resolution

- `user`、`uid`、`user:group`、`uid:gid`を解釈します。
- group省略時はpasswdのprimary GIDを採用します。
- 数値UIDにも対応するpasswd entryを必須とします。
- 名前付きgroupはgroup entryを必須とします。
- 空指定、余分なcolon、未解決entry、`u32`範囲外のUID/GIDを拒否します。
- `/etc` I/Oは`resolve_current_runtime_user`だけに置き、parserは文字列入力だけで試験可能です。

### staged Host Bindとmount target

- targetでは`${HOME}`と`${USER}`だけを展開します。
- staged sourceのfile種別に合わせ、directoryまたはfile targetを準備します。
- bind mount後、`ro`だけを`remount,bind,ro`します。
- エラーはBind index、source、展開後target、失敗operationを含みます。
- Card mount commandの`loop,ro`を維持しました。
- Composeから渡される全Volume targetを検証する`ensure_volume_targets_exist`を追加しました。

`RuntimeConfig`の指定フィールドにはVolume一覧が存在しないため、Volume target検証はtarget列を受け取る独立helperです。Task 9/10が生成・実行境界でこの列を渡す必要があります。

### Card check

- Card設定順でcheckを実行します。
- 各Cardのmount targetをcwdにします。
- 外部環境をclearし、計画済み環境と未指定時の`HOME`/`USER`だけを渡します。
- 失敗時は後続checkを実行せず、Card名、command、statusを返します。

## TDD証跡

### RED

```text
cargo test -p dembly-runtime
error[E0432]: unresolved import `dembly_runtime::resolve_runtime_user`
error[E0432]: unresolved import `dembly_runtime::render_runtime_config`
error[E0609]: no field `lock_digest` on type `RuntimeConfig`
error[E0609]: no field `binds` on type `RuntimeConfig`
error[E0609]: no field `checks` on type `RuntimeConfig`
```

旧Runtime計画にspec-only user、Lock digest、Bind、Checkがないことを確認しました。

### GREEN

```text
cargo test -p dembly-runtime
config: 3 passed
mount: 5 passed
setup: 3 passed
user: 4 passed
doc-tests: 0 failed
```

mount成功経路はfake `mount` executableを使用し、directory/file target作成、bind、ro remount、rw時のremount省略を実際のargvで検証しました。

## 自己レビュー

- 旧resolved user fieldsと`deck_name`をRuntime TOMLから除去しています。
- user解決はCard mountより先に呼べる純粋境界であり、解決失敗時のmount抑止をTask 10のrecording fakeで検証できます。
- unknown TOML fieldはroot、table、array-of-tableの各階層で拒否します。
- check参照先Cardをcommand実行前に全件検証します。
- mount失敗診断はsource/target/operationを失いません。
- read-only remount失敗後のunmount rollbackは仕様にないため実装していません。失敗したコンテナの破棄を前提とします。

## 検証

```text
cargo test -p dembly-runtime
15 passed; 0 failed

cargo clippy -p dembly-runtime --all-targets -- -D warnings
Finished successfully

cargo fmt --all -- --check
exit 0

git diff --check
exit 0
```

`cargo test --workspace --no-run`は、現行`crates/dembly-cli/src/main.rs`の旧Runtime orchestrationが削除済みの`runtime_user.uid/gid`を参照するため失敗します。Task 10で`resolve_current_runtime_user`、mount/setup、privilege drop、final execを一括配線する計画上の中間状態です。

## 根拠

- `design/spec.md` 7.6、10.1から10.5
- `design/superpowers/plans/2026-09-18-compose-workflow-migration.md` Task 6、Task 10
