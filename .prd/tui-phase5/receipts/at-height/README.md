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
| `9d1e116d` | the fix, round 1's code head: the height, the painted-rows scroll, the banner term, `docs/parity.md`, the ledger's `shell.rs` refs re-pointed |
| `ed7aa76f` | test only: the return from Ctrl-O is a frame the arrows scroll (pins the hook in `restore_primary`) |
| `ebd4be44` | test only: the height over every screen, composer and banner (`height-table.txt`) |
| `d32c6991` | test only: twelve Downs typed with a new word scroll ten rows -- an xfx-only path, replaced in round 2 by `bbf7dd93` |
| `af699911` | the ledger's three causes (`at-ranking`, `at-folder-row`, `at-resize-band-at-the-bottom`) |
| `ee0959a7` | test only: a frame that gave the list no rows does not set them (a surviving mutant, run 1) |
| `42cfea80`, `4910f08a` | round 1's receipt; the dispatcher's `<scratch>` edit |
| **round 2** | |
| `bbf7dd93` | RED: the `trusted` gate (arrows, Tab, Enter on no match, a refused frame) and a refresh that finds the mark on row 0 |
| `5e34287d` | the gate and the window rule |
| `efb3ee94` | test only: a changed letter of the word; a refused write keeps the count; the grid test renamed; the property test; the Up x9 comment (MUST 2) |
| `a0f1ce6d` | docs: `docs/parity.md`'s row, `file_rows_painted`'s doc, `fit`'s banner comment, the module doc |
| `6b8fdbf9` | ledger: the refs re-pointed and checked by text (MUST 1, `refs.py`) |
| `0fdeae02` | the list takes no key under the full transcript; one distrust per way off the screen -- **the code head** |
| `79b81967` | ledger refs at `0fdeae02`; scenario `p5-input-at-gate` -- **the head's build** |
| `e49db9cd` | test only: a resize spends the keys until its frame; a frame without the list |
| `a4914d8e` | ledger: the refs at the final code and tests (`refs.txt`: 16 checked, 0 wrong) |
| (receipt commit) | this folder |

