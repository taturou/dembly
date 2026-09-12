# Compose Base example

This Deck selects the `dev` service in `compose.yaml` as the Dembly Runtime. The same Compose project also defines `database`; `dembly up` starts both services in the Deck-root project. Dembly writes a generated override for the selected `dev` service under its Runtime metadata and leaves the user-authored `compose.yaml` unchanged.

Run the example from this directory after installing Dembly:

```sh
docker compose pull
dembly validate
dembly lock
dembly up
dembly exec -- /bin/echo compose-runtime
# Optional interactive shell:
dembly exec -- /bin/sh
dembly down
```

`up` and `down` affect all services in this Deck-root Compose project only, not unrelated host containers.
