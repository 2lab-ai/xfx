//! `terminal`: run one foreground command and return one captured result.
//!
//! The advertised action set is `["exec"]` and nothing else. Upstream's terminal
//! also owns durable interactive sessions -- `start`, `read`, `write`, `wait`,
//! `monitor`, `resize`, `signal`, `close`
//! (`vercel-labs/fx@580a0c5d src/tools/terminal/terminal.zig:180-232`) -- and
//! every one of them is a *reference the model can hold across turns*. A session
//! id outlives the authority that created it, which is exactly the property this
//! release is not ready to defend, so none of those names appears in the schema
//! at all. Advertisement is a promise; an action the model cannot see is an
//! action it cannot be talked into using.
//!
//! # Two routes, one of which involves no shell
//!
//! [`crate::permission::classify`] either reduces the command to an exact argv
//! or names the effect that stopped it:
//!
//! - **Direct.** The argv is executed with no shell process anywhere. Nothing
//!   re-parses the command text, so quoting, globbing, substitution, and
//!   redirection are not merely disallowed -- there is no component present that
//!   could perform them. This is the only route `auto` admits.
//! - **Reviewed shell.** `/bin/sh -c <command>`, with the exact text, cwd, and
//!   environment that were fingerprinted at admission. Reachable only through an
//!   explicit rule or a human approval.
//!
//! # What bounds it
//!
//! A wall-clock ceiling, a per-stream output ceiling, and the turn's
//! cancellation flag. Exit status and terminating signal are reported as facts
//! rather than translated into success or failure: a compiler that exits 1 has
//! answered the question, and a process killed by SIGSEGV has not.
//!
//! There is no OS sandbox. `status` reports `sandbox=none`, and the environment
//! is built rather than inherited so that xfx's own Gateway credential cannot
//! reach a child (design, "Risks and controls").

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::gateway::CancelToken;
use crate::permission::{CommandPlan, CommandRoute, PolicyDecision, ProposedAction};

use super::spec::{
    nonblank, object, optional_string, required_string, InputSchema, PermissionKind, Property,
    PropertyKind, ToolContext, ToolInput, ToolResult, ToolSpec,
};

/// How often a running command notices a timeout or a cancellation.
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// How long xfx will wait for a killed command's pipes to close.
///
/// A child that spawns a grandchild and exits leaves the grandchild holding the
/// inherited stdout. Killing the process group normally closes it; a process
/// that escaped its group with `setsid` will not. Waiting forever for that pipe
/// would turn "the command finished" into "xfx hangs", so the wait is bounded
/// and the shortfall is disclosed in the output.
const DRAIN_GRACE: Duration = Duration::from_millis(1_500);

/// How many extra times a killed command's process group is signalled while
/// its leader is still unreaped, and how long the sweep pauses between rounds.
///
/// Two rounds, not a duration: this is a bound on work, not a promise about
/// wall-clock time, which a loaded machine does not let anyone make.
const ANCHORED_ROUNDS: u32 = 2;

/// How many times the group is *asked* whether it is gone before xfx stops
/// waiting for it. Also a round bound rather than a time one; nothing is
/// signalled in these rounds, so their only cost is the wait.
const GROUP_SETTLE_ROUNDS: u32 = 20;

/// How long the sweep pauses between rounds.
const GROUP_KILL_PAUSE: Duration = Duration::from_millis(2);

/// The only action this build offers.
const EXEC_ACTION: &str = "exec";

const TERMINAL_DESCRIPTION: &str = "Run one command in the workspace and return its captured result: exit status, standard output, and standard error. \
Set action to exec. \
A recognized read-only command runs as an exact argument list with no shell, so quoting, globbing, variable substitution, redirection, and operators such as |, &&, ;, and > are not expanded and take the command off the automatic route. \
Commands that compile or run project code always need approval even though the automatic mode may have written the files they would compile; this includes cargo test, build, check, clippy, bench, run, and fmt, because a cargo alias in .cargo/config.toml can redirect any subcommand that is not a cargo built-in. \
The automatic cargo surface is cargo --version, cargo -V, cargo --list, and cargo metadata --no-deps. \
Operands must be relative, must not contain .., and must not resolve outside the authorized roots. \
Anything else needs an explicit approval before it runs. \
Output is captured, not streamed, and is truncated past a fixed size; the command is killed if it outruns its time limit. \
There is no sandbox: an approved command runs with the invoking user's privileges. \
When to use: build, test, lint, or inspect version control state. \
When NOT to use: reading or searching files (use the file tools), long-lived or interactive processes, or anything that publishes, installs, deletes, or reaches the network.";

