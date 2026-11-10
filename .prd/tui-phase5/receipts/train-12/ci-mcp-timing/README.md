---
title: 0.0.16 delta -- P5 train 12, the MCP menu's `remote` status word (TRAIN12-CI rounds 4 and 5)
type: receipt
tags: [xfx, phase5, fx-0.0.16, train, fxcap, mcp]
---

# Train 12: two "regressions" that were a snap landing on a retry's edge

PR #90's third CI run (42762620303) finished inside the 120 minutes. Its
verdict was **2 regressions**, both in `p5-mcp-menu-new`
(`ci-report-excerpt.txt`):

- `reopened`: row 8, `remote` reads `retrying` on fx and `connecting` on
  xfx;
- `list-rejected`: row 8 again, with the two words the other way round.

The base was normalized-identical on both pairs, and each ran once. No
ledger cause covers them, so there were no retries. This is a capture-timing
flake. The product did not change: train 12 touches nothing under `src/mcp`
(`code.txt`). Every figure below is in a file in this folder.

| commit | what |
|---|---|
| `2ee33f58` | round 4's fix: `remote` never answers, so it reads `connecting` for the whole run. It changed the shared `p5-mcp-profile.json`, which broke a unit test; round 5 moves it |
| `5f5074ed`, `635e7242` | round 4's receipt |
| `6bfc6562` | round 5: the held `remote` in `p5-mcp-menu-profile.json`, which only `p5-mcp-menu-new` and `p5-mcp-menu-old` name. `p5-mcp-profile.json`, `p5-mcp-verbs.cap` and `p5-mcp-config.cap` are back to their bytes at `3943d667`. The ledger cause `verbs-refused-port-word` added |
| `f4b47260` | round 5's corrections and gates in this receipt |

## Measured: 8 runs on each build (`before-runs.txt`, `before-verdicts.txt`, `before-words.txt`)

The setup:

- the head's harness, scenarios and ledger for both builds, as ci-sweep
  runs them;
- the head's xfx (src as at `3943d667`) and the base's (`df46777e`);
- fx v0.0.16, its sha256 the pin's before and after every sweep, with
  `FX_AUTO_UPGRADE=0`;
- `--jobs 2`, at a 49-byte TMPDIR, under the suite lock.

What it showed:

- **Both binaries' words vary from run to run.** At `back-list`, fx read
  `retrying` 14 times and `connecting` twice; xfx read `retrying` 12 times
  and `connecting` 4 times. The words are read from the `.ansi` capture,
  the one the sweep scores (`sweep.py:475-476`).
- **Every failure is the word.** Each of the 7 failed pairs differs at row
  7, `remote`'s word, and at no other row (`before-failures.txt`). One of
  them, `back-list` on base-4, fell on the turn inside its own snap.
  capture.sh takes the `.txt` and the `.ansi` with two `capture-pane` calls
  one after the other (`capture.sh:1096-1097`). fx's `.txt` read
  `retrying` and its `.ansi` `connecting` (`before-words.txt`, last lines).
- **Two pairs flipped, on both builds.** `back-list` passed 7 of 8 on the
  head and 5 of 8 on the base. `list-remote` passed 7 of 8 and 6 of 8.
  Across the 64 head-run x base-run pairings, 5 and 6 of them would read as
  a regression.
- **`reopened` and `list-rejected` held here.** Every run read `connecting`
  and `retrying` respectively. CI's runner is slower: this scenario took
  62 to 64 s there against a 61 s median here (`run-seconds.txt`). That
  moved these snaps onto the next edge (below).

## The cause, from the code (`code.txt`)

- **The status word.** It is the try's state. A try sets `connecting`; a
  failed try with tries left sets `retrying` and waits out its backoff
  (`runtime.rs:1328-1355`). fx v0.0.16's engine does the same
  (`engine.zig:193-211`).
- **A try against a refused port takes 10 s, not an instant.** The first
  step is a `server/discover` probe. A broken connection is sent once more,
  then the probe waits out its 10 s timer (`runtime.rs:1586-1642`,
  `era.rs:67`). fx does the same (`http_modern.zig:23` `max_attempts = 2`,
  `era.zig:25` `probe_timeout_ms = 10_000`, and `endAttempt`,
  `server.zig:1242-1257`, which ends a broken attempt without an answer).
  Then `initialize` is refused at once.
- **The backoffs are 1, 2, 4, 8 s** (`runtime.rs:93-106`; fx
  `request.zig:76-79`).
- **So the word turns at about 10, 11, 21, 23, 33, 37, 47 and 55 s** on
  both binaries. The two binaries start their tries a little apart, so a
  snap within about a second of a turn reads one word on one binary and
  the other on the other.
  - `before-timeline.txt` places every snap at its time since launch.
  - `back-list`, at 22.3 to 23.8 s, read `retrying` up to 23.1 s and
    `connecting` from 23.0 s: the turn at 23 s (the summary at the end of
    `before-timeline.txt`).
  - `reopened` (42.9 to 46.0 s) and `list-rejected` (50.2 to 53.5 s) sit
    before the turns at 47 and 55 s, which CI's slower clock reached.
