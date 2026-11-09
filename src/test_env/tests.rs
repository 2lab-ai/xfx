//! A unit test cannot read the process environment it runs in.
//!
//! Each case below asks one function of xfx's that reads the environment, as
//! a case that forgot to give it its own would, and requires the refusal --
//! a panic naming the function -- or, for what is only handed on to a child,
//! that nothing of the real process arrived. None prints a value: a failure
//! says which function read, or which name a child was given, and no more.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;

use super::FakeProcess;
use crate::config::Environment;
use crate::permission::{CommandEffect, CommandPlan, DeniedEffect, GitEnvironment};
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

/// A comparison of a value read from a (fake) process environment: on a
/// mismatch it says which variable, never what either side holds. If the
/// guard ever let a case read the real process, an `assert_eq!` here would
/// print the real value -- the leak this module exists to stop.
#[track_caller]
fn same(equal: bool, which: &str) {
    assert!(equal, "{which} is not what the case's process holds");
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
    std::env::split_paths(&super::path())
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
        let mut command = crate::process::command(git());
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

/// Whether `git config` finds a hook from a configuration file other than
/// the repository's own, run in `repo` with exactly `environment`.
fn a_hook_is_configured(repo: &Path, environment: &[(String, String)]) -> bool {
    let mut command = crate::process::command(git());
    command
        .args([
            "config",
            "--includes",
            "--get-regexp",
            r"^hook\..*\.command$",
        ])
        .current_dir(repo)
        .env_clear()
        .envs(environment.iter().map(|(name, value)| (name, value)))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    crate::process::status(&mut command)
        .expect("git runs")
        .success()
}

/// A git a unit test plans with no home of its own excludes the global
/// configuration, rather than counting on there being none where its empty
/// `HOME` points: whatever `HOME` git ends up with, a hook a global file
/// there sets -- a sentinel this case writes, in `~/.gitconfig` and in
/// `~/.config/git/config` -- is not read. Asked of git itself, under the
/// plan's environment with only `HOME` changed.
#[test]
fn a_git_planned_with_no_home_reads_no_global_configuration_wherever_home_is() {
    let root = tempfile::tempdir().expect("a workspace");
    a_repository(root.path());
    let sentinel = tempfile::tempdir().expect("a sentinel home");
    std::fs::create_dir_all(sentinel.path().join(".config/git")).expect("an XDG git dir");
    for file in [".gitconfig", ".config/git/config"] {
        std::fs::write(
            sentinel.path().join(file),
            "[hook \"sentinel\"]\n\tcommand = true\n\tevent = pre-commit\n",
        )
        .expect("a sentinel global configuration");
    }
    let scope = AccessScope::primary_only(root.path()).expect("a scope");
    let plan =
        CommandPlan::prepare("git status", &scope, None, &ToolLimits::default()).expect("a plan");
    let home = sentinel.path().display().to_string();
    let environment: Vec<(String, String)> = plan
        .environment()
        .iter()
        .map(|(name, value)| match name.as_str() {
            "HOME" => (name.clone(), home.clone()),
            _ => (name.clone(), value.clone()),
        })
        .collect();
    // The control: the sentinel is what git reads under that home when
    // nothing excludes it.
    let control = vec![
        ("HOME".to_string(), home.clone()),
        (
            "PATH".to_string(),
            super::path().to_string_lossy().into_owned(),
        ),
    ];
    assert!(
        a_hook_is_configured(root.path(), &control),
        "the control did not read the sentinel global configuration"
    );
    assert!(
        !a_hook_is_configured(root.path(), &environment),
        "a git planned with no home read a global configuration file"
    );
}

/// A child a unit test starts that builds a client for an https endpoint
/// takes no proxy from the process it runs in: the copy below is started
/// with `HTTPS_PROXY` and `ALL_PROXY` naming a loopback proxy and no fake
/// process, as a developer's shell may name one, and the proxy must receive
/// nothing. (The copy's own environment is one this case built, never the
/// developer's.)
#[test]
fn an_https_client_takes_no_proxy_from_the_process() {
    const COPY: &str = "XFX_TEST_ENV_PROXY_COPY";
    #[allow(
        clippy::disallowed_methods,
        reason = "env: XFX_TEST_ENV_PROXY_COPY -- the role this case's copy is started with"
    )]
    let copy = std::env::var_os(COPY);
    if let Some(target) = copy {
        let target = target.to_string_lossy().into_owned();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime");
        runtime.block_on(async {
            let client = crate::gateway::bearer_rules(reqwest::Client::builder(), &target)
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .expect("a client");
            // Failed, as it must be: the target speaks no TLS. What matters
            // is where it was sent.
            let _ = client.get(&target).send().await;
        });
        return;
    }
    let proxy = crate::test_net::Listener::answering(
        "HTTP/1.1 502 Bad Gateway\r\ncontent-length: 0\r\nconnection: close\r\n\r\n".to_string(),
    );
    let target = crate::test_net::Listener::answering(String::new());
    let https = target.origin().replacen("http://", "https://", 1);
    let mut copy = crate::process::command(std::env::current_exe().expect("this test binary"));
    for variable in ["HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy"] {
        copy.env(variable, proxy.origin());
    }
    copy.env(COPY, format!("{https}/v1/models"));
    copy.args([
        "--exact",
        "--test-threads=1",
        "test_env::tests::an_https_client_takes_no_proxy_from_the_process",
    ]);
    let output = crate::process::output(&mut copy).expect("a copy of this test binary");
    let reached = proxy.finish();
    drop(target.finish());
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("test result: ok. 1 passed"),
        "the copy did not run the case"
    );
    assert!(
        reached.is_empty(),
        "an https client a unit test built sent {} request(s) to the process's proxy",
        reached.len()
    );
}

