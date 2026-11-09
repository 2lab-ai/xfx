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
review of `8e7c1c32` (section 7); round 4 the review of `21bb78b2` (section
8); round 5 the review of `4c3f2e50` (section 9); round 6 the review of
`e1d34372` (section 10); round 7 the review of `15e37f4a` (section 11);
round 8 the review of `98af91ed` (section 12).

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
  resolved, still in the file's visible text where it ends whole -- the
  rewrite's own rule, so a sibling whose name extends a root's is no
  survivor, coloured mid-name or not -- or in its bytes where they overlap
  a control sequence's (an OSC payload no reader sees; a root whose head an
  escape's own bytes took, refused even beside a styled sibling, which no
  reader can tell from it there); a root with runs of gutter characters in
  it (a space, a tab, U+2500-U+259F), read in the visible text with every
  subset of them taken out, since the view drops a run whole where a row's
  edge falls beside it (a root ending in one at a row's end, or broken beside
  one) -- refused by the rewrite before it edits anything, so a root under
  another one cannot slip out once the outer one is written, and by the
  survivor reading (`--check`); or
  the run directory's own name (`fxcap.` and mktemp's tail) anywhere: exit 3,
  the file named, the survivor by placeholder. A root with more than six such
  runs is refused outright. A root a
  binary cut short, or whose head scrolled off a frame, keeps the run name.
  That guard covers the run directory and what is under it; a fragment of a
  root above it (the harness's TMPDIR, the user's HOME) holds no run name,
  and in a tracked file the check's shapes (section 4) are what refuse it.

**Identical on both sides.** One rule for fx's run and xfx's. In a sweep unit
the two runs share one TMPDIR, so every root is as long on both sides and so
is its placeholder. The two binaries' own TMPDIRs are the exception, `fxtmp`
a byte shorter than `xfxtmp`: no roots of their own, they stay as the run
directory's placeholder and the binary's name, `<scratch___>/fxtmp`. The
ledger's `capture-tmpdir` reads fx's name as xfx's right after the run
directory only -- `<scratch___>/`, `<scratch:real___>/`, a zero-filled
`<scratch>/`, or the old `fxcap.XXXXXX/` -- and nowhere else, and only the
name with a directory under it, `fxtmp/`, the one form the tracked captures
hold: before anything else (`fxtmp-old`, `fxtmp,backup`, `fxtmp backup`,
`fxtmpé`, the bare name before a quote) it is compared. capture.sh's `dir`
refuses a name that is or extends a binary's TMPDIR; `run-directory` stays
for captures taken before.

**Widths.** The rewrite comes after the binary laid out its frame and keeps
it, column for column. The `roots:` line each run prints first gives each
root's length on that run in bytes, the resolved form's too where it is
another path; a placeholder is as wide as its root in columns, the same
number for an ASCII root.

The sweep writes its committed summaries through `scripts/fxcap/redact.py`,
which keeps a path that continues a placeholder (`<scratch___>/home/...`,
whatever its width: `roots.PLACEHOLDER`) and still redacts one after a `>`
that is no placeholder.

**Tests** (`scripts/fxcap/test_capture.py`; RED and GREEN in `harness.txt`,
`red3-*.txt` and `red4-*.txt`):
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
  rewritten nor refused, plain or styled mid-name by an SGR, a 256-colour SGR
  or an nF escape (and roots.py itself exits 0 on it), and those roots
  themselves refused; a root in an OSC 8 target, and one whose head an nF
  escape took, still refused, beside a styled sibling too; a root ending in
  a space, a tab or U+2502 at a row's or the text's end, a spaced root
  broken where its space ends or opens a row or behind a box-drawing gutter,
  and a root with two spaces broken at either or both, refused (each spaced
  root inside a row is written; a root ending in a gutter run is refused
  there too, the conservative case of section 11); a root with seven runs
  refused outright (exit 3); the
  refusal of a cut-short root, with no path printed; a file without a root,
  byte for byte.
