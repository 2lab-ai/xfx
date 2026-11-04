# P5-EF2 receipt: the `/effort ` inline column

The unit is the `/effort ` column from fx v0.0.16 (`vercel-labs/fx@481104fd`,
commit `04a5e3af`, E6): an `/effort ` line is `/model`'s effort column on its
own, over the model in force. The spec is
`research/tui-phase5:.prd/tui-phase5/research/effort-fork.md` §6 unit 2, with
its §2-§3 column rules. The base is P5-EF1 with its fix round EF1b
(`receipts/effort-menu/`, `feat/p5-effort-menu` at `3e4ddeca`).

## How it was captured

```
FX_BIN=<fx v0.0.16, sha256 8e6cda2e…> scripts/fxcap/capture.sh fx  p5-effort-fork-effort-column 40x120 <dir>
XFX_BIN=<this branch's target/debug/xfx>  scripts/fxcap/capture.sh xfx p5-effort-fork-effort-column 40x120 <dir>
```

- Both binaries ran with the same `TMPDIR`, outside the repository.
  `capture.sh` launches fx with `FX_AUTO_UPGRADE=0` and checked after the run
  that the reference binary's bytes had not changed (sha256 `8e6cda2e…`).
- **`effort-column` is new.** Its first half is the research run's column
  block, from the same state (the level in use is `default`): `level-upper`
  (`/effort HIGH`, which EF1 left out), then `col-space`, `col-down`,
  `col-nomatch`, `col-tab`, `col-fill` and `col-enter`, the research's keys
  in the research's order. Its second half starts with `low` in use. It
  covers what the first half cannot tell apart: `space-first`/`tab-current`,
  `typed-space`/`typed-down`/`typed-enter`, `col-esc`/`esc-enter` and
  `level-prefix`. `home` snaps hold the settings after each part.
- EF1's seven scenarios (EF1b added `effort-backtab`) were captured again on
  this branch's binary and paired with **EF1's own fx snaps**
  (`../effort-menu/fx/`). The fx binary is the same, so no new fx run was
  needed.

## What the capture settled

- **A space opens the column on its first row, and Tab opens it on the level
  in use.** The research's E6 line says Space marks the level in use. Its
  run cannot show which is right, because `default` was in use there.
  `space-first` (`low` in use) marks `default`, and its hint row reads
  `auto · gpt-6-astra` with no level. `tab-current` marks `low`. Source:
  - a typed space inserts with the `clear` picker policy, and every insert
    puts the mark back on row 0 (`app_input_runtime.zig:1372`,
    `composer_insertion.zig:178-188`, `picker_state.zig:442-462@481104fd`);
  - Tab on a bare `/effort` is `openCurrentEffortPicker`
    (`input_completion_runtime.zig:1312-1328`).
