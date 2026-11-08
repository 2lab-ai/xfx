---
title: 0.0.16 delta -- P5 train 8 receipt (GRID-ROW-COST, FLAKE-MCPMODE, FLAKE-MCPMODE-F into one branch)
type: receipt
tags: [xfx, phase5, fx-0.0.16, train]
---

# P5 train 8: three closed units, one branch

Branch `train/p5-1109a`, from `origin/main` `58c92ba2` (#84, train 7), as
the brief set it; `origin/main` had not moved. Each unit was closed by its
own two-engine review and dispatcher gate; this train combines them. The
program goal (the user's words): «최신 fx랑 비교해서 ui쪽으로 모든 기능
구현하고 기본적으로 최신 fx ui를 그대로 구현해줘» (fx v0.0.16, pin
`481104fd`).

Oracle: fx v0.0.16 (`481104fd`), executable sha256
`8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked
by the sweep before and after every run, `FX_AUTO_UPGRADE=0`.

**At a glance.**
- **Merges:** three `--no-ff` merges, in the brief's order. Merge 1
  (GRID-ROW-COST) is git's own, but **a build conflict**: its session test
  applied `UiEvent::TurnStarted` as a unit variant, which STOP-NEXT (train 7)
  gave a submission; fixed in the next commit (`866ba6c3`, `start_turn()`),
  not in the merge (the brief: no history rewrite). Merge 2 (FLAKE-MCPMODE):
  one hunk in `scripts/fxcap/capture.sh`, as the brief expected -- both
  sides added a function after `since()` (LAUNCH-SETTLE's `await_up`,
  FLAKE-MCPMODE's `put_in_home`); both kept, and the merged file keeps the
  launch wait and every atomic writer. Merge 3 (FLAKE-MCPMODE-F `6a9a6a9d`,
  taken when the dispatcher named it) is git's own; it brings the redaction
  of the four local paths merge 2's receipt scripts held: `git diff
  origin/main..HEAD`'s added lines hold none.
- **Token check:** all three merges OK (train 7's script, byte for byte;
  neither unit touches `docs/parity.md` or the ledger); negative control
  **31 of 31**. **References:** 21 the merges moved (10 in the ledger, 11 in
  comments), re-pointed (`d57beb36`): **21 of 21 the same code**; 0 moved at
  the head; 12 stale on `main` already, left for REF-LINT.
- **Join check 1 (GRID-ROW-COST x STOP-NEXT-2):** GRID-ROW-COST's per-tick
  session test and its equality tests pass at the head; the new case -- a
  stop over an open group of long rows holds late text, a read and a long
  call's result, then writes them three ways -- **passes, with
  `stale_laid_rows` empty after every tick**. **Neither side alone builds
  it**; with the other side's half taken out it fails on GRID-ROW-COST alone
  (nothing held) and passes on `main`. 5 of 5 mutants killed, 4 by the join
  test.
- **Join check 2 (render cost):** head ÷ GRID-ROW-COST alone **0.87 to
  1.13** on every step of both probes, round by round: **none past 1.2x**;
  the tick with the long rows left in place 2.9-3.1 ms (one long call) and
  8.5-9.1 (three), against `main`'s 261-1205.
- **Join check 3 (FLAKE-MCPMODE x LAUNCH-SETTLE):** `test-capture` five times
  at the head, **5 of 5 OK (76 tests)**, and three more runs (72, 72, 76):
  no failure.
- **Join check 4 (network):** lib 4543 and tui 181 tests through a loopback
  logger: 2 CONNECTs, both `w.example`; **none to the Gateway**.
- **Gate** at `387a0104`, one hold: **20 of 20 steps** (`cargo test` 5460,
  fault injection 5497, each +14 on train 7's at `main`'s code; smoke-tui
  1168 checks; `test-capture` 72 OK). Merge 3's steps at `c4bb4c7f`: **7 of
  7** (`test-capture` 76 OK).
- **Sweep**, 118 scenarios: 381 of 529 pass, 0 failed, 0 timeout. Against
  `main` **381 -> 381, 0 moved**; against GRID-ROW-COST's own **228 -> 228, 0
  moved**; against FLAKE-MCPMODE-F's own **164 -> 174, 10 up (trains 6's and
  7's product changes, which `main` has), 0 down**; merge 3's `mcphttp`
  scenarios again at `c4bb4c7f`: 18 -> 18, 0 moved. **No pair dropped.**

**Head:** the branch's tip, this receipt's last commit (`train/p5-1109a`; no
push, no PR); its code is `c4bb4c7f`'s.

## Units and merges

Each is `git merge --no-ff`, in the brief's order; each message names the
unit and its head.

| # | unit | branch @ tip | merge | base | conflicts |
|---|---|---|---|---|---|
| 1 | GRID-ROW-COST | `feat/p5-grid-row-cost` @ `a3f90ef5` | `9a465471` | `fdc5ac0b` (train 6) | none textual; **a build conflict** in `src/tui/event_loop.rs`'s tests, fixed in `866ba6c3` |
| 2 | FLAKE-MCPMODE | `fix/p5-flake-mcpmode` @ `2971aeff` | `056b1b01` | `690aa505` (train 5) | `scripts/fxcap/capture.sh`, one hunk (both added a function after `since()`) |
| 3 | FLAKE-MCPMODE-F | `fix/p5-flake-mcpmode-f` @ `6a9a6a9d` (stacked on `2971aeff`) | `c4bb4c7f` | `2971aeff` | none |

Every base is git's merge base. After the merges, in order:

| commit | what |
|---|---|
| `866ba6c3` | merge 1's build conflict: GRID-ROW-COST's session test starts its turn with `start_turn()`; test only (below) |
| `3bf99857` | the join test (join check 1); test only |
| `d57beb36` | the `path:line` references merges 1 and 2 moved, re-pointed; numbers only (below) |
| `387a0104` | this receipt's scripts; the gate and the head sweep ran at it (its code is `d57beb36`'s) |
| `c4bb4c7f` | merge 3, taken when the dispatcher named its closing head; the gate steps it touches, the join runs, the mutants and the probe ran at it |
| the next ones | this receipt |

`git show --remerge-diff` of each merge is `remerge-1.txt` .. `remerge-3.txt`.
Merges 1 and 3 have an empty body: git's own result, unchanged.

**Merge 1 was committed as git made it, and its build conflict fixed in the
next commit** (`866ba6c3`), not in the merge as train 7 did with its two: the
merge commit had been made before `cargo check` ran, and the brief says not
to rewrite this branch's history. So `9a465471` alone does not build its
lib's test target: `cargo check --locked --all-targets` there stops with one
error, E0533 at `src/tui/event_loop.rs:6060` ("could not compile `xfx` (lib
test) due to 1 previous error") [measured]; no other target was checked at
that commit. From `866ba6c3` on, `cargo check --locked --all-targets` with
and without `fault-injection` and `cargo fmt --check` pass [measured].

### The build conflict (merge 1)

GRID-ROW-COST branched from `fdc5ac0b` (train 6), before STOP-NEXT (train 7)
gave `UiEvent::TurnStarted` its submission. Its session test
`a_groups_long_rows_stay_what_their_text_lays_through_scrolls_themes_and_a_resize`
applied `UiEvent::TurnStarted.made_inert()` as a unit variant; it now calls
`shell.start_turn()`, as STOP-NEXT converted `main`'s tests (`git diff
690aa505 58c92ba2 -- src`: every `apply(UiEvent::TurnStarted…)` of a test
became `start_turn()`, the two `.made_inert()` ones included). It is the
only unit-variant `TurnStarted` merge 1 brought (`git diff 58c92ba2 9a465471
-- src | grep TurnStarted`: that one line).

### Merge 2's conflict: `scripts/fxcap/capture.sh`

Both sides added a function after `since()`: LAUNCH-SETTLE's `await_up`
(train 7: the launch, and each `relaunch`, waits until the pane shows a first
frame or, under `keep`, is dead) and FLAKE-MCPMODE's `put_in_home` (a HOME
file the binary may read, renamed into place; a link or a directory there
refused). Resolved: both, `await_up` first (`remerge-2.txt`). Every other
hunk of either side merged textually, and the merged file keeps both
meanings (`capture-sh.txt`, the lines at the head):

- **the launch wait**: `await_up` (`capture.sh:467-495`), called by the
  launch (`:834-839`) and by `relaunch` (`:1007-1011`), `nodraw` its only
  way around;
- **every control file a verb changes mid-run renamed into place**:
  `put_in_home` (`:502-510`) for `reseed` (`:1015-1017`), `rehomefile`
  (`:1023-1025`) and `clock` (`:1057`); temp + `mv` for `release` (`:862`),
  `usage` (`:1062`), `mcphttp` (`:1065`), `models2` (`:1067`) and `clipimage`
  (`:1090`); the fake osascript's `cp` (`:767`).

LAUNCH-SETTLE's own new files (`$run/up`, `launched`, `seen`, `snapped`) are
stamps read by their mtime or written and read by `capture.sh` itself in
sequence, the class FLAKE-MCPMODE's table calls safe by construction
(`until`, `launched`). Train 7's harness tests added since FLAKE-MCPMODE's
base write only stale output files and stub scripts, before their run
(`test_capture.py:124`, the two `write(script, …)` calls), never a file a
running server reads. Before the merge was committed, `test-capture` on the
resolved tree: **72 tests, OK**, 302 s [measured] -- train 7's 68 and
FLAKE-MCPMODE's 4.

### Where the brief expected overlap, and where it was (`overlap.txt`)

Train 4's `overlap.py`: every file both sides of a merge changed since their
base, and the nearest pair of hunks.

- **Merge 1: two files, no textual conflict.** Train 7 changed `shell.rs`,
  `event_loop.rs` and `toolgroup.rs` since GRID-ROW-COST's base; GRID-ROW-COST
  changed **no line of `shell.rs`**. `event_loop.rs`: its one hunk (the
  session test) 158 unchanged lines from train 7's nearest. `toolgroup.rs`:
  its three hunks (`call_row`, the call row built at its length: its call
  and the function; and its RED test) 181 lines from train 7's nearest
  (T-U4-F's `unit_at` and `inline`); neither side changed the other's
  functions. The semantic meeting point is at run time, not in the text:
  STOP-NEXT-2 holds a stop's tool rows and late text in `Shell::late`
  (`shell.rs:1610`, `Late` at `:2165-2174`) and writes the rows later through
  `Shell::mark` (`release_late`, `:9204-9213`), the same way into the group
  and the transcript as any row; GRID-ROW-COST's per-row records
  (`grid.rs`'s `Laid`, `Grid::row_holds`) and the transcript's one-generation
  cut cache (`KeptRows`, `turn_kept_rows`) see those rows only as a later
  write of the block. Join check 1 drives that.
- **Merge 2: two files.** `capture.sh` (the conflict, above) and
  `test_capture.py` (13 unchanged lines apart; merged textually).
- **Merge 3: one file**, `test_capture.py` (252 lines apart).

## The token check (`token_check.py`, `token-check.txt`, `token-self-test.txt`)

Train 7's `token_check.py`, byte for byte (its clause, order, unchanged-row
and numbers-only rules, and its re-pointed-ref rules). **Result: all three
merges OK** [measured]. Neither unit changes `docs/parity.md` or the ledger
(`git diff 58c92ba2 3bf99857 -- docs/parity.md scripts/fxcap/parity-ledger.toml`,
up to the re-point commit, and `git diff 387a0104 c4bb4c7f` over the same
two files, merge 3: both empty), so every row and entry each merge holds
is `main`'s, token for token and in its order: merge 1 (base `fdc5ac0b`) reads
train 7's rows and entries as changed by ours and kept so; merges 2 and 3
(bases `690aa505`, `2971aeff`) read trains 6's and 7's the same way. No
comment line of the ledger lost.

**The negative control** (`--self-test`, on each merge's own texts, the cases
that apply): **merge 1: 7 of 7 found; merge 2: 12 of 12; merge 3: 12 of 12**
-- 31 of 31 [measured]. No case of train 7's re-pointed-ref rules applies:
no reference was re-pointed by both sides of any merge.

## The references (`d57beb36`; `refs.txt`, `refs-head.txt`, `repointed.txt`, `stale.txt`)

Train 7's `refs.py`, `refs_all.py`, `refs_short.py`, four sides: `main`
`58c92ba2`, GRID-ROW-COST `a3f90ef5`, FLAKE-MCPMODE `2971aeff`,
FLAKE-MCPMODE-F `6a9a6a9d`. At the join test's commit `3bf99857`, before the
re-points: in the ledger and `docs/parity.md` 81 references, **10 `MOVED`**
(`refs.txt`); in the widened scans the same 10 and **11 more in comments**
(`refs-all-merges.txt`, `refs-short-merges.txt`), every one cited by text
`main` holds and moved by a merge: neither unit re-pointed a reference into
code it moved (GRID-ROW-COST moved `frame.rs`, `grid.rs`, `full_transcript.rs`
and `toolgroup.rs` lines under them; FLAKE-MCPMODE `capture.sh`'s).

`d57beb36` re-points the 21 (`repoint.py`), each to where the code it names on
`main` is at the head. `repointed.py` checks each against `main`'s copy of
the cited file, the cited lines and two either side: **21 of 21 the same
code** (`repointed.txt`) [measured].

| who cites it | cited on `main` | now | moved by |
|---|---|---|---|
| ledger, `capture.sh`'s TMPDIR comment (normalization `capture-tmpdir`) | `scripts/fxcap/capture.sh:315-317` | `:317-319` | FLAKE-MCPMODE (`put_in_home` and its comment, above) |
| ledger, the run directory (normalization `run-directory`) | `capture.sh:521` | `:538` | the same |
| ledger, `remain-on-exit` (normalization `tmux-dead-pane`) | `capture.sh:952` | `:973` | the same |
| ledger (one entry) | `src/tui/frame.rs:2749` (`place`) | `:2757` | GRID-ROW-COST |
| ledger (two entries each) | `src/tui/full_transcript.rs:3005`, `:3056` | `:3010`, `:3061` | GRID-ROW-COST |
| ledger (one entry) | `src/tui/grid.rs:933` | `:1088` | GRID-ROW-COST |
| ledger `call-row-raw-argument`, `call-row-page-edge`: T-U4's `spelled` | `src/tui/toolgroup.rs:922` | `:940` | GRID-ROW-COST (`call_row`, above) |
| ledger `call-row-source-cut`: `command_spelled` | `toolgroup.rs:999` | `:1017` | the same |
| ledger `call-row-arabic-letter-mark`: the U+061C arm | `toolgroup.rs:1113` | `:1131` | the same |
| `frame.rs` comments (8) | `frame.rs:1063-1228`, `:1085`, `:1456`, `:1533-1585`, `:1587-1598`, `:1593-1616`, `:1617`, `:1617-1620` | `+6` each | GRID-ROW-COST |
| `frame.rs` comment | `grid.rs:240-243` | `:242-245` | GRID-ROW-COST |
| `wrap.rs` comments | `check.rs:1689-1694`, `grid.rs:210-214` | `:1808-1813`, `:212-216` | GRID-ROW-COST |

The ledger's twelve citations of the ten (`full_transcript.rs`'s two and
`toolgroup.rs:922` cited twice each) changed in numbers alone; the token check
reads them as ours, kept so, at merge 3 (merge 3's "ours" holds the
re-point commit).

At the head (`refs-head.txt`): 81 references, **0 moved**. What the widened
scans still flag (`refs-all.txt`) is not moved by this train (`stale.py`,
`stale.txt`, 12): train 7's eleven, each naming other code on `main` already
-- one of them, `wrap.rs`'s `check.rs:1689-1694`, moved by GRID-ROW-COST and
re-pointed by line to `:1808-1813`, so it names what it named on `main` --
and one more: `event_loop.rs:12004`'s `frame.rs:1615-1628` "the escape
`place()` writes", which on `main` names `render_append_recorded`'s reuse
comment (`place` is `frame.rs:2749` there) and whose lines GRID-ROW-COST
rewrote (`refs_short.py`: `CHANGED`), so it cannot be mapped; plus the
historical plans under `docs/superpowers/plans/` and lines the unit sides'
older copies read differently. Left for REF-LINT, as train 7 left its eleven.

## Join checks

### 1. GRID-ROW-COST x STOP-NEXT-2 / T-U4-F (`3bf99857`; `join-units.txt`, `join-mutants.txt`)

**What each unit does on the path** [read from the code at the head]. A
Ctrl-C over a running turn (`cancel_turn_with`) raises `cancelling` and
`stop_reads_end`; from then on STOP-NEXT-2 keeps the turn's answer text and
operational lines in `Shell::late` (`shell.rs:1610`; `Late`, `:2165-2174`),
and a tool row read behind held text goes there too (`mark_tool`,
`:9193-9199`), so it does not go ahead of that text. The turn's end, or a
drain that empties the channel (`ui_events_taken`), lets go of it
(`release_late`, `:9204-9213`): the rows always, through `Shell::mark` -- the
group's own path for any row -- and the text only with an end that is not
the stop's own. GRID-ROW-COST's frames keep, for each row they place, the
text it was laid from (`Laid`, `grid.rs:469`; `place_row`, `:610`), answer a
row left in place from that record (`Grid::row_holds`, `:704`), and forget it
on every other change to the row's cells (`erase_row`, `:548`, the retint);
the transcript keeps one generation of a block's cut rows (`KeptRows`,
`transcript.rs:682`; `turn_kept_rows`, `:986`, turned on every write of the
block, `:1209`). A held row reaches neither directly: it is a later write of
the group's block, and the rows it changes are placed again.

**At the head**, both cargo steps of the gate (5460 and 5497 passed, above)
ran GRID-ROW-COST's per-tick session test
(`a_groups_long_rows_stay_what_their_text_lays_through_scrolls_themes_and_a_resize`,
`event_loop.rs:6028`, with its Ctrl-O and collapse steps and its
`start_turn()`) and its equality-with-fresh-layout tests (`grid`'s three,
`check`'s three, `transcript`'s, and the five REDs): all pass; the same 13
and the join test, run alone at the merged tree before the join test's
commit: 14 passed [measured].

**The join test** (`a_stops_held_rows_over_a_groups_long_rows_are_written_as_their_text_lays`,
`event_loop.rs:16017`; its source `join_test.rs`, put in by
`apply_join.py`): an open group of two long calls (one letter under 64 Ki
marks each, as the session test has them), three reads joining, then a
Ctrl-C over the open group; behind the stop, late text (`LATE-TEXT`), a read
joining (`late.txt`) and the first long call's result, in a drain its bound
cut short (`ui_events_taken(false)`), so held; one tick while they are held;
then they are let go three ways -- with the stop's own end
(`TURN_CANCELLED`), with a finished end, and by a drain that empties before
a finished end -- and two ticks after. **After every tick: no vector
refused, and `Grid::stale_laid_rows` empty** (every row the shadow says was
laid from a text holds a fresh placement of it). And, per case: the long
rows laid from their text before the stop (`longest_laid`); the held tick
writes neither the read nor the text; after the end the read is written in
all three, the late text only with the finish, and the long rows -- both
written again, the first finished and the second stopped with the turn --
are known by their text again. **Passes at the head** (in both cargo steps
of the gate) [measured].

**On each side alone** (`join_units.sh`: a `git clone --no-local` of each
side, the test appended unchanged, `cargo test --no-run`; then the test with
the other side's half taken out, run; `jobs-a.txt`):

| side alone | the test unchanged | the other side's half taken out |
|---|---|---|
| GRID-ROW-COST `a3f90ef5` | **does not build**, 5 errors: STOP-NEXT's names (`bridge::TURN_CANCELLED`, `start_turn`, `ui_events_taken` x3) | STOP-NEXT's names replaced (`apply(UiEvent::TurnStarted)`, no `ui_events_taken`, the stop's end as its string): **fails** -- every tick's grid checks hold, but nothing is held: the read is written in the tick right after the stop (`holding` has `late.txt` in all three cases) and the late text is never shown, the finish included |
| `main` `58c92ba2` (STOP-NEXT-2, T-U4-F) | **does not build**, 3 errors: GRID-ROW-COST's (`stale_laid_rows`, `longest_laid` x2) | the grid's two checks taken out (no record to check): **passes** -- the hold is the head's |

So the test needs both units: the hold is STOP-NEXT-2's, and what it checks
of the rows the hold writes later is GRID-ROW-COST's records.

**Does it discriminate?** `join_mutants.py` on a `git clone --no-local` of
`c4bb4c7f` (`jobs-b.sh`, one hold of the suite lock), one mutant at a time
(one textual change), the file restored after each and the clone clean at
the end, each run bounded at 600 s (its process group killed past it), the
join test and GRID-ROW-COST's session test run each time; three of
GRID-ROW-COST's own grid mutants (its `mutants.py`'s text, unchanged) and two
on STOP-NEXT-2's hold. The unmutated clone passes both (`join-mutants.txt`).

| mutant | join | session | where the join test failed |
|---|---|---|---|
| G1 a row laid from any text answers for every text | **FAILED** | **FAILED** | a vector refused at the first long call, before the stop |
| G4 an erased row keeps its text | ok | **FAILED** | -- (no erased row on the join's path; the session test's collapse finds it) |
| G7 a placement remembers no text | **FAILED** | **FAILED** | the per-case tuple (the session test: "no row on the screen was laid from a long call's text") |
| S1 the stop's held tool rows dropped, never written | **FAILED** | ok | the per-case tuple |
| S2 a tool row read behind held text written at once, ahead of it | **FAILED** | ok | the per-case tuple |

**5 of 5 killed, 4 of them by the join test; none hung** [measured]. Which
of the tuple's fields each of G7, S1 and S2 moved was not printed (the
runner keeps the assertion's first lines); the two on the hold are killed by
the join test alone (the session test has no stop), the grid's by it and the
session test.


### 2. Render cost (`probe/probe.txt`, `probe/probe-summary.txt`)

GRID-ROW-COST's frame probe (`frame_cost_probe.rs`: an event applied, then
one `commit_frame` onto a screen that takes everything; 20 reads joining an
open group, 40 events; `heavy` -- one letter under 4 Mi U+0301 -- and `zw`
-- 2.8 Mi U+180E -- with one and three long calls, `short`, `c1`, `wider`
with one) and train 6's Ctrl-O probe as ARGROW-CACHE extended it
(`render_cost_probe.rs`, `XFX_PROBE_DETAIL=1`: the call's arguments too;
`c1`, 4 Mi U+009B, and `wider`, 2 Mi U+261D), with GRID-ROW-COST's runner
(`probe/run.sh`: its `run.sh`, the memory probe left out; release, one case a
process, the clones alternating case by case, two rounds), on three `git
clone --no-local`s with the two probes appended and nothing else
(`probe/prep.sh`): `main` `58c92ba2`, GRID-ROW-COST alone `a3f90ef5`, the
head `c4bb4c7f`. For `main` and the head the probes are the
`-head.rs` copies, the same files but for one line each -- `start_turn()`
where they applied `UiEvent::TurnStarted.made_inert()` (STOP-NEXT gave it its
submission). One hold of the suite lock, 19:39Z-19:47Z, load 4.2 to 12.2
(another worker's sweep beside it); all 54 runs exit 0. ms, `round 1 / round
2`.

| case | build | **tick (frame) median** | apply median | event worst | first placement | peak RSS MiB |
|---|---|---|---|---|---|---|
| heavy, 1 long call | `main` | 268.6 / 260.9 | 3.0 / 2.9 | 830 / 705 | 727 / 658 | 267 / 268 |
| | GRID-ROW-COST | 3.2 / 2.9 | 1.1 / 1.1 | 734 / 709 | 678 / 648 | 261 / 259 |
| | **head** | **2.9 / 2.9** | 1.1 / 1.1 | 721 / 712 | 677 / 655 | 261 / 259 |
| heavy, 3 | `main` | 951.1 / 936.0 | 11.0 / 10.3 | 1377 / 1347 | 1779 / 1794 | 614 / 613 |
| | GRID-ROW-COST | 8.6 / 8.6 | 3.3 / 3.3 | 753 / 726 | 1537 / 1513 | 670 / 669 |
| | **head** | **9.1 / 8.5** | 3.4 / 3.3 | 750 / 724 | 1561 / 1509 | 669 / 669 |
| zw, 1 | `main` | 337.0 / 323.6 | 3.0 / 2.9 | 934 / 889 | 903 / 872 | 691 / 686 |
| | GRID-ROW-COST | 3.0 / 3.1 | 1.1 / 1.1 | 907 / 919 | 873 / 877 | 285 / 287 |
| | **head** | **3.1 / 3.1** | 1.1 / 1.1 | 966 / 946 | 902 / 950 | 286 / 285 |
| zw, 3 | `main` | 1205.0 / 1169.7 | 11.2 / 10.8 | 2781 / 1690 | 2297 / 2295 | 915 / 911 |
| | GRID-ROW-COST | 8.4 / 8.5 | 3.3 / 3.3 | 889 / 896 | 1911 / 1910 | 718 / 719 |
| | **head** | **8.6 / 8.6** | 3.3 / 3.3 | 922 / 913 | 1954 / 1970 | 719 / 719 |
| short, c1, wider (1) | all three | 0.2-0.3 | 0.0 | 0.2-3.0 | 0.2-6.9 | 10-116 |

| Ctrl-O, with arguments | build | open | join | theme | resize | detail | peak |
|---|---|---|---|---|---|---|---|
| `c1` | `main` | 73.4 / 74.3 | 30.8 / 32.2 | 33.7 / 31.6 | 55.5 / 55.2 | 74.8 / 73.2 | 415 |
| | GRID-ROW-COST | 76.4 / 72.7 | 28.8 / 30.0 | 27.1 / 26.4 | 55.1 / 54.3 | 69.8 / 71.4 | 383 |
| | **head** | **81.7 / 69.3** | **29.2 / 32.8** | **27.2 / 29.8** | **56.2 / 54.8** | 66.8 / 73.1 | 383 |
| `wider` | `main` | 26.5 / 25.1 | 3.1 / 3.0 | 2.6 / 2.7 | 25.4 / 24.3 | 36.6 / 36.9 | 138 |
| | GRID-ROW-COST | 27.1 / 27.2 | 3.1 / 3.0 | 2.6 / 2.7 | 24.0 / 24.9 | 37.2 / 36.6 | 137 |
| | **head** | **26.5 / 26.8** | **3.0 / 3.0** | **2.6 / 2.7** | **24.0 / 25.0** | 35.7 / 38.4 | 137 |

**Head ÷ GRID-ROW-COST alone, every step of both probes, round by round:
0.87 to 1.13; none past 1.2x -- no finding** [derived from
`probe/probe.txt`; `probe-summary.txt` has every ratio]. The highest are
`c1`'s Ctrl-O theme in round 2 (29.8 against 26.4 ms, 1.13; 1.00 in round 1)
and its join in round 2 (1.09). The head keeps GRID-ROW-COST's numbers: the
tick with the long rows left in place 2.9-3.1 ms with one long call and
8.5-9.1 with three -- GRID-ROW-COST's own receipt 2.9-3.3 and 8.4-9.7 -- where
`main` takes 261-337 and 936-1205 ms (head ÷ `main` 0.01 on the tick median);
GRID-ROW-COST's bar (16 ms) holds at the head. Against `main`, the head's
peak is GRID-ROW-COST's own trade (`heavy` x3 614 -> 669 MiB, `zw` x1 691 ->
286), and Ctrl-O's `c1` peak 415 -> 383; no Ctrl-O step past 1.11x `main`'s.


### 3. FLAKE-MCPMODE x LAUNCH-SETTLE (`test-capture.txt`, `capture-sh.txt`)

`scripts/fxcap/test-capture.sh` five times at `c4bb4c7f` (all three merges),
from a `git clone --no-local`, one after another in one hold of the suite
lock (`jobs-a.sh`), 19:05Z-19:31Z, the train's other sweeps in other
slots beside them (`main`'s through all five), load 2.8 to 9.4 [measured]:

| run | result | seconds | load |
|---|---|---|---|
| 1 | 76 tests, OK | 306 | 7.87 |
| 2 | 76 tests, OK | 310 | 9.44 |
| 3 | 76 tests, OK | 305 | 2.80 |
| 4 | 76 tests, OK | 296 | 3.44 |
| 5 | 76 tests, OK | 289 | 3.12 |

And `gates-f.txt`'s run at `c4bb4c7f` (76, OK), the gate's at
`387a0104` (72, before merge 3) and the run on merge 2's resolved tree (72):
**eight runs, no failure**. 76 is train 7's 68, FLAKE-MCPMODE's 4 and
FLAKE-MCPMODE-F's 4 (`test-counts.txt`). unittest names a test only when it
fails or errors, and every run says `OK`: so in each, LAUNCH-SETTLE's launch
and relaunch tests, FLAKE-MCPMODE's (`test_a_held_request_reads_past_a_mode_file_caught_empty`,
`test_a_usage_file_or_second_catalog_caught_empty_is_read_again`,
`test_a_file_a_verb_changes_mid_run_is_replaced_whole`, the confinement
test) and FLAKE-MCPMODE-F's deadline tests passed. Five passes do not prove
a race is gone; the units' receipts carry the mechanisms. The merged
`capture.sh` keeps the launch wait and the atomic writers (the lines are in
Merge 2's conflict, above, and `capture-sh.txt`).

### 4. Network hermeticity (`net.txt`, `connect.log`)

Train 7's method (`net.py`: FLAKE-CATALOG-EVENT's `connect_logger.py`, a
loopback stand-in for a proxy that logs each connection's first request
line, answers 403 and forwards nothing, the script's own child on an
ephemeral port and ended by it; the four proxy variables at it, `NO_PROXY`,
`no_proxy`, `ALL_PROXY`, `all_proxy` unset), on a `git clone --no-local` at
`c4bb4c7f`, in one hold of the suite lock (`jobs-a.sh`).

| run | result | CONNECTs logged during it |
|---|---|---|
| `cargo test --locked --lib --no-fail-fast` | 4543 passed, 0 failed, 7 ignored | **2**, both `CONNECT w.example:443` |
| `cargo test --locked --test tui --no-fail-fast` | 181 passed, 0 failed | **0** |

**No CONNECT to the Gateway host**, in either suite [measured]. The two
`w.example` lines are the known MCP probe (trains 6 and 7). 4543 is train
7's 4529 and this train's 14.


## Gate (`gates.sh`, `gates.txt`; `gates-f.sh`, `gates-f.txt`)

`gates.sh` (trains 5's to 7's, its comment only changed): every step the
brief lists, at `387a0104` (the code of `d57beb36`: merges 1 and 2, the
build-conflict fix, the join test and the re-points), in a `git clone
--no-local` at that commit, one after another, **the whole script inside one
hold of the suite lock**, each step's exit taken from `$?` on the line right
after its command. Tree clean at the start and the end; load 2.4 to 12.7 at
the steps' starts (the train's head sweep and another worker's gate beside
it). **20 steps, 0 failed, none run again** [measured].

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| the same `--features fault-injection` | ok |
| `cargo test --locked --no-fail-fast` | **5460 passed**, 0 failed, 7 ignored (15 binaries; `tests/tui.rs` 181) |
| `cargo test --locked --features fault-injection --no-fail-fast` | **5497 passed**, 0 failed, 7 ignored (`tests/tui.rs` 195) |
| `scripts/check-xfx-identity.sh` / `check-no-stubs.sh` / `check-no-secrets.sh` | ok / ok / ok (15451 tracked files) |
| `scripts/check-preview-contract.sh` / `check-tui-preflight-cost.sh` / `check-usage-clock.sh` | ok / ok (200x300 replay 1.27 ms mean; budget 32 ms) / ok |
| `scripts/test-check-no-secrets.sh` / `test-gate-env.sh` / `test-preview-contract.sh` | ok (13 cases) / ok (62 cases) / ok (24 mutants caught) |
| `scripts/fxcap/test-sweep.sh` | 70 + 27 tests, OK |
| `scripts/fxcap/test-capture.sh` | **72 tests, OK** |
| `cargo build --locked --release`; `--release --features fault-injection --target-dir target/faulty` | ok; ok |
| `scripts/smoke.sh target/release/xfx <dir>` | 47 checks, 0 failures |
| `scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx <dir>` | 37 scenarios and the oracle, **1168 checks**, 0 failures |

**Every side's tests are in** (`test-counts.txt`, train 7's count: lines
holding `#[test]` under `src` and `tests`, `    def test_` under
`scripts/fxcap`): `main` 5360; GRID-ROW-COST +13 (5373 at merge 1, its 13
against `fdc5ac0b`'s 5280), the join test +1, 5374 at the head; the harness
165 on `main`, FLAKE-MCPMODE +4 (169 at merge 2), FLAKE-MCPMODE-F +4 (173 at
merge 3). `cargo test` counts 5460 against train 7's 5446 at `main`'s code
(its gate at `29786ced`): +14, the same difference (13 + 1); with fault
injection 5497 against 5483, +14.

**Merge 3's steps** (`gates-f.sh`, `gates-f.txt`). FLAKE-MCPMODE-F changes
`scripts/fxcap/mcphttp.ts`, `scripts/fxcap/test_capture.py` and its receipt,
and no Rust; no smoke script reads `scripts/fxcap/` (`smoke-tui.sh` names
`jobctl.pl` once, in a comment). So the steps it touches ran again at
`c4bb4c7f`, in a clone, one hold of the lock, gates.sh's `step()`: `cargo fmt
--check`, `check-xfx-identity`, `check-no-stubs`, `check-no-secrets` (15460
tracked files), `test-check-no-secrets`, `fxcap/test-sweep.sh` (70 + 27, OK)
and `fxcap/test-capture.sh` (**76 tests, OK**): **7 of 7, exit 0**, tree
clean before and after, load 2.4 to 5.0 [measured]. gates.txt's cargo,
build and smoke steps stand for `c4bb4c7f`: `git diff 387a0104 c4bb4c7f --
src tests Cargo.toml Cargo.lock scripts/*.sh` is empty.

**The file checks with the receipt in its final form** (`gates-final.txt`, at
`25bdf117`, in a clone): `cargo fmt --check`, `check-xfx-identity.sh`,
`check-no-stubs.sh`, `check-no-secrets.sh` (15521 tracked files), each exit
0, and no added line of `git diff 58c92ba2..25bdf117` holds the home root,
the account's name or the scratch root [measured]. The code has not changed
since `c4bb4c7f`; the one commit after `25bdf117` changes only
`gates-final.txt` and this README.



## Sweep

**Builds** (debug, as trains 4 to 7), each in its own `git clone --no-local`,
each swept with its own tree's harness, scenarios and ledger (`sweep.sh`,
train 7's): the head `387a0104` (the code of `d57beb36`; `x𝒇x
v0.1.0-387a010 [debug]` on row 1 of all 430 xfx snaps that show the banner,
sha256 `88c14eeb…`) and `main` `58c92ba2` (`v0.1.0-58c92ba`, all 430,
`6415a9e9…`). Merge 3 changes one file a sweep runs, `scripts/fxcap/mcphttp.ts`
(the read error kept for the log line); the 14 scenarios of FLAKE-MCPMODE's 48
that use its `mcphttp` verb were swept again at `c4bb4c7f` with that
harness (`sweep-f.sh`, `selection-f.txt`; `v0.1.0-c4bb4c7`, all 23 banner
snaps). fx: the v0.0.16 binary, sha256 `8e6cda2e…` before and after each
sweep, `FX_AUTO_UPGRADE=0`; 40x120, `--jobs 2`; each sweep inside one hold
of the suite lock (`sweep-runs.txt`). `main`'s sweep and each unit's own were
rescored with the head's ledger (`sweep.sh --rescore` on copies, no capture
taken; it moved no pass count), so a verdict that moves moved because a
capture did.

**Selection** (`selection.txt`, each group with why): the ctrl-o, transcript
and toolgroup scenarios -- GRID-ROW-COST's own selection (every
`p5-transcript-*`, the cancel scenarios `interrupt*`, `question*`,
`compact-cancel`, every `p5-ctrlo-wider-*` and `p5-toolgroup-*`, the wraps,
`h1-resize`, `fulltranscript*`, `toolgroup*`, `p5-inline-code-*`: 75) and
train 7's `*ctrlo*`; every scenario FLAKE-MCPMODE's receipt lists
(`scenarios.tsv` on its F branch: 48, 5 of them in the first group); and the
scenarios of every ledger entry the merges touched -- none: neither merge
changed the ledger, and the re-point commit changed reference numbers only,
which no sweep reads. **118 scenarios**, the same on both trees (no merge
adds a scenario). Three of the 48 (`p5-lifecycle-osc-interrupt`,
`-interruptnext`, `-states`) are wire-only, with no snap pair.

**Load.** The head sweep ran beside the gate and another worker's gate (load
5.5 at its start and its end); `main`'s beside `jobs-a` (5.9 and 3.4).

**Result.** Head (`sweep-head.md`): 118 scenarios, **0 failed, 0 timed out**;
529 pairs, **381 pass** (72.0%). `main` (`sweep-main.md`): 118, 0 failed, 0
timed out, 529 pairs, **381 pass**.

| family | **head** | `main` | GRID-ROW-COST's own | FLAKE-MCPMODE-F's own |
|---|---|---|---|---|
| Ctrl-O (`*ctrlo*`, `fulltranscript*`) | **21 / 54** | 21 / 54 | 21 / 54 | 1 / 4 |
| `p5-transcript-*` | **51 / 107** | 51 / 107 | 51 / 107 | 9 / 16 |
| tool groups (`toolgroup*`, `p5-toolgroup-*`) | **34 / 49** | 34 / 49 | 34 / 49 | - |
| the cancel scenarios (`interrupt*`, `question*`, `compact-cancel`) | **84 / 93** | 84 / 93 | 84 / 93 | 5 / 5 |
| the wraps, `h1-resize`, `p5-inline-code-*` | **44 / 49** | 44 / 49 | 44 / 49 | - |
| GRID-ROW-COST's selection (75) | **228 / 340** | 228 / 340 | 228 / 340 | 15 / 25 |
| FLAKE-MCPMODE's 48 (`scenarios.tsv`) | **174 / 214** | 174 / 214 | 21 / 25 | 164 / 214 |
| every scenario swept | **381 / 529** | 381 / 529 | 228 / 340 | 164 / 214 |

(pass / pairs, each sweep scored with the head's ledger, over its own pairs;
`families.txt`.)

### Against `main` (`compare-main.txt`, `screen-moves-main.txt`)

Over the 529 pairs both took: **381 -> 381; 0 verdicts moved; 0 down**
("passed on the reference, not on the train: 0"). Six pairs gained a
differing row, each unresolved on both and each run-local text:
`h1-rehomefile` `status`, `p5-effort-fork-fork-edge` `status-empty`,
`p5-panels-status-badsettings` `open` (the sweep's own random temporary
directory on `/status`'s Workspace row, which FLAKE-MCPMODE-F's receipt names
too), `p5-usage-turns` `credits` and `stats` (`/stats`'s `ansi_bytes`, on fx's
side as on xfx's: fx 15652 -> 15585), `interrupt-recovery` `full` (a network
notice's `0ms` -> `1ms`, fx's `14ms` -> `13ms`). `screen-moves-main.txt`: 190
of 220 moved snaps only in clock-like or run-local text; the other 30 are
resume and session ids, the build id on `/status`, and fx's leftover `)` in
`p5-ctrlo-wider-ri` (a known fx artefact, `ri-run-pairing`) -- no xfx row
of a ctrl-o, transcript or toolgroup screen moved.

### Against each unit's own sweep (`compare-unit-*.txt`, `screen-moves-unit-*.txt`)

- **GRID-ROW-COST** (its head sweep, `50a099b` -- the code of `96c3c88a`, on
  `fdc5ac0b` -- its 75 scenarios, 340 pairs): **228 -> 228; 0 verdicts
  moved**; one pair gained a row, `interrupt-recovery` `full` (the timing
  above, as on GRID-ROW-COST's own base-vs-head). Of the snaps that moved
  beyond clock-like or run-local text (15), 12 are xfx screens whose saved
  session's resume id moved and 3 are fx's leftover `)` in
  `p5-ctrlo-wider-ri`.
- **FLAKE-MCPMODE-F** (its head-harness sweep, `pairs/head`: xfx built at
  `a2c0265a`, which is `690aa505`'s product code, 48 scenarios, 214 pairs):
  **164 -> 174; 10 up, 0 down.** All ten are product changes between
  `690aa505` and `main`, and `main`'s own sweep has every one of them as the
  head has it (against `main`, 0 moved): T-U4's cancelled marker (train 6;
  `■` -> fx's `▪`) in `p5-transcript-cancel` `cmd-esc`, `cmd-running`,
  `end.full`, `esc`, `mid`, `next` and `p5-mcp-live` `cancel`, `end`,
  `failed`; MODELS-REASK's second catalog read (train 7) in `h1-models2`
  `after`. The rows gained are the six run-local ones above.
- **FLAKE-MCPMODE** (`2971aeff`) has no sweep of its own; FLAKE-MCPMODE-F's
  is the unit's (its base harness against its head harness on one binary:
  214 of 214 the same verdict).

### Merge 3's harness (`compare-final-mcphttp.txt`)

The 14 `mcphttp` scenarios at `c4bb4c7f` against the same pairs of the head
sweep: **24 pairs, 18 -> 18, 0 verdicts moved, 0 rows gained**; every moved
snap (25) only in clock-like or run-local text. Against FLAKE-MCPMODE-F's
own: 15 -> 18, the three `p5-mcp-live` markers above.

So no pair that passes on `main`, on GRID-ROW-COST's own sweep or on
FLAKE-MCPMODE-F's fails on the train.


## Redaction

The repository is public. Every file here taken from a sweep, a gate log, a
probe, the logger, a mutant run or a clone went through `redacted.py` (train
7's, unchanged): `scripts/fxcap/redact.py`'s `Redactor.write`, every path
under a local root to `<local-path>` or a named root such as `<repo>`, a
checkout's path cut short to `<checkout>/`, the account's name to `<user>`.
The scripts take the oracle's path, the suite lock and the scratch
directories as arguments or variables. Then, over every file of this
directory, for the four forms trains 5 to 7 named -- the uid fragment before
a slash, the machine's scratch root's name, the system temporary root and
the home root, each spelt with a bracket class so the command matches what
it looks for and not its own text -- and the account's name: **0 hits**
(`redaction-grep.txt`). And over the whole train, `git diff
origin/main..HEAD`'s added lines: none holds the home root, the account's
name or the scratch root (the dispatcher's check; merge 3 took out the four
merge 2 had brought).

## Found on the way, not changed here

- **The units' own stale references.** GRID-ROW-COST moved code under 18
  references `main`'s text holds (the ledger's seven, eleven comments) and
  FLAKE-MCPMODE under three (the ledger's `capture.sh` ones), and neither
  re-pointed them on its branch: re-pointed here (above). The one it rewrote
  under (`event_loop.rs`'s `frame.rs:1615-1628`) was stale on `main` already
  and is in `stale.txt`.
- **Merge 1's commit does not build its lib's test target** (above): a
  bisect over this branch should skip `9a465471` for the tests.

## Files

- `remerge-1.txt` .. `remerge-3.txt`: `git show --remerge-diff` of each merge.
- `overlap.py`, `overlap.txt`: every file both sides of a merge changed, and
  the nearest pair of hunks. `capture-sh.txt`: the merged `capture.sh`'s
  launch wait and atomic writers, by line.
- `merge-checks.sh`: writes the token check, its control, the remerge-diffs,
  the reference maps, the re-points' check, the stale list and the overlaps.
- `token_check.py` (train 7's, byte for byte), `token-check.txt`,
  `token-self-test.txt`.
- `refs.py`, `refs_all.py`, `refs_short.py` (train 7's); `refs.txt`,
  `refs-all-merges.txt`, `refs-short-merges.txt` (at `3bf99857`, before the
  re-points); `refs-head.txt`, `refs-all.txt` (at `c4bb4c7f`); `repoint.py`
  (the re-point commit's edit), `repointed.py`, `repointed.txt`; `stale.py`,
  `stale.txt`.
- `join_test.rs`, `apply_join.py`: join check 1's test; `join_units.sh`,
  `join-units.txt`: on each side alone; `join_mutants.py`, `join-mutants.txt`:
  its mutants.
- `jobs-a.sh` (the sides alone, the network, test-capture five times),
  `jobs-b.sh` (the mutants): each one hold of the suite lock; `jobs-a.txt`,
  `jobs-b.txt`: what each printed. `net.py`, `net.txt`, `connect.log`: join
  check 4; `test-capture.txt`: join check 3's five runs.
- `probe/`: `frame_cost_probe.rs` (GRID-ROW-COST's), `render_cost_probe.rs`
  (ARGROW-CACHE's), their `-head.rs` copies (`start_turn()`), `prep.sh`,
  `run.sh`, `summarize.py`, `probe.txt`, `probe-summary.txt`: join check 2.
- `gates.sh`, `gates_txt.py`, `gates.txt`: the gate; `gates-f.sh`,
  `gates-f.txt`: merge 3's steps; `gates-final.txt`: the file checks with the
  receipt in its final form.
- `test_counts.sh`, `test-counts.txt`: `#[test]` and the harness's `def
  test_` per commit.
- `selection.txt`, `sweep.sh`: the sweep; `selection-f.txt`, `sweep-f.sh`:
  merge 3's; `sweep-runs.txt`: their build, oracle and load lines;
  `sweep-head.md`, `sweep-main.md`, `sweep-unit-*.md` (each rescored with the
  head's ledger), `sweep-final-mcphttp.md`; `sweep-compare.sh`, `compare.py`,
  `compare-*.txt`, `screen_moves.py`, `screen-moves-*.txt`, `families.py`,
  `families.txt`: pair by pair and per family.
- `redacted.py`: every sweep-, gate- and run-derived file here went through
  `scripts/fxcap/redact.py`'s `Redactor.write`; `redaction-grep.txt`: the
  grep over this directory, its command and its count.
