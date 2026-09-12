# Release Distribution and Documentation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a versioned Linux x86_64 Dembly binary through GitHub Releases, provide a verified version-switching installer, and replace the public documentation in English and Japanese.

**Architecture:** Cargo workspace metadata defines the only version and license. `scripts/release.sh` builds and packages the CLI, renders `scripts/dembly-install.sh.in`, and is the only publisher. The installer stores immutable releases under XDG data and exposes one active binary through a symlink.

**Tech Stack:** Rust 1.85.1 via mise, Cargo workspace inheritance, Bash, GitHub CLI, Docker Compose, Markdown, Mermaid.

**Spec:** `design/superpowers/specs/2026-09-12-release-distribution-and-documentation-design.md`

## Global Constraints

- Use root `Cargo.toml` `[workspace.package].version` as the sole release-version definition.
- Preserve root `LICENSE` verbatim; do not call the license MIT or an OSS license.
- Support Linux x86_64 and `x86_64-unknown-linux-musl` only; do not use GitHub Actions.
- Call Cargo with `mise exec -- cargo`; release gates require `--locked`.
- Keep `dist/` ignored. `--clean X.Y.Z` deletes only its exact tarball, checksum, and build-info paths.
- README files must be complete English/Japanese counterparts with mutual links. The Japanese README uses one sentence per line.
- Do not publish a tag or GitHub Release while implementing or testing.

## Final whole-branch review fixes

**Goal:** Keep release quality gates version-independent and make installer and release-script filesystem effects safe under adversarial local paths.

**Files:** `crates/dembly-cli/tests/help.rs`, `scripts/dembly-install.sh.in`, `scripts/test-dembly-install.sh`, `scripts/release.sh`, and `scripts/test-release.sh`.

1. First make the CLI test derive its expected newline-terminated version from `env!("CARGO_PKG_VERSION")`; run that focused Cargo test before and after the test edit.
2. Add installer integration cases in which `licenses` and `licenses/dembly` are symlinks to an outside sentinel. Run the rendered installer and assert failure plus an unchanged sentinel. Then reject both paths before the license `mkdir` or `cp`.
3. Add release-fixture assertions that `1.2.3+build-id` invokes `gh release create` without `--prerelease`, while `1.2.3-rc.1+build-id` includes it. Implement classification with the SemVer prerelease separator restricted to the core-version portion before `+` metadata.
4. Add a dry-run quality-gate failure fixture and assert `git worktree list --porcelain` contains no temporary worktree afterwards. Implement worktree removal through a function-local `RETURN` trap so subshell failure cannot skip deregistration.
5. Add normal-preflight fixture cases: an untracked ordinary file fails before authentication, and ignored `dist/`/`target/` paths do not fail. Implement a `git status --porcelain --untracked-files=all` check, preserving ignored files as allowed.

**Verification:** Run both shell integration scripts, their `bash -n` checks, ShellCheck, focused CLI Cargo test, full locked Cargo tests, and a safe release dry-run when the local toolchain permits it. Commit all final review fixes together as one `fix(release): harden final distribution checks` commit with a Japanese body.

---

### Task 1: Centralize Cargo metadata and implement global version output

**Files:**
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `crates/dembly-{card,cli,core,docker,runtime}/Cargo.toml`
- Modify: `crates/dembly-cli/src/main.rs:12-45`
- Modify: `crates/dembly-cli/tests/help.rs`
- Create: `LICENSE`
- Modify: `.gitignore`

**Interfaces:**
- Produces: `dembly --version -> X.Y.Z\\n`, sourced from `env!("CARGO_PKG_VERSION")`.
- Preserves: nested `dembly card build --version VALUE` parsing.

- [ ] **Step 1: Add the failing binary test**

Add this test to `crates/dembly-cli/tests/help.rs`.

```rust
#[test]
fn global_version_is_bare_package_version() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_dembly"))
        .arg("--version")
        .output()
        .expect("dembly should start");
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "0.1.0\\n");
    assert!(output.stderr.is_empty());
}
```

