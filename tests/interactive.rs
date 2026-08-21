//! Acceptance tests for the interactive shell, driven through a real
//! pseudoterminal.
//!
//! The shell is the one fxr surface whose contract is *about* the terminal: it
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
use std::io::{Read, Write};
use std::os::fd::{AsFd, BorrowedFd};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use rustix::pty::{grantpt, openpt, ptsname, unlockpt, OpenptFlags};
use rustix::termios::{tcgetattr, LocalModes, Termios};
use serde_json::Value;
use tempfile::TempDir;

use support::fake_gateway::{content_only, sse_body, text_delta, FakeGateway, Reply};

/// Environment variables that must never leak in from the developer's shell.
const CONTROLLED_VARS: &[&str] = &[
    "VERCEL_OIDC_TOKEN",
    "AI_GATEWAY_API_KEY",
    "FXR_MODEL",
    "FXR_PERMISSION_MODE",
    "FXR_MAX_AGENT_STEPS",
    "FXR_GATEWAY_URL",
];

/// A test secret that must never appear on the terminal.
const TEST_KEY: &str = "fxr-test-interactive-key-must-not-appear";

/// How long a test waits for expected output before failing.
const WAIT: Duration = Duration::from_secs(20);

/// The prompt the shell writes before reading a line.
const PROMPT: &str = "> ";

// ---------------------------------------------------------------------------
// the pseudoterminal harness
// ---------------------------------------------------------------------------

/// A pty pair: the side a test writes to and reads from, and the device name
/// the child opens as its stdin, stdout, and stderr.
struct Pty {
    master: Arc<File>,
    slave_path: PathBuf,
}

impl Pty {
    fn open() -> Self {
        let master = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY).expect("open a pty master");
        grantpt(&master).expect("grant the pty slave");
        unlockpt(&master).expect("unlock the pty slave");
        let name: CString = ptsname(&master, Vec::new()).expect("name the pty slave");
        let slave_path = PathBuf::from(name.to_str().expect("the slave name is utf-8").to_string());
        Self {
            master: Arc::new(File::from(master)),
            slave_path,
        }
    }

    /// The line-discipline state of the terminal, as a caller would see it.
    ///
    /// Read through a freshly opened slave rather than through the master:
    /// BSD-derived kernels, macOS among them, answer `tcgetattr` on a pty
    /// master with `ENOTTY`. The handle is dropped immediately so that it can
    /// never be the descriptor keeping the terminal alive after the child is
    /// gone.
    fn termios(&self) -> Termios {
        let slave = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&self.slave_path)
            .expect("open the pty slave");
        tcgetattr(slave.as_fd()).expect("read the terminal state")
    }
}

/// The real `fxr` binary running on a pty, with everything it wrote captured.
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
    fn spawn(pty: &Pty, mut command: Command) -> Self {
        let slave = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&pty.slave_path)
            .expect("open the pty slave");
        let stdin = slave.try_clone().expect("clone the pty slave");
        let stdout = slave.try_clone().expect("clone the pty slave");
        command
            .stdin(Stdio::from(stdin))
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(slave));
        // SAFETY: both calls are async-signal-safe and touch only this child's
        // own session and controlling terminal. `std` has already dup'd the pty
        // onto 0/1/2 by the time a `pre_exec` closure runs, so fd 0 is the
        // terminal being claimed.
        unsafe {
            command.pre_exec(|| {
                rustix::process::setsid()?;
                rustix::process::ioctl_tiocsctty(BorrowedFd::borrow_raw(0))?;
                Ok(())
            });
        }
        let child = command.spawn().expect("spawn fxr on the pty");

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
                    // A closed slave reads as EOF on some platforms and as EIO
                    // on others; both mean the child is gone.
                    Ok(0) | Err(_) => break,
                    Ok(read) => thread_output
                        .lock()
                        .expect("output lock")
                        .extend_from_slice(&buffer[..read]),
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
    fn type_bytes(&self, bytes: &[u8]) {
        (&*self.master)
            .write_all(bytes)
            .expect("write to the terminal");
        (&*self.master).flush().expect("flush the terminal");
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
                        "fxr did not exit; terminal so far:\n{}",
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
        let mut command = Command::new(env!("CARGO_BIN_EXE_fxr"));
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
        command.env("FXR_GATEWAY_URL", gateway.chat_url());
        command
    }

    fn sessions_dir(&self) -> PathBuf {
        self.home.join(".fxr").join("sessions")
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
        .expect("spawn fxr");

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty(), "stdout must stay empty");
    let stderr = String::from_utf8(output.stderr).expect("stderr is utf-8");
    assert!(
        stderr.contains("interactive terminal") && stderr.contains("fxr ask"),
        "the refusal must name the requirement and the alternative: {stderr}"
    );
    // Refusing is not the same as failing halfway through starting: nothing was
    // created under the profile home.
    assert!(!sandbox.home.join(".fxr").exists(), "the home was touched");
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
        .expect("spawn fxr");

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
    assert!(!text.contains("fxr:"), "nothing was attempted: {text}");

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

