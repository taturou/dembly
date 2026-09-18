# Task 5 実装報告

## Status

完了。

Host 側の解決処理を `HostPlan::resolve` に集約し、Docker への外部呼び出しを `docker compose ... config --format json` と `docker image inspect` に限定しました。

## 実装内容

- `EffectiveService` は image、entrypoint、command、user、environment を保持します。
- entrypoint と command は未指定と空配列を区別します。
- environment は空文字と明示的な削除を区別します。
- `inspect_compose` は受け取った Compose ファイルを同じ順序の `-f` 引数へ変換します。
- `inspect_image` は一回の読み取り専用 inspect で ID とプロセス設定と環境変数と user を取得します。
- 任意の Docker 引数を実行する関数、container 操作、Compose lifecycle 操作、Host 側 user probe を削除しました。
- `HostPlan::resolve` は Core の Deck 解決、Card filesystem checksum 検証、管理対象 Compose 読み取り、Dev Containers 整合検証、Compose と image の inspect、最終環境の合成を一回の pipeline で実行します。
- intended user は適用前の Compose service user、image USER、`root` の順で選択します。
- Dev Containers の `remoteUser` は intended user のユーザー部分と照合します。
- `dembly-cli` に library target を追加し、後続 command が同じ `HostPlan` を利用できる公開境界を作りました。

## TDD 証跡

最初の RED では、次のコマンドが未実装 API を理由に終了コード 101 で失敗しました。

```text
cargo test -p dembly-docker --test compose_config
error[E0432]: unresolved import `dembly_docker::inspect_compose`

cargo test -p dembly-cli --test host_plan
error[E0432]: unresolved import `dembly_cli`
```

最初の全体 GREEN 実行では、fake Docker テストが process 全体の `PATH` を並列変更して競合しました。

環境変数を変更するテストを mutex で直列化し、製品コードを変更せずにテスト隔離を修正しました。

## 検証結果

```text
cargo test -p dembly-docker
22 passed; 0 failed

cargo test -p dembly-cli --test host_plan
4 passed; 0 failed

cargo test -p dembly-cli --no-run
all CLI test targets compiled

cargo clippy -p dembly-docker --all-targets -- -D warnings
Finished successfully

cargo clippy -p dembly-cli --all-targets -- -D warnings
Finished successfully
```

fake Docker の argv ログは、成功時の `HostPlan` が次の二種類だけを順に実行したことを検証します。

```text
compose -f <base> -f <managed> config --format json
image inspect <reference>
```

## 自己レビュー

- Compose ファイル列は Dev Containers の宣言順を保持し、管理対象 Compose が最後であることを inspect 前に検証します。
- 管理対象以外の各 Compose ファイルはトップレベル `x-dembly` を拒否します。
- Card filesystem checksum は Docker inspect より前に検証するため、改ざん時に Docker を呼び出しません。
- image 環境を基礎とし、Compose の設定と削除を反映してから Deck と Card の既存 precedence を適用します。
- fake Docker テストは exact argv を検証するため、lifecycle または container 操作の追加を検出します。
- `validate`、`lock`、`apply`、`inspect`、`check` の command 実装は Task 7 以降の範囲であるため先取りしていません。

## 残存懸念

- 既存の `crates/dembly-cli/tests/integration.rs` は移行前の lifecycle command を対象としているため、実行していません。
- `cargo test -p dembly-cli --no-run` で、この既存 test target を含む全 CLI target のコンパイルは確認しました。
