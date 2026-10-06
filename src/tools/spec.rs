//! What a tool *is*: an immutable specification plus the bounds it runs under.
//!
//! A [`ToolSpec`] is a `const` value. It owns its name, its description, its
//! closed JSON schema, its decoder, its validator, its permission kind, and its
//! executor, and none of them can be replaced at runtime. That is deliberate:
//! the schema xfx advertises and the code that runs when the model uses it are
//! the same object, so they cannot drift apart, and no caller can add a tool the
//! parity ledger has not accounted for
//! (`vercel-labs/fx@580a0c5d src/builtins/tools.zig:509-627`).
//!
//! Execution is staged -- decode, validate, admit, execute -- and each stage can
//! only refuse or hand a *more* specific value to the next one. A decoder cannot
//! read the filesystem and an executor never sees raw JSON, so "the model sent
//! nonsense" and "the filesystem said no" are different failures with different
//! messages (`src/core/tooling/tool_dispatch.zig:122-168`).

use std::sync::{Arc, Mutex, MutexGuard};

use serde_json::{Map, Value};

use crate::gateway::CancelToken;
use crate::permission::{ApprovalDiff, PermissionSession, ReadTracker};
use crate::workspace::AccessScope;

use super::mutate::{CreateFolderInput, EditFileInput, WriteFileInput};
use super::question::{AskUserQuestionInput, QuestionRequester};
use super::read::{GlobFilesInput, GrepFilesInput, ListFilesInput, ReadFileInput};
use super::terminal::TerminalInput;
use super::undo::ChangeJournal;

/// How much authority a tool needs before it may run.
///
/// The kind is declared on the spec rather than derived from the arguments, so
/// "is this call dangerous" is answered by the tool's identity and cannot be
/// changed by what the model sent
/// (`vercel-labs/fx@580a0c5d src/core/permissions/permission_gate.zig:59-70`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionKind {
    /// The tool observes the filesystem and changes nothing. Admitted in every
    /// permission mode: `ask` requires an approval for mutations and commands,
    /// not for reads (design, "Permissions").
    ReadOnly,
    /// The tool changes a file or a directory. Every call mints an authority.
    MutateFile,
    /// The tool starts a process. Every call mints an authority.
    RunCommand,
    /// The tool asks the user something and hands their answer back. It
    /// changes nothing on disk and starts no process, and answering it mints
    /// no authority for a later mutation or command call -- an interaction is
    /// a conversation, not a permission grant, so it is not `ReadOnly` either:
    /// that kind is reserved for a tool the safety boundary in `docs/parity.md`
    /// documents as admitted in every permission mode because it changes
    /// nothing, and this row exists so a reviewer never has to ask which one a
    /// new interactive tool is by squinting at what it does.
    Interaction,
}

impl PermissionKind {
    /// Whether a call of this kind must cross a permission decision.
    ///
    /// Exhaustive on purpose, not `matches!`: a future `PermissionKind`
    /// variant fails to compile here until someone decides which side of the
    /// line it falls on, rather than silently inheriting `false`.
    pub fn requires_authority(self) -> bool {
        match self {
            Self::ReadOnly | Self::Interaction => false,
            Self::MutateFile | Self::RunCommand => true,
        }
    }
}

/// The scalar types a tool input field may have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyKind {
    String,
    Integer,
    Boolean,
    Array,
}

impl PropertyKind {
    fn label(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Integer => "integer",
            Self::Boolean => "boolean",
            Self::Array => "array",
        }
    }
}

/// The shape of a bounded array field: what its items look like, and how many
/// of them are allowed.
///
/// The item schema is itself a closed object -- never a bare scalar or a
/// further-nested array -- so a field of this kind adds exactly one array edge
/// (`InputSchema` -> `ArraySpec::items` -> `InputSchema`) rather than an
/// unbounded one.
#[derive(Debug, Clone, Copy)]
pub struct ArraySpec {
    pub items: &'static InputSchema,
    pub min_items: usize,
    pub max_items: usize,
}

/// One field of a tool's input schema.
#[derive(Debug, Clone, Copy)]
pub struct Property {
    pub name: &'static str,
    pub kind: PropertyKind,
    pub description: &'static str,
    /// The complete set of accepted values, when the field is an enumeration.
    pub allowed: &'static [&'static str],
    /// The item schema and bounds, when the field is `PropertyKind::Array`.
    pub array: Option<ArraySpec>,
}

/// A tool's input schema: a closed object of typed scalars.
///
/// Closed is the point. `additionalProperties: false` is written for every
/// tool, so a field the model invents is rejected by the provider instead of
/// being silently dropped by xfx, and the model learns its mistake in the same
/// turn.
#[derive(Debug, Clone, Copy)]
pub struct InputSchema {
    pub properties: &'static [Property],
    pub required: &'static [&'static str],
}

impl InputSchema {
    /// The JSON Schema fragment sent as `inputSchema`.
    pub fn to_json(self) -> Value {
        let mut properties = Map::new();
        for property in self.properties {
            let mut rendered = Map::new();
            rendered.insert("type".to_string(), Value::from(property.kind.label()));
            rendered.insert("description".to_string(), Value::from(property.description));
            if !property.allowed.is_empty() {
                rendered.insert("enum".to_string(), Value::from(property.allowed.to_vec()));
            }
            if let Some(spec) = property.array {
                rendered.insert("items".to_string(), spec.items.to_json());
                rendered.insert("minItems".to_string(), Value::from(spec.min_items));
                rendered.insert("maxItems".to_string(), Value::from(spec.max_items));
            }
            properties.insert(property.name.to_string(), Value::Object(rendered));
        }

        let mut schema = Map::new();
        schema.insert("type".to_string(), Value::from("object"));
        schema.insert("properties".to_string(), Value::Object(properties));
        schema.insert("additionalProperties".to_string(), Value::Bool(false));
        if !self.required.is_empty() {
            schema.insert("required".to_string(), Value::from(self.required.to_vec()));
        }
        Value::Object(schema)
    }
}

