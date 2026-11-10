---
title: 0.0.16 delta -- P5 train 12, the CI sweep job's 60-minute limit (TRAIN12-CI rounds 2 and 3)
type: receipt
tags: [xfx, phase5, fx-0.0.16, train, fxcap, ci]
---

# Train 12: the sweep job ran out of time, and one fx run was never held

PR #90's second CI run (42748982362, head `1b15dad9`) passed all five gate
jobs and the sweep job's harness tests. The sweep job was then cancelled at
its 60-minute limit, in the step "Sweep the changed scenarios and the core
set" (`job-steps.txt`). Every figure below is in a file in this folder.
`artifact/` is the run's uploaded artifact (the head's and the first
retry's scoreboards). `ci-sweep2-step.log` is that step's log.

| commit | what |
|---|---|
| `1a9121a4` | round 2: deleted `p5-panels-badsettings` and its seed file. Its reason was false for xfx; round 3 restores it (§ The failed run) |
| `09bf63ab` | `sweep.yml`'s job timeout goes from 60 to 120 minutes; `test_ci_sweep.py` pins it and `--jobs 2` |
| `fc1d9830`, `5651cf23` | round 2's receipt, and a parser fix in `../ci-socket/sample_vs_head.py` (§ Also) |
| `f0a9af5b` | round 3: the scenario restored as a launch-only capture, its ledger cause, the sibling scenario's false comment, and "estimated" in `sweep.yml` and the test |
| the next commit after `f0a9af5b` | round 3's corrections to this receipt |

## What the step did (`measure.txt`, from the log)

- **The head sweep** ran 125 scenarios: 251 runs and 2800 s of wall time, at
  1.98 run-seconds per wall second with `--jobs 2`.
- **The retries.** The first retry (`titles-idle`, `resume-paging`) took
  172 s. The second was cancelled after one run.
- **The base sweep** and the verdict never ran.

## The cause: a selection the receipts rule made, timed

- **The selection** (`select.txt`, `ci-sweep.sh --select --base df46777e` at
  `1b15dad9`; counted in `measure.txt`). 125 scenarios:
  - 103 selected by a receipt alone;
  - 11 changed, also named by a receipt;
  - 8 core;
  - 2 core, also named by a receipt;
  - 1 changed, used and named by a receipt.

  The 103 receipt-only scenarios are **89% of the head's run-seconds**
  (4894 of 5517).
