# P5 T-U1 + T-U2 receipt: Ctrl-O tool rows wrap; the reading position holds

The unit is epic 4's first two (Transcript), shipped together: fx v0.0.16
(`vercel-labs/fx@481104fd`) T1 and T2 of
`research/tui-phase5:.prd/tui-phase5/research/transcript.md` (§2 T1, T2; §3
rows U1, U2). Base: `main` at `d314549e`. This is round 2 (T-U1U2b, the
review of `6c99caa5`); it replaces round 1's GREEN.

**Builds.** Every `xfx/` capture is from a debug build of `52b150c`, the
commit under this receipt; every `red/` capture from a build of the base
`d314549e`. `publish.py`'s check, run as the captures were copied in: each
xfx snap whose row 1 is the banner reads `x𝒇x v0.1.0-52b150c [debug]`
(every scenario has at least one such snap), each red one
`v0.1.0-d314549`. fx is the v0.16 reference binary (sha256 `8e6cda2e…`,
unchanged after every run).

## What changed

Round 1 (`e21d3aa1`):

- **U1, T1.** `src/tui/full_transcript/tool_status.rs` ports
  `wrapFullToolStatus` and its scanner
  (`src/ui/render_engine/transcript_blocks.zig:470-573,904-975@481104fd`) and
  `expandedContinuationPrefix` (`src/ui/transcript/tool_group_projection.zig:1094-1100@481104fd`);
  `tool_rows` wraps the group header and the call's row with it
  (`materializeExpandedOverride`, `:1061-1092@481104fd`): `│ ` under `├`/`│`,
  two blanks under the header and `└`, reset, prefix, then the tracked
  attributes and foreground (`CellSgr`, `src/ui/assistant/pacer.zig:42-155@481104fd`),
  `?` for a wide character a row cannot hold, a prefix with no room dropped.
- **Dedupe** (`argumentVisibleInToolHeading`,
  `src/ui/full_transcript_screen.zig:6594-6619@481104fd`, 78424464): hidden
  only when non-empty, in the heading's first line, and the heading fits.
  **The first-line half can never matter here**: `spelled()`
  (`src/tui/toolgroup.rs`) makes every control a space, so no heading has a
  line break. It is kept as fx's rule, not tested.
- **U2, T2.** `src/tui/full_transcript/position.rs` ports
  `transcript_presentation.State` (`src/core/output/transcript_presentation.zig:52-305@481104fd`)
  without the anchor; `View::select` pins on a page whose revision or width
  changed (`presentationForRebuiltPage`, `src/ui/transcript/runtime.zig:11584-11590@481104fd`)
  and selects on the page measured now (`selectProjectionViewportOffset`,
  `:11647-11660`).

Round 3 (T-U1U2c) changes only this receipt's checker (above), one comment
in `position.rs` (the tests that call `buildFullTranscriptProjection` also
include `runtime_tests.zig`'s and `resize_tests.zig`'s), and `docs/parity.md`:
the run-on applies to every row the record was given laid out (`Body::Block`)
-- a fork's line, a cancelled question, a replayed turn's cancelled line --
and `/trace` lays them out the same way. The captures stand: no product code
changed.

Round 2 (`25e2c4e`, `f3bf43f`, `52b150c`):

- **One measure** (opus 5). A character's cells are the screen's
  (`wrap::width`, which the frame clips by and the grid places by), and an
  escape ends where the frame's tokenizer ends it (`pacer::escape_at`). fx's
  `display_width` port and the screen disagree on 11 352 of the 2 224 104
  codepoints and codepoint-in-context strings probed; U+17D8 is one cell to
  the port and three to the screen. Breaks are fx's rule.
- **A run of blanks is measured once** (gpt6-U1). fx re-reads the rest of the
  run at every byte; a 200 000-tab row took 22.9 s. A differential test runs
  the cached scanner against the uncached one on 400 random strings, every
  start and every width from 0 to 13, and finds no break moved. A second one
  runs the scanner against round 1's on an alphabet both measures agree on.
