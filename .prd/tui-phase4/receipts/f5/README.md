# F5 full transcript (Ctrl-O) — fx `c1db919` vs xfx, 40x120 and 24x80

Two scenarios, driven identically on both binaries by `scripts/fxcap/capture.sh` (isolated tmux
socket, empty `HOME`, fake key, loopback fake gateway, desktop shims kept). The xfx captures were
retaken at `575ea48` (this branch with `plan/tui-phase4` at `f0b1caa` merged in); the fx captures of
`fulltranscript` are the ones taken at `26bc0df` (fx is the same build):

```
export TMPDIR=/private/tmp/claude-501
FX_BIN=<fx c1db919 build> scripts/fxcap/capture.sh fx  fulltranscript        40x120 <out>
scripts/fxcap/capture.sh                            xfx fulltranscript        40x120 <out>
FX_BIN=<fx c1db919 build> scripts/fxcap/capture.sh fx  fulltranscript        24x80  <out>
scripts/fxcap/capture.sh                            xfx fulltranscript        24x80  <out>
FX_BIN=<fx c1db919 build> scripts/fxcap/capture.sh fx  fulltranscript-notice 40x120 <out>
scripts/fxcap/capture.sh                            xfx fulltranscript-notice 40x120 <out>
FX_BIN=<fx c1db919 build> scripts/fxcap/capture.sh fx  fulltranscript-notice 24x80  <out>
scripts/fxcap/capture.sh                            xfx fulltranscript-notice 24x80  <out>
```

The machine was under load (load average 23-63) while these ran. The first xfx `fulltranscript`
40x120 run typed `run the checks` before the debug binary had taken the terminal, so the turn ran
without its approvals being answered; it was retaken from a copy of the scenario directory
(`FXCAP_SCENARIOS=<scratch>/scen-slow`) whose only change is the first `wait 2` → `wait 8`. An
earlier `fulltranscript-notice` run failed the same way. Scratch paths are replaced with `<scratch>`;
request bodies are not committed.

## `fulltranscript`

`fulltranscript.cap`, both in `ask` mode on the `toolgroup-ws` workspace, with a turns file of each
product's own (`fulltranscript-fx.json` / `-xfx.json`: fx's `shell`/`run` is xfx's `terminal`/`exec`):
`run the checks` → a text chunk and `read_file a.txt` → a command that prints `boom`/`err` and exits 3
(asked; **Ctrl-O pressed at the question**, snap `cmd-ask-ctrl-o`, then `1`) → `edit_file a.txt`
(asked, reviewed, `1`) → `read_file missing.txt` (fails) → `All done.`. Then Ctrl-O (`full`), PgUp
(`full-pgup`), Up Up (`full-up2`), PgDn (`full-pgdn`), Escape (`full-esc`); a second prompt
(`second`), Ctrl-O (`full2`), **Ctrl-C** (`full2-cc`), Ctrl-O (`full3`), **Ctrl-O** (`full3-close`).
Every completion the turns file scripts was requested on both (`*-gateway.log`).

Compared with the timestamps and the network records' milliseconds masked:

| snapshot | result |
|---|---|
| `cmd-ask` → `cmd-ask-ctrl-o` (both sizes) | **Ctrl-O over a question does nothing** on either: the two `.ansi` files are byte-identical on fx and on xfx. |
| `full` (both sizes) | identical but for #1, #3 and #4 below: headers, group header, call rows, the read's lines, the command's output and status, the blank row and the diff, `All done.`, the footer and the hint row. |
| `full-pgup`, `full-up2` 40x120 | **now the same screen**: the welcome, `* session:` (#2), five `* network: provider: gateway · model: fake/model-a · <n>ms` / `finish: tool_calls` (four) or `finish: stop` / `tokens: 1200 in · 40 out` records, then the prompt -- the records above every entry, in the order the requests settled, as fx's (`prependFullDetailRecords`, `transcript/runtime.zig:4980-5001`). Before this change xfx's record had none and PgUp/Up landed elsewhere. |
| `full-pgup`, `full-up2`, `full-pgdn` 24x80 | the same arithmetic on both (a page = rows − 4, Up/Down three rows); the windows differ only by the two rows of #1 and the session record's length (#2). |
| `full2`, `full3` (both sizes) | the same, with the second turn at the tail. |
| `full-esc`, `full2-cc`, `full3-close` (both sizes) | **closing gives back the screen it found, byte for byte**: `full-esc.ansi` == `end.ansi`, `full2-cc.ansi` == `second.ansi`, `full3-close.ansi` == `second.ansi`, on fx and on xfx. |

SGR, as captured on both: entry header `ESC[38;5;245m  <ts> UTC · <Kind>ESC[39m`; a record
`ESC[38;5;250m* network: …ESC[39m` with its continuation rows two columns in; group header
`ESC[38;5;255m●ESC[39m ESC[38;5;245m4 tool calls · …ESC[39m`; a call's row in the default foreground
with the command's words in 252; detail rows `│ESC[38;5;245m  1    alpha line 1ESC[39m` and
`│ESC[38;5;245m boomESC[39m`; diff context `ESC[38;5;245m  │ 1   alpha line 1ESC[39m`, change
`ESC[38;5;252m  │ 2 - alpha line 2ESC[39m`; prompt card `ESC[38;5;255m┃ESC[39m ESC[1mrun the checksESC[0m`;
footer `ESC[38;5;255m┃ESC[39m ESC[38;5;245mfull detail · …ESC[39m`.

## `fulltranscript-notice`

T2's `notice-midstream` (a 54-chunk answer at 100 ms a chunk; `/notacommand` and `/version` typed
while its second paragraph streams), then Ctrl-O (`full`), PgUp (`full-pgup`), PgUp (`full-top`).