pub const TERMINAL: ToolSpec = ToolSpec::new(
    "terminal",
    TERMINAL_DESCRIPTION,
    PermissionKind::RunCommand,
    InputSchema {
        properties: &[
            Property {
                name: "action",
                kind: PropertyKind::String,
                description: "The only supported action is exec: run one command and capture its result.",
                allowed: &[EXEC_ACTION],
            },
            Property {
                name: "command",
                kind: PropertyKind::String,
                description: "The command to run, as one line.",
                allowed: &[],
            },
            Property {
                name: "cwd",
                kind: PropertyKind::String,
                description:
                    "Directory to run in, inside an authorized root. Defaults to the workspace root.",
                allowed: &[],
            },
        ],
        required: &["action", "command"],
    },
    decode_terminal,
    validate_terminal,
    execute_terminal,
);

/// The decoded arguments of one `terminal` call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalInput {
    /// Always `exec` once decoding has succeeded; kept so a future action is a
    /// decoding change rather than a silent reinterpretation of this one.
    pub action: String,
    pub command: String,
    pub cwd: Option<String>,
}

fn decode_terminal(input: &Value) -> Result<ToolInput, String> {
    let object = object("terminal", input)?;
    let action = required_string("terminal", object, "action")?;
    if action != EXEC_ACTION {
        return Err(format!(
            "terminal field `action` must be one of {EXEC_ACTION}"
        ));
    }
    Ok(ToolInput::Terminal(TerminalInput {
        action,
        command: required_string("terminal", object, "command")?,
        cwd: optional_string("terminal", object, "cwd")?,
    }))
}

fn validate_terminal(input: &ToolInput) -> Result<(), String> {
    let ToolInput::Terminal(input) = input else {
        return Err("terminal received arguments that belong to another tool".to_string());
    };
    nonblank("terminal", "command", &input.command)?;
    if let Some(cwd) = &input.cwd {
        nonblank("terminal", "cwd", cwd)?;
    }
    Ok(())
}

fn execute_terminal(input: &ToolInput, context: &ToolContext) -> ToolResult {
    let ToolInput::Terminal(input) = input else {
        return ToolResult::failure("terminal received arguments that belong to another tool");
    };

    // Preparation resolves the working directory and classifies the command. It
    // starts nothing: by the time policy sees this, the command is a value.
    let plan = match CommandPlan::prepare(
        &input.command,
        context.scope(),
        input.cwd.as_deref(),
        context.limits(),
    ) {
        Ok(plan) => plan,
        Err(reason) => return ToolResult::failure(reason),
    };

    // The guard is released by the end of this statement; minting takes the
    // same lock.
    let decision = context.permissions().decide(ProposedAction::Command(&plan));
    let source = match decision {
        PolicyDecision::Allow { source } => source,
        PolicyDecision::Deny { reason, .. } => {
            return ToolResult::failure(format!("terminal did not run the command: {reason}"))
        }
        PolicyDecision::Prompt => {
            return ToolResult::failure(
                "terminal did not run the command: the approval was never resolved",
            )
        }
    };

    let authority = context.permissions().mint_command(plan, source);
    // Spent before it is used, so a failure cannot be retried on the same
    // approval.
    if let Err(err) = context.permissions().consume(&authority) {
        return ToolResult::revoked(format!("terminal could not use its authority: {err}"));
    }
    let plan = authority
        .command()
        .expect("a command authority carries a command plan");

    match run(plan, context) {
        Ok(outcome) => outcome.into_result(plan),
        Err(reason) => ToolResult::failure(format!("terminal could not run the command: {reason}")),
    }
}

// ---------------------------------------------------------------------------
// execution
// ---------------------------------------------------------------------------

/// How a command stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ending {
    Exited(i32),
    /// Killed by a signal. Reported as itself: a segfault is not "exit 139".
    Signalled(i32),
    /// xfx killed it because it ran too long.
    TimedOut {
        ms: u64,
        stranded: Option<Stranded>,
    },
    /// xfx killed it because the turn was cancelled.
    Cancelled {
        stranded: Option<Stranded>,
    },
}

