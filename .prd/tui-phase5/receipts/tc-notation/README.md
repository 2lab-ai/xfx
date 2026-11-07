---
title: 0.0.16 delta -- P5 TC-NOTATION receipt (builtin greys spelt 38;5 at every depth)
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, theme, truecolor]
---

# P5 TC-NOTATION: xfx spells every builtin grey as fx does, at every depth

Gap recorded by T-U3-TC (`../inline-code-truecolor/README.md`, "Divergence:
the notation of xfx's own greys"), filed as `[[cause]] truecolor-grey-notation`
(owner "TC-NOTATION builtin greys spelt 38;5 at every depth"), decided
2026-11-07: follow fx. Oracle: fx v0.0.16 (`481104fd`, executable sha256
`8e6cda2e…`, run with `FX_AUTO_UPGRADE=0` by `capture.sh`, the sweep checking
the sha256 before and after each run). Base: `ec390074`, the head of
TG-REDRAW (`feat/p5-toolgroup-redraw`, not yet on `main`). Branch
`feat/p5-tc-notation`.

**Builds.** The code is two commits: **`89eb5f31`** (the greys, the
deletion of the depth) and **`b8a4f952`** (the output alphabet: no direct
background; the ledger's citations moved with the lines). Then
**`16101f19`** changes comments and doc comments only (three more citations
moved, one fx citation pinned; `git show 16101f19`: no code line). Every head
capture is from a debug build of **`b8a4f952`** made after that commit from a
clean tree (row 1: `x𝒇x v0.1.0-b8a4f95 [debug]`), and a build of
`16101f19` differs from it in its banner alone; every base capture from a
debug build of `ec390074` (`v0.1.0-ec39007`). The gates ran at `16101f19`
(`gates.txt`). The commit after `16101f19` adds this directory and changes no
other file. Each capture is kept as its `capture-pane -e` file (`.ansi`)
only.

## fx's rule (all `@481104fd`)

- **Every builtin grey is a `38;5` literal.** `fx_dark` and `fx_light`
  (`src/core/shared/theme.zig:75-116`, `:118-159`): divider `:79`/`:122`
  (`240`/`250`), hint `:80`/`:123` (`255`/`235`), statusline `:81`/`:124`
  (`245`/`241`), subtitle and tag `:82-83`/`:125-126` (bold `255`/`235`),
  notice label `:84`/`:127` (bold `252`/`238`) and text `:85`/`:128`
  (`250`/`241`), dim `:86`/`:129` (`245`/`247`), warning, green, red
  `:87-89`/`:130-132` (`252`/`238`), the file picker's
  `approval_button_inactive_style` `:97`/`:140` (`48;5;239`+`38;5;255` /
  `48;5;251`+`38;5;237`), `permission_auto_style` `:99`/`:142`
  (`252`/`238`), the highlighter's syntax slots `:108-114`/`:151-157`. The
  only `38;2` in either theme are the diff markers' truecolor variants
  (`:92-93`, `:135-136`).
- **The renderer copies them verbatim, whatever the terminal claims.**
  `applyTheme` (`src/ui/render.zig:69-90`) assigns each slot as it is; the
  truecolor flag (`render.zig:48`, `true` unless set) is read only for the
  diff markers of a *selected* theme (`:92-104`, off for a terminal-following
  builtin) and for the Ultra mark (`:395`, `38;2;255;204;0` or `38;5;220`),
  set by `truecolorSupportedForValues`
  (`src/ui/terminal/theme_protocol.zig:84-93`).
- **A switch rewrites `38;5` bytes only.** The builtin token table
  (`src/ui/transcript/store.zig:3784-3800`) names `38;5` sequences alone;
  `retintEntriesForTheme` (`:3722`, the loop `:3743-3760`) applies it.
- So fx's screen under `COLORTERM=truecolor` is its 256-colour screen
  (`../inline-code-truecolor/fx-tc-vs-256.txt`, and this sweep's fx side:
  `tc-vs-256.txt`, sections `base-fx` and `head-fx`).

## The gap, on the base (`ec390074`)

Under `COLORTERM=truecolor` xfx spelt its palette `38;2;L;L;L`
(`theme::depth_from_env`, `truecolor()`), the same xterm-ramp greys fx
writes `38;5;N`; the file picker's bar `48;2`; the bold heading and brand
`1;38;2`.

- **Unit RED** (`red-unit.txt`; the five `red_tc_notation_*` cases inserted
  into `ec390074`'s src, nothing else changed): four fail --
  - every builtin grey spelt as fx's at both depths: `divider dark at
    TrueColor` gives `38;2;88;88;88` where fx's `38;5;240`;
  - a direct grey is nobody's on any row: `Dark TrueColor Plain 38` gives
    `Some(38;2;238;238;238)` -- the base recognised its own direct greys;
  - a truecolor session writes what a 256-colour session writes (a scripted
    session: card, activity row, inline code, a code block, a tool group,
    a switch, Ctrl-O open, a switch, Ctrl-O closed, a stopped call with a
    notice under it, a switch): tick 0 already differs, `38;2;238;238;238┃`
    against `38;5;255┃`;
  - a stopped call and a notice queued before a switch, read off the band's
    record of the screen as 256-colour indices at both depths: at TrueColor
    the call row, mark, `Cancelled`, argument, label, notice and rail read
    `None` (spelt direct) and only the highlighter's string and comment read
    `241 247`, where the 256-colour session reads all nine,
    `241 238 235 247 241 247 238 241 235` (`STOPPED_LIGHT`: fx's token table
    for the stopped line, the notice's and the rail's light greys).

  The fifth **passes** on the base: an indexed (`38;5`) cell is answered the
  same in a truecolor session as in a 256-colour one, on every row kind
  (`Plain`, `ToolGroup`, `ToolStopped`, `Answer`, `Code`, `Table`), in
  either mode, for all 256 indices. That is the collision search's answer
  (below).
- **Captures RED** (`red-sweep.txt`): the base's sweep captures scored again
  (`sweep.sh --rescore`, no capture taken) with `truecolor-grey-notation`
  deleted: all 34 `-tc` snap pairs gain uncaused rows (0 before) -- the
  rails, the hint row, the turn summary, a tool group's rows, a stopped
  call's line, the activity row, Ctrl-O's headings, session and network
  records and output rail -- the cause's own list. No other pair changes.
- **xfx's truecolor screen is not its 256-colour screen** (`tc-vs-256.txt`,
  section `base-xfx`; `tc_vs_256.py` over the base sweep's captures, each `-tc` snap against its
  256-colour twin's same snap): 0 of 34 byte-identical; every differing row
  differs in its SGR sequences. fx's (section `base-fx`, the same sweep):
  31 of 34 byte-identical, and the other three (Ctrl-O's `full-*` pages) are
  identical once each run of digits is masked -- the entries' clocks.

## Every place that told a palette grey from a highlighter colour by notation

The brief's trap: T-U3-TC's role restriction relied on palette greys being
`38;2` and highlighter colours `38;5`. Read at `ec390074`, the rules a theme
report runs on are `Palette::document_retint` / `document_retint_in`
(`src/tui/theme.rs`), applied cell by cell by `Grid::retint_document`
(`src/tui/grid.rs`) and row by row by `recoloured` (`src/tui/transcript.rs`,
a block painted in the other mode). What read the notation there:

1. `painter_of` (`theme.rs`): in a truecolor session an indexed cell was
   answered as a 256-colour session answers it, a direct one at the direct
   depth. The **role lists did not depend on it** -- the one that did, the
   `ToolGroup` arm for an indexed cell in a truecolor session
   (`[COMMENT, COMMAND, NOTICE]`, T-U3-TCb), TG-REDRAW deleted when it
   replaced colour matching on a tool group's rows with a redraw from source
   (`../toolgroup-redraw/README.md`). The passing fifth RED case above is
   the evidence: an indexed cell is answered alike at both depths, every row
   kind, every index. So the recorded constraint ("TC-NOTATION would bring
   back the collision in truecolor") is discharged by TG-REDRAW having
   landed first.
2. `shade`'s truecolor branch (`theme.rs`): the palette's own greys matched as
   `Rgb` ramp levels -- recognising the palette's direct spelling, not
   telling it from the highlighter's. With every grey `38;5`, nothing
   writes a direct grey; it is deleted, and a direct grey is nobody's
   (`a_direct_grey_is_nobodys_on_any_row`).
3. `recoloured_sequence`'s `38;2` parse (`transcript.rs`): the same
   recognition on a row written recoloured. Deleted; a direct grey is left
   as it is (`a_row_painted_in_the_other_mode_is_written_as_a_report_would_leave_it`).

**With both spelt `38;5`, how each row kind tells them apart** -- the way a
256-colour session always has, and the way fx does:

| row kind | xfx (by the row's recorded kind) | fx (`@481104fd`) |
|---|---|---|
| `Plain` (answer text, notices, summary, card) | roles `NOTICE, BODY, DIM, LABEL, WARNING`: 255<->235, 252<->238, 250<->241, 245<->247 | the builtin token table on the answer's and the theme-owned rows' bytes (`store.zig:3784-3800`, `:3743-3760`): the same four pairs |
| `ToolStopped` (a stopped call's line) | the table's four pairs (`WARNING, HINT, NOTICE, DIM`) | the table, on the `.tool_status` entry's bytes (`:3802-3806`) |
| `ToolGroup` (header, call rows) | drawn again from the group's own drawings (`Redraw`) -- no colour read | projected again in the new theme (`tool_group_projection.zig:558-668,742-784`) |
| `Code` (a code block) | the highlighter's roles `COMMENT, COMMAND, NOTICE` | drawn again in the new theme (`transcript_blocks.zig:2023-2029`) |
| `Answer` (an answered question's answer) | `ANSWER` alone | written in `statusline_style` then in force (`question_ui.zig:481`) |
| `Table` | nothing | kept as styled when added (`store.zig:3758`) |

No rule there reads a notation now: `shade` is an index, and `painter_of` is
gone. The captures agree: every `-tc` pair at the head scores as its
256-colour twin does (Sweep, below).

## What changed in xfx

`89eb5f31`:

- **`src/tui/theme.rs`**: `Palette` is a mode and nothing else. Every
  accessor spells `38;5` (`paint` -> `ansi256`), the bold heading and brand
  `1;38;5`, the picker's bar `48;5;239`/`48;5;251` -- fx's literals. Deleted,
  as nothing read them once the greys stopped using them: `Depth`,
  `depth_from_env` (and its two documented divergences from fx's
  truecolor rule, which no longer had an effect), `truecolor()`,
  `painter_of`; `shade` returns an index. `COLORTERM` still reaches the
  Ultra mark by fx's own rule (`ultra_marker`, `render.zig:395`,
  `theme_protocol.zig:84-93`), unchanged.
- **`src/tui/mod.rs`**: the launch builds the palette from the mode alone.
- **`src/tui/transcript.rs`**: `recoloured_sequence` reads `38;5` alone.
- **`src/tui/shell.rs`**: `retint`'s doc loses "the depth".
- The tests of 26 more files lose `depth: Depth::Ansi256` from their palettes
  (mechanical).

`b8a4f952`:

- **The output alphabet** (BRIEF.md's S bar): the palette's one background
  is `48;5` at every depth, so nothing writes `48;2`. The render allowlist
  (`pacer::is_palette_sgr`, which `colour_at` asks) and the write
  preflight's parser (`check::wire_color`) refuse a direct background; the
  smoke harness's emulator, whose vocabulary is theirs
  (`scripts/smoke-tui.sh`'s `vt_grid`), takes one as a finding. `38;2` stays
  admitted for the Ultra mark's yellow. Every outside string is inert before
  it gets here (`bridge::inert` turns `ESC` into a space), so nothing else
  could carry one. RED: `red-unit-b.txt` (both new assertions fail on
  `89eb5f31`: `colour_at("\e[48;2;78;78;78m")` is `Some(16)`,
  `wire_color("48;2;78;78;78")` is `Ok`).
- `scripts/fxcap/parity-ledger.toml`: the xfx citations into
  `toolgroup.rs` (`:617`, `:656`, `:704`, `:818`) and `shell.rs` (`:37238`)
  moved with the lines `89eb5f31` removed (each names the line it named at
  the base).
- `scripts/smoke-tui.sh`: the oracle's `PALETTE_SGR` holds the two spellings
  the product still writes (`38;5;250`, the Ultra mark's `38;2;255;204;0`).

`16101f19` (comments only):

- `scripts/fxcap/parity-ledger.toml`: `submit-multi-card-whole`'s
  `shell.rs:12192` -> `:12189` (the line it named at the base).
- `scripts/smoke-tui.sh`: its `theme.rs:66`, `:104`, `:108-126` -> `:67`,
  `:105`, `:109-127` (one doc line was added above them). Every
  `src/...rs:N` citation in `docs/`, `scripts/` and `src/` into a file this
  unit changed now names the line it named at the base (the audit compared
  the base's line text with the head's for each), except
  `docs/superpowers/plans/2026-09-11-tui-approval-amendment.md`'s
  `src/tui/theme.rs:333`, a historical plan whose `:333` already named
  another line at the base, left as it is. `shell.rs:12192` named a blank
  line at the base (the line before `send`'s doc): moved with it, not
  repaired.
- `theme.rs` (`ultra_marker`'s doc) and `docs/parity.md`: fx's
  `truecolorSupportedForValues` is `theme_protocol.zig:84-93@481104fd` (the
  doc said `:83-92`, a line off at both pins).

**The ledger.** `[[cause]] truecolor-grey-notation` deleted, with the comment
above `ctrl-o-diff-row-clipped` that ordered it after `cancelled-marker`.
**`docs/parity.md:241`**: the band "spells every one of its greys `38;5;N`
whatever the terminal claims"; the inline-code sentence and the divergence
paragraph replaced by what is true now.

## The minimal stage (`green-minimal-unit.txt`, `minimal.diff`)

Before deleting anything, `theme.rs` alone was changed to spell `38;5` at
both depths (`minimal.diff`: `theme.rs` alone, its three RED cases included;
the two event-loop RED cases sat unchanged in `event_loop.rs`). The five
`red_tc_notation_*` cases passed; nine others failed -- every one a case that
pinned the direct notation (`38;2` sequences, a direct grey matched and
repainted direct, a direct-notation summary): `every_shade_is_spelled_the_same_in_both_notations`,
`a_direct_colour_session_matches_and_repaints_in_the_same_notation`,
`a_code_blocks_row_answers_its_colours_as_the_highlighters` (its truecolor
tail), `a_stopped_calls_line_moves_through_upstreams_token_table` (its
truecolor tail), `an_outcome_notices_label_is_retinted_like_the_rest` (its
truecolor tail), `the_file_pickers_marked_row_is_a_grey_bar_behind_the_hint_grey`,
`a_row_painted_in_the_other_mode_is_written_as_a_report_would_leave_it`
(its direct half), `a_terminal_that_takes_direct_colour_is_given_the_same_rows`,
`a_truecolor_sessions_answer_code_recolours_in_the_notation_it_was_drawn_in`.

## Tests at the head

- **Rewritten to pin fx's spelling**: `tui::theme` --
  `every_builtin_grey_is_spelt_as_upstreams` (each accessor, both modes,
  fx's literal with its `theme.zig` line), `every_shade_is_upstreams_index`,
  `a_direct_grey_is_nobodys_on_any_row` (every ramp level, every row kind,
  both modes), `the_file_pickers_marked_row_is_a_grey_bar_behind_the_hint_grey`
  (`48;5` only), `a_code_and_stopped_rows_recolour_their_highlighter_greys_as_fx`,
  `a_tool_groups_rows_answer_no_colour`, `a_stopped_calls_line_moves_through_upstreams_token_table`
  and `an_outcome_notices_label_is_retinted_like_the_rest` without their
  direct tails; `tui::event_loop` --
  `an_answers_code_and_its_summary_are_spelt_as_upstream_spells_them_across_a_switch`
  and `a_tool_groups_rows_are_spelt_as_upstream_spells_them_across_a_switch`
  (each grey read **as spelt** off the bytes a tick writes, `fg_spelt`, so a
  direct spelling of the same grey fails them: the summary `38;5;245` ->
  `38;5;247` -> `38;5;245`; the statusline, string and comment of a tool row
  `245 250 245` -> `241 241 243` -> back); `tui::transcript` -- the
  direct half of `a_row_painted_in_the_other_mode_…` now says a direct grey
  is left as it is, and the recolour-agreement case runs over the one
  notation (`2 * 19 * 6 * 2` checks); `tui::toolgroup` -- the drawings and
  the kept colourings without a depth; `tui::pacer`, `tui::check` -- the
  direct background refused (round b).
- **Deleted** (each pinned the depth itself, which no longer exists):
  `truecolor_is_gated_on_colorterm_and_apple_terminal_is_downgraded`
  (`depth_from_env`), `a_background_that_changed_is_not_a_terminal_that_changed`
  (a report kept the depth), `a_direct_colour_session_matches_and_repaints_in_the_same_notation`,
  `a_terminal_that_takes_direct_colour_is_given_the_same_rows`, and the
  RED-only `red_tc_notation_*` session comparisons (with no depth there is
  no second session to compare: the launch no longer reads `COLORTERM` for
  the palette at all, so the claim is structural; the captures carry it,
  `tc-vs-256.txt`, section `head-xfx`).

## Sweep

`scripts/fxcap/sweep.sh --fx <fx v0.0.16> --pin-sha 8e6cda2e… --xfx <build>
--jobs 2 --only 'p5-inline-code-*' --only 'p5-toolgroup-*' --only 'p5-toolstop-*'
--only 'p5-transcript-*'` at 40x120: 42 scenarios, 169 snap pairs, the ten
`-tc` scenarios (34 pairs) and their 256-colour twins among them, so both
depths are in one run. fx's sha256 was checked before and after each run.

| sweep (scored with) | pass | identical | normalized-identical | approved-divergence | unresolved | pairs with uncaused rows |
|---|---|---|---|---|---|---|
| base `ec390074`, base ledger (`sweep-base.md`) | 63 / 169 | 10 | 53 | 2 | 104 | 13 |
| base `ec390074`, head ledger (`sweep-base-rescored.md`, `--rescore`) | 63 / 169 | 10 | 53 | 2 | 104 | 47 |
| **head `b8a4f952`, head ledger** (`sweep-head.md`) | **88 / 169** | 10 | 78 | 2 | 79 | 13 |

The head ledger is the base's with `truecolor-grey-notation` deleted and
the five xfx citations moved; nothing else.

Pair by pair, base against head (`sweep-compare.txt`, `cmp_sweeps.py`):
**34 pairs change, 0 drop, every one a `-tc` pair**:

- **25 pass on the head and not on the base** (`unresolved-defect` ->
  `normalized-identical`): all of `p5-inline-code-retint-tc`, `-light-tc`,
  `p5-inline-code-table-tc`, `-light-tc` (12), `p5-toolgroup-retint-tc`,
  `-light-tc` (6), `p5-toolgroup-live-tc` (4, its `running-*` snaps through
  the sweep's `activity-pulse` normalization as their 256-colour twins), and
  `p5-toolgroup-ctrlo-tc` `dark`, `light`, `dark-again` (3).
- **9 stay unresolved, each now with exactly its 256-colour twin's causes**:
  `p5-toolstop-retint-tc`, `-light-tc` (6) `cancelled-marker` alone (T-U4's
  `▪`/`■`, row 8); `p5-toolgroup-ctrlo-tc` `full-*` (3) `session-record`,
  `tool-schema`, `toolgroup-ctrlo-record-short` -- the twin's three (the
  base sweep's own verdicts for the twins, `sweep-base.md`).
- **No pair has an uncaused row it did not have at the base**: the 13 with
  uncaused rows on both sides are `p5-transcript-cancel`, `-md`,
  `-md-light`, `-question`, row for row the same at the base (TG-REDRAW's
  receipt names the same 13).

Read with T-U3-TC's `../inline-code-truecolor/notation.py` (`notation.txt`):
at the base every `-tc` pair differed from fx in 23 to 152 cells by spelling
alone; at the head **0 cells in every pair**, and `kept: 0` in both. `ramp` at
the head is 0 in 24 pairs; in the other ten it is the row the sweep's causes
and normalizations already name: 6 toolstop pairs, row 8 (`▪`/`■` and
`fx`/`xfx` in the follow-up question, `cancelled-marker`);
`p5-toolgroup-live-tc` `running-dark`, the activity row's clock (`Running
(1s)` / `(2s)`); the three Ctrl-O `full-*` pages, the record rows above.

**xfx's truecolor screen is its 256-colour screen** (`tc-vs-256.txt`, each
`-tc` snap against its twin's same snap, same side, same sweep):

| | fx base sweep | xfx base `ec390074` | fx head sweep | **xfx head `b8a4f952`** |
|---|---|---|---|---|
| byte-identical | 31 / 34 | **0 / 34** | 31 / 34 | **28 / 34** |
| identical once digits are masked | +3 | +0 | +3 | +6 |
| rows differing in their SGR sequences | 0 | every differing row | 0 | **0** |

The rows that differ only in digits: on both products, Ctrl-O's entry
clocks on the three `full-*` pages; on xfx's side also the turn summary's
wall time on `p5-toolgroup-retint-light-tc` (`0s` / `1s`, three snaps of one
run). Not a colour in any of them.

Captures kept here: `fx/` and `xfx/`, the head sweep's 34 `-tc` snaps of each
side (banner `v0.1.0-b8a4f95` on every `xfx/` file); `red/`, the base
sweep's 34 `-tc` snaps of xfx (`v0.1.0-ec39007`). The 256-colour twins'
captures are not committed (`check-no-secrets.sh` near ARG_MAX on this base);
`tc-vs-256.txt` prints every row that differs between a pair, both sides.

## Mutants (`mutants.txt`, `mutate.py`)

On a `git clone --no-local` of `b8a4f952` in the scratchpad, never the
worktree, each mutant from a clean checkout of the clone, `cargo test
--locked --lib --no-fail-fast`. Unmutated: `4265 passed; 0 failed`. All nine
killed:

| mutant | what it does | fails | killed by (among them) |
|---|---|---|---|
| A | the divider's dark 240 spelt `38;2` again | 2 | `every_builtin_grey_is_spelt_as_upstreams`, `every_shade_is_upstreams_index` |
| B | the statusline/dim 245 spelt `38;2` again | 86 | the two above, the event-loop and toolgroup spellings, and every case that reads a summary or a tool row's grey |
| C | the bold notice heading spelt `1;38;2` on dark | 4 | `every_builtin_grey_is_spelt_as_upstreams`, `an_outcome_notices_label_is_retinted_like_the_rest`, `tui::notice`, `tui::shell` label cases |
| D | the welcome brand spelt `1;38;2` on light | 1 | `every_builtin_grey_is_spelt_as_upstreams` |
| E | the file picker's bar spelt `48;2` on dark | 5 | `every_builtin_grey_is_spelt_as_upstreams`, `the_file_pickers_marked_row_is_a_grey_bar_behind_the_hint_grey` and three `tui::file_picker` cases |
| F | a direct ramp grey read as its index again (the base's truecolor reading) | 1 | `a_direct_grey_is_nobodys_on_any_row` |
| G | a row written recoloured reads a direct ramp grey as its index again | 1 | `a_row_painted_in_the_other_mode_is_written_as_a_report_would_leave_it` |
| H | the render allowlist admits a direct background | 1 | `tui::pacer::tests::a_row_may_carry_only_the_shapes_the_palette_paints_in` |
| I | the wire parser admits a direct background | 1 | `tui::check::tests::a_colour_out_of_range_is_refused_by_both_parsers` |

The counts exclude `tui::copy::tests::an_ending_session_lets_a_copy_that_answers_land`,
which also failed in D, F and I: it touches no palette (a fake clipboard
tool that sleeps; the copy must return within 500 ms, `src/tui/copy.rs:239-260`),
passed unmutated, and passed three times alone with D applied again -- load
timing (`mutants.txt`, its last lines), not a kill.

## Gates (`gates.txt`)

At the code head `16101f19`, the suites one at a time, load 15-38:

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| `cargo clippy --locked --all-targets --features fault-injection -- -D warnings` | ok |
| `cargo test --locked --no-fail-fast` | 5176 passed, 0 failed (lib 4265) |
| `cargo test --locked --features fault-injection --no-fail-fast` | 5213 passed, 0 failed (lib 4288) |
| `scripts/check-xfx-identity.sh` / `check-no-stubs.sh` / `check-preview-contract.sh` | ok / ok / ok |
| `scripts/check-no-secrets.sh` (this directory staged) | ok, 12348 tracked files (12347 before `gates.txt` was added; both runs in `gates.txt` and above) |
| `scripts/check-tui-preflight-cost.sh` (CI's) | ok |
| `scripts/fxcap/test-sweep.sh` (the ledger changed) | 50 + 26 tests ok |
| `scripts/fxcap/test-capture.sh` | 42 tests ok |
| `cargo build --locked --release`; `--features fault-injection --target-dir target/faulty` | ok; ok |
| `scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx <dir>` | 37 scenarios + the oracle, 1111 checks, 0 failures (its emulator's vocabulary narrowed in `b8a4f952`: `an_undeclared_sgr_parameter_is_a_finding` and `reverse_video_and_a_background_are_accepted_and_kept_apart` among the oracle's checks) |

**This directory and `check-no-secrets.sh`.** 119 files are added here (102
captures, the rest text). The checker passes `git ls-files` names to `grep`
as arguments: with this directory staged that is 12348 names, 921,643 bytes
NUL-separated, plus 8 bytes of pointer each -- about 1,020 KB against macOS's
`ARG_MAX` of 1,048,576, so about 28 KB of room on this machine's 2.3 KB
environment. Only the `.ansi` of each capture is kept, and the 256-colour
twins' captures are not; the separate fix (SECRETS-ARGMAX) is what makes the
room stop mattering.
