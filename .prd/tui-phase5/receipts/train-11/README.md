---
title: 0.0.16 delta -- P5 train 11 receipt (SUBMIT-CARD-PREVIEW, CATALOG-REFRESH, PASTE-GAP into one branch)
type: receipt
tags: [xfx, phase5, fx-0.0.16, train]
---

# P5 train 11: three closed units, one branch

Branch `train/p5-1109d`, from `origin/main` `32afd307` (#88, train 10), as the
brief set it; `git fetch origin main` found it unmoved. Each unit was closed
by its own two-engine review and the dispatcher's gate; this train combines
them. The program goal (the user's words): «최신 fx랑 비교해서 ui쪽으로 모든 기능
구현하고 기본적으로 최신 fx ui를 그대로 구현해줘» (fx v0.0.16, pin `481104fd`).

Oracle: fx v0.0.16 (`481104fd`), executable sha256
`8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked
by the sweep before and after every run, `FX_AUTO_UPGRADE=0`.

## Units and merges

| # | unit | branch @ tip | merge | base | conflicts |
|---|---|---|---|---|---|
| 1 | SUBMIT-CARD-PREVIEW | `feat/p5-submit-card-preview` @ `0c203cd3` | `4a0b900b` | `58c92ba2` (#84) | six files; two forecast (below) |
| 2 | CATALOG-REFRESH (S) | `feat/p5-catalog-refresh` @ `3ffbe0c7` | `0040fa59` | `e705a5dc` (train 10's CATALOG-BOUNDS) | `docs/parity.md`, the `/model` row (forecast); and two semantic conflicts, 4 build errors (not forecast) |
| 3 | PASTE-GAP (S) | `fix/p5-paste-gap` @ `5cc2e42c` (code `a4990f13`) | `d2ed0274` | `a832f4d7` (PASTE-AFTER-END) | `src/tui/event_loop.rs`, the tests module's end; `docs/parity.md`, the full-screen TUI row (forecast; `shell.rs`, forecast, merged clean) |

Every base is git's merge base. AT-FOLDERS, LOCAL-PATHS and
TEST-ENV-ISOLATION are not in this train. Every merge tree passed `cargo
check --all-targets` with and without `fault-injection`, `cargo fmt --check`
and clippy (both) before it was committed (`check-merges.txt`). Every
resolution is a script's (`blocks.py`, `rowmerge.py`, `semantic.py`), redone
by `reproduce.sh` in a clone to the merge commit's tree (`resolutions.txt`),
and checked from both ends -- ours -> merged is exactly theirs' edits,
theirs -> merged exactly ours'.

**At a glance.**
- **Merges:** three `--no-ff` merges, in order, each with a conflict. Merge 1
  had six conflicted files where two were forecast; merge 2's tree did not
  build until two semantic conflicts were fixed in it (not forecast); merge
  3's `shell.rs`, forecast, merged clean. Every resolution checked from both
  ends and redone by script in a clone to the merge commit's tree.
- **Token check** (train 7's script, byte for byte): **all three merges OK**.
  Negative control **21 of 21, 15 of 17, 18 of 18**; the two it misses plant
  nothing (the case picks the `/model` row, which both sides of merge 2
  changed); planted on the merged rows (`token_control.py`): **8 of 8**.
- **References:** after the last merge, **31 moved**, every one written on
  main -- 17 in the ledger, 2 in `capfile.py`, 2 in `docs/parity.md`, 10 in
  code, tests and harness comments; 5 of them bare continuations --
  re-pointed in one numbers-only commit; `repointed.py` **31 of 31 the same
  code**; `refs.py` **0 moved**; `refs_bare.py` **none moved outside the
  historical plans** (58 flagged, all in three plans).
- **Join checks:** two tests added (checks 1 and 2), each with what covered
  the meeting before; check 3 by the shared-function table and the loop's
  order, read from the code (no test: no unit changed `session`); five
  mutants (below). Check 2's bytes: **the same 438 bytes at the head, on
  SUBMIT-CARD-PREVIEW's tip and on merge 1**, in all three catalog cases.
- **Gate** at `820a8f15`: **20 of 20**; `cargo test` **5644** passed, with
  fault injection **5681** (each +105 on train 10's: the units' 103 and the
  two join tests); smoke-tui 1167 checks, `test-capture` 79 OK.
- **Network: no CONNECT to the Gateway** (2, both the MCP probe); two lib
  tests failed one case's time window in that run, with no connection logged
  during them; 6 of 6 pass alone under the same proxy (`net-rerun.txt`), and
  both pass in the gate's two suites.
- **Sweep** at `820a8f15`: 101 scenarios, **399 of 559 pass, 0 failed, 0
  timed out**. Against `main`'s sweep of the same selection: **379 -> 377 over
  537 shared pairs, 1 up, 3 down**: the three `model-column-idle` pairs down
  (the ledger's registered nondeterminism `catalog-window`, owner
  FLAKE-MODELSDOWN #19) and `model-parked` `menu-loading` up. Every one of the
  8 `model-column-idle` runs' verdicts follows from when each binary first
  asked the catalog (`first-asks.txt`, 8 of 8); reruns: head 2 of 3, `main` 3
  of 3. Not settled until the cause's own fix. Against each unit's own sweep:
  **0 down** but those same three on CATALOG-REFRESH's (0, 8 and 10 up).
- **Round 3, PR #89's arm64 CI** (`ci-young-clock.txt`): four
  CATALOG-REFRESH tests went five minutes back on the boot clock, which a
  runner up under five minutes has not counted. **A test defect**: no
  production path goes back on the catalog clock. Reproduced here on a young
  boot clock (the same four, the same way), fixed in tests at `c4354ec7`;
  after the look-ahead nothing goes back more than 10 s on the system clock.
  Gate at `c4354ec7` **20 of 20**, `cargo test` 5644 / 5681 as before.

After the merges, in order:

| commit | what |
|---|---|
| `33e8e5d8` | the merges' scripts; train 10's copied |
| `6ae87955` | the 31 references the merges moved, re-pointed; numbers only |
| `3c63fc71` | the two join tests; test only |
| `a51da9a2` | the join tests' files, the selection, the gate and sweep scripts |
| `820a8f15` | join check 1's test revised (an Enter inside the quarantine too); test only -- **the code head: the gate, the head sweep and every job ran here** |
| `7ce122d8` | the jobs' scripts, the shared-function table, the regroup check |
| `22c228b6`, `6ed18ed9` | the merge checks' files, at `820a8f15`, and two of train 10's scripts they ran |
| `ab6f4e50` | the gate's and the jobs' files, join check 2's bytes, at `820a8f15` |
| `b5795fae` | the sweeps' files, their compares and reruns; this README |
| `0695c1c8` | `gates-files.txt`, `redaction-grep.txt` |
| `8056c326`, `3be49c15` | round 2: this receipt's corrections; `redaction-grep.txt` again |
| `c4354ec7` | round 3: the tests that went five minutes (and two a minute) back on the boot clock run on clocks any runner has; test only -- **round 3's gate ran here** |
| `a35ba6dd` | round 3's receipt (`ci-young-clock.txt` and its harness, scans and bytes; `gates-r3.txt`); this README |
| the next | the look-ahead's superset: 11 sites the scanner had missed, the scanner extended, both scans again |
| the last one | `gates-files-r3.txt`, `redaction-grep.txt` again |

**No code changed after the gate**: `git diff --stat 820a8f15 0695c1c8 -- .
':!.prd/tui-phase5/receipts/train-11'` is empty (`gates-files.txt`, which
`gates-files.sh` writes in a clone at the receipt's final form). Round 3
changed test code at `c4354ec7` and ran the gate again there; nothing after
it but this folder (`gates-files-r3.txt`). The sweeps stay at `820a8f15`:
`c4354ec7` touches tests modules and one `#[cfg(test)]` fn only
(`ci-young-clock.txt`), none of it in a binary a sweep runs.

A gate and a head sweep begun at `a51da9a2` were stopped part-way when join
check 1's test was found to put its Enter in the paste's own delivery --
PASTE-AFTER-END's rule, on main -- and not in PASTE-GAP's quarantine; both ran
again, whole, at `820a8f15`. Nothing from the stopped runs is used here.

`git show --remerge-diff` of each merge is `remerge-1.txt` .. `remerge-3.txt`.

### Merge 1's conflicts: six files, two forecast

The brief forecast `docs/parity.md` and `capfile.py`. Git also stopped on the
ledger, `event_loop.rs`, `shell.rs` and `tests/tui.rs`: since the unit's base
`58c92ba2`, trains 9 and 10 re-pointed the same references the unit
re-pointed, and AT-HEIGHT (train 10) added a call beside the unit's in two
places. Each block resolved by `blocks.py` (diff3 style), one rule a block:

- **`docs/parity.md`, the full-screen TUI row**: token by token (train 10's
  `rowmerge.py`, base `58c92ba2`): the unit's 23 edits (the preview paragraph)
  and ours' 67 (PASTE-AFTER-END's paste paragraph and train 10's edits, on
  main), none overlapping or meeting; `rowcheck.py` true both ways.
- **The ledger, 13 blocks**: 12 that both sides re-pointed, each to its own
  lines -- ours' numbers kept, moved to the head's code by the re-point
  commit; one, the `preview-frame-before-the-turn-start` block, which the
  unit wrote in place of `submit-multi-card-whole` (its id line merged clean)
  and in which main had only re-pointed `src/tui/shell.rs:13112` to `:13187`,
  text the unit deleted: the unit's. (**Merge 1's commit message** says the
  unit "rewrote" that cause; the token check reads it exactly: the unit
  deleted `submit-multi-card-whole` and added `preview-frame-before-the-turn-start`.
  Not rewritten; corrected here.)