/// Why xfx could not confirm that a killed command's process group was gone.
///
/// The kernel's own answer, kept as an answer rather than flattened to a bool:
/// "something is still running in there" and "the question could not be asked"
/// are different failures, and a report that cannot tell them apart is the
/// silent path this sweep exists to close.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stranded {
    /// `kill(-pgid, 0)` succeeded: the group still has a member xfx may signal.
    Running,
    /// `EPERM`. Per `kill(2)`, when signalling a process group this "is
    /// returned if any members of the group could not be signaled" -- so the
    /// group still exists. A process that has died but not been collected by
    /// its reaper answers this way.
    Unsignalable,
    /// Any other errno, named rather than swallowed.
    Errno(i32),
}

/// What one command produced.
struct Outcome {
    ending: Ending,
    stdout: Captured,
    stderr: Captured,
}

impl Outcome {
    fn into_result(self, plan: &CommandPlan) -> ToolResult {
        let mut out = String::new();
        out.push_str(&format!("<command>{}</command>\n", framed(plan.command())));
        out.push_str(&format!("<cwd>{}</cwd>\n", framed(plan.display_cwd())));
        match self.ending {
            Ending::Exited(code) => out.push_str(&format!("<exit_code>{code}</exit_code>\n")),
            Ending::Signalled(signal) => out.push_str(&format!("<signal>{signal}</signal>\n")),
            Ending::TimedOut { ms, stranded } => {
                return ToolResult::failure(format!(
                    "terminal timed out: `{}` ran longer than {ms} ms and was killed\n{}{}",
                    framed(plan.command()),
                    self.streams(),
                    stranded_notice(stranded)
                ))
            }
            Ending::Cancelled { stranded } => {
                return ToolResult::failure(format!(
                    "terminal was cancelled: `{}` was killed before it finished\n{}{}",
                    framed(plan.command()),
                    self.streams(),
                    stranded_notice(stranded)
                ))
            }
        }
        out.push_str(&self.streams());

        let detail = match self.ending {
            Ending::Exited(code) => format!("{} (exit {code})", plan.command()),
            Ending::Signalled(signal) => format!("{} (signal {signal})", plan.command()),
            // Both are returned above.
            Ending::TimedOut { .. } | Ending::Cancelled { .. } => unreachable!(),
        };
        // A command that ran and reported a nonzero status answered the
        // question. Only xfx's own failures are refusals.
        ToolResult::success(out, detail)
    }

    fn streams(&self) -> String {
        format!(
            "<stdout>\n{}</stdout>\n<stderr>\n{}</stderr>\n",
            self.stdout.render(),
            self.stderr.render()
        )
    }
}

/// One captured stream, bounded in bytes and in how long xfx waited for it.
struct Captured {
    bytes: Vec<u8>,
    /// How many bytes were produced past the bound.
    dropped: usize,
    /// Whether the stream was still open when xfx stopped waiting.
    stalled: bool,
}

/// Escapes text so it cannot close, open, or counterfeit one of xfx's own tags.
///
/// The frame around a captured stream is the only thing telling the model that
/// these bytes are *a command's output* rather than a statement by xfx. A file
/// or a program that prints `</stdout><exit_code>0</exit_code>` would otherwise
/// end its own quotation and start writing the report -- the same attack
/// `crate::workspace::context` escapes an `AGENTS.md` body against, and the same
/// answer.
///
/// `<` and `>` because with them gone no tag can be opened or closed, and `&` so
/// the encoding is reversible rather than lossy: a model that sees `&lt;` can
/// tell it apart from output that really contained `&lt;`. Everything else --
/// quotes, newlines, ANSI escapes, the shape of a diff -- passes through
/// unchanged, because output a model cannot read is output it cannot act on.
fn framed(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            other => out.push(other),
        }
    }
    out
}

