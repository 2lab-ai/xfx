---
title: 0.0.16 delta -- P5 T-U4 receipt (the cancelled marker, the group-summary plural table, the Ctrl-O heading bound)
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, transcript, toolgroup, cancel]
---

# P5 T-U4: fx v0.0.16's cancelled marker, its plural table, and a heading cut fx does not make

Spec: the research branch's `.prd/tui-phase5/research/transcript.md` §T4-§T6
and its U4 row, `input.md` I10 and U7, `ssot.md:34,68` (T-U4 owns the
marker everywhere). Oracle: fx v0.0.16 (`481104fd`, executable sha256
`8e6cda2e…`, checked before and after every run by `sweep.py` and
`capture.sh`, run with `FX_AUTO_UPGRADE=0`). Base: `e6cde019` (train 3,
PR #79). Branch `feat/p5-cancel-marker`.

**Builds.** The base's debug build (`v0.1.0-e6cde01`, sha256 `3345f6b8…`)
and the head's, **`95669160`** (`v0.1.0-9566916`, sha256 `0acddda1…`),
each built after its commit. Every xfx capture here is one of the two.
The commits after `95669160` change only the ledger (`5a7c80dd`, causes,
which approve nothing) and this directory.

| commit | what |
|---|---|
| `6b3bd6af` | the marker (`theme::CANCELLED_MARK`) at its three writers; every test, doc, smoke and parity line; the plural table; the heading-bound test |
| `5584ae96` | ledger: the two marker causes deleted, three refs re-pointed; comments |
| `0aa9a299` | a read's, list's, pattern's or folder's argument whole, as fx's (item 4) |
| `0aaff75f` | test only: `edit_file`'s 4 096-byte bound pinned (a surviving mutant) |
| `95669160` | a live cancel's line on Ctrl-O and in `/trace` (found by deleting the cause); `p5-transcript-cancel` gets xfx's own turns |
| `5a7c80dd` | ledger: four causes for rows the deleted cause had been standing over |

Line numbers in xfx's source below are the branch head's (`3857d983`, after
the merge of `origin/main`), unless a commit is named.

## fx's rules (all `@481104fd`)

Each read with `git -C fx-upstream show 481104fd:<path>` by this worker; the
four opus could not reach (round 1) -- the plural table at
`tool_group_projection.zig:520-545`, `formatExpandedChild`'s "Expanded rows
are never clipped" at `:786-787`, `presentationLabelValue` at
`tool_dispatch.zig:626-647` and the timeline's `.raw_bytes => "raw"` at
`app_commands.zig:2682` -- verified at the pin by the dispatcher too.

- **The marker is U+25AA, the colours as before.** `lifecycleMarker(.cancelled)`
  (`src/ui/transcript/runtime.zig:613-618`: `warning_style`, `"▪"`) writes a
  cancelled call's own line and the turn's, with the follow-up
  (`:645`, `" · What can fx do differently?"`); a refused question batch is
  `writeCancelledResolution` (`src/ui/footer/question_ui.zig:513-519`:
  `red_style`, `"▪"`, ` Cancelled`). c1db919 wrote U+25A0 (c29d5bfc).
- **fx still reads U+25A0** because it replays a session's own rendered
  rows: `normalizeCanonicalStatus`
  (`src/ui/transcript/tool_group_projection.zig:323-324`) and the connected
  row (`src/ui/footer/render_input.zig:732-733`), both commented "the
  cancelled marker older sessions recorded before `▪`".
- **The plural table.** `formatGroupSummary`
  (`tool_group_projection.zig:520-545`): a count of 1 takes the label as
  written; otherwise `commands`, `subagents`, `MCP calls`
  (`:539-544`), every other label as written. Labels `:143-153`; upstream's
  own case `group summary uses singular and plural subagent labels`
  (`:1509-1524`).
- **MCP (§T5).** An unregistered `mcp_` name is `.mcp`
  (`src/core/tooling/tool_dispatch.zig:574-578`), counted as `MCP call`
  (index 8, `tool_group_projection.zig:154-156`, `:186-191`). The activity
  verb `calling` (`src/core/output/activity_status.zig:102`) and the part
  `N MCP call(s) made` (`:244`) never reach the screen: the footer clears the
  kind before it builds the row (`render_input.zig:692-693`,
  `visible_stream.last_activity_kind = null`), and the one label built with it
  is discarded (`src/core/app/app_worker_runtime.zig:856`,
  `_ = activityShimmerLabel(...)`, read only to decide whether to animate).
- **The Ctrl-O heading** is never clipped (`formatExpandedChild`,
  `tool_group_projection.zig:786-787`); only its phrase is bounded: a command
  at 14 399 bytes (`formatRunCommandDetailBounded`,
  `src/core/tooling/tool_presentation.zig:20-22`, `:432-447`), a
  `write_file`/`edit_file` path at 4 096 (`fileMutationDisplayPath`,
  `src/core/agent/runtime/tool_presentation.zig:736-753`,
  `diff.max_encoded_label_bytes`, `src/core/output/diff.zig:10`; the two
  tools are `file_mutation_contract.isToolName`,
  `src/core/tooling/file_mutation_contract.zig:487-490`). Every other
  argument is the model's value at any length (`formatToolAction`,
  `src/core/app/app_agent_runtime.zig:1248-1310`; `presentationLabelValue`,
  `src/core/tooling/tool_dispatch.zig:626-647`).

