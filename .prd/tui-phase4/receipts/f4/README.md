# F4 receipt: interrupt, exit hint, Escape interrupt, Ctrl-D, retry row

Captured on the branch **after** merging `plan/tui-phase4` at `2870b42` (F1 tool
groups, C2 reset, F3 questions, D4b statusline), so every row below is the
merged product. Re-checked on the branch head `ab08b05` after the later merges
(D2a, T1, F5, D3, T2, D4a, D4c; xfx only, every scenario at 40x120): every
interrupt, hint, question and retry row is the same as here; the only changes
are blink/count, stream position and retry timing (as below), the catalog's
rows (the registry grew), and xfx's no-questions mode now named `full access`
as fx's is (D4a). `interrupt-steer` was captured on that head. What the model is told after an
interrupt, and the full transcript's recovery records, were captured on both
products at `ddca9d9` (after merging plan `87c9ed9`): `interrupt-next-question`,
`interrupt-next-cmd`, `interrupt-recovery`, and xfx's `interrupt` bodies and
gateway log (its snaps are unchanged from the earlier capture but for the
banner). Scenarios under
`scripts/fxcap/scenarios/`, each run on both binaries:

```
FX_BIN=<fx c1db919 build, 0.0.13> scripts/fxcap/capture.sh fx  <scenario> <size> <dir>
XFX_BIN=<this branch's target/debug/xfx> scripts/fxcap/capture.sh xfx <scenario> <size> <dir>
```

| scenario | turns | sizes | what it drives |
|---|---|---|---|
| `interrupt.cap` | `interrupt-words.json` | 40x120 | idle Ctrl-C and its window; Ctrl-C over a stream of un-ended words; the window across the turn's conclusion; the next turn; Escape twice over a running turn |
| `interrupt2.cap` | `interrupt-lines.json` | 40x120, 24x80 | Escape twice over a stream of whole lines; Ctrl-D while an answer streams; Ctrl-C with a draft over a running turn, then again |
| `interrupt-cmd.cap` | `interrupt-cmd-fx.json` / `interrupt-cmd-xfx.json` | 40x120 | Ctrl-C while a six-second command runs (each product's own shell tool, each in its own no-questions mode), and after it would have ended |
| `interrupt-question.cap` | `interrupt-question.json` | 40x120 | Ctrl-C at a model's question |
| `interrupt-retry.cap` | `interrupt-retry.json` | 40x120, 24x80 | two HTTP 500s with the Gateway's error shape then an answer; then a prompt the fake has no turn for (every request a plain-text 500) |
| `interrupt-help.cap` | none | 40x120 | a first Ctrl-C at the `/help` catalog, and the window closing |
| `interrupt-steer.cap` | `interrupt-steer-fx.json` / `interrupt-steer-xfx.json` | 40x120 | a prompt submitted while an answer streams, and while a command runs |
| `interrupt-next-question.cap` | `interrupt-question.json` | 40x120 | Ctrl-C at a model's question, then a new prompt: the request it sends |
| `interrupt-next-cmd.cap` | `interrupt-cmd-fx.json` / `interrupt-cmd-xfx.json` | 40x120 | Ctrl-C while a six-second command runs, then (after it would have ended) a new prompt: the request it sends |
| `interrupt-recovery.cap` | `interrupt-retry.json` | 80x120 (tall enough for every record) | two HTTP 500s then an answer, then Ctrl-O: the full transcript's `recovery:` records |

The shims (`open`, `pbcopy`, `pbpaste`, `afplay`, `osascript`) and
`FX_NO_OPEN_BROWSER=1` are the harness's own, unchanged. Sanitized: the scratch
run directory appeared only in the system prompts of the request bodies; bodies
are kept for `interrupt` at 40x120 only, cut to their non-system messages (the
history question below), and no scratch path is left in any capture file. Every other scenario keeps its gateway log.

Row 1 differs in every snap and is not listed again: each product's own banner
(`𝒇x v0.0.13` / `x𝒇x v0.1.0-b9c5591 [debug]`). The activity row's marker blinks
(`•` lit for the first half of each second on both), so a snap may catch it lit
on one and dark on the other, and its token counts differ by a word or two of
timing: both are "blink/count" below and are not a difference in the row.
"Name" is the one word of the cancelled line that is the product's own (`fx` /
`xfx`).

## The rows, text and SGR