/// The names of the variables a child of `crate::process::command` starts
/// with: `env` itself, no shell, so they are exactly what it was given. Each
/// value is dropped as it is read and none is printed.
fn child_names() -> Vec<String> {
    let mut command = crate::process::command("/usr/bin/env");
    command.stdin(std::process::Stdio::null());
    let output = crate::process::output(&mut command).expect("env runs");
    assert!(output.status.success());
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split_once('=').map(|(name, _)| name.to_string()))
        .collect()
}

/// A child a unit test starts is given `PATH` and `TMPDIR` of the real
/// process and nothing else: no `HOME`, no xfx variable, no configuration.
#[test]
fn a_child_inherits_nothing_of_the_process_but_path_and_tmpdir() {
    let names = child_names();
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

// ---------------------------------------------------------------------------
// the way in: a case's own fake process
// ---------------------------------------------------------------------------

#[test]
fn inside_a_fake_process_each_reader_takes_the_fake_and_only_its_own_names() {
    let home = tempfile::tempdir().expect("a home");
    FakeProcess::new()
        .home(home.path())
        .var("XFX_MODEL", "xfx-fake-model")
        .var("AI_GATEWAY_API_KEY", "xfx-fake-gateway-key")
        .var("XFX_GATEWAY_URL", "https://fake.invalid/v1")
        .var("GIT_CONFIG_GLOBAL", "/dev/null")
        .var("XFX_FAKE_NOT_READ", "x")
        .run(|| {
            let env = Environment::from_process();
            same(env.home() == Some(home.path()), "HOME");
            same(env.var("XFX_MODEL") == Some("xfx-fake-model"), "XFX_MODEL");
            same(
                env.var("AI_GATEWAY_API_KEY") == Some("xfx-fake-gateway-key"),
                "AI_GATEWAY_API_KEY",
            );
            same(env.var("XFX_FAKE_NOT_READ").is_none(), "XFX_FAKE_NOT_READ");
            same(
                crate::gateway::Endpoint::from_process()
                    .is_ok_and(|endpoint| endpoint.url() == "https://fake.invalid/v1"),
                "XFX_GATEWAY_URL",
            );
            same(
                GitEnvironment::from_process()
                    == GitEnvironment::default().with("GIT_CONFIG_GLOBAL", "/dev/null"),
                "GIT_CONFIG_GLOBAL",
            );
        });
}

#[test]
fn a_fake_process_ends_with_its_body_however_the_body_ends() {
    let model = || {
        Environment::from_process()
            .var("XFX_MODEL")
            .map(str::to_string)
    };
    FakeProcess::new().var("XFX_MODEL", "outer").run(|| {
        FakeProcess::new()
            .var("XFX_MODEL", "inner")
            .run(|| same(model().as_deref() == Some("inner"), "the inner XFX_MODEL"));
        same(model().as_deref() == Some("outer"), "the outer XFX_MODEL");
    });
    assert!(panic_of(|| FakeProcess::new().run(|| panic!("the body"))).is_some());
    refused("config::Environment::from_process", || {
        drop(Environment::from_process());
    });
}

/// `tui::signals::spawn_unsignalled` hands its body to [`super::carried`]:
/// the TUI's own threads are in the process of the case that started them.
#[test]
fn a_carried_body_runs_in_the_process_of_the_thread_that_made_it() {
    let carried = FakeProcess::new().var("XFX_MODEL", "carried").run(|| {
        super::carried(|| {
            Environment::from_process()
                .var("XFX_MODEL")
                .map(str::to_string)
        })
    });
    let seen = std::thread::spawn(carried).join().expect("joined");
    same(seen.as_deref() == Some("carried"), "the carried XFX_MODEL");
    let outside = super::carried(|| panic_of(|| drop(Environment::from_process())));
    let refusal = std::thread::spawn(outside).join().expect("joined");
    assert!(
        refusal.is_some_and(|message| message.contains("config::Environment::from_process")),
        "a body made outside a fake process read on its thread"
    );
}

/// An MCP runtime's lookup is asked on threads of its own: it is the fake
/// process of the thread that made it, wherever it is asked.
#[test]
fn a_lookup_is_the_process_it_was_made_in_on_any_thread() {
    let lookup = FakeProcess::new()
        .var("XFX_FAKE", "made")
        .run(|| super::lookup("test_env::tests::made"));
    let answer = std::thread::spawn(move || (lookup("XFX_FAKE"), lookup("XFX_FAKE_UNSET")))
        .join()
        .expect("joined");
    same(answer.0.as_deref() == Some("made"), "XFX_FAKE");
    same(answer.1.is_none(), "XFX_FAKE_UNSET");
    let outside = super::lookup("test_env::tests::outside");
    refused("test_env::tests::outside", || drop(outside("XFX_FAKE")));
}

#[test]
fn an_mcp_runtime_made_outside_a_fake_process_refuses_its_lookup() {
    let paths = || crate::mcp::Paths {
        profile: None,
        settings: None,
        workspace: std::path::PathBuf::from("/nonexistent-xfx-test-workspace"),
    };
    // Made: nothing is asked yet. Asked: refused.
    let runtime = crate::mcp::McpRuntime::new(paths());
    refused("mcp::McpRuntime::new", || {
        drop(runtime.looked_up("XFX_FAKE"))
    });
    let runtime = FakeProcess::new()
        .var("XFX_FAKE", "fake")
        .run(|| crate::mcp::McpRuntime::new(paths()));
    same(
        runtime.looked_up("XFX_FAKE").as_deref() == Some("fake"),
        "XFX_FAKE",
    );
    let seam = crate::mcp::McpRuntime::with_environment(paths(), &[("XFX_GIVEN", "given")]);
    same(
        seam.looked_up("XFX_GIVEN").as_deref() == Some("given"),
        "XFX_GIVEN",
    );
    refused("mcp::McpRuntime::with_environment", || {
        drop(seam.looked_up("XFX_FAKE"));
    });
}

#[test]
fn what_is_only_handed_to_a_child_is_the_fakes_or_nothing() {
    for name in [
        "HOME",
        "XFX_MODEL",
        "AI_GATEWAY_API_KEY",
        "XDG_CONFIG_HOME",
        "USER",
    ] {
        same(super::child_var_os(name).is_none(), name);
    }
    assert!(super::child_var_os("PATH").is_some());
    FakeProcess::new().home("/fake/home").run(|| {
        same(
            super::child_var_os("HOME") == Some(OsString::from("/fake/home")),
            "HOME",
        );
        same(
            super::child_var("USER") == Err(std::env::VarError::NotPresent),
            "USER",
        );
        assert!(super::child_var_os("PATH").is_some());
    });
}

/// A child inherits the fake process as it would the real one -- less
/// xfx's own credential, which `crate::process::command` still removes.
#[test]
fn a_child_inherits_the_fake_process_less_xfxs_own_credential() {
    let home = tempfile::tempdir().expect("a home");
    let names: BTreeSet<String> = FakeProcess::new()
        .home(home.path())
        .var("XFX_FAKE_INHERITED", "1")
        .var("AI_GATEWAY_API_KEY", "xfx-fake-gateway-key")
        .var("VERCEL_OIDC_TOKEN", "xfx-fake-oidc-token")
        .run(child_names)
        .into_iter()
        .collect();
    for name in ["HOME", "PATH", "XFX_FAKE_INHERITED"] {
        assert!(names.contains(name), "{name} was not inherited: {names:?}");
    }
    for name in ["AI_GATEWAY_API_KEY", "VERCEL_OIDC_TOKEN"] {
        assert!(!names.contains(name), "{name} was inherited: {names:?}");
    }
}

/// The opt-in reaches git: a hook the fake home's configuration sets turns
/// the plan into a refusal, and the plan's command runs under that home.
#[test]
fn a_git_plan_reads_the_git_configuration_of_the_fake_home() {
    let root = tempfile::tempdir().expect("a workspace");
    a_repository(root.path());
    let home = tempfile::tempdir().expect("a home");
    std::fs::write(
        home.path().join(".gitconfig"),
        "[hook \"fake\"]\n\tcommand = true\n",
    )
    .expect("a global configuration");
    let scope = AccessScope::primary_only(root.path()).expect("a scope");
    let plan = FakeProcess::new().home(home.path()).run(|| {
        CommandPlan::prepare("git status", &scope, None, &ToolLimits::default()).expect("a plan")
    });
    assert!(matches!(
        plan.effect(),
        CommandEffect::Denied(DeniedEffect::ConfiguredHooks)
    ));
    assert!(plan
        .environment()
        .iter()
        .any(|(name, value)| name == "HOME" && Path::new(value) == home.path()));
}

// ---------------------------------------------------------------------------
// the rest of the crate: clippy's environment rule, and its exceptions
// ---------------------------------------------------------------------------

/// The readers of the process environment `clippy.toml` disallows: checked
/// on the resolved call, so an imported `var` or an aliased `std::env` is
/// caught as the plain path is.
const ENV_READERS: [&str; 6] = [
    "std::env::var",
    "std::env::var_os",
    "std::env::vars",
    "std::env::vars_os",
    "std::env::home_dir",
    "libc::getenv",
];

/// Every read of the process environment that stays outside `process_env!`,
/// by file and by the variables its statement reads -- one entry per
/// `#[allow(clippy::disallowed_methods, reason = "env: ...")]`. A read added,
/// moved, or changed to read another variable fails
/// [`every_read_outside_process_env_is_bound_to_its_variables`]. None names
/// the person's home, profile, configuration, an xfx setting or a credential
/// ([`GUARDED`]).
const EXCEPTIONS: &[(&str, &[&str])] = &[
    // `process_env!`'s build arms: `std::env` itself, in a build only.
    ("src/lib.rs", &["process_env!"]),
    ("src/lib.rs", &["process_env!"]),
    ("src/lib.rs", &["process_env!"]),
    ("src/lib.rs", &["process_env!"]),
    ("src/lib.rs", &["process_env!"]),
    // Where programs are and where temporary files go.
    ("src/test_env.rs", &["PATH", "TMPDIR"]),
    ("src/permission/authority.rs", &["PATH"]),
    ("src/permission/gitconfig.rs", &["PATH"]),
    ("src/interactive.rs", &["TMPDIR"]),
    ("src/tui/shell.rs", &["TMPDIR"]),
    ("src/mcp/docker.rs", &["TMPDIR", "TEMP", "TMP"]),
    ("tests/support/sandbox.rs", &["PATH", "TMPDIR"]),
    ("tests/cli.rs", &["PATH", "TMPDIR"]),
    // What the terminal says of itself.
    ("src/tui/term.rs", &["TMUX"]),
    ("src/tui/mod.rs", &["COLORFGBG"]),
    ("src/tui/mod.rs", &["COLORTERM"]),
    ("src/tui/mod.rs", &["TERM_PROGRAM"]),
    // A fault-injection point, and the roles a re-run test binary is given.
    ("src/tui/fault.rs", &["XFX_TUI_FAULT"]),
    (
        "src/usage/cache.rs",
        &["XFX_TEST_USAGE_CACHE_KILL_BEFORE_RENAME"],
    ),
    ("src/process.rs", &["XFX_TEST_SIGCHLD_PROBE"]),
    ("src/test_net.rs", &["XFX_TEST_PROXY_CASE_SERVER"]),
    ("src/tui/signals.rs", &["XFX_TEST_IGNORED_AT_ENTRY"]),
    (
        "src/profile_home/tests.rs",
        &["XFX_TEST_PROFILE_HOME_UNDER_OPEN_UMASK"],
    ),
    (
        "src/session/store/fork/tests.rs",
        &["XFX_TEST_UNDER_OPEN_UMASK"],
    ),
    ("src/local_time/tests.rs", &["XFX_TEST_LOCAL_TIME_MOMENTS"]),
    ("src/usage/cache/tests.rs", &["XFX_TEST_USAGE_CACHE_ROLE"]),
    (
        "src/usage/cache/tests.rs",
        &["XFX_TEST_USAGE_CACHE_HOME", "XFX_TEST_USAGE_CACHE_ARG"],
    ),
    ("src/mcp/transport.rs", &["XFX_TEST_SENTINEL_RUNNER"]),
    ("src/mcp/transport.rs", &["XFX_TEST_SENTINEL_RUNNER"]),
    ("src/test_env/tests.rs", &["XFX_TEST_ENV_PROXY_COPY"]),
    // An ignored case the drawn-wider sweep runs, with its own two.
    (
        "src/tui/drawn_wider.rs",
        &["XFX_DRAWN_WIDER_LIST", "XFX_DRAWN_WIDER_OUT"],
    ),
    // Set by cargo in a test process: the case shows a command does not
    // inherit it.
    ("tests/permissions.rs", &["CARGO_PKG_NAME"]),
];

/// What `process_env!` guards, and what a proxy is named by: no exception
/// may read one.
const GUARDED: &[&str] = &[
    "HOME",
    "XDG_CONFIG_HOME",
    "ZDOTDIR",
    "LLMUX_CONFIG",
    "AI_GATEWAY_API_KEY",
    "VERCEL_OIDC_TOKEN",
    "XFX_MODEL",
    "XFX_PERMISSION_MODE",
    "XFX_MAX_AGENT_STEPS",
    "XFX_SOUND",
    "XFX_SKILL_SYMLINK_AUTHORITIES",
    "XFX_GATEWAY_URL",
    "XFX_TUI",
    "XFX_THEME",
    "XFX_E2E_USAGE_CLOCK_FILE",
    "GIT_CONFIG_GLOBAL",
    "GIT_CONFIG_NOSYSTEM",
    "GIT_CONFIG_SYSTEM",
    "USER",
    "LOGNAME",
    "SHELL",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "NO_PROXY",
];

/// The lint an exception allows, spelled in halves so that this file does not
/// read as lifting it to `process::tests::no_child_process_is_made_any_other_way`.
const DISALLOWED: &str = concat!("clippy::disallowed", "_methods");

/// What starts a child or makes a pipe: an `env:` allow lifts clippy's start
/// rule too, so its statement may hold none of these.
const STARTS: [&str; 7] = [
    "Command",
    "spawn",
    "fork",
    "pipe",
    "socketpair",
    "popen",
    "libc::system",
];

/// Every `.rs` file under `dir`, sorted.
fn sources(dir: &Path, found: &mut Vec<std::path::PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .expect("a source directory")
        .map(|entry| entry.expect("an entry").path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            sources(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
}

/// `text` with every string literal's contents and every comment blanked,
/// and the literals, in order: what a statement says, apart from what it
/// quotes.
fn code_and_literals(text: &str) -> (String, Vec<String>) {
    let mut code = String::new();
    let mut literals = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                let mut literal = String::new();
                while let Some(c) = chars.next() {
                    match c {
                        '\\' => {
                            chars.next();
                        }
                        '"' => break,
                        c => literal.push(c),
                    }
                }
                literals.push(literal);
                code.push_str("\"\"");
            }
            '/' if chars.peek() == Some(&'/') => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        code.push('\n');
                        break;
                    }
                }
            }
            c => code.push(c),
        }
    }
    (code, literals)
}

