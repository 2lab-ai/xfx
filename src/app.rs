//! Application composition and command dispatch.
//!
//! [`run`] is the one place that turns a parsed invocation into bytes on a
//! stream and an exit code. Every later slice adds a match arm here rather than
//! a second entry point, so there is exactly one path from argument to effect.

use std::fmt;
use std::io::{self, Write};
use std::process::ExitCode;

use crate::cli::{Cli, Command};
use crate::config::{ConfigError, RuntimeConfig};
use crate::output::{CheckStatus, DoctorCheck, DoctorSnapshot, OutputFormat, StatusSnapshot};

/// The exit code for a rejected invocation.
///
/// Upstream exits 1 rather than the shell's conventional 2 for a usage error
/// (`vercel-labs/fx@580a0c5d tests/e2e/cli.test.ts:404-412`), and scripts around
/// fxr should not have to learn a second convention.
const REJECTED_EXIT_CODE: u8 = 1;

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
pub fn run(cli: Cli) -> Result<ExitCode, AppError> {
    let stdout = io::stdout();
    let stderr = io::stderr();
    run_with(cli, &mut stdout.lock(), &mut stderr.lock())
}

/// Runs one invocation against explicit streams.
///
/// Splitting this out keeps the dispatch table testable without a subprocess,
/// and makes the stdout/stderr split an argument rather than an assumption.
pub fn run_with(
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

    fn run_capture(command: Command) -> (ExitCode, String, String) {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_with(Cli { command }, &mut stdout, &mut stderr).expect("run succeeds");
        (
            code,
            String::from_utf8(stdout).unwrap(),
            String::from_utf8(stderr).unwrap(),
        )
    }

    #[test]
    fn version_prints_only_the_version_on_stdout() {
        let (_, stdout, stderr) = run_capture(Command::Version);
        assert_eq!(stdout, format!("{}\n", crate::VERSION));
        assert_eq!(stderr, "");
    }

    #[test]
    fn help_goes_to_stdout_verbatim() {
        let (_, stdout, stderr) = run_capture(Command::Help {
            page: "PAGE".to_string(),
        });
        assert_eq!(stdout, "PAGE");
        assert_eq!(stderr, "");
    }

    #[test]
    fn a_rejection_goes_to_stderr_and_terminates_its_line() {
        let (_, stdout, stderr) = run_capture(Command::Rejected {
            message: "nope".to_string(),
        });
        assert_eq!(stdout, "");
        assert_eq!(stderr, "nope\n");
    }

    #[test]
    fn a_rejection_that_already_ends_in_a_newline_is_not_double_spaced() {
        let (_, _, stderr) = run_capture(Command::Rejected {
            message: "nope\n".to_string(),
        });
        assert_eq!(stderr, "nope\n");
    }
}