**Builds.** The base's debug build (`v0.1.0-712dd8d`, sha256 `fd09f29c…`,
built in a `--no-local` clone at `712dd8d1`) and the head's, built after the
commit `79b81967` (`v0.1.0-79b8196`, sha256 `da4f6eae…`). Row 1 of every
capture in `xfx-base/` and `xfx/` names its build (`pairs.txt`, first line).
Round 1's head build was `v0.1.0-9d1e116`; every `xfx/` capture here is round
2's. The commits after `79b81967` change tests, the ledger and this folder,
never the binary's code.

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
   (`:77`, `:108`), and it lives in the picker's state: only `deinit` resets
   it (`:121-128`); `invalidate`, `distrust` and `reconcile` leave it
   (`:130-143`, `:147-196`). A changed query resets only the bar and the
   window, to 0 (`reconcileFilePicker`, `input_completion_runtime.zig:797`;
   `resetFilePickerIndex`, `picker_state.zig:249-252`). No key can read the
   count across words, though: a new word's keys wait for its first frame
   (item 6), which sets it.
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
   it scrolls within are new. **The page delta itself is not measured by this
   unit and is owned by INPUT-ROW-LIMIT** (gpt6's MUST): xfx's page is
   `layout::input_row_limit(rows)` (`src/tui/layout.rs:334-337`), which is
   also the composer's cap and every menu's page, so it is fixed there for
   all of them. Every `pgup` pair here wraps to the end on any page over 3
   rows, so none of them measures the page.
6. **The keys act only on rows a frame showed** (round 2; `trusted`,
   `file_completion_state.zig:104`, all quoted in `fx-src.txt`):
   - **read by** `navigate` (`:239-240`: the arrows and page keys are spent),
     `selected` (`:218-223`: Tab and Enter choose nothing;
     `input_completion_runtime.zig:946-966`, `:1034-1041`) and
     `acceptedEmpty` (`:225-228`: Enter on a query nothing matched submits
     only once a frame showed `no matching files`);
   - **cleared by** `reconcile` on a changed query (`:175`), `invalidate`
     when the picker closes (`:133`; `picker_state.zig:196-213`), and
     `distrust` (`:141-143`): a frame not committed
     (`app_render_runtime.zig:1002-1003`), a committed frame with no file
     picker on it (`:1024-1026`), and at each key while the picker is not
     eligible (the full transcript, a question, an approval, the auth picker,
     the catalog), a resize is pending, or the terminal must be reset or
     no layout fits it (`input_completion_runtime.zig:775-809`;
     `resize_runtime.zig:657-662`);
   - **set by** `acknowledge` only (`:295`), from a committed frame whose
     receipt shows the picker (`app_render_runtime.zig:1022-1023`,
     `:1854-1857`).
   - `stage` (new rows for the same query) leaves it, and the keys then act
     on the rows the last frame presented, not the new ones (`current`,
     `:212-216`, `:241`).
   - fx's own capture shows it: `fx/fx-p5-input-at-gate-40x120-*`. Three
     Downs and a Down-Tab written with a new word leave the bar on f-01 and
     choose nothing.
7. **A refresh keeps the stored window** where it finds the marked path,
   on row 0 too: `view` maps the mark and hands the frame the stored start,
   and `acknowledge` writes back the same (`:258-275`, `:292-293`). After a
   lost mark the next move restarts it at 0 (`:244-246`).

## What xfx does now (code head `0fdeae02`)

- `FilePicker::height(rows, input_extra, banner_rows)` is fx's formula
  (`src/tui/file_picker.rs`, `height`). The band asks it with the try's
  waiting-prompt rows plus the gap row, or 0 when the try shows none
  (`Shell::fit`, the `Slot::Files` arm). The existing clamp stays: the list
  gives way to a running turn's row, which xfx's band holds above the composer
  and fx holds outside its footer.
- `FilePicker::step(delta, painted)` edge-scrolls the rows it is given. The
  shell keeps `file_rows_painted`, six until a frame paints the list.
  `Shell::file_picker_landed` sets it from the geometry's panel rows when a
  frame that showed the `@` list lands: in `landed_band` and in
  `restore_primary`'s success arm (the return from Ctrl-O).
- **The gate (round 2).** `FilePicker` carries `trusted`, false for every new
  `FilePicker`; a changed query is a new one.
  - Set by `file_picker_landed` when the frame showed the list.
  - Cleared by:
    - `file_picker_landed` when the frame landed without it;
    - `approval_write_failed` (every refused or torn band write);
    - the frame that takes the other plane;
    - `Shell::resize`;
    - each key while a resize is signalled and not answered, or while the
      full transcript is up.
  - While it is false, `step` returns at once, `chosen()` (Tab and Enter's
    row) is `None`, and `enter()` is `Wait`, also for a query nothing
    matched. Tab on an unreadable list still asks again, as fx's does.
- `FilePicker::rows` still re-fits the painted window from the stored start
  without writing it back. `settle` does not write back a re-fitted start
  either (at the base it re-fitted to six rows). It keeps the stored start
  when the marked path is found, on row 0 too (round 2; the base put it back
  to 0 there), and restarts it only when the path is gone.
- No status row over rows: xfx has no `Selection unavailable` state. A refresh
  that loses the marked path puts the bar back on the first row. So the
  list's band rows are its file rows, and `docs/parity.md`'s `@` row records
  it as narrower than upstream's.

## RED (`856bc5cb`, `red-unit.txt`; the pairs in `pairs.txt` and `band.txt`, base column)

- `the_list_grows_to_ten_rows_while_five_transcript_rows_stay_visible`:
  `FilePicker::height(40, 0)` is 6, not 10.
- `the_at_list_scrolls_by_the_rows_the_frame_that_landed_painted` (event
  loop, real frames, 30 tie files at 40x120): the list is 6 rows, not 10.
  Its later steps (the window on 4-13 after Down x12, kept after Up x6 and
  after Up x9, when the bar reaches its top row; at 16 rows, Up scrolls the
  six rows painted) are the capture pairs' claims too.
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
  after Up x6 the window stays and the bar is on f-07; after Up x9 the bar
  reaches the window's top row, f-04, and the window still stays (round 1
  said it moves: wrong, opus MUST 2). At 18: eight rows, window 6-13, and
  after Up x9 it moves to 4-11. At 16 and 12: six.
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

## Round 2: RED (`bbf7dd93`, `red-unit-r2.txt`) and GREEN

