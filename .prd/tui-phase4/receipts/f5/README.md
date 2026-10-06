# F5 full transcript (Ctrl-O) — fx `c1db919` vs xfx, 40x120 and 24x80

One scenario, driven identically on both binaries by `scripts/fxcap/capture.sh` (isolated tmux
socket, empty `HOME`, fake key, loopback fake gateway, desktop shims kept):

```
export TMPDIR=/private/tmp/claude-501
FX_BIN=<fx c1db919 build> scripts/fxcap/capture.sh fx  fulltranscript 40x120 <out>
scripts/fxcap/capture.sh                            xfx fulltranscript 40x120 <out>   # target/debug/xfx at 26bc0df
FX_BIN=<fx c1db919 build> scripts/fxcap/capture.sh fx  fulltranscript 24x80  <out>
scripts/fxcap/capture.sh                            xfx fulltranscript 24x80  <out>
```

`fulltranscript.cap`, both in `ask` mode on the `toolgroup-ws` workspace, with a turns file of each
product's own (`fulltranscript-fx.json` / `-xfx.json`: fx's `shell`/`run` is xfx's `terminal`/`exec`):
`run the checks` → a text chunk and `read_file a.txt` → a command that prints `boom`/`err` and exits 3
(asked; **Ctrl-O pressed at the question**, snap `cmd-ask-ctrl-o`, then `1`) → `edit_file a.txt`
(asked, reviewed, `1`) → `read_file missing.txt` (fails) → `All done.`. Then Ctrl-O (`full`), PgUp
(`full-pgup`), Up Up (`full-up2`), PgDn (`full-pgdn`), Escape (`full-esc`); a second prompt
(`second`), Ctrl-O (`full2`), **Ctrl-C** (`full2-cc`), Ctrl-O (`full3`), **Ctrl-O** (`full3-close`).
Scratch paths are replaced with `<scratch>`; request bodies are not committed.

Every completion the turns file scripts was requested on both (`*-gateway.log`: fx 6 + a title call,
xfx 6).

## What matches

