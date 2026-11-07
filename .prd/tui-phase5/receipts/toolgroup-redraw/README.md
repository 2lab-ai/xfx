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

**Builds.** Code: `88e7c7b6` (the fix) and `b6571bea` (the new scenarios and
one more unit assertion, its only change under `src/`). Every `xfx/`
capture is from a debug build of `b6571bea` made after that commit (row 1:
`x𝒇x v0.1.0-b6571be [debug]`, 44 snaps, `green.txt`); every `red/` capture
from a debug build of `58b463c8` (`v0.1.0-58b463c`, 44 snaps, `red.txt`).
The commits after `b6571bea` change `docs/parity.md`,
`scripts/fxcap/parity-ledger.toml` and this directory, and no compiled file.
Each capture is kept as its `capture-pane -e` file (`.ansi`), which every
comparison here reads; the plain `.txt` render beside it is left out (Gates,
`check-no-secrets.sh`).

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

## What changed in xfx

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

## GREEN (`b6571bea`): `green.txt`, `ctrlo-rows.txt`

| pairs (40x120) | snaps | result |
|---|---|---|
| `p5-toolgroup-retint`, `-light` | 6 | `0 rows differ` each |
| `p5-toolgroup-retint-tc`, `-light-tc` | 6 | `ramp: 0`, `kept: 0` each |
| `p5-toolstop-retint`, `-light` | 6 | row 8 only: the marker `■`/`▪` (cause `cancelled-marker`, T-U4) |
| `p5-toolstop-retint-tc`, `-light-tc` | 6 | row 8 only (`cancelled-marker`); `kept: 0`. The `dark` snap of `-tc` also has fx's `press ctrl+c again to exit` on row 12, in this fx capture alone. The hint is transient on both products and the 3 s wait after Ctrl-C lands near its expiry: in the head sweep both sides of that snap show it, in the base sweep neither does |
| `p5-toolgroup-live`, `-tc` (new) | 8 | `light`, `dark`: identical / `ramp: 0`; `running-*`: row 8 only, the activity marker's blink phase -- in the sweep the `activity-pulse` normalization reads it, and the 256-colour pairs pass there |
| `p5-toolgroup-ctrlo`, `-tc` (new) | 12 | `dark`, `light`, `dark-again`: identical / `ramp: 0`; `full-*`: the tool group's two rows the same cell for cell (`ctrlo-rows.txt`, 12 of 12 at head, as at the base); the rest of those pages is Ctrl-O's session record, the command's argument rows and the rows they shift, and the clock (`session-record`, `tool-schema`, `full-transcript-shift`; the clock masked by `transcript-timestamp`) |

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

## Mutants (`mutants.txt`, `mutate.py`)

