# MODEL-FUZZY (U8, I13): `/model`'s search ranks as fx v0.0.16's does

fx v0.0.16 (`vercel-labs/fx@481104fd`, f9ddaff8 *Make model search fuzzy
across provider and model names*) reads a model search against the whole
`provider/model` id, word by word, and lists the matches in three ranks:
substring, then words, then fuzzy, each in catalog order. xfx on main matched
a substring of the id or of the vendor, in catalog order. This unit ports the
matcher and the ranks to every model picker xfx has, and keeps the menu's mark
on its model by id across a catalog swap.

The spec is `research/tui-phase5:.prd/tui-phase5/research/input.md` §2.8,
§3 I13, §4 row I13 and §5 U8, read as leads. The base is `main` at
`ccced1f5`. Every claim below names a file in this folder.

| commit | what |
|---|---|
| `cda3c90e` | RED: fx's tables against main's APIs, and the two scenarios with their catalogs (`red.txt` §1) |
| `08763761` | the matcher (`src/tui/model_search.rs`), read by the menu and the column |
| `f0aed140` | `docs/parity.md`'s `/model` row; the rank scenario's note |
| `260ffc85` | RED: Tab in the column leaves the mark where Down put it (`red.txt` §2) |
| `fc8ec00e` | its fix -- **the code head**; the xfx captures are this binary's |
| `8c6da0fc` | tests only: two pins the mutants need (below) |
| `2a43306e` | `docs/parity.md`: the column's mark after Tab |
| `307046f1` | this receipt, before its gate |
| `0bf5d2ab` | tests only: a comment that named the vendor match |
| `06c08b90` | tests only: the Alt-Enter column test held c1db9191's row -- **the gate's head** |
| (this one) | this receipt's gate |

`fc8ec00e`..`06c08b90` changes nothing outside `#[cfg(test)]` and
`docs/`/receipts (`git diff fc8ec00e 06c08b90 -- src/` touches only the
`mod tests` blocks of `model_menu.rs`, `model_column.rs` and `shell.rs`), so
the captures from `fc8ec00e`'s binary stand for the head.

The xfx captures in `xfx/` are from the binary built at `fc8ec00e` (row 1:
`x𝒇x v0.1.0-fc8ec00 [debug]`); those in `red/` from `ccced1f5`'s (row 1:
`x𝒇x v0.1.0-ccced1f [debug]`). fx is the v0.0.16 release binary, sha256
`8e6cda2e…`, which `capture.sh` ran with `FX_AUTO_UPGRADE=0` and checked
after each run (`guards: … sha256 8e6cda2efddb unchanged`).

## What fx v0.0.16 does

`fx-src.txt` cites every line. In short:

- **The matcher** (`matchModelQuery`, `model_cache_runtime.zig:371-389`). The
  blanks round the query go. If the id holds the query as typed, ignoring
  ASCII case, that is a **substring** match; an empty or all-blank query is
  one, for every id. Otherwise each blank-separated word must match:
  - **words**: its letters and digits form a run among the id's letters and
    digits, every other byte ignored (`:398-413`);
  - **fuzzy**: its letters and digits appear in order, the first at the start
    of an id word (`:415-436`). A word starts after punctuation or where
    letters turn into digits or back, never at a change of case (`:438-447`).
  - A word with no letter or digit is passed over (`:381`). The match is the
    worst word's, and one word that fails makes it none.
- **Bytes, not characters.** Everything is `std.ascii`: a byte outside ASCII
  is never a letter or digit, so it counts as punctuation.
- **The ranks** (`:349-369`): substring, words, fuzzy, in that order; equal
  matches keep catalog order (`modelMenuItemAt`, `:298-315`). The tab filters
  first (`:340-347`).
- **The id alone.** The old matcher also searched the provider field
  (`:252-261@c1db9191`). The new one reads only `item.id`, and a row's name
  or aliases never.
- **The menu.** `Models N` counts the matches (`model_menu_presentation.zig:
  155-158,224-230`). An empty search result says `No models found.`
  (`:394-401`). Matched letters are not highlighted (`composeTitleRow`,
  `:301-333`); only the file picker has `matched_spans`. Enter takes the
  marked row of the ranked list (`:270-274`).
