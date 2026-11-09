---
title: 0.0.16 delta -- AT-FOLDERS round 2 (hold the frame only; no hold under Ctrl-O)
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, input, file-picker]
---

# AT-FOLDERS round 2

Round 1's head `90fbeeb6` was reviewed by two engines (gpt6: one MUST,
PLAUSIBLE; opus: APPROVE, two SHOULDs). The dispatcher's decision: **hold
exactly what fx holds, which is the frame.** fx's lines are quoted in
`../../at-height/fx-src.txt`, which round 2 extends from line 1363; lines
1-1361 are byte-identical to round 1's (checked with `cmp`). `fx-src.txt:N`
below means that file.

| commit | what |
|---|---|
| `9a8f69c8` | RED (`red-unit.txt`): the held turn, Ctrl-O, and the expiry case; the round-1 hold case loosened to let the launch's report through |
| `ce82af2d` | the fix, `docs/parity.md`, fx's quotes -- **the code head** and the gate's commit |
| `b0c4ebfb` | `mutants.py` |
| (receipt commit) | this folder; `../README.md` updated (§3, §10, §11) |

## 1. What a held tick holds (gpt6 MUST, opus §3)

**The defect.** Round 1 skipped the whole commit while a folder listing held
the frame. That held the status report, the title and the bells too, and
`take_program_status` reports only changes (`shell.rs:10715-10723` at
`90fbeeb6`): a turn that started and ended inside a held window reported
neither `working` nor `done`.

**What fx holds.** `loopCommitFrame` skips `flushRequestedFrame` and nothing
else (`main.zig:3007`, `fx-src.txt:1340-1361`). That function writes nothing
while a handoff is pending and takes the active surface's requests, the full
transcript's included (`app_render_runtime.zig:924-936`,
`fx-src.txt:1363-1377`). Its wire frame carries the document movement bytes
inside the update (`terminal_diff.zig:601-646`, `fx-src.txt:1378-1425`).
Outside it:
- the program status report and the bell are writes of their own between
  frames (`transcript/runtime.zig:10402-10430`, `fx-src.txt:1469-1499`);
- the title is set by the provider when it changes
  (`app_session_runtime.zig:2958-2987`, `fx-src.txt:1500-1531`);
- the theme query is written while facts are collected
  (`main.zig:2830-2843`, `fx-src.txt:1426-1468`);
- the handoff is settled after the frame, held or not (`main.zig:3009`).

**So xfx, from `ce82af2d`** (`commit_or_hold` → `commit_counted_held` →
`commit_tick` → `commit_frame_led`, in `src/tui/event_loop.rs`):

