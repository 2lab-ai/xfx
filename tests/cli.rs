//! Binary-level acceptance tests for the advertised `xfx` command surface, plus
//! integration tests for configuration precedence.
//!
//! Every assertion here is a product promise: what the executable prints, what it
//! exits with, what it refuses to advertise, and what it refuses to create or leak.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;
use xfx::config::{
    CredentialSource, DiagnosticCause, Environment, PermissionMode, RuntimeConfig, SettingSource,
};
use xfx::provider::ProviderId;

/// Upstream command names that xfx deliberately does not implement in v0.1.
///
/// Evidence: `vercel-labs/fx@580a0c5d src/core/cli/cli_surface.zig:58-84`.
/// Advertising any of these would promise behavior the binary does not have.
const DEFERRED_COMMAND_NAMES: &[&str] = &[
    "acp",
    "pr",
    "issue",
    "login",
    "logout",
    "permissions",
    "models",
    "provider",
    "background",
    "teams",
    "credits",
    "usage",
    "upgrade",
    "replay",
    "workspace",
];

/// Environment variables that must never leak in from the developer's shell.
const CONTROLLED_VARS: &[&str] = &[
    "VERCEL_OIDC_TOKEN",
    "AI_GATEWAY_API_KEY",
    "XFX_MODEL",
    "XFX_PERMISSION_MODE",
    "XFX_MAX_AGENT_STEPS",
];

struct Sandbox {
    _root: TempDir,
    home: PathBuf,
    workspace: PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        let root = TempDir::new().expect("create sandbox root");
        let home = root.path().join("home");
        let workspace = root.path().join("workspace");
        fs::create_dir_all(&home).expect("create home");
        fs::create_dir_all(&workspace).expect("create workspace");
        // Canonicalize so the reported workspace matches the path we assert on;
        // macOS resolves the temp root through /private.
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
        for key in CONTROLLED_VARS {
            command.env_remove(key);
        }
        command
    }

    fn run(&self, args: &[&str]) -> Run {
        Run::of(self.command().args(args).output().expect("spawn xfx"))
    }

    fn run_with_env(&self, args: &[&str], env: &[(&str, &str)]) -> Run {
        let mut command = self.command();
        command.args(args);
        for (key, value) in env {
            command.env(key, value);
        }
        Run::of(command.output().expect("spawn xfx"))
    }

    fn profile_dir(&self) -> PathBuf {
        self.home.join(".xfx")
    }

    fn write_user_settings(&self, body: &str) {
        let dir = self.profile_dir();
        fs::create_dir_all(&dir).expect("create profile dir");
        fs::write(dir.join("settings.json"), body).expect("write user settings");
    }

    fn write_project_settings(&self, body: &str) {
        fs::write(self.workspace.join(".xfx.json"), body).expect("write project settings");
    }
}

struct Run {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

impl Run {
    fn of(output: Output) -> Self {
        Self {
            code: output.status.code(),
            stdout: String::from_utf8(output.stdout).expect("stdout is utf-8"),
            stderr: String::from_utf8(output.stderr).expect("stderr is utf-8"),
        }
    }

    fn json(&self) -> Value {
        assert!(
            self.stdout.ends_with('\n'),
            "json output must be newline terminated, got {:?}",
            self.stdout
        );
        assert_eq!(
            self.stdout.matches('\n').count(),
            1,
            "json output must be exactly one line, got {:?}",
            self.stdout
        );
        serde_json::from_str(self.stdout.trim_end()).expect("stdout parses as JSON")
    }
}

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

// ---------------------------------------------------------------------------
// version
// ---------------------------------------------------------------------------

#[test]
fn version_aliases_print_the_bare_package_version() {
    let sandbox = Sandbox::new();
    for alias in ["--version", "-v"] {
        let run = sandbox.run(&[alias]);
        assert_eq!(run.code, Some(0), "{alias} must exit 0");
        assert_eq!(
            run.stdout,
            format!("{}\n", env!("CARGO_PKG_VERSION")),
            "{alias} must print only the version"
        );
        assert_eq!(run.stderr, "", "{alias} must not write diagnostics");
    }
}

// ---------------------------------------------------------------------------
// help
// ---------------------------------------------------------------------------

#[test]
fn help_aliases_render_the_same_page_on_stdout() {
    let sandbox = Sandbox::new();
    let mut pages = Vec::new();
    for alias in ["help", "--help", "-h"] {
        let run = sandbox.run(&[alias]);
        assert_eq!(run.code, Some(0), "{alias} must exit 0");
        assert_eq!(run.stderr, "", "{alias} must not write diagnostics");
        assert!(
            run.stdout.contains("Commands:"),
            "{alias} must list commands, got {:?}",
            run.stdout
        );
        assert!(
            !run.stdout.contains('\u{1b}'),
            "{alias} must not emit ANSI escapes"
        );
        pages.push(run.stdout);
    }
    assert_eq!(pages[0], pages[1], "`help` and `--help` must agree");
    assert_eq!(pages[1], pages[2], "`--help` and `-h` must agree");
}

#[test]
fn help_advertises_every_implemented_command() {
    let sandbox = Sandbox::new();
    let run = sandbox.run(&["--help"]);
    for name in xfx::cli::ADVERTISED_COMMANDS {
        assert!(
            run.stdout.contains(name),
            "help must advertise the implemented command `{name}`, got {:?}",
            run.stdout
        );
    }
}

#[test]
fn help_never_advertises_a_deferred_command() {
    let sandbox = Sandbox::new();
    for alias in ["help", "--help", "-h"] {
        let run = sandbox.run(&[alias]);
        let listed = listed_commands(&run.stdout);
        assert!(
            !listed.is_empty(),
            "{alias} must list its commands, got {:?}",
            run.stdout
        );
        for name in DEFERRED_COMMAND_NAMES {
            assert!(
                !listed.iter().any(|listed| listed == name),
                "{alias} lists the deferred command `{name}`, got {listed:?}"
            );
        }
        let mut listed = listed;
        listed.sort();
        let mut expected: Vec<String> = xfx::cli::ADVERTISED_COMMANDS
            .iter()
            .map(|name| (*name).to_string())
            .collect();
        expected.sort();
        assert_eq!(
            listed, expected,
            "{alias} must list exactly the implemented commands"
        );
    }
}

/// The command names in the `Commands:` block of a help page.
///
/// Only this block is an advertisement. Prose elsewhere may legitimately use a
/// word such as "workspace" that also happens to name a deferred command, and
/// forbidding the word outright would make honest documentation impossible.
fn listed_commands(help: &str) -> Vec<String> {
    help.lines()
        .skip_while(|line| line.trim() != "Commands:")
        .skip(1)
        .take_while(|line| !line.trim().is_empty())
        .filter_map(|line| line.split_whitespace().next())
        .map(str::to_string)
        .collect()
}

/// Matches `name` only as a standalone word so that substrings of real help
/// prose (for example `provider` inside `providers`) cannot cause a false pass
/// and a legitimate word cannot hide inside a longer token.
fn word_present(haystack: &str, name: &str) -> bool {
    haystack
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
        .any(|word| word == name)
}

// ---------------------------------------------------------------------------
// rejection
// ---------------------------------------------------------------------------

#[test]
fn an_unknown_command_fails_on_stderr_with_exit_1() {
    let sandbox = Sandbox::new();
    let run = sandbox.run(&["bogus"]);
    assert_eq!(run.code, Some(1), "unknown command must exit 1");
    assert_eq!(run.stdout, "", "unknown command must not write to stdout");
    assert!(
        run.stderr.contains("bogus"),
        "stderr must name the rejected command, got {:?}",
        run.stderr
    );
}

#[test]
fn a_deferred_command_is_rejected_like_any_unknown_name() {
    let sandbox = Sandbox::new();
    for name in DEFERRED_COMMAND_NAMES {
        let run = sandbox.run(&[name]);
        assert_eq!(run.code, Some(1), "`{name}` must exit 1");
        assert_eq!(run.stdout, "", "`{name}` must not write to stdout");
    }
}

#[test]
fn a_bare_invocation_without_a_terminal_is_refused_with_exit_1() {
    // A bare `xfx` asks for the shell, and a shell needs a terminal. In a test
    // harness there is none, so this is the refusal path; the shell's own
    // acceptance tests in `tests/interactive.rs` drive the other one on a real
    // pty. Nothing is created under the profile home on the way out.
    let sandbox = Sandbox::new();
    let run = sandbox.run(&[]);
    assert_eq!(run.code, Some(1), "bare invocation must exit 1");
    assert_eq!(run.stdout, "", "bare invocation must not write to stdout");
    assert!(
        run.stderr.contains("interactive terminal"),
        "stderr must name the requirement, got {:?}",
        run.stderr
    );
    assert!(
        run.stderr.contains("xfx ask"),
        "stderr must name what to use instead, got {:?}",
        run.stderr
    );
    assert!(
        !sandbox.profile_dir().exists(),
        "a refused shell must not create a profile home"
    );
}

#[test]
fn status_and_doctor_reject_unknown_flags_and_extra_arguments() {
    let sandbox = Sandbox::new();
    for args in [
        vec!["status", "--bogus"],
        vec!["status", "extra"],
        vec!["doctor", "--bogus"],
        vec!["doctor", "extra"],
        vec!["status", "--json", "--json", "extra"],
    ] {
        let run = sandbox.run(&args);
        assert_eq!(run.code, Some(1), "{args:?} must exit 1");
        assert_eq!(run.stdout, "", "{args:?} must not write to stdout");
        assert!(
            !run.stderr.is_empty(),
            "{args:?} must explain the rejection"
        );
    }
}

// ---------------------------------------------------------------------------
// status
// ---------------------------------------------------------------------------

#[test]
fn status_json_is_a_single_newline_terminated_document() {
    let sandbox = Sandbox::new();
    let run = sandbox.run(&["status", "--json"]);
    assert_eq!(run.code, Some(0));
    assert_eq!(run.stderr, "");
    let json = run.json();
    assert_eq!(json["kind"], "status");
    assert_eq!(json["model"], xfx::config::DEFAULT_MODEL);
    assert_eq!(json["permission_mode"], "auto");
    assert_eq!(json["sandbox"], "none");
    assert_eq!(json["workspace"], sandbox.workspace.to_str().unwrap());
    assert_eq!(json["history_turns"], 0);
    assert_eq!(json["session_permission_grants"], 0);
    assert!(json["agent_step_limit"].is_u64());
    assert!(
        matches!(
            json["build_channel"].as_str(),
            Some("debug" | "release" | "preview")
        ),
        "build_channel must be a declared channel, got {:?}",
        json["build_channel"]
    );
    let revision = json["build_revision"].as_str().expect("build_revision");
    assert_eq!(revision.len(), 12, "build revision is 12 characters");
    assert!(
        revision
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()),
        "build revision is lowercase hex, got {revision}"
    );
}