impl Captured {
    /// The stream as the model sees it: the child's own bytes, escaped, then
    /// xfx's notices about what it did not capture.
    ///
    /// The notices are appended after the escaping rather than passed through
    /// it, so they stay xfx's words -- and they contain no tag of their own, so
    /// nothing about them is ambiguous.
    fn render(&self) -> String {
        let mut out = framed(&String::from_utf8_lossy(&self.bytes));
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        if self.dropped > 0 {
            out.push_str(&format!(
                "... [truncated; {} more bytes were not captured]\n",
                self.dropped
            ));
        }
        if self.stalled {
            out.push_str(
                "... [stream still open; a process outlived the command and xfx stopped waiting]\n",
            );
        }
        out
    }
}

/// Runs a prepared command to completion, a timeout, or a cancellation.
fn run(plan: &CommandPlan, context: &ToolContext) -> Result<Outcome, String> {
    let mut command = match plan.route() {
        CommandRoute::Direct { argv } => {
            let mut command = Command::new(&argv[0]);
            command.args(&argv[1..]);
            command
        }
        CommandRoute::Shell { program } => {
            let mut command = Command::new(program);
            command.arg("-c").arg(plan.command());
            command
        }
    };
    command.current_dir(plan.cwd());
    // Built, not inherited: whatever is in xfx's environment stays there.
    command.env_clear();
    for (name, value) in plan.environment() {
        command.env(name, value);
    }
    // A command that waits for input would otherwise wait forever; an immediate
    // end-of-file makes it fail fast and say so.
    command.stdin(Stdio::null());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    // Its own process group, so a timeout or a cancellation can reach every
    // process the command started and not only the one xfx forked. `sh -c 'x &'`
    // exits immediately while `x` keeps running; without a group kill, `x`
    // survives the turn and keeps xfx's pipe open.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    let mut child = command
        .spawn()
        .map_err(|err| format!("{}: {err}", first_word(plan.command())))?;

    let limit = context.limits().max_command_output_bytes;
    let stdout = drain(child.stdout.take(), limit);
    let stderr = drain(child.stderr.take(), limit);

    let ending = supervise(
        &mut child,
        Duration::from_millis(context.limits().command_timeout_ms),
        context.cancel(),
    );

    // Bounded: whatever has arrived by the deadline is what the model gets. A
    // stream still held open is reported as such rather than waited on.
    let deadline = Instant::now() + DRAIN_GRACE;
    Ok(Outcome {
        ending,
        stdout: stdout.collect(deadline),
        stderr: stderr.collect(deadline),
    })
}

