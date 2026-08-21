# Parity ledger

fxr is an unofficial behavioral port of
[`vercel-labs/fx`](https://github.com/vercel-labs/fx), pinned to
`580a0c5da9386317251968c09c1cee69e763487a`. This file is the product's truth
about what it can actually do.

Read it this way: **a surface that is not `implemented` here is absent from the
binary.** It is not a hidden flag, a silent no-op, or a stub that returns
success. Deferred rows exist so this document can be honest about the gap
between fxr and upstream, not so the gap can be advertised as a feature.

## Status values

| Value | Meaning |
|---|---|
| `implemented` | Complete for the documented contract, with a green acceptance test. |
| `partial` | Present and useful, but narrower than upstream. The row states the limit. |
| `deferred` | Absent from the binary. Not in help, not in a tool schema, not a stub. |

`scripts/check-no-stubs.sh` fails the build if a runtime surface is missing an
`implemented` row here, or if a `deferred` row is advertised by the parser.

## Commands

Upstream's command union is `src/core/cli/cli_surface.zig:58-84`.

| Surface | Kind | Status | Notes and upstream evidence |
|---|---|---|---|
| `status` | command | implemented | `[--json]`. Model, credential source, permission mode, sandbox, workspace, history turns, step limit. `cli_surface.zig:69`, `output_contracts.zig:489-540`. |
| `doctor` | command | implemented | `[--json]`. Aggregate counts plus `{name,status,detail}` checks. `cli_surface.zig:73`, `output_contracts.zig:1209-1285`. |
| `help` | command | implemented | `help`, `--help`, `-h`. Lists only implemented commands. `cli_surface.zig:60`. |
| `ask` | command | implemented | `[--json] [--no-save] [--add-dir <PATH>]... <prompt>`. A bounded multi-step Gateway turn: ordered assistant text, read-only tool calls executed locally, then exactly one terminal event. `--no-save` and `--add-dir` have their own rows under Configuration and persistence. The permission-mode (`--auto`/`--yolo`) and resume flags are not advertised and arrive with the mutation and session slices. `cli_surface.zig:61`. |
| `interactive` | command | deferred | Planned for the shell slice of v0.1; a bare `fxr` is rejected until then. `cli_surface.zig:59`. |
| `session` | command | deferred | Planned for the durability slice of v0.1. `cli_surface.zig:76`. |
| `sessions` | command | deferred | Planned for the durability slice of v0.1. `cli_surface.zig:77`. |
| `resume` | command | deferred | Upstream `resume_session`. Planned with sessions. `cli_surface.zig:78`. |
| `acp` | command | deferred | Agent Client Protocol server. Post-v0.1. `cli_surface.zig:62`. |
| `pr` | command | deferred | GitHub pull-request workflow. Post-v0.1. `cli_surface.zig:63`. |
| `issue` | command | deferred | GitHub issue workflow. Post-v0.1. `cli_surface.zig:64`. |
| `login` | command | deferred | Vercel OAuth. fxr reads environment credentials only. `cli_surface.zig:65`. |
| `logout` | command | deferred | Requires stored credentials, which fxr does not keep. `cli_surface.zig:66`. |
| `setup` | command | deferred | Interactive credential onboarding. `cli_surface.zig:67`. |
| `permissions` | command | deferred | Permission rule management UI. `cli_surface.zig:70`. |
| `models` | command | deferred | Model catalog. `cli_surface.zig:71`. |
| `provider` | command | deferred | Provider switching, including Codex. `cli_surface.zig:72`. |
| `background` | command | deferred | Background and durable work. `cli_surface.zig:74`. |
| `teams` | command | deferred | Vercel team selection; needs `login`. `cli_surface.zig:75`. |
| `credits` | command | deferred | Billing surface; needs `login`. `cli_surface.zig:79`. |
| `usage` | command | deferred | Usage reporting; needs `login`. `cli_surface.zig:80`. |
| `upgrade` | command | deferred | Self-updater and release channels. `cli_surface.zig:81`. |
| `replay` | command | deferred | Golden terminal replay; needs the full-screen UI. `cli_surface.zig:82`. |
| `workspace` | command | deferred | Additional-root management. `cli_surface.zig:83`. |

## Tools

Upstream's registry is the 26 entries in `src/builtins/tools.zig:1351-1378`.
fxr advertises the four read-only tools below, in that order, and nothing else.
The registry is a compile-time constant; `scripts/check-no-stubs.sh` reconciles
it against the `tool` rows here.

| Surface | Kind | Status | Notes and upstream evidence |
|---|---|---|---|
| `list_files` | tool | implemented | One directory level, sorted, ignored names omitted, capped at 100 entries with an explicit `... and more entries` line. `tools.zig:509-532`, `list_files.zig:80-115`. |
| `glob_files` | tool | implemented | `pattern` plus optional `path` and `mode=matches\|count`. Sorted before it is capped at 100; skips ignored and gitignored directories; does not follow symlinks. `tools.zig:534-562`, `glob_files.zig:88-245`. |
| `grep_files` | tool | implemented | Literal substring search with `path`, `include`, `case_insensitive`, `mode=matches\|files_with_matches\|count`, `head_limit`, `offset`, and `context_lines` (bounded at 5). Regular expressions are not supported, matching upstream. `tools.zig:564-597`, `grep_files.zig:161-530`. |
| `read_file` | tool | implemented | Line-numbered UTF-8 output with `start_line`/`line_count`, 400-line default, 2000-byte line clip, 256 KiB output cap, and an explicit sentinel stating how many of the file's lines were shown. Binary files are named, not dumped. `tools.zig:599-627`, `read_file.zig:119-372`. |
| tool permission modes | tool group | partial | Every advertised tool is read-only, and reads are admitted in every permission mode. The `ask`/`auto`/`yolo` distinction becomes observable with the mutation slice, which brings the approval channel. `permission_gate.zig`. |
| mutation group (`write_file`, `edit_file`, `create_folder`) | tool group | deferred | Planned for the mutation slice of v0.1. `tools.zig:1356-1361`. |
| `terminal` (exec action) | tool group | deferred | Planned for the mutation slice of v0.1; durable actions stay out. `tools.zig:1367`. |
| file management (`delete_file`, `rename_file`, `copy_file`, `file_info`, `open_file`) | tool group | deferred | Post-v0.1. `tools.zig:1358-1364`. |
| `memory` | tool group | deferred | Post-v0.1. `tools.zig:1362`. |
| `semantic_search` | tool group | deferred | Needs an embedding index. Post-v0.1. `tools.zig:1363`. |
| web (`web_fetch`, `web_search`) | tool group | deferred | Post-v0.1; network egress from the agent is out of scope for v0.1. `tools.zig:1365-1366`. |
| skills (`skill`, `install_skill`) | tool group | deferred | Post-v0.1. `tools.zig:1368-1369`. |
| `subagent` | tool group | deferred | Post-v0.1. `tools.zig:1370`. |
| MCP (`mcp_search_tools`, `mcp_select_tool`, `mcp_features`) | tool group | deferred | Post-v0.1. `tools.zig:1371-1373`. |
| `ask_user_question` | tool group | deferred | Post-v0.1. `tools.zig:1374`. |
| `vision` | tool group | deferred | Needs image input. Post-v0.1. `tools.zig:1375`. |
| `read_tool_result` | tool group | deferred | Needs bounded tool-result storage. Post-v0.1. `tools.zig:1376`. |

## Providers

| Surface | Kind | Status | Notes and upstream evidence |
|---|---|---|---|
| `Vercel AI Gateway` | provider | implemented | Streaming completions over rustls. Request is `prompt`/`tools`/`toolChoice`; the response is a bounded SSE decode that requires a canonical `finish`. An HTTP endpoint override is accepted only for loopback. `src/builtins/gateway.zig:41`, `:759-765`, `src/core/gateway/gateway_json.zig:333-363`, `src/gateway/client.zig:2718-3272`. |
| prompt caching and provider options | provider | deferred | Upstream sends `providerOptions`, `reasoning`, and Anthropic cache breakpoints. `src/core/gateway/gateway_json.zig:330-378`. |
| generation usage and billing reconciliation | provider | deferred | Upstream reads `providerMetadata.gateway` cost and generation ids. `src/gateway/client.zig:2496-2560`. |
| transport-owned retry and team routing | provider | deferred | fxr's turn owns attempts and sends no team header. `src/gateway/client.zig:1459-1494`, `:1810-1825`. |
| `VERCEL_OIDC_TOKEN` credential | provider | implemented | Resolved when nonblank; highest precedence. Reported by source name only. |
| `AI_GATEWAY_API_KEY` credential | provider | implemented | Resolved when nonblank; second precedence. Reported by source name only. |
| `fx login` credential | provider | deferred | OAuth credential store. `src/core/auth/auth_runtime.zig:685-700`. |
| stored API key credential | provider | deferred | Keychain and profile-stored keys. `src/core/shared/types.zig:90-96`. |
| Codex / ChatGPT subscription | provider | deferred | Second provider family. `src/core/shared/types.zig:90-96`. |

## Configuration and persistence

| Surface | Kind | Status | Notes and upstream evidence |
|---|---|---|---|
| project settings `.fxr.json` | persistence | implemented | Upstream `.fx.json`. Profile-only keys are ignored with a diagnostic. `config_runtime.zig:341-379`, `:548-576`. |
| profile settings `~/.fxr/settings.json` | persistence | implemented | Upstream `~/.fx/settings.json`. `config_runtime.zig:381-403`. |
| exact-workspace settings entry | persistence | implemented | `workspaces["<root>"]`; exact match only. `config_runtime.zig:405-443`. |
| environment overrides | persistence | implemented | `FXR_MODEL`, `FXR_PERMISSION_MODE`, `FXR_MAX_AGENT_STEPS`. Blank values are ignored. `config_runtime.zig:445-453`. |
| config diagnostics | persistence | implemented | Non-fatal; surfaced as `doctor` `config` checks. `config_runtime.zig:578-593`. |
| `ask --no-save` | persistence | partial | Advertised and honored: no turn run with it is written to a session. It is **not yet distinguishable from the default**, because this release persists no turn either way, so the flag currently constrains nothing the default does not already do. Its help says so. It becomes load-bearing when the session event log below lands; its meaning does not change then. |
| session event log | persistence | deferred | `~/.fxr/sessions/<id>/events.jsonl`; lands with the durability slice of v0.1. Until it does, `ask` records nothing with or without `--no-save`. |
| session manifest and index | persistence | deferred | Lands with the durability slice of v0.1. |
| permission rules and grants | persistence | deferred | Lands with the mutation slice of v0.1. |
| `AGENTS.md` project context | persistence | deferred | Lands with the durability slice of v0.1. |
| `ask --add-dir <PATH>` | persistence | implemented | Repeatable. Authorizes one extra directory for this turn's read tools; a path that is not a usable directory fails the turn before any request is sent. Upstream spells the same authority `--add-dir`. `cli_surface.zig:391-415`, `workspace_access.zig:53-96`. |
| saved additional workspace roots | persistence | deferred | Upstream persists added directories and manages them from the `workspace` command. fxr authorizes them per invocation only, so nothing is remembered between runs. `cli_surface.zig:83`. |
| prompt history | persistence | deferred | Post-v0.1. |

## Output and UI

| Surface | Kind | Status | Notes and upstream evidence |
|---|---|---|---|
| status/doctor text renderer | ui | implemented | One `[surface] key=value` line per fact. `output_contracts.zig:410-446`, `:1209-1236`. |
| status/doctor JSON renderer | ui | implemented | Exactly one newline-terminated document. `output_contracts.zig:489-540`, `:1240-1285`. |
| JSONL turn event stream | ui | implemented | `assistant_delta`, `tool_start`, `tool_result`, `final`, and `error` are all produced by `ask --json`, with exactly one terminal event per turn. |
| streamed assistant text | ui | implemented | `ask` without `--json` writes only the answer to stdout, one delta at a time, and puts a failure on stderr. `orchestrator.zig:4650-4655`. |
| interactive shell | ui | deferred | Line-oriented shell with six slash commands; lands with the shell slice of v0.1. |
| full-screen TUI | ui | deferred | Not a goal. The shell keeps normal scrollback. |
| notifications and status line | ui | deferred | Post-v0.1. `config_runtime.zig:139-152`. |
| colored and hyperlinked TTY output | ui | deferred | Post-v0.1; output is plain text. |

## Embedding surfaces

| Surface | Kind | Status | Notes and upstream evidence |
|---|---|---|---|
| CLI binary | embedding | implemented | The only supported entry point. |
| WASM core | embedding | deferred | `src/wasm_core_main.zig`, `src/wasm_term_main.zig`. |
| N-API module | embedding | deferred | `src/napi_core_main.zig`, `sdk/NAPI.md`. |
| Rust library crate | embedding | partial | The crate is public so tests can drive it, but its API is not stable and it is not published. |
