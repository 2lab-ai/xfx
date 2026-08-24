# xfx — Spec

An unofficial Rust port of the `fx` terminal coding agent (`vercel-labs/fx`, Apache-2.0), pinned to
`580a0c5da9386317251968c09c1cee69e763487a`. One binary: a bounded agent turn that streams from a
provider, reads and changes a workspace under a permission authority, and records every turn to a
resumable log.

Status: **shipped** — `0.1.0`, released 2026-08-24. The `v0.1.0` tag is pushed and is the first
stable cut; `CHANGELOG.md` dates its `[0.1.0]` section to that day and opens an `[Unreleased]`
section above it. Two Homebrew formulae, one executable: `2lab-ai/tap/xfx` (stable, from `v*` tags)
and `2lab-ai/tap/xfx-preview` (rolling, from every `main` push). Both install `bin/xfx`, so only one
can be held at a time, and `xfx status --json` distinguishes them by `build_channel` (`release` vs
`preview`) rather than by version, which is `0.1.0` on every channel. `README.md` §Install states the
stable formula conditionally — "once a stable release is published" — because publication is what
the tag's workflow does, not what the tag alone proves.

Three words that are not synonyms, and this document keeps them apart:

- **Version** — the semver in `Cargo.toml`, `0.1.0`. It is the same on every channel and it says
  nothing about which build you hold. It also never encodes closeness to upstream `fx`.
- **Release identifier** — the git tag a stable build is cut from, `v0.1.0`, and the name of the
  release whose archives Homebrew resolves.
- **Preview identifier** — the prerelease tag `preview-<date>-<time>-<run>-<attempt>-<sha12>`, which
  is a *provenance* claim: it names the commit and the run that produced the binary. `CHANGELOG.md`
  states it directly — a preview's "version is the timestamp of the run that produced it rather than
  a release number: a preview is a provenance claim, not a version."

So "what version is this" and "what build is this" are different questions, and only the second has a
useful answer: `build_channel` plus the 12-character `build_revision`.

This document records the currently implemented contract, **not a future target.** The row-by-row
account of what exists is [`docs/parity.md`](../docs/parity.md); it is not duplicated here.

## Problem

Upstream `fx` is a large product — a full-screen TUI, MCP, skills, subagents, an ACP server, OAuth
logins, a model catalog, an updater, 26 tools — written in Zig. Three things follow from that, and
each is a reason this port exists rather than a wrapper:

1. **The load-bearing loop is small and the surface is large.** What makes the product useful is one
   bounded multi-step turn against a streaming provider with locally executed tools. Porting the
   whole surface first would produce a facade with nothing behind it.
2. **A port's most likely failure is a lie.** A ported agent that advertises `web_search` because the
   original does, and returns a polite stub, is worse than one that never mentions it: the user pays
   a token to learn the truth, and the model plans around a capability that is not there.
3. **The upstream binary owns the names.** A port that installs as `fx`, reads `~/.fx`, and honours
   `.fx.json` cannot be installed next to the thing it is a port of without being able to corrupt it.

## Goals (implemented)

1. **A behavioral port of the load-bearing loop.** `xfx ask` runs one bounded, multi-step turn:
   ordered assistant text, tool calls executed locally, then exactly one terminal event. `--json`
   makes it JSONL.
2. **Absence is real absence.** Everything not implemented is absent *from the binary* — not a flag,
   not a stub returning success. Mechanized in both directions, see the honesty contract below.
3. **A separate identity.** Binary `xfx`, profile home `~/.xfx`, project file `.xfx.json`, overrides
   `XFX_*`. Installing xfx cannot shadow, read, or corrupt an upstream installation
   (`UPSTREAM.md` §"Why the name is `xfx`"). The credential variables are the deliberate exception:
   `VERCEL_OIDC_TOKEN` and `AI_GATEWAY_API_KEY` name a Vercel service, not the product.
