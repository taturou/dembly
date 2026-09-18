#!/usr/bin/env bash
set -euo pipefail

repository_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
readme="$repository_root/README.md"
japanese_readme="$repository_root/README-ja.md"
compose_readme="$repository_root/examples/compose-base/README.md"
compose_file="$repository_root/examples/compose-base/compose.yaml"
config_file="$repository_root/examples/compose-base/.dembly/config.toml"
devcontainer_file="$repository_root/examples/compose-base/.devcontainer/devcontainer.json"
initialize_host="$repository_root/examples/compose-base/.devcontainer/initialize-host.sh"

require() {
    local file=$1
    local pattern=$2
    if ! grep -Fq -- "$pattern" "$file"; then
        printf 'missing required documentation in %s: %s\n' "$file" "$pattern" >&2
        exit 1
    fi
}

require_exact_line() {
    local file=$1
    local line=$2
    if ! grep -Fxq -- "$line" "$file"; then
        printf 'missing exact documentation line in %s: %s\n' "$file" "$line" >&2
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
        '## Configuration'
        '## Cards'
        '## CLI reference'
        '## Development'
        '## Release'
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

artifact_layout_rows() {
    local file=$1
    local heading=$2

    awk -v heading="$heading" '
        $0 == heading { in_layout = 1; next }
        in_layout && /^## / { exit }
        in_layout && /^\| Base( \+| \|)/ { print }
    ' "$file"
}

require_matching_artifact_layout_rows() {
    local -a expected=(
        '| Base | `base-image` | `base-image` |'
        '| Base + ATfEP | `base-image-with-atfep` | `base-image` + `cards/atfep/rootfs.squashfs` |'
        '| Base + Clang | `base-image-with-clang` | `base-image` + `cards/clang/rootfs.squashfs` |'
        '| Base + ATfEP + TIS | `base-image-with-atfep-and-tis` | `base-image` + `cards/atfep/rootfs.squashfs` + `cards/tis/rootfs.squashfs` |'
    )
    local -a english_rows japanese_rows
    mapfile -t english_rows < <(artifact_layout_rows "$readme" '## Limitations and evaluation')
    mapfile -t japanese_rows < <(artifact_layout_rows "$japanese_readme" '## 制限と評価')

    if [[ ${#english_rows[@]} -ne ${#expected[@]} ]] || [[ ${#japanese_rows[@]} -ne ${#expected[@]} ]]; then
        printf 'README artifact-layout row count must be %d in both languages\n' "${#expected[@]}" >&2
        exit 1
    fi

    local index
    for index in "${!expected[@]}"; do
        if [[ "${english_rows[$index]}" != "${expected[$index]}" ]] || \
            [[ "${japanese_rows[$index]}" != "${expected[$index]}" ]]; then
            printf 'README artifact-layout row %d differs between English and Japanese documentation\n' \
                "$((index + 1))" >&2
            exit 1
        fi
    done
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
require "$readme" 'dembly init'
require "$readme" 'dembly validate'
require "$readme" 'dembly lock'
require "$readme" 'dembly apply'
require "$readme" 'docker compose -f compose.yaml up -d'
require "$readme" 'docker compose -f compose.yaml ps'
require "$readme" 'docker compose -f compose.yaml logs dev'
require "$readme" 'docker compose -f compose.yaml exec --user root dev'
require "$readme" 'docker compose -f compose.yaml run --rm dev'
require "$readme" '/run/dembly/bin/dembly __runtime check /run/dembly/runtime/dev.toml'
require "$readme" 'docker compose -f compose.yaml down'
require "$readme" 'dembly unapply'
require "$readme" 'dembly --version'
require "$readme" '(LICENSE)'
require "$readme" 'Runtime mounts Card SquashFS'
require "$readme" 'Host does not mount Card SquashFS'
require "$readme" '<replace-me>'
require "$readme" 'Base + ATfEP'
require "$readme" 'Base + Clang'
require "$readme" 'Base + ATfEP + TIS'

require "$japanese_readme" 'English documentation: [README.md](README.md)'
require_exact_line "$japanese_readme" '[English](README.md) | 日本語'
require "$japanese_readme" 'https://github.com/taturou/dembly/releases/latest/download/dembly-install.sh'
require "$japanese_readme" 'https://github.com/taturou/dembly/releases/download/v1.2.3/dembly-install.sh'
require "$japanese_readme" 'dembly init'
require "$japanese_readme" 'dembly validate'
require "$japanese_readme" 'dembly lock'
require "$japanese_readme" 'dembly apply'
require "$japanese_readme" 'docker compose -f compose.yaml up -d'
require "$japanese_readme" 'docker compose -f compose.yaml ps'
require "$japanese_readme" 'docker compose -f compose.yaml logs dev'
require "$japanese_readme" 'docker compose -f compose.yaml exec --user root dev'
require "$japanese_readme" 'docker compose -f compose.yaml run --rm dev'
require "$japanese_readme" '/run/dembly/bin/dembly __runtime check /run/dembly/runtime/dev.toml'
require "$japanese_readme" 'docker compose -f compose.yaml down'
require "$japanese_readme" 'dembly unapply'
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
    reject "$public_readme" 'performance evaluation|performance measurement|benchmark'
    reject "$public_readme" '性能測定|ベンチマーク|性能評価'
    reject "$public_readme" '\bMIT\b'
done

reject "$readme" 'implementation in progress'
reject "$compose_readme" 'implementation in progress'

require "$compose_readme" 'dembly validate'
require "$compose_readme" 'dembly lock'
require "$compose_readme" 'dembly apply'
require "$compose_readme" 'docker compose -f compose.yaml up -d'
require "$compose_readme" 'docker compose -f compose.yaml ps'
require "$compose_readme" 'docker compose -f compose.yaml logs dev'
require "$compose_readme" 'docker compose -f compose.yaml exec --user root dev'
require "$compose_readme" 'docker compose -f compose.yaml run --rm dev'
require "$compose_readme" '/run/dembly/bin/dembly __runtime check /run/dembly/runtime/dev.toml'
require "$compose_readme" 'docker compose -f compose.yaml down'
require "$compose_readme" 'dembly unapply'
require "$compose_readme" 'Do not use `-p` or `COMPOSE_PROJECT_NAME`'

require "$compose_file" 'name: dembly-compose-example'
require "$compose_file" 'x-dembly:'
require "$config_file" '[compose]'
require "$config_file" 'path = "../compose.yaml"'
require "$devcontainer_file" '"dockerComposeFile": ["../compose.yaml"]'
require "$devcontainer_file" '"initializeCommand": ".devcontainer/initialize-host.sh"'
require "$initialize_host" 'dembly validate'
require "$initialize_host" 'dembly apply'
require "$initialize_host" 'dembly lock'

for public_doc in "$readme" "$japanese_readme" "$compose_readme"; do
    reject "$public_doc" 'deck\.toml|deck\.lock|dembly (up|down|run|exec)([^[:alnum:]_]|$)|Image Base|image-base|compose\.override'
done

if [[ -e "$repository_root/examples/image-base" ]]; then
    printf 'obsolete examples/image-base directory still exists\n' >&2
    exit 1
fi

printf 'documentation assertions passed\n'
