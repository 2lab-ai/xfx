//! Every child process xfx starts, started the way a shell would start it.
//!
//! A child is born with its parent thread's signal mask and with every signal
//! its parent ignores still ignored; `exec` keeps both, and only caught signals
//! go back to their defaults. xfx starts its children from threads that block
//! every signal the TUI owns -- `SIGINT`, `SIGTERM`, `SIGHUP`, `SIGTSTP`,
//! `SIGCONT`, `SIGWINCH` ([`crate::tui`]'s `signals::spawn_unsignalled`) -- and
//! the standard library leaves that mask alone on purpose: "Inherit the signal
//! mask from the parent rather than resetting it" (Rust 1.98,
//! `library/std/src/sys/process/unix/unix.rs:357-358` on its `fork` path and
//! `:733-734` on its `posix_spawn` path). A command started that way ignores a
//! `timeout`'s `SIGTERM`, a test runner's `SIGINT` to its workers, and job
//! control.
//!
//! So [`command`] is the one way a child is made here: a [`Command`] whose
//! child, between `fork` and `exec`, empties its signal mask and puts back the
//! default for the one signal xfx ignores, `SIGPIPE` -- the Rust runtime ignores
//! it before `main`, and the TUI ignores it again when it installs its handlers
//! (`tui::signals::install_handlers`). Every other signal xfx touches it
//! catches, and `exec` gives a caught signal its default back. The standard
//! library resets `SIGPIPE` itself today (`unix.rs:364-376`, `:741-754`);
//! doing it here too keeps the rule ours rather than a version's.
//!
//! A signal xfx was *started* ignoring and does not catch is left ignored, as
//! a shell leaves it: `nohup` ignores `SIGHUP`, and a non-interactive shell's
//! `&` ignores `SIGINT` and `SIGQUIT`, precisely so that everything started
//! under them inherits the ignore through `exec`. That is the launcher's
//! choice, not a disposition xfx set. **A signal xfx catches is not** -- `exec`
//! gives it its default -- so what a command inherits depends on where it was
//! started from, as it does in fx c1db919:
//!
//! - **In the TUI**, `SIGHUP` and `SIGTERM` are caught whatever the launcher
//!   said, so the terminal is restored before a hangup or a stop ends the
//!   process (`tui::signals::install_handlers`), and a command has both at
//!   their defaults -- as under fx, which installs its restore over any
//!   ignore (`installAbnormalExitHandlers`, `app_lifecycle.zig:82-104`).
//!   `SIGINT` is caught only when it was not inherited ignored, so
//!   `nohup … &`'s ignore reaches the command, as fx's does: fx catches no
//!   `SIGINT` while its TUI runs. `SIGQUIT` is never caught.
//! - **In `ask` and the line shell**, `SIGINT` is the user's Ctrl-C and is
//!   caught (`app::spawn_interrupt_thread`), so a command has it at its
//!   default -- as under fx's headless ask, which installs its interrupt over
//!   any ignore (`cli_ask.zig:166-178`).
//!
//! `SIGCHLD` is the one signal reset whatever the launcher said:
//! [`collect_own_children`] puts it back to its default at startup, because
//! ignoring it takes xfx's children out of its hands (the kernel collects
//! them, and a group signal could then reach a reused id). fx sets no
//! `SIGCHLD` disposition at either pin.
//!
//! The case `no_child_process_is_made_any_other_way` keeps every
//! `Command::new` in the library's own code in this file; `clippy.toml` keeps
//! every start of one in [`spawn`] and every pipe in [`pipe`].

use std::ffi::OsStr;
use std::io;
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::sync::{Mutex, MutexGuard, PoisonError};

/// Held by every start of a child of xfx's ([`spawn`]), and by every pipe
/// xfx makes itself ([`pipe`]).
///
/// macOS has no `pipe2`, so the standard library makes each pipe a start
/// uses -- the child's input, output and error, and the one that says whether
/// `exec` worked -- with `pipe` and only then marks it close-on-exec
/// (`library/std/src/sys/pipe/unix.rs:32-41` in Rust 1.99.1, the stable
/// toolchain the gates and CI build with; the repository pins no toolchain,
/// and `Cargo.toml`'s `rust-version` is the 1.96 floor. Read in that
/// toolchain's own source, `$(rustc --print sysroot)/lib/rustlib/src/rust/`.)
/// A `fork` another thread makes in between hands that child both ends, and
/// `exec` keeps them. [`command`]'s signal reset is a `pre_exec`, which takes
/// every start off `posix_spawn` and onto the `fork` path
/// (`sys/process/unix/unix.rs:467-470`), whose parent makes the status pipe
/// (`:81`) and reads it until `exec` closes it (`:139-140`): a child that
/// inherited it -- an MCP server started beside it, which runs as long as
/// the session -- holds it open, and the start never returns (MCP-Me:
/// `mcp::tests::a_call_the_server_answers_with_no_result_fails_as_fx_says_it_did`,
/// one run in about five, a server "connecting" for good). One start at a
/// time makes no pipe of one start reach another's child.
static SPAWNING: Mutex<()> = Mutex::new(());

