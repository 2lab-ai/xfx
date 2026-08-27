//! The sandboxed home, workspace, and environment every acceptance test runs
//! the real binary in.
//!
//! A test that inherited the developer's `HOME` would read the developer's
//! profile and write into the developer's session store, so a `Sandbox` is a
//! temporary root with a home and a workspace under it and an environment with
//! every variable xfx reads stripped out.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use tempfile::TempDir;

use super::fake_gateway::{self, FakeGateway, Reply};
use super::fake_llmux::FakeLlmux;

/// Environment variables that must never leak in from the developer's shell.
pub const CONTROLLED_VARS: &[&str] = &[
    "VERCEL_OIDC_TOKEN",
    "AI_GATEWAY_API_KEY",
    "XFX_MODEL",
    "XFX_PERMISSION_MODE",
    "XFX_MAX_AGENT_STEPS",
    "XFX_GATEWAY_URL",
];

/// A test secret that must never appear on the terminal.
pub const TEST_KEY: &str = "xfx-test-interactive-key-must-not-appear";

pub struct Sandbox {
    _root: TempDir,
    pub home: PathBuf,
    pub workspace: PathBuf,
}

impl Sandbox {
    pub fn new() -> Self {
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

    pub fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_xfx"));
        self.apply_env(&mut command);
        command
    }

    /// The same controlled environment on a command this sandbox did not build.
    ///
    /// Extracted rather than duplicated: a test that reaches xfx through a
    /// shell builds its own `Command`, and the two must be the same environment
    /// or the shelled case is running somewhere else than every other case.
    pub fn apply_env(&self, command: &mut Command) {
        command.current_dir(&self.workspace);
        command.env("HOME", &self.home);
        // A shell is a terminal program; a terminal that claims to be nothing
        // is still a terminal, and the shell must not depend on this.
        command.env("TERM", "dumb");
        for key in CONTROLLED_VARS {
            command.env_remove(key);
        }
    }

    /// A command wired to a scripted local Gateway.
    pub fn command_with(&self, gateway: &FakeGateway) -> Command {
        let mut command = self.command();
        command.env("AI_GATEWAY_API_KEY", TEST_KEY);
        command.env("XFX_GATEWAY_URL", gateway.chat_url());
        command
    }

    /// A command whose profile points at a scripted local llmux daemon.
    ///
    /// Written into the profile rather than passed as a flag, because the
    /// provider is a property of the machine and that is where a real one is
    /// configured -- and because writing it is what proves the shell reads it.
    pub fn command_with_llmux(&self, daemon: &FakeLlmux) -> Command {
        let dir = self.home.join(".xfx");
        fs::create_dir_all(&dir).expect("create the profile dir");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))
                .expect("tighten the profile dir");
        }
        fs::write(
            dir.join("settings.json"),
            format!(
                "{{\"provider\":\"llmux\",\"llmux_url\":{},\"models\":{{\"llmux\":\"m-1\"}}}}",
                serde_json::to_string(&daemon.url()).expect("a url is serializable")
            ),
        )
        .expect("write the profile");
        self.command()
    }

    /// A command wired to a scripted Gateway **and** a scripted llmux at once,
    /// with the Gateway selected.
    ///
    /// Both fixtures are up and only one of them is configured, which is the
    /// whole point: a provider switch is only proven by a request reaching the
    /// daemon that was switched **to** and none reaching the one that was
    /// switched away from. With a single fixture running, "the prompt went
    /// somewhere else" and "the prompt went nowhere" look the same.
    ///
    /// The llmux url is written into the profile the way `xfx setup llmux`
    /// writes it, so `/setup llmux` finds this daemon rather than discovering
    /// whatever is on the developer's own loopback port -- which on a real
    /// machine is a live daemon and would make the test's answer depend on
    /// whether it happened to be running.
    pub fn command_with_both(&self, gateway: &FakeGateway, daemon: &FakeLlmux) -> Command {
        let dir = self.home.join(".xfx");
        fs::create_dir_all(&dir).expect("create the profile dir");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))
                .expect("tighten the profile dir");
        }
        fs::write(
            dir.join("settings.json"),
            format!(
                "{{\"provider\":\"gateway\",\"llmux_url\":{},\"models\":{{\"gateway\":\"zai/glm-5.2\"}}}}",
                serde_json::to_string(&daemon.url()).expect("a url is serializable")
            ),
        )
        .expect("write the profile");
        let mut command = self.command();
        command.env("AI_GATEWAY_API_KEY", TEST_KEY);
        command.env("XFX_GATEWAY_URL", gateway.chat_url());
        command
    }

    /// The settings file this sandbox's profile lives in.
    pub fn settings_path(&self) -> PathBuf {
        self.home.join(".xfx").join("settings.json")
    }

    /// The profile as JSON, or `None` when there is not one.
    ///
    /// Read rather than believed: a test that asserted a provider switch from
    /// the terminal alone would be asserting what the band said, and what the
    /// band says is exactly what a switch that wrote nothing would also say.
    pub fn settings(&self) -> Option<serde_json::Value> {
        let text = fs::read_to_string(self.settings_path()).ok()?;
        serde_json::from_str(&text).ok()
    }

    pub fn sessions_dir(&self) -> PathBuf {
        self.home.join(".xfx").join("sessions")
    }

    /// Every session directory currently in the store, sorted.
    pub fn session_ids(&self) -> Vec<String> {
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

// ---------------------------------------------------------------------------
// the scripted mutation both terminal suites drive the permission modes with
// ---------------------------------------------------------------------------

/// A Gateway that reads `notes.txt`, edits it, and then reports what happened.
///
/// The read is not decoration. An edit may only replace a file the turn has
/// already read in full, in *every* mode -- that is a validation rule about
/// knowing what you are overwriting, not a permission rule, so `yolo` does not
/// skip it either. A script that jumped straight to the edit would be testing
/// that rule rather than the permission modes.
///
/// Here rather than in one suite because both front ends have to be asked the
/// same question: `tests/interactive.rs` answers it on the terminal and
/// `tests/tui.rs` answers it in the band, and two copies of the script would be
/// two chances for the two surfaces to be tested against different mutations.
pub fn edit_then_finish() -> Vec<Reply> {
    vec![
        Reply::Sse(fake_gateway::sse_body(&[
            fake_gateway::tool_call(
                "call-0",
                "read_file",
                serde_json::json!({ "path": "notes.txt" }),
            ),
            fake_gateway::finish("tool-calls"),
        ])),
        Reply::Sse(fake_gateway::sse_body(&[
            fake_gateway::tool_call(
                "call-1",
                "edit_file",
                serde_json::json!({
                    "path": "notes.txt",
                    "old_string": "alpha",
                    "new_string": "beta",
                }),
            ),
            fake_gateway::finish("tool-calls"),
        ])),
        Reply::Sse(fake_gateway::content_only(&["the edit is done"])),
    ]
}

/// A workspace with one file the model is scripted to edit.
pub fn with_notes(sandbox: &Sandbox) -> PathBuf {
    let path = sandbox.workspace.join("notes.txt");
    fs::write(&path, "alpha\n").expect("write the fixture");
    path
}
