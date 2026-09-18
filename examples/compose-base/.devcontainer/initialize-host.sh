#!/bin/sh
set -eu

dembly validate

apply_errors=$(mktemp)
trap 'rm -f "$apply_errors"' EXIT HUP INT TERM

if ! dembly apply 2>"$apply_errors"; then
    cat "$apply_errors" >&2
    if grep -Fq 'run dembly lock' "$apply_errors"; then
        printf '%s\n' 'Dembly Lock is missing or stale. Run dembly lock, then retry opening the Dev Container.' >&2
    fi
    exit 1
fi

cat "$apply_errors" >&2
