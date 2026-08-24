# xfx — Architecture

Rust, edition 2021, `rust-version = 1.96`. One binary, one crate, a `tokio` current-thread-friendly
runtime (`rt`, no multi-thread feature). The dependency direction is one way, and every boundary that
would otherwise need a credential or a socket is a trait or an injected value — which is why the
whole suite runs with neither.

```
cli ──► app ──► agent ──► gateway::Provider ──► (network)
                  │  │                 └─ gateway::GatewayProvider | llmux::LlmuxProvider
                  │  └──► tools ──► permission
                  │           └──► workspace
                  └──► session ──► (~/.xfx)
                          output ◄── everything that prints
```

Narrative form of the same flow, step by step, is [`docs/architecture.md`](../docs/architecture.md).
This document is the map and the invariants.

## Crate layout

```text
src/
  main.rs            # entry: build the runtime, call app::run, map to an exit code
  lib.rs             # the module contract; the crate is public so tests can drive it
  cli.rs             # closed command grammar + help metadata; ADVERTISED_* consts the parity check reads
  config.rs          # layer discovery/merge, Backend, PermissionMode, Credential, Diagnostic, RuntimeConfig
  app.rs             # composition and dispatch: one match arm per command, AppError -> exit code
  interactive.rs     # the line-oriented shell, SLASH_COMMANDS, signal policy
  output.rs          # immutable snapshots + the text/JSON/JSONL renderers; the only thing that writes bytes
  build_meta.rs      # channel (debug|release|preview) + 12-char revision, validated at compile time
  agent/
    mod.rs           # re-exports
    types.rs         # TurnRequest, TurnJournal, NoJournal — a turn owns its limits
    machine.rs       # TurnMachine: bounded steps, ordered text, exactly one terminal event
  gateway/
    mod.rs           # Provider trait, DeltaSink, CancelToken, Endpoint/EndpointPolicy, ProviderError, GatewayProvider
    protocol.rs      # the Gateway request/response shape, hand-written; no I/O
    sse.rs           # bounded SSE decode; a canonical `finish` is required
  llmux/
    mod.rs           # LlmuxProvider + the loopback-only endpoint constructor
    protocol.rs      # the Anthropic Messages wire as llmux speaks it; no I/O, no `thinking` field
    sse.rs           # Anthropic event decode incl. thinking/content blocks preserved for replay
    setup.rs         # `xfx setup llmux`: discover, prove, record. SetupReport/SetupError/CatalogEntry
  tools/
    mod.rs           # Registry: a closed `static` list; what is advertised is what runs
    spec.rs          # the JSON schemas serialized into a request
    read.rs          # list_files, glob_files, grep_files, read_file — bounded, sorted, capped
    mutate.rs        # write_file, edit_file, create_folder — staged, revalidated, renamed atomically
    terminal.rs      # terminal(exec): argv for admitted read-only commands, shell only after approval
  permission/
    mod.rs           # re-exports
    policy.rs        # modes, Rule/Grant/PermissionRules, PolicyDecision, ApprovalPrompter/TtyPrompter, PermissionSession
    command.rs       # the reporting-only command grammar: CommandEffect / DeniedEffect
    authority.rs     # the one-use authority model (below): ContentHash, FileIdentity, Nonce, AuthorityLedger,
                     #   MutationPlan/PreparedMutation, CommandPlan/PreparedCommand, ExecutionAuthority
  session/
    mod.rs           # re-exports
    event.rs         # SessionEvent, EventEnvelope, RecordedToolCall, TurnConclusion, FrameError
    store.rs         # SessionStore/WritableSession/SessionRecorder, SessionManifest, DurableState, resume + listing
  workspace/
    mod.rs           # re-exports
    path.rs          # AccessScope, ResolvedPath, PathError, ignored/protected directory rules
    context.rs       # ProjectContext: bounded AGENTS.md discovery, ContextSection/ContextOmission
build.rs             # stamps channel + revision into the binary; rejects a channel that is neither
tests/
  cli.rs             # grammar, help, exit codes  (note: parts are cfg'd off macOS)
  gateway.rs         # the Gateway wire and its SSE, against a recording fake
  llmux.rs           # the Messages wire, setup discovery/proof, and the live-minimal capture
  interactive.rs     # a real pty: types into the shell, asserts termios is byte-identical after
  permissions.rs     # modes, grammar, authority spend/revalidate
  sessions.rs        # append, publish, crash tail, resume, rebind, listing
  tool_loop.rs       # multi-step turns end to end
  parity.rs          # reconciles docs/parity.md against the *running* binary
  support/
    fake_gateway.rs  # raw TcpListener: records method/path/headers/body, replays scripted SSE, can tear a stream
    fake_llmux.rs    # the same, for the Messages wire + `GET /` and `GET /models`
    llmux-live-minimal.sse  # a captured real daemon stream, replayed as a regression fixture
scripts/
  check-no-stubs.sh          # ledger <-> source, both directions
  check-no-secrets.sh        # credential shapes, with a positive control
  check-xfx-identity.sh      # the retired local name must not survive, with a positive control
  check-preview-contract.sh  # the preview release's artifact set and naming
  smoke.sh                   # release binary, end to end, on a fake Gateway and a real pty
```

