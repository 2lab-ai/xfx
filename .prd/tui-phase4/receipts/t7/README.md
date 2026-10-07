# T7 receipt: the answer wraps by fx's assistant wrap, fx c1db919 beside xfx

Captured on 2026-10-07 with the repository's harness. Each run had its own tmux socket, an empty HOME, the fake key, and a loopback fake gateway; all ten snaps below come from **one run** (both binaries, both sizes, started together).

```
scripts/fxcap/capture.sh {fx,xfx} prose-wrap       40x120 <dir>   # new in this unit
scripts/fxcap/capture.sh {fx,xfx} prose-wrap       24x80  <dir>
scripts/fxcap/capture.sh {fx,xfx} fast-unconfirmed 40x120 <dir>   # D2c's, unchanged, retaken
```

`fx` is the c1db919 build. `xfx` is `target/debug/xfx` at `2a4aa7f` (the banner's revision): this branch's port (`b2910c0`) after merging `plan/tui-phase4` at `33b963f` (merge `3febdbf`). `before-13dac71-*` is the same `prose-wrap` scenario at 24x80 on a debug build of `13dac71` -- `plan/tui-phase4` before this unit, built from an export of that commit, so its banner has no revision -- taken the same session for the comparison below.

`prose-wrap` (`scripts/fxcap/scenarios/prose-wrap.cap`, turns `prose-wrap-turns.json`) is one answer whose paragraphs are each aimed at one rule at one content width -- 118 cells (120 columns less the answer's two-column gutter) or 78 (80 less it) -- and arrives in 37-byte chunks cut inside words and across line ends. The answer is longer than either screen, so each size has the visible screen (`answered`) and the whole history (`answered.full`, `capture-pane -S -`). Its waits are long (10 s before typing, 1 s before Enter) because the machine was loaded: on a first attempt with 4 s, xfx's composer showed the prompt with an empty second row under it and no turn was sent -- the Enter landed as a line break, consistent with keys typed before the binary was in raw mode.

Each snap has a `.txt` and an `.ansi` (`capture-pane -e`, OSC 8 kept by tmux 3.7b). "Rows identical" counts the non-blank rows **after the banner** that are byte-equal in the two `.ansi` files, aligned so an inserted blank row reads as an insertion.

**Sanitized**: every file was scanned for the run directory (`fxcap.*`), `/private/tmp`, `/Users/`, the user name and the fake key; none matched. The request bodies are not kept (they carry the run's working directory); the harness's own logs are not copied (the fake gateway's are, for `fast-unconfirmed`).

## Verdicts

| Snap | Rows identical | What differs, and why |
|---|---|---|
| `prose-wrap 40x120 answered` | **27 of 27** | none |
| `prose-wrap 40x120 answered.full` | **32 of 32** | none |
| `prose-wrap 24x80 answered` | **16 of 16** | none |
| `prose-wrap 24x80 answered.full` | **42 of 42** | xfx's history holds two blank rows fx's does not, after `Overflow path at 78 columns: ...` (xfx rows 18-19). **Not this unit's**: `before-13dac71` has the same two rows, after the same paragraph -- see below |
| `fast-unconfirmed 40x120 turn` | **8 of 8** | none. D2c's was 6 of 8: xfx's `... a standard or lower` / `tier.` is now fx's `... a standard or` / `lower tier.` |
| `before-13dac71 prose-wrap 24x80 answered.full` (vs fx's) | 30 of 42 | the rows this unit changes, listed below |

## Each rule, where it shows

Row numbers are fx's `.full.txt` rows; xfx's are the same at 120 columns, and at 80 the same up to row 17 and two more after it.

| Rule (fx `assistant_wrap.zig`) | 40x120 (118 cells) | 24x80 (78 cells) |
|---|---|---|
| No orphan, at the space that crosses (`:219-267`): the row is full, the space after it crosses, the next word is the line's last, so the word before goes down with it | 7-8 `... what is left of` / `the-x end.` | 10-11 `... and moves it` / `down-xxx end.` |
| No orphan, at the word that crosses (`:269-298`), two words staying behind | 12-13 `... what is` / `left finale.` | 16-17 `... and moves` / `it finale.`; 38-39 in a list item `... three columns in from the` / `left edge.` |
| Not asked mid-paragraph | 17-19 | 19-23 |
| Not applied when the pair would not fit a row (`wordBreakFits`, `:830-842`) | 21-22 `No room 118: Aaa...` / `Bbb....` | 29-30 `No room 78: Aaa...` / `Bbb....`; 25-27 a word longer than a row breaks inside it |
| List continuation under the text (`listContinuationIndentWidth`, `:1055-1098`) | 26-27 bullet (2), 30-31 ordered `1.` (3), 32-33 nested bullet (5) | 32-34, 37-39, 40-41 |
| Pen carried across a break behind the lead (`:682-727`) | -- | 35-36: bold closed `\e[0m` at the row's end, the lead, then `\e[1m` again |
| Quote rule repeated, dim, on every continuation row (`blockquoteMarkerEnd`, `:1035-1052`) | 35-36, nested 37-38 | 43-44, nested 45-46 |
| Underline carried across a break (a heading has no lead) | -- | 48-49 |
| OSC 8 link closed at the break and opened again on the next row (`:693`, `:725`) | -- | 51-52 |

## Before this unit

`before-13dac71-xfx-prose-wrap-24x80-answered.full.*` against fx's: 30 of 42 non-blank rows identical. The twelve that differ are this unit's rules -- `moves it down-xxx` / `end.` and `moves it` / `finale.` (orphans), list rows continuing at column 3 instead of under the text (`  continuation rows ...`, `  letter after ...`, `  columns as well ...`), and quote continuation rows without their rules (`  front of the text ...`, `  continuation rows, at ...`).

## What does not match, and why

- **Two blank rows in xfx's 24x80 history** (`answered.full`, rows 18-19). Present on `13dac71` too (`before-13dac71`, rows 18-19, after the same paragraph), so it is not the wrap's. Both appear while the answer streams past the bottom of a 24-row screen, where the floating band stops floating; fx's history has none. Recorded here, not fixed in this unit.
- **The banner**: xfx's brand line (WU-A).

## Not in the scenario

- **Table cells**: fx wraps a cell with `wrapTableCell` (`transcript_blocks.zig:1443-1476`), not with its assistant wrap, so no table is in this receipt; xfx's table renderer is unchanged.
- **Code panels**: fx prefixes rendered rows (`prefixStructuralRows`, `assistant_wrap.zig:390-407`); unchanged.
- **Ctrl-O tool detail** (`wrapLiteralToolOutput`, `full_transcript_screen.zig:6384`): ported (`super::prose::literal`) and pinned by unit tests; no committed receipt row changes (every `│` detail row in `.prd` was scanned for a one-word last row).

## Earlier receipts whose rows change

- `d2c/xfx-fast-unconfirmed-40x120-turn.*`: rows 7-8 now break as fx's (retaken here).
- None else: every xfx `.txt` under `.prd` was scanned for an answer row near the margin followed by a one-word row, and for a wrapped list or quote row followed by a row not under its text. The other hits are notice rows (`tools: ... +1` / `more)`, `wrap_notice`), picker rows and question-panel rows, none of them answer prose.