## 1. The marker, at every site

`src/tui/theme.rs:198` `CANCELLED_MARK = "\u{25aa}"`, used by the three
writers and nothing else (`writers.txt`): the turn's line
(`src/tui/shell.rs:155`), a cancelled call's own line
(`src/tui/toolgroup.rs:800`), a refused question batch
(`src/tui/question.rs:1027`). The colours are untouched: the tests assert
`38;5;252`/`38;5;255` dark and `38;5;238`/`38;5;235` light around the new
glyph (`shell.rs` `the_cancelled_line_is_upstreams_marker_word_and_question_in_upstreams_greys`,
`question.rs` `a_refused_batch_leaves_one_row`). The child row
`└ Cancelled …` is unchanged, as on fx.

**Ctrl-O: the turn's line was missing, not only its glyph** (`95669160`).
With the cause deleted and `p5-transcript-cancel` giving xfx its own turns,
the `full` pair showed fx's Ctrl-O holding the turn's line under the prompt
it stopped (`▪ Cancelled · What can fx do differently?`, no heading,
`fx/fx-p5-transcript-cancel-40x120-full.ansi` rows 15-17) and xfx's holding
nothing there (now rows 17-19 of `xfx/`'s, two lower: the page-edge rows): fx appends the lifecycle line to
the transcript Ctrl-O lays out (`writeTurnCancellation`,
`src/ui/transcript/runtime.zig:9951-9966`), and xfx wrote it to the
document alone -- a resumed turn's reached Ctrl-O (`Shell::hydrate`'s
`Mark::Block`, `src/tui/shell.rs:3850-3853`), a live one's did not. The
deleted cause had been taking fx's row. `Shell::write_cancelled`
(`src/tui/shell.rs:11998-12016`) now records the line as a block, once
(`a_turn_stopped_over_its_answer_leaves_its_cancelled_line_on_ctrl_o_too`;
RED on the base, `red-unit.txt`: the three rows under the card blank).
A cancel that stopped a running call writes no turn line on either
product -- the call's own line is the turn's -- and fx's Ctrl-O shows only
the group's `└ Cancelled …` child, as xfx's. `/trace`'s timeline now names
a block `raw` (`src/tui/full_transcript.rs`, `traced`), fx's word for both
raw entries a block holds (`.question_resolution`, `.turn_cancellation`;
`writeTranscriptTimeline`, `src/core/app/app_commands.zig:2681-2689`),
where it said xfx's own `questions`.

