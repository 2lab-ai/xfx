---
title: 0.0.16 delta -- ARGROW-CACHE receipt (Ctrl-O lays a call's arguments out once a width)
type: receipt
tags: [xfx, phase5, fx-0.0.16, ctrl-o, render-cost]
---

# ARGROW-CACHE: a call's arguments on Ctrl-O, wrapped once a width

Branch `fix/p5-argrow-cache`, from train 6's head `train/p5-1108c` @
`9681e9d8` (T-U4 and CTRLO-WIDER together). The code is `1a9652cc`'s. After it
come only tests (`0c9e1dc9`, `06b4fc4b`, `5e794435`), a comment (`a015ada4`)
and this folder.

| step | ran at |
|---|---|
| the probe | `1a9652cc` |
| the mutants | `0c9e1dc9` (run 1) and `06b4fc4b` (run 2) |
| the gate | `255d3cac` |
| the sweep's head binary | `06b4fc4b` |

`5e794435` makes the shell test's round trips compare raw bytes. It ran
after the gate with fmt, clippy both ways and the 106 tests it touches
(`after-gate.txt`).
The branch merges with the train's later head `99eb8a13` without a conflict
(`git merge-tree --write-tree 99eb8a13 fix/p5-argrow-cache`: exit 0).

The program goal is to copy fx v0.0.16 (pin `481104fd`), which shows a call's
arguments whole on Ctrl-O (`formatExpandedChild`,
`tool_group_projection.zig:786-787@481104fd`, "never clipped"). So every row
stays on screen exactly as before, and only repeated work is removed. No row
changes: no `docs/parity.md` row, ledger entry, registry count or help entry
moves.

| commit | what |
|---|---|
| `11d6d64a` | **RED**: the wrap counter test and the equality test (test only, plus a test-only counter) |
| `a1ff89e7` | the fix: an argument's rows kept unpainted, once a width; an entry's rows held in runs |
| `1a9652cc` | the wraps' exact fast paths: a resize or a first open is one cheap pass |
| `0c9e1dc9` | test: kept rows are the rows `railed` wrote, at every width |
| `e0d1678c`, `47b0d043` | receipt files |
| `a015ada4` | `argument_rows`' comment: when the old width's rows are let go of |
| `06b4fc4b` | test: every one-byte cluster is counted as the general path counts it (mutant W2) |
| `255d3cac` | receipt: mutants |
| `5e794435` | test: the shell test's round trips compared raw |
| `ce5ad894`, then this README's commit | receipt files |

## The problem (train 6, join check 2)

Train 6's receipt (`../train-6/README.md`, "Render cost" (a)): T-U4 cached
Ctrl-O's group block and each heading but not a call's argument rows, and an
entry's layout was keyed on its group's revision, the width and the palette
(`laid_out`). So a call joining the group, a theme switch and a resize each
re-wrapped every member's arguments (`tool_rows`' loop over `railed`). For a
4 Mi U+009B pattern that cost 1.4 s a join (load 72). CTRLO-WIDER's naming
(8 bytes for U+261D's 3) multiplied it for 2 Mi U+261D (finding (b)).

## What changed

**The cache (`a1ff89e7`, `src/tui/full_transcript.rs`).**

- A call keeps each argument as `name: value` once (`CallArgument`, an
  `Rc<str>`, the same bytes the base kept as a `Vec<Argument>`), with its
  rows as last wrapped (`Railed`): ranges of that text at one width, not
  painted.
- `FullTranscript::argument_rows` wraps an argument once for a width. A join,
  a change to another call and a theme switch reuse those rows; a resize
  wraps them again.
- A call's new arguments (`Detail::Arguments` again) are new `CallArgument`s
  with no rows. So rows wrapped for one text are never taken for another's,
  and no key has to name the text.
- An entry's layout is `Rows`: runs, each held once. They are the entry's
  own rows, a cache's (the heading: `Run::Shared`, which the base copied into
  every entry), and an argument's (`Run::Railed`, with the entry's palette).
  `window` paints only the rows the screen shows (`Rows::copy`). A theme
  switch paints the same ranges in the new colours and wraps nothing.