On a `git clone --no-local` throwaway in the scratchpad, never the worktree.
Unmutated: 479 passed. Each mutant fails at least one case:

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
--only 'p5-inline-code-*' --only 'p5-transcript-*'` at 40x120, once with the
base's build (`v0.1.0-58b463c`) and once with the head's (`v0.1.0-b6571be`),
one after the other; fx's sha256 checked before and after each. 42 scenarios,
169 snap pairs, ran 42 of 42 each time. **Both depths** is read as the two
colour depths: the 256-colour scenarios and their `-tc` twins
(`COLORTERM=truecolor`) are in the same sweep; the size is the brief's.

| sweep (scored with) | pass | identical | normalized-identical | approved-divergence | unresolved | pairs with uncaused rows |
|---|---|---|---|---|---|---|
| base, the base's ledger (`sweep-base-ledger0.md`) | 55 / 169 | 10 | 45 | 2 | 112 | 30 |
| base, this branch's ledger (`sweep-base.md`) | 55 / 169 | 10 | 45 | 2 | 112 | 25 |
| head, this branch's ledger (`sweep-head.md`) | **62 / 169** | 11 | 51 | 2 | 105 | 14 |

Pair by pair (`sweep-compare.txt`, `cmp_sweeps.py`):

- **Passed on the head and not on the base: 8** -- the four clash snaps of
  `p5-toolgroup-retint`/`-light` (the base's `toolgroup-retint-256`), and of
  the new scenarios `p5-toolgroup-live` `running-light`, `dark` and
  `p5-toolgroup-ctrlo` `light`, `dark-again`. `p5-transcript-submit`
  `submit-0.15` went from normalized-identical to identical.
- **`p5-toolstop-retint`/`-light`**: the four clash snaps lose
  `toolgroup-retint-256` and keep only `cancelled-marker` (T-U4's `▪`).
- **No uncaused row on any pair this unit's scenarios hold.** The new
  scenarios' rows that differ for causes already in the ledger are now named
  by them: `truecolor-grey-notation` takes `p5-toolgroup-live-tc` and
  `p5-toolgroup-ctrlo-tc` (and the activity row, `^~ Running \(`, xfx's own
  grey spelt `38;2` there), `full-transcript-shift` takes
  `p5-toolgroup-ctrlo*`'s Ctrl-O pages (whose tool rows `ctrlo-rows.txt`
  compares by text instead).
- **One pair is worse on the head: `p5-transcript-records` `start`**, a snap 4
  s after launch, before any tool runs: the hint row's transient `Full access
  enabled: … permission checks disabled` is on xfx's screen and not on fx's.
  It is transient on both products -- in the base sweep fx's and xfx's both
  showed it, in the head sweep fx's did not -- and the head binary captured
  again three times gives `0 rows differ` against the head sweep's fx capture
  each time, the notice gone (`records-recapture.txt`). A timing race at that
  snap, outside this unit's path (nothing here touches the hint row).
- The 13 other pairs with uncaused rows (`p5-transcript-cancel`, `-md`,
  `-md-light`, `-question`) are uncaused at the base with its own ledger too,
  row for row; this unit did not touch them.

## The ledger and `docs/parity.md`

- `[[cause]] toolgroup-retint-256` removed: no pair it covered still differs
  in a cell it named (above, and the sweep).
- `truecolor-grey-notation` (TC-NOTATION's) also names `p5-toolgroup-live-tc`
  and `p5-toolgroup-ctrlo-tc`, and the activity row a running command adds;
  `full-transcript-shift` names `p5-toolgroup-ctrlo*`. Both are rows of
  causes the ledger already had, on this unit's new scenarios.
- `docs/parity.md` (the colored-output row): a group's header and call rows
  are drawn again from the group's drawings, open or closed, at either
  depth, a block landing after a switch in the palette in force, Ctrl-O's
  rows from the session's record; the paragraph that recorded the 256-colour
  clash as TG-REDRAW's is gone. The stopped call's own line and the
  TC-NOTATION divergence are unchanged.

## Gates

The suites one at a time, after both sweeps had finished,
`TMPDIR=/private/tmp/claude-501/tgr-gate`, at `6640f42d` with this receipt
and the ledger staged (no compiled file differs from `b6571bea`):

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| `cargo clippy --locked --all-targets --features fault-injection -- -D warnings` | ok |
| `cargo test --locked --no-fail-fast` | 5167 passed, 0 failed (15 binaries) |
| `cargo test --locked --features fault-injection --no-fail-fast` | 5204 passed, 0 failed |
| `scripts/check-xfx-identity.sh` / `check-no-stubs.sh` | ok (12213 tracked files) / ok |
| `scripts/check-no-secrets.sh` | ok (12213 tracked files) -- see below |
| `scripts/fxcap/test-sweep.sh` (the ledger changed) | 50 + 26 tests OK |

**`check-no-secrets.sh` and the size of the listing.** With the `.txt`
renders as well (12345 tracked files) the check failed: `/usr/bin/grep:
Argument list too long`, nine times, one per pattern -- it hands every
tracked file, each as `./<name>`, to one `grep` (`grep_names`,
`scripts/check-no-secrets.sh:191-214`, the operands built at `:242`), and those operands came to more
than `ARG_MAX` (1048576). Measured as name bytes plus a pointer each: the
base's 12060 files leave 30545 bytes; this branch's 12213 leave 15737. So
the check fails for any later change that adds some 150 files of names this
long, whatever they hold. Not changed here: it is the repository's secret
gate, outside this unit (an open item in the report).

## Look-ahead

Past the fix, where a tool row could still come out wrong on a switch:

- **Lines written under an open group** (`trailer`, xfx's own cards and
  notices) are rows of the block drawn once; they are recoloured by colour
  as plain rows, as before. They hold no highlighter grey, so no clash.
- **A row the screen no longer holds as drawn** -- written over by
  something that did not record a new row -- is left as it is rather than
  recoloured; every writer this unit found records its row (`place_row`
  and `erase_row` clear the record).
- **Native scrollback** is never repainted, as before.
