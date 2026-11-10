# LOCAL-PATHS-WRAP: a root wrapped behind a continuation mark

Train 12's CI round 4 found it (`../train-12/ci-mcp-timing/README.md`, "Open,
for LOCAL-PATHS"; `gutter-check.txt`): `scripts/fxcap/roots.py` read a root
across a row's end only behind spaces, tabs and U+2500-U+259F, and xfx wraps
a long command line behind `↳` (U+21B3). A trial with a 118-byte scenario
directory published 116 of its bytes on one row and 2 on the next, and
`roots.py --check` passed it. A longer TMPDIR or scenario path on any
machine would have put a home or scratch path into a committed capture.

Base `635e7242` (train 12's head, with LOCAL-PATHS). Branch
`fix/p5-local-paths-wrap`. Every claim below names the file in this
directory that backs it; no file here holds a path of a machine. Round 2
answers the reviews of `4851ba05` (section 8), round 3 gpt6's of `ff59f0f7`
(section 9), round 4 both engines' of `76fbee40` (section 10), round 5
gpt6's check of `d7e797a0` (section 11).

## 1. The marks (`marks.md`)

Two glyphs lead a continued row and were not read: xfx's `↳` (the
approval's wrapped pieces, `src/tui/mcp_menu.rs:1345`, `:1362-1364`; none in
fx at the pin) and `┃↑`, the rail with `↑` in its space's cell (the
composer's first row shown below rows scrolled out: xfx
`src/tui/shell.rs:317`, fx `src/ui/footer/input_presentation.zig:1021@481104fd`;
a bare `↑` opens key hints and is text). `┃↑` is a scroll mark, not a wrap:
in a real frame the row above is not the composer's, so what it guards is a
tail whose head is off the screen. Every other continuation prefix in both
renderers is spaces, `│`, `┃` or `┋`, already read; `…`, `...`, `?` and
first-row markers are no continuation (`marks.md` cites each). The same
head-gone shape with no mark in front -- a leading `...{tail}`, a frame's top
row, an approval row's mid-row `… {tail}` -- is recorded in `marks.md` as an
open residual, not read here.

## 2. The fix (`scripts/fxcap/roots.py`, `37a9dca2`; the tail rule as round 3 left it, `f5fed4b4`; comments only since, `same-rule.txt`)

- **Read through.** The view drops a mark among a row's leading gutter
  characters (`MARKS`, `View._trim`), read in the row's visible bytes
  across a colour change inside it (round 3), so a root a binary wrapped behind one
  is one root, rewritten whole over its own columns, the mark where it was.
  A mark in a root counts as a gutter run (`GUTTER_RUN`), as the view drops
  it. Only a whole root is written: a head whose row goes on behind a mark
  with other text is left (`test_text_that_only_starts_like_a_root_behind_a_mark_is_no_root`).