/// A decoded, typed tool input.
///
/// The set is closed and matches the registry exactly, so dispatch is a `match`
/// rather than a downcast: a spec whose decoder and executor disagree about the
/// input type would not compile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolInput {
    ListFiles(ListFilesInput),
    GlobFiles(GlobFilesInput),
    GrepFiles(GrepFilesInput),
    ReadFile(ReadFileInput),
    WriteFile(WriteFileInput),
    EditFile(EditFileInput),
    CreateFolder(CreateFolderInput),
    Terminal(TerminalInput),
    AskUserQuestion(AskUserQuestionInput),
}

impl ToolInput {
    /// The one argument a person watching the call is shown: the path, the
    /// pattern or the command (upstream's `label_arg_kind`,
    /// `vercel-labs/fx@c1db9191 src/builtins/tools.zig`).
    ///
    /// Exhaustive, so a tool added later has to say what its row names.
    /// `None` is an answer and not an omission: a `list_files` with no path
    /// lists the workspace root, and a question has no argument a row could
    /// carry -- its row is never drawn ([`Activity::Ask`]).
    pub fn target(&self) -> Option<&str> {
        match self {
            Self::ListFiles(input) => input.path.as_deref(),
            Self::GlobFiles(input) => Some(&input.pattern),
            Self::GrepFiles(input) => Some(&input.pattern),
            Self::ReadFile(input) => Some(&input.path),
            Self::WriteFile(input) => Some(&input.path),
            Self::EditFile(input) => Some(&input.path),
            Self::CreateFolder(input) => Some(&input.path),
            Self::Terminal(input) => Some(&input.command),
            Self::AskUserQuestion(_) => None,
        }
    }
}

/// What kind of work a call is, as a person watching the turn counts it
/// (upstream's `ToolActivityKind`, `vercel-labs/fx@c1db9191
/// src/core/shared/types.zig`).
///
/// Not [`PermissionKind`], and the difference is the point: a grep and a read
/// are both `ReadOnly` and both count as a read, but a write and an edit are
/// both `MutateFile` and are counted apart (`tool_group_projection.zig:140-150`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activity {
    Read,
    List,
    Write,
    Edit,
    Command,
    /// A question to the user. Upstream draws no tool row for it at all
    /// (`tool_presentation.zig` `startToolVisibleLifecycle` returns before the
    /// lifecycle starts), and the surface that answers it is its own.
    Ask,
}

/// What a person watching a turn is told about a call to one tool, before and
/// after it runs.
///
/// The four facts upstream declares on every tool spec
/// (`vercel-labs/fx@c1db9191 src/builtins/tools.zig`: `activity_kind`,
/// `action_label`, `completed_action_label`, `label_arg_default`) and builds
/// the transcript's tool rows from (`tool_group_projection.zig`). Declared on
/// the spec for the reason the schema is: the tool that runs and the row that
/// names it are one object and cannot drift apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Presentation {
    pub activity: Activity,
    /// The verb while the call runs: `Reading`, `Running`.
    pub running: &'static str,
    /// The verb once it has: `Read`, `Ran`.
    pub done: &'static str,
    /// What the row names when the call's own argument cannot be shown --
    /// it did not decode, or (for a file change) it never got as far as
    /// naming a file the user approved.
    pub default_target: &'static str,
}

impl Presentation {
    pub const fn new(
        activity: Activity,
        running: &'static str,
        done: &'static str,
        default_target: &'static str,
    ) -> Self {
        Self {
            activity,
            running,
            done,
            default_target,
        }
    }
}

/// One call, as a person watching the turn is shown it before it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    pub presentation: Presentation,
    /// The call's own argument ([`ToolInput::target`]), or `None` when it had
    /// none or did not decode.
    pub target: Option<String>,
}

/// How a call ended, for the person watching it -- beside [`ToolResult::ok`],
/// which is the same question answered for the model.
///
/// The two differ exactly where a row has to say more than "it worked": a
/// command that ran and exited 3 answered the model's question (`ok`) and is
/// still a failure on the screen (upstream's `commandOutcomeDecision`,
/// `tool_presentation.zig`), and a refusal the user gave is a denial rather
/// than the tool's own failure. Nothing here changes what the model is told.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Outcome {
    /// Whatever `ok` says: the call did what it was asked, or it refused on
    /// its own.
    #[default]
    Finished,
    /// A file change that would have left the file as it is, so nothing was
    /// written (upstream's `resultIsNoop`, `file_mutation_contract.zig:492`).
    Unchanged,
    /// A permission decision refused it, so it never ran.
    Denied,
    /// A command ran to its end with this status.
    Exited(i32),
    /// A command a signal stopped.
    Signalled(i32),
    /// A command xfx stopped for running past its ceiling.
    TimedOut,
    /// A command xfx stopped because the turn was cancelled.
    Cancelled,
}

