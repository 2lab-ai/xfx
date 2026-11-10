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
| `024a2bb5`, `a8777f54`, `50c4b63e` | this README and the result files; AT-FOLDERS' notes' wording -- **round 1's head** |
| `25c3d090` | round 2: the change-before-any-walk case (AT-LAUNCH-WALK) and the tick's case (§4) |
| `89a3729e` | round 2: the parity row (§8) |
| `02bb222a` | round 2: the ledger's 6 refs past the two cases (`refs.txt`: 16 checked, 0 wrong) |
| `db6fcd9c` | round 2's receipt: this README, `round2-tests.txt`, `mutants-r2.txt`, `mutants.py` (two mutants added), `samepair.*`, `fx_src.sh` and `fx-src.txt` -- **round 2's head** |
| `4fda5350` | round 3: the fix -- the first `@`'s decision takes a finished walk's end first (§3); the tick cases (§4) |
| `25ff67a6` | round 3: the parity row (§8) |
| `2461ec50` | round 3: the ledger's 6 refs past the tick cases |
| `a0bc7d33` | round 3: the mutant that restores the skip -- **round 3's checks, mutants and sweep** |
| (round 3 receipt commit) | this README, `round3-checks.txt`, `mutants-r3.txt`, `sweep-*-r3.*`, `refs.txt`, `fx_src.sh` and `fx-src.txt`; `mutants-r2.txt`'s header |

Every fx line cited here is printed in `fx-src.txt` (`fx_src.sh`, at
`481104fd`); "fx-src.txt:N" is its line. Rounds 2 and 3 appended ranges,
so lines 1-503 are round 1's and 1-512 round 2's.

**Round 2** (reviews of `50c4b63e`: opus APPROVE-WITH-FIXES, gpt6
conditionally ready) changes no production code:
- the interim is fx's only once xfx has walked once this session: fx walks
  at launch and xfx does not, a gap now named as unit AT-LAUNCH-WALK and
  pinned (§3, §4, §8, §10);
- the tick's one effect, named and pinned (§3, §4);
- §6 lists both cells of fx's differing row; §7's table follows the runs.

**Round 3** (reviews of `db6fcd9c`: opus APPROVE-WITH-FIXES, gpt6 MUST
none) copies the tick round 2 named: opus showed it is on screen (a failed
walk: fx `indexing files...`, xfx `Files unavailable` until Tab). The
first `@`'s refresh decision now takes a finished walk's end first, as
fx's `currentState` reads a finished loader before its join (§3); the
tick case flips to fx's result and a failed-walk case is added (§4). §8 is
brought up to date, the premise guards are named as AT-LAUNCH-WALK's
tripwire, and the NITs are done.

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
- **fx walks at launch** (`app_entry_runtime.zig:334`, fx-src.txt:511;
  `startFileIndex` -> `ensureScopeEpoch`, `app_workspace_runtime.zig:
  145-148`, fx-src.txt:45-48). xfx does not (round 2; §3).
- **fx's loop** (round 3): it joins a finished loader at the top of each
  pass (`loopCollectFacts`, `main.zig:2851`, fx-src.txt:573;
  `event_loop.zig:80`, fx-src.txt:519), then waits up to 8 ms for keys
  (`main.zig:174`, fx-src.txt:564) and routes them, then prepares the
  picker and paints (`main.zig:2998`, fx-src.txt:584; `event_loop.zig:119`,
  fx-src.txt:558). A loader that finishes while fx waits for a key is
  unjoined when the key is handled.

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
(b), and the failed case is residual (a). The trace's premise -- A, the
first change's walk, is the session's first and nothing is adopted while
it runs -- is xfx's: in fx the launch walk plays A's part (AT-LAUNCH-WALK,
§3).

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

