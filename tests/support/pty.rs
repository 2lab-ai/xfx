//! The pseudoterminal harness both acceptance suites drive the real binary
//! through.
//!
//! It lives here rather than in one suite because a copy is not a harness: the
//! retained slave descriptor, the non-blocking master, and the field-by-field
//! `TerminalState` comparator are each the answer to a way an earlier version
//! of these tests passed while proving nothing, and a second copy would drift
//! back into that. The reasons travel with the code, in the doc comments below.

use std::ffi::CString;
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use rustix::fs::{fcntl_getfl, fcntl_setfl, Mode, OFlags};
use rustix::process::{waitid, Pid, Signal, WaitId, WaitIdOptions};
use rustix::pty::{grantpt, openpt, ptsname, unlockpt, OpenptFlags};
use rustix::termios::{
    tcgetattr, tcgetwinsize, ControlModes, InputModes, LocalModes, OutputModes, SpecialCodeIndex,
    Termios, Winsize,
};

/// How long a test waits for expected output before failing.
pub const WAIT: Duration = Duration::from_secs(20);

/// How long the harness sleeps when a non-blocking descriptor has nothing yet.
pub const IDLE_POLL: Duration = Duration::from_millis(2);

/// How many times a pty allocation is attempted before the harness gives up.
///
/// Allocating a pty is not purely a function of this process: `/dev/ptmx` is a
/// shared, bounded resource, and on a host running several test suites (and
/// several terminals) at once the kernel can refuse an allocation that would
/// succeed a few milliseconds later. Observed on macOS as
/// `open a pty master: Os { code: -6 }` in roughly one full
/// `--features fault-injection --test tui` run in eighteen, taking two tests
/// down in the same run, with `kern.tty.ptmx_max` at 511 and a steady draw of
/// about 160 -- spike contention, not exhaustion.
///
/// Ten attempts spaced by `PTY_RETRY_PAUSE` cover such a spike without hiding
/// anything: a shortage that is real outlives eighty milliseconds, and
/// `retrying` still panics with the kernel's own error and the number of
/// attempts behind it.
const PTY_ATTEMPTS: u32 = 10;

/// How long the harness waits before re-attempting a refused pty allocation.
const PTY_RETRY_PAUSE: Duration = Duration::from_millis(8);

// ---------------------------------------------------------------------------
// the pseudoterminal harness
// ---------------------------------------------------------------------------

/// A pty pair: the side a test writes to and reads from, the device name the
/// child opens as its stdin, stdout, and stderr, and **one slave descriptor
/// held open for the pty's whole life**.
///
/// That retained descriptor is the difference between a real test and a
/// comforting one. A pty's line discipline is reinitialized to the system
/// defaults when its **last** slave descriptor closes, so a harness that opened
/// a slave, read `termios`, and closed it again was measuring freshly reset
/// defaults both before the child ran and after it exited -- and would have
/// reported "the terminal was left exactly as it was found" for a child that
/// left it in raw mode with echo off.
/// `the_harness_can_tell_when_a_child_leaves_the_terminal_changed` is the proof
/// that it no longer does.
///
/// The master is non-blocking. With a slave held open the child's exit no
/// longer closes the last one, so a read on the master never reaches EOF; a
/// blocking reader thread would then never return and every `Session` would
/// hang on drop.
pub struct Pty {
    pub master: Arc<File>,
    /// Held, never read from. Its only job is to exist.
    ///
    /// `None` only in the harness's own self-test, which reproduces the earlier
    /// version's blind spot deliberately.
    pub slave: Option<File>,
    pub slave_path: PathBuf,
}

impl Pty {
    pub fn open() -> Self {
        let master = retrying("allocate a pty master", Self::allocate);
        let name: CString = ptsname(&master, Vec::new()).expect("name the pty slave");
        let slave_path = PathBuf::from(name.to_str().expect("the slave name is utf-8").to_string());
        let flags = fcntl_getfl(&master).expect("read the master's flags");
        fcntl_setfl(&master, flags | OFlags::NONBLOCK).expect("make the master non-blocking");
        let slave = open_slave(&slave_path);
        Self {
            master: Arc::new(File::from(master)),
            slave: Some(slave),
            slave_path,
        }
    }