#[test]
fn status_text_reports_the_same_facts_as_status_json() {
    let sandbox = Sandbox::new();
    let text = sandbox.run(&["status"]);
    let json = sandbox.run(&["status", "--json"]).json();
    assert_eq!(text.code, Some(0));
    assert_eq!(text.stderr, "");
    for key in [
        "model",
        "auth",
        "auth_refreshable",
        "permission_mode",
        "sandbox",
        "workspace",
        "history_turns",
        "session_permission_grants",
        "agent_step_limit",
    ] {
        let value = match &json[key] {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        let line = format!("[status] {key}={value}\n");
        assert!(
            text.stdout.contains(&line),
            "status text must contain {line:?}, got {:?}",
            text.stdout
        );
    }
}

#[test]
fn status_defaults_permission_mode_to_auto() {
    let sandbox = Sandbox::new();
    let json = sandbox.run(&["status", "--json"]).json();
    assert_eq!(json["permission_mode"], "auto");
}

// ---------------------------------------------------------------------------
// doctor
// ---------------------------------------------------------------------------

#[test]
fn doctor_json_reports_aggregate_counts_and_named_checks() {
    let sandbox = Sandbox::new();
    let run = sandbox.run(&["doctor", "--json"]);
    assert_eq!(run.code, Some(0));
    assert_eq!(run.stderr, "");
    let json = run.json();
    assert_eq!(json["kind"], "doctor");
    assert_eq!(json["workspace"], sandbox.workspace.to_str().unwrap());

    let checks = json["checks"].as_array().expect("checks array");
    assert!(!checks.is_empty(), "doctor must run at least one check");
    let mut ok = 0u64;
    let mut warn = 0u64;
    let mut fail = 0u64;
    for check in checks {
        assert!(check["name"].is_string(), "check name is a string");
        assert!(check["detail"].is_string(), "check detail is a string");
        match check["status"].as_str().expect("check status") {
            "ok" => ok += 1,
            "warn" => warn += 1,
            "fail" => fail += 1,
            other => panic!("unexpected check status {other}"),
        }
    }
    assert_eq!(json["ok_count"], ok);
    assert_eq!(json["warn_count"], warn);
    assert_eq!(json["fail_count"], fail);

    let names: Vec<&str> = checks.iter().map(|c| c["name"].as_str().unwrap()).collect();
    for expected in [
        "workspace",
        "config",
        "auth",
        "permissions",
        "sessions",
        "startup",
    ] {
        assert!(
            names.contains(&expected),
            "doctor must run the `{expected}` check, got {names:?}"
        );
    }
}

/// Creates a session directory the store will agree to read.
///
/// `0700` on both levels, because the store re-checks that on every use: state
/// reachable by group or other is refused rather than read, and a fixture that
/// forgot would be testing the refusal instead of the check under test.
fn private_session_dir(sandbox: &Sandbox, id: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let session = sandbox.profile_dir().join("sessions").join(id);
    fs::create_dir_all(&session).expect("create a session directory");
    for level in [
        sandbox.profile_dir(),
        sandbox.profile_dir().join("sessions"),
        session.clone(),
    ] {
        fs::set_permissions(&level, fs::Permissions::from_mode(0o700)).expect("make it private");
    }
    session
}

/// The `detail` of one named `doctor` check.
fn doctor_check(sandbox: &Sandbox, name: &str) -> (String, String) {
    let json = sandbox.run(&["doctor", "--json"]).json();
    let checks = json["checks"].as_array().expect("checks array").clone();
    let check = checks
        .iter()
        .find(|check| check["name"] == name)
        .unwrap_or_else(|| panic!("doctor has no `{name}` check: {checks:?}"));
    (
        check["status"].as_str().expect("status").to_string(),
        check["detail"].as_str().expect("detail").to_string(),
    )
}

#[test]
fn doctor_reports_an_empty_session_store_as_nothing_wrong() {
    let sandbox = Sandbox::new();
    let (status, detail) = doctor_check(&sandbox, "sessions");
    assert_eq!(status, "ok", "{detail}");
    assert!(detail.contains("0 session(s)"), "{detail}");
    // Still read-only: asking about the store must not create it.
    assert!(!sandbox.profile_dir().exists());
}

#[test]
fn doctor_reports_a_session_directory_it_cannot_read() {
    let sandbox = Sandbox::new();
    // A directory with a usable name and nothing inside it is not a session
    // xfx wrote, and `xfx sessions` will quietly skip it. `doctor` is where
    // that becomes visible.
    private_session_dir(&sandbox, "1700000000000-aaaaaaaaaaaaaaaa");
    let (status, detail) = doctor_check(&sandbox, "sessions");
    assert_eq!(status, "warn", "{detail}");
    assert!(detail.contains("could not be read"), "{detail}");
}

#[test]
fn doctor_reports_a_staged_manifest_left_by_an_interrupted_write() {
    let sandbox = Sandbox::new();
    // Exactly what a process killed between staging and rename leaves behind.
    // xfx never unlinks one, because it cannot know whose it is; the promise
    // that replaces removal is that `doctor` says it is there.
    let session = private_session_dir(&sandbox, "1700000000000-bbbbbbbbbbbbbbbb");
    fs::write(
        session.join("session.json.999.abcdef0123456789.staged"),
        "{}",
    )
    .expect("leave a stage file");

    let (status, detail) = doctor_check(&sandbox, "sessions");
    assert_eq!(status, "warn", "{detail}");
    assert!(detail.contains("1 staged manifest"), "{detail}");
    assert!(detail.contains("interrupted write"), "{detail}");
    // A report, not a repair.
    assert!(
        session
            .join("session.json.999.abcdef0123456789.staged")
            .exists(),
        "doctor must not delete anything"
    );
}

#[test]
fn doctor_text_reports_counts_and_one_line_per_check() {
    let sandbox = Sandbox::new();
    let text = sandbox.run(&["doctor"]);
    let json = sandbox.run(&["doctor", "--json"]).json();
    assert_eq!(text.code, Some(0));
    assert_eq!(text.stderr, "");
    assert!(
        text.stdout.starts_with(&format!(
            "[doctor] ok={} warn={} fail={}\n",
            json["ok_count"], json["warn_count"], json["fail_count"]
        )),
        "doctor text must lead with the aggregate counts, got {:?}",
        text.stdout
    );
    for check in json["checks"].as_array().unwrap() {
        let line = format!(
            "[{}] {}: {}\n",
            check["status"].as_str().unwrap(),
            check["name"].as_str().unwrap(),
            check["detail"].as_str().unwrap()
        );
        assert!(
            text.stdout.contains(&line),
            "doctor text must contain {line:?}, got {:?}",
            text.stdout
        );
    }
}

// ---------------------------------------------------------------------------
// read-only guarantees
// ---------------------------------------------------------------------------

#[test]
fn read_only_commands_never_create_the_profile_directory() {
    let sandbox = Sandbox::new();
    for args in [
        vec!["status"],
        vec!["status", "--json"],
        vec!["doctor"],
        vec!["doctor", "--json"],
        vec!["--help"],
        vec!["--version"],
    ] {
        let run = sandbox.run(&args);
        assert_eq!(run.code, Some(0), "{args:?} must succeed");
        assert!(
            !sandbox.profile_dir().exists(),
            "{args:?} must not create {}",
            sandbox.profile_dir().display()
        );
    }
    let entries: Vec<_> = fs::read_dir(&sandbox.home)
        .expect("read home")
        .map(|e| e.unwrap().file_name())
        .collect();
    assert!(
        entries.is_empty(),
        "read-only commands must leave an empty home, found {entries:?}"
    );
}

#[test]
fn snapshots_report_the_credential_source_and_never_its_bytes() {
    let sandbox = Sandbox::new();
    let secret = "xfx-test-secret-value-must-not-appear";
    for (var, label) in [
        ("VERCEL_OIDC_TOKEN", "VERCEL_OIDC_TOKEN"),
        ("AI_GATEWAY_API_KEY", "AI_GATEWAY_API_KEY"),
    ] {
        for args in [
            vec!["status", "--json"],
            vec!["status"],
            vec!["doctor", "--json"],
            vec!["doctor"],
        ] {
            let run = sandbox.run_with_env(&args, &[(var, secret)]);
            assert_eq!(run.code, Some(0), "{args:?} must succeed");
            assert!(
                !run.stdout.contains(secret),
                "{args:?} leaked the secret on stdout"
            );
            assert!(
                !run.stderr.contains(secret),
                "{args:?} leaked the secret on stderr"
            );
            assert!(
                run.stdout.contains(label),
                "{args:?} must report the source label `{label}`, got {:?}",
                run.stdout
            );
        }
    }
}

#[test]
fn the_oidc_token_outranks_the_gateway_key_unless_it_is_blank() {
    let sandbox = Sandbox::new();
    let both = sandbox.run_with_env(
        &["status", "--json"],
        &[
            ("VERCEL_OIDC_TOKEN", "oidc-value"),
            ("AI_GATEWAY_API_KEY", "gateway-value"),
        ],
    );
    assert_eq!(both.json()["auth"], "VERCEL_OIDC_TOKEN");

    let blank = sandbox.run_with_env(
        &["status", "--json"],
        &[
            ("VERCEL_OIDC_TOKEN", "   \t "),
            ("AI_GATEWAY_API_KEY", "gateway-value"),
        ],
    );
    assert_eq!(blank.json()["auth"], "AI_GATEWAY_API_KEY");

    let empty = sandbox.run_with_env(
        &["status", "--json"],
        &[("VERCEL_OIDC_TOKEN", ""), ("AI_GATEWAY_API_KEY", "")],
    );
    assert_eq!(empty.json()["auth"], "missing");
}

#[test]
fn missing_credentials_are_a_reported_fact_not_a_failure() {
    let sandbox = Sandbox::new();
    let status = sandbox.run(&["status", "--json"]);
    let doctor = sandbox.run(&["doctor", "--json"]);
    assert_eq!(status.code, Some(0), "status must not require credentials");
    assert_eq!(doctor.code, Some(0), "doctor must not require credentials");

    let status_json = status.json();
    assert_eq!(status_json["auth"], "missing");
    assert_eq!(status_json["auth_refreshable"], false);
    let help = status_json["auth_help"].as_str().expect("auth_help");
    assert!(
        help.contains("VERCEL_OIDC_TOKEN") && help.contains("AI_GATEWAY_API_KEY"),
        "auth help must name the supported credentials, got {help}"
    );
    for name in DEFERRED_COMMAND_NAMES {
        assert!(
            !word_present(help, name),
            "auth help must not point at the deferred command `{name}`: {help}"
        );
    }

    let doctor_json = doctor.json();
    assert_eq!(doctor_json["auth"], "missing");
    let auth_check = doctor_json["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "auth")
        .expect("auth check");
    assert_eq!(auth_check["status"], "fail");
    assert_eq!(auth_check["detail"], help);
}

// ---------------------------------------------------------------------------
// environment overrides
// ---------------------------------------------------------------------------

#[test]
fn status_and_doctor_apply_an_exact_max_agent_steps_override() {
    let sandbox = Sandbox::new();
    let env = [("XFX_MAX_AGENT_STEPS", "3")];
    let status = sandbox.run_with_env(&["status", "--json"], &env);
    assert_eq!(status.json()["agent_step_limit"], 3);

    let doctor = sandbox.run_with_env(&["doctor", "--json"], &env);
    let startup = doctor.json()["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "startup")
        .cloned()
        .expect("startup check");
    assert!(
        startup["detail"]
            .as_str()
            .unwrap()
            .contains("agent_step_limit=3"),
        "startup detail must carry the resolved limit, got {startup:?}"
    );
}

#[test]
fn status_applies_model_and_permission_mode_overrides() {
    let sandbox = Sandbox::new();
    let json = sandbox
        .run_with_env(
            &["status", "--json"],
            &[
                ("XFX_MODEL", "vendor/override-model"),
                ("XFX_PERMISSION_MODE", "yolo"),
            ],
        )
        .json();
    assert_eq!(json["model"], "vendor/override-model");
    assert_eq!(json["permission_mode"], "yolo");
}

#[test]
fn an_invalid_permission_mode_override_is_a_diagnostic_not_a_crash() {
    let sandbox = Sandbox::new();
    let status = sandbox.run_with_env(
        &["status", "--json"],
        &[("XFX_PERMISSION_MODE", "definitely-not-a-mode")],
    );
    assert_eq!(status.code, Some(0));
    assert_eq!(status.json()["permission_mode"], "auto");

    let doctor = sandbox.run_with_env(
        &["doctor", "--json"],
        &[("XFX_PERMISSION_MODE", "definitely-not-a-mode")],
    );
    let warned = doctor.json()["checks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["name"] == "config" && c["status"] == "warn");
    assert!(warned, "doctor must warn about the rejected override");
}

// ---------------------------------------------------------------------------
// settings discovery through the binary
// ---------------------------------------------------------------------------

/// Every `config` check detail, in order.
fn config_details(json: &Value) -> Vec<String> {
    json["checks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|check| check["name"] == "config")
        .map(|check| check["detail"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn a_malformed_settings_file_warns_without_failing_the_command() {
    let sandbox = Sandbox::new();
    sandbox.write_user_settings("{ this is not json");
    let status = sandbox.run(&["status", "--json"]);
    assert_eq!(status.code, Some(0));
    assert_eq!(status.json()["model"], xfx::config::DEFAULT_MODEL);

    let doctor = sandbox.run(&["doctor", "--json"]).json();
    let warned = doctor["checks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["name"] == "config" && c["status"] == "warn");
    assert!(warned, "doctor must warn about unreadable settings");

    // The file is on disk. Reporting that none was found would send the user
    // hunting for a file they already wrote.
    let details = config_details(&doctor);
    assert!(
        !details.iter().any(|d| d.contains("no config files found")),
        "a settings file exists; doctor must not report that none was found: {details:?}"
    );
    assert!(
        details
            .iter()
            .any(|d| d.contains("found but could not use") && d.contains("~/.xfx/settings.json")),
        "doctor must say the profile settings were found and ignored: {details:?}"
    );
    assert!(
        details.iter().any(|d| d.contains("malformed_settings")),
        "doctor must still report why the layer was rejected: {details:?}"
    );
}

#[test]
fn an_unusable_layer_does_not_hide_a_usable_one() {
    let sandbox = Sandbox::new();
    sandbox.write_user_settings("{ this is not json");
    sandbox.write_project_settings("{\"max_agent_steps\":7}");

    let doctor = sandbox.run(&["doctor", "--json"]).json();
    assert_eq!(
        doctor["agent_step_limit"], 7,
        "the usable layer must still apply"
    );

    let details = config_details(&doctor);
    assert!(
        details.iter().any(|d| {
            d.contains("found but could not use")
                && d.contains("~/.xfx/settings.json")
                && d.contains("loaded settings from")
                && d.contains(".xfx.json")
        }),
        "doctor must report both the rejected and the loaded layer: {details:?}"
    );
}

#[test]
fn a_directory_where_a_settings_file_belongs_is_found_but_unusable() {
    let sandbox = Sandbox::new();
    // A directory at the settings path is present but can never be parsed.
    fs::create_dir_all(sandbox.profile_dir().join("settings.json")).expect("occupy the path");

    let doctor = sandbox.run(&["doctor", "--json"]);
    assert_eq!(doctor.code, Some(0), "doctor must still run");
    let details = config_details(&doctor.json());
    assert!(
        !details.iter().any(|d| d.contains("no config files found")),
        "the path is occupied; doctor must not report that none was found: {details:?}"
    );
    assert!(
        details
            .iter()
            .any(|d| d.contains("found but could not use")),
        "doctor must report the occupied path: {details:?}"
    );
}

#[test]
fn an_oversized_settings_file_is_found_but_unusable() {
    let sandbox = Sandbox::new();
    let padding = "x".repeat(xfx::config::MAX_SETTINGS_BYTES + 1);
    sandbox.write_user_settings(&format!("{{\"model\":\"{padding}\"}}"));

    let details = config_details(&sandbox.run(&["doctor", "--json"]).json());
    assert!(
        !details.iter().any(|d| d.contains("no config files found")),
        "an oversized file still exists on disk: {details:?}"
    );
    assert!(
        details.iter().any(|d| d.contains("settings_too_large")),
        "doctor must report the size rejection: {details:?}"
    );
}

#[test]
fn doctor_reports_which_settings_layers_were_found() {
    let sandbox = Sandbox::new();
    let empty = sandbox.run(&["doctor", "--json"]).json();
    let config_detail = |json: &Value| -> String {
        json["checks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "config")
            .unwrap()["detail"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert!(
        config_detail(&empty).contains("no config files found"),
        "got {}",
        config_detail(&empty)
    );

    sandbox.write_user_settings("{\"model\":\"vendor/from-profile\"}");
    sandbox.write_project_settings("{\"max_agent_steps\":7}");
    let both = sandbox.run(&["doctor", "--json"]).json();
    let detail = config_detail(&both);
    assert!(detail.contains("~/.xfx/settings.json"), "got {detail}");
    assert!(detail.contains(".xfx.json"), "got {detail}");
    assert_eq!(both["model"], "vendor/from-profile");
    assert_eq!(both["agent_step_limit"], 7);
}

// ---------------------------------------------------------------------------
// configuration precedence (library level, no process environment races)
// ---------------------------------------------------------------------------

fn environment(home: &Path, vars: &[(&str, &str)]) -> Environment {
    let mut map = BTreeMap::new();
    for (key, value) in vars {
        map.insert((*key).to_string(), (*value).to_string());
    }
    Environment::new(Some(home.to_path_buf()), map)
}

#[test]
fn precedence_runs_project_then_profile_then_exact_workspace_then_environment() {
    let sandbox = Sandbox::new();
    let workspace = sandbox.workspace.clone();
    let workspace_key = workspace.to_str().unwrap().to_string();

    // Compiled default only.
    let config = RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &workspace).unwrap();
    assert_eq!(config.max_agent_steps, xfx::config::DEFAULT_MAX_AGENT_STEPS);
    assert_eq!(
        config.sources.max_agent_steps,
        SettingSource::CompiledDefault
    );

    // Project layer wins over the compiled default.
    sandbox.write_project_settings("{\"max_agent_steps\":3}");
    let config = RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &workspace).unwrap();
    assert_eq!(config.max_agent_steps, 3);
    assert_eq!(config.sources.max_agent_steps, SettingSource::Project);

    // Profile global wins over the project layer.
    sandbox.write_user_settings("{\"max_agent_steps\":5}");
    let config = RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &workspace).unwrap();
    assert_eq!(config.max_agent_steps, 5);
    assert_eq!(config.sources.max_agent_steps, SettingSource::UserGlobal);

    // The exact workspace entry wins over the profile global.
    sandbox.write_user_settings(&format!(
        "{{\"max_agent_steps\":5,\"workspaces\":{{{}:{{\"max_agent_steps\":7}}}}}}",
        serde_json::to_string(&workspace_key).unwrap()
    ));
    let config = RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &workspace).unwrap();
    assert_eq!(config.max_agent_steps, 7);
    assert_eq!(config.sources.max_agent_steps, SettingSource::UserWorkspace);

    // The process environment wins over every file layer.
    let config = RuntimeConfig::load_with(
        &environment(&sandbox.home, &[("XFX_MAX_AGENT_STEPS", "9")]),
        &workspace,
    )
    .unwrap();
    assert_eq!(config.max_agent_steps, 9);
    assert_eq!(
        config.sources.max_agent_steps,
        SettingSource::ProcessOverride
    );
}

// `0` means an unbounded agent step limit, matching upstream
// (`vercel-labs/fx@580a0c5d src/core/config/agent_steps.zig:3-31`). It is the one
// configured value that looks like "unset" to a careless reader, so each layer
// that can carry it is proven to preserve it rather than fall back to the
// compiled default.

#[test]
fn an_environment_zero_selects_an_unbounded_step_limit() {
    let sandbox = Sandbox::new();
    let config = RuntimeConfig::load_with(
        &environment(&sandbox.home, &[("XFX_MAX_AGENT_STEPS", "0")]),
        &sandbox.workspace,
    )
    .unwrap();
    assert_eq!(
        config.max_agent_steps, 0,
        "explicit 0 must not fall back to the compiled default"
    );
    assert_ne!(config.max_agent_steps, xfx::config::DEFAULT_MAX_AGENT_STEPS);
    assert_eq!(
        config.sources.max_agent_steps,
        SettingSource::ProcessOverride
    );
}

#[test]
fn a_project_zero_selects_an_unbounded_step_limit() {
    let sandbox = Sandbox::new();
    sandbox.write_project_settings("{\"max_agent_steps\":0}");
    let config =
        RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &sandbox.workspace).unwrap();
    assert_eq!(
        config.max_agent_steps, 0,
        "explicit 0 must not fall back to the compiled default"
    );
    assert_eq!(config.sources.max_agent_steps, SettingSource::Project);
}

#[test]
fn a_zero_step_limit_overrides_a_configured_bound_at_every_layer() {
    let sandbox = Sandbox::new();
    let workspace_key = sandbox.workspace.to_str().unwrap().to_string();

    // A profile bound is replaced by a project-free profile zero.
    sandbox.write_user_settings("{\"max_agent_steps\":0}");
    sandbox.write_project_settings("{\"max_agent_steps\":11}");
    let config =
        RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &sandbox.workspace).unwrap();
    assert_eq!(config.max_agent_steps, 0);
    assert_eq!(config.sources.max_agent_steps, SettingSource::UserGlobal);

    // The exact workspace entry can also select unbounded.
    sandbox.write_user_settings(&format!(
        "{{\"max_agent_steps\":11,\"workspaces\":{{{}:{{\"max_agent_steps\":0}}}}}}",
        serde_json::to_string(&workspace_key).unwrap()
    ));
    let config =
        RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &sandbox.workspace).unwrap();
    assert_eq!(config.max_agent_steps, 0);
    assert_eq!(config.sources.max_agent_steps, SettingSource::UserWorkspace);

    // And a configured zero is still overridable by a bounded environment value.
    let config = RuntimeConfig::load_with(
        &environment(&sandbox.home, &[("XFX_MAX_AGENT_STEPS", "4")]),
        &sandbox.workspace,
    )
    .unwrap();
    assert_eq!(config.max_agent_steps, 4);
    assert_eq!(
        config.sources.max_agent_steps,
        SettingSource::ProcessOverride
    );
}

#[test]
fn status_and_doctor_report_an_unbounded_step_limit_as_zero() {
    let sandbox = Sandbox::new();
    let env = [("XFX_MAX_AGENT_STEPS", "0")];

    let status = sandbox.run_with_env(&["status", "--json"], &env);
    assert_eq!(status.code, Some(0));
    assert_eq!(status.json()["agent_step_limit"], 0);

    let doctor = sandbox.run_with_env(&["doctor", "--json"], &env).json();
    assert_eq!(doctor["agent_step_limit"], 0);
    let startup = doctor["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "startup")
        .cloned()
        .expect("startup check");
    assert!(
        startup["detail"]
            .as_str()
            .unwrap()
            .contains("agent_step_limit=0"),
        "startup detail must carry the unbounded limit, got {startup:?}"
    );
}

#[test]
fn a_workspace_entry_for_another_directory_is_ignored() {
    let sandbox = Sandbox::new();
    sandbox.write_user_settings(
        "{\"max_agent_steps\":5,\"workspaces\":{\"/some/other/place\":{\"max_agent_steps\":7}}}",
    );
    let config =
        RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &sandbox.workspace).unwrap();
    assert_eq!(config.max_agent_steps, 5);
    assert_eq!(config.sources.max_agent_steps, SettingSource::UserGlobal);
}

#[test]
fn project_settings_cannot_set_profile_only_keys() {
    let sandbox = Sandbox::new();
    sandbox.write_project_settings(
        "{\"model\":\"vendor/from-project\",\"permission_mode\":\"yolo\",\"max_agent_steps\":3}",
    );
    let config =
        RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &sandbox.workspace).unwrap();

    assert_eq!(config.model, xfx::config::DEFAULT_MODEL);
    assert_eq!(config.permission_mode, PermissionMode::Auto);
    assert_eq!(config.max_agent_steps, 3, "project-scoped keys still apply");

    let ignored: Vec<&str> = config
        .diagnostics
        .iter()
        .filter_map(|d| d.ignored_setting_key())
        .collect();
    assert!(ignored.contains(&"model"), "got {ignored:?}");
    assert!(ignored.contains(&"permission_mode"), "got {ignored:?}");
}

#[test]
fn the_provider_defaults_to_the_gateway_and_the_profile_may_select_llmux() {
    let sandbox = Sandbox::new();
    let config =
        RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &sandbox.workspace).unwrap();
    assert_eq!(config.provider, ProviderId::Gateway);
    assert_eq!(config.llmux_url, None);
    assert_eq!(config.sources.provider, SettingSource::CompiledDefault);

    sandbox.write_user_settings("{\"backend\":\"llmux\",\"llmux_url\":\"http://127.0.0.1:3456\"}");
    let config =
        RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &sandbox.workspace).unwrap();
    assert_eq!(config.provider, ProviderId::Llmux);
    assert_eq!(config.llmux_url.as_deref(), Some("http://127.0.0.1:3456"));
    assert_eq!(config.sources.provider, SettingSource::UserGlobal);
    assert!(config.diagnostics.is_empty(), "{:?}", config.diagnostics);
}

#[test]
fn an_exact_workspace_entry_may_choose_a_different_provider() {
    let sandbox = Sandbox::new();
    let workspace_key = sandbox.workspace.to_str().unwrap().to_string();
    sandbox.write_user_settings(&format!(
        "{{\"backend\":\"gateway\",\"workspaces\":{{{}:{{\"backend\":\"llmux\",\
         \"llmux_url\":\"http://127.0.0.1:4000\"}}}}}}",
        serde_json::to_string(&workspace_key).unwrap()
    ));
    let config =
        RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &sandbox.workspace).unwrap();
    assert_eq!(config.provider, ProviderId::Llmux);
    assert_eq!(config.llmux_url.as_deref(), Some("http://127.0.0.1:4000"));
    assert_eq!(config.sources.provider, SettingSource::UserWorkspace);
}

#[test]
fn project_settings_cannot_choose_the_provider_or_point_at_an_endpoint() {
    // A repository is shared. If `.xfx.json` could set these, cloning one would
    // be enough to redirect a prompt at an endpoint of its author's choosing.
    let sandbox = Sandbox::new();
    sandbox.write_project_settings("{\"backend\":\"llmux\",\"llmux_url\":\"http://127.0.0.1:9\"}");
    let config =
        RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &sandbox.workspace).unwrap();
    assert_eq!(config.provider, ProviderId::Gateway);
    assert_eq!(config.llmux_url, None);

    let ignored: Vec<&str> = config
        .diagnostics
        .iter()
        .filter_map(|d| d.ignored_setting_key())
        .collect();
    assert!(ignored.contains(&"backend"), "got {ignored:?}");
    assert!(ignored.contains(&"llmux_url"), "got {ignored:?}");
}

