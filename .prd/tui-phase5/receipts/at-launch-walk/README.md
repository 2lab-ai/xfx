---
title: 0.0.16 delta -- AT-LAUNCH-WALK (fx walks the @ index at launch)
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, input, file-picker]
---

# AT-LAUNCH-WALK

Base: `f4aa7dcc` (AT-SCOPE-INTERIM, closed, not yet on `main`). fx walks
the `@` index at launch; xfx walked it first at the first indexed `@`. This
unit copies fx's launch walk -- where it starts, what it walks, the epoch it
carries, none for an empty root -- and flips the cases AT-SCOPE-INTERIM left
as this unit's tripwire (its README §3, §4, §10).

| commit | what |
|---|---|
| `23454d7f` | RED: the seam (`Files::start_index` and `Shell::start_file_index` doing nothing; the launch's steps after `Shell::new` moved into `tui::open`, unchanged) and 13 cases (`red-unit.txt`) |
| `67665001` | the fix: `Files::start_index` is fx's `ensureScopeEpoch`; `tui::open` calls it last |
| `ef225f74` | `start_index`'s doc: each bare line number after its own fx file (`fxcites.py`) |
| `0d961b43` | `docs/parity.md`, the `@` file picker row (§8) |
| `284dc22c` | AT-SCOPE-INTERIM's README: each AT-LAUNCH-WALK line says it has ended (§8) |
| `7b16be2b` | the ledger's 12 `shell.rs` refs re-pointed (`refs.txt`) -- **the mutants' clone and the sweep's build** (`v0.1.0-7b16be2`) |
| `286e1153` | this folder's runners, fx quotes, RED run, mutants and refs -- **the checks' and the gate's commit** |
| (receipt) | this README, `checks.txt`, `gate.txt`, `fxcites.txt` and the sweep's files |

Every fx line cited here is printed in `fx-src.txt` (`fx_src.sh`, at
`481104fd`); "fx-src.txt:N" is its line. `fxcites.py` checks every fx
citation in this README, in the parity row's changed text and in the source
lines this unit added against `fx_src.sh`'s ranges (`fxcites.txt`).

## 1. fx's rule

- **When.** `runInteractiveWithDeps` builds the app -- `App.init`
  (`app_entry_runtime.zig:304`, fx-src.txt:36), which takes the workspace
  root and its access at bootstrap (`app_bootstrap_runtime.zig:438-441`,
  fx-src.txt:202), applies the launch's directories (`main.zig:682-686`,
  fx-src.txt:146) and opens a requested resume (`main.zig:696-713`,
  fx-src.txt:161) -- then starts the file index
  (`app_entry_runtime.zig:334`, fx-src.txt:66), the worker thread, and only
  then `app.run()`, whose loop paints the first frame (`app_entry_runtime.zig:343`,
  fx-src.txt:75). So: after the workspace load and the resume, before the
  first frame. `App.startFileIndex` "must be called after init() returns"
  (`main.zig:2073-2078`, fx-src.txt:185). fx's own test pins that order:
  `init`, `auto-upgrade`, `file-index`, `worker-thread`, `model-cache`,
  `run` (`app_entry_runtime.zig:964-973`, fx-src.txt:91).
- **Native launches only.** The call sits under `comptime !cooperative`;
  every native entry passes `false` (`app_entry_runtime.zig:155`, `:209`; fx-src.txt:15,
  23); the single-threaded host's `runInteractiveCooperative` passes
  `true` (`app_entry_runtime.zig:216`, fx-src.txt:30), and fx's test of it has no
  `file-index` (`app_entry_runtime.zig:975-984`, fx-src.txt:103).
