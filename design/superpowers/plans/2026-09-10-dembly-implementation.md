# Dembly PoC Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the Linux x86_64 Dembly PoC as one static `dembly` executable that materializes Image Base and Compose Base runtimes from immutable SquashFS Cards.

**Architecture:** `dembly-core` resolves, validates, and plans all domain data without Docker execution. `dembly-docker` turns plans into Docker CLI or Compose CLI arguments, while `dembly-runtime` implements the internal initializer invoked from the injected same executable. `dembly-cli` is the sole binary and command dispatcher.

**Tech Stack:** Rust stable, clap, serde, toml, sha2, thiserror, tempfile, nix, Docker CLI, Docker Compose v2, SquashFS tools.

**Spec:** `design/spec.md`

## Global Constraints

- Use Rust and distribute only one executable named `dembly` (REQ-GEN-001 through REQ-GEN-003).
- Use TOML for every Dembly-owned configuration and reject unknown fields (REQ-CFG-001 through REQ-CFG-004).
- Mount SquashFS only inside the privileged Runtime. Never mount it on the Host and never use FUSE (REQ-GEN-005 through REQ-GEN-007, REQ-SCP-001).
- Keep Docker command execution outside `dembly-core` (REQ-RST-010).
- Do not rebuild a Base only because selected Cards changed (REQ-GEN-004, REQ-CNS-003).
- Keep persistent Volume directories under Deck root. Do not use Docker named volumes or delete them from `down` (REQ-SCP-002, REQ-VOL-030 through REQ-VOL-034).
- Do not mutate user-authored Compose files. Generate an ephemeral override (REQ-CMP-010 through REQ-CMP-011).
- Require explicit `dembly lock`; `up`, `run`, and `check` must not update it (REQ-LCK-020 through REQ-LCK-023).
- Detect defined conflicts before runtime creation. Never apply last-wins behavior (REQ-VAL-001 through REQ-VAL-002).
- Implement Image Base Card materialization before Compose Base (REQ-IMP-001 through REQ-IMP-002).
- Put mise configuration at `.mise/config.toml` per the user instruction. Record this deliberate divergence from REQ-DEV-001 in traceability and README (REQ-TRC-003).

---

### Task 1: Workspace and Linux development contract

**Files:**

- Create: `Cargo.toml`, `.mise/config.toml`, `scripts/check-linux.sh`, `scripts/setup-dev.sh`
- Create: `crates/dembly-{core,docker,card,runtime,cli}/Cargo.toml`
- Create: `crates/dembly-{core,docker,card,runtime,cli}/src/lib.rs`, `crates/dembly-cli/src/main.rs`
- Test: `scripts/check-linux.sh` manually

**Interfaces:**

- Produces workspace packages `dembly_core`, `dembly_docker`, `dembly_card`, `dembly_runtime`, and binary `dembly`.
- Produces `scripts/check-linux.sh` returning 0 only when every prerequisite is present.

- [ ] **Step 1: Define workspace and dependency versions**

```toml
[workspace]
members = ["crates/dembly-core", "crates/dembly-docker", "crates/dembly-card", "crates/dembly-runtime", "crates/dembly-cli"]
resolver = "2"
```

- [ ] **Step 2: Add an initially failing CLI smoke test**

```rust
#[test]
fn binary_exposes_public_help_without_runtime_namespace() {
    let output = assert_cmd::Command::cargo_bin("dembly").unwrap().arg("--help").output().unwrap();
    assert!(String::from_utf8_lossy(&output.stdout).contains("validate"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("__runtime"));
}
```

- [ ] **Step 3: Implement minimal clap root command and empty crate boundaries**

```rust
#[derive(clap::Parser)]
#[command(name = "dembly")]
struct Cli { #[command(subcommand)] command: Command }
```

- [ ] **Step 4: Implement prerequisite scripts**

`check-linux.sh` checks Linux, `x86_64`, `git`, `mise`, `docker`, `docker info`, `docker compose version`, `mount`, `mksquashfs`, `/proc/filesystems` SquashFS entry, loop support, and `rustup target list --installed` for musl.

`setup-dev.sh` resolves its own repository root, runs checker, executes `mise install`, and runs `rustup target add x86_64-unknown-linux-musl` plus `rustup component add rustfmt clippy` without `sudo`.

- [ ] **Step 5: Verify and commit**

Run: `cargo test -p dembly-cli`, `cargo fmt --check`, `scripts/check-linux.sh`.

Commit: `build: scaffold dembly workspace and development scripts`

