---
title: 0.0.16 delta -- P5 train 7 receipt (LAUNCH-SETTLE-F, T-U4-F, MODELS-REASK, STOP-NEXT-2 into one branch)
type: receipt
tags: [xfx, phase5, fx-0.0.16, train]
---

# P5 train 7: four closed units, one branch

Branch `train/p5-1108d`, from train 6's head `fdc5ac0b` (PR #82,
`train/p5-1108c`, which holds `main` `690aa505`), as the brief set it. Each
unit was closed by its own two-engine review and dispatcher gate; this train
combines them. The program goal (the user's words): «최신 fx랑 비교해서 ui쪽으로
모든 기능 구현하고 기본적으로 최신 fx ui를 그대로 구현해줘» (fx v0.0.16, pin
`481104fd`).

Oracle: fx v0.0.16 (`481104fd`), executable sha256
`8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked
by `sweep.py` before and after every run, `FX_AUTO_UPGRADE=0`.

**At a glance.**
- **Merges:** four `--no-ff` merges, in the brief's order. Merge 1
  (LAUNCH-SETTLE-F) is git's own (empty remerge-diff). Merge 2 (T-U4-F): one
  hunk in `src/tui/toolgroup.rs`, the end of `mod tests` -- not the ASCII fast
  path the brief expected: ARGROW-CACHE's fast paths are in `wrap.rs`, which
  `inline` reaches through `wrap::width`, and neither side changed `unit_at`
  or `inline` against the other. Merge 3 (MODELS-REASK): `shell.rs`'s
  appended tests, five ledger reference numbers, and **a build conflict**
  (train 6's FLAKE-CATALOG-EVENT test against MODELS-REASK's `telling` and
  `CatalogLoaded { due }`), resolved in the merge. Merge 4 (STOP-NEXT-2):
  one `shell.rs` hunk, the OSC 7501 parity row (two reference numbers both
  sides re-pointed), one `capfile.py` reference, eleven ledger entries'
  numbers and `cancelled-marker` (deleted), and **a build conflict** (three
  of train 6's tests applying `TurnStarted` without its submission),
  resolved in the merge. Every reference both sides re-pointed is set to
  where its code is in the merged file.
- **A semantic conflict git did not see** (`06189ce8`): STOP-NEXT-2's new
  tests and text spell the cancelled marker U+25A0, which T-U4 (train 6) had
  changed to fx's U+25AA; at merge 4 eight of its nine marker tests fail
  [measured, `glyph-red.txt`]. Changed as T-U4 changed `main`'s.
- **Token check:** all four merges OK, with train 7's numbers-only rules;
  negative control **59 of 59**; train 6's five merges unchanged under the
  new script. **References:** 9 moved by the merges, re-pointed; with the 11
  the merges set, **20 of 20 the same code**; 0 moved at the head; train 6's
  eleven stale ones left for REF-LINT.
- **Join check 1 (MODELS-REASK x STOP-NEXT-2):** a catalog answer read in a
  stop's window reaches the band (row shown, `catalog_nudged` cleared) and
  leaves the stop's reports, titles, `Cancelled` line and late text exactly
  as without it; a stop inside the 2 s catalog wait (turn start and image
  route) ends the turn at once, the next start keeps submission 2. Neither
  unit alone builds the tests; 2 of 2 mutants killed.
- **Join check 2 (T-U4-F x ARGROW-CACHE):** both units' tests pass; **8 of
  8 mutants killed** on the merged `unit_at`, `inline` and `wrap.rs`'s fast
  paths, one by the 900 s timeout (T-U4-F's own hang, recorded KILLED (hang)).
- **Join check 3 (LAUNCH-SETTLE):** `test-capture` five times, **5 of 5 OK**
  (68 tests), and the gate's sixth; `test_capture.py:394`'s and FLAKE-MCPMODE's
  tests passed every time.
- **Join check 4 (OSC wires):** every osc scenario, **47 pairs, 0 failing
  claims**; `stopnext` x3, 0 failing.
- **Join check 5 (network):** lib 4529 and tui 181 tests through a loopback
  logger: 2 CONNECTs, both `w.example`; **none to the Gateway**.
- **Join check 6 (render cost):** head ÷ `fdc5ac0b` 0.93 to 1.18 on every
  step of train 6's probe; **none past 1.2×**.
- **Gate** at `29786ced`, one hold: **20 of 20 steps** (`cargo test` 5446,
  fault injection 5483, each +80 on train 6's at the same code; smoke-tui
  1168 checks; `test-capture` 68 OK); COMPACT-NOTICE-LATE's test passed.
- **Sweep**, 110 scenarios: 293 of 476 pass, 0 failed, 0 timeout. Against
  `fdc5ac0b` **280 → 287 over 467 (7 up -- MODELS-REASK's -- 0 down)**;
  against MODELS-REASK's, STOP-NEXT-2's and LAUNCH-SETTLE-F's own sweeps 0
  down. A load spike from another worker during the head sweep; every moved
  pair swept again after it, 0 verdicts moved.

**Head:** the branch's tip, this receipt's last commit (`train/p5-1108d`;
no push, no PR); its code is `c468f5e1`'s. Train 6 is now
`main` `978a28fa` (its tree is `fdc5ac0b`'s); `main` has moved on to
`f096c163` (#83, `.github/preview-epoch`). The dispatcher merges `main` in.

## Units and merges

Each is `git merge --no-ff`, in the brief's order; each message names the
unit and its head.

| # | unit | branch @ tip | merge | base | conflicts |
|---|---|---|---|---|---|
| 1 | LAUNCH-SETTLE-F | `fix/p5-launch-settle-f` @ `2f1c80e7` (stacked on LAUNCH-SETTLE @ `40fdbc85`) | `04ef61b0` | `7589ec42` | none |
| 2 | T-U4-F | `fix/p5-t-u4-f` @ `a8c7a87d` (stacked on T-U4 @ `5a7bb465`) | `1661d59d` | `5a7bb465` | `src/tui/toolgroup.rs`, one hunk (both appended a test) |
| 3 | MODELS-REASK | `feat/p5-models-reask` @ `66854557` | `46078553` | `690aa505` | `src/tui/shell.rs`, one hunk (both appended tests); the ledger, five reference numbers; **a build conflict** in `src/tui/worker.rs` |
| 4 | STOP-NEXT-2 | `feat/p5-stop-next-2` @ `f8d625c3` (holds STOP-NEXT `d2459a37`, STOP-LATE-EVENT `af4fbacf`) | `54e3196b` | `690aa505` | `src/tui/shell.rs`, one hunk; `docs/parity.md`, one row (two reference numbers); `scripts/fxcap/capfile.py`, one reference; the ledger, eleven entries' reference numbers and one deleted entry; **a build conflict** in `src/tui/shell.rs`'s tests |

Every base is git's merge base. LAUNCH-SETTLE-F's is `7589ec42`, a commit of
train 4's branch that `fdc5ac0b` holds; T-U4-F's is T-U4's tip `5a7bb465`,
which train 6 merged; MODELS-REASK's and STOP-NEXT-2's are `main` `690aa505`
(train 5), which train 6 is built on.

After the merges, in order:

| commit | what |
|---|---|
| `06189ce8` | T-U4's cancelled marker in the text STOP-NEXT-2 wrote (a semantic conflict git did not see; below) |
| `85b13f84` | the join tests (join check 1); test only |
| `c468f5e1` | the `path:line` references the merges moved, re-pointed (below) |
| `0160715c`, `29786ced` | this receipt's scripts and the merge checks' outputs; the gate, the sweeps and the join runs ran at `29786ced` |
| the next ones | this receipt |

**The commit messages.** This train's eight commits up to `0160715c` were
first written without the `Co-Authored-By` trailer the repository's
attribution asks for; `git filter-branch --msg-filter` over
`fdc5ac0b..HEAD` added it to those eight and rewrote nothing else: every tree
is the same, and the units' 106 commits -- each of which carries the trailer
already -- and the merges' second parents are the units' own tips (`git
log --merges --format='%h %p'`: `2f1c80e7`, `a8c7a87d`, `66854557`,
`f8d625c3`). One run below was taken before it, at the old id of the same
tree, and says so (`glyph-red.txt`); every other output here was written at
the new ids.

`git show --remerge-diff` of each merge is `remerge-1.txt` .. `remerge-4.txt`.
Merge 1's has an empty body: git's own result, unchanged.

### Where the brief expected overlap, and where it was

`overlap.txt` (train 4's `overlap.py`): every file both sides of a merge
changed since their base, and the nearest pair of hunks.

- **Merge 1: four files, no conflict.** `capfile.py` (nearest 5 unchanged
  lines: the `warning_shown` comment's re-point that `fdc5ac0b` holds
  against LAUNCH-SETTLE-F's `TIMERS` launch `settle` taken out),
  the ledger (28), `sweep.py` (52), `test_sweep.py` (7). Git's own result.
- **Merge 2: `src/tui/toolgroup.rs`, as the brief expected -- but not where
  it expected.** The brief expected T-U4-F's cut (`unit_at`, `inline`) to
  meet ARGROW-CACHE's exact ASCII fast path (`first_cluster`). Train 6 did
  not change `unit_at` or `inline` (`git log 5a7bb465..fdc5ac0b --
  src/tui/toolgroup.rs`: only train 6's join test, `03476db7` and
  `392ca22a`), and `unit_at` segments with `graphemes(true)` itself, not
  through `first_cluster`. The one conflict is the end of `mod tests`, where
  both sides appended a test (below). ARGROW-CACHE's fast paths are in
  `src/tui/wrap.rs` (`painting` walks clusters with `first_cluster`;
  `cluster_cells` counts a one-byte printable cluster at once), which
  `inline` reaches through `wrap::width` for each unit; neither side of the
  merge changed `wrap.rs`. Join check 2 puts its mutants on that path.
- **Merge 3: seven files.** `docs/parity.md` (2 unchanged lines apart:
  T-U4's `/resume` row, which train 6 holds, and MODELS-REASK's `/model`
  row; merged textually),
  the ledger (five entries, the conflicts), `shell.rs` (the conflict),
  `bridge.rs` (3), `event_loop.rs` (287), `question.rs` (414), `worker.rs`
  (507).
- **Merge 4: eight files.** The conflicts: `docs/parity.md`, `capfile.py`,
  the ledger, `shell.rs` (below). With MODELS-REASK, as the brief expected,
  in `worker.rs`, `event_loop.rs` and `bridge.rs`, merged textually:
  `worker.rs` 1 unchanged line apart, in `image_refusal`: MODELS-REASK's
  image-route wait (`await_failed_catalog_retry`, `worker.rs:5777` at the
  head) inserted right above STOP-LATE-EVENT's stop word (`if
  turn.token.is_cancelled()` .. `return Some(bridge::TURN_CANCELLED…)`,
  `:5779-5783`). The merged order is the
  wait, then the check, so a stop inside the wait answers with the stop's
  word: join check 1 (b)'s image-route case, whose first turn ends `the turn
  was cancelled`. `event_loop.rs` 23 lines apart, `bridge.rs` 34; where they
  meet at run time is join check 1.

Merge 1 changes no Rust. After merge 2 the tree built (`cargo check
--locked --all-targets`); after merges 3 and 4, with and without
`fault-injection`, once each merge's build conflict was resolved in it;
`cargo fmt --check` passed after merges 2, 3 and 4.

### Merge 2's conflict: `src/tui/toolgroup.rs`

The end of `mod tests`: train 6's join test
`the_transcripts_row_keeps_what_a_terminal_draws_wider_whole_at_every_width`
and T-U4-F's `a_unit_ends_before_a_spelling_whatever_joins_its_backslash`.
Resolved: train 6's, then T-U4-F's. The merged file against train 6's is
T-U4-F's diff against its base, line for line (`git diff fdc5ac0b` and `git
diff 5a7bb465 a8c7a87d`, their `+`/`-` lines the same). The ledger merged
textually (T-U4-F's four `call-row-*` entries: its re-points of T-U4's
`toolgroup.rs` references, and `call-row-raw-argument`'s text).

### Merge 3's conflicts and its build conflict

- `src/tui/shell.rs`, the end of `mod tests`: train 6's appended tests (T-U4's,
  CTRLO-WIDER's, ARGROW-CACHE's and the train's joins), then MODELS-REASK's
  section. Every `+`/`-` line either side wrote since `690aa505` is in the
  result (the sorted line multisets of each side's diff and of the merge's
  diff against the other side are the same).
- The ledger, five entries (`session-record`, `transcript-page-offset`,
  `toolgroup-ctrlo-record-short`, `safe-page-edge`,
  `full-transcript-shift`), reference numbers alone: train 6's
  `src/tui/shell.rs:37764` and MODELS-REASK's `:38002` for `main`'s
  `:37758` (the session record's tools line, a test's expected rows), set
  to `:38014`, where that line is in the merged file (`repointed.py`'s
  merge-3 row). MODELS-REASK's own `:38002` named a comment six lines above
  the line on its branch (`:38008`); merge 4 then moved it again (below).
- **A build conflict**, resolved in the merge so it compiles. Train 6's
  FLAKE-CATALOG-EVENT test
  `a_title_runtime_answers_a_forced_catalog_read_from_its_stand_in`
  (`src/tui/worker.rs`) called `telling(provider, events)` and compared a
  `CatalogLoaded` with two fields; MODELS-REASK gave `telling` the held
  catalog and the clock, and `CatalogLoaded` its `due`. The test now hands
  `telling` the selector's catalog and clock, as `CatalogAsks::asked_at`
  does, and asserts the stand-in's answer -- the Gateway, no rows -- with
  `due` left out of the comparison (`remerge-3.txt` shows it). Its sibling
  `the_fixtures_catalog_has_answered…` compiled unchanged, but its mechanism
  moved: under MODELS-REASK's rule `begin_catalog_read` starts only a
  first load, and the fixture pins the catalog as loaded, so the test now
  passes with no read begun rather than with one answered from the stand-in.
  It still guards the pin: without it the selector is `NotLoaded` with a
  fetcher, so `wants_first_load` holds and a first load starts and tells the
  band [read from the code: `src/provider/model.rs:1892-1896`,
  `src/tui/worker.rs:4141`; not run without the pin].

### Merge 4's conflicts, its build conflict, and the marker

- `src/tui/shell.rs`, the turn's end under a stop: STOP-NEXT took
  `cancel_covers` out and decides on `quiet`; T-U4 changed the comment's
  marker. Resolved: STOP-NEXT-2's code, T-U4's `▪` in the comment. No
  `cancel_covers` is left in the file.
- `docs/parity.md`, the `terminal program status (OSC 7501)` row: merged
  word by word against `690aa505` (`rowmerge.py`, train 6's, with a `--sub`
  added). The train's only change to the row was MODELS-REASK's re-point of
  two `worker.rs` references (`:3835`, `:3524` to `:3856`, `:3544`), which
  STOP-NEXT-2 re-pointed too (to `:3846`, `:3530`); a word-level conflict.
  Both set to where that code is in the merged file (`:3867`, the end sent;
  `:3550`, the place given back), every edit of the train's side is then
  one STOP-NEXT-2's side made too, and the row is STOP-NEXT-2's with those
  two numbers (checked: the merged row equals it).
- `scripts/fxcap/capfile.py`, `TIMERS`' `warning_shown` comment: both sides
  re-pointed `main`'s `shell.rs:14033-14058` (train 6 `:14039-14064`,
  STOP-NEXT-2 `:14517-14542`); set to the merged `:14603-14628`, the full
  transcript's lines at `:14621-14623` (`refconflict.py`; the 26 lines are
  `main`'s, `repointed.py`).
- The ledger, eleven entries' reference numbers alone, each set to where its
  code is in the merged file (`refconflict.py`, which refuses a block whose
  sides differ in anything but `:N`): `toolgroup.rs`'s `spelled` (`:922`),
  `command_spelled` (`:999`) and its U+061C arm (`:1113`) for
  `call-row-raw-argument`, `call-row-page-edge`, `call-row-source-cut`,
  `call-row-arabic-letter-mark`; `shell.rs`'s session record (`:38615`)
  for the five of merge 3, `fn send` (`:13112`) for
  `submit-multi-card-whole`, the stats line (`:1756`) for `stats-bytes`.
  And `cancelled-marker`: T-U4 deleted it (train 6), STOP-NEXT-2 changed only
  its reference (`toolgroup.rs:710` to `:718`); gone.
- **A build conflict**, resolved in the merge so it compiles: three of train
  6's tests (CTRLO-WIDER's
  `a_prompts_characters_a_terminal_draws_wider_are_named_on_ctrl_o`, the
  train's Ctrl-O join fixture `a_turn_that_read_a_wider_path`, ARGROW-CACHE's
  `a_turn_with_arguments`) applied `UiEvent::TurnStarted` as a unit variant;
  STOP-NEXT gave it its submission. Each now calls `start_turn()`, as STOP-NEXT
  converted `main`'s tests (`git diff 690aa505 d2459a37 -- src/tui/shell.rs`:
  41 `apply(UiEvent::TurnStarted…)` lines out, 43 `start_turn()` calls in).
- **The marker (`06189ce8`), a semantic conflict git did not see.** T-U4
  (train 6) changed xfx's cancelled marker from U+25A0 to fx's U+25AA in
  every line that writes, tests or names it, keeping four (its `sites.txt`).
  STOP-NEXT-2 branched from `690aa505`, before T-U4, and wrote sixteen new
  lines with U+25A0: ten lines of nine tests that count or find the turn's
  `Cancelled` line, five comments, and two clauses of the OSC 7501 parity row that say
  which line xfx writes. **At merge 4, eight of STOP-NEXT-2's nine such tests
  fail** -- each counts 0 of a line it expects once, or finds none -- and the
  ninth (`a_cancel_that_waited_for_a_call_leaves_a_held_call_still_running_its_own_line`,
  which asserts the line is absent) passes vacuously (`glyph-red.txt`: a
  clone at merge 4, the nine by name) [measured]. `06189ce8` changes each of
  the sixteen as T-U4 changed `main`'s; after it `git grep -i -E '■|25a0'`
  over `src`, `tests`, `scripts`, `docs` and `.github` finds only T-U4's four
  kept lines [measured]. A commit of its own, after the merge: the merge
  compiles, and the token check is about the merges; the row's two tokens
  are the only `docs/parity.md` change after merge 4.

## The token check (`token_check.py`, `token-check.txt`, `token-self-test.txt`)

Train 6's `token_check.py` (its clause, order and unchanged-row rules, and
train 4's numbers-only rule for a ledger entry), with two rules added for
what train 7 met: **one reference both sides re-pointed to their own
branch's line.**

- **`docs/parity.md`.** A token of a row that both sides replaced one for one
  at the same place in the base's row, the base's token and each side's
  equal once every `:N` (a line number or range) is masked -- one reference
  both sides re-pointed -- is compared with its numbers masked, in the base,
  both sides and the merge (the merge's token is the one at theirs' place,
  lined up with difflib). A side whose only edit to the row was such a
  re-point so did not change the row, and every other rule applies as for a
  row the other side alone changed: the merge's row must be that side's,
  token for token, but the masked tokens. The number the merge holds is
  `repointed.py`'s to check against the code.
- **The ledger.** An entry one side deleted and the other changed only in
  `path:line` numbers is gone; an entry one side changed and the other only
  in numbers is the first side's, its numbers aside (checked after train 4's
  rule for an entry both sides changed in numbers alone).

**Result: all four merges OK** [measured].

- Merge 1 (LAUNCH-SETTLE-F; its base `7589ec42` is older than train 6's
  rows, so train 6's rows read as changed by ours): every parity row ours',
  token for token; the ledger: LAUNCH-SETTLE-F's three added causes
  (`esc-clear-armed-through-an-edit`, `esc-clear-hint-over-empty-composer`,
  `mcp-tools-at-a-reconnect`) in, every other entry ours'.
- Merge 2 (T-U4-F): no parity row of T-U4-F's; its four ledger changes
  (`call-row-page-edge`, `call-row-arabic-letter-mark`,
  `call-row-raw-argument`, `call-row-source-cut`: its re-points of T-U4's
  `toolgroup.rs` references, and `call-row-raw-argument`'s text) as it has
  them.
- Merge 3 (MODELS-REASK): its `/model` row (657 tokens added, 36 deleted)
  and its re-point of the OSC 7501 row (2 and 2) as it has them; the ledger:
  its `catalog-window` change, `activity-tokens-before-request` added, its
  re-points in `stats-bytes`, `submit-multi-card-whole` and the `banner`
  normalization, and the five entries both sides re-pointed (train 4's
  numbers-only rule).
- Merge 4 (STOP-NEXT-2): the OSC 7501 row is STOP-NEXT-2's (1223 tokens
  added, 14 deleted), token for token and in its order, its two `worker.rs`
  references both sides re-pointed compared masked (the new rule; M holds
  `:3867` and `:3550`); the ledger: `cancelled-marker` gone (deleted by
  ours, re-pointed alone by theirs: the new rule), `call-row-raw-argument`
  ours' (T-U4's text) with theirs' re-point aside (the new rule), the
  ten others both sides re-pointed in numbers alone (train 4's rule).

No comment line of the ledger was lost on any merge.

**The negative control** (`--self-test`, on each merge's own texts): train
5's and 6's cases as they apply, and train 7's: for each parity reference
both sides re-pointed, the merge's token given another path, and dropped;
for each ledger entry one side deleted and the other re-pointed, the entry
put back as the side that kept it has it; for each entry one side changed
and the other re-pointed, a word of its `why` (or `reason`) changed. The
cases are chosen and planted on the texts with each such reference masked,
as the check reads them. **Merge 1: 14 of 14 found. Merge 2: 11 of 11.
Merge 3: 14 of 14. Merge 4: 20 of 20** (among them the four parity
re-point cases, `cancelled-marker` put back, `call-row-raw-argument` with a
word changed). In all, 59 of 59 [measured].

**Regression:** the same script over train 6's five merges prints train 6's
`token-check.txt` line for line (its comment and blank lines aside), and its
negative control finds 8, 17, 20, 14 and 5, as train 6's did [measured].

The re-point commit `c468f5e1` and the marker commit `06189ce8` change the
ledger and `docs/parity.md` after the merges; the token check is about the
merges, and `repointed.py` and the marker's `git grep` check those
(below, and above).

## The references (`c468f5e1`; `refs.txt`, `refs-head.txt`, `repointed.txt`, `stale.txt`)

Train 6's `refs.py` (the ledger and `docs/parity.md`), `refs_all.py` (every
unpinned `path:line` in every tracked file outside `.prd/`) and
`refs_short.py` (`name.rs:N`), five sides: train 6 `fdc5ac0b`,
LAUNCH-SETTLE-F `2f1c80e7`, T-U4-F `a8c7a87d`, MODELS-REASK `66854557`,
STOP-NEXT-2 `f8d625c3`. At the join tests' commit `85b13f84`, before the
re-points (`refs.txt`, `refs-all-merges.txt`, `refs-short-merges.txt`): 82
references, 17 (side, reference) pairs `MOVED` -- eight references -- and,
in the widened scans, one more (STOP-NEXT-2's comment in `shell.rs`).

`c468f5e1` re-points those nine, each to the code it named on the side whose
text cites it. `repointed.py` checks each against that side's copy of the
cited file, the line and two either side -- and, where a unit rewrote the
cited line, that both hold the call the reference names -- and with them
every number the merges themselves set in a conflict: **20 of 20 the same
code** (`repointed.txt`) [measured].

| who cites it | cited on its side | now | moved by |
|---|---|---|---|
| ledger, `capture.sh`'s TMPDIR comment (normalization) | `scripts/fxcap/capture.sh:291-293` (train 6) | `:315-317` | LAUNCH-SETTLE-F (+24 above) |
| ledger, the run directory (normalization) | `capture.sh:462` | `:521` | LAUNCH-SETTLE-F (+59) |
| ledger, `remain-on-exit` (normalization) | `capture.sh:878` | `:952` | LAUNCH-SETTLE-F (+74) |
| ledger `activity-tokens-before-request` (why: `tally.begin`) | `src/tui/shell.rs:4131` (MODELS-REASK) | `:4333` | STOP-NEXT, which rewrote the call (`input` became `estimate`): the call checked, not the lines around it |
| ledger `amendment-card-after-stop` (xfx, why) | `shell.rs:9762` (train 6: `fn trailer_rows`) | `:10194` | MODELS-REASK and STOP-NEXT-2 |
| ledger `answer-drawn-wider-named` (source) | `shell.rs:12859` (train 6: `shown_event` in `present`) | `:13423` | the same |
| ledger `resize-document-repaint` (source) | `shell.rs:15607` (train 6: `fn resize`) | `:16321` | the same |
| ledger `cancel-page-edge` (xfx) | `shell.rs:37764` (train 6) | `:38615` | the same |
| `shell.rs` comment, the end sent | `worker.rs:3846` (STOP-NEXT-2) | `:3867` | MODELS-REASK |

The numbers the merges set (`repointed.py`'s `merge 3:` and `merge 4:`
rows): the five session-record references at `:38014` at merge 3 and
`:38615` at the head; `fn send` `:13112`; the stats line `:1756`;
`toolgroup.rs`'s `spelled` `:922` (from T-U4-F's own `:914`: T-U4-F
re-pointed T-U4's `:889` on its branch), `command_spelled` `:999`, the
U+061C arm `:1113`; the OSC 7501 row's `worker.rs:3867` and `:3550`;
`capfile.py`'s `:14603-14628` and `:14621-14623`. LAUNCH-SETTLE-F's own
references into `capture.sh` were stale on its own branch: it moved the code
and not the references.

At the head (`refs-head.txt`): 81 references, **0 moved**. `refs-all.txt`
holds the widened scans at the head; what they still flag is not moved by
this train (`stale.py`, `stale.txt`): train 6's eleven, each naming other
code on `main` already, one of them as STOP-NEXT-2 re-pointed it --
`event_loop.rs`'s `event_loop.rs:959-962` is `:972-975` now, by line
mapping in STOP-NEXT-2's round 2, and names the same comment lines it did
on `main` (the frame-after-a-refused-append claim is still not at them);
and the historical plans under `docs/superpowers/plans/`. Left for REF-LINT,
as the dispatcher routed them.

## Join checks

### 1. MODELS-REASK x STOP-NEXT-2 (`85b13f84`; `join-units.txt`, `join-mutants.txt`)

**What each unit does on the path** [read from the code at the head]. A stop
is `TurnControl::Cancel { through }` on the control channel; the runtime's
race reads the channel first (`raced_against_control`, `biased`,
`worker.rs:5642`), raises the turn's token at once (`turn.cancel()`), and
gives the work back after the body ends. MODELS-REASK's asks are
`TurnControl::Catalog` on the same channel -- no work item, no submission
number -- and a read asked for mid-work starts beside the work
(`CatalogAsks::asked_at`), tells its own answer from its own thread through
`bridge::send_terminal` (`worker.rs:4176`; no turn's token), and is handed
to the selector after the work, stopped or not (`asks.finish`,
`worker.rs:3841`). Its waits for a failed catalog's retry are raced against
the turn's own token (`await_failed_catalog_retry`, `worker.rs:4464-4473`;
the tool images' gate at `:5861-5865`). The turn's start is sent before any
of them (`TurnStarted { submission: queue.taken }`, `worker.rs:3813-3816`,
ahead of the body). On the band, STOP-NEXT-2's `late` holds three kinds
(`Late`, `shell.rs:2165-2174`: text, operational lines, tool rows); the
catalog's events have arms of their own that no stop gates
(`shell.rs:4600-4682`).

**The tests, at the head** (each passes in both `cargo test` steps of the
gate):

- (a) `a_catalog_answer_read_in_a_stops_window_reaches_the_band_and_leaves_the_stop_as_it_was`
  (`src/tui/event_loop.rs:15649`): a catalog failed past its hold; `go`
  submitted (its activity ask goes out -- pending, the answer not read), the
  turn started; **Ctrl-C**; `/model` while the stopped turn unwinds
  (`Loading models…`, asked again). Then the answer -- the catalog's rows,
  `vendor/joined` -- read with late text (`LATE-ANSWER`) in four ways: with
  the stop's own end in one drain; with a finished end; in a drain that
  empties before the end (STOP-NEXT-2 drops the late text there); and in the
  keystroke's own tick. Each run is compared with the same run without the
  answer: **the reports and titles on the wire, in order, the `Cancelled`
  lines, the late text and the one stop are the same**, and `idle` <
  `Cancelled` < the title (`]2;go`) holds; without the answer the menu says
  `Loading models…` and a prompt after it asks nothing (`catalog_nudged`
  still set); **with it the menu lists the row** -- before the end where its
  drain came first -- and the prompt after it asks again (`catalog_nudged`
  cleared). In the keystroke's own tick no frame comes between `/model` and
  the answer, so no frame says `Loading models…` there. The answer is never
  held in or dropped with `late`.
- (b) `a_stop_in_a_turns_catalog_wait_ends_it_at_once_and_the_next_start_keeps_its_number`
  (`src/tui/worker.rs:17447`): two prompts through the real `turn_loop`
  (submissions 1 and 2), a failed catalog whose retry does not land, and
  `Cancel { through: 1 }` 300 ms into the first turn's wait -- at **the
  turn's start** (a model with no window without the catalog) and at **the
  image route** (an image in the history, a model whose window is known).
  The first turn ends as the stop's own end (`the turn was cancelled`)
  between 300 ms and 1 s, the second starts as submission 2 and runs; without
  the stop the first ends past the wait's 1.9 s (at the start, answered; at
  the image route, MODELS-REASK's `CAPABILITY_UNKNOWN` refusal).
- (b), the band's half, `catalog_asks_around_a_stop_leave_its_count_and_the_next_start_its_own`
  (`src/tui/shell.rs:48569`): one schedule -- `go`, Ctrl-C, `/model` while the
  stopped turn unwinds (closed with Escape), `next`, the stop's end, the
  runtime's `TurnStarted { submission: 2 }`, its answer and finish -- with
  and without a failed catalog behind it. The asks go out as control
  messages (with the failure: `go`'s activity ask, then `Cancel { through: 1
  }`, then `/model`'s), the stop quotes 1 either way, `next` is submission
  2, and the count, the reports (`working`, `idle`, `working` once the
  stopped turn's end lets its stop go, `done`), `next`'s card above its
  answer and the one `Cancelled` line are the same with the asks and without
  them.

**On each unit alone** (`join_units.sh`: each unit's tip, a `git clone
--no-local` with the three tests appended unchanged, `cargo test --no-run`):
neither builds, and the compiler names why -- each test needs both units.

| unit alone | build | what is missing (first errors) |
|---|---|---|
| MODELS-REASK `66854557` | fails, 10 errors | STOP-NEXT's: `TurnStarted` has no `submission`, no `start_turn`, no `bridge::TURN_CANCELLED`, no `Fixture::handle` |
| STOP-NEXT-2 `f8d625c3` | fails, 17 errors | MODELS-REASK's: no `CatalogAsk`, `CatalogUnread` has no `retry_at`, `CatalogLoaded` no `due`, no `FailsThenGated`, `retry_behind_failure`, `backdate_catalog`, `one_turn_with_images` |

**Do they discriminate?** `join_mutants.py` on a `git clone --no-local` of
`29786ced`, one mutant at a time (exactly one textual change), the lib tests
built, the three join tests run with a 900 s timeout, the file restored, the
clone clean at the end, in one hold of the suite lock (`jobs-b.sh`;
`join-mutants.txt`). The unmutated clone passes the three.

| mutant, planted on the train | (a) | (b) | (b), the band | said |
|---|---|---|---|---|
| J1 the band drops a catalog answer read while a stop is requested (`if self.cancelling { return; }` in the `CatalogLoaded` arm) | **FAILED** | ok | ok | the catalog row never shown, activity never asks again |
| J2 a turn's wait for a failed catalog's retry not raced against its stop (`await_failed_catalog_retry` without the token's arm) | ok | **FAILED** | ok | the first turn ends past the wait, not between 300 ms and 1 s |

**2 of 2 killed** [measured]. The band's half guards the count (no mutant
here moves it: MODELS-REASK's asks would have to take a work item to); its
check is the differential itself, with and without the asks.

### 2. T-U4-F x ARGROW-CACHE: the merged `unit_at` and `inline` (`join-mutants.txt`)

At the head T-U4-F's tests (`a_unit_ends_before_a_spelling_whatever_joins_its_backslash`,
and its Prepend cases in `the_transcripts_row_cuts_a_spelled_argument_between_its_spellings`),
train 6's row-cut join test
(`the_transcripts_row_keeps_what_a_terminal_draws_wider_whole_at_every_width`)
and ARGROW-CACHE's wrap property tests (`painting_reads_the_clusters_graphemes_reads`,
`a_printable_ascii_character_is_one_cell`,
`a_run_stepped_over_is_the_clusters_graphemes_reads_there`) pass in both
`cargo test` steps of the gate [measured].

**Mutants on the merged function** (`join_mutants.py`, group T, the same
clone and hold; `cargo test --locked --lib tui::`, T-U4's runner's filter,
2871 tests, the unmutated clone passing every one; a 900 s timeout per run,
the process group killed past it and the mutant recorded KILLED (hang)):

| mutant | result | killed by |
|---|---|---|
| T1 `unit_at`: a cluster keeps the spelling it took in (T-U4's rule, before T-U4-F) | **KILLED** | `a_unit_ends_before_a_spelling_whatever_joins_its_backslash`, `the_transcripts_row_cuts_a_spelled_argument_between_its_spellings` |
| T2 `unit_at`: a cluster ends before any `\` it took in, a spelling or not | **KILLED** | `a_unit_of_the_row_is_a_cluster_or_a_spelling_with_what_joins_its_last_character`, `a_unit_ends_before_…` |
| T3 `unit_at`: a cluster ends before its second character, whatever it is (the mutant T-U4-F's own run hung on) | **KILLED (hang)**: past 900 s, killed | none in time: on T-U4-F's diagnosis (its `t-u4-f/mutants.txt`) `unit_at` returns 0 and `inline` loops |
| T4 `unit_at`: a spelling's unit ends at its last character, a mark after it left out | **KILLED** | `a_unit_of_the_row_is_a_cluster_or_a_spelling_with_what_joins_its_last_character` |
| T5 `inline`: each unit measured raw, not as spelled | **KILLED** | `the_transcripts_row_shows_at_every_width_what_the_whole_row_does_and_ctrl_o_the_whole` |
| T6 `inline`: the bound one cell short (`>=`) | **KILLED** | the same |
| T7 ARGROW-CACHE's `first_cluster`: a printable ASCII byte its own cluster even before a mark | **KILLED** | `painting_reads_the_clusters_graphemes_reads`, `the_first_cluster_is_the_segmentations_first_cluster` and two of CTRLO-WIDER's naming tests |
| T8 ARGROW-CACHE's `cluster_cells`: any one-byte cluster below DEL, a control too, one cell | **KILLED** | `a_printable_ascii_character_is_one_cell`, `control_characters_cost_no_cells`, `ascii_fast_path_never_claims_a_control_escape_or_unicode_byte` and two tab tests |

**8 of 8 killed**, one of them by the timeout [measured]. So at the head
T-U4-F's Prepend rule and ARGROW-CACHE's fast paths each hold under their
own tests, and each mutant of one is caught without the other's tests
failing -- the two meanings sit side by side, as the merge left them (no
line of either changed in it).

### 3. LAUNCH-SETTLE: `test-capture` five times (`test-capture.txt`)

`scripts/fxcap/test-capture.sh` five times at `29786ced`, from a `git clone
--no-local`, one after another in one hold of the suite lock
(`jobs-a.sh`), load 2.5 to 42.0 [measured]:

| run | result | seconds |
|---|---|---|
| 1 | 68 tests, OK | 270 |
| 2 | 68 tests, OK | 273 |
| 3 | 68 tests, OK | 281 |
| 4 | 68 tests, OK | 291 |
| 5 | 68 tests, OK | 300 |

And the gate's own run (step 16): 68 tests, OK -- **six runs, no failure**.
The test the brief names at `test_capture.py:394` -- on `fdc5ac0b`
`test_a_pid_whose_executable_is_not_the_binary_is_refused_and_the_guards_still_run`
(`/bin/sh` against `/bin/cat`), at `:398` on the head -- passed in all six,
and so did FLAKE-MCPMODE's `test_hang_holds_until_the_mode_changes` (`:926`
on `fdc5ac0b`, `:1052` on the head): unittest names a test only when it fails
or errors, and each run says `OK`. Six passes do not prove a race is gone;
LAUNCH-SETTLE-F's own receipt carries the mechanism (the launch waits until
the pane runs the binary).

### 4. The OSC 7501 wires (`osc.txt`, `osc-head/`, `osc-head-run2/`, `osc-head-run3/`)

STOP-NEXT-2's checker (`../stop-next-2/osc.py`, unchanged; its claims
`build`, `intact`, `same`, `between`, `order`, `clear`, `last`, `inert`,
`activity`, `title`, `prompt`) over the head's wires against fx v0.0.16's at
the same pin, as STOP-NEXT-2 reads them (`stopnext`: STOP-LATE-EVENT's three
fx runs; every other osc scenario: L-U1+U2's). The head's wires: every
`p5-lifecycle-osc-*` scenario from the head sweep (`osc-head/`), and
`stopnext` captured twice more right after it, each a one-scenario sweep with
the head's harness and debug build (`osc-head-run2/`, `osc-head-run3/`), in
one hold of the suite lock (`jobs-c.sh`, load 6 to 9).

| wires | result |
|---|---|
| every osc scenario at `29786ce` (`osc-head/`) | **47 wire pairs, 0 failing claims**; the head's banner on every wire |
| `stopnext`, run 2 | 4 wire pairs, 0 failing claims |
| `stopnext`, run 3 | 4 wire pairs, 0 failing claims |

[measured; `osc.txt`]. So `stopnext` three times, 0 failing on each: `idle`,
the `Cancelled` frame, the title, then `working` and the prompt's frame, as
fx's (`s01-second`: `idle > working > idle > working` on both). 47 is
STOP-NEXT-2's own count at its head. The head sweep that took `osc-head/`
ran some of these scenarios under another worker's load (the Sweep, below);
the claims hold on every one.

### 5. Network hermeticity (`net.txt`, `connect.log`)

FLAKE-CATALOG-EVENT's method, as train 6 ran it (`net.py`: its
`connect_logger.py`, a loopback stand-in for a proxy that logs each
connection's first request line, answers 403 and forwards nothing, started
as the script's own child on an ephemeral port and ended by it; the four
proxy variables at it, `NO_PROXY`, `no_proxy`, `ALL_PROXY`, `all_proxy`
unset), on a `git clone --no-local` at `29786ced`, in one hold of the suite
lock (`jobs-a.sh`).

| run | result | CONNECTs logged during it |
|---|---|---|
| `cargo test --locked --lib --no-fail-fast` | 4529 passed, 0 failed, 7 ignored | **2**, both `CONNECT w.example:443` |
| `cargo test --locked --test tui --no-fail-fast` | 181 passed, 0 failed | **0** |

**No CONNECT to the Gateway host**, in either suite [measured]. The two
`w.example` lines are the known MCP probe (train 6's join check 4). MODELS-REASK
reads the catalog in many more tests now (its fixtures pin a stand-in
fetcher: `set_fetcher_for_test`), and none of them reached the Gateway.

### 6. Render cost (`probe/probe.txt`)

Train 6's probe and runner (`probe/run.sh`, `DETAILS=1 ROUNDS=2`: the
call's arguments sent, both cases -- `c1`, 4 Mi U+009B, and `wider`, 2 Mi
U+261D -- release, one case a process, the clones alternating case by case,
two rounds), on two `git clone --no-local`s with the probe appended
(`probe/prep.sh`): `fdc5ac0b` with train 6's `render_cost_probe.rs`, and the
head `29786ced` with `render_cost_probe-head.rs`, which is the same file but
for one line -- `live.start_turn()` where it applied
`UiEvent::TurnStarted.made_inert()`, a unit variant STOP-NEXT gave a
submission (the head would not compile it otherwise). One hold of the suite
lock (`jobs-b.sh`), load 7.9. ms, `r1 / r2`; peak RSS MiB; `probe/probe.txt`.

| case, with arguments | build | Ctrl-O open | join | theme | resize | peak |
|---|---|---|---|---|---|---|
| 4 Mi U+009B (`c1`) | `fdc5ac0b` | 74.5 / 78.6 | 31.4 / 35.1 | 33.6 / 33.6 | 60.6 / 57.2 | 415 |
| | **head** | **77.0 / 77.5** | **35.3 / 36.8** | **31.1 / 34.8** | **61.6 / 58.6** | 415 |
| 2 Mi U+261D (`wider`) | `fdc5ac0b` | 27.3 / 28.0 | 3.3 / 3.3 | 2.7 / 2.7 | 24.9 / 24.6 | 138 |
| | **head** | **29.0 / 28.0** | **3.4 / 3.2** | **2.8 / 2.7** | **24.5 / 24.9** | 138 |

**Head ÷ `fdc5ac0b`, step by step and round by round: 0.93 to 1.12 on Ctrl-O's
open, join, theme and resize; the call's start 0.94 to 1.18 (`c1` round 1,
3.3 against 3.9 ms); the arguments' crossing (`detail_ms`) 0.99 to 1.08;
peak 1.00** [derived from `probe/probe.txt`]. **No step slower than 1.2× --
no finding.** The numbers are train 6's round 3 at its head (`8ac4195b`:
`c1` join 33-34, theme 31-33, resize 55-59 ms; `wider` 3.3-3.5, 2.6-2.7,
23-25 ms), every Ctrl-O step under 0.1 s. None of this train's units touches
Ctrl-O's layout; T-U4-F's cut is the transcript's row, which the probe's
`start` and `rebuild` steps pay (`rebuild_worst` 0.4 and 2.7-2.8 ms on both).

## Gate (`gates.sh`, `gates.txt`)

`gates.sh` (trains 5's and 6's, its comment only changed): every step the
brief lists, at `29786ced` (the code of `c468f5e1`; the commits after it
change only this directory), one after another, **the whole script inside
one hold of the suite lock**, each step's exit taken from `$?` on the line
right after its command. Tree clean at the start. Load 1.7 to 54.9 (other
workers' jobs). **20 steps, 0 failed, none run again** [measured].

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| the same `--features fault-injection` | ok |
| `cargo test --locked --no-fail-fast` | **5446 passed**, 0 failed, 7 ignored (15 binaries; `tests/tui.rs` 181) |
| `cargo test --locked --features fault-injection --no-fail-fast` | **5483 passed**, 0 failed, 7 ignored (`tests/tui.rs` 195) |
| `scripts/check-xfx-identity.sh` / `check-no-stubs.sh` / `check-no-secrets.sh` | ok / ok / ok (15255 tracked files) |
| `scripts/check-preview-contract.sh` / `check-tui-preflight-cost.sh` / `check-usage-clock.sh` | ok / ok (200x300 replay 1.43 ms mean; budget 32 ms) / ok |
| `scripts/test-check-no-secrets.sh` / `test-gate-env.sh` / `test-preview-contract.sh` | ok (13 cases; 3473 tracked names past `ARG_MAX`) / ok (62 cases) / ok (24 mutants caught) |
| `scripts/fxcap/test-sweep.sh` | 70 + 27 tests, OK |
| `scripts/fxcap/test-capture.sh` | **68 tests, OK** |
| `cargo build --locked --release`; `--release --features fault-injection --target-dir target/faulty` | ok; ok |
| `scripts/smoke.sh target/release/xfx <dir>` | 47 checks, 0 failures |
| `scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx <dir>` | 37 scenarios and the oracle, **1168 checks**, 0 failures |

`tests/tui.rs`'s `ctrl_c_stops_a_compaction_that_is_waiting_for_its_notes`
(queue unit COMPACT-NOTICE-LATE) passed in both `cargo test` steps; it was
not run again.

**Every side's tests are in** (`test-counts.txt`, `#[test]` lines under
`src` and `tests`): train 6 `fdc5ac0b` 5280; T-U4-F +1 (5281 at merge 2),
MODELS-REASK +31 (5312 at merge 3), STOP-NEXT-2 +45 (5357 at merge 4), the
join tests +3, 5360 at the head; LAUNCH-SETTLE-F changes no Rust and adds 12
`def test_` to the harness (153 to 165). `cargo test` counts 5446 against
train 6's 5366 at the same code as `fdc5ac0b` (the dispatcher's gate at
`1755e9be`, train 6's README): +80, the same difference (1 + 31 + 45 + 3);
with fault injection 5483 against 5403, +80.

