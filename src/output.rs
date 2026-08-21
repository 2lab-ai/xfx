//! Immutable snapshots and the renderers that turn them into bytes.
//!
//! Three output shapes exist and never mix:
//!
//! - human text, one `[surface] key=value` line per fact;
//! - a JSON document, exactly one newline-terminated line;
//! - a JSONL event stream, one newline-terminated object per event.
//!
//! Snapshots are built once from typed data and then only read. A renderer
//! cannot reach a credential: no snapshot field ever holds a secret, only the
//! name of the environment variable a secret came from.

use std::io::{self, Write};

use serde::Serialize;

use crate::config::{Credential, RuntimeConfig};
use crate::session::{SessionDetail, SessionList, TurnStep};

/// The help text shown when no credential is configured.
///
/// It names only what fxr actually supports. Upstream points at `fx login` and
/// `fx setup` (`vercel-labs/fx@580a0c5d tests/e2e/cli.test.ts:39-40`); fxr defers
/// both, so pointing at them would advertise a command that does not exist.
pub const MISSING_AUTH_HELP: &str =
    "fxr needs access to Vercel AI Gateway. Set VERCEL_OIDC_TOKEN or AI_GATEWAY_API_KEY.";

/// The label used when no credential resolved.
pub const MISSING_AUTH_LABEL: &str = "missing";

/// The sandbox fxr reports.
///
/// fxr does not confine commands in v0.1, so it reports `none`. Reporting a
/// sandbox it does not have would be the most dangerous lie in the product.
pub const SANDBOX_LABEL: &str = "none";

/// Whether the caller asked for text or JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Text,
    Json,
}

impl OutputFormat {
    /// `--json` selects JSON; its absence selects text.
    pub fn from_json_flag(json: bool) -> Self {
        if json {
            Self::Json
        } else {
            Self::Text
        }
    }
}

/// The authentication facts a snapshot may show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthSnapshot {
    /// The environment variable that supplied the credential, or `missing`.
    pub source: String,
    /// Whether the credential can be renewed without user action. fxr has no
    /// refreshable credential source yet, so this is always false.
    pub refreshable: bool,
    /// Guidance shown only when no credential resolved.
    pub help: Option<String>,
}

impl AuthSnapshot {
    /// Builds the snapshot from an optional credential, taking the source label
    /// and nothing else.
    pub fn from_credential(credential: Option<&Credential>) -> Self {
        match credential {
            Some(credential) => Self {
                source: credential.source_label().to_string(),
                refreshable: false,
                help: None,
            },
            None => Self {
                source: MISSING_AUTH_LABEL.to_string(),
                refreshable: false,
                help: Some(MISSING_AUTH_HELP.to_string()),
            },
        }
    }
}

/// What `fxr status` reports.
#[derive(Debug, Clone, Serialize)]
pub struct StatusSnapshot {
    pub kind: &'static str,
    pub model: String,
    pub build_channel: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_revision: Option<String>,
    pub auth: String,
    pub auth_refreshable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_help: Option<String>,
    pub permission_mode: String,
    pub sandbox: String,
    pub workspace: String,
    pub history_turns: u64,
    pub session_permission_grants: u64,
    pub agent_step_limit: u32,
}

/// What `status` reports about the session a turn here would continue.
///
/// Zero when there is none, which is a measured fact about the store rather
/// than a reserved field: `fxr status` in a directory that has never been asked
/// a question has nothing to continue.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SessionFacts {
    pub history_turns: u64,
    pub permission_grants: u64,
}

impl StatusSnapshot {
    /// Builds the snapshot from resolved configuration and build metadata.
    pub fn new(config: &RuntimeConfig, build: crate::BuildInfo, session: SessionFacts) -> Self {
        let auth = AuthSnapshot::from_credential(config.credential.as_ref());
        Self {
            kind: "status",
            model: config.model.clone(),
            build_channel: build.channel.to_string(),
            build_revision: build.revision.map(str::to_string),
            auth: auth.source,
            auth_refreshable: auth.refreshable,
            auth_help: auth.help,
            permission_mode: config.permission_mode.label().to_string(),
            sandbox: SANDBOX_LABEL.to_string(),
            workspace: config.workspace_root.display().to_string(),
            history_turns: session.history_turns,
            session_permission_grants: session.permission_grants,
            agent_step_limit: config.max_agent_steps,
        }
    }

    /// One `[status] key=value` line per fact, in a fixed order.
    pub fn render_text(&self) -> String {
        let mut out = String::new();
        let mut line = |key: &str, value: &str| {
            out.push_str("[status] ");
            out.push_str(key);
            out.push('=');
            out.push_str(value);
            out.push('\n');
        };
        line("model", &self.model);
        line("build_channel", &self.build_channel);
        if let Some(revision) = &self.build_revision {
            line("build_revision", revision);
        }
        line("auth", &self.auth);
        line("auth_refreshable", &self.auth_refreshable.to_string());
        if let Some(help) = &self.auth_help {
            line("auth_help", help);
        }
        line("permission_mode", &self.permission_mode);
        line("sandbox", &self.sandbox);
        line("workspace", &self.workspace);
        line("history_turns", &self.history_turns.to_string());
        line(
            "session_permission_grants",
            &self.session_permission_grants.to_string(),
        );
        line("agent_step_limit", &self.agent_step_limit.to_string());
        out
    }

    /// Exactly one newline-terminated JSON document.
    pub fn render_json(&self) -> String {
        render_json_document(self)
    }

    pub fn render(&self, format: OutputFormat) -> String {
        match format {
            OutputFormat::Text => self.render_text(),
            OutputFormat::Json => self.render_json(),
        }
    }
}

/// The outcome of one diagnostic check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Ok,
    Warn,
    Fail,
}

impl CheckStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Warn => "warn",
            Self::Fail => "fail",
        }
    }
}