    /// One attempt at a fresh, granted, unlocked master.
    ///
    /// The three calls are retried as a single unit rather than one by one: a
    /// `grantpt` or `unlockpt` that failed did so for the master this attempt
    /// had just been handed, and re-running it against that same master asks
    /// the kernel the question it has already refused. Returning the error
    /// drops the `OwnedFd`, which hands the pty back before the next attempt --
    /// so a retry cannot itself become the contention it is waiting out.
    fn allocate() -> Result<OwnedFd, rustix::io::Errno> {
        let master = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY)?;
        grantpt(&master)?;
        unlockpt(&master)?;
        Ok(master)
    }

    /// The pty as an earlier version of this harness had it: nothing retained,
    /// every reading opening a slave and closing it again.
    ///
    /// It exists so that the blind spot can be demonstrated rather than
    /// described. See
    /// `on_macos_a_harness_that_retains_nothing_cannot_see_the_change`, which is
    /// its only caller and, for the reason given there, runs only on macOS --
    /// hence the `cfg`, which keeps this from being dead code elsewhere.
    #[cfg(target_os = "macos")]
    pub fn open_without_a_retained_slave() -> Self {
        let mut pty = Self::open();
        pty.slave = None;
        pty
    }

    /// The line-discipline state of the terminal, as a caller would see it.
    ///
    /// Read through the retained slave. Not through the master: BSD-derived
    /// kernels, macOS among them, answer `tcgetattr` on a pty master with
    /// `ENOTTY`. Not through a freshly opened one either, for the reason in the
    /// type's own documentation.
    pub fn try_termios(&self) -> Result<Termios, rustix::io::Errno> {
        match &self.slave {
            Some(slave) => tcgetattr(slave.as_fd()),
            None => tcgetattr(open_slave(&self.slave_path).as_fd()),
        }
    }

    /// The terminal's dimensions. A shell that resized the window would be
    /// changing state it was lent, exactly like a mode flag.
    pub fn try_winsize(&self) -> Result<Winsize, rustix::io::Errno> {
        match &self.slave {
            Some(slave) => tcgetwinsize(slave.as_fd()),
            None => tcgetwinsize(open_slave(&self.slave_path).as_fd()),
        }
    }
}

/// Opens a pty slave without letting it become this process's terminal.
///
/// `O_NOCTTY` matters on both sides: the test process must not acquire the
/// child's terminal by holding a descriptor on it, and the child claims it
/// deliberately with `TIOCSCTTY` rather than by accident of opening.
///
/// Retried like the master's allocation, and for the same reason: the pressure
/// that makes `/dev/ptmx` refuse an allocation is descriptor and device
/// pressure, and opening the slave it just handed out draws on the same pool.
/// A slave device that is genuinely not there is still an error -- eighty
/// milliseconds later, with the count in the message.
pub fn open_slave(path: &Path) -> File {
    let fd = retrying("open the pty slave", || {
        rustix::fs::open(path, OFlags::RDWR | OFlags::NOCTTY, Mode::empty())
    });
    File::from(fd)
}

/// Runs `attempt` until it succeeds, up to `PTY_ATTEMPTS` times, and panics
/// with the kernel's own error and the number of attempts when it never does.
///
/// The panic is the point. A harness that quietly returned something usable
/// after a failure -- or that retried without a bound -- would turn "this host
/// cannot allocate a pty" into a mystery somewhere downstream, or into a suite
/// that hangs instead of failing. Retrying only widens the window in which a
/// *transient* shortage resolves; a persistent one still ends the test, and the
/// attempt count in the message is what tells the two apart afterwards.
///
/// There is no unit test below this. Making `openpt` fail on demand means
/// substituting the syscall, and a test against a substituted `openpt` would
/// prove that the substitute returns what it was told to -- not that this
/// harness survives a real shortage. The evidence for that is the field
/// observation quoted on `PTY_ATTEMPTS` and the suite runs that no longer
/// reproduce it.
fn retrying<T>(what: &str, mut attempt: impl FnMut() -> Result<T, rustix::io::Errno>) -> T {
    let mut refused = None;
    for tried in 1..=PTY_ATTEMPTS {
        if tried > 1 {
            thread::sleep(PTY_RETRY_PAUSE);
        }
        match attempt() {
            Ok(value) => return value,
            Err(err) => refused = Some(err),
        }
    }
    let refused = refused.expect("a pty is attempted at least once");
    panic!("{what}: {refused:?}, after {PTY_ATTEMPTS} attempts {PTY_RETRY_PAUSE:?} apart")
}

/// The real `xfx` binary running on a pty, with everything it wrote captured.
pub struct Session {
    pub child: Child,
    pub master: Arc<File>,
    pub output: Arc<Mutex<Vec<u8>>>,
    pub reading: Arc<AtomicBool>,
    pub reader: Option<JoinHandle<()>>,
    pub exited: Option<ExitStatus>,
}

