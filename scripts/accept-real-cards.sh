#!/usr/bin/env bash

# Run a real Card acceptance suite without assigning any special role to a Card name.
set -euo pipefail

if [[ $# -lt 1 ]]; then
  printf 'usage: %s <deck.toml> [-- <command> [args...]]...\n' "$0" >&2
  exit 2
fi

deck_path=$1
shift
dembly_bin=${DEMBLY_BIN:-dembly}

"$dembly_bin" validate "$deck_path"
"$dembly_bin" lock "$deck_path"
"$dembly_bin" check "$deck_path"

while [[ $# -gt 0 ]]; do
  if [[ $1 != '--' ]]; then
    printf 'acceptance commands must start with --\n' >&2
    exit 2
  fi
  shift
  command=()
  while [[ $# -gt 0 && $1 != '--' ]]; do
    command+=("$1")
    shift
  done
  if [[ ${#command[@]} -eq 0 ]]; then
    printf 'acceptance command must not be empty\n' >&2
    exit 2
  fi
  "$dembly_bin" run "$deck_path" -- "${command[@]}"
done
