---
title: 0.0.16 delta -- INPUT-ROW-LIMIT receipt (the composer's cap and every page)
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, input, composer, menus]
---

# INPUT-ROW-LIMIT: fx v0.0.16's `inputRowLimit`, for every surface that reads it

Why: AT-HEIGHT's review found that PgUp/PgDn's page in the `@` list is
xfx's `layout::input_row_limit`, which is also the composer's row cap and
every menu's page (`../at-height/README.md`, "PgUp and PgDn's page"). So it
is fixed once, here, for all of them. Oracle: fx v0.0.16 (`481104fd`,
executable sha256 `8e6cda2e…`, guarded by `capture.sh` before and after every
run, `FX_AUTO_UPGRADE=0`). Base: `origin/main` `32afd307`. Branch
`fix/p5-input-row-limit`. Every claim below names the file in this folder
that backs it.

| commit | what |
|---|---|
| `c4558b2f` | RED: the unit test on the function at its boundary heights, one case per page surface at 41 rows, the provider column and the cap at 8 rows; scenario `p5-input-row-limit` |
| `053714d6` | the fix (`src/tui/layout.rs`), `docs/parity.md`, the ledger's shifted `shell.rs` refs; scenarios `p5-input-row-limit` (caret lines) and `p5-input-row-limit-short` |
| `d2e81c93` | scenario `p5-input-row-limit-menus` and its thirty-model catalog -- **the head's build** |
| `8baf0154`, `fc1d64f4` | this folder: the pairs, carets, marks, mutants, callers and fx's lines; then the gate, the sweeps and the README |
| (round 2) | this folder (`refs.txt`, `fx-src.txt`, the README) and `docs/parity.md`: the reviews' receipt and docs fixes, no code |

**Builds.** The base's debug build (`v0.1.0-32afd30`, sha256 `1756f4fa…`) and
the head's, built after the commit `d2e81c93` (`v0.1.0-d2e81c9`, sha256
`546468cf…`). Row 1 of every capture in `xfx-base/` and `xfx/` names its
build (`pairs.txt`, first two lines). The commits after `d2e81c93` change
only this folder and, in round 2, `docs/parity.md` -- no code, so the build
stands for every later commit.

## What fx v0.0.16 does (every line quoted in `fx-src.txt`, made by `fx_src.sh`)

1. **The limit.** `inputRowLimit(content_bottom)` is `content_bottom / 2`
   extra rows while `content_bottom > 4`, none otherwise, plus one
   (`src/ui/footer/input_presentation.zig:252-255@481104fd`).
2. **Its `content_bottom` is the screen less four.** Every caller passes
   `app.shell.layout.content_bottom`. That layout is only ever what
   `queryLayout(footer_rows)` returns for the terminal's size
   (`app_lifecycle.zig:883`, `main.zig:1036`, `resize_runtime.zig:271-279`,
   `app_session_runtime.zig:1598`), with `footer_rows = 4` (`main.zig:173`),
   and `layoutFromSize` sets `content_bottom = rows - footer_rows`
   (`src/ui/terminal/terminal.zig:47-59`). A terminal of 4 rows or fewer is
   refused there (`TerminalTooSmall`), so xfx's saturating arm below its own
   `MIN_ROWS` (7) is unreachable, not a divergence. Unchanged since
   `c1db9191`: xfx's `rows - 3` was never fx's.
3. **The cap.** `cappedInputRows` caps the composer at the limit
   (`input_presentation.zig:257-267`). The render path reads it
   (`measureRawInputGeometryPreparedWithProvider`, `surface_frame.zig:437-441`),
   and so does `footerRowBudget`, which sizes every menu's rows
   (`input_completion_runtime.zig:571`). The window over a capped draft ends
   at the caret or starts at 0 (`visibleWindow`, `visual_layout.zig:682-688`),
   as xfx's `editor::window` does.
4. **The page.** PgUp and PgDn go to `routeComposerVertical` with `page`
   (`app_input_runtime.zig:310-327`). There `row_count` is the limit and
   `picker_delta` is a step times the limit (`input_completion_runtime.zig:
   215-233`). `picker_delta` goes to whichever list has the keys
   (`routeNonSlashPickerMove`, `:395-422`), then to the slash menu
   (`:255-260`); otherwise the caret moves `row_count` rows (`:235-243`).

## The callers (`callers.txt`)

`callers.txt` lists fx's callers at the pin, xfx's at the base, and the fn
each xfx call is in.

