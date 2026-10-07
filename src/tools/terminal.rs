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
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::gateway::CancelToken;
use crate::permission::{CommandPlan, CommandRoute, Decided, PolicyDecision, ProposedAction};

use super::spec::{
    nonblank, object, optional_string, required_string, Activity, InputSchema,
    Outcome as CallOutcome, PermissionKind, Presentation, Property, PropertyKind, ToolContext,
    ToolInput, ToolResult, ToolSpec,
};

/// How often a running command notices a timeout or a cancellation.
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// How long a command its turn stopped is given to end on the `SIGTERM` it is
/// sent first, before the group is killed: upstream's
/// `foreground_target_termination_grace_ms` (`vercel-labs/fx@c1db9191
/// src/core/execution/command_runner.zig:85`).
const TERMINATION_GRACE: Duration = Duration::from_millis(700);

/// How long xfx will wait for a killed command's pipes to close.
///
/// A child that spawns a grandchild and exits leaves the grandchild holding the
/// inherited stdout. Killing the process group normally closes it; a process
/// that escaped its group with `setsid` will not. Waiting forever for that pipe
/// would turn "the command finished" into "xfx hangs", so the wait is bounded
/// and the shortfall is disclosed in the output.
const DRAIN_GRACE: Duration = Duration::from_millis(1_500);

/// How many more times, at most, a killed command's process group is signalled
/// once its leader has exited and before the leader is collected.
///
/// Two rounds, not a duration: this is a bound on work, not a promise about
/// wall-clock time, which a loaded machine does not let anyone make. What the
/// rounds wait for before they start is an event, the leader's exit, and not a
/// clock; see `sweep_anchored`.
const ANCHORED_ROUNDS: u32 = 2;

// A sweep of zero rounds would signal nothing after the leader's exit, which is
// the escape `sweep_anchored` exists to close.
const _: () = assert!(ANCHORED_ROUNDS > 0);

/// How many times the group is *asked* whether it is gone before xfx stops
/// waiting for it. Also a round bound rather than a time one; nothing is
/// signalled in these rounds, so their only cost is the wait.
const GROUP_SETTLE_ROUNDS: u32 = 20;

