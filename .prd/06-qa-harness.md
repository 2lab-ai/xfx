# xfx — TUI QA harness

Status: **built, and it is the gate.** `scripts/smoke.sh` still drives the line-oriented release
binary through a real pseudoterminal against a fake Gateway on loopback; `scripts/smoke-tui.sh` is
the second runner this document specified, and every Phase-1 and Phase-2 scenario below is
registered in it and runs against a release binary on a real terminal -- including the lettered row
3c, which is a Phase-1 restoration row driven by the `fault-injection` build beside it. Of Phase 3, the rows whose
ladder items have shipped are registered too -- 22 for the edit history, 23 and 23b for the question
the model asks, 24 for the approval readiness gate and 25 for an amended approval -- and the rest are
still specification, which they say where they are listed.

**The scenario tables below are the specification; the runner is the registration.** Every id in
`scripts/smoke-tui.sh` begins with the row number it implements — `3b-shutdown-drain` is row 3b and
`20-alternate-screen-approval` is row 20 — and the runner reconciles its own two lists, the shell
array and the python `SCENARIOS` table, before it drives anything. So the one drift left for a
reader to check by hand is a row here that no id begins with; a runner with no row, and a row with
two runners, are both refused by the run itself.

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
| 3b | Shutdown drain, no deadlock | 1+2 | Quit **while a fixture is mid-stream with the UI artificially slowed**, so the `UiEvent` channel is full and the async producer is pending in `send().await` on it: the process must still exit within the deadline, the terminal must be restored, and the session log's manifest must be published and self-consistent. This is the regression test for the drain protocol. Its second half drives a **genuinely full** non-blocking screen rather than an injected failure, and what it verifies is exactly two things: the starvation ends the session **bounded** -- exit 1, inside the deadline -- and the `termios` comes back. **It does not verify which road the session left by, and must not be read as doing so.** Whether a full screen refuses a whole vector or takes a prefix of one is the kernel buffer's business and varies by platform and by run, and the failure the session reports carries no route tag: `FrameFailures::failed` hands back the first error of the run and nothing about how many there were (`src/tui/event_loop.rs`), so neither the presence nor the absence of any particular wording in this row's capture is proof of a budget expiry. The two roads are proven where they can be proven deterministically instead: the budget's own semantics by `FrameFailures`' unit cases, and the partial road on a release binary by row 3c below. One Darwin trace of this row happened to show a prefix of 1,024 bytes; that is an observation of that run's kernel buffer, not a cross-platform capacity and not a claim this row asserts |
| 3c | A terminal that takes part of a frame | 1+2 | **Implemented** as `3c-partial-frame-containment`. The one failure a refusing screen cannot stand in for: every other restoration row fails a write that delivered **nothing**, so the terminal is where it was and the vector may be offered again, while here the terminal has really taken part of a synchronized frame. A `fault-injection` build's sink takes half of the first band frame onto the real pseudoterminal and then fails; the release build has neither the fault nor a way to ask for it. **The discriminator is split, and deliberately.** This session dies on the first band frame -- before a prompt can be typed and before a turn exists -- so the three-part nonce discriminator is driven in full against a **positive control**: the same fixture, geometry and profile with nothing injected, which renders this scenario's own marker and proves the setup really works. The torn session is then discriminated **against that control**: what the terminal took is asserted to be a real, incomplete **prefix of the very frame the control's launch paints**, the count the product reports is compared against the harness's own count of the bytes on the wire, exactly one frame is on the wire (the torn one was never offered again) and no frame was ever completed. The exit is then asserted whole: the process leaves with its own error, the restore goes out, the cleanup below it is still written -- the skip belongs to an exit whose *first* segment is taken in part, which a prior torn frame is not -- and `termios` is byte-identical. **The boundary this row does not cross**: `termios` equality is a `tcsetattr` fact and the restore is a byte fact, and neither says what a terminal made of an incomplete vector. This row proves **containment** -- the session terminates, nothing is re-offered, the line discipline comes back -- and claims **no parser recovery** and no restored screen. Nothing here passes by absence |
| 4 | Raw mode positively entered | 3 | `ECHO`/`ICANON`/`IEXTEN`/`ISIG` clear, `VMIN=1`, `VTIME=0`, mouse tracking absent |
| 5 | Editor basics | 2 | Type, arrows, Home/End, word moves, Backspace/Delete; the composer grid matches the typed text; grapheme motion moves a ZWJ family as one unit |
| 6 | Soft wrap and growth cap | 2 | A long paragraph wraps word-aware with hanging spaces; the composer stops growing at `content_bottom/2 + 1` |
| 7 | Multiline and paste **framing** | 1+2 | Shift/Alt+Enter and `\` continuation insert a newline. `?2004h` is enabled. A pasted block containing **embedded newlines, a `0x03`, and an ESC sequence** produces **exactly one** prompt: assert the fixture server received one request whose body carries the whole pasted text, that no turn was cancelled, and that the ESC was not decoded as a key. A paste over 1000 codepoints collapses to `[Pasted text #1, N lines]` on screen and expands verbatim on submit. Placeholder *atomicity* under cursor motion and delete is **Phase 2** ([`03-tui-port.md`](03-tui-port.md) §"Phase 1 and paste") and is not asserted here |
| 8 | Streaming render | 2 | The marker text arrives progressively across frames; final grid contains it exactly once; no SGR leaks past the answer (assert a plain-attribute cell after it) |
| 9 | Activity row | 2 | `• Thinking` with elapsed appears while the fixture withholds output, and the clock **freezes** while an approval is pending; the row's own literal foreground is asserted for **both** palettes (dark `38;5;252`, light `38;5;238`, matching upstream's `permission_auto_style` grey — `render.zig:55,86,105` — under xfx's own neutral name) and a plain-attribute cell below it proves no colour leaked past the rule (assert cell attributes, not a log line) |
| 10b | Approval mid-turn does not deadlock | 1+2 | With a turn in flight and a submit already queued in `TurnWork`, answering the approval must still take effect — proving the answer travelled on `TurnControl` and not behind the queued prompt. A second submit while one is queued must be **rejected with a visible notice** and must leave the composer text intact |
| 10 | Approval panel | 2 | A mutating tool call in `ask` renders the 3-choice panel with the correct "always" wording; `1`/`2`/`3`, ↑↓, Esc and Ctrl-C each produce the right outcome; the fixture server sees the tool result that the choice implies |
| 11 | Ctrl-C as a byte | 1+3 | `0x03` cancels a running turn; a second exits 130; terminal restored in both |
| 12 | Theme detection | 1+2 | OSC 11 queried at start; a dark and a light fixture response each select the matching palette (assert cell attributes, not a log line) |

**Phase 2 — surfaces**, and the Phase-3 rows that have shipped: the table is the registration's own
order, and a shipped row listed under a phase that has not finished would be a row nothing runs.

| # | Scenario | Oracle | Passes when |
|---|---|---|---|
| 13 | Cell diff correctness | 2 | **Implemented** as `13-cell-diff-correctness`. After the diff replaced full-band repaint, the rendered grid is **identical** to the Phase-1 full-repaint grid for the same input — the diff is an optimization and must be observationally equivalent |
| 14 | No-op frame skip | 1 | **Implemented** as `14-no-op-frame-skip`. A tick with nothing pending emits zero bytes |
| 15 | Resize | 2 | **Implemented** as `15-resize-reflow`. SIGWINCH at several widths, including mid-stream; content reflows, no stale rows, band geometry recomputed; a resize during an approval does not lose the pending decision |
| 16 | Slash menu | 2 | **Implemented** as `16-slash-menu`. `/` opens the picker in the rows a question would take, above the divider, with the composer still holding the caret; ranking is exact-prefix > alias > substring, ties in the order `/help` lists them. **`/exit` supplies the alias tier**: it is a real `SLASH_REGISTRY` alias for `/quit` rather than a test fixture, so `/e` -- which names no command -- must rank `/quit` above every command that merely contains an `e`, and the row must say which name put it there. A word that names nothing lists nothing. Esc dismisses without arming the composer's own double-Escape clear, and the dismissal survives further typing until the trigger kind changes; Tab completes, with a trailing space for the command that takes an argument (asserted through the caret, since a space is not a readable cell), and the completed command runs on one Return without reaching the fixture server |
| 17 | Prompt history | 2 | **Implemented** as `17-history`. Two nonce prompts are submitted and a third line is typed and never sent. ↑ from the draft's first visual row recalls the newest submitted line and `C-p` reaches the one before it -- which is the whole point of there being two key families: the arrow is the composer's own row movement everywhere else in a multi-row draft, and `C-p`/`C-n` are the recall wherever the caret is. A further `C-p` at the oldest entry is refused rather than wrapped, asserted through the `C-n` after it because a refusal paints nothing. ↓ at the last visual row hands the captured draft back unchanged, which is the claim a recall that merely replaced the composer would fail. A recalled entry is then edited, and the next `C-p` starts at the newest entry again while the `C-n` after it returns the **edited** text -- the edit-resets-the-walk contract. Judged on the grid rather than on the wire: the composer row is extracted by `composer_text` from the rebuilt 24x80 grid at every step (`grid-02-history-draft` through `grid-09-history-edited-draft-returned`, written into the evidence directory), and the caret is asserted at the end of the recalled line, since a caret parked in front of it would make the edit an insertion into the middle. The two fixture markers are `run.marker(...)` values that appear in no prompt, so an echoed line can never satisfy a wait for one, and `len(fixture.bodies()) == 2` closes it: no recall sent anything. 17 checks. |
| 18 | `/setup` provider switching | 2 | **Implemented** as `18-provider-switch`. With a fake gateway **and** a fake llmux both up -- which is the whole design, because with one fixture running "the prompt went to the other provider" and "the prompt went nowhere" are the same observation -- `/setup llmux` switches, and the **next prompt reaches only the newly selected fixture** (each fixture has its own marker, and the one that was switched away from must record zero requests). The screen is not the evidence on its own: the profile is **read off disk** and must agree, carry the url of the daemon that was actually probed, and still hold the model chosen for the provider that was left. The scenario waits on the line the switch reports *after* its reload, so what it observes is the configuration that was re-read rather than what the write intended. Evidence: `switched`/`answered` grids plus `llmux-requests.jsonl`. |
| 19 | `/model` catalog and context meter | 2 | **Implemented** as `19-model-catalog-and-context-meter`. A bare `/model` reports the model and provider at once and hands the catalog load to the runtime thread; the rows arrive afterwards carrying the two columns a daemon publishes. **Both shapes are driven**: a model with a window and an effort list, and one with neither -- the second is what makes `context=unknown efforts=none` reachable, so the browser is shown to render an absence as an absence rather than only the happy case. The meter is the same claim twice: the catalog alone is a denominator with no numerator and the hint row must say **nothing** about context; one completed turn supplies `input_tokens` and the row then reads `Context: 12k/1000k 1%`, exactly once. A layer that outranks the profile is reported by scenario 18's surface and by `tests/tui.rs`'s `the_catalog_browser_lists_context_and_effort_and_names_what_outranks_the_write`, which drives `XFX_MODEL` over a `/setup` write. Evidence: `before-any-turn`/`browsed`/`measured` grids. The **catalog-membership refusal** has its own real-pty receipt in `cargo test` rather than a scenario of its own, because what it turns on is the wire and not the screen: `tests/tui.rs`'s `a_model_the_daemons_catalog_does_not_publish_is_refused_and_the_next_turn_keeps_the_old_one` browses a fake daemon's catalog, types an id it does not publish, reads the refusal off the band, and then asserts the `model` field of the request the next turn actually sent. |
| 20 | Alt-screen approval | 2 | **Implemented** as `20-alternate-screen-approval`. A change whose before or after side is longer than the 160-byte summary takes the alternate buffer -- a property of the change, not of the terminal's height -- and the diff is asserted **only** on that plane while the primary plane's rows are asserted unchanged behind it. The answer gives the plane back, and every snapshot from the leave to the repaint carries the band's rows, so there is no intermediate blank grid. **Driven through both content mutations in one session**: an `edit_file` whose two strings outrun the summary, and a `write_file` that replaces the whole file -- whose "before" is the file that is there rather than a string the model sent, and which is the largest change this product makes. The second is not a repetition: the pair has to balance **per question** (entered twice, left twice), and the two halves discriminate each other because the edit's screen shows `alpha`/`beta` and the write's shows `beta`/`gamma`, so a screen carrying the wrong change fails on a named cell. Each half also asserts the file on disk is untouched while the question is up and is exactly what was approved afterwards. **A third turn drives the collision the escaping has to survive**: the file's own text with every line break spelled out as a backslash and an `n`. Those are two different files -- twenty lines against one -- and under an escaping that spent a backslash without escaping the backslash itself both sides rendered as the same string, so the screen showed the whole change as a no-op. What the scenario asserts is not that the two sides are different text but that they are different **shapes**: the side whose breaks are real gives each line a row of its own, and the side that only spells them is a wrapped run carrying the doubled literal, asserted on the rows under the `after` heading so the band's own summary of the same payload cannot satisfy it. Because the review is line-for-line, the far side of a twenty-line change is off the first screenful, so each half now also **walks** it with `C-n` to its clamped end and asserts the last lines there -- which is what the viewport exists for. A **fourth turn** drives a payload of 161 `ESC` bytes, delivered by the fixture rather than typed -- a raw control typed at a real terminal is an escape sequence to the input decoder rather than content, so driving it from the keyboard would measure the harness. Two claims the grid can make and a unit test cannot: the byte is **named by its code point** on the screen (`\u{001B}`, not one symbol standing in for every control), and the emulator finds no sequence it does not know, which is how "a hundred and sixty-one escape bytes reached the review and none of them reached the terminal" becomes a measurement. The pairwise claim -- that two *different* controls stay two different screens -- is proven in `cargo test` over the whole 65-scalar control domain at the permission boundary and pairwise at the renderer, because driving it here would cost two more approval turns to say less. The one-write invariant behind the restore is held by a counting writer seam in `cargo test`, because a sampled snapshot at a frame boundary cannot see a restore split in two |
| 21 | Paste placeholder atomicity | 2 | **Implemented** as `21-paste-entities`. Backspace at the placeholder's right edge removes the **whole** block -- the composer is empty afterwards rather than holding a damaged name -- and the block it removed is provably not on the wire; cursor motion steps over the summary as one unit, asserted through the **caret's own cell** as well as the row, because a caret left inside the name paints the same row; and recalling the line through history hands it back under a number this session has not used (`#3` after a `#1` a backspace took and a `#2` that was sent), with the **paste** on the wire rather than the words it looks like. A number a block was given is never given again, which is why the second paste is `#2`. The **undo boundary is a cargo receipt** here and a key in scenario 22: `tui::shell::tests::one_framed_paste_is_one_transaction_however_many_reads_it_arrived_in` asserts that one framed paste records exactly one `DeltaKind::Paste` entry whatever the bytes and however many reads they arrived in, that the keystroke after it is its own entry, and that two `C-_` presses take the keystroke and then the whole paste. `C-z` is still not undo on this surface, so no scenario drives one. Evidence: `collapsed`/`backspaced`/`stepped-over`/`answered`/`recalled` grids plus `gateway-requests.jsonl`. |
| 22 | Edit history and kill ring | 3 | **Implemented** as `22-edit-history`. `C-w` takes the last word and loads the **one** kill slot; `C-y` puts it back at the caret; `C-_` (`0x1f`) undoes the **yank** and the caret goes back with the text. **Redo is driven too, in both pinned spellings**: it has no control byte upstream (`shortcuts.zig`'s table has no arm for one), so Super+Shift+Z reaches a session as a CSI sequence, and a sequence is bytes -- `ESC[122;10u` and `ESC[27;10;122~` (`runtime.zig:3018,3025`) are written into the pty, which is exactly what a terminal speaking either protocol would write. Each spelling is proved on its own round trip -- redo, assert the composer text **and** the caret column off the emulator's own cursor, then undo again -- so neither rides on the other's work. What the scenario deliberately does **not** claim is that a given terminal emits those bytes for that chord: that is a claim about terminals, and only a receipt from one can make it. Four sequences are then proved inert with a keystroke behind them: the two near misses `ESC[122;1u` (no super bit) and `ESC[122;010u` (a spelling no terminal emits), driven while redo still holds something so that accepting either would show, and `C-z` (`0x1a`). A recorded edit then **clears** redo and the two real spellings stop meaning anything, proved the same way; and a submit is a **boundary** -- neither `C-_` nor either spelling puts the discarded draft back, and the session stays up. The caps, eviction and byte accounting stay in `src/tui/edit_history.rs`'s own cases, which is where a hundred entries can be counted. Evidence: `typed`/`killed`/`yanked`/`undone-yank`/`redone-csi-u`/`undone-after-csi-u`/`redone-csi-tilde`/`undone-after-csi-tilde`/`ctrl-z-and-near-misses-are-not-undo`/`redo-cleared-by-an-edit`/`undone-kill`/`submitted`/`after-the-boundary` grids plus `gateway-requests.jsonl`. |
| 23 | Question panel ordinals and freeform | 2 | **Implemented** as `23-question-panel`. A real `ask_user_question` call from the fixture puts **one** question of a two-question batch on the screen at a time, by ordinal, with the synthetic `Other` slot appended after the model's own options and the first option's description on its row; the title says which question of the batch it is. The batch is two questions rather than one because the result is an **ordered document** and a one-question batch passes whether or not order is kept. A model ordinal (`1`) submits at once; `3` at the freeform slot only **opens editing**, which is asserted through the terminal's own cursor on the draft row rather than through the `> ` marker -- the marker is painted whenever the slot is the marked choice and says nothing about where the next keystroke goes -- and the draft then takes `다시 묻지 마 — später`, whose caret column is checked against `unicode-width`'s measurement of it. The claim the screen cannot make on its own is made on the wire: the **next request's tool result**, correlated by call id on a `tool` message rather than by the tool's name (which is advertised in every request this suite captures), is `[{"question":…,"answer":"Thorough"},{"question":…,"answer":"다시 묻지 마 — später"}]` -- both answers, in question order, unclipped -- and exactly one request carries it. The panel is then gone from the grid the marker lands on. Evidence: `panel`/`second`/`freeform`/`answered` grids plus `question-requests.jsonl`. 21 checks. |
| 23b | Question cancelled, and a question interrupted | 1+2 | **Implemented** as `23b-question-cancelled`. **Escape declines the batch**: the panel comes down, the byte-exact `(user cancelled the question)` is what the next request carries as that call's result, once, and the turn carries on to the fixture's own marker. **Ctrl-C is a different key and a different claim** -- it stops the turn the batch belonged to -- so it is driven against a completion that asks **and then writes a file**, twice: once answered, where the write behind the question is observed to run and the turn asks for its own conclusion, and once interrupted, where the file never appears, the interrupted turn asks for **nothing more** (one captured request, and the fixture answers an unscripted one with a 500), and a **fresh prompt typed afterwards is answered** -- which is the positive half of "the turn stopped", since a requester still parked would fail there rather than at any absence. A file that never appears proves nothing about an interrupt unless the run that was not interrupted wrote it, which is what the answered half is for. Evidence: `cancelled`/`answered`/`interrupted` trials' grids plus their three request captures. 22 checks. |
| 24 | Readiness gate on an approval | 2 | **Implemented** as `24-approval-readiness`. An affirmative is answerable only after a frame that really disclosed *this* request was written and reconciled. **Positive control first**: the committed grid carries the title, the three answers and the always-scope *whole* -- including its tail, which is the half a fixed row allotment used to cut -- and `1` is then taken, evidenced by the answered call's tool result on the wire rather than by the panel disappearing. **Then the revocation**: a resize while the question is up drops the receipt, the same `1` is refused with `the question is not on the screen yet` on the screen, the file on disk is still the pre-edit content, and the question is left standing with its refusal visible; once the repaint for the new size has landed the same key lets the edit through and the file changes. Synchronized on the committed grid throughout, so no step is timed. **What it does not prove**: the pre-commit case, where a key is pressed strictly between the request arriving and its first frame -- there is no seam for it here and the tool call is asynchronous, so that boundary is proven by the deterministic `commit_band` cases in `cargo test --lib tui::event_loop` instead. Evidence: `disclosed`/`revoked` grids plus both trials' request captures. 9 checks. |

| 25 | An amended approval | 2 | **Implemented** as `25-amended-approval`. A decision the user amended, and where the sentence goes -- three claims no unit test can make together. **Trial A, allowed:** Tab opens the draft under `1. Yes`, asserted through the terminal's own cursor on the row beneath the answer rather than through the `> ` marker (the marker is painted whenever the choice is marked and says nothing about where a keystroke goes), the draft takes a phrase whose caret column is checked against `unicode-width`'s measurement, and the panel still shows all three answers and the whole always-scope beside it -- a draft is paid for out of what is left, never out of the disclosure. Enter then answers, and the file on disk holds the model's **own** `content` byte for byte: an amendment is context, never an argument. The wire carries the rest: the first request bearing that call's result also bears the phrase as a `user` message, exactly once, **after** the result (ordered by walking the request's parts, because upstream merges consecutive user messages and hoists results to the front of the merged one), and the phrase is **not** inside the result's own `output` -- a sentence merged into the tool's report would read as the tool's own account of what it did. **Trial B, refused:** the marker walks to `3. No`, Tab opens *that* side's draft, and a second phrase is submitted with Enter; the target does not exist, the result the model is shown says the call was not permitted, and the second phrase follows it under the same ordering. Both submitted phrases are then asserted **on the answered/denied grid**, not only on the draft grid: the sentence is read back in the transcript on a `[you]` row when the runtime reports the delivery, so what the user sees and what the model was told cannot disagree. **Trial C, interrupted:** a filled draft plus Ctrl-C, then a fresh prompt -- and the phrase typed at the interrupted panel appears in no captured request **and on no grid after the panel came down**, though it was on the screen before it (`grid-01-filled`), which is what makes the absence an observation rather than a phrase that was never rendered. That is the leak test at release level, on all three copies. The three phrases are distinct and appear nowhere else in the harness, so a grep across the evidence directory separates "the sentence reached the wire" from "the sentence was echoed on the screen": the two submitted ones are in a request capture *and* on a grid, the interrupted one is only ever on a grid. `termios` is asserted raw while a draft has the keys and byte-identical to `before` after each session exits 0. Evidence: `draft`/`answered`/`deny-draft`/`refused`/`filled`/`after-interrupt` grids plus the three request captures. 33 checks. |

| 26 | Layout convergence: document and draft survive growth/undo, and the band's steadiness survives a panel opening and closing | 2+3 | **Implemented** as `26-layout-stabilization`. Three ordered document lines are answered, then a paste grows the draft to three rows and an undo (`C-_`) shrinks it back to one -- both read off combined screen+scrollback (`Grid.document_text()`), so a line the growth or shrink carried into scrollback is not missed -- before a separate turn opens a real inline approval panel (`edit_file`, denied), whose own growth and close are asserted not to disturb the earlier lines either. All four markers -- the three document lines and the turn's own -- survive once each, in order, across the whole session. The session then clears its draft, and a timed idle watch confirms the screen writes nothing for the settle window before `C-D` and a byte-identical `termios`. 35 checks. |
| 26b | Layout convergence under a held reply: the composer really yields rows to a panel | 2+3 | **Implemented** as `26b-composer-yield-to-panel`. An `edit_file` call's reply is held behind a `release_when` gate while the request is confirmed captured and still unanswered -- an active turn, not a settled one -- and a ten-row draft is pasted and read off the committed grid during that hold, so the panel's later growth is read against a composer the harness knows was already tall rather than one raced into being tall. Releasing the reply opens the real panel: the composer's visible rows drop from ten to six with the draft's own tail shown unedited, and the activity/title rows are located on the grid rather than assumed from divider position. All three choices and the always-scope tail are asserted; a deny then restores the full ten-row draft unedited, and `termios` is asserted before and after. 27 checks. |

**Phase 3 — depth.** Undo/redo and kill-ring behavior (2, row 22), question panel ordinals and
freeform (2, rows 23 and 23b), the readiness gate (2, row 24), an amended approval (2, row 25) and
layout convergence (2+3, rows 26 and 26b) are implemented and registered above. Rows 26/26b are a
release-binary receipt: ordered markers survive across combined screen+scrollback through a real
panel's growth/close (26), and a held-reply composer really yields rows to that panel and gets them
back (26b). The exact-same-instant idle-replay proof -- zero bytes, unchanged geometry after every
transition -- is a separate in-crate test,
`document_and_band_transitions_settle_without_reemitting_document_rows` in `src/tui/event_loop.rs`,
not something either PTY row drives, and neither substitutes for the other
([`03-tui-port.md`](03-tui-port.md) item 22). Still specification: commit self-check **recovery**
under an injected partial write (1+2) -- the containment half of that row is driven by 3c above, and
recovery is the half that is not; live theme switch re-tints the transcript (2).

**Theme monitor status, beside the row above.** The mode-2031 monitor half itself (paired
enable/restore; a SIGCONT arms an outbound `?996n` query delivered on a checked counted paint tick,
and the `997;n` reply is decoded before focus and consumed on input) is implemented locally and not
published -- see item 23 in [`03-tui-port.md`](03-tui-port.md) -- and theme-specific native-PTY
tests exist for it in `cargo test`, but neither is a row in the tracked scenario table above: that
table (31 scenarios, 659 checks + oracle 52 = 711) is a **regression** suite, and its green says nothing about this
monitor. Scratch QA covering theme is a separate effort from this tracked regression suite and does
not register or close the theme scenario here.

**The self-check's first half now exists in the working tree, and it is deliberately not a scenario
here.** The per-vector output preflight of item 21 in [`03-tui-port.md`](03-tui-port.md) refuses a
vector *before* it is written, and what proves it is `src/tui/check.rs`'s own cases plus the tamper
cases in `src/tui/frame.rs` and `src/tui/term.rs`, where a real emitter's bytes are altered at a
`#[cfg(test)]` seam and the refusal is observed before a fake writer sees them. That seam exists for
in-crate tests only and is in no binary — including the release binary this harness drives, which is
why the claim cannot be restated as a scenario here. The scenarios above must not be read as covering it. They are a
**positive** regression: the screens a correct emitter leaves, on a real terminal. A mutation test
beside them mutates the **emitter**, which is a different question from what the terminal did with
the bytes — so neither of the two is an injected partial write, and neither closes this row.
**An injected partial write is now driven, and it closes the containment half of the row rather than
the row.** Scenario 3c above puts a real prefix of a real frame on a real terminal and asserts what
this product does about it: the session ends, the vector is never offered again, the exit writes its
own segments and the line discipline comes back. What it does not assert — because no measurement
here can — is that the terminal recovered: an incomplete vector leaves a screen nobody declared, and
nothing re-establishes a frame from a prefix. So what stays planned is the **recovery** scenario, and
that is what this row waits on: a write the screen takes only part of is now measured and contained
rather than unreadable, and no scenario above shows a session carrying on from one.

**A diagnostic record exists beside this and does not move that boundary.** After the same
restoration attempt, a session `event_loop::disposed` ended on either road is written,
independently of the torn screen, as a fixed four-field `last-tui-error.json`
(`src/tui/diagnostic.rs`; item 21 in [`03-tui-port.md`](03-tui-port.md)). It is proven by that
module's own cases and by native, `fault-injection`-gated PTY tests plus one unconditional positive
control in `tests/tui.rs` -- in-crate/native evidence, not a row in the table above, so this suite's
31/711 green is not evidence for it, and adding a tracked scenario for it is still open if this row
is ever revisited. It names which road a session left by; it is **containment's record, not
containment itself, and not the recovery scenario above**.

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
