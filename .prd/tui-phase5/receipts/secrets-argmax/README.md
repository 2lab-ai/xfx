---
title: SECRETS-ARGMAX receipt -- check-no-secrets greps in batches under ARG_MAX
type: receipt
tags: [xfx, phase5, ci, secrets-gate, receipt]
---

# SECRETS-ARGMAX: the secrets check outgrew one command line

Every number below is from a log in this folder; each is cited as
`file: section`. A claim with no log here is marked *(not measured here)*
with its source. Line numbers are at the code head, `f018d86d`, unless they
name another commit.

`scripts/check-no-secrets.sh` is the secrets gate of CI (`ci.yml`,
`release.yml`, `preview.yml`) and of every unit's local gate. At the base,
`main` @ `24a6b912`, `grep_names` (`scripts/check-no-secrets.sh:191-214@24a6b912`)
ran one `grep -n -E -e <pattern> -- "$@"` per pattern (`:205@24a6b912`), with
every tracked regular file as an operand, `./`-qualified by `scan`
(`:242@24a6b912`). The base's names, as one `grep`'s operands
(`platform-macos.txt: the tracked names of 24a6b912`):

| | bytes |
|---|---|
| names with their NULs (`git ls-tree -r -z --name-only 24a6b912 \| wc -c`), 12 060 names | 897 431 |
| `./` per name | 24 120 |
| an 8-byte pointer per name | 96 480 |
| total | 1 018 031 |
| macOS `getconf ARG_MAX` | 1 048 576 |
| room, before the environment | 30 545 |

A unit branch with more captures (TG-REDRAW) failed that `exec` with
"Argument list too long", and the pending units add hundreds of capture
files *(not measured here: the unit brief)*. The failure is closed: the
check takes any status of 2 or more as a failure. But it fails every merge.

## The fix

- **Batches** (round 1, `f244892f`):
  - `batch_bytes=65536` (`:202`) is the most bytes of names one `grep` gets.
    Linux's limit is a quarter of the stack limit and never under 128 KiB
    *(not measured here: the kernel's `fs/exec.c`; the Linux log's ARG_MAX,
    2 097 152, is 8 MiB / 4)*, so 64 KiB leaves half of the smallest one to the
    environment and to `grep`'s own arguments. It is a plain assignment, not read from the environment.
  - `batch_bounds` (`:209-224`) prints one `start:count` per batch, in order.
    A name costs its bytes plus its NUL plus its pointer. It counts with
    `local LC_ALL=C`, so `${#name}` is bytes, not UTF-8 characters: `café` is
    4 in `C.UTF-8`, 5 under `local LC_ALL=C`, and 4 again after the function
    returns, on bash 3.2.57 and 5.2.15 (both platform logs: `${#name} in a
    UTF-8 locale ...`). A name bigger than the budget is a batch of its own.
  - `grep_names` (`:235-288`):
    - refuses no operands, as before;
    - requires the bounds to cover every operand once and in order
      (`:245-257`), or fails with `the batches cover N of M files`;
    - runs `grep -H -n -E -e <pattern> -- <batch>` once per batch (`:271`),
      with standard input still `/dev/null`;
    - fails the pattern if any batch has status 2 or more (`:272-274`). That
      covers a file `grep` could not read, an `exec` that failed (126 is
      E2BIG), a command not found (127) and a `grep` killed by a signal. Every
      batch's hits are printed under the pattern, with the base's messages
      (`:275-287`).
    - `-H`, because `grep` given one file prints its hits without the file's
      name, and a batch can be one file.
  - The in-script control (`:379-382`) runs with one name per batch.
- **Round 2** (`333db903`):
  - `unset GREP_OPTIONS` (`:18-22`). BSD grep puts that variable in front of
    every argument list. With `GREP_OPTIONS=--exclude=*.md`, macOS's grep
    (2.6.0-FreeBSD) prints the key in `notes.txt` and not the same key in
    `notes.md`, and exits 0. GNU grep 3.8 prints both (both platform logs:
    `GREP_OPTIONS=--exclude=*.md and this grep`). Before this commit, on macOS,
    a key in a Markdown file passed the check silently. This predates the
    batching.
  - The in-script control's file `grep` cannot read is a batch before one
    holding a key. That key must now be printed under the read failure
    (`:417-420`), as the comment said.

