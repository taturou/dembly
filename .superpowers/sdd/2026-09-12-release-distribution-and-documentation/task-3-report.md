# Task 3 実装報告: ローカルリリースオーケストレーション

## 実装結果

- `scripts/release.sh` を追加した。
- `scripts/test-release.sh` を追加した。
- 公開対象は `x86_64-unknown-linux-musl` のみである。
- 対応する呼出しは `release.sh`、`release.sh X.Y.Z`、`release.sh --dry-run [X.Y.Z]`、`release.sh --clean X.Y.Z` である。

## Red/Green の証跡

### Red

実装前に次を実行した。

```console
$ bash scripts/test-release.sh
test-release: release script is missing: /home/h-morishita/job/dembly/main/scripts/release.sh
```

テストは存在しない `scripts/release.sh` を検出して exit 1 になった。

### Green

実装後に次を実行した。

```console
$ bash scripts/test-release.sh
release: invalid SemVer: 1.2
release: existing artifacts do not match current commit; run: scripts/release.sh --clean 1.2.3
test-release: PASS
```

テストは一時 Git repository、bare `origin`、偽装 `mise`、偽装 `gh`、偽装 Cargo build を使用する。
ネットワークアクセス、実 GitHub Release、実 tag、実 push は行わない。
次を検証する。

- invalid SemVer を拒否する。
- `--clean 1.2.3` が対象 tarball、checksum、build-info の三ファイルだけを削除する。
- `--dry-run 1.2.3` が呼出し元の `Cargo.toml` と tag を変更しない。
- 現在の commit/checksum と不一致の build-info がある場合、artifact を上書きせず停止する。

## 実行した検証

```console
$ bash -n scripts/release.sh
$ bash scripts/test-release.sh
test-release: PASS

$ shellcheck scripts/release.sh scripts/test-release.sh
# diagnostics なし

$ bash scripts/release.sh --dry-run
# detached temporary worktree で check-linux、fmt、clippy、全 test、musl release build を完走
```

dry-run の品質ゲートでは Rust test 43 件を含む workspace test と musl release build が成功した。
`cargo fmt` は Cargo の仕様上 `--locked` を受理せず dependency resolution もしないため、このコマンドだけ `--locked` を指定しない。
clippy、test、release build には `--locked` を指定する。

## 公開安全保証

- dry-run は `mktemp -d` 配下の detached Git worktree だけで version update、品質ゲート、package を実行する。
- dry-run は commit、tag、push、`gh auth status`、`gh release view`、`gh release create` を実行しない。
- 通常モードは clean `main`、`HEAD == origin/main`、必須コマンド、mise target/component、GitHub CLI 認証を要求する。
- 通常モードは既存の local/remote tag と GitHub Release を拒否する。
- 引数 version は strict SemVer（`v` 接頭辞不可、prerelease/build metadata 可）だけを受け付ける。
- 明示 version は root `[workspace.package]` の `version` だけを section-aware `awk` で変更し、lockfile 再生成後に変更対象が root `Cargo.toml` と `Cargo.lock` だけであることを検査する。
- version commit は `chore(release): prepare vX.Y.Z` と日本語本文で `main` に push する。
- package は binary と root `LICENSE` を `dembly-vX.Y.Z-x86_64-unknown-linux-musl/` に格納し、SHA-256 と version/target/commit/checksum の build-info を生成する。
- 既存 artifact は build-info と checksum が現在の HEAD に一致する場合だけ再利用する。不一致なら削除を要求して停止する。
- `--clean` は wildcard を使用せず、指定 version/target の tarball、checksum、build-info だけを `rm -f` する。
- tag push 成功後の Release 作成・upload 失敗時、tag を削除しない。`gh release create` の再実行案内を stderr に出す。

## 変更ファイル

- `scripts/release.sh`
- `scripts/test-release.sh`

## 自己レビュー

- invocation の形、SemVer、dry-run 隔離、clean の削除範囲、artifact manifest、version update 範囲、公開失敗時の tag 保持を仕様と照合した。
- `shellcheck` の SC2015 を検出し、worktree clean check を明示的な `if` に修正した。
- `git diff --check` を実行し、whitespace error がないことを確認した。

## 懸念事項

- 通常公開モードは意図的に実行していない。現在の `main` は `origin/main` より先行しており、さらに実 tag/push/Release を行わないという Task 3 の制約に従うためである。
- GitHub API の実レスポンスは偽装 `gh` で境界を検証した。認証済み実アカウントによる公開操作の事前確認は、実際のリリース担当時に通常モードの preflight で行う。