| row | fx | xfx |
|---|---|---|
| cancelled turn | `ESC[38;5;252m■ESC[39m ESC[38;5;255mCancelledESC[39m · What can fx do differently?` | the same bytes, `xfx` for `fx` |
| exit hint (hint row, whole) | `ESC[38;5;245mpress ctrl+c again to exitESC[39m` | the same |
| exit hint on the `/help` catalog | `ESC[38;5;245mpress ctrl+c again to exitESC[39m` in place of `ESC[38;5;245m↑↓ navigate     tab category     enter open     esc closeESC[39m` | the same, both rows |
| Escape interrupt cue | `ESC[38;5;252mautoESC[38;5;245m · model-aESC[39m` + 84 blanks + `ESC[38;5;245mesc again to interrupt` | `ESC[38;5;252mautoESC[38;5;245m · model-a` + 84 blanks + `esc again to interrupt` -- same cells and colours; see "differs" for the bytes |
| retry row | `ESC[38;5;252m⚠ Provider unavailable · HTTP 500 · server_error: upstream exploded · retrying request in 1sESC[39m` | the same |
| retry row, plain-text body | `ESC[38;5;252m⚠ Provider unavailable · HTTP 500: unexpected request · retrying request in 1sESC[39m` | the same |
| question cancelled by Ctrl-C | `ESC[38;5;252m■ESC[39m Cancelled`, nothing after it | the same |
| a prompt waiting for its turn | `ESC[38;5;245m┋ change toneESC[39m`, between the activity row's blank and a blank of its own, above the composer | the same row, same SGR, same place |
| full transcript, a retry | `ESC[38;5;252m! recovery:ESC[38;5;250m ⚠ Provider unavailable · HTTP 500 · server_error: upstream exploded · retrying requestESC[39m` then `  ESC[38;5;250mkind: auto_retry · cause: provider_unavailable · attempt: 1/10 · diagnostic: HTTP 500 · server_error: upstreamESC[39m` | the same bytes, `1/3` for `1/10` (so the row fits 120 columns unwrapped) |
| full transcript, the recovery | `ESC[38;5;252m✓ recovery:ESC[38;5;250m ✓ recovered · succeeded on attempt 3ESC[39m` then `  ESC[38;5;250mkind: auto_recovered · attempt: 3/10ESC[39m` | the same bytes, `3/3` for `3/10` |

(`ESC[39m` is tmux's re-serialization of the reset; xfx writes `ESC[0m`.)

## Per snap

### `interrupt` 40x120

| snap | matches | differs, and why |
|---|---|---|
| `idle-cc` | every row; row 5 is the exit hint, byte for byte | -- |
| `idle-cc-expired` | every row; the row is `auto · model-a` again | -- |
| `mid` | the card, the composer, the hint row; nothing of the un-ended words is on the screen on either | row 5 blink/count |
| `cc-0.3` | the card, the composer, the exit hint; row 5 is the cancelled line | name |
| `cc-1.5` | the same: the turn has concluded and the row still says `press ctrl+c again to exit`, on both | name |
| `cc-3.5` | the same, the hint row back to `auto · model-a` | name |
| `next` | the next turn under the cancelled line: card, `After interrupt answer.`, `0s (↑1 ↓40)` | name |
| `mid2` | rows 7-19 | name; row 15 blink/count |
| `esc1` | row 19's cells and colours: `esc again to interrupt` flush with column 120 in the dim grey | name; row 15 blink/count; row 19's bytes (below) |
| `esc2`, `esc2-later` | every row; the second cancelled line at row 15, and the hint row the session's own (an Escape pair arms no exit) | name, twice |

### `interrupt2` 40x120 and 24x80

The interrupt is the same on both: the lines already on the screen stay, then a
blank row, the cancelled line, a blank row, the composer. **How far the answer
had got differs** (40x120 `esc2`: fx's last line is `line 17`, xfx's `line 12`):
the two products release a stream at their own rates -- xfx paces
(`src/tui/pacer.rs`), fx paints lines as they arrive -- so every row under the
answer sits at a different row number, and the table states them by content.
The 24x80 snaps have scrolled the same skew through the whole screen.

| snap | matches | differs, and why |
|---|---|---|
| `mid` | the card and the lines' form | stream position; blink/count |
| `esc1` | the hint row: `esc again to interrupt` flush right in the dim grey, at both widths (`auto · model-a`, 44 blanks, the cue, at 80 columns) | stream position; blink/count |
| `esc2`, `esc2-later` | cancelled line, composer, `auto · model-a` | stream position; name |
| `cd-streaming` | **Ctrl-D while an answer streams is not an exit on either** (`app_input_runtime.zig:1750`): the session is up, the turn still `Generating` | stream position; blink/count |
| `draft` | `┃ draft text` under a running turn | stream position |
| `draft-cc` | the draft is gone, the turn still `Generating`, the hint row `auto · model-a` (nothing armed) | stream position; blink/count |
| `draft-cc2` | the second press stopped the turn: a cancelled line under `┃ stream words`, the hint row `press ctrl+c again to exit` | stream position; name |

### `interrupt-cmd` 40x120

| snap | matches | differs, and why |
|---|---|---|
| `cmd-running` | the card, `● 1 tool call · 1 command`, `└ Running sleep 6` | the activity row's `↓40` / `↓0` (fx counts the tool step's reported usage, xfx's row counts streamed text); the mode label `full access` / `yolo`, each product's own word |
| `cmd-cc`, `cmd-cc-2` | the card, the group header's first words, the composer, `press ctrl+c again to exit` | **fx kills the command at the keystroke** -- `· 1 cancelled`, `└ Cancelled sleep 6`, `■ Cancelled sleep 6 · What can fx do differently?`. **xfx's command keeps running**: a running executor holds the runtime's thread, so the cancellation is read only when it returns (F1's finding, `src/tui/shell.rs` `cancel_turn_with`), and the group says `└ Running sleep 6` under a still-running activity row rather than claim a stop that has not happened |
| `cmd-done` | the card, the cancelled line's place under the group, the composer; neither product asked the model again (each gateway log ends at the tool call) | fx: `└ Cancelled sleep 6` and `■ Cancelled sleep 6 · …`; xfx: `└ Ran sleep 6` (the command finished) and the turn's own `■ Cancelled · What can xfx do differently?` written when the turn ended. Making the stop reach a running command at once is a runtime change (the executor would have to be polled beside the control channel), not presentation |

