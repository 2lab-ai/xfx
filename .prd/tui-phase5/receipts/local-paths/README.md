# LOCAL-PATHS: no path of one machine in the tracked tree

xfx is public, and machine-local paths kept reaching it: on `main` at
`ccced1f5`, 156 tracked files held one machine's harness scratch root, every
image capture embedded the run directory (an OSC 8 link to
`<root>/fxcap.<run>/work/shot.png`), and four units in one night committed
scratch paths in receipt scripts and READMEs. This unit fixes the source
(`scripts/fxcap/capture.sh`), writes out what is already there, and adds a
gate that refuses it by shape.

Every claim below names the file in this directory that backs it. Base:
`ccced1f5`. Branch `fix/p5-local-paths`.

## 1. The source: capture.sh writes a run's roots as placeholders

Every file a run writes into its out directory -- `.txt`, `.ansi` (link
targets too), `.full.*`, the gateway log, every request body, `mcphttp.log`,
`.wire`, `.title`, `.cursor`, `.clip`, `.clipfile`, `.bells`, `.sockets`,
`.files`, `work-*.txt`, the `/trace` reports -- is rewritten the moment it is
written (`scrub` in `scripts/fxcap/capture.sh`, its rules in the header):

| root | placeholder |
|---|---|
| `$run/work`, the binary's working directory | `<work>` |
| `$run`, the rest of the run (its HOME is `<scratch>/home`, its TMPDIR `<scratch>/fxtmp` or `<scratch>/xfxtmp`) | `<scratch>` |
| the scenario directory, resolved | `<scenarios>` |
| the `bun` that runs fakegw | `<bun>` |
| capture.sh's own `$TMPDIR` (not a shared `/tmp`) | `<tmp>` |
| capture.sh's own `$HOME` | `<home>` |

Byte for byte, each root as given and as resolved, the longest first, only
where it ends whole. It runs before `expect` reads a snap and before the
sweep compares, and a run that fails halfway leaves only rewritten files.
`home`, `clipfile` and `reports` lose their own `sed`s for it.

**Identical on both sides.** One function and one rule for fx's run and
xfx's. In a sweep unit the two runs share one TMPDIR, so every root is as long
on both sides. The two binaries' own TMPDIRs are the exception, `fxtmp` a byte
shorter than `xfxtmp`, and are kept apart under `<scratch>`: one placeholder
for both would shorten xfx's rows by a byte more than fx's. The ledger's
`capture-tmpdir` now reads `<scratch>/fxtmp` as well as the old
`fxcap.XXXXXX/fxtmp`, and `run-directory` stays, for older captures and for a
run directory a frame wraps onto its next row.

**Widths.** The rewrite comes after the binary laid out its frame, so a path
the binary wrapped or padded keeps its layout and the stored row is shorter
where a placeholder stands. A claim that measures a path's width reads the
`roots:` line each run now prints first: every placeholder's length in bytes
on that run. `RunRoots.test_both_sides_are_written_alike_and_keep_their_widths`
pads a path to a fixed width on both sides and asserts the closing column is
where it was on each, and the `fxtmp`/`xfxtmp` offset is the one byte it was.

**Tests** (`scripts/fxcap/test_capture.py`, `harness.txt`):
- `RunRoots.test_every_file_a_run_writes_names_its_roots_by_placeholder`: a
  stub binary prints its working directory (logical and resolved), HOME,
  TMPDIR, the harness's TMPDIR, the user's home, the scenario directory, a
  coloured path, an OSC 8 link, a title, a padded path, and posts its
  directory to the fake gateway; no file of the run holds any root, and each
  is stored as its placeholder (the snap, the `.ansi`, the link target, the
  title, the wire, the request body, the workspace and HOME dumps).
- `RunRoots.test_both_sides_are_written_alike_and_keep_their_widths`: the
  sweep's way, fx then xfx in one TMPDIR: every row equal but each binary's
  own TMPDIR name, the padded columns where they were.
- `Placeholders.test_every_place_a_placeholder_is_written` now reads
  `<scenarios>`/`<bun>`/`<scratch>/home` where the snap used to hold the paths.
- RED: against `ccced1f5`'s capture.sh all three fail on a raw path; against
  `afbb866d`'s (the first version, which would not find a root right after an
  SGR sequence's `m`, i.e. a coloured path) the first fails. GREEN three runs
  in a row (`harness.txt`), and in the full suite (`test-capture.txt`).

**In real captures** (`runs-scan.txt`): of the baseline sweep's capture files,
251 of 677 held a machine root; of the after sweep's, 0 of 677, and 273 hold a
placeholder instead (an image badge's link is `file://<work>/...`).
`runs_scan.py` is the count.

## 2. The sweep: every pair keeps its verdict

`verdicts.tsv`, one row per snap pair; `verdicts.txt`, the summary. Both
sweeps ran the same selection under the suite lock, `--jobs 4`, 40x120, fx
v0.0.16 (sha256 `8e6cda2e…`):

- the image scenarios, `--only 'image*' links-image p5-lifecycle-relhome-image`
  (11; `p5-*attach*` names no scenario in `scripts/fxcap/scenarios/`);
- ten that put a path on screen: `workspace`, `workspace-index`, `skills`,
  `skills-paste`, `trace`, `trace-nowhere`, `p5-panels-status`,
  `p5-panels-status-short`, `p5-mcp-empty`, `status-version-stats`.

Before: `ccced1f5`'s harness from a clone, xfx built at `ccced1f5`. After:
`43024057`'s harness from a clone, xfx built at `43024057` (later commits
change capture.sh's comments and the ledger's line citations only).