### Task 2: Strict domain models and Deck discovery

**Files:**

- Create: `crates/dembly-core/src/{model.rs,parse.rs,discovery.rs,error.rs,lib.rs}`
- Test: `crates/dembly-core/src/{parse,discovery}.rs`

**Interfaces:**

- Produces `pub fn discover_deck(explicit: Option<&Path>, cwd: &Path) -> Result<PathBuf, CoreError>`.
- Produces `pub fn load_deck(path: &Path) -> Result<DeckDocument, CoreError>` and `pub fn load_card(path: &Path) -> Result<CardDocument, CoreError>`.

- [ ] **Step 1: Write failing parser and discovery tests for UT-001, UT-002, UT-004, UT-005**

```rust
assert!(load_deck(&fixture("unknown-deck-field.toml")).unwrap_err().to_string().contains("unknown field"));
assert_eq!(discover_deck(None, child_dir.as_ref()).unwrap_err().kind(), ErrorKind::DeckNotFound);
```

- [ ] **Step 2: Implement serde `deny_unknown_fields` models**

Models cover image/compose Base, Card references, `Bind`, `Volume`, `Export`, `Hook`, `Check`, `DeckLock`, and `RuntimeConfig` with `schema_version` fields.

- [ ] **Step 3: Implement discovery without parent traversal**

```rust
let candidate = explicit.map(PathBuf::from).unwrap_or_else(|| cwd.join("deck.toml"));
if !candidate.is_file() { return Err(CoreError::deck_not_found(candidate)); }
```

- [ ] **Step 4: Run and commit**

Run: `cargo test -p dembly-core parse discovery`.

Commit: `feat(core): add strict deck and card parsing`

### Task 3: Resolution, validation, and deterministic plans

**Files:**

- Create: `crates/dembly-core/src/{resolve.rs,validate.rs,environment.rs,checksum.rs}`
- Test: `crates/dembly-core/src/{resolve,validate,environment,checksum}.rs`

**Interfaces:**

- Produces `pub fn resolve_deck(input: ResolveInput) -> Result<ResolvedDeck, CoreError>`.
- `ResolvedDeck` contains resolved Cards, ordered mounts, Volumes, Binds, Exports, environment, and lock-comparison input.

- [ ] **Step 1: Write failing UT-006 through UT-017**

Cover Deck-root relative paths, exact `${VARIABLE}` expansion, undefined variable errors, name regexes, all three Volume layouts, shared/private mixing, symlink rejection, exact mount collision, duplicate exports, Card environment conflict, PATH order, and SHA-256 mismatch.

- [ ] **Step 2: Implement path and variable contracts**

```rust
pub struct BindVariables { pub host_home: PathBuf, pub deck_root: PathBuf, pub user: String, pub home: String }
pub fn expand_source(value: &str, vars: &BindVariables) -> Result<PathBuf, CoreError>;
pub fn expand_target(value: &str, vars: &BindVariables) -> Result<PathBuf, CoreError>;
```

- [ ] **Step 3: Implement volume and collision planners**

Treat nested targets as valid, but reject exact duplicate targets across Card mounts, Volumes, and Binds. Reject a physical Volume path when `symlink_metadata` reports a symlink.

- [ ] **Step 4: Implement environment planner**

Card normal environment keys must be unique across selected Cards. Build PATH as Card-order relative entries resolved beneath each Card mount root, then Deck prepend, then Base/Compose PATH.

- [ ] **Step 5: Run and commit**

Run: `cargo test -p dembly-core`.

Commit: `feat(core): resolve and validate runtime plans`

### Task 4: Card builder and artifact integrity

**Files:**

- Create: `crates/dembly-card/src/{build.rs,prompt.rs,lib.rs}`
- Modify: `crates/dembly-cli/src/main.rs`
- Test: `crates/dembly-card/src/build.rs`

**Interfaces:**

- Produces `pub fn build_card(request: CardBuildRequest, io: &mut dyn PromptIo) -> Result<CardBuildResult, CardBuildError>`.

- [ ] **Step 1: Write failing tests for non-interactive required fields and manifest rendering**

```rust
assert!(build_card(request_without_name_or_version(), &mut io).unwrap_err().to_string().contains("--name"));
```

- [ ] **Step 2: Implement option/prompt parity**

Use tool-root basename as the interactive Card-name default. Prompt only in interactive mode. Never prompt for checksum.

- [ ] **Step 3: Implement `mksquashfs` invocation and atomic output**

