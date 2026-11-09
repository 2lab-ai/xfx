# LOCAL-PATHS: no path of one machine in the tracked tree

xfx is public, and machine-local paths kept reaching it: on `main` at
`ccced1f5`, 156 tracked files held one machine's harness scratch root, every
image capture embedded the run directory (an OSC 8 link to
`<root>/fxcap.<run>/work/shot.png`), and four units in one night committed
scratch paths in receipt scripts and READMEs. This unit fixes the source
(`scripts/fxcap/capture.sh`, `scripts/fxcap/roots.py`), writes out what is
already there, and adds a gate that refuses it by shape.

Every claim below names the file in this directory that backs it. Base:
`ccced1f5`. Branch `fix/p5-local-paths`. Round 2 answered the review of
`1fe6ea37` and the dispatcher's failed gate there (section 6); round 3 the
review of `8e7c1c32` (section 7).

## 1. The source: every file a run writes, rewritten before anyone reads it

capture.sh writes every file of a run to a stage inside the run, has
`roots.py` rewrite it there, and only then renames it into the out directory
(`publish`): a snap's `.txt` and `.ansi`, the `.full` pair, the gateway log
and every request body, mcphttp's log and port, the wire, the title, the
cursor, the clipboard copy, a copied file's path, the `/trace` reports, the
bells, the sockets, the workspace and HOME dumps. `expect` and the sweep read
only what was renamed; a run killed halfway leaves no unrewritten file in the
out directory; a file `roots.py` refuses never reaches it, and the run fails
(exit 3). At the run's end everything it left there is read once more
(`roots.py --check`): a verb that wrote past `publish` would fail the run.

| root | as given | resolved, when that differs |
|---|---|---|
| `$run/work`, the binary's working directory | `<work___>` | `<work:real___>` |
| `$run`, the rest of the run (its HOME `<scratch___>/home`, its TMPDIR `<scratch___>/fxtmp` or `/xfxtmp`) | `<scratch___>` | `<scratch:real___>` |
| the scenario directory | `<scenarios___>` | |
| the `bun` that runs fakegw | `<bun___>` | `<bun:real___>` |
| capture.sh's own `$TMPDIR`, trailing slashes dropped (not a shared `/tmp`) | `<tmp___>` | `<tmp:real___>` |
| capture.sh's own `$HOME` | `<home___>` | `<home:real___>` |

How `roots.py` finds and writes a root (its docstring is the whole rule):

- **As wide as the root.** A placeholder is the name between `<` and `>`,
  filled with `_` to exactly the root's width in columns -- for an ASCII path,
  every root here and on the CI runners, its length in bytes. A root narrower
  than its name gets the name's letter (`/root` is `<h__>`, upper case when
  resolved); one narrower than four columns has no placeholder, and the file
  is refused (exit 3). A resolved form is padded to its own width.
- **Over the root's own columns.** The placeholder is written column for
  column over the root's characters, through the bytes they stood in: a root
  a terminal wrapped at its margin, or a binary broke onto its next row, wraps
  its placeholder at the same column; a colour change inside a root stays at
  its offset; every column after the root keeps its column. Two captures that
  broke a root, or coloured it, at different places differ there still; two
  sides whose roots are as long read the same placeholder.
- **In the text a reader sees.** Control sequences (CSI, OSC, nF such as
  `ESC ( B`) are set aside and rows are joined across their ends and the
  gutters at them (spaces, tabs, U+2500-U+259F), so a root is found where a
  colour splits it, where a terminal wrapped it, and where a binary broke it
  behind a gutter; and inside an OSC payload by its bytes (an OSC 8 link's
  target). Its spellings are the same root: JSON-escaped (`\/`), the
  percent-encoding `urllib.parse.quote(root, safe="/~")` produces (an encoded
  space, `%` or byte past ASCII), and the root with `//` and `/./` collapsed.
  It ends where no path-name character follows it within its row.
- **A literal placeholder is never a root.** Text that already reads like
  one (`<work>`, `<work___>`, `<h__>`) gains a `!` after its `<` before
  anything is rewritten (`<!work>`, `<!!work>`), so a placeholder without one
  is a root and nothing else. A `!` keeps a JSON body JSON. A literal so
  escaped is one column wider.
