---
title: 0.0.16 delta -- PASTE-GAP round 7 receipt (only the paste sentences are given up whole; the refusals put the condition before the key)
type: receipt
tags: [xfx, tui, phase5, input, paste, security]
---

# PASTE-GAP round 7

Round 6 closed at `a49039f7`. Both engines approved with fixes, and the
structural reconcile stands. gpt6's MUST, confirmed by the dispatcher,
rejects round 6's height trade. Opus's three SHOULDs are the third row's
cuts, the untested interrupted-burst leg, and the docs. Every figure below
rests on a file in this directory. Every `file:line` is at the code head.

| commit | what |
|---|---|
| `4ebd9004` | RED, tests only, on `a49039f7`'s source (`red.txt`) |
| `2b2f2bac` | GREEN. The oracle check and the mutants ran here |
| `ae695f40` | the receipt, first written |
| `a4990f13` | **The code head**: the whole-or-absent test also runs at 30 columns, and `STATUS_ROWS` names the width cuts (a test and a doc comment). The gate ran here |
| the commit after | this receipt, brought to `a4990f13`; receipt files only |

The oracle and the mutants were not rerun at `a4990f13`. Its only changes
are one test's width loop and one doc comment. The oracle is a different
test. That test was already mutant 2's killer at 20 columns, and it still
checks those 20-column cases.

**Size.** Round 7 adds 87 non-test source lines, 55 of them comments and
32 code (`../round6/nontest.py a49039f7 a4990f13`). The unit's total is
**638**, past the brief's ~600. The dispatcher decided not to split for
this round.

## 1. Only the paste sentences are given up whole (gpt6 MUST)

How a short screen gives up a status is now the caller's to say:
- `ApprovalScreen::set_status` (`src/tui/approval_screen.rs:235`) gives it
  up from its last row, as round 5 did;
- `set_whole_status` (`:246`) gives it up whole or not at all.

The `Status` struct (`:71`) carries `whole`. The drop loop takes the block
in one step only for a whole status (`:427`, `:432`).

`Shell::said_after_paste` (`src/tui/shell.rs:12278`, `:12280`) is the one
caller of `set_whole_status`. It says fx's `PASTE_SUFFIX` and the three
quarantine sentences, each of which says something else when cut. Every
other status is cut from its last row, so it keeps its first clause while
there is room: the readiness refusals and the amendment's limit.

