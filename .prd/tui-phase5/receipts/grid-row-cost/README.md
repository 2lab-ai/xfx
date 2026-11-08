---
title: GRID-ROW-COST -- render cost under adversarial rows
type: receipt
tags: [xfx, tui, phase5, receipt, perf, memory]
---

# GRID-ROW-COST (S): render cost under adversarial rows

Base: train 6's head, `fdc5ac0b` (`train/p5-1108c`). Branch
`feat/p5-grid-row-cost`. Every figure is **[measured]** (a file here holds the
run) or **[derived]** (arithmetic on measured figures, the operands named).

| commit | what |
|---|---|
| `1078ff37` | RED: three tests and a test-only byte counter (`grid::bytes_tokenized`) |
| `fcf78575` | GREEN: the grid keeps what a row was laid from; the check keeps its replays of long rows; the transcript keeps one generation of cuts |
| `c3f6fa48` | RED, item 5: two buffers held at up to twice what they hold |
| `96c3c88a` | GREEN, item 5: a call row built at its length; an argument's rows shrunk once laid |
| `d2b603be`, `50a099b2`, `8dd4604e`, `01149542` | tests only: a fitting row kept once; the session case's premise; a kept replay found is kept for the vector after; a test comment's citation |
| `b6c192b9` | this receipt, first part (the measurements, the sweep, the mutants, the gate at `01149542`) |
| `87783167` | RED, after review: a long row a theme report recoloured is never known to be laid from its text again |
| `0bddf3d8` | GREEN: a row found by placing it again is kept as laid (`Grid::row_holds`) |
| `d90ad8b5` | this receipt, second part (the theme probe, mutant G8, the banners, the gate at `0bddf3d8`) |
| `60ff598a` | review round 1 (opus SHOULD 4), tests only: the session case runs Ctrl-O over the long rows and a collapse |
| then | this receipt, round 1's fixes: the Ctrl-O sentence, the lock's path redacted, the line references, the themed peak, every mutant at one code point, GRID-KEY-HASH named |

`d2b603be`..`01149542` touch only `#[cfg(test)]` code (tests, a test helper, a
test's comment; `git diff 96c3c88a 01149542 -- src`), so the first part's
measurements at `01149542` are `96c3c88a`'s code. The final code is
`0bddf3d8`'s.

## The question, and the bar

**The happy path.** A tool event becomes bytes on the terminal: `Shell::apply`
(the group's rows rebuilt, `toolgroup.rs`), then one tick of the loop's writer,
`event_loop::commit_frame` -> `commit_document` (`event_loop.rs:1940`): the
transcript cuts the block's rows for the screen (`Transcript::prepare_block`,
`transcript.rs`), and `Band::append_floating` (`frame.rs:2640`) builds the
append (`render_append_recorded`, `frame.rs:1521`): the shadow cloned, each
settled row asked whether the screen already holds it (`Grid::row_matches`
at the base, `Grid::row_holds` now, `frame.rs:1654`) and placed if not, the new rows placed; then
`check::preflight` replays the declared script beside the decoded bytes; then
the bytes are written and the band's own frame follows.

**Where an event's time went at the base** (`profile-base.txt`: macOS `sample`
of the frame probe, heavy case, one long row, 120 events): of `commit_frame`'s
6 209 samples, `Grid::row_matches` -> `Grid::place_row` -> `tokenize_row` 3 017,
and `check::preflight` -> `Expected::edits` -> `tokenize_row` 1 606 + 1 499 --
**99 %** [derived] -- each reading the long row whole, cluster by cluster, on
every event that left the row where it was. Everything else (the shadow's
clones, the seeds, the band's frame, the transcript's kept cuts) 2 %.