// `settled` reports "gone" if it never gets to ask, so asking at least once is
// part of the type rather than part of the reading.
const _: () = assert!(GROUP_SETTLE_ROUNDS > 0);

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
    Presentation::new(Activity::Command, "Running", "Ran", "command"),
    InputSchema {
        properties: &[
            Property {
                name: "action",
                kind: PropertyKind::String,
                description: "The only supported action is exec: run one command and capture its result.",
                allowed: &[EXEC_ACTION],
                array: None,
            },
            Property {
                name: "command",
                kind: PropertyKind::String,
                description: "The command to run, as one line.",
                allowed: &[],
                array: None,
            },
            Property {
                name: "cwd",
                kind: PropertyKind::String,
                description:
                    "Directory to run in, inside an authorized root. Defaults to the workspace root.",
                allowed: &[],
                array: None,
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
    // same lock. `feedback` is what the user said while answering, owned by
    // this call and carried by whichever result it returns -- including the
    // ones that fail after the decision.
    let Decided { decision, feedback } = context
        .permissions()
        .decide_with_feedback(ProposedAction::Command(&plan));
    let source = match decision {
        PolicyDecision::Allow { source } => source,
        PolicyDecision::Deny { reason, .. } => {
            return ToolResult::refused("terminal", &reason).with_feedback(feedback)
        }
        PolicyDecision::Prompt => {
            return ToolResult::refused("terminal", "the approval was never resolved")
                .with_feedback(feedback)
        }
    };

    // The plan that was judged, unchanged: a sentence is not an argument.
    let authority = context.permissions().mint_command(plan, source);
    // Spent before it is used, so a failure cannot be retried on the same
    // approval.
    if let Err(err) = context.permissions().consume(&authority) {
        return ToolResult::revoked(format!("terminal could not use its authority: {err}"))
            .with_feedback(feedback);
    }
    let plan = authority
        .command()
        .expect("a command authority carries a command plan");

    match run(plan, context) {
        Ok(outcome) => outcome.into_result(plan).with_feedback(feedback),
        Err(reason) => ToolResult::failure(format!("terminal could not run the command: {reason}"))
            .with_feedback(feedback),
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
    /// xfx stopped it because the turn was cancelled, and how it ended:
    /// the signal it died of, when it was one.
    Cancelled {
        stranded: Option<Stranded>,
        status: Option<std::process::ExitStatus>,
    },
}

/// Why xfx could not confirm that a killed command's process group was gone.
///
/// The kernel's own answer, kept as an answer rather than flattened to a bool:
/// "something is still running in there" and "the question could not be asked"
/// are different failures, and a report that cannot tell them apart is the
/// silent path this sweep exists to close.
///
/// It is a claim that something may still be running, so it is made only about
/// a member that could be: one that is neither dead nor dying and that xfx did
/// not send `SIGKILL` to. A killed process that has not finished exiting, and
/// a zombie its reaper has not collected, keep a group in existence for a
/// while on a loaded machine; neither will run another instruction, and
/// reporting either as a survivor told the model that a process had escaped
/// when none had (`reconsider`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stranded {
    /// A member that is not dead, not exiting, and was never sent `SIGKILL`
    /// by xfx, and that xfx may signal. Where the kernel cannot be asked
    /// member by member, `kill(-pgid, 0)` succeeding is reported as this.
    Running,
    /// `EPERM`: a member xfx is not permitted to signal, so no kill of xfx's
    /// can have reached it. Where the kernel cannot be asked member by member,
    /// the group answering `kill(-pgid, 0)` with `EPERM` -- which per
    /// `kill(2)` "is returned if any members of the group could not be
    /// signaled" -- is reported as this.
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
        let streams = self.streams();
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
                .ending(CallOutcome::TimedOut)
            }
            // Stopped by the user under its turn: upstream's answer for the
            // call, `aborted by user` and nothing of the command's output
            // (`aborted_tool_output`, `vercel-labs/fx@c1db9191
            // src/core/session/session.zig:64`, `:82-108`; fx's capture
            // `fx-interrupt-next-cmd-40x120-bodies/003-completion.json`). The one
            // thing kept is xfx's own: a group that could not be emptied is
            // still said, because something the command started may be running.
            //
            // The person watching is shown what upstream's full transcript
            // shows of it -- the output it wrote and how it ended, `signal 15`
            // for one that stopped when asked (`fx-resume-tools-40x120-
            // live-ctrl-o`) -- in the frame a finished command's result is
            // in, so the screen reads it as one ([`ToolResult::presented`]).
            Ending::Cancelled { stranded, status } => {
                let notice = stranded_notice(stranded);
                let output = if notice.is_empty() {
                    crate::tools::question::ABORTED_OUTPUT.to_string()
                } else {
                    format!("{}\n{notice}", crate::tools::question::ABORTED_OUTPUT)
                };
                let result = ToolResult::failure(output).ending(CallOutcome::Cancelled);
                let status = match status.map(ended) {
                    Some(Ending::Exited(code)) => format!("<exit_code>{code}</exit_code>\n"),
                    Some(Ending::Signalled(signal)) => format!("<signal>{signal}</signal>\n"),
                    _ => return result,
                };
                out.push_str(&status);
                out.push_str(&streams);
                return result.presenting(out);
            }
        }
        out.push_str(&self.streams());

        let (detail, outcome) = match self.ending {
            Ending::Exited(code) => (
                format!("{} (exit {code})", plan.command()),
                CallOutcome::Exited(code),
            ),
            Ending::Signalled(signal) => (
                format!("{} (signal {signal})", plan.command()),
                CallOutcome::Signalled(signal),
            ),
            // Both are returned above.
            Ending::TimedOut { .. } | Ending::Cancelled { .. } => unreachable!(),
        };
        // A command that ran and reported a nonzero status answered the
        // question. Only xfx's own failures are refusals -- which is the
        // model's reading; the person watching is told the status
        // ([`CallOutcome::Exited`]), because a row that said `Ran` over an exit 3
        // would be the screen claiming a success the command did not have.
        ToolResult::success(out, detail).ending(outcome)
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
            let mut command = crate::process::command(&argv[0]);
            command.args(&argv[1..]);
            command
        }
        CommandRoute::Shell { program } => {
            let mut command = crate::process::command(program);
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
            // Asked first, as upstream asks (`terminationSignalPlan`,
            // `command_runner.zig:2473-2493`: `SIGTERM` to the group for a
            // cooperative stop, `SIGKILL` once its grace is spent), then
            // killed as a timeout kills it, which collects how it ended.
            terminate(child);
            let (stranded, status) = stop_collecting(child);
            return Ending::Cancelled { stranded, status };
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

/// Sends the command's group `SIGTERM` and gives its leader
/// [`TERMINATION_GRACE`] to exit, without collecting it: [`stop`]'s sweep
/// needs the leader uncollected, and kills whatever is left either way.
fn terminate(child: &Child) {
    #[cfg(unix)]
    {
        use rustix::process::{waitid, WaitId, WaitIdOptions};
        let Some(group) = rustix::process::Pid::from_raw(child.id() as i32) else {
            return;
        };
        if rustix::process::kill_process_group(group, rustix::process::Signal::TERM).is_err() {
            return;
        }
        let deadline = Instant::now() + TERMINATION_GRACE;
        while Instant::now() < deadline {
            match waitid(
                WaitId::Pid(group),
                WaitIdOptions::EXITED | WaitIdOptions::NOWAIT | WaitIdOptions::NOHANG,
            ) {
                Ok(Some(_)) => return,
                Err(rustix::io::Errno::INTR) | Ok(None) => std::thread::sleep(POLL_INTERVAL),
                Err(_) => return,
            }
        }
    }
    #[cfg(not(unix))]
    let _ = child;
}

/// [`stop`], answering how the child it reaped ended as well.
fn stop_collecting(child: &mut Child) -> (Option<Stranded>, Option<std::process::ExitStatus>) {
    let mut status = None;
    let stranded = stop_with(child, &mut status);
    (stranded, status)
}

/// Kills everything the command started, and reaps the child xfx forked.
///
/// The group is signalled first, because the process holding xfx's pipe is very
/// often not the process xfx forked. The direct kill follows as a backstop for
/// the case where the group could not be signalled at all. Then xfx waits for
/// the leader to exit without collecting it, and signals the group again --
/// see `sweep_anchored` for why once is not enough and why the wait comes
/// first -- and only after that is the leader collected and the group asked
/// whether it is gone.
///
/// Returns `None` when the group is gone or holds nothing that can run again,
/// and otherwise the kernel's reason for saying something might. That answer
/// is not a detail to swallow: it means something the command started may
/// still be running, still holding the pipes xfx handed it, after xfx did
/// everything it can do about it -- and the caller says so in the tool's own
/// output.
fn stop(child: &mut Child) -> Option<Stranded> {
    stop_with(child, &mut None)
}

/// [`stop`], keeping the reaped child's status in `status`.
fn stop_with(child: &mut Child, status: &mut Option<std::process::ExitStatus>) -> Option<Stranded> {
    #[cfg(unix)]
    let group = rustix::process::Pid::from_raw(child.id() as i32);

    #[cfg(unix)]
    if let Some(group) = group {
        let _ = rustix::process::kill_process_group(group, rustix::process::Signal::KILL);
    }
    let _ = child.kill();

    // After the leader has exited, before it is collected, never after: see
    // `sweep_anchored`.
    #[cfg(unix)]
    let killed = match group {
        Some(group) => {
            exited_uncollected(group);
            sweep_anchored(group)
        }
        None => Killed::new(),
    };

    *status = child.wait().ok();

    #[cfg(unix)]
    let stranded = match group {
        Some(group) => settled(group, &killed),
        None => None,
    };
    #[cfg(not(unix))]
    let stranded = None;

    stranded
}

/// Blocks until the command's leader has exited, and leaves it uncollected.
///
/// `waitid` with `WNOWAIT` reports the exit and keeps the zombie, so the
/// leader's id -- which is the group's id -- stays out of the allocator for
/// the sweep that follows, and `child.wait()` still does the collecting. This
/// is not a new wait: `child.wait()` already blocked on this same exit, and a
/// leader that never dies hung it exactly as it hangs this. What changes is
/// which side of the exit the sweep runs on.
///
/// `EINTR` asks again, because the TUI installs its signal handlers without
/// `SA_RESTART`. Any other error is the previous behaviour -- the sweep runs
/// without having waited -- rather than no sweep at all.
#[cfg(unix)]
fn exited_uncollected(leader: rustix::process::Pid) {
    use rustix::process::{waitid, WaitId, WaitIdOptions};

    while let Err(rustix::io::Errno::INTR) = waitid(
        WaitId::Pid(leader),
        WaitIdOptions::EXITED | WaitIdOptions::NOWAIT,
    ) {}
}

/// Signals `group` again, up to `ANCHORED_ROUNDS` times, after the command's
/// leader has exited and before it is collected.
///
/// **One signal is not enough, and the reason is in the kernel rather than in
/// xfx's bookkeeping.** On macOS, `kill(-pgid)` copies the group's member list
/// and then takes a reference on each member before signalling it. A child
/// that a `fork` has already put on that list but not yet finished creating
/// refuses the reference and is skipped, without an error: XNU
/// `bsd/kern/kern_proc.c` `pgrp_iterate` → `proc_find` → `proc_ref_try_fast`,
/// which fails while `P_REF_NEW` is set; `forkproc` sets it when it allocates
/// the child, before `pgrp_enter_locked` joins it to the group, and only
/// `pinsertchild` clears it, after the child's task has been created
/// (`bsd/kern/kern_fork.c`). That member never receives the signal. It keeps
/// running, and it keeps the stdout and stderr pipes xfx handed the command
/// open, which is exactly the "stream still open" the drain then has to
/// report. Measured on macOS, with one signal: 51 of 240 cancellations five
/// milliseconds into a twenty-child fork loop left a child alive, and 91 of 160
/// cancellations forty milliseconds into a three-hundred-child loop. Linux has
/// no such window: `copy_process` abandons a fork when a fatal signal is
/// pending and hands a group signal that arrives mid-fork to the child
/// (`kernel/fork.c`).
///
/// **The leader's exit comes first because it closes the window for the
/// leader's own forks.** A process cannot exit from inside `fork`: the pending
/// `SIGKILL` is acted on when the call returns, and the child is a finished
/// member by then. So once the leader has exited, every child it forked is a
/// member a signal reaches. The sweep used to run straight after the first
/// signal, two rounds two milliseconds apart -- a bound that held on an idle
/// machine and not on a loaded one, where a leader descheduled inside `fork`
/// finished it after the last round, exited, and left its child running. With
/// 144 forking commands stopped at once on 18 cores, 2 to 7 of 1152 stops left
/// a child that was never killed, in each of six runs; waiting for the exit
/// first, 0 in twelve. The rounds after the exit are for members that fork
/// too. One that finds nothing
/// left to signal ends the sweep: macOS's `kill` skips zombies and answers
/// `EPERM` when that leaves nobody (`killpg1`, `bsd/kern/kern_sig.c`). Linux
/// counts the zombie leader as signalled, so there every round runs.
///
/// **Every signal xfx sends to the group is sent before the leader is
/// collected, and that ordering is the whole safety argument.** A process
/// group id is a process id, and process ids are recycled. The id cannot be
/// recycled while this function runs, because the leader has been killed but
/// not yet waited for: POSIX ends a process's lifetime -- and only then returns
/// its id to the system -- when it is waited for, not when it dies, and does
/// not reuse a process group id until the group's lifetime ends (POSIX.1-2017
/// XBD 3.301, 4.14). XNU keeps the same promise in code: its pid allocator
/// skips any id still in the process or the process-group hash, and a member
/// leaves its group when it is collected, not when it dies (`forkproc`;
/// `reap_child_locked` → `pgrp_leave_locked`, `bsd/kern/kern_exit.c`).
/// Measured here rather than assumed: on macOS a group whose only member is an
/// unreaped leader answers `kill(-pgid, 0)` with `EPERM`, not `ESRCH`, which
/// per `kill(2)` "is returned if any members of the group could not be
/// signaled" -- the group is still there. So a signal sent in this window
/// provably reaches this command's group and nothing else.
///
/// After the leader is collected xfx signals nothing further, however the
/// checks in `settled` come out. A group that still had a member would be as
/// safe to signal then -- its id is not reused while it exists -- but xfx
/// cannot know that it still has one at the instant the signal is sent: asking
/// first and signalling second leaves a window in which the last member is
/// collected and the id handed out again. The unreaped leader is the one
/// member whose presence xfx controls, which is why every signal is sent while
/// it is held.
///
/// Returns who it killed, for `settled` to tell a member that is still exiting
/// from one that was never signalled (`Killed`).
///
/// The record is taken after the rounds, not inside them, and paid for with
/// one more signal. Listing the group before each round's signal delays that
/// signal by the listing, and measured against the rounds left as they were,
/// that delay is not free: with four forking shells inside each of 144
/// commands stopped at once, children that outlived the stop went from 17 to
/// 76 in 4608 stops. Listed after the rounds and signalled once more, the
/// rounds keep their timing and the record is still of members a signal
/// reached.
#[cfg(unix)]
fn sweep_anchored(group: rustix::process::Pid) -> Killed {
    for round in 0..ANCHORED_ROUNDS {
        // No pause before the first round: the leader has exited, so the
        // member its last `fork` made is already there to be signalled.
        if round > 0 {
            std::thread::sleep(GROUP_KILL_PAUSE);
        }
        if rustix::process::kill_process_group(group, rustix::process::Signal::KILL).is_err() {
            // Nothing in the group that a signal can find, so nothing for a
            // record to vouch for.
            return Killed::new();
        }
    }
    // Listed before the signal and recorded only once it has been sent: see
    // `Killed` for why that order is what makes the record true.
    let present = listed_before_a_kill(group);
    match rustix::process::kill_process_group(group, rustix::process::Signal::KILL) {
        Ok(()) => present.into_iter().collect(),
        Err(_) => Killed::new(),
    }
}

/// The members a group signal was sent to, by identity.
///
/// macOS will not say whether a process has a `SIGKILL` pending: `kinfo_proc`
/// and `proc_bsdinfo` carry its state and whether it has begun to exit
/// (`P_LEXIT`, set by the dying process itself in `exit_with_reason`,
/// `bsd/kern/kern_exit.c`), and nothing about signals. A member that has been
/// killed but has not yet been scheduled to act on it therefore looks exactly
/// like one that was never signalled. What tells them apart is a record of
/// who was there when xfx signalled.
///
/// The record is true because of what XNU lists. `proc_listpgrppids` walks
/// `allproc` and `zombproc` (`proc_listpids`, `bsd/kern/proc_info.c`), and a
/// forked child enters `allproc` in `pinsertchild` under the same lock that
/// clears `P_REF_NEW` (`bsd/kern/kern_proc.c`) -- so a listed member is never
/// one the group walk would skip as still being created. Listed before the
/// signal, it is a member that signal reached, or one that had already begun
/// to exit or left the group, or one xfx may not signal; `reconsider` asks
/// about the last of those again rather than trusting the record. A process
/// keeps its start time across `exec` (`forkproc`, `bsd/kern/kern_fork.c`),
/// so the identity survives the `sleep` a shell's child becomes.
///
/// Linux needs no record: `/proc/<pid>/status` shows the pending `SIGKILL`
/// itself (`procfs_members`), so the set stays empty there.
#[cfg(unix)]
type Killed = std::collections::HashSet<Identity>;

/// A process, named so that a recycled pid is a different name.
#[cfg(unix)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Identity {
    pid: i32,
    /// When it started, in the kernel's own units.
    started: u64,
}