Compared with the four timestamps per screen masked (`YYYY-MM-DD HH:MM:SS.mmm` → `<ts>`), on the
`.ansi` captures (tmux's re-serialization of both):

| snapshot | result |
|---|---|
| `cmd-ask` → `cmd-ask-ctrl-o` (both sizes) | **Ctrl-O over a question does nothing** on either: the two `.ansi` files are byte-identical on fx and on xfx (`transitionScreen`, `input_full_transcript_runtime.zig:83-99`; xfx: the question has the keys). |
| `full` 40x120 | fx rows 1-13 = xfx rows 3-15 and fx rows 16-40 = xfx rows 16-40, except fx rows 29-30 (#3), 32 (#5) and 36 (#4) below: the `<ts> · Response` / `<ts> · Tool` / `<ts> · Usage` headers, the group header `● 4 tool calls · 2 read · 1 edit · 1 command · 2 failed`, the call rows `├ Read a.txt`, `├ Exited 3 echo boom; echo err >&2; exit 3`, `├ Edited a.txt +1 / -1`, `└ Failed missing.txt: …`, the read's lines `│  1    alpha line 1` …, the command's `│ boom` / `│ err` / `│ exit code 3`, the blank row and the reviewed diff `  │ 1   alpha line 1` / `  │ 2 - alpha line 2` / `  │ 2 + ALPHA LINE TWO` / `  │ 3   alpha line 3`, `  All done.`, and the footer `┃ full detail · ctrl+o close · pgup/pgdn scroll · esc close` / blank / `ask · model-a`. |
| `full` 24x80 | rows 1-24 identical but the failure wording and the summary's out count (below). |
| `full2`, `full3` (both sizes) | the same, with the second turn's `<ts> · Prompt` / `┃ second prompt` / `<ts> · Response` / `  Second answer.` / `<ts> · Usage` at the tail. Opened after a second turn, the screen holds both turns and opens at the tail, as fx's does. |
| `full-esc`, `full2-cc`, `full3-close` (both sizes) | **closing gives back the screen it found, byte for byte**: `full-esc.ansi` == `end.ansi`, `full2-cc.ansi` == `second.ansi`, `full3-close.ansi` == `second.ansi`, on fx and on xfx. Escape, Ctrl-C and Ctrl-O all close; Ctrl-C does not interrupt or exit (the second turn's prompt and answer are intact). |
| `full-pgup` 24x80 | PgUp moved exactly one page (20 rows = 24 − 4) on both: rows 3-17 identical (`<ts> · Prompt` down to `├ Exited 3 …`). |
| `full-pgdn`, `full-up2` | the same arithmetic on both — Up/Down three rows, a page the rows the screen shows, clamped at the head and the tail — from different starting points, because fx's record is longer (below). |

SGR, as captured on both:

* entry header `ESC[38;5;245m  <ts> UTC · <Kind>ESC[39m`;
* group header `ESC[38;5;255m●ESC[39m ESC[38;5;245m4 tool calls · …ESC[39m`;
* a call's row in the **default** foreground, the command's words in 252:
  `├ Exited 3 ESC[38;5;252mechoESC[39m boomESC[38;5;252m;ESC[39m … ESC[38;5;252mexitESC[39m 3`
  (the inline group writes the same row in 245; `end.ansi` row 9);
* detail rows `│ESC[38;5;245m  1    alpha line 1ESC[39m` (argument or result: two cells) and
  `│ESC[38;5;245m boomESC[39m` (a command's output and status: one cell), the rail itself unstyled;
* reviewed diff: context `ESC[38;5;245m  │ 1   alpha line 1ESC[39m`, change
  `ESC[38;5;252m  │ 2 - alpha line 2ESC[39m`;
* prompt card `ESC[38;5;255m┃ESC[39m ESC[1mrun the checksESC[0m`;
* footer `ESC[38;5;255m┃ESC[39m ESC[38;5;245mfull detail · …ESC[39m`, hint row as the band's.

## What differs, and why

| # | fx | xfx | why |
|---|---|---|---|
| 1 | `│  action: run` / `│  yield_time_ms: 30000` under the command | nothing | the tool backend: fx's `shell` takes `{request: {action: run, …}}`; xfx's `terminal` takes `action: exec` + `command`, the command is on the row and `exec` is the only action, so both are left out by upstream's own rules (`appendFullSemanticArguments`, `full_transcript_screen.zig:6501-6510`). The two extra rows shift every row above them by two in `full`/`full2`. |
| 2 | `* session: …` and one `* network: … / finish: … / tokens: …` notice per request, before the prompt (`full-pgup`, `full-up2`) | none; the record opens with the welcome row and the prompt | xfx's document is given no such notices, and this screen shows what the document was given; inventing request telemetry here would be a second source of truth. Also why fx's record is ~34 rows longer and PgUp/Up land elsewhere at 40x120. |
| 3 | `Failed missing.txt: Path not found: missing.txt` | `Failed missing.txt: read_file refused the path: no such path: \`missing.txt\`` | the tool's own refusal sentence (`src/tools/read.rs`), not the screen. |
| 4 | `6s (↑4 ↓200)` | `6s (↑4 ↓40)` | the turn summary's out count is the inline summary's (`super::shell`'s `Tally`), and the same difference is on the inline `end` screen; not this unit's. |
| 5 | `│  Path not found: …` ends without a colour reset, so the next header's `ESC[38;5;245m` is folded into it by tmux | each row closes its colour | fx leaves the run open across a blank row; the screen shows the same greys. |
| 6 | welcome `𝒇x v0.0.13` | `x𝒇x v0.1.0-26bc0df [debug]` | the product's own name and build. |
| 7 | Response header stamped after the Usage header of the same turn (`.081` / `.078`) | in document order | xfx stamps an entry when the document is given it. |
| 8 | Home/End (not driven): reach the hidden composer | head / tail of the record | upstream binds neither on this screen (`input_full_transcript_runtime.zig:147-175`); the brief asked for them. Every other key xfx does not bind is swallowed rather than typed into a composer nobody can see (fx types into it: `app_input_runtime.zig:973-994`). |
| 9 | caret hidden | caret on the blank row under the footer text | xfx's plane painter always shows the caret (`src/tui/frame.rs` `END_FRAME`); not visible in a capture. |

Not exercised by this scenario and recorded for the reader: a change xfx made **without** a review
(`auto` mode) shows its arguments (`path`, `old_string`, `new_string`) where fx shows the diff from
its result store; a call's result is kept to its first 64 KiB with a line saying how much more the
session's log holds; a resumed session's earlier turns are not on the screen (they are on the
runtime thread, which the UI does not wait on).

## Smoke rows

`scripts/smoke-tui.sh` sends no Ctrl-O and no page key and reads no full-transcript row; no smoke
row is invalidated by this change.
