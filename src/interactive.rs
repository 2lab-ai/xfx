//! The interactive shell: what a bare `xfx` runs.
//!
//! It is deliberately not a full-screen TUI. It never takes the alternate
//! screen, never puts the terminal in raw mode, and never repaints anything it
//! has already written, so the transcript above it stays exactly where the user
//! left it and every line xfx prints is scrollback afterwards. Line editing is
//! the terminal's own: the kernel's canonical mode already provides backspace,
//! word erase, and line kill, and a shell that took those over would have to
//! restore a terminal it had changed. This one has nothing to restore
//! (`vercel-labs/fx@580a0c5d src/core/app/app_entry_runtime.zig:224` is the
//! upstream refusal to run without a terminal at all).
//!
//! Everything below the prompt is the same product as the command line: one
//! [`crate::agent::TurnMachine`] per prompt, a bundle from the same
//! [`crate::provider::Bundle::select`], the same [`crate::tools`] registry under the
//! same [`crate::permission`] authority, and the same
//! [`crate::session::SessionStore`]. The shell adds a loop, the slash commands
//! of [`SLASH_REGISTRY`], and an interrupt policy -- not a second way to talk to a model, and not a
//! second place that decides which backend a prompt goes to.

use std::fmt::Write as _;
use std::io::{self, IsTerminal, Write};
use std::process::ExitCode;
use std::sync::{Arc, Mutex};

use crate::agent::{run_turn_saved, TurnRequest};
use crate::app::{spawn_interrupt_thread, AppError, INTERRUPT_NOTICE};
use crate::config::{Environment, PermissionMode, RuntimeConfig};
use crate::gateway::{CancelToken, Provider, DEFAULT_MAX_ATTEMPTS};
use crate::output::{safe_one_line, Event, EventSink, OutputFormat, TextSink, SANDBOX_LABEL};
use crate::permission::{PermissionSession, YOLO_WARNING};
use crate::provider::model::{ModelOutcome, ModelRequest, ModelSelector};
use crate::provider::{Bundle, ProviderId};
use crate::session::{NewSession, SessionEvent, SessionId, SessionRecorder, SessionStore};
use crate::tools::ToolContext;
use crate::workspace::{AccessScope, ProjectContext};

/// Every slash command the shell accepts, in the order `/help` lists them.
///
/// The set is closed for the same reason the command grammar is: a name printed
/// by `/help` is a promise. `scripts/check-no-stubs.sh` reads this declaration
/// textually and requires an `implemented` row in `docs/parity.md` for each
/// entry, and refuses any name a `deferred` row claims.
///
/// It stays a flat list of **canonical names** even though [`SLASH_REGISTRY`]
/// below carries the same names with everything else known about them. The two
/// are pinned to each other at compile time, so the duplication cannot drift,
/// and the reason for it is the honesty gate: that script must be able to read
/// the advertised set out of the source *without building it*, so a repository
/// whose build is broken still cannot hide a broken promise.
///
/// The layout is pinned with `rustfmt::skip` because that script matches the
/// opening line; see [`crate::cli::ADVERTISED_COMMANDS`].
#[rustfmt::skip]
pub const SLASH_COMMANDS: &[&str] = &[
    "/help",
    "/new",
    "/clear",
    "/model",
    "/setup",
    "/version",
    "/quit",
];

/// One slash command, whole.
///
/// What a front end needs to know about a command in one place: what it is
/// called, what else it answers to, what `/help` says about it, and whether the
/// rest of the line is an argument or noise. Before this existed each of those
/// lived in its own `match` -- and a `match` per fact is a fact per place it can
/// be got wrong, which is exactly how a completion menu ends up offering a name
/// the parser does not answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlashSpec {
    /// The command this describes.
    pub command: Slash,
    /// The one name `/help` lists it under, leading slash included.
    pub name: &'static str,
    /// Every other name that reaches the same command.
    ///
    /// An alias is **not** a command of its own: it is metadata on this row, it
    /// is never printed as a name of its own, and [`SLASH_COMMANDS`] does not
    /// grow by one when it is added. What it does is make the parser answer it and
    /// the completion menu offer the command it belongs to.
    pub aliases: &'static [&'static str],
    /// The single line `/help` prints beside the name.
    pub summary: &'static str,
    /// Whether the rest of the line means anything to it.
    ///
    /// The completion menu's reason for existing: completing a command that
    /// takes an argument leaves the caret past a space, and completing one that
    /// does not leaves it at the end of the name.
    pub has_args: bool,
}

/// Every canonical command, in the order `/help` lists them, with everything
/// known about each.
///
/// **One declaration, three readers**: the parser ([`Slash::parse`]), the help
/// page ([`help_text`]), and the TUI's completion menu (`crate::tui`'s
/// `picker`). A name, a summary or an alias that only some of them agreed about
/// would be a command surface that answers differently depending on where you
/// asked -- which is the one thing a command surface must never be.
#[rustfmt::skip]
pub const SLASH_REGISTRY: &[SlashSpec] = &[
    SlashSpec {
        command: Slash::Help,
        name: "/help",
        aliases: &[],
        summary: "list these commands",
        has_args: false,
    },
    SlashSpec {
        command: Slash::New,
        name: "/new",
        aliases: &[],
        summary: "start a new session; the next prompt begins a fresh conversation",
        has_args: false,
    },
    SlashSpec {
        command: Slash::Clear,
        name: "/clear",
        aliases: &[],
        summary: "clear the screen; the conversation and its session are kept",
        has_args: false,
    },
    SlashSpec {
        command: Slash::Model,
        name: "/model",
        aliases: &[],
        summary: "show the model, or `/model <id>` to use another one from now on",
        has_args: true,
    },
    SlashSpec {
        command: Slash::Setup,
        name: "/setup",
        aliases: &[],
        summary: "`/setup <gateway|llmux>` to switch provider and record the choice",
        has_args: true,
    },
    SlashSpec {
        command: Slash::Version,
        name: "/version",
        aliases: &[],
        summary: "show the version this shell was built from",
        has_args: false,
    },
    SlashSpec {
        command: Slash::Quit,
        name: "/quit",
        // Upstream's own alias for the same command
        // (`vercel-labs/fx@580a0c5d src/builtins/commands.zig:457`), and the
        // only one xfx answers.
        aliases: &["/exit"],
        summary: "leave the shell",
        has_args: false,
    },
];

