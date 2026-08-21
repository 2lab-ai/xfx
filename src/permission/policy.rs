//! Who decides, on what evidence, and what a decision is worth.
//!
//! The decision is split in two on purpose:
//!
//! - [`PermissionSession::evaluate`] is a pure function of the mode, the
//!   configured rules, the session grants, and the *structure of the prepared
//!   action*. It reads no file, runs no process, and asks no human. It can
//!   therefore be tested exhaustively, and it cannot change the thing it is
//!   judging.
//! - [`PermissionSession::decide`] adds the only side effect a decision is
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

use crate::config::PermissionMode;

use super::authority::{
    issue, AuthorityError, AuthorityLedger, CommandPlan, ExecutionAuthority, MutationPlan,
    TargetScope,
};
use super::command::CommandEffect;

/// The warning `yolo` prints before anything runs.
///
/// It names all three things that are off, because "yolo mode" alone reads as a
/// speed setting rather than as the removal of every check.
pub const YOLO_WARNING: &str =
    "fxr: yolo mode is on -- tool calls run with no permission check, no approval prompt, and no sandbox.";

// ---------------------------------------------------------------------------
// rules and grants
// ---------------------------------------------------------------------------

/// One configured decision about one exact tool and target.
///
/// Exact, not glob. A pattern language is a second thing to get wrong, and the
/// failure mode of a too-wide pattern is silent over-authorization. Upstream
/// carries glob-shaped grants; fxr's first release does not, and `docs/parity.md`
/// records the difference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub tool: String,
    pub target: String,
}

impl Rule {
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

/// The configured allow and deny lists.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PermissionRules {
    allow: Vec<Rule>,
    deny: Vec<Rule>,
}

impl PermissionRules {
    pub fn new(allow: Vec<Rule>, deny: Vec<Rule>) -> Self {
        Self { allow, deny }
    }
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

    /// The exact thing being asked for: a display path, or the command text.
    pub fn target(&self) -> String {
        match self {
            Self::Mutation(plan) => plan.display().to_string(),
            Self::Command(plan) => plan.command().to_string(),
        }
    }

