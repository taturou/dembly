# Task 9 実装レポート

## 結果

- `apply` と `unapply` を CLI dispatch へ接続した。
- `build_runtime_config` と `build_managed_fields` を追加した。
- Runtime plan、実行ファイル、Volume、`.gitignore` を先に更新し、Compose を最後に atomic replace する順序を実装した。
- Lock 欠落・stale Lock・管理フィールド競合では書き込みを開始しない。
- `unapply` は全管理フィールドを検証してから Compose を復元し、`.dembly/runtime/` だけを削除する。Lock、Volume、設定、Card、`.gitignore` は保持する。
- Runtime と Volume のディレクトリ作成、および Runtime cleanup で symlink traversal を拒否する。

## TDD

RED:

- `cargo test -p dembly-cli --test apply`: 3件失敗。理由は `command is not implemented yet`。
- `cargo test -p dembly-cli --test unapply`: 2件失敗。理由は `command is not implemented yet`。

GREEN:

- `cargo test -p dembly-cli --test apply`: 3件成功。
- `cargo test -p dembly-cli --test unapply`: 2件成功。

## 検証

成功:

- `cargo test -p dembly-cli --test apply`
- `cargo test -p dembly-cli --test unapply`
- `cargo test -p dembly-cli --lib --bin dembly --test help --test host_commands --test host_plan --test init --test lock --test runtime`
- `cargo fmt --all -- --check`
- `git diff --check`

全体テストの既存阻害要因:

- `cargo test -p dembly-cli` は `tests/integration.rs` で完走しない。
- 同ファイルは現行 HEAD で削除済みの旧 `deck.toml`、`up`、`run`、`exec` 契約を前提としているため、静的検証を含む14件が失敗する。
- `card_build_interactive_creates_an_artifact_from_prompted_values` は、現行 HEAD の `main` が stdin を lock したまま `card_build` 内で再度 stdin を読むため、60秒超停止する。
- Task 9 の変更対象外であり、Task 9 対象および現行 Host/Runtime テストは上記コマンドですべて成功した。