/// [`SPAWNING`], taken: what every start and every pipe waits for.
fn one_at_a_time() -> MutexGuard<'static, ()> {
    // A case that holds the lock learns here that the thread it is waiting
    // on has got as far as the lock, rather than guessing with a sleep.
    #[cfg(all(test, unix))]
    tests::arrived();
    SPAWNING.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Holds every start and every pipe until it is dropped: for a case
/// elsewhere that must see what its work does while another start is in
/// flight.
#[cfg(all(test, unix))]
pub(crate) fn hold_every_start() -> MutexGuard<'static, ()> {
    SPAWNING.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(all(test, unix))]
pub(crate) use tests::has_arrived;

/// `command` started, under the lock every start of xfx's takes
/// ([`SPAWNING`]). The start alone is held -- a child that runs for an hour
/// holds nothing -- and a start never waits for more than another start.
#[allow(
    clippy::disallowed_methods,
    reason = "the one start every other start goes through, under the lock"
)]
pub fn spawn(command: &mut Command) -> io::Result<Child> {
    let _one_at_a_time = one_at_a_time();
    command.spawn()
}

/// [`spawn`], unless `given_up` says so once the lock is held: then nothing
/// is started, and the answer is `None`.
///
/// For work that can be given up while it waits for another start: a stop
/// that lands in that wait refuses the child rather than starting it to kill
/// it. `given_up` runs under the lock, so it must not start a child or make a
/// pipe; a look at a flag is what it is for.
#[allow(
    clippy::disallowed_methods,
    reason = "the one start every other start goes through, under the lock"
)]
pub fn spawn_unless(
    command: &mut Command,
    given_up: impl FnOnce() -> bool,
) -> io::Result<Option<Child>> {
    let _one_at_a_time = one_at_a_time();
    if given_up() {
        return Ok(None);
    }
    command.spawn().map(Some)
}

/// `Command::status`, started through [`spawn`]: the same start and the same
/// wait (`library/std/src/process.rs:1096-1098` and `:1177-1182`, Rust 1.99.1,
/// both `spawn(Stdio::Inherit, true)`).
pub fn status(command: &mut Command) -> io::Result<ExitStatus> {
    spawn(command)?.wait()
}

/// `Command::output` for a command that names none of its streams, started
/// through [`spawn`]: input from nothing, output and errors read to their
/// ends (`library/std/src/sys/process/mod.rs:55-56`, Rust 1.99.1).
///
/// It names all three streams, as `output` does only for the ones a caller
/// left alone; a command that names one is started with [`spawn`] and
/// `wait_with_output`.
pub fn output(command: &mut Command) -> io::Result<Output> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    spawn(command)?.wait_with_output()
}

/// A pipe, both ends close-on-exec, made under the lock every start takes
/// ([`SPAWNING`]).
///
/// macOS makes a pipe and only then marks it, as the standard library's own
/// pipes are made ([`SPAWNING`]); a start that forked in between would hand
/// both ends to its child -- an MCP server holds them all session. Made
/// here, no start is in the middle of its `fork`.
#[cfg(unix)]
pub fn pipe() -> io::Result<(std::os::fd::OwnedFd, std::os::fd::OwnedFd)> {
    let _no_start = one_at_a_time();
    #[allow(
        clippy::disallowed_methods,
        reason = "the one pipe every other pipe goes through, under the lock"
    )]
    let (read, write) = rustix::pipe::pipe()?;
    for end in [&read, &write] {
        rustix::io::fcntl_setfd(end, rustix::io::FdFlags::CLOEXEC)?;
    }
    Ok((read, write))
}

/// A command for `program` whose child starts with no signal blocked and
/// `SIGPIPE` at its default, and without xfx's own credential.
///
/// The rest of xfx's environment is inherited: `git`, a clipboard tool and
/// `osascript` need the person's display, locale and configuration, and none of
/// that is xfx's to keep. The credential is
/// ([`crate::config::CREDENTIAL_VARIABLES`]), so it is removed here, where
/// every child is made: a caller that means to hand it on sets it again, and a
/// caller that builds the whole environment (the terminal tool, an MCP stdio
/// server) clears everything first.
pub fn command(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    for name in crate::config::CREDENTIAL_VARIABLES {
        command.env_remove(name);
    }
    #[cfg(unix)]
    clean_signals(&mut command);
    command
}