#[test]
fn a_refused_llmux_url_is_a_diagnostic_and_never_becomes_an_endpoint() {
    let sandbox = Sandbox::new();
    sandbox.write_user_settings("{\"backend\":\"llmux\",\"llmux_url\":\"http://example.com:80\"}");
    let config =
        RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &sandbox.workspace).unwrap();
    assert_eq!(
        config.llmux_url, None,
        "a url the transport rule refuses must not survive as a string"
    );
    // The provider the operator chose is kept: a turn that cannot name an
    // endpoint has to refuse, not quietly send the prompt to the Gateway.
    assert_eq!(config.provider, ProviderId::Llmux);
    assert!(
        config
            .diagnostics
            .iter()
            .any(|d| d.setting_key.as_deref() == Some("llmux_url")),
        "got {:?}",
        config.diagnostics
    );
}

#[test]
fn a_refused_llmux_url_in_a_later_layer_clears_the_one_below_it() {
    // `merge` only overwrote on `Some`, so a workspace entry naming a url xfx
    // refused left the profile's url in charge -- the operator's latest word was
    // dropped and an older one silently governed where the prompt went. That is
    // the fallback `backend_rejected` exists to refuse, arriving through the
    // other key.
    let sandbox = Sandbox::new();
    let workspace_key = sandbox.workspace.to_str().unwrap().to_string();
    sandbox.write_user_settings(&format!(
        "{{\"backend\":\"llmux\",\"llmux_url\":\"http://127.0.0.1:4000\",\
         \"workspaces\":{{{}:{{\"llmux_url\":\"https://collector.example.com\"}}}}}}",
        serde_json::to_string(&workspace_key).unwrap()
    ));
    let config =
        RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &sandbox.workspace).unwrap();
    assert_eq!(
        config.llmux_url, None,
        "the refused later layer must clear the earlier value, not defer to it"
    );
    assert_eq!(config.provider, ProviderId::Llmux);
    assert!(config
        .diagnostics
        .iter()
        .any(|d| d.setting_key.as_deref() == Some("llmux_url")));

    // And the turn refuses rather than quietly using the profile's daemon.
    let run = sandbox.run(&["ask", "--json", "--no-save", "hello"]);
    assert_eq!(run.code, Some(1), "stdout={:?}", run.stdout);
    assert!(run.stdout.contains("xfx setup llmux"), "{}", run.stdout);
}