/// The registry row for one command.
///
/// Total by construction: the assertion below this proves at **compile time**
/// that the registry names the same commands as [`SLASH_COMMANDS`], in the
/// same order, so the scan cannot come back empty for a `Slash` that exists.
pub fn slash_spec(command: Slash) -> &'static SlashSpec {
    SLASH_REGISTRY
        .iter()
        .find(|spec| spec.command == command)
        .expect("every command has exactly one registry row")
}

/// The two declarations agree, or this crate does not build.
///
/// A unit test would find the same drift a moment later; a `const` assertion
/// finds it without running anything, which matters because one of the two
/// lists is read by a shell script that never builds the crate at all. What it
/// cannot check is that each row's `command` is the one its name belongs to --
/// `Slash` has no `const` equality -- and `the_canonical_list_and_the_registry_are_one_order`
/// is what checks that half.
const _: () = {
    assert!(SLASH_REGISTRY.len() == SLASH_COMMANDS.len());
    let mut index = 0;
    while index < SLASH_REGISTRY.len() {
        assert!(same(SLASH_REGISTRY[index].name, SLASH_COMMANDS[index]));
        index += 1;
    }
};

/// Whether two names are the same, in a `const` context.
///
/// `str`'s own comparison is not usable in one, and the alternative to eight
/// lines here is not checking the agreement until something runs.
const fn same(left: &str, right: &str) -> bool {
    let (left, right) = (left.as_bytes(), right.as_bytes());
    if left.len() != right.len() {
        return false;
    }
    let mut index = 0;
    while index < left.len() {
        if left[index] != right[index] {
            return false;
        }
        index += 1;
    }
    true
}

/// What the shell prints before reading a line.
pub const PROMPT: &str = "> ";

/// Why xfx will not open a shell here.
pub const NO_TERMINAL: &str = "xfx requires an interactive terminal (TTY); \
     run `xfx ask <prompt>` when there is not one";

/// Why xfx will not open a shell it could not record.
pub const NO_STORE: &str = "xfx cannot record a conversation because no home directory is set; \
     run `xfx ask --no-save <prompt>` to ask without recording";

/// What xfx says on the interrupt that ends it.
pub const LEAVING_NOTICE: &str = "xfx: interrupted -- leaving.";

/// The exit status of a process that stopped because it was interrupted.
const INTERRUPTED_EXIT_CODE: i32 = 130;

/// The most bytes of a mistyped command xfx quotes back.
///
/// A slash command is a word, and a word that runs past this is not a typo the
/// user needs to see in full -- it is something pasted, and the part of the
/// refusal worth reading is the guidance at the end of the line.
const MAX_QUOTED_COMMAND_BYTES: usize = 60;

/// Erase the screen, the scrollback, and put the cursor home.
///
/// The only control sequence the shell emits. `2J` clears what is visible and
/// `3J` clears the scrollback, which is what a user asking to clear a transcript
/// means -- the shell never does it on its own initiative.
const CLEAR_SCREEN: &str = "\u{1b}[H\u{1b}[2J\u{1b}[3J";

// ---------------------------------------------------------------------------
// what a submitted line is
// ---------------------------------------------------------------------------

/// One canonical slash command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slash {
    Help,
    New,
    Clear,
    Model,
    /// Switch the provider a turn talks to, and record the choice.
    ///
    /// The only command in the palette that writes to `~/.xfx/settings.json`
    /// and the only one that reaches the network, which is why it is a command
    /// of its own rather than an argument of `/model`: `/model` chooses among
    /// what the *current* provider offers, and this chooses the provider.
    Setup,
    Version,
    Quit,
}

impl Slash {
    /// The name as typed.
    ///
    /// Through the registry, like everything else that is known about a
    /// command: this used to be a `match` of its own, and a `match` that agreed
    /// with `/help`'s list only because someone kept them in step. There is no
    /// `summary` beside it any more for the same reason -- `/help` reads
    /// [`SLASH_REGISTRY`] directly, so there is nothing for a second accessor
    /// to disagree with.
    pub fn name(self) -> &'static str {
        slash_spec(self).name
    }

    /// The command `token` names, if it names one.
    ///
    /// **The registry is the whole grammar**, canonical names and aliases
    /// together, so a front end cannot answer a name the help page does not
    /// know about and cannot refuse one it does. The match stays exact and
    /// case-sensitive for the reason [`classify`] gives.
    fn parse(token: &str) -> Option<Self> {
        SLASH_REGISTRY
            .iter()
            .find(|spec| spec.name == token || spec.aliases.contains(&token))
            .map(|spec| spec.command)
    }
}

/// What one line the user submitted turns out to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Submitted {
    /// Nothing was typed. Nothing happens.
    Blank,
    /// One canonical command, with the rest of the line as its argument.
    Command { command: Slash, argument: String },
    /// A line that begins with `/` and names nothing.
    UnknownCommand { token: String },
    /// A prompt for the model.
    Prompt(String),
}

/// Decides what a submitted line is.
///
/// A slash command is a line whose *first* character is `/`, so `what does
/// a/b mean` is a prompt and `/nonesuch` is a mistake rather than a question.
/// The match is exact and case-sensitive: guessing at `/HELP` or `/hep` would
/// make the refusal nondeterministic, which is the one thing a command surface
/// must never be.
pub fn classify(line: &str) -> Submitted {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Submitted::Blank;
    }
    if !trimmed.starts_with('/') {
        return Submitted::Prompt(trimmed.to_string());
    }
    let (token, rest) = match trimmed.split_once(char::is_whitespace) {
        Some((token, rest)) => (token, rest.trim()),
        None => (trimmed, ""),
    };
    match Slash::parse(token) {
        Some(command) => Submitted::Command {
            command,
            argument: rest.to_string(),
        },
        None => Submitted::UnknownCommand {
            token: token.to_string(),
        },
    }
}

