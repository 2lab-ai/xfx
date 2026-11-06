# P5-PROC-SPAWNb receipt: one lock for every start and every pipe, kept by clippy

A security unit (S): it touches the one place every child process is made.
Branch `fix/p5-spawn-lock`, base `main` @ `a592c3a2`. PROC-SPAWN is the two
commits cherry-picked from MCP-M (`b3a8f6ef`, `7c3cf23a`); this round is
`a10d5335`, answering the two reviews of PROC-SPAWN (gpt6, opus).

## The cause, read in the standard library the gates build with

The repository pins no toolchain: `Cargo.toml:5` (`rust-version = "1.96"`)
is the floor, and the gates and CI (`ci.yml:84`, `rustup toolchain install
stable`) build with stable, rustc 1.99.1 here. Read in that toolchain's own
source, `$(rustc --print sysroot)/lib/rustlib/src/rust/library/std/src/`:

- `sys/pipe/unix.rs:14-31`: `pipe2(O_CLOEXEC)` on android, dragonfly,
  freebsd, hurd, illumos, linux, netbsd, openbsd, cygwin and redox only;
  `:32-41`, everywhere else, macOS included: `pipe()`, then `set_cloexec()`
  on each end.
- `sys/process/unix/unix.rs:71` (`setup_io`: the child's stdio pipes),
  `:81` (the pipe that says whether `exec` worked), `:139-140` (the parent
  reads it until EOF), `:467-470` (any `pre_exec` closure rules out
  `posix_spawn`, so every `process::command` start forks).
- A `fork` another thread makes between `pipe()` and `set_cloexec()` gives
  its child both ends; a child that lives all session (an MCP server) then
  holds the status pipe open, and that start never returns.

`process.rs:63` cited `sys/pal/unix/pipe.rs, Rust 1.99`; it now cites
`library/std/src/sys/pipe/unix.rs:32-41`, Rust 1.99.1, and where it was read.

## MUST 1 (opus): the lint failed on this tree

- RED, `7c3cf23a`, bounded: `gtimeout -k 10 1500 cargo test --locked --lib
  process::tests::no_child_process_is_made_any_other_way` -> FAILED,
  `src/process.rs:564`: `["…/src/session/store/fork/tests.rs:1668 (not
  through spawn)"]`. FK-RULES' `mkfifo` (`fork/tests.rs:1666-1669`), a test
  module only through `fork.rs:590-591`.
- Fix: that start goes through `process::status`, and the text rule that
  misjudged it is gone (MUST 3).
- GREEN: the case passes on the head (gates below).

## MUST 2 (both): the off-by-one

- RED, a `git clone --no-local` at `7c3cf23a`: the osascript start put back
  in its pre-fix form, a chain ending in `.status();`. The old case's
  finding list is unchanged, `fork/tests.rs:1668` alone: the form passes.
- Fix: the rule that walked back over `lines[..=number]` is deleted (MUST 3).
- GREEN, a clone at the head, same mutant: clippy
  `error: use of a disallowed method std::process::Command::status`,
  `src/clipboard.rs:434:14`.

## MUST 3 (gpt6) + SHOULD (opus): starts a text rule cannot see

- RED, the clone at `7c3cf23a`, the old case against each form:
  - a split statement, `let mut command = …; command.arg(…); let outcome =
    command.status();` (osascript): not flagged;
  - UFCS, `std::process::Command::spawn(&mut builder)` (MCP's stdio start):
    not flagged;
  - the control, a bare `builder.spawn()`: flagged, `src/mcp/transport.rs:245`.
- Fix: `clippy.toml` `disallowed-methods` on the resolved call:
  `std::process::Command::{spawn, status, output}`; tokio's three, with
  `allow-invalid` (tokio is built without `process`, so they wait until it
  is); `libc::{fork, posix_spawn, posix_spawnp}`; and the pipe makers
  `libc::pipe`, `rustix::pipe::pipe`, `std::io::pipe`. A scratch crate showed
  clippy 0.1.99 catching a later-statement call, UFCS, a `fn` value,
  `map(Command::spawn)` and a `libc` foreign function alike. The allows:
  `process::spawn` and `process::spawn_unless` (on the function: an
  attribute on a tail expression is not stable Rust), `process::pipe` (on
  its statement), and `build.rs` (its own one-thread process, which cannot
  reach the library's lock), each with a `reason`.
- Every start was routed, tests' too, so no unlocked start in a test binary
  races a locked one (opus's SHOULD): clippy's list held 64 sites -- 26 test
  starts and 3 pipes in `src/`, 32 in `tests/`, 2 in `build.rs`, and
  `process.rs`'s own. New helpers: `process::status` (std's `status` is
  `spawn(Stdio::Inherit, true)` and a wait, as `spawn` is:
  `process.rs:1096-1098`, `:1177-1182`) and `process::output`. std's
  `output` sets only the streams a caller left alone (`spawn(MakePipe,
  false)`, `sys/process/mod.rs:55-56`); the helper sets all three, so every
  `.output()` site was read for a stream it names. Three name all three
  (`tests/interactive.rs:99`, `:128`, `tests/tui.rs:1094`) and use `spawn`
  and `wait_with_output`; the rest name none (their builders in
  `tests/support/sandbox.rs`, `tests/cli.rs:74`, `tests/permissions.rs:4586`,
  `tests/tui.rs:28` set no stream).
- GREEN, the clone at the head: split-statement `status` -> clippy
  `src/clipboard.rs:432:31`; UFCS -> `src/mcp/transport.rs:245:13`; bare
  `spawn()` -> `:245:21`.

**Lint decision: the text start rule is deleted; the `Command::new` rule
stays.** Clippy judges the resolved call, so it is stronger on every form,
and both defects of the text rule (a gate-failing false positive, an
off-by-one) came from rebuilding statements out of lines, which a fix
narrows but does not end. The case keeps clippy's rule in place instead:
`clippy.toml` must still name the nine `std`/`libc`/`rustix` paths, and no
line in `src/` or `tests/` outside `process.rs` may name
`disallowed_methods`. Mutants (clone): `#[allow(clippy::disallowed_methods)]`
on MCP's start -> the case fails, `src/mcp/transport.rs:244 (lifts the
start rule)`; `clippy.toml` without its `status` entry -> `clippy.toml no
longer disallows std::process::Command::status`. (PROC-SPAWNc widens both
checks and says what they are not; see its section.)

## SHOULD 4 (both): the concurrent-start case

- Each worker sends its start's result; the case asserts every start
  started, and says why a round that failed to start says nothing of the
  lock.
- It owns every child: those back in the 10 s window are killed at once,
  each late one as it arrives. The wait for late ones is bounded at the
  sleepers' own end (`sleep 30`) plus 10 s: a start a sleeper's copy held
  is let go by then even when that sleeper never came back to be killed.
  Then every worker has sent and is joined.
- Mutants (clone): the lock taken out -> 3 of 3 runs fail (`round 0: 4 of
  16 starts returned only once the others' children were killed`; round 6,
  2 of 16; round 1, 1 of 16), each case done in 10-11 s, because the late
  starts came back once the others' children were killed; `spawn` always an
  error -> `round 0: 16 of 16 starts failed, so the round says nothing of
  the lock: ["mutant: no start", …]`.

## SHOULD 5 (opus): the clipboard's stop and the lock

- `process::spawn_unless(&mut command, || stop.stopped())`: the look is
  taken under the start lock, so a stop that lands while another start
  holds it refuses the tool rather than starting it to kill it.
- The look reads an atomic flag, never the stop's mutex, so a stop still
  never waits on a start, and no lock is taken inside another in a new
  order.
- (Superseded by PROC-SPAWNc, which keeps main's look before the lock as
  well.) This round had replaced it: work given up before it reached its
  start waited out a start in flight elsewhere before it refused.
- Pinned in `process`: `a_start_given_up_while_another_start_holds_the_lock_starts_nothing`
  (the case holds the lock, gives up while the starter waits: `None`; and a
  start nobody gave up starts). Mutant: the look taken before the lock ->
  fails at `src/process.rs:365`. (PROC-SPAWNc pins both looks at the
  clipboard as well, and makes this case wait on the starter's arrival at
  the lock rather than 100 ms.)

## SHOULD 6 (opus): `Wakeup::new`

`process::pipe()` makes a pipe and marks both ends close-on-exec under the
lock every start takes; `Wakeup::new` (`src/tui/signals.rs`) takes its pipe
from it and sets non-blocking itself. The two test pipes (`signals.rs`,
`probe.rs`) are made there too. Case
`a_pipe_is_made_close_on_exec_and_never_beside_a_start`. Mutants (clone):
`pipe()` without the lock -> fails at `src/process.rs:389`; without
close-on-exec -> `:397`; `Wakeup::new` back on `rustix::pipe::pipe` ->
clippy `src/tui/signals.rs:115:29`.

## Item 8: the runs

Both series one run at a time, each run under `gtimeout 300`, short
`TMPDIR`, a run's log kept when it failed.

- **This head, 30 full fault-injection lib suites in a row** (`cargo test
  --locked --features fault-injection --lib`, `a10d5335`): 30 of 30 passed,
  `3855 passed; 0 failed; 7 ignored` each, 16-18 s apiece.
- **The MCP case under the full suite, 200 runs.** The case
  (`mcp::tests::a_call_the_server_answers_with_no_result_fails_as_fx_says_it_did`)
  is MCP-M's (`src/mcp/tests.rs:1036@37391e34`); `main`, and so this branch,
  does not have it. So the tree measured is a throwaway clone built for it:
  MCP-M `37391e34` with `a10d5335` cherry-picked. Three conflicts, each
  resolved to the side that exists in MCP-M: `session_rules.rs` and
  `shell.rs` take MCP-M's (the conflicting hunks are tests only `main` has),
  `fork/tests.rs` is dropped (MCP-M has none), and the clipboard takes
  `spawn_unless`. Clippy (`--all-targets --features fault-injection`) is
  clean on it, so MCP-M brings no start or pipe of its own outside the
  lock. 200 full fault-injection lib suites in a row on it: 200 of 200
  passed (`3833 passed; 0 failed; 7 ignored` each, 11-28 s), the MCP case
  ok in every one, 0 stuck. MCP-Me's figures for comparison: 0 of 200 with
  the case run alone on the fixed head, 2 of 40 stuck on `c142070f`.
- The first tree that holds both the MCP case and this round's rule for
  real is the one that lands MCP-M on `main` after this unit: that merge's
  gates are where the case first runs under this rule outside a clone.

## PROC-SPAWNc: on the merge with `main` (`62d4173d`)

Both engines approved `ac7cc158` with no MUST. The dispatcher merged
`origin/main` `5ca0bc18` (H3 #56, FK-GAP #58) in as `62d4173d`; this round
is `f2d935a1` on top of it.

- **The merged tree's one clippy site.** Clippy (both feature sets) on
  `62d4173d` found one: `std::io::pipe` in H3's
  `src/usage/clock.rs:236` test (a pipe holding a time, read through
  `/dev/fd/<n>`). It is made by `process::pipe` now, its write end a
  `File`. No other site, in either feature set.
- **The clipboard looks twice** (opus SHOULD 4): main's look before the
  lock is back, ahead of `spawn_unless`'s look under it. Work already given
  up refuses without waiting out another start (`src/tui/copy.rs:95-97`
  joins with no limit); a stop that lands in the wait is still seen under
  the lock. Both are pinned at the clipboard now, by cases that hold the
  start lock (`process::hold_every_start`, test-only):
  `work_already_given_up_refuses_without_waiting_on_another_start` and
  `a_stop_that_lands_while_the_start_waits_on_another_never_starts_the_tool`.
  The second also holds the stop's hand-over lock, so a tool that was
  started is not killed before it can say so, as CLIP-STOP's `held` does.
  Mutants (clone), 3 runs each: no look before the lock -> `work already
  given up waited on another start before it refused`
  (`src/clipboard.rs:880`); no look under it (`|| false`) -> `a copy given
  up in its wait for the start lock started its tool`, the tool's pid in
  `left` (`:913`).
- **The rule case, hardened** (opus SHOULDs 1, 2). Each `clippy.toml` entry
  must begin a line of its own, so one commented out with `#` fails, and a
  line outside `process.rs` that names `disallowed_method` (the old name
  too), `clippy::all`, `clippy::style`, `allow(warnings` or
  `expect(warnings` is a finding. Mutants (clone), the attribute on MCP's
  start: `allow(renamed_and_removed_lints, clippy::disallowed_method)`,
  `allow(clippy::all)`, `allow(clippy::style)`, `allow(warnings)` -> each
  `src/mcp/transport.rs:244 (lifts the start rule)`; the `status` entry
  commented out -> `clippy.toml no longer disallows
  std::process::Command::status`.
- **What the case is** (gpt6): a guard against the ordinary ways to slip
  past the rule by accident, not a proof that none is left. A macro, an
  attribute under `cfg_attr`, a raw `syscall` or a dependency's own start
  are beyond what reading lines, or clippy's list, can see. The case's doc
  says so.
- **`clippy.toml`, widened** (opus SHOULD 3; none is called today):
  `libc::{vfork, forkpty, popen, system, socketpair}`,
  `rustix::pipe::pipe_with`, `std::os::unix::net::{UnixStream,
  UnixDatagram}::pair`, `mio::unix::pipe::new`. `allow-invalid` on the
  paths that do not resolve in every build: `vfork` (not in libc for Apple
  targets; libc 0.2.189 has it under `linux_like`), `pipe_with` (rustix's
  only where the system has `pipe2`), `mio` (not a dependency of xfx's
  own), tokio's three. A scratch crate with this `clippy.toml` (clippy
  0.1.99, macOS) flags `forkpty`, `popen`, `system`, `socketpair` and both
  `pair`s; none is reported as unresolved. The case requires all 21 entries.
- **No sleeps for timing** (gpt6). The given-up and pipe cases wait for
  the worker to reach the start lock: `one_at_a_time`, the one place the
  lock is taken, records each arriving thread in a test-only list, and the
  case waits (10 s bound) for its worker's id. Mutants (clone), 3 runs
  each: the look taken before the lock -> `a start given up in its wait
  for the lock still started its child` (`src/process.rs:410`); `pipe()`
  without the lock -> `a pipe was made while a start held the lock`
  (`:438`).

Gates, PROC-SPAWNc: one run of the brief's list on `f2d935a1`, a clean tree
at its start and its end (the receipt commit after it changes this
directory only):

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| `cargo clippy … --features fault-injection -- -D warnings` | ok |
| `cargo test --locked --no-fail-fast` | 4,762 passed, 0 failed, 7 ignored |
| `cargo test … --features fault-injection --no-fail-fast` | 4,791 passed, 0 failed, 7 ignored |
| `scripts/check-xfx-identity.sh`, `check-no-stubs.sh`, `check-no-secrets.sh` | ok |
| `cargo build --locked --release`; `--release --features fault-injection --target-dir target/faulty` | ok |
| `scripts/smoke.sh target/release/xfx <dir>` | 47 checks, 0 failures |
| `scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx <dir>` | 37 scenarios and the oracle, 1,111 checks, 0 failures |

## Gates, PROC-SPAWNb (head `a10d5335`, short `TMPDIR`)

One run of the brief's list, on `a10d5335` with a clean tree at its start
and its end (the receipt commit after it changes this directory only):

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| `cargo clippy … --features fault-injection -- -D warnings` | ok |
| `cargo test --locked --no-fail-fast` | 4,735 passed, 0 failed, 7 ignored |
| `cargo test … --features fault-injection --no-fail-fast` | 4,764 passed, 0 failed, 7 ignored |
| `scripts/check-xfx-identity.sh`, `check-no-stubs.sh`, `check-no-secrets.sh` | ok |
| `cargo build --locked --release`; `--release --features fault-injection --target-dir target/faulty` | ok |
| `scripts/smoke.sh target/release/xfx <dir>` | 46 checks, 0 failures |
| `scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx <dir>` | 37 scenarios and the oracle, 1,111 checks, 0 failures |

`docs/parity.md`: no row. No behaviour a user sees changes: the same
programs start with the same streams and environment, and a clipboard stop
that lands in the wait for the lock now refuses the tool rather than
killing it.

## Residuals (named, not fixed here)

1. **A child that never reaches `exec` now holds up every start, not only
   its own.** Examples: a terminal command whose `current_dir` is on a hung
   mount, a child stopped before `exec`. Normally the hold lasts as long as
   `exec` takes, which `src/clipboard.rs` notes can be seconds for a fresh
   executable. A session's exit joins its copies with no limit
   (`src/tui/copy.rs:95-97`), so such a stall delays the exit by as much.
2. **The reverse leak.** A descriptor made outside the lock with its
   close-on-exec set afterwards can still reach a locked start's child. On
   macOS that is every std socket (`sys/net/connection/socket/unix.rs:96-98`
   `socket`, `:135-139` `socketpair`, `:267-269` `accept`, Rust 1.99.1) and
   what tokio, mio and reqwest make inside their own code, which clippy does
   not read. Such a copy holds no start: a start waits only on its own
   status pipe, which is made under the lock. Not audited: whether some
   reader waits for an end that a copy would hold back (a loopback test
   server reading a connection to its close, say). None was seen in the
   runs below.

## Files

- `mutants.py`: every mutant above, applied to a throwaway clone's sources
  by name (run with the clone as the working directory).