Invoke `mksquashfs <tool-root> <temporary-output> -noappend -comp zstd`, calculate SHA-256, atomically rename, then write strict `card.toml`. Do not copy or modify tool root.

- [ ] **Step 4: Run and commit**

Run: `cargo test -p dembly-card`.

Commit: `feat(card): build squashfs card artifacts`

### Task 5: Docker command planning and runtime user probe

**Files:**

- Create: `crates/dembly-docker/src/{command.rs,image.rs,user.rs,lib.rs}`
- Test: `crates/dembly-docker/src/{command,user}.rs`

**Interfaces:**

- Produces `pub trait DockerClient` and `pub struct CliDockerClient`.
- Produces `pub fn image_runtime_plan(...) -> Result<Vec<OsString>, DockerError>`.
- Produces `pub fn resolve_runtime_user(...) -> Result<RuntimeUser, DockerError>`.

- [ ] **Step 1: Write UT-018 for image inspect, create, exec, and run command arguments**

Assert the plan contains `--privileged`, read-only same-executable bind, one read-only Card file bind per Card, labels, root user, and internal entrypoint.

- [ ] **Step 2: Implement command-only Docker adapter**

All subprocess calls use `Command` with argv arrays. Return command, status, stdout, and stderr in typed errors without printing secret file contents.

- [ ] **Step 3: Implement user precedence and internal probe**

Resolve Compose service user, then `Config.User`, then root. Create a short-lived probe container using the injected static binary; parse `/etc/passwd` and `/etc/group` in `__runtime probe-user`, not shell or `getent`.

- [ ] **Step 4: Run and commit**

Run: `cargo test -p dembly-docker`.

Commit: `feat(docker): plan image runtimes and resolve users`

### Task 6: Runtime initializer

**Files:**

- Create: `crates/dembly-runtime/src/{init.rs,mount.rs,process.rs,lib.rs}`
- Modify: `crates/dembly-cli/src/main.rs`
- Test: `crates/dembly-runtime/src/{init,mount}.rs`

**Interfaces:**

- Produces `pub fn initialize(config_path: &Path) -> Result<Infallible, RuntimeError>`.
- Produces `pub fn probe_user(request: ProbeRequest) -> Result<RuntimeUser, RuntimeError>`.

- [ ] **Step 1: Write unit tests for runtime TOML validation and ordered command plan**

Assert the order is Card mount, supplied mount verification, export, environment, hook, privilege drop, final `exec`.

- [ ] **Step 2: Implement external SquashFS mount**

Run `mount -t squashfs -o loop,ro /run/dembly/cards/<name>.squashfs <target>`. Do not use Rust `mount(2)`. Fail startup on non-zero status.

- [ ] **Step 3: Implement export, hooks, and final process**

Reject non-Dembly existing export targets. Run hooks in Card and declaration order with Card root CWD and final environment. Use narrowly isolated `nix` calls for `setgid`, `setuid`, and `execvp`, documenting root-init assumptions.

- [ ] **Step 4: Run and commit**

Run: `cargo test -p dembly-runtime`.

Commit: `feat(runtime): initialize cards inside privileged containers`

### Task 7: Locking, runtime state, and Image Base lifecycle

**Files:**

- Create: `crates/dembly-core/src/lock.rs`
- Modify: `crates/dembly-docker/src/image.rs`, `crates/dembly-cli/src/main.rs`
- Test: `crates/dembly-core/src/lock.rs`, `crates/dembly-docker/src/image.rs`

**Interfaces:**

- Produces `pub fn write_lock(deck: &ResolvedDeck, base: BaseIdentity) -> Result<(), CoreError>`.
- Produces `pub fn enforce_lock(deck: &ResolvedDeck, lock: &DeckLock) -> Result<(), CoreError>`.

- [ ] **Step 1: Write tests for lock writer/parser, missing lock, and stale Cards/Base**

Verify `up`, `run`, `check` receive an error that tells the user to run `dembly lock`; they never write the lock.

- [ ] **Step 2: Implement secure runtime-state directories**

Prefer `$XDG_RUNTIME_DIR/dembly`, otherwise create a user-and-Deck scoped directory in OS temp with mode `0700`. Write resolved `runtime.toml`; never copy bind content.

- [ ] **Step 3: Implement Image Base `validate`, `lock`, `up`, `down`, `run`, `exec`, `inspect`, `check`**

Use labels as authoritative ownership. `up` errors if the managed Runtime exists. `down` verifies labels, removes only Runtime and metadata. `run`/`check` create temporary containers and remove them in a cleanup guard. `exec` passes intended user.

