---
title: 0.0.16 delta -- P5 train 6 receipt (GATE-GITENV(+F), T-U4, CTRLO-WIDER(+F), FLAKE-CATALOG-EVENT into one branch)
type: receipt
tags: [xfx, phase5, fx-0.0.16, train]
---

# P5 train 6: four closed units, one branch

Branch `train/p5-1108c`, from `origin/main` `690aa505` (PR #81, train 5:
L-U1+U2 OSC 7501, FLAKE-COPYEND), as the brief expected; `origin/main` had
not moved when it was fetched. Each unit was closed by its own two-engine
review and dispatcher gate; this train combines them. The program goal (the
user's words): «최신 fx랑 비교해서 ui쪽으로 모든 기능 구현하고 기본적으로 최신 fx
ui를 그대로 구현해줘» (fx v0.0.16, pin `481104fd`).

Oracle: fx v0.0.16 (`481104fd`), executable sha256
`8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked
by `sweep.py` before and after every run, `FX_AUTO_UPGRADE=0`.

**At a glance.**
- **Merges:** 1 and 4 are git's own (empty remerge-diffs). Merge 2 (T-U4)
  resolved two `src/tui/shell.rs` hunks (train 5's program-status fields and
  T-U4's marker line; both sides' appended tests) and two `docs/parity.md`
  rows; merge 3 (CTRLO-WIDER) the same two rows. Each row was merged word
  by word against the base, no word-level conflict; the token check
  (with a new clause rule for rows both sides changed) is OK on all four,
  and its negative control finds 52 of 52 planted losses. 13 references the
  merges moved, re-pointed and checked against their code (13 of 13).
- **Join check 1:** on Ctrl-O CTRLO-WIDER's names (the arguments, the
  result, a code panel's label and its code) hold through T-U4's block and
  heading caches: after a theme switch and a resize each screen equals the
  same session laid out from nothing, and the way back is byte for byte.
  **The heading is raw, not named** -- CTRLO-WIDER's own design (its round
  2 (f)), not the brief's expectation; recorded, not changed. T-U4's row cut
  never splits a raw cluster or a named spelling, and equals the whole row
  at 40, 120 and 1000 columns. 7 of 7 mutants killed (one survived the
  first run; `392ca22a` adds the case that kills it). Each test needs both
  units, but the caches test passes on CTRLO-WIDER alone (no caches there).
- **Join check 2 (render cost), two findings, neither fixed here:** on
  T-U4's own probe the train costs what T-U4's code does in the same run
  (0.95-1.24×; both 2.0-2.9× T-U4's published numbers, the machine at load
  71-73). With the call's arguments, which every live call sends and T-U4's
  probe did not: (a) T-U4's own Ctrl-O opens a 4 Mi U+009B call in 1.9 s
  and pays 1.4 s a join -- its caches do not cover argument rows; (b) for a
  2 Mi argument of U+261D the train is **2.1-2.8×** T-U4 alone (open 0.73-0.76
  s, join 0.69-0.71 s): CTRLO-WIDER's named form is 8 bytes and cells where
  the raw scalar is 3 and 1, and the argument rows are laid out again on
  every join, theme switch and resize.
- **Join check 3:** a theme report while Ctrl-O is open recolours the named
  label, the cancelled line and every other row to the fresh light session's,
  and the report back gives the first screen byte for byte.
- **Join check 4:** lib (4440) and tui (181) tests through a loopback
  logger: 2 CONNECTs, both `w.example` (known); **none to the Gateway**.
- **Join check 5:** CI and the gate scripts are GATE-GITENV-F's byte for
  byte; the script tests pass on the macOS row (the gate) and the Linux row
  (a container): `test-gate-env` 62 and 60 cases, `test-preview-contract` 24
  mutants each.
- **Gate** at `0252c06f`, one suite-lock hold: **20 of 20 steps passed**
  (`cargo test` 5357, with fault injection 5394, each +30 on `main`'s;
  smoke-tui 1168 checks; `test-capture` 59 OK, its LAUNCH-SETTLE case not
  hit).
- **Sweep**, 141 scenarios: 472 of 684 pairs pass, 0 failed, 0 timeout.
  Against `main` 398 → 464 over the shared 661 (67 up -- T-U4's 62 marker
  pairs and 5 timing or run-local -- 0 down); against T-U4's own 462 → 462
  (3 up, 0 down); against CTRLO-WIDER's own 9 → 9 (0 moved).

## Units and merges

Each is `git merge --no-ff`, in the brief's order; each message names the
unit and its head.

| # | unit | branch @ tip | merge | base | conflicts |
|---|---|---|---|---|---|
| 1 | GATE-GITENV(+F) | `fix/p5-gate-gitenv-f` @ `4db63171` (stacked on `fix/p5-gate-gitenv` @ `2cd3c844`) | `a09e39c1` | `884497c0` | none |
| 2 | T-U4 | `feat/p5-cancel-marker` @ `5a7bb465` | `7fa4cc27` | `884497c0` | `src/tui/shell.rs`, two hunks; `docs/parity.md`, two rows |
| 3 | CTRLO-WIDER(+F) | `feat/p5-ctrlo-wider-f` @ `1824e868` (stacked on `feat/p5-ctrlo-wider` @ `eeb0f7a7`) | `d55e2317` | `884497c0` | `docs/parity.md`, the same two rows |
| 4 | FLAKE-CATALOG-EVENT | `fix/p5-flake-catalog-event` @ `709786bb` | `5f601851` | `884497c0` | none |

Every base is git's merge base, and every one is `884497c0` (train 4,
PR #80): GATE-GITENV-F merged it at `621c9504`, T-U4 at `3857d983`, CTRLO-WIDER
at `e7a2e041`, FLAKE-CATALOG-EVENT branched from it, and `main` `690aa505` is
train 5 on top of it. FLAKE-CATALOG-EVENT is the last merge and touches one
file, `src/tui/worker.rs`, which no commit after it changes: `git revert -m 1
5f601851` drops it alone if its dispatcher gate says so.

After the merges, in order:

| commit | what |
|---|---|
| `dca1004e` | the `path:line` references the merges moved, re-pointed (below) |
| `03476db7` | the join tests (join checks 1 and 3, below); test only |
| `0252c06f` | this receipt, first part (the gate, the sweeps, the probe, the network check and the Linux run ran at this commit; its code is `03476db7`'s) |
| `392ca22a` | the row-cut join test's skin-tone modifier case, after the first mutant run let a mutant through (join check 1); test only |
| the next | this receipt |

`git show --remerge-diff` of each merge is `remerge-1.txt` .. `remerge-4.txt`.
Merges 1 and 4 have an empty body: git's own result, unchanged. Merge 2's
names `docs/parity.md` and `src/tui/shell.rs`, merge 3's `docs/parity.md`,
each with the hunks below.

### Where the brief expected overlap, and where it was

`overlap.txt` (train 4's `overlap.py`) has every file both sides of a merge
changed since their base, and the nearest pair of hunks.

- **Merges 1 and 4: no file.** GATE-GITENV(+F) changed CI, the gate scripts,
  `build.rs`, the sweep's python and docs (`git diff --stat 884497c0
  4db63171 -- . ':!.prd'`: 16 files); `main` changed none of them since
  `884497c0`. FLAKE-CATALOG-EVENT changed `src/tui/worker.rs` alone, which
  no other side touched.
- **Merge 2 (T-U4): seven files.** `docs/parity.md` and `src/tui/shell.rs`
  (the conflicts, below); `scripts/fxcap/parity-ledger.toml` (nearest 3
  unchanged lines: train 5's re-point of `safe-page-edge`'s `shell.rs`
  reference against T-U4's `call-row-page-edge` entry right under it; each
  entry one side's); `scripts/smoke-tui.sh` (56), `src/tui/event_loop.rs`
  (4366), `src/tui/question.rs` (322), `tests/tui.rs` (61). The brief's
  expectation held: train 5 (L-U1+U2) changed `shell.rs` and `event_loop.rs`,
  and T-U4 too; in `event_loop.rs` T-U4's hunks are its test helpers' marker
  glyph, thousands of lines from train 5's.
- **Merge 3 (CTRLO-WIDER): six files.** `docs/parity.md` (the conflict);
  `src/tui/full_transcript.rs` (nearest 15 unchanged lines: T-U4's
  `Body::Block` recolour against CTRLO-WIDER's Prompt card naming -- two arms
  of one `match`); `src/tui/shell.rs` (2: T-U4's `Mark::Block(rows, _)` arm
  against CTRLO-WIDER's `Mark::Operational` arm naming the text);
  `scripts/fxcap/parity-ledger.toml` (7), `src/tui/wrap.rs` (317),
  `tests/tui.rs` (2310). **Not `src/tui/toolgroup.rs`**: the brief expected
  both units there, and CTRLO-WIDER(+F) does not change it (`git diff --stat
  884497c0 1824e868 -- src/tui/toolgroup.rs` is empty). In
  `full_transcript.rs` the two units touch different functions: T-U4 the
  group block and heading caches (`group_block`, `heading`, `tool_rows`) and
  the block recolour; CTRLO-WIDER `detail_spelled`, `terminal_safe_line`,
  `drawn_wider_named`, `shown_event` and the Prompt card. Where they meet is
  at run time: join check 1.

The merged tree built with and without `fault-injection` after merges 2, 3
and 4 (`cargo check --locked --all-targets`, both), and the four file
checks (`cargo fmt --check`, `check-xfx-identity.sh`, `check-no-stubs.sh`,
`check-no-secrets.sh`) passed on it before the gate.

### Merge 2's conflicts: `src/tui/shell.rs`, `docs/parity.md`

`src/tui/shell.rs`, two hunks:
1. the `Shell` struct: train 5 (L-U1+U2) added the program-status fields
   (`compaction_failed` .. `launch_status`) right above `cancel_owed`, whose
   doc line T-U4 changed (`■ Cancelled` to `▪ Cancelled`). Resolved: train
   5's fields, then T-U4's line.
2. the end of `mod tests`: both appended tests -- train 5 its OSC 7501
   section, T-U4 `a_turn_stopped_over_its_answer_leaves_its_cancelled_line_on_ctrl_o_too`
   and `a_theme_report_recolours_ctrl_os_stopped_turn_line_and_refused_batch_row`.
   Resolved: train 5's, then T-U4's.

Nothing else of either side was touched, and no U+25A0 is left that T-U4's
sweep would have changed: `git grep -i -E '■|25a0'` over `src`, `tests`,
`scripts`, `docs` and `.github` at the merge finds only the four lines
T-U4's `sites.txt` keeps (`unicode_display_data.rs:563`'s packed word, the
`p5-transcript-cancel.cap` comment naming the change, and `theme.rs:189`,
`:196`, the constant's doc).

`docs/parity.md`: the `full-screen TUI` and `full transcript (C-o)` rows. At
the base they are lines 237 and 238; train 5 changed `full-screen TUI`
(L-U1+U2's Ctrl-Z clause, 18 tokens added and 1 deleted) and not the other;
T-U4 changed both (five marker glyphs in the first, its argument clause in
the second). Resolved row by row, **word by word against the base**
(`rowmerge.py`: `git merge-file` over one whitespace token a line, the
base's row as the ancestor; a word-level conflict would stop it, and none
came): `full-screen TUI` holds train 5's clause and T-U4's five glyphs,
`full transcript (C-o)` is T-U4's (train 5 left it as the base has it).

### Merge 3's conflict: `docs/parity.md`

The same two rows. CTRLO-WIDER(+F) changed both: in `full-screen TUI` the
scalar count (`3,892` to `43 scalars since CTRLO-WIDER ...`), a regional
indicator's clause, and its new paragraph **Widths a terminal disputes, in
an answer and in the composer**; in `full transcript (C-o)` the clause on
the scalars a terminal draws wider. Each side's edits lie in different
clauses of each row; the same word-by-word merge took every one, and the
merged rows were read where each side's edits sit: the sentences join.

## The token check (`token_check.py`, `token-check.txt`, `token-self-test.txt`)

Train 5's `token_check.py`, with one rule added. For each merge, against
git's merge base: `docs/parity.md` row by row (whitespace tokens as a
multiset: the merged row is the base's plus what each side added minus what
each side deleted) and in the rows' order; the ledger entry by entry (an
entry one side changed is that side's, one a side deleted is gone, one a
side added is in, the rest the base's; every comment line a side added is
in, every one a side deleted is not), with train 4's numbers-only rule.
**Added for train 6: the clauses of a row both sides changed.** Every run of
tokens a side wrote in place of the base's (`difflib`, against the base's
row), with that side's own token either side of it, is in the merged row
whole and contiguous -- so a clause a side wrote is not split, reordered or
moved. The multiset check does not look at the order of tokens inside a
row, and merges 2 and 3 each resolved two rows word by word.

**Result: all four merges OK.**

- Merge 1: GATE-GITENV-F changed neither file; `main`'s rows and ledger as
  `main` has them.
- Merge 2: `full-screen TUI` changed by both, 23 tokens added and 6 deleted,
  as expected, its 6 clauses whole and in place; `full transcript (C-o)`
  T-U4's (54 added, 2 deleted); T-U4's `/resume` and `/allowlist` rows; the
  ledger: T-U4's two deleted causes (`cancelled-marker`,
  `U7-cancelled-glyph`) gone, its four added (`amendment-card-after-stop`,
  `cancel-page-edge`, `question-cap-four`, `retry-budget-lifecycle`) in, its
  four changed (`call-row-page-edge`, `call-row-arabic-letter-mark`,
  `call-row-raw-argument`, `call-row-source-cut`) as T-U4 has them; train
  5's entries as `main` has them.
- Merge 3: both rows changed by both sides, 438 and 301 tokens added, 15
  and 17 deleted, as expected, 9 and 12 clauses whole and in place; the
  ledger: CTRLO-WIDER's `ctrl-o-drawn-wider-raw` gone, its four causes and
  four divergences in, every other entry the first parent's.
- Merge 4: FLAKE-CATALOG-EVENT changed neither file.

No ledger entry was changed by two sides; no comment line was lost.

**The negative control** (`--self-test`, on each merge's own texts): train
5's cases (a token a side added dropped, one a side deleted put back, a
ledger entry a side added dropped and one a side deleted restored, a
comment line dropped, every row a side changed put back as the other side
has it, every row a side added dropped, a row a side added swapped with one
the other side changed) and train 6's: every row both sides changed put
back as ours has it and as theirs has it (one side's clauses lost), and its
longest clause with two tokens swapped (the multiset unchanged; only the
clause rule sees it). Merge 1: 7 of 7 found. Merge 2: 15 of 15, among them
`full-screen TUI` as each side has it and with a clause's tokens swapped.
Merge 3: 17 of 17, the same three for each of the two rows. Merge 4: 13 of
13.

The re-point commit `dca1004e` changes ledger numbers after the merges; the
token check is about the merges, and `repointed.py` checks each of those
numbers against its code (below).

## The references the merges moved (`dca1004e`)

Train 5's `refs.py` (the ledger and `docs/parity.md`), five sides: `main`
`690aa505`, GATE-GITENV-F `4db63171`, T-U4 `5a7bb465`, CTRLO-WIDER-F
`1824e868`, FLAKE-CATALOG-EVENT `709786bb`. At merge 4 `5f601851`
(`refs.txt`): 77 references, 20 (side, reference) pairs `MOVED`. Widened
(train 4's `refs_all.py`, every unpinned `path:line` in every tracked file
outside `.prd/`, and `refs_short.py`, `name.rs:N`), at merge 4
(`refs-all-merge4.txt`, `refs-short-merge4.txt`): two more in `capfile.py`
and three in `event_loop.rs`.

`dca1004e` re-points 13 of them, each to the code it named on the side
whose text cites it. `repointed.py` checks each against that side's copy of
the cited file, the line and two either side: **13 of 13 the same code**
(`repointed.txt`).

| who cites it | cited on its side | now | moved by |
|---|---|---|---|
| ledger `ctrl-o-command-marks` (source) | `src/tui/full_transcript.rs:492` (`main`: `fn detail_spelled`) | `:495` | CTRLO-WIDER (+3: `use std::borrow::Cow` and doc lines) |
| ledger `filler-modifier-count` (source) | `src/tui/wrap.rs:622` (`main`: `fn upstream_cells`) | `:656` | T-U4 (+19) and CTRLO-WIDER (+15); the signature only is checked: CTRLO-WIDER rewrote the function's doc and body |
| ledger `ctrl-o-drawn-wider-named`, `answer-modifier-sequence-wider` (source) | `src/tui/full_transcript.rs:2615` (CTRLO-WIDER: `fn drawn_wider_named`) | `:2743` | T-U4 (+128: its caches above it) |
| ledger `answer-drawn-wider-named`, `answer-label-and-modifier-named` (source) | `src/tui/full_transcript.rs:2666` (CTRLO-WIDER: the doc line above `fn shown_event`) | `:2794` | T-U4 (+128) |
| ledger `answer-drawn-wider-named` (source) | `src/tui/shell.rs:12531` (CTRLO-WIDER: `shown_event(event)` in `present`) | `:12859` | train 5 (+328) |
| ledger `session-record`, `transcript-page-offset`, `toolgroup-ctrlo-record-short`, `safe-page-edge`, `full-transcript-shift` (xfx) | `src/tui/shell.rs:37758` (`main`, train 5's re-point) | `:37764` | CTRLO-WIDER (+6) |
| ledger `cancel-page-edge` (xfx) | `src/tui/shell.rs:37424` (T-U4) | `:37764` | train 5 (+334) and CTRLO-WIDER (+6) |
| ledger `amendment-card-after-stop` (xfx, why) | `src/tui/shell.rs:9665` (T-U4: `fn trailer_rows`) | `:9762` | train 5 (+97) |
| ledger `ri-run-pairing` (source) | `src/tui/wrap.rs:593-604` (CTRLO-WIDER: `fn cluster_cells`) | `:612-623` | T-U4 (+19) |
| ledger `resize-document-repaint` (source) | `src/tui/shell.rs:15273` (CTRLO-WIDER: `fn resize`) | `:15607` | train 5 (+334) |
| `capfile.py` `TIMERS`, `warning_shown` | `src/tui/shell.rs:14033-14058` (`main`) | `:14039-14064` | CTRLO-WIDER (+6) |
| `capfile.py` `TIMERS`, the full transcript | `:14051-14053` | `:14057-14059` | the same |
| `event_loop.rs`, the wrap into 40 + 40 and 10 | `wrap.rs:213-221` (`main`) | `:232-240` | T-U4 (+19) |

Each unit's own copies of `main`'s old references were stale on its branch
(each moved code under a reference it did not change), and CTRLO-WIDER's
`full_transcript.rs:492` was stale on CTRLO-WIDER: on its branch line 492
is a doc comment.

At the head (`refs-head.txt`): 76 references, 0 moved. `refs-all.txt` holds
the widened scans at the head. What they still flag is not moved by this
train:

- the nine references train 5's `stale.txt` listed, each naming other code
  on `main` already (`smoke-tui.sh`'s `check.rs:1084`, `tests/tui.rs`'s
  `shell.rs:2015`, `shell.rs:2703` and `tests/tui.rs:1191`, `smoke-tui.sh`'s
  `shell.rs:2597`, `event_loop.rs`'s `event_loop.rs:959-962` and
  `shell.rs:2706`, `shell.rs`'s `shell.rs:5816`, `wrap.rs`'s
  `check.rs:1689-1694`);
- two more, flagged now because T-U4 moved `main`'s lines under them, each
  naming other code on `main` too: `event_loop.rs`'s `transcript.rs:136`
  for `MAX_TAIL_ROWS` (`main`'s line 136 is a `return;`; the constant is at
  347) and `transcript.rs:505-531` for "adopts only on `Landed::All`"
  (`main`'s lines are `Gutter` arms and the `Op` enum's doc). Re-pointing
  either to the line its old number maps to would name other wrong code;
  finding the code each meant is outside this train's merges, as train 5
  said of its nine. `stale.py`, `stale.txt`: the eleven, each citing line
  at the head beside the lines it names on `main`;
- the rest are in the historical plans under `docs/superpowers/plans/`
  (2026-09-10 and -11), as trains 4 and 5 left them.

## Join checks

### 1. CTRLO-WIDER's names through T-U4's caches (`03476db7`, `392ca22a`; `join-units.txt`, `join-mutants.txt`)

**What each unit does on the path.** On Ctrl-O, T-U4 builds a group's
expanded block once per (group revision, palette) and shares it among the
group's entries (`FullTranscript::group_block`,
`src/tui/full_transcript.rs:1907`), and keeps each call's heading rows per
(heading text, width) (`FullTranscript::heading`, `:1953`); the heading text
is the block's row, which `ToolGroup::expanded` builds from the call's
`shown`. CTRLO-WIDER names the 43 scalars a terminal draws wider than xfx
and fx count them where a file's or a tool's line is spelled for this
screen -- `detail_spelled` (`:495`, the arguments, the result and a
command's output, through `Detail::made_inert`, `:431`), `terminal_safe_line`
(`:2687`, a file's diff rows) -- in an answer's events (`shown_event`,
`:2795`: prose, a code panel's code **and its label**, `block.language`,
`:2807`; table cells) and on Ctrl-O's Prompt card (`:1807`). It names
nothing in `toolgroup.rs` or `bridge.rs`, so **the Ctrl-O heading holds the
scalar raw**, as fx's does: that is CTRLO-WIDER's own design (its README,
round 2 (f): "the heading holds the path raw and the argument row
`│  path: \u{261d}x\u{2e3a}.txt` is still shown", pinned by
`a_value_the_heading_had_to_spell_is_never_shown_by_it_already` and its
mutant M22), and the ledger's `ctrl-o-drawn-wider-named` says "a file's or
a tool's line". **The brief expected the heading named; it is not, on
CTRLO-WIDER alone or on the train.** This is recorded, not changed: the
train keeps each unit's meaning, and the heading is CTRLO-WIDER's scope. A
consequence, also CTRLO-WIDER's: a heading that holds one of the 43 at
Ctrl-O's margin is drawn as fx draws it, and loses glyphs on tmux as fx's
does (CTRLO-WIDER's `measure.txt` mechanisms; no scenario has such a
heading).

**The tests, at the train head** (all three pass; `cargo test --lib` in the
gate, below):

- `ctrl_o_names_what_a_terminal_draws_wider_and_its_caches_follow_the_palette_and_the_width`
  (`src/tui/shell.rs`): a turn that reads `☝x⸺www….txt` -- its start, its
  arguments and its result through the channel's `made_inert`, as a live
  turn's journal hands them over -- and answers with a code panel labelled
  `fam☝⸺` holding `code ☝ here`. On Ctrl-O at 120 columns: the heading
  `└ Read ☝x⸺www….txt` raw; the argument row `│  path: \u{261d}x\u{2e3a}www….txt`,
  the result `line \u{261d} one`, the panel's header `─ fam\u{261d}\u{2e3a} ─`
  and the code named; a raw one on no other row. Then **through the caches**:
  the screen equals, byte for byte but the entries' times, the screen of the
  same session laid out from nothing (`shell_with`, the same events, Ctrl-O
  opened once) -- in the dark palette at 120 columns; after a theme switch
  (closed, `?997;2n`, opened) in the light one, whose rows differ from the
  dark ones; after a resize to 40 columns with Ctrl-O open (the heading laid
  out again: no row is the 120-column heading, every row fits 40 cells) and
  opened again there, the light 40-column session's; and back to dark at
  120 columns, the first screen exactly. A block or a heading kept across
  a palette or a width would show here as a row the fresh session does not
  have.
- `the_transcripts_row_keeps_what_a_terminal_draws_wider_whole_at_every_width`
  (`src/tui/toolgroup.rs`): the transcript's own row (`inline`, T-U4's
  column cut, `toolgroup.rs:128`). CTRLO-WIDER names nothing on this row, so
  a raw one is a grapheme cluster (`☝`, `⸺`, `🖐`, `x` U+E0002, `a` U+FE0F)
  and a named form the model wrote itself (`\u{261d}` …) is one of the
  spellings `inline` keeps whole (`spelling_at`). Seven arguments: short,
  raw and named; raw and named past any screen (81 920 and 72 072 cells
  against the 65 536 the row keeps); a raw one at the cut, a named one
  across it, and (`392ca22a`) U+261D with its skin-tone modifier, the base
  in the row's last column and the modifier past it. For each, at 40, 120 and 1000 columns the row as a screen cuts it
  (`transcript::ellipsized`) equals the whole row; and what the row keeps
  is a prefix of the spelled argument ending where one of its units ends --
  never inside a cluster or a spelling.

**On each unit alone** (`join-units.txt`: each unit's tip, a `git clone
--no-local` with the three tests appended unchanged, under the lock):

| test | train head | T-U4 alone (`5a7bb465`) | CTRLO-WIDER-F alone (`1824e868`) |
|---|---|---|---|
| Ctrl-O names, through the caches | pass | **fails**: no argument row at all -- the raw value is found in the raw heading and its row left out, as before CTRLO-WIDER (`the argument row, named`) | pass (it has no caches; its layout is from nothing every time) |
| the row's cut | pass | pass (its cut; the raw scalars are clusters) | **fails**: `named, past any screen: cut at byte 4093 of 72072, inside a unit` -- the 4 KiB byte bound T-U4 removed for a read's, list's or pattern's argument |
| retint with Ctrl-O open (join check 3) | pass | **fails** at the label (not named) | **fails** at the first assertion: the stopped turn's line is not on Ctrl-O at all (T-U4's `95669160`) |

So each test needs both units, but the first, whose subject is the caches,
passes on CTRLO-WIDER alone: the caches change no screen there because
there are none. That it would catch a cache kept too long is the mutants'
question.

**Do they discriminate?** `join_mutants.py` on a `git clone --no-local` of
the head, one mutant at a time, the clone restored after each and clean at
the end, under the suite lock (`join-mutants.txt`; the line each failing
test said names what held it):

| mutant, planted on the train | Ctrl-O, caches | retint, open | the row's cut | said |
|---|---|---|---|---|
| JM1 T-U4's group block kept across a palette (`kept.palette == palette` dropped) | **FAILED** | **FAILED** | ok | `light, 120`; `the open screen after the report` |
| JM2 T-U4's heading kept across a resize (`heading.cols == cols` dropped) | **FAILED** | ok | ok | `the heading, laid out again` |
| JM3 CTRLO-WIDER's code panel label not named (`block.language`) | **FAILED** | **FAILED** | ok | `the code panel's label, named` |
| JM4 CTRLO-WIDER's file and tool lines not named (`detail_spelled`) | **FAILED** | ok | ok | `the argument row, named` |
| JM5 T-U4's retint of a Ctrl-O block off (shown as drawn) | ok | **FAILED** | ok | the open screen after the report |
| JM6 T-U4's row cut a character at a time, not a cluster | ok | ok | **FAILED** | `a raw one and its modifier across the cut: cut at byte 65538 of 65543, inside a unit` |
| JM7 T-U4's row cut not keeping a spelling whole | ok | ok | **FAILED** | `named, past any screen: cut at byte 65536 of 72072, inside a unit` |

**7 of 7 killed**, the unchanged tests passing. The first run
(`join-mutants-first.txt`, at `0252c06f`) killed 6: **JM6 survived** -- no
case put a cluster whose second scalar has cells of its own at the cut, so
a cut a character at a time fell between clusters anyway. `392ca22a` adds
that case (U+261D U+1F3FB, the base in the row's last column, the modifier
past it) and JM6 is killed by it; the same run took the three tests on each
unit again (`join-units.txt`, the table above unchanged; the first run is
`join-units-first.txt`), and at `392ca22a` clippy both ways, `cargo fmt
--check` and the tests it touched (`tui::toolgroup::tests` and the two
Ctrl-O join tests, 43 passed) (`join-head-r2.txt`). The gate and the sweep
ran at `0252c06f`; `392ca22a` changes one test's cases and nothing else.

### 2. T-U4's render cost on the train head (`probe/`)

T-U4's round 3 probe (`round3/render_cost_probe.rs`), appended to
`src/tui/shell.rs`'s tests on two `git clone --no-local` throwaways -- T-U4's
tip `5a7bb465` and the train at `0252c06f` -- release, one case a process,
both clones alternating case by case, two rounds, **the whole run in one
hold of the suite lock** (`probe/run.sh`, `probe/probe.txt`). Two things
added to the probe and nothing else changed (`probe/render_cost_probe.rs`):

- the case **`wider`**: 2 Mi × U+261D, a scalar a terminal draws wider than
  xfx and fx count it, which CTRLO-WIDER names on Ctrl-O's tool lines
  (`\u{261d}`: 8 bytes where the raw scalar is 3);
- **`XFX_PROBE_DETAIL=1`**: after the call's start, the call's arguments
  (`UiEvent::ToolDetail`, `Detail::Arguments`, through the channel's
  `made_inert`). A live turn's journal hands Ctrl-O the arguments of every
  call (`full_transcript::details`, `full_transcript.rs:519`); T-U4's
  probe never sent them, so its Ctrl-O laid out a heading and no argument
  row.

The machine's load was 71 to 73 through the run (other workers' jobs);
T-U4's round 3 numbers were taken at 21 to 62. So each train number is
read against **T-U4's own code in the same run**, case for case, and
against T-U4's published number. ms; the two rounds as `r1 / r2`.

