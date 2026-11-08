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
- **Base** `58c92ba2` (main). **Commits:** `ee0e526d` RED, `02b6dc81` the
  change, `70b10aa4` its tests, then this receipt's commit.
- **Build.** Every `xfx/` capture is from a binary built at `70b10aa4`. Row 1
  of each `.full` snap says `v0.1.0-70b10aa`. Every `red/` capture is from a
  binary built at `58c92ba2` (`v0.1.0-58c92ba`). `fx/` is fx v0.0.16, sha256
  `8e6cda2e…`, run with `FX_AUTO_UPGRADE=0`.

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
base's build) and `xfx/` (`70b10aa4`'s). `frames.py` replays the screens.

- **frame1 / frame2.** The screen after the first and the second frame
  after Enter, fx's against xfx's, cell by cell. Three kinds of row are set
  aside and printed: a banner, the activity row (T7: fx writes `• Thinking`
  with the prompt, xfx once the turn starts), and a turn's wall clock.
- **card.** The head's second frame leaves the screen the base's first frame
  left.
- **snaps.** Every snap, `.full` included, is the same on the head and the
  base.
- **whole.** The 60 card rows are in the `.full` history once each, in
  order: no preview row reached scrollback.

Results:

- **RED** (`red.txt`, the base): 7 claims fail, every frame1 of a card
  taller than its room. The two pairs that need no preview pass.
  `red-test.txt` is the unit test's failure on the base: frame 1 held
  `PREVIEW-LINE-1`.
- **GREEN** (`green.txt`, the head against the base): every claim holds.

## Mutants (`mutants.py`, `mutants.txt`)

16 one-line mutants ran on a `git clone --no-local` throwaway, never on the
worktree, with a 600 s limit each. They ran against the unit's five tests:
the RED, the exit, the resize, the room formula and `Geometry::previewed`.

## Sweep (`sweep-*.md`)

`p5-transcript-*` and `p5-submit-preview-*` were swept against fx v0.0.16 on
the base build and on the head build: SWEEP.

## Gates (`gates.txt`)

GATES.

## Ledger, docs, references

- `scripts/fxcap/parity-ledger.toml`: the `[[cause]]`
  `submit-multi-card-whole` is deleted, and so is the note that pointed to
  it. Nothing of it remains: fx's frame 1 is copied, and the activity row
  under it was never this cause's (T7 sets it aside).
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
