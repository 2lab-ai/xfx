# D4b receipt: /statusline

Two scenarios, run at 40x120 on both binaries:

```
FX_BIN=<fx c1db919 build, 0.0.13> scripts/fxcap/capture.sh fx  statusline      40x120 <dir>
                                  scripts/fxcap/capture.sh xfx statusline      40x120 <dir>
FX_BIN=<fx c1db919 build, 0.0.13> scripts/fxcap/capture.sh fx  statusline-turn 40x120 <dir>
                                  scripts/fxcap/capture.sh xfx statusline-turn 40x120 <dir>
```

- `scripts/fxcap/scenarios/statusline.cap` is the research scenario
  (`appearance-security-ext/scenarios/statusline.cap`) keystroke for keystroke:
  bare `/statusline`, Down, Enter, Escape, then `/statusline context`,
  `session`, `workspace` and `bogus`. It adds a `home` dump after every write.
- `scripts/fxcap/scenarios/statusline-turn.cap` makes the workspace a checkout
  on `main` (`githead main`), turns all three items on, and runs one turn that
  the fake gateway answers with `MARKER-STATUSLINE-TURN` (usage: 1200 input
  tokens, no published context window).

`capture.sh` gained two verbs and keeps its shims (`open`, `pbcopy`,
`pbpaste`, `afplay`, `osascript`), `FX_NO_OPEN_BROWSER=1`, the isolated tmux
socket and the empty `HOME`:

- `githead <branch>` writes `ref: refs/heads/<branch>` to the workspace's
  `.git/HEAD` before launch. A real `.git` directory cannot be committed under
  `scenarios/`.
- `home <label>` writes every `settings.json` under the run's `HOME`, and fx's
  `backups/`, to `<bin>-<scenario>-<size>-<label>.home`.

The xfx binary is `target/debug/xfx` built at `89690c0` (the banner says so).

Sanitized: the run directory `/private/tmp/claude-<uid>/fxcap.XXXXXX` is written
`<scratch>` in every file, so the workspace segment reads `<scratch>/work` on
both sides. Nothing else was edited.

`.ansi` files are tmux's re-serialization of cell attributes
(`capture-pane -e`), not the programs' wire bytes (see `../c0/README.md`). So
xfx's `ESC[1;38;5;255m` and fx's `ESC[1m ESC[38;5;255m` both come out as
`\e[1m\e[38;5;255m`.

## Result

In all 11 snaps, rows 2-40 are byte-identical between fx and xfx, in both
`.txt` and `.ansi`, after the `<scratch>` substitution. Row 1 is each binary's
own header (`𝒇x v0.0.13` against `x𝒇x v0.1.0-89690c0 [debug]`), which WU-A
settled.

## Per snap

| snap | matches | differs, and why |
|---|---|---|
| `statusline-bare` | Rows 3-10: the 240 rule where the composer was (no `┃` row), `Status line` in bold 255, a blank row, `Context` / `Session` / `Workspace` with the first marked in bold 255 and the rest in 245, `off` bold and `on` 245 at column 15, the 240 rule, and `↑↓ navigate     ←→ change     esc close` in 245. Nothing is written to the document. | — |
| `statusline-down1` | The mark is on `Session`. The values are unchanged. | — |
| `statusline-enter` | `Session` reads `off` 245 and `on` bold. **No transcript notice**: the picker's flip is silent. | — |
| `statusline-esc` | The picker is gone. The composer row `┃` and the footer `auto · model-a` are back. Session is on, but there is no title yet, so there is no segment. | — |
| `statusline-context` | `* statusline: saved to user settings (scope=user)`, a blank row, then `* statusline: context: on`, all in 250. The footer is still `auto · model-a`, because no turn has measured anything yet. | — |
| `statusline-session` | Two more notices, ending `session: off`, because Enter had turned it on. | — |
| `statusline-workspace` | `workspace: on`. The footer is `auto · model-a · <scratch>/work`: the **full path**, and no branch, because the run directory is not a repository. | — |
| `statusline-bogus` | `✗ usage: /statusline [context|session|workspace]`, with the glyph in 252 and the text in 250. | — |
| `statusline-turn-launch` | A workspace on `main` with every item off: the footer is `auto · model-a` and shows no branch. | — |
| `statusline-turn-all-on` | Three pairs of notices. The footer is `auto · model-a · <scratch>/work (main)`. | — |
| `statusline-turn-after-turn` | The card, the answer, `0s (↑4 ↓40)`, and the footer `auto · model-a · say the marker · 1k · <scratch>/work (main)`. The segments come in fx's order. The context shows the numerator alone, because the Gateway publishes no window. | The session title is the same text on both sides here, but for two different reasons. fx asks the gateway for a generated title, gets an empty answer from the fake, and falls back to the prompt. xfx's title is always the first line of the first prompt (`src/session/store.rs` `title_of`). Against a real gateway, fx would show a generated title and xfx would still show the prompt. |

