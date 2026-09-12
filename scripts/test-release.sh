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
      fmt|clippy|test) : ;;
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
case "$*" in
  'auth status') exit 0 ;;
  'release view '*) exit 1 ;;
  *) echo "unexpected gh invocation: $*" >&2; exit 64 ;;
esac
EOF
chmod +x "$fake_bin/mise" "$fake_bin/gh"

run_release() {
  (
    cd "$fixture"
    PATH="$fake_bin:$PATH" bash scripts/release.sh "$@"
  )
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
run_release --clean 1.2.3
[[ ! -e "$archive" && ! -e "$checksum" && ! -e "$build_info" ]] || fail 'clean retained a target artifact'
[[ -e "$fixture/dist/dembly-v1.2.30-$target.tar.gz" ]] || fail 'clean removed a non-target artifact'

original_manifest=$(<"$fixture/Cargo.toml")
run_release --dry-run 1.2.3
[[ $(<"$fixture/Cargo.toml") == "$original_manifest" ]] || fail 'dry-run changed caller Cargo.toml'
[[ ! -e "$fixture/.git/refs/tags/v1.2.3" ]] || fail 'dry-run created a tag'

head_sha=$(git -C "$fixture" rev-parse HEAD)
mkdir -p "$fixture/dist"
printf 'version=1.2.3\ntarget=%s\ncommit=%s\nsha256=not-the-archive\n' "$target" "$head_sha" >"$build_info"
if (
  cd "$fixture"
  PATH="$fake_bin:$PATH" RELEASE_DRY_WORKTREE=1 bash scripts/release.sh 1.2.3
); then
  fail 'mismatching build-info was overwritten'
fi

echo 'test-release: PASS'
