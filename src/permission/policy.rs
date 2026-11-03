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
    MutationPlan,
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

/// A read a configured rule may be about, as its tool resolved it before
/// opening anything.
///
/// Reading is free unless the table says otherwise, so a read is put to policy
/// for one reason: the table may deny it, or ask about it, and a rule that asks
/// is put to the person as upstream puts it (`requestPermissionOutcomeResolved`,
/// `vercel-labs/fx@c1db9191 src/core/tooling/tool_admission.zig:1389-1442`).
/// Everything here was resolved by the tool -- the file a path names now, links
/// followed -- so the question, the grant and the rule are all about the file
/// that would be read, never about the spelling the model used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadPlan {
    tool: &'static str,
    target: std::path::PathBuf,
    display: String,
    candidate: Vec<u8>,
    argument: String,
    binding: Vec<u8>,
}

impl ReadPlan {
    /// `tool`'s read of `target` -- canonical and absolute -- which the person
    /// is shown as `display` and the table judges as `candidate`
    /// ([`super::rules::rule_path`]). `argument` is the call's own label
    /// argument as the model wrote it -- a path, or a search's pattern -- which
    /// upstream's question quotes after the tool's name
    /// (`formatPermissionLabel`, `tool_presentation.zig:683-718`); `binding`
    /// is the call's typed input, field by field, for a saved-session rule's
    /// key ([`ProposedAction::state_key`]).
    pub fn new(
        tool: &'static str,
        target: std::path::PathBuf,
        display: String,
        candidate: Vec<u8>,
        argument: String,
        binding: Vec<u8>,
    ) -> Self {
        Self {
            tool,
            target,
            display,
            candidate,
            argument,
            binding,
        }
    }

    pub fn tool(&self) -> &'static str {
        self.tool
    }

    /// Whether this read lists or searches a directory rather than reading a
    /// file: what the question and the grant say it is about.
    fn searches(&self) -> bool {
        matches!(self.tool, "glob_files" | "grep_files")
    }
}