## The provider boundary

`gateway::Provider` is the seam (`src/gateway/mod.rs:645`):

```rust
#[async_trait::async_trait(?Send)]
pub trait Provider {
    async fn stream(&self, request: &CompletionRequest, deltas: &mut dyn DeltaSink)
        -> Result<Completion, ProviderError>;
}
```

Invariants it carries:

- **One call is one transport attempt.** An implementation must not retry internally, because only
  the turn knows whether an answer already reached the user. Retry is the turn's decision; the
  transport reports whether a failed attempt provably delivered nothing.
- **Two implementations, one shape.** `GatewayProvider` (HTTPS, bearer credential, Gateway wire) and
  `LlmuxProvider` (loopback HTTP, no credential, Anthropic Messages wire). A test drives a scripted
  stream through the same trait, which is why protocol assertions are about bytes rather than about a
  mock's expectations.
- **Wire shape is separated from I/O.** `*/protocol.rs` performs no I/O at all, so every wire
  question is answerable by a test that never opens a socket. Serialization is hand-written rather
  than derived: the shape is an external contract with another implementation, and it must not drift
  when an internal field is renamed.
- **Decode is bounded and demands a terminator.** A canonical `finish` is required, because `[DONE]`
  alone does not prove completion. The read loop re-checks the `CancelToken` on a short poll, so
  Ctrl-C ends a stream that has gone quiet.
- **The endpoint is a policy, not a string.** `EndpointPolicy` admits an HTTP override only for
  loopback; for llmux it is stricter still — loopback host, explicit port, no path, remote refused
  under either scheme, and no `HTTP_PROXY`/`ALL_PROXY` honoured. The reason is stated where it is
  enforced: TLS protects a credential, and on this backend there is none.
- **Identity is xfx's own.** The Gateway header set follows upstream's shape but does not claim to be
  `fx`; misattributing this port's traffic to the product it ports would be a lie of the same family
  the ledger exists to prevent.

The backend is chosen by the profile-only `backend` setting; an unrecognized value does **not** fall
back to a default, because falling back would send a prompt somewhere the operator did not choose —
it is reported by `doctor` and the turn refuses.

## The one-use permission authority

The load-bearing safety model, in `src/permission/`. A permission decision does not authorize a
*tool*; it mints an immutable authority for exactly one action against exactly one target, which is
then revalidated immediately before it is spent.

1. **Validate before deciding.** Every tool call is decoded and structurally validated before
   permission is considered, so an approval is never asked for a call that could not have run.
2. **Structural refusals precede policy.** Writes into `.git` or `.xfx` are refused before any
   permission check, in every mode including `yolo`, because a `.git/config` entry decides what the
   commands xfx may later run will execute.
3. **Mint.** `policy::PolicyDecision` yields a plan: `MutationPlan` (target scope, `Preimage`,
   `MutationExcerpt`) or `CommandPlan` (`CommandRoute`), carrying a `Nonce` recorded in the
   `AuthorityLedger`. A decision cannot mutate its target.
4. **Spend once, revalidate first.** `PreparedMutation`/`PreparedCommand` re-derive the proof at the
   moment of action: `FileIdentity` (device/inode) **and** `ContentHash` (SHA-256 of the preimage).
   A namespace that changed between proof and rename is a refusal, not a race the attacker wins.
   Path resolution walks component by component with `openat(..., O_NOFOLLOW)` (`rustix`), so no
   component can be substituted between check and write.
5. **Grants are scoped to what was sold.** "Always" grants one tool and one target: for a mutation
   the target is the file's **canonical absolute path**, so a grant cannot follow a resumed session
   into another workspace; for a command the key is its text **and** its working directory, so an
   approval in the workspace does not carry into an `--add-dir` root. Each is recorded against one
   session id and released with it (`/new` drops them).
6. **`auto` is a grammar, not a classifier.** `permission::command` admits reporting-only commands:
   no `&&`, no shell, no package-manager build or test families, nothing that compiles or runs
   project code, and no Cargo subcommand outside the alias-proof built-ins — because an `[alias]` in
   a `.cargo/config.toml` that `auto` may itself have written can redirect a benign-looking one.
7. **`ask` fails closed.** With no approval channel — a pipe, CI — it refuses instead of asking.

Everything above is a policy boundary. It bounds what xfx agrees to **start**; a started process is
not confined, and `status` reports `sandbox=none` for that reason.

## Session log design

- **Append-only truth, published boundary.** Events are appended to `~/.xfx/sessions/<id>/events.jsonl`
  as they happen — not at a tidy ending, because an interrupted turn is exactly the one whose
  evidence matters. Each append is `fsync`ed and then *published* by an atomically replaced
  `session.json`, so a crash tail past the published boundary is invisible to every reader.
