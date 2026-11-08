---
title: CI-X86-TIME receipt
type: receipt
tags: [xfx, ci, phase5]
---

# CI-X86-TIME: the x86_64 macOS gate row against its 60-minute timeout

Branch `ci/p5-x86-time` on main `f096c163`. What it changes, net of main, is
`.github/workflows/ci.yml` only: the x86_64 macOS row of the gate job runs as
two jobs, `checks` and `release`. Every step's `run:` text and the steps'
order are main's.

| commit | what |
|---|---|
| `29aeb854` | round 1: built the fault-injection binary in the shared target dir (**reverted**) |
| `d2ef642a` | the split: a `part` matrix key, 14 `if:` guards, 2 failure-upload guards, the job name |
| `5709934e` | round 1 receipts |
| `8693d1b4` | round 2: reverts `29aeb854`; the split is reapplied to main's step text |
| this commit | round 2 receipts |

## Round 2: the reviews and what changed

| review item | done | evidence |
|---|---|---|
| gpt6 MUST: the restoration smoke drove a fault-injection binary 46 bytes different from the old one | `8693d1b4` reverts the dedupe (dispatcher's decision). The faulty build is again `--target-dir target/faulty`, after the release build and smoke, and smoke-tui takes `target/faulty/release/xfx`, as on main. CONTRIBUTING.md is main's again | `git diff f096c163 -- CONTRIBUTING.md` is empty; sha-proof row 4 |
| opus MUST: "actionlint passes" had no output on file | actionlint 1.7.12 run on the final `ci.yml` and on every workflow | `actionlint.txt` (both exit 0) |
| opus SHOULD a: the debug `--lib` tests check-usage-clock.sh runs are compiled fresh in the `release` part | rows 2c and 4c. That binary is never the same bytes twice: its object-file names carry a per-build token. 2c differs only in those tokens and the code signature, exactly as two identical fresh builds do (4c) | `sha-proof.txt` rows 2c, 4c |
| opus SHOULD b: name the host; there is no x86 "before" digest | both said below | `sha-proof.txt` line 2 |
| gpt6 NTH: "the other rows do not change" overstated | now "partition membership unchanged" | below |
| gpt6 MUST (plausible): a renamed required check | the dispatcher's check: main has no protection, rulesets or branch rules | `branch-protection.txt` |

## Where the row's time went

`gate (x86_64-apple-darwin)` on PR #82, job 128747443532, 53.1 minutes. Step
times come from the jobs API (`ci-82-x86-steps.txt`). Crate counts per step
come from the job log (`ci-82-x86-compile-counts.txt`), and the compile timeline
is in `ci-82-x86-compile-lines.txt`. The job's other runs: #78 41.5, #79 44.7,
#80 47.5, #83 48.4 minutes.

| step | min | what it compiles |
|---|---:|---|
| Lints | 3.2 | check-mode deps (rmeta) + xfx, default |
| Tests | 9.0 | 5.0 compile (121 crates, debug, test mode) + 4.0 run |
| Tests, with the fault injector | 9.2 | xfx only (3x): clippy 1.2, `--test tui` 1.1, `--lib` 2.7; runs 0.8 + 3.4 |
| No credentials in tracked files | 2.6 | nothing (the same step takes 41 s on arm64 macOS and 6 s on Linux) |
| The output check still fits inside a frame's budget | 17.5 | release test-mode deps + xfx `--lib` tests default (10.1 incl. deps) + fault-injection (7.4) |
| A release build ignores the pinned usage clock | 0.0 | nothing: reuses the debug and both release `--lib` test builds |
| Release build | 2.8 | rustix + xfx (2 crates) |
| Build the binary the restoration matrix drives | 5.0 | 137 crates: every release dep again in `target/faulty` (2.5), then xfx (2.5) |
| Smoke the TUI on a real terminal | 1.6 | nothing |

The only compilation repeated in one job is the fault-injection build's
dependencies: 137 crates that `target/release` already holds, rebuilt in its
own `--target-dir`. Round 1 removed it. Round 2 restores it, because the
fault-injection binary's bytes depend on that directory's path (round 1,
below). Nothing else repeats:
- the output-cost step's two release builds are two feature sets, and
  check-usage-clock.sh already reuses both;
- rustix is built twice per profile because of a dev-dependency feature
  (`pty`), so the shipped binary's rustix is a different unit;
- clippy's check-mode rmeta shares no unit with test builds;
- the debug default and fault-injection builds are different feature sets.

So one job stays at about 53 of its 60 minutes, and xfx's own compile (about
31 of them, an estimate: the output-cost step hides its compile lines) grows
with every train. The split is what changes that.

## The plan, step by step (x86_64 macOS row; minutes from #82, after = expected)

