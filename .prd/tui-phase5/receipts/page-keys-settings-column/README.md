---
title: 0.0.16 delta -- PAGE-KEYS-SETTINGS-COLUMN receipt (page keys on the three lists that took none)
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, input, menus, settings, model, mcp]
---

# PAGE-KEYS-SETTINGS-COLUMN: PgUp/PgDn and C-j/C-k on `/settings`, the `/model ` column and `/mcp `'s arguments

Why: INPUT-ROW-LIMIT made every page xfx had fx's `inputRowLimit`, and named
three lists fx pages that xfx bound no page key on (`../input-row-limit/README.md`,
"The callers"; `docs/parity.md` at the base, the full-screen TUI row). This unit
binds them. Oracle: fx v0.0.16 (`481104fd`, executable sha256 `8e6cda2e…`,
guarded by `capture.sh` before and after every run, `FX_AUTO_UPGRADE=0`).
Base: INPUT-ROW-LIMIT's head `3d3d56c8` (branch `fix/p5-input-row-limit`).
Branch `fix/p5-page-keys-settings-column`. Every claim below names the file in
this folder that backs it.

| commit | what |
|---|---|
| `01d661ba` | RED: five shell cases at 24 rows; scenarios `p5-page-keys-settings`, `-column`, `-mcp` and the MCP profile `p5-page-keys-mcp-profile.json` |
| `c4bfa8b1` | the fix (`src/tui/settings.rs`, `src/tui/model_column.rs`, `src/tui/shell.rs`) -- **the head's build** |
| `fe316934` | `docs/parity.md`, the ledger's moved `shell.rs` refs, and the two scenarios reordered (C-j/C-k last) |
| `8f356ae4`, `101c6f84` | this folder, round 1 |
| `79e882fd` | round 2 RED: the slash lists on a taller composer, `C-j`/`C-k` under the full transcript, Up/Down over an empty settings list |
| `2cbf4d41` | round 2's fix (`src/tui/shell.rs`) |
| `8e85d9b5` | a pin: a page over an empty settings list, on a two-row search |
| `0312cf42` | `anchor_at` through `column_anchor` (found by round 2's wrapped capture) -- **the head's build** |
| `ab2cfb94` | `docs/parity.md`, the scenarios' round 2 steps, the ledger's and `capfile.py`'s refs |
| (this commit) | this folder, round 2 |

**Builds.** Three debug builds, each built after the commit it names, row 1
of every capture naming its build (`pairs.txt`, first lines): the base's
(`v0.1.0-3d3d56c`, sha256 `af8761fd…`) in `xfx-base/`, round 1's head
(`v0.1.0-c4bfa8b`, `2f9161d8…`; its code is `101c6f84`'s) in `xfx-r1/`, and
the head's (`v0.1.0-0312cf4`, `0bd74fd1…`) in `xfx/`. The commits after
`0312cf42` change docs, refs, scenarios and this folder -- no code -- so that
build stands for every later commit. `xfx-2cbf4d4/` holds the two `/mcp`
wrapped snaps of the build at `2cbf4d41` (`anchor-2cbf4d4.txt`). The non-test
source change is about +195/-57 lines against the base, +71/-18 of them in
round 2 (`git diff <rev> 0312cf42 -- src`, less the hunks inside each file's
`mod tests`).

## What fx v0.0.16 does (every line quoted in `fx-src.txt`, made by `fx_src.sh`)

**The keys.** PgUp/PgDn decode to `move(.page_up/.page_down)`
(`src/ui/input/shortcuts.zig:53-54`) and go to `routeComposerVertical` with
`page` (`src/core/app/app_input_runtime.zig:310-327`). There the page is
`inputRowLimit(content_bottom)` and `picker_delta` that many rows
(`src/core/app/input_completion_runtime.zig:222-229`), offered first to
`routeNonSlashPickerMove` (`:230`, `:395-422`), then to a visible slash list
(`:255-260`). `C-j` (10) and `C-k` (11) are a row down and up for whatever list
is visible (`routePickerControlByte`, `app_input_runtime.zig:1093-1108`, into
`routeVisiblePickerMove`, `input_completion_runtime.zig:390-393`); when nothing
takes them they go on to the composer as a newline and a kill
(`app_input_runtime.zig:879-893`). The page is 11 at 24 rows (`20 / 2 + 1`,
`src/ui/footer/input_presentation.zig:252-255`).

1. **`/settings`' menu** (`routeSettingsMenuMove`, `:431-463`). `Menu.move`
   (`src/core/config/settings_catalog.zig:225-241`): `next = current + delta`;
   below 0 it is the **last** setting, at or past the count the **first** --
   not taken round -- and the window moves only as far as keeps the mark in
   view (`updateWindowStart`, `:408-414`). Over an empty list it answers
   `false` (`:227`), so the key goes on. With the Model row's list open the
   same delta moves the list instead, by the same rule
   (`moveVisibleItems`, `src/core/app/model_cache_runtime.zig:229-245`, over
   `visibleModelItemsForBudget`, `input_completion_runtime.zig:440-455`), and
   is always spent. Fourteen
   settings (`settings_catalog.zig:261-276`).
2. **The `/model ` column** (`hasModelQuery`, `:629-631`;
   `navigateModelPicker`, `:1072-1096`). Each stage -- the models
   (`navigateModelCompletion`, `:1098-1106`), the effort levels and the speed
   rows, and an `/effort ` line's levels -- goes to `navigatePickerOptions`,
   which is `list_window.advanceSelection` (`:1158`; `src/core/shared/list_window.zig:40-53`):
   the same far-end rule, a six-row window (`default_max_picker_rows`, `:3`)
   kept at its edge (`updateEdgeStart`, `:55-68`). Always spent
   (`routeNonSlashPickerMove` answers `true` once `hasModelQuery` does,
   `:411-414`).
3. **`/mcp `'s argument completions** are a slash list: `slashCompletionCount`
   counts the registry's `arguments` (`src/core/slash_commands/command_specs.zig:578-585`,
   `runtimeArgQuery` `:883-889`), which are `/mcp`'s verbs and its servers'
   lines (`src/core/app/app_mcp_runtime.zig:459-468`), and the list moves by
   `navigateSlashCompletion` (`input_completion_runtime.zig:171-173`), the
   same `advanceSelection`. A
   server `"enabled": false` is held as `disabled` and never started
   (`src/core/mcp_host/config.zig:14-18`, `:186-192`).

**Sibling keys checked** (none of them changed here):

| key | fx at the pin | xfx |
|---|---|---|
| Up/Down | one row by the same moves, so a wrap (`routePlainVertical`, `input_completion_runtime.zig:211-213`) -- on a slash list only while the composer is one row (round 2) | the same (`step` is now `step_by(±1)`); round 2 below |
| Home/End | caret moves: `move(.line_start/.line_end)` (`shortcuts.zig:51-52`), with the column's later stage abandoned (`app_input_runtime.zig:298-309`) | the same (`offer_to_column`'s Home/End arm; the menus do not bind them) |
| `C-n` | `history_next`, a history step (`shortcuts.zig:17`, `app_input_runtime.zig:403-407`) | `HistoryNext`, the same; walks none of these lists |
| `C-p` | no composer shortcut (`shortcuts.zig:107-117`) | the model catalog's key; walks none of these lists |
| the wheel | nothing at the composer (`app_input_runtime.zig:1036-1040`) | no mouse reports at all (`docs/parity.md`, full-screen TUI row) |
| `C-j`/`C-k` | a row on all three (above), at any composer height; none while the full transcript is up (round 2) | **copied** on `/settings` and the column; `/mcp `'s arguments had them; round 2 below |