| xfx caller (`shell.rs` at the base) | what it pages or caps | fx's counterpart (`input_completion_runtime.zig@481104fd`) | pinned by (`src/tui/shell.rs`) |
|---|---|---|---|
| `offer_to_catalog` :5518 | `/help`'s catalog | `routeHelpMenuMove` :465-483 | `at_41_rows_a_page_of_the_help_catalog_is_19_commands` |
| `offer_to_skills` :5810 | `/skills` and `$` menu | `routeSkillsMenuMove` :536-539 | `…_the_skills_menu_is_19_skills` |
| `offer_to_files` :6158 | the `@` list | `navigateFilePicker` (`hasFileQuery`) :407-410 | `…_the_at_list_is_19_paths` |
| `offer_to_sessions` :6652 | `/resume`'s picker | `routeSessionPickerMove` :498-504 | `…_the_resume_picker_is_19_sessions` |
| `offer_to_models` :6699 | `/model`'s menu (and C-p) | `routeModelMenuMove` :485-496 | `…_the_model_menu_is_19_models` |
| `offer_to_effort` :6747 | `/effort`'s menu | `routeEffortMenuMove` :519-528 | `…_the_effort_menu_is_19_levels_round_its_five` |
| `offer_to_fork` :6872 | `/fork`'s menu | `routeForkMenuMove` :506-517 | `…_the_fork_menu_is_19_prompts` |
| `offer_to_picker` :8595 | the slash menu | `navigateSlashCompletion(picker_delta)` :255-260 | `…_the_slash_menu_is_19_commands` |
| `offer_to_provider_picker` :9167 | `/provider`'s column | `provider_picker_runtime.navigate` :415-418 | `on_an_eight_row_screen_a_page_and_the_composers_cap_are_one_row` |
| `act` :12376 | the caret, in the draft | `scanInputCursorRows(row_count)` :235-243 | `at_41_rows_the_composer_is_capped_at_19_rows_and_a_page_is_19_of_them` |
| `fit` :16131 | the composer's cap | `cappedInputRows` (render, `footerRowBudget`) | the same case, and the 8-row one |

- **Every xfx caller has an fx counterpart that reads the same limit.** None
  of them is paged by something else in fx. `/mcp`'s menu takes a page key as
  one step on both sides (`routeMcpHostMenuMove`, :424-429: up or down by the
  sign alone; xfx's `offer_to_mcp` maps PgUp and PgDn to Up and Down), so it
  is not a caller. Ctrl-O's full transcript pages by its own height on both
  sides, so it is not one either.
- **fx pages three more lists by the limit, which xfx binds no page key on.**
  These are `/settings`' menu and its model list (`routeSettingsMenuMove`,
  :431-463); the `/model ` column with its effort and speed stages
  (`navigateModelPicker`, :1072-1096); and `/mcp `'s argument completions.
  The last is a slash list in fx: `slashCompletionCount` counts the
  registry's `arguments` (`command_specs.zig:578-585`, `runtimeArgQuery`
  `:876-889`), which are `/mcp`'s verbs and servers (`main.zig:441-445`;
  the first is `/mcp list`, `app_mcp_runtime.zig:459-468`, so
  `runtimeArgCommand` is `/mcp`), and the slash list pages by `picker_delta`
  (:255-260). xfx's `SettingsAction::of` names no page key
  (`src/tui/settings.rs:539-551`), and neither does `offer_to_column`'s key
  set or `offer_to_mcp_args` (Up, Down, `C-j`/`C-k`, Tab, Enter, Escape only). That is a missing binding, not a wrong limit,
  so it is out of this unit. It is now stated in `docs/parity.md` with an
  owner, PAGE-KEYS-SETTINGS-COLUMN (see Open). fx's auth picker
  (`routeAuthPickerMove`) has no xfx surface.
- The six test callers at the base, and what became of each:
  - `layout.rs:389-391` asserted the old values as literals (6 rows → 2 among
    them); they are rewritten as fx's boundary table;
  - `shell.rs:20318` asserts a literal 5 at 12 rows and `34676` a literal 11
    at 24, which fx's limit gives too (8 / 2 + 1, 20 / 2 + 1);
  - `20395`, `22146` and `42319` compute their expectations from the
    function.
  
  All but the first pass on the head unchanged (the gate).

## The fix

