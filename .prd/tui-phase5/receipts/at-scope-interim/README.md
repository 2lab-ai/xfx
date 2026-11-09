---
title: 0.0.16 delta -- AT-SCOPE-INTERIM (fx's interim after a change of the workspace access)
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, input, file-picker]
---

# AT-SCOPE-INTERIM

Base: `b4cc300f` (AT-FOLDERS, closed). The unit copies fx's model of a
change of the workspace access for the `@` index: a scope epoch, a walk
that runs on, and `Files unavailable. tab to retry; esc to dismiss.` on an
indexed list until a walk of the new scope is whole. It retires the gap
AT-FOLDERS named and the two residuals of stopping the walk
(r10-gpt6.md, r10-opus.md).

| commit | what |
|---|---|
| `74960592` | RED: seven cases (`red-unit.txt`) |
| `c445c1d7` | the fix: `Files`' scope epoch, the walk run on; old cases adapted (§4) |
| `729d2e82` | `docs/parity.md`, the `@` file picker row (§8) |
| `aa83e2da` | the ledger's 12 `shell.rs` refs re-pointed |
| `274fc435` | AT-FOLDERS' receipts: each residual line says it has ended (§8) -- **the sweep's build** (`v0.1.0-274fc43`) |
| `9b69f7e4` | a path list in the interim lists its folder |
| `08f3798a` | the ledger's 6 refs past that case (`refs.txt`: 16 checked, 0 wrong) |
| `7b842c2c` | this folder's runners, RED run and fx quotes -- **the gate's commit** |
| (receipt commit) | this README, `gate.txt`, `tests.txt`, `mutants*.txt`, the sweep files and `samepair.*` |

Every fx line cited here is printed in `fx-src.txt` (`fx_src.sh`, at
`481104fd`); "fx-src.txt:N" is its line.

## 1. fx at the pin

- **Every change of the access moves the scope epoch.** `install` compares
  the whole access (`app_workspace_runtime.zig:134`, fx-src.txt:34) and on a
  change calls `invalidateScope` -- `scope_epoch +%= 1` (`:99`,
  fx-src.txt:13) -- then `refreshFileIndex` with the new epoch (`:140`,
  fx-src.txt:40). The roots moving or not does not enter.
- **A walk keeps the epoch it started with** (`loading.scope_epoch =
  self.scope_epoch`, `file_index.zig:386`, fx-src.txt:131).
- **A change stops no walk.** `refreshOwnedRoots` with a thread running
  only replaces or coalesces the pending scope and returns (`:494-497`,
  fx-src.txt:241-244; `replacePendingRoots`, `:511-523`). The walk is
  adopted when whole (`:425`, fx-src.txt:170), the pending scope starts
  after any end (`:443-451`, fx-src.txt:188-196), and a failed first index
  is loading again once it does (`startLoad` clears the failure, `:389`,
  fx-src.txt:134).
- **The index there is stays searchable.** `readableRevision` returns the
  searchable generation's epoch (`:572-583`, fx-src.txt:274-285);
  `searchableGeneration` is the adopted one, else the first walk's prefix
  (`:647-654`, fx-src.txt:288-295).
- **The list's epoch check.** An indexed list's source is that revision
  (`input_completion_runtime.zig:870`, fx-src.txt:335); a source of another
  epoch is staged `.unavailable` (`:887-888`, fx-src.txt:352-353), which
  an indexed list paints as `Files unavailable. tab to retry; esc to
  dismiss.` (`app_render_runtime.zig:649`, fx-src.txt:490). A path list's
  source is the app's own epoch (`:867`, fx-src.txt:332): never fails.
- **Tab there** retries (`:953`, fx-src.txt:395; `retry`,
  `file_completion_state.zig:230-233`, fx-src.txt:481-482): the index's
  refresh -- into the pending scope while a walk runs, coalesced when one is
  queued already -- and a lookup, which finds the old epoch again.
  **Escape** dismisses the picker whatever its status
  (`app_input_runtime.zig:2724-2729`, fx-src.txt:498-503).
