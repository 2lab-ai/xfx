---
title: 0.0.16 delta -- P5 TG-REDRAW receipt (a theme switch draws a tool group's rows again)
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, transcript, theme, toolgroup]
---

# P5 TG-REDRAW: a theme switch draws a tool group's rows again, as fx projects the group

Gap found by T-U3-TCb (`../inline-code-truecolor/README.md`, "At 256
colours"), filed as `[[cause]] toolgroup-retint-256`. Oracle: fx v0.0.16
(`481104fd`, executable sha256 `8e6cda2e…`, run with `FX_AUTO_UPGRADE=0` by
`capture.sh`, its guard reporting the bytes unchanged after every run).
Base: `58b463c8`, the head of train 2 (PR #76). Branch
`feat/p5-toolgroup-redraw`.

**Builds.** Round 1: `88e7c7b6` (the fix) and `b6571bea` (the new scenarios
and one more unit assertion). Round 2 (the review of `a765487d`, below):
`7027e284` and `f482d340`. Every `xfx/` capture is from a debug build of
**`f482d340`** made after that commit (row 1: `x𝒇x v0.1.0-f482d34 [debug]`,
44 snaps, `green.txt`); every `red/` capture from a debug build of `58b463c8`
(`v0.1.0-58b463c`, 44 snaps, `red.txt`). Round 1's `xfx/` captures were
`b6571bea`'s, with the same results (`green.txt` at `a765487d`). The commits
after `f482d340` change `docs/parity.md`, `scripts/fxcap/parity-ledger.toml`
and this directory, and no compiled file. Each capture is kept as its
`capture-pane -e` file (`.ansi`), which every comparison here reads; the plain
`.txt` render beside it is left out (Gates, `check-no-secrets.sh`).

## fx's rule (all `@481104fd`)

What fx redraws, and when:

- **A tool group is a projection of its entries, built on every transcript
  build.** `buildCompactTranscriptProjectionInterruptible`
  (`src/ui/transcript/source_preparation.zig:1526`) calls
  `tool_group_projection.buildStyledFocusedInterruptible` with the entries,
  their tool details and `.text_style = ui_render.statusline_style`, the
  statusline grey **then in force** (`:1533-1546`). Nothing of a projection
  is kept across a build; the entries are the source.
- **Each group is drawn by `formatGroupBlock`**
  (`src/ui/transcript/tool_group_projection.zig:558-668`): the header in the
  summary style (`formatGroupHeader`, `:506-517`), then each settled call
  (the focused, running one is the footer's) from its entry's phrase with
  its escapes stripped (`normalizeCanonicalStatus`, `:317-347`), through
  `highlightCommandPhrase` (`:620`) and written in `style.text_style`
  (`:631-634`). `highlightCommandPhrase` (`:742-784`) highlights the command
  with `shared_theme.current()` (`:770`), light or dark (`:772`), the row's
  style re-established after each token -- so every header and call row,
  in an open group and a closed one, is drawn again in the new theme.
- **A cancelled call's own line is not projected.** `formatGroupBlock` draws
  that entry's own bytes (`renderEntryToBlock`, `:649-664`), and a switch
  rewrites those bytes through the builtin token table first
  (`retintEntriesForTheme`, `src/ui/transcript/store.zig:3722`, the loop `:3743-3760`;
  `themeOwnsRawEntry` holds `.tool_status`, `:3802-3806`; pairs
  `:3784-3800`).
- **The switch.** `applyThemeUpdate`
  (`src/core/app/app_render_runtime.zig:482-512`) rewrites the raw entries
  (`:499`), applies the new theme -- and with it the new
  `statusline_style` -- (`:504-508`), resets the terminal (`:510`) and asks
  for the transcript (`:511`), whose build projects every group again.
- **Ctrl-O** builds its tool rows uncached from the same entries
  (`buildFullTranscriptProjectionUncachedFrom`,
  `src/ui/transcript/runtime.zig:11415`, the build `:11423-11440`): `installExpandedGroup` and
  `formatExpandedChild` (`tool_group_projection.zig:850-876,788-819`; the
  command highlighted over the default foreground, `:808`), the header
  styled with `ui_render.statusline_style` as it is materialized
  (`:1027-1062`, `:1064-1095`).

## The gap, on the base (`58b463c8`)

xfx recoloured a tool group's rows by the colour each cell held, within the
row's recorded kind (`Palette::document_retint_in`, `DocumentRow::ToolGroup`:
`[STATUSLINE, HINT, COMMAND, NOTICE, COMMENT]`, and in a truecolor session
`[COMMENT, COMMAND, NOTICE]` for an indexed cell), and kept no group to draw
again once its block closed. At 256 colours a string's light 241 and a
comment's dark 245 are the statusline's own greys, so the statusline took
them: `red.txt`, row 6 of every clash snap --

| pair | snap | cells | fx | xfx (base) |
|---|---|---|---|---|
| `p5-toolgroup-retint` | `light` | `# note` | 243 | 241 |
| `p5-toolgroup-retint` | `dark-again` | `"hi"` | 250 | 245 |
| `p5-toolgroup-retint-light` | `dark` | `"hi"` | 250 | 245 |
| `p5-toolgroup-retint-light` | `light-again` | `# note` | 243 | 241 |
| `p5-toolstop-retint` | `light` | `# note` (`└ Cancelled …`) | 243 | 241 |
| `p5-toolstop-retint` | `dark-again` | `"x"` | 250 | 245 |
| `p5-toolstop-retint-light` | `dark` | `"x"` | 250 | 245 |
| `p5-toolstop-retint-light` | `light-again` | `# note` | 243 | 241 |
| `p5-toolgroup-live` (new) | `running-light` | `# note` (`└ Running …`) | 243 | 241 |
| `p5-toolgroup-live` (new) | `dark` | `"hi"` | 250 | 245 |
| `p5-toolgroup-ctrlo` (new) | `light` | `# note` | 243 | 241 |
| `p5-toolgroup-ctrlo` (new) | `dark-again` | `"hi"` | 250 | 245 |

The `-tc` pairs were `ramp: 0` on the base (T-U3-TCb's highlighter arm),
and the Ctrl-O rows matched (`ctrlo-rows.txt`, `red`): the full transcript
already lays its rows out again per palette (`full_transcript.rs`
`laid_out`, keyed by the palette).

**A second break on the same path, found looking ahead.** A theme report's
recolour is paid before the appends of the same tick
(`event_loop.rs` `commit_document`), and a group's block is queued as rows
already drawn. A block queued before a report and landing after it landed in
the old palette with nothing left owed: `a_tool_group_queued_before_a_report_lands_in_the_palette_the_report_set`
gives `245` where `241` on the base (`red-unit.txt`).

**RED, unit (`red-unit.txt`).** The new event-loop cases, inserted into the
base's `event_loop.rs`: four fail, three on the clash cells --
`a_theme_report_draws_a_tool_groups_rows_again_as_upstream_projects_them`
(`# note` 241, wanted 243), `a_tool_group_drawn_light_is_drawn_again_dark_and_light_as_upstream_projects_it`
(`"hi` 245, wanted 250), `an_open_tool_group_is_drawn_again_on_a_switch_and_settles_in_the_new_palette`
(241, wanted 243), and the queued-block case above. The truecolor case
passes on the base too: it guards that the redraw keeps what T-U3-TCb's arm
gave, without the arm.

## What changed in xfx (round 1; line numbers at `a765487d`)

- **`toolgroup.rs`: `ToolGroup::redraws`** (`:495`) -- beside `kinds`, for
  each row of the block: the header and each call row drawn in the dark and
  the light palette at the session's depth (`Redraw`), `None` for the blank
  rows and for a cancelled call's own line. `shell.rs` `redraw_group`
  (`:9495`) hands them to the transcript with the rows and kinds.
- **`theme.rs`: `Redraw`** (`:354`), the row as drawn in each mode, shared
  (`Arc`). **`document_retint_in(ToolGroup, _)` answers nothing** (`:615`):
  the 256-colour role list and the truecolor highlighter arm are gone. A
  cancelled call's own line keeps the token-table pairs at either depth
  (`ToolStopped`, unchanged), because fx rewrites that entry's bytes.
- **`transcript.rs`: a block carries the drawings** (`Op::Block`,
  `Append::redraws`), cut to the width the row is written at, and **writes a
  drawn-again row in the palette in force when it lands** (`prepare_block`,
  `:1044-1062`) -- the second break.
- **`grid.rs`: one record per row** (`Record { kind, redraw }`, `:210`),
  moved and cleared with the cells by scroll, collapse, erase, resize and
  every placement. `retint_document` (`:448`) puts the drawing for the
  reported mode back on a recorded row (`redraw_row`, `:499`): each cell
  takes the attributes the drawing gives it -- a fresh paint of the drawing,
  which for a tool row is its foreground alone -- **only while every cell of
  the row is the drawing's grapheme at the drawing's width**; a row that
  holds anything else is left as it is, so the retint still cannot create a
  cell. Rows without a drawing are recoloured as before.
- **`frame.rs`**: an append records each row's kind and drawing on the
  grid, on the row placed and on a row reused unchanged alike (`Recorded`,
  `:379`; `:1647`).

**Not dependent on `38;2` palette greys (TC-NOTATION).** A drawing is what
the group's own renderer writes in each mode; nothing reads a cell's
notation to decide anything on a tool row. When TC-NOTATION spells the
palette's greys `38;5` at every depth, the drawings change with them and the
redraw still writes exactly the group's rows. The only notation-dependent
rule left near this path is `painter_of` for the other row kinds (a stopped
call's line among them, answered by the token-table pairs in either
notation), which this unit did not touch. The truecolor unit case reads the
statusline's spelling off the palette rather than hard-coding `38;2`.

## Round 2: the review of `a765487d` (`7027e284`, `f482d340`)

Both engines reviewed `a765487d` (gpt6 conditionally ready, one PLAUSIBLE
MUST; opus APPROVE, SHOULDs). What each finding came to:

**1. A block queued before a report, drained after it (both engines).** The
rows of a group's block that are not drawn again -- a stopped call's own
line (`toolgroup.rs` `cancelled_line`) and the lines written under the open
group (`shell.rs` `trailer_rows`) -- were painted when queued and written as
queued, after the tick had paid the report's recolour (`event_loop.rs`
`commit_document`: the recolour first, then the appends). fx rewrites those
entries' bytes at the switch (`app_render_runtime.zig:499`,
`store.zig:3743-3760@481104fd`) before it asks for the build (`:511`).

- **RED** (`red2-unit.txt`, the new cases in `a765487d`'s src): four fail.
  - `a_stopped_call_and_a_notice_queued_before_a_report_land_in_the_palette_the_report_set`
    (dark to light): the call row, mark, `Cancelled`, argument, string, comment,
    notice label and body read `241 252 255 245 250 245 252 250` off the
    screen, where fx's token table gives `241 238 235 247 241 247 238 241`.
  - `…_queued_light_land_in_the_dark_palette_the_report_set` (light to dark):
    `245 238 235 245 241 243 238 241`, wanted `245 252 255 245 250 243 252 250`.
  - `a_stopped_call_queued_while_ctrl_o_is_up_lands_in_the_palette_the_report_set`
    (the appends held while Ctrl-O has the other plane, the switch, Ctrl-O
    closed): as the first.
  - `a_block_landing_behind_a_resume_in_another_palette_owes_no_recolour_of_the_old_screen`:
    the stopped line's mark `252`, wanted `238`.
- **The fix, `f482d340`.** `Op::Block` carries the mode its rows were
  painted in, and `prepare_block` writes a row with no drawing that was
  painted in the other mode through `recoloured` (`transcript.rs:1084`,
  `:1413-1460`). That replaces each foreground this crate spells --
  `38;5` or `38;2`, bold or not -- by what `document_retint_in` answers for
  it on a row of that kind, which is the rule `Grid::retint_document`
  applies cell by cell, and fx's rewrite of the bytes. So a stopped call's
  argument drawn dark is `247` on a light screen, the token table's, not
  the `245` a fresh light `cancelled_line` would give (opus's caution).
- **What `7027e284` did first, and why it went.** It re-armed the screen's
  recolour when such a block landed. That cannot be right behind a resume:
  there the recolour is dropped, because the shadow describes a screen
  somebody else has written on (`Shell::external_damage`). The resume case
  above shows both halves (`rearm-7027e284.txt`, the case run in
  `7027e284`'s src): with the guard, the block's stopped line stayed dark
  (mark `252`, wanted `238`); with the guard removed, the old screen's rows
  were repainted from the stale shadow. Writing the rows right as
  they land needs neither, and `commit_document` is `a765487d`'s again.
- **Orderings pinned** (event-loop cases, each reading the screen the band
  records once the tick has landed, `Band::shadow`, or -- the resume case and
  round 1's -- the bytes the tick wrote):
  - report, then appends of the group's rows (round 1's
    `a_tool_group_queued_before_a_report_lands_in_the_palette_the_report_set`);
  - report, then appends of a stopped line and a notice under the group, both
    ways (above);
  - the same held behind Ctrl-O (above);
  - report, then resume, then appends (above);
  - report, then a narrower screen, then the rewrite: the row lands cut with
    an ellipsis in the new palette, and the next switch draws the cut row
    again (`a_switch_then_a_narrower_screen_before_a_groups_rewrite_lands_cuts_both_drawings_there`);
  - the whole group closed by its answer and a report before any of it lands
    (`a_group_closed_and_a_switch_before_any_of_it_lands_is_drawn_in_the_new_palette`);
  - a rewrite queued before a report whose header the report already drew:
    the row is kept, not written again, and keeps the rewrite's drawings
    (`a_rewrite_queued_before_a_switch_keeps_the_rows_the_switch_drew_and_their_drawings`).

  The last three pass on `a765487d` too (`red2-unit.txt`): coverage, not a
  defect. Nothing here claims an ordering these cases do not pin.

**2. Coverage of switch with resize and close (gpt6):** the three orderings
above.

**3. `docs/parity.md`:** the queued-block sentence now says what is true
after 1: the header and call rows drawn in the palette in force, the other
rows recoloured as they are written, by the rules a switch recolours them by on the screen, a
stopped call's argument `247` and not `245`.

**4. The receipt's own claims (opus).** `mutants.txt` now holds the
unmutated run's line it promised (`479 passed`, from the run that printed
the mutants). The stopped-line pairs' row 8 is what the capture shows, not
"the marker only": `row8.txt` lists every differing cell -- the marker
(`▪`/`■`) and the product name in the follow-up question (`fx`/`xfx`),
both `cancelled-marker`'s row -- and the argument and comment cells the same
(ramp reading at `-tc`). The token-table pin in the unit tests is
`toolgroup.rs:1654-1658@f482d340` (`:1601-1605@a765487d`), and the queued
case is the event-loop cases above.

**5. The ledger: Ctrl-O pages scored.** `full-transcript-shift` no longer
takes `p5-toolgroup-ctrlo*` (it was `whole_snap`). The 256-colour Ctrl-O
pages are scored row by row: a new cause, `toolgroup-ctrlo-record-short`,
covers rows 34-37 alone (xfx's record four rows shorter: `session-record`'s
two lines and `tool-schema`'s two argument rows), and the group's own rows
are compared cell by cell with their styles, by the sweep's alignment. On
the `-tc` pages the rows in xfx's own greys differ in notation, so
`truecolor-grey-notation` (TC-NOTATION's) also names the Ctrl-O row shapes
it covers there (an entry's heading, the session and network records, a
command's output rail) by pattern, not whole snap; their colours are
compared by the ramp reading in `ctrlo-rows.txt` until TC-NOTATION removes
that cause. The sweep below has no uncaused row on any Ctrl-O page.

**6. Performance (opus, PLAUSIBLE): measured, then fixed.** A resume replays
every call of every group before anything is drained, and each call's start
and result redraws the whole group. Measured with `perf/instrument.py`
(counters added to a throwaway clone only: `ToolGroup::rows` calls, the
highlighter's calls, what the queue holds after a hydration, and the drain),
release build, three interleaved rounds (`perf/interleaved.txt`, table by
`perf/perftable.py`). One session of 20 turns of 10 or 50 calls, and one of 5
turns of 200, each call a highlighted command:

| session | build | blocks queued | bytes held (rows + drawings) | `rows()` calls | highlighter calls | hydrate ms (median of 3) | drain ms (median of 3) | bytes written by the drain |
|---|---|---|---|---|---|---|---|---|
| 20 turns x 10 calls | `58b463c8` | 400 | 519,800 | 400 | 2200 | 7 | 58 | 159,052 |
| 20 turns x 10 calls | `a765487d` | 400 | 1,559,400 | 1200 | 6600 | 21 | 85 | 159,052 |
| 20 turns x 10 calls | `f482d340` | 20 | 138,720 | 800 | 400 | 5 | 15 | 54,592 |
| 20 turns x 50 calls | `58b463c8` | 2000 | 11,602,200 | 2000 | 51000 | 107 | 510 | 754,872 |
| 20 turns x 50 calls | `a765487d` | 2000 | 34,806,600 | 6000 | 153000 | 438 | 786 | 754,872 |
| 20 turns x 50 calls | `f482d340` | 20 | 678,720 | 4000 | 2000 | 63 | 31 | 248,992 |
| 5 turns x 200 calls | `58b463c8` | 2000 | 45,354,820 | 2000 | 201000 | 416 | 881 | 734,162 |
| 5 turns x 200 calls | `a765487d` | 2000 | 136,064,460 | 6000 | 603000 | 1593 | 1060 | 734,162 |
| 5 turns x 200 calls | `f482d340` | 5 | 675,960 | 4000 | 2000 | 243 | 23 | 244,702 |

So round 1 did triple the base's work on this path -- 3x the `rows()` calls
and highlighter calls, 3x the bytes held -- as opus estimated. Round 2 holds
under 0.7 MB where the base held 45 MB and round 1 136 MB, calls the
highlighter less often than the base did (1/5 to 1/100: 2,000 against
201,000 at 5 x 200), and its hydration plus drain is faster than the base's
in every case (times under load 17-21).

- **Coalescing** (`transcript.rs:894`): a block queued behind one of the
  same open block that has not landed replaces it. A block at the back of the
  queue is always the same open block, because every other write closes it
  first. Pinned by `a_rewrite_queued_behind_one_not_yet_landed_replaces_it`.
  Queuing four rewrites with text before and after leaves the same document
  row for row, and the same kinds and drawings on the block's rows, as
  landing each rewrite in turn, and writes the block once.
- **One more mode, not two** (`toolgroup.rs:545`): `redraws` takes the rows
  `redraw` just drew and draws only the other mode.
- **A call keeps its command's colourings** (`toolgroup.rs:149,320`), per
  mode and run colour, at most five: the highlighter is a function of the
  command, the mode and the colour between words. Pinned by
  `a_calls_command_is_coloured_as_a_fresh_colouring_would_and_kept_per_mode_and_grey`:
  a group drawn again across seven switches and both depths equals a fresh
  group drawn each time.
- Not done: a lazy drawing of the other mode. With the colourings kept,
  `rows()` calls are 2x the base's and the highlighter's 1/5 to 1/100 of
  them, and the measured cost is below the base's.

**Mutants, round 2** (`mutants-r2.txt`, `mutate.py`; `f482d340` on a
throwaway clone; unmutated `1314 passed`): all 16 killed. A-I are round 1's
again at this head. The new ones:

| mutant | what it does | fails |
|---|---|---|
| R | a row painted in the other mode written as painted | 4 |
| S | a bold foreground recoloured without its bold | 1 |
| U | a direct (`38;2`) foreground not recognised | 1 |
| M | a superseded block kept | 1 |
| N | whatever is at the back of the queue replaced | 3 |
| O | a kept colouring found without its mode | 2 |
| P | a kept colouring found without its run colour | 2 |

## GREEN (`f482d340`): `green.txt`, `ctrlo-rows.txt`, `row8.txt`

| pairs (40x120) | snaps | result |
|---|---|---|
| `p5-toolgroup-retint`, `-light` | 6 | `0 rows differ` each (`-light`'s first take read its turn summary `1s` where fx's read `0s`, a wall clock under load 35, nothing else; taken again, identical: `retint-light-first-take.txt`) |
| `p5-toolgroup-retint-tc`, `-light-tc` | 6 | `ramp: 0`, `kept: 0` each |
| `p5-toolstop-retint`, `-light` | 6 | row 8 only, the call's own line (cause `cancelled-marker`, T-U4, which covers the row): `row8.txt` lists every cell that differs there -- the marker (`▪` against `■`) and the product's name in the follow-up question (`fx do differently?` against `xfx do differently?`), and nothing of the argument or the comment. (`green.txt`'s `cells.py` line shows a 4-cell window from the first differing column only.) |
| `p5-toolstop-retint-tc`, `-light-tc` | 6 | row 8 only, the same cells by the ramp reading (`row8.txt`); `kept: 0`. The `dark` snap of `-tc` also has fx's `press ctrl+c again to exit` on row 12, in this fx capture alone. The hint is transient on both products and the 3 s wait after Ctrl-C lands near its expiry: in round 1's head sweep both sides of that snap showed it, in the base sweep neither did |
| `p5-toolgroup-live`, `-tc` (new) | 8 | `light`, `dark`: identical / `ramp: 0`; `running-*`: row 8 only, the activity marker's blink phase -- in the sweep the `activity-pulse` normalization reads it, and the 256-colour pairs pass there |
| `p5-toolgroup-ctrlo`, `-tc` (new) | 12 | `dark`, `light`, `dark-again`: identical / `ramp: 0`; `full-*`: the tool group's two rows the same cell for cell (`ctrlo-rows.txt`: 12 of 12 at `f482d340`, as at the base); the rest of those pages is Ctrl-O's session record, the command's argument rows and the rows they shift, and the clock -- in the sweep `session-record`, `tool-schema`, `toolgroup-ctrlo-record-short` and, at `-tc`, `truecolor-grey-notation`, every other row compared; the clock masked by `transcript-timestamp` |

Unit cases added or rewritten: `tui::grid` --
`a_row_drawn_again_is_what_a_fresh_paint_of_its_drawing_leaves` (both ways,
cell for cell against a paint of the drawing; the mode in force moves
nothing), `a_drawing_that_is_not_the_rows_text_is_not_put_on_it`,
`a_rows_drawings_move_with_it_and_go_with_its_cells`; `tui::transcript` --
`a_row_drawn_again_lands_in_the_palette_in_force_with_its_drawings`,
`a_blocks_drawings_stay_with_their_rows_past_the_rows_it_froze`;
`tui::toolgroup` --
`the_groups_header_and_calls_carry_their_drawings_in_both_modes_and_nothing_else_does`
(both depths), and `a_theme_report_repaints_a_group_as_the_other_palette_would_draw_it`
now records the drawings and drops its "without a quoted string" caveat: a
string, a number and a comment, light to dark to light, against fresh
drawings; `tui::theme` -- `a_tool_groups_rows_answer_no_colour_at_either_depth`
replaces the two cases that pinned the deleted roles
(`a_truecolor_tool_rows_indexed_greys_are_the_highlighters_both_ways`,
`a_256_colour_tool_rows_light_comment_is_a_comment_and_nothing_else_moves`),
and the truecolor code case and the kind case lost their ToolGroup lines;
`tui::event_loop` -- the five cases above.

## Mutants, round 1 (`mutants.txt`, `mutate-r1.py`)

On a `git clone --no-local` throwaway in the scratchpad, never the worktree,
at `b6571bea`'s src. Unmutated: 479 passed (`mutants.txt`, its first result
line). Each mutant fails at least one case (round 2's run of A-I at
`f482d340`, with the wider test set, is in `mutants-r2.txt`):

| mutant | what it does | fails |
|---|---|---|
| A | a recorded row falls through to colour matching | 7 |
| B | the shape guard removed (a drawing put on any row) | 1 (`a_drawing_that_is_not_the_rows_text_is_not_put_on_it`) |
| C | a block written as queued, not in the palette in force | 3 |
| D | T-U3-TC's colour roles restored for a group's row | 2 |
| E | a cancelled call's own line drawn again too | 2 |
| F | a reused row keeps its kind but loses its drawing | 4 |
| G | a scroll drops the drawings | 6 |
| H | drawings not cut to the written width | 1 |
| I | the drawing of the old mode put back | 7 |

## Sweep

`scripts/fxcap/sweep.sh --jobs 2 --only 'p5-toolgroup-*' --only 'p5-toolstop-*'
--only 'p5-inline-code-*' --only 'p5-transcript-*'` at 40x120. The base's
build (`v0.1.0-58b463c`) ran in round 1. The head's ran in round 2 at
**`f482d340`** (`v0.1.0-f482d34`), at load 38-40. fx's sha256 was checked
before and after each run. Each run covers 42 scenarios and 169 snap pairs,
and ran 42 of 42. **Both depths** is read as the two colour depths: the
256-colour scenarios and their `-tc` twins (`COLORTERM=truecolor`) are in the
same sweep. The size is the brief's.

| sweep (scored with) | pass | identical | normalized-identical | approved-divergence | unresolved | pairs with uncaused rows |
|---|---|---|---|---|---|---|
| base, the base's ledger (`sweep-base-ledger0.md`) | 55 / 169 | 10 | 45 | 2 | 112 | 30 |
| base, this branch's ledger (`sweep-base.md`) | 55 / 169 | 10 | 45 | 2 | 112 | 25 |
| round 1's head `b6571bea`, this branch's ledger | 62 / 169 | 11 | 51 | 2 | 105 | 14 |
| **head `f482d340`, this branch's ledger** (`sweep-head.md`) | **63 / 169** | 10 | 53 | 2 | 104 | 13 |

Pair by pair, base against `f482d340` (`sweep-compare.txt`,
`cmp_sweeps.py`): **0 dropped.**

- **8 pairs pass on the head and not on the base.** Four are the clash snaps
  of `p5-toolgroup-retint`/`-light` (the base's `toolgroup-retint-256`). The
  other four are on the new scenarios: `p5-toolgroup-live` `running-light`
  and `dark`, and `p5-toolgroup-ctrlo` `light` and `dark-again`.
- **`p5-toolstop-retint`/`-light`**: the four clash snaps lose their
  uncaused rows and keep only `cancelled-marker` (T-U4's `▪`).
- **No uncaused row on any pair this unit's scenarios hold.** That includes
  the Ctrl-O pages, which round 2 scores row by row (Round 2, 5).
- **The 13 pairs with uncaused rows** (`p5-transcript-cancel`, `-md`,
  `-md-light`, `-question`) are uncaused at the base with its own ledger too,
  row for row.
- **Round 1's one worse pair, `p5-transcript-records` `start`**, passes in
  this sweep. It was a transient launch hint, a timing race
  (`records-recapture.txt`).

## The ledger and `docs/parity.md`

- `[[cause]] toolgroup-retint-256` removed: no pair it covered still differs
  in a cell it named (above, and the sweep).
- `truecolor-grey-notation` (TC-NOTATION's) also names `p5-toolgroup-live-tc`
  and `p5-toolgroup-ctrlo-tc`, the activity row a running command adds, and
  on a Ctrl-O page an entry's heading, the session and network records and a
  command's output rail, each by its row's pattern.
- `toolgroup-ctrlo-record-short` (new, round 2): rows 34-37 of
  `p5-toolgroup-ctrlo*`'s Ctrl-O pages, which xfx's shorter record leaves
  blank. Round 1 had put those pages under `full-transcript-shift`, which is
  `whole_snap`; they are scored row by row now (Round 2, 5).
- `docs/parity.md` (the colored-output row): a group's header and call rows
  are drawn again from the group's drawings, open or closed, at either
  depth, a block landing after a switch in the palette in force, Ctrl-O's
  rows from the session's record; the paragraph that recorded the 256-colour
  clash as TG-REDRAW's is gone. The stopped call's own line and the
  TC-NOTATION divergence are unchanged. Round 2: the queued-block sentence
  (Round 2, 3).

## Gates

Round 2: the suites one at a time, after the sweep and the measurements had
finished, `TMPDIR=/private/tmp/claude-501/tgr-gate2`, at `f482d340` with
this receipt, the ledger and `docs/parity.md` staged (no compiled file
differs from `f482d340`):

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| `cargo clippy --locked --all-targets --features fault-injection -- -D warnings` | ok |
| `cargo test --locked --no-fail-fast` | 5177 passed, 0 failed (15 binaries) |
| `cargo test --locked --features fault-injection --no-fail-fast` | 5214 passed, 0 failed |
| `scripts/check-xfx-identity.sh` / `check-no-stubs.sh` | ok (12223 tracked files) / ok |
| `scripts/check-no-secrets.sh` | ok (12223 tracked files) -- see below |
| `scripts/fxcap/test-sweep.sh` (the ledger changed) | 50 + 26 tests OK |

Round 1's, at `6640f42d` (code `b6571bea`): 5167 and 5204 passed, the rest
ok, 12213 tracked files.

**`check-no-secrets.sh` and the size of the listing.** With the `.txt`
renders as well (12345 tracked files) the check failed: `/usr/bin/grep:
Argument list too long`, nine times, one per pattern -- it hands every
tracked file, each as `./<name>`, to one `grep` (`grep_names`,
`scripts/check-no-secrets.sh:191-214`, the operands built at `:242`), and those operands came to more
than `ARG_MAX` (1048576). Measured as name bytes plus a pointer each: the
base's 12060 files leave 30545 bytes; this branch's 12223 (round 2) leave
15053. So the check fails for any later change that adds some 140 files of
names this long, whatever they hold. Not changed here: it is the repository's secret
gate, outside this unit (an open item in the report).

## Look-ahead

Past the fix, where a tool row could still come out wrong on a switch:

- **Lines written under an open group** (`trailer`, xfx's own cards and
  notices) are rows of the block drawn once; they are recoloured by colour
  as plain rows -- on the screen by a report, and as they land when they were
  painted before one (Round 2, 1). They hold no highlighter grey, so no
  clash.
- **Rows already on the screen when it is resized** lose their records, as
  every document row does on a resize (`Grid::resize`), so a later switch
  does not reach them; base did the same. The open block is placed again at
  the new width on its next change, with its drawings cut there
  (`a_switch_then_a_narrower_screen_before_a_groups_rewrite_lands_cuts_both_drawings_there`).
- **With ENGINE-VS-ANCHOR** (not this unit's; the dispatcher's train 3): a
  redrawn row holding a variation-selector unit goes through `Grid::diff`'s
  emission, which that unit rewrites.
- **A row the screen no longer holds as drawn** -- written over by
  something that did not record a new row -- is left as it is rather than
  recoloured; every writer this unit found records its row (`place_row`
  and `erase_row` clear the record).
- **Native scrollback** is never repainted, as before.
