---
title: 0.0.16 delta -- P5 T-U3 receipt (inline code is the brightest neutral)
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, transcript, theme]
---

# P5 T-U3: inline code is the brightest neutral

Spec: `research/tui-phase5:.prd/tui-phase5/research/transcript.md` T3 and
§3 row U3. Oracle: fx v0.0.16 (`481104fd`, executable sha256 `8e6cda2e…`),
commit `5e32bce2` "Brighten inline code in assistant replies". Base: `main`
at `d314549e`.

## The rule (fx source)

- Inline code opens `ESC[38;5;255m` in `fx_dark` and `ESC[38;5;235m` in
  `fx_light` (`src/core/shared/theme.zig:102,145@481104fd`); `245` and `247`
  before `5e32bce2`.
- A live theme switch (`applyThemeUpdate`,
  `src/core/app/app_render_runtime.zig:482-512@481104fd`) does two things to
  an answer already in the transcript:
  - its text is rewritten through the builtin token table
    (`retintEntriesForTheme`, `src/ui/transcript/store.zig:3722@481104fd`;
    the table `:3784-3800`), which already maps `255` and `235` to each
    other, so inline code follows both ways;
  - a **code block is not in that rewrite** (the `.assistant_code_block`
    arm is `=> {}`, `store.zig:3758@481104fd`). It is drawn again from its
    source with the new `code_highlight_theme`
    (`src/ui/render_engine/transcript_blocks.zig:2023-2029@481104fd`), which
    `setCommandOutputRenderPolicy(shellStyles())` sets
    (`app_render_runtime.zig:509,526@481104fd`). So a dark comment (`245`,
    `theme.zig:111`) becomes the light comment grey `243` (`theme.zig:154`),
    not the `247` the table maps a `245` to.
- The pacer's in-place retint of a queued inline-code open now reads both
  builtins (`src/ui/assistant/pacer.zig:200-204@481104fd`). xfx has no
  counterpart to change: it queues raw markdown and renders it on release in
  the palette then in force (`docs/parity.md:240`).

## Two leads corrected

- The research names "the `ANSWER`-role shade in `src/tui/theme.rs`" as
  inline code's. It is not: `ANSWER` is the answer under an answered
  question, upstream's `statusline_style` (`245`/`241`), which v0.0.16 still
  writes there (`src/ui/footer/question_ui.zig:480@481104fd`) and
  `5e32bce2` does not touch. It is unchanged here.
- The brief's "`store.zig:3722`'s code-comment recolour": that function
  skips code blocks (above). The recolour comes from the redraw.

## What changed in xfx

- `src/markdown/theme.rs`: `FX_DARK`/`FX_LIGHT` `inline_code_open` = `255`/
  `235`.
- **The mismatch the research asked about.** xfx recolours visible cells by
  the colour they hold, within the kind of row their writer recorded
  (`Palette::document_retint_in`). A code block's rows were recorded as
  plain, so a dark comment (`245`) was taken for a turn's summary and went
  to `247`; a light comment (`243`) was no colour xfx knew and stayed `243`
  on a dark screen. This was true before this unit (the old inline code
  shared `245` with it) and is what fx's capture shows differently. Now:
  - `src/tui/theme.rs`: a row kind `DocumentRow::Code` and a sixteenth role,
    the highlighter's comment grey (`245`/`243`); a code row answers its
    colours as `[COMMENT, COMMAND, NOTICE]` (comment, words, strings and
    numbers); `243` added to the closed `ansi256`/`truecolor` sets.
  - `src/tui/shell.rs`: `present` records a code block's rows as `Code` and
    the blank row under them as plain (`write_structure(rows, kind)`).
- Inline code needs no new role: `255`/`235` are the body grey's numbers,
  so a plain row's recolour carries it both ways.