| step | before (1 job) | `checks` | `release` |
|---|---:|---:|---:|
| set up, checkout, assert, toolchain, target | 1.0 | 1.0 | 1.0 |
| Formatting | 0.1 | 0.1 | |
| Lints | 3.2 | 3.2 | |
| Tests | 9.0 | 9.0 | |
| Tests, with the fault injector | 9.2 | 9.2 | |
| No stubs | 0.7 | 0.7 | |
| No credentials | 2.6 | 2.6 | |
| identity, preview contract | 0.2 | 0.2 | |
| output cost | 17.5 | | 17.5 |
| usage clock | 0.0 | | ~4 (estimate: its debug `--lib` build, which the Tests step used to supply; fault-injection `--lib` compiled in 2.7, deps ~1) |
| Release build | 2.8 | | 2.8 |
| Smoke the release binary | 0.1 | | 0.1 |
| fault-injection build | 5.0 | | 5.0 |
| Smoke the TUI | 1.6 | | 1.6 |
| **job** | **53.1** | **~26** | **~32** |

Partition membership is unchanged on every row. Each of the other three rows
runs the same steps as before, in the same order, with the same `run:` text.
Their steps now carry `if: matrix.part != …` guards, which are true on a row
with no `part`. On the x86 row, the four setup steps run in both parts, and
the other 14 steps run in exactly one part. The two `script_tests` steps run
on neither part, as they ran on no x86 job before. `partition.txt` lists the
steps for each row and part.

## The same bytes

The `release` part builds everything it runs in a fresh `target/`, with none
of the `checks` part's debug steps before it. `sha-proof.sh` runs right after
the local gate. It moves the gate's `target/` aside, rebuilds a fresh
`target/` in the `release` part's order, and compares each binary that a
release-part check drives (`sha-proof.txt`). It compares at `target/`, the
path CI uses, because Mach-O binaries carry a linker UUID that follows the
target directory's path.

