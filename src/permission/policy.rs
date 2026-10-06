//! Who decides, on what evidence, and what a decision is worth.
//!
//! The decision is split in two on purpose:
//!
//! - [`PermissionSession::evaluate`] is a pure function of the mode, the
//!   configured rules, the session grants, and the *structure of the prepared
//!   action*. It reads no file, runs no process, and asks no human. It can
//!   therefore be tested exhaustively, and it cannot change the thing it is
//!   judging.
//! - [`PermissionSession::decide_with_feedback`] adds the only side effect a decision is
//!   allowed to have: asking the user, and remembering an answer of "always".
//!
//! Everything else follows from that split. `ask` with no approval channel is
//! [`DenyCause::NoApprovalChannel`], because the only honest answer to a
//! question nobody can be asked is no. `auto` never consults a human, so it can
//! only admit actions whose structure already proves they are bounded and
//! recoverable. `yolo` skips the whole thing and says so out loud
//! (`vercel-labs/fx@580a0c5d src/core/permissions/permission_gate.zig:72-107`).

use std::fmt;
use std::io::{self, IsTerminal, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, PoisonError, RwLock};

use crate::config::PermissionMode;

use super::rules::{RuleAction, RuleSet};

use super::authority::{
    issue, ApprovalDiff, AuthorityError, AuthorityLedger, CommandPlan, ExecutionAuthority,
    MutationPlan, TargetScope,
};
use super::command::CommandEffect;

/// The warning `yolo` prints before anything runs.
///
/// It names all three things that are off, because "yolo mode" alone reads as a
/// speed setting rather than as the removal of every check.
pub const YOLO_WARNING: &str =
    "xfx: yolo mode is on -- tool calls run with no permission check, no approval prompt, and no sandbox.";

// ---------------------------------------------------------------------------
// rules and grants
// ---------------------------------------------------------------------------

/// One approval the user gave for the rest of this session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grant {
    pub tool: String,
    pub target: String,
}

impl Grant {
    pub fn new(tool: impl Into<String>, target: impl Into<String>) -> Self {
        Self {
            tool: tool.into(),
            target: target.into(),
        }
    }

    fn matches(&self, tool: &str, target: &str) -> bool {
        self.tool == tool && self.target == target
    }
}

/// The configured table a session decides with, shared with whatever edits it.
///
/// `/allowlist` changes the table on the thread that holds the terminal, and
/// upstream's change is in force for the very next tool call
/// (`replaceEffectivePermissionRules`, `session_commands.zig:927-941`, under
/// the authority mutex), including a call of a turn already running. A session
/// that copied the table when it was built could not do that; one holding this
/// reads it at every decision.
///
/// A whole table is swapped at once and read as one `Arc`, so a decision never
/// sees half of an edit.
#[derive(Debug, Clone, Default)]
pub struct LiveRules(Arc<RwLock<Arc<RuleSet>>>);

impl LiveRules {
    /// A handle on `rules`.
    pub fn new(rules: RuleSet) -> Self {
        Self(Arc::new(RwLock::new(Arc::new(rules))))
    }

    /// The table in force now.
    pub fn current(&self) -> Arc<RuleSet> {
        Arc::clone(&self.0.read().unwrap_or_else(PoisonError::into_inner))
    }

    /// Puts `rules` in force for every session holding this handle, from its
    /// next decision on.
    pub fn replace(&self, rules: RuleSet) {
        *self.0.write().unwrap_or_else(PoisonError::into_inner) = Arc::new(rules);
    }
}

/// What a configured table makes of one prepared action, before anything else
/// is consulted.
#[derive(Debug, Clone, PartialEq, Eq)]
enum RuleVerdict {
    /// A rule denies one of the action's targets; the first such target.
    Denied(String),
    /// A rule asks about a target no session grant covers.
    Ask,
    /// A rule allows every target (or asks about one a grant covers).
    Allowed { by_grant: bool },
    /// Some target no rule decides.
    Undecided,
    /// A table is configured and the change carries no resolved targets to
    /// judge it against.
    Unresolved,
}

// ---------------------------------------------------------------------------
// what is being judged
// ---------------------------------------------------------------------------

