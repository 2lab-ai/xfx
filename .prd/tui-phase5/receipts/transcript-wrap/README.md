# P5 T-U1 + T-U2 receipt: Ctrl-O tool rows wrap; the reading position holds

The unit is epic 4's first two (Transcript), shipped together: fx v0.0.16
(`vercel-labs/fx@481104fd`) T1 and T2 of
`research/tui-phase5:.prd/tui-phase5/research/transcript.md` (§2 T1, T2; §3 rows
U1, U2). Base: `main` at `d314549e`.

## What changed

- **U1, T1.** `src/tui/full_transcript/tool_status.rs` ports
  `wrapFullToolStatus` and its scanner (`src/ui/render_engine/transcript_blocks.zig:470-573,904-975@481104fd`)
  and `expandedContinuationPrefix` (`src/ui/transcript/tool_group_projection.zig:1094-1100@481104fd`).
  `tool_rows` hands the group header and the call's row to it instead of
  `ellipsized` (`materializeExpandedOverride`, `tool_group_projection.zig:1061-1092@481104fd`):
  `│ ` under `├`/`│`, two blanks under the header and `└`; the continuation
  is reset, then the prefix in the default style, then the tracked attributes
  and foreground (`CellSgr`, upstream's `assistant_pacer.SgrState`,
  `src/ui/assistant/pacer.zig:42-155@481104fd`); a wide character a row
  cannot hold is `?`; a prefix that leaves no room is dropped.
- **Argument dedupe** (`argumentVisibleInToolHeading`,
  `src/ui/full_transcript_screen.zig:6594-6619@481104fd`, 78424464): hidden
  only when the value is non-empty, in the heading's first line, **and** the
  heading's width fits `cols`.
- **Raw statuses keep their replay** (`formatFullToolStatus`,
  `transcript_blocks.zig:871-902@481104fd`, 4e4b599e): xfx's Ctrl-O has no raw
  tool-status entry -- every call is drawn through its group, as upstream's
  expanded override, and a question is its resolution's rows (`Body::Block`,
  untouched). Nothing to port; said in `docs/parity.md`.
- **U2, T2.** `src/tui/full_transcript/position.rs` ports
  `transcript_presentation.State` (`src/core/output/transcript_presentation.zig:52-305@481104fd`)
  without the anchor: offset from the top, `follow_tail`, bookmark entry +
  intra row, `prepare_projection` (width change), `pin_reading_position`
  (records keep their offset, b1a53b01), `select_visual_offset`,
  `refresh_bookmark`. `View::select` pins on a page whose revision or width
  changed since the last one (`presentationForRebuiltPage`,
  `src/ui/transcript/runtime.zig:11584-11590@481104fd`) and selects on the
  page as measured now (`selectProjectionViewportOffset`, `:11647-11660`).
  Item rows: welcome = 1, records = 0, entry `i` = `i + 2`; an entry starts
  at its header, under the separator (`renderEntriesForFullPresentationInterruptible`,
  `transcript_blocks.zig:2084-2096@481104fd`).

## Tests ported