/// One member of a stopped command's group, as the kernel describes it.
#[cfg(unix)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Member {
    /// It will run no more code: a zombie, a process that has begun to exit,
    /// or one the kernel shows with `SIGKILL` pending.
    Finished,
    /// Anything else -- running, sleeping, or killed in a way only `Killed`
    /// can vouch for.
    Live(Identity),
}

#[cfg(target_os = "macos")]
fn listed_before_a_kill(group: rustix::process::Pid) -> Vec<Identity> {
    members(group)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|member| match member {
            Member::Live(identity) => Some(identity),
            Member::Finished => None,
        })
        .collect()
}

#[cfg(all(unix, not(target_os = "macos")))]
fn listed_before_a_kill(_: rustix::process::Pid) -> Vec<Identity> {
    Vec::new()
}

/// Every member of `group`, or `None` when the kernel could not be asked.
#[cfg(target_os = "macos")]
fn members(group: rustix::process::Pid) -> Option<Vec<Member>> {
    darwin_members(group.as_raw_nonzero().get())
}

#[cfg(target_os = "linux")]
fn members(group: rustix::process::Pid) -> Option<Vec<Member>> {
    procfs_members(std::path::Path::new("/proc"), group.as_raw_nonzero().get())
}

/// Elsewhere the kernel's group answer is all there is, and `reconsider`
/// keeps it.
#[cfg(all(unix, not(any(target_os = "macos", target_os = "linux"))))]
fn members(_: rustix::process::Pid) -> Option<Vec<Member>> {
    None
}

/// `PROC_FLAG_INEXIT`, "process is working its way in exit()"
/// (`bsd/sys/proc_info.h`); the `libc` crate does not carry it.
#[cfg(target_os = "macos")]
const PROC_FLAG_INEXIT: u32 = 4;

/// The members of `group`, from `proc_listpgrppids` and one
/// `proc_pidinfo(PROC_PIDTBSDINFO)` each.
///
/// A listed pid that `proc_pidinfo` answers with `ESRCH` is a member that has
/// already begun to die. `proc_pidinfo` finds a process with `proc_find`
/// unless its argument asks for zombies, and `proc_find` refuses a process
/// once `proc_prepareexit` has moved it to `zombproc` and marked it
/// `P_REF_DEAD` (`bsd/kern/kern_exit.c`); it is the same refusal that keeps
/// `kill(-pgid)` from counting it. The zombie lookup is not used: it takes the
/// zombie's `P_LIST_WAITING` (`proc_find_zombref`), which makes its reaper wait
/// on xfx.
///
/// What the listing cannot show is a member it skips: `proc_listpids` passes
/// over shadow processes -- the husk an `exec` leaves behind, which stays in
/// its group until the kernel reaps it (`proc_exit`, `bsd/kern/kern_exit.c`)
/// -- and a forked child is not on `allproc` until it is finished. See
/// `reconsider` for what that means for a group that lists nobody.
///
/// `proc_listpgrppids` returns a count, and turns a failed system call into
/// zero (`libsyscall/wrappers/libproc/libproc.c`) with `errno` left set. So
/// `errno` is cleared before the call and read after an empty answer: an empty
/// listing is only an answer if nothing failed.
#[cfg(target_os = "macos")]
fn darwin_members(group: libc::pid_t) -> Option<Vec<Member>> {
    let mut pids: Vec<libc::pid_t> = vec![0; 16];
    let listed = loop {
        let bytes = libc::c_int::try_from(std::mem::size_of_val(pids.as_slice())).ok()?;
        // SAFETY: `__error` returns this thread's `errno`, which is always a
        // valid, writable `int`.
        unsafe { *libc::__error() = 0 };
        // SAFETY: `pids` is an initialized buffer of exactly `bytes` bytes that
        // this frame owns, and the call writes at most `bytes` bytes into it.
        let count = unsafe { libc::proc_listpgrppids(group, pids.as_mut_ptr().cast(), bytes) };
        if count == 0 && std::io::Error::last_os_error().raw_os_error() != Some(0) {
            return None;
        }
        let count = usize::try_from(count).ok()?;
        // A full buffer may have been cut short. The kernel lists at most its
        // process count plus twenty, so doubling ends.
        if count < pids.len() {
            break count;
        }
        pids.resize(pids.len() * 2, 0);
    };

    let mut members = Vec::with_capacity(listed);
    for &pid in &pids[..listed] {
        let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::zeroed();
        let size = libc::c_int::try_from(std::mem::size_of::<libc::proc_bsdinfo>()).ok()?;
        // SAFETY: `info` is `size` bytes of zeroed, writable storage owned by
        // this frame, and the call writes at most `size` bytes into it.
        let wrote = unsafe {
            libc::proc_pidinfo(
                pid,
                libc::PROC_PIDTBSDINFO,
                0,
                info.as_mut_ptr().cast(),
                size,
            )
        };
        if wrote != size {
            match std::io::Error::last_os_error().raw_os_error() {
                Some(libc::ESRCH) => {
                    members.push(Member::Finished);
                    continue;
                }
                _ => return None,
            }
        }
        // SAFETY: `proc_bsdinfo` is plain integers and arrays of them, so the
        // zeroed storage was already a valid value, and the kernel has written
        // the whole of it.
        let info = unsafe { info.assume_init() };
        // Left the group since it was listed.
        if i64::from(info.pbi_pgid) != i64::from(group) {
            continue;
        }
        members.push(
            if info.pbi_status == libc::SZOMB || info.pbi_flags & PROC_FLAG_INEXIT != 0 {
                Member::Finished
            } else {
                Member::Live(Identity {
                    pid,
                    started: info
                        .pbi_start_tvsec
                        .saturating_mul(1_000_000)
                        .saturating_add(info.pbi_start_tvusec),
                })
            },
        );
    }
    Some(members)
}

/// `PF_EXITING`, "getting shut down" (`include/linux/sched.h`), set by
/// `exit_signals` as `do_exit` begins.
#[cfg(unix)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const PF_EXITING: u64 = 0x4;

/// `SIGKILL`'s bit in the masks `/proc/<pid>/status` prints, where signal `n`
/// is bit `n - 1` (`render_sigset_t`, `fs/proc/array.c`).
#[cfg(unix)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const SIGKILL_PENDING: u64 = 1 << (libc::SIGKILL as u32 - 1);

