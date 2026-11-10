# local-paths-selftest-flake: the local-path check accounts for every line git gives

Unit LOCAL-PATHS-SELFTEST-FLAKE. The macOS arm64 row's run of
`scripts/test-check-no-local-paths.sh` failed its `shapes` case twice with
inputs that pass elsewhere: the check exited 1 but did not name one or two
planted files. Base `origin/main` `d2b2ca75`. Every claim below is tied to a
file in this folder.

| commit | what |
|---|---|
| `351aa2df` | the fix: one accounted `git grep -n -z` pass; a NUL anywhere is binary; `[[ =~ ]]` status 2 fails; the test's late-NUL plant and its `disagree` and `vanished` cases |
| `373811b0` | one look for missing files, after the scan |
| `54581752` | the test's `git` shim runs under bash |
| `3ee1766f` | this folder's scripts and records -- **the round-1 gate's head** (`gate.txt`) |
| `a72b738a` | round 2, after the second sighting: every line lands in one count or the scan fails; a mark needs the marker's letters by string match too; `badnum` and `gitfail`; the test says where a lost plant was lost |
| `25336ee5` | the round-2 gate's steps (`gates-round2.sh`) -- **the round-2 gate's head** (`gate2.txt`) |
| (this one) | this README, the mutant and gate records, the binary mutant's anchor moved to round 2's code |

## The failure, and that it varies

