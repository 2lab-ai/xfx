# D4C receipt: /sound

Two scenarios, run at 40x120 on both binaries:

```
FX_BIN=<fx c1db919 build, 0.0.13> scripts/fxcap/capture.sh fx  sound         40x120 <dir>
                                  scripts/fxcap/capture.sh xfx sound         40x120 <dir>
FX_BIN=<fx c1db919 build, 0.0.13> scripts/fxcap/capture.sh fx  sound-default 40x120 <dir>
                                  scripts/fxcap/capture.sh xfx sound-default 40x120 <dir>
```

`scenarios/sound.cap` (turns: `scenarios/sound-turns.json`) starts both binaries
with sound off -- fx through the harness's own `FX_SOUND=0`, xfx through
`XFX_SOUND=off` -- and in ask mode. It runs one turn, then types `/sound`,
`/sound on`, `/sound max`, `/sound off`, `/sound` and `/sound bogus`, then a
turn, then a turn that asks to write `c.txt` (answered `1`), then `/sound off`
and a last turn. `scenarios/sound-default.cap` starts both with nothing setting
the level and types one `/sound`.

The xfx binary is `target/debug/xfx`, built from the working tree of this
commit. Its banner shows `b9c5591` because that commit is the base.

## What was added to capture.sh

- `files <label>`: every file under the run's HOME except session logs, to
  `<bin>-<scenario>-<size>-<label>.files`, with the run's scratch path replaced
  by `<scratch>`. This is where the settings file is read after each write.
- `bells <label>`: the number of terminal bells the pane has rung so far, and
  the number of tmux clients attached, to `<bin>-<scenario>-<size>-<label>.bells`.
  The count comes from a tmux `alert-bell` hook that capture.sh now sets on
  every run (`run-shell 'echo bell >> $run/bells'`). Before relying on it, it
  was probed on tmux 3.7b with a pane that printed a bare `BEL`, a window title
  (`ESC ] 2 ; title BEL`), and a second bare `BEL`. The hook counted 2, so a
  title's terminator is not counted. The probe also showed `bell-action any`,
  `monitor-bell on` and no clients attached.
- A header paragraph about fx's player. fx plays its cues through
  `/usr/bin/afplay` by absolute path, so the PATH shim for `afplay` does not
  intercept it. Both scenarios set `fx: env TMPDIR=/var/empty`, a directory fx
  cannot write. fx then cannot materialize its chime file and rings the
  terminal bell instead (`sound.zig:40-62`, `:176-181`). Nothing played on the
  capture host, and fx's bells are what this receipt compares with xfx's.

The shims (`open`, `pbcopy`, `pbpaste`, `afplay`, `osascript`) and
`FX_NO_OPEN_BROWSER=1` are unchanged.

**`XFX_SOUND=off` is not set for xfx runs in capture.sh.** A bell from a pane
in a capture's tmux server reaches no real terminal, because no client is
attached: `clients=0` in every `.bells` file here. A bell also changes no cell,
so the screens are the same either way. Each scenario sets the start level
itself where it matters (`xfx: env XFX_SOUND=off` in `sound.cap`).

Sanitized: no scratch path appears in any `.txt`, `.ansi` or `.bells` file. The
`.files` dumps have the run directory replaced by `<scratch>` by the `files`
verb. Nothing else was replaced.

## Per snap: the notices

| snap | fx row | xfx row | matches |
|---|---|---|---|
| `bare` (`/sound`, from off) | `* sound: on` | same | Row 9 is byte for byte the same: `ESC[38;5;250m* sound: onESC[39m`. Neither echoes the command, and neither prints a "saved" line. |
| `on` (`/sound on`) | `* sound: on` | same | Row 11 is byte for byte the same. |
| `max` (`/sound max`) | `* sound: max` | `✗ usage: /sound [on|off]` | **Differs by design.** xfx does not ship `max`, because its only effect is fx's UI-cue chimes. The xfx row is the usage line in fx's error shape: `ESC[38;5;252m✗ ESC[38;5;250musage: /sound [on|off]ESC[39m`. |
| `off` (`/sound off`) | `* sound: off` | same | Row 15 is byte for byte the same. |
| `bare2` (`/sound`, from off) | `* sound: on` | same | Row 17 is the same. A toggle from off goes to `on` on both, never to `max`. |
| `bogus` (`/sound bogus`) | `✗ usage: /sound [on|off|max]` | `✗ usage: /sound [on|off]` | Same SGR and the same shape (glyph and label `252`, body `250`, no topic). Only the grammar differs: xfx has no `max`. |
| `toggled` (`sound-default`, `/sound` with nothing set) | `* sound: off` | same | The whole screen is byte for byte the same apart from row 1, the banner. Both start on, which is fx's macOS default (`sound.zig:9-11`), so the toggle goes to off. |