/// The members of `group`, from `/proc/<pid>/stat` and `/proc/<pid>/status`.
///
/// Linux lists no group's members, so every process is read and the ones in
/// `group` kept. A member is finished when its state is `Z` or `X`, when
/// `PF_EXITING` is set, or when `SIGKILL` is pending: a group `SIGKILL` lands
/// in the shared pending set and in every thread's
/// (`__send_signal_locked`, `complete_signal`, `kernel/signal.c`), and
/// `get_signal` takes it back out of the thread's set only on the way into
/// `do_group_exit`, leaving the shared one -- so from the kill until the
/// process is released one of those three is visible.
///
/// Compiled everywhere and called only on Linux, so that the parsing is
/// tested on every platform the suite runs on (`root` is a parameter for the
/// same reason).
#[cfg(unix)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn procfs_members(root: &std::path::Path, group: i32) -> Option<Vec<Member>> {
    let mut members = Vec::new();
    for entry in std::fs::read_dir(root).ok()? {
        let entry = entry.ok()?;
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<i32>().ok())
        else {
            continue;
        };
        let dir = entry.path();
        // Gone since the directory was read: not a member of anything.
        let Some(stat) = procfs_read(&dir.join("stat"))? else {
            continue;
        };
        let stat = ProcStat::parse(&stat)?;
        if stat.pgrp != group {
            continue;
        }
        if stat.finished() {
            members.push(Member::Finished);
            continue;
        }
        // Gone between the two reads: it was a member, and it is finished.
        let Some(status) = procfs_read(&dir.join("status"))? else {
            members.push(Member::Finished);
            continue;
        };
        members.push(if sigkill_pending(&status)? {
            Member::Finished
        } else {
            Member::Live(Identity {
                pid,
                started: stat.started,
            })
        });
    }
    Some(members)
}

/// `Some(None)` for a process that has gone since it was listed, `None` for a
/// file that could not be read for any other reason.
#[cfg(unix)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn procfs_read(path: &std::path::Path) -> Option<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(text) => Some(Some(text)),
        Err(err)
            if err.kind() == std::io::ErrorKind::NotFound
                || err.raw_os_error() == Some(libc::ESRCH) =>
        {
            Some(None)
        }
        Err(_) => None,
    }
}

/// The fields of `/proc/<pid>/stat` that `procfs_members` reads.
#[cfg(unix)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProcStat {
    state: char,
    pgrp: i32,
    flags: u64,
    started: u64,
}

#[cfg(unix)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
impl ProcStat {
    /// Fields 3, 5, 9 and 22 of `proc(5)`: state, pgrp, flags, starttime.
    ///
    /// The command name, field 2, is in parentheses and may itself contain
    /// spaces and parentheses, so the fields are counted from the last `)`.
    fn parse(text: &str) -> Option<ProcStat> {
        let (_, rest) = text.rsplit_once(')')?;
        let fields: Vec<&str> = rest.split_whitespace().collect();
        Some(ProcStat {
            state: fields.first()?.chars().next()?,
            pgrp: fields.get(2)?.parse().ok()?,
            flags: fields.get(6)?.parse().ok()?,
            started: fields.get(19)?.parse().ok()?,
        })
    }

    fn finished(&self) -> bool {
        matches!(self.state, 'Z' | 'X' | 'x') || self.flags & PF_EXITING != 0
    }
}

/// Whether `/proc/<pid>/status` shows `SIGKILL` pending, for the thread or
/// for the whole process; `None` if it shows neither mask.
#[cfg(unix)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn sigkill_pending(status: &str) -> Option<bool> {
    let mut seen = false;
    let mut pending = 0;
    for line in status.lines() {
        if let Some(mask) = line
            .strip_prefix("SigPnd:")
            .or_else(|| line.strip_prefix("ShdPnd:"))
        {
            pending |= u64::from_str_radix(mask.trim(), 16).ok()?;
            seen = true;
        }
    }
    seen.then_some(pending & SIGKILL_PENDING != 0)
}

/// Asks whether `group` is gone, `GROUP_SETTLE_ROUNDS` times at most and
/// exactly that many if it never is. Sends nothing. Returns `None` once it is
/// gone, and otherwise the reason the **last** check gave.
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
///
/// A group still there after the last question is then looked at member by
/// member (`linger`, `reconsider`), because the group's own answer cannot say
/// *what* is still there.
#[cfg(unix)]
fn settled(group: rustix::process::Pid, killed: &Killed) -> Option<Stranded> {
    let answer = settle(|| rustix::process::test_kill_process_group(group))?;
    linger(|| {
        reconsider(
            answer,
            members(group),
            killed,
            // Linux lists every process its reader may see, and `kill` skips
            // nothing it lists, so there an unlisted member is one xfx may not
            // see -- another user's -- and the group's `EPERM` is the truth
            // about it.
            cfg!(target_os = "macos"),
            |pid| match rustix::process::Pid::from_raw(pid) {
                Some(pid) => rustix::process::test_kill_process(pid),
                None => Err(rustix::io::Errno::SRCH),
            },
            || rustix::process::test_kill_process_group(group),
        )
    })
}

/// What one look at a stopped command's members found.
#[cfg(unix)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Reading {
    /// The notice the members justify, if any.
    stranded: Option<Stranded>,
    /// Whether a member xfx killed has not yet begun to exit.
    dying: bool,
}

/// Looks at the members again, `GROUP_SETTLE_ROUNDS` times at most, while a
/// member xfx killed has not yet begun to exit, and returns what the last look
/// found.
///
/// Such a member cannot run another instruction of its own, but it may be
/// inside `fork`, which it finishes before it can act on the signal -- and the
/// child it makes was never signalled, and is not listed until it is finished
/// (`darwin_members`). The first look cannot see that child; a look after its
/// parent has begun to exit can. Measured with four forking shells inside each
/// of 144 commands stopped at once on 18 cores, in runs of 11520 stops: the
/// group's own answer, which calls every member `kill` can find a survivor,
/// disclosed every stop that left a child alive (34 of 34, 43 of 43) along
/// with 112 and 109 in which nothing had; one look at the members disclosed 39
/// of 47 and nothing else; looking until the killed have begun to exit, 29 of
/// 30 and nothing else.
///
/// A member still dying after the last look is not reported: everything xfx
/// can see has been sent `SIGKILL`, and a notice would be about a child that
/// may never exist. That is the one case this can miss, and it is bounded by
/// the rounds, as the wait is.
///
/// `read` is a parameter for the reason `settle`'s `ask` is.
#[cfg(unix)]
fn linger(mut read: impl FnMut() -> Reading) -> Option<Stranded> {
    let mut stranded = None;
    for round in 0..GROUP_SETTLE_ROUNDS {
        if round > 0 {
            std::thread::sleep(GROUP_KILL_PAUSE);
        }
        let reading = read();
        stranded = reading.stranded;
        // A running member is already the worst answer; nothing a later look
        // finds can change what the notice has to say.
        if !reading.dying || stranded == Some(Stranded::Running) {
            break;
        }
    }
    stranded
}

