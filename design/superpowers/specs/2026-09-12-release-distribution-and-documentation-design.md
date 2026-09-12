# Dembly の配布、リリース、公開文書の設計

## 目的

Dembly を Linux x86_64 向けの GitHub Release 配布物として提供し、利用者がリポジトリーを clone せずにインストールできるようにします。
公開文書を英語中心に再構成し、日本語版を完全な対訳として維持します。

## 対象と非対象

対象は、CLI のバージョン表示、Cargo ワークスペースのバージョンとライセンス設定、ローカル実行のリリーススクリプト、インストーラー、公開 README、例、および配布ライセンスです。
GitHub Actions は使用しません。
実際の GitHub Release 作成、タグ push、または公開は実装作業では実行しません。
性能測定の結果、ベンチマーク手順、および性能評価節は README に記載しません。

## 成果物と責務

| 成果物 | 責務 |
| --- | --- |
| `scripts/release.sh` | 検証、Linux musl バイナリーのビルド、アーカイブ作成、チェックサム作成、タグと GitHub Release の公開を一貫して行います。 |
| リリース tarball | `dembly-vX.Y.Z-x86_64-unknown-linux-musl/` の下に `dembly` と `LICENSE` を含めます。 |
| `dembly-install.sh` | リリース asset を検証して配置し、実行中のバージョンを symlink で切り替えます。 |
| `README.md` | 英語の公開入口です。 |
| `README-ja.md` | 英語版と同じ内容を日本語で提供するレビュー用の完全版です。 |
| `examples/*/README.md` | 例に固有の英語説明です。 |

## バージョンと Cargo メタデータ

ルートの `Cargo.toml` の `[workspace.package].version` を唯一のバージョン定義とします。
すべての member crate は個別の `version` を持たず、`version.workspace = true` を使用します。
リリース対象の member crate は `license-file.workspace = true` と `publish.workspace = true` を使用します。
ルートの workspace package は `license-file = "LICENSE"` と `publish = false` を定義します。

`dembly --version` は `env!("CARGO_PKG_VERSION")` を用い、接頭辞なしの厳密な `X.Y.Z` だけを標準出力に出します。
既存の subcommand に渡す `--version` の意味は変更しません。

バージョン引数を指定しない `scripts/release.sh` は root version を使用します。
`scripts/release.sh X.Y.Z` は strict SemVer（`v` 接頭辞なし、prerelease 可）を受け付け、root version を更新して `Cargo.lock` を再生成します。
この場合にコミットしてよい追跡対象ファイルは root `Cargo.toml` と `Cargo.lock` だけです。
コミット形式は `chore(release): prepare vX.Y.Z` とし、本文は日本語で更新理由と内容を簡潔に記載します。
検証に失敗した場合、この二ファイルの未コミット変更は残します。

## ライセンス

ルートの `LICENSE` は次の本文を一字も変更せずに含みます。

```text
Copyright (c) 2026 Morishita, Hiroshi
All rights reserved.

The source code in this repository is made publicly available solely for viewing and forking purposes on GitHub.

Any unauthorized copying, modification, redistribution, or commercial use of this software, in whole or in part, without the prior written permission of the copyright holder is strictly prohibited.
```

上記の GitHub の行は、実際の `LICENSE` では文末に半角スペースを 1 文字含めます。

MIT 表記や OSS ライセンスであるとの説明を残しません。
インストーラーは `${XDG_DATA_HOME:-$HOME/.local/share}/licenses/dembly/LICENSE` にもこのファイルを配置します。

## リリース手順

リリース対象は `x86_64-unknown-linux-musl` のみです。
Rust 1.85.1 は `.mise/config.toml` に従います。
スクリプトは toolchain、対象、`gh`、`tar`、`gzip`、`sha256sum`、および GitHub CLI 認証を事前検査します。
不足する toolchain、target、component は自動インストールしません。

通常の公開前提条件は、clean worktree、`main`、`origin/main` と同じ HEAD、未使用の version/tag/release です。
品質ゲートは `scripts/check-linux.sh`、`cargo fmt --check`、clippy、全 test、release build とします。
Cargo を呼ぶ全コマンドは `mise exec -- cargo ... --locked` の形で lockfile の暗黙変更を禁止します。

スクリプトは version commit が必要なら先に `origin/main` へ push し、その commit に annotated unsigned tag `vX.Y.Z` を作って push します。
その後、GitHub Release を即時公開します。
タイトルは `Dembly vX.Y.Z` とし、英語の固定本文に Linux x86_64、version 固定インストーラー、および LICENSE link を記載します。
GitHub generated notes は使用しません。
prerelease version は GitHub Release も prerelease とします。
tag push 後に Release 作成または asset upload が失敗しても、tag を自動削除しません。
再実行手順を表示します。