- **A resolution runs on** (opus 2, reproduced). A question's resolution
  keeps the rows it was written at. One wider than a narrowed screen goes on
  to the next row, mid-word and in its colours, as fx's row walker lets a
  static row (`ProjectionRowWalker.appendWithSoftWrapPrefix`,
  `full_transcript_screen.zig:3968-4039@481104fd`). Round 1's "nothing to
  apply to" was wrong about the screen. It was right that `formatFullToolStatus`
  is not the path: fx's resolution is a `question_resolution` raw entry, not a
  `tool_status` one.
- **A width change is a rebuild** (gpt6-U2, with fx as the oracle). fx's
  `prepare_projection` is reached only through `buildFullTranscriptProjection`
  (`runtime.zig:10661-10676@481104fd`), which only `runtime.zig`'s tests call.
  The live screen pins on each installed page. Round 1 queued the bookmark
  instead. `p5-transcript-records` shows fx keeping a reader on its
  `* session:` row across 120 → 80 → 120; round 1 moved it to the welcome's
  blank row. Positions among the records keep their row offset through a
  rebuild, a width change included.
- **`docs/parity.md`** (C-o row): one measure, the run-on, a width change as a
  rebuild. `TARGET_BYTES` (`src/tui/toolgroup.rs`, 1024 B then `…`) is named
  **open, T-U4, once epic 2 has landed**, with fx's unported test.
- **Ledger.** The whole-snap `full-transcript-shift` cover is off the four
  scenarios. `transcript-page-offset` takes only rows 1-2 and 35-37 of the
  `full-*` snaps of anchor, anchormid, resize, records and question.
  `session-record` matches the tool list's later rows at any width.
  `subagent-tool` (whole snap) covers only the tools scenario's snaps after
  fx's subagent step.

## RED

