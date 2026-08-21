//! Acceptance tests for the interactive shell, driven through a real
//! pseudoterminal.
//!
//! The shell is the one xfx surface whose contract is *about* the terminal: it
//! must refuse to run without one, keep the scrollback it was given, leave the
//! line discipline exactly as it found it, and survive an interrupt. None of
//! that can be proven against a pipe pretending to be a TTY, so every test here
//! allocates a pty, spawns the real binary on it, and asserts on the bytes the
//! terminal actually received and on the `termios` state that was left behind.
//!
//! Nothing here uses a real credential or a real endpoint. Upstream evidence is
//! pinned to `vercel-labs/fx@580a0c5da9386317251968c09c1cee69e763487a`.

#![cfg(unix)]

mod support;

use std::ffi::CString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::{AsFd, BorrowedFd};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use rustix::fs::{fcntl_getfl, fcntl_setfl, Mode, OFlags};
use rustix::pty::{grantpt, openpt, ptsname, unlockpt, OpenptFlags};
use rustix::termios::{
    tcgetattr, tcgetwinsize, ControlModes, InputModes, LocalModes, OutputModes, SpecialCodeIndex,
    Termios, Winsize,
};
use serde_json::Value;
use tempfile::TempDir;

use support::fake_gateway::{content_only, sse_body, text_delta, FakeGateway, Reply};

/// Environment variables that must never leak in from the developer's shell.
const CONTROLLED_VARS: &[&str] = &[
    "VERCEL_OIDC_TOKEN",
    "AI_GATEWAY_API_KEY",
    "XFX_MODEL",
    "XFX_PERMISSION_MODE",
    "XFX_MAX_AGENT_STEPS",
    "XFX_GATEWAY_URL",
];

/// A test secret that must never appear on the terminal.
const TEST_KEY: &str = "xfx-test-interactive-key-must-not-appear";

/// How long a test waits for expected output before failing.
const WAIT: Duration = Duration::from_secs(20);

/// How long the harness sleeps when a non-blocking descriptor has nothing yet.
const IDLE_POLL: Duration = Duration::from_millis(2);

/// The prompt the shell writes before reading a line.
const PROMPT: &str = "> ";

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
struct Pty {
    master: Arc<File>,
    /// Held, never read from. Its only job is to exist.
    ///
    /// `None` only in the harness's own self-test, which reproduces the earlier
    /// version's blind spot deliberately.
    slave: Option<File>,
    slave_path: PathBuf,
}

impl Pty {
    fn open() -> Self {
        let master = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY).expect("open a pty master");
        grantpt(&master).expect("grant the pty slave");
        unlockpt(&master).expect("unlock the pty slave");
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