/// One named diagnostic result.
#[derive(Debug, Clone, Serialize)]
pub struct DoctorCheck {
    pub name: String,
    pub status: CheckStatus,
    pub detail: String,
}

impl DoctorCheck {
    pub fn new(name: impl Into<String>, status: CheckStatus, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status,
            detail: detail.into(),
        }
    }
}

/// What `fxr doctor` reports.
#[derive(Debug, Clone, Serialize)]
pub struct DoctorSnapshot {
    pub kind: &'static str,
    pub ok_count: usize,
    pub warn_count: usize,
    pub fail_count: usize,
    pub workspace: String,
    pub model: String,
    pub auth: String,
    pub auth_refreshable: bool,
    pub permission_mode: String,
    pub agent_step_limit: u32,
    pub checks: Vec<DoctorCheck>,
}

impl DoctorSnapshot {
    /// Builds the snapshot, deriving the aggregate counts from the checks so the
    /// two can never disagree.
    pub fn new(config: &RuntimeConfig, checks: Vec<DoctorCheck>) -> Self {
        let auth = AuthSnapshot::from_credential(config.credential.as_ref());
        let count = |status: CheckStatus| checks.iter().filter(|c| c.status == status).count();
        Self {
            kind: "doctor",
            ok_count: count(CheckStatus::Ok),
            warn_count: count(CheckStatus::Warn),
            fail_count: count(CheckStatus::Fail),
            workspace: config.workspace_root.display().to_string(),
            model: config.model.clone(),
            auth: auth.source,
            auth_refreshable: auth.refreshable,
            permission_mode: config.permission_mode.label().to_string(),
            agent_step_limit: config.max_agent_steps,
            checks,
        }
    }

    /// Aggregate counts first, then the resolved facts, then one line per check.
    pub fn render_text(&self) -> String {
        let mut out = format!(
            "[doctor] ok={} warn={} fail={}\n",
            self.ok_count, self.warn_count, self.fail_count
        );
        out.push_str(&format!("[doctor] workspace={}\n", self.workspace));
        out.push_str(&format!("[doctor] model={}\n", self.model));
        out.push_str(&format!("[doctor] auth={}\n", self.auth));
        out.push_str(&format!(
            "[doctor] auth_refreshable={}\n",
            self.auth_refreshable
        ));
        out.push_str(&format!(
            "[doctor] permission_mode={}\n",
            self.permission_mode
        ));
        out.push_str(&format!(
            "[doctor] agent_step_limit={}\n",
            self.agent_step_limit
        ));
        for check in &self.checks {
            out.push_str(&format!(
                "[{}] {}: {}\n",
                check.status.label(),
                check.name,
                check.detail
            ));
        }
        out
    }

    /// Exactly one newline-terminated JSON document.
    pub fn render_json(&self) -> String {
        render_json_document(self)
    }

    pub fn render(&self, format: OutputFormat) -> String {
        match format {
            OutputFormat::Text => self.render_text(),
            OutputFormat::Json => self.render_json(),
        }
    }
}

// ---------------------------------------------------------------------------
// sessions
// ---------------------------------------------------------------------------

/// How many turns `session` shows before it says it stopped.
const MAX_DETAIL_TURNS: usize = 50;

/// How many bytes of one recorded text `session` shows.
///
/// A session detail is a summary a person reads, not an export: a turn that
/// read a 200 KiB file must not put 200 KiB on a terminal.
const MAX_DETAIL_TEXT_BYTES: usize = 2_000;

/// What `fxr sessions` reports.
///
/// # Every session-controlled value is flattened
///
/// A manifest is a file, and a file is something an attacker or a mistake can
/// write. The text renderer promises one labelled fact per line, so a workspace
/// path or a model name containing a newline would let a session forge a row --
/// a second `[session] id=...` line naming a session that does not exist, or a
/// `[turn]` line claiming an outcome nobody recorded. Every value that comes
/// from a session therefore goes through [`one_line`] on its way to text, not
/// only the ones that obviously carry prose.
#[derive(Debug, Clone, Serialize)]
pub struct SessionsSnapshot {
    pub kind: &'static str,
    /// `workspace` or `all`.
    pub scope: &'static str,
    pub count: usize,
    /// Whether the caller's limit cut the list short.
    pub has_more: bool,
    /// Whether the store holds more sessions than one scan considers, so there
    /// are rows this listing never looked at.
    pub truncated: bool,
    /// Session directories that could not be trusted and were skipped.
    pub skipped_invalid: usize,
    pub sessions: Vec<SessionRow>,
}

/// One line of a listing.
#[derive(Debug, Clone, Serialize)]
pub struct SessionRow {
    pub id: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub workspace: String,
    pub origin_workspace: String,
    pub history_turns: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

impl SessionsSnapshot {
    pub fn new(list: &SessionList) -> Self {
        Self {
            kind: "sessions",
            scope: list.scope,
            count: list.sessions.len(),
            has_more: list.has_more,
            truncated: list.truncated,
            skipped_invalid: list.skipped_invalid,
            sessions: list
                .sessions
                .iter()
                .map(|summary| SessionRow {
                    id: one_line(&summary.id),
                    created_at_ms: summary.created_at_ms,
                    updated_at_ms: summary.updated_at_ms,
                    workspace: one_line(&summary.workspace_root),
                    origin_workspace: one_line(&summary.origin_workspace_root),
                    history_turns: summary.history_turns,
                    title: summary.title.as_deref().map(one_line),
                })
                .collect(),
        }
    }