**RED.** The four tests below, run at `bbf7dd93` on round 1's code: 4
failed, by assertion.

- `a_new_at_words_arrows_wait_for_the_frame_that_shows_its_rows`: a new word
  and three Downs in one read put the bar on f-04, where fx's stays on f-01.
- `a_refused_frame_leaves_the_at_lists_arrows_waiting_for_one_that_lands`:
  the Down after a refused frame moved the bar (f-03, fx f-02).
- `tab_and_enter_before_a_new_words_first_frame_choose_nothing_and_send_nothing`:
  Tab typed with `@s` and a Down completed `@src/deep/sched.rs `, where fx
  keeps `@s`.
- `a_refresh_that_finds_the_mark_on_the_first_row_keeps_the_window`: the
  window was put back to 0, and fx keeps 3. **opus's PLAUSIBLE SHOULD 3
  reproduces**, so it is fixed.

**GREEN, cell for cell** (`pairs.txt`, the `p5-input-at-gate` rows):

| pair | base | head |
|---|---|---|
| `downs-in-the-read` | 8 rows differ | identical |
| `downs-after` | 12 | identical |
| `tab-in-the-read` | 14 | identical |

`p5-input-at-gate` writes a new word and its keys in one write after the
index has loaded. fx's own captures show the gate: the bar stays on f-01
and the Tab chooses nothing (`fx/fx-p5-input-at-gate-*`).

Every round-1 pair stays as it was on the head: 24 of 50 identical, with
`band.txt` at 77 of 77 on the head and 15 failures on the base.

## Mutants (`mutants.py`; `mutants-run1.txt`, `mutants-run2.txt`, round 1; `mutants-run3.txt`, round 2)

Each rule broken alone in a `git clone --no-local` throwaway, never in the
worktree. The unit's tests run against it in their own process group, with
a 900 s limit (`mutants.py`, `TESTS`, `LIMIT_S`), under the suite lock.

- **Round 1**, at `d32c6991` and `ee0959a7`: all 16 killed over two runs.
  - In run 1, `landed-no-panel-guard` survived; it is pinned by `ee0959a7`.
  - `landed-band-forgets`' text was written wrong in run 1 and corrected in
    run 2.
- **Round 2**, at `e49db9cd`: **30 run, 28 killed.** That is round 1's 16
  at the new code, plus 14 new:
  - the gate (12): a new word trusted; `step`, `chosen` and `Submit`
    ignoring trust; Tab taking the marked row; the landing never trusting
    and never distrusting; a refused write, the other plane, a resize, a
    pending resize or the full transcript keeping trust;
  - the window rule (2): the reset at row 0 put back, the reset for a lost
    mark deleted.
- **The two that survive are equivalent** (`equivalent-mutants.txt`):
  - `reset-per-word`: the count is unobservable across words, because a new
    word's keys wait for its first frame, which sets it. This is opus's
    SHOULD 1 point, and it holds in fx too.
  - `landed-any-slot`: no surface outranks a live `@` list. A question
    dismisses it, and a throwaway probe of Ctrl-P, `/provider @s` and
    `/mcp show @s` found no list under any of them.

## The sweep (`sweep-base.md`, `sweep-head.md`, `sweep-compare.txt`)