- **What survives refuses the file.** A root, in any spelling, as given or
  resolved, still in the file's bytes or visible text where it ends whole --
  the rewrite's own rule, so a sibling whose name extends a root's is no
  survivor -- or the run directory's own name (`fxcap.` and mktemp's tail)
  anywhere: exit 3, the file named, the survivor by placeholder. A root a
  binary cut short, or whose head scrolled off a frame, keeps the run name.
  That guard covers the run directory and what is under it; a fragment of a
  root above it (the harness's TMPDIR, the user's HOME) holds no run name,
  and in a tracked file the check's shapes (section 4) are what refuse it.

**Identical on both sides.** One rule for fx's run and xfx's. In a sweep unit
the two runs share one TMPDIR, so every root is as long on both sides and so
is its placeholder. The two binaries' own TMPDIRs are the exception, `fxtmp`
a byte shorter than `xfxtmp`, and are kept apart under `<scratch___>`, as raw.
The ledger's `capture-tmpdir` reads `<scratch___>/fxtmp` (any placeholder
ending `_>/`) as well as the old `fxcap.XXXXXX/fxtmp`; `run-directory` stays
for captures taken before.

**Widths.** The rewrite comes after the binary laid out its frame and keeps
it, column for column. The `roots:` line each run prints first gives each
root's width on that run, the resolved form's too where it is another path.

The sweep writes its committed summaries through `scripts/fxcap/redact.py`,
which keeps a path that continues a placeholder (`<scratch___>/home/...`,
whatever its width: `roots.PLACEHOLDER`) and still redacts one after a `>`
that is no placeholder.