    /// A header line, then one line per session, in the same order as the JSON.
    pub fn render_text(&self) -> String {
        let mut out = format!(
            "[sessions] scope={} count={} has_more={} truncated={} skipped_invalid={}\n",
            self.scope, self.count, self.has_more, self.truncated, self.skipped_invalid
        );
        for row in &self.sessions {
            // Every field here was flattened when the row was built, so a
            // recorded newline cannot end this line early and start a forged one.
            out.push_str(&format!(
                "[session] id={} updated_at_ms={} turns={} workspace={} title={}\n",
                row.id,
                row.updated_at_ms,
                row.history_turns,
                row.workspace,
                row.title.as_deref().unwrap_or("")
            ));
        }
        out
    }

    pub fn render_json(&self) -> String {
        render_json_document(self)
    }

    pub fn render(&self, format: OutputFormat) -> String {
        match format {
            OutputFormat::Text => self.render_text(),
            OutputFormat::Json => self.render_json(),
        }
    }
}

/// One step of a recorded turn, bounded for display.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum SessionStepRow {
    Assistant {
        text: String,
        /// The tools the step asked for, by name and in order.
        tool_calls: Vec<String>,
    },
    Tool {
        call_id: String,
        tool: String,
        ok: bool,
        output: String,
    },
}

/// One recorded turn, bounded for display.
#[derive(Debug, Clone, Serialize)]
pub struct SessionTurnRow {
    pub user: String,
    pub steps: Vec<SessionStepRow>,
    /// Absent for a turn whose conclusion never reached the log, which is what
    /// a crash mid-turn looks like.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<crate::session::TurnConclusion>,
}

/// One standing approval, exactly as it will be reused.
#[derive(Debug, Clone, Serialize)]
pub struct SessionGrantRow {
    pub tool: String,
    pub target: String,
}

/// What `fxr session` reports.
#[derive(Debug, Clone, Serialize)]
pub struct SessionDetailSnapshot {
    pub kind: &'static str,
    pub id: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub workspace: String,
    pub origin_workspace: String,
    pub model: String,
    pub permission_mode: String,
    pub history_turns: u64,
    pub permission_grants: u64,
    /// Every standing approval, by tool and exact target.
    ///
    /// A count is not enough. These are approvals that outlive the process that
    /// gave them: the next `ask --resume-id` will act on them without asking
    /// again, so the only honest way to show them is the same `tool` + `target`
    /// pair the policy will match on. "3 grants" tells a user they have lost
    /// track of something without telling them what.
    pub grants: Vec<SessionGrantRow>,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    /// The project-instruction files in force at the last recorded turn. Kept
    /// as provenance only: the next turn rediscovers them.
    pub context_sources: Vec<String>,
    /// Whether older turns were left out of `turns`.
    pub truncated: bool,
    pub turns: Vec<SessionTurnRow>,
}

impl SessionDetailSnapshot {
    pub fn new(detail: &SessionDetail) -> Self {
        let state = &detail.state;
        let skipped = state.turns.len().saturating_sub(MAX_DETAIL_TURNS);
        let turns = state
            .turns
            .iter()
            .skip(skipped)
            .map(|turn| SessionTurnRow {
                user: clip_text(&turn.user),
                steps: turn
                    .steps
                    .iter()
                    .map(|step| match step {
                        TurnStep::Assistant { text, tool_calls } => SessionStepRow::Assistant {
                            text: clip_text(text),
                            // A tool name is a string out of the log, and the
                            // log is read as untrusted whatever a live provider
                            // would have produced. The text renderer joins these
                            // onto one line, so a newline in one would end that
                            // row and start a forged one.
                            tool_calls: tool_calls
                                .iter()
                                .map(|call| clip_text(&one_line(&call.name)))
                                .collect(),
                        },
                        TurnStep::ToolResult {
                            call_id,
                            tool,
                            ok,
                            output,
                        } => SessionStepRow::Tool {
                            call_id: clip_text(&one_line(call_id)),
                            tool: clip_text(&one_line(tool)),
                            ok: *ok,
                            output: clip_text(output),
                        },
                    })
                    .collect(),
                outcome: turn.outcome.as_ref().map(flatten_conclusion),
            })
            .collect();

        Self {
            kind: "session",
            id: one_line(&state.id),
            created_at_ms: state.created_at_ms,
            updated_at_ms: state.updated_at_ms,
            workspace: one_line(&state.workspace_root),
            origin_workspace: one_line(&state.origin_workspace_root),
            model: one_line(&state.model),
            permission_mode: state.permission_mode.label().to_string(),
            history_turns: state.turns.len() as u64,
            permission_grants: state.grants.len() as u64,
            grants: state
                .grants
                .iter()
                .map(|grant| SessionGrantRow {
                    tool: one_line(&grant.tool),
                    target: clip_text(&one_line(&grant.target)),
                })
                .collect(),
            total_input_tokens: state.total_input_tokens,
            total_output_tokens: state.total_output_tokens,
            context_sources: state
                .context_sources
                .iter()
                .map(|source| clip_text(&one_line(source)))
                .collect(),
            truncated: skipped > 0,
            turns,
        }
    }

