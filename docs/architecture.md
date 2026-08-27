# Architecture

This describes what xfx actually does with your input, in the order it does it.
It is written against the code; where the two disagree the code is right and
this file is a bug.

## The shape

One binary, one crate, eleven modules. The dependency direction is one way:

```
cli ─┬─► app ──► agent ──► gateway ──► (network)
     │            │  └──► tools ──► permission
     │            │           └──► workspace
     │            └──► session ──► (~/.xfx)
     │                    output ◄── everything that prints
     └─► tui ──► (worker thread) ──► agent ──► …
          └──► the terminal
```

- **`cli`** decides what was asked. It owns the command grammar and nothing
  else: no leaf behavior, no I/O. Its command set is closed, and the closed list
  is a `const` the parity check reads.
- **`config`** decides how. Discovery, layer precedence, credential resolution,
  and the diagnostics that explain a settings file it could not use.
- **`app`** is composition and dispatch: one place that turns a parsed
  invocation into bytes on a stream and an exit code.
- **`interactive`** is the loop a bare `xfx` runs. It adds a prompt, the seven
  slash commands of `SLASH_REGISTRY`, and an interrupt policy on top of the same
  services `ask` uses. The count is the registry's: `/help`, the refusal for an
  unknown command, and the TUI's completion menu all derive it rather than
  spell it, so adding a command does not leave a stale number behind.
- **`gateway`** is the provider boundary. `Provider` is a trait, so a turn can
  be driven by a scripted stream in a test and a real socket in the binary.
- **`agent`** is the turn state machine: bounded steps, ordered assistant text,
  exactly one terminal event, exactly one journaled conclusion.
- **`tools`** is a closed `static` registry. What is advertised to the model is
  the same object that runs.
- **`permission`** owns modes, rules, grants, and one-use execution
  authorities. A decision cannot mutate its target.
- **`workspace`** owns canonical roots, path proofs, bounded search, and the
  `AGENTS.md` context a turn carries.
- **`session`** owns the durable event log, its published boundary, and resume.
- **`output`** owns every byte the product writes: immutable snapshots plus a
  text, JSON, and JSONL renderer. Nothing else formats for a user.
- **`tui`** is the opt-in full-screen surface, and the one branch that does not
  go through `app::run` (`src/main.rs:22-25`). It is not a command: it owns the
  main thread and blocks on the terminal, so it cannot be reached from inside
  the runtime at all.

Provider, filesystem, clock, approval, and output are traits or injected values,
which is why the whole suite runs with no credential and no network.

## One `ask`, end to end

1. **Parse.** `cli::Cli::from_args` produces a `Command`. A parse failure is a
   value, not a print, so `app::run` stays the only place that maps an outcome
   to an exit code.
2. **Resolve configuration.** `config::RuntimeConfig::load` merges the layers,
   resolves the credential by precedence, and collects non-fatal diagnostics.
   A malformed settings file is a fact `doctor` reports, not a refusal to run.
3. **Resolve authority.** `workspace::AccessScope` canonicalizes the workspace
   root and every `--add-dir`. A directory xfx cannot use fails the turn here,
   before a credential is read or a socket is opened.
4. **Open the session.** Unless `--no-save`, `session::SessionStore` creates or
   resumes a session and takes an exclusive advisory lock on its log. A resume
   that names a session that does not exist fails before a token is spent.
5. **Read project context.** `workspace::ProjectContext::discover` reads bounded
   `AGENTS.md` files from the filesystem root down to the workspace. It is
   rediscovered every turn, never restored from a session, so editing the file
   takes effect on the next prompt.
6. **Build the turn.** `agent::TurnRequest` carries the model, the prompt, the
   restored history, the step and attempt bounds, a cancellation token, and the
   tool context. A turn owns its limits rather than reading global state.
7. **Step.** For each step the machine builds a `CompletionRequest` -- stable
   context, refreshed overlays, durable history, the current user message, then
   this turn's own assistant and tool suffix -- and hands it to the provider.