- [ ] **Step 2: Prove it fails**

Run: `mise exec -- cargo test --locked -p dembly-cli --test help global_version_is_bare_package_version`

Expected: the current CLI exits 2 because it has no global version dispatch.

- [ ] **Step 3: Make package fields inherit from the workspace**

Set root `[workspace.package]` fields to `version = "0.1.0"`, `license-file = "LICENSE"`, and `publish = false`; remove `license = "MIT"`.
In every member package replace its literal version with `version.workspace = true`, replace `license.workspace = true` with `license-file.workspace = true`, and add `publish.workspace = true`.
Write the approved license text exactly to root `LICENSE`; add `/dist/` to `.gitignore`; regenerate the lockfile with `mise exec -- cargo generate-lockfile`.
Before help/subcommand dispatch, add:

```rust
if argument == "--version" {
    println!("{}", env!("CARGO_PKG_VERSION"));
    return ExitCode::SUCCESS;
}
```

- [ ] **Step 4: Verify metadata and behavior**

Run:

```bash
mise exec -- cargo test --locked -p dembly-cli --test help
mise exec -- cargo metadata --locked --no-deps --format-version 1
```

Expected: tests pass and metadata gives all Dembly packages version `0.1.0`, a `LICENSE` license file, and `publish = false`.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock .gitignore LICENSE crates/dembly-*/Cargo.toml \
  crates/dembly-cli/src/main.rs crates/dembly-cli/tests/help.rs
