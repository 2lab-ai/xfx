# T2 receipt: a notice written while an answer streams

Two scenarios, captured on both binaries:

```
FX_BIN=<fx c1db919 build, 0.0.13> scripts/fxcap/capture.sh fx  notice-midstream 40x120 <dir>
                                  scripts/fxcap/capture.sh xfx notice-midstream 40x120 <dir>
FX_BIN=<fx c1db919 build, 0.0.13> scripts/fxcap/capture.sh fx  notice-midstream 24x80  <dir>
                                  scripts/fxcap/capture.sh xfx notice-midstream 24x80  <dir>
FX_BIN=<fx c1db919 build, 0.0.13> scripts/fxcap/capture.sh fx  streaming        40x120 <dir>
                                  scripts/fxcap/capture.sh xfx streaming        40x120 <dir>
```

- `scripts/fxcap/scenarios/notice-midstream.cap` (new) sends one prompt; the
  fake gateway answers it with `notice-midstream-turn.json`, 53 chunks 100 ms
  apart: `Opening line of the answer.\n\n`, then one paragraph
  `MARKER-A word10 … word19 word100 … word139` one word per chunk, then
  `\n\nClosing line of the answer.`. Two seconds in -- inside the paragraph --
  it types `/notacommand` and Enter (snap `nosuch`), then `/version` and Enter
  (snap `version`), waits for the turn to end (snap `end`) and takes the whole
  history (`end.full`). The paragraph's words are picked so that its first row
  is **exactly as wide as the screen** at both sizes: `MARKER-A word10 …
  word104` is 118 cells behind the two-column gutter at 120 columns, and
  `MARKER-A word10 … word19` is 78 at 80.
- `streaming` is the existing scenario (`slow-line-turn.json`) D2a reported the
  lost cell on: `word32` and `word49` end full rows at 120 columns.

The xfx binary is `target/debug/xfx` built at `8a85323` (this branch with
`plan/tui-phase4` at `e50a8f0` merged; the banner says so). `before/` holds the
same captures from `3894210` (the branch point, before the fix).

Nothing was sanitized: no capture of these scenarios names the run directory.
`.ansi` files are tmux's re-serialization of cell attributes (`capture-pane
-e`), not the wire bytes (see `../c0/README.md`).

## What fx does with a notice that arrives mid-answer

It writes the notice at once, **after the last block the answer finished and
before the one it has not**, and the unfinished one arrives later whole, wrapped
as if nothing had come (`fx-notice-midstream-40x120-end.full.txt`):

```
 5    Opening line of the answer.
 6
 7  ✗ command: Unknown command. Try /help.
 8
 9  * version: 0.0.13
10
11    MARKER-A word10 word11 … word103 word104          <- 120 cells, last one kept
12    word105 word106 … word117 word118
13    word119 word120 … word131 word132
14    word133 word134 … word139
15
16    Closing line of the answer.
```

Mid-stream (`nosuch`, `version`) none of the paragraph is on the screen: fx's
presenter holds a line until its `\n` (`assistant_presentation.zig:207-217`)
and gives it up only at a tool's start, a restart or the stream's end
(`assistant_stream.zig:135-140`, `:300-312`); a notice is an entry appended
beside it (`appendSemanticNoticeAtomic`, `store.zig:1343-1390`). Not in the
middle, not held to the turn's end: above the held line.

## What xfx did before (`before/`)

```
 5    Opening line of the answer.
 6
 7    MARKER-A word10 word11 … word103 word10           <- last cell erased
 8    word105 word106 word107 word108 word109 word110 word
 9
10  ✗ command: Unknown command. Try /help.
11
12    111 word112 word113 … word121
13
14  * version: 0.1.0
15
16    word122 word123 … word135
17    word136 word137 word138 word139
```

Two breaks: the notice made the presenter give up the line it was holding
(`word` | `111`), and a row exactly as wide as the screen lost its last cell
(`word104` -> `word10`; in `before/xfx-streaming-40x120-end.txt`, `word32` ->
`word3` and `word49` -> `word4`, with no notice at all).

## Per snap

| snap | matches | differs, and why |
|---|---|---|
| `notice-midstream-40x120-nosuch` | rows 2-8 and 10-40: card, `Opening line of the answer.`, the refusal row byte-equal (`\e[38;5;252m✗ command:\e[38;5;250m Unknown command. Try /help.\e[39m`), composer, hint row; none of the held paragraph is on the screen on either | row 1, the banner; row 9, the activity row's blink phase and token count (`↓69` / `↓67`): timing |
| `notice-midstream-40x120-version` | rows 2-8, 10 and 12-40: the same rows, and `* version:` in `\e[38;5;250m` on row 9, under the refusal with one shared blank row | row 1; row 9 the version body (`0.0.13` / `0.1.0`), each binary's own; row 11 the token count |
| `notice-midstream-40x120-end` | every row but two: the notices above the paragraph, the paragraph whole on rows 11-14 with `word104` in column 120, `Closing line of the answer.`, the summary `5s (↑8 ↓40)` | row 1; row 9's version body |
| `notice-midstream-40x120-end.full` | the whole history, row for row and colour for colour | row 1; row 9's version body |
| `notice-midstream-24x80-nosuch` | rows 2-8 and 10-24, as at 120 | row 1; row 9 timing (`↓69` / `↓65`) |
| `notice-midstream-24x80-version` | rows 2-8, 10 and 12-24, as at 120 | row 1; row 9's body; row 11 timing |
| `notice-midstream-24x80-end` | every row but two: the paragraph on rows 11-16, `word19` in column 80 of row 11 | row 1; row 9's version body |
| `notice-midstream-24x80-end.full` | the whole history | row 1; row 9's version body |
| `streaming-40x120-mid` | rows 2-4 and 6-40; none of the unfinished line is on the screen on either | row 1; row 5 timing (`2s ↓59` / `3s ↓61`) |
| `streaming-40x120-end` | rows 2-40: `word32` and `word49` end rows 6 and 7 in column 120 on both | row 1 |

## The fix, by break

- **The split** (`src/tui/shell.rs`, `Shell::write_notice` and `run_mark`): a
  notice closed the answer with `close_answer`, which finishes the markdown
  presenter -- so the held line `MARKER-A … word110 word` was written at the
  notice and the rest started a paragraph of its own. A notice now closes only
  the block already written (`close_prose`); what the presenter holds stays
  held and is written under the notice when the presenter lets it go. Marks
  other than a notice (a card, a tool's start, a summary, the turn's end)
  still finish it, as fx's tool start and stream end do.
- **The lost cell** (`src/tui/frame.rs`, `place`): every document row was
  written as `CUP`, text, `EL`. Autowrap is off (`?7l`), so after a cluster in
  the last column the caret stays **on** column 120, and the `EL` erased it.
  The values that disagreed: the caret column the output checker modelled
  after a full row, `cols + 1` = 121 (where its `EL` erased nothing), against
  the terminal's, `cols` = 120. `place` now writes the `EL` only after a row
  narrower than the screen -- fx's own test before its erase
  (`finishComposedInputRow`, `input_presentation.zig:1095-1097`;
  `writeDocumentAppendClear`'s `start_col < target_cols`,
  `terminal_diff.zig:1177-1195`) -- and the checker (`src/tui/check.rs`,
  `erased_from`) erases from the last column for a caret parked past it, so a
  frame that does this again is refused rather than passed.