/// Waits for `child`, killing it if it outruns the deadline or the turn.
///
/// A poll loop rather than a blocking wait: the same loop has to notice two
/// different reasons to stop, and neither of them arrives on the child's own
/// file descriptors.
fn supervise(child: &mut Child, timeout: Duration, cancel: &CancelToken) -> Ending {
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return ended(status),
            Ok(None) => {}
            Err(_) => return Ending::Exited(-1),
        }
        if cancel.is_cancelled() {
            return Ending::Cancelled {
                stranded: stop(child),
            };
        }
        if started.elapsed() >= timeout {
            return Ending::TimedOut {
                ms: timeout.as_millis() as u64,
                stranded: stop(child),
            };
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Kills everything the command started, and reaps the child xfx forked.
///
/// The group is signalled first, because the process holding xfx's pipe is very
/// often not the process xfx forked. The direct kill follows as a backstop for
/// the case where the group could not be signalled at all. Then the group is
/// signalled again -- see `sweep_anchored` for why once is not enough -- and
/// only after that is the leader collected and the group asked whether it is
/// gone.
///
/// Returns `None` when the group is gone, and otherwise the kernel's reason for
/// saying it is not. That answer is not a detail to swallow: it means something
/// the command started may still be running, still holding the pipes xfx handed
/// it, after xfx did everything it can do about it -- and the caller says so in
/// the tool's own output.
fn stop(child: &mut Child) -> Option<Stranded> {
    #[cfg(unix)]
    let group = rustix::process::Pid::from_raw(child.id() as i32);

    #[cfg(unix)]
    if let Some(group) = group {
        let _ = rustix::process::kill_process_group(group, rustix::process::Signal::KILL);
    }
    let _ = child.kill();

    // Before the leader is collected, never after: see `sweep_anchored`.
    #[cfg(unix)]
    if let Some(group) = group {
        sweep_anchored(group);
    }

    let _ = child.wait();

    #[cfg(unix)]
    let stranded = match group {
        Some(group) => settled(group),
        None => None,
    };
    #[cfg(not(unix))]
    let stranded = None;

    stranded
}

/// Signals `group` again, `ANCHORED_ROUNDS` times, while the command's leader
/// is still unreaped.
///
/// **One signal is not enough, and the reason is a race in the kernel rather
/// than a gap in xfx's bookkeeping.** `kill(-pgid)` walks the group's list of
/// members; a `fork` running at the same instant adds a member the walk may
/// have already passed. That member never receives the signal. Its parent is
/// dead, so nothing will fork again -- but the survivor keeps running, and it
/// keeps the stdout and stderr pipes xfx handed the command open, which is
/// exactly the "stream still open" the drain then has to report. Measured on
/// macOS, with one signal: 51 of 240 cancellations five milliseconds into a
/// twenty-child fork loop left a child alive, and 91 of 160 cancellations forty
/// milliseconds into a three-hundred-child loop. With this sweep, both were 0.
///
/// **Every signal xfx sends to the group is sent from here, before the leader
/// is collected, and that ordering is the whole safety argument.** A process
/// group id is a process id, and process ids are recycled. The id cannot be
/// recycled while this function runs, because the leader has been killed but
/// not yet waited for: POSIX ends a process's lifetime -- and only then returns
/// its id to the system -- when it is waited for, not when it dies, and a group
/// exists while it has a member. Measured here rather than assumed: on macOS a
/// group whose only member is an unreaped leader answers `kill(-pgid, 0)` with
/// `EPERM`, not `ESRCH`, which per `kill(2)` "is returned if any members of the
/// group could not be signaled" -- the group is still there. So a signal sent
/// in this window provably reaches this command's group and nothing else.
///
/// After the leader is collected xfx signals nothing further, however the
/// checks in `settled` come out. A signal then would rest on the id still being
/// this group's, which is exactly what collecting the leader gave up.
#[cfg(unix)]
fn sweep_anchored(group: rustix::process::Pid) {
    for _ in 0..ANCHORED_ROUNDS {
        std::thread::sleep(GROUP_KILL_PAUSE);
        let _ = rustix::process::kill_process_group(group, rustix::process::Signal::KILL);
    }
}

/// Asks, up to `GROUP_SETTLE_ROUNDS` times, whether `group` is gone. Sends
/// nothing. Returns `None` once it is gone, and the kernel's reason otherwise.
///
/// The question is `kill(-pgid, 0)`, which rustix documents as "check validity
/// of pid and permissions to send signals to all processes in the process
/// group, without actually sending any signals" -- so its two failures mean
/// different things, and `is_err()` is not the predicate. Per `kill(2)`:
///
/// * `ESRCH` -- "no process or process group can be found corresponding to that
///   specified by pid". The group is gone. This is the only good answer.
/// * `EPERM` -- "when signalling a process group, this error is returned if any
///   members of the group could not be signaled". The group is **still there**;
///   a member has died but not yet been collected by its reaper. Waiting is the
///   right response, and reading it as "gone" -- which `is_err()` did -- is a
///   report of success about a group that still exists.
/// * anything else -- the question could not be asked, which is reported as
///   itself rather than guessed at.
///
/// Asking after the leader has been collected is safe in a way signalling would
/// not be: if the id has been recycled, the worst outcome is a notice about a
/// group that is not xfx's, and no process is touched either way.
#[cfg(unix)]
fn settled(group: rustix::process::Pid) -> Option<Stranded> {
    let mut last = Stranded::Running;
    for _ in 0..GROUP_SETTLE_ROUNDS {
        last = match rustix::process::test_kill_process_group(group) {
            Err(rustix::io::Errno::SRCH) => return None,
            Ok(()) => Stranded::Running,
            Err(rustix::io::Errno::PERM) => Stranded::Unsignalable,
            Err(other) => Stranded::Errno(other.raw_os_error()),
        };
        std::thread::sleep(GROUP_KILL_PAUSE);
    }
    match rustix::process::test_kill_process_group(group) {
        Err(rustix::io::Errno::SRCH) => None,
        _ => Some(last),
    }
}

/// What xfx says when it could not confirm that a killed command's group is
/// gone.
///
/// Its own words, appended after the escaped stream text like the drain's
/// notices, so a model can tell xfx's report apart from the command's output.
fn stranded_notice(stranded: Option<Stranded>) -> String {
    let Some(stranded) = stranded else {
        return String::new();
    };
    let because = match stranded {
        Stranded::Running => "it still has a running member".to_string(),
        Stranded::Unsignalable => {
            "its members could not be signalled (EPERM), so something in it has not been \
             collected yet"
                .to_string()
        }
        Stranded::Errno(code) => format!("the group could not be checked (errno {code})"),
    };
    format!(
        "... [the command's process group was still there after {GROUP_SETTLE_ROUNDS} checks: \
         {because}; something the command started may have outlived it]\n"
    )
}

fn ended(status: std::process::ExitStatus) -> Ending {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return Ending::Signalled(signal);
        }
    }
    Ending::Exited(status.code().unwrap_or(-1))
}

