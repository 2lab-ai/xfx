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
gateway log (its snaps, retaken in the same run and diffed against the ones
here, differ only by the banner and blink/count in `mid2` and `esc1`).

**Round 2** (the command stopped at the keystroke, refusals, the recovered
row, the wrapped retry row, steering, no `queued N`, the Escape cue's bytes):
xfx retaken at `1578121` (plan `51ba257` merged) for `interrupt` 40x120,
`interrupt2` 40x120, `interrupt-cmd`, `interrupt-next-cmd`, `interrupt-steer`
(with bodies), `interrupt-retry` 24x80, and the new `interrupt-recovered` and
`interrupt-steer-retract`; fx captured for the two new scenarios and for
`interrupt-steer` again, to keep its request bodies. The snaps below that say
"round 2" are those.

**Re-checked at `66728b6`** (plan `b887a7f` merged: the W1, W3, W4 and W5
follow-ups): every xfx scenario in the table retaken at its sizes and diffed
against the files here, row 1 aside; the files here are not replaced. 31 of the
78 snaps have identical text (29 of them identical SGR too), and the rest
differ only in timing: stream position, the steer's cut point (`word14` /
`word15`), the summaries' seconds, which part of a retry's countdown a snap
caught, the marker's blink, and the full-access warning at `rec-1.8`, which
lands about 3.8 s into its 4 s on the screen (present in one of two further
runs, absent in the other). The `/help` catalog also lists the commands plan
added; its Ctrl-C row is unchanged. The rows this receipt is about are
byte-identical: the recovered row (`rec-1.4`, as fx's), the wrapped retry row
at 24x80 (`r0.45`), all of `interrupt-cmd`, and the Escape cue's hint row
(`esc1`, as fx's). Every request body has the same messages; only the
interrupted answer is cut at a different point.

Scenarios under
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
| `interrupt-recovered.cap` | `interrupt-recovered-fx.json` / `-xfx.json` | 40x120 | a 500, then a four-second command the recovered step asked for (the row while it runs), then a 500 and a slow answer (the row while a retried answer arrives) |
| `interrupt-steer-retract.cap` | `interrupt-steer-retract-fx.json` / `-xfx.json` | 40x120 | a prompt sent while a command runs, taken back with Up, edited, sent again; the request after the command |

The shims (`open`, `pbcopy`, `pbpaste`, `afplay`, `osascript`) and
`FX_NO_OPEN_BROWSER=1` are the harness's own, unchanged. Sanitized: the scratch
run directory appeared only in the system prompts of the request bodies; bodies
are kept for the scenarios whose requests are the receipt (the history and the
steer questions below), cut to their non-system messages, and no scratch path
is left in any capture file. Every other scenario keeps its gateway log.

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
| Escape interrupt cue | `ESC[38;5;252mautoESC[38;5;245m · model-aESC[39m` + 84 blanks + `ESC[38;5;245mesc again to interrupt` | the same bytes (round 2: the row's run is closed before the padding) |
| retry row | `ESC[38;5;252m⚠ Provider unavailable · HTTP 500 · server_error: upstream exploded · retrying request in 1sESC[39m` | the same |
| retry row, plain-text body | `ESC[38;5;252m⚠ Provider unavailable · HTTP 500: unexpected request · retrying request in 1sESC[39m` | the same |
| question cancelled by Ctrl-C | `ESC[38;5;252m■ESC[39m Cancelled`, nothing after it | the same |
| a steer waiting in the band | `ESC[38;5;245m┋ change toneESC[39m`, between the activity row's blank and a blank of its own, above the composer | the same row, same SGR, same place |
| a command stopped at the keystroke | `ESC[38;5;255m●ESC[39m ESC[38;5;245m1 tool call · 1 command · 1 cancelledESC[39m`, `ESC[38;5;245m└ Cancelled ESC[38;5;252msleepESC[38;5;245m 6ESC[39m`, `ESC[38;5;252m■ESC[38;5;255m CancelledESC[39m ESC[38;5;252msleepESC[38;5;245m 6ESC[39m · What can fx do differently?` | the same bytes, name (round 2) |
| recovered | `ESC[38;5;252m✓ recovered · attempt 2ESC[39m` | the same (round 2) |
| retry row wrapped at 80 columns | `ESC[38;5;252m⚠ Provider unavailable · HTTP 500 · server_error: upstream exploded · retryingESC[39m` then `ESC[38;5;252m  request in 1sESC[39m` | the same, both rows (round 2) |
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
| `esc1` | row 19 byte for byte (round 2): `esc again to interrupt` flush with column 120 in the dim grey, the row's run closed before the padding | name; row 15 blink/count |
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

### `interrupt-cmd` 40x120 (round 2)

| snap | matches | differs, and why |
|---|---|---|
| `cmd-running` | the card, `● 1 tool call · 1 command`, `└ Running sleep 6` | the activity row's `↓40` / `↓0` (fx counts the tool step's reported usage, xfx's row counts streamed text) |
| `cmd-cc`, `cmd-cc-2` | every row after the banner, SGR included: `· 1 cancelled`, `└ Cancelled sleep 6`, `■ Cancelled sleep 6 · What can … do differently?`, `press ctrl+c again to exit` -- **the command is killed at the keystroke on both** | name |
| `cmd-done` | every row after the banner | name |

xfx's command holds the runtime's thread, so the control message is read only
after it returns; the UI now also raises the running turn's own flag
(`WorkHandle::control`), which the command's supervisor polls every 10 ms
(`src/tools/terminal.rs` `supervise`), and the shell settles the command's row
at the keystroke. The PTY case
`escape_twice_stops_a_running_command_and_its_group_at_once_as_upstreams_does`
checks the command's group is gone within a second.

### `interrupt-question` 40x120

`q` and `q-cc`: every row the same, SGR included. After Ctrl-C at a model's
question both show the question's `■ Cancelled` and then the composer and the
session's hint row -- no second cancelled line and no exit hint
(`cancelQuestionPrompt`, `input_question_runtime.zig:59-80`).

### `interrupt-steer` 40x120 (round 2)

Every snap is the same rows after the banner on both products -- the cut
answer `MARKER-HEAD … word15`, `┃ what are you doing`, `steered answer.`, one
summary; then `┋ change tone` above the composer while the command runs, and
`┃ change tone` under `└ Ran sleep 4; echo done` with `after steer.` -- but for
blink/count and the summaries' token counts (`↑4 ↓75` / `↑4 ↓40`: fx counts
reported usage). The requests (`*-bodies/`), non-system messages:

| request | fx | xfx |
|---|---|---|
| the steer over the answer (fx `004`, xfx `002`) | `user stream please` · `assistant MARKER-HEAD … word15 ` · `user <user_steering>…what are you doing…</user_steering>` | the same three messages |
| the next turn (fx `005`, xfx `003`) | … · `user what are you doing` (as typed) · `assistant steered answer.` · `user run it` | the same |
| the steer after the command (fx `006`, xfx `004`) | … · the call · its result · `user <user_steering>…change tone…</user_steering>` | the same, each product's own tool result |

### `interrupt-steer-retract` 40x120 (round 2)

`steer-wait` (`┋ change tone` while the command runs), `steer-up` (Up: the
prompt back in the composer, the `┋` row gone), `steer-again` (`┋ change tone
and keep it short`), `steer-done` (`┃ change tone and keep it short` under
`└ Ran sleep 4; echo done`, `after steer.`): the same rows on both but for the
banner, the warning's product name and blink/count. The request after the
command ends with `user <user_steering>…change tone and keep it short…
</user_steering>` on both (fx `005`, xfx `002`): fx's e2e
`tui-interrupt-recovery.test.ts:393`.

### `interrupt-recovered` 40x120 (round 2)

| snap | matches | differs, and why |
|---|---|---|
| `rec-1.4` (fx), `rec-0.5` (xfx) | `● 1 tool call · 1 command`, `└ Running sleep 4`, and over it `✓ recovered · attempt 2` in `ESC[38;5;252m` (`green_style`, the warning grey) | the moment: fx's first backoff is about a second, xfx's 250 ms, so the same state is a snap earlier on xfx |
| `rec-2.4`, `rec-3.2` | the turn's own `Running` row again once the 1.5 s are up | blink/count |
| `slow-1.8`, `slow-2.8` | `⚠ Provider unavailable · HTTP 500 · server_error: upstream exploded · retrying request` held while the retried answer arrives | -- |
| `rec-done`, `slow-done` | every row | the summary's seconds and tokens (timing, reported usage) |

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
| fx `r0.15` / xfx `r1.05` (24x80, round 2) | rows 1-10 after the banner, SGR included: the label wrapped onto a second row, `  request in 1s`, the band a row taller (`max_static_status_activity_rows`, `render_input.zig:406`, `:689-697`) | the moment (xfx's backoffs are 250 ms then 500 ms; its other 24x80 snaps catch other states of the same retries) |

### What the model is told: `interrupt` bodies, `interrupt-next-question`, `interrupt-next-cmd` 40x120

The request after the interrupt, non-system messages (the `*-bodies/`
directories):

| after | fx | xfx |
|---|---|---|
| an answer stopped mid-stream (`interrupt`, fx `003`, xfx `002`) | `user stream please` · `assistant <partial> \n\nThe previous response ended before completion.` · `user <turn_aborted>…` · `user next` | the same four messages; the partial ends one word earlier (stream position) |
| a question cancelled with Ctrl-C (fx `003`, xfx `002`) | `user ask me` · `assistant` tool-call `ask_user_question` · `tool` `{"type":"error-text","value":"aborted by user"}` · `user <turn_aborted>…` · `user next` | the same five messages and output type; the call id and the input's key order are each product's own |
| a command stopped while it ran (fx `003`, xfx `002`, round 2) | the call, `error-text` `aborted by user`, `<turn_aborted>`, `next` | the same four messages |

The `next` snaps of both scenarios are the same rows on both products but for
the banner (round 2 for `interrupt-next-cmd`).

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

- **The retry budget and its timing** (`.prd/tui-phase4/ssot.md` §Tensions):
  three attempts at 250 ms and 500 ms, against fx's ten at about a second and
  growing (`model_response_recovery.zig:3`). Retry policy, not presentation;
  every row and record is fx's, at xfx's moments and with xfx's numbers.
- **Escape over a retry arms the interrupt rather than pausing it.** fx pauses
  only its unbudgeted wait for connectivity (`pauseActiveRecovery`;
  `worker_runtime.zig:884-888`; `model_response_recovery.zig:166-180`) and
  resumes from a persisted checkpoint; xfx has neither (ssot §Tensions). Over a
  budgeted retry -- every retry xfx makes -- fx's Escape arms the interrupt too
  (`app_input_runtime.zig:3163-3176`).
- **A refusal's words** are xfx's (`write_file was not permitted: …`) inside
  fx's `execution-denied`, where fx's are a JSON error object
  (`tool_result_errors.zig:289-320`).
- **A file call is not stopped at the keystroke**: only the command polls the
  turn's flag; a file tool runs to its end, and its row shows its real result.
- **The activity row's output count while a tool runs** (`↓40` / `↓0`): fx
  counts the step's reported usage, xfx the text it streamed.