impl Session {
    /// Spawns `command` with a pty on all three standard streams.
    ///
    /// The child gets its own session and claims the pty as its controlling
    /// terminal, because that is what makes a typed Ctrl-C become a real SIGINT
    /// in the child's foreground process group rather than a byte in a buffer.
    pub fn spawn(pty: &Pty, command: Command) -> Self {
        Self::spawn_owning(pty, command, true)
    }

    /// Spawns a child on the pty that does **not** take it as a controlling
    /// terminal.
    ///
    /// Only the harness's own self-tests use this. A session leader's terminal
    /// is revoked when it exits on BSD-derived kernels, which makes "what did
    /// the child leave behind" unanswerable there; a child that never claimed
    /// the terminal leaves it inspectable, which is what makes the harness's
    /// blind spot demonstrable.
    pub fn spawn_without_taking_the_terminal(pty: &Pty, command: Command) -> Self {
        Self::spawn_owning(pty, command, false)
    }

    fn spawn_owning(pty: &Pty, mut command: Command, take_terminal: bool) -> Self {
        let slave = open_slave(&pty.slave_path);
        let stdin = slave.try_clone().expect("clone the pty slave");
        let stdout = slave.try_clone().expect("clone the pty slave");
        command
            .stdin(Stdio::from(stdin))
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(slave));
        if take_terminal {
            // SAFETY: both calls are async-signal-safe and touch only this
            // child's own session and controlling terminal. `std` has already
            // dup'd the pty onto 0/1/2 by the time a `pre_exec` closure runs,
            // so fd 0 is the terminal being claimed.
            unsafe {
                command.pre_exec(|| {
                    rustix::process::setsid()?;
                    rustix::process::ioctl_tiocsctty(BorrowedFd::borrow_raw(0))?;
                    Ok(())
                });
            }
        }
        let child = command.spawn().expect("spawn xfx on the pty");

        let master = Arc::clone(&pty.master);
        let output = Arc::new(Mutex::new(Vec::new()));
        let reading = Arc::new(AtomicBool::new(true));
        let thread_master = Arc::clone(&master);
        let thread_output = Arc::clone(&output);
        let thread_reading = Arc::clone(&reading);
        let reader = thread::spawn(move || {
            let mut buffer = [0u8; 4096];
            while thread_reading.load(Ordering::SeqCst) {
                match (&*thread_master).read(&mut buffer) {
                    // A pty whose last slave has closed reads as EOF on some
                    // platforms and as EIO on others. Neither happens here --
                    // the harness holds a slave open on purpose -- so the loop
                    // ends on the flag instead, which is why the master is
                    // non-blocking. Both cases are still handled: the reader
                    // must not spin on a descriptor that is genuinely finished.
                    Ok(0) => break,
                    Ok(read) => thread_output
                        .lock()
                        .expect("output lock")
                        .extend_from_slice(&buffer[..read]),
                    Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(IDLE_POLL);
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            child,
            master,
            output,
            reading,
            reader: Some(reader),
            exited: None,
        }
    }

    /// Everything the terminal has received so far, decoded leniently: a test
    /// asserts on text, and a partially read multibyte character is not a
    /// failure.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.output.lock().expect("output lock")).into_owned()
    }

    /// Types `bytes` on the terminal, exactly as a keyboard would.
    ///
    /// Written in a loop because the master is non-blocking: a full input queue
    /// is a `WouldBlock`, not a failure, and the reader on the other side is a
    /// real process that will get to it.
    pub fn type_bytes(&self, bytes: &[u8]) {
        let deadline = Instant::now() + WAIT;
        let mut rest = bytes;
        while !rest.is_empty() {
            match (&*self.master).write(rest) {
                Ok(0) => panic!("the terminal accepted nothing"),
                Ok(written) => rest = &rest[written..],
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                    assert!(
                        Instant::now() < deadline,
                        "the terminal never accepted input; terminal so far:\n{}",
                        self.text()
                    );
                    thread::sleep(IDLE_POLL);
                }
                Err(err) => panic!("write to the terminal: {err}"),
            }
        }
    }

    /// Types a line and presses Return. Return is a carriage return on a
    /// terminal; the line discipline is what turns it into a newline.
    pub fn type_line(&self, line: &str) {
        self.type_bytes(line.as_bytes());
        self.type_bytes(b"\r");
    }

    /// Waits until the terminal has received `needle`, and returns everything
    /// received so far.
    pub fn wait_for(&self, needle: &str) -> String {
        self.wait_until(&format!("{needle:?} on the terminal"), |text| {
            text.contains(needle)
        })
    }

