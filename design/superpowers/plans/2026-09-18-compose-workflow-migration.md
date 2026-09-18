# Docker Compose Workflow Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Host DemblyをCompose設定コンパイラへ限定し、`.dembly/config.toml`と管理対象ComposeファイルからRuntime成果物を生成して、コンテナ操作を標準の`docker compose`へ移す。

**Architecture:** `dembly-core`はDeck/Cardの入力、解決、Lock材料を扱い、`dembly-docker`はCompose/Dev Containers文書の検証と管理領域の編集を扱う。`dembly-runtime`はHostから渡された計画だけを解釈し、コンテナ内で利用者解決、mount、初期化、最終process実行を行う。`dembly-cli`はこれらをHostコマンドと非公開Runtimeコマンドへ配線し、Docker Engineへの照会はイメージ情報の読取りに限定する。

**Tech Stack:** Rust 2021（MSRV 1.75）、Cargo workspace、Serde、TOML、JSON、YAML、Docker Engine、Docker Compose、Linux mount namespace、SquashFS。

**Spec:** `design/spec.md`（正本）、`design/superpowers/specs/2026-09-14-compose-workflow-migration.md`（従来実装からの移行差分）

## Global Constraints

- `design/spec.md`を実装判断の正本とし、移行仕様と矛盾する場合は正本を優先する。
- この計画の実行中に仕様を補完しない。未定義のwire formatやエラー文が実装を阻害する場合は、先に`design/spec.md`を別目的の変更として改定する。
- Host公開コマンドから、コンテナのcreate/start/stop/remove、`docker exec`、`docker compose up/down/run/exec`を呼ばない。
- Dockerへの外部process呼出しは、`docker compose config`、`docker image inspect`など、仕様で必要な読取りだけに閉じ込める。
- 管理対象Composeファイルは`.dembly/config.toml`の`compose.path`で指定した1ファイルだけとする。
- Composeの対象外serviceと管理対象外fieldの値を保持する。YAML再直列化による空白・コメントの正規化は許容するが、semantic valueを失わせない。
- Compose fieldの未定義と明示的`null`を`ManagedValue::Missing`と`ManagedValue::Present(Value::Null)`として区別する。
- `lock`と`apply`の出力順序を決定的にし、同一入力の2回目はGit追跡対象へ差分を出さない。
- 各ファイルは同一directoryの一時ファイルへ書いて`sync_all`後にrenameする。`apply`ではRuntime成果物と`.gitignore`を先に、Composeファイルを最後に置き換える。
- 既存のCard build、checksum、Card競合検証は維持する。Image Baseと旧lifecycle実装だけを削除する。
- 各Taskは記載したfocused testを赤から緑にしてからcommitする。無関係な既存変更をcommitへ含めない。
- Conventional Commitsの英語titleと日本語bodyを使用する。

## Specification Coverage

| 正本の範囲 | 実装Task |
|---|---|
| 目的、スコープ、Deck/Card、基本アーキテクチャ、状態遷移 | Task 1、4、5、11 |
| `.dembly/config.toml`、`card.toml`、Host Bind変数 | Task 1、6 |
| `devcontainer.json`と固定ルート探索 | Task 2、3、5 |
| 管理対象Compose、`x-dembly.lock`、`x-dembly.state` | Task 4、8、9 |
| `.dembly/runtime/<service>.toml`、Runtime binary、Volume、`.gitignore` | Task 6、9 |
| `dembly init` | Task 3 |
| `dembly validate`、`dembly inspect`、Host `dembly check` | Task 7 |
| `dembly lock` | Task 8 |
| `dembly apply`、`dembly unapply` | Task 9 |
| `dembly card build` | Task 1で既存動作を保護し、Task 11でCompose受入へ統合する |
| Runtime `init`/`check`、指定利用者、Card/Volume/Bind、環境、hook | Task 6、10 |
| `docker compose up/ps/logs/exec/run/down`との併用 | Task 11 |
| 競合、原子的更新、冪等性、復元 | Task 4、8、9 |
| セキュリティ、schema、受入条件、公開文書 | Task 11、12、Final Verification |

---

### Task 1: Replace the Deck file model with `.dembly/config.toml`

**Files:**
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `crates/dembly-core/Cargo.toml`
- Modify: `crates/dembly-core/src/model.rs`
- Modify: `crates/dembly-core/src/parse.rs`
- Modify: `crates/dembly-core/src/discovery.rs`
- Modify: `crates/dembly-core/src/resolution.rs`
- Modify: `crates/dembly-core/src/error.rs`
- Modify: `crates/dembly-core/src/lib.rs`
- Modify: `crates/dembly-core/tests/parsing.rs`
- Modify: `crates/dembly-core/tests/planning.rs`
- Delete: `crates/dembly-core/src/lock.rs`
- Delete: `crates/dembly-core/tests/lock.rs`

**Interfaces:**
- `discover_config(explicit: Option<&Path>, cwd: &Path) -> Result<PathBuf, CoreError>`
- `load_config(path: &Path) -> Result<ConfigDocument, CoreError>`
- `resolve_deck(config_path: &Path, host: &HostVariables) -> Result<ResolvedDeck, CoreError>`
- `ConfigDocument { schema_version, compose, devcontainer, cards, environment, environment_path_prepend, volumes, binds }`
- `ComposeReference { path, service }`
- `HostVariables { host_home, deck_root }`
- `ResolvedBind` keeps the Host-resolved `source` and unresolved Runtime `target` string.