- **The mark across a swap** (`modelMenuDisplayIndex`, `:317-338`, in
  `hydrateMenuSnapshot`, `:1121-1146`). A revalidated catalog puts the mark on
  the same id, at its place in the ranked list. f9ddaff8 replaced the old
  catalog-order lookup there.
- **Every caller.**
  - The menu, and `/settings`' Model row, which lists the menu's rows
    (`settings_menu_presentation.zig:277-310`).
  - The `/model <query>` column (`modelCompletions`, `:895-913`): the same
    ranks, filled into 32 places rank by rank. Its readers are the render,
    the arrows, Enter's exact id, Left's re-marking, and Tab, which writes
    through a replacement that puts the mark back on row 0
    (`composer_text_replacement.zig:73-92`).
  - A space typed in the column inserts nothing, unless it follows an exact
    id (`app_input_runtime.zig:1355-1363`).
  - **Not a caller**: `/model <word>` sent as a command. It resolves by
    `fuzzyModelScore` (`session_commands.zig:1425-1448,1760`), whose body is
    byte-identical at `c1db9191` and `481104fd`. That closes the research's
    "[unverified] router form": the router keeps its score.

## How it was captured

```
FX_BIN=<fx v0.0.16> TMPDIR=<tmp> scripts/fxcap/capture.sh fx  <scenario> 40x120 <dir>
XFX_BIN=<xfx>       TMPDIR=<tmp> scripts/fxcap/capture.sh xfx <scenario> 40x120 <dir>
```

The two binaries ran with the same `TMPDIR`, outside the repository, one after
the other. No capture names a local path: `grep -rlE` for the home directory,
the user name and the harness's temp root finds no file here.

- **`p5-input-model`** (on main): the research's five queries (`q-cs45`,
  `q-anthropic-opus`, `q-sonnet45`, `q-zai-glm`, `q-grok-fast`) and
  `inline-gpt6`.
- **`p5-input-model-rank`** (new; catalog `p5-input-model-rank.json`). The
  edge cases `fx-src.txt` names, one snap each:
  - the three ranks reordering the catalog: `q-gpt5` lists `acme/gpt5-lite`,
    `openai/gpt-5`, `openai/gpt-4.5-preview`, where the menu's order is
    gpt-5 first;
  - one rank keeping catalog order: `q-gpt-5-words` (`gpt 5`) lists gpt-5,
    gpt5-lite, gpt-4.5-preview;
  - word starts: `q-omi` finds `acme/gpt4omni`, `q-mi` nothing; `q-dsc` finds
    `acme/DeepSeekCoder`, `q-sc` nothing, since a case change is not a start;
  - every word required: `q-sonnet-opus`;
  - punctuation-only words: `q-punct` (`. /`) finds nothing, and
    `q-anth-slash` (`anthropic / sonnet`) finds sonnet;
  - blanks: `q-padded` and `q-blank`;
  - mixed ranks: `q-sonet` (`anthropic sonet`);
  - tabs over ranks: `q-gpt5-openai`, `q-gpt5-all`;
  - the mark and Enter: Down marks openai/gpt-5, the second rank's row
    (`q-gpt5-down`), and Enter switches to it (`q-gpt5-enter`);
  - the column: `c-gpt5`, `c-gpt5-down`, `c-gpt5-tab`, typed
    `c-anthropic-opus` and `c-gpt5-lite` (the space dropped), pasted
    `c-paste-gpt-5` and `c-paste-anthropic-opus` (the spaces kept), `c-mi`,
    and `c-cs45`/`c-cs45-enter`, where Enter switches to the first ranked
    row.
- **`p5-input-model-swap`** (new; catalogs `p5-input-model-rank.json`, then
  `p5-input-model-rank2.json`, which adds `acme/gpt5-pro` first). Every
  catalog read answers 4 s after it is asked.
  - `first`: the launch's read is in.
  - `before`: the second `/model` opens past the 5 s floor and asks again.
    `gpt5` and Down mark openai/gpt-5, second of three.
  - `after`: the new catalog lands. acme/gpt5-pro ranks first, and the mark
    stays on openai/gpt-5, now third.
  - `enter`: Enter switches to it.
  - The gateway logs (`fx/`, `xfx/`, `red/` `…-gateway.log`) show both
    binaries asking twice, the second ask answered with `-> second catalog`.

