# Task 12 実装報告

## 結論

- runnable Compose 例、英語版 README、日本語版 README、traceability を canonical `design/spec.md` の native Compose workflow へ移行しました。
- 旧設定 file と旧例を削除し、`.dembly/config.toml`、トップレベル Compose `name`、同一 file 内の `x-dembly`、利用者管理の Dev Containers 初期化 script を追加しました。
- `design/spec.md` は変更していません。

## 前提の修正

Task brief は日本語版を `README.ja.md` と記載していますが、repository の公開契約、相互 link、文書検査、過去の release spec は `README-ja.md` を正本としています。

既存 URL を壊さないため `README-ja.md` を更新し、Task 12 の scan は実在する file 名へ読み替えました。

## 実装内容

### Compose 例

- `examples/compose-base/compose.yaml` に `name: dembly-compose-example` と初期 `x-dembly.schema_version` を置きました。
- `.dembly/config.toml` は同じ `compose.yaml` の `dev` service を選択し、`.devcontainer/devcontainer.json` を明示します。
- Dev Containers と native Compose は同じ一つの Compose file を同じ順序で使用します。
- `.devcontainer/initialize-host.sh` は `validate` 後に `apply` を実行し、missing または stale Lock の診断を検出した場合だけ利用者へ `lock` の手動実行を案内します。
- Dembly は `initializeCommand` と Host 初期化 script を生成または書換えません。
- 例は Alpine の root fallback を指定利用者として使い、`remoteUser=root`、`containerUser=root`、`exec --user root` を一致させました。
- 非 root の privilege drop は `native_compose_owns_applied_runtime_lifecycle` が UID 10001 で検証します。

### README

- `init`、`validate`、`lock`、`apply` の後に native Compose の `up`、`ps`、`logs`、`exec --user`、`run`、Runtime check、`down` を実行し、最後に `unapply` する順序へ統一しました。
- `unapply` は `down` の後だけ実行し、実行中 container の検出や停止を行わないことを明記しました。
- すべての Compose command で Dev Containers と同じ順序の `-f` 列を使い、`-p` と `COMPOSE_PROJECT_NAME` でトップレベル `name` を置換しない契約を明記しました。
- Lock と適用状態は、別 file ではなく管理対象 Compose file の `x-dembly` へ保存する説明に更新しました。
- 英語版と日本語版の見出し、command、制約、CLI reference を対応させ、日本語 prose は一文ごとに改行しました。

### Traceability

- `design/spec.md` 14節の20受入条件を AC-14-01 から AC-14-20 へ順番に割り当て、現在の unit、CLI、integration test 名へ対応付けました。
- 旧仕様だけを表す REQ 行と IT 行は削除しました。
- Docker Engine、Docker Compose plugin、Docker daemon 利用権限、privileged container、loop device、kernel SquashFS、`mksquashfs`、musl target を完全 integration の前提として記録しました。
- Dev Containers CLI は任意 integration の追加前提として分離しました。

### 文書検査

- `scripts/test-documentation.sh` を新 workflow、例の tracked inputs、旧語の拒否に更新しました。
- 文書更新前に `dembly init` 欠落で RED を確認し、更新後に `documentation assertions passed` の GREEN を確認しました。

## 仕様 2〜14 節の照合

- 2節：Linux、Docker Engine、Compose plugin、root/privileged、loop/SquashFS、任意 Dev Containers CLI の前提を README と traceability に記録しました。
- 3節：Deck の正本を `.dembly/config.toml` とし、Card を `card.toml` と `rootfs.squashfs` の組として説明しました。
- 4節：Host Dembly を設定 compiler、Runtime Dembly を root initializer、Docker Compose を lifecycle 所有者として分離し、所有権を文書化しました。
- 5節：AI の native Compose workflow と人間の Dev Containers workflow が同じ Compose file、service、project 名を使う例を提供しました。
- 6節：Lock、apply、Compose 起動、Compose down、unapply の状態順序を command 列で固定しました。
- 7節：tracked config、managed Compose、Dev Containers config、Host 初期化 script と、生成 Runtime artifact の区別を説明しました。
- 8節：公開 Host command と internal Runtime command を分離し、Runtime check だけを要求された native Compose command から呼び出します。
- 9節：全公開 Host command の入力形式、作用、主要 failure mode を CLI reference へ反映しました。
- 10節：同じ `-f` 列による `up`、`ps`、`logs`、`exec`、`run`、Runtime check、`down` と指定利用者を文書化しました。
- 11節：競合検出、atomic update、Lock と apply の冪等性、unapply の復元を traceability の現行 test へ対応付けました。
- 12節：selected service の privileged 実行、root initializer と hook、信頼済み Card、Host Bind review を警告しました。
- 13節：`schema_version = 1`、未知 field と未対応 schema の拒否を CLI reference と traceability へ反映しました。
- 14節：20受入条件を省略せず AC-14-01 から AC-14-20 へ対応付けました。

## 実行可能な例の検証