/// How many lines a file change added and removed, counted the way upstream's
/// review diff counts them (`vercel-labs/fx@c1db9191
/// src/core/output/diff.zig:794-860`, `countStats`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LineChanges {
    pub added: usize,
    pub removed: usize,
}

/// How many cells of the longest-common-subsequence table
/// [`LineChanges::between`] fills before it stops being exact.
///
/// The table is upstream's algorithm (`diff.zig:794-860`), and its cost is the
/// product of the two sides' changed lines. A change confined to a few lines
/// costs next to nothing -- the common head and tail are trimmed first -- but a
/// rewrite of a large file would spend seconds of the turn on a number for a
/// row; past this the count is the changed region's two sizes, which is an
/// upper bound on what a real diff would say rather than a guess.
const MAX_DIFF_CELLS: usize = 1 << 22;

impl LineChanges {
    /// The lines `after` added to `before` and the lines it took away.
    pub fn between(before: &str, after: &str) -> Self {
        let mut old: Vec<&str> = lines(before);
        let mut new: Vec<&str> = lines(after);
        // A file that gained or lost its final newline changed one line's
        // worth, and upstream counts it as one (`appendTrailingNewlineMarker`).
        // The marker cannot equal a real line: no line holds a NUL and a break.
        if !before.is_empty() && !after.is_empty() {
            match (before.ends_with('\n'), after.ends_with('\n')) {
                (true, false) => old.push("\u{0}\n"),
                (false, true) => new.push("\u{0}\n"),
                _ => {}
            }
        }
        let head = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
        let tail = old[head..]
            .iter()
            .rev()
            .zip(new[head..].iter().rev())
            .take_while(|(a, b)| a == b)
            .count();
        let old = &old[head..old.len() - tail];
        let new = &new[head..new.len() - tail];
        let common = if old.len().saturating_mul(new.len()) <= MAX_DIFF_CELLS {
            common_lines(old, new)
        } else {
            0
        };
        Self {
            added: new.len() - common,
            removed: old.len() - common,
        }
    }
}

/// `text`'s lines without their breaks, and no empty line after a final one
/// (`diff.zig` `appendLines`).
fn lines(text: &str) -> Vec<&str> {
    let mut lines: Vec<&str> = text.split('\n').collect();
    if text.is_empty() || text.ends_with('\n') {
        lines.pop();
    }
    lines
}

/// The length of the longest common subsequence of two line lists, one table
/// row at a time.
fn common_lines(old: &[&str], new: &[&str]) -> usize {
    let mut above = vec![0usize; new.len() + 1];
    let mut row = vec![0usize; new.len() + 1];
    for line in old {
        for (index, other) in new.iter().enumerate() {
            row[index + 1] = if line == other {
                above[index] + 1
            } else {
                row[index].max(above[index + 1])
            };
        }
        std::mem::swap(&mut above, &mut row);
    }
    above[new.len()]
}

/// Turns raw model JSON into a typed input, or says why it cannot.
pub type ToolDecoder = fn(&Value) -> Result<ToolInput, String>;

/// Checks a decoded input without touching the filesystem.
pub type ToolValidator = fn(&ToolInput) -> Result<(), String>;

/// Runs a validated input against a scope and returns a model-visible result.
pub type ToolExecutor = fn(&ToolInput, &ToolContext) -> ToolResult;

/// What one tool call produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolResult {
    /// Whether the tool did what it was asked.
    ///
    /// A refusal is a real result, not an error: the model is told exactly what
    /// went wrong and can correct itself in the next step.
    pub ok: bool,
    /// The text the model sees.
    pub output: String,
    /// A one-line summary for the `tool_result` event a human watches.
    pub detail: String,
    /// Whether this result ends the turn instead of going back to the model.
    ///
    /// Almost always false: a refusal is information the model can act on. It is
    /// true for exactly one situation -- an authority that was granted and then
    /// stopped describing the world, because the file moved underneath it. That
    /// is not an argument the model can fix, and letting it retry would let
    /// whoever won the race keep racing.
    pub fatal: bool,
    /// What the user said when they answered the approval this call needed.
    ///
    /// Delivered as a **separate user message after this result**, never merged
    /// into [`Self::output`]: merging would let a user's sentence be read as the
    /// tool's own report of what it did. It is context, not authority -- the
    /// call that ran is the one that was judged, whatever this says.
    pub feedback: Option<String>,
    /// How the call ended, for the person watching rather than for the model
    /// ([`Outcome`]). Never part of [`Self::output`].
    pub outcome: Outcome,
    /// The lines a file change added and removed, when it made one.
    pub changes: Option<LineChanges>,
    /// The change itself, when it made one: the part of the file it touched,
    /// before and after, as the plan that was judged carries it
    /// (`crate::permission::ApprovalDiff::around_change`). For the person
    /// watching -- upstream keeps one per change for its full transcript --
    /// and never part of [`Self::output`] or of the session's log. Boxed: a
    /// result is returned by value through every refusal path, and most
    /// carry no change.
    pub change: Option<Box<ApprovalDiff>>,
}

/// What follows the tool's name in every refusal ([`ToolResult::refused`]).
const REFUSED: &str = " was not permitted:";

/// Whether `output`, a failed result of `tool`, is a refusal
/// ([`ToolResult::refused`]) -- read back from a session log, which keeps the
/// result's words and not its [`Outcome`].
pub fn is_refusal(tool: &str, output: &str) -> bool {
    output
        .strip_prefix(tool)
        .is_some_and(|rest| rest.starts_with(REFUSED))
}