| case | arguments | build | Ctrl-O open | join | theme | resize | peak MiB |
|---|---|---|---|---|---|---|---|
| 4 Mi U+009B | no (T-U4's probe) | T-U4's published (round 3) | 229 | 36 | 41 | 206 | 409 |
| | | T-U4 alone, this run | 535 / 464 | 94 / 81 | 99 / 91 | 473 / 486 | 410 |
| | | **train** | **519 / 526** | **93 / 82** | **94 / 92** | **588 / 534** | 410 |
| 2 Mi U+261D | no | T-U4 alone | 102 / 100 | 7.0 / 6.9 | 5.8 / 5.7 | 97 / 94 | 95 |
| | | **train** | **98 / 100** | **6.9 / 6.9** | **5.7 / 5.7** | **95 / 98** | 96 |
| 4 Mi U+009B | yes | T-U4 alone | 1 895 / 1 877 | 1 451 / 1 428 | 1 479 / 1 440 | 1 914 / 1 871 | 775 |
| | | **train** | **1 971 / 1 918** | **1 439 / 1 459** | **1 425 / 1 426** | **1 949 / 1 891** | 781 |
| 2 Mi U+261D | yes | T-U4 alone | 314 / 311 | 251 / 250 | 307 / 264 | 392 / 390 | 142 |
| | | **train** | **762 / 727** | **690 / 709** | **760 / 668** | **826 / 828** | 307 |

**Reading it.**

1. **T-U4's own probe, 4 Mi U+009B: no regression from the train.** The
   train is T-U4's code's cost within the run's noise (open 0.97-1.13×,
   join 0.99-1.01×, theme 0.95-1.01×, resize 1.10-1.24×). Against T-U4's
   published numbers the train is 2.3× on open, 2.3-2.6× on join, 2.2-2.3×
   on theme, 2.6-2.9× on resize -- and T-U4's own code, in the same run, is
   2.0-2.6× them too: the machine's load (71-73 against 21-62), not the
   code.
   Under the brief's rule this is past 2× of T-U4's numbers; the same-run
   baseline places all of it on the load.
