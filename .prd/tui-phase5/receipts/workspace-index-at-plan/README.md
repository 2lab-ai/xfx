---
title: WORKSPACE-INDEX-AT-PLAN -- main's CI sweep regression, traced to the harness
type: receipt
tags: [xfx, tui, phase5, fxcap, sweep, ci]
---

# WORKSPACE-INDEX-AT-PLAN

main's CI sweep at d2b2ca75 (run 42844785959) reported one regression:
`workspace-index` / `at-plan` at 40x120, row 13, fx blank and xfx
`  <scratch___...>/extra/notes/`. Every claim below names the file in this
folder it rests on. `runs.sh` holds every command, by step.

## 1. No commit caused it

- The same tree passed it on CI. d2b2ca75 is the merge of 208a8dd4 into
  8f921425, and the two trees are one (`6f7fce71`). Run 42826560730 swept
  208a8dd4 and scored `at-plan` normalized-identical; run 42844785959 swept
  d2b2ca75 and scored it unresolved-defect. Runs 42762620303 (3943d667) and
  42804085569 (71d3e02b) also passed it (`ci/runs.txt`).
- The base binary does it too. With the harness CI used for both builds
  (d2b2ca75's), xfx 8f921425 at a CI-like TMPDIR gave CI's exact frame in
  unit 12 -- fx's row 13 blank, xfx's `<scratch` + 46 `_` + `>/extra/notes/`
  -- and the reverse in unit 04 (`red/8f921425-24.txt`,
  `red/8f921425-24-unit12/`, `red/8f921425-24-unit04/`).
- So there is nothing to bisect inside train 12. The per-run random name is
  in the base's harness as well (`run=$(mktemp -d "${TMPDIR:-/tmp}/fxcap.XXXXXX")`,
  `scripts/fxcap/capture.sh:546@8f921425`), whose `run_unit` docstring already
  said each side's path "differs only in capture.sh's own `fxcap.XXXXXX`"
  (`scripts/fxcap/sweep.py:180@8f921425`). The snap is W5's (12dc4a2d,
  2026-10-07). The base passed on CI by the luck of its names.

## 2. Root cause: the two runs of a unit ran in paths of one width and other letters

The happy path, and where it splits:

1. `scripts/fxcap/sweep.py` `run_unit` (`:178@d2b2ca75`) runs a unit's fx and
   xfx captures one after the other in one TMPDIR, `<tmp>/fxsw.<8>/<NNN>`.
2. `scripts/fxcap/capture.sh:681@d2b2ca75` makes each run its own
   `run=$(mktemp -d "$tmpdir/fxcap.XXXXXX")`: **six random letters per run**,
   different for fx and for xfx.
3. The scenario (`scripts/fxcap/scenarios/workspace-index.cap`) makes
   `dir extra`, types `/workspace add ../extra`, then `@plan`.
4. Both binaries index the added directory by its absolute path and match a
   query against the whole lower-cased path:
   - fx: an added root's candidate is displayed absolute
     (`file_index.zig:1098@481104fd`), paths are lower-cased (`:1178-1180`),
     a path holding the query's letters in order has a placement anywhere in
     it (`isSubsequence`, `:1804`; `alignBest(..., 0, 0, ...)`, `:1817`);
   - xfx: `score` falls back to `score_range(path, &scalars, 0, ...)`
     (`src/tui/file_index.rs:568`), case-folded (`fold`, `:527`).
5. So `.../extra/notes/` is listed for `@plan` iff its absolute path holds
   `p`, `l`, `a`, `n` in order. The runner's fixed path,
   `/Users/<runner>/work/_temp/fxsw.<8>/<NNN>/fxcap.<6>/extra/notes/`,
   has a `p` (`_temp`), `a` and `n` (`fxcap`, `extra`, `notes`), and **no `l`**:
   the row is there only if a random name holds an `l` (or `L`). One in
   fxsw's (shared) lists it on both sides; one in a side's own `fxcap.XXXXXX`
   lists it on that side only. The CI log names RUNNER_TEMP
   (`ci/runner-temp.txt`); its 24 bytes make the run directory 55, the
   width of CI's `<scratch...>`.
