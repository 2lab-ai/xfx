# D4d receipt: /settings

Eleven scenarios, run at 40x120 on both binaries through the repo's harness
(its shims, `FX_NO_OPEN_BROWSER=1` and the fx sound guard; every run exited 0,
so no chime name was written):

```
FX_BIN=<fx c1db919 build, 0.0.13> scripts/fxcap/capture.sh fx  <scenario> 40x120 <dir>
                                  scripts/fxcap/capture.sh xfx <scenario> 40x120 <dir>
```

for `<scenario>` in `settings`, `settings2`, `settings-perm`, `settings-cats`,
`settings-collapse`, `settings-history`, `settings-model`, `settings-effort`,
and -- for the follow-ups -- `settings-model-list`, `settings-scrollback`,
`recall-edge` and `settings-collapse-open` (`scripts/fxcap/scenarios/`). `settings` and `settings2` are the research
scenarios (`appearance-security-ext/scenarios/settings*.cap`) keystroke for
keystroke, with a `home` dump after each write. Where the two menus list a
different number of settings, the `Down` count is per binary (`fx:`/`xfx:`
lines) so both land on the same row. Every scenario starts xfx with
`XFX_SOUND=off`, as the harness starts fx with `FX_SOUND=0`.

**Round 2 (the follow-ups).** `settings-collapse-open` was captured on both
binaries with a debug build of `13b59ee`'s tree (the block that shrinks gives its
rows back; built from the working tree before that commit was made, so its
banner -- scrolled off in every snap -- would have said `6ea9808`);
`settings-collapse` was run again on it and differs from the `2c1fe4b` snaps
only in a session id and the activity marker's blink. Every other xfx snap here was taken again with a debug
build of `2c1fe4b` -- this branch with `plan/tui-phase4` at `357eb5e` merged and
the follow-up commits on it (the banner says so); `xfx-toolgroup-40x120-long-*`
are that build's `toolgroup` run, for the blank-row fix, against
`../f1/fx-toolgroup-40x120-long-*`. fx's snaps for the first eight scenarios
are round 1's (fx did not change); the three new scenarios were captured on
fx with the harness at `2c1fe4b`. Round 1's xfx binary was `b36b41a`, its fx
harness `f0b1caa`; the harness differs from it only in how a binary file is
dumped, the sound guard's glob (`cafab50`) and `relaunch` with no arguments,
none of which these scenarios reach on fx.

A mechanical check over every snap pair (`.ansi`, rows matched by label,
tmux's carried reset at a row's start set aside): of the 341 setting rows xfx
painted that fx painted too, 308 are byte-identical, and the other 33 are
`Sound level` rows, identical once fx's trailing `  max` is taken off. The
Model row's list rows (`settings-model-list-list`, `-list-down`, `-enter`) are
byte-identical too, the marked id bold 255 and the rest 245 at column 27.

Sanitized: the run directory is written `<scratch>` in every `.files` dump (the
harness does it); the `.txt`/`.ansi` screens hold no path. `.ansi` files are
tmux's re-serialization of cell attributes, not the programs' wire bytes (see
`../c0/README.md`): where fx's row ends on a coloured run, tmux puts the
foreground reset at the start of the next painted row (`settings2-tab1` rows
5/7, `-filter-sou` rows 11/13) and xfx's comes at the end of its own row; the
cells are the same.

## Result

Row 1 is each binary's own banner everywhere. Beyond it, every difference is
one of these, and each snap below names which:

- **R -- the rows xfx does not have.** fx lists 14 settings, xfx 11: `Fast
  mode`, `Ultra mode` and `Session titles` are absent (`../../ssot.md`
  §Tensions; D2c, which would bring the speed rows, is not in plan), and
  `Sound level` is `off  on` without `max`. So the header says `Settings 11`
  (`3` on Agent) where fx says `14` (`6`), the band is that many rows shorter,
  and the rule and the key help sit higher. Every row xfx does have is
  cell-identical to fx's row for it -- label, value column, choices, which is
  bold, the marked label.
- **T -- turn summaries.** `0s (↑1 ↓40)` against fx's `↓80` after a tool step
  and a text step: the token tally, not this unit (`../f1/`).
