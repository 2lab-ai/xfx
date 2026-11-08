//! The full transcript: the screen `C-o` opens over the conversation
//! (`vercel-labs/fx@c1db9191 src/ui/full_transcript_screen.zig`; its keys,
//! `src/core/app/input_full_transcript_runtime.zig:147-198`).
//!
//! The inline transcript is the conversation as it is read; this is the same
//! conversation as it was **recorded**: every entry under a dim timestamp
//! header in the local time zone (`full_transcript_metadata.zig:28-38@481104fd`,
//! [`stamp`]), and under each tool call what
//! it was called with and what it returned -- a command's output, a file's
//! lines, the change it was asked to make -- which the inline group never shows
//! (`tool_group_projection.zig:1150-1158`; upstream's e2e
//! `tui-command-permissions.test.ts:1118`, "keeps command output exclusive to
//! Ctrl-O").
//!
//! ```text
//!   2026-10-06 23:43:50.639 KST · Tool
//! ├ Exited 3 echo boom; echo err >&2; exit 3
//! │ boom
//! │ err
//! │ exit code 3
//!
//! ┃ full detail · ctrl+o close · pgup/pgdn scroll · esc close
//!
//! auto · model-a
//! ```
//!
//! # Where the record comes from
//!
//! **What the document was given, as it was given it**, and nothing parsed back
//! out of the screen. The shell tells this module about each thing it writes at
//! the moment it writes it -- the card, the answer as the markdown presenter
//! completes it, a notice, a tool call's row, a question's resolution, the
//! turn's summary -- so the entries are in
//! the document's own order and say what the document says. What the document
//! is never given -- a call's arguments and its whole result -- comes from the
//! session's journal: [`Teed`] stands between the turn and the recorder on the
//! runtime thread and hands each of those to the UI as a [`Detail`] as it is
//! written down. The UI thread never waits on the runtime for any of it, and
//! never reads the log back. A change's diff comes the same way
//! (`TurnJournal::change_made`), for every change, reviewed or not.
//!
//! Upstream's full-detail records -- `* session:` for a fresh launch and
//! `* network:` for each provider request a turn settles -- are shown here and
//! never in the document, above every entry, as upstream's are
//! ([`session_record`], [`Networked`]). A resumed session is recorded by the
//! writes that replay it, its calls' details read off the log
//! ([`details_of`]) and its entries stamped with the log's times
//! ([`FullTranscript::stamp_with`]).
//!
//! # The screen
//!
//! The terminal's alternate buffer, through the machinery the approval review
//! uses (`super::frame`'s `enter_alternate`, `repaint_alternate`,
//! `restore_primary`), so the primary document is exactly as it was when the
//! screen closes. Rows above a footer of four: a blank row, `┃ full detail ·
//! …`, a blank row and the hint row (`paint_plan.zig:993-1048`). It opens at
//! the tail and follows it while it is there. While it is not, it keeps the
//! entry it shows, and how far into it, across a page that rebuilds or a
//! width that changes ([`position`], v0.0.16's 026a8f4e and b1a53b01).
//!
//! A tool group's header and each call's row wrap under their tree rail
//! rather than being cut at the screen's edge ([`tool_status`], v0.0.16's
//! 427d481f).
//!
//! # What differs from upstream's screen, and why
//!
//! * Upstream's `* recovery:` record (a retry it scheduled,
//!   `app_worker_runtime.zig:95-133`) has no counterpart: xfx's turn retries
//!   without telling anything (`agent::machine`'s `wait_for_retry`).
//! * The `* session:` record has no `system prompt:` or `skills:` line
//!   ([`session_record`]): xfx assembles no system prompt at launch and has no
//!   skill catalog.
//! * A change made before a resume has no diff: the log keeps what the model
//!   was told, and the diff is handed to the screen alone
//!   (`TurnJournal::change_made`), where upstream persists its file
//!   presentation (`writeCommittedFilePresentation`,
//!   `app_session_runtime.zig:4978`).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;
use tokio::sync::mpsc::Sender;
use tokio_util::sync::CancellationToken;

use crate::agent::TurnJournal;
use crate::gateway::protocol::{Completion, CompletionRequest, FinishReason};
use crate::gateway::{DeltaSink, Provider, ProviderError};
use crate::markdown;
use crate::permission::ApprovalDiff;
use crate::session::{SessionEvent, TurnStep};
use crate::tools::{Activity, LineChanges, Outcome, Registry, Shown};

use super::approval_view::{Line as ReviewLine, Review};
use super::blocks;
use super::bridge::{self, UiEvent};
use super::frame::clip;
use super::input::Action;
use super::notice::{Notice, Tone};
use super::theme::{DocumentRow, Mode, Palette};
use super::toolgroup::ToolGroup;
use super::transcript::{ellipsized, recoloured, Gutter, Landed, Role, Transcript};
use super::wrap;

mod position;
mod tool_status;

use position::{ItemRow, Reading, RECORD};

/// The footer's one line of text (`paint_plan.zig:1003`).
pub(crate) const FOOTER: &str =
    "full detail \u{b7} ctrl+o close \u{b7} pgup/pgdn scroll \u{b7} esc close";

/// The welcome line's id on a page ([`FullTranscript::measure`]): after the
/// records' [`RECORD`] and before every entry's.
const WELCOME: usize = 1;

/// The document's first entry's id on a page.
const FIRST_ENTRY: usize = 2;

/// How many rows an arrow key moves the screen
/// (`transcript/runtime.zig:6876`, `full_transcript_wheel_rows`).
const ARROW_ROWS: usize = 3;

/// How much of one call's result this screen keeps.
///
/// The session's log keeps all of it (a tool result is capped at 256 KiB by
/// the registry); this is the UI thread's copy, which it wraps again on every
/// resize. Upstream pages a large result out of its result store instead; a
/// screen that shows the head of one and says how much more there is says the
/// same thing without a second store.
const RESULT_BYTES: usize = 64 * 1024;

/// What a header names an entry as (`full_transcript_metadata.zig:3-25`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Prompt,
    Response,
    Tool,
    Notice,
    Error,
    Usage,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Kind::Prompt => "Prompt",
            Kind::Response => "Response",
            Kind::Tool => "Tool",
            Kind::Notice => "Notice",
            Kind::Error => "Error",
            Kind::Usage => "Usage",
        }
    }
}

/// `YYYY-MM-DD HH:MM:SS.mmm <zone>`, read in the local zone in effect at that
/// moment -- `KST`, or `+0545` for a zone without a plain abbreviation
/// ([`crate::local_time`], v0.0.16's c9bc4876) -- or nothing for a moment
/// upstream will not print either (`formatHeader`,
/// `full_transcript_metadata.zig:28-38@481104fd`: zero, negative, or past the
/// year 9999).
pub(crate) fn stamp(at_ms: i64) -> Option<String> {
    if at_ms <= 0 {
        return None;
    }
    crate::local_time::local(at_ms).map(|reading| reading.to_string())
}

// ---------------------------------------------------------------------------
// The full-detail records
// ---------------------------------------------------------------------------

/// How many tool names the session record lists before `+N more`
/// (`session_tool_name_preview_max`, `app_bootstrap_runtime.zig:57`).
const SESSION_TOOLS_SHOWN: usize = 8;

/// The record of the session a fresh launch assembled
/// (`writeSessionAssemblyBody`, `app_bootstrap_runtime.zig:61-111`).
///
/// Only the lines xfx has a fact for. `system prompt:` is upstream's own
/// prompt, which it omits when there is none, and xfx has none: its one
/// system message is the workspace's `AGENTS.md`, read at each turn
/// (`agent::machine`'s `with_context`), not assembled at launch. `skills:`
/// counts upstream's skill catalog, and xfx has no catalog to count.
/// `effort: auto` is upstream's word for an effort nobody chose -- what it
/// writes for an app with no effort of its own (`:378-381`) -- and xfx sends
/// none. `mcp:` names the configured servers as upstream's does, each with
/// its connection and, once it is ready, its tools, the first ten and a count
/// of the rest (`:92-111`), or `none`.
pub(crate) fn session_record(
    provider: &str,
    model: &str,
    tools: &[&str],
    mcp: &[crate::mcp::runtime::ServerView],
) -> Notice {
    let mut body = format!("provider: {provider} \u{b7} model: {model} \u{b7} effort: auto\n");
    body.push_str(&format!("tools: {} advertised", tools.len()));
    if !tools.is_empty() {
        let shown = tools.len().min(SESSION_TOOLS_SHOWN);
        body.push_str(&format!(" ({}", tools[..shown].join(", ")));
        if tools.len() > shown {
            body.push_str(&format!(", +{} more", tools.len() - shown));
        }
        body.push(')');
    }
    body.push('\n');
    body.push_str(&mcp_record(mcp));
    Notice::neutral("session", body)
}

/// The session record's `mcp:` line (`app_bootstrap_runtime.zig:92-111`).
fn mcp_record(servers: &[crate::mcp::runtime::ServerView]) -> String {
    use crate::mcp::runtime::Connection;
    const SHOWN: usize = 10;
    if servers.is_empty() {
        return "mcp: none".to_string();
    }
    let mut line = format!(
        "mcp: {} server{}: ",
        servers.len(),
        if servers.len() == 1 { "" } else { "s" }
    );
    let named: Vec<String> = servers
        .iter()
        .take(SHOWN)
        .map(|server| {
            let connection = match server.connection {
                Connection::Disconnected => "disconnected",
                Connection::Disabled => "disabled",
                Connection::Connecting => "connecting",
                Connection::Retrying => "retrying",
                Connection::Ready => "ready",
                Connection::Failed => "failed",
            };
            match server.tool_count {
                Some(tools) => format!("{} ({connection}, {tools} tools)", server.name),
                None => format!("{} ({connection})", server.name),
            }
        })
        .collect();
    line.push_str(&named.join(", "));
    if servers.len() > SHOWN {
        line.push_str(&format!(", +{} more", servers.len() - SHOWN));
    }
    line
}

/// The record of one settled provider request (`writeNetworkRecordBody` and
/// `pushNetworkRecord`, `orchestrator.zig:4194-4280`): neutral for one that
/// completed, a warning for one the provider refused.
///
/// `None` for an attempt that ended any other way -- a connection that never
/// opened, a broken stream, a cancellation -- because upstream's stream call
/// returns those as errors before the record is pushed (`:7966`, `:8000`).
/// A completion's `generation:` and `served-by:` are left out because xfx's
/// completion carries neither, as upstream leaves them out when its does not.
pub(crate) fn network_record(
    provider: &str,
    model: &str,
    elapsed_ms: u64,
    settled: &Result<Completion, ProviderError>,
) -> Option<Notice> {
    let head = format!("provider: {provider} \u{b7} model: {model} \u{b7} {elapsed_ms}ms\n");
    match settled {
        Ok(completion) => {
            let reason = match completion.finish_reason {
                FinishReason::Stop => "stop",
                FinishReason::Length => "length",
                FinishReason::ContentFilter => "content_filter",
                FinishReason::ToolCalls => "tool_calls",
                FinishReason::ProviderError => "provider_error",
                FinishReason::Other => "other",
            };
            let mut body = format!("{head}finish: {reason}");
            // Only what the provider reported, as upstream writes it
            // (`writeUsageTokens`): zero included, an absent count left out.
            let counts: Vec<String> = [
                ("in", completion.usage.input_tokens),
                ("out", completion.usage.output_tokens),
            ]
            .into_iter()
            .filter_map(|(name, count)| count.map(|count| format!("{count} {name}")))
            .collect();
            if !counts.is_empty() {
                body.push_str(&format!("\ntokens: {}", counts.join(" \u{b7} ")));
            }
            Some(Notice::neutral("network", body))
        }
        Err(ProviderError::Status {
            status,
            retry_after,
            ..
        }) => {
            // Upstream's names for a refusing status (`failureKind`,
            // `host_stream_provider.zig:435-447`).
            let kind = match status {
                400 => "invalid_request",
                401 => "unauthorized",
                403 => "forbidden",
                413 => "request_too_large",
                429 => "rate_limited",
                500 => "server_error",
                502 => "bad_gateway",
                503 => "unavailable",
                504 => "gateway_timeout",
                _ => "provider_error",
            };
            let mut body = format!("{head}failed: {kind}");
            if let Some(after) = retry_after {
                body.push_str(&format!(" \u{b7} retry after: {}s", after.as_secs()));
            }
            Some(Notice::warning("network", body))
        }
        Err(_) => None,
    }
}

/// The provider a turn streams from, with a [`network_record`] handed to the
/// UI for every attempt it settles -- one call is one attempt
/// (`crate::gateway::Provider`), as upstream pushes one per stream it settles.
pub(crate) struct Networked<'a> {
    provider: &'a dyn Provider,
    label: &'static str,
    events: Sender<UiEvent>,
    cancel: CancellationToken,
}

impl<'a> Networked<'a> {
    pub(crate) fn new(
        provider: &'a dyn Provider,
        label: &'static str,
        events: Sender<UiEvent>,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            provider,
            label,
            events,
            cancel,
        }
    }
}

#[async_trait::async_trait(?Send)]
impl Provider for Networked<'_> {
    async fn stream(
        &self,
        request: &CompletionRequest,
        deltas: &mut dyn DeltaSink,
    ) -> Result<Completion, ProviderError> {
        let started = std::time::Instant::now();
        let settled = self.provider.stream(request, deltas).await;
        let elapsed = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        if let Some(record) = network_record(self.label, &request.model, elapsed, &settled) {
            // Best effort, as upstream's is ("publication failure never fails
            // the turn"): a turn already cancelled, or a UI already gone,
            // loses a record nobody would see.
            let _ = bridge::send_ui(&self.events, &self.cancel, UiEvent::FullDetail(record)).await;
        }
        settled
    }
}

/// Now, as the milliseconds a header is stamped with.
fn system_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|since| i64::try_from(since.as_millis()).ok())
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// what the runtime hands over
// ---------------------------------------------------------------------------

/// What the session's journal knows about a call that the document is never
/// given, on its way to this screen ([`Teed`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Detail {
    /// What the call was made with, one argument per row
    /// (`appendFullSemanticArguments`, `full_transcript_screen.zig:6452-6530`).
    Arguments(Vec<Argument>),
    /// What a command printed, and how it stopped when that was not a clean
    /// exit (`appendInlineCommandResult`, `appendCommandProcessPresentation`,
    /// `:6765-6860`).
    Command {
        output: String,
        status: Option<String>,
    },
    /// Anything else the tool returned to the model, made readable the way
    /// upstream makes it (`tool_result_display.contentForDisplay`).
    Result(String),
    /// The change a file call made: the part of the file it touched, before
    /// and after, as the plan the change was judged on carries it -- upstream
    /// keeps one per change for this screen (`diff_block` entries,
    /// `full_transcript_screen.zig:5731-5760`).
    Change(ApprovalDiff),
}

/// One argument of a call: `name: value`, the value as the model sent it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Argument {
    pub(crate) name: String,
    pub(crate) value: String,
    /// Whether the value was a JSON string the heading could show as the model
    /// wrote it: the only kind upstream leaves out when the call's own row
    /// already shows it (`argumentVisibleInToolHeading`,
    /// `full_transcript_screen.zig:6594-6619@481104fd`), which looks for the
    /// value as written in a heading fx spelled -- so one holding anything
    /// the heading spells, a control or a mark, is never found there -- and
    /// in its status phrase, whose command is its first 120 bytes: judged here,
    /// on the raw call, before anything is spelled (`super::toolgroup::status`).
    pub(crate) text: bool,
}

impl Detail {
    /// The same detail with nothing left in it that a terminal would act on
    /// or that would hide or reorder the text around it: all of it is the
    /// model's or a tool's.
    ///
    /// An argument, a result and a command's output are spelled as fx v0.0.16
    /// spells them on this screen ([`detail_spelled`]); a change's sides are
    /// still made inert (`super::bridge::inert`), the diff rows' own spelling
    /// being theirs. Idempotent: what it writes is printable ASCII.
    pub(crate) fn made_inert(self) -> Self {
        let spelled = |text: String| detail_spelled(&text, FOUR_CELLS);
        match self {
            Detail::Arguments(arguments) => Detail::Arguments(
                arguments
                    .into_iter()
                    .map(|argument| Argument {
                        name: spelled(argument.name),
                        value: spelled(argument.value),
                        text: argument.text,
                    })
                    .collect(),
            ),
            Detail::Command { output, status } => Detail::Command {
                // The tab is the row's to draw to its stop (`railed`).
                output: detail_spelled(&output, "\t"),
                status: status.map(spelled),
            },
            Detail::Result(text) => Detail::Result(spelled(text)),
            Detail::Change(change) => {
                let inert = |text: String| bridge::inert(&text).into_owned();
                Detail::Change(ApprovalDiff {
                    before: inert(change.before),
                    after: inert(change.after),
                    ..change
                })
            }
        }
    }
}

/// What fx writes for a tab in a call's argument or result: four spaces,
/// wherever it stands (`TerminalSafeIndentedWriter.append`,
/// `full_transcript_screen.zig:5171-5172@481104fd`).
const FOUR_CELLS: &str = "    ";

/// `text` as fx v0.0.16 writes a call's detail on this screen
/// (`TerminalSafeIndentedWriter`, `full_transcript_screen.zig:5147-5194@481104fd`,
/// for an argument, `appendFullSemanticArguments`, `:6681-6695`, and a
/// result, `appendStoredResultContent`, `:5196-5229`, and `:6856-6864`):
///
/// * a line feed ends the line, and stays (`:5167-5170`);
/// * a tab is `tab` -- [`FOUR_CELLS`] there; a command's output keeps its
///   own for the row to draw (`wrapLiteralCommandOutput`,
///   `assistant_wrap.zig:524-546@481104fd`);
/// * everything else is fx's `terminalSafeToken`
///   (`text_utils.zig:643-667@481104fd`), which is
///   [`crate::tools::question::terminal_safe`]: every other C0 control and
///   `DEL` `\xNN` -- a carriage return too, at a line's end or not -- and the
///   C1 controls, the zero-width and directional marks, the separators, the
///   embeddings, overrides and isolates and the byte-order mark `\u{NNNN}`;
/// * and the Arabic letter mark (`U+061C`), which fx writes as itself, is
///   `\u{061c}` too: it reorders the text around it as the marks fx names do
///   (`approval_view`'s `reorders`; `scripts/fxcap/parity-ledger.toml`,
///   `ctrl-o-command-marks`).
///
/// A command's output reaches this after [`command_stream`] has done what
/// fx's decoder does with its escapes and carriage returns; what is left of
/// either is spelled here. fx names the marks in an argument and a result,
/// not in a command's output (`command_output_content.zig:171-173@481104fd`);
/// here they are named in all three -- copying the raw ones is not safe
/// (`ctrl-o-command-marks`).
fn detail_spelled(text: &str, tab: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            out.push('\n');
        }
        for (index, piece) in line.split('\t').enumerate() {
            if index > 0 {
                out.push_str(tab);
            }
            let spelled = crate::tools::question::terminal_safe(piece);
            if spelled.contains('\u{061c}') {
                out.push_str(&spelled.replace('\u{061c}', "\\u{061c}"));
            } else {
                out.push_str(&spelled);
            }
        }
    }
    out
}

/// The details one journal record carries, by the call they belong to.
fn details(event: &SessionEvent) -> Vec<(String, Detail)> {
    match event {
        SessionEvent::AssistantMessage { tool_calls, .. } => tool_calls
            .iter()
            .map(|call| {
                (
                    call.id.clone(),
                    Detail::Arguments(arguments(&call.name, &call.input)),
                )
            })
            .collect(),
        SessionEvent::ToolResult {
            call_id,
            tool,
            output,
            ..
        } => vec![(call_id.clone(), result(tool, output))],
        _ => Vec::new(),
    }
}

/// What the full transcript shows of each call a saved turn made -- its
/// arguments and its whole result -- read off the steps the log kept, as
/// [`details`] reads them off the events a live turn journals; upstream's
/// resume replays a call through the adapters a live one went through
/// (`writeCompletedToolResult`, `app_session_runtime.zig:4970-5030`).
///
/// The entry point a resumed session's record is filled from
/// (`super::hydration`): the details go to [`FullTranscript::observe`] as
/// [`UiEvent::ToolDetail`]s before the turn's rows are replayed, and wait for
/// them there.
pub(crate) fn details_of(steps: &[TurnStep]) -> Vec<(String, Detail)> {
    let mut found = Vec::new();
    for step in steps {
        match step {
            TurnStep::Assistant { tool_calls, .. } => {
                found.extend(tool_calls.iter().map(|call| {
                    (
                        call.id.clone(),
                        Detail::Arguments(arguments(&call.name, &call.input)),
                    )
                }));
            }
            TurnStep::ToolResult {
                call_id,
                tool,
                output,
                change,
                ..
            } => {
                found.push((call_id.clone(), result(tool, output)));
                // After the result, where a live turn's journal hands it over
                // ([`TurnJournal::change_made`] follows the result's record).
                if let Some(change) = change {
                    found.push((
                        call_id.clone(),
                        Detail::Change(ApprovalDiff {
                            before: change.before.clone(),
                            after: change.after.clone(),
                            first_line: usize::try_from(change.first_line).unwrap_or(usize::MAX),
                            lines_below: usize::try_from(change.lines_below).unwrap_or(usize::MAX),
                        }),
                    ));
                }
            }
            TurnStep::ToolFeedback { .. } | TurnStep::Steered { .. } => {}
        }
    }
    found
}

/// A call's arguments, in the order its tool's schema declares them.
///
/// Upstream lists them in the order the model wrote them; xfx's JSON values
/// keep no order of their own (`serde_json` sorts an object's keys), so the
/// schema's order -- `path`, then `old_string`, then `new_string` -- stands in
/// for it, and anything the schema does not name follows in the order the map
/// gives.
fn arguments(tool: &str, input: &Value) -> Vec<Argument> {
    let Value::Object(map) = input else {
        // Upstream's fallback for arguments that are not an object
        // (`:6478-6494`).
        return vec![Argument {
            name: "arguments".to_string(),
            value: input.to_string(),
            text: false,
        }];
    };
    let declared: Vec<&str> = Registry::builtin()
        .spec(tool)
        .map(|spec| {
            spec.input_schema()
                .properties
                .iter()
                .map(|property| property.name)
                .collect()
        })
        .unwrap_or_default();
    let mut names: Vec<&String> = map
        .keys()
        .filter(|name| declared.contains(&name.as_str()))
        .collect();
    names.sort_by_key(|name| declared.iter().position(|declared| declared == name));
    names.extend(map.keys().filter(|name| !declared.contains(&name.as_str())));
    let status = Registry::builtin()
        .spec(tool)
        .and_then(|spec| super::toolgroup::status(&spec.shown(input)));
    names
        .into_iter()
        .map(|name| {
            let value = &map[name];
            let (value, text) = match value {
                // Shown by the heading only as written, never once it spelled
                // a control or a mark (`super::toolgroup::spelled`), and only
                // where fx's status phrase holds it as written
                // (`super::toolgroup::status`, made from the raw call).
                Value::String(text) => (
                    text.clone(),
                    crate::tools::question::terminal_safe(text).len() == text.len()
                        && !text.contains('\u{061c}')
                        && status
                            .as_deref()
                            .is_none_or(|status| status.contains(text.as_str())),
                ),
                other => (other.to_string(), false),
            };
            Argument {
                name: name.clone(),
                value: wrap::expand_tabs(&value).into_owned(),
                text,
            }
        })
        .collect()
}

/// What a tool returned, as this screen shows it.
fn result(tool: &str, output: &str) -> Detail {
    if tool == "terminal" {
        if let Some((output, status)) = command_result(output) {
            return Detail::Command {
                output: bounded(&output),
                status,
            };
        }
    }
    Detail::Result(bounded(&content_for_display(output)))
}

/// A command's output out of the frame `terminal` reports it in
/// (`crate::tools::terminal`'s `Outcome::into_result`): the streams with the
/// tool's escaping undone, each read as fx's decoder reads it
/// ([`command_stream`]), stdout before stderr, and the status when it was
/// not a clean exit -- upstream's `exit code N` and `signal N` rows
/// (`appendCommandProcessPresentation`, `:6765-6795`). Its tabs are kept, as
/// fx keeps them (`Decoder.consumeByte`, `command_output_content.zig:
/// 119-122@481104fd`), for the row to draw to the screen's stops.
///
/// `None` for anything not in that frame -- a refusal, a timeout, a
/// cancellation -- which is then shown as the text it is.
fn command_result(output: &str) -> Option<(String, Option<String>)> {
    let rest = output.strip_prefix("<command>")?;
    let (_, rest) = rest.split_once("</command>\n<cwd>")?;
    let (_, rest) = rest.split_once("</cwd>\n")?;
    let (status, rest) = if let Some(rest) = rest.strip_prefix("<timed_out/>\n") {
        // One xfx killed at its deadline: upstream's `timed out` row
        // (`fx-timeout-40x120-live-ctrl-o`).
        (Some("timed out".to_string()), rest)
    } else if let Some(rest) = rest.strip_prefix("<exit_code>") {
        let (code, rest) = rest.split_once("</exit_code>\n")?;
        let code: i32 = code.parse().ok()?;
        ((code != 0).then(|| format!("exit code {code}")), rest)
    } else {
        let rest = rest.strip_prefix("<signal>")?;
        let (signal, rest) = rest.split_once("</signal>\n")?;
        let signal: i32 = signal.parse().ok()?;
        (Some(format!("signal {signal}")), rest)
    };
    let rest = rest.strip_prefix("<stdout>\n")?;
    let (stdout, rest) = rest.split_once("</stdout>\n<stderr>\n")?;
    let (stderr, _) = rest.split_once("</stderr>")?;
    let mut text = String::new();
    for stream in [stdout, stderr] {
        text.push_str(&command_stream(&unframed(stream)));
    }
    let text = text.strip_suffix('\n').unwrap_or(&text).to_string();
    Some((text, status))
}

/// The most of one escape sequence fx's decoder keeps to write back when the
/// sequence never ends (`Decoder.escape_bytes`, `command_output_content.zig:
/// 57@481104fd`).
const ESCAPE_KEPT_BYTES: usize = 64;

