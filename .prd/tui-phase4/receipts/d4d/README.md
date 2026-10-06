# D4d receipt: /settings

Eight scenarios, run at 40x120 on both binaries through the repo's harness
(its shims, `FX_NO_OPEN_BROWSER=1` and the fx sound guard; every run exited 0,
so no chime name was written):

```
FX_BIN=<fx c1db919 build, 0.0.13> scripts/fxcap/capture.sh fx  <scenario> 40x120 <dir>
                                  scripts/fxcap/capture.sh xfx <scenario> 40x120 <dir>
```

for `<scenario>` in `settings`, `settings2`, `settings-perm`, `settings-cats`,
`settings-collapse`, `settings-history`, `settings-model`, `settings-effort`
(`scripts/fxcap/scenarios/`). `settings` and `settings2` are the research
scenarios (`appearance-security-ext/scenarios/settings*.cap`) keystroke for
keystroke, with a `home` dump after each write. Where the two menus list a
different number of settings, the `Down` count is per binary (`fx:`/`xfx:`
lines) so both land on the same row. Every scenario starts xfx with
`XFX_SOUND=off`, as the harness starts fx with `FX_SOUND=0`.

The xfx binary is a debug build of `b36b41a` -- this branch with `plan/tui-phase4`
at `aed448c` merged, E1, F4, W1 and W6 included -- passed as `XFX_BIN` (the
banner says so). fx was captured with the harness at `f0b1caa`, xfx with it at
`b36b41a`;
the two differ only in how a binary file is dumped and in the sound guard's
glob (`cafab50`) and in `relaunch` with no arguments, none of which these
scenarios reach on fx.

A mechanical check over every snap pair (`.ansi`, rows matched by label,
tmux's carried reset at a row's start set aside): of the 272 setting rows xfx
painted that fx painted too, 243 are byte-identical; the other 29 are the 28
`Sound level` rows, identical once fx's trailing `  max` is taken off, and the
`Advanced` tab's `Prompt history`, whose value column is narrower because
`Startup scrollback` is not there to be the widest label.

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

- **R -- the rows xfx does not have.** fx lists 14 settings, xfx 10: `Fast
  mode`, `Ultra mode`, `Session titles` and `Startup scrollback` are absent
  (`../../ssot.md` §Tensions), and `Sound level` is `off  on` without `max`.
  So the header says `Settings 10` (`3` on Agent, `1` on Advanced) where fx
  says `14` (`6`, `2`), the band is that many rows shorter, and the rule and
  the key help sit higher. Every row xfx does have is cell-identical to fx's
  row for it -- label, value column, choices, which is bold, the marked label.
- **T -- turn summaries.** `0s (↑1 ↓40)` against fx's `↓80` after a tool step
  and a text step: the token tally, not this unit (`../f1/`).
- **C -- landed tool groups keep their rows** when collapse is turned on (fx
  re-projects the visible transcript; Tension).
- **P -- the slash popup's own count**, `Commands 17` against fx's 36.
- **M -- the Model row's surface**: fx opens an inline list under the row, xfx
  `/model`'s menu (Tension).
- **S -- `startup-scrollback`**, fx's argument form, which xfx answers with
  `✗ usage: /settings`.
- **H -- the walk's step**: fx's Up after a recall first takes the caret to
  the start of the recalled line and recalls the next older one on the Up
  after that; xfx's recalls on every Up. The lines and their order are the
  same (composer keys, not this unit; reported, not changed).
- **I -- a running command and its interrupt**: xfx's two extra blank rows
  between an approved command's group and the activity row (pre-existing;
  reproduced without `/settings`, collapse off) and xfx's Ctrl-C on a running
  command, which stops the turn when the command returns (`../../ssot.md`
  §Tensions, "Interrupting a running command").

## Per snap