    /// Waits until `needle` has been received `count` times.
    pub fn wait_for_count(&self, needle: &str, count: usize) -> String {
        self.wait_until(&format!("{count} x {needle:?} on the terminal"), |text| {
            text.matches(needle).count() >= count
        })
    }

    pub fn wait_until(&self, what: &str, ready: impl Fn(&str) -> bool) -> String {
        let deadline = Instant::now() + WAIT;
        loop {
            let text = self.text();
            if ready(&text) {
                return text;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {what}; terminal so far:\n{text}"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    /// Waits for the process to exit and returns its status.
    pub fn wait_exit(&mut self) -> ExitStatus {
        if let Some(status) = self.exited {
            return status;
        }
        let deadline = Instant::now() + WAIT;
        loop {
            match self.child.try_wait().expect("poll the child") {
                Some(status) => {
                    self.exited = Some(status);
                    return status;
                }
                None => {
                    assert!(
                        Instant::now() < deadline,
                        "xfx did not exit; terminal so far:\n{}",
                        self.text()
                    );
                    thread::sleep(Duration::from_millis(10));
                }
            }
        }
    }

    /// Exits through `/quit` and requires a clean status.
    pub fn quit(&mut self) -> ExitStatus {
        self.type_line("/quit");
        self.wait_exit()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if self.exited.is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        self.reading.store(false, Ordering::SeqCst);
        if let Some(reader) = self.reader.take() {
            // The reader ends when the last slave descriptor closes, which the
            // child's exit guarantees.
            let _ = reader.join();
        }
    }
}

// ---------------------------------------------------------------------------
// what the terminal looks like
// ---------------------------------------------------------------------------

/// Every terminal fact a shell must not silently change.
///
/// Compared field by field as an exact tuple. An earlier version OR-ed the
/// input and output flags into one integer before comparing, which was wrong in
/// the direction that matters: the two words have overlapping bit values, so a
/// bit gained in one and lost in the other cancels out and a changed terminal
/// compares equal. There is no reason to fold them at all.
///
/// `VMIN` and `VTIME` are in here because they are precisely what raw mode
/// rewrites -- a shell can leave `ICANON` looking untouched and still have left
/// the read behaviour changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalState {
    pub input: InputModes,
    pub output: OutputModes,
    pub control: ControlModes,
    pub local: LocalModes,
    pub min: u8,
    pub time: u8,
    pub size: (u16, u16),
}

/// The terminal's state, when the kernel still allows the question to be asked.
pub fn try_modes(pty: &Pty) -> Option<TerminalState> {
    let state = pty.try_termios().ok()?;
    let size = pty.try_winsize().ok()?;
    Some(TerminalState {
        input: state.input_modes,
        output: state.output_modes,
        control: state.control_modes,
        local: state.local_modes,
        min: state.special_codes[SpecialCodeIndex::VMIN],
        time: state.special_codes[SpecialCodeIndex::VTIME],
        size: (size.ws_row, size.ws_col),
    })
}

/// The terminal's state, for a terminal that is still one.
pub fn modes(pty: &Pty) -> TerminalState {
    try_modes(pty).expect("the pty is still a terminal")
}

impl Session {
    /// The child's process id, for a test that has to signal it rather than
    /// type at it.
    pub fn pid(&self) -> Pid {
        Pid::from_raw(self.child.id() as i32).expect("the child has a pid")
    }

    /// Sends `signal` to the child, exactly as an operator or a supervisor
    /// would.
    pub fn signal(&self, signal: Signal) {
        rustix::process::kill_process(self.pid(), signal).expect("signal the child");
    }

    /// What the child is doing, answered from this session's own record of the
    /// status once there is one.
    ///
    /// `wait_exit` and `Drop` collect the child, and a collected child is one
    /// the kernel will not answer questions about any more: `waitid` fails with
    /// `ECHILD`, because there is no longer a process or a zombie to describe.
    /// The recorded `ExitStatus` is then the only truthful answer, so it is
    /// consulted first. While the child is still uncollected the kernel is
    /// asked instead, without consuming anything -- see `wait_state`.
    pub fn state(&self) -> Wait {
        match self.exited {
            Some(status) => collected_state(status),
            None => wait_state(self.pid()),
        }
    }

    /// Waits until `ready` describes the child, so a test can assert on a
    /// process that is *stopped* -- which `Child::try_wait` cannot report,
    /// because it does not pass `WUNTRACED`.
    pub fn wait_state(&self, what: &str, ready: impl Fn(&Wait) -> bool) -> Wait {
        let deadline = Instant::now() + WAIT;
        loop {
            let state = self.state();
            if ready(&state) {
                return state;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {what}; child is {state:?}; terminal so far:\n{}",
                self.text()
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
}

/// What a child is doing, including the three states `Child::try_wait` hides.
///
/// `Continued` is a state of its own rather than a flavour of `Running`. A
/// child that was stopped and resumed is *running again*, which is a different
/// claim from "was never interrupted", and a test that resumes a job asserts on
/// exactly that difference; folding the two together would let
/// "the resume never happened" pass as "the child is fine".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wait {
    /// Running, with nothing to report since the last reading.
    Running,
    /// Resumed by `SIGCONT` since it was last stopped, and running now.
    Continued,
    /// Stopped by the given signal, and still stopped.
    Stopped(i32),
    /// Exited with the given status code.
    Exited(i32),
    /// Killed by the given signal.
    Signalled(i32),
}

/// What the child `pid` is doing, **without consuming the answer**.
///
/// `waitid` with `NOWAIT`, not `waitpid`, and the difference is the whole
/// correctness of this function:
///
/// * `waitpid` *consumes* the event it reports. Asked twice about one stopped
///   child it answers "stopped" once and "nothing to report" afterwards, so a
///   second reading of a child that is still stopped came back as `Running` --
///   a lie, and precisely the lie a test that asserts on a stopped process
///   exists to catch.
/// * `waitpid` also *reaps* a child whose status is terminal, which steals the
///   status `Session::wait_exit` and `Session::drop` collect through
///   `Child::try_wait`. `std` would then fail with `ECHILD` on a child this
///   harness had already buried behind its back.
///
/// With `NOWAIT` the kernel keeps both the notification and the zombie, so
/// every reading is repeatable and `std` still owns the burial. Nothing else
/// here can consume a stop or a resume either: `Child::try_wait` passes
/// `WNOHANG` alone -- neither `WUNTRACED` nor `WCONTINUED` -- so it can only
/// ever collect a child that has terminated.
///
/// Every error is a panic rather than a state. An earlier version answered
/// `Running` for any `Err`, which turned "this pid is not mine" into "your
/// child is fine" and left the caller to blame the resulting timeout on the
/// child. A harness may fail, but it may not lie.
pub fn wait_state(pid: Pid) -> Wait {
    let reported = match waitid(
        WaitId::Pid(pid),
        WaitIdOptions::NOHANG
            | WaitIdOptions::EXITED
            | WaitIdOptions::STOPPED
            | WaitIdOptions::CONTINUED
            | WaitIdOptions::NOWAIT,
    ) {
        Ok(reported) => reported,
        Err(rustix::io::Errno::CHILD) => panic!(
            "{pid:?} is not a child of this process, or has already been collected; \
             a collected child is only knowable through the `Session` that kept its status"
        ),
        Err(err) => panic!("waitid on {pid:?}: {err}"),
    };
    // Nothing reported means the kernel has nothing to say about a child it
    // still has: it is running. Neither a stopped child nor a zombie can hide
    // here, because `NOWAIT` leaves both of their reports standing.
    let Some(reported) = reported else {
        return Wait::Running;
    };
    if let Some(signal) = reported.stopping_signal() {
        Wait::Stopped(signal)
    } else if let Some(signal) = reported.terminating_signal() {
        Wait::Signalled(signal)
    } else if let Some(code) = reported.exit_status() {
        Wait::Exited(code)
    } else if reported.continued() {
        Wait::Continued
    } else {
        panic!(
            "waitid reported a child state this harness does not model: si_code {}",
            reported.raw_code()
        )
    }
}

/// The `Wait` an already-collected `ExitStatus` describes.
fn collected_state(status: ExitStatus) -> Wait {
    if let Some(signal) = status.signal() {
        Wait::Signalled(signal)
    } else if let Some(code) = status.code() {
        Wait::Exited(code)
    } else {
        panic!("a collected child neither exited nor was signalled: {status:?}")
    }
}

impl Pty {
    /// Fixes the terminal's dimensions before a child is spawned, so a layout
    /// assertion is about geometry rather than about whatever window the
    /// developer happened to have open.
    pub fn resize(&self, rows: u16, cols: u16) {
        let size = Winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let slave = self.slave.as_ref().expect("the harness retains a slave");
        rustix::termios::tcsetwinsize(slave.as_fd(), size).expect("set the pty size");
    }
}
