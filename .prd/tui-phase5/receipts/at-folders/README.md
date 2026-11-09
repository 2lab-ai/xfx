---
title: 0.0.16 delta -- AT-FOLDERS receipt (I-U3, @ folders and the folder hold)
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, input, file-picker]
---

# AT-FOLDERS (I-U3): fx v0.0.16's `@` folders, and the 8 ms folder hold

Spec: the research branch's `.prd/tui-phase5/research/input.md` rows I4 and
I5, §2.3, §3's I4/I5 rules, §4's xfx-impact row, §5 U3 and H2, read as leads.
Oracle: fx v0.0.16 (`481104fd`, executable sha256 `8e6cda2e…`, checked
before every capture run; `capture.sh` and `sweep.sh` guard it too,
`FX_AUTO_UPGRADE=0`). Base: AT-HEIGHT's head `af2b6137`. Branch
`feat/p5-at-folders`. Every claim below names the file in this folder (or,
for fx's text, the quote file) that backs it.

**fx's lines are quoted in `../at-height/fx-src.txt`, lines 653-1361**, made
by `../at-height/fx_src.sh`, which this unit extended: the brief asks for
AT-HEIGHT's uncited lines and this unit's in the same file. Lines 1-651 are
byte-identical to AT-HEIGHT's (checked with `cmp` before the file was
replaced). `fx-src.txt:N` below means that file.

