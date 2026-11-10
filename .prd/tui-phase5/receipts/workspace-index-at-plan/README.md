---
title: WORKSPACE-INDEX-AT-PLAN -- main's CI sweep regression, traced to the harness
type: receipt
tags: [xfx, tui, phase5, fxcap, sweep, ci]
---

# WORKSPACE-INDEX-AT-PLAN

main's CI sweep at d2b2ca75 (run 42844785959) reported one regression:
`workspace-index` / `at-plan` at 40x120, row 13, fx blank and xfx
`  <scratch___...>/extra/notes/`. Every claim below names the file in this
folder it rests on. `runs.sh` holds every command, by step.

## 1. No commit caused it

- The same tree passed it on CI. d2b2ca75 is the merge of 208a8dd4 into
  8f921425, and the two trees are one (`6f7fce71`). Run 42826560730 swept
  208a8dd4 and scored `at-plan` normalized-identical; run 42844785959 swept
  d2b2ca75 and scored it unresolved-defect. Runs 42762620303 (3943d667) and
  42804085569 (71d3e02b) also passed it (`ci/runs.txt`).
- The base binary does it too. With the harness CI used for both builds
  (d2b2ca75's), xfx 8f921425 at a CI-like TMPDIR gave CI's exact frame in
  unit 12 -- fx's row 13 blank, xfx's `<scratch` + 46 `_` + `>/extra/notes/`
  -- and the reverse in unit 04 (`red/8f921425-24.txt`,
  `red/8f921425-24-unit12/`, `red/8f921425-24-unit04/`).
- So there is nothing to bisect inside train 12. The per-run random name is
  in the base's harness as well (`run=$(mktemp -d "${TMPDIR:-/tmp}/fxcap.XXXXXX")`,
  `scripts/fxcap/capture.sh:546@8f921425`), whose `run_unit` docstring already
  said each side's path "differs only in capture.sh's own `fxcap.XXXXXX`"
  (`scripts/fxcap/sweep.py:180@8f921425`). The snap is W5's (12dc4a2d,
  2026-10-07). The base passed on CI by the luck of its names.

## 2. Root cause: the two runs of a unit ran in paths of one width and other letters

The happy path, and where it splits:

1. `scripts/fxcap/sweep.py` `run_unit` (`:178@d2b2ca75`) runs a unit's fx and
   xfx captures one after the other in one TMPDIR, `<tmp>/fxsw.<8>/<NNN>`.
2. `scripts/fxcap/capture.sh:681@d2b2ca75` makes each run its own
   `run=$(mktemp -d "$tmpdir/fxcap.XXXXXX")`: **six random letters per run**,
   different for fx and for xfx.
3. The scenario (`scripts/fxcap/scenarios/workspace-index.cap`) makes
   `dir extra`, types `/workspace add ../extra`, then `@plan`.
4. Both binaries index the added directory by its absolute path and match a
   query against the whole lower-cased path:
   - fx: an added root's candidate is displayed absolute
     (`file_index.zig:1098@481104fd`), paths are lower-cased (`:1178-1180`),
     a path holding the query's letters in order has a placement anywhere in
     it (`isSubsequence`, `:1804`; `alignBest(..., 0, 0, ...)`, `:1817`);
   - xfx: `score` falls back to `score_range(path, &scalars, 0, ...)`
     (`src/tui/file_index.rs:568`), case-folded (`fold`, `:527`).
5. So `.../extra/notes/` is listed for `@plan` iff its absolute path holds
   `p`, `l`, `a`, `n` in order. The runner's fixed path,
   `/Users/<runner>/work/_temp/fxsw.<8>/<NNN>/fxcap.<6>/extra/notes/`,
   has a `p` (`_temp`), `a` and `n` (`fxcap`, `extra`, `notes`), and **no `l`**:
   the row is there only if a random name holds an `l` (or `L`). One in
   fxsw's (shared) lists it on both sides; one in a side's own `fxcap.XXXXXX`
   lists it on that side only. The CI log names RUNNER_TEMP
   (`ci/runner-temp.txt`); its 24 bytes make the run directory 55, the
   width of CI's `<scratch...>`.
6. roots.py writes both sides' run directories as one placeholder of the
   same width (LOCAL-PATHS), so no capture showed the two paths differ.