- **T-U3b: a table keeps the colours it was drawn in.** fx leaves
  `.assistant_table` out of the rewrite (`store.zig:3758@481104fd`, the same
  `=> {}` arm as code blocks) and draws a table again from cells that
  `parseTablePayload` styled when the table was added
  (`src/core/agent/presentation/block_render.zig:458@481104fd`, with the
  theme then in force); `renderTableForTranscript` takes no theme
  (`transcript_blocks.zig:980-984,2016-2018@481104fd`). So a table's inline
  code stays `255` on a light screen and `235` on a dark one, while the
  prose beside it moves (fx's own capture, `p5-inline-code-table`). xfx
  recorded table rows as plain and moved them; now `DocumentRow::Table`,
  which a report leaves alone (`document_retint_in` answers `None`).

## Captures

```
FX_BIN=<fx v0.0.16> TMPDIR=/private/tmp/claude-501 scripts/fxcap/capture.sh fx  <scenario> 40x120 fx/
XFX_BIN=<xfx>       TMPDIR=/private/tmp/claude-501 scripts/fxcap/capture.sh xfx <scenario> 40x120 red/ | xfx/
```

`capture.sh` runs fx with `FX_AUTO_UPGRADE=0` and checks its bytes after the
run (sha256 `8e6cda2e…` unchanged, each run). `red/` is `main`'s binary
(`d314549e`), `xfx/` this branch's, both debug builds.

- `p5-transcript-md`, `p5-transcript-md-light`: the research's scenarios.
  **`p5-transcript-md-light` is fixed**: it set only `FX_THEME=light`, so
  xfx ran dark (its card rail was `255`). It now also sets
  `xfx: env XFX_THEME=light`.
- **`p5-inline-code-retint` (new)**: an answer with inline code, a `sh`
  block with a comment and a `zig` block with a comment, keyword, number
  and string, drawn dark; then the terminal reports light (`CSI ? 997 ; 2
  n`) and dark again (`; 1 n`), sent with `key -H` -- no new harness verb.
  No theme pin, because a pin locks live updates out in both products
  (`src/main.zig:2808-2815@481104fd`). fx answers the report with a fence
  and an OSC 11 query; under `capture.sh` OSC 11 goes unanswered (both
  products fall through to `COLORFGBG` at launch in the `-light` run), so fx
  settles on the report itself (`settleNotificationFallback`,
  `src/ui/terminal/theme_monitor.zig:253@481104fd`).
- **`p5-inline-code-retint-light` (new)**: the same answer drawn light first
  (`COLORFGBG='0;15'`, which both read once OSC 11 goes unanswered), then
  dark and light again: what a light-drawn `243`/`238`/`241`/`235` becomes.
- **`p5-inline-code-table`, `p5-inline-code-table-light` (new, T-U3b)**:
  prose with inline code, then a table whose cells hold inline code and a
  link, drawn dark (light), then the two reports.

## RED (`main`, `d314549e`): `red.txt`

Every pair differs, and only in colour (cell text identical in all):

| pair | differing cells, fx vs xfx |
|---|---|
| `p5-transcript-md` `answered` (and `.full`, `closed`) | 197 inline-code cells: `255` vs `245` |
| `p5-transcript-md-light` `answered` (and `.full`, `closed`) | 197: `235` vs `247` |
| `p5-inline-code-retint` `dark`, `dark-again` | inline code `255` vs `245` |
| `p5-inline-code-retint` `light` | inline code `235` vs `247` (22); comments `243` vs `247` (24) |
| `p5-inline-code-retint-light` `light`, `light-again` | inline code `235` vs `247` |
| `p5-inline-code-retint-light` `dark` | inline code `255` vs `245` (22); comments `245` vs `243` (24) |
| `p5-inline-code-table` (all three), `-table-light` (all three) | on `main`: every inline-code cell (245/247 vs 255/235). On T-U3's head `232762e5` (`red-b/`): `light`, the table's 20 cells `255` vs `235`; `-light` `dark`, `235` vs `255` -- the recoloured table |

## GREEN: `green.txt`

`cells.py` over every pair, row 1 (the banner) left out: identical.

| scenario | snaps identical |
|---|---|
| `p5-transcript-md` | `answered`, `answered.full`, `closed` |
| `p5-transcript-md-light` | `answered`, `answered.full`, `closed` |
| `p5-inline-code-retint` | `dark`, `light`, `dark-again` |
| `p5-inline-code-retint-light` | `light`, `dark`, `light-again` |
| `p5-inline-code-table` | `dark`, `light`, `dark-again` |
| `p5-inline-code-table-light` | `light`, `dark`, `light-again` |

GREEN was taken again at T-U3b's head on all six scenarios (`xfx/`): 18
pairs identical, the same four Ctrl-O pairs not (below).

The two Ctrl-O snaps of each md scenario (`full`, `full-pgup`) are not
identical, for reasons outside this unit: the entry times' zone (T9: fx
`KST`, xfx `UTC`), the session record's tool list and `system prompt` line
(ledger cause `session-record`), and the page offset that follows from it
(`full-transcript-shift`). Inside them, `coderuns.txt` lists every run of a
code span's text with its colour, and fx and xfx agree on all of them (19
and 14 runs, all `255` dark, all `235` light); on `main` they were
`245`/`247`.

## No regression: `regress.txt`

