# FULLTRANSCRIPT-RESPONSE-HEADER receipt: an answer's blocks split its Ctrl-O record

fx v0.0.16 writes an answer into its full transcript as several entries: a
completed code block, table or thematic rule is an entry of its own with no
header, and the text after one is a new assistant entry under a
`<time> · Response` header of its own. xfx's record headed the whole answer
once, so its tail page stood one row short of fx's (the ledger's
`fulltranscript-response-header` cause: 8 snap labels, 11 pairs, in `p5-modified-page-keys`,
`p5-modified-arrows-full`, `p5-transcript-md` and `p5-transcript-md-light`).
This unit copies fx's split into xfx's record.

| commit | what |
|---|---|
| `87779e3f` | RED: the unit's tests (`src/tui/full_transcript.rs`, `src/tui/shell.rs`) and the oracle scenario `fulltranscript-response-split` (`.cap` and turns); no non-test change |
| `b9007bc3` | **the code head**: `Body::Structure` and `presented` (`src/tui/full_transcript.rs`), `/trace`'s names, the `trace::Entry` doc; no test change |
| `8bd5be58` | `docs/parity.md`, the ledger (the cause deleted, `full-transcript-shift`'s scenarios named, `session-record-page-bottom` added, refs re-pointed), this folder but its mutant, gate and `green.txt` records; no `src/` change. A first gate here failed `check-xfx-identity` on `captures.txt` (Gate, below) |
| `e3e66b0b` | `captures.txt`'s placeholder without the retired name; no `src/` change. **The gate ran here** |
| the commit after | `mutants.txt`, `gate.txt`, `green.txt` and this file's sections for them; no `src/` change |

The code commit is +62/-23 lines in two files, none of them test lines
(`git diff --stat 87779e3f b9007bc3`).

## fx's rule (`fx-src.txt`, written by `fx_src.sh`: every line read with `git show 481104fd:<path>`)

**Which entries an answer becomes, and on which blocks.** The presenter puts
each line together byte by byte and handles it whole
(`pushWithCompletions`, `assistant_presentation.zig:202-220`). A block
completes on a line: a fenced code block on its closing fence
(`assistant_presentation.zig:277-278`), an indented one on its first
unindented line (`:283-284`); a table on its first line that is not a pipe
line (`:301-307`); a rule on a `---` line (`:409-411`); and any still open
at the stream's end (`:254-256`). The stream hands the text before a block
on first (`deliverSemanticCodeBlock` and its siblings,
`assistant_stream.zig:529-545`, the `out.items.len > 0` push at `:534`), and
the worker drains the pacer before it applies a block
(`requiresTextDrain`, `assistant_presentation.zig:45-50`;
`app_worker_runtime.zig:1025`; `workerBridgeDrainAssistantText`,
`app_callbacks.zig:1490-1498`). The store appends each block as an entry of
its own, stamped then (`appendAssistantTableOwned`,
`appendAssistantCodeBlockOwned`, `appendAssistantThematicRule`,
`store.zig:2977`, `:3021`, `:3057`). Text reaches the store from the pacer
(`pacerEmit`, `main.zig:2552-2555`) and joins the tail entry only when that
is an assistant entry (`tailAssistantSegments`, `store.zig:3088-3095`);
otherwise it opens a new one stamped when its chunk lands
(`commitAssistantStreamChunkFast`, `store.zig:3116`, `:3141-3143`; the
staged path, `:3187-3189`).

**Which entries get a header, and with which time.** `beforeEntry`
(`full_transcript_screen.zig:5934-5958`) writes a dim
`  <formatHeader(...)>` before every entry `metadataKind`
(`full_transcript_screen.zig:6028-6056`) has a kind for:
`.assistant_turn => .response` (`:6040`); a table, a code block and a rule
none (`:6051-6054`). The time is the entry's own (`metadataCreatedAtMs`,
`full_transcript_screen.zig:5960-5974`; `createdAtMs`,
`transcript_blocks.zig:400-410`), formatted by `formatHeader`
(`full_transcript_metadata.zig:30-38`). The full presentation skips an entry
that renders nothing before it writes its header
(`renderedBlockHasContent`, `transcript_blocks.zig:2114`, `:2226-2228`),
and an assistant entry's head is trimmed of line breaks first
(`trimAssistantBlockHead`, `transcript_blocks.zig:2004`, `:2844-2848`).

