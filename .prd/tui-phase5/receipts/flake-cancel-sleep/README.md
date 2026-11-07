# flake-cancel-sleep: stop the work once it says it got there

Four tests stopped their work after a fixed sleep, then checked a state that
holds only if the work had already got that far. On a slow runner the stop
lands first. Each now waits for a marker the work itself writes at that
point, with a 60 s bound. No product code changed: the cancel paths are
right, and the fix is in the tests (`tests/permissions.rs`,
`tests/tool_loop.rs`, and the `#[cfg(test)]` module of
`src/usage/report.rs`). Line numbers are at `main` @ `88c7a5be` unless marked
"now".

This is the repeated-failure class FLAKE-MCPCANCEL (receipt on
`fix/p5-flake-mcpcancel`) fixed for the MCP transport: a fake server that
signals once the work is under way, and a cancel only after that signal. The
same worker listed `permissions.rs:3616`, `:3656`, `tool_loop.rs:4576` and
`llmux.rs:1432`. A sweep found two more in `src/usage/report.rs`.

## The sites

### `tests/permissions.rs:3606` — fixed (it can fail)

`a_stopped_command_is_asked_to_stop_and_the_screen_is_shown_how_it_ended`
cancelled 300 ms after its stopper thread started, just before the call
(`:3614-3617`).

- **What the checks need.** The second case,
  `trap '' TERM; sleep 30 & wait`, needs the shell to have set its trap when
  the stop comes. The stop is `terminate`: `SIGTERM` to the group, then up to
  `TERMINATION_GRACE` (700 ms) for the leader (`src/tools/terminal.rs:525-531`,
  `:557`), then `SIGKILL`. A shell with the trap set ignores the `SIGTERM`, is
  killed after the grace, and is shown `<signal>9</signal>`, and the call
  takes 300 ms + 700 ms, which meets `took >= 900 ms` (`:3641`).
- **Why it fails.** The command is spawned before the first look at the
  token (`terminal.rs:489`, `:525`). A stop that lands before the shell has
  run `trap` kills it with the `SIGTERM`: `<signal>15</signal>` within a few
  milliseconds, which fails both `<signal>9</signal> || <exit_code>`
  (`:3637-3640`) and `took >= 900 ms`. RED R1 shows exactly that.
- **A weak check found on the way.** The first case's
  `presented.contains("started")` (`:3630`) can never fail: the `<command>`
  line of the same account spells the command, `echo started; …`
  (`terminal.rs:307`). Under RED R1 the first case passed although its stop
  came 300 ms in, inside the 400 ms `sleep` that runs before `echo started`.
- **Fix** (now `:3597-3688`). Each command writes `began` after the state its
  case is about: `echo started; echo began > began; sleep 30`, and
  `trap '' TERM; echo began > began; sleep 30 & wait`. The command runs in the
  tree's root (as `:3929`'s `echo forked > forked` already relies on), so the
  marker is `tree.root()/began`. `stop_once_written` (now `:3666-3688`)
  polls for it every 5 ms for at most `START_PATIENCE` (60 s), cancels either
  way, and answers when it cancelled. The test fails by name if the marker
  never came.
- **Assertions, all kept.** `aborted by user`, `<signal>15</signal>`,
  `contains("started")`, `<signal>9</signal> || <exit_code>`, and the two
  bounds. The bounds are now measured from the stop instead of from the
  call, because the wait for the marker is not the command running:
  `took >= 900 ms` from the call, with the stop at 300 ms, was 600 ms past
  the stop, so it is now `>= 600 ms` from the stop (now `:3657`); `< 5 s`
  stays `< 5 s`, from the stop. One check is added beside the weak one:
  `<stdout>\nstarted\n` (now `:3644`), which only what the command wrote can
  meet.

### `tests/tool_loop.rs:4554` — fixed (it can fail)

`a_command_the_person_stopped_leaves_the_request_that_asked_for_it_counted`
cancelled 300 ms after the stopper thread started (`:4574-4577`).

- **What the checks need.** `sink.progress.last() == Settled(40)` and the
  usage frame's `Some(40)` need the request that asked for the command to
  have answered before the stop. The case named needs the command running.
- **Why it fails.** The turn looks at the token at its loop head, before
  each request (`src/agent/machine.rs:867-869`). A stop that lands before
  the turn reaches its first request ends the turn there: no request, no
  `Settled(40)`. RED R4 shows `left: None`. A stop between the answer and
  the spawn is taken when the call is admitted (`:1182`): the checks pass,
  with no command to stop.
- **Fix** (now `:4558-4632`). The command is `echo began > began; sleep 30`,
  so the marker exists only once the request has answered and the call has
  been approved and spawned. The stopper polls for it (60 s bound), cancels
  either way and returns when it did. The streamed `Chunk::ToolInput` text
  matches the new command. The ask-mode script still answers once: the new
  command is one approval, as the old was (R0, the runs).