## Settings file after each write

The values are the same after every write, and so is the partial merge: only
the keys written so far are present.

| after | fx `~/.fx/settings.json` | xfx `~/.xfx/settings.json` |
|---|---|---|
| Enter on Session (`enter`) | `{"statusLine":{"session":true}}` | `{"statusLine":{"session":true}}`, pretty-printed |
| `/statusline context` | `{"statusLine":{"session":true,"context":true}}` | `context: true, session: true` |
| `/statusline session` | `{"statusLine":{"session":false,"context":true}}` | `context: true, session: false` |
| `/statusline workspace` | `{"statusLine":{"session":false,"context":true,"workspace":true}}` | `context: true, session: false, workspace: true` |
| `/statusline bogus` | unchanged | unchanged |

The bytes differ, and why:

- **xfx writes pretty JSON with sorted keys and a trailing newline.** fx writes
  compact JSON in insertion order. xfx's bytes come from the profile's single
  serializer (`profile::write_preference`, the same one `/setup` uses), and
  `serde_json`'s map is sorted.
- **xfx keeps no `backups/` and takes no `settings.lock`.** fx keeps one backup
  per prior version (`~/.fx/backups/settings.json.backup.*`, shown in the
  `.home` files). xfx replaces the file through a staged owner-only rename,
  which `/setup` already uses. A reader never sees half a file, but no
  previous version is kept.

## Not in this receipt (unit tests cover them)

- **The workspace cut to the room left**, e.g. `…oot (detac…)`. See
  `src/tui/hint.rs`
  `the_workspace_is_cut_to_the_room_the_others_left_rather_than_dropped` and
  `src/tui/statusline.rs`
  `the_workspace_segment_keeps_the_end_of_the_path_and_the_start_of_the_branch`.
- **A linked worktree's `gitdir:` file, a detached HEAD
  (`detached:<12 hex>`), a nested workspace inside a repository.** See
  `the_branch_is_read_from_the_nearest_head_and_follows_a_linked_worktree`.
- **The branch being read again at the end of a turn.** See `src/tui/shell.rs`
  `the_branch_is_read_at_launch_and_again_at_the_end_of_a_turn`.
- **A write that fails.** `✗ statusline: active for this process but not saved
  to user settings (<reason>)` appears on both paths, and the change stays in
  force. See `a_write_that_fails_keeps_the_change_for_this_process_and_says_so`.
- **The keys the picker swallows** (letters, Tab, Backspace, Ctrl-C, Ctrl-D),
  and Ctrl-J/Ctrl-K. See
  `the_pickers_keys_move_the_mark_flip_silently_and_close`.
- **A 7-row screen**, which gives the picker 3 rows. See
  `a_short_screen_gives_the_picker_fewer_rows_rather_than_refusing_it`.
- **The line-oriented shell.** See `src/interactive.rs`
  `the_line_shell_flips_and_keeps_a_statusline_item_in_the_tuis_words`, the
  bare-form test and the refusal test.
- **A context window from a catalog** (`12k/200k 6%`). See the `src/tui/shell.rs`
  meter tests. The fake gateway publishes no window.