impl ToolResult {
    /// A completed call. The summary is bounded like a refusal's, so no result
    /// can flood the event stream a human is watching.
    pub fn success(output: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            ok: true,
            output: output.into(),
            detail: summarize(&detail.into()),
            fatal: false,
            feedback: None,
            outcome: Outcome::Finished,
            changes: None,
            change: None,
        }
    }

    /// The same result, ending as `outcome` for the person watching.
    pub fn ending(mut self, outcome: Outcome) -> Self {
        self.outcome = outcome;
        self
    }

    /// The same result, carrying the lines its change added and removed.
    pub fn with_changes(mut self, changes: Option<LineChanges>) -> Self {
        self.changes = changes;
        self
    }

    /// The same result, carrying the change it made.
    pub fn with_change(mut self, change: Option<ApprovalDiff>) -> Self {
        self.change = change.map(Box::new);
        self
    }

    /// A refusal. The summary is the first line of the explanation, bounded, so
    /// a long filesystem error cannot flood a human's event stream.
    pub fn failure(output: impl Into<String>) -> Self {
        let output = output.into();
        let detail = summarize(&output);
        Self {
            ok: false,
            output,
            detail,
            fatal: false,
            feedback: None,
            outcome: Outcome::Finished,
            changes: None,
            change: None,
        }
    }

    /// A call the permission policy or the person refused: `{tool} was not
    /// permitted: {reason}`, ending [`Outcome::Denied`] -- the one shape every
    /// refusal is written in, so a replayed log can tell it from any other
    /// failure ([`is_refusal`]) and say it as upstream says one
    /// (`execution-denied`, `crate::gateway::protocol::ResultStatus::Denied`).
    pub fn refused(tool: &str, reason: &str) -> Self {
        Self::failure(format!("{tool}{REFUSED} {reason}")).ending(Outcome::Denied)
    }

    /// A refusal that ends the turn: the authority for this call stopped being
    /// true before it could be spent.
    pub fn revoked(output: impl Into<String>) -> Self {
        Self {
            fatal: true,
            ..Self::failure(output)
        }
    }

    /// Attaches what the user said. Blank is absent.
    ///
    /// A draft of spaces is not a sentence, and an empty user message on the
    /// wire is worse than none: it would spend a turn saying nothing.
    pub fn with_feedback(mut self, feedback: Option<String>) -> Self {
        self.feedback = feedback.filter(|text| !text.trim().is_empty());
        self
    }
}

/// The first line of `text`, at most 160 bytes, on a character boundary.
fn summarize(text: &str) -> String {
    let line = text.lines().next().unwrap_or("");
    match clip(line, 160) {
        Some(clipped) => format!("{clipped}..."),
        None => line.to_string(),
    }
}

/// `text` clipped to at most `limit` bytes, or `None` when it already fits.
pub(crate) fn clip(text: &str, limit: usize) -> Option<&str> {
    if text.len() <= limit {
        return None;
    }
    let mut end = limit;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    Some(&text[..end])
}

/// The fixed ceilings every read tool runs under.
///
/// They exist so one tool call cannot consume a turn's context window or its
/// wall clock. Each default is upstream's; the field exists so a later slice can
/// lower one without editing four executors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolLimits {
    /// Directory entries `list_files` shows (`tool_dispatch.zig:46`).
    pub max_list_entries: usize,
    /// Lines `read_file` shows, whether or not the model names a count
    /// (`tool_dispatch.zig:48`).
    pub max_read_lines: usize,
    /// Bytes of one line `read_file` shows before clipping it
    /// (`tool_dispatch.zig:49`).
    pub max_read_line_len: usize,
    /// Bytes of a file `read_file` will load (`read_file.zig:14`).
    pub max_read_bytes: usize,
    /// Bytes of one tool result the model may receive (`read_file.zig:16`).
    pub max_output_bytes: usize,
    /// Paths `glob_files` lists (`tool_dispatch.zig:46`).
    pub max_glob_matches: usize,
    /// Matches `grep_files` renders (`src/core/workspace/grep_search.zig:14`).
    pub max_grep_matches: usize,
    /// Matches `grep_files` collects before it stops scanning
    /// (`grep_search.zig:17`).
    pub max_grep_scan: usize,
    /// Bytes of one file `grep_files` will search
    /// (`src/tools/filesystem/grep_files.zig:14`).
    pub max_grep_file_bytes: usize,
    /// Lines of context `grep_files` will show around a match
    /// (`grep_files.zig:13`).
    pub max_context_lines: usize,
    /// Files a walk will consider before it reports itself incomplete
    /// (`src/core/workspace/workspace_files.zig:10`).
    pub max_candidates: usize,
    /// Bytes a single mutation may write
    /// (`src/tools/filesystem/edit_file.zig:6`).
    pub max_mutation_bytes: usize,
    /// Bytes of one command's captured output, per stream
    /// (`src/core/permissions/direct_command.zig:10`).
    pub max_command_output_bytes: usize,
    /// How long one command may run before it is killed.
    ///
    /// An xfx value. Upstream's foreground executor takes an optional timeout
    /// and its callers supply their own (`sandbox.zig:152`); a coding agent that
    /// appears to hang is indistinguishable from a broken one, so xfx always has
    /// a ceiling.
    pub command_timeout_ms: u64,
}

