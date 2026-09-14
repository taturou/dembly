# Compose workflow migration

## Purpose

This document maps the previous Image/Compose orchestrator design to the Compose Base core design.

`design/spec.md` is the target implementation specification.

## Removed behavior

| Previous behavior | Target behavior | Reason |
|---|---|---|
| Image Base | Out of scope | One lifecycle model is required. |
| `dembly up/down/run/exec` | Native Docker Compose | Dembly compiles configuration and does not operate containers. |
| Ephemeral generated override | One managed Compose file | Standard Compose tooling must observe the actual Runtime. |
| `deck.toml` and `deck.lock` | `.dembly/config.toml` and `x-dembly.lock` | The Compose file carries the applied runtime state. |
| Host executable bind by original path | Copy to `.dembly/runtime/bin/dembly` | Apply output remains runnable after a Host binary upgrade. |

## Preserved behavior

- Card layout, checksum verification, mount collision detection, environment merge, exports, hooks, Volume and Host Bind semantics remain requirements.
- SquashFS is mounted only in the Runtime mount namespace.
- Runtime initialization starts as root and executes the original process as the resolved intended user.
- Card selection does not require a Base image rebuild.

## New applied state

`dembly apply` updates only `compose.path`.

`x-dembly` stores the lock, the original values of managed fields, and the last values applied by Dembly.

Apply and unapply reject a manually modified managed field.

`dembly unapply` restores the original values only after that check.

## Command migration

| Previous command | Target operation |
|---|---|
| `dembly up` | `dembly apply`; then `docker compose up -d` or Dev Containers rebuild |
| `dembly down` | `docker compose down` |
| `dembly run -- cmd` | `docker compose run <service> cmd` |
| `dembly exec -- cmd` | `docker compose exec --user <intended-user> <service> cmd` |
| `dembly lock` | Preserved; writes `x-dembly.lock` |
| `dembly check` | Preserved; requires the lock |

## Dev Containers

Dev Containers is optional.

When configured, `devcontainer.json` and `config.toml` must identify the same service.

The Dembly-managed Compose file is the last `dockerComposeFile` entry.

`overrideCommand` is false or absent.

`containerUser` is root or absent.

`remoteUser` equals the resolved intended user.
