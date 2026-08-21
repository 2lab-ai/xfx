# fxr Rust port design

Date: 2026-08-21

## Goal

Create `2lab-ai/fxr`, an unofficial Apache-2.0 Rust port of
[`vercel-labs/fx`](https://github.com/vercel-labs/fx), pinned to upstream commit
`580a0c5da9386317251968c09c1cee69e763487a`.

The first release must be a real coding agent. A user can stream a model turn,
let the model inspect and change an authorized workspace, run an admitted
command, persist the turn, resume it, inspect local status, and use the same
loop from a lightweight interactive shell. Unsupported upstream surfaces are
absent from command help and tool schemas, not represented by success stubs.

## Evidence boundary

The upstream product is much larger than its tagline implies:

- The native command union contains interactive, help, ask, ACP, GitHub,
  authentication, status, diagnostics, sessions, background work, usage,
  replay, workspace, and upgrade surfaces
  (`src/core/cli/cli_surface.zig:58-84`).
- The tool registry exposes 26 concrete tools (`src/builtins/tools.zig:1351-1378`).
- The main composition root separates app, agent, auth, config, permissions,
  sessions, tooling, Gateway, terminal, and UI responsibilities
  (`src/main.zig:8-153`).
- Upstream has 558 Zig files and 697,106 Zig lines at the pinned commit. Its
  deterministic suites include 95 root E2E owners.
- Upstream requires a built-binary happy-path interaction in addition to tests
  (`AGENTS.md:5-29`).

Therefore this is a behavioral port, not a line-for-line transliteration. Each
claimed parity row needs a runnable test.

## Approaches considered

### Literal whole-tree translation

Translate every Zig module before publishing. This maximizes nominal breadth
but cannot be reviewed or qualified as one change. It also preserves Zig-shaped
abstractions instead of producing idiomatic Rust. Rejected.

### Facade-first CLI

Implement every upstream command with placeholders, then fill them later. This
looks broad but violates the product contract: advertisement is a promise.
Rejected.

### Vertical behavioral port (selected)

Port the load-bearing loop first, from config and credentials through streamed
provider calls, permissioned tool execution, persistence, and user output.
Publish an explicit parity ledger and omit deferred commands/tools from all
runtime registries. This is the smallest release that is both useful and true.

## Supported v0.1 surface

### CLI

- `fxr`, the lightweight interactive shell
- `fxr ask [--auto|--yolo] [--json] [--quiet] [--no-save]
  [--resume <last|id>|--resume-id <id>] [--] <prompt>`
- `fxr status [--json]`
- `fxr doctor [--json]`
- `fxr sessions [--json] [--all] [--limit N]`
- `fxr session <last|id>|--id <id> [--json]`
- `fxr --help`, `fxr --version`

The binary is named `fxr` to avoid impersonating the upstream product. The
configuration home is `~/.fxr`, with project config `.fxr.json`.

### Providers

Vercel AI Gateway streaming is the first provider. Credential precedence is:

1. nonblank `VERCEL_OIDC_TOKEN`
2. nonblank `AI_GATEWAY_API_KEY`

The configured endpoint defaults to the upstream Gateway URL. An HTTP override
is accepted only for loopback test endpoints, matching the upstream bearer-token
safety boundary (`src/builtins/gateway.zig:759-765`).

### Tools

The initial registry advertises only complete tools:

- `list_files`
- `glob_files`
- `grep_files`
- `read_file`
- `write_file`
- `edit_file`
- `create_folder`
- `terminal` with an `exec` action only

Read output is bounded. Mutation paths are canonicalized inside the workspace,
existing files must be read before edit/write, writes use same-directory
staging plus atomic rename, and execution revalidates the prepared fingerprint.

### Permissions

- `ask`: mutations and commands require a TTY approval; noninteractive calls
  fail closed.
- `auto`: reads run directly, reversible workspace writes run after structural
  validation, and shell commands are limited to an allowlisted read/test grammar.
- `yolo`: skips policy checks and prints an explicit warning to stderr.

Rules and session grants are typed. Decode, validation, permission admission,
and execution are separate stages. A decision cannot mutate its target.

### Sessions and context

Sessions use an append-only JSONL event log under
`~/.fxr/sessions/<id>/events.jsonl`, plus an atomically replaced manifest.
Readers ignore an unpublished or malformed crash tail. Session indexes are
rebuildable projections.

Project context is loaded from bounded `AGENTS.md` files from filesystem root to
workspace and then from nested directories relevant to the active target.
Context is refreshed after resume rather than persisted as eternal truth.

### Interactive shell

The shell is deliberately not a full-screen TUI. It keeps normal terminal
scrollback, uses a line editor, streams assistant output, supports Ctrl-C, and
owns only `/help`, `/new`, `/clear`, `/model`, `/version`, and `/quit`.

## Architecture

A single Cargo package is used initially, with small modules and explicit
ports. Multiple crates add coordination cost before there are external
consumers.

- `cli`: command grammar and help; no leaf behavior.
- `config`: discovery, precedence, credentials, diagnostics.
- `output`: immutable snapshots and text/JSON/JSONL renderers.
- `gateway`: request serialization, reqwest/rustls transport, bounded SSE.
- `agent`: the explicit step state machine and exactly-once finalization.
- `tools`: immutable specs, schemas, decode/validate/execute implementations.
- `permission`: policies, rules, grants, and immutable execution authorities.
- `workspace`: canonical roots, path proofs, context, bounded search.
- `session`: durable events, manifest/index, list/detail/resume.
- `interactive`: line-oriented terminal shell.

The dependency direction is CLI -> application services -> domain contracts ->
adapters. Provider, filesystem, clock, approval, and output boundaries are
traits so deterministic tests do not use live credentials.

## Gateway data flow

1. Build messages as stable context, refreshed overlays, durable history,
   current user message, then within-turn assistant/tool suffix
   (`src/core/agent/runtime/prompt_context.zig:27-43`).
2. Serialize Gateway JSON as `prompt`, `tools`, and `toolChoice`; user and
   assistant content are typed parts and tool results correlate by call ID
   (`src/core/gateway/gateway_json.zig:333-381`,
   `src/core/gateway/gateway_json.zig:541-655`).
3. POST with bearer auth and stream SSE.
4. Consume `text-delta`, `tool-call`, and canonical `finish` events. A finish
   event is required; `[DONE]` alone does not prove completion.
5. Decode and validate every tool call before permission admission.
6. Mint an immutable one-use authority for the exact target, revalidate it,
   execute once, append assistant/tool messages, and request the next step.
7. Stop only on a valid terminal completion or an explicit bounded failure.

## Output contracts

Human mode streams text. `ask --json` emits JSONL events with `kind` values:
`assistant_delta`, `tool_start`, `tool_result`, `final`, and `error`.
Diagnostics go to stderr.

`status --json` is one newline-terminated JSON document with at least model,
auth source, permission mode, sandbox, workspace, history turns, and step limit.
`doctor --json` contains aggregate counts and `{name,status,detail}` checks.
Neither command requires credentials or mutates an empty home.

## No-stub rule

- Every advertised command has a handler and binary-level acceptance test.
- Every advertised tool has a schema, typed decoder, validator, permission
  policy, executor, and integration test.
- Production code may not contain `todo!`, `unimplemented!`, placeholder
  success, or canned assistant output.
- Deferred surfaces exist only in `docs/parity.md` and a centralized nonzero
  reserved-surface error, never in help or model schemas.

## Delivery slices

1. Foundation: Cargo project, CLI, config, credentials, status/doctor, parity
   ledger, license, and registry tests.
2. Gateway: exact message/request wire shapes, bounded SSE decoder, content-only
   `ask`, cancellation, and fake-Gateway binary E2E.
3. Read loop: tool registry, read/list/glob/grep, and a two-request tool loop.
4. Mutating loop: write/edit/create/terminal, permission authorities,
   adversarial path/race tests, and end-to-end edit/test scenario.
5. Durability: event log, manifest/index, sessions/detail/resume, and refreshed
   project context.
6. Interactive and ship: append shell, CI matrix, packages/checksums,
   documentation, external review, and live receipts.

## Deferred parity

The following are explicit post-v0.1 work: Vercel login/logout/setup, Codex OAuth,
model catalog/provider switching, ACP, MCP, skills, subagents, web tools,
background/durable terminals, images/vision, full-screen TUI, replay, usage and
credits, GitHub workflows, updater, WASM, and N-API. `docs/parity.md` records each
row with its upstream evidence and status.

## Testing and receipts

- Unit tests for request serialization, SSE fragmentation, permission policy,
  path resolution, session replay, and output snapshots.
- Integration tests for every advertised tool.
- Binary E2E against a local fake Gateway: content-only, read -> final, and
  read -> edit -> terminal -> final.
- Interactive smoke under a PTY.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test --all-targets`, and release build.
- GitHub CI for Linux/macOS on x86_64/aarch64 targets.
- External code review and an HTML receipt report.

## Risks and controls

- Shell commands are not OS-sandboxed in v0.1. The product reports
  `sandbox=none`; ask/auto authority is a policy boundary, not confinement.
- A retry after ambiguous delivery can duplicate model intent. The transport
  does not blindly retry a possibly sent request.
- File namespaces can change between proof and rename. Identity/hash checks and
  same-directory atomic replacement narrow the race; test hooks exercise stale
  authority rejection.
- Secrets never enter snapshots, logs, or tool output. Golden tests scan both
  stdout and stderr.
- Dependencies are pinned in `Cargo.lock`; TLS uses rustls, not ambient OpenSSL.

## Definition of done

The repository is done when all supported rows in `docs/parity.md` have a green
acceptance test, no deferred surface is advertised, all local and remote gates
pass on the exact commit, the release binary completes a real fake-Gateway
mutation loop, the interactive shell is exercised, the public GitHub repository
can be cloned and built, and the HTML receipt report captures raw outputs.