impl Default for ToolLimits {
    fn default() -> Self {
        Self {
            max_list_entries: 100,
            max_read_lines: 400,
            max_read_line_len: 2_000,
            max_read_bytes: 10 * 1024 * 1024,
            max_output_bytes: 256 * 1024,
            max_glob_matches: 100,
            max_grep_matches: 200,
            max_grep_scan: 2_000,
            max_grep_file_bytes: 200 * 1024,
            max_context_lines: 5,
            max_candidates: 100_000,
            max_mutation_bytes: 4 * 1024 * 1024,
            max_command_output_bytes: 64 * 1024,
            command_timeout_ms: 120_000,
        }
    }
}

/// A callback run at the exact moment an authority is minted and not yet spent.
///
/// This is the race window, and it is the only part of the mutation path that
/// cannot be observed from outside the process. A test installs a closure here
/// to change the filesystem at precisely that instant and prove the stale
/// authority is refused. Nothing in the product installs one: `ToolContext`
/// leaves it `None`, and `app.rs` never sets it.
pub type RaceInterlude = Arc<dyn Fn() + Send + Sync>;

/// The mutable state one run of xfx shares across every tool call.
///
/// Behind an `Arc` so that cloning a [`ToolContext`] -- which the turn does --
/// shares one set of read proofs and one permission ledger rather than forking
/// them. A session grant given during step two has to be visible in step three.
pub struct ToolSession {
    permissions: Mutex<PermissionSession>,
    reads: Mutex<ReadTracker>,
    /// What `/undo` can take back ([`super::undo`]). Here, beside the read
    /// proofs, because it is about the same thing they are -- what this
    /// conversation has done to files -- and so it ends where they do: a
    /// conversation `/new` drops takes its changes with it, and a resumed one
    /// starts with none, as upstream's does.
    changes: Mutex<ChangeJournal>,
}

impl ToolSession {
    pub fn new(permissions: PermissionSession) -> Self {
        Self {
            permissions: Mutex::new(permissions),
            reads: Mutex::new(ReadTracker::new()),
            changes: Mutex::new(ChangeJournal::new()),
        }
    }
}

impl std::fmt::Debug for ToolSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolSession").finish_non_exhaustive()
    }
}

/// Everything a tool executor is allowed to know.
///
/// It carries no provider and no credential: a tool cannot reach the network or
/// the model even by accident. What it does carry is the authority to change
/// things -- the scope, the bounds, the permission session, and the turn's
/// cancellation flag -- because those are exactly the facts an executor needs
/// and nothing more.
#[derive(Clone)]
pub struct ToolContext {
    scope: AccessScope,
    limits: ToolLimits,
    session: Arc<ToolSession>,
    cancel: CancelToken,
    interlude: Option<RaceInterlude>,
    /// Who `ask_user_question` may hand a batch to. Absent by default -- the
    /// tools layer never names the TUI, so this is set by whichever front end
    /// built the context, and a context nobody set it on answers with the
    /// availability sentinel rather than reaching for a UI that isn't there.
    questioner: Option<Arc<dyn QuestionRequester>>,
}

impl std::fmt::Debug for ToolContext {
    /// The interlude is a closure with no representation, so its presence is
    /// printed rather than its identity -- and printing it at all is the point:
    /// a context with one installed is a test context.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolContext")
            .field("scope", &self.scope)
            .field("limits", &self.limits)
            .field("session", &self.session)
            .field("cancelled", &self.cancel.is_cancelled())
            .field("has_race_interlude", &self.interlude.is_some())
            .field("has_questioner", &self.questioner.is_some())
            .finish()
    }
}

impl ToolContext {
    /// A context with the shipped limits and the most restrictive session.
    ///
    /// The default session is `ask` with no approval channel, so a context built
    /// without an explicit permission session can read but cannot change
    /// anything. That is the safe direction to be wrong in.
    pub fn new(scope: AccessScope) -> Self {
        Self::with_limits(scope, ToolLimits::default())
    }

    /// A context with explicit limits, so a test can prove a bound without
    /// building a fixture large enough to hit the shipped one.
    pub fn with_limits(scope: AccessScope, limits: ToolLimits) -> Self {
        Self {
            scope,
            limits,
            session: Arc::new(ToolSession::new(PermissionSession::default())),
            cancel: CancelToken::new(),
            interlude: None,
            questioner: None,
        }
    }

    /// The same context running under `permissions`.
    pub fn with_permissions(mut self, permissions: PermissionSession) -> Self {
        self.session = Arc::new(ToolSession::new(permissions));
        self
    }

    /// The same context, cancelled by `cancel`.
    ///
    /// A long-running command has to stop when the user presses Ctrl-C, and the
    /// turn's token is the one thing that already means that.
    pub fn with_cancel(mut self, cancel: CancelToken) -> Self {
        self.cancel = cancel;
        self
    }

    /// The same context with a [`RaceInterlude`] installed. Tests only.
    pub fn with_race_interlude(mut self, interlude: RaceInterlude) -> Self {
        self.interlude = Some(interlude);
        self
    }

    /// The same context, with `questioner` as what `ask_user_question` calls
    /// to show a batch and collect the answers.
    pub fn with_questioner(mut self, questioner: Arc<dyn QuestionRequester>) -> Self {
        self.questioner = Some(questioner);
        self
    }

    /// The requester `ask_user_question` may hand a batch to, if this context
    /// was built with one.
    pub fn questioner(&self) -> Option<&Arc<dyn QuestionRequester>> {
        self.questioner.as_ref()
    }