| fx test (`@481104fd`) | xfx test |
|---|---|
| `full tool status paints its continuation prefix unstyled before replaying styles` (`transcript_blocks.zig:3918`) | `tool_status::a_continuation_prefix_is_painted_unstyled_before_the_styles_are_reopened` |
| `full tool status drops a continuation prefix that leaves no room` (`:3948`) | `tool_status::a_continuation_prefix_that_leaves_no_room_is_left_off` |
| `full tool status reopens a bounded style on every continuation row` (`:3955`) | `tool_status::a_long_highlighted_row_reopens_a_bounded_style_on_every_continuation` |
| `full tool status marks a wide character that cannot fit` (`:3979`) | `tool_status::a_wide_character_that_cannot_fit_is_a_question_mark` |
| (6f071dea's 14 KB heredoc, bounded: bytes ≤ source + 37/row, < 2 s) | `tool_status::a_fourteen_kilobyte_heredoc_wraps_in_time_and_space_linear_in_its_length` |
| `expanded rows wrap under their tree rail and keep command styling` (`tool_group_projection.zig:2053`) | `a_groups_rows_wrap_under_their_tree_rail_and_keep_the_commands_colours` |
| `expanded group headers wrap instead of clipping` (`:2133`) | `a_groups_header_wraps_instead_of_being_cut` |
| `full projection wraps a long tool status instead of clipping it` (`full_transcript_screen.zig:969`) | `a_long_heading_wraps_under_its_rail_instead_of_being_cut` |
| `full projection keeps semantic arguments beside a wrapped tool heading` (`:1046`) | `arguments_beside_a_heading_wider_than_the_screen_keep_their_rows` |
| `full projection keeps short session input that a long heading contains by chance` (`:1089`) | `a_short_argument_a_long_heading_contains_by_chance_keeps_its_row` |
| `transcript presentation pins the reading position to its entry for a rebuilt page` (`transcript_presentation.zig:397-448`) | `position::the_reading_position_is_pinned_to_its_entry_for_a_rebuilt_page` (the anchor case has no counterpart) |
| `… clamps bookmarks and selects retained neighbor` (`:363`), `… scroll saturates` (`:331`), `… bookmark skips synthetic id-0 records` (`:464`) | `position::*` |
| (screen level) | `the_reading_position_holds_when_the_page_rebuilds_above_and_below_it`, `a_position_inside_the_records_keeps_its_offset`, `a_width_change_keeps_the_entry_the_reader_was_in`, `tool_rows_fit_the_narrowest_screens` |

## RED

The screen-level tests, pasted onto the base in a throwaway `git clone
--no-local` (`position.rs`'s tests and `a_position_inside_the_records_keeps_its_offset`
read the new type and were left out): **8 of the 9 pasted fail**
(`red-tests.txt`) -- the six U1 tests (an `…` on the row, the heading cut),
the anchor test (`after[..36] != before[..36]`) and the width test (`left:
"│  b line 2 of the b file"`, `right: "├ Read b.txt"`). GREEN: `cargo test
--lib full_transcript`, 75 passed.

## Captures

```
FX_BIN=<fx v0.0.16, sha256 8e6cda2e…> TMPDIR=/private/tmp/claude-501 scripts/fxcap/capture.sh fx  <scenario> <size> <dir>
XFX_BIN=<xfx>                         TMPDIR=/private/tmp/claude-501 scripts/fxcap/capture.sh xfx <scenario> <size> <dir>
```

`fx/` is v0.0.16, `red/` the base (`d314549e`), `xfx/` this branch's head.
capture.sh found the fx binary's bytes unchanged after every run. The
scenarios are the research's, with three changes each (said in their
headers): xfx's own turns file (`terminal`/`exec` for fx's `shell`/`run`; the
tools scenario's second step, two `subagent` calls, has no xfx tool and is
left out), `xfx: env XFX_PERMISSION_MODE=yolo`, and `env TZ=UTC` on both so
v0.0.16's local-time header (T-U5) reads UTC. `p5-transcript-resize` is new
(capture.sh has `resize` on `main`): a page up mid-document while the command
runs, the window narrowed to 80 columns with Ctrl-O up, closed and reopened,
widened back, a page up, the command settling.

`pair.py` compares a pair: the ledger's normalizations (banner, timestamp,
network ms), then `ledger.aligned`, and each row left without a match is
covered by a named `[[cause]]` on its own side (`session-record`,
`tool-schema`), is a blank above the footer (a shorter record leaves more), is
one of at most two rows at a page's edge (`PAGE_OFFSET=2`: fx's command entry
has two argument rows xfx's tool has not, `action: run` and
`yield_time_ms`, so a page counted from the tail starts two rows later on fx),
or is **UNEXPLAINED**. At the top of the document (`anchor`'s `full-pgup2`,
`full-pgup2-after`) the edge rows are the other way round: fx's record is two
rows longer at its head (`session-record`'s `system prompt:` and `skills:`
lines), so fx's row 1 is the blank under its banner, xfx's is the banner, and
xfx shows one more row at the bottom -- RED reads the same, not this unit's.
`pairs/{red,green}/` hold every report.