/// A prepared action, offered to policy for judgement.
///
/// It borrows the plan rather than owning it, so the same plan is what gets
/// minted afterwards. There is no path where policy judges one thing and
/// execution runs another.
#[derive(Debug, Clone, Copy)]
pub enum ProposedAction<'a> {
    Mutation(&'a MutationPlan),
    Command(&'a CommandPlan),
}

impl ProposedAction<'_> {
    /// The advertised tool name, which is what a rule or a grant names.
    pub fn tool(&self) -> &'static str {
        match self {
            Self::Mutation(plan) => plan.kind().tool(),
            Self::Command(_) => "terminal",
        }
    }

    /// The category, the tool name and every candidate a configured rule is
    /// matched against (`requestPermissionOutcomeResolved`,
    /// `tool_admission.zig:1386-1403`; `decideFileTargetPolicy`,
    /// `permissions.zig:803-830`).
    ///
    /// A command is judged by its **text alone**: upstream strips the
    /// directory before matching (`patternForRuleMatch`, `:1501-1516`), so a
    /// rule is about what runs, wherever it runs. A change to a file is judged
    /// at every path it touches -- the file, its parent, every directory it
    /// creates -- and a deny on any of them stops it.
    ///
    /// The first candidate is the action's own target, the one an exact
    /// session grant is about. `None` for a change whose executor did not
    /// resolve its targets ([`MutationPlan::rule_targets`]).
    fn rule_subject(&self) -> (&'static str, &'static str, Option<Vec<Vec<u8>>>) {
        match self {
            Self::Mutation(plan) => (
                super::rules::EDIT,
                plan.kind().tool(),
                plan.rule_targets().map(<[Vec<u8>]>::to_vec),
            ),
            Self::Command(plan) => (
                super::rules::BASH,
                super::rules::COMMAND_TOOL,
                Some(vec![plan.command().as_bytes().to_vec()]),
            ),
        }
    }

    /// The exact thing being asked for, and the key a grant matches.
    ///
    /// A command's key includes its working directory. Without it, approving
    /// `cat notes.md` in the workspace would also approve `cat notes.md` in a
    /// directory `--add-dir` opened, which is a different file and a different
    /// question.
    ///
    /// A mutation's key is the plan's **canonical absolute target**, never its
    /// display path. The display path is workspace-relative, and a grant
    /// outlives the process that gave it: `xfx ask --resume-id <id>` may be run
    /// from a different directory, which rebinds the session's workspace, and a
    /// relative key would then silently match a different file with the same
    /// name in the new tree. An approval is an answer about one file, so the key
    /// names that file. The *prose* a human is asked -- [`Self::summary`] and
    /// [`Self::always_scope`] -- keeps the friendly relative path, because that
    /// is the name the user and the model both use.
    ///
    /// The rest of a command plan -- the route and the environment -- is a
    /// function of the command text and the process environment, so (tool,
    /// command, cwd) determines the whole plan within one run. The fingerprint
    /// test `a_command_fingerprint_covers_the_command_the_cwd_and_the_environment`
    /// pins that.
    pub fn target(&self) -> String {
        match self {
            Self::Mutation(plan) => plan.target().to_string_lossy().into_owned(),
            Self::Command(plan) => format!("{} in {}", plan.command(), plan.cwd().display()),
        }
    }

    /// One sentence a human can answer yes or no to.
    ///
    /// It has to disclose the *content* of the change, not only its shape. "Edit
    /// notes.md" is not a question anyone can answer: the whole risk is in which
    /// bytes are going away and which are arriving. So an edit shows a bounded
    /// before and after, a write shows a bounded preview with its size, and both
    /// name digests so two similar-looking changes can be told apart.
    pub fn summary(&self) -> String {
        match self {
            Self::Mutation(plan) => {
                let target = plan.display();
                let existing = match plan.preimage() {
                    super::Preimage::Absent => "which does not exist yet".to_string(),
                    super::Preimage::Present { identity, hash } => {
                        format!("now {} bytes, sha256 {}", identity.size, hash.short())
                    }
                };
                match plan.kind() {
                    super::MutationKind::Write => format!(
                        "write {} bytes to `{target}` ({existing}), sha256 {}: \"{}\"",
                        plan.staged_bytes().len(),
                        plan.after_hash().short(),
                        plan.preview()
                    ),
                    super::MutationKind::Edit => {
                        let change = match plan.excerpt() {
                            Some(excerpt) => {
                                format!("replace \"{}\" with \"{}\"", excerpt.before, excerpt.after)
                            }
                            // Unreachable for `edit_file`, which always supplies
                            // one; kept honest rather than asserted away.
                            None => format!("rewrite it as {} bytes", plan.staged_bytes().len()),
                        };
                        format!(
                            "edit `{target}` ({existing}): {change}, leaving {} bytes sha256 {}",
                            plan.staged_bytes().len(),
                            plan.after_hash().short()
                        )
                    }
                    super::MutationKind::CreateFolder => {
                        format!("create the directory `{target}`")
                    }
                }
            }
            Self::Command(plan) => format!("run `{}` in {}", plan.command(), plan.display_cwd()),
        }
    }

    /// The whole of both sides of the change, bounded, when there are two.
    ///
    /// Taken from the plan that is **being judged**, never rebuilt beside it. A
    /// diff computed a second time -- by the prompt, or by whatever renders it
    /// -- would be a second reading of the change, and two readings are two
    /// answers to the only question that matters here: what is being approved.
    pub fn diff(&self) -> Option<ApprovalDiff> {
        match self {
            Self::Mutation(plan) => plan.diff().cloned(),
            // A command has no before and after. What it would do is the command
            // itself, and [`Self::summary`] quotes that whole.
            Self::Command(_) => None,
        }
    }

    /// What answering "always" would additionally allow, in plain words.
    ///
    /// A grant is keyed by tool and target, and for a file that means *the path*,
    /// not this particular content. A prompt that showed one diff and silently
    /// bought permission for every future diff to the same file would be
    /// misleading, so the prompt says which of the two it is.
    pub fn always_scope(&self) -> String {
        match self {
            Self::Mutation(plan) => format!(
                "allow every future {} to `{}` for the rest of this session, whatever its contents",
                plan.kind().tool(),
                plan.display()
            ),
            Self::Command(plan) => format!(
                "allow every future run of exactly `{}` in {} for the rest of this session",
                plan.command(),
                plan.display_cwd()
            ),
        }
    }

    /// What the question is about, in the pieces a review lays out
    /// ([`ApprovalSubject`]): taken from the plan being judged, for the reason
    /// [`Self::diff`] is.
    pub fn subject(&self) -> ApprovalSubject {
        match self {
            Self::Mutation(plan) => ApprovalSubject::Path(plan.display().to_string()),
            Self::Command(plan) => ApprovalSubject::Command {
                command: plan.command().to_string(),
                cwd: plan.display_cwd().to_string(),
            },
        }
    }

    /// Whether `auto` may admit this without asking anybody, and why not.
    ///
    /// `auto` has no human to consult, so the only actions it can admit are the
    /// ones whose *structure* already bounds them: a write of bounded size to a
    /// path inside the workspace the user is already sitting in, or a command
    /// the grammar reduced to a read-only argv.
    fn auto_admission(&self) -> Result<(), String> {
        match self {
            Self::Mutation(plan) => {
                if plan.scope() != TargetScope::PrimaryWorkspace {
                    return Err(format!(
                        "auto mode only changes files inside the workspace; `{}` is in an additional root that `--add-dir` opened for reading",
                        plan.display()
                    ));
                }
                Ok(())
            }
            Self::Command(plan) => match plan.effect() {
                CommandEffect::DirectReadOnly { .. } => Ok(()),
                CommandEffect::Denied(effect) => Err(format!(
                    "`{}` is not admitted in auto mode because {}; ask the user to approve it, or use a command that only reads",
                    plan.command(),
                    effect.describe()
                )),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// decisions
// ---------------------------------------------------------------------------

/// Which authority admitted an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllowSource {
    /// A configured allow rule.
    ConfiguredRule,
    /// An approval the user already gave this session.
    SessionGrant,
    /// The user said yes, for this call.
    InteractiveOnce,
    /// The user said yes, for the rest of the session.
    InteractiveAlways,
    /// The structure of the action satisfied `auto`.
    AutoMode,
    /// No check ran.
    Yolo,
}

/// Why an action was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenyCause {
    /// A configured deny rule matched.
    ConfiguredRule,
    /// `ask` mode with nowhere to ask.
    NoApprovalChannel,
    /// The user said no.
    UserDenied,
    /// `auto` mode, and the action is outside what it admits.
    NotAutoAdmissible,
    /// The approval channel existed and failed.
    ApprovalChannelFailed,
}

/// What policy concluded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyDecision {
    Allow {
        source: AllowSource,
    },
    /// A human has to answer. Only [`PermissionSession::evaluate`] returns this;
    /// [`PermissionSession::decide_with_feedback`] always resolves it.
    Prompt,
    Deny {
        cause: DenyCause,
        reason: String,
    },
}

// ---------------------------------------------------------------------------
// the approval channel
// ---------------------------------------------------------------------------

/// One question put to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalRequest {
    /// The advertised tool asking.
    pub tool: &'static str,
    /// The exact target, which is also what a grant would record.
    pub target: String,
    /// One sentence describing what would happen, including a bounded excerpt
    /// of the content being changed.
    pub summary: String,
    /// What answering "always" would additionally allow.
    pub always_scope: String,
    /// The whole of both sides of the change, bounded, when the action has two.
    ///
    /// Beside the summary rather than inside it: the summary is a sentence and
    /// stays one, on every surface. This is the payload a review with room for
    /// it can show, and it is `None` for every action that has no honest pair --
    /// a command, a whole-file write, a directory. A surface that has only rows
    /// ignores it and asks exactly the question it asked before.
    pub diff: Option<ApprovalDiff>,
    /// What the question is about, in the pieces a review surface lays out on
    /// rows of their own.
    ///
    /// Beside [`Self::summary`] rather than parsed back out of it: the summary
    /// is one sentence that quotes these, and a surface that recovered a path
    /// or a command by splitting prose would be reading a second, guessed
    /// version of what is being approved. Presentation only -- a grant is keyed
    /// by [`Self::tool`] and [`Self::target`], and nothing here is consulted by
    /// the decision.
    pub subject: ApprovalSubject,
}

/// The thing a question is about, as a reviewer names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalSubject {
    /// A file or folder, by the workspace-relative name the user and the model
    /// both use ([`MutationPlan::display`]).
    Path(String),
    /// A command exactly as it would run, and the directory it would run in,
    /// named the way the always-scope names it ([`CommandPlan::display_cwd`]).
    Command { command: String, cwd: String },
}

/// What the user answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalAnswer {
    /// Allow this call only.
    Once,
    /// Allow this exact tool and target for the rest of the session.
    Always,
    /// Refuse.
    Deny,
}

/// What the user answered, and anything they said about it.
///
/// The two travel together and mean different things. The answer is authority.
/// The feedback is *context*, delivered to the model after the result, and it
/// changes neither the arguments that ran nor the authority that let them
/// (`vercel-labs/fx@580a0c5d src/core/permissions/tool_admission.zig:2039-2050`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalResponse {
    pub answer: ApprovalAnswer,
    /// Nonempty when the user amended the decision. Never `Some("")`: a blank
    /// draft is an absent one, decided at the surface that owns the draft.
    pub feedback: Option<String>,
}

impl ApprovalResponse {
    /// An answer with nothing said about it.
    pub fn plain(answer: ApprovalAnswer) -> Self {
        Self {
            answer,
            feedback: None,
        }
    }
}

/// A resolved decision and whatever the user said while resolving it.
///
/// Returned by value, per call. There is deliberately no field anywhere that
/// holds feedback between calls: a session-scoped slot would survive an early
/// return and be read by the next tool as though it had been typed at that one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decided {
    pub decision: PolicyDecision,
    pub feedback: Option<String>,
}

/// Something that can put a question to a human and get an answer back.
///
/// A missing prompter is not "allow by default": [`PermissionSession::decide_with_feedback`]
/// treats it as [`DenyCause::NoApprovalChannel`]. That is the whole reason this
/// is an `Option` rather than a trait object with a permissive default.
///
/// `Send` because a session lives behind a `Mutex` that a
/// [`crate::tools::ToolContext`] shares across the turn; a prompter that could
/// not move between threads would make the whole context single-threaded.
pub trait ApprovalPrompter: Send {
    fn request(&mut self, request: &ApprovalRequest) -> io::Result<ApprovalAnswer>;