    /// The pty as an earlier version of this harness had it: nothing retained,
    /// every reading opening a slave and closing it again.
    ///
    /// It exists so that the blind spot can be demonstrated rather than
    /// described. See
    /// `on_macos_a_harness_that_retains_nothing_cannot_see_the_change`, which is
    /// its only caller and, for the reason given there, runs only on macOS --
    /// hence the `cfg`, which keeps this from being dead code elsewhere.
    #[cfg(target_os = "macos")]
    fn open_without_a_retained_slave() -> Self {
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
    fn try_termios(&self) -> Result<Termios, rustix::io::Errno> {
        match &self.slave {
            Some(slave) => tcgetattr(slave.as_fd()),
            None => tcgetattr(open_slave(&self.slave_path).as_fd()),
        }
    }

    /// The terminal's dimensions. A shell that resized the window would be
    /// changing state it was lent, exactly like a mode flag.
    fn try_winsize(&self) -> Result<Winsize, rustix::io::Errno> {
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
fn open_slave(path: &Path) -> File {
    let fd = rustix::fs::open(path, OFlags::RDWR | OFlags::NOCTTY, Mode::empty())
        .expect("open the pty slave");
    File::from(fd)
}

/// The real `xfx` binary running on a pty, with everything it wrote captured.
struct Session {
    child: Child,
    master: Arc<File>,
    output: Arc<Mutex<Vec<u8>>>,
    reading: Arc<AtomicBool>,
    reader: Option<JoinHandle<()>>,
    exited: Option<ExitStatus>,
}

impl Session {
    /// Spawns `command` with a pty on all three standard streams.
    ///
    /// The child gets its own session and claims the pty as its controlling
    /// terminal, because that is what makes a typed Ctrl-C become a real SIGINT
    /// in the child's foreground process group rather than a byte in a buffer.
    fn spawn(pty: &Pty, command: Command) -> Self {
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
    fn spawn_without_taking_the_terminal(pty: &Pty, command: Command) -> Self {
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
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.output.lock().expect("output lock")).into_owned()
    }

    /// Types `bytes` on the terminal, exactly as a keyboard would.
    ///
    /// Written in a loop because the master is non-blocking: a full input queue
    /// is a `WouldBlock`, not a failure, and the reader on the other side is a
    /// real process that will get to it.
    fn type_bytes(&self, bytes: &[u8]) {
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
    fn type_line(&self, line: &str) {
        self.type_bytes(line.as_bytes());
        self.type_bytes(b"\r");
    }

    /// Waits until the terminal has received `needle`, and returns everything
    /// received so far.
    fn wait_for(&self, needle: &str) -> String {
        self.wait_until(&format!("{needle:?} on the terminal"), |text| {
            text.contains(needle)
        })
    }

    /// Waits until `needle` has been received `count` times.
    fn wait_for_count(&self, needle: &str, count: usize) -> String {
        self.wait_until(&format!("{count} x {needle:?} on the terminal"), |text| {
            text.matches(needle).count() >= count
        })
    }

    fn wait_until(&self, what: &str, ready: impl Fn(&str) -> bool) -> String {
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
    fn wait_exit(&mut self) -> ExitStatus {
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
    fn quit(&mut self) -> ExitStatus {
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
// fixtures
// ---------------------------------------------------------------------------

struct Sandbox {
    _root: TempDir,
    home: PathBuf,
    workspace: PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        let root = TempDir::new().expect("create a sandbox root");
        let home = root.path().join("home");
        let workspace = root.path().join("workspace");
        fs::create_dir_all(&home).expect("create home");
        fs::create_dir_all(&workspace).expect("create workspace");
        let home = home.canonicalize().expect("canonicalize home");
        let workspace = workspace.canonicalize().expect("canonicalize workspace");
        Self {
            _root: root,
            home,
            workspace,
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_xfx"));
        command.current_dir(&self.workspace);
        command.env("HOME", &self.home);
        // A shell is a terminal program; a terminal that claims to be nothing
        // is still a terminal, and the shell must not depend on this.
        command.env("TERM", "dumb");
        for key in CONTROLLED_VARS {
            command.env_remove(key);
        }
        command
    }

    /// A command wired to a scripted local Gateway.
    fn command_with(&self, gateway: &FakeGateway) -> Command {
        let mut command = self.command();
        command.env("AI_GATEWAY_API_KEY", TEST_KEY);
        command.env("XFX_GATEWAY_URL", gateway.chat_url());
        command
    }

    fn sessions_dir(&self) -> PathBuf {
        self.home.join(".xfx").join("sessions")
    }

    /// Every session directory currently in the store, sorted.
    fn session_ids(&self) -> Vec<String> {
        let Ok(entries) = fs::read_dir(self.sessions_dir()) else {
            return Vec::new();
        };
        let mut ids: Vec<String> = entries
            .map(|entry| entry.expect("read a session directory"))
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        ids.sort();
        ids
    }
}

/// Starts a shell and waits for its first prompt.
fn start(sandbox: &Sandbox, pty: &Pty, command: Command) -> Session {
    let session = Session::spawn(pty, command);
    session.wait_for(PROMPT);
    let _ = sandbox;
    session
}

/// The JSON body of the request the Gateway received at `index`.
fn request_body(gateway: &FakeGateway, index: usize) -> Value {
    let requests = gateway.requests();
    assert!(
        requests.len() > index,
        "expected at least {} request(s), got {}",
        index + 1,
        requests.len()
    );
    requests[index].json()
}

/// The text of every user message in a request body, in order.
fn user_messages(body: &Value) -> Vec<String> {
    body["prompt"]
        .as_array()
        .expect("the request carries a prompt")
        .iter()
        .filter(|message| message["role"] == "user")
        .filter_map(|message| message["content"].as_array())
        .flat_map(|parts| parts.iter())
        .filter_map(|part| part["text"].as_str())
        .map(str::to_string)
        .collect()
}

// ---------------------------------------------------------------------------
// a shell needs a terminal
// ---------------------------------------------------------------------------

#[test]
fn a_bare_invocation_without_a_terminal_is_refused_before_anything_runs() {
    let sandbox = Sandbox::new();
    let output = sandbox
        .command()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("spawn xfx");

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty(), "stdout must stay empty");
    let stderr = String::from_utf8(output.stderr).expect("stderr is utf-8");
    assert!(
        stderr.contains("interactive terminal") && stderr.contains("xfx ask"),
        "the refusal must name the requirement and the alternative: {stderr}"
    );
    // Refusing is not the same as failing halfway through starting: nothing was
    // created under the profile home.
    assert!(!sandbox.home.join(".xfx").exists(), "the home was touched");
}

#[test]
fn a_shell_whose_answers_would_go_into_a_pipe_is_refused() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let slave = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&pty.slave_path)
        .expect("open the pty slave");
    let output = sandbox
        .command()
        .stdin(Stdio::from(slave))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("spawn xfx");

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty(), "stdout must stay empty");
    let stderr = String::from_utf8(output.stderr).expect("stderr is utf-8");
    assert!(stderr.contains("interactive terminal"), "{stderr}");
}

#[test]
fn a_shell_with_nowhere_to_record_says_so_instead_of_forgetting_the_conversation() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut command = sandbox.command();
    command.env_remove("HOME");
    let mut session = Session::spawn(&pty, command);

    let text = session.wait_for("cannot record");
    assert!(
        text.contains("--no-save"),
        "the refusal must say what is missing and what to use instead: {text}"
    );
    assert_eq!(session.wait_exit().code(), Some(1));
}

// ---------------------------------------------------------------------------
// the prompt loop
// ---------------------------------------------------------------------------

#[test]
fn the_shell_opens_with_what_it_resolved_and_a_prompt() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command());

    let text = session.text();
    assert!(text.contains(env!("CARGO_PKG_VERSION")), "{text}");
    assert!(text.contains("permission_mode=auto"), "{text}");
    assert!(text.contains("sandbox=none"), "{text}");
    assert!(
        text.contains(&sandbox.workspace.display().to_string()),
        "{text}"
    );
    assert!(text.contains("/help"), "{text}");
    // An append shell never takes the alternate screen and never hides the
    // cursor: the transcript above it has to stay where the user left it.
    assert!(
        !text.contains("\u{1b}[?1049h"),
        "alternate screen: {text:?}"
    );
    assert!(!text.contains("\u{1b}[?25l"), "hidden cursor: {text:?}");

    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn an_empty_line_asks_nothing_and_prompts_again() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    // No Gateway at all: a blank line that reached the provider would fail to
    // connect, and the shell would say so.
    let mut session = start(&sandbox, &pty, sandbox.command());

    session.type_line("");
    session.type_line("   ");
    let text = session.wait_for_count(PROMPT, 3);
    assert!(!text.contains("xfx:"), "nothing was attempted: {text}");

    assert_eq!(session.quit().code(), Some(0));
    assert!(sandbox.session_ids().is_empty(), "nothing was recorded");
}

#[test]
fn the_shell_streams_an_answer_to_a_unicode_prompt_through_the_ordinary_turn_path() {
    let gateway = FakeGateway::start(vec![Reply::Sse(content_only(&["안녕", "하세요 — ", "✓"]))]);
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command_with(&gateway));