**145 pairs before, 145 after, 145 with the same verdict, 0 changed**
(`compare_verdicts.py`): normalized-identical 102, identical 4,
approved-divergence-cascade 12, unresolved-defect 27 on both, pass 106 / 145
on both. No pair moved from normalized-identical to identical: no pair whose
only raw difference was the run directory's suffix had its banner off screen.
The capture harness's own suite: 76 tests OK before, 78 OK after
(`test-capture.txt`).

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

**By script** (`scrub.py`, `scrub-manifest.tsv`, commit `19a6b596`): 143
files. A sentence that names the root as a thing ("the run directory
`<root>/fxcap.XXXXXX` is written `<scratch>`") keeps it as the generic shape
`/private/tmp/claude-<uid>` (17); everywhere else the capture placeholders,
the longest first: `<work>` 88, `<scratch>` 112, `<tmp>` 105. A marked line is
left.

**Read again** (`verify.py`, `verify.txt`): every rewritten file with a
counterpart on the other side, fx against xfx, at `ccced1f5` and now, by
position, by `scripts/fxcap/cells.py`'s reading and by the sweep's normalized
reading: 41 pairs read the same; 6 (d1's `read-failures` Ctrl-O screens) read
the same but for one row whose only difference was the run directory's random
suffix, each side's own, which the placeholder took away (masked at the base,
the rows agree); 0 otherwise. Rewritten with nothing to pair against, so
listed as raw evidence rewritten: 21 files (`verify.txt`): unpaired request
bodies in d1, s1, w1 and w9, and gate and test logs in cancel-marker,
ctrlo-wider, transcript-receipts, tui-unsaved and width-disputed, where the
root was a test's TMPDIR in its output.

**Claims about a width, handled by name:** s1's "the same length, so no row
moves" and d1's "replacing it would move the breaks" now say what LOCAL-PATHS
changed and that the pairs read as they did; flake-status-s1's table, keyed on
which directory names hold `s1`, notes `<tmp>` holds none.

**`.prd/tui-phase4/loop.md`**: the common brief keeps it as it is; its two
lines holding the root are rewritten, because the check exempts no file.

## 4. The check

`scripts/check-no-local-paths.sh`, in CI's gate job on every row;
`scripts/test-check-no-local-paths.sh` once per OS (`script_tests`), and
`scripts/test-gate-env.sh` now requires it to reach a macOS and a Linux row.

- Shapes, never a name: `/Users/<name>/` and `/home/<name>/` where an absolute
  path starts (a line start, a character no path name holds, or the end of a
  terminal control sequence), `/tmp/claude-<digits>` with or without
  `/private`, and macOS's per-user temporary root.
- Exceptions: seven documented placeholder homes in the script (`/home/u/`,
  `/Users/u/`, `/Users/me/`, `/home/someone/`, `/Users/someone/`, `/home/you/`,
  `/home/runner/` -- GitHub's runners' and train-6's container user), taken out
  of a line before it is matched again; and a line marked `local-path-ok`, that
  line only. No directory. At this branch: 7 marked lines, 63 placeholder
  lines (`counts.txt` lists them).
- One `git grep` over the tree, one per file that holds a shape: no names on a
  command line, so no batching; about 2 s on this tree (check-no-secrets.sh:
  55 s). The guards are check-xfx-identity.sh's (every `GIT_*` dropped,
  `--work-tree=.`, the root's own `.git`, index bits and missing files
  refused), a positive control runs first, and a hit prints the file, the line
  and the shape, never the text.
- Its test: 10 cases, every shape in every kind of file (a name git quotes, a
  file named `-`, a binary, a coloured path, an OSC 8 link) and every form that
  must pass, under `GIT_DIR`, `GIT_INDEX_FILE`, `GIT_WORK_TREE`,
  `GREP_OPTIONS` and a HOME whose config runs a program. Nine mutants of the
  check, each on a throwaway clone, each failed by the test
  (`check-mutants.py`, `check-mutants.txt`).

**Counts** (`counts.txt`, by `counts.sh`): files holding each shape at
`ccced1f5` -- macOS home 4, Linux home 4, harness scratch root 156, macOS
temporary root 4, any 164; the user name 0, the home path 0. The check on
`ccced1f5`'s tree refuses 158 files (283 lines). At this branch every
remaining hit is a placeholder home or a marked line, and the check passes.

## 5. Gate

Every step of CI's gate job, and the sweep workflow's `test-sweep.sh`, at
this receipt's own commit, one at a time under the suite lock: `gate.txt`
(exit status, seconds, step). `gate.sh` is the script.

## Left as it is

- capture.sh's own standard output still names the out directory and the
  binary's path (`guards:`, a snap's file name); the sweep keeps it as
  `capture.log`, outside the repository. A receipt that commits one is held by
  the check.
- A root that a frame splits across two rows is found only in its parts, and
  the part on the second row keeps the run's random suffix (no machine root).
