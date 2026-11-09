---
title: 0.0.16 delta -- P5 train 10 receipt (CATALOG-BOUNDS, MODEL-FUZZY, COMPACT-NOTICE-LATE, AT-HEIGHT into one branch)
type: receipt
tags: [xfx, phase5, fx-0.0.16, train]
---

# P5 train 10: four closed units, one branch

Branch `train/p5-1109c`, from `origin/main` `6bb9fd90` (#87, train 9), as the
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
| 1 | CATALOG-BOUNDS | `feat/p5-catalog-bounds` @ `e705a5dc` | `13e7c9a0` | `ccced1f5` (#86, train 8) | `docs/parity.md`, the `/model` row (not forecast: see below) |
| 2 | MODEL-FUZZY | `feat/p5-model-fuzzy` @ `8d9ab0a0` | `33e476fa` | `ccced1f5` | `docs/parity.md`, the `/model` row (forecast) |
| 3 | COMPACT-NOTICE-LATE | `fix/p5-compact-notice-late` @ `96cc7849` | `5a98001a` | `58c92ba2` (#84) | `src/tui/event_loop.rs`, the tests module's end (forecast) |
| 4 | AT-HEIGHT | `feat/p5-at-height` @ `af2b6137` | `64873172` | `712dd8d1` (#85) | `src/tui/event_loop.rs`, the tests module's end; `scripts/fxcap/parity-ledger.toml`, nine blocks of reference numbers (both forecast) |

Every base is git's merge base. AT-FOLDERS (built on AT-HEIGHT) is not in
this train. Every merge tree passed `cargo check --all-targets` with and
without `fault-injection`, `cargo fmt --check` and clippy (both) before it
was committed (`check-merges.txt`). Each conflicted file is checked from
both ends -- ours -> merged is exactly theirs' edits, theirs -> merged
exactly ours' -- and every resolution was redone by this folder's scripts in
a clone and gave the merge commit's tree (`resolutions.txt`).

**At a glance.**
- **Merges:** four `--no-ff` merges, in order, each with a conflict. Merge 1
  (CATALOG-BOUNDS) was forecast clean and was not: train 9 had put
  STOP-IN-CATALOG-WAIT's paragraph into the `/model` row it edits. Merges 1
  and 2: the `/model` row, merged token by token (the sides' edits touch
  disjoint places). Merges 3 and 4: both sides appended tests at the end of
  `event_loop.rs`'s tests module, kept whole, ours first. Merge 4 also: nine
  ledger blocks that differ only in reference numbers, kept as ours' and
  re-pointed after. Every resolution checked from both ends and redone by
  script in a clone to the merge commit's tree.
- **Token check** (train 7's script, byte for byte): **all four merges OK**.
  Its negative control finds 13 of 14, 13 of 15, 13 of 13 and 16 of 16; the
  three it misses plant nothing (its token cases look for one side's text of
  a row both sides changed); planted on the merged row instead
  (`token_control.py`): **11 of 11**.
- **References:** after the last merge, **26 moved** -- 13 in the ledger, 2 in
  `docs/parity.md`, 11 in code and harness comments; **4 of the 26 bare
  continuations** (`capfile.py`'s `:14603-14605`, the one train 9 missed, among
  them) -- re-pointed in one numbers-only commit; `repointed.py` **27 of 27
  the same code**; `refs.py` 0 moved; `refs_bare.py` none moved outside the
  plans but two substring false reads, explained.
- **Join checks:** four tests added (one each for checks 1 and 2, two for 3),
  each with what covered the meeting before, each run on its sides alone
  (three fail on one side or do not build on it; the question test passes on
  AT-HEIGHT alone and pins routing older than both units); **9 of 9 mutants
  killed**, every one by a join test or a unit's test, none hung.
  Check 2's shared functions: 23, each changed by at most one of the two
  units. **Network: no CONNECT to the Gateway** (2, both the MCP probe).
- **Gate** at `e28e1b15`: **20 of 20**; `cargo test` **5539** passed, with
  fault injection **5576** (each +58 on train 9's: the units' 54 and the four
  join tests); smoke-tui 1167 checks, `test-capture` 79 OK.
- **Sweep** at `e28e1b15`, 120 scenarios: **451 of 640 pass, 0 failed, 0
  timed out**. Against `main`'s sweep of the same selection: **388 -> 406
  over 590 shared pairs, 18 up, 0 down**. Against each unit's own: **0 down**
  (9, 0, 0 and 12 up). Every move is explained below; the two pairs that move
  with the run (`model-parked` `menu-loading`, `workspace-index` `at-plan`)
  were swept three times more.

After the merges, in order:

| commit | what |
|---|---|
| `b5e242e7` | the merges' scripts; train 9's copied |
| `589248ac` | the 26 references the merges moved, re-pointed; numbers only |
| `de663b7c` | the four join tests; test only -- **the code head** |
| `e28e1b15` | the rest of the scripts; **the gate, the head sweep and every job ran here** (code of `de663b7c`) |
| `5f07b290` | the merge checks' files, at `e28e1b15` |
| `0b7700f4` | the gate's, sweeps' and jobs' files; this README |
| the last one | `gates-files.txt`, `redaction-grep.txt` |

**No code changed after the gate**: `git diff --stat e28e1b15 0b7700f4 -- .
':!.prd/tui-phase5/receipts/train-10'` is empty (`gates-files.txt`), so the
gate, the sweep and every job ran on the code of the branch's tip.

`git show --remerge-diff` of each merge is `remerge-1.txt` .. `remerge-4.txt`.

### Merge 1's conflict: the `/model` row (not forecast)

The brief said CATALOG-BOUNDS merged cleanly onto its base `ccced1f5`; onto
`6bb9fd90` it does not. Since `ccced1f5`, train 9's STOP-IN-CATALOG-WAIT put
a paragraph into the same row ("**A stop during the turn's own wait** ..."),
and git's line merge sees one line both sides changed. The two sides' edits
touch different places of the row: CATALOG-BOUNDS inserts the sleep-clock
sentence at base token 1446 and rewrites the bounds clause (1986-1993) and
the retry sentence (5206-5243); STOP-IN-CATALOG-WAIT's paragraph is one
insertion at base token 1733 (tokens are runs of non-blanks and of blanks,
`rowmerge.py`). **Merge 1's commit message says that paragraph sits "287
tokens before the unit's first edit": it sits 287 tokens after it** (1733 against
1446); the history is not rewritten, so the correction is here. Resolved
by `rowmerge.py`: the base row with both sides' edits applied; it stops on
edits that overlap or meet, and found none. `rowcheck.py`: ours -> merged is
exactly theirs' 12 edits and theirs -> merged exactly ours' one.

### Merge 2's conflict: the `/model` row again (forecast)

MODEL-FUZZY rewrote the menu's search, the column and the router form in the
row that now held train 9's paragraph and merge 1's edits. The same
resolution (`rowmerge.py`, base `ccced1f5`): 11 edits of theirs and 13 of
ours, none overlapping; `rowcheck.py` both ways true.

### Merge 3's conflict: the end of `event_loop.rs`'s tests (forecast)

Main's last 267 lines of tests (train 8's join test, PASTE-AFTER-END's
`Readable` helper and its 32-read carry-over test) and COMPACT-NOTICE-LATE's
243 (its `COMPACTION_CANCELLED`, helpers and two tests) were both appended after
base line 15841 (`58c92ba2`); git lined up the few lines they share and
split the rest into two hunks. Resolved as train 9 resolved its two: each
block whole, main's first (`append_both.py`). `filecheck.py`: ours -> merged
is exactly theirs' one hunk; theirs -> merged exactly ours' seven, one of
them git's diff slid by its leading blank line (the blank between two blocks
inserted at one place can be read as either's), which the check allows only
for blank lines. Every other file both sides changed (`worker.rs`,
`shell.rs`, `docs/parity.md`) git merged without a conflict; each checked the
same way, true both ways (`resolutions.txt`).

### Merge 4's conflicts: `event_loop.rs` again, and nine ledger blocks (forecast)

- **`event_loop.rs`**: ours' 510 appended lines (merge 3's two blocks) and
  AT-HEIGHT's 344 (its `AtList` harness and tests) after base line 15841
  (`712dd8d1`). Each whole, ours first; AT-HEIGHT's product hunks
  (`adopt_resume`, `landed_band`, `paint_alternate`) as git merged them.
  `filecheck.py`: true both ways (six and seven hunks, one slid by a blank).
  `append_both.py` was rewritten for this merge (to keep git's merge outside
  the conflict span; the first version took ours' whole file, right for merge
  3, where theirs had no other hunk in the file). `reproduce.sh` redoes all
  four merges with the scripts as committed, in a clone: **each gives its
  merge commit's tree, no difference** (`resolutions.txt`), merge 3 included.
- **The ledger**: nine blocks, each the same on both sides once every
  reference number is masked (`numbers_only.py` refuses any other): main
  (train 9's re-points) and AT-HEIGHT (its own) re-pointed the same
  `shell.rs` references -- `send`, `full_transcript::shown_event`, `resize`,
  the `tools: {} advertised` row six times -- each to its own branch's lines.
  Resolved as ours' numbers, then moved to the head's code in the re-point
  commit. `filecheck.py --numbers-masked`: what differs from ours + theirs is
  numbers alone, true both ways. **Each of the four pairs maps to one line at
  the head**: main's 38716 and AT-HEIGHT's 38921 both to 39046, 13405 and
  13506 to 13498, 13094 and 13195 to 13187, 16303 and 16412 to 16413.

### Where the sides meet (`overlap.txt`)

- **Merge 1**: `docs/parity.md` alone, the `/model` row (above).
- **Merge 2**: `docs/parity.md` (the row) and `src/tui/shell.rs`, 6267 base
  lines apart (MODEL-FUZZY's one test comment against train 9's tests).
- **Merge 3**: `docs/parity.md` (5 lines apart), `event_loop.rs` (the
  conflict), `shell.rs` (102 lines apart) and `worker.rs` (258): join check 2.
- **Merge 4**: `docs/parity.md` (the `@` row and the full-screen row, one line
  apart), the ledger and `event_loop.rs` (the conflicts), `shell.rs` (687
  lines apart): join check 3.

## The token check (`token_check.py`, `token-check.txt`, `token-self-test.txt`, `token_control.py`)

Train 7's `token_check.py`, byte for byte (`cmp` against `../train-7/`),
over the four merges. **Result: all four OK** [measured]:

- **Merges 1 and 2**: the `/model` row is changed by both sides; every token
  each side added is there and none it deleted came back, 7 and 12 clauses
  each whole, contiguous and in its side's order; every other row one side's,
  in order; the ledger's entries main's, kept so.
- **Merge 3**: `/compact` is COMPACT-NOTICE-LATE's (260 tokens added, 0
  deleted, in order); the rest ours.
- **Merge 4**: the `@` file picker row is AT-HEIGHT's (420 added, 7
  deleted, in order); its three new causes added as written; nine entries
  "both sides changed its `path:line` numbers alone; kept, the numbers
  re-pointed" (train 4's and 7's rules); one comment line added, kept.

**The negative control**: `--self-test` finds **13 of 14, 13 of 15, 13 of
13 and 16 of 16** [measured]. **The three it misses plant nothing**: its two
token cases pick the first row a side changed -- the `/model` row -- and plant
on that side's text of it (`pm.replace(side, row, 1)`); in merges 1 and 2
both sides changed that row, so the merged row is neither side's text and the
replace finds nothing to replace. `token_control.py` (it runs
`token_check.py`'s own `check_parity`, the file unchanged) says so per merge
and plants the same two cases on the merged row itself, for each side that
changed it: **11 of 11 found**.

## The references (`repoint.py`, `repointed.py`, `refs-*-pre.txt`, `refs-*-head.txt`, `stale.txt`)

Train 7's `refs.py`, `refs_all.py`, `refs_short.py` and train 9's
`refs_bare.py`, unchanged, six sides: `main` `6bb9fd90` and the four tips. At
the code head before any re-point (`64873172`, `refs-*-pre.txt`) they flag,
outside `.prd/` and the historical plans:

- the ledger: 13 `path:line` references (`capture.sh:317-319`, `:538`,
  `:973`, written on `main` and moved 8 to 14 lines by CATALOG-BOUNDS'
  `capture.sh` changes; `shell.rs`'s `send`, `shown_event`, `resize` twice, and the
  `tools: {} advertised` row six times, the merges' `shell.rs` lines);
- `docs/parity.md`: `worker.rs:3867` and its bare `:3550` (COMPACT-NOTICE-LATE's
  `worker.rs` lines above them);
- `capfile.py`: `shell.rs:14585-14610` **and its bare continuation
  `:14603-14605`** -- the one train 9 missed, moved again;
- `tests/tui.rs` x2, `smoke-tui.sh`, `event_loop.rs` x2, `shell.rs` x4 (two of
  them bare continuations).

**26 references, re-pointed in `589248ac`, numbers only** (25 lines out, 25
in, each the same once its numbers are masked). Each is read in the frame of
the side that wrote its citing line -- `git blame` of that line at the code
head names a commit, and the frame is the tip of the side that holds it
(`repoint-plan.txt`): main for 25, AT-HEIGHT for the ledger's `resize` at
`:16412` -- and its number mapped to the head through `git diff -U0`.

**After the last merge** (and the join tests, appended at their files' ends
so no cited line moves), at `e28e1b15`:

- **`repointed.py`: 27 of 27 the same code** [measured] -- the 26, and the
  ledger's bare `:1658`, which AT-HEIGHT wrote and which does not move in its
  frame -- each comparing the frame's cited lines and two either side with
  the head's at the number **the head's text holds** (read from the line,
  not from the plan).
- **`refs.py`: 82 references, 0 moved**.
- **`refs_bare.py`**: 73 bare continuations, 45 flagged; outside the plans
  two, both the scanner's substring rule and not a move: the head's
  `docs/parity.md:241` `:3556` (re-pointed, checked above) is read in `main`'s
  frame because `main`'s `docs/parity.md` holds `:3556` elsewhere (line 163,
  an fx `cli_ask.zig` continuation); the ledger's `:1658` is AT-HEIGHT's
  (above), and `main`'s ledger holds `:1658` in its own copy of that entry
  (line 845).
- `refs_all.py`, `refs_short.py`: outside the plans, only train 8's stale
  `frame.rs:1615-1628` (`CHANGED` for the frames older than GRID-ROW-COST).

**Not re-pointed** (`stale.txt`): the historical plans under
`docs/superpowers/plans/` (trains 8 and 9 left them for REF-LINT); the
ledger's bare `:1658` for `queue_card`, which is at `main`'s 1831 and the
head's 1840 -- stale before this train (train 7's `c468f5e1` wrote it; no
scanner read bare continuations until train 9); train 8's two in
`event_loop.rs`.

## Join checks

Four tests were added (`de663b7c`, test only; `join_test_*.rs`,
`apply_join.py`), each where no unit's test covers the meeting; each says in
its comment which tests cover each hop. Run by name at `e28e1b15` with the
units' tests on each meeting -- `jobs-named.txt`: 21 lib tests and 5 pty
tests, **all pass**; two more lib names in the list were spelt under the wrong
module and matched nothing, and ran in `jobs-alone2.txt` (**both pass**; the
committed `jobs.sh` has them under `shell`) -- in every gate step that runs
the suites, and against nine mutants (below). Each test on its sides alone:
`jobs-alone.txt`, `jobs-alone2.txt`, the failures' lines in
`alone-failures.txt`. Line numbers are the head's.

### 1. MODEL-FUZZY x CATALOG-BOUNDS: a catalog that fails, then loads

**The path, hop by hop** [read from the code]:
1. How a read fails is CATALOG-BOUNDS': `GatewayCatalog::fetch` maps an
   answer past 16 MiB to `retry: None` and a read still going at the
   deadline to `Some(CatalogRetry::Unreachable)` (`provider/model.rs`,
   `read_capped`'s arms); pinned by its
   `a_gateway_catalog_one_byte_past_16_mib_fails_and_is_not_cut_to_fit` and
   `a_gateway_catalog_past_its_deadline_is_a_failure_asked_again_about`.
2. `loaded` makes it `(CatalogState::Failed, retry)` (`model.rs:1605`), and
   `Applied::of` keeps rows the selector held **whatever the retry**
   (`(CatalogState::Failed(_), retry) if rows_held`, `model.rs:1495`);
   `told_event` (`worker.rs:4237`) tells the band `CatalogRefreshFailed` behind
   held rows (`:4246`), else `CatalogUnread`.
3. The band: `CatalogUnread` -> `menu.failed(retry)` (`shell.rs:4651`, the
   rows cleared, the query kept); `CatalogRefreshFailed` -> `menu.set_note`
   (`shell.rs:4688`, rows and mark stand); `CatalogLoaded` -> `menu.loaded`
   (`model_menu.rs:354`), which ranks through MODEL-FUZZY's `model_search` and,
   over rows it already listed, finds the marked id in the ranked list.

**What covered it already**: MODEL-FUZZY's
`better_matches_list_first_and_the_mark_keeps_its_model_through_a_revalidation`
reloads a loaded menu with no failure between; the older
`a_catalog_that_arrives_or_fails_is_what_the_open_menu_shows` goes Unread ->
Loaded with one row and no query. No test ranks after a failure or keeps a
mark across one. **Added**:
`a_catalog_read_that_fails_too_large_or_stalled_then_loads_ranks_in_three_tiers_and_keeps_its_mark_by_id`
(`shell.rs`): for each failure (`retry: None`, "Unable to load models."; and
`Unreachable`, "Could not reach AI Gateway; xfx retries automatically."): the
menu's first read fails and says so; `gpt5` typed; the next read lists
`acme/gpt5-lite`, `openai/gpt-5`, `openai/gpt-4.5-preview` (substring, words,
fuzzy); Down marks `openai/gpt-5`; a refresh fails the same way and rows and
mark stand; the next read adds `acme/gpt5-pro`, which ranks first, and the
mark stays on `openai/gpt-5`, now third (fx's own case,
`model_cache_runtime.zig:2086-2123@481104fd`, with the failures put in).

**On each side alone** (`jobs-alone.txt`; a clone, the test put in
unchanged): on CATALOG-BOUNDS `e705a5dc` it **fails** (`left:
["acme/gpt5-lite"]`, main's matcher); on MODEL-FUZZY `8d9ab0a0` it **passes**.
So the test is MODEL-FUZZY's menu under the two failure categories
CATALOG-BOUNDS' fetch produces, not a detector of an interaction: the band
code it drives is MODEL-FUZZY's and older, and CATALOG-BOUNDS changes nothing
under `src/tui/`. Mutants: F1 (no ranks) and C1 (a failed refresh drops the
rows) kill it; F2 (the mark looked up in catalog order) does not -- in this
catalog the marked model's catalog index and ranked index are both 2 --
and MODEL-FUZZY's unit test kills F2.

### 2. COMPACT-NOTICE-LATE x HISTORY-NOTICE-STOP: notices around a stop and a compaction (`shared.txt`)

**The shared functions, by hash, on every side** (`shared.py`, train 9's
widened; each unit's tip against its own base, reference numbers masked):
23 functions on the two paths. Every one is changed by **at most one** of the
two units, and the head's is that unit's:

| path | functions | changed by | the head's |
|---|---|---|---|
| the history notice | `one_turn`, `Conversation::unsaid` | HISTORY-NOTICE-STOP | its (= main's) |
| | `said`, the `Notice` arm, `announce`, `mark`, `abandon_answer`, `bridge::send_ui` | neither (or new in HNS) | the same everywhere |
| the compaction's end | `Queue::abandon`, `run_compact`, `turn_loop`, `compaction_update`, `settle_band`, `submit` | COMPACT-NOTICE-LATE | its |
| | `tell_dropped`, `reset_conversation`, `try_send_terminal`, `Compaction::answer` | new in COMPACT-NOTICE-LATE | its |
| | `drop_conversation`, `send_terminal`, `stop_compaction`, the reset's drain `drained`, `Compaction::settle` | neither | the same everywhere |

`settle_band`'s text at the head differs from COMPACT-NOTICE-LATE's only in
the two numbers the re-point moved (its comment's `worker.rs:3867`, `:3524`).
Where they meet [read from the code]: `turn_loop` (COMPACT-NOTICE-LATE's)
reaches `one_turn` (HISTORY-NOTICE-STOP's) through `run_turn`; a stop's
`Queue::abandon` (`worker.rs:3770`) drops queued work without running it, so
a dropped prompt never reaches `one_turn` and its notice stays unsaid; a
`Compact` dropped is told `Dropped` through `try_send_terminal`
(`bridge.rs:1526`), or later by `tell_dropped` (`worker.rs:3786`, called at
the loop's top `:3301` and before a reset's `drop_conversation` `:3591`);
`one_turn` marks a notice said only once `send_ui` has queued it
(`worker.rs:6263`); the reset's drain drops a `Notice` and a `Compaction`
alike (`shell.rs:15564`, `:15607`), with the conversation and its ledger.

**The brief's two cases, and what covers each.** *A stop during a running
compaction*: COMPACT-NOTICE-LATE's `ctrl_c_stops_a_compaction_that_is_waiting_for_its_notes`
(pty; the CI flake the unit fixed) and
`a_stopped_compactions_sentence_the_next_line_took_away_stays_away` (the
sentence said once, and a line after it takes it away for good). *A late
compaction notice* (an ending that could not go when the compaction was
dropped): `a_dropped_compactions_place_is_held_until_its_ending_is_on_the_channel`,
`a_compaction_dropped_with_no_room_for_its_word_is_told_once_before_the_reset_after_it`,
`a_compaction_dropped_in_a_turns_race_with_no_room_is_told_after_that_turn_and_before_more_work`
(told exactly once, before more work and before a reset). *Said only when
shown*: for the history notice, `one_turn`'s mark after `send_ui`
(HISTORY-NOTICE-STOP's
`a_turn_stopped_before_its_request_leaves_the_history_notice_to_the_next_turn`,
a stop in a catalog wait with no compaction, and
`the_history_notice_ledger_goes_with_its_conversation`); for the compaction's
sentence, the band settles it only on the runtime's word
(`a_line_sent_while_a_compaction_stops_takes_the_sentence_only_of_one_that_never_began`,
`a_compact_typed_while_a_reset_drains_answers_as_upstreams_would`). All of these
pass in the gate's `cargo test` at the head; those `jobs.sh` names, also
by name (`jobs-named.txt`, the last two in `jobs-alone2.txt`). None
has a history notice owed across a stop that drops a compaction. **Added**
(it covers a **queued** compaction a stop drops before it began, not a running
one -- the running case's ending goes through `run_compact`'s
`send_terminal` and no notice path, as the table shows):
`a_stop_through_a_dropped_compaction_tells_it_once_and_leaves_the_history_notice_to_the_turn_that_runs`
(`worker.rs`, the real `turn_loop`): a prompt stopped in its catalog wait
(HISTORY-NOTICE-STOP's `WindowRetry` case, its `sealed_on_llmux` notice
owed), a `/compact` queued behind it and a third prompt; a Ctrl-C quoting the
first two, 300 ms in. Result: ends `[TURN_CANCELLED, None]`, the notice said
`[0, 1]` (none by the stopped turn, once by the third), **one** compaction
word, `Dropped`, told after the first start and before the second, every
place given back.

**On main alone** (HISTORY-NOTICE-STOP, no COMPACT-NOTICE-LATE; `jobs-alone.txt`):
**fails** -- ends and notices as on the head, but no compaction word at all
(`[]`). On COMPACT-NOTICE-LATE alone it is not run: its tree lacks the
catalog-wait helpers (`CatalogWait`, `waiting_in`; STOP-IN-CATALOG-WAIT's) and
the notice's (`sealed_on_llmux`; HISTORY-NOTICE-STOP's) that the test's first
prompt uses (`helpers.txt`). Mutants: N1 (a
dropped compaction not told) and N2 (a notice marked said whether or not it
was handed on) each kill it, and each is killed by its own unit's test too.

### 3. AT-HEIGHT x PASTE-AFTER-END, and x QUESTIONS-20

**A paste into the `@` list** [read from the code]: the bytes behind a paste's
end marker in its read never become keys -- `Decoder::feed`'s first statement
returns for them (`input.rs:471-509`, **byte for byte PASTE-AFTER-END's
`a832f4d7` and the whole file the same**, `invariant.txt`), so neither
`offer_to_files` nor the composer sees that Enter. The paste's bytes are no
`Action` (`offer_to_files` returns at `let Input::Action(action) = event else`,
`shell.rs:5254`) and go to `self.paste` (`:12115`); its end applies the text,
the query changes, and the new list is trusted by the frame that shows it
(`file_picker_landed`, `:5381`). No unit has a test of a paste into the `@`
word: AT-HEIGHT's tests type into it, PASTE-AFTER-END's paste into the
composer and the question's field. **Added**:
`a_paste_into_the_at_word_with_an_enter_behind_its_marker_is_kept_trusted_and_sends_nothing`
(`event_loop.rs`, AT-HEIGHT's `AtList`, real frames): the bar on `f-03`,
then `ESC[200~0ESC[201~` and a CR in one read: the composer `@f-0` (not the
marked file), nothing sent, the warning written, the new rows from the first;
the first Down moves the bar (trusted by the paste's own frame); a plain
paste of `1`, and an Enter of its own takes the marked `items/f-01.txt`.
**On AT-HEIGHT alone** (PASTE-AFTER-END's two test helpers copied in, test
code only, `join_helpers_paste.rs`; `jobs-alone2.txt`): **fails** -- the
composer `@f-` where the head has `@f-0`: before PASTE-AFTER-END a paste with
input behind its marker was refused whole. On PASTE-AFTER-END alone it is not
run: `AtList` is AT-HEIGHT's (`helpers.txt`). Mutants X1 (the bytes behind the
marker decoded: the composer becomes `@items/f-03.txt 0`) and X6 (no warning)
kill it.

**A question panel over the `@` list** [read from the code]: the model's
batch is asked first of every surface but the full transcript
(`if self.ask.is_some()`, `shell.rs:12018`, before `offer_to_files` at
`:12093`), and its arrival dismisses the list (`ask_question` ->
`dismiss_picker`, `:9666`-`:9682`), so no key of the batch can reach the list
and the answer does not put it back. Covered before only by
`a_question_dismisses_the_file_list`, which calls `dismiss_picker` itself.
**Added**:
`a_twenty_question_panel_over_an_open_at_list_has_its_tab_arrows_and_enter`
(`shell.rs`): a turn running, `@` and its list up with a row Tab or Enter
would take; twenty questions: the list is gone; Tab x9 is `Question 10 of
20`; Down and Enter answer question 10 and show `Question 11 of 20`; the
composer still `@`, nothing sent, nothing answered yet. It **passes on
AT-HEIGHT alone** (`jobs-alone.txt`) and **does not build on main**
(`FilePicker::chosen` is AT-HEIGHT's; `alone-failures.txt`). AT-HEIGHT's
tree has the tool's old cap of 4 (`MAX_QUESTIONS`, `src/tools/question.rs`;
QUESTIONS-20 raised it to 20), but the test hands the band its batch
directly, past the tool, and the panel's pages and counter (`Question N of
M`, `src/tui/question.rs`) predate both units. So it pins the meeting -- the
panel's keys, which only a batch of more than one page needs, ahead of the
list's -- rather than joining two changes; QUESTIONS-20 is what makes a
twenty-page panel reachable from the model. Mutant
Q1 (no dismissal) kills it. Q1+Q2 (the routing reversed too) fails at Q1's
assertion, so the routing's own effect is not measured apart: with the
dismissal kept the list is gone before any key, and the order cannot
matter.

### Mutants (`join_mutants.py`, `jobs-mutants.txt`)

On a `git clone --no-local` at `e28e1b15`, each cargo run bounded at 900 s,
the files restored after each, the clone clean at the end; against the four
join tests and one test of each unit on the path. The unmutated clone passes
all nine. **9 of 9 killed, none hung** [measured]:

| mutant | join-catalog | join-compact | join-paste | join-question | the units' |
|---|---|---|---|---|---|
| F1 no ranks (MODEL-FUZZY's) | **FAILED** | ok | ok | ok | fuzzy FAILED |
| F2 the mark in catalog order (MODEL-FUZZY's) | ok | ok | ok | ok | fuzzy FAILED |
| C1 a failed refresh drops the rows | **FAILED** | ok | ok | ok | ok |
| N1 a dropped compaction not told | ok | **FAILED** | ok | ok | cnl FAILED |
| N2 a notice said whether or not handed on | ok | **FAILED** | ok | ok | hns FAILED |
| X1 the bytes behind the marker decoded (PASTE-AFTER-END's 1) | ok | ok | **FAILED** | ok | paste FAILED |
| X6 no warning (its 6) | ok | ok | **FAILED** | ok | paste FAILED |
| Q1 the question leaves the list up | ok | ok | ok | **FAILED** | ok |
| Q1+Q2 and the list offered keys first | ok | ok | ok | **FAILED** | ok |

### 5. Network hermeticity (`net.py`, `jobs-net.txt`, `connect.log`)

Train 9's method (FLAKE-CATALOG-EVENT's `connect_logger.py`, a loopback
logger that answers 403 and forwards nothing; the four proxy variables at it,
`NO_PROXY`, `no_proxy`, `ALL_PROXY`, `all_proxy` unset), a clone at
`e28e1b15`, one hold of the lock:

| run | result | CONNECTs logged during it |
|---|---|---|
| `cargo test --locked --lib --no-fail-fast` | 4617 passed, 0 failed, 7 ignored | **2**, both `CONNECT w.example:443` |
| `cargo test --locked --test tui --no-fail-fast` | 186 passed, 0 failed | **0** |

**No CONNECT to the Gateway host** [measured]; the two `w.example` lines are
the known MCP probe (trains 6 to 9).

## Gate (`gates.sh`, `gates.txt`)

`gates.sh` (trains 5's to 9's, its comment only changed): every step of
`.github/workflows/ci.yml` and `sweep.yml`'s two harness tests, in a `git
clone --no-local` at `e28e1b15` (the code of `de663b7c`), one after another,
**the whole script inside one hold of the suite lock**, each step's exit
taken from `$?` on the line right after its command, the tree clean before
and after. **20 steps, 0 failed, none run again** [measured]:

| step | at `e28e1b15` |
|---|---|
| `cargo fmt --check`; clippy; clippy `--features fault-injection` | ok; ok; ok |
| `cargo test --locked --no-fail-fast` | **5539 passed, 0 failed, 7 ignored** (lib 4617, `tui` 186) |
| the same `--features fault-injection` | **5576 passed, 0 failed, 7 ignored** (lib 4640, `tui` 200) |
| `check-xfx-identity` / `check-no-stubs` / `check-no-secrets` | ok / ok / ok (17096 files) |
| `check-preview-contract` / `check-tui-preflight-cost` / `check-usage-clock` | ok / ok (200x300 replay 1.37 ms, preflight 0.73 ms; budget 32) / ok |
| `test-check-no-secrets` / `test-gate-env` / `test-preview-contract` | ok / ok / ok (24 mutants) |
| `fxcap/test-sweep.sh` / `fxcap/test-capture.sh` | 70 + 27 OK / 79 OK (`gate-counts.txt`) |
| release build; `fault-injection` release build | ok; ok |
| `smoke.sh` / `smoke-tui.sh` | 47 checks, 0 failures / 37 scenarios + the oracle, 1167 checks, 0 failures |

Load at the steps' starts 11.8-72.8 (other workers' gates, sweeps and this
train's own head sweep, each under its own slot). `test-capture`'s 79 is
train 9's 76 plus CATALOG-BOUNDS' three harness tests.

**Every side's tests are in** (`test-counts.txt`, train 7's count: lines
holding `#[test]` under `src` and `tests`): `main` 5394; CATALOG-BOUNDS +9
(5383 against its base's 5374), MODEL-FUZZY +10 (5384 / 5374),
COMPACT-NOTICE-LATE +11 (5371 / 5360), AT-HEIGHT +24 (5384 / 5360): **5448 at
the merges' head, 5452 with the four join tests**. `cargo test` 5539 against
train 9's 5481 at `main`'s code: **+58** = 9 + 10 + 11 + 24 + 4; with fault
injection 5576 against 5518, the same +58. The harness's `def test_` 176
(CATALOG-BOUNDS +3).

## Sweeps

**Builds** (debug, as trains 4 to 9), each in its own `git clone --no-local`,
each swept with its own tree's harness, scenarios and ledger (`sweep.sh`,
train 9's, reading this train's `selection.txt`). fx: the v0.0.16 binary,
sha256 `8e6cda2e…` before and after each sweep, `FX_AUTO_UPGRADE=0`; 40x120,
`--jobs 2`; each sweep inside one hold of the suite lock (`sweep-runs.txt`).
Every reference sweep -- `main`'s and each unit's own -- was rescored with the
head's ledger (`sweep.sh --rescore` on a copy, no capture taken); it moved
no pass count (main 388, MODEL-FUZZY 204, COMPACT-NOTICE-LATE 29, AT-HEIGHT 83,
as each scoreboard had them). Every xfx snap that shows the banner shows the
build swept: `v0.1.0-e28e1b1` on all 530 of the head's, `v0.1.0-6bb9fd9` on
all 480 of `main`'s (`banners.py`, `sweep-runs.txt`).

**Selection** (`selection.py`, `selection.txt`; 120 scenarios at the head,
111 on `main`, which lacks the units' nine new ones -- each scoreboard's
`Scenarios:` line):
- the brief's: `model*`, `p5-input-model*`, `p5-catalog-bounds-*`;
  `p5-input-at*`, `filepicker*` (no scenario matches `at*`); `compact*`,
  `*compact*`, `*stop*`, `interrupt*`, `p5-transcript-cancel`;
- each unit's own sweep's `--only` globs, so that each unit's pairs are all
  compared: CATALOG-BOUNDS' (`h1-models*`, `settings-model*` more),
  MODEL-FUZZY's (`*model*`, `*effort*`), COMPACT-NOTICE-LATE's
  (`p5-lifecycle-*compact*`), AT-HEIGHT's (`p5-input-*`, `composer*`,
  `workspace-index`, `startup`, `h1-resize`, `model-column`,
  `settings-scrollback`, `p5-mcp-config`, `p5-mcp-verbs`);
- the scenarios of each of the 18 ledger entries the train changed (merge
  4's three new causes, the nine conflict blocks' entries, every re-pointed
  one; an entry naming every scenario adds none).

**Results** (`sweep-*.md`, `families.txt`; pass / pairs, each sweep scored
with the head's ledger, over its own pairs):

| family | **head `e28e1b15`** | `main` | MODEL-FUZZY | COMPACT-NOTICE-LATE | AT-HEIGHT |
|---|---|---|---|---|---|
| model and catalog | 120 / 128 | 77 / 92 | 115 / 123 | - | 11 / 17 |
| `p5-catalog-bounds-*` | 5 / 5 | - | - | - | - |
| `p5-input-model-rank`, `-swap` | 31 / 31 | - | 31 / 31 | - | - |
| `*effort*` | 96 / 117 | 96 / 117 | 96 / 117 | - | - |
| the `@` list (`p5-input-at*`, `filepicker*`, `workspace-index`) | 28 / 45 | 8 / 31 | - | - | 27 / 45 |
| `p5-input-at-height`, `-resize`, `-gate` | 9 / 14 | - | - | - | 9 / 14 |
| the other `p5-input-*` | 42 / 53 | 42 / 53 | - | - | 17 / 25 |
| the compaction scenarios | 29 / 29 | 29 / 29 | - | 29 / 29 | - |
| the stop scenarios | 73 / 86 | 73 / 86 | - | - | - |
| the band's | 36 / 70 | 36 / 70 | 8 / 8 | - | 36 / 70 |
| the re-pointed entries' scenarios | 52 / 139 | 52 / 139 | 4 / 5 | - | - |
| **every scenario swept** | **451 / 640** | 388 / 590 | 204 / 233 | 29 / 29 | 83 / 149 |

The head: 120 scenarios, **0 failed, 0 timed out**, 640 pairs, **451 pass**
(70.5%), in 2892 s at load 60.5 at its start (the gate and other workers
beside it).

### Against `main` (`compare-main.txt`, `screen-moves-main.txt`, `screen-classes.txt`)

**590 shared pairs: 388 -> 406; 18 up, 0 down** [measured]. Up:
- AT-HEIGHT's ten: `filepicker` `at`, `at-claim`, `at-sch`; `filepicker-keys`
  `at-ck`, `at-pgdn`; `filepicker-narrow` `at-alpha`, `at-alpha-down`,
  `at-deep`; `p5-input-at-dir` `dir-query`; `workspace-index` `at-all` -- the
  ten its own receipt lists among its 19 (the other nine are its new
  scenarios, which `main` lacks);
- `workspace-index` `at-plan`: on `main` it is the height (rows 18-23, the
  six-row list). AT-HEIGHT's own sweep left it unresolved on one row: the
  same text on both sides, a path under the sweep's random temporary
  directory, whose bold letters differ (its README: "only the bold letters
  differ, on a long path", the `at-ranking` cause). Swept three times more at
  the head (`reruns.txt`): **1 of 3 pass**, the two others on that one row,
  each under another random directory (`fxsw.unin34ke`, `fxsw.5kmdob2x`). So
  the height is AT-HEIGHT's, and whether the last row passes is the run's
  directory name: an up of luck, not a change -- no drop either way;
- MODEL-FUZZY's seven: `p5-input-model`'s six queries and `model-anchor`
  `alt-enter`, as its receipt lists.

Rows gained on pairs unresolved on both (22 pairs; `compare-main.txt`), each
explained: AT-HEIGHT's taller list showing more rows under its own causes
(`p5-input-at` `at-src`, `at-st`, `at-st-down12`, `at-st-up6`, `at-stat`: `at-ranking`, `at-folder-row`);
run-local text -- the `/status` Workspace row's and `doctor`'s temporary
directory (`p5-effort-fork-*` three), a stdio MCP server's command path
(`p5-mcp-verbs` eleven), a path in a tool's error (`read-failures`
`live-ctrl-o-up1`), a network notice's milliseconds (`interrupt-recovery`
`full`); and `model-parked` `menu-loading` (below). The screens that moved
beyond clock-like or run-local text on xfx's side alone (`screen-classes.txt`):
37 lines a saved session's id, 20 AT-HEIGHT's list, 7 MODEL-FUZZY's rows, 3
the spinner's glyph in front of `Compacting` (a frame's timing), and one,
`model-parked` `menu-loading`.

**`model-parked` `menu-loading`** (unresolved on both; on the head xfx's
menu shows the catalog's four rows where fx and `main`'s xfx still show
`Models 0`). The scenario's catalog is down for six seconds from the fake
Gateway's start (`modelsdown 6000`, wall-clock: the ledger's
`catalog-window` cause, FLAKE-MODELSDOWN), so whether a binary's ask falls
inside the outage depends on when it starts under load. In the head's sweep
xfx first asked at 6670 ms -- past the outage, so its rows came at once --
and fx at 2581 ms, inside it; on `main`'s, both inside (773, 1264 ms). Swept
three times more on each binary (`reruns.txt`, each binary's asks from its
gateway log): **the head 3 of 3 pass (5 / 5 pairs each), `main` 2 of 3** (its
failing run with both asks inside the outage again). A wall clock, not the
code: no unit touches the catalog's band path for a parked menu (CATALOG-BOUNDS
changes only how a read fails; MODEL-FUZZY only the ranks).

### Against each unit's own sweep (`compare-unit-*.txt`)

| unit (its sweep's head) | shared pairs | pass, unit -> head | moved |
|---|---|---|---|
| CATALOG-BOUNDS (`c9ebc95`, its 24) | 97 | 80 -> 89 | **9 up, 0 down**: MODEL-FUZZY's seven, and `model-column-reask` `col-down`, `col-back`, which its own receipt names as FLAKE-MODELSDOWN's skew in its run (both pass 3 / 3 alone, its `reruns.txt`) |
| MODEL-FUZZY (`fc8ec00`, its 39) | 233 | 204 -> 204 | **0**; rows gained on four pairs unresolved on both: the three temporary-directory rows above, and `model-parked` `menu-loading` |
| COMPACT-NOTICE-LATE (`730d36d`, its 12) | 29 | 29 -> 29 | **0** |
| AT-HEIGHT (`3a76208`, its 25) | 149 | 83 -> 95 | **12 up, 0 down**: MODEL-FUZZY's six `p5-input-model`; PASTE-AFTER-END's `p5-input-paste` `paste-enter`, `.full` and QUESTIONS-20's three `p5-input-question` `q6` (both on `main`, not on the unit's base); `workspace-index` `at-plan` (above). Rows gained: eleven `p5-mcp-verbs` pairs, the checkout path in a command row, which moved fx's screen at the same rows |

**CATALOG-BOUNDS' compare is at the verdict level** (`compare_catalog_bounds.py`):
its sweep's captures were in a scratch directory since deleted; its committed
`sweep-compare.txt` names every pair of its head sweep that did not pass, and
counts the rest. The script reads a pair of its 24 scenarios as passing unless
that file names it failing, over the 97 pairs its scenarios' snap labels give
at `c9ebc953` (the file's own count, 97: the same; no pair the head took is
outside them). Its verdicts are its own ledger's (at `c9ebc953`), not
rescored: MODEL-FUZZY's sweep of a superset of its model globs, rescored with
the head's ledger, moved 0 of 233. `p5-catalog-bounds-stall.cap` changed after
that sweep (`6dda7f8a`: the snap waits for `Loading models…` to go), with the
same two snap labels.

**So no pair that passes on `main`'s sweep or on any unit's own fails on the
train's**, and every move is a unit's change, a capture's run-local text, or
the catalog window's wall clock.

## Redaction

The repository is public. Every file here taken from a gate log, a job, the
logger, a sweep or a clone went through `redacted.py` (train 7's, unchanged):
`scripts/fxcap/redact.py`'s `Redactor.write`, every path under a local root
to `<local-path>` or a named root (`<scratch>`, `<clone>`, `<repo>`,
`<sweep-…>`), a checkout's path cut short to `<checkout>/`, the account's
name to `<user>`. The scripts take the oracle's path, the suite lock and the
scratch directories as arguments or variables. The brief's check -- the added
lines of `git show` of each commit this train adds, and of
`6bb9fd90..HEAD`, against the home root, the harness temp roots, the macOS
per-user temp root and the account's name -- **0 for every commit and 0 for
the range** (`redaction-grep.txt`, run at `0b7700f4`; its pattern is spelt
there with bracket classes so the file does not match itself; the commit that
adds it and `gates-files.txt` is checked the same way in the hand-back), and
`gates-files.sh` (train 9's) in a clone at `0b7700f4`, the receipt in its
final form but those two files (`gates-files.txt`).

## Found on the way, not changed here

- **Merge 1 was forecast clean and was not**: CATALOG-BOUNDS merged
  cleanly onto `ccced1f5`, but train 9 had since put STOP-IN-CATALOG-WAIT's
  paragraph into the same `/model` row.
- **Merge 1's commit message** says STOP-IN-CATALOG-WAIT's paragraph sits
  "287 tokens before the unit's first edit"; it sits 287 tokens after it.
  Not rewritten (the brief: no history rewrite); corrected above.
- **`token_check.py`'s negative control plants nothing** for its two token
  cases when both sides changed the row it picks (`pm.replace(side, ...)`
  finds no side's text in the merged row): merges 1 and 2 here.
  `token_control.py` plants them on the merged row; carrying that into
  `self_test` is the next train's or REF-LINT's.
- **The ledger's bare `:1658` for `queue_card`** (the `trailer_rows` cause)
  names a doc comment on `main` and at the head; `queue_card` is at `main`'s
  1831 and the head's 1840. Stale since train 7 wrote it (`c468f5e1`); no
  scanner read bare continuations until train 9's `refs_bare.py`, which reads
  it as not moved in AT-HEIGHT's frame. Left for REF-LINT (`stale.txt`).
- **`refs_bare.py` matches a continuation by its text** in each side's copy
  of the citing file, so a continuation spelt the same elsewhere in that copy
  (`docs/parity.md`'s `:3556`, the ledger's `:1658`) is read in that side's
  frame and flagged; `repointed.py` (which reads the head's own line) shows
  both right. Carrying the citing line into the match is REF-LINT's.
- **The historical plans' references** under `docs/superpowers/plans/`:
  left, as trains 8 and 9 left them.
- **A slip in this receipt's own tooling, corrected before the files here
  were written**: `jobs.sh`'s first `named` list spelt two `shell` tests
  under `event_loop` (they ran in `alone2`), and `alone2`'s first run printed
  a command substitution's exit for its `exit` field; the script was fixed
  and `alone2` run again (`jobs-alone2.txt` is the second run).

## Files

- `rowmerge.py`, `rowcheck.py` (merges 1-2's row), `append_both.py`,
  `filecheck.py` (merges 3-4's tests tails; `--numbers-masked` for the
  ledger), `numbers_only.py` (merge 4's ledger blocks), `reproduce.sh` (every
  resolution redone in a clone), `check-merge.sh` / `check-merges.txt` (each
  merge tree's checks before its commit), `resolutions.txt`.
- `merge-checks.sh`: writes `resolutions.txt`, the token check and its
  control, the remerge-diffs, the scans, the re-points' check, the stale
  list, the invariant, the shared functions, the overlaps, and checks
  `selection.txt`.
- `token_check.py` (train 7's, byte for byte), `token_control.py`,
  `token-check.txt`, `token-self-test.txt`.
- `refs.py`, `refs_all.py`, `refs_short.py`, `refs_bare.py` (trains 7's and
  9's); `refs-*-pre.txt` (at the code head before the re-point),
  `refs-*-head.txt` (at `e28e1b15`); `repoint.py`, `repoint-plan.txt`,
  `repointed.py`, `repointed.txt`; `stale.py`, `stale.txt`.
- `join_test_catalog.rs`, `join_test_worker.rs`, `join_test_paste.rs`,
  `join_test_question.rs`, `apply_join.py`; `join_helpers_paste.rs`
  (PASTE-AFTER-END's helpers for the run on AT-HEIGHT alone);
  `join_mutants.py`; `helpers.py`, `helpers.txt` (which side defines each
  helper a join test names); `shared.py`, `shared.txt`; `invariant.py`,
  `invariant.txt`; `overlap.py`, `overlap.txt`.
- `jobs.sh` (parts `named`, `alone`, `alone2`, `mutants`, `net`), `jobs-*.txt`,
  `alone-failures.txt`, `net.py`, `connect.log`.
- `gates.sh`, `gates_txt.py`, `gates.txt`, `gate-counts.txt`;
  `test_counts.sh`, `test-counts.txt`; `gates-files.sh`, `gates-files.txt`;
  `redaction_grep.sh`, `redaction-grep.txt`; `redacted.py`.
- `selection.py`, `selection.txt`; `sweep.sh`, `sweep-rerun.sh`,
  `sweep-runs.txt`; `sweep-*.md`; `sweep-compare.sh`, `compare.py`,
  `compare_catalog_bounds.py`, `screen_moves.py`, `families.py`;
  `compare-*.txt`, `screen-moves-*.txt`, `families.txt`.
