# F4 receipt: interrupt, exit hint, Escape interrupt, Ctrl-D, retry row

Scenarios under `scripts/fxcap/scenarios/`, each run on both binaries:

```
FX_BIN=<fx c1db919 build, 0.0.13> scripts/fxcap/capture.sh fx  <scenario> <size> <dir>
XFX_BIN=<this commit's target/debug/xfx> scripts/fxcap/capture.sh xfx <scenario> <size> <dir>
```

| scenario | turns | sizes | what it drives |
|---|---|---|---|
| `interrupt.cap` | `interrupt-words.json` | 40x120 | idle Ctrl-C and its window; Ctrl-C over a stream of un-ended words; the window across the turn's conclusion; the next turn; Escape twice over a running turn |
| `interrupt2.cap` | `interrupt-lines.json` | 40x120, 24x80 | Escape twice over a stream of whole lines; Ctrl-D while an answer streams; Ctrl-C with a draft over a running turn, then again |
| `interrupt-cmd.cap` | `interrupt-cmd-fx.json` / `interrupt-cmd-xfx.json` | 40x120 | Ctrl-C while a command runs (each product's own shell tool, each in its own no-questions mode) |
| `interrupt-retry.cap` | `interrupt-retry.json` | 40x120, 24x80 | two HTTP 500s with the Gateway's error shape then an answer; then a prompt the fake has no turn for (every request a plain-text 500) |
| `interrupt-help.cap` | none | 40x120 | a first Ctrl-C at the `/help` catalog, and the window closing |

The shims (`open`, `pbcopy`, `pbpaste`, `afplay`, `osascript`) and
`FX_NO_OPEN_BROWSER=1` are the harness's own, unchanged. Sanitized: every
scratch run directory (`/private/tmp/claude-501/fxcap.XXXXXX`, in 20 request
bodies) is `<scratch>`. Request bodies are kept for `interrupt` at 40x120 only,
cut to their non-system messages (the history question below); every other
scenario keeps its gateway log.

Row 1 differs in every snap and is not listed again: it is each product's own
banner (`𝒇x v0.0.13` / `x𝒇x v0.1.0-b9c5591 [debug]`; `b9c5591` is the base of
this commit). The activity row's marker blinks (`•` lit for the first half of
each second on both), so a snap may catch it lit on one and dark on the other,
and its token counts differ by a word or two of timing; both are listed as
"blink/count" below and are not a difference in the row.

## The rows, text and SGR

| row | fx | xfx |
|---|---|---|
| cancelled turn | `ESC[38;5;252m■ESC[39m ESC[38;5;255mCancelledESC[39m · What can fx do differently?` | `ESC[38;5;252m■ESC[39m ESC[38;5;255mCancelledESC[39m · What can xfx do differently?` -- the same bytes but the product's name |
| exit hint (hint row, whole) | `ESC[38;5;245mpress ctrl+c again to exitESC[39m` | the same |
| exit hint on the `/help` catalog | `ESC[38;5;245mpress ctrl+c again to exitESC[39m` in place of `ESC[38;5;245m↑↓ navigate     tab category     enter open     esc closeESC[39m` | the same, both rows |
| Escape interrupt cue | `ESC[38;5;252mautoESC[38;5;245m · model-aESC[39m` + padding + `ESC[38;5;245mesc again to interrupt` | `ESC[38;5;252mautoESC[38;5;245m · model-a` + padding + `esc again to interrupt` -- see "differs" below |
| retry row | `ESC[38;5;252m⚠ Provider unavailable · HTTP 500 · server_error: upstream exploded · retrying request in 1sESC[39m` | the same |
| retry row, plain-text body | `ESC[38;5;252m⚠ Provider unavailable · HTTP 500: unexpected request · retrying request in 1sESC[39m` | the same |

