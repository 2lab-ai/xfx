# flake-mdtime2: the markdown linear-time check, second flake

`markdown::tests::code_tables::long_lines_of_unmatched_or_unbalanced_delimiters_render_in_linear_time`
failed again on CI, now on the arm64 macOS runner, eleven hours after
flake-mdtime's fix merged. That makes this a repeated failure of one check.
The cause is in the check, not in the renderer. The fix changes
`src/markdown/tests/mod.rs` only. Old line numbers are at `main` @
`d314549e`; new ones are at this branch's test commit.

## The failure

- PR #62's CI, job `gate (aarch64-apple-darwin)`, image `macos-15-arm64`
  (`20261104.0514.1`), step `cargo test --locked --all-targets`, 2026-11-06
  20:47Z: 3954 passed, 1 failed.
- It panicked at `code_tables.rs:382`, inside `assert_renders_in_linear_time`
  (`mod.rs:98`): `"https://example.com ""*".."\n": x16384 took 424.084µs and
  x65536 took 5.065292ms, 11.94 times as long; a linear render is 4 times and
  the limit is 8`.
- The other six readings in that run were 5.10, 6.39, 4.76, 7.86, 5.06 and
  3.91. Six of the seven are above 4, the value a linear render reads.

## Where the time goes (measured)

A scratch probe (`probe.rs`, here; never compiled into the crate) rendered
the failing shape and its neighbour `"text " "*".. "x"` on this machine (M5
Max, debug build, fault-injection off):

- **Per unit, the cost is flat.** Cheapest of 7 per size, one reused
  processor: 19.9, 18.4, 17.7, 17.6, 17.8 and 17.9 ns per `*` at x1024,
  x4096, x16384, x65536, x262144 and x1048576. There is no knee where the
  line leaves a cache level, and no fixed per-call cost large enough to
  matter at x16384.
- **Both halves of a push grow by 4.0.** Accumulating the line (the byte
  loop in `processor.rs:325-335`) costs 73 µs at x16384 and 290 µs at x65536;
  rendering it costs 217 µs and 865 µs.
- **Nothing carries over between pushes.** Over 30 steady-state pushes of
  each size: 6 and 1 minor page faults in all, so no allocation is faulted in
  per push. A fresh processor per push reads 294 µs and 1.20 ms, the same as a
  reused one (313 µs and 1.23 ms), and its ratio is 4.06.
- **The thread CPU clock** reads in nanoseconds (`424.084µs`), so its
  resolution is not a factor at 424 µs.

So the renderer and the sizes are linear here, and the drift is not in the
code. What moves is the machine's speed while the test runs.

## The cause

**The runner's speed changes by more than the check tolerates.** The old
check took the cheapest of three runs of each size and allowed a factor of
two either way around 4. On PR #62's runner, renders of equal cost read
twice apart within a second:

| shapes of equal cost here | here, short / long | PR #62's runner, short / long |
|---|---|---|
| `"text " "*".. "x"` | 291 µs / 1.17 ms | 866 µs / 3.39 ms |
| `"https://example.com " "*"..` | 291 µs / 1.16 ms | 424 µs / 5.07 ms |
| `"*a _b ~~c "` | 8.6 ms / 34.4 ms | 14.1 ms / 67.0 ms |
| `"a* b_ c~~ "` | 8.6 ms / 34.3 ms | 14.4 ms / 113.0 ms |