- **When the walk lands** the revision moves, `needsLookup` is true
  (`file_completion_state.zig:199`, fx-src.txt:467), and the list is looked
  up again: its rows.
- **fx's "loading"** for the first episode's skip is `currentState() ==
  .loading` (`main.zig:2126`, fx-src.txt:299): `.ready` once a generation
  is adopted, which a change of scope keeps.

## 2. RED (`red-unit.txt`)

At `74960592` (the base and the cases), the seven run alone: **7 failed.**

| case | base gave | fx (and head) |
|---|---|---|
| a change of roots under an open `@plan` list, the new walk held | `indexing files...`, then the walk's prefix as rows | `Files unavailable` both times, then the rows |
| a change that moves no root (a directory added that is not there), a list open across it and one opened after it | the old rows, twice | `Files unavailable` twice |
| r10-gpt6's trace: the first `@` while A's walk runs | `indexing files...`, 3 walks | `Files unavailable`, 2 |
| ... after A was adopted | `indexing files...`, 3 | `Files unavailable`, 3 |
| ... after A failed | `indexing files...`, 3 | `indexing files...`, 2 |
| Tab in the interim, nothing queued | `indexing files...` (no retry), 3 | `Files unavailable`, 3 (Tab queues one) |
| Escape in the interim | `indexing files...` (the precondition) | the interim, then the list closed and `@plan` kept |

The first-`@` trace is the one r10-gpt6 asked for (NTH 1): "A still running
at the first `@`" gives 2 walks and "A adopted before the first `@`" 3, as
fx does. Base gave 3 for both; for the first that is AT-FOLDERS' residual
(b), and the failed case is residual (a).

## 3. The fix (`c445c1d7`)

`src/tui/file_index.rs`, and `adopt_workspace` and the constructor in `shell.rs`:

- `Files` holds the scope's `epoch`; `change_scope(extra)` moves it and
  sets the roots, on every change the shell sees (`adopt_workspace`, behind
  its `!=`) and once at construction, as fx's launch does. It stops
  nothing and drops nothing.
- A `Walk` carries the epoch it started with; `index_epoch` is set beside
  `index` when a walk is adopted, from that walk.
- `lookup`'s indexed arm: an index, or with none whole the walk's prefix,
  of another epoch is `Lookup::Unavailable`. The `(None, None)` answers are
  unchanged: fx's empty revision carries the index's own epoch, which the
  install's refresh has just set.
- `refresh_index`: a running walk queues the request, as before; the
  `superseded` term goes. `loading()` is `index.is_none() &&
  walk.is_some()`; `ever_whole` goes. With the index never dropped, an
  index held is exactly "a generation adopted" (fx's `currentState`).

**Invisible, and not copied:** fx's index is `.ready` or `.failed` once
its loader has finished, before `joinThreadIfDone` takes it
(`currentState`, `file_index.zig:333-341`, fx-src.txt:86-94); xfx's is at
the poll that takes the walk's end. A tick apart, on no screen. Nothing of
fx's model was left out for its cost: the old roots' walk running to its
end is the costly part, and it is copied, because when the new rows show
depends on it.

## 4. Cases changed

- `a_change_of_roots_during_a_walk_with_a_refresh_queued_walks_the_new_roots_once`
  (round 7): the old walk runs on now, so the case ends it (`Ready`) before
  it waits; still one walk of the new roots.
- `a_walk_a_change_of_roots_stopped_counts_as_whole_once_it_is_over` and
  `a_first_at_before_the_poll_that_ends_a_stopped_walk_coalesces` (round
  10) pinned the stopped walk; they go, for the trace in §2.
- `the_first_episode_after_a_whole_index_refreshes_even_where_the_roots_dropped_it`
  (round 9) is renamed `..._across_a_change_of_roots`; its body and count
  are unchanged.
- `a_workspace_change_that_keeps_the_rows_still_resets_the_bar_and_the_trust`:
  its first precondition flips, as its comment said. After the change the
  list says `Files unavailable`; once the same roots' walk lands the same
  rows are back, still untrusted until a frame lands, and the bar on row 0.
- `file_index`: `a_change_of_added_directories_drops_the_index...` becomes
  `a_change_of_scope_keeps_the_index_and_is_unavailable_until_its_walk_is_whole`
  (the same roots again too), and
  `a_change_of_scope_lets_the_walk_run_on_and_its_prefix_answers_nothing`
  is new.
- `a_path_list_in_the_interim_lists_its_folder` (`9b69f7e4`): `@src/`
  lists its folder while an indexed list says `Files unavailable`.

## 5. GREEN

- **The seven RED cases and every case this unit adds or changes pass at
  the head** (`tests.txt`, at `7b842c2c`, TMPDIR = a scratch directory
  named with every letter a-z in order): the six modules the unit's runs
  filter on, **1337 passed, 0 failed**. Every `@`-query assertion that an added root is in
  scope for names its rows (`@plan` against one root's `plan.md`, or rows
  the case sends itself), so no TMPDIR's letters can change it (AT-FOLDERS
  round 12).
