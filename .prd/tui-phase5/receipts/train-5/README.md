---
title: 0.0.16 delta -- P5 train 5 receipt (FLAKE-COPYEND, L-U1+U2 into one branch)
type: receipt
tags: [xfx, phase5, fx-0.0.16, train]
---

# P5 train 5: two closed units, one branch

Branch `train/p5-1108b`, from `origin/main` `884497c0` (PR #80, train 4:
MCP-E, TC-NOTATION, FLAKE-HINT4), as the brief expected; `origin/main` had
not moved when it was fetched. Each unit was closed by its own two-engine
review and gate; this train combines them. The program goal (the user's
words): «최신 fx랑 비교해서 ui쪽으로 모든 기능 구현하고 기본적으로 최신 fx ui를
그대로 구현해줘» (fx v0.0.16, pin `481104fd`).

Oracle: fx v0.0.16 (`481104fd`), executable sha256
`8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked
by `capture.sh` before and after every run, `FX_AUTO_UPGRADE=0`.

**At a glance.**
- Merges: 1 is git's own (empty remerge-diff; no file both sides changed).
  Merge 2 resolved one `docs/parity.md` hunk by hand: L-U1+U2's two new rows
  beside `main`'s rewritten `colored and hyperlinked TTY output` row, both
  kept. Nine references the merges moved, re-pointed and checked against
  their code (9 of 9).
- Join checks: the timer-window lint finds no violator among L-U1+U2's 13
  osc scenarios (nor with `wire` read as `snap`; its negative control
  refuses both timers); no osc scenario uses MCP, an MCP server's startup
  sets neither `working` nor `blocked` on either product (code, and a
  capture: `idle` through the 2026-07-28 server's startup on both); under
  `COLORTERM=truecolor` osc.py's 43 wire pairs still hold every claim and no
  osc frame on either binary carries a `38;2`; a join test pins the
  session's end -- the copy given up after the last frame and before the
  restore. Round 2 (`ffd53286`, after both resolution reviews) rebuilt it
  on `/copy` (no deadline, both platforms), judged on the whole terminal
  stream after the exit. 6 of 6 mutants killed, and 6 of 6 again with
  its observer delayed 3 s across the shutdown (the control); round 1's
  sampled test, delayed the same way, let two of them through. It passes
  on each unit alone.
- Gate (round 2) at (the next commit), one suite-lock hold: in the next commit. A first
  round 2 run at `f788dc54`, before a comment-only change to the join test
  (`7318963f`), was the same: 20 of 20 steps, 5327 and 5364, `test-capture`
  59 OK, the touched tests 3 of 3 green.
- Gate (round 1) at `d6a10eeb`, all under one suite-lock hold: 20 steps, 19 passed
  (`cargo test` 5327, with fault injection 5364, each +48 on `main`'s;
  smoke-tui 1168 checks). `test-capture.sh` failed one case, a wall-clock race in the
  harness's own test that neither unit touches (owned by LAUNCH-SETTLE, its
  item 6, in flight); under the lock right after, 59 of 59 OK and the case
  alone 5 of 5.
- Sweep, 58 scenarios at the head: 166 of 300 pairs pass, 0 failed, 0
  timeout. Against `main` 165 → 166, none down (one timing move up).
  Against L-U1+U2's own sweep 166 → 166: 3 up (TC-NOTATION's) and 3 down,
  each the harness's fixed `modelsdown` clock in L-U1+U2's run (its catalog
  asks came after the outage window, so it never saw the outage the
  scenarios are about), unresolved on `main` too; L-U1+U2's own xfx under the train's harness, locked, fails the
  three the same way.
  FLAKE-COPYEND changed nothing a sweep reads and has no sweep of its own.

## Units and merges

Each is `git merge --no-ff`, in this order; each message names the unit and
its head.

| # | unit | branch @ tip | merge | base | conflicts |
|---|---|---|---|---|---|
| 1 | FLAKE-COPYEND r2 | `fix/p5-flake-copyend` @ `0b432f9a` | `2fefa1fb` | `e6cde019` | none |
| 2 | L-U1+U2 r5 | `feat/p5-osc-program-status` @ `e636db48` | `7d91f833` | `e6cde019` | `docs/parity.md`, one hunk: two rows inserted beside a row the other side rewrote |

Both bases are git's merge bases, and both are `e6cde019` (train 3, PR #79):
each unit already held it (FLAKE-COPYEND's `d42ab154`, L-U1+U2's
`2f6323a3`), and `main` `884497c0` is train 4 on top of it.

After the merges, in order:

| commit | what |
|---|---|
| `81a71203` | the `path:line` references the merges moved, re-pointed (below) |
| `150334d9` | the copy errand x program status join test, round 1 (below) |
| `d6a10eeb` | this receipt, first part (round 1's gate ran here) |
| `140344ab`, `01daedb4` | this receipt, the rest (round 1) |
| `ffd53286` | round 2: the join test rebuilt on `/copy`, judged after the exit; the fake clipboard's hold mode (below) |
| the next | round 2's receipt: the join section, the LAUNCH-SETTLE owner, the redaction, round 2's gate |

`git show --remerge-diff` of each merge is `remerge-1.txt` and
`remerge-2.txt`. Merge 1's has an empty body: git's own result, unchanged.
Merge 2's names one file, `docs/parity.md`, and the one hunk below.

### Where the brief expected overlap, and where it was

The brief expected FLAKE-COPYEND to meet `tests/tui.rs` and the copy and
clipboard code, and L-U1+U2 to meet train 4 in `shell.rs`, `event_loop.rs`
and `smoke-tui.sh`. `overlap.txt` (train 4's `overlap.py`) has every file both
sides of a merge changed since their base, and the nearest pair of hunks:

- **Merge 1: no file.** `main` changed none of `src/clipboard.rs`,
  `src/tui/copy.rs`, `src/tui/trace.rs` (FLAKE-COPYEND's three code files)
  or `tests/tui.rs` since `e6cde019` (`git diff --stat e6cde019 884497c0 --`
  those four is empty), and FLAKE-COPYEND changed nothing under `tests/` or
  `scripts/`. The two units share no file either (`comm -12` of their two
  `git diff --name-only e6cde019 <tip>` lists is empty), and L-U1+U2 changed
  none of FLAKE-COPYEND's three. So the copy code meets L-U1+U2 only at run
  time, at a session's end: the join check below.
- **Merge 2: eight files.** `docs/parity.md` (the conflict, below);
  `scripts/smoke-tui.sh` (nearest hunks 2 unchanged lines apart: `main`'s
  re-pointed `theme.rs:66` comment against L-U1+U2's `KNOWN_OSC` gaining
  `7501` and its `STATUS_*` constants -- a comment beside a table, no shared
  logic; the rest of each side's hunks are TC-NOTATION's SGR parser and
  retint scenarios against L-U1+U2's OSC 7501 emulator, oracle and settle
  wait); `src/tui/check.rs` (74), `src/tui/event_loop.rs` (78),
  `src/tui/shell.rs` (39), `src/tui/mod.rs` (248), `src/tui/approval.rs`
  (1292), `src/tui/question.rs` (1777). All but `docs/parity.md` merged
  textually, and the merged tree builds with and without `fault-injection`
  (`cargo check --locked --all-targets`, both). `git grep` over the merged
  `src` and `tests` finds none of the names TC-NOTATION deleted (`Depth`,
  `shell_at(`, `depth_from_env`, `.truecolor()`, `painter_of`), and no line
  L-U1+U2 added writes or expects `38;2`.

### Merge 2's conflict: `docs/parity.md`

One hunk. At the base, the `colored and hyperlinked TTY output` row is line
241. `main` rewrote it in place (TC-NOTATION's row: 27 tokens added, 137
deleted). L-U1+U2 inserted two rows right above it --
`terminal program status (OSC 7501)` and
`a question's line breaks in a program status message` -- and left the row
itself as the base has it. Git saw both sides touch line 241.

Resolution, both sides' meaning: L-U1+U2's two rows where L-U1+U2 put them,
then `main`'s row as `main` has it. No row was changed by both sides. The
merge message says so. `docs/parity.md` is 291 lines at the head (288 on
`main`, 291 on L-U1+U2): L-U1+U2's third new row,
`A program status report (OSC 7501)`, is in the table under "Full-screen TUI:
a write the terminal took only part of" (line 261), which `main` did not
touch.

## The token check (`token_check.py`, `token-check.txt`, `token-self-test.txt`)

Train 4's `token_check.py`, with one rule added. For each merge, against
git's merge base: `docs/parity.md` row by row (whitespace tokens as a
multiset: the merged row is the base's plus what each side added minus what
each side deleted), and the ledger entry by entry (an entry one side changed
is that side's, one a side deleted is gone, one a side added is in, the rest
the base's; every comment line a side added is in, every one a side deleted
is not), with train 4's numbers-only rule. **Added for train 5: the rows'
order.** Every two table rows a side holds in one order, and which the merge
holds, are in that order in the merge -- so a row a side inserted sits where
that side put it against every row both kept. The multiset check alone did
not look at order, and merge 2's one conflict was an order question.