| run | commit | `shapes` | not named | file |
|---|---|---|---|---|
| 42863297443 attempt 2 (PR #92) | `4ad6405e` | FAIL | `w3/trace-report.md:1`, `w3/xfx-trace-40x120-report-1.md:1` | `ci-attempt2.txt` |
| 42863297443 attempt 3 | `4ad6405e` | ok | -- | `ci-attempt3-step.txt` |
| 42880581900 (main) | `dcfa5146` | FAIL | `w3/trace-report.md:1` | `ci-main-dcfa5146.txt` |

All on image `macos-15-arm64` `20261104.0514.1`, git 2.56.0, `/bin/bash`
3.2.57; every other plant named, in the third run the capture-named one in
`w3/` among them. `ci-history.txt`: the step on every macOS arm64 run since
the check landed. The macOS arm64 row is the only macOS row that runs the
script tests (`.github/workflows/ci.yml`, `script_tests`), so "arm64 only"
is "macOS only". The check and its test are the same in `d2b2ca75`,
`4ad6405e` and `dcfa5146`.

## Reproduction: not reproduced

`repro/tallies.txt`, each line the loop's own last line, all at `d2b2ca75`'s
check and test, on macOS 26.7.1 (Darwin 25.6.0), 18 cores, `/bin/bash`
3.2.57, git 2.56.0, under load averages between 8 and 34 (other workers'
suites; the samples are in the tallies file):

- the `shapes` case end to end (`repro/harness-shapes.sh`, `repro/loop.sh`):
  300 runs with a long `TMPDIR`, 300 with a short one, 300 with `TMPDIR`
  under `/var/folders` (a symlinked path, as a runner's), and 500 with the
  check instrumented to keep its listing and every per-file status
  (`repro/harness2.sh`, `repro/mirror.diff`): 0 failures in 1400;
- each stage alone: the tree built and `git add -A -f` (`repro/stageA.sh`)
  1000 times, `w3/` always indexed; the check on one fixed tree 999 times,
  and 499 more with `grep.threads=3` and a runner-like global config
  (`repro/stageC2.sh`, `repro/ci-like-gitconfig`); the listing grep and the
  two `w3/` per-file greps 3000 times with 3 threads (`repro/stageB.sh`);
  the shell's reading of the two missed lines 200000 times in one process
  and in 2000 more (`repro/stageD.sh`): 0 differences. (`stageB.sh` and
  `stageC2.sh` run on a fixed tree, `fixed/shapes` beside them: one
  `harness2.sh` run kept instead of removed. They are records of what ran,
  not standalone.)

What this machine cannot give is CI's macOS 15 libc, kernel and file system
and its smaller runner. Bash 5 was not run: it is not installed here, and
installing it would change `env bash` for other work on this machine. CI's
macOS row runs bash 3.2.57 (the image's readme, linked from each CI file
here), the one used here; the Linux row runs bash 5, and this unit's code
has not run under it yet.

## Where a lost plant goes unsaid (the check at `d2b2ca75`)

The CI trigger is not found. For `w3/trace-report.md` (authored text, so a
mark could excuse it) the old check has four places to lose it, and for the
capture `w3/xfx-trace-40x120-report-1.md` three (no mark is read in a
capture); none of them prints or fails:

1. **Not in the listing.** `scripts/check-no-local-paths.sh:214`: what `git
   grep -l` leaves out is never looked at.
2. **Listed, and the per-file grep gives no line.** `:234` runs `git grep -h
   -n` per file; status 1 with an empty stderr passes `:235`, the read loop
   at `:240` runs zero times. Forced by a `git` that gives nothing for that
   grep: exit 0 with the plant in the tree (`forced.txt`, `per-file-empty`).
3. **A line in which the shell finds no shape.** `:273-277` counts it as a
   placeholder line. Real content does it: a text file whose first 8000
   bytes hold no NUL (the probe at `:225` reads only those) with a NUL before
   the path on one line; git matches past the NUL, the shell's variable ends
   at it. Exit 0 (`forced.txt`, `late-nul`; and `per-file-shapeless`).
4. **A line the mark regex says is marked** (`:243`), authored files only.
5. And two that hold for any file: **a tracked file gone while git grep
   runs** -- git skips a file it cannot stat for ENOENT with no message
   (`git-grep-enoent.txt`, git v2.56.0 `grep.c:1954-1958`), and the look for
   missing files at `:400-409` comes before the scan at `:411` (`red.txt`,
   `vanished`: exit 0); and **a pattern bash cannot compile**: `[[ =~ ]]`
   returns 2, which every `if` and `while` at `:167`, `:243`, `:259`, `:263`,
   `:269` reads as "no match" (`bash-regex-status.txt`), inside `if !`
   (`:343`, `:411`) where `set -e` is off.

Which one fired on CI the old check did not record. So the line where the
plants drop is one of `:214`, `:240`, `:243` or `:273-277`, and the two that
disagree are git's first answer (the file holds a shape) and the next reading
of it (git's per-file answer, or the shell's). The check itself could pass a
real local path in a real tree: by 3 and 5 deterministically, by the others
whenever their trigger fires.

## The fix

- One `git grep -a -n -z` pass gives every matching line as
  `<path>NUL<number>NUL<text>LF`, and it is all the scan reads: no per-file
  grep is left to disagree with a listing (2 is gone).
- Every line lands in exactly one count -- refused, in a binary file, marked,
  or held only placeholders -- and the scan fails if the four do not add up
  to the lines git gave.
- A line git matched in which the shell finds no shape and takes no
  placeholder out is refused, labelled as such (3).
- A mark excuses a line only if the line holds the marker's letters by
  bash's string match as well as by the regex (4).
- A file with a NUL anywhere is a binary file, named and never read by the
  shell (3's real case).
- Missing files are looked for after the scan (5); a `[[ =~ ]]` status 2 or
  a line with no number fails the scan.
- On a failure the check prints what became of every line git gave, and the
  test prints, for each plant not named, whether the tree's index holds it,
  its size on disk and a direct `git grep -c` of it: the next sighting says
  whether `git add`, git's scan or the check's reading lost it.

What none of this catches is git's one pass leaving a matching line out (1):
nothing else in the check reads the tree. If the next sighting's diagnosis
says git did that, the answer is a second, independent reading, not more of
this one.

Source change (`git diff --numstat d2b2ca75 a72b738a -- scripts/`): the
check 156 lines in, 66 out; its test 111 in, 2 out.

## Look ahead: the real tree (`main-tree.txt`)

At `d2b2ca75` the old check passes (18935 files, 8 marked, 80 placeholder
lines), and one independent `git grep -c` with its own patterns (`count.sh`)
finds 88 matching lines in 17 files: 8 + 80, so no line was lost there. The
round-1 check gives the same counts on the same tree, so nothing on main
relied on a silent path, and no file there has a NUL past its first 8000
bytes with a hit (the new binary rule changes nothing). The round-2 check
passes the branch's own tree (`gate2.txt`, step 04).

## RED and GREEN

- `red.txt`: the round-1 self-test (`54581752`) against the check at
  `d2b2ca75`, in a `git clone --no-local`: `shapes` (the late-NUL plant not
  named), `disagree` and `vanished` fail; the other nine pass.
- `red2.txt`: the round-2 self-test (`a72b738a`) against the check at
  `d2b2ca75`: `shapes`, `disagree`, `vanished` and `badnum` fail, and the
  test prints where the lost plant was lost (`| lost: late-nul.md: in the
  index 1 time(s); ...`).
- `green.txt`, `green2.txt`: the same clone at `54581752` (12 cases) and at
  `a72b738a` (14 cases): every case passes.
- `forced.txt`: the three forced paths: exit 0 at `d2b2ca75`, exit 1 and the
  plant named at `373811b0`.

## Mutants (`mutants.py`, on a `git clone --no-local` under the suite lock)

`mutants-round1.txt` (at `54581752`): 6 of 9 killed; two of the three
survivors (a line with no number accepted, a scan that did not finish
passes) are why round 2 added `badnum` and `gitfail`. `mutants2.txt` (at
`a72b738a`): 13 mutants, 9 killed, 3 survive, 1 skipped; `mutants2b.txt`
is that skipped one, the binary mutant, re-run with its anchor moved to
round 2's code. The survivors, each a guard against a fault no test can
make on demand:

- *a pattern the shell cannot compile ignored*: the patterns are fixed in
  the check, so no case can hand bash one it cannot compile;
- *a mark by the regex alone*: the string-match guard matters only if the
  regex engine answers wrongly;
- *the count not checked*: the count fails only when the scan's own code
  loses a line; *one file's lines dropped, the count kept* shows it does
  then (killed: `shapes`, `passes`).

## Gate

- `gate.txt` (`gates.sh`, at `3ee1766f`, round 1): all 22 steps the common
  brief and CI list, 0 failed, the tree clean before and after.
- `gate2.txt` (`gates-round2.sh`, at `25336ee5`, round 2): the steps that
  read what round 2 changed -- `check-xfx-identity`, `check-no-stubs`,
  `check-no-secrets`, `check-no-local-paths`, the self-test three times and
  `test-gate-env` -- 0 failed. Round 2 changes no Rust, test or capture
  that the other steps read.
- The repo's path check over `git diff d2b2ca75..HEAD` prints 0, and so did
  the staged lines of each commit.