- [ ] **Step 1: Add failing config discovery and parsing tests**

Replace the old `deck.toml` discovery tests with cases proving:

```rust
assert_eq!(
    discover_config(None, &cwd).unwrap(),
    cwd.join(".dembly/config.toml")
);
assert_eq!(
    discover_config(Some(Path::new("custom/config.toml")), &cwd).unwrap(),
    cwd.join("custom/config.toml")
);
```

Add cases for required `[compose]`, optional `[devcontainer]`, absolute and Deck-root-relative Card paths, unknown fields, `schema_version != 1`, no parent-directory search, and a missing explicit config.

- [ ] **Step 2: Prove the new model is absent**

Run: `cargo test -p dembly-core --test parsing`

Expected: compile failure because `discover_config`, `load_config`, and `ConfigDocument` do not exist.

- [ ] **Step 3: Adopt structured TOML parsing and the Compose-only model**

Add workspace dependencies `serde = { version = "1", features = ["derive"] }` and `toml = "0.8"`; add them to `dembly-core`.
Replace `DeckDocument.base` and `Base` with mandatory `ConfigDocument.compose`; keep `CardDocument` semantics unchanged but deserialize it through the same strict Serde path using `#[serde(deny_unknown_fields)]`.
Resolve relative paths from the config parent (`.dembly/` by default), not the project root.
Rename `CoreError::DeckNotFound` to `ConfigNotFound` and ensure errors name the actual input path.

- [ ] **Step 4: Split Host and Runtime bind expansion**

Change Host resolution so `source` expands only `${HOST_HOME}` and `${DECK_ROOT}` and `target` is validated but retains `${HOME}`/`${USER}` for Runtime.
Reject `$HOME`, unsupported variables, non-absolute expanded Runtime targets, parent traversal, and symlinked Volume storage.
Keep Volume layout under `<deck-root>/volumes`, which resolves to `.dembly/volumes` for the default config.

- [ ] **Step 5: Remove standalone lock TOML support and verify core**

Delete `DeckLock`, `LockBase`, `read_lock`, and `write_lock`; embedded Compose Lock is introduced in Task 4.

Run:

```bash
cargo test -p dembly-core
cargo fmt --all -- --check
```