#[test]
fn a_workspace_entry_url_records_its_own_provenance() {
    let sandbox = Sandbox::new();
    let workspace_key = sandbox.workspace.to_str().unwrap().to_string();
    sandbox.write_user_settings(&format!(
        "{{\"backend\":\"llmux\",\"llmux_url\":\"http://127.0.0.1:4000\",\
         \"workspaces\":{{{}:{{\"llmux_url\":\"http://127.0.0.1:4111\"}}}}}}",
        serde_json::to_string(&workspace_key).unwrap()
    ));
    let config =
        RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &sandbox.workspace).unwrap();
    assert_eq!(config.llmux_url.as_deref(), Some("http://127.0.0.1:4111"));
    assert_eq!(config.sources.llmux_url, SettingSource::UserWorkspace);
}

#[test]
fn a_provider_name_is_read_without_regard_to_case_or_padding() {
    let sandbox = Sandbox::new();
    for spelling in ["llmux", "Llmux", "LLMUX", "  llmux \n"] {
        sandbox.write_user_settings(&format!(
            "{{\"backend\":{}}}",
            serde_json::to_string(spelling).unwrap()
        ));
        let config =
            RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &sandbox.workspace).unwrap();
        assert_eq!(config.provider, ProviderId::Llmux, "for {spelling:?}");
        assert!(config.diagnostics.is_empty(), "for {spelling:?}");
    }
}