    /// The same question, on a channel that can also carry a sentence.
    ///
    /// Defaulted rather than required: a prompter that cannot offer a draft --
    /// [`TtyPrompter`]'s `y/a/n` line, every test fixture -- is *correct* when
    /// it reports none. There is no recursion hazard: this default calls
    /// `request`, which every implementor has to write, so the call terminates
    /// in that implementor's own body.
    fn respond(&mut self, request: &ApprovalRequest) -> io::Result<ApprovalResponse> {
        Ok(ApprovalResponse::plain(self.request(request)?))
    }
}

/// The real approval channel: a question on stderr, an answer from the terminal.
///
/// stderr rather than stdout, because `ask --json` puts a machine-readable
/// stream on stdout and a prompt written there would corrupt it.
#[derive(Debug)]
pub struct TtyPrompter {
    _private: (),
}

impl TtyPrompter {
    /// A prompter, but only when there is a real terminal on both ends.
    ///
    /// Both ends matter: a question xfx cannot show is as useless as an answer
    /// it cannot read, and either one alone would let a piped invocation hang
    /// forever waiting for a person who is not there.
    pub fn available() -> Option<Self> {
        if io::stdin().is_terminal() && io::stderr().is_terminal() {
            Some(Self { _private: () })
        } else {
            None
        }
    }
}

impl ApprovalPrompter for TtyPrompter {
    fn request(&mut self, request: &ApprovalRequest) -> io::Result<ApprovalAnswer> {
        let mut stderr = io::stderr();
        loop {
            write!(
                stderr,
                "\nxfx wants to {}\n  [y] yes, once\n  [a] always -- {}\n  [n] no\n> ",
                request.summary, request.always_scope
            )?;
            stderr.flush()?;

            let mut answer = String::new();
            // Zero bytes means the terminal went away mid-question. Treating
            // that as "no answer" rather than as an empty answer keeps the loop
            // from spinning on a closed stdin.
            if io::stdin().read_line(&mut answer)? == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "the terminal closed before answering",
                ));
            }
            match answer.trim().to_ascii_lowercase().as_str() {
                "y" | "yes" => return Ok(ApprovalAnswer::Once),
                "a" | "always" => return Ok(ApprovalAnswer::Always),
                "n" | "no" | "" => return Ok(ApprovalAnswer::Deny),
                _ => continue,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// the live authority
// ---------------------------------------------------------------------------

/// The mode a person has chosen, and how many times they have reset, shared
/// between the thread that hears the keystroke and every session it governs.
///
/// Upstream's tool calls read the root permission mode **at the moment each
/// one is judged** (`agentSnapshotRootPermissionMode`,
/// `vercel-labs/fx@c1db9191 src/core/app/app_callbacks.zig:976-979`), so a
/// Shift-Tab to `ask` in the middle of a turn gates that turn's next call
/// (`tests/e2e/yolo-permission-mode.test.ts:373-450`). A session that copied
/// the mode when it was built could not do that; one holding this reads it
/// per decision.
///
/// **One word, and lock-free, for two reasons.** The session's own lock is
/// held across an approval prompt (`decide_with_feedback` asks while the tool
/// holds it), so the thread that answers the prompt must never be the thread
/// that waits for that lock: a change of mode made through it would deadlock
/// against the question it is trying to stop. And the mode and the reset count
/// travel together, so a reset -- `ask` *and* every earlier grant gone -- is
/// one store that no reader can see half of.
#[derive(Debug, Clone)]
pub struct LiveAuthority(Arc<AtomicU64>);

/// Where the mode sits in the word; the reset count is the rest of it.
const MODE_BITS: u32 = 8;
const MODE_MASK: u64 = (1 << MODE_BITS) - 1;

impl LiveAuthority {
    /// A live authority in `mode`, never reset.
    pub fn new(mode: PermissionMode) -> Self {
        Self(Arc::new(AtomicU64::new(Self::pack(mode, 0))))
    }

    /// The mode in force now.
    pub fn mode(&self) -> PermissionMode {
        Self::unpack(self.0.load(Ordering::Acquire)).0
    }

    /// How many resets have happened. Grants given before the latest one are
    /// no longer in force anywhere this authority is attached.
    pub fn resets(&self) -> u64 {
        Self::unpack(self.0.load(Ordering::Acquire)).1
    }

    /// Puts every attached session in `mode`, from its next decision on.
    pub fn set_mode(&self, mode: PermissionMode) {
        let _ = self
            .0
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |word| {
                Some(Self::pack(mode, Self::unpack(word).1))
            });
    }

    /// `/permissions reset`: `ask`, and every grant given so far out of force,
    /// in one store. Answers the new reset count.
    pub fn reset(&self) -> u64 {
        let previous = self
            .0
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |word| {
                Some(Self::pack(
                    PermissionMode::Ask,
                    Self::unpack(word).1.saturating_add(1),
                ))
            })
            .unwrap_or_else(|word| word);
        Self::unpack(previous).1.saturating_add(1)
    }

    fn pack(mode: PermissionMode, resets: u64) -> u64 {
        let code = match mode {
            PermissionMode::Ask => 0,
            PermissionMode::Auto => 1,
            PermissionMode::Yolo => 2,
        };
        (resets.min(u64::MAX >> MODE_BITS) << MODE_BITS) | code
    }

    fn unpack(word: u64) -> (PermissionMode, u64) {
        let mode = match word & MODE_MASK {
            1 => PermissionMode::Auto,
            2 => PermissionMode::Yolo,
            // `ask` for the code it is, and for any code this build did not
            // write: the restrictive reading is the safe one.
            _ => PermissionMode::Ask,
        };
        (mode, word >> MODE_BITS)
    }
}

// ---------------------------------------------------------------------------
// the session
// ---------------------------------------------------------------------------

/// The permission state one run of xfx carries.
///
/// Everything that can say yes lives here: the mode, the configured rules, the
/// approvals given so far, the channel that can ask for more, and the ledger of
/// authorities issued. Nothing outside this type can mint one.
pub struct PermissionSession {
    mode: PermissionMode,
    rules: LiveRules,
    grants: Vec<Grant>,
    prompter: Option<Box<dyn ApprovalPrompter>>,
    ledger: AuthorityLedger,
    /// The durable session id an "always" answer will outlive this process in.
    ///
    /// `None` for a turn nothing is recording, where "always" really does mean
    /// "until this command exits".
    durable_session: Option<String>,
    /// The mode and reset count a person can change while this session is
    /// in use ([`LiveAuthority`]), when a front end attached one.
    live: Option<LiveAuthority>,
    /// The reset count [`Self::grants`] were given under. Grants from an
    /// earlier count are out of force the moment the live count moves past
    /// it, and dropped by the next `&mut` call.
    grants_epoch: u64,
}

impl PermissionSession {
    /// A session in `mode` with no rules, no grants, and no approval channel.
    ///
    /// No channel is the safe default: a session built without one refuses every
    /// `ask`-mode mutation rather than assuming a terminal is there.
    pub fn new(mode: PermissionMode) -> Self {
        Self {
            mode,
            rules: LiveRules::default(),
            grants: Vec::new(),
            prompter: None,
            ledger: AuthorityLedger::new(),
            durable_session: None,
            live: None,
            grants_epoch: 0,
        }
    }

    /// The same session, governed by `live` from its next decision on: its
    /// mode is read from it at every decision, and a reset there takes every
    /// grant given before it out of force at once.
    ///
    /// The grants already held count as given under the reset count `live`
    /// has now, which is what lets a resumed session restore its log's grants
    /// after attaching.
    pub fn with_live(mut self, live: LiveAuthority) -> Self {
        self.grants_epoch = live.resets();
        self.live = Some(live);
        self
    }

    /// The same session with a way to ask the user.
    pub fn with_prompter(mut self, prompter: Box<dyn ApprovalPrompter>) -> Self {
        self.prompter = Some(prompter);
        self
    }

    /// The same session, deciding with `rules` and nothing that edits them.
    pub fn with_rules(mut self, rules: RuleSet) -> Self {
        self.rules = LiveRules::new(rules);
        self
    }

    /// The same session, deciding with whatever `rules` holds at the moment
    /// of each decision ([`LiveRules`]).
    pub fn with_live_rules(mut self, rules: LiveRules) -> Self {
        self.rules = rules;
        self
    }

