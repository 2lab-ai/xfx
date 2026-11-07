---
title: SECRETS-ARGMAX receipt -- check-no-secrets greps in batches under ARG_MAX
type: receipt
tags: [xfx, phase5, ci, secrets-gate, receipt]
---

# SECRETS-ARGMAX: the secrets check outgrew one command line

`scripts/check-no-secrets.sh` is the secrets gate of CI (`ci.yml`,
`release.yml`, `preview.yml`) and of every unit's local gate. At the base,
`main` @ `24a6b912`, `grep_names` (`scripts/check-no-secrets.sh:191-214@24a6b912`)
ran one `grep -n -E -e <pattern> -- "$@"` per pattern (`:205`) with every
tracked regular file as an operand, `./`-qualified by `scan` (`:242`, called at
`:249`). With about 12 000 tracked files that one `exec` was close to the
limit:

| | bytes |
|---|---|
| tracked names with their NULs (`git ls-files -z \| wc -c`, 12 060 files) | 897 431 |
| `./` per name | 24 120 |
| one 8-byte argv pointer per name | 96 480 |
| total | 1 018 031 |
| macOS `getconf ARG_MAX` | 1 048 576 |
| room left, before the environment | 30 545 |

A unit branch (TG-REDRAW) with more captures failed it with "Argument list
too long". That failure is closed -- the shell's exec failure is status 126,
`>= 2`, so `fail` -- but it fails every merge, and the pending units add
hundreds of capture files.

## The fix (`fix/p5-secrets-argmax`)

Line numbers at the head of this branch.

- `batch_bytes=65536` (`scripts/check-no-secrets.sh:196`): the most bytes of
  names one `grep` gets. Linux's limit is a quarter of the stack limit and
  never under 128 KiB; 64 KiB leaves the other half of the smallest one to
  the environment and `grep`'s own arguments. It is an assignment, not read
  from the environment.
- `batch_bounds` (`:203-218`) prints one `start:count` per batch, in order. A
  name costs its bytes plus NUL plus pointer, counted with `local LC_ALL=C`
  so `${#name}` is bytes, not UTF-8 characters (checked on bash 3.2.57 and
  5.2.15: 4 characters / 5 bytes for `café`, and the locale restored after
  return). A name bigger than the budget is a batch by itself.
- `grep_names` (`:229-282`):
  - refuses no operands, as before;
  - requires the bounds to cover every operand once and in order, or fails
    with `the batches cover N of M files` (`:239-251`) -- a lost batch is
    never a silent pass;
  - runs one `grep -H -n -E -e <pattern> -- <batch>` per batch (`:265`),
    standard input still `/dev/null`;
  - judges each pattern's batches together: any batch with status `>= 2`
    (unreadable file, or a `grep` that did not run or finish: 126 E2BIG, 127,
    a signal) fails the pattern, and every batch's hits are printed under it
    (`:266-281`). The messages are the base's.
  - `-H` is new: `grep` given one file prints its matches without the file
    name, and a batch can be one file. The in-script control caught this on
    the first run (`1:sk-…` with no name), see below.
- The in-script control (`:372-375`) now runs with `batch_bytes=1`, one name
  per batch. So on every run the check proves that the first batch (`./-`)
  and the last (the quoted name) are read, and that an unreadable file in a
  non-final batch (`grep_names ./does-not-exist ./-`) is a failure.

Why not `xargs`: it maps every child status 1-125 to 123, so "no match" (1)
and "unreadable" (2) are the same exit; BSD `-s` counts the environment and
GNU's does not; and it stops at a 255. Slicing `"${@:start:count}"` in bash
keeps each batch's own status, works on bash 3.2, and needs no new tool.

## The test: `scripts/test-check-no-secrets.sh`