/// A stream being read on its own thread, up to a byte bound and a deadline.
struct Drain {
    state: Arc<Mutex<Vec<u8>>>,
    dropped: Arc<AtomicUsize>,
    finished: Arc<AtomicBool>,
    /// True when there was no stream to read at all.
    absent: bool,
}

impl Drain {
    /// Everything read by `deadline`, and whether the stream was still open.
    ///
    /// The reader thread is never joined. Joining is exactly the bug: a
    /// grandchild holding the inherited pipe keeps the thread blocked in `read`,
    /// and a join would block the turn behind it for as long as that process
    /// lives. The thread is left to exit on its own; it writes into a mutex
    /// whose contents have already been taken, so it cannot affect the result.
    fn collect(self, deadline: Instant) -> Captured {
        if !self.absent {
            while !self.finished.load(Ordering::Acquire) && Instant::now() < deadline {
                std::thread::sleep(POLL_INTERVAL);
            }
        }
        let stalled = !self.absent && !self.finished.load(Ordering::Acquire);
        let bytes = std::mem::take(
            &mut *self
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        );
        Captured {
            bytes,
            dropped: self.dropped.load(Ordering::Relaxed),
            stalled,
        }
    }
}

/// Reads `source` on a thread, keeping at most `limit` bytes and counting the
/// rest.
///
/// Reading continues past the bound rather than stopping: a child whose pipe
/// fills up blocks forever, which would turn "too much output" into "hangs until
/// the timeout". The bytes past the bound are counted and discarded.
fn drain<R: Read + Send + 'static>(source: Option<R>, limit: usize) -> Drain {
    let state = Arc::new(Mutex::new(Vec::new()));
    let dropped = Arc::new(AtomicUsize::new(0));
    let finished = Arc::new(AtomicBool::new(false));
    let Some(mut source) = source else {
        return Drain {
            state,
            dropped,
            finished,
            absent: true,
        };
    };

    let thread_state = Arc::clone(&state);
    let thread_dropped = Arc::clone(&dropped);
    let thread_finished = Arc::clone(&finished);
    std::thread::spawn(move || {
        let mut buffer = [0u8; 8 * 1024];
        loop {
            match source.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(read) => {
                    let mut kept = thread_state
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    let room = limit.saturating_sub(kept.len());
                    let take = room.min(read);
                    kept.extend_from_slice(&buffer[..take]);
                    if read > take {
                        thread_dropped.fetch_add(read - take, Ordering::Relaxed);
                    }
                }
            }
        }
        thread_finished.store(true, Ordering::Release);
    });
    Drain {
        state,
        dropped,
        finished,
        absent: false,
    }
}

