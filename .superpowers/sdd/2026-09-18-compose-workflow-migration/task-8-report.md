# Task 8 実装報告

## 結論

`CardIdentity`と`LockInput`をCoreへ追加し、canonical Lock digestと`dembly lock`を実装しました。

`lock`は検証済みのmanifest checksum、filesystem checksum、Docker image IDを`x-dembly.lock`へ原子的に保存します。

Compose全体のhashと独立した`deck.lock`は生成しません。

## 実装内容

- `LockInput::digest()`はdomain prefixと長さ区切りfieldからSHA-256を計算します。
- SHA-256のbytes実装は`checksum.rs`へ集約し、Lock identityから再利用します。
- Cardは`.dembly/config.toml`の宣言順を維持し、構造体やmapの直列化順には依存しません。
- Lockは正規化済みCompose path、対象service、不変image ID、Card名、version、manifest checksum、検証済みfilesystem checksumを保持します。
- `ManagedCompose`の埋込みLock型はCoreのidentity型を使用します。
- `dembly lock`は`HostPlan`による入力検証と既存applied stateの競合検証を完了してから、`atomic_replace`で管理対象Composeを置き換えます。
- Host `check`も同じ`LockInput::digest()`とLock解決処理を使用します。

## TDD証跡

identity testはCore APIが存在しない状態で失敗しました。

```text
error[E0432]: unresolved imports `dembly_core::CardIdentity`, `dembly_core::LockInput`
```

CLI testは未実装dispatcherに対して失敗しました。

```text
2 failed
dembly: command is not implemented yet
```

実装後は次のfocused testが成功しました。

```text
cargo test --offline -p dembly-core --test identity
4 passed; 0 failed

cargo test --offline -p dembly-cli --test lock
2 passed; 0 failed

cargo test --offline -p dembly-docker --test managed_compose
16 passed; 0 failed

cargo test --offline -p dembly-cli --test host_commands
12 passed; 0 failed
```

`lock_embeds_resolved_identities_and_is_idempotent`は同一fixtureで`dembly lock`を2回実行し、2回目のCompose bytesと標準出力に差分がないことを検証します。

## 検証結果

```text
cargo clippy --offline --workspace --all-targets -- -D warnings
exit 0

cargo fmt --all -- --check
exit 0

git diff --check
exit 0
```

`cargo test --offline --workspace`は既存の`crates/dembly-cli/tests/integration.rs`で完走しませんでした。

このsuiteは削除済みの`deck.toml`、`[base]`、`image_base`、`compose_base`、旧lifecycle commandを使用しており、移行planではTask 11が更新を担当します。

今回変更したfocused suiteと、Task 7のHost command回帰suiteは成功しています。

## 設計上の境界

Compose全体をdigestへ含めると、埋込みLock自身の更新が入力を変える自己参照になります。

そのためdigest入力を正規化済みpath、service、image ID、Card identityだけに限定しました。

Lockを管理対象Composeへ埋め込むため、Composeと別に更新順や整合性を管理する`deck.lock`は不要です。

SHA-256は小さいcanonical byte列へだけ適用します。

Card filesystemは従来の`sha256_file`で検証し、検証済みの宣言値をLockへ保存するため、SquashFS全体をdigest計算用に再読込しません。

## 根拠

- `design/spec.md` 7.5.1、9.4、11.2、11.3、14
- `design/superpowers/plans/2026-09-18-compose-workflow-migration.md` Task 8

## Fix round 1/5

### 原因

初回実装はSHA-256の定数、padding、compressionを`identity.rs`へ直接置いていました。

この配置はchecksum責務を分散させ、planの既存SHA-256実装を再利用する条件に違反していました。

### RED

canonical Lock bytesを共通bytes checksum APIへ渡し、`LockInput::digest()`と一致することを要求するtestを追加しました。

```text
cargo test --offline -p dembly-core --test identity
error[E0432]: unresolved import `dembly_core::sha256_bytes`
```

### 修正

- SHA-256の定数、padding、compression、hex化を`checksum.rs::sha256_bytes`へ移しました。
- `identity.rs`はcanonical bytesの組立てだけを担当し、共通bytes checksum APIを呼び出します。
- `sha256_file`の外部`sha256sum`実行と失敗処理、および`LockInput::digest() -> String`の契約は維持しました。

### GREEN

```text
cargo test --offline -p dembly-core --test identity
5 passed; 0 failed

cargo test --offline -p dembly-cli --test lock
2 passed; 0 failed
```
