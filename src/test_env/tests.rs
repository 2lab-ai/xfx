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
            std::env::var("PATH").unwrap_or_default(),
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
    if let Some(target) = std::env::var_os(COPY) {
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
            assert_eq!(env.home(), Some(home.path()));
            assert_eq!(env.var("XFX_MODEL"), Some("xfx-fake-model"));
            assert_eq!(env.var("AI_GATEWAY_API_KEY"), Some("xfx-fake-gateway-key"));
            assert_eq!(env.var("XFX_FAKE_NOT_READ"), None);
            assert_eq!(
                crate::gateway::Endpoint::from_process()
                    .expect("an endpoint")
                    .url(),
                "https://fake.invalid/v1"
            );
            assert_eq!(
                GitEnvironment::from_process(),
                GitEnvironment::default().with("GIT_CONFIG_GLOBAL", "/dev/null")
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
            .run(|| assert_eq!(model().as_deref(), Some("inner")));
        assert_eq!(model().as_deref(), Some("outer"));
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
    assert_eq!(seen.as_deref(), Some("carried"));
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
    assert_eq!(answer, (Some("made".to_string()), None));
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
    assert_eq!(runtime.looked_up("XFX_FAKE").as_deref(), Some("fake"));
    let seam = crate::mcp::McpRuntime::with_environment(paths(), &[("XFX_GIVEN", "given")]);
    assert_eq!(seam.looked_up("XFX_GIVEN").as_deref(), Some("given"));
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
        assert_eq!(super::child_var_os(name), None, "{name}");
    }
    assert!(super::child_var_os("PATH").is_some());
    FakeProcess::new().home("/fake/home").run(|| {
        assert_eq!(
            super::child_var_os("HOME"),
            Some(OsString::from("/fake/home"))
        );
        assert_eq!(
            super::child_var("USER"),
            Err(std::env::VarError::NotPresent)
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
// the rest of `src`
// ---------------------------------------------------------------------------

/// What reads the process environment without `process_env!`, by file, and
/// why each may. A read added anywhere else -- or one more in a listed file --
/// fails this case until it goes through `process_env!` or is listed here
/// with its reason. None of these names the developer's home, profile,
/// configuration, an xfx setting or a credential.
const LISTED: &[(&str, usize, &str)] = &[
    (
        "src/lib.rs",
        5,
        "process_env!'s build arms: std::env itself",
    ),
    (
        "src/test_env.rs",
        2,
        "PATH and TMPDIR, the two a child keeps",
    ),
    ("src/test_env/tests.rs", 1, "PATH, to find git"),
    (
        "src/interactive.rs",
        1,
        "TMPDIR: where /trace writes its report",
    ),
    (
        "src/tui/shell.rs",
        1,
        "TMPDIR: where /trace writes its report",
    ),
    (
        "src/mcp/docker.rs",
        1,
        "TMPDIR, TEMP or TMP: where a cidfile goes",
    ),
    ("src/tui/term.rs", 1, "TMUX: whether the terminal is tmux's"),
    (
        "src/tui/mod.rs",
        3,
        "COLORFGBG, COLORTERM, TERM_PROGRAM: the terminal's own",
    ),
    (
        "src/permission/authority.rs",
        5,
        "PATH: one for a command, four in tests",
    ),
    (
        "src/permission/gitconfig.rs",
        2,
        "PATH: finding git, once in a test",
    ),
    (
        "src/mcp/transport.rs",
        3,
        "PATH and a sentinel runner's role, in tests",
    ),
    (
        "src/tui/fault.rs",
        1,
        "the fault-injection point a test names",
    ),
    (
        "src/usage/cache.rs",
        1,
        "a fault-injection role a test names",
    ),
    ("src/process.rs", 1, "a re-run test binary's role"),
    ("src/test_net.rs", 1, "a re-run test binary's role"),
    ("src/tui/signals.rs", 1, "a re-run test binary's role"),
    (
        "src/profile_home/tests.rs",
        1,
        "a re-run test binary's role",
    ),
    (
        "src/session/store/fork/tests.rs",
        1,
        "a re-run test binary's role",
    ),
    ("src/local_time/tests.rs", 1, "a re-run test binary's role"),
    (
        "src/usage/cache/tests.rs",
        3,
        "a re-run test binary's role and arguments",
    ),
    (
        "src/tui/drawn_wider.rs",
        2,
        "the drawn-wider harness's own two, in a test",
    ),
];

/// The spellings of a read: `std::env`'s functions under any path, and
/// libc's. Spelled in halves so that this list is not a read itself.
const READS: [&str; 6] = [
    concat!("env", "::var("),
    concat!("env", "::var_os("),
    concat!("env", "::vars("),
    concat!("env", "::vars_os("),
    concat!("env", "::home_dir("),
    concat!("get", "env("),
];

/// How many times `line` reads by one of [`READS`]: each spelling where it
/// starts a path segment -- `test_env::var(`, a function of this module's,
/// is not `env::var(`.
fn reads_in(line: &str) -> usize {
    READS
        .iter()
        .map(|read| {
            line.match_indices(read)
                .filter(|(at, _)| {
                    !line[..*at]
                        .chars()
                        .next_back()
                        .is_some_and(|before| before.is_alphanumeric() || before == '_')
                })
                .count()
        })
        .sum()
}

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

#[test]
fn every_other_read_of_the_environment_in_src_is_listed() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    sources(&manifest.join("src"), &mut files);
    let mut problems = Vec::new();
    let mut seen = BTreeSet::new();
    for path in files {
        let relative = path
            .strip_prefix(manifest)
            .expect("under the manifest")
            .to_string_lossy()
            .into_owned();
        let text = std::fs::read_to_string(&path).expect("a source file");
        let mut hits = Vec::new();
        for (index, line) in text.lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            hits.extend(std::iter::repeat_n(index + 1, reads_in(line)));
        }
        let listed = LISTED.iter().find(|(file, _, _)| *file == relative);
        if !hits.is_empty() || listed.is_some() {
            seen.insert(relative.clone());
        }
        let expected = listed.map_or(0, |(_, count, _)| *count);
        if hits.len() != expected {
            problems.push(format!(
                "{relative}: {} reads at lines {hits:?}, {expected} listed",
                hits.len()
            ));
        }
    }
    for (file, _, _) in LISTED {
        if !seen.contains(*file) {
            problems.push(format!("{file} is listed and has no read"));
        }
    }
    assert!(
        problems.is_empty(),
        "a read of the process environment outside process_env!:\n{}",
        problems.join("\n")
    );
}