**RED** (`red.txt`, on `a49039f7`'s source).
`a_refused_grant_on_a_short_review_plane_still_says_why_where_round_5_did`
(`shell.rs:39063`) presses `1` at each height from B to B+4. B is the plane's
chrome with no status and no blank, measured by `plane_chrome_rows`. At each
height it asserts:
- nothing is granted;
- all three choices are on the plane;
- nothing is said in the document;
- the refusal shows at least the rows round 5's cut showed: two status rows
  given up from the last, after the blank above them, so
  `min(2, H - B - 1)`. Its shortfalls are collected before the assert, so
  one height cannot hide another.

On `a49039f7` it **fails at 9 heights**: every one is B+2 or B+3, at 20, 30
and 63 columns, showing 0 rows where round 5 showed 1 or 2.

**The widths, a measured change from the brief.** The brief asks for both
refusals at 20 and 30 columns. The plane cannot grant a file review below
63 columns: its choices are not whole there (`plane_grants_from`,
`shell.rs:39052`: every fixture is undisclosed at 62 columns and whole at
63). And the shell composes the question when it is installed
(`shell.rs:5156`). So at 20 and 30 columns a `1` is refused as *never
whole*, before the frame and after it alike, and "not yet" cannot be
reached there. The test therefore presses:
- at 20 and 30 columns, before the frame and after it: APPROVAL_NOT_DISCLOSED;
- at the narrowest grantable width (63, measured by the test) and at 80,
  before the frame: APPROVAL_NOT_READY.

**The oracle is round 5 itself** (`oracle.txt`). This test, alone, inserted
into a `git clone --no-local` at `f9328d4e`, **passes there** (1 passed, 0 failed). So
its "rows round 5 showed" is round 5's behaviour, not a formula of mine.

**The other half**: `fx_s_warning_and_the_quarantine_s_sentences_are_whole_or_absent_on_a_short_plane`
(`shell.rs:39133`). It puts each of the four sentences on the plane through
its real path, at 20 and 30 columns from B to B+4. Each is whole or absent, and
each is on the plane at some height. It is green on `a49039f7` too (round 6
gave everything up whole); mutant 2 is its kill. The screen-level tests
split the same way:
- `a_status_given_up_whole_is_on_the_screen_whole_or_not_at_all_at_every_height`
  (`approval_screen.rs:1042`);
- `any_other_status_is_given_up_from_its_last_row_so_its_first_clause_stays`
  (`:1071`): every answer is on the screen at heights 7 to 40, and the
  refusal is cut at some height.

## 2. The third row's cuts (opus SHOULD 1)

`no_cut_of_xfx_s_refusals_shows_an_instruction_without_its_condition`
(`approval_screen.rs:1116`) runs every width from 20 to 80. At each it
takes every row count the screen can give the status, which is every prefix
of `status_rows`: one wrap shared with compose (`:85`). The parts that must
never show alone are named in the test:
- "press it" without "once you can read it";
- "press 3" and "ctrl+c" without "to refuse".

Both refusals' cuts are counted before either is asserted. On `a49039f7`
(`red.txt`):
- APPROVAL_NOT_READY: **47 cuts**, 8 of them at three rows ("... press it"
  at 20 columns, the dispatcher's hand wrap);
- APPROVAL_NOT_DISCLOSED: **29 cuts**, 9 of them at three rows ("... or
  press 3 or" from 41 columns).

Opus and the brief named only the first. This round's first draft of the
test asserted NOT_READY first and stopped there, so my first reading said the
second had no cut. Counting both before asserting is the fix in the test.

Both sentences are xfx's (fx has no readiness gate), so they are reworded
condition first, and their heads are unchanged. smoke-tui pins "the question
is not on the screen yet", and the 30-column test pins "this screen cannot
show the whole request":
- `APPROVAL_NOT_READY` (`shell.rs:502`): "the question is not on the screen
  yet; once you can read it, press it again".
- `APPROVAL_NOT_DISCLOSED` (`:514`): "…; make the window larger, or, to
  refuse it, press 3 or ctrl+c".

**fx's amendment limit keeps its words.** "That edit exceeds the input
limit and was not applied." is fx's (`input_limit_feedback.zig:40
@481104fd`). Its cuts that end on "was not" are pinned **exactly**:
`fx_s_amendment_limit_is_cut_only_where_parity_names_it` and
`FX_LIMIT_CUTS` (`approval_screen.rs:1147`, `:1164`). There are 14:
- one row, on a short screen, at 47 to 55 columns;
- two rows at 25 to 28 columns;
- three rows at 20 columns, where it is wider than its three.

parity.md names them. The draft it refused is on the plane beside it,
unchanged.

## 3. The interrupted-burst leg (opus SHOULD 2)

`the_plane_s_sentence_goes_in_a_turn_whose_burst_ended_with_neither_check`
(`shell.rs:39191`) does the following:
- puts "Key sequence open" on the plane;
- `route_read(b"x")` breaks the held marker;
- the turn ends with `settle_input` alone, neither `end_delivery_at` nor
  `still_at`, as a burst ends at `event_loop.rs:668` (an interrupted wait)
  and `:687` (an interrupted or would-block read).

The sentence is gone, nothing new is said, and a frame was requested. It is
green on `a49039f7`. Mutant 3 reconciles at the end of the two checks and
not in `settle_input`; this test is its only killer.

## 4. Docs (opus SHOULD 3)

- `STATUS_ROWS` (`approval_screen.rs:67`) names who takes the third row:
  - fx's paste warning at 20-29 columns;
  - fx's amendment limit at 20-28;
  - APPROVAL_NOT_READY at 20-39;
  - APPROVAL_NOT_DISCLOSED at 20-68.

  The quarantine sentences never take it. Wider than three rows, and so
  cut to them: the limit at 20, NOT_READY at 20-27, NOT_DISCLOSED at
  20-46. The widths were measured by a probe over `safe_rows`, then
  removed. The doc also says the third row
  costs no answer, but the review's viewport is one row shorter while such
  a status fits.
- The removal loop's comment: refusals are given up from their last row,
  and only a whole status goes in one step.
- `APPROVAL_NOT_READY`'s doc (`shell.rs:488`). "Said rather than swallowed"
  now says the line is cut and its condition comes first. It also names the
  case where the plane has no row: less than two rows above the chrome, the
  key does nothing visible.
- parity.md:237:
  - the four paste sentences are given up whole; every other status is cut
    from its last row;
  - both refusals put the condition first;
  - the no-row case is named;
  - fx's amendment-limit cut is named by size.

## 5. Mutants (`mutants.py`, `mutants.txt`)

Each mutant ran on a throwaway `git clone --no-local` at `2b2f2bac`, never on
the worktree, with `GIT_DIR`, `GIT_INDEX_FILE` and `GIT_WORK_TREE` unset.
- Each made one change, then ran `cargo test --locked --lib tui::` and the
  four pty paste tests, each with a 900 s limit. The change was undone
  after, and the clone removed.
- They ran under the suite lock.
- **5 of 5 killed**, and every killer is named in `mutants.txt`.

| # | the rule reverted | killed by |
|---|---|---|
| 1 | **item 1: every status given up whole (round 6's rule)** | 2: the refusal-height test, and `any_other_status_is_given_up_from_its_last_row_…` |
| 2 | **item 1: the paste sentences cut from their last row** | 1: `fx_s_warning_and_the_quarantine_s_sentences_are_whole_or_absent_on_a_short_plane` |
| 3 | **item 3: reconciled at the end of the two checks, not at the turn's end** | 1: the interrupted-burst test |
| 4 | item 2: the not-yet refusal's round 6 words | 1: the cut test |
| 5 | item 2: the never-whole refusal's round 6 words | 1: the cut test |

Mutant 3 is the one the brief says survives today, and it did survive
before this round: every round-6 test called `end_delivery_at` or `still_at`
before `settle_input`. The interrupted-burst test is its only killer.

## 6. The sweep: not run

A cell can change only where a capture holds one of these:
- a status on the review plane (the drop rule);
- one of the two readiness refusals, on the plane or in the document (the
  words).

fx's amendment-limit sentence keeps its words and its path. Over PASTE-GAP
round 1's head sweep of the same 41 input scenarios (`../sweep-head.md`)
(`sweep-reason.txt`):
- **0 of 254 xfx captures** has a plane status row;
- **0** has either refusal anywhere;
- fx's limit sentence is on 9 rows, all in the document of
  `approvals-amend-limit`, unchanged.

The patterns are shown able to match: 20 indented rows, 2 of 2 samples at
the indent, 1 of 1 in the document. The scenario files are unchanged since
`a832f4d7` (0 diff lines).

## 7. Gate (`gates.txt`)

Every CI step of `ci.yml` ran in order at **`a4990f13`**, under the suite
lock (one slot for the run). **All 17 exit 0** (`gates.txt`).

| step | result |
|---|---|
| fmt, clippy, fault-injection clippy | exit 0 |
| `cargo test --locked --all-targets` | 14 binaries, **5504 passed**, 0 failed (round 6's own gate had 5498; this round adds 6 tests). The first run, at `2b2f2bac`, gave the same counts with all 17 exit 0 (`gates-2b2f2bac.txt`) |
| with the fault injector | 15 binaries, **5542 passed**, 0 failed (+6) |
| no-stubs, no-secrets, gate-env (62 of 62), identity, preview contract and its mutants, preflight cost, usage clock | exit 0 |
| release and faulty builds, `smoke.sh` | exit 0 |
| `smoke-tui.sh` | 37 scenarios + the oracle, **1167 checks, 0 failures**. It pins the not-yet refusal's head, which round 7 keeps |

`green-unit.txt` comes from the gate's `test` log. It holds round 7's six
new tests and round 6's plane tests, the plane's status tests, and the
readiness refusal tests: all `ok`. The four pty paste tests are `ok` in the
same log.