- `RunRoots`, end to end over a stub binary on tmux, every expectation at the
  root's own width: one item a row -- its working directory, resolved, HOME,
  TMPDIR, the harness's TMPDIR, the user's home, the scenario directory, a
  root a name character follows, two padded roots, a root the terminal wraps
  at its margin (seven columns in), a root broken behind `  │ `, a root a
  colour change splits (its changes at their offsets in the `.ansi`), a
  coloured root, a link, a title, a bell -- and a copy, a copied file, a
  report, requests to the fake gateway and the MCP server. Every file kind
  holds no root and no run name, at three TMPDIR lengths in one run; a TMPDIR
  through a link and with a trailing slash (the binary's HOME as given, as
  wide as the run: no `//` widened it); a sibling of the harness's TMPDIR;
  fx and xfx in one TMPDIR, row for row; each side breaking a root at another
  column; a real root against a literal `<work>`.
- The harness suite at four TMPDIRs: 23 bytes (the dispatcher's), 26, 52 (past
  test_capture.py's 50-byte limit, so on `/tmp`, a link) and 27 with a trailing
  slash (`test-capture.txt`): 102 tests at each in round 7 (at `797ebfa0`),
  OK at 23, 52 and 27; at 26 one test failed once on the stub bash's own
  `setpgid` error in its pane (a message no earlier round's log holds), then
  passed five times alone and the whole suite passed once more there. 101 OK
  at each in rounds 5 and 6, 99 in round 4, 97 in round 3. Round 2's 89 and round 1's 78 are in the same file.

**In real captures** (`runs-scan.txt`, `runs_scan.py`: a whole root, the
shapes, and what a broken root leaves -- `claude-<digits>`, a sweep job's
`fxsw.` name, a run's `fxcap.` name). Of the baseline sweep's 677 capture
files, 251 held some part of a run's paths; after round 1, 29 (round 1's own
count saw only whole roots); after rounds 2 to 7, 0. Rounds 3 to 7: 273
hold a placeholder where a path stood (an image badge's link reads
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
`43024057`'s; round 2: `6cb5c4e7`'s; round 3: `f05f3bd7`'s; round 4:
`d3804683`'s (its ledger's narrower `capture-tmpdir`); round 5: `4abe4feb`'s
(that rule ended where a path name ends, and the survivor rule's gutter
readings), and its first fix `bb5f53ed`'s; round 6: `89a3fef1`'s (that rule
followed by a delimiter from a list, the rewrite's refusal); round 7:
`797ebfa0`'s (that rule `fxtmp(?=/)` only), each from a clone, xfx built
there.

**145 pairs before, 145 after round 7, 145 with the same verdict, 0
changed**, and the same against rounds 1 to 6 and round 5's first fix
(`compare_verdicts.py`): normalized-identical 102, identical 4,
approved-divergence-cascade 12, unresolved-defect 27 in all nine, pass 106 /
145 in all nine. No pair moved under `fxtmp(?=/)`: in round 7's sweep
captures, `/` is the only character after `fxtmp` or `xfxtmp` (8 times,
`runs-scan.txt`).

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

**Zero-filled placeholders from before this unit stay zero-filled**
(`zero-fill.py`, `zero-fill.txt`). 1307 tracked captures already held a
`<work>`, `<scratch>`, `<tmp>` or `<home>` (or a `:real` one) at `ccced1f5`
-- w3's 14, w5's 56 and w9's 14 among them; the rest, at the revision
`zero-fill.txt` names, are this unit's own receipt text. capture.sh itself wrote `<scratch>` then in a `home` dump (with
`<scenarios>` and `<bun>`), a clipboard file and a report (`sed
"s#$run#<scratch>#g"`, its lines 942, 1096 and 1109 at `ccced1f5`);
sweep.py and ci_sweep.py write `<tmp>` in their summaries (sweep.py:1130,
ci_sweep.py:595); units wrote others by hand. No column was kept there, and
no round rewrites them: a scrub touches only a root it finds raw. Each reader
takes the zero fill as a fill of none (`roots.PLACEHOLDER`, the check's
placeholder homes, the ledger's `<scratch>/fxtmp`).

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
  at a row's start, after a control sequence or after any character no path
  name holds (such a root broken after its `tmp/`, a README's backtick-quoted
  one among them), not after a name character or `/` (`models/claude-3/`);
  macOS's per-user temporary root.
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
  `.json`, `.log`, ...). No directory. `gate.txt`'s check line gives the
  marked and placeholder lines at the gate's commit (`counts.txt` lists the
  marked ones at `7f2298f7`, and the files of the rest).
- One `git grep` over the tree, one per file that holds a shape: no names on a
  command line, so no batching; about 2 s on this tree (check-no-secrets.sh:
  55 s). The guards are check-xfx-identity.sh's (every `GIT_*` dropped,
  `--work-tree=.`, the root's own `.git`, index bits and missing files
  refused), a positive control runs first, and a hit prints the file, the line
  and the shape, never the text.
