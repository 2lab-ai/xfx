//! The full transcript: the screen `C-o` opens over the conversation
//! (`vercel-labs/fx@c1db9191 src/ui/full_transcript_screen.zig`; its keys,
//! `src/core/app/input_full_transcript_runtime.zig:147-198`).
//!
//! The inline transcript is the conversation as it is read; this is the same
//! conversation as it was **recorded**: every entry under a dim timestamp
//! header (`full_transcript_metadata.zig:27-60`), and under each tool call what
//! it was called with and what it returned -- a command's output, a file's
//! lines, the change it was asked to make -- which the inline group never shows
//! (`tool_group_projection.zig:1150-1158`; upstream's e2e
//! `tui-command-permissions.test.ts:1118`, "keeps command output exclusive to
//! Ctrl-O").
//!
//! ```text
//!   2026-10-06 14:43:50.639 UTC · Tool
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
//! the moment it writes it -- the card, the answer's text as the pacer releases
//! it, a notice, a tool call's row, the turn's summary -- so the entries are in
//! the document's own order and say what the document says. What the document
//! is never given -- a call's arguments and its whole result -- comes from the
//! session's journal: [`Teed`] stands between the turn and the recorder on the
//! runtime thread and hands each of those to the UI as a [`Detail`] as it is
//! written down. The UI thread never waits on the runtime for any of it, and
//! never reads the log back.
//!
//! # The screen
//!
//! The terminal's alternate buffer, through the machinery the approval review
//! uses (`super::frame`'s `enter_alternate`, `repaint_alternate`,
//! `restore_primary`), so the primary document is exactly as it was when the
//! screen closes. Rows above a footer of four: a blank row, `┃ full detail ·
//! …`, a blank row and the hint row (`paint_plan.zig:993-1048`). It opens at
//! the tail, follows it while it is there, and holds still while it is not.
//!
//! # What differs from upstream's screen, and why
//!
//! * Upstream's also lists a `* network:` notice per request and a `* session:`
//!   notice at launch. xfx writes neither into its document, and this screen
//!   shows what the document was given; inventing them here would be a second
//!   source of truth about requests.
//! * A change is shown as a diff only when xfx reviewed one -- an approval in
//!   `ask` mode; otherwise the call's arguments say what it was asked to do.
//!   Upstream keeps the diff of every change in its result store.
//! * `Home` and `End` go to the top and the tail. Upstream binds neither on
//!   this screen (they reach its hidden composer); everything else this screen
//!   does not bind is swallowed here rather than typed into a composer nobody
//!   can see.
//! * The caret is left on the blank row under the footer's text: xfx's plane
//!   painter always shows it, where upstream's hides it.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;
use tokio::sync::mpsc::Sender;
use tokio_util::sync::CancellationToken;

use crate::agent::TurnJournal;
use crate::markdown::{self, MarkdownProcessor};
use crate::permission::ApprovalDiff;
use crate::session::SessionEvent;
use crate::tools::{Activity, LineChanges, Outcome, Registry, Shown};

use super::approval_view::{Line as ReviewLine, Review};
use super::blocks;
use super::bridge::{self, UiEvent};
use super::frame::clip;
use super::input::Action;
use super::notice::{Notice, Tone};
use super::theme::{Mode, Palette};
use super::toolgroup::ToolGroup;
use super::transcript::{ellipsized, Gutter, Landed, Role, Transcript};
use super::wrap;

/// The footer's one line of text (`paint_plan.zig:1003`).
pub(crate) const FOOTER: &str =
    "full detail \u{b7} ctrl+o close \u{b7} pgup/pgdn scroll \u{b7} esc close";

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

/// `YYYY-MM-DD HH:MM:SS.mmm UTC`, or nothing for a moment upstream will not
/// print either (`formatHeader`, `full_transcript_metadata.zig:27-60`: zero,
/// negative, or past the year 9999).
pub(crate) fn stamp(at_ms: i64) -> Option<String> {
    const LAST: i64 = 253_402_300_799_999;
    if at_ms <= 0 || at_ms > LAST {
        return None;
    }
    let seconds = at_ms.div_euclid(1000);
    let millis = at_ms.rem_euclid(1000);
    let (year, month, day) = civil(seconds.div_euclid(86_400));
    let of_day = seconds.rem_euclid(86_400);
    Some(format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}.{millis:03} UTC",
        of_day / 3600,
        of_day % 3600 / 60,
        of_day % 60
    ))
}