    /// One sentence a human can answer yes or no to.
    pub fn summary(&self) -> String {
        match self {
            Self::Mutation(plan) => match plan.kind() {
                super::MutationKind::Write => {
                    format!(
                        "write {} bytes to {}",
                        plan.staged_bytes().len(),
                        plan.display()
                    )
                }
                super::MutationKind::Edit => format!("edit {}", plan.display()),
                super::MutationKind::CreateFolder => {
                    format!("create the directory {}", plan.display())
                }
            },
            Self::Command(plan) => format!("run `{}` in {}", plan.command(), plan.display_cwd()),
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
    /// [`PermissionSession::decide`] always resolves it.
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
    /// One sentence describing what would happen.
    pub summary: String,
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

/// Something that can put a question to a human and get an answer back.
///
/// A missing prompter is not "allow by default": [`PermissionSession::decide`]
/// treats it as [`DenyCause::NoApprovalChannel`]. That is the whole reason this
/// is an `Option` rather than a trait object with a permissive default.
///
/// `Send` because a session lives behind a `Mutex` that a
/// [`crate::tools::ToolContext`] shares across the turn; a prompter that could
/// not move between threads would make the whole context single-threaded.
pub trait ApprovalPrompter: Send {
    fn request(&mut self, request: &ApprovalRequest) -> io::Result<ApprovalAnswer>;
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
    /// Both ends matter: a question fxr cannot show is as useless as an answer
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
                "\nfxr wants to {}\n  [y] once  [a] always for `{}`  [n] no: ",
                request.summary, request.target
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
// the session
// ---------------------------------------------------------------------------

/// The permission state one run of fxr carries.
///
/// Everything that can say yes lives here: the mode, the configured rules, the
/// approvals given so far, the channel that can ask for more, and the ledger of
/// authorities issued. Nothing outside this type can mint one.
pub struct PermissionSession {
    mode: PermissionMode,
    rules: PermissionRules,
    grants: Vec<Grant>,
    prompter: Option<Box<dyn ApprovalPrompter>>,
    ledger: AuthorityLedger,
}

impl PermissionSession {
    /// A session in `mode` with no rules, no grants, and no approval channel.
    ///
    /// No channel is the safe default: a session built without one refuses every
    /// `ask`-mode mutation rather than assuming a terminal is there.
    pub fn new(mode: PermissionMode) -> Self {
        Self {
            mode,
            rules: PermissionRules::default(),
            grants: Vec::new(),
            prompter: None,
            ledger: AuthorityLedger::new(),
        }
    }

    /// The same session with a way to ask the user.
    pub fn with_prompter(mut self, prompter: Box<dyn ApprovalPrompter>) -> Self {
        self.prompter = Some(prompter);
        self
    }

    /// The same session with configured rules.
    pub fn with_rules(mut self, rules: PermissionRules) -> Self {
        self.rules = rules;
        self
    }

    pub fn mode(&self) -> PermissionMode {
        self.mode
    }

    /// The approvals this session has been given, in the order they were given.
    pub fn grants(&self) -> &[Grant] {
        &self.grants
    }

    /// Records an approval directly. Used by the approval flow and by a caller
    /// restoring a session's grants.
    pub fn grant(&mut self, grant: Grant) {
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
        // `yolo` is documented as skipping policy, so it skips policy. Honoring
        // rules here would make the flag mean something narrower than it says.
        if self.mode == PermissionMode::Yolo {
            return PolicyDecision::Allow {
                source: AllowSource::Yolo,
            };
        }

        let tool = action.tool();
        let target = action.target();

        if self
            .rules
            .deny
            .iter()
            .any(|rule| rule.matches(tool, &target))
        {
            return PolicyDecision::Deny {
                cause: DenyCause::ConfiguredRule,
                reason: format!("a configured permission rule denies `{tool}` for `{target}`"),
            };
        }
        if self
            .rules
            .allow
            .iter()
            .any(|rule| rule.matches(tool, &target))
        {
            return PolicyDecision::Allow {
                source: AllowSource::ConfiguredRule,
            };
        }
        if self.grants.iter().any(|grant| grant.matches(tool, &target)) {
            return PolicyDecision::Allow {
                source: AllowSource::SessionGrant,
            };
        }

        match self.mode {
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

    /// Evaluates `action` and, when a human is needed, asks one.
    ///
    /// Never returns [`PolicyDecision::Prompt`]: by the time this returns, the
    /// question has either been answered or established to be unanswerable.
    pub fn decide(&mut self, action: ProposedAction<'_>) -> PolicyDecision {
        match self.evaluate(action) {
            PolicyDecision::Prompt => self.ask(action),
            decided => decided,
        }
    }

    /// Puts the question to the user, or reports that there is nobody to ask.
    fn ask(&mut self, action: ProposedAction<'_>) -> PolicyDecision {
        let tool = action.tool();
        let target = action.target();
        let Some(prompter) = self.prompter.as_mut() else {
            return PolicyDecision::Deny {
                cause: DenyCause::NoApprovalChannel,
                reason: format!(
                    "`ask` mode needs an interactive approval for `{tool}` on `{target}`, and this run has no approval channel; rerun in a terminal, or use --auto for bounded workspace changes"
                ),
            };
        };
        let request = ApprovalRequest {
            tool,
            target: target.clone(),
            summary: action.summary(),
        };
        match prompter.request(&request) {
            Ok(ApprovalAnswer::Once) => PolicyDecision::Allow {
                source: AllowSource::InteractiveOnce,
            },
            Ok(ApprovalAnswer::Always) => {
                self.grant(Grant::new(tool, target));
                PolicyDecision::Allow {
                    source: AllowSource::InteractiveAlways,
                }
            }
            Ok(ApprovalAnswer::Deny) => PolicyDecision::Deny {
                cause: DenyCause::UserDenied,
                reason: format!("you declined `{tool}` for `{target}`"),
            },
            Err(err) => PolicyDecision::Deny {
                cause: DenyCause::ApprovalChannelFailed,
                reason: format!("the approval channel failed: {err}"),
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
            .field("mode", &self.mode)
            .field("rules", &self.rules)
            .field("grants", &self.grants)
            .field("has_prompter", &self.prompter.is_some())
            .field("ledger", &self.ledger)
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
    fn a_deny_rule_outranks_an_allow_rule_for_the_same_target() {
        let plan = write_plan(TargetScope::PrimaryWorkspace);
        let session =
            PermissionSession::new(PermissionMode::Auto).with_rules(PermissionRules::new(
                vec![Rule::new("write_file", "a.txt")],
                vec![Rule::new("write_file", "a.txt")],
            ));
        assert!(matches!(
            session.evaluate(ProposedAction::Mutation(&plan)),
            PolicyDecision::Deny {
                cause: DenyCause::ConfiguredRule,
                ..
            }
        ));
    }

    #[test]
    fn a_rule_for_a_different_tool_does_not_apply() {
        let plan = write_plan(TargetScope::PrimaryWorkspace);
        let session = PermissionSession::new(PermissionMode::Ask).with_rules(PermissionRules::new(
            vec![Rule::new("edit_file", "a.txt")],
            Vec::new(),
        ));
        assert_eq!(
            session.evaluate(ProposedAction::Mutation(&plan)),
            PolicyDecision::Prompt
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
    fn a_summary_says_what_would_happen_rather_than_naming_a_policy() {
        let plan = write_plan(TargetScope::PrimaryWorkspace);
        let summary = ProposedAction::Mutation(&plan).summary();
        assert_eq!(summary, "write 5 bytes to a.txt");
    }

    #[test]
    fn the_default_session_can_authorize_nothing_on_its_own() {
        let mut session = PermissionSession::default();
        assert_eq!(session.mode(), PermissionMode::Ask);
        assert!(!session.has_prompter());
        let plan = write_plan(TargetScope::PrimaryWorkspace);
        assert!(matches!(
            session.decide(ProposedAction::Mutation(&plan)),
            PolicyDecision::Deny {
                cause: DenyCause::NoApprovalChannel,
                ..
            }
        ));
    }
}
