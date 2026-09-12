#!/usr/bin/env bash
set -euo pipefail

repository_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
readme="$repository_root/README.md"
japanese_readme="$repository_root/README-ja.md"
compose_readme="$repository_root/examples/compose-base/README.md"
image_readme="$repository_root/examples/image-base/README.md"

require() {
    local file=$1
    local pattern=$2
    if ! grep -Fq -- "$pattern" "$file"; then
        printf 'missing required documentation in %s: %s\n' "$file" "$pattern" >&2
        exit 1
    fi
}

reject() {
    local file=$1
    local pattern=$2
    if grep -Eiq -- "$pattern" "$file"; then
        printf 'stale documentation in %s matches: %s\n' "$file" "$pattern" >&2
        exit 1
    fi
}

require_exact_h2_sequence() {
    local -a expected=(
        '## Language'
        '## Overview'
        '## Security warning'
        '## Installation'
        '## Compose Quickstart'
        '## Architecture'
        '## Core concepts'
        '## Base types'
        '## Cards'
        '## Deck configuration'
        '## CLI reference'
        '## Development'
        '## Limitations and evaluation'
        '## License'
    )
    local -a actual=()
    mapfile -t actual < <(grep -E '^## ' "$readme")

    if [[ ${#actual[@]} -ne ${#expected[@]} ]]; then
        printf 'README H2 count differs: expected %d, found %d\n' "${#expected[@]}" "${#actual[@]}" >&2
        exit 1
    fi

    local index
    for index in "${!expected[@]}"; do
        if [[ "${actual[$index]}" != "${expected[$index]}" ]]; then
            printf 'README H2 differs at position %d: expected %s, found %s\n' \
                "$((index + 1))" "${expected[$index]}" "${actual[$index]}" >&2
            exit 1
        fi
    done
}

require_matching_artifact_layout_rows() {
    if ! diff -u \
        <(grep -E '^\| Base( \+| \|)' "$readme" | tail -n +2) \
        <(grep -E '^\| Base( \+| \|)' "$japanese_readme" | tail -n +2); then
        printf 'README artifact-layout rows differ between English and Japanese documentation\n' >&2
        exit 1
    fi
}

reject_multiple_japanese_sentences_per_line() {
    if ! awk '
        /^```/ { in_code = !in_code; next }
        !in_code {
            line = $0
            if (gsub(/。/, "", line) > 1) {
                print FNR ": " $0
                found = 1
            }
        }
        END { exit found }
    ' "$japanese_readme"; then
        printf 'README-ja.md contains multiple Japanese sentences on one line\n' >&2
        exit 1
    fi
}

require "$readme" 'https://github.com/taturou/dembly/releases/latest/download/dembly-install.sh'
require "$readme" 'https://github.com/taturou/dembly/releases/download/v1.2.3/dembly-install.sh'
require "$readme" 'English | [日本語](README-ja.md)'
require "$readme" '```mermaid'
require "$readme" 'docker compose pull'
require "$readme" 'dembly validate'
require "$readme" 'dembly lock'
require "$readme" 'dembly up'
require "$readme" 'dembly exec -- /bin/echo compose-runtime'
require "$readme" 'dembly down'
require "$readme" 'dembly --version'
require "$readme" '(LICENSE)'
require "$readme" 'Runtime mounts Card SquashFS'
require "$readme" 'Host does not mount Card SquashFS'
require "$readme" '<replace-me>'
require "$readme" 'Base + ATfEP'
require "$readme" 'Base + Clang'
require "$readme" 'Base + ATfEP + TIS'

require "$japanese_readme" 'English documentation: [README.md](README.md)'
require "$japanese_readme" 'https://github.com/taturou/dembly/releases/latest/download/dembly-install.sh'
require "$japanese_readme" 'https://github.com/taturou/dembly/releases/download/v1.2.3/dembly-install.sh'
require "$japanese_readme" 'docker compose pull'
require "$japanese_readme" 'dembly validate'
require "$japanese_readme" 'dembly lock'
require "$japanese_readme" 'dembly up'
require "$japanese_readme" 'dembly exec -- /bin/echo compose-runtime'
require "$japanese_readme" 'dembly down'
require "$japanese_readme" 'dembly --version'
require "$japanese_readme" '(LICENSE)'
require "$japanese_readme" '| Base |'
require "$japanese_readme" '| Base + ATfEP |'
require "$japanese_readme" '| Base + Clang |'
require "$japanese_readme" '| Base + ATfEP + TIS |'

require_exact_h2_sequence
require_matching_artifact_layout_rows
reject_multiple_japanese_sentences_per_line

cli_reference=$(awk '
    /^## CLI reference$/ { in_cli_reference = 1; next }
    in_cli_reference && /^## / { exit }
    in_cli_reference { print }
' "$readme")
if grep -Eiq 'dembly exec.*lock|lock.*dembly exec' <<<"$cli_reference"; then
    printf 'CLI reference incorrectly claims that dembly exec validates deck.lock\n' >&2
    exit 1
fi

for public_readme in "$readme" "$japanese_readme"; do
    reject "$public_readme" 'performance measurement'
    reject "$public_readme" '^#{1,6}[[:space:]]*(benchmark|evaluation procedure)'
    reject "$public_readme" '\bMIT\b'
done

reject "$readme" 'implementation in progress'
reject "$compose_readme" 'implementation in progress'

require "$compose_readme" 'docker compose pull'
require "$compose_readme" 'dembly validate'
require "$compose_readme" 'dembly lock'
require "$compose_readme" 'dembly up'
require "$compose_readme" 'dembly exec -- /bin/echo compose-runtime'
require "$compose_readme" 'dembly down'

require "$image_readme" 'docker build -t dembly-fixture-base:local ../../tests/fixtures/image-base'
require "$image_readme" 'dembly card build ../../tests/fixtures/hello-card/rootfs ./cards'
require "$image_readme" '--name hello --version 1 --mount-target /opt/dembly/cards/hello'
require "$image_readme" '--path-prepend bin --non-interactive'
require "$image_readme" 'dembly validate'
require "$image_readme" 'dembly lock'
require "$image_readme" 'dembly run -- /bin/true'

printf 'documentation assertions passed\n'
