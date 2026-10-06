# F1 receipt: the tool group

Scenario `scripts/fxcap/scenarios/toolgroup.cap` (turns `toolgroup-fx.json` / `toolgroup-xfx.json`, workspace `toolgroup-ws/`), captured at 40x120 with `scripts/fxcap/capture.sh` for fx `c1db919` (`FX_BIN`, built locally) and for xfx at this branch (`target/debug/xfx`). Both run in `ask` mode, so every change and command is decided by a key. Scratch paths in the copies here are replaced by `<scratch>`; the request bodies the gateway kept are not copied (they carry each product's system prompt and the scratch workspace path).

```
FX_BIN=<fx c1db919> scripts/fxcap/capture.sh fx  toolgroup 40x120 <out>
                    scripts/fxcap/capture.sh xfx toolgroup 40x120 <out>
python3 compare-group-rows.py <out>     # -> compare-group-rows.txt
FX_BIN=<fx c1db919> scripts/fxcap/capture.sh fx  toolgroup-noop 40x120 <out>
                    scripts/fxcap/capture.sh xfx toolgroup-noop 40x120 <out>
```

What the turn does: a sentence and two reads (one step), a glob and a grep (a second step), a command that sleeps 3 s (approved with `1`), an edit (`1`), a write (`3`, refused), a read of a missing file, a command that exits 3 (`1`), then the answer. A second prompt asks for a write and stops the turn at its approval; a third approves a `sleep 4` and presses Ctrl-C while it runs. `toolgroup-noop` reads `b.txt` and writes it back unchanged.

The xfx copies were recaptured after `plan/tui-phase4` was merged in a second time (89a4e91: F2's approvals with upstream's keys, C3-C5), so the stop at the second prompt's approval is Escape on both binaries.

## Matches

`compare-group-rows.txt`: **94 of 142** group rows (`●`/`├`/`└`/`■` rows, every snapshot plus the whole scrollback `end.full`) are byte-identical in tmux's re-serialized ANSI. The 48 that differ are the four reasons below and nothing else: 9 + 6 + 9 + 24. In `end.full` every group row is present exactly once on both (fx 16, xfx 15: reason 3's block is one row shorter).

- **Header**: `● 9 tool calls · 4 read · 2 commands · 1 list · 1 write · 1 edit · 2 failed · 1 denied` -- count, categories by count with ties in upstream's order, `commands` plural, then the outcome counts; `ESC[38;5;255m●ESC[39m ESC[38;5;245m…ESC[39m`.
- **One group across steps**, broken only by text: reads, glob, grep, the command, the edit, the refused write, the failed read and the exit-3 command are one group of nine; `All done.` closes it (`end`).
- **Rows**: `Read a.txt`, `Matched **/*.rs`, `Searched alpha`, `Ran <cmd>` / `Running <cmd>`, `Edited a.txt +1 / -1`, `Denied file`, `Exited 3 <cmd>`, `Cancelled file`; whole row `ESC[38;5;245m`, command words lifted to 252 (`;`, `>&` 252, the fd `2` 250) by the shell highlighter.
- **The running row** `└ Running sleep 3; echo hello from shell; ls` under the group while its approval is up (`cmd-ask`) and while it runs (`cmd-running`), rewritten in place to `├ Ran …` (`edit-ask`).
- **A file change has no row until it is decided**: the edit and the write waiting on their approvals are neither rows nor counted (`edit-ask`, `write-ask`).
- **The activity row is off the band while an approval is up** (`cmd-ask`, `exit-ask`, `long-ask`: no `Running`/`Thinking` row), back when it is answered (`cmd-running`).
- **A call stopped at its approval**: `● 1 tool call · 1 write · 1 cancelled` / `└ Cancelled file`, a blank row, `■ Cancelled file · What can … do differently?` (`again-stopped`), `ESC[38;5;252m■ESC[38;5;255m CancelledESC[39m ESC[38;5;245mfileESC[39m · …`.
- **Placement**: the group sits one blank row under the sentence before it and one above the answer after it; while open, the activity row is one blank row under it (`cmd-running`: rows 7-14 are the same on both).
- **A change that changed nothing** (`toolgroup-noop`): `● 2 tool calls · 1 read · 1 write` / `├ Read b.txt` / `└ No changes to b.txt; it already contains the requested content`, no approval on either, the file untouched (`*-noop-*-work-end.txt`). All three group rows byte-identical in ANSI (not counted in the 142).

## Differs, and why

1. **`Failed missing.txt: <reason>`** -- fx says `Path not found: missing.txt`, xfx says `read_file refused the path: no such path: \`missing.txt\``. The reason is the tool's own one-line summary (`ToolResult::detail`); the wording is xfx's tool backend, not the row (fx `src/core/tooling/tool_admission.zig:2471`). One row in each of 9 snapshots.
2. **`What can xfx do differently?`** -- the product's own name in upstream's follow-up (`src/ui/transcript/runtime.zig:644`). One row in each of 6 snapshots.
3. **Ctrl-C while a command runs** (`long-stopped`, `long-later`). fx kills the command at once: `└ Cancelled sleep 4` and `■ Cancelled sleep 4 · …`. xfx cannot stop a running command until it returns -- the executor holds the runtime's thread, and the control channel is polled beside the turn, not inside a tool (`src/tui/worker.rs` `raced_against_control`) -- so its row stays `└ Running sleep 4` with the interrupt notice under the group (`long-stopped`), and when the real result comes the same block is rewritten in place to `└ Ran sleep 4`, the notice still under it, then `✗ system: the turn was cancelled` (`long-later`). A `Cancelled` row at the keystroke would claim a stop the command never had. This is a backend limitation, reported, not fixed here. Three rows in each of `long-stopped`, `long-later` and `end.full`.
4. **The first six rows of the first group (`├ Read a.txt` … `├ Edited a.txt +1 / -1`) scrolled off the top of xfx's screen** in `long-ask` … `long-later`: after the stop at the approval xfx writes C0's two notices, each with its blank row (`⊘ turn: interrupted -- …`, `✗ system: the turn was cancelled`), where fx writes none, and its command question is two rows taller than fx's -- the `2 = allow every future run …` disclosure, F2's -- so its screen is six rows further down by then. The rows are in both scrollbacks (`end.full`). Six rows in each of 4 snapshots.

