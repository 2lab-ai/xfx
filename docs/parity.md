# Parity ledger

xfx is an unofficial behavioral port of
[`vercel-labs/fx`](https://github.com/vercel-labs/fx), pinned to
`580a0c5da9386317251968c09c1cee69e763487a`. This file is the product's truth
about what it can actually do.

Read it this way: **a surface that is not `implemented` here is absent from the
binary.** It is not a hidden flag, a silent no-op, or a stub that returns
success. Deferred rows exist so this document can be honest about the gap
between xfx and upstream, not so the gap can be advertised as a feature.

## Status values

| Value | Meaning |
|---|---|
| `implemented` | Complete for the documented contract, with a green acceptance test. |
| `partial` | Present and useful, but narrower than upstream. The row states the limit. |
| `deferred` | Absent from the binary. Not in help, not in a tool schema, not a stub. |

`scripts/check-no-stubs.sh` reconciles this file against the binary in **both**
directions, for commands, tools, and shell slash commands:

- every surface the binary advertises has an `implemented` row here;
- every `implemented` row names a surface the binary really advertises, so
  "implemented" cannot be claimed for something that does not exist;
- no name from a `deferred` row -- including the names listed inside a grouped
  row, such as `delete_file` or `/resume` -- is advertised anywhere; and
- no surface name appears in two rows.

`tests/parity.rs` proves the same reconciliation against the *running* binary
rather than against the source text: the parser's own subcommand list, the tool
schemas as they are serialized into a Gateway request, and the rendered help
pages.

## Commands

Upstream's command union is `src/core/cli/cli_surface.zig:58-84`.

| Surface | Kind | Status | Notes and upstream evidence |
|---|---|---|---|
| `status` | command | implemented | `[--json]`. Model, credential source, permission mode, sandbox, workspace, history turns, step limit. `cli_surface.zig:69`, `output_contracts.zig:489-540`. |
| `doctor` | command | implemented | `[--json]`. Aggregate counts plus `{name,status,detail}` checks: `workspace`, `config`, `backend`, `auth`, `permissions`, `sessions`, `startup`. The `backend` check appears only when the configured backend cannot run -- an unreadable `backend` value, or `backend: llmux` with no usable `llmux_url` -- and **fails**, because every turn on such a machine refuses; it names `xfx setup llmux` or quotes the unreadable value. A backend that works is reported by the snapshot's `backend`/`backend_url` fields instead. The `auth` check asks each backend its own question: the Gateway fails without a bearer credential, llmux passes without one, because a loopback request is accepted keyless. No network I/O: `doctor` stays a command that is always safe to run, and never probes the daemon. The `sessions` check reports how many sessions are recorded, how many directories could not be trusted, and how many staged manifests an interrupted write left behind -- a report, never a repair. `cli_surface.zig:73`, `output_contracts.zig:1209-1285`. |
| `help` | command | implemented | `help`, `--help`, `-h`. Lists only implemented commands. `cli_surface.zig:60`. |
| `ask` | command | implemented | `[--auto\|--yolo] [--json] [--no-save] [--add-dir <PATH>]... [--resume <last\|ID>\|--resume-id <ID>] <prompt>`. A bounded multi-step Gateway turn: ordered assistant text, tool calls executed locally under a permission authority, then exactly one terminal event. Ctrl-C cancels the turn and kills any running command's process group; a second Ctrl-C exits 130. `--no-save`, `--add-dir`, the resume flags, and the permission modes have their own rows. `cli_surface.zig:61`. |
| `interactive` | command | implemented | What a bare `xfx` runs. A line-oriented append shell on the terminal's own canonical mode: it never enters raw mode or the alternate screen, so scrollback is preserved and there is no terminal state to restore. It refuses to start without a terminal on both stdin and stdout, and without a place to record the conversation. Each prompt is one ordinary turn -- same provider, registry, permission authority, and session store as `ask` -- and it owns exactly the six `slash` rows below. Ctrl-C stops a running turn and a second one exits 130; at the prompt it clears the line, and twice in a row leaves. It has no name to type: the parser reaches it by a bare invocation, which `src/cli.rs` declares as `ADVERTISED_ENTRYPOINTS`. `cli_surface.zig:59`, `app_entry_runtime.zig:224`. |
| `session` | command | implemented | `<last\|ID>\|--id <ID> [--json]`. Replays one session's log through its published boundary and cross-checks it against the manifest, then renders bounded turns. Read-only: it creates no profile state, and a session it cannot trust is a named refusal rather than a partial read. `cli_surface.zig:76`. |
| `sessions` | command | implemented | `[--json] [--all] [--limit N]`. Newest first with a total order, scoped to the current workspace unless `--all`, bounded at 20 by default and 200 at most. A session directory that cannot be trusted is counted in `skipped_invalid` rather than failing the listing. Read-only. `cli_surface.zig:77`. |
| `resume` | command | deferred | Upstream's standalone `resume_session` command. xfx resumes through `ask --resume`/`--resume-id`, so the bare name is not advertised. `cli_surface.zig:78`. |
| `acp` | command | deferred | Agent Client Protocol server. Post-v0.1. `cli_surface.zig:62`. |
| `pr` | command | deferred | GitHub pull-request workflow. Post-v0.1. `cli_surface.zig:63`. |
| `issue` | command | deferred | GitHub issue workflow. Post-v0.1. `cli_surface.zig:64`. |
| `login` | command | deferred | Vercel OAuth. xfx reads environment credentials only. `cli_surface.zig:65`. |
| `logout` | command | deferred | Requires stored credentials, which xfx does not keep. `cli_surface.zig:66`. |
| `setup` | command | implemented | `llmux [--url URL] [--json]`. Points xfx at a local llmux daemon: discovers it (explicit `--url`, else the url a previous setup recorded, else `http://127.0.0.1:3456`, else the `proxy.port` in llmux's own config -- never a scan, never off this machine), proves it is llmux (`GET /` answers exactly `llmux` **and** `GET /models` answers a non-empty catalog), keeps the profile file's own model when the catalog has it and otherwise takes the catalog's first entry -- the decision is about the layer being written, not the fully resolved value -- and merges `backend`/`llmux_url`/`model` into `~/.xfx/settings.json` through a staged `0600` file and a rename -- preserving every unrelated key, and refusing rather than replacing settings it could not parse. When a higher layer (`XFX_MODEL`, or an exact-workspace entry) will still outrank what was written, it says so on stderr and in `overridden_by`. It sends **no completion request**: the ping and the catalog are the whole receipt. It reads exactly one field of llmux's config, `proxy.port`, and never any credential. Narrower than upstream's row, which is interactive credential onboarding for the Gateway -- that remains absent. `cli_surface.zig:67`. |
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

## Interactive shell commands

Upstream's slash palette is the ~40 entries in `src/builtins/commands.zig:414-457`.
xfx's shell owns the six below and nothing else; `scripts/check-no-stubs.sh`
reconciles them against `SLASH_COMMANDS` in `src/interactive.rs`, in both
directions.

| Surface | Kind | Status | Notes and upstream evidence |
|---|---|---|---|
| `/help` | slash | implemented | Lists these six with one line each, then says that anything else is a prompt. `commands.zig:415`. |
| `/new` | slash | implemented | Ends the current session and starts a fresh identity on the next prompt. The old session's writer lock, its "always" grants, and its read proofs are all released with it, because each was sold as being about that session. `commands.zig:417`. |
| `/clear` | slash | implemented | Erases the screen and the scrollback (`ESC[2J`, `ESC[3J`) and reprints the header. The conversation is untouched: same session id, same durable history, and the next prompt still carries every earlier turn. It is a display command, not a memory command. `commands.zig:416`, `app_input_runtime.zig:2718`. |
| `/model` | slash | implemented | With no argument, reports the active model and which settings layer chose it. With one, uses that model from the next turn on and records a durable `preferences_changed` event, so a resumed session continues in the model it was actually held in. Bounded to one printable word: the id becomes an HTTP header. It does not browse a catalog -- `models` and `provider` are separate deferred rows -- and an id the Gateway rejects is reported by the Gateway. `commands.zig:452`. |
| `/version` | slash | implemented | The version, build channel, and build revision of the running binary. `commands.zig:456`. |
| `/quit` | slash | implemented | Leaves with status 0. `commands.zig:457`. |
| other upstream slash commands (`/reset`, `/resume`, `/continue`, `/rename`, `/login`, `/logout`, `/setup`, `/stats`, `/usage`, `/status`, `/background`, `/image`, `/images`, `/models`, `/provider`, `/permissions`, `/allowlist`, `/undo`, `/mcp`, `/exit`) | slash | deferred | Absent from `/help` and from the parser: an unrecognized slash line is one deterministic refusal that names the command and points at `/help`. `/exit` is deferred as well rather than aliased, so the shell has exactly six names. `commands.zig:414-457`. |

## Tools

Upstream's registry is the 26 entries in `src/builtins/tools.zig:1351-1378`.
xfx advertises the 8 tools below, in that order, and nothing else:
4 read-only (`list_files`, `glob_files`, `grep_files`, `read_file`),
3 mutating (`write_file`, `edit_file`, `create_folder`), and
1 command (`terminal`).

That split is the safety boundary, not a taxonomy. A read-only call is admitted
in every permission mode without asking anyone, because it changes nothing. Each
of the other four crosses a permission decision that mints a one-use authority
for one exact target and revalidates it immediately before spending it: `ask`
stops on every one of them, and `auto` admits only bounded reversible writes and
a reporting-only command grammar. **xfx can change files in your workspace and
start processes on your machine**, under no OS sandbox; those four rows are
where that begins.

The registry is a compile-time constant. `scripts/check-no-stubs.sh` reconciles
it against the `tool` rows below, and `tests/parity.rs` reconciles the counts
and the three groups named in this paragraph against the registry's real
`PermissionKind`s -- so the prose cannot drift from the code any more quietly
than a row can.

| Surface | Kind | Status | Notes and upstream evidence |
|---|---|---|---|
| `list_files` | tool | implemented | One directory level, sorted, ignored names omitted, capped at 100 entries with an explicit `... and more entries` line. `tools.zig:509-532`, `list_files.zig:80-115`. |
| `glob_files` | tool | implemented | `pattern` plus optional `path` and `mode=matches\|count`. Sorted before it is capped at 100; skips ignored and gitignored directories; does not follow symlinks. `tools.zig:534-562`, `glob_files.zig:88-245`. |
| `grep_files` | tool | implemented | Literal substring search with `path`, `include`, `case_insensitive`, `mode=matches\|files_with_matches\|count`, `head_limit`, `offset`, and `context_lines` (bounded at 5). Regular expressions are not supported, matching upstream. `tools.zig:564-597`, `grep_files.zig:161-530`. |
| `read_file` | tool | implemented | Line-numbered UTF-8 output with `start_line`/`line_count`, 400-line default, 2000-byte line clip, 256 KiB output cap, and an explicit sentinel stating how many of the file's lines were shown. Binary files are named, not dumped. `tools.zig:599-627`, `read_file.zig:119-372`. |
| `write_file` | tool | implemented | Creates a file, or replaces one that has been read in full and has not changed since. Same-directory staging, identity plus SHA-256 revalidation, atomic rename, preserved permission bits, parent `fsync`. An existing target is measured by `stat` before it is loaded and refused above the complete-read ceiling, so an enormous preimage is declined rather than allocated. `tools.zig:629-651`, `write_file.zig:1-237`, `file_mutation_contract.zig:566-617`. |
| `edit_file` | tool | implemented | Replaces exactly one occurrence of `old_string`. Zero or several occurrences are refused rather than guessed; an edit that changes nothing reports `No changes to <path>`. Same read proof, preimage bound, and replacement path as `write_file`. `tools.zig:653-680`, `edit_file.zig:1-275`. |
| `create_folder` | tool | implemented | Creates a directory and any missing parents. An existing directory is reported as already present rather than treated as an error. `tools.zig:707-729`, `create_folder.zig:1-352`. |
| `terminal` | tool | implemented | `exec` action only. A recognized read-only command runs as an exact argv with no shell; anything else needs an approval and then runs through the platform shell with the exact command, cwd, and environment that were fingerprinted. Operands must be relative, free of `..`, and must resolve inside an authorized root. Commands that compile or run project code are **not** on the automatic route. Bounded output, wall-clock timeout, SIGINT cancellation, process-group kill, and exit/signal reported as facts. Captured stdout and stderr are escaped for `&`, `<`, and `>` before they are placed inside xfx's own `<stdout>`/`<stderr>` frame, so a program cannot close the quotation and counterfeit an `<exit_code>` or a project-instructions tag. An admitted `git` runs with `-c core.fsmonitor=false`, and the `diff`, `log`, and `show` subcommands with `--no-ext-diff --no-textconv`, so a repository's own configured commands cannot execute on the automatic route. Durable actions are a separate deferred row. `tools.zig:85-95`, `terminal.zig:180-232`, `command_effect.zig:249-355`, `local_executor.zig:52-73`. |
| tool permission modes | tool group | implemented | `ask` requires a real TTY approval, discloses a bounded excerpt of the change, states what "always" would grant, and denies when there is no approval channel; `auto` admits bounded workspace writes and a reporting-only command grammar that cannot compile or run project code; `yolo` skips policy and prints a warning to stderr. Above all three, and before any of them: `write_file`, `edit_file`, and `create_folder` refuse a target whose path passes through `.git` or `.xfx`, because those directories configure what xfx and git are then allowed to execute. The names are compared without case, since macOS's default volume is case-insensitive and `.GIT` reaches the same directory there. The set is explicit rather than every dot directory, so `.github` stays writable. Decisions mint one-use authorities that are spent before they are revalidated. `permission_gate.zig:72-121`, `command_admission.zig:18-149`. |
| automatic command grammar | tool group | partial | Deliberately narrower than upstream's auto classifier. Reporting commands only: no `&&` chaining, no package-manager build/test families, and no Cargo subcommand outside the alias-proof built-ins (`--version`, `-V`, `--list`, `metadata --no-deps`) -- an `[alias]` in a `.cargo/config.toml` that automatic mode may itself have written can redirect any externally implemented subcommand, so `cargo fmt` and `cargo clippy` are refused regardless of their own behaviour. Existing path operands are canonicalized and must stay inside an authorized root. Widening any of it is a review decision. `command_effect.zig:16-104`. |
| durable terminal sessions (`start`, `read`, `write`, `wait`, `monitor`, `resize`, `signal`, `close`) | tool group | deferred | Post-v0.1. A session id is a reference the model holds across turns and outlives the authority that created it. No such action name appears in the advertised schema. `terminal.zig:186-232`. |
| OS command sandbox | tool group | deferred | Upstream confines commands with a platform backend. xfx does not, and reports `sandbox=none` in `status`. `auto` bounds what xfx agrees to start, not what a started process may do. `sandbox.zig`. |
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
| `llmux` | provider | implemented | Streaming completions from a local llmux daemon over the Anthropic Messages wire. Request is `model`/`max_tokens`/`stream`/`system`/`messages`/`tools`/`tool_choice`, with consecutive same-role messages merged and `tool_result` blocks leading the message that carries them; the response is a bounded SSE decode that requires a `message_delta` stop reason and refuses an unknown one. **Keyless**: a loopback request carries no `authorization` and no `x-api-key`, because that is what the daemon accepts as the tenant `local`, and xfx never reads or forwards an llmux key. Selected by the profile-only `backend` and `llmux_url` settings, which `xfx setup llmux` writes. An `error` frame inside an HTTP 200 fails the attempt. |
| prompt caching and provider options | provider | deferred | Upstream sends `providerOptions`, `reasoning`, and Anthropic cache breakpoints. `src/core/gateway/gateway_json.zig:330-378`. |
| generation usage and billing reconciliation | provider | deferred | Upstream reads `providerMetadata.gateway` cost and generation ids. `src/gateway/client.zig:2496-2560`. |
| transport-owned retry and team routing | provider | deferred | xfx's turn owns attempts and sends no team header. `src/gateway/client.zig:1459-1494`, `:1810-1825`. |
| `VERCEL_OIDC_TOKEN` credential | provider | implemented | Resolved when nonblank; highest precedence. Reported by source name only. |
| `AI_GATEWAY_API_KEY` credential | provider | implemented | Resolved when nonblank; second precedence. Reported by source name only. |
| `fx login` credential | provider | deferred | OAuth credential store. `src/core/auth/auth_runtime.zig:685-700`. |
| stored API key credential | provider | deferred | Keychain and profile-stored keys. `src/core/shared/types.zig:90-96`. |
| Codex / ChatGPT subscription | provider | deferred | Second provider family. `src/core/shared/types.zig:90-96`. |

## Configuration and persistence

| Surface | Kind | Status | Notes and upstream evidence |
|---|---|---|---|
| project settings `.xfx.json` | persistence | implemented | Upstream `.fx.json`. Profile-only keys are ignored with a diagnostic. `config_runtime.zig:341-379`, `:548-576`. |
| profile settings `~/.xfx/settings.json` | persistence | implemented | Upstream `~/.fx/settings.json`. `config_runtime.zig:381-403`. |
| exact-workspace settings entry | persistence | implemented | `workspaces["<root>"]`; exact match only. `config_runtime.zig:405-443`. |
| environment overrides | persistence | implemented | `XFX_MODEL`, `XFX_PERMISSION_MODE`, `XFX_MAX_AGENT_STEPS`. Blank values are ignored. `config_runtime.zig:445-453`. |
| backend selection `backend` + `llmux_url` | persistence | implemented | Profile-only, like `model` and `permission_mode`: a shared repository must not be able to choose which endpoint receives a prompt, so a `.xfx.json` that sets either is dropped with the ignored-key diagnostic. `llmux_url` is validated by the same transport rule as `XFX_GATEWAY_URL` (https anywhere, http only on loopback with an explicit port, never userinfo) and a refused value is a diagnostic rather than a string anything downstream may trust. An unreadable `backend` falls back to `gateway` with a diagnostic; neither is fatal, so `status` and `doctor` still describe a machine whose settings are broken. A `backend: llmux` with no usable url refuses the turn and names `xfx setup llmux` rather than falling back to the Gateway. No new environment variables. |
| config diagnostics | persistence | implemented | Non-fatal; surfaced as `doctor` `config` checks. `config_runtime.zig:578-593`. |
| `ask --no-save` | persistence | implemented | Load-bearing: the default records the turn, and this flag opens no store at all, so nothing is created under `~/.xfx` -- not a session directory, not a manifest. It conflicts with the resume flags, because continuing a conversation while refusing to record it would fork its history in silence. |
| session event log | persistence | implemented | `~/.xfx/sessions/<id>/events.jsonl`. One typed JSON frame per line carrying schema version, log generation, contiguous sequence, unique event id, and timestamp. Append + `fsync`, then an atomically replaced manifest publishes the exact byte boundary and its SHA-256. Readers stop at the boundary, so an unpublished, malformed, or truncated crash tail is invisible; a writable open truncates it. A sequence gap, a repeated event id, a digest mismatch, a boundary past EOF, or an unsafe session id all fail closed. Exactly one writer per session: an exclusive advisory `flock` is held for the handle's life and a second writer is refused, and every append re-checks the log's length so a change from outside xfx is refused rather than written over. Directories are `0700`, owned by the current user, with nothing granted to group or other, and are re-checked on every use -- by every read command as well as every write, and both directories rather than only `sessions` -- rather than only on creation; files are `0600` (Unix). xfx's own Gateway credential is never persisted: no variant of the event union carries it, and the credential source is reported by name only. **Model-read content is persisted**: a `tool_result` event stores a file's contents or a command's output verbatim, so a secret the model was asked to read is on disk as owner-only plaintext until the session is deleted, and `--no-save` is the only way to record nothing. `session_log.zig:2185-2195`, `session_replay.zig:158-207`, `session_event.zig:169-202`. |
| session manifest and index | persistence | implemented | `session.json` is a rebuildable projection: it summarizes the log and names the boundary it was computed from, and any disagreement between the two refuses the session rather than preferring either. The manifest is staged under a process-unique name and cleaned up by RAII, so xfx never unlinks a stage another writer owns. The listing is a projection over manifests, deterministic (newest first, ties by id) and bounded: the whole directory is sorted before the scan cap drops anything, the newest candidates survive it, and `truncated` says so, so `last` can never mean an arbitrary old session. `session_projection.zig:15-43`, `:248-280`. |
| session resume | persistence | implemented | `ask --resume <last\|ID>` and `--resume-id <ID>`. `last` is scoped to the current workspace and never crosses one; an exact id may be resumed from another workspace and writes an explicit durable `workspace_rebound` event before the turn, keeping the origin root intact. History and the recorded model preference are restored; the **permission mode is not**, because a `--yolo` turn recorded once must not become a default later. `session_store_types.zig:130-141`. |
| permission rules and grants | persistence | partial | Typed and enforced in memory for one run: exact `tool`+`target` allow/deny rules, and session grants recorded when a user answers "always". A command's key is its text **and** its working directory, so an approval in the workspace does not carry into a `--add-dir` root; a mutation's key is the target's **canonical absolute path**, so a grant recorded in one workspace does not authorize the same relative name after an exact-id resume rebinds the session to another one. The approval prose still names the file the way the user does. **An "always" answer persists across processes for the same durable session id**: the approval prompt says so and names the id, and `session` lists every standing grant by exact `tool` and `target` rather than as a count. Configured *rules* are still not read from or written to settings, and glob-shaped grants are absent. `permissions.zig`, `session_permission_state.zig`. |
| `AGENTS.md` project context | persistence | implemented | Bounded instruction files from the filesystem root down to the workspace, plus a nested directory's own file admitted immediately before a tool call touches a target inside it. Each section is labelled with its file and scope; the narrowest scope renders last. At most 32 files, 64 KiB each, 256 KiB in total -- counted in **model-visible bytes**, so a body is budgeted after escaping and a section after its framing, and a file whose escaped form is too large is omitted rather than clipped. Omission markers are bounded by the same file cap and name the reason. Containment is decided on canonical paths, so a symlinked directory inside the workspace cannot pull in an external file -- neither its bytes nor its provenance; a link that resolves inside the workspace is ordinary layout and is delivered once, under its real scope. Rule bodies are escaped so a repository cannot close xfx's framing and write its own. `CLAUDE.md` is never read directly and an `--add-dir` root contributes nothing. Context is rediscovered after a resume rather than persisted, so editing `AGENTS.md` takes effect on the next turn. `context.zig:284-436`, `:502-519`. |
| `ask --add-dir <PATH>` | persistence | implemented | Repeatable. Authorizes one extra directory for this turn's **read** tools; a path that is not a usable directory fails the turn before any request is sent. `auto` will not write into an added root, so reading it and changing it stay separate decisions. `cli_surface.zig:391-415`, `workspace_access.zig:53-96`. |
| saved additional workspace roots | persistence | deferred | Upstream persists added directories and manages them from the `workspace` command. xfx authorizes them per invocation only, so nothing is remembered between runs. `cli_surface.zig:83`. |
| prompt history | persistence | deferred | Post-v0.1. The shell reads lines through the terminal's canonical mode, which gives backspace, word erase, and line kill but no recall: there is no in-shell history buffer, no arrow-key recall, and nothing written to a history file. Adding one means owning the line editor, and owning the line editor means owning the terminal state the shell currently guarantees it never changes. |

## Output and UI

| Surface | Kind | Status | Notes and upstream evidence |
|---|---|---|---|
| status/doctor text renderer | ui | implemented | One `[surface] key=value` line per fact. `output_contracts.zig:410-446`, `:1209-1236`. `backend` (and `backend_url` when the backend has a configured one) follows `model` directly, because a model name means nothing without the endpoint it is asked of. |
| status/doctor JSON renderer | ui | implemented | Exactly one newline-terminated document. `output_contracts.zig:489-540`, `:1240-1285`. Carries the same `backend`/`backend_url` fields; on the llmux backend `auth` is `llmux-keyless-loopback`, `auth_refreshable` is false, and `auth_help` is absent, because there is nothing to fix. |
| sessions/session renderers | ui | implemented | The same facts as text and as one JSON document. Deterministic: two reads of an unchanged store are byte-identical. Recorded text is clipped at 2000 bytes and *every* session-controlled value is flattened -- including tool-call names and `finish_reason`, which are a closed vocabulary coming from a provider but arbitrary strings coming off the disk -- so a session that read a large file cannot flood a terminal and a recorded newline cannot forge a row. |
| JSONL turn event stream | ui | implemented | `assistant_delta`, `tool_start`, `tool_result`, `final`, and `error` are all produced by `ask --json`, with exactly one terminal event per turn. |
| setup renderers | ui | implemented | `xfx setup llmux` prints `[setup] key=value` lines -- backend, url, catalog size, model, why that model, settings path -- or exactly one JSON document with the same fields under `--json`. A failure exits 1 and reports in the shape the caller asked for: a diagnostic on stderr, or one terminal `error` document on stdout, matching `ask`. |
| streamed assistant text | ui | implemented | `ask` without `--json` writes only the answer to stdout, one delta at a time, and puts a failure on stderr. `orchestrator.zig:4650-4655`. |
| interactive turn notices | ui | implemented | In the shell only, each tool call is announced on stderr as `[tool] <name> running` and then `ok` or `refused: <reason>`, with the reason flattened to one line and clipped. `ask` is unchanged: its output is its answer. |
| full-screen TUI | ui | deferred | Not a goal. The shell is line-oriented, never enters raw mode or the alternate screen, and leaves the terminal's line discipline byte-identical. |
| notifications and status line | ui | deferred | Post-v0.1. `config_runtime.zig:139-152`. |
| colored and hyperlinked TTY output | ui | deferred | Post-v0.1; output is plain text. The one control sequence xfx emits is the erase pair `/clear` writes, and only when asked. |

## Embedding surfaces

| Surface | Kind | Status | Notes and upstream evidence |
|---|---|---|---|
| CLI binary | embedding | implemented | The only supported entry point. |
| WASM core | embedding | deferred | `src/wasm_core_main.zig`, `src/wasm_term_main.zig`. |
| N-API module | embedding | deferred | `src/napi_core_main.zig`, `sdk/NAPI.md`. |
| Rust library crate | embedding | partial | The crate is public so tests can drive it, but its API is not stable and it is not published. |