In every `bare`, `on`, `off`, `bare2` and `bogus` pair, the only rows that
differ are row 1 (each product's banner) and the `max` and `usage` rows above.
The prompt card, the answer, the turn summary `0s (↑4 ↓40)`, the composer and
the statusline `ask · model-a` are all identical.

## The settings file after each write

| after | fx `~/.fx/settings.json` | xfx `~/.xfx/settings.json` |
|---|---|---|
| `start` | none | none |
| `bare` | `{"notifications":{"turn_end":true,"attention_required":true,"max":false}}` | `{"notifications": {"attention_required": true, "turn_end": true}}` (pretty, 2-space, trailing line break) |
| `on` | unchanged | unchanged values (rewritten) |
| `max` | `{…"turn_end":true,"attention_required":true,"max":true}` | unchanged (usage, nothing written) |
| `off` | `{…"turn_end":false,"attention_required":false,"max":false}` | `{"notifications": {"attention_required": false, "turn_end": false}}` |
| `bare2` | `{…true,true,"max":false}` | `{…true, true}` |
| `bogus` | unchanged | unchanged |
| `off2` | `{…false,false,"max":false}` | `{…false, false}` |
| `sound-default` `toggled` | `{"notifications":{"turn_end":false,"attention_required":false,"max":false}}` | `{"notifications": {"attention_required": false, "turn_end": false}}` |

The two products store the same switches with the same values. They differ in
four ways:

- **`max` is not written by xfx.** xfx does not read it, so it does not claim
  it. A `max` already in the file is kept, because the writer sets one key at a
  time (`src/sound.rs` `a_save_writes_both_switches_and_keeps_the_rest_of_the_file`).
- **Layout.** fx writes compact JSON. xfx writes pretty JSON with a trailing
  line break, which is the document discipline `setup` already uses
  (`provider::profile::write_preference`). xfx's keys come out sorted,
  `attention_required` before `turn_end`, because `serde_json` here is built
  without `preserve_order`.
- **Backups.** fx keeps every prior version as
  `~/.fx/backups/settings.json.backup.<ms>-<seq>-<hash>`, as the `.files`
  dumps show. xfx keeps none: its writer is a staged `0600` file and a rename.
- **Number of writes.** xfx saves the level as two preference writes,
  `turn_end` then `attention_required`. fx writes one patch. fx also skipped
  the write for `/sound on` when the switches were already on: no backup
  appears for that step. xfx rewrites the same values.

## The bells

Cumulative counts from the `alert-bell` hook. Each `.bells` file is read 0.3 s
after the snap before it.

| checkpoint | fx | xfx | the event |
|---|---|---|---|
| `turn-off` | 0 | 0 | A turn ends with sound off: no bell. |
| `before-turn` | 1 | 0 | **Differs.** fx rang once between `/sound max` and `/sound off`: one of the UI cues `max` adds. The likely one is the slash menu's `toggle` (`src/main.zig:807-813`), which became a bell because its chime could not be written. xfx has no `max`, so it has no UI cues. |
| `turn-on` | +1 | +1 | A turn that answered, with sound on: one bell at its end, once the answer was on the screen. |
| `ask` | +1 | +1 | The permission question opened: one bell. fx rings `BEL` before its chime for every question (`playAttention`, `sound.zig:167-173`). |
| `answered` | +1 | +1 | The approved turn ended: one bell. |
| `turn-off2` | +0 | +0 | After `/sound off`: no bell. |
| `sound-default` `start` | 1 | 0 | **Differs.** fx starts on and plays its start-up cue (`bloom`, `app_entry_runtime.zig:327`), which became a bell here. xfx rings for turn ends and questions only. |
| `sound-default` `toggled` | 1 | 0 | `/sound` itself rings nothing on either. |

`clients=0` in every file: none of these bells reached a terminal.

xfx's `ask` and `answered` screens differ from fx's in the approval panel and
tool rows: the scope sentence, the `wheel scroll` key, and the `[tool] write_file`
rows where fx draws a tool group card. Those surfaces belong to other units (the
approval layout and the tool groups) and are unchanged by this one. The
`/sound` rows on those screens match.

## What differs and is not on these screens

- **Chimes.** On macOS with a writable `TMPDIR`, fx plays an AAC chime through
  `/usr/bin/afplay` at a turn's end, and rings no bell there: it falls back to
  the bell only when the chime cannot be played (`sound.zig:176-200`). For a
  question it rings the bell **and** plays a chime. xfx ships no chimes and
  spawns no player, so it rings the bell in both places. That bell is what fx
  does everywhere except macOS, and what fx does on macOS for a question.
- **fx's other `on` cues** are start-up (`bloom`), cancel (`press`) and model
  switch (`click`) (`src/main.zig:795-805`). They are chimes on macOS and bells
  elsewhere. xfx rings for none of them: the bell marks the two moments the
  user is waited on, a turn's end and a question.
- **A failed save** says `✗ sound: active for this process but not saved to
  user settings (<reason>)` on both (`session_commands.zig:110-133`). fx says
  it before `* sound: <level>`, because fx saves before it answers. xfx saves
  on a writer thread of its own, so the UI thread never waits on the disk, and
  says it when that thread reports the failure, after the level. Not captured;
  `src/tui/shell.rs` `a_save_that_failed_says_so_after_the_level` proves it.
- **Where the bell is written.** fx writes the byte through its frame sink
  (`writeNotificationBell`, `runtime.zig:10152-10163`). xfx writes it as its own
  vector after the tick's frame. The output check accepts that vector only as a
  declared bell intent, and refuses a bell inside any other vector
  (`src/tui/check.rs` `a_bell_is_in_the_alphabet_only_where_it_is_declared`).
  `/stats`' `ansi_bytes` counts it on both.
