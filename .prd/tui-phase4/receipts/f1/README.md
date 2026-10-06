# F1 receipt: the tool group

Scenario `scripts/fxcap/scenarios/toolgroup.cap` (turns `toolgroup-fx.json` / `toolgroup-xfx.json`, workspace `toolgroup-ws/`), captured at 40x120 with `scripts/fxcap/capture.sh` for fx `c1db919` (`FX_BIN`, built locally) and for xfx at this branch (`target/debug/xfx`). Both run in `ask` mode, so every change and command is decided by a key. Scratch paths in the copies here are replaced by `<scratch>`; the request bodies the gateway kept are not copied (they carry each product's system prompt and the scratch workspace path).

```
FX_BIN=<fx c1db919> scripts/fxcap/capture.sh fx  toolgroup 40x120 <out>
                    scripts/fxcap/capture.sh xfx toolgroup 40x120 <out>
python3 compare-group-rows.py <out>     # -> compare-group-rows.txt
```

What the turn does: a sentence and two reads (one step), a glob and a grep (a second step), a command that sleeps 3 s (approved with `1`), an edit (`1`), a write (`3`, refused), a read of a missing file, a command that exits 3 (`1`), then the answer. A second prompt asks for a write and stops the turn at its approval; a third approves a `sleep 4` and presses Ctrl-C while it runs.

## Matches

`compare-group-rows.txt`: **103 of 126** group rows (`●`/`├`/`└`/`■` rows, every snapshot) are byte-identical in tmux's re-serialized ANSI. The 23 that differ are the four reasons below and nothing else.

- **Header**: `● 9 tool calls · 4 read · 2 commands · 1 list · 1 write · 1 edit · 2 failed · 1 denied` -- count, categories by count with ties in upstream's order, `commands` plural, then the outcome counts; `ESC[38;5;255m●ESC[39m ESC[38;5;245m…ESC[39m`.
- **One group across steps**, broken only by text: reads, glob, grep, the command, the edit, the refused write, the failed read and the exit-3 command are one group of nine; `All done.` closes it (`end`).
- **Rows**: `Read a.txt`, `Matched **/*.rs`, `Searched alpha`, `Ran <cmd>` / `Running <cmd>`, `Edited a.txt +1 / -1`, `Denied file`, `Exited 3 <cmd>`, `Cancelled file`; whole row `ESC[38;5;245m`, command words lifted to 252 (`;`, `>&` 252, the fd `2` 250) by the shell highlighter.
- **The running row** `└ Running sleep 3; echo hello from shell; ls` under the group while its approval is up (`cmd-ask`) and while it runs (`cmd-running`), rewritten in place to `├ Ran …` (`edit-ask`).
- **A file change has no row until it is decided**: the edit and the write waiting on their approvals are neither rows nor counted (`edit-ask`, `write-ask`).
- **The activity row is off the band while an approval is up** (`cmd-ask`, `exit-ask`, `long-ask`: no `Running`/`Thinking` row), back when it is answered (`cmd-running`).
- **A call stopped at its approval**: `● 1 tool call · 1 write · 1 cancelled` / `└ Cancelled file`, a blank row, `■ Cancelled file · What can … do differently?` (`again-stopped`), `ESC[38;5;252m■ESC[38;5;255m CancelledESC[39m ESC[38;5;245mfileESC[39m · …`.
- **Placement**: the group sits one blank row under the sentence before it and one above the answer after it; while open, the activity row is one blank row under it (`cmd-running`: rows 7-14 are the same on both).

## Differs, and why

1. **`Failed missing.txt: <reason>`** -- fx says `Path not found: missing.txt`, xfx says `read_file refused the path: no such path: \`missing.txt\``. The reason is the tool's own one-line summary (`ToolResult::detail`); the wording is xfx's tool backend, not the row (fx `src/core/tooling/tool_admission.zig:2471`). One row in each of 8 snapshots.
2. **`What can xfx do differently?`** -- the product's own name in upstream's follow-up (`src/ui/transcript/runtime.zig:644`). One row in each of 5 snapshots.
3. **Ctrl-C while a command runs** (`long-stopped`, `long-later`). fx kills the command at once: `└ Cancelled sleep 4` and `■ Cancelled sleep 4 · …`. xfx cannot stop a running command until it returns -- the executor holds the runtime's thread, and the control channel is polled beside the turn, not inside a tool (`src/tui/worker.rs` `raced_against_control`) -- so its row stays `└ Running sleep 4` above the interrupt notice and the command's real result is drawn as `● 1 tool call · 1 command` / `└ Ran sleep 4` under it when it comes, then `the turn was cancelled`. A `Cancelled` row at the keystroke would claim a stop the command never had. This is a backend limitation, reported, not fixed here. The third prompt's block in `long-stopped` and `long-later`.
4. **`├ Read a.txt` scrolled off the top of xfx's screen** in `long-ask` … `long-later`: xfx writes two more lines after the stop at the approval (`xfx: interrupted …`, `the turn was cancelled` -- F4's), so its screen is one row further down by then. The row is in both scrollbacks (`*-end.full.txt`).

Outside the group, and owned by other units:

- The approval band itself (xfx `Permission needed` / `> 1. Yes` / the `2 = allow …` row vs fx's `Review change` diff block and hint row) -- F2.
- `xfx: interrupted -- …` and `the turn was cancelled` after a stop, where fx shows nothing more (`■ …` is the turn's last word) and `press ctrl+c again to exit` -- F4.
- The turn summary's output count (`13s (↑8 ↓40)` vs `↓320`) and the activity row's (`↓9` vs `↓120`): xfx's tally counts provider usage, fx counts streamed tool arguments too. Not this unit's.
- Blank rows between `└ Running sleep 4` and the band in xfx's `long-running`: the approval panel grew the band on a full screen and carried the document up; when it closed, the rows the band gave back stay blank until something is written (`src/tui/frame.rs` `release`, `render_append`'s `vacated`). The same happens to a plain line on the base build `0ce2d17` (a scratch capture of this scenario, not kept: `[tool] write_file running`, a blank row, then the interrupt notice). [추정] that nothing else contributes: the geometry was not traced past those two functions. fx re-renders the rows it owns. Band geometry, not the group.
- The welcome line's version (`x𝒇x v0.1.0-0ce2d17 [debug]`) -- the binary's own build stamp.

## Not in this receipt

Command output stays out of the inline transcript on both (`tool_group_projection.zig:1150-1158`); Ctrl-O is a later unit.
