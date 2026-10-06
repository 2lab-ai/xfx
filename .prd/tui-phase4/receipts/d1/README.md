# D1 receipt: `/resume` (picker and replay) and `/rename`

Seven scenarios on both binaries -- the first round's four, and three for the
follow-up (tool rows and stopped turns on replay, the picker's pages, a
rename while a turn runs):

```
FX_BIN=<fx c1db919 build, 0.0.13> scripts/fxcap/capture.sh fx  <scenario> <size> <dir>
                                  scripts/fxcap/capture.sh xfx <scenario> <size> <dir>
```

| scenario | size | what it does |
|---|---|---|
| `resume.cap` | 40x120 | Three sessions in one run: `first question alpha`, then `/rename Renamed alpha session with a much longer title than the others`, `/new`; `second question beta`, `/new`; `third question gamma`, `/new`. Then `/resume` and the picker's keys -- Down three times (the third is past the last row), Up, Tab, Shift-Tab, `beta`, Escape -- then `/resume`, `alpha`, Enter. After the resume: the screen, the whole tmux history, the window title, `/resume` again, a fourth prompt, `/quit`. |
| `resume-empty.cap` | 40x120 | `/resume` with nothing saved, `zzz`, Enter on nothing, Escape; then a slow streaming turn and `/resume` while it streams. |
| `rename.cap` | 40x120 | `/rename` with nothing, `/rename   My Title  `, a 241-byte title, `/quit` -- then the binary is started again in the same pane on the latest session (`fx --resume-last`, `xfx --resume last`). |
| `resume-launch.cap` | 40x120 | Two turns, `/quit`, then the binary again on the latest session, and a third turn in it. |
| `resume-tools.cap` | 40x120 | The F1 tool-group turns in `ask` mode (`toolgroup-*.json`, `toolgroup-ws`): a group with every outcome form (Read, Matched, Searched, Ran, `Edited a.txt +1 / -1`, `Denied file`, `Failed missing.txt`, `Exited 3`), the closing answer and its summary; a turn stopped with Escape at a write's approval; a turn stopped with Ctrl-C while `sleep 4` runs. Then `/new`, `/resume`, Enter, and Ctrl-O on the replay. |
| `resume-paging.cap` | 24x80 | Eighteen sessions (`question one` ... `question eighteen`), then `/resume`: the first page (seventeen), Down fourteen, fifteen, sixteen (the last loaded session -- the next page loads), seventeen; Tab; Shift-Tab and `ve`; Down three. |
| `rename-midturn.cap` | 40x120 | A turn that streams ten chunks half a second apart; `/rename Counted mid turn` while it streams; the screen and the window title then, after the turn, and on a relaunch on the latest session. |

Turns are `scenarios/resume-turns.json`, `slow-line-turn.json`, the F1
tool-group files, `resume-paging-turns.json` and `rename-midturn-turns.json`.
`capture.sh` is plan's (one `home` verb, the sound guard, `relaunch`); nothing
in it changed for this round. `respawn-pane` blanks the pane when it restarts
it, so the first launch's screen is not above the second in either binary's
`relaunched`/`resumed` snaps; the history keeps what had scrolled off.

The xfx binary is `target/debug/xfx` built at `cbf8bea` (this branch with
`plan/tui-phase4` at `aed448c` merged) and its banner shows that revision;
the fx side is the c1db919 build, taken in the same round.
Sanitized: no scratch path appears in any capture; the host name tmux restores
as the window title after xfx's exit is written `<host>`. `.ansi` files are
tmux's re-serialization of cell attributes (`capture-pane -e`), not the
programs' wire bytes. `resume-paging`'s fx side was taken twice: in the first
run, under the load of the other units' builds, fx lost one of the eighteen
prompts (`question thirteen` typed while the previous `/new` was still being
handled), so it had seventeen sessions and no page to load; the second run,
the one kept, has all eighteen. `resume`'s xfx side was taken twice for the
same reason (load average above 30): the first run lost `first question
alpha` the same way and listed two sessions; the run kept lists all three.