    /// The configured table in force now.
    pub fn rules(&self) -> Arc<RuleSet> {
        self.rules.current()
    }

    /// The same session, recording its approvals into the durable session `id`.
    ///
    /// This changes what the word "always" is worth, so it changes what the
    /// prompt says. An approval that survives the process is a different
    /// question from one that does not, and the user has to be asked the
    /// question they are actually answering -- including the id, because that is
    /// the exact thing a later `xfx ask --resume-id <id>` will reuse it for.
    pub fn with_durable_session(mut self, id: impl Into<String>) -> Self {
        self.durable_session = Some(id.into());
        self
    }

    /// What an "always" answer buys, in this session's terms.
    fn always_scope_for(&self, action: ProposedAction<'_>) -> String {
        match &self.durable_session {
            Some(id) => format!(
                "{}, and in every later `xfx ask --resume-id {id}` of this saved session",
                action.always_scope()
            ),
            None => format!(
                "{}; this turn is not being recorded, so the approval ends with this command",
                action.always_scope()
            ),
        }
    }

    /// The mode the next decision is made in: the live one when a front end
    /// attached one, this session's own otherwise.
    pub fn mode(&self) -> PermissionMode {
        match &self.live {
            Some(live) => live.mode(),
            None => self.mode,
        }
    }

    /// Puts this session in `mode` from its next decision on.
    ///
    /// What `/permissions <mode>` and Shift-Tab change on a conversation that
    /// already exists: the grants, the rules, the prompter and the ledger are
    /// the conversation's and stay, as upstream's `setMode` changes only the
    /// engine's mode (`app_permission_runtime.zig:73-85`).
    ///
    /// **This session's own copy only.** A session with a [`LiveAuthority`]
    /// takes its mode from that, which the front end has already moved; the
    /// copy is what a session without one decides with, and what one with one
    /// is recorded as when it is written down.
    pub fn set_mode(&mut self, mode: PermissionMode) {
        self.mode = mode;
    }

    /// Forgets every "always" given before the reset being applied, returning
    /// how many.
    ///
    /// `/permissions reset`'s second half (`permission_engine.clear`,
    /// `app_permission_runtime.zig:144-159`): the grants go, so the next action
    /// that needed one is asked about again. The configured rules are not
    /// grants and are kept.
    ///
    /// Without a [`LiveAuthority`] every grant this session holds predates the
    /// reset, and all of them go. With one, the reset already happened there --
    /// and took every earlier grant out of force at that moment -- so what goes
    /// here is exactly those: an "always" the person gave *after* resetting,
    /// in the turn that was running when they did, is theirs and stays.
    pub fn clear_grants(&mut self) -> usize {
        if self.live.is_some() {
            return self.sync_grants();
        }
        std::mem::take(&mut self.grants).len()
    }

    /// Drops the grants a reset on the live authority has taken out of
    /// force, returning how many.
    fn sync_grants(&mut self) -> usize {
        let Some(live) = &self.live else {
            return 0;
        };
        let now = live.resets();
        if now == self.grants_epoch {
            return 0;
        }
        self.grants_epoch = now;
        std::mem::take(&mut self.grants).len()
    }

    /// The approvals in force, in the order they were given.
    ///
    /// None at all once the live authority has been reset since they were
    /// given: what a reset took away is gone from every answer this session
    /// gives -- a decision, a listing, a count -- from the moment it happened,
    /// not from the next time something mutable touched this session.
    pub fn grants(&self) -> &[Grant] {
        match &self.live {
            Some(live) if live.resets() != self.grants_epoch => &[],
            _ => &self.grants,
        }
    }

    /// Records an approval directly. Used by the approval flow and by a caller
    /// restoring a session's grants.
    pub fn grant(&mut self, grant: Grant) {
        self.sync_grants();
        if !self
            .grants
            .iter()
            .any(|existing| existing.matches(&grant.tool, &grant.target))
        {
            self.grants.push(grant);
        }
    }

    pub fn has_prompter(&self) -> bool {
        self.prompter.is_some()
    }

    /// Judges `action` against everything already known, and nothing else.
    ///
    /// Pure: no I/O, no prompt, no recorded state. The order is deliberate --
    /// an explicit denial outranks an explicit allowance, which outranks a
    /// session grant, which outranks whatever the mode would have said -- so a
    /// user who wrote a rule is never overruled by a default.
    pub fn evaluate(&self, action: ProposedAction<'_>) -> PolicyDecision {
        // Read once, so the two places below cannot see two different modes.
        let mode = self.mode();
        // `yolo` is documented as skipping policy, so it skips policy. Honoring
        // rules here would make the flag mean something narrower than it says.
        if mode == PermissionMode::Yolo {
            return PolicyDecision::Allow {
                source: AllowSource::Yolo,
            };
        }

        let tool = action.tool();
        let target = action.target();
        let granted = self
            .grants()
            .iter()
            .any(|grant| grant.matches(tool, &target));

        match self.rule_verdict(action, granted) {
            RuleVerdict::Denied(candidate) => {
                return PolicyDecision::Deny {
                    cause: DenyCause::ConfiguredRule,
                    reason: format!(
                        "a configured permission rule denies `{tool}` for `{candidate}`"
                    ),
                }
            }
            // In every mode but yolo, `auto` included: a rule that asks is the
            // person saying they want to be asked, and no structural
            // admission outranks that (`fileMutationTargetsContainConfiguredAsk`,
            // `tool_admission.zig:645-661`; `configured_ask`, `:1430-1444`).
            RuleVerdict::Ask => return PolicyDecision::Prompt,
            RuleVerdict::Allowed { by_grant } => {
                return PolicyDecision::Allow {
                    source: if by_grant {
                        AllowSource::SessionGrant
                    } else {
                        AllowSource::ConfiguredRule
                    },
                }
            }
            // Fail closed: a plan built without the targets its executor
            // resolves would otherwise be judged by a spelling of them.
            RuleVerdict::Unresolved => {
                return PolicyDecision::Deny {
                    cause: DenyCause::ConfiguredRule,
                    reason: format!(
                        "`{tool}` for `{target}` cannot be judged against the configured permission rules: its targets were not resolved"
                    ),
                }
            }
            RuleVerdict::Undecided => {}
        }
        if granted {
            return PolicyDecision::Allow {
                source: AllowSource::SessionGrant,
            };
        }

        match mode {
            PermissionMode::Yolo => PolicyDecision::Allow {
                source: AllowSource::Yolo,
            },
            PermissionMode::Auto => match action.auto_admission() {
                Ok(()) => PolicyDecision::Allow {
                    source: AllowSource::AutoMode,
                },
                Err(reason) => PolicyDecision::Deny {
                    cause: DenyCause::NotAutoAdmissible,
                    reason,
                },
            },
            PermissionMode::Ask => PolicyDecision::Prompt,
        }
    }

    /// What the configured table says about every target of `action`, with
    /// `granted` -- an exact session grant for it -- covering a target a rule
    /// asks about, as upstream's grants cover an `ask` (`:1393-1400`).
    ///
    /// **The grant covers the action's own target and nothing else**: an
    /// exact "always" for `/w/a.txt` is not an answer about `.` or about a
    /// directory the write creates, as upstream checks each target's grant
    /// on its own (`decideFileTargetPolicy`, `permissions.zig:803-830`).
    fn rule_verdict(&self, action: ProposedAction<'_>, granted: bool) -> RuleVerdict {
        let rules = self.rules.current();
        if rules.is_empty() {
            return RuleVerdict::Undecided;
        }
        let (permission, rule_tool, candidates) = action.rule_subject();
        let Some(candidates) = candidates else {
            return RuleVerdict::Unresolved;
        };
        let mut ask = false;
        let mut by_grant = false;
        let mut decided = !candidates.is_empty();
        for (index, candidate) in candidates.iter().enumerate() {
            match rules.decide(permission, Some(rule_tool), candidate) {
                Some(RuleAction::Deny) => {
                    return RuleVerdict::Denied(String::from_utf8_lossy(candidate).into_owned())
                }
                Some(RuleAction::Allow) => {}
                Some(RuleAction::Ask) if granted && index == 0 => by_grant = true,
                Some(RuleAction::Ask) => ask = true,
                None => decided = false,
            }
        }
        if ask {
            RuleVerdict::Ask
        } else if decided {
            RuleVerdict::Allowed { by_grant }
        } else {
            RuleVerdict::Undecided
        }
    }