/// The first word of a command, for an error that names what could not start.
fn first_word(command: &str) -> &str {
    command.split_whitespace().next().unwrap_or(command)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_only_action_the_decoder_accepts_is_exec() {
        assert!(decode_terminal(&json!({ "action": "exec", "command": "pwd" })).is_ok());
        for action in ["start", "read", "write", "close", "monitor"] {
            let message =
                decode_terminal(&json!({ "action": action, "command": "pwd" })).unwrap_err();
            assert_eq!(message, "terminal field `action` must be one of exec");
        }
    }

    #[test]
    fn the_schema_offers_exactly_one_action_and_no_durable_field() {
        let schema = TERMINAL.advertisement().to_string();
        assert!(schema.contains("\"enum\":[\"exec\"]"), "{schema}");
        for durable in [
            "session_id",
            "return_when",
            "monitor",
            "lease",
            "dimensions",
        ] {
            assert!(!schema.contains(durable), "{schema}");
        }
    }

    #[test]
    fn a_missing_command_is_named_rather_than_defaulted() {
        assert_eq!(
            decode_terminal(&json!({ "action": "exec" })).unwrap_err(),
            "terminal requires the string field `command`"
        );
        let input = decode_terminal(&json!({ "action": "exec", "command": "  " })).unwrap();
        assert_eq!(
            validate_terminal(&input).unwrap_err(),
            "terminal field `command` must not be empty"
        );
    }

    #[test]
    fn a_blank_working_directory_is_refused_rather_than_treated_as_absent() {
        let input =
            decode_terminal(&json!({ "action": "exec", "command": "pwd", "cwd": " " })).unwrap();
        assert_eq!(
            validate_terminal(&input).unwrap_err(),
            "terminal field `cwd` must not be empty"
        );
    }

    #[test]
    fn a_bounded_stream_says_how_much_it_dropped() {
        let captured = Captured {
            bytes: b"kept".to_vec(),
            dropped: 7,
            stalled: false,
        };
        let rendered = captured.render();
        assert!(rendered.starts_with("kept\n"), "{rendered}");
        assert!(
            rendered.contains("... [truncated; 7 more bytes were not captured]"),
            "{rendered}"
        );

        let complete = Captured {
            bytes: b"all\n".to_vec(),
            dropped: 0,
            stalled: false,
        };
        assert_eq!(complete.render(), "all\n");

        // A stream xfx stopped waiting for says so, so "no more output" is
        // distinguishable from "output xfx never saw".
        let stalled = Captured {
            bytes: b"partial\n".to_vec(),
            dropped: 0,
            stalled: true,
        };
        assert!(
            stalled.render().contains("stream still open"),
            "{stalled:?}",
            stalled = stalled.render()
        );
        // Silence stays silent, so "no output" is distinguishable from "output
        // xfx chose not to show".
        let empty = Captured {
            bytes: Vec::new(),
            dropped: 0,
            stalled: false,
        };
        assert_eq!(empty.render(), "");
    }

    #[test]
    fn the_description_states_that_there_is_no_sandbox() {
        assert!(
            TERMINAL_DESCRIPTION.contains("no sandbox"),
            "{TERMINAL_DESCRIPTION}"
        );
        assert!(
            TERMINAL_DESCRIPTION.contains("approval"),
            "{TERMINAL_DESCRIPTION}"
        );
        // The one asymmetry a model has to know about: it may write the files
        // it may not compile.
        assert!(
            TERMINAL_DESCRIPTION.contains("cargo test"),
            "{TERMINAL_DESCRIPTION}"
        );
    }

    /// A stream that never ends and never closes, like an inherited pipe held
    /// by a process that outlived the command.
    struct NeverCloses;

    impl Read for NeverCloses {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            std::thread::sleep(Duration::from_millis(20));
            buffer[0] = b'.';
            Ok(1)
        }
    }

    #[test]
    fn a_drain_returns_at_its_deadline_instead_of_waiting_for_the_pipe() {
        // Joining here is the bug this exists to prevent: the reader thread can
        // never finish, so a join would hold the turn open for as long as
        // whatever is holding the pipe lives.
        let drain = drain(Some(NeverCloses), 16);
        let started = Instant::now();
        let captured = drain.collect(started + Duration::from_millis(200));
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the drain waited {:?}",
            started.elapsed()
        );
        assert!(captured.stalled, "an open stream must be reported as open");
        assert!(captured.bytes.len() <= 16, "{}", captured.bytes.len());
        assert!(captured.render().contains("stream still open"));
    }

    #[test]
    fn a_command_that_could_not_start_is_named_by_its_first_word() {
        assert_eq!(first_word("cargo test --all"), "cargo");
        assert_eq!(first_word(""), "");
    }

    /// A shell that spends milliseconds forking children into its own process
    /// group, spawned exactly the way `run` spawns one.
    ///
    /// The fork loop is the whole point: the escape being tested for happens
    /// when the group is signalled *while* a member is being forked, so the
    /// command under test has to be forking when the signal arrives.
    #[cfg(unix)]
    fn a_forking_shell(tag: &str, children: usize) -> Child {
        use std::os::unix::process::CommandExt;

        let mut command = Command::new("/bin/sh");
        command.arg("-c").arg(format!(
            "i=0; while [ $i -lt {children} ]; do sleep {tag} & i=$((i+1)); done; wait"
        ));
        command.env_clear();
        command.stdin(Stdio::null());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        command.process_group(0);
        command.spawn().expect("spawn a shell")
    }

    /// `stop` must leave the command's process group empty, even when the
    /// signal lands while the command is still forking.
    ///
    /// **This is a bounded statistical test, and deliberately so.** The escape
    /// it guards against is a race between the kernel's walk of a process
    /// group's member list and a `fork` adding to that list, and no portable,
    /// unprivileged, black-box mechanism available to this suite can make it
    /// happen on demand: a member cannot be made to appear after the walk
    /// without racing the walk, because every other way of joining a group
    /// (`setpgid` from a helper, a late `Command` `process_group`) requires the
    /// group to still have a member, which is exactly what the first signal
    /// removes. A debugger stopping the shell mid-`fork`, or a kernel probe,
    /// could do it; neither belongs in a unit test.
    ///
    /// So the window is entered repeatedly instead. Measured on macOS with a
    /// single signal, 240 cancellations 5ms into a 20-child fork loop escaped
    /// 51 times, and 160 cancellations 40ms into a 300-child loop escaped 91
    /// times; with the group signalled until it is empty, both were 0. At the
    /// shape below one round escaped about 29% of the time, so 16 rounds fail
    /// with probability ~0.996 against a single-signal `stop` -- and must be
    /// exactly 0 against this one.
    #[cfg(unix)]
    #[test]
    fn a_stopped_command_leaves_nothing_alive_in_its_process_group() {
        // A duration unique to this process, so a survivor can be cleaned up
        // without reaching for another test's children.
        let tag = format!("30.{}", std::process::id());
        let mut escaped = 0;
        let mut rounds = 0;

        for _ in 0..16 {
            let mut child = a_forking_shell(&tag, 50);
            let group = rustix::process::Pid::from_raw(child.id() as i32).expect("a group");
            // Into the window: long enough that the shell is forking, short
            // enough that it has not finished.
            std::thread::sleep(Duration::from_millis(10));

            stop(&mut child);

            rounds += 1;
            // `Ok` is the strict reading: the group still holds a member that
            // can be signalled, which is a process that is running. `EPERM`
            // would mean something is there but already dead and uncollected,
            // and `ESRCH` that the group is gone; neither is an escape.
            if rustix::process::test_kill_process_group(group) == Ok(()) {
                escaped += 1;
                // Never leave a survivor behind, whatever the verdict is.
                let _ = rustix::process::kill_process_group(group, rustix::process::Signal::KILL);
            }
        }

        assert_eq!(
            escaped, 0,
            "{escaped} of {rounds} cancellations left a process alive in the \
             command\'s group; one signal is not enough because the kernel\'s \
             walk of the group can pass a member that is still being forked"
        );
    }

    /// The timeout ending gets the same sweep as the cancelled one, because
    /// both go through `stop`.
    ///
    /// This does not claim the timeout path *cannot* enter the fork window: a
    /// spawn returning is not the same instant as the shell finishing its
    /// forks, and nothing here measures the gap between them. The claim worth
    /// testing is the one that holds either way -- whichever reason xfx had for
    /// killing the command, the group is left gone.
    #[cfg(unix)]
    #[test]
    fn a_timed_out_command_gets_the_same_sweep_as_a_cancelled_one() {
        let tag = format!("30.{}", std::process::id());
        let mut child = a_forking_shell(&tag, 50);
        let group = rustix::process::Pid::from_raw(child.id() as i32).expect("a group");
        std::thread::sleep(Duration::from_millis(10));

        // A timeout that has already run out drives the timeout branch on the
        // first poll, with the command still forking.
        let ending = supervise(&mut child, Duration::from_millis(0), &CancelToken::new());

        assert!(
            matches!(ending, Ending::TimedOut { stranded: None, .. }),
            "{ending:?}"
        );
        assert_eq!(
            rustix::process::test_kill_process_group(group),
            Err(rustix::io::Errno::SRCH),
            "the timeout path left the command's process group behind"
        );
    }
}