git commit -m "feat: centralize release metadata"
```

Use a Japanese body stating that workspace metadata and machine-readable CLI version output were added.

### Task 2: Create a safe installer template with isolated tests

**Files:**
- Create: `scripts/dembly-install.sh.in`
- Create: `scripts/test-dembly-install.sh`

**Interfaces:**
- Consumes: rendered `@VERSION@` and `@REPOSITORY@` markers.
- Produces: `dembly-install.sh [--list | --uninstall X.Y.Z | --purge]`.

- [ ] **Step 1: Write a failing temporary-directory installer test**

Write `scripts/test-dembly-install.sh` with `set -euo pipefail`, `mktemp -d`, and a trap. Put fake `curl` first in `PATH`; it copies a locally generated archive and checksum.
The archive must have exactly `dembly-v1.2.3-x86_64-unknown-linux-musl/{dembly,LICENSE}`; its binary prints `1.2.3`.
Assert:

```bash
test -x "$test_data/dembly/releases/1.2.3/dembly"
test -f "$test_data/dembly/releases/1.2.3/LICENSE"
test "$(readlink "$test_home/.local/bin/dembly")" = "$test_data/dembly/releases/1.2.3/dembly"
test "$("$test_home/.local/bin/dembly" --version)" = '1.2.3'
```

Also test repeat-install link replacement, `--list`, active-version uninstall refusal, regular-file link refusal, and `--purge` scoped to test Dembly paths.

- [ ] **Step 2: Prove it fails**

Run: `bash scripts/test-dembly-install.sh`

Expected: failure because the template does not exist.

- [ ] **Step 3: Implement the installer**

Embed `dembly_version='@VERSION@'` and `repository='@REPOSITORY@'`.
Use only derived locations:

```bash
data_home=${XDG_DATA_HOME:-"$HOME/.local/share"}
release_root="$data_home/dembly/releases/$dembly_version"
active_link="$HOME/.local/bin/dembly"
license_destination="$data_home/licenses/dembly/LICENSE"
```

Download the exact versioned tarball and `.sha256` from GitHub, verify with `sha256sum --check`, extract to a temporary directory, and require `dembly --version` to equal the embedded version.
If an existing release directory already has a valid binary and `LICENSE`, do not download it.
Refuse an existing non-symlink active path; replace only a symlink with `ln -sfnT`.
Validate SemVer before `--uninstall`; reject removal of the active release. `--purge` removes only derived Dembly release, symlink, and shared-license paths.

- [ ] **Step 4: Verify and commit**

Run:

```bash
bash -n scripts/dembly-install.sh.in
bash scripts/test-dembly-install.sh
```

Commit with `feat: add verified release installer` and a Japanese body describing checksum verification and deletion scope.

### Task 3: Implement local release orchestration

**Files:**
- Create: `scripts/release.sh`
- Create: `scripts/test-release.sh`

**Interfaces:**
- Consumes: `[X.Y.Z]`, `--dry-run [X.Y.Z]`, or `--clean X.Y.Z`.
- Produces: versioned tarball, checksum, build-info, installer asset, and only in normal mode an annotated `vX.Y.Z` tag and public release.

- [ ] **Step 1: Write failing release-script tests**

Use a temporary Git repository and fake `mise`, `gh`, and Cargo commands in `scripts/test-release.sh`.
Assert invalid SemVer fails, `--clean 1.2.3` removes only these exact names, dry-run leaves caller `Cargo.toml` unchanged, and a mismatching existing build-info aborts rather than overwriting artifacts.

```bash
dist/dembly-v1.2.3-x86_64-unknown-linux-musl.tar.gz
dist/dembly-v1.2.3-x86_64-unknown-linux-musl.tar.gz.sha256
dist/dembly-v1.2.3-x86_64-unknown-linux-musl.build-info
```

- [ ] **Step 2: Prove it fails**

Run: `bash scripts/test-release.sh`

Expected: failure because `scripts/release.sh` does not exist.

- [ ] **Step 3: Implement preflight and local packaging**

Accept only the four documented invocation forms. Require normal mode to have a clean `main` worktree at `origin/main`, required commands, installed mise target/components, and `gh auth status`.
When a version argument is passed, update only `[workspace.package]` using a section-aware `awk` rewrite, regenerate the lockfile, and reject any tracked diff outside root `Cargo.toml` and `Cargo.lock`.
Run `scripts/check-linux.sh`, formatter, clippy, tests, and:

```bash
mise exec -- cargo build --locked --release --target x86_64-unknown-linux-musl -p dembly-cli
```

Require the produced binary to print the selected version.
Render the installer markers with `sed`, package binary plus root `LICENSE` in the specified inner directory, checksum it, and write build-info with version, target, HEAD SHA, and SHA-256.
Reuse only an artifact whose manifest and checksum match current HEAD.
Implement dry-run in a `mktemp -d` detached Git worktree; it must never commit, tag, push, or call GitHub Release APIs.

- [ ] **Step 4: Implement normal publication**

Reject an already existing local/remote tag or GitHub Release.
For explicit version changes, commit root `Cargo.toml` and `Cargo.lock` only with `chore(release): prepare vX.Y.Z` and Japanese body, then push `main`.
Create and push an annotated unsigned tag.
Run `gh release create` with title `Dembly vX.Y.Z`, fixed English notes, tarball, checksum, rendered installer, and `--prerelease` only for prereleases.
On upload failure, preserve the tag and print the rerun command.

- [ ] **Step 5: Verify and commit**

Run:

```bash
bash -n scripts/release.sh
bash scripts/test-release.sh
bash scripts/release.sh --dry-run
```

Commit with `feat: add local release automation` and a Japanese body describing safe dry-run and exact artifact cleanup.

### Task 4: Replace English public documentation and examples

**Files:**
- Modify: `README.md`
- Modify: `examples/compose-base/README.md`
- Modify: `examples/image-base/README.md`
- Create: `scripts/test-documentation.sh`

**Interfaces:**
- Produces: an English public guide usable without repository clone and current example instructions.

- [ ] **Step 1: Write failing documentation assertions**

Make `scripts/test-documentation.sh` assert both installer URLs, a `README-ja.md` link, Mermaid fence, quickstart commands, `dembly --version`, and `LICENSE` link in `README.md`.
Reject `performance measurement`, benchmark procedure headings, `implementation in progress`, and MIT text.
Require each example README to contain its documented build/run commands.

- [ ] **Step 2: Prove it fails**

Run: `bash scripts/test-documentation.sh`

Expected: current README content and stale example claims violate the checks.

- [ ] **Step 3: Rewrite README.md**

Use this exact section order: Language, Overview, Security warning, Installation, Compose Quickstart, Architecture, Core concepts, Base types, Cards, Deck configuration, CLI reference, Development, Limitations and evaluation, License.
Show both commands:

```sh
curl -fsSL https://github.com/taturou/dembly/releases/latest/download/dembly-install.sh | sh
curl -fsSL https://github.com/taturou/dembly/releases/download/v1.2.3/dembly-install.sh | sh
```

Quickstart must link the GitHub `examples/compose-base` files and run `docker compose pull`, `dembly validate`, `dembly lock`, `dembly up`, `dembly exec -- /bin/echo compose-runtime`, optional shell, then `dembly down`.
Explain there that up/down affect only all services in that deck-root Compose project.
Use a Mermaid diagram proving Runtime, not Host, mounts Card SquashFS.
Document Image and Compose Base lifecycle, separate Card/Volume/Bind/Environment/lock snippets, a complete Compose template with `<replace-me>`, all public CLI commands, and the four illustrative artifact-layout rows: Base, Base + ATfEP, Base + Clang, Base + ATfEP + TIS.

- [ ] **Step 4: Rewrite English example README files**

For Compose Base, describe the selected `dev` service, `database`, and generated override without calling it incomplete.
For Image Base, document exactly:

```bash
docker build -t dembly-fixture-base:local ../../tests/fixtures/image-base
dembly card build ../../tests/fixtures/hello-card/rootfs ./cards \
  --name hello --version 1 --mount-target /opt/dembly/cards/hello \
  --path-prepend bin --non-interactive