Expected: all core tests pass; no core source or test refers to `deck.toml`, `deck.lock`, `Base::Image`, or `Base::Compose`.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock crates/dembly-core
git commit -m "refactor(core): adopt dembly config model"
```

Use a Japanese body stating why the Deck input moved under `.dembly/` and that Image Base/standalone Lock models were removed.

### Task 2: Implement fixed-root init discovery and Dev Containers parsing

**Files:**
- Modify: `crates/dembly-core/src/discovery.rs`
- Modify: `crates/dembly-core/src/lib.rs`
- Create: `crates/dembly-core/tests/init_discovery.rs`
- Modify: `crates/dembly-docker/Cargo.toml`
- Create: `crates/dembly-docker/src/devcontainer.rs`
- Modify: `crates/dembly-docker/src/lib.rs`
- Create: `crates/dembly-docker/tests/devcontainer.rs`

**Interfaces:**
- `discover_init_candidates(cwd: &Path) -> Result<InitCandidates, CoreError>`
- `InitCandidates { cards: Vec<CardCandidate>, devcontainers: Vec<PathBuf> }`
- `CardCandidate { path: PathBuf, name: String }`
- `load_devcontainer(path: &Path) -> Result<DevContainerDocument, String>`
- `DevContainerDocument { docker_compose_file: Vec<String>, service: String, initialize_command, override_command, container_user, remote_user, update_remote_user_uid }`
- `validate_devcontainer(document, path, managed_compose, service, intended_user) -> Result<Vec<PathBuf>, String>` returns the normalized Compose file sequence.

- [ ] **Step 1: Add discovery tests for every allowed and forbidden location**

Create regular-file fixtures for all four Card roots and all three Dev Containers patterns.
Assert normalized cwd-relative lexical order.
Add negative cases for files two child levels deep, a symlink directory, a symlink candidate file, and similarly named roots outside the fixed list.
Assert duplicate Card names remain separate candidates at discovery time.

- [ ] **Step 2: Prove fixed-root discovery fails**

Run: `cargo test -p dembly-core --test init_discovery`

Expected: compile failure because `discover_init_candidates` does not exist.

- [ ] **Step 3: Implement non-recursive, non-symlink discovery**

Use `symlink_metadata`, never `canonicalize` for traversal.
Read only each specified root and its direct child directories; accept only regular `card.toml` files.
Read Card names with `load_card`, normalize displayed paths lexically relative to cwd, then sort.

- [ ] **Step 4: Add failing Dev Containers schema and consistency tests**

Cover string and array `dockerComposeFile`, missing `remoteUser`, mismatched service, managed file not last, `overrideCommand: true`, non-root `containerUser`, relative paths based on the JSON parent, and `x-dembly` in a non-managed Compose file.

- [ ] **Step 5: Implement strict JSON parsing**

Add `serde_json = "1"` to workspace and `dembly-docker`.
Deserialize only fields Dembly validates while retaining a strict wrapper for their types; never write `devcontainer.json`.
Expose normalized Compose files so Host commands can pass the same ordered list to Compose inspection.

- [ ] **Step 6: Verify and commit**

Run:

```bash
cargo test -p dembly-core --test init_discovery
cargo test -p dembly-docker --test devcontainer
```

Commit with `feat(init): discover cards and devcontainers` and a Japanese body explaining the fixed roots and symlink exclusion.

### Task 3: Add a testable Host CLI shell and interactive `init`

**Files:**
- Create: `crates/dembly-cli/src/args.rs`
- Create: `crates/dembly-cli/src/commands/mod.rs`
- Create: `crates/dembly-cli/src/commands/init.rs`
- Modify: `crates/dembly-cli/src/main.rs`
- Modify: `crates/dembly-cli/tests/help.rs`
- Create: `crates/dembly-cli/tests/init.rs`

**Interfaces:**
- `HostCommand::{Init, Validate, Lock, Apply, Unapply, Inspect, Check, CardBuild}`
- All config-reading commands accept only `--config <path>`; omission resolves to `<cwd>/.dembly/config.toml`.
- `commands::init::run(context: &HostContext, input: &mut dyn BufRead, output: &mut dyn Write) -> Result<(), CliError>`
- `init` produces a strict `ConfigDocument` and refuses to overwrite the destination.

- [ ] **Step 1: Replace help expectations and add parser failures**

Assert help lists `init`, `validate`, `lock`, `apply`, `unapply`, `inspect`, `check`, and `card`, but not `up`, `down`, `run`, `exec`, or `__runtime`.
Assert positional `deck.toml`, repeated `--config`, unknown options, and missing option values fail with exit 2.

- [ ] **Step 2: Add scripted stdin tests for `init`**

Use a temporary project and piped answers to cover:

- one candidate still requires confirmation;
- multiple Cards are selected and emitted in selected order;
- duplicate selected Card names fail before writing;
- a selected Dev Containers file exposes its service and Compose sequence for confirmation;
- no Dev Containers selection asks for Compose path and then service;
- existing output is unchanged;
- `--config custom/config.toml` creates its parent and writes Card paths relative to that parent.

- [ ] **Step 3: Prove the command surface fails**

Run:

```bash
cargo test -p dembly-cli --test help
cargo test -p dembly-cli --test init
```

Expected: failures because new commands/parser do not exist.

- [ ] **Step 4: Extract command parsing before deleting old implementations**

Make `main` dispatch through `args.rs`; retain `__runtime` as a hidden branch.
Return typed errors to one top-level stderr/exit-code adapter rather than printing throughout new modules.
Remove old public command dispatch immediately, but defer deletion of its helper functions until replacement commands land.

- [ ] **Step 5: Implement deterministic interactive generation**

Render `[compose]`, optional `[devcontainer]`, and selected `[[cards]]` in a stable order.
Do not modify Compose, Dev Containers, Lock, or any existing config.
Require explicit confirmation for every adoption, including a one-item candidate list.

- [ ] **Step 6: Verify and commit**

Run both focused CLI tests and commit with `feat(cli): add interactive project initialization` plus a Japanese body describing human confirmation and non-overwrite behavior.

### Task 4: Model and edit the managed Compose document

**Files:**
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `crates/dembly-docker/Cargo.toml`
- Create: `crates/dembly-docker/src/managed_compose.rs`
- Create: `crates/dembly-docker/src/atomic_write.rs`
- Modify: `crates/dembly-docker/src/lib.rs`
- Create: `crates/dembly-docker/tests/managed_compose.rs`
- Delete: `crates/dembly-docker/src/compose.rs`
- Delete: `crates/dembly-docker/tests/compose_override.rs`

**Interfaces:**
- `ManagedCompose::read(path: &Path) -> Result<Self, String>`
- `ManagedCompose::project_name() -> Result<&str, String>`
- `ManagedCompose::service(name: &str) -> Result<ServiceSnapshot, String>`
- `ManagedCompose::lock() -> Result<Option<DemblyLock>, String>`
- `ManagedCompose::state() -> Result<Option<ApplyState>, String>`
- `ManagedCompose::set_lock(lock: DemblyLock) -> Result<(), String>`
- `ManagedCompose::apply(service: &str, desired: ManagedFields, lock_digest: &str) -> Result<(), Conflict>`
- `ManagedCompose::unapply(service: &str) -> Result<(), Conflict>`
- `atomic_replace(path: &Path, bytes: &[u8]) -> Result<(), String>`
- `ManagedValue::{Missing, Present(serde_yaml::Value)}` preserves missing versus explicit null.

- [ ] **Step 1: Add YAML round-trip and state tests**

Use fixtures containing a top-level `name`, selected and unselected services, custom extension fields, maps, sequences, explicit `null`, and an absent field.
Assert:

- missing/`null` remain distinguishable in `original` and `applied`;
- only `x-dembly.lock` changes during `set_lock`;
- first apply stores original/applied pairs;
- repeated apply with identical desired values is byte-stable after first normalization;
- changed managed values return a conflict naming service, field, expected, and current;
- unapply restores missing versus null and removes only `x-dembly.state`;
- top-level `name` missing or invalid, unknown `x-dembly` fields, and schema other than 1 fail.

- [ ] **Step 2: Prove the managed editor is absent**

Run: `cargo test -p dembly-docker --test managed_compose`

Expected: compile failure because `ManagedCompose` does not exist.

- [ ] **Step 3: Implement the YAML domain types**

Add `serde_yaml = "0.9"` and `serde` to `dembly-docker`.
Represent the whole document as `serde_yaml::Value`, then decode only `x-dembly` through strict typed structs.
Use `BTreeMap` for Dembly-owned mappings and sort Card locks by config order to make serialization deterministic.
Do not hash the Compose document.

- [ ] **Step 4: Implement field-level apply/unapply**

Manage only selected-service `entrypoint`, `command`, `user`, `privileged`, Dembly label, and Dembly-added mount entries.
Preserve user-owned labels and mounts by identifying Dembly-owned entries explicitly; state must record the exact original and applied values needed to reverse the edit.
On reapply/unapply, compare every current managed value before mutating any in-memory field.

- [ ] **Step 5: Implement durable single-file replacement**

Create the temporary file in the target directory, copy target permissions when replacing an existing file, write all bytes, `sync_all`, rename, then sync the parent directory.
Add a test proving a write/validation failure leaves the original file unchanged.

- [ ] **Step 6: Remove override generation, verify, and commit**

Run `cargo test -p dembly-docker --test managed_compose` and `cargo test -p dembly-docker`.
Commit with `feat(compose): manage persistent dembly state` and a Japanese body describing field-level conflict detection and embedded Lock/state.

### Task 5: Build the resolved Host plan and read-only Docker inspection boundary

**Files:**
- Modify: `crates/dembly-docker/src/client.rs`
- Modify: `crates/dembly-docker/src/compose_config.rs`
- Modify: `crates/dembly-docker/src/lib.rs`
- Modify: `crates/dembly-docker/tests/compose_config.rs`
- Create: `crates/dembly-cli/src/host_plan.rs`
- Create: `crates/dembly-cli/tests/host_plan.rs`

**Interfaces:**
- `inspect_compose(files: &[PathBuf], service: &str) -> Result<EffectiveService, String>` executes only `docker compose ... config --format json`.
- `inspect_image(reference: &str) -> Result<ImageConfig, String>` executes only `docker image inspect`.
- `EffectiveService { image, entrypoint, command, user, environment }`
- `HostPlan::resolve(config_path: &Path) -> Result<HostPlan, CliError>` combines Core resolution, managed Compose, optional Dev Containers validation, and image metadata.
- `HostPlan::intended_user_spec()` chooses pre-apply service `user`, then image `USER`, then `root`.

- [ ] **Step 1: Add parser and fake-Docker boundary tests**

Extend Compose JSON parser tests for effective image, environment, absent/empty entrypoint and command, and user.
Place a fake `docker` executable first in `PATH`; record argv and return fixture JSON.
Assert Host plan issues only `compose config` and `image inspect` calls and never lifecycle/container commands.

- [ ] **Step 2: Add Dev Containers cross-file validation tests**

Assert the selected service matches, managed Compose is the final file, `remoteUser` matches the user portion of `intended_user_spec`, and every non-managed file rejects top-level `x-dembly`.

- [ ] **Step 3: Prove Host plan resolution fails**

Run: `cargo test -p dembly-cli --test host_plan`

Expected: compile or assertion failure because `HostPlan` and the read-only boundary are absent.

- [ ] **Step 4: Narrow `dembly-docker::client`**

Pass the exact ordered `-f` sequence to `docker compose config`.
Delete `run_docker`, `docker_status`, `docker_status_quiet`, `container_label`, `exec_in_container`, `probe_image_user`, `compose_status`, `compose_service_container`, and `compose_service_exists` exports and implementations.
Keep no generic function capable of invoking arbitrary Docker commands.

- [ ] **Step 5: Implement one shared Host resolution pipeline**

Make `validate`, `lock`, `apply`, `inspect`, and `check` consume `HostPlan` rather than independently reparsing files.
Compute base environment from effective Compose/image data before applying Deck/Card precedence.
Verify Card manifest and filesystem checksums during plan resolution.

- [ ] **Step 6: Verify and commit**

Run:

```bash
cargo test -p dembly-docker
cargo test -p dembly-cli --test host_plan
rg -n 'docker (create|start|stop|rm|exec)|compose (up|down|run|exec)' crates/dembly-{cli,docker}/src
```

Expected: tests pass; the search has no Host invocation sites.
Commit with `refactor(host): isolate read-only docker inspection` and a Japanese body describing the lifecycle boundary.

### Task 6: Redesign the Runtime plan and initialization engine

**Files:**
- Modify: `crates/dembly-runtime/src/config.rs`
- Create: `crates/dembly-runtime/src/user.rs`
- Modify: `crates/dembly-runtime/src/mount.rs`
- Modify: `crates/dembly-runtime/src/setup.rs`
- Modify: `crates/dembly-runtime/src/lib.rs`
- Modify: `crates/dembly-runtime/tests/config.rs`
- Create: `crates/dembly-runtime/tests/user.rs`
- Modify: `crates/dembly-runtime/tests/mount.rs`
- Create: `crates/dembly-runtime/tests/setup.rs`

**Interfaces:**
- `RuntimeConfig { schema_version, lock_digest, runtime_user: RuntimeUserSpec, cards, binds, exports, hooks, checks, environment, process_argv }`
- `RuntimeUserSpec { spec: String }`
- `ResolvedRuntimeUser { name, uid, gid, home }`
- `resolve_runtime_user(spec: &str, passwd: &str, group: &str) -> Result<ResolvedRuntimeUser, String>`
- `RuntimeBind { source, target, mode }`
- `mount_bind(bind: &RuntimeBind, user: &ResolvedRuntimeUser) -> Result<(), String>`
- `run_checks(config: &RuntimeConfig, user: &ResolvedRuntimeUser) -> Result<(), String>`
- `render_runtime_config(config: &RuntimeConfig) -> Result<String, String>`

- [ ] **Step 1: Add strict Runtime config round-trip tests**

Replace `name/uid/gid/home` fixtures with `[runtime_user] spec = "vscode"`.
Cover binds with `${HOME}`/`${USER}`, check declarations, unknown fields, missing/invalid schema, deterministic rendering, and non-empty Lock digest.

- [ ] **Step 2: Add pure user resolution tests**

Test `user`, `uid`, `user:group`, and `uid:gid`; primary GID fallback; named-group lookup; numeric user without `/etc/passwd`; missing named group; extra colon; empty user; and UID/GID values outside `u32`.
Assert failures occur before any mount helper is called by the Runtime command test added in Task 7.

- [ ] **Step 3: Prove the Runtime schema fails**

Run: `cargo test -p dembly-runtime`

Expected: failures because the old config expects resolved user fields and has no binds/checks.

- [ ] **Step 4: Implement config rendering and user resolution**

Use strict Serde TOML for both load and render.
Keep `/etc/passwd` and `/etc/group` I/O in a thin `resolve_current_runtime_user`; test parsing through supplied strings.
Do not accept a numeric UID without its passwd entry.

- [ ] **Step 5: Implement staged Host Bind mount behavior**

Validate the staged source exists, expand only `${HOME}` and `${USER}`, create a directory target for a directory source or an empty file target for a file source, execute a bind mount, and remount read-only for `mode = "ro"`.
Return errors containing the bind index/source/target and failing operation.
Keep Card mounts read-only; validate that every Volume target supplied by Compose is present before continuing.

- [ ] **Step 6: Implement Card checks and verify**

Run checks in Card config order, with each Card mount as cwd and the planned environment, after privilege drop in the Runtime command orchestration.
Use test command fixtures that succeed/fail without requiring mount privileges.

Run `cargo test -p dembly-runtime` and commit with `feat(runtime): resolve users and staged binds` plus a Japanese body describing container-local user resolution and failure ordering.

### Task 7: Implement `validate`, `inspect`, and Host `check`

**Files:**
- Create: `crates/dembly-cli/src/commands/validate.rs`
- Create: `crates/dembly-cli/src/commands/inspect.rs`
- Create: `crates/dembly-cli/src/commands/check.rs`
- Modify: `crates/dembly-cli/src/commands/mod.rs`
- Modify: `crates/dembly-cli/src/main.rs`
- Modify: `crates/dembly-cli/tests/help.rs`
- Create: `crates/dembly-cli/tests/host_commands.rs`

**Interfaces:**
- `validate` resolves `HostPlan`, prints warnings/results, and writes nothing.
- `inspect` prints config path, Compose project/path/service, intended user spec, Cards, mounts, environment, Lock, and apply/conflict state without writing.
- Host `check` requires a valid Lock and applied state, verifies generated artifact existence/digests and managed field equality, and never executes Card checks.

- [ ] **Step 1: Add command contract tests with filesystem snapshots**

For each command, snapshot all project files before/after and assert no writes.
Cover `--config`, missing config, optional Bind warning, stale/missing Lock, unapplied state, managed-field conflict, and successful inspect output.
Use fake Docker inspection as in Task 5.

- [ ] **Step 2: Prove the contracts fail**

Run: `cargo test -p dembly-cli --test host_commands`

Expected: failures because the new command modules are not implemented.

- [ ] **Step 3: Implement commands over `HostPlan`**

Keep validation shared; do not duplicate parsing or checksum logic.
Define one stable inspect rendering function and one static applied-state checker.
Ensure Host `check` error directs Card execution checks to `docker compose run ... __runtime check`.

- [ ] **Step 4: Verify and commit**

Run focused CLI tests and the fake-Docker command log assertion.
Commit with `feat(cli): add static host validation commands` plus a Japanese body distinguishing Host integrity checks from Runtime Card checks.

### Task 8: Implement embedded Lock and `dembly lock`

**Files:**
- Create: `crates/dembly-core/src/identity.rs`
- Modify: `crates/dembly-core/src/lib.rs`
- Create: `crates/dembly-core/tests/identity.rs`
- Modify: `crates/dembly-docker/src/managed_compose.rs`
- Modify: `crates/dembly-docker/tests/managed_compose.rs`
- Create: `crates/dembly-cli/src/commands/lock.rs`
- Modify: `crates/dembly-cli/src/commands/mod.rs`
- Create: `crates/dembly-cli/tests/lock.rs`

**Interfaces:**
- `CardIdentity { name, version, manifest_sha256, filesystem_sha256 }`
- `LockInput { compose_path, service, image, cards }`
- `LockInput::digest() -> String` hashes a canonical representation, never the Compose document.
- `lock` updates only `x-dembly.lock`; it preserves `x-dembly.state` byte-semantically and all service fields.

- [ ] **Step 1: Add canonical identity/digest tests**

Assert config order is preserved, map ordering cannot change the digest, Card manifest changes do change it, and the managed Compose file contents outside Lock are not part of the digest.

- [ ] **Step 2: Add CLI Lock mutation tests**

Use a Compose file with an existing state and unrelated extension fields.
Assert only Lock changes, the image is stored as an immutable ID, paths are normalized, serial output is deterministic, and running twice makes no second diff.
Assert a conflicting applied state prevents updating Lock when the command cannot validate the current state safely.

- [ ] **Step 3: Prove Lock tests fail**

Run:

```bash
cargo test -p dembly-core --test identity
cargo test -p dembly-cli --test lock
```

- [ ] **Step 4: Implement canonical hashing and atomic Lock update**

Reuse the existing SHA-256 implementation for bytes/files.
Compute `manifest_sha256` from `card.toml` bytes and use the declared, verified SquashFS checksum for `filesystem_sha256`.
Write the Compose file through `atomic_replace` only after all validation succeeds.

- [ ] **Step 5: Verify and commit**

Run focused tests twice against the same fixture and assert the second Git diff is empty.
Commit with `feat(lock): embed resolved identities in compose` plus a Japanese body explaining why whole-Compose hashing and `deck.lock` are absent.

### Task 9: Implement `apply` and `unapply` artifact transactions

**Files:**
- Create: `crates/dembly-cli/src/artifacts.rs`
- Create: `crates/dembly-cli/src/commands/apply.rs`
- Create: `crates/dembly-cli/src/commands/unapply.rs`
- Modify: `crates/dembly-cli/src/commands/mod.rs`
- Modify: `crates/dembly-cli/src/main.rs`
- Create: `crates/dembly-cli/tests/apply.rs`
- Create: `crates/dembly-cli/tests/unapply.rs`

**Interfaces:**
- `build_runtime_config(plan: &HostPlan, lock_digest: &str) -> RuntimeConfig`
- `build_managed_fields(plan: &HostPlan, runtime_path: &Path, lock_digest: &str) -> ManagedFields`
- `apply` writes runtime plan, copied binary, Volume directories, `.gitignore`, then managed Compose.
- `unapply` restores Compose, removes `.dembly/runtime/`, and retains Lock, Volumes, config, Cards, `.gitignore`, Dev Containers files, and Host scripts.

- [ ] **Step 1: Add first-apply and artifact tests**

Assert `apply`:

- rejects missing/stale Lock before writing;
- copies the current executable to `.dembly/runtime/bin/dembly` with executable permissions;
- writes `.dembly/runtime/<service>.toml` with `runtime_user.spec`, original process, Card/check/hook/export/environment data, and staged binds;
- creates `.dembly/volumes/<name>` and Card private/shared layouts without following symlinks;
- adds exactly `/.dembly/runtime/` and `/.dembly/volumes/` once to `.gitignore`;
- adds root entrypoint, empty command, root user, privileged mode, Lock-digest label, and only Dembly mount entries to the selected service;
- leaves other services and user fields unchanged.

- [ ] **Step 2: Add reapply, conflict, ordering, and failure tests**

Assert identical reapply is diff-free; a Card change requires `lock`; external edits to each managed field fail without writes; artifact generation failure leaves old Compose active; and the Compose rename occurs after artifact/.gitignore replacements.

- [ ] **Step 3: Add unapply tests**

Assert exact restoration for absent, null, scalar, and array original fields.
Assert a managed-field conflict prevents all restoration/removal.
Assert successful unapply removes state and Runtime directory but preserves Lock, Volumes, and ignore rules; repeated unapply without state fails.

- [ ] **Step 4: Prove apply/unapply tests fail**

Run:

```bash
cargo test -p dembly-cli --test apply
cargo test -p dembly-cli --test unapply
```

- [ ] **Step 5: Implement artifact staging and final Compose commit**

Build every in-memory output before writing.
Atomically replace Runtime TOML, copied binary, and `.gitignore`; create Volume directories with symlink checks; call managed Compose `apply`; replace Compose last.
Use a Dembly-owned label key dedicated to the Lock digest and a deterministic mount order: Runtime binary, Runtime plan, Cards in config order, Volumes in resolved order, then staged Binds by index.

- [ ] **Step 6: Implement safe unapply cleanup**

Validate all managed fields first, write restored Compose atomically, then remove only the exact `<deck-root>/runtime` directory after validating the resolved target is beneath the config parent and is not a symlink.

- [ ] **Step 7: Verify and commit**

Run both focused tests, `git diff --check`, and `cargo test -p dembly-cli`.
Commit with `feat(cli): apply runtime plan to compose` plus a Japanese body describing write ordering, conflict handling, and retained data.

### Task 10: Wire Runtime `init`/`check` and native Compose process override

**Files:**
- Create: `crates/dembly-cli/src/runtime.rs`
- Modify: `crates/dembly-cli/src/main.rs`
- Modify: `crates/dembly-cli/tests/help.rs`
- Create: `crates/dembly-cli/tests/runtime.rs`
- Delete: legacy Runtime probe functions from `crates/dembly-cli/src/main.rs`

**Interfaces:**
- `dembly __runtime init <plan> [compose-run-argv...]`
- `dembly __runtime check <plan>`
- `init` chooses appended Compose-run argv when present; otherwise `RuntimeConfig.process_argv`.
- `check` assumes the outer `init` has completed setup, then loads the plan and executes Card checks without mounting again.
- User resolution occurs before Card/Bind mount; mount/setup/hook occurs as root; final process/check occurs after `setgid`/`setuid`.

- [ ] **Step 1: Add Runtime orchestration tests behind injected system operations**

Define a small `RuntimeSystem` trait for passwd/group reads, effective UID, mounts, privilege drop, hook/check execution, and final exec.
With a recording fake, assert exact initialization order and that:

- non-root initializer fails before reading/mounting;
- invalid Lock/schema/user fails before Card mount;
- failed Card mount, Bind mount, export, or hook prevents privilege drop/final exec;
- default startup execs saved original argv;
- appended argv replaces, rather than appends to, original argv;
- the native Compose check argv is selected as the outer `init` final process, and the resulting `__runtime check` runs Card checks in order without repeating mounts;
- every error names the operation and relevant Card/path and exits non-zero.

- [ ] **Step 2: Prove Runtime orchestration tests fail**

Run: `cargo test -p dembly-cli --test runtime`

Expected: failure because Runtime behavior is still embedded in `main.rs` and has no injected boundary.

- [ ] **Step 3: Implement Runtime orchestration**

Move internal command handling into `runtime.rs`.
Resolve user from the container's current passwd/group before any mount.
After setup, set environment, call `setgid` then `setuid`, and use `CommandExt::exec` only for the selected final process.
For Runtime checks, rely on the outer `init` for setup and privilege drop, then spawn each check with the planned environment and return the first non-zero status.

- [ ] **Step 4: Remove Host-side user probing and verify hidden command behavior**

Delete `__runtime probe` and all Docker-based user probe support.
Assert public help does not expose `__runtime` and malformed internal invocations return exit 2.

- [ ] **Step 5: Verify and commit**

Run:

```bash
cargo test -p dembly-runtime
cargo test -p dembly-cli --test runtime
cargo test -p dembly-cli --test help
```

Commit with `feat(runtime): execute compose startup plans` plus a Japanese body describing root setup, intended-user execution, and run override semantics.

### Task 11: Delete legacy lifecycle/Image Base code and migrate the Compose acceptance test

**Files:**
- Modify: `crates/dembly-cli/src/main.rs`
- Modify: `crates/dembly-cli/tests/integration.rs`
- Modify: `crates/dembly-docker/src/lib.rs`
- Delete: `crates/dembly-docker/src/image.rs`
- Delete: `crates/dembly-docker/tests/image_plan.rs`
- Delete or repurpose: `tests/fixtures/image-base/Dockerfile`
- Modify: `tests/fixtures/nonroot-base/Dockerfile`
- Create: `tests/fixtures/compose-workflow/compose.yaml`
- Create: `tests/fixtures/compose-workflow/.devcontainer/devcontainer.json`
- Create: `tests/fixtures/compose-workflow/.devcontainer/initialize-host.sh`

**Interfaces:**
- Public binary contains no `up`, `down`, `run`, or `exec` branch.
- Acceptance lifecycle is `dembly lock/apply`, then native `docker compose` commands using the same file list/project name.

- [ ] **Step 1: Replace old lifecycle tests with one native Compose scenario**

Build the test Card as today, generate `.dembly/config.toml`, run `lock` and `apply`, then execute:

```bash
docker compose -f tests/fixtures/compose-workflow/compose.yaml up -d
docker compose -f tests/fixtures/compose-workflow/compose.yaml ps -a
docker compose -f tests/fixtures/compose-workflow/compose.yaml logs dev
docker compose -f tests/fixtures/compose-workflow/compose.yaml exec --user <intended-user> dev /usr/local/bin/hello
docker compose -f tests/fixtures/compose-workflow/compose.yaml run --rm dev /usr/local/bin/hello
docker compose -f tests/fixtures/compose-workflow/compose.yaml run --rm dev /run/dembly/bin/dembly __runtime check /run/dembly/runtime/dev.toml
docker compose -f tests/fixtures/compose-workflow/compose.yaml down
```

Assert `ps` sees the selected service in the user-defined top-level project, sidecar fields remain unchanged, Card updates do not rebuild the image, init failure appears in logs/non-zero status, and Host mount namespace contains no Card mount.

- [ ] **Step 2: Add non-root Dockerfile USER and Dev Containers validation coverage**

Use a service whose image declares a non-root user and whose original entrypoint records its UID.
Assert Runtime setup/hook records UID 0 while the original process and `run` override record the intended UID.
Run `devcontainer read-configuration` or the repository's available Dev Containers CLI validation without starting VS Code; assert service, file order, `containerUser`, and `remoteUser` match the Host plan.

- [ ] **Step 3: Prove the migrated acceptance test fails**

Run the focused integration test with `--nocapture`.
Expected: failure until persistent Compose application and Runtime override behavior are complete.

- [ ] **Step 4: Remove obsolete implementations and tests**

Delete all old lifecycle functions, temporary Runtime state, generated override support, Image Base command builders, implicit project-name generation, and legacy ownership-label/container lookup code.
Remove old tests whose only assertion is behavior no longer in the specification; preserve Card filesystem, checksum, environment, export, hook, Volume, Bind, and non-root behavior by moving their assertions into the Compose acceptance scenario.

- [ ] **Step 5: Verify no legacy code path remains and commit**

Run:

```bash
rg -n 'deck\.toml|deck\.lock|Base::Image|ImageRuntimePlan|compose_override|dembly (up|down|run|exec)' crates tests/fixtures
cargo test --workspace
```

Expected: the search has no product/test-fixture references; the workspace suite passes on a Docker/Compose/SquashFS-capable Linux host.
Commit with `refactor: remove dembly lifecycle orchestration` plus a Japanese body explaining that Docker Compose now owns the lifecycle.

### Task 12: Migrate examples, README, and traceability; run final gates

**Files:**
- Modify: `README.md`
- Modify: `README.ja.md`
- Modify: `examples/compose-base/README.md`
- Modify: `examples/compose-base/compose.yaml`
- Delete: `examples/compose-base/deck.toml`
- Create: `examples/compose-base/.dembly/config.toml`
- Create: `examples/compose-base/.devcontainer/devcontainer.json`
- Create: `examples/compose-base/.devcontainer/initialize-host.sh`
- Delete: `examples/image-base/README.md`
- Delete: `examples/image-base/deck.toml`
- Modify: `tests/TRACEABILITY.md`

**Interfaces:**
- Documentation teaches `init -> validate -> lock -> apply -> docker compose ... -> down`, and `unapply` only after `down`.
- Examples use `.dembly/config.toml`, top-level Compose `name`, embedded `x-dembly`, and no Image Base.
- Traceability maps every `design/spec.md` acceptance condition to automated evidence or an explicit manual environment prerequisite.

- [ ] **Step 1: Update the runnable Compose example first**

Make the example executable from its root with the documented `-f` list.
Include a user-managed Host initialization script for Dev Containers that runs `validate`, conditionally instructs the user to refresh Lock, and runs `apply`; do not have Dembly rewrite `initializeCommand`.

- [ ] **Step 2: Rewrite both README workflows**

Remove `deck.toml`, `deck.lock`, Image Base, implicit `dembly-*` project names, and Dembly lifecycle commands.
Document native `up`, `ps`, `logs`, `exec --user`, `run`, Runtime check, and `down`, plus the same-file-order requirement and the `-p`/`COMPOSE_PROJECT_NAME` exclusion.
Keep English and Japanese documents equivalent; use one sentence per line in Japanese prose.

- [ ] **Step 3: Replace traceability entries**

Map the final acceptance items to the new unit/CLI/integration test names.
Delete obsolete REQ/IT rows rather than marking removed behavior verified.
Record Linux requirements: Docker Engine, Docker Compose plugin, root/privileged container capability, loop/SquashFS support, and the optional Dev Containers CLI validation prerequisite.

- [ ] **Step 4: Run formatting, static checks, tests, and legacy scans**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
git diff --check
rg -n 'deck\.toml|deck\.lock|dembly (up|down|run|exec)|Image Base|image-base|compose\.override' \
  README.md README.ja.md examples crates tests design/CONTEXT.md
```