/// The exact refusal for a slash command that does not exist.
///
/// The token is quoted back so a user can see their typo, and it is the one
/// thing on this line that xfx did not write. It therefore goes through
/// [`safe_one_line`] first: a line beginning `/` and continuing with an escape
/// sequence would otherwise be echoed straight back and *obeyed* by the
/// terminal -- clearing the screen, retitling the window, moving the cursor --
/// and a very long one would push the guidance off the end of it. The backticks
/// are part of the same job: once control characters have become spaces, the
/// reader still has to be able to see where the quoted text stops.
///
/// **The count is derived, never spelled.** It used to read "the six it has",
/// which was a second declaration of the palette's size beside
/// [`SLASH_COMMANDS`] -- and a second declaration is a second thing that can
/// drift. `/setup` made it wrong the moment it was added, in the one sentence
/// whose whole job is to be exactly true about what the shell has.
pub fn unknown_command_message(token: &str) -> String {
    format!(
        "xfx: `{}` is not an xfx command; /help lists its {} commands",
        safe_one_line(token, MAX_QUOTED_COMMAND_BYTES),
        SLASH_COMMANDS.len()
    )
}

/// The `/help` page.
///
/// **One line per canonical name**, from [`SLASH_REGISTRY`] in its own order,
/// which is what keeps the page and the parser one declaration. An alias is
/// said on the line of the command it belongs to rather than on a line of its
/// own: a page with one line per *name* would be advertising one command per
/// name, and what the shell has is one command answering to two of them.
pub fn help_text() -> String {
    let mut out = String::from("xfx shell commands\n");
    for spec in SLASH_REGISTRY {
        let _ = write!(out, "  {:<9} {}", spec.name, spec.summary);
        if !spec.aliases.is_empty() {
            let _ = write!(out, " (also {})", spec.aliases.join(", "));
        }
        let _ = writeln!(out);
    }
    out.push_str("Anything else is a prompt. Ctrl-C stops a running turn; Ctrl-D leaves.\n");
    out
}

// ---------------------------------------------------------------------------
// interrupts
// ---------------------------------------------------------------------------

/// What the shell is doing when a signal arrives.
#[derive(Debug, Clone, Copy)]
enum Activity {
    /// Waiting for a line. `consecutive` counts interrupts since the last line
    /// the user actually submitted.
    Idle { consecutive: u32 },
    /// Running a turn, and whether it has already been asked to stop.
    Running { cancelled: bool },
}

/// The shell's interrupt policy, shared with the signal thread.
///
/// One lock covers both the activity and the token, so "is this Ctrl-C a
/// cancellation or an exit" and "clear the token for the next turn" cannot
/// interleave. That is the whole reason [`CancelToken::reset`] is safe here.
#[derive(Debug)]
struct Interrupts {
    state: Mutex<Activity>,
    cancel: CancelToken,
}