Why not `xargs`: it maps every child status from 1 to 125 to 123, so "no
match" (1) and "unreadable" (2) become the same exit. BSD's `-s` counts the
environment and GNU's does not. It also stops at a 255 *(not measured here:
the BSD and GNU `xargs` manuals)*. Slicing
`"${@:start:count}"` keeps each batch's own status, runs on bash 3.2, and
needs no new tool.

## The test: `scripts/test-check-no-secrets.sh`

It builds a scratch git repository under `$TMPDIR` and sizes it to
`ceil(1.25 x min(ARG_MAX, 6 MiB) / per-name cost)` names. Then it proves the
names overflow: `exec` with all of them must fail with "Argument list too
long" (`:178`). It checks the message, not the status, because bash 3.2 under
`set -e` reports E2BIG as 1 where bash 5.2 reports 126
(`platform-macos.txt` and `platform-linux.txt`: `the status bash gives an exec
that fails with E2BIG`). The check's `grep` runs inside a command
substitution, where both report 126.

Every name in the tree's directories carries a tab, a newline and 0xFF. macOS
refuses a name that is not UTF-8, so there the names carry U+00E9 instead,
and the test says so. As a result every batch boundary falls between two
such names. The test copies the check in and runs it with a fake key on
standard input. Every case requires that the output says neither "Argument
list too long" nor "(standard input)".

| case | requires |
|---|---|
| `clean` (`:246`) | exit 0, `ok (<n> tracked file(s) scanned)` |
| `first-batch` (`:250`) | key in `+first-batch`, the first operand, found as `./+first-batch:1:` |
| `last-batch` (`:255`) | key in `zz-last-batch`, the last operand, found |
| `dash-n-name` (`:260`) | key in a root file named `-n`, found as `./-n:1:` |
| `unreadable-middle` (`:269`) | mode-000 file at the middle index: `grep could not read every file for the 'OpenAI-style key' pattern:` and its name |
| `grep-options` (`:277`) | key in a tracked `notes.md` with `GREP_OPTIONS=--exclude=*.md`: found |
| `grep-signal`, `grep-126`, `grep-127` (`:280-310`) | a `grep` shim on `PATH` stops the batch naming the file at a third of the tree, by SIGKILL or by exiting 126 or 127; the check must fail with the read-failure header. The shim leaves a file when it fires, or the case fails as proving nothing |
| `every-file` + `every-file-set` (`:312-343`) | a distinct key in every file but the check's copy. The reported lines, sorted, must equal the expected ones, so each key is reported exactly once. A count would not do: the batches are the same size, so a batch read twice could hide one not read |
| `nothing-tracked` (`:355`) | empty repository refused: `no files to scan; ...` |
| `only-a-symlink` (`:364`) | repository tracking one symlink refused the same way |

The keys are `sk-` plus 40 `A`s, with a 6-digit suffix in `every-file`. They
are assembled at run time and exist only in the scratch repository. In the
logs here they are `<fake-key>`. The test refuses to run as root, because
root can read a mode-000 file.

## RED

- **The base's check under this test** fails 11 of 13 cases on both
  platforms, from E2BIG. The three shim cases fail as "never saw the batch",
  because the shim cannot even be exec'd (`platform-macos.txt` and
  `platform-linux.txt`: `RED: the test, with the base's check`; `mutants.txt`:
  `base-check`).
- **Round 1's check (`37a5d09f`) under this test**: on macOS only
  `grep-options` fails, which is the GREP_OPTIONS hole (`platform-macos.txt`:
  `the test, with round 1's check`; `mutants.txt`: `round1-check`). On Linux
  all 13 pass, because GNU grep ignores the variable (`platform-linux.txt`,
  same section).
- **Round 1's test is weaker.** The mutants `third-batch-is-second` and
  `status-eq-2` survive it and are caught by this test (`mutants.txt`: `under
  the round-1 test`).

## GREEN

