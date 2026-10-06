# C5 receipt: /quit, /exit, and the way back in

Two scenarios, run at 40x120 on both binaries:

```
FX_BIN=<fx c1db919 build, 0.0.13> scripts/fxcap/capture.sh fx  quit          40x120 <dir>
                                  scripts/fxcap/capture.sh xfx quit          40x120 <dir>
FX_BIN=<fx c1db919 build, 0.0.13> scripts/fxcap/capture.sh fx  quit-pristine 40x120 <dir>
                                  scripts/fxcap/capture.sh xfx quit-pristine 40x120 <dir>
```

`quit.cap` sends one prompt (`first question alpha`, answered by
`quit-turn.json`), then `/quit now` and `/exit now`, then `/quit`. It snaps the
screen after the answer, after each refused line and after the exit, and records
the window title at launch, after the answer and after the exit.
`quit-pristine.cap` sends nothing and leaves with `/exit`. Both wait 4 seconds
before the first keystroke, because at 2 seconds a debug xfx was not yet up and
the typed prompt was lost. `capture.sh` is unchanged: its shims (`open`,
`pbcopy`, `pbpaste`, `afplay`, `osascript`) and `FX_NO_OPEN_BROWSER=1` stay as
they were.

The xfx binary is `target/debug/xfx`, built from this commit's working tree. Its
banner shows `ea484d5` because that commit is the base.

Sanitized: no scratch path appeared in any capture. The host name, which is
tmux's default pane title, is shown as `<host>`.

`.ansi` files are tmux's re-serialization of cell attributes
(`capture-pane -e`), not the programs' wire bytes.

## Per snap

| snap | fx | xfx | matches |
|---|---|---|---|
| `answered` | header, card, two answer rows, `0s (↑6 ↓40)`, composer, statusline | same | Rows 2-13 match byte for byte. Row 1 is each product's own banner. |
| `quit-arg` | `✗ command: Unknown command. Try /help.` | same | Byte for byte (row 11), SGR included. `/quit now` does not leave on either side (`commands.zig:498`: `/quit` takes nothing). |
| `exit-arg` | the same notice again | same | Byte for byte (row 13). The alias is refused with an argument the same way. |
| `quit` | the band is gone, the document stays, row 15 `Continue session with: fx --resume knhBbESGtXWW` | row 15 `Continue session with: xfx --resume 1791303682561-32bcb1a25dda2dd8` | Rows 1-14 match byte for byte. Row 15 has the same SGR on both, `ESC[38;5;245m…ESC[39m`, and is on the same row: the band's top row (14) is erased and left blank, and the line is on the row after it. The product name and the id differ (see below). Both exited with status 0. |
| `quit-pristine` / `quit` | no line | no line | The whole screen matches byte for byte. Neither names a session that had no prompt. |
| `launch.title` | `fx v0.0.13 \| work` | `xfx v0.1.0 \| work` | Same shape. |
| `answered.title` | `first question alpha` | `first question alpha` | Same. The first prompt titles the session. |
| `quit.title` | (empty) | `<host>` | **Differs on purpose.** See `ssot.md` §Tensions, "The window title on exit". Both write `ESC]2;BEL` on the way out. xfx then pops the title it pushed at launch, so tmux gives back its own default. |

On both sides, row 40 of the `quit` snaps is tmux's own `Pane is dead (status
0, <date>)`. In every `quit` snap the screen has moved up one row, on both
sides (the welcome line is gone and row 1 is blank). This receipt does not
explain the shift. It is the same on both sides. The research capture recorded
the same shift on one of its runs, also without an explanation
(`general-session.md`, header).

## What differs, and why

- **The command.** `xfx --resume <id>` where upstream prints `fx --resume <id>`
  (`render.zig:801-802`). The line must name a command this binary answers. A
  bare `xfx --resume <last|ID>` is now that command (`src/cli.rs`).
- **The id.** fx ids are 12 characters. xfx ids are
  `<creation ms>-<16 hex>` (`SessionId::generate`, `src/session/store.rs:147`).
  The store is xfx's own, so the id is too. At 40x120 both lines fit on one row.
  Below 66 columns xfx's line breaks before the command (`"\n  "`), by
  upstream's rule (`render.zig:803-804`). The unit test
  `the_handoff_breaks_before_the_command_when_one_row_cannot_hold_it` pins the
  break.
- **`fx --sessions-v2 --resume`** (`render.zig:802`) has no xfx counterpart.
  xfx has a single session format.
- **The window title after exit.** See the table.

## Not in this receipt, and where it is proven instead

- **Running the command the line names.** `capture.sh` runs one launch per
  scenario under a fresh `HOME`, so a second launch cannot see the first one's
  session. The pty test `quit_after_a_conversation_names_the_session_and_resume_continues_it`
  (`tests/tui.rs`) covers it. That test runs a prompt and `/quit`, then reads
  the id from the line printed after the restore. It relaunches
  `xfx --resume <id>`, sees `* session resumed:`, and sends a second prompt.
  The request carries both user messages, and no second session is created.
  Ctrl-D, and the second Ctrl-C of a resumed launch that asked nothing, name the
  same session (the Ctrl-C exits 130).
- **What a resumed launch shows.** Upstream writes `* session resumed: <title>`
  and then replays the saved turns into the document
  (`app_session_runtime.zig:2433-2437`). xfx writes the same notice and sets
  the window title to the session's title, but it does not replay the turns
  yet. The conversation continues, and the screen does not show its past.
- **No pristine session is kept.** Upstream deletes a pristine, untitled session
  when it closes (`app_session_runtime.zig:5702-5720`). xfx never writes one,
  because it creates the session at the first prompt. The tests
  `quit_before_any_prompt_names_nothing_and_keeps_no_session` (`tests/tui.rs`)
  and `a_runtime_that_was_asked_nothing_names_no_session_and_wrote_none`
  (`src/tui/worker.rs`) check that the sessions directory stays empty.