| # | binary (the check that drives it) | one-job order vs `release` part order |
|---|---|---|
| 2a | release lib tests, default (output cost; usage clock's release run) | same sha256 |
| 2b | release lib tests, fault-injection (output cost; usage clock's run) | same sha256 |
| 2c | debug lib tests, default (usage clock's debug run): built by `cargo test --all-targets` vs by `cargo test --lib` alone | same except per-build tokens: see below |
| 3 | `target/release/xfx` (smoke, smoke-tui) | same sha256 |
| 4 | `target/faulty/release/xfx` (smoke-tui `--faulty`) | same sha256 |
| 4c | control: debug lib tests built twice, fresh, same command, same path | same except per-build tokens, as 2c |

**Row 2c, exactly what differs.** The debug lib test binary is never the same
bytes twice. Its debug map (the unstripped symbol table) names each of its
256 object files `xfx-<hash>.<cgu>.<token>.rcgu.o`, and the 7-character
`<token>` is new in every build. Two fresh builds made the same way, at the
same path (row 4c), differ in exactly that. `objname-diff.py` counts every
differing byte of 2c and 4c in three groups:
- the code signature, which hashes the pages holding those names;
- those 256 names, which differ only in `<token>`;
- anywhere else: code, data, other strings. That group is 0 in both pairs.

So in the old single job too, this binary's bytes changed on every run. Its
code and data are the same in the `release` part as after
`cargo test --all-targets`.

- **Host:** these digests come from aarch64-apple-darwin, which is this
  machine. CI's x86_64-apple-darwin row builds the same Cargo units on a
  different machine, so the digests differ there.
- **No x86 "before":** neither old job printed a digest, so there is no
  x86_64 "before" digest to compare with. What carries over is the argument:
  the commands, their target directory and their predecessors' profile
  directories are as above.
- **The `checks` part:** it runs the old job's first steps, unchanged and in
  the same order, so its binaries are built exactly as before.

## Cache: not adopted

A dependency cache (actions/cache or Swatinem/rust-cache) would save only the
dependency compiles: about 1.5 min check-mode (13:38:55 to 13:40:22), about
1 min debug test-mode (13:42:03 to 13:43:04) and about 2.5 min release
test-mode. That is about 5 min gross per job. Restoring and saving a `target/`
that holds three profiles' dependencies costs part of that back on macOS. A
cache cannot help with xfx's own ~31 minutes, which change with every commit.
The split already leaves each part about 30 minutes of headroom.

If one is ever added:
- **Key:** OS, target, the full `rustc -vV` output (the toolchain step
  installs a floating `stable`), and `hashFiles('Cargo.lock')`.
- **Contents:** the registry and dependency artifacts only. The workspace
  crate is never cached (rust-cache cleans it by default), so every check
  still compiles and tests xfx from the checkout.
- **Why a stale entry cannot pass a broken build:** Cargo fingerprints every
  dependency unit by its package id (name, version and source, whose checksum
  Cargo.lock pins), its features, profile, target and rustc version, and the
  fingerprints of its own dependencies. An artifact restored for anything
  else is rebuilt, not reused.
- **What a fingerprint does not cover:** a poisoned artifact with a matching
  fingerprint. A pull request's run reads the caches the default branch wrote,
  and its own; it cannot write the default branch's. So a cache adds the
  default branch's runs, and the cache action itself, to what the gate trusts.

## The renamed x86 job contexts

Two new names, `gate (x86_64-apple-darwin, checks)` and
`gate (x86_64-apple-darwin, release)`, replace `gate (x86_64-apple-darwin)`.
The other three names do not change. Nothing in the tree reads a job name:
there is no `needs:`, no `workflow_run` and no name lookup in any workflow or
script. Checked by the dispatcher in round 2: main has no branch protection,
no rulesets and no branch rules (`gh api`: 404, `[]`, `[]`). So the new names
need no migration. `branch-protection.txt` is the same read, made from this
worktree.

## Local gate (round 2)

Head `8693d1b4`, every step of `ci.yml`'s gate job in file order (a row
with no `part`; the x86 parts run its first 12 and its last 10 steps, the
four setup steps in both). Run under the machine's suite lock from an empty
`target/`, with `TMPDIR`, `RUNNER_TEMP` and `COMMIT_UNDER_TEST` set as CI
sets them. `gate.txt` is the log, filtered to step, result, check and compile
lines.

| step | name | status | s |
|---|---|---:|---:|
| 01 | Confirm the checkout is the commit under test | 0 | 0 |
| 02 | Confirm the runner really is this target | 0 | 0 |
| 03 | Formatting | 0 | 1 |
| 04 | Lints | 0 | 27 |
| 05 | Tests | 0 | 130 |
| 06 | Tests, with the fault injector | 0 | 111 |
| 07 | No stubs, and the parity ledger reconciles | 0 | 8 |
| 08 | No credentials in tracked files | 0 | 31 |
| 09 | No gate takes its options, repository or config from the environment | 0 | 190 |
| 10 | One product name in tracked files | 0 | 2 |
| 11 | The preview publication contract holds | 0 | 5 |
| 12 | The preview contract catches every mutant it exists for | 0 | 53 |
| 13 | The output check still fits inside a frame's budget | 0 | 79 |
| 14 | A release build ignores the pinned usage clock | 0 | 0 |
| 15 | Release build | 0 | 14 |
| 16 | Smoke the release binary | 0 | 2 |
| 17 | Build the binary the restoration matrix drives | 0 | 22 |
| 18 | Smoke the TUI on a real terminal | 0 | 64 |

- `test result` lines: 10032 passed, 0 failed. The lib suites ran 4449
  (default) and 4472 (fault-injection).
- `smoke.sh`: 47 checks, 0 failures. `smoke-tui`: 37 scenarios plus the
  oracle, 1168 checks, 0 failures, driving `target/release/xfx` and
  `target/faulty/release/xfx`.
- `test-gate-env`: ok (62 cases). `test-preview-contract`: ok (24 mutants).
- Step 17, the fault-injection build in its own `--target-dir`, compiled 137
  crates, as on #82.
- `actionlint.txt`: actionlint 1.7.12 (the version `preview.yml` pins), exit
  0 on `ci.yml` and exit 0 on every workflow, at the `ci.yml` whose sha256 it
  records.

## Round 1 (superseded by round 2)

`29aeb854` built the fault-injection binary in the shared `target/` first,
copied it to `target/faulty/xfx` and then ran the plain release build. That
saved the 137-crate rebuild (2.5 min). `sha-proof-r1.txt` (from
`sha-proof-r1.sh` and `locate-diff.py`) found two things:
- the release binary and both release lib test binaries had the same bytes
  in either order;
- the fault-injection binary differed from the old one in 46 bytes, all
  inside `LC_UUID` and the code signature, which follow the target
  directory's path.

Under the exact-bytes bar that difference counts, and the dedupe was worth
about 2.5 minutes against the split's ~25, so round 2 reverts it.
`gate-r1.txt` is round 1's local gate at `d2ef642a` (18 of 18 steps passed).

## Files

- `extract-steps.rb` writes each `run:` step of `ci.yml`'s gate job to its
  own file, in order. It skips `uses:` steps and the toolchain install, which
  would change this machine's default toolchain.
- `run-steps.sh` runs each of those files under `bash -e`, as a GitHub run
  step does.
- `partition.rb` writes `partition.txt`.
- `sha-proof.sh` writes `sha-proof.txt` and uses `objname-diff.py`.
- `actionlint.txt` and `branch-protection.txt` are command output.
- `ci-82-x86-*.txt` come from GitHub's jobs API and the job log of #82.
- Round 1: `sha-proof-r1.sh` writes `sha-proof-r1.txt` and uses
  `locate-diff.py`; `gate-r1.txt` is that round's gate log.