| where | the check on this repository (`f018d86d`, 12 067 names) | the base's check on it | the test |
|---|---|---|---|
| macOS: Darwin arm64, uid 501, `LANG=C.UTF-8`; bash 3.2.57, BSD grep 2.6.0-FreeBSD, ARG_MAX 1 048 576 | ok, 51 s | ok, 56 s | ok, 13 cases, 3 473 names, 167 s |
| Linux: Docker `rust:1.99-bookworm` (run as `docker run --rm --network none --user 1000:1000 -e LANG=C.UTF-8`; the log shows the uid and `LANG`), aarch64; bash 5.2.15, GNU grep 3.8, ARG_MAX 2 097 152 | ok, 3 s | ok, 1 s | ok, 13 cases, 6 959 names, 66 s |

Sources: `platform-macos.txt` and `platform-linux.txt`, sections `platform`,
`scripts/check-no-secrets.sh on this copy`, `the base's check ... on this
copy` and `scripts/test-check-no-secrets.sh`. The base's check still passes
on the repository itself, which fits. The tree past ARG_MAX is the RED. The
two checks were timed back to back in one run. The round-1 receipt's macOS
times (24 s, 27 s, 52 s) had no log; these replace them.

## Mutants

`mutants.py` runs on a `git clone --no-local` of `f018d86d` in the scratchpad,
never on the worktree. Each mutant is one exact-once edit to the clone's
`scripts/check-no-secrets.sh`, followed by the clone's test; the script is
restored afterwards. The unmutated clone must pass first (`control: test exit
0`). A mutant counts as caught only if the precondition held and a case
failed. Results are in `mutants.txt`, run on macOS:

| mutant | the edit | cases that fail |
|---|---|---|
| `base-check` (the RED) | the whole check as at `24a6b912` | 11 of 13: all but `nothing-tracked`, `only-a-symlink` |
| `round1-check` | the whole check as at `37a5d09f` | `grep-options` |
| `nonfinal-status-ignored` | a batch's status of 2 or more counts only in the last batch | `clean` (the in-script control), `unreadable-middle`, `grep-signal`, `grep-126`, `grep-127` |
| `second-batch-dropped` | the second batch skipped, for every pattern | `clean` (the in-script control: the key after the read failure), `every-file-set` |
| `first-batch-only` | `break` after the first batch | `clean`, `last-batch`, `unreadable-middle`, `grep-options`, the three shim cases, `every-file-set` |
| `batch-edge-lost` | each batch's slice starts one name late | `clean`, `first-batch`, `every-file-set` |
| `with-filename-dropped` | `grep -n` without `-H` | `clean` (the in-script control: `1:` hits with no name) |
| `last-bound-lost` | `batch_bounds` never prints the last batch | 11 of 13 (the coverage guard: `the batches cover`) |
| `last-bound-lost-unguarded` | the same, with the coverage guard disabled | `clean`, `last-batch`, `grep-options`, `every-file-set` |
| `third-batch-is-second` | the third batch replaced by the second: one batch read twice, one not read, the same count | `every-file-set` |
| `status-eq-2` | only a status of exactly 2 counts, the half-revert | `grep-signal`, `grep-126`, `grep-127` |
| `grep-options-kept` | `unset GREP_OPTIONS` removed | `grep-options` (on macOS; GNU grep would ignore it) |
| `break-after-unreadable` | the first batch `grep` cannot read ends the pattern's scan | `clean` (both in-script read-failure controls), `unreadable-middle` |
| `unquoted-slice` | `${@:start:count}` unquoted, so a name splits on its tab and newline | 11 of 13 |

12 of 12 mutants are caught. Your three are `nonfinal-status-ignored`,
`second-batch-dropped` and `first-batch-only`. Under round 1's test
(`37a5d09f`), `third-batch-is-second` and `status-eq-2` both survive, with
test exit 0 (`mutants.txt`: `under the round-1 test`). `every-file-set` and
the shim cases are what catch them.

## Gates