dembly validate
dembly lock
dembly run -- /bin/true
```

- [ ] **Step 5: Verify and commit**

Run: `bash scripts/test-documentation.sh`

Commit with `docs: rewrite English public guide` and a Japanese body describing release installation and current Base examples.

### Task 5: Add Japanese counterpart and complete verification

**Files:**
- Create: `README-ja.md`
- Modify: `scripts/test-documentation.sh`
- Verify: all files changed above

**Interfaces:**
- Produces: a complete Japanese public guide with equal commands and constraints.

- [ ] **Step 1: Extend the documentation test for parity**

Require mutual root README links; require `README-ja.md` to include both URLs, the Compose commands, `dembly --version`, `LICENSE`, and the same four artifact-layout cases.

- [ ] **Step 2: Prove it fails**

Run: `bash scripts/test-documentation.sh`

Expected: failure because `README-ja.md` does not exist.

- [ ] **Step 3: Write README-ja.md**

Translate every English section and preserve literal commands, paths, URLs, identifiers, Mermaid source, and the license reference.
Use one Japanese sentence per line and include at top:

```markdown
English documentation: [README.md](README.md)
```

- [ ] **Step 4: Run complete verification**

Run:

```bash
bash scripts/test-dembly-install.sh
bash scripts/test-release.sh
bash scripts/test-documentation.sh
mise exec -- cargo fmt --check
mise exec -- cargo clippy --locked --all-targets --all-features -- -D warnings
mise exec -- cargo test --locked
scripts/check-linux.sh
mise exec -- cargo build --locked --release --target x86_64-unknown-linux-musl -p dembly-cli
target/x86_64-unknown-linux-musl/release/dembly --version
bash scripts/release.sh --dry-run
git diff --check
```

Expected: every command succeeds, the final binary outputs only the root Cargo version, and dry-run creates no tags, remotes, releases, or changes in the original worktree.

- [ ] **Step 5: Commit the Japanese guide**

```bash
git add README-ja.md scripts/test-documentation.sh
git commit -m "docs: add Japanese public guide"
```

Use a Japanese body stating that English/Japanese documentation now contain matching install, operation, security, and license facts.