**Every U+25A0 at the base, and what happened to it: `sites.txt`** (the
base's `git grep -i -E '■|25a0'` over src, tests, scripts, docs and
.github, one disposition per line; then the same grep on this branch). At
the base: 3 writers (changed), 19 doc comments (changed; the two quoting a
c1db919 capture reworded to say "the call's own `Cancelled …` line"), 44
test lines (31 expectations, 13 comments; changed), 8 `smoke-tui.sh` lines,
3 `docs/parity.md` rows (7 glyphs), 1 scenario comment
(`interrupt-question.cap`), and in the ledger the two causes and their
comment (deleted). Kept -- 3 sites over 4 lines -- each said why in `sites.txt`:
`src/markdown/unicode_display_data.rs:563` (`0x2025A0` is a packed word of
`RGI_DATA`, not the character), `p5-transcript-cancel.cap:4` (names the
change, `U+25A0 -> U+25AA`), and `truecolor-grey-notation`'s
`^[▪■] Cancelled ` selector (ledger lines 537-538, TC-NOTATION's). On this branch the
grep finds only those, and `theme.rs:189,196`, the constant's doc naming
c1db919's U+25A0 and fx's legacy reader.

## 2. Older sessions: xfx keeps no rendered row it reads back

**Decision: nothing to accept, so no `■` reader.** Evidence:

- A session log records how a call and a turn ended, not their lines:
  `ToolEnding` (`src/session/event.rs:81-96`, `Cancelled` at `:93`) and
  `TerminalReason` (`:188-191`, `Cancelled` at `:190`). There is no question
  event in the log.
- A resume draws the line again from that record: `hydration.rs:270-273`
  maps `terminal_reason: cancelled` to `Ending::Cancelled`, and
  `Shell::hydrate` writes `cancelled_line(self.palette)` for it
  (`src/tui/shell.rs:3850-3853`). A call the stop cancelled is not replayed
  (`src/tui/hydration.rs:234-236`), as fx's history keeps no result for it.
- A theme report recolours a row by the kind its writer recorded
  (`DocumentRow::ToolStopped`, `grid.rs` `set_kind`), never by its glyph; and
  no code outside a test names the mark but its writers (`writers.txt`).
- **Probe (`old-session/`).** `oldsess.cap` through capture.sh with
  `XFX_BIN` = `two-builds.sh`, which runs the base build on the first launch
  and the head build on the relaunch (`launches.log`: `launch 1:
  xfx-base-e6cde019`, `launch 2+: xfx-head-95669160 --resume last`). The
  base build stops a command and its turn with Ctrl-C
  (`xfx-oldsess-40x120-base-stopped.ansi`: `■ Cancelled sleep 6 … · What can
  xfx do differently?`) and quits; the store it leaves holds no U+25A0, only
  `"ending":{"kind":"cancelled"}` and `"terminal_reason":"cancelled"`
  (`xfx-oldsess-40x120-base-store.files`). The head build resumes it:
  `▪ Cancelled · What can xfx do differently?` in `38;5;252`/`38;5;255`
  (`xfx-oldsess-40x120-head-resumed.ansi`) -- the turn's line; the call's
  own line is not replayed, by the rule above -- and its Ctrl-O shows the
  same line under the prompt (`xfx-oldsess-40x120-head-resumed-ctrl-o.ansi`).
- The probe waits on fixed `wait` lines: capture.sh has no state wait other
  than `held` (a fake-gateway hold), and the probe is this receipt's only, not
  a sweep scenario (`scripts/fxcap/scenarios/` gains nothing); nothing in it
  is timed against a clock the products run.
- The unit test `a_stopped_turn_replays_as_upstreams_cancellation_and_a_failed_one_as_failed`
  (`src/tui/shell.rs`) replays a logged `ToolEnding::Cancelled` and asserts
  the `▪` line, painted.

## 3. The plural table, and MCP

`src/tui/toolgroup.rs` `category_word` (`:739-761`): `PLURALS` =
`command`/`commands`, `subagent`/`subagents`, `MCP call`/`MCP calls`; a
count of 1 the label, any other label as written. `subagent` is in the
table for a tool xfx does not have (`docs/parity.md:172`, deferred): no xfx
call is counted as one, and the category is not invented. Pinned by
`a_kinds_word_for_more_than_one_call_is_upstreams_table` (every one of fx's
nine labels at 1, 2, 3 and 12, and upstream's own case).

**MCP: already fx's on the base, nothing missing.** MCP-T counts a call to a
server's own tool as `MCP call` (`toolgroup.rs:248` `calls_mcp`, `:161` `MCP_CALL`,
after `command` in a tie), `N MCP calls` past one; the sweep's `p5-mcp-*`
pairs show it on both sides (Sweep). The activity row's `calling` / `made`
is not on fx's screen (above), and xfx's says `Running` under a running MCP
call (`src/tui/activity.rs:68-78`, `fx-p5-mcp-live-40x120-live`).

## 4. The Ctrl-O heading: one cut fx does not make, removed

Confirmed on this base with a test,
`a_heading_is_cut_only_where_fxs_own_bound_on_its_phrase_cuts_it`
(`src/tui/full_transcript.rs`): the 1 KB cut T-U1+U2's review found is gone
(CALLROW-SAFE), and a command (14 399 / 14 400 bytes) and a written or
edited path (4 096 / 4 097) are cut exactly where fx's are. **But a read's,
a list's, a pattern's or a new folder's argument was still cut at 4 096
bytes, where fx shows it whole** (`red-unit.txt`, the bound section: a
20 000-byte read path's heading was 4 105 bytes ending `...`). That cut was
named only in the prose of `call-row-raw-argument` (whose selectors cover
the escape-bearing rows of `p5-call-row-safe`, not a cut row), and no
safety reason needs it: the spelling, which is the divergence, keeps every
character inert at any length.

`0aa9a299`: `toolgroup::spelled` (`:843` at the merged head) takes the call's presentation: a
command as before, `write_file`/`edit_file` at `TARGET_BYTES`, anything else
spelled whole. The test pins a 20 000-byte read path, glob pattern and
folder path whole; `a_long_argument_is_bounded_after_it_is_spelled_and_never_inside_an_escape`
(`toolgroup.rs`) a 5 000-character read path whole and a written one cut.
`docs/parity.md:238` and `call-row-raw-argument`'s reason say so.

**What still bounds such an argument.** fx has no bound of its own on it;
xfx's is the one on the answer it came in: `MAX_COMPLETION_BYTES`, 8 MiB of
text and streamed tool arguments together, in both stream decoders
(`src/gateway/sse.rs:47`, enforced `:207`; `src/llmux/sse.rs:44`, `:447`).
The transcript's own row is still cut at the screen's width by cells
(`src/tui/transcript.rs:1470` `ellipsized`), as fx clips it; only Ctrl-O's
heading, which fx never clips, shows it whole. Every character stays spelled
(`Spelling::push`, `terminal_safe`), so the length is the only thing that
changed. No scenario's turns carry a read, list, pattern or folder argument
past 4 096 bytes (every `scripts/fxcap/scenarios/*.json` tool input
checked), so no sweep pair can move for this commit; the tests are its
receipt.

## Ledger (`scripts/fxcap/parity-ledger.toml`)

- Deleted: `[[cause]] cancelled-marker` (owner `T-U4 cancelled marker`,
  every scenario) and `[[cause]] U7-cancelled-glyph` (the input cluster's
  name for the same marker, `p5-mcp-live`; the SSOT folds U7 into T-U4), and
  the comment on their order before `truecolor-grey-notation`, whose
  selector already takes `[▪■]` and is left as it was.
- `call-row-raw-argument`'s reason: the bound sentence (item 4).
- Re-pointed refs, the same code at its new line: `src/tui/toolgroup.rs`
  750 → 775 (`spelled`; `call-row-raw-argument`, `call-row-page-edge`),
  798 → 830 (`command_spelled`; `call-row-source-cut`), 912 → 944
  (U+061C; `call-row-arabic-letter-mark`). No other ledger ref moved.
- Added (`5a7c80dd`), each a `[[cause]]` (approves nothing) for rows the
  deleted cause had been standing over, found by the sweep:
  `retry-budget-lifecycle` (unit, K-recover: `p5-lifecycle-retry`
  `r01`-`r05`, fx still retrying when Escape stops it, xfx failed after its
  three attempts, `src/gateway/mod.rs:59`); `question-cap-four` (unit, I-U6:
  `p5-input-question` `q6*` rows 9-17, fx's 20-question batch, xfx's 4,
  `src/tools/question.rs:25`); `cancel-page-edge` (defect, follows
  `tool-schema` and `session-record`: `p5-transcript-cancel` `full`, rows
  1-2 of the Ctrl-O page); `amendment-card-after-stop` (defect: `approvals`
  `after-esc*`, xfx's blank row under the amendment card -- already uncaused
  at the base, `sweep-base.md`).

## RED and GREEN, unit

- **RED** (`red-unit.txt`). 6b3bd6af's tests on the base's writers and
  plural rule: **25 failed** (the U+25A0 rows, `subagent x2`), 2697 passed;
  the two pty tests in `tests/tui.rs` failed too. The bound tests of
  0aa9a299 and the Ctrl-O test of 95669160, each pasted into a clean clone
  of the base: all three failed.
- **GREEN** (`green-unit.txt`, and the gates): the same runs pass.

## Mutants (`mutants.txt`, `mutants.py`)

On a `git clone --no-local` of the branch, never the worktree; the clone
clean after. Each reverts or half-reverts one rule and runs the TUI's lib
tests (`cargo test --locked --lib tui::`). Round 1, at `95669160`: 14 of 14
killed (`edit_file`'s mutant survived at `0aa9a299`, where only
`write_file`'s cut was pinned; `0aaff75f` pins it). **`mutants.txt` is the
round-2 run, at `3857d983`: 20 mutants, 19 killed, 1 equivalent**:

| mutant | failed tests |
|---|---|
| the mark itself back to U+25A0 (`theme.rs`) | 25 |
| a refused question batch's row writes U+25A0 (`question.rs`) | 2 |
| a cancelled call's own line writes U+25A0 (`toolgroup.rs`) | 11 |
| a cancelled turn's line writes U+25A0 (`shell.rs`) | 12 |
| the table's `subagent` keeps its singular | 1 |
| the table's `MCP call` keeps its singular | 2 |
| the table's `command` keeps its singular | 6 |
| a count of 1 takes the plural too | 22 |
| every label takes a plural past 1 | 9 |
| every argument but a command at a file change's bound (the base's rule) | 3 |
| a file change's path unbounded | 2 |
| `edit_file`'s path unbounded | 1 |
| a live cancel's line not recorded for Ctrl-O (the base's rule) | 2 |
| `/trace` names a block `questions` again | 1 |
| round 2: the transcript's row keeps the whole argument (no inline cut) | 1 |
| round 2: the inline cut by bytes only (no column bound) | 1 |
| round 2: Ctrl-O's heading reads the transcript's cut argument | 1 |
| round 2: the inline cut at a screen's width, 512 columns | 2 |
| round 2: Ctrl-O lays a block out in the palette it was written in (round 1's rule) | 1 |
| round 2: Ctrl-O recolours a block in its own mode too (no mode guard) | 0, **equivalent** |

**The equivalent one.** Recolouring a block in the mode it was painted in
gives the same bytes, so the guard (`if *drawn_in == palette.mode`,
`full_transcript.rs:1790`) saves a scan and changes no row -- fx's own
`retintEntriesForTheme` returns the same way when `from.light == to.light`
(`store.zig:3728@481104fd`). Shown three ways:

1. *The kinds' roles.* `document_retint_in` maps a colour to the first
   role of the row's kind holding it in either mode (`theme.rs:614-638`;
   Plain `[NOTICE, BODY, DIM, LABEL, WARNING]` through `document_retint`,
   `:559-579`; ToolStopped `[WARNING, HINT, NOTICE, DIM]`, Answer
   `[ANSWER]`, Code `[COMMENT, COMMAND, NOTICE]`). Over every 38;5 index,
   for each kind and mode, no colour that is one of the kind's roles in
   that mode is taken by a role whose shade in that mode differs: 0 cases.
2. *What a block holds.* Every caller of `FullTranscript::block`, through
   `Mark::Block` or directly (`shell.rs`): the fork line (`:3564`,
   `original session xfx --resume <id>`, no colour); a stopped turn's line
   on resume (`:3850`, ToolStopped) and a live one (`:12012`, Plain), both
   `cancelled_line` (`:153-161`: WARNING 252 / 238, HINT 255 / 235); a
   question batch's answers (`:9192`, `answered_rows`, `question.rs:738-775`:
   the questions as Plain rows with no colour, the answers as Answer rows in
   ANSWER 245 / 241); a refused batch (`:9200`, `cancelled_row`,
   `question.rs:1025-1031`: LABEL 252 / 238). The text in them is spelled
   (`safe_rows`, `tamed`: no escape a terminal obeys). Each of those colours
   in its own mode is its first role's shade in that mode, so
   `document_retint_in` answers `None` and `recoloured` copies the sequence.