- **Assertions, all kept.** `< 10 s` "the command ran on" is now from the
  stop (the wait is not the command running). Added: the marker came.

### `src/usage/report.rs:1843` and `:1894` — fixed (they can fail)

Found by the sweep. `a_fetch_runs_on_its_own_thread_and_can_be_cancelled_mid_body`
and `a_dropped_fetch_stops_and_lets_go_of_the_key` both cancelled (one by
`cancel()`, one by `drop`) after a fixed 1.5 s (`:1865`, `:1906`).

- **What the checks need.** `dripped >= 1 && hung_up` (`:1880-1883`) and
  `hung_up` (`:1916`) need the worker to have connected and the server to
  have sent the head and a body byte. The comment at `:1864` says so: "Into
  the body: the head and a byte or two have arrived."
- **Why they fail.** `fetch_all` selects the cancel first (`biased`,
  `:835-836`), so a stop before the worker has built its client and
  connected ends the fetch without a request. The `dripping` server then
  waits out `accept_within`'s 20 s (`:1476`) and answers `(0, false)`. RED R7
  and R10: `dripped 0, hung up: false` and "the connection outlived the
  fetch", each after 20 s.
- **Fix.** `dripping` (now `:1467-1519`) also returns the count of body bytes
  sent so far, stored after each write. `in_the_body` (now `:1527-1539`)
  waits for the first one (60 s bound). Both tests cancel or drop only after
  it. One byte, not two: one is the earliest point at which every check
  holds (the server has read the request and sent the head and a byte), and
  waiting for a second byte would add a second to every run for nothing a
  check reads. The
  third caller (`:1817`) ignores the count.
- **Assertions, all kept.** The mid-body test's `started.elapsed() < 7 s`
  (`:1871`) was 5.5 s past a cancel 1.5 s in; it is now
  `cancelled.elapsed() < 5.5 s`. Added: the fetch reached the body.

### `tests/permissions.rs:3646` — left (it cannot fail on timing)

`terminal_stops_when_the_turn_is_cancelled`, 150 ms then cancel, `sleep 30`,
checks `aborted by user` and `< 10 s`. The command is spawned unconditionally
before the token is first read (`terminal.rs:489`, `:525`), so a stop before
or after the shell starts takes the same branch (`terminate`, then
`stop_collecting`) and gives the same answer. Only a stop later than 10 s
would fail it, and a marker would not change that.

### `tests/llmux.rs:1418` — left (it cannot fail on timing)

`cancelling_a_started_stream_ends_the_attempt`, 100 ms then cancel, checks
`Err(ProviderError::Cancelled)`. Every look at the token in the provider
answers that: before the socket (`src/llmux/mod.rs:151-153`), at each chunk
or each `CANCEL_POLL` slice of the stream (`:202-204`, `:208-210`), and in
the decoder (`src/llmux/sse.rs:210`, `:223`). A stop that lands before the
request is answered at `:151` instead of in the stream loop the test is
named for: at worst it passes without its case. Noted for whoever next
touches it: a mutant that ignored the token in the stream loop would hang on
`SseThenHang` rather than fail.

## Sweep

Every `sleep(` in `tests/` and `src/` (209 calls) was listed with the 14
lines after it, and kept where those lines cancel, kill, signal, drop, close
or shut down something, or type Escape, Ctrl-C or Ctrl-D (116 sites). Each was
then read. The scripts are not committed; the list is.

