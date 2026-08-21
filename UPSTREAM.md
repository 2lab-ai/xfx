# Upstream

xfx is an unofficial Rust port of [`vercel-labs/fx`](https://github.com/vercel-labs/fx).

| | |
|---|---|
| Upstream | `vercel-labs/fx` |
| Pinned commit | `580a0c5da9386317251968c09c1cee69e763487a` |
| Upstream license | Apache-2.0, Copyright 2025 Vercel, Inc. |
| xfx license | Apache-2.0, Copyright 2026 2lab.ai |
| Relationship | Independent reimplementation. No Zig source is copied. |

xfx is not affiliated with or endorsed by Vercel. Every behavioral claim below
cites a file and line at the pinned commit. When a claim here disagrees with the
code, the code wins and this file is wrong.

## Why the name is `xfx`

The executable is `xfx`, the profile home is `~/.xfx`, and the project file is
`.xfx.json`. Upstream uses `fx`, `~/.fx`, and `.fx.json`
(`src/core/config/config_runtime.zig:341`). The names are deliberately distinct
so that installing xfx cannot shadow the upstream binary, cannot read or corrupt
an upstream profile, and cannot be mistaken for the official product. Environment
overrides follow the same rule: `XFX_MODEL`, `XFX_PERMISSION_MODE`, and
`XFX_MAX_AGENT_STEPS` rather than the upstream `FX_` prefix.

Credential variables are the exception. `VERCEL_OIDC_TOKEN` and
`AI_GATEWAY_API_KEY` name a Vercel service, not the fx product, so renaming them
would break the integration rather than disambiguate it.

## Behavior taken from upstream

| xfx behavior | Upstream evidence |
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
| A bare invocation starts the interactive shell | `src/core/cli/cli_surface.zig:443` |
| Refusing to open a shell without a terminal, exit 1 | `src/core/app/app_entry_runtime.zig:224` |
| The shell's `/clear` clears the transcript rather than the conversation | `src/core/app/app_input_runtime.zig:2718` |
| `/new` starts a fresh session; `/model` takes an id; `/version`; `/quit` | `src/builtins/commands.zig:415-457` |
| An unrecognized slash command is one refusal that points at `/help` | `src/core/app/app_commands.zig:1754-1761` |
| Snapshots print the credential source label, never the secret | `tests/e2e/cli.test.ts:625-640`, `:698-716` |
| 12-character abbreviated build revision | `tests/e2e/cli.test.ts:729` |

## Deliberate deviations

Each of these is a decision, not an omission.

1. **Default agent step limit is bounded.** Upstream compiles in `0`, meaning
   unbounded (`src/core/config/agent_steps.zig:3`). xfx compiles in `25`.
   Configuring `0` still selects unbounded, so the configured semantics match;
   only the shipped default differs. An unbounded default contradicts the
   bounded-turn guarantee xfx's design makes.
2. **`sandbox` is always `none`.** Upstream reports `os` on macOS
   (`tests/e2e/cli.test.ts:572`). xfx does not confine commands in v0.1, so
   reporting a sandbox it does not have would be the most dangerous possible
   lie. `ask`/`auto` are a policy boundary, not confinement.
3. **No `update_channel` field.** Upstream reports one
   (`src/core/output/output_contracts.zig:497-499`). xfx has no updater, and an
   update channel with nothing to deliver implies a command that does not exist.
   `build_channel` is retained, and it says where a binary came from rather than
   what will be delivered to it: the compile profile, `debug` or `release`,
   unless the build declared `preview` -- the one channel that is not a profile,
   because a published prerelease compiles with the release profile and must not
   read as a tagged release. A declared channel that is neither is rejected at
   compile time rather than stamped (`src/build_meta.rs`).
4. **The missing-credential help names only environment variables.** Upstream
   points at `fx login` and `fx setup` (`tests/e2e/cli.test.ts:39-40`). xfx
   defers both, so naming them would advertise absent commands.
5. **`auth_refreshable` is always `false`.** xfx has no refreshable credential
   source, because OAuth login is deferred.
6. **The shell is line-oriented, not a TUI.** Upstream's interactive product is
   a full terminal application with a composer, a status line, a slash-command
   menu, and five classes of owner that may take the alternate screen
   (`AGENTS.md:265-278`, `src/ui/shell_runtime.zig`). xfx's shell appends lines
   to the terminal it was given: it uses the kernel's own canonical mode, never
   enters raw mode, never takes the alternate screen, and leaves the line
   discipline byte-identical. The cost is real and is recorded as the deferred
   `prompt history` row -- no recall, no arrow-key editing, no completion menu.
   The benefit is that scrollback survives, output is pipeable in the parts that
   are meant to be, and "xfx left your terminal as it found it" is a property
   with nothing to restore rather than a cleanup path that has to be right on
   every exit.
7. **The shell owns six slash commands, not forty.** Upstream registers about
   forty (`src/builtins/commands.zig:414-457`). xfx answers `/help`, `/new`,
   `/clear`, `/model`, `/version`, and `/quit`, and refuses everything else by
   name. `/exit` is not aliased to `/quit`, because a seventh accepted spelling
   is a seventh promise.
8. **Settings surface is a small subset.** Upstream's `Settings` carries ~35
   keys (`src/core/config/config_runtime.zig:68-129`). xfx implements `model`,
   `permission_mode`, and `max_agent_steps`, which are the keys its runtime
   actually consumes. An unread key is not configuration; it is decoration.
9. **The default permission mode asks for less.** Upstream's `auto` runs
   "routine understood development actions" directly and gives an unresolved
   sensitive action one bounded automatic review (`README.md:96-98`). xfx's
   `auto` admits a reporting-only command grammar that cannot compile or run
   project code, has no automatic review, and never widens itself. Narrower is
   the right direction to be wrong in for a port with no sandbox.

## Scope

`docs/parity.md` is the authoritative row-by-row account of what is implemented,
partial, and deferred. `scripts/check-no-stubs.sh` reconciles it against the
source in both directions -- an advertised surface with no `implemented` row and
an `implemented` row with no surface both fail the build -- and
`tests/parity.rs` runs the same reconciliation against the running binary.

xfx does not claim parity with `fx` and will not encode closeness to it in a
version number.
