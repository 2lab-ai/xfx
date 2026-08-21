# fxr Initial Rust Port Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Publish a usable, security-bounded Rust coding agent at `2lab-ai/fxr` that implements the supported v0.1 behavioral contract of `vercel-labs/fx`.

**Architecture:** One Rust package with focused modules and explicit provider/filesystem/approval/output ports. Implement the vertical path in dependency order: CLI/config/output, Gateway turn, read-only tools, mutating tools and authorities, durable sessions/context, then the interactive shell and distribution. Runtime registries are closed sets whose inventory must reconcile with tests and `docs/parity.md`.

**Tech Stack:** Rust 1.96+, Tokio, reqwest with rustls, serde/serde_json, clap, futures-util, globset/ignore, regex, sha2, uuid, fs2, tempfile, rustyline, crossterm, assert_cmd, predicates, wiremock, insta.

**Spec:** `docs/superpowers/specs/2026-08-21-fxr-rust-port-design.md`

## Global Constraints

- Upstream evidence is pinned to `580a0c5da9386317251968c09c1cee69e763487a`.
- Product and executable name is `fxr`; profile directory is `~/.fxr`; project file is `.fxr.json`.
- Targets are macOS and Linux on x86_64 and aarch64; no Windows claim.
- Every advertised command/tool is implemented end to end and tested; no `todo!`, `unimplemented!`, placeholder success, canned assistant output, or unsupported advertised surface.
- Deferred surfaces appear only in `docs/parity.md`, never in command help or model tool schemas.
- Gateway URL overrides carrying bearer credentials accept loopback HTTP only; normal remote endpoints require HTTPS.
- Human text, JSON document, and JSONL event outputs are separate renderers; diagnostics use stderr.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets`, and `cargo build --release` must pass.
- Commit messages end with `Co-Authored-By: Claude <noreply@anthropic.com>`.

## File Structure

- `Cargo.toml`, `Cargo.lock`: package metadata and pinned dependencies.
- `build.rs`: 12-character source revision and build channel metadata.
- `src/main.rs`: Tokio bootstrap and exit-code mapping only.
- `src/app.rs`: application composition and command dispatch.
- `src/cli.rs`: closed command grammar and help metadata.
- `src/config.rs`: profile/project/env discovery, merge, credentials, diagnostics.
- `src/output.rs`: immutable snapshots and text/JSON/JSONL renderers.
- `src/gateway/{mod.rs,protocol.rs,sse.rs}`: provider trait, wire serialization, transport, SSE events.
- `src/agent/{mod.rs,machine.rs,types.rs}`: message contracts and bounded multi-step turn machine.
- `src/tools/{mod.rs,spec.rs,read.rs,mutate.rs,terminal.rs}`: registry and complete tool implementations.
- `src/permission/{mod.rs,authority.rs,policy.rs}`: modes, rules, grants, and one-use authorities.
- `src/workspace/{mod.rs,path.rs,context.rs}`: canonical scope, proofs, and bounded `AGENTS.md` context.
- `src/session/{mod.rs,event.rs,store.rs}`: event log, manifest/index, list/detail/resume.
- `src/interactive.rs`: line-oriented interactive shell.
- `tests/{cli.rs,gateway.rs,tool_loop.rs,permissions.rs,sessions.rs,interactive.rs}`: binary/integration contracts.
- `tests/support/{mod.rs,fake_gateway.rs}`: deterministic local SSE server and request capture.
- `docs/{parity.md,architecture.md}` and `UPSTREAM.md`: product truth and evidence.
- `.github/workflows/ci.yml`, `.github/workflows/release.yml`: qualification and package assembly.
- `scripts/check-no-stubs.sh`, `scripts/smoke.sh`: inventory/no-stub and live receipts.

---

### Task 1: Foundation, CLI, configuration, and snapshots

**Files:**
- Create: `Cargo.toml`, `build.rs`, `src/{main.rs,lib.rs,app.rs,cli.rs,config.rs,output.rs}`
- Create: `tests/cli.rs`, `docs/parity.md`, `UPSTREAM.md`, `LICENSE`, `NOTICE`
- Create: `scripts/check-no-stubs.sh`

**Interfaces:**
- Produces: `cli::Cli`, `cli::Command`, `config::RuntimeConfig::load(workspace)`, `config::Credential`, `output::StatusSnapshot`, `output::DoctorSnapshot`, `output::EventSink`.
- Produces: `app::run(cli: Cli) -> Result<ExitCode, AppError>` used by every later task.

- [ ] **Step 1: Write failing CLI and snapshot tests**

Add binary tests asserting exact `--version`, `help` aliases, unknown command stderr/exit 1, no deferred command names, strict `status [--json]` and `doctor [--json]`, newline-terminated JSON, no secret bytes, and no `~/.fxr` creation for read-only commands. Add config unit tests for project -> profile global -> exact workspace -> env precedence and ignored profile-only project keys.

- [ ] **Step 2: Verify RED**

Run: `cargo test --test cli`
Expected: FAIL because the package/binary does not exist.

- [ ] **Step 3: Implement the minimal foundation**

Create the package and closed Clap command enum. `main.rs` calls only `app::run`. Implement a manual merge of `.fxr.json`, `~/.fxr/settings.json`, exact workspace settings, and `FXR_MODEL`/`FXR_PERMISSION_MODE`/`FXR_MAX_AGENT_STEPS`. Resolve only nonblank `VERCEL_OIDC_TOKEN` and `AI_GATEWAY_API_KEY`, retaining source labels but never values in snapshots. Render status and doctor from typed structs.

- [ ] **Step 4: Add the parity and no-stub inventories**

`docs/parity.md` contains one row per upstream command, tool group, provider, persistence surface, UI, and embedding surface with `implemented`, `partial`, or `deferred`. `scripts/check-no-stubs.sh` fails on production `todo!`, `unimplemented!`, placeholder/canned output, or any implemented CLI/tool inventory item missing a parity row.

- [ ] **Step 5: Verify GREEN**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --test cli && ./scripts/check-no-stubs.sh`
Expected: all commands exit 0.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock build.rs src tests/cli.rs docs/parity.md UPSTREAM.md LICENSE NOTICE scripts/check-no-stubs.sh
git commit -m $'feat: establish fxr CLI and configuration core\n\nCo-Authored-By: Claude <noreply@anthropic.com>'
```

### Task 2: Gateway protocol and content-only turn

**Files:**
- Create: `src/gateway/{mod.rs,protocol.rs,sse.rs}`, `src/agent/{mod.rs,machine.rs,types.rs}`
- Create: `tests/gateway.rs`, `tests/support/{mod.rs,fake_gateway.rs}`
- Modify: `src/{lib.rs,app.rs,output.rs}`

**Interfaces:**
- Consumes: `RuntimeConfig`, `Credential`, `EventSink`.
- Produces: `gateway::Provider` async trait, `GatewayProvider`, `agent::TurnMachine`, `agent::run_turn(TurnRequest, &dyn Provider, &mut dyn EventSink)`.
- Produces wire types `Message`, `ContentPart`, `ToolCall`, `Completion`, `FinishReason`.

- [ ] **Step 1: Write request and SSE RED tests**

Assert exact Gateway shape: `prompt`, closed `tools`, `toolChoice`; typed user text, assistant tool calls, and correlated tool results. Feed SSE one byte at a time and assert `text-delta`, direct/streamed `tool-call`, usage, finish, error, malformed nonterminal handling, cancellation, and rejection of EOF or `[DONE]` without canonical finish.

- [ ] **Step 2: Write content-only binary RED test**

Start the fake Gateway, run `fxr ask --json --no-save hello`, verify bearer/source headers and request shape, stream two deltas and one stop finish, and expect `assistant_delta` JSONL followed by exactly one `final` event.

- [ ] **Step 3: Verify RED**

Run: `cargo test --test gateway`
Expected: FAIL because provider and state machine are absent.

- [ ] **Step 4: Implement protocol and transport**

Use reqwest/rustls and `bytes_stream`. Serialize protocol structs through serde. Implement a bounded SSE frame decoder with a 32 MiB event ceiling, a required finish event, usage extraction, and typed errors. Reject non-loopback HTTP overrides and credential-bearing non-HTTPS URLs.

- [ ] **Step 5: Implement bounded turn state machine**

A turn owns `max_steps`, `max_attempts`, cancellation, and an exactly-once finalizer. A transport attempt is never blindly replayed after body delivery begins. Content-only completion writes deltas, emits a final event, and returns. Tool calls are rejected until Task 3 rather than advertised.

- [ ] **Step 6: Verify GREEN and commit**

Run: `cargo test --test gateway && cargo test --lib gateway agent`
Expected: PASS.

```bash
git add src/gateway src/agent src/app.rs src/lib.rs src/output.rs tests/gateway.rs tests/support
git commit -m $'feat: stream Gateway turns through a bounded agent core\n\nCo-Authored-By: Claude <noreply@anthropic.com>'
```

### Task 3: Read-only tool registry and multi-step loop

**Files:**
- Create: `src/tools/{mod.rs,spec.rs,read.rs}`, `src/workspace/{mod.rs,path.rs}`
- Create: `tests/tool_loop.rs`
- Modify: `src/agent/{machine.rs,types.rs}`, `src/gateway/protocol.rs`, `src/lib.rs`

**Interfaces:**
- Produces: `tools::Registry`, `ToolSpec`, `ToolExecutor`, `ToolResult`, `workspace::AccessScope`.
- `Registry::advertisement()` returns schemas only for executable specs.
- `Registry::execute(call, context)` performs decode -> validate -> availability -> admission -> execution.

- [ ] **Step 1: Write registry and executor RED tests**

Assert the registry is exactly `list_files`, `glob_files`, `grep_files`, `read_file`; schemas are closed and bounded. Test canonical workspace and additional roots, symlink escape rejection, UTF-8/binary behavior, line ranges, 400-line defaults, output truncation, ignored directories, literal grep, and deterministic ordering.

- [ ] **Step 2: Write two-request loop RED test**

Fake request 1 returns a `read_file` call. Assert one local execution. Fake request 2 must contain one assistant call and one matching tool result and then returns final text. Assert exactly two Gateway requests, one execution, and one finalization.

- [ ] **Step 3: Verify RED**

Run: `cargo test --test tool_loop`
Expected: FAIL because the registry is absent.

- [ ] **Step 4: Implement the immutable registry and read tools**

Specs own name, description, JSON schema, decoder, validator, permission kind, and executor. Use `ignore::WalkBuilder`, `globset`, and `regex` with fixed limits. Resolve every path through `AccessScope`; canonical targets outside authorized roots fail before I/O.

- [ ] **Step 5: Extend TurnMachine**

Advertise registry schemas, append assistant tool-call messages and tool-result messages in provider order, execute read-only calls sequentially, and request the next model step. Reject duplicate IDs, malformed input, missing specs, and an assistant tool-call finish with zero valid calls.

- [ ] **Step 6: Verify GREEN and commit**

Run: `cargo test --test tool_loop && cargo test --lib tools workspace agent`
Expected: PASS.

```bash
git add src/tools src/workspace src/agent src/gateway/protocol.rs src/lib.rs tests/tool_loop.rs
git commit -m $'feat: add bounded read tools and multi-step execution\n\nCo-Authored-By: Claude <noreply@anthropic.com>'
```

### Task 4: Secure mutations, terminal exec, and permission authorities

**Files:**
- Create: `src/tools/{mutate.rs,terminal.rs}`, `src/permission/{mod.rs,authority.rs,policy.rs}`
- Create: `tests/permissions.rs`
- Modify: `src/tools/{mod.rs,spec.rs}`, `src/workspace/path.rs`, `src/agent/machine.rs`, `src/app.rs`

**Interfaces:**
- Produces: `PermissionMode::{Ask,Auto,Yolo}`, `PolicyDecision`, `ExecutionAuthority`.
- `PreparedMutation` includes canonical target, prior identity/hash, staged bytes, and one-use nonce.
- `PreparedCommand` includes exact command, cwd, environment, and effect class.

- [ ] **Step 1: Write permission and mutation RED tests**

Cover ask/no-prompter fail closed, once/always/deny, auto reads, reversible writes, destructive shell denial, exact session grants, yolo warning, changed authority fingerprints, stale preimage, unique edit replacement, no-op, repeated/missing edit strings, parent/target symlink swaps, cancellation, permissions preservation, atomic replacement, and cleanup.

- [ ] **Step 2: Write release-defining E2E RED test**

The fake Gateway asks to read a fixture, edit it, run `cargo test`, and return a final summary. Assert the actual file changed once, terminal output is correlated, every tool executes once, request history is valid, and JSONL remains parseable. A second run changes the file after admission and must fail without mutation or continuation.

- [ ] **Step 3: Verify RED**

Run: `cargo test --test permissions`
Expected: FAIL because mutation/authority modules are absent.

- [ ] **Step 4: Implement authority-first admission**

Policy evaluates configured rules, session grants, mode, and action structure without side effects. Ask uses a TTY prompter; auto admits only declared safe reads/reversible writes and a small read/test command grammar; yolo emits the warning. Decisions mint immutable one-use authorities and execution consumes them.

- [ ] **Step 5: Implement mutation and terminal executors**

Require an earlier complete read proof before modifying an existing file. Stage in the same directory, revalidate identity and SHA-256, preserve permissions, rename atomically, and sync parent metadata. Terminal exec uses Tokio process, exact cwd, timeout/cancellation, bounded output, exit/signal facts, and no shell for direct commands; reviewed commands use the platform shell.

- [ ] **Step 6: Advertise only qualified specs**

Add `write_file`, `edit_file`, `create_folder`, and exec-only `terminal` only after their tests pass. The terminal schema exposes no durable actions.

- [ ] **Step 7: Verify GREEN and commit**

Run: `cargo test --test permissions && cargo test --lib permission tools workspace agent`
Expected: PASS.

```bash
git add src/permission src/tools src/workspace/path.rs src/agent/machine.rs src/app.rs tests/permissions.rs
git commit -m $'feat: authorize and execute secure workspace changes\n\nCo-Authored-By: Claude <noreply@anthropic.com>'
```

### Task 5: Crash-safe sessions, resume, and project context

**Files:**
- Create: `src/session/{mod.rs,event.rs,store.rs}`, `src/workspace/context.rs`
- Create: `tests/sessions.rs`
- Modify: `src/{app.rs,cli.rs,config.rs,output.rs}`, `src/agent/{machine.rs,types.rs}`, `src/lib.rs`

**Interfaces:**
- Produces: `SessionStore::{create,append,publish,list,detail,resume}`, `SessionEvent`, `SessionManifest`, `ProjectContext::discover`.
- `TurnMachine` receives restored history and emits typed durable events through a `TurnJournal` trait.

- [ ] **Step 1: Write durability RED tests**

Commit a turn with read/edit/command evidence, restart the store, and require exact replay through the published boundary. Append a valid unpublished crash tail and malformed/truncated tail; readers ignore it and a writable open truncates it. Reject sequence gaps, duplicate IDs, unsafe session paths, and mismatched manifests.

- [ ] **Step 2: Write list/detail/resume/context RED tests**

Test deterministic newest ordering, workspace filtering, `--all`, exact ID, latest current workspace, cross-workspace exact resume with an explicit rebound event, bounded `AGENTS.md` precedence, nested target context, additional-root exclusion, duplicate suppression, and refreshed context after resume.

- [ ] **Step 3: Verify RED**

Run: `cargo test --test sessions`
Expected: FAIL because the store/context modules are absent.

- [ ] **Step 4: Implement append log and projections**

Each event is one JSON line with schema version, sequence, ID, and timestamp. Append+fsync first, then atomically replace the manifest with the published byte boundary and digest. Index and summaries are projections rebuilt from manifests. File/directory modes are private on Unix.

- [ ] **Step 5: Integrate ask persistence and resume**

Unless `--no-save`, persist user start, assistant/tool steps, final/interrupted outcome, usage, preferences, workspace origin/current roots, and context boundary. On resume, restore history and preferences but rediscover current project context before the next model request.

- [ ] **Step 6: Implement sessions command surfaces**

Wire list/detail and ask resume grammar. Status reports active history count and session grant count. Read-only session/status/doctor commands do not create profile state.

- [ ] **Step 7: Verify GREEN and commit**

Run: `cargo test --test sessions && cargo test --all-targets`
Expected: PASS.

```bash
git add src/session src/workspace/context.rs src/app.rs src/cli.rs src/config.rs src/output.rs src/agent src/lib.rs tests/sessions.rs
git commit -m $'feat: persist and resume durable agent sessions\n\nCo-Authored-By: Claude <noreply@anthropic.com>'
```

### Task 6: Interactive shell, distribution, qualification, and publication

**Files:**
- Create: `src/interactive.rs`, `tests/interactive.rs`, `scripts/smoke.sh`
- Create: `.github/workflows/{ci.yml,release.yml}`
- Create: `README.md`, `CONTRIBUTING.md`, `CHANGELOG.md`, `docs/architecture.md`
- Modify: `src/{app.rs,cli.rs,lib.rs}`, `docs/parity.md`, `UPSTREAM.md`

**Interfaces:**
- Consumes all prior application services.
- Produces a line-oriented interactive entrypoint with `/help`, `/new`, `/clear`, `/model`, `/version`, `/quit`.
- Produces release archives `fxr-<target>.tar.gz` plus SHA-256 checksums.

- [ ] **Step 1: Write interactive RED tests**

Under a PTY, verify non-TTY rejection for bare `fxr`, prompt display, Unicode input, streamed response, Ctrl-C cancellation, `/clear` preserving session identity, `/new` creating a new identity, the six commands, deterministic unknown slash errors, and terminal restoration after normal/error paths.

- [ ] **Step 2: Verify RED**

Run: `cargo test --test interactive`
Expected: FAIL because the interactive shell is absent.

- [ ] **Step 3: Implement the append shell**

Use Rustyline for cooked input and crossterm only for temporary stream/cancellation handling. Never enter an alternate screen. Feed each submitted prompt into the same `TurnMachine` and `SessionStore`; join worker tasks before restoring the terminal.

- [ ] **Step 4: Add docs and parity reconciliation**

README leads with “unofficial experimental Rust port,” the supported v0.1 path, install/build/run examples, permission warning, and parity link. Architecture documents the real data flow. Update every parity row and add a test/script that every runtime command/tool maps to exactly one implemented row and every deferred row is absent from help/schema.

- [ ] **Step 5: Add CI and release packaging**

CI runs fmt, clippy, all tests, no-stub/parity checks, secret scan, and release build on native macOS/Linux, with cross-check builds for the other architectures. Release workflow packages four target archives and checksums on tags; do not create a stable tag in this task.

- [ ] **Step 6: Run the full direct gate**

Run:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo build --release
./scripts/check-no-stubs.sh
./scripts/smoke.sh target/release/fxr
```