- Its test: 10 cases, 26 plants -- every shape in every kind of file (a name
  git quotes, a file named `-`, a binary, a coloured path, an OSC 8 link, a
  redirect, an HTML tag, a bare home, a broken root at a row's start and
  after a backtick, a mark in a request body,
  a `.txt`, a report capture and mid-line, a home cut by its first match, a
  report renamed out of capture.sh's names with its line unmarked, an
  authored file named into them with its line marked) and every form that
  must pass (padded and short placeholder homes among them, model paths with
  `claude-<digits>/` inside a name or after a `/`, and the renamed report
  with its line marked, counted as marked),
  under `GIT_DIR`, `GIT_INDEX_FILE`, `GIT_WORK_TREE`, `GREP_OPTIONS` and a
  HOME whose config runs a program. Nineteen mutants of the check, each on a
  throwaway clone, each failed by it (`check-mutants.py`,
  `check-mutants.txt`): round 3 added two (no sentinel, a report is no
  capture) and retargeted one (the placeholder-home strip, now at any width);
  round 7 two (round 6's broken-root left context, and one that lets `/`
  lead it).
  Round 4 runs each once more with the check's own positive control taken
  out, so its line names the test's own cases and plants that fail it: all
  17 still fail, most by `shapes` (the plants) or `passes` alone, and the
  unmutated check passes either way.
- The harness's own mutants (`roots-mutants.py`, `roots-mutants.txt`, each
  line naming the assertion that failed it): the placeholder on the first
  span only, not as wide as its root, no short form, survivors by substring,
  no trailing-slash strip, no `//`-collapsed spelling, no nF escape, a
  backslash escape (round 3's eight); round 4's survivors read every byte
  (round 3's rule), read none, or read an OSC's only; and the ledger's
  `capture-tmpdir` after any `_>/` (round 3's), after any placeholder, after
  the short form too, or with no zero fill; round 5's survivors with no
  reading at the gutters (round 4's), every gutter out at once (its first
  fix), one run out at a time only, or no bound on the runs, and the ledger
  with a word boundary again (round 4's) or no end at all; round 6's rewrite
  without its refusal; round 7's ledger with round 5's negative class,
  round 6's delimiter list, `/` or the row's end, or `/` or white space, and
  capture.sh's `dir` without its refusal of the TMPDIR names; round 8's
  `dir` refusing lower case only -- 28 at round 8 (`roots-mutants.txt`),
  each failed by RootsRewrite, RunRoots, Refusals or test_sweep.py's
  Normalize. Round
  5's first fix widened the byte rule to the view's trimmed gutters; that
  mutant, run against the readings, survived over the tested cases (section
  9 says why it adds nothing), and the code is round 4's overlap again.

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
gate was at `6b507197`, round 2's at `7d855291`, round 3's at `00b8ed9e`,
round 4's at `730692c8` (the code of `cd9e446d`), round 5's at `026a7c58` (the
code of `4abe4feb`), round 6's at `3ed4a8c4` (the code of `89a3fef1`), round
7's at `5743e3c7` (the code of `1d707ba4`), round 8's at `bfdb2703` (the code
of `0ab7b6dd`): 20 steps, 0 failed, the tree clean, each.

**Every commit, scanned** (`commits.sh`, `commits.txt`): each commit of the
branch checked out in a throwaway clone and scanned by its own check and by
the head's, as a push publishes it, with the lines the head's check names
beyond what the base held (by file and shape), through `bfdb2703`. Round 7's
check, widened, refuses 341 lines on the base and finds the base's own two
README fragments in every commit up to `1a7cb6a1` (0 beyond the base's;
`1d707ba4` rewrites them). From `6cb5c4e7` on, every commit passes its own
check, but for section 11's one (`2e90c292`, the plant quoted with its uid,
fixed in `5743e3c7`), section 10's two (`c57ffb77`, `e0b0e697`: two lines
each, the stand-in home in a docstring, removed in `89a3fef1`) and the four
section 4 names (`6b4f72fd`, `f05f3bd7`,
`4a74f098`, `c6e12e82`: one line each, added and removed in round 3). Before
it, rounds 1 and 2 had not yet scrubbed all of the base's own paths: 339 to
56 lines named, every one the base's, except one line in `26d6c639` to
`56e51cb0` -- a test comment in `scripts/fxcap/test_sweep.py` naming a
fixture's redirect into `bob`'s home, which round 1's check, with no `>` as a
path's start, passed; `506febef` (round 2's MUST 3) refuses it, and the
comment was reworded there. No revision from the base to `bfdb2703` holds
the user's name or home path (65 revisions, by `git grep`), and no commit
message does (64): names-scan.sh, the end of `commits.txt`. The
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
clone at `8e7c1c32`) and the same on round 3's (`red3-r3.txt`, from a clone
at `21bb78b2`; both run again in round 4, when the trailing-slash line came
to pin the guard's own line as well as the exit status):