`main`'s binary against this branch's (taken again at T-U3b's head), on every other `p5-transcript-*`
scenario (`anchor`, `anchormid`, `cancel`, `diffctl`, `submit`,
`tallfirst`, `tools`, `tz`, `tzsys`; the md pair is above), every snap
grouped by `cellpairs.py`: 39 pairs, 22 identical. In the other 17 no cell
changed colour with its text unchanged; each differing cell is a clock or a
counter -- Ctrl-O's entry times and network milliseconds (16 Ctrl-O snaps)
and `submit-0.15`'s live token count. No other `p5-*` scenario's answer
holds a code span or a code block (their gateway files have no backtick),
but this unit's own `p5-inline-code-*`.

## Tests

- `tui::event_loop::tests::a_theme_report_recolours_an_answers_code_as_upstream_draws_it_again`:
  an answer with inline code and a `sh` block, light then dark reports;
  reads the foreground each run is written under.
- T-U3b, each with a plain row after the block and a report:
  `a_code_blocks_kind_ends_with_the_block_so_the_summary_after_it_is_a_summary`
  (the turn's summary goes `245`->`247`, not `243`, and back);
  `a_fence_the_turn_left_open_is_a_code_block_and_ends_as_one` (an unclosed
  fence given up at turn end); `an_indented_code_block_is_a_code_block_and_ends_as_one`
  (no fence, language inferred); `a_resumed_answers_code_block_is_recorded_and_recoloured_as_a_live_ones`
  (`Shell::hydrate`); `a_tables_cells_keep_the_colours_they_were_drawn_in_across_a_report`
  (dark-drawn and light-drawn tables); `tui::theme::tests::a_tables_row_keeps_the_colours_it_was_drawn_in`.
- **Mutants** (`mutants.txt`, each restored from a saved copy): A, the
  `queue_kind(DocumentRow::Plain)` after a block deleted -- 5 of the 6
  event-loop tests fail (summary `243`/none where `247` is wanted); B, code
  rows recorded plain -- 5 fail (comment `247` where `243`); C, table rows
  recorded plain (T-U3's head) -- the table test fails.
- `tui::grid::tests::a_code_blocks_comment_is_retinted_as_a_comment_and_a_summary_as_a_summary`.
- `tui::theme::tests::a_code_blocks_row_answers_its_colours_as_the_highlighters`,
  `the_highlighters_quieter_greys_are_the_code_rows_roles_in_both_modes`
  (the roles against `markdown::theme`'s syntax palette, so they cannot
  drift), `an_answers_inline_code_is_retinted_as_the_body_grey`.
- Updated literals, mirroring `5e32bce2`'s own test edits
  (`src/core/agent/assistant_presentation.zig`,
  `src/ui/render.zig:917@481104fd`): 26 inline-code `245`->`255` and one
  `247`->`235` in `src/markdown/tests/{inline,links,code_tables}.rs`.
  The code-highlighter's comment literals stay `245`.
- `tui::blocks` goldens: `FX_GOLDENS` and the sweep hashes were dumped from
  fx `c1db9191`. Only inline code inside a table cell moved, a
  same-length change, so the six `table_*`/`cells_*` goldens carry
  `5e32bce2`'s `255` and the table sweeps compare against `c1db9191`'s
  hashes with that one sequence put back (`as_c1db`). No Zig 0.17 toolchain
  is on this machine to dump them at `481104fd`.

## Ledger and parity

- `docs/parity.md:240` (coloured output row): inline code's shade and how a
  code block's rows are recoloured.
- `scripts/fxcap/parity-ledger.toml`: divergence `inline-code-shade`,
  `replaced-in-next-pin` for the `c1db919` pin only (as `status-notice`):
  against `c1db919` the 12 sweep scenarios with a code span differ in that
  colour. Nothing changes for the `v0.0.16` pin, where those rows now match.
  **T-U3b:** T-U3's entry had `fx_row` only, and a regex speaks for its own
  side (`ledger.py:321-323`), so a colour-only row was never covered on
  xfx's side; it now has `xfx_row` too, and `h1-reset` (no code span) is
  dropped. `ledger.txt`: one sweep of the 12 against `c1db919`, classified
  with the old entry and the new (`--rescore`, same captures): 45 unresolved
  -> 44 `replaced-in-next-pin` + 1 (`links/full`, Ctrl-O, whose rows are
  the existing causes `session-record` and `full-transcript-shift`). All 46
  differing rows holding `code` are covered.

## Not here

- **Truecolor sessions, found and not changed.** The presenter and the
  highlighter spell their greys `38;5;N` at every depth, while a
  `COLORTERM=truecolor` session recolours only its own direct spelling
  (`Palette::shade`, `src/tui/theme.rs:573`; the cross-depth asserts of
  `a_direct_colour_session_matches_and_repaints_in_the_same_notation`,
  `:1138`). So there an answer's inline code and code block, and a tool
  group's highlighted words, keep the colours they were drawn in across a
  switch; fx's builtins are 256-colour at every depth and recolour. It
  predates this unit and reverses a tested decision, so it is recorded in
  `docs/parity.md:240` and left to its own unit.

## fx lines relied on (all `@481104fd`)

| line | text |
|---|---|
| `src/core/shared/theme.zig:102` | `.inline_code_open = "\x1b[38;5;255m",` (fx_dark) |
| `src/core/shared/theme.zig:111` | `.comment_style = "\x1b[38;5;245m",` (fx_dark) |
| `src/core/shared/theme.zig:145` | `.inline_code_open = "\x1b[38;5;235m",` (fx_light) |
| `src/core/shared/theme.zig:154` | `.comment_style = "\x1b[38;5;243m",` (fx_light) |
| `src/core/app/app_render_runtime.zig:482` | `pub fn applyThemeUpdate(...)` |
| `src/core/app/app_render_runtime.zig:499` | `app.shell.retintEntriesForTheme(app.alloc, prior_theme, target)` |
| `src/core/app/app_render_runtime.zig:509` | `app.shell.setCommandOutputRenderPolicy(shellStyles());` |
| `src/core/app/app_render_runtime.zig:526` | `.code_highlight_theme = if (ui_render.is_light) .light else .dark,` |
| `src/ui/transcript/store.zig:3722` | `pub fn retintEntriesForTheme(` |
| `src/ui/transcript/store.zig:3747` | raw entries: `retintThemeBytes(alloc, raw.bytes, tokens)` |
| `src/ui/transcript/store.zig:3753` | assistant text: `retintThemeBytes(alloc, assistant.segments.text.items, tokens)` |
| `src/ui/transcript/store.zig:3758` | `.semantic_notice, .user_turn, .assistant_table, .assistant_code_block, .assistant_thematic_rule => {},` |
| `src/ui/transcript/store.zig:3767` | `rebuildTranscriptCacheFromEntries(&shadow, alloc, "theme retint")` |
| `src/ui/transcript/store.zig:3784,3786,3790` | `dark_to_light_theme_tokens`: `255`->`235`, `245`->`247` |
| `src/ui/transcript/store.zig:3793,3795,3799` | `light_to_dark_theme_tokens`: `235`->`255`, `247`->`245` |
| `src/ui/render_engine/transcript_blocks.zig:980-984` | `pub fn renderTableForTranscript(alloc, table, cols)` (no theme) |
| `src/ui/render_engine/transcript_blocks.zig:2016,2018` | `.assistant_table => ... renderTableForTranscript(alloc, e.table, cols -\| gutter)` |
| `src/ui/render_engine/transcript_blocks.zig:2023,2025,2029` | `.assistant_code_block => ... renderCodeBlockForTranscriptWithTheme(..., styles.code_highlight_theme)` |
| `src/core/agent/presentation/block_render.zig:419` | `pub fn parseTablePayload(alloc, buf, link_id)` |
| `src/core/agent/presentation/block_render.zig:458` | `inline_render.writeInline(alloc, raw_cell, &rendered, false, footnotes, link_id)` (cells styled at parse) |
| `src/core/agent/presentation/inline_render.zig:444` | `out.appendSlice(alloc, ansi.inline_code_open)` |
| `src/ui/assistant/pacer.zig:200-204` | `rethemeInlineCode` reads both builtins' `inline_code_open` |
| `src/ui/footer/question_ui.zig:480` | the answered question's answer in `ui_render.statusline_style` |
| `src/main.zig:2808-2815` | a configured theme pin locks live updates out (`themeInputLocked`, `:2812`) |
| `src/ui/terminal/theme_monitor.zig:253` | `fn settleNotificationFallback` |
| `src/ui/render.zig:917` | test: `"run \x1b[38;5;235mzig build\x1b[39m now\n"` |

## Gates (at T-U3b's code head, one at a time after the idle wait, `TMPDIR=/private/tmp/claude-501/ics`)

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| `cargo clippy --locked --all-targets --features fault-injection -- -D warnings` | ok |
| `cargo test --locked --no-fail-fast` | 4876 passed, 0 failed |
| `cargo test --locked --features fault-injection --no-fail-fast` | 4905 passed, 0 failed |
| `scripts/check-xfx-identity.sh` / `check-no-stubs.sh` / `check-no-secrets.sh` | ok / ok / ok (after staging) |
| `cargo build --release --locked`, and with `--features fault-injection` | ok |
| `scripts/smoke.sh target/release/xfx` | 47 checks, 0 failures |
| `scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx` | 37 scenarios + the oracle, 1111 checks, 0 failures (`27-live-theme-retint` ok) |
| `scripts/fxcap/test-sweep.sh` | 24 tests ok |
