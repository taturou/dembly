# Task 2 Report: Safe installer template

## TDD evidence

### RED

Command:

```bash
bash scripts/test-dembly-install.sh
```

Result: exit code 2.

```text
sed: can't read /home/h-morishita/job/dembly/main/scripts/dembly-install.sh.in: No such file or directory
```

The test failed because the installer template did not exist, as required.

A second RED cycle was used for strict SemVer validation. After adding the
`1.2.3-01` uninstall case, the pre-fix test exited 1 and printed:

```text
expected invalid SemVer uninstall to fail
```

This demonstrated that numeric prerelease identifiers with leading zeroes
were incorrectly accepted.

### GREEN

Command:

```bash
bash -n scripts/dembly-install.sh.in
bash scripts/test-dembly-install.sh
git diff --check
```

Result: all commands exited 0. The installer test verified:

- checksum validation and local archive extraction through fake `curl`;
- executable and LICENSE installation;
- active symlink target and `dembly --version` output;
- repeat-install symlink replacement;
- `--list` active-version output;
- active-version uninstall refusal;
- invalid strict-SemVer uninstall refusal;
- regular-file active-path refusal; and
- `--purge` deletion limited to Dembly-derived paths while unrelated HOME and
  XDG-data files remain intact.

## Changed files

- `scripts/dembly-install.sh.in`
- `scripts/test-dembly-install.sh`
- `.superpowers/sdd/2026-09-12-release-distribution-and-documentation/task-2-report.md`

## Self-review

- The installer embeds the required `@VERSION@` and `@REPOSITORY@` markers.
- Normal installation downloads the exact versioned Linux musl archive and
  checksum, checks it with `sha256sum --check`, then checks the extracted
  binary version before it is moved to the derived release path.
- A valid existing release is reused without downloading; its symlink is still
  updated.
- Every persistent release deletion names either the embedded-version release
  path, an uninstall version validated as SemVer, the derived releases
  directory for purge, the derived shared LICENSE path, or the managed
  symlink. Temporary cleanup targets only the `mktemp -d` result.
- A non-symlink active path is rejected before an install or purge can modify
  it. Existing symlinks are replaced with `ln -sfnT`.
- The root `LICENSE` file was not modified; its approved trailing ASCII space
  is preserved.

## Concerns

- The checksum establishes archive integrity against the downloaded checksum
  asset, but does not authenticate the installer script itself. This is the
  explicitly intended trust boundary for this task.
- The test uses GNU/Linux utilities available on the supported Linux x86_64
  host; portability to non-GNU userlands is outside the stated target.
