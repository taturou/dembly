# Task 4 実装報告

## Status

完了。

管理対象 Compose を `serde_yaml::Value` として保持し、`x-dembly` だけを strict typed decode する `ManagedCompose` を追加した。

旧 Compose override 生成 API とテストは削除した。

## 実装内容

- `ManagedValue::Missing` と `ManagedValue::Present(Value::Null)` を区別し、適用状態へ保存する。
- トップレベル `name` を必須化し、Compose project name の形式を検証する。
- `x-dembly` の schema version と未知フィールドを、Lock と適用状態の入れ子まで検証する。
- `set_lock` は既存 state と Compose サービスを保持し、Lock だけを置換する。
- `apply` は選択サービスの `entrypoint`、`command`、`user`、`privileged`、Dembly label、Dembly mount だけを更新する。
- label は key 単位、mount は target 単位で所有権を識別し、利用者所有 entry を保持する。
- 再適用と適用解除は全管理値を検証してから clone 上で変更し、競合時には元の文書を変更しない。
- 再適用時は Dembly mount を desired 順に再構築する。
- `unapply` は missing と null を含む original を復元し、`x-dembly.state` だけを削除する。
- `atomic_replace` は対象と同じディレクトリに一時ファイルを作成し、既存 permission の複製、全 byte の書込み、file sync、rename、parent directory sync の順に実行する。
- `Cargo.toml`、`Cargo.lock`、`dembly-docker/Cargo.toml` の Serde YAML 依存は先行 Task で追加済みだったため、Task 4 では重複変更していない。

## TDD 証跡

RED では `cargo test -p dembly-docker --test managed_compose` が未実装の `ManagedCompose` など7 APIを理由に終了コード101で失敗した。

追加 RED では mount の desired 順変更テストだけが失敗し、再配置処理を追加した後に GREEN へ移行した。

## 検証結果

```text
cargo test --locked -p dembly-docker --test managed_compose
13 passed; 0 failed

cargo test --locked -p dembly-docker
18 passed; 0 failed

cargo clippy --locked -p dembly-docker --all-targets -- -D warnings
Finished successfully

git diff --check
No output
```

`cargo test --locked --workspace` は Task 4 対象 package を通過した後、移行前 CLI 統合テスト15件のうち14件が現行 CLI と不一致で失敗した。

この workspace failure は Task 4 の Compose editor 変更で発生した compile failure ではなく、後続 Task が置換する旧 CLI lifecycle テストに残る既知の移行途中状態である。

## 残存懸念

- `serde_yaml` はコメントと元の表記形式を保持しないため、初回更新時に文書全体を正規化する。
- 同じ入力に対する2回目以降の直列化は byte-stable である。
- `atomic_replace` が rename 後の parent directory sync で失敗した場合、置換自体は完了している可能性がある。
- Task 4 の試験は rename 前の書込み失敗で既存 target が不変であることを確認している。