`gates.txt`, from `gates.sh`, holds every gate with its output, exit status
and time, run in the worktree. Its first line names the commit it ran at: the
commit before the one that adds `gates.txt`, which changes nothing else.
Gates: `cargo fmt --check`, `check-no-secrets`, `test-check-no-secrets`,
`test-gate-env`, `check-xfx-identity`, `check-no-stubs`, `check-preview-contract`,
`test-preview-contract`, `actionlint .github/workflows/ci.yml`. Its
`what changed` section shows no Rust source changed, so clippy and `cargo
test` were not run.

## Round 3 (SECRETS-ARGMAXc): the other gates and GREP_OPTIONS

**The sweep** (`c-sweep-macos.txt`; the same on Linux in `c-linux.txt`,
second half; `c-sweep-env.sh` ran both). It covers every script `ci.yml`
runs, plus every `scripts/test-*.sh`, with its grep lines, its other tools,
and whether it unsets `GREP_OPTIONS`.

**Steerable, now fixed.** At `43ae27de` (the test alone) each gate below
passes one planted violation under one `GREP_OPTIONS` value on macOS
(`c-gate-env-macos.txt`, first run: 5 problems, every gate `ok`). At
`64f5f820` each unsets the variable and refuses it (second run: 5 cases):

| gate | planted violation (`scripts/test-gate-env.sh`) | value | the unset |
|---|---|---|---|
| `check-no-stubs.sh` | `todo!()` in `src/zz_planted.rs` (`:84-92`) | `-q`: every marker grep prints nothing | `:39-43` |
| `check-preview-contract.sh` | README without its `brew install` lines (`:107-110`) | `-v`: "the README carries the command" holds for any README with another line | `:43-47` |
| `test-preview-contract.sh` | the check's epoch-gate message changed, so `gate-removed` fails for another reason (`:119-132`) | `-v`: "the log names this mutant's break" holds for any log with another line; it reported 24 caught | `:24-28` |
| `check-tui-preflight-cost.sh` | a `cargo` on PATH printing a run where 1 of 2 tests ran (`:141-164`) | `-v`; and BSD grep's `-v -o` prints every non-matching line, so the measurement count passes too | `:37-41` |
| `check-usage-clock.sh` | the same `cargo`, 0 tests ran (`:166-171`) | `-v` | `:24-28` |

`check-no-secrets.sh` was fixed in round 2 (`:18-22`, its own `grep-options`
case).

- **The plants are real.** With every unset removed and every case's
  value emptied, the test passes: each gate refuses its plant
  (`c-mutants.txt`: `plants, ...`). On Linux, GNU grep 3.8 ignores the
  variable, so the test passes with and without the five unsets
  (`c-linux.txt`: `at the head` and `as at 43ae27de`).
- **Mutants** (`c-mutants.py` on a `--no-local` clone of `64f5f820`, after an
  unmodified control run that passed): each gate's unset block removed. Each
  is caught by its own case only (`c-mutants.txt`): 5 of 5, `no-unset-no-stubs` failing only `no-stubs`, and so on for `preview-contract`, `test-preview-contract`, `tui-preflight-cost` and `usage-clock`.
- **Wiring:** `ci.yml:133-138` runs `./scripts/test-gate-env.sh` after the
  secrets test. It takes 118-120 s on macOS, most of it
  `test-preview-contract`, and 24-26 s on Linux (`c-gate-env-macos.txt`,
  `c-linux.txt`). It is also added to the gate lists in `README.md`,
  `CONTRIBUTING.md` and `.prd/02-architecture.md`.

**Not steerable, one line each:**
- `check-xfx-identity.sh`: all 12 values tried fail one of its own controls
  or report the planted path (`c-sweep-macos.txt`: `under each
  GREP_OPTIONS`). Its controls run the same grep both ways: must find in the
  dirty repository, must not in the clean one. A pattern given in
  `GREP_OPTIONS` turns their positional patterns into file names.
- `smoke.sh`: `expect_absent` and `expect_contains` are the same `grep -qF
  <needle> <file>`. The values that hide a present key, `-x` and `-m 0`, hide
  `expect_contains`' present needle too, so the run fails (`c-sweep-macos.txt`:
  `smoke.sh`). `:666` and `:768` have the same shape.