`--dry-run` は一時 Git worktree 内で version update、品質ゲート、ビルド、パッケージングを行います。
commit、tag、push、GitHub Release は行わず、元の worktree を変更しません。

`dist/` は gitignore し、作成済み artifact を保持します。
artifact は version、target、HEAD SHA、SHA-256 を `.build-info` に記録します。
同じ artifact を再利用できるのは manifest が現在の HEAD と一致する場合だけです。
`--clean X.Y.Z` は指定 version と唯一の target に対応する tarball、checksum、build info の三つだけを明示的に削除します。
wildcard と `dist/` 全体の削除はしません。

## インストーラー

各 Release に version を埋め込んだ `dembly-install.sh` を asset として upload します。
そのため、以下の両方を提供できます。

```sh
curl -fsSL https://github.com/taturou/dembly/releases/latest/download/dembly-install.sh | sh
curl -fsSL https://github.com/taturou/dembly/releases/download/v1.2.3/dembly-install.sh | sh
```

`latest` は GitHub Releases の redirect です。
インストーラーは自分自身ではなく、ダウンロードした tarball を対応する `.sha256` asset で検証します。

実体は `${XDG_DATA_HOME:-$HOME/.local/share}/dembly/releases/<version>/{dembly,LICENSE}` に保存します。
公開コマンド `${HOME}/.local/bin/dembly` は active version の binary を指す symlink です。
既存 symlink は古い version のインストール時を含め常に更新します。
同名の通常ファイルまたは directory は変更せず失敗します。

同じ version の配置先が既にあり、binary の `--version` と LICENSE が有効なら再コピーせず symlink だけを更新します。
`--list` は保存済み version と active version を表示します。
`--uninstall <version>` は active version を拒否し、指定 release directory だけを削除します。
`--purge` は Dembly の release directory、管理 symlink、共有ライセンスだけを削除します。

## 公開文書

`README.md` と `README-ja.md` は相互リンクを持ち、内容と見出し順を対応させます。
日本語版は root の `README-ja.md` のみとします。
README の順序は Language、Overview、Security warning、Installation、Compose Quickstart、Architecture、Core concepts、Base types、Cards、Deck configuration、CLI reference、Development、Limitations and evaluation、License です。

Security warning は runtime の privileged 実行、Card を信頼済みコードとして扱うこと、root hook、Host Bind、untrusted Card sandbox が未提供であることを明記します。
Compose の `up` と `down` は `<deck-root>/compose.yaml` の同一 project に属する全 service だけへ影響し、host 上の無関係な Docker container へ影響しないことを操作説明内で記載します。

Quickstart は `examples/compose-base` のみを使います。
利用者は GitHub 上の `compose.yaml` と `deck.toml` を取得し、`docker compose pull`、`dembly validate`、`dembly lock`、`dembly up`、非対話的な `dembly exec -- /bin/echo compose-runtime`、`dembly down` を実行します。
必要条件は Linux x86_64、Docker Engine/daemon、Compose v2 です。
リポジトリー clone、release と example の version 対応、source build は利用者へ要求しません。

Architecture は Mermaid で Base 種別、Deck、Card、Volume、Host、Runtime の関係を示します。
Host は SquashFS を mount せず、Runtime が mount することを図と説明で明確にします。
機能説明では Image Base と Compose Base をともに扱い、lifecycle の相違を表で示します。
Card、Volume、Bind、Environment、lock の独立 snippet と、`<replace-me>` を含む全体 Compose Base template を示します。
Card build は実在する syntax/options と非対話例を主に記載し、対話入力と既定値は簡潔に補足します。
CLI reference は全 public command、side effect、失敗条件、`run`/`exec` の `-- <command...>`、bare `--version` を含みます。

開発節は mise を使うセットアップと format、clippy、test、release build を明記します。
`scripts/check-linux.sh` は完全環境の検査として扱います。
性能評価は掲載しません。
ディスク使用量は測定値ではなく模式的 artifact layout 表として、Base、Base + ATfEP、Base + Clang、Base + ATfEP + TIS の四ケースを conventional layout と Dembly artifact layout で比較します。

`examples/compose-base/README.md` は実装予定などの古い説明を削除します。
`examples/image-base/README.md` は fixture image build、Card build、validate、lock、および `/bin/true` による非対話実行を記載します。

## 検証

CLI の `--version` は出力形式と subcommand 非干渉を integration test で検証します。
release script と installer は shellcheck 相当の静的確認と、可能な範囲の一時 directory を使う shell test で destructive scope、version、checksum、symlink の挙動を検証します。
README の command、path、example 名、license link は実際の実装と照合します。
最終確認では formatter、clippy、全 test、`scripts/check-linux.sh`、release build を実行します。
