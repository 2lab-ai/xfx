# W5 receipt: /workspace

Two scenarios, run at 40x120 on both binaries:

```
FX_BIN=<fx c1db919 build, 0.0.13> scripts/fxcap/capture.sh fx  workspace       40x120 <dir>
                                  scripts/fxcap/capture.sh xfx workspace       40x120 <dir>
FX_BIN=<fx c1db919 build, 0.0.13> scripts/fxcap/capture.sh fx  workspace-index 40x120 <dir>
                                  scripts/fxcap/capture.sh xfx workspace-index 40x120 <dir>
```

- `scripts/fxcap/scenarios/workspace.cap`: bare `/workspace`, Escape, `list`,
  three refusals (`bogus`, `add ../missing`, `remove ../extra` before anything
  is saved), `add ../extra`, then the picker with one saved directory and
  Enter on each of its three choices (Ctrl-U between them, as fx's e2e does,
  `tui-slash-menu.test.ts:2361`), and finally Enter on the prefilled
  `/workspace clear`. `home` dumps after the add and after the clear.
- `scripts/fxcap/scenarios/workspace-index.cap`: `add ../extra` with a file
  in it (`scenarios/workspace-extra/notes/plan.md`), then `@plan` and a bare
  `@`.

`capture.sh` gained one verb, `dir <name> [<from>]`: a directory beside the
workspace, `<run>/<name>`, made before launch (empty, or a copy of
`scenarios/<from>`), so `/workspace add ../<name>` names a directory inside
the run and nowhere else. A name with `/`, `.` or `..` is refused. The shims
(`open`, `pbcopy`, `pbpaste`, `afplay`, `osascript`), `FX_NO_OPEN_BROWSER=1`,
the fx sound guard (exit 0 on every fx run here), the isolated tmux socket and
the empty `HOME` are unchanged.

The xfx binary is `target/debug/xfx` built at `8a9c676` (the merge of
`plan/tui-phase4` `aed448c` into this branch; the banner says so).

Sanitized: the run directory `/private/tmp/claude-501/fxcap.XXXXXX` is
written `<scratch>` in every file. Where SGR runs split that path -- the
`@plan` picker bolds the matched letters, some of them inside the run
directory's name -- the six random characters alone are written `<id>`
(`fxcap.<id>`). Nothing else was edited. Because `<scratch>` is shorter than
the path it replaces, the picker's description column looks closer to the
directory row than it was on the screen; the column was computed from the
real path on both sides, identically.

`.ansi` files are tmux's re-serialization of cell attributes
(`capture-pane -e`), not the programs' wire bytes (see `../c0/README.md`).

## Result

In all 14 snaps, rows 2-40 are byte-identical between fx and xfx, in both
`.txt` and `.ansi`, after the substitution. Row 1 is each binary's own header
(`𝒇x v0.0.13` against `x𝒇x v0.1.0-8a9c676 [debug]`), which WU-A settled.

## Per snap

| snap | matches | differs, and why |
|---|---|---|
| `workspace-bare` | Rows 3-11: the 240 rule where the composer was (no `┃` row), `Workspace` in bold 255, a blank, `  Primary` and `  Additional directories` in bold 252 with `<scratch>/work` and `0 / 16` in 245 at column 28, a blank, `❯ Add directory…` in bold 252 with `Grant access to another directory` in 245, the 240 rule, and `↑↓ navigate     enter use     esc close` in 245. Nothing written to the document. | — |
| `workspace-list` | After Escape: `* workspace: primary=<scratch>/work`, `  saved_suppressed=false limit=16`, `  additional directories: (none)`, all 250. The composer and `auto · model-a` are back. | — |
| `workspace-refusals` | `✗ usage: /workspace [add PATH|remove PATH|clear]` (glyph 252, text 250, no topic); `✗ workspace: Workspace update rejected: directory does not exist` for `add ../missing`; `✗ workspace: Workspace update rejected: path is invalid` for `remove ../extra` -- fx reads a remove that matches no saved spelling as a saved spelling, which must be absolute. | — |
| `workspace-add` | The snapshot with `add ../extra saved_changed=true runtime_changed=true launch_flag_can_restore=false` (the path as typed) and ` - <scratch>/extra saved=true command_line=false available=true active=true` (the path resolved). | — |
| `workspace-picker-one` | `Additional directories    1 / 16`, then the three choices with the description column after the longest label and four columns of gap: `❯ Add directory…` bold 252, `  <scratch>/extra  Active · Saved` and `  Clear saved directories  Remove every saved additional directory` in 245. | — |
| `workspace-picker-down1` | The mark is on the directory row, which is now bold 252 with `❯`. | — |
| `workspace-dir-enter` | The picker is gone and the composer holds `/workspace remove <scratch>/extra`; nothing ran, nothing was written. | — |
| `workspace-add-row-enter` | After Ctrl-U and `/workspace` again, Enter on the first row: the composer holds `/workspace add ` (the caret after the space). | — |
| `workspace-clear-row` | Down twice: the mark on `❯ Clear saved directories`. | — |
| `workspace-clear-prefill` | Enter: the composer holds `/workspace clear`. | — |
| `workspace-clear-done` | Enter on it: `clear saved_changed=true runtime_changed=true launch_flag_can_restore=false` and `additional directories: (none)`. | — |
| `workspace-index-at-plan` | `@plan` lists `<scratch>/extra/notes/plan.md` first, marked on the 239 bar with `plan` bold, then `/private/tmp/c…/extra/notes/` with `p`, `la`, `n` bold in 245: an added directory's entries by their absolute paths, ranked the same way. | — |
| `workspace-index-at-all` | A bare `@` lists `<scratch>/extra/notes/` and `<scratch>/extra/notes/plan.md` after the (empty) workspace's own entries. | — |

## The profile (`.files`)

| dump | matches | differs, and why |
|---|---|---|
| `after-add` | `workspaces["<scratch>/work"].additional_directories == ["<scratch>/extra"]` on both. | xfx writes the profile pretty-printed (its one writer, `profile::write_preferences`, always has) where fx writes it compact; fx also keeps `~/.fx/backups/settings.json.backup.*`, a prompt history file and a session opened at launch, none of which this unit adds (xfx keeps no settings backups, its history is in memory, and its session opens at the first prompt -- each recorded elsewhere). |
| `after-clear` | `settings.json` is `{}` on both: the emptied list took the key, the workspace entry and `workspaces` out with it. | As above. |

## Not captured, and why

- The busy refusal (`* workspace: Workspace changes are unavailable until the
  active and queued work finishes.`) needs a turn held open across a
  keystroke; it is pinned by `tui::shell::tests::a_workspace_change_is_refused_while_a_turn_is_in_hand`.
- An `Unavailable · Saved` row, a saved directory swapped for a symlink, and
  the auto-mode write rules are filesystem states a scenario cannot stage
  between two binaries' launches; each has unit and integration tests
  (`workspace::access`, `workspace::path`, `tui::worker`, `tests/permissions.rs`,
  and `tests/tool_loop.rs` for `ask` reading a saved directory).