### `interrupt-question` 40x120

`q` and `q-cc`: every row the same, SGR included. After Ctrl-C at a model's
question both show the question's `■ Cancelled` and then the composer and the
session's hint row -- no second cancelled line and no exit hint
(`cancelQuestionPrompt`, `input_question_runtime.zig:59-80`).

### `interrupt-steer` 40x120

| snap | matches | differs, and why |
|---|---|---|
| `steer-tool` | the waiting prompt's row: `┋ change tone` in `ESC[38;5;245m`, the activity row, a blank, the row, a blank, the composer -- byte for byte | the hint row's `queued 1 ·` (xfx's queue is said there as well); everything above it by the stream difference below |
| `steer-stream` | -- | **fx steers a tool-free answer**: Enter interrupts it, keeps what was flushed, and answers the new prompt at once (`steered answer.`, e2e `tui-interrupt-recovery.test.ts:291`). **xfx keeps its queue**: the first answer streams on and the prompt waits as `┋ what are you doing` above the composer |
| `steer-stream-done` | -- | xfx's first answer whole, then the prompt's card and its answer as the next turn; fx's was cut where the steer landed |
| `steer-done` | the card is written when the waiting prompt is delivered, not at the keystroke, on both | fx delivers it after the tool **inside the same turn** (`┃ change tone` under `└ Ran …`, then `after steer.`); xfx runs the turn to its end (`after steer.`) and then the prompt as the next turn (`┃ change tone`, `spare.`) |

### `interrupt-retry` 40x120 and 24x80

xfx's retry policy is unchanged: three attempts a step, 250 ms then 500 ms
(`src/agent/machine.rs` `retry_delay`); fx backs off about a second, growing,
and does not give up (`model_response_recovery.zig:3-9`). The snap times catch
xfx's row; fx's row at the same times shows the same state wherever both are
retrying.

| snap | matches | differs, and why |
|---|---|---|
| `r0.15`, `r0.45` (40x120) | every row: the retry row byte for byte | -- |
| `r1.05` (both sizes) | -- | xfx's third attempt has already answered (`Recovered answer.`, `1s (↑2 ↓40)`); fx is still counting down its first backoff. Policy, not presentation |
| `r2.5` (both sizes) | every row: both recovered, same answer and summary | -- |
| `f0.15` (both sizes), `f0.55` (24x80) | every row: `⚠ Provider unavailable · HTTP 500: unexpected request · retrying request in 1s` | -- |
| `f0.55` (40x120) | the row's words | xfx caught with its attempt under way (`· retrying request`, no count), fx in its countdown (`in 1s`): timing |
| `f2.5` (both sizes) | rows above the last | xfx spent its three attempts and wrote `✗ system: the Gateway returned HTTP 500: unexpected request` (its terminal failure, unchanged); fx is still retrying |
| `r0.15`, `r0.45` (24x80) | the row's words and colour up to column 80 | **fx wraps the label onto a second row** (`  request in 1s`, two-column indent, up to three rows: `max_static_status_activity_rows`, `render_input.zig:406`, `:690-697`); **xfx cuts it at column 80** (`…· retrying r`). xfx's activity row is one geometry row (`layout.rs` `Geometry::activity`); a multi-row activity block is a layout change not made here |

### What the model is told: `interrupt` bodies, `interrupt-next-question`, `interrupt-next-cmd` 40x120

The request after the interrupt, non-system messages (the `*-bodies/`
directories):

| after | fx | xfx |
|---|---|---|
| an answer stopped mid-stream (`interrupt`, fx `003`, xfx `002`) | `user stream please` · `assistant <partial> \n\nThe previous response ended before completion.` · `user <turn_aborted>…` · `user next` | the same four messages; the partial ends one word earlier (stream position) |
| a question cancelled with Ctrl-C (fx `003`, xfx `002`) | `user ask me` · `assistant` tool-call `ask_user_question` · `tool` `{"type":"error-text","value":"aborted by user"}` · `user <turn_aborted>…` · `user next` | the same five messages and output type; the call id and the input's key order are each product's own |
| a command stopped while it ran (fx `003`, xfx `002`) | the call, `error-text` `aborted by user`, `<turn_aborted>`, `next` | the call, **its real result** (`exit_code 0`, `text`), `assistant Interrupted by user after completing 1 tool call: terminal.`, `<turn_aborted>`, `next` |

The last row is the running-command residual below, not the history: xfx's
command ran to its end, so the history says it completed, in fx's own sentence
for completed calls (`formatCompletedToolSummary`, `session.zig:3020-3032`).
The `next` snaps: `interrupt-next-question`'s are the same rows on both
products but for the banner; `interrupt-next-cmd`'s differ only in the stopped
group, as in `interrupt-cmd` `cmd-done` (fx `└ Cancelled sleep 6`, xfx `└ Ran
sleep 6` and the turn's own cancelled line).

### `interrupt-recovery` 80x120

`full`: the full transcript's records, under `* session:`, in fx's order -- the
failed request's `! network:`, `! recovery:` for the retry (`attempt: 1/…`),
`! recovery:` again as attempt 2 starts, the second failure's `! network:` and
`! recovery:`, `! recovery:` as attempt 3 starts, the `* network:` that
finished, and `✓ recovery: ✓ recovered · succeeded on attempt 3` -- SGR byte for
byte (table above). Differs: the budget (`/3`, `/10`); fx's second backoff is a
second, so its second retry and the attempt after it say `in 1s` and `retry
delay: 1s`, where xfx's 500 ms rounds down to neither (`orchestrator.zig:5017`);
fx's `* session:` has its own `system prompt:` and `skills:` lines and fifteen
tools (F5's).

### `interrupt-help` 40x120

`help-open`, `help-cc`, `help-cc-expired`: the catalog stays up under a first
Ctrl-C on both, its key-help row becomes `press ctrl+c again to exit` in the
statusline grey, and the key help comes back when the window closes -- the hint
row byte for byte in all three. Every other row differs by the two registries
(fx's 36 commands, xfx's), as in `receipts/c1`.

## What differs and is not fixed here

- **The bytes before the Escape cue.** fx closes the row's run before the
  padding (`input_presentation.zig:559-567`); xfx closes it after the padding
  and before the cue, so tmux reads the padding as grey-245 blanks and prints
  no SGR change before the cue. Cells and colours are the same; the cue's own
  colour is fx's `dim_style` in both palettes (pinned by
  `the_right_hand_cue_is_painted_in_the_dim_grey_after_the_rows_run_is_closed`).
  Moving the reset in front of the padding is a change to `hint::row`'s
  right-slot rule, which `esc again to clear` shares.
- **A running command is not stopped at the keystroke** (`interrupt-cmd`
  above): runtime, not presentation.
- **A failed result that is a refusal** goes as `error-text`; fx sends a
  permission denial as `execution-denied` (`vercel_protocol.zig:742-744`).
  Every other failed result is `error-text` on both.
- **`✓ recovered · attempt N` on the band** for 1.5 s after a recovery
  (`worker_status.zig:5`, `:221-235`) is not shown; the row goes back to the
  turn's own on the first sign the attempt got through. Neither capture caught
  fx's at these intervals. Its full-transcript record is written
  (`interrupt-recovery`).
- **Escape pausing a retry** (`app_input_runtime.zig:3163-3167`) and the
  `waiting for connection` / `recovery paused` labels: xfx has no pausable
  recovery.
- **Submitting while busy (§4.2)**: the presentation is fx's -- the waiting
  prompt is the band's `┋` row (at most two rows, `…`), and its card is written
  when its turn starts -- over xfx's own queue (one prompt waiting, a second
  refused, `queued 1` still on the hint row; the prompt runs as the next turn).
  fx's semantics need what xfx lacks: interrupt-and-continue of a tool-free
  answer with a `<user_steering>` message (`tui-interrupt-recovery.test.ts:291`),
  delivery at a tool boundary inside the same turn, several steers at once, and
  Up withdrawing a waiting prompt into the composer (the runtime takes the
  queued prompt at the top of `turn_loop` the moment a turn ends, so a withdraw
  needs a control round-trip the runtime acknowledges). Up is still the
  history recall.
