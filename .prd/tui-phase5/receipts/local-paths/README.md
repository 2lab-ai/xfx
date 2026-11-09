# LOCAL-PATHS: no path of one machine in the tracked tree

xfx is public, and machine-local paths kept reaching it: on `main` at
`ccced1f5`, 156 tracked files held one machine's harness scratch root, every
image capture embedded the run directory (an OSC 8 link to
`<root>/fxcap.<run>/work/shot.png`), and four units in one night committed
scratch paths in receipt scripts and READMEs. This unit fixes the source
(`scripts/fxcap/capture.sh`, `scripts/fxcap/roots.py`), writes out what is
already there, and adds a gate that refuses it by shape.

Every claim below names the file in this directory that backs it. Base:
`ccced1f5`. Branch `fix/p5-local-paths`. Round 2 answers the two-engine
review of `1fe6ea37` and the dispatcher's gate, whose `test-capture` failed
there (section 6).

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
| `$run/work`, the binary's working directory | `<work>` | `<work:real>` |
| `$run`, the rest of the run (its HOME `<scratch>/home`, its TMPDIR `<scratch>/fxtmp` or `<scratch>/xfxtmp`) | `<scratch>` | `<scratch:real>` |
| the scenario directory | `<scenarios>` | |
| the `bun` that runs fakegw | `<bun>` | `<bun:real>` |
| capture.sh's own `$TMPDIR` (not a shared `/tmp`) | `<tmp>` | `<tmp:real>` |
| capture.sh's own `$HOME` | `<home>` | `<home:real>` |

How `roots.py` finds and writes a root (its docstring is the whole rule):

