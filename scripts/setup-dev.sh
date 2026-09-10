#!/usr/bin/env bash

# Install project-managed tools. This script never invokes sudo.
set -euo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
repo_root=$(cd -- "$script_dir/.." && pwd -P)

if ! command -v mise >/dev/null 2>&1; then
  printf 'mise is required before running setup-dev.sh\n' >&2
  exit 1
fi

cd -- "$repo_root"
mise install
mise exec -- rustup target add x86_64-unknown-linux-musl
mise exec -- rustup component add rustfmt clippy
"$repo_root/scripts/check-linux.sh"