So the run after a block carries **the moment the first text after the block
reached the store**, whatever that text is: commonly the blank line under a
closing fence, which renders nothing but opens the entry the next line joins.
The oracle shows it (`fx/fx-fulltranscript-response-split-40x120-full-block-first.txt`
rows 25 and 32): the answer `"```sh\necho first\n```\n\n"` then
`"Then text.\n"`, its chunks 60 ms apart
(`scripts/fxcap/scenarios/fulltranscript-response-split-turns.json`'s
`delayMs`, slept after each event, `scripts/fxcap/fakegw.ts:661`); the
Prompt is stamped `.821`, the `Response` over `Then text.` `.833`, 12 ms
later: the first chunk's moment, which no later chunk could reach.

**A table and a rule** split the answer exactly as a code block does:
`full-table` rows 21 and 32 (`Before the table.` / `After the table.`),
`full-rule` rows 27 and 32 (`Above the rule.` / `Below the rule.`).

**An answer that ends in a block** has nothing after the block: no header
(`full-block-last` rows 28-36). A trailing line break would open an empty
assistant entry, which has no header; a resumed answer always gets one
(`app_session_runtime.zig:4802`), with the same result.
**An answer that is only a block** has no assistant entry with content, so
no `Response` header at all (`full-block-only` rows 28-36); two blocks with
only a blank line between them, neither (`full-blocks` rows 24-36).

**Streamed chunks** do not move the split: a block completes on the line
that ends it, and lines are put together byte by byte however the chunks
fell. They move only the moments: each entry's is when its first chunk
reached the store (after the pacer), and a table is stamped when the line
after it arrives.

**A resumed answer**: the replay stamps every entry of the answer with the
turn's completion before it writes the answer's markdown (`setCreatedAtMs`,
`app_session_runtime.zig:3997`; the replay's sink, `:4720-4805`;
`ResumeProjection`'s appenders, `resume_projection.zig:76-78`, `:150-193`),
so every run of a resumed answer carries the same moment.

**`/trace`** lists the entries under fx's names (`writeTranscriptTimeline`,
`app_commands.zig:2675-2690`): `assistant`, `assistant table`,
`assistant code block`, `assistant thematic rule`.

## What xfx does now (`git diff 87779e3f b9007bc3 -- src`)

- `Body::Response` is one run of the answer's prose; `Body::Structure` is a
  completed code block, table or rule, an entry of its own, laid out by the
  same `answer` writer and with no header (`content` returns no kind).
- `presented`: prose joins the last entry when that is a prose run; after a
  block it opens a new run **even when it is only line breaks** (fx's store
  opens the entry on the first chunk, and the header takes its moment); after
  anything else line breaks alone still open nothing and leave a tool group
  open, as before (`answer_text_breaks_a_group_and_a_notice_does_not`). A
  block always opens an entry of its own and closes the group, as before.
- The layout needs no change: the screen already writes one blank row
  between two entries and lays out a run that is only line breaks as nothing
  (`measure`, `window`), and `answer` trims an entry's blank rows at both
  ends, so the rows are the single entry's rows plus the headers.
- `traced` names a block entry as fx's timeline does.
- **The inline transcript is not touched**: `super::shell`'s `present` still
  writes the document from the same events; only the record it hands them to
  splits them.

**Decision (reversible): `/trace` follows the record.** It lists a block as
an entry of its own under fx's name, since it reads the same entries and fx's
timeline lists them so; before, the whole answer was one `assistant` entry.