#[test]
fn an_unreadable_provider_selection_poisons_turns_rather_than_falling_back_to_the_gateway() {
    // The fallback this replaces would have sent the prompt *and the Vercel
    // credential* to a remote paid endpoint because a settings value was
    // mistyped -- the same silent redirection the llmux url rules forbid.
    let sandbox = Sandbox::new();
    sandbox.write_user_settings("{\"backend\":\"definitely-not-a-backend\"}");
    let config =
        RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &sandbox.workspace).unwrap();
    let rejection = config
        .provider_rejected
        .as_ref()
        .expect("an unreadable selection is kept, never defaulted");
    assert_eq!(rejection.key, "backend");
    assert_eq!(rejection.value, "definitely-not-a-backend");
    assert!(
        config
            .diagnostics
            .iter()
            .any(|d| d.setting_key.as_deref() == Some("backend")),
        "got {:?}",
        config.diagnostics
    );

    // status still describes the machine: a broken setting is a fact to report,
    // not a reason to refuse to run.
    let status_run = sandbox.run(&["status"]);
    assert_eq!(status_run.code, Some(0));

    // But no turn is sent anywhere, even with a credential available.
    let run = sandbox.run_with_env(
        &["ask", "--json", "--no-save", "hello"],
        &[("AI_GATEWAY_API_KEY", "must-not-be-sent")],
    );
    assert_eq!(run.code, Some(1), "stdout={:?}", run.stdout);
    // The refusal names the provider axis (the runtime axis) and quotes what it
    // could not read, including both the settings key and the rejected value.
    // It appears in the error message of the JSON error event.
    assert!(run.stdout.contains("provider=rejected"), "{}", run.stdout);
    assert!(run.stdout.contains("`backend`"), "{}", run.stdout);
    assert!(
        run.stdout.contains("definitely-not-a-backend"),
        "the refusal must quote the value: {}",
        run.stdout
    );
    assert!(
        !run.stdout.contains("must-not-be-sent") && !run.stderr.contains("must-not-be-sent"),
        "the credential must not appear anywhere"
    );
}