Expected: all commands pass; the final search returns no stale product wording except the migration document's deliberate historical comparison.

- [ ] **Step 5: Review against the canonical specification**

Read `design/spec.md` sections 2 through 14 in order and check off every input, output, failure mode, ownership rule, Docker Compose relation, and acceptance condition against code/tests/docs.
Confirm `design/spec.md` itself has no implementation-driven edits in this migration branch.

- [ ] **Step 6: Commit**

```bash
git add README.md README.ja.md examples tests/TRACEABILITY.md design/CONTEXT.md
git commit -m "docs: adopt native compose workflow"
```

Use a Japanese body stating that examples, command guidance, prerequisites, and traceability now match the canonical specification.

## Final Verification

- [ ] Run `git status --short` and account for every changed path.
- [ ] Run `git diff --check`.
- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- [ ] Run `cargo test --workspace` on a Docker/Compose/SquashFS-capable Linux host.
- [ ] Run the native Compose acceptance test alone with `--nocapture` and preserve its command log as evidence.
- [ ] Run the legacy-term scan from Task 12 and confirm only the migration specification contains historical names.
- [ ] Compare `git diff -- design/spec.md` and confirm it is empty.
- [ ] Review commit boundaries: each commit has one purpose, an English Conventional Commit title, and a concise Japanese body.