- `railed` (a command's output) is now `Railed::new` and then `Railed::row`
  for each row. `prose::literal_drawn` is `literal_layout` and then
  `literal_piece` for each row (now test-only).

**Why wrapped rows independent of the palette, not only the rows on screen.**
The coordinator gave the choice. The screen's reading position is a count of
absolute rows (`full_transcript/position.rs`: `Reading::scroll_rows`, and
`select_visual_offset(total_rows, ..)`). Opening at the tail needs every
entry's row count, and an argument's row count is its wrap. Laying out only
the visible rows would still pay the whole wrap for the count. What remained
was to make that one pass cheap: `1a9652cc`.

**The one pass (`1a9652cc`, `src/tui/wrap.rs`, `src/tui/prose.rs`,
`src/tui/full_transcript/tool_status.rs`).** With the rows kept, a resize and
the first open still walked every character. That cost 0.89 s for c1 and 0.35 s
for the wider case at `a1ff89e7`, while a join and a theme switch were already
36 ms and 3 ms (`probe-a1ff89e7.txt`, measured by hand before the run below). Each
of these steps is a faster way to the base's own result, not a new one:

- `wrap::painting` reads a cluster through `first_cluster`, whose contract is
  `graphemes(true).next()`. `cluster_cells` returns 1 for one printable ASCII
  character, as its general path does.
- `wrap::single_cells`: a run of printable ASCII after the space, each
  followed by ASCII or nothing, is that many one-cell clusters (the last is
  left out before a byte outside ASCII). `wrap::repeats`: a copy of the
  cluster just found that another copy follows is that cluster again,
  because where a cluster ends depends on what it holds and on the character
  after it.
- `prose::Layout::step_over` steps over such a run after a whole cluster that
  is not a space and lands on the row, when the run does not cross the margin.
  The loop only moves the column for each of them. It never steps inside a
  cluster it cut into characters. `remaining_word_width` and `is_final_word`
  step over ASCII runs, because a line's last word can be megabytes.
  `tool_status::scan_with` counts the same runs at once when they fit the
  rest of the row.
- An argument of megabytes, once the channel has spelled it, is one of these
  runs (`\u{009b}`, `\u{261d}`, `a`), and so is its heading.

**Spelling and naming, once a call.** `detail_spelled` (the spelling and
CTRLO-WIDER's naming of an argument) is called only from `Detail::made_inert`,
on the channel (`grep -n 'detail_spelled(' src/tui/full_transcript.rs`: its
definition at `:495`; two calls in `made_inert`, `:432` (the closure each
argument's name and value goes through) and `:446` (a command's output);
and tests). The layout path is
`tool_rows` → `argument_rows` → `Railed::new` → `prose::literal_layout`, and
then `Railed::row` → `prose::literal_piece`, `clip`. It calls no spelling or
naming function.

**Memory, and its bound.** Per argument shown, the cache holds one width's
rows: 24 bytes a row (`RailRow`), 16 a tab, and no painted row. A painted row
lives only in the frame it is copied into, at most the viewport. The text is
held once (`CallArgument::line`), as the base held it. Rows at width W number
at most cells / (W - 3) plus lines. For the stream's bound (an argument of an
8 MiB completion, spelled at most 4 bytes for each raw byte: 32 MiB, the c1
case) that is about 287 000 rows, 6.6 MiB, at 120 columns, and about 2 M rows,
45 MiB, at the narrowest screen (`layout::MIN_COLS`, 20).

While a resize is laid out, two widths' rows are held: the entry's last
layout keeps the old ones until `laid_out` replaces it. `/trace` lays out
through the same caches at its own width (`traced`), as T-U4's heading does.
That replaces an argument's one width, and the next Ctrl-O paint wraps it
again: one pass, not a wrong row (the key is the width).

The c1 peak (415 MiB, below) is mostly what this unit did not change: T-U4's
heading rows (`Call::heading`, about 284 000 `String`s at 120 columns), its
group block, and the two 32 MiB texts (the call's `shown` and its argument).

## RED → GREEN

- **RED at `11d6d64a`** (`red.txt`):
  `ctrl_o_wraps_a_calls_arguments_once_for_a_width_and_not_again_for_a_join_or_a_theme`
  fails with `after a call joined` / `left: 13` / `right: 7`. Every member's
  arguments were wrapped again for one call's join.
- **GREEN from `a1ff89e7`:** 6 wraps; none again; 1 for a join (the new
  call's); 0 for a theme switch and 0 back; 7 for a resize and 7 back; 1 for
  a call's new arguments. Every layout equals the record laid out from
  nothing. In the gate (`gates.txt`).

## Equality, after every event

`ctrl_o_with_arguments_through_its_caches_is_the_session_laid_out_from_nothing_after_every_event`
(`src/tui/shell.rs`, at `11d6d64a`; from `5e794435` its round trips compare
raw bytes, times included). It passed on the base too: it is the
guard the cache must keep. A turn's calls carry arguments that Ctrl-O lays
out on rows of their own:

- a pattern of words, a long token, a C1 control, U+261D and CJK;
- a command's `cwd` with U+2E3A;
- a note of three lines, one of them empty.

After each event it compares two things with the same session laid out from
nothing, at that width and palette, at once and with Ctrl-O still open. The
first is the screen's record rows. The second is the whole record
(`FullTranscript::all_rows`: every row through the screen's caches, without
moving the reading position). Train 6's join test compared only after a
reopen, and its screen at the tail showed no argument row but the last
call's.

The events:

- open;
- a call joining the open group;
- a theme report while open, and back (byte for byte);
- a resize to 40 columns, compared before anything else, and back to 120
  (byte for byte);
- close and reopen;
- the glob's arguments given again with another pattern.

`rows_kept_unpainted_are_the_rows_railed_painted_at_once` (`0c9e1dc9`) holds
`railed` and kept `Railed` rows against `railed` as `9681e9d8` wrote it. The
kept rows sit inside an entry's runs and are read through a random window.
The test runs over the wraps' corpus, from 1 to 120 columns, in both
palettes, with both gutters.

The fast paths' proofs run over a corpus of what decides where a cluster
ends: marks, selectors, joiners, regional indicators, jamo, a prepended
character, a virama, CR LF and escapes, often repeated, and clusters of over
256 bytes (`wrap::corpus`). Each is held against the code that counts every
unit:

- `painting_reads_the_clusters_graphemes_reads`;
- `a_run_stepped_over_is_the_clusters_graphemes_reads_there`;
- `a_printable_ascii_character_is_one_cell`;
- `stepping_over_what_the_wrap_only_counts_changes_no_row` (both forms,
  every opening, 1 to 34 columns);
- `a_wrap_of_megabytes_of_one_token_steps_a_row_at_a_time`;
- `counting_a_run_at_once_changes_no_break`.

## The numbers

`probe.txt`, `probe-summary.txt`. `render_cost_probe.rs` is train 6's probe
with two additions: a line with three decimals, and a key typed while Ctrl-O
opens (`XFX_PROBE_KEY=1`: Ctrl-O, then PageUp before any frame, then the
frame, timed from the key's arrival). It was appended to `src/tui/shell.rs`'s
tests on five `git clone --no-local`s (`apply_probe.py`): main `690aa505`,
CTRLO-WIDER alone `1824e868`, T-U4 alone `5a7bb465`, the train `9681e9d8`,
and this head `1a9652cc`. The run was release, one case a process, the clones
alternating case by case, two rounds, **the whole run in one hold of the
suite lock** (`run.sh`), load 5-12. Times in ms as `r1 / r2`; "key" is the
key's latency during the open.

**With the call's arguments (`XFX_PROBE_DETAIL=1`), the path every live call
takes:**

| case | build | open | key | join | theme | resize | peak MiB |
|---|---|---|---|---|---|---|---|
| 4 Mi U+009B | main | 601 / 601 | 601 / 602 | 605 / 602 | 602 / 601 | 609 / 623 | 455 |
| | CTRLO-WIDER alone | 638 / 647 | 639 / 643 | 645 / 651 | 640 / 643 | 653 / 651 | 455 |
| | T-U4 alone | 852 / 851 | 858 / 865 | 662 / 674 | 668 / 673 | 841 / 841 | 771 |
| | train | 869 / 866 | 874 / 871 | 658 / 652 | 664 / 662 | 858 / 865 | 777 |
| | **head** | **73 / 74** | **73 / 76** | **32 / 34** | **30 / 36** | **55 / 56** | **415** |
| 2 Mi U+261D | main | 114 / 116 | 116 / 117 | 114 / 116 | 113 / 115 | 116 / 118 | 84 |
| | CTRLO-WIDER alone | 320 / 321 | 325 / 319 | 318 / 326 | 323 / 318 | 332 / 325 | 235 |
| | T-U4 alone | 165 / 167 | 167 / 165 | 115 / 117 | 115 / 118 | 163 / 167 | 139 |
| | train | 364 / 359 | 365 / 366 | 314 / 323 | 320 / 318 | 365 / 363 | 303 / 296 |
| | **head** | **26 / 35** | **26 / 27** | **3.1 / 3.1** | **2.7 / 2.6** | **28 / 25** | **138** |
| 9-byte pattern | main | 0.179 / 0.180 | 0.197 / 0.212 | 0.174 / 0.177 | 0.164 / 0.166 | 0.166 / 0.171 | 9 |
| | CTRLO-WIDER alone | 0.180 / 0.182 | 0.188 / 0.187 | 0.174 / 0.181 | 0.164 / 0.172 | 0.165 / 0.170 | 9 |
| | T-U4 alone | 0.065 / 0.066 | 0.064 / 0.066 | 0.048 / 0.050 | 0.034 / 0.038 | 0.036 / 0.038 | 9 |
| | train | 0.065 / 0.066 | 0.062 / 0.067 | 0.048 / 0.046 | 0.036 / 0.035 | 0.036 / 0.035 | 9 |
| | **head** | **0.065 / 0.063** | **0.064 / 0.069** | **0.045 / 0.044** | **0.034 / 0.031** | **0.029 / 0.028** | **9** |

The key column comes from a second run of each case in the key mode. Its open
equals its key latency to within 5 µs; the other columns are the first run's.

- **The bar** (the coordinator's, from opus's train 6 review): join, theme
  and resize well under 0.1 s for both cases, and the first open bounded.
  The head joins in 32-34 and 3.1 ms, switches theme in 30-36 and 2.6-2.7 ms,
  resizes in 55-56 and 25-28 ms, and opens in 73-76 and 26-35 ms. That is
  below every baseline, main included. Against the train, a join is 19-20×
  faster for c1 and about 100× for U+261D, and the open 12× and 10-14×.
- **A key during the open** waits for the open's one pass (the loop is
  single-threaded): 73-76 ms for c1 and 26-27 ms for U+261D, where the train
  makes it wait 871-874 and 365-366 ms. Once Ctrl-O is laid out, a key costs
  0.010-0.019 ms (PageUp; `probe-summary.txt`, `ctrlo_pageup_ms`).
- **The short argument is not slower:** every Ctrl-O time is the train's or
  less (open 0.063-0.065 against 0.065-0.066, resize 0.028-0.029 against
  0.035-0.036).
- **What is left of c1's join and theme switch (30-36 ms)** is T-U4's group
  block: built again for the group's revision and the palette (`group_block`,
  which spells the 32 MiB target into the group's expanded rows). It is not
  an argument row, and it is unchanged here.
- **Without arguments (T-U4's own probe),** the head is also below the train:
  c1 opens in 66-70 ms against 261-263 and resizes in 43 against 242-247; the
  heading's scan takes the same fast path (`probe-summary.txt`, the `false`
  rows).
- **Peak RSS:** c1 415 MiB against the train's 777, U+261D 138 against
  296-303.
- **The channel's crossing** (`detail_ms`: the arguments' spelling and
  naming, once a call, in `made_inert`) is unchanged: 72-75 ms c1 and
  37-38 ms U+261D, the train's. It is CTRLO-WIDER's naming pass, run once
  where the probe calls it. Live, it runs on the sending side of the channel
  (`bridge::send_ui`).

**Does the change apply to either unit alone?** No. It does not cherry-pick
onto T-U4 alone or onto CTRLO-WIDER alone (`applies.txt`). It extends T-U4's
caches and holds CTRLO-WIDER's named text, so both units are measured above
as baselines only.

**Results at their bound** (`results.txt`, `results_probe.rs`). A result's
rows are still laid out on each layout of its entry. They are bounded by
`RESULT_BYTES` (64 KiB a result). A group of 20 reads, each with a 64 KiB
result, costs 16.6 ms a join and 16.1 ms a theme switch at the head (16.1 and
16.4 at `a1ff89e7`). That is a measured cost proportional to the group's
size, and the brief's subject (argument rows, which have no bound) is not
part of it.

## Mutants

`mutants.py`, `mutants.txt`. Each mutant is one textual change on a
`git clone --no-local` (never the worktree), the clone restored after each
and clean at the end. Each is bounded at 600 s, and a looping mutant counts as
"KILLED (hang)" (none did). It runs against the unit's 9 tests and the 3
earlier ones that held these paths (T-U4's counter test, train 6's join
test, the scan's run test). **18 of 18 killed.**

- **Run 1, at `0c9e1dc9`:** 15 killed by the unit's tests. They are the cache
  without its hit (M1), the width out of the key (M2), new arguments taking the
  old ones' rows (M3), the palette ignored when painting (M4), a window
  ignoring where each run starts (M7), trailing spaces kept (M8), the blank
  row lost (M9), and the fast paths: CR LF split (W1), a space in a run (W3),
  the last ASCII before a non-ASCII byte kept (W4), the last copy counted
  (W5), a run past the margin (P1), a run inside a cluster cut into
  characters (P2), the final-word test stepping over a space (P3), and the
  scan counting past the row (S1).
- **Run 2, at `06b4fc4b`:** the three run 1 let through.
  - W2 (a DEL counted a cell): killed by `a_printable_ascii_character_is_one_cell`
    as `06b4fc4b` widened it to every one-byte cluster.
  - M5 (an argument's trailing break kept) and M6 (a window starting each
    entry a row late) survive the unit's tests. They are killed by the full
    transcript's own tests (1 and 22 failed): those rows were pinned before
    this unit (`a_trailing_line_break_is_no_rail_row_an_argument_drops_it_and_a_result_leaves_a_blank`,
    and every test that reads an exact screen). The unit's equality tests
    compare the cache with a session laid out from nothing, and both go
    through the mutated window.

## Sweep

`sweep.sh` is train 6's with this unit's selection (`selection.txt`). Each
binary was built in its own `git clone --no-local` and swept with its own
tree's harness, scenarios and ledger:

- the base, `9681e9d8` (`x𝒇x v0.1.0-9681e9d [debug]` on row 1);
- the head, `06b4fc4b` (`v0.1.0-06b4fc4`, whose code is `5e794435`'s; only a
  test differs).

The oracle was fx v0.0.16, sha256 `8e6cda2e…` before and after each sweep,
with `FX_AUTO_UPGRADE=0`. Size 40x120, `--jobs 2`, both sweeps one after the
other in one hold of the suite lock (`sweep-run.txt`).

The selection is the brief's families (every `p5-transcript-*`, the cancel
scenarios as train 6 named them (`interrupt*`, `question*`, `compact-cancel`),
every `p5-ctrlo-wider-*` and `p5-toolgroup-*`) plus, because `1a9652cc` goes
through wraps every surface uses:

- the scenarios named for a wrap or a resize;
- `fulltranscript*` and `toolgroup*`;
- the answer's code and tables, `p5-inline-code-*`.

**Result** (`sweep-base.md`, `sweep-head.md`, `compare.txt`, `families.txt`):
75 scenarios each, 0 failed, 0 timed out, **228 / 340 pairs pass on both**,
and **no pair drops**. `compare.py` exits 0: every pair that passes on the
base passes on the head.

| family | scenarios | pairs | pass, base | pass, head | scenarios passing fewer on the head |
|---|---|---|---|---|---|
| wraps, resize, fulltranscript*, toolgroup* (added) | 12 | 61 | 35 | 35 | 0 |
| cancel: interrupt*, question*, compact-cancel | 16 | 93 | 84 | 84 | 0 |
| p5-ctrlo-wider-* | 7 | 23 | 8 | 8 | 0 |
| p5-inline-code-* | 8 | 24 | 24 | 24 | 0 |
| p5-toolgroup-* | 8 | 32 | 26 | 26 | 0 |
| p5-transcript-* | 24 | 107 | 51 | 51 | 0 |
| **all** | 75 | 340 | 228 | 228 | 0 |

- **Four pairs moved from identical to normalized-identical** (still passes):
  `p5-transcript-md` `answered` and `closed`, and `p5-transcript-submit-multi`
  `filled` and `second`. In each, the only row of xfx's text that differs is
  the turn summary's seconds (`1s` against `2s`, `0s` against `1s`), the wall
  clock, which the ledger normalizes. fx's screen did not move.
- **Two unresolved pairs gained a differing row**, from timing on both
  sides: `interrupt-recovery` `full` (fx's network line, `15ms` against
  `12ms`) and `interrupt-retry` `r1.05` (where the retry stood at 1.05 s).
  Neither passes on either side.

## Gate

`gates.sh` (train 6's, unchanged but for its comment), `gates.txt`: every CI
step, one after another, in one hold of the suite lock, at `255d3cac`, the
tree clean before and after. **20 steps, 0 failed:**

- fmt, and clippy both ways;
- `cargo test` (lib 4449 passed, tui 181, every suite 0 failed) and with
  fault injection;
- `check-xfx-identity`, `check-no-stubs`, `check-no-secrets`,
  `check-preview-contract`, `check-tui-preflight-cost`, `check-usage-clock`;
- `test-check-no-secrets`, `test-gate-env`, `test-preview-contract`;
- fxcap `test-sweep` and `test-capture`;
- both release builds, smoke and smoke-tui.

After it, `5e794435` (test only): fmt, clippy both ways, and the 106 tests of
`tui::shell::tests::ctrl_o` and `tui::full_transcript`, all ok
(`after-gate.txt`).

## Network

`net.py`, `net.txt`: the nine new tests ran with every proxy variable pointed
at a loopback logger (train 6's check, FLAKE-CATALOG-EVENT's
`connect_logger.py`). All 9 passed, and 0 connections were logged.

## Security (an S unit: model-controlled text on the terminal)

There is no new path from model text to the terminal, and no row's bytes
change:

- `railed` is `Railed::new` and then `Railed::row` for each row, and an
  argument's rows are those same `Railed` rows painted when shown.
- `rows_kept_unpainted_are_the_rows_railed_painted_at_once` holds both
  against `railed` as `9681e9d8` wrote it.
- The shell equality test compares the screen and the whole record with the
  session laid out from nothing after every event.
- The sweep compares the screens with fx's.

The text is the channel's spelled and named argument, as before; nothing
here spells, unspells or names. The wraps' fast paths change how a cluster
is measured and stepped over, never which bytes a row holds. Each is held
against the code that counts every unit. The cache's memory is bounded
above. No file, credential, child process or network path is touched.

## Files

- `red.txt`: the RED at `11d6d64a`.
- `render_cost_probe.rs`, `apply_probe.py`, `run.sh`, `probe.txt`,
  `probe-summary.txt`, `summarize.py`: the numbers.
- `applies.txt`: the change on T-U4 alone and on CTRLO-WIDER alone.
- `results_probe.rs`, `results.txt`: a group whose members carry results at
  their bound.
- `mutants.py`, `mutants.txt`: the mutants.
- `selection.txt`, `sweep.sh`, `sweep-run.txt`, `sweep-base.md`,
  `sweep-head.md`, `compare.py`, `compare.txt`, `families.py`,
  `families.txt`, `redacted.py`: the sweep (every sweep file through
  `redacted.py`).
- `probe-a1ff89e7.txt`: the cache alone, by hand.
- `gates.sh`, `gates.txt`: the gate; `after-gate.txt`: the checks after it.
- `net.py`, `net.txt`: the network check.
