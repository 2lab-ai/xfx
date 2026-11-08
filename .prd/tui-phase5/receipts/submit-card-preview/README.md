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
  - then this receipt's own commits (no source).
- **Build.** Every `xfx/` capture is from a binary built at `e4c678eb`, the
  last commit that touches source. Row 1 of each `.full` snap says
  `v0.1.0-e4c678e`. Every `red/` capture is from a binary built at
  `58c92ba2` (`v0.1.0-58c92ba`). `fx/` is fx v0.0.16, sha256 `8e6cda2e…`,
  run with `FX_AUTO_UPGRADE=0`.

## What fx does (`fx-src.txt`, `fx-trace.txt`)

`fx-src.txt` quotes every line cited here, by block (F1 to F7).

- **When.** A submitted prompt is a pending submission (`awaiting_frame`).
  The first frame after Enter paints its card's *tail*. When that frame
  commits, the submission is adopted and the card is written into the
  transcript. The next frame shows the card
  (`noteCommittedFrame` -> `adoptPendingSubmission` ->
  `adoptPendingUserPrompt`, `input_submit_runtime.zig:289-307`, `:572-584`,
  `src/main.zig:1234-1243`; F7). Only a prompt submitted when nothing is
  running gets this path; a queued one is written when its turn starts.
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

- `Shell::preview_room` measures the room before the draft goes.
- `Shell::preview_card` puts the card's newest rows, built by
  `transcript::pushed_rows` from the bytes the card's own push writes, at
  the band's top (`Geometry::previewed`).
- While the band shows them, the document is held: the card is queued,
  recorded once for Ctrl-O, and none of it is written.
- When that band frame lands, `Shell::preview_landed` (from `landed_band`)
  drops the preview. The next frame writes the card exactly as before.
- The preview is band rows, so neither a carry nor a resize can move one of
  them into scrollback. A resize drops the preview. The exit's frame writes
  a card that is still held (`flush_paced`, `shut_down`).
- A card is written at once, as before, when:
  - it fits;
  - the document still owes rows;
  - the document does not end blank;
  - the card follows a fork's line (`raw_gap`);
  - a tool group is open.

## The claims (`check.py`)

`python3 -I -B check.py <repo> . <side> [red]` reads `fx/`, `red/` (the
base's build) and `xfx/` (`e4c678eb`'s). `frames.py` replays the screens.

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

- **RED** (`red.txt`, the base): 7 claims fail, every frame1 of a card
  taller than its room. The two pairs that need no preview pass.
  `red-test.txt` is the unit test's failure on the base: frame 1 held
  `PREVIEW-LINE-1`.
- **GREEN** (`green.txt`, the head against fx and against the base): all
  69 claims hold. In every pair the frame after the card's is the base's
  frame after its card, byte for byte (`its bytes: the base's`, 9 of 9). The
  card's own frame differs from the base's in its bytes: it writes over the
  rows the band gave back instead of scrolling them in (5 bytes more for the
  three-line card). The screen it leaves is the same.

## Mutants (`mutants.py`, `mutants.txt`)

Round 2, at `e4c678eb`: all 17 one-line mutants are killed. Each ran on a
`git clone --no-local` throwaway, never on the worktree, with a 600 s limit.
They ran against the unit's six tests: the RED, the exit, the resize, the
hold, the room formula and `Geometry::previewed`.

Round 1, at `70b10aa4`, left three mutants alive, and `e4c678eb` answers
each one:

- the band's slice of the preview to its newest rows was redundant, so it
  is deleted, and the mutant moves to `preview_card`'s choice;
- the hold (`owes_document`, `emit_document_front`) was never reached by a
  frame under test, so a direct test was added;
- the owed-rows guard was shadowed by `ends_blank`, so a case was added that
  exercises it alone.

## Sweep (`sweep-*.md`)

`p5-transcript-*` and `p5-submit-preview-*` (28 scenarios, 126 snap pairs)
were swept against fx v0.0.16. One sweep used the base build, one the head
build (`e4c678eb`, sha256 `6e313514…`). The comparison is
`sweep-compare.txt` (`cmp_sweeps.py`).

- Both sweeps pass 70 of 126; none failed, none timed out.
- 125 pairs have the same verdict on both builds, and none dropped.
- One pair moved inside the pass class: `p5-submit-preview-full/image-held`,
  identical to normalized-identical. The cause is the activity row's pulse
  glyph (`•` against a blank), which the ledger normalizes. The head's first
  sweep had the same pair identical, and `five-held` moved the other way.
- The 53 unresolved pairs are the same on both builds, under the causes they
  had before (`sweep-base.md`, `sweep-head.md`).
- The frames this unit changes are not snaps: no sweep pair holds the frame
  after Enter. `check.py` holds those claims: frame1, frame2, card and
  later.

## Gates (`gates.txt`)

`gates.sh` runs every step of `ci.yml` and the sweep's two test scripts,
one at a time, in one suite-lock slot: fmt, both clippy runs, the full
`cargo test`, both fault-injection runs, every `check-*`/`test-*` script,
the release build, `smoke.sh`, the faulty build and `smoke-tui.sh`. It runs
on `be6fd7f9`, the commit before the one that adds `gates.txt`; that
commit changes nothing but `gates.txt` and this paragraph. Step 00 is
`false` and reads `rc=1`.

All 21 steps read `rc=0`. Step 04 ran 5451 tests, step 06 ran 4558 and
step 05 ran 195, with 0 failed in each. An earlier run, on `93c11c9f`,
failed step 04 with two older shell tests that read the document right
after Enter; `4139bd68` fixed them.

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

## Not captured on fx

xfx writes these at once, and fx's path for them was not captured:

- a card after a fork's `original session` line;
- a card submitted while the document still owes rows;
- a resize between the two frames.