/// Turns the group's last answer into a claim about its members.
///
/// `kill(-pgid, 0)` reports on the group as a whole, and under load the group
/// outlasts `settle`'s rounds for reasons that are not survivals: on macOS a
/// member xfx killed answers `Ok` until it is scheduled to act on the signal,
/// and members that have begun to exit or are zombies answer `EPERM` until
/// their reaper collects them; on Linux a zombie answers `Ok`. With 144 forking
/// commands stopped at once on 18 cores, `stop` reported a stranded group in 3
/// to 10 of 1152 stops in each of six runs (40 of 6912), both `Running` and
/// `Unsignalable`, and nothing in any of them outlived the kill.
///
/// So the members are listed, and a notice is kept only for one that could
/// still run: a `Member::Live` that xfx did not kill and may signal
/// (`Running`), or one it may not signal at all (`Unsignalable`, whatever the
/// record says, since no kill of xfx's can have reached it). A `Member::Live`
/// that xfx did kill is reported as `dying`, for `linger` to look again. Each
/// look is bounded: one listing, at most one question per member, and one more
/// question of the group if the listing is empty.
///
/// A listing with nobody in it, from a group that still answers, is read by
/// what the group says when asked again:
///
/// * `ESRCH` -- it has just gone.
/// * `EPERM`, where `unlisted_cannot_run` -- the macOS case. The group exists
///   and holds nobody `kill` can find; the listing shows every process `kill`
///   can find and every zombie, so what is left is what it skips, the husks of
///   `exec`s waiting for the kernel to reap them (`darwin_members`). A child
///   still being forked is not among them: its parent is in the group, in
///   `fork`, and findable, so the group would have answered `Ok`. Measured: the
///   8 such groups seen under the load above all answered `ESRCH` on their own
///   42 to 722 milliseconds later.
/// * anything else -- the listing and the group disagree, and the group's own
///   answer stands. So does it when the kernel cannot be asked member by
///   member at all.
///
/// Every input is a parameter, for the reason `settle`'s `ask` is: the states
/// that matter cannot be produced on demand, so the reading is proven against
/// scripted ones (`tests` below).
#[cfg(unix)]
fn reconsider(
    answer: Stranded,
    members: Option<Vec<Member>>,
    killed: &Killed,
    unlisted_cannot_run: bool,
    mut ask_member: impl FnMut(i32) -> Result<(), rustix::io::Errno>,
    mut ask_group: impl FnMut() -> Result<(), rustix::io::Errno>,
) -> Reading {
    let Some(members) = members else {
        return Reading {
            stranded: Some(answer),
            dying: false,
        };
    };
    if members.is_empty() {
        let stranded = match ask_group() {
            Err(rustix::io::Errno::SRCH) => None,
            Err(rustix::io::Errno::PERM) if unlisted_cannot_run => None,
            _ => Some(answer),
        };
        return Reading {
            stranded,
            dying: false,
        };
    }

    let mut verdict: Option<Stranded> = None;
    let mut dying = false;
    for member in members {
        let Member::Live(identity) = member else {
            continue;
        };
        let reason = match ask_member(identity.pid) {
            // Gone since it was listed.
            Err(rustix::io::Errno::SRCH) => continue,
            Err(rustix::io::Errno::PERM) => Stranded::Unsignalable,
            Err(other) => Stranded::Errno(other.raw_os_error()),
            // Sent `SIGKILL` while a listed member, and not yet far enough
            // into dying for the kernel to say so.
            Ok(()) if killed.contains(&identity) => {
                dying = true;
                continue;
            }
            Ok(()) => Stranded::Running,
        };
        // The most serious answer wins: something running outranks something
        // xfx may not touch, which outranks a question that failed.
        verdict = Some(match (verdict, reason) {
            (Some(Stranded::Running), _) | (_, Stranded::Running) => Stranded::Running,
            (Some(Stranded::Unsignalable), _) | (_, Stranded::Unsignalable) => {
                Stranded::Unsignalable
            }
            (_, errno) => errno,
        });
    }
    Reading {
        stranded: verdict,
        dying,
    }
}