| finding | round 2 | round 3 |
|---|---|---|
| gpt6 MUST 1: one root wrapped at two columns | the same after the rewrite | differ |
| gpt6 MUST 2: a mark in a `/trace` report capture | passes | refused |
| gpt6 MUST 3: a sibling of HOME | refused | left |
| opus SHOULD 1: a home cut at its first match | passes | refused |
| opus SHOULD 3: a TMPDIR through a link, with a trailing slash | exit 3, 2 `LOCAL-PATH GUARD` lines | exit 0, none |
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

## 8. Round 4: the review of `21bb78b2`

`red4.sh` shows each finding on round 3's own code (`red4-r3.txt`, from a
clone at `21bb78b2`) and the same on round 4's (`red4-r4.txt`):

| finding | round 3 | round 4 |
|---|---|---|
| gpt6 MUST 1 / opus SHOULD 1: a sibling of HOME, TMPDIR or the scenario directory, styled mid-name by an SGR, a 256-colour SGR or an nF escape (9) | refused, all 9 | left, all 9 |
| the same: HOME in an OSC 8 target; its sibling there; HOME whose head an nF escape took | refused; left; refused | refused; left; refused |
| gpt6 MUST 2 / opus SHOULD 2: `fxtmp` after `<work___>`, `<work:real__>`, `<tmp______>`, `<!scratch__>`, `a_>`, `<s__>`, `<S__>` (7) | normalized, all 7 | compared, all 7 |
| the same: after `<scratch___>`, `<scratch:real___>`, `<scratch>`, `fxcap.XXXXXX` | normalized | normalized |
| gpt6 SHOULD 1: a report renamed `trace-report.md`, unmarked; marked; an authored `fx-guide-40x120-notes.md`, marked | refused; passes; refused | the same, now pinned |
| opus NIT: verify.py's width of an OSC 8 link | 32 (its payload counted) | 4 |

