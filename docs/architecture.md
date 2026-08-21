# Architecture

This describes what xfx actually does with your input, in the order it does it.
It is written against the code; where the two disagree the code is right and
this file is a bug.

## The shape

One binary, one crate, ten modules. The dependency direction is one way:

```
cli ──► app ──► agent ──► gateway ──► (network)
                  │  └──► tools ──► permission
                  │           └──► workspace
                  └──► session ──► (~/.xfx)
                          output ◄── everything that prints
```

- **`cli`** decides what was asked. It owns the command grammar and nothing
  else: no leaf behavior, no I/O. Its command set is closed, and the closed list
  is a `const` the parity check reads.
- **`config`** decides how. Discovery, layer precedence, credential resolution,
  and the diagnostics that explain a settings file it could not use.
- **`app`** is composition and dispatch: one place that turns a parsed
  invocation into bytes on a stream and an exit code.
- **`interactive`** is the loop a bare `xfx` runs. It adds a prompt, six slash
  commands, and an interrupt policy on top of the same services `ask` uses.
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

## One shell prompt

The shell is the same pipeline with a loop around it.

- It requires a terminal on both stdin and stdout, and a place to record, and
  refuses precisely when either is missing.
- It reads a line through the terminal's own canonical mode. xfx never enters
  raw mode and never takes the alternate screen, so there is no terminal state
  to restore -- a property the acceptance tests assert by comparing `termios`
  before and after, on a real pseudoterminal.
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
- `scripts/smoke.sh` does the same for a *release* binary, end to end, and
  leaves raw evidence behind.

Nothing in any of it uses a live credential or reaches the network.
