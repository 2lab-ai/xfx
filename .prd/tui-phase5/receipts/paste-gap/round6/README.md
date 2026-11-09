---
title: 0.0.16 delta -- PASTE-GAP round 6 receipt (the plane's quarantine line follows the decoder at the end of every turn)
type: receipt
tags: [xfx, tui, phase5, input, paste, security]
---

# PASTE-GAP round 6

Round 5 closed at `f9328d4e`. gpt6 rejected it with two MUSTs; opus approved
it with five SHOULDs. Rounds 3, 4 and 5 each found one more event after
which the review plane's present-tense sentence outlived the state it named,
and each fix added one more event that cleared it. This round removes the
per-event clears: one reconcile, at one point. Every figure below rests on a
file in this directory or the one named.

| commit | what |
|---|---|
| `3c0c5fa9` | RED, tests only, on `f9328d4e`'s source (`red.txt`) |
| `d0789285` | GREEN. **The code head**: the mutants and the gate ran here |
| the commit after | this receipt, and two doc comments that still said round 5's rule: `input.rs`'s module doc ("takes down when the quarantine ends") and `clear_status_if`'s ("a state that has ended"). Comment text only; fmt and clippy green on it |

Round 6 adds 90 non-test source lines, most of them doc comments. The unit's
total is 562 (the counter, which gives round 5's 497 at `6cb46583`, is
in section 7).

## 1. The plane's line follows the decoder, at one point (gpt6 MUSTs 1-2, opus SHOULD 1)

**Option (b): one reconcile step at the end of every turn.** I did not pick
(a), computing the line when the frame is built. `ApprovalScreen::compose`
is reached from the readiness gate, the disclosure and the rows, and none of
them has the decoder. (a) would also still need a repaint whenever the hold
changes, and that is a reconcile under another name.

**What is true now** is the decoder's to say: `Decoder::named_hold`
(`src/tui/input.rs:639`). It gives the hold that the last repeat *in this
quarantine* named (`Quarantine.said_hold`), while that hold still holds it.
Otherwise it gives `None`:
- when there is no quarantine;
- before any repeat in this quarantine. A new `Quarantine` starts with
  `said_hold: None`, so a sentence said in the old one names nothing;
- once the hold has changed.

**The step:** `Shell::plane_names_the_hold` (`src/tui/shell.rs:10463`).
- If the plane's status is one of the three sentences and is not the
  sentence for `named_hold`, it is taken down (`clear_status_if`) and a
  frame is requested (`Reason::Modal`).
- Nothing new is said. The notices stay rate-limited; the plane's line is
  not rate-limited.
- fx's warning, every other status and the document's rows are left alone.
- The sentence for each hold is one function now, `quarantine_sentence`
  (`shell.rs:586`). It serves both what is said and what the plane may keep
  showing.

**Where it runs:** once, at the tail of `Shell::settle_input`
(`shell.rs:10511`). Round 5's clear in `end_delivery_at` is deleted.
- The loop's turn is `read_burst`, then `settle_input`, then the UI events,
  then `settle_band`, then the frame (`src/tui/event_loop.rs:275`, `:282`,
  `:297`). So `settle_input` runs after the burst however the burst ended:
  - a check that found nothing;
  - the read cap (`still_at`);
  - an interrupted wait or read, which calls neither of those;
  - and before every frame.
- The shell touches the decoder at five places
  (`grep -n 'self\.decoder\.' src/tui/shell.rs`, non-test):
  - `route_read` `:10397`;
  - `end_delivery_at` `:10422`;
  - `still_at` `:10438`;
  - the step's own read `:10464`;
  - `settle_input` `:10509`.

  The UI events and `settle_band` do not touch it.
- At exit, `shut_down` gives the plane back (`release_screen`,
  `event_loop.rs:470`) before its drain paints, and reads no input. So no
  frame of the plane is painted after the last `settle_input`.

**The five transitions** (RED on `f9328d4e`'s source in `red.txt`; GREEN in
the gate's `test` log). Each test ends its turn as the loop does
(`settle_input`) and asserts five things:
- the sentence is gone from the plane;
- fx's warning has not come back;
- nothing was said in the document;
- a frame was requested;
- the decoder is in the state that makes it this transition.

| transition | test (`the_plane_s_sentence_goes_when_…`) | on `f9328d4e` |
|---|---|---|
| Marker -> Quiet: `x` inside `PASTE_REPEAT` | `…a_breaking_byte_ends_the_open_sequence` | FAILED: "Key sequence open" still on the plane |
| an open dropped Paste -> Quiet: its end marker | `…the_dropped_paste_s_end_marker_arrives` | FAILED: "Paste still open" still on the plane |
| a whole paste replaces the quarantine: `route_read(b"\x1b[200~x\x1b[201~")` (opus SHOULD 1) | `…a_whole_paste_replaces_the_quarantine` | FAILED: "Keys ignored" still on the plane |
| the read cap, with a paste begun and not ended | `…a_paste_begins_in_a_turn_that_ends_at_the_read_cap` | FAILED: "Keys ignored" still on the plane |
| the quiet (round 5's case) | `…the_quiet_ends_the_quarantine` | ok, as round 5 made it |

Two more tests:
- `fx_s_warning_on_the_plane_outlives_every_change_of_hold_and_the_quarantine`.
  fx's warning stays through an open sequence, the byte that breaks it, and
  the quiet. It is green on both trees; mutant 5 is its kill.
- `a_flapping_hold_never_leaves_the_plane_naming_the_other_one`. For 450
  turns of `ESC [` / `x`, the plane at the end of each turn names the hold of
  that turn or nothing. It does name each hold at some turn. On `f9328d4e` it
  failed at step 127: held by the quiet, the plane still said "Key sequence
  open".

**One test changed steps.** Round 5's
`the_quarantine_s_sentence_leaves_the_review_plane_when_the_quarantine_ends`
now ends its turn with `settle_input`, as the loop does. Its assertions are
unchanged, and it is green on both trees.

**Opus SHOULD 1 is part of item 1.** A whole paste settled in one delivery
re-arms the quarantine. It does not clear the line by itself; the new
quarantine's `named_hold` is `None`, and that clears it. The hold alone
would not: the old quarantine and the new one are both held by the quiet.
Mutant 3 compares the hold alone, and the whole-paste test kills it.

## 2. The flapping test counts every notice (gpt6 SHOULD)

`a_hold_that_flaps_is_said_at_most_once_a_second_and_its_last_state_is_said`
now does the following:
- it keeps every `! input:` row of every turn, not only the first one a
  turn's rows hold;
- it asserts at most one per turn;
- it asserts that the first is fx's warning and every later one is one of
  xfx's three sentences;
- it checks every interval, including the one from fx's warning to the
  first repeat. Round 5's test skipped that interval (`said_at[1..]`).

It was green on `f9328d4e` too: the code was right, and the observer was
too weak to show it. Mutants 8 and 9 show the difference. Each was run on
round 6's code against the new test, and on round 5's code against round 5's
test (`mutants-round5.txt`).

## 3. Opus SHOULDs 2-5

- **(2) The repaint is pinned.** Every transition test asserts that a frame
  was requested (`render.begin()`), drained before the transition. In the
  breaking-byte case, nothing else requests one, because the notice is
  rate-limited. Mutant 4 deletes the request.
- **(3) A status is given up whole.**
  - When the screen is too short, the drop loop takes every `Part::Status`
    row at once (`src/tui/approval_screen.rs:382`).
  - Before, `rposition` dropped the second row first, so at one height the
    plane painted "Keys ignored until" alone. On `f9328d4e`, at 20 columns
    and 16 rows, `red.txt` shows 1 of 2 rows.
  - The test is
    `a_status_is_on_the_screen_whole_or_not_at_all_at_every_height`: 20
    columns, heights 7 to 40, all rows or none, and the status shown at some
    height.
  - The trade, named: a status too long for its rows is shown as its leading
    clause (the readiness refusal, by design). It is now given up whole too,
    so at the one to three heights where only part of its block fitted, the
    refusal is not shown. The gate still refuses, and a taller screen shows
    it.
- **(4) fx's warning has the room. `STATUS_ROWS` is 3**
  (`approval_screen.rs:56`), and fx's words are unchanged.
  - From 29 columns down to 20, `PASTE_SUFFIX` needs three rows. Two rows
    cut it to "Input that arrived right after the" (`red.txt`).
  - The test is
    `fx_s_warning_is_whole_on_the_review_plane_at_every_width_down_to_the_narrowest`:
    20 to 40 columns.
  - The status is the first thing given up on a short screen, and it is
    given up whole, so the extra row never costs the question an answer.
    `a_status_line_never_costs_the_question_its_answers` is still green.
  - The 30-column refusal test's comment now says three rows.
- **(5) Docs.**
  - `Hold` (`input.rs:493`) no longer says "what the user is told will end
    it". Only the quiet's sentence names a release point.
  - The doc on the sentences (`shell.rs:554`) and `parity.md`'s paste
    paragraph now say a cut sentence appears at no width and no height.
    They also say where and when the line is reconciled: at the end of every
    turn's input, with no new notice. This replaces round 5's "the moment
    the quarantine ends", together with the comment that said it, which
    went with the deleted clear.

## 4. Runtime in tests

The tests in this round build no `Runtime`. The fixture's `shell_with` takes
`WorkHandle::detached()` (`shell.rs:16969`). The file's two runtime
constructors are MCP ones in other tests (`McpRuntime::with_environment`,
`McpRuntime::new`). None of these tests reaches a reload.

## 5. Mutants (`mutants.py`, `mutants.txt`, `mutants-round5.txt`)

Each mutant ran on a throwaway `git clone --no-local` at `d0789285`, never on
the worktree, with `GIT_DIR`, `GIT_INDEX_FILE` and `GIT_WORK_TREE` unset.
- Each made one change (one or more edits), then ran
  `cargo test --locked --lib tui::` and the four pty paste tests, each in its
  own process group with a 900 s limit. The change was undone after.
- They ran under the suite lock.
- **10 of 10 killed**, and every failing test is named in `mutants.txt`.
- Every killer is a paste, quarantine or plane test. No timing test killed
  any of them this round.

| # | the rule reverted | killed by |
|---|---|---|
| 1 | **item 1: cleared only on release (round 5's rule)** | 4: the breaking byte, the dropped paste's end marker, the whole paste, the flapping plane |
| 2 | **item 1: reconciled on the drained path only, so the read cap's turn skips it** | 1: the read-cap test |
| 3 | item 1: the hold compared without the quarantine it was said in | 1: the whole-paste test |
| 4 | **item 3(2): no repaint when the line is taken down** | 5: all five transition tests |
| 5 | item 1: fx's warning taken down too | 1: `fx_s_warning_on_the_plane_outlives_…` |
| 6 | **item 3(4): the plane's status has two rows again** | 1: `fx_s_warning_is_whole_on_the_review_plane_…` |
| 7 | **item 3(3): a status too tall is given up from its last row again** | 1: `a_status_is_on_the_screen_whole_or_not_at_all_…` |
| 8 | item 2: the first repeat half a repeat after fx's warning | 7, the flapping test among them |
| 9 | item 2: fx's warning said again behind a repeat, in the same turn | 15, the flapping test among them |
| 10 | item 1: no reconcile at all | 7 |

Notes on the table:
- **Mutant 2 is the "skip the cap path" mutant.** In this design the cap
  path has no code of its own to delete: the reconcile runs at the turn's
  end, which every burst reaches. So the regression it stands for is a
  reconcile on the drained path only. The mutant moves the call from
  `settle_input` to the end of `end_delivery_at`. The read-cap test is its
  only killer; every other transition still passes.
- **Mutant 1 is not killed by the read-cap test.** A paste begun inside the
  quarantine ends it, so "cleared only on release" clears there too. What
  kills mutant 1 is the three transitions where the quarantine goes on, and
  the flapping plane.

**Round 5's observer** (`mutants-round5.txt`): mutants 8 and 9 were run
again in round 5's code (`f9328d4e`), against round 5's flapping test
alone. **Both survived.** Its observer skipped the interval from fx's
warning to the first repeat, and kept one notice per turn. Round 6's test
kills both (`mutants.txt`, lines 9-10: mutants 8 and 9).

## 6. The sweep: not run (`sweep-reason.txt`)

A cell can change only where the plane has a status, for three reasons:
- the row budget and the whole-or-nothing drop act only on `Part::Status`
  rows, which exist only while the plane has a status;
- the reconcile takes down only one of the three sentences;
- the decoder is unchanged.

None of the 41 input scenarios has a plane status:
- In PASTE-GAP round 1's head sweep (`../sweep-head.md`, the same 41
  scenarios), **0 of 254 xfx captures** has a plane status row.
- The pattern is shown able to match. Captures keep the two-column indent
  (20 rows), and a real composed status row matches it.
- The scenario files are unchanged since `a832f4d7` (0 diff lines).

## 7. Gate (`gates.txt`)

Every CI step of `ci.yml` ran in order at **`d0789285`**, under the suite
lock (one slot for the run). **All 17 exit 0** (`gates.txt`).

| step | result |
|---|---|
| fmt, clippy, fault-injection clippy | exit 0 |
| `cargo test --locked --all-targets` | 14 binaries, **5498 passed**, 0 failed (round 5's gate had 5489; this round adds 9 tests) |
| with the fault injector | 15 binaries, **5536 passed**, 0 failed |
| no-stubs, no-secrets, identity, preview contract and its mutants, preflight cost, usage clock | exit 0 |
| gate-env | exit 0, 62 of 62. `identity-own-miscased`, the harness race in the dispatcher's round 5 gate, is `ok` here |
| release and faulty builds, `smoke.sh` | exit 0 |
| `smoke-tui.sh` | 37 scenarios + the oracle, **1167 checks, 0 failures** |

`green-unit.txt` comes from the gate's `test` log. It holds round 6's nine
tests, round 5's plane and sentence tests, and the plane's status tests:
15 `ok`. The four pty paste tests are `ok` in the same log.

The non-test line count: `nontest.py`. It counts the added lines before each
changed file's `#[cfg(test)] mod tests`. Its figures:
- `a832f4d7..6cb46583`: 497, round 5's figure;
- `6cb46583..d0789285`: 90;
- `a832f4d7..d0789285`: 562.

## parity.md

The `full-screen TUI` paste paragraph has three changes:
- fx's warning wraps into three rows at 20 columns;
- the plane has three status rows, and a status too tall for the screen is
  given up whole;
- the line is reconciled at the end of every turn's input. It is taken down
  once it no longer names what holds the quarantine (the hold changed, the
  quarantine ended, or a paste replaced it), and nothing new is said.