`input_row_limit(rows)` is now `(if rows - 4 > 4 then (rows - 4) / 2 else 0)
+ 1`, with the 4 named `FX_FOOTER_ROWS`. Every caller above reads it, so all
of them move at once: one function, about twenty lines of source, and no
caller edited. The old one was `(rows - 3) / 2 + 1`. That is one row more at
every odd height from 9 up (20 at 41 rows, where fx's is 19), and two more at
7 rows (`MIN_ROWS`) and at 8, where fx's is 1. At even heights from 10 up the two agree, which is
why no 40x120 capture has ever shown it.

## RED (`red-unit.txt`; pairs: `pairs.txt` column `base`)

- **Unit** (`c4558b2f` over the base's code, `red-unit.txt`): all 11 new cases
  fail. The function's boundary table fails at its first row past 4 rows,
  `a 7-row screen`, `left: 3, right: 1`. Seven of the 41-row cases fail on
  `a page is not 19 steps`: 19 steps reach where fx's page does, and the
  page went one further (`s20` vs `s21`, `items/f-20.txt` vs `f-21`,
  `vendor/m20` vs `m21`, `/fast` vs `/ultrafast`, turn 11 vs 10, index 19 vs
  20, `s18` vs `s19`). The effort case fails at the page's `assert_ne`
  (`shell.rs:35084`, both sides `Auto`: 20 is 0 round five levels). The
  composer case fails on `the composer's cap`, `left: 20, right: 19`. The
  8-row case fails on `a page of one row`: PgDn wrapped the provider column
  back to `Gateway`.
- **Pairs at 41x120 and 8x120.** These pairs differ on the base: `at-pgdn` 10
  rows, `grown` 22, `grown-pgup` 3, `grown-pgup2` 3, `help-pgdn` 21,
  `menus model-pgdn` 20, `menus effort-pgdn` 2, `short grown` 4,
  `short grown-pgup` 4, `short slash-pgdn` 1. The carets differ too
  (`grown` 9,21 against fx's 9,20; `grown-pgup` 9,6 against 9,7;
  `short grown` 6,4 against 6,2; `short grown-pgup` 5,2 against 6,2). Where
  the base's page landed (`marks.txt`): `f-21` against fx's `f-20`;
  `fake/row-10` against `fake/row-11`; `default` (unmoved) against `xhigh`;
  the slash counter `16–21` against `15–20`; at 8 rows `/reset` against
  `/clear`.

## GREEN (`pairs.txt` column `head`, `marks.txt`, `green-unit.txt`)

- **13 of 17 pairs are identical** (0 rows differ, row 1 left out):
  - `at`, `at-pgdn`, `grown`, `grown-pgup` and `grown-pgup2` at 41x120;
  - `menus` `model`, `model-pgdn`, `effort` and `effort-pgdn` at 41x120;
  - `short` `slash`, `slash-pgdn`, `grown` and `grown-pgup` at 8x120.
  
  All five carets are the same as fx's.
- **The other four are `/help` and the slash menu.** fx lists 38 commands and
  xfx 34 (no `/login` or `/logout`, a different `/model` line, `/effort` and
  `/fast` in `/help`; the ledger's existing causes). So their rows differ
  before any key: `help` 9 rows on the base and the head, `slash` 1. The page
  lands where fx's does (`marks.txt`):
  - `help-pgdn`'s mark is on screen row 26, the 20th command of each list,
    under a window that did not scroll. On the head that is 9 rows apart,
    the same 9 rows as `help`. On the base it was 21, because its window
    scrolled one row further.
  - `slash-pgdn`'s counter reads `15–20` on fx and the head (`16–21` on the
    base), with the mark on row 12. Its 7 rows on the head are the `Commands 38`
    / `34` header and 6 command rows, the 15th to 20th of each list.
- **Unit** (`green-unit.txt`): the 11 cases pass on the head.

## Mutants (`mutants.txt`, made by `mutants.py`)

All 13 were killed. Each ran on a `git clone --no-local` of the head
(`d2e81c93`) in the scratch directory, never on the worktree: the clone's
tree reset, one edit, and the unit's 11 cases run (`cargo test --lib --
at_41_rows on_an_eight_row_screen the_composer_may_take_half`, under the
suite lock).

| mutant | killed by |
|---|---|
| the old `content_bottom`, `rows - 3` (guard kept) | all 11 |
| no `> 4` guard | the boundary table (7 and 8 rows) and the 8-row case |
| each of the 11 surfaces left on `(rows - 3) / 2 + 1`, one at a time | that surface's case; `fit`'s also by the 8-row case |

So a revert, or a half-revert that leaves any one surface on the old limit,
fails at least one case.

## The sweep (`sweep-41x120-base.md`, `sweep-41x120-head.md`, `sweep-compare.txt`)

`sweep.sh` against fx v0.0.16 at **41x120**, `--jobs 2`, under the suite
lock, over 18 scenarios that page or grow the composer:
`p5-input-row-limit*` (this unit's three), `composer*`, `filepicker*`,
`p5-input-at*`, `p5-panels-keys`, `fulltranscript*`, `card-wrap*` and
`p5-ctrlo-wider-inline-fam`. It ran on the base's build, then on the head's
(scoreboards: `x𝒇x v0.1.0-32afd30`, `v0.1.0-d2e81c9`), with the ledger as it
is at the head.

- **pass 61/120 → 67/120.** Six pairs move to normalized-identical, and
  **none drops** (`sweep-compare.txt`): `p5-input-row-limit` `at-pgdn`,
  `grown`, `grown-pgup` and `grown-pgup2`; `p5-input-row-limit-menus`
  `model-pgdn` and `effort-pgdn`.
- **Named, not counted.** On the base, exactly those six were the snaps no
  cause covered ("Rows no cause covers (6 snaps)", `sweep-41x120-base.md`).
  On the head, no snap is left without a cause. `help-pgdn` and
  `slash-pgdn` stay under the ledger's existing `menu-count`, `menu-shift`
  and `C7-help-entries`.
- **Why 41 and not 40.** At every even height from 10 up, `(rows - 3) / 2`
  equals `(rows - 4) / 2`, so the base and the head compute the same limit
  there (the boundary table's `(40, 19)`, `(42, 20)`). A 40x120 sweep cannot
  move, and none was run.

## The gate (`gate.txt`, made by `gates.sh`)

**20 steps, 0 failed**, run once at `8baf0154`, whose code is the head's,
in one hold of the suite lock (`gate.txt`). The steps are fmt, clippy (plain
and fault-injection), `cargo test` (plain and fault-injection), the `check-*`
scripts, their self-tests, the fxcap sweep and capture self-tests, both
release builds, smoke and smoke-tui. The tests: 5549 passed and 0 failed;
5586 and 0 under fault-injection (15 binaries each; their result lines are
copied into `gate.txt`). The 1-minute load at a step's start was between 46.0 (clippy-fault)
and 5.0 (the release build). No step failed, and none was rerun.

`gate.txt` says "tree clean: no" before and after. The only paths not
committed were in this folder: the README being written, and the sweep files
and `compare.py`, added after `8baf0154`.

## `docs/parity.md` and the ledger

- `full-screen TUI` row: the composer's cap is now stated as fx's
  `inputRowLimit`, with the numbers at 41 and 8 rows (`-41x120-grown`,
  `-short-8x120-grown`). The page-key sentence now cites
  `routeComposerVertical`'s `picker_delta` and this receipt's pairs. It names
  three fx lists xfx binds no page key on -- `/settings`' menu, the `/model `
  column and `/mcp `'s argument completions (round 2) -- with an owner, and
  says `/mcp`'s menu takes a page as one step, as fx's does.
- `/effort` row (round 2): "upstream's page keys there are uncaptured" is
  now what `-menus-41x120-effort-pgdn` shows: PgDn from `default` lands on
  `xhigh`, 19 round five levels.
- `@` file picker row: "the page itself is unit INPUT-ROW-LIMIT's" is now
  fx's `inputRowLimit`, 19 rows at 41.
- **The ledger retires no cause.** No `[[cause]]` in
  `scripts/fxcap/parity-ledger.toml` names INPUT-ROW-LIMIT; AT-HEIGHT named
  it in prose only. The sweep shows no cause this unit closed (above).
  The ledger's six `src/tui/shell.rs:39046` refs moved to `:39279` with the
  233 lines of tests added above them, checked by text (`refs.txt`, made by
  `refs.py`: 16 refs, 0 wrong).

## Open

- **PAGE-KEYS-SETTINGS-COLUMN** (queued by the dispatcher): bind PgUp and
  PgDn in `/settings`' menu (and its model list), in the `/model ` column's
  three stages and in `/mcp `'s argument completions, by `input_row_limit`,
  as fx does (`input_completion_runtime.zig:431-463`, `:1072-1096`,
  `:255-260@481104fd`). It is a new binding with its own captures, so it was
  left out of this unit.
- The `/help` and slash-menu pairs stay apart until the command set matches
  (`marks.txt` shows the page itself agrees).