    /// Whether a read-only tool may read `candidate` -- the canonical path it
    /// resolved, as [`super::rules::rule_path`] names it -- by the configured
    /// table (`requestPermissionOutcomeResolved` for a tool with a permission
    /// contract and no approval requirement, `tool_admission.zig:1386-1500`).
    ///
    /// Reading is free unless a rule says otherwise: a deny refuses, as
    /// upstream's does. A rule that **asks** refuses too, with a reason that
    /// names it, where upstream would put the question to the person: xfx has
    /// no review for a read, and refusing is the side of that difference that
    /// grants nothing upstream would not. `yolo` reads everything, as it
    /// skips every other check.
    pub fn judge_read(&self, tool: &str, candidate: &[u8]) -> Result<(), String> {
        if self.mode() == PermissionMode::Yolo {
            return Ok(());
        }
        let permission = super::rules::permission_name_for_tool(tool);
        let shown = String::from_utf8_lossy(candidate);
        match self.rules.current().decide(permission, Some(tool), candidate) {
            Some(RuleAction::Deny) => Err(format!(
                "a configured permission rule denies `{tool}` for `{shown}`"
            )),
            Some(RuleAction::Ask) => Err(format!(
                "a configured permission rule asks before `{tool}` reads `{shown}`, and xfx does not ask about reads; change the rule to allow it"
            )),
            Some(RuleAction::Allow) | None => Ok(()),
        }
    }

    /// Evaluates `action` and, when a human is needed, asks one.
    ///
    /// Never returns [`PolicyDecision::Prompt`]: by the time this returns, the
    /// question has either been answered or established to be unanswerable.
    ///
    /// Named for what it carries rather than `decide`, because a caller that
    /// kept the shorter name would silently drop the user's sentence. Every
    /// path that never asked anybody -- a rule, a grant, `auto`, `yolo` -- has
    /// no sentence to carry and says so with `feedback: None`.
    pub fn decide_with_feedback(&mut self, action: ProposedAction<'_>) -> Decided {
        self.sync_grants();
        match self.evaluate(action) {
            PolicyDecision::Prompt => self.ask(action),
            decision => Decided {
                decision,
                feedback: None,
            },
        }
    }

    /// Puts the question to the user, or reports that there is nobody to ask.
    fn ask(&mut self, action: ProposedAction<'_>) -> Decided {
        let tool = action.tool();
        let target = action.target();
        // Built before the prompter is borrowed: the question depends on the
        // session's durable scope, and the answer is what mutates the session.
        let request = ApprovalRequest {
            tool,
            target: target.clone(),
            summary: action.summary(),
            always_scope: self.always_scope_for(action),
            diff: action.diff(),
            subject: action.subject(),
        };
        if self.prompter.is_none() {
            // Nobody was asked, so nobody said anything. Which question went
            // unasked is named: a rule that asks is not `ask` mode, and
            // `--auto` does not answer it.
            let configured = self.mode() != PermissionMode::Ask
                || self.rule_verdict(action, false) == RuleVerdict::Ask;
            let reason = if configured {
                format!(
                    "a configured permission rule asks before `{tool}` on `{target}`, and this run has no approval channel; rerun in a terminal, or change the rule"
                )
            } else {
                format!(
                    "`ask` mode needs an interactive approval for `{tool}` on `{target}`, and this run has no approval channel; rerun in a terminal, or use --auto for bounded workspace changes"
                )
            };
            return Decided {
                decision: PolicyDecision::Deny {
                    cause: DenyCause::NoApprovalChannel,
                    reason,
                },
                feedback: None,
            };
        }
        let prompter = self
            .prompter
            .as_mut()
            .expect("a prompter was checked for above");
        match prompter.respond(&request) {
            Ok(ApprovalResponse { answer, feedback }) => {
                let decision = match answer {
                    ApprovalAnswer::Once => PolicyDecision::Allow {
                        source: AllowSource::InteractiveOnce,
                    },
                    ApprovalAnswer::Always => {
                        // Unchanged, and deliberately: the grant is keyed by
                        // tool and target. Feedback is not part of that key and
                        // cannot be -- it is context, not authority.
                        self.grant(Grant::new(tool, target));
                        PolicyDecision::Allow {
                            source: AllowSource::InteractiveAlways,
                        }
                    }
                    ApprovalAnswer::Deny => PolicyDecision::Deny {
                        cause: DenyCause::UserDenied,
                        reason: format!("you declined `{tool}` for `{target}`"),
                    },
                };
                // Carried beside the decision, never folded into it: the
                // answer is authority and this is context, and the two are
                // read by different things.
                Decided { decision, feedback }
            }
            // A channel that failed did not deliver a sentence.
            Err(err) => Decided {
                decision: PolicyDecision::Deny {
                    cause: DenyCause::ApprovalChannelFailed,
                    reason: format!("the approval channel failed: {err}"),
                },
                feedback: None,
            },
        }
    }

    /// Turns an allowed mutation plan into permission to run it exactly once.
    pub fn mint_mutation(&mut self, plan: MutationPlan, source: AllowSource) -> ExecutionAuthority {
        let nonce = issue(&mut self.ledger);
        ExecutionAuthority::mint_mutation(plan, source, nonce)
    }

    /// Turns an allowed command plan into permission to run it exactly once.
    pub fn mint_command(&mut self, plan: CommandPlan, source: AllowSource) -> ExecutionAuthority {
        let nonce = issue(&mut self.ledger);
        ExecutionAuthority::mint_command(plan, source, nonce)
    }

    /// Spends an authority. Any outcome after this point has already burned it.
    pub fn consume(&mut self, authority: &ExecutionAuthority) -> Result<(), AuthorityError> {
        self.ledger.consume(authority.nonce())
    }

    /// How many authorities this session issued and spent, for `status`.
    pub fn ledger_counts(&self) -> (usize, usize) {
        (self.ledger.issued_count(), self.ledger.consumed_count())
    }
}

impl fmt::Debug for PermissionSession {
    /// The prompter is a trait object with no useful representation, so its
    /// presence is printed rather than its identity.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PermissionSession")
            .field("mode", &self.mode())
            .field("rules", &self.rules.current())
            .field("grants", &self.grants())
            .field("has_prompter", &self.prompter.is_some())
            .field("ledger", &self.ledger)
            .field("durable_session", &self.durable_session)
            .finish()
    }
}

