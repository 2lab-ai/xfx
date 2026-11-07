# WIDTH-DISPUTED receipt: a soft hyphen or a Hangul filler takes the cells fx counts, alone or in a cluster, and never fewer than the width table

T-U6c found this defect. xfx's painter counted U+00AD, U+1160, U+3164,
U+FFA0 and U+FFF0-FFF8 as no cells, while tmux 3.7b draws the soft hyphen,
U+FFA0 and U+FFF0-FFF8 a cell each. So a row clipped at the margin ran past
it: on Ctrl-O, `p5-transcript-diffwide`'s line ended `…TAI8` where fx's ends
`…TAIL`.

- **Round 1** (`2f328d47`) fixed each code point standing alone.
- **Round 2** (`41a31300`) fixes it for the clusters these code points
  stand in:
  - a filler followed by a mark, a selector or a joiner (U+FFA0 U+0301);
  - conjoining jamo with a filler (U+115F U+1160).

  It answers MUST 1 and MUST 2 of the round-1 reviews
  (`…/p5/WIDTH-DISPUTED/reviews/`).
- **Round 3** (`db3b54a1`) fixes round 2's regression, the MUST of the
  round-2 reviews. Round 2 counted a cluster that holds one of the 14 by the
  port alone, and the port counts a skin-tone modifier (U+1F3FB-1F3FF) as no
  cells. So U+FFA0, U+1160 or U+115F with a modifier after it counted fewer
  cells than the width table does, and than round 1 did. Now each code point
  of such a cluster counts no less than the width table counts it.

Width decides what is clipped at the margin, so this is security-adjacent.

- **Branch:** `fix/p5-width-disputed`, cut in the same worktree
  (`.worktrees/feat-p5-transcript-receipts`) from `feat/p5-transcript-receipts`
  at `8837634a`.
- **The code:** `db3b54a1` is the fix. `cb9b45e2` changes one doc comment
  and nothing else: `git diff db3b54a1 cb9b45e2 -- src/` is
  `src/tui/wrap.rs`, 5 lines in and 3 out, every one a `///` line.
  - Every `xfx/` capture is from a debug build of `cb9b45e2` (banner
    `v0.1.0-cb9b45e`).
  - The regression set (below) is from a debug build of `db3b54a1`
    (`v0.1.0-db3b54a`).
  - The commits after `cb9b45e2` change the ledger and this receipt only.
- **`red/`, the build before:**
  - `p5-transcript-diffwide`: a build of `20046d63`, whose code is
    `8837634a`'s (`v0.1.0-20046d6`);
  - `p5-transcript-diffwide2`, the cluster form: a build of round 2's
    `41a31300` (`v0.1.0-41a3130`), the build round 3's MUST is about.
    Round 2's `red/` for it, round 1's build, is in `e349f7bc`.
- **fx:** the v0.0.16 reference binary (sha256 `8e6cda2e…`). The guard line
  of each run in rounds 1 and 2 says it was unchanged; after round 3's
  captures its sha256 was checked again, `8e6cda2e…`.

## The measures

**fx at 481104fd** counts a line one code point at a time:
- `displayUnitAt`, `src/core/shared/display_width.zig:234-263`, takes RGI
  emoji and `base U+FE0E`/`U+FE0F` whole for a variation base and every
  other code point alone;
- `runeWidth` (`:275-282`) gives 0 for NUL, C0 and C1;
- 0 for a combining mark or a joiner (`isZeroWidthContinuation`, `:409-421`;
  that covers U+0301, U+034F, U+FE0E and U+200D, and the skin-tone
  modifiers U+1F3FB-1F3FF wherever they stand, `emoji_modifier_ranges`,
  `unicode_display_data.zig:250-252`), but none of the 14 is one;
- 2 for East Asian Wide or emoji presentation (`wide_ranges`,
  `unicode_display_data.zig:41` `0x1100-0x115F` and `:85` `0x3131-0x318E`);
- 1 for everything else.

So the 14 are: U+00AD 1, U+115F 2, U+1160 1, U+3164 2, U+FFA0 1, and
U+FFF0-FFF8 1 each. In a cluster each keeps its own count, and the mark after
it counts 0. The same lines hold at c1db9191.

`markdown::display_width` (`src/markdown/display_width.rs`, `rune_width`
`:306`, `display_unit_at` `:235`) is that code ported, and it is the oracle
here.

