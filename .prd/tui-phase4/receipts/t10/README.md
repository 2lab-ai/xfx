# T10 receipt: the user card and an approval's command wrap as fx's, fx c1db919 beside xfx

Captured on 2026-10-07 with the repository's harness. Each run had its own tmux socket, an empty HOME, the fake key, and a loopback fake gateway.

```
scripts/fxcap/capture.sh {fx,xfx} card-wrap  40x120 <dir>   # new in this unit
scripts/fxcap/capture.sh {fx,xfx} card-wrap  24x80  <dir>
scripts/fxcap/capture.sh {fx,xfx} cmd-wrap   40x120 <dir>   # new in this unit
scripts/fxcap/capture.sh {fx,xfx} cmd-wrap   24x80  <dir>
scripts/fxcap/capture.sh {fx,xfx} table-wrap 40x120 <dir>   # new in this unit
scripts/fxcap/capture.sh {fx,xfx} table-wrap 24x80  <dir>
```

`fx` is the c1db919 build. `xfx` is `target/debug/xfx` at `e19747f` (the banner's revision): this branch's port (`d70e235`, `6605035`) after merging `plan/tui-phase4` at `831deed` (`c3aec73`), `e3df90f` (`cdb2747`), `60fe9c6` (`047af0a`), `0a1bb45` (`f59ed24`), `227c976` (`0f7822f`) and `8240063` (`e19747f`). The xfx side was taken at `6605035`, `ed367c7` and `f59ed24` as plan moved, with the same verdicts row for row each time; the `card-wrap` and `table-wrap` files here are the `e19747f` take, and the `cmd-wrap` files the `585bae8` one (the grant row's spelling, below; the same verdicts again). `before-23907ea-*` is the same scenario on a debug build of `23907ea` -- `plan/tui-phase4` before this unit (banner `23907ea`) -- taken the same session.

- `card-wrap` (`scripts/fxcap/scenarios/card-wrap.cap`, turns `card-wrap-turns.json`): two pasted prompts, each a set of lines aimed at one rule at one card width -- `A-*` at 118 cells (120 columns less the rail and its space), `B-*` at 78. Both prompts are on both screens, so each size also shows the other prompt's lines cut at its width. The history is longer than either screen: each size has the screen (`cards`) and the whole history (`cards.full`, `capture-pane -S -`).
- `cmd-wrap` (`cmd-wrap.cap`, turns `cmd-wrap-fx.json` / `cmd-wrap-xfx.json`, fx's `shell` being xfx's `terminal`): two commands, each refused with `3`. Command A's lines are aimed at 116 cells (120 less `  $ `); command B is two lines aimed at 76. Snaps: `cmd-a` (the first question), `cmd-b` (the second), `after`.
- `table-wrap` (`table-wrap.cap`, turns `table-wrap-turns.json`): one answer with a three-column table -- CJK cells that wrap at an even and an odd column width, a CJK cell with spaces, a long English cell, styles and a link inside a cell, the three alignments -- in 37-character chunks.

Waits are long (10 s before the first key, 20 s per turn) because the machine was loaded (load average 58-99).

Each snap has a `.txt` and an `.ansi` (`capture-pane -e`, OSC 8 kept by tmux 3.7b). "Rows identical" counts the non-blank rows **after the banner** that are byte-equal in the two `.ansi` files, aligned so an inserted row reads as an insertion. For the command snaps a second count is given for the **command rows** alone (the rows under `  $ #...`, up to the choices), which are what this unit changes.

**Sanitized**: every file was scanned for the run directory (`fxcap.`), `/private/tmp`, `/Users/`, the user name and the fake key; none matched (80 files). The request bodies and the harness's logs are not kept.

## Verdicts

| Snap | Rows identical | What differs, and why |
|---|---|---|
| `card-wrap 40x120 cards` | **31 of 31** | none |
| `card-wrap 40x120 cards.full` | **31 of 31** | none |
| `card-wrap 24x80 cards` | **19 of 19** | none |
| `card-wrap 24x80 cards.full` | **37 of 38** | `A-R8` (history row 18): fx's history line holds the two cells its raw tab skipped as a tab (`\t`), where xfx wrote two blank cells; the screen is the same, and the same row of fx's on the 40x120 screen is spelled with blanks, as xfx's is |
| `before-23907ea card-wrap 40x120 cards.full` (vs fx's) | 21 of 31 | the rows this unit changes, below |
| `before-23907ea card-wrap 24x80 cards.full` (vs fx's) | 22 of 38 | the same |
| `cmd-wrap 40x120 cmd-a` | 21 of 24; **command rows 10 of 10** | xfx's comment line (`# terminal.exec cwd=.`, fx's `# shell.run profile=user shell=/bin/zsh`); fx's trailing empty command row (row 24 -- xfx's permission layer trims the command, `src/permission/authority.rs:1037`); xfx's own `2 = allow every future run...` scope rows; the blank rows a shorter band leaves |
| `cmd-wrap 40x120 cmd-b` | 14 of 16; **command rows 2 of 2** | the comment line; xfx's grant rows |
| `cmd-wrap 40x120 after` / `.full` | **8 of 8** / **8 of 8** | none |
| `cmd-wrap 24x80` fx `cmd-b` vs xfx `cmd-a` | 12 of 15; **command rows 4 of 4** | the same question on both screens: xfx refused command A as too small for its band (`✗ permissions: this screen is too small to ask for permission`) -- its scope rows do not fit at 24 rows -- so its first question is B, which is fx's second. Before this unit too (`before-23907ea`) |
| `cmd-wrap 24x80 cmd-a`, `cmd-b`, `after` | 8 of 22, 0 of 15, 1 of 5 | the questions are not the same ones (above) |
| `before-23907ea cmd-wrap 40x120 cmd-a` (vs fx's) | 16 of 24; command rows 5 of 10 | the rows this unit changes, below |
| `before-23907ea cmd-wrap 40x120 cmd-b` (vs fx's) | 13 of 16; command rows 1 of 2 | the same |
| `before-23907ea cmd-wrap 24x80 cmd-a` vs fx `cmd-b` | 9 of 15; command rows 1 of 4 | the same |
| `table-wrap 40x120 answered` / `.full` | **24 of 24** / **24 of 24** | none |
| `table-wrap 24x80 answered` / `.full` | **19 of 19** / **37 of 37** | none |

## The card: fx's `wrapCut`, rule by rule

fx: `vercel-labs/fx@c1db9191 src/ui/assistant/user_message_card.zig` (`wrapCut` `:57-77`, looped by `collectLogicalLinesWithFirstPrefix` `:349-383`). xfx: `wrap::wrap_card` (`src/tui/wrap.rs:408`, `card_cut` `:442`), behind `Gutter::Rail` (`src/tui/transcript.rs:477`), painted by `body_rows` (`:1361`). Rows are fx's `.full.txt` rows.

| Rule | fx file:line | xfx before | xfx after | Where it shows |
|---|---|---|---|---|
| Width: the screen less the rail and its space | `:201` | `cols - 2` (`transcript.rs:1247` `inner`) | unchanged | every row |
| A line is cut on its own; an empty line is a rail-only row | `:358-368` | the transcript splits on `\n` (`transcript.rs:1047`) | unchanged, and `wrap_card` splits too | 40x120 row 17 |
| The longest run that fits, escapes and zero-width units carried wherever they fall, the ones before the unit that does not fit included | `:59`; `display_width.zig:178-194` | the row ends where a word or the crossing cluster begins, escapes in front of it moving down (`wrap.rs:254-266`) | `card_cut` | (unit test `a_card_row_is_wrap_cuts_keep_and_the_next_begins_at_its_skip`) |
| The rest of the line fits: it is the row | `:60` | same | same | -- |
| Otherwise the row ends at the **last** space or tab inside the run, and exactly one blank is dropped | `:62-64`, `lastSpaceIn` `:45-52` | the word moves down whole and **every** space hangs on the row it ends (`wrap.rs:241-266`) | as fx | all rows |
| A word that ends at the margin, an earlier space inside the run: the word goes down | `:59-64` (the run stops before the space that would cross) | the word stays; the space hangs | as fx | 40x120 9-10 `... the quick` / `ends-at--------- goes down whole`; 24x80 30-31 |
| A word that ends at the margin, no earlier space: it is the row, and the next begins **on** the space -- a leading blank when the rest fits | `:59-64`, `:369-381` | the next row begins at the word | as fx | 40x120 3-4 ` then a row that starts with a blank`; 24x80 24-25 |
| ... and an **empty card row** when nothing after the space fits beside it | `:62-63` with `sp = 0` | no empty row | as fx | 40x120 5-6, 24x80 26-27 (the rail and nothing) |
| Of a run of spaces at a break, one is dropped and the rest kept (end of the row, start of the next) | `:63` (`skip = sp + 1`) | all hang on the row | as fx | 40x120 11-12 `... spssssss ` / ` after three spaces`; 24x80 32-33 |
| Kept spaces are inside the row's bold | `:373-379`, `emitRow` `:39-43` | a hung space was written after the bold closed (`transcript.rs` `body_rows`, `hung`) | inside (`transcript.rs:1431`) | `.ansi` of the rows above |
| No blank inside the run: the run is the row (a long word cut at the margin) | `:76` | same rows (the crossing cluster) | same | 40x120 13-14, 24x80 34-35 |
| A two-cell glyph that would cross the margin starts the next row | `displayUnitAt` widths | same | same | 40x120 15-16, 24x80 36-37 |
| Nothing fits: the first unit is the row | `:66-75` | one cluster per row (`wrap.rs:254`, `index > start`) | `card_cut` `fits == 0` | (unit test, 1 column) |
| A **tab** is a blank of no width in the cut | `:49`; `display_width.zig:241-243` (a control is 0 cells) | four cells (`wrap.rs:53` `TAB_WIDTH`) | 0 (`wrap.rs:483` `card_cells`) | -- |
| ... written as it is, so the terminal moves to the screen's next 8-column stop and the cells it skips are blank | `:376` (the fragment written raw) | painted as four bold spaces | the spaces to the screen's stop, the pen closed around them (`transcript.rs:1462` `card_tabs`) | 40x120 18 `A-R8  tab     stops`; before: `A-R8    tab    stops` |
| A badge's link is opened again on each row it continues on and closed at each row's end | `:361`, `:375-378`, `hyperlinkStateAfter` `:327-346` | the transcript's pen state (`body_rows`), unchanged | unchanged; fx's own case pinned (`AA [Image #1] ZZZ` at 7 columns, `:713-761`) | (unit test) |
| The rail is on every row, empty ones included | `:319-325`, `:363-367` | `texts` (`transcript.rs`) | unchanged | 40x120 6, 17 |

## The command: fx's `CommandSegmentIterator`, rule by rule

fx: `src/ui/footer/approval_ui.zig` (`projectCommandText` `:228-247`, `CommandSegmentIterator` `:265-340`, rows `:350-366`), `src/core/shared/text_utils.zig` (`terminalSafeToken` `:560-584`, `isNonPrintingCodepoint` `:620-626`, `prefixTerminalSafeByWidth` `:294-310`, `terminalSafeEncodedTokenLen` `:312-341`), `display_width.zig` (`wrapCutIgnoringAnsi` `:354-374`, `trimBreakWhitespace` `:376-380`). xfx: `approval_view::command_rows` (`src/tui/approval_view.rs:1057`) over `command_text` (`:1008`), used at `:666`; the bridge spells the command at the seam with `command_text` (`src/tui/bridge.rs:651`). Rows are the `.txt` rows of fx's `cmd-a` at 40x120 and `cmd-b` at 24x80.

| Rule | fx file:line | xfx before | xfx after | Where it shows |
|---|---|---|---|---|
| Width: the panel less `  $ ` | `approval_ui.zig:273-278` | `cols - 4` (`approval_view.rs:666`) | unchanged | every row |
| Lines are cut on their own; an empty line, and a trailing line break, is an empty row | `approval_ui.zig:290-305`, `:332-339` | split on `\n` **and `\r`** (`safe_rows`, `approval_view.rs:967`) | split on `\n` only | 40x120 19 |
| A C0 control and `DEL` are `\xNN`, lowercase -- a tab `\x09`, an `ESC` `\x1b`, a `CR` `\x0d` | `text_utils.zig:560-563`, `writeByteEscape` `:592-603` | blanked to a space by the bridge (`bridge.rs` `inert`); `tamed` names `\t` `\r` and writes `\u{001B}` | as fx (`command_text`) | 40x120 17-18 `\x1b[31mred\x1b[0m`, `&&\x09tab`; before: ` [31mred [0m`, `&& tab` |
| C1 controls, `U+200B-200F`, `U+2028-202E`, `U+2060-206F`, `U+FEFF` are `\u{NNNN}`, at least four lowercase digits | `:575-578`, `:620-626`, `writeCodepointEscape` `:605-618` | `\u{XXXX}` uppercase for controls and xfx's bidi set (`approval_view.rs:915-938`) | as fx, plus `U+061C` (below) | (unit test) |
| A spelled token is one unit of the cut and is never split | `text_utils.zig:294-310`, `:312-341`; `approval_ui.zig:306-310` | cut anywhere a cluster crosses | as fx (`token_prefix`, `token_len`) | 40x120 16-17 `A-C2:xxx...` / `\x1b[31mred...`; 24x80 11-12 |
| The cut is the tokens that fit, brought back to the last space inside the width (not the first byte) when that is shorter | `approval_ui.zig:312-321`; `display_width.zig:354-374` | the word moves down whole, its space hung and trimmed (`approval_view.rs:973-977`) | as fx (`soft_cut`) | 40x120 14-15 `... past the margin` / `eeeeeeeeeeeee goes down`; 24x80 9-10 |
| Every blank after the cut is dropped | `approval_ui.zig:325-327` | the next row never began with one either | as fx | 40x120 22-23 |
| The blanks before the cut stay on the row | `display_width.zig:370-372` (the cut is before the last space) | trimmed (`trim_end`) | kept | 40x120 22 (`... zzzzzz ` in the `.ansi`) |
| A token wider than the whole row | `approval_ui.zig:310` (`error.CommandDoesNotFit`) | one cluster per row | a row of its own | not reachable: the narrowest band leaves 16 cells |
| `  $ ` on the first row, `    ` on the rest | `approval_ui.zig:350-366` | same | same | every row |

## Table cells: fx's `wrapTableCell`, checked

`src/tui/blocks.rs` was already a line port of fx's table renderer, measured with fx's own `display_width` (`crate::markdown::display_width`), and its golden cases were rendered by fx `c1db9191` itself (`blocks.rs` `FX_GOLDENS`). Nothing to port; the receipt confirms it live, and `wrap_table_cell_cuts_wide_glyphs_whole_at_the_column_edge` adds CJK cases.

| Rule | fx `transcript_blocks.zig` | xfx `blocks.rs` | Where it shows |
|---|---|---|---|
| Column widths: natural, settled at an even share, the rest scaled, words kept when every column can | `fitTableColumnWidths` `:981-1053` | `fit_table_column_widths` `:611` | the three columns at both sizes |
| A cell line: the last space inside the width, or the width itself when a space follows | `tableCellCut` `:1490-1494` | `table_cell_cut` `:904` | 24x80 the `Notes` column |
| The spaces at a break are dropped, escapes among them only update the carried state | `skipTableCellBreak` `:1498-1511` | `skip_table_cell_break` `:914` | -- |
| A leading space is a break, never a line of its own; an empty cell is one empty line | `:1450-1462`, `:1476` | `wrap_table_cell` `:843` | -- |
| A two-cell glyph that would cross the column edge starts the next line | `display_width.prefixByWidthIgnoringAnsi` | the same function, ported | 24x80 rows 10-11 `...列の幅を超` / `えて折り返される...` (a 41-cell column) |
| Styles and a link are re-opened per line and closed before the padding | `:1464-1474`, `trackTableCellEscapes` `:1514-1526`, `appendTableCellLine` `:1528-1542` | `append_table_cell_line` `:956` | 40x120 / 24x80 the `**bold** and `code` and [a link]` cell |

## What does not match, and why

- **The banner** (xfx's brand line) and, on the command screens, everything that is not the command's rows: the comment line names xfx's own tool, and xfx's band adds what `2` would grant (pre-existing, not this unit).
- **The 24x80 command A.** xfx refuses it as too small, fx asks it. Its scope rows do not fit the band at 24 rows; the same was true before this unit. B's command rows are four on both builds at 24x80 (`B-C1` now breaks as fx's), inside the band.
- **A trailing line break in a command** is a row in fx's question (40x120 `cmd-a` row 24); xfx's permission layer trims the command before anyone asks (`src/permission/authority.rs:1037`), so it has no such row. Not a wrap rule.
- **`U+061C` (Arabic letter mark) is named in a command** (`\u{061c}`), where fx shows it as it is: xfx's bidi rule names it (`approval_view.rs:915`), and the command is what a reviewer is asked to run.
- **A command's grant row** (xfx's own `2 = allow every future run of exactly ...`, not fx's) quotes the command spelled as the rows above it are (`585bae8`; before that it was blanked by the bridge's `inert`). It is wrapped by `safe_rows`, the composer's rule, which can cut between a spelled `\x1b` and what follows it (40x120 `cmd-a` rows 29-30); fx has no such row to match.
- **fx's command screen** (`approval_screen.zig:208-223` `needsScreen`, `:511-520`, cut at `width - 5`) has no counterpart: xfx never moves a command to its review plane -- a command the band cannot hold is refused (`src/tui/shell.rs:3797-3807`, `reviewable` is false without a diff) -- so `subject_rows` on the plane never lays out a command, and that width is not ported.
- **The card's blank is found among clusters, fx's among bytes.** A space inside an escape sequence is never a break in xfx; fx's `lastSpaceIn` would break there. The only sequences a card carries -- its bold and a badge's link, whose target is percent-encoded (`src/tui/hyperlink.rs`) -- have no space in them, so the breaks are the same.
- **Measure.** The card and the command are measured with the painter's widths (`wrap::cluster_width`, `unicode-width`), as T7's answer wrap is; fx measures with its own tables (`display_width.zig`). The two agree on every row here; a cluster the tables disagree on would be cut a cell apart. Tables use fx's tables.
- **A card at 2 columns or fewer** has no rows in fx (`:206-209`); xfx's screen is never narrower than 20 columns (`src/tui/layout.rs:38`).

## Earlier receipts whose rows change

None. Every xfx `.txt` under `.prd` (930 files at `e19747f`, this directory excepted) was scanned for a card of more than one row (a run of rail rows) and for a command question with a wrapped row near the margin: every multi-row rail run is composer rows in the band, with the status line under it (`d2c/xfx-model-anchor-40x120-alt-enter`, `e1/xfx-filepicker-keys-40x120-pg{up,dn}`, `w2/xfx-paste-block-undo-40x120-yanked` and fifteen `w2/xfx-composer-wrap-*`), which this unit does not change, and no command question wraps. The one set of multi-row **cards** arrived with W2 round 9 (`w2/xfx-paste-card-40x120-*`: empty lines and runs of one letter, cut where there is no space); `paste-card` was taken again at `f59ed24` and every row of all seven of its snaps is byte-equal to W2's committed xfx `.ansi` (`small` 5, `multi-composer` 7, `multi` 10, `large-composer` 10, `large` 24, `resumed` 24, `end.full` 27). No scenario's command carries a control character, so no earlier question changes from the bridge's spelling either.

`scripts/smoke-tui.sh` (not edited): its `cards()` reader joins a card's rows with nothing between them (`:2938-2959`); with fx's cut a prompt that breaks at a space loses that one space in the join. Its one claim that reads a card -- scenario 6's capped composer (`scenario_6`, `:4467`), `"0123456789012345678" * 8` at 20 columns (`:4580-4586`) -- has no space in it, so its claim is unchanged.