- [ ] **Step 4: Run and commit**

Run: `cargo test --workspace`.

Commit: `feat(cli): add image base lifecycle and lock enforcement`

### Task 8: Compose Base override and lifecycle

**Files:**

- Create: `crates/dembly-docker/src/compose.rs`
- Modify: `crates/dembly-docker/src/lib.rs`, `crates/dembly-cli/src/main.rs`
- Test: `crates/dembly-docker/src/compose.rs`

**Interfaces:**

- Produces `pub fn compose_override(plan: &RuntimePlan) -> Result<String, DockerError>`.
- Produces Compose `up`, `down`, image identity, and effective argv operations.

- [ ] **Step 1: Write override-generation unit tests**

Assert only selected service changes; assert original Compose file is read-only input; assert override adds mounts, root initializer, privileged flag, labels, and internal entrypoint without appending original command twice.

- [ ] **Step 2: Implement Compose identity and lifecycle**

Use project name `dembly-<deck-name>`. Capture original selected-service entrypoint and command in runtime TOML. Start the full project for `up`, preserve other services, and run project `down` only after ownership verification.

- [ ] **Step 3: Run and commit**

Run: `cargo test -p dembly-docker`.

Commit: `feat(compose): materialize selected compose service`

### Task 9: Integration fixtures and acceptance suite

**Files:**

- Create: `tests/fixtures/hello-card/rootfs/{bin,setup,lib}/...`
- Create: `tests/integration/{common,image_base,compose_base}.rs`
- Create: `tests/TRACEABILITY.md`

**Interfaces:**

- Produces test fixture setup that creates SquashFS with `mksquashfs` and skips only when prerequisites are absent.

- [ ] **Step 1: Implement prerequisite-gated fixture builder**

Use `cargo test --test image_base` only after confirming Docker daemon, Compose v2, SquashFS, loop device, and musl executable availability. Report skipped environment-prerequisite cases distinctly.

- [ ] **Step 2: Implement Image Base IT-001 through IT-035**

Cover mount semantics, executable/shared library/symlink/export/PATH/hook, checksum and validation errors, Volumes, Binds and variables, no Host mount namespace entry, no Base rebuild on Card selection, same binary, static binary, user, lock, and lifecycle.

- [ ] **Step 3: Implement Compose IT-036 through IT-040**

Cover selected service initialization, non-selected service health, preserved original process, `run` replacement, and non-parent discovery.

- [ ] **Step 4: Write traceability table and run**

Map UT-001 through UT-018 and IT-001 through IT-040 to Requirement IDs. Run `cargo test`; record executed and skipped suites separately.

Commit: `test: add docker and squashfs acceptance coverage`

### Task 10: Documentation and release verification

**Files:**

- Modify: `README.md`
- Create: `examples/image-base/{deck.toml,README.md}`, `examples/compose-base/{deck.toml,compose.yaml,README.md}`
- Modify: `tests/TRACEABILITY.md`

**Interfaces:**

- Produces documented commands that match the implemented CLI.

- [ ] **Step 1: Document all REQ-DOC-001 topics**

Include concepts, architecture, Linux prerequisites, setup/check scripts, build/test/lint, Card format and build modes, Image/Compose examples, Volume layouts, bind variables, lock workflow, trusted privileged Card risks, non-goals, and five-run median benchmark procedure with `<= 10%` as a target rather than guarantee.

- [ ] **Step 2: Add disk and performance recording procedures**

Use `hyperfine` or repeated `/usr/bin/time`, `docker system df`, `docker image inspect`, `du`, and SquashFS size. Include a result-table template; do not invent measurements.

- [ ] **Step 3: Build static binary and run quality gates**

Run: `cargo build --release --target x86_64-unknown-linux-musl`.

Run: `cargo fmt --check`.

Run: `cargo clippy --all-targets --all-features -- -D warnings`.

Run: `cargo test`.

- [ ] **Step 4: Commit**

Commit: `docs: document dembly poc usage and evaluation`

## Self-review

The plan covers workspace and development scripts, strict TOML, Card build, generic Cards, Image Base before Compose, same executable injection, user probe, Runtime-only SquashFS mount, lifecycle, Volume/Bind layouts, locking, Compose override, synthetic fixture integration tests, traceability, README, performance, disk evaluation, and quality gates.

The deliberate `.mise/config.toml` path divergence is recorded and remains the only planned difference from the formal specification.