| site (at `88c7a5be`) | what it is | verdict |
|---|---|---|
| `tests/permissions.rs:3615` | 300 ms, cancel, checks the trap held | **fixed** |
| `tests/tool_loop.rs:4575` | 300 ms, cancel, checks the request counted | **fixed** |
| `src/usage/report.rs:1865`, `:1906` | 1.5 s, cancel / drop, checks the server was called | **fixed** |
| `tests/permissions.rs:3655` | 150 ms, cancel | left: cannot fail (above) |
| `tests/llmux.rs:1431` | 100 ms, cancel | left: cannot fail (above) |
| `tests/permissions.rs:4821` | 500 ms after the first Gateway request, `SIGINT`, checks exit 1, a `cancelled` error, one request | left: a `SIGINT` before the command starts ends the turn at the provider's look at the token or the loop head's (`machine.rs:867`, `:1182`), and both errors read "the turn was cancelled" (`src/agent/types.rs:327`, `src/gateway/mod.rs:689`), so it passes either way [the wrapping of the provider's error was not traced]; at worst it passes without a running command |
| `src/agent/machine.rs:2569` | 40 ms, cancel a 30 s backoff | left: an early cancel is seen at the first look (`:244`) with the same answer |
| `src/tui/question.rs:3323` | 20 ms, cancel a question waiting for room | left: the wait is `send_ui`'s `biased` select (`src/tui/bridge.rs:1382-1392`), whose cancel arm answers the same before or during it |
| `src/mcp/transport.rs:1868`, `:1912` | 300 ms, cancel / close | owned by FLAKE-MCPCANCEL (`fix/p5-flake-mcpcancel`), not touched |
| `tests/tui.rs:3750` | Escape, 150 ms, Escape | left: the command's pid file and its row were waited for first (`:3730-3745`); the 150 ms only separates two keys inside fx's one-second window |
| `tests/tui.rs:4910`, `:1284`, `:5276`, `:5333`, `:5399`, `:5582`, `:6004`, `:11430`; `tests/ledger_compat.rs:319`, `:341`, `:350`, `:352`; `src/usage/dashboard/tests.rs:1038`, `:1117`, `:1146`, `:1662`; `src/provider/profile.rs:1292`; `src/provider/setup.rs:749`; `src/tui/signals.rs:795` | a fixed wait before a check that something did **not** happen (no request, no write through a held lock, still running, panel still up), or between two keys after the state was waited for | left: a slow runner cannot fail these; they are the opposite shape |
| `src/tui/signals.rs:1059` | a byte written 300 ms after a signal was already raised and pending | left: the signal comes first by construction |
| `src/tui/worker.rs:10041` | a 50 ms work future, raced against controls queued before it | left: the order is fixed before it runs |
| `src/tools/terminal.rs:540`, `:738`, `:1160`, `:1295`; `src/mcp/transport.rs:536`; `src/permission/gitconfig.rs:205`; `src/skills/install.rs:404`; `src/agent/machine.rs:251` | product code: paced polls and kill sweeps | not tests |
| the other 75 | bounded poll loops (`while`/`loop` on a condition with a deadline), or fake servers' accept loops | the shape the fix uses |

## RED

Every variant ran on a throwaway clone (`git clone --no-local`) at
`73c6d1b0`, this fix, never on this worktree. One build carried the new tests,
the old ones renamed `old_…`, and the mutants, each switched on by an
environment variable. `mutants/make_red.py` writes them into a clone;
`mutants/red-on-fix.patch` is the result (it applies on top of this fix).
Each run was one test process under `gtimeout -k 5 180`. `ulimit -v` is
refused by macOS, so the runs had a time bound only; none of them reads an
unbounded source. The per-row lines are `red.log`.

- `XFX_RED_SHELL_DELAY=0.4`: the test puts `sleep 0.4; ` in front of each
  command, a shell slow to reach its first word.
- `XFX_RED_SLOW_TURN_MS=400`: the turn sleeps before its loop head
  (`machine.rs:866`), a turn slow to reach its first request.
- `XFX_RED_SLOW_FETCH_MS=2000`: the fetch's worker sleeps before it builds its
  runtime (`report.rs:803`), a worker slow to connect.
- `XFX_RED_IGNORE_CANCEL=1`: `supervise` does not see the token
  (`terminal.rs:525`).
- `XFX_RED_FETCH_IGNORE_CANCEL=1`: `fetch_all`'s cancel arm is off
  (`report.rs:836`).

| # | test | variant | result |
|---|---|---|---|
| R0 | all eight, new and old | none | 8/8 passed |
| R1 | old permissions | slow shell | **failed 5/5** at the `<signal>9</signal> \|\| <exit_code>` check: the second case's account shows `<signal>15</signal>` and an empty `<stdout>`; the first case passed (above) |
| R2 | new permissions | slow shell | passed 5/5 |
| R3 | new permissions | ignore cancel | **failed 2/2** at 30 s: the command ran to its end, `left` is its `<exit_code>` account, not `aborted by user` |
| R4 | old tool_loop | slow turn | **failed 5/5**: `sink.progress.last()` is `None`, not `Settled(40)` |
| R5 | new tool_loop | slow turn | passed 5/5 |
| R6 | new tool_loop | ignore cancel | **failed 2/2** at 30 s: "the command ran on" |
| R7 | old mid-body fetch | slow fetch | **failed 3/3** at 20 s: "dripped 0, hung up: false" |
| R8 | new mid-body fetch | slow fetch | passed 5/5 |
| R9 | new mid-body fetch | ignore cancel | **failed 2/2** at 5 s: no outcome within 5 s of the cancel |
| R10 | old dropped fetch | slow fetch | **failed 3/3** at 20 s: "the connection outlived the fetch" |
| R11 | new dropped fetch | slow fetch | passed 5/5 |
| R12 | new dropped fetch | ignore cancel | **failed 2/2** at 5 s: "the worker outlived its fetch" |

R1/R2, R4/R5, R7/R8 and R10/R11 are the fix: the same delay fails the old
test every time and the new one never. R3, R6, R9 and R12 show the new tests
still fail when the stop is ignored. No process was left behind: the mutant
commands ran to their own end (R3, R6), and the fetch mutants' threads ended
with their test process.

## Runs

On this machine (M5 Max), debug build, short `TMPDIR`, at `73c6d1b0`. The
lines are `runs.log`.

- **Alone, ×30 each**: each changed test, `--exact`, one process per run, one
  run at a time. All four passed 30/30: the permissions test (slowest 1.27 s,
  its 700 ms grace included), the tool_loop test (0.59 s), the mid-body fetch
  (2.91 s) and the dropped fetch (2.07 s; both wait for the server's next
  write to fail). The one-minute load average was 3.4 to 4.1.
- **The full targets, ×3 each**: `cargo test --locked --test permissions`,
  `--test tool_loop` and `--lib`, one run at a time, each started only once
  no `cargo` and no `xfx-*` test binary was running. 9/9 passed: permissions
  158 passed each time, tool_loop 109, the lib 3979 passed and 7 ignored, in
  33-34 s, with the changed tests `ok` in every run. The load average was 5.8
  to 8.9 (other workers' runs between ours).
- A first try ran the three test binaries directly, without `cargo`; there
  `an_approved_command_runs_through_the_platform_shell_with_a_clean_environment`
  fails every time, because it checks that `CARGO_PKG_NAME`, which `cargo
  test` sets, is in the test process (`tests/permissions.rs:4111`). That is
  the runner, not this unit; the `cargo` runs above are the count.
- Nothing here reproduces the old failures without a delay: on this machine
  every marker comes within milliseconds. The fix does not rest on that. The
  RED rows reproduce each old failure once its work is slower than the old
  sleep, and show the new test passing there.

## Gates

All on the code of `73c6d1b0`, which is this branch's code; the commit
after it only adds this receipt. Each step ran with a short `TMPDIR`, one at
a time, and each test suite and smoke run started only once no `cargo` and no
`xfx-*` test binary was running.

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| `cargo clippy --locked --all-targets --features fault-injection -- -D warnings` | ok |
| `cargo test --locked --no-fail-fast` | 4879 passed, 0 failed, 7 ignored (15 targets) |
| `cargo test --locked --features fault-injection --no-fail-fast` | 4908 passed, 0 failed, 7 ignored (15 targets) |
| `scripts/check-xfx-identity.sh` · `check-no-stubs.sh` · `check-no-secrets.sh` | ok · ok · ok (re-run on the final tree, receipt included) |
| `cargo build --locked --release` | ok |
| `scripts/smoke.sh target/release/xfx <evidence>` | 47 checks, 0 failures |
| `cargo build --locked --release --features fault-injection --target-dir target/faulty` | ok |
| `scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx <evidence>` | 37 scenarios + the oracle, 1111 checks, 0 failures |

The release binaries contain none of the changed code: two of the files are
integration tests and the third change is inside `src/usage/report.rs`'s
`#[cfg(test)] mod tests`.

`docs/parity.md`: no row touched. The unit changes tests, not behaviour.
Registry counts, help entries and `check-no-stubs` rows: none changed.

Merges: `git merge-tree --write-tree` of this branch with every local
`fix/`, `feat/` and `integrate/p5-*` branch that touches these files — `feat/p5-mcp-wire` and `integrate/p5-epic-2` (both
`tests/permissions.rs` and `tests/tool_loop.rs`) merge cleanly;
`feat/p5-mcp-menu` and `feat/p5-mcp-verbs` conflict only in files this unit
does not touch (`src/tui/shell.rs`, `src/clipboard.rs`, `src/process.rs`,
`scripts/fxcap/parity-ledger.toml`); both also conflict when merged with
`main` itself. No other local `p5` branch touches `src/usage/report.rs`.

`crate::process::START_PATIENCE`, which the brief points at, is not on
`main` (it is on `fix/p5-flake-filelist`, and `pub(crate)`, so `tests/`
could not use it). Each file has its own 60 s `START_PATIENCE` instead.

## Re-run

```
cargo test --locked --test permissions -- --exact \
  a_stopped_command_is_asked_to_stop_and_the_screen_is_shown_how_it_ended
cargo test --locked --test tool_loop -- --exact \
  a_command_the_person_stopped_leaves_the_request_that_asked_for_it_counted
cargo test --locked --lib -- --exact \
  usage::report::tests::a_fetch_runs_on_its_own_thread_and_can_be_cancelled_mid_body \
  usage::report::tests::a_dropped_fetch_stops_and_lets_go_of_the_key
# RED, on a throwaway clone only (git clone --no-local <repo> <clone>), at this fix:
python3 -I <this dir>/mutants/make_red.py <clone>
XFX_RED_SHELL_DELAY=0.4 <permissions test binary> --exact \
  old_a_stopped_command_is_asked_to_stop_and_the_screen_is_shown_how_it_ended
```
