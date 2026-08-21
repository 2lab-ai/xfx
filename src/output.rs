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

impl StatusSnapshot {
    /// Builds the snapshot from resolved configuration and build metadata.
    ///
    /// `history_turns` and `session_permission_grants` are zero because there is
    /// no durable session in this release slice; they are measured facts about
    /// the current process, not reserved fields.
    pub fn new(config: &RuntimeConfig, build: crate::BuildInfo) -> Self {
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
            history_turns: 0,
            session_permission_grants: 0,
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
pub struct TextSink<W: Write> {
    writer: W,
}

impl<W: Write> TextSink<W> {
    pub fn new(writer: W) -> Self {
        Self { writer }
    }

    pub fn into_inner(self) -> W {
        self.writer
    }
}

impl<W: Write> EventSink for TextSink<W> {
    fn emit(&mut self, event: &Event) -> io::Result<()> {
        match event {
            Event::AssistantDelta { text } => {
                self.writer.write_all(text.as_bytes())?;
                self.writer.flush()
            }
            // The final output is the concatenation of the deltas already
            // written, so repeating it would duplicate the answer.
            Event::Final { .. } | Event::ToolStart { .. } | Event::ToolResult { .. } => Ok(()),
            Event::Error { message } => {
                self.writer.write_all(b"\n")?;
                self.writer.write_all(message.as_bytes())?;
                self.writer.write_all(b"\n")?;
                self.writer.flush()
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
        let snapshot = StatusSnapshot::new(&config, crate::build_info());
        assert_eq!(snapshot.auth, MISSING_AUTH_LABEL);
        assert!(!snapshot.auth_refreshable);
        assert_eq!(snapshot.auth_help.as_deref(), Some(MISSING_AUTH_HELP));
        assert_eq!(snapshot.sandbox, SANDBOX_LABEL);
        assert_eq!(snapshot.kind, "status");
    }

    #[test]
    fn an_absent_build_revision_is_omitted_rather_than_rendered_empty() {
        let (_workspace, config) = empty_config();
        let mut snapshot = StatusSnapshot::new(&config, crate::build_info());
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

    #[test]
    fn the_text_sink_streams_only_assistant_text() {
        let mut sink = TextSink::new(Vec::new());
        sink.emit(&Event::AssistantDelta {
            text: "one ".to_string(),
        })
        .unwrap();
        sink.emit(&Event::ToolStart {
            call_id: "c1".to_string(),
            tool: "read_file".to_string(),
        })
        .unwrap();
        sink.emit(&Event::AssistantDelta {
            text: "two".to_string(),
        })
        .unwrap();
        sink.emit(&Event::Final {
            output: "one two".to_string(),
        })
        .unwrap();
        assert_eq!(String::from_utf8(sink.into_inner()).unwrap(), "one two");
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
