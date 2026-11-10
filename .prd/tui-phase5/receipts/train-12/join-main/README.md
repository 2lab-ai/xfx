---
title: 0.0.16 delta -- P5 train 12 joins main (PR #91, preview 0.2.0)
type: receipt
tags: [xfx, phase5, fx-0.0.16, train, merge]
---

# Train 12 joins main: PR #91, one merge, no conflict

PR #91 (the 0.2.0 preview) merged to main as `8f921425`. Train 12 (PR #90,
`train/p5-1110a` at `71d3e02b`) had to join it before it can merge.
`git fetch origin main` moved `origin/main` from `df46777e` to `8f921425`;
`df46777e` is the merge base. Main brought PR #91 alone: the version line
says when the build is from (`XFX_BUILD_ID` in place of
`XFX_BUILD_REVISION`), a width-bounded reader of a banner the screen wrapped
(cells.py, capfile.py, sweep.py, test_sweep.py), the retired
`xfx-debug-banner-wrap` cause, and the preview's receipt.

| commit | what |
|---|---|
| `0c39ce8c` | `git merge --no-ff origin/main`, parents `71d3e02b` (train) and `8f921425` (main). No conflict |
| `c7d30afc` | the 23 references the merge moved, re-pointed; numbers only (`repoint.py`, `repoint-plan.txt`) |
| `2e388ca8` | three test comments that named the wrong code, re-pointed; numbers only (`repoint.py ... late`, `repoint-late.txt`) |
| this commit | this folder |

## The merge, from both ends (`merge_ends.py` / `merge-ends.txt`, `rows_ends.py` / `rows-ends.txt`)

The brief forecast conflicts in the ledger, `docs/parity.md`, and perhaps
cells.py, sweep.py, ledger.py and test_sweep.py. There were none: git
merged every file. Eight files were changed on both sides: CHANGELOG.md,
README.md, docs/parity.md, parity-ledger.toml, sweep.py, test_sweep.py,
test-gate-env.sh and src/tui/shell.rs.

- `merge-ends.txt`: for each of the eight, ours -> merged holds exactly the
  line edits of base -> theirs, and theirs -> merged exactly base -> ours.
  The 820 files only one side changed are that side's bytes (779 the
  train's, 41 main's). 0 disagree.
- `rows-ends.txt`: docs/parity.md's 157 row keys (first cells, rowsmerge.py's
  key). The train changed 3, main changed 3 (`/version`, `status`, version
  text), and no row was changed on both sides. Every row is as the side that
  changed it left it, or as the base had it if neither did.
- No ledger entry was edited on both sides. Main removed the
  `xfx-debug-banner-wrap` cause, and no file outside `.prd/` and the
  historical plans names it at the head. `XFX_BUILD_REVISION` is left only
  in CHANGELOG.md's line saying it is gone.
- The ledger loads, and the merge tree passed `cargo check --all-targets`
  with and without `fault-injection` and `cargo fmt --check` before it was
  committed.

## The references (`repoint.py`, `frames.py` / `frames.txt`, `refs*-pre.txt`, `refs*-head.txt`)

Train 12's five checks (`../refs.py`, `../refs_prd.py`, `../refs_all.py`,
`../refs_short.py`, `../refs_bare.py`) ran at the merge, with `-pre`
(`0c39ce8c`), and at the head, with `-head` (`2e388ca8`), each against
`train=71d3e02b main=8f921425`. Outside the historical plans
(`docs/superpowers/plans/`), they flagged 28 references MOVED at the merge. 26
are re-pointed and 2 stay:

- **23 re-pointed in `c7d30afc`** (`repoint-plan.txt`). They were in the
  ledger (16), capfile.py (2), smoke-tui.sh, event_loop.rs, shell.rs (2),
  and docs/parity.md's `.prd/01-spec.md:56` (main's own, stale since main's
  `5700fdaa`). Each one is mapped from the frame of the commit that last
  wrote its citing line, through `git diff -U0`. The cited line, with two
  lines either side, is the same code at the new number. That commit was
  applied reading the lines both sides hold in the merge base's frame.
  `frames.txt` reads all 23 again from the blamed commit and gets the same
  numbers. `repoint.py` now reads that frame.
