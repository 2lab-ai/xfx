---
title: 0.0.16 delta -- AT-FOLDERS round 10 (a walk a change of roots stopped counts as whole once it is over)
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, input, file-picker]
---

# AT-FOLDERS round 10

Round 9's head `48f96022` was reviewed by two engines:
- gpt6 approved.
- opus approved with fixes: one MUST, confirmed by the dispatcher at the
  pin, and two SHOULDs.

The dispatcher's gate on `48f96022` passed 21 of 21 steps.

fx's lines are quoted in `../../at-height/fx-src.txt`, extended from line
2554. Lines 1-2553 are byte-identical to round 9's (checked with `cmp`).
`fx-src.txt:N` below means that file.

| commit | what |
|---|---|
| `4107f213` | RED (`red-unit.txt`): 1 case fails on round 9's code; 1 pins the before-the-poll case |
| `079bc6f4` | the fix, the quote, `docs/parity.md`, rounds 8 and 9 receipts corrected -- **the code head** |
| `4dc07715` | the ledger's 6 `shell.rs` refs re-pointed, each `src/tui/shell.rs:40403` to `:40458` (`refs.txt`: 16 checked, 0 wrong) |
| `4278fe27` | `mutants.py` -- **the gate's commit** |
| (receipt commit) | this folder; `mutants.py` gains round 7's `superseded-never-cleared`, re-texted (§3) |

## 1. A walk a change of roots stopped counts as whole once it is over (opus r9 MUST)

**fx at the pin:** a change of scope stops no walk.
- `refreshScopeEpoch` (`file_index.zig:480-486`, `fx-src.txt:2556-2562`)
  hands the new roots to `refreshOwnedRoots` (`:488-497`,
  `fx-src.txt:2083-2092`). While a thread runs, that only replaces or
  coalesces the pending scope (`replacePendingRoots`, `:511-523`,
  `fx-src.txt:2106-2118`).
- The running walk goes on and is adopted when whole (`joinThreadIfDone`,
  `:425`). From then on fx's index is `.ready`.

**Before:** xfx's `set_extra_roots` stops the walk, so its `Ready` was never
taken in and `ever_whole` stayed false. So: A's walk stopped by B's install,
then the poll ran, with B's walk running, then the session's first `@`. xfx
skipped the refresh (2 walks in all), where fx asks for it (3).

**Now:**
- `poll` sets `ever_whole` where it ends a walk that a change of roots
  stopped (file_index.rs:1222-1223, just before `superseded` is cleared).
- **Why there, not in `set_extra_roots`:** that poll is where xfx's model
  ends the stopped walk, and so where fx's would be adopted. A first `@`
  before it meets a refresh that is queued anyway, beside the change's own,
  so no count differs between the two places.
  `a_first_at_before_the_poll_that_ends_a_stopped_walk_coalesces` pins that
  case at 2 walks.
- **The residual, named in the code comment, the parity row and here:** a
  stopped first walk that would have failed leaves fx loading (`:432-438`,
  `fx-src.txt:2309-2315`). xfx, having stopped it, cannot know, and counts
  it whole. This ends when AT-SCOPE-INTERIM lets the walk run on, as fx's
  does.

**RED** (opus's recipe):
`a_walk_a_change_of_roots_stopped_counts_as_whole_once_it_is_over`. adopt(A)
with A's walk held; adopt(B); `settle_band`; B's walk held; `@plan`; B's
`Ready`; `settle_band`. Walks + 3, where round 9's code gave + 2.

## 2. The receipt lines (opus r9 SHOULDs)

- Round 9's commit table said "the ledger's 6 `file_index.rs` refs". All
  six were `src/tui/shell.rs:40363` to `:40403`; `git show c98aefd4` shows
  six hunks, each that one change. Corrected in place.
- Round 9 §4 ("changes one count only") and round 8 §1 ("could change no
  count") now name the second count: the first `@` after the poll that ends
  a stopped walk.

## 3. Mutants (`mutants.txt`)

One `--no-local` clone at `4278fe27`, under the suite lock: **92 run, 92
killed**.
- Round 10's `stopped-walk-not-whole` removes the assignment. It is killed
  by the RED case.
- Rounds 9, 8, 7, 6, 5, 4, 3, 2 and 1 were re-run at this head: 3, 5, 17, 2,
  12, 10, 5, 8 and 28 killed.
- One more BROKEN than round 9: round 7's `superseded-never-cleared`,
  whose anchor text this round's comment moved. It was re-texted in
  `round10/mutants.py` and run alone in the same clone, and was killed by
  `a_change_of_roots_during_a_walk_with_a_refresh_queued_walks_the_new_roots_once`.
  The other 16 BROKEN are round 7's (`../round7/mutants.txt`).

## 4. Pairs and sweep: not re-run

The change acts only when a change of roots has stopped a walk and the
session's first indexed `@` comes after the next poll. No scenario changes
roots while a walk runs before its first `@`. In `workspace-index`,
`/workspace add` comes before any `@`, with no walk running. And the rows
come from the walk either way.

## 5. The gate (`gate.txt`)

Every CI step (`../gates.sh`) ran in a `--no-local` clone at `4278fe27`, in
one hold of the suite lock, with `TMPDIR=<scratch>/atfg10`. **20 steps, 0
failed**, and the tree was clean before and after. The load was 7.45-33.28.
- fmt, and both clippies.
- `cargo test`: 5520 passed, 0 failed.
- `test-fault`: 5557 passed, 0 failed. Round 7's pty case passed.
- The checks and their self-tests; the fxcap self-tests (sweep 70 + 27,
  capture 68).
- Both release builds; smoke: 47 checks, 0 failures.
- smoke-tui: 37 scenarios and the oracle, 1168 checks, 0 failures.

## 6. Refs and paths

`refs.txt`: 16 checked, 0 wrong. `git diff af2b6137..HEAD` adds no local
path (the brief's grep prints 0 at every commit of this round). The new
cases take their directories from `tempfile`, and no test builds
`Runtime::new`.
