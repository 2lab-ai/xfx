---
title: 0.0.16 delta -- P5 train 15 receipt (CATALOG-WAIT-CLOCK, the decoder line, FULLTRANSCRIPT-RESPONSE-HEADER, the approval stack and QUESTION-CTRL-J into one branch)
type: receipt
tags: [xfx, phase5, fx-0.0.16, train]
---

# P5 train 15: thirteen closed units, five merges, one branch

Branch `train/p5-1111b`, from main at `753b0a72` (PR #93, train 14, merged
2026-11-11 05:28Z), as the brief set it; then main `e2fc7e94` (#95, #96 the
v0.2.0 release, #97) joined in before the gate and the sweeps, at the
dispatcher's word, so both run once on the final tree. Each unit was closed
by its own two-engine review and the dispatcher's gate; this train combines
them. The program goal (the user's words): «최신 fx랑 비교해서 ui쪽으로 모든
기능 구현하고 기본적으로 최신 fx ui를 그대로 구현해줘» (fx v0.0.16, pin
`481104fd`).

Oracle: fx v0.0.16 (`481104fd`), executable sha256
`8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked
by each sweep before and after it ran, `FX_AUTO_UPGRADE=0`
(`sweep-runs.txt`).

Model: train 14's receipt (`../train-14/`, its `join-main/`) and the
dispatcher's reviews of it. Its scripts are copied here, each saying in its
first lines what changed for this train (most: nothing but that line); the
new ones say so too, and train 14's scripts for checks this train has no use
for are not copied (`anchor.sh`, `red3.sh`, `pages.sh`, `joins*`,
`walk_rows.py`, `main-gate.sh`, `named.*`, `eventloop.py`, `semantic.py`).
What those reviews asked is carried: every figure rests on a file of this
folder, named where it is used; counts are checked against the lists they
count; the path scans run over this folder (`pathcheck.sh` before every
commit, `final-scan.sh`); "no code after the code head" is shown by
`final-scan.txt` part 4, which reads through the receipt's second-to-last
commit and names it, while the last commit stores `final-scan.txt` alone.

## Units and merges

Each merge is a stack top and carries the units under it; every base is
git's merge base of the merge's two parents (`merge-ends.txt` names each).

| # | merge | units it carries (branch @ tip; commits since its base) | base | conflicts |
|---|---|---|---|---|
| 1 | `2ff302c5` | CATALOG-WAIT-CLOCK (`fix/p5-catalog-wait-clock` @ `72fc9672`; 17) | `afd891af` | `src/tui/worker.rs` 1 block, by hand |
| 2 | `ed92e515` | the decoder line, oldest first: DECODER-TILDE-HOME-END `ebb04642` (7), TILDE-EMPTY-MODIFIER `fab7a217` (10), MODIFIED-PAGE-KEYS `c77a77ce` (7), MODIFIED-ARROWS-SCREENS `3f2d6e78` (9), COMPOSER-PARAGRAPH-MOVE `28bd1810` (10), COMPOSER-SUPER-ARROWS `df45025c` (13) (`fix/p5-composer-super-arrows`; 56) | `afd891af` | `src/tui/shell.rs` 2 by hand; the ledger 12 (11 `numbers`, 1 by hand); `docs/parity.md` 1 block, 3 rows |
| 3 | `b10ca771` | FULLTRANSCRIPT-RESPONSE-HEADER (`fix/p5-fulltranscript-response-header` @ `fffb3cfe`; 13 of its own, on MODIFIED-ARROWS-SCREENS `3f2d6e78`, which merge 2 brought) | `3f2d6e78` | the ledger 5 (4 `numbers`, 1 by hand); `docs/parity.md` 1 block, 3 rows |
| 4 | `88e83a68` | the approval stack, oldest first: APPROVAL-READINESS-SETTLED `b3a19ede` (8), APPROVAL-DRAFT-KEYS `d7916178` (5), APPROVAL-CTRL-J `1a0adaec` (6), RULE-CONFIRM-KEYS `7c9dfb2e` (13) (`fix/p5-rule-confirm-keys`; 32) | `a37a969e` | `src/tui/shell.rs` 1 and `src/tui/approval.rs` 1, by hand; `tests/tui.rs` 1 `rewrite`; the ledger 12 (10 `numbers`, 2 `tail`); `docs/parity.md` 2 blocks, 8 rows |
| 5 | `07eefa89` | QUESTION-CTRL-J (`fix/p5-question-ctrl-j` @ `b5cb6f46`; 17) | `afd891af` | `src/tui/input.rs` 1, by hand; the ledger 13 (12 `numbers`, 1 `ours`); `docs/parity.md` 3 blocks, 7 rows |
| join | `a14df710` | main `e2fc7e94`: #95 `ed0fe4ff`, #96 `35cb4c7a` (v0.2.0), #97 `e2fc7e94` | `753b0a72` | `docs/parity.md` 3 blocks, 9 rows |

The unit counts are `git rev-list --count <base>..<tip>` (the decoder line's
7 + 10 + 7 + 9 + 10 + 13 = 56, the approval stack's 8 + 5 + 6 + 13 = 32;
`final-scan.txt` part 2 counts each tip since this train's base). Not
merged, as the brief has it: the profile and slash lines, TEST-ENV-ISOLATION,
LOCAL-PATHS-WRAP (train 16) and every open unit.

After the merges, in order (`git log --first-parent 753b0a72..`):

| commit | what |
|---|---|
| `0341dd2b` | **code and tests**: the join MUSTs (join check 2): the approval's C-j counts for `/mcp`'s held rule; two join tests; the stale owner clauses in `docs/parity.md` |
| `da8e4c1a` | 34 references re-pointed, numbers only (join check 6). **Its message says "32 + 2 = 34 lines"; it is 32 lines holding 34 references** -- ledger :506 and :882 hold two each (`repoint-plan.txt`, `repointed.txt`: 32 lines out, 32 in) |
| `bc9ea199` | `docs/parity.md`, one sentence merge 2 left reading the other way (the token check's first difference, below) |
| `a14df710` | **the join of main** `e2fc7e94` |
| `2c575f52` | the three UPSTREAM.md references #96 moved, re-pointed: **the code head the gate, the sweeps and every job ran at** |
| the receipt's commits | this folder only (`final-scan.txt` part 4) |

A first gate ran at `da8e4c1a` and was stopped after 16 of 23 steps, every
one exit 0 (`gate-da8e4c1a.txt`), when `bc9ea199` was found to be needed;
then the handoff stopped the train and main moved. Nothing of it is used
here but the record that it ran.

**A gap, and what it cost.** The gate, the mutants, both sweeps, join checks
5 and 7 and every file here but `reruns.txt`, `gates-files.txt` and
`final-scan.txt` were made on 2026-11-11, at the code head `2c575f52`, and
copied into this folder as each finished. The run then stopped (an API rate
limit) until 2026-11-16; in the gap the machine's periodic `/tmp` cleanup
deleted the scratch copies -- the gate's logs, the sweeps' run directories,
the clones -- and the first reruns of the movers, which had not finished
being copied. The code head did not move, so nothing was gated or swept
again; the reruns were made again from the same two commits, built afresh
(`rerun-job.sh`, `reruns.txt`), and the files that read the sweeps' raw
output were all written before the cleanup. Main moved too, to `afeaea7c`
(#98, `.github/preview-epoch` alone): the base stays `e2fc7e94`.

## The conflicts and their resolutions (`resolutions.txt`, `check-merges.txt`)

Every resolution is a script's, redone by `reproduce.sh` in a clone to
each merge commit's tree: **all six "no difference"**, each resolver's own
lines (one per block or row) printed under its merge (`resolutions.txt`).
Before each commit the staged tree passed `cargo check --all-targets` with
and without `fault-injection`, `cargo fmt --check` and both clippy runs
(`check-merge.sh`, no lock, as trains 13 and 14; `check-merges.txt`, the
join commit's and the join of main's runs too).

The tools, each train 14's with what this train added (each script's head
says it):
- `blocks.py`: one rule added, `tail` -- theirs is the base's lines and
  lines it appended after them, where git met ours' change of those lines:
  the base's lines as `numbers` reads them (or ours' when ours rewrote them),
  then theirs' new lines. Used twice, merge 4.
- `rowsmerge.py`: two options added, each used once and each read by hand
  here: `--meet <row>@<a1>-<a2>` -- both sides replaced the same base token
  with texts that start alike, merged as ours' text, a blank, then theirs'
  past what they share (merge 2); `--inside <row>@<a1>-<a2>` -- theirs'
  edit of a word lies inside an edit of ours', whose text holds that word
  once, and is made there (merge 5).
- `byhand.py`: the blocks no rule fits, keyed by merge and found by the
  sha256 of their three sides, each rule a function that checks the text
  it expects and stops otherwise.

**Merge 1** (`2ff302c5`, CATALOG-WAIT-CLOCK onto main):
- `src/tui/worker.rs`, 1 block (`byhand.py 1`). The base held, in a row,
  CATALOG-WAIT-CLOCK's test **E**
  (`a_turn_stopped_before_its_request_leaves_the_history_notice_to_the_next_turn`),
  `the_history_notice_ledger_goes_with_its_conversation`, and the head of
  **D**
  (`a_stop_through_a_dropped_compaction_tells_it_once_and_leaves_the_history_notice_to_the_turn_that_runs`).
  Main's IMAGE-LOSS-COPY-FX (train 14) deleted the first two -- the history
  holds no notice any more -- and renamed D
  (`..._tells_it_once_before_the_turn_that_runs`) with its notice taken out.
  The unit moved E and D onto its harness (the stop sent when the band hears
  the first turn start, the paused clock, `Held::run`) and gave D one assert.
  **E stays deleted: the unit's edits of it are dropped**, since what E
  asserts (the notice handed to the next turn) no longer exists; so does the
  ledger test. D keeps main's name and notice-free body, on the unit's
  harness (merged clean below the block), with the unit's `held` assert and
  its doc's account of when the stop is sent ("300 ms in" was no longer
  true). So of the unit's five catalog-wait tests A to E, four are at the
  head (A, B, C and D under main's name), plus its F and G
  (`unit-tests.txt`; D under its head name in `cw_runs.py`, ok in all 20 of
  `cw-runs.txt`'s runs).

**Merge 2** (`ed92e515`, the decoder line onto merge 1):
- `src/tui/shell.rs`, 2 blocks in `Shell::consume_inputs` (`byhand.py 2`):
  train 14's C-j/C-k check under the full transcript, then the line's
  `let menus = arrow_to_pickers(&event);` (both inserted at one place); and
  CTRLO-OTHER-PICKERS' `offer_to_pickers` call -- the same twelve pickers in
  the same order, which the rule checks -- given `&menus`, as the line gave
  each of the twelve.
- The ledger, 12 blocks: 11 `numbers`, 1 by hand: MODIFIED-PAGE-KEYS' new
  `fulltranscript-response-header` cause, then train 14's comment where
  `at-ranking` stood.
- `docs/parity.md`, 1 block, 3 rows: the full-screen TUI row (25 edits of
  ours, 118 of theirs) and the `@` row token by token; in the full
  transcript row both sides extended the same clause after
  `(\`app_input_runtime.zig:973-994\`)` -- train 14's C-j/C-k clause and the
  line's composer-move clause -- merged by `--meet`, train 14's first.

**Merge 3** (`b10ca771`, FULLTRANSCRIPT-RESPONSE-HEADER):
- The ledger, 5 blocks: 4 `numbers`, 1 by hand: the unit deleted the cause
  it closes (`fulltranscript-response-header`) and main had deleted
  `at-ranking` after it; both deleted, train 14's comment kept, and the
  deleted entry's `[[cause]]` header above the block (which git matched to
  the other entry's) taken out with it.
- `docs/parity.md`, 1 block, 3 rows, token by token, none meeting.

**Merge 4** (`88e83a68`, the approval stack):
- `src/tui/shell.rs`, `Shell::consume_inputs`, 1 block (`byhand.py 4`):
  RULE-CONFIRM-KEYS put the rule confirmation's check above the fold,
  reading `spelled` (the event before `unspelled`, which its base has);
  the decoder line (MODIFIED-ARROWS-SCREENS `8b5aacaa`) deleted `unspelled`
  and its `let (spelled, event)`, so every surface above the fold reads the
  event as it came, the approval's `decide(event, ..)` included. The check
  reads `event`. Then the line's comment on the fold, less its clause "a rule
  change is fx's approval", which put the rule confirmation among the menus
  below the fold, where it is no longer read.
- `src/tui/approval.rs`, `approval::edits`' doc, 1 block: the decoder line's
  first paragraph, then APPROVAL-DRAFT-KEYS' new one.
- `tests/tui.rs`, 1 block, `rewrite`: main had moved only a number in a
  comment the unit rewrote.
- The ledger, 12 blocks: 10 `numbers`, 2 `tail` (the stack's two new causes,
  `approval-settled-every-approval` and `rule-confirm-card-after-stop`,
  appended after entries main had changed).
- `docs/parity.md`, 2 blocks, 8 rows, token by token, none meeting.
- **Merged clean, and read**: RULE-CONFIRM-KEYS moved `Shell::decide`'s key
  translation into a function of its own, `Shell::approval_action`; the
  decoder line had edited that translation inside `decide`. git applied the
  line's edits to the moved text: `moved_edits.py` reads the line's edits of
  `decide` (14 lines out, 19 in) as exactly the edits that take
  `approval_action` from the stack's tip to the merge, and `decide` at the
  merge as the stack's byte for byte (`moved-edits.txt`). That is why
  `shared.py` reads `decide` "not kept both ways" (`shared.txt`).

**Merge 5** (`07eefa89`, QUESTION-CTRL-J):
- `src/tui/input.rs`, `Action::ControlJ`'s doc, 1 block (`byhand.py 5`):
  both questions C-j answers as Enter, the permission question
  (APPROVAL-CTRL-J, `approvalActionFromByte`,
  `src/ui/input/runtime.zig:64@481104fd`) and the model question
  (QUESTION-CTRL-J, `questionActionFromByte`, `:141`) -- join MUST 2b.
- The ledger, 13 blocks: 12 `numbers`, 1 `ours`.
- `docs/parity.md`, 3 blocks, 7 rows; in the `ask_user_question` row the
  unit's "(proposed)" -> "(queued)" for META-PREFIX-DECODE is made inside the
  decoder line's rewrite of that sentence (`--inside`).

**The join of main** (`a14df710`, main `e2fc7e94`):
- `docs/parity.md`, 3 blocks, 9 rows, token by token: in `/mcp` (1 edit of
  the train's, 7 of #96's) and the full-screen TUI row (149 and 42) none
  meet, so each of #96's rewordings stands as #96 wrote it -- the 34 slash
  rows, the three-case full-screen TUI sentence, the private references
  removed. Six phrasings #96 took out of the file (`XFX_TUI=1`, "preview
  channel", "preview-channel", `zbrain`, "open-loop", "Opt-in") are in the
  merge as often as on main (0) -- the train added none (`phrases.sh`,
  `phrases.txt`).
- The ledger merged clean: #97's five `capture.sh` numbers. Its three
  UPSTREAM.md references #96 moved are `2c575f52`'s (join check 6).
- No `src/tui` file of the train's is touched by main; `shell.rs` is not
  (`git diff 753b0a72 e2fc7e94 -- src/tui/shell.rs` is empty; main's one
  source change is a comment in `src/interactive.rs`).

## The token check (`token_check.py`, `token-check.txt`, `token-self-test.txt`, `merge-keep.txt`)

`token_check.py` is train 7's, byte for byte (`token-check.txt`'s first
line, `cmp` against `../train-7/`): every `docs/parity.md` row and every
ledger entry of each merge read against its base and both sides. Over the
six merges it **exits 1, on four differences, each read here**:

1. **Merge 2, full-screen TUI row, "2 of 70 clauses not whole".** Main
   (PAGE-KEYS-SETTINGS-COLUMN) wrote "... the composer's caret by the same
   number of rows (...). So do the three lists that took none until unit
   PAGE-KEYS-SETTINGS-COLUMN: ...", which says those lists page too.
   MODIFIED-PAGE-KEYS ended the sentence before it with "the question and the
   approval take neither (...)", so "So do" came to read as "take neither
   too" -- false. **Fixed in `bc9ea199`**: "The three lists that took none
   until unit PAGE-KEYS-SETTINGS-COLUMN page so too: ...".
2. **Merge 2, full transcript row, one token missing**
   (``(`app_input_runtime.zig:973-994`),``): the `--meet` above. Both sides
   replaced that token with text that begins with it; the merge has it once,
   then ours' clause, then theirs'. Each side's clause is whole in the merge
   (`merge-keep.txt`, merge 2's two "missing" units are these two
   sentences, now one).
3. **Merge 4, full-screen TUI row, "2 of 118 clauses not whole"**: two
   edits one blank apart. The decoder line (COMPOSER-SUPER-ARROWS) inserted
   its bold paragraph "**A capture artifact, not a divergence, ...** ... is
   proposed as its own unit." right after "owner unit proposed,
   APPROVAL-PASTE-SPACES."; APPROVAL-READINESS-SETTLED rewrote the head of the next
   sentence, "**Also not changed, found by this unit: when" to "**When". The
   merge reads "... APPROVAL-PASTE-SPACES. **A capture artifact ...** ... its
   own unit. **When an affirmative may be taken: ...", each side's text whole,
   each clause with the other side's text on one flank (`merge-keep.txt`: 0
   units missing at merge 4).
4. **Merge 5, `ask_user_question`, tokens swapped**: `(proposed),` and
   `(queued)` against `(queued),` and `(proposed)`: the `--inside` above --
   ours rewrote "(proposed) for the third," to "(proposed)," and theirs
   changed the word to "(queued)"; the merge says "(queued)," once.

- The ledger: every entry read as expected but one, at merge 5,
  `amendment-card-after-stop` "both sides changed it differently": both
  moved its `shell.rs` numbers alone -- the entry is one text on the base,
  both sides and the merge once every `:N` is masked -- ours to `:10947` and
  `:1932`, QUESTION-CTRL-J to `:10562` and `:1933`; ours kept (`blocks.py`'s
  `ours`, `resolutions.txt`), then re-pointed to `:11069` and `:1937` (join
  check 6).
- The join of main: OK, exit 0 (its own run before its commit, and the
  six-merge run).
- **The negative controls** (`token-self-test.txt`): `--self-test` on
  each merge, **187 of 187 planted cases found** (24, 34, 31, 35, 38 and the
  join's 25); `token_control.py` (train 13's) plants its two cases on the
  merged text of every row both sides changed, **33 of 33 found**;
  `word_control.py` (train 14's join of main's) plants a word in `why` of
  every ledger entry both sides changed in numbers alone, **27 of 27 found**
  (9, 1, 7 and 10 at merges 2 to 5; merge 1 and the join of main have no
  such entry, and the script exits 1 on "0 of 0", by its rule).
- **`merge_keep.py`** (the dispatcher's, from train 14's review): every unit
  (a cell, a sentence, a ledger line; 25 characters or more) new on either
  side, looked for in the merge (`merge-keep.txt`). As written 32 are not
  there, 28 of them ledger lines whose numbers the other side moved
  (`xfx = [...]`, `source = [...]`, and the `why` lines that cite a line),
  kept by `numbers` and re-pointed after; with digits read as `N`, **4**:
  merge 2's two sentences the `--meet` joined (one unit now) and merge 5's
  two the `--inside` joined. None else on any side.

## Join checks

Each with its evidence. Two join tests are added (join check 2), where no
unit's tests reach the meeting; every other meeting is reached by the units'
own tests, run in the gate (`unit-tests.txt`).

### 1. The approval amendment, where three units meet (`functions.txt`, `shared.txt`, `moved-edits.txt`, `unit-tests.txt`, `scenarios.txt`)

APPROVAL-DRAFT-KEYS (the draft's keys) and APPROVAL-CTRL-J (C-j) against
COMPOSER-SUPER-ARROWS (Super- and Ctrl-Home/End to the amendment's edges,
`shell.rs`' amendment routing, `approval.rs`).

- **The functions both sides changed** (`functions.txt`: each function's
  sha256, 12 hex digits, per commit, from its `fn` line, doc comments not
  included; `shared.txt`: each function two sides changed, read from both
  ends at its merge):
  - `Shell::decide` and `Shell::approval_action`: the decoder line edited
    the key translation inside `decide` (base `5fedc5badb25` -> decoder
    `303f0e0defa7`); RULE-CONFIRM-KEYS moved the translation out into
    `approval_action` (`decide` `25c7b22d9405`, `approval_action`
    `518b7263f640`). At merge 4 `decide` is the stack's text byte for byte
    and `approval_action` is `5e35ce3fa588`: the stack's plus exactly the
    decoder's edits, 14 lines out and 19 in (`moved-edits.txt`) -- the
    `DraftStart`/`DraftEnd` arm for Ctrl- and Super-Home/End, `ParagraphUp`
    and `ParagraphDown` swallowed, the fold's `LineStart`/`LineEnd` named.
    Both are that text at every commit after it and at the head.
  - `approval::edits` (`src/tui/approval.rs`): only APPROVAL-DRAFT-KEYS
    changed its body (`330f506fcc46` -> `4780c6a1dc2a`, `Undo`, `Redo`,
    `Yank` out); the decoder line changed its doc alone (merge 4's by-hand
    block). The head holds the stack's body.
  - `folded`: the decoder line's (`62f83981fd0b`, `LineStart`/`LineEnd` to
    Home/End), unchanged since merge 2; the stack does not touch it.
  - `Shell::consume_inputs`: changed by every side; at merges 2 and 4 by
    hand (above), at merge 5 kept both ways (`shared.txt`), at the join
    commit the held rule's C-j arm (join check 2).
- **Each side's tests on the merged code** (`unit-tests.txt`, read from the
  gate's two `cargo test` runs at the head): every test APPROVAL-DRAFT-KEYS
  (8), APPROVAL-CTRL-J (5) and COMPOSER-SUPER-ARROWS (22) added or changed is
  ok in both runs, 35 of 35, and so are the join's two.
- **Merge 4 from both ends** (`shared.txt`): four `shell.rs` functions both
  sides changed; `decide` (the moved translation, above) and
  `consume_inputs` (by hand) are not kept both ways, as said;
  `the_fold_maps_the_question_only_keys_to_the_plain_ones_and_leaves_the_rest`
  is; `the_full_transcript_holds_the_composer_spellings_of_the_arrows_as_it_holds_the_arrows`,
  which MODIFIED-ARROWS-SCREENS replaced with
  `a_held_arrow_under_the_full_transcript_moves_the_composer_behind_it_not_the_screen`
  (`08d6d605`: the screen no longer holds those spellings) and the stack
  left as it was, is not at the head, as on the decoder line's tip; its
  replacement is ok in both gate runs (`unit-tests.txt`,
  MODIFIED-ARROWS-SCREENS).
- **Their scenarios** (`scenarios.txt`, the two sweeps pair by pair): no
  `approval*` or `p5-approval*` pair the base took moved, verdict or cause,
  and the 49 new ones (the stack's and `p5-super-arrows-approval`) are each
  read below ("Join checks 1 and 4 in the sweeps").

### 2. The join MUSTs carried from earlier trains (`0341dd2b`, `mutants.txt`, `unit-tests.txt`)

Found in the units' reviews (QUESTION-CTRL-J's opus review, "MUST, at the
train join"; APPROVAL-DRAFT-KEYS' opus N-1 and DECODER-TILDE-HOME-END's README
"Join note") and STATUS.md's train 14 split; closed in `0341dd2b`:

- **The approval's C-j counts in `/mcp`'s guard.** `consume_inputs`'
  `answers` takes `Action::ControlJ` when `self.ask.is_some() ||
  self.asking()` (it took the model question's alone), and the `McpEnters`
  doc says so. New test
  `a_ctrl_j_a_permission_question_took_counts_for_the_held_rule`: a
  permission question over `/mcp`'s Remove; Enter (the control), a C-j the
  question answers, and a C-j its readiness check holds back (its frame not
  yet landed; `3`, the refusal, takes the question 100 ms later), each then
  Enter 200 ms later on Remove: swallowed and said, and an Enter past the
  window answers. **Mutants** (`mutants.txt`, a `git clone --no-local` at
  `da8e4c1a`, whose `shell.rs` is the head's, a 900 s bound each): M1 drops
  `|| self.asking()` -- the new test fails, its neighbours pass; M2 makes the
  C-j arm `false` -- the new test and QUESTION-CTRL-J's
  `an_enter_another_surface_took_counts_for_the_held_rule` fail. The
  unmutated run passes all five.
  - **The rule confirmation is left out, by reasoning, not a test**: a C-j at
    a `/permissions` rule confirmation (RULE-CONFIRM-KEYS) answers it too,
    but the two cannot be up together: `Shell::permissions` is the one place
    that opens a confirmation, from a submitted `/permissions` command (the
    composer's Enter, which `offer_to_mcp` takes first while `/mcp`'s menu is
    up), and while a confirmation is up it takes every key before the
    menu's place in `consume_inputs`. Not driven by a case.
- **The `Action::ControlJ` doc**: names both questions (merge 5, above).
- **APPROVAL-DRAFT-KEYS' owner clause**: the full-screen TUI row said the
  draft's keys are fx's "but for four differences each owned by a unit" and
  listed the tilde Home/End "(owner unit proposed, DECODER-TILDE-HOME-END)"
  and Ctrl+Shift+A/B/E/F as "Still apart". The decoder line made all three
  fx's; now the row names one difference (a paste's line breaks,
  APPROVAL-PASTE-SPACES) and says the other three are fx's since
  COMPOSER-SUPER-ARROWS and DECODER-TILDE-HOME-END. New test
  `the_tilde_edges_and_ctrl_shift_letters_in_an_amendment_are_fxs`: rxvt
  Home and End, the tilde Ctrl edges, kitty and xterm Ctrl+Shift+A/B/E/F,
  each in an amendment as fx's draft takes it
  (`approvalDraftActionFromShortcut`, `src/ui/input/runtime.zig:75-96@481104fd`).
- And the full-screen TUI row's "(QUESTION-CTRL-J, proposed)" now points at
  the `ask_user_question` row; the `/mcp` row names the permission
  question's C-j beside the model question's.
- **Not done here, named for a follow-up**: QUESTION-CTRL-J's review SHOULD 2,
  one table of the Ctrl+J spellings for the two questions (`CTRL_J`, 11, and
  `MODEL_QUESTION_CTRL_J`, 13, in `src/tui/shell.rs`'s tests).

### 3. The fold and the decoder (`functions.txt`, `shared.txt`, `unit-tests.txt`)

COMPOSER-SUPER-ARROWS' fold (`folded`: `LineStart`/`LineEnd` -> Home/End)
against the readers of Home/End, the draft actions, C-j and the new actions
in the approval stack and QUESTION-CTRL-J. Each reader and what holds it:

- **The approval's draft** reads a folded key through `approval::edits`
  (`approval_action`'s draft arm, `folded(Input::Action(editing))`), so
  Super-Left/-Right and Shift-Home/-End reach it as Home and End; the
  draft's edges (`DraftStart`/`DraftEnd`) are read ahead of the fold. Both
  in `approval_action` at the head (join check 1).
- **The rule confirmation** reads the event as it came, through
  `approval_action` with no draft (merge 4's resolution), so it takes the
  approval's keys and swallows the rest.
- **The model question** reads the event as it came too: `question::Act::of`
  is changed by the decoder line (`LineStart`/`LineEnd` as Home/End, the
  question's own moves) and by QUESTION-CTRL-J (`ControlJ` as `Submit`), and
  merge 5 keeps both ways (`shared.txt`, merge 5, `src/tui/question.rs`:
  base `c9af2dcb27d8`, decoder line `33805810b264`, QUESTION-CTRL-J
  `98e7736231e6`, merge `f8450db68b64`).
- **`/mcp`'s held rule** reads C-j before the fold (`consume_inputs`'
  `answers`; join check 2).
- **Every unit's own tests on the head** (`unit-tests.txt`, `unit-tests.sh`:
  each unit's tests added or changed in its range, read in the gate's two
  `cargo test` runs): **142 (unit, test) pairs, 0 not ok, 0 lost**. Those
  gone at the head are of two kinds, each named in the file: eight the
  decoder line folded or renamed within itself (DECODER-TILDE-HOME-END's six,
  TILDE-EMPTY-MODIFIER's two; not at the line's own tip `df45025c` either),
  and merge 1's two (CATALOG-WAIT-CLOCK's E, and D's old name). Every other
  one of the thirteen units' tests is ok in both runs.

### 4. FULLTRANSCRIPT-RESPONSE-HEADER x the decoder line (`resolutions.txt`, `repoint-plan.txt`, `repointed.txt`, `scenarios.txt`)

FULLTRANSCRIPT-RESPONSE-HEADER is built on MODIFIED-ARROWS-SCREENS
(`3f2d6e78`); the decoder line's last two units moved `shell.rs` under it.

- **Parity.** FULLTRANSCRIPT-RESPONSE-HEADER changed one row, the full
  transcript's: two insertions (an answer split at its blocks as fx's store
  splits it; an entry that lays out as nothing is no place to keep). Merge 3
  merged that row token by token with the decoder line's 23 edits, none
  meeting, and the full-screen TUI row (row 237) as the decoder line's
  alone: FULLTRANSCRIPT-RESPONSE-HEADER has no edit in it
  (`resolutions.txt`, merge 3's rows).
- **The ledger.** MODIFIED-PAGE-KEYS (the decoder line) wrote the cause
  `fulltranscript-response-header` for the second Response record; the unit
  deleted it, closing it, and named two others: `full-transcript-shift`'s
  scenarios (named, no longer `fulltranscript*`, so the cause has nothing
  to say of the unit's split scenario) and a new
  `session-record-page-bottom`. Merge 3 keeps the deletion (`byhand.py 3`)
  and both entries (the token check reads `full-transcript-shift` "changed by
  ours, its path:line numbers alone by theirs").
- **The re-point** (`da8e4c1a`): both causes' `xfx` refs name the session
  record's `tools:` row -- `full-transcript-shift`'s `src/tui/shell.rs:46711`
  (main's) and `session-record-page-bottom`'s `:42649` (written on the
  unit's tree) -- and both stand at `:49793` now, the frame's lines and two
  either side at one place (`repoint-plan.txt`, `auto`; `repointed.txt`).
- **In the sweeps** ("Join checks 1 and 4 in the sweeps", below): the two
  causes name at the head the pairs they named at the base, less `links`
  `full` (FULLTRANSCRIPT-RESPONSE-HEADER's record split moved it under
  `session-record` alone) and plus `p5-modified-page-keys` `full-pgup` (a
  scenario new to this sweep); the unit's two `full` pairs pass at the head
  where they did not at the base.

### 5. CATALOG-WAIT-CLOCK x everything (`jobs.sh cw`, `cw_runs.py`, `jobs-cw.txt`, `cw-runs.txt`)

After all the merges, at the code head: the head's two lib test binaries
(plain and `--features fault-injection`, built in a `git clone --no-local`
of `2c575f52`), then **the catalog-wait tests' own module,
`tui::worker::tests`, run whole -- every test beside them, as the gate runs
them -- 10 times on each**, one run after the other in one hold of the suite
lock, at the shared machine's own load, each run bounded at 900 s
(`cw_runs.py`, `cw-runs.txt`: one line a run, its load, its `test result`
line and each named test's result).

| binary | A | B | C | D | F | G | the module (204 tests plain, 212 with fault injection) | load at the runs' start |
|---|---|---|---|---|---|---|---|---|
| plain | 10/10 | 10/10 | 10/10 | 10/10 | 10/10 | 10/10 | exit 0 in 10/10 | 11.90-26.59, median 18.73 |
| fault-injection | 10/10 | 10/10 | 10/10 | 10/10 | 10/10 | 10/10 | exit 0 in 10/10 | 9.06-23.39, median 16.69 |

A to D are the unit's catalog-wait tests at the head (D under main's name,
merge 1), F and G its round-4 watcher tests; E is not at the head (merge
1). In the gate, at loads 60.6 and 50.9 (`gates.txt`, steps 04 and 05),
A, B, C, F and G passed in both `cargo test` runs (`unit-tests.txt`,
CATALOG-WAIT-CLOCK, by name), and both runs had 0 failed (`gate-counts.txt`).

### 6. References (`refs*.py`, `content.sh`, `refs_content.py`, `repoint.py`, `repointed.py`)

**Before any re-point**, at `0341dd2b` (the five merges and the join commit;
`refs-*-pre.txt`, seven sides: main `753b0a72` and the five tips): `refs.py`
11 (side, reference) pairs MOVED; outside the historical plans
(`docs/superpowers/plans/`) `refs_all.py` 14, `refs_short.py` 6,
`refs_bare.py` 3; `refs_prd.py` 0.

**The content check over every reference** (`content.sh`, train 14's: every
tracked file outside `.prd/` and the historical plans that holds a reference
by `refs_content.py`'s own pattern; each reference read against the commit
that wrote its numbers, the cited line and two either side):

| at | WRONG | `content-*.txt` |
|---|---|---|
| main `753b0a72` / main `e2fc7e94` | 2 / 5 | `content-753b0a72.txt`, `content-e2fc7e94.txt` |
| CATALOG-WAIT-CLOCK `72fc9672` / the decoder line `df45025c` / FULLTRANSCRIPT-RESPONSE-HEADER `fffb3cfe` / the approval stack `7c9dfb2e` / QUESTION-CTRL-J `b5cb6f46` | 34 / 36 / 36 / 37 / 41 | `content-<tip>.txt` |
| `0341dd2b`, before the re-point | **34** | `content-0341dd2b.txt` |
| `bc9ea199`, the train before the join | 2 | `content-bc9ea199.txt` |
| `a14df710`, the join of main | 5 | `content-a14df710.txt` |
| the code head `2c575f52`, as it is | **2** | `content-head.txt` |
| the same, blame ignoring the two re-points (each re-pointed ref read against the frame before it) | 2, the same two | `content-head-ignoring.txt` |

The units' tips read 34 to 41 WRONG because they were built on train 13's
head (`afd891af`, or `a37a969e` for the approval stack), before train 14
re-pointed main's: what decides is the head.

**Where the 34 came from** (`repoint-plan.txt`, `repoint.py --derive` at
`0341dd2b`; a ref is a side's when that side's content check reads it WRONG
with the same citing file, frame and numbers): **32 moved by the merges**,
2 WRONG on main already, the two train 14 left. All 32 `auto`: the frame's
cited lines and two either side stand at one place of the head's file.
Among them join check 4's two, RULE-CONFIRM-KEYS' two `queue_card` refs
written on its base (`:1840`) with main's (`:1932`), all `:1937`, and
smoke-tui.sh's three at `:11208`-`:11213` that train 14 set (`lines`,
`hand` and `extra` there), now `editor.rs:552`, `shell.rs:13184` and
`input.rs:1431`, each `auto` from the frame train 14's re-point wrote. Applied in
`da8e4c1a` with the two bare continuations `refs_bare.py` read MOVED --
`capfile.py:181`'s `:15609-15611` to `:15785-15787` (the full transcript's
`if self.owner != ScreenOwner::Primary` in `warning_shown`, as in train 14)
and `src/tui/shell.rs:27637`'s `:6384` to `:6418`, with its partner: 32
lines out, 32 in, the same once numbers are masked, holding 34 references
(`repointed.txt`).

**Where the join's 5 came from** (`repoint-plan-2.txt`, at `a14df710`): all
main's -- the two train 14 left and three ledger `source` refs into
UPSTREAM.md that #96 moved, WRONG on main `e2fc7e94` too: xfx's three
deliberate departures (the step limit `:90-94` -> `:83-87`, the sandbox
`:95-98` -> `:88-91`, the update channel `:99-101` -> `:92-94`), `auto`.
Applied in `2c575f52`: 3 lines out, 3 in, numbers only (`repointed.txt`).

**At the code head `2c575f52`** (`refs-*-head.txt`, against main
`e2fc7e94`, the train before the join and the five tips): **`refs.py` 0
MOVED (91 references); `refs_prd.py` 0**; outside the historical plans
`refs_all.py` 1, `refs_short.py` 0, `refs_bare.py` 0. The one is
`scripts/fxcap/test_sweep.py`'s `cells.py:100000`, a test's number past any
file's end (the content check's `UNREAD`), as in trains 13 and 14. Inside
the historical plans the scans list entries they do not judge, as in train
14. **The content check: 2 WRONG, the two left on purpose and named**
("Found on the way"): `src/tools/question.rs:303`'s `Cargo.toml:42` and
`tests/session_compat.rs:57`'s `src/session/event.rs:98-192`. The same
five scans and the content check run again at the receipt's second-to-last
commit, with the receipt in its final form (`gates-files.sh`,
`gates-files.txt`, which names the commit it read).

### 7. Network hermeticity (`jobs.sh net`, `net.py`, `jobs-net.txt`, `connect.log`)

Train 14's method (FLAKE-CATALOG-EVENT's `connect_logger.py`, a loopback
logger that answers 403 and forwards nothing; the four proxy variables at
it, `NO_PROXY`, `no_proxy`, `ALL_PROXY`, `all_proxy` unset), a `git clone
--no-local` at `2c575f52`, one hold of the suite lock, the same three test
binaries:

| run | result | CONNECTs logged during it |
|---|---|---|
| `cargo test --locked --lib --no-fail-fast` | 5015 passed, 0 failed, 7 ignored | **2**, both `CONNECT w.example:443` |
| `cargo test --locked --test tui --no-fail-fast` | 189 passed, 0 failed | **0** |
| `cargo test --locked --test sessions --no-fail-fast` | 67 passed, 0 failed | **0** |

**No CONNECT to the Gateway host**; the two `w.example` lines are the known
MCP probe (trains 6 to 14). The other integration binaries ran in the gate,
with no logger. No unit of this train adds a network path: the decoder line
and the C-j units route keys, CATALOG-WAIT-CLOCK changes tests only,
FULLTRANSCRIPT-RESPONSE-HEADER how an answer is recorded, the approval stack
when and how a question is answered; main's three PRs are docs, a check and
the capture harness.

## Commits partway along the branch (`midbranch_scan.py`, `final-scan.sh`, `final-scan.txt`)

The brief's check over **every commit of `753b0a72..HEAD`**: the units' own
(each tip since this train's base: CATALOG-WAIT-CLOCK 17, the decoder line 56,
FULLTRANSCRIPT-RESPONSE-HEADER 46 -- the decoder line's first 33 and its own
13 --, the approval stack 32, QUESTION-CTRL-J 17), main's three PRs, and this
train's, each as `git show --format=` prints it (a merge as its combined
diff), every hit classified by `midbranch_scan.py` (train 14's; the
account's name and uid read at run time, never written; no hit printed but
its kind and file):

- **No real user name or credential**: NAME 0 and CREDENTIAL 0 in every
  commit and in the range; no commit message holds the account's name. The
  train went on.
- **No hit at all**: the brief's count is 0 in every commit of the range, the
  units' mid-branch commits included, and over the range itself
  (`final-scan.txt` parts 1 and 2). There was nothing to classify as a
  placeholder or a scratch root, and nothing to rewrite.
- **The dispatcher's per-commit check** (`final-scan.txt` part 5): every
  commit of `git rev-list --no-merges HEAD --not origin/main` (`afeaea7c`
  by then: #98's one commit, which leaves the list as it is against
  `e2fc7e94`), its added lines and its message, and every merge commit's
  own message: 0 hits.
- Beyond the brief's terms, `shapes.py`'s run-name shape (a capture's
  `fxcap.` and six) matches 14 added lines of the range, all APPROVAL-CTRL-J's
  (`final-scan.txt` part 1, by file): its `p5-approval-ctrl-j-panel`
  captures' `/status` panel, whose Workspace row is
  `<tmp>/fxcap.<six letters>/work` -- a placeholder root and the run's random
  directory name, kept as the unit committed them.
- "No code after the code head": `final-scan.txt` part 4 reads every commit
  after `2c575f52` through the receipt's second-to-last commit and finds no
  file touched outside this folder; the last commit stores `final-scan.txt`
  alone (`git show --stat` of it).

## Gate (`gates.sh`, `gates.txt`, `gate-counts.txt`, `gate-files.sh`)

`gates.sh` (train 14's; `ci.yml` and `sweep.yml` are the same at main
`e2fc7e94` as at `753b0a72`): every step of `.github/workflows/ci.yml`'s
gate job and `sweep.yml`'s two harness tests, in a fresh `git clone
--no-local` at **`2c575f52`**, one after another, **the whole script inside
one hold of the suite lock** (one normal slot), each step's exit taken from
`$?` on the line right after its command, the tree clean before and after,
TMPDIR the harness scratch root. **23 steps, 0 failed, none run again**
(`gates.txt`), at a machine load of 32.6 to 66.8 at the steps' starts. With
CATALOG-WAIT-CLOCK merged, a catalog-wait failure would have been a finding;
there was none (and join check 5).

| step | at `2c575f52` |
|---|---|
| `cargo fmt --check`; clippy; clippy `--features fault-injection` | ok; ok; ok |
| `cargo test --locked --no-fail-fast` | **5942 passed, 0 failed, 7 ignored** |
| the same `--features fault-injection` | **5979 passed, 0 failed, 7 ignored** |
| `check-xfx-identity` / `check-no-stubs` / `check-no-secrets` | ok / ok / ok |
| `check-no-local-paths` / `test-check-no-local-paths` | ok (24250 tracked files; 8 marked fixture lines, 81 placeholder lines) / ok (31 cases; #95's) |
| `check-preview-contract` / `check-tui-preflight-cost` | ok / ok (200x300 full replay 1.20 ms mean; budget 32) |
| `check-at-search-cost` / `check-usage-clock` | ok (every keystroke, 1460 searches: p50 0.28 ms, p95 2.01 ms) / ok |
| `test-check-no-secrets` / `test-gate-env` / `test-preview-contract` | ok (13 cases) / ok (66 cases) / ok (28 mutants) |
| `fxcap/test-sweep.sh` / `fxcap/test-capture.sh` | 79 + 28 OK / 134 OK |
| release build; `fault-injection` release build | ok; ok |
| `smoke.sh` / `smoke-tui.sh` | 47 checks, 0 failures / 37 scenarios + the oracle, 1167 checks, 0 failures |

The counts are `gate-counts.txt`'s (each binary's `test result` line, 15 a
run, and their sums).

**Every side's tests are in** (`test-counts.txt`: train 7's count, lines
holding `#[test]` under `src` and `tests`, and `tests_kept.py`, every test
function each side added, by file and name, looked up at the head): main
5766 (`753b0a72`, and `e2fc7e94`, whose PRs add none); each merge adds
exactly its side's count -- CATALOG-WAIT-CLOCK +2 (5600 against `afd891af`'s
5598), the decoder line +35 (5633), FULLTRANSCRIPT-RESPONSE-HEADER +12 (5631
against `3f2d6e78`'s 5619), the approval stack +29 (5497 against
`a37a969e`'s 5468), QUESTION-CTRL-J +8 (5606) -- and the join commit +2:
**5854 at the head, main + 88**, and **0 tests any side added missing** at
the head. `cargo test` 5942 against main's 5854 (train 14's join-main
checks at `42935f45`, whose code is main's: `../train-14/join-main/checks.txt`)
is the same +88, and with fault injection 5979 against 5891. The harness's
`def test_`: 241 at main `e2fc7e94` and at the head (#97 +4), the two
harness suites' 107 + 134.

**The first gate** (`gate-da8e4c1a.txt`), at `da8e4c1a` before
`bc9ea199` and the join of main, was stopped during its 17th step; its 16
steps were all exit 0 (5942 and 5979 passed, 0 failed).

## Sweeps

**Builds** (debug, as trains 4 to 14), each in its own `git clone
--no-local`, each swept with its own tree's harness, scenarios and ledger
(`sweep.sh`) -- the head `2c575f52` and **the base, main `e2fc7e94`** (the
dispatcher's: the join of main makes main the base the train is read
against, and both sweeps run #97's harness). fx: the v0.0.16 binary,
sha256 `8e6cda2e...` before and after each sweep, `FX_AUTO_UPGRADE=0`;
40x120, `--jobs 2`; each sweep inside one hold of the suite lock
(`sweep-runs.txt`: each sweep's lines, its `run.json`'s start, wall time,
loads and `repo_head`). **One selection, both sweeps**
(`selection-sweep.txt`; `selections.sh`, `selections.txt`: made again here,
the same names):
- the main selection (`selection-all.txt`, `selection.py`): the brief's
  fourteen globs (`composer*`, `p5-modified-*`, `p5-paragraph-*`,
  `p5-super-arrows*`, `p5-page-keys*`, `p5-input-*`, `fulltranscript*`,
  `p5-transcript*`, `approval*`, `p5-approval*`, `question*`,
  `p5-question*`, `cancel*`, `p5-cancel*`), every scenario named by a ledger
  entry the merges changed, every scenario a merge added, and the scenarios
  the two re-points' entries name; `*` (`session-record`, `stats-bytes`, and
  #96's three `status-panel-*` divergences) left out;
- the units' selection (`selection-units.txt`, `selection_units.py`): the 34
  scenarios the units' eleven committed head sweeps took that the first does
  not, so every scenario a unit swept is swept here.

At the base 115 scenarios of 411 are selected, at the head 136 of 432
(`selections.txt`); every one gives pairs. The base sweep was rescored with
the head's ledger (`rescore.sh`): the pass line the same before and after,
and **pair by pair 0 verdicts moved** (`sweep-runs.txt`; the ledger's
sha256 `a4a2b20e`, the head's). Every xfx snap that shows the banner names
the build swept: `2c575f5` on all 847 of the head's, `e2fc7e9` on all 638 of
the base's (`banners.py`). Loads at the sweeps' starts 58.51 (base) and
13.69 (head), peaks 97.63 and 33.45; walls 36m22s and 44m52s.

| sweep | scenarios | pairs | pass | failed | timeout |
|---|---|---|---|---|---|
| head `2c575f52` (`sweep-head.md`) | 136 | 977 | **681** (69.7%) | 0 | 0 |
| base `e2fc7e94` (`sweep-base.md`, rescored) | 115 | 735 | 494 (67.2%) | 0 | 0 |

The head's 21 scenarios more are the units' new ones, 242 pairs, 185 pass
(`new-scenarios.txt`), so 496 + 185 = 681.

### Against the base, pair by pair (`compare-base.txt`, `screen-moves-base.txt`, `screen-classes.txt`, `screen-partition.txt`, `gained-rows.txt`, `reruns.txt`)

**735 shared pairs: 494 -> 496; 2 up, 0 down, 4 moved within passing**
(`compare-base.txt`: "passed on the reference, not on the train: 0"):
- **2 up, FULLTRANSCRIPT-RESPONSE-HEADER's**: `p5-transcript-md` `full` and
  `p5-transcript-md-light` `full`, unresolved -> normalized-identical (its
  second Response record; the unit's README and its sweep).
- **4 moved within passing, identical -> normalized-identical**:
  `p5-transcript-md` `answered` and `closed`, `p5-transcript-submit-multi`
  `filled` and `second`. In each the one row that differs is the finished
  turn's summary, its elapsed seconds -- fx `1s` / xfx `2s` and fx `0s` / xfx
  `1s`, the token counts the same -- a wall-clock reading the ledger's
  normalization reads as `<wall>`; at the base the two sides read the same
  second. **Rerun 3 times on each tree** (`reruns.txt`, `rerun-job.sh`):
  `p5-transcript-md` `answered` and `closed` are identical in 2 of 3 at the
  head (xfx `1s`, `1s`, `2s` against fx's `1s`) and in 0 of 3 at the base
  (xfx `3s`, `2s`, `2s`) -- normalized-identical otherwise, passing 3 of 3
  on both; `p5-transcript-submit-multi` `filled` and `second` are identical
  3 of 3 on both trees. A turn's elapsed seconds, read on a shared machine:
  run-local, and if anything slower at the base, not the head. The reruns
  also hold the two ups: `p5-transcript-md` `full` normalized-identical 3 of
  3 at the head, unresolved 3 of 3 at the base.
- **`links` `full`, unresolved on both, its causes moved**
  (`scenarios.txt`, `gained-rows.txt`): at the base `session-record` and
  `full-transcript-shift`, at the head `session-record` alone, no row
  uncovered on either. FULLTRANSCRIPT-RESPONSE-HEADER writes the answer's
  prose after its table as a Response record of its own, as fx does: at the
  head xfx's page holds its records at fx's rows (Notice at row 4, Prompt 9,
  Responses 12 and 32, Usage 35, on both sides), one row lower than at the
  base, so the session record's top row leaves xfx's page as it leaves fx's.
  The one "new" differing row is that record's (fx `skills: loading` / xfx
  `+4 more)`), under `session-record`.
- `p5-transcript-question` `full-60` gained a differing row, unresolved on
  both: xfx's banner row holds its build id, as in train 14.

**The screens** (`screen-partition.txt` ties the counts together):
compare.py finds fx's `.ansi` moved in 114 shared snaps and xfx's in 217;
by class (`screen-classes.txt`), fx's 111 in clock-like or run-local text
alone, 1 beyond it, 2 in colour or attributes alone; xfx's 122, 99 and 0 (4
of them a `.txt` move whose `.ansi` did not). Of the moves beyond run-local
text, `screen-moves-base.txt` lists 100 lines: 93 a saved session's id
(`--resume` ids, `/status`'s Session row), and 7 others -- four
`fulltranscript-notice` snaps' `* version:` line (the build's own id,
`...-1018-e2fc7e94d66e` / `...-1059-2c575f52db5d`), and the three full
transcripts the Response split moved (`links`, `p5-transcript-md`,
`-md-light` `full`). **Rows gained** (`gained-rows.txt`): the two above.

### Join checks 1 and 4 in the sweeps (`scenarios.txt`)

- **J1** -- `approval*`: 18 pairs on both, 2 pass on both, every pair's
  verdict and causes the same at the head as at the base
  (`approval-grant-scope`, `tool-schema`, `amendment-card-after-stop`,
  `approval-scroll`), no row uncovered. `p5-approval*`: its 56 pairs the
  base took, unresolved on both, verdicts and causes the same; its 33 new
  ones (the stack's scenarios: `p5-approval-ctrl-j`, `-panel`,
  `p5-approval-draft-keys`, `p5-approval-readiness-settled`) 2
  normalized-identical, 3 approved divergences, 28 unresolved under
  `approval-grant-scope`, `tool-schema` and the stack's own
  `approval-settled-every-approval` (`p5-approval-readiness-settled`
  `cmd2-down-enter`, the dispatcher's readiness decision), no row
  uncovered. `p5-super-arrows-approval`: 16 pairs, new, each unresolved under
  `approval-grant-scope`, and in `amend-super-right` one row no cause
  covers: the caret cell COMPOSER-SUPER-ARROWS records as a capture artifact,
  not a divergence (`docs/parity.md`'s full-screen TUI row; its receipt).
- **J4** -- `full-transcript-shift` names 18 pairs at the base and 17 at the
  head: the same, less `links` `full` (above). `session-record-page-bottom`
  names 2 at the base and 3 at the head: `p5-transcript-md` and `-md-light`
  `full-pgup` on both, and `p5-modified-page-keys` `full-pgup`, new at the
  head (MODIFIED-PAGE-KEYS' scenario, which the cause names). The unit's
  own `fulltranscript-response-split`: 14 pairs, 14 pass (new).

### Against each unit's own sweep (`compare-units.txt`)

Every unit's committed head sweep (eleven; CATALOG-WAIT-CLOCK and
TILDE-EMPTY-MODIFIER committed none) against the head's, scenario by
scenario (`unit_compare.py board`):

| unit (its sweep) | shared scenarios | pass, unit -> head | lower at the head |
|---|---|---|---|
| DECODER-TILDE-HOME-END | 7 | 45 -> 45 | 0 |
| MODIFIED-PAGE-KEYS (round 2) | 3 | 11 -> 17 | 0 |
| MODIFIED-ARROWS-SCREENS | 16 | 119 -> 128 | **1**: `fulltranscript-notice` 1 -> 0 |
| COMPOSER-PARAGRAPH-MOVE | 56 | 333 -> 349 | **1**: `fulltranscript-notice` 1 -> 0 |
| COMPOSER-SUPER-ARROWS | 47 | 286 -> 302 | **1**: `fulltranscript-notice` 1 -> 0 |
| FULLTRANSCRIPT-RESPONSE-HEADER (head2) | 32 | 116 -> 115 | **1**: `fulltranscript-notice` 1 -> 0 |
| APPROVAL-READINESS-SETTLED | 9 | 21 -> 21 | 0 |
| APPROVAL-DRAFT-KEYS | 8 | 3 -> 3 | 0 |
| APPROVAL-CTRL-J | 10 | 4 -> 4 | 0 |
| RULE-CONFIRM-KEYS | 30 | 110 -> 110 | 0 |
| QUESTION-CTRL-J | 11 | 77 -> 77 | 0 |

**`fulltranscript-notice` `end`** passes on the four decoder-line units'
sweeps (built on train 13's head, xfx `0.1.0`) and is unresolved at the head
**and at the base** (`scenarios.txt`: 0 of 4 on both): the `* version:`
line, xfx's `0.2.0 (debug <date>-<rev>)` against fx's `0.0.16`, train 14's
reading ("Against each unit's own sweep") -- main's, not the train's.

**So no pair that passes on the base fails at the head**, and against the
units' own sweeps the one lower scenario is lower at the base too.

## Decisions, for the user's veto

Each with its revert.

- **CATALOG-WAIT-CLOCK's test E and its edits of it are dropped** (merge 1):
  main deleted E and the ledger test with the history notice they assert;
  D keeps main's name and body on the unit's harness. **Revert**: none that
  builds -- E asserts a notice the history no longer holds.
- **One `offer_to_pickers(&menus)`** (merge 2): CTRLO-OTHER-PICKERS' one
  call, given the decoder line's `menus`, in place of the line's twelve
  calls. **Revert**: the twelve calls inline, each on `&menus`.
- **The rule confirmation reads the event as it came** (merge 4):
  RULE-CONFIRM-KEYS' `spelled` is the decoder line's `event` since
  `unspelled` went; the fold's comment loses the clause that put the rule
  confirmation below the fold. **Revert**: none -- `unspelled` is gone.
- **The approval's C-j counts for `/mcp`'s held rule** (`0341dd2b`,
  QUESTION-CTRL-J's review MUST): a C-j answers a permission question as
  Enter does, so the Enter after it waits. **Revert**: drop `||
  self.asking()` from the C-j arm (M1 in `mutants.txt`: the new test then
  fails).
- **The rule confirmation's C-j is not counted**, by the reasoning in join
  check 2 (the two are never up together), not by a test. **Revert**: none
  needed; to count it, add `|| self.rule_confirm.is_some()` and a case.
- **The owner clauses retired** (`0341dd2b`): the draft's keys are fx's but
  for a paste's line breaks; "(QUESTION-CTRL-J, proposed)" points at its row.
  **Revert**: `git revert 0341dd2b` takes the code with it; the doc lines
  alone are its `docs/parity.md` hunks.
- **One sentence of the full-screen TUI row reworded** (`bc9ea199`; the token
  check's difference 1). **Revert**: `git revert bc9ea199` -- "So do" then
  reads as "take neither too" again.
- **Every WRONG reference re-pointed**, the merges' 32 and the three #96
  moved on main (`da8e4c1a`, `2c575f52`); the two train 14 left, left.
  **Revert**: `git revert 2c575f52 da8e4c1a`.
- **Main joined before the gate and the sweeps** (`a14df710`), at the
  dispatcher's word, so both run once, on the final tree, and the sweeps run
  #97's harness. **Revert**: the train without `a14df710` and `2c575f52`;
  the gate and the sweeps would then have to be run again on that tree.
- **One sweep a tree over one selection** (`selection-sweep.txt`: the main
  selection and the units' selection in one file), where train 14 swept the
  two apart. **Revert**: sweep the two files apart.
- **`check-merge.sh` without the suite lock**, as trains 13 and 14.
  **Revert**: run it under the lock.
- **Two join tests** (`0341dd2b`), where no unit's tests reach the meeting;
  every other meeting is read through the units' own tests.
  **Revert**: drop them; the C-j guard then has no case.

## Found on the way, not changed here

- **`da8e4c1a`'s message says "32 + 2 = 34 lines out and in"**; it is 32
  lines, holding 34 references (ledger :506 and :882 hold two each;
  `repointed.txt`). History is not rewritten.
- **The two references train 14 left WRONG on purpose are WRONG still**
  (`content-head.txt`): `src/tools/question.rs:303`'s `Cargo.toml:42` and
  `tests/session_compat.rs:57`'s `src/session/event.rs:98-192`. Both are
  TRAIN14-FOLLOWUPS' (src after the code head), with that queue's other
  three (the helper's mark assert after C-j when held,
  `shell.rs:24979-24980`'s pair, `tests/tui.rs:2600`'s list).
- **QUESTION-CTRL-J's review SHOULD 2**: one table of the Ctrl+J spellings
  for the two questions (`CTRL_J`, 11, and `MODEL_QUESTION_CTRL_J`, 13, in
  `src/tui/shell.rs`'s tests). A follow-up; tests only.
- **The mutant clone reads "tree clean after: no"** (`mutants.txt`): each
  mutant's log is written into the clone's root, untracked; `shell.rs` is
  put back after each (`git checkout`), which the unmutated run before them
  and each mutant's own result show.

## Redaction

The repository is public. Every file here taken from a gate log, a job, the
logger, a sweep, a capture or a clone went through `redacted.py` (train
14's, unchanged but for its first line): `scripts/fxcap/redact.py`'s
`Redactor.write`, every path under a local root to `<local-path>` or a named
root (`<scratch>`, `<repo>`, `<sweep-head>`, `<sweep-base>`, ...), a
checkout's path cut short to `<checkout>/`, the account's name to `<user>`,
and train 12's tail rule. The scripts take the oracle's path, the suite lock
and the scratch directories as arguments or variables (`mutants.sh` read its
TMPDIR from a line of its own as run; the copy here reads it from the
environment, the one change), and spell the brief's pattern with bracket
classes. **Every commit this train adds was gated on the brief's check**
before it was made (`pathcheck.sh`, under `set -o pipefail` and an explicit
`if`): a merge as the combined diff `git show` prints of it, read from a
probe commit of the staged tree; any other commit as its staged lines; each
commit's message too; and `shapes.py`'s three shapes over the staged lines
and over every file of this folder. `final-scan.sh` reads every commit and
the range at the end (below), and the file checks run with the receipt in
its final form (`gates-files.sh`, `gates-files.txt`).

## Files

- Merges: `rowsmerge.py` (`--meet`, `--inside` new), `blocks.py` (`tail`
  new), `byhand.py`, `reproduce.sh`, `resolutions.txt`, `check-merge.sh` /
  `check-merges.txt`, `merge_ends.py` / `merge-ends.txt`, `remerge-1.txt` ..
  `remerge-6.txt`, `overlap.py` / `overlap.txt`, `merge_keep.py` /
  `merge-keep.txt`, `phrases.sh` / `phrases.txt`; `pathcheck.sh`,
  `shapes.py`, `carried.py` (the brief's check before every commit).
- `merge-checks.sh`: writes `check-merges.txt`, `resolutions.txt`,
  `merge-ends.txt`, the token check and its controls, `merge-keep.txt`, the
  remerge-diffs, the reference scans, the content checks, the two re-point
  plans and their check, `shared.txt`, `functions.txt`, `moved-edits.txt`,
  `overlap.txt`, `test-counts.txt`.
- `token_check.py` (train 7's, byte for byte), `token_control.py`,
  `word_control.py`, `token-check.txt`, `token-self-test.txt`.
- References: `refs.py`, `refs_all.py`, `refs_short.py`, `refs_bare.py`,
  `refs_prd.py`; `refs-*-pre.txt` (at `0341dd2b`), `refs-*-head.txt` (at
  `2c575f52`); `refs_content.py`, `content.sh` and `content-*.txt`;
  `repoint.py`, `repoint-plan.txt`, `repoint-plan-2.txt`, `repointed.py`,
  `repointed.txt`.
- Join checks: `functions.py` / `functions.txt`, `shared.py` /
  `shared.txt`, `moved_edits.py` / `moved-edits.txt`; `tests_kept.py` (in
  `test-counts.txt`), `unit_tests.py` / `unit-tests.txt`; `mutants.sh` /
  `mutants.txt`; `scenarios.sh`, `scenarios.py` / `scenarios.txt`;
  `unit-tests.sh` / `unit-tests.txt`; `jobs.sh` (parts `cw` and `net`),
  `cw_runs.py`, `jobs-cw.txt`, `cw-runs.txt`, `net.py`, `jobs-net.txt`,
  `connect.log`.
- Gate: `gates.sh`, `gates_txt.py`, `gate-files.sh`, `gates.txt`,
  `gate-da8e4c1a.txt`, `gate_counts.sh`, `gate-counts.txt`;
  `test_counts.sh`, `test-counts.txt`; `gates-files.sh`, `gates-files.txt`
  (the file checks and the reference checks at the receipt's
  second-to-last commit).
- Sweeps: `selection.py`, `selection_units.py`, `selections.sh`,
  `selection-all.txt`, `selection-units.txt`, `selection-sweep.txt`,
  `selections.txt`; `sweep.sh`, `rescore.sh`, `sweep-runs.sh` /
  `sweep-runs.txt`, `sweep-*.md`; `sweep-compare.sh`, `compare.py`,
  `screen_moves.py`, `screen_classes.py`, `screen_partition.py`,
  `gained_rows.py`, `new_scenarios.py`, `banners.py`, `unit_compare.py`,
  `compare-base.txt`, `screen-moves-base.txt`, `screen-classes.txt`,
  `screen-partition.txt`, `gained-rows.txt`, `new-scenarios.txt`,
  `compare-units.txt`; `rerun-job.sh`, `sweep-rerun.sh`, `reruns.sh`,
  `rerun_verdicts.py`, `reruns_txt.sh`, `reruns.txt`.
- `final-scan.sh`, `midbranch_scan.py`, `final-scan.txt`; `redacted.py`.
