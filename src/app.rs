//! Application composition and command dispatch.
//!
//! [`run`] is the one place that turns a parsed invocation into bytes on a
//! stream and an exit code. Every later slice adds a match arm here rather than
//! a second entry point, so there is exactly one path from argument to effect.

use std::fmt::{self, Write as _};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use crate::agent::{run_turn_saved, TurnRequest};
use crate::cli::{Cli, Command};
use crate::config::{ConfigError, PermissionMode, RuntimeConfig, SettingSource};
use crate::gateway::{CancelToken, Endpoint, GatewayProvider, DEFAULT_MAX_ATTEMPTS};
use crate::output::{
    CheckStatus, DoctorCheck, DoctorSnapshot, Event, EventSink, JsonlSink, OutputFormat,
    SessionDetailSnapshot, SessionFacts, SessionsSnapshot, StatusSnapshot, TextSink,
    MISSING_AUTH_HELP,
};
use crate::permission::{Grant, PermissionSession, TtyPrompter, YOLO_WARNING};
use crate::session::{
    ListFilter, ListScope, NewSession, Selector, SessionError, SessionEvent, SessionId,
    SessionRecorder, SessionStore,
};
use crate::tools::ToolContext;
use crate::workspace::{AccessScope, ProjectContext};

/// The exit code for a rejected invocation.
///
/// Upstream exits 1 rather than the shell's conventional 2 for a usage error
/// (`vercel-labs/fx@580a0c5d tests/e2e/cli.test.ts:404-412`), and scripts around
/// xfx should not have to learn a second convention.
const REJECTED_EXIT_CODE: u8 = 1;

/// The exit code for a turn that did not complete.
///
/// The same value as a rejection: from a script's point of view both mean "xfx
/// did not do what you asked", and the `error` event carries the difference.
const TURN_FAILURE_EXIT_CODE: u8 = 1;

/// What xfx prints when the user interrupts a turn.
///
/// The first interrupt is a request, not a kill: xfx stops the running command,
/// lets the turn report itself as cancelled, and exits with a terminal event a
/// `--json` caller can still parse. Saying so is the difference between "it
/// ignored me" and "it is stopping".
pub const INTERRUPT_NOTICE: &str =
    "xfx: interrupted -- stopping the turn; press Ctrl-C again to exit immediately.";

/// The exit status for a process killed on a second interrupt: 128 + SIGINT.
const INTERRUPTED_EXIT_CODE: i32 = 130;