#[test]
fn a_provider_key_is_read_and_a_flat_model_seeds_that_providers_entry() {
    // Rule 1 and rule 2: the derived provider is the one named, and a flat
    // `model` seeds `models[<provider>]` in memory only.
    let sandbox = Sandbox::new();
    sandbox.write_user_settings(
        "{\"provider\":\"llmux\",\"llmux_url\":\"http://127.0.0.1:3456\",\"model\":\"fable\"}",
    );
    let config =
        RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &sandbox.workspace).unwrap();
    assert_eq!(config.provider, ProviderId::Llmux);
    assert_eq!(config.model, "fable");
    assert_eq!(
        config.models.get("llmux").map(String::as_str),
        Some("fable")
    );
}

#[test]
fn a_models_entry_outranks_a_flat_model_inside_one_layer() {
    let sandbox = Sandbox::new();
    sandbox.write_user_settings(
        "{\"provider\":\"llmux\",\"llmux_url\":\"http://127.0.0.1:3456\",\
          \"model\":\"flat\",\"models\":{\"llmux\":\"chosen\",\"gateway\":\"other\"}}",
    );
    let config =
        RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &sandbox.workspace).unwrap();
    assert_eq!(
        config.model, "chosen",
        "the newer key is the deliberate one"
    );
    // The other provider's preference is kept, not discarded: switching back
    // must not lose what was chosen for it.
    assert_eq!(
        config.models.get("gateway").map(String::as_str),
        Some("other")
    );
}

#[test]
fn a_higher_layer_flat_model_still_outranks_a_lower_layer_models_entry() {
    // Layer order is untouched by the new key. A workspace entry that pins this
    // directory's model must not stop working because the global file grew a
    // `models` object.
    let sandbox = Sandbox::new();
    sandbox.write_user_settings(&format!(
        "{{\"provider\":\"gateway\",\"models\":{{\"gateway\":\"global\"}},\
          \"workspaces\":{{{}:{{\"model\":\"pinned\"}}}}}}",
        serde_json::to_string(&sandbox.workspace.display().to_string()).unwrap()
    ));
    let config =
        RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &sandbox.workspace).unwrap();
    assert_eq!(config.model, "pinned");
    assert_eq!(config.sources.model, SettingSource::UserWorkspace);
}

#[test]
fn an_environment_model_still_outranks_every_layer_including_models() {
    let sandbox = Sandbox::new();
    sandbox.write_user_settings("{\"provider\":\"gateway\",\"models\":{\"gateway\":\"file\"}}");
    let config = RuntimeConfig::load_with(
        &Environment::new(
            Some(sandbox.home.clone()),
            [("XFX_MODEL".to_string(), "shell".to_string())]
                .into_iter()
                .collect(),
        ),
        &sandbox.workspace,
    )
    .expect("load config");
    assert_eq!(config.model, "shell");
    assert_eq!(config.sources.model, SettingSource::ProcessOverride);
}

#[test]
fn a_provider_key_outranks_a_legacy_backend_key_and_the_disagreement_is_reported() {
    // Coexistence: the newer key is the one a newer binary wrote deliberately.
    // A machine whose profile says two different things about where prompts go
    // is exactly the machine an operator should be told about.
    let sandbox = Sandbox::new();
    sandbox.write_user_settings(
        "{\"backend\":\"gateway\",\"provider\":\"llmux\",\"llmux_url\":\"http://127.0.0.1:3456\"}",
    );
    let config =
        RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &sandbox.workspace).unwrap();
    assert_eq!(config.provider, ProviderId::Llmux);

    let details: Vec<String> = config.diagnostics.iter().map(|d| d.detail()).collect();
    let conflict = details
        .iter()
        .find(|detail| detail.contains("conflicting_provider_selection"))
        .unwrap_or_else(|| panic!("no conflict reported: {details:?}"));
    assert!(conflict.contains("provider=llmux"), "{conflict}");
    assert!(conflict.contains("backend=gateway"), "{conflict}");
}

#[test]
fn two_keys_that_agree_are_not_reported_as_a_disagreement() {
    let sandbox = Sandbox::new();
    sandbox.write_user_settings(
        "{\"backend\":\"llmux\",\"provider\":\"llmux\",\"llmux_url\":\"http://127.0.0.1:3456\"}",
    );
    let config =
        RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &sandbox.workspace).unwrap();
    assert!(
        !config
            .diagnostics
            .iter()
            .any(|d| d.cause == DiagnosticCause::ConflictingProviderSelection),
        "agreement is not a problem to report"
    );
}

#[test]
fn an_unreadable_provider_poisons_the_turn_even_when_backend_is_readable() {
    // The operator's most recent word is what they meant. Quietly using the
    // older key would be exactly the fallback this mechanism exists to refuse.
    let sandbox = Sandbox::new();
    sandbox.write_user_settings("{\"backend\":\"gateway\",\"provider\":\"not-a-provider\"}");
    let config =
        RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &sandbox.workspace).unwrap();
    let rejection = config.provider_rejected.expect("kept, never defaulted");
    assert_eq!(rejection.key, "provider");
    assert_eq!(rejection.value, "not-a-provider");

    let run = sandbox.run(&["ask", "--no-save", "hello"]);
    assert_eq!(run.code, Some(1));
    assert!(run.stderr.contains("`provider`"), "{}", run.stderr);
    assert!(run.stderr.contains("not-a-provider"), "{}", run.stderr);
}

#[test]
fn an_unreadable_backend_still_poisons_the_turn_when_no_provider_key_exists() {
    let sandbox = Sandbox::new();
    sandbox.write_user_settings("{\"backend\":\"definitely-not-a-backend\"}");
    let config =
        RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &sandbox.workspace).unwrap();
    let rejection = config.provider_rejected.expect("kept, never defaulted");
    assert_eq!(rejection.key, "backend");
}

#[test]
fn a_project_file_cannot_choose_the_provider_or_a_models_entry() {
    let sandbox = Sandbox::new();
    sandbox.write_project_settings("{\"provider\":\"llmux\",\"models\":{\"llmux\":\"whatever\"}}");
    let config =
        RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &sandbox.workspace).unwrap();
    assert_eq!(config.provider, ProviderId::Gateway);
    assert!(config.models.is_empty());

    let ignored: Vec<&str> = config
        .diagnostics
        .iter()
        .filter_map(|d| d.ignored_setting_key())
        .collect();
    assert!(ignored.contains(&"provider"), "got {ignored:?}");
    assert!(ignored.contains(&"models"), "got {ignored:?}");
}

#[test]
fn reading_a_profile_never_rewrites_it() {
    // A diagnostic command that edits the profile it is describing is the
    // opposite of the contract.
    let sandbox = Sandbox::new();
    let body =
        "{\"backend\":\"llmux\",\"llmux_url\":\"http://127.0.0.1:3456\",\"model\":\"fable\"}";
    sandbox.write_user_settings(body);
    let path = sandbox.profile_dir().join("settings.json");
    let before = fs::metadata(&path).unwrap().modified().unwrap();

    assert_eq!(sandbox.run(&["status", "--json"]).code, Some(0));
    assert_eq!(sandbox.run(&["doctor", "--json"]).code, Some(0));

    assert_eq!(fs::read_to_string(&path).unwrap(), body);
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), before);
}

#[test]
fn a_models_value_that_is_not_a_string_is_a_diagnostic_and_not_a_model() {
    let sandbox = Sandbox::new();
    sandbox.write_user_settings("{\"provider\":\"gateway\",\"models\":{\"gateway\":7}}");
    let config =
        RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &sandbox.workspace).unwrap();
    assert_eq!(config.model, xfx::config::DEFAULT_MODEL);
    assert!(config
        .diagnostics
        .iter()
        .any(|d| d.setting_key.as_deref() == Some("models")));
}

#[test]
fn a_newer_backend_overrides_an_older_provider() {
    // Layer order decides when keys are on different layers.
    // Global has provider=llmux, workspace has backend=gateway.
    // Workspace (newer) wins.
    let sandbox = Sandbox::new();
    sandbox.write_user_settings(&format!(
        "{{\"provider\":\"llmux\",\"llmux_url\":\"http://127.0.0.1:3456\",\
          \"workspaces\":{{{}:{{\"backend\":\"gateway\"}}}}}}",
        serde_json::to_string(&sandbox.workspace.display().to_string()).unwrap()
    ));
    let config =
        RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &sandbox.workspace).unwrap();
    assert_eq!(
        config.provider,
        ProviderId::Gateway,
        "workspace backend wins over global provider"
    );
}

