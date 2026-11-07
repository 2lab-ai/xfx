# flake-mcpkill: the leader check after an MCP cut

`mcp::transport::tests::a_request_to_a_server_that_never_reads_ends_at_its_deadline_and_close_stops_it`
failed once on CI and once locally under load. The cause is in the test. It
looked once, at one instant, for an exit that the kernel completes after
`kill` returns. The production kill path is right. The fix changes the test
region of `src/mcp/transport.rs` only (`:1757-1864`, clear of epic 2's hunks).
Line numbers are at `main` @ `d314549e` unless marked "now".

## The failure

- PR #63's CI, job `gate (aarch64-apple-darwin)`, step "Tests, with the fault
  injector", 2026-11-06 20:59Z: 4008 passed, 1 failed. PR #63 does not touch
  MCP.
- It panicked at `src/mcp/transport.rs:1829:9`, the same line here, with
  "the leader was not killed, or was collected, before the close".
- That assertion was one `waitid(Pid(leader), EXITED|NOWAIT|NOHANG)` that
  had to return `Ok(Some(_))`. The message covered two different failures, so
  the log could not say which one happened.
- The same test failed once locally at load ~20 (PROFILE-DIRc's gate,
  2026-11-06) and passed 3/3 alone. It is a repeated failure.

## The production path: the kill is sent before `request` returns

- `request` writes the frame (`:703`). A frame its deadline cuts mid-write is
  `Unsent::Cut`, and `write`'s error mapping calls `abandon` (`:298`) before
  `request` returns the error (`:708`).
- `abandon` (`:545-555`) marks the connection dead and fails the waiters. Then,
  under the child's lock, it sends `kill_process_group(group, KILL)` (`:552`)
  and `child.kill()` (`:554`). Both are `kill(2)`: they return once the signal
  is posted, not once the process has exited.
- The leader leaves its slot only in `close` (`:766-771`), and is collected
  only by `stop_child` → `Server::collect` (`:876-880`). `Child::kill` does
  not collect.
- Nothing else in the test process collects it. There is no
  `waitpid(-1)`/`WaitId::All`/`WaitId::Pgid` anywhere in `src/` or `tests/`.
  The one test that ignores `SIGCHLD` sets it in a child's `pre_exec`
  (`process.rs:899-903`), never in this process.
- So of the two failures the old message covered, "collected" (`Err(ECHILD)`)
  cannot happen in this code. The one that fired was `Ok(None)`: SIGKILL had
  been sent, and the leader's exit was not yet visible. There is no
  production bug.

## Why it was rare: the grandchild check runs first

- The leader check came right after `assert_gone(grandchild, ...)` (`:1828`),
  which polls for up to 30 s.
- On macOS, `kill(pid, 0)` succeeds on a zombie. A probe on this machine: an
  exited child that had not been collected still answered `kill(pid, 0)`, and
  gave `ESRCH` only after it was collected.
- `DEAF`'s grandchild is the leader's own child, and the leader is an
  `exec`ed `sleep`, which never collects. So the grandchild disappears only
  once the leader is exiting. The old single look therefore ran after the
  leader's exit had begun. It failed only when it fell between the leader
  giving up its children and its own exit becoming visible to `waitid`. That
  window is short, and load widens it. [This is a hypothesis about the kernel's
  ordering, not something read in the kernel source. RED A and E below are the
  measured part.]

## Fix

- `assert_exited_uncollected(pid, what)` (now `:1773-1797`) calls `waitid(Pid,
  EXITED|NOWAIT|NOHANG)` every 20 ms:
  - `Ok(Some(_))`: return. The leader has exited and is still uncollected, so
    `close` can collect it.
  - `Ok(None)` or `EINTR`: look again. `EINTR` is retried because it says
    nothing about the process, as in production's `Server::exited` (`:862-868`).
    At `EXIT_PATIENCE` the helper fails: "{what} was not killed: it had not
    exited 30s later".
  - Any other error (`ECHILD`): fail at once: "{what} was collected before the
    close: {err}".
- `EXIT_PATIENCE` (now `:1761`) is 30 s, the same as `assert_gone`'s, which
  now uses the constant instead of its own literal.
- The call (now `:1864`) replaces the single look (`:1829-1842`). The comment
  above it says why the look waits.
- **The next break.** After the call, every remaining step of the test is
  either a poll or has a bound with at least 5 s of slack: `close` in
  `within(20 s)` and under `STOP_GRACE + 5 s`, then `assert_gone` for the
  leader and the grandchild, 30 s each. The other deaf-server tests
  (`:1853-1950`) check the leader and grandchild only through `assert_gone`.

## Sweep: every `NOHANG` wait in `src/` and `tests/`

`grep -rn "NOHANG\|WNOHANG" src tests`, without doc lines, finds 8 sites.

| site (at `d314549e`) | what it expects | when | verdict |
|---|---|---|---|
| `mcp/transport.rs:1837` | `Ok(Some)` | one look, right after a kill | **changed** |
| `mcp/transport.rs:1807` | `Ok(None)` | a frame held back whole; nothing was killed | unchanged: it asserts the leader is alive |
| `mcp/transport.rs:2025` | `Some` | already a 30 s poll loop | unchanged |
| `mcp/transport.rs:2239` | `Ok(Some)` | after a poll (`Server::exited`) saw the exit; a zombie stays one until it is collected | unchanged: the state is stable |
| `mcp/transport.rs:864` | (production) | `Server::exited`, inside `stop`'s grace loop | production code |
| `clipboard.rs:1018` | `Some` | inside `until(...)` | unchanged |
| `tools/terminal.rs:561` | (production) | the supervisor's poll loop | production code |
| `tests/support/pty.rs:745` | (helper) | `wait_state`, read through `Session::wait_state`'s polling loop; the direct read at `tests/interactive.rs:2689` follows a poll that saw the exit | unchanged |

## RED

Every variant ran only on a throwaway clone (`git clone --no-local`) at
`d314549e`, never on this worktree. Each was one test process at a time under
`gtimeout -k 10 120`. The patches are in `mutants/`; the full per-run log is
`red.log`. The mutants are:

- `M-delay`: the cut's kill is sent 150 ms late, from a thread.
- `M-nokill`: the cut kills nothing; `close` still stops the group.
- `M-collect`: the cut also collects the leader.
- `red-scratch-tests.patch`, which applies on top of the fix, adds two scratch
  tests that drive the real cut path against a server with no grandchild
  (`echo "$$ $$" > "$1"; exec sleep 120`). Without a grandchild check first,
  the leader is looked at straight after the cut. `red_old_point_check`
  makes the old single look; `red_new_poll` calls the new helper.

Neither mutant can signal a collected, reusable id. `M-delay`'s thread
sends its kill under the slot's lock, and only while the leader is still in
the slot. `M-collect` takes the leader out of the slot before collecting it,
so `close` signals nothing afterwards.

| # | mutant | test | result |
|---|---|---|---|
| A | `M-delay` | the old test, verbatim | 0/10 failed: the grandchild check absorbs the 150 ms |
| E | none | `red_old_point_check` | **19/20 failed**: "the leader was not killed, or was collected, before the close" |
| E | none | `red_new_poll` | 5/5 passed; the exit became visible after 22-30 ms |
| B | `M-delay` | `red_old_point_check` | **10/10 failed**, same message |
| B | `M-delay` | `red_new_poll` | 10/10 passed after 163-184 ms |
| B | `M-delay` | the new test, verbatim | 5/5 passed |
| C | `M-nokill` | `red_new_poll` | failed at 31.2 s: "the leader, at the cut, was not killed: it had not exited 30s later" |
| C | `M-nokill` | the new test, verbatim | failed at 30.4 s at the check that runs first: "the server's grandchild, at the cut, outlived its stop" |
| D | `M-collect` | the new test, verbatim | failed at once (1.2 s): "the leader, at the cut, was collected before the close: No child processes (os error 10)" |
| D | `M-collect` | `red_old_point_check` | failed with the old message, which covers both failures |
| F | `M-collect` | the old test, verbatim | failed with the old message |

E is the plainest evidence. With no mutant, a single look straight after
the cut fails 19 times in 20, although the kill was sent before `request`
returned. The exit becomes visible a few milliseconds later.

## Runs

All runs were on this machine (M5 Max) with the fault-injection lib test
binary (the CI step that failed), a debug build, and a short `TMPDIR`. The
load came from other workers' gates.

- **Alone, ×50**: the test with `--exact --test-threads=1`, 50 runs one at a
  time. 50/50 passed. The one-minute load average was 4.0 to 4.7.
- **The full fault-injection lib suite, ×3**: one suite at a time, each
  started only once no `cargo` and no `xfx-*` test binary was running. 3/3
  passed, 3985 passed and 0 failed each time, and the test passed in every
  suite. The one-minute load average was 4.0 to 6.5.
- The CI failure itself was not reproduced here: the old test was run only
  under `M-delay` (RED A), and passed. The fix does not rest on reproducing
  it. RED E shows the single look losing the race whenever no earlier wait
  hides it, and RED B shows the new poll winning the same race.

## Gates

All on the code of `5cbf64f4`, which is this branch's code; the commit
after it only finishes this file. Each step ran with a short `TMPDIR`, one at
a time, and each test suite started only on an idle machine.

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| `cargo clippy --locked --all-targets --features fault-injection -- -D warnings` | ok |
| `cargo test --locked --no-fail-fast` | 4865 passed, 0 failed, 7 ignored (14 targets) |
| `cargo test --locked --features fault-injection --no-fail-fast` | 4894 passed, 0 failed, 7 ignored (14 targets) |
| `scripts/check-xfx-identity.sh` · `check-no-stubs.sh` · `check-no-secrets.sh` | ok · ok · ok (re-run on the final tree) |
| `cargo build --locked --release`, and with `--features fault-injection --target-dir target/faulty` | ok, ok |
| `scripts/smoke.sh target/release/xfx` | 47 checks, 0 failures |
| `scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx` | 37 scenarios + the oracle, 1111 checks, 0 failures, no traceback in any scenario log |

`docs/parity.md`: no row touched. The unit changes a test, not behaviour.

## Re-run

```
cargo test --locked --features fault-injection --lib -- --exact \
  mcp::transport::tests::a_request_to_a_server_that_never_reads_ends_at_its_deadline_and_close_stops_it
# RED, on a throwaway clone only (git clone --no-local <repo> <clone>):
git -C <clone> apply <this dir>/mutants/M-delay.patch      # at d314549e
# the scratch tests: apply the fix first, then red-scratch-tests.patch
```
