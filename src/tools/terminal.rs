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
//! is built rather than inherited so that fxr's own Gateway credential cannot
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

/// How long fxr will wait for a killed command's pipes to close.
///
/// A child that spawns a grandchild and exits leaves the grandchild holding the
/// inherited stdout. Killing the process group normally closes it; a process
/// that escaped its group with `setsid` will not. Waiting forever for that pipe
/// would turn "the command finished" into "fxr hangs", so the wait is bounded
/// and the shortfall is disclosed in the output.
const DRAIN_GRACE: Duration = Duration::from_millis(1_500);

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
    /// fxr killed it because it ran too long.
    TimedOut(u64),
    /// fxr killed it because the turn was cancelled.
    Cancelled,
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
        out.push_str(&format!("<command>{}</command>\n", plan.command()));
        out.push_str(&format!("<cwd>{}</cwd>\n", plan.display_cwd()));
        match self.ending {
            Ending::Exited(code) => out.push_str(&format!("<exit_code>{code}</exit_code>\n")),
            Ending::Signalled(signal) => out.push_str(&format!("<signal>{signal}</signal>\n")),
            Ending::TimedOut(ms) => {
                return ToolResult::failure(format!(
                    "terminal timed out: `{}` ran longer than {ms} ms and was killed\n{}",
                    plan.command(),
                    self.streams()
                ))
            }
            Ending::Cancelled => {
                return ToolResult::failure(format!(
                    "terminal was cancelled: `{}` was killed before it finished\n{}",
                    plan.command(),
                    self.streams()
                ))
            }
        }
        out.push_str(&self.streams());

        let detail = match self.ending {
            Ending::Exited(code) => format!("{} (exit {code})", plan.command()),
            Ending::Signalled(signal) => format!("{} (signal {signal})", plan.command()),
            // Both are returned above.
            Ending::TimedOut(_) | Ending::Cancelled => unreachable!(),
        };
        // A command that ran and reported a nonzero status answered the
        // question. Only fxr's own failures are refusals.
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

/// One captured stream, bounded in bytes and in how long fxr waited for it.
struct Captured {
    bytes: Vec<u8>,
    /// How many bytes were produced past the bound.
    dropped: usize,
    /// Whether the stream was still open when fxr stopped waiting.
    stalled: bool,
}

impl Captured {
    fn render(&self) -> String {
        let mut out = String::from_utf8_lossy(&self.bytes).into_owned();
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
                "... [stream still open; a process outlived the command and fxr stopped waiting]\n",
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
    // Built, not inherited: whatever is in fxr's environment stays there.
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
    // process the command started and not only the one fxr forked. `sh -c 'x &'`
    // exits immediately while `x` keeps running; without a group kill, `x`
    // survives the turn and keeps fxr's pipe open.
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
            stop(child);
            return Ending::Cancelled;
        }
        if started.elapsed() >= timeout {
            stop(child);
            return Ending::TimedOut(timeout.as_millis() as u64);
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Kills everything the command started, and reaps the child fxr forked.
///
/// The group is signalled first, because the process holding fxr's pipe is very
/// often not the process fxr forked. The direct kill follows as a backstop for
/// the case where the group could not be signalled at all.
fn stop(child: &mut Child) {
    #[cfg(unix)]
    {
        if let Some(pid) = rustix::process::Pid::from_raw(child.id() as i32) {
            let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
        }
    }
    let _ = child.kill();
    let _ = child.wait();
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

        // A stream fxr stopped waiting for says so, so "no more output" is
        // distinguishable from "output fxr never saw".
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
        // fxr chose not to show".
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
}