/// One of a command's streams as fx v0.0.16's decoder makes it presentable
/// (`Decoder`, `command_output_content.zig:52-282@481104fd`), before a byte of
/// it is measured or bounded:
///
/// * an escape sequence is dropped whole -- a CSI to its final byte, an OSC
///   to its `BEL` or `ESC \`, `ESC` and one byte in `@`..`_` (`:196-253`);
///   one a control interrupts, or the stream ends inside, is written back
///   instead, its controls as `\xNN` and `...` after it when it ran past
///   [`ESCAPE_KEPT_BYTES`] (`flushIncompleteEscape`, `:264-275`); an `ESC`
///   before anything else is written back and the byte after it read again
///   (`:212-219`);
/// * a carriage return before a line feed ends the line with it, one before
///   anything else starts the line again (`replaceLine`, `:86-95`), and one
///   that ends the stream is dropped (`finish`, `:68-72`);
/// * a tab and printable ASCII are themselves, any other C0 control or `DEL`
///   is `\xNN`, a C1 control `\u{00NN}` (`:113-176`), and everything else is
///   itself -- [`detail_spelled`] names what fx leaves there.
///
/// fx works on bytes and names each one outside valid UTF-8 `\xNN`; a stream
/// here is already text (`terminal` decodes its bytes lossily), and an escape
/// kept past its last whole character is cut on that character rather than
/// inside it.
fn command_stream(raw: &str) -> String {
    // fx's `EscapeState` (`:29-35`).
    #[derive(PartialEq)]
    enum EscapeState {
        None,
        Escape,
        Csi,
        Osc,
        OscEscape,
    }
    struct Decoder {
        out: String,
        line_start: usize,
        pending_cr: bool,
        state: EscapeState,
        kept: String,
        overflow: bool,
    }
    impl Decoder {
        fn byte_escape(&mut self, character: char) {
            use std::fmt::Write as _;
            let _ = write!(self.out, "\\x{:02x}", u32::from(character));
        }
        /// `recordEscapeByte` (`:255-262`): the sequence's first
        /// [`ESCAPE_KEPT_BYTES`], then nothing -- once it has overflowed, a
        /// shorter character after it is not kept either. A character is kept
        /// whole or not at all, where fx keeps bytes: one that would straddle
        /// the 64th byte ends what is kept one character early.
        fn keep(&mut self, character: char) {
            if self.overflow {
                return;
            }
            if self.kept.len() + character.len_utf8() <= ESCAPE_KEPT_BYTES {
                self.kept.push(character);
            } else {
                self.overflow = true;
            }
        }
        fn reset(&mut self) {
            self.state = EscapeState::None;
            self.kept.clear();
            self.overflow = false;
        }
        fn flush_incomplete(&mut self) {
            let kept = std::mem::take(&mut self.kept);
            for character in kept.chars() {
                if character < ' ' || character == '\u{7f}' {
                    self.byte_escape(character);
                } else {
                    self.out.push(character);
                }
            }
            if self.overflow {
                self.out.push_str("...");
            }
            self.reset();
        }
        /// `consumeEscapeByte`: whether `character` was the sequence's.
        fn escape(&mut self, character: char) -> bool {
            match self.state {
                EscapeState::None => false,
                EscapeState::Escape => match character {
                    '[' => {
                        self.keep(character);
                        self.state = EscapeState::Csi;
                        true
                    }
                    ']' => {
                        self.keep(character);
                        self.state = EscapeState::Osc;
                        true
                    }
                    '\u{40}'..='\u{5f}' => {
                        self.reset();
                        true
                    }
                    _ => {
                        self.flush_incomplete();
                        false
                    }
                },
                EscapeState::Csi => {
                    self.keep(character);
                    if ('\u{40}'..='\u{7e}').contains(&character) {
                        self.reset();
                    } else if character < ' ' || character == '\u{7f}' {
                        self.flush_incomplete();
                    }
                    true
                }
                EscapeState::Osc => {
                    self.keep(character);
                    if character == '\u{7}' {
                        self.reset();
                    } else if character == '\u{1b}' {
                        self.state = EscapeState::OscEscape;
                    }
                    true
                }
                EscapeState::OscEscape => {
                    self.keep(character);
                    if character == '\\' {
                        self.reset();
                    } else if character != '\u{1b}' {
                        self.state = EscapeState::Osc;
                    }
                    true
                }
            }
        }
        /// `consumeByte`, a character at a time.
        fn consume(&mut self, character: char) {
            if self.escape(character) {
                return;
            }
            if self.pending_cr {
                self.pending_cr = false;
                if character == '\n' {
                    self.out.push('\n');
                    self.line_start = self.out.len();
                    return;
                }
                self.out.truncate(self.line_start);
            }
            match character {
                '\u{1b}' => {
                    self.state = EscapeState::Escape;
                    self.kept.clear();
                    self.overflow = false;
                    self.keep(character);
                }
                '\r' => self.pending_cr = true,
                '\n' => {
                    self.out.push('\n');
                    self.line_start = self.out.len();
                }
                '\t' | ' '..='\u{7e}' => self.out.push(character),
                '\u{80}'..='\u{9f}' => {
                    use std::fmt::Write as _;
                    let _ = write!(self.out, "\\u{{{:04x}}}", u32::from(character));
                }
                _ if character < ' ' || character == '\u{7f}' => self.byte_escape(character),
                _ => self.out.push(character),
            }
        }
    }
    let mut decoder = Decoder {
        out: String::with_capacity(raw.len()),
        line_start: 0,
        pending_cr: false,
        state: EscapeState::None,
        kept: String::new(),
        overflow: false,
    };
    for character in raw.chars() {
        decoder.consume(character);
    }
    // `finish`.
    decoder.pending_cr = false;
    if decoder.state != EscapeState::None {
        decoder.flush_incomplete();
    }
    decoder.out
}

/// `terminal`'s escaping undone (`framed`: `&`, `<`, `>`).
fn unframed(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// A result with the `<path>`/`<content>` envelope `read_file` wraps a file in
/// taken off, and its tabs made the four cells upstream paints them as
/// (`tool_result_display.contentForDisplay`; `TerminalSafeIndentedWriter`,
/// `full_transcript_screen.zig:4986-5033`). Anything not in that shape is
/// itself.
fn content_for_display(output: &str) -> String {
    let unwrapped = (|| {
        let mut rest = output;
        if let Some(after) = rest.strip_prefix("<path>") {
            let (_, after) = after.split_once("</path>")?;
            rest = after.trim_start_matches(['\r', '\n']);
        }
        let rest = rest
            .strip_prefix("<content>")?
            .trim_start_matches(['\r', '\n']);
        let rest = rest.strip_suffix("</content>")?;
        Some(rest.trim_end_matches(['\r', '\n']))
    })()
    .unwrap_or(output);
    wrap::expand_tabs(unwrapped).into_owned()
}

/// At most [`RESULT_BYTES`] of `text`, cut on a character boundary, and a
/// line saying how much more the log holds when it was cut.
fn bounded(text: &str) -> String {
    if text.len() <= RESULT_BYTES {
        return text.to_string();
    }
    let mut end = RESULT_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!(
        "{}\n\u{2026} {} more bytes are in the session's log",
        &text[..end],
        text.len() - end
    )
}

/// The session's journal, with what this screen needs of it handed to the UI
/// as it is written down.
///
/// Stands where the recorder stood (`super::worker`'s `run_turn`): every
/// record goes to the journal first, untouched and unconditionally, and only
/// then are its [`details`] sent. **Best effort**, like the history notices
/// beside it: the log is the record and this is a view of it, so a detail the
/// UI could not be given -- a cancelled turn, a UI that is gone -- costs the
/// view and nothing else.
pub(crate) struct Teed<'a> {
    journal: &'a mut dyn TurnJournal,
    events: Sender<UiEvent>,
    cancel: CancellationToken,
}

impl<'a> Teed<'a> {
    pub(crate) fn new(
        journal: &'a mut dyn TurnJournal,
        events: Sender<UiEvent>,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            journal,
            events,
            cancel,
        }
    }
}

impl Teed<'_> {
    /// Hands one detail to the UI, and says whether it could.
    ///
    /// Through the same door every event takes ([`bridge::send_ui`]), waiting
    /// for a permit the way the event sink does: the journal is as
    /// synchronous a seam as that one.
    fn hand_over(&self, call_id: String, detail: Detail) -> bool {
        bridge::park_on(bridge::send_ui(
            &self.events,
            &self.cancel,
            UiEvent::ToolDetail { call_id, detail },
        ))
        .is_ok()
    }
}

impl TurnJournal for Teed<'_> {
    fn record(&mut self, event: SessionEvent) {
        let details = details(&event);
        self.journal.record(event);
        for (call_id, detail) in details {
            if !self.hand_over(call_id, detail) {
                return;
            }
        }
    }

    /// Nothing for the log, which never keeps one; the change goes to the
    /// screen alone.
    fn change_made(&mut self, call_id: &str, change: &ApprovalDiff) {
        self.journal.change_made(call_id, change);
        self.hand_over(call_id.to_string(), Detail::Change(change.clone()));
    }

    /// The screen's account of the call, after the result whose detail it
    /// replaces: a stopped command's output and the signal it died of.
    fn presented(&mut self, call_id: &str, tool: &str, text: &str) {
        self.journal.presented(call_id, tool, text);
        self.hand_over(call_id.to_string(), result(tool, text));
    }

    /// The log's alone: a prompt's images are on its card already.
    fn attach_images(&mut self, images: &[crate::image::Image]) -> Option<std::path::PathBuf> {
        self.journal.attach_images(images)
    }

    fn attach_tool_images(&mut self, call_id: &str, images: &[crate::image::ToolImage]) {
        self.journal.attach_tool_images(call_id, images);
    }

    fn state(&self) -> Option<&crate::session::DurableState> {
        self.journal.state()
    }
}

// ---------------------------------------------------------------------------
// the record
// ---------------------------------------------------------------------------

/// One thing the document was given, and when.
struct Entry {
    at_ms: i64,
    body: Body,
    /// Moved on by every change to [`Self::body`], so a layout of the old one
    /// is never taken for a layout of the new.
    revision: u64,
    /// The rows this entry was laid out as last, and for what.
    drawn: RefCell<Option<(Key, Rc<Vec<String>>)>>,
}

/// What a layout depends on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Key {
    cols: u16,
    palette: Palette,
    revision: u64,
}

enum Body {
    Prompt(String),
    /// What the presenter completed of the answer, as the document was given
    /// it: rendered prose, code, tables and rules, laid out at the screen's
    /// width when drawn.
    Response(Vec<markdown::Event>),
    Notice(Notice),
    Usage(String),
    /// Rows the document was given already laid out: what a question batch
    /// leaves behind (`super::question`'s `answered_rows`), a cancelled turn's
    /// line (`super::shell`'s `cancelled_line`) -- each with what the document
    /// recorded it as, and the mode they were drawn in, so a theme report
    /// recolours them here as it does there.
    Block(Vec<(String, DocumentRow)>, Mode),
    /// Boxed: a call carries its arguments, its result and the change it made,
    /// and a record of one-line notices should not be as wide as that.
    Tool(Box<Call>),
}

/// One tool call, as the transcript's group shows it and as the journal
/// recorded it.
struct Call {
    id: String,
    shown: Shown,
    state: State,
    /// Which of the turn's tool groups it was drawn in
    /// (`super::toolgroup`): upstream draws the group's header over its first
    /// call and connects each call under it, here as in the transcript.
    group: usize,
    arguments: Vec<Argument>,
    result: Option<Detail>,
    /// The change it made, when it made one ([`Detail::Change`]).
    change: Option<ApprovalDiff>,
    /// Drawn from a resumed session's log rather than run
    /// ([`FullTranscript::replay`]).
    replayed: bool,
    /// Its heading as it was last laid out ([`FullTranscript::heading`]).
    heading: RefCell<Option<Heading>>,
}

/// A call's heading laid out at one width: its rows wrapped under their rail
/// and whether it fits one row -- and what it was laid out from, the block
/// row it is (named by its block and index, so the text is not held twice).
struct Heading {
    block: Rc<Vec<String>>,
    index: usize,
    cols: u16,
    rows: Rc<Vec<String>>,
    fits: bool,
}

/// A tool group's expanded block as it was last built, and for what
/// ([`FullTranscript::group_block`]).
struct GroupBlock {
    revision: u64,
    palette: Palette,
    rows: Rc<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum State {
    Running,
    Finished {
        ok: bool,
        detail: String,
        outcome: Outcome,
        changes: Option<LineChanges>,
    },
    /// Still running when its turn ended (`super::shell`'s `settle_group`).
    Cancelled,
}

/// What arrived for a call before its row did.
#[derive(Default)]
struct Pending {
    arguments: Option<Vec<Argument>>,
    result: Option<Detail>,
    change: Option<ApprovalDiff>,
}

/// The conversation as this session recorded it, and the screen it is shown on
/// while that screen is up.
pub(crate) struct FullTranscript {
    clock: fn() -> i64,
    /// The time new entries are stamped with instead of the clock's: a
    /// resumed turn's, while it is replayed ([`Self::stamp_with`]).
    stamp: Option<i64>,
    /// Whether the calls drawn now are a resumed session's ([`Self::replay`]).
    replaying: bool,
    /// The time the next call's entry is stamped with instead
    /// ([`Self::stamp_call`]).
    call_stamp: Option<i64>,
    /// Whether the document opened with the welcome line, which upstream's
    /// screen shows first and without a header.
    welcome: bool,
    /// Upstream's full-detail records -- the session it assembled, each
    /// provider request it settled -- which the document is never given.
    /// Kept apart from the entries and shown above them, under the welcome,
    /// in the order they came (`prependFullDetailRecords`,
    /// `transcript/runtime.zig:4970-5001`).
    records: Vec<Entry>,
    entries: Vec<Entry>,
    /// Which entry each call is.
    calls: HashMap<String, usize>,
    /// How many times anything in each tool group changed: a call's row is
    /// laid out from its whole group, since the header counts every call.
    groups: Vec<u64>,
    /// Each group's expanded block, built once for its revision and palette
    /// and shared by every call's entry in it.
    blocks: RefCell<HashMap<usize, GroupBlock>>,
    /// Whether the next call joins the last group. Closed by the answer's
    /// text, a card and the turn's end -- what closes the transcript's own
    /// group -- and not by a notice, which the group carries under it.
    group_open: bool,
    pending: HashMap<String, Pending>,
    revision: u64,
    view: Option<View>,
    /// How many times a tool group's expanded block was built, and a call's
    /// heading laid out: what a layout of the record costs, for the tests.
    #[cfg(test)]
    built: Cell<(usize, usize)>,
}

/// The screen, while it is up.
struct View {
    /// Where the reader is ([`position`]).
    reading: Cell<Reading>,
    /// The page the screen was last drawn from: what it was laid out for, and
    /// where each entry starts on it.
    page: RefCell<Option<Page>>,
}

/// One page as it was measured.
struct Page {
    cols: u16,
    /// [`FullTranscript::revision`] when it was laid out: any change to the
    /// record moves it on, so a different one is a rebuilt page.
    revision: u64,
    items: Vec<ItemRow>,
}

/// The page the screen shows now, and the first of its rows on screen.
struct Selected {
    offset: usize,
    total: usize,
    items: Vec<ItemRow>,
}

impl View {
    fn at_tail() -> Self {
        Self {
            reading: Cell::new(Reading::default()),
            page: RefCell::new(None),
        }
    }

    /// Where the screen stands on the page as it is now, `viewport` rows at a
    /// time.
    ///
    /// A page that rebuilt since the last one -- anything recorded, or a
    /// width it was not laid out at -- has the reading position pinned to the
    /// entry it showed on the last page first, and found on this one
    /// (upstream pins it on each page it installs, `presentationForRebuiltPage`,
    /// `transcript/runtime.zig:11584-11590@481104fd`, and selects on the page
    /// it measures, `selectProjectionViewportOffset`, `:11647-11660`). A
    /// screen at the tail follows it; rows added under a screen that is not
    /// leave it where it is. Asked by the paint and by a keystroke alike, so a
    /// key pressed between a change and the frame that would show it moves
    /// from where the reader really is.
    fn select(
        &self,
        record: &FullTranscript,
        cols: u16,
        viewport: usize,
        palette: Palette,
    ) -> Selected {
        let (total, items) = record.measure(cols, palette);
        let mut reading = self.reading.get();
        if let Some(page) = self.page.borrow().as_ref() {
            if page.cols != cols || page.revision != record.revision {
                reading = reading.pin_reading_position(&page.items);
            }
        }
        let (reading, offset) = reading.select_visual_offset(total, viewport, &items);
        self.reading.set(reading);
        *self.page.borrow_mut() = Some(Page {
            cols,
            revision: record.revision,
            items: items.clone(),
        });
        Selected {
            offset,
            total,
            items,
        }
    }
}

/// What a keystroke did to the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reply {
    /// Close it.
    Close,
    /// The rows it shows moved.
    Moved,
    /// A key the screen binds, that changed nothing here: an arrow past an
    /// end, Left, Right, a redraw.
    Held,
    /// A key the screen does not bind. Upstream hands it on to its composer
    /// (`app_input_runtime.zig:973-994`), and so does the shell.
    Unbound,
}

impl Default for FullTranscript {
    fn default() -> Self {
        Self::with_clock(system_ms)
    }
}

impl FullTranscript {
    fn with_clock(clock: fn() -> i64) -> Self {
        Self {
            clock,
            stamp: None,
            replaying: false,
            call_stamp: None,
            welcome: false,
            records: Vec::new(),
            entries: Vec::new(),
            calls: HashMap::new(),
            groups: Vec::new(),
            blocks: RefCell::default(),
            group_open: false,
            pending: HashMap::new(),
            revision: 0,
            view: None,
            #[cfg(test)]
            built: Cell::default(),
        }
    }

    fn push(&mut self, body: Body) {
        let entry = self.entry(body);
        self.entries.push(entry);
    }

    fn entry(&mut self, body: Body) -> Entry {
        self.revision += 1;
        Entry {
            at_ms: self.stamp.unwrap_or_else(self.clock),
            body,
            revision: self.revision,
            drawn: RefCell::new(None),
        }
    }

    /// Stamps what is recorded from here with `at_ms` rather than the clock,
    /// or with the clock again for `None`: a resumed session's turns carry
    /// the times the log has for them, as upstream's replay sets each one's
    /// (`setCreatedAtMs`, `app_session_runtime.zig:4753-4810`, through
    /// `ResumeProjection.setCreatedAtMs`, `resume_projection.zig:76-78`).
    pub(crate) fn stamp_with(&mut self, at_ms: Option<i64>) {
        self.stamp = at_ms;
    }

    /// Stamps the next call's entry, and only it, with `at_ms`: a resumed
    /// call's result's own time, which upstream's header prefers to its
    /// entry's (`full_transcript_screen.zig:5801-5814`). The answer text
    /// before the call is not that call's, so it keeps [`Self::stamp_with`]'s.
    pub(crate) fn stamp_call(&mut self, at_ms: Option<i64>) {
        self.call_stamp = at_ms;
    }

    /// The clock's time now: the moment a resume's stamps start from.
    pub(crate) fn now(&self) -> i64 {
        (self.clock)()
    }

    /// Whether the calls drawn from here are a resumed session's, replayed
    /// from its log rather than run: upstream's replay hands each one the
    /// output the log kept, a file change's included ([`Call::replayed`]).
    pub(crate) fn replay(&mut self, replaying: bool) {
        self.replaying = replaying;
    }

    /// One of upstream's full-detail records: shown here and nowhere else
    /// (`visibility = .full_only`).
    pub(crate) fn full_detail(&mut self, notice: Notice) {
        let entry = self.entry(Body::Notice(notice));
        self.records.push(entry);
    }

    /// Every entry in the order the screen shows them: the records, then
    /// what the document was given.
    fn shown(&self) -> impl Iterator<Item = &Entry> {
        self.records.iter().chain(&self.entries)
    }

    /// The document opened with the welcome line.
    pub(crate) fn welcome(&mut self) {
        self.welcome = true;
    }

    /// A card: what the user submitted.
    pub(crate) fn prompt(&mut self, text: &str) {
        self.group_open = false;
        self.push(Body::Prompt(text.to_string()));
    }

    /// Something the answer's presenter completed and the document was
    /// given (`super::shell`'s `present`).
    ///
    /// One entry per run of it between other entries, as upstream's assistant
    /// entry holds what its presenter gave it. What the presenter is still
    /// holding -- a line whose end has not arrived -- is in no entry yet, so a
    /// notice that lands meanwhile is an entry of its own **before** that
    /// line, which goes on, whole, in the entry after it: upstream's order,
    /// whose notice is appended beside the presenter and asks it for nothing
    /// (`store.zig:1343-1390`, `fx-fulltranscript-notice-40x120-full-top`).
    /// Prose that is only line breaks opens none, for the reason the document
    /// writes none for it (`super::shell`'s `write_prose`), and closes no
    /// group either.
    pub(crate) fn presented(&mut self, event: &markdown::Event) {
        self.revision += 1;
        let revision = self.revision;
        if let Some(Entry {
            body: Body::Response(events),
            revision: entry,
            ..
        }) = self.entries.last_mut()
        {
            events.push(event.clone());
            *entry = revision;
            return;
        }
        if matches!(event, markdown::Event::Text(text) if text.trim().is_empty()) {
            return;
        }
        self.group_open = false;
        self.push(Body::Response(vec![event.clone()]));
    }

    /// Answer text through a presenter of its own, completed, as the shell's
    /// presents a whole answer.
    #[cfg(test)]
    pub(crate) fn answer(&mut self, text: &str) {
        let mut processor = markdown::MarkdownProcessor::with_styles(markdown::ansi::Styles::new(
            markdown::theme::builtin(false),
        ));
        for event in processor.feed(text) {
            self.presented(&event);
        }
        for event in processor.finish() {
            self.presented(&event);
        }
    }

    pub(crate) fn notice(&mut self, notice: &Notice) {
        self.push(Body::Notice(notice.clone()));
    }

    /// Rows laid out already -- a question batch's resolution, a cancelled
    /// turn's line -- which close the group as any other block written into
    /// the document does.
    pub(crate) fn block(&mut self, rows: &[(String, DocumentRow)], drawn_in: Mode) {
        self.group_open = false;
        self.push(Body::Block(rows.to_vec(), drawn_in));
    }

    /// The turn's summary under its answer.
    pub(crate) fn usage(&mut self, line: &str) {
        self.group_open = false;
        self.push(Body::Usage(line.to_string()));
    }

    /// A call's row was drawn: it joins the open group or starts one.
    pub(crate) fn tool_started(&mut self, id: &str, shown: &Shown) {
        if !self.group_open {
            self.groups.push(0);
            self.group_open = true;
        }
        let group = self.groups.len() - 1;
        let pending = self.pending.remove(id).unwrap_or_default();
        let stamp = self.stamp;
        if let Some(at_ms) = self.call_stamp.take() {
            self.stamp = Some(at_ms);
        }
        self.push(Body::Tool(Box::new(Call {
            id: id.to_string(),
            shown: shown.clone(),
            state: State::Running,
            group,
            arguments: pending.arguments.unwrap_or_default(),
            result: pending.result,
            change: pending.change,
            replayed: self.replaying,
            heading: RefCell::default(),
        })));
        self.stamp = stamp;
        self.calls.insert(id.to_string(), self.entries.len() - 1);
        self.touch_group(group);
    }

    /// A call's row settled.
    pub(crate) fn tool_finished(
        &mut self,
        id: &str,
        ok: bool,
        detail: &str,
        outcome: Outcome,
        changes: Option<LineChanges>,
    ) {
        self.change_call(id, |call| {
            if call.state == State::Running {
                call.state = State::Finished {
                    ok,
                    detail: detail.to_string(),
                    outcome,
                    changes,
                };
            }
        });
    }

    /// Whatever the group still has running was stopped, and the group is
    /// closed (`super::shell`'s `settle_group`): a turn that ended, or a
    /// question escaped out of.
    pub(crate) fn settle(&mut self) {
        let running: Vec<String> = self
            .entries
            .iter()
            .filter_map(|entry| match &entry.body {
                Body::Tool(call) if call.state == State::Running => Some(call.id.clone()),
                _ => None,
            })
            .collect();
        for id in running {
            self.change_call(&id, |call| call.state = State::Cancelled);
        }
        self.group_open = false;
    }

    /// The turn is over: its calls are settled, and nothing more will arrive
    /// for one that has not got a row.
    pub(crate) fn turn_over(&mut self) {
        self.settle();
        // A detail for a call that never got a row is about a call that never
        // ran -- the rest of a step the turn abandoned -- and nothing will ask
        // for it now.
        self.pending.clear();
    }

    /// What the runtime said that the document is not told about: the
    /// journal's details for each call, and the records only this screen
    /// shows.
    pub(crate) fn observe(&mut self, event: &UiEvent) {
        if let UiEvent::FullDetail(notice) = event {
            self.full_detail(notice.clone());
        }
        if let UiEvent::ToolDetail { call_id, detail } = event {
            match detail {
                Detail::Arguments(arguments) => self.attach(
                    call_id,
                    |call| call.arguments = arguments.clone(),
                    |pending| pending.arguments = Some(arguments.clone()),
                ),
                Detail::Change(change) => self.attach(
                    call_id,
                    |call| call.change = Some(change.clone()),
                    |pending| pending.change = Some(change.clone()),
                ),
                result => self.attach(
                    call_id,
                    |call| call.result = Some(result.clone()),
                    |pending| pending.result = Some(result.clone()),
                ),
            }
        }
    }

    /// Gives `id`'s call what arrived for it, or keeps it for the call's row.
    fn attach(
        &mut self,
        id: &str,
        to_call: impl FnOnce(&mut Call),
        to_pending: impl FnOnce(&mut Pending),
    ) {
        if self.calls.contains_key(id) {
            self.change_call(id, to_call);
        } else {
            to_pending(self.pending.entry(id.to_string()).or_default());
        }
    }

    fn change_call(&mut self, id: &str, change: impl FnOnce(&mut Call)) {
        let Some(&at) = self.calls.get(id) else {
            return;
        };
        let Body::Tool(call) = &mut self.entries[at].body else {
            return;
        };
        change(call);
        let group = call.group;
        self.touch_group(group);
    }

    fn touch_group(&mut self, group: usize) {
        self.revision += 1;
        if let Some(revision) = self.groups.get_mut(group) {
            *revision = self.revision;
        }
    }

    // -----------------------------------------------------------------------
    // the screen
    // -----------------------------------------------------------------------

    pub(crate) fn is_open(&self) -> bool {
        self.view.is_some()
    }

    /// Opens the screen at the tail.
    pub(crate) fn open(&mut self) {
        self.view = Some(View::at_tail());
    }

    pub(crate) fn close(&mut self) {
        self.view = None;
    }

    /// What one keystroke does while the screen is up: upstream's key map
    /// (`keyForAction`/`routeKey`, `input_full_transcript_runtime.zig:147-198`)
    /// and nothing more -- Ctrl-O, Escape and Ctrl-C close, the page keys
    /// page, Up and Down move three rows, Left, Right and a redraw are held.
    /// Home, End and everything else are [`Reply::Unbound`].
    ///
    /// `rows` is the terminal's height: a page is the rows the screen shows
    /// (`fullTranscriptPageRows`, `transcript/runtime.zig:6882-6888`).
    pub(crate) fn key(&self, action: Action, cols: u16, rows: u16, palette: Palette) -> Reply {
        let Some(view) = &self.view else {
            return Reply::Unbound;
        };
        // How far one key moves the screen, up (toward the head) or down
        // (upstream's `scroll`, then the selection its next frame makes).
        let moved = |up: bool, rows_moved: Option<usize>| {
            let page = Layout::of(rows).viewport.max(1);
            let now = view.select(self, cols, page, palette);
            let step = rows_moved.unwrap_or(page);
            let (reading, offset) = view
                .reading
                .get()
                .scroll(up, step)
                .select_visual_offset(now.total, page, &now.items);
            view.reading.set(reading);
            if offset == now.offset {
                Reply::Held
            } else {
                Reply::Moved
            }
        };
        match action {
            Action::ToggleTranscript | Action::Escape | Action::Cancel => Reply::Close,
            Action::PageUp => moved(true, None),
            Action::PageDown => moved(false, None),
            Action::Up => moved(true, Some(ARROW_ROWS)),
            Action::Down => moved(false, Some(ARROW_ROWS)),
            Action::Left | Action::Right | Action::Redraw => Reply::Held,
            _ => Reply::Unbound,
        }
    }

    /// The whole screen, top first: exactly `rows` rows, none wider than
    /// `cols`. `status` is the hint row as the band would paint it.
    pub(crate) fn rows(&self, cols: u16, rows: u16, palette: Palette, status: &str) -> Vec<String> {
        let layout = Layout::of(rows);
        let (start, total) = match &self.view {
            Some(view) => {
                let selected = view.select(self, cols, layout.viewport, palette);
                (selected.offset, selected.total)
            }
            None => {
                let total = self.measure(cols, palette).0;
                (total.saturating_sub(layout.viewport), total)
            }
        };
        let shown = total.saturating_sub(start).min(layout.viewport);
        let mut screen = self.window(start, shown, cols, palette);
        screen.resize(layout.viewport, String::new());
        let reset = palette.reset();
        for part in &layout.footer {
            screen.push(match part {
                Part::Gap => String::new(),
                Part::Text => {
                    let text = ellipsized(FOOTER, cols.saturating_sub(2));
                    if cols < 2 {
                        clip(&format!("{}\u{2503}{reset}", palette.hint()), cols).to_string()
                    } else {
                        format!(
                            "{}\u{2503}{reset} {}{text}{reset}",
                            palette.hint(),
                            palette.statusline()
                        )
                    }
                }
                Part::Status => status.to_string(),
            });
        }
        screen
    }

    /// Where the caret goes: the blank row under the footer's text, or the
    /// text's own row when the screen is too short to have one. One-based row,
    /// zero-based column, as [`super::frame::Band::enter_alternate`] takes it.
    pub(crate) fn caret(&self, rows: u16) -> (u16, u16) {
        let layout = Layout::of(rows);
        let text = layout
            .footer
            .iter()
            .position(|part| *part == Part::Text)
            .unwrap_or(0);
        let below = layout
            .footer
            .get(text + 1)
            .filter(|part| **part == Part::Gap)
            .map_or(text, |_| text + 1);
        let row = layout.viewport + below + 1;
        (u16::try_from(row).unwrap_or(rows).clamp(1, rows.max(1)), 0)
    }

