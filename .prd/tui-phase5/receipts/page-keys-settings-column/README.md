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
| (this commit) | this folder |

**Builds.** The base's debug build (`v0.1.0-3d3d56c`, sha256 `af8761fd…`) and
the head's, built after the commit `c4bfa8b1` (`v0.1.0-c4bfa8b`, sha256
`2f9161d8…`). Row 1 of every capture in `xfx-base/` and `xfx/` names its build
(`pairs.txt`, first lines). The commits after `c4bfa8b1` change docs, the
ledger's line refs, scenarios and this folder -- no code -- so the build stands
for every later commit. The non-test source change is about +130/-45 lines
(`git diff 3d3d56c8 c4bfa8b1 -- src`, less the hunks inside each file's
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
   `visibleModelItemsForBudget`, `:440-455`), and is always spent. Fourteen
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
   `navigateSlashCompletion` (`:171-173`), the same `advanceSelection`. A
   server `"enabled": false` is held as `disabled` and never started
   (`src/core/mcp_host/config.zig:14-18`, `:186-192`).

**Sibling keys checked** (none of them changed here):

| key | fx at the pin | xfx |
|---|---|---|
| Up/Down | one row by the same moves, so a wrap (`:211-213`) | the same, unchanged (`step` is now `step_by(±1)`) |
| Home/End | caret moves: `move(.line_start/.line_end)` (`shortcuts.zig:51-52`), with the column's later stage abandoned (`app_input_runtime.zig:298-309`) | the same (`offer_to_column`'s Home/End arm; the menus do not bind them) |
| `C-n` | `history_next`, a history step (`shortcuts.zig:17`, `app_input_runtime.zig:403-407`) | `HistoryNext`, the same; walks none of these lists |
| `C-p` | no composer shortcut (`shortcuts.zig:107-117`) | the model catalog's key; walks none of these lists |
| the wheel | nothing at the composer (`app_input_runtime.zig:1036-1040`) | no mouse reports at all (`docs/parity.md`, full-screen TUI row) |
| `C-j`/`C-k` | a row on all three (above) | **copied** on `/settings` and the column; `/mcp `'s arguments had them |

## The fix

- `SettingsAction` gains `Row(bool)` (`C-j`/`C-k`) and `Page(bool)`; the menu's
  `step_by` moves by a delta with `picker::advanced`, the far-end rule xfx's
  other pages already use. With the list open the same deltas go to the list
  with the settings budget (`visible_models`). Over an empty list `Row` and
  `Page` are not taken, so they reach the composer as fx's do; the arrows stay
  spent, as they were (see Open).
- `model_column::Column::step_by` (all stages), its `advance` on
  `picker::advanced`; `offer_to_column` binds PgUp/PgDn and `C-j`/`C-k`.
- `offer_to_mcp_args` maps PgUp/PgDn to `McpArgs::step_by(±page)`.

## RED (`red-unit.txt`; pairs: `pairs.txt` column `base`; marks: `marks.txt`)

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
- **Pairs at 24x120** (`pairs.txt`): 21 of 39 differ on the base -- the 19
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

## GREEN (`pairs.txt` column `head`, `marks.txt`, `green-unit.txt`)

- **34 of 39 pairs are identical** (0 rows differ, row 1 left out), every
  pair of the column and of `/mcp ` among them.