**The terminals, fx's count and xfx's.** There are 164 forms, each between
two markers (`clusters.txt`; probes in `probe/`, the cluster versions of
`scripts/drawn-wider/`'s). xfx's column is the painter at `db3b54a1`
(`probe/painter_probe.rs`).
- Round 2's 76 (`probe/seqs.txt`):
  - each of the 14 alone, and with U+0301, U+FE0E, U+034F or U+200D after
    it;
  - `U+115F U+1160`, `U+1100 U+1160`, `U+115F U+1161` and
    `U+1100 U+1160 U+11A8`;
  - `U+3164 U+3164` and `U+00AD U+00AD`.
- Round 3's 88 (`probe/seqs3.txt`):
  - each of the 14 with each skin-tone modifier, and with U+FE0F, after
    it (84);
  - and `a` U+1F3FD, U+1F3FD alone, `a` U+FE0F and U+2764 U+FE0F.

In round 2's 76, fx's count is below a terminal's advance in none. In
round 3's 88, it is below in 79: every modifier form but U+3164's, and every
U+FE0F form after a soft hyphen, U+FFA0, U+FFF0-FFF8 or `a`. **xfx's count
is below a terminal's in 12, all U+FE0F forms** (below). Some of them:

| | tmux 3.7b | libghostty-vt 1.3.2 | fx's count | xfx's |
|---|---|---|---|---|
| U+00AD, alone or with a mark | 1 | 1 | 1 | 1 |
| U+115F, alone or with a mark | 2 | 0 | 2 | 2 |
| U+1160, alone or with a mark | 0 | 0 | 1 | 1 |
| U+3164, alone or with a mark | 0 | 0 | 2 | 2 |
| U+FFA0, U+FFF0-FFF8, alone or with a mark | 1 | 0 | 1 | 1 |
| U+115F U+1160 | 2 | 0 | 3 | 3 |
| U+1100 U+1160 U+11A8 | 2 | 2 | 4 | 4 |
| U+FFA0 or U+FFF0-FFF8, then a modifier | 3 | 2 | 1 | 3 |
| U+1160, then a modifier | 2 | 2 | 1 | 3 |
| U+115F, then a modifier | 4 | 2 | 2 | 4 |
| U+3164, then a modifier | 2 | 2 | 2 | 4 |
| U+00AD, then a modifier | 3 | 3 | 1 | 3 |
| `a` U+1F3FD | 3 | 3 | 1 | 3 |
| U+1F3FD alone | 2 | 2 | 0 | 2 |
| U+FFA0 or U+FFF0-FFF8, then U+FE0F | 2 | 0 | 1 | 1 |
| U+00AD or `a`, then U+FE0F | 2 | 1 | 1 | 1 |

**xfx's measures, and where each is used:**
- `wrap::width`, through `cluster_cells` and `cluster_width`
  (`src/tui/wrap.rs`), is **the painter's one measure**. It is used by:
  - `frame::clip` and the frame's grid;
  - the composer (`wrap::composer`, `composer_cells`);
  - the transcript's wrap;
  - Ctrl-O (`railed`, `literal`, `diff_rows` through `clip`,
    `tool_status`);
  - the approval band and plane (`Review::rows` → `safe_rows` →
    `wrap::wrap`, and `header`).

  The fix is in `cluster_cells`, so every one of these agrees.
- `approval_view`'s direct `unicode_width` reads are unchanged:
  - `uncounted` (`:1074`) decides what the review names, and the review
    still names every default-ignorable code point
    (`every_default_ignorable_code_point_in_text_is_named_and_an_agreed_emoji_kept`);
  - `squeezed` (`:1095`) asks whether the painter counts a cluster as fewer
    cells than its parts, and a count that grew cannot newly trip it.

## What changed

**Round 1, `2f328d47`:**
- the 14 alone took fx's count;
- `the_soft_hyphen_and_the_fillers_take_the_cells_fx_counts`;
- `src/tui/drawn_wider.rs` regenerated by `scripts/drawn-wider/sweep.sh`:
  3,892 → 3,881 scalars, exactly U+00AD, U+FFA0 and U+FFF0-FFF8
  (`sweep.txt`).

**Round 2, `41a31300`:**
- **`wrap::cluster_cells`:** any cluster that holds one of the 14 is counted
  by the port, `markdown::display_width::visible_width`, before the U+FE0E
  arm. `counted_as_upstream` is the predicate and the port is the one
  count. Every other cluster is unchanged: emoji, an ordinary Hangul
  syllable, and so on.
  - Where clusters form: U+3164, U+FFA0, U+115F and U+1160 join a mark, a
    joiner or the jamo after them.
  - The soft hyphen and U+FFF0-FFF8 are clusters of their own
    (Grapheme_Cluster_Break Control).
- **`U+115F U+1160` counts 3, fx's count**, not 2. It is not kept at 2 as a
  divergence: under the rule "a cluster that holds one of the 14 counts as
  fx counts it" it is fx's, and 3 is no fewer than either terminal draws.
  The ordinary decomposed Hangul of `approval_view.rs:1072-1073`
  (`U+1100 U+1161`, no filler) is not affected.
- **The test**
  `a_cluster_that_holds_a_soft_hyphen_or_a_filler_takes_the_cells_fx_counts`:
  - every one of the 14, alone and × {U+0301, U+FE0E, U+034F, U+200D}, plus
    the four jamo clusters;
  - each checked alone and between two letters, against the port;
  - and never under the terminal advances measured for ten of them.
- **The doc comments** on `cluster_cells` and `counted_as_upstream`:
  - the cluster form;
  - "never fewer cells than either draws" holds **in the measured forms**;
  - the over-count (SHOULD a, below).
- **The `drawn_wider` table is unchanged.** It measures each scalar alone,
  and round 2 changes no scalar's count alone.

**Round 3, `db3b54a1`:**
- **`wrap::cluster_cells`:** a cluster that holds one of the 14 counts
  `upstream_cells(cluster).max(cluster.width())`. `upstream_cells` sums,
  code point by code point, the port's count, each no less than the width
  table's count of that code point.
  - **Not the brief's `visible_width(cluster).max(cluster.width())`.** For
    U+FFA0 U+1F3FD that is max(1, 2) = 2, and tmux draws 3. Code point by
    code point it is 1 + 2 = 3.
  - The cost: U+1160 U+1F3FD counts 3 where both terminals draw 2, and
    U+3164 U+1F3FD 4 where both draw 2. That is the over-count of U+1160 and
    U+3164 that round 2 kept, below.
  - On round 2's 74 tested forms the count is still the port's: round 2's
    test,
    `a_cluster_that_holds_a_soft_hyphen_or_a_filler_takes_the_cells_fx_counts`,
    holds unchanged (the gates' test runs).
- **The test**
  `a_filler_cluster_never_counts_fewer_cells_than_the_width_table_or_a_terminal_draws`:
  - U+00AD, U+115F, U+1160, U+3164, U+FFA0, U+FFF0 and U+FFF8, each with
    U+0301, U+FE0E, U+FE0F, U+034F, U+200D and U+1F3FB-1F3FF after it: the
    painter is never below the width table (`unicode-width`);
  - and never below what either terminal was measured to draw for nine of
    them (`clusters.txt`).
- **Not closed: U+FE0F after a soft hyphen, U+FFA0, U+FFF0-FFF8 or a
  letter.** tmux draws one cell more than xfx counts, in 12 forms. xfx's
  count is fx's in all 12, and libghostty-vt draws no more than it.
  - It is not fixed here. A rule for filler clusters only would reach just
    U+FFA0 U+FE0F: the soft hyphen and U+FFF0-FFF8 are clusters of their
    own, so the U+FE0F after them is a cluster alone. The rule that closes
    all 12 gives U+FE0F a cell after a letter too, which parts from fx
    (`a` U+FE0F counts 1 there, and libghostty-vt draws 1).
  - It is `drawn_wider`'s U+FE0F, which CTRLO-WIDER owns. The ledger's
    `ctrl-o-drawn-wider-raw` says so in its `why`. Its selector leaves
    U+FE0F out, because a U+FE0F selector would take in every row with an
    emoji in emoji presentation.
- **The doc comments** on `cluster_cells`, `upstream_cells` and
  `counted_as_upstream` (`cb9b45e2` adds only the `green.txt:18` and
  `green.txt:21` citations below).

### The over-count (SHOULD a)

fx's count is often *more* than a terminal draws:
- U+1160 counts 1, and U+3164 2, where both terminals draw none;
- U+115F, U+FFA0 and U+FFF0-FFF8 count 1 or 2 where libghostty-vt draws
  none;
- a jamo cluster with a filler counts 1 or 2 more than both.

A row holding one is laid out as if it were that much wider. Two costs are
captured, in `p5-transcript-diffwide2` `full`:
- line 2 (`green.txt:18`): after the U+3164 and U+1160, xfx's text stands
  three cells left of where its grid holds it. fx's frame re-anchors there
  and shows three blank cells.
- line 5 (`green.txt:21`): four U+1160 U+1F3FD, counted 3 cells each where
  tmux draws 2. xfx's Ctrl-O row is cut after two of them, at 108 of the
  row's 112 cells, and its read row wraps after two.

The rest is inferred from the code [추론]:
- the frame can skip the `EL` of a row it thinks is full (`frame::place`,
  `frame.rs:2715-2722`) and leave a stale cell at its right edge;
- a partial row update (`grid.rs:683-697`) can leave cells the terminal
  never moved over;
- the composer's caret (`wrap::composer_cells`) sits that many cells right
  of the text before it.

None of these puts a byte past the margin. This is fx's own count, kept as
fx keeps it.

## RED

- **Round 3's test** (`red-test-3.txt`), on `e349f7bc`'s code (round 2's
  product code, `41a31300`) with only the test added. 15 forms count under
  the width table, as (cluster, painter, width table): U+FFA0 and U+1160
  with each modifier, `("FFA0 1F3FD", 1, 2)`, `("1160 1F3FD", 1, 2)`; U+115F
  with each, `("115F 1F3FD", 2, 4)`.
- **The captures** (`red.txt`, `check.py … red`): 11 claims, 6 hold, 5 fail.
  - **`p5-transcript-diffwide2`** on round 2's build, line 5 (`104 w` +
    4×(U+1160 U+1F3FD) + `TAIL5678`): the Ctrl-O row is fx's, `…🏽🏽🏽 L`
    in the screen's last cell, the fourth modifier and `TAI` lost. That is
    the MUST's under-count, on tmux. Its read rows lose the same, so `bytes` and
    `whole` fail.
  - **`p5-transcript-diffwide`** on the base: line 3 ends `…TAI8`, so
    `bytes`, `margin` and `full` fail.
- **Round 2's test** (`red-test-2.txt`), on `d3584529`'s code (round 1's
  product code) with only the test added. 16 forms are wrong, as (cluster,
  painter, fx):
  `("1160 0301", 0, 1)`, `("3164 0301", 0, 2)`, `("FFA0 0301", 0, 1)`,
  `("FFA0 FE0E", 0, 1)`, `("FFA0 200D", 0, 1)`, `("115F 1160", 2, 3)`,
  `("1100 1160 11A8", 2, 4)`, and more.
- **Round 1's test** (`red-test.txt`), on `8837634a`'s code: 13 of the 14
  alone counted 0.
- **Round 2's captures** (`red.txt` in `e349f7bc`): 10 claims, 4 hold,
  6 fail.
  - **`p5-transcript-diffwide2`** (the cluster form) on round 1's build:
    - line 3, `104 w` + 4×(U+FFA0 U+0301) + `TAIL5678`, ends **`…TAI8`**
      (fx `…TAIL`). That is the MUST 1 reproduction on tmux;
    - line 4, `104 w` + 2×(U+115F U+1160) + `TAIL5678`, ends `…TAIL`
      (fx `…TA`, because fx counts the two jamo clusters 6 cells).
  - **`p5-transcript-diffwide`** on the base: line 3 ends `…TAI8`.

## GREEN (`green.txt`: 11 claims, 11 hold)

**Round 3's test** passes on `cb9b45e2`, and round 2's 74-form test with
it (the gates' two test runs, below).

The `bytes` claim compares the diff rows **cell by cell**, text and
attributes. The rows' raw `capture-pane -e` bytes are the same except where a
row's first SGR follows a row that only one side has.

**`p5-transcript-diffwide`:**
- every Ctrl-O diff row is the same cells, and line 3 ends
  `\xad\xad\xad\xadTAIL`;
- aligned, 40 rows are the same. Only fx shows the continuation
  `  │     5678`;
- `edited` and `closed` are identical.

**`p5-transcript-diffwide2`:**
- **margin:**
  - line 3 ends `\uffa0\u0301`×4`TAIL` on both sides (`…TAI8` on round
    1's build);
  - line 4 ends `\u115f\u1160`×2`TA` on both sides (`…TAIL` on round 1's
    build). The row is cut where fx's count, not the terminal's, reaches
    112.
- **bytes:** rows 1-, 1+, 3, 4 and 6 are the same cells. Row 6 is `mod`
  U+FFA0 U+1F3FD `end`, a short line with the modifier form.
- **row 5 stands apart** (the ledger's divergence `filler-modifier-count`,
  below): fx's row ends `…🏽🏽🏽 L` in the screen's last cell, past the
  margin by the terminal's count; xfx's ends `…🏽🏽`, two of the four
  pairs, inside it.
- **whole:** xfx's read rows of line 5, joined, are the line as the capture
  keeps it, `…🏽🏽🏽🏽TAIL5678`. The capture keeps no cell for U+1160, on
  either side. fx's lose `🏽TAI`.
- **row 2 is named.** It is the short line with every cluster form. fx's
  cells are xfx's with three unstyled blank cells before `x zwj`:
  - fx re-anchors the cursor (`ESC[42G`) after `half` U+FFA0 U+FE0E,
    because the cluster holds a variation selector;
  - its count of the U+3164 and U+1160 before it is three cells more than
    tmux draws;
  - xfx's row runs on.

  This is fx's frame (`95e6c8fd`), not the count: the cause
  `frame-variation-selector-anchor`, below.
- **full, aligned:** 33 rows are the same.
  - Only fx shows the continuations `  │     5678` (lines 3 and 5) and
    `  │     IL5678` (`ctrl-o-diff-row-clipped`);
  - the two re-anchored rows, the diff row and the read row of line 2, are
    named;
  - line 5's rows on each side are those `filler-modifier-count` covers;
  - only xfx shows the two top rows that fx's later window pushes out.
- `edited` and `closed` are identical.

## Regressions (`regressions.txt`, `regressions-r2-r3.txt`, `regressions-r1-r2.txt`, `timing.txt`)

All 31 `p5-transcript-*`, `p5-input-*` and `composer-*` scenarios at 40x120,
plus `p5-transcript-tools` at 40x80, were captured on round 3's build
(`db3b54a1`). Against the base (`regressions.txt`): **165 snaps, 162
identical, 3 differ**.

- **`p5-transcript-diffwide` `full`:** this receipt.
  - Row 30 goes from `TAI8` to `TAIL`.
  - Rows 1-22: the read row `│  3 …` now wraps to `│  5678` as fx's does,
    so the window starts a row later.
- **`p5-transcript-records` `start`, row 5**, the launch notice: a timing
  row, below.
- **`p5-transcript-submit` `submit-0.15`, row 36:** base `↓40`, round 3
  `↓4`. A timing row, below.

Against round 2's build (`regressions-r2-r3.txt`): 163 identical, 2 differ,
the same two timing rows. Round 1's against round 2's
(`regressions-r1-r2.txt`): 162 identical, 3 differ, all timing. **Neither
round 2 nor round 3 changes a snap of these scenarios but timing.** None of
their 165 snaps holds a skin-tone modifier, or a U+FE0F after one of the 14.

**The timing rows** (`timing.txt`, every run, read from the captures).
No row's text holds a disputed code point:

- **`p5-transcript-submit` `submit-0.15`, row 36** (SHOULD 5). The row is
  the phase and the turn's token estimate 0.15 s after Enter; `↓` is what
  of the answer has streamed by then (`Tally::output`,
  `src/tui/shell.rs:1961-1965`). So `↓0`, `↓2`, `↓4` or `↓40` is how far
  the stream had got, a stream-state difference:
  - base: `↓40` 3 times in 4 runs, `↓2` once;
  - round 1: `↓40` once in 1 run;
  - round 2: `Thinking ↓0` once in 4 runs, `↓40` 3 times;
  - round 3: `↓4` once in 4 runs, `↓40` 3 times.
- **`p5-transcript-submit-multi` `submit-0.15`, row 36.** This is restated
  from round 1, where the README said "each build showed both". The data
  did not show that for round 1's build.
  - base: `Thinking` once in 7 runs, `Generating` 6 times;
  - round 1's fix: `Generating` 3 times out of 3;
  - round 2's fix: `Thinking` once in 5 runs, `Generating` 4 times;
  - round 3's: `Thinking` once in 1 run.

  `Thinking` is the rare outcome on every build. Round 1's three runs
  happened not to catch it.
- **`p5-transcript-records` `start`, row 5**, the launch notice `Full access
  enabled: …`: base 2 of 3, round 1 1 of 3, round 2 0 of 1, round 3 1 of 1.

The checkers, re-run on their committed captures under this ledger:
- T-U1+U2's `transcript-wrap/check.py`: 34/34;
- T-U6's `transcript-receipts/check.py`: 35/35.

Each reproduced its `green.txt`.

## The ledger

- **`ctrl-o-width-disputed`** was removed in round 1. Its form is fixed.
  The cluster form round 2's MUST 1 named is fixed, with its own capture.
- **`filler-modifier-count`** (`[[divergence]]`, unsafe-to-copy, owner
  WIDTH-DISPUTED, expires `date:2026-12-04`), new in round 3.
  - fx v0.0.16 counts a skin-tone modifier as no cells wherever it stands
    (`isZeroWidthContinuation`, `display_width.zig:409-421@481104fd`). A
    terminal draws one that follows no emoji as cells of its own. So fx's
    row runs past the margin: line 5 of `p5-transcript-diffwide2`.
  - xfx counts the modifier as the width table does, at least. Copying fx
    would put the line past the margin.
  - Its selector: a modifier after no `Emoji_Modifier_Base` (all of them lie
    in U+261D-270D and U+1F385-1FAF8, unicode-width 0.2.2's
    `is_emoji_modifier_base`), so a person emoji with its skin tone is not
    taken in; and fx's `│  5678`. Checked with `ledger.cascade` on the
    pair: of the rows the two sides do not share, it covers line 5's six
    and no other.
  - The same divergence holds for `a` U+1F3FD and a modifier alone (fx 1
    and 0, xfx 3 and 2, tmux 3 and 2). xfx counted them so before this unit.
- **`ctrl-o-diff-row-clipped`** (`[[cause]]`, defect, owner **DIFF-WRAP**,
  SHOULD 2).
  - fx reflows an overlong Ctrl-O diff row (`reflowDiffRow`,
    `transcript_blocks.zig:718-766@c1db9191`), and xfx's `diff_rows` clips
    it.
  - It is a c1db919 KEEP gap, not width. It covers `p5-transcript-diffwide`
    and `p5-transcript-diffwide2`.
- **`frame-variation-selector-anchor`** (`[[cause]]`, unit, owner
  **ENGINE-VS-ANCHOR**, SHOULD 3).
  - fx v0.0.16 writes a cursor column after every glyph whose cluster holds
    a variation selector (`95e6c8fd`): `disputed_width`,
    `src/core/terminal/engine.zig:2572@481104fd`, and the `ESC[<col>G` it
    sends, `:2586-2588@481104fd`. Not at c1db9191. xfx's row writers do
    not.
  - It is scoped to `p5-transcript-diffwide2` `full`, the one capture that
    shows it. It is not a margin overrun. ENGINE-VS-ANCHOR is a proposed
    unit, not dispatched.
- **`ctrl-o-drawn-wider-raw`** (`[[cause]]`, defect, owner **CTRLO-WIDER**,
  narrowed in round 3, SHOULD 1).
  - At 481104fd fx writes the `drawn_wider` table's scalars outside its
    `isNonPrintingCodepoint` as themselves, and so does xfx's Ctrl-O. The
    bytes are fx's. xfx's painter counts them fewer cells than a terminal
    draws.
  - Its selector is now only the scalars fx counts more cells than xfx does,
    one code point at a time: 3,812 scalars in 63 ranges (U+0605 and U+FF9E
    one cell, U+302E two, `unicode_display_data.zig:81@481104fd`).
  - Left out of it, so a row with one sweeps unresolved rather than here:
    U+FE0F, which would take in every emoji row; the family
    `drawn_wider.rs:23-30` lists (U+261D, U+26F9, U+270C-270D,
    U+2E3A-2E3B, the regional indicators, U+1F3CB-1F3CC, U+1F574-1F575,
    U+1F590), which xfx and fx both count one cell where a terminal draws
    two; and U+E0002-E001F, which both count as none.
  - Its `resolution` says that counting as fx does cannot close that family,
    where xfx's count is already fx's: only naming closes it, as the review
    does. Its `why` now records the 12 U+FE0F forms above.
- **`submit-multi-card-whole`** (`[[cause]]`, decision), unchanged.

**WIDTH-DISPUTED-d** (docs and ledger only; opus-c's round-3 SHOULDs):
- **`ctrl-o-result-format-raw`** (`[[cause]]`, defect, owner T-U1-SAFE),
  new. fx writes a tool's arguments, output and stored result on Ctrl-O
  through its terminal-safe writer, which spells `isNonPrintingCodepoint`'s
  set as `\u{NNNN}` (`full_transcript_screen.zig:5147-5194` and
  `text_utils.zig:643-667@481104fd`; the same at c1db9191). xfx's `railed`
  and `literal` write it raw after `bridge::inert`. It covers read row 2 of
  `p5-transcript-diffwide2` (U+200D; `green.txt:31`), which
  `frame-variation-selector-anchor`'s `why` now names as its second
  reason, and `p5-transcript-diffodd`'s read rows 3 and 4 (U+202E, U+200B,
  U+FEFF), which no cause covered before. The owner is T-U1-SAFE, not
  CTRLO-WIDER: `docs/parity.md:238` keeps this gap open for it, and its
  `127f0502` (`fix/p5-ctrlo-safe-rows`, not in this branch) spells these
  rows as fx does, U+200B-200F among them.
- **`ctrl-o-drawn-wider-raw`**: CTRLO-WIDER owns the 12 U+FE0F forms, and
  the rest of the table, in the inline transcript and the composer too.
  `drawn_wider` is consulted only by `approval_view`.
- **`wrap.rs`'s comment** on U+FE0F: tmux widens it by a cell only when the
  character before it draws narrower than two cells (`U+115F U+FE0F` stays
  two, `clusters.txt:91` against `:9`).

## Files

- `fx/`, `xfx/`, `red/`: both scenarios' snaps, redacted.
- The scripts: `publish.py`, `check.py`, `regress.py`, and `probe/`
  (`probe_seq.c` for libghostty-vt, `dsr_seq.py` for tmux,
  `painter_probe.rs` for xfx's column, `seqs.txt` and `seqs3.txt`).
- The outputs: `green.txt`, `red.txt`, `regressions.txt`,
  `regressions-r2-r3.txt`, `regressions-r1-r2.txt`, `timing.txt`,
  `clusters.txt`, `red-test.txt`, `red-test-2.txt`, `red-test-3.txt` and
  `sweep.txt`.

Commands, from the worktree (`R=.prd/tui-phase5/receipts/width-disputed`):

```
python3 -I -B $R/publish.py . $R p5-transcript-diffwide  <fx-caps> <fix-caps> <base-caps> cb9b45e 20046d6
python3 -I -B $R/publish.py . $R p5-transcript-diffwide2 <fx-caps> <fix-caps> <r2-caps>   cb9b45e 41a3130
python3 -I -B $R/check.py . $R xfx    # green.txt
python3 -I -B $R/check.py . $R red    # red.txt
python3 -I -B $R/regress.py . <base-caps> <fix-caps>    # regressions.txt
```

## Gates

The gates ran on `cb9b45e2` (whose code is `db3b54a1`'s; see above), with
the ledger and this receipt staged. `TMPDIR` was a short scratch directory,
and each suite ran alone, after the idle wait.

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| `cargo clippy --locked --all-targets --features fault-injection -- -D warnings` | ok |
| `cargo test --locked --no-fail-fast` | 4932 passed, 0 failed, 7 ignored; round 3's and round 2's tests ok |
| `cargo test --locked --features fault-injection --no-fail-fast` | 4961 passed, 0 failed, 7 ignored |
| `scripts/check-xfx-identity.sh`, `check-no-stubs.sh`, `check-no-secrets.sh` | ok, ok, ok |
| `scripts/fxcap/test-sweep.sh` (the ledger changed); the ledger loads with its sources checked | OK, both suites (48 and 24 tests); ok |
| `check.py … xfx` (`green.txt`) | 11 claims, 11 hold |
| `check.py … red` (`red.txt`) | 11 claims, 6 hold, 5 fail |
| `regress.py` base / round 3 (`regressions.txt`) | 165 snaps: 162 identical, 3 differ, each explained above |
| `regress.py` round 2 / round 3 (`regressions-r2-r3.txt`) | 165 snaps: 163 identical, 2 differ, both timing |
| `regress.py` round 1 / round 2 (`regressions-r1-r2.txt`, round 2's) | 165 snaps: 162 identical, 3 differ, all timing |
| `transcript-wrap/check.py`, `transcript-receipts/check.py` | 34/34 and 35/35, their `green.txt` reproduced byte for byte |