/// What the attribute that ends on line `end` (0-based) is on: a statement, a
/// match arm or an item -- up to the first `;`, `,` or `}` that closes it at
/// depth 0.
fn statement_after(lines: &[&str], end: usize) -> String {
    let mut statement = String::new();
    let mut depth = 0i32;
    for line in &lines[end + 1..] {
        statement.push_str(line);
        statement.push('\n');
        let (code, _) = code_and_literals(line);
        for c in code.chars() {
            match c {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth -= 1,
                _ => {}
            }
        }
        if depth <= 0 && code.trim_end().ends_with([';', ',', '}']) {
            break;
        }
    }
    statement
}

/// The value of `const NAME: &str = "..."`, from `text` if it is defined
/// there, or else from the one other file in `others` that defines it.
fn constant(name: &str, text: &str, others: &[(String, String)]) -> Option<String> {
    let find = |text: &str| {
        text.lines().find_map(|line| {
            let (code, literals) = code_and_literals(line);
            let at = code.find(&format!("const {name}: &str = \"\""))?;
            (code[..at].trim().is_empty() || code[..at].trim().starts_with("pub"))
                .then(|| literals.first().cloned())
                .flatten()
        })
    };
    find(text).or_else(|| {
        let mut found = others.iter().filter_map(|(_, text)| find(text));
        let first = found.next()?;
        found.next().is_none().then_some(first)
    })
}