- **Tests on the base**, pasted into a throwaway `git clone --no-local`:
  - **Round 1 (`red-tests.txt`):** 8 tests were pasted and **8 failed**
    (`a_position_inside_the_records_keeps_its_offset` reads the new type and
    could not be pasted). Round 1's anchor test did not reach its hold
    assertion. It stopped at the U1 precondition ("the heading wraps to two
    rows", `red-tests.txt:54-57`), so round 1 had no U2 test RED; U2's RED was
    only the anchormid capture.
  - **Round 2 (`red-u2-base.txt`):** the anchor test now asserts the hold
    first and fails there: `after[..36] != before[..36]`, `left: "│  a line 6
    …"`, `right: "│  a line 11 …"`. The width test (`left: "│  b line 2 …"`,
    `right: "├ Read b.txt"`) and the records width test fail too: 3 of 3.
- **Round 2's tests on round 1's head** `6c99caa5` (`red-round1-head.txt`):
  **5 fail.**
  - the resolution, cut at 60 with `…`;
  - the records width change, which moved the reader;
  - a lone ESC before `€`, which became `"\u{1b}�"`;
  - `"៘" at 8: "  ៘x ៘x" is 11 cells`;
  - `22.9s for a run of 200 000 tabs`.

  The zero-width and the attributes-only tests pass on round 1 as well. They
  pin behaviour; they caught no defect.
- **Captures** (`red.txt`, `check.py` on `fx/` against `red/`): the stitched
  pages fail where the heading is cut, and `anchormid`'s hold fails. The
  base's row 1 goes from `│  6  a line 6` to `● 6 tool calls …`. The records
  top fails at 80, where the base moved to `  mcp: none`, and the question
  fails at 60, where the base cut `fast loc…`.

## GREEN: `check.py` on the captures (`green.txt`)

`check.py` reads every pair cell by cell, styles included, after the
ledger's normalizations. It takes out only two causes, by exact pattern:

- fx's `│  action: run` and `│  yield_time_ms: N` (`tool-schema`), counted;
- fx's `system prompt:` and `skills:` lines, and each side's tool list as one
  row (`session-record`), counted.

Nothing else is excused. There is no edge allowance. And `check.py` fails
closed (round 3, T-U1U2c):

- a tool list must end at a row ending `)`, and each of its later rows must
  be tool names only. Otherwise the capture is malformed and the claim fails;
  round 2 folded every row after an unclosed list into `<tools>`;
- every claim states how many rows it takes out, side by side and cause by
  cause, and those counts must match;
- every claim states how many rows only one side covers past the compared
  ones, and how many are compared from the top; those must match too;
- the row above the footer must be blank and unstyled. It is read, not
  dropped.

The claims:

- **stitch:** each side's windows, a page or an arrow apart, are checked
  against each other where they overlap. They are joined into the bottom of
  that side's document, and the two documents are compared bottom-up. A page
  counted from the tail starts two rows later on fx, so the anchormid and
  resize scenarios now take an arrow window (`Up`, 3 rows) above each claimed
  page. That window is what gives every row of the page an fx partner.
- **inside:** a pinned window is found whole, once, inside each side's
  document and within the rows both sides cover. fx's window starts *d* rows
  later; a view pinned at another width must start the same *d* as the view
  it was pinned from: one signed shift.
- **hold:** a side's two snaps are identical across a rebuild. **row1:** the
  first row is unchanged across a width change. **insert:** at the top of the
  document, where the new `* network:` record lands *inside* the view, the
  rows above it are the same, then come its header and `* network:` row,
  then the earlier rows pushed down by its height.

**Which rows have an fx partner.** A tail window's top two xfx rows are
compared only when the stitch has a window above it; fx's page starts two
rows later. So:

- anchormid's and resize's claimed pages (`full-pgup`, the pinned 80- and
  120-column views) are fully compared, every row;
- tools and question fit on one screen, and are compared whole;
- anchor's three windows reach the document's top, and only xfx's banner is
  left;
- **two exceptions**:
  - anchormid's `full-end`, a lone tail: xfx's top two rows are not compared;
  - records' windows, at the top: fx's session record is two rows taller, so
    xfx's last two record rows (`a line 6`, `a line 7`) are not compared.

The `green.txt` tally is in "Gates" below. Highlights:

- **tools** at 120 and at 80: the whole document is the same with exactly 2
  `tool-schema` and 2 `session-record` rows taken out, and the heading is
  wrapped as fx wraps it.
- **anchormid:** the joined documents agree on 73 rows, and xfx's
  `full-pgup` lies inside them. Every row of it is compared, which includes
  `│  7     a line 7 …`, the row round 1's checker excused. fx's page starts 2
  rows later. **Hold:** `full-pgup == full-pgup-back == full-pgup-after` on
  both sides, all 36 rows (xfx row 1 `│  7`, fx `│  9`).
- **anchor:** 103 rows the same from the top of the document down. Only xfx's
  banner has no fx partner, because fx's two-pages-up stops one row lower.
  At the top, the settling turn's `* network:` record lands inside the view,
  under the record before it (xfx row 19, fx row 20). The view holds above
  it, and the 13 (fx 12) rows under it are the ones that were there, pushed
  down 5.
- **resize:**
  - the 120-column running page agrees with fx on 73 rows;
  - the view pinned at 80 is found in the 80-column document with fx 2 rows
    later, as at 120;
  - widened back, the same holds;
  - `full-tail-120-pgup == full-pgup-after` on both sides.
- **records:** 33 rows the same from the top at 120, at 80 and at 120 again.
  Row 1 is `* session: …` in every snap, on both sides.
- **question:** at 60, the resolution runs on exactly as fx's does: `…the
  nightly imp` / `ort job write …`, `…the fast loca` / `l disk`. `l disk`'s
  cells are in 245 on both sides, the colour the row opened with: the
  comparison includes styles, so the reopen is fx's.

## The checkers against changed captures (`mutants.txt`)

`mutants.py` copies the captures, changes one row, and runs three checks:
round 1's `pair.py` (`PAGE_OFFSET=2`, round 1's ledger), `check.py`, and
`sweep-view.py`. The last runs the sweep's own classification
(`scripts/fxcap/sweep.py`) under round 1's ledger and under this one.

- **Unchanged captures:** all three pass.
- **xfx `full-pgup` row 1 (top edge), a printed digit changed** (`a line 7` →
  `a line 8`):
  - in that window only: round 1 passes it as "page offset". `check.py`
    fails it, because xfx's own arrow window disagrees;
  - in both xfx windows that show the row: round 1 passes it. `check.py`
    fails it against fx: `fx '│  7     a line 7 …' / xfx '│  7     a line 8 …'`.
- **fx `full-pgup` row 36 (bottom edge), `Tool` → `Toil`:** round 1 passes
  it. `check.py` fails it: `fx '… · Toil' / xfx '… · Tool'`.
- **xfx row 12 (mid-page), in both windows:**
  - round 1's checker catches it;
  - the sweep under round 1's ledger does not, since its whole-snap cover
    names a cause for it;
  - under this ledger the sweep lists 2 "rows no cause covers".

On the two edge mutants the sweep under this ledger stays quiet: rows 1-2
and 35-37 are what `transcript-page-offset` names. `check.py` reads them:

- in anchormid's claimed `full-pgup`, the page those mutants change, rows
  1-2 and 35-36 are compared through the stitch: the arrow window above it
  gives each an fx partner. Not on every snap: the two exceptions in "Which
  rows have an fx partner" above (anchormid's lone `full-end`, records'
  windows at the top) leave xfx's top two and last two rows there without
  one;
- row 37, the blank row above the footer, must be blank and unstyled, on
  every Ctrl-O snap;
- and since round 4 (T-U6) the footer must be on row 38, so every record is
  exactly 36 rows (`record()`).

## Round 3's assertions against changed captures (`mutants-c.txt`)

`mutants-c.py` runs one mutant per assertion round 3 added, through round
2's `check.py` and through this one. Each mutant keeps its screen's 40 rows,
with the footer where it was. **Round 2 passes all six, and this `check.py`
fails all six**; on unchanged captures both pass.

| mutant | this `check.py` |
|---|---|
| records: xfx's tool list loses its `)`; `mcp: none` and `a line 3` below it changed (gpt6-b's MUST) | `malformed capture: … the tool list from row 2 has no closing row: '  +5 more'` |
| records: xfx's tool list gains a row of tool names | `xfx: 3 tools-list rows taken out, the claim is 2` |
| records: fx gains a second `skills: loading` | `fx: 3 session-record rows taken out, the claim is 2` |
| tools: xfx's banner gone from the one-screen record | `only fx covers 1 and only xfx 0, the claim is 0 and 0` |
| question: a row above xfx's banner | `only fx covers 0 and only xfx 1, the claim is 0 and 0` |
| anchormid: text in the blank row above xfx's footer, on all three screens of the held page | `malformed capture: the row above the footer is 'stray', not a blank one` |

## Round 4's assertions (T-U6, `mutants-c.txt`)

T-U1+U2's review left three pins open, and `mutants-c.py` printed its
results without asserting them. Round 4 (T-U6, branch
`feat/p5-transcript-receipts`):

- `record()` fails a capture whose footer is not on row 38: a record is 36
  rows, never a shorter or longer window;
- `check_insert` pins the row the record is put in at (fx 20, xfx 19) and its
  height (5), as the claim's;
- `mutants-c.py` **asserts**: every checker passes the unchanged captures,
  the round before passes each mutant (round 2 for round 3's six, round 3 --
  `6eee9fb9`'s `check.py` -- for round 4's three), and this `check.py` fails
  each with the FAIL line the mutant names. It exits 1 otherwise, and does so
  when it is run with round 3's `check.py` in this one's place
  (`mutants-c-meta.txt`).

| round 4 mutant | this `check.py` |
|---|---|
| tools: xfx's footer one row up on both screens of the one-screen record | `malformed capture: the footer is on row 37, not row 38` |
| anchor: xfx's network record lands a row higher | `xfx: a 5-row record at row 18, the claim is a 5-row record at row 19` |
| anchor: xfx's network record gains a row | `xfx: a 6-row record at row 19, the claim is a 5-row record at row 19` |

`green.txt` and `red.txt` are unchanged by round 4: `check.py` prints the
same lines on the same captures (34 claims, 34 hold; 34 claims, 12 fail).

`sweep-view.txt` is the sweep's view of every `full*` pair of the seven
scenarios under this ledger: **0 rows no cause covers**.

## Regressions (`regressions.txt`)

Every other `p5-*` scenario that opens Ctrl-O was compared, red against xfx,
every snap's `.ansi` with the build, clocks and network ms normalized:

- `p5-transcript-cancel`, `-diffctl`, `-md`, `-md-light`, `-tz`, `-tzsys`.

The result is in "Gates".

## Notes for a re-run

- anchormid's `full-pgup-up` and `full-pgup-back` land about a second before
  the 9 s command settles. That is the research's turn, kept so its fx
  captures still apply. resize has its own 20 s turn for this reason.
- `Reading::refresh_bookmark` has no reader in xfx now: the pin overwrites
  the bookmark before any selection reads it. It is kept because it is fx's
  state, as `select_visual_offset` leaves it.
- `origin/main` moved to `88c7a5be` after `46c4efa6`, which this branch
  merged last. It is **not merged here**: a merge commit would be a head that
  built none of these captures.

## Files

- `fx/`, `xfx/`, `red/`: the captures (redacted with `scripts/fxcap/redact.py`).
- `check.py`, `sweep-view.py`, `mutants.py`, `mutants-c.py`: the checkers.
- `green.txt`, `red.txt`, `sweep-view.txt`, `mutants.txt`, `mutants-c.txt`,
  `regressions.txt`: their output (round 3 re-ran all but `regressions.txt`
  from the same captures).
- `red-tests.txt`, `red-u2-base.txt`, `red-round1-head.txt`: the RED test runs.

## Gates

Round 2 ran these on `4a19a15e`, whose code is `52b150c`'s. Round 3 ran the
whole list again on `01981fc3`, with the same results (9817 files for the
three checks). `TMPDIR=<tmp>/tw`; each full run came after
the idle wait:

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| `cargo clippy --locked --all-targets --features fault-injection -- -D warnings` | ok |
| `cargo test --locked --no-fail-fast` | 4913 passed, 0 failed, 7 ignored |
| `cargo test --locked --features fault-injection --no-fail-fast` | 4942 passed, 0 failed, 7 ignored |
| `scripts/check-xfx-identity.sh`, `check-no-stubs.sh`, `check-no-secrets.sh` | ok, ok, ok (9815 files) |
| `scripts/fxcap/test-sweep.sh` (the ledger changed) | OK, both suites |
| `cargo build --locked --release`; `--features fault-injection --target-dir target/faulty` | ok; ok |
| `scripts/smoke.sh target/release/xfx` | 47 checks, 0 failures |
| `scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx` | 37 scenarios + the oracle, 1111 checks, 0 failures |
| `check.py` on `fx/` against `xfx/` (`green.txt`) | 34 claims, 34 hold |
| `check.py` on `fx/` against `red/` (`red.txt`) | 34 claims, 12 fail |
| `mutants-c.py` (`mutants-c.txt`, round 3) | round 2's `check.py` passes 6 of 6 mutants, this one fails 6 of 6 |
| `mutants-c.py` (`mutants-c.txt`, round 4, T-U6) | asserted: the earlier round passes 9 of 9 mutants, this `check.py` fails 9 of 9 with the named FAIL line; exit 0 (`mutants-c-meta.txt`: exit 1 with round 3's `check.py` in its place) |
| `sweep-view.py` (`sweep-view.txt`) | 0 rows no cause covers |
| `regressions.txt` | 23 of 23 snaps identical |
