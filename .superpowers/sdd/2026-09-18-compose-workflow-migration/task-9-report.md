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

## Review round 1

- managed field の `entrypoint`、`command`、`user`、`privileged`、Lock digest label、mount を個別に外部変更し、Compose、Runtime plan、Runtime binary、Volume 内容・mtime、`.gitignore` が一切更新されない統合テストを追加した。
- artifact 生成失敗時の旧 Compose 維持、Runtime/Volume symlink 拒否、Lock digest label と entrypoint の正確値、Runtime plan → binary → Volume → `.gitignore` → Compose の更新順を検証した。
- `.gitignore` は既存バイト列、行順、改行、否定規則を保持し、不足規則だけを末尾へ追記するよう修正した。
- `dembly-core::resolve_compose_path` を公開し、`unapply` の重複 resolver を削除した。`${DECK_ROOT}` の展開前に変数構文を検証するため、Deck root 自体に `$` が含まれても処理できる。

RED:

- `.gitignore` 保持テストは既存 Volume 規則が末尾へ移動したため失敗した。
- `$` を含む Deck root の unapply テストは展開後 `$` の誤検出により失敗した。

GREEN:

- `cargo test -p dembly-cli --test apply --test unapply`: 10件成功。

## Review round 2

- mtime の `<=` 比較による順序テストを削除した。
- Runtime plan、Runtime binary、executable permission、Volume、`.gitignore`、Compose の対象書込みを一つの sequencer に集約した。
- production の native writer と test の recording writer が同じ sequencer を通る構造にした。
- recording writer が記録した実イベント列を完全一致で検証し、Compose replace が必ず最後であることを固定した。

RED:

- Compose replace を意図的に先頭へ移した mutation で、recording writer の先頭イベントが Compose となりテストが失敗した。

GREEN:

- `cargo test -p dembly-cli --lib artifacts::tests::apply_replaces_artifacts_in_runtime_binary_volume_ignore_compose_order -- --exact`: 1件成功。
- `cargo test -p dembly-cli --test apply`: 6件成功。