    /// The session's facts, then one line per recorded step.
    pub fn render_text(&self) -> String {
        let mut out = String::new();
        let mut line = |key: &str, value: &str| {
            out.push_str("[session] ");
            out.push_str(key);
            out.push('=');
            out.push_str(value);
            out.push('\n');
        };
        line("id", &self.id);
        line("created_at_ms", &self.created_at_ms.to_string());
        line("updated_at_ms", &self.updated_at_ms.to_string());
        line("workspace", &self.workspace);
        line("origin_workspace", &self.origin_workspace);
        line("model", &self.model);
        line("permission_mode", &self.permission_mode);
        line("history_turns", &self.history_turns.to_string());
        line("permission_grants", &self.permission_grants.to_string());
        line("total_input_tokens", &self.total_input_tokens.to_string());
        line("total_output_tokens", &self.total_output_tokens.to_string());
        line("truncated", &self.truncated.to_string());
        for source in &self.context_sources {
            line("context_source", source);
        }
        // The exact standing approvals, not just how many. Each one is what a
        // later resume will act on without asking.
        for grant in &self.grants {
            out.push_str(&format!(
                "[grant] tool={} target={}\n",
                grant.tool, grant.target
            ));
        }

        for (index, turn) in self.turns.iter().enumerate() {
            out.push_str(&format!(
                "[turn] index={index} role=user text={}\n",
                one_line(&turn.user)
            ));
            for step in &turn.steps {
                match step {
                    SessionStepRow::Assistant { text, tool_calls } => out.push_str(&format!(
                        "[turn] index={index} role=assistant tools={} text={}\n",
                        tool_calls.join(","),
                        one_line(text)
                    )),
                    SessionStepRow::Tool {
                        call_id,
                        tool,
                        ok,
                        output,
                    } => out.push_str(&format!(
                        "[turn] index={index} role=tool call_id={call_id} tool={tool} ok={ok} output={}\n",
                        one_line(output)
                    )),
                }
            }
            let outcome = match &turn.outcome {
                Some(crate::session::TurnConclusion::Final {
                    finish_reason,
                    steps,
                }) => format!("final finish_reason={finish_reason} steps={steps}"),
                Some(crate::session::TurnConclusion::Interrupted { reason }) => {
                    format!("interrupted reason={}", one_line(reason))
                }
                None => "unfinished".to_string(),
            };
            out.push_str(&format!("[turn] index={index} outcome={outcome}\n"));
        }
        out
    }

    pub fn render_json(&self) -> String {
        render_json_document(self)
    }

    pub fn render(&self, format: OutputFormat) -> String {
        match format {
            OutputFormat::Text => self.render_text(),
            OutputFormat::Json => self.render_json(),
        }
    }
}

/// A recorded conclusion with every string of it made safe to render.
///
/// `finish_reason` looks like a closed vocabulary -- `stop`, `length`,
/// `tool-calls` -- and it is, coming from a live provider. It is not a closed
/// vocabulary coming off the disk, which is where this one came from. The reader
/// treats the log as untrusted input rather than as something fxr wrote, because
/// on any run where that distinction matters, fxr did not write it.
fn flatten_conclusion(outcome: &crate::session::TurnConclusion) -> crate::session::TurnConclusion {
    use crate::session::TurnConclusion;
    match outcome {
        TurnConclusion::Final {
            finish_reason,
            steps,
        } => TurnConclusion::Final {
            finish_reason: clip_text(&one_line(finish_reason)),
            steps: *steps,
        },
        TurnConclusion::Interrupted { reason } => TurnConclusion::Interrupted {
            reason: clip_text(&one_line(reason)),
        },
    }
}

/// `text` clipped to [`MAX_DETAIL_TEXT_BYTES`], on a character boundary, with a
/// sentinel that says it was clipped.
fn clip_text(text: &str) -> String {
    if text.len() <= MAX_DETAIL_TEXT_BYTES {
        return text.to_string();
    }
    let mut end = MAX_DETAIL_TEXT_BYTES;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!(
        "{}... [clipped at {MAX_DETAIL_TEXT_BYTES} bytes]",
        &text[..end]
    )
}

/// `text` with every control character turned into a space.
///
/// The text renderer promises one fact per line, and a recorded prompt is the
/// one place a newline can arrive from outside fxr.
fn one_line(text: &str) -> String {
    text.chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect()
}

/// Serializes to exactly one line and terminates it.
///
/// Every JSON surface goes through here so "one newline-terminated document" is
/// a property of the module rather than of each call site.
fn render_json_document<T: Serialize>(value: &T) -> String {
    let mut document = serde_json::to_string(value).expect("snapshots are always serializable");
    document.push('\n');
    document
}

/// A streamed turn event.
///
/// The variants are the closed set the design fixes for `ask --json`
/// (`docs/superpowers/specs/2026-08-21-fxr-rust-port-design.md`, "Output
/// contracts"). Task 1 defines and renders them; the agent loop that produces
/// them arrives with the Gateway turn.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    /// A fragment of assistant text, in arrival order.
    AssistantDelta { text: String },
    /// A tool call was admitted and is about to run.
    ToolStart { call_id: String, tool: String },
    /// A tool call finished, correlated to its start by `call_id`.
    ToolResult {
        call_id: String,
        tool: String,
        ok: bool,
        detail: String,
    },
    /// The turn completed. Emitted exactly once on a successful turn.
    Final { output: String },
    /// The turn failed. Emitted exactly once instead of `Final`.
    Error { message: String },
}

/// Where turn events go.
///
/// The trait exists so the agent loop can be driven by a deterministic recorder
/// in tests and by a real stream in the binary, without either knowing the other.
pub trait EventSink {
    /// Writes one event. Implementations flush whatever a consumer needs to see
    /// the event now, because a stream that arrives after the turn is useless.
    fn emit(&mut self, event: &Event) -> io::Result<()>;
}

/// Emits one newline-terminated JSON object per event.
pub struct JsonlSink<W: Write> {
    writer: W,
}

impl<W: Write> JsonlSink<W> {
    pub fn new(writer: W) -> Self {
        Self { writer }
    }

    pub fn into_inner(self) -> W {
        self.writer
    }
}

impl<W: Write> EventSink for JsonlSink<W> {
    fn emit(&mut self, event: &Event) -> io::Result<()> {
        let line = serde_json::to_string(event).expect("events are always serializable");
        self.writer.write_all(line.as_bytes())?;
        self.writer.write_all(b"\n")?;
        self.writer.flush()
    }
}

