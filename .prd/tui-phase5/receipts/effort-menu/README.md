# P5-EF1 receipt: `/effort` command and menu

The unit is `/effort` from fx v0.0.16 (`vercel-labs/fx@481104fd`, commit
`04a5e3af`): the registry row, the Effort menu, `/effort <level>` with its
notices, and the announcing effort change. The spec is
`research/tui-phase5:.prd/tui-phase5/research/effort-fork.md` §6 unit 1 (E1-E5,
E7-E10). The `/effort ` inline column (E6) is P5-EF2's and is not driven here.

## How it was captured

Seven scenarios, four of them the research branch's `p5-effort-fork-*` as they are:

```
FX_BIN=<fx v0.0.16, sha256 8e6cda2e…> scripts/fxcap/capture.sh fx  p5-effort-fork-<s> <size> <dir>
XFX_BIN=<this branch's target/debug/xfx>  scripts/fxcap/capture.sh xfx p5-effort-fork-<s> <size> <dir>
```

- `effort-menu`, `effort-nolevels`, `effort-down`, `effort-loading`,
  `effort-ctrlc` and `effort-backtab` run at 40x120. `effort-short` runs at
  12x120.
- Both binaries were run with the same `TMPDIR`, outside the repository.
- `capture.sh` launches fx with `FX_AUTO_UPGRADE=0` and checked after each
  run that the reference binary's bytes had not changed (sha256 `8e6cda2e…`
  before and after).

`effort-nolevels`, `effort-down` and `effort-short` are the research's
scenarios unchanged. Four are this unit's:

- **`effort-menu` is EF1's copy.** It drops the research run's `col-*` block
  (the `/effort ` column is EF2's) and its `level-upper` snap. fx applies
  `/effort HIGH` through that column's Enter, not through the command:
  `app_input_runtime.zig:1421@481104fd` (`hasModelQuery` ->
  `submitModelPicker`) -> `submitEffortPicker`, whose filter ignores case
  (`input_completion_runtime.zig:1476`). This resolves the research's
  "case folded somewhere [unverified]". In the column's place,
  `/effort low` is applied, so the slow turn runs on `low` as it did in the
  research run. The copy adds `slash-eff`, `menu-edit` (a typed `x` closes the
  menu and stands), and a `home` snap after every change.
- **`effort-loading` is new.** It uses `modelsdelay 4000`: each catalog read
  answers 4 s after it is asked. It reaches the loading notice
  (`session_commands.zig:1004-1006`), then the menu once the launch's read
  has landed. The harness verb existed on this base (`capture.sh:38`).
- **`effort-ctrlc` is new.** A first Ctrl-C over the menu: the menu stays,
  and its key help gives way to `press ctrl+c again to exit`
  (`composeEffortMenuHintRow`'s `ctrl_c_pending`,
  `input_presentation.zig:671-679`), still so 2.8 s later.
- **`effort-backtab` is new (EF1b).** Shift-Tab twice with the menu up: the
  menu does not bind it, so the permission mode cycles (auto -> full access
  -> ask) and the menu stays (`.toggle_permission_mode`,
  `app_input_runtime.zig:1048-1057`).

## RED (the base, `e0d97bd5`)

`red/` holds the base binary's screens. `red.txt` is `cells.py` over every
pair. 33 of the 34 snaps differ. Only `launch` matches.

- Bare `/effort` answers `✗ command: Unknown command. Try /help.`
  (`red/xfx-…-menu.txt`, row 3).
- `/effort high` and the other forms are sent to the model as prompts
  (`red/xfx-…-level-high.txt`, rows 11-15).
- The slash popup finds nothing for `/ef`.

## GREEN

`green.txt` is `cells.py` (`.prd/tui-phase4/receipts/m1/cells.py`) over
every `fx/` and `xfx/` pair. All 34 are identical, row 1 (each product's
banner) left out.

| scenario | snaps, all identical |
|---|---|
| `effort-menu` | `launch`, `slash-ef`, `slash-eff`, `menu`, `menu-down`, `menu-wrap`, `menu-esc`, `menu-enter`, `menu-edit`, `level-high`, `level-bogus`, `level-max`, `level-malformed`, `level-default`, `level-low`, `midturn`, `midturn-done`, `after-turn` |
| `effort-nolevels` | `bare`, `level`, `col-space`, `col-enter`, `col-tab` |
| `effort-down` | `early`, `later` |
| `effort-short` | `menu`, `menu-down5`, `menu-down6` |
| `effort-loading` | `early`, `later` |
| `effort-ctrlc` | `ctrlc`, `ctrlc-later` |
| `effort-backtab` | `backtab`, `backtab-esc` |

The xfx binary is `target/debug/xfx` at `2630f11` (EF1b), its banner
`v0.1.0-2630f11`. EF1b's changes -- the effort-only change written again and
handed back to the band, and the menu closed when its model goes -- can move
only scenarios that open the menu or apply a level, so `effort-menu`,
`effort-short`, `effort-loading`, `effort-ctrlc` and the new `effort-backtab`
were captured again on it: 27 pairs, all identical. `effort-nolevels` and
`effort-down` open no menu and apply no level; their xfx screens are the
EF1 take (banner `e0d97bd`, the base, as that build stamped it).

`midturn` snaps the running turn's live output count (`Generating (2s)
(↑4 ↓7)`) while the slow turn's chunks arrive 700 ms apart: one EF1 take of
xfx read `↓6` there, every other cell matching; every other take matched
whole.

In this fx run, `slash-ef` lists `/effort` alone (`Commands 1`). The
research's capture listed `/fork` too. Both pair here.

`effort-nolevels`' `col-*` snaps pair without EF2: fx shows no column for a
model without levels (`model_query_active` is false,
`app_render_runtime.zig:750`), and Enter runs the command.

## The settings file after each change

`fx/…-<label>.files` and `xfx/…-<label>.files` hold the whole HOME at each
`home` snap. The two products write the same keys and values. xfx writes its
own file shape, pretty-printed.

| after | fx `~/.fx/settings.json` | xfx `~/.xfx/settings.json` |
|---|---|---|
| `menu-enter` | `"effort":"medium"` | `"effort": "medium"` |
| `/effort high` | `"effort":"high"` | `"effort": "high"` |
| `/effort default` | `"effort":"auto"` | `"effort": "auto"` |
| `/effort low` | `"effort":"low"` | `"effort": "low"` |
| mid-turn `/effort high` | `"effort":"high"` | `"effort": "high"` |

Neither product says `saved to user settings` for these changes.

## The gateway bodies

The slow turn is sent on `low`. `/effort high` is given while it runs:
`* Next turn will use reasoning effort high for openai/gpt-6-astra`. The
running turn keeps `low`, and the next request carries `high`.

| request | fx | xfx |
|---|---|---|
| slow turn | `fx/…-bodies/001-completion.json` `"reasoning":"low"` | `xfx/…-bodies/002-completion.json` `"reasoning":"low"` |
| after the change | `002-completion.json` `"reasoning":"high"` | `003-completion.json` `"reasoning":"high"` |

xfx's `001-title.json` is its session-title request. It is not part of this
unit.

## Not here

- The `/effort ` column (E6): P5-EF2.
- `/fork`, which shares the research doc: P5-FK1 and P5-FK2.
- Upstream's click after the announcement (`playInteractionSound`,
  `main.zig:800-802`): xfx ships no chimes.

`.ansi` files are tmux's re-serialization of cell attributes
(`capture-pane -e`). They are not the programs' wire bytes. Local paths are
redacted with `scripts/fxcap/redact.py`.
