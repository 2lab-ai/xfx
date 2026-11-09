---
title: 0.0.16 delta -- AT-FOLDERS round 11 (both residuals of stopping the walk named)
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, input, file-picker]
---

# AT-FOLDERS round 11

Round 10's head `1d859de5` was reviewed by two engines (gpt6, opus). Both
approved the code with the same MUST, which the dispatcher confirmed, and
opus gave two SHOULDs. The dispatcher's gate on `1d859de5` passed 21 of 21
steps.

This round changes comments, docs and receipts only. No code, so no new
mutant and no gate.

| commit | what |
|---|---|
| `f2a63b2d` | the wording: `file_index.rs`, the test comment in `shell.rs`, `docs/parity.md`, the receipts of rounds 8, 9 and 10 |
| `8360bc25` | the ledger's 6 `shell.rs` refs re-pointed, each `src/tui/shell.rs:40458` to `:40461` (`refs.txt`: 16 checked, 0 wrong) |
| (receipt commit) | this folder |

## 1. The MUST: a second residual

fx stops no walk on a change of scope. Its walk runs to its own end, and
fx is `.loading` until a walk is adopted (`currentState`,
`file_index.zig:333-341`, `../../at-height/fx-src.txt:2072-2080`;
`refreshOwnedRoots`, `:488-497`, `fx-src.txt:2083-2092`). xfx stops the
walk and counts it as whole at its next poll. That poll is xfx's choice, not
where fx adopts. Stopping the walk leaves two residuals. Each is a session's
first `@` that refreshes in xfx (3 walks) where fx, still loading, skips (2):
- **the still-running walk:** the `@` comes after xfx's poll, but before
  fx's walk would have ended. Round 10 named nothing here.
- **the would-have-failed walk:** a stopped first walk that would have
  failed leaves fx loading (`joinThreadIfDone`, `:432-438`). Round 10 named
  this one.

Both ended with AT-SCOPE-INTERIM, which lets the walk run on, as fx's does (`../../at-scope-interim/README.md`).

Where round 11 said it (each gone with AT-SCOPE-INTERIM):
- `file_index.rs`:
  - the `ever_whole` doc: set by a walk's `Ready`, or by the end of a walk
    that a change of roots stopped;
  - the `loading()` doc: no longer "as fx would have adopted it";
  - the `poll` comment: both residuals named, and "this poll is not where
    fx adopts".
- `shell.rs`: the comment of
  `a_walk_a_change_of_roots_stopped_counts_as_whole_once_it_is_over`. Its
  scenario matches fx only once A's walk has ended whole.
- `docs/parity.md` (the `@` file picker row): the first-list sentence points
  to AT-SCOPE-INTERIM, and that paragraph names both gaps and says they end
  with that unit.
- `round10/README.md`, corrected in place:
  - "Before" is now conditional ("if A's walk has ended whole by then");
  - "Why there" no longer says the poll is where fx's walk would be adopted;
  - the residual bullet names both.

  Each change is marked "Round 11".
- `round8/README.md` §1 and `round9/README.md` §4 are now conditional on
  fx's walk having ended whole.

## 2. The SHOULDs

- `round9/README.md`'s commit table now says "This line said `file_index.rs`
  until round 10 corrected it". Before, "which said" could be read as round
  10 saying it.
- `replacePendingRoots` spans `file_index.zig:511-523` at the pin: 523 is
  its closing brace, and 524 is blank (`fx-src.txt:2106-2118`). The test
  comment's `:511-524` is now `:511-523`, matching `round10/README.md`.

## 3. Checks (`tests.txt`, `refs.txt`)

- `cargo fmt --check`: rc 0. `cargo build --locked --tests`: rc 0.
- Filter `cargo test --locked --lib -- tui::file_index::tests
  tui::file_picker::tests tui::shell::tests`, on the tree at `f2a63b2d`
  (code identical to `1d859de5`):
  - **run 1: 894 passed, 1 failed;**
  - run 2: 895 passed, 0 failed.
- `refs.py af2b6137`: 6 wrong after the wording commit (the test comment grew
  3 lines). After `8360bc25`: 16 checked, 0 wrong.
- `check-no-stubs`: ok. `check-no-secrets`: ok (16015 tracked files).
- The brief's path grep printed 0 at `f2a63b2d`, `8360bc25` and the receipt
  commit.

## 4. Run 1's failure: a path-dependent test, from round 7

The case is
`a_change_of_roots_during_a_walk_with_a_refresh_queued_walks_the_new_roots_once`.
It fails at `shell.rs:35391`, `assert_eq!(listed_paths(&shell),
["src/deep/sched.rs"])`. The list also held the extra root's `plan.md`, by
its absolute temp path. Two things disagree:
- `score` (`file_index.rs:598-622`) tries the basename, then the whole
  folded path. So `@sch` matches `<tmp>/.tmpXXXXXX/plan.md` whenever the
  temp path holds an s, then a c, then an h.
- The test asserts the whole listing is the workspace's one row.

Measured (`tests.txt`):
- 3 of 3 runs fail with a TMPDIR whose path holds that subsequence.
- 2 of 6 fail with the default TMPDIR. Each failure has an h or H in the
  random suffix (`.tmpukHgnF`, `.tmp6HKi2i`, and run 1's `.tmpe6S18h`).

Whether it fails depends on the TMPDIR path. It is not this round's change:
the round's source diff is comments only. It is left as it stands here,
because this round changes no code. A candidate fix, for the dispatcher:
assert on the first row. `sched.rs` matches `sch` in its basename and
`plan.md` does not, and `score_order` (`file_index.rs:539-550`) ranks a
basename match first.