    session.type_line("설명해줘 — le café ☕");
    let text = session.wait_for("안녕하세요 — ✓");

    // The prompt reached the provider byte for byte, through the same request
    // shape `ask` uses.
    let body = request_body(&gateway, 0);
    assert_eq!(
        user_messages(&body),
        vec!["설명해줘 — le café ☕".to_string()]
    );
    assert!(
        body["tools"].as_array().expect("tools").len() > 1,
        "the shell advertises the same registry as ask"
    );
    assert!(
        !text.contains(TEST_KEY),
        "the credential reached the terminal"
    );

    assert_eq!(session.quit().code(), Some(0));
    assert_eq!(sandbox.session_ids().len(), 1, "the turn was recorded once");
}

#[test]
fn a_second_prompt_continues_the_same_conversation() {
    let gateway = FakeGateway::start(vec![
        Reply::Sse(content_only(&["first"])),
        Reply::Sse(content_only(&["second"])),
    ]);
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command_with(&gateway));

    session.type_line("one");
    session.wait_for("first");
    session.type_line("two");
    session.wait_for("second");

    let body = request_body(&gateway, 1);
    assert_eq!(
        user_messages(&body),
        vec!["one".to_string(), "two".to_string()],
        "the second request carries the first turn"
    );
    assert_eq!(session.quit().code(), Some(0));
    assert_eq!(sandbox.session_ids().len(), 1, "one session, two turns");
}

#[test]
fn a_failed_turn_is_reported_and_the_shell_keeps_going() {
    let gateway = FakeGateway::start(vec![
        Reply::Status(401, "{\"error\":\"unauthorized\"}".to_string()),
        Reply::Sse(content_only(&["recovered"])),
    ]);
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command_with(&gateway));

    session.type_line("first");
    session.wait_for("401");
    session.type_line("second");
    session.wait_for("recovered");

    assert_eq!(session.quit().code(), Some(0));
}

// ---------------------------------------------------------------------------
// the six slash commands
// ---------------------------------------------------------------------------

#[test]
fn slash_help_lists_exactly_the_six_commands() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command());

    session.type_line("/help");
    let text = session.wait_for_count(PROMPT, 2);
    for command in ["/help", "/new", "/clear", "/model", "/version", "/quit"] {
        assert!(text.contains(command), "help omits {command}: {text}");
    }
    // Advertisement is a promise here too: no deferred upstream slash command
    // may appear.
    for absent in [
        "/resume",
        "/status",
        "/login",
        "/setup",
        "/permissions",
        "/models",
        "/provider",
        "/mcp",
        "/undo",
        "/stats",
        "/usage",
        "/image",
    ] {
        assert!(!text.contains(absent), "help advertises {absent}: {text}");
    }

    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn slash_version_prints_the_same_version_the_command_line_does() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command());

    session.type_line("/version");
    let text = session.wait_for_count(PROMPT, 2);
    assert!(text.contains(env!("CARGO_PKG_VERSION")), "{text}");

    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn slash_model_reports_the_active_model_and_changes_it_for_later_turns() {
    let gateway = FakeGateway::start(vec![Reply::Sse(content_only(&["ok"]))]);
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command_with(&gateway));

    session.type_line("/model");
    let text = session.wait_for("model=");
    assert!(text.contains("zai/glm-5.2"), "{text}");

    session.type_line("/model acme/model-9");
    session.wait_for("model=acme/model-9");
    session.type_line("hello");
    session.wait_for("ok");

    // The model travels as the Gateway's own header, exactly as `ask` sends it.
    let requests = gateway.requests();
    assert_eq!(
        requests[0].header("ai-language-model-id"),
        Some("acme/model-9")
    );

    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn slash_model_refuses_an_unusable_name_rather_than_sending_it() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command());

    session.type_line("/model one two");
    let text = session.wait_for_count(PROMPT, 2);
    assert!(text.contains("one model"), "{text}");
    // The refusal did not change anything.
    session.type_line("/model");
    let text = session.wait_for("model=");
    assert!(text.contains("zai/glm-5.2"), "{text}");

    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn slash_clear_erases_the_screen_and_keeps_the_conversation() {
    let gateway = FakeGateway::start(vec![
        Reply::Sse(content_only(&["before"])),
        Reply::Sse(content_only(&["after"])),
    ]);
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command_with(&gateway));

    session.type_line("remember this");
    session.wait_for("before");
    let ids_before = sandbox.session_ids();

    session.type_line("/clear");
    let text = session.wait_for("\u{1b}[2J");
    assert!(text.contains("\u{1b}[3J"), "the scrollback was not erased");

    session.type_line("and now");
    session.wait_for("after");

    // Same session, and the cleared turn is still in the conversation.
    assert_eq!(sandbox.session_ids(), ids_before, "/clear changed identity");
    assert_eq!(
        user_messages(&request_body(&gateway, 1)),
        vec!["remember this".to_string(), "and now".to_string()],
        "/clear forgot the conversation"
    );

    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn slash_new_starts_a_second_session_that_remembers_nothing() {
    let gateway = FakeGateway::start(vec![
        Reply::Sse(content_only(&["first"])),
        Reply::Sse(content_only(&["second"])),
    ]);
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command_with(&gateway));

    session.type_line("one");
    session.wait_for("first");
    let ids_before = sandbox.session_ids();
    assert_eq!(ids_before.len(), 1);

    session.type_line("/new");
    session.wait_for_count(PROMPT, 3);
    session.type_line("two");
    session.wait_for("second");

    let ids_after = sandbox.session_ids();
    assert_eq!(ids_after.len(), 2, "a new identity was not created");
    assert_ne!(ids_after, ids_before);
    assert_eq!(
        user_messages(&request_body(&gateway, 1)),
        vec!["two".to_string()],
        "the new session inherited history"
    );

    assert_eq!(session.quit().code(), Some(0));
}