## What matches

Every pair below is identical row for row, SGR included, **except row 1 where
row 1 is the welcome line** (`𝒇x v0.0.13` against `x𝒇x v0.1.0-cbf8bea
[debug]`). Where row 1 is the resume notice the whole `.ansi` file is
identical.

### The first round's scenarios, retaken

Every comparison of the first round holds on this tree, the same way:

| `resume/list3` | `Sessions 3  [Current workspace]  All workspaces`, a blank row, three rows, newest first; the renamed session under its title, every title padded to the widest (62 cells) plus four, `work · now · 1 turn` | Rows 2-40 identical. Header SGR `ESC[1mESC[38;5;255mSessions 3ESC[0m  ESC[1mESC[38;5;255m[Current workspace]ESC[0m  ESC[38;5;245mAll workspacesESC[39m`; marked row title and metadata both `ESC[1mESC[38;5;255m`, the others `ESC[38;5;245m`. |
| `resume/list3-down`, `list3-down2` | the mark on the second, then the third row | Rows 2-40 identical. |
| `resume/list3-down3-clamp` | the mark still on the third row | Rows 2-40 identical: Down **stops** at the last row on both. |
| `resume/list3-up` | the mark back on the second row | Rows 2-40 identical. |
| `resume/tab-all` | `Sessions 3  Current workspace  [All workspaces]` | Rows 2-40 identical. |
| `resume/query-beta` | `Sessions 1`, `second question beta` alone, its title column its own width | Rows 2-40 identical; the composer holds `beta`. |
| `resume/esc` | the idle band, composer empty | Rows 2-40 identical: Escape closed the picker and emptied the search. |
| `resume/resumed`, `resumed.full` | `* session resumed: Renamed alpha session …`, the card, the two answer rows, `  0s (↑6 ↓40)`, the band | **Whole files identical, screen and history.** The history is the screen and nothing above it on both: the terminal reset erased the scrollback (`ESC[3J`). No welcome line. |
| `resume/resumed.title` | `Renamed alpha session with a much longer title than the others` | Identical: the window takes the session's title, the `/rename` one. |
| `resume/list-after-resume` | `Sessions 2`: `third question gamma`, `second question beta` | Whole file identical. The session the resume closed is not listed on either: it was the one `/new` opened and nothing was said in it -- upstream deletes it on close (`discardableOnClose`, `app_session_runtime.zig:5702-5720`) and xfx never wrote it (a session is created at the first prompt or a `/rename`). The resumed session itself is left out on both. |
| `resume/continued` | the resumed turn, then `┃ fourth question delta`, `DELTA answer.`, `  0s (↑6 ↓40)` | Whole file identical: the conversation continued in the resumed session. |
| `resume/quit` | the document, then `Continue session with: … --resume <id>` | Identical but the product name and the id (C5's receipt says why). |
| `resume-empty/empty`, `empty-query`, `empty-enter` | `Sessions 0 …`, a blank row, `  No sessions found.` (dim, two columns in); Enter changes nothing | Rows 2-40 identical. |
| `rename/rename-empty` | `✗ usage: /rename <title>` -- no topic | Rows 2-40 identical, SGR `ESC[38;5;252m✗ ESC[38;5;250musage: /rename <title>ESC[39m`. |
| `rename/rename-title` | `* session: renamed to "My Title"` -- the title trimmed | Rows 2-40 identical. |
| `rename/too-long` | `✗ session: title is too long` for 241 bytes | Rows 2-40 identical. |
| `rename/quit` | the notices, **no** `Continue session with:` row | Rows 2-39 identical (row 40 is tmux's `Pane is dead` with its own clock). A session renamed and never asked anything is kept but not named: it is pristine (`shouldCreateResumeHandoff`, `app_session_runtime.zig:350-354`). |
| `rename/relaunched` | `* session resumed: My Title`, the band; no turn under it | **Whole file identical.** The titled, empty session survived the exit on both and is the latest one. |
| `rename` titles | `launch` `fx v0.0.13 \| work` / `xfx v0.1.0 \| work`; `renamed` and `too-long` `My Title` on both | Same shape; the refused title left the window alone. |
| `resume-launch/resumed` | `* session resumed: first question alpha` on row 1, both turns under it, no welcome line | Whole `.ansi` identical. |
| `resume-launch/resumed.full` | the first launch's welcome line in the history, then the screen above | Identical but that history row, which is each product's banner: a resume **at launch** resets nothing on either, so the row the first launch left in scrollback is still there. |
| `resume-launch/continued` | a third turn under the replay, `  0s (↑6 ↓40)` | Whole `.ansi` identical. |
| `resume-launch/resumed.title` | `first question alpha` | Identical. |


### Tool rows and stopped turns on replay (`resume-tools`)

| snap | both show | matches |
|---|---|---|
| `resumed`, rows 2-18 | `* session resumed: look at the files and fix them`, the card, `I will look at the files first.`, then `● 9 tool calls · 4 read · 2 commands · 1 list · 1 write · 1 edit · 2 failed · 1 denied` and its rows: `Ran sleep 3; …`, `Edited a.txt +1 / -1`, `Denied file`, `Failed missing.txt`, `Exited 3 echo boom; …` | **Identical, SGR included.** The live group's outcomes and counts come back from the log (`ending`, `lines`); the failure is `Failed missing.txt` without its reason, as fx's replay writes it (the live row has the reason on both). Before this round xfx replayed `Edited a.txt`, `Failed c.txt: …`, `Ran echo boom; …` and `2 failed` with no `denied`. |
| `resumed`, rows 21-24 | `┃ and once more`, a blank row, `■ Cancelled · What can fx do differently?` | Identical but the product's name: the turn stopped with Escape at the write's approval replays as fx's turn cancellation, no group -- the call the stop cut off is not replayed. `ESC[38;5;252m■ESC[39m ESC[38;5;255mCancelledESC[39m · …` on both. |
| `end`, `again-stopped` | the live group, and the live stopped turn `● 1 tool call · 1 write · 1 cancelled`, `└ Cancelled file`, `■ Cancelled file · …` | Identical but the failure's reason (each product's own error text) and the summary's output count (below) -- and since F4 merged, the stopped turn's rows are fx's. |
| `ctrl-o` | the full transcript after the resume: each call's entry with its arguments and its result -- `Denied file` with `path: c.txt` and the content, `Failed missing.txt` with the reason, `Exited 3 …` with `boom`, `err`, `exit code 3` | Same entries; differences below. |
| `picker` | `Sessions 1`, `look at the files and fix them    work · now · 3 turns` | Rows 2-40 identical. |

### The picker's pages (`resume-paging`, 24x80)

| snap | both show | matches |
|---|---|---|
| `page1` | `Sessions 17  [Current workspace]  All workspaces`, a blank row, `question eighteen` … `question eight` (eleven rows), `  ↓ Load more` | Rows 2-24 identical. The load-more row `  ESC[38;5;245m↓ Load moreESC[39m` on both. |
| `down14` | the window moved down to `question fourteen` … `question four`, `↓ Load more` still last | Rows 2-24 identical. |
| `down15`, `down16-loaded`, `down17` | the mark on the last loaded session loads the next page: `Sessions 18`, twelve session rows, no load-more row, the window ending on `question one` | Identical but the age column of the two or three oldest rows, which is the wall clock: fx's oldest sessions were a minute old at the snap (` 1m`), xfx's were not (`now`). |
| `tab-all` | `Sessions 17  Current workspace  [All workspaces]`, the first page again | Rows 2-24 identical: a scope change starts from one page. |
| `query-ve`, `query-ve-down3` | `Sessions 5`: the loaded sessions with `ve` in them, `↓ Load more` under them | Rows 2-24 identical: the search covers the loaded pages only. |

### `/rename` while a turn streams (`rename-midturn`)

| snap | both show | matches |
|---|---|---|
| `during` | `┃ count to ten slowly`, `* session: renamed to "Counted mid turn"` under it **while the answer streams**, the activity row | Rows 2-40 identical (an earlier take differed only in the activity row's blink phase, `•`). |
| `during.title` | `Counted mid turn` | Identical: the window took the title at once. |
| `after` | the notice, then the whole answer and `  5s (↑6 ↓40)` | Rows 2-40 identical. |
| `relaunched`, `relaunched.title` | `* session resumed: Counted mid turn` and the turn | **Whole files identical**: the title the session got mid-turn is in its log. |

## What differs, and why

- **`resume-tools`, the summary's output count** (`13s (↑8 ↓320)` against
  `↓40`): the live summaries already differ (`end`), so the replay of each
  says what its own live one said. Not this unit's: the live tally.
- **`resume-tools`, the turn stopped while `sleep 4` ran.** fx kills the
  command at the Ctrl-C (`Cancelled sleep 4`, `■ Cancelled sleep 4 · …`) and
  keeps no result for it, so its replay is the card and `■ Cancelled · …`. On
  this tree xfx's command ran to its end (`Ran sleep 4`) and the turn then
  ended cancelled -- live (`long-later`) and so on replay, which shows the call
  that came back and the turn's cancellation line. The replay is faithful to
  xfx's own log; the live stop is the divergence (the terminal tool's
  cancellation, not this unit's surface).
- **`resume-tools/ctrl-o`.** fx stamps every replayed entry with the moment of
  the resume (`21:44:45.685` on all of them: its live replay sink ignores the
  turn's times, `LiveHistorySink.setCreatedAtMs`,
  `app_session_runtime.zig:3972`); xfx stamps them with the log's times. fx
  lists the shell call's own arguments (`action: run`, `yield_time_ms: 30000`)
  where xfx's terminal call has none to list; xfx's `Denied file` entry ends
  with one more `│` row; each product's own failure text. All are the full
  transcript's (F5's) and are reported, not changed, here.