    pub fn scope(&self) -> &AccessScope {
        &self.scope
    }

    pub fn limits(&self) -> &ToolLimits {
        &self.limits
    }

    pub fn cancel(&self) -> &CancelToken {
        &self.cancel
    }

    /// The permission session, locked.
    ///
    /// A poisoned lock is recovered rather than propagated: a panic in one tool
    /// executor must not make every later permission decision panic too, and the
    /// session's invariants are "a set of nonces and a list of grants", which a
    /// partial update cannot corrupt.
    pub fn permissions(&self) -> MutexGuard<'_, PermissionSession> {
        self.session
            .permissions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// This session's read proofs, locked.
    pub fn reads(&self) -> MutexGuard<'_, ReadTracker> {
        self.session
            .reads
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The changes this session's tools committed, locked
    /// ([`super::undo::ChangeJournal`]).
    ///
    /// Recovered from a poisoned lock like the other two: a journal is a list
    /// that a push or a pop leaves whole.
    pub fn changes(&self) -> MutexGuard<'_, ChangeJournal> {
        self.session
            .changes
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Runs the installed interlude, if a test installed one.
    pub(crate) fn run_race_interlude(&self) {
        if let Some(interlude) = &self.interlude {
            interlude();
        }
    }
}

/// One tool, whole: what it is called, what it accepts, and what it does.
#[derive(Clone, Copy)]
pub struct ToolSpec {
    name: &'static str,
    description: &'static str,
    permission: PermissionKind,
    presentation: Presentation,
    input_schema: InputSchema,
    decode: ToolDecoder,
    validate: ToolValidator,
    execute: ToolExecutor,
}

impl ToolSpec {
    /// Builds a spec. `const` so the registry is a static table rather than
    /// something assembled -- and therefore mutable -- at startup.
    ///
    /// Eight arguments, deliberately rather than a builder: every one of them
    /// is part of what the tool *is*, and a builder would let a spec leave one
    /// out and inherit a default nobody chose -- a row with no verb, or a tool
    /// with no validator.
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        name: &'static str,
        description: &'static str,
        permission: PermissionKind,
        presentation: Presentation,
        input_schema: InputSchema,
        decode: ToolDecoder,
        validate: ToolValidator,
        execute: ToolExecutor,
    ) -> Self {
        Self {
            name,
            description,
            permission,
            presentation,
            input_schema,
            decode,
            validate,
            execute,
        }
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn description(&self) -> &'static str {
        self.description
    }

    pub fn permission(&self) -> PermissionKind {
        self.permission
    }

    pub fn presentation(&self) -> Presentation {
        self.presentation
    }

    /// What a person watching is shown about a call with these arguments,
    /// before it runs.
    ///
    /// Decoded here rather than read off the raw JSON, so the row names the
    /// argument the executor will act on and not a field of the same name the
    /// decoder would refuse. A decoder touches nothing (see the module
    /// documentation), so decoding twice costs a parse and nothing else; an
    /// input that does not decode is shown by the tool's default, and its
    /// refusal follows in the result.
    pub fn shown(&self, input: &Value) -> Shown {
        Shown {
            presentation: self.presentation,
            target: (self.decode)(input)
                .ok()
                .and_then(|decoded| decoded.target().map(str::to_string)),
        }
    }

    pub fn input_schema(&self) -> InputSchema {
        self.input_schema
    }

    /// The Gateway's flattened function-tool envelope
    /// (`vercel-labs/fx@580a0c5d src/core/tooling/gateway_schema.zig:82-104`).
    ///
    /// Keys sorted at every depth. JSON objects keep the order they were
    /// built in (`serde_json`'s `preserve_order`, which settings need: a
    /// permission table is read last-match-wins in document order), and the
    /// bytes every model has been sent are the sorted ones -- a schema whose
    /// keys moved would be a new prefix to every provider's prompt cache.
    pub fn advertisement(&self) -> Value {
        let mut envelope = Map::new();
        envelope.insert("type".to_string(), Value::from("function"));
        envelope.insert("name".to_string(), Value::from(self.name));
        envelope.insert("description".to_string(), Value::from(self.description));
        envelope.insert("inputSchema".to_string(), self.input_schema.to_json());
        let mut envelope = Value::Object(envelope);
        envelope.sort_all_objects();
        envelope
    }

    /// Decodes and validates one call's arguments, and runs nothing: the
    /// typed input [`Self::run`] would hand the executor.
    pub fn decode(&self, input: &Value) -> Result<ToolInput, String> {
        let decoded = (self.decode)(input)?;
        (self.validate)(&decoded)?;
        Ok(decoded)
    }

    /// Decodes, validates, and runs one call's arguments.
    ///
    /// The stages are separate so that a refusal names the stage it came from.
    /// Nothing here reaches the filesystem until the input is typed and
    /// self-consistent.
    pub fn run(&self, input: &Value, context: &ToolContext) -> ToolResult {
        let decoded = match (self.decode)(input) {
            Ok(decoded) => decoded,
            Err(reason) => return ToolResult::failure(reason),
        };
        if let Err(reason) = (self.validate)(&decoded) {
            return ToolResult::failure(reason);
        }
        (self.execute)(&decoded, context)
    }
}

impl std::fmt::Debug for ToolSpec {
    /// Function pointers are not worth printing; the identity is the name.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolSpec")
            .field("name", &self.name)
            .field("permission", &self.permission)
            .finish_non_exhaustive()
    }
}