- **`capfile.py`, `tests/tui.rs` (two)**: numbers only, ours'. The unit had
  moved `capfile.py`'s `shell.rs:14603-14628` and left its bare continuation
  `:14621-14623`; main had moved both.
- **`event_loop.rs`** (after `approval_landed`) and **`shell.rs`** (in
  `resize`): both sides inserted at one base place -- AT-HEIGHT's
  `file_picker_landed()` / `file_picker_unseen()` and the unit's
  `band_landed()` / `landed_bottom = None; preview = None` -- each touching
  only its own state (`Shell::file_picker_landed` and `band_landed` read in
  full): ours, then theirs.

`filecheck.py`: `event_loop.rs`, `shell.rs` and `transcript.rs` true both ways;
the ledger, `capfile.py` and `tests/tui.rs` true both ways with reference
numbers masked.

### Merge 2's conflict and two semantic conflicts (not forecast)

**The `/model` row** (forecast): token by token (`rowmerge.py`, base
`e705a5dc`): the unit's 46 edits and ours' 12 (train 10's MODEL-FUZZY), none
overlapping; `rowcheck.py` true both ways.

**The tree did not build**: `cargo check --all-targets` stopped on four
errors in test code (`check-merges.txt`, reproduced at `0040fa59` with the
fixes taken out). Each side's new tests were written against a type the
other side had changed, so git's line merge saw nothing:
- train 10's join test on main
  (`a_catalog_read_that_fails_too_large_or_stalled_then_loads_ranks_in_three_tiers_and_keeps_its_mark_by_id`)
  gives `CatalogLoaded` and `CatalogRefreshFailed` an `Instant` due, which
  CATALOG-REFRESH made a `CatalogDue` on the catalog's clock (two E0308);
- CATALOG-REFRESH's two worker tests that build a `Queue` by hand lack the
  `owed` field COMPACT-NOTICE-LATE (train 10) added (two E0063).

Fixed in the merge commit by `semantic.py`, each spelt as the tree already
spells it: the dues as CATALOG-REFRESH rewrote the same expressions in its
own tests (`CatalogDue::at(catalog_now() + REFRESH_INTERVAL)`; a refresh
failed just now its `refresh_failed`'s `CatalogDue { since, at }`), `owed: 0`
as `turn_loop` and every other literal start it. The three tests pass on that
tree, and in the named run at the head (`jobs-named.txt`).

**From both ends**: with the four fixes taken out (`semantic.py --reverse`,
in a clone), `shell.rs` is true both ways; `worker.rs` true from ours' end,
and from theirs' end the hunks differ only in how `git diff` groups ours'
appended tests -- a run inserted after a base `        );` that the run itself
also holds -- which `regroup.py` shows: the same one base line removed, and
ours' lines 18165-19093 and the merge's 18756-19684 the same, line for line
(929). `interactive.rs`, `bridge.rs`, `event_loop.rs`: true both ways. The
nearest the sides come is `worker.rs`'s shared fixture `recording()`, 3 base
lines apart: main made it `pub(super)`, CATALOG-REFRESH made its runtime read
the test's own home (join check 5); both in the merge.

### Merge 3's conflicts (forecast)

- **`event_loop.rs`, the tests module's end**: ours' 642 appended lines
  (COMPACT-NOTICE-LATE's and AT-HEIGHT's, train 10's join test,
  SUBMIT-CARD-PREVIEW's) and the unit's 197, both after base line 15991
  (`a832f4d7`'s last test). Each whole, ours first (`blocks.py --append`, the
  rule trains 9 and 10 used). **The merge commit's message names line 18327 as
  the base line: that is the merged file's line; the base's is 15991.** The
  history is not rewritten; corrected here.
- **`docs/parity.md`, the full-screen TUI row**: token by token (`rowmerge.py`,
  base `a832f4d7`): the unit's 65 edits and ours' 23 (SUBMIT-CARD-PREVIEW's),
  none overlapping or meeting; `rowcheck.py` true both ways.
- **`shell.rs`**, forecast to conflict, merged clean: the nearest hunks 52
  base lines apart (`overlap.txt`). `filecheck.py`: `shell.rs`, `event_loop.rs`
  (one hunk slid by a blank line), `smoke-tui.sh`, `tests/tui.rs` true both
  ways.

## The token check (`token_check.py`, `token-check.txt`, `token-self-test.txt`, `token_control.py`)

Train 7's `token_check.py`, byte for byte (`cmp` against `../train-7/`),
over the three merges. **Result: all three OK** [measured]:
- **Merge 1**: the full-screen TUI row is changed by both sides; 833 tokens
  added, 54 deleted, all as expected, 32 clauses whole, contiguous and in
  their side's order; every other row one side's; 11 ledger entries "both
  sides changed its path:line numbers alone" (`grep -c 'numbers alone'
  token-check.txt`: 11; 12 is the count of numbers-only diff3 blocks, since
  `amendment-card-after-stop` held two), the cause the unit added and the one
  it deleted as written.
- **Merge 2**: the `/model` row both sides', 801 added, 47 deleted, 21
  clauses whole; the rest one side's.
- **Merge 3**: the full-screen TUI row both sides', 2204 added, 31 deleted,
  37 clauses whole; the rest one side's.

**The negative control**: `--self-test` finds **21 of 21, 15 of 17 and 18 of
18** [measured]. The two it misses in merge 2 plant nothing: its two token
cases pick the first row a side changed -- the `/model` row -- and plant on
that side's text of it, but both sides of merge 2 changed that row, so the
merged row is neither side's text (train 10 found the same for its merges 1
and 2). `token_control.py` plants the same two cases on the merged row
itself, for each side that changed it: **8 of 8 found**.

## The references (`repoint.py`, `repointed.py`, `refs-*-pre.txt`, `refs-*-head.txt`, `stale.txt`)

Train 7's `refs.py`, `refs_all.py`, `refs_short.py` and train 9's
`refs_bare.py`, unchanged, five sides: `main` `32afd307` and the three tips.
At the merges' head before any re-point (`d2ed0274`, `refs-*-pre.txt`) they
flag, outside `.prd/` and the historical plans:

- the ledger: 17 -- `shell.rs`'s `send`, `shown_event`, `resize` twice, the
  `tools: {} advertised` row six times, the amendment row twice and its bare
  `:1658`, the catalog wait's `:4343`, the `/model` cause's `:1765`, and
  `shell.rs:187` twice and `src/provider/mod.rs:116`, which CATALOG-REFRESH's
  import and new module moved by two lines and one;
- `capfile.py`: `shell.rs:14687-14712` and its bare `:14705-14707`;
- `docs/parity.md`: `worker.rs:3917` and its bare `:3556`;
- `tests/tui.rs` x2, `smoke-tui.sh` x2, `event_loop.rs` x2, `shell.rs` x4
  (two of them bare continuations).

**31 references, re-pointed in `6ae87955`, numbers only** (29 lines out, 29
in, each the same once its numbers are masked). Each is read in the frame of
the side that wrote its citing line -- `git blame` of that line at the
merges' head names a commit, and the frame is the tip of the side that holds
it (`repoint-plan.txt`): **main for all 31** -- and its number mapped to the
head through `git diff -U0`.

**After the last merge** (and the join tests, appended at their file's end,
so no cited line moves), at `820a8f15`:
- **`repointed.py`: 31 of 31 the same code** [measured] -- each comparing
  main's cited lines and two either side with the head's at the number **the
  head's text holds**.
- **`refs.py`: 81 references, 0 moved.**
- **`refs_bare.py`**: 73 bare continuations, 58 flagged, **every one in three
  historical plans** under `docs/superpowers/plans/` (17, 15 and 26).
- `refs_all.py`: 15 flagged, all in the plans; `refs_short.py`: outside the
  plans only train 8's stale `frame.rs:1615-1628` (`CHANGED` for the two
  units whose frames are older than GRID-ROW-COST).

**Not re-pointed, or re-pointed by line only** (`stale.txt`): the historical
plans (trains 8 to 10 left them for REF-LINT); the ledger's bare `:1658`, now
`:1747`, for `queue_card`, which is at main's 1840 and the head's 1929 -- stale
since train 7's `c468f5e1` (train 10's `stale.txt`); `event_loop.rs`'s
`event_loop.rs:1005-1008`, now `:1046-1049`, and `frame.rs:1615-1628` (train
8's). Round 2 adds eight more the re-point moved by line though their text
named other code on main already (`stale.txt`'s second list). `repointed.py`'s
31 of 31 checks that each number names the same lines as main's did, not that
main's were right.

## Join checks

Two tests were added (`3c63fc71`, revised in `820a8f15`, test only;
`join_test_*.rs`, `apply_join.py`), each where no unit's test covers the
meeting; each says in its comment which tests cover each hop. Run by name at
`820a8f15` with the units' tests on each meeting (`jobs-named.txt`: **22 lib
tests and 4 pty tests, all pass**), in every gate step that runs the suites,
on merge 1 (`jobs-alone.txt`), and against five mutants (below). Which side
defines each helper the tests name: `helpers.txt`.

### 1. SUBMIT-CARD-PREVIEW x PASTE-GAP: what one Enter does

**The path** [read from the code]: a read's bytes reach the decoder only
through `Shell::route_read` (`burst`'s reads); a delivery ends only at a
readiness check that finds nothing (`burst` -> `Shell::end_delivery` ->
`Decoder::end_delivery`). Behind a paste's end marker in its own delivery,
`Decoder::feed`'s first branch counts the byte and returns
(PASTE-AFTER-END's, on main); when that delivery ends, `end_delivery` starts
PASTE-GAP's quarantine (`quarantine: Some(..)`, `quiet_since: Some(now)`), and
every byte fed inside it is dropped until two empty checks `PASTE_QUIET`
apart. An Enter decoded as a key reaches `Shell::send`, which -- before the
composer clears -- asks `preview_room()` and, when the card is taller than
the room, `preview_card(..)` (SUBMIT-CARD-PREVIEW's); the tick's
`commit_counted` writes the preview's frame and then the card's. So an Enter
the decoder drops never reaches `send`, and no preview can start.

**What covered it already**: PASTE-GAP's
`a_readiness_gap_inside_one_paste_does_not_turn_the_rest_of_it_into_keys` and
PASTE-AFTER-END's carry-over test (nothing sent: the composer and the work
channel, no frame); SUBMIT-CARD-PREVIEW's
`the_tick_that_shows_a_preview_writes_the_card_in_the_next_frame` (a pasted
draft whose Enter is typed apart, `route_bytes`). None sends a paste taller
than the room through the loop's reads. **Added**:
`a_tall_paste_with_enters_behind_its_marker_and_in_its_quarantine_previews_once_after_it`
(`event_loop.rs`): on a full 40x120 screen, a twelve-line paste and an Enter
behind its marker in one delivery (`Readable`, `burst`), then an Enter in the
next turn, behind the check that settled the paste: neither starts a preview
(`previewing()` false before and after each tick), nothing is sent, fx's
warning is written; after an idle turn and a check `PASTE_QUIET` later, an
Enter of its own sends the twelve lines once, and its tick writes **exactly
one** preview update (rows 12, not row 1), then the card **once, in order**
(`card_once`), and the next tick writes none of its rows.

**On merge 1** (main and SUBMIT-CARD-PREVIEW, no PASTE-GAP; `jobs-alone.txt`,
its one `end_delivery_at(.. PASTE_QUIET)` read as `end_delivery()`, since
there is no quiet there): **fails** at the second Enter -- "the Enter inside
the quarantine started a preview" -- after the first, behind the marker,
passed. So the test is the meeting: without PASTE-GAP the Enter after the
gap previews and sends the paste. On PASTE-GAP alone it does not build:
`Terminal`, `loop_tick`, `card_once`, `previewing` are SUBMIT-CARD-PREVIEW's
(`helpers.txt`).

### 2. SUBMIT-CARD-PREVIEW x CATALOG-REFRESH: a submitted prompt is catalog activity

**The path** [read from the code]: `Shell::send` holds both units' edits and
nothing else changed from main's (`shared.txt`: main's `send` plus
CATALOG-REFRESH's `note_catalog_activity(self.catalog_clock.now())` and
SUBMIT-CARD-PREVIEW's `preview_room()` / `preview_card(..)`, each a single
disjoint edit). `note_catalog_activity` asks once per answer
(`catalog_nudged`) when the catalog is due by CATALOG-REFRESH's rule
(`CatalogDue::is_due`); the tick's frame asks again only for an open `/model`
column (`commit_counted` -> `settle_column`, which returns unless the slot is
`Column`).

**What covered it already**: the ask by
`a_prompt_past_a_failed_catalogs_backoff_asks_for_it_and_one_inside_asks_nothing`
(`shell.rs`; a one-line prompt, no frame), the column's frame by
CATALOG-REFRESH's `the_model_columns_frame_reads_the_catalogs_clock`, the
preview by `the_tick_that_shows_a_preview_writes_the_card_in_the_next_frame`
(no catalog told). **Added**:
`a_submit_that_previews_asks_a_due_catalog_once_and_writes_the_bytes_it_writes_without_one`
(`event_loop.rs`): one Enter on a three-line draft over a full screen, three
cases -- no catalog told; a failure whose hold has 60 s to run; one past its
hold (round 3: a hold that ends as it is told, `catalog_due(Duration::ZERO)`;
it was one that ended 60 s ago, a reading a runner booted under a minute
before has not got -- `ci-young-clock.txt`): the Enter asks `ACTIVITY` once past the hold and nothing
otherwise; neither the preview's tick nor the next asks; the card once, the
draft sent; and the preview's tick writes the same bytes in all three.

**The bytes, against SUBMIT-CARD-PREVIEW's own** (`join_bytes.sh`,
`join-bytes.txt`): the same test with a probe writing each case's tick bytes,
run at the head twice, on SUBMIT-CARD-PREVIEW's tip `0c203cd3` and on merge 1
(the last two without CATALOG-REFRESH, so with its two helpers as their
types have them, `join_helpers_catalog.rs`): **all 12 dumps the same 438
bytes** (sha256 `12824a1c5a716dc6...`) [measured] -- the preview's update and
the card's, byte-identical to the unit's own. Round 3, with the head at
`c4354ec7` and the revised case: the same 12 dumps (`join-bytes-r3.txt`).

**On merge 1** (no CATALOG-REFRESH; `jobs-alone.txt`): **passes**. So this test
pins the meeting's outcome -- one ask per submit, the preview and card
untouched by the catalog -- under the ask rule main already had; what
CATALOG-REFRESH changed about that rule (a sleep-counting clock, an earlier
reading due at once) is its own tests' (`named` runs five of them). Mutants C1
and C2 below show the test follows the ask rule at the head. On
CATALOG-REFRESH alone it does not build (SUBMIT-CARD-PREVIEW's helpers,
`helpers.txt`).

### 3. CATALOG-REFRESH x PASTE-GAP: the turn's order (`shared.txt`, `loop-order.txt`)

**The loop** [read from the code, `loop_order.py`, at the head and on every
side]: `session` is the same text on every side (`shared.txt`), and its turn is
`collect_facts` (:278) -> `read_burst` (:279) -> `settle_input` (:286) ->
`take_ui_events` (:289) -> `settle_band` (:295) -> `collect_facts` (:300) ->
`commit_counted` (:301) -- PASTE-GAP's order, kept.

**No catalog event between a decoder change and the reconcile**:
- the decoder is changed only by `route_read` (`feed`), `end_delivery_at`
  (`end_delivery`), `still_at` (`still`) and `settle_input` (`flush`), and read
  by `plane_names_the_hold` (`named_hold`), called only from `settle_input` --
  the same five at the head and on PASTE-GAP's tip; outside tests they are
  called only from `session` (the launch's leftovers, before the loop) and
  `burst`, and from `route_bytes`, which is `#[cfg(test)]`;
- the catalog's events reach the shell only through `take_ui_events` ->
  `Shell::apply`, after `settle_input`; CATALOG-REFRESH's loop work is
  `commit_counted`'s `catalog_now()` and `settle_column(..)` (:1172-1173), at
  the frame, after both;
- a frame is built only by `commit_counted` in the loop (and by `shut_down`'s
  exit path, after it).

**The shared functions, by hash, on every side** (`shared.py`; each unit's tip
against its own base, reference numbers masked): 26 functions on the loop,
the reads, the reconcile, the events, the frame, the catalog's asks and the
submit. 14 the same on every side that holds them (`session`,
`collect_facts`, `take_ui_events`, `drain_ui_events`, `apply`, `consume`,
`route_read`, `end_delivery_at`, `still_at`, `said_ignored`,
`plane_names_the_hold`, `catalog_now`, `band_landed`, `preview_landed`). Of the
rest, each changed by **one** unit and the head's that unit's -- `burst` and
`settle_input` PASTE-GAP's; `commit_counted`, `settle_column`,
`note_catalog_activity`, `want_catalog` CATALOG-REFRESH's; `commit_tick`,
`commit_frame` SUBMIT-CARD-PREVIEW's -- or changed by none (`read_burst`,
`settle_band`, `submit`: main's, newer than the units' older bases;
`settle_band`'s the same once the re-point's two numbers are masked) -- but
`send`, which both SUBMIT-CARD-PREVIEW and CATALOG-REFRESH changed, each by
one disjoint edit, both in the head's (check 2).

No test added: no unit changed `session` or the order, and PASTE-GAP's own
tests pin the reconcile at the end of every turn
(`the_plane_s_sentence_goes_in_a_turn_whose_burst_ended_with_neither_check`,
`..._when_a_paste_begins_in_a_turn_that_ends_at_the_read_cap`,
`the_quarantine_s_sentence_leaves_the_review_plane_when_the_quarantine_ends`;
all pass by name, `jobs-named.txt`).

### Mutants (`join_mutants.py`, `jobs-mutants.txt`)

On a `git clone --no-local` at `820a8f15`, each cargo run bounded at 900 s,
the files restored after each, the clone clean at the end; against the two
join tests and one test of each unit on the path. The unmutated clone passes
all six. **5 of 5 killed, none hung** [measured]:

| mutant | join-paste | join-catalog | the units' |
|---|---|---|---|
| P1 the bytes behind a paste's end marker decoded (PASTE-AFTER-END's `feed`) | **FAILED** | ok | PASTE-AFTER-END's FAILED |
| P2 no quarantine after a settled paste (PASTE-GAP's `end_delivery`) | **FAILED** | ok | PASTE-GAP's FAILED |
| S1 no preview, the card whole at once (SUBMIT-CARD-PREVIEW's `send`) | **FAILED** | **FAILED** | SUBMIT-CARD-PREVIEW's FAILED |
| C1 a submitted prompt is no catalog activity (CATALOG-REFRESH's `send`) | ok | **FAILED** ("past the failure's hold: the Enter's asks", `[]`) | CATALOG-REFRESH's FAILED |
| C2 a failure's hold holds nothing (`note_catalog_activity`) | ok | **FAILED** ("inside the failure's hold", one ask) | CATALOG-REFRESH's FAILED |

Each mutant is also killed by its own unit's test, so neither join test is
the only guard of a hop; what each adds is the meeting (P2 and S1 together:
the second Enter; S1, C1, C2 together: the ask and the preview in one
Enter).

### 5. Network hermeticity (`net.py`, `jobs-net.txt`, `connect.log`)

Train 9's and 10's method (FLAKE-CATALOG-EVENT's `connect_logger.py`, a
loopback logger that answers 403 and forwards nothing; the four proxy
variables at it, `NO_PROXY`, `no_proxy`, `ALL_PROXY`, `all_proxy` unset), a
clone at `820a8f15`, one hold of the lock:

| run | result | CONNECTs logged during it |
|---|---|---|
| `cargo test --locked --lib --no-fail-fast` | 4718 passed, **2 failed**, 7 ignored | **2**, both `CONNECT w.example:443` |
| `cargo test --locked --test tui --no-fail-fast` | 188 passed, 0 failed | **0** |

**No CONNECT to the Gateway host** [measured]; the two `w.example` lines are
the known MCP probe (trains 6 to 10). **The two lib failures, what was
measured**: both are STOP-IN-CATALOG-WAIT's tests (train 7's joins); each
failed one case's window -- a stopped turn's end due at or past 300 ms and
before 1 s, a stop being sent 300 ms into the wait -- its `assert_eq` holding
`false` where `true` was due (`alone-failures.txt`, the left and right lines).
The log holds the window's verdict, not the end's time, so whether the end
came late or early is *(not measured here)*. No connection was logged during
the run but the two MCP probes. Run alone three times each, under the same
logger and proxy variables, in the same clone, under the lock
(`net_rerun.py`, `net-rerun.txt`): **6 of 6 pass, 0 connections logged**; both
pass in the gate's two suites too. The failures came only in the whole lib
suite's run (load 14.1 at its start), and are not counted here, as the
common brief's rule for a timing failure under load reads it. On the path
[read from the code, opus's reading checked]: the tests' window is wall
clock, and the 2 s wait is `tokio::time::timeout(FAILED_CATALOG_RETRY_WAIT,
..)` (`worker.rs:4555`), a line CATALOG-REFRESH did not change; what it waits
on, the selector's `settle_load`, is CATALOG-REFRESH's.

**CATALOG-REFRESH's tests and the environment** [read from the code]: its
tests read a scripted in-process catalog (`Turns`, a `ModelCatalog` whose
`fetch` returns the test's rows): no socket at all, loopback or other. No
test it wrote builds a `Runtime::new` -- every `Runtime::new` in the test
modules at the head was written on main, by blame (`runtime-new.txt`); its two
runtime tests build theirs with `Runtime::with_environment` over a temp home,
and its fixture test `the_shared_fixtures_runtime_reads_only_its_own_home`
(that `recording()` holds no process variable) passes by name
(`jobs-named.txt`).

## Gate (`gates.sh`, `gates.txt`, `gate-counts.txt`)

`gates.sh` (trains 5's to 10's, its comment only changed): every step of
`.github/workflows/ci.yml` and `sweep.yml`'s two harness tests, in a `git
clone --no-local` at `820a8f15`, one after another, **the whole script inside
one hold of the suite lock**, each step's exit taken from `$?` on the line
right after its command, the tree clean before and after. **20 steps, 0
failed, none run again** [measured]:

| step | at `820a8f15` |
|---|---|
| `cargo fmt --check`; clippy; clippy `--features fault-injection` | ok; ok; ok |
| `cargo test --locked --no-fail-fast` | **5644 passed, 0 failed, 7 ignored** (lib 4720, `tui` 188) |
| the same `--features fault-injection` | **5681 passed, 0 failed, 7 ignored** (lib 4743, `tui` 202) |
| `check-xfx-identity` / `check-no-stubs` / `check-no-secrets` | ok / ok / ok (18202 files) |
| `check-preview-contract` / `check-tui-preflight-cost` / `check-usage-clock` | ok / ok (200x300 replay 1.19 ms; budget 32) / ok |
| `test-check-no-secrets` / `test-gate-env` / `test-preview-contract` | ok / ok / ok |
| `fxcap/test-sweep.sh` / `fxcap/test-capture.sh` | 70 + 27 OK / 79 OK (`gate-counts.txt`) |
| release build; `fault-injection` release build | ok; ok |
| `smoke.sh` / `smoke-tui.sh` | 47 checks, 0 failures / 37 scenarios + the oracle, 1167 checks, 0 failures |

Load at the steps' starts 7.2-27.2. **Every side's tests are in**
(`test-counts.txt`, train 7's count: lines holding `#[test]` under `src` and
`tests`): `main` 5452; SUBMIT-CARD-PREVIEW +28 (5388 against its base's
5360), CATALOG-REFRESH +20 (5403 / 5383), PASTE-GAP +55 (5419 / 5364):
**5555 at the merges' head, 5557 with the two join tests**. `cargo test`
5644 against train 10's 5539 at `main`'s code: **+105** = 28 + 20 + 55 + 2;
with fault injection 5681 against 5576, the same +105. The harness's `def
test_` 176 (no unit added one).

**Round 3, at `c4354ec7`** (`gates-r3.txt`, the same script, one hold of the
lock, load 15.1-61.0): **20 steps, 0 failed, none run again**; `cargo test`
5644 passed (lib 4720), with fault injection 5681 (lib 4743), 0 failed, 7
ignored each -- the same counts (the fix changes tests, adds none); smoke-tui
1167 checks, 0 failures.

## Round 3: PR #89's arm64 gate, a young boot clock (`ci-young-clock.txt`)

PR #89's `gate (aarch64-unknown-linux-gnu)` at `3be49c15`: lib 4710 passed,
4 failed, all CATALOG-REFRESH's -- three `checked_sub(5 min + 1 s).expect(..)`
and one five-minute `backdate_catalog` whose `backdate_for_test` left the
time where it was. The catalog clock counts from boot; that runner had
counted at least 135 s and under five minutes. Every step, with its file:

- **Test-only** (`young-clock.sh sub`, `young_scan.py`): the lib builds both
  ways with `CatalogTime`'s `Sub` made `#[cfg(test)]`; no site outside a tests
  module goes back on a `CatalogTime` or an `Instant`.
- **RED here** (`young_clock.py`, in a `git clone --no-local` only): the system
  catalog clock read as a machine booted 150 s before; the same four fail the
  same way at the same lines, nothing else. At 15 s, the minute-back ones too.
- **Fix** (`c4354ec7`, test only): the four on a `FakeMachine` clock;
  `backdate_for_test` panics past the clock's origin; two `event_loop` tests'
  hold "a minute past" becomes one that ends as it is told.
- **Look-ahead** (`young-scan-*.txt`): 42 sites at `3be49c15`, 7 more than
  10 s back on the system clock; 40 at `c4354ec7`, none, each one left listed
  with why it is safe. A superset grep (`young-superset-c4354ec7.txt`) found
  11 sites the scanner's first form missed (none more than a second back);
  the scanner as committed takes them. **Green**: the young-clock suite at `c4354ec7` at 150 s
  and 15 s, 4719 passed, 0 failed.

## Found on the way, not changed here

- **Merge 1 was forecast to conflict in two files and conflicted in six**:
  trains 9 and 10 re-pointed, on main, the same references the unit
  re-pointed on its base, and AT-HEIGHT added a call beside each of the
  unit's two.
- **Merge 2 merged as text and did not build** (four errors, test code
  only): train 10's join test against CATALOG-REFRESH's `CatalogDue`, and
  CATALOG-REFRESH's tests against COMPACT-NOTICE-LATE's `Queue::owed`. A
  unit built on a base older than main's can carry this; `cargo check
  --all-targets` before each merge commit is what caught it.
- **Two commit messages are corrected here, not rewritten**: merge 1's
  "rewrote" (a cause deleted and one added) and merge 3's base line (18327 is
  the merged file's; the base's is 15991).
- **`filecheck.py` reads a run inserted next to a base line it also holds as
  False from one end**: git's diff may group it across that line (merge 2's
  `worker.rs`). `regroup.py` checks that case; carrying it into
  `filecheck.py` is REF-LINT's or the next train's.
- **`token_check.py`'s negative control plants nothing** for its two token
  cases when both sides changed the row it picks (merge 2 here, as merges 1
  and 2 of train 10); `token_control.py` plants them on the merged row.
- **The catalog join test passes on merge 1**, without CATALOG-REFRESH: it
  pins the meeting under main's ask rule; CATALOG-REFRESH's own rule (the
  sleep-counting clock, an earlier reading due at once) is its own tests'.
- **Two STOP-IN-CATALOG-WAIT tests missed one case's window once** in the
  network run's whole lib suite (load 14.1 at its start); 6 of 6 alone and
  green in both gate suites. Their 2 s wait is tokio's timer, unchanged by
  CATALOG-REFRESH (`worker.rs:4555`). Named here for the security review
  rather than counted.
- **The sweep's one drop is a registered nondeterminism** (`catalog-window`);
  its 8 verdicts follow from the first asks' times (`first-asks.txt`). This
  receipt's first wording ("a wall clock, not the code", "the head's xfx ...
  starting late") said more than the files show and was wrong on the second
  count (the head's xfx's median first ask is earlier); rewritten in round 2.
- **Re-pointed by line though stale on main already** (`stale.txt`, each
  shown against main and the head with where the named code is): the
  ledger's bare `:1747` (was `:1658`) for `queue_card`;
  `event_loop.rs:1046-1049`; and, round 2, eight more -- `tests/tui.rs:9331`
  (`Shell::blind` cited at `shell.rs:2814`, `fn blind` at 16804) and `:9371`,
  `smoke-tui.sh:11212` and `:11213`, `event_loop.rs:14833`, `shell.rs:10805`
  (bare `:3535`), `shell.rs:23737` and `:23738`. Left for REF-LINT, with the
  historical plans.
- **The units' own sweeps predate their last rounds**: SUBMIT-CARD-PREVIEW's
  ran at `b0c8d70e` (round 3), CATALOG-REFRESH's at `3d96f590` (round 2),
  PASTE-GAP's at `ea2b8a60` (round 1, before rounds 2 to 8). The compares
  below are against those sweeps, as their receipts commit them.

## Sweeps

**Builds** (debug, as trains 4 to 10), each in its own `git clone --no-local`,
each swept with its own tree's harness, scenarios and ledger (`sweep.sh`,
train 10's, reading this train's `selection.txt`). fx: the v0.0.16 binary,
sha256 `8e6cda2e...` before and after each sweep, `FX_AUTO_UPGRADE=0`; 40x120,
`--jobs 2`; each sweep inside one hold of the suite lock (`sweep-runs.txt`).
Every reference sweep -- `main`'s and each unit's own -- was rescored with the
head's ledger (`sweep.sh --rescore` on a copy, no capture taken); it moved no
pass count (main 379, SUBMIT-CARD-PREVIEW 73, CATALOG-REFRESH 77, PASTE-GAP
200, as each scoreboard had them). Every xfx snap that shows the banner shows
the build swept: `v0.1.0-820a8f1` on all 444 of the head's, `v0.1.0-32afd30`
on all 438 of `main`'s (`banners.py`, `sweep-runs.txt`).

**Selection** (`selection.py`, `selection.txt`; 101 scenarios at the head, 96
on `main`, which lacks SUBMIT-CARD-PREVIEW's five new ones -- each
scoreboard's `Scenarios:` line):
- the brief's: `p5-submit-preview-*` and `*submit*`; `model*`,
  `p5-input-model*`, `p5-catalog-*`; `approvals*`, `paste*`, `p5-paste*`
  (matching none), `p5-input-paste*`;
- each unit's own sweep's `--only` list, so that each unit's pairs are all
  compared: SUBMIT-CARD-PREVIEW's (`p5-transcript-*`), CATALOG-REFRESH's
  (`h1-models*`, `settings-model*` more), PASTE-GAP's (its 41 names);
- the scenarios of each of the 18 ledger entries the train changed (the cause
  SUBMIT-CARD-PREVIEW added and the one it deleted, the conflict blocks'
  entries, every re-pointed one; an entry naming every scenario adds none).

**Results** (`sweep-*.md`, `families.txt`; pass / pairs, each sweep scored
with the head's ledger, over its own pairs):

| family | head | main | unit-submit-card-preview | unit-catalog-refresh | unit-paste-gap |
|---|---|---|---|---|---|
| the submit scenarios (p5-submit-preview-*, *submit*) | 31 / 31 | 9 / 9 | 31 / 31 | - | 9 / 9 |
| p5-submit-preview-* (SUBMIT-CARD-PREVIEW's) | 22 / 22 | - | 22 / 22 | - | - |
| the other p5-transcript-* (SUBMIT-CARD-PREVIEW's sweep's) | 42 / 98 | 42 / 98 | 42 / 98 | - | 2 / 2 |
| model and catalog (model*, p5-input-model*, p5-catalog-*, h1-models*, settings-model*) | 118 / 128 | 120 / 128 | - | 77 / 92 | 26 / 32 |
| the approvals (approvals*) | 2 / 18 | 2 / 18 | - | - | 2 / 18 |
| the pastes (paste*, p5-paste*, p5-input-paste*) | 27 / 33 | 27 / 33 | - | - | 27 / 33 |
| the other p5-input-* (PASTE-GAP's sweep's) | 22 / 38 | 22 / 38 | - | - | 17 / 32 |
| PASTE-GAP's sweep's others (card-wrap, composer-wrap, compact-toolarge, image-*, question*, skills-paste, p5-effort-fork-*, p5-panels-keys, p5-session-early-prefs) | 117 / 127 | 117 / 127 | - | - | 117 / 127 |
| the changed ledger entries' others (interrupt2, model-parked, allowlist2, fulltranscript*, links, read-failures, resume-tools, timeout, p5-ctrlo-wider-inline-fam, h1-resize, p5-toolgroup-ctrlo*) | 45 / 91 | 44 / 91 | - | 4 / 5 | - |
| every scenario swept | 399 / 559 | 379 / 537 | 73 / 129 | 77 / 92 | 200 / 253 |

The head: 101 scenarios, **0 failed, 0 timed out**, 559 pairs, **399 pass**
(71.4%), in 2317 s, load 14.0 at its start and 49.0 at its peak.

### Against `main` (`compare-main.txt`, `screen-moves-main.txt`, `screen-classes.txt`)

**537 shared pairs: 379 -> 377; 1 up, 3 down** [measured]:
- **down: `model-column-idle` `col-down`, `col-idle`, `col-typed`.** The
  ledger registers this as nondeterminism: the cause `catalog-window`
  (`scripts/fxcap/parity-ledger.toml:1059-1066`; owner FLAKE-MODELSDOWN,
  registry #19; scenarios `model-column-idle` among them, snaps `col-*`,
  `whole_snap`). The scenario's catalog is down for the first 9000 ms of the
  fake Gateway's life (`modelsdown 9000`, wall clock); at the head's snap fx
  says `unable to load models` and xfx lists the rows. **What the files
  show** (`asks.py` -> `asks.txt`, every run's asks from its gateway log;
  `first_asks.py` -> `first-asks.txt`):
  - A binary asks twice: at launch, and 6078-6137 ms later (11 gaps, median
    6102 ms). That gap is the scenario's `wait 6` before it types `/model `:
    the second ask is the column's, not a timer's retry (opus's reading).
  - So a binary lists the rows when its first ask is at or past
    9000 - 6102 = 2898 ms, and a pair passes when both binaries' first asks
    fall on the same side of ~2.9 s. That rule gives **all 8 runs' verdicts**
    (the two sweeps and three reruns of each) **and all 16 binaries' rows**.
  - The head's sweep: xfx first asked at 4027 ms (past), fx at 2119 ms
    (before); `main`'s: xfx 908 ms, fx 1244 ms (both before). The reruns
    (`reruns.sh`, `reruns.txt`): **the head 2 of 3 pass, `main` 3 of 3**; the
    head's failing rerun is xfx 1106 ms (before) against fx 5744 ms (past),
    the other way round.
  - First asks, medians (`first-asks.txt`): over both wall-clock scenarios
    (8 runs a binary, as opus counted them), xfx 6.01 s in the `main` runs and
    5.47 s in the head's, fx 4.74 s and 4.83 s; over `model-column-idle` alone
    (4 runs), xfx 8.34 s and 4.57 s, fx 5.90 s and 4.83 s. The head's xfx does
    not start later than `main`'s.
  
  The files show where each run's first asks fell, not why they fell there;
  they do not rule the code out. What settles it is the cause's own fix: an
  outage counted in catalog requests rather than milliseconds since the
  gateway started (`catalog-window`'s `fix`).
- **up: `model-parked` `menu-loading`**: its catalog is down for the first
  6000 ms (`modelsdown 6000`), and the pair is in the ledger under
  `activity-tokens-before-request`. Swept three times more on each: **the head
  0 of 3, `main` 1 of 3**; it passed only where both binaries first asked past
  6 s (the head's sweep, `main`'s second rerun; `asks.txt`). So the head's up
  is not a change it can be credited with.

The screens that moved beyond clock-like or run-local text
(`screen-classes.txt`): 45 lines a saved session's id, 4 the `/status` Build
row (each sweep's own build), 4 the catalog's wall-clock outage (the two
scenarios above and `model-reask` `menu-failed`, where fx's screen moved at
the same rows). Rows gained on pairs unresolved on both (`compare-main.txt`):
`p5-panels-keys` x7, `p5-effort-fork-fork-edge` `status-empty` -- the
`/status` Workspace row's temporary directory -- and `read-failures`
`live-ctrl-o-up1`, a path in a tool's error: run-local text, the same rows.

### Against each unit's own sweep (`compare-unit-*.txt`)

| unit (its sweep's run) | shared pairs | pass, unit -> head | moved |
|---|---|---|---|
| SUBMIT-CARD-PREVIEW (`b0c8d70e`, round 3; its 29) | 129 | 73 -> 73 | **0 up, 0 down**; `p5-submit-preview-first` `block`, `block-held` identical -> normalized-identical (both pass) |
| CATALOG-REFRESH (`3d96f590`, round 2; its 20) | 92 | 77 -> 82 | **8 up, 3 down**: up, MODEL-FUZZY's six `p5-input-model` queries and `model-anchor` `alt-enter` (train 10, on main, not on the unit's base `e705a5dc`) and `model-parked` `menu-loading`; down, the three `model-column-idle` pairs (above) |
| PASTE-GAP (`ea2b8a60`, round 1; its 41) | 253 | 200 -> 210 | **10 up, 0 down**: AT-HEIGHT's `p5-input-at-dir` `dir-query`, MODEL-FUZZY's six `p5-input-model`, QUESTIONS-20's three `p5-input-question` `q6` (all on main, not on the unit's base `a832f4d7`). Rows gained: AT-HEIGHT's taller `@` list (`p5-input-at` x5), and the run-local `/status` rows |

**So no pair that passes on `main`'s sweep or on any unit's own fails on the
train's, but the three `model-column-idle` pairs**: the registered
`catalog-window` nondeterminism, whose 8 verdicts the first asks' times give
(reruns: the head 2 of 3, `main` 3 of 3); not settled until that cause's
fix. Every other move is a unit's change, a capture's run-local text, or the
`model-parked` window the other way.

## Redaction

The repository is public. Every file here taken from a gate log, a job, the
logger, a sweep or a clone went through `redacted.py` (train 7's, unchanged):
`scripts/fxcap/redact.py`'s `Redactor.write`, every path under a local root
to `<local-path>` or a named root (`<scratch>`, `<clone>`, `<repo>`,
`<sweep-...>`), a checkout's path cut short to `<checkout>/`, the account's
name to `<user>`. The scripts take the oracle's path, the suite lock and the
scratch directories as arguments or variables. The brief's check -- the added
lines of each commit this train adds (a merge read as everything it adds,
`git diff <merge>^1 <merge>`) and of `32afd307..<tip>`, against the home
root, the harness temp roots, the macOS per-user temp root, the account's
name and `claude-<n>/` -- was run before every commit (each commit gated on
it, `set -o pipefail`, an explicit `if`) and over the whole range by
`redaction_grep.sh` (`redaction-grep.txt`; its pattern spelt with bracket
classes so the file does not match itself; every line the last term matches
is listed there, to be read). `gates-files.sh` in a clone at the receipt's
final form (`gates-files.txt`; round 3's, `gates-files-r3.txt`).

## Files

- Merges: `blocks.py` (diff3 blocks, one rule each), `rowmerge.py`,
  `rowcheck.py`, `filecheck.py` (train 10's), `semantic.py` (merge 2's fixes;
  `--reverse`), `regroup.py`, `reproduce.sh`, `check-merge.sh` /
  `check-merges.txt`, `resolutions.txt`, `remerge-1.txt` .. `remerge-3.txt`,
  `overlap.py` / `overlap.txt`.
- `merge-checks.sh`: writes `resolutions.txt`, the token check and its
  control, the remerge-diffs, the scans, the re-points' check, the stale
  list, the shared functions, the loop order, the helpers, the overlaps, and
  checks `selection.txt`.
- `token_check.py` (train 7's, byte for byte), `token_control.py`,
  `token-check.txt`, `token-self-test.txt`.
- `refs.py`, `refs_all.py`, `refs_short.py`, `refs_bare.py` (trains 7's and
  9's); `refs-pre.txt`, `refs-*-pre.txt` (at `d2ed0274`), `refs-head.txt`,
  `refs-*-head.txt` (at `820a8f15`);
  `repoint.py`, `repoint-plan.txt`, `repointed.py`, `repointed.txt`;
  `stale.py`, `stale.txt` (round 2: the re-pointed stale references).
- Join checks: `join_test_paste.rs`, `join_test_catalog.rs`, `apply_join.py`;
  `join_helpers_catalog.rs`; `helpers.py` / `helpers.txt`; `shared.py` /
  `shared.txt`; `loop_order.py` / `loop-order.txt`; `join_bytes.sh` /
  `join-bytes.txt`; `join_mutants.py`; `runtime_new.py` / `runtime-new.txt`.
- `jobs.sh` (parts `named`, `alone`, `mutants`, `net`), `jobs-*.txt`,
  `alone-failures.txt`, `net.py`, `connect.log`, `net_rerun.py`,
  `net-rerun.txt`.
- `gates.sh`, `gates_txt.py`, `gates.txt`, `gate-counts.txt`;
  `test_counts.sh`, `test-counts.txt`; `gates-files.sh`, `gates-files.txt`;
  `redaction_grep.sh`, `redaction-grep.txt`; `redacted.py`.
- `selection.py`, `selection.txt`; `sweep.sh`, `sweep-rerun.sh`, `reruns.sh`,
  `reruns.txt`, `asks.py`, `asks.txt`, `first_asks.py`, `first-asks.txt`;
  `sweep-runs.sh`, `sweep-runs.txt`;
  `sweep-*.md`; `sweep-compare.sh` / `sweep-compare.txt` (compare.py's exits), `compare.py`, `screen_moves.py`,
  `screen_classes.py`, `families.py`, `banners.py`; `compare-*.txt`,
  `screen-moves-*.txt`, `screen-classes.txt`, `families.txt`.
- Round 3: `ci-young-clock.txt`; `young-clock.sh` (parts `red`, `sub`),
  `young_clock.py`; `young_scan.py`, `young-scan-3be49c15.txt`,
  `young-scan-c4354ec7.txt`, `young-superset-c4354ec7.txt`; `join-bytes-r3.txt` (`join_bytes.sh` now takes
  the head); `gates-r3.txt`, `gates-files-r3.txt`.