- **`resume-tools`, the approval snaps** (`cmd-ask`, `edit-ask`,
  `write-ask`, `exit-ask`) and `live.full`: there to show that each key
  reached the prompt it was meant for; the panels and the live screens are
  F1's and F2's surfaces, compared in their receipts, not here.
- **`quit` rows.** The product name and the id, and tmux's `Pane is dead`
  clock.
- **`resume-empty/while-streaming`.** The activity row's blink phase and
  how much had streamed (`↓71` against `↓69`).
- **The window title after exit.** `<host>` against empty, as C5's receipt and
  `ssot.md` §Tensions say.

## Not in this receipt, and where it is proven instead

- **The replayed group is the live group, byte for byte**, for every outcome
  form a row has (`Edited +1 / -1`, `Denied`, `Exited 3`, `No changes to …`,
  `Failed`), from a log-shaped state through `Hydration::of`
  (`src/tui/shell.rs`
  `a_replayed_tool_group_is_the_live_group_with_its_outcomes_and_counts`);
  the stopped turn's line and `✗ system: failed` for any other interrupted one
  (`a_stopped_turn_replays_as_upstreams_cancellation_and_a_failed_one_as_failed`);
  the log keeps the row's half (`tests/tool_loop.rs`
  `the_journal_keeps_how_each_call_ended_for_the_row_and_the_lines_it_changed`,
  `src/session/event.rs`
  `a_result_keeps_how_its_row_ended_and_an_older_or_newer_one_still_reads`);
  a call the stop cut off and a step with nothing back are not replayed
  (`src/tui/hydration.rs`
  `a_call_the_stop_cancelled_and_a_step_with_nothing_back_are_not_replayed`);
  a call refused by a stop at its approval is logged as stopped
  (`src/tui/worker.rs`
  `a_call_refused_by_a_stop_at_its_approval_is_recorded_as_stopped`).
