---
title: 0.0.16 delta -- AT-HEIGHT receipt (I-U1, the @ list's height and scroll)
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, input, file-picker]
---

# AT-HEIGHT (I-U1): fx v0.0.16's `@` list height and scroll

Spec: the research branch's `.prd/tui-phase5/research/input.md` §2.1, §3 rules
I1 and I2, §5 U1, read as leads. Oracle: fx v0.0.16 (`481104fd`, executable
sha256 `8e6cda2e…`, guarded by `capture.sh` and `sweep.sh` before and after
every run, `FX_AUTO_UPGRADE=0`). Base: `main` `712dd8d1`. Branch
`feat/p5-at-height`. Every claim below names the file in this folder that
backs it.

| commit | what |
|---|---|
| `856bc5cb` | RED: fx's height table and a through-the-frames scroll case, alone; the scenarios `p5-input-at-height` and `p5-input-at-resize` and their 30-file workspace |
| `9d1e116d` | the fix, and **the code head**: the height, the painted-rows scroll, the banner term, `docs/parity.md`, the ledger's `shell.rs` refs re-pointed |
| `ed7aa76f` | test only: the return from Ctrl-O is a frame the arrows scroll (pins the hook in `restore_primary`) |
| `ebd4be44` | test only: the height over every screen, composer and banner (`height-table.txt`) |
| `d32c6991` | test only: the painted rows outlive their `@` word, and only the list's frames set them |
| `af699911` | the ledger's three causes (`at-ranking`, `at-folder-row`, `at-resize-band-at-the-bottom`) |
| `ee0959a7` | test only: a frame that gave the list no rows does not set them (a surviving mutant, run 1) |
| (receipt commit) | this folder |

**Builds.** The base's debug build (`v0.1.0-712dd8d`, sha256 `fd09f29c…`,
built in a `--no-local` clone at `712dd8d1`) and the head's (`v0.1.0-9d1e116`,
sha256 `2bd67696…`, built after the commit `9d1e116d`). Row 1 of every capture
in `xfx-base/` and `xfx/` names its build (`pairs.txt`, first line). The
commits after `9d1e116d` change tests, the ledger and this folder, never the
binary's code.

## What fx v0.0.16 does (every line quoted in `fx-src.txt`, made by `fx_src.sh`)