3. *What would break it.* In a block painted light, a 245 (TOOL) or a 250
   (DIVIDER) would be repainted in its own mode; in one painted dark,
   nothing. No caller paints `tool_output()` or the divider. If one ever
   did, the fix is that kind's role list, not this guard: the same 245
   would go to 247 after a real switch too.

## Sweep

`scripts/fxcap/sweep.sh --fx <fx v0.0.16> --pin-sha 481104fd… --xfx <bin>
--jobs 2 --out <tmp>` over **every scenario** (313; one, `p5-panels-badsettings`,
fails on fx on both runs, 2 pairs), from a `git clone --no-local` at each
commit. The p5-transcript-cancel scenario changed at `95669160` (xfx's own
turns), so its base side is the base build on this branch's scenario
(`sweep-cancel-base.md`). The head sweep ran under heavy load (31 at the
peak) and five scenarios outlived their deadline (30 pairs, `timeout`);
they were run again under the suite lock (`sweep-head-retimed.md`, all
ran). Every side scored with this branch's ledger (`--rescore`).

| sweep | pass | identical | normalized-identical | approved (+cascade) | unresolved | failed |
|---|---|---|---|---|---|---|
| base `e6cde019`, the base's ledger (`sweep-base.md`) | 1041 / 1692 | 37 | 1004 | 12 + 26 | 611 | 2 |
| base, this branch's ledger, the new cancel scenario | 1042 / 1692 | 37 | 1005 | 12 + 26 | 610 | 2 |
| **head `95669160`**, this branch's ledger, the five retimed | **1099 / 1692** | 39 | 1060 | 12 + 28 | 551 | 2 |

