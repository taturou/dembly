#!/usr/bin/env bash
set -euo pipefail

project_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
release_source="$project_root/scripts/release.sh"
temporary_directory=$(mktemp -d)

cleanup() {
  rm -rf -- "$temporary_directory"
}
trap cleanup EXIT

fail() {
  echo "test-release: $*" >&2
  exit 1
}

[[ -f "$release_source" ]] || fail "release script is missing: $release_source"

fixture="$temporary_directory/fixture"
remote="$temporary_directory/origin.git"
fake_bin="$temporary_directory/bin"
git_log="$temporary_directory/git.log"
gh_log="$temporary_directory/gh.log"
mkdir -p "$fixture/scripts" "$fake_bin"
git init --bare --initial-branch=main "$remote" >/dev/null
git init --initial-branch=main "$fixture" >/dev/null
git -C "$fixture" config user.name 'Release test'
git -C "$fixture" config user.email 'release-test@example.invalid'
git -C "$fixture" remote add origin "$remote"

cp "$release_source" "$fixture/scripts/release.sh"
chmod +x "$fixture/scripts/release.sh"
cp "$project_root/scripts/dembly-install.sh.in" "$fixture/scripts/dembly-install.sh.in"
cp "$project_root/LICENSE" "$fixture/LICENSE"
cat >"$fixture/Cargo.toml" <<'EOF'
[workspace]
members = []
resolver = "2"

[workspace.package]
version = "0.1.0"
edition = "2021"
EOF
: >"$fixture/Cargo.lock"
cat >"$fixture/scripts/check-linux.sh" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
chmod +x "$fixture/scripts/check-linux.sh"
git -C "$fixture" add .
git -C "$fixture" commit -m 'test fixture' >/dev/null
git -C "$fixture" push -u origin main >/dev/null

real_git=$(command -v git)
cat >"$fake_bin/git" <<'EOF'
#!/usr/bin/env bash
printf '%s\\n' "$*" >>"$RELEASE_TEST_GIT_LOG"
exec "$RELEASE_TEST_REAL_GIT" "$@"
EOF
cat >"$fake_bin/mise" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
[[ $1 == exec && $2 == -- ]] || exit 64
shift 2
case $1 in
  rustup)
    if [[ $2 == target ]]; then
      printf '%s\n' x86_64-unknown-linux-musl
    else
      printf '%s\n' clippy rustfmt
    fi
    ;;
  cargo)
    shift
    case $1 in
      generate-lockfile) : >Cargo.lock ;;
      fmt)
        [[ ${RELEASE_TEST_FAIL_QUALITY_GATE:-0} != 1 ]] || exit 1
        ;;
      clippy|test) : ;;
      build)
        target=''
        while (($#)); do
          if [[ $1 == --target ]]; then target=$2; shift 2; else shift; fi
        done
        mkdir -p "target/$target/release"
        version=$(awk '/^version = / { gsub(/"/, "", $3); print $3; exit }' Cargo.toml)
        printf '#!/usr/bin/env bash\nprintf "%%s\\n" "%s"\n' "$version" >"target/$target/release/dembly"
        chmod +x "target/$target/release/dembly"
        ;;
      *) exit 64 ;;
    esac
    ;;
  *) exit 64 ;;
esac
EOF
cat >"$fake_bin/gh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"$RELEASE_TEST_GH_LOG"
case "$*" in
  'auth status') exit 0 ;;
  'release view '*) exit 1 ;;
  'release create '*) exit 0 ;;
  *) echo "unexpected gh invocation: $*" >&2; exit 64 ;;
esac
EOF
chmod +x "$fake_bin/git" "$fake_bin/mise" "$fake_bin/gh"

run_release() {
  (
    cd "$fixture"
    PATH="$fake_bin:$PATH" RELEASE_TEST_FAIL_QUALITY_GATE="${RELEASE_TEST_FAIL_QUALITY_GATE:-0}" RELEASE_TEST_REAL_GIT="$real_git" RELEASE_TEST_GIT_LOG="$git_log" RELEASE_TEST_GH_LOG="$gh_log" bash scripts/release.sh "$@"
  )
}

worktree_count() {
  git -C "$fixture" worktree list --porcelain | grep -c '^worktree ' || true
}

if run_release 1.2; then
  fail 'invalid SemVer was accepted'
fi

