# flake-mcpcancel: the cancellation test raced the frame's serialization

`mcp::transport::tests::a_request_to_a_server_that_never_reads_stops_at_its_cancellation`
failed once on CI. The cause is in the test: it cancelled after a fixed
300 ms, and a cancellation that comes before the first byte of the frame is
written holds the frame back whole, by design. The production cancel path is
right. The fix changes the test region of `src/mcp/transport.rs` only. Line
numbers are at `main` @ `88c7a5be` unless marked "now".

This is the third sighting of this file's timing class in two days
(FLAKE-MCPKILL, PR #66, was the leader-exit look in the deadline sibling).
It is a repeated failure.

## The failure

- PR #64's CI, job `gate (x86_64-apple-darwin)`, step "Tests, with the fault
  injector" (`cargo test --locked --lib --features fault-injection`,
  `.github/workflows/ci.yml:117`), 2026-11-06 23:30Z: 3993 passed, 1 failed,
  7 ignored. PR #64 does not touch MCP.
- It panicked at `src/mcp/transport.rs:1875:9`, the same line here:
  `assertion failed: !transport.alive()`. The request had returned
  `Err(Cancelled)` (the assertion before it passed), and the connection was
  still up.
- The same test passed in the plain `cargo test` step of the same job.

## The production path: held whole before the first byte, cut after

- `request` (`:682`) sets the deadline, then builds the frame:
  `with_progress_token` (`:697`, `:886-901`) and `request_frame` (`:703`).
  The test's `oversized_call()` (`:1737-1739`), a 2 MiB string, is built on
  the request's thread just before.
- `Input::send` (`:364-378`) serializes the frame (`frame.to_string()`,
  `:370`) and appends the newline (`:371`) **before** it takes the pipe
  (`:372`). Nothing has been written yet.
- `Input::take` (`:381-404`) looks at the cancellation before it hands out
  the pipe (`:387`): a cancelled call gets `Unsent::Held`.
- `Input::push` (`:415-455`) looks before every write attempt (`:431`). The
  verdict is `Held` while `written == 0 && !refused` (`:432`), and `Cut`
  after (`:435`). `written` grows only after a write returns (`:440`).
- `StdioTransport::write` (`:287-307`) maps `Held(err)` to `err` and nothing
  else (`:296`): the connection stays as it was. `Cut(err)` calls `abandon`
  first (`:297-298`), which marks the connection dead (`:546`) and kills the
  group, before `request` returns.
- `Unsent` documents exactly this (`:334-341`), with fx's `taint_connection`
  as the source. A held-back frame leaving the connection up is also pinned by
  `a_cancelled_or_expired_call_puts_nothing_on_the_wire`.

So the panic means the cancellation was seen at `:387` or at the first look
at `:431`, before any byte went. On a loaded x86_64 runner, building and
serializing 2 MiB in a debug build took longer than the 300 ms the test
slept. There is no production bug. The deadline sibling's comment already
records the same window (some 40 ms on a fast machine, more on a loaded one)
and retries around it.

## Fix

- `DEAF_AFTER_A_BYTE` (now `:1739`) is `DEAF` with one step before the
  leader stops reading: `dd bs=1 count=1 of=/dev/null` reads exactly one
  byte, then `: > "$1.began"` drops a marker. `dd` is POSIX; `head -c` and
  `read -n` are not, and `head` may read a whole block.
- `await_first_byte(dir)` (now `:1748-1755`) polls for that marker, 60 s at
  most ("the frame never started").