Round 2's run: `sweep.sh` against fx v0.0.16 at 40x120, `--jobs 2`, under
the suite lock, over 25 picker and band scenarios. These are `p5-input-*`
(every input-cluster scenario, with this unit's three), `filepicker*`,
`composer*`, `workspace-index`, `startup`, `h1-resize`, `model-column`,
`settings-scrollback`, `p5-mcp-config` and `p5-mcp-verbs`: every scenario
whose `.cap` types an `@`, plus the core set's band and menu scenarios and
the composer's. It ran on the base's build, then the head's
(`v0.1.0-79b8196`). Both are rescored with the ledger at `a4914d8e`, so they
read the same causes.

- **pass 64/149 → 83/149.** 19 pairs move to normalized-identical, and
  **none drops**:
  - every `p5-input-at-height` snap, and `p5-input-at-resize down12`;
  - the three `p5-input-at-gate` snaps;
  - `filepicker` `at`, `at-sch`, `at-claim`;
  - `filepicker-keys` `at-pgdn`, `at-ck`;
  - `filepicker-narrow` `at-alpha`, `at-alpha-down`, `at-deep`;
  - `p5-input-at-dir dir-query`, `workspace-index at-all`.
  Round 1's run was 64/146 → 80/146, without the gate scenario.
- **Named, not counted.** On the head no snap of a picker, `@` or composer
  scenario is left without a cause; on the base, 19 were, the 19 above. The
  rows that stay apart:
  - `at-ranking` (I-U2), 5 snaps: `p5-input-at` `at-st`, `at-st-down12`,
    `at-st-up6`, `at-stat`; `workspace-index at-plan`, where only the bold
    letters differ, on a long path, because fx rebuilds the highlight from
    the placement that set the rank;
  - `at-folder-row` (I-U3), 8 snaps: `p5-input-at` `at-src`,
    `at-src-enter`, `at-src-tab`; `p5-input-at-dir dir-enter`; `filepicker`
    `at-dir`, `at-dir-down`, `at-dir-cj`, `at-tab`;
  - `at-resize-band-at-the-bottom` (RESIZE-REPAINT), 5 snaps: the resize
    scenario after the resize.
- The other unresolved pairs (`p5-input-approval`, `p5-input-model`,
  `p5-input-paste`, the two `p5-mcp-*`, and the snaps other causes name) have
  the same verdict on the base and the head.

## Unit tests (in the gate's `cargo test`)

| test | pins |
|---|---|
| `file_picker::the_list_grows_to_ten_rows_while_five_transcript_rows_stay_visible` | fx's table, `picker_presentation.zig:1588-1597` |
| `file_picker::the_height_matches_fxs_table_over_21_screens_4_composers_and_3_banners` | `height-table.txt`, 252 cells (renamed in round 2: it was named for every screen) |
| `file_picker::the_height_is_a_function_of_the_rows_left_past_the_fixed_ones` | round 2: every screen to 64 rows, composer and banner to 16 (18 785 cases) against a piecewise table written apart from the formula |
| `file_picker::below_six_rows_the_list_takes_what_the_screen_has_and_never_none` | the list floor, saturation |
| `file_picker::the_arrows_scroll_within_the_rows_the_last_frame_painted` | fx's `file_completion_state.zig:319-350` |
| `file_picker::the_window_scrolls_only_past_the_rows_painted` | fx's `list_window.zig:70-85` |
| `file_picker::a_frame_paints_the_rows_that_keep_the_mark_in_view_and_keeps_the_window` | the resize re-fit |
| `file_picker::a_refresh_leaves_the_window_where_the_last_move_left_it` | `acknowledge`'s write-back |
| `file_picker::a_refresh_that_finds_the_mark_on_the_first_row_keeps_the_window` | round 2 RED: the stored start kept when the mark is found on row 0 |
| `file_picker::a_refresh_that_loses_the_mark_starts_the_window_over` | round 2: the reset kept only for a lost mark |
| `file_picker::an_empty_list_says_why_at_the_at_column` | updated: Enter on no match waits until a frame showed it |
| `event_loop::the_at_list_scrolls_by_the_rows_the_frame_that_landed_painted` | RED; `landed_band` |
| `event_loop::the_frame_that_gives_the_plane_back_is_one_the_at_lists_arrows_scroll` | `restore_primary` |
| `event_loop::a_new_at_words_arrows_wait_for_the_frame_that_shows_its_rows` | round 2 RED: a new word, and a changed letter, with Downs in one read; replaces round 1's `the_rows_painted_are_kept_…`, which pinned an xfx-only path and called it fx's (opus SHOULD 1) |
| `event_loop::a_refused_frame_leaves_the_at_lists_arrows_waiting_for_one_that_lands` | round 2 RED: a refused frame, and a resize whose first frame is refused (gpt6) |
| `event_loop::a_resize_spends_the_at_lists_keys_until_its_frame_lands` | round 2: a signalled resize, and a screen no band fits |
| `event_loop::under_the_full_transcript_the_at_list_takes_no_key` | round 2: Ctrl-O and Tab in one read; the return with a Down in it |
| `shell::tab_and_enter_before_a_new_words_first_frame_choose_nothing_and_send_nothing` | round 2 RED: Tab, and Enter on no match |
| `shell::only_a_frame_that_landed_showing_the_list_changes_the_rows_its_arrows_scroll` | round 2: a refused write on a shorter screen keeps the count and spends the key (gpt6) |
| `shell::a_frame_that_gave_the_list_no_rows_leaves_the_rows_its_arrows_scroll` | fx's `picker_rows > 0`; round 2: and the key after it is spent |
| `shell::a_waiting_prompts_rows_come_out_of_the_at_lists_growth` | the banner term: 10 rows alone, 8 past a one-row prompt at 20 rows |
| `shell::an_at_opens_…`, `shell::a_query_narrows_…` | updated: ten rows (`filepicker`'s `at`, `at-sch` at 40x120 on fx) |

## The gate (`gate.txt`, `gates.sh`)

Round 2's gate covers every CI step. It ran in a `--no-local` clone at
`a4914d8e`, the last commit before this folder's, in one hold of the suite
lock, with `TMPDIR=<scratch>/at-height`. **20 steps, 0 failed**, and the
tree was clean before and after:
- fmt, clippy, and clippy with fault injection;
- `cargo test`: 5465 passed, 0 failed. With fault injection: 5502 passed, 0
  failed;
- the identity, stubs, secrets, preview-contract, preflight-cost and
  usage-clock checks, and their self-tests;
- the fxcap sweep and capture self-tests;
- both release builds;
- smoke, and smoke-tui: 37 scenarios and the oracle, 1168 checks, 0
  failures.

Round 1's gate was at `ee0959a7`, 20 of 20 (5457 and 5494 tests). The
receipt commit after `a4914d8e` adds only this folder.

## docs/parity.md

The `@` file picker row:
- the height (rule, table, the turn's-row clamp);
- the scroll (the painted rows, the resize re-fit, the refresh that keeps the
  window);
- round 2: **every key acts only on rows a frame that landed has shown**, and
  when that is cleared. Round 1's "kept from one `@` word to the next" is
  gone: no key reads the count across words;
- two narrower points, with owners:
  - no `Selection unavailable` row (AT-STALE-SELECTION);
  - keys after a refresh act on its rows at once, where fx's act on the rows
    the last frame showed;
- PgUp/PgDn's page named INPUT-ROW-LIMIT's.

The receipt pointer names this folder.

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
  `c1db9191` too. Owner: **AT-STALE-SELECTION** (the dispatcher's queue). If
  it ports the stale state, `file_picker_landed` must subtract the status row
  (`filePickerItemRows`).
- **PgUp and PgDn's page** is **INPUT-ROW-LIMIT**'s. This unit does not
  measure it. xfx's page is `layout::input_row_limit(rows)`
  (`src/tui/layout.rs:334-337`: `content_bottom = rows - 3`); fx's is
  `inputRowLimit(content_bottom)` with `content_bottom = rows -| 4`
  (`app_render_runtime.zig:2472`). Only the rows the page scrolls within
  changed here. The `pgup` pairs are identical at every size, but they wrap
  to the end on any page over 3 rows, so they do not measure it.
- **A waiting prompt's rows in the other list pickers'** heights:
  **MENU-BANNER-ROWS** (`surface_frame.zig:538-542@481104fd`; xfx's
  `Slot::Menu` does not count them). `fit`'s comment now says so.
- **Where the keys go under Ctrl-O** (found in round 2, owner none yet).
  fx's list takes no key while the full transcript is up, and neither does
  xfx's now; the base's took them.
  - The keys the transcript leaves unbound go somewhere else in fx. Its C-j
    under Ctrl-O lands in the composer as a new line, which closes the list
    (`ctrlo-probe/`: fx, base and head `cj-back` captures, and the
    scenario's lines; the scenario itself is not committed).
  - xfx spends them.
  - This is the full transcript's key routing (`filePickerEligible` false,
    so the composer gets the key; `input_completion_runtime.zig:775-786`,
    `:1054-1056`), not this unit's gate.
- **The prepared/presented split** is not ported. fx's keys act on the rows
  the last frame presented until the next frame shows a refresh's new rows
  (`current`, `file_completion_state.zig:212-216`, `:241`); xfx's act on the
  new rows at once. It shows only for a key typed between a refresh and its
  frame, and `docs/parity.md` records it.
