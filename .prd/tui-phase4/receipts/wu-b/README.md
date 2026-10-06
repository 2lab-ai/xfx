# WU-B receipt: the slash popup, fx c1db919 beside xfx

Captured on 2026-10-07 with the repository's harness. Each run had its own tmux socket, an empty HOME, a fake key and a loopback fake gateway:

```
scripts/fxcap/capture.sh {fx,xfx} slash        40x120 <dir>
scripts/fxcap/capture.sh {fx,xfx} slash-filter 40x120 <dir>   # added in this unit
scripts/fxcap/capture.sh {fx,xfx} slash        12x60  <dir>   # small screen; only slash, down3 and down7 are kept
```

`fx` is the c1db919 build. `xfx` is `target/debug/xfx` from this branch. Each snap has a `.txt` file (text) and an `.ansi` file (`capture-pane -e`). Below, "band rows" means the rows from the composer (`┃ /…`) down to the hint row. "Identical" means the `.ansi` rows are byte-equal.

xfx has 7 of fx's 36 commands, so counts, ranges and the commands in view differ by design. The comparison is about shape: the header format, the blank row, the row format, the name column, the category column, the colours, the key-help row and the filter header.

## Verdicts

| Snap | Band rows identical | What differs, and why |
|---|---|---|
| `slash` 40x120 | 5 of 12: composer, both rules, the blank row, the key help | Header: `Commands 7 · type to filter … 1–6` against fx's `Commands 36 …` (7 commands). Same format, same 245 grey, and the range ends at column 119 in both. Item rows: same SGR shape (marked row `1;255` with the summary and category in 245 and still bold; other rows 245). The name column is 8+3 (`/version` is in view) against fx's 7+3 (`/resume`). Categories share one column ending at column 119 in both. The commands differ (xfx has 7). |
| `down3` 40x120 | 5 of 12 | Three Downs mark the 4th row in both (`/setup` against fx's `/reset`). The window does not move. |
| `down7` 40x120 | 5 of 12 | fx: `3–8`, `/logout` marked. xfx: 7 Downs over 7 commands wrap to `/help` with the window at `1–6`. fx's wrap rule (`list_window.advanceSelection`) gives this result with a list of 7. |
| `filter-re` 40x120 | n/a | No xfx command has `re` in it (fx matches `/reset /resume /rename /credits`), so xfx opens no menu. The filter header is compared in `slash-filter` instead. |
| `esc` 40x120 | 2 of 3 | fx's Escape closes the menu. xfx had no menu at `/re`, so the same Escape arms the clear gesture (`esc again to clear`). Both keep the composer text `/re`. The menu-open Escape is compared in `tab-vers-esc`. |
| `help-typed` 40x120 | **12 of 12** | none |
| `filter-q` 40x120 | **12 of 12** | none: `Commands 1`, one marked row (`/quit … General`), five blank rows, key help |
| `filter-mo` 40x120 | 11 of 12 | The `/model` summary. xfx keeps its own text because fx's mentions a reasoning effort that xfx does not send. The layout is the same: name column 6+3, `Model` at column 115. |
| `filter-ex` 40x120 | **12 of 12** | none: the alias shows under its own name, `/exit` |
| `tab-ex` 40x120 | **12 of 12** | none: Tab completes to `/exit`, and the menu follows the completed word |
| `tab-vers` 40x120 | 11 of 12 | Only the summary text: `show the xfx version` against `show the fx version` (xfx's `/version` names xfx, as the brief asks). The menu stays open on the completed word in both. |
| `tab-vers-esc` 40x120 | **3 of 3** | none: Escape closes the menu, the text stays, and the hint row comes back |
| `slash` 12x60 | 5 of 6 | Only the `/clear` summary (xfx keeps its own, see below). Both have a two-row budget, no header, `…` truncation, and the categories in one column ending at column 59. |
| `down3` 12x60 | 5 of 6 | The second command row (`/setup` against `/reset`). Both scroll the two-row window so the marked row is at the bottom. |
| `down7` 12x60 | 4 of 6 | Different commands, because xfx wraps after 7 rows. |

Verdict: **the popup's shape matches fx in every snap where both binaries have a match.** Five snaps have identical band rows (`help-typed`, `filter-q`, `filter-ex`, `tab-ex` and `tab-vers-esc`). In every other snap, each difference is either fx's own rule applied to xfx's 7 commands or a summary that xfx keeps on purpose.

## Summaries kept from xfx (not fx's text)

- `/clear` keeps xfx's summary until its behaviour changes. This is the brief's exception, and fx's text is at `commands.zig:464`.
- `/model`: fx says "choose what model and reasoning effort to use" (`commands.zig:478`). xfx has no reasoning effort.
- `/setup`: fx's row is `/provider` with `/setup` as its alias (`commands.zig:472`). Its text says "choose the model provider and how it signs in". xfx's `/setup` takes the provider as an argument and does not sign anything in.

In all three, fx's sentence describes behaviour that xfx does not have. Under CONTRIBUTING rule 1, a description advertises behaviour.