#[test]
fn a_newer_rejected_backend_poisons_even_when_an_older_provider_is_readable() {
    // An unreadable newer value poisons over an older readable one.
    let sandbox = Sandbox::new();
    sandbox.write_user_settings(&format!(
        "{{\"provider\":\"llmux\",\"llmux_url\":\"http://127.0.0.1:3456\",\
          \"workspaces\":{{{}:{{\"backend\":\"not-a-backend\"}}}}}}",
        serde_json::to_string(&sandbox.workspace.display().to_string()).unwrap()
    ));
    let config =
        RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &sandbox.workspace).unwrap();
    let rejection = config.provider_rejected.expect("kept, never defaulted");
    assert_eq!(rejection.key, "backend", "newer rejected backend poisons");
    assert_eq!(rejection.value, "not-a-backend");
}

#[test]
fn a_higher_layer_flat_model_updates_the_models_entry() {
    // When a flat model from a higher layer outranks a lower-layer models entry,
    // it should update config.models[provider] to the effective value.
    let sandbox = Sandbox::new();
    sandbox.write_user_settings(&format!(
        "{{\"provider\":\"gateway\",\"models\":{{\"gateway\":\"global-model\"}},\
          \"workspaces\":{{{}:{{\"model\":\"workspace-model\"}}}}}}",
        serde_json::to_string(&sandbox.workspace.display().to_string()).unwrap()
    ));
    let config =
        RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &sandbox.workspace).unwrap();
    assert_eq!(config.model, "workspace-model");
    assert_eq!(
        config.models.get("gateway").map(String::as_str),
        Some("workspace-model"),
        "higher-layer flat model should update models[provider]"
    );
}

#[test]
fn disagreement_diagnostic_surfaces_in_doctor_json() {
    // Test that cross-layer conflict is reported end-to-end in doctor --json output.
    // Global: provider=llmux; Workspace: backend=gateway
    // Expected: workspace backend (newer) wins, but disagreement is diagnosed with both values
    let sandbox = Sandbox::new();
    sandbox.write_user_settings(&format!(
        "{{\"provider\":\"llmux\",\"llmux_url\":\"http://127.0.0.1:3456\",\
          \"workspaces\":{{{}:{{\"backend\":\"gateway\"}}}}}}",
        serde_json::to_string(&sandbox.workspace.display().to_string()).unwrap()
    ));
    let run = sandbox.run(&["doctor", "--json"]);
    assert_eq!(run.code, Some(0));

    let output: serde_json::Value =
        serde_json::from_str(&run.stdout).expect("doctor --json output is valid JSON");

    // Verify the winning provider is gateway (workspace backend)
    assert_eq!(
        output["provider"].as_str(),
        Some("gateway"),
        "workspace backend should win"
    );

    // Verify the conflict diagnostic contains both values and explains layer precedence
    let found_conflict = output["checks"]
        .as_array()
        .map(|checks| {
            checks.iter().any(|check| {
                let detail = check["detail"].as_str().unwrap_or("");
                detail.contains("conflicting_provider_selection")
                    && detail.contains("provider=llmux")
                    && detail.contains("backend=gateway")
                    && detail.contains("layer order decides")
            })
        })
        .unwrap_or(false);
    assert!(
        found_conflict,
        "cross-layer conflict should appear with both values and layer explanation, got: {}",
        run.stdout
    );
}

#[test]
fn a_cross_layer_disagreement_in_readable_values_is_diagnosed() {
    // Cross-layer disagreement (global provider vs workspace backend) must be visible.
    let sandbox = Sandbox::new();
    sandbox.write_user_settings(&format!(
        "{{\"provider\":\"llmux\",\"llmux_url\":\"http://127.0.0.1:3456\",\
          \"workspaces\":{{{}:{{\"backend\":\"gateway\"}}}}}}",
        serde_json::to_string(&sandbox.workspace.display().to_string()).unwrap()
    ));
    let config =
        RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &sandbox.workspace).unwrap();
    // Workspace backend wins (newer layer)
    assert_eq!(config.provider, ProviderId::Gateway);
    // But the disagreement must be diagnosed
    let conflict_diagnosed = config.diagnostics.iter().any(|d| {
        d.cause == DiagnosticCause::ConflictingProviderSelection
            && d.note
                .as_ref()
                .is_some_and(|n| n.contains("provider=llmux") && n.contains("backend=gateway"))
    });
    assert!(
        conflict_diagnosed,
        "cross-layer disagreement must be diagnosed; got: {:?}",
        config.diagnostics
    );
}

#[test]
fn provider_backend_cross_layer_matrix() {
    // Table-driven test: all combinations of {provider,backend} × {older,newer} × {readable,rejected}
    // Row: (older_key, older_state, newer_key, newer_state, expected_provider, should_diagnose)
    #[derive(Clone, Copy, Debug)]
    enum KeyState {
        Readable(&'static str),
        Rejected,
    }

    let test_cases = vec![
        // (older_layer_key, older_state, newer_layer_key, newer_state, expected_provider, should_diagnose_readable_conflict, description)
        // Cross-layer: Provider global (older), backend workspace (newer)
        (
            "provider",
            KeyState::Readable("llmux"),
            "backend",
            KeyState::Readable("gateway"),
            ProviderId::Gateway,
            true,
            "older readable provider vs newer readable backend → backend wins, diagnose",
        ),
        (
            "provider",
            KeyState::Readable("llmux"),
            "backend",
            KeyState::Rejected,
            ProviderId::default(),
            false,
            "older readable provider vs newer rejected backend → poisoned",
        ),
        (
            "provider",
            KeyState::Rejected,
            "backend",
            KeyState::Readable("gateway"),
            ProviderId::Gateway,
            false,
            "older rejected provider vs newer readable backend → backend wins",
        ),
        (
            "provider",
            KeyState::Rejected,
            "backend",
            KeyState::Rejected,
            ProviderId::default(),
            false,
            "older rejected provider vs newer rejected backend → poisoned by newer",
        ),
        // Cross-layer: Backend global (older), provider workspace (newer)
        (
            "backend",
            KeyState::Readable("gateway"),
            "provider",
            KeyState::Readable("llmux"),
            ProviderId::Llmux,
            true,
            "older readable backend vs newer readable provider → provider wins, diagnose",
        ),
        (
            "backend",
            KeyState::Readable("gateway"),
            "provider",
            KeyState::Rejected,
            ProviderId::default(),
            false,
            "older readable backend vs newer rejected provider → poisoned",
        ),
        (
            "backend",
            KeyState::Rejected,
            "provider",
            KeyState::Readable("llmux"),
            ProviderId::Llmux,
            false,
            "older rejected backend vs newer readable provider → provider wins",
        ),
        (
            "backend",
            KeyState::Rejected,
            "provider",
            KeyState::Rejected,
            ProviderId::default(),
            false,
            "older rejected backend vs newer rejected provider → poisoned by newer",
        ),
    ];

    for (older_key, older_state, newer_key, newer_state, expected, should_diagnose, desc) in
        test_cases
    {
        let sandbox = Sandbox::new();
        let workspace_workspace = sandbox.workspace.clone();

        // Build the settings JSON dynamically
        let mut global_json = serde_json::json!({});
        match older_key {
            "provider" => match older_state {
                KeyState::Readable(val) => {
                    global_json["provider"] = serde_json::json!(val);
                }
                KeyState::Rejected => {
                    global_json["provider"] = serde_json::json!("not-a-provider");
                }
            },
            "backend" => match older_state {
                KeyState::Readable(val) => {
                    global_json["backend"] = serde_json::json!(val);
                }
                KeyState::Rejected => {
                    global_json["backend"] = serde_json::json!("not-a-backend");
                }
            },
            _ => {}
        }
        global_json["llmux_url"] = serde_json::json!("http://127.0.0.1:3456");

        // Add workspace-level setting
        let mut workspace_json = serde_json::json!({});
        match newer_key {
            "provider" => match newer_state {
                KeyState::Readable(val) => {
                    workspace_json["provider"] = serde_json::json!(val);
                }
                KeyState::Rejected => {
                    workspace_json["provider"] = serde_json::json!("not-a-provider");
                }
            },
            "backend" => match newer_state {
                KeyState::Readable(val) => {
                    workspace_json["backend"] = serde_json::json!(val);
                }
                KeyState::Rejected => {
                    workspace_json["backend"] = serde_json::json!("not-a-backend");
                }
            },
            _ => {}
        }

        global_json["workspaces"] = serde_json::json!({
            workspace_workspace.display().to_string(): workspace_json
        });

        sandbox.write_user_settings(&global_json.to_string());

        let config = RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &sandbox.workspace)
            .unwrap_or_else(|_| panic!("config load for: {}", desc));

        // Assert provider outcome
        // Check if the newer state is rejected; if so, we expect poisoning
        let newer_is_rejected = matches!(newer_state, KeyState::Rejected);

        // Assert poisoning vs readable outcomes
        if newer_is_rejected {
            // Poisoned case: assert the newer key is stored with its raw value
            let rejection_msg = format!(
                "test case '{}': expected poisoning from newer rejected key but got none",
                desc
            );
            let rejection = config.provider_rejected.as_ref().expect(&rejection_msg);
            assert_eq!(
                rejection.key, newer_key,
                "test case '{}': poisoning should carry the newer key name",
                desc
            );
            // Check that the raw invalid value is stored
            let expected_value = match newer_key {
                "provider" => "not-a-provider",
                "backend" => "not-a-backend",
                _ => panic!("unknown key"),
            };
            assert_eq!(
                rejection.value, expected_value,
                "test case '{}': poisoning should carry the raw invalid value",
                desc
            );
        } else {
            // Readable winner: no poisoning
            assert!(
                config.provider_rejected.is_none(),
                "test case '{}': expected readable winner but got poisoned: {:?}",
                desc,
                config.provider_rejected
            );
            // Also verify the correct provider won
            assert_eq!(
                config.provider, expected,
                "test case '{}': expected provider {:?}, got {:?}",
                desc, expected, config.provider
            );
        }

        // Assert diagnostic presence/absence
        let has_readable_conflict = config
            .diagnostics
            .iter()
            .any(|d| d.cause == DiagnosticCause::ConflictingProviderSelection);

        if should_diagnose {
            assert!(
                has_readable_conflict,
                "test case '{}': should diagnose readable conflict but didn't. diagnostics: {:?}",
                desc, config.diagnostics
            );
        } else {
            assert!(
                !has_readable_conflict,
                "test case '{}': should NOT diagnose conflict but did. diagnostics: {:?}",
                desc, config.diagnostics
            );
        }
    }
}