- **MUST 1**: survivors read a root's bytes only where they overlap a
  control sequence; the visible text judges the rest, by the rewrite's rule.
  That is the reviewers' "OSC ranges only" and one case more: a root whose
  head an escape's own bytes took (`ESC / h` is an nF escape), which round 3's
  byte search refused and an OSC-only reading would pass (the mutant "bytes
  in an OSC only" is failed by that test).
- **MUST 2**: `(<scratch(?::real)?_*>/|fxcap\.XXXXXX/)fxtmp\b` -> `\1xfxtmp`.
  The short form is not covered: the run directory, `$TMPDIR/fxcap.XXXXXX`,
  is 13 columns or more and never narrower than `<scratch>`; resolved, it is
  narrower than `<scratch:real>` only where TMPDIR resolves to `/`. Both
  short forms are negatives.
- **SHOULD 1 (gpt6)**: the name is reserved, in capture.sh and in the check's
  header; the test pins both edges. A renamed report is authored text: its
  marks excuse their lines and an unmarked shape is refused. An authored file
  named like a capture is a capture: no mark excuses it.
- **SHOULD 2 (gpt6), opus SHOULD 3**: the `roots:` line is bytes, a
  placeholder columns (capture.sh, section 1); capture.sh's round-2 "a stored
  row is shorter by it" and its TMPDIR paragraph's "one placeholder for both
  would shorten xfx's rows" are gone; the check's lines 12-14, 46 and 51-54
  name the width form and the reserved names; section 4's mutant count and
  section 3's zero-filled placeholders.
- **NITs (opus)**: the trailing-slash test's `assertNotIn("//", run)` could
  not fail (its operand comes from `glob` and `join`); it now reads the
  binary's HOME as given, as wide as the run, which the "no trailing-slash
  strip" mutant fails (`roots-mutants.txt`). `red3.sh` pins the
  `LOCAL-PATH GUARD` line. verify.py's ST is ESC and one backslash; verify.py
  run again reads the same (`verify.txt` unchanged: 7748 of 7748 rows).

## 9. Round 5: the review of `4c3f2e50`

`red5.sh` shows each finding on round 4's own code (`red5-r4.txt`, from a
clone at `4c3f2e50`), on round 5's first fix (`red5-r5a.txt`, `bb5f53ed`) and
on round 5's code (`red5-r5.txt`, the code of `4abe4feb`):

| finding | round 4 | `bb5f53ed` | round 5 |
|---|---|---|---|
| gpt6 MUST: `<scratch>/fxtmp-old/a`, `fxtmp.bak`, `fxtmp~`, `fxtmp+1` | normalized | compared | compared |
| the same: `fxtmp_x`, `fxtmpx` | compared | compared | compared |
| the same: `<scratch>/fxtmp/b.md`, a row ending `fxtmp`, `"<scratch>/fxtmp"` | normalized | normalized | normalized |
| opus PLAUSIBLE 1: a root ending in a space, a tab or U+2502, at a row's or the text's end (6) | passes, all 6 | refused | refused |
| the next break past it: `/tmp/a b` broken where its space ends or opens a row, or behind a box-drawing gutter (3) | passes, all 3 | refused | refused |
| the one past that: `/tmp/a b c` broken at its first space, its second, both (3) | passes, all 3 | passes 2, refused 1 | refused, all 3 |
| a spaced root inside a row | written | written | written |
| gpt6 SHOULD / opus PLAUSIBLE 2: `ESC <HOME> ESC[1m 2/notes` | refused | refused | refused, pinned and documented |

- **MUST**: `fxtmp(?![A-Za-z0-9._~+-])`. The set is roots.py's EDGE, the
  characters that continue a path name for the rewrite (`ends_whole`, the
  survivor rule); capture.sh's `writable` set is the same, `/` aside, without
  `~`. The ledger ends `fxtmp` where the rewrite would end a root. The
  coordinator's two forms name what may follow instead (`/`, a space, a
  quote, `<`, `>`, ESC), which would leave a TMPDIR before `)`, `,` or `:`
  unread; EDGE reads every character that cannot continue a name, and
  nothing else.