- **What the snaps wait for.** Fixed `wait` lines from the launch. The
  only `until` is `startup-settled`'s. Nothing waits for `remote`'s state,
  and no state of a refused-port server lasts the whole run.
- **What train 12 changed.** No MCP code (`git diff --stat df46777e HEAD --
  src/mcp` is empty). It did change the harness: LOCAL-PATHS runs roots.py
  on every file a snap writes before the next step (`publish`,
  `capture.sh:455`). capture.sh at `55d993a7^1` has no `publish`
  (`code.txt`).
  - That costs about 0.09 s a snap here (`publish-cost.txt`). There are 26
    snaps before `reopened` and 30 before `list-rejected`, so they now run
    about 2.3 and 2.7 s later than at the base's harness.
  - Train 12's own receipt already saw `list-remote` move this way
    (`../README.md:472-482`). Both
    builds run the head's harness in ci-sweep, so the shift is the same on
    both. It moved which snaps fall near a turn; it did not make the head
    differ from the base.

## The fix (`2ee33f58`, moved in `6bfc6562`): a state that holds on both binaries

- **`remote` never answers.** It is the run's mcphttp server in `hang`
  (`http://127.0.0.1:@MCPPORT@/mcp`), with `"startup_timeout_ms":600000`,
  which both binaries read per server (xfx `config.rs:453-461`; fx
  `project_config.zig:1054`, `:1130-1140@481104fd`, for an HTTP entry of
  `mcpServers`).
  - The discover probe times out at 10 s. `initialize` is then held until
    the start's deadline, 600 s on. xfx: `runtime.rs:1375`, the try's
    deadline. fx: `startLeft`, `server.zig:966-971`, and its use at
    `:2043`.
  - So its first try outlasts the run, and it reads `connecting` in every
    snap on both binaries.
  - The sweep already writes the run's mcphttp port as `<mcpport>`
    (`sweep.py` `run_rules`).
- **In its own profile, `p5-mcp-menu-profile.json`.** `2ee33f58` put this
  in the shared `p5-mcp-profile.json`. That broke
  `the_verbs_scenario_leaves_fx_v0_0_16s_bytes` (`src/mcp/config.rs:3425-3436`),
  which seeds itself from that file and expects its `:9` URL. `6bfc6562`
  restores the shared file's bytes and gives the menu scenarios a file of
  their own.
- **Where it applies:**
  - `p5-mcp-menu-new`.
  - `p5-mcp-menu-old`, which had the same server and the same flaw: its
    base run flipped `prompts` and `resources` on `remote`'s word
    (`others-before.txt`).
- **What the menu scenarios no longer compare: a `retrying` row.** Before,
  `back-list` (mostly) and `list-rejected` (always) read `retrying`
  (`before-words.txt`). `retrying` is still compared elsewhere:
  - `p5-mcp-failed`'s status line (`retrying`, `still-retrying` snaps);
  - the `p5-mcp-discover-*` scenarios whose server shares no version
    (`hostile`, `old2024`, `depth`, `escaped`);
  - `p5-mcp-verbs` (below).
- **Not `p5-mcp-config`.** `2ee33f58` held its added `web` too; `6bfc6562`
  reverts that. `web` got only the default 30 s start, the scenario gained
  no pair (1 of 3 a run, before and after), and its typed `:9` URL is
  MCP-C's oracle (`config.rs:3468`, `../../mcp-config/README.md:49`).
  Its `list` pair is unresolved on both builds at the `/mcp list` header,
  whose `SOURCE` column is a cell wider on xfx (`config-rows.txt`).