impl Interrupts {
    fn new(cancel: CancelToken) -> Self {
        Self {
            state: Mutex::new(Activity::Idle { consecutive: 0 }),
            cancel,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Activity> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Marks a turn as running and clears any earlier cancellation.
    fn begin_turn(&self) {
        let mut state = self.lock();
        self.cancel.reset();
        *state = Activity::Running { cancelled: false };
    }

    fn end_turn(&self) {
        *self.lock() = Activity::Idle { consecutive: 0 };
    }

    /// Records that the user submitted a line, so the interrupt count starts
    /// again from there.
    fn line_submitted(&self) {
        let mut state = self.lock();
        if let Activity::Idle { consecutive } = &mut *state {
            *consecutive = 0;
        }
    }

    /// Handles one SIGINT. Runs on the signal thread.
    fn signalled(&self) {
        let mut state = self.lock();
        match *state {
            Activity::Running { cancelled: false } => {
                self.cancel.cancel();
                *state = Activity::Running { cancelled: true };
                let _ = writeln!(io::stderr(), "{INTERRUPT_NOTICE}");
            }
            // Asked twice: the first request is still being honored somewhere
            // and the user has decided not to wait for it.
            Activity::Running { cancelled: true } => std::process::exit(INTERRUPTED_EXIT_CODE),
            Activity::Idle { consecutive } if consecutive >= 1 => {
                let _ = writeln!(io::stderr(), "{LEAVING_NOTICE}");
                std::process::exit(INTERRUPTED_EXIT_CODE)
            }
            Activity::Idle { consecutive } => {
                *state = Activity::Idle {
                    consecutive: consecutive + 1,
                };
                // The line discipline already discarded whatever was typed, so
                // the user is looking at a dead line. Offer a live one.
                let mut stdout = io::stdout();
                let _ = write!(stdout, "\n{PROMPT}");
                let _ = stdout.flush();
            }
        }
    }
}

// ---------------------------------------------------------------------------
// the conversation
// ---------------------------------------------------------------------------

/// One session and the tool authority bound to it.
///
/// The two are created and discarded together on purpose. A session grant --
/// the user answering "always" -- is sold as being about *this* session id, and
/// the read proofs that let a file be edited are about what this conversation
/// has seen. `/new` therefore drops both, and `/clear` keeps both.
///
/// Visible to the crate because the TUI's runtime thread opens its conversation
/// through this same call rather than growing a second one of its own
/// (`crate::tui::worker`).
pub(crate) struct Conversation {
    pub(crate) recorder: SessionRecorder,
    pub(crate) tools: ToolContext,
}

/// Creates the session this shell records into, and the authority its tools run
/// under.
///
/// Lazily, on the first prompt: a shell that is opened and closed without asking
/// anything leaves no empty session behind, and `/new` costs nothing until it is
/// used.
///
/// **The authority is the caller's to build**, and that is the one thing this
/// function does not decide. `crate::app::permission_session` attaches a
/// prompter that reads a line from standard input, which is right for a shell
/// whose loop is the only reader of that descriptor and wrong for the TUI,
/// where the UI thread is polling it -- two readers on one terminal is the bug
/// the TUI's whole topology exists to prevent. So the caller says what may say
/// yes, and this function only binds it to the session it just created, which
/// is what makes an "always" answer durable.
pub(crate) fn open_conversation(
    store: &SessionStore,
    config: &RuntimeConfig,
    model: &str,
    permissions: PermissionSession,
    cancel: &CancelToken,
) -> Result<Conversation, String> {
    let scope = AccessScope::primary_only(&config.workspace_root).map_err(|err| err.to_string())?;
    let session = store
        .create(
            SessionId::generate(),
            NewSession {
                origin_workspace_root: config.workspace_root.clone(),
                workspace_root: config.workspace_root.clone(),
                model: model.to_string(),
                // The mode the authority is really in, rather than a second
                // copy of it the caller could pass a different value for.
                permission_mode: permissions.mode(),
            },
        )
        .map_err(|err| err.to_string())?;
    let permissions = permissions.with_durable_session(session.id().as_str());
    let recorder = SessionRecorder::new(store.clone(), session);
    let tools = ToolContext::new(scope)
        .with_permissions(permissions)
        .with_cancel(cancel.clone());
    Ok(Conversation { recorder, tools })
}

// ---------------------------------------------------------------------------
// the loop
// ---------------------------------------------------------------------------

/// Runs the shell until the user leaves it.
///
/// `diagnostics` receives everything the shell says before it has a terminal to
/// own -- the refusals above. Past that point the shell writes to the process
/// streams directly, because by then they are provably the terminal.
pub async fn run(
    config: &RuntimeConfig,
    diagnostics: &mut dyn Write,
) -> Result<ExitCode, AppError> {
    // Both ends, before anything else is resolved and before the profile home
    // is touched: a shell whose prompt nobody can see, or whose answers go into
    // a pipe, is a program waiting forever for a person who is not there.
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        writeln!(diagnostics, "{NO_TERMINAL}")?;
        return Ok(ExitCode::from(1));
    }
    let Some(profile_dir) = config.profile_dir.clone() else {
        writeln!(diagnostics, "{NO_STORE}")?;
        return Ok(ExitCode::from(1));
    };
    // Opened now rather than at the first prompt: "xfx cannot write here" is a
    // fact about the machine, and the moment to say it is before the user has
    // typed a paragraph they are about to lose.
    let store = match SessionStore::open(&profile_dir) {
        Ok(store) => store,
        Err(err) => {
            writeln!(diagnostics, "xfx: {err}")?;
            return Ok(ExitCode::from(1));
        }
    };

    let mode = config.permission_mode;
    if mode == PermissionMode::Yolo {
        writeln!(diagnostics, "{YOLO_WARNING}")?;
    }

    // Owned from here down, because `/setup` replaces it. Everything below
    // reads the configuration through this binding, so a provider switch is one
    // assignment rather than a set of fields kept in step by hand -- and the
    // value it is assigned comes from re-reading the file that was just
    // written, never from what the writer believed it wrote.
    let mut config = config.clone();
    let env = Environment::from_process();

    let cancel = CancelToken::new();
    let interrupts = Arc::new(Interrupts::new(cancel.clone()));
    let signal_target = Arc::clone(&interrupts);
    spawn_interrupt_thread(move || signal_target.signalled());

    let mut selector = ModelSelector::new(&config);
    let mut model = selector.model().to_string();
    let mut conversation: Option<Conversation> = None;
    // Built on first use: a shell must open on a machine with no credential --
    // that is exactly the machine whose user needs `/help` -- and it must not
    // reach for a network endpoint until there is something to send.
    let mut bundle: Option<Bundle> = None;

    write!(io::stdout(), "{}", banner(&config, &model))?;
    io::stdout().flush()?;

    loop {
        {
            let mut stdout = io::stdout();
            write!(stdout, "{PROMPT}")?;
            stdout.flush()?;
        }
        let Some(line) = read_line()? else {
            // End of input. The prompt is unterminated, so close its line.
            writeln!(io::stdout())?;
            return Ok(ExitCode::SUCCESS);
        };
        interrupts.line_submitted();

        match classify(&line) {
            Submitted::Blank => continue,
            Submitted::UnknownCommand { token } => {
                writeln!(io::stderr(), "{}", unknown_command_message(&token))?;
            }
            Submitted::Command { command, argument } => {
                match command {
                    Slash::Quit => return Ok(ExitCode::SUCCESS),
                    Slash::Help => {
                        write!(io::stdout(), "{}", help_text())?;
                    }
                    Slash::Version => {
                        writeln!(io::stdout(), "{}", version_line())?;
                    }
                    Slash::Model => {
                        model =
                            apply_model(&argument, &mut selector, conversation.as_mut()).await?;
                    }
                    Slash::Setup => {
                        model = apply_setup(
                            &argument,
                            &env,
                            &mut config,
                            &mut selector,
                            &mut bundle,
                            &mut conversation,
                        )
                        .await?;
                    }
                    Slash::Clear => {
                        let mut stdout = io::stdout();
                        write!(stdout, "{CLEAR_SCREEN}")?;
                        write!(stdout, "{}", banner(&config, &model))?;
                        writeln!(stdout, "{}", kept_line(conversation.as_ref()))?;
                    }
                    Slash::New => {
                        // Dropping the recorder closes the log and releases the
                        // session's writer lock, so the next prompt is free to
                        // create a genuinely new identity.
                        conversation = None;
                        writeln!(
                            io::stdout(),
                            "[shell] new session; the next prompt starts a fresh conversation"
                        )?;
                    }
                }
                io::stdout().flush()?;
            }
            Submitted::Prompt(prompt) => {
                let bundle_ref = match ensure_provider(&mut bundle, &config, &cancel) {
                    Ok(bundle) => bundle,
                    Err(message) => {
                        report_turn_failure(message)?;
                        continue;
                    }
                };
                let conversation = match &mut conversation {
                    Some(existing) => existing,
                    slot @ None => match open_conversation(
                        &store,
                        &config,
                        &model,
                        crate::app::permission_session(mode),
                        &cancel,
                    ) {
                        Ok(opened) => slot.insert(opened),
                        Err(message) => {
                            report_turn_failure(format!("xfx: {message}"))?;
                            continue;
                        }
                    },
                };
                one_turn(
                    bundle_ref.stream.as_ref(),
                    conversation,
                    &model,
                    prompt,
                    &config,
                    &interrupts,
                )
                .await?;
            }
        }
    }
}

/// Runs exactly one turn, the same way `xfx ask` runs its one.
async fn one_turn(
    provider: &dyn Provider,
    conversation: &mut Conversation,
    model: &str,
    prompt: String,
    config: &RuntimeConfig,
    interrupts: &Interrupts,
) -> io::Result<()> {
    // Project instructions are read now rather than remembered, so editing
    // `AGENTS.md` in another window takes effect on the next prompt.
    let context = ProjectContext::discover(conversation.tools.scope());
    conversation
        .recorder
        .commit(SessionEvent::ProjectContextRecorded {
            sources: context
                .sources()
                .iter()
                .map(|path| path.display().to_string())
                .collect(),
            bytes: context.total_bytes() as u64,
        });

    let replay = conversation
        .recorder
        .state()
        .history_messages(config.provider.wire());
    let request = TurnRequest {
        model: model.to_string(),
        prompt,
        history: replay.messages,
        max_steps: config.max_agent_steps,
        max_attempts: DEFAULT_MAX_ATTEMPTS,
        cancel: conversation.tools.cancel().clone(),
        tools: conversation.tools.clone(),
    };

    let stdout = io::stdout();
    let stderr = io::stderr();
    let mut stderr_lock = stderr.lock();
    for notice in &replay.notices {
        writeln!(stderr_lock, "{notice}")?;
    }
    drop(stderr_lock);
    let mut sink = TextSink::new(stdout, stderr).with_tool_notices();

    interrupts.begin_turn();
    // Awaited here, on this thread: when this returns there is no work left
    // running anywhere, which is what makes the next prompt safe to print.
    let _ = run_turn_saved(
        request,
        context,
        provider,
        &mut sink,
        &mut conversation.recorder,
    )
    .await;
    interrupts.end_turn();

    // Approvals given during the turn become durable once, after it, so an
    // "always" answer survives to the next `xfx ask --resume-id <id>`. The step
    // is the recorder's ([`SessionRecorder::record_new_grants`]), which is what
    // keeps this shell, `app::ask` and the TUI's worker from drifting into
    // three meanings of the same sentence.
    let granted = conversation.tools.permissions().grants().to_vec();
    conversation.recorder.record_new_grants(&granted);
    if let Some(failure) = conversation.recorder.failure() {
        writeln!(io::stderr(), "xfx: {failure}")?;
    }
    Ok(())
}

/// Builds the bundle once, or explains why there can not be one.
///
/// The decision itself belongs to [`Bundle::select`], which `ask` uses too: the
/// shell caches the result for the life of the session, it does not choose a
/// backend of its own.
fn ensure_provider<'a>(
    slot: &'a mut Option<Bundle>,
    config: &RuntimeConfig,
    cancel: &CancelToken,
) -> Result<&'a Bundle, String> {
    if slot.is_none() {
        *slot = Some(Bundle::select(config, cancel)?);
    }
    Ok(slot.as_ref().expect("the bundle was just built"))
}