/// Streams assistant text for a human reader and drops the machine-only events.
///
/// Two writers, not one. Assistant text is the command's answer and belongs on
/// stdout; a failure is a diagnostic and belongs on stderr, so a shell pipeline
/// that captures the answer never captures an error message as part of it.
pub struct TextSink<W: Write, D: Write> {
    writer: W,
    diagnostics: D,
    /// Whether any assistant text has been written, and whether it ended in a
    /// newline. A streamed answer rarely ends in one, and a shell prompt landing
    /// mid-sentence looks like truncated output.
    pending_newline: bool,
    /// Whether tool activity is announced on the diagnostic stream.
    tool_notices: bool,
}

/// How much of a failed tool's report is echoed beside its notice.
///
/// The whole report goes to the model, which is where it is useful. A watching
/// human needs the first line and a bound, so that a tool refusing to read a
/// 200 KiB file does not repaint the terminal to say so.
const MAX_TOOL_NOTICE_DETAIL: usize = 120;

impl<W: Write, D: Write> TextSink<W, D> {
    pub fn new(writer: W, diagnostics: D) -> Self {
        Self {
            writer,
            diagnostics,
            pending_newline: false,
            tool_notices: false,
        }
    }

    /// The same sink, announcing each tool call as it starts and finishes.
    ///
    /// Off by default, because the output of `fxr ask` is its answer and
    /// nothing else. On in the interactive shell, where the alternative is a
    /// terminal that sits silent for a minute while the model reads files, and
    /// a user who cannot tell "working" from "hung".
    pub fn with_tool_notices(mut self) -> Self {
        self.tool_notices = true;
        self
    }

    pub fn into_inner(self) -> (W, D) {
        (self.writer, self.diagnostics)
    }

    /// Terminates the assistant line, once, if it needs it.
    fn end_line(&mut self) -> io::Result<()> {
        if !self.pending_newline {
            return Ok(());
        }
        self.pending_newline = false;
        self.writer.write_all(b"\n")?;
        self.writer.flush()
    }

    /// Writes one `[tool] ...` line on the diagnostic stream.
    fn notice(&mut self, line: &str) -> io::Result<()> {
        self.end_line()?;
        self.diagnostics.write_all(line.as_bytes())?;
        self.diagnostics.write_all(b"\n")?;
        self.diagnostics.flush()
    }
}

/// `detail` as one bounded line that cannot forge another notice.
///
/// Tool output is model-facing text that came from a file, a command, or a
/// refusal, so it is arbitrary. It goes through the same flattening a recorded
/// prompt does -- a newline in it must not be able to write a second `[tool]`
/// line on the user's terminal -- and is then clipped, because the full report
/// is for the model and not for the scrollback.
fn bounded_notice_detail(detail: &str) -> String {
    let flattened = one_line(detail);
    let trimmed = flattened.trim();
    let mut out = String::new();
    for character in trimmed.chars() {
        if out.len() + character.len_utf8() > MAX_TOOL_NOTICE_DETAIL {
            out.push('…');
            break;
        }
        out.push(character);
    }
    out
}

impl<W: Write, D: Write> EventSink for TextSink<W, D> {
    fn emit(&mut self, event: &Event) -> io::Result<()> {
        match event {
            Event::AssistantDelta { text } => {
                if text.is_empty() {
                    return Ok(());
                }
                self.writer.write_all(text.as_bytes())?;
                self.pending_newline = !text.ends_with('\n');
                self.writer.flush()
            }
            // The final output is the concatenation of the deltas already
            // written, so repeating it would duplicate the answer. Upstream
            // likewise emits only a closing newline
            // (`vercel-labs/fx@580a0c5d src/core/agent/runtime/orchestrator.zig:4654`).
            Event::Final { .. } => self.end_line(),
            Event::ToolStart { tool, .. } => {
                if !self.tool_notices {
                    return Ok(());
                }
                self.notice(&format!("[tool] {tool} running"))
            }
            Event::ToolResult {
                tool, ok, detail, ..
            } => {
                if !self.tool_notices {
                    return Ok(());
                }
                if *ok {
                    self.notice(&format!("[tool] {tool} ok"))
                } else {
                    self.notice(&format!(
                        "[tool] {tool} refused: {}",
                        bounded_notice_detail(detail)
                    ))
                }
            }
            Event::Error { message } => {
                self.end_line()?;
                self.diagnostics.write_all(message.as_bytes())?;
                self.diagnostics.write_all(b"\n")?;
                self.diagnostics.flush()
            }
        }
    }
}

/// Records events in order. Deterministic tests assert against this instead of
/// parsing bytes.
#[derive(Debug, Default)]
pub struct RecordingSink {
    events: Vec<Event>,
}

impl RecordingSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn events(&self) -> &[Event] {
        &self.events
    }
}