2. **2 Mi U+261D, the call's start only: no regression.** The heading holds
   the scalar raw (join check 1), so CTRLO-WIDER changes nothing the probe
   lays out; the train's numbers are T-U4's.
3. **With the call's arguments -- the path every live call takes -- two
   findings.**
   - **(a) T-U4's, on T-U4 alone too:** for 4 Mi U+009B Ctrl-O opens in 1.9 s,
     a call joining the open group costs 1.4 s, a theme switch 1.4 s, a
     resize 1.9 s, peak 775 MiB -- in the same run 3.5-4× (open, resize)
     and 15× (join, theme) what the same code costs without the arguments,
     and 8-40× T-U4's published round 3 numbers. The argument row is the 32 MiB spelled argument
     (`│  pattern: \u{009b}…`), railed at 120 columns on every layout of the
     entry (`tool_rows`, `full_transcript.rs:2057`), and the entry's cache is
     keyed on the group's revision, the width and the palette
     (`laid_out`, `:1763-1782`): a call joining the group, a theme switch
     and a resize each lay every member's argument rows out again. T-U4's
     round 3 caches cover the group's block and each heading, not the
     arguments' rows; its probe sent no arguments, so its "the first open
     of the worst case is 0.23 s" is the heading's cost, not a live call's.
     Not a train regression (the train is T-U4's cost: 0.96-1.04×), and
     not fixed here.
   - **(b) The train's, past 2×:** for 2 Mi U+261D the train is **2.3-2.4×**
     T-U4 alone on open, **2.7-2.8×** on join, **2.5×** on theme, **2.1×** on
     resize, peak 2.2× (307 MiB against 142), and the arguments' own
     crossing 11× (61-64 ms against 5.6-5.7). The mechanism is the join of
     the two units: CTRLO-WIDER names the scalar in the argument row
     (`detail_spelled`, `:513`), 8 bytes and 8 cells where T-U4 alone writes
     the raw scalar's 3 bytes and 1 cell, so the row T-U4's layout rails on
     every join, theme switch and resize is 16 MiB and 16 Mi cells (about
     140 000 rows at 120 columns) where T-U4 alone rails 6 MiB in about
     18 000; and
     the naming itself is a pass over every character on its way across the
     channel. The cost is (a)'s, multiplied by the named form's length.
     **A finding, recorded and not fixed here** (a train keeps each unit's
     code): its owner is the argument rows' layout -- T-U4's caches, extended
     to the rows `tool_rows` rails for each argument, or a bound on what
     Ctrl-O lays out of an argument -- and both units' reviewers should
     see it. The case is the stream's own bound (a 6 MiB argument of an
     8 MiB completion), not a common call; shorter arguments were not
     measured here.