mkdir -p "$fixture/dist"
target='x86_64-unknown-linux-musl'
archive="$fixture/dist/dembly-v1.2.3-$target.tar.gz"
checksum="$archive.sha256"
build_info="$fixture/dist/dembly-v1.2.3-$target.build-info"
touch "$archive" "$checksum" "$build_info" "$fixture/dist/dembly-v1.2.30-$target.tar.gz"
cwd_dist="$temporary_directory/dist"
mkdir -p "$cwd_dist"
touch "$cwd_dist/dembly-v1.2.3-$target.tar.gz"
(
  cd "$temporary_directory"
  PATH="$fake_bin:$PATH" RELEASE_TEST_REAL_GIT="$real_git" RELEASE_TEST_GIT_LOG="$git_log" RELEASE_TEST_GH_LOG="$gh_log" bash "$fixture/scripts/release.sh" --clean 1.2.3
)
[[ ! -e "$archive" && ! -e "$checksum" && ! -e "$build_info" ]] || fail 'clean retained a target artifact'
[[ -e "$fixture/dist/dembly-v1.2.30-$target.tar.gz" ]] || fail 'clean removed a non-target artifact'
[[ -e "$cwd_dist/dembly-v1.2.3-$target.tar.gz" ]] || fail 'clean removed a cwd-relative artifact'

original_manifest=$(<"$fixture/Cargo.toml")
: >"$git_log"
: >"$gh_log"
run_release --dry-run 1.2.3
[[ $(<"$fixture/Cargo.toml") == "$original_manifest" ]] || fail 'dry-run changed caller Cargo.toml'
[[ ! -e "$fixture/.git/refs/tags/v1.2.3" ]] || fail 'dry-run created a tag'
! grep -Eq '^(ls-remote|push|.* tag )' "$git_log" || fail 'dry-run inspected or published a remote tag'
[[ ! -s "$gh_log" ]] || fail 'dry-run called GitHub CLI'

# Break caught: a failed dry-run quality gate leaves a detached worktree registered.
dry_worktree_count=$(worktree_count)
export RELEASE_TEST_FAIL_QUALITY_GATE=1
if run_release --dry-run; then
  fail 'dry-run accepted a failed quality gate'
fi
unset RELEASE_TEST_FAIL_QUALITY_GATE
[[ $(worktree_count) == "$dry_worktree_count" ]] || fail 'failed dry-run retained a worktree registration'

mkdir -p "$fixture/dist"
printf 'version=1.2.3\ntarget=%s\ncommit=wrong\nsha256=not-the-archive\n' "$target" >"$build_info"
git -C "$fixture" add dist
git -C "$fixture" commit -m 'add mismatched artifact' >/dev/null
git -C "$fixture" push >/dev/null
if run_release --dry-run 1.2.3; then
  fail 'mismatching build-info was overwritten'
fi

printf '# dirty\n' >>"$fixture/Cargo.toml"
if run_release; then
  fail 'normal release accepted a dirty checkout'
fi
git -C "$fixture" checkout -- Cargo.toml

printf 'untracked\n' > "$fixture/untracked-release-input"
if run_release; then
  fail 'normal release accepted an untracked nonignored file'
fi
rm -- "$fixture/untracked-release-input"

printf '/dist/\n/target/\n' > "$fixture/.gitignore"
git -C "$fixture" rm -r --cached dist >/dev/null
git -C "$fixture" add .gitignore
git -C "$fixture" commit -m 'ignore release artifacts' >/dev/null
git -C "$fixture" push >/dev/null
mkdir -p "$fixture/dist" "$fixture/target"
printf 'ignored\n' > "$fixture/dist/ignored-artifact"
printf 'ignored\n' > "$fixture/target/ignored-artifact"
: >"$gh_log"
run_release
grep -q '^release create v0.1.0 ' "$gh_log" || fail 'normal release rejected ignored dist or target artifacts'

: >"$gh_log"
run_release 1.2.3+build-id
stable_release=$(grep '^release create v1.2.3+build-id ' "$gh_log") || fail 'build metadata release was not created'
[[ $stable_release != *--prerelease* ]] || fail 'build metadata release was classified as prerelease'

: >"$gh_log"
run_release 1.2.3-rc.1+build-id
grep -q '^release create v1.2.3-rc.1+build-id ' "$gh_log" || fail 'prerelease release was not created'
grep -q -- '--prerelease' "$gh_log" || fail 'prerelease metadata was not classified as prerelease'

git -C "$fixture" checkout -b review-non-main >/dev/null
if run_release; then
  fail 'normal release accepted a non-main branch'
fi
git -C "$fixture" checkout main >/dev/null

git -C "$fixture" commit --allow-empty -m 'ahead of origin' >/dev/null
if run_release; then
  fail 'normal release accepted a HEAD that differs from origin/main'
fi

if bypass_output=$( (
  cd "$fixture"
  PATH="$fake_bin:$PATH" RELEASE_DRY_WORKTREE=1 RELEASE_TEST_REAL_GIT="$real_git" RELEASE_TEST_GIT_LOG="$git_log" RELEASE_TEST_GH_LOG="$gh_log" bash scripts/release.sh
) 2>&1 ); then
  fail 'external dry-run environment variable bypassed normal preflight'
fi
[[ $bypass_output == *'normal release requires HEAD to equal origin/main'* ]] || fail 'external dry-run environment variable did not reach normal preflight'
git -C "$fixture" reset --hard origin/main >/dev/null

echo 'test-release: PASS'
