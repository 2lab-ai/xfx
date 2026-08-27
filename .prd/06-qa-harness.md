# xfx — TUI QA harness

Status: **specification.** The seed exists — `scripts/smoke.sh` already drives a release binary
through a real pseudoterminal against a fake Gateway on loopback. This document extends that seed
into the acceptance harness the TUI epic ([`03-tui-port.md`](03-tui-port.md)) needs, phase by phase.

## What this is, and what it is not

Two things in this epic are called "subagent", and they have nothing to do with each other:

- **fx's in-product subagents** — the `subagent` tool, the manager alt screen, the panel and its input
  routing. **Deferred**, in `docs/parity.md` and in `03-tui-port.md`, and this document does not change
  that.
- **This harness** — Claude agents driving the built `xfx` binary from outside, through a pty, and
  asserting on what the terminal received. It ships **nothing** into the binary: no flag, no
  entrypoint, no tool. It is test infrastructure under `scripts/` and `tests/`, and the honesty
  contract is unaffected because nothing is advertised.

The reason it has to exist: a TUI's contract is *what is on the screen*, and no unit test can see a
screen. Today's binary is testable by reading stdout because output is line-oriented; a cell grid
painted with minimal diffs is not, and "it looked right when I ran it" is not a receipt.

## The seed, and what it already proves

`scripts/smoke.sh` writes out two helpers under an evidence directory and runs them
(`scripts/smoke.sh:222-300`):

- `pty_shell.py` — `pty.fork()`, `os.execve` the real binary with an environment **built from nothing**
  rather than inherited (the comment records why: a developer with `XFX_PERMISSION_MODE=yolo` exported
  would otherwise be smoke-testing their shell instead of the binary), then a `select`-driven `pump()`
  that waits for a regex against everything captured so far, `send()` for input, and a `require()`
  that accumulates named problems instead of dying at the first one.
- `fake_gateway.py` — a loopback HTTP server that replays scripted SSE and records what it was sent.

It already asserts the shell prints a prompt, `/help` lists commands, `/model` reports the model, an
unknown slash command is refused, a **unicode** prompt is answered, and `/quit` exits — with the raw
transcript kept as evidence. Everything below is the same shape, with three additions: a **frame
oracle**, an **agent driver**, and **termios inspection**.

## Architecture

```
Claude agent (scenario author + judge)
   │  runs, reads captured frames, asserts, files the receipt
   ▼
harness runner (python)
   ├─ pty.fork() ──► xfx (release binary, real terminal)
   ├─ fixture server ──► fake gateway / fake llmux on loopback
   ├─ frame oracle: feed captured bytes to a VT emulator → cell grid snapshots
   └─ evidence dir: raw byte log, per-step grid snapshots, termios before/after
```

**Why an agent rather than a fixed script**: a scripted assertion can only check what its author
predicted. The failures a TUI actually has — a leaked SGR after a stream, a band that shrank and left
a stale row, a footer that repainted over the last line of an answer — are visible in a rendered grid
and invisible to a regex. An agent that can look at successive grids, compare them, and describe the
difference is the cheapest available oracle for "the screen is wrong in a way nobody wrote a test
for". Deterministic assertions stay in the runner; the agent adds exploration on top and, when it
finds something, its finding is **converted into a deterministic assertion** before the phase is
accepted. The agent is a discovery instrument, never the gate.

## The oracle

Three levels, cheapest first. Every scenario names which it uses.

