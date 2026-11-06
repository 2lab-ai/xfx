# P5-FK-GAP receipt: the blank row above a panel over a fork's line

The one pair the epic-1 integration left unresolved
(`.prd/tui-phase5/receipts/epic-1-integration/README.md`, "Three pairs are
new"): `fork-edge/status-empty` in `scripts/fxcap/scenarios/p5-effort-fork-fork-edge.cap`.
`/status` opened right after a fork made before the first prompt. fx v0.0.16
draws a blank row between `original session …` and the panel's rule; xfx
drew none. Base: `integrate/p5-epic-1` @ `cf25feaf`.

## The rule (fx `481104fd`)

- The footer asks its frame for a blank row above it whenever a modal hides
  the composer: `.top_gap_rows = if (viewer_active or modal_active) 1 else 0`
  (`src/ui/footer/surface_frame.zig:686@481104fd`). `modal_active` is an
  approval, a question or a compact command menu (`:392-395`), and the
  compact menus are `/statusline`, `/status` (the info panel), `/usage` and
  `/workspace` (`src/ui/footer/render_input.zig:427-433`). The composer is
  hidden exactly then (`input_visible`, `surface_frame.zig:396`).
- The frame takes the **larger** of that and the transcript's own gap under
  its tail, never their sum: `idle_gap = @max(transcript_gap,
  input.footer.top_gap_rows)`, where `transcript_gap` is the tail's trailing
  blank rows or its block kind's footer gap
  (`src/ui/render_engine/frame_layout.zig:388-392`;
  `footerBoundaryGapRowsForTail`, `transcript_blocks.zig:2978-2990`, gives
  notices and answers 1). The fork's plain line has none: with the composer
  under it fx draws no blank row (`fork-edge/forked-first`, rows 3-4),
  with the panel it draws one (`fork-edge/status-empty`, row 4).
  Upstream's own test of the rule is "footer request reserves a boundary
  gap after a user tail" (`frame_layout.zig:949-968`).
- Only on the idle branch: a turn's `.thinking` activity takes the gap above
  its own row instead (`:369-385`). And only with room: the footer is solved
  first and the gap gets what it leaves
  (`available_rows >= footer_height +| min_body_rows +| idle_gap`, `:393`).

FK2's `raw_gap` (`src/tui/shell.rs`) opens the next notice or card under the
plain line with a blank row; that is the document's gap. The band had no
counterpart of `top_gap_rows`, so nothing put a blank above a panel over a
tail with no gap of its own.

## The port

xfx's transcript gap is a row of the document, the blank row a block ends
on (`Transcript::ends_blank`). So `max(transcript_gap, 1)` is: the band adds
one blank row of its own exactly when the document does not end on one.

- `src/tui/layout.rs`: `Geometry::gap` and `Geometry::gapped(document_ends_blank)`.
  The row is the band's top (`band_top()`), above a band with no composer
  rows (the panels `fit` solves with none: a question, a rule confirmation,
  `/statusline`, `/status`, `/usage`, `/workspace` -- upstream's
  `modal_active` set) and no turn's row, and only where a document row is
  left beside it. Nothing under it moves.
- `src/tui/shell.rs`: `Shell::placed` applies it to every geometry `fit`
  solves and to every `refloat` (rows landing), so a block written under the
  panel ends the document blank and takes the row back. Closing the panel
  re-solves the band without it. `band_rows` paints it blank; the approval
  readiness offset counts it.
- `docs/parity.md`: the `/fork` row's sentence on the plain line now says
  the panel keeps the band's blank row above it.

Not ported: `viewer_active`. xfx's full transcript is on the alternate
plane, where there is no band.

## Tests

- `layout::tests::a_panel_in_place_of_the_composer_keeps_a_blank_row_over_a_document_without_one`
- `layout::tests::the_blank_row_over_a_panel_floats_with_the_band_and_gives_way_to_the_panel`
- `shell::tests::a_panel_over_a_forks_plain_line_keeps_a_blank_row_above_it_until_it_closes`
- `shell::tests::a_block_that_lands_under_the_panel_ends_the_document_blank_and_takes_the_row_back`

With `gapped` made to keep no row (a scratch edit, reverted), all four
fail; with it, they pass.

## RED (the base, `cf25feaf`)

`capture.sh` at 40x120 on fx v0.0.16 (sha256 `8e6cda2e…`, `FX_AUTO_UPGRADE=0`,
the binary's bytes unchanged after the run) and on the base's debug build,
each under a 180 s `timeout`, no unbounded input. `red/` holds the pair and
`red.txt` is `cells.py` and `status-panel/pairs.py` over it:
`fork-edge/status-empty` is **unresolved-defect, no cause (`?`)**. fx's row 4
is blank and its rule is on row 5; xfx's rule is on row 4, so every panel
row after it is one row higher (15 rows differ). The other ten
`fork-edge` pairs pass on the base as they do here.

## GREEN

`fx/` and `xfx/` are the whole `fork-edge` scenario from the sweep below
(the head's first retry; see "Sweep"), and `green.txt` is `cells.py` and
`pairs.py` over every pair.

- `status-empty`: rows 2-7 are now fx's -- the blank, `original session
  …` (its product name and id normalized, as in every fork pair), **the
  blank row (row 4)**, the rule, `Status`, the blank. What still differs is only the panel's own approved
  rows (`Sandbox`, `Step limit`, `Updates`) and the ids, as in
  `fork/status`: `approved-divergence-cascade (aligned)` under
  `status-panel-sandbox`, `status-panel-step-limit`, `status-panel-updates`,
  no unresolved row. It cannot be normalized-identical while those rows are
  approved divergences; it is now the same verdict as its sibling
  `fork/status`.
- `fork-of-empty` (after Escape): normalized-identical, as before. The row
  went with the panel.
- Every other `fork-edge` pair: normalized-identical or identical.

## Pairs (fx v0.0.16; xfx = this branch)

Counts are identical-or-normalized / approved / total, classified with
`status-panel/pairs.py` under this branch's ledger. "Integrated" is the
epic-1 integration receipt's column. Here: the sweep below at 40x120, plus
`capture.sh` at the units' own sizes (14x120 and 12x120 on the debug build,
24x50 on the release build, as before).

| scenario | integrated | this branch |
|---|---|---|
| `p5-panels-keys` | 2 / 5 / 10 | 2 / 5 / 10 |
| `p5-panels-noauth` | 0 / 2 / 2 | 0 / 2 / 2 |
| `p5-panels-status` | 5 / 6 / 11 | 5 / 6 / 11 |
| `p5-panels-status-first` | 1 / 2 / 3 | 1 / 2 / 3 |
| `p5-panels-status-badsettings` | 0 / 3 / 3 | 0 / 3 / 3 |
| `p5-panels-status-short` 14x120 | 3 / 1 / 6 | 3 / 1 / 6 |
| `p5-panels-status-short` 24x50 | 1 / 5 / 6 | 1 / 5 / 6 |
| `p5-panels-status-short` 40x120 | (swept, no count) | 1 / 5 / 6 |
| `p5-panels-permissions` | 16 / 1 / 20 | 16 / 1 / 20 |
| `p5-panels-permissions-saved` | 5 / 0 / 8 | 5 / 0 / 8 |
| `p5-permissions-early` | 11 / 0 / 11 | 11 / 0 / 11 |
| `p5-permissions-early-probe-ask-after` | 0 / 1 / 1 | 0 / 1 / 1 |
| `p5-session-early-prefs` | 5 / 0 / 5 | 5 / 0 / 5 |
| `p5-effort-fork-fork` | 15 / 1 / 16 | 15 / 1 / 16 |
| `p5-effort-fork-fork-edge` | 10 / 0 / 11 | 10 / **1** / 11 |
| `p5-effort-fork-fork-empty` | 4 / 0 / 4 | 4 / 0 / 4 |
| `p5-effort-fork-fork-continue` | 2 / 0 / 2 | 2 / 0 / 2 |
| `p5-effort-fork-effort-menu` | 18 / 0 / 18 | 18 / 0 / 18 |
| `p5-effort-fork-effort-column` | 17 / 0 / 17 | 17 / 0 / 17 |
| `p5-effort-fork-effort-{backtab,ctrlc,down,loading}` | 2 / 0 / 2 each | 2 / 0 / 2 each |
| `p5-effort-fork-effort-nolevels` | 5 / 0 / 5 | 5 / 0 / 5 |
| `p5-effort-fork-effort-short` 12x120 | 3 / 0 / 3 | 3 / 0 / 3 |
| `p5-effort-fork-effort-short` 40x120 | (swept, no count) | 3 / 0 / 3 |
| `p5-effort-fork-slash-rank` | 3 / 0 / 4 | 3 / 0 / 4 |

One pair moved: `fork-edge/status-empty`, unresolved with no cause to
approved (aligned). Nothing else moved. The other `p5-*` scenarios the sweep
took (`p5-panels-help`, `-slash`, `-sweep-a`, `-sweep-b`, `-badsettings`,
`p5-effort-fork-oldread-*`) are not in the integration table; every pair of
theirs that does not pass at the head fails the same way at the base.

## Sweep

The `sweep.yml` job, run locally:
`ci-sweep.sh --base cf25feaf --fx <fx v0.0.16> --pin v0.0.16 --xfx <head
debug> --base-xfx <cf25feaf debug> --jobs 2`, `FX_AUTO_UPGRADE=0`, head
`ea3db6ea` (this receipt's draft; the code is `1bde6c14`'s). 42 scenarios
(10 core, 32 named by this receipt), **313 snap pairs: 205 pass at the head,
0 new, 0 regressions**; 1,452 s. The pairs not passing at the head are
the base's as well: `h1-resize` `small`/`large` (known on `main`,
`scripts/fxcap/ci-core.txt`), the panels' approved
pairs, and the research scenarios' known unresolved ones above.
`fork-edge/midturn` varied in the head's first run (`stream-position`, a
`nondeterminism` cause) and passed on its first retry, which `fx/`/`xfx/`
here are; `status-empty` is the one pair whose verdict changed against the
base (base `unresolved-defect`, head `approved-divergence-cascade`).

Every file here went through `scripts/fxcap/redact.py`:
`/private/tmp/claude-501` is `<tmp>`. `.ansi` files are tmux's
re-serialization (`capture-pane -e`), not the programs' wire bytes.

The 32 scenarios this receipt names for the sweep, so the PR's own
`sweep.yml` run takes the same set: `p5-panels-keys.cap`,
`p5-panels-noauth.cap`, `p5-panels-status.cap`, `p5-panels-status-first.cap`,
`p5-panels-status-badsettings.cap`, `p5-panels-status-short.cap`,
`p5-panels-permissions.cap`, `p5-panels-permissions-saved.cap`,
`p5-panels-help.cap`, `p5-panels-slash.cap`, `p5-panels-badsettings.cap`,
`p5-panels-sweep-a.cap`, `p5-panels-sweep-b.cap`,
`p5-permissions-early.cap`, `p5-permissions-early-probe-ask-after.cap`,
`p5-session-early-prefs.cap`, `p5-effort-fork-fork.cap`,
`p5-effort-fork-fork-edge.cap`, `p5-effort-fork-fork-empty.cap`,
`p5-effort-fork-fork-continue.cap`, `p5-effort-fork-effort-menu.cap`,
`p5-effort-fork-effort-column.cap`, `p5-effort-fork-effort-backtab.cap`,
`p5-effort-fork-effort-ctrlc.cap`, `p5-effort-fork-effort-down.cap`,
`p5-effort-fork-effort-loading.cap`, `p5-effort-fork-effort-nolevels.cap`,
`p5-effort-fork-effort-short.cap`, `p5-effort-fork-slash-rank.cap`,
`p5-effort-fork-oldread-src.cap`, `p5-effort-fork-oldread-v1.cap`,
`p5-effort-fork-oldread-v2.cap`.

## Gates

Run on this commit's tree before this Gates section was appended (the code is `1bde6c14`'s), with `TMPDIR=/private/tmp/claude-501/fsg`.

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| `cargo clippy … --features fault-injection -- -D warnings` | ok |
| `cargo test --locked --no-fail-fast` | 4,721 passed, 0 failed, 7 ignored (the integration's 4,717 and this unit's 4) |
| `cargo test … --features fault-injection` | 4,750 passed, 0 failed, 7 ignored |
| `scripts/check-xfx-identity.sh`, `check-no-stubs.sh`, `check-no-secrets.sh` | ok |
| `cargo build --locked --release`; `--release --features fault-injection --target-dir target/faulty` | ok |
| `scripts/smoke.sh target/release/xfx` | 46 checks, 0 failures |
| `scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx <dir>` | 37 scenarios and the oracle, 1,111 checks, 0 failures |
| `scripts/check-preview-contract.sh` | ok |
| `scripts/check-tui-preflight-cost.sh` | ok (every case under budget) |

No pty test failed, so none was re-run.