8. **Stream.** `gateway::GatewayProvider` POSTs with bearer auth over rustls and
   decodes the SSE body with a bounded reader. `text-delta` fragments reach the
   sink as they arrive; `tool-call` events are collected; a canonical `finish`
   is *required*, because `[DONE]` alone does not prove completion. The read
   loop re-checks cancellation on a short poll, so Ctrl-C ends a stream that has
   gone quiet instead of waiting for a server that has stopped talking.
9. **Admit.** Every tool call is decoded and validated before permission is
   considered. `permission` then mints an immutable, one-use authority for the
   exact target; `tools` revalidates it immediately before acting, so a
   namespace that changed between proof and rename is a refusal rather than a
   race the attacker wins.
10. **Execute and correlate.** One tool call becomes exactly one local execution
    and exactly one tool result, correlated by call id, appended to the turn's
    suffix and sent with the next request.
11. **Journal.** Assistant steps, tool calls, results, and the conclusion are
    appended to the session log as they happen -- not at a tidy ending, because
    an interrupted turn is exactly the one whose evidence matters. Each append
    is `fsync`ed and then *published* by an atomically replaced manifest, so a
    crash tail past the published boundary is invisible to every reader.
12. **Finish.** The machine emits exactly one terminal event: `final` or
    `error`. `output` renders it as streamed text or as JSONL. Persistence
    failures are reported *beside* the answer, never instead of it: the answer
    did arrive.

## Session log design and replay fidelity

An `assistant_message` event records the provider's own replayable state in two disjoint fields: `raw_content` carries Anthropic Messages content blocks (signed reasoning blocks), and `responses_state` carries OpenAI Responses items (encrypted reasoning). A turn writes at most one, because only one provider's state is present. A `wire` field names the provider **and authority** that produced the state and is written only when there is state to replay, so byte-parity with older records is free: missing `wire` and missing `responses_state` are byte-identical to records from binaries that do not know these fields at all.

Replay is **keyed by authority, not by shape**. Two wires can serialize reasoning identically and still not be interchangeable, because the reasoning is sealed by the provider that issued the credential — Anthropic seals its blocks with signatures that it verifies on replay, and OpenAI encrypts its items with keys only it holds. The replay decision is therefore never "does this look like something I could send" but always "did the provider I am about to talk to produce it". When a session recorded under one wire is resumed under another, the state is **dropped** — with a one-line notice on stderr naming both wires — and the turn replays with text and tool calls only, degraded but correct. The stored items remain on disk (not deleted), so a later resume back onto the original authority replays them: dropping shapes a *request*, never a record.

Legacy records (pre-wire) are handled by authority inference: a record with non-empty `raw_content` and no `wire` field can only have come from Anthropic (the only provider that ever wrote that field), so it replays on Anthropic. A record with non-empty `responses_state` and no `wire` is invalid — no pre-wire binary produced this field — and drops with a notice rather than being guessed at. A record with neither field and no wire replays under the active provider.

Unknown future wires (a record naming a `wire` this binary does not know) are dropped for the same reason rather than being guessed at, and the notice names the unknown wire and the active one the user is asking for.

## One band at the bottom of the screen

`XFX_TUI=1` on a bare invocation opts into the TUI, and the whole of what is
different about it is who owns which thread.

**The UI thread is the process's main thread and is not inside a runtime.** It
takes the terminal into raw mode on the *normal* buffer, waits in `pselect(2)`
with an 8 ms tick, decodes escape sequences into keystrokes, and commits each
frame as one write wrapped in synchronized output. It never awaits anything.

**The worker owns the runtime.** A submitted prompt is handed to a worker thread
that builds its own current-thread runtime and runs exactly the turn `ask` runs
-- same provider, same registry, same permission authority, same session store.