- **A change's diff after a resume** (`src/tui/full_transcript.rs`
  `a_replayed_change_shows_the_diff_the_log_kept`).
- **A hostile log** -- an OSC, a clear, a bell, a C1 CSI in a prompt, an
  answer, a call's argument and a result -- reaches the UI through one inert
  step on both ways in (`src/tui/worker.rs`
  `a_hostile_log_reaches_the_ui_inert_at_launch_and_from_the_picker`), and the
  document and Ctrl-O paint no sequence of its own (`src/tui/shell.rs`
  `a_hostile_log_paints_no_sequence_of_its_own_on_the_document_or_the_full_transcript`);
  a rename holding one refuses its log (`src/session/store.rs`
  `a_rename_the_validation_would_refuse_is_not_a_log_xfx_wrote`).
- **The derived title** (`src/session/store.rs`
  `a_title_is_the_first_line_of_the_first_prompt_and_is_bounded`,
  `a_session_is_titled_by_the_first_prompt_that_gives_a_title`).
- **`/resume`'s busy rule** is a response in hand, not other work
  (`src/tui/shell.rs`
  `resume_waits_for_a_response_or_a_prompt_and_not_for_other_work`).
- **Pages**: `src/tui/session_picker.rs`
  `a_page_is_the_screen_less_seven_rows_and_never_fewer_than_ten`,
  `more_sessions_than_a_page_end_on_load_more_and_reaching_the_last_loads_the_next`,
  `the_search_and_a_scope_change_keep_to_the_pages_loaded`,
  `enter_on_load_more_loads_the_page_and_marks_its_first_session`; through the
  shell (`a_picker_with_more_sessions_than_a_page_loads_the_next_as_the_mark_reaches_it`).
