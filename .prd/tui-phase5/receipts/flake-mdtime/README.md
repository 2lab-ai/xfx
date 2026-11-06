# flake-mdtime: the markdown linear-time tests

`markdown::tests::code_tables::long_lines_of_unmatched_or_unbalanced_delimiters_render_in_linear_time`
failed on the macOS Intel CI runner. The cause is in the test. It held a
linear render to a fixed CPU budget, and that budget measures the machine as
well as the algorithm. The fix changes `src/markdown/tests/` only. Line
numbers are at `main` @ `0b16d7e5`.

## The failure

- PR #57's CI, job `gate (x86_64-apple-darwin)` on GitHub's `macos-15-intel`,
  step `cargo test --locked --all-targets`, 2026-11-06 17:43Z: 3964 passed,
  1 failed.
- It panicked at `src/markdown/tests/code_tables.rs:400` with
  `"""a* b_ c~~ "x40000 took 568.326139ms`. The limit was 500 ms of the
  thread's CPU time (`:350`).

## The two facts that disagreed

- **The claim.** The test proves the inline renderer is linear, not
  quadratic (`code_tables.rs:368`, `:379-380`). The guards it is about are in
  `src/markdown/inline_render.rs`: the failed-search cache
  (`openers_bottom`, `:435-443`, `:510-515`), the stack truncation (`:502-509`),
  and the shared `]` search (`:148-166`).
- **The check.** Each line had to render in under 500 ms of thread CPU time
  (`:377`, `:389`, `:400`). The line that failed costs 34 ms here (M5 Max,
  debug build) and cost 568 ms on that runner: 16.5 times as much, with the
  same code. Reading the thread's CPU clock instead of the wall clock
  (`79f24439`) took other processes' turns out of the reading. A slower core,
  or one shared with another hardware thread, is still in it.
- **The algorithm is linear on that line.** At 10,000 and 40,000 units it
  costs 4.0 times as much (the tables below). A quadratic walk would cost 16
  times as much.

## Fix

- `assert_renders_in_linear_time` (`src/markdown/tests/mod.rs`) renders
  `line(n / 4)` and `line(n)` three times each, alternating, on one processor.
  It keeps the cheapest run of each size, since noise only adds time, and
  asserts that the long line costs less than **8** times the short one. A
  linear render costs 4 times as much and a quadratic one 16 times. 8 is
  halfway between them on a log scale, so either side can be off by a factor
  of two before the check reads it wrong.
- The long size is the old test's size, so every input the comments talk
  about is unchanged. Each push has a **10 s** ceiling, a backstop against a
  hang only. Each push must also end in `\n`, which shows the push rendered
  its own line and did not hold it back for the next push to pay for.
- Every shape the test checks goes through the helper: the nested pairs, the
  residual closers, and all 11 entries of `shapes`. The correctness checks
  (`starts_with` for the nested pairs, `contains` for the residual closers)
  read the long line's output, as before.
- **The next break.** `inline.rs:398`,
  `entity_lookup_is_bounded_and_a_long_ampersand_line_renders_in_bounded_time`,
  held 64 KiB of `&` to the same fixed 500 ms (`:413`). It has the same flaw,
  so it now uses the same helper.
- Every reading is printed as a `linear-time <shape>: ...` line, which
  `--show-output` shows, and a failure names the shape, both sizes, both
  times, the ratio and the limit.

## RED

Each mutant removes one guard. They ran only on a throwaway clone
(`git clone --no-local`), never on this worktree, one test process at a time
under `timeout 300`. The patches are in `mutants/`. With each mutant the other
212 `markdown::` tests still pass, so a mutant changes what a line costs and
not what it renders.

Old test = the tests at `0b16d7e5` (500 ms). New test = this branch.

| mutant | guard it removes | old test | new test |
|---|---|---|---|
| `M1-cache-off` | the failed-search cache, `*bottom = top.seq` (`inline_render.rs:514`) | fails: `residual closers took 4.30030375s` | fails at the residual closers: x8000 284 ms, x32000 4.59 s, ratio **16.20** |
| `M2-remainder-uncached` | a closer that matched part of its run caches the failed rest (`:510`) | fails: `residual closers took 4.370746791s` | fails at the residual closers: 282 ms, 4.61 s, **16.36** |
| `M3-bracket-rescan` | one `]` search shared by a run of `[` (`:165`) | fails: `"""["x65536 took 5.350289792s` | fails at `[`..`]`: x16384 337 ms, x65536 5.48 s, **16.29** |
| `M4-entity-unbounded` | the entity terminator window (`:947`) | the code_tables test passes; the entity test fails at `inline.rs:413` with `5.413813584s` | fails at `&`: x16384 333 ms, x65536 5.38 s, **16.17** |
| `M5-tombstones` | the stack truncation: consumed entries stay and are walked again (`:508`) | fails: `nested pairs took 3.1559785s` | fails at the nested pairs: x8000 197 ms, x32000 3.15 s, **16.01** |