| scenario | snap | RED unexplained | GREEN unexplained |
|---|---|---|---|
| tools 40x120 | `full-running`, `full-pgup` | 4, 4 (the cut heading) | **0, 0** |
| tools 40x80 | `full-running`, `full-pgup` | 6, 6 | **0, 0** |
| anchormid 40x120 | `full-running` / `full-pgup` / `full-pgup-after` | 5 / 5 / 19 | **0 / 0 / 0** |
| anchormid 40x120 | `full-end`, `running`, `end` | 0 | 0 |
| anchor 40x120 | `full-running`, `full-pgup` | 5, 5 | **0, 0** |
| anchor 40x120 | `full-pgup2`, `full-pgup2-after` (top of document) | 0 | 0 |
| resize 40x120 | `full-running`, `full-pgup`, `full-pgup-80`, `full-tail-80` | 5, 5, 5, 6 | **0, 0, 0, 0** |
| resize 40x120 | `full-tail-120`, `full-pgup-120`, `full-pgup-after` | 0 | 0 |
| tools 40x120 / 40x80 | `end`, `end.full`, `full-pgup-after`, `full-tail`, `full-tail-up4` | 10, 10, 21, 21, 24 | the same, row for row |

The last row's rows are fx's two subagent calls and the counts they change
(`● 8 tool calls · 5 read · 2 subagents · 1 command`): xfx has no subagent
tool (`docs/parity.md`, deferred). Not this unit's, and unchanged by it.

**Row 1 holds** (`anchormid`, before the rebuild → after it): fx `│  9     a
line 9 of the a file` → the same; RED `│  6     a line 6 …` → `● 6 tool calls
· 5 read · 1 command` (jumped); GREEN `│  7     a line 7 …` → the same. The two
rows between fx's `9` and xfx's `7` are the `tool-schema` rows below the
page. Across the width change (`resize`): fx, RED and GREEN all keep row 1 at
80 columns; after reopening at 80 and widening to 120 every side follows the
tail.

## Regressions

Every other `p5-*` scenario that renders Ctrl-O (`p5-transcript-cancel`,
`-diffctl`, `-md`, `-md-light`, `-tz`, `-tzsys`), RED against GREEN: every
snap's text is the same after the banner and the clocks: 23 of 23 identical
(`regressions.txt`).

## `docs/parity.md` and the ledger

- `docs/parity.md`, the `full transcript (C-o)` row: the wrap, the dedupe
  rule, the reading position; two new narrower points -- a call's row names
  at most 1024 bytes of its argument and then `…` (`src/tui/toolgroup.rs`
  `TARGET_BYTES`; fx's bound is 120 × 120 bytes,
  `src/core/tooling/tool_presentation.zig:21-22@481104fd`; owner: the tools
  cluster) and the raw-status replay that has nothing to apply to.
- `scripts/fxcap/parity-ledger.toml`: `session-record` matches the tool
  list's second row at any width; `full-transcript-shift` lists the four
  scenarios.

## Gates (at `e21d3aa1`, `TMPDIR=/private/tmp/claude-501/tw`, each full run after the idle wait)

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| `cargo clippy --locked --all-targets --features fault-injection -- -D warnings` | ok |
| `cargo test --locked --no-fail-fast` | 4887 passed, 0 failed, 7 ignored |
| `cargo test --locked --features fault-injection --no-fail-fast` | 4916 passed, 0 failed |
| `scripts/check-xfx-identity.sh`, `check-no-stubs.sh`, `check-no-secrets.sh` (after this receipt) | ok, ok, ok (8883 files) |
| `scripts/fxcap/test-sweep.sh` (the ledger changed) | ok (24 + sweep tests) |
| `cargo build --locked --release`; `--features fault-injection --target-dir target/faulty` | ok; ok |
| `scripts/smoke.sh target/release/xfx` | 47 checks, 0 failures |
| `scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx` | 37 scenarios + the oracle, 1111 checks, 0 failures |