| commit | what |
|---|---|
| `7ef30e66` | RED: 15 cases, and the seams they need carrying the base's behaviour (`red-unit.txt`) |
| `4f36d8c6` | the fix (5 files, +181 -55 with its docs) and the parity row -- **the code head** |
| `2f08a27f` | AT-HEIGHT's uncited fx lines and this unit's, quoted; AT-HEIGHT's README sentence corrected; scenario `p5-input-at-folders` and its workspace |
| `b5322ead` | ledger refs re-pointed (`refs.py`) |
| `498ae606` | test only: two `Files` cases that counted a listing's rows, with the folder's row now |
| `71483927` | test only: the index's loading holds no frame (a mutant needed it) |
| `d59c41d4` | `mutants.py` |
| `9294e3f2` | ledger: the `at-folder-row` cause deleted |
| `18492259` | ledger refs at the head's tests (`refs.txt`: 16 checked, 0 wrong) |
| `9a83cad3` | test only: a key handed on under Ctrl-O writes nothing (the distrust's frame request there) -- **the head's build** (`v0.1.0-9a83cad`), the gate's and the sweep's commit |
| `6310d5c3`, (this commit) | this folder: the receipt, then the gate's result |

`git diff --stat 9a83cad3..HEAD -- . ':(exclude).prd/tui-phase5/receipts/at-folders'`
is empty: after the head's build only this folder changes.

## 1. AT-HEIGHT's leftover (opus r3 SHOULD 1)

- **The defect.** A key the `@` list does not take, pressed while a resize is
  pending, distrusted the list (`offer_to_files`' first block) and went on to
  `act` with no frame asked for. On a resize that comes back the same size
  (`Shell::resize` returns `Unchanged`, no frame of its own) nothing trusted
  the list again, so the next Down was spent where fx's moves the bar.
- **fx.** The resize signal asks for a frame of its own (`observeResizeSignal`,
  `render_request.zig:196-200`, `fx-src.txt:725-730`), and a committed frame
  that shows the list trusts it (`app_render_runtime.zig:1022-1023`,
  `fx-src.txt:518-519`). Opus cited these lines on fx's main tree; the pin's
  numbers are the ones here.
- **RED** (`red-unit.txt`): `a_key_the_list_hands_on_mid_resize_asks_for_the_frame_too`,
  opus's recipe on two keys, each on a list of its own: an unbound C-g
  (`0x07`, `Action::Ignore`) and a Redo with nothing to redo
  (`ESC[122;10u`). Both spent the Down.
- **Fix:** the distrust block asks for `Reason::Modal`. The test passes; the
  mutant that drops the request is killed (`mutants.txt`,
  `distrust-asks-nothing`).
- AT-HEIGHT's README said "That frame is what restores trust after a spent
  key"; it now says what restores trust after a resize and why xfx's distrust
  asks for the frame (`../at-height/README.md`, item 6).
- AT-HEIGHT's six uncited ranges are quoted at `fx-src.txt:653-723`
  (`app_input_runtime.zig:310-330`, `:1013-1020`, `:1317-1321`,
  `:1405-1410`, `app_lifecycle.zig:1198-1212`,
  `transcript/runtime.zig:7164-7168`). Each holds what AT-HEIGHT says: the
  four key arms end in `request(.footer)` (`:330`, `:1015`, `:1019`,
  `:1321`, `:1409`); the resume calls `requestTerminalReset` (`:1210`); the
  reset sets `terminal_reset_pending` (`:7167`).

## 2. What fx v0.0.16 does (I4, 7c8ef4a3; I5, 09d2e934 and 6aa678d9)

1. **Enter selects a folder, Tab opens it.** Enter calls
   `applyFilePickerSelection(.select)` (`input_completion_runtime.zig:1042`,
   `fx-src.txt:750-817` covers `:967-1032`; `submitFilePickerOnEnter` at
   `fx-src.txt:584-607`), Tab `.open` (`:957`, `fx-src.txt:561-583`). On a
   folder, `.select` encodes `path + "/"` with `.directory = false`, so a
   quote is closed, and adds the terminator unless one is there (`:993-1012`).
   fx's own test: `@srcn` → Tab `@src/nested/`, Enter `@src/nested/ `;
   `before @srcn tail` → Enter `before @src/nested/ tail` with the caret past
   the space (`app_input_runtime.zig:6708-6757`, `fx-src.txt:1045-1096`).
2. **A listed folder is its own first row.** `completeCancellable` reserves
   row 0 for `listedFolder(parsed)` and lists the entries in the rest
   (`path_completion.zig:79-84`, `:124-130`, `fx-src.txt:818-919`):
   `src/` lists `src`, `~` lists `~`, `.` lists `.`; none for a partial
   name, the root (`prefix.len < 2`) or a doubled `/`, and none for a folder
   that is not terminal-safe or not representable (`:133-143`;
   `isRepresentable`, `file_picker_path.zig:14-16`, `fx-src.txt:1004-1017`).
   fx's tests: `fx-src.txt:942-967`, `:968-1003`. The row has no matched
   spans. The list's `capacity` is 32 (`file_completion_state.zig:7`,
   `fx-src.txt:1042-1044`), so the folder takes one of them.
3. **The folder hold.** `FileListingWait` (`picker_state.zig:99-139`,
   `fx-src.txt:1135-1177`): `hold_ms = 8`; `hold(in_flight, now)` starts the
   wait on the first in-flight call and holds while `active`; a listing that
   ends records whether it outlasted the wait, and the next listing is not
   held after a slow one but is still `measuring`, so one that lands inside 8
   ms turns the hold back on; a clock that steps backwards ends it.
   `holdFrameForFileListing` harvests the listing first, resets the wait when
   no picker is active, and holds only for a directory request
   (`!state.indexed and state.directory_request != null`,
   `input_completion_runtime.zig:817-835`, `fx-src.txt:1209-1238`). The loop
   skips `flushRequestedFrame` while it holds (`main.zig:3007`,
   `fx-src.txt:1340-1361`) and polls every 1 ms while `measuring`
   (`nativeLoopPollTimeoutMs`, `main.zig:191-207`, `:1140`,
   `fx-src.txt:1287-1339`). fx's tests: `fx-src.txt:1181-1208`,
   `:1239-1286`.

## 3. What xfx does now

- `file_picker::completion` takes a `FolderAction` (`Open` for Tab, `Select`
  for Enter, `Shell::offer_to_files`): a selected folder is encoded as
  `path/` with no directory flag, so the quote closes, and gets the space or
  steps over the one there.
- `file_index::rank_listing` puts `listed_folder`'s row first and truncates
  the entries to `MOST_RESULTS - 1` then. `listed_folder` is fx's: no row for
  a name, the root, a doubled `/`, or a folder that is not terminal-safe.
  xfx paths are `String`s, so "representable" is non-empty and
  terminal-safe, which the checks already require.
- `file_picker::ListingWait` is `FileListingWait` on `Instant`s
  (`checked_duration_since`, so a backwards step ends the wait).
  `Shell::hold_frame_for_file_listing` takes the workers' results first
  (`settle_files`), forgets the wait when no list is up, and counts a
  listing in flight as a list that browses a path (`!indexed()`) and still
  says `Loading`, and -- since round 2 -- not while the full transcript is
  up. `Shell::file_listing_hold_pending` is `measuring`.
- The loop: `commit_or_hold` holds the turn's **frame** while the shell
  holds, and `wait_timeout` is 1 ms while a listing is in its first tick,
  else the 8 ms `TICK`. The listing's worker wakes nothing, so the 1 ms poll
  is what hears it, as in fx.
- **Round 2 (`round2/README.md`)**: only the frame is held, as only fx's
  `flushRequestedFrame` is. The status report, the title (on its own) and
  the bells go out on their tick. Round 1 held the whole tick; that is gone.

## 4. RED (`7ef30e66`, `red-unit.txt`)

The 15 cases at `7ef30e66`, where the seams carry the base's behaviour
(`completion` ignores the action, `ListingWait` never holds, the shell never
holds, the loop's wait is the tick): **15 failed, by assertion.** Among them:
- Enter on `@srcn`'s folder left `@src/nested/` (fx `@src/nested/ `);
  `@"./space dir/` Enter left it open (fx `@"./space dir/" `);
- `src/` listed `src/main.zig` alone (fx `src`, then `src/main.zig`);
  `filepicker`'s `see @src/` showed `src/deep/` first (fx `src/`);
- the loop wrote a frame showing `@tree/` before the folder's rows;
- the wait's state machine never held.

## 5. GREEN: the pairs (`pairs.txt`, `pairs.py`; captures in `fx/`, `xfx/`, `xfx-base/`)

**23 pairs, all 23 identical on the head, cell for cell** (cells.py, row 1
aside); the base differs on 20. Row 1 of every capture names its build
(`pairs.txt`, first line: head `v0.1.0-9a83cad`, base `v0.1.0-af2b613`).

| scenario | snaps (size) | base rows differing | head |
|---|---|---|---|
| `p5-input-at` | `at-src`, `at-src-enter`, `at-src-tab` (40, 18, 16 rows) | 1-13 | 0 |
| `p5-input-at-dir` | `dir-query`, `dir-enter` | 0, 5 | 0 |
| `p5-input-at-folders` (new) | `space-query`, `space-open`, `space-enter`, `space-x`, `sto-query`, `sto-x`, `src-open`, `src-enter-x` | 0-5 | 0 |
| `filepicker` | `at-dir`, `at-dir-down`, `at-dir-cj`, `at-tab` | 4, 4, 4, 1 | 0 |

- `p5-input-at-folders` is fx's own test as keys: `@./space`, Tab →
  `@"./space dir/` listing `./space dir/` above `./space dir/item.txt`,
  Enter → `@"./space dir/"`, and an `x` typed after it shows the space and
  that the list closed (`@"./space dir/" x`). Then `@src/sto` Enter `x` →
  `@src/store/ x`, and `@src/` Enter on its own row `x` → `@src/ x`. Its
  workspace is four files; no local path is in it.
- **The hold is not in a capture**: H2 says keystrokes cannot stage an 8 ms
  window, so the rule is pinned by the unit tests in §7 with a driven
  listing and a given clock.

## 6. Mutants (`mutants.py`, `mutants.txt`)

Each rule broken alone in a `git clone --no-local` throwaway at `d59c41d4`
(the code head's code, every test but `9a83cad3`'s), never in the worktree;
the unit's tests (`TESTS`) against it in their own process group, 900 s each,
under the suite lock. **31 run, 31 killed.**
- I4 (8): Enter opening a folder, Tab selecting one, no `/`, the quote left
  open, no space, the caret before the space; Tab and Enter swapped at the
  shell.
- The folder row (6): none; one for a partial name, for the root, for a
  doubled `/`, for an unsafe folder; the row taking no slot of the 32.
- I5 (16): the loop never holding, the shell never holding, the hold for an
  index search, for a loaded folder, without taking the worker's rows first;
  a closed list keeping the wait; keystrokes restarting it; the slow folder
  never recorded or recorded forever; `active` ignoring it; `measuring`
  equal to `active`; `<=` 8 ms; a backwards clock extending it; a 16 ms hold;
  the loop's wait always the tick; `pending` as `active`.
- §1 (1): the distrust asking for no frame.

## 7. Unit tests (in the gate's `cargo test`)

| test | pins |
|---|---|
| `file_picker::tab_opens_a_folder_and_enter_mentions_it` | fx's `app_input_runtime.zig:6708-6757` (`fx-src.txt:1045-1096`), and a file the same either way |
| `file_picker::a_mentioned_folder_closes_its_quote` | fx's `input_completion_runtime.zig:2200-2235`; `~notes` stays `./~notes/` |
| `file_picker::a_folder_listing_holds_its_frame_for_one_tick` | fx's `picker_state.zig:1087-1112`, line for line |
| `file_picker::a_listing_after_a_slow_folder_is_watched_so_the_wait_can_come_back` | fx's `input_completion_runtime.zig:2272-2282` |
| `file_index::a_listed_folder_is_its_own_first_row` | fx's `path_completion.zig:307-330`, `:401-434`; the root, `//`, unsafe folders, the 32 |
| `file_index::a_browsed_directory_lists_its_entries_by_name` | updated: `src/` first |
| `file_index::a_browsed_directory_is_read_once_on_a_worker_and_then_ranked_here`, `home_is_where_a_tilde_goes` | updated: the folder's row |
| `shell::enter_on_a_folder_row_mentions_the_folder_and_closes_the_list` | `@src` Enter, `@src/` Enter on its own row, `@src/de` Enter `x` |
| `shell::tab_on_the_folders_own_row_lists_it_again` | `at-src-tab` |
| `shell::a_folder_with_a_space_is_mentioned_inside_its_quotes` | the quoted case through the shell |
| `shell::a_later_at_puts_the_rows_under_itself_and_down_and_tab_choose`, `a_directory_choice_keeps_the_list_up_on_that_directory` | updated: `filepicker`'s `at-dir`, `at-dir-down`, `at-dir-cj`, `at-tab` |
| `shell::a_keystroke_that_lists_a_folder_holds_its_frame_until_the_rows_land` | fx's `input_completion_runtime.zig:2237-2270`, with a driven listing and a given clock |
| `shell::after_a_slow_folder_the_frame_goes_at_once_until_a_fast_one` | skip, watch, re-arm, and a closed list forgetting the slow folder |
| `shell::only_a_folder_listing_holds_the_frame_not_the_index` | `!state.indexed` (`:828`) |
| `event_loop::the_frame_of_a_keystroke_that_lists_a_folder_waits_for_its_rows` | `main.zig:3007`, `:191-207`: no bytes while held; the frame with the rows, never `indexing files`; the 1 ms wait |
| `event_loop::a_key_the_list_hands_on_mid_resize_asks_for_the_frame_too` | §1 |
| `event_loop::a_key_handed_on_under_the_full_transcript_writes_nothing` | §1: the request also fires under Ctrl-O (`self.full.is_open()` is in the same block); the frame it brings writes no byte there |

`file_index::tests::driven_listing` holds a listing in flight until the case
sends its entries; no case sleeps.

## 8. The sweep (`sweep-base.md`, `sweep-head.md`, `sweep-compare.txt`)

`sweep.sh` against fx v0.0.16 at 40x120, `--jobs 2`, under the suite lock,
over AT-HEIGHT's 25 picker and band scenarios plus `p5-input-at-folders`
(26): `p5-input-*`, `filepicker*`, `composer*`, `workspace-index`,
`startup`, `h1-resize`, `model-column`, `settings-scrollback`,
`p5-mcp-config`, `p5-mcp-verbs`. The base's build (`v0.1.0-af2b613`, a
`--no-local` clone at `af2b6137`), then the head's (`v0.1.0-9a83cad`), both
scored with the ledger at `18492259`, which no later commit changes.

- **pass 85/157 → 99/157: 14 move to normalized-identical, none drops, no
  other verdict changes** (`sweep-compare.txt`): `p5-input-at` `at-src`,
  `at-src-enter`, `at-src-tab`; `p5-input-at-dir dir-enter`; `filepicker`
  `at-dir`, `at-dir-down`, `at-dir-cj`, `at-tab`; and six of the eight
  `p5-input-at-folders` snaps (`space-query` and `sto-query` show no folder
  row and pass on both).
- With `at-folder-row` deleted, the base has those 14 among its rows no cause
  covers (`sweep-base.md`); the head has no `@`, picker or composer snap
  without a cause (`sweep-head.md`). What stays apart there is `at-ranking`
  (I-U2), `at-resize-band-at-the-bottom` (RESIZE-REPAINT), and the other
  clusters' rows, with the same verdict on both sides.

## 9. The gate (`gate.txt`, `gates.sh`)

Every CI step the brief lists (`gates.sh`, AT-HEIGHT's unchanged), in a
`--no-local` clone at `9a83cad3`, in one hold of the suite lock, with
`TMPDIR=<scratch>/atfg`. **20 steps, 0 failed**, the tree clean before and
after (load 14-45 over the run):
- fmt, clippy, and clippy with fault injection;
- `cargo test`: 5484 passed, 0 failed; with fault injection: 5521 passed, 0
  failed (AT-HEIGHT's head: 5470 and 5507);
- the identity, stubs, secrets, preview-contract, preflight-cost and
  usage-clock checks, and their self-tests;
- the fxcap sweep and capture self-tests (68 capture tests, with the new
  workspace's `space dir/` in the tree);
- both release builds; smoke; smoke-tui: 37 scenarios and the oracle, 1168
  checks, 0 failures.

## 10. docs/parity.md and the ledger

- The `@` file picker row: "Tab and Enter put the path in (`@path ` for a
  file, `@dir/` and the listing goes on)" is rewritten -- Enter mentions a
  folder (`@dir/ `, the quote closed, the list closes), Tab opens it; a
  path-shaped query lists its folder first; the 8 ms hold, its skip and
  re-arm; and (round 2) what the hold holds -- the frame alone -- and no
  hold under the full transcript. The
  receipt pointer names this folder.
- `file_picker.rs`' module doc ("Tab and Enter both …") is rewritten the same
  way; `completion`'s and `complete_file`'s docs too.
- `scripts/fxcap/parity-ledger.toml`: the `at-folder-row` cause is deleted
  (its 8 snaps are identical, §5); its 16 `src/tui/*.rs` refs are re-pointed
  by text (`refs.py`, `refs.txt`: 16 checked, 0 wrong).
- No help entry and no registry count names these keys; `check-no-stubs` has
  no row for them.

## 11. Not this unit's

- **CTRLO-AT-KEYS** (opus r3 SHOULD 2): Tab under Ctrl-O over an unreadable
  list re-reads the index. Untouched.
- fx's `isCurrentFileCompletion` check before a choice is applied
  (`input_completion_runtime.zig:981-985`, `fx-src.txt:765-769`) is not
  ported, here or at the base. It belongs to the queued AT-STALE-SELECTION
  (dispatcher, round 1).
- **I-U2 ranking**: a folder query's entries are still ranked by xfx's eight
  facts; with no name typed they tie, so byte order, as fx.