/// The reads in `text` (the file `file`) allowed past clippy's environment
/// rule, each as the variables its `env:` reason names; and what is wrong
/// with any of them: a reason that is not the statement's variables, a
/// statement that reads nothing, or one that starts a child.
fn bound_reads(
    file: &str,
    text: &str,
    others: &[(String, String)],
) -> (Vec<(String, Vec<String>)>, Vec<String>) {
    let lines: Vec<&str> = text.lines().collect();
    let mut reads = Vec::new();
    let mut problems = Vec::new();
    for (number, line) in lines.iter().enumerate() {
        if line.trim_start().starts_with("//") || !line.contains(DISALLOWED) {
            continue;
        }
        let Some(end) = (number..lines.len().min(number + 4)).find(|at| lines[*at].contains(")]"))
        else {
            continue;
        };
        let attribute = lines[number..=end].join(" ");
        let Some(reason) = attribute
            .split_once("reason = \"env: ")
            .and_then(|(_, rest)| rest.split_once('"'))
            .map(|(reason, _)| reason)
        else {
            continue;
        };
        let at = format!("{file}:{}", number + 1);
        let named: Vec<String> = reason
            .split_once(" -- ")
            .map_or(reason, |(names, _)| names)
            .split(", ")
            .map(str::to_string)
            .collect();
        let statement = statement_after(&lines, end);
        let (code, literals) = code_and_literals(&statement);
        if !code.contains("std::env::") {
            problems.push(format!(
                "{at}: an env: allow over a statement that reads nothing"
            ));
        }
        if let Some(start) = STARTS.iter().find(|start| code.contains(*start)) {
            problems.push(format!(
                "{at}: an env: allow over a statement with `{start}`"
            ));
        }
        let shaped = |name: &str| {
            !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
                && name.starts_with(|c: char| c.is_ascii_uppercase())
        };
        let mut read: Vec<String> = literals.into_iter().filter(|name| shaped(name)).collect();
        for token in code.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
            if shaped(token) {
                if let Some(value) = constant(token, text, others) {
                    read.push(value);
                }
            }
        }
        read.sort();
        read.dedup();
        let mut said = named.clone();
        said.sort();
        if said != ["process_env!"] && read != said {
            problems.push(format!(
                "{at}: the allow names {said:?}, the statement reads {read:?}"
            ));
        }
        if said == ["process_env!"] && !(file == "src/lib.rs" && read.is_empty()) {
            problems.push(format!("{at}: process_env!'s allow outside its build arms"));
        }
        reads.push((file.to_string(), named));
    }
    (reads, problems)
}

