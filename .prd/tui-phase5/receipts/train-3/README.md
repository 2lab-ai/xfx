---
title: 0.0.16 delta -- P5 train 3 receipt (DIFF-WRAP, ENGINE-VS-ANCHOR, TG-REDRAW into one branch)
type: receipt
tags: [xfx, phase5, fx-0.0.16, train]
---

# P5 train 3: three closed units, one branch

Branch `train/p5-1107c`, from `origin/main` `e51685d6` (PR #77, as the
brief expected); `origin/main` `6c93f359` (PR #78, SECRETS-ARGMAX) merged in
after the three unit merges, at the dispatcher's word. Every unit was closed by its own two-engine review; this
train combines them and re-designs nothing. No source line was changed to
make them meet: the merges are git's own, the one test added is the join
check both reviews asked for, and it passed on the merged code at its first
run.

Oracle: fx v0.0.16 (`481104fd`, executable sha256
`8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked
by `capture.sh` before and after every run, `FX_AUTO_UPGRADE=0`). `sweep.sh`
takes the executable's sha256 as `--pin-sha`, not the commit.

## Merges

Each is `git merge --no-ff`, in this order. All three units forked from
train 2's round-3 head `58b463c8`, which is git's merge base for each.

| # | unit | branch @ tip | merge | base | conflicts |
|---|---|---|---|---|---|
| 1 | DIFF-WRAP + b/c/d | `feat/p5-ctrlo-diff-wrap` @ `82442e28` | `b095ffe7` | `58b463c8` | none |
| 2 | ENGINE-VS-ANCHOR + b | `feat/p5-engine-vs-anchor` @ `8d802907` | `23fd093c` | `58b463c8` | none |
| 3 | TG-REDRAW + b/c | `feat/p5-toolgroup-redraw` @ `ec390074` | `116c7b10` | `58b463c8` | none |
| 4 | `origin/main` (#78, SECRETS-ARGMAX) | `6c93f359` | `a1a1524c` | `e51685d6` | none |

Between merges 3 and 4: `ebba626d` (the join test, below), `b4e0d06e` (the
ledger's moved refs, below) and `cc686a4c` (this receipt, first part); after
merge 4, the receipt's last commits. The sweep's binary is `b4e0d06e`'s:
`git diff b4e0d06e <head> -- src tests build.rs Cargo.toml Cargo.lock` is
empty, merge 4 included (#78 changes scripts outside `scripts/fxcap`, CI and
docs only).

**No textual conflict anywhere.** `git show --remerge-diff` of each merge
(`remerge-1.txt` .. `remerge-4.txt`) has an empty body: each merge commit is
git's automatic result, unchanged.

- **Merge 1, DIFF-WRAP.** `src/tui/full_transcript.rs`,
  `.prd/tui-phase5/receipts/width-disputed/check.py`, the ledger
  (`ctrl-o-diff-row-clipped` deleted; `filler-modifier-count`'s `fx_row`
  takes the `5678` continuation row) and the `full transcript (C-o)` row of
  `docs/parity.md`. `main` changed four other rows of `docs/parity.md` and
  other ledger entries since the base.
- **Merge 2, ENGINE-VS-ANCHOR.** `grid.rs`, `frame.rs`, `check.rs`,
  `scripts/smoke-tui.sh`, the ledger (`frame-variation-selector-anchor`
  deleted). DIFF-WRAP touched none of those files but the ledger, in other
  entries.
- **Merge 3, TG-REDRAW.** `grid.rs` and `frame.rs` are the two files both
  ENGINE-VS-ANCHOR and TG-REDRAW changed, and they auto-merged
  (`overlap.txt`, every hunk of both sides against the base):
  - `grid.rs`: the nearest hunks are 4 unchanged base lines apart --
    TG-REDRAW's line 39 (the `use super::theme::{...}` import, `Redraw`
    added) against ENGINE-VS-ANCHOR's 30-34 (the module doc's alphabet) and
    its insertion after 43 (the `CLEAR_TWO`/`AUTOWRAP_*` constants). Next
    nearest: 10 lines (ENGINE-VS-ANCHOR's `put_unit`/`write_cell`/
    `write_row_text` inserted after 188, TG-REDRAW's `Grid` struct at
    199-202). The opus review's "no textual overlap" holds.
  - `frame.rs`: 91 unchanged base lines at the nearest.
  - The merge commit's message says "within four lines"; the exact figure is
    the one above (`overlap.txt`), and `remerge-3.txt` is the evidence that
    nothing was resolved by hand.
  - `theme.rs`, `toolgroup.rs`, `transcript.rs`, `event_loop.rs`,
    `shell.rs`, the ledger (`toolgroup-retint-256` deleted,
    `toolgroup-ctrlo-record-short` added, `truecolor-grey-notation`
    widened) and the `colored and hyperlinked TTY output` row merged clean.
- **Merge 4, `origin/main` `6c93f359`.** Its base is the train's own,
  `e51685d6`. `main`'s side: `scripts/check-no-secrets.sh` (batched under
  `ARG_MAX`), `GREP_OPTIONS` unset in `check-no-stubs.sh`,
  `check-preview-contract.sh`, `check-tui-preflight-cost.sh`,
  `check-usage-clock.sh`, `test-preview-contract.sh`, two new scripts
  (`test-check-no-secrets.sh`, `test-gate-env.sh`), `ci.yml`,
  `CONTRIBUTING.md`, `README.md`, `.prd/02-architecture.md` and its receipt.
  The train touches none of those files; no ledger entry, no
  `docs/parity.md` row, no compiled file.

**Historical scripts that still name a closed cause.**
`width-disputed/check.py` (a comment), `ctrlo-diff-wrap-b/check.py` and
`ctrlo-diff-wrap-c/check.py` (`"covered": {"frame-variation-selector-anchor",
...}`) name ENGINE-VS-ANCHOR's cause, which this train's ledger no longer
has. They are the receipts of their own heads and are not gates here. Run on
the train's captures anyway, DIFF-WRAP-c's fails that one claim for that
reason alone (Sweep, below).

## The token check (`token_check.py`, `token-check.txt`)

For each merge, against git's merge base:
- `docs/parity.md` row by row (a table row keyed by its first cell): the
  merged row's whitespace tokens, as a multiset, are exactly the base's plus
  what each side added minus what each side deleted -- every token a side
  added is in it, and no token a side deleted came back.
- The ledger entry by entry (`[[table]]` keyed by `id`, every field): an
  entry one side changed is that side's, one a side deleted is gone, one a
  side added is in, the rest are the base's; and every comment line a side
  added is in and every one a side deleted is not.

Result: all four merges OK. No row and no entry was changed by both sides,
so every changed row is exactly its one side's (merge 1: `/image`, `/paste`,
`MCP server tools`, `profile home ~/.xfx` from `main`, `full transcript
(C-o)` from DIFF-WRAP; merge 3 adds `colored and hyperlinked TTY output`
from TG-REDRAW; merge 4 changes none: `main`'s side touched neither file). The ledger: `ctrl-o-diff-row-clipped`,
`frame-variation-selector-anchor` and `toolgroup-retint-256` deleted by
their units and gone; `toolgroup-ctrlo-record-short` added and in;
`filler-modifier-count` and `truecolor-grey-notation` as their units changed
them; `main`'s five re-pointed entries as `main` has them.

**The negative control** (`--self-test`, in the same file): on each merge's
own texts, one token a side added dropped from its row, one token a side
deleted put back, an added ledger entry dropped, a deleted one restored, a
comment line dropped -- 17 cases over the four merges, 17 found.

`docs/parity.md` is 288 lines at the base, on `main`, on every unit tip and
at the head, and every side's change is a row rewritten in place, so the
ledger's `docs/parity.md:<line>` citations still name the same rows (237
`full-screen TUI`, 238 `full transcript (C-o)`, 241 `colored and
hyperlinked TTY output`): `refs.py` reads each by its row (`= row`).

## The ledger's moved refs (`b4e0d06e`; `refs.py`, `refs.txt`, `repointed.py`, `repointed.txt`)

`refs.py` takes every `path:line` reference the ledger and `docs/parity.md`
hold at the last merge's head (`ebba626d`, 66 of them) and maps each one
from every side's copy of the cited file to the head's (`git diff -U0`); a
reference a side's own text cites whose line moved is `MOVED`. Seven did,
all into the two files TG-REDRAW grew above them. `repointed.py` then
checks each re-point against the code the entry was written about, in the
frame it was written in -- the cited line and two on either side, the same
text at the new line: 7 of 7.

| entry | cited (frame) | now | moved by |
|---|---|---|---|
| `submit-multi-card-whole` (why; `send`) | `src/tui/shell.rs:12267` (`main`) | `:12275` | TG-REDRAW (+8 above it) |
| `session-record`, `transcript-page-offset`, `safe-page-edge`, `full-transcript-shift` | `src/tui/shell.rs:37356` (`main`) | `:37458` | stale in `main`'s own frame: `main`'s `:37356` is a test's `let (said_more, unsaid) = ...`; the `* session:` record's `"  tools: {} advertised ({},"` is `main`'s `:37440`. `:37356` is where the record stood on train 2's head `24a6b912`; the L-U3 merge `f83d96be` pointed the ledger there, and L-U3's own lines above it had moved it to 37440 in the merged file. Then TG-REDRAW (+18 above it) |
| `toolgroup-ctrlo-record-short` (TG-REDRAW's own) | `src/tui/shell.rs:37272` (TG-REDRAW) | `:37458` | stale in its own frame: copied from its base's siblings; TG-REDRAW's `:37272` is a comment, its record line `:37374`. Pointed at its siblings' line |
| `cancelled-marker` (xfx) | `src/tui/toolgroup.rs:618` | `:711` | stale on `main` by 4 since train 2's `5f2d0b88` (written at `97644085`, where `:618` is the stopped call's `\u{25a0} ... Cancelled` line), then TG-REDRAW (+89) |
| `call-row-raw-argument` (source), `call-row-page-edge` (xfx) | `src/tui/toolgroup.rs:657` | `:750` | the same (`pub(crate) fn spelled`) |
| `call-row-source-cut` (source) | `src/tui/toolgroup.rs:705` | `:798` | the same (`fn command_spelled`) |
| `call-row-arabic-letter-mark` (source) | `src/tui/toolgroup.rs:819` | `:912` | the same (the U+061C arm) |

No other reference moved, and merge 4 moved none (`refs-head.txt`: the 65
references at the merge-4 head mapped from the train before it and from
`main` `6c93f359`, 0 moved; `repointed.py` at that head, 7 of 7 again).
`src/tui/full_transcript.rs:492`
(`ctrl-o-command-marks`) is in the file DIFF-WRAP grew, below it; no
reference points into `grid.rs`, `frame.rs`, `check.rs`, `theme.rs`,
`transcript.rs` or `event_loop.rs`. No entry's text other than the line
number changed.

## The join: a tool group's selector rows drawn again (`ebba626d`)

TG-REDRAW's retint (`Band::retint_document`, `frame.rs`) gives a tool
group's row the attributes its drawing in the new mode puts on each cell
(`Grid::retint_document` → `redraw_row`), diffs the shadow against that
(`Grid::diff` → `diff_from`), which ENGINE-VS-ANCHOR's merge makes write
each cell through `put_unit` and `write_cell`, and hands the vector to
`check::preflight` before anything is written. Two tests, as both reviews
asked:

- `tui::event_loop::tests::a_tool_rows_selector_glyph_is_drawn_again_in_the_frame_writers_bracket_and_accepted`
  -- the whole path, from `UiEvent`s through the shell, the transcript, the
  band and the screen. One group of three command calls whose rows each hold
  a variation-selector unit, on an 80-column screen:
  - `├ Ran echo "⚡️" 42 # note`: U+26A1 U+FE0F, two cells, mid-row (columns
    13-14), inside the highlighter's string;
  - `├ Ran echo "☀"️ x`: a U+FE0F after the string's closing quote, which
    the grid joins to the quote's one cell across the highlighter's colour
    sequences -- a split cell from the real highlighter (column 14), whose
    two parts read back as one cell written together;
  - `└ Ran echo x…x⚡️`: U+26A1 U+FE0F in columns 79-80, the right margin.

  At 256 colours and at truecolor, dark to light and back to dark, each
  switch's vector is accepted (`owes_retint()` is false after the tick: the
  debt is paid only by a vector that landed, and the band records the screen
  only after the write), each row is cell for cell what a fresh landing of
  the group in that palette leaves (a second session switched to light
  before the group lands; the first landing for the way back), and the bytes
  are fx's frame writer's: `ESC[2X⚡️ESC[15G` mid-row, `"️ESC[15G` after the
  joined one-cell unit, `ESC[?7hESC[2X⚡️ESC[?7l` with no anchor at the
  margin. The landing itself wrote the rows as they stand (no `ESC[2X`:
  `place`, fx's document append), so the bracket is the retint's.
- `tui::frame::tests::a_tool_rows_split_selector_cell_is_drawn_again_split_anchored_and_accepted`
  -- `☀` ESC U+FE0F, which no command tried while writing the test got out
  of the highlighter (`☀` and U+FE0F stayed in one run, or the selector
  followed a quote, as above), on two tool-group rows appended
  with their kinds and drawings (`Recorded`, what `commit_document` hands a
  block's append): mid-row (column 12)
  and in the last column (80). Through `Band::retint_document` both ways:
  `Retint::Settled(Commit::Painted)`, the cell written split with its pen
  between (`☀ESC[38;5;241m U+FE0F`, then `ESC[13G` mid-row; no anchor and no
  bracket at the margin, a one-cell unit), and both rows equal to a fresh
  paint of the drawing in that mode.

**Result: both pass, at the first run on the merged code. No source line
changed.** `fmt` and both clippies clean with them.

**Every side's tests are in.** `#[test]` attributes under `src` and `tests`:
base 5071; `main` +7, DIFF-WRAP +10, ENGINE-VS-ANCHOR +29, TG-REDRAW +21,
and the two join tests: 5071 + 69 = 5140 at the head (`git grep -c`).

**Do they discriminate?** `mutants.py`, on a `git clone --no-local`
throwaway of the train branch, each mutant one exact replacement in
`grid.rs`, the two tests run against it (`mutants.txt`): 6 of 6 caught --

| mutant | caught by |
|---|---|
| A: no `CHA` after a selector unit | both |
| B: no `?7h`/`?7l` at the margin | event-loop |
| C: no `ECH 2` before a two-cell selector unit | event-loop |
| D: a split cell written together | frame (the check refuses the vector) |
| E: the retint ignores a row's drawings (colour matching only) | both |
| F: the diff writes a cell bare, not through `put_unit` | both |

## Gates

Two runs, each gate one at a time, `TMPDIR` a short scratch directory;
`gates.txt` holds both as they printed, with the totals read from the full
logs. The machine was shared (other sessions' builds and sweeps).

**Run 2, the head: `e1e839cd`** (merge 4 in, the receipt committed, the
worktree clean), with `main`'s two new gate tests:

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| the same `--features fault-injection` | ok |
| `cargo test --locked --no-fail-fast` | 5226 passed, 0 failed, 7 ignored (15 binaries) |
| `cargo test --locked --features fault-injection --no-fail-fast` | 5263 passed, 0 failed, 7 ignored |
| `scripts/check-xfx-identity.sh` / `check-no-stubs.sh` | ok (12549 tracked files) / ok |
| `scripts/check-preview-contract.sh` / `test-preview-contract.sh` | ok / ok (24 mutants caught) |
| `scripts/check-no-secrets.sh` | ok (12549 tracked files) |
| `scripts/test-check-no-secrets.sh` (new, #78) | ok (13 cases; 3473 tracked names past ARG_MAX) |
| `scripts/test-gate-env.sh` (new, #78) | ok (5 cases) |
| `scripts/fxcap/test-sweep.sh` / `test-capture.sh` | 50 + 26 OK / 42 OK |
| `scripts/check-tui-preflight-cost.sh` / `check-usage-clock.sh` | ok (200x300 full 1.00 ms, update 0.65 ms; budget 32 ms) / ok |
| `cargo build --locked --release`; `--release --features fault-injection --target-dir target/faulty` | ok; ok |
| `scripts/smoke.sh target/release/xfx <dir>` | 47 checks, 0 failures |
| `scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx <dir>` | 37 scenarios and the oracle, 1127 checks, 0 failures |

With this README and `gates.txt` in their final form staged on top of
`e1e839cd` (the last receipt commit changes only those two files):
`cargo fmt --check` ok, `check-xfx-identity.sh` ok (12549 tracked files),
`check-no-stubs.sh` ok, `check-no-secrets.sh` ok (12549 tracked files).

**Run 1, `b4e0d06e`, before merge 4** (the receipt directory untracked
beside it): every gate above passed too (`cargo test` 5226 and 5263 again;
12495 tracked files), but for the two new tests (not on the branch yet) and
`check-no-secrets.sh`, which **failed closed**:
`/usr/bin/grep: Argument list too long`, once per pattern (9). That
script then handed every tracked file, as `./<name>`, to one `grep` per
pattern (`scripts/check-no-secrets.sh:205` at `b4e0d06e`), and the three
units' receipts carried the listing past `ARG_MAX` (1048576 here). Measured
as each name plus `./`, its NUL and an 8-byte pointer: `main` `e51685d6`,
12090 files, left 27991 bytes; the train at `b4e0d06e`, 12495 files, was
15230 bytes over, before the environment and `grep`'s own arguments. The
script was not edited here and no receipt file was removed; merge 4 brings
SECRETS-ARGMAX's batched check, and run 2 passes. Its name rules (`.env`
files, bytecode) found nothing in run 1 (its 9 problems were the 9 `grep`
failures). The supplement run then (`secrets_batched.py`,
`secrets-batched.txt`: the script's own nine patterns over every tracked
file in batches of 2000 names, each pattern first finding a sample of its
own shape) found 9 controls and 0 hits.

## Sweep

**Builds.** Head: `b4e0d06e`, a debug build (`x𝒇x v0.1.0-b4e0d06 [debug]` on
row 1 of every xfx snap); the receipt commit after it changes no compiled
file. `main`: `e51685d6`, built in a `git clone --no-local` scratch clone
(`v0.1.0-e51685d`). fx: the v0.0.16 reference binary, sha256 `8e6cda2e…`,
`FX_AUTO_UPGRADE=0`. Both sweeps ran from the train's worktree, one after the
other, so they share the train's scenarios and ledger: `scripts/fxcap/
sweep.sh --fx <fx> --pin-sha 8e6cda2e… --xfx <bin> --jobs 2 --only ...`,
40x120.

**Selection, 75 scenarios.**
- The brief's four globs: `p5-transcript-*`, `p5-toolgroup-*`,
  `p5-toolstop-*`, `p5-inline-code-*` (44).
- The scenarios of every ledger entry the train touched, the ones these
  globs do not hold: `p5-call-row-*` (`call-row-raw-argument`,
  `-page-edge`, `-source-cut`, `-arabic-letter-mark`, re-pointed) and
  `fulltranscript*`, `links`, `read-failures`, `resume-tools`, `timeout`
  (`full-transcript-shift`, re-pointed) (8). The others' scenarios are in
  the globs (`submit-multi-card-whole`, `transcript-page-offset`,
  `safe-page-edge`, `toolgroup-ctrlo-record-short`, `filler-modifier-count`,
  `truecolor-grey-notation`).
- ENGINE-VS-ANCHOR's own selection, every scenario whose captures hold a
  variation selector (its README): `composer-*`, `p5-input-*`, `fast-*`,
  `model-column*`, `model-speed`, `p5-session-early-prefs` (23). Its rule
  writes every grid diff and every row painted whole, so its scenarios are
  the ones its merge can move outside the globs.
- **Not all 314.** `session-record`, `cancelled-marker` and `stats-bytes`
  name `scenarios = ["*"]`, and `session-record` was re-pointed. A
  re-point changes a line number in `xfx = [...]`, which the sweep does not
  read: it changes no selector, so it can move no verdict. What can move one
  is the code, and the code's reach is the selection above.

**References.** Each unit's own head sweep, as its receipt scored it: the
`pairs.json` and captures beside the `sweep-head` the receipt holds, matched
to it by the xfx banner and sha256 and the ledger sha256 in both scoreboards
(DIFF-WRAP `v0.1.0-3a24386`, `8caa1f2b…`, ledger `eb7bafc1…`;
ENGINE-VS-ANCHOR `v0.1.0-a66155c`, `8eda29bb…`, ledger `b273f183…`;
TG-REDRAW `v0.1.0-f482d34`, `4cb8117e…`, ledger `2a6c1fb7…`, plus its round-3
`p5-toolgroup-ctrlo*` re-sweep, ledger `9abab070…`). Each swept build is its
unit's code: the commits after it are tests and comments only (DIFF-WRAPd: a
test and a doc comment in `full_transcript.rs`; E-V-A-b: tests in
`check.rs`; TG-REDRAWc: tests in `event_loop.rs` and `transcript.rs`). Every
reference sweep was rescored with the train's ledger (`sweep.sh --rescore`)
before comparing, so a verdict that moves moved because a capture did, not
because a ledger did. One consequence: a unit's captures of a row another
unit fixed are scored under the train's ledger, where the other unit's cause
is gone, so they read as uncaused rows on the reference side.

**Result at the head** (`sweep-head.md`): 75 scenarios ran, 0 failed, 0
timed out; 347 pairs, **178 pass** (51.3%), 165 unresolved, wall 24 min,
load 15 at its start and 104 at its peak (other sessions' work, its last
minutes).

### Against `main`

`main` `e51685d6`'s build on the same selection (`sweep-main.md`): 347
pairs, 167 pass, 177 unresolved, 0 failed or timed out, wall 26 min. It ran
right after the head's, at load 100 at its start (other sessions' work;
`uptime` read 7.6 two-thirds of the way through). `6c93f359`, merged in since, changes no compiled file,
so this is `main`'s code.

`compare-main.txt`, pair by pair over all 347: **167 → 178 pass, 0
dropped**; 12 verdicts moved, every one up, and every one a unit's code:

| pair(s) | `main` → train | what moved it |
|---|---|---|
| `p5-toolgroup-retint` `light`, `dark-again`; `-retint-light` `dark`, `light-again` | unresolved → normalized-identical | TG-REDRAW: the four clash snaps, its receipt's `toolgroup-retint-256` cells |
| `p5-toolgroup-live` `running-light`, `dark`; `p5-toolgroup-ctrlo` `light`, `dark-again` | unresolved → normalized-identical | TG-REDRAW: the four its receipt names on its new scenarios |
| `p5-transcript-diffwrap` `full`, `-diffwrap2` `full`, `-diffwide` `full` | unresolved → normalized-identical | DIFF-WRAP's reflow (`-diffwrap2` with its tab and mark rules, b/c/d) |
| `p5-transcript-diffwide2` `full` | unresolved → approved-divergence | DIFF-WRAP's reflow and ENGINE-VS-ANCHOR's anchor: every row left is `filler-modifier-count`'s (below) |

Two pairs changed differing rows without a verdict move:
- `p5-transcript-diffwide2` `full`: the four `filler-modifier-count` rows,
  now aligned to fx's at the same row numbers (as against the units' sweeps,
  below).
- `read-failures` `live-ctrl-o-up1` (unresolved on both, causes
  `session-record` and `full-transcript-shift` on both): the "new" rows are
  the same rows, whose fx text names a path under the sweep's own scratch
  directory, a fresh random name per run (written `<run-dir>` in
  `compare-main.txt`). Not the code.

Screens that moved without a verdict (`screen-moves.txt`, its last block):
204 snaps differ, 189 only in clock-like text; the other 15 are the resume
id a saved session prints (a fresh one each run: `fulltranscript` ×3,
`resume-tools` ×4, `p5-transcript-diffask`), fx's own transient `press
ctrl+c again to exit` on `p5-toolstop-retint-light` `light` and its `-tc`
twin (fx's screen; TG-REDRAW's receipt names the same hint), the launch
notice on `p5-transcript-resize` `start`, and the four Ctrl-O diff pages
whose rows DIFF-WRAP's reflow moves (`diffwide`, `diffwide2`, `diffwrap`,
`diffwrap2` `full`).

### Against each unit's own head sweep

Pair by pair over the pairs both took (`compare.py`, `compare-<unit>.txt`;
the rescored references in `sweep-unit-<unit>.md`). "Pass" is identical or
normalized-identical, the sweep's own measure.

| unit (reference build) | shared pairs | pass, unit → train | verdicts moved | dropped |
|---|---|---|---|---|
| DIFF-WRAP (`3a24386`, `p5-transcript-*`) | 107 | 45 → 45 | 2 | **0** |
| ENGINE-VS-ANCHOR (`a66155c`, its 25) | 124 | 92 → 93 | 2 | **0** |
| TG-REDRAW (`f482d34`, its 42) | 169 | 63 → 64 | 2 | **0** |
| TG-REDRAW round 3 (`p5-toolgroup-ctrlo*`) | 12 | 3 → 3 | 0 | **0** |

Every move, and what moved it:
- **`p5-transcript-diffwide2` `full`**, unresolved → approved-divergence,
  against all three. **The join of DIFF-WRAP and ENGINE-VS-ANCHOR on one
  snap.** At the head every row one side holds alone is
  `filler-modifier-count`'s, the approved divergence (rows 14-15 and 28-29,
  line 5's read and diff rows and their continuations); nothing else
  differs.
  - Against DIFF-WRAP the two rows that left are line 2's read and diff rows
    (`halfﾠ︎   x zwj…`): ENGINE-VS-ANCHOR's anchor, now in the build. At
    DIFF-WRAP's head they were `frame-variation-selector-anchor`'s, which
    the train's ledger no longer has, so its rescored sweep reads them as
    uncaused.
  - Against ENGINE-VS-ANCHOR and TG-REDRAW the rows that left are
    `ctrl-o-diff-row-clipped`'s, DIFF-WRAP's reflow now in the build. The
    four "new" differing rows `compare.py` prints for this pair are
    `filler-modifier-count`'s same four rows, now aligned to fx's at the
    same row numbers; at those heads xfx's clipped rows stood against other
    fx rows.
  - ENGINE-VS-ANCHOR's own checker on the train's captures (`unit-checks.txt`):
    4 claims, 4 hold -- `anchor`: line 2's read row and diff row, every cell
    the same as fx's; `aligned`: 37 rows the same and the 8 rows one side
    holds alone all `filler-modifier-count`'s (at its own head
    `ctrl-o-diff-row-clipped`'s were among them).
  - DIFF-WRAP-c's checker on the same captures: 20 claims, 19 hold. The one
    that does not is `full p5-transcript-diffwide2`, which demands that each
    cause it names covers some differing row (`seen == covered`, its
    `check.py:120`) and names `frame-variation-selector-anchor`: at the head
    that cause is gone and the rows it covered (9 and 23) are the same as
    fx's. Every row that does differ is covered (`filler-modifier-count`):
    the claim was written for a head without ENGINE-VS-ANCHOR, and the join
    is what fails it.
- **`p5-transcript-diffwide` `full`**, unresolved → normalized-identical,
  against ENGINE-VS-ANCHOR and TG-REDRAW: DIFF-WRAP's reflow (at their heads
  `ctrl-o-diff-row-clipped`, gone).
- **`p5-transcript-submit` `submit-0.15`**, identical → normalized-identical,
  against DIFF-WRAP; a pass both ways. **Timing**: fx's screen is the same in
  both sweeps; xfx's row 35 caught the activity row's mark in its other
  blink phase (`  Generating (1s) (↑4 ↓2)` where DIFF-WRAP's capture has `•
  Generating ...`), which the `activity-pulse` normalization reads.

No pair that passes on a unit's sweep fails on the train's, and no pair
gained a differing row but `diffwide2` `full`'s realigned four.

**Screens that moved without a verdict** (`screen_moves.py`,
`screen-moves.txt`): of the snaps whose screen differs between a unit's
sweep and the train's (row 1, the banner, left out), all but these differ
only in clock-like text (Ctrl-O's timestamps, durations, latencies):
- `p5-transcript-diffwide2` `full` rows 9 and 23 (DIFF-WRAP's reference: the
  anchor) and, against ENGINE-VS-ANCHOR and TG-REDRAW, `diffwide` and
  `diffwide2` `full` from row 2 (the reflow puts the page's rows where fx's
  are);
- `p5-transcript-diffask` `ask` row 24: the saved session's resume id, a
  fresh one each run;
- `p5-transcript-records` `start` and `p5-transcript-resize` `start` row 5,
  against DIFF-WRAP: the transient `Full access enabled` launch notice, on
  one capture and not the other (DIFF-WRAPb's
  `../ctrlo-diff-wrap-b/records-reruns.txt` coin flip). Both pairs keep
  their verdicts.

**TG-REDRAW's own checkers on the train's captures** (`unit-checks.txt`):
`toolrows.py` on the six Ctrl-O pages of `p5-toolgroup-ctrlo` and `-tc`:
the group's header and call row the same as fx's, cell by cell with the ramp
reading, 12 of 12 (its `ctrlo-rows.txt`: 12 of 12). `ctrlo_records.py` on the
three `-tc` pages: 22 rows a page the same, cell by cell, and fx's two
argument rows (`│  action: run`, `│  yield_time_ms: 30000`) with no xfx row,
as its `ctrlo-records.txt` reads them (`tool-schema`).

## Files

- `remerge-1.txt`, `remerge-2.txt`, `remerge-3.txt`: `git show --remerge-diff`
  of each merge.
- `overlap.txt`: ENGINE-VS-ANCHOR's and TG-REDRAW's hunks in `grid.rs` and
  `frame.rs` and the nearest pairs.
- `token_check.py`, `token-check.txt`: the token check and its negative
  control.
- `refs.py`, `refs.txt`: every `path:line` reference mapped from each side to
  the last merge's head; `repointed.py`, `repointed.txt`: the seven re-points
  checked against their code.
- `mutants.py`, `mutants.txt`: the join test's mutants.
- `gates.txt`: the gates, as they ran.
- `secrets_batched.py`, `secrets-batched.txt`: the secret patterns over every
  tracked file in batches, the supplement while the gate fails on E2BIG.
- `compare.py`: two sweeps pair by pair over the pairs both took
  (ENGINE-VS-ANCHOR's, for a subset); `screen_moves.py`: the snaps whose
  screen moved between two sweeps, clock-like text set aside.
- `sweep-head.md`: the head's scoreboard; `sweep-main.md`: `main`'s, on the
  same selection; `compare-main.txt`: the two pair by pair.
- `sweep-unit-diff-wrap.md`, `-engine-vs-anchor.md`, `-toolgroup-redraw.md`,
  `-toolgroup-ctrlo-r3.md`: each unit's own head sweep rescored with the
  train's ledger; `compare-<unit>.txt`: each against the head's;
  `screen-moves.txt`: the screens that moved, per reference.
- `unit-checks.txt`: ENGINE-VS-ANCHOR's `check.py`, DIFF-WRAP-c's `check.py`
  and TG-REDRAW's `toolrows.py` and `ctrlo_records.py` on the head's
  captures.

Every sweep-derived file was written through `scripts/fxcap/redact.py`
(`Redactor.write`, which refuses a text with a local path left in it).