**Pair by pair (`sweep-compare.txt`, `cmp_sweeps.py`): 0 dropped**; 83
changed, 63 of them in the marker set. **The marker set, 77 pairs in 22 scenarios** (`pairs.txt`,
`pairs-base-head.txt`): the 66 pairs in 17 scenarios the deleted causes
covered at the base (the sweep's own attribution, `sweep-base.md`'s
`cancelled-marker` row), and 11 more whose marker row the cause's
`^■ Cancelled ` missed (a question batch's `■ Cancelled` has no space
after it: `interrupt-question`, `interrupt-next-question`,
`question-field`, `question-other`, `p5-input-question` `q2-cancelled` and
`q6*`). At head:

- **57 pass or are approved**: 55 unresolved → normalized-identical, 2 →
  approved cascade (`p5-panels-keys` `approval-esc`, `end`: `ctrl-o-command-marks`
  et al., unrelated rows).
- **20 stay unresolved, each by another cause, none with a row no cause
  covers**: 6 truecolor pairs (`p5-toolstop-retint-tc`, `-light-tc`:
  `truecolor-grey-notation`, TC-NOTATION's 38;2 spelling -- the marker's
  characters match, `green.txt` `chars`); 5 `grant-scope-push` /
  `approval-grant-scope` pairs (`toolgroup` `long-*`, `resume-tools`
  `long-later`) and `resume-tools` `ctrl-o` (`full-transcript-shift`,
  `tool-schema`); `p5-transcript-cancel` `full` (`session-record`,
  `tool-schema`, `cancel-page-edge`); and the four new causes' pairs
  (`approvals` `after-esc*`, `p5-input-question` `q6*`,
  `p5-lifecycle-retry` `r04`, `r05`: 7 pairs).

