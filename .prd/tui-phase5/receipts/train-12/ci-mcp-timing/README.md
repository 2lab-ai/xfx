---
title: 0.0.16 delta -- P5 train 12, the MCP menu's `remote` status word (TRAIN12-CI round 4)
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
| `2ee33f58` | the fix: `remote` never answers, so it reads `connecting` for the whole run in `p5-mcp-menu-new`, `p5-mcp-menu-old` and `p5-mcp-config`; `p5-mcp-verbs` keeps the refused port in a profile of its own |
| `5f5074ed` | this receipt |

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
  `retrying` 15 times and `connecting` once; xfx read `retrying` 12 times
  and `connecting` 4 times.
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
  `era.zig:25` `probe_timeout_ms = 10_000`). Then `initialize` is refused at
  once.
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

## The fix (`2ee33f58`): a state that holds on both binaries

- **`remote` never answers.** It is the run's mcphttp server in `hang`
  (`http://127.0.0.1:@MCPPORT@/mcp`), with `"startup_timeout_ms":600000`,
  which both binaries read per server (xfx `config.rs:453-461`; fx
  `project_config.zig:1054`, `:1130-1140@481104fd`, for an HTTP entry of
  `mcpServers`).
  - Its discover probe times out at 10 s, and its `initialize` is held until
    the start's deadline, 600 s on. So its first try outlasts the run, and
    it reads `connecting` in every snap on both binaries.
  - The sweep already writes the run's mcphttp port as `<mcpport>`
    (`sweep.py` `run_rules`).
- **Where it applies:**
  - `p5-mcp-menu-new`.
  - `p5-mcp-menu-old`, which uses the same profile and has the same flaw:
    its base run flipped `prompts` and `resources` on `remote`'s word
    (`others-before.txt`).
  - `p5-mcp-config`, whose added `web` was listed 10 s after its add,
    right on the first probe's end.
- **Not `p5-mcp-verbs`.** It keeps the refused port, in its own copy of
  the old profile (`p5-mcp-verbs-profile.json`), and its comment says why.
  - The hang was tried there (`verbs-hang-trial.txt`). `/mcp login remote`
    then waits on the server instead of failing, so fx's `Couldn't sign in
    to remote` and xfx's `Interactive MCP authentication is unavailable
    here` are gone, and 43 captures change.
  - Every one of its pairs that shows a status word is already unresolved
    on both builds, for the `list` table's column widths. Only `startup`
    passes (`others-before.txt`).
- **`p5-mcp-config-vars` has no such server.** Its `remote` is skipped for
  an unset `${...}` and shows no status word (`others-before.txt`, `scan.py`).
- **A ledger cause was not needed**: the state now holds.

## After: 5 runs on each build (`after-runs.txt`, `after-verdicts.txt`, `after-words.txt`, `content-diff.txt`)

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
  scenarios differs except in the status word and the port, on either
  build (`content-diff.txt`).
- **Gates.** `test-sweep.sh` passes, 72 and 28 OK (`sweep-tests.txt`). The
  next CI run still selects 125 scenarios, the four of them as `changed`
  (`select-after.txt`), so the base also runs the fixed form.

## Also

- **The timeout estimate held.** CI's step took 5188 s: head 2813, retries
  520, base 1852 (`ci-report-excerpt.txt`). `../ci-timeout/` estimated
  about 5117 s for the step.
- **Open, for LOCAL-PATHS: a path xfx wraps behind its `↳` mark is not
  rewritten.** A trial with a 118-byte scenario directory wrapped
  `approve-confirm`'s command line. The published capture held the
  directory's first 116 bytes on one row and its last 2 on the next, and
  `roots.py --check` passed it (`gutter-check.txt`). roots.py reads
  U+2500-U+259F as gutters (`roots.py:46`), and `↳` is U+21B3. CI's
  scenario directory is short enough not to wrap, so no committed capture
  is known to hold one.

## Files

`repeat.sh` (N sweeps on each build; `SCENARIOS_DIR` for a trial copy) ·
`verdicts.py`, `words.py`, `timeline.py`, `scan.py`, `content_diff.py` (read
its out directories) · `code.sh` → `code.txt` · `publish_cost.sh` →
`publish-cost.txt` · `gutter_check.py` → `gutter-check.txt` ·
`before-runs.txt`, `before-verdicts.txt`, `before-words.txt`,
`before-timeline.txt` · `others-before.txt` · `verbs-hang-trial.txt` ·
`after-runs.txt`, `after-verdicts.txt`, `after-words.txt` ·
`content-diff.txt` · `sweep-tests.txt` · `select-after.txt` ·
`ci-report-excerpt.txt` · `run-seconds.txt`