4. **A safety story that is stated rather than implied.** No OS sandbox, and `status` says
   `sandbox=none` for that reason. `ask` requires a real terminal approval per change and fails
   closed with no terminal; `auto` admits bounded reversible workspace writes and a reporting-only
   command grammar; `yolo` skips policy and says so on stderr, every time.
5. **Two backends behind one provider boundary.** `gateway` (Vercel AI Gateway, bearer credential
   from the environment) and `llmux` (a local llmux daemon over the Anthropic Messages wire, keyless
   on loopback). The choice is profile-only: a cloned repository must not be able to choose which
   endpoint receives a prompt.
6. **Durable, resumable, inspectable sessions.** Append-only `events.jsonl` with an atomically
   published manifest; `xfx sessions`, `xfx session <id>`, `ask --resume`. `--no-save` opens no store
   at all.
7. **A line-oriented shell that leaves the terminal as it found it.** Never raw mode, never the
   alternate screen; scrollback survives, and the acceptance test compares `termios` before and after
   on a real pseudoterminal.
8. **Testable without a credential or a network.** Provider, filesystem, clock, approval, and output
   are traits or injected values. The whole suite, and `scripts/smoke.sh` on a release binary, run
   against fakes on loopback.

## The honesty contract — advertisement is a promise

This is the product's distinguishing property. It is rule 1 of `CONTRIBUTING.md` — introduced there
as "a rule that is unusual, and it is the first one below" — and the one rule `README.md`
§Contributing bothers to restate: a name in `--help`, in `/help`, or in a tool schema must have a
handler, an acceptance test, and an `implemented` row in `docs/parity.md`, **in the same change that
adds the name.** There is no "wire it up later".

It is mechanized, not asserted:

- `scripts/check-no-stubs.sh` reconciles `docs/parity.md` against `src/cli.rs`, `src/tools/mod.rs`,
  and `src/interactive.rs` in **both** directions: every advertised surface has an `implemented` row;
  every `implemented` row names a real advertised surface; no name from a `deferred` row — including
  names listed inside a grouped row — is advertised anywhere; no surface appears in two rows.
- `tests/parity.rs` runs the same reconciliation against the **running binary**: the parser's own
  subcommand list, the tool schemas as they are serialized into a request, and the rendered help.
- The precedence rule is written down: if `README.md` disagrees with `docs/parity.md`, the ledger
  wins; if the ledger disagrees with the code, the code wins and both documents are bugs.

