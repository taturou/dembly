FROM alpine:3.21
RUN addgroup -g 10001 dembly \
    && adduser -D -u 10001 -G dembly -h /home/dembly dembly \
    && printf '%s\n' \
        '#!/bin/sh' \
        'uid="$(id -u)"' \
        'printf "%s\\n" "$uid" > /tmp/dembly-original-uid' \
        'printf "original-uid=%s\\n" "$uid" >&2' \
        'exec "$@"' \
        > /usr/local/bin/original-entrypoint \
    && chmod 0755 /usr/local/bin/original-entrypoint
USER dembly:dembly
ENTRYPOINT ["/usr/local/bin/original-entrypoint"]
CMD ["sleep", "infinity"]
