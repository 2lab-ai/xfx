---
title: 0.0.16 delta -- AT-FOLDERS round 12 (a test that hung on the temp path's letters)
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, input, file-picker]
---

# AT-FOLDERS round 12

Round 11 at `da1c1ae2` was accepted. This round fixes the test that round 11
found depending on the temp path's letters. It changes tests only.

| commit | what |
|---|---|
| `0cac3766` | the test fix, plus fx's matcher lines quoted in `../../at-height/fx-src.txt` |
| `fad2701d` | the ledger's 6 `shell.rs` refs re-pointed, each `src/tui/shell.rs:40461` to `:40471` (`refs.txt`: 16 checked, 0 wrong) |
| (receipt commit) | this folder |

`tests.txt` holds every run below. TMPDIRs:
- AL = `<scratch>/abcdefghijklmnopqrstuvwxyz`, as in the brief;
- SP = `<scratch>/with space`;
- A6 = `<scratch>/` plus the alphabet six times over;
- SAL and SSP = the same two names under a short 2-level directory.

## 1. fx first: the code is right, the test was wrong

fx at the pin matches a query anywhere in a path whose name does not match:

- `choosePlacement` tries a placement inside the name first, then one that
  crosses from the folder holding the name. Only when neither fits does it
  fall back to anywhere in the path, with `name_fit = false`
  (`file_index.zig:1816-1818@481104fd`; `fx-src.txt:2577-2621`).
- An added root's candidates are absolute paths (`:957-958`, `:1098`;
  `fx-src.txt:2564-2575`).

So in fx too, `@sch` matches an added root's `<tmp>/.tmpXXXXXX/plan.md`
whenever that path holds an s, then a c, then an h. xfx's `score`
(`file_index.rs:598-622`) does the same. The case's whole-list expectation
was wrong.

`fx_src.sh` gains those four ranges. `fx-src.txt`'s lines 1-2562 are
byte-identical to round 11's (checked with `cmp`).

## 2. RED

Before the fix (`da1c1ae2`),
`a_change_of_roots_during_a_walk_with_a_refresh_queued_walks_the_new_roots_once`
was run alone under AL. It failed 5 of 5 runs: the listing was
`["src/deep/sched.rs", "<scratch>/abcdefghijklmnopqrstuvwxyz/.tmpgoj5ud/plan.md"]`.

## 3. The fix (`0cac3766`)

The case still pins what it was written for:
- one walk of the new roots (the walk count, unchanged);
- the listing is that walk's: its first row is `src/deep/sched.rs`, and any
  row after it is the extra root's `plan.md`.

The second point holds in fx as in xfx. A name match ranks above a
path-only one in fx (`scoreBetter`, `:1884-1890`; `fx-src.txt:2623-2630`)
and in xfx (`score_order`, `file_index.rs:539-550`). `sched.rs` holds `sch`
in its name, and `plan.md` does not.

This is the first-row assertion round 11 proposed, with fx's ranking now
read at the pin. The comment in the test cites fx.

**After the fix**, the case alone: 10 of 10 runs green under AL, and 10 of
10 under the default TMPDIR.

## 4. Look-ahead

This unit's tests are those in `file_index`, `file_picker`, `shell`,
`event_loop`, `check` and `frame`, the files its diff touches. The unit adds
no case to `tests/tui.rs`, the pty suite. They were run at `0cac3766`:

| run | result |
|---|---|
| the unit's filter, TMPDIR=SAL | 1330 passed, 0 failed |
| the unit's filter, TMPDIR=SSP | 1328 passed, 2 failed |
| the unit's filter, TMPDIR=AL (inside `--lib`) | 11 failed in `tui::shell` |
| the unit's filter, TMPDIR=SP | 1318 passed, 12 failed |
| the unit's filter, TMPDIR=A6 | 1319 passed, 11 failed |
| `--lib`, TMPDIR=AL | 4591 passed, 12 failed |
| `--test tui`, TMPDIR=AL, SP and SSP | 181 passed, 0 failed, each |

**None of this unit's tests fails on a TMPDIR's letters.** The short
all-letters run is fully green, and A6 holds every letter sequence up to six
long.

**Every failure is in a test this unit neither added nor changed.** For each
failing test, its function body is the same at `af2b6137` and at the head.
And each fails at the base too: a `--no-local` clone at `af2b6137`, the same
14 cases with `--exact`, failed 12 under AL and 13 under SP. Those are the
head's sets: the head's SP run used the unit's filter, which leaves out
`image`.
- **12 depend on the path's length, not its letters:**
  - `image::tests::a_fifo_named_like_an_image_is_refused_without_waiting_for_a_writer`:
    the socket path is longer than `SUN_LEN`.
  - 11 in `tui::shell::tests` paint or wrap a long temp path at 120
    columns, or refuse one as too long to draw:
    - `a_bare_workspace_opens_upstreams_picker_in_place_of_the_composer`
    - `a_change_in_an_added_root_can_be_remembered_as_one_in_the_workspace_can`
    - `a_saved_directory_is_listed_and_counted_from_the_profile_at_launch`
    - `enter_on_a_choice_writes_its_command_into_the_composer_and_runs_nothing`
    - `right_on_a_status_line_row_is_statusline_item_and_says_both_notices`
    - `show_opens_on_the_skills_own_tab_and_the_other_forms_answer_in_upstreams_words`
    - `the_branch_is_read_at_launch_and_again_at_the_end_of_a_turn`
    - `the_segments_follow_the_model_in_upstreams_order_and_only_when_on`
    - `trace_that_the_clipboard_refused_says_where_the_file_is_as_an_error`
    - `what_a_change_to_the_managed_root_says_waits_for_the_list_that_holds_it`
    - `workspace_list_says_the_snapshot_and_bad_words_say_the_usage`
- **2 depend on a space in the path:** `a_pasted_path_is_its_badge_at_once_and_says_nothing`
  and `a_pasted_path_the_read_table_denies_stays_words_and_says_why`. They
  fail under SP and SSP, at the head and at the base.

They are listed here and not fixed, as the brief says.

## 5. Checks

- `cargo fmt --check`: rc 0.
- `cargo clippy --locked --all-targets -- -D warnings`: rc 0. With
  `--features fault-injection`: rc 0.
- `refs.py af2b6137`: 16 checked, 0 wrong after `fad2701d` (`refs.txt`).
- The brief's path grep printed 0 on each commit's staged lines.
- The suites, the base build and the clippies ran under the suite lock.
  The single-case runs (RED, the 10 and 10) did not.