// ---------------------------------------------------------------------------
// shared decoding
// ---------------------------------------------------------------------------
//
// Every decoder is strict: a field of the wrong type is a refusal naming the
// field, not a value quietly treated as absent. The schema is closed and typed,
// so "the model sent `path: 7`" is a mistake the model can only fix if it is
// told.

pub(crate) fn object<'a>(tool: &str, input: &'a Value) -> Result<&'a Map<String, Value>, String> {
    input
        .as_object()
        .ok_or_else(|| format!("{tool} arguments must be a JSON object"))
}

pub(crate) fn required_string(
    tool: &str,
    object: &Map<String, Value>,
    key: &str,
) -> Result<String, String> {
    match object.get(key) {
        None => Err(format!("{tool} requires the string field `{key}`")),
        Some(Value::String(value)) => Ok(value.clone()),
        Some(_) => Err(format!("{tool} field `{key}` must be a string")),
    }
}

pub(crate) fn optional_string(
    tool: &str,
    object: &Map<String, Value>,
    key: &str,
) -> Result<Option<String>, String> {
    match object.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(format!("{tool} field `{key}` must be a string")),
    }
}

pub(crate) fn optional_bool(
    tool: &str,
    object: &Map<String, Value>,
    key: &str,
) -> Result<bool, String> {
    match object.get(key) {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(value)) => Ok(*value),
        Some(_) => Err(format!("{tool} field `{key}` must be a boolean")),
    }
}

/// An optional integer field with an inclusive lower bound.
pub(crate) fn optional_integer(
    tool: &str,
    object: &Map<String, Value>,
    key: &str,
    minimum: u64,
) -> Result<Option<usize>, String> {
    let Some(value) = object.get(key) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let described = if minimum == 0 {
        "non-negative"
    } else {
        "positive"
    };
    let number = value
        .as_u64()
        .filter(|number| *number >= minimum)
        .ok_or_else(|| format!("{tool} field `{key}` must be a {described} integer"))?;
    Ok(Some(usize::try_from(number).unwrap_or(usize::MAX)))
}

/// An optional field whose value must be one of `allowed`.
pub(crate) fn optional_enum(
    tool: &str,
    object: &Map<String, Value>,
    key: &str,
    allowed: &[&str],
) -> Result<Option<String>, String> {
    let Some(raw) = optional_string(tool, object, key)? else {
        return Ok(None);
    };
    if allowed.contains(&raw.as_str()) {
        return Ok(Some(raw));
    }
    Err(format!(
        "{tool} field `{key}` must be one of {}",
        allowed.join(", ")
    ))
}