(`ESC[39m` is tmux's re-serialization of the reset; xfx writes `ESC[0m`.)

## Per snap

### `interrupt` 40x120

| snap | matches | differs, and why |
|---|---|---|
| `idle-cc` | every row; row 5 is the exit hint, byte for byte | -- |
| `idle-cc-expired` | every row; the row is `auto · model-a` again | -- |
| `mid` | rows 3 and 7-9; nothing of the un-ended words is on the screen on either | row 5 blink/count |
| `cc-0.3` | rows 3, 7, 9: the card, the composer, the exit hint; row 5 is the cancelled line (above) | the product name in the question |
| `cc-1.5` | the same: the turn has concluded and the window still says `press ctrl+c again to exit`, on both | the product name |
| `cc-3.5` | the same rows, the hint row back to `auto · model-a` | the product name |
| `next` | the next turn answered under the cancelled line: card, `After interrupt answer.`, `0s (↑1 ↓40)` | the product name |
| `mid2` | rows 7-19 | the product name (row 5); row 15 blink/count |
| `esc1` | row 19's words and colours: `esc again to interrupt` flush with column 120 in the dim grey | row 15 blink/count; row 19's bytes -- see below |
| `esc2` | every row; the second cancelled line at row 15 and the hint row the session's own (no exit armed by an Escape pair) | the product name, twice |
| `esc2-later` | every row | the product name, twice |

### `interrupt2` 40x120 and 24x80

The interrupt is the same on both: the lines already on the screen stay, then a
blank row, the cancelled line, a blank row, the composer. **Which line the
answer had reached differs** (40x120 `esc2`: fx's last line is `line 17`, xfx's
`line 19`; 24x80 has the same skew): that is the two products' streaming
release rates, not the interrupt -- xfx paces at its own rate
(`src/tui/pacer.rs`), fx paints as lines arrive -- so every row below the
answer is shifted by those two lines, and the table states the rows by content.

| snap | matches | differs, and why |
|---|---|---|
| `mid` | the card and the lines' form | how far the stream had got; blink/count |
| `esc1` | the hint row, `esc again to interrupt` flush right in the dim grey, at both widths (`auto · model-a`, 44 blank cells, the cue, at 80 columns) | stream position; blink/count |
| `esc2`, `esc2-later` | cancelled line, composer, `auto · model-a` | stream position; the product name |
| `cd-streaming` | **Ctrl-D while an answer streams is not an exit on either** (`app_input_runtime.zig:1750`): the session is up, `Generating (1s)` still running | stream position; blink/count |
| `draft` | composer `┃ draft text` under a running turn | stream position |
| `draft-cc` | the draft is gone, the turn still `Generating`, the hint row `auto · model-a` (nothing armed) | stream position; blink/count |
| `draft-cc2` | the second press stopped the turn: a cancelled line under `┃ stream words`, the hint row `press ctrl+c again to exit` | stream position; the product name |

### `interrupt-cmd` 40x120

| snap | matches | differs, and why |
|---|---|---|
| `cmd-running` | the card, the activity row `• Running (2s)` | fx's tool group `● 1 tool call · 1 command` / `└ Running sleep 30` against xfx's `[tool] terminal running` -- F1's rows, not this unit's; mode label `full access` / `yolo` (each product's own name for its no-questions mode) |
| `cmd-cc`, `cmd-cc-2` | a cancelled line, then the composer and the exit hint; the command was stopped (no further request on either gateway log) | fx's line names the command (`■ Cancelled sleep 30 · What can fx do differently?`, `tool_group_projection.zig:2166`) and its group says `· 1 cancelled` / `└ Cancelled sleep 30`; xfx's says `■ Cancelled · …` because its tool event carries no command (`UiEvent::ToolStart { call_id, tool }`) -- the summary needs F1's tool payload |

### `interrupt-retry` 40x120 and 24x80

xfx's retry policy is unchanged: three attempts a step, 250 ms then 500 ms
(`src/agent/machine.rs` `retry_delay`); fx backs off about a second, growing,
and does not give up (`model_response_recovery.zig:3-9`). The snap times catch
xfx's row; fx's row at the same times is the same state where both are
retrying.

| snap | matches | differs, and why |
|---|---|---|
| `r0.15`, `r0.45` (40x120) | every row: the retry row byte for byte (above) | -- |
| `r1.05` (40x120) | -- | xfx's third attempt has already answered (`Recovered answer.`, `1s (↑2 ↓40)`); fx is still counting down its first backoff. Policy, not presentation |
| `r2.5` | every row: both recovered, same answer and summary | -- |
| `f0.15`, `f0.55` (40x120) | every row: `⚠ Provider unavailable · HTTP 500: unexpected request · retrying request in 1s` | -- |
| `f2.5` | rows 1-9 | xfx spent its three attempts and wrote `✗ system: the Gateway returned HTTP 500: unexpected request` (its terminal failure, unchanged); fx is still retrying. Gateway logs: three completions on the second prompt on each by the end of the run -- xfx's last, fx's so far |
| `r0.15`, `r0.45`, `r1.05` (24x80) | the row's text and colour up to column 80 | **fx wraps the label onto a second row** (`  request in 1s`, two-column indent, up to three rows: `max_static_status_activity_rows`, `render_input.zig:406`, `:690-697`); **xfx cuts it at column 80** (`…· retrying r`). xfx's activity row is one geometry row (`layout.rs` `Geometry::activity`); a multi-row activity block is a layout change not made here |
| `f0.15` (24x80) | the row | fx caught between attempts (`retrying request`), xfx in its countdown (`in 1s`): timing |

### `interrupt-help` 40x120

`help-open`, `help-cc`, `help-cc-expired`: the catalog stays up under a first
Ctrl-C on both, its key-help row becomes `press ctrl+c again to exit` in the
statusline grey, and the key help comes back when the window closes -- the hint
row byte for byte in all three snaps. Every other row differs by the two
registries (fx's 36 commands, xfx's 10), as in `receipts/c1`.

## What differs and is not fixed here

- **The blank cells before the Escape cue.** fx closes the row's run before the
  padding (`input_presentation.zig:559-567`); xfx closes it after the padding
  and before the cue, so tmux reads the padding cells as grey-245 blanks and
  prints no SGR change before the cue. The cue's own colour is fx's `dim_style`
  in both palettes (pinned by
  `the_right_hand_cue_is_painted_in_the_dim_grey_after_the_rows_run_is_closed`);
  the blank cells look the same. Moving the reset in front of the padding is a
  change to `hint::row`'s right-slot rule, which `esc again to clear` shares.
- **What the model is told about a stopped turn.** fx sends the partial answer
  with `The previous response ended before completion.` and a `<turn_aborted>`
  user message (`fx-interrupt-40x120-bodies/003-completion.json`); xfx sends the
  two user prompts and nothing of the stopped turn
  (`xfx-interrupt-40x120-bodies/002-completion.json`). That is the runtime's
  record (`session.zig:53-56`, `:63` upstream), not this screen.
- **`✓ recovered · attempt N`** for 1.5 s after a recovery
  (`worker_status.zig:5`, `:221-235`) is not shown; the row goes back to the
  turn's own on the first sign the attempt got through. Neither capture caught
  fx's at these intervals.
- **Escape pausing a retry** (`app_input_runtime.zig:3163-3167`) and the
  `waiting for connection` / `recovery paused` labels: xfx has no pausable
  recovery to pause.
- **Submitting while busy (§4.2).** Not changed: xfx's queue (one prompt,
  printed as a card at once, `queued 1` on the hint row) stands. fx's steer
  needs three things xfx lacks -- interrupt-and-continue of a tool-free answer
  with a `<user_steering>` message (`tui-interrupt-recovery.test.ts:291`), a
  steer delivered at a tool boundary inside the same turn, and Up withdrawing a
  queued item (the runtime takes the queued prompt at the top of `turn_loop` the
  moment a turn ends, so a withdraw needs a control round-trip the runtime
  acknowledges) -- and its `┋` row needs a band row the layout solver does not
  have.
