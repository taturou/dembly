# Compose Base example

This example selects `dev` in `compose.yaml` as the Dembly Runtime.
The same Compose project defines `database`, and native Docker Compose owns both services throughout their lifecycle.
Host Dembly updates only the selected service's managed fields and the top-level `x-dembly` state in `compose.yaml`.

The tracked `.dembly/config.toml` is the result of the one-time `dembly init` step.
When adding Dembly to another project, run `dembly init`, review the generated configuration, and commit it.

Run the example from this directory after installing Dembly:

```sh
docker compose -f compose.yaml pull
dembly validate
dembly lock
dembly apply

docker compose -f compose.yaml up -d
docker compose -f compose.yaml ps
docker compose -f compose.yaml logs dev
docker compose -f compose.yaml exec --user root dev /bin/echo compose-runtime
docker compose -f compose.yaml run --rm dev /bin/echo compose-run
dembly check
docker compose -f compose.yaml run --rm dev \
  /run/dembly/bin/dembly __runtime check /run/dembly/runtime/dev.toml
docker compose -f compose.yaml down

dembly unapply
```

Use the same ordered `-f` list for every Docker Compose command.
This example has one file, and `.devcontainer/devcontainer.json` names that same file.
Do not use `-p` or `COMPOSE_PROJECT_NAME` to replace the top-level `name`.

Run `dembly unapply` only after `docker compose down`.
Dembly does not detect or stop running containers during `unapply`.

The user-managed `.devcontainer/initialize-host.sh` runs `validate` and `apply`.
If `apply` reports a missing or stale Lock, the script asks the user to run `dembly lock` and retry; Dembly does not rewrite `initializeCommand` or the script.