- The gate's full suites (§9) ran with a short TMPDIR.

## 6. Pairs: no scenario can hold a walk

No fxcap verb holds or slows the index's walk (`scripts/fxcap/capture.sh`'s
verb list, lines 13-272): the verbs that wait on the program wait on the
fake gateway's streams (`held`, `release`, :89-97) or on the screen
(`until`, :98-117). A scenario's workspace is a copy of a small committed
tree (`work`, `dir`, :71-79), which both binaries walk inside a tick, so at
any `snap` the interim is over. A tree large enough to be slow cannot be
committed as a fixture, and how long its walk took would decide the pair:
a coin flip, as `sweep.py`'s docstring calls a pair a UI timer decides
(:30-33). So the interim is pinned by
the cases in §2, which assert the band rows themselves; the pairs are the
sweep's (the scenarios that reach the `@` list and `/workspace`).

`sweep.sh` against fx v0.0.16 (sha256 `8e6cda2e...`, checked before and
after each run) at 40x120, `--jobs 2`, both sweeps under one hold of the
suite lock, `--only 'at*' 'p5-input-at*' 'filepicker*' 'workspace*'`: 12
scenarios, 66 pairs (no scenario is named `at*`). Base `v0.1.0-b4cc300`,
head `v0.1.0-274fc43` (`sweep-base.md`, `sweep-head.md`,
`sweep-compare.txt`):

- base 57/66, head 56/66; **moved to passing 0, dropped 1**:
  `workspace-index at-plan`.
- **The drop is fx's run name, not this unit** (`samepair.txt`,
  `samepair.py`, `at-plan.diff`):
  - xfx's captures of `workspace-index` from the two sweeps, cell by cell
    with each run's own directory names masked and the styles kept: **0
    rows differ** in `at-plan` and in `at-all`.
  - fx's two captures of the same snap differ in one cell, row 13 column
    50: the first letter of its run directory's name. The head sweep's fx
    ran in `fxcap.NpCJBx`, and fx bolds that `N` as the `n` of `@plan` on
    the added directory's absolute path; the base sweep's ran in
    `fxcap.RmwRfV`, with no `n` to bold. xfx's two runs (`gyUsKU`,
    `5Jqbc9`) have none either.
  - This is the ledger's `at-ranking` divergence (fx's placement score
    bolds other letters of a long path, `scripts/fxcap/parity-ledger.toml`)
    meeting a run name's random letters. The interim itself is over by
    that snap on both sides (above).

## 7. Mutants (`mutants.txt`)

`mutants.py` on a `git clone --no-local` throwaway, one at a time, the
six modules the unit's runs filter on, under the suite lock: **11 run, 11
killed**, none by a failed build (no `no test ran` line).