| on a held tick | xfx | why |
|---|---|---|
| band frame, full-transcript frame | held | fx's frame |
| document rows (`commit_primary`'s appends), a resume's wipe | held | they ride the frame's update, as fx's movement bytes ride its wire frame |
| `/model` column's catalog ask (`settle_column`) | held | the render path's ask, made only for a frame about to be painted; on a held tick the slot is the `@` list's, so it would do nothing anyway |
| program status report | written, in its order | fx: a write of its own |
| window title | written **on its own** (`write_title_alone`: the band's own `OSC 2` bytes, checked as a new `Intent::Title`, then `Band::title_landed`), in the order the frame's lead would have had it | fx: the provider's write; xfx's title is otherwise frame metadata |
| bells | written, last | fx: a write of its own |
| theme query, handoff | written | not fx's frame |
| `tick_committed` | runs | bookkeeping, not a write |

The title on its own is written only on the primary plane with a band on the
screen: that is where an unheld tick writes it too. `paint_alternate`
(`event_loop.rs`) writes no title, and a screen no band fits on writes
nothing (`shell.blind()`).

## 2. No hold under the full transcript (opus SHOULD 1)

fx's hold harvests with `filePickerEligible` (`input_completion_runtime.zig:
821`, `fx-src.txt:1209-1238`), false while the full transcript is up (`:783`,
`fx-src.txt:524-560`), and an ineligible harvest stops the listing and drops
the directory request (`directory_completion_job.zig:140-151`,
`fx-src.txt:1532-1544`). `hold_frame_for_file_listing`'s `in_flight` now has
`&& !self.full.is_open()` (`shell.rs`). xfx does not stop the listing: its
rows land and refresh the list behind the transcript, which shows nothing of
it. The full transcript is the one ineligible state the list stays up in that
round 2 checked: an approval and a question take the list down (`ask` and
`ask_question` call `dismiss_picker`, `shell.rs:5071`, `:9751`). The catalog
lends the draft out of the composer, so no `@` word is left [추론, not
traced].

## 3. RED → GREEN (`red-unit.txt`)

On round 1's code (`9a8f69c8`), 2 of 4 fail by assertion:
- `event_loop::a_frame_held_for_a_folder_holds_no_report_title_or_bell`: a
  turn's start and end, a bell, inside a held window, against the same run
  through the unheld commit. Unheld publishes `working`, `done`, `title go`,
  `bell x1`; held published nothing (`left: []`).
- `shell::under_the_full_transcript_a_folder_listing_holds_no_frame`: "the
  transcript's frame waited for the list's rows".
- The expiry case (gpt6 SHOULD 1),
  `event_loop::a_listing_still_out_at_8_ms_lets_the_owed_frame_go_and_its_rows_follow`,
  passes on both: no frame at 0, 4 and 7 ms; at 8 ms the owed frame with
  `indexing files...`; the rows with the frame after they land.

At `ce82af2d` all pass. The held-turn case also checks that the frame after
the hold does not lead with the title again, and
`check::a_title_on_its_own_sets_that_title_and_nothing_else` pins the new
intent: another title, a cell, a bell, a report or the `ST` spelling in the
vector are each refused.

## 4. Mutants (`mutants.py`, `mutants.txt`)

In one `git clone --no-local` at `b0c4ebfb` (the code head's code), under
the suite lock, 900 s each, never in the worktree: **37 run, 37 killed, 0
survived.**
- Round 2's 6: the whole tick held again (item 1; killed by the held-turn
  case), the held frame written anyway, the title not written, the title
  written but not recorded (the next frame led with it again), the new
  intent checking no title, and the eligibility term dropped (item 2; killed
  by the Ctrl-O case).
- Round 1's 31 at this head: 28 killed as written; the 3 whose text round 2
  moved (`loop-never-holds`, `hold-ignores-indexed`, `hold-ignores-loading`)
  are BROKEN there and run re-texted in `mutants.py`, all killed.

## 5. Pairs and sweep: not re-run

The pairs compare the screen's cells at each snap (`cells.py`), and the sweep
is the same comparison over 26 scenarios. Neither change can move a cell at
any snap:
- item 1 changes only what a held tick writes besides the frame -- reports,
  a title, bells: none is a cell -- and, after a hold, whether the frame
  leads with a title already written. The frame that paints the rows is the
  same frame, and every snap is taken after it (each `snap` follows a `wait`
  of 0.3 s or more in the four pair scenarios; a hold is at most 8 ms);
- item 2 changes only when a frame under the full transcript goes out while
  a listing is in flight. No scenario in either set types an `@` word and
  presses Ctrl-O: the seven that press C-o type no `@` word.

So round 1's 23 pairs (`../pairs.txt`, head `v0.1.0-9a83cad`) and sweep
(`../sweep-compare.txt`: 85 → 99 of 157) stand for this code.

## 6. The gate (`gate.txt`)

Every CI step (`../gates.sh`), in a `--no-local` clone at `ce82af2d`, in
one hold of the suite lock, `TMPDIR=<scratch>/atfg2`. **20 steps, 0
failed**, the tree clean before and after (load 10-46 over the run, other
workers' gates beside it):
- fmt and both clippies;
- `cargo test`: 5488 passed, 0 failed; with fault injection: 5525 passed, 0
  failed (round 1: 5484 and 5521);
- the identity, stubs, secrets, preview-contract, preflight-cost and
  usage-clock checks and their self-tests; the fxcap self-tests (68 capture
  tests);
- both release builds; smoke; smoke-tui: 37 scenarios and the oracle, 1168
  checks, 0 failures.

## 7. docs/parity.md

The `@` file picker row: the round-1 sentence that a held tick holds the
whole tick's writes is gone. It now says only the frame waits (the band's or
the full transcript's update and the document rows it carries), and the
status report, the title (then on its own) and the bells go out on their
tick. It also says no frame waits while the full transcript is up.

## 8. Ownership

fx's `isCurrentFileCompletion` check is AT-STALE-SELECTION's (gpt6 SHOULD
2; `../README.md` §11).