Without a mutant, both new tests pass in the same clone. Every new-test
failure above is the ratio assertion, reported at the calling line
(`#[track_caller]`), not the 10 s ceiling and not a correctness check.

A test stops at its first failure, so a scratch probe (never committed)
rendered every shape once at n/4, n/2 and n under each mutant. The shapes a
mutant reaches read 15.70 to 16.51 at x4: M1 and M2 the residual closers, M3
`[`..`]` and `![`..`]`, M4 `&`, M5 the nested pairs. Every other shape read
3.07 to 5.22, from single runs with no minimum taken. The line that failed
on CI, `"a* b_ c~~ "`, is not reached by any of the five; like several other
shapes, it guards against an algorithm the code does not use today, and the
ratio check covers it the same way.

## Ratios

`ratios.py` (here) reads the `linear-time` lines from a set of logs.

All on this machine (M5 Max, 18 cores), using the fault-injection lib
test binary in a debug build. Each reading is one test run's cheapest-of-3
ratio, so 50 runs give 50 readings per shape.

- **Alone**: the two tests, `--test-threads=1`, 50 runs one at a time. 50/50
  passed. The one-minute load average, from other workers, was 6.5 to 7.9.
- **Under the full fault-injection lib suite** (3877 tests, all threads): 33
  runs, one at a time, 33/33 passed with 0 failures among the 3877. The plan
  was 50. The dispatcher stopped the loop after 28 runs, because a second
  worker's 50-run suite loop was running at the same time and load reached
  about 20. The other 5 ran only while that worker had no test running. The
  one-minute load average was 9 to 30.

| shape | alone: median | alone: max | suite: median | suite: max |
|---|---|---|---|---|
| nested pairs | 3.91 | 4.14 | 4.08 | 4.30 |
| residual closers | 3.98 | 4.20 | 3.84 | 4.21 |
| `"*a _b ~~c "` | 3.99 | 4.21 | 3.84 | 4.00 |
| `"a* b_ c~~ "` (the CI failure) | 3.99 | 4.25 | 3.83 | 4.06 |
| `"*a https://example.com/b _c ~~d "` | 3.99 | 4.21 | 3.98 | 4.14 |
| `"text " "*".. "x"` | 3.97 | 4.18 | 3.99 | 4.27 |
| `"https://example.com " "*"..` | 3.98 | 4.14 | 3.97 | 4.15 |
| `"*a" "*".. "x"` | 3.98 | 4.16 | 3.98 | 4.15 |
| `"["..` | 4.01 | 4.18 | 4.01 | 4.45 |
| `"!["..` | 4.00 | 4.21 | 3.99 | 4.12 |
| `"[".. "]"` | 4.00 | 4.41 | 4.01 | 4.38 |
| `"![".. "]"` | 4.00 | 4.14 | 4.00 | 4.29 |
| `"[^".. "]"` | 3.99 | 4.21 | 4.03 | 4.89 |
| `"&"..` (entity test) | 4.01 | 4.38 | 4.01 | 4.20 |

## Margins

- **Linear side.** The largest of 1162 readings (83 runs x 14 shapes) is
  4.89, `"[^".. "]"` under the suite. The limit 8 is 1.64 times that. The
  largest reading alone is 4.41, 1.81 times under the limit.
- **Quadratic side.** The smallest mutant reading in the new test is 16.01
  (M5). That is 2.0 times the limit. The smallest in the single-run probe is
  15.70, 1.96 times.
- **Machine speed** drops out of the ratio: both sizes run on one thread, one
  after the other. What can still move it is speed that changes between the
  two sizes, for example a hardware-thread sibling that is busy only during
  the long runs. Taking the cheapest of three alternating runs handles short
  bursts, and the limit allows either reading to be off by a factor of two.
- **Backstop.** The slowest cheapest-run long line was 64 ms, under the
  suite. At the 16.5 times slowdown CI showed, that is about 1.05 s, so the
  10 s ceiling is about 9.5 times clear.
- **Not measured here: the Intel runner.** This machine has no x86 core and
  no SMT. This PR's own `gate (x86_64-apple-darwin)` run gives the first
  Intel readings: the `linear-time` lines show with `--show-output`, and a
  failure prints both times and the ratio.
- **Cost.** The code_tables test went from 0.19 s alone to about 0.7 s,
  since each shape now renders 3.75 times as much.

## Gates

All at `16ac4f89` (the test change), with a short `TMPDIR`, one step at a
time. Each heavy step started only while no other worker's suite was running.

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| `cargo clippy --locked --all-targets --features fault-injection -- -D warnings` | ok |
| `cargo test --locked --no-fail-fast` | 4757 passed, 0 failed, 7 ignored (14 targets) |
| `cargo test --locked --features fault-injection --no-fail-fast` | 4786 passed, 0 failed, 7 ignored |
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
python3 .prd/tui-phase5/receipts/flake-mdtime/ratios.py run.log
# RED, on a throwaway clone only:
git clone --no-local <repo> <clone> && git -C <clone> apply <this dir>/mutants/M1-cache-off.patch
```
