# Task 3 実装レポート

## Status

- Host CLI を `args.rs` の parser と dispatcher へ移し、公開面を `init`、`validate`、`lock`、`apply`、`unapply`、`inspect`、`check`、`card build` に限定しました。
- `init` は候補を番号で選択し、Card と Dev Containers の採用を明示確認してから strict `ConfigDocument` を決定的に生成します。
- config の既存ファイルは `symlink_metadata` と `create_new` の両方で拒否し、Compose、Dev Containers、Lock は変更しません。

## RED

実行コマンド:

```bash
CARGO_HOME=/home/f-shigemitsu/.cargo \
RUSTC=/home/f-shigemitsu/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin/rustc \
RUSTDOC=/home/f-shigemitsu/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin/rustdoc \
/home/f-shigemitsu/.rustup/toolchains/nightly-x86_64-unknown-linux-gnu/bin/cargo \
test --offline -p dembly-cli --test help
```

結果:

```text
error[E0433]: could not find `Base` in `dembly_core`
error[E0425]: cannot find function `discover_deck` in crate `dembly_core`
error[E0425]: cannot find function `read_lock` in crate `dembly_core`
... 旧 Deck/lifecycle helper による 79 errors
```

Task 1 が旧 Core API を削除済みである一方、旧 `main.rs` はその API と旧 public lifecycle dispatch を参照していたため、新しい parser/command surface の test はコンパイル前に失敗しました。

通常の Cargo は crates.io への HTTP 407 で停止するため、先行 Task と同じ共有 offline registry/toolchain を使用しました。

## GREEN

実行コマンド:

```bash
CARGO_HOME=/home/f-shigemitsu/.cargo \
RUSTC=/home/f-shigemitsu/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin/rustc \
RUSTDOC=/home/f-shigemitsu/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin/rustdoc \
/home/f-shigemitsu/.rustup/toolchains/nightly-x86_64-unknown-linux-gnu/bin/cargo \
test --offline -p dembly-cli --test help

CARGO_HOME=/home/f-shigemitsu/.cargo \
RUSTC=/home/f-shigemitsu/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin/rustc \
RUSTDOC=/home/f-shigemitsu/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin/rustdoc \
/home/f-shigemitsu/.rustup/toolchains/nightly-x86_64-unknown-linux-gnu/bin/cargo \
test --offline -p dembly-cli --test init

CARGO_HOME=/home/f-shigemitsu/.cargo \
RUSTC=/home/f-shigemitsu/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin/rustc \
RUSTDOC=/home/f-shigemitsu/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin/rustdoc \
/home/f-shigemitsu/.rustup/toolchains/nightly-x86_64-unknown-linux-gnu/bin/cargo \
test --offline -p dembly-cli --no-run

cargo fmt --all -- --check
git diff --check
```

結果:

```text
help: 5 passed; 0 failed
init: 7 passed; 0 failed
dembly-cli --no-run: help/init/integration を含む test target の compile 成功
cargo fmt --check: exit 0
git diff --check: exit 0
```

## 変更

- `crates/dembly-cli/src/args.rs`: `HostCommand`、`HostContext`、typed `CliError`、`--config <path>` 専用 parser を追加しました。
- `crates/dembly-cli/src/commands/init.rs`: scripted stdin/stdout で試験可能な候補選択、確認、TOML rendering、strict parser による生成前検証、non-overwrite write を追加しました。
- `crates/dembly-cli/src/main.rs`: public help と新 dispatcher を導入し、旧 public lifecycle dispatch を除去しました。hidden `__runtime` branch と Card build は維持しています。
- `crates/dembly-cli/tests/help.rs`、`tests/init.rs`: parser、help、hidden runtime、Card build、interactive init の focused regression coverage を追加・更新しました。

## 自己レビュー

- `deck.toml` positional、重複 `--config`、未知 option、値なし `--config` は top-level adapter から exit 2 になります。
- Card の番号選択は入力順を保持し、採用前に duplicate name を検出します。各 Card 候補は1件でも `y/yes` を要求します。
- Dev Containers 採用時は JSON の service と `dockerComposeFile` 全順序を表示し、最後の Compose file を config parent 相対 path として記録します。
- default/custom config の親を必要時だけ作成し、Card/Dev Containers path は config parent から lexical relative path に変換します。

## 懸念

- `validate`、`lock`、`apply`、`unapply`、`inspect`、`check` は parser が受理しますが、実装は後続 Task のため現時点では typed "command is not implemented yet" error (exit 2) です。
- Task 1 が旧 Core API を削除済みのため、コンパイル不能な旧 lifecycle helper 群を `main.rs` から除去しました。旧 helper を保持するという計画上の順序とは整合しませんが、旧 Core interface を復活させず新 shell を build 可能にするために必要でした。

## Fix round 1/5

### 原因と修正

- 手動入力したCompose pathは文字列のままconfigへ書き込まれ、cwdを基準とする入力規約とconfig parentを基準とするConfigDocumentの解釈がずれていました。
- `init`にはCompose file/serviceを生成前に検証する経路がなく、手動入力とDev Containers選択のどちらでも無効な設定を作成できました。
- 手動入力はcwd基準でabsolute pathへ解決し、config parentからの相対pathへ変換してから書き込みます。
- Dev Containers選択は`dockerComposeFile`全順序のYAMLを読み、いずれかの`services` mappingに選択serviceが存在することを検証します。Docker CLIや後続Task 5のCompose config pipelineは呼びません。
- すべてのCompose path/service検証はconfig親directory作成と`create_new`より前に実行します。

### RED

実行コマンド:

```bash
CARGO_HOME=/home/f-shigemitsu/.cargo \
RUSTC=/home/f-shigemitsu/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin/rustc \
RUSTDOC=/home/f-shigemitsu/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin/rustdoc \
/home/f-shigemitsu/.rustup/toolchains/nightly-x86_64-unknown-linux-gnu/bin/cargo \
test --offline -p dembly-cli --test init
```

結果:

```text
10 tests 中 4 failed:
- default/custom config は cwd の compose.yaml ではなくconfig parent配下を参照した
- 手動入力の存在しないCompose fileが exit 0 でconfigを生成した
- Dev Containersの存在しないCompose fileが exit 0 でconfigを生成した
```

### GREEN

実行コマンド:

```bash
CARGO_HOME=/home/f-shigemitsu/.cargo \
RUSTC=/home/f-shigemitsu/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin/rustc \
RUSTDOC=/home/f-shigemitsu/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin/rustdoc \
/home/f-shigemitsu/.rustup/toolchains/nightly-x86_64-unknown-linux-gnu/bin/cargo \
test --offline -p dembly-cli --test init

CARGO_HOME=/home/f-shigemitsu/.cargo \
RUSTC=/home/f-shigemitsu/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin/rustc \
RUSTDOC=/home/f-shigemitsu/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin/rustdoc \
/home/f-shigemitsu/.rustup/toolchains/nightly-x86_64-unknown-linux-gnu/bin/cargo \
test --offline -p dembly-cli --test help

cargo fmt --all -- --check
+git diff --check
```

結果:

```text
init: 10 passed; 0 failed
help: 5 passed; 0 failed
cargo fmt --check: exit 0
git diff --check: exit 0
```

### 追加coverage

- default/custom `--config`の両方について、生成configを`resolve_deck`へ渡し、cwd上のCompose fileへ解決されることを確認しました。
- 手動入力とDev Containers選択の双方で、存在しないCompose fileと存在しないserviceがconfig未作成のままexit 2となることを確認しました。
