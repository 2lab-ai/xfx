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
directory that backs it; no file here holds a path of a machine.

## 1. The marks (`marks.md`)

Two glyphs lead a continued row and were not read: xfx's `↳` (the
approval's wrapped pieces, `src/tui/mcp_menu.rs:1345`, `:1362-1364`; none in
fx at the pin) and `↑` in `┃↑` (the composer's first row shown below rows
scrolled out: xfx `src/tui/shell.rs:317`, fx
`src/ui/footer/input_presentation.zig:1021@481104fd`). `↑` is a scroll mark,
not a wrap: in a real frame the row above is not the composer's, so what it
guards is a tail whose head is off the screen. Every other continuation
prefix in both renderers is spaces, `│`, `┃` or `┋`, already read; `…`,
`...`, `?` and first-row markers are no continuation (`marks.md` cites each).

## 2. The fix (`scripts/fxcap/roots.py`, `37a9dca2`)

- **Read through.** The view drops a mark among a row's leading gutter
  characters (`MARKS`, `View._trim`), so a root a binary wrapped behind one
  is one root, rewritten whole over its own columns, the mark where it was.
  A mark in a root counts as a gutter run (`GUTTER_RUN`), as the view drops
  it. Only a whole root is written: a head whose row goes on behind a mark
  with other text is left (`test_text_that_only_starts_like_a_root_behind_a_mark_is_no_root`).
- **Fail closed.** A row behind a mark that starts with a root's end -- its
  last byte or more, short of the whole, ending whole -- refuses the file,
  in the rewrite's survivor reading and in `--check` (`View.tail_behind_mark`,
  `survivors`), whether or not the head was rewritten: a head written as a
  shorter root's placeholder (the TMPDIR a scenario copy is under) leaves the
  scenario directory nowhere whole, and its tail alone refuses the file.

## 3. RED -> GREEN

**Unit** (`mutants.txt`, first row; the tests in
`scripts/fxcap/test_capture.py` RootsRewrite): at the base, 233 subtests
fail in the three new tests that read a mark -- a root wrapped behind `  ↳ `,
`┃↑` or a coloured `┃↑` at every byte is neither rewritten nor refused, its
tail behind a mark after a partial rewrite passes `--check` (exit 0, through
the CLI as well), a root holding a mark is not refused. At the head the 22
tests pass.

**End to end** (`e2e-red.txt`, `e2e-green.txt`; `e2e.sh`): train 12's trial
again, `p5-mcp-menu-new` swept on fx v0.0.16 (the pin, sha256 checked
before and after) and on a release build of the base's source (the head
changes no `src/`), from a copy of the scenarios at a 118-byte directory.

- RED, the base's capture.sh and roots.py: xfx's `approve-confirm` row 9,
  led by `  ↳ `, holds the directory's first 116 bytes and row 10 its last
  2, and `gutter_check.py`'s `--check` exits 0 -- as train 12 found. The
  head's roots.py `--check` refuses that same capture (exit 3).
- GREEN, the head's: no row holds 4 bytes of it in one piece, rows 9 and 10
  still open with `↳` and hold none of its bytes (the placeholder, written
  over its own columns, is there); `--check` exits 0 with either roots.py,
  and every run of the sweep exited 0 (33 / 34 pairs, as RED).
- fx's side holds none of it in either: fx goes on four spaces in.
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
  read none of them.
- **Fixed in this unit** (`6c71502f`, `fix-tails.txt`): each tail is
  `<scenarios>`, the token redact.py writes for that directory whole (the
  same captures at 40x120 read `↳ <scenarios>/p5-mcp-stdio.ts`); fx's side
  of the pair too, so `remote_only.py`'s verdicts for those 34 snaps read as
  before: 22 identical, 11 `remote`'s status only, 1 other.
- **After** (`scan.txt`, the head): machine roots over the 17 800-odd
  captures and receipts, **0 refused**. Over the rest, 3 files, each a test
  or source fixture whose line is marked `local-path-ok` (roots.py reads no
  mark). Public roots: 13 receipts hold Homebrew's or a runner's `bun`, or a
  runner's HOME, and 3 source files; they name no user and no host and are
  left as they are (check-no-local-paths allows a runner's HOME by name).
  `scripts/check-no-local-paths.sh`: ok. 2998 rows of tracked files open
  behind a mark, which the tail rule read.
- **What the tail rule could refuse in a run** (`flake.txt`, `flake.py`): a
  run's scratch root ends in mktemp's six random letters and digits, so a row
  behind a mark that starts with 1 to 6 of them, ending whole, would refuse
  one run in 62^n. Of the 2998 committed rows behind a mark, **0** start that
  way (and 0 with a work root's end, `k` to `work`): 2785 open with `↓` (a
  key hint after `↑`), 163 with a placeholder's `<`, and 50 with a word of
  seven letters or more or one that runs on. The bound is the committed
  captures'; a new scenario whose marked row starts with a short word ending
  whole is the case to watch.

## 5. Mutants (`mutants.txt`)

`mutants.py` on a `git clone --no-local` of `6c71502f`, under the suite
lock: RootsRewrite (22 tests) against roots.py at the base, then each mutant
of the head's, then unmutated. Every mutant fails it:

| roots.py | failed (sub)tests | first failed assertion |
|---|---|---|
| at the base, `635e7242` (the unit RED) | 233, in the three new tests that read a mark | `test_capture.py:462` |
| `↳` dropped from `MARKS` | 78 | `:462` |
| `↑` dropped from `MARKS` | 155 | `:462` |
| the fail-closed tail check dropped (`survivors`) | 78, the tail test alone | `:462` |
| the tail check from two bytes up | 6 (a one-byte tail, each lead) | `:462` |
| the marks out of `GUTTER_RUN` | 2 (the rewrite does not refuse a root holding one) | `:484` |
| unmutated | 0: `OK` | -- |

The brief's two (drop `↳`, drop the tail check) are the second and fourth
rows. `test_text_that_only_starts_like_a_root_behind_a_mark_is_no_root` passes
at the base and at the head: it pins the rule this unit keeps.

## 6. The gate (`gate.txt`)

`gate.sh` (LOCAL-PATHS' gate with `test-capture.sh` added) under the suite
lock, `TMPDIR` under the harness scratch root, at `0dd6f3c5`; the commit
after it changes only this directory's README, scan files and flake files,
and `git diff 0dd6f3c5 HEAD -- scripts src` is empty. **21 steps, 19 pass,
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

## Decisions

- **`↑` is a mark.** It is a scroll mark, not a wrap, but the row it leads
  can hold a tail whose head is off the screen, which is what the tail rule
  is for; reading it costs nothing the scan found (`flake.txt`).
- **Any tail, from one byte.** As the brief says; the mutant that starts at
  two bytes is killed by a one-byte tail at every lead.
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
`fix-tails.txt` · `scan.sh` -> `scan-before.txt`, `scan.txt` · `flake.py` ->
`flake.txt` · `gate.sh` -> `gate.txt`
