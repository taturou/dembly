#!/usr/bin/env bash

# Read-only prerequisite checker for the Linux-only Dembly PoC.
set -u

failed=0

ok() {
  printf '[OK] %s\n' "$1"
}

fail() {
  printf '[FAIL] %s\n' "$1" >&2
  failed=1
}

require_command() {
  local command_name="$1"
  local label="$2"

  if command -v "$command_name" >/dev/null 2>&1; then
    ok "$label"
  else
    fail "$label not found"
  fi
}

if [[ "$(uname -s)" == 'Linux' ]]; then
  ok 'Linux host'
else
  fail "Linux host required, found $(uname -s)"
fi

if [[ "$(uname -m)" == 'x86_64' ]]; then
  ok 'x86_64 architecture'
else
  fail "x86_64 architecture required, found $(uname -m)"
fi

require_command git git
require_command mise mise
require_command docker 'Docker CLI'
require_command mount mount
require_command mksquashfs mksquashfs

if command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
  ok 'Docker daemon'
else
  fail 'Docker daemon is unavailable or inaccessible'
fi

if command -v docker >/dev/null 2>&1 && docker compose version >/dev/null 2>&1; then
  ok 'Docker Compose v2'
else
  fail 'Docker Compose v2 is unavailable'
fi

if grep -qw squashfs /proc/filesystems 2>/dev/null; then
  ok 'kernel SquashFS support'
else
  fail 'kernel SquashFS support is unavailable'
fi

if [[ -e /dev/loop-control || -b /dev/loop0 ]]; then
  ok 'loop-device support'
else
  fail 'loop-device support is unavailable'
fi

if command -v mise >/dev/null 2>&1 \
  && mise exec -- rustup target list --installed 2>/dev/null | grep -qx 'x86_64-unknown-linux-musl'; then
  ok 'x86_64-unknown-linux-musl target'
else
  fail 'x86_64-unknown-linux-musl target is not installed through mise'
fi

if [[ "$failed" -ne 0 ]]; then
  exit 1
fi
