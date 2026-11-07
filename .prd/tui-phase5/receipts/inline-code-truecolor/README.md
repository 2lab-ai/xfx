---
title: 0.0.16 delta -- P5 T-U3-TC receipt (a truecolor session recolours an answer's code)
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, transcript, theme, truecolor]
---

# P5 T-U3-TC: a truecolor session recolours an answer's code as fx does

Gap found by T-U3 (`../inline-code-shade/README.md`, "Not here"). Oracle:
fx v0.0.16 (`481104fd`, executable sha256 `8e6cda2e…`). Base: PR #67's head
`02b4619b` (T-U3 and T-U3b merged with `origin/main` `10434d28`). Code head:
`9461bf70` -- every xfx capture under `xfx/` carries the banner
`v0.1.0-9461bf7`, every one under `red/` `v0.1.0-02b4619`.

## fx's rule (all `@481104fd`)

- **fx's builtins are 256-colour at every depth.** Every slot of `fx_dark`
  and `fx_light` is a `38;5` literal (`src/core/shared/theme.zig:75-159`):
  inline code `255`/`235` (`:102,145`), the highlighter's words, strings,
  numbers and comments (`:107-115`, `:150-158`). The only `38;2` in them are
  the diff markers (`:92-93,135-136`).
- **fx assumes truecolor unless told otherwise**: a `COLORTERM` claim, else
  anything but Apple Terminal (`src/ui/terminal/theme_protocol.zig:84-93`),
  set once at launch (`src/core/app/app_lifecycle.zig:899-903`;
  `src/ui/render.zig:48` defaults it on). It reaches a selected theme's diff
  markers (`render.zig:92-104`), the Ultra mark (`:395`) and the hex colours
  of a named theme file (`theme.zig:10-11`, `ParseOptions`, `:408`) -- never
  a builtin's greys: `applyTheme` copies them verbatim (`render.zig:69-90`).
- **A switch rewrites bytes, not depths.** `applyThemeUpdate`
  (`src/core/app/app_render_runtime.zig:482-512`) rewrites an answer's text
  and the theme-owned raw rows through the builtin token table
  (`src/ui/transcript/store.zig:3743-3757`, table `:3784-3800`: `38;5`
  only, `255`<->`235`, `252`<->`238`, `250`<->`241`, `245`<->`247`), draws a
  code block again with `code_highlight_theme`, which is `.light`/`.dark`
  and nothing about depth (`app_render_runtime.zig:526`,
  `src/ui/render_engine/transcript_blocks.zig:2023-2029`), and leaves a table
  as it was styled when it was added (`store.zig:3758`,
  `transcript_blocks.zig:2016-2018`).
- So fx's screen under `COLORTERM=truecolor` is its 256-colour screen, byte
  for byte: its twelve snaps of the four `-tc` scenarios against T-U3's
  `fx/` captures of the same four without `COLORTERM`: `cells.py` `0 rows
  differ` each, and all twelve `.ansi` files identical (`fx-tc-vs-256.txt`).

## The gap and the tested rule it touched

xfx spells **its own** greys `38;2;L;L;L` when `COLORTERM` claims truecolor
(`theme::depth_from_env`, `src/tui/theme.rs:843`; `truecolor`, `:691`) --
the card's and the composer's rail, a turn's summary, the hint row, its
notices, a tool group's statusline grey. The presenter and the highlighter
do not: an answer's inline code, a code block's greys and a command's words
in a tool group are `markdown::theme`'s `38;5` at every depth, as fx's are
(`src/markdown/theme.rs:49,54-60,69,74-80`). A theme report matched a cell
only in the notation of the session's depth, so in a truecolor session those
`38;5` cells were nobody's and kept the colours they were drawn in.

`a_direct_colour_session_matches_and_repaints_in_the_same_notation` pinned
that: `light.document_retint(Color::Indexed(255)) == None` at
`Depth::TrueColor` ("an indexed cell is not one this session painted").
Its premise was false -- the presenter paints that cell, in this session --
and it pinned the current behaviour, not fx's. Its other half is compatible
with fx and stays: a direct grey is matched and repainted in the direct
notation, and a direct grey in a **256-colour** session is nobody's.

