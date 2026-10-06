# C4 receipt: /copy

Scenario `scripts/fxcap/scenarios/copy.cap`, turn `scenarios/copy-turn.json`,
run at 40x120 on both binaries:

```
FX_BIN=<fx c1db919 build, 0.0.13> scripts/fxcap/capture.sh fx  copy 40x120 <dir>
                                  scripts/fxcap/capture.sh xfx copy 40x120 <dir>
```

It types `/copy` before any turn, then `/copy now`, then one prompt whose
answer is `ALPHA answer line one.\n\nSecond paragraph with `code`.` (the same
turn the research capture `fx-sessions-40x120-sh.log` used), then `/copy`, then
`/copy` again with the fake `pbcopy` set to fail. It snaps the screen after each
step and keeps what the fake `pbcopy` was fed.

Two verbs were added to `capture.sh` for this receipt: `clip <label>` moves the
bytes the fake `pbcopy` received to `<bin>-copy-40x120-<label>.clip`, and
`clipfail on|off` makes the fake read its input and exit 1. The fake is also run
once before the session starts. macOS assesses a new executable on its first
run, which took 1.2 seconds for a fresh script on the capture host (and longer
for fx's first copy in one run) and landed inside the scenario's `wait`: in a run without the warm-up, fx's first
copy had not finished when `clipfail on` was set, so it failed too. The shims
(`open`, `pbcopy`, `pbpaste`, `afplay`, `osascript`) and `FX_NO_OPEN_BROWSER=1`
are otherwise unchanged. Nothing reached the real clipboard.

The xfx binary is `target/debug/xfx`, built from the working tree of this
commit. Its banner shows `ea484d5` because that commit is the base.

Sanitized: no scratch path appears in any capture, so nothing was replaced.

## Per snap

| snap | fx | xfx | matches |
|---|---|---|---|
| `copy-empty` | `* clipboard: No assistant reply to copy.` | same | Row 3 is byte for byte the same, SGR included (`ESC[38;5;250m* clipboard: No assistant reply to copy.ESC[39m`). There is no echo of `/copy` on either. Nothing was copied (`clip copy-empty: nothing was copied` on both). |
| `copy-now` | `✗ command: Unknown command. Try /help.` | same | Row 5 is byte for byte the same (`ESC[38;5;252m✗ command:ESC[38;5;250m Unknown command. Try /help.ESC[39m`). `/copy` takes no argument on either, and nothing was copied. |
| `turn1` | the prompt card, the answer, `0s (↑6 ↓40)` | same | Rows 7-13 are the same on both. |
| `copy` | `* clipboard: Copied to clipboard.` | same | Row 15 is byte for byte the same. `fx-copy-40x120-copy.clip` and `xfx-copy-40x120-copy.clip` are identical: 53 bytes, `ALPHA answer line one.\n\nSecond paragraph with `code`.`, backticks kept, no line break added. They are also the bytes in the research capture (`fx-sessions-40x120-sh.log`, `od -c`). |
| `copy-fail` | `✗ clipboard: Failed to copy to clipboard.` | same | Row 17 is byte for byte the same (`ESC[38;5;252m✗ clipboard:ESC[38;5;250m Failed to copy to clipboard.ESC[39m`). Nothing was copied. |

In every `.txt` and `.ansi` pair the only row that differs is row 1, the
welcome line, which is each product's own banner (`𝒇x v0.0.13` vs
`x𝒇x v0.1.0-ea484d5 [debug]`). Every other row is identical: the notices, the
blank rows, the prompt card, the answer, the turn summary, the composer and
the statusline.

## What differs and is not on these screens

- **Where the copy waits.** fx runs `pbcopy` on its UI thread and waits for it
  (`copyToClipboard`, `src/core/hosts/native.zig:15-52`, called from
  `commandCopyLast`, `app_commands.zig:1974-2001`). xfx runs it on a thread of
  its own and says how it went when the tool exits, so a clipboard tool that is
  slow or never exits does not stop the screen being painted. On a fast tool the
  screens are the same, as above. In a discarded run without the warm-up (not
  in this directory), fx's screen still showed `/copy` in the composer with the
  menu open 0.3 and 3.3 seconds after the Enter. xfx's screen 0.3 seconds after
  the Enter had already cleared the composer, with no notice yet, and its copy
  had finished by 3.3 seconds. The unit tests prove the xfx side: `a_copy_is_started_and_answered_later_rather_than_waited_for`
  (`src/tui/copy.rs`) and `copy_does_not_wait_behind_the_work_the_runtime_has_in_hand`
  (`src/tui/shell.rs`).
- **Where the reply comes from.** fx reads its session history on the UI
  thread. In xfx the conversation belongs to the runtime thread, which
  publishes its last reply just before each turn's conclusion is sent and again
  after every piece of work. `/copy` reads the newest one at once, so it does not
  wait behind a running turn and it does not take a place in the queue. The
  reply is read from the session log and not from the screen: the screen gets
  the answer with its control characters made inert, so a tab is drawn as a
  space, but the clipboard gets the tab. `tests/tui.rs`
  `slash_copy_puts_the_reply_on_the_clipboard_as_the_model_wrote_it` shows
  this with a tab in the reply.
- **`/clear` on this branch.** xfx's `/clear` still keeps the conversation
  (`CLEARED_NOTICE`). Because `/copy` reads the conversation, a `/copy` after
  `/clear` on this branch still copies the last reply. fx's `/clear` starts a
  new conversation, so fx's `/copy` would say `No assistant reply to copy.`
  there. When `/clear` drops the conversation (the reset unit), `/copy` follows
  it with no change here, because the reply is published after every piece of
  work, whatever that work did to the conversation. `/new` already drops it:
  `work_that_drops_the_conversation_takes_its_reply_with_it`
  (`src/tui/worker.rs`).
- **`/setup`.** A provider switch in xfx replaces the conversation, so `/copy`
  after one says `No assistant reply to copy.`. fx's `/provider` was not
  captured for this receipt.