1. **The height** (a5fe00d7). `filePickerReservedRows(rows, input_extra,
   banner_rows)` is `max(list, grown)`
   (`src/ui/footer/picker_presentation.zig:654-666@481104fd`):
   - `list` is the list picker's height, `activeListPickerReservedRows`
     (`:647-652`): with `fixed = 5 + input_extra + banner_rows`, the rows
     past `fixed`, at most six (`list_window.zig:3`), and 1 when there are
     none;
   - `grown` is `min(10, rows - fixed - 5)`: ten rows (`:656`) while five
     transcript rows stay visible (`:663`).
   - It is the band's height only while a file query is up
     (`surface_frame.zig:661-662`). Before, the `@` list took
     `list_picker_rows` (`surface_frame.zig:636-641@c1db9191`, `:634-639`).
   - `banner_rows` is a waiting prompt's rows and one composer gap
     (`render_input.zig:540-551`; fx's own test, 3-line message → 3), clamped
     to the screen (`surface_frame.zig:356-369`).
   - fx's own table (`:1588-1597`): 40 and 24 → 10, 18 → 8, 16 and 12 → 6,
     6 → 1, (20, 2, 4) → 6. The whole table over terminal rows × composer
     rows × banner, with the status-row column, is `height-table.txt`, made by
     `height_table.py` from a line-for-line transcription, which asserts fx's
     seven cases first.
2. **The scroll** (99e62b8e). The arrows move the bar with
   `advanceSelectionWithin(…, visible_rows)` (`file_completion_state.zig:
   239-249`, `list_window.zig:44-53`): past either end to the other end, and
   the window edge-scrolls (`updateEdgeStart`, `:55-68`): it moves only when
   the bar leaves it. `visible_rows` is the painted rows of the last committed
   frame that showed the file picker (`app_render_runtime.zig:1854-1859`,
   `paintedFilePickerReceipt` `:2355-2361`, `acknowledge`
   `file_completion_state.zig:294`). It is six until a frame lands
   (`:77`, `:108`), and it lives in the picker's state, which only `deinit`
   resets (`:104-110`), so it carries from one `@` word to the next. A changed
   query resets only the bar and the window, to 0 (`reconcileFilePicker`,
   `input_completion_runtime.zig:797`; `resetFilePickerIndex`,
   `picker_state.zig:249-252`).
3. **The status row.** `filePickerItemRows` takes the first row for a status
   line (`picker_presentation.zig:668-672`), and the frame paints it there
   (`paint_plan.zig:928-935`). Over rows, the only status is `.stale`:
   `Selection unavailable. navigate to choose; tab to retry.`
   (`app_render_runtime.zig:647-654`).
4. **A resize.** A frame paints the window from the stored start, re-fitted
   to keep the bar in view of the rows it has (`paint_plan.zig:936-937`). It
   does not write that start back: `view` hands the frame the stored window
   (`file_completion_state.zig:275`), and `acknowledge` stores the same value
   (`:293`). So on a shorter screen the next move scrolls from where the last
   move left the window, by the rows the new frame painted.
5. **PgUp and PgDn** go through the same `navigate` with a delta of
   `inputRowLimit(content_bottom)` rows (`input_completion_runtime.zig:
   215-233`, `:939-944`). That is unchanged since `c1db9191`; only the rows
   it scrolls within are new.

## What xfx does now (code head `9d1e116d`)

- `FilePicker::height(rows, input_extra, banner_rows)` is fx's formula
  (`src/tui/file_picker.rs`, `height`). The band asks it with the try's
  waiting-prompt rows plus the gap row, or 0 when the try shows none
  (`Shell::fit`, the `Slot::Files` arm). The existing clamp stays: the list
  gives way to a running turn's row, which xfx's band holds above the composer
  and fx holds outside its footer.
- `FilePicker::step(delta, painted)` edge-scrolls the rows it is given. The
  shell keeps `file_rows_painted` (six until a frame paints the list, kept
  across `@` words). `Shell::file_picker_landed` sets it from the geometry's
  panel rows when a frame that showed the `@` list lands: in `landed_band`
  and in `restore_primary`'s success arm (the return from Ctrl-O).
- `FilePicker::rows` still re-fits the painted window from the stored start
  without writing it back. `settle` no longer writes back a re-fitted start
  either; at the base it re-fitted to six rows.
- No status row over rows: xfx has no `Selection unavailable` state. A refresh
  that loses the marked path puts the bar back on the first row. So the
  list's band rows are its file rows, and `docs/parity.md`'s `@` row records
  it as narrower than upstream's.

## RED (`856bc5cb`, `red-unit.txt`; the pairs in `pairs.txt` and `band.txt`, base column)

- `the_list_grows_to_ten_rows_while_five_transcript_rows_stay_visible`:
  `FilePicker::height(40, 0)` is 6, not 10.
- `the_at_list_scrolls_by_the_rows_the_frame_that_landed_painted` (event
  loop, real frames, 30 tie files at 40x120): the list is 6 rows, not 10.
  Its later steps (the window on 4-13 after Down x12, kept after Up x6,
  moved after Up x9; at 16 rows, Up scrolls the six rows painted) are the
  capture pairs' claims too.
- The base's pairs: `p5-input-at-height` differs at 40 and 18 rows (6 to 12
  rows a snap) and is identical at 16 and 12, where fx keeps six rows too.
  `p5-input-at-resize down12` differs by 12 rows. `band.txt` (base) fails 15
  checks: the list's height and the bar's row at 40 and 18 rows, and the
  resize band after the first Up and after the grow.

## GREEN (`pairs.txt`, `band.txt`; captures in `fx/`, `xfx/`, `xfx-base/`)

- **Identical, cell for cell** (cells.py, row 1 aside): every
  `p5-input-at-height` snap (`at`, `down12`, `up6`, `up9`, `pgup`) at 40x120,
  18x120, 16x120 and 12x120 (20 pairs), and `p5-input-at-resize down12`. The
  names tie on both rankings, so the rows are the same paths. At 40: ten
  rows; after Down x12 the window is on 4-13 with the bar on the last row;
  after Up x6 the window stays and the bar is on f-07; after Up x9 the window
  moves. At 18: eight rows, window 6-13. At 16 and 12: six.
- **The band's shape, where the rows cannot agree** (`band.txt`, head: 77
  checks, 0 failed):
  - p5-input-at's `@st` triple and `@src/` at 40, 18 and 16 rows: the same
    composer row, list height (10, 8, 6), bar row and hint row as fx. Each
    side's window is the edge-scrolled one over its own ranking: a rank shows
    one path across the three snaps, on both sides. So fx's captures check
    the rule too.
  - `p5-input-at-resize` after the resize: the band, from the composer row to
    the hint row, is fx's cell for cell (`short`, `short-up1`, `short-up7`,
    `tall`, `tall-down1`). `short-up1` is the discriminating one: the window
    left at rank 3 by the 10-row moves scrolls six rows, to 7-12, where the
    base showed 8-13.