## What changed in xfx

- `src/tui/theme.rs`: `Palette::painter_of` -- an indexed cell in a
  truecolor session is answered as a 256-colour session answers it, and
  repainted in the notation it holds (`38;5`); everything else as before.
  Both `document_retint` and `document_retint_in` go through it, so the two
  cannot disagree. A 256-colour session takes the old path unchanged.
- Tables keep their drawn colours at either depth (`DocumentRow::Table`
  answers nothing before any depth is asked).
- **Also covered, by the same rule, without a capture**: a tool group's
  highlighted words (`252`/`238`) and strings (`250`/`241`) on its
  `ToolGroup`/`ToolStopped` rows, which T-U3 named in the same note. fx
  rewrites a tool row's bytes through the same table (`store.zig:3745-3750`,
  `themeOwnsRawEntry` `.tool_status`, `:3802-3813`). Held by
  `a_truecolor_sessions_code_and_tool_rows_recolour_their_indexed_greys_as_fx`;
  no `p5-*` scenario reports a theme change over a tool group.

## Captures

```
FX_BIN=<fx v0.0.16> TMPDIR=/private/tmp/claude-501 scripts/fxcap/capture.sh fx  <scenario> 40x120 fx/
XFX_BIN=<xfx>       TMPDIR=/private/tmp/claude-501 scripts/fxcap/capture.sh xfx <scenario> 40x120 red/ | xfx/
```

`tmux` keeps a `38;2` through `capture-pane -e` (3.7b: a pane that printed
`ESC[38;2;38;38;38mAB` captures as exactly that), so a truecolor session can
be captured; `capture.sh` passes a scenario's `env COLORTERM=truecolor` to
both products. `capture.sh` ran fx with `FX_AUTO_UPGRADE=0` and its guard
reported sha256 `8e6cda2e…` unchanged after each run. `red/` is `02b4619b`'s
debug build, `xfx/` `9461bf70`'s, each built from a clean committed tree.

New scenarios, each T-U3's with `env COLORTERM=truecolor` added and nothing
else: `p5-inline-code-retint-tc`, `p5-inline-code-retint-light-tc`,
`p5-inline-code-table-tc`, `p5-inline-code-table-light-tc`.

**How a pair is read (`notation.py`).** A strict `cells.py` reading of a
truecolor pair differs on every cell xfx paints in its own grey: fx writes
`38;5;N`, xfx `38;2;L;L;L`, the same colour on xterm's ramp
(`L = 8 + 10 (N - 232)`). That spelling is a decision of xfx's, outside this
unit (below), so each pair is read three ways and all three are printed:

- `ramp`: rows that differ once each direct grey is read as the index it
  spells -- the colours compared. GREEN needs `0`.
- `colour`: the cells behind those rows, grouped.
- `notation`: the cells that differ in spelling alone, grouped.
- `kept`: cells (same row, column, character) whose foreground is spelt in
  another notation than in the run's first snap, taken before any report.
  A report that matched `38;5` and repainted in `38;2` would pass `ramp` and
  fail here. GREEN needs `0`.

## RED (`02b4619b`): `red.txt`

| pair | differing cells after the ramp reading, fx vs xfx |
|---|---|
| `p5-inline-code-retint-tc` `light` | inline code `235` vs `255` (22); keywords `238` vs `252` (13); strings and numbers `241` vs `250` (15); comments `243` vs `245` (24) |
| `p5-inline-code-retint-light-tc` `dark` | inline code `255` vs `235` (22); keywords `252` vs `238` (13); strings and numbers `250` vs `241` (15); comments `245` vs `243` (24) |
| `p5-inline-code-table-tc` `light` | the prose's inline code `235` vs `255` (18) -- the table's own cells match |
| `p5-inline-code-table-light-tc` `dark` | the prose's inline code `255` vs `235` (18) |

The first snap of each run (before any report) and the last (back where it
started) are `ramp: 0`: the gap is the switch. `kept: 0` throughout: `02b4619b`
never repainted those cells at all.

