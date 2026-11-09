---
title: 0.0.16 delta -- AT-FOLDERS round 8 (fx's first-episode skip; claims with their conditions; an equal access does nothing)
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, input, file-picker]
---

# AT-FOLDERS round 8

Round 7's head `b4a07a4a` was reviewed by two engines:
- gpt6 approved with fixes: two MUSTs, both confirmed by the dispatcher.
- opus approved with fixes: two SHOULDs (one of them gpt6's second MUST), a
  hardening note and a receipt nit.

The dispatcher's gate on `b4a07a4a` passed 21 of 21 steps, `test-fault`
included.

fx's lines are quoted in `../../at-height/fx-src.txt`, extended from line
2462. Lines 1-2461 are byte-identical to round 7's (checked with `cmp`).
`fx-src.txt:N` below means that file.

| commit | what |
|---|---|
| `a19983bb` | RED (`red-unit.txt`): 2 cases fail on round 7's code |
| `51987c6a` | the fix, the quotes, `docs/parity.md`, round 7's receipt qualified, three test comments, `r6-check.py` -- **the code head** |
| `b1bc8eb9` | the ledger's 12 `shell.rs` refs re-pointed (`refs.txt`: 16 checked, 0 wrong) |
| `6710b846` | `mutants.py` |
| `d26c2d51` | test only: round 2's Ctrl-O hold case pastes its last folder (§6) |
| `ef9a5ffb` | the ledger's refs after it (`refs.txt`: 16 checked, 0 wrong) -- **the gate's commit** |
| (receipt commit) | this folder |

## 1. fx's first-episode skip (gpt6 r7 MUST 1)

**fx at the pin.** `prepareFilePicker` takes the list's refresh request for
an indexed list. The session's first such request sets
`file_picker_episode_seen` and refreshes only if the index is not loading
(`input_completion_runtime.zig:853-866`, `fx-src.txt:2464-2477`).
- "Loading" means no generation is whole yet, while one loads
  (`currentState`, `file_index.zig:333-341`; `isFileIndexLoading`,
  `main.zig:2125-2127`).
- The flag is a picker-state field that is set once and never reset
  (`picker_state.zig:171`).
- **Corrected in round 9** (`../round9/README.md` §1): "loading" means no
  generation adopted yet this session, with one in progress. fx keeps an
  adopted generation across a change of scope (set at `file_index.zig:425`
  and `:677`, cleared only at shutdown, `:325`), so `currentState` is
  `.ready` from then on. "No generation is whole yet" therefore means "none
  has been whole this session", not "none is held now".

**Why round 8 needs it.** Round 7's install walk means a `/workspace` change
before any `@` starts the index. So the session's first indexed `@` can now
meet a loading index. xfx queued one more walk there, where fx skips.

**xfx:**
- `Files::episode_seen` (file_index.rs:1001) is the session flag.
- `Files::refresh_index_for_list` (:1066) skips the first request while
  `loading()` (:1081): no index yet, and a walk running. Every later request
  is `refresh_index`. (Round 9: "no index yet" is now "none has been whole
  this session"; round 8's `index.is_none()` skipped where fx asks once a
  change of roots had dropped a whole index.)
- `prepare_index` (shell.rs:16713) calls it. The install's own refresh still
  calls `refresh_index`, ungated, as fx's `install` does.
- `loading()` has no term for a walk a change of roots has just stopped
  (`superseded`). A request at that moment is queued anyway, and coalesces
  with the change's own refresh, so such a term could change no count.
  Round 8's first cut had it; it was dropped before the commit.
  (Corrected in round 10, and made conditional in round 11: that holds only
  before the next poll. After that poll, with the new roots' walk running,
  fx refreshes if the stopped walk has ended whole by then, as fx adopts it
  only then. Round 8 never counted it whole, so the first `@` skipped even
  then. Round 10 counts it whole at that poll, which leaves two residuals
  the other way: before fx's walk would have ended, and after one that
  would have failed; `../round10/README.md`. Both ended when
  AT-SCOPE-INTERIM lets the walk run on, as fx's does: both are gone (`../../at-scope-interim/README.md`).)

**RED:** `the_first_indexed_episode_skips_its_refresh_while_the_index_loads`.
- An access change before any `@`, with the install's walk held in flight,
  then an indexed `@plan`: 1 walk in all. On round 7's code it was 2.
- Then another change of roots, a walk held, and a second episode: it
  refreshes behind the walk, 1 more walk.

**Test comments fixed:**
- `an_indexed_query_opens_with_one_walk_or_none_while_one_runs` now names
  the skip.
- `no_listing_starts_while_the_list_is_not_eligible` now says its bare `@`
  is the session's first episode and asks for no walk.

## 2. Each claim with its condition (gpt6 r7 MUST 2; opus r7 SHOULD 2)

`docs/parity.md:239` now says:
- "an `@` typed once that walk has published finds the added directory's
  files without `indexing files...`";
- a change of the access costs:

  | case | walks |
  |---|---|
  | no list open, a path list (`@src/`), or a walk already running | 1 |
  | an ineligible indexed list | 1, then 1 more once it is eligible |
  | an open, eligible, indexed list with no walk running | 2 |

- the first-episode skip, with what ships: xfx starts no index at launch,
  but a `/workspace` change before the first `@` does (round 9 states the
  skip's condition as "while no index has been whole yet this session");
- "xfx drops the old index and stops its walk" where the roots moved; where
  they did not, xfx keeps its index and rows (§3).

Round 7's receipt is qualified in place: its walk-count table, its §3, and
§4's mutant claim.

## 3. AT-SCOPE-INTERIM covers an access change that moves no root (opus r7 SHOULD 1)

At the pin, any access change moves fx's scope epoch (`invalidateScope`,
`app_workspace_runtime.zig:98-104`). An indexed list's source is
`fileCompletionRevision()`, which is the index's `readableRevision`
(`:74-80`, `fx-src.txt:2506-2512`). A source of another epoch is staged
`.unavailable` (`input_completion_runtime.zig:868-890`, `fx-src.txt:2481-2503`).

So fx says `Files unavailable` until a walk of the new scope is adopted,
whether the roots moved or not. xfx keeps its index, its walk and its rows
when they did not (`set_extra_roots` returns early).
- The parity sentence now covers both cases.
- `a_workspace_change_that_keeps_the_rows_still_resets_the_bar_and_the_trust`
  now says in a comment that fx shows the interim text there, and that its
  first precondition (the rows kept) flips when AT-SCOPE-INTERIM lands.
  (It did: `../../at-scope-interim/README.md`.)

## 4. An equal access does nothing (opus r7 note)

fx changes nothing when `eql` holds (`install`, `:134-141`). xfx called
`set_extra_roots(index_roots(…))` before it compared the accesses, and
`index_roots` reads the filesystem: `with_observed_roots` keeps a root only
while it is still a directory whose real path is itself
(`src/workspace/path.rs:277-299`). `adopt_workspace` (shell.rs:8444) now reads
the roots only inside `if changed`.

**A test can make the roots differ under an equal access**:
`an_equal_access_changes_nothing_even_where_its_directory_is_gone`.
1. The access adds a directory holding `plan.md`, and its walk lands.
2. `@plan` lists the file.
3. The directory is removed, and the same access is adopted again.
4. No walk follows, and a Backspace to `@pla` answers from the kept index
   (`Shown::Rows`, the same row).

On round 7's code the index was dropped with no walk asked for, so the list
said `indexing files...` with nothing walking (RED: `Shown::Loading`).

## 5. The receipt nit (opus r7)

Round 7's README §4 said `prepare-index-from-adopt` survived on round 6's
code, with no run behind it. `r6-check.py` writes that mutant against round
6's `adopt_workspace` and runs round 6's own test list in a clone at
`08ef8070`. It ran twice (`r6-check.txt`):
- **Run 1**, beside this round's gate: KILLED, by
  `under_the_full_transcript_a_folder_listing_holds_no_frame`, a round-2
  case that has nothing to do with `adopt_workspace`.
- That case then passed five times out of five run alone on that clone,
  with the mutant and without it.
- **Run 2**, alone under the lock: SURVIVED, every test passed.

So the claim stands: the mutant survived on round 6's code. Run 1's kill was
the flake in §6.

## 6. A flaky case, found by the mutant run, and fixed (`mutants-run1.txt`)

The first run of every set, at `6710b846`, beside the gate, had
`under_the_full_transcript_a_folder_listing_holds_no_frame` fail twice. Once
it was among `invalidate-keeps-trust`'s failing tests (with that mutant's
own killers); once it was the only failure behind round 6's
`prepare-index-from-adopt` (§5).

**Cause, traced.** The case drives a listing of `src/deep` and then typed
`deep/` after `@src/`. Each of `d`, `e`, `e`, `p` looks `src/` up again, and
the first of them replaces the driven `src/deep` listing with a real
listing of `src`. The `/` then starts a real listing of `src/deep`. When
that real listing landed before the case asked for the hold -- a matter of
scheduling on a loaded machine -- the list had rows, nothing was in flight,
and the hold's assertion failed.

A probe on the old case confirmed it: after typing `deep/`, a send on the
driven listing's sender failed, because its receiver was gone. After a
paste, the send went through.

**Fix** (`d26c2d51`, test only): the case pastes `deep/`, so `src/deep/` is
the one lookup. It now also asserts that a listing is in flight and that
the composer holds `@src/deep/`. Every set was then run again at the new
head (§7). Run 1's counts are not this receipt's.

## 7. Mutants (`mutants.txt`)

One `--no-local` clone at `ef9a5ffb`, under the suite lock: **88 run, 88
killed**, and the same 16 BROKEN as round 7 (`../round7/mutants.txt`).
- Round 8's 5, 5 killed:
  - no first-episode skip;
  - a skip on every episode;
  - a skip whether loading or not;
  - the list's refresh not going through the skip;
  - an equal access reading the roots again.
- Rounds 7, 6, 5, 4, 3, 2 and 1 re-run at this head: 18, 2, 12, 10, 5, 8 and
  28 killed.
- The fixed Ctrl-O case now fails only under the two mutants it was written
  for, `hold-ignores-eligibility` and `predicate-drops-transcript`; run 1
  also had it fail under `invalidate-keeps-trust`.

## 8. Pairs and sweep: not re-run

Round 8 changes no rows:
- The skip changes only whether a second walk is queued behind the first
  index's. The rows still come from that first walk.
- The equal-access change acts only when the same access is adopted again.

In `workspace-index`, the one scenario with a `/workspace` change, the
change is a new directory (a changed access). `@plan` is typed 1.5 s later
and snapped after `wait 2`. If the install's walk is still loading then,
round 8 skips the episode's refresh where round 7 queued one more. Either
way, the rows at the snap are the install walk's. Round 7's pairs of that
scenario (`../round7/pairs.txt`, 0 rows differ) stand.

## 9. The gate (`gate.txt`)

Every CI step (`../gates.sh`) ran in a `--no-local` clone at `ef9a5ffb`, in
one hold of the suite lock, with `TMPDIR=<scratch>/atfg8`. **20 steps, 0
failed**, and the tree was clean before and after. The load was 5.40-61.78.
- fmt, and both clippies.
- `cargo test`: 5517 passed, 0 failed.
- `test-fault`: 5554 passed, 0 failed. Round 7's pty case passed this time.
- The checks and their self-tests; the fxcap self-tests (sweep 70 + 27,
  capture 68).
- Both release builds; smoke: 47 checks, 0 failures.
- smoke-tui: 37 scenarios and the oracle, 1168 checks, 0 failures.

An earlier gate at `6710b846` was stopped after step 12, when the flaky case
in §6 was found. Its output is not kept.

## 10. Refs and paths

`refs.txt`: 16 checked, 0 wrong. `git diff af2b6137..HEAD` adds no local
path (the brief's grep prints 0 at every commit of this round). The new
cases take their directories from `tempfile`, and no test builds
`Runtime::new`.
