# Final Fix Report

## 修正内容

- CLI の `--version` テストを `env!("CARGO_PKG_VERSION")` から改行付きで期待する形式に変更しました。
- インストーラーは XDG の `licenses` と `licenses/dembly` が symlink の場合、作成・コピー前に拒否します。外部 sentinel が変更されない統合テストを追加しました。
- Release の prerelease 判定は build metadata を除いた部分だけを調べ、`1.2.3+build-id` を stable として扱います。
- dry-run は品質ゲート失敗時にも detached worktree の登録を削除します。
- 通常 release の事前検査は untracked 非 ignored file を拒否し、ignored の `dist/` と `target/` は許可します。

## 実行した検証

- `bash scripts/test-release.sh`
- `bash scripts/test-dembly-install.sh`
- `mise exec -- cargo test --locked -p dembly-cli --test help global_version_is_bare_package_version`
- `bash -n scripts/release.sh scripts/dembly-install.sh.in scripts/test-release.sh scripts/test-dembly-install.sh`
- `shellcheck scripts/release.sh scripts/dembly-install.sh.in scripts/test-release.sh scripts/test-dembly-install.sh`
- `mise exec -- cargo test --locked`
- `bash scripts/release.sh --dry-run`

すべて exit code 0 でした。
