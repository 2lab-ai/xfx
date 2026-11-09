---
title: 0.0.16 delta -- PASTE-GAP round 5 receipt (the plane's sentence leaves with the quarantine; every sentence whole at 20 columns)
type: receipt
tags: [xfx, tui, phase5, input, paste, security]
---

# PASTE-GAP round 5

Round 4 closed at `4bc343b4`. opus found one MUST, which the dispatcher
confirmed in the code: the present-tense sentence stayed on the review plane
after the quarantine ended. gpt6 found none. Every figure below rests on a
file in this directory or the one named.

| commit | what |
|---|---|
| `7c5b1fa8` | RED, test only, on `4bc343b4`'s source: the sentences cut at 20 columns (`red-width.txt`) |
| `e05f62f8` | GREEN for that: three short sentences; the `Delivery::Ignored` and module docs (item 5) |
| `51ae872c` | RED, test only, on `e05f62f8`'s source: the sentence outlives the quarantine on the plane (`red-clear.txt`) |
| `1cbdf567` | GREEN for that: `clear_status_if`, called when a check finds the quarantine over |
| `bed4a4bd` | tests only: the rendering at 120, 80 and 20 columns (item 3), and the flapping hold (item 4); names that spelled the old sentences renamed |
| `6cb46583` | tests only: `bed4a4bd` was committed with a clippy failure in one assertion, fixed here. **The code head**: the mutants and the gate ran here (banner `v0.1.0-6cb4658`) |
| the commits after | this receipt; receipt files only |

Round 5 adds 66 non-test source lines; the unit's total is 497.

## 1. The sentence leaves the review plane when the quarantine ends (opus MUST)

When a check finds the quarantine over -- by the quiet, or because a paste
began inside it -- `Shell::end_delivery_at` now takes the plane's status row
down (`src/tui/shell.rs:10423`, `ApprovalScreen::clear_status_if`,
`src/tui/approval_screen.rs:202`). It does so only if that status is one of
the three quarantine sentences (`QUARANTINE_SENTENCES`, `shell.rs:577`), then
asks for a frame.

Left alone:
- fx's own warning about input after a paste, and every other status: each
  is about something that happened, and is as true now as when it was said;
- the document's rows, which are a record of what was said when.

RED, `red-clear.txt` (`51ae872c`, on `e05f62f8`'s source), from
`the_quarantine_s_sentence_leaves_the_review_plane_when_the_quarantine_ends`
(`shell.rs:38545`). For each of the three holds, in order:
1. the sentence is asserted on the plane;
2. the quarantine is ended: by the quiet, or by `ESC[201~` (the open paste)
   or `x` (the open sequence) and then the quiet;
3. it is asserted over;
4. the test fails: `"Keys ignored until input stops." outlived the
   quarantine`.

GREEN: all three sentences are gone, and a `1` answers the question. The
test's fixture marks the plane painted (`delivered`) so that last key can
answer.

## 2. Every sentence whole at 20 columns (opus SHOULD 1)

The review plane gives its status two rows (`STATUS_ROWS`,
`approval_screen.rs:49`). A session exists down to `MIN_COLS` = 20 columns
(`layout.rs:38`; the plane presents its choices there,
`approval_screen.rs:578`). Two columns are the indent, so each sentence must
wrap into two rows of at most 18 columns.

RED, `red-width.txt` (`7c5b1fa8`, on `4bc343b4`'s source): at 20 columns
the plane painted `Input is still being ignored` and then its rule.

The three sentences now (`shell.rs:561`, `:568`, `:573`):

| hold | sentence |
|---|---|
| the quiet | `Keys ignored until input stops.` |
| an open dropped paste | `Paste still open; keys are ignored.` |
| a marker read past its first byte | `Key sequence open; keys are ignored.` |

The two framing sentences name the cause and no release point. They stay true
however many keys continue the sequence and however the open paste ends
(round 4's rule). The quiet's sentence states its release, which is that
state's only release. Pinned:
`every_quarantine_sentence_is_whole_on_the_review_plane_at_the_narrowest_screen`
(`shell.rs:38522`), each sentence said in its own state on the plane at
`MIN_COLS`.

## 3. Rendering at ordinary widths (gpt6 SHOULD 1, opus SHOULD 3)

`the_quarantine_sentences_render_whole_at_120_80_and_20_columns`
(`shell.rs:38612`) checks each of the three, said in its own state.

In the document:
- at 120 and 80 columns: exactly one row, `! input: ` and the sentence;
- at 20 columns: more than one row, the first carrying the topic and the
  rest under the hanging indent, none wider than the screen, and every word
  there in order.

On the review plane: the sentence is whole at all three widths.

The 160-column tests of round 4 still stand beside it.

## 4. A flapping hold (gpt6 SHOULD 2)

`a_hold_that_flaps_is_said_at_most_once_a_second_and_its_last_state_is_said`
(`shell.rs:38662`):
- `ESC [` and `x` alternate every turn for 450 turns (3.6 s), so the hold
  flips between the open sequence and the quiet at each check;
- then `ESC [` and 3 s of checks;
- every pair of sentences after the first warning is at least
  `PASTE_REPEAT` apart, and at least four are said;
- the quarantine still holds, and the last sentence said is the open
  sequence's.

Items 3 and 4 pin behaviour already in place; they are not RED.

## 5. Stale docs (opus SHOULD 2)

- `Delivery::Ignored` (`input.rs:485`): `again` comes when input kept
  arriving, **or with `dropped` 0** when the hold has changed since a repeat
  named it.
- The module doc: "Once a repeat has named a hold", the three sentences'
  shape, and the plane's status taken down when the quarantine ends.
- parity.md:237: the three sentences in full, their two-row fit at 20
  columns, and the plane's status taken down.

## 6. RED -> GREEN

- RED: `red-width.txt` and `red-clear.txt`, as above.
- `green-unit.txt` (`6cb46583`): round 5's four tests, round 4's sentence
  and hold cases, and every review-plane case, 16 `ok`.

## 7. Mutants (`mutants.py`, `mutants.txt`)

Each mutant ran on a throwaway `git clone --no-local` at `6cb46583`, never on
the worktree, with `GIT_DIR`, `GIT_INDEX_FILE` and `GIT_WORK_TREE` unset. Each
made one textual change, then ran `cargo test --locked --lib tui::` and the
four pty paste tests, each in its own process group with a 900 s limit; the
change was undone after. They ran under the suite lock. **8 of 8 killed**,
and every failing test is named in `mutants.txt`.

| # | the rule reverted | killed by |
|---|---|---|
| 1 | **item 1: the quarantine's sentence is never taken down** | the clear test (and a worker timing test: load, below) |
| 2 | item 1: only the quiet's sentence is taken down | the clear test |
| 3 | item 1: every status is taken down, fx's warning included | 2, the round-1 review-plane cases that read fx's warning after the quarantine ends |
| 4 | **item 2: the quiet's sentence is round 4's** | 5, the 20-column test and the rendering test among them |
| 5 | **item 2: the open paste's sentence is round 4's** | 3, the same two among them |
| 6 | **item 2: the open sequence's sentence is round 4's** | 5, the same two among them |
| 7 | round 4: a hold changed by held bytes alone is never owed | 1 |
| 8 | round 4: the hold is recorded at the first repeat only | 1 |

Mutant 1's second killer,
`tui::worker::tests::a_stop_in_a_turns_catalog_wait_ends_it_at_once_and_the_next_start_keeps_its_number`,
times a catalog wait (`src/tui/worker.rs`) and shares no code with the plane's
status. Run alone on the unmutated head it passes 3 of 3 at load average 34
to 44 (`worker-timing.txt`). So its kill is load, not detection, and the
clear test is mutant 1's real killer.

## 8. The sweep: not run

No cell any of the 41 input scenarios reaches can change:
- the three sentences are said only as repeats, which need input to keep
  arriving for a second after a paste;
- the plane's status is taken down only when it is one of them;
- no scenario types straight after a paste (a `wait` always comes between,
  `../README.md` §8), and none streams after one.

## 9. Gate (`gates.txt`)

Every CI step of `ci.yml` ran in order at **`6cb46583`**, under the suite
lock. **All 17 exit 0** (`gates.txt`).

| step | result |
|---|---|
| fmt, clippy, fault-injection clippy | exit 0 |
| `cargo test --locked --all-targets` | 14 binaries, **5489 passed**, 0 failed |
| with the fault injector | 15 binaries, **5527 passed**, 0 failed |
| no-stubs, no-secrets, gate-env, identity, preview contract and its mutants, preflight cost, usage clock | exit 0 |
| release and faulty builds, `smoke.sh` | exit 0 |
| `smoke-tui.sh` | 37 scenarios + the oracle, **1167 checks, 0 failures**; scenarios 7 and 21 (the paste ones) `ok` |

Round 5's tests, every review-plane case and the unit's pty paste cases are
all `ok` in the `test` log.

## parity.md

The `full-screen TUI` paste paragraph: the three sentences in full, their
two-row fit at 20 columns, and the plane's status taken down when the
quarantine ends (fx's warning and other statuses stay).