- **P -- the slash popup's own count**, `Commands 17` against fx's 36.
- **A -- the approval panel**: xfx's says what choice 2 grants in a dim
  `2 = …` row, two rows here (`../../ssot.md` §Tensions, "Approval scope
  wording"), and names the tool `terminal.exec` (`../f2/`).
- **I -- a running command and its interrupt**: xfx's Ctrl-C on a running
  command stops the turn when the command returns (`../../ssot.md`
  §Tensions, "Interrupting a running command"), so the call settles as run,
  the header gains no `· 1 cancelled`, and the turn's stop is F4's line.

Round 1's C (groups already on the screen kept their call rows), M (the Model
row opened `/model`'s menu), S (`startup-scrollback` was a usage error), H
(Up recalled from anywhere on the first row) and the blank rows under an
approved command's group are gone: each is fx's now, snap for snap below.

## Per snap

| snap | matches | differs, and why |
|---|---|---|
| `settings-typed` | The popup row `/settings   browse and update settings ... Appearance`, byte for byte. | -- |
| `settings-bare` | The composer `┃` stays above the 240 rule; `Settings N  [All]  Interface  Agent  Notifications  Advanced` with the title and `[All]` bold 255 and the tabs 245; a blank; `Status line context` marked bold 255 with `off` bold and `on` 245 at column 27; every other label 245; `Model  fake/model-a` bold value; `Reasoning effort  default` bold; `Permission mode  ask  auto  full access` with `auto` bold; the rule; `↑↓ navigate     tab category     ←→ change     esc close` in 245. | R |
| `settings-down1`, `-down4` | The bold label moves down one and four rows. | R |
| `settings-esc` | The menu is gone, the composer and `auto · model-a` are back. | -- (identical) |
| `settings-ss-query` | `* settings: startup_scrollback: off (applies on next launch)`. | -- (identical) |
| `settings-bogus` | `✗ usage: /settings [startup-scrollback [on|off]]` in 252/250; row 3 is the previous step's answer. | -- (identical) |
| `settings2-tab1` | `Settings 5  All  [Interface]  …`, the five Interface rows, the column at 27. | -- (cells identical; tmux reset placement only) |
| `settings2-tab2` | `[Agent]`, Model, Reasoning effort, Permission mode at column 22. | R: fx 6 rows (Fast, Ultra, Session titles), xfx 3. |
| `settings2-tab3` | `Settings 1  …  [Notifications]`, `Sound level` at column 17. | R: `off  on` without `max`. |
| `settings2-tab4` | `[Advanced]`: `Startup scrollback` and `Prompt history` at column 24. | -- (identical) |
| `settings2-tab5` | Back on `[All]`, first row marked. | R |
| `settings2-right-context` | `* statusline: saved to user settings (scope=user)`, a blank, `* statusline: context: on`, all 250, above the still-open menu; the row's `on` bold. | R |
| `settings2-down-sound` | `Sound level` marked. | R |
| `settings2-sound-right` | `* sound: on`; the row's `on` bold. | R (`max`) |
| `settings2-enter` | Identical to `-sound-right`: Enter on a setting does nothing. | R |
| `settings2-filter-sou` | `sou` in the composer; `Settings 2`; `Slash menu categories` (by its description's "skill sources") and `Sound level`, column at 27. | R (`max`); cells otherwise identical. |
| `settings2-filter-none` | `Settings 0`, the dim `No settings found.` | -- (cells identical) |
| `settings2-esc` | The menu closed and the search emptied. | -- (identical) |
| `settings-perm-marked` | `Permission mode` marked. | R |
| `settings-perm-full` | `! permissions: mode set to full access` (252 glyph and topic, 250 body) above the open menu, whose key help keeps the hint row; `full access` bold. | R |
| `settings-perm-ask` | `* permissions: mode set to ask`, wrapping from full access. | R |
| `settings-perm-esc` | Closed; `ask · model-a`. | -- (identical) |
| `settings-cats-off` | `* slash menu categories: saved to user settings (scope=user)`; the row's `off` bold. | R |
| `settings-cats-popup-off` | Every popup row without its category column: `  /help     show available slash commands` and nothing at the right, same SGR. | P |
| `settings-cats-on` / `-popup-on` | `… saved …` again; the popup's `General`/`Session` column back. | R / P |
| `settings-collapse-group` | `● 2 tool calls · 2 read`, `├ Read a.txt`, `└ Read b.txt`, the answer. | T |
| `settings-collapse-collapse-on` | `* collapse tool calls: saved to user settings (scope=user)`; the group already on the screen is its header alone, `Read both.` moved up under its blank row, the band with it. | R, T |
| `settings-collapse-after-esc` | Closed. | T |
| `settings-collapse-group2` | The new group is `● 1 tool call · 1 read` and a blank row -- no `└ Read a.txt` -- then `Again.` | T |
| `settings-collapse-cmd-ask` | The command's group is its header alone above the approval panel. | T, A |
| `settings-collapse-running` | The header alone -- no `└ Running sleep 4; echo hi` row -- a blank, and the activity row. | T (`↓0` in the activity row is the tally) |
| `settings-collapse-stopped` | `press ctrl+c again to exit` on the hint row. | I: fx cancels the call at once (`· 1 cancelled`, no activity row); xfx's command is still running (`Running (3s)`). |
| `settings-collapse-later` | The group is its header alone. | I: xfx's command ran to its end, so its header gains no `· 1 cancelled`, and the turn's stop is F4's `■ Cancelled · What can xfx do differently?` under it. |
| `settings-history-marked` | Up from the first row wraps to `Prompt history`. | R |
| `settings-history-off` | `* history: saved to user settings (scope=user)`; `off` bold. | R |
| `settings-history-on` | The same, `on`. | R |
| `settings-history-up-after-secret` | Up recalls `/settings`, not `secret prompt`: a line submitted with the history off is kept nowhere. | -- (identical) |
| `settings-history-relaunch` | A fresh launch on the same HOME. | -- (identical) |
| `settings-history-relaunch-up1` | Up recalls `public prompt` -- the newest line saved before the relaunch. | -- (identical) |
| `settings-history-relaunch-up2..4` | `public prompt` again (the first Up takes the caret to its start), then `/settings`, then `/settings` again. | -- (identical) |
| `settings-model-marked` | `Model` marked. | R |
| `settings-model-enter` | The settings menu stays, its window from `Model`, and `fake/model-a` -- the one model -- under the row at column 27, bold. | R |
| `settings-model-esc` | The settings menu closes and its list stays up as `/model`'s menu, `Models 1  [All]`. | -- (identical) |
| `settings-model-list-list` | `models.json`'s four under `Model`: `openai/gpt-6-astra` bold, the others 245. | R |
| `settings-model-list-list-down` | The bold moves to `anthropic/claude-opus-5`. | R |
| `settings-model-list-used` | Left: `* Switched to anthropic/claude-opus-5 (effort: default, speed: normal)` and `* model picker: saved to user settings (scope=user); fresh sessions use the saved model`, the list closed, the window back on top, `Model  anthropic/claude-opus-5`, its effort row `default  low  medium  high  max`. | R (fx's `Fast mode` turns `off  on` for this model) |
| `settings-model-list-enter` | Enter opens it again under the new model. | R |
| `settings-model-list-tab` | Tab: `[Interface]`, the list gone. | -- (identical) |
| `settings-scrollback-row` | `startup` typed: `Settings 1`, `Startup scrollback` alone, `on` bold. | -- (identical) |
| `settings-scrollback-row-off` | Right: `* settings: startup_scrollback: off (applies on next launch)` and nothing else; `off` bold. | -- (identical) |
| `settings-scrollback-toggle`, `-off` | `/settings startup-scrollback` alone says `on`; `/settings STARTUP-SCROLLBACK off` says `off`. | -- (identical) |
| `settings-scrollback-usage` | `startup-scrollback maybe`: `✗ usage: /settings [startup-scrollback [on|off]]`. | -- (identical) |
| `recall-edge-start` | After `/copy` and `/stats`, `half` and Up, then `X`: `┃ Xhalf` -- the Up took the caret to the draft's start. | -- (identical but for `/stats`' byte count) |
| `recall-edge-recalled` | Two Ups: the first to the start again, the second recalls `/stats`. | -- (the same) |
| `recall-edge-recalled-start` | Up, then `Y`: `┃ Y/stats` -- a recalled line's first Up is its start too. | -- (the same) |
| `settings-collapse-open-running` | The approved command's group open: header, `└ Running sleep 4; echo hi`, a blank, the activity row. | T; xfx's screen is two rows further up (A) |
| `settings-collapse-open-collapse-on`, `-after-esc` | Collapse switched on mid-turn: the open group is its header and its blank row, and the notice lands right under them, then the activity row; the first group -- its header already above the screen -- loses its call rows too. | T; xfx's rows are fx's one row lower: fx re-renders the whole screen and pulls an older row down into the space, xfx's top rows stay where they are (`../../ssot.md` §Tensions) |
| `toolgroup-long-ask..-later` (xfx only; fx in `../f1/`) | The approved command's group, a blank row, and the activity row right under it -- no extra blank rows. xfx's screen is two rows further up than fx's (its approval panel is two rows taller, so more went to scrollback). | A, I |
| `settings-effort-bare`, `-marked` | `zai/glm-5.2`: `Reasoning effort  default  low  high`, `default` bold. | R |
| `settings-effort-right1..3`, `-left` | Each change says only `* effort: saved to user settings (scope=user)`; the bold walks `low`, `high`, `default`, and Left back to `high`. | R |
| `settings-effort-no-levels` | After `/model fake/model-a` (`* Switched to fake/model-a (effort: high, speed: normal)`): the row is `default` alone, dim -- `high` is carried over and `default` is the one choice. | R |
| `settings-effort-no-levels-right` | Right puts the default back: `* effort: saved …`, `default` bold. | R |

## Settings file after each write

Values identical after every write; xfx writes pretty JSON with sorted keys and
a trailing newline (shown here compacted), fx compact JSON in insertion order,
and fx keeps every prior version under `~/.fx/backups/` (`../d4b/README.md`).

| after | fx `~/.fx/settings.json` | xfx `~/.xfx/settings.json` |
|---|---|---|
| Right on Status line context (`settings2-right-context`) | `{"statusLine":{"context":true}}` | `{"statusLine":{"context":true}}` |
| Right on Sound level (`settings2-sound-right`) | `+ "notifications":{"turn_end":true,"attention_required":true,"max":false}` | `+ "notifications":{"attention_required":true,"turn_end":true}` (no `max`: D4c) |
| Permission mode -> full access (`settings-perm-full`) | `{"permission_mode":"yolo"}` | `{"permission_mode":"yolo"}` |
| -> ask (`settings-perm-ask`) | `{"permission_mode":"ask"}` | `{"permission_mode":"ask"}` |
| Slash menu categories off / on | `{"slash_menu_categories":false}` / `true` | the same |
| Collapse tool calls on | `{…seed…,"collapse_tool_calls":true}` | `{"collapse_tool_calls":true}` (fx's run was seeded with `toolgroup-ask-settings.json`; xfx's is not -- `XFX_PERMISSION_MODE=ask` instead) |
| Reasoning effort -> low / high / default | `{"models":{"gateway":"zai/glm-5.2"},"provider":"gateway","effort":"low"}`, `"high"`, `"auto"` | the same keys and values |
| … after `/model fake/model-a`, Right (`settings-effort-no-levels-right`) | `"effort":"auto"` and `/model`'s `fast_mode`, `ultrafast_mode`, `fast_mode_model_bound` | `"effort":"auto"`; `/model` writes no speed keys (D2b) |
| Prompt history off / on | `{"prompt_history":{"enabled":false}}` / `true` | the same |
| `/settings startup-scrollback`, `bogus` (`settings-files`) | `{"startup_scrollback":false}` | `{"startup_scrollback":false}` |
| Startup scrollback row off (`settings-scrollback-row-off`), then the command on and off (`-off`) | `{"startup_scrollback":false}` | the same |
| The Model list's Left (`settings-model-list-used`) | `{"models":{"gateway":"anthropic/claude-opus-5"},"provider":"gateway","effort":"auto","fast_mode":false,"ultrafast_mode":false,"fast_mode_model_bound":true}` | `{"models":{"gateway":"anthropic/claude-opus-5"}}` -- what xfx's `/model` writes (D2b: no speed keys, and the effort only when it was chosen) |

## History file

`~/.fx/history.jsonl` and `~/.xfx/history.jsonl` after each step of
`settings-history`, timestamps aside, are the same lines:

| after | both |
|---|---|
| `first prompt` (`-first`) | `{"schema_version":1,"timestamp_ms":…,"workspace_root":"<scratch>/work","text":"first prompt"}` |
| history off (`-off`) | `+ … "text":"/settings"` (the line that opened the menu, kept before the switch) |
| `secret prompt` with it off (`-secret`) | unchanged |
| history on (`-on`) | unchanged: the `/settings` typed while it was off is not kept |
| `public prompt` (`-public`, `-end`) | `+ … "text":"public prompt"` |

Both keep a `history.lock` beside it; xfx's file is `0600` in the `0700`
profile directory.