**Result: both merges OK.**

- Merge 1: FLAKE-COPYEND changed neither file. `main`'s `MCP client runtime`
  (846 tokens added, 91 deleted) and `colored and hyperlinked TTY output`
  (27 added, 137 deleted) as `main` has them; the ledger is `main`'s
  (train 4's entries; `truecolor-grey-notation` deleted by `main` and gone;
  4 comment lines `main` deleted, none back).
- Merge 2: L-U1+U2's `transport-owned retry and team routing` (49 added),
  `full-screen TUI` (18 added, 1 deleted), and its three new rows
  (`terminal program status (OSC 7501)` 1164 tokens, `a question's line
  breaks in a program status message` 106, `A program status report (OSC
  7501)` 82); `main`'s two rows; every row in each side's order; the ledger
  `main`'s (L-U1+U2 did not change it).

No `docs/parity.md` row was changed by both sides of either merge.

**The negative control** (`--self-test`, on each merge's own texts): train
4's cases (a token a side added dropped, a token a side deleted put back --
where the first row a side changed deleted one --, a ledger entry a side
deleted restored, a comment line dropped) and train 5's: every row a side
changed put back as the other side has it, every row a side added dropped,
and each row a side added next to a row the other side changed swapped with
it (the order rule). Merge 1: 6 of 6 found. Merge 2: 11 of 11 found, among
them `main`'s `colored ...` row taken as L-U1+U2 has it (the wrong side of
the conflict), each of the three new rows dropped, and
`a question's line breaks ...` swapped with `colored ...`.

The re-point commit `81a71203` changes ledger numbers after the merges; the
token check is about the merges, and `repointed.py` checks each of those
numbers against its code (below).

## The references the merges moved (`81a71203`)

Train 4's `refs.py` (the ledger and `docs/parity.md`), three sides: `main`
`884497c0`, FLAKE-COPYEND `0b432f9a`, L-U1+U2 `e636db48`. At merge 2
`7d91f833` (`refs.txt`): 68 references, 8 (side, reference) pairs `MOVED`,
five references, every one `main`'s pointer moved by L-U1+U2's insertions
above it (in `shell.rs`, and two rows in `docs/parity.md`). L-U1+U2's own
copies of those references were stale on its branch: it moved the code and
did not change the ledger. Widened (train 4's `refs_all.py`, every unpinned
`path:line` in every tracked file outside `.prd/`, and `refs_short.py`, the
same for `name.rs:N`), at merge 2 (`refs-all-merge2.txt`): four more in
`capfile.py` and `smoke-tui.sh` that L-U1+U2 moved and `main` had right.

`81a71203` re-points those nine, each to the code it named on `main`.
`repointed.py` checks each against `main`'s copy of the cited file: **9 of 9
the same code** (`repointed.txt`).

| who cites it | cited on `main` | now | moved by |
|---|---|---|---|
| ledger `inline-code-shade` (source), the `colored and hyperlinked TTY output` row | `docs/parity.md:241` | `:243` | L-U1+U2's two rows above it |
| ledger `banner` normalization (why, source) | `src/tui/shell.rs:184` | `:185` | L-U1+U2 (+1 above) |
| ledger `stats-bytes` (xfx) | `src/tui/shell.rs:1583` | `:1644` | L-U1+U2 (+61) |
| ledger `submit-multi-card-whole` (why; `send`) | `src/tui/shell.rs:12272` | `:12600` | L-U1+U2 (+328) |
| ledger `session-record`, `transcript-page-offset`, `toolgroup-ctrlo-record-short`, `safe-page-edge`, `full-transcript-shift` (xfx) | `src/tui/shell.rs:37424` | `:37758` | L-U1+U2 (+334) |
| `capfile.py` `TIMERS`, `warning_shown` | `src/tui/shell.rs:13705-13730` | `:14033-14058` | L-U1+U2 (+328) |
| `capfile.py` `TIMERS`, the full transcript | `:13723-13725` | `:14051-14053` | L-U1+U2 (+328) |
| `smoke-tui.sh`, the mode set and the restores in order | `src/tui/term.rs:56-90` | `:56-101` | L-U1+U2's paragraph and clears inside the range |
| `smoke-tui.sh`, every restore opens with `?2026l` | `src/tui/term.rs:74-98` | `:74-109` | the same |