- **Not `p5-mcp-verbs`.** It keeps the refused port and its old profile.
  - The hang was tried there (`verbs-hang-trial.txt`). `/mcp login remote`
    then waits on the server instead of failing. fx's `Couldn't sign in to
    remote` and xfx's `Interactive MCP authentication is unavailable here`
    are gone, and 43 captures change.
  - Every pair of it except `startup` is unresolved on both builds: 27 of
    them. All 27 have rows left unmatched that do not hold the word
    (`verbs-rows.txt`). In 17 the first is the `/mcp list` header, whose
    `SOURCE` column is a cell wider on xfx. So holding the word would pass
    none of them.
  - The flake is tracked by the ledger cause `verbs-refused-port-word`
    (owner MCP-V). It names the rows with `remote`, `web` or `web2`
    `connecting`/`retrying`; a rescore of a kept sweep names it on 10 verbs
    pairs and changes no verdict (`rescore-cause.txt`).
  - It is a `scenario` cause, not `nondeterminism`. A nondeterminism cause
    would buy each CI run up to three head and three base retries of this
    90 s scenario, about 18 minutes of the 120, for pairs no retry can
    pass. The cause says to change kind, or apply its fix, once MCP-V's
    other rows match.
- **`p5-mcp-config-vars` has no such server.** Its `remote` is skipped for
  an unset `${...}` and shows no status word (`others-before.txt`, `scan.py`).

## After: 5 runs on each build (`after-runs.txt`, `after-verdicts.txt`, `after-words.txt`, `content-diff.txt`)

These are round 4's runs, at `2ee33f58`, with the hang in the shared
profile and in `p5-mcp-config`. Round 5's runs of the menu scenarios at
`6bfc6562` are below (`r5-repeat.txt`).

- **Every pair passes or fails the same way in every run.**
  - `p5-mcp-menu-new`: 170 of 170 on both builds.
  - `p5-mcp-menu-old`: 80 of 80.
  - `p5-mcp-config`: 5 of 15 on both, 1 of 3 a run as before.
  - `p5-mcp-verbs`: 5 of 140 on both, unchanged.
- **0 pairs vary across a build's runs**, and 0 head-run x base-run
  pairings would read as a regression.
- **`remote` reads `connecting` in all 10 runs** of every `p5-mcp-menu-new`
  snap that shows it.
- **Nothing else changed.** Against the runs before, no capture of the four
  scenarios differs except in the status word and the port. This compares
  the first run of each build, one against one (`content-diff.txt`).
- **After the move (`6bfc6562`, `r5-repeat.txt`, `r5-verdicts.txt`,
  `r5-words.txt`):** `p5-mcp-menu-new` and `p5-mcp-menu-old`, 5 runs per
  build again, with `p5-mcp-menu-profile.json`.
  - `p5-mcp-menu-new` 170 of 170 and `p5-mcp-menu-old` 80 of 80 on both
    builds; 0 pairs vary, 0 pairings would read as a regression.
  - `remote` reads `connecting` in every snap of all 10 runs, and no
    snap's `.txt` and `.ansi` disagree.
- **Gates at `6bfc6562`:**
  - `cargo test --locked --lib mcp::config`: 59 passed, `the_verbs_scenario_leaves_fx_v0_0_16s_bytes` among them (`r5-cargo-mcp-config.txt`);
  - the full `cargo test --locked` (`r5-cargo-full.txt`):
    - **The first run failed one test**, 4729 passed. The failure was
      `local_time::tests::the_c_library_reads_a_zone_that_cannot_apply_in_utc`:
      its child copy of the test binary exited without a panic line
      (`the copy failed: `), while three other suites held lock slots.
    - **That test then passed 3 of 3 alone, and the whole suite passed
      again:** 4730 passed, every test binary ok.
    - Neither this unit nor train 12 changes `src/local_time` or
      `src/process.rs` (`git diff --stat df46777e HEAD` over both is
      empty).
  - `test-sweep.sh`: 72 and 28 OK (`r5-sweep-tests.txt`);
  - `test-capture.sh`: 108 OK (`r5-capture-tests.txt`).
  - Round 4's `test-sweep.sh` is `sweep-tests.txt`, and its selection is
    `select-after.txt`. Round 5's selection is `r5-select.txt`: 125 scenarios, `p5-mcp-menu-new` and `p5-mcp-menu-old` as `changed` and using `p5-mcp-menu-profile.json`, so the base also runs the held form.

## Also

- **The timeout estimate held.** CI's step took 5188 s: head 2813, retries
  520, base 1852 (`ci-report-excerpt.txt`). `../ci-timeout/` estimated
  about 5117 s for the step.
- **Open, queue unit LOCAL-PATHS-WRAP: a path xfx wraps behind its `↳`
  mark is not rewritten.** A trial with a 118-byte scenario directory wrapped
  `approve-confirm`'s command line. The published capture held the
  directory's first 116 bytes on one row and its last 2 on the next, and
  `roots.py --check` passed it (`gutter-check.txt`). roots.py reads
  U+2500-U+259F as gutters (`roots.py:46`), and `↳` is U+21B3. CI's
  scenario directory is short enough not to wrap, so no committed capture
  is known to hold one.

## Files

`repeat.sh` (N sweeps on each build; `SCENARIOS_DIR` for a trial copy) ·
`verdicts.py`, `words.py`, `timeline.py`, `scan.py`, `content_diff.py`,
`verbs_rows.py` (read its out directories) · `verbs-rows.txt`, `config-rows.txt` ·
`rescore-cause.txt` · `before-failures.txt` · `r5-repeat.txt`,
`r5-verdicts.txt`, `r5-words.txt`, `r5-cargo-mcp-config.txt`,
`r5-cargo-full.txt`, `r5-sweep-tests.txt`, `r5-capture-tests.txt`,
`r5-select.txt` · `code.sh` → `code.txt` · `publish_cost.sh` →
`publish-cost.txt` · `gutter_check.py` → `gutter-check.txt` ·
`before-runs.txt`, `before-verdicts.txt`, `before-words.txt`,
`before-timeline.txt` · `others-before.txt` · `verbs-hang-trial.txt` ·
`after-runs.txt`, `after-verdicts.txt`, `after-words.txt` ·
`content-diff.txt` · `sweep-tests.txt` · `select-after.txt` ·
`ci-report-excerpt.txt` · `run-seconds.txt`
