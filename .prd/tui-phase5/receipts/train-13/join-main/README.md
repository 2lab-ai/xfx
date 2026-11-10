---
title: 0.0.16 delta -- P5 train 13 joins main (PR #90, train 12)
type: receipt
tags: [xfx, phase5, fx-0.0.16, train, merge]
---

# Train 13 joins main: PR #90, one merge, four files of reference numbers

Train 12 merged to main as PR #90: `origin/main` is `d2b2ca75`
(committed 2026-11-10 15:28:57Z, `git log`), its second parent train 12's head `208a8dd4`.
Train 13 (`train/p5-1110b` at `afd891af`) was built on `f53606d1`, train
12's head before its CI rounds and its join of PR #91; `f53606d1` is the
merge base. Main brought train 12's CI rounds (the sweep job's socket path
and its 120-minute limit, `p5-panels-badsettings` restored, the MCP menu
scenarios' held remote), PR #91's preview 0.2.0 (`XFX_BUILD_ID` in place
of `XFX_BUILD_REVISION`; the wrapped-banner reader, `cells.join_wrapped`
and `capfile.Reading.cols_at`), and train 12's join of it. The program goal
(the user's words): «최신 fx랑 비교해서 ui쪽으로 모든 기능 구현하고 기본적으로
최신 fx ui를 그대로 구현해줘» (fx v0.0.16, pin `481104fd`).

| commit | what |
|---|---|
| `8adc57df` | `git merge --no-ff origin/main`, parents `afd891af` (train) and `d2b2ca75` (main). Four files conflicted, 15 blocks, every one a reference number |
| `896a11c0` | the 19 references the merge moved, re-pointed; numbers only (`repoint.py`) |
| `0dbfaf39` | the ledger's `nondeterminism` cause for `p5-mcp-fail` / `list`. **The code head: the gate and the re-runs ran here** |
| this commit | this folder, and train 13's README's section on the moved code head |

## The conflicts (`blocks.py`, `reproduce.sh` / `reproduce.txt`)

`git merge-tree` forecast four: `capfile.py`, the ledger, `smoke-tui.sh` and
`shell.rs`; `docs/parity.md` auto-merges. The merge, in diff3 style, had
those four and 15 blocks. In every block the three sides are the same once
each `:N` / `:N-M` is masked: both sides re-pointed references, each to its
own branch's lines (train 13 in `e1e15efd`, train 12 in `c7d30afc`). So each
number is merged on its own (`blocks.py`): the side that moved it wins; a
number both moved keeps ours', and `896a11c0` moves it to the merged code.
That is train 13's `numbers` rule, read token by token, so a number only
one side moved is that side's at the merge.

| file | blocks | each number: base / ours / theirs -> merged |
|---|---|---|
| `scripts/fxcap/capfile.py` (TIMERS, the yolo warning's comment) | 1 | `:15057-15082` / `:15194-15219` / `:15059-15084` -> ours; bare `:15075-15077` / `:15212-15214` / `:15077-15079` -> ours |
| `scripts/fxcap/parity-ledger.toml` | 12 | 13 `src/tui/shell.rs` numbers, both moved, ours kept: `:13866`/`:13962`/`:13864`, `:16840`/`:16977`/`:16842` twice, `:41113`/`:42431`/`:41165` six times, `:10480`/`:10558`/`:10478` twice, the bare `:1747`/`:1929`/`:1745`, `:4541`/`:4557`/`:4539` |
| `scripts/smoke-tui.sh` (`scenario_26`) | 1 | `shell.rs:2705` / `:2705` / `:2703` -> theirs; `input.rs:1063` / `:1152` / `:1063` -> ours |
| `src/tui/shell.rs` (a test's comment) | 1 | `shell.rs:6070` / `:6086` / `:6068` -> ours; bare `:6258` / `:6274` / `:6256` -> ours |

`blocks.py` prints that line for each block; `reproduce.sh` redoes the
merge in a clone with it and gets `8adc57df`'s tree, "no difference"
(`reproduce.txt`). Before the commit the staged tree passed `cargo check
--all-targets` with and without `fault-injection`, `cargo fmt --check`,
clippy both (train 13's `check-merge.sh`, `check-merge.txt`) and the
ledger's load. `capfile.py`'s other changes are main's alone and merged as
git merged them: `RESIZE` and `Reading.cols_at`. `cells.join_wrapped` is in
`cells.py`, which the train did not change; LOCAL-PATHS is in the base.

## The merge, from both ends (`merge-ends.txt`, `functions.txt`, `rows-ends.txt`)

- **`merge_ends.py`** (train 12's, a residual and a masked reading added):
  nine files both sides changed. Six read true both ways as git or the
  token rule merged them: `docs/parity.md`, `smoke-tui.sh`, `src/app.rs`,
  `src/interactive.rs`, `tests/sessions.rs`, `tests/tui.rs`. The three
  that hold a number both moved read false, and the residual is exactly
  those lines: `capfile.py` its 2, `shell.rs` its 2, the ledger its 12
  (eight distinct texts). With every number masked all nine read true both ways.
  The 1390 files one side alone changed (1211 the train's, 179 main's) are
  that side's bytes: 0 differ.
- **`functions.py`**, each code unit a code file's block sat in, read whole
  against both parents: `shell.rs`'s
  `a_screen_too_small_for_the_question_cancels_it_rather_than_painting_half`
  is ours' (0 lines differ) and theirs' but for its two numbers;
  `smoke-tui.sh`'s `scenario_26` differs from each parent by the one line
  holding the other side's number (2 lines each). Both the same as each
  parent and the base once masked.
- **`rows_ends.py`** (train 12's, the ledger read entry by entry):
  `docs/parity.md`'s 157 row keys, 7 the train's, 4 main's, none both
  sides', each as the side that changed it left it. The ledger's 100
  entries: 4 the train's, 13 main's, 11 both -- each of the 11 `numbers`,
  the same from both ends once masked, its merged numbers ours', and none
  else. Main removed `xfx-debug-banner-wrap`; it is gone at the merge.

## The references (`refs*-pre.txt`, `refs*-head.txt`, `content-*.txt`, `repoint.py`, `repointed.py`)

Train 13's checks, unchanged (`../refs*.py`, `../refs_content.py`), with
`train=afd891af main=d2b2ca75`, at the merge (`-pre`, `content-8adc57df.txt`)
and at the code head (`-head`, `content-head.txt`); `content-afd891af.txt`
and `content-d2b2ca75.txt` are the content check on each parent.

**At the merge**, outside the historical plans (`docs/superpowers/plans/`):
`refs.py` 5 references MOVED (13 ledger numbers), `refs_all.py` 9,
`refs_short.py` 1, `refs_bare.py` 3, `refs_prd.py` 0. The content check
reads WRONG: the ledger 14, `capfile.py` 1, `shell.rs` 1, `tests/tui.rs`
6, `smoke-tui.sh` 5, `docs/parity.md` and `tests/sessions.rs` 0.

**19 re-pointed in `896a11c0`, numbers only** (`repoint-plan.txt`): every
reference those scans flag MOVED but one, each moved by the other side's
lines and each WRONG in the content check where it reads it:
- 17 of the train's into `src/tui/shell.rs`, moved by main's lines: the
  ledger's 13 (the twelve blocks' numbers), `capfile.py`'s
  `:15194-15219` and its bare `:15212-15214`, and `shell.rs`'s own
  `shell.rs:6086` and bare `:6274`. Their frame is `e1e15efd`, train 13's
  re-point, which wrote each of them; `:1929`, which train 13 set by name
  (`queue_card`), is `queue_card` there, so it maps like the rest
  (`:1927`).
- 2 of main's in `tests/tui.rs`, moved by the train's lines:
  `src/tui/shell.rs:16806` -> `:16943` and `:9667` -> `:9736`. Train 12 set
  them by name in `2e388ca8` (`fn blind`, `decide`'s Redraw arm, its join
  README); that commit is their frame, and each maps to the same code.
- Stays: `test_sweep.py`'s `scripts/fxcap/cells.py:100000`, a line past
  the file's end on purpose (the case that such a reference is refused).

Each is mapped from its frame through `git diff -U0`, and the frame's cited
line and two either side stand at the new number. **`repointed.py`: 19 of
19 the same code; `896a11c0` 18 lines out, 18 in, each the same once its
numbers are masked** (`repointed.txt`, read at `896a11c0`: the cause after
it inserts 14 ledger lines above one of the plan's citing lines, `:1083`). No
reference is set by name.

**At the code head**: `refs.py` 0 MOVED, `refs_prd.py` 0; outside the
historical plans `refs_all.py` 1 (`cells.py:100000`), `refs_short.py` 0,
`refs_bare.py` 0. The content check, blame ignoring the re-point and as it
is: the ledger 1, `smoke-tui.sh` 5, `tests/tui.rs` 4, the rest 0. Those 10
are WRONG on both parents already, the same 10 (`content-afd891af.txt`,
`content-d2b2ca75.txt`), and no line of the join wrote them: the ledger's
`.prd/07-fx-parity-program.md:19` (its text is at no place), the five in
`smoke-tui.sh` train 13 lists (`../README.md`, "Found on the way"), and four
old ones in `tests/tui.rs` (lines 591, 2404, 2483, 3129). They are left as
the two sides have them. So are the two in `smoke-tui.sh`'s conflicted
block, which train 13 found naming the wrong lines on both sides (the same
section; the content check reads them `SAME`, its blind spot):
`shell.rs:2703` (main's number) holds `pending_cut: None,` at the head as on
main, and `input.rs:1152` (the train's) a doc comment, as on the train;
`0x1f => Action::Undo` is `input.rs:1347`. The join neither moved nor fixed
them; owner REF-LINT, as train 13 names it.

## Join checks (`join_checks.sh`, `join-checks.txt`, parts 1 and 2)

1. **`XFX_BUILD_REVISION`** outside `.prd`: 8 lines on the train (the
   base's build.rs, preview.yml, `src/lib.rs`, the contract and gate-env
   scripts), 1 on main, 1 at the head -- `CHANGELOG.md:40`, main's line
   saying it is gone. Every file that read it is main's at the head; the
   build id is `XFX_BUILD_ID` (19 lines outside `.prd`).
2. **`scripts/check-no-local-paths.sh`**: ok at the code head (the gate,
   below).
3. **`p5-mcp-fail` / `list`'s timing is unchanged by the join.** The
   files that time it -- the scenario, its profile, the stdio server,
   `fakegw.ts`, `src/mcp/runtime.rs` and `src/tui/mcp_menu.rs` -- are the
   same at the head as on main, the train and the base; `capture.sh` is
   main's (the train did not change it). The train changed no file under
   `src/mcp`, no `src/tui/mcp_*.rs` and none of the harness's scripts.
   Measured: the head's release build, three runs of the scenario
   (`mcpfail.sh`, the dispatcher's re-run's head side), row 6 of `list`
   reads `› bad retrying profile` 3 of 3 (`mcp-fail-head-reruns.txt`), as the dispatcher's
   re-runs read it on train 12's head and main's base.

## The ledger cause (`0dbfaf39`; `mcp-fail-ci-sweep.md`, `mcp-fail-reruns.txt`)

PR #90's CI sweep counted `p5-mcp-fail` snap `list` a regression: fx and
the base's xfx (`8f921425`) drew row 6 `› bad connecting profile`, the
head's (`208a8dd4`) `› bad retrying profile`, one run each
(`mcp-fail-ci-sweep.md`, the job's summary, copied). The dispatcher's local
re-runs read `retrying` 3 of 3 on the head and 3 of 3 on the base
(`mcp-fail-reruns.txt`). The scenario draws the list about 15 s in
(`wait 13`, `/mcp`, `wait 2`), on the server's try near 15 s (tries near 1,
3, 7, 15 and 31 s, `src/mcp/runtime.rs:90`); xfx's row reads `connecting`
while a try runs and `retrying` between (`src/mcp/runtime.rs:1336`,
`:1350`).

`mcp-fail-list-on-a-try`, placed after `verbs-refused-port-word`, the
ledger's other MCP status-word cause: `kind = "nondeterminism"`, `owner =
"scenario"`, `scenarios = ["p5-mcp-fail"]`, `snaps = ["list"]`, `fx_row` and
`xfx_row` `^\W*bad\b.*\b(?:connecting|retrying)\b`, a `fix` (draw the list
between the tries at 7 and 15 s) and a `why`. That one is `scenario`, not
`nondeterminism`, because no re-run can pass its pairs; this pair passed on
the base, so the up to three re-runs on each build that `ci_sweep.py` gives
a pair a `nondeterminism` cause covers can pass it. Checked with the
ledger: it loads, covers row 6 of `p5-mcp-fail` / `list` on both sides'
text, and no other snap or scenario; `test-sweep.sh` passes.

## Gate (`gates.sh`, `gates.txt`)

`gates.sh` (train 13's; ci.yml did not change since train 13's base, and
sweep.yml only its timeout) ran at `0dbfaf39`, in a `git clone --no-local`
of it, the whole script inside one hold of the machine's suite lock
(`suite-lock.sh bash gates.sh`), `TMPDIR` the harness scratch root, one
step after another, each step's exit from `$?` on the line after its
command, 2026-11-10 15:46:39Z to 16:40:50Z. **22 steps, 0 failed**; the
clone clean before and after (`gates.txt`, each step's last lines through
`../gates_txt.py`; `gate-counts.txt`, `../gate_counts.sh`'s counts):

| step | result |
|---|---|
| `cargo fmt --check` | exit 0 |
| `cargo clippy --locked --all-targets -- -D warnings` | exit 0 |
| the same with `--features fault-injection` | exit 0 |
| `cargo test --locked --no-fail-fast` | 15 test binaries: 5699 passed, 0 failed, 7 ignored (lib 4775) |
| `cargo test --locked --features fault-injection --no-fail-fast` | 15 test binaries: 5736 passed, 0 failed, 7 ignored (lib 4798) |
| `check-xfx-identity`, `check-no-stubs`, `check-no-secrets` | exit 0 each |
| `check-no-local-paths` | ok, 20128 tracked files scanned |
| `test-check-no-local-paths`, `test-check-no-secrets`, `test-gate-env` (64 cases) | exit 0 each |
| `check-preview-contract`, `check-tui-preflight-cost`, `check-usage-clock` | exit 0 each |
| `test-preview-contract` | ok, 28 mutants caught; the unbroken copy passes |
| `scripts/fxcap/test-sweep.sh` | 76 + 28 tests, OK |
| `scripts/fxcap/test-capture.sh` | 108 tests, OK |
| release build, fault-injection release build | exit 0 each |
| `scripts/smoke.sh` (release build) | 47 checks, 0 failures |
| `scripts/smoke-tui.sh` (release build, `--faulty` the fault-injection one) | 37 scenarios + the oracle, 1167 checks, 0 failures |

Against train 13's gate at `e1e15efd` (`../gate-counts.txt`: 5685 and
5722), each suite has 14 more: main's. The tracked `#[test]` lines are 5602
on the base, 5633 on the train, 5616 on main and 5647 at the head, the
base's plus each side's (`join-checks.txt`, part 3); the join adds none.

## Local paths (`pathcheck.sh`, `pathcheck.txt`)

The repository is public. Each commit of the join was made behind `set -o
pipefail` and an explicit `if` on the brief's check, `git diff --cached |
grep -E '^[+]' | grep -cE <the brief's pattern>`, which printed 0 each time,
and on train 13's `shapes.py` over the same lines and over this folder
(`pathcheck.sh`). `pathcheck.txt` holds the runs before the three code
commits; this commit's own run, and `check-no-local-paths.sh` over its
staged files, are in the dispatcher's report, as a commit cannot hold its
own check. During the merge
the staged lines are the index against the train's head, 16793 lines with
the headers: 0 hits. `shapes.py`'s run-name shape matches 8 of them, each
an added line of main's own (`git diff HEAD MERGE_HEAD`): seven CI runners'
run-directory names under `<tmpdir>` in train 12's `ci-socket/` receipts,
and a made-up suffix of six `A`s in its `ci-mcp-timing/publish_cost.sh`;
`pathcheck.sh` lists them and gates on the rest, 0. Every file here
taken from a log, a clone or another folder went through `../redacted.py`.
The two evidence files came through it unchanged.

## Files

- Merge: `blocks.py`, `reproduce.sh` / `reproduce.txt`, `check-merge.txt`
  (train 13's `check-merge.sh`).
- Both ends: `merge_ends.py` / `merge-ends.txt`, `functions.py` /
  `functions.txt`, `rows_ends.py` / `rows-ends.txt`.
- References: `refs*-pre.txt` (at `8adc57df`), `refs*-head.txt` (at
  `0dbfaf39`), `content-afd891af.txt`, `content-d2b2ca75.txt`,
  `content-8adc57df.txt`, `content-head.txt`; `repoint.py` /
  `repoint-plan.txt`, `repointed.py` / `repointed.txt`.
- `checks.sh` writes all of the above but `check-merge.txt`;
  `join_checks.sh` / `join-checks.txt`.
- The cause: `mcp-fail-ci-sweep.md`, `mcp-fail-reruns.txt`; `mcpfail.sh` /
  `mcp-fail-head-reruns.txt`.
- `gates.sh`, `gates.txt`; `pathcheck.sh`, `pathcheck.txt`.