/// Every line xfx wrote that begins with `xfx: `, without the terminal's echo
/// of what was typed.
///
/// A refusal is what xfx *wrote*; the same bytes coming back as echo are the
/// terminal's doing and prove nothing. They are told apart by the prefix, which
/// only xfx writes.
fn diagnostics(text: &str) -> Vec<String> {
    text.split('\n')
        .map(|line| line.trim_end_matches('\r'))
        .filter(|line| line.starts_with("xfx: "))
        .map(str::to_string)
        .collect()
}

#[test]
fn an_unknown_slash_command_is_refused_with_the_same_words_every_time() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command());

    session.type_line("/nonesuch");
    session.wait_for("is not an xfx command");
    session.type_line("/nonesuch");
    let text = session.wait_for_count("is not an xfx command", 2);

    let refusals = diagnostics(&text);
    assert_eq!(refusals.len(), 2, "{text}");
    assert_eq!(refusals[0], refusals[1], "the refusal is not deterministic");
    assert_eq!(
        refusals[0],
        "xfx: `/nonesuch` is not an xfx command; /help lists the six it has"
    );

    // A slash command that is not one is never sent to a model: there is no
    // Gateway configured here, and no connection failure was reported.
    assert!(!text.contains("connect"), "{text}");
    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn an_unknown_command_cannot_paint_on_the_terminal_through_the_refusal() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command());

    // A line that starts with `/` is quoted back by xfx. If it were quoted
    // verbatim, this one would clear the screen, wipe the scrollback, retitle
    // the window, and then bury the guidance under 500 bytes of padding.
    session.type_bytes(b"/\x1b[2J\x1b[3J\x1b]0;pwned\x07\x1b[H");
    session.type_bytes("x".repeat(500).as_bytes());
    session.type_bytes(b"\r");
    let text = session.wait_for("is not an xfx command");

    let refusals = diagnostics(&text);
    assert_eq!(refusals.len(), 1, "{text}");
    let refusal = &refusals[0];
    assert!(
        !refusal.contains('\u{1b}'),
        "the refusal carries an escape: {refusal:?}"
    );
    assert!(
        !refusal.chars().any(char::is_control),
        "the refusal carries a control character: {refusal:?}"
    );
    assert!(
        refusal.len() < 200,
        "the refusal is unbounded ({} bytes)",
        refusal.len()
    );
    assert!(
        refusal.ends_with("/help lists the six it has"),
        "the guidance was pushed off the line: {refusal:?}"
    );
    // The shell is unharmed and still answering.
    session.type_line("/version");
    session.wait_for(env!("CARGO_PKG_VERSION"));
    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn a_prompt_that_merely_mentions_a_slash_is_still_a_prompt() {
    let gateway = FakeGateway::start(vec![Reply::Sse(content_only(&["answered"]))]);
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command_with(&gateway));

    session.type_line("what does a/b /help mean");
    session.wait_for("answered");
    assert_eq!(
        user_messages(&request_body(&gateway, 0)),
        vec!["what does a/b /help mean".to_string()]
    );

    assert_eq!(session.quit().code(), Some(0));
}

// ---------------------------------------------------------------------------
// interruption and exit
// ---------------------------------------------------------------------------

#[test]
fn ctrl_c_stops_a_running_turn_and_the_shell_survives_it() {
    // A started answer that never finishes: the only state a user can really
    // interrupt.
    let gateway = FakeGateway::start(vec![
        Reply::SseThenHang(vec![sse_body(&[text_delta("a", "thinking")])]),
        Reply::Sse(content_only(&["done"])),
    ]);
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command_with(&gateway));

    session.type_line("start something long");
    session.wait_for("thinking");
    session.type_bytes(&[0x03]);
    let text = session.wait_for("interrupted");
    assert!(text.contains("stopping the turn"), "{text}");

    // Still alive, and still able to run a turn.
    session.type_line("again");
    session.wait_for("done");
    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn ctrl_c_at_an_idle_prompt_clears_the_line_and_twice_leaves() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command());

    // Typed but not submitted: the line discipline discards it, and xfx offers
    // a fresh prompt rather than leaving the user staring at a dead line.
    session.type_bytes(b"half a thought");
    session.type_bytes(&[0x03]);
    session.wait_for_count(PROMPT, 2);
    session.type_bytes(&[0x03]);

    let status = session.wait_exit();
    assert_eq!(
        status.code(),
        Some(130),
        "a second interrupt exits 128+SIGINT; got {status:?}"
    );
    assert!(sandbox.session_ids().is_empty(), "nothing was recorded");
}

#[test]
fn a_submitted_line_resets_the_interrupt_count() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command());

    session.type_bytes(&[0x03]);
    session.wait_for_count(PROMPT, 2);
    session.type_line("/version");
    session.wait_for_count(PROMPT, 3);
    session.type_bytes(&[0x03]);
    session.wait_for_count(PROMPT, 4);

    // Still alive: the earlier interrupt did not count toward this one.
    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn end_of_input_leaves_the_shell_cleanly() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command());

    // Ctrl-D on an empty line is end of input.
    session.type_bytes(&[0x04]);
    assert_eq!(session.wait_exit().code(), Some(0));
}