Input while Ctrl-O is open is still 0.0-0.3 ms a key once laid out (PageUp,
every row). The probe's file and runner are kept; nothing of it is
committed to `src`.

### 3. Retint x naming: a theme report while Ctrl-O is open (`03476db7`)

`a_theme_report_while_ctrl_o_is_open_recolours_a_named_label_and_a_cancelled_line_and_back`
(`src/tui/shell.rs`): the session of join check 1, then a turn stopped by
Ctrl-C over its answer (T-U4's `▪ Cancelled · What can xfx do differently?`
line, which Ctrl-O shows since `95669160`), Ctrl-O opened in the dark
palette: the cancelled line in the dark greys (`cancelled_line(PALETTE)`)
and the named label `─ fam\u{261d}\u{2e3a} ─`. **The report arrives with
Ctrl-O open** (`?997;2n` routed while the full transcript holds the
screen): the open screen is redrawn at once; it holds the light cancelled
line and not the dark one, and it equals, but the entries' times, the
screen of the same session drawn from nothing in the light palette -- every
row, the label's, the block's (drawn dark, shown light: T-U4's
`recoloured`) and the Response's included, so no grey of the dark palette
is left. The report back (`?997;1n`, still open) gives the first screen
**byte for byte** (`screen_rows()` equal, every row). It passes at the
train head and on neither unit alone (the table above). T-U4's own retint
test closes Ctrl-O before the report; this one does not.