/// Reports a prompt that never became a request, in the shape a turn failure has.
fn report_turn_failure(message: String) -> io::Result<()> {
    let mut sink = TextSink::new(io::stdout(), io::stderr());
    sink.emit(&Event::Error { message })
}

/// Reads one line, or `None` at end of input.
///
/// The process-wide buffered stdin is used rather than a private reader,
/// because the permission prompt reads from the same terminal in the middle of
/// a turn: two buffers over one descriptor would let one of them swallow the
/// other's answer.
fn read_line() -> io::Result<Option<String>> {
    let mut line = String::new();
    match io::stdin().read_line(&mut line) {
        Ok(0) => Ok(None),
        Ok(_) => Ok(Some(line)),
        // Not fatal: the bytes that were not text are gone, and the shell says
        // so and reads the next line rather than exiting on a stray paste.
        Err(err) if err.kind() == io::ErrorKind::InvalidData => {
            writeln!(io::stderr(), "xfx: that line was not valid UTF-8; ignored")?;
            Ok(Some(String::new()))
        }
        Err(err) => Err(err),
    }
}

/// Gets the label for a provider id.
fn provider_label(provider: crate::provider::ProviderId) -> &'static str {
    match provider {
        crate::provider::ProviderId::Gateway => "gateway",
        crate::provider::ProviderId::Llmux => "llmux",
    }
}

/// Prints the catalog to stdout, bounded at MAX_RENDERED_MODELS.
fn print_catalog(catalog: &crate::provider::model::CatalogState) -> io::Result<()> {
    use crate::provider::model::{CatalogState, MAX_RENDERED_MODELS};

    match catalog {
        CatalogState::Unavailable => {
            writeln!(
                io::stdout(),
                "{}",
                crate::provider::model::NO_CATALOG_NOTICE
            )?;
        }
        CatalogState::NotLoaded => {
            // Not printed if not loaded yet.
        }
        CatalogState::Loaded(entries) => {
            writeln!(io::stdout(), "[shell] catalog={} models", entries.len())?;
            for entry in entries.iter().take(MAX_RENDERED_MODELS) {
                let context_str = entry
                    .max_context
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                let efforts_str = if entry.efforts.is_empty() {
                    "none".to_string()
                } else {
                    entry.efforts.join(",")
                };
                writeln!(
                    io::stdout(),
                    "[shell]   {} context={} efforts={}",
                    entry.preferred_name(),
                    context_str,
                    efforts_str
                )?;
            }
            let remaining = entries.len().saturating_sub(MAX_RENDERED_MODELS);
            if remaining > 0 {
                writeln!(io::stdout(), "[shell]   ... and {remaining} more")?;
            }
        }
        CatalogState::Failed(reason) => {
            writeln!(io::stdout(), "[shell] catalog=unread ({reason})")?;
        }
    }
    Ok(())
}

/// What `/setup` says when it was not given a provider it can set up.
///
/// Derived from [`ProviderId::ALL`] rather than spelled, for the reason
/// [`unknown_command_message`] derives its count: a provider added to the build
/// and not to this sentence would be a provider the shell can switch to and
/// will not admit to.
pub fn setup_usage() -> String {
    let names: Vec<&str> = ProviderId::ALL.iter().map(|id| id.label()).collect();
    format!(
        "xfx: /setup takes a provider to set up -- {}",
        names.join(" or ")
    )
}

