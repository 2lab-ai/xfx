---
title: 0.0.16 delta -- AT-FOLDERS round 7 (fx's install on a changed scope; a walk that starts takes the queued refresh)
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, input, file-picker]
---

# AT-FOLDERS round 7

Round 6's head `08ef8070` was reviewed by two engines:
- gpt6 approved with fixes: two MUSTs, both confirmed by the dispatcher.
- opus approved with fixes: its MUST is gpt6's second, and it has three
  SHOULDs.

The dispatcher's decision: close the change of scope, and copy fx's
`install` whole.

fx's lines are quoted in `../../at-height/fx-src.txt`, extended from line
2382. Lines 1-2381 are byte-identical to round 6's (checked with `cmp`).
`fx-src.txt:N` below means that file.

| commit | what |
|---|---|
| `60158da4` | RED (`red-unit.txt`): 5 cases fail on round 6's code; a test-only lookup counter |
| `e40b900c` | the fix, the quotes, `docs/parity.md`, round 6's receipt correction, round 6's open-list case re-pointed -- **the code head** |
| `57b00e24` | the ledger's 12 `shell.rs` refs re-pointed (`refs.txt`: 16 checked, 0 wrong); **the pairs' xfx binary** |
| `1daf761a` | the `workspace-index` pairs (`pairs.txt`, `runid.py`, `fx/`, `xfx/`, `xfx-base/`) |
| `e4422da0` | two pins (a listing in flight is dropped; a stopped walk is over at the next poll) |
| `1a39356e` | `mutants.py`, 17 |
| `1fbb8e74` | two cases the first mutant run asked for (the reset where the rows stay; the reset's four fields), and an 18th mutant -- **the gate's commit** |
| (receipt commit) | this folder |

## 1. A walk that starts takes the queued refresh (gpt6 r6 MUST 1)

**Before:** `set_extra_roots` stopped the walk but left `pending_refresh`.
The next walk started without taking it, so when that walk ended, `poll`
started one more.

**Now:**
- `start_walk` takes any queued refresh (file_index.rs:1110). A walk of the
  current roots is all a request asks for.
- `set_extra_roots` does not clear the flag. It marks the walk it stopped as
  `superseded` (:1002, :1093), and until the next `poll` (:1168) a refresh is
  queued as if that walk still ran (`refresh_index`, :1062).

Why `set_extra_roots` does not just clear the flag: fx does not stop the
running walk on a change of scope. `invalidateScope`'s `requestStop` stops
the folder listing only (`app_workspace_runtime.zig:70-72`,
`fx-src.txt:2400-2402`). The install's refresh and the list's own then
coalesce into the one pending scope behind that walk (`replacePendingRoots`).
With `superseded`, xfx's two requests coalesce the same way, into one walk
of the new roots. Clearing the flag instead would start the install's walk
at once and queue the list's refresh behind it: two walks where fx makes
one.

xfx still stops the old walk and drops the old index: it has no scope tag
to tell an old index from a new one (§3).

**RED:** a walk, a refresh queued behind it, a change of roots, then an
eligible list's refresh. On round 6's code that made 2 walks of the new
roots; now it makes 1. The case then checks that the next episode's
refresh starts at once.

## 2. fx's install (gpt6 r6 MUST 2 = opus r6 MUST)

**fx at the pin:**
- `install` (`app_workspace_runtime.zig:127-143`, `fx-src.txt:2365-2381`)
  sets `runtime_changed = !access.eql(replacement)`. When it is true, it
  calls `invalidateScope(app)` and then `refreshFileIndex(app)`, with no
  list or eligibility condition.
- `eql` (`workspace_access.zig:176-189`, `fx-src.txt:2384-2397`) compares
  each entry's path, `saved`, `command_line`, `available` and `active`.
- `invalidateScope` (`:98-104`, `fx-src.txt:2356-2362`):
  - moves the scope's epoch;
  - stops the folder listing (`requestStop`, `:70-72`);
  - invalidates the list (`file_completion_state.zig:130-139`,
    `fx-src.txt:2405-2414`): inactive, untrusted, lookup dirty, the folder's
    request dropped.
- The next reconcile is then a new episode, and resets the bar and the
  window to row 0 (`input_completion_runtime.zig:797`; `resetFilePickerIndex`,
  `picker_state.zig:249-252`, `fx-src.txt:2417-2420`).

**xfx** (`adopt_workspace`, shell.rs:8439):
- **The trigger** is `self.workspace_access != access` (:8440). xfx's
  `Entry` holds a path and its availability (`src/workspace/access.rs:154-164`):
  every entry is saved, and active exactly when available. So `!=` compares
  the fields of fx's `eql` that xfx has.
- **On a change:**
  - the folder listing in flight is stopped (:8445), and the listed folder
    forgotten, so it is read again;
  - an open list is invalidated (`FilePicker::invalidate`, file_picker.rs:575):
    bar and window to row 0, untrusted, the index's refresh asked for;
  - the index is refreshed (`Files::refresh_index`), a list open or not and
    eligible or not;
  - the list's own refresh and lookup go through `refresh_file_picker`, and
    wait for the list to be eligible as before.

**Walk counts, now fx's** (opus r6 §2):

| case | walks |
|---|---|
| (a) an open eligible list, no walk running | 2: the install's, then the list's refresh queued behind it |
| (b) a walk running, a refresh queued | 1 (§1) |
| (c) no list open | 1 |

Round 6's open-list case now asserts (a)'s 2. Its comment that equated
"same roots" with `runtime_changed` now reads "same access". Round 6's
receipt (§2, "One walk follows") is corrected in place.

**The `@` after the change.** fx refreshes once per episode, so it does walk
a second time for the next `@` (`prepareFilePicker`,
`input_completion_runtime.zig:854-866`). The skip happens only for the
session's first episode, and only while its index loads. So "rows without a
second walk" is not what fx does. What fx shows is rows at once, from the
install's index. The case asserts that, and asserts the episode's walk too.

**RED** (all fail on round 6's code):
- `a_workspace_change_with_no_list_open_walks_the_new_roots_at_once`: one
  walk at once. Then the next `@plan` shows `<extra>/plan.md` at once
  (`Shown::Rows`), and the episode's own refresh makes the second walk.
- `a_workspace_change_that_moves_no_root_still_refreshes_the_index`: a
  directory that is not there is added. That makes an entry, unavailable
  and not a root, and is still a changed access: one walk. The same access
  again walks nothing.
- `a_workspace_change_resets_an_open_list_and_reads_its_folder_again`: on a
  pasted `@src/` with the bar on `src/deep`, the change drops trust and
  re-lists the folder. Once its frame lands, the bar is on `src`.

`a_workspace_change_drops_a_listing_in_flight` was added after the fix. It
pins the stopped listing: a late answer is not taken.

The first mutant run, at `1a39356e`, was stopped once `invalidate-keeps-the-bar`
and `invalidate-keeps-trust` had survived (`mutants-run1.txt`). In every case
so far, the change cleared the rows, which put the bar back on row 0 and left
nothing to choose. Two cases were added for that:
- `a_workspace_change_that_keeps_the_rows_still_resets_the_bar_and_the_trust`:
  an access change that moves no root keeps the index and the rows, and the
  list is still untrusted and back on row 0.
- `a_list_invalidated_by_a_change_of_scope_starts_over`, a unit case for the
  reset's four fields (bar, window, trust, the refresh request).

A first gate, started at the same time, was stopped with them. Its output is
not kept.

## 3. The interim state: a gap, queued as AT-SCOPE-INTERIM

At the pin, fx keeps the old scope's index searchable, tagged with its
scope:
- `ReadableRevision` carries a `scope_epoch` (`file_index.zig:103-108`,
  `fx-src.txt:2423-2428`).
- `readableRevision` returns the searchable generation's epoch (`:572-583`,
  `fx-src.txt:2431-2442`).
- `searchableGeneration` is the active generation first, the old scope's
  included (`:647-654`, `fx-src.txt:2445-2452`).
- `prepareFilePicker` stages `.unavailable` when that epoch is not the
  workspace's (`input_completion_runtime.zig:887`, `fx-src.txt:2458`).

So until the new scope's index is whole, an indexed list in fx says
`Files unavailable. tab to retry; esc to dismiss.` xfx drops the old index
and stops its walk, so its list says `indexing files...` and fills in as
the new walk publishes. **The pin confirms the gap.** It is named in
`docs/parity.md` as unit AT-SCOPE-INTERIM and is not built here.

## 4. A changed scope behind the transcript (gpt6 r6 SHOULD = opus r6 SHOULD 3)

`a_workspace_change_behind_the_transcript_looks_nothing_up_until_it_closes`
counts the install's walk apart from the list's work:
- Under the transcript, the change starts the install's walk (walks + 1).
- Once that walk is done, still under the transcript, there is no second
  walk, and the list was not looked up (the test-only lookup counter is
  unchanged).
- When the transcript closes, the new root's `plan.md` shows at once, and
  the list's refresh walks (walks + 2).

Mutant `prepare-index-from-adopt` takes the list's request in
`adopt_workspace` directly. It survived on round 6's code; this case kills
it (§6).

## 5. parity.md:239 (opus r6 SHOULD 2)

"No listing and no index refresh starts" while the list is not eligible
was no longer true. The row now says:
- **No lookup is asked for the list.** The list asks for no listing and no
  index refresh, and Tab does not retry.
- **The index's own work goes on.** A refresh queued before then starts
  when the running walk ends, and a `/workspace` change walks its new
  roots, as fx's join and install do.

The same row now names the `/workspace` path (the trigger, the reset, the
walk counts) and the AT-SCOPE-INTERIM gap.

## 6. Mutants (`mutants.txt`, `mutants-run1.txt`)

One `--no-local` clone at `1fbb8e74`, under the suite lock: **83 run, 83
killed**, 16 BROKEN, all accounted for in `mutants.txt`'s header.
- Round 7's 18, 18 killed:
  - the queue: the start keeps the pending; `superseded` never set, or
    never cleared;
  - the install: no install refresh; no invalidation; the trigger on the
    roots only (killed where the access changes and no root does); the
    listing not stopped; the folder not read again; the reset keeping the
    bar, the window or the trust;
  - round 6's six whose text this round rewrote, re-texted: drop the
    pending, never coalesce, pending never cleared, no refresh asked, no
    lookup, unchanged access refreshing;
  - item 3: `prepare-index-from-adopt`, which survived on round 6's code.
- Rounds 6, 5, 4, 3, 2 and 1 re-run at this head: 2, 12, 10, 5, 8 and 28
  killed.
- The first run, at `1a39356e`, and why it was stopped: §2 and
  `mutants-run1.txt`.

## 7. Pairs (`pairs.txt`, `runid.py`, `fx/`, `xfx/`, `xfx-base/`)

`workspace-index` was captured at 40x120, since its `/workspace add` now
starts a walk at install, on three builds:
- fx v0.0.16 (sha256 `8e6cda2efddb`, capture.sh's guard: unchanged);
- xfx at the head (built at `57b00e24`, row 1 `v0.1.0-57b00e2`);
- xfx at round 6's code (built at `1467e1e9`, row 1 `v0.1.0-1467e1e`).

Every capture's own run directory (`<scratch>/fxcap.XXXXXX`) is on screen,
in absolute paths. They were compared as taken with `runid.py compare`,
which is cells.py's parse and comparison with each run directory's
six-character name written as `XXXXXX`, every style kept.

| snap | fx vs head | round 6 vs head |
|---|---|---|
| `at-plan` | 0 rows differ | 0 rows differ |
| `at-all` | 0 rows differ | 0 rows differ |

**No cell moves.** The added directory's `plan.md` shows in both rounds: by
the time `@plan` is typed, round 6's episode walk had also landed. The
committed copies have the scratch root and run directory written as
`<tmp>`. A highlighted letter splits a path with escape sequences, so
`runid.py redact` matches through them. cells.py on the committed fx and
head copies gives 0 rows differing too.

## 8. The gate (`gate.txt`)

Every CI step (`../gates.sh`) ran in a `--no-local` clone at `1fbb8e74`, in
one hold of the suite lock, with `TMPDIR=<scratch>/atfg7`. **20 steps, 1
failed**, and the tree was clean before and after. The machine's load was
37.32-125.62 over the run (`gate.txt`, per step).
- **The failure is step 05, `test-fault`:** 5551 passed, 1 failed. The case
  is `tests/tui.rs`'
  `a_streamed_answer_grows_the_document_only_inside_the_update_that_paints_the_band`,
  which failed with "xfx did not exit" in the pty harness's exit wait
  (`tests/support/pty.rs:388`).
  - It streams an answer and touches no `@` list, folder or workspace.
  - It passed in step 04 of the same gate, at load 54.65; step 05 started at
    load 37.32, and the load reached 125.62 later in the run.
  - Run again alone in the same clone, under the lock (`05b`, load 53.04),
    step 05 passed: 5552 passed, 0 failed.
  - The cause is not shown. Nothing this round changed is on the case's
    path, so a load flake is a guess, not a finding. The gate as a whole did
    not pass: 19 of 20 steps passed, and step 05 passed only on its rerun.
- fmt, and both clippies.
- `cargo test`: 5515 passed, 0 failed.
- The checks and their self-tests; the fxcap self-tests (sweep 70 + 27,
  capture 68).
- Both release builds; smoke: 47 checks, 0 failures.
- smoke-tui: 37 scenarios and the oracle, 1168 checks, 0 failures.

## 9. Refs and paths

`refs.txt`: 16 checked, 0 wrong. `git diff af2b6137..HEAD` adds no local
path (the brief's grep prints 0 at every commit of this round). Every
committed capture of this unit was also scanned as cells.py's text, escapes
removed, for a scratch root or run directory: none. The new cases take
their directories from `tempfile`, and no test builds `Runtime::new`.