tracked example を一時 directory へ複製し、配布形態と同じ static musl release binary で次を実行しました。

```text
cargo build --release --target x86_64-unknown-linux-musl -p dembly-cli
docker compose -f compose.yaml pull
dembly validate
dembly lock
dembly apply
docker compose -f compose.yaml up -d
docker compose -f compose.yaml ps
docker compose -f compose.yaml logs dev
docker compose -f compose.yaml exec --user root dev /bin/echo compose-runtime
docker compose -f compose.yaml run --rm dev /bin/echo compose-run
dembly check
docker compose -f compose.yaml run --rm dev /run/dembly/bin/dembly __runtime check /run/dembly/runtime/dev.toml
docker compose -f compose.yaml down
dembly unapply
```

`compose-runtime` と `compose-run` を確認し、全 command が exit 0 で完了しました。

debug build は glibc 動的 link のため Alpine では実行できず、配布契約の static musl binary を検証対象にしました。

Host 初期化 script は Lock 欠落時に非ゼロとなって手動 `lock` を案内し、`lock` 後の再実行で `apply` が成功することを一時 copy 上で確認しました。

`devcontainer read-configuration` は `dockerComposeFile=["../compose.yaml"]`、`service=dev`、`containerUser=root`、`remoteUser=root` を返しました。

## 検証結果

```text
bash scripts/test-documentation.sh
documentation assertions passed

cargo fmt --all -- --check
exit 0

cargo clippy --workspace --all-targets --all-features -- -D warnings
exit 0

cargo test --workspace
122 passed; 0 failed

cargo test -p dembly-cli --test integration native_compose_owns_applied_runtime_lifecycle -- --exact --nocapture
1 passed; 0 failed; finished in 19.05s

git diff --check
exit 0

git diff --exit-code -- design/spec.md
exit 0
```

legacy scan は `README.md`、`README-ja.md`、`examples`、`crates`、`tests`、`design/CONTEXT.md` で出力 0 件でした。

## Trade-off

- runnable 例は外部 Card artifact を要求しないため、Runtime check は空の Card list に対して成功します。
- Card mount、hook、check、非 root process の end-to-end 契約は、同じ native Compose command 列を使う acceptance test が実 Card で検証します。
- Host 初期化 script は Lock を自動更新しないため、初回または入力変更後に利用者の明示的な `lock` が必要です。
- この明示操作により、`lock` だけが不変 identity を更新する所有権を維持します。

## Fix round 1/5

### runnable workflow の順序

`examples/compose-base/README.md` だけが `pull` を `apply` 後に記載していました。

tracked 例の command 列を `pull`、`validate`、`lock`、`apply`、native Compose lifecycle、`down`、`unapply` の順へ修正しました。

英語版と日本語版の quickstart は、すでに `pull`、`init`、`validate`、`lock`、`apply` の順であり、その順序を維持しました。

本 report の「実行可能な例の検証」は tracked 例と同じ `pull`、`validate`、`lock`、`apply` の順です。

### 文書検査の RED と GREEN

`scripts/test-documentation.sh` に command 行の順序検査を先に追加しました。

最初の実行では prose 中の `dembly init` を command と誤認したため、完全一致する command 行だけを対象に修正しました。

再実行では、tracked 例の `validate` が `pull` より前にあることを理由として RED を確認しました。

```text
workflow command out of order in examples/compose-base/README.md at line 13: dembly validate
```

tracked 例の command 順序を修正した後、GREEN を確認しました。

```text
documentation assertions passed
```

### Dev Containers 接続 process

従来の AC-14-13 は `devcontainer read-configuration` を接続 process の実行 user の証拠としていましたが、この command が証明するのは設定上の `remoteUser` だけです。

元の process と Compose `run` command の自動証拠は維持し、Dev Containers 接続 process は `devcontainer up` 後の `devcontainer exec` で user 名と UID を確認する手動検証へ分離しました。

この検証には Dev Containers CLI と Runtime 起動に必要な Linux 環境を要求します。

tracked 例の一時 copy に対して手動 gate を実行し、接続 process の実 user を確認しました。

```text
devcontainer up --workspace-folder <temporary-example> --config <temporary-example>/.devcontainer/devcontainer.json
outcome: success
remoteUser: root

devcontainer exec --workspace-folder <temporary-example> --config <temporary-example>/.devcontainer/devcontainer.json sh -c 'id -un; id -u'
remote-user=root uid=0
```

永続 integration test へ `devcontainer up` を追加すると、任意の Dev Containers CLI と外部 manifest 取得を workspace suite の必須依存にするため、AC-14-13 では明示的な手動 gate としました。

### Fix round 1 検証

```text
bash -n scripts/test-documentation.sh
exit 0

bash scripts/test-documentation.sh
documentation assertions passed

cargo fmt --all -- --check
exit 0

cargo clippy --workspace --all-targets --all-features -- -D warnings
exit 0

cargo test --workspace
122 passed; 0 failed

git diff --check
exit 0

git diff --exit-code -- design/spec.md
exit 0
```