/// The calendar date `days` after 1970-01-01, proleptic Gregorian -- Howard
/// Hinnant's `civil_from_days`, the same arithmetic `std.time.epoch` does for
/// upstream.
fn civil(days: i64) -> (i64, i64, i64) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let of_era = shifted.rem_euclid(146_097);
    let year_of_era = (of_era - of_era / 1460 + of_era / 36_524 - of_era / 146_096) / 365;
    let of_year = of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * of_year + 2) / 153;
    let day = of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
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
}

/// One argument of a call: `name: value`, the value as the model sent it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Argument {
    pub(crate) name: String,
    pub(crate) value: String,
    /// Whether the value was a JSON string: the only kind upstream leaves out
    /// when the call's own row already shows it (`argumentVisibleInToolHeading`,
    /// `:6505-6510`).
    pub(crate) text: bool,
}

impl Detail {
    /// The same detail with every control a terminal would obey made a space
    /// (`super::bridge::inert`): all of it is the model's or a tool's.
    pub(crate) fn made_inert(self) -> Self {
        let inert = |text: String| bridge::inert(&text).into_owned();
        match self {
            Detail::Arguments(arguments) => Detail::Arguments(
                arguments
                    .into_iter()
                    .map(|argument| Argument {
                        name: inert(argument.name),
                        value: inert(argument.value),
                        text: argument.text,
                    })
                    .collect(),
            ),
            Detail::Command { output, status } => Detail::Command {
                output: inert(output),
                status: status.map(inert),
            },
            Detail::Result(text) => Detail::Result(inert(text)),
        }
    }
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
    names
        .into_iter()
        .map(|name| {
            let value = &map[name];
            let (value, text) = match value {
                Value::String(text) => (text.clone(), true),
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
/// tool's escaping undone, stdout before stderr, and the status when it was
/// not a clean exit -- upstream's `exit code N` and `signal N` rows
/// (`appendCommandProcessPresentation`, `:6765-6795`).
///
/// `None` for anything not in that frame -- a refusal, a timeout, a
/// cancellation -- which is then shown as the text it is.
fn command_result(output: &str) -> Option<(String, Option<String>)> {
    let rest = output.strip_prefix("<command>")?;
    let (_, rest) = rest.split_once("</command>\n<cwd>")?;
    let (_, rest) = rest.split_once("</cwd>\n")?;
    let (status, rest) = if let Some(rest) = rest.strip_prefix("<exit_code>") {
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
        text.push_str(&unframed(stream));
    }
    let text = text.strip_suffix('\n').unwrap_or(&text).to_string();
    Some((wrap::expand_tabs(&text).into_owned(), status))
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

impl TurnJournal for Teed<'_> {
    fn record(&mut self, event: SessionEvent) {
        let details = details(&event);
        self.journal.record(event);
        for (call_id, detail) in details {
            // Through the same door every event takes ([`bridge::send_ui`]),
            // waiting for a permit the way the event sink does: the journal is
            // as synchronous a seam as that one.
            let sent = bridge::park_on(bridge::send_ui(
                &self.events,
                &self.cancel,
                UiEvent::ToolDetail { call_id, detail },
            ));
            if sent.is_err() {
                return;
            }
        }
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
    /// The answer's markdown, as the pacer released it to the document.
    Response(String),
    /// A line of xfx's own that is not a notice: what the user said at an
    /// approval (`[you] …`).
    Line(String, Role, Kind),
    Notice(Notice),
    Usage(String),
    /// Boxed: a call carries its arguments, its result and a reviewed change,
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
    /// The change an approval reviewed for it, when one did.
    review: Option<ApprovalDiff>,
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
    review: Option<ApprovalDiff>,
}

/// The conversation as this session recorded it, and the screen it is shown on
/// while that screen is up.
pub(crate) struct FullTranscript {
    clock: fn() -> i64,
    /// Whether the document opened with the welcome line, which upstream's
    /// screen shows first and without a header.
    welcome: bool,
    entries: Vec<Entry>,
    /// Which entry each call is.
    calls: HashMap<String, usize>,
    /// How many times anything in each tool group changed: a call's row is
    /// laid out from its whole group, since the header counts every call.
    groups: Vec<u64>,
    /// Whether the next call joins the last group. Closed by the answer's
    /// text, a card and the turn's end -- what closes the transcript's own
    /// group -- and not by a notice, which the group carries under it.
    group_open: bool,
    pending: HashMap<String, Pending>,
    /// The call the runtime last said it started: the one an approval that
    /// arrives now is about (a turn runs its calls one at a time).
    started: Option<String>,
    revision: u64,
    view: Option<View>,
}

/// The screen, while it is up.
struct View {
    /// How many rows above the tail the screen shows: `0` follows it.
    scroll: Cell<usize>,
    /// The width and the number of rows the last screen was laid out from, so
    /// that rows added under a screen that is not at the tail can be stepped
    /// over rather than scrolled into it.
    laid: Cell<Option<(u16, usize)>>,
}

impl View {
    /// How far above the tail the screen is, once the rows added under it
    /// since it was last laid out have been stepped over.
    ///
    /// Rows added under a screen that is not at the tail move the tail away
    /// from what the screen shows; stepping over them keeps it still, as
    /// upstream's bookmark does. A screen at the tail follows it. Asked by the
    /// paint and by a keystroke alike, so a key pressed between a change and
    /// the frame that would show it moves from where the reader really is.
    fn settled(&self, cols: u16, total: usize) -> usize {
        if let Some((laid_cols, laid_total)) = self.laid.get() {
            if laid_cols == cols && total > laid_total && self.scroll.get() > 0 {
                self.scroll.set(self.scroll.get() + (total - laid_total));
            }
        }
        self.laid.set(Some((cols, total)));
        self.scroll.get()
    }
}

/// What a keystroke did to the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reply {
    /// Close it.
    Close,
    /// The rows it shows moved.
    Moved,
    /// Nothing that shows.
    Nothing,
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
            welcome: false,
            entries: Vec::new(),
            calls: HashMap::new(),
            groups: Vec::new(),
            group_open: false,
            pending: HashMap::new(),
            started: None,
            revision: 0,
            view: None,
        }
    }

    fn push(&mut self, body: Body) {
        self.revision += 1;
        self.entries.push(Entry {
            at_ms: (self.clock)(),
            body,
            revision: self.revision,
            drawn: RefCell::new(None),
        });
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

    /// Answer text the pacer released into the document.
    ///
    /// One entry per run of text between other things, as upstream has one
    /// entry per assistant block. Text that is only line breaks opens none,
    /// for the reason the document writes none for it
    /// (`super::shell`'s `write_prose`), and closes no group either.
    pub(crate) fn answer(&mut self, text: &str) {
        self.revision += 1;
        let revision = self.revision;
        if let Some(Entry {
            body: Body::Response(markdown),
            revision: entry,
            ..
        }) = self.entries.last_mut()
        {
            markdown.push_str(text);
            *entry = revision;
            return;
        }
        if text.trim().is_empty() {
            return;
        }
        self.group_open = false;
        self.push(Body::Response(text.to_string()));
    }

    /// One of xfx's own lines that is not a notice, and what to call it.
    pub(crate) fn line(&mut self, text: &str, role: Role, kind: Kind) {
        self.push(Body::Line(text.to_string(), role, kind));
    }

    pub(crate) fn notice(&mut self, notice: &Notice) {
        self.push(Body::Notice(notice.clone()));
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
        self.push(Body::Tool(Box::new(Call {
            id: id.to_string(),
            shown: shown.clone(),
            state: State::Running,
            group,
            arguments: pending.arguments.unwrap_or_default(),
            result: pending.result,
            review: pending.review,
        })));
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
        self.started = None;
    }

    /// What the runtime said that the document is not told about: which call
    /// is running, the change an approval reviews, and the journal's details.
    pub(crate) fn observe(&mut self, event: &UiEvent) {
        match event {
            UiEvent::ToolStart { call_id, .. } => self.started = Some(call_id.clone()),
            UiEvent::Approval(asked) => {
                if let (Some(id), Some(diff)) = (self.started.clone(), asked.request.diff.clone()) {
                    self.attach(
                        &id,
                        |call| call.review = Some(diff.clone()),
                        |pending| {
                            pending.review = Some(diff.clone());
                        },
                    );
                }
            }
            UiEvent::ToolDetail { call_id, detail } => match detail {
                Detail::Arguments(arguments) => self.attach(
                    call_id,
                    |call| call.arguments = arguments.clone(),
                    |pending| pending.arguments = Some(arguments.clone()),
                ),
                result => self.attach(
                    call_id,
                    |call| call.result = Some(result.clone()),
                    |pending| pending.result = Some(result.clone()),
                ),
            },
            _ => {}
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
        self.view = Some(View {
            scroll: Cell::new(0),
            laid: Cell::new(None),
        });
    }

    pub(crate) fn close(&mut self) {
        self.view = None;
    }

    /// What one keystroke does while the screen is up (`routeKey`,
    /// `input_full_transcript_runtime.zig:177-198`).
    ///
    /// `rows` is the terminal's height: a page is the rows the screen shows
    /// (`fullTranscriptPageRows`, `transcript/runtime.zig:6882-6888`).
    pub(crate) fn key(&self, action: Action, cols: u16, rows: u16, palette: Palette) -> Reply {
        let Some(view) = &self.view else {
            return Reply::Nothing;
        };
        let page = Layout::of(rows).viewport.max(1);
        let total = self.total(cols, palette);
        let furthest = total.saturating_sub(page);
        let scroll = view.settled(cols, total).min(furthest);
        let moved = match action {
            Action::ToggleTranscript | Action::Escape | Action::Cancel => return Reply::Close,
            Action::PageUp => scroll.saturating_add(page),
            Action::PageDown => scroll.saturating_sub(page),
            Action::Up => scroll.saturating_add(ARROW_ROWS),
            Action::Down => scroll.saturating_sub(ARROW_ROWS),
            Action::Home => furthest,
            Action::End => 0,
            _ => return Reply::Nothing,
        }
        .min(furthest);
        if moved == view.scroll.get() {
            return Reply::Nothing;
        }
        view.scroll.set(moved);
        Reply::Moved
    }

    /// The whole screen, top first: exactly `rows` rows, none wider than
    /// `cols`. `status` is the hint row as the band would paint it.
    pub(crate) fn rows(&self, cols: u16, rows: u16, palette: Palette, status: &str) -> Vec<String> {
        let layout = Layout::of(rows);
        let total = self.total(cols, palette);
        let scroll = self
            .view
            .as_ref()
            .map_or(0, |view| view.settled(cols, total));
        let shown = total.min(layout.viewport);
        let furthest = total - shown;
        let start = furthest - scroll.min(furthest);
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

    /// How many rows the whole record lays out as at this width.
    fn total(&self, cols: u16, palette: Palette) -> usize {
        let welcome = if self.welcome { 1 } else { 0 };
        let mut total = welcome;
        for entry in &self.entries {
            let rows = self.laid_out(entry, cols, palette);
            if rows.is_empty() {
                continue;
            }
            if total > 0 {
                total += 1;
            }
            total += rows.len();
        }
        total
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
        for entry in &self.entries {
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
        let (kind, body) = match &entry.body {
            Body::Prompt(text) => (Kind::Prompt, card(text, cols, palette)),
            Body::Response(text) => (Kind::Response, answer(text, cols, palette)),
            Body::Line(text, role, kind) => (*kind, line(text, *role, cols, palette)),
            Body::Notice(notice) => (
                if notice.tone == Tone::Error {
                    Kind::Error
                } else {
                    Kind::Notice
                },
                notice_rows(notice, cols, palette),
            ),
            Body::Usage(text) => (Kind::Usage, line(text, Role::Dim, cols, palette)),
            Body::Tool(call) => (Kind::Tool, self.tool_rows(call, cols, palette)),
        };
        if body.is_empty() {
            return Vec::new();
        }
        let mut rows = Vec::with_capacity(body.len() + 1);
        if let Some(stamp) = stamp(entry.at_ms) {
            let text = clip(&format!("  {stamp} \u{b7} {}", kind.label()), cols).to_string();
            rows.push(format!("{}{text}{}", palette.dim(), palette.reset()));
        }
        rows.extend(body);
        rows
    }

    /// A call's rows: its group's header when it is the group's first, its own
    /// row as the group draws it, then what it was called with, what it
    /// returned and the change that was reviewed for it.
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
        let mut group = ToolGroup::default();
        for member in &members {
            group.start(member.id.clone(), member.shown.clone());
            if let State::Finished {
                ok,
                detail,
                outcome,
                changes,
            } = &member.state
            {
                group.finish(&member.id, *ok, detail, *outcome, *changes);
            }
        }
        if members
            .iter()
            .any(|member| member.state == State::Cancelled)
        {
            group.cancel_running();
        }
        let block = group.redraw(palette, &[]).unwrap_or_default();
        let mut rows = Vec::new();
        if position == 0 {
            if let Some(header) = block.first() {
                rows.push(ellipsized(header, cols));
            }
        }
        let heading = block
            .get(1 + position)
            .map(|row| ellipsized(row, cols))
            .unwrap_or_default();
        let plain_heading = unpainted(&heading);
        rows.push(heading);
        if call.review.is_none() {
            for argument in &call.arguments {
                let shown_already =
                    argument.text && plain_heading.contains(argument.value.as_str());
                let exec = call.shown.presentation.activity == Activity::Command
                    && argument.name == "action"
                    && argument.value == "exec";
                if shown_already || exec {
                    continue;
                }
                rows.extend(railed(
                    &format!("{}: {}", argument.name, argument.value),
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
            Some(Detail::Result(text)) if !text.is_empty() => {
                rows.extend(railed(text, "  ", cols, palette));
            }
            _ => {}
        }
        if let Some(diff) = &call.review {
            rows.push(String::new());
            rows.extend(review_rows(diff, cols, palette));
        }
        rows
    }
}

impl Call {
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
fn railed(text: &str, gutter: &str, cols: u16, palette: Palette) -> Vec<String> {
    let width = cols.saturating_sub(1 + wrap::width(gutter)).max(1);
    let dim = palette.dim();
    let reset = palette.reset();
    let mut rows = Vec::new();
    for line in text.split('\n') {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            rows.push(clip(&format!("\u{2502}{dim}{gutter}{reset}"), cols).to_string());
            continue;
        }
        for row in wrap::wrap(line, width) {
            let piece = line[row.start..row.end].trim_end_matches([' ', '\n']);
            let piece = clip(piece, width);
            rows.push(format!("\u{2502}{dim}{gutter}{piece}{reset}"));
        }
    }
    rows
}

/// A reviewed change as upstream's transcript shows one under its call
/// (`appendDiffRenderLine`, `diff.zig:1063-1084`): `  │ N ± text`, the changed
/// lines in the diff grey and the rest in the dim one -- monochrome, as
/// upstream's markers are until a theme is pinned (`render.zig:93-105`).
fn review_rows(diff: &ApprovalDiff, cols: u16, palette: Palette) -> Vec<String> {
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
    review
        .lines
        .iter()
        .map(|line| {
            let (style, number, sign, text) = match line {
                ReviewLine::Context(number, text) => {
                    (palette.dim(), number.to_string(), ' ', text.clone())
                }
                ReviewLine::Added(number, text) => {
                    (palette.diff(), number.to_string(), '+', text.clone())
                }
                ReviewLine::Removed(number, text) => {
                    (palette.diff(), number.to_string(), '-', text.clone())
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
            let row = format!("  \u{2502} {number:>width$} {sign} {text}");
            format!("{style}{}{reset}", clip(&row, cols))
        })
        .collect()
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

/// The answer's markdown, presented as the document presents it: prose in the
/// two-column gutter, code, tables and rules as blocks of their own
/// (`super::shell`'s `present`, `write_prose` and `write_structure`).
fn answer(text: &str, cols: u16, palette: Palette) -> Vec<String> {
    let light = palette.mode == Mode::Light;
    let mut processor = MarkdownProcessor::with_styles(markdown::ansi::Styles::new(
        markdown::theme::builtin(light),
    ));
    let mut writer = Prose {
        transcript: Transcript::new(cols),
        open: false,
        held: 0,
        cols,
        light,
    };
    for event in processor.feed(text) {
        writer.present(event);
    }
    for event in processor.finish() {
        writer.present(event);
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

    use crate::permission::{ApprovalRequest, ApprovalSubject};
    use crate::tools::{read, terminal};
    use serde_json::json;

    use super::super::theme::Depth;

    const DARK: Palette = Palette {
        mode: Mode::Dark,
        depth: Depth::Ansi256,
    };

    /// 2026-10-06 14:43:50.620 UTC, the moment the capture's second entry is
    /// stamped with (`fx-tools-single-40x120-full`).
    const AT: i64 = 1_791_297_830_620;

    fn fixed() -> i64 {
        AT
    }

    fn record() -> FullTranscript {
        FullTranscript::with_clock(fixed)
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
    fn a_header_is_upstreams_utc_stamp_and_the_entrys_kind() {
        // `full_transcript_metadata.zig:63-72`, the same instant.
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
    }

    #[test]
    fn a_reviewed_change_is_shown_as_upstreams_diff_block_instead_of_its_arguments() {
        // `fx-approvals-cc-40x120-full` rows 19-21: the row, a blank row, and
        // `  │ 1 + new file` in the diff grey.
        let input = json!({"path": "c.txt", "content": "new file\n"});
        let mut record = record();
        record.observe(&UiEvent::ToolStart {
            call_id: "w".into(),
            tool: "write_file".into(),
            shown: crate::tools::mutate::WRITE_FILE.shown(&input),
        });
        record.observe(&UiEvent::ToolDetail {
            call_id: "w".into(),
            detail: Detail::Arguments(arguments("write_file", &input)),
        });
        record.observe(&UiEvent::Approval(super::super::approval::ApprovalAsked {
            id: super::super::approval_readiness::ApprovalId(1),
            request: ApprovalRequest {
                tool: "write_file",
                target: "c.txt".into(),
                summary: "write `c.txt`".into(),
                always_scope: "allow".into(),
                diff: Some(ApprovalDiff::of("", "new file\n")),
                subject: ApprovalSubject::Path("c.txt".into()),
            },
        }));
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
        // The group's own `■ Cancelled …` line is the inline transcript's:
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
    fn the_amendment_line_keeps_the_kind_the_shell_gave_it() {
        let mut record = record();
        record.line("[you] use capitals", Role::Notice, Kind::Prompt);
        assert_eq!(
            &screen(&record)[..2],
            [
                "  2026-10-06 14:43:50.620 UTC \u{b7} Prompt",
                "[you] use capitals",
            ]
        );
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
        assert_eq!(record.total(120, DARK), 89);
        let top = |record: &FullTranscript| screen(record)[0].clone();
        let at_tail = top(&record);
        assert_eq!(screen(&record)[35], "* n: 29");

        assert_eq!(record.key(Action::PageUp, 120, 40, DARK), Reply::Moved);
        assert_eq!(screen(&record)[35], screen(&record)[35]);
        let page_up = screen(&record);
        // A page is the 36 rows the screen shows.
        assert_eq!(record.view.as_ref().unwrap().scroll.get(), 36);
        assert_eq!(record.key(Action::Up, 120, 40, DARK), Reply::Moved);
        assert_eq!(record.view.as_ref().unwrap().scroll.get(), 39);
        assert_eq!(record.key(Action::Down, 120, 40, DARK), Reply::Moved);
        assert_eq!(screen(&record), page_up);
        assert_eq!(record.key(Action::Home, 120, 40, DARK), Reply::Moved);
        assert_eq!(
            screen(&record)[0],
            "  2026-10-06 14:43:50.620 UTC \u{b7} Notice"
        );
        // Past the head is nothing.
        assert_eq!(record.key(Action::PageUp, 120, 40, DARK), Reply::Nothing);
        assert_eq!(record.key(Action::End, 120, 40, DARK), Reply::Moved);
        assert_eq!(top(&record), at_tail);
        assert_eq!(record.key(Action::PageDown, 120, 40, DARK), Reply::Nothing);
        // Left and Right keep the screen where it is (`navigate`); a key it
        // does not bind is swallowed.
        for action in [
            Action::Left,
            Action::Right,
            Action::Submit,
            Action::Tab,
            Action::Redraw,
        ] {
            assert_eq!(record.key(action, 120, 40, DARK), Reply::Nothing);
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
        record.key(Action::End, 120, 40, DARK);
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
        assert_eq!(record.key(Action::PageUp, 120, 40, DARK), Reply::Nothing);
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
        assert!(!arguments[0].name.contains('\u{1b}'));
        assert!(!arguments[0].value.chars().any(char::is_control));
        let Detail::Command { output, status } = Detail::Command {
            output: "a\u{1b}[31mb\nc".into(),
            status: Some("s\u{7}".into()),
        }
        .made_inert() else {
            panic!("not a command");
        };
        assert_eq!(output, "a [31mb\nc");
        assert_eq!(status.as_deref(), Some("s "));
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
    fn the_journal_tee_records_first_and_hands_over_what_the_document_never_gets() {
        struct Kept(Vec<SessionEvent>);
        impl TurnJournal for Kept {
            fn record(&mut self, event: SessionEvent) {
                self.0.push(event);
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
            });
            teed.record(SessionEvent::ToolResult {
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
                    text: true,
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

        // A cancelled turn still journals everything and hands nothing over.
        let (events, mut received) = tokio::sync::mpsc::channel(8);
        let cancel = CancellationToken::new();
        cancel.cancel();
        let mut kept = Kept(Vec::new());
        Teed::new(&mut kept, events, cancel).record(SessionEvent::ToolResult {
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
}