- **The other five are the settings menu's** `menu`, `menu-pgdn`, `-pgdn2`,
  `-pgup`, `-pgup2`, one row each: row 18, fx's `Sound level  off  on  max`
  against xfx's `off  on`. That is the ledger's `sound-max` cause
  (`scripts/fxcap/parity-ledger.toml`, `scenarios = ["*"]`), and it differs
  before any key (`menu`). The marks there are fx's (`marks.txt`: `Sound
  level`, `Status line context`, `Prompt history`, `Status line workspace`),
  and the sweep puts each one's only row under `sound-max` with nothing
  unattributed (below). `sound-max` is an unowned defect cause, not an
  approval, so those five stay `unresolved-defect` until xfx has the level;
  no capture at this height leaves the Sound level row out of the menu's
  window while a page lands inside the list.
- Every mark in `marks.txt` is the same on the head as on fx (39 `same`).
- **Unit** (`green-unit.txt`): the 5 cases pass on the head.

## Mutants (`mutants.txt`, made by `mutants.py`)

**All 12 were killed.** Each ran on a `git clone --no-local` of the fix
(`c4bfa8b1`) in the scratch directory, never on the worktree: the clone's
tree reset, one edit (exactly one match in the named fn, or the run stops),
and the unit's five cases run (`cargo test --locked --lib -- at_24_rows`,
under the suite lock).

| list | the page key back to nothing | a page of one row | a page taken round the list (`rem_euclid`) where fx goes to the far end | more |
|---|---|---|---|---|
| `/settings` | killed (the menu case) | killed (the menu and the list cases) | killed (the menu case) | `C-j`/`C-k` back to nothing: killed; the list's moves sent to the menu under it: killed (the list case) |
| the column | killed (model and effort/speed cases) | killed (both) | killed (both) | `C-j`/`C-k` out of its key set: killed (both) |
| `/mcp ` | killed | killed | killed | -- |

So a revert, or a half-revert of any one list's page, its size, its far-end
rule or (where this unit added them) its `C-j`/`C-k`, fails at least one case.

## The sweeps (`sweep-*.md`, `sweep-compare-*.txt`)

`sweep.sh` against fx v0.0.16, `--jobs 2`, all four runs in one hold of the
suite lock, the base's build then the head's (scoreboards: `x𝒇x
v0.1.0-3d3d56c`, `v0.1.0-c4bfa8b`), with the ledger as it is at `fe316934`.
No ledger cause moves with this unit, so there is no cause's set of
scenarios to sweep; these are the unit's own and its neighbours.

- **24x120, `p5-page-keys-*`** (`sweep-24x120-base.md`, `-head.md`,
  `sweep-compare-24x120.txt`): pass **18/39 → 34/39**, 16 pairs moved to
  passing, **none dropped**. The five left are the settings menu's, each
  `sound-max` only (GREEN above).
- **40x120, the neighbours** -- `settings*`, `model-column*`, `model-effort`,
  `model-speed`, `model`, `p5-effort-fork-effort-column`, `p5-input-model*`,
  `p5-mcp-verbs`: 25 scenarios, the surfaces whose key routing this unit
  touched (`sweep-40x120-base.md`, `-head.md`, `sweep-compare-40x120.txt`):
  pass **133/206 → 136/206**, **none dropped**. The three that moved are
  `model-column-idle`'s `col-down`, `col-idle` and `col-typed`, which no key
  of this unit reaches (`/model`, a space, `o`): on the base run fx's column
  had listed the models and xfx's said `unable to load models`, the ledger's
  `catalog-window` cause (the catalog is down for the first nine seconds and
  the two binaries' starts race it). Not counted as this unit's.

## The gate (`gate.txt`, made by `gates.sh`)

**20 steps, 0 failed**, run once at `fe316934`, whose code is the fix's
(`c4bfa8b1`), in one hold of the suite lock (`gate.txt`). The steps are
INPUT-ROW-LIMIT's, unchanged: fmt, clippy (plain and fault-injection),
`cargo test` (plain and fault-injection), the `check-*` scripts, their
self-tests, the fxcap sweep and capture self-tests, both release builds,
smoke and smoke-tui. The tests: 5554 passed and 0 failed; 5591 and 0 under
fault-injection (15 binaries each; their result lines are copied into
`gate.txt`). The 1-minute load at a step's start was between 49.1
(check-no-stubs) and 15.3 (the release build). No step failed, and none was
rerun.

`gate.txt` says "tree clean: no" before and after. The only path not
committed was this folder, untracked while it was being written.

## `docs/parity.md` and the ledger

- full-screen TUI row: "Three of fx's lists that page so take no page key here
  yet ... Owner: PAGE-KEYS-SETTINGS-COLUMN" is now what they do, with this
  folder and its captures; the sibling keys above; and the two differences
  that remain (Open).
- `/model` (the inline column), `/effort` (the `/effort ` column), `/mcp` (the
  argument completions) and `/settings` (the menu and its Model row's list)
  rows: their page keys and `C-j`/`C-k`, each with its captures.
- **The ledger retires no cause**: none names this unit or these keys, and the
  captures end none (the settings menu pairs stay under `sound-max`). Its
  `src/tui/shell.rs` refs that this unit's lines moved are re-pointed, checked
  by text (`refs.txt`, made by `refs.py`: 17 refs, 0 wrong; the one
  `docs/parity.md` ref, `settings.rs:539`, went with the sentence it was in).

## Open

- **A slash list on a taller composer.** fx walks a slash list -- `/mcp `'s
  arguments among them -- with the arrows and a page only while the composer
  is one row (`capped.input_extra == 0`, `input_completion_runtime.zig:255`);
  past that they move the caret, and `C-j`/`C-k` still walk the list
  (`:175-180`). xfx's `offer_to_mcp_args`, its Up/Down as before and its page
  now, and the slash menu's page (INPUT-ROW-LIMIT's) walk it at any composer
  height. Uncaptured; stated in `docs/parity.md`; not a unit yet.
- **Up/Down over an empty `/settings` list.** fx's go on to the composer
  (`Menu.move` answers `false`); xfx's are spent, as before this unit. Its new
  `C-j`, `C-k` and page go on, as fx's do (the settings case's last steps).
  Stated in `docs/parity.md`; not a unit yet.