## The sweep (`sweep-base.md`, `sweep-head.md`, `sweep-compare.txt`)

`sweep.sh` against fx v0.0.16 at 40x120, `--jobs 2`, under the suite lock,
over the 24 picker and band scenarios: `p5-input-*` (every input-cluster
scenario, the two new ones included), `filepicker*`, `composer*`,
`workspace-index`, `startup`, `h1-resize`, `model-column`,
`settings-scrollback`, `p5-mcp-config`, `p5-mcp-verbs` (the scenarios whose
`.cap` types an `@`, the core set's band and menu scenarios, and the
composer's). The base's build, then the head's. Both are rescored with this
receipt's ledger, so they read the same causes.

- **pass 64/146 → 80/146.** 16 pairs move to normalized-identical, and **none
  drops**. The 16 are every `p5-input-at-height` snap,
  `p5-input-at-resize down12`, `filepicker` `at`, `at-sch`, `at-claim`,
  `filepicker-keys` `at-pgdn`, `at-ck`, `filepicker-narrow` `at-alpha`,
  `at-alpha-down`, `at-deep`, `p5-input-at-dir dir-query` and
  `workspace-index at-all`.
- **Named, not counted.** On the head no snap of a picker, `@` or composer
  scenario is left without a cause. On the base, 16 were: the 16 above. The
  rows that stay apart are:
  - `at-ranking` (I-U2), 5 snaps: `p5-input-at` `at-st`, `at-st-down12`,
    `at-st-up6`, `at-stat`, and `workspace-index at-plan`. The last is the
    same text with other letters bold on a long path, because fx rebuilds
    the highlight from the placement that set the rank.
  - `at-folder-row` (I-U3), 8 snaps: `p5-input-at` `at-src`, `at-src-enter`,
    `at-src-tab`; `p5-input-at-dir dir-enter`; `filepicker` `at-dir`,
    `at-dir-down`, `at-dir-cj`, `at-tab`.
  - `at-resize-band-at-the-bottom` (RESIZE-REPAINT), 5 snaps: the resize
    scenario after the resize.
- The other unresolved pairs (`p5-input-approval`, `p5-input-model`,
  `p5-input-paste`, the two `p5-mcp-*`, and the snaps other causes name) have
  the same verdict on the base and the head.

## Mutants (`mutants.py`, `mutants-run1.txt`, `mutants-run2.txt`)

Each rule broken alone in a `git clone --no-local` throwaway at the code's
head, never in the worktree. The unit's tests run against it in their own
process group, with a 900 s limit (`mutants.py`, `TESTS`, `LIMIT_S`). Under
the suite lock.

- **Run 1** at `d32c6991`: 15 run, 14 killed.
  - `landed-no-panel-guard` survived. It drops `geometry.panel > 0` from
    `file_picker_landed`, which is fx's `picker_rows > 0`. Pinned by
    `ee0959a7`.
  - `landed-band-forgets` did not apply: its text was written wrong.
- **Run 2** at `ee0959a7`: the two again, the second's text fixed.
  2 run, 2 killed. Over both runs, all **16 mutants are killed**.
- Every mutant, the test that kills it:
  - the ten (`ten-is-six`), the five transcript rows
    (`no-transcript-floor`), `max(list, grown)` either half (`grown-only`,
    `list-only`) and the never-none floor (`never-none-gone`): the height
    table and grid tests.
  - The banner in the formula (`banner-ignored-in-height`), the band passing
    none (`band-passes-no-banner`) or no gap row
    (`band-banner-without-gap`): `a_waiting_prompts_rows_come_out_of_the_at_lists_growth`.
  - The arrows scrolling six (`step-scrolls-six`) or ignoring the rows they
    are given (`step-ignores-rows`): the event-loop and `file_picker`
    navigation cases.
  - The two landing hooks deleted (`landed-band-forgets`, `restore-forgets`),
    any slot's frame setting the count (`landed-any-slot`), the count reset
    per word (`reset-per-word`), the panel guard dropped
    (`landed-no-panel-guard`).
  - A refresh re-fitting the window (`refresh-refits-window`):
    `a_refresh_leaves_the_window_where_the_last_move_left_it`.

## Unit tests (in the gate's `cargo test`)

| test | pins |
|---|---|
| `file_picker::the_list_grows_to_ten_rows_while_five_transcript_rows_stay_visible` | fx's table, `picker_presentation.zig:1588-1597` |
| `file_picker::the_height_is_fxs_for_every_screen_composer_and_waiting_prompt` | `height-table.txt`, 252 cells |
| `file_picker::below_six_rows_the_list_takes_what_the_screen_has_and_never_none` | the list floor, saturation |
| `file_picker::the_arrows_scroll_within_the_rows_the_last_frame_painted` | fx's `file_completion_state.zig:319-350` |
| `file_picker::the_window_scrolls_only_past_the_rows_painted` | fx's `list_window.zig:70-85` |
| `file_picker::a_frame_paints_the_rows_that_keep_the_mark_in_view_and_keeps_the_window` | the resize re-fit |
| `file_picker::a_refresh_leaves_the_window_where_the_last_move_left_it` | `acknowledge`'s write-back |
| `event_loop::the_at_list_scrolls_by_the_rows_the_frame_that_landed_painted` | RED; `landed_band` |
| `event_loop::the_frame_that_gives_the_plane_back_is_one_the_at_lists_arrows_scroll` | `restore_primary` |
| `event_loop::the_rows_painted_are_kept_from_one_at_word_to_the_next_and_only_the_lists_frames_set_them` | the count's lifetime and its one writer |
| `shell::a_frame_that_gave_the_list_no_rows_leaves_the_rows_its_arrows_scroll` | fx's `picker_rows > 0` |
| `shell::a_waiting_prompts_rows_come_out_of_the_at_lists_growth` | the banner term: 10 rows alone, 8 past a one-row prompt at 20 rows |
| `shell::an_at_opens_…`, `shell::a_query_narrows_…` | updated: ten rows (`filepicker`'s `at`, `at-sch` at 40x120 on fx) |

## The gate (`gate.txt`, `gates.sh`)

Every CI step, in a `--no-local` clone at `ee0959a7` (the last commit that
touches code or tests), the whole run in one hold of the suite lock, with
`TMPDIR=<scratch>/at-height`. **20 steps, 0 failed**, the tree
clean before and after:
- fmt, clippy, and clippy with fault injection;
- `cargo test`: 5457 passed, 0 failed. With fault injection: 5494 passed,
  0 failed;
- the identity, stubs, secrets, preview-contract, preflight-cost and
  usage-clock checks, and their self-tests;
- the fxcap sweep and capture self-tests;
- both release builds;
- smoke, and smoke-tui: 37 scenarios and the oracle, 1168 checks,
  0 failures.

The receipt commit after it adds only this folder.

## docs/parity.md

The `@` file picker row: the height (rule, table, the turn's-row clamp), the
scroll (the painted rows, their lifetime, the resize re-fit), and one narrower
point added (no `Selection unavailable` row). The receipt pointer names this
folder.

## Interactions and what is not this unit's

- **I-U2 (ranking)** owns every row difference left in `p5-input-at`'s
  `at-st`, `at-st-down12`, `at-st-up6`, `at-stat` and `filepicker`'s
  `at-sch`-like queries: ledger cause `at-ranking`. No code interaction:
  the scroll works over whatever order the index returns.
- **I-U3 (folders)** owns `at-src`, `at-src-enter`, `at-src-tab`: ledger
  cause `at-folder-row`. When U3 adds the folder's own row, that row is one
  more list row, inside the same height.
- **I-U4 (index without git)**: none.
- **RESIZE-REPAINT** owns where the band stands after a resize (fx repaints
  from the top, xfx solves the band at the bottom): ledger cause
  `at-resize-band-at-the-bottom`, beside `resize-document-repaint`.
- **The status row** (`Selection unavailable`) is a pre-existing gap, at
  `c1db9191` too, with no owner. It is now written in `docs/parity.md`. If a
  unit ports the stale state, `file_picker_landed` must subtract the status
  row (`filePickerItemRows`).
- **PgUp and PgDn** keep xfx's page of `layout::input_row_limit(rows)` (the
  composer's row cap, `content_bottom = rows - 3`); fx's is
  `inputRowLimit(layout.content_bottom)`. This unit did not change or measure
  the page. Only the rows it scrolls within changed. The `pgup` pairs are
  identical at every size, but they wrap to the end on any page over 3, so
  they do not measure the page.