**The bar** is the brief's own example, taken unchanged: the
tick that follows an event (`commit_frame`: every append the event owed,
through its check, and the band's frame -- append to bytes written) costs
**at most 16 ms, median of 40 events**, with the group's long rows left where
they are: one and three long calls, `heavy` (one letter under 4 Mi U+0301 and
`tail`: one 8 MiB cluster) and `zw` (2.8 Mi U+180E: 8 MiB that paint nothing),
release, under the suite lock. Reported beside it and **not** in the bar: the
shell's `apply`, the event's total, the worst event (the row's connector
changes once, so its 8 MiB are written again -- fx writes the same bytes) and
the first placement of each long row.

## The fix

**1. The grid answers for a row it laid without reading it** (`grid.rs`).
Each row's record keeps the text its cells were laid from and the two widths
the tokenizer read (`Laid`, `grid.rs:469`: the grid's, the screen's), shared
by the shadow and the target cloned from it (`Arc<str>`). `place_row`
(`:610`) sets it; `put_row` (`:632`) is the same cells with nothing kept, for
the scratch rows a comparison or a drawing uses. It is forgotten by every
change to a cell that is not a placement -- `erase_row` (`:548`, the whole
record), a recolour or a drawing put on the row (`retint_document`, `:793`,
when a cell of the row moved, `:830`) -- carried by the moves (`scroll_up`,
`remove_rows`: the records move with the cells), and dropped by `resize`.
`record` (`:574`), the writer's own say about the row, leaves it.
`row_matches` (`:688`; the frame's `row_holds`, `:704`, since fix 4) answers `true` from it when the text and both widths
are the same, after the width guard it always had, and places the row again
otherwise. A comparison of the text is one pass over its bytes.

**2. The output check lays a long row it laid in the last append from what it
laid** (`check.rs`). `Replays` (`:1182`): the cells the replay itself laid for
a declared row of 4 KiB or more (`REPLAY_KEPT_BYTES`, `:1162`), keyed on the
text, the plane's width and the screen's -- everything `tokenize_row` reads --
held one generation: what an append's replay laid replaces what the one
before laid (`turn`, `:1215`), so every long row a block declares again is
found however many there are, and nothing outlives the vector after the last
one that declared it. `Expected::place` (`:1251`) lays a found row from it;
`preflight_kept` (`:1439`) is `preflight` with the band's `Replays`
(`frame.rs:176`, `:2672`). **The check's own record, never the emitter's:**
nothing in `check.rs` reads the grid's `Laid`, so a screen (seed or bytes)
that does not hold the declared row is refused as before
(`a_kept_replay_still_refuses_a_screen_that_does_not_hold_the_declared_row`).

**3. The transcript keeps one generation of cut rows, not five**
(`transcript.rs`, item 4). A tool row is cut in the mode it is written in and
in each mode its drawing is kept for (`Redraw::map`), two long rows a call:
three long calls ask for six, and the five kept (`KEPT_ROWS`) thrashed --
**four of the six cut again on every write of the block** (RED: 12 cuts in
three writes, `red.txt`) [measured]. `KeptRows` (`:682`) holds the last
write's cuts and this one's; `turn_kept_rows` (`:986`, called at `:1209`)
replaces the one with the other, so a block's long rows are found however many,
and a row the block stopped writing is let go by the write after
(`a_long_row_the_block_stopped_writing_is_let_go_by_the_write_after`). A row
that fits is kept once (`written: None`), not as the row and a copy of it.
**Opus's "~83 MiB worst" [추론]** was five kept rows of row and cut; the
generation holds two rows a long call, each once when it fits: for three heavy
calls 6 x 8 MiB where the five held 5 x 16 MiB [derived], and it grows with a
block's long calls rather than thrashing.

**4. A row found by placing it again is kept as laid** (`0bddf3d8`, found in
review after the first part). A theme report recolours the long rows where
they are and forgets their text (fix 1, correctly: the cells moved). The
append after it finds each row, drawn in the new palette, already on the
screen -- by placing the text again and comparing -- and kept nothing, so
every later append read it whole again for as long as it was on the screen
(RED `87783167`, `red-relay.txt`: the session test's assertion after the
light reads). `Grid::row_holds` is the frame's question now (`row_matches`
the tests', which keeps nothing): a match found by placing the row again is
kept as a placement keeps it. It answers the same `bool` as `row_matches`
(both are `Grid::matched(..).is_some()`), so no reuse decision and no byte
moves; only what is kept does.

**5. Item 5, two buffers held at their size** (`96c3c88a`). A call row that
keeps its whole argument was grown by `format!` to twice the row for the reset
written after the argument; `toolgroup::call_row` (`toolgroup.rs:754`) builds
it at its length, the same bytes. An argument's rows laid line by line are
shrunk once laid (`full_transcript.rs:1199`).

## The numbers: main, base, head

`run.sh` on three `git clone --no-local`s -- main `690aa505`, the base
`fdc5ac0b`, the head `01149542` (the code of `96c3c88a`) -- each with the three
probes appended (`apply_probe.py`, `apply_memory_probe.py`,
`apply_ctrlo_probe.py`) and nothing else, release, one case a process, the
clones alternating case by case, two rounds, **one hold of the suite lock**
(`probe.txt`, ended 16:15:53Z, before the load loop another worker ran from
16:35Z; the 1-minute load average is printed on every line and in the tables'
last column). Round 1's long-row cases ran at load 10-48 (other workers'
jobs); **round 2's at 4.8-8.5**, and round 2 is the measurement the bar is read
on.
`probe-summary.txt` is `summarize.py`'s tables of it. ms as `round 1 / round 2`.

**The frame path** (`frame_cost_probe.rs`: an event applied, then one
`commit_frame` onto a screen that takes everything; 20 reads joining the
group, a start and a result each, 40 events):

| case | long calls | build | **tick (frame) median** | apply median | event worst | first placement | peak RSS MiB | load (1 min) |
|---|---|---|---|---|---|---|---|---|
| heavy | 1 | main | 0.7 / 0.4 | 0.0 / 0.0 | 1.1 / 0.7 | 1.0 / 0.5 | 52 / 51 | 48.4 / 8.5 |
| heavy | 1 | base | 271.4 / 263.3 | 3.1 / 2.8 | 1 497 / 720 | 1 298 / 667 | 270 / 268 | 47.0 / 8.5 |
| heavy | 1 | **head** | **3.3 / 2.9** | 1.2 / 1.1 | 769 / 708 | 690 / 643 | 259 / 259 | 41.3 / 7.5 |
| heavy | 3 | main | 0.8 / 0.8 | 0.1 / 0.1 | 1.0 / 1.0 | 1.2 / 1.1 | 51 / 51 | 38.7 / 7.5 |
| heavy | 3 | base | 973.5 / 930.3 | 11.1 / 10.7 | 1 464 / 1 331 | 1 903 / 1 765 | 614 / 614 | 38.7 / 7.5 |
| heavy | 3 | **head** | **8.4 / 8.6** | 3.3 / 3.3 | 719 / 724 | 1 538 / 1 583 | 669 / 669 | 21.5 / 5.6 |
| zw | 1 | main | 0.4 / 0.5 | 0.0 / 0.0 | 0.7 / 1.0 | 0.6 / 0.9 | 42 / 44 | 20.0 / 5.4 |
| zw | 1 | base | 373.4 / 336.2 | 3.0 / 3.0 | 892 / 1 119 | 865 / 912 | 692 / 690 | 20.0 / 5.4 |
| zw | 1 | **head** | **3.0 / 2.9** | 1.1 / 1.1 | 951 / 880 | 907 / 853 | 286 / 285 | 16.9 / 4.8 |
| zw | 3 | main | 0.9 / 0.9 | 0.1 / 0.1 | 1.2 / 1.2 | 1.4 / 1.4 | 43 / 43 | 16.3 / 4.8 |
| zw | 3 | base | 1 239.2 / 1 376.2 | 11.6 / 12.9 | 1 750 / 1 844 | 2 337 / 2 259 | 910 / 916 | 16.3 / 4.8 |
| zw | 3 | **head** | **8.5 / 9.7** | 3.3 / 3.4 | 960 / 1 226 | 2 034 / 2 981 | 720 / 725 | 9.8 / 7.0 |
| short, c1, wider | 1 | all three | 0.2-0.4 | 0.0 | 0.2-3.1 | 0.2-6.6 | 10-116 | 7.4-9.0 |

- **The bar holds**: in round 2 (load 4.8-7.5) the tick's median is 2.9 ms
  with one long call (heavy, zw) and 8.6 / 9.7 ms with three, against 263 /
  336 ms and 930 / 1 376 ms at the base (load 4.8-8.5) [measured]: **91-142x**
  [derived]. Round 1, under load 10-48, gives 3.0-3.3 and 8.4-8.5 at the
  head. The event as a whole (apply + tick)
  median 3.9-4.5 and 11.6-13.1 ms (`probe-summary.txt`).
- **main is not the reference for these two cases**: main cuts a call's row at
  256 KiB (T-U4 keeps it whole, as fx and the screen keep it), so its rows here
  are short. The base is T-U4's cost; the head is what is left of it.
- **Not in the bar, unchanged** [measured]: the worst event, when the first read
  joins and the long call's connector changes, so its 8 MiB are written again
  (708-1 226 ms at the head, 720-1 844 at the base), and each long call's first
  placement (643-907 ms a call). Both write the row's bytes, as fx does; what
  they cost beyond the write is in Residuals.
- `apply` fell too (2.8-3.1 -> 1.1-1.2 ms with one long call, 10.7-12.9 ->
  3.3-3.4 with three) [measured]: the
  shell's rows are built at their length (`call_row`) and a fitting row is cut
  from the kept generation without a second copy.
- `short`, `c1` and `wider` (rows a screen's width, no long row on the
  transcript): the base and the head within 0.2 ms of each other on every
  time and 1 MiB on the peak; main differs from both where T-U4 does (`c1`'s first placement,
  `wider`'s worst event).

**Memory** (`memory_probe.rs`: the heap in use, `malloc_zone_statistics`, and
the peak RSS after each stage, train 6's DETAILS=1 path; MiB, live / peak,
round 1, load 8.95 on every case; `probe-summary.txt` has every stage):

| case | build | after start | after the 40 rebuilds | Ctrl-O opened | a call joined | resized, closed (end) |
|---|---|---|---|---|---|---|
| wider (2 Mi U+261D) | main | 6.1 / 32.1 | 12.1 / 32.6 | 25.7 / 59.8 | 25.7 / 74.3 | 25.9 / **83.0** |
| | base | 18.9 / 32.8 | 35.4 / 88.2 | 60.7 / 116.5 | 61.7 / 122.5 | 61.7 / **138.0** |
| | head | 18.9 / 32.8 | 35.0 / 87.7 | 54.3 / 115.0 | 60.9 / 121.0 | 54.9 / **137.6** |
| heavy (4 Mi U+0301) | main | 16.1 / 48.2 | 24.1 / 48.6 | 44.9 / 79.3 | 45.0 / 100.8 | 45.0 / **100.8** |
| | base | 88.2 / 144.4 | 128.3 / 185.0 | 152.3 / 185.2 | 192.4 / 193.3 | 192.4 / **193.3** |
| | head | 72.2 / 128.3 | 80.2 / 144.8 | 96.2 / 145.1 | 120.3 / 145.1 | 120.3 / **145.2** |

**After a theme report** (`run-theme.sh`, `probe-theme.txt`: the frame
probe with the terminal reported light before the 40 events timed,
`XFX_PROBE_THEME=1`; the base, the first part's head `01149542` and the final
`0bddf3d8`; one hold of the lock; ms, round 1 / round 2; load 3.2-8.6):

| case | long calls | build | tick median | load (1 min) |
|---|---|---|---|---|
| heavy | 1 | base | 268.7 / 260.3 | 8.62 / 4.07 |
| heavy | 1 | `01149542` | 3.0 / 2.9 | 7.76 / 3.89 |
| heavy | 1 | **`0bddf3d8`** | **3.0 / 2.9** | 7.76 / 3.89 |
| heavy | 3 | base | 939.9 / 918.6 | 7.38 / 3.98 |
| heavy | 3 | `01149542` | **266.0 / 265.2** | 5.70 / 3.72 |
| heavy | 3 | **`0bddf3d8`** | **8.4 / 8.3** | 5.18 / 3.24 |

With one long call the gap does not show: the first read joining changes the
call's connector, so the row is placed again and known. With three, the first
two are never placed again, and at `01149542` every event after the theme
report read them whole (266 ms); at `0bddf3d8` 8.3-8.4 ms, the same as with no
theme report [measured]. The same run, without the theme report, at
`0bddf3d8`: heavy 2.9 / 2.9 and 8.6 / 8.3, zw 2.9 / 2.9 and 8.4 / 8.4 (load
3.3-4.8) -- the bar holds at the final code.

**Ctrl-O** (ARGROW-CACHE's probe, `XFX_PROBE_DETAIL=1`; ms; load 7.4-9.0):

| case | build | open | join | theme | resize | peak RSS MiB |
|---|---|---|---|---|---|---|
| c1 | main | 587.8 / 595.2 | 585.2 / 596.6 | 580.7 / 590.2 | 591.7 / 599.9 | 455 |
| c1 | base | 76.0 / 73.5 | 34.3 / 30.8 | 33.3 / 34.5 | 58.2 / 59.2 | 415 |
| c1 | head | 71.8 / 77.7 | 25.9 / 27.9 | 24.1 / 28.5 | 56.1 / 58.4 | 383 |
| wider | main | 107.7 / 107.8 | 107.9 / 107.4 | 108.6 / 108.3 | 109.4 / 109.1 | 84 |
| wider | base | 27.2 / 29.0 | 3.1 / 3.6 | 2.7 / 2.8 | 25.1 / 24.4 | 138 |
| wider | head | 27.6 / 29.3 | 3.1 / 3.2 | 2.7 / 2.6 | 23.3 / 24.6 | 137 |

No Ctrl-O figure moved past round-to-round noise [measured]: `c1` open 76.0 /
73.5 -> 71.8 / 77.7; `wider`'s open, join, theme and resize within 0.4 ms of
the base's in the same round, or below them; `wider`'s detail 34.9 / 39.0 ->
38.4 / 38.5, inside the base's own spread across its two rounds. `c1`'s join
and theme fell 3-9 ms and its peak 32 MiB (415 -> 383; the call row built at
its length). Two rounds a build: not a statistically established
non-regression.

## RED -> GREEN

RED (`red.txt`, at the base's code with the tests and the test-only counter
added, `1078ff37`), each **FAILED** there and **ok** from `fcf78575`:

| test | RED's figure |
|---|---|
| `grid::a_row_this_grid_placed_is_matched_again_without_reading_it` | `row_matches` read 131 077 of a 131 077-byte row it had placed itself |
| `frame::an_append_that_leaves_a_long_row_where_it_is_reads_none_of_it_again` | an append that left a 128 KiB row in place read 262 186 bytes of rows (the grid's comparison and the check's replay, both whole) |
| `transcript::a_block_with_three_long_rows_drawn_again_cuts_none_of_them_again` | three later writes of a block with three long drawn rows cut 12 rows again |

RED for item 5 (`red-memory.txt`, `c3f6fa48`), **FAILED** there and **ok**
from `96c3c88a`:

| test | RED's figure |
|---|---|
| `toolgroup::a_call_row_with_a_long_argument_is_held_at_its_length` | a 196 625-byte row held in 393 242 |
| `full_transcript::an_arguments_rows_are_held_at_their_count` | 21 001 rows held in a vector of 28 672 |

**Equality with a fresh layout** (from `fcf78575`; each asks the remembered
answer and a fresh placement the same question and requires the same
answer, or holds the session to it after every tick):

- `grid::a_rows_text_moves_with_its_cells_and_goes_with_any_change_to_them`:
  a scroll and a removal carry the text with the cells; a writer's record keeps
  it (and reads nothing); a placement of another text, an erase, a resize and a
  band painted over the row each leave a row that answers as a fresh placement
  does;
- `grid::a_retint_leaves_a_row_that_answers_as_a_fresh_placement`: a retint
  that moves nothing keeps the text; a recolour and a drawing that move cells
  forget it;
- `grid::a_row_laid_for_a_narrower_screen_is_not_the_row_this_screen_lays`: the
  screen's width is part of the key;
- `check::a_kept_replay_lays_the_cells_a_fresh_one_lays_and_reads_none_of_the_row`
  (three long rows: a coloured mark run, a wide cluster with marks behind its
  second half, a row far wider than the screen; 10 and 80 columns),
  `check::a_kept_replay_is_found_only_for_its_text_at_its_widths_and_for_one_vector`,
  `check::a_kept_replay_still_refuses_a_screen_that_does_not_hold_the_declared_row`;
- `event_loop::a_groups_long_rows_stay_what_their_text_lays_through_scrolls_themes_and_a_resize`:
  a whole session -- two long calls (64 Ki marks each) in an open group, reads
  joining it, Ctrl-O opened over them and closed (from `60ff598a`: the plane
  taken and given back, the primary frame planned from the saved shadow), a
  theme report to light and reads, back to dark, a resize, 12 more reads that
  scroll the rows away, the long calls finishing, the turn ending, collapse
  switched on (from `60ff598a`: `remove_rows` on the live shadow) -- held
  after every tick to no refused vector and every row the
  shadow says it laid from a text holding a fresh placement of it
  (`Grid::stale_laid_rows`, test-only); with its premise checked mid-way
  (`Grid::longest_laid`: a long call's row is on the screen, laid from its
  text);
- `transcript::a_long_row_the_block_stopped_writing_is_let_go_by_the_write_after`.

## Mutants

`mutants.py`, **`mutants-final.txt`**: all 23 mutants at one code point, a
`git clone --no-local` at `60ff598a` (the final code, `0bddf3d8`'s; the commit
adds only the session test's Ctrl-O and collapse), never the worktree, each
one textual change, the clone restored after each and clean at the end, each
test run bounded at 600 s (its process group killed; a hang counts as killed),
one hold of the suite lock. **22 of 23 killed; none hung; G3 survives, declared
equivalent before the run.** The earlier runs -- 22 mutants at `8dd4604e`
(`mutants.txt`) and G8 alone at `0bddf3d8` (`mutants-g8.txt`) -- gave the same
verdict for every mutant.

The tests' short names are `mutants.py`'s (`SHORT`): `red-grid`, `red-frame`,
`red-thrash`, `red-row`, `red-rail` are the five REDs; `moves`, `retint`,
`narrower` the grid's; `kept-cells`, `kept-key`, `kept-refuses` the check's;
`session` the event loop's; `let-go` the transcript's.

| mutant | killed by (this unit's tests that failed, `mutants-final.txt`) |
|---|---|
| G1 a row laid from any text answers for every text | `moves`, `session` |
| G2 the screen's width left out of the key | `narrower` |
| G3 the grid's width left out of the key | **survives, declared equivalent before the run**: a grid's width changes only through `Grid::resize`, which forgets every record, so the width kept with a text always equals the grid's; kept in the key because the tokenizer reads it |
| G4 an erased row keeps its text | `moves`, `session` |
| G5 a retinted row keeps its text | `retint`, `session` |
| G6 a writer's record forgets the text | `red-frame`, `moves`, `session` |
| G7 a placement remembers no text | `red-grid`, `red-frame`, `moves`, `session` |
| G8 a row found by placing it again is not kept | `session` |
| C1 the screen's width left out of a kept replay's key | `kept-key` |
| C2 the plane's width left out of a kept replay's key | `kept-key` |
| C3 a kept replay found for any text at its widths | `session` |
| C4 a kept replay found trusts the row the screen holds | `kept-cells`, `kept-refuses` |
| C5 a vector's replays never kept for the next | `red-frame`, `kept-cells`, `kept-key`, `kept-refuses` |
| C6 every vector's replays kept for good | `kept-key` |
| C7 a kept replay found is not carried into this vector's | `kept-cells` |
| C8 no row is long enough to keep | `red-frame`, `kept-cells`, `kept-key`, `kept-refuses` |
| T1 a block's write never turns its cuts over | `let-go` |
| T2 every write's cuts kept for good | `let-go` |
| T3 a kept cut found is not carried into this write's | `red-thrash` |
| T4 a cut row kept as the row it was cut from | the module's own `transcript::a_kept_row_is_found_only_for_the_same_row_at_the_same_width` (this unit's 13 let it through) |
| T5 a row that fits kept twice | `let-go` |
| B1 a call's row built by format! | `red-row` |
| B2 an argument's rows left as the growth left them | `red-rail` |

## Sweep

`sweep.sh` and `selection.txt` are ARGROW-CACHE's (train 6's script), unchanged
but for their comments: every `p5-transcript-*`, the cancel scenarios
(`interrupt*`, `question*`, `compact-cancel`), every `p5-ctrlo-wider-*` and
`p5-toolgroup-*`, the wrap and resize scenarios, `fulltranscript*`,
`toolgroup*` and `p5-inline-code-*` -- every ctrl-o and transcript family. Each
binary built in its own `git clone --no-local` and swept with its own tree's
harness, scenarios and ledger: the base `fdc5ac0b`, the head `50a099b2` (the
code of `96c3c88a`). The oracle fx v0.0.16, sha256 `8e6cda2e…` before and
after each sweep, `FX_AUTO_UPGRADE=0`; 40x120, `--jobs 2`; both sweeps one after
the other in one hold of the suite lock (`sweep-run.txt`).

**Result** (`sweep-base.md`, `sweep-head.md`, `compare.txt`, `families.txt`):
75 scenarios each, 0 failed, 0 timed out, **228 / 340 pairs pass on both, no
verdict moved, no pair drops**; `compare.py` exits 0.

| family | scenarios | pairs | pass, base | pass, head | scenarios passing fewer on the head |
|---|---|---|---|---|---|
| wraps, resize, fulltranscript*, toolgroup* | 12 | 61 | 35 | 35 | 0 |
| cancel: interrupt*, question*, compact-cancel | 16 | 93 | 84 | 84 | 0 |
| p5-ctrlo-wider-* | 7 | 23 | 8 | 8 | 0 |
| p5-inline-code-* | 8 | 24 | 24 | 24 | 0 |
| p5-toolgroup-* | 8 | 32 | 26 | 26 | 0 |
| p5-transcript-* | 24 | 107 | 51 | 51 | 0 |
| **all** | 75 | 340 | 228 | 228 | 0 |

- One unresolved pair gained a differing row, from timing on both sides:
  `interrupt-recovery` `full` (the network line's `13ms`/`15ms` on fx's side,
  `0ms`/`1ms` on xfx's). It passes on neither.
- **The binaries are the commits named** (`banners.txt`): row 1 of every xfx
  capture that shows the banner reads `v0.1.0-fdc5ac0` in the base's sweep
  (255 of 340; the other 85 open on a scrolled screen or Ctrl-O's) and
  `v0.1.0-50a099b` in the head's (the same 255).
- **The final code (`0bddf3d8`) was not swept again**: it changes no reuse
  decision (`row_holds` and `row_matches` answer the same `bool`), so no byte
  the head's sweep compared moves; what it changes is held to by the gate's
  tests and the theme probe.
- 78 fx screens and 101 xfx screens are not the same in the two sweeps (row 1
  left out). Every xfx row that moved (`xfx-moved.txt`, `xfx_moved.py`) is a
  clock (a Ctrl-O entry's stamp, in UTC, KST or IST), a duration (`8526ms`
  against `8520ms`; a turn's `5s` against `6s`), a session id, a spinner's
  frame (`  Generating` against `• Generating`), or -- two rows, `composer-wrap` `badge-71` and `badge-111`,
  the same text -- a link's target, the sweep's own temporary directory.

## The preflight-cost step on PR #82's CI (the coordinator's question)

What `scripts/check-tui-preflight-cost.sh` runs: **two release
`cargo test --lib` invocations** (default features, then
`--features fault-injection`), each filtered to two tests in `check.rs`
(`a_preflight_costs_a_small_fraction_of_one_frames_budget`,
`a_replayed_append_costs_a_small_fraction_of_one_frames_budget`), single
threaded, with the harness's count checked. Each invocation builds the whole
library's test binary in release first; CI's runner keeps no cargo cache
(`ci.yml` has none), so each builds every dependency too.

**PR #82's x86_64-apple-darwin job** (`ci-preflight-step.txt`, from
`gh run view` of run 42485857189): the step ran 14:03:42 -> 14:21:13, 17 m 31 s
[measured]; the script's first line came at 14:03:42.60, the default-features
measurements at 14:13:48.74 (10 m 06 s), the fault-injection ones at
14:21:13.06 (7 m 24 s). The script prints only after each `cargo test` returns,
so CI's log does not split a build from its two tests; the tests' own figures
there are each preflight under 10 ms and each 1 000-row replay under 8 ms
[measured]. That the 17.5 min is the two release builds is **[추론]** from CI
alone; the local runs below split them.

**Locally, at main, the base and the head** (`preflight_cost.sh`,
`preflight-cost.txt`: each a clean `git clone --no-local` with an empty
target directory, the script run twice in a row, one hold of the suite lock,
ended 16:20:55Z, before the 16:35Z load loop):

| build | cold (both release builds and the tests) | warm (the tests alone) | result |
|---|---|---|---|
| main `690aa505` | 84 s, load 7.36 | 2 s, load 15.19 | ok |
| base `fdc5ac0b` | 84 s, load 15.19 | 1 s, load 10.59 | ok |
| head `01149542` | 86 s, load 10.59 | 1 s, load 12.71 | ok |

- **The step's time is its two release builds, not the check** [measured
  locally]: from nothing 84-86 s, with the test binaries built 1-2 s. Each
  preflight measured there is 15 µs - 1.5 ms median, each 1 000-row replay
  0.33-1.63 ms, at all three builds, budgets 8 and 32 ms. CI's 17.5 min on
  x86_64-apple-darwin is the same two release builds on that runner
  **[추론]**: its log does not split them, and the tests' own figures there
  are under 10 ms.
- **This unit does not shrink it** [measured]: 86 s against 84 at the head,
  1 s warm at both. The two cost tests call `check::preflight` on rows of a
  screen's width, which the kept replays (rows of 4 KiB or more, and only
  through `preflight_kept`) do not touch; their figures are the base's.
- **The script's bar and what it checks are unchanged** (`git diff fdc5ac0b --
  scripts/` is empty). What would shrink the step, not done here: the two
  `cargo test --release --lib` builds share nothing with the job's other
  release builds and the runner keeps no cargo cache (`ci.yml` has none) -- a
  cache of the target directory, or building the two test binaries once with
  `--no-run` beside the job's own release build, is CI's change to make.

## Item 5: the memory residual, isolated

Train 6 named it: 2 Mi U+261D peaks at 138 MiB at the train's head and 84 on
main; T-U4 alone 139; "T-U4's heading rows and group block [추론, not
isolated]".

**Where it is** [measured]: `memory.txt` (`live_sites.py` over
`malloc_history` at the probe's last stage, base and head) and the stage table
above. At the base's end the heap holds 61.7 MiB to main's 25.9: +35.8.

| what is held (base, end of the probe; the base's lines, as `memory.txt` prints them) | MiB | main | whose |
|---|---|---|---|
| the argument as Ctrl-O's argument line, spelled (`full_transcript.rs:1124`, `CallArgument::of`) | 16.0 | 6 (raw) | CTRLO-WIDER: fx spells U+261D `\u{261d}`, 8 bytes where the scalar is 3 |
| Ctrl-O's group block: its 22 rows, the long call's whole (`toolgroup.rs:738` via `full_transcript.rs:2069`) | **12.0** | small (main cuts the row) | T-U4 keeps a call's row whole -- **of it, 6 MiB a buffer twice its row** |
| the call's heading, wrapped whole (21 399 rows, `tool_status.rs:126`) | 6.5 | -- | T-U4: fx lays the heading whole |
| the argument's rows (`full_transcript.rs:1194`, `Railed::new`) | 6.0 | -- | ARGROW-CACHE: ranges into the line, one vector |
| the call's `Shown` twice: the transcript's group (`bridge.rs:498`) and Ctrl-O's call (`full_transcript.rs:1670`) | 6.0 + 6.0 | cut, small | T-U4 keeps the shown argument whole, in two places |

The peak is set later than the live residual: at the base it rises at the
arguments (33 -> 87: the spelled line built and copied), at Ctrl-O's open
(-> 116.5), its join (-> 122.5) and its resize (-> 138.0), each a re-layout's
transient on top of what is held (stage table, `probe-summary.txt`).

**Cut** (`96c3c88a`, RED `c3f6fa48`): the group row's slack -- 12.0 -> 6.0 MiB
at that site [measured, `memory.txt`]; the live heap at the end 61.7 -> 54.9
[measured]. Its peak moves 138.0 -> 137.6: **the wider peak is not cut**, and
this receipt does not claim it. The argument rows' vector held 6.0 MiB at both
(that case's rows were laid in one line, and the vector had no slack); the
shrink is the multi-line case of `red-memory.txt` (21 001 rows in 28 672).
The heavy case's peak is: 193.3 -> 145.2, live 192.4 -> 120.3 [measured],
from the call rows built at their length and a fitting long row cut
once and kept once.

**Left, by design**: the spelled line (fx's spelling), the heading's rows (fx's
whole heading), the call's row whole (T-U4, as the screen holds it), the
argument's rows.

**Left, and waste, not cut here**: `Shown` held twice (6 + 6 MiB here, the
argument's size each). Sharing it (an `Rc` from the event to the group and to
Ctrl-O's call) changes `tools::Shown`, which is `pub` with
`target: Option<String>` and used across the crate; proposed as its own unit,
not done in this one.

**What this unit adds** [measured, the frame table's peak]: a long row on the
screen is held once more as the text its cells were laid from (`Laid`), and
the check holds the cells and the text of each long row it replayed, for one
append. Three heavy calls: 614 -> 669 MiB (+55); for `zw` the base's churn was
larger than that and the peak fell (910-916 -> 720-725, one call 690 -> 285).
After a theme report (`probe-theme.txt:8-10`, `:18-20`), three heavy calls
peak at **781 MiB at the final code**, against 700-701 at the base (+81) and
765 at `01149542` (+16: the two long rows fix 4 re-lays, now kept as laid).

## Residuals (what still costs, and where)

- **A long row that changes** (the worst event, 0.7-1.2 s for 8 MiB): its bytes
  are written, as fx writes them, but it is also read four times on the way:
  the transcript cuts it (`ellipsized`, `transcript.rs:1559`: `wrap::width`
  of the whole row), the frame clips it and asks its width to decide the erase
  after it (`place`, `frame.rs:2757-2761`: `row_text`, then `wrap::width`),
  the grid places it (`put_row`, `grid.rs:632`) and the check replays it once
  (then keeps it). The erase's width could be read off the placement; not done
  here.
- **A long call's first placement** (0.6-0.9 s a call): the same reads, once.
- **The shell's `apply`**: 1.1 ms a long call an event (`ToolGroup::rows` and
  `redraws`, `toolgroup.rs:687`, `:660`: both drawings of every row rebuilt on
  every change) -- shell side, outside the frame path this unit's bar is on.
- **What the frame still copies**: the shadow into the target, the seeds, the
  check's model and expected plane each hold the long row's 8 MiB cluster;
  most of the 2.9-3.3 ms left of a tick with one long call is these copies
  **[추론]**: the base's profile put them in its other 2 %, and the head was
  not profiled.
- **Memory**: the wider peak (above); `Shown` twice; what this unit adds.
- **Whole-text keys, scanned linearly** (opus, review round 1, SHOULD 5,
  PLAUSIBLE, deferred to queue unit **GRID-KEY-HASH**): the grid's `Laid`
  (`grid.rs:736`), the check's `Replays::find` (`check.rs:1202-1212`) and the
  transcript's `cut_for_screen` (`transcript.rs:963-971`) each compare a long
  row's whole text, and the last two scan their entries in order, so N
  long rows of one length sharing a long prefix cost about N^2 * L bytes of
  comparison an append [추론, no test shows it; the probe's rows differ in
  their first byte]. Hashing the text into the key would close it.
- **The other emitters whose check replays a script of rows**
  (`Band::restore_primary`, `carry_document`, `hand_off`: `frame.rs:798`,
  `:1894`, `:2053`, each `Intent::Document`) go through plain
  `check::preflight`: a band that grows under a long row (an approval opening
  under a running group) has its check read the row whole once. Only the
  appends keep replays. (The wipe, the collapse and the retint are checked
  against a target grid, `Intent::Wiped`/`Primary`, which replays no text.)
- **The kept replays outlive their use by one append**: `Replays` holds the last
  append's long rows (the text and its cells, about twice the row) until the
  next append turns them, so an idle session keeps them -- 16 MiB a heavy row
  on the screen [derived: an 8 MiB text and its 8 MiB cell].

## Gate

**At the final code, `0bddf3d8`** (`gates-final.txt`, `gate-tests-final.txt`):
the same 20 steps in one hold of the suite lock, tree clean before and after,
**20 steps, 0 failed**, every step started at load 2.8-7.3 (outside the load
loop below); lib tests 4 462 and, with fault injection, 4 485 passed, 0 failed
(the session test's new assertion is inside an existing test).
`check-tui-preflight-cost` 80 s, load 3.57.

**At the first part's head, `01149542`** (below, `gates.txt`,
`gate-tests.txt`):

`gates.sh` (ARGROW-CACHE's, train 6's, unchanged but for its comment),
`gates.txt`: every CI step the brief lists, one after another, in one hold of
the suite lock, on a `git clone --no-local` at `01149542`, tree clean before
and after: **20 steps, 0 failed**. Seconds and the 1-minute load at each step's
start are in `gates.txt`.

- fmt; clippy, clippy with fault-injection;
- `cargo test` (lib: **4 462 passed**, 0 failed, 7 ignored -- the base's 4 449
  and this unit's 13), `cargo test --features fault-injection` (lib: 4 485, the
  base's 4 472 and 13), every result line in `gate-tests.txt`;
- `check-xfx-identity`, `check-no-stubs`, `check-no-secrets`,
  `check-preview-contract`, `check-tui-preflight-cost` (85 s, load 3.86),
  `check-usage-clock`;
- `test-check-no-secrets`, `test-gate-env`, `test-preview-contract`;
- fxcap `test-sweep` and `test-capture`;
- both release builds, `smoke`, `smoke-tui`.

**The load:** steps 17-20 (the two release builds, smoke, smoke-tui) ran from
16:35Z inside another worker's CPU loop (load 29-37 at their start): they
passed, and their seconds are not clean timings. Every other step started at
load 2.7-12.7. **The lock's own exit:** `suite-lock.sh` was rewritten on disk at
16:38:47Z while this hold was open, and bash read the new file at the old
offset after the gate returned -- its last two lines are the wrapper's syntax
error and exit 2, not the gate's (`gates.txt`'s header says so).

## Files

- `red.txt`, `red-memory.txt`: the two REDs.
- `profile-base.txt`, `profile_tree.py`: where the base's event went.
- `frame_cost_probe.rs`, `apply_probe.py`; `memory_probe.rs`,
  `apply_memory_probe.py`; `apply_ctrlo_probe.py` (ARGROW-CACHE's probe);
  `run.sh`, `probe.txt`, `summarize.py`, `probe-summary.txt`: main / base / head.
- `memory.txt`, `live_sites.py`: what is live at the probe's end.
- `mutants.py`, `mutants-final.txt` (all 23 at the final code); `mutants.txt`
  (22 at `8dd4604e`), `mutants-g8.txt` (G8 at `0bddf3d8`).
- `sweep.sh`, `selection.txt`, `sweep-run.txt`, `sweep-base.md`,
  `sweep-head.md`, `compare.py`, `compare.txt`, `families.py`, `families.txt`,
  `xfx_moved.py`, `xfx-moved.txt`, `redacted.py`: the sweep (every
  sweep-derived file through `redacted.py`).
- `ci-preflight-step.txt`, `preflight_cost.sh`, `preflight-cost.txt`: the
  preflight-cost step.
- `red-relay.txt`, `run-theme.sh`, `probe-theme.txt`: the re-laying fix.
- `banners.txt`: the sweeps' binaries.
- `gates.sh`, `gates-final.txt`, `gate-tests-final.txt`: the gate at the final
  code; `gates.txt`, `gate-tests.txt`: at `01149542`.