- **The manifest is a projection, never an authority.** It summarizes the log and names the boundary
  it was computed from; disagreement between the two **refuses** the session rather than preferring
  either. The manifest is staged under a process-unique name and cleaned up by RAII, so xfx never
  unlinks a stage another writer owns.
- **One writer.** A writable session takes an exclusive advisory lock on its log.
- **Replay fidelity.** An `assistant_message` additionally records `raw_content` when the provider
  sent its own content blocks — the Anthropic wire's signed reasoning — replayed on resume and never
  displayed. Additive: absent on older records and on the Gateway wire.
- **Resume restores history and the recorded model preference; it never restores the permission
  mode.** `--resume last` is scoped to the current workspace; an exact id may be resumed from
  another workspace and writes a durable `workspace_rebound` event before the turn, keeping the
  origin root intact.
- **`--no-save` opens no store at all** — no session directory, no manifest — and therefore conflicts
  with the resume flags, because continuing a conversation while refusing to record it would fork its
  history in silence.
- **Reads are deterministic and flattening.** Two reads of an unchanged store are byte-identical;
  every session-controlled value is flattened and clipped, including tool-call names and
  `finish_reason`, which are a closed vocabulary from a provider but arbitrary strings off a disk.

## Test harness

- **Unit tests beside the code**: request serialization, SSE fragmentation, permission policy, path
  resolution, session replay, output snapshots.
- **Binary-level tests** spawn the real executable against `tests/support/fake_gateway.rs` /
  `fake_llmux.rs`. Both are written directly on `std::net::TcpListener` rather than on a framework,
  so a test can split one SSE event across several TCP writes and close a connection mid-body without
  a terminating chunk — both are protocol facts xfx must survive, and both are how a real stream
  fails. They record the exact method, path, headers, and body xfx sent.
- **A captured real stream** (`tests/support/llmux-live-minimal.sse`) is replayed as a fixture, so the
  daemon's actual bytes — not an idealization of them — stay a regression.
- **A real pseudoterminal** for the shell: the child gets its own session and controlling terminal,
  the test types into it, and `termios` is compared before and after. That is the only way to test a
  prompt, an echoed Ctrl-C, and a restored line discipline.
- **`tests/parity.rs`** reconciles the ledger against the running binary; `scripts/check-no-stubs.sh`
  does it against the source text.
- **`scripts/smoke.sh`** does the whole thing again for a *release* binary and leaves raw evidence in
  a printed directory.
- Nothing in any of it uses a live credential or reaches the network.

**Known blind spot:** parts of `tests/cli.rs` are compiled out on macOS, so a CLI-surface change can
be locally green and only fail on `check (ubuntu-latest)`. Read that job before believing a local
pass on a grammar change.

## Concurrency and process model

This section describes **v0.1.0**. The TUI epic changes the topology, and
[`03-tui-port.md`](03-tui-port.md) §"Runtime topology (authoritative)" is the single source of truth
for the target — thread ownership, the bounded channels between the UI thread and the runtime,
cancellation, and shutdown ordering. What follows is what ships today and what the target must not
break.

- Single binary, no daemon, no background threads for work: one **current-thread** tokio runtime built
  in `main` and driving everything through one `block_on(app::run(..))` (`src/main.rs:17-25`), because
  xfx drives one turn at a time and never spawns work that outlives the command. When it returns there
  is no work left running anywhere.
- The one extra OS thread is `xfx-interrupt` (`src/app.rs:539-580`): its own current-thread runtime,
  detached, SIGINT only. It exists because the main runtime is single-threaded and is *blocked* for
  the duration of a `terminal` command and for as long as a user takes to type a line, so a signal
  observable only by that runtime would arrive exactly when it could not be noticed. Its start is a
  handshake — a `sync_channel(1)` released once the signal future has been polled once, bounded by a
  timeout — so a prompt is never printed while Ctrl-C would still kill xfx outright.
- The shell's line read is a blocking `stdin().read_line` on the runtime thread
  (`src/interactive.rs:602-604`); turn-vs-idle state is a `Mutex<Activity>` (`:254-266`) the signal
  thread inspects to decide whether an interrupt means cancel, clear the line, or exit 130.
- A cancelled turn kills any running command's **process group**, so a grandchild cannot outlive it
  holding the pipe xfx is reading (`rustix`, since `std` exposes neither a path-relative `openat` nor
  a group kill).
- `CancelToken` is an `Arc<AtomicBool>` polled by the stream reader, so cancellation reaches a socket
  that has gone quiet rather than waiting for a server that stopped talking.

## Key dependencies

`clap` (grammar), `serde`/`serde_json`, `reqwest` with **rustls only** (never an ambient system
OpenSSL) + `futures-util` for the byte stream, `tokio` (`rt`, `macros`, `net`, `signal`, `sync`,
`time`), `sha2` (preimage digests), `ignore` (ripgrep's walker: bounded, deterministic when sorted,
does not follow symlinks, applies `.gitignore`), `globset`, `async-trait`, and on Unix `rustix`
(`fs`, `process`; plus `pty`, `termios` for tests only — a release build links neither). Everything is
pinned in `Cargo.lock`, and the release profile strips.