**The two things that disagree:** fx's run directory
`<tmp>/fxsw.<8>/<NNN>/fxcap.<6 letters>` and xfx's
`<tmp>/fxsw.<8>/<NNN>/fxcap.<6 other letters>` -- same bytes long, other
letters. The captures show where: the bold cells of the `notes/` row, read
from the .ansi as bytes of the run path (bytes 50-55 are a run's own six).

- CI's frame, unit 12 (`red/8f921425-24-unit12/`): xfx bolds byte 2 (the `p`
  of `private`), **byte 54, inside xfx's own name**, then the `a` of `extra`
  and the `n` of `notes`. fx's row 13 is blank: fx's name held no `l`.
- The reverse, unit 04 (`red/8f921425-24-unit04/`): fx bolds byte 48 (the `p`
  of `fxcap`), **byte 54, inside fx's own name**, then `a` and `n`. xfx's row
  is blank.

`tests/red.txt` shows the same without the placeholder: a checksum of each
side's working directory, `sum=988592721 63` against `sum=4276430234 63`.

The same cause, a second way: where both sides list the row, which letters
are bold depends on the names too (fx's best placement, xfx's leftmost one;
AT-RANK's ledger cause `at-plan-run-directory` on feat/p5-at-rank names it).
At the gate's own TMPDIR, whose fixed part holds `p`, `l`, `a`
(`private`, `claude`), the row is always there and only the bold letters
flip (`red/d2b2ca75-25.txt`: 9 of 12 differ).

## 3. RED, at d2b2ca75's harness

Sweeps of twelve copies of the scenario (`runs.sh red`, `runs.sh copies`):

| leg | TMPDIR | at-plan differing | how | file |
|---|---|---|---|---|
| xfx d2b2ca75 | 24 B, no `l` | 12 / 12 | row on both, bold differs: the `l` was in fxsw's shared name (xfx's second bold cell is byte 34 of the run path, inside `fxsw.`'s eight: `red/d2b2ca75-24-unit01/`) | `red/d2b2ca75-24.txt` |
| xfx 8f921425 | 24 B, no `l` | 2 / 12 | row on one side only: units 04 (fx), 12 (xfx, CI's frame) | `red/8f921425-24.txt` |
| xfx d2b2ca75 | 49 B, no `l` | 5 / 12 | row on one side only: fx 01, 03, 09; xfx 04, 07 | `red/d2b2ca75-49.txt` |
| xfx d2b2ca75 | 16 B, no `l` | 5 / 12 | row on both, bold differs | `red/d2b2ca75-16.txt` |
| xfx d2b2ca75 | 25 B, gate's | 9 / 12 | row on both, bold differs | `red/d2b2ca75-25.txt` |

`at-all` (an empty query: the index's order, no match) is identical in all
60 units. The harness tests, new in this unit, fail at d2b2ca75: 4 tests, 8
failures (`tests/red.txt`: 3 tests and 7 failures in test_capture.py, 1 and 1
in test_sweep.py).

## 4. The fix (2a721846; outside tests +75/-22: capture.sh, sweep.py, the ledger)

- `scripts/fxcap/capture.sh` `make_run` (`:431-459@2a721846`, used at `:711`): the run
  directory is `fxcap.` and six random digits, never one that exists, owner
  only; or the name `FXCAP_RUN` gives, refused (exit 2, nothing started)
  unless it is `fxcap.` and six digits and not already there.
- `scripts/fxcap/sweep.py`: `run_unit` gives a unit's two runs one
  `FXCAP_RUN` (`:191@2a721846`, `:173`), so fx and xfx run in the same path, byte for
  byte; the sweep's root is `fxsw.` and eight digits (`digit_dir`, `:299`).
- So no random letter is left in any path a run sees: a pair compares two
  binaries on one input, and a base sweep and a head sweep read the same
  letters (those of `--tmp`).
- The ledger: the four capture.sh references re-pointed (+30), and
  `run-directory`'s why. Its rule still reads older captures (`--rescore`).
- Not copied from fx: this is the harness, not the product.

## 5. GREEN, at 2a721846's harness

The same five legs, over four TMPDIR lengths (24 bytes twice, then 49, 16
and 25; `runs.sh green`):

| leg | TMPDIR | at-plan differing | `notes/` row | file |
|---|---|---|---|---|
| xfx 2a721846 | 24 B | 0 / 12 | on neither side | `green/2a721846-24.txt` |
| xfx 8f921425 | 24 B | 0 / 12 | on neither side | `green/8f921425-24.txt` |
| xfx 2a721846 | 49 B | 0 / 12 | on neither side | `green/2a721846-49.txt` |
| xfx 2a721846 | 16 B | 0 / 12 | on neither side | `green/2a721846-16.txt` |
| xfx 2a721846 | 25 B, gate's | 0 / 12 | on both, the same bold letters | `green/2a721846-25.txt` |

Each leg 24 / 24 pairs. The roots lines keep CI's widths (`<scratch>=55` at
24 bytes). A pair for each kind of path is in `green/2a721846-24-unit01/` and
`green/2a721846-25-unit01/`; row 1 of each xfx capture names 2a721846. The
new tests pass (`tests/green.txt`: 3 + 1).

Where the row is listed, its bold letters are now a function of `--tmp`, not
of the run: at the gate's TMPDIR fx's placement and xfx's leftmost one fall
on the same `p`, `l`, `a`, `n`. A TMPDIR whose letters give `@plan` other
placements would still show AT-RANK's difference -- on every run alike, as
the parity defect it is, under the ledger's `at-ranking` cause.

## 6. Look-ahead

- **`at-all`:** an empty query lists the index in its order and matches
  nothing, so no letter of a path decides a row: identical in all 60 RED and
  60 GREEN units.
- **Other scenarios:** only `workspace-index` types an `@` query over an
  added directory; `workspace.cap` adds one but types no `@`. The other `@`
  scenarios search the primary workspace, which both list by relative path
  (`file_index.zig:1098@481104fd`). A sweep of the `workspace*`,
  `filepicker*`, `p5-input-at*` and `p5-input-row-limit*` families at 24
  bytes, the fixed harness, xfx 2a721846 against 8f921425: 73 pairs each, 50
  pass each, **no pair crosses pass/fail** (`look/head-vs-base-24.txt`; each
  build's pairs in `look/2a721846-24.txt`, `look/8f921425-24.txt`). The 23
  failures are the ledger's known rows (folders, ranking, resize, menus).
- **`p5-input-row-limit` `at` / `at-pgdn` on PR #92's sweep (run
  42863297405)** -- xfx `  indexing files...` at row 5, 1.5 s after `@f-` --
  is **another cause**, not this one: no run-path letter is read there.
  - fx starts its file index at launch (`app.startFileIndex()`,
    `src/core/app/app_entry_runtime.zig:334@481104fd`).
  - xfx reads nothing until a picker opens (`Files::new`,
    `src/tui/file_index.rs:923-924`; `begin_episode` at the first `@`,
    `src/tui/shell.rs:9172-9174`).
  - Its walk asks git first (`git ls-files`, `discover`,
    `src/tui/file_index.rs:216-229`) and only then walks.
  - So xfx's index has the 1.5 s after `@f-`, where fx's has had the whole
    7 s since launch. [hypothesis] Under the runner's load (CI's head sweep
    read 11.4), that is too short. Here the pair passed on both builds, and
    `git ls-files` outside a repository took 0.01 s at load 12.
  - The copy of fx, an index started at launch, is AT-LAUNCH-WALK (closed at
    5a83bc4a, in train 14), not this unit.

## 7. Mutants

8 / 8 killed, on a `git clone --no-local` at 2a721846 (`mutants.sh`,
`mutants.txt`): FXCAP_RUN ignored, letters back in the run name, any name
shape accepted, an existing directory reused, the run directory not
owner-only, a name per run instead of per unit, no FXCAP_RUN from the sweep,
mkdtemp's letters back in the sweep's root.

## 8. Gate

At 2a721846, on a `git clone --no-local`, under the suite lock (`gates.sh`,
train 12's): 21 / 22 (`gate/gates.txt`). Step 04, `cargo test`, failed two
`tui::worker` stop-window tests at load 11.6 (4742 passed, 2 failed). Each
then passed three runs out of three alone (`gate/step-04-reruns.txt`). This
unit changes no `src/` file. The two are tests B and E of CATALOG-WAIT-CLOCK
(fix/p5-catalog-wait-clock, not merged: its receipt README, lines 33 and 37),
which names their 300 ms to 1 s wall-clock window as the race and moves them
to tokio's paused clock (3c0a2ab3 to a6ab3ee6).

The receipt's own commits came after the gate; at 267ab4f1 the checks that
read tracked files pass: check-no-secrets, check-no-stubs,
check-no-local-paths, check-xfx-identity, and test_ci_sweep.py
(`gate/quick-checks-267ab4f1.txt`).

## 8b. Round 2 (f718e3c1; the reviews' SHOULDs and NITs)

- **The fx run's late writes.** `cleanup` read no pid and returned as soon as
  kill-server did; a unit's next run reuses the name, so an fx still on its
  way out could write into xfx's HOME or have xfx's mkdir refused. Now it
  reads the pane's pid, and that pid's start time and command, before
  kill-server, waits up to 3 s for it, then SIGKILLs that pid only while it is
  still that process (`reap`, `scripts/fxcap/capture.sh:524-539@f718e3c1`; `cleanup`,
  `:541-553`). **Replaced in round 3 (8c): no SIGKILL.**
- **Pinned** (`tests/red2.txt`: the three of new behaviour fail at cc913145):
  - a binary on its way out is waited for;
  - one that ignores SIGHUP is killed;
  - with no TMPDIR, the first failure ends it.
  Already true at cc913145 and now pinned:
  - fx's sentinel profile, session and config are not in xfx's HOME;
  - under `--jobs 2` two units get two TMPDIRs under one root, and both
    sides' captures outlive the wipe;
  - the named directory is 0700 under umask 022;
  - a random name already there is passed over (an `od` shim).
- **Mutants:** 14 / 14 killed at f718e3c1, round 1's eight and six new:
  - `-m 700` dropped from the named branch;
  - `mkdir -p` on the random branch;
  - no reap;
  - no kill after the wait;
  - every mkdir failure retried;
  - no wipe between sides.
  (`mutants2.sh`, `mutants2.txt`.)
- **The pair again,** with the round-2 harness and xfx f718e3c1, four units
  each: 24 bytes 8 / 8, 49 bytes 8 / 8 (`r2/f718e3c1-24.txt`,
  `r2/f718e3c1-49.txt`).
- **Gate** at f718e3c1, on a clone, under the lock: 22 / 22
  (`gate/gates-f718e3c1.txt`).
- **NITs:**
  - FXCAP_RUN is in capture.sh's usage header;
  - roots.py no longer calls the run name mktemp's;
  - make_run's loop ends on a failure other than "already there";
  - sweep.py's imports are in order.

## 8c. Round 3 (87219db1, edba4758; gpt6's MUST: no SIGKILL)

Round 2's `reap` sent SIGKILL to a bare pid 0.05 s after its last check of an
identity first read at cleanup. Now the pane wait sends no signal. tmux's
own hangup of its pane (`kill-server`) and capture.sh's existing `kill`s of
the fake gateway and mcphttp, servers it started, remain:

- **Recorded at launch.** `own_pane` (`scripts/fxcap/capture.sh:525-534@edba4758`)
  writes the pane's pid and its start time to `$run/pane` right after
  `new-session` (`:1091`) and after each `relaunch`'s `respawn-pane`
  (`:1276`).
  - Its start time only (`ps lstart`, trimmed by `start_of`, `:536-542`), not
    `comm`. The pane runs `sh`, then `env`, then the binary, in one process:
    an exec keeps the start time and changes `comm`.
- **Waited for, never signalled.** `cleanup` (`:564-573`) runs kill-server, which
  hangs the pane up as tmux does. Then `await_pane` (`:549-562`) looks every
  0.05 s, 60 times, while the pid still has the recorded start time.
  - Gone, or another process's: on at once (`pane: ... gone (N looks)`).
  - Still there: a `LIVE-WRITER GUARD` line naming the pid and its start
    time, and exit 4 (an earlier non-zero status is kept).
- **The sweep.** A failed side is a failed unit (`unit_status`, as before).
  The next side of that unit now runs in its own job's TMPDIR under a new
  name, never behind the failed run's writer (`run_unit`,
  `scripts/fxcap/sweep.py:195-201`).
- **Tests.** No test signals a process: each stub logs the HUP and TERM it
  gets, and ends by itself.
  - a binary on its way out is waited for, and gets only tmux's HUP;
  - one that stays fails the run (exit 4, the line), is alive when the run
    ends, and gets only tmux's HUP;
  - a pane already dead is not waited for (0 looks);
  - a pid whose recorded start time no longer matches (as reuse would leave
    it) is not waited for and is alive after the run;
  - a relaunched binary is the one waited for;
  - the sweep records a failed side as a failed unit, and the next side runs
    in another TMPDIR.
  At bf0ed67f, 5 of 10 RunDirectory tests and the sweep test fail
  (`tests/red3.txt`). Round 2's SIGKILL test is deleted.
- **Mutants:** 19 / 19 killed at edba4758 (`mutants3.sh`, `mutants3.txt`):
  the twelve of rounds 1 and 2 that still apply, and seven of round 3:
  - no wait;
  - a wait that never fails the run;
  - the identity read at cleanup instead of at launch;
  - no record after a relaunch;
  - the pid's existence in place of its start time;
  - a 0.1 s wait;
  - the sweep's next side in the failed side's path.
- **The pair again,** with the round-3 harness and xfx edba4758, four units a
  leg: 24 bytes 8 / 8 and 49 bytes 8 / 8. Every one of the 16 runs found its
  pane process gone at the first look (`r3/edba4758-24.txt`,
  `r3/edba4758-49.txt`).
- **Gate** at edba4758, on a clone, under the lock: 22 / 22; the capture
  harness ran 118 tests (`gate/gates-edba4758.txt`).

## 8d. Round 4 (2e85d15b; gpt6's MUST: unknown is never gone)

Round 3 turned a failed look into a pass: a failed `tmux display-message` or
`ps` at launch left an empty record, which cleanup read as nothing to wait
for, and a failed `ps` in the wait read as gone. A live binary plus one
failed look ended the run 0, and the sweep reused its directory.

- **Three states** (`scripts/fxcap/capture.sh:525-595@2e85d15b`):
  - `there`: the pid runs with the recorded start time;
  - `gone`: `kill -0` fails (it sends no signal; it fails only when no
    process of ours has the pid), or the start time differs (the pid was
    reused);
  - `unknown`: any look that failed, among them a `ps` that fails on a pid
    `kill -0` finds. `start_of` (`:547-552`) now fails when `ps` does.
- **The record** (`$run/pane` at 2e85d15b; **round 5 (8e) moved it into
  capture.sh's own shell**, as the binary can write that file):
  - it reads `unknown` from just before the pane is made (`:1120`);
  - `own_pane` (`:530-543`) then writes `pid` with the pid and start time,
    `gone`, or `unknown` with a reason;
  - no record at all means no pane was made, and nothing is waited for.
- **The wait** (`await_pane`, `:574-595`):
  - a `gone` record ends at once;
  - an `unknown` record fails the run (exit 4) with a `LIVE-WRITER GUARD ...
    could not be recorded` line;
  - otherwise it keeps looking while the state is there or unknown, and
    fails at 3 s if the state is not gone.
- **Conservative, not exact.** A pid and a start second are evidence, not an
  identity. Another process started in the same second under the same pid
  reads as `there`: the run waits for it and fails, and sends it nothing.
- **Tests** (`tests/red4.txt`; each stub stays past the wait and ends by
  itself):
  - the pane pid unreadable at launch (a `tmux` shim), exit 4;
  - `ps` failing in the wait (a `ps` shim, turned on once the stub is
    recorded), exit 4 with "in a state no look could read";
  - a live process the run did not start (the test's own), recorded with its
    pid and start time, waited for and the run failed (exit 4) -- a test that
    forged the file, and so went in round 5;
  - the sweep: both TMPDIRs a failed unit used are gone at its end.
  The two fault-injection tests fail at a7dbc9d0 (exit 0). The same-start
  test fails there too, but only because it writes round 4's record format.
- **Mutants:** 6 / 6 killed at 2e85d15b (`mutants4.sh`, `mutants4.txt`):
  - unknown read as gone at launch;
  - unknown read as gone in the wait;
  - the unknown record ignored at cleanup;
  - `kill -0` dropped (every gone pid then reads unknown, and runs fail);
  - round 3's no-wait and no-fail, at their new line.
- **The pair again,** with xfx 2e85d15b, eight units a leg: 24 bytes 16 /
  16, 49 bytes 16 / 16. All 32 runs found their pane gone at the first
  look, and none took the unknown path (`r4/2e85d15b-24.txt`,
  `r4/2e85d15b-49.txt`).
- **Gate** at 2e85d15b, on a clone, under the lock: 22 / 22; the capture
  harness ran 121 tests (`gate/gates-2e85d15b.txt`).
- **No real scenario takes the unknown path.** Every scenario that has a
  `relaunch` also has `keep`, so a binary that exits at once leaves a dead
  pane and a live server: own_pane reads its pid, and `kill -0` reads it
  `gone`. A binary that exits at once without `keep` fails its run anyway:
  `await_up` fails a pane gone before its first frame, and that exit status
  is kept.

## 8e. Round 5 (80486812; gpt6's two MUSTs: the record file)

Round 4's cleanup decided from `$run/pane`: a missing file returned 0, and a
`pid` record without its fields read as gone. The binary in the pane can
write its run directory (round 3's REUSED test rewrote that file), so no
check of the file would make it trustworthy.

- **The record is shell state** (`scripts/fxcap/capture.sh:422-426@80486812`):
  `pane_kind`, `pane_pid`, `pane_start`, `pane_why`, set only by `pane_set`
  (`:528-535`).
  - `unknown` ("the pane was being made") right before `new-session`
    (`:1131`), and ("being remade") before each relaunch's `respawn-pane`
    (`:1321`); then `own_pane` (`:542-555`).
  - An empty kind -- no pane was ever made -- is the one case that returns
    0 without a confirmed gone.
- **Cleanup reads only that state.** `await_pane` (`:587-606`) reads no file.
  The EXIT trap runs in the same shell, and every `own_pane` call is in it:
  the scenario loop reads `< "$file"`, not a pipe.
- **`$run/pane` is a copy for the tests,** never read back. It is written as
  a new file (`rm -f`, then `set -C`), so a link the binary left at its name
  is not followed.
- **Tests** (`tests/red5.txt`; each stub stays past the wait):
  - the binary deletes the copy: exit 4;
  - the binary writes `gone` into it: exit 4;
  - the binary writes `pid` and its pid, without a start time: exit 4;
  - all three exit 0 at 05d80375;
  - a tmux that respawns the pane and then fails: the run says "the pane
    was being remade" (at 05d80375, no such line);
  - `ps` failing at launch: exit 4 (round 4 already did this; now pinned);
  - REUSED is now a `ps` shim giving the recorded pid another start time:
    gone at the first look, exit 0, the stub alive and sent nothing;
  - the same-start-second case cannot be told from a binary that stays, so
    the stays test covers it, as its comment says.
- **Mutants:** 9 / 9 killed at 80486812 (`mutants5.sh`, `mutants5.txt`):
  - cleanup reads the file again (killed by the forged `gone` and forged
    `pid` tests);
  - an empty kind for an unreadable pane pid (that is, read as no pane);
  - the relaunch's reset to unknown dropped;
  - round 4's six at their new lines.
- **The pair again,** with xfx 80486812, eight units a leg: 24 bytes 16 /
  16, 49 bytes 16 / 16, no LIVE-WRITER line (`r5/80486812-24.txt`,
  `r5/80486812-49.txt`).
- **Gate** at 80486812, on a clone, under the lock: 22 / 22; the capture
  harness ran 125 tests (`gate/gates-80486812.txt`).
- **Not covered:** the first process a `relaunch` replaces is not waited
  for. `respawn-pane -k` hangs it up, and the record moves to the new one.
  It writes, if at all, into the same side's own run, while the scenario
  goes on.

## 9. Open

- AT-RANK (feat/p5-at-rank, not merged) adds a ledger cause
  `at-plan-run-directory` owned by SWEEP-TEMPNAME-STYLE. This unit is that
  queue item's fix: when AT-RANK joins, its cause and its owner text need
  re-anchoring or deleting.
- CI no longer exercises the whole-path match: whether `notes/` is listed now
  follows `--tmp`'s letters, so on CI's TMPDIR it is on neither side (on the
  gate's, on both). **AT-PLAN-WHOLE-PATH** (queued) restores it: the letters
  `@plan` needs placed inside the run (the `dir` name, say), once AT-RANK
  lands.
- `p5-input-row-limit`'s lazy index start (section 6) is AT-LAUNCH-WALK's.