**The tick, copied (round 3):** fx's `currentState` reads the loading
generation's own state, so a loader that has finished is `.ready` or
`.failed` before the join takes it (`file_index.zig:336-340`,
fx-src.txt:89-93), and the session's first `@` typed then is not loading
and asks for its refresh (`input_completion_runtime.zig:860`,
fx-src.txt:325), queued behind the unjoined loader (`:494-497`) and started
by the join. xfx's walk says it has ended on its channel, which `poll` read
only after the keys were routed, so the first `@` skipped where fx
refreshes: after a failed walk xfx said `Files unavailable` until Tab where
fx says `indexing files...`, and after a whole one fx's second walk
re-reads the tree and xfx's did not (opus r2 MUST 1). Now `take_walk` --
`poll`'s walk drain, factored out -- runs before the decision asks
`loading()` (`Files::refresh_index_for_list`). It is the smallest faithful
form: the channel is xfx's counterpart of fx's atomic state, and draining
it is what the poll does next anyway; a whole walk adopted there shows the
rows fx's unjoined ready generation shows (`searchableGeneration`,
`:647-654`). The second walk starts at once rather than at the next pass's
join; that is a background walk, private until whole.

**Not copied, the loop's frame order** (round 3, the dispatcher's to
decide): fx paints between handling the keys and the next pass's join
(§1), so a failed first walk with a refresh queued behind it -- the first
`@` above, or any refresh queued during a first walk -- is painted once as
`Files unavailable` (its lookup reads `.failed`, `input_completion_runtime.
zig:909`, fx-src.txt:374) and then, after the join starts the queued walk
an 8 ms poll later, as `indexing files...`. xfx's `settle_band` takes the
walk's end and starts the queued walk before it paints, so it shows
`indexing files...` at once. This is the order of the loop, not of the
index, and AT-FOLDERS' failed-walk case
(`a_failed_walk_with_a_refresh_queued_walks_again_and_shows_rows`) has it
too; copying it means holding a queued walk's start until after a frame.

