---
title: SUBMIT-CARD-PREVIEW receipt
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, transcript, submit]
---

# SUBMIT-CARD-PREVIEW: fx's first frame after Enter for a tall prompt

- **Unit.** Copy fx v0.0.16's frame that clears the composer for a prompt
  whose card is taller than the room the document has left (fx `70611a91`,
  "A multi-line prompt on a full screen previews its last line until the
  full card is written").
- **Decision.** The ledger cause `submit-multi-card-whole` waited on the
  user. The dispatcher decided it on 2026-11-08: copy fx. It is reversible
  and the user can veto it in the batch review.
- **Base** `58c92ba2` (main). **Commits:**
  - `ee0e526d` RED;
  - `02b6dc81` the change;
  - `70b10aa4` its tests;
  - `93c11c9f` docs, the ledger, the receipt;
  - `4139bd68` two older shell tests updated for the preview's frame (the
    gate's step 04 found them);
  - `e4c678eb` mutants round 1: a redundant slice deleted, two guards given
    tests;
  - round 2: `35d8c94b`/`72d1b68f` (MUST 1, RED then fix), `05a6811b`/
    `1577d48e` (MUST 2 and SHOULD (d)), `d37abb11` (MUST 2's other cases),
    `9f94af7e` (SHOULD (b)), `943d3f79`/`d82556e5` (SHOULD (e)),
    `4db02b1e` (the mutants' equivalent);
  - the rest are this receipt's own commits (no source).
- **Build.** Every `xfx/` capture is from a binary built at `4db02b1e`, the
  last commit that touches source. Row 1 of each `.full` snap says
  `v0.1.0-4db02b1`. Every `red/` capture is from a binary built at
  `58c92ba2` (`v0.1.0-58c92ba`). `fx/` is fx v0.0.16, sha256 `8e6cda2e…`,
  run with `FX_AUTO_UPGRADE=0`.

## What fx does (`fx-src.txt`, `fx-trace.txt`)

`fx-src.txt` quotes every line cited here, by block (F1 to F8).

- **When.** A submitted prompt is a pending submission (`awaiting_frame`).
  The first frame after Enter paints its card's *tail*. When that frame
  commits, the submission is adopted and the card is written into the
  transcript. The next frame shows the card
  (`noteCommittedFrame` -> `adoptPendingSubmission` ->
  `adoptPendingUserPrompt`, `input_submit_runtime.zig:289-307`, `:572-584`,
  `src/main.zig:1234-1243`; F7). Only a prompt submitted with no stream
  open, no output paced and Ctrl-O shut, whose turn start the worker holds,
  gets this path; any other is enqueued and its card waits for its turn
  (`installPendingSubmission`, `input_submit_runtime.zig:1123-1150`; F8).
  xfx's `send` already draws only such a card at once.
- **How many rows.** The tail is the card's newest visual rows, at most
  `available_rows`, and at least one
  (`buildPendingCardProjection`, `app_render_runtime.zig:250-264`; F1):
  - `available_rows = content_bottom - cursor_row + 1 - leading`;
  - `content_bottom` is fixed at `rows - 4`, whatever the band holds
    (`footer_rows = 4`, `src/main.zig:173`; `layoutFromSize`,
    `terminal.zig:47-59`; F2);
  - `cursor_row` is the flow endpoint's row in the last committed frame
    (`runtime.zig:9931`; F6);
  - `leading` is 2 for a mid-line endpoint (the row's end and a blank row),
    else 0 (`pendingCardLeadingAdvanceRows`, `:209-218`; F1).
- **Where.** The frame reserves the tail's rows below the transcript's flow.
  The transcript keeps at least its endpoint row, and scrolls to keep the
  endpoint in what is left (`previewWithPendingCard`, `:373-386`;
  `transcriptAreaBeforePendingTail`, `:2230-2238`; the fixed-point context,
  `:2112-2155`; F4). The tail is painted from the endpoint row plus
  `leading`, cut to the transcript band less the blank row over the activity
  row (`PendingCardPaintContext.init`, `:174-192`; F5).
- **The trace.** fx's own trace for a 60-line first prompt says
  `start=3 paint_rows=32 layout_rows=36 transcript=1..35 body=retain`. Under
  the welcome and a notice it says `layout_rows=34 … body=paint`
  (`fx-trace.txt`).

In xfx's terms, with D the document's last row on the screen (the blank row
a card starts under) and Hc the content bottom of a running band with a
one-row composer:

- the tail is `min(C, max(1, (rows - 4) - D))` rows of a C-row card;
- the band shows `min(tail, Hc - 3)` of them, the newest, with a blank row
  under them;
- the document scrolls by whatever that does not leave room for.

| Pair | D | tail | shown | fx frame 1 |
|---|---|---|---|---|
| `p5-transcript-submit-multi` (3 lines, full) | 35 | 1 | 1 | `┃ multi line three` at row 34 |
| `p5-submit-preview-full` `five` (5 lines, full) | 33 | 3 | 3 | lines three to five at rows 32-34 |
| `p5-submit-preview-full` `image` (badge + 2 lines) | 36 | 1 | 1 | `┃ image line two` |
| `p5-submit-preview-full` `block` (60 lines, full) | 37 | 1 | 1 | `┃ TALL_PROMPT_TAIL` |
| `p5-submit-preview-first` `block` (welcome only) | 2 | 34 | 32 | body 28 to the tail at rows 3-34; the welcome stays |
| `p5-submit-preview-notice` `block` (welcome + notice) | 4 | 32 | 32 | the same rows; the welcome scrolls off |
| `p5-submit-preview-fresh` `block` (after a short turn) | 10 | 26 | 26 | body 34 to the tail at rows 9-34 |
| `p5-submit-preview-fresh` `three`, `p5-transcript-submit` | | all | all | the whole card: no preview |

## What xfx does now

- `Shell::preview_room` measures the room when the card is echoed, against
  the content bottom of the band the last landed frame painted
  (`Shell::band_landed`), as fx measures from its last committed frame.
- `Shell::preview_card` puts the card's newest rows, built by
  `transcript::pushed_rows` from the bytes the card's own push writes, at
  the band's top (`Geometry::previewed`).
- While the band shows them, the document is held: the card is queued,
  recorded once for Ctrl-O, and none of it is written.
- When that band frame lands, `Shell::preview_landed` (from `landed_band`)
  drops the preview, and the same tick writes the card's frame
  (`commit_tick`). That frame leaves the screen the base's first frame
  left; its bytes are not the base's (see the results below). fx builds its card frame
  1-2 ms after the preview's frame commits (`fx-trace.txt`).
- A band solved without the preview's rows (the slash menu, a steer's
  banner) drops it for good (`unplaced_preview_gone`). A question or Ctrl-O
  that takes the other plane gives it up, so the card is paid before
  `1049h`.
- The preview is band rows, so neither a carry nor a resize can move one of
  them into scrollback. A resize drops the preview. The exit's frame writes
  a card that is still held (`flush_paced`, `shut_down`).
- A card is written at once, as before, when it fits, and in each case
  under "Where xfx previews less than fx".

## The claims (`check.py`)

`python3 -I -B check.py <repo> . <side> [red]` reads `fx/`, `red/` (the
base's build) and `xfx/` (`4db02b1e`'s). `frames.py` replays the screens.

- **frame1 / frame2.** The screen after the first and the second frame
  after Enter, fx's against xfx's, cell by cell. Three kinds of row are set
  aside and printed: a banner, the activity row (T7: fx writes `• Thinking`
  with the prompt, xfx once the turn starts), and a turn's wall clock.
- **card.** The frame that writes the card leaves the screen the base's first
  frame left.
- **later.** The frame after that one leaves the screen the base's second
  frame left; whether its bytes are the base's is printed.
- **snaps.** Every snap, `.full` included, is the same on the head and the
  base.
- **whole.** The 60 card rows are in the `.full` history once each, in
  order: no preview row reached scrollback.

Results:

- **RED** (`red.txt`, the base): 16 of its 45 claims fail, every frame1
  of a card taller than its room, at 40x120, 30x100 and 24x80. The two
  pairs that need no preview pass.
  `red-test.txt` is the unit test's failure on the base: frame 1 held
  `PREVIEW-LINE-1`.
- **GREEN** (`green.txt`, the head against fx and against the base): all
  134 claims hold: frame1, frame2, card and later for 18 pairs, 53 snaps,
  and 9 whole histories. In every pair the frame after the card's is the
  base's frame after its card, byte for byte (`its bytes: the base's`, 18
  of 18). In `d82556e5`'s capture three of the 18 differed only in the
  activity row: the head's card frame had taken the activity update the
  base painted in its second frame, and the frame after it was the base's.
  `4db02b1e` changes nothing the activity row reads, so the likely cause is
  when the turn started against the tick; it was not traced further. The
  card's own frame differs from the base's in its bytes: it writes over the
  rows the band gave back instead of scrolling them in (5 bytes more for the
  three-line card). The screen it leaves is the same.

## Mutants (`mutants.py`, `mutants.txt`)

Round 3, at `4db02b1e`: all 22 one-line mutants are killed. Each ran on a
`git clone --no-local` throwaway, never on the worktree, with a 600 s limit,
against the unit's 16 tests (`TESTS` in `mutants.py`). Among them are the
new mutants for MUST 1, the same-tick card frame, the plane barrier, the
landed-frame room and the three room guards that stay.

- Round 2b, at `d82556e5`, 23 mutants: one survived, the room measured after
  the draft went (`round2/mutants-at-d82556e5.txt`). `4db02b1e` deleted the
  equivalent it showed (see round 2 below).
- Round 2, at `e4c678eb`: 17 of 17 killed.
- Round 1, at `70b10aa4`, left three mutants alive, and `e4c678eb` answered
  each one:
  - the band's slice of the preview to its newest rows was redundant, so it
    is deleted, and the mutant moves to `preview_card`'s choice;
  - the hold (`owes_document`, `emit_document_front`) was never reached by a
    frame under test, so a direct test was added;
  - the owed-rows guard was shadowed by `ends_blank`, so a case was added
    that exercises it alone.

## Sweep (`sweep-*.md`)

`p5-transcript-*` and `p5-submit-preview-*` (29 scenarios, 129 snap pairs)
were swept against fx v0.0.16. One sweep used the base build, one the head
build (`4db02b1e`, sha256 `7cbd362d…`). The comparison is
`sweep-compare.txt` (`cmp_sweeps.py`).

- Both sweeps pass 73 of 129; none failed, none timed out.
- 122 pairs have the same verdict on both builds, and none dropped.
- Seven pairs moved inside the pass class, between identical and
  normalized-identical: six toward identical on the head, one away. Each
  differs from fx only in a turn's wall clock (`0s` against `1s`) or the
  activity row's pulse glyph (`•` against a blank), both of which the
  ledger normalizes. Such pairs moved both ways in earlier sweeps too.
- The 53 unresolved pairs are the same on both builds, under the causes they
  had before (`sweep-base.md`, `sweep-head.md`).
- The frames this unit changes are not snaps: no sweep pair holds the frame
  after Enter. `check.py` holds those claims: frame1, frame2, card and
  later.

## Gates (`gates.txt`)

`gates.sh` runs every step of `ci.yml` and the sweep's two test scripts,
one at a time, in one suite-lock slot: fmt, both clippy runs, the full
`cargo test`, both fault-injection runs, every `check-*`/`test-*` script,
the release build, `smoke.sh`, the faulty build and `smoke-tui.sh`. It ran
on `7e4f6d47`, which adds the head's captures. The two commits after it
are docs only. The first adds `gates.txt`, `mutants.txt`, the sweeps and
this README; the three tracked-file scanners then ran on it, outside the
lock, and the second adds their lines to the end of `gates.txt`. Step 00
is `false` and reads `rc=1`.

All 21 steps read `rc=0`. Step 04 ran 5462 tests, step 06 ran 4569 and
step 05 ran 195, with 0 failed in each. Earlier runs: `be6fd7f9` (round 1, all
`rc=0`); `93c11c9f` failed step 04 with two older shell tests that read the
document right after Enter, which `4139bd68` fixed.

## Ledger, docs, references

- `scripts/fxcap/parity-ledger.toml`: the `[[cause]]`
  `submit-multi-card-whole` is deleted, and so is the note that pointed to
  it. Nothing of it remains: fx's frame 1 is copied, and the activity row
  under it was never this cause's (T7 sets it aside). The cause named the
  snap `frame1` of `p5-transcript-submit-multi`, which no sweep takes. Its
  rows are `check.py`'s `p5-transcript-submit-multi submit frame1`. That
  claim fails on the base (17 rows, `red.txt`) and holds on the head.
- `docs/parity.md`, the full-screen TUI row: the card in the frame that
  clears the composer, the preview, and the cases written at once.
- `repoint.py`, `repoint.txt`: 17 `src/tui/*.rs:N` references this unit's
  lines moved, re-pointed in the ledger, `scripts/fxcap/capfile.py` and
  `tests/tui.rs`, each one's text the same at both numbers.
- The receipts that name the cause (`transcript-receipts`, `train-*`,
  `width-disputed`, `tc-notation`) are records of their own commits and
  are left as they were.

## Where xfx previews less than fx

fx's pending path is shut only by an open stream, paced output, Ctrl-O, or
a turn start the worker will not hold (F8). xfx also writes the card whole
at once in the cases below. That is a narrowing, xfx doing less than fx:
no capture of fx's frame for them was taken. The dispatcher has queued
SUBMIT-PREVIEW-EDGES to capture them and copy fx there. Each case, with
the fx line it differs from:

- **A row that is not blank.** A card under a line that does not end on a
  blank row: fx measures from a mid-line endpoint with two rows of
  separator (`pendingCardLeadingAdvanceRows`, F1 :209-218).
- **A fork's line.** A card after a fork's `original session` line
  (`raw_gap`).
- **An open tool group.** A card under a tool group still open
  (`group.drawn()`).
- **Rows still owed.** A card submitted while the document still owes
  rows, a reset's welcome among them. The departing-document guard was
  deleted, because `owes` covers it.
- **No room.** A screen of 8 rows or fewer, whose running band leaves the
  preview no row (`room` 0). fx may still paint one row at its band's bottom
  (`PendingCardPaintContext.init`, F5 :185-187).
- **The band loses the room.** The band loses the room before the preview's
  frame: the slash menu opened by a key read with the Enter, or a steer's
  banner. The card is written whole that frame, and the preview is gone for
  good (MUST 1).
- **A resize** before the preview's frame. The preview is dropped.
- **The other plane.** A question or Ctrl-O takes the other plane before
  the preview's frame. The card is paid whole before `1049h`. fx paints no
  pending card while the alternate screen owns rendering (`:1264-1268`).
- **The exit.**

## Round 2 (both engines: APPROVE-WITH-FIXES)

Every claim below rests on a file in `round2/` or on a test named in it.

- **MUST 1: a preview that did not fit came back.**
  - RED `35d8c94b` (`round2/must1-red.txt`). Two shell tests: the slash
    menu opened by a key read with the Enter, and a steer's banner after the
    turn started. Each took the band's room, the card was written whole,
    and when the room came back the old card's tail came back with it.
  - Fix `72d1b68f`: `refit` and `refloat` drop a preview that the band was
    solved without (`unplaced_preview_gone`).
- **MUST 2: a painted preview across resizes, refused writes, Ctrl-O and a
  stream.**
  - A `Terminal` with a history, in the event_loop tests, reads every byte.
    It resizes as tmux does, as measured in `round2/tmux-resize-probe.txt`:
    rows under the cursor go first, then rows off the top into the history.
  - RED `05a6811b` (`round2/must2-red.txt`). The preview's tick wrote only
    the preview. A 60-line first prompt's 32-row preview, then a shorter
    screen, left 14 preview rows in the history ahead of the card.
  - Fix `1577d48e`. The card's frame now follows the preview's in the same
    tick, as fx builds it 1-2 ms after its preview's frame commits
    (`fx-trace.txt`), so a resize never finds the preview's rows with the
    card unwritten.
  - `d37abb11` covers four more cases: the preview's frame refused, the
    card's frame refused, the preview's frame torn, Ctrl-O read with the
    Enter, and an answer starting with the preview. In each, the card is in
    the history once, in order.
  - **Found outside this unit.** Under a tmux shrink, the band's repaint
    covers the document's last row, with no preview anywhere: a one-row card
    under a filled screen loses its row. The tall-resize test allows that
    one row, and says so. It is the base's resize, reported to the
    dispatcher.
- **SHOULD (a).** The list above is now complete, and `docs/parity.md`
  matches it.
- **SHOULD (b)** (`9f94af7e`). The `departing` guard is deleted (`owes`
  covers it). A test pins the three guards that stay: an open tool group, a
  reset still owed, and the 8-row screen whose room is 0.
- **SHOULD (c).** The `first` and `full` scenarios were captured at 24x80
  and 30x100 as well. Every frame1/frame2/card/later claim holds there, so
  the room's last term, the running band's content bottom less three, is
  now measured against fx at three heights.
- **SHOULD (d)** (RED `05a6811b`, fix `1577d48e`). A question that takes
  the plane before the preview's frame used to leave the card held under
  it. The barrier now gives the preview up and pays the card before
  `1049h`.
- **SHOULD (e)** (RED `943d3f79`, `round2/e-red.txt`; fix `d82556e5`).
  `p5-submit-preview-oneread` sends five typed lines and their Enter in one
  write. fx previews one row, measured from its last committed frame
  (`runtime.zig:9929-9932`); xfx showed three. `Shell::band_landed` now
  records the content bottom of the last landed band, and the room is
  measured against it.
  - A paste followed by an Enter in the same read is refused on both
    binaries, so the scenario types its lines instead (`frames.py` also
    skips fx's cursor-probe update, which moves no cell).
- **The mutants' survivor** (`4db02b1e`). At `d82556e5` one mutant lived:
  the room measured after the draft went, not before
  (`round2/mutants-at-d82556e5.txt`). Once a band frame has landed it is an
  equivalent: the room reads that frame's band, which the draft's going
  does not change. Before one lands (at start, or after a resize) the room
  falls back to the band of the moment, and no frame has shown the
  composer either measurement would read. `send` now measures the room
  where it echoes the card, and the mutant is deleted.

## Local paths in the captures

The image scenarios' card badges link to the run's file, so the captures
held `<scratch>/fxcap.<run>/work/shot.png` as an OSC 8 target. `<scratch>`
is the harness temp folder. The target is a wire byte, and no cell shows
it.

- **The rewrite.** Every capture in `fx/`, `red/` and `xfx/` was rewritten
  after capture, all three sides alike, by `rewrite_root.py`. Each
  `<scratch>/fxcap.<run>/` became `<work>/`, rewritten on bytes so every
  other byte is unchanged. The script builds the root from `$TMPDIR` when it
  runs and holds no path of its own.
- **What changed.** It rewrote 78 links in 58 files and left none
  (`rewrite.txt`).
- **What did not change.**
  - `check.py`'s output, all 69 claims and the base's 7 failures, is byte
    for byte what it was before the rewrite.
  - Round 2 ran it again twice: after the round-2 captures
    (`round2/rewrite.txt`, 143 links) and after `4db02b1e`'s
    (`round2/rewrite-at-4db02b1e.txt`, 57 links in 47 files). None was
    left either time, and `check.py`'s 134 claims printed the same before
    and after each.
  - So is `cells.py`'s output over every image pair, and over the two block
    pairs whose captures held a link, snaps and frames alike.
  - The rewritten wires, replayed, give the rewritten screens: 45 of 45
    (`rewrite-check.txt`).