impl Default for PermissionSession {
    /// `ask` with no channel: the most restrictive session that can exist.
    ///
    /// This is what a [`crate::tools::ToolContext`] carries until a caller
    /// supplies a real one, so a context built by a test or by a future caller
    /// that forgot cannot silently mutate anything.
    fn default() -> Self {
        Self::new(PermissionMode::Ask)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permission::authority::{MutationKind, Preimage};
    use std::path::PathBuf;

    fn write_plan(scope: TargetScope) -> MutationPlan {
        MutationPlan::new(
            MutationKind::Write,
            PathBuf::from("/w/a.txt"),
            "a.txt".to_string(),
            scope,
            Preimage::Absent,
            b"hello".to_vec(),
        )
        .with_rule_targets(vec![b"a.txt".to_vec(), b".".to_vec()])
    }

    #[test]
    fn auto_admits_a_workspace_write_and_refuses_an_added_root() {
        let session = PermissionSession::new(PermissionMode::Auto);
        let inside = write_plan(TargetScope::PrimaryWorkspace);
        assert_eq!(
            session.evaluate(ProposedAction::Mutation(&inside)),
            PolicyDecision::Allow {
                source: AllowSource::AutoMode
            }
        );

        let outside = write_plan(TargetScope::AdditionalRoot);
        let PolicyDecision::Deny { cause, reason } =
            session.evaluate(ProposedAction::Mutation(&outside))
        else {
            panic!("auto must not write into an added root");
        };
        assert_eq!(cause, DenyCause::NotAutoAdmissible);
        assert!(reason.contains("--add-dir"), "{reason}");
    }

    #[test]
    fn a_mutation_is_keyed_by_its_absolute_target_and_not_by_the_name_it_is_shown_as() {
        // The retargeting defect in one assertion: two workspaces, the same
        // relative name, one key each. An `always` given in `/w` is persisted
        // and reused by a later `--resume-id` run that rebound the session to
        // another tree, so a key that was only `a.txt` would authorize a file
        // nobody was ever asked about.
        let here = write_plan(TargetScope::PrimaryWorkspace);
        let elsewhere = MutationPlan::new(
            MutationKind::Write,
            PathBuf::from("/other/a.txt"),
            "a.txt".to_string(),
            TargetScope::PrimaryWorkspace,
            Preimage::Absent,
            b"hello".to_vec(),
        );
        assert_eq!(ProposedAction::Mutation(&here).target(), "/w/a.txt");
        assert_ne!(
            ProposedAction::Mutation(&here).target(),
            ProposedAction::Mutation(&elsewhere).target()
        );

        let mut session = PermissionSession::new(PermissionMode::Ask);
        session.grant(Grant::new("write_file", "/w/a.txt"));
        assert_eq!(
            session.evaluate(ProposedAction::Mutation(&here)),
            PolicyDecision::Allow {
                source: AllowSource::SessionGrant
            }
        );
        assert_eq!(
            session.evaluate(ProposedAction::Mutation(&elsewhere)),
            PolicyDecision::Prompt,
            "a grant for one tree authorized the same relative name in another"
        );

        // The question a human answers still names the file the way they do.
        assert!(
            ProposedAction::Mutation(&here)
                .always_scope()
                .contains("`a.txt`"),
            "the prose must keep the friendly path"
        );
    }

    #[test]
    fn a_mode_change_and_a_cleared_grant_decide_the_next_action() {
        let plan = write_plan(TargetScope::PrimaryWorkspace);
        let mut session = PermissionSession::new(PermissionMode::Ask);
        session.grant(Grant::new("write_file", "/w/a.txt"));
        session.grant(Grant::new("terminal", "ls in /w"));
        assert_eq!(
            session.evaluate(ProposedAction::Mutation(&plan)),
            PolicyDecision::Allow {
                source: AllowSource::SessionGrant
            }
        );

        session.set_mode(PermissionMode::Yolo);
        assert_eq!(session.mode(), PermissionMode::Yolo);
        assert_eq!(
            session.evaluate(ProposedAction::Mutation(&plan)),
            PolicyDecision::Allow {
                source: AllowSource::Yolo
            }
        );
        assert_eq!(session.grants().len(), 2, "a mode change keeps the grants");

        session.set_mode(PermissionMode::Ask);
        assert_eq!(session.clear_grants(), 2);
        assert!(session.grants().is_empty());
        assert_eq!(
            session.evaluate(ProposedAction::Mutation(&plan)),
            PolicyDecision::Prompt,
            "a cleared grant is asked about again"
        );
        assert_eq!(session.clear_grants(), 0);
    }

    #[test]
    fn a_live_mode_decides_the_next_action_of_a_session_already_in_use() {
        // `agentSnapshotRootPermissionMode` (`app_callbacks.zig:976-979`): the
        // mode is read at the decision, so a change made elsewhere -- the UI
        // thread, mid-turn -- governs the very next call.
        let plan = write_plan(TargetScope::PrimaryWorkspace);
        let live = LiveAuthority::new(PermissionMode::Auto);
        let session = PermissionSession::new(PermissionMode::Auto).with_live(live.clone());
        assert_eq!(
            session.evaluate(ProposedAction::Mutation(&plan)),
            PolicyDecision::Allow {
                source: AllowSource::AutoMode
            }
        );
        live.set_mode(PermissionMode::Ask);
        assert_eq!(session.mode(), PermissionMode::Ask);
        assert_eq!(
            session.evaluate(ProposedAction::Mutation(&plan)),
            PolicyDecision::Prompt,
            "a session in use kept the mode it was built in"
        );
        live.set_mode(PermissionMode::Yolo);
        assert_eq!(
            session.evaluate(ProposedAction::Mutation(&plan)),
            PolicyDecision::Allow {
                source: AllowSource::Yolo
            }
        );
        // The session's own copy is what it is recorded as, never what it
        // decides with while a live authority is attached.
        let mut session = session;
        session.set_mode(PermissionMode::Auto);
        assert_eq!(session.mode(), PermissionMode::Yolo);
        assert_eq!(live.resets(), 0);
    }

    #[test]
    fn a_live_reset_takes_every_earlier_grant_out_of_force_at_once() {
        let plan = write_plan(TargetScope::PrimaryWorkspace);
        let live = LiveAuthority::new(PermissionMode::Ask);
        let mut session = PermissionSession::new(PermissionMode::Ask).with_live(live.clone());
        // Restored before anything is reset, as a resume restores its log's.
        session.grant(Grant::new("write_file", "/w/a.txt"));
        assert_eq!(session.grants().len(), 1);
        live.set_mode(PermissionMode::Yolo);

        // One store: `ask`, and the grant gone -- with no `&mut` call between.
        assert_eq!(live.reset(), 1);
        assert_eq!(session.mode(), PermissionMode::Ask);
        assert!(
            session.grants().is_empty(),
            "a reset grant was still listed"
        );
        assert_eq!(
            session.evaluate(ProposedAction::Mutation(&plan)),
            PolicyDecision::Prompt,
            "a reset grant still allowed the action"
        );

        // An "always" given after the reset is the person's and stays: a
        // later application of the same reset drops only what predates it.
        session.grant(Grant::new("terminal", "ls in /w"));
        assert_eq!(session.grants(), [Grant::new("terminal", "ls in /w")]);
        assert_eq!(session.clear_grants(), 0);
        assert_eq!(session.grants().len(), 1);

        // A second reset takes that one too.
        assert_eq!(live.reset(), 2);
        assert!(session.grants().is_empty());
        assert_eq!(session.clear_grants(), 1);
    }

    #[test]
    fn a_session_attached_after_a_reset_keeps_what_it_is_given() {
        // `bind_conversation` restores a resumed log's grants after the
        // authority is attached; they belong to the reset count in force then.
        let live = LiveAuthority::new(PermissionMode::Auto);
        live.reset();
        live.reset();
        let mut session = PermissionSession::new(PermissionMode::Ask).with_live(live.clone());
        session.grant(Grant::new("edit_file", "/w/b.txt"));
        assert_eq!(session.grants().len(), 1);
        assert_eq!(live.mode(), PermissionMode::Ask);
        live.set_mode(PermissionMode::Auto);
        assert_eq!(live.resets(), 2, "a mode change is not a reset");
        assert_eq!(session.grants().len(), 1);
    }

    fn table(rules: &[(&str, &str, RuleAction)]) -> RuleSet {
        RuleSet::new(
            rules
                .iter()
                .map(|(permission, pattern, action)| {
                    crate::permission::rules::ConfiguredRule::new(*permission, *pattern, *action)
                })
                .collect(),
        )
    }

    /// A write to `display` under `/w`, judged at it and at its parent.
    fn plan_at(display: &str) -> MutationPlan {
        let parent = match std::path::Path::new(display).parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent.to_string_lossy().into_owned(),
            _ => ".".to_string(),
        };
        MutationPlan::new(
            MutationKind::Write,
            PathBuf::from("/w").join(display),
            display.to_string(),
            TargetScope::PrimaryWorkspace,
            Preimage::Absent,
            b"hello".to_vec(),
        )
        .with_rule_targets(vec![display.as_bytes().to_vec(), parent.into_bytes()])
    }

    #[test]
    fn the_last_matching_rule_decides_between_an_allow_and_a_deny() {
        use RuleAction::{Allow, Deny};
        let plan = write_plan(TargetScope::PrimaryWorkspace);
        let deny_last = PermissionSession::new(PermissionMode::Auto)
            .with_rules(table(&[("edit", "*", Allow), ("edit", "a.txt", Deny)]));
        assert!(matches!(
            deny_last.evaluate(ProposedAction::Mutation(&plan)),
            PolicyDecision::Deny {
                cause: DenyCause::ConfiguredRule,
                ref reason,
            } if reason == "a configured permission rule denies `write_file` for `a.txt`"
        ));
        let allow_last = PermissionSession::new(PermissionMode::Ask)
            .with_rules(table(&[("edit", "a.txt", Deny), ("edit", "*", Allow)]));
        assert_eq!(
            allow_last.evaluate(ProposedAction::Mutation(&plan)),
            PolicyDecision::Allow {
                source: AllowSource::ConfiguredRule
            }
        );
    }

