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
| (receipt commit) | this folder |

**Builds.** The base's debug build (`v0.1.0-32afd30`, sha256 `1756f4fa…`) and
the head's, built after the commit `d2e81c93` (`v0.1.0-d2e81c9`, sha256
`546468cf…`). Row 1 of every capture in `xfx-base/` and `xfx/` names its
build (`pairs.txt`, first two lines). The commits after `d2e81c93` add only
this folder.

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
- **fx pages two more lists by the limit, which xfx binds no page key on.**
  These are `/settings`' menu and its model list (`routeSettingsMenuMove`,
  :431-463) and the `/model ` column with its effort and speed stages
  (`navigateModelPicker`, :1072-1096). xfx's `SettingsAction::of` names no
  page key (`src/tui/settings.rs:539-551`), and neither does
  `offer_to_column`'s key set. That is a missing binding, not a wrong limit,
  so it is out of this unit. It is now stated in `docs/parity.md` with an
  owner, PAGE-KEYS-SETTINGS-COLUMN (see Open). fx's auth picker
  (`routeAuthPickerMove`) has no xfx surface.
- The six test callers at the base (`layout.rs:389-391`, `shell.rs:20318`,
  `20395`, `22146`, `34676`, `42319`) compute their expectations from the
  function, and they pass on the head as they are (the gate).

## The fix

`input_row_limit(rows)` is now `(if rows - 4 > 4 then (rows - 4) / 2 else 0)
+ 1`, with the 4 named `FX_FOOTER_ROWS`. Every caller above reads it, so all
of them move at once: one function, about twenty lines of source, and no
caller edited. The old one was `(rows - 3) / 2 + 1`. That is one row more at
every odd height from 9 up (20 at 41 rows, where fx's is 19), and two more at
8 rows, where fx's is 1. At even heights from 10 up the two agree, which is
why no 40x120 capture has ever shown it.

## RED (`red-unit.txt`; pairs: `pairs.txt` column `base`)

- **Unit** (`c4558b2f` over the base's code, `red-unit.txt`): all 11 new cases
  fail. The function's boundary table fails at `8 rows: left 3, right 1`. Each
  41-row surface case fails on `a page is not 19 steps` (19 steps reach the
  20th entry where the page reached the 21st: `s20` vs `s21`, `f-20` vs
  `f-21`, `vendor/m20` vs `m21`, turn 11 vs 10). The composer case fails on its
  cap, `left: 20, right: 19`. The effort case fails on `the page did not move
  the mark` (20 is 0 round five levels). The 8-row case fails because PgDn
  wrapped the provider column back to `Gateway`.
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
    base), with the mark on row 12. Its 7 rows on the head are the commands
    that sit 15th to 20th in each list.
- **Unit** (`green-unit.txt`): the 11 cases pass on the head.

## Mutants (`mutants.txt`, made by `mutants.py`)

TODO-MUTANTS

## The sweep (`sweep-41x120-base.md`, `sweep-41x120-head.md`, `sweep-compare.txt`)

TODO-SWEEP

## The gate (`gate.txt`, made by `gates.sh`)

TODO-GATE

## `docs/parity.md` and the ledger

- `full-screen TUI` row: the composer's cap is now stated as fx's
  `inputRowLimit`, with the numbers at 41 and 8 rows. The page-key sentence
  now cites `routeComposerVertical`'s `picker_delta` and this receipt's
  pairs. It names `/settings`' menu and the `/model ` column as fx lists xfx
  binds no page key on, with an owner, and says `/mcp`'s menu takes a page
  as one step, as fx's does.
- `@` file picker row: "the page itself is unit INPUT-ROW-LIMIT's" is now
  fx's `inputRowLimit`, 19 rows at 41.
- **The ledger retires no cause.** No `[[cause]]` in
  `scripts/fxcap/parity-ledger.toml` names INPUT-ROW-LIMIT; AT-HEIGHT named
  it in prose only. The sweep shows no cause this unit closed (above).
  The ledger's six `src/tui/shell.rs:39046` refs moved to `:39279` with the
  233 lines of tests added above them, checked by text (`refs.txt`, made by
  `refs.py`: 16 refs, 0 wrong).

## Open

- **PAGE-KEYS-SETTINGS-COLUMN** (proposed): bind PgUp and PgDn in
  `/settings`' menu (and its model list) and in the `/model ` column's three
  stages, by `input_row_limit`, as fx does (`input_completion_runtime.zig:
  431-463`, `:1072-1096@481104fd`). It is a new binding with its own
  captures, so it was left out of this unit.
- The `/help` and slash-menu pairs stay apart until the command set matches
  (`marks.txt` shows the page itself agrees).
