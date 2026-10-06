# C2 receipt: `/clear`, `/new`, `/reset`

Scenario `scripts/fxcap/scenarios/reset.cap`, turns
`scenarios/reset-turns.json`, run at 40x120 on both binaries:

```
FX_BIN=<fx c1db919 build, 0.0.13> scripts/fxcap/capture.sh fx  reset 40x120 <dir>
                                  scripts/fxcap/capture.sh xfx reset 40x120 <dir>
```

It sends `first question alpha` (answered `ALPHA answer line one.\n\nSecond
paragraph with `code`.`), then `/clear`; `second question beta` (answered
`BETA answer.`), then `/new`; then `/reset` on the fresh screen `/new` left;
then `third question gamma`. It snaps the screen after each step, the whole
tmux history above the screen after each reset and after the last turn, and
the window title at launch, after the first turn, and after `/clear` and
`/new`.

One verb was added to `capture.sh` for this receipt: `snapfull <label>` writes
`capture-pane -S -` -- the pane's history and the screen -- to
`<bin>-reset-40x120-<label>.full.{txt,ansi}`. That history is the terminal's own
native scrollback, which is where a reset hands the screen. The shims (`open`,
`pbcopy`, `pbpaste`, `afplay`, `osascript`), the fake `pbcopy`'s pipe and
`FX_NO_OPEN_BROWSER=1` are unchanged.

The xfx binary is `target/debug/xfx`, built from the working tree of this
commit. Its banner shows `ea484d5`, the base the branch started from: `build.rs`
records the revision when it runs, and its `.git/HEAD` trigger does not see a
worktree's HEAD move (`.git` is a file there).

Sanitized: no scratch path appears in any capture, so nothing was replaced.

## What matches

Every `.txt` and `.ansi` pair, screen and full history, is identical row for
row apart from the welcome line itself (`𝒇x v0.0.13` against
`x𝒇x v0.1.0-ea484d5 [debug]`) -- the rows of the old document in scrollback, the
blank rows between them, where the fresh welcome line starts, the composer and
statusline under it, and the SGR of every row.

| snap | fx | xfx | matches |
|---|---|---|---|
| `turn1` | answer, summary `0s (↑6 ↓40)` | same | Identical but the welcome line. |
| `clear` | welcome on row 1, `┃` on row 3, statusline on row 5; nothing else | same | Identical but the welcome line. **No notice** on either. |
| `clear.full` | history rows 1-9 are the old screen through the summary; row 10 is the new welcome line | same | Identical but the two welcome lines. The blank row under the summary is not carried: the history ends on the summary and the welcome follows it at once, on both. |
| `turn2` | `┃ second question beta` and `BETA answer.` under the fresh welcome | same | Identical but the welcome line. |
| `new`, `new.full` | `/new` hands rows 10-16 (second welcome through `0s (↑5 ↓40)`) to history; welcome at history row 17 | same | Identical but the welcome lines. |
| `reset`, `reset.full` | `/reset` on a fresh screen hands over only the welcome line (history row 17); the new one is row 18 | same | Identical but the welcome lines. |
| `turn3`, `turn3.full` | `third question gamma` answered under the reset's welcome, summary `0s (↑6 ↓40)` | same | Identical but the welcome lines. The summary's `↑6` is the prompt alone on both: the request carried no earlier turn. |
| `launch.title`, `clear.title`, `new.title` | `fx v0.0.13 \| work` | `xfx v0.1.0 \| work` | Same shape: the fallback title, so each reset gave the session's title back. |
| `turn1.title` | `first question alpha` | same | Identical: the first prompt titles the session on both. |

## Not in this receipt

- **A reset while a turn is running.** xfx cancels the turn (it does not wait
  behind it), drops what the turn still sends, and runs the next prompt at once
  in a fresh conversation. `tests/tui.rs`
  `a_reset_mid_turn_cancels_the_turn_rather_than_waiting_for_it` drives it on a
  pseudoterminal against a turn that never finishes, and the unit tests in
  `src/tui/shell.rs` (`a_reset_mid_turn_cancels_the_turn_and_does_not_wait_behind_it`,
  `a_reset_with_a_prompt_waiting_drops_it_and_is_never_refused`) and
  `src/tui/worker.rs` (`a_reset_reaches_work_that_is_still_running_and_names_the_last_count`)
  cover the rest. One known difference: answer rows that had already landed
  when the turn was reset go to scrollback with the rest of the document;
  upstream holds a streaming answer in its live region and commits only the
  prompt card (`research/general-session` `fx-busy-40x120-clear-settled.full.txt`).
- **A reset while a question owns the alternate plane** is deferred until the
  plane is given back, as upstream's is; `src/tui/event_loop.rs`
  `a_reset_while_a_question_owns_the_other_plane_waits_for_the_plane_to_come_back`.
- **The session on disk.** Upstream opens the fresh session at once and deletes
  it on exit if nothing was said in it; xfx opens it with the first prompt.
  Either way the sessions left on disk are the ones a prompt was sent in.
