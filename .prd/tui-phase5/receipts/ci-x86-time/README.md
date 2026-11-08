---
title: CI-X86-TIME receipt
type: receipt
tags: [xfx, ci, phase5]
---

# CI-X86-TIME: the x86_64 macOS gate row against its 60-minute timeout

Branch `ci/p5-x86-time` on main `f096c163`, two commits:

- `29aeb854` builds the fault-injection binary in the shared target directory
  (the one duplicated build the row had).
- `d2ef642a` splits the x86_64 macOS row into two jobs, `checks` and
  `release` (the rest of the time cannot be shared in one job).

Each commit can be pushed alone; the second one sits on top of the first one.

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
| Build the binary the restoration matrix drives | 5.0 | **137 crates**: every release dep again (2.5), then xfx (2.5) |
| Smoke the TUI on a real terminal | 1.6 | nothing |

## Every cargo invocation, and which ones share work

| step | invocation | profile | features | units | target dir |
|---|---|---|---|---|---|
| Lints | `clippy --all-targets` | dev, check | default | rmeta of lib, bin, tests | `target/debug` |
| Tests | `test --all-targets` | dev/test | default | lib, bin, lib tests, bin tests, integration tests | `target/debug` |
| FI | `clippy --all-targets --features fault-injection` | dev, check | fi | rmeta (xfx only, deps reused) | `target/debug` |
| FI | `test --features fault-injection --test tui` | test | fi | lib, bin, `tui` | `target/debug` |
| FI | `test --lib --features fault-injection` | test | fi | lib tests | `target/debug` |
| output cost | `test --release --lib` and `... --features fault-injection` | release | default, fi | lib tests | `target/release` |
| usage clock | `test --lib`, `--release --lib`, `--release --lib --features fault-injection` | | | all three reuse the builds above | |
| Release build | `build --release` | release | default | lib, bin | `target/release` |
| faulty build (before) | `build --release --features fault-injection --target-dir target/faulty` | release | fi | **all deps**, lib, bin | `target/faulty/release` |

What was duplicated, and what only looks duplicated:

1. **Duplicated: the faulty build's dependencies.** `fault-injection = []`
   turns on no dependency feature, so its release dependencies are the units
   the output-cost step had already compiled into `target/release`. Its own
   `--target-dir` compiled all 137 again (14:24:06 to 14:26:35, 2.5 min).
   Fixed in `29aeb854`.
2. **Not duplicated: the output-cost step's two release builds.** They are
   two feature sets, so they are two different test binaries.
   check-usage-clock.sh already reuses both (the step takes 2 s). Dropping one
   would change the check's scope.
3. **Not duplicated: rustix, compiled twice per profile.** The lib tests turn
   on rustix's dev-dependency `pty` feature. Resolver 2 keeps dev-dependency
   features out of a non-test build, so the shipped binary's rustix is a
   different unit. Unifying them would change the shipped bytes.
4. **Not duplicated: clippy against test.** Clippy produces check-mode rmeta
   and test produces machine code, so no unit is shared.
5. **Not duplicated: the debug default against the debug fault-injection
   builds.** They are different feature sets; only the dependencies are
   shared, and Cargo shares those already.

So one job cannot get much under ~50 minutes of the 60. xfx's own compile is
about 31 of them, and it grows with every train. (That is an estimate: the
output-cost step hides its compile lines, so its dependency share is taken as
the 2.5 minutes the old faulty build spent on the same units.) That is why
`d2ef642a` splits the row.

## The plan, step by step (x86_64 macOS row; minutes from #82, after = expected)

| step | before (1 job) | after 29aeb854 (1 job) | after d2ef642a: `checks` | after d2ef642a: `release` |
|---|---:|---:|---:|---:|
| set up, checkout, assert, toolchain, target | 1.0 | 1.0 | 1.0 | 1.0 |
| Formatting | 0.1 | 0.1 | 0.1 | |
| Lints | 3.2 | 3.2 | 3.2 | |
| Tests | 9.0 | 9.0 | 9.0 | |
| Tests, with the fault injector | 9.2 | 9.2 | 9.2 | |
| No stubs | 0.7 | 0.7 | 0.7 | |
| No credentials | 2.6 | 2.6 | 2.6 | |
| identity, preview contract | 0.2 | 0.2 | 0.2 | |
| output cost | 17.5 | 17.5 | | 17.5 |
| usage clock | 0.0 | 0.0 | | ~4 (its debug `--lib` build, [estimate]: fault-injection `--lib` compiled in 2.7, deps ~1) |
| faulty build | 5.0 | ~2.5 (xfx + rustix; 137 → 2 crates) | | ~2.5 |
| Release build | 2.8 | ~2.7 (rustix moved to the step above) | | ~2.7 |
| smoke, smoke-tui | 1.7 | 1.7 | | 1.7 |
| **job** | **53.1** | **~50.6** | **~26** | **~29.5** |

The fault-injection binary's 2.5 minutes are measured: its xfx compile ran
14:26:18 to 14:29:08, while its 135 other crates finished by 14:26:35. The
usage-clock figure is an estimate; the dispatcher's CI push measures it.

The other three rows do not change. Rows without `part` run every step in one
job, and `partition.txt` lists the steps each row and part runs.

## The same bytes

`sha-proof.sh` runs right after the local gate and writes `sha-proof.txt`.
Mach-O binaries carry a linker UUID that changes with the target directory's
path, so two builds can only match byte for byte at the same path. Every
comparison is therefore made at `target/`, the path CI uses, except row 5,
which cannot be.