/// Rejects a field that is present but blank once trimmed.
pub(crate) fn nonblank(tool: &str, key: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{tool} field `{key}` must not be empty"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const SAMPLE: InputSchema = InputSchema {
        properties: &[
            Property {
                name: "path",
                kind: PropertyKind::String,
                description: "A path.",
                allowed: &[],
                array: None,
            },
            Property {
                name: "mode",
                kind: PropertyKind::String,
                description: "A mode.",
                allowed: &["matches", "count"],
                array: None,
            },
        ],
        required: &["path"],
    };

    #[test]
    fn a_refusal_is_told_from_any_other_failure_by_its_own_shape() {
        let refused = ToolResult::refused("write_file", "you declined");
        assert_eq!(refused.output, "write_file was not permitted: you declined");
        assert_eq!(refused.outcome, Outcome::Denied);
        assert!(!refused.ok);
        assert!(is_refusal("write_file", &refused.output));
        assert!(
            !is_refusal("edit_file", &refused.output),
            "another tool's refusal"
        );
        assert!(!is_refusal("write_file", "write_file failed: disk full"));
        assert!(!is_refusal("terminal", "aborted by user"));
    }

    #[test]
    fn a_schema_is_always_closed() {
        let rendered = SAMPLE.to_json();
        assert_eq!(rendered["additionalProperties"], json!(false));
        assert_eq!(rendered["type"], "object");
        assert_eq!(rendered["required"], json!(["path"]));
        assert_eq!(rendered["properties"]["path"]["type"], "string");
        assert_eq!(
            rendered["properties"]["mode"]["enum"],
            json!(["matches", "count"])
        );
        assert!(rendered["properties"]["path"].get("enum").is_none());
    }

    #[test]
    fn a_wrong_typed_field_is_refused_by_name_rather_than_ignored() {
        let object = json!({ "path": 7, "flag": "yes", "count": -1 });
        let object = object.as_object().unwrap();
        assert_eq!(
            required_string("t", object, "path").unwrap_err(),
            "t field `path` must be a string"
        );
        assert_eq!(
            optional_bool("t", object, "flag").unwrap_err(),
            "t field `flag` must be a boolean"
        );
        assert_eq!(
            optional_integer("t", object, "count", 0).unwrap_err(),
            "t field `count` must be a non-negative integer"
        );
    }

    #[test]
    fn an_absent_or_null_optional_field_is_absent() {
        let object = json!({ "path": null });
        let object = object.as_object().unwrap();
        assert_eq!(optional_string("t", object, "path").unwrap(), None);
        assert_eq!(optional_string("t", object, "missing").unwrap(), None);
        assert_eq!(optional_integer("t", object, "missing", 1).unwrap(), None);
        assert!(!optional_bool("t", object, "missing").unwrap());
    }

    #[test]
    fn a_missing_required_field_names_the_tool_and_the_field() {
        let object = json!({});
        let message =
            required_string("read_file", object.as_object().unwrap(), "path").unwrap_err();
        assert_eq!(message, "read_file requires the string field `path`");
    }

    #[test]
    fn an_enum_field_lists_what_it_would_have_accepted() {
        let object = json!({ "mode": "sideways" });
        let message =
            optional_enum("t", object.as_object().unwrap(), "mode", &["a", "b"]).unwrap_err();
        assert_eq!(message, "t field `mode` must be one of a, b");
    }

    #[test]
    fn clipping_never_splits_a_character() {
        assert_eq!(clip("short", 10), None);
        // Each `한` is three bytes, so a nine-byte clip of a twelve-byte string
        // must land on a boundary rather than mid-character.
        assert_eq!(clip("한한한한", 10), Some("한한한"));
        assert_eq!(clip("abcdef", 3), Some("abc"));
    }

    #[test]
    fn a_bounded_array_of_closed_objects_renders_items_and_bounds() {
        static ITEM: InputSchema = InputSchema {
            properties: &[Property {
                name: "label",
                kind: PropertyKind::String,
                description: "d",
                allowed: &[],
                array: None,
            }],
            required: &["label"],
        };
        static OUTER: InputSchema = InputSchema {
            properties: &[Property {
                name: "questions",
                kind: PropertyKind::Array,
                description: "d",
                allowed: &[],
                array: Some(ArraySpec {
                    items: &ITEM,
                    min_items: 1,
                    max_items: 4,
                }),
            }],
            required: &["questions"],
        };
        let field = &OUTER.to_json()["properties"]["questions"];
        assert_eq!(field["type"], "array");
        assert_eq!(field["minItems"], 1);
        assert_eq!(field["maxItems"], 4);
        assert_eq!(field["items"]["type"], "object");
        assert_eq!(field["items"]["additionalProperties"], false);
        assert_eq!(field["items"]["required"][0], "label");
        // The whole nested shape, closed at every level -- not just a few keys.
        assert_eq!(
            field,
            &json!({
                "type": "array",
                "description": "d",
                "minItems": 1,
                "maxItems": 4,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["label"],
                    "properties": {
                        "label": { "type": "string", "description": "d" }
                    }
                }
            })
        );
    }

    #[test]
    fn a_failure_summary_is_the_first_line_and_is_bounded() {
        let result = ToolResult::failure("no such path: `x`\nmore detail");
        assert!(!result.ok);
        assert_eq!(result.detail, "no such path: `x`");

        let long = ToolResult::failure("e".repeat(400));
        assert!(long.detail.len() <= 164, "{}", long.detail.len());
        assert!(long.detail.ends_with("..."));
    }

    fn changes(before: &str, after: &str) -> (usize, usize) {
        let counted = LineChanges::between(before, after);
        (counted.added, counted.removed)
    }

    #[test]
    fn a_new_file_adds_every_line_it_has_and_removes_none() {
        // Upstream's capture of the same write: `Wrote c.txt +2`.
        assert_eq!(changes("", "new file\nsecond line\n"), (2, 0));
        assert_eq!(changes("", ""), (0, 0));
    }

    #[test]
    fn a_line_replaced_in_place_is_one_added_and_one_removed() {
        // `Edited a.txt +1 / -1` for upstream's `alpha line 2` edit.
        assert_eq!(
            changes(
                "alpha line 1\nalpha line 2\nalpha line 3\n",
                "alpha line 1\nALPHA LINE TWO\nalpha line 3\n"
            ),
            (1, 1)
        );
    }

    #[test]
    fn lines_that_moved_are_counted_by_the_longest_common_run_not_by_position() {
        // Neither the head nor the tail is common here, so only the table can
        // see that `b` and `c` survived.
        assert_eq!(changes("a\nb\nc\n", "b\nc\nd\n"), (1, 1));
        assert_eq!(changes("a\nb\nc\nd\n", "a\nc\nb\nd\n"), (1, 1));
    }

    #[test]
    fn a_final_newline_gained_or_lost_is_one_line_of_change() {
        // `diff.zig` `appendTrailingNewlineMarker`: the marker line is the change.
        assert_eq!(changes("a\nb", "a\nb\n"), (1, 0));
        assert_eq!(changes("a\nb\n", "a\nb"), (0, 1));
        assert_eq!(changes("a\nb\n", "a\nb\n"), (0, 0));
    }

    #[test]
    fn a_change_too_large_to_tabulate_is_counted_by_its_changed_region() {
        // Past the table's budget the answer is the two sizes of what differs,
        // which no real diff can exceed -- an upper bound, never a guess.
        let before: String = (0..3000).map(|i| format!("old {i}\n")).collect();
        let after: String = (0..2000).map(|i| format!("new {i}\n")).collect();
        assert_eq!(changes(&before, &after), (2000, 3000));
    }

    #[test]
    fn a_row_names_the_argument_the_executor_acts_on_or_nothing() {
        let shown = super::super::read::READ_FILE.shown(&json!({"path": "a.txt"}));
        assert_eq!(shown.target.as_deref(), Some("a.txt"));
        assert_eq!(shown.presentation.done, "Read");
        // A field of the wrong type is a refusal the result will carry, so the
        // row falls back to the tool's own word rather than quoting the model.
        let refused = super::super::read::READ_FILE.shown(&json!({"path": 7}));
        assert_eq!(refused.target, None);
        assert_eq!(refused.presentation.default_target, "file");
    }
}
