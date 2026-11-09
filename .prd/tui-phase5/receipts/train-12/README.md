---
title: 0.0.16 delta -- P5 train 12 receipt (GATE-ENV-MAINTLOCK-F, SWEEP-WALLCLOCK-F, LOCAL-PATHS, INPUT-ROW-LIMIT into one branch)
type: receipt
tags: [xfx, phase5, fx-0.0.16, train]
---

# P5 train 12: four closed units, one branch

Branch `train/p5-1110a`, from `origin/main` `df46777e` (#89, train 11), as
the brief set it; `git fetch origin main` found it unmoved. Each unit was
closed by its own two-engine review and the dispatcher's gate; this train
combines them. The program goal (the user's words): «최신 fx랑 비교해서 ui쪽으로
모든 기능 구현하고 기본적으로 최신 fx ui를 그대로 구현해줘» (fx v0.0.16, pin
`481104fd`).

Oracle: fx v0.0.16 (`481104fd`), executable sha256
`8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked
by every sweep before and after it ran, `FX_AUTO_UPGRADE=0`
(`sweep-runs.txt`).

Model: train 11's receipt (`../train-11/`): its scripts are copied here, each
saying in its first lines what changed.

## Units and merges

| # | unit | branch @ tip | merge | base | conflicts |
|---|---|---|---|---|---|
| 1 | GATE-ENV-MAINTLOCK-F | `fix/p5-gate-env-maintlock` @ `a1cbdcaf` | `d43c8c92` | `32afd307` | none (forecast none) |
| 2 | SWEEP-WALLCLOCK-F | `fix/p5-sweep-wallclock` @ `4bc9ab13` | `2c640bac` | `3be49c15` | none (forecast none) |
| 3 | LOCAL-PATHS | `fix/p5-local-paths` @ `5609108f` | `55d993a7` | `ccced1f5` | `scripts/fxcap/parity-ledger.toml`, three blocks (forecast) |
| 4 | INPUT-ROW-LIMIT | `fix/p5-input-row-limit` @ `3d3d56c8` | `12b56ab8` | `32afd307` | `docs/parity.md`, two blocks, and the ledger, six (forecast); **one semantic conflict, the tree did not build** (not forecast) |

Every base is git's merge base. The units of train 13 (QUESTION-WORD-LEFT,
AT-FOLDERS, HISTORY-NOTICES-COPY-FX, TEST-ENV-ISOLATION, QUESTION-RESUME)
are not in this train. Every merge tree passed `cargo check --all-targets`
with and without `fault-injection`, `cargo fmt --check` and clippy (both)
before it was committed (`check-merges.txt`); merge 4's only after its fix.
Every resolution is a script's (`blocks.py`, `rowsmerge.py`, `semantic.py`),
redone by `reproduce.sh` in a clone to each merge commit's tree, all four
"no difference" (`resolutions.txt`), and checked from both ends -- ours ->
merged is exactly theirs' edits, theirs -> merged exactly ours'.

After the merges, in order:

| commit | what |
|---|---|
| `03fb5436` | the 13 references the merges moved, re-pointed; numbers only -- **the code head: the gate, every sweep and every job ran here** |
| `e1122281` | the merges' scripts and checks, the scripts the gate, sweeps and jobs run |
| the next ones | the gate's, the jobs' and the sweeps' files; this README; `final-scan.txt` last |

**No code after the code head**: `gates-files.txt` (`gates-files.sh`, in a
clone at the receipt's final form) says what `git diff --stat 03fb5436 HEAD`
holds outside this folder.

### Merge 3's conflict (forecast): the ledger, three blocks

The ledger's three diff3 blocks, each by train 11's `blocks.py`, one rule a
block (`remerge-3.txt`):
- **`run-directory`** and **`capture-tmpdir`** (`rewrite`): main had moved
  only their `capture.sh` numbers (`:538` -> `:546`, `:317-319` -> `:325-327`;
  the blocks in `remerge-3.txt`), in text LOCAL-PATHS rewrote: LOCAL-PATHS's
  entries.
- **`tmux-dead-pane`'s `source`** (`numbers`): both sides moved its numbers
  alone (main `capture.sh:973` -> `:987`, its line written by train 10's
  re-point `589248ac`, `repoint-plan.txt`; LOCAL-PATHS `d1/README.md:145` ->
  `:151` and `capture.sh:973` -> `:1123`): ours kept; the re-point commit
  moves both to the head's lines.

`capture.sh`, `test_capture.py`, `test-gate-env.sh`, `approval.rs` and
`shell.rs`, which both sides changed, merged clean; `filecheck.py` reads each
true both ways, and the ledger true both ways with its numbers masked
(`resolutions.txt`). The nearest two sides' hunks in a file both changed:
`capture.sh` 11 base lines apart, `test_capture.py` 27 (`overlap.txt`).

### Merge 4's conflicts (forecast) and one semantic conflict (not forecast)

- **The ledger, six blocks** (`numbers`): main's `src/tui/shell.rs:40879`
  and the unit's `:39279`, the same line (`"  tools: {} advertised ({},"`)
  moved by each side's tests: ours kept, then re-pointed to `:41113`.
- **`docs/parity.md`, two blocks** (`rowsmerge.py`: train 10's `rowmerge.py`
  for a block of several rows, each row token by token against the base's):
  the first holds two rows, `/model` (main's 46 edits, the unit's none) and
  `/effort` (the unit's 6, main's none); the second the full-screen TUI row,
  main's 88 edits and the unit's 8, none overlapping or meeting.
  `rowcheck.py` true both ways for all three (`resolutions.txt`).
- **The tree did not build** (`check-merges.txt`: both cargo checks and both
  clippy runs, one error each, E0308 at `src/tui/shell.rs:36273:18`). The unit's
  `at_41_rows_a_page_of_the_model_menu_is_19_models`, written on
  `32afd307`, gives `UiEvent::CatalogLoaded` an `Instant` due;
  CATALOG-REFRESH (train 11, on main) made every such due a `CatalogDue`.
  git's line merge saw nothing. Fixed in the merge commit by `semantic.py`,
  spelt as every other test of the module spells a load's due
  (`CatalogDue::at(catalog_now() + crate::provider::model::REFRESH_INTERVAL)`).
  With the fix taken out (`semantic.py --reverse`, in a clone) `shell.rs` is
  true both ways; `layout.rs` true both ways. The nearest hunks of the two
  sides: `shell.rs` 1 base line apart (the unit's `effort_shell_at` split
  above main's `CatalogDue` line in `effort_shell`; merged clean, main's due
  kept [read from the code at `03fb5436`]), `layout.rs` 4 (`overlap.txt`).

## The token check (`token_check.py`, `token-check.txt`, `token-self-test.txt`, `token_control.py`, `ledger_numbers.py`)

Train 7's `token_check.py`, byte for byte (`cmp` against `../train-7/`, its
first line), over the four merges: **merges 1, 2 and 4 OK; merge 3, one
entry handed back "read by hand"** [measured]:
- **Merge 3**: `tmux-dead-pane`, "both sides changed it differently". Both
  changed its numbers alone, but one of them names `.prd/tui-phase4/
  receipts/d1/README.md`, which the script's reference pattern
  (`src|scripts|tests|docs`) does not mask. `ledger_numbers.py` masks every
  `:N`: base, ours, theirs, the merge and the head **the same entry**, the
  numbers `:145 :32 :987` (the merge, ours') and `:151 :32 :1147` (the head,
  re-pointed). The two entries LOCAL-PATHS rewrote read "changed by theirs,
  its numbers alone by ours; theirs' entry".
- **Merge 4**: the full-screen TUI row both sides', 2361 tokens added, 35
  deleted, 43 clauses whole and contiguous, each side's tokens in its order;
  `/model` ours', `/effort` and the `@` file picker row theirs'; six ledger
  entries "both sides changed its path:line numbers alone".

**The negative control** (`--self-test`): **9 of 9, 5 of 5, 18 of 18 and 16
of 16 found** [measured]. Train 11's `token_control.py` stops on merge 2
(no row a side changed there: `KeyError`, `token-control-train11.txt`); its
train 12 form plants the two token cases
on every merged row **both** sides changed -- merge 4's full-screen TUI row,
the one row the self-test's own plant cannot land on: **4 of 4 found**.

## The references (`repoint.py`, `repointed.py`, `refs-*-pre.txt`, `refs-*-head.txt`)

Train 7's `refs.py`, `refs_all.py`, `refs_short.py`, train 9's
`refs_bare.py`, and `refs_prd.py` (new: `refs.py` over the `.prd/`
references of the ledger, `docs/parity.md` and `capfile.py`, which LOCAL-PATHS
rewrote and refs.py's pattern does not read), five sides: main `df46777e`
and the four tips. At the merges' head `12b56ab8`, before any re-point
(`refs-*-pre.txt`), outside the historical plans
(`docs/superpowers/plans/`) they flag 13 references, all in the ledger:
LOCAL-PATHS's five `capture.sh` numbers (moved by the other side's lines
above them), main's `capture.sh:987` and `d1/README.md:145` (merge 3), and
main's `shell.rs:40879` six times (merge 4). The `.prd/` references cited
from anywhere else (13, 10 outside the plans) name six files that did not
change between main and the head (`prd_refs.sh`, `prd-refs.txt`).

**13 re-pointed in `03fb5436`, numbers only** (9 lines out, 9 in, each the
same once its numbers are masked), each in the frame of the side that wrote
its citing line (`git blame` at `12b56ab8`; `repoint-plan.txt`): LOCAL-PATHS
for its five, main for the rest.

**After the last merge**, at `03fb5436`:
- **`repointed.py`: 13 of 13 the same code** [measured] (`repointed.txt`).
- **`refs.py` 0 moved; `refs_prd.py` 0 moved.**
- **`refs_bare.py`**: 73 bare continuations, 58 flagged, **every one in three
  historical plans** (17, 15 and 26, as at train 11); `refs_all.py` 15 and
  `refs_short.py` 65, all in the plans. Each is "moved" only on a unit side
  whose base is older than main; on main's side each reads `=`.

## Join checks

Join check 5 (docs/parity.md, the ledger and `capfile.py`: the token check,
its negative control, the re-points, `repointed.py` and `refs_bare.py` after
the last merge) is the two sections above.

No test was added: for each meeting the units' own tests already cover it,
named below, and each runs at the head (the gate's suites, or by name in
`jobs-named.txt`).

### 1. LOCAL-PATHS x SWEEP-WALLCLOCK-F: the release channel under the rewrite

**The path** [read from the code, `03fb5436`]: a `modelsdown` line sets
`modelsdown=$run/models-up` (`scripts/fxcap/capture.sh:732`), which reaches
the fake gateway as `FAKEGW_MODELS_UP` (`:836`); `fakegw.ts` answers 503 on
every catalog ask until that file exists (`fakegw.ts:444-453`); the
`modelsup` verb writes it, `echo up` to a `.tmp` and a rename, into `$run`
(`capture.sh:1032`). LOCAL-PATHS's rewrite is `publish` (`:455-469`):
every file a run writes **to the out directory** is staged under
`$run/stage`, rewritten by `roots.py`, then renamed into place; the files
left there are read again at the end (`:498`). The release file is not one
of them: it is written into `$run`, read only by the gateway, and holds
`up`. The gateway's log, which the channel's 503s land in
(`GET /coding-agent/v1/models -> 503 (down)`, `fakegw.ts:451`), is published
through it by a scenario's `gwlog` line (`capture.sh:1100-1105`). So the two
meet only in that log, whose lines name the catalog's URL path, no file.

**Both units' `test_capture.py` cases in the merged file** (`jobs-named.txt`:
the harness suite with `-v` at the head, 107 tests OK, and `harness_names.sh`
over it): SWEEP-WALLCLOCK-F's `test_modelsdown_until_modelsup` (end to end
over the stub binary, through the merged capture.sh: two asks 503, then
`modelsup`, one 200), `test_modelsdown_answers_503_until_its_file_is_in_place`
and its refusals in `test_each_verbs_arguments` -- **3 of 3 ok**; LOCAL-PATHS's
26 new cases (`RootsRewrite`, `RunRoots`, the refusals) -- **26 of 26 ok**.
The first is the meeting itself: its run goes through LOCAL-PATHS's
`publish` and end-of-run `--check`, and the release still ends the outage.

**The eight scenarios at the head** (the head's sweep, `sweep-head.md`;
`compare-unit-sweep-wallclock.txt` against the unit's five GREEN sweeps and
round 2's five, as its receipt commits them): **25 of 26 pairs pass, as on
the unit's** -- every one the same verdict, `model-parked` `menu-loading`
unresolved under `activity-tokens-before-request` on both, as the unit
registered it; the 26th pair, `model-column-reask` `col-held`, is round 2's.
Their catalog asks, from the gateway logs the rewrite published
(`asks-head.txt`): on both binaries, `552` in both column scenarios, `52` in
`model-reask` and `model-parked`, `55` in `model-floor`, `5.` in
`p5-stop-in-catalog-wait` -- each the same as in all five of the unit's
GREEN runs (the file's second half). So the release still ends the outage
at the scenario line, under the rewrite.

**Nothing the channel writes leaves a path** (`capture_scan.py`,
`capture-scan.txt`, every file capture.sh wrote in the head's sweep): the
eight scenarios' files, 134 of them, **0** holding the brief's shapes, the
sweep's TMPDIR, a run directory's name or the account's name; the three
`p5-input-row-limit*` too (78 files, 0). The release file itself is never in
the out directory. Over all 380 scenarios, 62 files hold one, **all in three
MCP scenarios** (below, "Found on the way"): not this channel's.

### 2. LOCAL-PATHS x GATE-ENV-MAINTLOCK-F: test-gate-env.sh

**The merged script** [read from the code, `03fb5436`]: GATE-ENV-MAINTLOCK-F's
guard on `sg` (`scripts/test-gate-env.sh:103-106`, `-c maintenance.auto=false
-c gc.auto=0`), its two cases (`decoy-setup-maintenance` `:468`,
`decoy-setup-starts-nothing` `:469-521`), and LOCAL-PATHS's one change, the
CI coverage check's script list with `test-check-no-local-paths`
(`:824`, `:830`). `filecheck.py`: ours -> merged is exactly LOCAL-PATHS's 2
hunks, theirs -> merged exactly main's 7 (`resolutions.txt`), 300 base lines
apart (`overlap.txt`).

**At the head** (the gate's `test-gate-env` step, `gate-counts.txt`):
`ok (64 cases)` -- the base's 62 and GATE-ENV-MAINTLOCK-F's two, both `ok`
(`decoy-setup-maintenance`, `decoy-setup-starts-nothing`, `gates.txt`) -- and
its CI-coverage case `ci-script-tests-per-os` names
`test-check-no-local-paths` among the script tests on macOS and Linux rows,
LOCAL-PATHS's change.

**The maintenance fix reverted** (`jobs.sh mutant`, `jobs-mutant.txt`): the
unit's own m1 (`runs-r3/m1-no-guard.diff`: `sg` without `-c
maintenance.auto=false -c gc.auto=0`) applied to the merged script in a `git
clone --no-local` at `03fb5436`, the whole `test-gate-env.sh` under `timeout
-k 10 900` and the suite lock, the clone restored and clean after: **exit 1,
`3 problem(s), 62 case(s) passed`** in 218 s -- `decoy-setup-maintenance`
(the decoy's `objects/maintenance.lock` in one sum and not the other), and
the trace check `decoy-setup-starts-nothing` twice (`run_command: git
maintenance run --auto --quiet --detach` and `git gc --auto ...` in the
trace; the `pre-auto-gc` marker). The unit's round 3 had the same three on
its own branch (`../gate-env-maintlock/runs-r3/summary.txt`: `m1-no-guard`,
3 problems). So the trace check still fails without the fix, at the head.

### 3. LOCAL-PATHS's gate over the other units' files

- **The check and its test at the head** (the gate's steps 9 and 10,
  `gate-counts.txt`): `check-no-local-paths: ok (18674 tracked file(s)
  scanned; 8 marked fixture line(s), 80 placeholder line(s))` and
  `test-check-no-local-paths: ok (10 cases)`, its `shapes` case 28 plants.
  Every file passes; nothing was changed for it. The marked and placeholder
  lines are LOCAL-PATHS's own (its `gate.txt` gives the same 8 and 80 at its
  head).
- **The other units' files, read one by one** (`unit_paths.sh`,
  `unit-paths.txt`): GATE-ENV-MAINTLOCK-F's 83 receipt files,
  SWEEP-WALLCLOCK-F's 98 and INPUT-ROW-LIMIT's 137 hold none of the brief's
  shapes and no run directory's name; GATE-ENV-MAINTLOCK-F's 10 and
  SWEEP-WALLCLOCK-F's 1 hold a placeholder written by hand. Their code and
  harness files are in the check's whole-tree pass above.
- **CI's new steps run at the head** (`ci_rows.rb`, `ci-rows.txt`, read from
  ci.yml at `03fb5436`): `No path of one machine in tracked files`
  (`./scripts/check-no-local-paths.sh`, `if: matrix.part != 'release'`) runs
  on four of the gate job's five rows -- both Linux rows, arm64 macOS and the
  x86_64 macOS `checks` half -- and skips only the `release` half;
  `The local-path check refuses what it exists for`
  (`./scripts/test-check-no-local-paths.sh`, `if: matrix.script_tests`) runs
  on one Linux and one macOS row. At main `df46777e` no step runs either.
  test-gate-env.sh's own CI-coverage check (LOCAL-PATHS's line `:824-830`)
  says the same of the script test at the head (`ci-script-tests-per-os`,
  above).

### 4. INPUT-ROW-LIMIT x LOCAL-PATHS

**In `src/tui/shell.rs`** (`shell_hunks.sh`, `shell-hunks.txt`): both units'
hunks there are in the tests module -- LOCAL-PATHS's five same-length
stand-ins for a scratch root in test strings, INPUT-ROW-LIMIT's 233 lines of
tests and `effort_shell_at`; INPUT-ROW-LIMIT's one code change is
`layout::input_row_limit` (`src/tui/layout.rs`), which LOCAL-PATHS does not
touch. LOCAL-PATHS's hunks sit at its base's lines 20978-24928,
INPUT-ROW-LIMIT's at its base's 35054 and 42230: thousands of lines apart.
In merge 4 the nearest pair of hunks, 1 base line apart, is the unit's
`effort_shell_at` and main's (CATALOG-REFRESH's) `CatalogDue` line, read
above (`shell-hunks.txt`, `overlap.txt`).

**The pairs at the head, as on its own sweep.** INPUT-ROW-LIMIT swept 18
scenarios at **41x120** (an even height cannot show its change: its README,
"Why 41 and not 40"); the head swept the same 18 at 41x120 with LOCAL-PATHS's
capture.sh (`irl41-selection.txt`, `sweep-irl41.md`), each pair rewritten
before it was read:
- **pass 67 / 120 on both, every one of the 18 scenarios the same count**
  (`unit_compare.py` against the unit's committed `sweep-41x120-head.md`,
  `compare-unit-input-row-limit.txt`: 0 passed on the unit and not on the
  head);
- its `p5-input-row-limit*` pairs (`irl-pairs.txt`): `at`, `at-pgdn`,
  `grown`, `grown-pgup`, `grown-pgup2` and the four `-menus` pairs
  normalized-identical -- the six its README names as moved by the fix among
  them -- and `help`, `help-pgdn`, `slash`, `slash-pgdn` unresolved under the
  ledger's existing causes, as on its sweep (`slash` 1 row and `slash-pgdn` 7,
  as its `pairs.txt` has them);
- **at 8x120**, where its tracked `-short-8x120-` captures were taken: the
  head's two captures of `p5-input-row-limit-short` (sweep.sh ran both, then
  stopped on its own inventory, which an 8-row screen cannot give: `sweep:
  a help snap shows no menu`, `sweep-runs.txt`), through `cells.py` as the
  unit's `pairs.py` reads them (`short8.py`): **4 of 4 with no differing
  row, both carets `6,2` on each side**, as its `pairs.txt` has the head.

**Its tracked captures pass the path check**: the gate's
`check-no-local-paths` over the whole tree, and `unit-paths.txt` for its
folder -- 137 files, none holding the brief's pattern, a placeholder or a
run directory's name.

### 6. Network hermeticity (`net.py`, `jobs-net.txt`, `connect.log`)

Train 9's to 11's method (FLAKE-CATALOG-EVENT's `connect_logger.py`, a
loopback logger that answers 403 and forwards nothing; the four proxy
variables at it, `NO_PROXY`, `no_proxy`, `ALL_PROXY`, `all_proxy` unset), a
clone at `03fb5436`, one hold of the lock:

| run | result | CONNECTs logged during it |
|---|---|---|
| `cargo test --locked --lib --no-fail-fast` | 4730 passed, 0 failed, 7 ignored | **2**, both `CONNECT w.example:443` |
| `cargo test --locked --test tui --no-fail-fast` | 188 passed, 0 failed | **0** |

**No CONNECT to the Gateway host** [measured]; the two `w.example` lines are
the known MCP probe (trains 6 to 11). No unit of this train adds a
network path: GATE-ENV-MAINTLOCK-F and SWEEP-WALLCLOCK-F change no `src/`,
LOCAL-PATHS's `src/` edits are same-length stand-ins for a path in tests and
a marked fixture (its README section 3; `shell-hunks.txt` for `shell.rs`),
INPUT-ROW-LIMIT's is the row limit.

### 7. Commits partway along the branch (`midbranch_scan.py`, `final-scan.txt`)

The brief's check over **every commit of `df46777e..HEAD`** -- the units' 86
and this train's -- each as `git show --format=` prints it, every hit
classified by `midbranch_scan.py` (the account's name and uid read at run
time, never written; no hit printed but its kind and file):

- **No real user name or credential**: kind NAME 0 and CREDENTIAL 0 in every
  commit and in the range; no commit message holds the account's name. The
  train goes on.
- **15 commits hold a hit, every one LOCAL-PATHS's** (33 lines, the brief's
  count, 43 matches); none of the other three units' commits and none of
  this train's.
- **Placeholders (31 matches)**: homes named `u`, `me`, `example`,
  `someone`, `z` (the check's documented placeholder homes, its tests and
  README, `.prd/tui-phase4/loop.md`'s list), a harness root with uid `0`
  (`red3.sh`), and `claud[e]-3/` and `claud[e]-2/` inside a name or after a
  `/` (the model paths the check's tests pass on purpose).
- **Scratch-root shapes (12 matches)**, every one of this machine's uid or
  the macOS per-user temp root:
  - **10 on lines marked `local-path-ok`, 8 of them at the head**: the
    check's own header naming the shapes it refuses (`/var/folder[s]/`,
    twice in `43024057` and twice again where `506febef` rewrote those
    lines), the redactor's inputs in `test_sweep.py` (`43024057`, 4), a
    fixture root never read in `test_capture.py` (`56e51cb0`), and
    upstream's bytes in the skill-catalog fixture (`41023d5f`,
    `src/skills/catalog.rs`; LOCAL-PATHS's README section 3).
  - **2 on unmarked lines, both gone by the unit's tip**: `c6e12e82`'s
    `roots-mutants.txt` header (the TMPDIR of a mutant run -- the brief's
    named one) and `2e90c292`'s README quoting a plant with its uid (fixed in
    `5743e3c7`, LOCAL-PATHS's section 11). The repo keeps both commits; they
    publish this machine's harness root and uid, no name.
- **The range** (`git diff df46777e..HEAD`'s added lines): 21 lines, the
  brief's count -- all LOCAL-PATHS's: 21 placeholder matches and 8
  scratch-root shapes, the 8 above, each on a `local-path-ok` line. This train's
  own commits add 0 (`final-scan.txt` part 1: every merge's combined diff,
  the re-point and every receipt commit 0); merge 3 read as everything it
  brings holds LOCAL-PATHS's 21.

## Gate (`gates.sh`, `gates.txt`, `gate-counts.txt`)

`gates.sh` (trains 5's to 11's, with LOCAL-PATHS's two CI steps added where
ci.yml runs them, after `check-no-secrets`): every step of
`.github/workflows/ci.yml`'s gate job and `sweep.yml`'s two harness tests,
in a `git clone --no-local` at `03fb5436`, one after another, **the whole
script inside one hold of the suite lock**, each step's exit taken from `$?`
on the line right after its command, the tree clean before and after.
TMPDIR: the dispatcher's own, the harness scratch root itself (23 bytes, as
LOCAL-PATHS's README names "the dispatcher's"), not this session's scratch
directory (the brief: fourteen tests on main fail under a long or spaced
TMPDIR, queued as TMPDIR-ROBUST; *(not measured here)*).
**22 steps, 0 failed, none run again** [measured]:

| step | at `03fb5436` |
|---|---|
| `cargo fmt --check`; clippy; clippy `--features fault-injection` | ok; ok; ok |
| `cargo test --locked --no-fail-fast` | **5654 passed, 0 failed, 7 ignored** |
| the same `--features fault-injection` | **5691 passed, 0 failed, 7 ignored** |
| `check-xfx-identity` / `check-no-stubs` / `check-no-secrets` | ok / ok / ok |
| **`check-no-local-paths`** / **`test-check-no-local-paths`** (LOCAL-PATHS's, new) | ok (18674 tracked files; 8 marked fixture lines, 80 placeholder lines) / ok (10 cases, 28 plants) |
| `check-preview-contract` / `check-tui-preflight-cost` / `check-usage-clock` | ok / ok (200x300 full replay 1.18 ms; budget 32) / ok |
| `test-check-no-secrets` / `test-gate-env` / `test-preview-contract` | ok / ok (64 cases) / ok |
| `fxcap/test-sweep.sh` / `fxcap/test-capture.sh` | 72 + 27 OK / 107 OK |
| release build; `fault-injection` release build | ok; ok |
| `smoke.sh` / `smoke-tui.sh` | 47 checks, 0 failures / 37 scenarios + the oracle, 1167 checks, 0 failures |

Load at the steps' starts 6.9-34.5. **Every side's tests are in**
(`test-counts.txt`, train 7's count: lines holding `#[test]` under `src` and
`tests`, `def test_` under `scripts/fxcap`): main 5557; INPUT-ROW-LIMIT +10
(5462 against its base's 5452), the three others +0 in Rust; **5567 at the
head**. `cargo test` 5654 against train 11's 5644 at main's code: **+10**,
with fault injection 5691 against 5681, the same +10. The harness's
`def test_`: main 176, SWEEP-WALLCLOCK-F +2 (178 / 176), LOCAL-PATHS +28
(201 / 173), **206 at the head**; `test-capture.sh` ran 107 (train 11's 79,
SWEEP-WALLCLOCK-F's 2, LOCAL-PATHS's 26) and `test-sweep.sh` 72 (70 and
LOCAL-PATHS's 2).

## Sweeps

**Builds** (debug, as trains 4 to 11), each in its own `git clone --no-local`,
each swept with its own tree's harness, scenarios and ledger (`sweep.sh`,
train 11's with **every scenario** and a size). fx: the v0.0.16 binary,
sha256 `8e6cda2e...` before and after each sweep, `FX_AUTO_UPGRADE=0`;
40x120, `--jobs 2`; each sweep inside one hold of the suite lock
(`sweep-runs.txt`). Main's sweep was rescored with the head's ledger
(`sweep.sh --rescore` on a copy): no verdict moved, 1297 before and after.
Every xfx snap that shows the banner shows the build swept: `v0.1.0-03fb543`
on all 1619 of the head's, `v0.1.0-df46777` on all 1601 of main's
(`sweep-runs.txt`).

| sweep | scenarios | pairs | pass | failed | timeout |
|---|---|---|---|---|---|
| head `03fb5436` (`sweep-head.md`) | 380 | 1872 | **1310** (70.0%) | 2 | 0 |
| main `df46777e` (`sweep-main.md`, rescored) | 377 | 1854 | 1297 (70.0%) | 2 | 0 |
| head at 41x120, INPUT-ROW-LIMIT's 18 (`sweep-irl41.md`) | 18 | 120 | 67 | 0 | 0 |

The head's three scenarios more are INPUT-ROW-LIMIT's; its 18 pairs more are
those three's 17 and SWEEP-WALLCLOCK-F's `model-column-reask` `col-held` --
12 pass, and the 6 unresolved are `help`, `help-pgdn`, `slash`,
`slash-pgdn` and the short ones' `slash`, `slash-pgdn`, as on the unit's own
sweep (join check 4). The 2 failed pairs on both are `p5-panels-badsettings`:
fx ends before its first frame on both (below).

### Against main (`compare-main.txt`, `screen-moves-main.txt`, `screen-classes.txt`, `gained-rows.txt`)

**1854 shared pairs: 1297 -> 1298; 1 down, 2 up** [measured], and six moves
between identical and normalized-identical, both passing
(`p5-submit-preview-full` x4 and `settings-collapse-open` `after-esc` to
identical, `p5-transcript-submit` x2 to normalized-identical):

MOVERS

**The screens** (`screen-classes.txt`, over `screen_moves.py`'s 193 listed
snaps whose screen moved beyond clock-like or run-local text): 121 a saved
session's id, 55 **LOCAL-PATHS's placeholder** where main's row held a path
of the run, 7 xfx's `/stats` row (`ansi_bytes` 4367 on main, 4391 at the
head, in `p5-panels-sweep-b`; *(cause not measured)*, the pairs' verdicts
unmoved), 6 the build id, 2 a turn's retry at a fixed moment, 1
SWEEP-WALLCLOCK-F's `model-column-idle` `col-typed` (the rows, after the
release), 1 the MCP state above. **Rows gained** (`gained-rows.txt`): 78
pairs, 77 unresolved or approved on both sides; 57 hold a placeholder where
main's row held a path, 21 run-local text (ports, clocks, a retry's moment,
the `/stats` bytes) -- and `list-remote`, above.