Expected: every command exits 0; smoke output captures help, status JSON, content-only ask, multi-step mutation, sessions/resume, and interactive PTY.

- [ ] **Step 7: External review and repairs**

Dispatch one external reviewer for correctness/security and one for test completeness against the spec, each with the exact diff and receipt paths. Reproduce every confirmed blocker as RED, repair it through the owning earlier task boundary, and rerun Step 6 until no blocker survives.

- [ ] **Step 8: Commit the qualified release candidate**

```bash
git add src tests scripts .github README.md CONTRIBUTING.md CHANGELOG.md docs UPSTREAM.md Cargo.toml Cargo.lock
git commit -m $'feat: qualify the initial fxr release\n\nCo-Authored-By: Claude <noreply@anthropic.com>'
```

- [ ] **Step 9: Publish and verify remote CI**

Create public `2lab-ai/fxr` if still absent, add `origin`, push the feature branch, open a PR, require exact-head CI green, merge, then verify main CI and a fresh clone build. Repository creation/push are outward actions already explicitly requested by the active goal; if the harness classifier denies them, report the exact denied command without bypassing it.

- [ ] **Step 10: Build the mandatory HTML receipt**

Write a self-contained report in the session scratchpad with upstream pin, supported/deferred reconciliation, commits, raw gate outputs, live fake-Gateway request/result evidence, PTY capture, external review outcomes, remote URLs/CI, and fresh-clone commands. Load `artifact-design`, publish it with `Artifact`, and include the private link in the final report.

## Final self-review

- Spec coverage: all supported CLI, Gateway, tools, permissions, sessions/context, interactive, CI, publication, and receipt requirements map to Tasks 1-6.
- Deferred surfaces are represented only by Task 1/6 parity ledger work and inventory checks.
- Type continuity: `RuntimeConfig`, `EventSink`, `Provider`, `TurnMachine`, `Registry`, `ExecutionAuthority`, `SessionStore`, and `ProjectContext` are defined before consumption.
- Placeholder scan: no implementation step relies on an undefined placeholder; deferred product work is explicit scope, not a production stub.
