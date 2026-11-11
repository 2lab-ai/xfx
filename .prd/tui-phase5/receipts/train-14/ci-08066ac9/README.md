---
title: Train 14 -- PR #93's CI Sweep regression at 08066ac9, read
type: receipt
tags: [xfx, phase5, fx-0.0.16, train, ci, sweep]
---

# PR #93's CI Sweep: `workspace-index` / `at-plan`, row 13

**It is the class WORKSPACE-INDEX-AT-PLAN traced on main, not the train's.**
Whether a side's `at-plan` lists the added folder on row 13 depends only on
the random letters of that side's own run directory, on the code head
`b3dffdad` and on the base `dcfa5146` alike; CI's frame (fx lists it, xfx
blank) comes out of both trees here. Every figure below rests on a file of
this folder.

## What CI reported (`ci.sh`, `ci.txt`)

The Sweep at `08066ac9` (code `b3dffdad`), base `dcfa5146`: one
regression, `workspace-index` `at-plan`, base normalized-identical, head
unresolved-defect, one run each; row 13 fx `  <scratch` + 46 `_` +
`>/extra/notes/`, xfx blank. The placeholder is 55 wide: CI's run
directory (RUNNER_TEMP, 24 bytes, + `/fxsw.` and eight + `/NNN` +
`/fxcap.` and six). CI's sweep runs the head (fx and head xfx) and then,
for a suspect, the base (fx and base xfx again), each capture with names
of its own (`scripts/fxcap/ci_sweep.py:364`, `:391@b3dffdad`); no retry
(`:379` retries only a pair a cause marks as varying; `ci.txt`: retries
0 s).

## Reproduced at CI's width (`run.sh`, `wrap.sh`, `run.txt`, `read.py`, `read.txt`)

A 24-byte TMPDIR directly under the system's `/private/tmp`, whose fixed
part, like RUNNER_TEMP's, holds no `l` (`tmpdir.txt`: as run, `run.sh`'s
line that prints this used a `case` inside `$( )`, which bash refused --
`run.txt`'s first three lines; the line now uses `grep`, and its
computation was run again on the same string), so the run directory is 55
wide as on CI (WORKSPACE-INDEX-AT-PLAN's method). Twelve copies of the scenario a
sweep, three sweeps a tree, head and base in turn: xfx `b3dffdad` and xfx
`dcfa5146` (sha256 in `run.txt`), each against fx v0.0.16 (sha256 the pin
before and after every sweep, `FX_AUTO_UPGRADE=0`), all with the head
tree's harness, in one hold of the suite lock. `wrap.sh` runs the tree's
own `capture.sh` unchanged and logs each run directory's name to a scratch
file; `read.py` reads those names for their letters only (none is written
here). **72 units, 144 sides:**

| tree | units | both list | **fx only (CI's frame)** | xfx only | neither |
|---|---|---|---|---|---|
| head `b3dffdad` | 36 | 2 | **9** | 7 | 18 |
| base `dcfa5146` | 36 | 14 | **2** | 2 | 18 |

- **Every one of the 144 sides lists the folder on row 13 exactly when
  its own path `<TMPDIR>/<fxsw>/<NNN>/<fxcap>/extra/notes/` holds `p`,
  `l`, `a`, `n` in order** (`read.txt`, the line after the table of
  units): with no `l` in the fixed part, exactly when the sweep's shared
  `fxsw.` name or that side's own `fxcap.` name holds an `l`.
- **CI's frame comes out of both trees**: 9 units at the head, 2 at the
  base (`read.txt`: the base's sweep 2, unit 09, rows 1-20 side by side;
  row 12, `notes/plan.md`, on both; row 13 fx's only). Every pair listed
  on one side only is unresolved; every pair listed on neither passes.
- The base's 14 "both list" are its sweep 1, whose shared `fxsw.` name
  held an `l`: all twelve units listed on both sides, and each pair is
  unresolved on the bold letters (the at-plan flip of attributes the
  train's README names), not on the row.
- The trees differ in how often each kind came up, by the names each
  sweep drew, not by their code: the rule (row listed iff the path holds
  the letters) is the same 144 of 144.

## Why (`trace.sh`, `trace.txt`)

1. **Each side's run directory has its own letters.** The sweep's root is
   `fxsw.` and eight random letters (`scripts/fxcap/sweep.py:151`), a
   unit's runs go one after the other in `<root>/<NNN>` (`:178-191`), and
   each capture makes its own `fxcap.` and six random letters
   (`scripts/fxcap/capture.sh:681@dcfa5146`, `:685@b3dffdad`): fx's and
   xfx's paths are as long, with other letters.
2. **Both products list an added root's entry by its absolute path, and
   only if the query's letters stand in it in order.** fx:
   `file_index.zig:1095-1099@481104fd` (an added root's candidate is
   absolute), `:1802-1804` (no placement in the name and no subsequence:
   not listed), `:1821-1830` (`isSubsequence`). xfx at the head (AT-RANK's
   copy of fx's placement): `src/tui/file_index.rs:1556-1563` (absolute),
   `:121-124` (its letters from that path), `:1178-1182` (`admits`),
   `:1104-1110` (no subsequence: none), `:1132-1141`. xfx at the base
   (main's ranking): `src/tui/file_index.rs:838-846` (absolute),
   `:547-575` (the name first, then the whole path), `:576-586` and
   `:612-615` (`score_range`: the letters in order, or none).
3. CI's fixed part (`/Users/<runner>/work/_temp`) has a `p` and no `l`, so
   for `@plan` the folder's row is there only when a random name brings
   an `l`: the shared one lists it on both sides, a side's own on that
   side alone. In PR #93's run, fx's own name held one and the head xfx's
   did not; in the base's run, both or neither [inferred from the rule:
   CI keeps no names].

So no unit of the train changed it: the picker's row budget, the cut of a
long path and the launch walk play no part -- row 12, the folder's
`plan.md` listed by its own name, stands on all 144 sides (`read.txt`) --
and the ranking AT-RANK changed keeps main's listing rule.

## Whose, and how big

- **Not a regression of the train.** The base passed on CI by the letters
  its run drew, as WORKSPACE-INDEX-AT-PLAN found of main's own CI runs
  (`wiap.sh`, `wiap.txt`: its README §1-3 -- the mirror frame, fx blank and
  xfx listing, in main's CI at `d2b2ca75`, and both frames on the binary
  `8f921425`, units 12 and 04).
- **Owner: WORKSPACE-INDEX-AT-PLAN** (`fix/p5-workspace-index-at-plan`
  @ `1012f418`, on main's `d2b2ca75`; squash-merged as PR #94, main
  `3570a182`, and joined into train 14 in `583cc13a`). Its fix gives both sides of a unit
  one run path (FXCAP_RUN): harness only -- `scripts/fxcap/capture.sh`
  169, `sweep.py` 62, `roots.py` 8 and the ledger 8 lines changed, tests
  334 + 104, no `src` (`wiap.txt`, `git diff --stat` against
  `dcfa5146`). Train 14 needed no change of its own: the join of main
  brought the fix, and at the joined head the pair is identical on every
  run (`../join-main/`).