    #[test]
    fn a_rule_for_a_different_tool_does_not_apply() {
        let plan = write_plan(TargetScope::PrimaryWorkspace);
        let session = PermissionSession::new(PermissionMode::Ask).with_rules(table(&[(
            "edit_file",
            "*",
            RuleAction::Allow,
        )]));
        assert_eq!(
            session.evaluate(ProposedAction::Mutation(&plan)),
            PolicyDecision::Prompt
        );
    }

    #[test]
    fn a_deny_on_the_directory_a_file_is_in_stops_the_write() {
        // Every target of the change is judged (`decideFileTargetPolicy`):
        // a rule about `secret` is about what goes into it.
        let plan = plan_at("secret/a.txt");
        let session = PermissionSession::new(PermissionMode::Yolo).with_rules(table(&[(
            "edit",
            "secret",
            RuleAction::Deny,
        )]));
        assert_eq!(
            session.evaluate(ProposedAction::Mutation(&plan)),
            PolicyDecision::Allow {
                source: AllowSource::Yolo
            },
            "yolo consults no rule"
        );
        let asking = PermissionSession::new(PermissionMode::Ask).with_rules(table(&[(
            "edit",
            "secret",
            RuleAction::Deny,
        )]));
        assert!(matches!(
            asking.evaluate(ProposedAction::Mutation(&plan)),
            PolicyDecision::Deny { ref reason, .. } if reason.ends_with("for `secret`")
        ));
    }

    #[test]
    fn an_allow_that_misses_one_target_leaves_the_decision_to_the_mode() {
        let plan = plan_at("a.txt");
        let rules = table(&[("edit", "a.txt", RuleAction::Allow)]);
        // The parent, `.`, is not covered: `ask` asks, `auto` admits by
        // structure, and neither is credited to the rule.
        assert_eq!(
            PermissionSession::new(PermissionMode::Ask)
                .with_rules(rules.clone())
                .evaluate(ProposedAction::Mutation(&plan)),
            PolicyDecision::Prompt
        );
        assert_eq!(
            PermissionSession::new(PermissionMode::Auto)
                .with_rules(rules)
                .evaluate(ProposedAction::Mutation(&plan)),
            PolicyDecision::Allow {
                source: AllowSource::AutoMode
            }
        );
        let tree = PermissionSession::new(PermissionMode::Ask).with_rules(table(&[(
            "edit",
            "*",
            RuleAction::Allow,
        )]));
        assert_eq!(
            tree.evaluate(ProposedAction::Mutation(&plan)),
            PolicyDecision::Allow {
                source: AllowSource::ConfiguredRule
            }
        );
    }

    #[test]
    fn a_rule_that_asks_is_asked_in_auto_mode_and_an_exact_grant_does_not_silence_it_everywhere() {
        let plan = write_plan(TargetScope::PrimaryWorkspace);
        let mut session = PermissionSession::new(PermissionMode::Auto).with_rules(table(&[(
            "edit",
            "*",
            RuleAction::Ask,
        )]));
        assert_eq!(
            session.evaluate(ProposedAction::Mutation(&plan)),
            PolicyDecision::Prompt
        );
        // The rule also asks about `.`, which an "always" for the file is not
        // an answer about: the question is put again.
        session.grant(Grant::new("write_file", "/w/a.txt"));
        assert_eq!(
            session.evaluate(ProposedAction::Mutation(&plan)),
            PolicyDecision::Prompt
        );
    }

    #[test]
    fn an_exact_grant_answers_an_ask_about_its_own_target_and_not_about_the_directory() {
        // A grant for `/w/a.txt` is not an answer about `.`: upstream checks
        // each target's grant on its own, and an exact file grant never
        // matches the parent (`decideFileTargetPolicy`).
        let plan = write_plan(TargetScope::PrimaryWorkspace);
        let mut parent_asks = PermissionSession::new(PermissionMode::Auto).with_rules(table(&[(
            "edit",
            ".",
            RuleAction::Ask,
        )]));
        parent_asks.grant(Grant::new("write_file", "/w/a.txt"));
        assert_eq!(
            parent_asks.evaluate(ProposedAction::Mutation(&plan)),
            PolicyDecision::Prompt
        );
        let mut file_asks = PermissionSession::new(PermissionMode::Auto).with_rules(table(&[
            ("edit", "a.txt", RuleAction::Ask),
            ("edit", ".", RuleAction::Allow),
        ]));
        file_asks.grant(Grant::new("write_file", "/w/a.txt"));
        assert_eq!(
            file_asks.evaluate(ProposedAction::Mutation(&plan)),
            PolicyDecision::Allow {
                source: AllowSource::SessionGrant
            }
        );
    }

    #[test]
    fn a_change_with_no_resolved_targets_is_refused_rather_than_judged_by_its_name() {
        let unresolved = MutationPlan::new(
            MutationKind::Write,
            PathBuf::from("/w/a.txt"),
            "a.txt".to_string(),
            TargetScope::PrimaryWorkspace,
            Preimage::Absent,
            b"hello".to_vec(),
        );
        let ruled = PermissionSession::new(PermissionMode::Auto).with_rules(table(&[(
            "edit",
            "*",
            RuleAction::Allow,
        )]));
        assert!(matches!(
            ruled.evaluate(ProposedAction::Mutation(&unresolved)),
            PolicyDecision::Deny {
                cause: DenyCause::ConfiguredRule,
                ref reason,
            } if reason.ends_with("its targets were not resolved")
        ));
        // With no table there is nothing to judge, and the mode decides.
        assert_eq!(
            PermissionSession::new(PermissionMode::Auto)
                .evaluate(ProposedAction::Mutation(&unresolved)),
            PolicyDecision::Allow {
                source: AllowSource::AutoMode
            }
        );
    }

    #[test]
    fn a_deny_rule_outranks_a_session_grant() {
        let plan = write_plan(TargetScope::PrimaryWorkspace);
        let mut session = PermissionSession::new(PermissionMode::Ask).with_rules(table(&[(
            "*",
            "*",
            RuleAction::Deny,
        )]));
        session.grant(Grant::new("write_file", "/w/a.txt"));
        assert!(matches!(
            session.evaluate(ProposedAction::Mutation(&plan)),
            PolicyDecision::Deny {
                cause: DenyCause::ConfiguredRule,
                ..
            }
        ));
    }

    #[test]
    fn a_rule_that_asks_with_nobody_to_ask_says_it_was_the_rule() {
        let plan = write_plan(TargetScope::PrimaryWorkspace);
        let mut session = PermissionSession::new(PermissionMode::Auto).with_rules(table(&[(
            "edit",
            "*",
            RuleAction::Ask,
        )]));
        let Decided { decision, .. } =
            session.decide_with_feedback(ProposedAction::Mutation(&plan));
        let PolicyDecision::Deny { cause, reason } = decision else {
            panic!("an unanswerable question is a no");
        };
        assert_eq!(cause, DenyCause::NoApprovalChannel);
        assert!(
            reason.starts_with("a configured permission rule asks before `write_file`"),
            "{reason}"
        );
        let mut plain = PermissionSession::new(PermissionMode::Ask);
        let Decided { decision, .. } = plain.decide_with_feedback(ProposedAction::Mutation(&plan));
        assert!(matches!(
            decision,
            PolicyDecision::Deny { ref reason, .. } if reason.starts_with("`ask` mode needs")
        ));
    }

    #[test]
    fn a_live_table_decides_the_next_action_of_a_session_already_in_use() {
        let plan = write_plan(TargetScope::PrimaryWorkspace);
        let live = LiveRules::default();
        let session = PermissionSession::new(PermissionMode::Ask).with_live_rules(live.clone());
        assert_eq!(
            session.evaluate(ProposedAction::Mutation(&plan)),
            PolicyDecision::Prompt
        );
        live.replace(table(&[("edit", "*", RuleAction::Allow)]));
        assert_eq!(
            session.evaluate(ProposedAction::Mutation(&plan)),
            PolicyDecision::Allow {
                source: AllowSource::ConfiguredRule
            }
        );
        assert_eq!(session.rules().rules().len(), 1);
        live.replace(RuleSet::default());
        assert_eq!(
            session.evaluate(ProposedAction::Mutation(&plan)),
            PolicyDecision::Prompt
        );
    }