- **Fresh or resumed.** `app_entry_runtime.zig:334` runs whatever `resume_requested`
  says; the flag gates only the warmup (`app_entry_runtime.zig:330`, fx-src.txt:62), and
  fx's test of a resumed launch has `file-index` after it
  (`app_entry_runtime.zig:1316-1325`, fx-src.txt:115).
  A resume or a fork inside the session starts no walk:
  `resumeSelectedSession` and `forkBeforeSelectedPrompt` switch in place
  through `installResumedV2Session` and call nothing of the index
  (`app_session_runtime.zig:2052-2068`, `:2098-2126`; fx-src.txt:601,
  632),
  and `startFileIndex` has no other caller.
- **What it walks.** `startFileIndex` returns for an empty workspace root
  (`app_workspace_runtime.zig:146`, fx-src.txt:267), else calls
  `ensureScopeEpoch(scope(app), app.workspace.scope_epoch)`
  (`app_workspace_runtime.zig:147`): the
  access over the workspace root (`scope`, `app_workspace_runtime.zig:
  26-28`, fx-src.txt:214), whose
  roots are the primary and each active added directory, once
  (`activeRootsAlloc`, `file_index.zig:1041-1061`, fx-src.txt:491).
- **Only from idle.** `ensureScopeEpoch` returns unless `currentState()` is
  `.idle` -- nothing adopted, nothing loading, no failure
  (`file_index.zig:357`, fx-src.txt:293; `currentState`,
  `file_index.zig:333-341`,
  fx-src.txt:276) -- and for an empty primary (`file_index.zig:358`,
  fx-src.txt:294).
- **Which epoch.** It sets the index's epoch to the app's at that moment
  (`file_index.zig:361`, fx-src.txt:297), and `startLoad` gives the walk
  that epoch (`file_index.zig:386`, fx-src.txt:322), which nothing changes after: a change of the
  access moves the app's epoch (`invalidateScope`,
  `app_workspace_runtime.zig:98-104`, fx-src.txt:219) and queues the new
  scope behind the running walk (`refreshOwnedRoots`,
  `file_index.zig:494-497`, fx-src.txt:434). So the launch generation,
  loading or adopted, is searchable with the launch's epoch
  (`searchableGeneration`, `file_index.zig:647-654`, fx-src.txt:481;
  `readableRevision`, `file_index.zig:572-583`, fx-src.txt:467), and an indexed list of another epoch is
  staged unavailable (`input_completion_runtime.zig:887-888`,
  fx-src.txt:554). One that failed is no searchable generation
  (`file_index.zig:652`, fx-src.txt:486).