/// A prepared action, offered to policy for judgement.
///
/// It borrows the plan rather than owning it, so the same plan is what gets
/// minted afterwards. There is no path where policy judges one thing and
/// execution runs another.
#[derive(Debug, Clone, Copy)]
pub enum ProposedAction<'a> {
    Mutation(&'a MutationPlan),
    Command(&'a CommandPlan),
    /// A read, which mints nothing: an allowed read reads, at once, the file
    /// its tool resolved.
    Read(&'a ReadPlan),
}

impl ProposedAction<'_> {
    /// The advertised tool name, which is what a rule or a grant names.
    pub fn tool(&self) -> &'static str {
        match self {
            Self::Mutation(plan) => plan.kind().tool(),
            Self::Command(_) => "terminal",
            Self::Read(plan) => plan.tool,
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
            // One target: the path the tool resolved, a search's being the
            // directory it searches below (`path_optional_existing`,
            // `permissions.zig:205-211`).
            Self::Read(plan) => (
                super::rules::permission_name_for_tool(plan.tool),
                plan.tool,
                Some(vec![plan.candidate.clone()]),
            ),
        }
    }

    /// The key a saved-session rule for exactly this action is kept under
    /// (`permissionStateKeyForCall`, `tool_admission.zig:2189-2232`, and
    /// `permissionStateKeyForPreparedFileMutation`, `:2304-2342`), or `None`
    /// for a change whose executor did not bind one.
    ///
    /// A command: its text, the directory it runs in, how and where
    /// (`commandKeyV2`). A change: the tool, the call's typed input and the
    /// identity of the directory it is made in (the executor's binding), the
    /// canonical target, the kind, the preimage -- absent, or its device,
    /// inode and digest -- the digest of what it would leave, and every path
    /// a configured rule judges it at. A read: the tool, the call's typed
    /// input, and the file or directory it resolved -- upstream's structured
    /// key of the call's name, its arguments and its targets
    /// (`permissionStateKeyForCall`, `:2219-2235`), in xfx's encoding of the
    /// same facts. Nothing in it is a pattern: a rule made from one action
    /// matches that action and no other.
    pub fn state_key(&self) -> Option<super::session_rules::RuleKey> {
        use super::session_rules::{field, os_bytes, target_os, KeyKind, RuleKey};
        match self {
            Self::Command(plan) => RuleKey::command(plan.command(), plan.cwd(), target_os()),
            Self::Read(plan) => {
                let mut canonical = Vec::new();
                field(&mut canonical, b"xfx-permission-state-read-v1");
                field(&mut canonical, plan.tool.as_bytes());
                field(&mut canonical, &plan.binding);
                field(&mut canonical, &os_bytes(plan.target.as_os_str()));
                RuleKey::new(KeyKind::StructuredTool, canonical)
            }
            Self::Mutation(plan) => {
                let binding = plan.state_binding()?;
                let targets = plan.rule_targets()?;
                let mut canonical = Vec::new();
                field(&mut canonical, b"xfx-permission-state-file-v1");
                field(&mut canonical, plan.kind().tool().as_bytes());
                field(&mut canonical, binding);
                field(&mut canonical, &os_bytes(plan.target().as_os_str()));
                match plan.preimage() {
                    super::Preimage::Absent => field(&mut canonical, b"preimage-absent"),
                    super::Preimage::Present { identity, hash } => {
                        field(&mut canonical, b"preimage-present");
                        field(&mut canonical, &identity.device.to_be_bytes());
                        field(&mut canonical, &identity.inode.to_be_bytes());
                        field(&mut canonical, hash.hex().as_bytes());
                    }
                }
                field(&mut canonical, plan.after_hash().hex().as_bytes());
                for target in targets {
                    field(&mut canonical, target);
                }
                RuleKey::new(KeyKind::FileMutation, canonical)
            }
        }
    }

    /// How a saved-session rule for this action is shown
    /// (`filePermissionStateDisplay`, `tool_admission.zig:2344-2369`):
    /// `write_file a.txt preimage=absent after=2d711642b726b044`, the digests
    /// cut to their first eight bytes as upstream's are; a command as the
    /// band's comment names it, with where it runs.
    pub fn state_display(&self) -> String {
        match self {
            Self::Command(plan) => {
                format!("terminal.exec {} in {}", plan.command(), plan.display_cwd())
            }
            // Upstream's display for a structured call is its question's
            // label (`preparePermissionStateAction`, `:2261-2267`).
            Self::Read(plan) => format!("{} {}", plan.tool, plan.argument),
            Self::Mutation(plan) => {
                let after = &plan.after_hash().hex()[..16];
                match plan.preimage() {
                    super::Preimage::Absent => format!(
                        "{} {} preimage=absent after={after}",
                        plan.kind().tool(),
                        plan.display()
                    ),
                    super::Preimage::Present { hash, .. } => format!(
                        "{} {} preimage={} after={after}",
                        plan.kind().tool(),
                        plan.display(),
                        &hash.hex()[..16]
                    ),
                }
            }
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
            // The file it resolved, for the reason a mutation's is.
            Self::Read(plan) => plan.target.to_string_lossy().into_owned(),
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
            Self::Read(plan) => match plan.tool {
                "list_files" => format!("list the entries of `{}`", plan.display),
                "glob_files" => format!(
                    "search below `{}` for files named like `{}`",
                    plan.display, plan.argument
                ),
                "grep_files" => format!(
                    "search the files below `{}` for `{}`",
                    plan.display, plan.argument
                ),
                _ => format!("read `{}`", plan.display),
            },
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
            // itself, and [`Self::summary`] quotes that whole. A read changes
            // nothing.
            Self::Command(_) | Self::Read(_) => None,
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
            // The grant is this tool at this path, whatever else the call
            // asks: a later `start_line`, another pattern below the same
            // directory.
            Self::Read(plan) if plan.searches() => format!(
                "allow every future {} below `{}` for the rest of this session, whatever it searches for",
                plan.tool, plan.display
            ),
            Self::Read(plan) => format!(
                "allow every future {} of `{}` for the rest of this session",
                plan.tool, plan.display
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
            Self::Read(plan) => ApprovalSubject::Call(plan.argument.clone()),
        }
    }

    /// Whether `auto` may admit this without asking anybody, and why not.
    ///
    /// `auto` has no human to consult, so the only actions it can admit are the
    /// ones whose *structure* already bounds them: a write of bounded size to a
    /// path inside a root the user put the session in -- the workspace or an
    /// added one, alike ([`super::TargetScope`]) -- that is not a file deciding what
    /// runs later ([`MutationPlan::sensitive_auto_write_target`]), or a command
    /// the grammar reduced to a read-only argv.
    fn auto_admission(&self) -> Result<(), String> {
        match self {
            Self::Mutation(plan) => match plan.sensitive_auto_write_target() {
                Some(sequence) => Err(format!(
                    "auto mode does not change `{}` on its own because `{sequence}` decides what runs later; ask the user to approve it",
                    plan.display()
                )),
                None => Ok(()),
            },
            Self::Command(plan) => match plan.effect() {
                CommandEffect::DirectReadOnly { .. } => Ok(()),
                CommandEffect::Denied(effect) => Err(format!(
                    "`{}` is not admitted in auto mode because {}; ask the user to approve it, or use a command that only reads",
                    plan.command(),
                    effect.describe()
                )),
            },
            // Never consulted: a read no rule decides is admitted before the
            // mode is ([`PermissionSession::evaluate_with`]).
            Self::Read(_) => Ok(()),
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
    /// A saved-session rule (`/permissions remember allow`) for exactly this
    /// action.
    SavedSessionRule,
    /// The user said yes, for this call.
    InteractiveOnce,
    /// The user said yes, for the rest of the session.
    InteractiveAlways,
    /// The structure of the action satisfied `auto`.
    AutoMode,
    /// A read no rule decided: reading is free, in every mode.
    FreeRead,
    /// No check ran.
    Yolo,
}

/// Why an action was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenyCause {
    /// A configured deny rule matched.
    ConfiguredRule,
    /// A saved-session rule (`/permissions remember deny`) for exactly this
    /// action, or a store of them that could not be read.
    SavedSessionRule,
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
    /// A call as upstream's question names one that is neither a change nor
    /// a command: its one label argument -- a read's path, a search's
    /// pattern -- as the model wrote it, shown after the request's tool
    /// (`formatPermissionLabel`, `tool_presentation.zig:683-718`). What the
    /// call resolved to is the summary's and the always-scope's.
    Call(String),
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
        // fetch_update is renamed try_update in Rust 1.99; kept for the 1.96 MSRV.
        #[allow(deprecated)]
        let _ = self
            .0
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |word| {
                Some(Self::pack(mode, Self::unpack(word).1))
            });
    }

    /// `/permissions reset`: `ask`, and every grant given so far out of force,
    /// in one store. Answers the new reset count.
    pub fn reset(&self) -> u64 {
        // fetch_update is renamed try_update in Rust 1.99; kept for the 1.96 MSRV.
        #[allow(deprecated)]
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
    /// The saved session's `permissions.json`, read at every decision, when
    /// this session is recorded ([`super::session_rules`]).
    session_rules: Option<std::path::PathBuf>,
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
            session_rules: None,
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

    /// The same session, deciding with the saved-session rules at `path`
    /// (`<session>/permissions.json`) as they stand at each decision -- read
    /// afresh every time, as upstream's snapshot provider is
    /// (`requestPermissionOutcome`, `tool_admission.zig:1279-1309`), so a rule
    /// the person confirmed on another thread, or in another process, is in
    /// force for the next call.
    pub fn with_session_rules(mut self, path: impl Into<std::path::PathBuf>) -> Self {
        self.session_rules = Some(path.into());
        self
    }

    /// Where this session's saved-session rules are kept, when it has any.
    pub fn session_rules_path(&self) -> Option<&std::path::Path> {
        self.session_rules.as_deref()
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
        self.evaluate_with(action, None)
    }

    /// [`Self::evaluate`], with the saved-session rules `saved` holds.
    ///
    /// Still pure: the store is read by the caller. A saved rule is consulted
    /// **after** a configured deny and **before** everything else, as upstream
    /// consults its session state (`requestPermissionOutcomeResolved`,
    /// `tool_admission.zig:1386-1428`): a remembered allow cannot beat a deny
    /// the person configured, and a remembered deny beats any allow.
    pub fn evaluate_with(
        &self,
        action: ProposedAction<'_>,
        saved: Option<&super::session_rules::State>,
    ) -> PolicyDecision {
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

        let verdict = self.rule_verdict(action, granted);
        if let RuleVerdict::Denied(candidate) = verdict {
            return PolicyDecision::Deny {
                cause: DenyCause::ConfiguredRule,
                reason: format!("a configured permission rule denies `{tool}` for `{candidate}`"),
            };
        }
        if let Some(saved) = saved.filter(|saved| !saved.rules.is_empty()) {
            use super::session_rules::Verdict;
            // Fail closed: a store with rules in it, and an action that
            // cannot say which of them is its, is not an action to let by.
            let Some(key) = action.state_key() else {
                return PolicyDecision::Deny {
                    cause: DenyCause::SavedSessionRule,
                    reason: format!(
                        "`{tool}` for `{target}` cannot be matched against this session's saved permission rules"
                    ),
                };
            };
            match saved.decide(&key) {
                Verdict::Deny => {
                    return PolicyDecision::Deny {
                        cause: DenyCause::SavedSessionRule,
                        reason: format!(
                            "a saved-session permission rule denies `{}`",
                            action.state_display()
                        ),
                    }
                }
                Verdict::Allow => {
                    return PolicyDecision::Allow {
                        source: AllowSource::SavedSessionRule,
                    }
                }
                Verdict::Unresolved => {}
            }
        }
        match verdict {
            RuleVerdict::Denied(_) => unreachable!("returned above"),
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
        // Reading is free unless a rule says otherwise, in every mode: a read
        // has no approval requirement of its own (`resolveOrdinaryPermissionOutcome`,
        // `tool_admission.zig:1123-1125`).
        if let ProposedAction::Read(_) = action {
            return PolicyDecision::Allow {
                source: AllowSource::FreeRead,
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
        // The saved-session rules as they stand now. A store that cannot be
        // read refuses the call, as upstream's failed snapshot fails it: a
        // deny the person saved must not lapse because its file broke.
        let saved = match &self.session_rules {
            Some(path) if self.mode() != PermissionMode::Yolo => {
                match super::session_rules::load(path) {
                    Ok(state) => Some(state),
                    Err(reason) => {
                        return Decided {
                            decision: PolicyDecision::Deny {
                                cause: DenyCause::SavedSessionRule,
                                reason,
                            },
                            feedback: None,
                        }
                    }
                }
            }
            _ => None,
        };
        match self.evaluate_with(action, saved.as_ref()) {
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
    use crate::permission::authority::{MutationKind, Preimage, TargetScope};
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
    fn auto_admits_a_write_in_an_added_root_as_it_does_in_the_workspace() {
        // Upstream's rule: an active added root is a root the target is
        // *contained* in, and a contained target bypasses review in auto
        // (`vercel-labs/fx@c1db9191 src/core/tooling/tool_admission.zig:665-679`;
        // the e2e `write_trusted_added`, `expectedReview: false`,
        // `tests/e2e/file-tool-paths.test.ts:1170-1179`).
        let session = PermissionSession::new(PermissionMode::Auto);
        for scope in [TargetScope::PrimaryWorkspace, TargetScope::AdditionalRoot] {
            assert_eq!(
                session.evaluate(ProposedAction::Mutation(&write_plan(scope))),
                PolicyDecision::Allow {
                    source: AllowSource::AutoMode
                },
                "{scope:?}"
            );
        }
    }

    #[test]
    fn auto_does_not_change_a_login_or_git_configuration_file_in_any_root() {
        // Upstream sends these to its reviewer even inside a root
        // (`sensitiveAutoWriteTarget`, `tool_admission.zig:680-694`). xfx's
        // auto has no reviewer, so "review" is a refusal: the user can still
        // approve it in `ask`. Without this, `/workspace add ~` would let auto
        // rewrite `~/.zshrc`, which upstream never does unreviewed.
        let session = PermissionSession::new(PermissionMode::Auto);
        for target in [
            "/home/u/.zshrc",
            "/home/u/.bashrc",
            "/home/u/.bash_profile",
            "/home/u/.profile",
            "/home/u/.ssh/authorized_keys",
            "/home/u/.ssh/config",
            "/home/u/.config/autostart/x.desktop",
            "/home/u/.config/fish/config.fish",
            "/Users/u/Library/LaunchAgents/x.plist",
            "/Library/LaunchDaemons/x.plist",
            "/w/repo/.git/hooks/pre-commit",
            "/w/repo/.git/config",
            "/w/repo/.git/config.worktree",
            // APFS is case-insensitive by default, so is this.
            "/home/u/.ZSHRC",
        ] {
            for scope in [TargetScope::PrimaryWorkspace, TargetScope::AdditionalRoot] {
                let plan = MutationPlan::new(
                    MutationKind::Write,
                    PathBuf::from(target),
                    target.to_string(),
                    scope,
                    Preimage::Absent,
                    b"x".to_vec(),
                );
                let PolicyDecision::Deny { cause, reason } =
                    session.evaluate(ProposedAction::Mutation(&plan))
                else {
                    panic!("auto changed {target} in {scope:?}");
                };
                assert_eq!(cause, DenyCause::NotAutoAdmissible, "{target}");
                assert!(reason.contains("ask the user"), "{reason}");
            }
        }
        // The sequence has to be whole and in order: these are ordinary files.
        for target in [
            "/w/zshrc",
            "/w/.zshrc.bak",
            "/w/config/.ssh",
            "/w/hooks/.git",
            "/w/.git-config",
        ] {
            let plan = MutationPlan::new(
                MutationKind::Write,
                PathBuf::from(target),
                target.to_string(),
                TargetScope::AdditionalRoot,
                Preimage::Absent,
                b"x".to_vec(),
            );
            assert_eq!(
                session.evaluate(ProposedAction::Mutation(&plan)),
                PolicyDecision::Allow {
                    source: AllowSource::AutoMode
                },
                "{target}"
            );
        }
        // And `ask` is unchanged: a human can still say yes to one.
        let ask = PermissionSession::new(PermissionMode::Ask);
        let plan = MutationPlan::new(
            MutationKind::Write,
            PathBuf::from("/home/u/.zshrc"),
            ".zshrc".to_string(),
            TargetScope::AdditionalRoot,
            Preimage::Absent,
            b"x".to_vec(),
        );
        assert_eq!(
            ask.evaluate(ProposedAction::Mutation(&plan)),
            PolicyDecision::Prompt
        );
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

    /// `tool`'s read of `/w/<path>`, judged at `path`.
    fn read(tool: &'static str, path: &str) -> ReadPlan {
        ReadPlan::new(
            tool,
            PathBuf::from(format!("/w/{path}")),
            path.to_string(),
            path.as_bytes().to_vec(),
            path.to_string(),
            Vec::new(),
        )
    }

    #[test]
    fn a_read_is_free_unless_a_rule_denies_or_asks_in_every_mode_but_full_access() {
        let rows = [
            ("read", "secrets/*", RuleAction::Deny),
            ("grep", "*", RuleAction::Ask),
            ("read", "secrets/ok", RuleAction::Allow),
        ];
        for mode in [PermissionMode::Ask, PermissionMode::Auto] {
            let session = PermissionSession::new(mode).with_rules(table(&rows));
            let decide = |plan: &ReadPlan| session.evaluate(ProposedAction::Read(plan));
            assert_eq!(
                decide(&read("read_file", "notes.md")),
                PolicyDecision::Allow {
                    source: AllowSource::FreeRead
                },
                "{mode:?}"
            );
            assert_eq!(
                decide(&read("read_file", "secrets/key")),
                PolicyDecision::Deny {
                    cause: DenyCause::ConfiguredRule,
                    reason: "a configured permission rule denies `read_file` for `secrets/key`"
                        .to_string()
                }
            );
            // The last match wins for reads too.
            assert_eq!(
                decide(&read("read_file", "secrets/ok")),
                PolicyDecision::Allow {
                    source: AllowSource::ConfiguredRule
                }
            );
            // A rule that asks is put to the person in `auto` too
            // (`configured_ask`, `tool_admission.zig:1426-1442`).
            assert_eq!(
                decide(&read("grep_files", ".")),
                PolicyDecision::Prompt,
                "{mode:?}"
            );
            // A read rule is not a glob rule: `list_files` is judged as `glob`.
            assert_eq!(
                decide(&read("list_files", "secrets")),
                PolicyDecision::Allow {
                    source: AllowSource::FreeRead
                }
            );
        }
        let yolo = PermissionSession::new(PermissionMode::Yolo).with_rules(table(&[(
            "*",
            "*",
            RuleAction::Deny,
        )]));
        assert_eq!(
            yolo.evaluate(ProposedAction::Read(&read("read_file", "secrets/key"))),
            PolicyDecision::Allow {
                source: AllowSource::Yolo
            }
        );
    }

    #[test]
    fn an_exact_grant_answers_a_read_rule_that_asks_and_never_a_deny() {
        let mut session = PermissionSession::new(PermissionMode::Ask).with_rules(table(&[
            ("read", "*", RuleAction::Ask),
            ("read", "keys/*", RuleAction::Deny),
        ]));
        session.grant(Grant::new("read_file", "/w/a.txt"));
        session.grant(Grant::new("read_file", "/w/keys/k"));
        assert_eq!(
            session.evaluate(ProposedAction::Read(&read("read_file", "a.txt"))),
            PolicyDecision::Allow {
                source: AllowSource::SessionGrant
            }
        );
        // Another file, and the same file by another tool, are asked again.
        assert_eq!(
            session.evaluate(ProposedAction::Read(&read("read_file", "b.txt"))),
            PolicyDecision::Prompt
        );
        assert_eq!(
            session.evaluate(ProposedAction::Read(&read("grep_files", "a.txt"))),
            PolicyDecision::Allow {
                source: AllowSource::FreeRead
            },
            "no grep rule asks"
        );
        assert!(matches!(
            session.evaluate(ProposedAction::Read(&read("read_file", "keys/k"))),
            PolicyDecision::Deny {
                cause: DenyCause::ConfiguredRule,
                ..
            }
        ));
    }

    #[test]
    fn a_read_question_names_the_resolved_file_and_quotes_the_call() {
        let mut plan = read("read_file", "docs/a.md");
        plan.argument = "./docs/../docs/a.md".to_string();
        let action = ProposedAction::Read(&plan);
        assert_eq!(action.tool(), "read_file");
        assert_eq!(action.target(), "/w/docs/a.md");
        assert_eq!(action.summary(), "read `docs/a.md`");
        assert_eq!(
            action.always_scope(),
            "allow every future read_file of `docs/a.md` for the rest of this session"
        );
        assert_eq!(
            action.subject(),
            ApprovalSubject::Call("./docs/../docs/a.md".to_string())
        );
        assert_eq!(action.state_display(), "read_file ./docs/../docs/a.md");
        assert!(action.diff().is_none());
        let search = ReadPlan::new(
            "grep_files",
            PathBuf::from("/w/src"),
            "src".to_string(),
            b"src".to_vec(),
            "needle".to_string(),
            Vec::new(),
        );
        let action = ProposedAction::Read(&search);
        assert_eq!(
            action.summary(),
            "search the files below `src` for `needle`"
        );
        assert!(
            action
                .always_scope()
                .starts_with("allow every future grep_files below `src`"),
            "{}",
            action.always_scope()
        );
        // The key is the tool, the typed input and the resolved target.
        let key = |tool: &'static str, binding: &[u8], target: &str| {
            ProposedAction::Read(&ReadPlan::new(
                tool,
                PathBuf::from(target),
                String::new(),
                Vec::new(),
                String::new(),
                binding.to_vec(),
            ))
            .state_key()
            .expect("a key")
        };
        assert_eq!(
            key("read_file", b"a", "/w/a"),
            key("read_file", b"a", "/w/a")
        );
        assert_ne!(
            key("read_file", b"a", "/w/a"),
            key("read_file", b"b", "/w/a")
        );
        assert_ne!(
            key("read_file", b"a", "/w/a"),
            key("read_file", b"a", "/w/b")
        );
        assert_ne!(
            key("read_file", b"a", "/w/a"),
            key("grep_files", b"a", "/w/a")
        );
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
