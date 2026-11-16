# What leads a continued row, in xfx and in fx v0.0.16

`scripts/fxcap/roots.py` reads a root across a row's end. Before this unit
it read past spaces, tabs and U+2500-U+259F at the next row's start, and
nothing else. This is every glyph that leads a row which goes on from the row
above it, as found by reading each renderer's wrap, in xfx at the base
(`635e7242`) and in fx at the pin (`481104fd`). Line numbers are those
revisions'.

## Marks roots.py did not read (now `roots.MARKS`)

| Mark | Where | What the row is |
|---|---|---|
| `↳` U+21B3 | xfx `src/tui/mcp_menu.rs:1345` (measured), `:1362-1364` (drawn: two spaces, `↳`, a space), the enum's doc `:1499-1503` | The rest of an approval row the width wrapped (`ConfirmLine::Continued`), a command line or an address among them. xfx's own: fx's approval goes on four columns in, with spaces. 0 hits for `↳`, `\xe2\x86\xb3` or `\u{21b3}` in fx's `src/` at the pin. Train 12's trial: a 118-byte scenario directory, 116 bytes on one such row and 2 on the next (`../train-12/ci-mcp-timing/gutter-check.txt`). |
| `┃↑`: the rail U+2503 and `↑` U+2191 in its space's cell | xfx `src/tui/shell.rs:317` (`RAIL_HIDDEN_ABOVE`), `:2923-2930` (the first row shown, when rows above it are scrolled out; no space after it); fx `src/ui/footer/input_presentation.zig:1021` | The composer's first visible row when its rows above are scrolled out of view. A scroll mark, not a wrap: the head of a line it continues is off the screen, so a frame holds only the tail. The "rewritten whole" test for it is synthetic (a head on the row above, which a real frame never has); what it guards in a real frame is the tail (`--check`'s tail rule). A path reaches the composer when a scenario types one (`@SCENARIOS@`). The mark is the pair: a bare `↑` opens `↑↓` key hints, which are text (round 2). |

## Already read: spaces, tabs, U+2500-U+259F

Every other continuation prefix found is one of these, which roots.py's view
already drops at a row's edge:

- fx: tool rows `│ ` or two spaces (`src/ui/transcript/tool_group_projection.zig:1097-1100`, through `wrapFullToolStatus`, `src/ui/render_engine/transcript_blocks.zig:905-923`); the Ctrl-O secondary lines `│  ` (`src/ui/full_transcript_screen.zig:1296`, painted by `writeSecondaryLinePrefix`, `:5247-5260`); a diff row's `  │ ` and spaces (`transcript_blocks.zig:757-761`); a quote's `│ ` per depth after spaces (`src/ui/render_engine/assistant_wrap.zig:710-714`); spaces for a notice (`transcript_blocks.zig:813-815`, `:1819`), a question's fields (`src/ui/footer/question_ui.zig:483-497`), the help (`src/core/slash_commands/command_specs.zig:1381-1396`), a status label (`src/ui/transcript/shimmer_runtime.zig:76-87`) and a gateway error (`src/core/shared/gateway_error_format.zig:150-165`); the composer's `┃ ` (`input_presentation.zig:1021-1023`) and a waiting prompt's `┋ ` (`:70`).
- xfx: the same rules, ported -- `full_transcript/tool_status.rs:66-75` (`│ ` or spaces), `full_transcript.rs:2643`, `:2673` (a diff's `  │ `), `prose.rs:96` (`│`, a quote), `transcript.rs` `Gutter` (`┃` card rail, two spaces otherwise), `retry.rs:143-147`, `blocks.rs:379-426` and `approval_view.rs:758` (spaces), `shell.rs:312` (`┃`) and `:376` (`┋ `).

## Not continuation marks

- `…` U+2026 and `...` (`retry.rs:152-160`, a label's head or tail cut): a
  cut, not a wrap -- the rest of the text is gone, not on the next row. What
  a cut leaves of a run's own path keeps the run directory's name (`--suffix`).
- `?` for a character too wide for a row of one cell (`tool_status.rs:127`,
  fx's `transcript_blocks.zig:941`): it stands in for the character, at the
  row's end.
- `›`, `❯`, `•`, `●`, `▪`: they lead an item's first row, not a continued one.

## Open residuals: a tail whose head is gone, with no mark

Round 2 (opus Q3). `┃↑` is read for a tail whose head scrolled off; the same
head-gone shape stands with no mark in front of it, and nothing here reads
it -- recorded, not fixed in this unit:

- a leading `...{tail}`: a status label too long for its third row, cut in
  front of its tail (`src/tui/retry.rs:152-160`, `keep_tail`, called from
  `src/tui/shell.rs:11800-11801`; fx's status label wraps the same way,
  `src/ui/transcript/shimmer_runtime.zig:115-124@481104fd`);
- a frame's top row, whose head scrolled off with the rows above it;
- the mid-row `… {tail}` of an approval entry row past 1024 bytes, its last
  256 bytes after the cut (`src/tui/mcp_menu.rs:1587-1595`).

What such a tail keeps of the run directory's name is refused (`--suffix`);
a tail of HOME or the TMPDIR is refused by neither check.