/// Makes `command`'s child empty its signal mask and put `SIGPIPE` back to its
/// default before `exec`.
///
/// The sets are built here, in the parent; the child only installs them, with
/// `pthread_sigmask` and `sigaction`, both on POSIX's list of async-signal-safe
/// functions -- the only kind a child of a threaded process may call before
/// `exec`.
#[cfg(unix)]
fn clean_signals(command: &mut Command) {
    use std::os::unix::process::CommandExt;

    // SAFETY: `sigemptyset` writes the zeroed set it is handed; `sigaction` is
    // a plain struct for which all-zero is a valid value, then given `SIG_DFL`.
    let (empty, default) = unsafe {
        let mut empty: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut empty);
        let mut default: libc::sigaction = std::mem::zeroed();
        default.sa_sigaction = libc::SIG_DFL;
        libc::sigemptyset(&mut default.sa_mask);
        (empty, default)
    };
    // SAFETY: the closure runs in the child between `fork` and `exec`. It
    // allocates nothing, takes no lock, and calls only `pthread_sigmask` and
    // `sigaction` on values it owns by copy.
    unsafe {
        command.pre_exec(move || {
            let code = libc::pthread_sigmask(libc::SIG_SETMASK, &empty, std::ptr::null_mut());
            if code != 0 {
                return Err(std::io::Error::from_raw_os_error(code));
            }
            if libc::sigaction(libc::SIGPIPE, &default, std::ptr::null_mut()) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

/// Puts `SIGCHLD` back to its default, whatever xfx was started with.
///
/// Run first in `main`. An ignored `SIGCHLD` -- which, unlike the ignores the
/// module keeps, a launcher can hand down by accident -- has the kernel
/// collect each child the moment it exits (POSIX `waitpid`, XSI). Linux does
/// so for an ignore inherited through `exec` -- `wait` fails with `ECHILD`, the
/// case below -- and macOS 26 does not. Where it does, `wait` fails,
/// and worse, a process group whose leader is already gone can have its id
/// handed out again before xfx signals the group: the stop paths in
/// [`crate::mcp`] and [`crate::tools`] rely on the leader staying
/// uncollected until they have signalled. xfx installs no `SIGCHLD` handler
/// of its own, so nothing is lost; its children inherit the default. fx sets
/// no disposition for `SIGCHLD` at either pin (`c1db9191`, `481104fd`).
#[cfg(unix)]
pub fn collect_own_children() -> std::io::Result<()> {
    // SAFETY: `sigaction` is a plain struct for which all-zero is a valid
    // value, then given `SIG_DFL` and an empty mask; no flag is kept, so
    // `SA_NOCLDWAIT` goes with the ignore.
    unsafe {
        let mut default: libc::sigaction = std::mem::zeroed();
        default.sa_sigaction = libc::SIG_DFL;
        libc::sigemptyset(&mut default.sa_mask);
        if libc::sigaction(libc::SIGCHLD, &default, std::ptr::null_mut()) != 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

/// A test's child, run to its end within `limit`: its output, or `TimedOut`
/// once it has run that long -- killed and reaped -- or once it has exited but
/// its output has not closed within what is left of `limit` (a grandchild
/// holding a pipe). A hung child fails the case that started it rather than
/// the whole run waiting on it. The child's stdin is null and its stdout and
/// stderr are read whole, each on a thread of its own, so a child that fills
/// one pipe cannot stall waiting on the other.
#[cfg(test)]
pub(crate) fn output_within(
    command: &mut Command,
    limit: std::time::Duration,
) -> std::io::Result<std::process::Output> {
    use std::io::Read;
    use std::process::Stdio;
    use std::sync::mpsc;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Started as every child of xfx's is, under the one start lock.
    let mut child = spawn(command)?;
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        let (sent, read) = mpsc::channel();
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut bytes);
            }
            let _ = sent.send(bytes);
        });
        read
    };
    let stdout = drain(
        child
            .stdout
            .take()
            .map(|pipe| Box::new(pipe) as Box<dyn Read + Send>),
    );
    let stderr = drain(
        child
            .stderr
            .take()
            .map(|pipe| Box::new(pipe) as Box<dyn Read + Send>),
    );
    let timed_out = |what: &str| {
        std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            format!("{what} after {limit:?}"),
        )
    };
    let started = std::time::Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() >= limit {
            // This test's own child, by its handle: killed, then waited for,
            // so no zombie is left behind. The readers are left to end with
            // the pipes.
            let _ = child.kill();
            let _ = child.wait();
            return Err(timed_out("the child was still running, and was killed,"));
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    let collect = |read: mpsc::Receiver<Vec<u8>>| {
        read.recv_timeout(limit.saturating_sub(started.elapsed()))
            .map_err(|_| timed_out("the child exited but its output was still open"))
    };
    let stdout = collect(stdout)?;
    let stderr = collect(stderr)?;
    Ok(std::process::Output {
        status,
        stdout,
        stderr,
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn a_child_past_its_limit_is_killed_and_reaped() {
        let pid_file = tempfile::NamedTempFile::new().expect("a pid file");
        let mut child = command("/bin/sh");
        child.args(["-c", "echo $$ > \"$1\"; exec sleep 30", "sh"]);
        child.arg(pid_file.path());
        let started = std::time::Instant::now();
        let err = output_within(&mut child, std::time::Duration::from_millis(500))
            .expect_err("a child that sleeps past its limit");
        assert_eq!(err.kind(), std::io::ErrorKind::TimedOut, "{err}");
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
        let pid: i32 = std::fs::read_to_string(pid_file.path())
            .expect("the pid")
            .trim()
            .parse()
            .expect("a number");
        // Reaped, not only killed: a zombie still answers a signal 0.
        let pid = rustix::process::Pid::from_raw(pid).expect("a pid");
        assert_eq!(
            rustix::process::test_kill_process(pid),
            Err(rustix::io::Errno::SRCH)
        );
    }

    #[test]
    fn a_child_whose_output_outlives_it_is_given_up_on_at_the_limit() {
        // The child exits at once; a grandchild it leaves behind holds the
        // pipes open past the limit. The wait ends at the limit regardless.
        let pid_file = tempfile::NamedTempFile::new().expect("a pid file");
        let mut child = command("/bin/sh");
        child.args(["-c", "sleep 30 & echo $! > \"$1\"; exit 0", "sh"]);
        child.arg(pid_file.path());
        let started = std::time::Instant::now();
        let err = output_within(&mut child, std::time::Duration::from_millis(500))
            .expect_err("output still open");
        assert_eq!(err.kind(), std::io::ErrorKind::TimedOut, "{err}");
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
        // The grandchild is this test's to end: its pid, written by its own
        // parent, signalled once.
        let pid: i32 = std::fs::read_to_string(pid_file.path())
            .expect("the pid")
            .trim()
            .parse()
            .expect("a number");
        let pid = rustix::process::Pid::from_raw(pid).expect("a pid");
        let _ = rustix::process::kill_process(pid, rustix::process::Signal::KILL);
    }

    #[test]
    fn a_child_within_its_limit_hands_back_all_it_said() {
        let mut child = command("/bin/sh");
        child.args(["-c", "printf out; printf err >&2; exit 3"]);
        let output = output_within(&mut child, std::time::Duration::from_secs(60)).expect("ran");
        assert_eq!(output.status.code(), Some(3));
        assert_eq!(output.stdout, b"out");
        assert_eq!(output.stderr, b"err");
    }
    use std::time::{Duration, Instant};

    #[test]
    fn children_started_at_once_never_hold_up_one_anothers_start() {
        // MCP-Me: started from many threads at once, as a session starts its
        // MCP servers, each start must return. A child that inherited
        // another start's pipe from the `pipe`-then-close-on-exec window
        // held that start until it exited -- here 30 s, for an MCP server
        // the whole session. Sixteen at a time, each with its input, output
        // and error piped as an MCP server's are, thirty times; each start
        // must also have started its child.
        use std::sync::mpsc;
        const AT_ONCE: usize = 16;
        // A sleeper ends on its own after this long. So a start a sleeper's
        // copy of its pipe held is let go by then at the latest, even one
        // whose holder had not come back to be killed, and nothing this case
        // starts outlives the case by more.
        const SLEEP: Duration = Duration::from_secs(30);
        let end = |child: &mut Child| {
            let _ = child.kill();
            let _ = child.wait();
        };
        for round in 0..30 {
            let (done, finished) = mpsc::channel();
            let workers: Vec<_> = (0..AT_ONCE)
                .map(|_| {
                    let done = done.clone();
                    std::thread::spawn(move || {
                        let mut sleeper = command("/bin/sleep");
                        sleeper
                            .arg(SLEEP.as_secs().to_string())
                            .stdin(Stdio::piped())
                            .stdout(Stdio::piped())
                            .stderr(Stdio::piped());
                        let _ = done.send(spawn(&mut sleeper));
                    })
                })
                .collect();
            drop(done);
            let mut started = Vec::new();
            let mut failed = Vec::new();
            let on_time = Instant::now() + Duration::from_secs(10);
            while started.len() + failed.len() < AT_ONCE {
                match finished.recv_timeout(on_time.saturating_duration_since(Instant::now())) {
                    Ok(Ok(child)) => started.push(child),
                    Ok(Err(err)) => failed.push(err.to_string()),
                    Err(_) => break,
                }
            }
            let returned = started.len() + failed.len();
            // Every sleeper is this case's to end: those that came back, and
            // a late one as it comes back. Killing the first lets go of a
            // start their copies held; a holder still starting itself ends
            // within `SLEEP`. Every worker sends once, so once all have,
            // each join is immediate.
            started.iter_mut().for_each(end);
            let mut late = 0;
            let cleanup = Instant::now() + SLEEP + Duration::from_secs(10);
            while returned + late < AT_ONCE {
                match finished.recv_timeout(cleanup.saturating_duration_since(Instant::now())) {
                    Ok(Ok(mut child)) => end(&mut child),
                    Ok(Err(_)) => {}
                    Err(_) => break,
                }
                late += 1;
            }
            assert_eq!(
                returned + late,
                AT_ONCE,
                "round {round}: a start had not returned {} s on, past every sleeper's own end",
                (SLEEP + Duration::from_secs(20)).as_secs()
            );
            for worker in workers {
                worker.join().expect("a worker");
            }
            assert_eq!(
                returned, AT_ONCE,
                "round {round}: {late} of {AT_ONCE} starts returned only once the others' children were killed"
            );
            assert!(
                failed.is_empty(),
                "round {round}: {} of {AT_ONCE} starts failed, so the round says nothing of the lock: {failed:?}",
                failed.len()
            );
        }
    }

    /// The threads that have got as far as the start lock, in order: every
    /// start and every pipe in this test binary reports here
    /// ([`one_at_a_time`]).
    static ARRIVALS: Mutex<Vec<std::thread::ThreadId>> = Mutex::new(Vec::new());

    pub(super) fn arrived() {
        ARRIVALS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(std::thread::current().id());
    }

    /// Whether `thread` has got as far as the start lock: one that has, and
    /// finds it held, is waiting on it.
    pub(crate) fn has_arrived(thread: std::thread::ThreadId) -> bool {
        ARRIVALS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(&thread)
    }

    #[test]
    fn a_start_given_up_while_another_start_holds_the_lock_starts_nothing() {
        // The clipboard's stop: given up while the start waits for another
        // start, the child is refused, not started and then killed. The look
        // is taken under the lock, however late the work got there: the case
        // gives up only once the starter is at the lock.
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        let held = SPAWNING.lock().unwrap_or_else(PoisonError::into_inner);
        let given_up = Arc::new(AtomicBool::new(false));
        let starter = std::thread::spawn({
            let given_up = Arc::clone(&given_up);
            move || {
                let mut tool = command("/usr/bin/true");
                tool.stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                spawn_unless(&mut tool, || given_up.load(Ordering::Acquire))
                    .expect("a start")
                    .map(|mut child| child.wait().expect("a status"))
            }
        });
        let deadline = Instant::now() + Duration::from_secs(10);
        while !has_arrived(starter.thread().id()) {
            assert!(
                Instant::now() < deadline,
                "the starter never reached the start lock"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        given_up.store(true, Ordering::Release);
        drop(held);
        assert_eq!(
            starter.join().expect("the starter"),
            None,
            "a start given up in its wait for the lock still started its child"
        );

        let mut tool = command("/usr/bin/true");
        tool.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = spawn_unless(&mut tool, || false)
            .expect("a start")
            .expect("a start nobody gave up");
        assert!(child.wait().expect("a status").success());
    }

    #[test]
    fn a_pipe_is_made_close_on_exec_and_never_beside_a_start() {
        // `Wakeup`'s pipe, and every pipe xfx makes: made while a start
        // holds the lock, it waits for the start to be done. Nothing may
        // come before the maker is at the lock, and nothing while it waits.
        use std::sync::mpsc;
        let held = SPAWNING.lock().unwrap_or_else(PoisonError::into_inner);
        let (made, pipes) = mpsc::channel();
        let maker = std::thread::spawn(move || {
            let _ = made.send(pipe().expect("a pipe"));
        });
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let at_the_lock = has_arrived(maker.thread().id());
            assert!(
                pipes.try_recv().is_err(),
                "a pipe was made while a start held the lock"
            );
            if at_the_lock {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "the maker never reached the start lock"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        drop(held);
        let (read, write) = pipes
            .recv_timeout(Duration::from_secs(10))
            .expect("the pipe, once the start was done");
        maker.join().expect("the maker");
        for end in [&read, &write] {
            assert!(
                rustix::io::fcntl_getfd(end)
                    .expect("its flags")
                    .contains(rustix::io::FdFlags::CLOEXEC),
                "an end of the pipe is not close-on-exec"
            );
        }
    }

    /// Runs `body` on a thread that blocks every signal the TUI owns, as
    /// every thread a session starts does, and hands back what it returned.
    fn on_a_blocked_thread<T: Send + 'static>(body: impl FnOnce() -> T + Send + 'static) -> T {
        std::thread::spawn(move || {
            // SAFETY: `set` is a valid set this closure owns for the call.
            unsafe {
                let mut set: libc::sigset_t = std::mem::zeroed();
                libc::sigemptyset(&mut set);
                for signal in [
                    libc::SIGINT,
                    libc::SIGTERM,
                    libc::SIGHUP,
                    libc::SIGTSTP,
                    libc::SIGCONT,
                    libc::SIGWINCH,
                ] {
                    libc::sigaddset(&mut set, signal);
                }
                assert_eq!(
                    libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut()),
                    0
                );
            }
            body()
        })
        .join()
        .expect("joined")
    }

    /// Waits for `child` up to `limit`, killing it if it is still running.
    fn waited(
        child: &mut std::process::Child,
        limit: Duration,
    ) -> Option<std::process::ExitStatus> {
        let deadline = Instant::now() + limit;
        loop {
            if let Some(status) = child.try_wait().expect("a status") {
                return Some(status);
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// `command`, with each of `signals` that this test binary was started
    /// ignoring put back to its default in the child.
    ///
    /// A launcher can hand the runner an ignored signal -- `nohup` ignores
    /// `SIGHUP`, a non-interactive shell's `&` ignores `SIGINT` and `SIGQUIT`
    /// -- and `exec` keeps it, in the runner and in every child [`command`]
    /// makes there (the module's rule, not a lapse). A case that sends such a
    /// signal and waits for a death, or a shell that traps it, would then be
    /// reading the launcher instead of the mask: a runner started with
    /// `nohup … &` failed the case below on `SIGINT` with no status at all,
    /// every time.
    ///
    /// So the fixture is undone where it was inherited, and nowhere else. The
    /// dispositions are only *read* here -- they are the runner's, other cases
    /// install and release handlers on them, and `nohup` set them for the whole
    /// run -- and the reset is the child's, after [`command`]'s own: a signal
    /// the runner does not ignore is not touched, so a case started the
    /// ordinary way runs exactly the child it always did. Never `SIGPIPE`,
    /// whose default is [`command`]'s to prove.
    fn undoing_the_runners_ignores(mut command: Command, signals: &[libc::c_int]) -> Command {
        use std::os::unix::process::CommandExt;

        assert!(
            !signals.contains(&libc::SIGPIPE),
            "SIGPIPE's default is the claim under test, not a fixture"
        );
        let ignored: Vec<libc::c_int> = signals
            .iter()
            .copied()
            .filter(|&signal| {
                // SAFETY: a null new action makes `sigaction` a pure query, and
                // `current` is a valid out-parameter for the call.
                unsafe {
                    let mut current: libc::sigaction = std::mem::zeroed();
                    libc::sigaction(signal, std::ptr::null(), &mut current) == 0
                        && current.sa_sigaction == libc::SIG_IGN
                }
            })
            .collect();
        if ignored.is_empty() {
            return command;
        }
        // SAFETY: as `clean_signals`.
        let default = unsafe {
            let mut default: libc::sigaction = std::mem::zeroed();
            default.sa_sigaction = libc::SIG_DFL;
            libc::sigemptyset(&mut default.sa_mask);
            default
        };
        // SAFETY: as `clean_signals`: the child only reads `ignored` and calls
        // `sigaction`.
        unsafe {
            command.pre_exec(move || {
                for &signal in &ignored {
                    if libc::sigaction(signal, &default, std::ptr::null_mut()) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                }
                Ok(())
            });
        }
        command
    }

    #[test]
    fn a_child_of_a_thread_that_blocks_everything_takes_a_sigterm() {
        // The shell signals itself; a mask the child inherited would hold it
        // pending and the `echo` would run.
        let output = on_a_blocked_thread(|| {
            super::output(
                undoing_the_runners_ignores(command("/bin/sh"), &[libc::SIGTERM])
                    .args(["-c", "kill -TERM $$; echo survived"]),
            )
            .expect("a child")
        });
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(output.status.signal(), Some(libc::SIGTERM), "{output:?}");
        assert!(output.stdout.is_empty(), "{output:?}");
    }

    #[test]
    fn every_owned_signal_reaches_a_child_and_sigpipe_is_its_default() {
        // `kill -0` from inside cannot see a mask, so each signal is sent from
        // here and the child is asked whether it died of it. `SIGCONT` and
        // `SIGWINCH` default to nothing, so they are trapped instead: a blocked
        // one never runs its trap.
        for (signal, name) in [
            (libc::SIGINT, "INT"),
            (libc::SIGHUP, "HUP"),
            (libc::SIGPIPE, "PIPE"),
        ] {
            let fixture = (signal != libc::SIGPIPE).then_some(signal);
            let mut child = on_a_blocked_thread(move || {
                spawn(
                    undoing_the_runners_ignores(command("/bin/sleep"), fixture.as_slice())
                        .arg("30"),
                )
                .expect("a child")
            });
            let pid = libc::pid_t::try_from(child.id()).expect("a pid");
            // SAFETY: `kill` on a child this case started and has not reaped.
            assert_eq!(unsafe { libc::kill(pid, signal) }, 0);
            let status = waited(&mut child, Duration::from_secs(10));
            use std::os::unix::process::ExitStatusExt;
            assert_eq!(
                status.and_then(|status| status.signal()),
                Some(signal),
                "SIG{name} did not end the child"
            );
        }
        for (signal, name) in [(libc::SIGCONT, "CONT"), (libc::SIGWINCH, "WINCH")] {
            let script =
                format!("trap 'echo {name}; exit 0' {name}; kill -{name} $$; sleep 5 & wait");
            let output = on_a_blocked_thread(move || {
                super::output(
                    undoing_the_runners_ignores(command("/bin/sh"), &[signal])
                        .args(["-c", &script]),
                )
                .expect("a child")
            });
            assert_eq!(
                String::from_utf8_lossy(&output.stdout).trim(),
                name,
                "SIG{name} was held from the child"
            );
        }
    }

    /// The two cases above, run again in a copy of this test binary started
    /// the way `nohup … &` starts one -- and further, ignoring every signal
    /// they send that is not the product's claim. Without the fixture both
    /// fail here on every run; with it they test the mask, wherever the runner
    /// came from.
    #[test]
    fn the_signal_cases_hold_in_a_runner_started_ignoring_what_they_send() {
        use std::os::unix::process::CommandExt;

        const CASES: [&str; 2] = [
            "process::tests::a_child_of_a_thread_that_blocks_everything_takes_a_sigterm",
            "process::tests::every_owned_signal_reaches_a_child_and_sigpipe_is_its_default",
        ];
        // SAFETY: `ignore` is a plain struct for which all-zero is valid, then
        // given `SIG_IGN`.
        let ignore = unsafe {
            let mut ignore: libc::sigaction = std::mem::zeroed();
            ignore.sa_sigaction = libc::SIG_IGN;
            libc::sigemptyset(&mut ignore.sa_mask);
            ignore
        };
        let mut runner = command(std::env::current_exe().expect("this test binary"));
        runner.args(["--exact", "--test-threads=1"]).args(CASES);
        // SAFETY: as `clean_signals`: the child only calls `sigaction` on a
        // value it owns by copy.
        unsafe {
            runner.pre_exec(move || {
                for signal in [
                    libc::SIGHUP,
                    libc::SIGINT,
                    libc::SIGQUIT,
                    libc::SIGTERM,
                    libc::SIGCONT,
                    libc::SIGWINCH,
                ] {
                    if libc::sigaction(signal, &ignore, std::ptr::null_mut()) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                }
                Ok(())
            });
        }
        let output = super::output(&mut runner).expect("a runner");
        let report = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        // Counted, not only exited-zero: a filter that matched nothing passes.
        assert!(
            output.status.success() && report.contains("test result: ok. 2 passed"),
            "{report}"
        );
    }

    /// xfx's own credential never reaches a child by inheritance: both of its
    /// variables are removed from every command made here, and a caller that
    /// names one on purpose -- an MCP server's configured `env` -- still can.
    /// Named here as literals, so a wrong shared constant cannot pass by
    /// agreeing with itself.
    #[test]
    fn a_child_is_never_handed_xfxs_own_credential_unless_a_caller_names_it() {
        let removed: Vec<String> = command("/bin/true")
            .get_envs()
            .filter(|(_, value)| value.is_none())
            .map(|(name, _)| name.to_string_lossy().into_owned())
            .collect();
        for name in ["AI_GATEWAY_API_KEY", "VERCEL_OIDC_TOKEN"] {
            assert!(
                removed.iter().any(|removed| removed == name),
                "{name} is not removed from a child's environment: {removed:?}"
            );
        }
        let mut granted = command("/bin/true");
        granted.env("VERCEL_OIDC_TOKEN", "xfx-sentinel-granted");
        assert!(
            granted
                .get_envs()
                .any(|(name, value)| name == "VERCEL_OIDC_TOKEN"
                    && value == Some(OsStr::new("xfx-sentinel-granted"))),
            "a credential a caller names is not passed"
        );
    }

    /// The rule, kept by reading the library's own code: every child process
    /// is made by [`command`]. A test module -- everything from a file's first
    /// unindented `#[cfg(test)]` or `#[cfg(all(test, …))]` that puts a `mod`
    /// under it -- may make one as it likes.
    ///
    /// Every start of one, tests' too, goes through [`spawn`], and every pipe
    /// through [`pipe`]: that half is clippy's, on the resolved call
    /// (`clippy.toml`'s `disallowed-methods`), which sees a command started
    /// in a later statement as well as in the one that made it. Kept here:
    /// each entry is still a line of its own in `clippy.toml`, not commented
    /// out, and no code but this file's lifts them -- by name, by the old
    /// name, or by a group or `warnings` allow that holds them.
    ///
    /// A guard against the ordinary ways to slip past the rule by accident,
    /// not a proof that none is left: a macro, an attribute under `cfg_attr`,
    /// a raw `syscall`, or a dependency's own start are beyond what reading
    /// lines can see.
    #[test]
    fn no_child_process_is_made_any_other_way() {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let rules = std::fs::read_to_string(manifest.join("clippy.toml")).expect("clippy.toml");
        for path in [
            "std::process::Command::spawn",
            "std::process::Command::status",
            "std::process::Command::output",
            "tokio::process::Command::spawn",
            "tokio::process::Command::status",
            "tokio::process::Command::output",
            "libc::fork",
            "libc::vfork",
            "libc::forkpty",
            "libc::posix_spawn",
            "libc::posix_spawnp",
            "libc::popen",
            "libc::system",
            "libc::pipe",
            "libc::socketpair",
            "rustix::pipe::pipe",
            "rustix::pipe::pipe_with",
            "std::io::pipe",
            "std::os::unix::net::UnixStream::pair",
            "std::os::unix::net::UnixDatagram::pair",
            "mio::unix::pipe::new",
        ] {
            let entry = format!("{{ path = \"{path}\", ");
            assert!(
                rules
                    .lines()
                    .any(|line| line.trim_start().starts_with(&entry)),
                "clippy.toml no longer disallows {path}"
            );
        }
        let root = manifest.join("src");
        let mut stack = vec![root.clone(), manifest.join("tests")];
        let mut found = Vec::new();
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("a source dir") {
                let path = entry.expect("an entry").path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(OsStr::to_str) != Some("rs")
                    || path.ends_with("process.rs") && path.parent() == Some(root.as_path())
                {
                    continue;
                }
                let library = path.starts_with(&root);
                let text = std::fs::read_to_string(&path).expect("a source file");
                let lines: Vec<&str> = text.lines().collect();
                let mut in_tests = !library;
                for (number, line) in lines.iter().enumerate() {
                    in_tests |= (line.starts_with("#[cfg(test)]")
                        || line.starts_with("#[cfg(all(test"))
                        && lines[number + 1..]
                            .iter()
                            .find(|next| !next.starts_with("#["))
                            .is_some_and(|next| next.contains("mod "));
                    if line.trim_start().starts_with("//") {
                        continue;
                    }
                    if !in_tests
                        && (line.contains("Command::new(")
                            || line.contains("tokio::process::Command"))
                    {
                        found.push(format!("{}:{}", path.display(), number + 1));
                    }
                    if [
                        "disallowed_method",
                        "clippy::all",
                        "clippy::style",
                        "allow(warnings",
                        "expect(warnings",
                    ]
                    .iter()
                    .any(|lift| line.contains(lift))
                    {
                        found.push(format!(
                            "{}:{} (lifts the start rule)",
                            path.display(),
                            number + 1
                        ));
                    }
                }
            }
        }
        assert!(
            found.is_empty(),
            "a child process made or started without crate::process: {found:?}"
        );
    }

    #[test]
    fn main_takes_back_sigchld_before_anything_else() {
        // The reset protects every child xfx makes, so it is the first thing
        // `main` does: read off the source, as the case above reads it.
        let main = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs"),
        )
        .expect("main.rs");
        let body = main
            .split_once("fn main() -> ExitCode {")
            .expect("fn main")
            .1;
        let first = body
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty() && !line.starts_with("//") && !line.starts_with("#["));
        assert_eq!(
            first,
            Some("if let Err(err) = xfx::process::collect_own_children() {"),
            "main no longer puts SIGCHLD back first"
        );
    }

    /// Set in the test process the case below starts, which runs only the
    /// probe half.
    const SIGCHLD_PROBE: &str = "XFX_TEST_SIGCHLD_PROBE";

    #[test]
    fn a_child_is_collected_even_when_xfx_was_started_ignoring_sigchld() {
        // `SIGCHLD` ignored makes the kernel collect every child as it exits:
        // `wait` then fails, and the id of a process group whose leader xfx
        // is about to signal can be handed out again first. The disposition
        // is the whole process's, so the case runs in a process of its own.
        if std::env::var_os(SIGCHLD_PROBE).is_some() {
            return sigchld_probe();
        }
        use std::os::unix::process::CommandExt;

        let mut probe = std::process::Command::new(std::env::current_exe().expect("this binary"));
        probe
            .args([
                "--exact",
                "process::tests::a_child_is_collected_even_when_xfx_was_started_ignoring_sigchld",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(SIGCHLD_PROBE, "1");
        // The launcher's ignore, set where a launcher sets it: in the child,
        // before `exec`, which keeps it. Not through a shell's `trap`, which
        // dash does not hand on.
        // SAFETY: as `clean_signals`: the closure allocates nothing and calls
        // only `sigaction` on a value it owns by copy.
        let ignore = unsafe {
            let mut ignore: libc::sigaction = std::mem::zeroed();
            ignore.sa_sigaction = libc::SIG_IGN;
            libc::sigemptyset(&mut ignore.sa_mask);
            ignore
        };
        // SAFETY: as above.
        unsafe {
            probe.pre_exec(move || {
                if libc::sigaction(libc::SIGCHLD, &ignore, std::ptr::null_mut()) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let output = super::output(&mut probe).expect("the probe ran");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success() && stdout.contains("sigchld probe: collected"),
            "{}{}",
            stdout,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn sigchld_probe() {
        // SAFETY: a null new action makes `sigaction` a pure query.
        let ignored = unsafe {
            let mut current: libc::sigaction = std::mem::zeroed();
            libc::sigaction(libc::SIGCHLD, std::ptr::null(), &mut current) == 0
                && current.sa_sigaction == libc::SIG_IGN
        };
        assert!(ignored, "the probe was not started ignoring SIGCHLD");
        collect_own_children().expect("SIGCHLD taken back");
        let status = super::status(command("/bin/sh").args(["-c", "exit 3"]))
            .expect("an inherited SIGCHLD ignore collected the child first");
        assert_eq!(status.code(), Some(3));
        println!("sigchld probe: collected");
    }
}