/// Applies `/setup <provider>`, returning the model in force afterwards.
///
/// The command's whole shape is that the **file** decides, not this function:
/// the transaction writes `~/.xfx/settings.json` through the same
/// `provider::setup::run` the `xfx setup` subcommand runs -- so the on-disk
/// bytes are the same bytes -- and then the configuration is **re-read from
/// that file**. Nothing here assigns a provider, a model or a URL from what the
/// writer believed it wrote; a layer that outranks the profile still outranks it
/// after the write, and the report says which one.
///
/// The bundle and the conversation go with it. A conversation carried across a
/// provider switch would replay one provider's history into another's wire
/// format, and a bundle is a connection to the endpoint that is no longer the
/// one configured.
async fn apply_setup(
    argument: &str,
    env: &Environment,
    config: &mut RuntimeConfig,
    selector: &mut ModelSelector,
    bundle: &mut Option<Bundle>,
    conversation: &mut Option<Conversation>,
) -> io::Result<String> {
    let Some(provider) = ProviderId::parse(argument) else {
        writeln!(io::stderr(), "{}", setup_usage())?;
        return Ok(selector.model().to_string());
    };
    let report = match crate::provider::setup::run(config, env, provider, None).await {
        Ok(report) => report,
        Err(err) => {
            // The old provider is kept, whole: nothing was written, so nothing
            // about this session has changed.
            writeln!(io::stderr(), "xfx: {err}")?;
            return Ok(selector.model().to_string());
        }
    };
    let settings_path = report.settings_path.clone();
    let snapshot = crate::output::SetupSnapshot::new(&report);
    write!(io::stdout(), "{}", snapshot.render(OutputFormat::Text))?;
    if let Some(warning) = snapshot.override_warning() {
        writeln!(io::stderr(), "{warning}")?;
    }
    if let Some(warning) = snapshot.credential_warning() {
        writeln!(io::stderr(), "{}", warning)?;
    }

    // Re-read, rather than believed. `load_with` is the only thing in the
    // product that decides what a settings file means, and a shell that set the
    // fields itself would be a second such thing -- one that cannot see the
    // environment override the report just warned about.
    match RuntimeConfig::load_with(env, &config.workspace_root) {
        Ok(reloaded) => {
            *config = reloaded;
            *selector = ModelSelector::new(config);
            *bundle = None;
            *conversation = None;
            let model = selector.model().to_string();
            writeln!(
                io::stdout(),
                "[shell] provider={} model={model}; the next prompt starts a fresh conversation",
                config.provider.label()
            )?;
            Ok(model)
        }
        Err(err) => {
            // The file on disk is the new one and this process cannot read it.
            // Saying so is the whole of what is left to do: inventing the
            // provider from the report would be running against a
            // configuration nothing has parsed.
            writeln!(
                io::stderr(),
                "xfx: {} was written but could not be re-read: {err}",
                settings_path.display()
            )?;
            Ok(selector.model().to_string())
        }
    }
}

/// Applies `/model`, returning the model in force afterwards.
///
/// The rules live in [`crate::provider::model::ModelSelector`] so the shell and
/// any other front end cannot disagree about what `/model` means; what stays
/// here is the printing, which is this surface's own business.
async fn apply_model(
    argument: &str,
    selector: &mut ModelSelector,
    conversation: Option<&mut Conversation>,
) -> io::Result<String> {
    if argument.is_empty() {
        // The one place `/model` is allowed to touch the network: the catalog
        // load is where a provider that is not answering legitimately surfaces,
        // and `status` and `doctor` are where it must not.
        selector.ensure_catalog().await;
    }
    match selector.apply(if argument.is_empty() {
        ModelRequest::Report
    } else {
        ModelRequest::Select(argument)
    }) {
        ModelOutcome::Reported {
            provider,
            model,
            source,
        } => {
            writeln!(
                io::stdout(),
                "[shell] model={model} provider={} source={}",
                provider_label(provider),
                source.label()
            )?;
            print_catalog(selector.catalog())?;
            Ok(model)
        }
        ModelOutcome::Selected {
            model,
            previous: _,
            unverified,
            ..
        } => {
            writeln!(io::stdout(), "[shell] model={model}")?;
            if let Some(reason) = unverified {
                // The sentence is shared with the band
                // (`provider::model::unverified_notice`); where it is written
                // is this surface's own business.
                writeln!(
                    io::stderr(),
                    "{}",
                    crate::provider::model::unverified_notice(&reason)
                )?;
            }
            // Durable when there is something to record it in: a resumed session must
            // continue with the model the conversation was actually held in.
            if let Some(conversation) = conversation {
                conversation
                    .recorder
                    .commit(SessionEvent::PreferencesChanged {
                        model: Some(model.clone()),
                        permission_mode: None,
                    });
            }
            Ok(model)
        }
        ModelOutcome::Unchanged { model } => {
            writeln!(io::stdout(), "[shell] model={model} unchanged")?;
            Ok(model)
        }
        ModelOutcome::Refused { reason } => {
            writeln!(io::stderr(), "xfx: {reason}")?;
            Ok(selector.model().to_string())
        }
    }
}

/// What `/clear` says about the conversation it did not touch.
fn kept_line(conversation: Option<&Conversation>) -> String {
    match conversation {
        Some(conversation) => format!(
            "[shell] cleared the screen; session={} keeps {} turn(s)",
            conversation.recorder.id(),
            conversation.recorder.state().turns.len()
        ),
        None => "[shell] cleared the screen; no conversation has started yet".to_string(),
    }
}

/// The version line `/version` prints.
///
/// Visible to the crate because the TUI answers the same commands from the
/// same declarations rather than growing a second `/version` that could drift
/// from this one (`crate::tui::shell`).
pub(crate) fn version_line() -> String {
    let build = crate::build_info();
    match build.revision {
        Some(revision) => format!(
            "xfx {} ({}, revision {revision})",
            crate::VERSION,
            build.channel
        ),
        None => format!("xfx {} ({})", crate::VERSION, build.channel),
    }
}