6. roots.py writes both sides' run directories as one placeholder of the
   same width (LOCAL-PATHS), so no capture showed the two paths differ.

**The two things that disagree:** fx's run directory
`<tmp>/fxsw.<8>/<NNN>/fxcap.<6 letters>` and xfx's
`<tmp>/fxsw.<8>/<NNN>/fxcap.<6 other letters>` -- same bytes long, other
letters. `tests/red.txt` shows it without the placeholder: a checksum of each
side's working directory, `sum=988592721 63` against `sum=4276430234 63`.

The same cause, a second way: where both sides list the row, which letters
are bold depends on the names too (fx's best placement, xfx's leftmost one;
AT-RANK's ledger cause `at-plan-run-directory` on feat/p5-at-rank names it).
At the gate's own TMPDIR, whose fixed part holds `p`, `l`, `a`
(`private`, `claude`), the row is always there and only the bold letters
flip (`red/d2b2ca75-25.txt`: 9 of 12 differ).

## 3. RED, at d2b2ca75's harness

Sweeps of twelve copies of the scenario (`runs.sh red`, `runs.sh copies`):

| leg | TMPDIR | at-plan differing | how | file |
|---|---|---|---|---|
| xfx d2b2ca75 | 24 B, no `l` | 12 / 12 | row on both, bold differs: the `l` was in fxsw's shared name (xfx's second bold cell is byte 34 of the run path, inside `fxsw.`'s eight: `red/d2b2ca75-24-unit01/`) | `red/d2b2ca75-24.txt` |
| xfx 8f921425 | 24 B, no `l` | 2 / 12 | row on one side only: units 04 (fx), 12 (xfx, CI's frame) | `red/8f921425-24.txt` |
| xfx d2b2ca75 | 49 B, no `l` | 5 / 12 | row on one side only: fx 01, 03, 09; xfx 04, 07 | `red/d2b2ca75-49.txt` |
| xfx d2b2ca75 | 16 B, no `l` | 5 / 12 | row on both, bold differs | `red/d2b2ca75-16.txt` |
| xfx d2b2ca75 | 25 B, gate's | 9 / 12 | row on both, bold differs | `red/d2b2ca75-25.txt` |

`at-all` (an empty query: the index's order, no match) is identical in all
60 units. The harness tests, new in this unit, fail at d2b2ca75: 3 tests, 7
failures (`tests/red.txt`).

## 4. The fix (2a721846; outside tests +75/-22: capture.sh, sweep.py, the ledger)

- `scripts/fxcap/capture.sh` `make_run` (`:431-459`, used at `:711`): the run
  directory is `fxcap.` and six random digits, never one that exists, owner
  only; or the name `FXCAP_RUN` gives, refused (exit 2, nothing started)
  unless it is `fxcap.` and six digits and not already there.
- `scripts/fxcap/sweep.py`: `run_unit` gives a unit's two runs one
  `FXCAP_RUN` (`:191`, `:173`), so fx and xfx run in the same path, byte for
  byte; the sweep's root is `fxsw.` and eight digits (`digit_dir`, `:299`).
- So no random letter is left in any path a run sees: a pair compares two
  binaries on one input, and a base sweep and a head sweep read the same
  letters (those of `--tmp`).
- The ledger: the four capture.sh references re-pointed (+30), and
  `run-directory`'s why. Its rule still reads older captures (`--rescore`).
- Not copied from fx: this is the harness, not the product.

## 5. GREEN, at 2a721846's harness

The same legs (`runs.sh green`):

| leg | TMPDIR | at-plan differing | `notes/` row | file |
|---|---|---|---|---|
| xfx 2a721846 | 24 B | 0 / 12 | on neither side | `green/2a721846-24.txt` |
| xfx 8f921425 | 24 B | 0 / 12 | on neither side | `green/8f921425-24.txt` |
| xfx 2a721846 | 49 B | 0 / 12 | on neither side | `green/2a721846-49.txt` |
| xfx 2a721846 | 16 B | 0 / 12 | on neither side | `green/2a721846-16.txt` |
| xfx 2a721846 | 25 B, gate's | 0 / 12 | on both, the same bold letters | `green/2a721846-25.txt` |

Each leg 24 / 24 pairs. The roots lines keep CI's widths (`<scratch>=55` at
24 bytes). A pair for each kind of path is in `green/2a721846-24-unit01/` and
`green/2a721846-25-unit01/`; row 1 of each xfx capture names 2a721846. The
new tests pass (`tests/green.txt`: 3 + 1).

Where the row is listed, its bold letters are now a function of `--tmp`, not
of the run: at the gate's TMPDIR fx's placement and xfx's leftmost one fall
on the same `p`, `l`, `a`, `n`. A TMPDIR whose letters give `@plan` other
placements would still show AT-RANK's difference -- on every run alike, as
the parity defect it is, under the ledger's `at-ranking` cause.

## 6. Look-ahead

- **`at-all`:** an empty query lists the index in its order and matches
  nothing, so no letter of a path decides a row: identical in all 60 RED and
  60 GREEN units.
- **Other scenarios:** only `workspace-index` types an `@` query over an
  added directory; `workspace.cap` adds one but types no `@`. The other `@`
  scenarios search the primary workspace, which both list by relative path
  (`file_index.zig:1098@481104fd`). A sweep of the `workspace*`,
  `filepicker*`, `p5-input-at*` and `p5-input-row-limit*` families at 24
  bytes, the fixed harness, xfx 2a721846 against 8f921425: 73 pairs each, 50
  pass each, **no pair crosses pass/fail** (`look/head-vs-base-24.txt`; each
  build's pairs in `look/2a721846-24.txt`, `look/8f921425-24.txt`). The 23
  failures are the ledger's known rows (folders, ranking, resize, menus).
- **`p5-input-row-limit` `at` / `at-pgdn` on PR #92's sweep (run
  42863297405)** -- xfx `  indexing files...` at row 5, 1.5 s after `@f-` --
  is **another cause**, not this one: no run-path letter is read there.
  - fx starts its file index at launch (`app.startFileIndex()`,
    `src/core/app/app_entry_runtime.zig:334@481104fd`).
  - xfx reads nothing until a picker opens (`Files::new`,
    `src/tui/file_index.rs:923-924`; `begin_episode` at the first `@`,
    `src/tui/shell.rs:9172-9174`).
  - Its walk asks git first (`git ls-files`, `discover`,
    `src/tui/file_index.rs:216-229`) and only then walks.
  - So xfx's index has the 1.5 s after `@f-`, where fx's has had the whole
    7 s since launch. [hypothesis] Under the runner's load (CI's head sweep
    read 11.4), that is too short. Here the pair passed on both builds, and
    `git ls-files` outside a repository took 0.01 s at load 12.
  - The copy of fx would start the index at launch. That is a product change
    of its own, not this unit's.

## 7. Mutants

8 / 8 killed, on a `git clone --no-local` at 2a721846 (`mutants.sh`,
`mutants.txt`): FXCAP_RUN ignored, letters back in the run name, any name
shape accepted, an existing directory reused, the run directory not
owner-only, a name per run instead of per unit, no FXCAP_RUN from the sweep,
mkdtemp's letters back in the sweep's root.

## 8. Gate

At 2a721846, on a `git clone --no-local`, under the suite lock (`gates.sh`,
train 12's): 21 / 22 (`gate/gates.txt`). Step 04, `cargo test`, failed two
`tui::worker` stop-window tests at load 11.6 (4742 passed, 2 failed). Each
then passed three runs out of three alone (`gate/step-04-reruns.txt`). This
unit changes no `src/` file; the two are the catalog-wait clock tests.

## 9. Open

- AT-RANK (feat/p5-at-rank, not merged) adds a ledger cause
  `at-plan-run-directory` owned by SWEEP-TEMPNAME-STYLE. This unit is that
  queue item's fix: when AT-RANK joins, its cause and its owner text need
  re-anchoring or deleting.
- `p5-input-row-limit`'s lazy index start (section 6) wants a unit of its
  own.