- `test-check-no-secrets.sh` (`:55`) and `test-gate-env.sh` (`:38`) unset it
  themselves.
- `GREP_COLOR`, `GREP_COLORS`: they act only with `--color`, which no gate
  passes. Output is identical with and without them (`c-sweep-*`: `GREP_COLOR
  and GREP_COLORS`).
- BSD `sed`, `awk` and `sort` read only locale variables, plus
  `POSIXLY_CORRECT` for `awk`'s `sub`/`gsub` backslashes *(not measured here:
  their macOS man pages)*. `ruby`'s `RUBYOPT` changes what runs only by `-r` of
  a file on disk *(not measured here)*. `cargo`, `gh`, `jq` and `actionlint`
  were not swept.

**Leads, measured and not fixed here: git reads its environment too.** On
both platforms (`c-sweep-macos.txt` and `c-linux.txt`, last three sections):
- `check-no-secrets.sh` with `GIT_INDEX_FILE` naming an index that lacks the
  file holding a key: `ok (2 tracked file(s) scanned)`, exit 0.
- `check-xfx-identity.sh` with `GIT_CONFIG_COUNT`/`KEY_0`/`VALUE_0` setting
  `core.attributesFile` to `*.lock -diff`: `git grep -I` skips `Cargo.lock`,
  and the retired name in it passes, exit 0. A `~/.gitconfig` or
  `.git/info/attributes` saying the same would do it without any environment
  *(not measured here)*.
- With `GIT_DIR` exported at a decoy repository, `check-xfx-identity.sh`
  writes its control files into the decoy's index, and
  `check-preview-contract.sh` writes 5 commits into it. That is the EF1d
  class, live in two gates.

Why not here: this is a different mechanism from GREP_OPTIONS. Unsetting
`GIT_INDEX_FILE` changes what a gate sees inside a git hook, where git sets
it to the index being committed, and the attributes path also arrives
through config that is not environment. It wants its own unit, cases and
review.

## Wiring

- `.github/workflows/ci.yml:125-131`: a step after `No credentials in tracked
  files` runs `./scripts/test-check-no-secrets.sh` on all four native rows.
  The test fails, rather than passes, on a runner that is root. `release.yml:78-79` and
  `preview.yml:289-290` run the same `check-no-secrets.sh`, so they take the
  fix without a change.
- The gate lists in `README.md` and `CONTRIBUTING.md`, and the scripts
  listing in `.prd/02-architecture.md`.
- `docs/parity.md`: no row. This is tooling, not fx behaviour.

## Not fixed here

`scripts/check-xfx-identity.sh:56` pipes `git ls-files` into `grep -i`, so on
macOS that grep reads `GREP_OPTIONS` too. That check is another gate with its
own positive control, and I have not measured whether a `GREP_OPTIONS` value
gets past it. It reads names on standard input, so it has no ARG_MAX issue.

## Files

- `platform.sh`, `platform-macos.txt`, `platform-linux.txt`: the platform
  receipt and its two runs, each on a disposable copy.
- `mutants.py`, `mutants.txt`: the mutant driver and its results.
- `gates.sh`, `gates.txt`: the gates.
- `c-*`: round 3. `c-sweep-env.sh` (the sweep), `c-gate-env-runs.sh` (the
  macOS RED and GREEN), `c-linux.sh` (the Linux run, which ends with the
  sweep), `c-mutants.py`, and their logs.
- `publish.py`: how the logs got here. One edit beyond redaction: the
  driver's first line in `mutants.txt` named the session shell's `grep`, a
  wrapper the tests never run. `publish.py` replaced it with the `grep` the
  tests do run under bash, `/usr/bin/grep`, BSD grep 2.6.0-FreeBSD
  (`platform-macos.txt: platform`).
- The logs went through `scripts/fxcap/redact.py`: `<tmp>` is the gate's
  `TMPDIR`, `<scratch>` the session scratchpad, `<copy>` the container's
  copy, and `<local-path>` any other local path. The fake key is
  `<fake-key>`, because a committed `sk-` shape would be a finding of this
  very check. The Linux names' 0xFF bytes are U+FFFD.