- **In the text a reader sees.** Control sequences are set aside and rows are
  joined across their ends and the gutters at them, so a root is found where
  a colour change splits it (the `@plan` picker bolds matched letters inside
  a path), where a terminal wrapped it at the margin, and where a binary broke
  it onto its next row behind spaces or a box-drawing border (fx's notice);
  and inside an OSC payload by its bytes (an OSC 8 link's target). Its
  JSON-escaped (`\/`) and percent-encoded spellings are the same root. It ends
  where no path-name character follows it, a row's end being none.
- **Where it stood.** The placeholder takes the place of the root's first
  visible character; its other characters go; every control sequence, gutter
  and row end between them stays, in order. A root broken across rows leaves
  its placeholder on the first row. Colours that changed inside a root are
  replayed after its placeholder: the state after it is exact, which of its
  characters carried them is not kept.
- **A literal placeholder is never a root.** Text that already reads `<work>`
  (or `<\work>`, ...) gains one backslash before anything is rewritten, so a
  placeholder with none is a root and nothing else.
- **A resolved root is its own placeholder.** A binary that prints `$HOME` and
  its pair that prints `realpath(HOME)` differ in the capture as they did on
  the screen; under a TMPDIR without a link the two forms are one.
- **What survives refuses the file.** A root in any form, or the run
  directory's own name (`fxcap.` and mktemp's tail), still in the file's bytes
  or visible text after the rewrite: exit 3, the file named, the survivor by
  placeholder. A root a binary cut short (`…/fxcap.Ab12Cd/work`), or whose head
  scrolled off the top of a frame, is found by that name.

**Identical on both sides.** One rule for fx's run and xfx's. In a sweep unit
the two runs share one TMPDIR, so every root is as long on both sides. The two
binaries' own TMPDIRs are the exception, `fxtmp` a byte shorter than
`xfxtmp`, and are kept apart under `<scratch>`: one placeholder for both would
shorten xfx's rows by a byte more than fx's. The ledger's `capture-tmpdir`
reads `<scratch>/fxtmp` and `<scratch:real>/fxtmp` as well as the old
`fxcap.XXXXXX/fxtmp`; `run-directory` stays for captures taken before.

**Widths.** The rewrite comes after the binary laid out its frame, so a path
the binary wrapped or padded keeps its layout, and the stored row is shorter
where a placeholder stands (a row a broken root's placeholder ends can be
longer than the screen). A claim that measures a path's width reads the
`roots:` line each run prints first, each root's length in bytes on that run.

The sweep writes its committed summaries through `scripts/fxcap/redact.py`,
which wrote any path under a local root as `<local-path>`, a run's HOME
(`<scratch>/home/.fx/mcp.json`) too. A path that continues one of the
placeholders above is now left as it is; a bare `>` (a redirect) is no
placeholder and is still redacted (`Redaction.test_a_path_under_a_capture_placeholder_is_left_and_a_redirect_is_not`).

**Tests** (`scripts/fxcap/test_capture.py`; RED and GREEN in `harness.txt`):
- `RootsRewrite`, on bytes: a root wrapped at the margin at every one of its
  bytes; broken behind a gutter of spaces and colours (s1's shape) and of
  box-drawing borders; split by colour changes (at-plan's shape), the `.ansi`
  and the `.txt` reading the same text after; JSON, percent-encoded and OSC 8
  spellings; a root a name character follows; a literal placeholder, one
  split by a colour, one with backslashes; a resolved root through a link;
  the refusal of a cut-short root, exit 3, with no path printed; a file
  without a root left byte for byte.
- `RunRoots`, end to end over a stub binary on tmux: one item a row -- its
  working directory, resolved, HOME, TMPDIR, the harness's TMPDIR, the user's
  home, the scenario directory, a root a name character follows, two padded
  roots, a root the terminal wraps at its margin (it starts seven columns
  before it, whatever its length), a root broken behind `  │ `, a root a
  colour change splits, a coloured root, a link, a title, a bell -- and a
  copy, a copied file, a report, requests to the fake gateway and to the MCP
  server carrying its directory. Every file kind the verbs write, each read on
  its own (the `.txt` row by row, the `.ansi` apart from it), holds no root
  and no run name, at three TMPDIR lengths in one run; a linked TMPDIR gives
  `<work:real>` beside `<scratch>/home`; fx and xfx in one TMPDIR read the
  same row for row but their own TMPDIR names, the padded column where it was;
  and a real root on one side against a literal `<work>` on the other never
  reads the same.
- RED: every new RunRoots case fails against `1fe6ea37`'s capture.sh, at the
  dispatcher's TMPDIR; GREEN: the whole harness suite at three TMPDIR lengths
  (`test-capture.txt`).

**In real captures** (`runs-scan.txt`, `runs_scan.py`, which counts a whole
root, the shapes, and what a broken root leaves: `claude-<digits>`, a sweep
job's `fxsw.` name, a run's `fxcap.` name). Of the baseline sweep's 677
capture files, 251 held some part of a run's paths. Of round 1's after sweep,
29 still did: s1's `skills` notices, broken after `/private/tmp/`, and
at-plan's bolded `.ansi` rows -- round 1's count saw only whole roots and
shapes, and reported 0. Of round 2's, 0 of 677 by any of the three counts,
and 273 hold a placeholder where a path stood (an image badge's link reads
`file://<work>/...`). No round-2 run was refused: `failed 0`.

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
`43024057`'s. Round 2: `6cb5c4e7`'s harness from a clone, xfx built there.

**145 pairs before, 145 after round 2, 145 with the same verdict, 0 changed**,
and the same against round 1 (`compare_verdicts.py`): normalized-identical
102, identical 4, approved-divergence-cascade 12, unresolved-defect 27 in all
three, pass 106 / 145 in all three. No pair moved from normalized-identical to
identical. The capture harness's own suite: 76 tests OK before, 78 after
round 1, 89 after round 2 at each of three TMPDIR lengths, 23, 26 and 52
bytes (`test-capture.txt`).

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

**By script, round 1** (`scrub.py`, `scrub-manifest.tsv`, `19a6b596`): 143
files, by line. A sentence that names the root as a thing ("the run directory
`<root>/fxcap.XXXXXX` is written `<scratch>`") keeps it as the generic shape
`/private/tmp/claude-<uid>` (17); everywhere else the capture placeholders,
the longest first: `<work>` 88, `<scratch>` 112, `<tmp>` 105.

**By script, round 2** (`scrub.py --round-2`, `scrub-manifest-2.tsv`,
`6cb5c4e7`): 35 files whose roots no line held whole -- s1's 28 `skills`
frames (fx broke the root after `/private/tmp/` behind a gutter), w5's two
at-plan `.ansi` frames (the picker's bold letters split it), and five
comparison reports that quote rows opening with a root's tail. Found in the
visible text, as `roots.py` finds them; 30 split, 83 tails. A marked line is
left in both rounds.

**What `<tmp>` means in a scrubbed file:** only the harness root,
`/private/tmp/claude-<uid>` -- what followed it (`<tmp>/fsg`, `<tmp>/bin/...`)
is the file's own. In a capture written by capture.sh since this unit, `<tmp>`
is capture.sh's whole `$TMPDIR`, which in a sweep is the sweep's job
directory.

**Read again** (`verify.py`): every rewritten file with a counterpart on the
other side, fx against xfx, at `ccced1f5` and now, by position, by
`scripts/fxcap/cells.py`'s reading and by the sweep's normalized reading.
Round 1 (`verify.txt`, read again after round 2): 41 pairs read the same; 6
(d1's `read-failures` Ctrl-O screens) read the same but for one row whose only
difference was the run directory's random suffix, each side's own; 0
otherwise. Round 2 (`verify-2.txt`): 15 pairs read the same, 0 otherwise. With
nothing to pair against, listed as raw evidence rewritten: 21 files in round 1
and the 5 comparison reports of round 2 (each file says which).

**Claims about a width, handled by name:** s1's "the same length, so no row
moves" and d1's "replacing it would move the breaks" say what LOCAL-PATHS
changed and that the pairs read as they did; flake-status-s1's table, keyed on
which directory names hold `s1`, notes `<tmp>` holds none.

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
  `/tmp/claude-<digits>` with or without `/private`, and a row that opens with
  `claude-<digits>/` (such a root broken after its `tmp/`); macOS's per-user
  temporary root.
- Not every path of a machine: a Windows path, `~user`, a percent-encoded
  path, a home directory broken across two rows and a scratch root broken
  anywhere but after its `tmp/` are not found by shape (capture.sh's rewrite
  finds its own roots in all of those forms).
- Exceptions: nine documented placeholder homes in the script (`/home/u`,
  `/Users/u`, `/Users/me`, `/home/me`, `/Users/example`, `/home/someone`,
  `/Users/someone`, `/home/you`, `/home/runner` -- GitHub's runners' and
  train-6's container user) and a home that continues one of capture.sh's
  placeholders (`<scratch>/home/...`), taken out of a line before it is
  matched again; and a line that ends in a `local-path-ok` comment, that line
  only -- never in a capture (`.txt`, `.ansi`, `.wire`, `.json`, `.log`, ..., a
  request body). No directory. At this branch: 8 marked lines, 77 placeholder lines
  (`counts.txt` lists the marked ones and the files of the rest).
- One `git grep` over the tree, one per file that holds a shape: no names on a
  command line, so no batching; about 2 s on this tree (check-no-secrets.sh:
  55 s). The guards are check-xfx-identity.sh's (every `GIT_*` dropped,
  `--work-tree=.`, the root's own `.git`, index bits and missing files
  refused), a positive control runs first, and a hit prints the file, the line
  and the shape, never the text.
- Its test: 10 cases, 21 plants -- every shape in every kind of file (a name
  git quotes, a file named `-`, a binary, a coloured path, an OSC 8 link, a
  redirect, an HTML tag, a bare home, a broken root, a mark in a request body,
  in a `.txt` and mid-line) and every form that must pass, under `GIT_DIR`,
  `GIT_INDEX_FILE`, `GIT_WORK_TREE`, `GREP_OPTIONS` and a HOME whose config
  runs a program. Fifteen mutants of the check, each on a throwaway clone,
  each failed by it (`check-mutants.py`, `check-mutants.txt`).

**Counts** (`counts.txt`, by `counts.sh`, the round-2 shapes): files holding
each shape at `ccced1f5` -- macOS home 6, Linux home 5, harness scratch root
156, one broken after its `tmp/` 29, macOS temporary root 4, any 167; the user
name 0, the home path 0. The check refuses 159 of them on `ccced1f5`'s tree
(339 lines; round 1's check, by its shapes, 158 and 283). At `7ff36c75`, 16
files hold a shape, every hit a marked line or a placeholder home, and the
check refuses 0.

## 5. Gate

Every step of CI's gate job, and the sweep workflow's `test-sweep.sh`, at
round 2's receipt commit, one at a time under the suite lock, and then the
check once more over the tree the last commit holds (`gate.txt`: the commit,
exit status, seconds and step of each; `gate.sh` is the script). Round 1's
gate, at `6b507197`, was 20 steps, 0 failed. The capture harness's suite is
the sweep workflow's, not the gate job's: `test-capture.txt`.

## 6. Round 2: the review of `1fe6ea37`

- **The dispatcher's `test-capture` failure** (`RunRoots`, both cases): its
  TMPDIR, `/private/tmp/claude-<uid>`, is three bytes shorter than round 1's,
  so each of the three roots before `tmp=` on the stub's one long row was
  three bytes shorter, and the terminal's wrap at column 200 fell nine
  columns later -- inside the `tmp=` root, after its first eight bytes.
  Neither part was a root, and both stayed raw. Round 1's own TMPDIR happened
  to wrap the row right before `=`. Real captures had the same defect
  (section 1). Fixed by finding roots in the visible text; the test now wraps
  a root on purpose and runs at three lengths.
- **gpt6 MUST 1, a literal placeholder**: escaped before the rewrite;
  `test_a_literal_placeholder_is_never_a_root` and
  `test_a_real_root_and_a_literal_placeholder_never_read_the_same`.
- **MUST 2, a root a colour, a row's end or JSON splits**: section 1, and
  the fail-closed refusal of anything that survives.
- **MUST 3, the check's bypasses**: `>` starts a path, a home needs no slash
  after it, placeholder-prefixed homes are taken out before matching; plants
  and mutants for each.
- **MUST 4, the mark**: a line-end comment only, never in a capture.
- **opus SHOULD 4**: every verb's file is in RunRoots. **SHOULD 5**: `:real`.
  **SHOULD 6**: the `.ansi` and the `.txt` are rewritten from one reading of
  the visible text. **SHOULD 7**: this README's counts are the head's, the CI
  wiring is stated exactly, `<tmp>` in a scrubbed file is explained.
- **gpt6 NTH**: publishing is atomic (the stage); the check's scope is stated.

## Left as it is

- capture.sh's own standard output still names the out directory and the
  binary's path (`guards:`, a snap's file name); the sweep keeps it as
  `capture.log`, outside the repository. A receipt that commits one is held by
  the check.
- Colours that change inside a root are replayed after its placeholder; a
  difference in which of the root's own characters carried them is not kept.