// ---------------------------------------------------------------------------
// what the terminal looks like afterwards
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
struct TerminalState {
    input: InputModes,
    output: OutputModes,
    control: ControlModes,
    local: LocalModes,
    min: u8,
    time: u8,
    size: (u16, u16),
}

/// The terminal's state, when the kernel still allows the question to be asked.
fn try_modes(pty: &Pty) -> Option<TerminalState> {
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
fn modes(pty: &Pty) -> TerminalState {
    try_modes(pty).expect("the pty is still a terminal")
}

/// Requires that xfx left the terminal alone, given a reading taken while it
/// was still running and, where the platform still permits one, a second
/// reading taken after it exited.
///
/// **The reading that matters is the one taken while xfx is alive.** On
/// BSD-derived kernels -- macOS among them -- the terminal of a session leader
/// is revoked when that leader exits: every descriptor to it, including the one
/// this harness holds open, stops being a terminal, and a freshly opened one
/// gets a pristine device with the system defaults. So "read the terminal after
/// the child is gone" cannot distinguish a shell that restored the state from
/// one that never touched it *or from one that left it in raw mode* -- which is
/// exactly how the earlier version of these tests passed while proving nothing.
///
/// Sampling during the run is also the stronger question. xfx's claim is not
/// "it puts the terminal back", it is "it never changes the terminal", and a
/// during-run reading is the only one that can tell those two apart.
fn assert_terminal_untouched(pty: &Pty, before: TerminalState, during: TerminalState) {
    assert_eq!(
        before, during,
        "the terminal was changed while xfx was running"
    );
    if let Some(after) = try_modes(pty) {
        assert_eq!(before, after, "the terminal was left changed");
    }
    assert!(during.local.contains(LocalModes::ECHO), "echo is off");
    assert!(
        during.local.contains(LocalModes::ICANON),
        "canonical mode is off"
    );
}

#[test]
fn a_normal_exit_leaves_the_line_discipline_exactly_as_it_was() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let before = modes(&pty);
    let mut session = start(&sandbox, &pty, sandbox.command());

    session.type_line("/version");
    session.wait_for_count(PROMPT, 2);
    // Read while xfx is running and idle at its prompt. A shell that had taken
    // the terminal for its line editor is in raw mode right now.
    let during = modes(&pty);
    assert!(during.local.contains(LocalModes::ISIG), "signals are off");
    assert_eq!(session.quit().code(), Some(0));

    assert_terminal_untouched(&pty, before, during);
    assert!(
        !session.text().contains("\u{1b}[?1049"),
        "the alternate screen was used"
    );
}

#[test]
fn an_interrupted_turn_leaves_the_line_discipline_exactly_as_it_was() {
    let gateway = FakeGateway::start(vec![Reply::SseThenHang(vec![sse_body(&[text_delta(
        "a", "thinking",
    )])])]);
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let before = modes(&pty);
    let mut session = start(&sandbox, &pty, sandbox.command_with(&gateway));

    session.type_line("start something long");
    session.wait_for("thinking");
    session.type_bytes(&[0x03]);
    session.wait_for("interrupted");
    // While the shell is still alive, having just cancelled a turn: the moment
    // a restore-on-exit path would still be hiding a raw terminal.
    let during = modes(&pty);
    assert_eq!(session.quit().code(), Some(0));

    assert_terminal_untouched(&pty, before, during);
}

#[test]
fn a_hard_exit_on_a_second_interrupt_still_leaves_a_usable_terminal() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let before = modes(&pty);
    let mut session = start(&sandbox, &pty, sandbox.command());

    session.type_bytes(&[0x03]);
    session.wait_for_count(PROMPT, 2);
    // Sampled before the interrupt that ends the process: `exit(130)` runs no
    // destructor, so if xfx owed the terminal anything this is the last moment
    // it could have owed it.
    let during = modes(&pty);
    session.type_bytes(&[0x03]);
    let status = session.wait_exit();
    assert_eq!(status.code(), Some(130));
    assert_eq!(status.signal(), None, "xfx exits, it is not killed");

    assert_terminal_untouched(&pty, before, during);
}

// ---------------------------------------------------------------------------
// the shell is the same product as the command line
// ---------------------------------------------------------------------------

#[test]
fn the_shell_runs_tools_under_the_configured_permission_mode() {
    let gateway = FakeGateway::start(vec![
        Reply::Sse(sse_body(&[
            support::fake_gateway::tool_call(
                "call-1",
                "read_file",
                serde_json::json!({ "path": "notes.txt" }),
            ),
            support::fake_gateway::finish("tool-calls"),
        ])),
        Reply::Sse(content_only(&["read it"])),
    ]);
    let sandbox = Sandbox::new();
    fs::write(sandbox.workspace.join("notes.txt"), "alpha\n").expect("write the fixture");
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command_with(&gateway));

    session.type_line("read notes.txt");
    session.wait_for("read it");

    let body = request_body(&gateway, 1);
    let text = serde_json::to_string(&body).expect("serialize");
    assert!(text.contains("alpha"), "the tool result was not sent back");

    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn the_shell_records_a_session_the_command_line_can_read() {
    let gateway = FakeGateway::start(vec![Reply::Sse(content_only(&["recorded"]))]);
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command_with(&gateway));

    session.type_line("remember me");
    session.wait_for("recorded");
    assert_eq!(session.quit().code(), Some(0));

    let listed = sandbox
        .command()
        .args(["sessions", "--json"])
        .output()
        .expect("spawn xfx sessions");
    assert_eq!(listed.status.code(), Some(0));
    let document: Value =
        serde_json::from_slice(&listed.stdout).expect("the listing is one JSON document");
    let sessions = document["sessions"].as_array().expect("sessions array");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0]["history_turns"], 1);
}