## RED

`red.txt` holds the three parts.

1. **Five tests on main's matcher** (commit `cda3c90e`, run over `ccced1f5`'s
   code). All fail:
   - fx's table: `"claude sonnet 4.5" finds anthropic/claude-sonnet-4.5`;
   - the ranks: `left: ["acme/gpt5-lite"]` against the three;
   - the whole id: `anthropic opus` finds nothing;
   - the column: the same one row against three;
   - Enter: `cs45` takes nothing.
2. **Tab's mark** (`260ffc85` over `08763761`): the mark is on the second
   row after Tab.
3. **The pairs on main's binary**: 29 of 38 differ.
   - `p5-input-model`: six of seven. Row 5 counts `Models 0` where fx has 1
     or 2, row 7 says `No models found.` where fx lists the model, and
     `inline-gpt6` says `no matching models`.
   - `p5-input-model-rank`: 20 of 27. Main finds nothing for `omi`, `dsc`,
     `gpt 5`, `anthropic / sonnet` and `anthropic sonet`, and one row for
     `gpt5`. The column has the same misses. `q-gpt5-enter` switches to
     acme/gpt5-lite, so every later snap differs in its transcript too.
   - `p5-input-model-swap`: three of four. Main lists 1, then 2 rows, and
     Enter switches to acme/gpt5-lite.
   - The nine that already pair are the guards: the bare menu (twice),
     `q-blank`, `q-padded` (a substring after the trim, which main had),
     `q-mi`, `q-sc`, `q-punct`, `q-sonnet-opus` (nothing on either), and
     the swap's `first`.

## GREEN