## RED (`red.txt`, `red-pairs.txt`, `red/`)

- The tests at `87779e3f` over the base's code: **7 fail, 1 passes**
  (`red.txt`): the split at each block kind, a block with no header (only a
  block, block first, two blocks), the run's moment, chunking, `/trace`'s
  names, the resumed answer's stamps, and the shell test's Ctrl-O kinds;
  `answer_text_breaks_a_group_and_a_notice_does_not` passes, as it pins what
  the base already did. The shell test fails at its kinds
  (`shell.rs:43158`), past its pin of the inline document
  (`shell.rs:43123`), which holds over the base's code.
- The scenario on the base's binary (`v0.1.0-3f2d6e7`, `red/`): the seven
  inline snaps and `launch` identical to fx's, cell for cell; the six full snaps differ,
  with the moments masked too (`red-pairs.txt`: 8 of 14, 8 of 14 masked) --
  `full-block-only` and `full-block-first` head the block, `full-table`,
  `full-rule` have one `Response` where fx's have two.

## GREEN (`green-pairs.txt`, `xfx/`, `green.txt`)

- The scenario on the code head's binary (`v0.1.0-b9007bc`, `xfx/`): **14 of
  14 pairs identical with each header's moment masked**, the seven inline
  snaps and `launch` identical as captured, and the full snaps' header kinds
  fx's, top to bottom (`green-pairs.txt`). Each side's moments are its own
  run's, which `cells.py` does not mask: `pairs.sh` writes them
  `XXXX-XX-XX XX:XX:XX.XXX` on both sides, as the ledger's
  `transcript-timestamp` normalization does.
- The ledger's four scenarios, in the sweep (below): the cause's 8 snap
  labels, 11 pairs over the four scenarios
  (`p5-modified-page-keys` `full`, `full-ctrl-pgup`, `full2`,
  `full2-kp-ctrl-pgdn`; `p5-modified-arrows-full` `full`, `full-super-left`,
  `full-ctrl-up`, `full-ctrl-b`, `full-ctrl-f`; `p5-transcript-md` and
  `-light` `full`) go from 2 uncaused rows each, fx's second header and
  xfx's blank first row, to **normalized-identical**, and this unit's six
  full snaps from 2 or 4 to normalized-identical (`sweep-compare.txt`).
- `green.txt`: the unit's tests in the gate's run.

## Ledger

- **`fulltranscript-response-header` deleted**: its 11 pairs are
  normalized-identical at the head, every row of them (`sweep-compare.txt`).