impl EventSink for RecordingSink {
    fn emit(&mut self, event: &Event) -> io::Result<()> {
        self.events.push(event.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jsonl(events: &[Event]) -> String {
        let mut sink = JsonlSink::new(Vec::new());
        for event in events {
            sink.emit(event).unwrap();
        }
        String::from_utf8(sink.into_inner()).unwrap()
    }

    #[test]
    fn missing_credentials_produce_the_missing_label_and_help() {
        let auth = AuthSnapshot::from_credential(None);
        assert_eq!(auth.source, MISSING_AUTH_LABEL);
        assert!(!auth.refreshable);
        assert_eq!(auth.help.as_deref(), Some(MISSING_AUTH_HELP));
    }

    #[test]
    fn the_missing_auth_help_names_only_supported_credentials() {
        assert!(MISSING_AUTH_HELP.contains("VERCEL_OIDC_TOKEN"));
        assert!(MISSING_AUTH_HELP.contains("AI_GATEWAY_API_KEY"));
        for deferred in ["login", "setup", "logout", "models", "provider"] {
            assert!(
                !MISSING_AUTH_HELP.contains(deferred),
                "help must not point at the deferred `{deferred}` command"
            );
        }
    }

    /// A configuration for an empty temporary workspace with no environment.
    fn empty_config() -> (tempfile::TempDir, RuntimeConfig) {
        let workspace = tempfile::tempdir().expect("temp workspace");
        let config = RuntimeConfig::load_with(
            &crate::config::Environment::new(None, std::collections::BTreeMap::new()),
            workspace.path(),
        )
        .expect("load config");
        (workspace, config)
    }

    #[test]
    fn doctor_counts_are_derived_from_the_checks() {
        let (_workspace, config) = empty_config();
        let snapshot = DoctorSnapshot::new(
            &config,
            vec![
                DoctorCheck::new("a", CheckStatus::Ok, "fine"),
                DoctorCheck::new("b", CheckStatus::Warn, "hmm"),
                DoctorCheck::new("c", CheckStatus::Warn, "hmm"),
                DoctorCheck::new("d", CheckStatus::Fail, "no"),
            ],
        );
        assert_eq!(snapshot.ok_count, 1);
        assert_eq!(snapshot.warn_count, 2);
        assert_eq!(snapshot.fail_count, 1);
        assert_eq!(snapshot.checks.len(), 4);
        assert!(
            snapshot
                .render_text()
                .starts_with("[doctor] ok=1 warn=2 fail=1\n"),
            "{}",
            snapshot.render_text()
        );
    }

    #[test]
    fn a_status_snapshot_without_credentials_reports_missing_and_no_secret_field() {
        let (_workspace, config) = empty_config();
        let snapshot = StatusSnapshot::new(&config, crate::build_info(), SessionFacts::default());
        assert_eq!(snapshot.auth, MISSING_AUTH_LABEL);
        assert!(!snapshot.auth_refreshable);
        assert_eq!(snapshot.auth_help.as_deref(), Some(MISSING_AUTH_HELP));
        assert_eq!(snapshot.sandbox, SANDBOX_LABEL);
        assert_eq!(snapshot.kind, "status");
    }

    #[test]
    fn status_reports_the_session_facts_it_was_given_rather_than_a_fixed_zero() {
        let (_workspace, config) = empty_config();
        let snapshot = StatusSnapshot::new(
            &config,
            crate::build_info(),
            SessionFacts {
                history_turns: 4,
                permission_grants: 2,
            },
        );
        assert_eq!(snapshot.history_turns, 4);
        assert_eq!(snapshot.session_permission_grants, 2);
        assert!(snapshot
            .render_text()
            .contains("[status] history_turns=4\n"));
        assert!(snapshot
            .render_text()
            .contains("[status] session_permission_grants=2\n"));
    }

    // -- sessions ----------------------------------------------------------

    fn turn(user: &str, steps: Vec<crate::session::TurnStep>) -> crate::session::HistoryTurn {
        crate::session::HistoryTurn {
            user: user.to_string(),
            steps,
            outcome: Some(crate::session::TurnConclusion::Final {
                finish_reason: "stop".to_string(),
                steps: 1,
            }),
        }
    }

    fn state(turns: Vec<crate::session::HistoryTurn>) -> crate::session::DurableState {
        crate::session::DurableState {
            id: "s1".to_string(),
            created_at_ms: 10,
            updated_at_ms: 20,
            origin_workspace_root: "/w".to_string(),
            workspace_root: "/w".to_string(),
            model: "m".to_string(),
            permission_mode: crate::config::PermissionMode::Auto,
            last_event_seq: 1,
            total_input_tokens: 3,
            total_output_tokens: 4,
            grants: Vec::new(),
            context_sources: vec!["/w/AGENTS.md".to_string()],
            turns,
        }
    }

    fn detail_of(state: crate::session::DurableState) -> SessionDetail {
        SessionDetail {
            summary: crate::session::SessionSummary {
                id: state.id.clone(),
                created_at_ms: state.created_at_ms,
                updated_at_ms: state.updated_at_ms,
                workspace_root: state.workspace_root.clone(),
                origin_workspace_root: state.origin_workspace_root.clone(),
                history_turns: state.turns.len() as u64,
                title: state.title(),
            },
            manifest: crate::session::SessionManifest {
                schema_version: crate::session::MANIFEST_SCHEMA_VERSION,
                storage_format: crate::session::STORAGE_FORMAT.to_string(),
                id: state.id.clone(),
                log_generation: "a".repeat(32),
                created_at_ms: state.created_at_ms,
                updated_at_ms: state.updated_at_ms,
                origin_workspace_root: state.origin_workspace_root.clone(),
                workspace_root: state.workspace_root.clone(),
                model: state.model.clone(),
                permission_mode: state.permission_mode.label().to_string(),
                title: state.title(),
                history_turns: state.turns.len() as u64,
                permission_grants: 0,
                total_input_tokens: state.total_input_tokens,
                total_output_tokens: state.total_output_tokens,
                last_event_seq: state.last_event_seq,
                event_log_bytes: 1,
                event_log_sha256: "0".repeat(64),
            },
            state,
        }
    }

    #[test]
    fn a_session_detail_clips_a_long_recorded_text_and_says_that_it_did() {
        let long = "x".repeat(MAX_DETAIL_TEXT_BYTES * 2);
        let snapshot = SessionDetailSnapshot::new(&detail_of(state(vec![turn(
            &long,
            vec![crate::session::TurnStep::ToolResult {
                call_id: "c1".to_string(),
                tool: "read_file".to_string(),
                ok: true,
                output: long.clone(),
            }],
        )])));
        assert!(snapshot.turns[0].user.ends_with("bytes]"), "the user text");
        assert!(
            snapshot.turns[0].user.len() < long.len(),
            "a recorded prompt is bounded"
        );
        let json = snapshot.render_json();
        assert_eq!(json.matches('\n').count(), 1, "one document");
        assert!(json.contains("clipped at"), "{json}");
    }

    #[test]
    fn a_session_detail_shows_the_most_recent_turns_and_reports_the_ones_it_dropped() {
        let turns: Vec<crate::session::HistoryTurn> = (0..MAX_DETAIL_TURNS + 5)
            .map(|index| turn(&format!("turn {index}"), Vec::new()))
            .collect();
        let snapshot = SessionDetailSnapshot::new(&detail_of(state(turns)));
        assert_eq!(snapshot.history_turns as usize, MAX_DETAIL_TURNS + 5);
        assert_eq!(snapshot.turns.len(), MAX_DETAIL_TURNS);
        assert!(snapshot.truncated);
        assert_eq!(snapshot.turns[0].user, "turn 5", "the newest are kept");
        assert!(snapshot
            .render_text()
            .contains("[session] truncated=true\n"));
    }

    #[test]
    fn a_recorded_newline_cannot_break_the_one_fact_per_line_contract() {
        let snapshot =
            SessionDetailSnapshot::new(&detail_of(state(vec![turn("first\nsecond", Vec::new())])));
        let text = snapshot.render_text();
        assert!(
            text.contains("[turn] index=0 role=user text=first second\n"),
            "{text}"
        );
        // The real property: every line is a labelled fact, so a recorded
        // newline cannot produce a line a parser would not recognize.
        for line in text.lines() {
            assert!(
                line.starts_with("[session] ") || line.starts_with("[turn] "),
                "unlabelled line {line:?} in {text}"
            );
        }
    }

    #[test]
    fn a_listing_renders_the_same_facts_as_text_and_as_one_document() {
        let list = SessionList {
            scope: "workspace",
            sessions: vec![crate::session::SessionSummary {
                id: "s1".to_string(),
                created_at_ms: 10,
                updated_at_ms: 20,
                workspace_root: "/w".to_string(),
                origin_workspace_root: "/w".to_string(),
                history_turns: 2,
                title: Some("a question".to_string()),
            }],
            has_more: true,
            truncated: false,
            skipped_invalid: 1,
        };
        let snapshot = SessionsSnapshot::new(&list);
        let text = snapshot.render_text();
        assert!(
            text.starts_with(
                "[sessions] scope=workspace count=1 has_more=true truncated=false skipped_invalid=1\n"
            ),
            "{text}"
        );
        assert!(
            text.contains(
                "[session] id=s1 updated_at_ms=20 turns=2 workspace=/w title=a question\n"
            ),
            "{text}"
        );
        let json = snapshot.render_json();
        assert_eq!(json.matches('\n').count(), 1);
        let parsed: serde_json::Value = serde_json::from_str(json.trim_end()).unwrap();
        assert_eq!(parsed["kind"], "sessions");
        assert_eq!(parsed["skipped_invalid"], 1);
        assert_eq!(parsed["sessions"][0]["title"], "a question");
    }

    #[test]
    fn an_untitled_session_omits_the_field_rather_than_inventing_one() {
        let list = SessionList {
            scope: "all",
            sessions: vec![crate::session::SessionSummary {
                id: "s1".to_string(),
                created_at_ms: 10,
                updated_at_ms: 20,
                workspace_root: "/w".to_string(),
                origin_workspace_root: "/w".to_string(),
                history_turns: 0,
                title: None,
            }],
            has_more: false,
            truncated: false,
            skipped_invalid: 0,
        };
        let json = SessionsSnapshot::new(&list).render_json();
        assert!(!json.contains("title"), "{json}");
    }

    #[test]
    fn clipping_never_splits_a_character() {
        let text = "한".repeat(MAX_DETAIL_TEXT_BYTES);
        let clipped = clip_text(&text);
        assert!(clipped.starts_with('한'));
        assert!(clipped.ends_with("bytes]"));
        assert_eq!(clip_text("short"), "short");
    }

    #[test]
    fn an_absent_build_revision_is_omitted_rather_than_rendered_empty() {
        let (_workspace, config) = empty_config();
        let mut snapshot =
            StatusSnapshot::new(&config, crate::build_info(), SessionFacts::default());
        snapshot.build_revision = None;
        assert!(!snapshot.render_json().contains("build_revision"));
        assert!(!snapshot.render_text().contains("build_revision"));

        snapshot.build_revision = Some("0123456789ab".to_string());
        assert!(snapshot
            .render_json()
            .contains("\"build_revision\":\"0123456789ab\""));
        assert!(snapshot
            .render_text()
            .contains("[status] build_revision=0123456789ab\n"));
    }

    #[test]
    fn a_json_document_is_exactly_one_terminated_line() {
        #[derive(Serialize)]
        struct Sample {
            value: &'static str,
        }
        let rendered = render_json_document(&Sample {
            value: "with \"quotes\" and a \n newline",
        });
        assert!(rendered.ends_with('\n'));
        assert_eq!(rendered.matches('\n').count(), 1);
        serde_json::from_str::<serde_json::Value>(rendered.trim_end()).unwrap();
    }

    #[test]
    fn jsonl_emits_one_tagged_object_per_event() {
        let rendered = jsonl(&[
            Event::AssistantDelta {
                text: "hi".to_string(),
            },
            Event::ToolStart {
                call_id: "c1".to_string(),
                tool: "read_file".to_string(),
            },
            Event::ToolResult {
                call_id: "c1".to_string(),
                tool: "read_file".to_string(),
                ok: true,
                detail: "12 lines".to_string(),
            },
            Event::Final {
                output: "hi".to_string(),
            },
        ]);
        let lines: Vec<&str> = rendered.lines().collect();
        assert_eq!(lines.len(), 4);
        let kinds: Vec<String> = lines
            .iter()
            .map(|line| {
                serde_json::from_str::<serde_json::Value>(line).unwrap()["kind"]
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect();
        assert_eq!(
            kinds,
            ["assistant_delta", "tool_start", "tool_result", "final"]
        );
        assert!(rendered.ends_with('\n'));
    }

    #[test]
    fn jsonl_escapes_a_newline_inside_an_event_rather_than_splitting_it() {
        let rendered = jsonl(&[Event::AssistantDelta {
            text: "one\ntwo".to_string(),
        }]);
        assert_eq!(rendered.lines().count(), 1, "got {rendered:?}");
    }

    fn text_sink_output(events: &[Event]) -> (String, String) {
        let mut sink = TextSink::new(Vec::new(), Vec::new());
        for event in events {
            sink.emit(event).unwrap();
        }
        let (stdout, stderr) = sink.into_inner();
        (
            String::from_utf8(stdout).unwrap(),
            String::from_utf8(stderr).unwrap(),
        )
    }

    #[test]
    fn the_text_sink_streams_only_assistant_text() {
        let (stdout, stderr) = text_sink_output(&[
            Event::AssistantDelta {
                text: "one ".to_string(),
            },
            Event::ToolStart {
                call_id: "c1".to_string(),
                tool: "read_file".to_string(),
            },
            Event::AssistantDelta {
                text: "two".to_string(),
            },
            Event::Final {
                output: "one two".to_string(),
            },
        ]);
        assert_eq!(stdout, "one two\n");
        assert_eq!(stderr, "");
    }

    fn noticing_sink_output(events: &[Event]) -> (String, String) {
        let mut sink = TextSink::new(Vec::new(), Vec::new()).with_tool_notices();
        for event in events {
            sink.emit(event).unwrap();
        }
        let (stdout, stderr) = sink.into_inner();
        (
            String::from_utf8(stdout).unwrap(),
            String::from_utf8(stderr).unwrap(),
        )
    }

    #[test]
    fn a_noticing_sink_announces_each_tool_without_touching_the_answer() {
        let (stdout, stderr) = noticing_sink_output(&[
            Event::AssistantDelta {
                text: "reading".to_string(),
            },
            Event::ToolStart {
                call_id: "c1".to_string(),
                tool: "read_file".to_string(),
            },
            Event::ToolResult {
                call_id: "c1".to_string(),
                tool: "read_file".to_string(),
                ok: true,
                detail: "12 lines".to_string(),
            },
            Event::AssistantDelta {
                text: " done".to_string(),
            },
            Event::Final {
                output: "reading done".to_string(),
            },
        ]);
        // The answer is unchanged except for the newline the notice forced, so
        // a notice never lands in the middle of a sentence.
        assert_eq!(stdout, "reading\n done\n");
        assert_eq!(stderr, "[tool] read_file running\n[tool] read_file ok\n");
    }

    #[test]
    fn a_notice_cannot_be_forged_by_a_tool_that_refuses_with_a_newline() {
        let (_, stderr) = noticing_sink_output(&[Event::ToolResult {
            call_id: "c1".to_string(),
            tool: "terminal".to_string(),
            ok: false,
            detail: "denied\n[tool] terminal ok".to_string(),
        }]);
        assert_eq!(
            stderr,
            "[tool] terminal refused: denied [tool] terminal ok\n"
        );
        assert_eq!(stderr.lines().count(), 1);
    }

    #[test]
    fn a_notice_bounds_what_it_quotes_back() {
        let detail = "x".repeat(10_000);
        let (_, stderr) = noticing_sink_output(&[Event::ToolResult {
            call_id: "c1".to_string(),
            tool: "grep_files".to_string(),
            ok: false,
            detail,
        }]);
        assert!(
            stderr.len() < MAX_TOOL_NOTICE_DETAIL + 64,
            "{}",
            stderr.len()
        );
        assert!(stderr.ends_with("…\n"), "{stderr}");
    }

    #[test]
    fn a_plain_text_sink_still_says_nothing_about_tools() {
        let (stdout, stderr) = text_sink_output(&[
            Event::ToolStart {
                call_id: "c1".to_string(),
                tool: "read_file".to_string(),
            },
            Event::ToolResult {
                call_id: "c1".to_string(),
                tool: "read_file".to_string(),
                ok: false,
                detail: "no such file".to_string(),
            },
        ]);
        assert_eq!(stdout, "");
        assert_eq!(stderr, "");
    }

    #[test]
    fn the_text_sink_does_not_double_space_an_answer_that_ends_in_a_newline() {
        let (stdout, _) = text_sink_output(&[
            Event::AssistantDelta {
                text: "done\n".to_string(),
            },
            Event::Final {
                output: "done\n".to_string(),
            },
        ]);
        assert_eq!(stdout, "done\n");
    }

    #[test]
    fn the_text_sink_writes_a_failure_to_the_diagnostic_stream() {
        let (stdout, stderr) = text_sink_output(&[
            Event::AssistantDelta {
                text: "half".to_string(),
            },
            Event::Error {
                message: "boom".to_string(),
            },
        ]);
        assert_eq!(stdout, "half\n", "the answer stream keeps only the answer");
        assert_eq!(stderr, "boom\n");
    }

    #[test]
    fn a_failure_before_any_output_writes_no_stray_newline() {
        let (stdout, stderr) = text_sink_output(&[Event::Error {
            message: "boom".to_string(),
        }]);
        assert_eq!(stdout, "");
        assert_eq!(stderr, "boom\n");
    }

    #[test]
    fn the_recording_sink_preserves_event_order() {
        let mut sink = RecordingSink::new();
        sink.emit(&Event::AssistantDelta {
            text: "a".to_string(),
        })
        .unwrap();
        sink.emit(&Event::Error {
            message: "boom".to_string(),
        })
        .unwrap();
        assert_eq!(sink.events().len(), 2);
        assert!(matches!(sink.events()[1], Event::Error { .. }));
    }

    #[test]
    fn output_format_follows_the_json_flag() {
        assert_eq!(OutputFormat::from_json_flag(true), OutputFormat::Json);
        assert_eq!(OutputFormat::from_json_flag(false), OutputFormat::Text);
    }
}