#[test]
fn the_shell_reads_its_own_workspace_instructions() {
    let gateway = FakeGateway::start(vec![Reply::Sse(content_only(&["noted"]))]);
    let sandbox = Sandbox::new();
    fs::write(
        sandbox.workspace.join("AGENTS.md"),
        "Always answer in haiku.\n",
    )
    .expect("write the project instructions");
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command_with(&gateway));

    session.type_line("hello");
    session.wait_for("noted");

    let body = serde_json::to_string(&request_body(&gateway, 0)).expect("serialize");
    assert!(body.contains("Always answer in haiku."), "{body}");

    assert_eq!(session.quit().code(), Some(0));
}

/// Keeps the `Path` import honest: the fixtures above name paths, and this is
/// the one assertion about the shape of the store's own directory.
#[test]
fn the_store_lives_where_the_documentation_says_it_does() {
    let sandbox = Sandbox::new();
    assert_eq!(
        sandbox.sessions_dir(),
        Path::new(&sandbox.home).join(".xfx").join("sessions")
    );
}

/// `xfx ask` has the same interrupt, and until the shell existed nothing proved
/// it: the notice is written by a second thread while the command holds the
/// output streams, and the turn was waiting on a stream that had gone quiet.
/// Both are facts about a terminal, so both are proven on one.
#[test]
fn ctrl_c_during_a_noninteractive_ask_says_so_and_ends_the_turn() {
    let gateway = FakeGateway::start(vec![Reply::SseThenHang(vec![sse_body(&[text_delta(
        "a", "thinking",
    )])])]);
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut command = sandbox.command_with(&gateway);
    command.args(["ask", "--no-save", "something long"]);
    let mut session = Session::spawn(&pty, command);

    session.wait_for("thinking");
    session.type_bytes(&[0x03]);
    session.wait_for("interrupted");
    assert_eq!(
        session.wait_exit().code(),
        Some(1),
        "a cancelled turn is a turn that did not complete"
    );
}

#[test]
fn a_prompt_with_no_credential_says_so_and_records_nothing() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    // No `AI_GATEWAY_API_KEY`, and no Gateway either: a shell must still open
    // on a machine that cannot yet talk to a model -- that is the machine whose
    // user needs `/help` -- and a prompt it cannot send must not leave an empty
    // session behind, once per attempt.
    let mut session = start(&sandbox, &pty, sandbox.command());

    session.type_line("anything at all");
    let text = session.wait_for("AI_GATEWAY_API_KEY");
    assert!(text.contains("VERCEL_OIDC_TOKEN"), "{text}");

    session.type_line("and again");
    session.wait_for_count("AI_GATEWAY_API_KEY", 2);
    assert!(
        sandbox.session_ids().is_empty(),
        "an empty session was left"
    );

    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn a_model_chosen_in_the_shell_is_what_the_session_records() {
    let gateway = FakeGateway::start(vec![Reply::Sse(content_only(&["ok"]))]);
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command_with(&gateway));

    session.type_line("/model acme/model-9");
    session.wait_for("model=acme/model-9");
    session.type_line("hello");
    session.wait_for("ok");
    assert_eq!(session.quit().code(), Some(0));

    // The durable record agrees with what the terminal was told, so a later
    // `xfx ask --resume` continues in the model the conversation was held in.
    let shown = sandbox
        .command()
        .args(["session", "last", "--json"])
        .output()
        .expect("spawn xfx session");
    let document: Value = serde_json::from_slice(&shown.stdout).expect("one JSON document");
    assert_eq!(document["model"], "acme/model-9");
}

// ---------------------------------------------------------------------------
// the permission modes, on a terminal
// ---------------------------------------------------------------------------

/// A Gateway that reads `notes.txt`, edits it, and then reports what happened.
///
/// The read is not decoration. An edit may only replace a file the turn has
/// already read in full, in *every* mode -- that is a validation rule about
/// knowing what you are overwriting, not a permission rule, so `yolo` does not
/// skip it either. A script that jumped straight to the edit would be testing
/// that rule rather than the permission modes.
fn edit_then_finish() -> Vec<Reply> {
    vec![
        Reply::Sse(sse_body(&[
            support::fake_gateway::tool_call(
                "call-0",
                "read_file",
                serde_json::json!({ "path": "notes.txt" }),
            ),
            support::fake_gateway::finish("tool-calls"),
        ])),
        Reply::Sse(sse_body(&[
            support::fake_gateway::tool_call(
                "call-1",
                "edit_file",
                serde_json::json!({
                    "path": "notes.txt",
                    "old_string": "alpha",
                    "new_string": "beta",
                }),
            ),
            support::fake_gateway::finish("tool-calls"),
        ])),
        Reply::Sse(content_only(&["the edit is done"])),
    ]
}

/// A workspace with one file the model is scripted to edit.
fn with_notes(sandbox: &Sandbox) -> PathBuf {
    let path = sandbox.workspace.join("notes.txt");
    fs::write(&path, "alpha\n").expect("write the fixture");
    path
}