**Reproduced here.** Under `taskpolicy -b` (background QoS) one x16384 push
of the failing shape read anywhere from 311 µs to 1.06 ms, 3.4 times apart.
The old check, run 30 times that way (the logs' summary is in § Ratios),
failed 5 times, with the CI signature: the short side's cheapest run at or
near full speed (307 µs, 309 µs and 330 µs against 291 µs alone) and the long
side's 2.5 to 3.3 times its full-speed cost. Its readings spread from 0.38 to
2.50 times the linear 4.

**A floor under the short render would not have helped.** One of the 5
failures was `"*a https://example.com/b _c ~~d "`, with a 13.6 ms short
render and a ratio of 9.10. On CI the 7.86 had a 14.4 ms short render.
Scaling n until the short render clears 10 to 20 ms does not take the
spread out.

**Why unequal windows widen it.** The short line's window is a quarter of
the long line's. A short window can sit wholly inside a fast or a slow spell;
a long one averages more of them. So the short side's cheapest run swings
further than the long side's, and either side can be the unlucky one. That
the CI host moved the runner's virtual CPUs between fast and slow cores, or
took them away for a while, is a guess [가설]: the measured facts are the
2× spread on CI and the reproduction here.

## Fix

`assert_renders_in_linear_time` (`mod.rs:79-133`):

- **Equal windows.** It times one push of 16 copies of `line(n / 16)`
  against one push of `line(n)`. They carry the same bytes, so for a linear
  render the two windows are the same length and their cheapest runs come
  from the same spread. A linear render reads 1, a quadratic one 16.
- **Limit 4** (`mod.rs:39`), halfway between 1 and 16 on a log scale. Either
  reading can be off by a factor of **four** before the check is wrong; the
  old one allowed two.
- **Re-measure before failing** (`LINEAR_ROUNDS = 3`, `mod.rs:52`). A round
  is still the cheapest of 3 alternating runs of each input. A round at or
  over the limit is measured again, and the check fails only if all three
  rounds do. A passing run costs one round.
- Kept: the 10 s ceiling on every push, as a hang backstop only; the long
  size of every shape, so every comment about the inputs still holds; one
  line printed per round (`linear-time <shape>: round r, 16 x<n/16> <time>,
  x<n> <time>, ratio ...`).
- Each push must render as many lines as it was given (`mod.rs:109-113`), so
  a processor that held a line back could not shrink a window.
- Both callers go through the helper: all 13 shapes in `code_tables.rs` and
  the 64 KiB `&` line in `inline.rs:411`.

### Against both kinds of error

- **A linear render failing (noise).** One side's cheapest run has to be 4
  times the other's, in three rounds in a row. The widest reading measured
  with this check is 1.79 times linear, under background QoS (§ Ratios).
  The CI run that failed read 2.98 times linear, with the old, unequal
  windows.
- **A quadratic render passing.** The 5 mutants read 14.5 to 16.2 (§ RED),
  3.6 to 4.1 times the limit, in every round. Noise can lower a quadratic
  reading: it has to slow all three short runs of a round while it spares
  one long run. To pass, a mutant needs the short side's cheapest run
  slowed at least 3.6 times, in one of three rounds, while one long run, a
  window 16 times as long, runs at full speed. The widest swing of one push
  measured here is 3.4 times (one x16384 push under background QoS, 311 µs
  to 1.06 ms), and on CI 2.0 times between two cheapest-of-3 readings. So
  re-measuring gives a mutant three tries at a reading that has not been
  seen even once.
- **Each part is needed.** The same 30 background-QoS runs, three ways
  (same machine, same session, readings divided by the linear value):

  | check | windows | margin | rounds | fails | max | readings > 1.5× | > 2× |
  |---|---|---|---|---|---|---|---|
  | old (`d314549e`) | unequal, 1:4 | 2× | 1 | 5 / 30 | 2.50 | 16 of 395 | 5 |
  | equal windows only (growth 4, limit 2) | equal | 2× | 1 | 1 / 30 | 2.85 | 3 of 420 | 1 |
  | this check (growth 16, limit 4) | equal | 4× | up to 3 | 0 / 30 | 1.79 | 4 of 420 | 0 |

  Equal windows cut the upper tail (16 readings over 1.5× down to 3) but do
  not remove it: one equal-window reading still hit 2.85 (`"[^".. "]"`,
  1.18 ms against 3.37 ms) and failed a 2× margin. So the margin is 4×, and
  a round that still fails is measured again. The equal-windows-only
  variant ran on the throwaway clone, with three constants changed and
  nothing else: `LINEAR_GROWTH = 4`, `LINEAR_RATIO_LIMIT = 2.0`,
  `LINEAR_ROUNDS = 1`. It is not committed.
- **What else was weighed.** More runs of unequal windows keep the bias. A
  bigger limit alone eats the quadratic margin. A count of loop iterations
  would be exact, but it needs counters in `inline_render.rs`, outside a
  test-only change, and covers only the loops it is put in. A hardware
  instruction counter is not readable on every runner [추정].
- **Cost.** A round renders 2n per shape instead of 1.25n. The two tests take
  1.29 to 1.40 s alone here (50 runs), up from 0.70 s.

## RED

**The 5 mutants, against this check.** Each patch in
`../flake-mdtime/mutants/` removes one guard from
`src/markdown/inline_render.rs`. They ran on a throwaway
`git clone --no-local` checked out at this branch's test commit as first
written (`4f50cce5`; the amend that followed changed one doc comment),
never on the worktree. One test process at a time, 5 runs per mutant, 25 runs in
all, each under `timeout 900`. A memory cap was not possible: macOS rejects
`ulimit -v` and `ulimit -d` (`cannot modify limit: Invalid argument`).
The inputs are fixed strings, not an unbounded source, and
`/usr/bin/time -l` recorded each run's peak resident set, 80 to 248 MB.

| mutant | guard it removes | fails at | rounds (run 1) | all 15 rounds of 5 runs |
|---|---|---|---|---|
| `M1-cache-off` | failed-search cache (`inline_render.rs:514`) | residual closers, `code_tables.rs:359` | 14.97, 15.00, 15.21 | 14.80 to 15.27 |
| `M2-remainder-uncached` | partial closer caches the rest (`:510`) | residual closers, `:359` | 15.43, 15.00, 15.03 | 14.64 to 15.43 |
| `M3-bracket-rescan` | one shared `]` search (`:165`) | `"[".. "]"`, `:382` | 16.21, 16.19, 16.21 | 15.97 to 16.22 |
| `M4-entity-unbounded` | entity terminator window (`:947`) | `"&"..`, `inline.rs:411` | 16.06, 16.01, 15.97 | 15.97 to 16.24 |
| `M5-tombstones` | stack truncation (`:508`) | nested pairs, `code_tables.rs:350` | 14.58, 14.59, 14.60 | 14.50 to 14.81 |

- 25 of 25 runs failed, each on the ratio assertion after three rounds
  (`... in each of 3 rounds: ...; a linear render is about 1 times as long
  and the limit is 4`), never on the ceiling or the line-count check. In
  each run the other test passed, as it does not reach that guard.
- For example, M5: `nested pairs: 16 lines of x2000 against one of x32000,
  in each of 3 rounds: 203.410541ms and 2.966722584s, 14.58; ...`.
- Each mutant's reading holds round after round and run after run: across
  the 15 rounds of one mutant it moves by at most 0.8.
- Without a mutant the same clone's tests pass (the 30 background-QoS runs
  in § Ratios ran there).