#[test]
fn an_unknown_slash_command_is_refused_with_the_same_words_every_time() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command());

    session.type_line("/nonesuch");
    let text = session.wait_for("/nonesuch is not");
    session.type_line("/nonesuch");
    let text_again = session.wait_for_count("/nonesuch is not", 2);

    let refusals: Vec<&str> = text_again
        .match_indices("/nonesuch is not")
        .map(|(index, _)| {
            let rest = &text_again[index..];
            rest.split('\n').next().unwrap_or(rest)
        })
        .collect();
    assert_eq!(refusals.len(), 2);
    assert_eq!(refusals[0], refusals[1], "the refusal is not deterministic");
    assert!(text.contains("/help"), "the refusal must point somewhere");

    // A slash command that is not one is never sent to a model: there is no
    // Gateway configured here, and no connection failure was reported.
    assert!(!text_again.contains("connect"), "{text_again}");
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

    // Typed but not submitted: the line discipline discards it, and fxr offers
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

/// The line-discipline facts a shell must not silently change.
fn modes(state: &Termios) -> (u32, LocalModes) {
    (
        state.input_modes.bits() as u32 | state.output_modes.bits() as u32,
        state.local_modes,
    )
}

#[test]
fn a_normal_exit_leaves_the_line_discipline_exactly_as_it_was() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let before = pty.termios();
    let mut session = start(&sandbox, &pty, sandbox.command());

    session.type_line("/version");
    session.wait_for_count(PROMPT, 2);
    assert_eq!(session.quit().code(), Some(0));

    let after = pty.termios();
    assert_eq!(
        modes(&before),
        modes(&after),
        "the terminal was left changed"
    );
    assert!(after.local_modes.contains(LocalModes::ECHO), "echo is off");
    assert!(
        after.local_modes.contains(LocalModes::ICANON),
        "canonical mode is off"
    );
    assert!(
        after.local_modes.contains(LocalModes::ISIG),
        "signals are off"
    );
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
    let before = pty.termios();
    let mut session = start(&sandbox, &pty, sandbox.command_with(&gateway));

    session.type_line("start something long");
    session.wait_for("thinking");
    session.type_bytes(&[0x03]);
    session.wait_for("interrupted");
    assert_eq!(session.quit().code(), Some(0));

    let after = pty.termios();
    assert_eq!(
        modes(&before),
        modes(&after),
        "the terminal was left changed"
    );
    assert!(after.local_modes.contains(LocalModes::ECHO), "echo is off");
    assert!(
        after.local_modes.contains(LocalModes::ICANON),
        "canonical mode is off"
    );
}

#[test]
fn a_hard_exit_on_a_second_interrupt_still_leaves_a_usable_terminal() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let before = pty.termios();
    let mut session = start(&sandbox, &pty, sandbox.command());

    session.type_bytes(&[0x03]);
    session.wait_for_count(PROMPT, 2);
    session.type_bytes(&[0x03]);
    let status = session.wait_exit();
    assert_eq!(status.code(), Some(130));
    assert_eq!(status.signal(), None, "fxr exits, it is not killed");

    let after = pty.termios();
    assert_eq!(
        modes(&before),
        modes(&after),
        "the terminal was left changed"
    );
    assert!(after.local_modes.contains(LocalModes::ECHO), "echo is off");
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
        .expect("spawn fxr sessions");
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
        Path::new(&sandbox.home).join(".fxr").join("sessions")
    );
}

/// `fxr ask` has the same interrupt, and until the shell existed nothing proved
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
    // `fxr ask --resume` continues in the model the conversation was held in.
    let shown = sandbox
        .command()
        .args(["session", "last", "--json"])
        .output()
        .expect("spawn fxr session");
    let document: Value = serde_json::from_slice(&shown.stdout).expect("one JSON document");
    assert_eq!(document["model"], "acme/model-9");
}