- **3 in tests/ re-pointed in `2e388ca8`**. The first plan missed them
  because its list was cut short when it was read. Mapping alone would have
  kept them wrong:
  - tests/sessions.rs `src/app.rs:242` -> `:280`. Mapped from the commit
    that wrote it (`7daab7f9`), it names
    `SessionDetailSnapshot::new(&detail).render(..)`. In the base's frame it
    lands on `Command::Sessions`, because src/app.rs moved between the two
    and nothing re-pointed it. This is the one `frames.txt` row where the
    two readings differ.
  - tests/tui.rs `src/tui/shell.rs:2814` -> `:16806` and `:2113` ->
    `:9667`. At train 11's re-point (`6ae87955`), the last commit to write
    them, they named a struct field and a `.collect()`, and mapping keeps
    that in every frame. So they are set by name, not mapped, and
    `repoint.py` checks that each new line holds the named code.
    `Shell::blind` is `fn blind`: at `e94af067`, which wrote both
    references, its `:2703` was that line. The Redraw reference has two
    candidates. At `e94af067`, `decide`'s Redraw arm was line 1400 and
    `act`'s was 2018. The `:2015` written there sat three lines above
    `act`'s arm and was not on either. The comment says the Redraw is
    answered "inside the approval panel" and "answers nothing". With the
    panel open, `decide` handles the Redraw and returns before `act` runs.
    So `:9667` is `decide`'s arm. `act`'s arm is now `:12825`, if a
    reviewer reads the comment the other way.
- **2 stay**:
  - test_sweep.py's `scripts/fxcap/cells.py:100000` is deliberately a line
    past the end of the file. The test checks that a reference to a line
    the file lacks is refused.
  - the ledger's `capture.sh:987` at line 86 is the train's own
    (capture-tmpdir, re-pointed in `ea772025`) and reads `=` in the train's
    frame. Main's MOVED there is main's old `:987` for tmux-dead-pane, which
    the train had already re-pointed to `:1157`.

At the head, outside the historical plans: refs.py 1 MOVED (that
`capture.sh:987`), refs_prd.py 0, refs_all.py 2 (`capture.sh:987`,
`cells.py:100000`), refs_short.py 0, refs_bare.py 0.

## Local paths and the tree's checks

`scripts/check-no-local-paths.sh` over the merged tree, which now holds
main's preview receipt: ok, 0 hits. `check-no-secrets.sh`,
`check-no-stubs.sh` and `check-xfx-identity.sh`: ok.

The brief's path check, behind `if` under `pipefail` (`pathcheck.txt`):

- the merge's added lines against the train (`71d3e02b`), which are what
  main brings, including the preview receipt: 1909 lines, 0 hits. This is
  the `git diff --cached` the merge was committed behind;
- against main (`8f921425`), which is everything the train brings: 44613
  lines, 21 hits. These are the range's 21 that the dispatcher accepted in
  round 2 (README, join check 7): `git diff df46777e 71d3e02b` holds the
  same 21. The merge adds none;
- `c7d30afc` and `2e388ca8`: 0 each, and this commit 0.

## Gates (`gates.txt`)

These ran at `2e388ca8` under the suite lock, one after another, with
`TMPDIR=<tmpdir>`, as one batch (`<scratch>/join-gates.sh`). `gates.txt`
keeps each step's command, times, exit and the lines that count, with paths
replaced by placeholders (`gates_txt.sh`). Every step exited 0:

| step | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok, no warning |
| the same with `--features fault-injection` | ok, no warning |
| `cargo test --locked` | 15 test binaries: 5668 passed, 0 failed, 7 ignored (lib: 4744) |
| `cargo test --locked --features fault-injection` | 15 test binaries: 5705 passed, 0 failed, 7 ignored (lib: 4767) |
| `scripts/fxcap/test-sweep.sh` | 76 + 28 tests, OK |
| `scripts/fxcap/test-capture.sh` | 108 tests, OK |
| `scripts/test-preview-contract.sh` | ok, 28 mutants caught; the unbroken copy passes |
| `scripts/smoke-tui.sh` (release build, `--faulty` the fault-injection release build, as CI runs it) | 37 scenarios + the oracle, 1167 checks, 0 failures |

## Files

- `merge_ends.py` / `merge-ends.txt`, `rows_ends.py` / `rows-ends.txt`: the
  merge from both ends.
- `repoint.py` (`--derive`, `--apply`; `late` for `2e388ca8`'s three),
  `repoint-plan.txt`, `repoint-late.txt`; `frames.py` / `frames.txt`.
- `refs-pre.txt`, `refs_*-pre.txt` (at `0c39ce8c`), `refs-head.txt`,
  `refs_*-head.txt` (at `2e388ca8`), written by `../refs*.py`.
- `pathcheck.txt`; `gates_txt.sh` / `gates.txt`.