    #[test]
    fn a_read_is_free_unless_a_rule_denies_or_asks_and_yolo_reads_everything() {
        let session = PermissionSession::new(PermissionMode::Auto).with_rules(table(&[
            ("read", "secrets/*", RuleAction::Deny),
            ("grep", "*", RuleAction::Ask),
            ("read", "secrets/ok", RuleAction::Allow),
        ]));
        assert_eq!(session.judge_read("read_file", b"notes.md"), Ok(()));
        assert_eq!(
            session.judge_read("read_file", b"secrets/key"),
            Err("a configured permission rule denies `read_file` for `secrets/key`".to_string())
        );
        // The last match wins for reads too.
        assert_eq!(session.judge_read("read_file", b"secrets/ok"), Ok(()));
        let asked = session.judge_read("grep_files", b".").unwrap_err();
        assert!(
            asked.contains("asks before `grep_files` reads `.`"),
            "{asked}"
        );
        // A read rule is not a glob rule: `list_files` is judged as `glob`.
        assert_eq!(session.judge_read("list_files", b"secrets"), Ok(()));
        let yolo = PermissionSession::new(PermissionMode::Yolo).with_rules(table(&[(
            "*",
            "*",
            RuleAction::Deny,
        )]));
        assert_eq!(yolo.judge_read("read_file", b"secrets/key"), Ok(()));
    }

    #[test]
    fn granting_the_same_thing_twice_records_it_once() {
        let mut session = PermissionSession::new(PermissionMode::Ask);
        session.grant(Grant::new("terminal", "cargo test"));
        session.grant(Grant::new("terminal", "cargo test"));
        assert_eq!(session.grants().len(), 1);
    }

    #[test]
    fn a_summary_discloses_the_content_and_not_only_the_shape() {
        let plan = write_plan(TargetScope::PrimaryWorkspace);
        let summary = ProposedAction::Mutation(&plan).summary();
        // Size, digest, and the bytes themselves. "write to a.txt" is not a
        // question anyone can answer.
        assert!(summary.contains("write 5 bytes to `a.txt`"), "{summary}");
        assert!(summary.contains("does not exist yet"), "{summary}");
        assert!(summary.contains("\"hello\""), "{summary}");
        assert!(
            summary.contains(&crate::permission::ContentHash::of(b"hello").short()),
            "{summary}"
        );
    }

    #[test]
    fn the_always_answer_says_what_it_would_buy() {
        let plan = write_plan(TargetScope::PrimaryWorkspace);
        let scope = ProposedAction::Mutation(&plan).always_scope();
        // The grant is keyed by path, not by content, so the prompt says so
        // rather than letting one shown diff imply permission for every future
        // one.
        assert!(
            scope.contains("every future write_file to `a.txt`"),
            "{scope}"
        );
        assert!(scope.contains("whatever its contents"), "{scope}");
    }

    /// A prompter that answers "no" and keeps the question it was asked.
    struct RecordingPrompter {
        asked: std::sync::Arc<std::sync::Mutex<Vec<ApprovalRequest>>>,
    }

    impl ApprovalPrompter for RecordingPrompter {
        fn request(&mut self, request: &ApprovalRequest) -> io::Result<ApprovalAnswer> {
            self.asked.lock().expect("lock").push(request.clone());
            Ok(ApprovalAnswer::Deny)
        }
    }

    #[test]
    fn the_question_the_user_is_actually_asked_names_the_durable_scope() {
        // The one that matters: not that the helper can produce the sentence,
        // but that the sentence reaches the prompt. A prompt that understated
        // what "always" buys would be the whole defect.
        let plan = write_plan(TargetScope::PrimaryWorkspace);
        let asked = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut session = PermissionSession::new(PermissionMode::Ask)
            .with_durable_session("2026-abc")
            .with_prompter(Box::new(RecordingPrompter {
                asked: std::sync::Arc::clone(&asked),
            }));

        session.decide_with_feedback(ProposedAction::Mutation(&plan));

        let asked = asked.lock().expect("lock");
        assert_eq!(asked.len(), 1);
        assert!(
            asked[0]
                .always_scope
                .contains("xfx ask --resume-id 2026-abc"),
            "the prompt must disclose the durable scope: {}",
            asked[0].always_scope
        );
    }

    #[test]
    fn an_unrecorded_turn_is_not_asked_a_durable_question() {
        let plan = write_plan(TargetScope::PrimaryWorkspace);
        let asked = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut session = PermissionSession::new(PermissionMode::Ask).with_prompter(Box::new(
            RecordingPrompter {
                asked: std::sync::Arc::clone(&asked),
            },
        ));

        session.decide_with_feedback(ProposedAction::Mutation(&plan));

        let asked = asked.lock().expect("lock");
        assert!(
            asked[0].always_scope.contains("ends with this command"),
            "{}",
            asked[0].always_scope
        );
        assert!(!asked[0].always_scope.contains("--resume-id"));
    }

    #[test]
    fn a_recorded_session_says_that_always_outlives_the_command() {
        let plan = write_plan(TargetScope::PrimaryWorkspace);
        let action = ProposedAction::Mutation(&plan);

        // Recorded: the approval survives, so the prompt names the exact thing
        // that will reuse it.
        let durable = PermissionSession::new(PermissionMode::Ask).with_durable_session("2026-abc");
        let scope = durable.always_scope_for(action);
        assert!(
            scope.contains("every future write_file to `a.txt`"),
            "{scope}"
        );
        assert!(
            scope.contains("xfx ask --resume-id 2026-abc"),
            "the prompt must name the durable scope it is buying: {scope}"
        );

        // Not recorded: "always" really does end with the process, and the
        // prompt must not imply otherwise.
        let ephemeral = PermissionSession::new(PermissionMode::Ask);
        let scope = ephemeral.always_scope_for(action);
        assert!(
            scope.contains("ends with this command"),
            "an unrecorded turn must not promise durability: {scope}"
        );
        assert!(!scope.contains("--resume-id"), "{scope}");
    }

    #[test]
    fn the_question_carries_the_plans_own_diff_and_invents_one_for_nothing_else() {
        // The payload travels **with the decision**, from the plan that was
        // judged rather than from anything the UI could recompute: a diff built
        // beside the question would be a second reading of the change, and two
        // readings are two answers to "what is being approved".
        let plan =
            write_plan(TargetScope::PrimaryWorkspace).with_diff(ApprovalDiff::of("alpha", "beta"));
        let asked = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut session = PermissionSession::new(PermissionMode::Ask).with_prompter(Box::new(
            RecordingPrompter {
                asked: std::sync::Arc::clone(&asked),
            },
        ));

        session.decide_with_feedback(ProposedAction::Mutation(&plan));

        let carried = asked.lock().expect("lock")[0]
            .diff
            .clone()
            .expect("the question carries the plan's diff");
        assert_eq!(carried.before, "alpha");
        assert_eq!(carried.after, "beta");

        // A plan that never had one, and a command, which has no before and
        // after at all: what a command would do is the command, and the summary
        // already quotes that whole.
        let bare = write_plan(TargetScope::PrimaryWorkspace);
        session.decide_with_feedback(ProposedAction::Mutation(&bare));
        assert!(
            asked.lock().expect("lock")[1].diff.is_none(),
            "a question invented a diff its plan never carried"
        );

        let dir = tempfile::tempdir().expect("a temporary root");
        let scope = crate::workspace::AccessScope::primary_only(dir.path()).expect("a usable root");
        let command =
            CommandPlan::prepare("pwd", &scope, None, &crate::tools::ToolLimits::default())
                .expect("a plannable command");
        session.decide_with_feedback(ProposedAction::Command(&command));
        assert!(
            asked.lock().expect("lock")[2].diff.is_none(),
            "a command was given a before and an after it does not have"
        );
    }

    #[test]
    fn the_default_session_can_authorize_nothing_on_its_own() {
        let mut session = PermissionSession::default();
        assert_eq!(session.mode(), PermissionMode::Ask);
        assert!(!session.has_prompter());
        let plan = write_plan(TargetScope::PrimaryWorkspace);
        assert!(matches!(
            session.decide_with_feedback(ProposedAction::Mutation(&plan)),
            Decided {
                decision: PolicyDecision::Deny {
                    cause: DenyCause::NoApprovalChannel,
                    ..
                },
                feedback: None,
            }
        ));
    }
}