    /// How many rows the whole record lays out as at this width, and where
    /// each entry starts among them.
    ///
    /// The welcome line is entry [`WELCOME`], a record [`RECORD`] and the
    /// document's `i`th entry `i + FIRST_ENTRY`, so the ids rise in the order
    /// they are shown, as upstream's do. An entry starts at its header, below
    /// the blank row that parts it from the one above; one that lays out as
    /// nothing starts where the next one's blank row is, as upstream marks a
    /// hidden entry before the separator it skips
    /// (`renderEntriesForFullPresentationInterruptible`,
    /// `transcript_blocks.zig:2084-2096@481104fd`).
    fn measure(&self, cols: u16, palette: Palette) -> (usize, Vec<ItemRow>) {
        let mut items = Vec::with_capacity(1 + self.records.len() + self.entries.len());
        let mut total = 0;
        if self.welcome {
            items.push(ItemRow {
                id: WELCOME,
                row: 0,
            });
            total = 1;
        }
        let ids = std::iter::repeat_n(RECORD, self.records.len())
            .chain((0..self.entries.len()).map(|index| index + FIRST_ENTRY));
        for (id, entry) in ids.zip(self.shown()) {
            let rows = self.laid_out(entry, cols, palette);
            if rows.is_empty() {
                items.push(ItemRow { id, row: total });
                continue;
            }
            if total > 0 {
                total += 1;
            }
            items.push(ItemRow { id, row: total });
            total += rows.len();
        }
        (total, items)
    }

    /// `count` rows of the record from row `start`, laid out at `cols`.
    ///
    /// Walks the entries' cached layouts and copies only the rows the screen
    /// shows, so a long session costs the screen what fits on it.
    fn window(&self, start: usize, count: usize, cols: u16, palette: Palette) -> Vec<String> {
        let end = start + count;
        let mut out = Vec::with_capacity(count);
        let mut at = 0usize;
        let mut take = |row: &str, at: &mut usize| {
            if *at >= start && *at < end {
                out.push(row.to_string());
            }
            *at += 1;
        };
        if self.welcome {
            take(
                &ellipsized(&super::shell::welcome_line(palette), cols),
                &mut at,
            );
        }
        for entry in self.shown() {
            if at >= end {
                break;
            }
            let rows = self.laid_out(entry, cols, palette);
            if rows.is_empty() {
                continue;
            }
            if at > 0 {
                take("", &mut at);
            }
            if at + rows.len() <= start {
                at += rows.len();
                continue;
            }
            for row in rows.iter() {
                take(row, &mut at);
            }
        }
        out
    }

    /// One entry's rows -- its header, then what it says -- from its cache
    /// when that is still the layout of what it says now.
    fn laid_out(&self, entry: &Entry, cols: u16, palette: Palette) -> Rc<Vec<String>> {
        let revision = match &entry.body {
            Body::Tool(call) => self.groups.get(call.group).copied().unwrap_or(0),
            _ => entry.revision,
        };
        let key = Key {
            cols,
            palette,
            revision,
        };
        if let Some((drawn, rows)) = entry.drawn.borrow().as_ref() {
            if *drawn == key {
                return Rc::clone(rows);
            }
        }
        let rows = Rc::new(self.lay_out(entry, cols, palette));
        *entry.drawn.borrow_mut() = Some((key, Rc::clone(&rows)));
        rows
    }

    fn lay_out(&self, entry: &Entry, cols: u16, palette: Palette) -> Vec<String> {
        let (kind, body) = self.content(entry, cols, palette);
        if body.is_empty() {
            return Vec::new();
        }
        let mut rows = Vec::with_capacity(body.len() + 1);
        if let (Some(kind), Some(stamp)) = (kind, stamp(entry.at_ms)) {
            let text = clip(&format!("  {stamp} \u{b7} {}", kind.label()), cols).to_string();
            rows.push(format!("{}{text}{}", palette.dim(), palette.reset()));
        }
        rows.extend(body);
        rows
    }

    /// What one entry says, laid out at `cols`, and what kind of entry its
    /// header calls it -- [`Self::lay_out`] without the header.
    fn content(&self, entry: &Entry, cols: u16, palette: Palette) -> (Option<Kind>, Vec<String>) {
        match &entry.body {
            Body::Prompt(text) => (Some(Kind::Prompt), card(text, cols, palette)),
            Body::Response(events) => (Some(Kind::Response), answer(events, cols, palette)),
            Body::Notice(notice) => (
                Some(if notice.tone == Tone::Error {
                    Kind::Error
                } else {
                    Kind::Notice
                }),
                notice_rows(notice, cols, palette),
            ),
            Body::Usage(text) => (Some(Kind::Usage), line(text, Role::Dim, cols, palette)),
            Body::Tool(call) => (Some(Kind::Tool), self.tool_rows(call, cols, palette)),
            // Upstream heads only the entries it has a kind for
            // (`metadataKind`, `full_transcript_screen.zig:5867-5895`), and a
            // question's resolution is not one of them. Its rows are the ones
            // written at the width of the moment; one wider than this screen
            // runs on to the next row, as upstream's row walker lets it. In
            // the palette now in force: upstream rewrites a question's
            // resolution and a cancelled turn's line through its token table
            // on a theme switch (`retintEntriesForTheme`, `store.zig:3722-3777`,
            // `themeOwnsRawEntry` `:3802-3814`@481104fd), as a report recolours
            // the document's rows (`super::transcript`'s `recoloured`).
            Body::Block(rows, drawn_in) => (
                None,
                rows.iter()
                    .flat_map(|(row, kind)| {
                        if *drawn_in == palette.mode {
                            run_on(row, cols)
                        } else {
                            run_on(&recoloured(row, *kind, palette), cols)
                        }
                    })
                    .collect(),
            ),
        }
    }

    /// The record as `/trace` lists it (`crate::trace::Entry`): what the
    /// document was given, in its order, and apart from it the `* network:`
    /// records -- each laid out at `cols` as this screen would lay it out,
    /// without its header, and with no colour.
    ///
    /// Laid out afresh rather than through [`Self::laid_out`], whose cache is
    /// the screen's: a report at its own width must not leave the screen a
    /// layout it would take for one of its own. The welcome line and the
    /// session record are left out: the report's summary says what each says.
    pub(crate) fn traced(
        &self,
        cols: u16,
        palette: Palette,
    ) -> (Vec<crate::trace::Entry>, Vec<crate::trace::Entry>) {
        let plain = |entry: &Entry| {
            // Upstream's names for its own entries (`writeTranscriptTimeline`,
            // `app_commands.zig:3481-3489`), where xfx has one of the same
            // kind; a prompt is what was typed, as upstream's `user` entry is,
            // rather than the card it was drawn as.
            let (kind, rows) = match &entry.body {
                Body::Prompt(text) => ("user", text.lines().map(str::to_string).collect()),
                body => {
                    let kind = match body {
                        Body::Response(_) => "assistant",
                        Body::Notice(_) => "semantic notice",
                        Body::Usage(_) => "raw",
                        Body::Tool(_) => "tool",
                        // A block: a question's resolution or a cancelled
                        // turn's line, raw entries upstream
                        // (`.question_resolution`, `.turn_cancellation`).
                        _ => "raw",
                    };
                    let (_, rows) = self.content(entry, cols, palette);
                    (kind, rows.iter().map(|row| unpainted(row)).collect())
                }
            };
            let problem = matches!(&entry.body, Body::Notice(notice)
                if matches!(notice.tone, Tone::Error | Tone::Warning));
            crate::trace::Entry {
                at_ms: entry.at_ms,
                kind,
                rows,
                problem,
            }
        };
        let timeline = self.entries.iter().map(plain).collect();
        let network = self
            .records
            .iter()
            .filter(
                |record| matches!(&record.body, Body::Notice(notice) if notice.topic == "network"),
            )
            .map(plain)
            .collect();
        (timeline, network)
    }

    /// `group`'s expanded block in `palette` (`super::toolgroup`'s
    /// `ToolGroup::expanded`), built from `members` -- every call in the group,
    /// in order -- once for the group's revision and the palette, and shared by
    /// every member's entry: each used to build the whole group again, every
    /// member's argument spelled again, to read one row of it.
    fn group_block(&self, group: usize, members: &[&Call], palette: Palette) -> Rc<Vec<String>> {
        let revision = self.groups.get(group).copied().unwrap_or(0);
        if let Some(kept) = self.blocks.borrow().get(&group) {
            if kept.revision == revision && kept.palette == palette {
                return Rc::clone(&kept.rows);
            }
        }
        let mut built = ToolGroup::default();
        for member in members {
            built.start(member.id.clone(), member.shown.clone());
            if let State::Finished {
                ok,
                detail,
                outcome,
                changes,
            } = &member.state
            {
                built.finish(&member.id, *ok, detail, *outcome, *changes);
            }
        }
        if members
            .iter()
            .any(|member| member.state == State::Cancelled)
        {
            built.cancel_running();
        }
        let rows = Rc::new(built.expanded(palette));
        #[cfg(test)]
        self.built.set((self.built.get().0 + 1, self.built.get().1));
        self.blocks.borrow_mut().insert(
            group,
            GroupBlock {
                revision,
                palette,
                rows: Rc::clone(&rows),
            },
        );
        rows
    }

    /// `call`'s heading -- row `index` of its group's `block` -- laid out at
    /// `cols`, from the call's own cache when it was laid out from the same
    /// text at the same width: a call that joins the group, or a change to
    /// another call, leaves it as it was, and only a heading whose own text
    /// changed (its state, its connector, the palette) or a resize lays it out
    /// again.
    fn heading(
        &self,
        call: &Call,
        block: &Rc<Vec<String>>,
        index: usize,
        cols: u16,
    ) -> (Rc<Vec<String>>, bool) {
        let text = block.get(index).map_or("", String::as_str);
        let mut kept = call.heading.borrow_mut();
        if let Some(heading) = kept.as_mut() {
            let same = heading.cols == cols
                && ((Rc::ptr_eq(&heading.block, block) && heading.index == index)
                    || heading.block.get(heading.index).map(String::as_str) == Some(text));
            if same {
                // The old block is let go of: this one holds the same text.
                heading.block = Rc::clone(block);
                heading.index = index;
                return (Rc::clone(&heading.rows), heading.fits);
            }
        }
        let rows = Rc::new(tool_status::expanded_rows(text, cols));
        #[cfg(test)]
        self.built.set((self.built.get().0, self.built.get().1 + 1));
        // Measured as the screen measures it (`tool_status`'s measure).
        let fits = wrap::fits(text, cols);
        *kept = Some(Heading {
            block: Rc::clone(block),
            index,
            cols,
            rows: Rc::clone(&rows),
            fits,
        });
        (rows, fits)
    }

    /// A call's rows: its group's header when it is the group's first, its own
    /// row as the group draws it, then what it was called with, what it
    /// returned and the change it made -- the change in place of its
    /// arguments, which only say what the change was asked to be.
    ///
    /// Nothing at all for a call the group draws no row for -- a question,
    /// and a file change still waiting on its approval -- as upstream draws
    /// none (`toolgroup`'s `Call::visible`).
    fn tool_rows(&self, call: &Call, cols: u16, palette: Palette) -> Vec<String> {
        let members: Vec<&Call> = self
            .entries
            .iter()
            .filter_map(|entry| match &entry.body {
                Body::Tool(other) if other.group == call.group => Some(other.as_ref()),
                _ => None,
            })
            .collect();
        let visible: Vec<&&Call> = members.iter().filter(|member| member.visible()).collect();
        let Some(position) = visible.iter().position(|member| member.id == call.id) else {
            return Vec::new();
        };
        let block = self.group_block(call.group, &members, palette);
        let mut rows = Vec::new();
        // Each row wrapped under its rail rather than cut at the edge
        // (`materializeExpandedOverride`, `tool_group_projection.zig:
        // 1061-1092@481104fd`).
        if position == 0 {
            if let Some(header) = block.first() {
                rows.extend(tool_status::expanded_rows(header, cols));
            }
        }
        let (heading, heading_fits) = self.heading(call, &block, 1 + position, cols);
        rows.extend(heading.iter().cloned());
        // An argument the heading already shows has no row of its own -- but
        // only one in the heading's first line, all the heading shows of a
        // status, and only while the heading fits one row: a short value
        // turns up inside a long command by chance, and a wrapped heading
        // must not hide more than a clipped one did
        // (`argumentVisibleInToolHeading`, `full_transcript_screen.zig:
        // 6594-6619@481104fd`, 78424464). Read off the heading only when it
        // fits and something could be in it.
        let first_line = (heading_fits
            && call.change.is_none()
            && call.arguments.iter().any(|argument| argument.text))
        .then(|| {
            let plain = unpainted(block.get(1 + position).map_or("", String::as_str));
            plain
                .split(['\n', '\r'])
                .next()
                .unwrap_or_default()
                .to_string()
        });
        if call.change.is_none() {
            for argument in &call.arguments {
                let shown_already = argument.text
                    && !argument.value.is_empty()
                    && first_line
                        .as_deref()
                        .is_some_and(|line| line.contains(argument.value.as_str()));
                let exec = call.shown.presentation.activity == Activity::Command
                    && argument.name == "action"
                    && argument.value == "exec";
                if shown_already || exec {
                    continue;
                }
                // Without the one break it may end on: upstream wraps an
                // argument's text short of it (`full_transcript_screen.zig:
                // 6381-6383`), so `content: …\n` is no rail row of its own.
                let line = format!("{}: {}", argument.name, argument.value);
                rows.extend(railed(
                    line.strip_suffix('\n').unwrap_or(&line),
                    "  ",
                    cols,
                    palette,
                ));
            }
        }
        match &call.result {
            Some(Detail::Command { output, status }) => {
                if !output.is_empty() {
                    rows.extend(railed(output, " ", cols, palette));
                }
                if let Some(status) = status {
                    rows.extend(railed(status, " ", cols, palette));
                }
            }
            Some(Detail::Result(text)) if !text.is_empty() && call.reports_its_result() => {
                // A result's last break ends its last row, and the line the
                // detail is closed with after it is a blank one, no rail
                // (`appendDetailContent`, `full_transcript_screen.zig:
                // 6618-6642`: a stored result leaves `ends_with_newline`
                // false) -- a glob's list is followed by two blank rows.
                let body = text.strip_suffix('\n');
                rows.extend(literal(body.unwrap_or(text), cols, palette));
                if body.is_some() {
                    rows.push(String::new());
                }
            }
            _ => {}
        }
        if let Some(change) = &call.change {
            rows.push(String::new());
            rows.extend(diff_rows(change, cols, palette));
        }
        rows
    }
}

impl Call {
    /// Whether what the tool said back is shown under the call.
    ///
    /// Not for a file change that was made, refused by the user or stopped:
    /// its row already says which (`Edited a.txt +1 / -1`, `Denied file`), and
    /// what it changed is the diff -- upstream shows a change's arguments or
    /// its diff and never a sentence about it (`fx-approvals-cc-40x120-full`,
    /// `fx-fulltranscript-40x120-full`). A change the tool itself refused
    /// keeps its reason, as a failed read does.
    ///
    /// A change made in a resumed session's log keeps what the tool said
    /// too, above its diff: upstream's replay hands every call the output
    /// the log kept (`attachHistoricalToolDetailResolved`,
    /// `transcript/runtime.zig:5762-5829`), where its live one gets none for
    /// a change -- `fx-resume-tools-40x120-ctrl-o-up1` against
    /// `-live-ctrl-o-up1`. A refused one still has nothing: its output is the
    /// refusal, which that replay drops (`permission_denied`, `:5774`).
    fn reports_its_result(&self) -> bool {
        let changes_a_file = matches!(
            self.shown.presentation.activity,
            Activity::Write | Activity::Edit
        );
        !changes_a_file
            || matches!(
                self.state,
                State::Finished {
                    ok: false,
                    outcome: Outcome::Finished,
                    ..
                }
            )
            || (self.replayed && matches!(self.state, State::Finished { ok: true, .. }))
    }

    /// Whether the transcript's group draws a row for this call
    /// (`super::toolgroup`'s `ToolGroup::start` and `Call::visible`): not for
    /// a question, which is never part of a group, and not for a file change
    /// while it runs -- its row starts once the change is settled.
    fn visible(&self) -> bool {
        let activity = self.shown.presentation.activity;
        activity != Activity::Ask
            && !(matches!(activity, Activity::Write | Activity::Edit)
                && self.state == State::Running)
    }
}

/// The screen's parts below the record, in the order they go when the screen
/// is too short for all of them: the top gap first, the text last.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    Gap,
    Text,
    Status,
}

struct Layout {
    viewport: usize,
    footer: Vec<Part>,
}

impl Layout {
    /// Upstream's `rows - 4` for the record and four rows under it
    /// (`local_visible_rows`, `transcript/runtime.zig:6825`;
    /// `composeTranscriptViewerFooterFrame`, `paint_plan.zig:993-1048`), and
    /// on a screen shorter than five rows the footer's own rows given up,
    /// least needed first.
    fn of(rows: u16) -> Self {
        let height = usize::from(rows);
        let mut footer = vec![Part::Gap, Part::Text, Part::Gap, Part::Status];
        // Keep at least one row of the record while there is room for it.
        while !footer.is_empty() && footer.len() + 1 > height {
            let drop = [Part::Gap, Part::Status, Part::Text]
                .into_iter()
                .find_map(|wanted| footer.iter().position(|part| *part == wanted))
                .unwrap_or(0);
            footer.remove(drop);
        }
        let viewport = height.saturating_sub(footer.len());
        Self { viewport, footer }
    }
}

// ---------------------------------------------------------------------------
// laying entries out
// ---------------------------------------------------------------------------

/// `text` without the colour sequences in it.
fn unpainted(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while !rest.is_empty() {
        if let Some(len) = super::pacer::escape_at(rest) {
            rest = &rest[len..];
            continue;
        }
        let mut characters = rest.chars();
        if let Some(character) = characters.next() {
            out.push(character);
        }
        rest = characters.as_str();
    }
    out
}

/// Literal detail under a rail (`wrapLiteralToolOutput`, `assistant_wrap.zig:548-579`;
/// `appendTerminalSafeToolOutputInterruptible`, `full_transcript_screen.zig:6366-6411`):
/// every row `│` in the default colour, then `gutter` and the text in the dim
/// grey, wrapped to what the rail and the gutter leave.
///
/// `"  "` for an argument or a tool's result, `" "` for a command's own output
/// (`renderCommandOutputRecordWithPrimaryGutter`).
///
/// Wrapped by upstream's assistant wrap in its literal form
/// ([`super::prose::literal_drawn`]), as both of upstream's are: no word is left
/// alone on a line's last row, and no markdown is read into the text.
fn railed(text: &str, gutter: &str, cols: u16, palette: Palette) -> Vec<String> {
    let rail = 1 + wrap::width(gutter);
    let width = cols.saturating_sub(rail).max(1);
    let dim = palette.dim();
    let reset = palette.reset();
    let mut rows = Vec::new();
    for line in text.split('\n') {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            rows.push(clip(&format!("\u{2502}{dim}{gutter}{reset}"), cols).to_string());
            continue;
        }
        // A command's tab, as the spaces the wrap measured it to where it met
        // it -- a stop of the screen, the rail and the gutter included -- and
        // as many where the reflow moved it down with its word
        // (`wrapLiteralCommandOutput`, `assistant_wrap.zig:524-546@481104fd`;
        // [`super::prose::literal_drawn`]).
        for piece in super::prose::literal_drawn(line, width, rail) {
            let piece = piece.trim_end_matches([' ', '\n']);
            let piece = clip(piece, width);
            rows.push(format!("\u{2502}{dim}{gutter}{piece}{reset}"));
        }
    }
    rows
}

/// A tool's result as upstream's full transcript writes a stored one
/// (`appendStoredResultContent`, `full_transcript_screen.zig:5035-5070`,
/// through `appendWithSoftWrapPrefix`, `:3811-3870`): each line behind the
/// rail and the result's two-cell gutter, and a line too long for the screen
/// broken at its last column, mid-word, with the rail and gutter repeated --
/// not word by word as [`railed`] breaks an argument. A line with nothing on
/// it keeps its rail, as there.
fn literal(text: &str, cols: u16, palette: Palette) -> Vec<String> {
    use unicode_segmentation::UnicodeSegmentation;
    const GUTTER: &str = "  ";
    let room = cols.saturating_sub(1 + wrap::width(GUTTER)).max(1);
    let dim = palette.dim();
    let reset = palette.reset();
    let mut rows = Vec::new();
    for line in text.split('\n') {
        let line = line.trim_end_matches('\r');
        let mut piece = String::new();
        let mut used = 0u16;
        for cluster in line.graphemes(true) {
            let width = wrap::width(cluster);
            if used + width > room && !piece.is_empty() {
                rows.push(format!("\u{2502}{dim}{GUTTER}{piece}{reset}"));
                piece.clear();
                used = 0;
            }
            piece.push_str(cluster);
            used += width;
        }
        rows.push(format!("\u{2502}{dim}{GUTTER}{piece}{reset}"));
    }
    rows
}

/// A row written at another width, laid out at `cols` as upstream's row walker
/// lays out a static row (`ProjectionRowWalker.appendWithSoftWrapPrefix`,
/// `full_transcript_screen.zig:3968-4039@481104fd`): every character where it
/// is, and one whose cells would pass the last column on the next row, as a
/// terminal wraps it -- no indent, mid-word. The walker writes the row as one
/// stream, so what it has switched on is still on past the wrap; here each
/// row is a row of its own, so a row that runs on is closed and the next
/// reopens it (`super::pacer::SgrState`, the frame's own model of a row's
/// colours). A row that fits is the row as it is.
fn run_on(row: &str, cols: u16) -> Vec<String> {
    if wrap::width(row) <= cols || cols == 0 {
        return vec![row.to_string()];
    }
    let reset = "\u{1b}[0m";
    let mut rows = Vec::new();
    let mut pen = super::pacer::SgrState::default();
    let mut piece = String::new();
    let mut used = 0u16;
    let mut rest = row;
    while !rest.is_empty() {
        if let Some(len) = super::pacer::escape_at(rest) {
            pen.observe(&rest[..len]);
            piece.push_str(&rest[..len]);
            rest = &rest[len..];
            continue;
        }
        let Some(cluster) = wrap::first_cluster(rest) else {
            break;
        };
        rest = &rest[cluster.len()..];
        let width = wrap::cluster_width(cluster);
        if width > 0 && used + width > cols && used > 0 {
            piece.push_str(reset);
            rows.push(std::mem::take(&mut piece));
            piece.push_str(&pen.reopen());
            used = 0;
        }
        piece.push_str(cluster);
        used += width;
    }
    rows.push(piece);
    rows
}