## The fix, round 1

- `SettingsAction` gains `Row(bool)` (`C-j`/`C-k`) and `Page(bool)`; the menu's
  `step_by` moves by a delta with `picker::advanced`, the far-end rule xfx's
  other pages already use. With the list open the same deltas go to the list
  with the settings budget (`visible_models`). Over an empty list no move is
  taken (round 2; round 1 still spent the arrows).
- `model_column::Column::step_by` (all stages), its `advance` on
  `picker::advanced`; `offer_to_column` binds PgUp/PgDn and `C-j`/`C-k`.
- `offer_to_mcp_args` maps PgUp/PgDn to `McpArgs::step_by(±page)`.

## RED, round 1 (`red-unit.txt`; pairs: `pairs.txt` column `base`; marks: `marks.txt`)

Round 1's RED, against the base; `pairs.txt` and `marks.txt` are now round 2's
re-captures of the same steps, on the same base build, so their counts below
are the round 2 files' (the numbers did not change).

- **Unit** (`01d661ba` over the base's code): 4 of the 5 cases fail at the
  first key that should have moved the mark -- the settings menu's first PgDn
  (`left: Statusline(Context), right: SoundLevel`), the column's first PgDn
  (`left: vendor/m01, right: vendor/m12`), the effort column's first PgUp
  (its PgDns from the first of five rows land on the first row on both
  sides; `PgUp from "default" ... right: "xhigh"`), and `/mcp `'s first PgDn
  (`left: "/mcp show s01", right: "/mcp show s12"`). **The settings list case passes on the
  base**: there its keys fall through to `/model`'s menu handler
  (`offer_to_models`, whose `ModelAction::of` maps PgUp/PgDn and `C-j`/`C-k`),
  which moves the same list.
- **Pairs at 24x120** (`pairs.txt`): 21 of round 1's 39 differ on the base
  (29 of all 48 with round 2's steps) -- the 19
  below, and `menu` and `menu-pgdn2`, whose one row is `sound-max`'s on the
  head too (`menu-pgdn2`'s second page runs from `Sound level` past the end
  to the first setting, where the base's mark had stayed).
  - settings menu: `menu-pgdn` 2 rows, `menu-pgup` 12, `menu-pgup2` 12,
    `menu-cj` 11, `menu-ck` 11 -- the base's mark never left `Status line
    context`, and its `C-j` typed a newline into the search (`marks.txt`).
  - column: `model-pgdn`, `-pgdn2`, `-pgup` 6 rows each, `model-cj`/`-ck` 10,
    `effort-pgup`, `speed-pgdn`, `speed-pgup`, `levels-pgup` 3 each.
  - `/mcp `: `args-pgdn`, `-pgdn2`, `-pgup`, `-pgup2`, `-cj` 6 rows each.
  - **Not RED, and said so:** the seven settings-list pairs and `tab` are
    identical on the base (the fall-through above). So are `model-pgdn3` and
    `args-pgdn3` (three pages past the end land on the first row, where the
    base's mark had stayed) and `effort-pgdn`, `effort-pgdn2`, `levels-pgdn`
    (a page down from the first row runs past the end to the first). The
    five first snaps (`model`, `effort`, `speed`, `levels`, `args`) are
    before any key. 18 in all.

## Round 2 (the reviews' MUSTs and SHOULDs)

**What fx does** (quoted in `fx-src.txt`):

- **A slash list takes the arrows and a page only on a one-row composer.**
  `routeComposerVertical` offers the non-slash lists first (`:230`), then the
  slash list only when `capped.input_extra == 0` (`:250-260`); otherwise the
  caret moves (`:262-291`). `C-j`/`C-k` reach a slash list by
  `routeSlashCompletionMove` (`:175-180`), which asks no height. `/mcp `'s
  arguments are a slash list (above), and `slashCompletionPrefix` trims the
  draft's leading blanks and newlines (`command_specs.zig:568-576`), so the
  list shows over a three-row composer. `/settings` and the column are asked
  first, so they take every move at any height.
- **No list takes `C-j`/`C-k` while the full transcript is up.**
  `routePickerControlByte` returns `false` unless
  `composerPickerSurfaceVisible` (`app_input_runtime.zig:1094`, `:1110-1116`),
  so the byte goes on to the composer (`:879-893`): a newline, a kill to the
  end. `/effort`'s and `/fork`'s menus, which read no search, close on that
  edit (`dismissActiveMenusForComposerEdit`, `:881-886`, `:1984-1990`).
- **Over an empty `/settings` list every move goes on** (`Menu.move` answers
  `false`, `settings_catalog.zig:227`): Up and Down to the search's start and
  end, a page by the composer's rows.
- **A list hangs under its argument only on the caret's row**:
  `projectedAnchorColumn` (`visual_layout.zig:705-713`), for every picker
  fx anchors (`input_presentation.zig:383-386`).

**The fix** (`2cbf4d41`, `0312cf42`):

- `composer_rows` (factored out of `fit`) and `composer_grown`;
  `offer_to_picker` and `offer_to_mcp_args` pass Up, Down, PgUp and PgDn on
  when the composer is taller than one row.
- One check in `consume_inputs`, before the first list: with the full
  transcript up, `C-j`/`C-k` go to `act` (the composer) -- one check, as fx
  has one, so it covers every list after it. Tested: the four the reviews
  named, and the slash menu, `/help`, `/provider`, `/effort` and `/fork`;
  by position only: `/resume`, the skills menu and `/mcp`'s menu. The `@`
  list still sees them first: its keys there are unit CTRLO-AT-KEYS's
  (`docs/parity.md`).
- `offer_to_settings`: a move that moved nothing is not taken.
- `anchor_at` (`/mcp `'s arguments, `/provider`'s column) goes through
  `column_anchor`, which the `/model` column already used.

**RED** (`red2-unit.txt`; pairs: `pairs.txt` column `r1`):

- **Unit** (`79e882fd` over `101c6f84`'s code): 8 cases fail. Both
  taller-composer cases on `Up did not move the caret` (the list took it);
  the four named full-transcript cases and the other-lists case on `C-k
  walked a list behind the screen` (settings to `PromptHistory`, the column
  and the menu to `xai/grok-4.7`, `/mcp ` to `/mcp reject`, the slash menu
  from 0 to 33); the settings case on `Up was spent on an empty list`.
- **Pairs on round 1's build**: `wrapped-up`, `-pgup`, `-pgdn` differ by 6
  rows and `wrapped-cj` by 2 (the mark walked); the carets `wrapped-up` and
  `-pgup` are `12,4` where fx's are `2,3` and `2,2`, and `empty-up` is `5,2`
  where fx's is `2,2` (`pairs.txt`, the caret table).
- **The anchor, on `2cbf4d41`** (`anchor-2cbf4d4.txt`): with the keys fixed,
  `wrapped-up` and `-pgup` still differed by the list's 6 rows, drawn 7
  columns in where fx's are 2. `0312cf42` fixed it.
- **Not RED:** a page and `C-k` over an empty settings list already went on
  in round 1; the case's page step (`8e85d9b5`) and `C-k` step are pins,
  killed by their mutants below.

## GREEN (`pairs.txt` column `head`, `marks.txt`, `green-unit.txt`)

- **43 of 48 pairs are identical** (0 rows differ, row 1 left out; round 2's
  nine steps included), every pair of the column and of `/mcp ` among them,
  and all nine carets are fx's (`pairs.txt`, the caret table).
- **The other five are the settings menu's** `menu`, `menu-pgdn`, `-pgdn2`,
  `-pgup`, `-pgup2`, one row each: fx's `Sound level  off  on  max` against
  xfx's `off  on` -- row 18 in `menu`, `-pgdn` and `-pgdn2`, row 16 in `-pgup`
  and `-pgup2`, where the window has scrolled two rows
  (`fx/fx-p5-page-keys-settings-24x120-menu-pgup.txt:16`). That is the ledger's `sound-max` cause
  (`scripts/fxcap/parity-ledger.toml`, `scenarios = ["*"]`), and it differs
  before any key (`menu`). The marks there are fx's (`marks.txt`: `Sound
  level`, `Status line context`, `Prompt history`, `Status line workspace`),
  and the sweep puts each one's only row under `sound-max` with nothing
  unattributed (below). `sound-max` is an unowned defect cause, not an
  approval, so those five stay `unresolved-defect` until xfx has the level;
  no capture at this height leaves the Sound level row out of the menu's
  window while a page lands inside the list.
- Every mark in `marks.txt` is the same on the head as on fx (48 `same`).
- **Unit** (`green-unit.txt`): the unit's 12 cases pass on the head (the
  filter also runs three older cases whose names it matches; they pass).

## Mutants (`mutants.txt`, made by `mutants.py`)

**All 20 were killed** (round 1's 12 and round 2's 8). Each ran on a `git
clone --no-local` of `0312cf42` in the scratch directory, never on the
worktree: the clone's tree reset, one edit (exactly one match in the named
fn, or the run stops), and the unit's cases run (`cargo test --locked --lib
-- at_24_rows taller_composer under_the_full_transcript_the
under_the_full_transcript_no_other`, under the suite lock).

| list | the page key back to nothing | a page of one row | a page taken round the list (`rem_euclid`) where fx goes to the far end | more |
|---|---|---|---|---|
| `/settings` | killed (the menu case) | killed (the menu and the list cases) | killed (the menu case) | `C-j`/`C-k` back to nothing: killed; the list's moves sent to the menu under it: killed (the list case) |
| the column | killed (model and effort/speed cases) | killed (both) | killed (both) | `C-j`/`C-k` out of its key set: killed (both) |
| `/mcp ` | killed | killed | killed | -- |

Round 2's, each killed:

| mutant | killed by |
|---|---|
| the one-row check removed from the slash menu (`offer_to_picker`) | the slash taller-composer case |
| the one-row check removed from `/mcp `'s arguments | the `/mcp` taller-composer case |
| the full-transcript check removed (`consume_inputs`) | all five full-transcript cases |
| the empty-list fall-through removed (every move spent) | the settings case |
| ... for the arrows only (round 1's rule) | the settings case |
| ... for a page only | the settings case (its two-row page step) |
| ... for `C-j`/`C-k` only | the settings case (its `C-k` step) |
| `anchor_at` back on the argument's row (`2cbf4d41`'s) | the `/mcp` taller-composer case's anchor asserts |

There is one full-transcript check for every list, so one mutant removes it
for all of them; each list's case fails on its own.

## The sweeps (`sweep-*.md`, `sweep-compare-*.txt`)

Round 2's, which replace round 1's: `sweep.sh` against fx v0.0.16, `--jobs
2`, all four runs in one hold of the suite lock, the base's build then the
head's (scoreboards: `x𝒇x v0.1.0-3d3d56c`, `v0.1.0-0312cf4`), with the ledger
as it is at `ab2cfb94`. No ledger cause moves with this unit, so there is no
cause's set of scenarios to sweep; these are the unit's own and its
neighbours.

- **24x120, `p5-page-keys-*`** (`sweep-24x120-base.md`, `-head.md`,
  `sweep-compare-24x120.txt`): pass **19/48 → 43/48**, 24 pairs moved to
  passing, **none dropped**. The five left are the settings menu's, each
  `sound-max` only (GREEN).
- **40x120, the neighbours** -- `settings*`, `model-column*`, `model-effort`,
  `model-speed`, `model`, `p5-effort-fork-effort-column`, `p5-input-model*`,
  `p5-mcp-verbs`, and for round 2 `provider*` (the anchor), `slash*` (the
  one-row rule), `fulltranscript*` (`C-j`/`C-k`) and `composer-keys`: 37
  scenarios, 295 pairs (`sweep-40x120-base.md`, `-head.md`,
  `sweep-compare-40x120.txt`): pass **169 → 166**. The three that dropped
  are `model-column-idle`'s `col-down`, `col-idle` and `col-typed`, whose
  only cause on the head is the ledger's `catalog-window` (`kind =
  "nondeterminism"`: the catalog is down for the first nine seconds and the
  two binaries' starts race it; xfx said `unable to load models` where fx
  listed the models, before any key of this unit). Round 1's sweep saw the
  same three move the other way (133 → 136). Not counted as this unit's.
  One pair moved from normalized-identical to identical
  (`settings-collapse-open` `after-esc`). No other verdict moved.

## The gate (`gate.txt`, made by `gates.sh`)

Round 2's, which replaces round 1's: **20 steps; 19 passed and 1 failed**,
run once at `ab2cfb94` (code `0312cf42`), in one hold of the suite lock
(`gate.txt`, the same `gates.sh`). The tests: 5561 passed and 0 failed;
5597 passed and **1 failed** under fault-injection (15 binaries each; result
lines copied into `gate.txt`). The 1-minute load at a step's start was
between 22.3 (test-preview-contract) and 83.7 (test-gate-env).

- **The one failure** is
  `tui::worker::tests::a_stop_in_the_catalog_wait_of_a_turn_that_would_ask_twenty_questions_ends_it_as_the_stop`,
  at load 63.5: a timing window (the stop's end "300 ms to 1 s"), read
  `false`. `src/tui/worker.rs` is not in this unit's diff (`git diff
  3d3d56c8 -- src/tui/worker.rs` is empty), and no key of this unit reaches
  a worker. Run alone three times after the gate on the same fault-injection
  build, it passed 3 of 3 (`gate.txt`, last lines; load 49-50).
- Round 1's gate (`fe316934`) was 20 of 20, 5554 and 5591 passed.

`gate.txt` says "tree clean: no" before and after: the only paths not
committed were in this folder.

## `docs/parity.md` and the ledger

- full-screen TUI row: round 1 turned "Three of fx's lists that page so take
  no page key here yet ... Owner: PAGE-KEYS-SETTINGS-COLUMN" into what they
  do, with the sibling keys above. Round 2 turns its two open differences
  into what xfx now does -- a slash list's arrows and pages only on a one-row
  composer, every move over an empty `/settings` list going on -- and adds
  `C-j`/`C-k` under the full transcript and the caret-row anchor.
- `/model` (the inline column), `/effort` (the `/effort ` column), `/mcp` (the
  argument completions, and in round 2 their one-row rule) and `/settings`
  (the menu, its Model row's list, and in round 2 the empty list) rows: their
  page keys and `C-j`/`C-k`, each with its captures.
- **The ledger retires no cause**: none names this unit or these keys, and the
  captures end none (the settings menu pairs stay under `sound-max`). Its
  `src/tui/shell.rs` refs that this unit's lines moved, and
  `scripts/fxcap/capfile.py`'s two `warning_shown` ranges, are re-pointed,
  checked by text (`refs.txt`, made by `refs.py`: 21 refs, 0 wrong; the one
  `docs/parity.md` ref, `settings.rs:539`, went with the sentence it was in).

## Open

None of round 1's two remains. Questions and approvals take every key before
any list here (`Shell::answer`, `Shell::decide`), so fx's other two
`composerPickerSurfaceVisible` conditions have nothing to refuse.
