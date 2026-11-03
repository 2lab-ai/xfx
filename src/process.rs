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
//! The case `no_child_process_is_made_any_other_way` keeps every
//! `Command::new` in the library's own code in this file.

use std::ffi::OsStr;
use std::process::Command;

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

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

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
            undoing_the_runners_ignores(command("/bin/sh"), &[libc::SIGTERM])
                .args(["-c", "kill -TERM $$; echo survived"])
                .output()
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
                undoing_the_runners_ignores(command("/bin/sleep"), fixture.as_slice())
                    .arg("30")
                    .spawn()
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
                undoing_the_runners_ignores(command("/bin/sh"), &[signal])
                    .args(["-c", &script])
                    .output()
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
        let output = runner.output().expect("a runner");
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
    /// under it -- may start what it likes.
    #[test]
    fn no_child_process_is_made_any_other_way() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut stack = vec![root.clone()];
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
                let text = std::fs::read_to_string(&path).expect("a source file");
                let lines: Vec<&str> = text.lines().collect();
                for (number, line) in lines.iter().enumerate() {
                    let opens_tests = (line.starts_with("#[cfg(test)]")
                        || line.starts_with("#[cfg(all(test"))
                        && lines[number + 1..]
                            .iter()
                            .find(|next| !next.starts_with("#["))
                            .is_some_and(|next| next.contains("mod "));
                    if opens_tests {
                        break;
                    }
                    if line.trim_start().starts_with("//") {
                        continue;
                    }
                    if line.contains("Command::new(") || line.contains("tokio::process::Command") {
                        found.push(format!("{}:{}", path.display(), number + 1));
                    }
                }
            }
        }
        assert!(
            found.is_empty(),
            "a child process made without crate::process::command: {found:?}"
        );
    }
}