- **A rename around a reset**: between turns, one typed before `/new` is
  written into the session `/new` drops and one typed after it into the
  session `/new` starts (`src/tui/worker.rs`
  `between_turns_a_rename_is_written_into_the_session_it_was_typed_in`);
  while work runs it is put aside, never a stop, and marked when it came
  behind a reset (`a_rename_while_work_runs_waits_for_it_and_one_after_a_reset_is_the_fresh_ones`),
  then written on the right side of the drop, `/quit` or not
  (`renames_put_aside_land_on_either_side_of_the_reset_they_were_typed_around`);
  the shell sends it on the control channel behind a reset
  (`src/tui/shell.rs`
  `rename_while_a_turn_runs_applies_at_once_and_follows_a_reset_on_one_channel`).
- **A session another xfx holds** is refused at once with upstream's red row
  (`src/tui/worker.rs`
  `resuming_from_the_picker_swaps_the_conversation_and_refuses_a_held_one_at_once`,
  `src/tui/session_picker.rs`
  `a_failed_resume_is_said_in_red_on_the_row_under_the_header_and_enter_may_retry`).
- **The reset vector**, `ESC[H ESC[2J ESC[3J` and the replay from row 1, and
  the launch's resume writing neither: `src/tui/event_loop.rs`
  `a_resume_resets_the_screen_and_its_scrollback_and_replays_from_row_one`,
  `a_resume_owed_on_top_of_a_reset_is_only_the_reset_of_the_terminal`,
  `a_launch_resume_resets_nothing_and_writes_no_welcome_line`.
- **A rename the store refuses** says `! session: renamed for this process but
  not saved (…)`: `src/tui/worker.rs`
  `a_rename_the_store_refuses_says_why_and_the_session_keeps_its_old_title` and
  `src/tui/shell.rs` `rename_refuses_in_upstreams_words_and_titles_the_window_at_once`.
- **`title must be printable text`** cannot be typed through tmux's `send-keys
  -l`; the shell case above drives it through the handler.