**Not copied, queued as AT-LAUNCH-WALK (round 2, opus MUST):** fx walks
the index at launch (§1); xfx's constructor starts no walk (`shell.rs`,
`Files::new` then `change_scope`). So the interim is fx's only once xfx has
walked once this session. A change of the access before xfx's first walk
-- launch, `/workspace add <dir>`, then `@plan` before the walk ends --
starts the session's first walk at the new epoch, and its prefix is
searched: xfx says `indexing files...` and lists rows as they are
published, where fx, whose launch generation holds the old epoch, says
`Files unavailable` -- while that generation is adopted or still loading
(`searchableGeneration`, `file_index.zig:647-654`); one that failed is no
searchable generation, and fx's list then reads its new walk's prefix, as
xfx's does. The gap is AT-FOLDERS' ("xfx starts no index at
launch") and is older than this unit; copying the launch walk changes
every Shell a test builds, so it is its own unit. Pinned by
`a_change_before_any_walk_lists_the_first_walks_prefix_with_no_launch_walk`,
which that unit flips.

Of the rest of fx's model nothing was left out for its cost: the old
roots' walk running to its end is the costly part, and it is copied,
because when the new rows show depends on it.

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
- Round 2 (`25c3d090`), both pinning xfx's side of a named difference:
  - `a_change_before_any_walk_lists_the_first_walks_prefix_with_no_launch_walk`
    (opus SHOULD): no walk at launch; a change, then `@plan` during its
    walk, says `indexing files...` and then the published row. Its comment
    names AT-LAUNCH-WALK, which flips it.
  - `a_first_at_before_the_poll_that_takes_a_finished_walks_end_skips_its_refresh`
    pinned the tick as xfx's (1 walk). Round 3 copies fx and replaces it
    with two cases on one helper, `first_at_before_the_poll` (the first
    walk's end sent and not yet polled, then the first `@plan`):
    `a_first_at_after_a_walk_ended_whole_but_before_the_poll_refreshes`
    (2 walks, the walk's `plan.md` listed) and
    `a_first_at_after_a_walk_failed_but_before_the_poll_says_indexing`
    (opus r2's: `indexing files...`, 2 walks).
  - Both the change-before-any-walk case and the helper assert no walk at
    launch first. That assert is AT-LAUNCH-WALK's tripwire, and their
    comments say so: that unit must drive the launch walk to its end
    before the change, not only change the rows they expect.
  - The trace helper's doc and the second half of
    `a_change_of_scope_lets_the_walk_run_on_and_its_prefix_answers_nothing`
    name the same gap.

## 5. GREEN

- **The seven RED cases and every case this unit adds or changes pass at
  the head** (`tests.txt`, at `7b842c2c`, TMPDIR = a scratch directory
  named with every letter a-z in order): the six modules the unit's runs
  filter on, **1337 passed, 0 failed**. Every `@`-query assertion that an added root is in
  scope for names its rows (`@plan` against one root's `plan.md`, or rows
  the case sends itself), so no TMPDIR's letters can change it (AT-FOLDERS
  round 12).
- The gate's full suites (§9) ran with a short TMPDIR.
- **Round 2** (`round2-tests.txt`, at `02bb222a`): the same six modules
  under the same a-z TMPDIR, **1339 passed, 0 failed** (round 1's 1337 and
  the two new cases); `cargo test --locked --test parity`, 26 passed, 0
  failed. (Round 2's fmt and clippy runs were not recorded in a file;
  round 3's are.)
- **Round 3** (`round3-checks.txt`, at `a0bc7d33`, every step under the
  a-z TMPDIR and one hold of the suite lock): `cargo fmt --check` and both
  clippy runs exit 0; the six modules, **1340 passed, 0 failed** (round 2's
  1339, less the tick case, plus its two); parity, 26 passed, 0 failed.
  The full gate is the dispatcher's, after the hand-back.

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
  - fx's two captures of the same snap differ in row 13 only, in two
    cells (round 2: `samepair.py` now lists every differing cell). Cell
    index 50 (0-based; column 51), the first letter of the run directory's
    name, is bold in the head sweep's capture; index 63 (column 64), the
    `n` of `notes/`, is bold in the base sweep's. So fx bolds a different `n` for `@plan` on the added
    directory's absolute path: the head sweep's fx ran in `fxcap.NpCJBx`,
    whose `N` comes first; the base sweep's ran in `fxcap.RmwRfV`, which
    has no `n`. xfx's two runs (`gyUsKU`, `5Jqbc9`) have none either.
  - This is the ledger's `at-ranking` divergence (fx's placement score
    bolds other letters of a long path, `scripts/fxcap/parity-ledger.toml`)
    meeting a run name's random letters. The interim itself is over by
    that snap on both sides (above).

**Round 3's sweep** (`sweep-base-r3.md`, `sweep-head-r3.md`,
`sweep-compare-r3.txt`): the same globs, both sides again under the a-z
TMPDIR and one hold of the suite lock, base `v0.1.0-b4cc300`, head
`v0.1.0-a0bc7d3`: base 56/66, head 56/66, **0 moved either way**, the same
ten pairs apart on both sides (`workspace-index at-plan` among them this
time on the base too), and xfx's `workspace-index` snaps 0 rows apart.

## 7. Mutants (`mutants.txt`, `mutants-pathlist.txt`, `mutants-r2.txt`, `mutants-r3.txt`)

`mutants.py` on a `git clone --no-local` throwaway, one at a time, the six
modules the unit's runs filter on, under the suite lock. None was killed
by a failed build (no `no test ran` line in any log). Four runs, each row
below under the run it was in, in that log's order. "Killed by" is a
summary of the killers; each log lists them all.

| run | mutant | what it breaks | killed by |
|---|---|---|---|
| `aa83e2da` (`mutants.txt`, 10 of 10) | `no-epoch-check` | both epoch arms of `lookup` gone | the two `file_index` cases, both change cases, keeps-the-rows, Escape, Tab, the trace (running, adopted) |
| | `no-epoch-check-on-the-index` | only the index's | the same, less the running-walk trace |
| | `no-epoch-check-on-the-prefix` | only the first walk's prefix | the run-on `file_index` case, the running-walk trace |
| | `old-text-kept` | a mismatch says `Loading` (`indexing files...`) | as `no-epoch-check` |
| | `epoch-only-on-root-change` | the epoch moves only when the roots do | the two `file_index` cases, the same-roots and keeps-the-rows cases |
| | `stop-walk-on-change` | the old model: the change stops the walk | the run-on `file_index` case, the trace (running, adopted), Tab, round 7's case |
| | `drop-index-on-change` | the old model: the change drops the index | the keeps-the-index `file_index` case, both change cases, keeps-the-rows, Escape, Tab, round 9's case |
| | `adopted-as-the-current-scope` | a walk adopted with the epoch of now | the run-on `file_index` case, the trace (adopted), Tab |
| | `loading-ignores-the-index` | fx's "loading" = a walk running | round 9's case, the trace (adopted) |
| | `no-scope-change-in-adopt` | the shell does not tell `Files` | both change cases, the trace, Tab, Escape, keeps-the-rows, and 4 older workspace cases |
| `7b842c2c` (`mutants-pathlist.txt`, 1 of 1) | `epoch-check-on-a-path-list` | `@src/` checked against the epoch | the path-list case only |
| `02bb222a` (`mutants-r2.txt`, 13 of 13) | the eleven above, again | -- | the same cases, plus the path-list case (added after the first run) for five of them; neither round-2 case is among them |
| | `walk-at-launch` | the constructor walks (a guess at AT-LAUNCH-WALK) | the change-before-any-walk case, the trace (all three), Tab, and 6 older walk-count cases |
| | `never-loading` | the first episode never skips | the tick's case, round 8's case, the trace (failed) |
| `a0bc7d33` (`mutants-r3.txt`, 14 of 14; a-z TMPDIR) | the thirteen above, again | -- | the same cases, but for the tick's: the round-2 tick case is gone, and the two round-3 cases are among `walk-at-launch`'s killers (its premise guard); `never-loading` keeps round 8's case and the trace (failed), as fx's result is now the never-skip one there |
| | `decision-before-the-walks-end` | the first `@`'s decision before the walk's end is taken: the round-2 skip | both round-3 tick cases, and only they |

The change-before-any-walk case pins xfx's side of AT-LAUNCH-WALK, so the
mutants that break the copy leave it passing; `walk-at-launch` is the
change that moves it.

## 8. The parity row, the ledger, AT-FOLDERS' receipts

- `docs/parity.md`, the `@` file picker row, in three commits:
  - `729d2e82` (round 1): the "Narrower than fx's, queued as unit
    AT-SCOPE-INTERIM" paragraph becomes the copy -- the epoch, the walk
    that runs on, `Files unavailable` until a walk of the new scope is
    adopted, a path list unaffected, Tab and Escape there, the rows when
    the walk lands. The first-episode sentence loses its two gaps.
  - `89a3729e` (round 2): the copy holds once xfx has walked once this
    session; a change before then is named as unit **AT-LAUNCH-WALK** --
    fx walks at launch (`app_entry_runtime.zig:334`) and xfx does not, so
    there xfx says `indexing files...` and lists the first walk's prefix
    where fx says `Files unavailable` -- with the first-episode skip's
    reliance on it. `searchableGeneration` is cited at the pin's
    `:647-654` in both places.
  - `25ff67a6` (round 3): round 2's "not copied, a tick apart" sentence
    goes; the row says the first `@`'s decision reads a finished walk as
    fx's `currentState` does, and names the loop's frame order (§3).
  - Its other narrower item (keystrokes prepared one at a time) keeps its
    own label; the row's status stays `partial` for its other items.
- The ledger: `shell.rs` refs re-pointed in four commits -- `aa83e2da`
  (12), `08f3798a` (6), `02bb222a` (6), `2461ec50` (6). `refs.txt` is the
  run after the last commit of round 3: 16 checked, 0 wrong (`refs.py`,
  AT-FOLDERS', base `b4cc300f`).
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
- **What stays apart from fx here:** AT-LAUNCH-WALK (§3), pinned as
  xfx's, and the loop's frame order (§3), named for the dispatcher; the
  parity row's other narrower items, untouched. AT-LAUNCH-WALK flips
  `a_change_before_any_walk_lists_the_first_walks_prefix_with_no_launch_walk`;
  its launch walk would take A's part in the trace (§2) and the first
  walk's part in the round-3 tick cases, whose guard trips first.
- **The sweep's `workspace-index at-plan`** (§6) is a pair a run name's
  letters decide: it passed on round 1's base sweep and on neither side of
  round 3's. It will pass or not from sweep to sweep until the ledger's
  `at-ranking` divergence is closed (fx's placement score) or the sweep
  masks run names' styles; neither is this unit's.