### `model-column-idle`: did SWEEP-WALLCLOCK-F end `catalog-window`? **Yes** (`reruns.txt`)

Train 11's three `col-*` pairs dropped on `catalog-window`: the outage was
the first 9000 ms of the gateway's life, and a pair's verdict followed from
when each binary first asked. **At the head the outcome no longer depends on
that, and the screens are the ones the scenario is written for**
[measured]:

| run | `col-down`, `col-idle`, `col-typed` | what each binary shows (`rerun_verdicts.py`) | catalog asks, fx / xfx |
|---|---|---|---|
| head sweep | NI, NI, NI | failure, failure, rows -- both | 552 / 552 |
| head reruns 1-3 | NI, NI, NI in each | failure, failure, rows -- both, 3 of 3 | 552 / 552, 3 of 3 |
| main sweep | NI, NI, NI | **failure in all three** -- both | 55 / 55 |
| main reruns 1-3 | NI, NI, NI in each | **rows in all three** -- both, 3 of 3 | 2 / 2, 3 of 3 |

On main the pairs pass only because both binaries' first asks fell on the
same side of the window: in its sweep both asks inside it (503, 503: the
keystroke's frame still the failure, where the scenario means the rows), in
the three reruns both past it (one ask, answered 200: the outage never
seen). At the
head every run asks three times, 503, 503, 200, on both binaries, and shows
the failure until the `modelsup` line and the rows after: 4 of 4 runs. The
ledger's `catalog-window` is retired (SWEEP-WALLCLOCK-F's README, "The
ledger"); nothing at the head needed it.

### Against each unit's own sweep (`compare-unit-*.txt`)

| unit (its sweep, as its receipt commits it) | shared | pass, unit -> head | moved |
|---|---|---|---|
| GATE-ENV-MAINTLOCK-F | -- | -- | no sweep (no UI change) |
| SWEEP-WALLCLOCK-F (`green-verdicts.txt`, `green-r2-verdicts.txt`: five runs each) | 26 pairs | 25 -> 25 | **0 up, 0 down** |
| LOCAL-PATHS (`verdicts.tsv`, round 7's column) | 145 pairs | 106 -> 108 | **2 up, 0 down**: `workspace-index` `at-all` and `at-plan` (the run's random name, above) |
| INPUT-ROW-LIMIT (`sweep-41x120-head.md`, at 41x120) | 18 scenarios, 120 pairs | 67 -> 67 | **0**: every scenario the same count (join check 4) |

**So no pair that passes on main's sweep or on any unit's own fails at the
head, but `p5-mcp-menu-new` `list-remote`** -- MCPLINE

## Found on the way, not changed here

- **Merge 4 merged as text and did not build** (one E0308, test code only):
  INPUT-ROW-LIMIT's model-menu case, written on `32afd307`, against
  CATALOG-REFRESH's `CatalogDue` (train 11). The same shape as train 11's
  merge 2: a unit on a base older than main's; `cargo check --all-targets`
  before each merge commit caught it.
- **token_check.py masks the numbers of `src|scripts|tests|docs` references
  only**, so a ledger entry both sides re-pointed, one through a `.prd/`
  receipt, reads "both sides changed it differently" (merge 3's
  `tmux-dead-pane`); `ledger_numbers.py` reads it with every `:N` masked.
  refs.py has the same pattern; `refs_prd.py` reads the `.prd/` ones. Carrying
  `.prd/` into both is REF-LINT's or the next train's.
- **Train 11's token_control.py stops on a merge where no docs/parity.md row
  changed on a side** (a `KeyError`); its train 12 form plants on every row
  both sides changed and says so for a merge with none.
- **Three MCP scenarios' captures keep a fragment of the scenario
  directory** (`capture-scan.txt`: 62 files of the head's sweep, every one in
  `p5-mcp-menu-new` (24 fx, 26 xfx), `p5-mcp-menu-old` (4 and 4) and
  `p5-mcp-fail` (2 and 2)). Their `/mcp` menu's `runs` row prints the stdio
  server's argument, `@SCENARIOS@/p5-mcp-stdio.ts` filled with the scenario
  directory; under this train's clones that directory is 141 bytes, so both
  binaries cut the row with `…` at the same column, and `roots.py`, which
  writes a root it finds whole, leaves the cut one. It holds no run name, so
  it is not refused either. This is LOCAL-PATHS's stated limit (its README
  section 1: "a fragment of a root above it ... holds no run name, and in a
  tracked file the check's shapes are what refuse it"), met by a long
  checkout path, not by this train's merge: the pairs read the same on both
  sides, and a capture holding such a fragment cannot be committed past
  `check-no-local-paths.sh`. Main's sweep, with no rewrite, has them in 950
  of its 9474 capture files (`capture-scan-main.txt`); the head's in 62 of
  9556.
- **`p5-panels-badsettings` fails on both sweeps**: fx ends before its first
  frame (`up: the pane is gone after 0.02 s ... no keep held the pane`),
  exit 1, at main and at the head alike; its two pairs read `failed` on both
  (RERUN_BADSETTINGS). Not this train's change; named for its owner.
- **A sweep at 8 rows cannot finish**: sweep.py reads fx's surface inventory
  from `/help`, which an 8-row screen does not show (`sweep: a help snap
  shows no menu`, exit 2, after both captures were taken). INPUT-ROW-LIMIT's
  8-row pairs are read by `short8.py` instead (join check 4).

## Redaction

The repository is public. Every file here taken from a gate log, a job, the
logger, a sweep or a clone went through `redacted.py` (train 7's, unchanged
but for its line): scripts/fxcap/redact.py's `Redactor.write` -- at this
head, LOCAL-PATHS's, which keeps a path that continues one of capture.sh's
placeholders -- every path under a local root to `<local-path>` or a named
root (`<scratch>`, `<repo>`, `<worktree>`), a checkout's path cut short to
`<checkout>/`, the account's name to `<user>`. The scripts take the oracle's
path, the suite lock and the scratch directories as arguments or variables,
and spell the brief's pattern with bracket classes. **Every commit this train
adds was gated on the brief's check** before it was made (`pathcheck.sh`,
`set -o pipefail` and an explicit `if`): a merge as the combined diff `git
show` prints of it, read from a probe commit of the staged tree
(`git commit-tree`, on no branch), any other commit as its staged lines. Then
check-no-local-paths.sh over the staged receipt. `final-scan.sh` at the
receipt's last commit (`final-scan.txt`).