- **The cache** (`file_index.zig:371-378`, fx-src.txt:307): fx's first
  generation of a scope may paint from an on-disk cache before a real scan.
  xfx keeps no index cache, an older narrower item of the row ("no on-disk
  index cache"); not this unit's.

## 2. The copy, and how tests stay hermetic

- `Files::start_index` (`src/tui/file_index.rs`) is `ensureScopeEpoch`:
  nothing unless there is no index, no walk and no failure (xfx's `failed`
  is fx's `initial_failed`), nothing for an empty root, else `start_walk`,
  which gives the walk the epoch as it stands (`Walk::epoch`,
  AT-SCOPE-INTERIM's). `Shell::start_file_index` is `startFileIndex`, a
  pass-through.
- `tui::open` (`src/tui/mod.rs`) is the launch's steps after `Shell::new`
  -- the resumed document or the welcome, the config and history notices,
  moved there unchanged -- ending with `shell.start_file_index()`. `hold`
  calls it, then `event_loop::run`, whose first pass paints. It is the
  method's only production caller; the in-session resume and fork
  (`Shell::hydrate`) call nothing of it, as fx's do not.
- The scope is the one the constructor set (`Files::change_scope` over
  `index_roots`, `shell.rs`): the workspace and the active added
  directories, with the epoch that change gave it. xfx's root is canonical
  (`RuntimeConfig::load_with`), so the empty-root guard is fx's rule kept,
  reached only by a `Files` over an empty path.
- **Hermetic tests: the walk is not the constructor's.** That is fx's own
  split: `App.init` walks nothing and the entry does (fx-src.txt:66, 185),
  and fx's entry tests use an app whose `startFileIndex` records an event
  and walks nothing (`app_entry_runtime.zig:867-869`, fx-src.txt:78). So
  `Shell::new` still walks nothing, and a case that wants fx's launch calls
  `start_file_index` -- the method the launch calls -- over its own temp
  workspace. Not a test-only constructor, a fake walker or a fixture root,
  because: no `#[cfg(test)]` branch enters the product, so a case runs the
  code a user runs; the shell suite builds thousands of shells and every
  walk starts `git ls-files` (`discover`), so a walk per shell would be
  thousands of process starts per run on a shared machine; and no existing
  case's walk count moves. The launch's own call is pinned by
  `the_launch_starts_the_index_walk_fresh_or_resumed`, which runs `open`
  over a temp workspace with and without a resumed session (§7: removing
  the call fails it, and only it).
- No case walks a directory outside its own temp root: each launch walk in
  a case is over `seeded`/`in_ws2`'s temp tree (`git = None` in the
  `file_index` cases) or the mod case's temp workspace. No assertion reads
  TMPDIR's letters or length: the rows asserted are relative
  (`src/deep/sched.rs`, `notes/plan.md`) or the case's own added
  directory's `plan.md`, and the unit run is under an a-z TMPDIR (§5).
- A case drives the launch walk with `held` (`file_index::tests`): it takes
  over the receiver of the walk in flight -- the one the code started, with
  the epoch the code gave it -- and fails the case when there is none.
  (`driven` installs a walk of its own, with which a case would pass with
  no launch walk at all.)

## 3. RED (`red-unit.txt`)

At `23454d7f` (the seam and the cases), the 13 run alone: **13 failed.**

| case | at the seam | head (fx) |
|---|---|---|
| `the_launch_walks_a_root_once_and_an_empty_root_never` (`file_index`) | 0 walks for a root | 1; none again while it runs, over its index, after a failure; none for an empty root |
| `the_launch_walk_keeps_the_launch_epoch_through_a_change` (`file_index`) | no walk in flight | its prefix, then its index, `Unavailable` after a change; rows once the new scope's walk is whole |
| `the_launch_starts_the_index_walk_fresh_or_resumed` (`tui::tests`) | `(0, 0)` | `(0, 1)`, fresh and resumed |
| `a_fresh_sessions_first_at_after_the_launch_walk_is_whole_shows_its_rows_at_once` | `Loading`, no rows | `Rows`, `src/deep/sched.rs`; 2 walks |
| `a_change_before_the_launch_walk_is_whole_says_files_unavailable_until_the_new_scopes_walk_is` | no walk in flight | `Files unavailable` at the `@`, after the launch walk's prefix and after its adoption; then the change's `plan.md`; 2 walks |
| `a_change_after_the_launch_walk_failed_lists_the_new_walks_prefix` | no walk in flight | `indexing files...`, then the change's `plan.md` |
| `a_first_at_after_the_launch_walk_ended_whole_but_before_the_poll_refreshes` | no walk in flight | 2 walks, the launch walk's `notes/plan.md` |
| `a_first_at_after_the_launch_walk_failed_but_before_the_poll_says_indexing` | no walk in flight | `indexing files...`, 2 walks, never `Files unavailable` |
| `a_first_at_after_a_change_whose_walk_failed_before_the_poll_says_files_unavailable` | no walk in flight | `Files unavailable` at the `@` and after the next pass; 3 walks |
| the trace (`the_first_at_while_...`, `..._was_adopted_...`, `..._failed_...`) and `tab_in_the_interim_...` | no walk in flight | as before: `Files unavailable`/2, `Files unavailable`/3, `indexing files...`/2; Tab 3 -- the launch walk is A |

The base's own row for the second shell case is the case it flips,
`a_change_before_any_walk_lists_the_first_walks_prefix_with_no_launch_walk`,
which passed in AT-SCOPE-INTERIM's round 3 (`../at-scope-interim/
round3-checks.txt`, its line 14; round 4 changed no line of it):
`indexing files...`, then the change's walk's prefix as rows.

## 4. The tripwires flipped, and the trace's premise

- **The change-before-any-walk case** becomes
  `a_change_before_the_launch_walk_is_whole_says_files_unavailable_until_the_new_scopes_walk_is`:
  it starts the launch walk and holds it before the change, as its comment
  asked, and asserts fx's result (§3). What it used to assert --
  `indexing files...`, then the prefix -- is fx's only after a failed
  launch walk, which `a_change_after_the_launch_walk_failed_lists_the_new_walks_prefix`
  now pins.
- **The tick helper** (`first_at_before_the_poll`) asserted no walk at
  launch, and its comment asked this unit to drive the launch walk to its
  end before the change. It takes a `change` flag:
  - with it, the launch walk is held and ended whole, then the change, whose
    walk ends before the poll. Under it fx's adopted launch generation
    answers with the old epoch, so for a whole change-walk fx paints one
    frame of `Files unavailable` before its join adopts it, where xfx paints
    the rows (the loop-order decision, §10), and the case
    that differs from the base is the failed one:
    `a_first_at_after_a_change_whose_walk_failed_before_the_poll_says_files_unavailable`
    (`Files unavailable` at the `@` and after; 3 walks), where the base's
    helper said `indexing files...`;
  - without it, the walk that ends before the poll is the launch walk
    itself -- the session's first walk, the part the change's walk played
    (AT-SCOPE-INTERIM's README §10 said so). That is where the tick is
    still visible, so the two round-3 cases move there, renamed
    `a_first_at_after_the_launch_walk_..._refreshes` and
    `..._says_indexing`, their results unchanged. These are the cases
    where the screen tells the skip apart: with the launch walk adopted
    first (`change`), `loading()` is false whether or not the decision
    takes the walk's end, and the two differ only in when the walks start
    (the walk count at the `@`).
  - A walk the `@` starts is held too, so the row after the next pass is the
    state's, not a race with a real walk of the seeded tree.
- **The trace** (`first_at_after_two_changes`) and Tab's first half: A, the
  walk both changes find running, is now the launch walk (`held` over
  `start_file_index`'s walk) rather than the first change's. The results
  are the same as AT-SCOPE-INTERIM's (2, 3, 2; Tab 3), as fx's version of
  the trace gives them. The helper's doc says the premise is fx's now.
- `file_index`'s `a_change_of_scope_lets_the_walk_run_on_and_its_prefix_answers_nothing`
  keeps its second half (a change before any walk); its comment now says
  that in a session this is the source after a failed launch walk.

## 5. GREEN (`checks.txt`)

At the gate's commit, every step under the a-z TMPDIR and one hold of the
suite lock (`checks.sh`, at `286e1153`, `src` as at `7b16be2b`): the 13
cases pass; the seven modules the unit's runs filter on --
AT-SCOPE-INTERIM's six and `tui::tests` -- **1357 passed, 0 failed**;
parity, 26 passed, 0 failed. The full suites, fmt and clippy are the
gate's (§9).

## 6. Pairs: no scenario can hold the walk

No fxcap verb holds or slows the index's walk (`scripts/fxcap/capture.sh`'s
verb list, lines 13-272): the verbs that wait on the program wait on the
fake gateway's streams (`held`, `release`, lines 89-97) or on the screen
(`until`, lines 98-114), and a scenario's workspace is a copy of a small
committed tree (`work`, `dir`, lines 71-79) that both binaries walk well
inside the scenario's first `wait`. So by any `snap` the launch walk is whole on
both sides, and on the base the first `@`'s own walk has landed too: the
first `@` of a fresh session shows rows on all three binaries, and only a
snap timed inside the first walk's few milliseconds would differ -- a pair
a timer decides (`sweep.py`'s docstring). No pair is added. The fresh
session's first `@` with rows is already swept: `filepicker at` (`@` two
seconds after launch, snapped 1.2 s later) and `p5-input-at-height at`
(`@f-` after four), normalized-identical on both sides of the sweep
below (`sweep-base.md`, `sweep-head.md`). The launch walk's rule is pinned
by the cases in §3, which assert the band rows themselves.

**The sweep** (`sweeps.sh`; `sweep-base.md`, `sweep-head.md`,
`sweep-compare.txt`): fx v0.0.16 (sha256 `8e6cda2e...`, checked before and
after each run by `sweep.py`) against the base's xfx (`v0.1.0-f4aa7dc`) and
the head's (`v0.1.0-7b16be2`), each banner read from its sweep's header, at
40x120, `--jobs 2`, `--only 'at*' 'p5-input-at*' 'filepicker*' 'workspace*'
'p5-lifecycle*'`: 42 scenarios, 135 pairs (no scenario is named `at*`),
both sweeps inside one hold of the suite lock under the a-z TMPDIR. Base
84/135, head 84/135; **0 moved to passing, 0 dropped, 0 other verdict
changes**, `p5-lifecycle*` (the resumed launches among them) included. The
51 pairs that do not pass -- 50 unresolved, one an approved cascade -- are
the same set on both sides (`sweep-compare.txt`'s last lines), the ten
AT-SCOPE-INTERIM's round-3 sweep lists among them (`workspace-index
at-plan`, the run-name letters, its README §6).

## 7. Mutants (`mutants.txt`)

`mutants.py` on a `git clone --no-local` throwaway at `7b16be2b`, one at a
time, the seven modules, under the a-z TMPDIR and one hold of the suite
lock, 900 s each. None failed to build. The clone was clean after (0
changes). **8 run, 7 killed.** "Killed by" summarises; the log lists every
killer.

| mutant | what it breaks | killed by |
|---|---|---|
| `no-launch-walk` | `tui::open` without `start_file_index` | `the_launch_starts_the_index_walk_fresh_or_resumed`, and only it |
| `no-launch-walk-in-files` | `Files::start_index` walks nothing | 13: every case of §3 |
| `launch-walk-current-epoch` | a change retags the first index's walk with the epoch of now (fx's keeps the launch's) | the run-on and launch-epoch `file_index` cases, the change-before case, the trace (running, adopted), Tab |
| `launch-walk-on-an-empty-root` | the empty-root guard gone | `the_launch_walks_a_root_once_and_an_empty_root_never` |
| `launch-walk-not-only-from-idle` | the idle guard gone | the same |
| `walk-in-the-constructor` | `Shell::new` walks (the cost this unit avoided) | 11: six older walk-count cases this unit did not touch, the trace and Tab, and the `open` case (a walk at construction) |
| `tripwire-cases-restored` | the base's tripwire cases put back for the head's | **SURVIVED** (below) |
| `flipped-cases-without-the-launch-walk` | every `shell.start_file_index()` in the shell cases removed | 10: every shell case that calls it |

`tripwire-cases-restored` survives by design. Its guards assert no walk
after `Shell::new` (`walks_for_test() == 0`), which still holds: the walk
moved to `open`, as fx's is outside `App.init`. Put back, the old cases pass
as a state no session reaches -- a shell nobody launched -- and assert what
xfx did there. What the brief's mutant asks -- would the flipped cases pass
without the launch walk? -- is `flipped-cases-without-the-launch-walk`,
killed: each of them fails at `held` (no walk in flight) or on its rows.

## 8. The parity row, the ledger, AT-SCOPE-INTERIM's README

- `docs/parity.md`, the `@` file picker row (`0d961b43`): the
  "Narrower than fx's, queued as unit AT-LAUNCH-WALK" paragraph becomes the
  copy -- fx's launch walk, fresh or resumed, its scope and epoch, only from
  idle, none for an empty root, none for a resume or fork inside the
  session, and what a change before it is whole shows -- with the gap in the
  past tense and this receipt. The interim's sentence loses "once xfx has
  walked once this session", and the first-episode sentence names the walk
  it meets. The loop-order decision AT-SCOPE-INTERIM recorded names its
  second instance, which a launch walk makes reachable (§10). The row's
  status stays `partial` for its other narrower items.
- The ledger (`7b16be2b`): 12 `shell.rs` refs re-pointed past the new
  method. `refs.py` is AT-SCOPE-INTERIM's with `mod.rs` among the files
  (this unit changes it) and a range `file.rs:N-M` checked at both ends;
  `refs.txt`: **17 checked, 0 wrong** (the 16 of AT-SCOPE-INTERIM's run and
  the ledger's one `mod.rs:127-133`, which this unit's insertion below it
  does not move).
- AT-SCOPE-INTERIM's README (`284dc22c`): each line that named
  AT-LAUNCH-WALK as a gap or a case as its tripwire -- round 2's summary,
  §1, §2's trace premise, §3, §4 (three places), §7, §8, §10 -- says it has
  ended, past tense, and points here; what each round said of its own code
  is left as it was.
- Source comments that said xfx starts no index at launch
  (`Files::refresh_index_for_list`, `Files::new`, the run-on case's second
  half, the trace helper) say what it does now.

## 9. Gate (`gate.txt`)

`gates.sh` (AT-SCOPE-INTERIM's, unchanged but for its first comment line)
at `286e1153`, the whole script in one hold of the suite lock, TMPDIR a
short scratch directory: **20 steps, 0 failed**, the tree clean before and
after. `cargo test`: **5536 passed, 0 failed**; with `fault-injection`:
**5573 passed, 0 failed** (`gate.txt`, summed over each run's test
binaries). That is AT-SCOPE-INTERIM round 1's 5527 and 5564
(`../at-scope-interim/gate.txt`), its rounds 2 and 3's three cases net
(its README §5: 1337, 1339, 1340), and this unit's six net: nine added
(two `file_index`, one `tui::tests`, six shell) and three replaced (the
change-before-any-walk case and the two tick cases, which became their
launch-walk forms).

## 10. Look-ahead

- **Next break on the path, looked for.** The launch walk lands while no
  list is open: `settle_files` polls it and `refresh_file_picker` returns
  with no list, so no frame is asked for (`shell.rs`, `refresh_file_picker`).
  The first `@` then reads it through AT-SCOPE-INTERIM's model unchanged
  (`Files::refresh_index_for_list`, `lookup`). Keys the launch probe read
  are routed at the loop's start (`event_loop.rs`, `session`:
  `route_read(launch.deferred)`), after `open` has started the walk, so
  they meet it as any later key does. No other surface reads `Files`.
- **The call is pinned at `open`, not at `hold`'s call of `open`.** A change
  that built the shell some other way and skipped `open` would pass the
  unit suite; only a pty session could see it, and no pty observable holds
  the walk (§6). `open` is `hold`'s one path to the loop.
- **The loop-order decision, a second instance** (row §8): fx can paint one
  frame of `Files unavailable` when a new scope's walk ends whole during
  its key wait behind an adopted launch index of the old epoch; xfx takes
  the end at the first `@`'s decision or the poll and paints the rows
  (the tick helper with `change` and a whole walk gives xfx's). Before
  this unit it needed an index walked whole before the change -- an `@`
  first; with the launch walk a change after launch reaches it.
  AT-SCOPE-INTERIM recorded the decision for the user's veto; it is named,
  not copied.
- **Cost:** every launch now walks the workspace on a worker thread -- the
  trusted `git ls-files`, or a walk of at most 100,000 entries -- whether
  or not an `@` is typed, as fx's does. fx may paint its first generation
  from an on-disk cache; xfx has none (the row's older narrower item).
- **Unit cases that build a shell without `start_file_index`** model a shell
  nobody launched. The ones that claim fx's result for "the session's first
  walk" now drive the launch walk (§4); the older picker cases that start
  the first walk at an `@` keep testing `Files`' rules, not the launch.

## 11. Open questions

None that needs a user decision. The loop-order instance (§10) extends a
decision AT-SCOPE-INTERIM already recorded for the user's veto.