It builds a scratch git repository under `$TMPDIR`. Each name is 368 bytes as
an operand (a 126-byte directory and a 238-byte file, `./`-qualified), and it
makes `ceil(1.25 x min(ARG_MAX, 6 MiB) / 376)` of them. Then it proves the
precondition: `exec` with every name fails with E2BIG. It checks the shell's
"Argument list too long" in the C locale, not the status, because bash 5.2
reports 126 but bash 3.2 under `set -e` reports 1 (measured here; in a
command substitution, where the check's `grep` runs, 3.2 reports 126 too).
It copies the check in and runs it with a fake key on standard input. Every
case also requires that the output says neither "Argument list too long" nor
"(standard input)".

| case | requires |
|---|---|
| `clean` | exit 0, `ok (<n> tracked file(s) scanned)` |
| `first-batch` | key in `+first-batch` (the first operand) found as `./+first-batch:1:` |
| `last-batch` | key in `zz-last-batch` (the last operand) found |
| `dash-n-name` | key in a root file named `-n` found as `./-n:1:` |
| `unreadable-middle` | mode-000 file at the middle index: `grep could not read every file for the 'OpenAI-style key' pattern:` and its name |
| `every-file` + `-count` | key in every file but the check's copy: every one reported (3 489 of 3 489 on macOS, 6 975 of 6 975 on Linux) |
| `nothing-tracked` | empty repository refused, `no files to scan; …` |
| `only-a-symlink` | repository tracking one symlink refused the same way |

The keys are `sk-` plus 40 `A`s, assembled at run time; they exist only in
the scratch repository. In the logs here they are `<fake-key>`. The test
refuses to run as root, because root can read a mode-000 file.

## RED

The final test (`28f5d410`) against the base's check, `24a6b912` (macOS:
`red-macos.txt`, the driver's `base-script` run; Linux: `linux.txt`): 7
problems, 2 cases passed. Each large-tree case shows
`/usr/bin/grep: Argument list too long` under `grep could not read every
file`. `every-file` reports `0 of the 3489 planted keys`. Only
`nothing-tracked` and `only-a-symlink` pass.

## GREEN

| where | `check-no-secrets.sh` on this repository | `test-check-no-secrets.sh` |
|---|---|---|
| macOS 26 arm64, bash 3.2.57, BSD grep, ARG_MAX 1 MiB | ok (12 061 files), 24 s (the base's check: 27 s) | ok, 9 cases, 3 490 names, 52 s (`green-macos.txt`) |
| Linux (Docker `rust:1.99-bookworm`, aarch64, uid 1000, no network), bash 5.2.15, GNU grep 3.8, ARG_MAX 2 MiB | ok (12 061 files), 2 s | ok, 9 cases, 6 976 names, 27 s (`linux.txt`) |

## Mutants

`mutants.py` runs on a `git clone --no-local` of `28f5d410` in the
scratchpad, never on the worktree. Each mutant is one exact-once edit to the
clone's `scripts/check-no-secrets.sh`, followed by the clone's
`scripts/test-check-no-secrets.sh`, and the script is restored afterwards.
The unmutated check must pass first (`control: test exit 0`). A mutant
counts as caught only if the precondition held and a case failed.
`mutants.txt` has the results: 8 of 8 caught.

| mutant | the edit | cases that fail |
|---|---|---|
| `base-script` (the RED) | the whole check as at `24a6b912` | clean, first-batch, last-batch, dash-n-name, unreadable-middle, every-file (+count) |
| `nonfinal-status-ignored` | a batch's status `>= 2` counts only if it is the last batch | clean (the in-script control), unreadable-middle |
| `second-batch-dropped` | the second batch skipped, for every pattern | every-file |
| `first-batch-only` | `break` after the first batch | clean, last-batch, unreadable-middle, every-file |
| `batch-edge-lost` | each batch's slice starts one name late | clean, first-batch, every-file |
| `with-filename-dropped` | `grep -n` without `-H` | clean (the in-script control: `1:` hits with no name) |
| `last-bound-lost` | `batch_bounds` never prints the last batch | all 7 large-tree cases (the coverage guard: `the batches cover`) |
| `last-bound-lost-unguarded` | the same, with the coverage guard disabled | clean, last-batch, every-file |

The brief's three mutants are `nonfinal-status-ignored`, `second-batch-dropped`
and `first-batch-only`. `second-batch-dropped` gets past every case except
`every-file`: no single planted file sits in that batch. That is why the
test plants a key in every file.

## Gates (scripts at `28f5d410`; the receipt commit adds files under `.prd/` only)

| gate | result |
|---|---|
| `scripts/check-no-secrets.sh` | ok (12 061 tracked file(s) scanned) |
| `scripts/test-check-no-secrets.sh` | ok (9 cases) |
| `scripts/check-xfx-identity.sh` | ok (12 061) |
| `scripts/check-no-stubs.sh` | ok |
| `cargo fmt --check` | ok |
| `scripts/check-preview-contract.sh` (ci.yml changed) | ok |
| `scripts/test-preview-contract.sh` | ok (24 mutants caught) |

No Rust source changed, so clippy and `cargo test` were not run.

## Wiring

- `.github/workflows/ci.yml:125-131`: a step after `No credentials in tracked files` runs
  `./scripts/test-check-no-secrets.sh` on all four native rows. GitHub-hosted
  runners are not root.
- The gate lists in `README.md` and `CONTRIBUTING.md`, and the scripts
  listing in `.prd/02-architecture.md`.
- `docs/parity.md`: no row. This is tooling, not fx behaviour.

## Files

- `red-macos.txt`, `green-macos.txt`: the test's output on macOS.
- `linux.txt`: the Linux run (check, test, and the RED test with the base check).
- `mutants.py`, `mutants.txt`: the mutant driver and its results.

The logs went through `scripts/fxcap/redact.py`: `<tmp>` is the gate's
`TMPDIR`, `<scratch>` the session scratchpad, `<local-path>` any other local
path. The fake key is `<fake-key>`, because a committed `sk-` shape would be
a finding of this very check.
