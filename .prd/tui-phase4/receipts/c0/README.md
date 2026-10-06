# C0 receipt: notices, slash dispatch, window title

Scenario `scripts/fxcap/scenarios/notices.cap`, run at 40x120 on both binaries:

```
FX_BIN=<fx c1db919 build, 0.0.13> scripts/fxcap/capture.sh fx  notices 40x120 <dir>
                                  scripts/fxcap/capture.sh xfx notices 40x120 <dir>
```

It types `/version`, `/version now`, `/nosuch` and `/version ` (trailing space),
each followed by Enter, and snaps the screen after each one. It also records the
window title (`#{pane_title}`) at launch, before `/quit` and after `/quit`. The
`title` and `keep` verbs were added to `capture.sh` for this receipt. The shims
(`open`, `pbcopy`, `pbpaste`, `afplay`, `osascript`) and `FX_NO_OPEN_BROWSER=1`
are unchanged.

The xfx binary is `target/debug/xfx`, built from the working tree of this
commit. Its banner shows `014f57f` because that commit is the base.

Sanitized: no scratch path appears in any capture. The `/quit` title on xfx was
the capture host's name, which is shown as `<host>`.

`.ansi` files are tmux's re-serialization of cell attributes
(`capture-pane -e`), not the programs' wire bytes. xfx writes a notice label as
two runs (`{252}✗ {reset}{252}command:{reset}{250} Unknown…`). fx writes one run
(`{252}✗ command:{reset}{250} Unknown…`). The cells are the same, so tmux
serializes both identically. A raw-wire diff would show the difference; a
paired capture does not.

## Per snap

| snap | fx | xfx | matches |
|---|---|---|---|
| `version` | `* version: 0.0.13` | `* version: 0.1.0` | Row 3 matches byte for byte, including the SGR (`ESC[38;5;250m* version: …ESC[39m`), apart from the version number. Rows 2 and 4 are blank on both. There is no echo of `/version` on either. |
| `version-now` | `✗ command: Unknown command. Try /help.` | same | Row 5 matches byte for byte. The SGR is `ESC[38;5;252m✗ command:ESC[38;5;250m Unknown command. Try /help.ESC[39m`. The blank row between the two notices is shared on both, so there is one blank row, not two. |
| `nosuch` | same notice again | same | Matches byte for byte (row 7). |
| `version-trailing-space` | `* version: 0.0.13` | `* version: 0.1.0` | Row 9 matches apart from the version number. The trailing space is tolerated on both. |
| `launch.title` | `fx v0.0.13 \| work` | `xfx v0.1.0 \| work` | Same shape: `<name> v<version> \| <workspace basename>`. |
| `before-quit.title` | `fx v0.0.13 \| work` | `xfx v0.1.0 \| work` | Same. No prompt was sent, so neither session has a title of its own. |
| `quit.title` | (empty) | `<host>` | **Differs on purpose.** Both write `ESC]2;BEL` on exit. xfx then pops the title it pushed at launch (`XTWINOPS 23;2`). tmux implements the title stack, so the pane gets back the title it had before xfx started (tmux's default, the host name). fx has no push, so its pane is left empty. On a terminal without a title stack, xfx's pop is ignored and the clear is what remains, which matches fx. |

The only row that differs in every `.txt`/`.ansi` pair is row 1, the welcome
line, which is each product's own banner (`𝒇x v0.0.13` vs
`x𝒇x v0.1.0-014f57f [debug]`). The version number in the notices differs as
expected. Every other row is identical in both files of each pair: the notices,
the blank rows, the composer rail and the statusline.

## Not in this receipt

- **A session title in the window.** xfx sets the window title to the first
  prompt's title once that prompt's turn finishes, and resets it to the fallback
  on `/new` and on a provider switch. The unit tests in `src/tui/shell.rs`
  cover this: `the_window_title_is_the_workspace_until_the_first_turn_titles_the_session`,
  `dropping_the_conversation_takes_the_window_title_back_to_the_workspace` and
  `a_failed_first_turn_does_not_title_the_session`. This scenario sends no
  prompt.
- **Notices in the light palette, wrapping and retint.** The unit tests in
  `src/tui/notice.rs`, `transcript.rs`, `theme.rs`, `shell.rs` and
  `event_loop.rs` cover these.