- **`full-transcript-shift` names its scenarios** (`fulltranscript`,
  `fulltranscript-notice`, where it read `fulltranscript*`). It is a
  whole-snap cause about fx's taller session record and shell calls, and it
  took all six of this unit's full snaps at the base with no row left to
  read (the base sweep before the rescore), though their tail pages show
  neither. Decision (reversible): the cause keeps every pair it covered
  (`fulltranscript`'s, `fulltranscript-notice`'s, `links`' at the base:
  no pair of theirs changes between each sweep's first scoring and its
  rescore, `sweep-compare.txt`'s third block), and this unit's scenario is
  read row by row.
- **`session-record-page-bottom` added**: the three `full-pgup` snaps of
  the ledger's scenarios stay apart under `session-record`, as they did at
  the base, and two rows of each, xfx's blank and `A list that starts at
  nine:` at the page's bottom, were covered by no cause at the base or the
  head. Their true cause is the same record: fx's is two rows taller
  (`system prompt:`, `skills:`), so its head page holds two rows fewer of
  the answer (`pgup/`, the head sweep's `p5-transcript-md` `full-pgup`
  pair: fx's rows 5-8 against xfx's 5-6, and xfx's rows 34-35 past fx's
  last). They are not this unit's: the split adds a header under that
  page, not on it.
- The refs the change moved, re-pointed (below).

## Sweep (`sweep-base.md`, `sweep-head.md`, `sweep-compare.txt`)

`scripts/fxcap/sweep.sh --only 'p5-transcript*' --only 'fulltranscript*'
--only 'p5-modified-*' --only 'transcript*'` at 40x120, `--jobs 2`, this
branch's scenarios, once with the base's binary (`v0.1.0-3f2d6e7`) and once
with the code head's (`v0.1.0-b9007bc`), both in one suite-lock slot, then
both rescored (`--rescore`) with this branch's final ledger. 30 scenarios,
178 pairs (`transcript*` selects none). APPROVAL-VERTICAL-SPELLINGS'
`cmp_sweeps.py` compares them pair by pair:

- **base 94 / 178, head 111 / 178, drops 0**; pairs with uncaused rows
  19 at the base, 2 at the head.
- The 26 that move: the cause's 11 pairs and this unit's 6 to
  normalized-identical (above); and 9 inline pairs from
  normalized-identical to identical (`p5-modified-arrows-full` `answered`,
  `draft`, `typed` to `typed4`; `p5-transcript-submit` `filled`, `second`;
  `p5-transcript-tallfirst` `tall`), which are each run's own clock
  (`turn-wall-time`: xfx's base run of `p5-modified-arrows-full` read `2s`
  where fx's read `1s`), not this unit's: the inline document is unchanged.
- No other pair changes class, causes or uncaused-row count.
- The two pairs left with uncaused rows at the head are
  `p5-transcript-question` `asked` (6 rows: an option's wrap) and `narrow`
  (27 rows: the page after a resize), the same at the base: the question's
  card and the resize, not the record's split.
- **The other scenarios whose answers hold a block and that open Ctrl-O**
  (`links`, `p5-ctrlo-wider-fence`; `p5-ctrlo-wider-mod` and the
  `p5-inline-code-retint*` four never open it), the same two binaries and
  ledger: 5 / 8 at both, drops 0; `links` `full` needs
  `full-transcript-shift` no more, its page now heading the text after the
  answer's table as fx's does (`sweep-compare.txt`'s second block).

## Mutants (`mutants.py`, `mutants.txt`)

On a `git clone --no-local` (its remote removed) under `$TMPDIR/mut/`, at
the code head `b9007bc3`, one at a time, the file restored after each, under
the suite lock, each run the unit's tests (`tui::full_transcript::tests`,
the two shell tests, `trace::`): **6 of 6 killed**, the clone's tree clean
after the run (`mutants.txt`, each naming the first six failing tests):

| mutant | killed by |
|---|---|
| M1 a block joins the open run, and the text after it with it (the base's reading) | the split, no-header, moment, chunking and `/trace` tests and the inline-pin shell test, 7 failing in all |
| M2 a block's entry is headed `Response` | the no-header, moment, split and chunking tests, both shell tests |
| M3 a line break after a block opens nothing, so the run is stamped at its first prose | the moment test alone (its stamps) |
| M4 a block opens a `Response` entry of its own | the no-header, moment, split and `/trace` tests |
| M5 a block's entry lays out as nothing | the no-header, `/trace` and inline-pin tests |
| M6 `/trace` names every block `assistant` | the `/trace` test alone |

M3 is the rule the oracle's `full-block-first` decides (the header's
moment): the moments are masked in every pair, so only the test sees it.

## Gate (`gates.sh`, `gate.txt`)

Train 13's `gates.sh` (its body unchanged: `ci.yml` did not change between
train 13's head and this unit's base), once, under the suite lock in one
normal slot with a short `TMPDIR`, on a `git clone --no-local` at
`e3e66b0b`, whose `src/` is the code head's (`gate.txt` carries the empty
`git diff --stat`). **22 steps, 0 failed**, the tree clean before and after:

| step | result |
|---|---|
| `cargo fmt --check`, both `clippy -D warnings` | clean |
| `cargo test --locked --no-fail-fast` | 5713 passed, 0 failed, 7 ignored (15 result lines) |
| `cargo test --locked --features fault-injection --no-fail-fast` | 5750 passed, 0 failed, 7 ignored (15 result lines) |
| `check-xfx-identity`, `check-no-stubs`, `check-no-secrets`, `check-no-local-paths` and its test, `check-preview-contract`, `check-tui-preflight-cost`, `check-usage-clock`, `test-check-no-secrets`, `test-gate-env`, `test-preview-contract`, `fxcap/test-sweep`, `fxcap/test-capture`, both release builds, `smoke`, `smoke-tui` | exit 0 each |

The base's own gate read 5706 and 5743 passed
(`.prd/tui-phase5/receipts/modified-arrows-screens/gate.txt`); this unit
adds 7 `#[test]`s (`red.txt`'s seven new ones) and removes none: 5713 and
5750.

**The first run**, at `8bd5be58` (the same `src/`), failed and was stopped
after step 11 (`gate.txt`'s last block): `check-xfx-identity` refused
`captures.txt`, whose placeholder for the oracle's directory spelled the
retired product name, and so did its wrapper test in both suites;
`e3e66b0b` renames the placeholder. Its plain suite's other 3 failures were
the known catalog-wait class (`tui::worker::tests::a_stop_in_*catalog_wait*`);
the rerun above has none.

`green.txt`: the unit's 8 tests in both suites of that run, 16 of 16 ok.

## The citations (`fx-src-check.txt`)

`fx_src.sh` over this file and the unit's added lines (`src`, the scenario,
and `docs/parity.md`'s added words), the command in the file: every
citation falls inside a range of `fx-src.txt`.

## The ledger's refs (`refs.py`, `refs.txt`)

This unit adds lines to `src/tui/full_transcript.rs` above five of the
ledger's refs into it (`ctrl-o-drawn-wider-named`, `answer-drawn-wider-named`,
`answer-label-and-modifier-named`, `answer-modifier-sequence-wider`; the
fifth, `ctrl-o-command-marks` at `fn detail_spelled`, stands above the
change). MODIFIED-PAGE-KEYS' `refs.py`, with `full_transcript` among its
files and those entries' texts, read 22 checked, 4 wrong; `--fix`
re-pointed them, and it reads **22 checked, 0 wrong** (`refs.txt:23`). The
22nd is new: `session-record-page-bottom`'s `src/tui/shell.rs:42649`, which
has no base ref (`refs.txt:10`). (Round 1 wrote 21 here; opus r1 NIT 1.) The
shell tests land below every ref into `src/tui/shell.rs`. `docs/parity.md`'s
row stays on line 238. Round 2 moves the same four again (below,
`refs-r2.txt`).

## `docs/parity.md`

The full transcript row: the answer is split at its blocks as fx's store
splits it -- each completed code block, table and rule an entry of its own
with no header, the prose after one a `Response` of its own stamped when the
first text after the block came, so an answer that is only a block has no
`Response` header and one that ends in a block none after it, and `/trace`
lists each block under fx's name.

## Round 2 (opus r1's SHOULD 1 and NITs 1-6; both r1 reviews had no MUST)

| commit | what |
|---|---|
| `92a8f9b3` | RED: the round's tests (the reading place, an indented and an unclosed block, the blank rows around a block, the shell test without its blank-row filter); no non-test change |
| `58b7ee2b` | **round 2's code head**: `measure` marks no entry that lays out as nothing; `/trace`'s match names every `Event` variant; two citations at the pin (one in a test comment) |
| `996011b2` | one test comment names the file of its bare `:2114`; comment only |
| the commits after | `docs/parity.md`, the ledger (NIT 2, the four refs again), this folder; no `src/` change |

**SHOULD 1, the open item, rewritten.** At the pin fx never stores a line
break that opens a step: until a step's stream shows visible text it skips
each chunk's leading space, tab, CR and LF and drops a chunk that is nothing
else (`assistant_stream.zig:402-410`, the set at `:668-669`), and the stream's
context is built inside the step loop (`orchestrator.zig:7312`), so a step
after a call starts with nothing seen. xfx's return for line breaks after a
non-answer entry (`presented`, `src/tui/full_transcript.rs:1645-1647`)
matches it there. The case that remains: **mid-step, after visible text and
then a notice**. The notice is an entry at the store's tail
(`store.zig:1301-1322`); a chunk that renders only a line break -- the
presenter holding nothing, as after a blank line or a list line (a blank
line is `processLine` plus one `\n`, `assistant_presentation.zig:416-417`) --
is pushed (`assistant_stream.zig:457-459`), reaches the store through the
pacer (`main.zig:2552-2555`) and, finding no assistant entry at the tail,
opens one stamped then (`store.zig:3088-3095`, `:3141-3143`); the prose after
it joins it. xfx opens nothing for it and stamps the run with the prose. Only
the moments differ: the rows are the same (the head's line breaks trimmed,
`transcript_blocks.zig:2004`), the entry has no header while it holds only
them (`:2114`), and the step's visible text already closed any tool group.
It is "Left apart" below and in `docs/parity.md:238`. This also closes
gpt6 r1's NTH (a call, a blank line, then text), which rested on the old
paragraph: that case is the same on both.

**NIT 1**: the refs paragraph above says 22, as `refs.txt:23` does, and names
the new one.

**NIT 2**: `session-record-page-bottom`'s `xfx_row` is
`'^  A list that starts at nine:$'`, its `|^$` dropped: `classify` reads no
blank text (`scripts/fxcap/ledger.py:366-367`) and `account` covers none
(`:421`). Both sweeps rescored with round 1's final ledger and with round
2's give byte-identical `pairs.json` (`sweep-compare.txt`, its round-2
block).