/// What the shell says about itself before its first prompt.
fn banner(config: &RuntimeConfig, model: &str) -> String {
    let mut out = format!(
        "{} -- unofficial, experimental Rust port of fx\n",
        version_line()
    );
    let _ = writeln!(
        out,
        "[shell] model={model} permission_mode={} sandbox={SANDBOX_LABEL}",
        config.permission_mode.label()
    );
    let _ = writeln!(out, "[shell] workspace={}", config.workspace_root.display());
    let _ = writeln!(
        out,
        "[shell] type a prompt, or /help for the {} commands; Ctrl-D leaves",
        SLASH_COMMANDS.len()
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_advertised_slash_command_parses_back_to_itself() {
        for name in SLASH_COMMANDS {
            let command = Slash::parse(name).unwrap_or_else(|| panic!("{name} does not parse"));
            assert_eq!(command.name(), *name);
        }
    }

    #[test]
    fn the_shell_advertises_exactly_seven_commands() {
        // The count is a product decision, not an accident: the shell is
        // deliberately smaller than upstream's ~40-command palette
        // (`vercel-labs/fx@580a0c5d src/builtins/commands.zig:414-457`).
        // Seven since `/setup`; `/models` and `/provider` stay deferred.
        assert_eq!(SLASH_COMMANDS.len(), 7);
        let mut sorted = SLASH_COMMANDS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), SLASH_COMMANDS.len(), "a name is repeated");
    }

    #[test]
    fn a_blank_line_is_not_a_prompt() {
        assert_eq!(classify(""), Submitted::Blank);
        assert_eq!(classify("   \t "), Submitted::Blank);
        assert_eq!(classify("\n"), Submitted::Blank);
    }

    #[test]
    fn a_leading_slash_is_the_only_thing_that_makes_a_command() {
        assert_eq!(
            classify("what does a/b /help mean"),
            Submitted::Prompt("what does a/b /help mean".to_string())
        );
        assert_eq!(
            classify("  /quit  "),
            Submitted::Command {
                command: Slash::Quit,
                argument: String::new()
            }
        );
    }

    #[test]
    fn a_command_keeps_the_rest_of_its_line_as_one_argument() {
        assert_eq!(
            classify("/model acme/model-9"),
            Submitted::Command {
                command: Slash::Model,
                argument: "acme/model-9".to_string()
            }
        );
        assert_eq!(
            classify("/model  one two "),
            Submitted::Command {
                command: Slash::Model,
                argument: "one two".to_string()
            }
        );
    }

    #[test]
    fn an_unknown_command_is_named_exactly_and_matched_exactly() {
        assert_eq!(
            classify("/nonesuch now"),
            Submitted::UnknownCommand {
                token: "/nonesuch".to_string()
            }
        );
        // No case folding and no prefix guessing: a refusal has to be the same
        // sentence every time. `/exit` is **not** in this list any more: it is
        // a registry alias for `/quit` and is answered rather than refused
        // (`exit_is_an_alias_for_quit_and_not_a_canonical_name`).
        for near_miss in ["/HELP", "/hel", "/helpme", "/exitx"] {
            assert_eq!(
                classify(near_miss),
                Submitted::UnknownCommand {
                    token: near_miss.to_string()
                },
                "{near_miss}"
            );
        }
    }

    #[test]
    fn the_refusal_is_a_pure_function_of_the_name() {
        assert_eq!(
            unknown_command_message("/nonesuch"),
            unknown_command_message("/nonesuch")
        );
        // Exact, and derived: the sentence quotes the registry's own count, so
        // this expectation moves with the palette instead of pinning a word
        // that goes stale the next time a command is added.
        assert_eq!(
            unknown_command_message("/nonesuch"),
            format!(
                "xfx: `/nonesuch` is not an xfx command; /help lists its {} commands",
                SLASH_COMMANDS.len()
            )
        );
    }

    #[test]
    fn a_refusal_never_quotes_back_a_control_character() {
        // A line beginning `/` is echoed by the terminal *and* quoted by xfx.
        // The echo is the terminal's own business; the quote is xfx's, and it
        // must not be a way to make xfx clear the screen, retitle the window,
        // or move the cursor on the user's behalf.
        let hostile = "/\u{1b}[2J\u{1b}]0;pwned\u{7}\u{1b}[H";
        let message = unknown_command_message(hostile);
        assert!(!message.contains('\u{1b}'), "{message:?}");
        assert!(!message.chars().any(char::is_control), "{message:?}");
        assert_eq!(message.lines().count(), 1, "{message:?}");
        assert!(message.contains("/help"), "{message:?}");
    }

    #[test]
    fn a_refusal_is_bounded_however_long_the_mistake_was() {
        let message = unknown_command_message(&format!("/{}", "x".repeat(100_000)));
        assert!(
            message.len() < MAX_QUOTED_COMMAND_BYTES + 128,
            "{message:?}"
        );
        assert!(message.contains('…'), "{message:?}");
        // The guidance is the part worth reading, so it survives the clip.
        assert!(
            message.ends_with(&format!(
                "/help lists its {} commands",
                SLASH_COMMANDS.len()
            )),
            "{message:?}"
        );
    }

    #[test]
    fn a_refusal_keeps_a_multibyte_typo_readable() {
        let message = unknown_command_message("/설명");
        assert!(message.contains("/설명"), "{message}");
        // Clipping is on a character boundary, so a long multibyte token can
        // never produce invalid text.
        let long = unknown_command_message(&format!("/{}", "설".repeat(1000)));
        assert!(long.contains('…'), "{long}");
    }

    #[test]
    fn help_lists_every_command_and_nothing_else() {
        let help = help_text();
        for name in SLASH_COMMANDS {
            assert!(help.contains(name), "help omits {name}: {help}");
        }
        // One line per command, plus the title and the closing note.
        assert_eq!(help.lines().count(), SLASH_COMMANDS.len() + 2, "{help}");
    }

    #[test]
    fn help_advertises_no_deferred_upstream_command() {
        let help = help_text();
        for deferred in [
            "/resume",
            "/status",
            "/login",
            "/logout",
            "/permissions",
            "/models",
            "/provider",
            "/mcp",
            "/undo",
            "/stats",
            "/usage",
            "/image",
            "/reset",
            "/rename",
            "/background",
        ] {
            assert!(!help.contains(deferred), "help advertises {deferred}");
        }
    }

    #[test]
    fn the_clear_sequence_erases_the_screen_and_the_scrollback() {
        assert!(CLEAR_SCREEN.contains("\u{1b}[2J"));
        assert!(CLEAR_SCREEN.contains("\u{1b}[3J"));
        // Never the alternate screen: the scrollback above the shell is the
        // user's, and a shell that borrowed it would have to give it back.
        assert!(!CLEAR_SCREEN.contains("1049"));
    }

    #[test]
    fn the_refusals_say_what_to_do_instead() {
        assert!(NO_TERMINAL.contains("xfx ask"));
        assert!(NO_STORE.contains("--no-save"));
    }

    #[test]
    fn an_idle_interrupt_becomes_an_exit_only_on_the_second_one() {
        let interrupts = Interrupts::new(CancelToken::new());
        assert!(matches!(
            *interrupts.lock(),
            Activity::Idle { consecutive: 0 }
        ));
        // A submitted line puts the count back, which is what keeps two
        // interrupts an hour apart from ending the shell.
        interrupts.line_submitted();
        assert!(matches!(
            *interrupts.lock(),
            Activity::Idle { consecutive: 0 }
        ));
    }

    #[test]
    fn beginning_a_turn_clears_the_previous_turns_cancellation() {
        let cancel = CancelToken::new();
        let interrupts = Interrupts::new(cancel.clone());
        cancel.cancel();
        interrupts.begin_turn();
        assert!(!cancel.is_cancelled(), "the next turn started cancelled");
        assert!(matches!(
            *interrupts.lock(),
            Activity::Running { cancelled: false }
        ));
        interrupts.end_turn();
        assert!(matches!(
            *interrupts.lock(),
            Activity::Idle { consecutive: 0 }
        ));
    }

    #[test]
    fn the_notice_a_cancelled_turn_writes_promises_the_exit_this_table_honours() {
        // `Interrupts::signalled` writes `app::INTERRUPT_NOTICE` on the arm
        // below, so the sentence is a claim about this state machine. The claim
        // it may make is the one the next arm keeps -- a press while the turn is
        // still `Running { cancelled: true }` exits 130 -- and not the one
        // `end_turn` takes away: from `Idle` the next press offers a fresh
        // prompt line, and it takes two of them to leave.
        //
        // Driven through `signalled` rather than by writing the state, so the
        // arm that writes the notice is the arm that is measured. The exiting
        // arm is not called: it is `process::exit`, which a test cannot survive,
        // so what is asserted is the state that reaches it.
        let cancel = CancelToken::new();
        let interrupts = Interrupts::new(cancel.clone());
        interrupts.begin_turn();

        interrupts.signalled();

        assert!(cancel.is_cancelled(), "the turn was not asked to stop");
        assert!(
            matches!(*interrupts.lock(), Activity::Running { cancelled: true }),
            "the notice was written from a state whose next press does not exit"
        );
        interrupts.end_turn();
        assert!(
            matches!(*interrupts.lock(), Activity::Idle { consecutive: 0 }),
            "a turn that ended left the exiting arm armed"
        );
        assert!(
            crate::app::INTERRUPT_NOTICE.contains("before it stops"),
            "the notice promises an exit this table does not honour once the \
             turn has ended: {}",
            crate::app::INTERRUPT_NOTICE
        );
    }

    #[test]
    fn the_canonical_list_and_the_registry_are_one_order() {
        assert_eq!(SLASH_REGISTRY.len(), SLASH_COMMANDS.len());
        for (spec, name) in SLASH_REGISTRY.iter().zip(SLASH_COMMANDS) {
            assert_eq!(spec.name, *name);
            assert_eq!(spec.command.name(), *name);
        }
    }

    #[test]
    fn every_canonical_name_has_exactly_one_spec() {
        for name in SLASH_COMMANDS {
            let command = Slash::parse(name).expect("a canonical name parses");
            let found: Vec<&SlashSpec> = SLASH_REGISTRY
                .iter()
                .filter(|spec| spec.command == command)
                .collect();
            assert_eq!(found.len(), 1, "{name}");
            assert_eq!(slash_spec(command).name, *name);
        }
    }

    #[test]
    fn exit_is_an_alias_for_quit_and_not_a_canonical_name() {
        assert_eq!(
            classify("/exit"),
            Submitted::Command {
                command: Slash::Quit,
                argument: String::new()
            }
        );
        assert!(!SLASH_COMMANDS.contains(&"/exit"));
        assert_eq!(slash_spec(Slash::Quit).aliases, &["/exit"]);
        assert_eq!(Slash::Quit.name(), "/quit");
    }

    #[test]
    fn help_distinguishes_an_alias_without_advertising_another_command() {
        let help = help_text();
        assert!(help.contains("/exit"), "{help}");
        assert_eq!(help.lines().count(), SLASH_COMMANDS.len() + 2, "{help}");
        let named: Vec<&str> = help
            .lines()
            .filter_map(|line| line.split_whitespace().next())
            .filter(|word| word.starts_with('/'))
            .collect();
        assert_eq!(named, SLASH_COMMANDS.to_vec(), "{help}");
    }

    #[test]
    fn only_a_command_that_takes_one_is_declared_to_have_an_argument() {
        // The two that mean something by the rest of the line: `/model <id>`
        // picks among what a provider offers, `/setup <provider>` picks the
        // provider. Every other name is the whole command.
        for spec in SLASH_REGISTRY {
            let takes_one = matches!(spec.name, "/model" | "/setup");
            assert_eq!(spec.has_args, takes_one, "{}", spec.name);
        }
    }

    #[test]
    fn setup_is_the_seventh_canonical_command() {
        // Task 4's one new name. It is canonical rather than an alias: it takes
        // an argument, it has a handler of its own on both surfaces, and it is
        // the only command in the palette that can change which provider a turn
        // talks to.
        assert!(SLASH_COMMANDS.contains(&"/setup"), "{SLASH_COMMANDS:?}");
        assert_eq!(Slash::parse("/setup"), Some(Slash::Setup));
        assert_eq!(Slash::Setup.name(), "/setup");
        assert!(slash_spec(Slash::Setup).has_args, "`/setup <provider>`");
        assert!(help_text().contains("/setup"), "{}", help_text());
    }

    #[test]
    fn the_standalone_catalog_commands_are_still_deferred() {
        // `/setup` is the *only* name this task adds. `/models` and `/provider`
        // are upstream commands xfx does not answer, and the catalog browser
        // this task builds is reached by bare `/model` rather than by either of
        // them -- so advertising one would be advertising a surface that is not
        // there.
        for absent in ["/models", "/provider"] {
            assert!(!SLASH_COMMANDS.contains(&absent), "{absent} is canonical");
            assert_eq!(Slash::parse(absent), None, "{absent} parses");
            assert!(!help_text().contains(absent), "help advertises {absent}");
        }
    }

    #[test]
    fn the_unknown_command_refusal_counts_the_registry_rather_than_a_literal() {
        // The refusal used to say "the six it has" as a literal, which is a
        // second declaration of the palette's size and therefore a second thing
        // that can drift out of step with it. It is derived now: a name added
        // to the registry changes this sentence without anybody editing it.
        let refusal = unknown_command_message("/nonesuch");
        assert!(
            refusal.contains(&SLASH_COMMANDS.len().to_string()),
            "the refusal does not carry the registry's own count: {refusal}"
        );
        assert!(
            !refusal.contains("six"),
            "the refusal still spells a count as a literal word: {refusal}"
        );
        // Still one bounded, sanitized line: deriving the count must not have
        // moved the guidance off the end of it.
        assert!(refusal.ends_with("commands"), "{refusal}");
    }
}
