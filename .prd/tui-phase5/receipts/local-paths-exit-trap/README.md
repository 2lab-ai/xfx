# local-paths-exit-trap: a gate script that stops early fails

Unit LOCAL-PATHS-EXIT-TRAP. Base `origin/main` `dcfa5146`. Found in
LOCAL-PATHS-SELFTEST-FLAKE (PR #95, `b0fd5157`): under bash 3.2 a script
stopped by an error of its own leaves through its EXIT trap with status 0.
This unit proves which of the other scripts with an EXIT trap can fail open
that way, fixes them, and holds each to it. Every claim below is tied to a
file in this folder.

| commit | what |
|---|---|
| `47837c3c` | the guard in seven scripts; the signal traps gone; the early-stop cases; `test-early-stop.sh` in CI -- **the code** |
| `6071c6f8` | the `exit 7` control in every case; check-no-secrets.sh's late stop and its TMPDIR -- **the gate head**: every run below is at it |
| `5b20b9d6` | comments only: three said which bash CI runs, which no CI log shows -- **the code head** (`head-delta.txt`: no line but comments changed; the quick steps rerun there) |
| `ef4e74ea` | round 1's README and records |
| `967581c9` | round 2: one BASH_ENV generator (`scripts/early-stop-env.sh`), stopping on the EXIT trap only; HUP, INT, group INT and a failed subshell; TMPDIR empty in every case; the RED cases `skipped-listing`, `skipped-scan`, `ci-bash32` |
| `0a9a3ba1` | check-no-secrets.sh's listing and scan markers |
| `f910b4b2` | ci.yml: `bash32` on the macOS script-test row, and the bash the scripts run printed |
| `de48bb6d` | the generator's own case, `generator-exit-only` |
| `401b3fb2` | `skipped-listing` on a tree of its own -- **round 2's gate head** |
| `81248004` | ci.yml: `sed -n 1p`, not `head -n 1` -- **round 2's code head** (`head-delta-r2.txt`) |
| `1a121f8e` | round 2's records so far |
| `3f03a1ef` | **merge of main `753b0a72`** (train 14, PR #94), an ordinary merge, no conflict |
| `f529be91` | ci.yml: the `script_tests` comment rewrapped, comments only |
| `9e88f9ce` | round 2's README and records, the first merge's |
| `03cd1e67` | **merge of main `35cb4c7a`** (PR #95, release v0.2.0 #96), no conflict -- **the code head and the final gate's head** |
| (this one) | the second merge's records and README |

Sections from "The class" to "Gate" are round 1's, at `6071c6f8`; round 2's
runs are in the next section, at `401b3fb2`.

## Round 2 (reviews r1-opus and r1-gpt6)

Each item, where it is, and its file here.

1. **Which bash CI runs** (opus S1, gpt6 S2). In `ci.yml` (lines at
   `f529be91`, the same at `03cd1e67`):
   - the macos-15 row has `bash32: true` (`:91`);
   - the step "The gate scripts run /bin/bash" (under `matrix.bash32`,
     `:135-151`) checks /bin/bash is 3.2 and puts a `bash` link to it first
     on GITHUB_PATH, for the rest of that row's steps;
   - the step "Which bash the gate scripts run" (under
     `matrix.script_tests`, `:153-162`) prints, from a script that starts
     `#!/usr/bin/env bash`, the bash that then runs, then `command -v bash`
     and `bash --version`'s first line.

   Both come before the first script test, and their `if:`s are plain
   `matrix.X`. test-gate-env.sh's `ci-bash32` holds the row, both steps and
   their order. Run on this Mac as a runner would (`ci-bash-local.txt`): the
   first prints `/bin/bash is bash 3.2.57(1)-release`, the second `#!/usr/bin/env
   bash runs <tmp>/bash32/bash, bash 3.2.57(1)-release`. What a runner
   prints is for its log. The comments (`:52-57`, `:75-81`, `:254-259`) say
   only what the steps do. Every run step after the pin on the macOS row is
   one script or one cargo command, each run under /bin/bash 3.2 by the local
   gate; none uses bash 4 syntax (a reading of ci.yml).
2. **capture.sh's owner** (opus S2): below, "fxcap/capture.sh".
3. **The BASH_ENV file** (opus S3): `scripts/early-stop-env.sh` writes every
   host's files. Its `trap` stops only on a trap whose arguments name EXIT,
   after installing it, and prints `early stop: ` and `trap -p EXIT`; every
   case requires that line to end ` EXIT`. Its own case,
   test-early-stop.sh's `generator-exit-only`, runs a script that traps PIPE
   first: it must stop with the EXIT trap installed.
4. **More stops, and cleanup** (gpt6 S1, opus NIT 1). Each host's cases are
   now `unset` 1, `exit7` 7, `subshell` 5 (a `( exit 5 )` that `set -e`
   carries out), `term` 143, `hup` 129, `int` 130 (SIGINT to the script
   alone, between commands) and `int-group` 130 (SIGINT to the script and to
   the child it waits for, as a Ctrl-C or a cancelled step sends it), and
   every case requires an empty TMPDIR after it. SIGINT to the script alone
   while it waits for a child is not a case: bash goes on then, by its own
   rule (`probe-*.txt`, `child INT`).
5. **check-no-secrets.sh's listing and scan** (opus NIT 4). An arithmetic
   error is one bash does not stop on: it drops the whole command it is in
   (`probe-r2.txt`, (a): a function called at the top, its loop cut at the
   third pass, and the script goes on to exit 0, both bashes). `list_files`
   (`:133`) and `scan` (`:376`) each set a marker as their last statement,
   and the check fails without it (`:161-171`, `:478-482`).
6. **NITs.** 3: `probe.sh` has a `guarded` trap shape in its first table
   (`guarded unset-top exit=1` under 3.2). 5: test-gate-env.sh holds
   test-early-stop.sh's own guard (`test-early-stop-stopped-*`; mutants E1,
   E4-E6). 7: verdicts-base-bash5.txt's first line says base;
   test-early-stop.sh's header is rewrapped; ci.yml's comments as in 1. 2, 6
   and 8 are in Open.

### Round 2's runs (at `401b3fb2` unless named; `/bin/bash` 3.2.57)

- **RED** (`red-fixes.txt`, `redgreen-r2.sh`: each fixed file put back to
  `967581c9`, its host run):
  - check-no-secrets.sh without its markers: `skipped-listing` and
    `skipped-scan` exit **0**, with the check's ok line: `ok (1 tracked
    file(s) scanned)` over a tree of 4 that holds a key, and `ok (3 ...)`
    over a tree that tracks a .env;
  - ci.yml without `bash32`: `ci-bash32` fails, naming the missing row key
    and both missing steps; the other 92 cases pass. (Its ci-bash32 line is
    copied from the run's raw log: the runner's redaction had written
    `/bin/bash:` as `<path>`.)
- **The per-subject RED, again** (`red-r2.txt`, round 1's `redgreen.sh`:
  each subject put back to `dcfa5146`, its host run): every host exits 1.
  - check-no-secrets.sh: `stopped-unset`, `stopped-late` exit 0, and
    `skipped-listing`, `skipped-scan` (4 problems; the base has no
    markers either);
  - check-preview-contract.sh: `unset` exit 0; `term`, `hup`, `int` exit 1
    (they ran on);
  - check-xfx-identity.sh, smoke.sh: `unset` exit 0;
  - test-check-no-secrets.sh: `unset` exit 0; `term`, `hup`, `int` exit 1;
  - test-preview-contract.sh, test-gate-env.sh: `unset`, `term`, `hup`,
    `int` exit 0 (they ran on to their ok line).

  The first run was cut off by the handoff stop during its sixth subject;
  the last two subjects are a second run on the same clone (both in the
  file).

  Which cases are RED: `unset` (bash 3.2's exit 0), and `term`, `hup` and
  `int` for the four scripts that trapped those signals (they went on).
  `exit7`, `subshell` and `int-group` pass at the base too: bash kept an
  explicit exit's status there, and `set -e` carries a child's death by
  SIGINT out as 130 whether or not a trap caught the shell's own SIGINT.
  They pin the status the guard keeps and the group signal's end.
- **GREEN** (`green-r2.txt`): the four hosts exit 0, the clone clean after:
  test-check-no-secrets.sh 23 cases (its 10 early-stop and skipped cases
  among them), test-preview-contract.sh 35 (7 stopped), test-gate-env.sh 93
  (28 stopped, `ci-bash32`), test-early-stop.sh 15 (14 stopped,
  `generator-exit-only`).
- **Per script** (`verdicts-head-r2-bash32.txt`, `verdicts-head-r2-bash5.txt`):
  the seven exit 1 for `set-u`, 1 for `set-e`, 7 for `exit7` and 143 for
  `term` under both bashes; capture.sh still exits 0 for `set-u` (3.2).
- **Mutants** (`mutants-r2.py`; `mutants-r2.txt`, `mutants-r2-gate-env.txt`):
  13 of 13 killed:
  - SL1, SL2, SS1, SS2: each marker dropped or not checked (by
    `skipped-listing`, `skipped-scan`, `stopped-late`);
  - E1, E4, E5, E6: test-early-stop.sh's own guard, held by test-gate-env.sh
    (E6, `finished=1` dropped, by test-early-stop.sh's own run);
  - W1: the generator stopping on any `trap` (`generator-exit-only`); W2: no
    `trap -p EXIT` line (every case);
  - PIPE: a `trap '' PIPE` put above smoke.sh's EXIT trap with its guard
    reverted. The round-2 host fails it (`smoke-stopped-unset`); round 1's
    host, run on the same file, passes it (exit 0): opus S3's blind spot,
    shown and closed;
  - C1, C2: `bash32` dropped, the link not made (`ci-bash32`).
- **Normal paths** (`normal-r2.txt`, round 1's `normal.sh`, `ef4e74ea`
  against `401b3fb2` on one tree): every run exits 0 at both.
  check-no-secrets.sh, check-xfx-identity.sh, check-preview-contract.sh: 0
  lines differ. smoke.sh: the session id only. The three test hosts differ
  only in the new cases' lines and their ok-line counts (test-check-no-secrets
  17 to 23, test-preview-contract 31 to 35, test-gate-env 73 to 93, with
  `ci-bash32`), the E2BIG message's line number in
  test-check-no-secrets.sh (201 to 203), and test-gate-env's build ids.
- **Gate** (`gate-r2.txt`, `gates.sh`, on a `git clone --no-local`, one
  hold of the suite lock): 23 of 23 steps exit 0, the clone clean before and
  after; `cargo test` 5699 passed, with the fault injector 5736 passed, 0
  failed each.
- **`81248004`** changes one line of ci.yml (`head -n 1` to `sed -n 1p`, so
  a pipefail step cannot die of SIGPIPE): `head-delta-r2.txt`, actionlint
  and test-gate-env.sh at it: the diff is that line alone, actionlint exits
  0, test-gate-env.sh 93 cases ok (`ci-bash32` among them), the clone clean
  after. (Its first run was killed by the handoff stop; this is the
  rerun. Its `ci-bash32` line says `<path>` where the message says
  `/bin/bash`: the runner's redaction writes every `/...` word so.)

### The merge of main (`3f03a1ef`)

CI runs main's workflow against a pull request's tree, so the branch merges
`origin/main` `753b0a72` (`git ls-remote origin refs/heads/main` gave it).
`git merge --no-ff` merged ci.yml and test-gate-env.sh without a conflict:
main added the @ search's cost step and two test-gate-env.sh cases beside
this branch's changes; capture.sh came in rewritten by #94.

- The path check over the merge's staged diff (main's changes into the
  branch) printed 0, and so did `git diff dcfa5146 753b0a72` alone.
- The merged ci.yml: `bash32` on the macos-15 row, the two new steps (4,
  5) after "Confirm the runner really is this target" and before
  "Formatting", every script test after them, main's new step at 20; each
  `if:` a plain `matrix.X` or main's own; actionlint passes; `ci-bash32`
  covers it (`merged-scripts.txt`).
- Main's new script, check-at-search-cost.sh, installs no EXIT trap; the
  merged tree's traps are this branch's, the two local-path scripts (PR #95)
  and capture.sh's (now `:990`).
- `merged-scripts.txt` (`merged-scripts.sh`, a clone at `f529be91`, one hold
  of the suite lock, `/bin/bash`): actionlint and the five quick checks pass
  over 22360 tracked files; the script tests pass: test-check-no-local-paths
  10 cases, test-check-no-secrets 23, test-gate-env 95 (93 and main's two
  `at-search-*`), test-preview-contract 35, test-early-stop 15; the clone
  clean after.
- **capture.sh as #94 left it** (the same file): stopped after its EXIT
  trap, it still exits **0** for `set-u`, and 1, 7 and 143 for the rest. Its
  `cleanup()` (`:617`) still starts `local code=$?` and ends `exit
  "$code"` (`:644`), installed at `:990`. The finding stands; its owner is
  below.

### The second merge of main (`03cd1e67`)

Main moved to `35cb4c7a` (PR #95 at `ed0fe4ff`, then the v0.2.0 release,
#96); `git fetch origin main` gave it. An ordinary merge, no conflict: #95
changes check-no-local-paths.sh and its test, #96 CHANGELOG.md, README.md,
UPSTREAM.md, docs/parity.md and src/interactive.rs, and this branch changes
none of them.

- **Path checks**, each under the brief's rule and the common brief's
  shorter one: `git diff origin/main..03cd1e67` (what the branch adds over
  main) 0 and 0; `git diff 753b0a72..origin/main` (main's own range since
  the first merge) 0 and 0; and for the first merge, `git diff
  753b0a72..3f03a1ef` 0 and 0 with main's own `dcfa5146..753b0a72` 0 and 0.
  `git diff dcfa5146..03cd1e67` 0 and 0.
- **#95's guard and this branch's** now sit on main side by side. They do
  not conflict: no file holds both, and neither script reads the other's
  variables (each `finished` is its own script's). They differ in one
  thing, shown by `two-forms.txt` (`two-forms.sh`, every script stopped
  right after its EXIT trap by `scripts/early-stop-env.sh`): an early `exit
  7` or a `( exit 5 )` carried out by `set -e` leaves #95's two scripts
  with status 1, and this branch's with 7 and 5. An unset variable gives 1
  and SIGTERM 143 in both. Both fail closed. Left as two forms (Open).
- **The final gate** at `03cd1e67`, on a `git clone --no-local`, one hold of
  the suite lock: `gate-final.txt`. 23 of 23 steps exit 0, the clone clean
  before and after; `cargo test` 5854 passed, with the fault injector 5891,
  0 failed each; the script tests on this tree: test-check-no-local-paths
  31 cases (#95's), test-check-no-secrets 23, test-gate-env 95,
  test-preview-contract 35, test-early-stop 15.
- **The @ search's cost step**, which main's ci.yml added (#93) and
  `gates.sh` does not list, on the same clone after the gate:
  `at-search-final.txt`: exit 0, `check-at-search-cost: ok` (every keystroke
  p99 3.28 ms, max 3.53 ms, against its 8 ms frame).

## The class, in bash itself (`probe.sh`)

`probe-bash32.txt` (`/bin/bash` 3.2.57, which is also the bash on PATH on
this machine) and `probe-bash5.txt` (5.3.20, the `bash:5` image): a script
file, `set -euo pipefail`, stopped each way under each trap shape the
repository uses.

- **An unset variable** (`unset-top`, `-array`, `-func`, `-local`,
  `colon-q`): bash 3.2 exits **0** under every trap shape and 1 with no trap;
  bash 5 exits 1 under every shape, except `unset-array`, which is no stop
  there (reached-end yes). So every case here reads a scalar.
- **The status is gone before the trap runs**: after an unset variable, the
  EXIT trap sees `$?` 0 under 3.2 and 1 under 5 (`entry-unset`). So the fix
  cannot keep 3.2's status; it gives 1, the status the same stop has with no
  trap (the `none` rows).
- **Kept by both bashes, under every trap**: `set -e` (`false-top`,
  `false-func`), `pipefail`, an unset variable inside `$( )` (the assignment
  fails under `set -e`), and `exit 3` at the top or in a function (the trap
  sees 3, `entry-exit3`).
- **A signal**: a trap that lists HUP, INT or TERM catches it, runs the
  cleanup and the script goes on, in both bashes (`rm-sig sigterm-self`:
  exit 0, reached-end yes). With an EXIT trap alone the shell runs it and
  dies of the signal, 128+n, its scratch removed (`exit-only` rows); the
  guarded trap this unit installs does the same (`guarded` rows).
- **Not stops**: an arithmetic error in an assignment goes on in both bashes
  (`arith`), trap or not. SIGINT sent to the shell alone while it waits for a
  child (`child INT`) goes on too, by bash's own rule for SIGINT: the run
  continues whole and exits through its last line. Neither is the class.

## The scripts (`verdicts.sh`)

Each script, run whole from a throwaway clone, stopped right after its first
`trap` statement by a BASH_ENV file whose function `trap` installs the trap
and then stops one way: `set-u` (reads an unset variable), `set-e` (runs
`false`), `exit7`, `term` (SIGTERM to itself, then a line saying it is still
running). Files: `verdicts-base-bash32.txt`, `verdicts-base-bash5.txt` (bash
5.2.37 in `ruby:3.4-trixie`, which has git, ruby and python3), and the same
at the head (`verdicts-head-*.txt`).

| script | trap at `dcfa5146` | `set-u`, bash 3.2 | `set-u`, bash 5 | `term`, bash 3.2 | `term`, bash 5 | fails open |
|---|---|---|---|---|---|---|
| check-no-secrets.sh | `:61`, `:177`, `:386`, EXIT | **exit 0**, its only lines the marker and bash's (`excerpts.txt`) | 1 | 143 | 143 | yes, bash 3.2 |
| test-check-no-secrets.sh | `:69`, EXIT HUP INT TERM | **exit 0** | 1 | ran on, 1 | ran on, 1 | yes: bash 3.2; a signal in both |
| check-xfx-identity.sh | `:171`, EXIT | **exit 0** | 1 | 143 | 143 | yes, bash 3.2 |
| check-preview-contract.sh | `:957`, EXIT HUP INT TERM | **exit 0** | 1 | ran on, 1 | ran on, 1 | yes: bash 3.2; a signal in both |
| test-preview-contract.sh | `:35`, EXIT HUP INT TERM | **exit 0** | 1 | ran on to its ok line, **exit 0** (51 s) | ran on to its ok line, **exit 0** (17 s) | yes: bash 3.2; a signal in both |
| test-gate-env.sh | `:109`, EXIT HUP INT TERM | **exit 0** | 1 | ran on to its ok line, **exit 0** (160 s) | ran on, 1 (123 s) | yes: bash 3.2; a signal in both |
| smoke.sh | `:413` `cleanup`, EXIT | **exit 0** | 1 | 143 | 143 | yes, bash 3.2 |
| fxcap/capture.sh | `:849` `cleanup`, EXIT | **exit 0** | not run | 143 | not run | yes, bash 3.2 (below) |

At the head every one of the seven exits 1 for `set-u`, 1 for `set-e`, 7
for `exit7` and 143 for `term` under both bashes; capture.sh still exits 0
for `set-u` (`verdicts-head-*.txt`).

`set-e` exits 1 and `exit7` exits 7 for every script, at the base and the
head, under both bashes: those stops were never the class.

**Where a signal runs on** (`term` above): the trap on TERM deleted the
scratch directory and the script went on (`excerpts.txt`: each log's
`still running after SIGTERM` line). test-check-no-secrets.sh and
check-preview-contract.sh next wrote into the deleted directory (`:87`,
`:959` at the base), failed, and exited 1. test-preview-contract.sh and
test-gate-env.sh ran every case to their ok line and exited 0; their first
writes under it are `mkdir -p` (`:44`, `:152`), which made it again (a
reading of those lines; the ok line is in `excerpts.txt`).

Where the signal lands decides the status, so `late.sh` sends it later
(`late.txt`, bash 3.2): check-preview-contract.sh with SIGTERM as actionlint
starts exits **0** with its ok line, and test-check-no-secrets.sh with
SIGTERM just before its ok line exits **0**. At the head both exit 143. And
check-no-secrets.sh stopped by an unset variable just before its ok line,
its whole scan done and its last trap installed, exits **0** at the base
and 1 at the head. The base `set-u` run of check-no-secrets.sh prints two
lines, the stop's marker and bash's own (`excerpts.txt`), and exits 0.

test-gate-env.sh's bash 5 `term` run at the base is 1, not 0
(`v5-gate-env-term.txt`: it ran on for 93 s, 1 problem, 60 cases passed):
its one failing case is `build-script`, `rustc could not compile build.rs`,
in a container that has no Rust. The same container runs it unsignalled
to the same 1 problem, `build-script`, and 60 cases passed
(`v5-gate-env-plain.txt`): its 1 is the missing rustc, and after the
SIGTERM it ran every case it runs unsignalled.

## The fix

Seven scripts (`git diff --numstat dcfa5146 6071c6f8 -- scripts .github`:
the files that are not tests, 57 lines in and 6 out):

- **A run that stops before its last line fails.** `finished=0` before the
  trap, `finished=1` as the last line, and the trap ends
  `[ "$finished" = 1 ] || exit "$((stopped == 0 ? 1 : stopped))"`, its first
  statement `stopped=$?`. So a stop keeps the status bash gives it (1 under
  bash 5, 7 for `exit 7`, 128+n for a signal) and only 0 becomes 1. That is
  the difference from `b0fd5157`'s `|| exit 1`: an `exit 2` there is 1.
  smoke.sh's `cleanup()` takes `local stopped=$?` first and ends the same way.
- **check-no-secrets.sh installs one trap** (`on_exit`, at `:77`) for its
  three scratch files, each removed if it exists (`${control:+...}`,
  `${sample_dir:-}`), instead of three traps in turn (`:61`, `:177`, `:386`
  at the base): each `trap` replaces the one before, so three would each
  need the guard, and a fourth added without it would undo it.
- **No trap on HUP, INT or TERM** in test-check-no-secrets.sh,
  check-preview-contract.sh, test-preview-contract.sh and test-gate-env.sh:
  the shell runs the EXIT trap on the signal and dies of it, its scratch
  removed (`probe-*.txt`, `exit-only` rows). A trap there ran the cleanup and
  went on without the scratch directory.
- `ci.yml`: `scripts/test-early-stop.sh` on the `script_tests` rows (one
  macOS, one Linux), and test-gate-env.sh's `ci-script-tests-per-os` case
  holds it there with the other script tests.

## RED and GREEN

The cases (`/bin/bash` 3.2.57; `redgreen.sh` on a clone at `6071c6f8`):
each host stops its subjects with a BASH_ENV `trap` right after their trap
is installed, three ways (`unset` 1, `exit7` 7, `term` 143), and requires
that status, the stop's own line in the output, and nothing after it.
test-check-no-secrets.sh adds `stopped-late`: check-no-secrets.sh stopped by
an unset variable just before its ok line, on a one-file tree, which must
also leave its TMPDIR empty.

| subject | host |
|---|---|
| check-no-secrets.sh | test-check-no-secrets.sh (`stopped-*`, 4 cases) |
| check-preview-contract.sh | test-preview-contract.sh (`stopped-*`, 3) |
| check-xfx-identity.sh, test-check-no-secrets.sh, test-preview-contract.sh | test-gate-env.sh (`<script>-stopped-*`, 9) |
| test-gate-env.sh, smoke.sh | test-early-stop.sh, new (6), on the `script_tests` rows |

- `green.txt`: the four hosts at the head, all exit 0, every early-stop case
  ok, the clone clean after.
- `red.txt`: each subject put back to `dcfa5146` alone, its host run; every
  host exits 1:
  - check-no-secrets.sh: `stopped-unset` and `stopped-late` exit 0;
  - check-preview-contract.sh: `stopped-unset` exits 0, `stopped-term` 1
    (it ran on);
  - check-xfx-identity.sh: `stopped-unset` exits 0;
  - test-check-no-secrets.sh: `stopped-unset` exits 0, `stopped-term` 1;
  - test-preview-contract.sh: `stopped-unset` and `stopped-term` exit 0;
  - test-gate-env.sh: `stopped-unset` and `stopped-term` exit 0;
  - smoke.sh: `stopped-unset` exits 0.

  `exit7` passes at the base for all of them: bash keeps an explicit exit's
  status there. It is the control that pins the status the guard keeps
  (the `status not kept` mutants below). Every other case passes in each
  RED run, so each RED is its subject's revert alone.

## Mutants (`mutants.py`, on `git clone --no-local` copies, under the suite lock)

Each fix line reverted alone in a throwaway clone at `6071c6f8`, the host
that holds it run with `/bin/bash`; a dropped `finished=1` also runs the
script's own normal run where the host does not.

- `mutants-rest.txt`: 28 of 28 killed. check-no-secrets.sh 11 (S1-S11: the
  base trap back, the guard, the status kept, `local stopped=$?`,
  `finished=0`, `finished=1`, each later trap put back, each scratch file's
  removal, the signals trapped); check-preview-contract.sh 6 (P1-P6);
  test-gate-env.sh 6 (G1-G6, G6 by its own normal run); smoke.sh 5 (M1-M5).
- `mutants-gate-env.txt`: 16 of 16 killed, the three scripts
  test-gate-env.sh holds (T1-T6, X1 and X4-X6, R1-R6; check-xfx-identity.sh
  never trapped a signal, so it has no X2 or X3), each a full run of it.
  X6 and R6, `finished=1` dropped, pass test-gate-env.sh, whose cases want
  those scripts to fail; each script's own normal run fails (exit 1), and CI
  runs both on their own (`ci.yml`: check-xfx-identity.sh on every row,
  test-preview-contract.sh on the `script_tests` rows).
- `mutants-m6.txt`: M6, smoke.sh's `finished=1` dropped: test-early-stop.sh
  passes it (its cases want smoke.sh to fail), and smoke.sh's own run on the
  gate's release binary exits 1, which CI's smoke step runs on every row.

All 45 mutants killed, each clone clean after.

Which case kills which mutant: the base trap back and the guard dropped by
`stopped-unset` (and `stopped-late` for check-no-secrets.sh); the signals
trapped again by `stopped-term`; `exit 1` for the kept status, and
`stopped=$?` or `finished=0` dropped, by `stopped-exit7` (`finished=0`
dropped makes the trap stop on its own unset variable, status 1); the later
traps and the cleanup's files by `stopped-late`'s empty TMPDIR.

## Normal paths (`normal.sh`)

`normal.txt`: each changed script's own run, the base's version and the
head's on the same tree (a clone at `6071c6f8`), `/bin/bash`, smoke.sh on
the gate's release binary; each output with paths written `<path>` and
seconds `<n>s`, then compared. Every run exits 0 at both.

- check-no-secrets.sh, check-xfx-identity.sh, check-preview-contract.sh: 0
  lines differ.
- smoke.sh: 1 line each side, the session id the run recorded (a new one
  each run).
- test-check-no-secrets.sh: the four new `stopped-*` lines; the case count
  in its ok line, 13 to 17; and the line number bash puts in the E2BIG
  message of its own precondition, `line 189` to `line 201`, the 12 lines
  this unit added above it.
- test-preview-contract.sh: the three new `stopped-*` lines and its count,
  28 to 31.
- test-gate-env.sh: the nine new `*-stopped-*` lines and its count, 64 to 73;
  `ci-script-tests-per-os` names test-early-stop too; the two build ids its
  `build-script` cases print (a timestamp and a scratch commit, new each
  run).

No other line differs.

## Gate

`gate.txt` (`gates.sh`: local-paths-selftest-flake's, with
`test-early-stop.sh` added), at `6071c6f8`, on a `git clone --no-local`, in
one hold of the suite lock: 22 of 23 steps exit 0, the clone clean before
and after. The one failure is step 05, `cargo test --features
fault-injection`: 4797 passed, 1 failed,
`tui::worker::tests::a_stop_in_the_catalog_wait_of_a_turn_that_would_ask_twenty_questions_ends_it_as_the_stop`,
its timing window (`with the stop: 300 ms to 1 s`) missed, at load 11.84:
the known catalog-wait class, in Rust code this unit does not touch.
`rerun.txt` (`rerun.sh`): that test alone, three times, under the lock.
3 of 3 pass, at loads 15.26 to 17.80.

The code head `5b20b9d6` changes comments only (`head-delta.txt`); the quick
steps there pass. The full gate was not rerun for it.

## fxcap/capture.sh: affected, not changed here

`scripts/fxcap/capture.sh:849` installs `trap cleanup EXIT` in `main`;
`cleanup()` (`:487`) starts `local code=$?` and ends `exit "$code"`
(`:503`). Under bash 3.2 a stop by an unset variable after `:849` gives
`code` 0: the run exits 0 (`verdicts-base-bash32.txt`, `fxcap/capture.sh
set-u exit=0`, scenario `alias`, a stub binary; `:849` comes after the fake
gateway starts at `:846` and before the binary is launched). `set -e` gives
1, `exit 7` 7 and SIGTERM 143. Only a guard that fires sets `code=3`
(`:492`, a reading; no guard ran here, the stub being xfx). A capture that
stops early is a half capture that reads as done. It is
unchanged at the head (`verdicts-head-bash32.txt`, the same row).
It is a CI gate, not only a tool: sweep.yml's sweep job (`runs-on:
macos-15`, `:40`; the sweep step `:159-167`) runs `ci-sweep.sh`, whose
sweep.py runs `bash capture.sh` from PATH (`sweep.py:205` at `dcfa5146`,
`:224` after the merge) and counts exit 0 as a run (`unit_status`,
`sweep.py:388-405`, `:422` after the merge). WORKSPACE-INDEX-AT-PLAN (PR
#94) rewrote capture.sh, so this unit does not touch it. #94 is on main
since `3570a182`, and the merged tree's capture.sh still exits 0 after a
`set -u` stop (`merged-scripts.txt`; its trap at `:990`, `cleanup()` at
`:617`). **Owner: LOCAL-PATHS-EXIT-TRAP-CAPTURE**, now unblocked:
`cleanup()` keeps its `code` and fails a 0 before the end of `main` the same
way (a `finished` set after `main` returns), with a case that stops it after
the trap. Whether the sweep's later comparison would catch a half capture
is not checked here. Not run under bash
5: the container has no bun or tmux to reach `:849`. There the trap sees
`$?` 1 after an unset variable (`probe-bash5.txt`, `entry-unset`), so `code`
would be 1 (a reading).

## Outside the class, or not this unit's

- `scripts/check-no-local-paths.sh:283` and
  `scripts/test-check-no-local-paths.sh:62` at `dcfa5146` are the same class
  (EXIT trap ending in `rm`, `set -euo pipefail`). PR #95 fixed both with
  `|| exit 1` (on main since `ed0fe4ff`); this unit leaves them to it. The two
  forms differ only in what an early `exit N` reports (1 there, N here;
  `two-forms.txt`).
- `.github/workflows/preview.yml:501` and `:767`: `trap 'ssh-agent -k ...
  || true' EXIT` in steps that run `set -euo pipefail`, in the `publish` job,
  `runs-on: ubuntu-24.04` (`:408`). A Linux runner's bash is not macOS's
  3.2, and bash 5 keeps a `set -u` stop's status (`probe-bash5.txt`); not
  run here, so this one is a reading: not affected.
- No other tracked script installs an EXIT trap (`traps-at-base.txt`: the
  eight scripts above, the two local-path scripts and the two workflow
  steps; smoke-tui.sh's one `trap` is a `trap ''` on signals in the command
  line of a child it starts).

## Open

- **Two forms of the guard on main.** PR #95's check-no-local-paths.sh and
  its test end the trap `|| exit 1`, which turns an early `exit N` into 1;
  these seven keep N (`two-forms.txt`). Both fail closed, and they do not
  conflict. Left as two forms; converging them is a one-line change to #95's
  two traps, for its own unit.
- **capture.sh** (above): LOCAL-PATHS-EXIT-TRAP-CAPTURE, unblocked now that
  #94 is on main.
- **A SIGTERM to the script alone while a foreground child runs** (review r1,
  NIT 2, a reading): bash runs the EXIT trap and dies without waiting for
  the child, which can go on and write into the removed scratch directory.
  The probe's `child` rows do not record the child. A cancelled CI step or
  a Ctrl-C signals the whole process group. Not changed.
- **check-preview-contract.sh sources workflow text** (`:990`, `:1015`, two
  snippets extracted from preview.yml) in the shell that holds the guard
  (NIT 6): a snippet that assigned `finished` or `work` would reach it.
  Neither does today (review r1). Not changed.
- **Two signal windows** that leave a scratch file (NIT 8): a signal between
  check-no-secrets.sh's first `mktemp` (`:60`) and its trap (`:77`), and the
  `mktemp -d` of its sample directory, which runs in a command substitution.
  Older than this unit; not changed.
- **SIGINT to the shell alone while it waits for a child** goes on, by
  bash's rule for SIGINT (`probe-*.txt`, `child INT`), with or without these
  traps: the run continues whole to its last line. Not the class, not
  changed.
- **Which bash CI runs**: since round 2 the macOS script-test row runs the
  gate scripts under /bin/bash, after checking it is 3.2, and both
  script-test rows print what `#!/usr/bin/env bash` runs. No CI log of it
  exists yet: the dispatcher pushes and reads it. The Linux row's bash is
  unknown until then; the bash 5 runs here are containers (`bashes.txt`).