#[test]
fn ask_mode_asks_on_the_terminal_and_a_yes_lets_the_edit_through() {
    let gateway = FakeGateway::start(edit_then_finish());
    let sandbox = Sandbox::new();
    let notes = with_notes(&sandbox);
    let pty = Pty::open();
    let mut command = sandbox.command_with(&gateway);
    command.env("XFX_PERMISSION_MODE", "ask");
    let mut session = start(&sandbox, &pty, command);

    session.type_line("fix the notes");
    // The prompt is the real one: it says what xfx wants, what "always" would
    // grant, and it is asked on this terminal rather than assumed.
    let asked = session.wait_for("xfx wants to");
    assert!(asked.contains("[y] yes, once"), "{asked}");
    assert!(asked.contains("[a] always"), "{asked}");
    assert!(asked.contains("notes.txt"), "{asked}");
    assert_eq!(
        fs::read_to_string(&notes).expect("read the file"),
        "alpha\n",
        "the edit ran before it was approved"
    );

    session.type_line("y");
    session.wait_for("the edit is done");
    assert_eq!(
        fs::read_to_string(&notes).expect("read the file"),
        "beta\n",
        "an approved edit did not land"
    );

    // The answer was consumed by the approval and nothing else: the next line
    // is read by the shell as a fresh line, not as leftover bytes. Two readers
    // share one buffered stdin, and this is the assertion that they do not
    // swallow each other's input.
    session.type_line("/version");
    session.wait_for(env!("CARGO_PKG_VERSION"));
    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn ask_mode_takes_no_for_an_answer_and_the_file_is_untouched() {
    let gateway = FakeGateway::start(edit_then_finish());
    let sandbox = Sandbox::new();
    let notes = with_notes(&sandbox);
    let pty = Pty::open();
    let mut command = sandbox.command_with(&gateway);
    command.env("XFX_PERMISSION_MODE", "ask");
    let mut session = start(&sandbox, &pty, command);

    session.type_line("fix the notes");
    session.wait_for("xfx wants to");
    session.type_line("n");
    // The refusal is a tool result the model can act on, so the turn continues
    // and finishes normally.
    session.wait_for("the edit is done");
    assert_eq!(
        fs::read_to_string(&notes).expect("read the file"),
        "alpha\n",
        "a refused edit changed the file"
    );

    session.type_line("/version");
    session.wait_for(env!("CARGO_PKG_VERSION"));
    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn ctrl_c_at_an_approval_prompt_does_not_hang_the_shell() {
    let gateway = FakeGateway::start(edit_then_finish());
    let sandbox = Sandbox::new();
    let notes = with_notes(&sandbox);
    let pty = Pty::open();
    let mut command = sandbox.command_with(&gateway);
    command.env("XFX_PERMISSION_MODE", "ask");
    let mut session = start(&sandbox, &pty, command);

    session.type_line("fix the notes");
    session.wait_for("xfx wants to");
    session.type_bytes(&[0x03]);
    session.wait_for("interrupted");

    // The question is still on the terminal and still needs an answer -- the
    // interrupt stopped the turn, it did not answer for the user. What must not
    // happen is a shell that never comes back.
    session.type_line("n");
    session.wait_for_count(PROMPT, 2);
    // The file is the assertion, not the transcript: the question itself quotes
    // an excerpt of the change it is asking about, so the new text is on the
    // terminal by design and means nothing about what happened on disk.
    assert_eq!(
        fs::read_to_string(&notes).expect("read the file"),
        "alpha\n",
        "a cancelled and refused edit changed the file"
    );

    session.type_line("/version");
    session.wait_for(env!("CARGO_PKG_VERSION"));
    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn yolo_mode_warns_once_and_then_asks_nobody_anything() {
    let gateway = FakeGateway::start(edit_then_finish());
    let sandbox = Sandbox::new();
    let notes = with_notes(&sandbox);
    let pty = Pty::open();
    let mut command = sandbox.command_with(&gateway);
    command.env("XFX_PERMISSION_MODE", "yolo");
    let mut session = Session::spawn(&pty, command);

    // The warning is on the way in, before a prompt exists to type at.
    let opening = session.wait_for(PROMPT);
    assert!(
        opening.to_lowercase().contains("yolo"),
        "yolo must announce itself: {opening}"
    );
    assert!(opening.contains("permission_mode=yolo"), "{opening}");

    session.type_line("fix the notes");
    session.wait_for("the edit is done");
    let text = session.text();
    assert!(
        !text.contains("xfx wants to"),
        "yolo asked a question: {text}"
    );
    assert_eq!(
        fs::read_to_string(&notes).expect("read the file"),
        "beta\n",
        "yolo did not run the edit"
    );

    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn status_reports_the_mode_the_shell_is_running_under() {
    // The same fact from the other side of the product: what the shell warned
    // about is what `status` says, so the two cannot disagree.
    let sandbox = Sandbox::new();
    let listed = sandbox
        .command()
        .args(["status", "--json"])
        .env("XFX_PERMISSION_MODE", "yolo")
        .output()
        .expect("spawn xfx status");
    let document: Value = serde_json::from_slice(&listed.stdout).expect("one JSON document");
    assert_eq!(document["permission_mode"], "yolo");
    assert_eq!(document["sandbox"], "none");
}

#[test]
fn the_terminal_comparator_notices_a_change_in_any_single_field() {
    // The regression this pins: the comparator used to fold the input and
    // output flag words together with a bitwise OR before comparing. The two
    // words have overlapping bit values, so a flag gained in one and lost in
    // the other cancelled out and a changed terminal compared equal. Each field
    // now stands on its own, and this proves each one is actually looked at.
    let pty = Pty::open();
    let base = modes(&pty);

    let variants = [
        (
            "input",
            TerminalState {
                input: base.input ^ InputModes::ICRNL,
                ..base
            },
        ),
        (
            "output",
            TerminalState {
                output: base.output ^ OutputModes::OPOST,
                ..base
            },
        ),
        (
            "control",
            TerminalState {
                control: base.control ^ ControlModes::CREAD,
                ..base
            },
        ),
        (
            "local",
            TerminalState {
                local: base.local ^ LocalModes::ECHO,
                ..base
            },
        ),
        (
            "VMIN",
            TerminalState {
                min: base.min.wrapping_add(1),
                ..base
            },
        ),
        (
            "VTIME",
            TerminalState {
                time: base.time.wrapping_add(1),
                ..base
            },
        ),
        (
            "size",
            TerminalState {
                size: (base.size.0.wrapping_add(1), base.size.1),
                ..base
            },
        ),
    ];
    for (field, changed) in variants {
        assert_ne!(base, changed, "a change to {field} compares equal");
    }
    assert_eq!(base, modes(&pty), "reading twice must be stable");
}

// ---------------------------------------------------------------------------
// the harness, tested against itself
// ---------------------------------------------------------------------------

/// A child that puts the terminal in raw mode with echo off and leaves it that
/// way, which is the exact thing every restoration test above claims xfx never
/// does.
///
/// `take_terminal` chooses whether it does so as a session leader owning the
/// terminal, the way xfx runs, or as an ordinary process. The two are not
/// interchangeable: a session leader's terminal is revoked when it exits on
/// BSD-derived kernels, and that revocation is the whole reason the old harness
/// saw nothing.
fn leave_the_terminal_in_raw_mode(pty: &Pty, take_terminal: bool) -> Session {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "stty raw -echo"]);
    let mut session = if take_terminal {
        Session::spawn(pty, command)
    } else {
        Session::spawn_without_taking_the_terminal(pty, command)
    };
    assert_eq!(
        session.wait_exit().code(),
        Some(0),
        "stty failed; terminal so far:\n{}",
        session.text()
    );
    session
}

#[test]
fn the_harness_can_tell_when_a_child_leaves_the_terminal_changed() {
    // Without this the whole "the terminal was left exactly as it was found"
    // section is a green light with nothing behind it. Here a child really does
    // wreck the terminal, and the comparator has to say so.
    let pty = Pty::open();
    let before = modes(&pty);
    // Not a session leader, so the terminal survives its exit to be inspected.
    // The session-leader case is covered by the during-run reading below, which
    // is the one the restoration tests actually take.
    let _child = leave_the_terminal_in_raw_mode(&pty, false);

    let after = modes(&pty);
    assert_ne!(before, after, "a raw-mode child compared equal");
    assert!(!after.local.contains(LocalModes::ECHO), "echo survived raw");
    assert!(
        !after.local.contains(LocalModes::ICANON),
        "canonical mode survived raw"
    );
    assert!(before.local.contains(LocalModes::ECHO), "{before:?}");
}

/// The old blind spot, reproduced -- on macOS only, because only a BSD-derived
/// kernel can be made to show it.
///
/// The child is spawned exactly the way xfx is: its own session, owning the
/// terminal. When such a child exits, a BSD-derived kernel revokes its
/// terminal, and the next open of that device name is a pristine one carrying
/// the system defaults. A harness that opens a slave per reading therefore
/// reports "the terminal is exactly as it was found" about the very raw-mode
/// child `the_harness_can_tell_when_a_child_leaves_the_terminal_changed`
/// catches.
///
/// **This fixture is not portable, and its `cfg` is the fix rather than a
/// concession.** Linux does not revoke an exiting session leader's terminal, so
/// a freshly opened slave there still reports the raw mode the child left:
/// `before != after`, and asserting blindness fails on a kernel that is not
/// blind. That is exactly what both Linux jobs of run 32505490051 reported
/// (`assertion left == right failed` at this test, `ECHO | ICANON | ISIG`
/// present before and absent after). macOS is the only BSD-derived target xfx
/// ships, so `target_os = "macos"` is the whole of the supported blind
/// platform.
///
/// Nothing about the harness's guarantees is scoped by this. What proves the
/// harness can see a change is
/// `the_harness_can_tell_when_a_child_leaves_the_terminal_changed` and
/// `a_during_run_reading_sees_a_terminal_the_running_child_has_changed`, and
/// both of those, like every restoration test above them, run on every
/// platform.
#[cfg(target_os = "macos")]
#[test]
fn on_macos_a_harness_that_retains_nothing_cannot_see_the_change() {
    let pty = Pty::open_without_a_retained_slave();
    let before = modes(&pty);
    let _child = leave_the_terminal_in_raw_mode(&pty, true);

    let after = modes(&pty);
    assert_eq!(
        before, after,
        "macOS reported an exiting session leader's terminal as the child left \
         it, so it no longer revokes and this macOS-only fixture has nothing \
         left to document; delete it rather than repair it"
    );
    assert!(
        after.local.contains(LocalModes::ECHO),
        "a fresh descriptor did not report defaults, so the retained one is not \
         what made the difference"
    );
}

#[test]
fn a_during_run_reading_sees_a_terminal_the_running_child_has_changed() {
    // The restoration tests above take their reading while xfx is alive, and
    // this is the proof that such a reading can fail. The child is spawned
    // exactly as xfx is -- its own session, holding this terminal -- so the
    // path under test is the same one, down to the descriptor.
    let pty = Pty::open();
    let before = modes(&pty);
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "stty raw -echo; echo ready; sleep 30"]);
    let session = Session::spawn(&pty, command);
    session.wait_for("ready");

    let during = modes(&pty);
    assert_ne!(before, during, "a live raw-mode child compared equal");
    assert!(!during.local.contains(LocalModes::ECHO), "{during:?}");
    assert!(!during.local.contains(LocalModes::ICANON), "{during:?}");
    // Dropping the session kills the child, which is the point: nothing here
    // waits for a `sleep 30`.
}