1. **Byte assertions** (the seed's `pump`/`require`): a regex against the captured stream. Adequate for
   presence and ordering of escape sequences — "`?2026h` wrapped the frame", "`1049h` was never
   written", "the restore sequence contains no `1049l`".
2. **Cell-grid assertions** (new, the main oracle): feed the captured bytes to a VT emulator
   (`pyte` is the obvious choice) sized to the pty's own dimensions, and assert on **cell content**:
   the text of row N, that the footer's top row is where the geometry says it is, that a given cell's
   foreground attribute is what the theme says, that no row contains a stale fragment of the previous
   frame. Grids are snapshotted per step and written to the evidence directory as plain text, so a
   failure is diffable by a human and by an agent.
3. **Terminal-state assertions**: `termios` before launch and after exit, plus the **positive** raw-mode
   proof read from the child's terminal while it runs — the two-sided suite
   [`03-tui-port.md`](03-tui-port.md) §"Acceptance — terminal state, positively proven" specifies. This
   level is not optional in any phase, because it is the one property the product currently sells.

Snapshot discipline: a grid snapshot is committed as a golden **only** where the content is genuinely
deterministic (layout geometry, static chrome, a fixed fixture's rendered answer). Anything carrying a
clock, an elapsed time, a token count or an animation phase is asserted by predicate, not by golden —
otherwise the suite teaches people to re-bless it, and a re-blessed golden proves nothing.

## Fixtures and the mock-vs-live rule

All fixtures are SSE scripts served by the existing fakes (`tests/support/fake_gateway.rs`,
`fake_llmux.rs`, and smoke's python equivalents). Three rules:

1. **Every fixture's assistant text carries a unique marker string** — a token that exists nowhere in
   the product, in any real model's vocabulary, or in another fixture. The seed already does this
   informally (`shell answer`); the harness makes it a contract. An assertion is against the marker,
   so a screen that "looks right" but came from somewhere else fails.
2. **Mock-vs-live is decided by positive evidence, never by absence.** "No fixture marker appeared"
   is satisfied by a blank screen, a crashed binary, and a hung turn — it proves nothing. Each run
   therefore mints a **per-run nonce** and requires all three of:
   (i) the nonce is embedded in the prompt the scenario sends;
   (ii) the nonce is present in a **client-side capture of the request xfx actually sent** — not in a
   server-side log the harness also controls, and not a bare request id, which proves only that
   *something* was sent. In mock mode the fake servers already record method, path, headers and body
   and the harness asserts the nonce in that body. In live mode, where there is no cooperating server,
   the capture is xfx's own outbound record (a debug request log written under the evidence directory,
   or a loopback recording proxy the harness inserts) **or** a provable echo: the prompt instructs the
   model to repeat the nonce verbatim and the assertion is that it comes back in the rendered output.
   One of capture-or-echo is mandatory; a request id satisfies neither, because an id is generated
   whether or not the prompt reached anything;
   (iii) the rendered output is **non-empty** and contains the expected evidence — the fixture marker
   in mock mode, or a positively asserted live property in live mode (a real model id in the hint row,
   a generation id, a `usage` count greater than zero).
   Marker-absence may be used only as an *additional* negative check on top of (i)–(iii), never as the
   pass condition. This is the failure class where a mockup screen gets mistaken for real data, and it
   is only closed by requiring something to be present.
3. **Fixtures include the ugly cases**, because the pretty ones never fail: an SSE event split across
   several TCP writes; a stream closed mid-body with no terminator; a `finish` that never arrives; an
   `error` frame inside a 200; text containing CR, tabs, wide CJK glyphs, combining marks, ZWJ
   emoji, and an ANSI sequence the model emitted as *content* (which must be rendered inert, never
   obeyed). The Rust fakes already support the first two by construction.

The captured real stream `tests/support/llmux-live-minimal.sse` stays what it is — a regression
fixture of a daemon's actual bytes — and is one of the streaming scenarios.

## Scenarios by phase

Each scenario names its oracle level. Phases match [`03-tui-port.md`](03-tui-port.md) §"MVS ladder".

**Phase 1 — launch, restore, editor, streaming, approval.**

| # | Scenario | Oracle | Passes when |
|---|---|---|---|
| 1 | Launch and band ownership | 2 | The band is painted at the bottom; prior shell output is above it and intact; `1049h` never written; frames wrapped in `?2026h`/`?2026l` |
| 2 | Cursor probe and scrollback push | 1+2 | CSI `6n` issued; pre-existing shell lines are still readable above the band after the first frame |
| 3 | Restore matrix | 3 | Every row of [`03-tui-port.md`](03-tui-port.md) §"Acceptance — terminal state, positively proven": normal, panic, SIGTERM/SIGHUP (assert `WIFSIGNALED`), TSTP/CONT (assert `WIFSTOPPED` while stopped), partial init, and no-SIGINT-handler. `termios` equality is asserted in every one, because only `tcsetattr` from the saved struct can produce it |
| 3b | Shutdown drain, no deadlock | 1+2 | Quit **while a fixture is mid-stream with the UI artificially slowed**, so the `UiEvent` channel is full and the async producer is pending in `send().await` on it: the process must still exit within the deadline, the terminal must be restored, and the session log's manifest must be published and self-consistent. This is the regression test for the drain protocol |
| 4 | Raw mode positively entered | 3 | `ECHO`/`ICANON`/`IEXTEN`/`ISIG` clear, `VMIN=1`, `VTIME=0`, mouse tracking absent |
| 5 | Editor basics | 2 | Type, arrows, Home/End, word moves, Backspace/Delete; the composer grid matches the typed text; grapheme motion moves a ZWJ family as one unit |
| 6 | Soft wrap and growth cap | 2 | A long paragraph wraps word-aware with hanging spaces; the composer stops growing at `content_bottom/2 + 1` |
| 7 | Multiline and paste **framing** | 1+2 | Shift/Alt+Enter and `\` continuation insert a newline. `?2004h` is enabled. A pasted block containing **embedded newlines, a `0x03`, and an ESC sequence** produces **exactly one** prompt: assert the fixture server received one request whose body carries the whole pasted text, that no turn was cancelled, and that the ESC was not decoded as a key. A paste over 1000 codepoints collapses to `[Pasted text #1, N lines]` on screen and expands verbatim on submit. Placeholder *atomicity* under cursor motion and delete is **Phase 2** ([`03-tui-port.md`](03-tui-port.md) §"Phase 1 and paste") and is not asserted here |
| 8 | Streaming render | 2 | The marker text arrives progressively across frames; final grid contains it exactly once; no SGR leaks past the answer (assert a plain-attribute cell after it) |
| 9 | Activity row | 2 | `• Thinking` with elapsed appears while the fixture withholds output, and the clock **freezes** while an approval is pending |
| 10b | Approval mid-turn does not deadlock | 1+2 | With a turn in flight and a submit already queued in `TurnWork`, answering the approval must still take effect — proving the answer travelled on `TurnControl` and not behind the queued prompt. A second submit while one is queued must be **rejected with a visible notice** and must leave the composer text intact |
| 10 | Approval panel | 2 | A mutating tool call in `ask` renders the 3-choice panel with the correct "always" wording; `1`/`2`/`3`, ↑↓, Esc and Ctrl-C each produce the right outcome; the fixture server sees the tool result that the choice implies |
| 11 | Ctrl-C as a byte | 1+3 | `0x03` cancels a running turn; a second exits 130; terminal restored in both |
| 12 | Theme detection | 1+2 | OSC 11 queried at start; a dark and a light fixture response each select the matching palette (assert cell attributes, not a log line) |

**Phase 2 — surfaces.**

| # | Scenario | Oracle | Passes when |
|---|---|---|---|
| 13 | Cell diff correctness | 2 | After the diff replaces full-band repaint, the rendered grid is **identical** to the Phase-1 full-repaint grid for the same input — the diff is an optimization and must be observationally equivalent |
| 14 | No-op frame skip | 1 | A tick with nothing pending emits zero bytes |
| 15 | Resize | 2 | SIGWINCH at several widths, including mid-stream; content reflows, no stale rows, band geometry recomputed; a resize during an approval does not lose the pending decision |
| 16 | Slash menu | 2 | `/` opens the picker in the rows a question would take, above the divider, with the composer still holding the caret; ranking is exact-prefix > alias > substring, ties in the order `/help` lists them. **`/exit` supplies the alias tier**: it is a real `SLASH_REGISTRY` alias for `/quit` rather than a test fixture, so `/e` -- which names no command -- must rank `/quit` above the five commands that merely contain an `e`, and the row must say which name put it there. A word that names nothing lists nothing. Esc dismisses without arming the composer's own double-Escape clear, and the dismissal survives further typing until the trigger kind changes; Tab completes, with a trailing space for the command that takes an argument (asserted through the caret, since a space is not a readable cell), and the completed command runs on one Return without reaching the fixture server |
| 17 | Prompt history | 2 | ↑/↓ and C-p/C-n recall; entering history captures the current draft and leaving restores it |
| 18 | `/setup` provider switching | 2 | With fake gateway **and** fake llmux both up, the picker lists both, switching repaints the hint row's model label, and the **next prompt reaches the newly selected fixture server** (marker discriminates which) |
| 19 | `/model` catalog | 2 | The catalog fixture's models render with context window and effort; selection persists to the profile; a higher layer that outranks the write is reported |
| 20 | Alt-screen approval | 2 | A large diff takes the alt screen, and leaving it restores the primary screen in one commit with no flicker gap (assert no intermediate blank grid) |
| 21 | Paste placeholder atomicity | 2 | Backspace at the placeholder's right edge removes the **whole** block; cursor motion steps over it as one unit; recalling it through history renumbers the paste id; undo treats the paste as one boundary |

**Phase 3 — depth.** Undo/redo and kill-ring behavior (2); question panel ordinals and freeform (2);
readiness gate — an affirmative before the frame committed must be **refused** (2); commit self-check
recovery under an injected partial write (1+2); live theme switch re-tints the transcript (2).

## Acceptance criteria per phase

A phase is accepted when **all** hold:

1. Every scenario for that phase and every earlier phase passes on a **release** binary, on macOS and
   Linux, on its own native runner.
2. The terminal-state suite (oracle 3) passes in every case, including the ones an earlier phase
   already had — restoration regressions are the class most likely to come back.
3. Every finding an agent made during exploration is either fixed or converted into a deterministic
   scenario above; an open finding with neither is a blocking item.
4. Evidence is complete: raw byte log, per-step grids, and `termios` captures, in a printed directory,
   for every scenario — the standard `scripts/smoke.sh` already sets.
5. Every run satisfies the three-part positive discriminator above — nonce in the prompt, nonce in a
   client-side request capture or provably echoed back, and non-empty output carrying the expected
   evidence — and
   every mock-mode assertion names its marker. A scenario that passes only because nothing appeared is
   a failed scenario.
6. `docs/parity.md` is updated in the same change for anything the phase made advertisable, and
   nothing the phase did **not** finish is advertised anywhere.

## Relationship to the existing gate

`scripts/smoke.sh` stays what it is and keeps running: it is the line-oriented product's receipt, and
that product does not stop existing when a TUI arrives — `xfx ask` is still a pipe-friendly command
with no terminal. The TUI harness is a **second** runner alongside it, gated on the same rule the
current one obeys: **no live credential, no network.** Both write evidence to a printed directory, and
CI runs both on native runners for the same reason nothing is cross-compiled — the tests that decide
whether a build is publishable open pseudoterminals and run child processes.