| snapshot | result |
|---|---|
| `full-top` 40x120 | **the same entries in the same order**: `* session:`, `* network: … / finish: stop / tokens: 1200 in · 40 out`, Prompt `┃ stream a long answer please`, Response `Opening line of the answer.`, Error `✗ command: Unknown command. Try /help.`, Notice `* version: …`, **one** Response holding the whole `MARKER-A word10 … word139` paragraph wrapped as with no notice and `Closing line of the answer.`, Usage. Before this change xfx's record cut the paragraph at each notice: Response `…word105` / Error / Response `word106 … word115` / Notice / Response `word116 …`. |
| `full`, `full-pgup` (both sizes) | the same, windowed; 24x80 shifted by the session record's two fewer rows (#2). |

## What differs, and why

| # | fx | xfx | why |
|---|---|---|---|
| 1 | `│  action: run` / `│  yield_time_ms: 30000` under the command | nothing | the tool backend: fx's `shell` takes `{request: {action: run, …}}`; xfx's `terminal` takes `action: exec` + `command`, and both are left out by upstream's own rules (`appendFullSemanticArguments`, `full_transcript_screen.zig:6501-6510`). |
| 2 | `* session:` with `system prompt: ready · 5900 bytes`, `tools: 15 advertised (read_file, …, +7 more)`, `skills: loading` | no `system prompt:` and no `skills:` line; `tools: 9 advertised (list_files, …, terminal, +1 more)` | xfx assembles no system prompt at launch (its one system message is the workspace's `AGENTS.md`, read each turn) and has no skill catalog; the tools are xfx's own. `effort: auto` and `mcp: none` are written as fx writes them for none (`app_bootstrap_runtime.zig:61-111`). |
| 3 | `Failed missing.txt: Path not found: missing.txt` | `Failed missing.txt: read_file refused the path: no such path: \`missing.txt\`` | the tool's own refusal sentence, not the screen. |
| 4 | Usage `6s (↑4 ↓200)`; in `fulltranscript-notice` an empty Usage entry | `6s (↑4 ↓40)`; `5s (↑8 ↓40)` | the summary's out count is the inline summary's (the same difference is on the inline `end` screen). fx's empty Usage body in the notice scenario, where its inline summary reads `5s (↑8 ↓40)`, was not traced. |
| 5 | `│  Path not found: …` ends without a colour reset | each row closes its colour | fx leaves the run open across a blank row; the screen shows the same greys. |
| 6 | welcome `𝒇x v0.0.13`, `* version: 0.0.13` | `x𝒇x v0.1.0-575ea48 [debug]`, `* version: 0.1.0` | the product's own name and build. |
| 7 | Response header stamped after the Usage header of the same turn | in document order | xfx stamps an entry when the document is given it. |

Matched since the first receipt and not visible in a text capture: the screen has no caret (its frames
end on `?2026l` alone, `paint_plan.zig:1047`; the give-back frame shows it again); Home, End and every
key the screen does not bind go to the composer behind it (`app_input_runtime.zig:973-994`); a change
made without a review (`auto`/`yolo`) shows its diff, taken from the preimage the tool read; a resumed
session's turns are on the screen with their calls' arguments and results and the log's times. Not
matched: no `* recovery:` record (xfx's retries say nothing, `app_worker_runtime.zig:95-133`), and a
change made before a resume has no diff (the log keeps none).

## Smoke rows

`scripts/smoke-tui.sh` sends no Ctrl-O and no page key and reads no full-transcript row; no smoke
row is invalidated by this change.