The two `term.rs` ranges grew rather than moved: L-U1+U2 wrote a paragraph
of doc comment and the program status clear into the four restore literals
inside them. `repointed.py` checks their first line and two either side as
the same text, and their last line and the one above it as the same text
once the unit's clear is taken out (the `RESTORE_TMUX` declaration; the
`ABNORMAL_RESTORE` literal). The `docs/parity.md` row is checked alone: the
two lines above it are the rows the merge inserted.

At the head (`refs-head.txt`): 68 references, 0 moved. `refs-all.txt` holds
the widened scans at the head. What they still flag is not moved by this
train:

- 9 references outside the plans name other code on `main` already
  (`stale.py`, `stale.txt`: each citing line at the head beside the line it
  names on `main`). Three were in train 4's widened scans too, flagged
  against older frames and left as `main` had them (`smoke-tui.sh`'s
  `check.rs:1084`, stale since `9acf15d1`; `event_loop.rs`'s
  `event_loop.rs:959-962`, written 2026-10-06 at `063fc66e` for
  `commit_document`'s early return; `wrap.rs`'s `check.rs:1689-1694`). Six
  are new to the scan because L-U1+U2 moved `main`'s lines under them, and
  each names other code on `main`: `tests/tui.rs`'s `shell.rs:2015`
  (`Reason::ExternalDamage`; `main`'s line is `.saturating_add(...)`),
  `shell.rs:2703` and `tests/tui.rs:1191`, `smoke-tui.sh`'s `shell.rs:2597`
  (`editor.insert()`), `event_loop.rs`'s `shell.rs:2706` (`InsertNewline`),
  `shell.rs`'s `shell.rs:5816`. Re-pointing one to the line its old number
  maps to would name other wrong code; finding the code each meant is
  outside this train's merges.
- the rest are in historical plans under `docs/superpowers/plans/`
  (2026-09-10 and -11), which name the lines of their own day on every side,
  as train 4 left them.

## Join checks

### 1. FLAKE-HINT4's timer-window lint x L-U1+U2's scenarios (`lint.py`, `lint.txt`, `lint_osc.py`, `lint-osc.txt`)

`lint.py` (train 4's) runs the head's `capfile.races` -- the check
`test_sweep.py`'s `Capfile.test_no_snap_of_the_repos_scenarios_races_a_timer`
runs, in `test-sweep.sh` and CI, and that `sweep.py` refuses a scenario by --
over each commit's scenarios (`lint.txt`):

| commit | scenarios | races |
|---|---|---|
| `main` `884497c0` | 332 | 0 |
| FLAKE-COPYEND `0b432f9a` | 313 | 79, in 20: the 20 FLAKE-HINT4 converted (its base is `e6cde019`, before FLAKE-HINT4) |
| L-U1+U2 `e636db48` | 326 | 79, in the same 20; none in its 13 `p5-lifecycle-osc-*` |
| merge 1 `2fefa1fb` | 332 | 0 |
| merge 2 `7d91f833` | 345 | **0** |
| head | 345 | 0 |

**No violator, so nothing converted.** `lint_osc.py` asks the same of the 13
osc scenarios three ways, the lint unchanged (`lint-osc.txt`):

1. as the repository runs it: 0 races;
2. with every `wire <label>` read as a `snap <label>` (43 `wire` lines),
   since an osc scenario captures by `wire` alone and the lint counts a `wire`
   as 0.3 s spent, not as a capture that can race (`capfile.py`'s `SPENDS`):
   0 races. That blind spot is a fact of FLAKE-HINT4's lint, recorded here,
   not changed by a train (a change to the lint wants its own RED);
3. the negative control, so the lint is shown to read these files:
   `p5-lifecycle-osc-interruptnext` with full access added to both
   binaries' environment -- refused, 4 races (its `key Escape` lines inside
   the warning's window); and with a `key C-c`, `wait 2.5` and a `snap` after
   its first `wire` -- refused, 2 races (the Ctrl-C hint's window).

Why none races: neither timer is armed in any of the 13. The full-access
warning is armed at launch by a `PERMISSION_MODE` of full access (or a seeded
`permission_mode`); the only such lines are `states`' and `hostile`'s, and
each says `ask` (env and seeded settings). The Ctrl-C hint is armed by
`key C-c`; none of the 13 has one. And neither timer's text (`Full access`,
`press ctrl+c again to exit`) is in any wire L-U1+U2 recorded, fx's or xfx's
(`grep -l -a` over its 43 + 43 wires: none).

### 2. OSC 7501 x MCP-E (`mcp-status.txt`, `osc/mcp.txt`)

**No osc scenario uses MCP** (`grep -l -i mcp scripts/fxcap/scenarios/p5-lifecycle-osc-*`
at the head: no file, exit 1). So the question is answered from the code on
both sides and from one capture made for it.

- **xfx** (`Shell::status_activity`, `src/tui/shell.rs:10133`): `blocked`
  is an approval panel's or screen's request, a rule change's confirmation
  or the model's question; `working` is a turn's activity, a prompt waiting
  or a manual `/compact`. Every writer of those fields is in a turn's
  events or the user's own command (`mcp-status.txt` lists each line). What
  the MCP runtime reaches in the shell is `settle_mcp`
  (`shell.rs:7840-7862`), which writes each runtime notice and asks for a
  frame, and the status line's `McpStatus` -- neither is read by the program
  status.
- **fx** v0.0.16: `reportProgramStatus`'s `busy` is `stream.active or
  processing or queued_count > 0` (`app_worker_runtime.zig:763-765@481104fd`),
  the worker processes a `WorkItem`, a prompt or a context compaction
  (`worker_runtime.zig:167-170@481104fd`), and `app_mcp_runtime.zig` names
  none of the approval prompt, the question prompt, the worker's processing
  or the program status (0 matches).

So an MCP server's startup sets neither `working` nor `blocked` on either
side. **The capture** (`osc/mcp.txt`): `p5-lifecycle-osc-mcp`, MCP-E's
`p5-mcp-discover-stdio-modern` (a 2026-07-28 stdio server, served in that
era after `server/discover`) with a `wire` at launch, once the server has
started, after `/mcp show`, after the turn that calls its tool, and after
`/quit`; a scratch copy of the scenario directory
(`FXCAP_SCENARIOS`), the twin's text in `osc/scenarios/`. **On both binaries, at the head**
(`osc.py`, `osc/mcp.txt`: 5 wire pairs, 0 failing claims): `idle` at launch
and nothing else through the server's whole startup, the `until gone Full
access` and `/mcp show` (`m00-start`, `m01-started`, `m02-show`: one report,
`idle`, on fx and on xfx); then the turn that calls the server's tool,
`working`, `done` (`m03-turn`); then the exit's `clear` (`m04-exit`), with
no introducer after it. The `/mcp show` snap names the era on both
(`2026-07-28`). So the MCP era's startup and the program status meet without
a report of their own, as fx's code says they must. Of the 10 wires, the 4
taken before `/mcp show` are in the receipt (`osc/mcp/`); the 6 after it
carry the scratch path of the run's server command (`/mcp show` prints it)
and are not published (`osc/publish.txt` names each); osc.py's result over
all 10 is `osc/mcp.txt`.

### 3. TC-NOTATION's greys x L-U1+U2's frames (`osc/`)

What L-U1+U2 reads off a frame, in its tests and in `osc.py`:

- **`osc.py`** reads two words off frames: `Thinking` (a keystroke's
  `working` must follow the frame that shows it) and `Compaction failed` (an
  automatic compaction's `error` must come ahead of the frame that shows
  it). It sets every escape sequence aside before it looks (`SEQUENCE`, SGR
  included), so a `38;5` or a `38;2` spelling of a grey cannot change what
  it finds -- except by splitting a word, if a frame painted it a glyph at a
  time. `greys.py` asks that directly.
- **L-U1+U2's tests** read reports, frame brackets and words: the pty cases
  in `tests/tui.rs` (`STATUS_*`, `FRAME_BEGIN`/`FRAME_END`, `Thinking`),
  `smoke-tui.sh`'s emulator (`KNOWN_OSC`'s `7501`, the report shapes), and the
  unit tests in `program_status.rs`, `shell.rs`, `event_loop.rs`, `check.rs`.
  `git diff e6cde019 e636db48 -- src tests scripts` adds no line with `38;2`,
  `48;2` or `COLORTERM`; and since TC-NOTATION the palette is one, spelt
  `38;5` at every depth, `COLORTERM` read once, for the Ultra mark (train 4,
  `mcp-greys.txt`). The gate's two `cargo test` runs and smoke-tui run every
  one of them on the merged palette (Gate, below).

**Run at the head**, `d6a10ee` (`osc/capture.sh`, under the suite lock): fx
v0.0.16 and xfx on all 13 osc scenarios as they are (`osc/default/`), and
on their 13 `-tc` twins (`osc/tc/`; `osc/scenarios/*-tc.cap`, each the
scenario with `env COLORTERM=truecolor`, the form of `main`'s `-tc` twins),
both binaries both times, the head's harness, from a scratch copy of the
scenario directory with the twins added (`FXCAP_SCENARIOS`; no scenario of
the repository is added).

| set | `osc.py` | |
|---|---|---|
| `default`: the head's fx and xfx (`osc/default.txt`) | **43 wire pairs, 0 failing claims** | every xfx wire carries `v0.1.0-d6a10ee` and no other banner |
| the head's xfx against the fx wires L-U1+U2 recorded with its own harness (`osc/recorded-fx.txt`) | 43, 0 failing | the same reports, in the same places |
| `tc`: the `-tc` twins, fx and xfx (`osc/tc.txt`) | **43 wire pairs, 0 failing claims** | |

Every report sequence is L-U1+U2's GREEN table's, scenario by scenario
(`states` 16 reports ending `clear`, `interruptnext` idle, working, idle,
working, done, clear, and so on).

`greys.py` (`osc/greys.txt`), over each scenario's last wire, both binaries,
both environments: **0 direct colours (`38;2` or `48;2`) in any frame, on
either binary, in either environment** (xfx 30 to 229 SGR sequences a
scenario, fx 34 to 398); `Thinking` and `Compaction failed` are in the bytes
whole in every frame osc.py finds them in (e.g. `states` xfx 7 of 7 frames
with `Thinking`, `autocompactfail` 1 of 1 with `Compaction failed`), so no
frame paints either a glyph at a time; and the set of SGR sequences each
binary's frames use is the same under `COLORTERM=truecolor` as without it,
in every scenario. So no frame `osc.py` or L-U1+U2's tests read depends on a
`38;2` grey: there is none to depend on, at either depth.

### 4. The copy errand x the OSC report at a session's end (r2: `ffd53286`, `join-mutants.txt`)

Where each of the three comes, in the code at the head:

1. **The last frame and the last report** are the loop's.
   `tui::event_loop::run` (`src/tui/event_loop.rs:192`) runs the session and
   then `shut_down` (`:421`). Reports are written only by `commit_tick`
   (`:1133`, `take_checked_status` and `write_program_status`), which the
   session's ticks call (`session`, `:231`, through `commit_counted`,
   `:297`, `:1079`); `shut_down`'s drain draws through `drained` and
   `flushed`, which call `commit_frame` (`:838`): frames, no report.
2. **The copy's end** is the shell's drop. The `Shell` is made and owned in
   `tui::hold` (`src/tui/mod.rs:329`, `:481`) and dropped when `hold`
   returns, after `event_loop::run` and the MCP runtime's shutdown
   (`:504-524`). Its `copies` and `traces` each hold an `Errands`, whose drop
   is FLAKE-COPYEND's `end` (`src/tui/copy.rs:123-126`): the grace (1 s,
   `GRACE`, `copy.rs:48`), then every thread stopped and joined through the
   `Ending` guard; a copy's thread returns only once `Stop::settle` has
   killed and reaped its tool (`src/clipboard.rs` `run`, `settle`).
3. **The restore, with the program status clear in it**, is `term::shutdown`
   in `tui::session` (`src/tui/mod.rs:105`), after `hold` has returned
   (`:260`); L-U1+U2 put the clear inside the four restore literals
   (`src/tui/term.rs:97-113`).

Neither unit moved any of the three. FLAKE-COPYEND changed what `Errands`'
drop does inside (the old body became `end`, the guard added), not when it
runs; L-U1+U2 changed what the restore holds and where a tick writes its
report, not where the shell drops or the restore is written (its
`src/tui/mod.rs` change is a `clears_status: false` field and `mod
program_status;`, `git diff 884497c0 <head> -- src/tui/mod.rs`).

**Round 1's test was a sample, and round 2 replaces it.** Both reviews of
round 1 (`150334d9`) found that its order checks were looks taken while the
session ran: a session that exited before the next look skipped them
(`if let Some(gone)`), the first look that found the tool gone could already
hold a late frame, and it ran on `/trace`, whose file copy has its own 5 s
deadline (`clipboard.rs:121`, `:191`) beside the 1 s grace -- so its comment's
and this receipt's "no clock bounds anything" was false. Round 2's test
(`tests/tui.rs`
`a_copy_running_at_the_end_is_given_up_after_the_last_frame_and_before_the_restore`,
same name, on both platforms now):

- **`/copy`**, whose tool has no deadline (`copy_until` passes `None`,
  `clipboard.rs:220`) and is the same fake script on macOS (`pbcopy`) and
  Linux (`xclip`). The fake gains a hold mode
  (`tests/support/fake_clipboard.rs`, `hold_copies_on`): the tool takes what
  it is fed, then, until it is killed, writes a heartbeat (`<<BEAT>>`, one
  `write(2)` from the tool's own shell, about every 10 ms) into the pty's
  slave -- the stream xfx's output goes to.
- A reply, `/copy`, the tool holding and alive (its pid file, `kill -0`) and
  fed the reply; Ctrl-D; the session's exit; the tool dead. Then **every
  check reads the whole terminal stream once the session has exited**
  (`settled_text`: the reader joined, the pty drained), so none depends on
  when the test looks.

What each check proves, and what it can miss:

| check | proves | can miss / can falsely fail |
|---|---|---|
| nothing after the tool's last heartbeat but the restore's clear: no frame opener, no report | everything xfx writes after the copy's end comes after it. Every heartbeat is written by the tool before it dies; xfx writes nothing of the end until `Stop::settle` has reaped it; the pty keeps writes in the order they were made. So a frame or a report written after the copy's end, however soon or late, is after every heartbeat | misses nothing written after the reap. A frame written during the grace, while the tool still beats, is followed by heartbeats and not caught here. Needs a heartbeat after the last frame, which the 1 s grace gives: a tool starved of the CPU for the whole grace would fail the case, not pass it |
| no heartbeat after the restore began | the tool was dead before the restore was written | a restore written before the copy's end is caught only if the tool beats between the two. A misordered restore would come with the grace between it and the kill (1 s, about a hundred heartbeats); one written within the last heartbeat before the kill would not be caught |
| one restore; nothing after it but its own clear (no frame, no report) | the restore is last | -- |
| the tool alive at Ctrl-D, dead after the exit | the copy was running when the session ended (the tool never ends by itself) and the session ended it | -- |

Every wait in it is a bound (`WAIT`) on something that never comes -- the
session's start, the turn's end, the tool's start, the exit; no order is
read from a clock. The deterministic part is
the first row; the second is a sample, at the tool's heartbeat rate, of an
interval the code makes a second long. That is what the test proves.

**Does it discriminate, and does it depend on its observer?**
`join_mutants.py` (round 2), on a `git clone --no-local` of the train at
`ffd53286`, under one suite-lock hold (`join-mutants.txt`; the line each
failing run said names the check that caught it):

| mutant | where | caught by | observer delayed 3 s across the shutdown (the control) |
|---|---|---|---|
| J1: a second restore written before the shell drops | `tui/mod.rs`, `hold`'s return | the restore count | KILLED, the same check |
| J2: the end leaves its copies running (`Errands`' drop detaches the threads) | `tui/copy.rs` `Drop` | "the copy's tool outlived the session" | KILLED, the same |
| J3: a report written after the restore | `tui/mod.rs`, after `term::shutdown` | nothing after the last heartbeat (the restore itself is after it) | KILLED, the same |
| J4: a frame written right after the copy's end, before the restore -- no delay, flushed | `tui/copy.rs` `Drop`, after `end` | nothing after the last heartbeat | KILLED, the same |
| J5: a report written right after the copy's end, before the restore | the same place | nothing after the last heartbeat | KILLED, the same |
| J6: the one restore moved ahead of the copy's end (written in `hold`, none after) | `tui/mod.rs`, two places | "the copy's tool was alive after the restore began" | KILLED, the same |

**6 of 6 killed, and 6 of 6 again with the observer delayed**: the control
(gpt6's) inserts `std::thread::sleep(3 s)` right after the test's Ctrl-D,
so the session has exited before the test reads anything again; the
unchanged test passes with it, and every mutant is still caught. Round 2's
test has no looks to skip.

**The contrast** (the same file): round 1's sampled test at `150334d9`,
with the same 3 s delay after its Ctrl-D, passes unchanged -- and **lets
round 1's J1 and J4 through (SURVIVED, both)**: with its observer delayed,
its loop takes no look, `gone_with` stays `None`, and only its after-the-
restore checks run. That is the blind spot both reviews named, shown.

A first run of round 2's driver (`join-mutants-r2-first.txt`) had no J6;
there J1 was caught by the restore count, so no row showed the
no-heartbeat-after-the-restore check on its own. J6 does.

**On each unit alone**, the test's text and the train's fake clipboard
added unchanged: it passes on L-U1+U2 `e636db48` (the old `Errands` drop,
the clear in the restore) and on FLAKE-COPYEND `0b432f9a` (its `end`, a
restore with no clear). So the order -- last frame, the copy's end, the
restore -- is each unit's own and the train's.

Round 1's runs (`join-mutants-r1.txt`, `join-mutants-r1-first.txt`) and
driver (`join_mutants_r1.py`) are kept as they ran.

## Gates

### Round 2 (`gates-r2.txt`, `join-tests-r2.txt`; first run `gates-r2a.txt`, `join-tests-r2a.txt`)

`gates.sh` at (the next commit), the whole script inside one hold of the suite lock,
and in the same hold every test that uses the fake clipboard -- the join
test, the two `/copy` cases, the four `/trace` cases, the `/compact` case
with no credential, the slash-command walk (`tests/tui.rs`,
`tests/interactive.rs`), and fault injection's killed-report case -- three
times. 

| gate | result |
|---|---|
| (the next commit) | |

The first round 2 run, at `f788dc54` (`gates-r2a.txt`, `join-tests-r2a.txt`),
was the same in every row: 20 of 20, 5327 and 5364, `test-capture` 59 OK,
smoke-tui 1168, the touched tests 3 of 3 (load 79.8 at its start, other
workers' jobs, and 5 from step 16). Between it and (the next commit) the only change
outside this directory is `7318963f`, the join test's comment (`git diff
ffd53286 7318963f -- src tests` changes comment lines only).

The counts are round 1's: round 2's join test replaces round 1's (one
`#[test]`, 5241 at both heads, `test-counts.txt`), and on macOS both run.
The join test now runs on Linux too, where round 1's did not.

### Round 1 (`gates.txt`, `join-tests.txt`)

`gates.sh` (`gates.txt`): every step the brief lists, at `d6a10eeb` (the
receipt's first part committed on top of the join test `150334d9`), one
after another, `TMPDIR` a short scratch directory. **The whole script ran
inside one hold of the machine's suite lock** (`suite-lock.sh gates.sh ...`,
two heavy jobs from all the workers at once, the coordinator's rule,
2026-11-08), so every step ran under it and none beside another of this
train's own jobs; train 4 took the lock step by step for its four heaviest.
"tree clean: no" at the start and the end is this directory's untracked
files only (`osc/osc.py`, `osc/publish.py` and `test-counts.txt` were
written before the gate, the published wires while it ran); no tracked file
changed. The commits after `d6a10eeb` change only this directory
(`git diff d6a10eeb <head> -- src tests scripts docs Cargo.toml Cargo.lock
build.rs` is empty).

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| the same `--features fault-injection` | ok |
| `cargo test --locked --no-fail-fast` | **5327 passed**, 0 failed, 7 ignored (15 binaries) |
| `cargo test --locked --features fault-injection --no-fail-fast` | **5364 passed**, 0 failed, 7 ignored |
| `scripts/check-xfx-identity.sh` / `check-no-stubs.sh` | ok (13289 tracked files) / ok |
| `scripts/check-no-secrets.sh` | ok (13289 tracked files) |
| `scripts/check-preview-contract.sh` / `test-preview-contract.sh` | ok / ok (24 mutants caught) |
| `scripts/check-tui-preflight-cost.sh` | ok (200x300 full 1.20 ms, update 0.64 ms, replay 1.19 ms; budget 32 ms) |
| `scripts/check-usage-clock.sh` | ok |
| `scripts/test-check-no-secrets.sh` / `test-gate-env.sh` | ok (13 cases) / ok (5 cases) |
| `scripts/fxcap/test-sweep.sh` | 66 + 26 OK |
| `scripts/fxcap/test-capture.sh` | **59 run, 1 failed** in the gate; again under the lock right after: **59 OK**, and the failed case alone 5 of 5 OK |
| `cargo build --locked --release`; `--release --features fault-injection --target-dir target/faulty` | ok; ok |
| `scripts/smoke.sh target/release/xfx <dir>` | 47 checks, 0 failures |
| `scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx <dir>` | 37 scenarios and the oracle, **1168 checks**, 0 failures |

**The one failure, `test-capture.sh`'s
`Verbs.test_a_pid_whose_executable_is_not_the_binary_is_refused_and_the_guards_still_run`**
(`scripts/fxcap/test_capture.py:394`). The case writes a fresh wrapper
script whose body is `exec /bin/cat`, runs it as the "binary", `wait 1`,
then `signal SIGTERM`, and wants capture.sh's refusal to name the pid's
executable as `/bin/cat`. In the gate the refusal named `/bin/sh`: at 1 s the
fresh script's first start (which macOS assesses) had not reached its
`exec`. A wall-clock race in the harness's own test, against a stub binary
-- no xfx, no fx. Neither unit and no train commit touches it: `git diff
884497c0 <head> -- scripts/fxcap/test_capture.py scripts/fxcap/capture.sh
scripts/fxcap/test-capture.sh` is empty, so the case runs on the train
exactly as on `main`. It is not this train's to fix (a harness change wants
its own RED and review), and it is **not new**: FLAKE-COPYEND's r2 gate
(`flake-copyend/r2/fxcap-capture-flakes.txt`, "a new sighting") and
CTRLO-WIDER's run 2 recorded the same case failing the same way, and
GATE-GITENV-F's unlocked rerun did too. It has an owner: **LAUNCH-SETTLE,
item 6** ("test_capture.py:385 waits for the exec to land"), dispatched and
in flight; the program's status cites the case at `:385`, its line on
FLAKE-COPYEND's branch, and it is `:394` at this head (`main`'s train 4 grew
the file above it). **Run again** under the suite lock right after the
gate, at the same head (`gates.txt`, the reruns): `test-capture.sh` whole,
59 tests OK (222 s); the case alone through `test-capture.sh`, five times,
OK each time (load 8.3 to 8.7).

**Every side's tests are in** (`test-counts.txt`). `#[test]` attributes
under `src` and `tests`: `main` 5193; FLAKE-COPYEND +4 since its base
(5140 to 5144), at merge 1 5197; L-U1+U2 +43 (5140 to 5183), at merge 2
5240; the join test +1, 5241 at the head. `cargo test` counts 5327 against
train 4's 5279 at `main`'s code (train 4's gate): +48, the same difference;
with fault injection 5364 against 5316, +48. The harness's (`def test_`):
`test_sweep.py` 66 and `test_capture.py` 59 on `main` and at the head, as
neither unit changed them -- the counts the gate's two runs report.

The join test's lines in the gate's two cargo test logs are
`join-tests.txt`. With the receipt in its final form staged: `cargo fmt
--check`, `check-xfx-identity.sh`, `check-no-stubs.sh` and
`check-no-secrets.sh` again (`gates-final.txt`).

## Sweep

**Builds** (debug, as train 4's): the head `d6a10eeb` (`x𝒇x v0.1.0-d6a10ee
[debug]` on row 1 of every xfx snap, sha256 `1dbb3c3e…`; the receipt commits
after it change no compiled file), built in the train's worktree; `main`
`884497c0` (sha256 `b9242e06…`), built in a `git clone --no-local` scratch
clone. fx: the v0.0.16 binary, sha256 `8e6cda2e…`, `FX_AUTO_UPGRADE=0`,
checked by `capture.sh` around every run. `scripts/fxcap/sweep.sh --fx <fx>
--pin-sha 8e6cda2e… --xfx <bin> --jobs 2 --size 40x120 --only ...`, each
sweep inside one hold of the suite lock.

**Each binary with its own tree's harness**, as train 4: the head's sweep
from the train's worktree, `main`'s from its clone (`main`'s scenarios,
harness and ledger). `main`'s sweep and L-U1+U2's were then rescored with the
train's ledger (`sweep.sh --rescore`, no capture taken), so a verdict that
moves moved because a capture did.

**Selection, 58 scenarios** (`selection.txt`, 45 globs, each with why):
- every `p5-lifecycle-osc-*` (13): the only scenarios either unit added or
  edited -- FLAKE-COPYEND added and edited none
  (`git diff --name-only e6cde019 0b432f9a -- scripts` is empty);
- the copy and clipboard scenarios: every `.cap` that types `/copy` or
  `/trace` or names the clipboard or `osascript` (`copy`, `recall-edge`,
  `p5-panels-sweep-a`, `-b`, `trace`, `trace-nowhere`, `titles-idle`,
  `-rename`, `-steer`, `image-paste`);
- the scenarios of every ledger entry the re-points touched:
  `inline-code-shade` (its 11 scenarios), `submit-multi-card-whole`
  (`p5-transcript-submit-multi`), `transcript-page-offset`,
  `toolgroup-ctrlo-record-short`, `safe-page-edge`, `full-transcript-shift`
  (their 17). `session-record`, `stats-bytes` and the `banner` normalization
  name every scenario; a re-point changes a line number the sweep does not
  read, so they are not swept whole, as train 4 said of the same three;
- the nine scenarios L-U1+U2's own sweep moved against its base (its
  `gates.txt`, "the parity sweep": `image-composer`, `p5-effort-fork-fork`,
  `rules-read-auto`, `interrupt-retry`, `model-column-reask`, `model-parked`,
  `model-reask`, `p5-input-model-fail`, `resume-paging`), so this sweep
  answers each of those moves again.

The osc scenarios take no snap (`wire` only): they run in the sweep and add
no pair; `osc.py` compares their wires (join check 3). So the 58 scenarios
at the head and `main`'s 45 (no osc scenario on `main`) take the same 300
pairs.

**Result at the head** (`sweep-head.md`): 58 scenarios ran, **0 failed, 0
timed out**; 300 pairs, **166 pass** (55.3%: 13 identical, 153
normalized-identical), 2 approved divergences and 1 cascade, 131 unresolved;
wall 26m04s, load 6.7 at its start and 21.7 at its peak (other workers'
jobs).

### Against `main`

`main`'s sweep (`sweep-main.md`): 45 scenarios, 0 failed, 0 timed out; 300
pairs, 165 pass (rescored with the train's ledger; 165 as it scored too).
`compare-main.txt` (train 4's `compare.py`), pair by pair over the 300:
**165 → 166 pass; 1 verdict moved, up; 0 down** ("passed on the reference,
not on the train: 0").

| pair | `main` → train | what moved it |
|---|---|---|
| `model-column-reask` `col-back` | unresolved → normalized-identical | **timing, the harness's catalog clock**: `modelsdown 6000` answers the models request 503 for 6 s from the fake gateway's start. In `main`'s run fx asked at 1.07 s and 6.62 s, both 503, and its column read `unable to load models`; in the head's fx asked at 4.43 s (503) and 9.98 s (200) and showed the list xfx showed. A pass on the head's side; the same class as below |

Rows that moved without a verdict move (`compare-main.txt`, "differing
rows"; `screen-moves-main.txt`, 152 of 207 moved snaps only in clock-like or
run-local text):
- the `/stats` row's `ansi_bytes=` on xfx (`p5-panels-sweep-a` ×13,
  `p5-panels-sweep-b` ×7, `recall-edge` ×3): unresolved on both sides
  (`stats-bytes`, every scenario, owner unowned). It moved by 27 bytes where
  the session had run no turn before `/stats` (`655` → `682`, `675` →
  `702`): the launch's `ESC ] 7501 ; state=idle:app=xfx ESC \`, 27 bytes,
  which L-U1+U2 writes and xfx counts as it counts every byte it writes;
  by more where turns ran (`p5-panels-sweep-b` `stats` `369` → `506`).
- `model-parked` `menu-loading`, `model-column-reask` `col-down`: the
  catalog clock again (below).
- the scratch path in `read-failures` `live-ctrl-o-up1` and in
  `p5-effort-fork-fork` `status` (a fresh run directory per run), resume
  ids (`fulltranscript`, `resume-tools`, `rules-read-auto`, `resume`,
  `resume-launch`, `p5-effort-fork-fork`), the banner's build (`reset`,
  `reset-wire`), and `p5-panels-sweep-a`'s own rows (train 4 found the same
  rows moving between FLAKE-HINT4's base and head runs).

### Against each unit's own sweep

**L-U1+U2**: its own head sweep (`75f70b03`, every scenario, 1692 pairs;
its five timed-out scenarios replaced by its own locked reruns, as its
README did; rescored with the train's ledger: `sweep-unit-lu.md`) against
the train's, over the 300 pairs both took (`compare-lu.txt`): **166 → 166
pass; 6 verdicts moved, 3 up, 3 down.**

| pair | L-U1+U2 → train | what moved it |
|---|---|---|
| `p5-toolgroup-ctrlo-tc` `dark`, `light`, `dark-again` | unresolved → normalized-identical | TC-NOTATION, on `main` since train 4 and not on L-U1+U2's base (train 4's README: these three among its 25) |
| `model-parked` `menu-loading` | normalized-identical → unresolved | **the reference's timing**: `modelsdown 6000` answers the models request 503 for 6 s from the fake gateway's start. In L-U1+U2's run the first ask came 34.3 s (fx) and 33.6 s (xfx) after it -- the launch itself late, on a machine at load 31 -- so neither binary saw the outage the scenario is about and both menus listed `Models 4`. At the head both asked inside it (0.76 s, 0.83 s; 503): `Loading models…`, `Models 0` on both, the parked turn's activity row then differing (fx `Thinking`, xfx `Generating (↑4 ↓3)`). Unresolved on `main` too, where fx asked inside the window (3.47 s) and xfx after it (11.72 s) |
| `model-reask` `menu-failed` | normalized-identical → unresolved | the same clock: in L-U1+U2's run fx's first ask came at 13.7 s (200) and xfx's second at 6.02 s (200), both screens `Models 4`; at the head fx asked at 5.23 s and xfx at 1.24 s and 3.77 s, all 503, and with the outage seen fx says `Could not reach AI Gateway; fx retries automatically.` and xfx `…; retry /model.` -- xfx does not ask the catalog again by itself, the gap train 4 named MODELS-REASK. The same rows on `main` (fx 1.08 s, xfx 0.75 s and 3.27 s, 503), unresolved |
| `p5-input-model-fail` `fail-open` | normalized-identical → unresolved | the same, `modelsdown 15000` (this scenario takes no gateway log): in L-U1+U2's run both screens at `fail-open` read `Models 8`, the outage already over; at the head and on `main` both read `Models 0`, fx `…fx retries automatically.`, xfx `…retry /model.`. Unresolved on `main` too |

**The deciding cell** (`models-cell.txt`): L-U1+U2's own xfx (sha256
`b47a4e46…`, the binary of its sweep) under the train's harness and
scenarios, under the suite lock, on the three, and the head's xfx right
after it in the same hold. Both binaries asked inside the outage window this
time (fx 3.09 to 3.76 s, xfx 2.25 to 4.99 s, each 503), and **all three
pairs are unresolved on both builds, with the head's rows exactly**
(`Models 0` and `Loading models…` on both sides of `model-parked`; fx's
`…fx retries automatically.` against xfx's `…retry /model.` on the other
two).

So no pair that passes on `main` fails on the train, and the three that
passed on L-U1+U2's own run and not here passed there because its catalog
asks came after the harness's fixed outage window, not because of its code.
The clock is the harness's (`modelsdown <ms>`, from the fake gateway's
start), the class FLAKE-HINT4 removed from `models2` and left here:
named for its own unit, **FLAKE-MODELSDOWN** (the outage ends by order --
after the scenario's first `/model` is answered -- not after a fixed time).

**FLAKE-COPYEND: no sweep of its own.** Its receipt has gates and no sweep
(its two rounds' `README.md`: "Gates", no "Sweep"), and nothing a sweep
reads changed: no scenario, no harness file (`git diff --name-only e6cde019
0b432f9a -- scripts` is empty), no drawing code -- its code is
`src/tui/copy.rs`'s `Errands` (the `end` method, the `Ending` guard, the
`Drop`, `copy.rs:82-128`), and everything else it changed in
`src/clipboard.rs`, `copy.rs` and `src/tui/trace.rs` is under
`#[cfg(test)]` or `#[cfg(all(test, unix))]` (its README, round 2). The copy
and clipboard scenarios above are its sweep: against `main` none of their
pairs moved a verdict (`compare-main.txt`).

**Screens that moved without a verdict** against L-U1+U2
(`screen-moves-lu.txt`, 127 of 193 only in clock-like or run-local text):
the catalog clock (`model-parked`, `model-reask`, `p5-input-model-fail`,
above), the banner's build, resume ids and scratch paths, the `/stats`
counters, `p5-panels-sweep-a`'s own rows, and `p5-panels-sweep-a` `shell`
(`retrying request in 1s` against the next frame's `✗ system: … HTTP 500`,
a retry's notice caught a moment apart).

`until` lines (`until-waits.txt`, every one in the two sweeps' capture logs,
22): `p5-transcript-records` and `-resize`'s `until shown Full access`
returned 0.01 to 0.07 s after their line, `until gone Full access` 4.06 to
4.12 s after launch; `p5-panels-sweep-a`'s `until shown 'No context to
compact.'` 0.01 to 0.07 s after its line.

## Redaction (round 2)

opus found 8 lines of `compare-lu.txt` (1841-1847) and `compare-main.txt`
(116-119) holding the tail of a scratch path (a run directory's name and
what follows it, the root cut off). The cause was this receipt's own
`compare.py`: it printed each differing row's last 70 characters, and the
caller piped that through `redacted.py`, so a row holding a local path could
be cut inside the path, leaving a tail with no root for the redactor to
recognize. `screen_moves.py` cut rows the same way (60). Both now redact
each row whole (`redact.Redactor`, every path under a local root to
`<local-path>`) before cutting it, and the four files were written again
from the same sweeps: the 8 lines now read `"path":"<local-path>"`.
`scripts/fxcap/redact.py` itself is unchanged: catching a root-less tail
after the fact would take a guess at what is a path in screen text, a
harness change of its own; the fix is where the cut is made.

Then, over every file of this receipt (wires included, `grep -raoE`), for
the four forms the coordinator named -- the uid fragment before a slash,
the machine's scratch root's name, the system temporary root and the home
root, each spelt with a bracket class (`50[1]`, `privat[e]`, `User[s]`) so
the command matches what it looks for and not its own text: **0 hits**
(`redaction-grep.txt`). The paths of that kind already in other receipts on
`main` belong to LOCAL-PATHS and are left.

## Files

- `remerge-1.txt`, `remerge-2.txt`: `git show --remerge-diff` of each merge.
- `overlap.py`, `overlap.txt`: every file both sides of a merge changed, and
  the nearest pair of hunks.
- `token_check.py`, `token-check.txt`, `token-self-test.txt`: train 4's
  token check with the rows' order added, and its negative control with the
  per-row cases added.
- `refs.py`, `refs.txt` (merge 2), `refs-head.txt` (the head): the reference
  map, three sides; `refs_all.py`, `refs_short.py`, `refs-all-merge2.txt`
  (merge 2), `refs-all.txt` (the head): the same over every tracked file
  outside `.prd/`, both forms; `repointed.py`, `repointed.txt`: train 5's 9
  re-points checked against their code; `stale.py`, `stale.txt`: the nine
  references still flagged outside the plans, each beside the code it names
  on `main`.
- `lint.py`, `lint.txt`: FLAKE-HINT4's lint over each side's scenarios and
  the train's; `lint_osc.py`, `lint-osc.txt`: the same over the 13 osc
  scenarios, widened to `wire`, with its negative control.
- `mcp-status.txt`: what can set `working` or `blocked` on each side, and
  what an MCP server's startup reaches.
- `osc/`: `scenarios/` (the 13 `-tc` twins and `p5-lifecycle-osc-mcp`,
  captured from a scratch copy of the scenario directory, not scenarios of
  the repository); `capture.sh` (the captures), `publish.py` (the wires into
  the receipt, refused where a local path is in one), `osc.py` (L-U1+U2's
  checker, `m04-exit` added to `ENDS`), `greys.py` (TC-NOTATION's greys
  against the frames osc.py reads); `default/`, `tc/`, `mcp/` (each `fx/`
  and `xfx/` wires); `default.txt`, `tc.txt`, `mcp.txt`, `recorded-fx.txt`
  (osc.py's runs), `greys.txt`, `publish.txt` (what was published and the
  six wires that were not).
- `join_mutants.py`, `join-mutants.txt`: round 2's join test against five
  mutants, the same with its observer delayed across the shutdown (the
  control), on each unit alone, and round 1's test delayed the same way;
  `join_mutants_r1.py`, `join-mutants-r1.txt`, `join-mutants-r1-first.txt`:
  round 1's driver and its two runs (the first with J4 unflushed).
- `join-tests.txt`: the join test's lines in round 1's gate's cargo test
  logs; `join-tests-r2.txt`, in round 2's final gate's, and the touched
  tests' three runs; `join-tests-r2a.txt`, the same for round 2's first run;
  `test-counts.txt`: the test counts per commit, with their commands;
  `until-waits.txt`: every `until` line of the swept scenarios that have one.
- `gates.sh`, `gates.txt`: round 1's gate as it ran, and the test-capture
  reruns after it; `gates-r2.txt`: round 2's final gate and the touched tests' runs;
  `gates-r2a.txt`: round 2's first run, at `f788dc54`; `gates_txt.py`: how `gates.txt` was assembled; `gates-final.txt`:
  the file checks with the receipt in its final form.
- `selection.txt`: the sweep's `--only` globs, each with why.
- `sweep-head.md`, `sweep-main.md`: the two scoreboards (`main`'s rescored
  with the train's ledger); `sweep-unit-lu.md`: L-U1+U2's own head sweep
  with its locked reruns swapped in, rescored; `compare.py`,
  `compare-main.txt`, `compare-lu.txt`: pair by pair.
- `screen_moves.py`, `screen-moves-main.txt`, `screen-moves-lu.txt`: the
  screens that moved, clock-like and run-local text set aside.
- `models_cell.py`, `models-cell.txt`: the three pairs that passed on
  L-U1+U2's own sweep and not here, in five sweeps (its own, `main`'s, the
  head's, and the deciding cell's two), each binary's catalog asks and what
  its screen says.
- `until_waits.py`, `until-waits.txt`: every `until` line in the two sweeps'
  capture logs.
- `redacted.py`: every sweep-derived file here went through
  `scripts/fxcap/redact.py`'s `Redactor.write`; `redaction-grep.txt`: the
  round 2 grep over this directory, its command and its count.