**Tests** (`scripts/fxcap/test_capture.py`; RED and GREEN in `harness.txt`
and `red3-*.txt`):
- `RootsRewrite`, on bytes: a root wrapped at every one of its bytes, its
  placeholder wrapped at that byte; the same root wrapped at two offsets
  reads different; broken behind a gutter of spaces and colours (s1's shape)
  and of box-drawing borders; split by colour changes (at-plan's shape),
  every change at its offset, every cell where it was, and another bolding
  reads different; JSON (still JSON), percent-encoded, `//`-collapsed and OSC 8
  spellings; an nF escape beside a root; a root a name character follows; a
  literal placeholder, split by a colour, with `!`s, in JSON; a resolved
  root; a short root and one too narrow (exit 3); a non-ASCII root's
  columns; a sibling of HOME, TMPDIR and the scenario directory, neither
  rewritten nor refused, and those roots themselves refused; the refusal of a
  cut-short root, with no path printed; a file without a root, byte for byte.
- `RunRoots`, end to end over a stub binary on tmux, every expectation at the
  root's own width: one item a row -- its working directory, resolved, HOME,
  TMPDIR, the harness's TMPDIR, the user's home, the scenario directory, a
  root a name character follows, two padded roots, a root the terminal wraps
  at its margin (seven columns in), a root broken behind `  │ `, a root a
  colour change splits (its changes at their offsets in the `.ansi`), a
  coloured root, a link, a title, a bell -- and a copy, a copied file, a
  report, requests to the fake gateway and the MCP server. Every file kind
  holds no root and no run name, at three TMPDIR lengths in one run; a TMPDIR
  through a link and with a trailing slash; a sibling of the harness's TMPDIR;
  fx and xfx in one TMPDIR, row for row; each side breaking a root at another
  column; a real root against a literal `<work>`.
- The harness suite at four TMPDIRs: 23 bytes (the dispatcher's), 26, 52 (past
  test_capture.py's 50-byte limit, so on `/tmp`, a link) and 27 with a trailing
  slash (`test-capture.txt`): 97 tests OK at each. Round 2's 89 and round 1's
  78 are in the same file.

**In real captures** (`runs-scan.txt`, `runs_scan.py`: a whole root, the
shapes, and what a broken root leaves -- `claude-<digits>`, a sweep job's
`fxsw.` name, a run's `fxcap.` name). Of the baseline sweep's 677 capture
files, 251 held some part of a run's paths; after round 1, 29 (round 1's own
count saw only whole roots); after rounds 2 and 3, 0. Round 3's: 273 hold a
placeholder where a path stood (an image badge's link reads
`file://<work___>/...`), no run was refused, and each run's `roots:` line
named `<bun:real>` too: Homebrew's `bun` is a link.

## 2. The sweep: every pair keeps its verdict

`verdicts.tsv`, one row per snap pair; `verdicts.txt`, the summary. Every sweep
ran the same selection under the suite lock, `--jobs 4`, 40x120, fx v0.0.16
(sha256 `8e6cda2e…`):

- the image scenarios, `--only 'image*' links-image p5-lifecycle-relhome-image`
  (11; `p5-*attach*` names no scenario in `scripts/fxcap/scenarios/`);
- ten that put a path on screen: `workspace`, `workspace-index`, `skills`,
  `skills-paste`, `trace`, `trace-nowhere`, `p5-panels-status`,
  `p5-panels-status-short`, `p5-mcp-empty`, `status-version-stats`.

Before: `ccced1f5`'s harness from a clone, xfx built at `ccced1f5`. Round 1:
`43024057`'s; round 2: `6cb5c4e7`'s; round 3: `f05f3bd7`'s, each from a clone,
xfx built there.

**145 pairs before, 145 after round 3, 145 with the same verdict, 0
changed**, and the same against round 1 and round 2 (`compare_verdicts.py`):
normalized-identical 102, identical 4, approved-divergence-cascade 12,
unresolved-defect 27 in all four, pass 106 / 145 in all four.

## 3. What was there: scrubbed, and read again

**By hand** (commits `afbb866d`, `41023d5f`, `43024057`):
- `src/`: the scratch root in five test files is a stand-in of the same
  length and separators (`/scratch/workdir/claude/`, as `info_panel.rs` already
  had), so every wrap and break the tests measure is where it was; the
  per-user macOS temporary folder in `approval.rs` and `shell.rs` (its name
  is a hash of the user) is a same-length stand-in. `src/skills/catalog.rs`
  keeps upstream's bytes, once, marked: the block's namespace hash covers the
  roots, so the fixture is upstream's only for that path.
- seven receipt scripts that set `TMPDIR` to the machine's root derive it
  from `$TMPDIR` (`flake-status-s1/sweep.sh` notes its literals must not be in
  it); the redactor test's inputs are marked; c3's note names no folder.

**By script.** Round 1 (`scrub.py`, `scrub-manifest.tsv`, `19a6b596`): 143
files, by line, short placeholders; a sentence that names the root as a thing
keeps it as `/private/tmp/claude-<uid>` (17). Round 2 (`--round-2`,
`scrub-manifest-2.tsv`, `6cb5c4e7`): 35 files whose roots no line held whole
(s1's skills frames, w5's at-plan `.ansi`, five comparison reports). Round 3
(`--round-3 43024057`, `scrub-manifest-3.tsv`, `6b4f72fd`): every capture file
either round rewrote, read again as it was before any round touched it, and
every root in it -- whole, across rows, across colours, a quoted row's tail --
written as a placeholder exactly as wide as it, over its own columns: 110
files, 259 roots found in the visible text, 83 tails. A marked line is left in
every round.

**Prose keeps the short placeholders.** In a README, a loop file or a script
no column is a measurement, so `TMPDIR=<tmp>` stays as rounds 1 and 2 wrote
it, and there `<tmp>` means only the harness root,
`/private/tmp/claude-<uid>`: what follows it (`<tmp>/fsg`, `<tmp>/bin/...`) is
the file's own. In a capture written by capture.sh since this unit, `<tmp___>`
is capture.sh's whole `$TMPDIR` (in a sweep, the sweep's job directory).

**Read again** (`verify.py`, `verify.txt`): every pair round 3 rewrote, fx
against xfx, at `ccced1f5` and now, by position, by `scripts/fxcap/cells.py`'s
reading and by the sweep's normalized reading: 42 read the same; 6 (d1's
`read-failures` Ctrl-O screens) the same but for one row whose only
difference was the run directory's random suffix, each side's own; 0
otherwise. And every row of all 110 rewritten captures, paired or not, keeps
its width: 7748 of 7748 rows. With nothing to pair against, listed as raw
evidence rewritten: 26 files (request bodies whose turn numbering differs by
side, gate and test logs, comparison reports). `verify-2.txt` is round 2's.

**Claims about a width, handled by name:** s1's "the same length, so no row
moves" is true again and says why; d1's "a stand-in of another length would
move the breaks" says the placeholder is as wide; flake-status-s1's table,
keyed on which directory names hold `s1`, notes `<tmp>` holds none.

**`.prd/tui-phase4/loop.md`**: the common brief keeps it as it is; its two
lines holding the root are rewritten, because the check exempts no file.

## 4. The check

`scripts/check-no-local-paths.sh` runs in CI's gate job on every row but the
`release` half of the split x86_64 macOS row (its `checks` half runs it, on the
same commit); `scripts/test-check-no-local-paths.sh` runs once per OS
(`script_tests`), and `scripts/test-gate-env.sh` requires it to reach a macOS
and a Linux row.

- Shapes, never a name: a home directory, `/Users/<name>` or `/home/<name>`,
  where an absolute path starts (a line start, a character no path name holds
  -- `>` among them -- or the end of a terminal control sequence) and ends with
  its name (before a `/`, any other non-name character, or the line's end);
  `/tmp/claude-<digits>` with or without `/private`, and a `claude-<digits>/`
  at a row's start or after a space or a control sequence (such a root broken
  after its `tmp/`); macOS's per-user temporary root.
- Not every path of a machine: a Windows path, `~user`, a percent-encoded
  path, a home directory broken across two rows and a scratch root broken
  anywhere but after its `tmp/` are not found by shape (capture.sh's rewrite
  finds its own roots in all of those forms).
- Exceptions: nine documented placeholder homes in the script (`/home/u`,
  `/Users/u`, `/Users/me`, `/home/me`, `/Users/example`, `/home/someone`,
  `/Users/someone`, `/home/you`, `/home/runner` -- GitHub's runners' and
  train-6's container user) and a home that continues one of capture.sh's
  placeholders, at any width, taken out of a line before it is matched again
  (on the line with a space after it, so each is cut where it matched); and a
  line that ends in a `local-path-ok` comment, that line only -- never in a
  capture: a file capture.sh names (`<bin>-<scenario>-<rows>x<cols>-...`, a
  `/trace` report's `.md` among them, wherever it is), one under a request
  body's directory, or one of a capture's kinds (`.txt`, `.ansi`, `.wire`,
  `.json`, `.log`, ...). No directory. At this branch: 8 marked lines, 77 placeholder lines
  (`counts.txt` lists the marked ones and the files of the rest).
- One `git grep` over the tree, one per file that holds a shape: no names on a
  command line, so no batching; about 2 s on this tree (check-no-secrets.sh:
  55 s). The guards are check-xfx-identity.sh's (every `GIT_*` dropped,
  `--work-tree=.`, the root's own `.git`, index bits and missing files
  refused), a positive control runs first, and a hit prints the file, the line
  and the shape, never the text.
- Its test: 10 cases, 23 plants -- every shape in every kind of file (a name
  git quotes, a file named `-`, a binary, a coloured path, an OSC 8 link, a
  redirect, an HTML tag, a bare home, a broken root, a mark in a request body,
  a `.txt`, a report capture and mid-line, a home cut by its first match) and
  every form that must pass (padded and short placeholder homes among them),
  under `GIT_DIR`, `GIT_INDEX_FILE`, `GIT_WORK_TREE`, `GREP_OPTIONS` and a
  HOME whose config runs a program. Seventeen mutants of the check, each on a
  throwaway clone, each failed by it (`check-mutants.py`,
  `check-mutants.txt`): round 3 adds no sentinel, a report is no capture, and
  the placeholder-home strip at any width.
- The harness's own mutants (`roots-mutants.py`, `roots-mutants.txt`): the
  placeholder on the first span only, not as wide as its root, no short form,
  survivors by substring, no trailing-slash strip, no `//`-collapsed
  spelling, no nF escape, a backslash escape -- eight, each failed by
  RootsRewrite or RunRoots.

**Counts** (`counts.txt`, by `counts.sh`, the round-2 shapes): files holding
each shape at `ccced1f5` -- macOS home 6, Linux home 5, harness scratch root
156, one broken after its `tmp/` 29, macOS temporary root 4, any 167; the user
name 0, the home path 0. The check refuses 159 of them on `ccced1f5`'s tree
(339 lines; round 1's check, by its shapes, 158 and 283). At the receipt's
commit, `counts.txt` gives the files still holding a shape and what excuses
each line, and the check refuses 0.

Commits `6b4f72fd`, `f05f3bd7` and `4a74f098` held one harness-shaped fixture
root in `red3.sh`, and `c6e12e82` the TMPDIR a mutant run's header named in
`roots-mutants.txt`; the check refuses each of them there, and the commits
after them assemble the one and write the other `<tmp>`. The check ran before
those files were staged, and reads only tracked files: it now runs after
`git add`.

## 5. Gate

Every step of CI's gate job, and the sweep workflow's `test-sweep.sh`, one at
a time under the suite lock, at the receipt's commit (`gate.txt`: the commit,
exit status, seconds and step of each; `gate.sh` is the script). Round 1's
gate was at `6b507197`, round 2's at `7d855291`: 20 steps, 0 failed, each. The
capture harness's suite is the sweep workflow's, not the gate job's:
`test-capture.txt`.

## 6. Round 2: the review of `1fe6ea37`

- **The dispatcher's `test-capture` failure** (`RunRoots`, both cases): its
  TMPDIR, `/private/tmp/claude-<uid>`, is three bytes shorter than round 1's,
  so each of the three roots before `tmp=` on the stub's one long row was
  three bytes shorter, and the terminal's wrap at column 200 fell nine
  columns later -- inside the `tmp=` root, after its first eight bytes.
  Neither part was a root, and both stayed raw. Round 1's own TMPDIR happened
  to wrap the row right before `=`. Real captures had the same defect. Fixed
  by finding roots in the visible text; the test wraps a root on purpose and
  runs at several lengths.
- **gpt6 MUST 1, a literal placeholder**: escaped before the rewrite.
- **MUST 2, a root a colour, a row's end or JSON splits**: the visible-text
  reading, and the fail-closed refusal of anything that survives.
- **MUST 3, the check's bypasses**: `>` starts a path, a home needs no slash
  after it, placeholder-prefixed homes are taken out before matching.
- **MUST 4, the mark**: a line-end comment only, never in a capture.
- **opus SHOULDs 4-7**, **gpt6 NTHs**: every verb in RunRoots, `:real`, one
  reading for `.ansi` and `.txt`, the receipt's counts and wording, atomic
  publishing, the check's scope.

## 7. Round 3: the review of `8e7c1c32`

`red3.sh` shows each finding on round 2's own code (`red3-r2.txt`, from a
clone at `8e7c1c32`) and the same on round 3's (`red3-r3.txt`):

| finding | round 2 | round 3 |
|---|---|---|
| gpt6 MUST 1: one root wrapped at two columns | the same after the rewrite | differ |
| gpt6 MUST 2: a mark in a `/trace` report capture | passes | refused |
| gpt6 MUST 3: a sibling of HOME | refused | left |
| opus SHOULD 1: a home cut at its first match | passes | refused |
| opus SHOULD 3: a TMPDIR through a link, with a trailing slash | exit 3 | exit 0 |
| gpt6 NTH: a literal placeholder in JSON | not JSON | valid |
| opus NIT: an nF escape after a root | `<work>` read as `<scratch>/work` | rewritten whole |

- **MUST 1 (dispatcher's decision: a placeholder of the root's width)**:
  section 1; the every-byte wrap test now expects the split at that byte, the
  paired test expects two offsets to differ, the SGR test expects each change
  at its offset. The tracked captures are written again at their roots'
  widths (section 3). A root narrower than four columns refuses the capture.
- **MUST 2**: `is_capture` reads capture.sh's file naming, not only the kind.
- **MUST 3**: survivors read a root where it ends whole; the run name stays a
  plain search.
- **opus SHOULD 1**: the strip runs on the line with a space after it.
- **opus SHOULD 3**: `${TMPDIR%/}` (every trailing slash) before mktemp, and
  `//`-collapsed spellings are roots.
- **Docs and NITs**: the layering of the run-name guard and the check's
  shapes (section 1), sweep.py's docstring, the check's "at a row's start or
  after a space", roots.py's box-drawing range (U+2500-U+259F, in the code and
  the doc), nF escapes, "within its row", the `roots:` line's `:real`
  widths, the percent-encoding claim narrowed to what `quote` produces.

## Left as it is

- capture.sh's own standard output still names the out directory and the
  binary's path (`guards:`, a snap's file name); the sweep keeps it as
  `capture.log`, outside the repository. A receipt that commits one is held by
  the check.
- A literal placeholder escaped with `!` is one column wider than it was.
- PLAUSIBLE (opus): rows are joined with nothing between them, so a row that
  ends in a whole root and a next row that opens with `/work` read as one
  root; both sides get the same rule, and the run's random suffix makes such
  a join unlikely.