- **PLAUSIBLE 1, fail closed, traced to the end.** The view drops a run of
  gutter characters whole where a row's edge falls beside it, and nothing
  else; so a root holding k runs reads, in the visible text, as itself with
  some subset of them taken out. survivors reads every such subset (`2^k - 1`
  readings, k at most 6: a root with more runs is refused in `rules_for`).
  That covers the root ending in one at a row's end (opus's case), one
  broken beside one (`/tmp/ab`), and a root broken at several (`/tmp/ab c`,
  `/tmp/abc`), which `bb5f53ed`'s single reading with every gutter out at
  once passed. The byte rule is round 4's control-sequence overlap again:
  `bb5f53ed` widened it to the view's trimmed gutters, and its mutant
  survived over the tested cases. The reason is structural, not a property
  test: a root's bytes hold no newline or escape and start with `/`, so the
  only bytes of a raw match the view can trim are a run of gutters the root
  ends in, at a row's end -- and the reading with that run taken out ends
  there, at the row's end. opus's `Refused` for
  every root ending in a gutter character would refuse each file of such a
  run where only some rows break; the readings refuse a file only where the
  root is broken. Roots with no gutter character, every root here and on the
  CI runners, read as in round 4.
- **SHOULD / PLAUSIBLE 2**: refused, conservatively, in roots.py's docstring
  and section 1; "siblings are left" is not claimed past what the view holds.
- **NITs**: roots.py's "an escape after them is no end of it" now says a
  colour change is read through and a row's end or a cursor move ends them;
  section 3's `<tmp>` writers in the present tense, cited
  (`zero-fill.txt`); CONTRIBUTING.md's rule wrapped.

## 10. Round 6: the review of `e1d34372`

`red6.sh` shows each finding on round 5's own code (`red6-r5.txt`, from a
clone at `e1d34372`) and the same on round 6's (`red6-r6.txt`, the code of
`89a3fef1`):

| finding | round 5 | round 6 |
|---|---|---|
| gpt6 MUST: `<scratch>/fxtmp` then `é`, `%C3%A9`, `中`, `@`, `=`, `#`, `(`, `!`, `<` (9) | normalized, all 9 | compared, all 9 |
| the same: round 5's `-old`, `.bak`, `~`, `+1` (4) | compared | compared |
| the same: `/`, a space, a tab, `"`, `'`, `)`, `]`, `}`, `>`, `,`, `:`, `;`, `|`, the row's end (14) | normalized | normalized |
| opus SHOULD 2: a scenario directory under HOME, broken at its space | published as `<home__>/My Docs/...` | refused, exit 3, the file untouched |
| the same, a root on its own: `/tmp/a b` broken at its space | rewritten, then refused by the survivor reading | refused before any edit |

- **MUST** (replaced in round 7, section 11): `fxtmp(?=[/\s"'\]}),:;|>]|$)`,
  the list the coordinator gave.
  What really follows the binaries' TMPDIR in the tracked captures is `/`
  alone (w2's and w3's, `fx-trace-...` and `xfx-image-...` under it); the
  rest are the delimiters a binary's text puts after a directory's name.
  Any other character can be part of a name (capture.sh's `dir` refuses only
  an empty name, `.`, `..` and `/`), so it is a sibling and is compared. The
  ledger now differs from roots.py's EDGE on purpose: an over-wide rewrite in
  roots.py fails toward privacy (a sibling written as a placeholder), an
  over-wide normalization in the ledger toward false parity (a sibling read
  as the TMPDIR).
- **SHOULD 2 (opus)**: rewrite() reads every root's gutter readings in the
  view before it edits and raises Refused where one ends whole (main: the
  guard, exit 3, the file not written). The tests read every broken case
  both ways: the rewrite refuses, and survivors refuses the unrewritten
  text.
- **SHOULD 1 (opus) and the NITs**: roots.py's fail-closed docstring and
  survivors' say what the code does (bytes: control-sequence overlap only;
  gutters: readings of the visible text, every subset); the ledger's `why`
  no longer claims capture.sh's `writable` holds EDGE (it lacks `~`, and the
  ledger no longer uses EDGE); `publish`'s refusal says roots.py's line gives
  the cause; roots-mutants.py names the first fix `bb5f53ed`, as section 9
  does; a positive test for each delimiter; `commits.txt` through the head.
- **SHOULD (gpt6)**: section 9 says the widened byte rule's mutant survived
  over the tested cases, and why it adds nothing, rather than that the
  readings refuse every file it did.