- **`/effort HIGH` is `high`** (the research's "case folded somewhere"):
  - the column's Enter takes the level the text spells, else the marked one
    (`selectedEffortCompletion`, `input_completion_runtime.zig:1491-1503`);
  - no level is spelled `HIGH`, and the filter ignores case and keeps
    `high` and `xhigh`, with the first marked;
  - `/effort med` applies `medium` the same way (`level-prefix`);
  - `bogus`, `max` and `two words` match nothing, so the line runs as the
    command and gets EF1's notices.
- The hint row previews the **marked** row, not the spelled level:
  `typed-down` reads `· xhigh` over `/effort hi`
  (`effort_command_preview`, `app_render_runtime.zig:546-584`).
- A space typed in the column goes nowhere (`typed-space`;
  `advanceModelPickerOnSpace`, `:1221`). Escape puts the column away and
  leaves the line, so Enter then runs the bare command and opens the menu
  (`col-esc`, `esc-enter`).

## RED (EF1's head before the rebase, `6b01fb4c`)

`red/` holds that binary's screens. Its tree is `562040ac`'s plus the
`.pyc` the rebuild dropped. EF1b adds no column, so RED stands. `red.txt` is `cells.py` over every
pair. 16 of the 17 snaps differ, and only `launch` matches:

- `level-upper` row 3: `* effort: HIGH is not available for …` where fx has
  `* Reasoning effort set to high for …`;
- `col-*`: no column under `/effort ` (from row 8 on), and the hint row has
  no preview.

## GREEN

`green.txt` is `cells.py` (`.prd/tui-phase4/receipts/m1/cells.py`) over
every pair, with row 1 (each product's banner) left out. All 51 pairs are
identical:

| scenario | snaps, all identical |
|---|---|
| `effort-column` (`fx/`) | `launch`, `level-upper`, `level-default`, `col-space`, `col-down`, `col-nomatch`, `col-tab`, `col-fill`, `col-enter`, `space-first`, `tab-current`, `typed-space`, `typed-down`, `typed-enter`, `col-esc`, `esc-enter`, `level-prefix` |
| EF1's (`../effort-menu/fx/`) | all 34 of `effort-menu`, `effort-nolevels` (its `col-space`, `col-enter`, `col-tab` among them), `effort-down`, `effort-short`, `effort-loading`, `effort-ctrlc`, `effort-backtab` |

- The xfx binary is `target/debug/xfx`, built from this branch's tree before
  its commit, so its banner names the base revision.
- **After the rebase onto EF1b** (`3e4ddeca`), these were captured again on
  the rebased binary: every scenario that EF1b's changes can move, or that
  the column's model-change rule below can move. That is `effort-column`,
  `effort-menu`, `effort-short`, `effort-loading`, `effort-ctrlc`,
  `effort-backtab` and `effort-nolevels`. Each matched on the first take.
  `effort-down` keeps the earlier take: it types no `/effort ` line and
  applies no level.
- In EF1's `effort-menu`, `/effort <level>` and the mid-turn `/effort high`
  now go through the column's Enter. They still pair, and the gateway bodies
  are unchanged: the slow turn is sent with `"reasoning":"low"`, and the next
  one with `"high"` (`xfx/…-effort-menu-40x120-bodies/002`, `003`).

## The settings file

| after | fx `~/.fx/settings.json` | xfx `~/.xfx/settings.json` |
|---|---|---|
| `col-enter` (`after-col`) | `"effort":"low"` | `"effort": "low"` |
| `typed-enter` (`after-typed`) | `"effort":"xhigh"` | `"effort": "xhigh"` |
| `level-prefix` (`end`) | `"effort":"medium"` | `"effort": "medium"` |

Neither product says `saved to user settings` for these changes.

## A model change under an `/effort ` line

This is EF1b's S3, applied to the column. A model change, a provider switch
or a resume can land while an `/effort ` line is in the composer. When one
does, the column is put away and its mark let go, and the line is left as
typed. Enter then runs the line as the command, for the model now in force.

Before this rule, the result was a probe:

- `xhigh` was marked by Tab on astra;
- opus landed;
- Enter applied opus's fifth row:
  `* Reasoning effort set to max for anthropic/claude-opus-5`.

fx has no such rule. Its changes are commands typed in that composer. The
test is `shell::tests::the_effort_column_goes_when_the_model_or_the_provider_it_was_shown_for_goes`.

## Not here

- A `/model` flow left pending under an `/effort ` line. fx lists the model
  in force there (`app_render_runtime.zig:542-546`). No key path reaches that
  state, so a unit test covers it in place of a capture
  (`model_column::tests::an_effort_line_lists_the_models_in_force_whatever_model_has_pending`).
- The column's page keys and `C-j`/`C-k` were not captured. They are bound
  as for `/model`'s column: Up and Down only.

`.ansi` files are tmux's re-serialization of cell attributes
(`capture-pane -e`). They are not the programs' wire bytes. Local paths are
redacted with `scripts/fxcap/redact.py`.
