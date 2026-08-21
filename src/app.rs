//! Application composition and command dispatch.
//!
//! [`run`] is the one place that turns a parsed invocation into bytes on a
//! stream and an exit code. Every later slice adds a match arm here rather than
//! a second entry point, so there is exactly one path from argument to effect.

use std::fmt;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use crate::agent::{run_turn, TurnRequest};
use crate::cli::{Cli, Command};
use crate::config::{ConfigError, PermissionMode, RuntimeConfig};
use crate::gateway::{CancelToken, Endpoint, GatewayProvider, DEFAULT_MAX_ATTEMPTS};
use crate::output::{
    CheckStatus, DoctorCheck, DoctorSnapshot, Event, EventSink, JsonlSink, OutputFormat,
    StatusSnapshot, TextSink, MISSING_AUTH_HELP,
};
use crate::permission::{PermissionSession, TtyPrompter, YOLO_WARNING};
use crate::tools::ToolContext;
use crate::workspace::AccessScope;

/// The exit code for a rejected invocation.
///
/// Upstream exits 1 rather than the shell's conventional 2 for a usage error
/// (`vercel-labs/fx@580a0c5d tests/e2e/cli.test.ts:404-412`), and scripts around
/// fxr should not have to learn a second convention.
const REJECTED_EXIT_CODE: u8 = 1;

/// The exit code for a turn that did not complete.
///
/// The same value as a rejection: from a script's point of view both mean "fxr
/// did not do what you asked", and the `error` event carries the difference.
const TURN_FAILURE_EXIT_CODE: u8 = 1;

/// A failure that stops a command before it can report anything.
///
/// Malformed settings are deliberately not in here: they are facts `status` and
/// `doctor` must be able to describe, not reasons to refuse to run.
#[derive(Debug)]
pub enum AppError {
    /// The workspace could not be resolved.
    Config(ConfigError),
    /// Output could not be written.
    Io(io::Error),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(err) => write!(f, "{err}"),
            Self::Io(err) => write!(f, "cannot write output: {err}"),
        }
    }
}

impl std::error::Error for AppError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Config(err) => Some(err),
            Self::Io(err) => Some(err),
        }
    }
}

impl From<ConfigError> for AppError {
    fn from(err: ConfigError) -> Self {
        Self::Config(err)
    }
}

impl From<io::Error> for AppError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

/// Runs one invocation against the real process streams.
pub async fn run(cli: Cli) -> Result<ExitCode, AppError> {
    let stdout = io::stdout();
    let stderr = io::stderr();
    run_with(cli, &mut stdout.lock(), &mut stderr.lock()).await
}