/// The rounds and the reading, with the question itself a parameter.
///
/// Exactly `GROUP_SETTLE_ROUNDS` questions are asked, and the reason returned
/// is the **last** answer. An earlier version classified the first twenty and
/// then asked a twenty-first only to throw its answer away, keeping the stale
/// one -- so a group that had just become unsignalable was reported as running,
/// and an unexpected errno on the final ask was reported as whatever the round
/// before had seen. A report of the kernel's answer has to be the answer the
/// kernel last gave.
///
/// `ask` is a parameter because a kernel cannot be made to answer `EPERM`
/// nineteen times and then something unexpected on demand, and a test against a
/// real group can only watch it agree with itself. The ordering and the count
/// are therefore proven against a scripted one (`tests` below); the real
/// question is wired in by `settled` above, and nothing else calls this.
#[cfg(unix)]
fn settle(mut ask: impl FnMut() -> Result<(), rustix::io::Errno>) -> Option<Stranded> {
    let mut answer = None;
    for round in 0..GROUP_SETTLE_ROUNDS {
        // A pause before every question but the first: the previous answer was
        // "still there", and nothing changes without time passing.
        if round > 0 {
            std::thread::sleep(GROUP_KILL_PAUSE);
        }
        answer = match ask() {
            Err(rustix::io::Errno::SRCH) => return None,
            Ok(()) => Some(Stranded::Running),
            Err(rustix::io::Errno::PERM) => Some(Stranded::Unsignalable),
            Err(other) => Some(Stranded::Errno(other.raw_os_error())),
        };
    }
    answer
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
        Stranded::Unsignalable => "a member could not be signalled (EPERM)".to_string(),
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
    use std::cell::Cell;
    use std::process::Command;
    use std::rc::Rc;

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

    /// A scripted `ask`: the answers `settle` will get, in order, and a count
    /// of how many it asked for.
    ///
    /// An answer past the end of the script reads as "still running", so a
    /// version that asks more questions than it was written to ask is caught by
    /// the count rather than by an index panic that says nothing.
    #[cfg(unix)]
    fn scripted(
        answers: Vec<Result<(), rustix::io::Errno>>,
    ) -> (
        impl FnMut() -> Result<(), rustix::io::Errno>,
        Rc<Cell<usize>>,
    ) {
        let asked = Rc::new(Cell::new(0));
        let counter = Rc::clone(&asked);
        let ask = move || {
            let answer = answers.get(counter.get()).copied().unwrap_or(Ok(()));
            counter.set(counter.get() + 1);
            answer
        };
        (ask, asked)
    }

    #[cfg(unix)]
    fn errno(raw: i32) -> rustix::io::Errno {
        rustix::io::Errno::from_raw_os_error(raw)
    }

    /// The reason reported is the last answer, not the one before it, and the
    /// number of questions is the number the constant names.
    ///
    /// This is the test the live-group one below cannot be: a real group agrees
    /// with itself round after round, so "reports the last answer" and "reports
    /// the answer from the round before" are indistinguishable against it. Here
    /// the penultimate answer and the final one differ on purpose, in both
    /// directions, which is the only arrangement that can tell them apart.
    #[cfg(unix)]
    #[test]
    fn settling_reports_the_final_answer_and_asks_exactly_the_rounds_it_names() {
        let rounds = GROUP_SETTLE_ROUNDS as usize;

        // And the case where asking one question too many changes the answer
        // rather than just the count: a group that says `ESRCH` on a
        // twenty-first question is a group this function never asked. The
        // implementation before the fix asked anyway and reported `None` --
        // "everything is gone" -- about a group its own twentieth answer had
        // just called stranded.
        let mut script = vec![Err(rustix::io::Errno::PERM); rounds - 1];
        script.push(Err(errno(42)));
        script.push(Err(rustix::io::Errno::SRCH));
        let (ask, asked) = scripted(script);
        assert_eq!(
            settle(ask),
            Some(Stranded::Errno(42)),
            "an answer to a question this function does not ask changed its report"
        );
        assert_eq!(asked.get(), rounds);

        // Unsignalable all the way, and then something unexpected.
        let mut script = vec![Err(rustix::io::Errno::PERM); rounds - 1];
        script.push(Err(errno(42)));
        let (ask, asked) = scripted(script);
        assert_eq!(
            settle(ask),
            Some(Stranded::Errno(42)),
            "the answer before the last one was reported instead of the last"
        );
        assert_eq!(
            asked.get(),
            rounds,
            "the number of questions asked is not the number `GROUP_SETTLE_ROUNDS` names"
        );

        // And the other way round: an unexpected errno throughout, then EPERM.
        let mut script = vec![Err(errno(42)); rounds - 1];
        script.push(Err(rustix::io::Errno::PERM));
        let (ask, asked) = scripted(script);
        assert_eq!(settle(ask), Some(Stranded::Unsignalable));
        assert_eq!(asked.get(), rounds);

        // A group that is gone is reported the moment it says so, and nothing
        // is asked after that.
        let (ask, asked) = scripted(vec![
            Err(rustix::io::Errno::PERM),
            Err(rustix::io::Errno::SRCH),
        ]);
        assert_eq!(settle(ask), None);
        assert_eq!(asked.get(), 2, "the answer `ESRCH` ends the questioning");
    }

    #[cfg(unix)]
    fn live(pid: i32) -> Member {
        Member::Live(Identity { pid, started: 7 })
    }

    #[cfg(unix)]
    fn killed_set(pids: &[i32]) -> Killed {
        pids.iter()
            .map(|&pid| Identity { pid, started: 7 })
            .collect()
    }

    /// `reconsider` with every kernel answer scripted: `members` is the
    /// listing, `asks` what each member answers `kill(pid, 0)`, and `group`
    /// what the group answers when asked again.
    #[cfg(unix)]
    fn read_members(
        answer: Stranded,
        members: Option<Vec<Member>>,
        killed: &Killed,
        unlisted_cannot_run: bool,
        asks: &[(i32, Result<(), rustix::io::Errno>)],
        group: Result<(), rustix::io::Errno>,
    ) -> Reading {
        reconsider(
            answer,
            members,
            killed,
            unlisted_cannot_run,
            |pid| {
                asks.iter()
                    .find(|(asked, _)| *asked == pid)
                    .map_or(Err(rustix::io::Errno::SRCH), |(_, answer)| *answer)
            },
            || group,
        )
    }

    #[cfg(unix)]
    fn clear() -> Reading {
        Reading {
            stranded: None,
            dying: false,
        }
    }

    /// A group made only of the dead, the dying, and members xfx killed is
    /// not a stranded group, whatever the group as a whole answered.
    #[cfg(unix)]
    #[test]
    fn a_group_of_the_dead_and_the_killed_is_not_reported_as_stranded() {
        let none = Killed::new();
        let ok = Ok(());
        let srch = Err(rustix::io::Errno::SRCH);
        let perm = Err(rustix::io::Errno::PERM);

        // Zombies and processes already exiting, behind either group answer.
        for answer in [Stranded::Running, Stranded::Unsignalable] {
            let members = Some(vec![Member::Finished, Member::Finished]);
            assert_eq!(read_members(answer, members, &none, true, &[], ok), clear());
        }

        // A member xfx killed that has not got far enough to say so: nothing
        // to report, and worth a second look.
        let members = Some(vec![live(10), Member::Finished]);
        assert_eq!(
            read_members(
                Stranded::Running,
                members,
                &killed_set(&[10]),
                true,
                &[(10, ok)],
                ok
            ),
            Reading {
                stranded: None,
                dying: true
            }
        );

        // A listed member that has gone since is nobody.
        let members = Some(vec![live(11)]);
        assert_eq!(
            read_members(Stranded::Running, members, &none, true, &[(11, srch)], ok),
            clear()
        );

        // An empty listing from a group that is gone when asked again.
        for unlisted_cannot_run in [true, false] {
            let reading = read_members(
                Stranded::Unsignalable,
                Some(Vec::new()),
                &none,
                unlisted_cannot_run,
                &[],
                srch,
            );
            assert_eq!(reading, clear());
        }

        // An empty listing from a group that still says `EPERM`: on macOS,
        // only husks the listing skips.
        let reading = read_members(
            Stranded::Unsignalable,
            Some(Vec::new()),
            &none,
            true,
            &[],
            perm,
        );
        assert_eq!(reading, clear());
    }

    /// Everything that could still run is reported, and as what it is.
    #[cfg(unix)]
    #[test]
    fn a_member_that_could_still_run_is_reported_as_what_it_is() {
        let none = Killed::new();
        let ok = Ok(());
        let perm = Err(rustix::io::Errno::PERM);
        let running = |stranded| Reading {
            stranded: Some(stranded),
            dying: false,
        };

        // Never signalled, and signalable: the escape this notice exists for.
        let members = Some(vec![live(20), Member::Finished]);
        assert_eq!(
            read_members(Stranded::Running, members, &none, true, &[(20, ok)], ok),
            running(Stranded::Running)
        );

        // Not signalable is reported as such, even when the record says it was
        // killed: no kill of xfx's can have reached it.
        for killed in [none.clone(), killed_set(&[21])] {
            let members = Some(vec![live(21)]);
            assert_eq!(
                read_members(Stranded::Running, members, &killed, true, &[(21, perm)], ok),
                running(Stranded::Unsignalable)
            );
        }

        // Any other answer is named.
        let members = Some(vec![live(22)]);
        let odd = Err(errno(42));
        assert_eq!(
            read_members(Stranded::Running, members, &none, true, &[(22, odd)], ok),
            running(Stranded::Errno(42))
        );

        // The most serious answer wins, in either order, and a killed member
        // beside it is still noticed as dying.
        let members = Some(vec![live(23), live(24), live(25), live(26)]);
        let asks = [(23, odd), (24, perm), (25, ok), (26, ok)];
        assert_eq!(
            read_members(
                Stranded::Unsignalable,
                members,
                &killed_set(&[26]),
                true,
                &asks,
                ok
            ),
            Reading {
                stranded: Some(Stranded::Running),
                dying: true
            }
        );
        let members = Some(vec![live(24), live(23)]);
        assert_eq!(
            read_members(Stranded::Running, members, &none, true, &asks, ok),
            running(Stranded::Unsignalable)
        );
    }

    /// Where the members cannot be read, the group's own answer stands.
    #[cfg(unix)]
    #[test]
    fn a_group_whose_members_cannot_be_read_keeps_the_kernels_answer() {
        let none = Killed::new();
        let answers = [
            Stranded::Running,
            Stranded::Unsignalable,
            Stranded::Errno(42),
        ];
        for answer in answers {
            let reading = read_members(answer, None, &none, true, &[], Ok(()));
            assert_eq!(reading.stranded, Some(answer));
            assert!(!reading.dying);
        }

        // An empty listing the group contradicts: a failed listing looks like
        // this too.
        let reading = read_members(
            Stranded::Running,
            Some(Vec::new()),
            &none,
            true,
            &[],
            Ok(()),
        );
        assert_eq!(reading.stranded, Some(Stranded::Running));
        // And `EPERM` with nobody listed, where unlisted members may be other
        // users' processes rather than husks (Linux).
        let perm = Err(rustix::io::Errno::PERM);
        let reading = read_members(
            Stranded::Unsignalable,
            Some(Vec::new()),
            &none,
            false,
            &[],
            perm,
        );
        assert_eq!(reading.stranded, Some(Stranded::Unsignalable));
    }

    /// `linger` looks again only while a killed member is dying, stops at the
    /// first running one, never asks more than `GROUP_SETTLE_ROUNDS` times,
    /// and reports what the last look found.
    #[cfg(unix)]
    #[test]
    fn lingering_looks_again_only_while_a_killed_member_is_dying() {
        let rounds = GROUP_SETTLE_ROUNDS as usize;
        let dying = Reading {
            stranded: None,
            dying: true,
        };
        let scripted = |readings: Vec<Reading>| {
            let looked = Rc::new(Cell::new(0));
            let counter = Rc::clone(&looked);
            let read = move || {
                let reading = readings.get(counter.get()).copied().unwrap_or(dying);
                counter.set(counter.get() + 1);
                reading
            };
            (read, looked)
        };

        // Nothing dying: one look.
        let (read, looked) = scripted(vec![clear()]);
        assert_eq!(linger(read), None);
        assert_eq!(looked.get(), 1);

        // Dying, then the child its `fork` made, never signalled.
        let escaped = Reading {
            stranded: Some(Stranded::Running),
            dying: true,
        };
        let (read, looked) = scripted(vec![dying, dying, escaped, clear()]);
        assert_eq!(linger(read), Some(Stranded::Running));
        assert_eq!(looked.get(), 3, "a running member ends the looking");

        // Dying, then gone.
        let (read, looked) = scripted(vec![dying, dying, clear()]);
        assert_eq!(linger(read), None);
        assert_eq!(looked.get(), 3);

        // An unsignalable member beside a dying one is looked at again, and the
        // last look is what is reported.
        let unsignalable = Reading {
            stranded: Some(Stranded::Unsignalable),
            dying: true,
        };
        let (read, looked) = scripted(vec![unsignalable, clear()]);
        assert_eq!(linger(read), None);
        assert_eq!(looked.get(), 2);

        // Dying forever: exactly the bound, and nothing reported.
        let (read, looked) = scripted(Vec::new());
        assert_eq!(linger(read), None);
        assert_eq!(looked.get(), rounds);
    }

    /// `/proc/<pid>/stat` is read by its fields, counted from the end of the
    /// command name, which may contain anything.
    #[cfg(unix)]
    #[test]
    fn a_proc_stat_line_is_read_past_a_command_name_that_contains_parentheses() {
        let line = "4242 (sh) (x) y) S 1 4240 4240 0 -1 4194560 92 0 0 0 0 0 0 0 20 0 1 0 \
                    123456 3784704 213 18446744073709551615 0 0 0 0 0 0 0 0 0 0 0 0 17 3 0 0\n";
        let stat = ProcStat::parse(line).expect("a stat line");
        assert_eq!(
            stat,
            ProcStat {
                state: 'S',
                pgrp: 4240,
                flags: 4194560,
                started: 123456,
            }
        );
        assert!(!stat.finished());

        let zombie = ProcStat::parse(&line.replace(") S 1", ") Z 1")).expect("a zombie");
        assert!(zombie.finished());
        let exiting = ProcStat::parse(&line.replace("4194560", "4194564")).expect("exiting");
        assert!(exiting.finished(), "PF_EXITING is bit 0x4");

        assert_eq!(ProcStat::parse("4242 (sh"), None);
        assert_eq!(ProcStat::parse("4242 (sh) S 1"), None);
    }

    /// The pending masks are read for `SIGKILL`, bit 8, in either set.
    #[cfg(unix)]
    #[test]
    fn a_pending_sigkill_is_read_from_either_mask() {
        let status = |thread: &str, shared: &str| {
            format!("Name:\tsleep\nState:\tS (sleeping)\nSigQ:\t0/63\nSigPnd:\t{thread}\nShdPnd:\t{shared}\nSigBlk:\t0000000000000000\n")
        };
        let none = "0000000000000000";
        let kill = "0000000000000100";
        let term = "0000000000004000";
        assert_eq!(sigkill_pending(&status(none, none)), Some(false));
        assert_eq!(sigkill_pending(&status(kill, none)), Some(true));
        assert_eq!(sigkill_pending(&status(none, kill)), Some(true));
        assert_eq!(sigkill_pending(&status(term, term)), Some(false));
        assert_eq!(sigkill_pending("Name:\tsleep\n"), None);
        assert_eq!(sigkill_pending(&status("zz", none)), None);
    }

    /// The Linux reader, against a `/proc` laid out in a directory: members of
    /// the group are kept and classified, everything else is passed over.
    #[cfg(unix)]
    #[test]
    fn a_proc_tree_is_read_into_the_members_of_one_group() {
        let root = tempfile::tempdir().expect("a root");
        let process = |pid: i32, state: char, pgrp: i32, flags: u64, status: Option<&str>| {
            let dir = root.path().join(pid.to_string());
            std::fs::create_dir(&dir).expect("a process directory");
            std::fs::write(
                dir.join("stat"),
                format!(
                    "{pid} (sleep) {state} 1 {pgrp} {pgrp} 0 -1 {flags} 0 0 0 0 0 0 0 0 20 0 1 0 \
                     {} 0 0\n",
                    1000 + pid
                ),
            )
            .expect("stat");
            if let Some(shared) = status {
                std::fs::write(
                    dir.join("status"),
                    format!("Name:\tsleep\nSigPnd:\t0000000000000000\nShdPnd:\t{shared}\n"),
                )
                .expect("status");
            }
        };
        process(100, 'S', 7, 0, Some("0000000000000000"));
        process(101, 'Z', 7, 0, None);
        process(102, 'S', 7, 0, Some("0000000000000100"));
        process(103, 'R', 7, 0x4, None);
        process(104, 'S', 8, 0, Some("0000000000000000"));
        // Listed, then gone before its status could be read.
        process(105, 'S', 7, 0, None);
        std::fs::create_dir(root.path().join("self")).expect("a non-process entry");

        let mut members = procfs_members(root.path(), 7).expect("a readable tree");
        members.sort_by_key(|member| match member {
            Member::Live(identity) => identity.pid,
            Member::Finished => i32::MAX,
        });
        assert_eq!(
            members,
            vec![
                Member::Live(Identity {
                    pid: 100,
                    started: 1100
                }),
                Member::Finished,
                Member::Finished,
                Member::Finished,
                Member::Finished,
            ]
        );

        // A stat line that cannot be read as one is not an answer.
        std::fs::write(root.path().join("100").join("stat"), "garbage").expect("stat");
        assert_eq!(procfs_members(root.path(), 7), None);
    }

    /// The kernel's listing, on the platforms that have one, names a live
    /// group's members, and finds nothing that can run once `stop` is done.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn the_kernel_lists_a_groups_members_and_nothing_live_after_a_stop() {
        let _alone = one_group_test_at_a_time();
        let tag = tag_for(4);
        let _sweep = SweepTag(tag.clone());
        let (mut child, marker) = a_shell_that_says_it_forked(&tag);
        let group = rustix::process::Pid::from_raw(child.id() as i32).expect("a group");
        wait_for_the_fork(&marker);

        let listed = members(group).expect("a listing");
        let live: Vec<i32> = listed
            .iter()
            .filter_map(|member| match member {
                Member::Live(identity) => Some(identity.pid),
                Member::Finished => None,
            })
            .collect();
        assert!(
            live.contains(&group.as_raw_nonzero().get()),
            "the leader is a live member: {listed:?}"
        );
        assert_eq!(live.len(), 2, "the shell and its sleep: {listed:?}");

        assert_eq!(stop(&mut child), None);
        // What `stop` killed may take a while to say so on a loaded machine;
        // it may not stay live.
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let after = members(group).expect("a listing");
            if after.iter().all(|member| *member == Member::Finished) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "still live after a stop: {after:?}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// `settled` asks the kernel the question the scripted test asks a script,
    /// and the two answers a real group can give come back as themselves.
    ///
    /// The integration half: it proves the real `kill(-pgid, 0)` is wired in and
    /// that its `Ok` and `ESRCH` mean what the reading above assumes. What it
    /// cannot prove is which round's answer is reported, which is why
    /// `settling_reports_the_final_answer_and_asks_exactly_the_rounds_it_names`
    /// exists.
    #[cfg(unix)]
    #[test]
    fn settling_reports_the_last_answer_the_kernel_gave() {
        let _alone = one_group_test_at_a_time();
        let tag = tag_for(2);
        let _sweep = SweepTag(tag.clone());
        let (mut child, marker) = a_shell_that_says_it_forked(&tag);
        let group = rustix::process::Pid::from_raw(child.id() as i32).expect("a group");
        // The group must have had something in it for "it was emptied" to mean
        // anything, and the command itself is what says so.
        wait_for_the_fork(&marker);

        // A group with something running in it is reported as running, every
        // round, and the answer that comes back is that one.
        assert_eq!(settled(group, &Killed::new()), Some(Stranded::Running));

        // And once it is gone, there is nothing to report.
        assert_eq!(stop(&mut child), None);
        assert_eq!(settled(group, &Killed::new()), None);
    }

    /// Each answer the kernel can give reads as itself, and none of them reads
    /// as another.
    ///
    /// The opening phrase is pinned deliberately: `tests/permissions.rs`
    /// asserts that a cancellation does *not* produce this notice, and it can
    /// only do that by matching text. That assertion was already written
    /// against a phrase this function had stopped emitting, which made it an
    /// assertion that could not fail -- so the phrase is now something a test
    /// fails on when it changes.
    #[test]
    fn every_stranded_answer_says_which_one_it_is() {
        assert_eq!(stranded_notice(None), "");

        let running = stranded_notice(Some(Stranded::Running));
        let unsignalable = stranded_notice(Some(Stranded::Unsignalable));
        let errno = stranded_notice(Some(Stranded::Errno(42)));

        for notice in [&running, &unsignalable, &errno] {
            assert!(
                notice.contains("process group was still there"),
                "the acceptance suite matches on this phrase: {notice}"
            );
            assert!(notice.ends_with('\n'), "{notice}");
            assert_eq!(notice.lines().count(), 1, "{notice}");
        }

        assert!(running.contains("still has a running member"), "{running}");
        assert!(unsignalable.contains("EPERM"), "{unsignalable}");
        assert!(errno.contains("errno 42"), "{errno}");

        // Three answers, three notices. A report that cannot tell them apart is
        // the thing `Stranded` exists to prevent.
        assert_ne!(running, unsignalable);
        assert_ne!(running, errno);
        assert_ne!(unsignalable, errno);
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
    /// The pids of every process whose whole argv is `sleep <tag>`.
    ///
    /// One `ps` per call and never in a tight loop. A table that cannot be read
    /// is an empty answer rather than a panic, because this also runs from
    /// `Drop`, where panicking while another panic unwinds aborts the process
    /// and takes the real failure's message with it.
    #[cfg(unix)]
    fn tagged(tag: &str) -> Vec<rustix::process::Pid> {
        let Ok(listing) = Command::new("ps").args(["-eo", "pid=,args="]).output() else {
            return Vec::new();
        };
        String::from_utf8_lossy(&listing.stdout)
            .lines()
            .filter_map(|line| {
                let mut fields = line.split_whitespace();
                let pid: i32 = fields.next()?.parse().ok()?;
                let argv: Vec<&str> = fields.collect();
                (argv == ["sleep", tag]).then_some(pid)
            })
            .filter_map(rustix::process::Pid::from_raw)
            .collect()
    }

    /// Kills this test's own children when it goes out of scope, one pid at a
    /// time.
    ///
    /// **By pid, never by process group.** `stop` has already collected the
    /// leader by the time a test cleans up, so the group id may since have been
    /// recycled -- signalling it is exactly the hazard the product code goes
    /// out of its way to avoid, and test code does not get to reintroduce it. A
    /// pid read out of the table carrying this process's own tag was this
    /// test's when it was read; that is best effort, which is all cleanup needs
    /// to be, and it can never reach a group that is not ours.
    #[cfg(unix)]
    struct SweepTag(String);

    #[cfg(unix)]
    impl Drop for SweepTag {
        fn drop(&mut self) {
            for pid in tagged(&self.0) {
                let _ = rustix::process::kill_process(pid, rustix::process::Signal::KILL);
            }
            let _ = std::fs::remove_file(marker_for(&self.0));
        }
    }

    /// A duration unique to this process *and* to the test using it, so two
    /// tests in this binary cannot claim each other's children or markers.
    #[cfg(unix)]
    fn tag_for(test: u32) -> String {
        format!("30.{test}{}", std::process::id())
    }

    /// Held by every test here that stops a process group and then reads what
    /// is left of it, so none of them runs beside another.
    ///
    /// The loaded-machine test is load by design, and the others assert that a
    /// group is gone within `settle`'s rounds -- a bound a killed process can
    /// outlast while it is still exiting on a saturated machine. Run beside the
    /// loaded test, each of the three failed in some runs and passed in others,
    /// for that reason and not for an escape. Tags keep their children apart;
    /// this keeps their schedules apart.
    #[cfg(unix)]
    fn one_group_test_at_a_time() -> std::sync::MutexGuard<'static, ()> {
        static GROUP_TESTS: Mutex<()> = Mutex::new(());
        // A test that panicked while holding it has already failed by name;
        // the next one should still run.
        GROUP_TESTS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[cfg(unix)]
    fn marker_for(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("xfx-forked-{tag}"))
    }

    /// A shell that forks one child and then says that it has, in its own
    /// process group.
    ///
    /// The announcement is a file, and waiting for it is a `stat`. Finding out
    /// whether a fork had happened by asking `ps` -- a `fork` -- is what made
    /// two of these tests fail under a parallel run: `ps` could not be spawned
    /// at all, the answer came back empty, and the wait timed out against a
    /// command that had started its child long before.
    #[cfg(unix)]
    fn a_shell_that_says_it_forked(tag: &str) -> (Child, std::path::PathBuf) {
        use std::os::unix::process::CommandExt;

        let marker = marker_for(tag);
        let _ = std::fs::remove_file(&marker);
        let mut command = Command::new("/bin/sh");
        command.arg("-c").arg(format!(
            "sleep {tag} & echo forked > {}; wait",
            marker.display()
        ));
        command.env_clear();
        command.stdin(Stdio::null());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        command.process_group(0);
        (command.spawn().expect("spawn a shell"), marker)
    }

    #[cfg(unix)]
    fn wait_for_the_fork(marker: &std::path::Path) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !marker.exists() {
            assert!(
                Instant::now() < deadline,
                "the command never reported forking a child"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Whether `group` still holds a process that was never killed.
    ///
    /// Right after `stop` returns, `kill(-pgid, 0)` answering `Ok` means a
    /// member that is not yet a zombie, and that is one of two things: a
    /// process that escaped every signal, or one that received `SIGKILL` and
    /// has not finished exiting. Under load the second is common -- with 144
    /// forking commands stopped at once on 18 cores, `stop` returned with such
    /// a member in up to 24 of 1152 stops -- each one `ps` still listed a
    /// forked shell in state `U` or `E`, reparented to launchd, and every group
    /// gone 1.5 seconds later -- and that is not an escape. So the question is asked until the group is
    /// gone, for at most ten seconds. Every process these tests start is
    /// `sleep 30.<tag>`, so one still there after ten seconds was never killed.
    ///
    /// Only asked, never signalled: the leader has been collected by now, so
    /// this rests on the group's id not being handed out again between two
    /// questions ten milliseconds apart, which would take the whole pid space
    /// cycling in that time.
    #[cfg(unix)]
    fn outlived_its_kill(group: rustix::process::Pid) -> bool {
        let deadline = Instant::now() + Duration::from_secs(10);
        while rustix::process::test_kill_process_group(group) == Ok(()) {
            if Instant::now() >= deadline {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

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
        let _alone = one_group_test_at_a_time();
        // A duration unique to this process, so a survivor can be cleaned up
        // without reaching for another test's children.
        let tag = tag_for(1);
        // Armed before the first child exists, and it is the only cleanup:
        // whatever escapes is killed by pid, on every exit from this test.
        let _sweep = SweepTag(tag.clone());
        let mut escaped = 0;
        let mut rounds = 0;

        for _ in 0..16 {
            let mut child = a_forking_shell(&tag, 50);
            let group = rustix::process::Pid::from_raw(child.id() as i32).expect("a group");
            // Calibrated, not guaranteed: at this delay one round escaped about
            // 29% of the time against a single-signal `stop` on this machine.
            // Nothing here can observe a fork in progress, and the guarantee
            // under test does not depend on catching one.
            std::thread::sleep(Duration::from_millis(10));

            stop(&mut child);

            rounds += 1;
            // `Ok` is the strict reading: the group still holds a member that
            // can be signalled, which is a process that is running. `EPERM`
            // would mean something is there but already dead and uncollected,
            // and `ESRCH` that the group is gone; neither is an escape.
            if rustix::process::test_kill_process_group(group) == Ok(()) {
                escaped += 1;
            }
        }

        assert_eq!(
            escaped, 0,
            "{escaped} of {rounds} cancellations left a process alive in the \
             command\'s group; one signal is not enough because the kernel\'s \
             walk of the group can pass a member that is still being forked"
        );
    }

    /// The same claim on a machine with more runnable work than cores.
    ///
    /// The test above passed run after run alone and failed once inside a full
    /// suite at load 33, because a leader descheduled inside `fork` can finish
    /// the call after any fixed number of signals (`sweep_anchored`). Load is
    /// what makes that happen, so this test brings its own: eight forking
    /// commands per core, all stopped at once, eight times each -- 1152 stops in
    /// about two and a half seconds on 18 cores. Each command is the same
    /// leader-only fork loop as above, because for that shape zero is a
    /// guarantee rather than a rate: its only forker is the leader, and `stop`
    /// waits for the leader's exit before it sweeps.
    ///
    /// Measured on 18 cores at an ambient load of 50 to 80: against the `stop`
    /// that swept on a clock, 6 of 6 runs failed, with 7, 2, 2, 6, 3 and 3 of
    /// 1152 stops leaving a child that was never killed -- 23 of 6912, 0.33% a
    /// stop. A run of 1152 stops passes against that at
    /// `(1 - 0.0033)^1152`, about 2%, and at the lowest rate seen in one run
    /// (2 of 1152) about 14%. Against this one, 12 of 12 runs and 0 of 13824
    /// stops. A quieter machine lowers the old rate, not the new one.
    ///
    /// The same load also tests the notice from the other side: a stop
    /// reported as stranded when nothing in the group outlived the kill is a
    /// false report, and fails this test as an escape does. Against the
    /// `settled` that read only the group's answer, 6 of 6 runs failed, with
    /// 5, 3, 10, 7, 6 and 9 false notices in 1152 stops (40 of 6912), both
    /// `Running` and `Unsignalable`; reading the members (`reconsider`), 12 of
    /// 12 runs and 0 of 13824.
    #[cfg(unix)]
    #[test]
    fn a_stopped_command_leaves_nothing_alive_in_its_process_group_on_a_loaded_machine() {
        let _alone = one_group_test_at_a_time();
        const SHELLS_PER_CORE: usize = 8;
        const ROUNDS: usize = 8;
        let cores = std::thread::available_parallelism().map_or(4, |cores| cores.get());
        let workers: Vec<_> = (0..cores * SHELLS_PER_CORE)
            .map(|worker| {
                std::thread::spawn(move || {
                    let tag = tag_for(100 + worker as u32);
                    let _sweep = SweepTag(tag.clone());
                    let mut escaped = 0;
                    let mut misreported = Vec::new();
                    for _ in 0..ROUNDS {
                        let mut child = a_forking_shell(&tag, 50);
                        let group =
                            rustix::process::Pid::from_raw(child.id() as i32).expect("a group");
                        std::thread::sleep(Duration::from_millis(10));
                        let stranded = stop(&mut child);
                        let outlived = outlived_its_kill(group);
                        if outlived {
                            escaped += 1;
                        }
                        // The notice is a claim that something may still be
                        // running. Made about a group whose every member was
                        // dead or dying, it is false.
                        if let (Some(stranded), false) = (stranded, outlived) {
                            misreported.push(stranded);
                        }
                    }
                    (escaped, misreported)
                })
            })
            .collect();
        let stopped = workers.len() * ROUNDS;
        let mut escaped = 0;
        let mut misreported = Vec::new();
        for worker in workers {
            let (escapes, notices) = worker.join().expect("a worker");
            escaped += escapes;
            misreported.extend(notices);
        }

        assert_eq!(
            escaped, 0,
            "{escaped} of {stopped} commands stopped on a loaded machine left a process \
             alive in their group"
        );
        assert!(
            misreported.is_empty(),
            "{} of {stopped} commands stopped on a loaded machine were reported as leaving \
             their group behind, and nothing in it outlived the kill: {misreported:?}",
            misreported.len()
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
        let _alone = one_group_test_at_a_time();
        let tag = tag_for(3);
        let _sweep = SweepTag(tag.clone());
        let (mut child, marker) = a_shell_that_says_it_forked(&tag);
        let group = rustix::process::Pid::from_raw(child.id() as i32).expect("a group");
        // The group must have had something in it for "it was emptied" to mean
        // anything, and the command itself is what says so.
        wait_for_the_fork(&marker);

        // A timeout that has already run out drives the timeout branch on the
        // first poll.
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