- **Why this is deterministic.** Every look that can give `Held` (`take` at
  `:387`, `push`'s first look at `:431`) comes before the frame's first
  write in the writer's program order. The server can read a byte only after
  that write has put it in the pipe. So a cancellation set after the marker
  exists can be seen only by a later look, when `written > 0`: always `Cut`.
  The rest of the 2 MiB can never fit in the pipe, so the write cannot end
  before the cancellation.
- The cancellation test (now `:1875-1909`) uses the new server and cancels
  after `await_first_byte`. Every assertion is kept: `Cancelled` within 20 s,
  `!transport.alive()`, the next request refused with
  `McpConnectionClosed`, and close stopping the leader and the grandchild.
- **The next break, looked for.** After the cancel, each step either follows
  from the code or polls: `abandon` clears `alive` before `request` returns,
  so the alive check is not a race; `ping` is refused at `request`'s first
  line (`:689`); the leader and grandchild are checked through
  `assert_gone`'s 30 s poll. The writer sees the cancellation within one
  `CANCEL_POLL` (20 ms) slice of `poll`.

## Sweep

Every `sleep(` in `src/mcp/` and `tests/mcp.rs`, and every `.cancel()` in
`src/mcp/` and `tests/`, was read.

| site (at `88c7a5be`) | what it does | verdict |
|---|---|---|
| `mcp/transport.rs:1868` | fixed 300 ms, then cancel; asserts the frame was cut | **changed** (the failure) |
| `mcp/transport.rs:1912` | `close_stops_..._while_a_write_to_it_is_blocked`: waits for the request to register, then a fixed 300 ms, then close | **changed**: its assertions hold either way (`Closed` before `take` gives `McpConnectionClosed` too), so it could not go red, but under slow serialization it closed before any write held the pipe and passed without the case it names (RED R5). It now waits for the same marker (now `:1930`). |
| `mcp/transport.rs:1769` (deadline sibling) | a deadline is fixed before serialization, so it cannot be set "after the first byte" | unchanged: its retry loop is the right shape, and PR #66 owns that region |
| `mcp/transport.rs:1684`, `:2184` | cancel or a 100 ms / zero deadline before or around a small frame | unchanged: the result is the same whether the frame was held or written |
| `mcp/transport.rs:1724`, `:1763`, `:1941`, `:2032`, `:2210`, `:2230`, `:2273`; `mcp/tests.rs:72`, `:187`, `:299`, `:402`, `:1192`; `mcp/runtime.rs:794` | poll loops with a bound | unchanged |
| `tests/mcp.rs` | no sleep, no cancel | none |

Outside MCP, four tests in `tests/` also cancel after a fixed sleep. They
are not MCP frames, so they are outside this unit. They are listed here for
a unit of their own:

- `tests/permissions.rs:3616`, `a_stopped_command_is_asked_to_stop_and_the_screen_is_shown_how_it_ended`
  (300 ms). This one can go red. In its second case
  (`trap '' TERM; sleep 30 & wait`), a cancellation seen before the shell has
  set the trap gets `SIGTERM` (`src/tools/terminal.rs:525-531`), and
  `<signal>15</signal>` fails both the `<signal>9</signal>`/`<exit_code>`
  check and `took >= 900 ms`. The first case needs `started` echoed before
  the stop.
- `tests/permissions.rs:3656` (150 ms), `tests/tool_loop.rs:4576` (300 ms)
  and `tests/llmux.rs:1432` (100 ms) [inferred from the tests, not traced]:
  their assertions look the same whether the cancellation comes early or
  late, so at worst they pass without the case they name.
- `tests/permissions.rs:3966` already waits for a marker.

## RED

Every variant ran only on a throwaway clone (`git clone --no-local`) at
`88c7a5be` with this fix applied, never on this worktree. One build carried
both the new tests and the old ones, renamed `old_…`, and three mutants that
each switch on with an environment variable, so one binary covered every
row. The patch is `mutants/red-on-fix.patch` (it applies on top of this
fix); the per-run lines are `red.log`. Each run was one test process under
`gtimeout -k 5 180`, so a mutant that hangs ends.

- `XFX_RED_SLOW_MS=400`: `Input::send` sleeps 400 ms after serializing,
  before `take`. That is slow serialization, past the old 300 ms.
- `XFX_RED_IGNORE_CANCEL=1`: `push`'s looks do not see the cancellation.
- `XFX_RED_CLOSE_WAITS=1`: `Input::close` waits for a writer to put the pipe
  back, which is the bug the close test guards.

| # | test | mutants | result |
|---|---|---|---|
| R0 | all four (new and old) | none | 4/4 passed |
| R1 | old cancellation test | slow | **failed 11/11**: `assertion failed: !transport.alive()`, the CI failure |
| R2 | new cancellation test | slow | passed 11/11 |
| R3 | new cancellation test | slow + ignore-cancel | **failed** at 20.5 s: "the request outlived its cancellation: its write to the full pipe does not look" |
| R4 | new cancellation test | ignore-cancel | **failed** at 20.1 s, same message |
| R5 | old close test | slow + close-waits | passed 11/11: the close came before the write took the pipe, so the guarded bug went unseen |
| R6 | new close test | slow + close-waits | **failed** at 20.5 s: "close never returned: it waits for the input a blocked write holds, and the server runs on" |
| R7 | old close test | close-waits | failed at 20.4 s: without slow serialization the old test still catches it |
| R8 | new close test | slow | passed 11/11 |

R1 against R2 is the fix: the same slow serialization fails the old test
every time and the new test never. R3 and R4 show the new test still fails
when the write ignores its cancellation. R5 against R6 shows the old close
test passing a real bug when serialization is slow, and the new one
catching it.

The mutant runs that failed at a bound (R3, R4, R6, R7) left their server's
two `sleep 120`s running when the test process exited. They end on their own
within 120 s. They were not killed by hand: other workers' tests run the
same `sleep 120` scripts, and the pid files were gone with the test's
temporary directory.

## Runs

On this machine (M5 Max), debug build, short `TMPDIR`, with the
fault-injection lib test binary (the CI step that failed).

- **Alone, ×50 each**: the two changed tests with `--exact`, 50 runs one at
  a time. The cancellation test passed 50/50 (slowest run 913 ms); the close
  test passed 50/50 (slowest 107 ms). The one-minute load average was 3.0.
- **The full fault-injection lib suite, ×3**
  (`cargo test --locked --features fault-injection --lib --no-fail-fast`):
  one suite at a time, each started only once no `cargo` and no `xfx-*`
  test binary was running. 3/3 passed, 3994 passed, 0 failed and 7 ignored
  each time, in 34-35 s, with both changed tests `ok` in every suite. The
  one-minute load average was 2.7 to 3.4.
- The CI failure itself was not reproduced without a mutant: on this machine
  the first byte goes well inside 300 ms (the new test, which waits for it,
  finished in 0.09 s in R0, server start included). The fix does not rest on
  reproducing it. RED R1 reproduces its exact assertion once serialization
  is slower than the old sleep, and R2 shows the new test passing there.

## Gates

All on the code of `d2f16138`, which is this branch's code; the commit
after it only adds this receipt. Each step ran with a short `TMPDIR`, one at
a time, and each test suite started only on an idle machine.

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| `cargo clippy --locked --all-targets --features fault-injection -- -D warnings` | ok |
| `cargo test --locked --no-fail-fast` | 4879 passed, 0 failed, 7 ignored (15 targets) |
| `cargo test --locked --features fault-injection --no-fail-fast` | 4908 passed, 0 failed, 7 ignored (15 targets) |
| `scripts/check-xfx-identity.sh` · `check-no-stubs.sh` · `check-no-secrets.sh` | ok · ok · ok (re-run on the final tree) |

The release build and the smoke runners were not run: the change is inside
`#[cfg(test)] mod tests`, which no release binary contains.

`docs/parity.md`: no row touched. The unit changes tests, not behaviour.

Merges: `git merge-tree --write-tree` of this branch with
`fix/p5-flake-mcpkill` (PR #66), `integrate/p5-epic-2`, `feat/p5-mcp-wire`
and `feat/p5-mcp-status`, which all touch `src/mcp/transport.rs`, merges
`src/mcp/transport.rs` cleanly. `feat/p5-mcp-verbs` and `feat/p5-mcp-menu`
conflict only in files this unit does not touch (`src/tui/shell.rs`,
`src/clipboard.rs`, `src/process.rs`, `scripts/fxcap/parity-ledger.toml`),
because their base is newer than `88c7a5be`.

## Re-run

```
cargo test --locked --features fault-injection --lib -- --exact \
  mcp::transport::tests::a_request_to_a_server_that_never_reads_stops_at_its_cancellation
# RED, on a throwaway clone only (git clone --no-local <repo> <clone>), at this fix:
git -C <clone> apply <this dir>/mutants/red-on-fix.patch
XFX_RED_SLOW_MS=400 <lib test binary> --exact \
  mcp::transport::tests::old_a_request_to_a_server_that_never_reads_stops_at_its_cancellation
```