## GREEN (`9461bf70`): `green.txt`

All 12 pairs: `ramp: 0 rows differ`, `kept: 0`, and the same 23 cells
differing in notation alone (the rows `┃ show me the …` (the card's rail),
the turn's summary, the composer's `┃` and the hint row: fx `38;5;N`, xfx
`38;2` of the same ramp grey).

| scenario | snaps, `ramp: 0` and `kept: 0` |
|---|---|
| `p5-inline-code-retint-tc` | `dark`, `light`, `dark-again` |
| `p5-inline-code-retint-light-tc` | `light`, `dark`, `light-again` |
| `p5-inline-code-table-tc` | `dark`, `light`, `dark-again` |
| `p5-inline-code-table-light-tc` | `light`, `dark`, `light-again` |

The first snap of each run is identical between `red/` and `xfx/`
(`cellpairs.py`, no differing cell in any of the four): what a truecolor
session draws is unchanged, and only the switch moved.

## Divergence: the notation of xfx's own greys

In every pair, before and after the fix, the cells xfx paints in its own
palette differ from fx's in spelling and in nothing else (`notation:` lines;
`kept: 0`). Recorded in `docs/parity.md:241` and in the ledger
(`scripts/fxcap/parity-ledger.toml`, `truecolor-grey-notation`,
`user-decision`, the v0.0.16 pin, these four scenarios, the rows that hold
those greys). Keeping it or spelling xfx's greys `38;5` as fx does is the
user's call; this unit does neither.

The entry selects rows by their normalized text -- the rails (`^┃`), the
summary (`^  <wall> \(`, after `turn-wall-time`) and the hint row
(`^auto · model-a$`) -- so it cannot cover an answer's row. `ledger.txt`,
one sweep of the four scenarios each way: at the code head 12 of 12 pairs
`approved-divergence`, every differing row matched by this entry and no
other; at `02b4619b` the four switched snaps stay `unresolved-defect`, the
rows no entry covers being exactly the answer's inline code and code rows
(`Run zig build test …`, `# build and test`, `const n = 42;`, …), and the
other eight `approved-divergence`. T-U3's entry `inline-code-shade` cited
`docs/parity.md:240`, which a merge from `main` had moved to `:241`;
corrected.

## No regression at 256 colours

A 256-colour session takes the old path by construction (`painter_of`
changes nothing at `Depth::Ansi256`; mutant C is the guard). Shown on
captures two ways:

- **Against fx, T-U3's six scenarios** (`x256.txt`; this unit's code head,
  against T-U3's `fx/`): the 18 pairs T-U3 called identical are identical
  (`p5-inline-code-retint`, `-retint-light`, `-table`, `-table-light`, three
  snaps each; `p5-transcript-md`, `-md-light`: `answered`,
  `answered.full`, `closed`); the four Ctrl-O pairs differ as T-U3 recorded
  (31 and 32 rows: entry times, the session record).
- **Base against head, every other `p5-transcript-*` scenario**
  (`regress.txt`; `anchor`, `anchormid`, `cancel`, `diffctl`, `submit`,
  `tallfirst`, `tools`, `tz`, `tzsys`, every snap, `cellpairs.py`): 39 pairs,
  22 identical. In the other 17 no cell changed colour with its text the
  same (no `same-text` group); every differing row is the same row with
  other digits -- checked by masking each run of digits and comparing the
  rows' text and each printed cell's attributes again -- Ctrl-O's entry
  times and network milliseconds (16 snaps) and `cancel`'s `mid` live token
  count. The same 39-pair, 22-identical shape as T-U3's `regress.txt`.

## Tests

- `tui::theme::tests::a_direct_colour_session_matches_and_repaints_in_the_same_notation`
  (rewritten): at `Depth::TrueColor` an indexed `255` goes to `38;5;235` on
  light and `235` to `38;5;255` on dark, a direct grey still to the direct
  one; at `Depth::Ansi256` a direct grey is still `None`, on a plain row and
  on a code row.