Outside the group, and owned by other units:

- The approval band itself -- upstream's layout and keys since F2, plus xfx's `2 = allow …` disclosure rows under the choices -- F2's.
- `⊘ turn: interrupted -- …` and `✗ system: the turn was cancelled` after a stop, where fx shows nothing more (`■ …` is the turn's last word) and `press ctrl+c again to exit` -- C0's notices, F4's wording.
- The turn summary's output count (`13s (↑8 ↓40)` vs `↓320`) and the activity row's (`↓9` vs `↓120`): xfx's tally counts provider usage, fx counts streamed tool arguments too. Not this unit's.
- Blank rows after a closed approval: the panel grew the band on a full screen and carried the document up (`Band::carry_document`); when it closed, the rows the band gave back are blank (`release`). They sit between `└ Running sleep 4` and the band in `long-running`, and between `┃ run the long one` and the group once the group is rewritten next to the band (`long-stopped`, `long-later`). fx re-renders the rows it owns. Band geometry, not the group.
- The welcome line's version (`x𝒇x v0.1.0-0ce2d17 [debug]`) -- the binary's own build stamp.

## Not in this receipt

Command output stays out of the inline transcript on both (`tool_group_projection.zig:1150-1158`); Ctrl-O is a later unit.

## Follow-up after the merge of `plan/tui-phase4`

- **A line written under an open group does not split it.** fx keys a group by its presentation id -- one per turn, cleared only by visible text (`presentationGroupForStep`, `src/core/agent/runtime/tool_presentation.zig:667-708`) -- and draws every member at the group's anchor, hiding the rest (`tool_group_projection.zig:1185-1201`, `:1295-1300`), so an entry between two calls of one group stays under the whole group (`fx-approvals-40x120-cmd2`: `└ Running echo again` above `┃ use capitals`). xfx keeps such lines -- the `[you]` sentence and a notice alike -- as the open block's trailing rows: the group grows above them and its rows are rewritten in place (`long-stopped` → `long-later`: one block, `└ Running sleep 4` → `└ Ran sleep 4`, the notice under it). Text still closes it.
- **A change that changed nothing** is `No changes to <path>; it already contains the requested content` -- the result text fx uses as the status (`file_mutation_execution.zig:88`, `tool_presentation.zig:1046-1047`) -- not a failure and not counted as one (`toolgroup-noop`).
- **A theme report repaints a group** in the other palette's values for the roles the group uses: rows 245 → 241 (statusline, not dim's 247), the shell highlighter's command words 252 → 238; the `■` line's target is 245 in both modes. A row is told apart by its first cell (`●` in the hint grey, `├`/`└` in statusline grey, `■` in the warning grey), since dark 245 is both dim and statusline. Not captured: the harness has no way to change tmux's background report; proved by `toolgroup::a_theme_report_repaints_a_group_as_the_other_palette_would_draw_it` (RED before: `row 1, column 3: …247m against …241m`).
- **No duplicate group after an approval closes.** A scratch capture of the first fix above showed an old `● 1 tool call · 1 command` / `└ Running sleep 4` block left above the rewritten one (`long-stopped`, `long-later`, `end.full`). The frame that painted the shorter band landed before the block was rewritten and recorded its top as `painted`, so `render_append` measured what the block vacated as nothing and left the old copy where the carry had put it. The defect was already in F1's `render_append`; F1's capture did not show it because there the notice closed the group, so nothing rewrote a block after the panel closed. It now measures from `document_bottom`, the row the block's last row is on (`frame::a_block_re_placed_after_the_band_shrank_under_a_frame_leaves_no_copy`, RED before: `["answer: XFXMA", "answer: XFXMARK-COMPLETE"]`).
- **After the second merge, F2's amendment still lands under the group.** The draft is now typed on the choice row (`Apply once, use capitals`); what reaches the document after the answer is the same `UiEvent::ToolFeedback` row, and it goes through `trail_group` while the group is open. F2's own scenario `approvals.cap` on the merged binary (`xfx-approvals-40x120-cmd2-ask`, `-after-esc.full` here; fx's are in `../f2/`): the edit amended with `use capitals`, a write refused with Ctrl-C and two commands are one group of five, `[you] use capitals` under it, and the `■` line of the call Esc stopped between the two -- every `●`/`├`/`└` row byte-identical to fx's, and the rows in the same places through the amendment. What differs is the amendment's own row: fx's is a `┃ use capitals` card with a blank row under it, xfx's is still the `[you]` line with none (F2's row).