| # | binary | compared | result |
|---|---|---|---|
| 2a | release lib tests, default (output cost, usage clock) | built after every debug step (one job) vs with none before it (the `release` part) | same sha256 |
| 2b | release lib tests, fault-injection | same as 2a | same sha256 |
| 3 | `target/release/xfx` (smoke, smoke-tui) | built right after the fault-injection build (new order) vs with none before it (old order) | same sha256 |
| 4 | `target/faulty/xfx` | the gate's copy vs the new-way build redone in a fresh `target/` | same sha256 |
| 5 | fault-injection binary | old `--target-dir target/faulty` vs new shared dir | 46 bytes differ, all inside `LC_UUID` (bytes 1945-1960) and the code signature; 0 bytes outside them, so the code, data and strings are the same |
| 6 | `target/release/xfx`, re-run | a second plain release build after a fault-injection build, with every unit fresh | same sha256: Cargo re-links a fresh unit, so CONTRIBUTING's local order is safe to run twice |

On CI, every run checks it as well. The Release build step fails if
`target/release/xfx` has the fault-injection copy's bytes (`cmp`), and it
prints both digests. Smoke the TUI prints them again before it drives them.
In the local gate both steps printed `1764…6dba` (release) and `1e8a…7f06`
(fault-injection), the same digests as rows 3 and 4.

## Cache: not adopted

A dependency cache (actions/cache or Swatinem/rust-cache) would save the
dependency compiles only: ~1.5 min check-mode (13:38:55 to 13:40:22), ~1 min
debug test mode (13:42:03 to 13:43:04), and ~2.5 min release test mode. That
is ~5 min gross per single job. Restoring and saving a `target/` of three
profiles' dependencies on macOS costs part of that back. It cannot help with
xfx's own ~31 minutes, which change with every commit, and the split already
leaves each part ~30 minutes of headroom.

If one is ever added, the key and the safety argument are:

- **Key:** OS, target, the full `rustc -vV` output (the toolchain step
  installs a floating `stable`), and `hashFiles('Cargo.lock')`.
- **Contents:** registry and dependency artifacts only. The workspace crate is
  never cached (rust-cache's default cleans it), so every check still compiles
  and tests xfx from the checkout.
- **Why a stale entry cannot pass a broken build:** Cargo fingerprints every
  dependency unit by package id (name, version and source, whose checksum
  Cargo.lock pins), features, profile, target and rustc version, and by the
  fingerprints of its own dependencies. An artifact restored for anything
  else is rebuilt, not reused.
- **What a fingerprint does not cover:** a poisoned artifact with a matching
  fingerprint. A pull request's run reads the caches the default branch wrote
  (and its own); it cannot write the default branch's. So the cache adds the
  default branch's runs, and the cache action itself, to what the gate
  trusts.

## Local gate

Head `d2ef642a`, every step of `ci.yml`'s gate job in file order (a row with
no `part`, which is also the order of `29aeb854`'s single job). It ran under
the machine's suite lock, with `TMPDIR`, `RUNNER_TEMP` and
`COMMIT_UNDER_TEST` set as CI sets them. `gate.txt` is the log, filtered to
step, result, check and compile lines.

| step | name | status | s |
|---|---|---:|---:|
| 01 | Confirm the checkout is the commit under test | 0 | 0 |
| 02 | Confirm the runner really is this target | 0 | 0 |
| 03 | Formatting | 0 | 1 |
| 04 | Lints | 0 | 23 |
| 05 | Tests | 0 | 126 |
| 06 | Tests, with the fault injector | 0 | 108 |
| 07 | No stubs, and the parity ledger reconciles | 0 | 9 |
| 08 | No credentials in tracked files | 0 | 30 |
| 09 | No gate takes its options, repository or config from the environment | 0 | 248 |
| 10 | One product name in tracked files | 0 | 1 |
| 11 | The preview publication contract holds | 0 | 3 |
| 12 | The preview contract catches every mutant it exists for | 0 | 62 |
| 13 | The output check still fits inside a frame's budget | 0 | 88 |
| 14 | A release build ignores the pinned usage clock | 0 | 1 |
| 15 | Build the binary the restoration matrix drives | 0 | 15 |
| 16 | Release build | 0 | 14 |
| 17 | Smoke the release binary | 0 | 2 |
| 18 | Smoke the TUI on a real terminal | 0 | 63 |

- `test result` lines: 10032 passed, 0 failed. The lib suites ran 4449
  (default) and 4472 (fault-injection).
- `smoke.sh`: 47 checks, 0 failures. `smoke-tui`: 37 scenarios plus the
  oracle, 1168 checks, 0 failures.
- `test-gate-env`: ok (62 cases). `test-preview-contract`: ok (24 mutants).
- `actionlint` 1.7.12, the version `preview.yml` pins, passes `ci.yml`.
- Step 15 compiled 2 crates (rustix, xfx). The old-way build in
  `sha-proof.txt` section 5 compiled 137, the same count as CI's old step on
  #82.

Files: `extract-steps.rb` writes each `run:` step of `ci.yml`'s gate job to a
file, in order. It skips `uses:` steps and the toolchain install, which would
change this machine's default toolchain. `run-steps.sh` runs each file under
`bash -e`, as a GitHub run step does. `gate.txt` is the redacted log.
`partition.rb` produces `partition.txt`.