The file checks with the receipt in its final form (`gates-final.txt`, at
`146f2494`): `cargo fmt --check`, `check-xfx-identity.sh`,
`check-no-stubs.sh`, `check-no-secrets.sh` (15370 tracked files), each exit
0 [measured]; the code has not changed since `c468f5e1`, so the gate's
other steps stand. The two commits after `146f2494` change only
`gates-final.txt` and this README.

## Sweep

**Builds** (debug, as trains 4 to 6): the head `29786ced` (`x𝒇x
v0.1.0-29786ce [debug]` on row 1 of every xfx snap, sha256 `17fad4eb…`; the
code of `c468f5e1`), built in a `git clone --no-local`; `fdc5ac0b` (train
6, `v0.1.0-fdc5ac0`, `04326bea…`) built in another. fx: the v0.0.16 binary,
sha256 `8e6cda2e…` before and after each sweep, `FX_AUTO_UPGRADE=0`.
`sweep.sh` (train 6's): `scripts/fxcap/sweep.sh --fx <fx> --pin-sha
8e6cda2e… --xfx <bin> --jobs 2 --size 40x120 --only …`, each sweep inside one
hold of the suite lock, each binary with its own tree's harness, scenarios
and ledger (`sweep-runs.txt`). `fdc5ac0b`'s sweep and every unit's own were
then rescored with the head's ledger (`sweep.sh --rescore` on copies, no
capture taken), so a verdict that moves moved because a capture did.

**Selection** (`selection.txt`, each glob with why): every `p5-lifecycle-*`
(STOP-NEXT-2's selection), every model and catalog scenario (`model*`,
`h1-models*`, `p5-input-model*`: MODELS-REASK's selection), the Ctrl-O and
transcript scenarios (`*ctrlo*`, `p5-transcript-*`, `fulltranscript*`),
every scenario a unit added or edited (`git diff --name-only fdc5ac0b HEAD --
scripts/fxcap/scenarios`: those, and LAUNCH-SETTLE-F's `p5-esc-clear`,
`p5-mcp-fail`, `p5-mcp-fail-held`, `p5-mcp-menu-old`), the scenarios of
every ledger entry the train changed (`approvals`, `p5-call-row-*`, `links`,
`read-failures`, `resume-tools`, `timeout`, `h1-resize`; the two `'*'`
entries `session-record` and `stats-bytes` changed in reference numbers
only, which no sweep reads), and the cancel scenarios (`interrupt*`,
`question*`: STOP-NEXT-2's stop). 110 scenarios at the head, 106 at
`fdc5ac0b` (the four the units added are not there).

**Load.** The base sweep ran at load 50 to 3 (start to end). The head sweep
started at 7.6 and ended at 6.4, but from about 16:35Z another worker
(FLAKE-MCPMODE) ran a load loop and the load was 29 to 37 from 16:37 to
16:42Z (`head-sweep-load.txt`: scenarios 114 to 158 of 221, among them
`model`, `model-busy`, `model-column-reask`, `model-reask`, `p5-input-model`
and several `p5-lifecycle-osc-*`). Every pair that moved or gained a row
was swept again on the same build once the load was gone (`sweep-rerun.sh`,
load 7.0 to 4.4; `sweep-rerun.md`, `compare-rerun.txt`): **0 verdicts moved
against the head sweep** [measured].

**Result at the head** (`sweep-head.md`): 110 scenarios ran, **0 failed, 0
timed out**; 476 pairs, **293 pass** (61.6%). `fdc5ac0b`'s (`sweep-base.md`):
106 scenarios, 467 pairs, **280 pass**, 0 failed, 0 timed out.

| family | `fdc5ac0b` | **head** | LAUNCH-SETTLE's own | MODELS-REASK's own | STOP-NEXT-2's own |
|---|---|---|---|---|---|
| `p5-lifecycle-*` | 28 / 69 | **28 / 69** | 28 / 69 (round 1) | - | 28 / 69 |
| model and catalog | 64 / 80 | **75 / 84** | 68 / 80 (round 1) | 75 / 84 | - |
| Ctrl-O (`*ctrlo*`, `fulltranscript*`) | 21 / 54 | **21 / 54** | 13 / 31 (round 1) | - | - |
| `p5-transcript-*` | 51 / 107 | **51 / 107** | 45 / 107 (round 1) | - | - |
| the cancel scenarios | 78 / 87 | **78 / 87** | 50 / 87 (round 1) | - | - |
| LAUNCH-SETTLE-F's scenarios | 21 / 24 | **24 / 29** | 23 / 29 (round 2); 2 / 5 (F) | 2 / 2 | - |
| every scenario swept | 280 / 467 | **293 / 476** | 1102 / 1716 (round 1, every scenario) | 75 / 84 | 28 / 69 |

(pass / pairs, each sweep scored with the head's ledger; `families.txt`.)

### Against `fdc5ac0b` (`compare-base.txt`, `screen-moves-base.txt`)

Over the 467 pairs both took: **280 → 287 pass; 7 verdicts moved, all up;
0 down** ("passed on the reference, not on the train: 0"). The other 6 of
the head's 293 are pairs of the scenarios the units added (`model-column-hold`
×4, `p5-esc-clear` `a-cleared` and `c-later`; `p5-esc-clear` `b-edit-escape`
and `c-deleted` and `p5-mcp-fail-held` `held` are unresolved, as on
LAUNCH-SETTLE-F's own).

| pairs | `fdc5ac0b` → head | what moved them |
|---|---|---|
| `h1-models2` `after` | unresolved → normalized-identical | **MODELS-REASK**: the second `/model` reads the catalog again, so xfx lists fx's two models (`fdc5ac0b`'s rows: xfx `Models 1`, fx `Models 2`) |
| `model-column-idle` `col-typed`, `model-column-reask` `col-back` | unresolved → normalized-identical | **MODELS-REASK**: the column says `unable to load models` while the retry reads, as fx's (`fdc5ac0b`: xfx listed rows) |
| `model-floor` `after-floor`, `inside-floor`; `model-reask` `menu-failed`; `p5-input-model-fail` `fail-open` | unresolved → normalized-identical | **MODELS-REASK**: `Could not reach AI Gateway; xfx retries automatically.` where `fdc5ac0b` said `…; retry /model.` |

The same seven moved in the rerun at load 4-7 (`compare-rerun-base.txt`),
and they are MODELS-REASK's own sweep's verdicts (below).

Rows that moved without a verdict move (`compare-base.txt`, "pairs that
gained a differing row", 7; each unresolved on both): `model-parked`
`menu-loading` (xfx `~ Thinking (<s>) (↑4 ↓0)` where `fdc5ac0b`'s was `~
Generating (<s>) (↑4 ↓3)` against fx's `~ Thinking (<s>)`: MODELS-REASK's
turn waits out the retry, its ledger cause `activity-tokens-before-request`,
owner ACTIVITY-TOKENS; the same rows on MODELS-REASK's own sweep);
`interrupt-recovery` `full` (a network notice's `1ms` against `0ms`, fx's
`13ms` against `14ms`: timing); `p5-ctrlo-wider-ri` `answer`, `erased`,
`typed` (**fx's** row: its leftover `)` drawn at another column, a known fx
artefact, `ri-run-pairing`; in the rerun fx's row was back to
`fdc5ac0b`'s); `p5-lifecycle-relhome-unsaved` `u02-status` and
`read-failures` `live-ctrl-o-up1` (the sweeping tree's path in a row, whose
length moves what aligns and wraps; run-local). `screen-moves-base.txt`:
213 of 253 moved snaps only in clock-like or run-local text; of the rest,
xfx's moves are resume ids and the build id (run-local) and MODELS-REASK's
model rows above, and fx's are `p5-ctrlo-wider-ri`'s `)` and a resume id.

### Against each unit's own sweep (`compare-unit-*.txt`, `screen-moves-unit-*.txt`)

- **MODELS-REASK** (its round 2 sweep, `ba6670a`, `model*`, `h1-models*`,
  `p5-input-model*`, 84 pairs; `sweep-unit-models-reask.md`): **75 → 75; 0
  verdicts moved, 0 rows gained**; 2 snaps moved only in clock-like text.
- **STOP-NEXT-2** (its head sweep, `7d5e3d4`, every `p5-lifecycle-*`, 69
  pairs; its round 2 sweep at `a1b3451` changed no verdict against round
  1's, its `sweep-r2.txt`): **28 → 28; 0 verdicts moved**; one pair gained a row,
  `p5-lifecycle-relhome-unsaved` `u02-status`, the run-local path above.
- **LAUNCH-SETTLE-F** (its F round's sweep, `9c01878`, `p5-esc-clear` and
  `p5-mcp-fail-held`, 5 pairs): **2 → 2; 0 moved, 0 rows gained.** Its round
  2 sweep (`f525e50`, 29 pairs): 23 → 24, one up -- `h1-models2` `after`,
  MODELS-REASK's (above) -- 0 down. Its round 1 full sweep (`85428cc`, at
  train 4's base, every scenario; 444 pairs shared): 235 → 279, 46 up (T-U4's
  marker in the cancel scenarios, `p5-transcript-cancel` and `resume-tools`;
  MODELS-REASK's sentences and rows; LAUNCH-SETTLE r2's `p5-mcp-fail`
  `server-1` and `p5-mcp-menu-old`; `read-failures` `resumed` to identical)
  and **one down: `model-parked` `menu-loading`**, normalized-identical on
  that sweep and unresolved on the head -- and on `fdc5ac0b` already, and on
  MODELS-REASK's own: the activity row's token suffix, which came between
  train 4 and `fdc5ac0b` (`fdc5ac0b`'s xfx row `~ Generating (<s>) (↑4
  ↓3)`; which unit, not traced here), and since MODELS-REASK the cause
  above. Not this train's merge; no pair that passes on `fdc5ac0b` or on any
  unit's own sweep of this train's code fails on the head.

- **T-U4-F** has no sweep of its own (its receipt is its RED, its gate and
  its mutants: `../cancel-marker/t-u4-f/`); its path is the transcript
  row's cut, and its scenarios -- every `p5-call-row-*`, the cancel set,
  `p5-transcript-*` -- moved no verdict against `fdc5ac0b` (the seven moves
  above are all model scenarios).

So no pair that passes on `fdc5ac0b`, or on MODELS-REASK's, STOP-NEXT-2's
or LAUNCH-SETTLE-F's own sweep, fails on the train.

## Found on the way, not changed here

- **A local path in two of a unit's receipts.** STOP-NEXT-2's
  `.prd/tui-phase5/receipts/stop-next-2/gates.sh:9` and STOP-NEXT's
  `.prd/tui-phase5/receipts/stop-next/gates.sh:9` each set `TMPDIR` to a path
  under the machine's temporary root with the account's uid in it; merge 4
  brought both onto this branch (`git diff fdc5ac0b HEAD -- .prd`: the only
  two such added lines; none outside `.prd/`). `main` holds the same root in older receipts
  already (`git grep` over `origin/main`: `.prd/tui-phase4/…`,
  `osc-program-status/mutants.py:13`). This train changes no unit's receipt;
  routed to the dispatcher (a one-line redaction there, or REF-LINT's sweep
  of receipts).
- **A Ctrl-C at `/model`'s menu.** It is no key of the menu
  (`ModelAction::of`, `src/tui/model_menu.rs:125-139`: Escape closes it); in a
  stop's window it reaches `interrupt` and stops nothing more, and the menu
  stays open. Train 7's band test closes the menu with Escape for that
  reason. Whether fx's menu takes Ctrl-C was not checked here [not
  measured]; MODELS-REASK's own test backs out of `/model`'s column with
  Ctrl-C, not of the menu.
- **The FLAKE-CATALOG-EVENT pin's first test** passes with no read begun
  under MODELS-REASK's first-load rule (merge 3, above); its guard holds
  [read from the code, there].

## Redaction

The repository is public. Every file here taken from a sweep, a gate log, a
probe, the logger, a mutant run or a clone went through `redacted.py` (train
6's: `scripts/fxcap/redact.py`'s `Redactor.write`, every path under a local
root to `<local-path>` or a named root such as `<repo>`; a checkout's path
cut short to `<checkout>/`), with one change for train 7: the cut-short rule
starts at a token's start and leaves a diff's own `a/` and `b/` prefixes
alone (a remerge-diff names `a/scripts/fxcap/…`), and does not run again on
its own `<checkout>/`. The scripts here take the oracle's path and the
scratch directories as arguments or variables. Then, over every file of this
directory, for the four forms trains 5 and 6 named -- the uid fragment
before a slash, the machine's scratch root's name, the system temporary root
and the home root, each spelt with a bracket class so the command matches
what it looks for and not its own text -- and the account's name:
**0 hits** (`redaction-grep.txt`).

## Files

- `remerge-1.txt` .. `remerge-4.txt`: `git show --remerge-diff` of each merge.
- `overlap.py`, `overlap.txt`: every file both sides of a merge changed, and
  the nearest pair of hunks.
- `rowmerge.py`: how merge 4 resolved `docs/parity.md`'s row, word by word
  against the base (train 6's, `--sub` and the side-within rule added);
  `refconflict.py`: how merges 3 and 4 resolved conflict blocks whose sides
  differ in `:N` alone; `findline.py`: where a cited line's window is in a
  merged file.
- `merge-checks.sh`: writes the token check, its control, the remerge-diffs,
  the reference maps, the re-points' check, the stale list and the overlaps.
- `token_check.py`, `token-check.txt`, `token-self-test.txt`: train 6's token
  check with train 7's numbers-only rules, and its negative control.
- `refs.py`, `refs.txt` (before the re-points), `refs-head.txt` (the head);
  `refs_all.py`, `refs_short.py`, `refs-all-merges.txt`,
  `refs-short-merges.txt`, `refs-all.txt` (the head, both forms);
  `repointed.py`, `repointed.txt`: the re-points and the merges' numbers
  against their code; `stale.py`, `stale.txt`: the eleven references still
  flagged outside the plans.
- `glyph-red.txt`: STOP-NEXT-2's marker tests at merge 4, before `06189ce8`.
- `apply_join.py`, `join_units.sh`, `join-units.txt`: join check 1's tests on
  each unit alone; `join_mutants.py`, `join-mutants.txt`: join checks 1 and
  2's mutants.
- `jobs-a.sh` (the units alone, the network, test-capture five times),
  `jobs-b.sh` (the mutants, the probe), `jobs-c.sh` (the OSC wires): each one
  hold of the suite lock; `jobs-a.txt`, `jobs-b.txt`, `jobs-c.txt`: what
  each printed.
- `net.py`, `net.txt`, `connect.log`: join check 5; `test-capture.txt`: join
  check 3's five runs.
- `osc-head/`, `osc-head-run2/`, `osc-head-run3/`: the head's xfx wires for
  STOP-NEXT-2's checker (`../stop-next-2/osc.py`); `osc.txt`: its output.
- `probe/`: `render_cost_probe.rs` (train 6's, for `fdc5ac0b`),
  `render_cost_probe-head.rs` (the same with `live.start_turn()`), `prep.sh`,
  `run.sh` (train 6's), `probe.txt`: join check 6.
- `gates.sh`, `gates_txt.py`, `gates.txt`: the gate as it ran, and how
  `gates.txt` was assembled.
- `test-counts.txt`: `#[test]` and the harness's `def test_` per commit.
- `selection.txt`: the sweep's `--only` globs, each with why; `sweep.sh`:
  how each sweep ran; `families.py`, `families.txt`: pass per family.
- `sweep-head.md`, `sweep-base.md` (rescored with the head's ledger),
  `sweep-unit-*.md` (each unit's own sweep, rescored); `sweep-runs.txt`: the
  two sweeps' build, oracle and load lines; `head-sweep-load.txt`: the head
  sweep's progress and load every 30 s through the other worker's spike;
  `sweep-compare.sh`, `compare.py`, `compare-*.txt`, `screen_moves.py`,
  `screen-moves-*.txt`: pair by pair; `sweep-rerun.sh`, `sweep-rerun.md`,
  `compare-rerun.txt`, `compare-rerun-base.txt`: the moved pairs swept again
  after the spike.
- `gates-final.txt`: `cargo fmt --check` and the three file checks with the
  receipt in its final form (all exit 0).
- `redacted.py`: every sweep-, gate- and run-derived file here went through
  `scripts/fxcap/redact.py`'s `Redactor.write`; `redaction-grep.txt`: the
  grep over this directory, its command and its count.
