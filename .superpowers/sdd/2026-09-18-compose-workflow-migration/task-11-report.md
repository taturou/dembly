# Task 11 実装報告

## 結論

- 公開 CLI の lifecycle を Docker Compose へ完全移管しました。
- 旧 Image Base command builder と専用テスト、fixture を削除しました。
- 旧統合テストを、`dembly lock/apply` 後に native `docker compose` を使用する単一の受け入れシナリオへ置き換えました。
- canonical `design/spec.md` は変更していません。

## 実装内容

- 同じ Compose file list と top-level project name を使用して、`up -d`、`ps -a`、`logs`、`exec --user`、`run --rm`、Runtime `check`、`down` を実行します。
- Card 更新後に `lock/apply` と `compose up -d` を再実行し、対象 container が再作成される一方、基礎 image ID が変わらないことを検証します。
- Dembly 管理外の sidecar service 全体と、対象 service の利用者管理フィールドが apply 前後で保持されることを検証します。
- Card filesystem、checksum、環境、export、root hook、Volume、read-only/read-write Host Bind、非 root process を同じシナリオで検証します。
- Host mount namespace に Card mount target が現れないことを検証します。
- Runtime 初期化失敗時に container が非ゼロで終了し、Compose logs と Compose run の双方から失敗を確認できることを検証します。
- 非 root image の元 entrypoint と Compose run override は UID 10001、Runtime hook は UID 0 で動作することを検証します。
- `devcontainer read-configuration` で service、Compose file order、`containerUser=root`、`remoteUser=dembly` を検証します。
- `tests/fixtures/compose-workflow/.devcontainer/initialize-host.sh` は Host 側で `dembly lock`、`dembly apply` を順に実行します。

## 削除内容

- `crates/dembly-docker/src/image.rs`
- `crates/dembly-docker/tests/image_plan.rs`
- `tests/fixtures/image-base/Dockerfile`
- 旧 `deck.toml`、Image Base、Compose Base、`dembly up/down/run/exec` に依存する統合テスト
- `dembly-docker` の `image_create_command`、`CardFileBind`、`ImageRuntimePlan` 公開 export

## TDD 証跡

新しい受け入れテストを先に実行し、RED を確認しました。

```text
cargo test -p dembly-cli --test integration native_compose_owns_applied_runtime_lifecycle -- --exact --nocapture
compose exec: status=exit status: 4
```

診断では intended user、元 entrypoint、root hook、export、Bind、Volume の存在が確認でき、旧 hello fixture だけが Compose exec process に PID 1 の process-local 環境を期待していました。

環境検査を Runtime root hook へ分離した後、rw Bind source と Volume の Host ownership 前提も fixture で明示し、GREEN を確認しました。

```text
test native_compose_owns_applied_runtime_lifecycle ... ok
test result: ok. 1 passed; 0 failed
```

## Legacy scan

指定 pattern を `crates` と `tests/fixtures` に対して実行し、出力 0 件を確認しました。

```text
rg -n 'deck\.toml|deck\.lock|Base::Image|ImageRuntimePlan|compose_override|dembly (up|down|run|exec)' crates tests/fixtures
```

内部 Runtime の error prefix は、`dembly run` の誤検出を避け、実際の非公開 command 名に一致する `dembly __runtime` へ変更しました。

## 検証環境

- Docker client/server 29.6.1
- Docker Compose v5.3.1
- Dev Containers CLI 0.87.0
- `/usr/bin/mksquashfs` 利用可能
- SquashFS mount と privileged container を含む受け入れシナリオ成功

## 検証結果

```text
cargo test --workspace
122 passed; 0 failed

cargo clippy --workspace --all-targets -- -D warnings
exit 0

cargo fmt --all -- --check
exit 0

git diff --check
exit 0
```

## トレードオフ

- 受け入れテストは Docker Engine、Compose、Dev Containers CLI、musl target、SquashFS、privileged container を要求します。単体テストより実行コストは高い一方、Dembly が container lifecycle を所有しない契約を実物で検証できます。
- rw Bind の mount mode は Host filesystem の所有権を変更しません。fixture は intended user が書ける source permission を明示し、製品コードへ暗黙の `chown` を追加していません。
- Runtime 環境は初期化後の application process と hook に適用されます。別 process である Compose exec へ暗黙継承させず、環境検証は root hook、exec 検証は intended user と Card export に分離しました。

## 根拠

- `design/spec.md` 4.3、4.4、4.5、7.4、7.5、9.5、10.1 から 10.6、14
- `design/superpowers/plans/2026-09-18-compose-workflow-migration.md` Task 11

## Fix round 1/5

### P2 所見

旧 lifecycle テストの削除時に、Compose acceptance へ統合されない次の仕様回帰テストまで削除していました。

- 対話的 `dembly card build`
- 必須 Host Bind 欠落時の拒否
- Card 間で重複する export target の拒否

3契約を `crates/dembly-cli/tests/card_and_validation.rs` の独立した focused tests として復元しました。

### 対話的 Card build

timeout 付きの実 process テストを先に追加し、stdin prompt で停止する RED を確認しました。

```text
card build did not finish: stdout=Card name [interactive-tool]:  stderr=
test result: FAILED. 0 passed; 1 failed
```

原因は `main` が stdin を lock した後、`card_build` の prompt が `io::stdin()`を再取得していたことです。

`dispatch` の `BufRead` と `Write` を `card_build` と prompt へ渡し、Card名、version、mount target を同じ stream から読み取るよう修正しました。

spec 9.9 に従い、成功出力へ Card root に加えて filesystem SHA-256 を出力します。

### 必須 Bind と重複 export

既存実装が残っていた2契約は、一時 mutation でテストの検出力を確認しました。

```text
# bind.required 分岐を反転
validate_rejects_a_missing_required_host_bind ... FAILED
stdout=valid: .../.dembly/config.toml

# validate_exports 呼出しを削除
validate_rejects_duplicate_export_targets_across_cards ... FAILED
stdout=valid: .../.dembly/config.toml
```

mutation は直ちに復元し、Core実装へ差分を残していません。

### GREEN

```text
cargo test -p dembly-cli --test card_and_validation -- --nocapture
3 passed; 0 failed
```

対話テストは生成されたmanifest、SquashFS、default Card名、version、default mount target、標準出力のCard root/checksum、入力tool root不変を検証します。

validation testsはDocker inspectionより前に対象エラーが返る実CLI境界を検証します。
