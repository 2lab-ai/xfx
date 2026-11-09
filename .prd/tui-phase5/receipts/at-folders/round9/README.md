---
title: 0.0.16 delta -- AT-FOLDERS round 9 (fx's "loading" is no index whole yet this session)
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, input, file-picker]
---

# AT-FOLDERS round 9

Round 8's head `04ee6e58` was reviewed by two engines. Each approved with
fixes and found the same MUST independently; gpt6 adds one SHOULD and opus
two. The dispatcher confirmed the MUST at the pin: `startLoad`
(`file_index.zig:360-392`) never clears `active_generation`. The
dispatcher's gate on `04ee6e58` passed 21 of 21 steps.

fx's lines are quoted in `../../at-height/fx-src.txt`, extended from line
2513. Lines 1-2512 are byte-identical to round 8's (checked with `cmp`).
`fx-src.txt:N` below means that file.

| commit | what |
|---|---|
| `8e59306d` | RED (`red-unit.txt`): 1 case fails on round 8's code; the Ctrl-O case's sharper assertion |
| `2c6960b3` | the fix, the quotes, `docs/parity.md`, round 8's receipt corrected -- **the code head** |
| `c98aefd4` | the ledger's 6 `shell.rs` refs re-pointed, each `src/tui/shell.rs:40363` to `:40403` (`refs.txt`: 16 checked, 0 wrong). This line said `file_index.rs` until round 10 corrected it |
| `e7b81325` | `mutants.py` -- **the gate's commit** |
| (receipt commit) | this folder |

## 1. fx's "loading" is no index whole yet this session (gpt6 r8 MUST = opus r8 MUST)

**fx at the pin:**
- `isFileIndexLoading` is `currentState() == .loading` (`main.zig:2125-2127`,
  `fx-src.txt:2067-2069`).
- `currentState` is `.ready` whenever `active_generation` is set; otherwise
  it is the state of the generation that is loading, or failed or idle
  (`file_index.zig:333-341`, `fx-src.txt:2072-2080`).
- `active_generation` is set when a generation is adopted
  (`joinThreadIfDone` at `:425`, `fx-src.txt:2302`; `buildFromSource` at
  `:677`, `fx-src.txt:2549`). It is cleared only at shutdown (`deinit`,
  `:325`, `fx-src.txt:2531`) and in two tests (`:2208`, `:3205`). A change of
  scope keeps it.

So once a generation has been adopted, fx's index is never `.loading` again
this session.

**Before:** xfx's `loading()` was `index.is_none() && walk.is_some()`, and a
change of roots drops `index`. So after one install's walk had landed,
another install of other roots, and then the session's first `@` during
that walk, xfx skipped the refresh where fx asks for one, queued behind the
walk.

**Now:**
- `Files::ever_whole` (file_index.rs:1008) is set when a walk's `Ready` is
  taken in (:1187) and is never reset.
- `loading()` (:1096) is `!ever_whole && walk.is_some()`.
- A failed first index with nothing walking is `.failed` in fx, not loading.
  A walk started after it is loading again, because `startLoad` clears the
  failure (`:389`). xfx's predicate gives both without a term of its own.

**RED:** `the_first_episode_after_a_whole_index_refreshes_even_where_the_roots_dropped_it`,
opus's recipe. adopt(A), its walk done; adopt(B, another directory), its
walk held; `@plan`; that walk's `Ready`. Walks + 3, where round 8's code
gave + 2. Round 8's cold-install case still makes 1 (it passes on both
codes).

**Docs:**
- The comment on `loading()`, which said it equals `currentState() ==
  .loading`, now gives fx's whole state.
- The parity sentence now reads "while no index has been whole yet this
  session and a walk runs", and says an index a change of roots has
  dropped still counts.
- Round 8's receipt §1 and §2 are corrected in place.

## 2. The Ctrl-O case's assertion (gpt6 r8 SHOULD)

Round 8's "the driven listing was replaced" checked only that some listing
was in flight, which a real listing satisfies too. The case now takes the
driven listing's stop flag before the paste. After it, the case requires
that the listing in flight has that same flag (`Arc::ptr_eq`). The driven
job is not answered before the hold is asked.

With the paste turned back into round 8's typing, in a throwaway clone,
this assertion fails ("the driven listing was replaced"; `red-unit.txt`,
last lines). So it now tells the driven listing from a real one.

## 3. Mutants (`mutants.txt`)

One `--no-local` clone at `e7b81325`, under the suite lock: **91 run, 91
killed**, and the same 16 BROKEN as round 7 (`../round7/mutants.txt`).
- Round 9's 3, each killed by round 9's RED case:
  - round 8's predicate back (`index.is_none() && walk.is_some()`);
  - `loading()` as a walk alone (opus r8 SHOULD 1: nothing pinned the first
    term until now);
  - `ever_whole` never set.
- Rounds 8, 7, 6, 5, 4, 3, 2 and 1 re-run at this head: 5, 18, 2, 12, 10, 5,
  8 and 28 killed.

## 4. Pairs and sweep: not re-run

The predicate changes one count only: the session's first indexed episode,
after an index has been whole once and a change of roots has dropped it,
during a walk. It now queues a refresh behind that walk. (Round 10 adds a
second count: the first `@` after the poll that ends a walk a change of
roots stopped. fx refreshes there only if that walk has ended whole by then;
`../round10/README.md` names the two residuals; both ended when
AT-SCOPE-INTERIM lets the walk run on, as fx's does: both are gone (`../../at-scope-interim/README.md`).) The rows come from
the walk either way, and no scenario installs twice before its first `@`.
So round 7's `workspace-index` pairs and round 1's pairs and sweep stand.

## 5. The gate (`gate.txt`)

Every CI step (`../gates.sh`) ran in a `--no-local` clone at `e7b81325`, in
one hold of the suite lock, with `TMPDIR=<scratch>/atfg9`. **20 steps, 0
failed**, and the tree was clean before and after. The load was 6.55-32.30.
- fmt, and both clippies.
- `cargo test`: 5518 passed, 0 failed.
- `test-fault`: 5555 passed, 0 failed. Round 7's pty case passed.
- The checks and their self-tests; the fxcap self-tests (sweep 70 + 27,
  capture 68).
- Both release builds; smoke: 47 checks, 0 failures.
- smoke-tui: 37 scenarios and the oracle, 1168 checks, 0 failures.

## 6. Refs and paths

`refs.txt`: 16 checked, 0 wrong. `git diff af2b6137..HEAD` adds no local
path (the brief's grep prints 0 at every commit of this round). The new
case takes its directories from `tempfile`, and no test builds
`Runtime::new`.
