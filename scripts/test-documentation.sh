#!/usr/bin/env bash
set -euo pipefail

repository_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
readme="$repository_root/README.md"
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

require_in_order() {
    local previous=0
    local heading
    for heading in "$@"; do
        local line
        line=$(grep -nFx -- "$heading" "$readme" | head -n 1 | cut -d: -f1)
        if [[ -z "$line" || "$line" -le "$previous" ]]; then
            printf 'README headings are missing or out of order at: %s\n' "$heading" >&2
            exit 1
        fi
        previous=$line
    done
}

require "$readme" 'https://github.com/taturou/dembly/releases/latest/download/dembly-install.sh'
require "$readme" 'https://github.com/taturou/dembly/releases/download/v1.2.3/dembly-install.sh'
require "$readme" 'README-ja.md'
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

require_in_order \
    '## Language' \
    '## Overview' \
    '## Security warning' \
    '## Installation' \
    '## Compose Quickstart' \
    '## Architecture' \
    '## Core concepts' \
    '## Base types' \
    '## Cards' \
    '## Deck configuration' \
    '## CLI reference' \
    '## Development' \
    '## Limitations and evaluation' \
    '## License'

reject "$readme" 'performance measurement'
reject "$readme" '^#{1,6}[[:space:]]*(benchmark|evaluation procedure)'
reject "$readme" 'implementation in progress'
reject "$readme" '\bMIT\b'
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
