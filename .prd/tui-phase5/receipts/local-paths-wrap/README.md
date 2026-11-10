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
answers the reviews of `4851ba05` (section 8).

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

## 2. The fix (`scripts/fxcap/roots.py`, `37a9dca2`; the tail rule as round 2 left it, `a74a293b`)

- **Read through.** The view drops a mark among a row's leading gutter
  characters (`MARKS`, `View._trim`), so a root a binary wrapped behind one
  is one root, rewritten whole over its own columns, the mark where it was.
  A mark in a root counts as a gutter run (`GUTTER_RUN`), as the view drops
  it. Only a whole root is written: a head whose row goes on behind a mark
  with other text is left (`test_text_that_only_starts_like_a_root_behind_a_mark_is_no_root`).
- **Fail closed, behind a mark, on a path-shaped end.** A row behind a mark
  that starts with a root's end -- its last byte or more, short of the
  whole, ending whole -- **with a `/` in it or right after it** refuses the
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
`scripts/fxcap/test_capture.py` RootsRewrite): at the base, 419
subtests fail in 7 of the 26 tests -- a root wrapped behind a mark at every
byte (behind `  ↳ `, `┃↑`, a coloured `┃↑`, `│ ↳ `, `↳` and three spaces),
every spelling over three rows, a root holding a mark, the tail after a
partial rewrite (through the CLI's `--check` too), the path-shaped end both
ways, the placeholder, the mark alone and the bare arrow. At the head the
26 pass. (Round 1's figures: 233 in 3 of 22 tests.)

**End to end** (`e2e-red.txt`, `e2e-green.txt`; `e2e.sh`), round 2's run,
with the head's roots.py: train 12's trial again, `p5-mcp-menu-new` swept
on fx v0.0.16 (the pin, sha256 checked before and after) and on a release
build of the head (its sha256 and banner in each file; the head changes no
`src/`), from a copy of the scenarios at a 118-byte directory.

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
- Round 1's run (round 1's roots.py, a build of the base) read the same;
  round 2 ran again because the rule changed and the binary's sha256 had
  not been recorded. rows.py ran on the same captures after each sweep:
  the copy of `e2e.sh` that ran could not find it beside itself (each
  file's second line).
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
- **After** (`scan.txt`, round 2's head, every tracked file): machine
  roots over the 17814 captures and receipts, **0 refused**;
  `scripts/check-no-local-paths.sh` ok. Over the rest, 3 files, each a test
  or source fixture whose line ends in a `local-path-ok` comment, which
  roots.py does not read (check-no-local-paths does). Public roots: 13
  receipts hold Homebrew's or a runner's `bun`, or a runner's HOME, and 4
  source files -- round 2's path-shape test among them, whose fixture roots
  are a runner's HOME and bun; they name no user and no host and are left as
  they are (check-no-local-paths allows a runner's HOME by name). 213 rows
  of tracked files open behind a mark (2998 in round 1, before a bare `↑`
  stopped counting).
- **What the tail rule could refuse in a run.** Round 1's measured only a
  random scratch name's 1-6 letters (`flake.py`, dropped in round 2: opus
  showed the real false refusals were fixed words, which that model could
  not see). With the path shape, a tail of a run directory is refused only
  with a `/` at it -- a piece of that path, which is what the rule is for --
  and the committed corpus is the figure: `scan.txt` above, every root and
  every row.

## 5. Mutants (`mutants.txt`)

`mutants.py` on a `git clone --no-local` of `a74a293b`, under the suite
lock: RootsRewrite (`Ran 26 tests`, printed by each run) against roots.py
at the base, then each mutant of the head's, then unmutated. Every mutant
fails it:

| roots.py | failed (sub)tests | first failed assertion |
|---|---|---|
| at the base, `635e7242` (the unit RED) | 419, in 7 tests | `test_capture.py:537` |
| `↳` dropped from `MARKS` | 254 | `:527` |
| `┃↑` dropped from `MARKS` | 164 | `:464` |
| a bare `↑` a mark again (round 1) | 1, the bare arrow | `:540` |
| the fail-closed tail check dropped (`survivors`) | 137 | `:464` |
| any end, no path shape (round 1) | 6, the words that must pass | `:508` |
| path shape: a `/` in the end only | 54 | `:464` |
| path shape: a `/` after the end only | 2 | `:508` |
| the tail check from two bytes up | 12 (a one-byte end before a `/`) | `:464` |
| no skip inside a written placeholder | 4, k = 1..4 | `:528` |
| a mark alone on its row leads the next | 2 | `:537` |
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
  no longer leaves. The mutant that starts at two bytes is killed by a
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
`gate-head-diff.txt` · `rerun.sh` -> `rerun.txt`, `rerun2.txt`
