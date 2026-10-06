# C1 receipt: the `/help` catalog, fx c1db919 beside xfx

Captured on 2026-10-07 with the repository's harness. Each run had its own tmux socket, an empty HOME, a fake key and a loopback fake gateway:

```
scripts/fxcap/capture.sh {fx,xfx} help 40x120 <dir>   # scenario added in this unit
scripts/fxcap/capture.sh {fx,xfx} help 24x80  <dir>   # small screen; only open, btab3-product and query-version are kept
```

`fx` is the c1db919 build. `xfx` is `target/debug/xfx` built from this branch's working tree just before the commit, so its banner names the parent commit (`v0.1.0-04d9593`). Each snap has a `.txt` file (text) and an `.ansi` file (`capture-pane -e`). "Band rows" means the rows from the last composer row (`┃ …`) down to the hint row, and "identical" counts the rows that are byte-equal in the `.ansi` files **at the same offset** from that composer row. Where xfx's list is shorter than fx's, its bottom rule and key-help row are byte-equal to fx's too but sit higher, and are not counted.

**Sanitized** means: every file was scanned for the run directory (`fxcap.*`), home and temp paths, and the fake key. Nothing matched -- the scenario never runs `/status`, which is the fx command that prints the run directory -- so no file was edited; they are the captures as written.

xfx lists 7 commands against fx's 36, so counts, rows and the summary column (indent + widest name **in the filtered list** + 4) differ by design. The comparison is about shape: the header and its tabs, the blank row, the row format, the colours, the height, the key-help row, and what every key does.

## Verdicts, 40x120

| Snap | Band rows identical | What differs, and why |
|---|---|---|
| `open` | 3 of 13 (composer, top rule, blank row) | Header `Commands 7` with every tab against fx's `Commands 36 … Extensions  …`: fx's own rule (`help_menu_presentation.zig:150`, `<= width`) on a one-digit count -- `Commands 7` and the twelve tabs are exactly 120 cells, `Commands 36` is 121. Rows: same SGR shape (marked row name and summary each `1;255` and closed by `0m`, uncoloured padding between; others `245`), summary column 14 (`/version`) against fx's 18 (`/permissions`). xfx shows its 7 commands; fx shows 20 of 36. |
| `tab-general` | 4 of 10 | `[General]` active and unpacked in both. 4 commands against 5 (xfx has no `/status`). Summary column 14 in both (`/version`), and the `/help` row is byte-equal. |
| `tab-session` | 3 of 7 | xfx's Session tab is `/new` alone, so its column is 2+4+4. |
| `btab3-product` | 3 of 7 | Three Shift-Tabs from Session reach `[Product]` in both. xfx files nothing under Product: `Commands 0` and `No commands found.` (fx's empty-tab rendering, which fx shows for Agents). |
| `all-up-wrap` | 3 of 13 | Up on the first row wraps to the last command in both. fx scrolls its 20-row window to `/trace`; xfx's 7 commands all fit, so only the mark moves (to `/model`). |
| `all-down-wrap` | 3 of 13 | Down wraps back to the first command and the top of the window in both. |
| `query-session` | 3 of 8 | The composer is the search and no slash popup opens, in both. xfx finds `/clear` (its summary says "session") and `/new`; fx finds 9. |
| `query-none` | **7 of 7** | none: `Commands 0`, the tabs, `No commands found.` at column 0 in 245 |
| `ctrl-u` | 3 of 13 | C-u empties the search and the catalog stays up, in both. Rows differ by registry. |
| `query-ses` | 3 of 8 | 2 matches against 10. |
| `esc-clears-query` | **3 of 3** | none: Escape closes the catalog and empties the composer; the hint row is back |
| `tab-account` | 3 of 7 | `[Account]`: xfx `/setup`, fx `/login /logout /provider /stats /usage /credits`. |
| `enter-args` | 0 of 3 | Enter on a command that takes an argument puts `/cmd ` in the composer and runs nothing, in both. fx's marked row is `/login`, whose argument picker (vercel, codex, grok) then opens; xfx's is `/setup`, which has no argument picker. |
| `query-version` | 6 of 7 | Only the summary text: `show the xfx version` against `show the fx version`. Header, blank row, marked-row SGR, column 14, rules and key help are byte-equal. |
| `enter-row` | **3 of 3** | none in the band: Enter on the marked row (`version` is a search, not a command) ran `/version`. The answers above the band differ (see below). |
| `typed-exact` | 6 of 7 | `/version` typed into the catalog is its search (`Commands 1`), no slash popup. Only the summary text differs. |
| `enter-exact` | **3 of 3** | none in the band: a command typed in full runs as typed. |
| `ctrl-c` | 3 of 13 | fx replaces the key help with `press ctrl+c again to exit`; xfx keeps the key help. See below. |
| `esc-after-ctrlc` | **3 of 3** | none |

## 24x80

`open`: both have a 14-row budget (`inlinePickerRowBudgetCapped`, 24 - 10). fx shows a header, the blank row and 12 of 36 commands; xfx shows all 7. Both headers pack at 80 columns to `… Security  …`. `btab3-product`: both pack to `All  General  Session  Account  Model  Appearance  …  [Product]` -- the active tab after the ellipsis (`help_menu_presentation.zig:171-174`). `query-version`: 6 of 7 band rows identical, the summary text only.

## What does not match, and why

- **The echo card.** xfx writes `┃ /help` (and `┃ /version`) into the document above the band; fx writes nothing for a slash line (`input_submit_runtime.zig:998-1052`). The echo is the slash dispatch's, owned by the notices unit (C0); the `/help` handler itself writes nothing (`help_opens_the_catalog_in_the_band_and_writes_nothing_to_the_document`).
- **The `/version` answer.** `xfx 0.1.0 (debug, revision …)` against `* version: 0.0.13`: the notice format and `/version`'s body are C0's and C3's.
- **`press ctrl+c again to exit`** (`input_presentation.zig:696-703`). xfx has no "Ctrl-C pending" state for any surface to show; the interrupt unit (F4) owns the exit hint. A first Ctrl-C at an empty composer arms xfx's own exit window and leaves the catalog up, as fx's does.
- **The argument picker after `/login `.** A different surface (fx's provider stage picker); `/login` is gated (#88) and the `/provider` picker is D3's.
- **Summaries.** `/clear`, `/setup` and `/model` keep xfx's own text (WU-B's README gives the reasons); `/version` names xfx.
- **The search has no usage string.** fx also matches a command's `help_entry` (`command_specs.zig:457-473`), e.g. `session` finds `/statusline [context|session|workspace]`. xfx's registry has no such field; its summaries carry the argument grammar (`/setup <gateway|llmux>`, `/model <id>`), which is searched.
- **The banner.** `x𝒇x v0.1.0-04d9593 [debug]` is xfx's brand line (WU-A).

Verdict: **the catalog's shape and every key match fx in every snap where the two registries allow a comparison.** Five snaps have identical band rows (`query-none`, `esc-clears-query`, `enter-row`, `enter-exact`, `esc-after-ctrlc`); `query-version` and `typed-exact` (and `query-version` at 24x80) differ in the one summary string. Everything else is fx's own rule applied to xfx's seven commands, or a surface another unit owns.
