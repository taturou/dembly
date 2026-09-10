# Integration Coverage Completion Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `executing-plans` task-by-task. This repository is intentionally edited in the current workspace; do not create a worktree.

**Goal:** Verify every IT-001 through IT-040 behavior in `design/spec.md` using only synthetic Cards and local Docker fixtures.

**Architecture:** Keep Docker-dependent end-to-end cases in `crates/dembly-cli/tests/integration.rs`, sharing fixture builders that create a temporary Deck, Cards, and an Alpine Base. Keep parser/planner unit tests unchanged; this plan adds runtime evidence rather than duplicating them. Update `tests/TRACEABILITY.md` only after the mapped integration test passes.

**Tech Stack:** Rust 1.85.1 through mise, Docker Engine, Docker Compose v2, kernel SquashFS, Alpine fixture images.

**Spec:** `design/spec.md`, section 39 (IT-001 through IT-040).

## Global Constraints

- Use test-only Cards; no eMCOS, clang, or TIS artifact is required (REQ-DOD-016).
- Build and run the same musl `dembly` binary on Host and Runtime (REQ-GEN-003).
- Do not mount Card SquashFS on the Host or alter user-authored Compose files.
- Use `mise exec rust@1.85.1 -- cargo ...`; the shell default Rust is 1.75.0 and cannot compile this source tree.

---

### Task 1: Extend the Image fixture for Card filesystem semantics

**Files:**

- Modify: `tests/fixtures/hello-card/rootfs/` and `crates/dembly-cli/tests/integration.rs`
- Modify: `tests/TRACEABILITY.md`

**Interfaces:**

- Test helper creates named synthetic Cards with distinct mount targets from fixture roots.
- The Image integration test directly invokes `dembly validate`, `lock`, `run`, `up`, `exec`, `down`, and `check`.

- [ ] Add a second Card, a Card-internal symlink, and a dynamically linked or mmap-backed fixture executable.
- [ ] Add assertions for IT-002, IT-004, IT-005, IT-007, IT-009, IT-011, IT-012, and IT-013.
- [ ] Run `mise exec rust@1.85.1 -- cargo test -p dembly-cli --test integration` and update the matching traceability rows.
- [ ] Commit with `test(integration): cover card runtime semantics`.

### Task 2: Cover persistent paths and Host Bind resolution end-to-end

**Files:**

- Modify: `crates/dembly-cli/tests/integration.rs`
- Modify: `tests/TRACEABILITY.md`

**Interfaces:**

- Each assertion uses a fresh temporary Deck root and reads the resulting Host path or command status.

- [ ] Add cases for private Card volume, shared volume, shared/private collision, volume symlink rejection, `${HOST_HOME}`, `${USER}`, `${HOME}`, optional missing bind warning, and required missing bind error (IT-015 through IT-018 and IT-021 through IT-026).
- [ ] Verify each command uses explicit `deck.toml` or the temporary Deck working directory, so discovery is part of the behavior under test.
- [ ] Run the integration suite and update traceability.
- [ ] Commit with `test(integration): cover storage and bind contracts`.

### Task 3: Cover lock and no-rebuild contracts

**Files:**

- Modify: `crates/dembly-cli/tests/integration.rs`
- Modify: `tests/TRACEABILITY.md`

**Interfaces:**

- The test records `docker image inspect --format {{.Id}}` before and after changing the selected Card set.

- [ ] Add a stale-lock case by mutating a selected Card manifest after `dembly lock`, and assert `run` fails before container creation (IT-034).
- [ ] Add a Card-selection change case and assert the Base image ID is unchanged while the newly selected Card executes (IT-028).
- [ ] Run the integration suite and update traceability.
- [ ] Commit with `test(integration): cover lock and base reuse`.

### Task 4: Cover non-selected Compose services

**Files:**

- Modify: `crates/dembly-cli/tests/integration.rs`
- Modify: `tests/TRACEABILITY.md`

**Interfaces:**

- Compose fixture declares selected `dev` and non-selected `sidecar` services.
- The test queries `docker compose ... exec sidecar` after `dembly up`.

- [ ] Add a long-running non-selected service and assert it remains command-executable after selected-service Runtime initialization (IT-037).
- [ ] Preserve existing IT-036, IT-038, and IT-039 assertions.
- [ ] Run the integration suite and update traceability.
- [ ] Commit with `test(compose): preserve non-selected services`.

### Task 5: Final traceability and quality gate

**Files:**

- Modify: `tests/TRACEABILITY.md`

- [ ] Mark every IT-001 through IT-040 as verified only when its mapped test has passed in this environment.
- [ ] Run `mise exec rust@1.85.1 -- cargo fmt --check`.
- [ ] Run `mise exec rust@1.85.1 -- cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- [ ] Run `mise exec rust@1.85.1 -- cargo test --workspace`.
- [ ] Run `mise exec rust@1.85.1 -- cargo build --release --target x86_64-unknown-linux-musl -p dembly-cli`.
- [ ] Commit with `docs(test): complete integration traceability`.