#[test]
fn a_rejected_provider_from_newer_layer_poisons_at_binary_level() {
    // Test: newer workspace provider rejection refuses the turn, mirroring Task-1 poison pattern
    let sandbox = Sandbox::new();
    sandbox.write_user_settings(&format!(
        "{{\"backend\":\"gateway\",\"workspaces\":{{{}:{{\"provider\":\"not-a-provider\"}}}}}}",
        serde_json::to_string(&sandbox.workspace.display().to_string()).unwrap()
    ));
    let run = sandbox.run(&["ask", "--no-save", "hello"]);
    assert_eq!(run.code, Some(1));
    assert!(run.stderr.contains("`provider`"), "{}", run.stderr);
    assert!(run.stderr.contains("not-a-provider"), "{}", run.stderr);
    assert!(run.stderr.contains("provider=rejected"), "{}", run.stderr);
}

#[test]
fn a_rejected_backend_from_newer_layer_poisons_at_binary_level() {
    // Test: newer workspace backend rejection refuses the turn, mirroring Task-1 poison pattern
    let sandbox = Sandbox::new();
    sandbox.write_user_settings(&format!(
        "{{\"provider\":\"llmux\",\"llmux_url\":\"http://127.0.0.1:3456\",\"workspaces\":{{{}:{{\"backend\":\"not-a-backend\"}}}}}}",
        serde_json::to_string(&sandbox.workspace.display().to_string()).unwrap()
    ));
    let run = sandbox.run(&["ask", "--no-save", "hello"]);
    assert_eq!(run.code, Some(1));
    assert!(run.stderr.contains("`backend`"), "{}", run.stderr);
    assert!(run.stderr.contains("not-a-backend"), "{}", run.stderr);
    assert!(run.stderr.contains("provider=rejected"), "{}", run.stderr);
}

#[test]
fn a_blank_environment_override_does_not_displace_a_configured_value() {
    let sandbox = Sandbox::new();
    sandbox.write_user_settings("{\"model\":\"vendor/from-profile\"}");
    let config = RuntimeConfig::load_with(
        &environment(&sandbox.home, &[("XFX_MODEL", "   ")]),
        &sandbox.workspace,
    )
    .unwrap();
    assert_eq!(config.model, "vendor/from-profile");
    assert_eq!(config.sources.model, SettingSource::UserGlobal);
}

#[test]
fn an_oversized_settings_file_is_rejected_with_a_diagnostic() {
    let sandbox = Sandbox::new();
    let padding = "x".repeat(xfx::config::MAX_SETTINGS_BYTES + 1);
    sandbox.write_user_settings(&format!("{{\"model\":\"{padding}\"}}"));
    let config =
        RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &sandbox.workspace).unwrap();
    assert_eq!(config.model, xfx::config::DEFAULT_MODEL);
    assert!(
        config.diagnostics.iter().any(|d| d.is_too_large()),
        "an oversized layer must be reported, got {:?}",
        config.diagnostics
    );
}

#[test]
fn credentials_resolve_from_the_environment_without_exposing_bytes() {
    let sandbox = Sandbox::new();
    let config = RuntimeConfig::load_with(
        &environment(
            &sandbox.home,
            &[
                ("VERCEL_OIDC_TOKEN", " \n "),
                ("AI_GATEWAY_API_KEY", "gateway-secret"),
            ],
        ),
        &sandbox.workspace,
    )
    .unwrap();

    let credential = config.credential.as_ref().expect("credential resolved");
    assert_eq!(credential.source(), CredentialSource::AiGatewayApiKey);
    assert_eq!(credential.source_label(), "AI_GATEWAY_API_KEY");
    assert_eq!(credential.secret(), "gateway-secret");
    let rendered = format!("{credential:?}");
    assert!(
        !rendered.contains("gateway-secret"),
        "Debug must redact the secret, got {rendered}"
    );
}

#[test]
fn config_load_never_creates_the_profile_directory() {
    let sandbox = Sandbox::new();
    RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &sandbox.workspace).unwrap();
    assert!(!sandbox.profile_dir().exists());
}

// ---------------------------------------------------------------------------
// inventory reconciliation
// ---------------------------------------------------------------------------

#[test]
fn the_advertised_command_inventory_matches_the_parser() {
    let mut parsed = xfx::cli::parser_command_names();
    parsed.sort();
    let mut advertised: Vec<String> = xfx::cli::ADVERTISED_COMMANDS
        .iter()
        .map(|name| (*name).to_string())
        .collect();
    advertised.sort();
    assert_eq!(
        parsed, advertised,
        "ADVERTISED_COMMANDS must match the real parser surface"
    );
}

#[test]
fn every_advertised_command_has_an_implemented_parity_row() {
    let parity = fs::read_to_string(manifest_dir().join("docs/parity.md")).expect("read parity.md");
    for name in xfx::cli::ADVERTISED_COMMANDS {
        let row = parity
            .lines()
            .find(|line| line.starts_with(&format!("| `{name}` | command |")))
            .unwrap_or_else(|| panic!("docs/parity.md has no command row for `{name}`"));
        assert!(
            row.contains("| implemented |"),
            "`{name}` is advertised but its parity row is not implemented: {row}"
        );
    }
}

#[test]
fn every_deferred_command_row_stays_out_of_the_parser() {
    let parity = fs::read_to_string(manifest_dir().join("docs/parity.md")).expect("read parity.md");
    let advertised = xfx::cli::parser_command_names();
    for line in parity.lines() {
        if !line.contains("| command |") || !line.contains("| deferred |") {
            continue;
        }
        let name = line
            .split('`')
            .nth(1)
            .expect("a parity row names its surface in backticks");
        assert!(
            !advertised.iter().any(|command| command == name),
            "`{name}` is documented as deferred but the parser advertises it"
        );
    }
}

#[test]
fn every_upstream_command_appears_exactly_once_in_the_parity_ledger() {
    let parity = fs::read_to_string(manifest_dir().join("docs/parity.md")).expect("read parity.md");
    // The upstream command union, `cli_surface.zig:58-84`, minus the `unknown`
    // fallback which is a parse outcome rather than a command.
    let upstream = [
        "interactive",
        "help",
        "ask",
        "acp",
        "pr",
        "issue",
        "login",
        "logout",
        "setup",
        "status",
        "permissions",
        "models",
        "provider",
        "doctor",
        "background",
        "teams",
        "session",
        "sessions",
        "resume",
        "credits",
        "usage",
        "upgrade",
        "replay",
        "workspace",
    ];
    for name in upstream {
        let count = parity
            .lines()
            .filter(|line| line.starts_with(&format!("| `{name}` | command |")))
            .count();
        assert_eq!(count, 1, "`{name}` needs exactly one command parity row");
    }
}
