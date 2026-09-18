#!/bin/sh
[ "${HELLO_ENV:-}" = "enabled" ] || exit 4
printf '%s\n' mounted > /tmp/dembly-hello-card-hook
id -u > /tmp/dembly-hook-uid
printf '%s\n' persisted >> /work/cache/result