| snap | matches | differs, and why |
|---|---|---|
| `settings-typed` | The popup row `/settings   browse and update settings ... Appearance`, byte for byte. | -- |
| `settings-bare` | The composer `┃` stays above the 240 rule; `Settings N  [All]  Interface  Agent  Notifications  Advanced` with the title and `[All]` bold 255 and the tabs 245; a blank; `Status line context` marked bold 255 with `off` bold and `on` 245 at column 27; every other label 245; `Model  fake/model-a` bold value; `Reasoning effort  default` bold; `Permission mode  ask  auto  full access` with `auto` bold; the rule; `↑↓ navigate     tab category     ←→ change     esc close` in 245. | R |
| `settings-down1`, `-down4` | The bold label moves down one and four rows. | R |
| `settings-esc` | The menu is gone, the composer and `auto · model-a` are back. | -- (identical) |
| `settings-ss-query` | -- | S: fx `* settings: startup_scrollback: off (applies on next launch)`. |
| `settings-bogus` | `✗ usage: …` in 252/250. | S: fx's usage names `[startup-scrollback [on|off]]`; xfx's is `usage: /settings`. Row 3 is the previous step's answer. |
| `settings2-tab1` | `Settings 5  All  [Interface]  …`, the five Interface rows, the column at 27. | -- (cells identical; tmux reset placement only) |
| `settings2-tab2` | `[Agent]`, Model, Reasoning effort, Permission mode at column 22. | R: fx 6 rows (Fast, Ultra, Session titles), xfx 3. |
| `settings2-tab3` | `Settings 1  …  [Notifications]`, `Sound level` at column 17. | R: `off  on` without `max`. |
| `settings2-tab4` | `[Advanced]`. | R: fx `Startup scrollback` and `Prompt history` at column 24; xfx `Prompt history` alone at column 20 (the column follows the widest label on the tab). |
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
| `settings-collapse-collapse-on` | `* collapse tool calls: saved to user settings (scope=user)`. | C: fx's landed group is now its header alone; xfx's keeps its two rows. R. |
| `settings-collapse-after-esc` | Closed. | C, T |
| `settings-collapse-group2` | The new group is `● 1 tool call · 1 read` and a blank row -- no `└ Read a.txt` -- then `Again.` | C (the first group), T |
| `settings-collapse-cmd-ask` | The command's group is its header alone above the approval panel. | C, T; the panel's `$ # terminal.exec cwd=.` against fx's `$ # shell.run …` is the tool's own (`../f2/`). |
| `settings-collapse-running` | The header alone -- no `└ Running sleep 4; echo hi` row -- and the activity row. | C, T, I (two blank rows above xfx's activity row; the same without collapse). |
| `settings-collapse-stopped` | `press ctrl+c again to exit` on the hint row. | I: fx cancels the call at once (`· 1 cancelled`, no activity row); xfx's command is still running (`Running (3s)`). |
| `settings-collapse-later` | The group is its header alone. | I: xfx's command ran to its end, so its header gains no `· 1 cancelled`, and the turn's stop is F4's `■ Cancelled · What can xfx do differently?` under it. |
| `settings-history-marked` | Up from the first row wraps to `Prompt history`. | R |
| `settings-history-off` | `* history: saved to user settings (scope=user)`; `off` bold. | R |
| `settings-history-on` | The same, `on`. | R |
| `settings-history-up-after-secret` | Up recalls `/settings`, not `secret prompt`: a line submitted with the history off is kept nowhere. | -- (identical) |
| `settings-history-relaunch` | A fresh launch on the same HOME. | -- (identical) |
| `settings-history-relaunch-up1` | Up recalls `public prompt` -- the newest line saved before the relaunch. | -- (identical) |
| `settings-history-relaunch-up2..4` | Both walk the saved lines, newest first: `public prompt`, `/settings`, `first prompt`. | H: fx `public prompt`, `/settings`, `/settings`; xfx `/settings`, `first prompt`, `first prompt` (the oldest, where the walk stops). |
| `settings-model-marked` | `Model` marked. | R |
| `settings-model-enter` | -- | M: fx's inline row `fake/model-a` under `Model`; xfx's `Models 1  [All]` menu. |
| `settings-model-esc` | -- | M: fx's Escape closes the settings menu and leaves its model list up as `/model`'s menu (`Models 1  [All]` -- the screen xfx showed one step earlier); xfx's Escape closes `/model`'s menu. |
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
| `/settings startup-scrollback`, `bogus` (`settings-files`) | `{"startup_scrollback":false}` | no file: nothing was written |

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