/// The longest xfx waits for its interrupt handler before starting anyway.
///
/// Installing one takes a fraction of a millisecond. This bound exists so that
/// a machine where it somehow cannot be installed still gets a working xfx,
/// with the default Ctrl-C behaviour, rather than a hung one.
const INTERRUPT_INSTALL_TIMEOUT: Duration = Duration::from_secs(2);

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
///
/// The handles are deliberately *not* locked for the duration of the command.
/// xfx's interrupt watcher lives on another thread and its whole job is to say
/// something while a command is still running; a lock held across the command
/// makes that write block forever on the lock the command is holding. That is
/// the difference between "Ctrl-C says it is stopping" and "Ctrl-C appears to
/// do nothing, and the second one does nothing either". Each write takes the
/// lock for itself instead.
pub async fn run(cli: Cli) -> Result<ExitCode, AppError> {
    run_with(cli, &mut io::stdout(), &mut io::stderr()).await
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
        // The shell is the one command whose streams are not arguments: it is
        // defined by the terminal the process was given, and it refuses to run
        // when there is not one. Its startup diagnostics still go through the
        // caller's stderr, so the refusal is testable without a terminal.
        Command::Interactive => {
            let config = load_config()?;
            crate::interactive::run(&config, stderr).await
        }
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
            no_save,
            add_dirs,
            mode,
            resume,
        } => {
            let config = load_config()?;
            let request = AskRequest {
                prompt,
                json,
                no_save,
                add_dirs,
                mode,
                resume,
            };
            ask(&config, request, stdout, stderr).await
        }
        Command::Status { json } => {
            let config = load_config()?;
            let snapshot =
                StatusSnapshot::new(&config, crate::build_info(), session_facts(&config));
            write!(
                stdout,
                "{}",
                snapshot.render(OutputFormat::from_json_flag(json))
            )?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Sessions { json, all, limit } => {
            let config = load_config()?;
            let scope = if all {
                ListScope::AllWorkspaces
            } else {
                ListScope::CurrentWorkspace(config.workspace_root.clone())
            };
            // Read-only: a machine that has never run `ask` still has an empty
            // home after `xfx sessions`.
            let store = read_only_store(&config);
            let listed = match store.list(&ListFilter::new(scope).with_limit(limit)) {
                Ok(listed) => listed,
                Err(err) => return fail_command(stderr, &err.to_string()),
            };
            write!(
                stdout,
                "{}",
                SessionsSnapshot::new(&listed).render(OutputFormat::from_json_flag(json))
            )?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Session { json, selector } => {
            let config = load_config()?;
            let store = read_only_store(&config);
            let detail = match store.detail(&selector, &config.workspace_root) {
                Ok(detail) => detail,
                Err(err) => return fail_command(stderr, &err.to_string()),
            };
            write!(
                stdout,
                "{}",
                SessionDetailSnapshot::new(&detail).render(OutputFormat::from_json_flag(json))
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

/// One `ask` invocation, as parsed.
struct AskRequest {
    prompt: String,
    json: bool,
    no_save: bool,
    add_dirs: Vec<PathBuf>,
    /// The mode the invocation asked for, if it asked. Kept as an `Option` so
    /// "the user chose ask mode" and "nothing chose a mode" stay different
    /// facts after a session is restored.
    mode: Option<PermissionMode>,
    resume: Option<Selector>,
}

/// Runs one streamed model turn, and records it unless told not to.
///
/// Every failure is reported through the same event sink as a success, so a
/// `--json` caller gets exactly one terminal JSONL event whatever happened, and
/// a human gets the answer on stdout and the diagnosis on stderr.
///
/// The order below is the order the failures cost the least in: the workspace,
/// then the credential and the endpoint, then the session. A resume that names a
/// session that does not exist must fail before a token is spent, and a machine
/// with no credential must not collect one empty session per attempt.
async fn ask(
    config: &RuntimeConfig,
    request: AskRequest,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<ExitCode, AppError> {
    // The permission mode is deliberately *not* restored from a session. It is
    // the most dangerous setting xfx has, and a `--yolo` turn recorded last week
    // must not become the default of a turn run today without the word being
    // typed again.
    let mode = request.mode.unwrap_or(config.permission_mode);
    // Before anything runs, and on stderr, so it is visible whether or not the
    // caller is parsing stdout as JSONL.
    if mode == PermissionMode::Yolo {
        writeln!(stderr, "{YOLO_WARNING}")?;
    }

    // The sink borrows both streams in text mode, so the whole turn happens in
    // here and anything that still has to be said afterwards comes back out.
    let (code, warning) = run_ask(config, request, mode, stdout, stderr).await?;
    if let Some(warning) = warning {
        writeln!(stderr, "{warning}")?;
    }
    Ok(code)
}

/// The body of `ask`, returning the exit code and anything left to report.
async fn run_ask(
    config: &RuntimeConfig,
    request: AskRequest,
    mode: PermissionMode,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<(ExitCode, Option<String>), AppError> {
    let mut sink: Box<dyn EventSink + '_> = if request.json {
        Box::new(JsonlSink::new(stdout))
    } else {
        Box::new(TextSink::new(stdout, stderr))
    };
    let quiet = |code: Result<ExitCode, AppError>| code.map(|code| (code, None));

    // The authority the turn's tools will run under, resolved before anything
    // else. A directory the user named but xfx cannot use is a mistake in the
    // invocation, and reporting it here costs no credential and no round trip.
    let scope = match AccessScope::new(&config.workspace_root, &request.add_dirs) {
        Ok(scope) => scope,
        Err(err) => return quiet(fail_turn(sink.as_mut(), err.to_string())),
    };

    // The credential and the endpoint are checked next because they are free,
    // they leak nothing, and they are the two ways an `ask` most often cannot
    // start at all. Checking them before the session is opened is also what
    // keeps a machine with no credential from accumulating a directory of empty
    // sessions, one per failed attempt.
    let Some(credential) = config.credential.clone() else {
        return quiet(fail_turn(sink.as_mut(), MISSING_AUTH_HELP.to_string()));
    };
    let endpoint = match Endpoint::from_process() {
        Ok(endpoint) => endpoint,
        Err(err) => return quiet(fail_turn(sink.as_mut(), err.to_string())),
    };

    // The session, before anything is asked of a model. A resume that names a
    // session that does not exist must fail before a token is spent.
    let mut opened = match open_session(config, &request, mode) {
        Ok(opened) => opened,
        Err(err) => return quiet(fail_turn(sink.as_mut(), err.to_string())),
    };

    // A restored model preference applies only when nothing this run chose one:
    // an explicit setting or override outranks what a past turn happened to use.
    let model = match &opened.restored_model {
        Some(model) if config.sources.model == SettingSource::CompiledDefault => model.clone(),
        _ => config.model.clone(),
    };
    if let Some(recorder) = opened.recorder.as_mut() {
        let model_changed = recorder.state().model != model;
        let mode_changed = recorder.state().permission_mode != mode;
        if model_changed || mode_changed {
            recorder.commit(SessionEvent::PreferencesChanged {
                model: model_changed.then(|| model.clone()),
                permission_mode: mode_changed.then(|| mode.label().to_string()),
            });
        }
    }

    // Project instructions, read now rather than restored: they are a fact about
    // the working tree as it is, and a resumed session must not carry a stale
    // copy that outranks the file on disk.
    let context = ProjectContext::discover(&scope);
    if let Some(recorder) = opened.recorder.as_mut() {
        recorder.commit(SessionEvent::ProjectContextRecorded {
            sources: context
                .sources()
                .iter()
                .map(|path| path.display().to_string())
                .collect(),
            bytes: context.total_bytes() as u64,
        });
    }

    let cancel = CancelToken::new();
    // Ctrl-C now means something. Without this the token existed and nothing
    // ever set it, so every cancellation path in the turn and in `terminal` was
    // unreachable from the binary.
    watch_for_interrupt(cancel.clone());
    let provider = match GatewayProvider::new(endpoint, credential, cancel.clone()) {
        Ok(provider) => provider,
        Err(err) => return quiet(fail_turn(sink.as_mut(), err.to_string())),
    };

    let mut permissions = permission_session(mode);
    // The prompt has to state the scope it is actually selling. When this turn
    // is being recorded, an "always" answer outlives the process and will be
    // reused by any later `ask --resume-id <id>`, so the id goes into the
    // question rather than being discovered afterwards.
    if let Some(recorder) = opened.recorder.as_ref() {
        permissions = permissions.with_durable_session(recorder.id().as_str());
    }
    for grant in &opened.restored_grants {
        permissions.grant(grant.clone());
    }
    let tools = ToolContext::new(scope)
        .with_permissions(permissions)
        .with_cancel(cancel.clone());

    let turn = TurnRequest {
        model,
        prompt: request.prompt,
        history: opened.history,
        max_steps: config.max_agent_steps,
        max_attempts: DEFAULT_MAX_ATTEMPTS,
        cancel,
        // Cloned rather than moved: a context shares one permission ledger and
        // one set of read proofs with the turn, so the grants it accumulates are
        // readable here after the turn ends.
        tools: tools.clone(),
    };

    // The turn writes its own terminal event, including on failure.
    let outcome = match opened.recorder.as_mut() {
        Some(recorder) => run_turn_saved(turn, context, &provider, sink.as_mut(), recorder).await,
        None => {
            let mut journal = crate::agent::NoJournal;
            run_turn_saved(turn, context, &provider, sink.as_mut(), &mut journal).await
        }
    };

    // Approvals the user gave during the turn are recorded after it, once, so a
    // grant survives to the next resume. Reading them back from the shared
    // context is what makes this the real list rather than a second tally.
    let mut warning = None;
    if let Some(recorder) = opened.recorder.as_mut() {
        for grant in tools.permissions().grants().to_vec() {
            if !opened.restored_grants.contains(&grant) {
                recorder.commit(SessionEvent::PermissionGrantRecorded {
                    tool: grant.tool,
                    target: grant.target,
                });
            }
        }
        // A turn that could not be recorded is reported next to the answer
        // rather than instead of it: the answer did arrive, and saying the turn
        // failed would be a lie in the other direction.
        warning = recorder.failure().map(|failure| format!("xfx: {failure}"));
    }

    Ok((
        match outcome {
            Ok(_) => ExitCode::SUCCESS,
            Err(_) => ExitCode::from(TURN_FAILURE_EXIT_CODE),
        },
        warning,
    ))
}

/// What opening a session produced for the turn that follows.
struct OpenedSession {
    recorder: Option<SessionRecorder>,
    history: Vec<crate::gateway::protocol::Message>,
    restored_grants: Vec<Grant>,
    restored_model: Option<String>,
}

/// Creates or resumes the session this invocation writes to.
///
/// `--no-save` returns an empty result without touching the filesystem, so the
/// flag's promise is kept by there being no code path that could break it: not
/// an empty session directory, not a manifest, nothing.
fn open_session(
    config: &RuntimeConfig,
    request: &AskRequest,
    mode: PermissionMode,
) -> Result<OpenedSession, SessionError> {
    let empty = OpenedSession {
        recorder: None,
        history: Vec::new(),
        restored_grants: Vec::new(),
        restored_model: None,
    };
    if request.no_save {
        return Ok(empty);
    }
    let Some(profile_dir) = config.profile_dir.as_deref() else {
        return Err(SessionError::Unavailable {
            detail: "xfx cannot record this turn because no home directory is set; \
                     rerun with --no-save to ask without recording"
                .to_string(),
        });
    };

    let store = SessionStore::open(profile_dir)?;
    match &request.resume {
        Some(selector) => {
            let resumed = store.resume(selector, &config.workspace_root)?;
            let state = resumed.session.state();
            let history = state.history_messages();
            let restored_grants = state.grants.clone();
            let restored_model = Some(state.model.clone());
            Ok(OpenedSession {
                recorder: Some(SessionRecorder::new(store, resumed.session)),
                history,
                restored_grants,
                restored_model,
            })
        }
        None => {
            let session = store.create(
                SessionId::generate(),
                NewSession {
                    origin_workspace_root: config.workspace_root.clone(),
                    workspace_root: config.workspace_root.clone(),
                    model: config.model.clone(),
                    permission_mode: mode,
                },
            )?;
            Ok(OpenedSession {
                recorder: Some(SessionRecorder::new(store, session)),
                ..empty
            })
        }
    }
}

/// Turns the next Ctrl-C into a cancellation, and the one after that into an exit.
fn watch_for_interrupt(cancel: CancelToken) {
    spawn_interrupt_thread(move || {
        if cancel.is_cancelled() {
            // Asked twice. The first request is still being honored somewhere;
            // the user has decided not to wait for it.
            std::process::exit(INTERRUPTED_EXIT_CODE);
        }
        cancel.cancel();
        let _ = writeln!(io::stderr(), "{INTERRUPT_NOTICE}");
    });
}

/// Calls `on_signal` once for every SIGINT, forever.
///
/// It runs on its own OS thread with its own small runtime, because xfx's
/// runtime is single-threaded and `terminal` blocks it for the duration of a
/// command -- and the shell blocks it for as long as a user takes to type. A
/// signal that could only be observed by the blocked runtime would arrive
/// exactly when it is least able to be noticed, which is when the user is most
/// likely to send one.
///
/// The thread is detached. It has nothing to clean up, and it must outlive
/// nothing: when the process ends, it ends.
///
/// **It returns only once the handler exists.** The OS handler is installed by
/// the first poll of the signal future, and until then SIGINT keeps its default
/// disposition -- so a caller that started printing a prompt before this
/// returned would be offering the user a Ctrl-C that kills xfx outright. The
/// wait is bounded because a handler xfx cannot install must not stop it from
/// starting either.
pub(crate) fn spawn_interrupt_thread<F>(mut on_signal: F)
where
    F: FnMut() + Send + 'static,
{
    let (installed, wait_for_install) = std::sync::mpsc::sync_channel::<()>(1);
    let spawned = std::thread::Builder::new()
        .name("xfx-interrupt".to_string())
        .spawn(move || {
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                // No runtime means no handler, which means the default SIGINT
                // disposition stays in place. Ctrl-C then kills xfx outright:
                // worse, but not silently worse. Dropping `installed` releases
                // the caller immediately rather than making it wait for a
                // handler that is never coming.
                return;
            };
            runtime.block_on(async move {
                // Polled once, before anything is reported as ready: that poll
                // is the registration.
                let mut first = std::pin::pin!(tokio::signal::ctrl_c());
                let during_install = tokio::time::timeout(Duration::ZERO, first.as_mut()).await;
                let _ = installed.send(());
                // A signal that arrived inside that window is still the user's
                // interrupt, so it is delivered rather than dropped.
                if during_install.is_ok() {
                    on_signal();
                }
                loop {
                    if tokio::signal::ctrl_c().await.is_err() {
                        return;
                    }
                    on_signal();
                }
            });
        });
    if spawned.is_ok() {
        let _ = wait_for_install.recv_timeout(INTERRUPT_INSTALL_TIMEOUT);
    }
}

/// Builds the permission session one `ask` runs under.
///
/// The approval channel is attached only when there is a real terminal on both
/// ends. Without one, `ask` mode denies every mutation rather than hanging on a
/// question nobody can see -- so a piped or scripted `xfx ask` fails closed by
/// construction rather than by remembering to check.
pub(crate) fn permission_session(mode: PermissionMode) -> PermissionSession {
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

/// Reports a command that produced no output, on the diagnostic stream.
///
/// `sessions` and `session` are not turns: they have no event stream, so a
/// failure is a plain diagnostic and stdout stays empty rather than carrying
/// half a document a caller might try to parse.
fn fail_command(stderr: &mut dyn Write, message: &str) -> Result<ExitCode, AppError> {
    writeln!(stderr, "xfx: {message}")?;
    Ok(ExitCode::from(REJECTED_EXIT_CODE))
}

/// A store that can only read, rooted at the configured profile home.
///
/// A machine with no home has no store, and an unusable path is reported by the
/// operation that needed it rather than by refusing to start.
fn read_only_store(config: &RuntimeConfig) -> SessionStore {
    let profile = config
        .profile_dir
        .clone()
        .unwrap_or_else(|| Path::new(crate::config::PROFILE_DIR_NAME).to_path_buf());
    SessionStore::read_only(&profile)
}

/// What `status` says about the session a turn in this workspace would continue.
///
/// Read-only and best-effort: `status` describes the machine, so a store that
/// cannot be read is reported as "nothing to continue" rather than as a failed
/// command. A session that exists but is damaged is visible through `doctor`'s
/// job, not by making `status` refuse to run.
fn session_facts(config: &RuntimeConfig) -> SessionFacts {
    let store = read_only_store(config);
    match store.detail(&Selector::Last, &config.workspace_root) {
        Ok(detail) => SessionFacts {
            history_turns: detail.state.turns.len() as u64,
            permission_grants: detail.state.grants.len() as u64,
        },
        Err(_) => SessionFacts::default(),
    }
}

/// Resolves configuration for the current directory.
fn load_config() -> Result<RuntimeConfig, AppError> {
    let workspace = std::env::current_dir().map_err(ConfigError::Workspace)?;
    Ok(RuntimeConfig::load(&workspace)?)
}

/// Builds the diagnostic checks in a fixed order: what xfx is looking at, what
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
    checks.push(sessions_check(config));

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
                "mode=auto: bounded workspace changes and reporting commands run without asking, but nothing that compiles or runs project code; sandbox={sandbox}"
            ),
        ),
        PermissionMode::Yolo => DoctorCheck::new(
            "permissions",
            CheckStatus::Warn,
            format!("mode=yolo: no permission check runs at all; sandbox={sandbox}"),
        ),
    }
}

/// Reports what the session store holds, and what it holds that xfx cannot use.
///
/// Three facts, in one line, and each of them is something a user can otherwise
/// only discover by accident:
///
/// - how many sessions are recorded, so `~/.xfx` is not a black box;
/// - how many session directories could not be trusted -- a store that is
///   quietly losing conversations should say so out loud rather than only in the
///   `skipped_invalid` field of a listing nobody reads; and
/// - how many staged manifests were left behind by a process that died between
///   staging and rename. `session/store.rs` promises exactly this report as the
///   reason it never unlinks a stage file it did not create, and until now that
///   promise had no reader. They are inert, but they are also the only visible
///   evidence that xfx was killed mid-write.
///
/// It is a report, not a repair: nothing here deletes, rebuilds, or compacts
/// anything. Read-only and bounded, so `doctor` stays a command that is always
/// safe to run.
fn sessions_check(config: &RuntimeConfig) -> DoctorCheck {
    let store = read_only_store(config);
    let listed = match store.list(&ListFilter::new(ListScope::AllWorkspaces).with_limit(usize::MAX))
    {
        Ok(listed) => listed,
        Err(err) => {
            return DoctorCheck::new(
                "sessions",
                CheckStatus::Warn,
                format!("cannot read the session store: {err}"),
            )
        }
    };
    let stages = leftover_stage_count(store.sessions_dir());

    // "at least", never a bare number that could be a ceiling reported as a
    // total: the listing is bounded twice, once by the limit it was asked for
    // and once by the store's own scan cap.
    let mut detail = if listed.truncated || listed.has_more {
        format!(
            "at least {} session(s) recorded; the store holds more than xfx reads in one pass",
            listed.sessions.len()
        )
    } else {
        format!("{} session(s) recorded", listed.sessions.len())
    };
    let mut status = CheckStatus::Ok;
    if listed.skipped_invalid > 0 {
        status = CheckStatus::Warn;
        let _ = write!(
            detail,
            "; {} session director{} could not be read and {} skipped by `xfx sessions`",
            listed.skipped_invalid,
            if listed.skipped_invalid == 1 {
                "y"
            } else {
                "ies"
            },
            if listed.skipped_invalid == 1 {
                "is"
            } else {
                "are"
            },
        );
    }
    if stages > 0 {
        status = CheckStatus::Warn;
        let _ = write!(
            detail,
            "; {stages} staged manifest file(s) left by an interrupted write remain under {} \
             (inert, and never read as session state)",
            store.sessions_dir().display()
        );
    }
    DoctorCheck::new("sessions", status, detail)
}

/// Counts leftover `*.staged` files under the session store.
///
/// Depth-bounded by construction: exactly one directory level below `sessions`,
/// which is the only place xfx's own staging writes, and only entries whose name
/// ends in that suffix. `DirEntry::file_type` does not follow symbolic links, so
/// a link planted in the store cannot make this walk somewhere else. A path it
/// cannot read contributes nothing rather than failing the check -- why it
/// cannot be read is already reported by the check above.
fn leftover_stage_count(sessions_dir: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(sessions_dir) else {
        return 0;
    };
    let mut count = 0usize;
    for entry in entries.flatten() {
        if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let Ok(inner) = std::fs::read_dir(entry.path()) else {
            continue;
        };
        count += inner
            .flatten()
            .filter(|file| {
                file.file_name()
                    .to_string_lossy()
                    .ends_with(crate::session::STAGE_SUFFIX)
            })
            .count();
    }
    count
}

/// Reports which settings layers were found, and which were found but ignored.
///
/// A file that exists but could not be parsed is the case that most needs to be
/// said out loud: the user wrote settings, xfx silently ran without them, and
/// reporting "no config files found" would tell them to go looking for a file
/// that is already there. Presence and usability are therefore reported
/// separately.
fn config_presence_check(config: &RuntimeConfig) -> DoctorCheck {
    // Paths are written in their documented `~/.xfx/...` and `.xfx.json` forms
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

    #[tokio::test]
    async fn the_shell_is_dispatched_and_refuses_a_test_harness_that_is_not_a_terminal() {
        // Under `cargo test` there is no terminal, so this exercises the arm
        // and the refusal at once -- and proves the refusal goes to the stderr
        // it was handed rather than to the process's own.
        let (code, stdout, stderr) = run_capture(Command::Interactive).await;
        assert_eq!(
            format!("{code:?}"),
            format!("{:?}", ExitCode::from(REJECTED_EXIT_CODE))
        );
        assert_eq!(stdout, "");
        assert!(stderr.contains("interactive terminal"), "{stderr}");
        assert!(stderr.contains("xfx ask"), "{stderr}");
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