- **Two commits fail the check**: `c57ffb77` and `e0b0e697` hold a stand-in
  home literally in roots.py's new docstring (the check ran after `git add`
  and refused; a `| tail -1` hid its exit status). `89a3fef1` writes it
  `<HOME>`; the branch is not rewritten.

## 11. Round 7: the review of `15e37f4a`

`red7.sh` shows each finding on round 6's own code (`red7-r6.txt`, from a
clone at `15e37f4a`) and the same on round 7's (`red7-r7.txt`, the code of
`1d707ba4`):

| finding | round 6 | round 7 |
|---|---|---|
| gpt6 MUST: `<scratch>/fxtmp` then `,backup/a`, ` backup/a`, `:x`, `)`, the bare name, `"`, `\|` (7) | normalized, all 7 | compared, all 7 |
| the same: `-old/a`, `é/a` | compared | compared |
| the same: `<scratch>/fxtmp/b.md` | normalized | normalized |
| gpt6 SHOULD: `/tmp/fxt ` whole inside a row (2) | refused | refused, now pinned and documented |
| opus SHOULD 1: `dir fxtmp-old` | made beside the TMPDIRs, exit 0 | refused, exit 2 |
| opus SHOULD 2: `` (`claude-<digits>/p5uc`) `` | passes | refused |
| the same: `models/claude-3/x`, `anthropic.claude-3/x` | passes | passes |

- **MUST**: the ledger reads only `fxtmp(?=/)`. Round 6's list (the
  coordinator's own, the coordinator says) let a sibling whose name starts
  with a listed character through, and every one of them is legal in a
  name; `/` is the one follower the tracked captures hold (w2's and w3's
  reports and images). Every other follower is compared: the rule fails
  toward comparing. Thirteen of test_sweep.py's fourteen round-6 delimiter
  positives are now negatives (the bare name before each of them); the
  fourteenth, `/`, stays the one positive, `fxtmp/`. Beside them are every
  round 5 and 6 sibling and `fxtmp,backup/a`, `fxtmp backup/a`, `fxtmp:x`,
  `fxtmp)`.
  Why the ledger is not roots.py's EDGE: an over-wide rewrite in roots.py
  fails toward privacy (a sibling written as a placeholder), an over-wide
  normalization in the ledger toward false parity (a sibling read as the
  TMPDIR); so roots.py reads any non-name character as a root's end, and the
  ledger reads only the demonstrated form.
- **SHOULD (gpt6)**: a root ending in a gutter run, whole inside a row, is
  refused (its reading without the run ends whole there too). Conservative,
  kept, as the coordinator decided: pinned by a test that expects the
  refusal (mid-row and before `/notes`), in roots.py's docstring and here.
- **SHOULD 1 (opus)**: capture.sh's `dir` refuses `fxtmp*` and `xfxtmp*`
  (six names tested, and a plain `dir` still runs).
- **SHOULD 2 (opus)**: the two receipts write `claude-<uid>`. The check's
  broken-root shape is found after a row's start, a control sequence, or any
  character no path name holds; not after a name character or `/`, so a
  model path (`models/claude-3/`, `anthropic.claude-3/`, `my-claude-2/`)
  passes. Over the tree, before the two receipts were rewritten, it hit
  their two lines and nothing else (`counts.txt`: the shape in 2 files at
  `15e37f4a`, 0 at `797ebfa0`); at the head the check is ok (`gate.txt`).
  The check's test plants `` (`claude-<uid>/p5uc`) ``, with a uid, and passes the model
  paths; counts.sh counts the same shape (`counts.txt`).
- **NITs**: the nested-root test and red6.sh use `/Users/someone`, the
  check's own placeholder home, not an assembled one; roots-mutants.py
  prints the first failing subTest.
- **One more commit fails the check**: `2e90c292`'s section 11 quoted the
  plant with its uid, which the widened shape refuses. The check ran after
  `git add` and refused; `set -e` does not hold inside the shell's `eval`,
  so the commit went on. The line is now `claude-<uid>`, and every commit
  after it runs behind an explicit `if <check>; then git commit; fi`.
  `1d707ba4`'s commit message quotes the same fragment, the one already on
  main; a commit message is outside the check's scope (tracked files).