### 4. Network hermeticity (`net.txt`, `connect.log`)

FLAKE-CATALOG-EVENT's method: its `connect_logger.py` (a loopback stand-in
for a proxy: it logs each connection's first request line, answers 403 and
forwards nothing), started by `net.py` as its own child on an ephemeral
port and ended by it; `HTTPS_PROXY`, `https_proxy`, `HTTP_PROXY` and
`http_proxy` at it, `NO_PROXY`, `no_proxy`, `ALL_PROXY` and `all_proxy`
unset; a `git clone --no-local` of the train at `0252c06f`; one hold of the
suite lock.

| run | result | CONNECTs logged during it |
|---|---|---|
| `cargo test --locked --lib --no-fail-fast` | 4440 passed, 0 failed, 7 ignored | **2**, both `CONNECT w.example:443` |
| `cargo test --locked --test tui --no-fail-fast` | 181 passed, 0 failed | **0** |

**No CONNECT to the Gateway host** (`ai-gateway.vercel.sh`), in either
suite. The two `w.example` lines are the known MCP probe
(`mcp::command::tests::a_header_or_env_value_is_never_said_back_and_never_planted_in_the_project`,
FLAKE-CATALOG-EVENT's `proxy-sweep.txt`), TEST-NET-ISOLATION's. The tui
suite's children are spawned with the parent's proxy variables
(`tests/support/sandbox.rs`'s `CONTROLLED_VARS` removes none of them), so
the 0 covers the xfx processes they start too; a loopback Gateway over
cleartext takes no proxy by design (`gateway::may_use_a_proxy`), so a fake
Gateway's requests were not expected here.

### 5. The gate scripts (`gates.txt`, `linux.txt`)

- `git diff 4db63171 HEAD -- .github` is empty, and so is the same diff
  over `scripts/check-*.sh`, `scripts/test-*.sh`, `scripts/fxcap/sweep.py`,
  `scripts/fxcap/ci_sweep.py` and `build.rs`: the head's CI and gate scripts
  are GATE-GITENV-F's, byte for byte. CI's per-OS split is its `matrix`'s
  `script_tests` rows (`ubuntu-24.04` and `macos-15`), which alone run
  `test-gate-env.sh` (and through it `test-check-no-secrets.sh`) and
  `test-preview-contract.sh`; every `check-*.sh` still runs on every row.
- **macOS row** (this machine, the gate above): `test-gate-env.sh` 62
  cases ok (through it `test-check-no-secrets.sh`, 13 cases, 3473 tracked
  names past `ARG_MAX`), `test-check-no-secrets.sh` on its own 13 cases ok,
  `test-preview-contract.sh` 24 mutants caught. macOS, bash 3.2's and BSD
  grep's userland.
- **Linux row** (`linux.sh`, `linux.txt`): a fresh `rust:1.99-bookworm`
  container, as GATE-GITENV-F's `linux-f.sh` ran its own (root installs
  ruby, then uid 1000 runs on a copy of a `git clone --no-local` of the
  train at `0252c06f`), under the suite lock: Linux aarch64, GNU bash 5.2,
  git 2.39.5, GNU grep 3.8. `test-gate-env.sh` **60 cases ok** (127 s;
  through it `test-check-no-secrets.sh`, 13 cases, 6959 tracked names past
  `ARG_MAX`); `test-preview-contract.sh` **24 mutants caught**; the copy clean
  after. 62 and 60 are GATE-GITENV-F's own counts at its tip on the two
  platforms (its `runs-f-macos.txt:207`, `runs-f-linux.txt:208`); git 2.39
  runs fewer of the cases, as its README says.
- What is not run here: GitHub's own runners. The matrix and its
  `script_tests` rows are checked as text (unchanged since `4db63171`), and
  each row's script tests were run on its OS's userland.

## Gate (`gates.sh`, `gates.txt`, `join-tests.txt`)

`gates.sh` (train 5's, its comment only changed): every step the brief
lists, at `0252c06f` (this receipt's first part on the join tests
`03476db7`; the commits after it change only this directory), one after
another, **the whole script inside one hold of the suite lock**, each
step's exit taken from `$?` on the line right after its command. Tree clean
at the start; at the end only this directory's `selection.txt` and
`test-counts.txt`, written while it ran, were untracked. Load 59 to 79
(other workers' jobs). **20 steps, 0 failed, none run again.**

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| the same `--features fault-injection` | ok |
| `cargo test --locked --no-fail-fast` | **5357 passed**, 0 failed, 7 ignored (15 binaries) |
| `cargo test --locked --features fault-injection --no-fail-fast` | **5394 passed**, 0 failed, 7 ignored |
| `scripts/check-xfx-identity.sh` / `check-no-stubs.sh` / `check-no-secrets.sh` | ok (14223 tracked files) / ok / ok (14223) |
| `scripts/check-preview-contract.sh` / `check-tui-preflight-cost.sh` / `check-usage-clock.sh` | ok / ok (200x300 replay 2.16 ms mean; budget 32 ms) / ok |
| `scripts/test-check-no-secrets.sh` / `test-gate-env.sh` / `test-preview-contract.sh` | ok (13 cases; 3473 tracked names past `ARG_MAX`) / ok (62 cases) / ok (24 mutants caught) |
| `scripts/fxcap/test-sweep.sh` | 67 + 27 tests, OK |
| `scripts/fxcap/test-capture.sh` | **59 tests, OK** -- `test_capture.py:394`'s case (LAUNCH-SETTLE's) passed this time |
| `cargo build --locked --release`; `--release --features fault-injection --target-dir target/faulty` | ok; ok |
| `scripts/smoke.sh target/release/xfx <dir>` | 47 checks, 0 failures |
| `scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx <dir>` | 37 scenarios and the oracle, **1168 checks**, 0 failures |

Every side's tests are in (`test-counts.txt`, `#[test]` under `src` and
`tests`): base `884497c0` 5191; `main` 5239 (+48, train 5); T-U4 +12,
CTRLO-WIDER(+F) +13, FLAKE-CATALOG-EVENT +2, GATE-GITENV(+F) none of
Rust's (+1 in `test_sweep.py`, +1 in `test_ci_sweep.py`); merge 2 5251,
merge 3 5264, merge 4 5266; the join tests +3, 5269 at the head. `cargo
test` counts 5357 against train 5's 5327 at `main`'s code: +30, the same
difference (12 + 13 + 2 + 3); with fault injection 5394 against 5364,
+30. `test-sweep.sh`'s 67 and 27 are GATE-GITENV's. The three join tests
and CTRLO-WIDER's heading test (f) are `ok` in both cargo runs
(`join-tests.txt`).

## Sweep

**Builds** (debug, as trains 4 and 5): the head `0252c06f` (`x𝒇x
v0.1.0-0252c06 [debug]` on row 1 of every xfx snap, sha256 `540f4d7a…`;
its code is `03476db7`'s, and `392ca22a` after it changes one test), built
in a `git clone --no-local`; `main` `690aa505` built in another. fx: the
v0.0.16 binary, sha256 `8e6cda2e…` before and after each sweep,
`FX_AUTO_UPGRADE=0`. `scripts/fxcap/sweep.sh --fx <fx> --pin-sha 8e6cda2e…
--xfx <bin> --jobs 2 --size 40x120 --only ...` (`sweep.sh`, as run but for
the oracle's path and the short `TMPDIR`, now passed in), each sweep inside
one hold of the suite lock, one after the other.

**Each binary with its own tree's harness**, as trains 4 and 5: the head's
from the train's worktree, `main`'s from its clone (`main`'s scenarios,
harness and ledger; GATE-GITENV changed `sweep.py` and `ci_sweep.py` only
in how they call git). `main`'s sweep, T-U4's own and CTRLO-WIDER's own
were then rescored with the train's ledger (`sweep.sh --rescore` on copies,
no capture taken), so a verdict that moves moved because a capture did.

**Selection, 141 scenarios** (`selection.txt`, 22 globs, each with why):
T-U4's round 3 selection whole (132 scenarios at the head: every
`p5-transcript-*`, the tool group's and Ctrl-O's, the call rows, MCP, the
cancel scenarios `interrupt*` and `question*`, `approvals*`, `toolgroup*`,
`resume-tools`, `read-failures`, `rules-read*`, ...); every `p5-ctrlo-wider-*`
(CTRLO-WIDER added all seven) and `h1-resize`; and `p5-lifecycle-retry`.
That covers every scenario a unit added or edited (`git diff --name-only
690aa505 HEAD -- scripts/fxcap/scenarios`: 19 files, each a scenario or a
fixture of one selected) and every scenario a ledger entry the train
changed names (26 entries), but three that name every scenario:
`session-record` (a re-point, which no sweep reads, as train 5 said) and the
two `'*'` causes the units deleted, `cancelled-marker` (T-U4) and
`ctrl-o-drawn-wider-raw` (CTRLO-WIDER). Their rows are the cancelled
marker's -- T-U4's swept set is the set of scenarios that write one -- and
rows holding one of the 43 scalars, which no scenario's input holds but
CTRLO-WIDER's seven and `p5-transcript-diffwrap2` (CTRLO-WIDER's README,
"Pre-existing scenarios"), all swept. GATE-GITENV(+F) and
FLAKE-CATALOG-EVENT changed nothing a sweep reads (no scenario, no drawing
code; GATE-GITENV's harness change is in how it calls git) and have no
sweep of their own.

**Result at the head** (`sweep-head.md`): 141 scenarios ran, **0 failed, 0
timed out**; 684 pairs, **472 pass** (69.0%: 14 identical, 458
normalized-identical), 15 approved divergences and 2 cascades, 195
unresolved; wall 50m46s, load 72.6 at its start (other workers' jobs).
`main`'s (`sweep-main.md`): 134 scenarios (CTRLO-WIDER's seven are not on
`main`), 661 pairs, 398 pass, 0 failed, 0 timed out; 398 as it scored too.

| family | `main` | **train** | T-U4's own | CTRLO-WIDER's own |
|---|---|---|---|---|
| `p5-ctrlo-wider-*` | - | **8 / 23** | - | 8 / 23 |
| `h1-resize` | 1 / 3 | **1 / 3** | - | 1 / 3 |
| the cancel scenarios (`p5-transcript-cancel`, `interrupt*`, `question*`) | 50 / 94 | **84 / 94** | 84 / 94 | - |
| `p5-transcript-*` | 45 / 107 | **51 / 107** | 51 / 107 | - |
| every scenario swept | 398 / 661 | **472 / 684** | 462 / 651 | 9 / 26 |

(pass / pairs, each sweep scored with the train's ledger; `families.txt`.)

### Against `main` (`compare-main.txt`, `screen-moves-main.txt`)

Over the 661 pairs both took: **398 → 464 pass; 67 verdicts moved, all
up; 0 down** ("passed on the reference, not on the train: 0"). The other 8
of the head's 472 are CTRLO-WIDER's scenarios' pairs, which `main` has not.

| pairs | `main` → train | what moved them |
|---|---|---|
| 62: `approvals-amend-limit` `end`, `interrupt` ×9, `interrupt-cmd` ×4, `interrupt-next-cmd`, `interrupt-next-question`, `interrupt-question` ×2, `interrupt2` ×7, `p5-input-question` `q2-cancelled`, `p5-mcp-live` ×3, `p5-toolstop-retint*` ×12, `p5-transcript-cancel` ×6, `question-field` ×2, `question-other` ×2, `resume-tools` ×4, `rules-read-auto` ×5, `toolgroup` ×2 | unresolved → normalized-identical | **T-U4's marker**: `main`'s rows read `■ Cancelled …` where fx's read `▪ Cancelled …` (`compare-main.txt`'s `left:` rows); at the head both `▪`. Exactly T-U4's own 62 (its `round3/sweep-compare.txt`, every pair `unresolved-defect -> normalized-identical`; `comm` of the two lists: none missing, none extra) |
| `p5-mcp-menu-new` `approve-confirm` | approved divergence → normalized-identical | **run-local**: the approval shows the stdio server's command, a path into the sweeping tree's `scenarios/`; `main`'s clone's path is longer, the row wraps (fx `    `, xfx `  ↳ `, the approved wrap); the worktree's does not wrap |
| `p5-mcp-menu-new` `back-list`, `list-remote` | unresolved → normalized-identical | **timing**: the remote server's state at the snap -- fx `retrying`, xfx `connecting` in `main`'s run; the same word on both at the head |
| `p5-mcp-verbs` `startup` | unresolved → normalized-identical | **timing**: in `main`'s run xfx's screen held nothing yet where fx's had its banner, composer and status rows (blank rows on xfx's side); T-U4 saw the same pair unresolved in its sweep at `71bd4f31` and passing in the next, at `1d52413d` (its README, round 3) |
| `rules-read` `granted` | normalized-identical → identical | **timing**: nothing left to normalize in the head's capture |