#[test]
fn clippy_disallows_every_reader_of_the_environment() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let rules = std::fs::read_to_string(manifest.join("clippy.toml")).expect("clippy.toml");
    for path in ENV_READERS {
        let entry = format!("{{ path = \"{path}\", ");
        assert!(
            rules
                .lines()
                .any(|line| line.trim_start().starts_with(&entry)),
            "clippy.toml no longer disallows {path}"
        );
    }
}

#[test]
fn every_read_outside_process_env_is_bound_to_its_variables() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    sources(&manifest.join("src"), &mut files);
    sources(&manifest.join("tests"), &mut files);
    let texts: Vec<(String, String)> = files
        .iter()
        .map(|path| {
            (
                path.strip_prefix(manifest)
                    .expect("under the manifest")
                    .to_string_lossy()
                    .into_owned(),
                std::fs::read_to_string(path).expect("a source file"),
            )
        })
        .collect();
    let mut found = Vec::new();
    let mut problems = Vec::new();
    for (file, text) in &texts {
        let (reads, wrong) = bound_reads(file, text, &texts);
        found.extend(reads);
        problems.extend(wrong);
    }
    let mut listed: Vec<(String, Vec<String>)> = EXCEPTIONS
        .iter()
        .map(|(file, names)| {
            (
                file.to_string(),
                names.iter().map(|name| name.to_string()).collect(),
            )
        })
        .collect();
    for (file, names) in &found {
        match listed
            .iter()
            .position(|entry| entry == &(file.clone(), names.clone()))
        {
            Some(at) => drop(listed.remove(at)),
            None => problems.push(format!("{file}: an unlisted read of {names:?}")),
        }
    }
    for (file, names) in listed {
        problems.push(format!("{file}: a listed read of {names:?} is not there"));
    }
    for (file, names) in EXCEPTIONS {
        for name in *names {
            if GUARDED.contains(name) {
                problems.push(format!(
                    "{file}: an exception reads {name}, which is guarded"
                ));
            }
        }
    }
    assert!(
        problems.is_empty(),
        "a read of the process environment outside process_env!:\n{}",
        problems.join("\n")
    );
}