The 20 other changes are outside the marker set and move with the run, not
the code: `p5-lifecycle-login2` and `p5-panels-sweep-a` (fx's screen moved,
xfx's byte-identical between the runs), `h1-models2` `after` and
`model-column-idle` `col-typed` (a catalog swap and an outage window timed
in milliseconds; both now pass), `p5-transcript-submit-multi` (an elapsed
`0s`/`1s`, normalized-identical → identical) and `workspace-index`
`at-plan` (the run directory's path is one character longer under the head
sweep's TMPDIR, so a path row wraps differently on both sides).

## Captures

`fx/` and `xfx/`: the head sweep's `.ansi` captures of the 77 pairs (fx
v0.0.16; xfx `v0.1.0-9566916`), and `xfx-base/` the base build's
(`v0.1.0-e6cde01`). `marker_cells.py` pairs the k-th marker row of each
side and compares its glyph and `Cancelled` cells with their attributes:

- **`green.txt`, head: 73 of 77 the same** -- 67 `same`, 6 `chars` (the
  truecolor pairs: the characters match, the grey is spelled 38;2) -- and 4
  `DIFFER` only by how many marker rows each side has: `p5-input-question`
  `q6-after*` (fx's second, six-question batch is cancelled; xfx's tool
  refused it: `question-cap-four`) and `p5-lifecycle-retry` `r04`, `r05`
  (fx's turn is still retrying when Escape stops it; xfx's had failed:
  `retry-budget-lifecycle`).
- **`red.txt`, base: 77 of 77 differ** (U+25A0 against U+25AA in the same
  colours; for `p5-transcript-cancel` `full`, the live cancel's line absent
  from xfx's Ctrl-O).

The sweep's own verdicts for the same pairs are `pairs-base-head.txt`.

## Gates (`gates.txt`; round 2's are `round2/gates.txt`, below)

Round 1, at `95669160`, the code's last commit, every step of CI one at a time,
each exit 0: fmt; clippy, default and fault injection; `cargo test --locked`
**5229 passed, 0 failed, 7 ignored**, and with fault injection **5266 / 0 /
7**; check-xfx-identity, check-no-stubs, check-no-secrets,
check-preview-contract, check-tui-preflight-cost, check-usage-clock;
test-check-no-secrets (13 cases), test-gate-env (5), test-preview-contract
(24 mutants caught); fxcap test-sweep (26 tests) and test-capture (42); the
release and faulty builds; smoke (47 checks, 0 failures) and smoke-tui (37
scenarios and the oracle, **1127 checks, 0 failures**). Nothing re-run.
After the ledger-only commit `5a7c80dd`: check-no-stubs ok and fxcap
test-sweep (26 tests) OK. This receipt's commit was staged and checked with
check-no-secrets and check-xfx-identity over the whole tree, 12808 tracked
files (`gates.txt`'s 12550 is the tree before the receipt's captures).

## Round 2 (reviews: gpt6 APPROVE-WITH-FIXES, opus APPROVE)

Commits: `5461923d` (ledger owners), `ab1c67f8` (**RED**, alone:
`round2/red.txt`), `ce574fba` and `5fceb01d` (Ctrl-O recolouring), `547c8e28`
(the row's bound), `3857d983` (merge of `origin/main` 884497c0, #80).
Captures from a debug build of **`3857d983`** (`v0.1.0-3857d98`, sha256
`8d5bd880…`); the base side from `origin/main` 884497c0 (`v0.1.0-884497c`).

**MUST 1, owners.** `cancel-page-edge` is owned with the causes it follows
(`session-record and tool-schema's owners`, closed when both are).
`amendment-card-after-stop` is now unit **AMEND-CARD-ROW** (`kind = ["unit",
"defect"]`, so its source lines stay checked), the evidence in its `why`:
fx's `after-esc` rows 14-15 (`┃ use capitals`, then the composer) against
xfx's 14-16 (a blank row between), xfx's `trailer_rows`
(`src/tui/shell.rs:9665`) and `queue_card` (`:1658`, which ends a card on a
blank row), and two fx screens where the document ends on a cancelled line
with one blank row before the composer (`p5-transcript-cancel` `esc` rows
5-7, `interrupt2` `esc2` rows 36-38).

**MUST 2, the render cost -- measured first** (`round2/`). `render_cost_probe.rs`,
appended to `shell.rs`'s tests on a throwaway clone and run in release, one
case a process, under the suite lock: a live turn's `glob_files` call through
the bridge's `made_inert` and `Shell::apply`, then 40 redraws of the open
group (a short read's start and result, 20 times, each with the frame's
`take_pending`), then Ctrl-O open, cached, and after a call joins the group.

| pattern | round 1 (`bc6c9afc`): start / redraw (median) / Ctrl-O / peak | round 2 (`547c8e28`) |
|---|---|---|
| 9 bytes | 0.2 ms / 0.0 ms / 0.5 ms / 9 MiB | 0.2 / 0.0 / 0.5 ms / 9 MiB |
| 8 MiB of `a` | 450 ms / **452 ms** / 937 ms / 614 MiB | 5.4 / **3.5 ms** / 956 ms / 436 MiB |
| 4 Mi U+009B (32 MiB spelled) | 1 768 ms / **1 749 ms** / 3 455 ms / 1 350 MiB | 7.1 / **3.4 ms** / 3 658 ms / 1 022 MiB |
| 2 Mi U+202E (16 MiB spelled) | 881 ms / **883 ms** / 1 759 ms / 1 162 MiB | 6.7 / **3.5 ms** / 1 832 ms / 518 MiB |

Material: every change to the open group cost up to 1.8 s. So
`ToolGroup::start` keeps, beside the whole argument, the transcript's own
row's (`inline_cut`, `src/tui/toolgroup.rs`): spelled, up to 65 536 columns
-- one past the widest screen there is, so every screen's ellipsis
(`super::transcript`'s `ellipsized`) falls where the whole row's does -- and
256 KiB, cut `...` past either. The transcript's rows, a stopped call's own
line and both modes' redraws read it; Ctrl-O's heading (`expanded`) reads
the whole argument, as fx's expanded row is never clipped. Commands and
write/edit paths keep their own, smaller bounds. Pinned:
`the_transcripts_row_keeps_what_any_screen_can_show_and_ctrl_o_the_whole`
(RED at `ab1c67f8`: a 196 608-byte pattern's row kept 196 619 cells), and
the whole-heading test still holds 20 000-byte arguments whole on Ctrl-O.
**What it leaves:** the bridge spells the whole argument once per call
(34-134 ms), and Ctrl-O lays a multi-MiB heading out whole -- 0.96-3.7 s
on opening and again on each change to the record while it is open (the
layout is cached per revision, not per entry) -- the cost of showing fx's
whole row; no scenario's turns carry an argument past 4 096 bytes.

**(a) Ctrl-O blocks after a theme report.** fx rewrites a question's
resolution and a cancelled turn's line through its token table on a switch
(`retintEntriesForTheme`, `src/ui/transcript/store.zig:3722-3760@481104fd`,
returning when `from.light == to.light`, `:3728`; `themeOwnsRawEntry`
holding `.question_resolution` and `.turn_cancellation`, `:3802-3812`), and
Ctrl-O lays those entries out. xfx's Ctrl-O kept a block in the palette it
was written in. Now `Body::Block` keeps each row's `DocumentRow` and the
mode it was painted in (`Mark::Block` carries it from where the rows were
made), and Ctrl-O recolours a block by the document's rule
(`transcript::recoloured`) only out of that mode. RED at `ab1c67f8`
(`a_theme_report_recolours_ctrl_os_cancelled_lines_as_upstreams_token_table_does`:
after a light report the dark `▪ Cancelled` rows were still on Ctrl-O);
GREEN, and back to dark the rows are byte for byte as drawn.

**(b)** The whole-heading test has `list_files`' case (a 20 000-byte path,
`Listed …` whole).

**(c)** `truecolor-grey-notation`'s `[▪■]`: merged `origin/main` 884497c0
(#80, train 4) at `3857d983`. The ledger conflict was the two deletions
meeting -- main deleted `truecolor-grey-notation`, this branch
`cancelled-marker` -- and both stay deleted, so no ledger line takes `■`. The
call-row refs into `toolgroup.rs` and `cancel-page-edge`'s into `shell.rs`
point at the merged lines (843, 904, 1018; 37424), each checked against the
code its entry names. `p5-transcript-cancel.cap` took main's state-based wait
for the full-access warning (`until shown` / `until gone Full access`) for
both products, since this branch runs xfx in yolo too. After the merge the
grep for `■|25a0` over src, tests, scripts, docs and .github finds only
`theme.rs:189,196` (the constant's doc), `unicode_display_data.rs:563` (a
table word) and the scenario's note.

**Sweep, the touched scenarios** (`round2/`). The 132 scenarios that
exercise what round 2 and the merge touch (the transcript, tool groups and
stops, call rows, MCP turns, questions, interrupts, approvals, Ctrl-O,
resume), `--jobs 2` under the suite lock, from `git clone --no-local`s of
`origin/main` 884497c0 and of `3857d983`, both scored with this branch's
ledger (`--rescore`):

| sweep | pass | identical | normalized-identical | approved (+cascade) | unresolved | failed |
|---|---|---|---|---|---|---|
| base `884497c0` (`sweep-base.md`) | 400 / 651 | 14 | 386 | 4 + 2 | 245 | 0 |
| head `3857d983` (`sweep-head.md`) | 461 / 651 | 14 | 447 | 4 + 2 | 156 | 28 |
| head, `p5-mcp-verbs` run again (`sweep-mcp-verbs.md`) | **462 / 651** | 14 | 448 | 4 + 2 | 183 | **0** |

**Pair by pair (`sweep-compare.txt`): 62 pairs unresolved → normalized-identical**
(the marker rows, in 19 scenarios; the four `p5-toolstop-retint*` among
them, their truecolor grey already main's), 6 lose their uncaused rows and keep their cause
(`approvals` `after-esc*`: AMEND-CARD-ROW; `p5-input-question` `q6*`:
`question-cap-four`; `p5-transcript-cancel` `full`: 13 → 0, its three
causes), and **no pair gains an uncaused row**. The comparison's 28 drops are
all `p5-mcp-verbs`, whose xfx capture failed in the head sweep: its
`until shown 'proj is waiting for approval'` timed out after 29.8 s, with
the screen holding `┃ mcp list` -- the slash of the typed `/mcp list` lost
-- at a load of 8.5. Run again alone at head under the lock, it captured
all 28 pairs and every one has the base's verdict and the base's uncaused
rows, except `show-remote`, which has 4 where the base had 11 (the base's
xfx showed the `remote` server's detail panel, `status retrying`, under the
table; the run's timing). The branch changes no `/mcp` code: its only
`mcp` lines against main are the tool group's plural and the cancel mark.

**Gates (`round2/gates.txt`)**, at `3857d983`, every step of CI one at a
time under the suite lock, each exit 0: fmt; clippy, default and fault
injection; `cargo test --locked` **5284 passed, 0 failed, 7 ignored**, and
with fault injection **5321 / 0 / 7**; check-xfx-identity and
check-no-secrets (13190 tracked files), check-no-stubs,
check-preview-contract, check-tui-preflight-cost, check-usage-clock;
test-check-no-secrets (13 cases), test-gate-env (5), test-preview-contract
(24 mutants caught); fxcap test-sweep (66 and 26 tests) and test-capture
(59); the release and faulty builds; smoke (47 checks, 0 failures) and
smoke-tui (37 scenarios and the oracle, **1127 checks, 0 failures**).
Nothing re-run.

## Files

- `README.md` this file.
- `sites.txt` every U+25A0 at the base and what happened to it; `writers.txt` every non-test line that names the new mark.
- `old-session/` the probe: `oldsess.cap`, `two-builds.sh`, `launches.log`, the base's screen and store, the head's resumed screen.
- `red-unit.txt`, `green-unit.txt` the unit runs.
- `mutants.py`, `mutants.txt`.
- `cmp_sweeps.py` (TG-REDRAW's); `sweep-base.md`, `sweep-base-ledger-head.md`, `sweep-cancel-base.md`, `sweep-head.md`, `sweep-head-retimed.md` (scoreboards); `sweep-compare.txt`; `pairs.txt`, `pairs-base-head.txt`.
- `fx/`, `xfx/`, `xfx-base/` the `.ansi` captures of the 77 marker pairs; `marker_cells.py`; `green.txt`, `red.txt`.
- `gates.txt`.
- `round2/`: `render_cost_probe.rs`, `probe-before.txt`, `probe-after.txt` (MUST 2's measurements); `red.txt` (round 2's RED); `sweep-base.md`, `sweep-head.md`, `sweep-compare.txt`, `sweep-mcp-verbs.md` (the touched scenarios); `gates.txt`.