/// A change as upstream's transcript shows one under its call
/// (`appendDiffRenderLine`, `diff.zig:1250-1276@481104fd`): `  │ N ± text`,
/// the changed lines in the diff grey and the rest in the dim one --
/// monochrome, as upstream's markers are until a theme is pinned
/// (`render.zig:93-105`).
///
/// **A line's text is a file's, and it is written as fx v0.0.16 writes it**
/// (9db199e4): read back from the permission boundary's spelling
/// ([`unspelled`]) and spelled again with fx's own forms
/// ([`terminal_safe_line`], `appendTerminalSafeLine`,
/// `text_utils.zig:419-442@481104fd`, from `diff.zig:1273@481104fd`), so the
/// row is fx's byte for byte and nothing in it is a byte a terminal acts on.
/// A tab is fx's too: fx hands the terminal the tab, which moves to the next
/// stop over cells it leaves unwritten; this screen places rows cell by cell,
/// so it writes those cells, unstyled, as the terminal leaves them
/// ([`walked`]).
///
/// **A line too long for the screen goes on under its gutter**, as fx's
/// Ctrl-O screen reflows it (`reflowDiffBlock`/`reflowDiffRow`,
/// `src/ui/render_engine/transcript_blocks.zig:720-797@481104fd`, for the
/// diff at `full_transcript_screen.zig:5917@481104fd`; the same at
/// c1db9191): each further row `  │ ` in the row's colour, blanks under the
/// number and the sign -- the widest number's columns, every number padded
/// to them as fx pads it (`appendDiffLineNumber`, `diff.zig:1316-1329`) --
/// the text in the column it started in ([`reflowed`] says where it is cut).
/// Then each row is laid out as fx's row walker lays out a static one
/// ([`walked`]): what still passes the margin -- a tab's cells, which the
/// reflow counted as none, or the prefix on a screen narrower than it --
/// runs on to the next row with no gutter, and a tab goes to the next stop
/// of the row it is on, run on or not. A row with no line number (the elided
/// run, a notice), or whose number leaves the screen no column, is not
/// reflowed (`diffPrefix`, `:659-695`; `:783`) and is only walked.
///
/// The review keeps xfx's own spelling (`approval_view::tamed` over
/// `bounded_diff_side`): it is the screen a change is decided on, and fx's
/// forms there would show a change of a carriage return as no change at all
/// (`scripts/fxcap/parity-ledger.toml`, `review-diff-spelling`).
fn diff_rows(diff: &ApprovalDiff, cols: u16, palette: Palette) -> Vec<String> {
    let review = Review::of_diff(diff);
    let width = review
        .lines
        .iter()
        .filter_map(|line| match line {
            ReviewLine::Context(number, _)
            | ReviewLine::Added(number, _)
            | ReviewLine::Removed(number, _) => Some(*number),
            _ => None,
        })
        .max()
        .unwrap_or(1)
        .to_string()
        .len();
    let reset = palette.reset();
    // `  │ `, the number, a space, the sign, a space.
    let start = 4 + width + 3;
    // The text's cells on a row, where the number leaves the screen any.
    let room = u16::try_from(start)
        .ok()
        .and_then(|start| cols.checked_sub(start))
        .filter(|room| *room > 0);
    // What stands in front of a continuation's text: the gutter, then
    // blanks where the number and the sign were.
    let under = format!("  \u{2502} {:1$}", "", start - 4);
    review
        .lines
        .iter()
        .flat_map(|line| {
            let file = |text: &str| terminal_safe_line(&unspelled(text));
            let (style, number, sign, text) = match line {
                ReviewLine::Context(number, text) => {
                    (palette.dim(), number.to_string(), ' ', file(text))
                }
                ReviewLine::Added(number, text) => {
                    (palette.diff(), number.to_string(), '+', file(text))
                }
                ReviewLine::Removed(number, text) => {
                    (palette.diff(), number.to_string(), '-', file(text))
                }
                ReviewLine::Elided(count) => (
                    palette.dim(),
                    String::new(),
                    ' ',
                    format!("{count} unchanged lines \u{22ef}"),
                ),
                ReviewLine::Notice(text) => {
                    (palette.dim(), String::new(), ' ', (*text).to_string())
                }
            };
            let pieces = match room {
                Some(room) if !number.is_empty() => reflowed(&text, room),
                _ => vec![text],
            };
            let head = format!("  \u{2502} {number:>width$} {sign} ");
            pieces
                .iter()
                .enumerate()
                .flat_map(|(index, piece)| {
                    let head = if index == 0 { &head } else { &under };
                    walked(&format!("{head}{piece}"), cols, style, reset)
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// One cell of a row [`walked`] lays out: a cluster, or a blank a tab moved
/// over, and the cells it takes.
struct WalkedCell {
    text: String,
    cells: u16,
    styled: bool,
}

impl WalkedCell {
    fn blank() -> Self {
        Self {
            text: " ".into(),
            cells: 1,
            styled: false,
        }
    }
}

/// One row of a change, `row` its prefix and a piece of its text, laid out
/// at `cols` as fx's row walker lays out a static row
/// (`ProjectionRowWalker.appendWithSoftWrapPrefix`,
/// `full_transcript_screen.zig:3972-4031@481104fd`, with no prefix, `:3848`),
/// a column at a time from the left edge:
///
/// * **A character whose cells would pass the last column** starts the next
///   row, at its column 1 (`shouldWrapAt`, `:4019-4022`).
/// * **What takes no cells** moves no column. It is mostly part of the
///   cluster before it ([`wrap::first_cluster`]), but a character that UAX
///   #29 counts as a control ends a grapheme cluster (GB4, GB5), so a mark
///   or a selector after one is a cluster of its own: after a tab, or after
///   one [`terminal_safe_line`] writes as itself, such as the soft hyphen
///   U+00AD. After such a character it is kept, a cell of no width after
///   that character's cell -- the same text the frame segments again and
///   measures the same way. **Right after a tab** it is dropped: fx writes
///   it where its column is, and its screen does not draw it, whether the
///   tab moved the column or not (`p5-transcript-diffwrap2` lines 15-19:
///   `…k1Z`, `…meZ`, `p      q` for a U+FE0F or a U+0301 after the tab;
///   `t́      u` for one before it). Kept, it would stand in the row text
///   right after the character before the tab, and the frame, which
///   segments the row again, would measure the two as one cluster (`1`
///   U+FE0F two cells) and cut the row's last cell.
/// * **A tab** moves to the next stop of eight of the row it is on -- a row
///   run on included, counted from that row's column -- and at most to the
///   last column (`nextTabStopColumn`, `display_width.zig:292-300@481104fd`,
///   at `:4004-4006`), over cells left blank and unstyled, as the terminal
///   leaves the cells fx's tab moves over. A tab never starts a row.
/// * **A tab on a row already full** moves back onto its last column
///   (`nextTabStopColumn` gives `cols` for a column at or past it), and the
///   next character, if it fits there, is written over the row's last cell
///   (`p5-transcript-diffwrap2`: `…cZ`, then `W`); over the right half of a
///   wide character, which leaves that character's left cell blank and
///   unstyled, as fx's screen shows it (`…i Z`). That character of the line
///   is not on Ctrl-O, as on fx; the review, the screen a change is decided
///   on, wraps the line whole (`approval_view`). A wide character after
///   such a tab does not fit and starts the next row, the last cell kept.
///
/// Every row opens in `style` and ends in `reset`; a tab's blanks are
/// written between `reset` and `style`.
fn walked(row: &str, cols: u16, style: &str, reset: &str) -> Vec<String> {
    if cols == 0 {
        return vec![format!("{style}{row}{reset}")];
    }
    let mut rows: Vec<Vec<WalkedCell>> = Vec::new();
    let mut cells: Vec<WalkedCell> = Vec::new();
    // fx's one-based column: where the next character goes.
    let mut column = 1u16;
    // Whether the last cluster was a tab, with no character after it yet.
    let mut after_tab = false;
    let mut rest = row;
    while let Some(cluster) = wrap::first_cluster(rest) {
        rest = &rest[cluster.len()..];
        if cluster == "\t" {
            let stop = if column >= cols {
                cols
            } else {
                ((column - 1) / 8 + 1)
                    .saturating_mul(8)
                    .saturating_add(1)
                    .min(cols)
            };
            cells.extend((column..stop).map(|_| WalkedCell::blank()));
            column = stop;
            after_tab = true;
            continue;
        }
        let width = wrap::cluster_width(cluster);
        if width == 0 {
            // A cluster of its own only after a character UAX #29 counts as
            // a control: dropped after a tab, kept after any other (U+00AD).
            if !after_tab {
                cells.push(WalkedCell {
                    text: cluster.into(),
                    cells: 0,
                    styled: true,
                });
            }
            continue;
        }
        after_tab = false;
        if u32::from(column) + u32::from(width) - 1 > u32::from(cols) {
            rows.push(std::mem::take(&mut cells));
            column = 1;
        }
        // What already stands at or past `column` -- only ever the row's last
        // cell, after a tab moved back onto it -- is written over; a wide
        // character half under it leaves its left cell blank.
        let mut end: u16 = cells.iter().map(|cell| cell.cells).sum();
        while end >= column {
            let Some(cell) = cells.pop() else { break };
            end -= cell.cells;
            cells.extend((end + 1..column).map(|_| WalkedCell::blank()));
            end = end.max(column - 1);
        }
        cells.push(WalkedCell {
            text: cluster.into(),
            cells: width,
            styled: true,
        });
        column = column.saturating_add(width);
    }
    rows.push(cells);
    rows.iter()
        .map(|cells| {
            let mut out = String::from(style);
            let mut blank = false;
            for cell in cells {
                if cell.styled == blank {
                    out.push_str(if blank { style } else { reset });
                    blank = !blank;
                }
                out.push_str(&cell.text);
            }
            if blank {
                out.push_str(style);
            }
            out.push_str(reset);
            out
        })
        .collect()
}

/// A line's text cut into the pieces fx's `reflowDiffRow` puts on a row each
/// (`transcript_blocks.zig:720-768@481104fd`), `room` the cells a row has
/// after the number and the sign.
///
/// * **By character, never by word**, before the first character whose cells
///   would pass the margin (`shouldWrapAt`, `display_width.zig:286-288`,
///   asked at `transcript_blocks.zig:757`): a wide one moves down whole and
///   leaves the last cell blank; one of no cells -- a mark joined to the
///   character before it -- is never moved (`:751-755`).
/// * **A tab is no cells** here, as fx counts every C0 control
///   (`displayUnitAt`, `display_width.zig:241-244`), though it is drawn
///   wider; the cells it takes are the row walker's to run on ([`walked`]),
///   as fx's are.
/// * **A character's cells are the screen's own measure** ([`wrap::width`],
///   which the frame places and clips a row by), a grapheme cluster at a
///   time, where fx counts with its `display_width` a unit at a time (an
///   emoji sequence, a character and its variation selector, else one code
///   point). They count apart for a skin-tone modifier after no emoji,
///   which fx counts as none and a terminal draws two cells wide, so fx's
///   row runs past the margin where this one breaks before it
///   (`parity-ledger.toml`'s `filler-modifier-count`); and for the scalars
///   `super::drawn_wider` lists, which this measure counts fewer cells than
///   fx and a terminal do, so a row holding one can still run past the
///   margin here as it could before (`ctrl-o-drawn-wider-raw`, owner
///   CTRLO-WIDER). They cut apart in a cluster of two characters fx counts a
///   cell or more each, conjoining jamo (`U+1100 U+1161`), which fx may cut
///   between them where this keeps the cluster whole.
///
/// Nothing in `text` is a byte a terminal acts on ([`terminal_safe_line`]
/// spelled every one), so a spelling such as `\x1b` or `\u{202e}` is cut
/// where the margin falls like any other text, as fx's is: each piece of it
/// printable ASCII.
fn reflowed(text: &str, room: u16) -> Vec<String> {
    let mut pieces = Vec::new();
    let mut piece = String::new();
    let mut used = 0u16;
    let mut rest = text;
    while let Some(cluster) = wrap::first_cluster(rest) {
        rest = &rest[cluster.len()..];
        let cells = if cluster == "\t" {
            0
        } else {
            wrap::cluster_width(cluster)
        };
        if cells > 0 && used.saturating_add(cells) > room {
            pieces.push(std::mem::take(&mut piece));
            used = 0;
        }
        piece.push_str(cluster);
        used = used.saturating_add(cells);
    }
    pieces.push(piece);
    pieces
}

/// A line of a change as the permission boundary spelled it
/// (`crate::permission::bounded_diff_side`), read back to the file's own text:
/// `\\` a backslash, `\t` a tab, `\r` a carriage return, `\u{…}` the code point
/// it names. The boundary spends a backslash on every name and doubles the
/// file's own, so each backslash here begins one of those; a backslash that
/// begins none of them (text that did not come from the boundary: a log
/// written by hand) is kept as it is.
///
/// What this returns is never written as it is: [`terminal_safe_line`] spells
/// every byte of it a terminal would act on.
fn unspelled(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(at) = rest.find('\\') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let after = &rest[1..];
        let (character, len) = if after.starts_with('\\') {
            (Some('\\'), 2)
        } else if after.starts_with('t') {
            (Some('\t'), 2)
        } else if after.starts_with('r') {
            (Some('\r'), 2)
        } else {
            match after
                .strip_prefix("u{")
                .and_then(|digits| digits.split_once('}'))
                .filter(|(hex, _)| {
                    (1..=6).contains(&hex.len()) && hex.bytes().all(|b| b.is_ascii_hexdigit())
                })
                .and_then(|(hex, _)| {
                    u32::from_str_radix(hex, 16)
                        .ok()
                        .map(|code| (code, hex.len()))
                })
                .and_then(|(code, digits)| char::from_u32(code).map(|c| (c, digits)))
            {
                Some((character, digits)) => (Some(character), 1 + 2 + digits + 1),
                None => (None, 1),
            }
        };
        match character {
            Some(character) => out.push(character),
            None => out.push('\\'),
        }
        rest = &rest[len..];
    }
    out.push_str(rest);
    out
}

/// One line of a file, ready to draw, as fx v0.0.16 writes one
/// (`appendTerminalSafeLine`, `src/core/shared/text_utils.zig:419-442@481104fd`):
///
/// * a carriage return that ends the line is dropped (`:429`);
/// * a tab stays (`:432-436`);
/// * every other C0 control and `DEL` is `\xNN`, two lower-case hex digits
///   (`terminalSafeToken`, `:645-646`; `writeByteEscape`, `:675-686`);
/// * a C1 control, a zero-width or directional mark, a line or paragraph
///   separator, an embedding, an override, an isolate, an invisible operator
///   and the byte-order mark are `\u{NNNN}`, at least four lower-case hex
///   digits (`isNonPrintingCodepoint`, `:703-708`; `writeCodepointEscape`,
///   `:688-701`) -- and so is the Arabic letter mark (`U+061C`), which fx
///   writes as itself: it reorders the text around it as the marks fx names
///   do (`approval_view`'s `reorders`), so it is named here too
///   (`scripts/fxcap/parity-ledger.toml`, `ctrl-o-arabic-letter-mark`);
/// * everything else is itself (`:666`).
///
/// fx also writes `\xNN` for each byte outside a valid UTF-8 sequence
/// (`:652-662`); a line here is a `str`, so it has none -- a file that is not
/// UTF-8 is never edited (`crate::tools::mutate`), and a write's review of one
/// says so in words ([`crate::permission::non_text_summary`]).
fn terminal_safe_line(line: &str) -> String {
    use std::fmt::Write as _;
    let line = line.strip_suffix('\r').unwrap_or(line);
    let mut out = String::with_capacity(line.len());
    for character in line.chars() {
        let code = u32::from(character);
        if character == '\t' {
            out.push('\t');
        } else if code < 0x20 || code == 0x7f {
            let _ = write!(out, "\\x{code:02x}");
        } else if fx_names(code) || character == '\u{061c}' {
            let _ = write!(out, "\\u{{{code:04x}}}");
        } else {
            out.push(character);
        }
    }
    out
}

/// fx's `isNonPrintingCodepoint` (`text_utils.zig:703-708@481104fd`).
fn fx_names(code: u32) -> bool {
    (0x80..=0x9f).contains(&code)
        || (0x200b..=0x200f).contains(&code)
        || (0x2028..=0x202e).contains(&code)
        || (0x2060..=0x206f).contains(&code)
        || code == 0xfeff
}

/// Everything `transcript` has queued, laid out as the rows a terminal would
/// be left holding (`super::shell`'s `trailer_rows`): each append rewrites the
/// rows it does not scroll in, at the bottom of what is already there.
fn drained(mut transcript: Transcript, palette: Palette) -> Vec<String> {
    let mut rows: Vec<String> = Vec::new();
    while transcript
        .emit_front(palette, |append| {
            let settled = append.rows.len() - append.scroll.min(append.rows.len());
            rows.truncate(rows.len().saturating_sub(settled));
            rows.extend(append.rows.iter().cloned());
            Landed::<std::convert::Infallible>::All
        })
        .is_some()
    {}
    // An entry's blank rows above and below are the screen's to give it, one
    // between each two entries, as upstream's projection does.
    while rows.last().is_some_and(String::is_empty) {
        rows.pop();
    }
    let leading = rows.iter().take_while(|row| row.is_empty()).count();
    rows.drain(..leading);
    rows
}

/// The card: the rail and the text in bold, as the document has it
/// (`super::shell`'s `write_card`).
fn card(text: &str, cols: u16, palette: Palette) -> Vec<String> {
    const BOLD: &str = "\u{1b}[1m";
    const RESET: &str = "\u{1b}[0m";
    let mut transcript = Transcript::new(cols);
    transcript.queue_gutter(Gutter::Rail);
    let card = text
        .split('\n')
        .map(|line| format!("{BOLD}{line}{RESET}"))
        .collect::<Vec<_>>()
        .join("\n");
    transcript.queue_push(&card, Role::Plain);
    transcript.queue_end_line();
    transcript.queue_gutter(Gutter::None);
    drained(transcript, palette)
}

/// One whole line in `role`, as the document has it.
fn line(text: &str, role: Role, cols: u16, palette: Palette) -> Vec<String> {
    let mut transcript = Transcript::new(cols);
    super::shell::queue_line(&mut transcript, text, role);
    drained(transcript, palette)
}

fn notice_rows(notice: &Notice, cols: u16, palette: Palette) -> Vec<String> {
    let mut transcript = Transcript::new(cols);
    super::shell::queue_notice(&mut transcript, notice);
    drained(transcript, palette)
}

/// What the presenter completed, laid out as the document lays it out: prose
/// in the two-column gutter, code, tables and rules as blocks of their own
/// (`super::shell`'s `present`, `write_prose` and `write_structure`).
fn answer(events: &[markdown::Event], cols: u16, palette: Palette) -> Vec<String> {
    let mut writer = Prose {
        transcript: Transcript::new(cols),
        open: false,
        held: 0,
        cols,
        light: palette.mode == Mode::Light,
    };
    for event in events {
        writer.present(event.clone());
    }
    writer.close();
    drained(writer.transcript, palette)
}

/// The answer's writer, on a transcript of its own.
struct Prose {
    transcript: Transcript,
    open: bool,
    held: usize,
    cols: u16,
    light: bool,
}

impl Prose {
    fn present(&mut self, event: markdown::Event) {
        match event {
            markdown::Event::Text(text) => self.prose(&text),
            markdown::Event::CodeBlock(block) => {
                let theme = markdown::theme::builtin(self.light);
                let rows = blocks::code_block_row_bytes(&block, self.cols, &theme);
                self.structure(&rows);
            }
            markdown::Event::Table(table) => {
                let rows = blocks::table_row_bytes(&table, self.cols);
                self.structure(&rows);
            }
            markdown::Event::ThematicRule => {
                let rule = blocks::render_thematic_rule(self.cols);
                self.structure(&rule);
            }
        }
    }

    fn prose(&mut self, text: &str) {
        let mut text = text;
        if !self.open {
            text = text.trim_start_matches('\n');
            if text.is_empty() {
                return;
            }
            self.finish_line();
            self.transcript.queue_gutter(Gutter::Indent);
            self.open = true;
        }
        let body = text.trim_end_matches('\n');
        if !body.is_empty() {
            if self.held > 0 {
                let breaks = "\n".repeat(self.held);
                self.held = 0;
                self.transcript.queue_push(&breaks, Role::Plain);
            }
            self.transcript.queue_push(body, Role::Plain);
        }
        self.held += text.len() - body.len();
    }

    fn close(&mut self) {
        if !self.open {
            return;
        }
        self.open = false;
        self.held = 0;
        self.finish_line();
        self.transcript.queue_gutter(Gutter::None);
        self.transcript.queue_end_line();
    }

    fn structure(&mut self, rows: &str) {
        self.close();
        self.finish_line();
        for row in rows.trim_end_matches('\n').split('\n') {
            self.transcript.queue_push(row, Role::Plain);
            self.transcript.queue_end_line();
        }
        self.transcript.queue_end_line();
    }

    fn finish_line(&mut self) {
        if self.transcript.open_line() {
            self.transcript.queue_end_line();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::tools::{read, terminal};
    use serde_json::json;

    const DARK: Palette = Palette { mode: Mode::Dark };

    /// 2026-10-06 14:43:50.620 UTC, the moment the capture's second entry is
    /// stamped with (`fx-tools-single-40x120-full`).
    const AT: i64 = 1_791_297_830_620;

    fn fixed() -> i64 {
        AT
    }

    fn record() -> FullTranscript {
        FullTranscript::with_clock(fixed)
    }

    /// Upstream's own cases (`transcript_blocks.zig:3220-3247`): a body row
    /// that opens with a tree branch stands under the notice's glyph, where
    /// every other row is two columns in, and the branch's own wrapped rows
    /// are two in again (`noticeContinuationIndent`, `:1718-1725`) -- the
    /// shape of a skill load notice (`fx-skills-40x120-sent`).
    #[test]
    fn a_notice_rows_tree_branch_stands_under_its_glyph() {
        for (body, cols, expected) in [
            (
                "2 skills loaded\n\u{251c} Loaded alpha\n\u{2514} Loaded beta",
                40,
                &[
                    "* 2 skills loaded",
                    "\u{251c} Loaded alpha",
                    "\u{2514} Loaded beta",
                ][..],
            ),
            (
                "Ready\n\u{2514} alpha beta gamma",
                12,
                &["* Ready", "\u{2514} alpha beta", "  gamma"][..],
            ),
            (
                "alpha \u{2514} beta",
                8,
                &["* alpha", "  \u{2514} beta"][..],
            ),
            (
                "Ready\nordinary prose",
                40,
                &["* Ready", "  ordinary prose"][..],
            ),
            (
                "Ready\n\n\u{2514} after a blank\nthen prose",
                40,
                &["* Ready", "", "\u{2514} after a blank", "  then prose"][..],
            ),
        ] {
            let rows = notice_rows(&Notice::neutral("", body), cols, DARK);
            assert_eq!(plain(&rows), expected, "{body:?} at {cols}");
        }
    }

    fn plain(rows: &[String]) -> Vec<String> {
        rows.iter().map(|row| unpainted(row)).collect()
    }

    fn command(text: &str) -> Shown {
        terminal::TERMINAL.shown(&json!({"action": "exec", "command": text}))
    }

    fn read(path: &str) -> Shown {
        read::READ_FILE.shown(&json!({ "path": path }))
    }

    /// The whole screen with no colour, at 40x120 under `auto · model-a`.
    fn screen(record: &FullTranscript) -> Vec<String> {
        plain(&record.rows(120, 40, DARK, "auto \u{b7} model-a"))
    }

    #[test]
    fn a_header_is_upstreams_local_stamp_and_the_entrys_kind() {
        // `full_transcript_metadata.zig:40-52@481104fd`, the same instant: a
        // test build reads its zone from `local_time`'s test zone, UTC.
        assert_eq!(
            stamp(1_704_164_645_006).as_deref(),
            Some("2024-01-02 03:04:05.006 UTC")
        );
        assert_eq!(stamp(AT).as_deref(), Some("2026-10-06 14:43:50.620 UTC"));
        // Leap day and the last moment upstream prints.
        assert_eq!(
            stamp(951_782_400_000).as_deref(),
            Some("2000-02-29 00:00:00.000 UTC")
        );
        assert_eq!(
            stamp(253_402_300_799_999).as_deref(),
            Some("9999-12-31 23:59:59.999 UTC")
        );
        // No header at all for a moment upstream refuses to print.
        assert_eq!(stamp(0), None);
        assert_eq!(stamp(-1), None);
        assert_eq!(stamp(253_402_300_800_000), None);
        // West of UTC, the day before; and a zone with no plain name.
        let est = crate::local_time::Zone {
            offset_s: -5 * 3600,
            name: b"EST",
        };
        crate::local_time::with_test_zone(Some(est), || {
            assert_eq!(
                stamp(1_704_164_645_006).as_deref(),
                Some("2024-01-01 22:04:05.006 EST")
            );
        });
        let unnamed = crate::local_time::Zone {
            offset_s: 5 * 3600 + 45 * 60,
            name: b"\x1b]0;x\x07",
        };
        crate::local_time::with_test_zone(Some(unnamed), || {
            assert_eq!(
                stamp(1_704_164_645_006).as_deref(),
                Some("2024-01-02 08:49:05.006 +0545")
            );
        });
    }

    #[test]
    fn each_entry_sits_under_its_header_with_one_blank_row_between_entries() {
        let mut record = record();
        record.welcome();
        record.prompt("run the loop");
        record.answer("Now a missing file.\n");
        record.usage("  5s (\u{2191}3 \u{2193}200)");
        let rows = screen(&record);
        let text: Vec<&str> = rows.iter().map(String::as_str).collect();
        assert!(text[0].starts_with("x\u{1d487}x v"), "{text:?}");
        assert_eq!(
            &text[1..13],
            [
                "",
                "  2026-10-06 14:43:50.620 UTC \u{b7} Prompt",
                "\u{2503} run the loop",
                "",
                "  2026-10-06 14:43:50.620 UTC \u{b7} Response",
                "  Now a missing file.",
                "",
                "  2026-10-06 14:43:50.620 UTC \u{b7} Usage",
                "  5s (\u{2191}3 \u{2193}200)",
                "",
                "",
                "",
            ]
        );
    }

    #[test]
    fn the_footer_is_upstreams_four_rows_under_the_record() {
        // `fx-tools-single-40x120-full` rows 37-40, and their SGR.
        let rows = record().rows(120, 40, DARK, "STATUS");
        assert_eq!(rows.len(), 40);
        assert_eq!(rows[36], "");
        assert_eq!(
            rows[37],
            "\u{1b}[38;5;255m\u{2503}\u{1b}[0m \u{1b}[38;5;245mfull detail \u{b7} ctrl+o close \u{b7} pgup/pgdn scroll \u{b7} esc close\u{1b}[0m"
        );
        assert_eq!(rows[38], "");
        assert_eq!(rows[39], "STATUS");
        // The caret rests on the blank row under the text.
        assert_eq!(record().caret(40), (39, 0));
    }

    #[test]
    fn a_short_screen_gives_up_the_footers_rows_before_the_record_and_never_overflows() {
        for (height, footer) in [
            (5u16, vec![Part::Gap, Part::Text, Part::Gap, Part::Status]),
            (4, vec![Part::Text, Part::Gap, Part::Status]),
            (3, vec![Part::Text, Part::Status]),
            (2, vec![Part::Text]),
            (1, vec![]),
        ] {
            let layout = Layout::of(height);
            assert_eq!(layout.footer, footer, "{height} rows");
            assert!(layout.viewport >= 1, "{height} rows left no record");
            let mut record = record();
            record.prompt("a prompt long enough to wrap on a narrow screen of twenty cells");
            let rows = record.rows(20, height, DARK, "status");
            assert_eq!(rows.len(), usize::from(height));
            for row in &rows {
                assert!(wrap::width(row) <= 20, "{row:?}");
            }
            let (caret, _) = record.caret(height);
            assert!((1..=height).contains(&caret));
        }
    }

    #[test]
    fn header_rows_are_dim_and_detail_rows_open_on_a_plain_rail() {
        // `fx-tools-single-40x120-full.ansi` rows 10, 20 and 22.
        let mut record = record();
        record.tool_started("c1", &command("echo boom; echo err >&2; exit 3"));
        record.observe(&UiEvent::ToolDetail {
            call_id: "c1".into(),
            detail: Detail::Command {
                output: "boom\nerr".into(),
                status: Some("exit code 3".into()),
            },
        });
        record.tool_finished("c1", true, "", Outcome::Exited(3), None);
        let rows = record.rows(120, 40, DARK, "");
        assert_eq!(
            rows[0],
            "\u{1b}[38;5;245m  2026-10-06 14:43:50.620 UTC \u{b7} Tool\u{1b}[0m"
        );
        assert!(rows.contains(&"\u{2502}\u{1b}[38;5;245m boom\u{1b}[0m".to_string()));
        assert!(rows.contains(&"\u{2502}\u{1b}[38;5;245m exit code 3\u{1b}[0m".to_string()));
    }

    #[test]
    fn a_calls_row_is_the_groups_in_the_default_foreground_with_its_command_words_coloured() {
        // `fx-fulltranscript-40x120-full.ansi` row 13 against `-end.ansi` row
        // 9: the inline row is the statusline grey throughout, the full
        // screen's is the default foreground with the shell's words in 252.
        let mut record = record();
        record.tool_started("c", &command("echo boom; exit 3"));
        record.tool_finished("c", true, "", Outcome::Exited(3), None);
        let rows = record.rows(120, 40, DARK, "");
        assert_eq!(
            rows[1],
            "\u{1b}[38;5;255m\u{25cf}\u{1b}[0m \u{1b}[38;5;245m1 tool call \u{b7} 1 command \u{b7} 1 failed\u{1b}[0m"
        );
        assert_eq!(
            rows[2],
            "\u{2514} Exited 3 \u{1b}[38;5;252mecho\u{1b}[39m boom\u{1b}[38;5;252m;\u{1b}[39m \u{1b}[38;5;252mexit\u{1b}[39m 3\u{1b}[0m"
        );
    }

    #[test]
    fn a_group_shows_its_header_over_its_first_call_and_each_call_under_its_own_header() {
        // `fx-tools-single-40x120-full` rows 13-30.
        let mut record = record();
        record.tool_started("c2", &read("missing.txt"));
        record.tool_finished(
            "c2",
            false,
            "Path not found: missing.txt",
            Outcome::Finished,
            None,
        );
        record.observe(&UiEvent::ToolDetail {
            call_id: "c2".into(),
            detail: Detail::Result("Path not found: missing.txt".into()),
        });
        record.tool_started("c3", &command("echo boom; echo err >&2; exit 3"));
        record.observe(&UiEvent::ToolDetail {
            call_id: "c3".into(),
            detail: Detail::Command {
                output: "boom\nerr".into(),
                status: Some("exit code 3".into()),
            },
        });
        record.tool_finished("c3", true, "", Outcome::Exited(3), None);
        record.tool_started("c4", &read("a.txt"));
        record.observe(&UiEvent::ToolDetail {
            call_id: "c4".into(),
            detail: result(
                "read_file",
                "<path>a.txt</path>\n<content>\n1\talpha line 1\n2\talpha line 2\n</content>",
            ),
        });
        record.tool_finished("c4", true, "", Outcome::Finished, None);
        let rows = screen(&record);
        assert_eq!(
            &rows[..17],
            [
                "  2026-10-06 14:43:50.620 UTC \u{b7} Tool",
                "\u{25cf} 3 tool calls \u{b7} 2 read \u{b7} 1 command \u{b7} 2 failed",
                "\u{251c} Failed missing.txt: Path not found: missing.txt",
                "\u{2502}  Path not found: missing.txt",
                "",
                "  2026-10-06 14:43:50.620 UTC \u{b7} Tool",
                "\u{251c} Exited 3 echo boom; echo err >&2; exit 3",
                "\u{2502} boom",
                "\u{2502} err",
                "\u{2502} exit code 3",
                "",
                "  2026-10-06 14:43:50.620 UTC \u{b7} Tool",
                "\u{2514} Read a.txt",
                "\u{2502}  1    alpha line 1",
                "\u{2502}  2    alpha line 2",
                "",
                "",
            ]
        );
    }

    #[test]
    fn arguments_the_row_already_shows_are_left_out_and_the_rest_listed_in_schema_order() {
        // `fx-approvals-cc-40x120-full` rows 13-16: a denied edit lists what it
        // was asked to do, under a row that names the file only as `file`.
        let input = json!({
            "new_string": "ALPHA LINE TWO",
            "old_string": "alpha line 2",
            "path": "a.txt"
        });
        let mut record = record();
        record.observe(&UiEvent::ToolDetail {
            call_id: "e1".into(),
            detail: Detail::Arguments(arguments("edit_file", &input)),
        });
        record.tool_started("e1", &crate::tools::mutate::EDIT_FILE.shown(&input));
        record.tool_finished("e1", false, "declined", Outcome::Denied, None);
        let rows = screen(&record);
        assert_eq!(
            &rows[1..6],
            [
                "\u{25cf} 1 tool call \u{b7} 1 edit \u{b7} 1 denied",
                "\u{2514} Denied file",
                "\u{2502}  path: a.txt",
                "\u{2502}  old_string: alpha line 2",
                "\u{2502}  new_string: ALPHA LINE TWO",
            ]
        );

        // A command's row names the command, and `exec` is the only action
        // `terminal` has: neither is listed again, and nothing else is left.
        let mut record = self::record();
        let input = json!({"action": "exec", "command": "ls"});
        record.observe(&UiEvent::ToolDetail {
            call_id: "t".into(),
            detail: Detail::Arguments(arguments("terminal", &input)),
        });
        record.tool_started("t", &command("ls"));
        record.tool_finished("t", true, "", Outcome::Exited(0), None);
        let rows = screen(&record);
        assert_eq!(rows[2], "\u{2514} Ran ls");
        assert_eq!(rows[3], "");

        // Upstream's own rule, and only it: every argument but the captured
        // command's `exec` and a value the row already shows
        // (`full_transcript_screen.zig:6498-6511`; `exec` is the captured
        // `terminal` action, `captured_command.zig:26`). Its `shell` lists
        // `action: run` and `yield_time_ms` by the same rule
        // (`fx-resume-tools-40x120-live-ctrl-o`); `terminal`'s one other
        // field is `cwd`, listed when given.
        let mut record = self::record();
        let input = json!({"action": "exec", "command": "ls", "cwd": "sub"});
        record.observe(&UiEvent::ToolDetail {
            call_id: "t".into(),
            detail: Detail::Arguments(arguments("terminal", &input)),
        });
        record.tool_started("t", &command("ls"));
        record.tool_finished("t", true, "", Outcome::Exited(0), None);
        let rows = screen(&record);
        assert_eq!(&rows[2..5], ["\u{2514} Ran ls", "\u{2502}  cwd: sub", ""]);
    }

    #[test]
    fn a_result_wraps_at_the_screens_edge_by_character_as_upstreams_stored_result_does() {
        // `fx-read-failures-40x120-ctrl-o`: the envelope's rows are 120 cells,
        // broken mid-word (`…"message":"read_file requires a regular
        // file","detai` / `│  ls":{…`): a stored result is written through
        // the row walker, which breaks at the screen's last column and
        // repeats the rail (`appendWithSoftWrapPrefix`,
        // `full_transcript_screen.zig:3811-3870`), not word by word as an
        // argument is.
        let mut record = record();
        let input = json!({"path": "src"});
        record.tool_started("r", &read("src"));
        record.observe(&UiEvent::ToolDetail {
            call_id: "r".into(),
            detail: Detail::Arguments(arguments("read_file", &input)),
        });
        let text = format!("{} {}", "a".repeat(100), "b".repeat(30));
        record.observe(&UiEvent::ToolDetail {
            call_id: "r".into(),
            detail: result("read_file", &text),
        });
        record.tool_finished("r", false, "", Outcome::Finished, None);
        let rows = screen(&record);
        let at = rows
            .iter()
            .position(|row| row.ends_with("Failed src"))
            .expect("the row");
        assert_eq!(
            &rows[at + 1..at + 3],
            [
                format!("\u{2502}  {} {}", "a".repeat(100), "b".repeat(16)),
                format!("\u{2502}  {}", "b".repeat(14)),
            ]
        );
    }

    #[test]
    fn a_trailing_line_break_is_no_rail_row_an_argument_drops_it_and_a_result_leaves_a_blank() {
        // `fx-resume-tools-40x120-ctrl-o` / `-live-ctrl-o-up1` (D1 receipts):
        // a refused write's `content: new file\nsecond line\n` ends on its
        // second line, and a glob's `…\n - src/main.rs\n` on a blank row with
        // no rail before the next entry's gap. An argument is written without
        // its one trailing break (`appendTerminalSafeToolOutputInterruptible`,
        // `full_transcript_screen.zig:6381-6383`); a result's ends its last
        // row, and the line the detail closes with after it is that blank
        // (`appendDetailContent`, `:6618-6642`). A break inside either keeps
        // its rail.
        let input = json!({"path": "c.txt", "content": "new file\n\nsecond line\n"});
        let mut record = record();
        record.observe(&UiEvent::ToolDetail {
            call_id: "w".into(),
            detail: Detail::Arguments(arguments("write_file", &input)),
        });
        record.tool_started("w", &crate::tools::mutate::WRITE_FILE.shown(&input));
        record.tool_finished("w", false, "declined", Outcome::Denied, None);
        let pattern = json!({"pattern": "**/*.rs"});
        record.observe(&UiEvent::ToolDetail {
            call_id: "g".into(),
            detail: Detail::Arguments(arguments("glob_files", &pattern)),
        });
        record.tool_started("g", &read::GLOB_FILES.shown(&pattern));
        record.observe(&UiEvent::ToolDetail {
            call_id: "g".into(),
            detail: result(
                "glob_files",
                "[glob] 1 matches for **/*.rs\n\n - src/main.rs\n",
            ),
        });
        record.tool_finished("g", true, "", Outcome::Finished, None);
        let rows = screen(&record);
        let denied = rows
            .iter()
            .position(|row| row.ends_with("Denied file"))
            .expect("the refused write");
        assert_eq!(
            &rows[denied..denied + 6],
            [
                "\u{251c} Denied file",
                "\u{2502}  path: c.txt",
                "\u{2502}  content: new file",
                "\u{2502}  ",
                "\u{2502}  second line",
                "",
            ]
        );
        let matched = rows
            .iter()
            .position(|row| row.ends_with("Matched **/*.rs"))
            .expect("the glob");
        assert_eq!(
            &rows[matched..matched + 6],
            [
                "\u{2514} Matched **/*.rs",
                "\u{2502}  [glob] 1 matches for **/*.rs",
                "\u{2502}  ",
                "\u{2502}   - src/main.rs",
                "",
                "",
            ]
        );
    }

    #[test]
    fn a_change_is_shown_as_upstreams_diff_block_instead_of_its_arguments_with_no_review() {
        // `fx-approvals-cc-40x120-full` rows 19-21: the row, a blank row, and
        // `  │ 1 + new file` in the diff grey -- here for a change no approval
        // reviewed, since the change arrives from the tool that made it.
        let input = json!({"path": "c.txt", "content": "new file\n"});
        let mut record = record();
        record.observe(&UiEvent::ToolDetail {
            call_id: "w".into(),
            detail: Detail::Arguments(arguments("write_file", &input)),
        });
        record.tool_started("w", &crate::tools::mutate::WRITE_FILE.shown(&input));
        record.tool_finished(
            "w",
            true,
            "",
            Outcome::Finished,
            Some(LineChanges {
                added: 1,
                removed: 0,
            }),
        );
        record.observe(&UiEvent::ToolDetail {
            call_id: "w".into(),
            detail: Detail::Result("Wrote c.txt".into()),
        });
        let before = plain(&record.rows(120, 40, DARK, ""));
        assert!(
            before.iter().any(|row| row.contains("content: new file")),
            "until the change arrives the arguments say what was asked: {before:?}"
        );
        record.observe(&UiEvent::ToolDetail {
            call_id: "w".into(),
            detail: Detail::Change(ApprovalDiff::of("", "new file\n")),
        });
        let rows = record.rows(120, 40, DARK, "");
        let plain = plain(&rows);
        assert_eq!(
            &plain[2..5],
            ["\u{2514} Wrote c.txt +1", "", "  \u{2502} 1 + new file"]
        );
        assert_eq!(rows[4], "\u{1b}[38;5;252m  \u{2502} 1 + new file\u{1b}[0m");
        assert!(
            !plain.iter().any(|row| row.contains("content:")),
            "the arguments were listed beside the diff that covers them: {plain:?}"
        );
    }

    #[test]
    fn a_changes_rows_write_a_files_bytes_as_fx_does_and_hand_the_terminal_none() {
        // fx v0.0.16's 9db199e4: a diff row's text goes through
        // `appendTerminalSafeLine` (`text_utils.zig:419-442@481104fd`, from
        // `diff.zig:1273`). The permission boundary spelled the sides its own
        // way (`bounded_diff_side`); the row is fx's.
        let diff = ApprovalDiff::of(
            "a\n",
            "bidi \u{202e} x\nzero\u{200b}width \u{feff} bom\nesc \u{1b}[2J c1 \u{9b}\ntab\there\ncrlf\r\nlit \\x1b\n",
        );
        let rows = diff_rows(&diff, 120, DARK);
        // Written out rather than read from `fx_names`, so a change there
        // cannot pass itself.
        let acted_on = |c: char| {
            c.is_control()
                || matches!(c, '\u{200b}'..='\u{200f}' | '\u{2028}'..='\u{202e}' | '\u{2060}'..='\u{206f}' | '\u{feff}' | '\u{61c}')
        };
        // Only the palette's own colours are taken out, so a raw ESC -- and
        // the sequence it begins -- is still there to be seen (`plain` would
        // strip it with every other sequence).
        for row in &rows {
            let bare = [DARK.dim(), DARK.diff(), DARK.reset()]
                .iter()
                .fold(row.clone(), |row, sgr| row.replace(sgr, ""));
            assert!(
                !bare.chars().any(acted_on),
                "a row hands the terminal a character it acts on: {bare:?}"
            );
        }
        assert_eq!(
            rows,
            [
                "\u{1b}[38;5;252m  \u{2502} 1 - a\u{1b}[0m",
                "\u{1b}[38;5;252m  \u{2502} 1 + bidi \\u{202e} x\u{1b}[0m",
                "\u{1b}[38;5;252m  \u{2502} 2 + zero\\u{200b}width \\u{feff} bom\u{1b}[0m",
                "\u{1b}[38;5;252m  \u{2502} 3 + esc \\x1b[2J c1 \\u{009b}\u{1b}[0m",
                // The tab's cells, unstyled, up to column 16, as a terminal
                // leaves the cells fx's tab moves over.
                "\u{1b}[38;5;252m  \u{2502} 4 + tab\u{1b}[0m     \u{1b}[38;5;252mhere\u{1b}[0m",
                "\u{1b}[38;5;252m  \u{2502} 5 + crlf\u{1b}[0m",
                "\u{1b}[38;5;252m  \u{2502} 6 + lit \\x1b\u{1b}[0m",
            ]
        );
        // The review keeps xfx's own spelling: it is the screen a change is
        // decided on (parity-ledger.toml `review-diff-spelling`).
        let reviewed: Vec<String> = Review::of_diff(&diff)
            .rows(120, &super::super::approval_view::Styles::PLAIN)
            .into_iter()
            .map(|(row, _)| row.trim_start().to_string())
            .collect();
        assert_eq!(reviewed[1], "1 + bidi \\u{202E} x");
        assert_eq!(reviewed[5], "5 + crlf\\r");
        assert_eq!(reviewed[6], "6 + lit \\\\x1b");
    }

    /// One added line's rows at `cols`.
    fn added(line: &str, cols: u16) -> Vec<String> {
        diff_rows(&ApprovalDiff::of("", &format!("{line}\n")), cols, DARK)
    }

    /// `row` in the diff grey with the reset after it, as [`diff_rows`]
    /// writes a changed line with nothing in it to draw unstyled.
    fn diff_grey(row: &str) -> String {
        format!("{}{row}{}", DARK.diff(), DARK.reset())
    }

    #[test]
    fn a_changes_line_wider_than_the_screen_goes_on_under_its_gutter_as_fx_reflows_it() {
        // fx's `reflowDiffRow` (`transcript_blocks.zig:720-768@481104fd`,
        // from `reflowDiffBlock`, `:772-797`, for the Ctrl-O screen's diff at
        // `full_transcript_screen.zig:5917@481104fd`): the rest of a line too
        // long for the screen goes on to a row of its own, `  │ ` in the
        // row's colour and blanks under the number and the sign, the text in
        // the same column -- `  │     5678` in `p5-transcript-diffwide`'s
        // `full`. 108 `w`s and `TAIL` are the 112 cells the first row has.
        let w108 = "w".repeat(108);
        assert_eq!(
            added(&format!("{w108}TAIL5678"), 120),
            [
                diff_grey(&format!("  \u{2502} 1 + {w108}TAIL")),
                diff_grey("  \u{2502}     5678"),
            ]
        );
        // A line that reaches the last column is one row, as it was.
        assert_eq!(
            added(&format!("{w108}TAIL"), 120),
            [diff_grey(&format!("  \u{2502} 1 + {w108}TAIL"))]
        );
        // One cell more is one cell on the next row.
        assert_eq!(
            added(&format!("{w108}TAILZ"), 120),
            [
                diff_grey(&format!("  \u{2502} 1 + {w108}TAIL")),
                diff_grey("  \u{2502}     Z"),
            ]
        );
        // A line past two rows goes on to a third; a context line keeps the
        // dim grey on each, and a wider number column widens the blanks.
        let long = "x".repeat(250);
        let before: String = (1..=9).map(|n| format!("{n}\n")).collect();
        let rows = diff_rows(
            &ApprovalDiff::of(
                &format!("{before}{long}\nold\n"),
                &format!("{before}{long}\nnew\n"),
            ),
            120,
            DARK,
        );
        let context: Vec<&String> = rows.iter().filter(|row| row.contains("xx")).collect();
        let dim = |row: String| format!("{}{row}{}", DARK.dim(), DARK.reset());
        assert_eq!(
            context,
            [
                &dim(format!("  \u{2502} 10   {}", "x".repeat(111))),
                &dim(format!("  \u{2502}      {}", "x".repeat(111))),
                &dim(format!("  \u{2502}      {}", "x".repeat(28))),
            ],
            "{rows:?}"
        );
    }

    #[test]
    fn a_wide_character_at_the_margin_moves_down_whole_and_leaves_the_last_cell_blank() {
        // `shouldWrapAt` (`display_width.zig:286-288@481104fd`): a character
        // whose cells would pass the last column starts the next row, as
        // `reflowDiffRow` asks before each one (`transcript_blocks.zig:757`).
        let a111 = "a".repeat(111);
        for wide in ["\u{6f22}", "\u{1f600}"] {
            assert_eq!(
                added(&format!("{a111}{wide}TAIL"), 120),
                [
                    diff_grey(&format!("  \u{2502} 1 + {a111}")),
                    diff_grey(&format!("  \u{2502}     {wide}TAIL")),
                ],
                "{wide:?}"
            );
        }
        // A mark joined to the character in the last column stays with it:
        // what takes no cells never starts a row (`:751-755`).
        let f112 = "f".repeat(112);
        assert_eq!(
            added(&format!("{f112}\u{301}Z"), 120),
            [
                diff_grey(&format!("  \u{2502} 1 + {f112}\u{301}")),
                diff_grey("  \u{2502}     Z"),
            ]
        );
    }

    #[test]
    fn a_spelled_escape_at_the_margin_is_cut_like_any_text_and_no_row_carries_a_control() {
        // The text is fx's spelling already (`appendTerminalSafeLine`), so
        // `\x1b` and `\u{202e}` are printable ASCII and are cut where the
        // margin falls, as fx's reflow cuts them; no row, the first or a
        // continuation, holds a byte a terminal acts on.
        let acted_on = |c: char| {
            c.is_control()
                || matches!(c, '\u{200b}'..='\u{200f}' | '\u{2028}'..='\u{202e}' | '\u{2060}'..='\u{206f}' | '\u{feff}' | '\u{61c}')
        };
        let b110 = "b".repeat(110);
        let c108 = "c".repeat(108);
        let cases = [
            (
                format!("{b110}\u{1b}[2JEND"),
                [
                    format!("  \u{2502} 1 + {b110}\\x"),
                    "  \u{2502}     1b[2JEND".to_string(),
                ],
            ),
            (
                format!("{c108}\u{202e}END"),
                [
                    format!("  \u{2502} 1 + {c108}\\u{{2"),
                    "  \u{2502}     02e}END".to_string(),
                ],
            ),
        ];
        for (line, expected) in cases {
            let rows = added(&line, 120);
            assert_eq!(rows, expected.map(|row| diff_grey(&row)), "{line:?}");
            for row in &rows {
                let bare = [DARK.dim(), DARK.diff(), DARK.reset()]
                    .iter()
                    .fold(row.clone(), |row, sgr| row.replace(sgr, ""));
                assert!(
                    !bare.chars().any(acted_on),
                    "a row hands the terminal a character it acts on: {bare:?}"
                );
            }
        }
    }

    #[test]
    fn a_tab_is_no_cells_to_the_reflow_and_the_row_walker_runs_its_overflow_on() {
        // fx counts a tab as no cells where it breaks the line
        // (`displayUnitAt`, `display_width.zig:241-244@481104fd`: a C0
        // control is none), then its row walker moves the tab to the next
        // stop of eight, at most to the last column (`nextTabStopColumn`,
        // `:292-300`; `appendWithSoftWrapPrefix`,
        // `full_transcript_screen.zig:4004-4010@481104fd`), and runs what
        // still passes the margin on to the next row with no gutter
        // (`:4019-4031`; a static row's prefix is empty, `:3848`).
        //
        // `tab` and 109 `d`s are the first row's 112 cells to the reflow;
        // drawn, the tab takes five cells, 104 `d`s fit and five run on;
        // the reflow's own row holds the last 11 under the gutter.
        let rows = added(&format!("tab\t{}", "d".repeat(120)), 120);
        assert_eq!(
            plain(&rows),
            [
                format!("  \u{2502} 1 + tab     {}", "d".repeat(104)),
                "d".repeat(5),
                format!("  \u{2502}     {}", "d".repeat(11)),
            ],
            "{rows:?}"
        );
        // The tab's cells unstyled, the rest in the diff grey -- the row
        // the walker ran on too.
        let tab = format!(
            "{}  \u{2502} 1 + tab{}     {}d",
            DARK.diff(),
            DARK.reset(),
            DARK.diff()
        );
        assert!(rows[0].starts_with(&tab), "{rows:?}");
        assert!(
            rows[1].starts_with(&format!("{}d", DARK.diff())),
            "{rows:?}"
        );
        assert!(rows.iter().all(|row| wrap::width(row) <= 120), "{rows:?}");
        // A tab one cell short of the last column moves to it, not past it:
        // `X` is in the last column and only `Y` runs on.
        let e110 = "e".repeat(110);
        assert_eq!(
            plain(&added(&format!("{e110}\tXY"), 120)),
            [format!("  \u{2502} 1 + {e110} X"), "Y".to_string()]
        );
    }

    #[test]
    fn a_row_with_no_number_or_no_room_runs_on_as_the_row_walker_lets_it() {
        // fx reflows only a row with a line number (`diffPrefix`,
        // `transcript_blocks.zig:659-695@481104fd`), and only when its
        // prefix leaves the screen a column (`:783`); the walker runs any
        // other row on to the next, mid-word and with no gutter, as it does
        // every static row (`full_transcript_screen.zig:4019-4031@481104fd`).
        let notice = diff_rows(&ApprovalDiff::of("same\n", "same\n"), 30, DARK);
        assert_eq!(
            plain(&notice),
            [
                "  \u{2502}     (the text of this file",
                " would not change)",
            ]
        );
        assert_eq!(plain(&added("abc", 6)), ["  \u{2502} 1 ", "+ abc"]);
    }

    #[test]
    fn what_takes_no_cells_stays_on_its_row_even_past_a_character_wider_than_the_room() {
        // `reflowDiffRow` writes a unit of no cells before it asks whether to
        // break (`transcript_blocks.zig:751-755@481104fd`), so it never moves
        // down -- even after a character wider than the whole room, which
        // fx breaks before at once and leaves on its row past the margin
        // (`:756-766`): an empty first row, `漢` and the tab, then `Z`.
        assert_eq!(reflowed("\u{6f22}\tZ", 1), ["", "\u{6f22}\t", "Z"]);
        assert_eq!(reflowed("ab\u{301}c", 2), ["ab\u{301}", "c"]);
    }

    #[test]
    fn a_tab_the_walker_meets_on_a_row_it_ran_on_goes_to_that_rows_next_stop() {
        // fx's walker runs a row on first, its column back to 1
        // (`full_transcript_screen.zig:4019-4022@481104fd`), and places a
        // tab after that from the column of the row it is on (`:4004-4006`,
        // `nextTabStopColumn`, `display_width.zig:292-300@481104fd`) --
        // `p5-transcript-diffwrap2` `full`, its lines 1 and 2. Both lines
        // are one piece to the reflow (a tab is no cells there): drawn, the
        // first tab takes eight cells and 104 characters fill the row.
        let blanks = " ".repeat(8);
        assert_eq!(
            plain(&added(&format!("\t{}\tZ", "b".repeat(108)), 120)),
            [
                format!("  \u{2502} 1 + {blanks}{}", "b".repeat(104)),
                "bbbb    Z".to_string(),
            ]
        );
        assert_eq!(
            plain(&added(&format!("\t{}\tXY", "a".repeat(110)), 120)),
            [
                format!("  \u{2502} 1 + {blanks}{}", "a".repeat(104)),
                "aaaaaa  XY".to_string(),
            ]
        );
        // The run-on row's tab cells unstyled, as the first row's are.
        let rows = added(&format!("\t{}\tZ", "b".repeat(108)), 120);
        assert_eq!(
            rows[1],
            format!(
                "{}bbbb{}    {}Z{}",
                DARK.diff(),
                DARK.reset(),
                DARK.diff(),
                DARK.reset()
            )
        );
        // A screen narrower than the prefix: no reflow, and the walker runs
        // the prefix itself on; the tab goes from the second row's column 3
        // to its last, 6.
        assert_eq!(plain(&added("\tab", 6)), ["  \u{2502} 1 ", "+    a", "b"]);
    }

    #[test]
    fn a_tab_after_a_row_the_walker_filled_puts_the_next_character_over_its_last_cell() {
        // A tab whose row is already full moves the walker's column back to
        // the last one (`nextTabStopColumn` gives `cols` for a column at or
        // past it, `display_width.zig:295@481104fd`), and the next
        // character, which fits there, is written over the row's last cell
        // (`shouldWrapAt(cols, 1, cols)` is false) -- `p5-transcript-
        // diffwrap2` `full` line 3: `…cZ`, then `W`. Over the right half of
        // a wide character, the character's left cell is left blank and
        // unstyled (line 13: `…i Z`); a wide character after such a tab
        // does not fit there and starts the next row, the row's last cell
        // kept (line 14).
        let blanks = " ".repeat(8);
        assert_eq!(
            plain(&added(&format!("\t{}\tZW", "c".repeat(104)), 120)),
            [
                format!("  \u{2502} 1 + {blanks}{}Z", "c".repeat(103)),
                "W".to_string(),
            ]
        );
        let rows = added(&format!("\t{}\u{6f22}\tZW", "i".repeat(102)), 120);
        assert_eq!(
            plain(&rows),
            [
                format!("  \u{2502} 1 + {blanks}{} Z", "i".repeat(102)),
                "W".to_string(),
            ]
        );
        assert!(
            rows[0].ends_with(&format!(
                "i{} {}Z{}",
                DARK.reset(),
                DARK.diff(),
                DARK.reset()
            )),
            "{rows:?}"
        );
        assert_eq!(
            plain(&added(&format!("\t{}\t\u{6f22}W", "j".repeat(104)), 120)),
            [
                format!("  \u{2502} 1 + {blanks}{}", "j".repeat(104)),
                "\u{6f22}W".to_string(),
            ]
        );
    }

    #[test]
    fn a_mark_right_after_a_tab_is_not_drawn_as_fxs_screen_does_not_draw_it() {
        // fx's walker writes a unit of no cells where its column is
        // (`full_transcript_screen.zig:4017-4034@481104fd`, `width > 0`
        // guarding only the wrap), and one right after a tab has no
        // character before it on fx's screen: `p5-transcript-diffwrap2`
        // `full` lines 15-19 show neither a U+FE0F nor a U+0301 that follows
        // a tab, whether the tab moved the column (`p      q`) or not
        // (`…k1Z`, `…meZ`), where one before a tab stays (`t́      u`).
        let blanks = " ".repeat(8);
        let k102 = "k".repeat(102);
        let rows = added(&format!("\t{k102}1\t\u{fe0f}Z"), 120);
        // The frame keeps every cell of each row: what the walker counts is
        // what the frame measures (`super::frame::clip`).
        for row in &rows {
            assert_eq!(super::super::frame::clip(row, 120), row, "{rows:?}");
        }
        assert_eq!(plain(&rows), [format!("  \u{2502} 1 + {blanks}{k102}1Z")]);
        let m102 = "m".repeat(102);
        assert_eq!(
            plain(&added(&format!("\t{m102}e\t\u{301}Z"), 120)),
            [format!("  \u{2502} 1 + {blanks}{m102}eZ")]
        );
        // After a tab on a full row: dropped, and `Z` over the `1`.
        let l103 = "l".repeat(103);
        assert_eq!(
            plain(&added(&format!("\t{l103}1\t\u{fe0f}ZW"), 120)),
            [format!("  \u{2502} 1 + {blanks}{l103}Z"), "W".to_string()]
        );
        // After a tab that moved the column, and before one.
        assert_eq!(
            plain(&added("p\t\u{301}q", 120)),
            ["  \u{2502} 1 + p       q"]
        );
        assert_eq!(
            plain(&added("r\t\u{fe0f}s", 120)),
            ["  \u{2502} 1 + r       s"]
        );
        assert_eq!(
            plain(&added("t\u{301}\tu", 120)),
            ["  \u{2502} 1 + t\u{301}       u"]
        );
    }

    #[test]
    fn a_mark_alone_after_a_character_that_ends_its_cluster_is_kept_after_a_tab() {
        // A tab counts only for the cluster right after it: once a character
        // has been written, a mark that stands as a cluster of its own --
        // after a soft hyphen, which ends a grapheme cluster (it is a
        // control to UAX #29, GB4/GB5) and which `terminal_safe_line` writes
        // as itself -- is kept, a cell of no width after the hyphen's cell,
        // as before the tab rule. The frame segments the row text the same
        // way and keeps every cell (`super::frame::clip`).
        let blanks = " ".repeat(8);
        let rows = added("\tx\u{ad}\u{301}y", 120);
        assert_eq!(
            plain(&rows),
            [format!("  \u{2502} 1 + {blanks}x\u{ad}\u{301}y")]
        );
        for row in &rows {
            assert_eq!(super::super::frame::clip(row, 120), row, "{rows:?}");
        }
    }

    #[test]
    fn a_files_line_is_spelled_as_fxs_append_terminal_safe_line_spells_it() {
        // (the file's line, the row's text, fx's line at 481104fd in
        // src/core/shared/text_utils.zig)
        let table: &[(&str, &str, &str)] = &[
            (
                "before\u{1b}[2Jcleared",
                "before\\x1b[2Jcleared",
                "ESC, a CSI: terminalSafeToken :645-646, writeByteEscape :675-686",
            ),
            (
                "\u{1b}]0;title\u{7}",
                "\\x1b]0;title\\x07",
                "an OSC and its BEL: :645-646",
            ),
            (
                "second \u{9b}line",
                "second \\u{009b}line",
                "C1 CSI U+009B: isNonPrintingCodepoint :704, writeCodepointEscape :688-701",
            ),
            ("osc \u{9d}x", "osc \\u{009d}x", "C1 OSC U+009D: :704"),
            ("del \u{7f}", "del \\x7f", "DEL: :645"),
            (
                "tab\there",
                "tab\there",
                "a tab stays: appendTerminalSafeLine :432-436",
            ),
            ("crlf line\r", "crlf line", "a trailing CR is dropped: :429"),
            ("two\r\r", "two\\x0d", "only one: :429, then :645"),
            ("mid\rline", "mid\\x0dline", "a CR inside the line: :645"),
            ("bidi \u{202e} x", "bidi \\u{202e} x", "U+202E: :706"),
            (
                "isolate \u{2066}x\u{2069}",
                "isolate \\u{2066}x\\u{2069}",
                "an isolate: :707",
            ),
            (
                "mark \u{200e}\u{200f}",
                "mark \\u{200e}\\u{200f}",
                "LRM, RLM: :705",
            ),
            ("zero\u{200b}width", "zero\\u{200b}width", "U+200B: :705"),
            ("line\u{2028}sep", "line\\u{2028}sep", "U+2028: :706"),
            ("\u{feff}bom", "\\u{feff}bom", "U+FEFF: :708"),
            (
                "é 한 😀 ⋯ \u{202e}",
                "é 한 😀 ⋯ \\u{202e}",
                "printables stay: :666",
            ),
            (
                "literal \\x1b",
                "literal \\x1b",
                "a backslash is itself: :648-649",
            ),
            // xfx's one addition: fx writes U+061C as itself (:703-708 has no
            // arm for it); it reorders as the marks above do.
            (
                "alm \u{61c}x",
                "alm \\u{061c}x",
                "U+061C: xfx names it (ctrl-o-arabic-letter-mark)",
            ),
        ];
        // A byte outside a valid UTF-8 sequence (:652-662) cannot be here:
        // the line is a `str`.
        for (line, row, why) in table {
            assert_eq!(terminal_safe_line(line), *row, "{why}");
        }
    }

    #[test]
    fn every_code_point_to_u2100_and_past_it_is_spelled_as_fx_spells_it() {
        // fx's rule for one character of a line, written out here rather
        // than read from `terminal_safe_line` or `fx_names`, so a narrowed
        // range cannot pass itself (`terminalSafeToken`,
        // `text_utils.zig:643-667@481104fd`; `isNonPrintingCodepoint`,
        // `:703-708`): C0 and DEL `\xNN`; C1, U+200B-200F, U+2028-202E,
        // U+2060-206F and U+FEFF `\u{NNNN}`; a tab itself; and xfx's one
        // addition, U+061C.
        let fx = |c: char| -> String {
            let code = u32::from(c);
            if c == '\t' {
                "\t".to_string()
            } else if code < 0x20 || code == 0x7f {
                format!("\\x{code:02x}")
            } else if (0x80..=0x9f).contains(&code)
                || (0x200b..=0x200f).contains(&code)
                || (0x2028..=0x202e).contains(&code)
                || (0x2060..=0x206f).contains(&code)
                || code == 0xfeff
                || code == 0x061c
            {
                format!("\\u{{{code:04x}}}")
            } else {
                c.to_string()
            }
        };
        // Every code point to U+2100, then the byte-order mark and the
        // default-ignorable ones fx writes as themselves and tmux 3.7b draws
        // a cell wide (`approval_view`'s `default_ignorable`): those stay
        // raw here too, as fx's, and the painter counts them as fx does
        // (`super::super::wrap`'s `counted_as_upstream`, WIDTH-DISPUTED).
        let points = (0u32..=0x2100)
            .chain([0xfeff, 0x061c, 0x3164, 0xffa0])
            .chain(0xfff0..=0xfff8);
        let mut checked = 0;
        for code in points {
            let Some(c) = char::from_u32(code) else {
                continue;
            };
            // Between two letters, so a carriage return is not the one
            // that ends the line (that one is dropped, `:429`).
            let line = format!("a{c}b");
            assert_eq!(
                terminal_safe_line(&line),
                format!("a{}b", fx(c)),
                "U+{code:04X}"
            );
            checked += 1;
        }
        assert_eq!(checked, 0x2101 + 2 + 2 + 9);
    }

    #[test]
    fn the_boundarys_spelling_reads_back_to_the_files_text() {
        // `bounded_diff_side` is injective; `unspelled` is its inverse on a
        // line, over every scalar up to U+00FF and the marks fx names.
        let mut samples: Vec<String> = (0u32..=0xff)
            .filter_map(char::from_u32)
            .filter(|c| *c != '\n')
            .map(|c| format!("a{c}b"))
            .collect();
        samples.extend(
            [
                "\u{202e}",
                "\u{200b}",
                "\u{feff}",
                "\u{61c}",
                "\\",
                "\\\\",
                "\\t",
                "\\u{001B}",
                "x\\",
            ]
            .iter()
            .map(|s| (*s).to_string()),
        );
        for raw in samples {
            let spelled = crate::permission::bounded_diff_side(&raw);
            assert_eq!(unspelled(&spelled), raw, "{spelled:?}");
        }
        // What the boundary never writes is kept as it is, and spelled
        // safely after it.
        for (odd, kept) in [
            ("\\q", "\\q"),
            ("\\u{D800}", "\\u{D800}"),
            ("\\u{zz}", "\\u{zz}"),
            ("\\u{+41}", "\\u{+41}"),
            ("\\u{", "\\u{"),
            ("end\\", "end\\"),
        ] {
            assert_eq!(unspelled(odd), kept);
            assert_eq!(terminal_safe_line(&unspelled(odd)), kept);
        }
    }

    #[test]
    fn a_change_that_arrives_before_its_row_waits_for_it_and_a_refused_one_keeps_its_arguments() {
        let input = json!({"path": "a.txt", "old_string": "a", "new_string": "b"});
        let shown = crate::tools::mutate::EDIT_FILE.shown(&input);
        let mut record = record();
        record.observe(&UiEvent::ToolDetail {
            call_id: "e".into(),
            detail: Detail::Change(ApprovalDiff::of("a\n", "b\n")),
        });
        record.tool_started("e", &shown);
        record.tool_finished(
            "e",
            true,
            "",
            Outcome::Finished,
            Some(LineChanges {
                added: 1,
                removed: 1,
            }),
        );
        let made = plain(&record.rows(120, 40, DARK, ""));
        assert!(
            made.iter().any(|row| row == "  \u{2502} 1 - a")
                && made.iter().any(|row| row == "  \u{2502} 1 + b"),
            "{made:?}"
        );

        let mut refused = self::record();
        refused.observe(&UiEvent::ToolDetail {
            call_id: "e".into(),
            detail: Detail::Arguments(arguments("edit_file", &input)),
        });
        refused.tool_started("e", &shown);
        refused.tool_finished("e", false, "denied", Outcome::Denied, None);
        let refused = plain(&refused.rows(120, 40, DARK, ""));
        assert!(
            refused.iter().any(|row| row.contains("new_string: b")),
            "a change that was not made shows what was asked: {refused:?}"
        );
        assert!(!refused.iter().any(|row| row.contains("\u{2502} 1 +")));
    }

    #[test]
    fn a_file_change_says_nothing_under_its_row_but_a_refusal_of_its_own() {
        // `fx-fulltranscript-40x120-full`: `├ Edited a.txt +1 / -1` and its
        // diff, with no `Edited a.txt` sentence between them.
        let input = json!({"path": "a.txt", "old_string": "a", "new_string": "b"});
        let shown = crate::tools::mutate::EDIT_FILE.shown(&input);
        let said = |ok: bool, outcome: Outcome| {
            let mut record = record();
            record.tool_started("e", &shown);
            record.observe(&UiEvent::ToolDetail {
                call_id: "e".into(),
                detail: Detail::Result("the tool's sentence".into()),
            });
            record.tool_finished("e", ok, "the tool's sentence", outcome, None);
            screen(&record)
                .iter()
                .any(|row| row == "\u{2502}  the tool's sentence")
        };
        assert!(!said(true, Outcome::Finished), "a change that was made");
        assert!(!said(false, Outcome::Denied), "a change the user refused");
        assert!(!said(true, Outcome::Unchanged), "a change of nothing");
        assert!(said(false, Outcome::Finished), "a change the tool refused");
    }

    #[test]
    fn a_file_change_waiting_on_its_approval_has_no_entry_until_it_settles() {
        // The group draws no row for it (`fx-approvals-40x120-write-ask`), so
        // neither does this screen; it appears once it is decided.
        let input = json!({"path": "c.txt", "content": "x"});
        let mut record = record();
        record.tool_started("w", &crate::tools::mutate::WRITE_FILE.shown(&input));
        assert!(screen(&record)[..36].iter().all(String::is_empty));
        record.tool_finished("w", false, "no", Outcome::Denied, None);
        assert_eq!(screen(&record)[2], "\u{2514} Denied file");
    }

    #[test]
    fn a_call_still_running_when_its_turn_ends_is_cancelled_like_the_groups() {
        let mut record = record();
        record.tool_started("c", &command("sleep 30"));
        record.turn_over();
        let rows = screen(&record);
        assert_eq!(
            &rows[1..3],
            [
                "\u{25cf} 1 tool call \u{b7} 1 command \u{b7} 1 cancelled",
                "\u{2514} Cancelled sleep 30",
            ]
        );
        // The group's own `▪ Cancelled …` line is the inline transcript's:
        // upstream's full screen does not repeat it (`fx-approvals-cc-40x120-full`).
        assert!(!rows.iter().any(|row| row.contains("What can")));
    }

    #[test]
    fn answer_text_breaks_a_group_and_a_notice_does_not() {
        let mut record = record();
        record.tool_started("a", &read("a.txt"));
        record.notice(&Notice::neutral("note", "between"));
        record.tool_started("b", &read("b.txt"));
        record.answer("text\n");
        record.tool_started("c", &read("c.txt"));
        let rows = screen(&record);
        let headers: Vec<&String> = rows
            .iter()
            .filter(|row| row.starts_with('\u{25cf}'))
            .collect();
        assert_eq!(
            headers,
            [
                "\u{25cf} 2 tool calls \u{b7} 2 read",
                "\u{25cf} 1 tool call \u{b7} 1 read"
            ]
        );
        // Line breaks alone open no response and break nothing.
        let mut record = self::record();
        record.tool_started("a", &read("a.txt"));
        record.answer("\n");
        record.tool_started("b", &read("b.txt"));
        let rows = screen(&record);
        assert_eq!(
            rows.iter()
                .filter(|row| row.starts_with('\u{25cf}'))
                .count(),
            1
        );
        assert!(!rows.iter().any(|row| row.contains("Response")));
    }

    #[test]
    fn a_notice_is_the_documents_notice_and_an_error_is_headed_error() {
        let mut record = record();
        record.notice(&Notice::neutral("status", "model=a\nmode=auto"));
        record.notice(&Notice::error("system", "it broke"));
        let rows = screen(&record);
        assert_eq!(
            &rows[..7],
            [
                "  2026-10-06 14:43:50.620 UTC \u{b7} Notice",
                "* status: model=a",
                "  mode=auto",
                "",
                "  2026-10-06 14:43:50.620 UTC \u{b7} Error",
                "\u{2717} system: it broke",
                "",
            ]
        );
    }

    #[test]
    fn the_session_record_is_upstreams_body_with_the_lines_xfx_has_a_fact_for() {
        // `fx-fulltranscript-notice-40x120-full-top`: the record under the
        // welcome, the tool list cut at eight and wrapped two columns in.
        let notice = session_record(
            "gateway",
            "fake/model-a",
            &["a", "b", "c", "d", "e", "f", "g", "h", "i"],
            &[],
        );
        assert_eq!(notice.tone, Tone::Neutral);
        assert_eq!(notice.topic, "session");
        assert_eq!(
            notice.body,
            "provider: gateway \u{b7} model: fake/model-a \u{b7} effort: auto\n\
             tools: 9 advertised (a, b, c, d, e, f, g, h, +1 more)\n\
             mcp: none"
        );
        assert_eq!(
            session_record("llmux", "m", &["a"], &[]).body,
            "provider: llmux \u{b7} model: m \u{b7} effort: auto\ntools: 1 advertised (a)\nmcp: none"
        );
    }

    fn completion(
        finish_reason: FinishReason,
        usage: crate::gateway::protocol::Usage,
    ) -> Completion {
        Completion {
            text: String::new(),
            tool_calls: Vec::new(),
            finish_reason,
            usage,
            provider_detail: None,
            raw_content: Vec::new(),
            wire: crate::provider::Wire::VercelGateway,
            billing: None,
            service_tier: None,
        }
    }

    #[test]
    fn a_network_record_says_how_a_request_settled_as_upstreams_does() {
        // `writeNetworkRecordBody` (`orchestrator.zig:4194-4235`) and the
        // capture's `* network: provider: gateway · model: fake/model-a ·
        // 5510ms` / `finish: stop` / `tokens: 1200 in · 40 out`.
        let usage = crate::gateway::protocol::Usage {
            input_tokens: Some(1200),
            output_tokens: Some(40),
        };
        let done = network_record(
            "gateway",
            "fake/model-a",
            5510,
            &Ok(completion(FinishReason::Stop, usage)),
        )
        .expect("a completed request");
        assert_eq!((done.tone, done.topic.as_str()), (Tone::Neutral, "network"));
        assert_eq!(
            done.body,
            "provider: gateway \u{b7} model: fake/model-a \u{b7} 5510ms\nfinish: stop\ntokens: 1200 in \u{b7} 40 out"
        );
        // Only the counts the provider reported, zero included.
        let silent = network_record(
            "gateway",
            "m",
            1,
            &Ok(completion(
                FinishReason::ToolCalls,
                crate::gateway::protocol::Usage {
                    input_tokens: None,
                    output_tokens: Some(0),
                },
            )),
        )
        .expect("a completed request");
        assert_eq!(
            silent.body,
            "provider: gateway \u{b7} model: m \u{b7} 1ms\nfinish: tool_calls\ntokens: 0 out"
        );
        let unreported = network_record(
            "gateway",
            "m",
            1,
            &Ok(completion(
                FinishReason::ContentFilter,
                crate::gateway::protocol::Usage::default(),
            )),
        )
        .expect("a completed request");
        assert!(
            unreported.body.ends_with("\nfinish: content_filter"),
            "{unreported:?}"
        );

        // A refusal is a warning, named as upstream names its status.
        let refused = network_record(
            "llmux",
            "m",
            7,
            &Err(ProviderError::Status {
                subject: "completion",
                status: 429,
                body: "slow down \u{1b}[2J".into(),
                retryable: true,
                retry_after: Some(std::time::Duration::from_secs(3)),
            }),
        )
        .expect("a refused request");
        assert_eq!(refused.tone, Tone::Warning);
        assert_eq!(
            refused.body,
            "provider: llmux \u{b7} model: m \u{b7} 7ms\nfailed: rate_limited \u{b7} retry after: 3s"
        );
        for (status, kind) in [
            (400, "invalid_request"),
            (503, "unavailable"),
            (418, "provider_error"),
        ] {
            let body = network_record(
                "gateway",
                "m",
                0,
                &Err(ProviderError::Status {
                    subject: "completion",
                    status,
                    body: String::new(),
                    retryable: false,
                    retry_after: None,
                }),
            )
            .expect("a refused request")
            .body;
            assert!(
                body.ends_with(&format!("failed: {kind}")),
                "{status}: {body}"
            );
        }

        // Anything else never settled, and upstream pushes no record for it.
        assert!(network_record("gateway", "m", 0, &Err(ProviderError::Cancelled)).is_none());
        assert!(network_record(
            "gateway",
            "m",
            0,
            &Err(ProviderError::Connect {
                subject: "completion",
                detail: "refused".into(),
            })
        )
        .is_none());
    }

    #[test]
    fn every_attempt_the_turns_provider_settles_is_handed_to_the_ui_as_a_record() {
        struct Answers(RefCell<Vec<Result<Completion, ProviderError>>>);
        #[async_trait::async_trait(?Send)]
        impl Provider for Answers {
            async fn stream(
                &self,
                _request: &CompletionRequest,
                _deltas: &mut dyn DeltaSink,
            ) -> Result<Completion, ProviderError> {
                self.0.borrow_mut().remove(0)
            }
        }
        struct Nowhere;
        impl DeltaSink for Nowhere {
            fn text_delta(&mut self, _text: &str) -> std::io::Result<()> {
                Ok(())
            }
        }
        let inner = Answers(RefCell::new(vec![
            Err(ProviderError::Status {
                subject: "completion",
                status: 503,
                body: String::new(),
                retryable: true,
                retry_after: None,
            }),
            Ok(completion(
                FinishReason::Stop,
                crate::gateway::protocol::Usage::default(),
            )),
            Err(ProviderError::Cancelled),
        ]));
        let (events, mut received) = tokio::sync::mpsc::channel(8);
        let networked = Networked::new(&inner, "gateway", events, CancellationToken::new());
        let request = CompletionRequest {
            model: "vendor/\u{1b}model".into(),
            messages: Vec::new(),
            tools: Vec::new(),
            tool_choice: crate::gateway::protocol::ToolChoice::Auto,
        };
        for _ in 0..3 {
            let _ = bridge::park_on(networked.stream(&request, &mut Nowhere));
        }
        let mut records = Vec::new();
        while let Ok(UiEvent::FullDetail(notice)) = received.try_recv() {
            records.push(notice);
        }
        assert_eq!(records.len(), 2, "{records:?}");
        assert_eq!(records[0].tone, Tone::Warning);
        assert!(records[0].body.contains("failed: unavailable"));
        assert_eq!(records[1].tone, Tone::Neutral);
        // The model id came out of a settings file: made inert at the channel.
        assert!(
            records[1]
                .body
                .starts_with("provider: gateway \u{b7} model: vendor/ model \u{b7} "),
            "{:?}",
            records[1].body
        );
        assert!(records[1].body.ends_with("\nfinish: stop"));
    }

    #[test]
    fn records_sit_under_the_welcome_above_every_entry_in_the_order_they_came() {
        // `prependFullDetailRecords` (`transcript/runtime.zig:4980-5001`):
        // the network record of a request that settled after the prompt and
        // the answer is still above them, as in the capture.
        let mut record = record();
        record.welcome();
        record.full_detail(session_record("gateway", "m", &["read_file"], &[]));
        record.prompt("hi");
        record.answer("hello\n");
        record.observe(&UiEvent::FullDetail(Notice::neutral(
            "network",
            "provider: gateway \u{b7} model: m \u{b7} 5ms\nfinish: stop",
        )));
        let rows = screen(&record);
        let kinds: Vec<&str> = rows
            .iter()
            .filter_map(|row| row.split_once(" UTC \u{b7} ").map(|(_, kind)| kind))
            .collect();
        assert_eq!(kinds, ["Notice", "Notice", "Prompt", "Response"]);
        assert!(rows[0].contains("Run /help for commands"), "{rows:?}");
        assert_eq!(
            rows[3],
            "* session: provider: gateway \u{b7} model: m \u{b7} effort: auto"
        );
        assert_eq!(rows[4], "  tools: 1 advertised (read_file)");
        assert_eq!(
            rows[8],
            "* network: provider: gateway \u{b7} model: m \u{b7} 5ms"
        );
        assert_eq!(rows[9], "  finish: stop");
        // A reset starts a record with none.
        assert!(!screen(&FullTranscript::with_clock(fixed))
            .iter()
            .any(|row| row.contains("session:")));
    }

    /// A change made before a resume has its diff in the log beside its
    /// result, and the replayed record shows it as the live one did.
    #[test]
    fn a_replayed_change_shows_the_diff_the_log_kept() {
        let input = json!({"path": "a.txt", "old_string": "a", "new_string": "b"});
        let steps = vec![
            TurnStep::Assistant {
                text: String::new(),
                tool_calls: vec![crate::session::RecordedToolCall {
                    id: "e".into(),
                    name: "edit_file".into(),
                    input: input.clone(),
                }],
                raw_content: Vec::new(),
                responses_state: Vec::new(),
                wire: None,
                model: None,
            },
            TurnStep::ToolResult {
                images: Vec::new(),
                call_id: "e".into(),
                tool: "edit_file".into(),
                ok: true,
                output: "Edited a.txt".into(),
                ending: None,
                lines: Some(crate::session::LineCounts {
                    added: 1,
                    removed: 1,
                }),
                change: Some(crate::session::RecordedChange {
                    before: "a\n".into(),
                    after: "b\n".into(),
                    first_line: 1,
                    lines_below: 0,
                }),
            },
        ];
        let details = details_of(&steps);
        assert_eq!(
            details.last(),
            Some(&(
                "e".to_string(),
                Detail::Change(ApprovalDiff::of("a\n", "b\n"))
            ))
        );
        // Replayed, the change's own result is listed above its diff, as
        // upstream's replay hands the recorded output to the call's detail
        // (`attachHistoricalToolDetailResolved`, `transcript/runtime.zig:
        // 5762-5829`; `fx-resume-tools-40x120-ctrl-o-up1`: `│  edited a.txt
        // (41 bytes)`, a blank row, the diff) -- where a live one shows the
        // diff alone (`-live-ctrl-o-up1`).
        let replayed = |replaying: bool| {
            let mut record = record();
            record.replay(replaying);
            for (call_id, detail) in details.clone() {
                record.observe(&UiEvent::ToolDetail { call_id, detail });
            }
            record.tool_started("e", &crate::tools::mutate::EDIT_FILE.shown(&input));
            record.tool_finished(
                "e",
                true,
                "",
                Outcome::Finished,
                Some(LineChanges {
                    added: 1,
                    removed: 1,
                }),
            );
            record.replay(false);
            plain(&record.rows(120, 40, DARK, ""))
        };
        let rows = replayed(true);
        let edited = rows
            .iter()
            .position(|row| row.ends_with("Edited a.txt +1 / -1"))
            .expect("the row");
        assert_eq!(
            &rows[edited..edited + 5],
            [
                "\u{2514} Edited a.txt +1 / -1",
                "\u{2502}  Edited a.txt",
                "",
                "  \u{2502} 1 - a",
                "  \u{2502} 1 + b",
            ]
        );
        let rows = replayed(false);
        assert!(
            !rows.iter().any(|row| row == "\u{2502}  Edited a.txt"),
            "a live change listed its result: {rows:?}"
        );
        assert!(rows.iter().any(|row| row == "  \u{2502} 1 + b"), "{rows:?}");
    }

    #[test]
    fn a_replayed_turn_carries_the_times_the_log_has_and_its_calls_details() {
        // The entry point a resumed session fills the record through:
        // `details_of` over the log's steps, and `stamp_with` its times.
        let steps = vec![
            TurnStep::Assistant {
                text: String::new(),
                tool_calls: vec![crate::session::RecordedToolCall {
                    id: "c".into(),
                    name: "read_file".into(),
                    input: json!({"path": "a.txt"}),
                }],
                raw_content: Vec::new(),
                responses_state: Vec::new(),
                wire: None,
                model: None,
            },
            TurnStep::ToolResult {
                call_id: "c".into(),
                tool: "read_file".into(),
                ok: true,
                output: "<path>a.txt</path>\n<content>\n1\tsaved line\n</content>".into(),
                images: Vec::new(),
                ending: None,
                lines: None,
                change: None,
            },
            TurnStep::ToolFeedback {
                call_id: "c".into(),
                text: "fine".into(),
            },
        ];
        let details = details_of(&steps);
        assert_eq!(
            details,
            vec![
                (
                    "c".to_string(),
                    Detail::Arguments(vec![Argument {
                        name: "path".into(),
                        value: "a.txt".into(),
                        text: true,
                    }])
                ),
                ("c".to_string(), Detail::Result("1    saved line".into())),
            ]
        );

        let mut record = record();
        for (call_id, detail) in details {
            record.observe(&UiEvent::ToolDetail { call_id, detail });
        }
        // 2026-10-02 00:00:00.000 UTC, then a second later.
        record.stamp_with(Some(1_790_899_200_000));
        record.prompt("read it");
        record.tool_started("c", &read("a.txt"));
        record.tool_finished("c", true, "", Outcome::Finished, None);
        record.stamp_with(Some(1_790_899_201_000));
        record.answer("done\n");
        record.stamp_with(None);
        record.notice(&Notice::neutral("now", "live"));
        let rows = screen(&record);
        assert_eq!(rows[0], "  2026-10-02 00:00:00.000 UTC \u{b7} Prompt");
        assert_eq!(rows[3], "  2026-10-02 00:00:00.000 UTC \u{b7} Tool");
        assert!(
            rows.contains(&"\u{2502}  1    saved line".to_string()),
            "{rows:?}"
        );
        assert!(
            rows.contains(&"  2026-10-02 00:00:01.000 UTC \u{b7} Response".to_string()),
            "{rows:?}"
        );
        assert!(
            rows.contains(&"  2026-10-06 14:43:50.620 UTC \u{b7} Notice".to_string()),
            "{rows:?}"
        );
    }

    #[test]
    fn a_questions_resolution_is_shown_as_the_document_has_it_under_no_header() {
        // Upstream's `question_resolution` entries have no metadata kind
        // (`metadataKind`, `full_transcript_screen.zig:5867-5895`), so no header.
        let mut record = record();
        record.prompt("ask me");
        record.block(
            &[
                ("  1) Which database?".to_string(), DocumentRow::Plain),
                ("     SQLite".to_string(), DocumentRow::Answer),
            ],
            Mode::Dark,
        );
        let rows = screen(&record);
        assert_eq!(
            &rows[..5],
            [
                "  2026-10-06 14:43:50.620 UTC \u{b7} Prompt",
                "\u{2503} ask me",
                "",
                "  1) Which database?",
                "     SQLite",
            ]
        );
    }

    #[test]
    fn a_resolution_wider_than_a_narrowed_screen_runs_on_to_the_next_row() {
        // fx v0.0.16 keeps the rows it composed at the width of the moment
        // and walks a row wider than the screen onto the next one, cell by
        // cell and in its colours, as a terminal wraps it
        // (`ProjectionRowWalker.appendWithSoftWrapPrefix`,
        // `full_transcript_screen.zig:3968-4039@481104fd`):
        // `p5-transcript-question` at 60 columns, `  1) Which of the two
        // storage engines should the nightly imp` / `ort job write …`.
        let question = "  1) Which of the two storage engines should the nightly import job write its staging tables into before it runs the";
        let mut record = record();
        record.prompt("pick a storage engine");
        record.block(
            &[
                (question.to_string(), DocumentRow::Plain),
                ("     checks?".to_string(), DocumentRow::Plain),
                (
                    format!("{}     Postgres{} with the log", DARK.dim(), DARK.reset()),
                    DocumentRow::Answer,
                ),
            ],
            Mode::Dark,
        );
        let rows = record_at(&record, 60);
        assert_eq!(
            &rows[3..7],
            [
                &question[..60],
                &question[60..],
                "     checks?",
                "     Postgres with the log",
            ]
        );
        assert!(!rows.concat().contains('\u{2026}'));
        // A colour in force where a row runs on is in force on the next.
        let painted = record.rows(20, 40, DARK, "");
        let at = plain(&painted)
            .iter()
            .position(|row| row == "     Postgres with t")
            .expect("the resolution's third row");
        assert!(painted[at].starts_with(DARK.dim()), "{:?}", painted[at]);
        assert_eq!(plain(&painted)[at + 1], "he log");
        for row in &painted {
            assert!(wrap::width(row) <= 20, "{row:?}");
        }
    }

    #[test]
    fn details_that_arrive_before_the_calls_row_wait_for_it_and_die_with_the_turn() {
        let mut record = record();
        record.observe(&UiEvent::ToolDetail {
            call_id: "x".into(),
            detail: Detail::Result("early".into()),
        });
        record.tool_started("x", &read("a.txt"));
        assert!(screen(&record).contains(&"\u{2502}  early".to_string()));

        let mut record = self::record();
        record.observe(&UiEvent::ToolDetail {
            call_id: "never".into(),
            detail: Detail::Result("orphan".into()),
        });
        record.turn_over();
        record.tool_started("never", &read("a.txt"));
        assert!(!screen(&record).iter().any(|row| row.contains("orphan")));
    }

    #[test]
    fn it_opens_at_the_tail_and_the_keys_page_scroll_and_close_as_upstreams() {
        let mut record = record();
        for index in 0..30 {
            record.notice(&Notice::neutral("n", format!("{index}")));
        }
        record.open();
        // 30 notices of two rows plus a blank between: 89 rows, 36 shown.
        assert_eq!(record.measure(120, DARK).0, 89);
        let top = |record: &FullTranscript| screen(record)[0].clone();
        let at_tail = top(&record);
        assert_eq!(screen(&record)[35], "* n: 29");

        assert_eq!(record.key(Action::PageUp, 120, 40, DARK), Reply::Moved);
        assert_eq!(screen(&record)[35], screen(&record)[35]);
        let page_up = screen(&record);
        // A page is the 36 rows the screen shows: from row 53, the tail's
        // first, to row 17.
        let offset =
            |record: &FullTranscript| record.view.as_ref().unwrap().reading.get().scroll_rows;
        assert_eq!(offset(&record), 89 - 36 - 36);
        assert_eq!(record.key(Action::Up, 120, 40, DARK), Reply::Moved);
        assert_eq!(offset(&record), 89 - 36 - 36 - 3);
        assert_eq!(record.key(Action::Down, 120, 40, DARK), Reply::Moved);
        assert_eq!(screen(&record), page_up);
        assert_eq!(record.key(Action::PageUp, 120, 40, DARK), Reply::Moved);
        assert_eq!(
            screen(&record)[0],
            "  2026-10-06 14:43:50.620 UTC \u{b7} Notice"
        );
        // Past the head is held: the key is the screen's, and it moves nothing.
        assert_eq!(record.key(Action::PageUp, 120, 40, DARK), Reply::Held);
        assert_eq!(record.key(Action::Up, 120, 40, DARK), Reply::Held);
        for _ in 0..3 {
            record.key(Action::PageDown, 120, 40, DARK);
        }
        assert_eq!(top(&record), at_tail);
        assert_eq!(record.key(Action::PageDown, 120, 40, DARK), Reply::Held);
        // Left and Right keep the screen where it is (`navigate`), and a
        // redraw does nothing (`.redraw => {}`).
        for action in [Action::Left, Action::Right, Action::Redraw] {
            assert_eq!(record.key(action, 120, 40, DARK), Reply::Held);
        }
        // Everything upstream's map does not name is its composer's
        // (`input_full_transcript_runtime.zig:147-175`): Home and End too.
        for action in [
            Action::Home,
            Action::End,
            Action::Submit,
            Action::Tab,
            Action::Backspace,
            Action::Eof,
            Action::PasteStart,
            Action::ControlP,
        ] {
            assert_eq!(
                record.key(action, 120, 40, DARK),
                Reply::Unbound,
                "{action:?}"
            );
        }
        for action in [Action::ToggleTranscript, Action::Escape, Action::Cancel] {
            assert_eq!(record.key(action, 120, 40, DARK), Reply::Close);
        }
    }

    #[test]
    fn rows_added_under_a_screen_scrolled_up_do_not_move_it_and_at_the_tail_they_are_followed() {
        let mut record = record();
        for index in 0..30 {
            record.notice(&Notice::neutral("n", format!("{index}")));
        }
        record.open();
        record.key(Action::PageUp, 120, 40, DARK);
        let before = screen(&record);
        record.notice(&Notice::neutral("n", "late"));
        assert_eq!(screen(&record), before, "the screen moved under the reader");
        // Back down to the tail, which the rows that arrived moved a page and
        // three rows away.
        while record.key(Action::PageDown, 120, 40, DARK) == Reply::Moved {}
        record.notice(&Notice::neutral("n", "later"));
        assert_eq!(screen(&record)[35], "* n: later");
    }

    #[test]
    fn a_short_record_is_drawn_from_the_top() {
        let mut record = record();
        record.prompt("hi");
        record.open();
        let rows = screen(&record);
        assert_eq!(rows[0], "  2026-10-06 14:43:50.620 UTC \u{b7} Prompt");
        assert_eq!(rows[1], "\u{2503} hi");
        assert!(rows[2..36].iter().all(String::is_empty));
        assert_eq!(record.key(Action::PageUp, 120, 40, DARK), Reply::Held);
    }

    #[test]
    fn a_command_result_is_its_streams_and_a_status_that_was_not_a_clean_exit() {
        // `crate::tools::terminal`'s frame, as `Outcome::into_result` writes
        // it, tags escaped.
        let framed = "<command>echo &lt;b&gt;</command>\n<cwd>.</cwd>\n<exit_code>3</exit_code>\n<stdout>\na &lt;b&gt; &amp; c\n</stdout>\n<stderr>\nerr\n</stderr>\n";
        assert_eq!(
            result("terminal", framed),
            Detail::Command {
                output: "a <b> & c\nerr".into(),
                status: Some("exit code 3".into()),
            }
        );
        let clean = "<command>ls</command>\n<cwd>.</cwd>\n<exit_code>0</exit_code>\n<stdout>\n</stdout>\n<stderr>\n</stderr>\n";
        assert_eq!(
            result("terminal", clean),
            Detail::Command {
                output: String::new(),
                status: None,
            }
        );
        let killed = "<command>x</command>\n<cwd>.</cwd>\n<signal>9</signal>\n<stdout>\n</stdout>\n<stderr>\n</stderr>\n";
        assert_eq!(
            result("terminal", killed),
            Detail::Command {
                output: String::new(),
                status: Some("signal 9".into()),
            }
        );
        // A refusal is not in that frame and is shown as itself.
        assert_eq!(
            result("terminal", "terminal did not run the command: no"),
            Detail::Result("terminal did not run the command: no".into())
        );
    }

    #[test]
    fn a_files_lines_lose_their_envelope_and_their_tabs_become_four_cells() {
        assert_eq!(
            result(
                "read_file",
                "<path>a.txt</path>\n<content>\n1\talpha\n2\tbeta\n</content>"
            ),
            Detail::Result("1    alpha\n2    beta".into())
        );
        // `tool_result_display.zig:25-29`: anything else is itself.
        assert_eq!(
            result("read_file", "<path>a</path>\n<content>missing close"),
            Detail::Result("<path>a</path>\n<content>missing close".into())
        );
        assert_eq!(
            result("glob_files", "plain output"),
            Detail::Result("plain output".into())
        );
    }

    #[test]
    fn a_long_result_keeps_its_head_and_says_how_much_more_the_log_holds() {
        let long = "é".repeat(RESULT_BYTES);
        let Detail::Result(kept) = result("glob_files", &long) else {
            panic!("not a result");
        };
        assert!(kept.len() < RESULT_BYTES + 64);
        assert!(kept.ends_with(&format!(
            "\u{2026} {} more bytes are in the session's log",
            long.len() - RESULT_BYTES
        )));
    }

    #[test]
    fn a_detail_is_made_inert_before_it_reaches_a_row() {
        let detail = Detail::Arguments(vec![Argument {
            name: "p\u{1b}[2J".into(),
            value: "v\u{1b}]0;x\u{7}".into(),
            text: true,
        }])
        .made_inert();
        let Detail::Arguments(arguments) = detail else {
            panic!("not arguments");
        };
        // Spelled, not blanked: fx's `\xNN` (`terminalSafeToken`,
        // `text_utils.zig:643-667@481104fd`).
        assert_eq!(arguments[0].name, "p\\x1b[2J");
        assert_eq!(arguments[0].value, "v\\x1b]0;x\\x07");
        // A command's output that never went through `command_stream` (made
        // here by hand) is spelled the same; its line break and tab stay.
        let Detail::Command { output, status } = Detail::Command {
            output: "a\u{1b}[31mb\nc\td\u{202e}".into(),
            status: Some("s\u{7}".into()),
        }
        .made_inert() else {
            panic!("not a command");
        };
        assert_eq!(output, "a\\x1b[31mb\nc\td\\u{202e}");
        assert_eq!(status.as_deref(), Some("s\\x07"));
        // And it is idempotent: what it writes is printable ASCII.
        let again = Detail::Command {
            output: output.clone(),
            status: status.clone(),
        }
        .made_inert();
        assert_eq!(again, Detail::Command { output, status });
    }

    #[test]
    fn a_commands_stream_is_read_as_fxs_decoder_reads_it() {
        // `command_output_content.zig:52-282@481104fd`, one case per branch.
        for (raw, shown) in [
            // Sequences dropped whole: CSI, OSC to BEL and to ST, ESC + Fe.
            ("a\u{1b}[2Jb\u{1b}[31mc", "abc"),
            ("a\u{1b}]0;title\u{7}b\u{1b}]8;;x\u{1b}\\c", "abc"),
            ("a\u{1b}Mb\u{1b}7c", "ab\\x1b7c"),
            // A sequence a control interrupts, or the stream ends inside, is
            // written back, its controls `\xNN`; a line feed inside a CSI is
            // the sequence's, not a line's end.
            ("a\u{1b}[2\u{7}b", "a\\x1b[2\\x07b"),
            ("a\u{1b}[2\nb", "a\\x1b[2\\x0ab"),
            ("a\u{1b}[12", "a\\x1b[12"),
            ("a\u{1b}", "a\\x1b"),
            // `ESC ESC [`: the first written back, the second a sequence.
            ("a\u{1b}\u{1b}[1mb", "a\\x1bb"),
            // Carriage returns: CRLF a line's end, a bare one the line again,
            // the stream's last one dropped.
            ("one\r\ntwo", "one\ntwo"),
            ("over\rX\nnext", "X\nnext"),
            ("a\rb\rc", "c"),
            ("done\r", "done"),
            ("x\r\u{1b}[2Ky", "y"),
            // Tabs and printable ASCII themselves, other controls `\xNN`, C1
            // `\u{00NN}`, the rest itself (the marks are `detail_spelled`'s).
            ("a\tb\u{7}c\u{7f}d", "a\tb\\x07c\\x7fd"),
            ("e\u{9b}f\u{85}", "e\\u{009b}f\\u{0085}"),
            ("\u{202e}\u{e9}", "\u{202e}\u{e9}"),
        ] {
            assert_eq!(command_stream(raw), shown, "{raw:?}");
        }
        // A sequence that runs past what fx keeps of it ends `...`.
        let long = format!("\u{1b}[{}", "1;".repeat(40));
        let kept = command_stream(&long);
        assert!(kept.ends_with("..."), "{kept:?}");
        assert_eq!(
            kept.len(),
            "\\x1b".len() + ESCAPE_KEPT_BYTES - 1 + "...".len()
        );
        // At the 64th byte: `ESC ]` and 61 bytes are 63, `é` would be the 64th
        // and 65th, so it ends what is kept -- and the `b` after it, which
        // would fit, is not kept either, as fx keeps nothing past its 64th
        // byte (`recordEscapeByte`, `:255-262`). fx keeps `é`'s first byte;
        // a character here is kept whole or not at all.
        let osc = format!("\u{1b}]{}\u{e9}b", "a".repeat(61));
        assert_eq!(command_stream(&osc), format!("\\x1b]{}...", "a".repeat(61)));
        // Each stream is its own: stdout's carriage return does not reach
        // into stderr.
        let framed = "<command>x</command>\n<cwd>.</cwd>\n<exit_code>0</exit_code>\n<stdout>\nout\r</stdout>\n<stderr>\nerr\n</stderr>\n";
        assert_eq!(
            result("terminal", framed),
            Detail::Command {
                output: "outerr".into(),
                status: None,
            }
        );
    }

    // -- T-U1-SAFE: what a terminal would act on, in a call's detail rows ----

    /// `p5-transcript-safe`'s step: a command whose own text carries a
    /// right-to-left override, a zero-width space, a byte-order mark, an
    /// `ESC[2J`, the C1 `U+009B`, a tab, the Arabic letter mark, a
    /// right-to-left isolate and -- after a comment -- a trailing carriage
    /// return, printing a file that holds the same and ends its line on CRLF,
    /// then a line a bare carriage return rewrites; then a read of that file.
    /// Each detail as the bridge hands it over (`made_inert`).
    fn unsafe_detail_record() -> FullTranscript {
        let command_text =
            ": 'A\u{202e}B\u{200b}C\u{feff}D\u{1b}[2JE\u{9b}F\tG\u{61c}H\u{2067}I'; cat safe.txt # \r";
        let file = "a\u{202e}b\u{200b}c\u{feff}d\u{1b}[2Je\u{9b}f\tg\u{61c}h\u{2067}i\r\nover\rX\n";
        let mut record = record();
        let detail = |record: &mut FullTranscript, id: &str, detail: Detail| {
            record.observe(&UiEvent::ToolDetail {
                call_id: id.into(),
                detail: detail.made_inert(),
            });
        };
        let input = json!({"action": "exec", "command": command_text});
        record.tool_started("c", &command(command_text));
        detail(
            &mut record,
            "c",
            Detail::Arguments(arguments("terminal", &input)),
        );
        // `crate::tools::terminal`'s frame, as `p5-transcript-safe`'s
        // gateway log has it.
        let framed = format!(
            "<command>x</command>\n<cwd>.</cwd>\n<exit_code>0</exit_code>\n<stdout>\n{file}</stdout>\n<stderr>\n</stderr>\n"
        );
        detail(&mut record, "c", result("terminal", &framed));
        record.tool_finished("c", true, "", Outcome::Exited(0), None);
        let input = json!({"path": "safe.txt"});
        record.tool_started("r", &read("safe.txt"));
        detail(
            &mut record,
            "r",
            Detail::Arguments(arguments("read_file", &input)),
        );
        let read = "<path>safe.txt</path>\n<content>\n1\ta\u{202e}b\u{200b}c\u{feff}d\u{1b}[2Je\u{9b}f\tg\u{61c}h\u{2067}i\r\n2\tover\rX\n</content>";
        detail(&mut record, "r", result("read_file", read));
        record.tool_finished("r", true, "", Outcome::Finished, None);
        record
    }

    /// A character no detail row may hand the terminal as itself: a control
    /// other than the escape (which [`only_colours`] reads), or a mark fx
    /// names (`isNonPrintingCodepoint`, `text_utils.zig:703-708@481104fd`) or
    /// this screen does (`U+061C`).
    fn obeyed_or_hidden(character: char) -> bool {
        let code = u32::from(character);
        (code < 0x20 && character != '\u{1b}')
            || code == 0x7f
            || (0x80..=0x9f).contains(&code)
            || (0x200b..=0x200f).contains(&code)
            || (0x2028..=0x202e).contains(&code)
            || (0x2060..=0x206f).contains(&code)
            || code == 0xfeff
            || code == 0x061c
    }

    /// Whether every escape in `row` is a colour (`ESC[…m`), the only ones
    /// this screen writes.
    fn only_colours(row: &str) -> bool {
        let mut rest = row;
        while let Some(at) = rest.find('\u{1b}') {
            rest = &rest[at..];
            match super::super::pacer::escape_at(rest) {
                Some(len) if rest[..len].starts_with("\u{1b}[") && rest[..len].ends_with('m') => {
                    rest = &rest[len..];
                }
                _ => return false,
            }
        }
        true
    }

    #[test]
    fn an_argument_row_is_spelled_as_fxs_terminal_safe_writer_spells_it() {
        // `p5-transcript-safe`, fx v0.0.16's Ctrl-O: `appendFullSemanticArguments`
        // writes `name: value` through `TerminalSafeIndentedWriter`
        // (`full_transcript_screen.zig:6681-6695@481104fd`, `:5147-5194`): a tab
        // four spaces, every other control `\xNN` -- the trailing carriage
        // return too -- and the marks `\u{NNNN}` (`terminalSafeToken`,
        // `text_utils.zig:643-667@481104fd`), the isolate among them; the
        // Arabic letter mark, which fx writes as itself, is named too
        // (`ctrl-o-command-marks`).
        let rows = screen(&unsafe_detail_record());
        let row = "\u{2502}  command: : 'A\\u{202e}B\\u{200b}C\\u{feff}D\\x1b[2JE\\u{009b}F    G\\u{061c}H\\u{2067}I'; cat safe.txt # \\x0d";
        assert!(rows.iter().any(|shown| shown == row), "{rows:#?}");
    }

    #[test]
    fn a_read_row_is_spelled_as_fxs_terminal_safe_writer_spells_it() {
        // The same writer for a tool's result (`appendStoredResultContent`,
        // `:5196-5229@481104fd`, and the inline result, `:6856-6864`): each
        // line's carriage return `\x0d`, the bare one too.
        let rows = screen(&unsafe_detail_record());
        for row in [
            "\u{2502}  1    a\\u{202e}b\\u{200b}c\\u{feff}d\\x1b[2Je\\u{009b}f    g\\u{061c}h\\u{2067}i\\x0d",
            "\u{2502}  2    over\\x0dX",
        ] {
            assert!(
                rows.iter().any(|shown| shown == row),
                "{row:?} in {rows:#?}"
            );
        }
    }

    #[test]
    fn a_command_row_is_decoded_as_fxs_command_output_is_and_names_the_marks() {
        // fx's command output goes through its `Decoder`
        // (`command_output_content.zig:52-282@481104fd`) instead: an escape
        // sequence is dropped, a bare carriage return starts the line again,
        // CRLF ends it, the C1 control is `\u{009b}`, and the tab is kept to
        // the next eight-column stop of the screen (`wrapLiteralCommandOutput`,
        // `assistant_wrap.zig:524-546@481104fd`). fx leaves the marks as they
        // are (`:171-173`); here they are named as on every other detail row
        // (`scripts/fxcap/parity-ledger.toml`, `ctrl-o-command-marks`).
        let rows = screen(&unsafe_detail_record());
        let at = rows
            .iter()
            .position(|row| row.starts_with("\u{2502} a"))
            .unwrap_or_else(|| panic!("the output's first row in {rows:#?}"));
        assert_eq!(
            rows[at..at + 2],
            [
                "\u{2502} a\\u{202e}b\\u{200b}c\\u{feff}de\\u{009b}f        g\\u{061c}h\\u{2067}i",
                "\u{2502} X",
            ]
        );
    }

    #[test]
    fn no_detail_row_hands_the_terminal_a_control_or_a_mark_at_any_width() {
        let record = unsafe_detail_record();
        for cols in [10u16, 24, 80, 120] {
            // Every row the record lays out at this width, not one page of it.
            let (total, _) = record.measure(cols, DARK);
            let rows = record.window(0, total, cols, DARK);
            assert_eq!(rows.len(), total, "{cols}");
            if cols == 10 {
                assert!(total > Layout::of(40).viewport, "{total} rows fit one page");
            }
            for row in &rows {
                assert!(wrap::width(row) <= cols, "{cols}: {row:?}");
            }
            // The detail rows: the rail, then the detail's grey (`railed`,
            // `literal`). Not the calls' own rows, whose wrapped rows stand
            // under the rail too (`tool_status`'s `│ `, which the whole layout
            // shows at 10 columns): those are CALLROW-SAFE's
            // (`toolgroup`'s `no_calls_row_hands_the_terminal_a_control_or_a_mark_at_any_width`).
            let detail = format!("\u{2502}{}", DARK.dim());
            let details: Vec<&String> =
                rows.iter().filter(|row| row.starts_with(&detail)).collect();
            assert!(details.len() >= 5, "{cols}: {details:?}");
            for row in details {
                assert!(!row.chars().any(obeyed_or_hidden), "{cols}: {row:?}");
                assert!(only_colours(row), "{cols}: {row:?}");
            }
        }
    }

    #[test]
    fn a_spelled_token_may_be_cut_and_every_piece_of_it_is_inert_ascii() {
        // No spelling is kept whole. A command's C1 control is spelled by
        // `command_stream` before the result is bounded, so the bound can cut
        // `\u{009b}` to `\u{`; what is left is three printable characters.
        let raw = format!("{}\u{9b}", "a".repeat(RESULT_BYTES - 3));
        let framed = format!(
            "<command>x</command>\n<cwd>.</cwd>\n<exit_code>0</exit_code>\n<stdout>\n{raw}\n</stdout>\n<stderr>\n</stderr>\n"
        );
        let Detail::Command { output, .. } = result("terminal", &framed).made_inert() else {
            panic!("not a command");
        };
        let (kept, more) = output.split_once('\n').expect("the bound's line");
        assert_eq!(kept.len(), RESULT_BYTES);
        assert!(kept.ends_with("a\\u{"), "{:?}", &kept[kept.len() - 8..]);
        assert!(kept.chars().all(|c| c.is_ascii_graphic()));
        assert_eq!(more, "\u{2026} 5 more bytes are in the session's log");
        // A result is bounded first and spelled after, so its cut falls on
        // the file's own characters: an `ESC` cut from its `[2J` is `\x1b`.
        let raw = format!("{}\u{1b}[2J", "a".repeat(RESULT_BYTES - 1));
        let Detail::Result(text) = result("read_file", &raw).made_inert() else {
            panic!("not a result");
        };
        let (kept, _) = text.split_once('\n').expect("the bound's line");
        assert!(kept.ends_with("a\\x1b"), "{:?}", &kept[kept.len() - 8..]);
        assert!(!text.contains('\u{1b}'));
        // And a row can break inside a spelling: each piece is ASCII, and
        // the rows read back to it.
        for (rows, width) in [
            (railed("\\u{202e}", " ", 6, DARK), 6),
            (literal("\\u{202e}", 7, DARK), 7),
        ] {
            let rows = plain(&rows);
            assert!(rows.len() > 1, "{rows:?}");
            let joined: String = rows
                .iter()
                .map(|row| row.trim_start_matches('\u{2502}').trim_start())
                .collect();
            assert_eq!(joined, "\\u{202e}", "{rows:?}");
            for row in &rows {
                assert!(wrap::width(row) <= width, "{row:?}");
                assert!(row
                    .chars()
                    .skip(1)
                    .all(|c| c.is_ascii_graphic() || c == ' '));
            }
        }
    }

    #[test]
    fn a_detail_row_wraps_under_its_rail_and_never_overruns_the_screen() {
        let rows = railed(&"word ".repeat(30), "  ", 40, DARK);
        assert!(rows.len() > 1);
        for row in &rows {
            assert!(row.starts_with("\u{2502}\u{1b}[38;5;245m  "), "{row:?}");
            assert!(wrap::width(row) <= 40, "{row:?}");
        }
        assert_eq!(
            plain(&railed("a\n\nb", " ", 40, DARK)),
            ["\u{2502} a", "\u{2502} ", "\u{2502} b"]
        );
    }

    #[test]
    fn a_command_lines_tab_loses_no_character_when_its_word_moves_down() {
        // T-U1-SAFE review (gpt6, MUST 1): a tab measured where it stood, then
        // moved down with its word by the reflow (`reflowAtWordBreak`,
        // `assistant_wrap.zig:729-775@481104fd`), must not be drawn wider
        // where it lands and cut a character off the row's end.
        for line in [
            "aaa b\t1234567890123",
            "aaaa bbbbbb\t123456789012",
            "aaaaaaaaaa b\t12345678901234567890",
            "aa b\tc\td 12345678901234567 e\tf",
        ] {
            let sent: String = line.chars().filter(|c| !c.is_whitespace()).collect();
            for cols in 8u16..=48 {
                let rows = plain(&railed(line, " ", cols, DARK));
                let kept: String = rows
                    .iter()
                    .flat_map(|row| row.chars().skip(1))
                    .filter(|c| !c.is_whitespace())
                    .collect();
                assert_eq!(kept, sent, "{line:?} at {cols}: {rows:#?}");
                for row in &rows {
                    assert!(wrap::width(row) <= cols, "{line:?} at {cols}: {row:?}");
                }
            }
        }
        // fx's own row for the review's case: fx's wrap writes a tab as the
        // spaces to its stop where it meets it (`assistant_wrap.zig:163-174`),
        // so the word it moves down keeps that one space
        // (`p5-transcript-safetab`).
        assert_eq!(
            plain(&railed("aaa b\t1234567890123", " ", 20, DARK)),
            ["\u{2502} aaa", "\u{2502} b 1234567890123"]
        );
    }

    #[test]
    fn the_journal_tee_records_first_and_hands_over_what_the_document_never_gets() {
        struct Kept(Vec<SessionEvent>);
        impl TurnJournal for Kept {
            fn record(&mut self, event: SessionEvent) {
                self.0.push(event);
            }

            fn change_made(&mut self, _call_id: &str, _change: &crate::permission::ApprovalDiff) {}

            fn presented(&mut self, _call_id: &str, _tool: &str, _text: &str) {}

            fn attach_images(
                &mut self,
                _images: &[crate::image::Image],
            ) -> Option<std::path::PathBuf> {
                None
            }

            fn attach_tool_images(&mut self, _call_id: &str, _images: &[crate::image::ToolImage]) {}

            fn state(&self) -> Option<&crate::session::DurableState> {
                None
            }
        }
        let (events, mut received) = tokio::sync::mpsc::channel(8);
        let mut kept = Kept(Vec::new());
        {
            let mut teed = Teed::new(&mut kept, events, CancellationToken::new());
            teed.record(SessionEvent::UserMessage { text: "hi".into() });
            teed.record(SessionEvent::AssistantMessage {
                text: String::new(),
                tool_calls: vec![crate::session::RecordedToolCall {
                    id: "c".into(),
                    name: "read_file".into(),
                    input: json!({"path": "a\tb"}),
                }],
                raw_content: Vec::new(),
                responses_state: Vec::new(),
                wire: None,
                model: None,
            });
            teed.record(SessionEvent::ToolResult {
                ending: None,
                lines: None,
                change: None,
                call_id: "c".into(),
                tool: "read_file".into(),
                ok: true,
                output: "<path>a</path>\n<content>\n1\tx\n</content>".into(),
            });
        }
        assert_eq!(kept.0.len(), 3, "a record was not journalled");
        assert_eq!(
            received.try_recv().expect("the arguments"),
            UiEvent::ToolDetail {
                call_id: "c".into(),
                detail: Detail::Arguments(vec![Argument {
                    name: "path".into(),
                    value: "a    b".into(),
                    // The heading spells the tab `\x09`, so it never shows the
                    // value as written.
                    text: false,
                }]),
            }
        );
        assert_eq!(
            received.try_recv().expect("the result"),
            UiEvent::ToolDetail {
                call_id: "c".into(),
                detail: Detail::Result("1    x".into()),
            }
        );
        assert!(
            received.try_recv().is_err(),
            "a user message is not a detail"
        );

        // A change goes to the screen alone: the log keeps no diff.
        let (events, mut received) = tokio::sync::mpsc::channel(8);
        let mut kept = Kept(Vec::new());
        Teed::new(&mut kept, events, CancellationToken::new())
            .change_made("w", &ApprovalDiff::of("old\n", "new\u{1b}[2J\n"));
        assert!(kept.0.is_empty(), "a change was journalled");
        assert_eq!(
            received.try_recv().expect("the change"),
            UiEvent::ToolDetail {
                call_id: "w".into(),
                detail: Detail::Change(ApprovalDiff::of("old\n", "new\u{1b}[2J\n")).made_inert(),
            }
        );

        // A stopped command's account goes to the screen alone, read as a
        // command's: what it wrote and the signal it died of
        // (`fx-resume-tools-40x120-live-ctrl-o`: `│ signal 15`).
        let (events, mut received) = tokio::sync::mpsc::channel(8);
        let mut kept = Kept(Vec::new());
        Teed::new(&mut kept, events, CancellationToken::new()).presented(
            "t",
            "terminal",
            "<command>sleep 4</command>\n<cwd>.</cwd>\n<signal>15</signal>\n<stdout>\nstarted\n</stdout>\n<stderr>\n</stderr>\n",
        );
        assert!(kept.0.is_empty(), "the screen's account was journalled");
        assert_eq!(
            received.try_recv().expect("the account"),
            UiEvent::ToolDetail {
                call_id: "t".into(),
                detail: Detail::Command {
                    output: "started".into(),
                    status: Some("signal 15".into()),
                },
            }
        );

        // And one killed at its deadline: what it wrote, then `timed out`
        // (`fx-timeout-40x120-live-ctrl-o`).
        assert_eq!(
            result(
                "terminal",
                "<command>x</command>\n<cwd>.</cwd>\n<timed_out/>\n<stdout>\nstarted\n</stdout>\n<stderr>\n</stderr>\n",
            ),
            Detail::Command {
                output: "started".into(),
                status: Some("timed out".into()),
            }
        );

        // A cancelled turn still journals everything and hands nothing over.
        let (events, mut received) = tokio::sync::mpsc::channel(8);
        let cancel = CancellationToken::new();
        cancel.cancel();
        let mut kept = Kept(Vec::new());
        Teed::new(&mut kept, events, cancel).record(SessionEvent::ToolResult {
            ending: None,
            lines: None,
            change: None,
            call_id: "c".into(),
            tool: "glob_files".into(),
            ok: true,
            output: "x".into(),
        });
        assert_eq!(kept.0.len(), 1);
        assert!(received.try_recv().is_err());
    }

    #[test]
    fn every_row_fits_the_screen_at_every_width() {
        let mut record = record();
        record.welcome();
        record.prompt(&"long prompt ".repeat(20));
        record.answer(&format!(
            "{}\n\n```rust\nfn main() {{}}\n```\n",
            "prose ".repeat(40)
        ));
        record.tool_started("c", &command(&"x".repeat(300)));
        record.observe(&UiEvent::ToolDetail {
            call_id: "c".into(),
            detail: Detail::Command {
                output: "y".repeat(300),
                status: Some("exit code 1".into()),
            },
        });
        record.tool_finished("c", true, "", Outcome::Exited(1), None);
        record.notice(&Notice::error("system", "z".repeat(200)));
        record.open();
        for cols in [10u16, 24, 80, 120] {
            for row in record.rows(cols, 24, DARK, "") {
                assert!(wrap::width(&row) <= cols, "{cols}: {row:?}");
            }
        }
    }

    // -- v0.0.16: tool rows wrap (427d481f, 6f071dea, 78424464) -------------

    /// The whole screen with no colour, `cols` wide and 40 rows high.
    fn screen_at(record: &FullTranscript, cols: u16) -> Vec<String> {
        plain(&record.rows(cols, 40, DARK, ""))
    }

    /// The record's rows on that screen, without the footer under them (whose
    /// own text is cut with an ellipsis on a narrow screen).
    fn record_at(record: &FullTranscript, cols: u16) -> Vec<String> {
        let mut rows = screen_at(record, cols);
        rows.truncate(Layout::of(40).viewport);
        rows
    }

    fn with_arguments(record: &mut FullTranscript, id: &str, tool: &str, input: &Value) {
        record.observe(&UiEvent::ToolDetail {
            call_id: id.into(),
            detail: Detail::Arguments(arguments(tool, input)),
        });
    }

    #[test]
    fn a_long_heading_wraps_under_its_rail_instead_of_being_cut() {
        // `full projection wraps a long tool status instead of clipping it`
        // (`full_transcript_screen.zig:969-995@481104fd`).
        let mut record = record();
        record.tool_started(
            "c",
            &command("printf a deliberately long query that reaches FULL_STATUS_TAIL"),
        );
        let rows = record_at(&record, 24);
        let at = rows
            .iter()
            .position(|row| row.starts_with("\u{2514} Running printf a"))
            .expect("the heading");
        assert!(rows[at + 1].starts_with("  "), "{rows:?}");
        assert!(rows.concat().contains("FULL_STATUS_TAIL"), "{rows:?}");
        assert!(!rows.concat().contains('\u{2026}'), "{rows:?}");
        for row in record.rows(24, 40, DARK, "") {
            assert!(wrap::width(&row) <= 24, "{row:?}");
        }
    }

    #[test]
    fn a_heading_is_cut_only_where_fxs_own_bound_on_its_phrase_cuts_it() {
        // T-U1+U2's review found the heading cut at 1 KB where fx shows it
        // whole: fx never clips an expanded row (`formatExpandedChild`,
        // `tool_group_projection.zig:786-787@481104fd`) and bounds only the
        // phrase it is made of -- a command at 14 399 bytes
        // (`formatRunCommandDetailBounded`, `src/core/tooling/
        // tool_presentation.zig:432-447`, with `max_run_command_reflow_bytes`,
        // `:22`), a file change's path at 4 096 (`fileMutationDisplayPath`,
        // `src/core/agent/runtime/tool_presentation.zig:736-753`,
        // `diff.max_encoded_label_bytes`), each cut back to leave room for
        // `...` (`encodeTerminalSafe`). Every other argument fx writes raw at
        // any length (`formatToolAction`, `src/core/app/app_agent_runtime.zig:
        // 1248-1310`; `presentationLabelValue`, `src/core/tooling/
        // tool_dispatch.zig:626-647`): xfx spells it (ledger
        // `call-row-raw-argument`) and shows it whole too.
        let heading = |shown: Shown| -> String {
            let mut record = record();
            record.tool_started("c", &shown);
            record.tool_finished("c", true, "", Outcome::Finished, None);
            let rows = plain(&record.rows(120, 20_000, DARK, ""));
            let at = rows
                .iter()
                .position(|row| row.starts_with('\u{2514}'))
                .expect("the heading");
            let mut whole = rows[at].clone();
            for row in &rows[at + 1..] {
                let Some(rest) = row.strip_prefix("  ") else {
                    break;
                };
                whole.push_str(rest);
            }
            whole
        };
        let write = |path: &str| {
            crate::tools::mutate::WRITE_FILE.shown(&json!({"path": path, "content": "x"}))
        };
        let a = |count: usize| "a".repeat(count);
        for (shown, expected) in [
            (command(&a(14_399)), format!("\u{2514} Ran {}", a(14_399))),
            (
                command(&a(14_400)),
                format!("\u{2514} Ran {}...", a(14_396)),
            ),
            (write(&a(4_096)), format!("\u{2514} Wrote {}", a(4_096))),
            (write(&a(4_097)), format!("\u{2514} Wrote {}...", a(4_093))),
            (
                crate::tools::mutate::EDIT_FILE
                    .shown(&json!({"path": a(4_097), "old_string": "x", "new_string": "y"})),
                format!("\u{2514} Edited {}...", a(4_093)),
            ),
            // Past the old 1 KB cut and past a file change's bound: whole, as
            // on fx -- a read's path, a pattern, a listed directory, a new folder's path.
            (read(&a(3_000)), format!("\u{2514} Read {}", a(3_000))),
            (read(&a(20_000)), format!("\u{2514} Read {}", a(20_000))),
            (
                read::GLOB_FILES.shown(&json!({"pattern": a(20_000)})),
                format!("\u{2514} Matched {}", a(20_000)),
            ),
            (
                read::LIST_FILES.shown(&json!({"path": a(20_000)})),
                format!("\u{2514} Listed {}", a(20_000)),
            ),
            (
                crate::tools::mutate::CREATE_FOLDER.shown(&json!({"path": a(20_000)})),
                format!("\u{2514} Created {}", a(20_000)),
            ),
        ] {
            let got = heading(shown);
            assert!(
                got == expected,
                "{} bytes, ends {:?}, wanted {} ending {:?}",
                got.len(),
                &got[got.len().saturating_sub(12)..],
                expected.len(),
                &expected[expected.len() - 12..]
            );
        }
    }

    #[test]
    fn a_groups_rows_wrap_under_their_tree_rail_and_keep_the_commands_colours() {
        // `expanded rows wrap under their tree rail and keep command styling`
        // (`tool_group_projection.zig:2053-2131@481104fd`), at 24 columns.
        let first = "printf 'alpha beta gamma delta epsilon zeta eta theta'";
        let last = "printf 'iota kappa lambda mu nu xi omicron pi rho'";
        let mut record = record();
        for (id, text) in [("c1", first), ("c2", last)] {
            record.tool_started(id, &command(text));
            record.tool_finished(id, true, "", Outcome::Exited(0), None);
        }
        let painted = record.rows(24, 40, DARK, "");
        let rows = record_at(&record, 24);
        assert!(!rows.concat().contains('\u{2026}'));
        let header = rows
            .iter()
            .position(|row| row.starts_with('\u{25cf}'))
            .expect("the header");
        let first_child = rows
            .iter()
            .position(|row| row.starts_with('\u{251c}'))
            .expect("the first call");
        let last_child = rows
            .iter()
            .position(|row| row.starts_with('\u{2514}'))
            .expect("the last call");
        // The header itself is wider than 24 columns and wraps before the
        // first call, under two blanks.
        assert!(first_child > header + 1, "{rows:?}");
        assert!(rows[header + 1].starts_with("  "), "{rows:?}");
        assert!(last_child > first_child + 1, "{rows:?}");

        let geometry = super::super::layout::solve(40, 120, 1).expect("a band");
        let mut grid = super::super::grid::Grid::blank(40, 24);
        for (index, row) in painted.iter().enumerate() {
            grid.place_row(u16::try_from(index + 1).expect("line"), row, &geometry);
        }
        let colour =
            |line: usize, column: u16| match grid.cell(u16::try_from(line + 1).unwrap(), column) {
                Some(super::super::grid::Cell::Lead { sgr, .. }) => sgr.color().expect("a colour"),
                other => panic!("row {line}, column {column}: {other:?}"),
            };
        let last_content = |line: usize| colour(line, wrap::width(&rows[line]));
        // Each call is an entry of its own, under its own header: the first
        // call's rows end at the blank row before the second's.
        let first_end = first_child
            + rows[first_child..]
                .iter()
                .position(String::is_empty)
                .expect("the blank row after the first call");
        assert!(first_end > first_child + 1, "{rows:?}");
        // A call that is not the last continues under an unstyled rail, and
        // its quoted argument keeps the string's colour across every wrap.
        for (line, row) in rows
            .iter()
            .enumerate()
            .take(first_end)
            .skip(first_child + 1)
        {
            assert!(row.starts_with("\u{2502} "), "{row:?}");
            assert_eq!(colour(line, 1), super::super::check::Color::Default);
            let before = last_content(line - 1);
            assert_ne!(before, super::super::check::Color::Default, "row {line}");
            assert_eq!(colour(line, 3), before, "row {line}");
        }
        // The last call indents past its glyph.
        let end = rows[last_child..]
            .iter()
            .position(String::is_empty)
            .map_or(rows.len(), |blank| last_child + blank);
        assert!(end > last_child + 1, "{rows:?}");
        for row in &rows[last_child + 1..end] {
            assert!(row.starts_with("  ") && !row.starts_with("   "), "{row:?}");
        }
        // Nothing is hidden: each call's rows hold its whole command.
        let joined = |from: usize, to: usize| -> String {
            rows[from..to]
                .iter()
                .map(|row| row.chars().skip(2).collect::<String>())
                .collect::<Vec<_>>()
                .join(" ")
        };
        assert_eq!(joined(first_child, first_end), format!("Ran {first}"));
        assert_eq!(joined(last_child, end), format!("Ran {last}"));
    }

    #[test]
    fn a_groups_header_wraps_instead_of_being_cut() {
        // `expanded group headers wrap instead of clipping`
        // (`tool_group_projection.zig:2133-2162@481104fd`), at 16 columns.
        let mut record = record();
        record.tool_started("r", &read("a.zig"));
        record.tool_finished("r", true, "", Outcome::Finished, None);
        record.tool_started("c", &command("zig build"));
        record.tool_finished("c", true, "", Outcome::Exited(1), None);
        let rows = record_at(&record, 16);
        let header = rows
            .iter()
            .position(|row| row.starts_with('\u{25cf}'))
            .expect("the header");
        let first_child = rows
            .iter()
            .position(|row| row.starts_with('\u{251c}'))
            .expect("the first call");
        assert!(first_child > header + 1, "{rows:?}");
        assert!(
            rows[header..first_child].concat().contains("failed"),
            "{rows:?}"
        );
        assert!(!rows.concat().contains('\u{2026}'));
    }

    #[test]
    fn arguments_beside_a_heading_wider_than_the_screen_keep_their_rows() {
        // `full projection keeps semantic arguments beside a wrapped tool
        // heading` (`full_transcript_screen.zig:1046-1087@481104fd`): a
        // heading wider than the screen never hides an argument.
        let text = "printf a deliberately long command with FULL_ARGUMENT_TAIL";
        let input = json!({"action": "exec", "command": text, "profile": "clean"});
        let mut record = record();
        with_arguments(&mut record, "c", "terminal", &input);
        record.tool_started("c", &command(text));
        record.tool_finished("c", true, "", Outcome::Exited(0), None);
        let rows = record_at(&record, 24).concat();
        assert!(rows.contains("FULL_ARGUMENT_TAIL"));
        assert!(!rows.contains('\u{2026}'));
        assert!(rows.contains("command: printf"), "{rows}");
        assert!(!rows.contains("action: exec"));
        assert!(rows.contains("profile: clean"));
        assert!(!rows.contains("{\"command\""));

        // The same call at a width its heading fits: the command is the
        // heading's, and has no row of its own.
        let rows = screen_at(&record, 120);
        assert!(rows
            .iter()
            .any(|row| row == &format!("\u{2514} Ran {text}")));
        assert!(!rows.concat().contains("command: printf"), "{rows:?}");
    }

    #[test]
    fn a_short_argument_a_long_heading_contains_by_chance_keeps_its_row() {
        // `full projection keeps short session input that a long heading
        // contains by chance` (`full_transcript_screen.zig:1089-1128@481104fd`,
        // a terminal session's `chars: y` inside `python3`). xfx's terminal
        // has no sessions; the rule is the same for any short text value.
        let launch = "cd . && python3 - <<'PYEOF' import pathlib led = pathlib.Path(\"docs/workflows/ledger.md\"); s = led.read_text() old = \"Fixed in draft\" assert old in s";
        let input = json!({"action": "exec", "command": launch, "chars": "y"});
        let mut record = record();
        with_arguments(&mut record, "c", "terminal", &input);
        record.tool_started("c", &command(launch));
        record.tool_finished("c", true, "", Outcome::Exited(0), None);
        let rows = screen_at(&record, 80).concat();
        assert!(rows.contains("Ran cd . && python3"), "{rows}");
        assert!(rows.contains("chars: y"), "{rows}");
        // The rule's other half: an empty value is never "shown already".
        let input = json!({"action": "exec", "command": "ls", "note": ""});
        let mut record = self::record();
        with_arguments(&mut record, "c", "terminal", &input);
        record.tool_started("c", &command("ls"));
        record.tool_finished("c", true, "", Outcome::Exited(0), None);
        assert!(
            screen_at(&record, 80)
                .iter()
                .any(|row| row == "\u{2502}  note:"),
            "{:?}",
            screen_at(&record, 80)
        );
    }

    #[test]
    fn tool_rows_fit_the_narrowest_screens() {
        // A prefix wider than the room is left off, and a wide character a
        // row cannot hold is a `?` (`transcript_blocks.zig:922-923,941@481104fd`).
        let mut record = record();
        record.tool_started("c", &command("echo \u{65e5}\u{672c} wide words"));
        record.tool_started("d", &command("ls"));
        for cols in 1u16..=6 {
            let rows = record.rows(cols, 40, DARK, "");
            for row in &rows {
                assert!(wrap::width(row) <= cols, "{cols}: {row:?}");
            }
        }
        assert!(screen_at(&record, 1).concat().contains('?'));
    }

    // -- v0.0.16: the reading position holds (026a8f4e, b1a53b01) ----------

    /// `p5-transcript-anchormid`'s record: the session's, a prompt, five reads
    /// of twelve lines each and a command whose `Running` heading is wider
    /// than 120 columns and whose `Ran` one is not.
    fn reading_session(prompt: &str) -> FullTranscript {
        let mut record = record();
        record.welcome();
        record.full_detail(Notice::neutral(
            "session",
            "provider: gateway \u{b7} model: fake/model-a\nmcp: none",
        ));
        record.prompt(prompt);
        for name in ["a", "b", "c", "d", "e"] {
            let id = format!("r{name}");
            record.tool_started(&id, &read(&format!("{name}.txt")));
            let lines: String = (1..=12)
                .map(|n| format!("{name} line {n} of the {name} file\n"))
                .collect();
            record.observe(&UiEvent::ToolDetail {
                call_id: id.clone(),
                detail: Detail::Result(lines),
            });
            record.tool_finished(&id, true, "", Outcome::Finished, None);
        }
        let words: Vec<String> = (0..24).map(|n| format!("w{n:02}")).collect();
        let long = format!("sleep 9; echo  {} zz", words.join(" "));
        with_arguments(
            &mut record,
            "c",
            "terminal",
            &json!({"action": "exec", "command": long}),
        );
        record.tool_started("c", &command(&long));
        record
    }

    /// What arrives while the reader is a page up: the command settles (its
    /// heading loses a row), a `* network:` record lands above every entry,
    /// and the answer lands below.
    fn settle_under(record: &mut FullTranscript) {
        record.full_detail(Notice::neutral(
            "network",
            "provider: gateway \u{b7} model: fake/model-a \u{b7} 14ms\nfinish: tool_calls\ntokens: 1200 in \u{b7} 40 out",
        ));
        record.tool_finished("c", true, "", Outcome::Exited(0), None);
        record.answer("Done reading.");
        record.turn_over();
    }

    #[test]
    fn the_reading_position_holds_when_the_page_rebuilds_above_and_below_it() {
        // fx v0.0.16 `p5-transcript-anchormid` `full-pgup` and
        // `full-pgup-after`: row 1 is `│  9     a line 9 of the a file` in
        // both; c1db919 jumped five rows to `a line 3`.
        let mut record = reading_session("read and wait");
        record.open();
        let tail = screen(&record);
        assert_eq!(record.key(Action::PageUp, 120, 40, DARK), Reply::Moved);
        let before = screen(&record);
        settle_under(&mut record);
        let after = screen(&record);
        // The hold first, so that a build without the wrap (one whose
        // heading never changes height) is held to it too.
        assert_eq!(after[..36], before[..36]);
        // And it still moves from where it is.
        assert_eq!(record.key(Action::Down, 120, 40, DARK), Reply::Moved);
        assert_eq!(screen(&record)[0], before[3]);
        // What the rebuild changed below the view: the `Running` heading,
        // two rows wrapped, is a `Ran` one of one row.
        let heading = tail
            .iter()
            .position(|row| row.starts_with("\u{2514} Running sleep 9;"))
            .expect("the heading");
        assert_eq!(tail[heading + 1], "  zz", "the heading wraps to two rows");
    }

    #[test]
    fn a_position_inside_the_records_keeps_its_offset() {
        // b1a53b01: the records share one id, so a reader among them keeps
        // the row offset; a record that lands after them moves nothing above.
        let mut record = reading_session("read and wait");
        record.full_detail(Notice::neutral(
            "network",
            "first request\nfinish: tool_calls",
        ));
        record.open();
        while record.key(Action::PageUp, 120, 40, DARK) == Reply::Moved {}
        let top = screen(&record);
        // One row into the second record: `finish: tool_calls`.
        let at = top
            .iter()
            .position(|row| row == "  finish: tool_calls")
            .expect("the second record's second row");
        let view = record.view.as_ref().expect("open");
        view.reading.set(Reading {
            scroll_rows: at,
            follow_tail: false,
            ..Reading::default()
        });
        let before = screen(&record);
        assert_eq!(before[0], "  finish: tool_calls");
        // A third record lands after the second, the command settles and the
        // answer lands: the reader is not sent back to the first record.
        settle_under(&mut record);
        let after = screen(&record);
        assert_eq!(after[..2], before[..2], "{after:?}");
        assert!(after[2].contains("Notice"), "{after:?}");
    }

    #[test]
    fn a_reader_among_the_records_keeps_its_row_when_the_width_changes() {
        // fx v0.0.16 `p5-transcript-records`: from the top, Down once puts
        // the `* session:` record's first row at row 1; at 80 columns and
        // back at 120 it is still row 1 (`a_width_change_is_a_rebuild` in
        // `position`'s docs).
        let mut record = reading_session("read and wait");
        settle_under(&mut record);
        record.open();
        while record.key(Action::PageUp, 120, 40, DARK) == Reply::Moved {}
        assert_eq!(record.key(Action::Down, 120, 40, DARK), Reply::Moved);
        let wide = screen(&record);
        assert!(wide[0].starts_with("* session:"), "{wide:?}");
        let narrow = screen_at(&record, 80);
        assert!(narrow[0].starts_with("* session:"), "{narrow:?}");
        assert_eq!(screen(&record)[0], wide[0]);
    }

    #[test]
    fn a_width_change_keeps_the_entry_the_reader_was_in() {
        // A width change is a rebuild: the position is pinned on the page
        // laid out at the old width and found on the new one
        // (`presentationForRebuiltPage`, `transcript/runtime.zig:
        // 11584-11590@481104fd`). At 80 columns the prompt above the view
        // takes another row and the answer below it two more; the reader
        // stays in the same line of the same read.
        let mut record = reading_session(&"read and wait ".repeat(14));
        record.answer(&"tail words ".repeat(30));
        record.open();
        record.key(Action::PageUp, 120, 40, DARK);
        let wide = screen(&record);
        let narrow = screen_at(&record, 80);
        assert_eq!(narrow[0], wide[0]);
        assert_eq!(screen(&record)[..36], wide[..36]);
        // At the tail, a width change keeps the tail.
        while record.key(Action::PageDown, 120, 40, DARK) == Reply::Moved {}
        let narrow = screen_at(&record, 80);
        assert!(narrow[35].trim_end().ends_with("tail words"), "{narrow:?}");
    }

    // -- CALLROW-SAFE: a call's own row, this screen's heading --------------

    /// `p5-call-row-safe`'s step: a command, a glob pattern and a read path,
    /// each with a right-to-left override, a left-to-right isolate, the
    /// Arabic letter mark, `ESC[2J`, a bare carriage return, a tab and the C1
    /// `U+009B`, and each call's arguments as the bridge hands them over. The
    /// read fails; its reason quotes the path.
    fn unsafe_heading_record() -> FullTranscript {
        let command_text =
            "printf '%s' 'A\u{202e}B\u{2066}C\u{061c}D\u{1b}[2JE\rF\tG\u{9b}H' >/dev/null";
        let pattern = "n\u{202e}o\u{2066}p\u{061c}q\u{1b}7r\ts\u{9b}t\ru*.md";
        let path = "m\u{202e}n\u{2066}o\u{061c}p\u{1b}[2Jq\tr\u{9b}s\rt.txt";
        let mut record = record();
        let calls = [
            (
                "c",
                "terminal",
                json!({"action": "exec", "command": command_text}),
                command(command_text),
            ),
            (
                "g",
                "glob_files",
                json!({ "pattern": pattern }),
                read::GLOB_FILES.shown(&json!({ "pattern": pattern })),
            ),
            ("r", "read_file", json!({ "path": path }), read(path)),
        ];
        for (id, tool, input, shown) in &calls {
            record.tool_started(id, shown);
            record.observe(&UiEvent::ToolDetail {
                call_id: (*id).into(),
                detail: Detail::Arguments(arguments(tool, input)).made_inert(),
            });
        }
        record.tool_finished("c", true, "", Outcome::Exited(0), None);
        record.tool_finished("g", true, "", Outcome::Finished, None);
        let reason = format!("Path not found: {path}");
        record.tool_finished("r", false, &reason, Outcome::Finished, None);
        record
    }

    #[test]
    fn a_calls_heading_is_spelled_as_its_transcript_row_is_at_any_width() {
        // fx's heading here is the status phrase its transcript row is made
        // from (`formatExpandedChild`, `tool_group_projection.zig:788-820
        // @481104fd`), wrapped and never cut: the command as
        // `encodeTerminalSafe` wrote it (`p5-call-row-safe-…-full`). The
        // pattern and the path are spelled the same way here, where fx's
        // are raw (ledger `call-row-raw-argument`); so is the Arabic letter
        // mark. The argument rows stay: fx finds none of the three raw values
        // in its heading (`argumentVisibleInToolHeading`).
        let record = unsafe_heading_record();
        for cols in [10u16, 24, 80, 120] {
            for row in record.rows(cols, 400, DARK, "") {
                assert!(wrap::width(&row) <= cols, "{cols}: {row:?}");
                assert!(!row.chars().any(obeyed_or_hidden), "{cols}: {row:?}");
                assert!(only_colours(&row), "{cols}: {row:?}");
            }
        }
        let rows = plain(&record.rows(240, 400, DARK, ""));
        for row in [
            r"├ Ran printf '%s' 'A\u{202e}B\u{2066}C\u{061c}D\x1b[2JE F\x09G\u{009b}H' >/dev/null",
            r"│  command: printf '%s' 'A\u{202e}B\u{2066}C\u{061c}D\x1b[2JE\x0dF    G\u{009b}H' >/dev/null",
            r"├ Matched n\u{202e}o\u{2066}p\u{061c}q\x1b7r\x09s\u{009b}t\x0du*.md",
            r"│  pattern: n\u{202e}o\u{2066}p\u{061c}q\x1b7r    s\u{009b}t\x0du*.md",
            r"└ Failed m\u{202e}n\u{2066}o\u{061c}p\x1b[2Jq\x09r\u{009b}s\x0dt.txt: Path not found: m\u{202e}n\u{2066}o\u{061c}p\x1b[2Jq r\u{009b}s t.txt",
            r"│  path: m\u{202e}n\u{2066}o\u{061c}p\x1b[2Jq    r\u{009b}s\x0dt.txt",
        ] {
            assert!(rows.iter().any(|shown| shown == row), "{row}\n{rows:#?}");
        }
    }

    #[test]
    fn a_value_the_heading_had_to_spell_is_never_shown_by_it_already() {
        // `argumentVisibleInToolHeading` (`full_transcript_screen.zig:
        // 6594-6619@481104fd`) looks for the value as the model wrote it in
        // the heading fx spelled: one that holds anything `encodeTerminalSafe`
        // names -- a mark with no control beside it, here -- is never found,
        // and keeps its row. The heading is spelled here too, so the value is
        // judged as it was written, not as this screen spells it
        // (`Argument::text`). The Arabic letter mark, which fx writes as
        // itself, is named in this heading, so its value keeps its row too.
        for (tool, input, shown, row) in [
            (
                "terminal",
                json!({"action": "exec", "command": "echo A\u{202e}B"}),
                command("echo A\u{202e}B"),
                Some(r"│  command: echo A\u{202e}B"),
            ),
            (
                "terminal",
                json!({"action": "exec", "command": "echo A\u{061c}B"}),
                command("echo A\u{061c}B"),
                Some(r"│  command: echo A\u{061c}B"),
            ),
            (
                "terminal",
                json!({"action": "exec", "command": "echo plain"}),
                command("echo plain"),
                None,
            ),
            (
                "read_file",
                json!({"path": "a\u{2066}b.txt"}),
                read("a\u{2066}b.txt"),
                Some(r"│  path: a\u{2066}b.txt"),
            ),
            (
                "read_file",
                json!({"path": "plain.txt"}),
                read("plain.txt"),
                None,
            ),
        ] {
            let mut record = record();
            record.tool_started("c", &shown);
            record.observe(&UiEvent::ToolDetail {
                call_id: "c".into(),
                detail: Detail::Arguments(arguments(tool, &input)).made_inert(),
            });
            record.tool_finished("c", true, "", Outcome::Exited(0), None);
            let rows = screen_at(&record, 120);
            let arguments: Vec<&String> = rows
                .iter()
                .filter(|line| line.starts_with("\u{2502}  "))
                .collect();
            match row {
                Some(row) => assert_eq!(arguments, [row], "{input}"),
                None => assert_eq!(arguments, Vec::<&String>::new(), "{input}"),
            }
        }
        // fx looks in the status entry's own bytes, whose command is its
        // first 120 spelled (`formatRunCommandActivity`,
        // `src/core/tooling/tool_presentation.zig:393-416@481104fd`), not in
        // the command the heading shows whole: past them the status ends
        // `...` and holds no whole command, so the command keeps its row on a
        // screen the heading fits, as on fx's.
        // A command of 120 bytes is still whole there; one of 121 is not.
        for (length, kept) in [(120, false), (121, true), (150, true)] {
            let text = format!("echo {}", "a".repeat(length - 5));
            let mut record = record();
            record.tool_started("c", &command(&text));
            with_arguments(
                &mut record,
                "c",
                "terminal",
                &json!({"action": "exec", "command": text}),
            );
            record.tool_finished("c", true, "", Outcome::Exited(0), None);
            let rows = screen_at(&record, 200);
            assert!(
                rows.contains(&format!("\u{2514} Ran {text}")),
                "{length}: {rows:#?}"
            );
            let row = format!("\u{2502}  command: {text}");
            assert_eq!(rows.contains(&row), kept, "{length}: {rows:#?}");
        }
    }

    #[test]
    fn a_command_read_only_in_part_hides_no_argument_fxs_status_would_show() {
        // CALLROW-SAFEc: fx's 120-byte status phrase marks nothing of a
        // command it stopped reading at 14 400 bytes (`formatRunCommandDetailBounded`,
        // `src/core/tooling/tool_presentation.zig:441-445@481104fd`, the
        // marker only the encoder's): `ls`, 14 400 line breaks and `x` is the
        // status `ls`, which holds no `.`, so `cwd: .` keeps its row on fx. The
        // row here ends `...` (`call-row-source-cut`), and a status that ended
        // so too would find the `.` in it.
        let text = format!("ls{}x", "\n".repeat(14_400));
        let input = json!({"action": "exec", "command": text, "cwd": "."});
        let mut record = record();
        record.tool_started("c", &command(&text));
        with_arguments(&mut record, "c", "terminal", &input);
        record.tool_finished("c", true, "", Outcome::Exited(0), None);
        // The whole record: the `command:` row is 14 400 rows of its own.
        let rows = plain(&record.rows(120, 20_000, DARK, ""));
        assert!(
            rows.contains(&"\u{2514} Ran ls...".to_string()),
            "{rows:#?}"
        );
        assert!(rows.contains(&"\u{2502}  cwd: .".to_string()), "{rows:#?}");
    }

    #[test]
    fn ctrl_o_builds_a_groups_block_once_and_lays_each_heading_out_once_for_what_it_shows() {
        // T-U4 round 3: each member's entry rebuilt the whole group and its
        // block -- every member's whole argument spelled again -- to use one
        // row of it, and laid its own heading out again on every change to
        // the group. The block is the group's, built once for its revision
        // and palette; a heading is laid out again only when its text or the
        // width changes.
        const LIGHT: Palette = Palette { mode: Mode::Light };
        let mut record = record();
        record.tool_started(
            "big",
            &read::GLOB_FILES.shown(&json!({ "pattern": "x".repeat(4000) })),
        );
        record.tool_started("ls", &command("ls -la src"));
        record.tool_finished("ls", true, "", Outcome::Finished, None);
        for index in 0..4 {
            let id = format!("r{index}");
            record.tool_started(&id, &read("a.txt"));
            record.tool_finished(&id, true, "", Outcome::Finished, None);
        }
        let _ = record.measure(120, DARK);
        assert_eq!(record.built.get(), (1, 6), "the first layout");
        let _ = record.measure(120, DARK);
        assert_eq!(record.built.get(), (1, 6), "laid out already");
        // A call joins: the block again, and two headings -- the new call's,
        // and the one that was last, whose connector it changed.
        record.tool_started("late", &read("b.txt"));
        let _ = record.measure(120, DARK);
        assert_eq!(record.built.get(), (2, 8), "after a call joined");
        // The other theme: the block again, and the one heading whose text
        // has colours in it -- the command's words. A read's and a pattern's
        // heading is the same text in both modes on this screen.
        let _ = record.measure(120, LIGHT);
        assert_eq!(record.built.get(), (3, 9), "after a theme switch");
        // Narrower: every heading again, the block as it was.
        let _ = record.measure(100, LIGHT);
        assert_eq!(record.built.get(), (3, 16), "after a resize");
    }
}