/// Runs one invocation against explicit streams.
///
/// Splitting this out keeps the dispatch table testable without a subprocess,
/// and makes the stdout/stderr split an argument rather than an assumption.
pub async fn run_with(
    cli: Cli,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<ExitCode, AppError> {
    match cli.command {
        Command::Help { page } => {
            write!(stdout, "{page}")?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Version => {
            // Upstream prints the bare version with no program name
            // (`tests/e2e/cli.test.ts:445`).
            writeln!(stdout, "{}", crate::VERSION)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Ask {
            prompt,
            json,
            // Nothing is persisted in this release, so the flag's promise --
            // "this turn is not recorded" -- already holds. The session store
            // that would make the default meaningful is a later slice
            // (`docs/parity.md`, session event log).
            no_save: _,
            add_dirs,
            mode,
        } => {
            let config = load_config()?;
            // A flag overrides the configured mode for this invocation only.
            // Nothing is written back: `--yolo` must not be something a user can
            // turn on once and then forget is on.
            let mode = mode.unwrap_or(config.permission_mode);
            ask(&config, mode, prompt, add_dirs, json, stdout, stderr).await
        }
        Command::Status { json } => {
            let config = load_config()?;
            let snapshot = StatusSnapshot::new(&config, crate::build_info());
            write!(
                stdout,
                "{}",
                snapshot.render(OutputFormat::from_json_flag(json))
            )?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Doctor { json } => {
            let config = load_config()?;
            let snapshot = DoctorSnapshot::new(&config, doctor_checks(&config));
            write!(
                stdout,
                "{}",
                snapshot.render(OutputFormat::from_json_flag(json))
            )?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Rejected { message } => {
            // Diagnostics go to stderr so a rejected `--json` invocation never
            // puts non-JSON bytes on a pipe a caller is parsing.
            write!(stderr, "{message}")?;
            if !message.ends_with('\n') {
                writeln!(stderr)?;
            }
            Ok(ExitCode::from(REJECTED_EXIT_CODE))
        }
    }
}

/// Runs one streamed model turn.
///
/// Every failure is reported through the same event sink as a success, so a
/// `--json` caller gets exactly one terminal JSONL event whatever happened, and
/// a human gets the answer on stdout and the diagnosis on stderr.
#[allow(clippy::too_many_arguments)]
async fn ask(
    config: &RuntimeConfig,
    mode: PermissionMode,
    prompt: String,
    add_dirs: Vec<PathBuf>,
    json: bool,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<ExitCode, AppError> {
    // Before anything runs, and on stderr, so it is visible whether or not the
    // caller is parsing stdout as JSONL.
    if mode == PermissionMode::Yolo {
        writeln!(stderr, "{YOLO_WARNING}")?;
    }

    let mut sink: Box<dyn EventSink + '_> = if json {
        Box::new(JsonlSink::new(stdout))
    } else {
        Box::new(TextSink::new(stdout, stderr))
    };

    // The authority the turn's tools will run under, resolved before anything
    // else. A directory the user named but fxr cannot use is a mistake in the
    // invocation, and reporting it here costs no credential and no round trip.
    let scope = match AccessScope::new(&config.workspace_root, &add_dirs) {
        Ok(scope) => scope,
        Err(err) => return fail_turn(sink.as_mut(), err.to_string()),
    };

    // Both preconditions are checked before a request is built, so a missing
    // credential and an unusable endpoint each cost nothing and leak nothing.
    let Some(credential) = config.credential.clone() else {
        return fail_turn(sink.as_mut(), MISSING_AUTH_HELP.to_string());
    };
    let endpoint = match Endpoint::from_process() {
        Ok(endpoint) => endpoint,
        Err(err) => return fail_turn(sink.as_mut(), err.to_string()),
    };
    let cancel = CancelToken::new();
    let provider = match GatewayProvider::new(endpoint, credential, cancel.clone()) {
        Ok(provider) => provider,
        Err(err) => return fail_turn(sink.as_mut(), err.to_string()),
    };

    let request = TurnRequest {
        model: config.model.clone(),
        prompt,
        history: Vec::new(),
        max_steps: config.max_agent_steps,
        max_attempts: DEFAULT_MAX_ATTEMPTS,
        cancel: cancel.clone(),
        tools: ToolContext::new(scope)
            .with_permissions(permission_session(mode))
            .with_cancel(cancel),
    };
    // The turn writes its own terminal event, including on failure.
    match run_turn(request, &provider, sink.as_mut()).await {
        Ok(_) => Ok(ExitCode::SUCCESS),
        Err(_) => Ok(ExitCode::from(TURN_FAILURE_EXIT_CODE)),
    }
}

/// Builds the permission session one `ask` runs under.
///
/// The approval channel is attached only when there is a real terminal on both
/// ends. Without one, `ask` mode denies every mutation rather than hanging on a
/// question nobody can see -- so a piped or scripted `fxr ask` fails closed by
/// construction rather than by remembering to check.
fn permission_session(mode: PermissionMode) -> PermissionSession {
    let session = PermissionSession::new(mode);
    match TtyPrompter::available() {
        Some(prompter) => session.with_prompter(Box::new(prompter)),
        None => session,
    }
}

/// Reports a turn that could not start, in the same shape as one that failed.
fn fail_turn(sink: &mut dyn EventSink, message: String) -> Result<ExitCode, AppError> {
    sink.emit(&Event::Error { message })?;
    Ok(ExitCode::from(TURN_FAILURE_EXIT_CODE))
}

/// Resolves configuration for the current directory.
fn load_config() -> Result<RuntimeConfig, AppError> {
    let workspace = std::env::current_dir().map_err(ConfigError::Workspace)?;
    Ok(RuntimeConfig::load(&workspace)?)
}

/// Builds the diagnostic checks in a fixed order: what fxr is looking at, what
/// it read, whether it can authenticate, and what it resolved.
fn doctor_checks(config: &RuntimeConfig) -> Vec<DoctorCheck> {
    let mut checks = vec![DoctorCheck::new(
        "workspace",
        CheckStatus::Ok,
        format!("using workspace {}", config.workspace_root.display()),
    )];

    checks.push(config_presence_check(config));
    for diagnostic in &config.diagnostics {
        checks.push(DoctorCheck::new(
            "config",
            CheckStatus::Warn,
            diagnostic.detail(),
        ));
    }

    checks.push(match &config.credential {
        Some(credential) => DoctorCheck::new(
            "auth",
            CheckStatus::Ok,
            format!("{} is configured", credential.source_label()),
        ),
        None => DoctorCheck::new("auth", CheckStatus::Fail, crate::output::MISSING_AUTH_HELP),
    });

    checks.push(permissions_check(config.permission_mode));

    checks.push(DoctorCheck::new(
        "startup",
        CheckStatus::Ok,
        format!(
            "resolved model={}, permission_mode={}, agent_step_limit={}",
            config.model,
            config.permission_mode.label(),
            config.max_agent_steps
        ),
    ));

    checks
}

/// Reports what the current mode will and will not do without being asked.
///
/// This check exists because the most confusing failure in the whole permission
/// system is invisible from the outside: `ask` mode in a pipe refuses every
/// change, correctly, and the only symptom is a model that keeps apologizing.
/// Saying so here turns that into a diagnosis. `yolo` is reported as a warning
/// even though nothing is wrong with it, because a machine configured that way
/// is a fact its owner should be reminded of.
fn permissions_check(mode: PermissionMode) -> DoctorCheck {
    let sandbox = crate::output::SANDBOX_LABEL;
    match mode {
        PermissionMode::Ask => {
            let (status, detail) = match TtyPrompter::available() {
                Some(_) => (
                    CheckStatus::Ok,
                    "mode=ask: changes and commands will ask for approval on this terminal"
                        .to_string(),
                ),
                None => (
                    CheckStatus::Warn,
                    "mode=ask, but this run has no terminal to ask on, so every change and command will be refused; use --auto for bounded workspace changes".to_string(),
                ),
            };
            DoctorCheck::new("permissions", status, detail)
        }
        PermissionMode::Auto => DoctorCheck::new(
            "permissions",
            CheckStatus::Ok,
            format!(
                "mode=auto: bounded workspace changes and read-only commands run without asking; sandbox={sandbox}"
            ),
        ),
        PermissionMode::Yolo => DoctorCheck::new(
            "permissions",
            CheckStatus::Warn,
            format!("mode=yolo: no permission check runs at all; sandbox={sandbox}"),
        ),
    }
}

/// Reports which settings layers were found, and which were found but ignored.
///
/// A file that exists but could not be parsed is the case that most needs to be
/// said out loud: the user wrote settings, fxr silently ran without them, and
/// reporting "no config files found" would tell them to go looking for a file
/// that is already there. Presence and usability are therefore reported
/// separately.
fn config_presence_check(config: &RuntimeConfig) -> DoctorCheck {
    // Paths are written in their documented `~/.fxr/...` and `.fxr.json` forms
    // rather than expanded, so the detail is stable across machines and carries
    // no home directory name into a pasted report.
    let profile = format!("~/{}/settings.json", crate::config::PROFILE_DIR_NAME);
    let project = crate::config::PROJECT_SETTINGS_FILE.to_string();

    let layers = [
        (
            config.user_settings_present,
            config.user_settings_loaded,
            profile,
        ),
        (
            config.project_settings_present,
            config.project_settings_loaded,
            project,
        ),
    ];

    let mut loaded: Vec<&str> = Vec::new();
    let mut unusable: Vec<&str> = Vec::new();
    for (present, was_loaded, name) in &layers {
        if *was_loaded {
            loaded.push(name);
        } else if *present {
            unusable.push(name);
        }
    }

    if loaded.is_empty() && unusable.is_empty() {
        return DoctorCheck::new(
            "config",
            CheckStatus::Warn,
            "no config files found; using defaults and env overrides",
        );
    }

    if unusable.is_empty() {
        return DoctorCheck::new(
            "config",
            CheckStatus::Ok,
            format!("loaded settings from {}", loaded.join(", ")),
        );
    }

    // The specific reason each layer was rejected is reported by its own
    // diagnostic check; this one states the consequence.
    let mut detail = format!("found but could not use {}", unusable.join(", "));
    if loaded.is_empty() {
        detail.push_str("; using defaults and env overrides");
    } else {
        detail.push_str(&format!("; loaded settings from {}", loaded.join(", ")));
    }
    DoctorCheck::new("config", CheckStatus::Warn, detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Cli;

    async fn run_capture(command: Command) -> (ExitCode, String, String) {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_with(Cli { command }, &mut stdout, &mut stderr)
            .await
            .expect("run succeeds");
        (
            code,
            String::from_utf8(stdout).unwrap(),
            String::from_utf8(stderr).unwrap(),
        )
    }

    #[tokio::test]
    async fn version_prints_only_the_version_on_stdout() {
        let (_, stdout, stderr) = run_capture(Command::Version).await;
        assert_eq!(stdout, format!("{}\n", crate::VERSION));
        assert_eq!(stderr, "");
    }

    #[tokio::test]
    async fn help_goes_to_stdout_verbatim() {
        let (_, stdout, stderr) = run_capture(Command::Help {
            page: "PAGE".to_string(),
        })
        .await;
        assert_eq!(stdout, "PAGE");
        assert_eq!(stderr, "");
    }

    #[tokio::test]
    async fn a_rejection_goes_to_stderr_and_terminates_its_line() {
        let (_, stdout, stderr) = run_capture(Command::Rejected {
            message: "nope".to_string(),
        })
        .await;
        assert_eq!(stdout, "");
        assert_eq!(stderr, "nope\n");
    }

    #[tokio::test]
    async fn a_rejection_that_already_ends_in_a_newline_is_not_double_spaced() {
        let (_, _, stderr) = run_capture(Command::Rejected {
            message: "nope\n".to_string(),
        })
        .await;
        assert_eq!(stderr, "nope\n");
    }

    #[test]
    fn a_turn_that_cannot_start_reports_one_error_event_and_a_failure_code() {
        let mut sink = crate::output::RecordingSink::new();
        let code = fail_turn(&mut sink, "no credential".to_string()).expect("reported");
        assert_eq!(sink.events().len(), 1);
        assert!(matches!(sink.events()[0], Event::Error { .. }));
        assert_eq!(
            format!("{code:?}"),
            format!("{:?}", ExitCode::from(TURN_FAILURE_EXIT_CODE))
        );
    }
}