| mutant | what it breaks | killed by (first) |
|---|---|---|
| `no-epoch-check` | both epoch arms of `lookup` gone | the change-of-roots and same-roots cases |
| `no-epoch-check-on-the-index` | only the index's | the same |
| `no-epoch-check-on-the-prefix` | only the first walk's prefix | the first `@` while the old roots' walk runs |
| `old-text-kept` | a mismatch says `Loading` (`indexing files...`) | the change-of-roots and same-roots cases |
| `epoch-only-on-root-change` | the epoch moves only when the roots do | the same-roots and keeps-the-rows cases |
| `stop-walk-on-change` | the old model: the change stops the walk | the trace (§2), Tab, round 7's case |
| `drop-index-on-change` | the old model: the change drops the index | the change cases, keeps-the-rows, Escape |
| `adopted-as-the-current-scope` | a walk adopted with the epoch of now | the trace (adopted before `@`), Tab |
| `loading-ignores-the-index` | fx's "loading" = a walk running | round 9's case, the trace |
| `epoch-check-on-a-path-list` | `@src/` checked against the epoch | the path-list case only |
| `no-scope-change-in-adopt` | the shell does not tell `Files` | the change cases |

The first ten ran at `aa83e2da` (`mutants.txt`); the eleventh, written
with the path-list case, at `7b842c2c` (`mutants-pathlist.txt`), whose
tests are the first ten's and that case.

## 8. The parity row, the ledger, AT-FOLDERS' receipts

- `docs/parity.md`, the `@` file picker row (`729d2e82`): the "Narrower
  than fx's, queued as unit AT-SCOPE-INTERIM" paragraph is now the copy --
  the epoch, the walk that runs on, `Files unavailable` until a walk of the
  new scope is adopted, a path list unaffected, Tab and Escape there, the
  rows when the walk lands, and the one tick named in §3. The first-episode
  sentence loses its two gaps. Its other narrower item (keystrokes prepared
  one at a time) keeps its own label. The row's status stays `partial` for
  its other items.
- The ledger: `shell.rs` refs re-pointed in two commits (12, then 6); `refs.txt`, 16
  checked, 0 wrong at `08f3798a` (`refs.py`, AT-FOLDERS', base `b4cc300f`).
- AT-FOLDERS' receipts (`274fc435`): round 7 §3, round 8 §1 and §3, round
  9 §4, round 10 §1 and round 11 §1 said the gap or the residuals would end
  with this unit. Each now says they have, and points here. What each round
  said of its own code is left as it was: it is still true of that code.
- `file_index.rs`: the `set_extra_roots` doc (the gap), the `ever_whole`
  doc, the `loading()` doc and the `poll` comment (both residuals) are
  gone with the code they described.

## 9. Gate (`gate.txt`)

`gates.sh` (AT-FOLDERS', unchanged) at `7b842c2c`, the whole script in
one hold of the suite lock, TMPDIR a short scratch directory: **20 steps, 0
failed**, the tree clean before and after. `cargo test`: 5527 passed, 0
failed; with `fault-injection`: 5564 passed, 0 failed (AT-FOLDERS round
10's 5520 and 5557, `../at-folders/round10/gate.txt:24`, plus the 9 cases
this unit adds, less the 2 it removes).

## 10. Look-ahead

- **Next break on the path, looked for:** the list's refresh and lookup
  after a change still wait for eligibility (unchanged), and a walk landing
  while the list is not eligible looks nothing up until it is
  (`refresh_file_picker`); then the lookup reads the adopted epoch. Enter
  in the interim is spent: `FilePicker::enter` (file_picker.rs:732-738)
  sends the line only for a trusted `no matching files`, as fx's
  `submitFilePickerOnEnter` returns `.inactive` without a selection unless
  `acceptedEmpty` (`input_completion_runtime.zig:1038-1041`,
  `../at-height/fx-src.txt:589-592`). No surface beside the `@` list reads
  `Files`' epoch.
- **What stays apart from fx here:** the tick named in §3, and the parity
  row's other narrower items, untouched.
- **The sweep's one dropped pair** (§6) is a pair a run name's letters
  decide. It will pass or not from sweep to sweep until the ledger's
  `at-ranking` divergence is closed (fx's placement score) or the sweep
  masks run names' styles; neither is this unit's.