**The old check flaking on input that reproduces the drift.** The failing
line, `"https://example.com " "*".. "\n"`, is linear here (§ Where the time
goes), so the drift is not in the input; it comes from a machine whose
speed moves. `taskpolicy -b` makes this one do that. Under it the old check
at `d314549e` failed 5 of 30 runs:

```
"*a""*".."x\n": x16384 took 308.792µs and x65536 took 2.840625ms, 9.20 times as long; a linear render is 4 times and the limit is 8
"&".."\n": x16384 took 1.5335ms and x65536 took 14.677833ms, 9.57 times as long; a linear render is 4 times and the limit is 8
"*a""*".."x\n": x16384 took 307µs and x65536 took 3.074833ms, 10.02 times as long; a linear render is 4 times and the limit is 8
"text ""*".."x\n": x16384 took 330.25µs and x65536 took 2.942791ms, 8.91 times as long; a linear render is 4 times and the limit is 8
"""*a https://example.com/b _c ~~d ".."\n": x5000 took 13.612625ms and x20000 took 123.848834ms, 9.10 times as long; a linear render is 4 times and the limit is 8
```

The new check, on the same 30-run loop, failed 0 times and never needed a
second round.

## Ratios

`ratios.py` (here) reads the `linear-time` lines from a set of logs, in
this check's form and in flake-mdtime's. A reading is one round's ratio;
`max/lin` divides it by what a linear render reads (1 here, 4 for the old
check), so the two checks compare on one scale. `min base` is the cheapest
short-side reading of the shape: here 16 short lines pushed at once.

All on this machine (M5 Max, 18 cores, debug build). Alone and under the
suite: the worktree at `783102b8`, fault-injection lib test binary. Under
background QoS: the throwaway clone, default lib test binary.