/// The check's own fixtures: a swapped variable, an allow over a start, an
/// allow over nothing, and a constant whose value moved.
#[test]
fn the_binding_check_refuses_a_swapped_variable_a_start_and_a_moved_constant() {
    let allowed = |statement: &str| {
        format!(
            "fn f() {{\n    #[allow(\n        {DISALLOWED},\n        \
             reason = \"env: TMUX -- the terminal\"\n    )]\n    {statement}\n}}\n"
        )
    };
    let problems = |text: &str| bound_reads("src/fixture.rs", text, &[]).1;
    assert!(problems(&allowed(r#"let tmux = std::env::var_os("TMUX");"#)).is_empty());
    let swapped = problems(&allowed(r#"let tmux = std::env::var_os("HOME");"#));
    assert!(
        swapped
            .iter()
            .any(|problem| problem.contains("reads [\"HOME\"]")),
        "{swapped:?}"
    );
    let started = problems(&allowed(
        r#"let tmux = (std::env::var_os("TMUX"), Command::spawn);"#,
    ));
    assert!(
        started.iter().any(|problem| problem.contains("`Command`")),
        "{started:?}"
    );
    let nothing = problems(&allowed(r#"let tmux = "TMUX";"#));
    assert!(
        nothing
            .iter()
            .any(|problem| problem.contains("reads nothing")),
        "{nothing:?}"
    );
    let constant = |value: &str| {
        format!(
            "const NAME: &str = \"{value}\";\n{}",
            allowed("let tmux = std::env::var_os(NAME);")
        )
    };
    assert!(problems(&constant("TMUX")).is_empty());
    let moved = problems(&constant("HOME"));
    assert!(
        moved
            .iter()
            .any(|problem| problem.contains("reads [\"HOME\"]")),
        "{moved:?}"
    );
}

/// A runtime's environment laid over a case's fake process: what the
/// environment holds wins, the fake fills the rest; with no fake on the
/// thread, the environment alone is the process.
#[test]
fn an_environment_laid_over_a_fake_process_wins_where_it_holds_and_no_further() {
    let under = FakeProcess::new()
        .home("/fake/under")
        .var("XFX_MODEL", "under")
        .var("XFX_GATEWAY_URL", "https://under.invalid/v1");
    let over = FakeProcess::new()
        .home("/fake/over")
        .var("XFX_MODEL", "over");
    under.run(|| {
        over.clone().over_this_thread();
        let env = Environment::from_process();
        same(env.home() == Some(Path::new("/fake/over")), "HOME");
        same(env.var("XFX_MODEL") == Some("over"), "XFX_MODEL");
        same(
            crate::gateway::Endpoint::from_process()
                .is_ok_and(|endpoint| endpoint.url() == "https://under.invalid/v1"),
            "XFX_GATEWAY_URL",
        );
    });
    let alone = std::thread::spawn(move || {
        over.over_this_thread();
        let env = Environment::from_process();
        (
            env.home() == Some(Path::new("/fake/over")),
            crate::gateway::Endpoint::from_process()
                .is_ok_and(|endpoint| endpoint.url() == crate::gateway::DEFAULT_CHAT_URL),
        )
    })
    .join()
    .expect("joined");
    same(alone.0, "HOME, laid on a thread with no fake process");
    same(
        alone.1,
        "XFX_GATEWAY_URL, laid on a thread with no fake process",
    );
}

/// Sends one request from an https client built under `bearer_rules` in
/// `process`, and says how many requests a loopback proxy received.
fn requests_to_a_proxy(process: impl FnOnce(&str) -> FakeProcess) -> usize {
    let proxy = crate::test_net::Listener::answering(
        "HTTP/1.1 502 Bad Gateway\r\ncontent-length: 0\r\nconnection: close\r\n\r\n".to_string(),
    );
    let target = crate::test_net::Listener::answering(String::new());
    let https = target.origin().replacen("http://", "https://", 1);
    process(&proxy.origin()).run(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime")
            .block_on(async {
                let client = crate::gateway::bearer_rules(reqwest::Client::builder(), &https)
                    .timeout(std::time::Duration::from_secs(5))
                    .build()
                    .expect("a client");
                let _ = client.get(&https).send().await;
            });
    });
    drop(target.finish());
    proxy.finish().len()
}

/// The way a case gives a client a proxy: its fake process's `HTTPS_PROXY`,
/// which `NO_PROXY` there still exempts a host from.
#[test]
fn an_https_client_takes_the_fake_processs_proxy_and_its_exemptions() {
    assert_eq!(
        requests_to_a_proxy(|proxy| FakeProcess::new().var("HTTPS_PROXY", proxy)),
        1,
        "the fake process's HTTPS_PROXY was not taken"
    );
    assert_eq!(
        requests_to_a_proxy(|proxy| FakeProcess::new()
            .var("HTTPS_PROXY", proxy)
            .var("NO_PROXY", "127.0.0.1")),
        0,
        "the fake process's NO_PROXY was not honoured"
    );
    assert_eq!(
        requests_to_a_proxy(|_| FakeProcess::new()),
        0,
        "a client took a proxy the fake process does not name"
    );
}
