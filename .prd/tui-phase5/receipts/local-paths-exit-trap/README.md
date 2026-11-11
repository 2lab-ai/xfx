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
| (this one) | this README and the records |

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
WORKSPACE-INDEX-AT-PLAN (PR #94) rewrites capture.sh, so this unit does not
touch it. **Follow-up**: once #94 is on main, `cleanup()` keeps its `code`
and fails a 0 before the end of `main` the same way (a `finished` set after
`main` returns), with a case that stops it after `:849`. Not run under bash
5: the container has no bun or tmux to reach `:849`. There the trap sees
`$?` 1 after an unset variable (`probe-bash5.txt`, `entry-unset`), so `code`
would be 1 (a reading).

## Outside the class, or not this unit's

- `scripts/check-no-local-paths.sh:283` and
  `scripts/test-check-no-local-paths.sh:62` at `dcfa5146` are the same class
  (EXIT trap ending in `rm`, `set -euo pipefail`). PR #95 (`b0fd5157`) fixes
  both with `|| exit 1`; this unit leaves them to it. Once both are on main,
  the two forms differ only in what an early `exit N` reports (1 there, N
  here).
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

- **Two forms of the guard on main.** PR #95's check-no-local-paths.sh
  and its test end the trap `|| exit 1`, which turns an early `exit N` into
  1; these seven keep N. Both fail closed. Converging them is a one-line
  follow-up once #95 merges; not done here, to keep off #95's files.
- **capture.sh** (above): the same fix after PR #94 lands.
- **SIGINT to the shell alone while it waits for a child** goes on, by
  bash's rule for SIGINT (`probe-*.txt`, `child INT`), with or without these
  traps: the run continues whole to its last line. Not the class, not
  changed.
- **Which bash the macOS CI row runs** these scripts with is not in any CI
  log yet (they take bash from PATH). The bash 5 runs here are containers
  (`bashes.txt`), not CI rows; the Linux row is bash 5's first CI run of
  this code.