Two corollaries the product holds to elsewhere: `sandbox=none` is reported because it is true
(`UPSTREAM.md` deviation #2), and **xfx does not claim parity with `fx` and will not encode closeness
to it in a version number.**

## Shipped contract (v0.1.0)

Summarized; every row and its upstream evidence lives in `docs/parity.md`.

| Area | What ships |
|---|---|
| Commands | `ask`, `interactive` (a bare `xfx`), `status`, `doctor`, `sessions`, `session`, `setup llmux`, `help`. Six shell slash commands: `/help`, `/new`, `/clear`, `/model`, `/version`, `/quit`. |
| Tools | Eight, in registry order: `list_files`, `glob_files`, `grep_files`, `read_file` (read-only, admitted in every mode), `write_file`, `edit_file`, `create_folder` (mutating), `terminal` (`exec` action only). |
| Permissions | Three modes (`ask`/`auto`/`yolo`) over one authority model: a decision mints an immutable **one-use** authority for one exact target, revalidated immediately before it is spent. `.git` and `.xfx` are refused structurally, before any permission check, in every mode including `yolo`. |
| Sessions | `~/.xfx/sessions/<id>/events.jsonl`, `fsync`ed appends published by an atomically replaced manifest; a crash tail past the published boundary is invisible to readers. Resume restores history and the recorded model preference, never the permission mode. |
| Context | `AGENTS.md` from the filesystem root down to the workspace, bounded (32 files, 64 KiB each, 256 KiB total in model-visible bytes), rediscovered every turn rather than restored from a session. |
| Config | Layers: project `.xfx.json` → `~/.xfx/settings.json` → that file's exact `workspaces["<root>"]` entry → environment. Five keys are read because five are consumed: `max_agent_steps`, and the profile-only `model`, `permission_mode`, `backend`, `llmux_url`. Keys xfx does not consume are simply not read — which is what makes an older binary safe against a newer profile, and is the load-bearing fact in the migration rule below. |
| Backends | `gateway` (default) and `llmux`. `llmux_url` is held to a loopback-service policy — explicit port, no path, remote refused including https, because TLS protects a credential and there is no credential here. On llmux the credential is *present* when a `llmux_url` is configured and passes that rule — a **configuration** fact, not a reachability one. `auth=llmux-keyless-loopback` says exactly that: `status` and `doctor` do no network I/O and never probe the daemon, so a daemon that is down still has a present credential and is discovered where a request already legitimately happens — `xfx setup llmux`'s ping and catalog proof, a catalog load, or a turn. |
| Diagnostics | `status [--json]` and `doctor [--json]` resolve and report without a credential, without network I/O, and without creating anything. |
| Distribution | Two channels. A `v*` tag publishes one archive per target (`x86_64`/`aarch64` × linux-gnu/apple-darwin), each with its own `.sha256` plus one `SHA256SUMS`, and the `xfx` formula selects the archive matching the machine; a build from a tag reports `build_channel=release`. Every `main` push publishes a prerelease of that exact commit — four flat native binaries + `SHA256SUMS` — reporting `build_channel=preview` and a 12-character revision, installed by the `xfx-preview` formula. A preview is never marked latest and its version is the run's timestamp, "a provenance claim, not a version". Nothing is cross-compiled: the tests that decide whether a build is publishable open pseudoterminals and run child processes. |

Credential handling, stated as the two opposite promises the product actually makes:

- **xfx's own Gateway credential is never persisted.** It is read from the environment and sent to
  one endpoint; no session event, snapshot, or tool result carries it, and golden tests scan stdout
  and stderr.
- **What the model reads is saved.** Every file's contents and every command's output a tool returns
  is written verbatim to the session log as owner-only (`0600`) plaintext. `--no-save` is the only
  way to leave nothing behind.

## Profile compatibility promise

The shipped profile shape is flat: `backend`, `llmux_url`, and one `model`. The provider epic
introduces `provider` plus a per-provider `models{}` ([`04-providers.md`](04-providers.md)
§"Profile migration"). Because both shapes will exist on real machines at once, the compatibility
rule is part of *this* contract rather than a detail of that one:

1. **A read never rewrites the profile.** `status` and `doctor` stay side-effect-free; migration is
   read-repair in memory, and the file changes only when a command that already writes it runs.
2. **A newer key wins, and the older key is kept.** `provider` outranks `backend` and
   `models[provider]` outranks flat `model`; the legacy pair keeps being written for the two
   providers an older binary can actually reach, so a downgrade lands on an operator-chosen value
   rather than a built-in default.
3. **An unreadable choice is still never defaulted.** A `provider` xfx cannot read refuses exactly as
   an unreadable `backend` does today, and a disagreement between the two shapes is reported by
   `doctor` rather than silently resolved.
4. **Every one of these keys is profile-only.** A shared repository cannot choose the endpoint a
   prompt is sent to, before or after the migration. That is the property the whole rule exists to
   preserve.

## Non-goals (v0.1.0)

- **Parity.** Not a goal, not a roadmap item, and never a version number.
- **A full-screen TUI.** The shell is line-oriented by decision (`UPSTREAM.md` deviation #6). The
  cost is recorded as the deferred `prompt history` row: no recall, no arrow-key editing, no menu.
  (The next epic revisits exactly this — [`03-tui-port.md`](03-tui-port.md), with its acceptance
  harness in [`06-qa-harness.md`](06-qa-harness.md).)
- **A second provider family.** No Codex, no Grok, no `login`/`logout`, no stored or Keychain
  credentials, no model catalog or provider switching. (Next epic —
  [`04-providers.md`](04-providers.md).)
- **An OS sandbox.** Deferred and reported as absent. `ask`/`auto` are a policy boundary, not
  confinement.
- **Agent network egress.** No `web_fetch`, no `web_search`.
- **MCP, skills, subagents, ACP, vision, memory, semantic search, durable terminals, GitHub
  workflows, the updater, replay, usage and credits, WASM, N-API.**
- **A stable library API.** The crate is public so tests can drive it; it is not published.
- **Windows.** Linux and macOS, x86_64 and aarch64.

## Acceptance (the gate, verified per commit)

`cargo fmt --check`; `cargo clippy --locked --all-targets -- -D warnings`; `cargo test --locked
--all-targets`; `cargo build --locked --release`; then `scripts/check-no-stubs.sh`,
`check-no-secrets.sh`, `check-xfx-identity.sh`, `check-preview-contract.sh`, and
`scripts/smoke.sh target/release/xfx`.

Three of those are self-checking rather than trusted: the secret scan and the identity scan each run
their patterns against a deliberately dirty control fixture first and fail if the scan reports
nothing, because a check whose failure mode is "silently passes" is not evidence. `smoke.sh` drives
the **release** binary end to end against a fake Gateway on a loopback port — no credential, no
network — through a mutation loop, a refused destructive command, the session lifecycle including
resume and rebind, and the shell on a real pseudoterminal, leaving every captured stream in an
evidence directory it prints.

## Risks / tensions

- **No confinement.** A command xfx agrees to run runs with the user's privileges, environment, and
  network. The permission modes decide what xfx agrees to *start*. This is the residual risk of the
  whole product, and it is why `auto` is deliberately narrower than upstream's
  (`UPSTREAM.md` deviation #9). Upstream retired its own sandbox in 0.0.5, which removes the
  asymmetry argument but not the risk — see [`05-upstream-delta.md`](05-upstream-delta.md) §B1.
- **The session log is a secondary secret store.** Tool results are saved verbatim. Documented in
  `README.md` §"Safety, in plain terms" rather than mitigated; `--no-save` is the escape hatch.
- **The pin ages.** Every behavioral claim cites `580a0c5d`, and upstream has since moved to
  `ef1d0d0` (0.0.5). The ledger does not silently follow — `UPSTREAM.md` §"Upstream has moved since
  the pin" states that a surface upstream added after the pin has no row at all, "because an unread
  surface is not one it can claim to be missing". The gap is tracked as a document
  ([`05-upstream-delta.md`](05-upstream-delta.md)) so advancing the pin is a decision, not a drift.
- **The ledger is prose about code.** The reconciliation scripts cover names — commands, tools, slash
  commands — not the *narrowness* claims inside a row's notes. Those are held by tests, and a note
  that outruns its test is the ledger's residual failure mode.
- **llmux's wire is another implementation's contract.** There is no `thinking` field because pinning
  it to `disabled` was refused by the daemon for the default model (measured 2026-08-22); adaptive
  thinking is what xfx gets, and the decoder preserves the resulting blocks for replay.

## Provenance

- Upstream behavior and every `file:line` claim: `vercel-labs/fx` @ `580a0c5d`, Apache-2.0,
  Copyright 2025 Vercel, Inc. Attribution and the nine deliberate deviations: `UPSTREAM.md`.
- No Zig source is copied. xfx: Apache-2.0, Copyright 2026 2lab.ai. `LICENSE` and `NOTICE` travel
  with every archive.
- The port's design record — approaches considered, delivery slices, definition of done:
  `docs/superpowers/specs/2026-08-21-xfx-rust-port-design.md`.
- Backend daemon: `2lab-ai/llmux`.
