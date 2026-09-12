#!/usr/bin/env bash
set -euo pipefail

target='x86_64-unknown-linux-musl'
repository='taturou/dembly'
project_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
temporary_directory=''

die() {
  echo "release: $*" >&2
  exit 1
}

usage() {
  die 'usage: release.sh [X.Y.Z | --dry-run [X.Y.Z] | --clean X.Y.Z]'
}

is_semver() {
  local numeric_identifier='(0|[1-9][0-9]*)'
  local prerelease_identifier='(0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)'
  local build_identifier='[0-9A-Za-z-]+'
  [[ $1 =~ ^$numeric_identifier\.$numeric_identifier\.$numeric_identifier(-$prerelease_identifier(\.$prerelease_identifier)*)?(\+$build_identifier(\.$build_identifier)*)?$ ]]
}

workspace_version() {
  awk '
    /^\[workspace\.package\]$/ { in_workspace_package = 1; next }
    /^\[/ { in_workspace_package = 0 }
    in_workspace_package && $1 == "version" && $2 == "=" {
      value = $3
      gsub(/^"|"$/, "", value)
      print value
      exit
    }
  ' Cargo.toml
}

set_workspace_version() {
  local selected_version=$1
  local replacement_file=''
  replacement_file=$(mktemp)
  awk -v version="$selected_version" '
    /^\[workspace\.package\]$/ { in_workspace_package = 1 }
    /^\[/ && $0 != "[workspace.package]" { in_workspace_package = 0 }
    in_workspace_package && $1 == "version" && $2 == "=" {
      print "version = \"" version "\""
      changed = 1
      next
    }
    { print }
    END { if (!changed) exit 1 }
  ' Cargo.toml >"$replacement_file" || {
    rm -f -- "$replacement_file"
    die 'root Cargo.toml lacks [workspace.package].version'
  }
  mv "$replacement_file" Cargo.toml || {
    rm -f -- "$replacement_file"
    die 'failed to replace root Cargo.toml'
  }
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

require_tools() {
  local tool
  for tool in git mise tar gzip sha256sum sed awk mktemp; do
    require_command "$tool"
  done
}

verify_mise_environment() {
  mise exec -- rustup target list --installed | grep -qx "$target" || die "Rust target is not installed through mise: $target"
  mise exec -- rustup component list --installed | grep -Eq '^clippy(-|$)' || die 'Rust clippy component is not installed through mise'
  mise exec -- rustup component list --installed | grep -Eq '^rustfmt(-|$)' || die 'Rust rustfmt component is not installed through mise'
}

ensure_normal_preflight() {
  [[ $(git branch --show-current) == main ]] || die 'normal release requires the main branch'
  if ! git diff --quiet || ! git diff --cached --quiet; then
    die 'normal release requires a clean worktree'
  fi
  git rev-parse --verify --quiet refs/remotes/origin/main >/dev/null || die 'normal release requires origin/main'
  [[ $(git rev-parse HEAD) == $(git rev-parse origin/main) ]] || die 'normal release requires HEAD to equal origin/main'
  require_command gh
  gh auth status >/dev/null 2>&1 || die 'GitHub CLI is not authenticated'
}

ensure_only_version_files_changed() {
  local changed_paths
  changed_paths=$(git diff --name-only)
  [[ -n $changed_paths ]] || die 'version update did not change Cargo.toml or Cargo.lock'
  while IFS= read -r changed_path; do
    [[ $changed_path == Cargo.toml || $changed_path == Cargo.lock ]] || die "version update changed an unexpected tracked file: $changed_path"
  done <<<"$changed_paths"
}

run_quality_gates() {
  scripts/check-linux.sh
  # cargo fmt does not accept Cargo's --locked flag and does not resolve dependencies.
  mise exec -- cargo fmt --check
  mise exec -- cargo clippy --locked --all-targets --all-features -- -D warnings
  mise exec -- cargo test --locked
  mise exec -- cargo build --locked --release --target "$target" -p dembly-cli
}

artifact_paths() {
  local artifact_root=$1
  dist_directory="$artifact_root/dist"
  artifact_basename="dembly-v${version}-${target}"
  archive="$dist_directory/$artifact_basename.tar.gz"
  checksum="$archive.sha256"
  build_info="$dist_directory/$artifact_basename.build-info"
  installer="$dist_directory/dembly-install.sh"
}

artifact_matches_current_commit() {
  [[ -f $archive && -f $checksum && -f $build_info ]] || return 1
  local expected_info actual_checksum
  actual_checksum=$(sha256sum "$archive" | awk '{print $1}')
  expected_info=$(printf 'version=%s\ntarget=%s\ncommit=%s\nsha256=%s' "$version" "$target" "$commit_sha" "$actual_checksum")
  [[ $(<"$build_info") == "$expected_info" ]] || return 1
  (cd "$dist_directory" && sha256sum --check "$(basename "$checksum")") >/dev/null 2>&1
}

package_artifacts() {
  local artifact_root=$1
  artifact_paths "$artifact_root"
  if [[ -e $archive || -e $checksum || -e $build_info ]]; then
    artifact_matches_current_commit || die "existing artifacts do not match current commit; run: scripts/release.sh --clean $version"
    sed -e "s/@VERSION@/$version/g" -e "s|@REPOSITORY@|$repository|g" scripts/dembly-install.sh.in >"$installer"
    chmod +x "$installer"
    return
  fi

  local binary package_directory package_root archive_checksum
  binary="target/$target/release/dembly"
  [[ -x $binary ]] || die "release binary is missing: $binary"
  [[ $("$binary" --version) == "$version" ]] || die 'release binary version does not match selected version'
  mkdir -p "$dist_directory"
  temporary_directory=$(mktemp -d)
  package_directory="$temporary_directory/dembly-v${version}-${target}"
  mkdir -p "$package_directory"
  cp "$binary" "$package_directory/dembly"
  cp LICENSE "$package_directory/LICENSE"
  package_root=$(dirname "$package_directory")
  tar -C "$package_root" -czf "$archive" "$(basename "$package_directory")"
  archive_checksum=$(sha256sum "$archive" | awk '{print $1}')
  printf '%s  %s\n' "$archive_checksum" "$(basename "$archive")" >"$checksum"
  printf 'version=%s\ntarget=%s\ncommit=%s\nsha256=%s' "$version" "$target" "$commit_sha" "$archive_checksum" >"$build_info"
  sed -e "s/@VERSION@/$version/g" -e "s|@REPOSITORY@|$repository|g" scripts/dembly-install.sh.in >"$installer"
  chmod +x "$installer"
  rm -rf -- "$temporary_directory"
  temporary_directory=''
}

clean_artifacts() {
  local clean_version=$1
  local clean_basename="dembly-v${clean_version}-${target}"
  rm -f -- "$project_root/dist/${clean_basename}.tar.gz" "$project_root/dist/${clean_basename}.tar.gz.sha256" "$project_root/dist/${clean_basename}.build-info"
}

cleanup() {
  [[ -z $temporary_directory ]] || rm -rf -- "$temporary_directory"
}
trap cleanup EXIT

run_in_dry_worktree() {
  local dry_version=${1:-}
  temporary_directory=$(mktemp -d)
  local dry_worktree="$temporary_directory/worktree"
  git worktree add --detach "$dry_worktree" HEAD >/dev/null
  (
    cd "$dry_worktree"
    execute_release "$dry_worktree" 1 "$dry_version"
  )
  git worktree remove --force "$dry_worktree"
  temporary_directory=''
}

ensure_publication_is_unused() {
  local tag="v$version"
  git rev-parse --verify --quiet "refs/tags/$tag" >/dev/null && die "local tag already exists: $tag"
  git ls-remote --exit-code --tags origin "refs/tags/$tag" >/dev/null 2>&1 && die "remote tag already exists: $tag"
  gh release view "$tag" >/dev/null 2>&1 && die "GitHub Release already exists: $tag"
}

commit_version_change() {
  git add Cargo.toml Cargo.lock
  git commit -m "chore(release): prepare v$version" -m "リリース版のバージョンと lockfile を更新します。\n公開対象を v$version に固定します。"
  git push origin main
}

publish_release() {
  local tag="v$version"
  ensure_publication_is_unused
  git -c tag.gpgSign=false tag -a "$tag" -m "Dembly $tag"
  git push origin "$tag"
  build_release_create_arguments "$tag"
  if ! gh "${release_create_arguments[@]}"; then
    echo "release: tag $tag was pushed but GitHub Release creation failed; the tag was preserved." >&2
    echo "release: after resolving the failure, rerun: $(release_create_command)" >&2
    exit 1
  fi
}

build_release_create_arguments() {
  local tag=$1
  release_create_arguments=(release create "$tag" "$archive" "$checksum" "$installer" --title "Dembly $tag" --notes 'Dembly for Linux x86_64. Install this exact version with the version-pinned installer. See LICENSE for distribution terms.')
  [[ $version == *-* ]] && release_create_arguments+=(--prerelease)
}

release_create_command() {
  printf 'gh'
  printf ' %q' "${release_create_arguments[@]}"
  printf '\n'
}

execute_release() {
  local execution_root=$1
  local is_internal_dry_run=$2
  local selected_version_argument=$3
  cd "$execution_root"
  require_tools
  if [[ $is_internal_dry_run != 1 ]]; then
    ensure_normal_preflight
  fi

  current_version=$(workspace_version)
  is_semver "$current_version" || die "root Cargo.toml contains invalid SemVer: $current_version"
  version=${selected_version_argument:-$current_version}
  is_semver "$version" || die "invalid SemVer: $version"
  version_changed=0
  if [[ -n $selected_version_argument && $version != "$current_version" ]]; then
    if [[ $is_internal_dry_run != 1 ]]; then
      ensure_publication_is_unused
    fi
    set_workspace_version "$version"
    mise exec -- cargo generate-lockfile
    ensure_only_version_files_changed
    version_changed=1
  fi

  verify_mise_environment
  run_quality_gates
  if [[ $is_internal_dry_run != 1 && $version_changed == 1 ]]; then
    commit_version_change
  fi
  commit_sha=$(git rev-parse HEAD)
  package_artifacts "$execution_root"

  [[ $is_internal_dry_run == 1 ]] && return
  publish_release
}

mode=normal
version_argument=''
case $# in
  0) ;;
  1)
    case $1 in
      --dry-run) mode=dry-run ;;
      --clean) usage ;;
      *) version_argument=$1 ;;
    esac
    ;;
  2)
    case $1 in
      --dry-run) mode=dry-run; version_argument=$2 ;;
      --clean) mode=clean; version_argument=$2 ;;
      *) usage ;;
    esac
    ;;
  *) usage ;;
esac

if [[ $mode == clean ]]; then
  is_semver "$version_argument" || die "invalid SemVer: $version_argument"
  clean_artifacts "$version_argument"
  exit 0
fi

if [[ $mode == dry-run ]]; then
  [[ -z $version_argument ]] || is_semver "$version_argument" || die "invalid SemVer: $version_argument"
  run_in_dry_worktree "$version_argument"
  exit 0
fi

execute_release "$project_root" 0 "$version_argument"