- **Where the 103 come from** (`receipts-rule.txt`):
  - The change adds or edits 666 receipts: 508 new and 158 edited.
    `ci_sweep.select` reads every scenario name in the whole of each one
    (`scripts/fxcap/ci_sweep.py:240-247`, `names_in` at `:250-261`).
  - **All 158 edited receipts** were edited only by LOCAL-PATHS's own
    commits: its path rewrites and its gate (`edited-receipts.txt`, the
    commits on `55d993a7`'s second parent and not its first). The largest
    folder is phase 4's `s1`, with 53 files (`receipts-rule.txt`).
  - Their added lines name 15 scenarios. Read whole, the receipts the change
    touches name 103. Only 23 of those are on lines the change adds, and
    they are the same 23 the new receipts name.
  - So **80 scenarios are selected only by unchanged lines of receipts whose
    other lines changed.**
- **What the whole step needs** (`measure.txt`). This is an estimate from
  one run: the parts after the head never ran.

  | part | seconds | from |
  |---|---|---|
  | head | 2800 | measured |
  | head retries | about 516 | 3 x the first retry's measured 172 s (each retry re-runs at most the scenarios the one before it did, `ci_sweep.py:375-385`) |
  | base sweep | about 1285 | the 49 scenarios that hold a pair the head does not pass and the base snaps (2 more are new at the head and are never swept on the base): their head run-seconds at 1.98 per second, plus fx's inventory run |
  | base retries | about 516 | 3 x the first retry's measured 172 s: the two base-swept scenarios a live `nondeterminism` cause covers are the two the head's retry re-ran |
  | **step** | **about 5117 (85 min)** | |
  | before the step | 591 | job start to the step's start (`job-steps.txt`) |
  | **job** | **about 5708 (95 min)** | |

  60 minutes could not hold this selection: the head sweep plus the steps
  before it already took 57 minutes.

## The fix (`09bf63ab`)

- **`timeout-minutes: 120`**: the estimated 95 minutes, plus about a
  quarter. A comment in `sweep.yml` gives the figures and says they are an
  estimate from one run. `--jobs` stays 2.
- **The test.** `test_the_sweep_job_has_the_time_its_selection_needs`
  requires the job's timeout to be at least 95 x 5/4 (118) minutes, and the
  sweep step to run `--jobs 2`.
  - `wf-test-red.txt`: against the old file it fails with `60 not greater
    than or equal to 118`.
  - `wf-test-green.txt`: it passes at 120.
  - `sweep-tests.txt`: `test-sweep.sh` passes, 72 and 28 tests OK, and
    `capture-tests.txt`: `test-capture.sh` passes, 108 tests OK, both on
    round 3's tree at the runner-length TMPDIR.
  - No other test pins `sweep.yml`'s timeout (`timeout-pins.txt`): the
    only other pin is `test-preview-contract.sh`'s, on `preview.yml`'s 45.
- **Proposed, not changed: a narrower receipts rule.** A receipt would
  select a scenario only where the lines the change adds to it name the
  scenario, or the receipt's own path does. A new receipt counts whole,
  since every line of it is added.
  - Here that selects 45 scenarios instead of 125, about 890 s of head
    instead of 2786 (`receipts-rule.txt`).
  - What it gives up: a scenario named only on a receipt's unchanged lines,
    such as a path rewrite's untouched neighbours, is no longer swept.
  - Which scenarios a change sweeps is a review decision, so the rule is
    left as it is here. It is queued as its own unit, CI-SELECT-ADDED-LINES,
    which the review would have count removed lines as well (a retracted
    claim about a scenario). `receipts-rule.txt` counts added lines only.

## The failed run: `fx p5-panels-badsettings`, exit 1 in 0 s

- **One run, two pairs.**
  - `artifact/head/summary.md` says `Scenarios: 125 -- ran 124, failed 1`
    and lists `failed 2` among the pairs.
  - `artifact/head/scoreboard.json` gives that scenario `fx: exit 1`, with
    its 2 pairs (`launch`, `open`) failed.
  - So the second "failed" is the same run's second snap. No other run
    failed (`measure.txt`: `runs not 'ran': 1`).
- **The cause, from the code** (`fx-src.txt`, `badsettings-repro.txt` at
  the runner-length TMPDIR, the scenario as it was at `1b15dad9`):
  - **The seed.** The scenario seeds `{"permission_mode": "auto",
    "sandbox": ` (cut off, so malformed JSON) as the profile, before launch.
  - **What fx does with it.** fx v0.0.16's default auth mode is local
    (`credentials.zig:29`). In local mode it refuses a user profile with a
    `malformed_settings` diagnostic: `return
    error.InvalidProfileConfiguration` (`app_lifecycle.zig:690-701@481104fd`).
  - **What the repro shows.** capture.sh reports `up: the pane is gone after
    0.02 s`. fx run alone in a pane of its own exits 1 with `fx:
    InvalidProfileConfiguration` on standard error.
  - **History** (`badsettings-history.txt`). Every sweep row a receipt
    holds for it reads `failed`: train 12's `../sweep-main.md` and
    `../sweep-head.md`, and trains 5 and 7, cancel-marker, ctrlo-wider and
    launch-settle before them. No row reads `ran`, so it was never a
    verdict.
- **Round 2 deleted it for a false reason.** Its receipt said the surface
  was covered elsewhere and that a rewrite would duplicate
  `p5-panels-status-badsettings`. That is true of the `/status` row, but not
  of the launch:
  - fx refuses the malformed profile at launch. xfx does not: malformed
    content is never fatal (`src/config.rs:850-851`). It starts and
    announces `config: 1 configuration issue (ctrl+o to view)`
    (`src/tui/shell.rs:3820-3837`).
  - CI's own log has both sides: `fx p5-panels-badsettings: failed exit 1
    (0s)` and `xfx p5-panels-badsettings: ran` (`ci-sweep2-step.log`).
  - The other two scenarios swap the bad file in after launch, so this one
    was the only capture of that gap. Its 0-second failure was the missing
    `keep`: without it, a pane whose binary ended is a failed run
    (`capture.sh:606`); with it, a final state.
- **Round 3 restores it as a launch-only capture.** The new form keeps
  `keep`, the env lines, the seed (restored byte for byte), `wait 3` and
  `snap launch`. It drops `type /status`, `key Enter` and `snap open`,
  since fx leaves no TUI to type into.
  - Its comment says what each binary does.
  - `p5-panels-status-badsettings`'s comment no longer says both binaries
    refuse at launch.
- **The ledger cause `profile-refuse-at-launch`.** It covers the whole
  `launch` snap; the owner is PROFILE-REFUSE-AT-LAUNCH, a new queue unit
  outside this PR. It is the first `[[cause]]`, so it is that snap's only
  cause.
- **Swept the way CI sweeps it** (`restored-sweep.sh` → `restored-sweep.txt`,
  the runner-length TMPDIR, the oracle's sha256 the pin's before and after;
  `restored-launch-fx.txt` and `restored-launch-xfx.txt`, the head's snaps):
  - With HEAD's scenarios and ledger, both runs now `ran`: fx's pane reads
    `Pane is dead (status 1, …)`, and xfx's has its banner, the config
    notice and a permissions refusal.
  - With the head's xfx and the base's (`df46777e`), the pair is
    `unresolved-defect` on both, its only cause `profile-refuse-at-launch`,
    with 0 rows no cause covers.
  - So it is reported, not a regression: the base does not pass it
    (`ci_sweep.py:427`).
  - `select-after.txt` (`--select` at `f0a9af5b`) selects it as `changed,
    receipt`, so the next CI run sweeps it.

## Look-ahead: what the base sweep could call a regression (`lookahead.txt`)

- **What was compared.** The artifact has every scenario's counts but not
  every pair's verdict (no `pairs.json`). Its per-pair entries are the 42
  scenarios' first unresolved pairs (`first_unresolved` in
  `artifact/head/scoreboard.json`). So this compares pass counts scenario
  by scenario, then those first pairs. CI's head is set beside train 12's
  sweep of `df46777e` (`../sweep-main.md`, the PR's base) and of its own
  code head (`../sweep-head.md`).
- **Result.** Of 125 scenarios:
  - 119 pass as many pairs;
  - 2 pass more;
  - 3 are new (the `p5-input-row-limit` three, never failed as
    regressions);
  - **1 passes fewer**: `resume-paging`, 1 of 8 at CI's head (and on its
    retry) against 2 of 8 at main and at train 12's head.
- **That pair is the `session-age` nondeterminism**
  (`parity-ledger.toml:1058-1067`). The picker's age column reads ` 1m` on
  one side and `now` on the other (CI's first unresolved pair is `down14`
  row 22).
  - It gets three retries on each build.
  - It is called a regression only if no head run passes it and some base
    run does. Then it would be this flake, not a change of the head's.
- **The first unresolved pairs.** sweep.py walks a scenario's snaps in the
  `.cap`'s order and keeps the first unresolved one (`sweep.py:468`,
  `:523-526`). So a CI first-unresolved snap that comes earlier than main's
  was not unresolved at main: it passed there or was approved. Of the 40
  scenarios with one on both sides, **0** are earlier. A pair main passed
  and CI's head does not, hidden at equal counts, would show here if it
  came before main's first unresolved snap.
- **Not seen:** such a swap after both sides' first unresolved snap.

## Also

- **CI's tmux is 3.8** (`tmux-ci.txt`). The harness step passed there at
  `1b15dad9` (`job-steps.txt`, step 7), including RunRoots' 103-byte
  socket. That answers `../ci-socket/`'s first open item.
- **`../ci-socket/sample_vs_head.py` read the wrong columns.** It took only
  the normalized-identical column, from any table of `sweep-head.md`. It now
  reads the scenario table and counts identical plus normalized-identical.
  Its output is byte for byte the same (`sample-vs-train12-head.txt`): none
  of those 11 rows had an identical pair, and the group row `startup`
  equals its one scenario's.

## Files

`runs.sh` (makes `select.txt`, `select-after.txt`, `measure.txt`,
`receipts-rule.txt`, `lookahead.txt` from this folder's files and two
checkouts) · `measure.py` · `receipts_rule.py` · `lookahead.py` ·
`edited_receipts.py` → `edited-receipts.txt` ·
`ci-sweep2-step.log` · `job-steps.txt` · `tmux-ci.txt` · `timeout-pins.txt` ·
`badsettings-history.txt` · `artifact/head/`
and `artifact/head-retry-1/` (`scoreboard.json`, `summary.md`) ·
`badsettings-repro.sh` → `badsettings-repro.txt` · `fx_src.sh` →
`fx-src.txt` · `restored-sweep.sh` → `restored-sweep.txt`,
`restored-launch-fx.txt`, `restored-launch-xfx.txt` · `surface-sweep.sh` →
`surface-sweep.txt` (round 2: the `/status` row's two scenarios swept) ·
`wf-test-red.txt` · `wf-test-green.txt` · `sweep-tests.txt` ·
`capture-tests.txt`
