# Upstream

fxr is an unofficial Rust port of [`vercel-labs/fx`](https://github.com/vercel-labs/fx).

| | |
|---|---|
| Upstream | `vercel-labs/fx` |
| Pinned commit | `580a0c5da9386317251968c09c1cee69e763487a` |
| Upstream license | Apache-2.0, Copyright 2025 Vercel, Inc. |
| fxr license | Apache-2.0, Copyright 2026 2lab.ai |
| Relationship | Independent reimplementation. No Zig source is copied. |

fxr is not affiliated with or endorsed by Vercel. Every behavioral claim below
cites a file and line at the pinned commit. When a claim here disagrees with the
code, the code wins and this file is wrong.

## Why the name is `fxr`

The executable is `fxr`, the profile home is `~/.fxr`, and the project file is
`.fxr.json`. Upstream uses `fx`, `~/.fx`, and `.fx.json`
(`src/core/config/config_runtime.zig:341`). The names are deliberately distinct
so that installing fxr cannot shadow the upstream binary, cannot read or corrupt
an upstream profile, and cannot be mistaken for the official product. Environment
overrides follow the same rule: `FXR_MODEL`, `FXR_PERMISSION_MODE`, and
`FXR_MAX_AGENT_STEPS` rather than the upstream `FX_` prefix.

Credential variables are the exception. `VERCEL_OIDC_TOKEN` and
`AI_GATEWAY_API_KEY` name a Vercel service, not the fx product, so renaming them
would break the integration rather than disambiguate it.

## Behavior taken from upstream

| fxr behavior | Upstream evidence |
|---|---|
| Command grammar shape and the `unknown` outcome | `src/core/cli/cli_surface.zig:58-84`, `:441-517` |
| Exit code 1 for a usage rejection, diagnostics on stderr | `tests/e2e/cli.test.ts:404-412` |
| `--version` / `-v` print the bare version with no program name | `tests/e2e/cli.test.ts:441-452` |
| Settings merge order: project, profile global, exact workspace, environment | `src/core/config/config_runtime.zig:341-455` |
| A later layer overrides only the keys it sets | `src/core/config/config_runtime.zig:1532-1545` |
| Profile-only keys in the project file are ignored with a diagnostic | `src/core/config/config_runtime.zig:548-593` |
| Exact-workspace settings under `workspaces["<root>"]` | `src/core/config/config_runtime.zig:405-443` |
| A blank environment override does not displace a configured value | `src/core/config/config_runtime.zig:449-453` |
| 64 KiB settings size ceiling | `src/core/config/config_runtime.zig:17` |
| Default permission mode `auto` | `src/core/config/config_runtime.zig:18`, `tests/e2e/cli.test.ts:786-806` |
| Default model `zai/glm-5.2` | `src/builtins/gateway.zig:40` |
| `0` means an unbounded agent step limit | `src/core/config/agent_steps.zig:3-31` |
| Credential precedence and source labels | `tests/e2e/cli.test.ts:686-717`, `src/core/shared/types.zig:90-96` |
| `status` fields and JSON shape | `src/core/output/output_contracts.zig:410-446`, `:489-540` |
| `doctor` aggregate counts and `{name,status,detail}` checks | `src/core/output/output_contracts.zig:1209-1285` |
| `doctor` check names `workspace`, `config`, `auth`, `startup` | `src/core/cli/doctor_runtime.zig:90`, `:156-179`, `:207-214`, `:237` |
| `startup` detail format `resolved model=..., permission_mode=..., agent_step_limit=N` | `src/core/cli/doctor_runtime.zig:231-237`, `tests/e2e/cli.test.ts:808-848` |
| `config` warning text when no settings file exists | `src/core/cli/doctor_runtime.zig:174` |
| `status` and `doctor` succeed without credentials and do not mutate an empty home | `tests/e2e/cli.test.ts:551-586` |
| Snapshots print the credential source label, never the secret | `tests/e2e/cli.test.ts:625-640`, `:698-716` |
| 12-character abbreviated build revision | `tests/e2e/cli.test.ts:729` |

## Deliberate deviations

Each of these is a decision, not an omission.

1. **Default agent step limit is bounded.** Upstream compiles in `0`, meaning
   unbounded (`src/core/config/agent_steps.zig:3`). fxr compiles in `25`.
   Configuring `0` still selects unbounded, so the configured semantics match;
   only the shipped default differs. An unbounded default contradicts the
   bounded-turn guarantee fxr's design makes.
2. **`sandbox` is always `none`.** Upstream reports `os` on macOS
   (`tests/e2e/cli.test.ts:572`). fxr does not confine commands in v0.1, so
   reporting a sandbox it does not have would be the most dangerous possible
   lie. `ask`/`auto` are a policy boundary, not confinement.
3. **No `update_channel` field.** Upstream reports one
   (`src/core/output/output_contracts.zig:497-499`). fxr has no updater, and a
   release channel with nothing to release implies a command that does not
   exist. `build_channel` is retained but means the compile profile.
4. **The missing-credential help names only environment variables.** Upstream
   points at `fx login` and `fx setup` (`tests/e2e/cli.test.ts:39-40`). fxr
   defers both, so naming them would advertise absent commands.
5. **`auth_refreshable` is always `false`.** fxr has no refreshable credential
   source, because OAuth login is deferred.
6. **A bare `fxr` is rejected.** Upstream starts the interactive shell
   (`src/core/cli/cli_surface.zig:443`). fxr's shell is a later slice, so a bare
   invocation exits 1 with usage rather than succeeding at nothing.
7. **Settings surface is a small subset.** Upstream's `Settings` carries ~35
   keys (`src/core/config/config_runtime.zig:68-129`). fxr implements `model`,
   `permission_mode`, and `max_agent_steps`, which are the keys its runtime
   actually consumes. An unread key is not configuration; it is decoration.

## Scope

`docs/parity.md` is the authoritative row-by-row account of what is implemented,
partial, and deferred. `scripts/check-no-stubs.sh` fails the build if the binary
advertises a surface that ledger does not record as implemented.