## 12. Round 8: the review of `98af91ed`

| finding | round 7 | round 8 |
|---|---|---|
| gpt6 MUST: `dir FXTMP`, `FxTmp`, `XFXTMP`, `Fxtmp-old`, `xFxTmP.bak` (5) | made, exit 0 (`FXTMP` names `$run/fxtmp` on this APFS volume) | refused, exit 2, before launch |
| the same: `dir Notes`, `tmp`, `xfx`, `side` | run | run |
| gpt6: `cache@claude-<n>/x`, an accented letter before `claude-<n>/` | refused, unsaid | refused, pinned as conservative and said |

- **MUST (gpt6)**: capture.sh's `dir` refuses `[fF][xX][tT][mM][pP]*` and
  `[xX][fF][xX][tT][mM][pP]*`. This volume is case-insensitive (`diskutil
  info /`: APFS; a `mkdir fxtmp` answers to `FXTMP`), so `dir FXTMP` named
  fx's own TMPDIR and its `from` was copied into it. Bracket classes, not
  `nocasematch`: macOS's bash 3.2 parses them, and a run of capture.sh under
  `/bin/bash` (3.2.57) with `dir FxTmp` exits 2 with the refusal. The test
  (`Refusals.test_a_dir_never_takes_a_binarys_tmpdir_name`) adds the five
  case spellings, and runs `Notes`, `tmp` and `xfx` beside `side`. RED and
  GREEN under `/bin/bash` 3.2 in `red8-r7.txt` (98af91ed: the five run) and
  `red8-r8.txt` (refused); the mutant that refuses lower case only fails at
  `FXTMP` (`roots-mutants.txt`, 28 of 28). The harness suite, for a guard
  change once, at TMPDIR 23 bytes: 102 tests OK (`test-capture.txt`).
- **gpt6 (receipt)**: section 11 says thirteen of the fourteen round-6
  positives became negatives, and `fxtmp/` stays the positive.
- **gpt6 (the check's comment)**: the check's header calls its character
  sets a heuristic: `@`, `=` and a letter past ASCII can be in a name, so
  `cache@claude-<n>/x` and an accented letter before `claude-<n>/` match. It
  fails toward refusal, and the test pins both (28 plants).
- **SHOULD 1 (opus)**: the ledger cites capture.sh:367-373, :947, :954
  (`TMPDIR=$run/fxtmp`, `TMPDIR=$run/xfxtmp`) and :1123 (`keep`).
  `ledger-refs.py` prints the line each capture.sh citation names
  (`ledger-refs.txt`). ledger.py's `_sources` checks only that a file has
  that many lines, so a cite that drifts onto another line still passes:
  that mechanization is REF-LINT's.
- **SHOULD 2-3 (opus)**: section 4 cites `gate.txt` for the marked and
  placeholder counts rather than a number that ages; section 1 says only
  spaced roots are written inside a row, a gutter-ending one refused there.
  runs-scan.txt's `afterN:` lines name runs-tail.sh, and commits.txt's last
  lines names-scan.sh.

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
- A root with a gutter character in it, broken beside one at a row's edge,
  or ending in one there, refuses the file rather than being written: the
  view has lost that run, and a placeholder written over the rest would not
  be as wide as the root. A sibling that reads like such a root with a run
  taken out (`/tmp/ab` for `/tmp/a b`) is refused too. No root here or on the
  CI runners has a gutter character.
- PLAUSIBLE (opus, round 3): a placeholder a terminal wrapped between its `_`
  and its `>` leaves `>/fxtmp` on the next row, which `capture-tmpdir` does
  not read: such a pair shows the row as a difference, so the rule fails
  toward comparing, never toward hiding one. A HOME-prefixed sibling in a
  tracked file relies on the check's shapes, which miss a `\/`-escaped form.
- A report renamed out of capture.sh's names is read as authored text (gpt6
  round 3): the name is the convention, documented as reserved.
