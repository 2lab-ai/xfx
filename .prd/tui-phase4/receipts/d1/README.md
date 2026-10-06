# D1 receipt: `/resume` (picker and replay) and `/rename`

Four scenarios, run at 40x120 on both binaries:

```
FX_BIN=<fx c1db919 build, 0.0.13> scripts/fxcap/capture.sh fx  <scenario> 40x120 <dir>
                                  scripts/fxcap/capture.sh xfx <scenario> 40x120 <dir>
```

| scenario | what it does |
|---|---|
| `resume.cap` | Three sessions in one run: `first question alpha`, then `/rename Renamed alpha session with a much longer title than the others`, `/new`; `second question beta`, `/new`; `third question gamma`, `/new`. Then `/resume` and the picker's keys -- Down three times (the third is past the last row), Up, Tab, Shift-Tab, `beta`, Escape -- then `/resume`, `alpha`, Enter. After the resume: the screen, the whole tmux history, the window title, `/resume` again, a fourth prompt, `/quit`. |
| `resume-empty.cap` | `/resume` with nothing saved, `zzz`, Enter on nothing, Escape; then a slow streaming turn and `/resume` while it streams. |
| `rename.cap` | `/rename` with nothing, `/rename   My Title  `, a 241-byte title, `/quit` -- then the binary is started again in the same pane on the latest session (`fx --resume-last`, `xfx --resume last`). |
| `resume-launch.cap` | Two turns, `/quit`, then the binary again on the latest session, and a third turn in it. |

Turns are `scenarios/resume-turns.json` (the research capture's `four-turns.json`)
and `slow-line-turn.json`. `capture.sh` gained one verb for the last two
scenarios: `relaunch <args>` restarts the binary in the same pane with the same
`HOME`, environment and gateway, after `keep` and an exit, so the second launch
sees the session the first one saved. Nothing else in it changed: the shims
(`open`, `pbcopy`, `pbpaste`, `afplay`, `osascript`) and `FX_NO_OPEN_BROWSER=1`
are as they were. `respawn-pane` blanks the pane's screen when it restarts it,
so what the first launch left on the screen is not above the second one in
either binary's `relaunched`/`resumed` snaps; the history keeps the rows that
had scrolled off before it.

The xfx binary is `target/debug/xfx`, built at `214b7f8` -- this branch with
`plan/tui-phase4` (`f7d6118`) merged into it -- and its banner shows that
revision. The receipt was first taken on the branch before any merge, and
again at `1482ec6` (`e50a8f0` merged) and `a4d7861` (`0204be9` merged); every
comparison below came out the same each time but the streaming one, which T2
changed (see below). Sanitized: no scratch path appears in any
capture; the host name tmux restores as the window title after xfx's exit is
written `<host>`. `.ansi` files are tmux's re-serialization of cell attributes
(`capture-pane -e`), not the programs' wire bytes.

## What matches

Every pair below is identical row for row, SGR included, **except row 1 where
row 1 is the welcome line** (`𝒇x v0.0.13` against `x𝒇x v0.1.0-214b7f8
[debug]`). Where row 1 is the resume notice the whole `.ansi` file is
identical.

| snap | both show | matches |
|---|---|---|
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

## What differs, and why

- **`resume-empty/while-streaming`.** Rows 2-6 and 8-40 are identical: the
  card, then `* session: resume is unavailable until the response finishes`
  with upstream's SGR, and no picker. Row 7, the activity row, differs in two
  things the clock decides -- `• Generating` against ` Generating` is the
  blink phase at the moment of the snap, and `↓73` against `↓71` is how much
  had streamed by then. (Before T2 merged, the notice landed under the
  answer's wrapped rows rather than under the card; T2 holds the unfinished
  line whole, as upstream does.)
- **The window title after exit.** `<host>` against empty, as C5's receipt and
  `ssot.md` §Tensions say.
- **`quit` rows.** The product name and the id, as C5's receipt says.

## Not in this receipt, and where it is proven instead

- **A session another xfx holds** is refused at once with upstream's red row
  under the header (`This session is open in another xfx. Close it there, then
  press enter to retry.`): `src/tui/worker.rs`
  `resuming_from_the_picker_swaps_the_conversation_and_refuses_a_held_one_at_once`
  (under 200 ms, nothing changed) and `src/tui/session_picker.rs`
  `a_failed_resume_is_said_in_red_on_the_row_under_the_header_and_enter_may_retry`.
- **The replay is the live document**, byte for byte, colours included, for
  two turns (`src/tui/shell.rs`
  `a_resumed_session_replays_its_turns_as_the_live_ones_were_written`) and for
  a turn with a tool group and a stopped turn
  (`a_replayed_tool_group_is_the_live_group_and_a_stopped_turn_ends_on_its_reason`).
- **The reset vector**, `ESC[H ESC[2J ESC[3J` and the replay from row 1, and
  the launch's resume writing neither: `src/tui/event_loop.rs`
  `a_resume_resets_the_screen_and_its_scrollback_and_replays_from_row_one`,
  `a_resume_owed_on_top_of_a_reset_is_only_the_reset_of_the_terminal`,
  `a_launch_resume_resets_nothing_and_writes_no_welcome_line`.
- **A rename the store refuses** says `! session: renamed for this process but
  not saved (…)`: `src/tui/worker.rs`
  `a_rename_the_store_refuses_says_why_and_the_session_keeps_its_old_title` and
  `src/tui/shell.rs` `rename_refuses_in_upstreams_words_and_titles_the_window_at_once`.
- **Ctrl-O after a resume** shows the replayed session, because the replay is
  written through the writes the full transcript records
  (`src/tui/shell.rs`
  `ctrl_o_after_a_resume_shows_the_replayed_session_and_not_the_one_it_replaced`).
  A replayed call's expanded detail -- its arguments, a command's output --
  is not there yet: the live screen gets it teed from the turn's journal
  (`UiEvent::ToolDetail`), and the replay sends none.
- **`title must be printable text`** cannot be typed through tmux's `send-keys
  -l` (a control byte is a key to the composer, not text); the shell case
  above drives it through the handler.
