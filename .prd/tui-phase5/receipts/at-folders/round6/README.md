---
title: 0.0.16 delta -- AT-FOLDERS round 6 (fx's refresh queue; the scope-epoch term; a failed advance; a paired receipt)
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, input, file-picker]
---

# AT-FOLDERS round 6

Round 5's head `6d1958df` was reviewed by two engines:
- gpt6 approved with fixes: one MUST, and the dispatcher's decision on it.
- opus approved: its SHOULD 1 is the same queue, and its SHOULD 2 is fx's
  scope-epoch term.

The dispatcher's decision is to copy fx's refresh queue rather than name the
gap: stale rows are visible, and the goal is fx's UI.

fx's lines are quoted in `../../at-height/fx-src.txt`, extended from line
2269. Lines 1-2268 are byte-identical to round 5's (checked with `cmp`).
`fx-src.txt:N` below means that file.

| commit | what |
|---|---|
| `d9b9cdd8` | RED (`red-unit.txt`): 4 cases fail on round 5's code |
| `ca06d5aa` | the fix, the quotes, `docs/parity.md`, round 5's receipt correction, the failed-advance case, two earlier cases re-pointed |
| `7e4a4073` | the ledger's 12 `shell.rs` refs re-pointed |
| `b434d4a9` | scenario `p5-input-at-paste` |
| `f6853d8b` | a failure reset no lookup can see, dropped; unchanged roots pinned -- **the code head** |
| `303d310f` | the ledger's 6 `file_index.rs` refs re-pointed (`refs.txt`: 16 checked, 0 wrong); **the pair's xfx binary** |
| `08e7044f` | the pair (`pairs.txt`, `fx/`, `xfx/`) |
| `1467e1e9` | `mutants.py` -- **the gate's commit** |
| (receipt commit) | this folder |

## 1. fx's refresh queue (gpt6 r5 MUST; opus r5 SHOULD 1)

**fx at the pin (`file_index.zig@481104fd`):**
- The index has one `pending_scope` (`:284-288`, `fx-src.txt:2271-2275`).
- A refresh asked for while a walk's thread runs becomes that pending scope.
  An identical one is coalesced, and a different one supersedes it
  (`refreshOwnedRoots`, `replacePendingRoots`, `:488-523`,
  `fx-src.txt:2089-2118`).
- When the walk ends, ready **or failed**, the pending scope starts
  (`joinThreadIfDone`, `:409-461`; the start at `:443-451`;
  `fx-src.txt:2286-2338`).
- A load that starts clears the first index's failure (`startLoad`, `:389`,
  `fx-src.txt:2281`).

**xfx:**
- `Files::pending_refresh` (file_index.rs:995) is set by `refresh_index`
  while a walk runs (:1049-1055).
- `poll` takes it once the walk has ended (Ready, Failed or gone) and starts
  the walk (:1146-1149).
- One flag is enough: xfx's roots change only through `set_extra_roots`,
  which ends the walk, so every request in hand is for the same scope.
- After a failed first index the list says `indexing files...` again,
  because a lookup reads the walk in flight before the failure. fx says the
  same once `startLoad` clears the failure.
- Round 6's first fix also cleared `failed` in `start_walk`. No lookup can
  see that, so `f6853d8b` drops it.

**The first-episode skip.** fx's `file_picker_episode_seen` is a
picker-state field (`picker_state.zig:171`, `fx-src.txt:2353`). It is set
once (`input_completion_runtime.zig:858-859`) and never reset, so fx skips
only the session's first episode while its startup index loads. xfx starts
no index at launch, so nothing is added. "None beside a loading walk" is
kept as it was; its comment now says the refresh waits for the running walk.

**Re-pointed:** two earlier cases change, because their walk now ends with
a refresh in hand.
- Round 4's `no_listing_starts_while_the_list_is_not_eligible` counts
  listings: the bare `@`'s refresh, queued behind the driven walk, starts
  when that walk ends, under the transcript or not (fx's `joinThreadIfDone`
  does not ask the list).
- Round 5's Tab-retry case now fails the episode's own walk, so no refresh
  waits for it.

**Corrected:** round 5's claim that "no walk while one runs" is the skip's
effect (`../round5/README.md` §1), and `docs/parity.md`'s row.

## 2. fx's scope-epoch term (opus r5 SHOULD 2)

fx's reconcile also asks for a refresh when the scope's epoch moved
(`scope_epoch != scope_epoch`, `file_completion_state.zig:164`,
`fx-src.txt:2036`). A workspace change moves it (`invalidateScope`,
`app_workspace_runtime.zig:98-104`, `fx-src.txt:2356-2362`).
- `set_extra_roots` now says whether the roots changed (file_index.rs:1068).
- When they did, `adopt_workspace` (shell.rs:8434-8445) asks an open list
  for a refresh and looks it up, only while it is eligible
  (`refresh_file_picker`).
- The two docs that quoted `:164` with three terms now give all four:
  `FilePicker::refresh_requested`, and the open path in `reconcile_picker`.

fx's `install` (`:127-143`, `fx-src.txt:2365-2381`) does two more things
on a changed scope that this round does not copy. Both are outside what the
dispatcher scoped here:
- it refreshes the index at once, with or without a list;
- it invalidates the list, which drops trust and re-reads a browsed folder.

