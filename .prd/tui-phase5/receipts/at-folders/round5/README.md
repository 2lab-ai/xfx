---
title: 0.0.16 delta -- AT-FOLDERS round 5 (the index refresh follows the query; rule confirmation; stop before each advance)
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, input, file-picker]
---

# AT-FOLDERS round 5

Round 4's head `a7658343` was reviewed by two engines: gpt6 approved with
fixes (one MUST, two SHOULDs) and opus approved (four SHOULDs; its SHOULD 3
is gpt6's MUST). The dispatcher's decisions, all "copy fx": the index
refresh follows the query; the rule confirmation joins the predicate; the
listing stops before each advance; a real-routing Enter case; two doc fixes.
fx's lines are quoted in `../../at-height/fx-src.txt`, extended from line
1986. Lines 1-1985 are byte-identical to round 4's (checked with `cmp`).
`fx-src.txt:N` below means that file.

| commit | what |
|---|---|
| `df74d4b0` | RED (`red-unit.txt`): 5 cases fail on round 4's code; a test-only walk counter |
| `475fd5f1` | the fix, the quotes, `docs/parity.md`, the round-3 and round-4 receipt corrections, the Enter case -- **the code head** |
| `5f57b162` | the ledger's 12 `shell.rs` refs re-pointed (`refs.txt`: 16 checked, 0 wrong) |
| `945c4e20` | test: an undecodable query counts as indexed (fails with round 4's `indexed()`) |
| `489397bb` | test: two mutant targets (the request across keys; Tab's retry) |
| `ebdd34be` | `mutants.py` -- **the gate's commit** |
| (receipt commit) | this folder |

## 1. The index refresh follows the query (gpt6 r4 MUST; opus r4 SHOULD 3)

**fx at the pin:**
- `reconcile` asks for a refresh on a new episode and on a change between
  an indexed and an explicit query, and a request stands until it is taken
  (`file_completion_state.zig:164`, `fx-src.txt:2036`). `retry` asks too
  (`:230-233`, `fx-src.txt:2043-2046`).
- `indexed` is `fileCompletionsDependOnIndex`, which is
  `queryMode(query) == .workspace_index`. A query that cannot be decoded
  counts as indexed (`input_completion_runtime.zig:793-795`,
  `app_workspace_runtime.zig:82-84`, `path_completion.zig:25-27`;
  `fx-src.txt:1994-1996`, `:2009-2011`, `:2014-2016`).
- `prepareFilePicker` takes the request only while the list is eligible.
  It walks the index only `if (state.indexed)` (`:853-866`,
  `fx-src.txt:1774-1786`).

**xfx:**
- The list carries fx's request (`FilePicker::refresh_requested`,
  file_picker.rs:567). The open path asks for it on a new episode, on a
  kind change, or when the request is still untaken (shell.rs:9128-9138).
  Tab's retry asks too (:6227).
- `prepare_index` (shell.rs:16681) takes the request and walks only for an
  indexed query. It is called at the two lookup sites, the open path
  (:9139-9140) and `refresh_file_picker` (:6894), both only while eligible.
- `Files::begin_episode` (file_index.rs:1013) now only forgets the listing.
  `Files::refresh_index` (:1028) walks, and `Files::retry` (:1254) no
  longer walks by itself.
- `FilePicker::indexed` (file_picker.rs:621) counts an undecodable query
  as indexed, so it says `Files unavailable`, as fx's does.
- `settle_files`' eligible branch goes back to round 3's dirty mark alone;
  the request is taken by `refresh_file_picker`.

**fx's first-episode skip** (`:857-861`, `fx-src.txt:1778-1782`). fx skips
the first episode's refresh while its startup index is loading, and later
queues a refresh asked for during a walk (`file_index.zig:494-523`,
`fx-src.txt:2089-2118`). xfx starts no index at launch and queues nothing:
`refresh_index` walks only when no walk is running, on every episode. So no
xfx refresh ever starts a second walk beside a running one, which is the
skip's effect.
- Pinned by `an_indexed_query_opens_with_one_walk_or_none_while_one_runs`
  (none beside a loading walk).
- No separate first-sight flag: one would change nothing xfx can show.
- Queueing is the index's own build behaviour. It is recorded in
  `docs/parity.md`.
- **Corrected in round 6:** "which is the skip's effect" overstated it. fx
  skips only the session's first episode while its startup index loads;
  after that it queues a refresh asked for during a walk, and runs it when
  the walk ends, failed or not. xfx dropped those, so it could keep stale
  rows, or `Files unavailable`, until another trigger. Round 6 copies fx's
  queue (`../round6/README.md` §1).

**Per keystroke, not per batch.** fx prepares after input batching. xfx
reconciles every key, so a typed `@src/` walks at its bare `@`, as it did
before; a pasted `@src/` does not. Recorded in `docs/parity.md`.

**Re-pointed:** round 4's transcript and skills cases now expect the
listing alone (`base + 1`) and no walk.

## 2. Rule confirmation in the predicate (opus r4 SHOULD 2)

**fx at the pin:** `isActive` is `request != null` (`approval_prompt.zig:61-63`,
`fx-src.txt:2169-2171`), and `syncRuleManagement` sets `request`
(`:119-146`, `fx-src.txt:2174-2201`). `file_picker_eligible` now also needs
`self.rule_confirm.is_none()` (shell.rs:6161).

**The `@` list cannot be open under it:**
- A confirmation opens only from `/permissions` (shell.rs:14241). That
  command is a submitted line, and `run_command` takes the draft and
  reconciles before it runs (:13791-13793), so no `@` word is left.
- While the confirmation is up, every key goes to it and what it does not
  bind is swallowed (:12196-12198).

So, as for the question and the approval, there is no case for it, and a
mutant of the term would survive.

## 3. Stop before each advance (gpt6 r4 SHOULD 1)

fx asks before the folder is opened, then for each entry before the advance,
after it, and before the entry's `stat`; a failed advance fails the listing
(`path_completion.zig:75`, `:88-96`, `fx-src.txt:1913`, `:1926-1934`).
`list_directory_until` (file_index.rs:459-505) does the same, without
`flatten()`.
- `a_listing_stops_between_entries_once_told_to` asks exactly 3n + 2 times
  over 2000 entries (round 4: 2001), and stops at question 302.
- A failed advance cannot be provoked on a real folder from a test, so that
  arm has no case.
- The rule opus found unstated is now stated at `abandon_listing`
  (file_index.rs:1235-1240), the superseded listing in `lookup`, and
  `Drop`: every place that sets the stop flag also lets go of the
  receiver, so a cancel's `Err` never reaches `listed`.

## 4. Enter under a mention menu (gpt6 r4 SHOULD 2; opus r4)

Round 4's report said Enter under a shown skills menu now goes to the
composer. **It does not.** fx asks the skills menu before the `@` list
(`app_input_runtime.zig:1392-1411`, `fx-src.txt:2204-2223`), and so does
xfx's loop (`offer_to_skills` at shell.rs:12247, then `offer_to_files` at
:12253).

`enter_under_a_mention_menu_goes_to_the_menu_while_it_shows_and_then_to_the_list`
uses real routing, with `demo` the one skill:
- `$` + paste `dem @src/`: the menu shows and takes Enter. The draft
  becomes `demo @src/`, the list closes having read nothing, and nothing is
  sent.
- `$` + paste `zz @src/`: nothing matches, so the menu is not shown and the
  list is eligible, listed and trusted. Enter closes the menu and goes on
  to the list (fx `:2005-2009`, `fx-src.txt:2233-2237`). The list mentions
  its marked folder: the draft becomes `$zz @src/ `, and nothing is sent.

The round-4 receipt carries this correction (`../round4/README.md` §1).

## 5. Docs (opus r4 SHOULDs 1 and 4)

- `offer_to_files`' summary is back above it (shell.rs:6170-6171).
- `../round3/README.md` §6 notes that round 4 reverted the kept title.

## 6. RED → GREEN (`red-unit.txt`)

On round 4's code, 5 cases FAILED:
- `a_folder_query_opens_with_its_listing_and_no_walk` (walks 1, expected 0);
- `a_folder_query_edited_into_an_indexed_one_asks_for_the_refresh`
  (walks 1, expected 2);
- round 4's transcript and skills cases, re-pointed (2, expected 1);
- `a_listing_stops_between_entries_once_told_to` (asked 2001, expected
  6002).

`an_indexed_query_opens_with_one_walk_or_none_while_one_runs` passes there,
so it pins rather than shows a RED.

Added after the fix, all noted in `red-unit.txt`:
- The undecodable query's lines: they fail with round 4's `indexed()` put
  back.
- `an_episode_opened_under_the_transcript_keeps_its_refresh_across_keys`
  and `tab_retries_a_failed_index_by_walking_it_and_a_failed_folder_without`
  pass on round 4's code, which walked at every episode, so they pin round
  5's rule. They are mutant targets.

## 7. Mutants (`mutants.txt`)

One `--no-local` clone at `ebdd34be`, under the suite lock: **63 run, 63
killed**, 10 BROKEN, all accounted for in `mutants.txt`'s header.
- Round 5's 12: walk regardless of the query; no refresh on the kind
  change, on an episode, or carried across keys; Tab's retry asks none; the
  open site ungated; `refresh_file_picker` prepares nothing; an
  undecodable query not indexed; each of the three cancel points; an empty
  mention menu keeping Enter. 12 killed.
- Rounds 4, 3, 2 and 1 re-run at this head: 10, 5, 8 and 28 killed. Round
  3's `ctrl-o-never-relists` applies again to the settle code this round
  restored, and is killed.
- Not mutated, and a mutant would survive: the predicate's question,
  approval, rule-confirm and four menu terms (the list cannot be open under
  them: `../round4/README.md` §1, and §2 here), and the failed-advance arm
  (§3).

## 8. Pairs and sweep: not re-run

Round 5 starts a walk at a different keystroke only when an episode's first
query is a path (pasted, or the caret moved into one), when a query turns
from a path back into an indexed one, or on a Tab retry of an unreadable
folder.
- In the ten `@` scenarios of the pairs and the sweep, every `@` word is
  typed (fxcap's `type` is `tmux send-keys -l`, capture.sh:884), so each
  episode opens on a bare `@`, indexed, and walks at that key in both
  rounds.
- None has a `paste` line or deletes back over a `/`. C-u ends the word,
  so the next `@` is a new episode.
- None shows `Directory unavailable` and presses Tab.

So every walk in these scenarios starts at the same keystroke in rounds 4
and 5, and no snap is taken under a different walk state. The cancel
points change only an abandoned listing (no scenario has a `@` word under
C-o), and the rule-confirm term changes nothing a scenario can reach. So
round 1's pairs (`../pairs.txt`) and sweep (`../sweep-compare.txt`) stand.

## 9. The gate (`gate.txt`)

Every CI step (`../gates.sh`) ran in a `--no-local` clone at `ebdd34be`, in
one hold of the suite lock, with `TMPDIR=<scratch>/atfg5`. **20 steps, 0
failed**, and the tree was clean before and after.
- fmt, and both clippies.
- `cargo test`: 5502 passed, 0 failed. With fault injection: 5539 passed, 0
  failed.
- The checks and their self-tests; the fxcap self-tests (sweep 70 + 27,
  capture 68).
- Both release builds; smoke: 47 checks, 0 failures.
- smoke-tui: 37 scenarios and the oracle, 1168 checks, 0 failures.

## 10. Refs and paths

`refs.txt`: 16 checked, 0 wrong. `git diff af2b6137..HEAD` adds no local
path (the brief's grep prints 0 at every commit of this round).