- **Fail closed, behind a mark, on a path-shaped end.** A row behind a mark
  that starts with a root's end -- its last byte or more, short of the
  whole, ending whole -- **with a `/` in it or right after it**, or right
  after it the JSON-escaped `\/` every root is also spelled as (round 3;
  the percent-encoded spelling keeps `/` as `/`, so there is no `%2F`
  rule) -- refuses the
  file, in the rewrite's survivor reading and in `--check`
  (`View.tail_behind_mark`, `survivors`), whether or not the head was
  rewritten: a head written as a shorter root's placeholder (the TMPDIR a
  scenario copy is under) leaves the scenario directory nowhere whole, and
  its tail alone refuses the file. A user name's fragment before a `/`
  (`↳ ers/someone/x`, `┃↑one/notes`) is refused; a root's last word with no
  `/` at it (`↳ bun x.ts`, `┃↑work on it`, `↳ scenarios`, `↳ runner`,
  `↳ s`) reads as a word and passes (round 1 refused each of these on every
  run whose roots end so: gpt6's MUST, opus Q2.1).
  - **Behind a mark only.** It is a heuristic for the rows a mark leads, not
    a rule for every row: the same end behind a plain gutter passes. 72
    committed files hold a path piece on the row after `<local-path>`
    behind spaces or a rail and no mark, and `--check` passes them
    (`gutter-tails.txt`, `gutter_tails.py`; opus Q2.4).
  - **Not inside a placeholder.** A row behind a mark that starts inside a
    placeholder the rewrite wrote (`<wor` then `↳ k:real___>`) is no text,
    so no tail is read there (round 1 refused its own `:real` output, opus
    Q2.3); and a row holding only a gutter and a mark leads nothing.

## 3. RED -> GREEN

**Unit** (`mutants.txt`, first row; the tests in
`scripts/fxcap/test_capture.py` RootsRewrite): at the base, 436
subtests fail (`mutants.txt`, round 4's 29 tests) -- a root wrapped behind
a mark at every byte (behind `  ↳ `, `┃↑`, a coloured `┃↑`, `│ ↳ `, `↳` and
three spaces), every spelling over three rows, a root holding a mark, the
tail after a partial rewrite (through the CLI's `--check` too), the
path-shaped end both ways, the escaped separator, the split mark, the
placeholder, the mark alone, the bare arrow and round 4's edges. At the
head the 29 pass. (Round 1's figures: 233 in 3 of 22 tests; round 2's: 419
of 26; round 3's: 429 of 28.)

**End to end** (`e2e-red.txt`, `e2e-green.txt`; `e2e.sh`), round 4's run,
with roots.py at `f5e95437` (round 3's rule; its sha256 in each file):
train 12's trial again, `p5-mcp-menu-new` swept on fx v0.0.16 (the pin,
sha256 checked before and after) and on a release build of `f5e95437`
(its sha256 in each file; no round changed `src/`), from a copy of the
scenarios at a 118-byte directory.

- RED, the base's capture.sh and roots.py: xfx's `approve-confirm` row 9,
  led by `  ↳ `, holds the directory's first 116 bytes and row 10 its last
  2, and `gutter_check.py`'s `--check` exits 0 -- as train 12 found. The
  head's roots.py `--check` refuses that same capture (exit 3): the rows
  join behind the mark, and the directory is whole.
- GREEN, the head's: no row holds 4 bytes of it in one piece, rows 9 and 10
  still open with `↳` and hold none of its bytes (the placeholder, written
  over its own columns, is there); `--check` exits 0 with either roots.py,
  and every run of the sweep exited 0 (33 / 34 pairs, as RED).
- fx's side holds none of it in either: fx goes on four spaces in.
- Rounds 1 and 2 ran it with their own roots.py and read the same (round
  2's: `a74a293b`'s roots.py, whose sha256 was not the round-3 rule's, which
  opus round 3 noted). Round 4 ran the committed `e2e.sh` from a clone of
  `f5e95437`, rows.py beside it: the same reading, roots.py sha256
  `a71c4a5344e4` in both files, the head's (`scan.txt`'s first line).
- **Not this unit's: a cut.** In both runs 48 published files hold the
  directory's head cut short by `…` (the server screen's `runs` row): a cut
  is not a continuation (`marks.md`), and roots.py writes only a whole root.
  capture.sh gives HOME and the TMPDIR as roots of their own
  (`scripts/fxcap/capture.sh:695`, `:697`), so a cut head under either has
  that part written; that case was not run here. Here the copy sat outside
  both, so the head is left whole up to the cut; every one of those
  files holds a shape `scripts/check-no-local-paths.sh` refuses in a tracked
  file (`cuts.py`, the last line of each e2e file) -- the backstop roots.py's
  docstring names for a fragment of a root above the run directory.

## 4. What is committed (`scan.txt`, `fix-tails.txt`)

`scan.sh` runs roots.py `--check` over every tracked file, the committed
captures and receipts (`.prd/`) apart from the rest, with two sets of roots
built at run time: **machine** (this machine's HOME and harness scratch
root, the scenario directories of this checkout and of a runner's) and
**public** (a runner's HOME and bun, Homebrew's bun: paths that name no user
or host).

- **Before** (`scan-before.txt`, the base's tree, the head's roots.py): 36
  committed captures refused, all on the machine set -- `mcp-menu/xfx-base`'s
  34 at 40x60 and `xfx-head`'s `approve-confirm` pair: a wrap broke the
  scenario directory behind `↳`, `redact.py` wrote the head as
  `<local-path>`, and the tail (`cripts/fxcap/scenarios`,
  `/scripts/fxcap/scenarios`) stayed on the next row. The base's roots.py
  read none of them. **None held an identity**: the head was already
  `<local-path>`, and the tail is those fixed, repository-relative
  characters (`fix_tails.py`'s pattern matches nothing else).
- **Fixed in this unit** (`6c71502f`, `fix-tails.txt`): **38 files changed**,
  one row each -- the 36 refused, and fx's `approve-confirm` `.txt` and
  `.ansi`, which were never refused (fx's tail stands behind four spaces,
  no mark) and were edited only so the pair reads as before. Each tail is
  `<scenarios>`, the token redact.py writes for that directory whole (the
  same captures at 40x120 read `↳ <scenarios>/p5-mcp-stdio.ts`), so
  `remote_only.py`'s verdicts for those 34 snaps read as before: 22
  identical, 11 `remote`'s status only, 1 other. mcp-menu's README now says
  so where it describes those rows.
- **After** (`scan.txt`, round 4's head, every tracked file): machine
  roots over the 17820 captures and receipts, **0 refused** (rounds 2 and
  3: 0 of 17814 and of 17815, the same files refused elsewhere);
  `scripts/check-no-local-paths.sh` ok. Over the rest, 3 files, each a test
  or source fixture whose line ends in a `local-path-ok` comment, which
  roots.py does not read (check-no-local-paths does). Public roots: 13
  receipts hold Homebrew's or a runner's `bun`, or a runner's HOME, and 4
  source files -- round 2's path-shape test among them, whose fixture roots
  are a runner's HOME and bun; they name no user and no host and are left as
  they are (check-no-local-paths allows a runner's HOME by name). 213 rows
  of tracked files open behind a mark (2998 in round 1, before a bare `↑`
  stopped counting), counted since round 4 in every file holding `↳`, or
  `┃` and `↑` apart: the widened filter adds no marked rows (8466 files
  read, 173 with round 3's filter, 213 rows in both; `split-marks.txt`).
  That cannot see a split mark in a file that also holds an intact one;
  the byte count below can.
- **What the tail rule could refuse in a run.** Round 1's measured only a
  random scratch name's 1-6 letters (`flake.py`, dropped in round 2: opus
  showed the real false refusals were fixed words, which that model could
  not see). With the path shape, a tail of a run directory is refused only
  with a `/` at it -- a piece of that path, which is what the rule is for --
  and the committed corpus is the figure: `scan.txt` above, every root and
  every row.

## 5. Mutants (`mutants.txt`)

`mutants.py` on a `git clone --no-local` of `f5e95437` (round 4), under the
suite lock: RootsRewrite (`Ran 29 tests`, printed by each run) against
roots.py at the base, then each mutant of the head's, then unmutated. Every
mutant fails it:

| roots.py | failed (sub)tests | first failed assertion |
|---|---|---|
| at the base, `635e7242` (the unit RED) | 436 | `test_capture.py:597` |
| `↳` dropped from `MARKS` | 262 | `:536` |
| `┃↑` dropped from `MARKS` | 169 | `:536` |
| a bare `↑` a mark again (round 1) | 4 | `:600` |
| the fail-closed tail check dropped (`survivors`) | 148 | `:537` |
| any end, no path shape (round 1) | 15, the words that must pass | `:525` |
| path shape: a separator in the end only | 65 | `:537` |
| path shape: a separator after the end only | 2 | `:508` |
| no `\/` after the end (round 2) | 6 | `:525` |
| the tail check from two bytes up | 12 (a one-byte end before a `/`) | `:464` |
| no skip inside a written placeholder | 4, k = 1..4 | `:588` |
| a mark read in one run only (round 2) | 2, the two split marks | `:536` |
| a mark alone on its row leads the next | 4 | `:597` |
| the marks out of `GUTTER_RUN` | 2 (the rewrite does not refuse a root holding one) | `:486` |
| unmutated | 0: `OK` | -- |

`test_text_that_only_starts_like_a_root_behind_a_mark_is_no_root` passes
at the base and at the head: it pins the rule this unit keeps.

## 6. The gate (`gate.txt`)

`gate.sh` (LOCAL-PATHS' gate with `test-capture.sh` added) under the suite
lock, `TMPDIR` under the harness scratch root, at `0dd6f3c5`; the commit
after it (`4851ba05`) changes only this directory's README, gate, scan and
flake files, and nothing under `scripts/` or `src/` (`gate-head-diff.txt`).
Round 2 changes roots.py and its tests; section 8 reruns what they touch.
**21 steps, 19 pass,
the tree clean.** `test-capture.sh` 112 tests OK, `test-sweep.sh` 28 OK,
`check-no-local-paths.sh` and its test ok, smoke-tui 1167 checks, 0
failures.

**The 2 that fail are the base's, one test in both:** `cargo test` and the
fault-injection build each fail
`mcp::config::tests::the_verbs_scenario_leaves_fx_v0_0_16s_bytes` (4729 and
4752 pass). It reads only `src/mcp/config.rs` and
`scripts/fxcap/scenarios/p5-mcp-profile.json`, and `src/` and the scenarios
here are `635e7242`'s byte for byte: train 12's `2ee33f58` gave the menu's
`remote` an `@MCPPORT@` and a timeout in that profile, which the test pins
to fx's bytes. Train 12 put the profile back in `6bfc6562` (on its branch
after this base: the file there is as it was before `2ee33f58`), which
touches no file this unit does.

## 7. After the merge of train 12's head (`rerun.txt`)

`1e023110` merges train 12's head `71d3e02b` (`--no-ff`, no conflict: the
train's commits since the base touch none of this branch's files), which
brings `6bfc6562`'s profile back. `rerun.sh` under the suite lock, `TMPDIR`
the harness scratch root, at `1e023110`: **5 steps, 0 failed, the tree
clean.** `cargo test --locked` and `cargo test --locked --features
fault-injection`: 15 test targets ok each, `the_verbs_scenario_leaves_fx_v0_0_16s_bytes`
ok in both; `test-capture.sh` 112 tests OK; `check-no-local-paths.sh` ok;
`scan.sh`: the machine roots refuse 0 of the 17811 captures and receipts,
the rest as in section 4.

## 8. Round 2: the reviews of `4851ba05`

Both reviews (gpt6, opus) approved with fixes; both found the tail rule
refused too much. `a74a293b` takes the path shape, the placeholder skip,
the mark-alone row and `┃↑` for a bare `↑` (section 2), with tests for each
and the regression matrix gpt6 asked for: every spelling `rules_for` makes
(as given, `//` collapsed, JSON-escaped, percent-encoded) over three rows
behind five leads, `│ ↳ ` and `↳` with three spaces after it among them.
The receipt gains `gutter-tails.txt`, `gate-head-diff.txt`, the 38-file
count (section 4, and mcp-menu's README), the residuals (`marks.md`), the
xfx sha256 in `e2e.sh`, and `Ran N tests` in `mutants.py`; `flake.py`
goes. Reruns (`rerun2.txt`, `TMPDIR` the harness scratch root):
`test-capture.sh` at `a74a293b`, under the suite lock, **116 tests OK**;
the mutants (section 5), **11 of 11** fail RootsRewrite and it passes
unmutated; `scan.sh` and `check-no-local-paths.sh` over the tree with this
receipt staged, **0** captures or receipts refused and **ok** (section 4).
`cargo test` was not run again: round 2 changes no `src/` and no file it
reads (`rerun.txt` holds both suites green at `1e023110`).

## 9. Round 3: gpt6's review of `ff59f0f7`

Both findings reproduced at `ff59f0f7` by the dispatcher and by this
round's tests: the two mutants that each revert one of them fail 4 and 2
subtests in round 3's run (6 and 2 in round 4's, `mutants.txt:10` and
`:13`, where round 4's edge test adds two `\/` cases), and none fails at
`f5fed4b4`:

- **MUST, a JSON-escaped separator.** Behind a mark `someone/x` was refused
  and `someone\/x` passed. Every root is also spelled JSON-escaped
  (`spellings`), so `\/` right after a root's end now counts as `/`; in the
  end itself `\/` holds a `/` already. Refused behind `  ↳ ` and `┃↑`:
  `someone\/x`, `one\/x`, the escaped root over three rows; still passed:
  `bun x.ts`, `work on it`, `scenarios`.
- **SHOULD, a mark split by a colour change.** `ESC[2m┃ESC[0mESC[1m↑` was no
  mark: `_trim` matched a mark within one run's bytes. Both renderers draw
  `┃↑` in one run (xfx `src/tui/shell.rs:2924-2932`, fx
  `input_presentation.zig:1020-1022@481104fd`), and **no tracked file holds
  one, by its bytes**: of the 12824 tracked files with a rail `┃`, 0 hold a
  rail, one or more control sequences and then `↑` -- 0 occurrences
  (`split-marks.txt`, `split_marks.py`, round 5: gpt6's MUST; it checks its
  pattern on the two split forms and two that are not before it counts).
  Round 4's figure for this, the widened filter's rows (8466 files, 173
  with round 3's filter, 213 rows in both), says only that the filter adds
  no marked rows: it read one View per file for both sets, so a split mark
  beside an intact one counted in both. The reader now matches the row's
  visible bytes across runs. A styled `┃`, a space, then `↑↓` is still a
  key hint, and `ESC[2m  ESC[0m↳` stays a mark.

Reruns (`rerun3.txt`, `TMPDIR` the harness scratch root): `test-capture.sh`
at `f5fed4b4`, under the suite lock, **118 tests OK**; every mutant, old and
new (section 5), **13 of 13** fail RootsRewrite (`Ran 28 tests` each) and it
passes unmutated; `scan.sh` over the 18913-odd tracked files with this
receipt staged, **0 of the 17815 captures and receipts refused** on the
machine roots and the same files elsewhere as round 2 -- **no new
refusal**; `check-no-local-paths.sh` **ok**.

## 10. Round 4: both engines' reviews of `76fbee40`

Both approved, no MUST. No rule changes: roots.py gains comments only --
the byte after an end is read across the row's end, what passes by design,
the placeholder skip as a second guard -- and its code without docstrings
and comments is `f5fed4b4`'s (`same-rule.txt`, `same_rule.py`). One test
pins the edges (opus NIT 2, gpt6's four): `↳ scenarios` before a row
opening `/x` or `\/x` is refused and before `x` passes; a percent-form end
(`e%20one/x`) is refused and `e%20one more` passes; `\/` at the end of the
file is refused and a lone `\` passes; a split mark with nothing after it
leads nothing; a rail and an arrow a cursor move parts are no mark.
`scan.sh` reads a file holding `↳`, or `┃` and `↑` apart, so a split mark
in a file with no intact one is counted (opus SHOULD 1); whether any file
holds a split mark at all is round 5's byte count (section 11), which
supersedes round 4's `split-marks.txt` line in `rerun4.txt`. Decisions say what passes (SHOULD 2).

Reruns (`rerun4.txt`, `TMPDIR` the harness scratch root): `test-capture.sh`
at `f5e95437`, under the suite lock, **119 tests OK**; every mutant
(section 5), **13 of 13** fail RootsRewrite (`Ran 29 tests` each) and it
passes unmutated; the end-to-end trial again (section 3); `scan.sh` over
every tracked file with this receipt staged, **0 of the 17820 captures
and receipts refused** on the machine roots and the same files elsewhere
as rounds 2 and 3, **no new refusal**, 213 rows behind a mark;
`check-no-local-paths.sh` **ok**.

## 11. Round 5: gpt6's check of `d7e797a0`

One MUST, receipt only: round 4's `split_marks.py` compared two counts
from the same View per file, so a split mark beside an intact one counted
in both and "0 that only a split mark adds" could not see it. It now
counts split marks by their bytes -- `┃`, one or more control sequences as
roots.py's `CONTROL` reads them, `↑` -- after checking its pattern on both
split forms the tests pin (match) and on `┃↑` and `┃`, a reset, a space,
`↑` (no match): **12824 tracked files hold a rail, 0 a split rail-arrow, 0
occurrences** (`split-marks.txt`; the dispatcher's own count agrees). Its
second line keeps the widened filter's figures, described as what they
are. Reruns (`rerun5.txt`): `split_marks.py` and `check-no-local-paths.sh`
over the tree with this receipt staged.

## Decisions

- **`┃↑` is a mark.** It is a scroll mark, not a wrap, but the row it leads
  can hold a tail whose head is off the screen, which is what the tail rule
  is for. A bare `↑` is not: it opens key hints, and counting it made 2998
  committed rows "behind a mark" in round 1's scan, against 213 now
  (`scan.txt`, then and now).
- **A path-shaped end, from one byte, behind a mark only.** A user name's
  fragment before a `/` is what leaks; a root's last word with no `/` at it
  is a word. What it gives up (opus Q2.5): a root's last bytes alone at a
  row's end after a partial rewrite, which this roots.py, reading marks,
  no longer leaves. **And a root's last bytes with nothing path-like after
  them now pass** -- HOME's tail closing a `┃↑` row (whose head is never in
  the frame), ending the file, or before a space or a quote. Round 1
  refused them; neither check catches such a fragment now (`marks.md`,
  open residuals). The mutant that starts at two bytes is killed by a
  one-byte end before a `/`.
- **The fixed captures' tail is `<scenarios>`**, the token redact.py writes
  for the whole directory, and fx's side of the pair is written the same,
  so the pair's verdicts do not move. The rows keep their place; a
  receipt's text there is now redact.py's, not the run's.
- **Public paths left.** Homebrew's and a runner's `bun` and a runner's
  HOME in 13 receipts and 3 source files name no user and no host;
  check-no-local-paths.sh allows a runner's HOME by name. Rewriting old
  receipts' scenario inputs would make them say what was not run.
- **A cut is not this unit's.** `…` is no continuation (`marks.md`). A cut
  inside the run directory keeps its name, which is refused; in both trials
  every file holding a cut head of the scenario directory also holds a shape
  check-no-local-paths.sh refuses in a tracked file (`cuts.py`). A cut head
  of a directory outside HOME, the TMPDIR and those shapes is refused by
  neither.

## Files

`marks.md` · `mutants.py` -> `mutants.txt` · `e2e.sh`, `rows.py`, `cuts.py`
-> `e2e-red.txt`, `e2e-green.txt` · `fix_tails.py`, `fix-tails.sh` ->
`fix-tails.txt` · `scan.sh` -> `scan-before.txt`, `scan.txt` ·
`gutter_tails.py` -> `gutter-tails.txt` · `gate.sh` -> `gate.txt`,
`gate-head-diff.txt` · `rerun.sh` -> `rerun.txt`, `rerun2.txt`, `rerun3.txt`,
`rerun4.txt`, `rerun5.txt` · `same_rule.py` -> `same-rule.txt` ·
`split_marks.py` -> `split-marks.txt`
