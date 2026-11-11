# local-paths-selftest-flake: the local-path check reads the tree twice and says what it lost

Unit LOCAL-PATHS-SELFTEST-FLAKE. The macOS arm64 row's run of
`scripts/test-check-no-local-paths.sh` failed its `shapes` case twice with
inputs that pass elsewhere: the check exited 1 but did not name one or two
planted files. Base `origin/main` `d2b2ca75`. Every claim below is tied to a
file in this folder.

**The trigger is not found.** This unit does not claim to stop the flake. It
closes the check's fail-opens (a late NUL, a file gone mid-scan, a tracked
symlink, an index listing cut inside its last entry, a run that stops early
and exits 0 under bash 3.2), hardens the scan (two readings that must agree,
every line accounted for), and makes the next failure decisive (the failing
run's own evidence, the expected accounting, a second run on the same
tree), without printing a path.

| commit | what |
|---|---|
| `351aa2df` | one accounted `git grep -n -z` pass; a NUL anywhere is binary; `[[ =~ ]]` status 2 fails; the late-NUL plant, `disagree`, `vanished` |
| `373811b0` | one look for missing files, after the scan |
| `54581752` | the test's `git` shim runs under bash |
| `3ee1766f` | the first receipts (the round-1 gate's head, `gate.txt`) |
| `a72b738a` | every line in one count; a mark needs the marker by string match too; `badnum`, `gitfail`; a first lost-plant probe |
| `25336ee5` | `gates-round2.sh` (the round-2 gate's head, `gate2.txt`) |
| `080a22b2` | README, both sightings, mutants |
| `4743a479` | round 3: the second reading (`git grep -c`, file by file); tracked symlinks; a record cut short; the trace; the pinned accounting and the decisive diagnosis; `omit`, `truncated` |
| `1f6e5345` | receipts: the copies of the self-test's lines dropped, round-3 mutants, `bash-regex-status.txt` fixed |
| `2ddf7d88` | the diagnosis against a check that writes no trace |
| `b0fd5157` | a run that stops early fails, in the check and its test (bash 3.2 exits 0 through an EXIT trap) |
| `836ef942` | the early-exit guard's mutant -- **round 3's gate head** (`gate3.txt`, `mutants3.txt`) |
| `f133ce6c` | round 3's README and records |
| `4e2bdacb` | review round 2: listings cut inside their last entry fail; the diagnosis prints no path; `stopped`, `diagnosis`, `cutlinks`, `cuttracked`; a directory link -- **the code head** |
| `dbb15990` | the listing guards' mutants -- **round 4's gate head** (`gate4.txt`, on a clone; `mutants4.txt`) |
| `7f97d7e3` | round 4's README and records |
| `f9a4b0db` | the diagnosis reads config names, never values -- **round 5's gate head** (`gate5.txt`, on a clone) |
| `6194f1a5` | round 5's README and records |
| `d7bc404c` | the diagnosis writes to fd 3 and every command in it drops its stderr; `diagnosis-gone`; every line shape anchored whole -- **the code head and round 6's gate head** (`gate6.txt`, on a clone) |
| (this one) | this README and round 6's records |

`3ee1766f` added `repro/harness-shapes.sh`, `repro/harness2.sh` and
`repro/stageA.sh`, copies of the self-test's fixture lines; `1f6e5345` drops
them (`git show 3ee1766f:<path>` reads them). Under the dispatcher's
per-commit rule `3ee1766f` alone prints 6, for those lines; every other
commit prints 0.

## The failure, and that it varies

| run | commit | `shapes` | not named | file |
|---|---|---|---|---|
| 42863297443 attempt 2 (PR #92) | `4ad6405e` | FAIL | `w3/trace-report.md:1`, `w3/xfx-trace-40x120-report-1.md:1` | `ci-attempt2.txt` |
| 42863297443 attempt 3 | `4ad6405e` | ok | -- | `ci-attempt3-step.txt` |
| 42880581900 (main) | `dcfa5146` | FAIL | `w3/trace-report.md:1` | `ci-main-dcfa5146.txt` |

All on image `macos-15-arm64` `20261104.0514.1` with git 2.56.0; the image's
readme lists bash 3.2.57, but the scripts find bash on PATH and no log yet
says which ran (round 3's diagnosis prints it). `ci-history.txt`: the step
on every macOS arm64 run since the check landed. That row is the only macOS
row that runs the script tests (`.github/workflows/ci.yml`, `script_tests`).
The check and its test are the same in `d2b2ca75`, `4ad6405e` and `dcfa5146`.

## Reproduction: not reproduced

`repro/tallies.txt`, each line the loop's own last line, at `d2b2ca75`'s
check and test, on macOS 26.7.1, 18 cores, `/bin/bash` 3.2.57, git 2.56.0,
under load averages between 8 and 34: 1400 end-to-end `shapes` runs (short,
long and `/var/folders` TMPDIRs, 500 of them instrumented, `repro/mirror.diff`),
and each stage alone -- `git add` 1000 times, the check on one fixed tree
1498 times (some with 3 grep threads and a runner-like global config,
`repro/ci-like-gitconfig`), the two git greps 3000 times, the shell's regex
on the two missed lines 200000 times: 0 failures, 0 differences. The
harnesses behind the first and second figures are at `3ee1766f` (above);
`repro/stageB.sh`, `stageC2.sh` and `stageD.sh` ran on a tree one harness
run kept, and are records of what ran. Bash 5 was not run here (it is not
installed, and installing it would change `env bash` for other work on this
machine); the Linux CI row runs it, and nothing in this unit has run under
it yet.

## Where a plant went unsaid (the check at `d2b2ca75`)

In `scripts/check-no-local-paths.sh` at `d2b2ca75`, a plant is lost without
a word at one of four places:

1. `:214`: what `git grep -l` leaves out of the listing is never looked at.
2. `:234` then `:240`: the per-file grep gives no line (status 1, empty
   stderr), and the read loop runs zero times. Forced by a `git` that gives
   nothing for that grep: exit 0 with the plant in the tree (`forced.txt`,
   `per-file-empty`).
3. `:273-277`: a line in which the shell finds no shape counts as a
   placeholder line. Real content does it: a text file whose first 8000
   bytes hold no NUL (the probe at `:225` reads only those), with a NUL
   before the path on one line (`forced.txt`, `late-nul`, exit 0).
4. `:243`: a line the mark regex says is marked (authored files only).

And for any file: a tracked file gone while git grep runs (git skips a file
it cannot stat for ENOENT without a message, `git-grep-enoent.txt`; the
look for missing files at `:400-409` came before the scan at `:411`;
`red.txt`, `vanished`, exit 0), and a tracked symlink, which git grep does
not read at all (the same file, `grep.c:1960-1961`: `S_ISREG` or skip), whose
target is what it publishes (`red3.txt`, `shapes`: `home-link` not named).

Which of 1-4 fired on CI the old check did not record. The two that
disagree are git's first answer (the file holds a shape) and the next
reading of it (git's per-file grep or the shell's regex). The check could
pass a real local path in a real tree: by the late NUL, the vanished file
and the symlink every time, and by the others whenever their trigger fires.

## The fix

- One `git grep -a -n -z` pass gives every matching line as
  `<path>NUL<number>NUL<text>LF`; every line lands in one count (refused,
  binary, marked, placeholder). That the counts add up guards the scan's own
  code, not git's reading.
- **The second reading**: `git grep -c -z` counts the matching lines file
  by file, and the scan fails if that disagrees with the line pass, naming
  the file and both counts (`omit` forces it). A line that git's line pass
  leaves out on one run and not on the next now fails closed.
- A line git matched in which the shell finds no shape and takes no
  placeholder out is refused, labelled so; a mark excuses a line only if
  bash's string match finds the marker too.
- A file with a NUL anywhere is binary, refused by name.
- **Tracked symlinks**: each mode-120000 entry's target (`readlink`) is
  matched against the shapes, with no exception, and a hit is refused by
  name; the positive control and `shapes` plant one.
- Missing files are looked for after the scan; a regex bash cannot compile,
  a line with no number and a stream that ends inside a record (`truncated`)
  fail the scan, and so does an index listing that ends inside its last
  entry: the symlink listing (`cutlinks`) and the tracked one
  (`cuttracked`). Both used to read that entry and drop it, so a symlink or
  a skip-worktree entry listed last went unexamined (`red4.txt`: exit 0 at
  `f133ce6c`). The ok line counts symlinks (main tracks none,
  `main-tree.txt`).
- The limit, now in the header: a line from which a placeholder was taken
  out passes if the shell then finds no shape in it, so a real path beside a
  placeholder that the shell's matcher misses and git's does not would pass.
  Nothing in the check can tell that line from one that held only the
  placeholder. I recorded it rather than refuse every such line.
- Encodings the shapes' ASCII letters do not spell (UTF-16, a
  `working-tree-encoding` file) are added to the header's list of what is
  not found.
- **A run that stops early fails.** Under bash 3.2.57 a script stopped by
  its own error (an unset array under `set -u`) leaves through an EXIT trap
  with status 0 (`bash-exit-trap.txt`). The round-3 mutant run found it: the
  self-test's diagnosis looped over an empty array, stopped, and two
  mutants read as survivors while their cases failed. The check and the
  test now fail any exit before their last line; the `stopped` case makes
  the check stop so (a BASH_ENV `cmp` that reads an unset variable) and
  requires exit 1 with no ok line.

Source change (`git diff --numstat d2b2ca75 4e2bdacb -- scripts/`): the
check 270 lines in, 70 out; its test 351 in, 32 out.

## The diagnosis

The check writes, when `CHECK_NO_LOCAL_PATHS_TRACE` names a file, each line
git gave as its class, number and path, never its text. `shapes` pins the
accounting the check must print for its tree (`git gave 43 line(s): 27
refused, 2 in 2 binary file(s), 3 marked, 11 placeholder`, from the plants
and the `self` case's counts), and on a failure prints:

- `ran:` the bash and git versions, never their paths, and `config:` the
  `grep.*`, `core.fsmonitor`, `index.*` and `feature.*` keys with their scope.
  It prints no path by construction: it never reads a value as text. It
  lists names only (`git config --show-scope --name-only --get-regexp`), a
  name cannot hold a newline or a tab, and it still prints a scope only if
  it is one of git's scope words and a name only if it matches
  `[A-Za-z0-9.-]+` (`<odd>` otherwise); a value is printed only as `git
  config --type=bool-or-int --get` types it, a boolean or a number, `<set>`
  if not, git's stderr dropped. The round-4 version read `--get-regexp`'s
  `scope<TAB>key value` lines, and a value that spans lines printed its
  second line raw as a scope and a key (`red5.txt`). The `diagnosis` case
  plants a global config whose origin is a path, one value a path and one
  value two lines with a home on the second, and requires that no line holds
  a path and that every line the diagnosis prints has a known shape; on a
  failure it prints counts, not the text. Nor can a command's own error
  print one: the diagnosis writes its lines to fd 3, every `cd`, git call
  and file read in it drops its own stderr (a `cd` that fails names its
  directory, which round 5's did: `red6.txt`), and the caller drops fd 2.
  The `diagnosis-gone` case runs it over a tree that does not exist, its
  fd 2 and fd 3 captured together. Each kind of line is matched whole; the
  free text in them by design is a plant's relative name and the bash and
  git version strings, none allowed to start a word with `/`;
- `expected:` against `actual:` accounting;
- for each plant not named that the failing pass did not refuse, `lost:`
  what that pass gave of it (from the trace: `line 1 placeholder`, or `no
  line of it`), then what the tree holds after it: in the index and on disk,
  and a later `git grep -c`, each with its exit status, git's stderr
  dropped; and one count of the plants the failing pass refused before the
  scan failed (so a "two readings disagree" line stands out);
- a second run over the same tree: `per tree` or `per run`.

A plant in the index and on disk, which a later grep matches and of which
the failing pass gave no line, was left out by the check's own git pass.
Under round 3's check such a pass also fails closed on the second reading,
naming the file. The two forced cases that show the lines are below
(`diagnosis.txt`, made at `2ddf7d88`; `b0fd5157` changes neither line's
code).

## Look ahead: main's tree (`main-tree.txt`)

At `d2b2ca75`, main's tree: the old check passes with 18935 files, 8 marked
and 80 placeholder lines, and one independent `git grep -c` with its
patterns (`count.sh`) finds 88 matching lines in 17 files (8 + 80). The
checks at `373811b0` and at `4743a479` (round 3) give the same counts on the
same tree.

## RED and GREEN

- `red.txt`: the self-test at `54581752` against the check at `d2b2ca75`, in
  a `git clone --no-local`: 3 of 12 fail.
- `red2.txt`: the self-test at `a72b738a`: 4 of 14 fail.
- `red3.txt`: the self-test at `b0fd5157`: 7 of 16 fail (`shapes`, its late
  NUL and its symlink; `disagree`, `vanished`, `badnum`, `gitfail`, `omit`,
  `truncated`), and the diagnosis says the old check wrote no trace.
  `gitfail`'s is about the message only: the old check also exits 1 there,
  naming `./plant.md` where the case wants `.`.
- `red4.txt`: the self-test at `4e2bdacb` against the check at `f133ce6c`
  (round 3's head): 5 of 20 fail. `cutlinks` and `cuttracked` exit 0 with
  the plant in the tree, the fail-opens fixed; `self`, `shapes` and `passes`
  fail on the ok line's new symlink count only.
- `red5.txt`: the self-test at `f9a4b0db` with `7f97d7e3`'s
  `diagnose_shapes`, against the check at `7f97d7e3`: `diagnosis` fails (1
  line with a path, 1 line of no known shape); the other 19 pass.
- `red6.txt`: the self-test at `d7bc404c` with `6194f1a5`'s
  `diagnose_shapes`, against the check at `6194f1a5`: `diagnosis-gone` fails
  (2 lines with a path, 2 of no known shape); the other 20 pass.
- `green.txt` to `green6.txt`: the same clone at `54581752` (12 cases),
  `a72b738a` (14), `b0fd5157` (16), `4e2bdacb` (20), `f9a4b0db` (20) and
  `d7bc404c` (21): all pass.
- `forced.txt`: the three forced paths: exit 0 at `d2b2ca75`, exit 1 and the
  plant named at `373811b0`.

## Mutants (`mutants.py`, on a `git clone --no-local`, under the suite lock)

- `mutants-round1.txt` (9 mutants, at `54581752`); `mutants2.txt` and
  `mutants2b.txt` (13, at `a72b738a`).
- `mutants3.txt` (17, the old and the new, at `836ef942`, whose scripts are
  `b0fd5157`'s): 13 killed, 4 survive. Three of the four round-3 guards are
  killed: the second reading (`omit`), symlinks (`shapes` and the positive
  control), a record cut short (`truncated`); the fourth, the early-exit
  guard, survived (below). In a first run, at
  `2ddf7d88`, two placeholder mutants read as survivors while their cases
  failed: that is how the bash 3.2 exit status was found, fixed in
  `b0fd5157`.

- `mutants4.txt` (19, at `dbb15990`, whose scripts are `4e2bdacb`'s): 16
  killed, 3 survive. The early-exit guard is killed now (`stopped`), and
  so are both listing guards (`cutlinks`, `cuttracked`).

The three older survivors are guards the tree fixtures do not exercise,
with the patterns fixed and a correct regex engine assumed; an
instrumented copy of the check could, and each is narrowed instead (opus
judged these three not worth a test):

- *a pattern the shell cannot compile ignored*: the patterns are fixed, and
  the positive control, which runs every one of them, is the guard that
  fails first; `uncompiled` only keeps a compile failure in the tree's scan
  alone from passing.
- *a mark by the regex alone*: on a correct regex engine it changes nothing,
  because the regex holds the marker's letters itself.
- *the count not checked*: it fires only if this file's code loses a line;
  the mutant that drops one is killed by the `shapes` and `passes`
  expectations whether or not the count is checked.
- *a run that stops early passes*, round 3's survivor: the `stopped` case now
  makes the check stop on its own error from the environment (BASH_ENV), as
  the PATH-shim cases do for git, and kills this mutant (`mutants4.txt`;
  under bash 3.2, which this ran; not run under bash 5).

## Gate (each figure with its head)

- Round 1, `gate.txt` (`gates.sh`), at `3ee1766f`: 22 of 22 steps.
- Round 2, `gate2.txt` (`gates-round2.sh`), at `25336ee5`: the 8 steps that
  read what it changed.
- Round 3, `gate3.txt` (`gates.sh`), at `836ef942` (the scripts of
  `b0fd5157`): 22 of 22 steps, the tree clean before and after, in one
  suite-lock slot after `mutants3.txt`.
- Round 4, `gate4.txt` (`gates.sh`), at `dbb15990` (the scripts of
  `4e2bdacb`), on a `git clone --no-local`: 22 of 22 steps, the clone clean
  before and after, in one suite-lock slot after `mutants4.txt`.
- Round 5, `gate5.txt` (`gates.sh`), at `f9a4b0db` (the code head), on a
  `git clone --no-local`: 22 of 22 steps, the clone clean before and after. Round 5 changes the test's diagnosis only,
  not the check, so `mutants4.txt` still holds for the check.
- Round 6, `gate6.txt` (`gates.sh`), at `d7bc404c` (the code head), on a
  `git clone --no-local`: 22 of 22 steps, the clone clean before and after. Round 6 also changes the test only.
- The brief's path grep over `git diff d2b2ca75..HEAD` prints 0, and the
  dispatcher's per-commit rule prints 0 for every commit but `3ee1766f`
  (6, above).

`bash-regex-status.txt` (fixed in `1f6e5345`: both commands now set the
pattern): bash 3.2.57 returns 2 for a pattern it cannot compile, and an
`if` reads that as "no match".

## Open

- **LOCAL-PATHS-EXIT-TRAP (queued):** eight more scripts install an EXIT
  trap, and whether each exits 0 after a `set -u` stop depends on its
  options and its trap's last status: `check-no-secrets.sh` (`:61`,
  `:177`, `:386`; a public-repo guard, so first), `check-xfx-identity.sh`
  (`:171`), `check-preview-contract.sh` (`:957`), `test-check-no-secrets.sh`
  (`:69`), `test-preview-contract.sh` (`:35`), `test-gate-env.sh` (`:109`),
  `smoke.sh` (`:413`) and `fxcap/capture.sh` (`:849`) (line numbers at
  `f133ce6c`: `git show f133ce6c:scripts/<file> | grep -n 'trap .* EXIT'`).
- Bash 5 has not run this code; the Linux CI row is its first run.
- The trigger: a macOS 15 host looping the repro is devops-engineer work.