- `tui::theme::tests::a_truecolor_sessions_code_and_tool_rows_recolour_their_indexed_greys_as_fx`
  (new): code rows (`245`->`243`, `252`->`238`, `250`->`241` and back), tool
  group and stopped-call rows (`252`<->`238`), the statusline grey still
  direct on a tool row, a table row `None`, a link `None`.
- `tui::event_loop::tests::a_truecolor_sessions_answer_code_recolours_in_the_notation_it_was_drawn_in`
  (new): a `Depth::TrueColor` shell, an answer with inline code and a `sh`
  block, the turn ended; light then dark reports. Reads each run's
  foreground **as spelt** (`fg_spelt`): inline code `38;5;255`->`38;5;235`->
  `38;5;255`, the comment `38;5;245`->`38;5;243`->`38;5;245`, the summary
  `38;2;138;138;138`->`38;2;158;158;158`->back.
- **RED at unit level** = mutant A below (the base's behaviour).
- **Mutants** (`mutants.txt`; each applied to a saved copy of
  `src/tui/theme.rs`, which was restored after, sha256 checked):
  - A, `painter_of` returns the palette unchanged (= `02b4619b`'s rule):
    the three tests above fail (`None` where `Some("\x1b[38;5;235m")`;
    `Code 245` `None`; the event-loop frame never rewrites `zig build`).
  - B, matched in `38;5` but repainted in the session's `38;2`: the same
    three fail (`Some("\x1b[38;2;38;38;38m")` where `38;5;235`).
  - C, also reading a direct grey in a 256-colour session as truecolor:
    the rewritten test fails (`Some("\x1b[38;2;38;38;38m")` where `None`) --
    the guard that keeps 256-colour sessions as T-U3 left them.

## T-U3's provenance, corrected

T-U3b's GREEN captures (`../inline-code-shade/xfx/`) carried the banner
`v0.1.0-232762e`: built from T-U3b's working tree before it was committed as
`50b44d11`. Its `red-b/` carried `v0.1.0-d314549`: T-U3's code built before
its commit `232762e5`. Corrected here:

- `../inline-code-shade/xfx/` is **re-taken from PR #67's head `02b4619b`**
  (`red/`'s binary, a clean committed tree; banner `v0.1.0-02b4619` on each
  of the 16 snaps whose row 1 is the banner -- the md pair's `answered`,
  `closed` and `full` snaps have scrolled it away), the same six scenarios,
  and `../inline-code-shade/green.txt` and `coderuns.txt` are re-run over
  it: the 18 pairs T-U3 called identical are identical, the four Ctrl-O
  pairs differ as T-U3 said (entry times, the session record), and every
  code span's run is `255` dark and `235` light on both sides. The first
  take of the md pair read `2s` on the turn's summary where fx read `1s` (a
  wall clock, under load); taken again, identical.
- `../inline-code-shade/README.md` ("Captures") now says what every one of
  its capture directories was built from, `red-b.txt`'s header too, and its
  truecolor note points here.
- The same six scenarios from this unit's code head (`9461bf70`, banner
  `v0.1.0-9461bf7`) against fx: `x256.txt`, below.

## Gates

At the code head `9461bf70`, one at a time, the test suites each after the
idle wait (`until ! pgrep -x cargo … && ! pgrep -x 'xfx-[0-9a-f]+' …`),
`TMPDIR=/private/tmp/claude-501/ict`:

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| `cargo clippy --locked --all-targets --features fault-injection -- -D warnings` | ok |
| `cargo test --locked --no-fail-fast` | 4892 passed, 0 failed |
| `cargo test --locked --features fault-injection --no-fail-fast` | 4921 passed, 0 failed |
| `scripts/check-xfx-identity.sh` / `check-no-stubs.sh` / `check-no-secrets.sh` | ok / ok / ok (9730 tracked files, this receipt staged) |
| `scripts/fxcap/test-sweep.sh` (the ledger changed) | 24 tests ok |

The receipt commit on top of `9461bf70` changes no compiled file (`.prd/`,
`docs/parity.md`, `scripts/fxcap/parity-ledger.toml` only), so a binary
built at it differs from `xfx/`'s in its banner alone.