Rows that moved without a verdict move (`compare-main.txt`, "pairs that
gained a differing row", 23; `screen-moves-main.txt`, 240 of 469 moved
snaps only in clock-like or run-local text): T-U4's marker inside rows that
differ for another reason (`toolgroup` `long-*` ×4, `resume-tools`
`long-later`: `■ Cancelled file · …` → `▪ Cancelled file · …` against fx's
`└ Cancelled file`); T-U4's `p5-transcript-cancel` `full` (T-U4 gave xfx its
own turns and the live cancel's line on Ctrl-O: the head's 34 differing rows
are T-U4's own sweep's 34, unresolved on both); `p5-mcp-verbs` ×11 and
`p5-effort-fork-*` `status`/`doctor` (the sweeping tree's path in a row,
whose length moves what wraps and what aligns); `read-failures`
`live-ctrl-o-up1` (the same path, wrapped elsewhere); `interrupt-recovery`
`full` (a network notice's `1ms` against `5ms`); `p5-lifecycle-retry`
`r04a-esc-armed` (fx's `retrying request in 7s` against `6s`).

### Against each unit's own sweep (`compare-tu4.txt`, `compare-wider.txt`, `screen-moves-tu4.txt`, `screen-moves-wider.txt`)

**T-U4**: its round 3 head sweep (`1d52413d`'s build, the 132 scenarios;
`sweep-unit-tu4.md`, rescored: 462 / 651 as T-U4 scored it) against the
train's, over the 651 pairs both took: **462 → 462 pass; 3 verdicts moved,
up; 0 down.** The three are `p5-transcript-submit` `filled`, `second`,
`tall`, normalized-identical → identical: T-U4's run had xfx's elapsed `1s`
where fx's was `0s` (row 36, its README), the head's both `0s` -- timing.
16 pairs gained a differing row, each unresolved on both and each a
run-local or timed row (`p5-mcp-verbs` ×11 and `p5-effort-fork-*` ×3: the
tree's path; `read-failures` `live-ctrl-o-up1`: the path, wrapped elsewhere;
`interrupt-recovery` `full`: the network notice's milliseconds).
`screen-moves-tu4.txt`: 272 of 411 moved snaps only in clock-like or
run-local text; of the rest, the xfx-only moves are resume ids and build
ids (`1d52413d5699` → `0252c06f0eee`) the masks do not catch, the scratch
path, and `p5-mcp-verbs`' server list caught in another order.

**CTRLO-WIDER**: its own F head sweep (`9730ac86`'s code, banner
`v0.1.0-b7cf010`: the seven scenarios and `h1-resize`, with
`p5-ctrlo-wider-fence` from its realigned rerun -- its `sweep-f-head.md` and
`sweep-f-fence-head.md`; rescored: 9 / 26 as CTRLO-WIDER scored it;
`sweep-unit-wider.md`, `sweep-unit-wider-fence.md`) against the train's,
over the 26 pairs: **9 → 9 pass; 0 verdicts moved.** Three pairs gained a
row: `p5-ctrlo-wider-ri` `answer`, `erased`, `typed`, where **fx's** row is
`  ri 🇿🇺🇸 end  )` and xfx's `  ri 🇿🇺🇸 end` -- fx's own leftover `)` when
its first frame lands on `• Thinking (0s)`, which CTRLO-WIDER recorded in
four of five fx runs (its README (e); `ri-run-pairing`); unresolved on both.
`screen-moves-wider.txt`: 10 of 13 moved snaps only in clock-like text
(Ctrl-O's entry times), the other 3 those fx rows.

So no pair that passes on `main` or on either unit's own sweep fails on the
train.

## Redaction

The repository is public. Every file here taken from a sweep, a gate log, a
probe, the logger or a container went through `redacted.py` (train 5's:
`scripts/fxcap/redact.py`'s `Redactor.write`, every path under a local
root to `<local-path>` or a named root such as `<scratch>`, `<sweeps>`,
`<repo>`; a checkout's path cut short to `<checkout>/`), and `compare.py`
and `screen_moves.py` redact each row whole before cutting it (train 5's
fix). The scripts here take the oracle's path and the scratch directories
as arguments or variables (`sweep.sh`: `FX`, `SHORT_TMP`; `linux.sh` writes
its logs under the container user's `$HOME`, where the run wrote them to
the container's temporary directory). Then, over every file of this
directory, for the four forms train 5 named -- the uid fragment before a
slash, the machine's scratch root's name, the system temporary root and the
home root, each spelt with a bracket class so the command matches what it
looks for and not its own text -- and the account's name: **0 hits**
(`redaction-grep.txt`).

## Files

- `remerge-1.txt` .. `remerge-4.txt`: `git show --remerge-diff` of each merge.
- `rowmerge.py`: how merges 2 and 3 resolved `docs/parity.md`'s two rows,
  word by word against the base.
- `overlap.py`, `overlap.txt`: every file both sides of a merge changed, and
  the nearest pair of hunks.
- `token_check.py`, `token-check.txt`, `token-self-test.txt`: train 5's token
  check with the clause rule added, and its negative control with the
  both-sides cases added.
- `refs.py`, `refs.txt` (merge 4), `refs-head.txt` (the head): the
  reference map, five sides; `refs_all.py`, `refs_short.py`,
  `refs-all-merge4.txt`, `refs-short-merge4.txt` (merge 4), `refs-all.txt`
  (the head, both forms); `repointed.py`, `repointed.txt`: the 13 re-points
  checked against their code; `stale.py`, `stale.txt`: the eleven references
  still flagged outside the plans, each beside the code it names on `main`.
- `join_units.sh`, `apply_join.py`, `join-units.txt`: the three join tests,
  their text unchanged, on each unit's tip alone (at `392ca22a`;
  `join-units-first.txt` at `03476db7`); `join_mutants.py`,
  `join-mutants.txt`: the join tests against seven mutants on a clone of
  the head (at `392ca22a`; `join-mutants-first.txt`, the run JM6 survived);
  `join-head-r2.txt`: clippy, fmt and the touched tests at `392ca22a`.
- `probe/`: `render_cost_probe.rs` (T-U4's round 3 probe, the `wider` case
  and `XFX_PROBE_DETAIL` added), `run.sh`, `probe.txt` (both clones, both
  cases, both modes, two rounds).
- `net.py`, `net.txt`, `connect.log`: join check 4.
- `linux.sh`, `linux.txt`: the Linux row of CI's script tests, in a
  container.
- `gates.sh`, `gates_txt.py`, `gates.txt`: the gate as it ran, and how
  `gates.txt` was assembled; `gates-final.txt`: the file checks with the
  receipt in its final form.
- `test-counts.txt`: `#[test]` and the harness's `def test_` per commit.
- `selection.txt`: the sweep's `--only` globs, each with why; `sweep.sh`:
  how each sweep ran.
- `families.py`, `families.txt`: pass per family the brief names, in each
  sweep.
- `sweep-head.md`, `sweep-main.md` (rescored with the train's ledger),
  `sweep-unit-tu4.md`, `sweep-unit-wider.md`, `sweep-unit-wider-fence.md`
  (each unit's own head sweep, rescored); `compare.py`, `compare-main.txt`, `compare-tu4.txt`,
  `compare-wider.txt`: pair by pair; `screen_moves.py`,
  `screen-moves-main.txt`: the screens that moved, clock-like and run-local
  text set aside.
- `redacted.py`: every sweep- and gate-derived file here went through
  `scripts/fxcap/redact.py`'s `Redactor.write`; `redaction-grep.txt`: the
  grep over this directory, its command and its count.