`green.txt` is `scripts/fxcap/cells.py` over every pair, row 1 (each
product's banner) left out. **All 38 pairs are identical.**

| scenario | snaps, all identical |
|---|---|
| `p5-input-model` | `model`, `q-cs45`, `q-anthropic-opus`, `q-sonnet45`, `q-zai-glm`, `q-grok-fast`, `inline-gpt6` |
| `p5-input-model-rank` | `model`, `q-gpt5`, `q-gpt-5-words`, `q-omi`, `q-mi`, `q-dsc`, `q-sc`, `q-sonnet-opus`, `q-punct`, `q-anth-slash`, `q-padded`, `q-blank`, `q-sonet`, `q-gpt5-openai`, `q-gpt5-all`, `q-gpt5-down`, `q-gpt5-enter`, `c-gpt5`, `c-gpt5-down`, `c-gpt5-tab`, `c-anthropic-opus`, `c-gpt5-lite`, `c-mi`, `c-paste-gpt-5`, `c-paste-anthropic-opus`, `c-cs45`, `c-cs45-enter` |
| `p5-input-model-swap` | `first`, `before`, `after`, `enter` |

The unit tests:

- `model_search::tests` (4): fx's table rank by rank (`:2060-2084`), the word
  starts, the worst word and punctuation-only words (a byte outside ASCII
  included), and the ranks' order;
- the RED tests above, now passing;
- the two pins `8c6da0fc` adds.

## The next break: Tab's mark

With the ranks in, the first GREEN take paired 37 of 38. `c-gpt5-tab` differed
on rows 9-10. Tab had written `/model openai/gpt-5`, and that line now lists
`openai/gpt-4.5-preview` too, a fuzzy match. fx marked row 1, the model
written; xfx left the mark on row 2, where Down had put it.

- fx writes the marked row as a replacement, and its commit sets
  `model_completion_index = 0` (`composer_text_replacement.zig:73-92`).
- xfx's `Column::complete` kept the index. Before the ranks the written line
  listed only its own row, so `index % 1` hid it.
- The RED is `260ffc85`'s test (`tab_puts_the_mark_on_the_first_row_the_written_line_lists`),
  and the fix is `fc8ec00e`.

A second one turned up in the gate's first run, a test rather than a screen.
`shell::tests::a_column_over_a_line_the_caret_has_left_hangs_from_the_caret_rows_start`
types `/model xai/grok-4.7`, then Alt-Enter, and held `no matching models`.

- That was c1db9191's answer: its completions did not trim the query
  (`model_cache_runtime.zig:690@c1db9191`).
- fx v0.0.16 trims the trailing newline (`:375`) and lists the model. Its
  `model-anchor` `alt-enter` frame shows it, and the sweep moves that pair to
  identical (below).
- `06c08b90` sets the test to fx v0.0.16's row.

Past that, the trace runs through every reader of `Column::models` and
`ModelMenu::items` -- the render, the arrows, Enter, the exact id, Left,
`/settings`' row, the count and the revalidation -- and each reads the one
ranked list. Nothing else in xfx searches models, and the router keeps its
score (above).

## Mutants

`mutants.txt` is `mutants.py` on a `git clone --no-local` of this branch at
`8c6da0fc`, with `GIT_DIR`, `GIT_INDEX_FILE` and `GIT_WORK_TREE` unset and a
target directory of its own, under `suite-lock.sh`. Each mutant ran
`timeout 900 cargo test --locked --lib -- tui::model_menu tui::model_column
tui::model_search`, and the clone was reset before each. The control (the unit
as committed) passes 47 tests.

| mutant | verdict | killed by (among others) |
|---|---|---|
| tier order: no ranks, catalog order | KILLED (5) | the menu's and the column's `gpt5` order |
| tier order: words above substring | KILLED (5) | the same, and Enter/Down on the ranked rows |
| word start: any letter starts a word | KILLED (5) | fx's table (`mi`), the word-start test |
| word start: a case change starts a word | KILLED (2) | `sc` on `acme/DeepSeekCoder` |
| word start: no letter/digit turn | KILLED (2) | `omi` on `acme/gpt4omni` (fx's table) |
| all words: a word that matches nothing is passed over | KILLED (5) | `sonnet opus` (fx's table), `zhipu` |
| id only: the model part after the slash | KILLED (11) | `anthropic opus`, `ANTHROPIC/claude` |
| id only: the column also searches names and aliases | KILLED (1) | the column's `zhipu` pin (`8c6da0fc`) |
| id only: the menu also searches the vendor apart | **SURVIVED -- equivalent** | -- |
| query: the blanks round it kept | KILLED (3) | `" \t"` and `" Claude-Sonnet "` (fx's table) |
| column: the 32-row cap before the ranks | KILLED (1) | the cap test (`acme/gpt5-last`, 33rd) |
| column: Tab keeps the mark where it was | KILLED (1) | `tab_puts_the_mark_on_the_first_row_the_written_line_lists` |
| menu: the mark looked up in catalog order on a revalidation | KILLED (2) | the ranked-vs-catalog pin (`8c6da0fc`) |

**The survivor is equivalent by construction, and was run to show it.** The
vendor is the part of the id in front of its first `/`. Any query the vendor
holds, the id holds as typed, so it is already a substring match. Putting the
old `|| vendor contains query` back changes no list.

`8c6da0fc` added the two pins after a dry read of the mutants. Without them,
two mutants would have survived:

- the mark looked up in catalog order: fx's own case puts the mark at the
  same index either way (2);
- the column's name and alias search: the menu's id-only test cannot see the
  column.

## Sweep

`sweep.txt` compares two runs of the sweep over the 39 scenarios matching
`*model*`, `*effort*` and `settings-model*`, which is every scenario that
reaches the menu, the column or `/settings`' Model row. Both runs used this
branch's `scenarios/` and the same fx. Only the xfx binary differs: main's
(`ccced1f5`) for the base, the code head's (`fc8ec00e`) for the head.
`sweep-base.md` and `sweep-head.md` are the scoreboards, both PARTIAL by
design (`--only`).

- **Pass: 174 / 233 on the base, 204 / 233 on the head.** Neither run had a
  failed or timed-out scenario.
- **30 pairs moved to identical or normalized, and none dropped.** They are
  the 29 targets (6 + 20 + 3 above) and `model-anchor` `alt-enter`. That last
  one types `/model xai/grok-4.7`, then Alt-Enter, so the query ends in a
  newline. fx trims `\n` with the other blanks (`model_cache_runtime.zig:375`)
  and lists the model. Main said `no matching models`. The port fixes it.
- **26 pairs are unresolved on both, with the same rows.** None reads the
  search:
  - `model` `typed`: the help summary row (ledger cause `C7-help-entries`);
  - `model-parked` `menu-loading`: cause `activity-tokens-before-request`;
  - the `p5-effort-fork-oldread-*`, `-slash-rank`, `settings-effort` and
    `settings-model*` pairs: the fork/resume pickers and `/settings`'
    `Sound level  off  on  max` row, other units' rows.
- **3 more are unresolved on both with rows that changed.** They differ only
  in the sweep's own temp directory name (`fxsw.<random>`), which is written
  on their `Workspace` or `doctor` rows, the same on fx and xfx within one
  sweep.
- `p5-input-model-fail` pairs on both: I14's sentence was already fx's on
  main.

## Gate

`gate.txt` is one run of the gate under `suite-lock.sh` at
`06c08b90`, with the tree clean before and after. It covers every step in
`.github/workflows/ci.yml`, the brief's two `--no-fail-fast` suites, and
`sweep.yml`'s own tests (`test-sweep.sh`, `test-capture.sh`). **Every step
exits 0.**

| step | result |
|---|---|
| `cargo fmt --check`; `clippy --all-targets -D warnings`, plain and `--features fault-injection` | 0 |
| `cargo test --locked --all-targets` | 0 |
| `cargo test --locked --no-fail-fast` | 0 (5470 passed, 0 failed) |
| `--features fault-injection`: `--test tui`, `--lib`, and `--no-fail-fast` | 0 (5507 passed, 0 failed) |
| `check-no-stubs`, `check-no-secrets`, `test-gate-env`, `check-xfx-identity` | 0 |
| `check-preview-contract`, `test-preview-contract`, `check-tui-preflight-cost`, `check-usage-clock` | 0 |
| release build, `smoke.sh`, the faulty build, `smoke-tui.sh` | 0 (smoke-tui: 37 scenarios + the oracle, 1168 checks, 0 failures) |
| `fxcap/test-sweep.sh`, `fxcap/test-capture.sh` | 0 (capture: 76 tests OK) |

The run before it, at `0bf5d2ab`, failed one test: the Alt-Enter one above,
the next break. That one fixed, nothing else failed.

The source change outside tests is `src/tui/model_search.rs` (150 lines
before its `#[cfg(test)]`, doc comments included) and about 30 lines in `model_menu.rs` and
`model_column.rs`, well under the brief's 600.

## `docs/parity.md`, the ledger, the refs

- The `/model` row (`docs/parity.md:93`) changes in three places:
  - the menu's search: the three ranks over the whole id, no vendor match, no
    highlight, a blank search, and the mark kept by id;
  - the column: the same ranks and the first 32, and the dropped space;
  - the router form: `fuzzyModelScore`, which v0.0.16 kept.
- `parity-ledger.toml`: no divergence or cause named these scenarios or rows
  before (`grep` for `p5-input-model`, `No models found`, `no matching models`
  finds only `p5-input-question`'s cause), and every pair now pairs, so none
  is added or removed.
- Refs: no `model_menu.rs:<n>` or `model_column.rs:<n>` reference exists in the
  ledger, `docs/`, `src/` or `tests/`, so none needed re-pointing.

## Decisions (reversible)

- **One module, `src/tui/model_search.rs`**, read by both pickers, so the
  menu and the column cannot drift apart. fx shares one function the same way.
- **`Item.vendor` deleted.** The search was its only reader. The tab is
  worked out once, when the row is built.
- **Bytes, as fx reads them**, not Unicode lowercasing: `é` is punctuation in
  both, so the same query finds the same ids.
- **The router form untouched.** fx did not change `fuzzyModelScore`, so xfx's
  `provider::model::fuzzy_model_score` stays.

`.ansi` files are tmux's re-serialization of cell attributes
(`capture-pane -e`), not the programs' wire bytes.