`a_workspace_change_under_an_open_indexed_list_walks_the_new_roots`:
`@plan` is open with no rows; a directory holding `plan.md` is added. One
walk follows, and the list shows the new root's `plan.md`. The same roots
again walk nothing.

## 3. A failed advance (gpt6 r5 SHOULD 3)

`list_entries` (file_index.rs:477) now takes the listing's entries as an
iterator. `list_directory_until` maps `read_dir` into it.
`an_advance_that_fails_after_entries_that_read_fails_the_listing` gives it
two entries that read, then an advance that fails. The result is `Err`, the
folder unavailable, as fx's `iterator.next(io) catch return
error.PathUnavailable` is (`path_completion.zig:90`, `fx-src.txt:1928`).
Without the failure the same two entries list as `Ok(2)`.

## 4. The paired receipt (gpt6 r5 SHOULD 2) (`pairs.txt`, `fx/`, `xfx/`)

The new scenario `p5-input-at-paste` drives the paths round 5 changed:
- `paste @src/` (fxcap's `paste` is a bracketed paste, capture.sh:885):
  the episode opens on the folder's query;
- `key BSpace`: the query turns back into an indexed one.

The fx oracle (v0.0.16, sha256 `8e6cda2efddb`, capture.sh's guard:
unchanged) and xfx built at `303d310f` (row 1 `v0.1.0-303d310`) were each
captured at 40x120. `cells.py` shows **0 rows differ** in each snap:
- `paste-src`: `src/`, then `src/main.zig`, `src/store/`;
- `paste-src-back`: `src/`, `src/store/`, `src/main.zig`,
  `src/store/index.ts`, `src/store/slots.ts`.

What this pair cannot show:
- A walk changes no cell of a folder query's rows, and both snaps come
  after the rows land. So this pair is a GREEN receipt, not a RED; the walk
  counts are the unit cases'.
- A folder that cannot be read: no fxcap verb changes a folder's
  permissions (`work` copies a tree with `cp -R`, capture.sh:570).

## 5. RED → GREEN (`red-unit.txt`)

On round 5's code, 4 cases FAILED:
- `a_new_episode_during_a_walk_queues_one_walk_and_its_rows_show`
  (walks 1, expected 2);
- `two_requests_during_one_walk_queue_one_walk` (1, expected 2);
- `a_failed_walk_with_a_refresh_queued_walks_again_and_shows_rows`
  (1, expected 2);
- `a_workspace_change_under_an_open_indexed_list_walks_the_new_roots`
  (1, expected 2).

Added with the fix, as pins (round 5's code had no seam for them, or got
them right): the failed-advance case, and the unchanged-roots lines.

## 6. Mutants (`mutants.txt`)

One `--no-local` clone at `1467e1e9`, under the suite lock: **71 run, 71
killed**, and the same 10 BROKEN as round 5 (explained in
`../round5/mutants.txt`).
- Round 6's 8: drop the pending refresh; not taken on Failed; never
  coalesce (a second request walks beside the first); pending never
  cleared; the workspace change asks for no refresh, or looks nothing up;
  unchanged roots refresh; a failed advance skipped. 8 killed.
- Rounds 5, 4, 3, 2 and 1 re-run at this head: 12, 10, 5, 8 and 28 killed.

## 7. Pairs and sweep: the earlier sets not re-run

The queue changes a snap only if a walk it adds shows different rows. It
adds a walk only when a refresh is asked for while one runs: a later
episode, or a kind change back to an indexed query, before the last walk
ended. In the ten `@` scenarios of the pairs and the sweep that can happen,
but the walk it queues cannot change a cell:
- no scenario changes its workspace's files during the run, so the queued
  walk reads what the first read;
- a walk that refreshes an index is private until it is whole, and the
  index before it is searched meanwhile (`docs/parity.md`). A walk that
  follows a first index still being published starts only after that first
  one is whole;
- the one visible difference is a failed walk, where round 5 said
  `Files unavailable`. No scenario's walk fails: its workspace is a copied,
  readable tree.

The workspace term acts only when `/workspace` changes the roots under an
open `@` list. In `workspace-index`, `/workspace add` is typed and answered
before any `@` is typed (`scripts/fxcap/scenarios/workspace-index.cap`). So
round 1's pairs (`../pairs.txt`) and sweep (`../sweep-compare.txt`) stand,
and §4's pair is new.

## 8. The gate (`gate.txt`)

Every CI step (`../gates.sh`) ran in a `--no-local` clone at `1467e1e9`, in
one hold of the suite lock, with `TMPDIR=<scratch>/atfg6`. **20 steps, 0
failed**, and the tree was clean before and after. The machine's load was
53.6-151.6 during the run (`gate.txt`, per step).
- fmt, and both clippies.
- `cargo test`: 5507 passed, 0 failed. With fault injection: 5544 passed, 0
  failed.
- The checks and their self-tests; the fxcap self-tests (sweep 70 + 27,
  capture 68).
- Both release builds; smoke: 47 checks, 0 failures.
- smoke-tui: 37 scenarios and the oracle, 1168 checks, 0 failures.

## 9. Refs and paths

`refs.txt`: 16 checked, 0 wrong. `git diff af2b6137..HEAD` adds no local
path (the brief's grep prints 0 at every commit of this round). No test
here builds `Runtime::new`: the shell cases use `WorkHandle::detached`.