**Three channels join them**, and the split is what keeps a question answerable
while a prompt is waiting: `TurnWork` carries submissions to the worker and is
bounded, so a queue is a queue rather than a surprise; `UiEvent` carries
everything the turn produces back and is bounded too, so a UI that cannot keep
up parks the producer instead of growing without limit; and `TurnControl`
carries cancellations and approval answers, is unbounded, and is drained
*inside* the turn -- which is why an answer cannot queue behind a prompt the
turn will not dequeue until it ends.

Thread and runtime ownership is specified in full in
[`.prd/03-tui-port.md`](../.prd/03-tui-port.md) §"Runtime topology
(authoritative)"; `.prd/02-architecture.md` §"Concurrency and process model"
already points there.

## One shell prompt

The shell is the same pipeline with a loop around it.

- It requires a terminal on both stdin and stdout, and a place to record, and
  refuses precisely when either is missing.
- It reads a line through the terminal's own canonical mode. On this path xfx
  never enters raw mode and never takes the alternate screen, so there is no
  terminal state to restore -- a property the acceptance tests assert by
  comparing `termios` before and after, on a real pseudoterminal. The TUI above
  is the one surface that does take the terminal, and it is therefore the one
  that has to give it back on every exit path.
- The first prompt lazily creates the session and the tool authority bound to
  it. `/new` drops both; `/clear` erases the screen and keeps both.
- Each prompt runs one `TurnMachine` through the same provider, registry,
  permission session, and recorder. History comes from the recorder's durable
  state, so the shell has no second memory that could drift from the log.
- One signal thread handles SIGINT for the process. It is installed before the
  first prompt is printed, holds the same lock that decides whether a turn is
  running, and therefore knows whether an interrupt means "cancel this turn",
  "exit now", or "clear the line". The turn is awaited on the main thread, so
  when it returns there is no work left running anywhere.

## What is deliberately not here

- **No second entry point.** Every command is a match arm in `app::run_with`.
- **No hidden surface.** The advertised commands, entrypoints, tools, and slash
  commands are three `const` lists reconciled against `docs/parity.md` by
  `scripts/check-no-stubs.sh` (source text) and `tests/parity.rs` (the running
  binary), in both directions.
- **No sandbox.** xfx reports `sandbox=none` because it does not confine
  commands. `ask` and `auto` bound what xfx agrees to start, which is a policy
  boundary and not confinement.
- **No blind retry.** The transport performs exactly one attempt per call and
  reports whether a failed attempt provably delivered nothing. The turn owns the
  retry decision, because only the turn knows whether an answer already reached
  the user.
- **No ambient TLS.** rustls, pinned in `Cargo.lock`, never a system OpenSSL.

## Testing model

- Unit tests live beside the code and cover request serialization, SSE
  fragmentation, permission policy, path resolution, session replay, and output
  snapshots.
- `tests/*.rs` are binary-level: they spawn the real executable against a fake
  Gateway that records exactly what xfx sent and replays a scripted stream, so
  protocol assertions are about bytes rather than about a mock's expectations.
- `tests/interactive.rs` allocates a real pseudoterminal, gives the child its
  own session and controlling terminal, and types into it -- which is the only
  way to test a prompt, an echoed Ctrl-C, and a restored line discipline.
- `tests/parity.rs` reconciles the ledger against the running binary.
- `tests/tui.rs` is the same idea for the full-screen surface: every case is one
  row of `.prd/03-tui-port.md` §"Acceptance -- terminal state, positively
  proven", and the rows that need a deliberate failure run under the
  `fault-injection` feature, which is off by default and in no shipped binary.
- `scripts/smoke.sh` does the same for a *release* binary, end to end, and
  leaves raw evidence behind.
- `scripts/smoke-tui.sh` is the second runner beside it, for the surface whose
  contract is what is on the screen: it drives a release binary on a real
  pseudoterminal, rebuilds a cell grid from the bytes the terminal received, and
  reads the child's `termios` while it runs. Its VT emulator **fails the run on
  any sequence it does not know**, so the emitted subset is pinned as well as
  read.

Nothing in any of it uses a live credential or reaches the network.
