#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
test_root=$(mktemp -d)
trap 'rm -rf -- "$test_root"' EXIT

test_home="$test_root/home"
test_data="$test_root/data"
test_bin="$test_root/bin"
fixture_root="$test_root/fixture"
archive_root="$fixture_root/dembly-v1.2.3-x86_64-unknown-linux-musl"
archive="$fixture_root/dembly-v1.2.3-x86_64-unknown-linux-musl.tar.gz"
checksum="$fixture_root/dembly-v1.2.3-x86_64-unknown-linux-musl.tar.gz.sha256"
installer="$test_root/dembly-install.sh"

mkdir -p "$archive_root" "$test_bin" "$test_home/.local/bin" "$test_data"
printf '#!/usr/bin/env bash\nprintf "1.2.3\\n"\n' > "$archive_root/dembly"
chmod +x "$archive_root/dembly"
printf 'fixture license\n' > "$archive_root/LICENSE"
tar -C "$fixture_root" -czf "$archive" dembly-v1.2.3-x86_64-unknown-linux-musl
(cd "$fixture_root" && sha256sum "$(basename "$archive")") > "$checksum"

cat > "$test_bin/curl" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

destination=''
while (($#)); do
  case "$1" in
    -o)
      destination=$2
      shift 2
      ;;
    *)
      shift
      ;;
  esac
done

case "$destination" in
  *.sha256) cp "$FAKE_CHECKSUM" "$destination" ;;
  *) cp "$FAKE_ARCHIVE" "$destination" ;;
esac
EOF
chmod +x "$test_bin/curl"

sed \
  -e 's|@VERSION@|1.2.3|g' \
  -e 's|@REPOSITORY@|example/dembly|g' \
  "$repo_root/scripts/dembly-install.sh.in" > "$installer"
chmod +x "$installer"

run_installer() {
  HOME="$test_home" \
    XDG_DATA_HOME="$test_data" \
    PATH="$test_bin:$PATH" \
    FAKE_ARCHIVE="$archive" \
    FAKE_CHECKSUM="$checksum" \
    "$installer" "$@"
}

# Break caught: an install fails to verify, extract, and activate the release.
run_installer
test -x "$test_data/dembly/releases/1.2.3/dembly"
test -f "$test_data/dembly/releases/1.2.3/LICENSE"
test "$(readlink "$test_home/.local/bin/dembly")" = "$test_data/dembly/releases/1.2.3/dembly"
test "$("$test_home/.local/bin/dembly" --version)" = '1.2.3'

# Break caught: a stale managed symlink is not replaced on a repeat install.
ln -sfnT "$test_root/stale-dembly" "$test_home/.local/bin/dembly"
run_installer
test "$(readlink "$test_home/.local/bin/dembly")" = "$test_data/dembly/releases/1.2.3/dembly"

# Break caught: --list does not report the installed version and its active state.
test "$(run_installer --list)" = '1.2.3 (active)'

# Break caught: --uninstall can delete the binary targeted by the active link.
if run_installer --uninstall 1.2.3; then
  echo 'expected active-version uninstall to fail' >&2
  exit 1
fi
test -x "$test_data/dembly/releases/1.2.3/dembly"

# Break caught: --uninstall accepts a non-SemVer prerelease identifier.
if run_installer --uninstall 1.2.3-01; then
  echo 'expected invalid SemVer uninstall to fail' >&2
  exit 1
fi

# Break caught: install overwrites a user-managed regular file at the active path.
run_installer --purge
printf 'user-managed binary\n' > "$test_home/.local/bin/dembly"
if run_installer; then
  echo 'expected regular active path install to fail' >&2
  exit 1
fi
test -f "$test_home/.local/bin/dembly"
test ! -L "$test_home/.local/bin/dembly"
rm -- "$test_home/.local/bin/dembly"

# Break caught: --purge removes paths outside its Dembly-derived scope.
run_installer
mkdir -p "$test_data/unrelated" "$test_home/unrelated"
printf 'keep\n' > "$test_data/unrelated/file"
printf 'keep\n' > "$test_home/unrelated/file"
run_installer --purge
test ! -e "$test_data/dembly/releases/1.2.3"
test ! -e "$test_data/licenses/dembly/LICENSE"
test ! -e "$test_home/.local/bin/dembly"
test -f "$test_data/unrelated/file"
test -f "$test_home/unrelated/file"

echo 'dembly installer tests passed'