## Files

- Merges: `blocks.py` (train 11's), `rowsmerge.py` (new), `semantic.py`
  (merge 4's fix; `--reverse`), `rowcheck.py`, `filecheck.py`,
  `reproduce.sh`, `check-merge.sh` / `check-merges.txt`, `resolutions.txt`,
  `remerge-1.txt` .. `remerge-4.txt`, `overlap.py` / `overlap.txt`;
  `pathcheck.sh` (the brief's check before every commit).
- `merge-checks.sh`: writes `resolutions.txt`, the token check and its
  control, the remerge-diffs, the scans, the re-points' check and the
  overlaps.
- `token_check.py` (train 7's, byte for byte), `token_control.py`,
  `ledger_numbers.py`, `token-check.txt`, `token-self-test.txt`,
  `token-control-train11.txt`.
- `refs.py`, `refs_all.py`, `refs_short.py`, `refs_bare.py` (trains 7's and
  9's), `refs_prd.py` (new); `refs-*-pre.txt` (at `12b56ab8`),
  `refs-*-head.txt` (at `03fb5436`); `repoint.py`, `repoint-plan.txt`,
  `repointed.py`, `repointed.txt`; `prd_refs.sh`, `prd-refs.txt`.
- Join checks: `harness_names.sh`, `asks.py`, `capture_scan.py` /
  `capture-scan.txt`; `jobs.sh` (parts `named`, `mutant`, `net`),
  `jobs-*.txt`, `net.py`, `connect.log`; `unit_paths.sh` / `unit-paths.txt`,
  `ci_rows.rb` / `ci-rows.txt`, `shell_hunks.sh` / `shell-hunks.txt`,
  `pairs_of.py`, `short8.py`, `irl-pairs.txt`; `midbranch_scan.py`.
- `gates.sh`, `gates_txt.py`, `gates.txt`, `gate_counts.sh`,
  `gate-counts.txt`; `test_counts.sh`, `test-counts.txt`; `gates-files.sh`,
  `gates-files.txt`; `final-scan.sh`, `final-scan.txt`; `redacted.py`.
- Sweeps: `sweep.sh`, `irl41-selection.txt`, `irl8-selection.txt`,
  `sweep-runs.sh` / `sweep-runs.txt`, `sweep-*.md`; `sweep-compare.sh`,
  `compare.py`, `screen_moves.py`, `unit_compare.py`, `banners.py`,
  `compare-*.txt`, `screen-moves-main.txt`; `sweep-rerun.sh`, `reruns.sh`,
  `reruns.txt`.