| run | runs | failed | largest `max/lin` | smallest `max/lin` | smallest base | second rounds |
|---|---|---|---|---|---|---|
| alone, `--test-threads=1` (1-min load 4.8 to 5.7) | 50 | 0 | 1.06 | 0.91 | 1.12 ms | 0 |
| full fault-injection lib suite (3985 tests, all threads; load 4.0 to 6.7) | 5 | 0 of 19925 | 1.07 | 0.85 | 1.15 ms | 0 |
| alone under `taskpolicy -b` | 30 | 0 | 1.79 | 0.60 | 1.24 ms | 0 |
| old check, alone under `taskpolicy -b` | 30 | 5 | 2.50 | 0.38 | 293 µs | (none) |

Each suite run started only after `pgrep -x cargo` and
`pgrep -x 'xfx-[0-9a-f]+'` both came back empty.

Largest reading per shape (`max/lin`), this check:

| shape | alone ×50 | suite ×5 | background QoS ×30 |
|---|---|---|---|
| nested pairs | 1.06 | 1.07 | 1.16 |
| residual closers | 1.05 | 1.03 | 1.34 |
| `"*a _b ~~c "` | 1.06 | 1.04 | 1.15 |
| `"a* b_ c~~ "` | 1.05 | 1.06 | 1.21 |
| `"*a https://example.com/b _c ~~d "` | 1.02 | 1.03 | 1.37 |
| `"text " "*".. "x"` | 1.05 | 1.03 | 1.63 |
| `"https://example.com " "*"..` (the CI failure) | 1.00 | 0.97 | 1.48 |
| `"*a" "*".. "x"` | 1.02 | 1.00 | 1.42 |
| `"["..` | 1.03 | 1.05 | 1.60 |
| `"!["..` | 1.01 | 1.01 | 1.79 |
| `"[".. "]"` | 1.02 | 1.03 | 1.44 |
| `"![".. "]"` | 1.03 | 1.03 | 1.36 |
| `"[^".. "]"` | 1.01 | 1.01 | 1.21 |
| `"&"..` (entity test) | 1.01 | 1.02 | 1.49 |

- **Linear side.** The largest of 1190 first-round readings with this check
  is 1.79, under background QoS: the limit 4 is 2.2 times that. Alone and
  under the suite it is 1.07, 3.7 times under. No run needed a second round.
- **Quadratic side.** The smallest mutant reading is 14.50 (M5), 3.6 times
  the limit.
- **Smallest base.** The cheapest short side of any shape is now about
  1.1 ms (the 16 lines of `"[^".. "]"`), where the old check's was 281 µs
  alone and 424 µs on the failing CI run.
- **Not measured here: the GitHub runners.** This branch's own CI run is
  the first reading there. A passing test's output is not shown on CI, so
  only a failure would print its rounds.

## Gates

All at `6b6b9cdf` (the test change and this receipt), with a short
`TMPDIR`, one step at a time. Each heavy step started only after
`pgrep -x cargo` and `pgrep -x 'xfx-[0-9a-f]+'` both came back empty.

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| `cargo clippy --locked --all-targets --features fault-injection -- -D warnings` | ok |
| `cargo test --locked --no-fail-fast` | 4865 passed, 0 failed, 7 ignored (14 targets) |
| `cargo test --locked --features fault-injection --no-fail-fast` | 4894 passed, 0 failed, 7 ignored (14 targets) |
| `scripts/check-xfx-identity.sh` · `check-no-stubs.sh` · `check-no-secrets.sh` | ok · ok · ok |
| `cargo build --locked --release` and `--features fault-injection --target-dir target/faulty` | ok, ok |
| `scripts/smoke.sh target/release/xfx` | 47 checks, 0 failures |
| `scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx` | 37 scenarios + the oracle, 1111 checks, 0 failures, no traceback in any scenario log |

## Re-run

```
cargo test --locked --lib -- --show-output \
  markdown::tests::code_tables::long_lines_of_unmatched_or_unbalanced_delimiters_render_in_linear_time \
  markdown::tests::inline::entity_lookup_is_bounded_and_a_long_ampersand_line_renders_in_bounded_time \
  > run.log 2>&1
python3 .prd/tui-phase5/receipts/flake-mdtime2/ratios.py run.log
# The noise this check is built for, on macOS:
taskpolicy -b target/debug/deps/xfx-<hash> --exact --test-threads=1 --show-output <the two tests>
# RED, on a throwaway clone only:
git clone --no-local <repo> <clone>
git -C <clone> apply .prd/tui-phase5/receipts/flake-mdtime/mutants/M1-cache-off.patch
```
