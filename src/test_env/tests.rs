//! A unit test cannot read the process environment it runs in.
//!
//! Each case below asks one function of xfx's that reads the environment, as
//! a case that forgot to give it its own would, and requires the refusal --
//! a panic naming the function -- or, for what is only handed on to a child,
//! that nothing of the real process arrived. None prints a value: a failure
//! says which function read, or which name a child was given, and no more.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;

use crate::permission::{CommandEffect, CommandPlan, DeniedEffect};
use crate::tools::ToolLimits;
use crate::workspace::AccessScope;

/// What `body` panicked with, or `None` when it returned.
fn panic_of(body: impl FnOnce()) -> Option<String> {
    let payload = catch_unwind(AssertUnwindSafe(body)).err()?;
    Some(
        payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|text| text.to_string()))
            .unwrap_or_default(),
    )
}

/// `body` is refused, by a panic that names `who`.
fn refused(who: &str, body: impl FnOnce()) {
    match panic_of(body) {
        Some(message) => assert!(
            message.contains(who) && message.contains("real process environment"),
            "{who} panicked, but not with the refusal: {message}"
        ),
        None => panic!("{who} read the real process environment in a unit test"),
    }
}

#[test]
fn the_environment_xfx_loads_with_is_refused() {
    refused("config::Environment::from_process", || {
        drop(crate::config::Environment::from_process());
    });
}

#[test]
fn the_gateway_endpoint_is_refused() {
    refused("gateway::Endpoint::from_process", || {
        drop(crate::gateway::Endpoint::from_process());
    });
}

#[test]
fn the_variables_that_choose_gits_configuration_are_refused() {
    refused("permission::GitEnvironment::from_process", || {
        drop(crate::permission::GitEnvironment::from_process());
    });
}

#[test]
fn the_link_bases_are_refused() {
    refused("permission::LinkBase::from_environment", || {
        drop(crate::permission::LinkBase::from_environment());
    });
}

#[test]
fn the_usage_clock_file_is_refused() {
    refused("usage::clock::now_ms", || {
        let _ = crate::usage::clock::now_ms();
    });
}

#[test]
fn whether_the_tui_runs_is_refused() {
    let cli = crate::cli::Cli {
        command: crate::cli::Command::Interactive { resume: None },
    };
    refused("tui::should_run", || {
        let _ = crate::tui::should_run(&cli);
    });
}

/// The real `git`, as the plan's own would be found.
fn git() -> std::path::PathBuf {
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|dir| dir.join("git"))
        .find(|git| git.is_file())
        .expect("git on PATH: this runs the real one")
}

/// A repository with one commit, made by a git that reads no configuration
/// but the repository's own.
fn a_repository(root: &Path) {
    for args in [
        &["init", "-q"][..],
        &[
            "-c",
            "user.email=a@b",
            "-c",
            "user.name=a",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "a",
        ][..],
    ] {
        let mut command = std::process::Command::new(git());
        command
            .args(args)
            .current_dir(root)
            .env_clear()
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        assert!(crate::process::status(&mut command)
            .expect("git runs")
            .success());
    }
}

/// A `git` the model asks for is planned by asking the user's git which hooks
/// its configuration sets (`super::super::permission::gitconfig`), under the
/// `HOME` the command would run with. In a unit test that `HOME` is not the
/// developer's: a hook configured there -- a sentinel the RED run planted --
/// does not turn this plan into a refusal.
#[test]
fn a_git_plan_reads_no_git_configuration_under_the_process_home() {
    let root = tempfile::tempdir().expect("a workspace");
    a_repository(root.path());
    let scope = AccessScope::primary_only(root.path()).expect("a scope");
    let plan =
        CommandPlan::prepare("git status", &scope, None, &ToolLimits::default()).expect("a plan");
    assert!(
        matches!(plan.effect(), CommandEffect::DirectReadOnly { .. }),
        "the plan was decided by the process HOME's git configuration (configured hooks: {})",
        matches!(
            plan.effect(),
            CommandEffect::Denied(DeniedEffect::ConfiguredHooks)
        )
    );
    let home = plan
        .environment()
        .iter()
        .find(|(name, _)| name == "HOME")
        .map(|(_, value)| value.as_str());
    assert!(
        home.is_some_and(str::is_empty),
        "the plan hands its command the process HOME"
    );
}

/// A child a unit test starts is given `PATH` and `TMPDIR` of the real
/// process and nothing else: no `HOME`, no xfx variable, no configuration.
/// Names only: the child prints its variables' names, never their values.
#[test]
fn a_child_inherits_nothing_of_the_process_but_path_and_tmpdir() {
    let mut command = crate::process::command("/bin/sh");
    command
        .args(["-c", "/usr/bin/env | /usr/bin/cut -d= -f1"])
        .stdin(std::process::Stdio::null());
    let output = crate::process::output(&mut command).expect("sh runs");
    assert!(output.status.success());
    let names: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_string)
        // `sh` sets these three itself, whatever it inherited.
        .filter(|name| !matches!(name.as_str(), "PWD" | "SHLVL" | "_"))
        .collect();
    let inherited = names
        .iter()
        .filter(|name| !matches!(name.as_str(), "PATH" | "TMPDIR"))
        .count();
    assert_eq!(
        inherited,
        0,
        "a child of a unit test inherited {inherited} more variables (HOME among them: {})",
        names.iter().any(|name| name == "HOME")
    );
}