**NIT 3, the reading place of an entry that lays out as nothing.** fx's full
presentation marks an entry where it writes its header, after the separator
(`transcript_blocks.zig:2116-2117`), and an entry its projection skips, before
the separator it skips (`:2087-2090`); an entry that renders nothing it
passes over unmarked (`:2114`). `skipEntry` (`full_transcript_screen.zig:
5886-5897`) skips what the tool-group projection hides and a command's owned
source: a group's later statuses, which its first draws whole
(`tool_group_projection.zig:1366-1383`, `:1321-1335`, `setOwnedGroup`
`:52-57`), and a call's attached output and diff rows (`isAttachedEntry`
`:246-251`, `hideAttachedRows` `:948-962`). In xfx none of those is an entry
of its own: each call draws its own rows from its group's block, its output
and diff with it. The xfx entries that lay out as nothing are line breaks
alone after a block (fx's renders-nothing path), a question's call (fx keeps
and draws its status, `tool_group_projection.zig:1355-1359`; its `● Asked`
is under a `Tool` header, the ledger's `question-resume-ctrlo-detail`) and a
file change while it runs (no row in fx yet, `src/tui/toolgroup.rs:297-303`,
capture `fx-approvals-40x120-write-ask`). So xfx marks none of them now
(`measure`, `src/tui/full_transcript.rs`, round 2's diff), within the brief's
bound (+29/-12 in two files with the citations and doc, the mark itself
-1 line).
The RED test: a reader on the blank row under a block that re-wraps keeps
fx's place across a width change, in the block that many rows down rather
than on the separator, for the line breaks after a block and for a running
write (`an_entry_that_lays_out_as_nothing_is_no_place_to_hold_a_reader_as_upstreams_is_not`).

**NIT 4**: `/trace`'s label match names `Table`, `CodeBlock`, `ThematicRule`
and `Text` (never made: prose is a `Response`) with no fallback, so a new
`Event` variant does not compile until it is named.

**NIT 5**: `src/trace.rs` `timeline` cites `writeTranscriptTimeline` at
`app_commands.zig:2675-2747@481104fd` (it was c1db9191's `3475-3567`, no pin);
`src/tui/full_transcript.rs`'s two `metadataKind` citations (in `content` and
in a test comment) are `full_transcript_screen.zig:6028-6056@481104fd` (they
were c1db9191's `5867-5895`).

**NIT 6**: `an_indented_code_block_and_a_block_the_stream_ends_split_the_answer_as_fences_do`
(an indented block completed by the line after it, which opens and stamps
the next run, `assistant_presentation.zig:283-286`; a fence and a table the
stream's end closes, `:254-256`); `the_rows_around_a_block_are_one_blank_row_each_as_the_document_has_them`
(the record's rows, blank rows included, for lead, two blocks a blank line
apart, prose and the summary); and the shell test now compares the record's
rows with the document's without filtering blank rows.

**RED → GREEN** (`red-r2.txt`, `green-r2.txt`): over round 1's code the
reading-place test fails (`red-r2.txt`: the reader lands on `""`); the three
other new or changed tests pass there, as they pin round 1's split, and
fail under the base's reading (mutant M1, below). At round 2's code head all
pass.

**Mutants** (`mutants.py`, `mutants-r2.txt`): on a `git clone --no-local`
at `58b7ee2b`, under the lock, **9 of 9 killed**, the clone's tree clean
after the run: round 1's six again (M6 on the label match that now names
every variant) and three for round 2's lines. M7 (every entry that lays out
as nothing is a reading place again, round 1's code) and M8 (a call that
lays out as nothing is one) are killed by the reading-place test alone --
M8 by its running-write case, the line-breaks case having no call; M9
(`/trace` names a rule `assistant`) by the `/trace` test. M1 (the base's
reading) is now also killed by the indented/unclosed test and the
blank-rows test, which pass over round 1's code: the RED those two need is
against the base's reading.

**Sweep** (`sweep-head2.md`, `sweep-compare.txt`'s last block): round 1's
families plus `links` and `p5-ctrlo-wider-fence`, 186 pairs, with round
2's code head's binary (`v0.1.0-58b7ee2`), rescored with this branch's
ledger, against round 1's head sweeps rescored with the same ledger:
**116 / 186 at both, drops 0**, 2 pairs with uncaused rows at both (the
`p5-transcript-question` pair of round 1). The one pair that moves,
`p5-transcript-tallfirst` `tall`, goes from identical to
normalized-identical: each run's own turn time (fx `0s`, xfx `1s`).

**Gate** (`gate-r2.txt`, `green-r2.txt`): `gates.sh` once, under the suite
lock in one normal slot with a short `TMPDIR`, on a `git clone --no-local`
at `615c2bdb`, whose `src/` is round 2's code head with `996011b2`'s test
comment (`gate-r2.txt` carries the empty `git diff --stat`): **22 steps, 0
failed**, the tree clean before and after; 5716 and 5753 passed, 0 failed,
7 ignored each -- round 1's 5713 and 5750 and round 2's 3 new
`#[test]`s (`git diff 07b00d43 615c2bdb -- src`). No catalog-wait failure
in this run. `green-r2.txt`: the unit's 11 tests and the two
reading-position tests the mark change touches, 13 tests, 26 of 26 ok
across both suites.

## Left apart

- **A line break mid-step, after a notice** (round 2; see "Round 2"
  below for the source): fx's store opens an assistant entry for it,
  stamped then, and the prose after it joins that entry; xfx opens nothing
  until the prose, so the `Response` over that prose carries a later moment
  here. Only the moments differ: the entry renders the same rows (its head's
  line breaks trimmed), has no header while it holds only them, and a step's
  visible text has already closed any tool group before it. The moments are
  masked in every pair, so no capture shows it.
- **Where the pacer stands**: fx paces the rendered text between its
  presenter and its store, xfx paces the raw text before its presenter, so
  an entry's moment may differ by a pacer tick on the two; the moments are
  each run's own and masked in every pair.
