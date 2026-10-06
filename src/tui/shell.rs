//! What the band is a picture of.
//!
//! The event loop reads bytes and writes frames; everything between the two --
//! what the band's rows say, where the caret is, and whether the session is
//! leaving -- is here, so that "what would the band look like now" is a
//! question about a value rather than about a terminal.
//!
//! In this phase that value is small. The band is a divider, the composer and a
//! hint row, with one more row above the divider while a turn is running --
//! what it is doing and how long it has been doing it ([`super::activity`]).
//! The shape is what carries the rest: the rows are produced top-down from the
//! geometry, and the caret is reported in the composer's own coordinates rather
//! than derived a second time by whatever draws it.
//!
//! # The composer, its gutter, and the band's own height
//!
//! The text belongs to [`Editor`]; what this module owns is where it goes. Two
//! decisions carry that:
//!
//! * **The prompt marker is a gutter, not a prefix.** `> ` is two cells the
//!   text never uses, so the composer is measured against `cols - 2` and every
//!   one of its rows is written into that same two-cell indent. The
//!   alternative -- wrapping at the screen's width and putting the marker in
//!   front of the first row -- makes that row two cells too long, which the
//!   painter clips and the caret does not: the caret would be reported past the
//!   last column of a row whose end is not on the screen. The marker itself
//!   stays on the composer's **first** row, and a composer scrolled past that
//!   row shows none, because the marker means *this is where what you are
//!   writing begins* and there is exactly one such row.
//! * **The band grows and shrinks with the text, up to the cap.** A composer of
//!   *n* rows is a band with *n* composer rows, re-solved through
//!   [`super::layout::solve`] so the divider, the hint row and the content area
//!   stay one derivation rather than three. Past
//!   [`super::layout::input_row_limit`] it stops growing and scrolls inside the
//!   rows it has ([`editor::window`]), which is what keeps a long draft from
//!   eating the transcript.
//!
//! # What the keystrokes mean here
//!
//! [`Shell::route_bytes`] owns the [`Decoder`], so the deferred bytes the
//! launch probe read and the bytes every later read produces go through one
//! machine in arrival order -- which is the whole of what that decoder asks of
//! its caller. What comes out is routed in exactly three ways: the composer's
//! own actions go to the editor, `Submit` and `Ctrl-D` are the session's, and
//! everything else is a keystroke this phase has no binding for. `Ctrl-D` is
//! the one worth stating: it leaves **only** on an empty composer, and with
//! text under the caret it is a forward delete, which is what it means in every
//! shell that has both.
//!
//! The transcript is the one thing here that is **not** a picture of the band.
//! Nothing above the divider belongs to xfx: a row that goes there goes into
//! the terminal's own document and is never repainted, so what the shell holds
//! for it is not a state to draw but a *queue of writes it owes* -- one
//! [`Append`] per push, drained by the loop before the frame, because an append
//! scrolls the screen and a band painted first would be carried up with it.
//!
//! # The turn
//!
//! A submitted line is offered to the runtime thread and echoed into the
//! document, and what comes back is a [`UiEvent`] this module turns into more
//! document rows ([`Shell::apply`]). The two directions never meet in the
//! middle: nothing here awaits anything, and nothing here writes a byte -- the
//! submission is a `try_send` that refuses rather than waits, and the events
//! are text that becomes an [`Append`] like any other.
//!
//! One event is not a document row. [`UiEvent::Fatal`] is the runtime saying it
//! cannot go on, and there is nothing useful to paint about it into a band that
//! is about to be taken down: it is *remembered* ([`Shell::fatal`]) and the
//! session leaves, so the message is printed by the ordinary failure path --
//! on a terminal that has been given back first.

use std::collections::VecDeque;
use std::process::ExitCode;
use std::time::Instant;

use super::activity::{Activity, Work, PHASES};
use super::approval::{self, Panel};
use super::approval_readiness::{ApprovalId, Intent, Outcome, Readiness, Surface};
use super::approval_screen::ApprovalScreen;
use super::blocks;
use super::bridge::{self, ModelAnswer, ModelChange, TurnControl, TurnWork, UiEvent};
use super::copy::Copies;
use super::edit_history::{Delta, EditHistory};
use super::editor::{self, Editor};
use super::entity::{Entities, EntityKind};
use super::frame::Caret;
use super::full_transcript::{self, FullTranscript};
use super::gesture::{Escape, Gestures, Interrupt, INTERRUPTED_EXIT_CODE};
use super::help_menu::{self, HelpAction, HelpMenu};
use super::hint::{self, Hint, Notice};
use super::history::{History, HistoryEntry, HistoryStep};
use super::hydration::{Ending, HydratedStep, HydratedTurn, Hydration};
use super::input::{Action, Decoder, Input};
use super::layout::{self, Geometry};
use super::model_column::{self, Chosen, Column, Edit as ColumnEdit, Known};
use super::model_menu::{self, ModelAction, ModelMenu};
use super::notice::{self, unattributed};
use super::pacer::Pacer;
use super::paste::{self, Paste, Pasted, Refusal};
use super::permissions::{self, Change};
use super::picker::{self, Dismissed, Picker, PickerAction, PickerOutcome, Trigger};
use super::provider_picker::{self, ProviderPicker};
use super::question::{self, QuestionPanel, QuestionRequest};
use super::render_request::{Reason, RenderRequest};
use super::router::{self, CommandHandlers};
use super::session_picker::{self, SessionAction, SessionPicker};
use super::sound::Sound;
use super::statusline::{self, MenuAction as StatuslineAction};
use super::theme::{DocumentRow, Mode, Palette};
use super::toolgroup::ToolGroup;
use super::transcript::{Append, Gutter, Landed, Role, Transcript};
use super::worker::{Rejected, WorkHandle};
use crate::clipboard::{self, Clipboard};
use crate::config::{PermissionMode, RuntimeConfig, StatusLine, StatusLineItem};
use crate::interactive::{self, Submitted};
use crate::markdown::{self, MarkdownProcessor};
use crate::output::safe_one_line;
use crate::permission::{ApprovalAnswer, ApprovalRequest};
use crate::provider::effort::Effort;
use crate::provider::model::model_id_problem;
use crate::provider::model::{resolve_in_order, CatalogEntry, CatalogRetry};
use crate::provider::ProviderId;
use crate::session::store::title_of;
use crate::tools::{LineChanges, Outcome as CallOutcome, Shown};

/// Upstream's line for a cancelled turn, in this product's name and in
/// `palette`: `■` in the warning grey, `Cancelled` in the hint grey, and the
/// question after it in the terminal's own colour (`lifecycleTerminalLine`,
/// `transcript/runtime.zig:633-680`; `fx-interrupt-40x120-cc-0.3.ansi`:
/// `ESC[38;5;252m■ESC[39m ESC[38;5;255mCancelledESC[39m · What can fx do
/// differently?`).
pub(crate) fn cancelled_line(palette: Palette) -> String {
    format!(
        "{}\u{25a0}{} {}Cancelled{}{CANCELLED_FOLLOW_UP}",
        palette.warning(),
        palette.reset(),
        palette.hint(),
        palette.reset()
    )
}

/// Upstream's welcome line, for this binary (`render.zig:160-193`, `c1db919`).
///
/// The brand in the subtitle style, then the build and where the commands are
/// listed in the dim one. A release build says its version; any other build
/// says the revision it was built from and its channel, the channel in the
/// hint colour -- upstream's `[dev]` tag, because a preview ships on every
/// merge and the version alone does not identify it.
pub(crate) fn welcome_line(palette: Palette) -> String {
    let build = crate::build_info();
    let version = crate::VERSION;
    let label = match (build.channel, build.revision) {
        ("release", _) => format!("v{version}"),
        (channel, Some(revision)) => format!(
            "v{version}-{} {}[{channel}]{}",
            revision.get(..7).unwrap_or(revision),
            palette.hint(),
            palette.dim()
        ),
        (channel, None) => format!("v{version} {}[{channel}]{}", palette.hint(), palette.dim()),
    };
    format!(
        "{}x\u{1d487}x{}{} {label} \u{b7} Run /help for commands{}",
        palette.subtitle(),
        palette.reset(),
        palette.dim(),
        palette.reset()
    )
}

/// Upstream's way back into a session, written to standard output once the
/// terminal has been given back (`formatResumeHandoff`, `render.zig:795-810`,
/// `c1db919`; written at `app_entry_runtime.zig:424-438`).
///
/// The label, then the command: on the same row when the whole line fits the
/// screen, and on the next one, two columns in, when it does not -- all of it
/// in the dim grey, ended by a reset and a line feed. `xfx` where upstream says
/// `fx`, because the command has to be one this binary answers: a bare
/// `xfx --resume <id>` is the TUI on that session (`crate::cli`).
pub(crate) fn resume_handoff(id: &str, columns: u16, palette: Palette) -> String {
    let command = format!("{RESUME_COMMAND} {id}");
    let one_row = RESUME_LABEL.len() + 1 + command.len();
    let separator = if one_row <= usize::from(columns) {
        " "
    } else {
        "\n  "
    };
    format!(
        "{}{RESUME_LABEL}{separator}{command}{}\n",
        palette.dim(),
        palette.reset()
    )
}

/// Upstream's label for that line (`render.zig:801`).
const RESUME_LABEL: &str = "Continue session with:";

/// The command it names, without the id.
const RESUME_COMMAND: &str = "xfx --resume";

/// What a resumed session with no title of its own is called
/// (`session_display_metadata.zig:14`).
const UNTITLED_SESSION: &str = session_picker::UNTITLED;

/// What `/resume` says while a turn is in hand, rather than opening a picker
/// (`openSessionPickerWithScope`, `app_session_runtime.zig:2588-2595`): a
/// neutral notice under `session`.
const RESUME_BUSY: &str = "resume is unavailable until the response finishes";

/// The divider's rule, one cell wide, repeated across the screen.
const RULE: char = '\u{2500}';

/// The rail every composer row starts with, followed by one space
/// (`input_presentation.zig:977-992`).
const RAIL: &str = "\u{2503}";

/// The rail on the first row shown when the rows above it are scrolled out of
/// view: the arrow takes the space's cell.
const RAIL_HIDDEN_ABOVE: &str = "\u{2503}\u{2191}";

/// How many cells the rail and its space occupy, and therefore where an empty
/// composer's caret sits (`visual_layout.zig:689-694`).
const PROMPT_CELLS: u16 = 2;

/// How much of a refused tool's own words a notice quotes back.
///
/// The same bound the line-oriented path gives a tool notice
/// (`src/output.rs:1062`), spelled again rather than shared because that one is
/// private to a module this task does not own. Both are there for the same
/// reason: what a tool says about why it refused is for whoever needs it in
/// full, and never for a row of the terminal's document.
const TOOL_DETAIL_BYTES: usize = 120;

/// What the user is told when a submission arrives with the queue already full.
///
/// On the **hint row** rather than in the document, and with the draft left in
/// the composer, because those are the two halves of one sentence: the line was
/// not sent, so it is still somewhere, and where it still is is where the user
/// last saw it. A document row would scroll away from the text it is about.
pub(crate) const QUEUE_REJECTED: &str = "one prompt is already queued; this one was not sent";

/// What the user is told when the interrupt takes a waiting prompt with it.
///
/// Present tense, because it is the *request* landing rather than a report of
/// what the runtime has finished doing: the drop itself is `super::worker`'s `abandon_pending`, on the far
/// side of the control channel. It is written here and not sent from there for
/// one reason -- there is no synchronous way to put a sentence on that channel,
/// and a notice that waited for the cancelled turn to unwind would never arrive
/// at all for the turn most worth interrupting, the one whose provider has gone
/// quiet without hanging up.
///
/// The body of a cancelled notice under the topic `queue`: the glyph says the
/// sentence is xfx's, so the words do not.
const QUEUE_DROPPED: &str = "dropping what was queued behind it as well.";

/// What the hint row says while a second Escape would clear the composer.
///
/// The gesture is destructive and unprompted -- nothing else in this phase
/// throws a draft away without a key that says so -- and this row is the whole
/// of the warning the user gets before the second tap.
pub(crate) const ESCAPE_ARMED: &str = "esc again to clear";

/// What the hint row says while a Ctrl-C would leave
/// (`input_presentation.zig:502`, `c1db919`).
///
/// It takes the **whole** row rather than a slot beside the session's facts,
/// as upstream's does: it is the one thing on the row that is about the next
/// keystroke, and the next keystroke ends the session.
pub(crate) const EXIT_ARMED: &str = "press ctrl+c again to exit";

/// What the hint row's right-hand slot says while a second Escape would stop
/// the work in hand, widest first: the first that fits beside the row is the
/// one shown (`input_presentation.zig:510-525`).
const INTERRUPT_ARMED: [&str; 3] = ["esc again to interrupt", "esc esc interrupt", "esc esc"];

/// What the hint row says, whole, when not even the narrowest of those fits
/// beside the session's facts (`input_presentation.zig:525`).
const INTERRUPT_ARMED_ALONE: &str = "esc esc to interrupt";

/// What leads a waiting prompt's row in the band (`input_presentation.zig:76`).
const BANNER_MARK: &str = "\u{250b} ";

/// The most rows one waiting prompt is shown in (`max_steering_message_rows`,
/// `render_input.zig:491`).
const BANNER_ROWS: usize = 2;

/// `text` laid out in at most [`BANNER_ROWS`] rows `width` cells wide: broken
/// at its own line breaks and, inside a line, after the last space that fits;
/// a word wider than a row cut at the row; and the last row cut one cell short
/// for `…` when anything is left (`steering_message_layout`,
/// `render_input.zig:518-577`).
fn banner_layout(text: &str, width: usize) -> Vec<String> {
    let mut rows: Vec<String> = Vec::new();
    let mut lines = text.split('\n').map(|line| line.trim_end_matches('\r'));
    let mut rest = lines.next().unwrap_or_default().to_string();
    loop {
        // The row: as much of the line as fits, back to its last space when
        // the line goes on past it.
        let mut cells = 0usize;
        let mut end = 0usize;
        let mut last_space = None;
        for (at, character) in rest.char_indices() {
            let wide = usize::from(super::wrap::width(&rest[at..at + character.len_utf8()]));
            if cells + wide > width {
                break;
            }
            if character == ' ' {
                last_space = Some(at);
            }
            cells += wide;
            end = at + character.len_utf8();
        }
        let whole = end == rest.len();
        let cut = match (whole, last_space) {
            (false, Some(space)) if space > 0 => space,
            _ => end,
        };
        rows.push(rest[..cut].to_string());
        let remainder = rest[cut..].trim_start_matches(' ').to_string();
        let more = !remainder.is_empty() || lines.clone().next().is_some();
        if rows.len() == BANNER_ROWS && more {
            if let Some(last) = rows.last_mut() {
                let room = width.saturating_sub(1);
                let mut kept = String::new();
                for character in last.chars() {
                    let next = format!("{kept}{character}");
                    if usize::from(super::wrap::width(&next)) > room {
                        break;
                    }
                    kept = next;
                }
                *last = format!("{kept}\u{2026}");
            }
            break;
        }
        if !remainder.is_empty() {
            rest = remainder;
            continue;
        }
        match lines.next() {
            Some(line) => rest = line.to_string(),
            None => break,
        }
        if rows.len() == BANNER_ROWS {
            break;
        }
    }
    rows
}

/// The question a cancelled turn leaves in the document, after its marker and
/// its word (`lifecycleTerminalLine`, `transcript/runtime.zig:644`), in this
/// product's name.
const CANCELLED_FOLLOW_UP: &str = " \u{b7} What can xfx do differently?";

/// What the user is told when the screen cannot hold the question.
///
/// A refusal rather than a squeezed panel, and it is the same rule the panel's
/// own Ctrl-C is: **a decision xfx was never given is a refusal**. A question
/// whose three choices were off the bottom of the screen would leave the
/// session waiting for an answer the user has no way to give, which is worse
/// than a change that did not happen. In the document rather than on the hint
/// row, because it is about a turn rather than about a keystroke.
///
/// An error under the topic `permissions`, upstream's topic for the permission
/// engine's own notices.
const PANEL_TOO_SMALL: &str =
    "this screen is too small to ask for permission, so the change was refused";

/// What the user is told when they granted a question the screen has not shown
/// them yet.
///
/// **Said rather than swallowed.** The keystroke is spent and the question stays
/// up, so the alternative to a sentence is a key that appears to do nothing --
/// and a user whose `1` does nothing presses it again, harder, which is the one
/// input pattern this gate must not train.
pub(crate) const APPROVAL_NOT_READY: &str =
    "the question is not on the screen yet - press it again once you can read it";

/// What they are told when this screen can never show the whole request, so no
/// frame on it will ever grant.
///
/// A different sentence from [`APPROVAL_NOT_READY`] because it asks for a
/// different action: waiting fixes the first and only a bigger window fixes the
/// second. It names the two keys that still work, because a question that cannot
/// be granted must still be refusable.
pub(crate) const APPROVAL_NOT_DISCLOSED: &str =
    "this screen cannot show the whole request, so it cannot be approved here; \
     make the window larger, or press 3 or ctrl+c to refuse";

/// What the user is told when the screen cannot hold a question the **model**
/// asked ([`super::question`]).
///
/// The same rule as [`PANEL_TOO_SMALL`] -- a decision xfx was never given is not
/// one it may invent -- and a **different sentence**, because the two refusals
/// are about different things. `ask_user_question` asks for no permission and
/// changes nothing: a batch refused for want of rows grants nothing, denies
/// nothing, and leaves the model to ask in freeform instead. Borrowing the
/// permission panel's wording would tell the user that a change they never made
/// was refused, which is the one kind of sentence a refusal must not contain.
///
/// A cancelled notice under the topic `question`: what happened to the batch is
/// exactly that it was cancelled, which is the tone's own meaning.
const QUESTION_TOO_SMALL: &str =
    "this screen is too small to show the question, so the question was cancelled";

/// What the user is told when the runtime thread is not there to take work.
///
/// Told apart from [`QUEUE_REJECTED`] on purpose: a runtime that is gone is not
/// a runtime with a queue, and a fatal event is already on its way to say so.
/// In the document rather than on the hint row, because unlike a full queue it
/// is not a condition that clears.
///
/// An error under upstream's topic for the session's own machinery, `system`
/// (`app_process_runtime.zig:61`, where a request that failed is reported).
const GONE_NOTICE: &str = "the runtime is gone; that line was not sent";

/// The hint row's refusal for a paste larger than the byte budget.
///
/// A **hint-row** refusal rather than a document line, for the reason
/// [`QUEUE_REJECTED`] is one: it belongs beside the composer it did not change,
/// and it is a condition that clears the moment anything else happens on that
/// row. Said at all, unlike the composer's own budget -- which refuses silently
/// because a keystroke that changes nothing is its own feedback -- because a
/// paste that vanished without a word looks exactly like a terminal that never
/// sent it.
const PASTE_REFUSED: &str = "that paste is larger than 8 MiB; nothing was taken";

/// The hint row's refusal for a paste this session has no number left for.
///
/// Its own sentence rather than [`PASTE_REFUSED`]'s, because the two are
/// different problems with different answers: a paste past the budget is one a
/// user can make smaller, and a session four billion pastes deep is one they
/// have to leave. Reachable only through `Paste::with_next`, which is why the
/// case that proves it is a cargo test rather than a scenario.
const PASTE_UNNUMBERED: &str = "this session has no paste numbers left; nothing was taken";

/// The hint row's word for a recalled line whose blocks could not be renumbered.
///
/// The same exhaustion seen from the other side: the summaries are still on the
/// screen, because they are the line as it was submitted, but they stand for
/// nothing now and the line would be sent as the words it looks like. Said,
/// rather than left for the user to discover in what the model answers.
const RECALL_UNNUMBERED: &str = "no paste numbers left; the recalled blocks are words now";

/// The same exhaustion seen from a yank.
///
/// Its own sentence rather than [`RECALL_UNNUMBERED`]'s, because a user who
/// pressed `C-y` did not recall anything and a row that said so would be
/// describing a keystroke they did not make.
const YANK_UNNUMBERED: &str = "no paste numbers left; the yanked blocks are words now";

/// What a line that names no command -- or names one with something after it
/// that the command does not take -- is answered with: upstream's own
/// sentence, an error under the topic `command` (`app_commands.zig:2162-2169`).
const UNKNOWN_COMMAND: &str = "Unknown command. Try /help.";

/// What `/provider` says while a turn is running or a prompt is waiting for
/// one, under the topic `provider`: upstream's own sentence
/// (`provider_busy_message`, `app_auth_runtime.zig:30`, written at
/// `:430-439`). When that is is [`Shell::provider_work_busy`].
const PROVIDER_BUSY: &str =
    "Provider switching is unavailable until active and queued work finishes.";

/// What a `/model` explicit form that does not hold is answered with --
/// four words or more, an effort the model does not list, a speed xfx cannot
/// send: upstream's usage line, an error with no topic
/// (`submitExplicitModelSelection`, `app_input_runtime.zig:2860-2894`;
/// `fx-model3-40x120-paste-bad-effort`).
const MODEL_USAGE: &str = "usage: /model <id> <effort> [normal|fast|ultrafast]";

/// Upstream's announcement of a model change (`announceModelSelection`,
/// `session_commands.zig:1266-1279`): `Switched to`, or `Next turn will use`
/// while a turn is still to finish, then the id, the effort -- `default` for
/// none -- and the speed. The speed is `normal` unless the id itself is a fast
/// one (`intrinsic_fast`, `model_capabilities.zig:117-119`): xfx sends no speed
/// of its own yet.
fn switched(next_turn: bool, model: &str, effort: &Effort) -> String {
    let lead = if next_turn {
        "Next turn will use "
    } else {
        "Switched to "
    };
    let speed = if model.ends_with("-fast") {
        "fast"
    } else {
        "normal"
    };
    format!(
        "{lead}{model} (effort: {}, speed: {speed})",
        effort.display()
    )
}

/// What labels the sentence a user attached to a permission decision.
///
/// **Its own label, beside `[tool]` and `[shell]` rather than one of them**,
/// because it is a different kind of claim: those two rows are xfx reporting
/// what it did, and this row is the user's own words being read back to them
/// after they were delivered to the model. A sentence painted as `[tool] …`
/// would read as the tool's account of what it did.
pub(crate) const AMENDMENT_PREFIX: &str = "[you]";

/// How much of an amendment the document row carries.
///
/// Longer than [`TOOL_DETAIL_BYTES`], because this one is a sentence a person
/// wrote on purpose and the point of the row is that they can check it against
/// what they meant; still bounded, because the draft behind it holds up to four
/// kibibytes ([`super::approval_amendment::MAX_FEEDBACK_BYTES`]) and a
/// transcript row is not where the whole of one belongs. The model gets the
/// text whole either way -- this is a display of it, not the record.
const AMENDMENT_BYTES: usize = 240;

/// What a screen that changed size did to the band.
///
/// Three answers rather than a `bool`, because the caller owes a different
/// thing for each and two of them owe nothing at all
/// (`super::event_loop::resolve_resize`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Resize {
    /// The screen is the size it already was, or it would not say what size it
    /// is. Neither is news, and neither costs a frame.
    Unchanged,
    /// The band was re-solved and everything the terminal is holding is now a
    /// claim about a screen that no longer exists.
    Repaint(Geometry),
    /// No band fits on this screen. Nothing is re-solved, **nothing is
    /// answered**, and the band is left where it was until the screen can hold
    /// one again.
    TooSmall,
}

/// The UI's state: what the band shows, and what it owes the screen.
pub(crate) struct Shell {
    pub(crate) geometry: Geometry,
    /// The screen's size as the terminal last reported it.
    ///
    /// The same two numbers as [`Self::geometry`] for as long as a band fits on
    /// it, and a **separate fact** for exactly the window in which one does
    /// not: [`Resize::TooSmall`] leaves the geometry describing the screen the
    /// band was last solved for, so a shell that asked its geometry "what size
    /// is the screen" would answer a size the terminal has contradicted -- and
    /// a window dragged small and back to where it started would be reported as
    /// no news, leaving the band solved for a screen the terminal re-wrapped
    /// twice.
    screen: (u16, u16),
    pub(crate) render: RenderRequest,
    /// The colours the band paints its own rows in.
    ///
    /// Started from the terminal xfx was launched in ([`super::theme`]) and
    /// moved by every theme report the terminal sends afterwards
    /// ([`Self::retint`]). Held here rather than consulted per frame for the
    /// reason [`Self::model`] is -- one field, not a borrow of the launch.
    palette: Palette,
    /// Whether the user fixed the palette themselves, in which case no report
    /// may move it.
    ///
    /// Decided at launch and never again: it is exactly "`XFX_THEME` named one
    /// of the two modes" (`super::theme::decided`), which is the one source
    /// that outranks the terminal. Carried in rather than read here, so that
    /// nothing in this module touches the process environment and a case about
    /// a locked session is a constructor argument rather than a global.
    ///
    /// **`COLORFGBG` is not a lock.** It is a guess a terminal exports once and
    /// never updates, and it is consulted only when the terminal would not
    /// answer at all; a session that started from it must still follow the
    /// terminal when the terminal finally says something.
    theme_locked: bool,
    /// Whether the terminal owes an answer to [`super::theme::MODE_QUERY`] that
    /// nothing has written yet.
    ///
    /// Set when something happened that may have changed the background while
    /// this session was not listening -- a stop and continue, where the user's
    /// shell had the terminal and the subscription was gone with it -- and
    /// taken by the loop's paint tick, which is the one place in the session
    /// that writes to the terminal under a budget.
    theme_query: bool,
    /// Whether the document rows already on the screen are still painted in a
    /// palette the session has moved on from.
    ///
    /// Raised by [`Self::retint`] and paid by the loop's writer against the
    /// band's own shadow ([`super::frame::Band::retint_document`]), which is
    /// the only thing that knows which of those rows it wrote. Held as a debt
    /// rather than written at the report for the reason [`Self::theme_query`]
    /// is: a report is decoded on the input side, and the one writer on this
    /// terminal is the paint tick.
    ///
    /// **It carries no palette of its own.** What is owed is "the visible
    /// document, in whatever palette is in force when it is paid", so a
    /// terminal that reports dark, light and dark again before any of it
    /// reaches the screen converges on dark and costs nothing -- which a debt
    /// remembering an old-to-new pair could not do.
    retint_debt: bool,
    /// Whether something that is not the band has written on the screen and
    /// the band has not been told yet.
    ///
    /// Raised beside every [`Reason::ExternalDamage`] this shell asks for --
    /// and by a resize, which invalidates on its own road -- and lowered by
    /// the loop at the point it really tells the band
    /// ([`super::frame::Band::invalidate`]).
    ///
    /// It exists because the two facts are recorded at **different moments of
    /// one tick**: a `/clear` or a Ctrl-L is applied while events are, and the
    /// band is told while the frame is built -- and the retint above is paid
    /// in between, off a shadow that still describes the screen that was. A
    /// report that arrives behind a `/clear` in the same batch would otherwise
    /// repaint erased history back onto a screen the user emptied.
    damage_untold: bool,
    /// The model a turn will talk to.
    ///
    /// Read from the configuration once, at startup, rather than consulted per
    /// frame: the hint row renders a compact form of it and a `/model` change
    /// replaces it, and both want one field rather than a borrow of the whole
    /// configuration.
    model: String,
    /// How much authority a turn will have before it has to ask.
    ///
    /// Read from the configuration at startup, and moved by `/permissions` and
    /// Shift-Tab ([`Self::choose_mode`]) **the moment the runtime has been
    /// handed the change** -- which is what the hint row says from then on. The
    /// runtime applies it before the next turn starts and not under one that
    /// is running (`super::worker`'s `turn_loop`), so while a turn is in
    /// flight this is the mode the *next* one will have.
    mode: PermissionMode,
    /// The one-time full-access warning on the hint row, and how much of its
    /// time on the screen is left (`super::permissions::Warning`).
    warning: permissions::Warning,
    /// Whether this person has been shown that warning: the profile's
    /// `yolo_acknowledged` at startup, and set the first time it is on the
    /// screen here. Once true it stays true, which is what makes the warning
    /// once per person rather than once per session.
    acknowledged: bool,
    /// Whether the configured provider has nothing to authenticate with.
    ///
    /// Asked of the **provider** rather than of one field
    /// ([`crate::provider::resolve_credential_for`]), because the two providers
    /// need different things and both refuse every turn without them: a Gateway
    /// session with no bearer credential and a llmux session with no
    /// `llmux_url` are the same fact on this row. Settled at startup like
    /// [`Self::model`], because both of its inputs are the environment and the
    /// profile, and neither moves under a running session.
    missing_credential: bool,
    /// The provider a turn will talk to.
    ///
    /// Beside [`Self::model`] and settled the same way -- from the
    /// configuration at startup, and from [`UiEvent::ProviderSelected`]
    /// afterwards. It is what a catalog row is *about*: entries loaded for one
    /// provider say nothing about another, so the two move together or the
    /// browser lists one daemon's models under another's name.
    provider: ProviderId,
    /// The rows the last catalog load produced, for the provider above.
    ///
    /// Two readers and one of them is not obvious: the browser paints them, and
    /// the hint row's context meter takes its **denominator** from the entry
    /// matching the model in force ([`CatalogEntry::max_context`]). Empty until
    /// a load has succeeded, and emptied on a provider switch, because a window
    /// published by the daemon xfx has stopped talking to is not this
    /// conversation's window.
    catalog: Vec<CatalogEntry>,
    /// What this side knows of the catalog above: whether [`Self::catalog`] is
    /// an answer -- possibly an empty one -- or nothing has been asked, or the
    /// runtime is being asked, or it could not read it.
    ///
    /// **Three readers decide by it and not by whether the rows are empty**:
    /// whether a `/model` asks the runtime again, what the inline column says
    /// in place of rows, and whether a typed query can be resolved -- and
    /// announced -- on this side ([`Self::request_model`]). An empty answer is
    /// an answer: a provider with no catalog applies a query as typed, which
    /// this side can say at once; a catalog nobody has read cannot be
    /// resolved here at all.
    catalog_known: CatalogKnown,
    /// The session's reasoning effort, as the band shows and announces it.
    ///
    /// The runtime's is the one a request is sent with
    /// (`super::worker`'s `Runtime::effort`); this one starts from the same
    /// profile and moves with every change this side asks for -- at once, the
    /// moment it is asked for, because that is when the band says so -- and
    /// with every [`UiEvent::ModelAnswered`], which carries the runtime's.
    effort: Effort,
    /// `/model`'s inline column picker ([`super::model_column`]): which column
    /// is open over the composer's `/model ` line, which model is pending and
    /// which row is marked. A view of the composer like the completion menu,
    /// reconciled with it on every change ([`Self::reconcile_column`]).
    column: Column,
    /// The composer's text as the column last saw it, which is how it tells an
    /// edit from a caret move.
    column_seen: String,
    /// What the change being reconciled was, as far as the column's later
    /// stages care: set by the typing and the Backspace that may keep them, and
    /// taken by the reconcile.
    column_edit: ColumnEdit,
    /// What the last **completed** turn reported as its input tokens.
    ///
    /// The meter's numerator, and it is `input_tokens` alone rather than
    /// input-plus-output on purpose: what a context meter answers is "how much
    /// of the window does the next request carry", and the next request carries
    /// the conversation the provider has just counted as its input. Adding the
    /// output would count this turn's answer twice -- once as output now, and
    /// again inside the input of the turn after it.
    ///
    /// `None` until a turn has finished and said so. Absent is not zero: a
    /// provider that publishes no usage gets no meter rather than a nought.
    context_used: Option<u64>,
    /// The text being composed, and where the caret is in it.
    editor: Editor,
    /// The paste that is arriving, and the numbers this session has spent.
    ///
    /// Held beside the editor rather than inside it because a paste is *not* an
    /// edit until it is finished: the bytes between the markers are content
    /// being filtered and counted, and only [`Action::PasteEnd`] decides
    /// whether what the composer receives is the text or a summary standing in
    /// for it ([`super::paste`]). The blocks themselves live in the composer,
    /// as spans of the draft (`super::entity`).
    paste: Paste,
    /// What the composer's edits were, and what was last killed out of it.
    ///
    /// **`edit_history`, never `history`.** [`Self::history`] below is the
    /// prompt recall -- the lines this session has submitted -- and it has no
    /// undo. This is the composer's own history: bounded deltas
    /// ([`super::edit_history`]), fed by [`Self::edited`], walked by
    /// [`Action::Undo`] and [`Action::Redo`], and emptied at every whole-draft
    /// boundary ([`Self::take_draft`]).
    edit_history: EditHistory,
    /// The lines this session has submitted, and where a walk back through
    /// them has got to.
    ///
    /// Beside the editor rather than inside it, for the reason [`Self::paste`]
    /// is: what an entry holds is the draft *as the composer held it*, and the
    /// composer is the thing being replaced rather than the thing that decides
    /// to replace it. Recording is [`Self::submit`]'s -- the one place a
    /// submitted line still exists -- and leaving is every edit's
    /// ([`Self::amended`]).
    history: History,
    /// The one input machine of the session.
    ///
    /// Held here rather than in the loop because it is *state a keystroke can
    /// be half-way through*: a `CSI` split across two reads, a scalar split
    /// across two, a paste that spans a hundred. A decoder made per read would
    /// begin each of them again.
    decoder: Decoder,
    /// The answer text that has not ended its line yet, the rows it has put on
    /// the screen, and every write the document is owed and has not been given.
    ///
    /// The owed writes live there rather than here, as the **text** they were
    /// made of, because what they cost the screen is a function of how wide the
    /// screen is when they are written -- and a row measured here, against the
    /// width the session happened to have when the delta arrived, is a row the
    /// terminal clips after a resize it never saw (`super::transcript`).
    transcript: Transcript,
    /// What the runtime has produced and the document has not been given yet.
    ///
    /// Every byte of an answer goes through here, which is what makes the
    /// stream steady rather than bursty (`super::pacer`). What it costs is a
    /// second place text can be waiting, and the two rules that pay for it are
    /// [`Self::pace`] -- run once a turn, so nothing sits here longer than a
    /// tick past its due time -- and [`Self::flush_paced`], which the exit path
    /// runs so that a session coming down never takes an answer with it.
    pacer: Pacer,
    /// The document writes that belong *after* text still in the pacer, each
    /// with the stream position it was issued at.
    ///
    /// A tool notice, a refusal, the echo of a submitted prompt and the end of
    /// a turn are xfx's own words rather than the provider's, so they do not go
    /// through the pacer -- but they still have a **place** in the document,
    /// and it is the place the stream had reached when they happened. Held
    /// against a byte count rather than against "the queue is empty", because a
    /// second turn's deltas can be enqueued behind the first turn's tail and
    /// the first turn's conclusion belongs between them.
    marks: VecDeque<(usize, Mark)>,
    /// How many bytes have been enqueued for pacing, ever.
    enqueued: usize,
    /// How many of them have reached the transcript.
    emitted: usize,
    /// Where a submitted line goes.
    work: WorkHandle,
    /// What the keystroke before this one was, for the two keys whose second
    /// press means something else.
    gestures: Gestures,
    /// The refusal the hint row is showing, if it is showing one.
    ///
    /// `'static`, because everything that lands here is text this crate wrote:
    /// a row that could carry a provider's words would be a row that can carry
    /// a provider's escape sequences.
    notice: Option<&'static str>,
    /// Whether a second Escape would clear the composer, as of the last settle.
    ///
    /// Cached rather than asked at paint time because [`Self::band_rows`] has
    /// no clock: what is painted has to be the same answer that asked for the
    /// frame, or the row and the reason for it disagree.
    escape_armed: bool,
    /// Whether a Ctrl-C would leave, as of the last settle -- cached for the
    /// reason [`Self::escape_armed`] is.
    exit_armed: bool,
    /// Whether a second Escape would stop the work in hand, as of the last
    /// settle.
    interrupt_armed: bool,
    /// Whether this session asked the turn in hand to stop and has not yet
    /// been told it did.
    ///
    /// Between the two, the turn's own words are already off the screen
    /// ([`Self::cancel_turn`]) and anything still on its way across the
    /// channel is the tail of an answer the user stopped: it is dropped
    /// rather than painted under the line that says it was cancelled, and the
    /// conclusion that ends it says nothing the line has not.
    cancelling: bool,
    /// The turn's `■ Cancelled` line, while it waits for a running call to
    /// settle ([`Self::cancel_turn_with`]), and whether the queue notice waits
    /// with it.
    cancel_owed: Option<bool>,
    /// How many of the turns still to start were submitted before the cancel,
    /// and so are the cancelled work starting late rather than a new turn
    /// ([`UiEvent::TurnStarted`]).
    cancel_covers: usize,
    /// The retry the turn is waiting on, while it is ([`super::retry`]).
    ///
    /// It replaces the activity row rather than sitting beside it, as
    /// upstream's recovery label does (`worker_status.zig:41-49`), and it goes
    /// the moment the turn shows any sign of having got through -- an answer,
    /// a tool, a question -- or of being over.
    retry: Option<super::retry::Standing>,
    /// How many submissions are waiting behind the one in flight, as of the
    /// last settle. Cached for the same reason [`Self::escape_armed`] is.
    queued: usize,
    /// What the turn is doing, and how long it has been doing it.
    activity: Activity,
    /// Which phase of the activity row's blink the band is on.
    ///
    /// Counted in phases rather than read off a clock, and moved on by the
    /// render request's animation tick ([`super::render_request`]), which is
    /// what makes the blink a multiple of that tick rather than a second clock
    /// beside it.
    phase: u8,
    /// The question the turn is waiting on an answer to, while it is waiting.
    ///
    /// `Some` is the whole of "the panel has the focus": the band paints it,
    /// the geometry gives it rows, the caret sits on its marked choice, and
    /// every keystroke goes to it rather than to the composer. Held here rather
    /// than derived from an event, because the answer is a keystroke and the
    /// keystroke has to find something to be an answer *to*.
    panel: Option<Panel>,
    /// The same question, when the change behind it is too big for the band to
    /// show and it is being reviewed on a plane of its own
    /// ([`super::approval_screen`]).
    ///
    /// Beside [`Self::panel`] rather than inside it, and **never both at once**
    /// ([`Self::ask`] installs exactly one): the two surfaces answer the same
    /// question with the same keys, but only one of them is painted, only one
    /// takes rows out of the band, and only one has a caret on the band's own
    /// grid. A single field holding either would have every reader ask which
    /// kind it was.
    alternate: Option<ApprovalScreen>,
    /// The id the standing question was asked under, while one is standing.
    ///
    /// Beside the two surfaces rather than inside either, because it is the one
    /// fact that is the *same* on both and the answer has to carry it whichever
    /// plane the question was shown on. `Some` exactly when [`Self::asking`] is
    /// true: [`Self::ask`] sets it with the surface, and every way out --
    /// [`Self::decide`] and [`Self::release_screen`] -- clears it with the
    /// surface, in the same statement.
    asked_id: Option<ApprovalId>,
    /// What makes an affirmative answer to that question answerable.
    ///
    /// A module of its own rather than four more fields here
    /// ([`super::approval_readiness`]): what it holds is a state machine with
    /// its own contract, and the shell's job is only to tell it what was
    /// composed, what the terminal did with it, and when the loop came round.
    readiness: Readiness,
    /// The batch of questions the **model** asked, while the user is answering
    /// it ([`super::question`]).
    ///
    /// Beside [`Self::panel`] rather than inside it, and never both at once
    /// ([`Self::slot`]'s assertion): a question grants nothing, it is refused
    /// rather than denied, and its answer is text -- so an `Option<Panel>` that
    /// could hold either would have every reader ask which kind it was before it
    /// could say what a keystroke meant. What the two share is the band's
    /// elastic slot and the focus, and that is stated once, in [`Self::slot`].
    ask: Option<QuestionPanel>,
    /// Which plane the session is composing frames for.
    ///
    /// Taken when a question arrives that the band's review cannot show, and
    /// given back the instant that question is answered -- by a choice, by a
    /// refusal, or by the interrupt that refuses it on the user's behalf. Held
    /// beside [`Self::panel`] rather than derived from it because the two are
    /// different facts: the panel says a question is up, this says whose screen
    /// it is up on.
    owner: ScreenOwner,
    /// The completion menu the draft is asking for, while it is asking for one.
    ///
    /// The other occupant of the band's elastic slot, and the two are mutually
    /// exclusive by construction: [`Self::ask`] closes a menu before it
    /// installs a question, and [`Self::fit`] reads the question first. What a
    /// menu is **not** is a second [`Self::panel`] -- it never takes the caret
    /// and it swallows only the five keys it binds ([`super::picker`]).
    picker: Option<Picker>,
    /// The trigger whose menu the user closed, while it is still the trigger.
    ///
    /// Held here rather than in the menu, because it has to outlive one: what
    /// it is for is that the *next* keystroke does not re-open what Escape just
    /// closed.
    dismissed: Dismissed,
    /// The `/help` catalog, while it is up ([`super::help_menu`]).
    ///
    /// The third occupant of the band's elastic slot, and like the completion
    /// menu a view of the composer rather than a focus: what is typed goes on
    /// going into the composer and is the catalog's search. The two are
    /// mutually exclusive by construction -- [`Self::reconcile_picker`] opens
    /// no completion menu while this is `Some`, and `/help` closes the menu
    /// before it opens this. A question is not excluded: it outranks the
    /// catalog in [`Self::slot`] and takes the keys while it is up, and the
    /// catalog is back when it has been answered, as upstream's is
    /// (`surface_frame.zig:401`, `!modal_active`).
    help_menu: Option<HelpMenu>,
    /// The `/resume` picker, while it is up ([`super::session_picker`]).
    ///
    /// The fourth occupant of the band's elastic slot, and like the catalog a
    /// view of the composer: what is typed is its search. The catalog and it
    /// are never both up -- each closes the other when it opens.
    sessions: Option<SessionPicker>,
    /// Whether the screen owes a resume's terminal reset
    /// (`super::frame::Band::wipe`): asked by the loop, settled only by a
    /// reset that landed ([`Self::wiped`]), and ahead of a handoff, which it
    /// makes moot.
    wipe: bool,
    /// `/model`'s menu, while it is up ([`super::model_menu`]).
    ///
    /// The fourth occupant of the slot, and the same kind as the catalog: a
    /// view of the composer, which is its search, opened by a bare `/model`
    /// with the composer emptied. It and the catalog cannot both be up --
    /// `/model` reached from the catalog closes the catalog on the way -- and
    /// no completion menu opens while it is ([`Self::reconcile_picker`]).
    model_menu: Option<ModelMenu>,
    /// What `/model`'s menu says about a Gateway catalog under its rows
    /// ([`crate::provider::model::gateway_catalog_note`]), asked once of the
    /// credential this session launched with: a provider switch re-reads the
    /// file, not the environment the credential comes from.
    gateway_note: Option<&'static str>,
    /// Which of the footer's optional segments are shown: the profile's
    /// `statusLine` at launch, and every `/statusline` change since.
    statusline: StatusLine,
    /// The `/statusline` picker, while it is up ([`super::statusline::Menu`]).
    ///
    /// The fourth occupant of the band's elastic slot, and unlike the catalog
    /// and the completion menu **a focus rather than a view**: the composer
    /// row is not shown while it is up and every keystroke is the picker's
    /// (`routeActiveModalInput`, `app_input_runtime.zig:1366-1395`). A
    /// question outranks it in [`Self::slot`] and takes the keys while it is
    /// up; the picker is back when it has been answered.
    statusline_menu: Option<statusline::Menu>,
    /// The workspace's path and the Git branch its `HEAD` was last read on,
    /// for the workspace segment ([`super::statusline::Workspace`]).
    workspace_identity: statusline::Workspace,
    /// Where a `/statusline` change is kept: `~/.xfx/settings.json`, or `None`
    /// for a session with no home to keep it in.
    settings_path: Option<std::path::PathBuf>,
    /// The `/provider` picker, while the draft is its argument
    /// ([`super::provider_picker`]).
    ///
    /// Another occupant of the slot, and a view of the composer like the
    /// catalog and the completion menu: it opens when the text begins
    /// `/provider ` or `/setup ` and closes when it stops doing so, both in
    /// [`Self::reconcile_picker`].
    /// It cannot be up beside the completion menu -- a draft with a space in
    /// it is not a slash word ([`Trigger::of`]) -- nor beside the catalog,
    /// whose search the draft is while it is up.
    provider_picker: Option<ProviderPicker>,
    /// Whether Escape closed the `/provider` picker over the text the draft
    /// still begins with.
    ///
    /// [`Self::dismissed`]'s job for this picker: it stays closed while the
    /// argument is edited, and is forgotten once the draft stops being the
    /// picker's (upstream's `dismissed_until_trigger_change`,
    /// `picker_state.zig:36-46`). A flag rather than a [`Trigger`] variant,
    /// because whether the draft is the picker's is a property of the whole
    /// text that [`Trigger::of`] -- one slash word -- does not read.
    provider_dismissed: bool,
    /// What that row says, as of the last settle, or `None` while there is no
    /// work to say anything about.
    ///
    /// Cached for the reason [`Self::escape_armed`] is -- [`Self::band_rows`]
    /// has no clock -- and for one more that is this row's own: whether the
    /// band **has** the row is a fact of the geometry, and the geometry is
    /// re-solved from exactly this field, so the row's presence and its text
    /// cannot disagree.
    activity_row: Option<String>,
    /// Whether the screen owes a reset's scrollback handoff
    /// (`super::frame::Band::hand_off`), and how many blank rows the document
    /// it hands off ends on.
    ///
    /// Asked by the loop, which owns the writer, and settled only by a handoff
    /// that landed ([`Self::handed_off`]). One value rather than a queue of
    /// writes because two resets before one tick are one handoff: the second
    /// has no document of its own to hand off.
    handoff: Option<u16>,
    /// Whether the rows above the band may no longer end the way the
    /// transcript's own record of its tail says ([`Transcript::ends_blank`]).
    ///
    /// Raised by a resize, after which the terminal has reflowed them, and by
    /// anything else that damaged the screen from outside -- a resume after
    /// somebody else had the terminal above all. Cleared only by a handoff,
    /// which leaves a screen this session wrote every row of. While it is up a
    /// reset carries the blank row a document ends on into scrollback rather
    /// than guess that it is there ([`Self::handoff`]) -- and so does a reset
    /// already owed when it is raised.
    tail_unknown: bool,
    /// Whether the work a reset cancelled is still reporting: the count the
    /// last reset quoted, until the runtime's fence for it arrives.
    ///
    /// From the reset until the runtime says the conversation is gone
    /// ([`UiEvent::Fresh`] with that count or a later one). Everything that arrives in between belongs to the
    /// conversation the reset handed to scrollback -- the tail of an answer, a
    /// tool row, a turn's conclusion -- and painted on the fresh screen it
    /// would be a stray from a session the user just left. So it is dropped,
    /// except for the state the runtime really changed and the questions a
    /// parked turn is waiting on ([`Self::drained`]).
    draining: Option<usize>,
    /// The answer as upstream presents it: the model's markdown rendered as it
    /// streams (`assistant_stream.zig:435-457`), its prose in the two-column
    /// gutter and its code, tables and rules as blocks of their own.
    markdown: MarkdownProcessor,
    /// Where the answer being written stands ([`Answer`]).
    answer: Answer,
    /// The tool calls the transcript is showing as one group, and whether its
    /// block is the document's open tail ([`super::toolgroup`]).
    ///
    /// Moved only where the stream reaches the call -- by the marks the tool
    /// events leave ([`Mark::ToolStart`], [`Mark::ToolResult`]) -- so the group
    /// lands where the answer had got to when the call happened, as every other
    /// line of xfx's does.
    group: ToolGroup,
    /// The calls this turn settled as cancelled while they were still running.
    ///
    /// A stop is shown when the user asks for it ([`Self::interrupt`]), and the
    /// runtime may already have sent the result of a call that finished in the
    /// same moment. That result is about a row the screen has already settled
    /// the other way, so it is dropped rather than drawn as a second answer to
    /// one call. Emptied when the turn is over ([`Mark::TurnOver`]): the
    /// runtime sends a turn's conclusion after everything else of it.
    abandoned: Vec<String>,
    /// xfx's own lines written while the tool group was open, which are the
    /// group's block's last rows until it closes ([`Self::trail_group`]).
    trailer: Vec<Trailed>,
    /// What the summary under the turn's answer will say ([`Tally`]).
    tally: Tally,
    /// The token estimate of every prompt that was sent and whose turn has
    /// not started yet, oldest first: a prompt may wait behind another turn,
    /// and the summary counts the prompt of the turn it is under.
    ///
    /// Beside each, the title that prompt would give a session it opened
    /// ([`title_of`], the store's own rule, so the window says what a listing
    /// of the session says): which prompt opens the session is only known
    /// when its turn starts.
    ///
    /// The third is the prompt's card while it is **waiting**: a prompt
    /// submitted while the runtime had work in hand is shown in the band
    /// ([`Self::banner_rows`]) and its card is written when its turn starts,
    /// as upstream delivers a steer (`fx-steer-40x120-steer-done`) -- never at
    /// the keystroke, in the middle of the answer it was typed under.
    prompts: VecDeque<(u64, Option<String>, Option<String>)>,
    /// The title of the session this conversation is recorded in, once its
    /// first turn has finished -- upstream's cached session title
    /// (`ensureCachedSessionTitle`, `app_session_runtime.zig:3640-3655`), which
    /// is what the window title shows once there is one
    /// ([`super::frame::title`]).
    session_title: Option<String>,
    /// The title the running turn's prompt would give its session, held from
    /// the turn's start to its end ([`Self::title_session`]).
    turn_title: Option<String>,
    /// Whether the band has not yet been told the window title the session
    /// wants now ([`Self::take_title`]). `true` at launch, so the first frame
    /// carries one.
    title_owed: bool,
    /// The directory the session works in, whose last component the window
    /// title names until the session has a title of its own.
    workspace: std::path::PathBuf,
    /// How far down the screen the document reaches, for the band that floats
    /// under it ([`Geometry::floated`]).
    ///
    /// Every row an append added, less every row a growing band carried off
    /// the top: so while the screen is not full it is the document's last row,
    /// and once the screen has filled it is at least the rows above the band
    /// and the band stays at the bottom. A reset's handoff starts it again at
    /// none ([`Self::handed_off`]). A
    /// resize sets it past any screen, because where a terminal reflows the
    /// rows already on it is the terminal's own business and a band floated
    /// under a guess would be painted over them.
    document_rows: u32,
    /// Why the session is ending, when it is ending because the runtime cannot
    /// go on. Printed by the caller **after** the terminal has been restored.
    fatal: Option<String>,
    /// How the session is ending, once it is.
    leaving: Option<Leaving>,
    /// The `/copy`s still running on their own threads, and how each went
    /// ([`super::copy`]).
    copies: Copies,
    /// `/sound`'s two switches, the bells they owe the loop, and the saves
    /// that keep them ([`super::sound`]).
    sound: Sound,
    /// What the terminal writer has done this session, for `/stats`.
    ///
    /// Counted by the loop that does the writing (`super::event_loop`), which
    /// is the only place a byte reaching the terminal or a repaint landing is
    /// known; held here because `/stats` is answered here.
    pub(crate) metrics: Metrics,
    /// The conversation as the document was given it, and the screen `C-o`
    /// shows it on while that screen is up ([`super::full_transcript`]).
    ///
    /// Told about each write as it is made -- the card, the released answer,
    /// a notice, a call's row, the summary ([`Self::record`]) -- so it is in
    /// the document's order; told what the document never gets -- a call's
    /// arguments and its whole result -- by the runtime's journal
    /// ([`UiEvent::ToolDetail`]).
    full: FullTranscript,
}

/// What `/stats` reports: the terminal writer's counters.
///
/// Upstream's `Metrics` (`vercel-labs/fx@c1db9191
/// src/core/shared/types.zig:15-21`), less the two it never increments
/// ([`Metrics::body`]). Each counter is one named event and nothing else, so
/// the three cannot count the same thing twice.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Metrics {
    /// Bytes the terminal took, as the kernel counted them
    /// (`super::deliver::Sink::emit`): every frame, append, clear and query the
    /// loop wrote, and the delivered prefix of one that failed part way.
    /// Upstream counts the same thing at its frame sink
    /// (`src/ui/transcript/io.zig:58,67`).
    pub(crate) ansi_bytes: u64,
    /// Whole repaints that **landed** because something other than the band
    /// wrote on the screen -- a resume, a `/clear`, a Ctrl-L: a frame whose
    /// attempt was damaged (`super::render_request::Attempt::damaged`) and
    /// that the terminal took. The launch's first frame is whole too and is not
    /// one of these, since nothing damaged it; nor is the repaint after a
    /// resize, which is [`Self::debounced_resizes`]'s. Upstream's own counter
    /// is reached only from its tests at `c1db919` (`.reanchor_top`,
    /// `src/ui/resize_runtime.zig:167`), so its `/stats` prints `redraws=0`
    /// where xfx's can print more.
    pub(crate) redraws: u64,
    /// Bursts of `SIGWINCH` answered with one re-solve at a size that really
    /// changed (`super::render_request::RESIZE_DEBOUNCE`), whether or not a
    /// band fits on it; a burst that ended where it began costs nothing and
    /// counts nothing, as upstream's returns before counting
    /// (`src/ui/resize_runtime.zig:275-283`).
    pub(crate) debounced_resizes: u64,
}

impl Metrics {
    /// The `/stats` body, in upstream's words and order
    /// (`commandShowStats`, `src/core/app/app_commands.zig:1009-1022`).
    ///
    /// `footer_updates` and `stream_chunks` are written as `0` because that is
    /// what upstream prints for them, always: nothing in it increments either
    /// (their only assignment outside the formatter is a test fixture,
    /// `app_input_runtime.zig:7775-7776`). Kept so the line reads the same on
    /// both, and literal so nobody mistakes them for something xfx measures.
    pub(crate) fn body(&self) -> String {
        format!(
            "ansi_bytes={}, redraws={}, debounced_resizes={}, footer_updates=0, stream_chunks=0",
            self.ansi_bytes, self.redraws, self.debounced_resizes
        )
    }
}

/// Which plane the session is composing frames for.
///
/// A **separate enum**, not another meaning of `Option<Panel>`. The band's
/// elastic slot answers "what is in the band"; this answers "whose screen is it"
/// -- and the two really can disagree, because a question can be asked on a
/// surface the band is not painting. Collapsing them would make a frame's plane
/// a function of whether a field happened to be `Some`, which is exactly the
/// kind of implicit state a restoration path cannot check.
///
/// This checkpoint takes the state and gives it back; it emits no alternate
/// screen. The renderer, the `1049` bytes that enter and leave one, and the
/// exit/panic/signal restoration that has to account for the owner are the next
/// one's -- and the order is deliberate: the state a restore path reads has to
/// exist and be correct before anything can be written that depends on it being
/// given back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ScreenOwner {
    /// The normal buffer: the user's document, with xfx's band at the bottom.
    /// The main TUI surface never gives this up.
    #[default]
    Primary,
    /// The alternate buffer: a question with a change too large for the band
    /// to show, or the full transcript (`C-o`, [`super::full_transcript`]).
    ///
    /// One variant for both: they are the same plane to everything that
    /// reads this -- the loop that takes and gives it back, the exit, the
    /// panic hook -- and which of the two is on it is the shell's own
    /// business ([`Shell::screen_rows`]).
    Alternate,
}

/// A question composed for the surface it belongs on, before anything is
/// installed.
///
/// The value [`Shell::ask`] decides with. It exists so that the fit question --
/// which is a *different* question on the two surfaces -- is asked of the thing
/// that would actually be painted, and so that the refusal for both is written
/// once: a second `say`/`Deny` pair in a second arm is a second chance to
/// forget the rule that a decision xfx was never given is a refusal.
enum Asked {
    /// The band's own panel, with the document still above it.
    Inline(Panel),
    /// A screen of its own, for a change the band's review cannot show.
    Alternate(ApprovalScreen),
}

/// One of xfx's own lines written while a tool group was open
/// ([`Shell::trail_group`]): what it is, so it can be laid out again at the
/// width of whatever screen the block is next written to.
#[derive(Debug, Clone)]
enum Trailed {
    Line(String, Role),
    Notice(notice::Notice),
}

/// Queues one whole line onto `transcript`, on rows of its own -- the
/// transcript half of [`Shell::write_document_line`].
pub(crate) fn queue_line(transcript: &mut Transcript, line: &str, role: Role) {
    if transcript.open_line() {
        transcript.queue_end_line();
    }
    transcript.queue_push(line, role);
    transcript.queue_end_line();
}

/// Queues one notice onto `transcript` as upstream renders one -- the
/// transcript half of [`Shell::write_notice`], whose documentation says what
/// the shape is -- and says whether the document is owed a frame for it.
pub(crate) fn queue_notice(transcript: &mut Transcript, notice: &notice::Notice) -> bool {
    let mut owed = false;
    // The gutter in force is the one the row above is written with.
    owed |= transcript.queue_gutter(Gutter::None);
    if !transcript.ends_blank() {
        owed |= transcript.queue_end_line();
    }
    owed |= transcript.queue_gutter(Gutter::Head(notice.tone));
    // One push for the label and the whole body: the head is one row's, and
    // every line after the first is written two columns in by the gutter
    // itself (`Gutter::continued`), so nothing here splits lines.
    let label = notice.label();
    let body = notice.said();
    let gap = if label.is_empty() || body.is_empty() {
        ""
    } else {
        " "
    };
    owed |= transcript.queue_push_runs(&[
        (&label, Role::Label(notice.tone)),
        (gap, Role::Notice),
        (body, Role::Notice),
    ]);
    owed |= transcript.queue_end_line();
    owed |= transcript.queue_gutter(Gutter::None);
    owed |= transcript.queue_end_line();
    owed
}

/// One of xfx's own document writes, waiting for its place in the stream.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Mark {
    /// The line the user submitted, as upstream's card: the rail, the text in
    /// bold, and a blank row under it (`user_message_card.zig:319-325`).
    Card(String),
    /// The turn's summary under its answer, and a blank row under it
    /// (`transcript/runtime.zig:540-547`).
    Summary(String),
    /// A whole line, on rows of its own, and what it is.
    ///
    /// The role is carried rather than decided where the mark is run, because
    /// the two things that reach the document this way are not the same thing:
    /// xfx's own sentences about the session are [`Role::Notice`], and the echo
    /// of a line the **user** submitted is [`Role::Plain`] -- it is their text,
    /// and colouring it as xfx's words would be this surface claiming to have
    /// said it.
    Line(String, Role),
    /// One of xfx's own notices, with a blank row above and below it
    /// ([`Shell::write_notice`]).
    Notice(notice::Notice),
    /// Rows laid out already -- wrapped at the screen they were composed for
    /// and painted in the palette's own colours -- each with what it is, as
    /// one block of the document with a blank row above and below it
    /// ([`Shell::write_block`]). What a question batch leaves behind
    /// ([`super::question`]).
    Block(Vec<(String, DocumentRow)>),
    /// The end of the answer's line, and nothing else. What a turn that ended
    /// without a failure owes: the next thing written starts on a new row.
    EndOfLine,
    /// A tool call started: a row in the open group, or a group of its own.
    ToolStart { id: String, shown: Shown },
    /// A tool call finished: its row settled, and the header recounted.
    ToolResult {
        id: String,
        ok: bool,
        detail: String,
        outcome: CallOutcome,
        changes: Option<LineChanges>,
    },
    /// The turn is over: whatever of its group is still running was stopped
    /// with it, and the group is closed.
    TurnOver,
}

/// The answer being written: whether its prose block is open, and the line
/// breaks that block ended on so far, held back until something follows them.
#[derive(Debug, Default)]
struct Answer {
    open: bool,
    held: usize,
}

/// Upstream's streaming token estimate (`token_estimate.zig:5-31`): a
/// whitespace-separated span of `n` bytes is `ceil(n / 4)` tokens, and the
/// count is the same however the text was cut into chunks.
#[derive(Debug, Default, Clone, Copy)]
struct Estimator {
    settled: u64,
    span: u64,
}

impl Estimator {
    fn consume(&mut self, text: &str) {
        for byte in text.bytes() {
            if byte.is_ascii_whitespace() {
                self.settled = self.settled.saturating_add(span_tokens(self.span));
                self.span = 0;
            } else {
                self.span = self.span.saturating_add(1);
            }
        }
    }

    fn estimate(&self) -> u64 {
        self.settled.saturating_add(span_tokens(self.span))
    }
}

fn span_tokens(span: u64) -> u64 {
    span / 4 + u64::from(!span.is_multiple_of(4))
}

/// What the summary under a turn's answer says: how long the turn took and the
/// tokens it moved (`transcript/runtime.zig:540-547`). In is the prompt's
/// estimate, as upstream counts it (`telemetry.zig:87-96`); out is what the
/// provider reported, or the estimate of what streamed when it reported
/// nothing.
#[derive(Debug, Default)]
struct Tally {
    started: Option<Instant>,
    input: u64,
    output: Option<u64>,
    streamed: Estimator,
}

impl Tally {
    fn begin(&mut self, now: Instant, input: u64) {
        *self = Tally {
            started: Some(now),
            input,
            ..Tally::default()
        };
    }

    fn line(&self, now: Instant) -> String {
        let seconds = self.started.map_or(0, |started| {
            now.saturating_duration_since(started).as_secs()
        });
        let output = self.output.unwrap_or_else(|| self.streamed.estimate());
        summary_line(seconds, self.input, output)
    }
}

/// A turn's summary row: how long it took, and the tokens it moved when it
/// moved any (`transcript/runtime.zig:540-547`). A live turn's and a replayed
/// one's are this one function, so a resumed screen says what the live one
/// said.
fn summary_line(seconds: u64, input: u64, output: u64) -> String {
    let mut line = format!("  {}", compact_duration(seconds));
    if input != 0 || output != 0 {
        line.push_str(&format!(
            " (\u{2191}{} \u{2193}{})",
            compact_tokens(input),
            compact_tokens(output)
        ));
    }
    line
}

/// `formatDurationCompact` (`transcript/runtime.zig:524-537`).
fn compact_duration(seconds: u64) -> String {
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 60 * 60 {
        format!("{}m {}s", seconds / 60, seconds % 60)
    } else {
        format!("{}h {:02}m", seconds / 3600, (seconds % 3600) / 60)
    }
}

use super::activity::compact_tokens;

/// Bold on, and every attribute off: what upstream's card puts around the
/// submitted text (`user_message_card.zig:319-325`).
const BOLD: &str = "\u{1b}[1m";
const RESET: &str = "\u{1b}[0m";

/// What the band's elastic slot is holding.
///
/// A borrow rather than a state: which of the two it is follows from the fields
/// ([`Shell::slot`]), so there is no third answer for a flag to drift into.
#[derive(Debug, Clone, Copy)]
enum Slot<'a> {
    /// A decision the turn is waiting on. It owns the focus.
    Question(&'a Panel),
    /// A batch of questions the model asked. It owns the focus too, and it is
    /// **not** the same thing: it grants nothing and its answer is text
    /// ([`super::question`]).
    Ask(&'a QuestionPanel),
    /// The `/statusline` picker. It owns the focus and the composer's rows.
    Statusline(&'a statusline::Menu),
    /// The `/help` catalog. Like the menu it owns nothing but rows, and the
    /// composer is its search.
    Catalog(&'a HelpMenu),
    /// The `/resume` picker. The same: rows, and the composer as its search.
    Sessions(&'a SessionPicker),
    /// `/model`'s menu, the catalog's kind: rows, and the composer as its
    /// search.
    Models(&'a ModelMenu),
    /// `/model`'s inline column, under the argument of a `/model ` line. Like
    /// the completion menu it owns nothing but rows: the typing goes on into
    /// the composer, whose text is what it is a view of.
    Column,
    /// The `/provider` column. Like the menu it owns nothing but rows, and the
    /// composer's argument is its query.
    Provider(&'a ProviderPicker),
    /// A completion menu for what the composer holds. It owns nothing but rows.
    Menu(&'a Picker),
}

/// What the band knows of the provider's catalog ([`Shell::catalog_known`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CatalogKnown {
    /// Nothing has asked for it this session, or for this provider.
    Unasked,
    /// The runtime has been asked and has not answered.
    Asked,
    /// The inline column wanted it while the queue was full; it is asked for
    /// again as soon as a keystroke or the end of a turn finds room.
    Wanted,
    /// The runtime's answer is [`Shell::catalog`].
    Loaded,
    /// The runtime could not read it, and whether asking again could help.
    Unread(Option<CatalogRetry>),
}

/// The catalog as `/model`'s column is drawn from it.
///
/// A function of two fields rather than a method, so a caller can hold it
/// while it changes the column -- a third field of the same shell.
fn known(state: CatalogKnown, catalog: &[CatalogEntry]) -> Known<'_> {
    match state {
        CatalogKnown::Loaded => Known::Rows(catalog),
        CatalogKnown::Unread(_) => Known::Failed,
        CatalogKnown::Unasked | CatalogKnown::Asked | CatalogKnown::Wanted => Known::Loading,
    }
}

/// How a session ended, which is the same question as what it exits with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Leaving {
    /// Ctrl-D, `/quit`, or a terminal with no writer left on it.
    Quit,
    /// The second Ctrl-C. The session exits [`INTERRUPTED_EXIT_CODE`], which is
    /// what the line-oriented shell exits with for the same gesture -- and what
    /// a caller reading `$?` uses to tell "the user stopped it" from "it
    /// failed".
    Interrupted,
}

impl Shell {
    pub(crate) fn new(
        config: &RuntimeConfig,
        geometry: Geometry,
        palette: Palette,
        theme_locked: bool,
        work: WorkHandle,
    ) -> Self {
        // The user's clipboard -- except in a shell a test built, which
        // reaches none unless the test hands it a fake one: typing `/copy`
        // in a case about something else must not be able to write the
        // clipboard of whoever is running the suite.
        #[cfg(not(test))]
        let clipboard = Clipboard::system();
        #[cfg(test)]
        let clipboard = Clipboard::searching(super::copy::NOWHERE);
        // The profile `/sound` saves into -- except in a shell a test built,
        // which saves nowhere unless the test hands it a file: the fixtures'
        // homes are gone by the time a case runs, and a save would create one
        // again wherever that was.
        #[cfg(not(test))]
        let settings = config.user_settings_path.clone();
        #[cfg(test)]
        let settings = None;
        Self {
            // Under a document of no rows: the launch pushed the shell's output
            // into scrollback, so the screen this session opens on is empty.
            geometry: geometry.floated(0),
            screen: (geometry.rows, geometry.cols),
            palette,
            theme_locked,
            // Nothing is owed at launch: the probe already asked
            // ([`super::probe::CursorProbe::ask`]), and asking again from the
            // first frame would be a second query on a terminal that is still
            // answering the first.
            theme_query: false,
            // Nothing on the screen to recolour, and nothing the band has not
            // been told: a session that has painted nothing has neither.
            retint_debt: false,
            damage_untold: false,
            // A session that has drawn nothing owes a frame. Requesting it here
            // rather than in the loop is what keeps "the band appears" a
            // property of having a shell at all.
            render: {
                let mut render = RenderRequest::default();
                render.request(Reason::FirstFrame);
                render
            },
            model: config.model.clone(),
            mode: config.permission_mode,
            // A session that **starts** in full access owes the warning too
            // (`initializeYoloWarning`, `app_permission_runtime.zig:86-88`).
            warning: {
                let mut warning = permissions::Warning::default();
                if config.permission_mode == PermissionMode::Yolo && !config.yolo_acknowledged {
                    warning.arm();
                }
                warning
            },
            acknowledged: config.yolo_acknowledged,
            missing_credential: !crate::provider::credential_ready(config.provider, config),
            provider: config.provider,
            catalog: Vec::new(),
            catalog_known: CatalogKnown::Unasked,
            effort: config.effort.clone(),
            column: Column::default(),
            column_seen: String::new(),
            column_edit: ColumnEdit::Other,
            context_used: None,
            editor: Editor::new(),
            paste: Paste::default(),
            edit_history: EditHistory::new(),
            history: History::new(),
            decoder: Decoder::new(),
            // Wrapped to the screen the band was solved for: the document rows
            // and the band rows share a terminal, and a transcript measured
            // against a different width would wrap where the screen does not.
            transcript: Transcript::new(geometry.cols),
            pacer: Pacer::new(),
            marks: VecDeque::new(),
            enqueued: 0,
            emitted: 0,
            work,
            gestures: Gestures::default(),
            notice: None,
            escape_armed: false,
            exit_armed: false,
            interrupt_armed: false,
            cancelling: false,
            cancel_owed: None,
            cancel_covers: 0,
            retry: None,
            queued: 0,
            activity: Activity::new(),
            phase: 0,
            panel: None,
            alternate: None,
            asked_id: None,
            readiness: Readiness::default(),
            ask: None,
            owner: ScreenOwner::Primary,
            picker: None,
            dismissed: Dismissed::default(),
            help_menu: None,
            sessions: None,
            wipe: false,
            model_menu: None,
            gateway_note: crate::provider::model::gateway_catalog_note(config.credential.as_ref()),
            statusline: config.statusline,
            statusline_menu: None,
            workspace_identity: {
                // Read now only when it is on: a session that never shows the
                // branch has no reason to open a file for it.
                let mut identity = statusline::Workspace::new(&config.workspace_root);
                if config.statusline.workspace {
                    identity.refresh();
                }
                identity
            },
            settings_path: config.user_settings_path.clone(),
            provider_picker: None,
            provider_dismissed: false,
            activity_row: None,
            handoff: None,
            tail_unknown: false,
            draining: None,
            markdown: MarkdownProcessor::with_styles(markdown::ansi::Styles::new(
                markdown::theme::builtin(palette.mode == Mode::Light),
            )),
            answer: Answer::default(),
            group: ToolGroup::default(),
            abandoned: Vec::new(),
            trailer: Vec::new(),
            tally: Tally::default(),
            prompts: VecDeque::new(),
            session_title: None,
            turn_title: None,
            title_owed: true,
            workspace: config.workspace_root.clone(),
            document_rows: 0,
            fatal: None,
            leaving: None,
            copies: Copies::new(clipboard),
            sound: Sound::new(config.notifications, settings),
            metrics: Metrics::default(),
            full: FullTranscript::default(),
        }
    }

    /// The band's rows, top first, starting at the band's own top row -- the
    /// blank row above the activity row while a turn is running, and the
    /// composer's first row otherwise.
    ///
    /// Exactly as many rows as the band owns, in upstream's footer order
    /// (`paint_plan.zig:660-960`): the writer places them by counting down from
    /// the band's top, so a row missing here would shift every row below it up
    /// by one.
    pub(crate) fn band_rows(&self) -> Vec<String> {
        let mut rows = Vec::with_capacity(usize::from(self.geometry.band_rows()));
        // What the turn is doing, with a blank row under it, and only while the
        // geometry says the band owns those rows: the row's presence and its
        // text are one fact settled together ([`Self::tick_activity`]), and a
        // band that painted rows the geometry did not give it would push its
        // hint row off the bottom of the screen. The row carries its own
        // colours and closes them ([`super::activity::Activity::row`]), so
        // nothing of it leaks onto the composer.
        if self.geometry.activity.is_some() {
            let padding = usize::from(self.geometry.activity_padding);
            rows.push(self.activity_row.clone().unwrap_or_default());
            // A waiting prompt's rows, between the turn's blank row and a
            // blank row of their own -- when the band was solved with them
            // ([`layout::solve_band_with_banner`]), which is the only way the
            // padding is that tall.
            let banner = self.banner_rows(self.geometry.cols);
            if !banner.is_empty() && padding == banner.len() + 2 {
                rows.push(String::new());
                for row in banner {
                    rows.push(self.painted(self.palette.dim(), row));
                }
                rows.push(String::new());
            } else {
                rows.extend(std::iter::repeat_n(String::new(), padding));
            }
        }
        // The composer's own rows, as many of them as the window shows, each
        // behind upstream's rail: `┃` in the hint colour and a space, or `┃↑`
        // on the first row shown when rows above it are scrolled out of view
        // (`input_presentation.zig:977-992`).
        //
        // **None while a question is up** -- a permission question or the
        // model's own batch: the question replaces the composer, as upstream's
        // replaces its footer ([`layout::fits_approval`]), and the draft waits
        // in the editor, unpainted, for the answer to give the band back.
        let composer = self.editor.rows(self.text_cols());
        let window = if self.geometry.input_rows() == 0 {
            0..0
        } else {
            self.window(composer.len())
        };
        let first_shown = window.start;
        let mut shown = 0;
        for index in window {
            let marker = if index == first_shown && first_shown > 0 {
                RAIL_HIDDEN_ABOVE
            } else {
                RAIL
            };
            let gap = if marker == RAIL { " " } else { "" };
            rows.push(format!(
                "{}{marker}{}{gap}{}",
                self.palette.hint(),
                self.palette.reset(),
                composer[index]
            ));
            shown += 1;
        }
        // A composer shorter than the rows the geometry gave it. Those rows are
        // the band's, so they are written -- blank -- rather than left out: a
        // row the frame does not place is a row the last frame's text stays on.
        while shown < usize::from(self.geometry.input_rows()) {
            rows.push(String::new());
            shown += 1;
        }
        // The question -- or, when there is no question, the completion menu --
        // between two rules under the composer, in the rows the geometry gave
        // the slot and only while it gave them: the block's height and the
        // band's are one fact settled together ([`Self::refit`]). The order is
        // the one [`Self::fit`] solved with, and it is the whole of "they are
        // mutually exclusive": a question outranks a menu. With no panel the
        // bottom rule is a blank row (`paint_plan.zig:937-941`).
        if self.geometry.panel > 0 {
            rows.push(self.rule());
            let mut panel = match self.slot() {
                Some(Slot::Question(panel)) => {
                    panel.painted(self.geometry.cols, self.geometry.rows, &self.palette)
                }
                Some(Slot::Ask(ask)) => {
                    ask.painted(self.geometry.cols, self.geometry.rows, &self.palette)
                }
                // In the rows the geometry gave it, which are the rows its own
                // height asked for: one layout answers both
                // (`composeHelpMenuRow` is handed `menuRowCount`'s answer).
                Some(Slot::Catalog(menu)) => {
                    menu.rows(self.geometry.cols, self.geometry.panel, self.palette)
                }
                Some(Slot::Sessions(sessions)) => {
                    sessions.rows(self.geometry.cols, self.geometry.panel, self.palette)
                }
                // The same: one layout answers its height and its rows
                // (`composeModelMenuRow` is handed `menuRowCount`'s answer).
                Some(Slot::Models(menu)) => {
                    menu.rows(self.geometry.cols, self.geometry.panel, self.palette)
                }
                // In the rows the geometry gave it, with the values in force.
                Some(Slot::Statusline(menu)) => menu.rows(
                    self.geometry.cols,
                    self.geometry.panel,
                    self.statusline,
                    self.palette,
                ),
                Some(Slot::Menu(picker)) => {
                    picker.rows(self.geometry.cols, self.geometry.rows, self.palette)
                }
                Some(Slot::Column) => self.column_rows(),
                // In the rows the geometry gave it, under the argument it is
                // the column of.
                Some(Slot::Provider(column)) => column.rows(
                    self.geometry.cols,
                    self.provider_anchor(column),
                    self.geometry.panel,
                    self.palette,
                ),
                None => Vec::new(),
            };
            // Exactly the panel's rows: one that answered with fewer is padded
            // blank, and the bottom rule stays on the row the geometry put it.
            panel.resize(usize::from(self.geometry.panel), String::new());
            rows.extend(panel);
            rows.push(self.rule());
        } else {
            rows.push(String::new());
        }
        // The hint row: what a turn will be run with, in upstream's order and
        // budgeted to the screen ([`super::hint`]) -- or, while the menu is
        // up, the keys it binds; or, while a permission question is up, what
        // its keys do.
        rows.push(self.hint_or_key_help());
        rows
    }

    /// The band's last row, painted.
    ///
    /// **Replaced, not annotated, while the menu is up**: upstream paints the
    /// slash menu's key help in that row instead of the hint, in the dim grey
    /// rather than the statusline's (`paint_plan.zig:970-971`), and with no
    /// notice or armed gesture beside it. A menu does not get this row while a
    /// question is up -- the slot is the question's then ([`Self::slot`]), and
    /// so is the row: what the question's keys do (`approval_ui.zig:20-25`).
    /// A notice the question owes the user still wins the row for as long as
    /// it stands, because it is about the keystroke just pressed.
    fn hint_or_key_help(&self) -> String {
        match self.slot() {
            Some(Slot::Question(panel)) if self.notice.is_none() => {
                panel.hint(self.geometry.cols, &self.palette)
            }
            // The model's question batch owns the row the same way: its keys,
            // which question of the batch this is, and -- while an Escape at
            // its draft has armed the clear -- the warning that says so
            // (`questionInteractionHint`, `input_presentation.zig:192-249`).
            Some(Slot::Ask(ask)) if self.notice.is_none() => {
                ask.hint(self.geometry.cols, self.escape_armed, &self.palette)
            }
            Some(Slot::Menu(_)) => self.painted(
                self.palette.dim(),
                picker::key_help(self.geometry.cols).to_string(),
            ),
            // The catalog's own key help, in the same dim grey
            // (`composeHelpMenuHintRow`, `input_presentation.zig:696-721`) --
            // except while a Ctrl-C would leave, which the catalog says in the
            // statusline grey instead of its keys (`:697-703`): the catalog
            // stays up under a first Ctrl-C, and the row is the only place the
            // second one is announced.
            Some(Slot::Catalog(_)) if self.exit_armed => {
                self.painted(self.palette.statusline(), EXIT_ARMED.to_string())
            }
            Some(Slot::Catalog(_)) => self.painted(
                self.palette.dim(),
                help_menu::key_help(self.geometry.cols).to_string(),
            ),
            // The picker's, in the same grey (`input_presentation.zig:834-840`).
            Some(Slot::Sessions(_)) => self.painted(
                self.palette.dim(),
                session_picker::key_help(self.geometry.cols).to_string(),
            ),
            // `/model`'s own, `tab provider` where the catalog's says
            // `tab category` (`composeCatalogMenuHintRow`,
            // `input_presentation.zig:810-857`).
            Some(Slot::Models(_)) => self.painted(
                self.palette.dim(),
                model_menu::key_help(self.geometry.cols).to_string(),
            ),
            // And the `/statusline` picker's, in the same grey
            // (`composeCompactCommandMenuHintRow`, `input_presentation.zig:766-790`).
            Some(Slot::Statusline(_)) => self.painted(
                self.palette.dim(),
                statusline::key_help(self.geometry.cols).to_string(),
            ),
            _ => self.painted(self.palette.statusline(), self.hint_row()),
        }
    }

    /// A rule across the whole screen, in the divider's colour.
    fn rule(&self) -> String {
        self.painted(
            self.palette.divider(),
            std::iter::repeat_n(RULE, usize::from(self.geometry.cols)).collect(),
        )
    }

    /// `text` in one of the palette's colours, ended.
    ///
    /// **Clipped here, before the colour is wrapped around it**, and that is
    /// the whole of why this function exists rather than a `format!` at each
    /// call site. The painter clips a row that overruns the screen
    /// (`super::frame`'s `row_text`) by stopping at the first cell that will
    /// not fit -- and everything after that cell goes, the closing reset
    /// included. A row that lost its reset leaves the colour open on a terminal
    /// whose next rows are the *user's document*, so the hint row overflowing
    /// on a narrow screen would tint the shell that outlives xfx. Cutting the
    /// text first, by [`super::frame::clip`] -- the painter's own rule, so the
    /// two cannot disagree about where a row ends -- puts the reset inside the
    /// budget by construction: an escape sequence costs no cells, so it is
    /// never what the clip drops.
    fn painted(&self, colour: &'static str, text: String) -> String {
        // A row with nothing on it is painted in no colour: there is nothing
        // for the attribute to apply to, and eight bytes of it on every frame
        // of an idle band is eight bytes that say nothing.
        if colour.is_empty() || text.is_empty() {
            return text;
        }
        format!(
            "{colour}{}{}",
            super::frame::clip(&text, self.geometry.cols),
            self.palette.reset()
        )
    }

    /// What the band's last row says.
    ///
    /// **Budgeted rather than clipped**, and by [`super::hint`] rather than
    /// here: that module owns the segments, the order they are said in and the
    /// order they give way in on a screen too narrow for all of them. What is
    /// this method's is the half of the row the shell knows and the hint does
    /// not -- which facts are true right now, and what colour each one is
    /// painted in.
    ///
    /// The notice is the colour: a refusal is painted in its own role *inside*
    /// the row's, and the row's colour is put back after it. The two halves are
    /// handed over **separately** ([`Notice`]) rather than wrapped around the
    /// text here, and that is the difference between a rule and a hope: the
    /// budget cuts the text of a notice too wide for its side, and a closing
    /// sequence written behind that text would be cut with it -- leaving the
    /// warning to the right of the row painted in the refusal's colour. The
    /// opening half travels with the text, because a colour costs no columns
    /// ([`super::wrap::width`], which is what the budget is measured with) and
    /// a cut that left nothing for it to apply to leaves nothing to look at.
    fn hint_row(&self) -> String {
        // A Ctrl-C that would leave outranks everything the row says, a
        // refusal included (`composeHintRow`, `input_presentation.zig:501`):
        // the refusal was about a keystroke, and this is about the next one.
        if self.exit_armed {
            return EXIT_ARMED.to_string();
        }
        let session_title = self
            .session_title
            .as_deref()
            .map(statusline::session_label)
            .filter(|title| !title.is_empty());
        let notice = self.notice.map(|text| Notice {
            text,
            style: self.palette.notice(),
            resume: self.palette.statusline(),
        });
        // The right-hand slot is upstream's `right_text`: the Escape cues in
        // the dim grey after the row's own run is closed
        // (`input_presentation.zig:559-567`) -- the same number as the row's
        // in the dark palette and not in the light one (`theme.zig:123,128`)
        // -- and, when neither is armed, the full-access warning in
        // `red_style` while it has time on the screen left
        // ([`Self::warning_shown`]). An armed cue outranks the warning
        // (`dangerStatusText` is empty while one is, `:580`).
        let cue_style = format!("{}{}", self.palette.reset(), self.palette.dim());
        let (model, effort) = self.hinted_model();
        let mut hint = Hint {
            missing_credential: self.missing_credential,
            queued: self.queued,
            mode: self.mode,
            model: &model,
            effort: effort.as_ref().map(Effort::display),
            // The three optional segments, each only while it is on
            // (`buildStatuslineItems`, `app_render_runtime.zig:875-899`).
            session_title: self
                .statusline
                .session
                .then_some(session_title.as_deref())
                .flatten(),
            context_used: self
                .statusline
                .context
                .then(|| self.context_meter())
                .flatten(),
            workspace: self.statusline.workspace.then(|| {
                (
                    self.workspace_identity.label(),
                    self.workspace_identity.branch(),
                )
            }),
            notice,
            // The armed half of the double-Escape gesture goes to the
            // right-hand slot: it is a warning about what the *next*
            // keystroke would do rather than another fact about the
            // session, and a warning that moved left and right with the
            // queue's depth would be one the eye has to look for.
            right: self
                .escape_armed
                .then_some(ESCAPE_ARMED)
                .or_else(|| self.warning_shown()),
            right_style: if self.escape_armed {
                cue_style.as_str()
            } else {
                self.palette.danger()
            },
            accent: (self.palette.activity(), self.palette.statusline()),
        };
        let cols = self.geometry.cols;
        if self.interrupt_armed {
            // Upstream's rule rather than this row's own budget: the widest
            // cue that fits beside the **whole** row, and when none does, the
            // cue owns the row (`input_presentation.zig:510-525`). The row
            // still shrinks first for a cue that does fit, by `hint::row`'s
            // own rule, but a narrow screen never loses the warning.
            hint.right = None;
            let left = usize::from(super::wrap::width(&hint::row(&hint, cols)));
            let fits = INTERRUPT_ARMED
                .into_iter()
                .find(|cue| usize::from(cols) > left + usize::from(super::wrap::width(cue)));
            match fits {
                Some(cue) => {
                    hint.right = Some(cue);
                    hint.right_style = cue_style.as_str();
                }
                None => return INTERRUPT_ARMED_ALONE.to_string(),
            }
        }
        hint::row(&hint, cols)
    }

    /// The model and the effort the hint row says a turn will use.
    ///
    /// **While the inline column has a model pending, the pending one**, and
    /// the level the text spells or the mark is on: upstream's footer previews
    /// the choice before it is made (`app_render_runtime.zig:653-668`;
    /// `fx-model-40x120-gpt-enter`, `effort-enter`). Otherwise the model in
    /// force and the session's effort.
    ///
    /// The effort is shown only when it is not the default and the model takes
    /// it (`render.zig:390-393`): its catalog row lists that level -- or no
    /// catalog has been read yet, which upstream treats as a level it has not
    /// been told is unsupported (`active_capabilities_pending`, `:655-658`). A
    /// catalog that could not be read lists nothing, so nothing is shown.
    fn hinted_model(&self) -> (String, Option<Effort>) {
        let catalog = known(self.catalog_known, &self.catalog);
        if let Some(preview) = self
            .column
            .active(self.editor.text())
            .filter(|_| self.help_menu.is_none() && self.model_menu.is_none())
            .and_then(|query| self.column.preview(&query, catalog))
        {
            let (model, effort) = preview;
            return (model, (!effort.is_default()).then_some(effort));
        }
        let shown = match self.catalog_known {
            CatalogKnown::Loaded => self
                .catalog
                .iter()
                .find(|entry| entry.matches(&self.model))
                .is_some_and(|entry| self.effort.supported_by(entry) && !entry.efforts.is_empty()),
            CatalogKnown::Unread(_) => false,
            CatalogKnown::Unasked | CatalogKnown::Asked | CatalogKnown::Wanted => true,
        };
        let effort = (shown && !self.effort.is_default()).then(|| self.effort.clone());
        (self.model.clone(), effort)
    }

    /// The context meter's numerator and, when there is one, its denominator.
    ///
    /// **No numerator, no meter.** The numerator is a completed turn's
    /// `input_tokens`, and a provider need not publish usage: a missing one
    /// removes the segment rather than defaulting it, because a row that
    /// filled it in with a zero would report a measurement nobody took -- the
    /// rule the activity row's token count follows
    /// ([`super::activity::Activity::tokens`]). The denominator is the
    /// catalog's `max_context` for the model in force, and there may be none --
    /// no catalog read yet, or a row that publishes no window; then the
    /// numerator stands alone, as upstream's does (`12k`, `render.zig:448-451`), which says
    /// exactly what was measured and nothing that was not.
    ///
    /// Whether the meter is **shown** is not this method's question: it is
    /// off until `/statusline context` turns it on ([`Self::hint_row`]).
    fn context_meter(&self) -> Option<(u64, Option<u64>)> {
        // Nothing until a turn has measured something, and nothing for a
        // measurement of nought either (`app_render_runtime.zig:889-892`
        // hands the row upstream's total; `render.zig:441` draws it only past
        // zero).
        let used = self.context_used.filter(|used| *used > 0)?;
        // Matched by id **or alias**, because the model in force is whatever the
        // operator or the profile spelled and the catalog publishes both
        // ([`CatalogEntry::matches`]).
        let total = self
            .catalog
            .iter()
            .find(|entry| entry.matches(&self.model))
            .and_then(|entry| entry.max_context);
        Some((used, total))
    }

    /// What is in the band's elastic slot, if anything.
    ///
    /// **One place answers it**, because the two readers -- the height
    /// [`Self::fit`] solves for and the rows [`Self::band_rows`] paints -- are
    /// the pair whose disagreement leaves a stale row standing in the band.
    ///
    /// The two occupants are mutually exclusive by construction: [`Self::ask`]
    /// dismisses a menu before it installs a question, and while a question is
    /// up every keystroke goes to it ([`Self::consume`]), so nothing can call
    /// [`Self::edited`] and open one behind it. The order below is therefore a
    /// statement of which would win rather than a case the session reaches --
    /// and it is stated once rather than twice, so it cannot be answered two
    /// ways.
    fn slot(&self) -> Option<Slot<'_>> {
        // **A turn asks one thing at a time.** A permission question and a
        // model's question batch both take the focus and both take the slot, so
        // two at once would be one of them painted and the other holding a turn
        // open behind it, unanswerable. It cannot be reached today -- a tool
        // call is either the question tool or a mutation waiting on an approval,
        // and both park the runtime thread until they are answered -- so this
        // states the hazard where the next edit reads it rather than adding a
        // branch no test can drive honestly, exactly as [`Self::ask`]'s
        // one-surface assertion does.
        debug_assert!(
            !(self.panel.is_some() && self.ask.is_some()),
            "a permission question and a model's question are both up"
        );
        if let Some(panel) = self.panel.as_ref() {
            return Some(Slot::Question(panel));
        }
        if let Some(ask) = self.ask.as_ref() {
            return Some(Slot::Ask(ask));
        }
        if let Some(menu) = self.statusline_menu.as_ref() {
            return Some(Slot::Statusline(menu));
        }
        if let Some(menu) = self.help_menu.as_ref() {
            return Some(Slot::Catalog(menu));
        }
        if let Some(sessions) = self.sessions.as_ref() {
            return Some(Slot::Sessions(sessions));
        }
        if let Some(menu) = self.model_menu.as_ref() {
            return Some(Slot::Models(menu));
        }
        if let Some(column) = self.provider_picker.as_ref() {
            return Some(Slot::Provider(column));
        }
        if self.column.active(self.editor.text()).is_some() {
            return Some(Slot::Column);
        }
        self.picker.as_ref().map(Slot::Menu)
    }

    /// The cell the `/provider` column is anchored at: under the first byte of
    /// its argument, on the composer row that byte is on
    /// (`projectedAnchorColumn`, `input_presentation.zig:368-370`).
    ///
    /// Measured by the composer's own wrap, so an argument that a wrapped
    /// draft put at the start of its second row anchors at that row's start
    /// rather than past the edge of the screen.
    fn provider_anchor(&self, column: &ProviderPicker) -> u16 {
        let text = self.editor.text();
        // The column is rebuilt on every edit, so its offset is into this very
        // text and after an ASCII prefix; checked anyway, because
        // `cursor_point` panics on an offset that is not a `char` boundary.
        let start = column.token_start();
        if !text.is_char_boundary(start) {
            return PROMPT_CELLS;
        }
        let rows = super::wrap::wrap(text, self.text_cols());
        let (_, at) = super::wrap::cursor_point(text, &rows, start);
        PROMPT_CELLS.saturating_add(at)
    }

    /// Where the caret goes: the terminal's own row, and the number of cells to
    /// the left of it on that row.
    pub(crate) fn cursor(&self) -> (u16, u16) {
        // While a question is up the panel has the focus, so the caret sits on
        // the choice Enter would take. A caret left blinking in the composer
        // would say the next keystroke goes there, and it does not.
        //
        // **A completion menu is the opposite case and takes no branch here**:
        // it is a view of what the composer holds, the typing goes on going
        // into the composer, and the caret says so. The rows it takes move the
        // composer down, and the composer's own row is read out of the geometry
        // below -- so the caret follows the menu's height without knowing the
        // menu exists.
        if self.geometry.panel > 0 {
            if let Some(panel) = self.panel.as_ref() {
                // **Two places the caret can be**, exactly as a model's
                // question has: on the marked choice, where a digit takes one,
                // and inside an amendment draft, where a digit is a character
                // ([`super::approval_amendment`]). One call answers both, from
                // the one walk that placed the rows.
                let (row, column) = panel.caret(self.geometry.cols, self.geometry.rows);
                return (self.geometry.panel_first().saturating_add(row), column);
            }
            // A model's question has **two** places the caret can be, and they
            // are two different claims about the next keystroke: on the marked
            // choice, where a digit takes one, and inside the freeform draft,
            // where a digit is a character ([`super::question::QuestionPanel`]).
            if let Some(ask) = self.ask.as_ref() {
                let (cols, rows) = (self.geometry.cols, self.geometry.rows);
                let (row, column) = ask
                    .caret(cols, rows)
                    .unwrap_or_else(|| (ask.caret_row(cols, rows), 0));
                return (self.geometry.panel_first().saturating_add(row), column);
            }
            // The `/statusline` picker has the focus too, and no composer row
            // to leave a caret in: it goes to the start of the marked row,
            // which is what Enter and the arrows act on.
            if let Some(menu) = self.statusline_menu.as_ref() {
                let row = menu.marked_row(self.geometry.panel);
                return (self.geometry.panel_first().saturating_add(row), 0);
            }
        }
        let (row, column) = self.editor.point(self.text_cols());
        let window = self.window(self.editor.rows(self.text_cols()).len());
        // Here, and nowhere before it, is where a row becomes a **terminal
        // coordinate**: the composer's rows are counted in `usize` all the way
        // down (`Editor::point`), because a draft can have more rows than a
        // `u16` can name, and a count saturated earlier would leave the window
        // following a row the caret is not on. The clamp comes first and the
        // narrowing second, so the conversion cannot fail and its answer, if it
        // ever could, is the band's own last composer row rather than a row
        // outside the band -- the shape `transcript`'s `shown` uses for the
        // same reason.
        let last = self.geometry.input_rows().saturating_sub(1);
        let offset =
            u16::try_from(row.saturating_sub(window.start).min(usize::from(last))).unwrap_or(last);
        (
            self.geometry.input_first.saturating_add(offset),
            PROMPT_CELLS.saturating_add(column),
        )
    }

    /// How wide the composer's *text* is: the screen without the gutter.
    fn text_cols(&self) -> u16 {
        self.geometry.cols.saturating_sub(PROMPT_CELLS).max(1)
    }

    /// Which of a `rows`-row composer's rows the band is showing.
    fn window(&self, rows: usize) -> std::ops::Range<usize> {
        editor::window(
            rows,
            self.editor.point(self.text_cols()).0,
            self.geometry.input_rows(),
        )
    }

    /// Adds text of no particular kind to the transcript, for the cases that
    /// are about **when** a row lands rather than about what it is.
    ///
    /// Test-only, and it is the shape this call had before a row's text came
    /// with a meaning: every production write says what it is writing
    /// ([`Self::write_transcript_as`]), because the colour of the row is that
    /// answer and nothing else. What the cases reached through here get is the
    /// plain row they always got.
    #[cfg(test)]
    pub(crate) fn write_transcript(&mut self, text: &str) {
        self.write_transcript_as(text, Role::Plain);
    }

    /// Adds text to the transcript, with what it is.
    ///
    /// Nothing is written here. What the text costs the terminal is queued and
    /// a frame is asked for, because the append and the frame that follows it
    /// are one turn's worth of work and the loop is the only thing that owns
    /// the screen.
    ///
    /// No colour is chosen here either: the role travels with the text and the
    /// sequence is made when the row is handed to the terminal, in whatever
    /// palette the session is in by then ([`Transcript::emit_front`]).
    // The composer's submit is the first caller -- it echoes the line the user
    // sent so the loop is visibly closed -- and Task 12's deltas are the next.
    fn write_transcript_as(&mut self, text: &str, role: Role) {
        self.close_group();
        if self.transcript.queue_push(text, role) {
            self.owed();
        }
    }

    /// Writes upstream's welcome line as the session's first document row, with
    /// a blank row under it (`render.zig:181-193`, `c1db919`).
    pub(crate) fn welcome(&mut self) {
        self.full.welcome();
        let line = welcome_line(self.palette);
        self.write_transcript_as(&line, Role::Plain);
        self.end_transcript_line();
        self.end_transcript_line();
    }

    /// The full transcript's record of the session a fresh launch assembled,
    /// as upstream writes one for a fresh session and none for a resumed one
    /// (`writeSessionAssemblyNotice`, `app_bootstrap_runtime.zig:640-642`).
    /// The document is not given it (`visibility = .full_only`).
    pub(crate) fn session_assembled(&mut self) {
        self.full.full_detail(full_transcript::session_record(
            self.provider.label(),
            &self.model,
            crate::tools::ADVERTISED_TOOLS,
        ));
    }

    /// Says that this session continues a saved one, as upstream's resume does
    /// (`writeResumeNotice`, `app_session_runtime.zig:4657-4672`): a neutral
    /// notice under `session resumed` whose body is the session's title, and
    /// the title is the window's from here on, as a title the first turn had
    /// given it would be.
    ///
    /// `title` is the store's title of the session -- a `/rename`, or its
    /// first prompt -- read off a log on disk, so it is made inert before it is
    /// written anywhere. A session with none is called what upstream calls
    /// one, and the window keeps the workspace's fallback.
    ///
    /// The first line of [`Self::hydrate`], which writes the turns under it.
    fn resumed(&mut self, title: Option<String>) {
        let title = title
            .map(|title| super::bridge::inert(&title).trim().to_string())
            .filter(|title| !title.is_empty());
        let body = title
            .clone()
            .unwrap_or_else(|| UNTITLED_SESSION.to_string());
        self.announce(notice::Notice::neutral("session resumed", body));
        if title.is_some() {
            self.session_title = title;
            self.retitle();
        }
    }

    /// Replays a saved session onto the screen, as upstream's resume does
    /// (`hydrateResumedSession`, `app_session_runtime.zig:2368-2470`): the
    /// resume notice, then every turn as the live one was shown -- the card,
    /// the answer through the presenter, the tool group, the summary row.
    ///
    /// `wipe` is the picker's case: the screen **and its scrollback** are reset
    /// first (`finishLiveSessionResume` → `requestTerminalReset`,
    /// `app_session_runtime.zig:1926-1929`), so nothing of the session that
    /// was is left above the one that is. A launch's resume does not reset
    /// anything (`commitStartupResumeReplayAnchor`, and
    /// `tui-resume.test.ts:330-400`, which keeps every row the shell printed
    /// before `fx --resume`): the launch has already pushed the shell's output
    /// into scrollback, and the replay starts on the screen it left. Neither
    /// writes the welcome line (`app_bootstrap_runtime.zig:619`, written only
    /// when nothing is being resumed).
    ///
    /// Written straight into the transcript rather than through the pacer: a
    /// conversation that already happened is not streamed again.
    pub(crate) fn hydrate(&mut self, replay: Hydration, wipe: bool) {
        // Whatever the screen still owed the conversation being replaced: a
        // tail the pacer holds, the lines behind it, a presenter's open block,
        // a tool group. None of it belongs above the session that replaces it.
        self.pacer.forget();
        self.marks.clear();
        self.emitted = self.enqueued;
        self.markdown.reset();
        self.answer = Answer::default();
        self.group = ToolGroup::default();
        self.trailer.clear();
        self.abandoned.clear();
        self.prompts.clear();
        self.tally = Tally::default();
        self.context_used = None;
        self.forget_session_title();
        if wipe {
            // The rows a recolour or a handoff was owed against are about to
            // be erased, scrollback and all.
            self.retint_debt = false;
            self.handoff = None;
            self.wipe = true;
            self.transcript = Transcript::new(self.geometry.cols);
            // And the full transcript is the document's record: the session
            // it recorded is not on this screen any more, the replayed one is
            // -- recorded below by the same writes that put it on the screen
            // ([`Self::record`], [`Self::write_answer`], [`Self::write_notice`]).
            self.close_full_transcript();
            self.full = FullTranscript::default();
        }
        if let Some(model) = replay.model {
            self.model = model;
        }
        self.resumed(replay.title);
        for turn in replay.turns {
            self.replay_turn(turn);
        }
        self.render.request(Reason::Footer);
    }

    /// One saved turn, through what each live event reaches
    /// ([`Self::apply`]): the card, the presenter, the tool group's marks, the
    /// amendment row, the conclusion.
    ///
    /// The full transcript is filled by the same writes, and given what the
    /// screen never shows -- each call's arguments and whole result -- before
    /// them, to wait for their rows ([`full_transcript::details_of`]). Its
    /// entries carry the log's times, as upstream's replay stamps them
    /// (`setCreatedAtMs`, `app_session_runtime.zig:4753-4810`): the turn's
    /// start for the card and what it did on the way, its conclusion for the
    /// answer after its last call and the summary.
    fn replay_turn(&mut self, turn: HydratedTurn) {
        for (call_id, detail) in turn.details {
            // Made inert here as the channel would have: a launch's replay is
            // handed over without crossing it.
            self.full.observe(&UiEvent::ToolDetail {
                call_id,
                detail: detail.made_inert(),
            });
        }
        // A turn the log has no conclusion for keeps its start throughout.
        let concluded = turn.concluded_at_ms.unwrap_or(turn.started_at_ms);
        self.full.stamp_with(Some(turn.started_at_ms));
        let concluding = turn
            .steps
            .iter()
            .rposition(|step| !matches!(step, HydratedStep::Text(_)))
            .map_or(0, |last| last + 1);
        self.run_mark(Mark::Card(turn.user.clone()));
        let mut streamed = Estimator::default();
        for (index, step) in turn.steps.into_iter().enumerate() {
            if index == concluding {
                self.full.stamp_with(Some(concluded));
            }
            match step {
                HydratedStep::Text(text) => {
                    streamed.consume(&text);
                    self.write_answer(&text);
                }
                HydratedStep::ToolStart { id, shown } => {
                    self.run_mark(Mark::ToolStart { id, shown });
                }
                HydratedStep::ToolResult {
                    id,
                    ok,
                    detail,
                    outcome,
                } => self.run_mark(Mark::ToolResult {
                    id,
                    ok,
                    detail: safe_one_line(&detail, TOOL_DETAIL_BYTES),
                    outcome,
                    changes: None,
                }),
                HydratedStep::Feedback(text) => self.run_mark(Mark::Line(
                    format!(
                        "{AMENDMENT_PREFIX} {}",
                        safe_one_line(&text, AMENDMENT_BYTES)
                    ),
                    Role::Notice,
                )),
            }
        }
        self.full.stamp_with(Some(concluded));
        self.run_mark(Mark::TurnOver);
        match turn.ending {
            // In is the prompt's estimate, as a live turn's is; out is what
            // the provider said, or the estimate of what it wrote.
            Ending::Concluded { seconds, output } => {
                let mut input = Estimator::default();
                input.consume(&turn.user);
                let output = output.unwrap_or_else(|| streamed.estimate());
                self.run_mark(Mark::Summary(summary_line(
                    seconds,
                    input.estimate(),
                    output,
                )));
            }
            // The sentence the live turn ended on, as its conclusion showed it.
            Ending::Interrupted(reason) => {
                self.run_mark(Mark::Notice(notice::Notice::error(
                    "system",
                    unattributed(&reason),
                )));
            }
            Ending::Unconcluded => self.run_mark(Mark::EndOfLine),
        }
        self.full.stamp_with(None);
    }

    /// Ends the transcript's current line, leaving it in the document.
    // The composer's submit is the first caller, and Task 12's end-of-turn is
    // the next: a turn ends whether or not the last delta carried a newline.
    pub(crate) fn end_transcript_line(&mut self) {
        // An open group's block ends as a line, and its blank last row is the
        // break this would have made.
        if self.close_group() {
            return;
        }
        if self.transcript.queue_end_line() {
            self.owed();
        }
    }

    /// Asks for the frame a document write owes.
    ///
    /// Only for a write that will really put something on the screen, and the
    /// guard is not tidiness: an operation that scrolls nothing and writes no
    /// rows -- ending a line the screen already has -- would still cost the
    /// loop a frame, and a frame the band did not need is a repaint of the
    /// whole band on a link that may be a serial line.
    fn owed(&mut self) {
        // An append scrolls the screen out from under the band, so the frame
        // that follows it is not optional.
        self.render.request(Reason::Transcript);
    }

    /// Whether the **primary** plane is owed anything that has not been
    /// written.
    ///
    /// Asked without taking it, by the one thing that has to know before it
    /// decides what to write: the frame that would hand the terminal to a
    /// question on the other buffer, where a document row cannot be written at
    /// all (`super::event_loop`'s `commit_frame`). Text the pacer is still
    /// holding is deliberately **not** counted -- it is not a row yet, and it
    /// waits for its own release whichever plane the session is on.
    pub(crate) fn owes_document(&self) -> bool {
        self.transcript.owes()
    }

    /// Offers the oldest thing the document is owed to the terminal, measured
    /// at the width the screen has **now**.
    ///
    /// `None` when nothing is owed. The writer is the caller's, because the
    /// loop owns the terminal; what is written is the transcript's, because
    /// only it knows what text has not landed and what the rows it makes are
    /// worth. One operation per call: what the terminal took decides whether
    /// the next one may be offered at all, and that is the caller's question
    /// (`super::event_loop`'s `commit_document`).
    ///
    /// The rows do not outlive the call, which is what keeps them honest --
    /// nothing can resize, clear or repaint between the moment they are
    /// measured and the moment they are written, because this borrows the whole
    /// shell for exactly that long.
    pub(crate) fn emit_document_front<E>(
        &mut self,
        emit: impl FnOnce(&Append) -> Landed<E>,
    ) -> Option<Result<(), E>> {
        self.transcript.emit_front(self.palette, emit)
    }

    /// Shows what the runtime just did.
    ///
    /// Exhaustive on purpose: a [`UiEvent`] added later has to be given a home
    /// here rather than falling into a wildcard that drops it -- and a dropped
    /// event is a tool that ran invisibly or a turn that ended without saying
    /// so.
    pub(crate) fn apply(&mut self, event: UiEvent) {
        // The old conversation's events, after a reset handed its screen to
        // scrollback and before the runtime has said it is gone.
        if self.draining.is_some() && self.drained(&event) {
            return;
        }
        self.full_transcript_sees(&event);
        match event {
            // A turn is running, which is what the row above the divider is
            // about. The clock is not read here: the *label* is this event's
            // and the *moment* is the next settle's, so the row is timed by the
            // same clock every other row of the band is settled against
            // ([`Self::tick_activity`]).
            UiEvent::TurnStarted => {
                self.retry = None;
                // A turn the user already stopped can still say it started --
                // the runtime took it before it read the cancellation -- and
                // its row must not come back under the line that says so. Any
                // other turn is a new one, and what it says is painted: a
                // cancel that landed after its turn had already ended is
                // answered by no conclusion, and must not silence this one.
                if self.cancel_covers > 0 {
                    self.cancel_covers -= 1;
                } else {
                    self.cancelling = false;
                    self.cancel_owed = None;
                }
                if !self.cancelling {
                    self.activity.set(Work::Thinking);
                }
                let (input, title, card) = self.prompts.pop_front().unwrap_or_default();
                self.tally.begin(Instant::now(), input);
                self.turn_title = title;
                // The waiting prompt is delivered: its card goes into the
                // document behind whatever the last turn still has to say,
                // and its rows leave the band.
                if let Some(card) = card {
                    if !self.cancelling {
                        self.echo(&card);
                    }
                    self.refit();
                    self.render.request(Reason::Footer);
                }
            }
            // The answer, as it arrives -- into the pacer rather than into the
            // document, so a provider that sends a kilobyte in one frame and
            // nothing in the next is still read at one speed.
            //
            // **Not after a cancel**: what is still crossing the channel when
            // the user stopped the turn is the tail of the answer they stopped,
            // and the line saying so is already under what they read
            // ([`Self::cancel_turn`]).
            UiEvent::Delta(text) => {
                if !self.cancelling {
                    // The attempt got through: the answer is the evidence.
                    self.retry = None;
                    self.stream(&text);
                }
            }
            // A request failed and the turn will try it again. Not after a
            // cancel: the turn that is retrying is the one the user stopped.
            UiEvent::Retrying {
                cause,
                diagnostic,
                delay,
            } => {
                if !self.cancelling {
                    self.retry = Some(super::retry::Standing {
                        cause,
                        diagnostic,
                        deadline: Instant::now() + delay,
                    });
                    self.render.request(Reason::Footer);
                }
            }
            // A row in the transcript's tool group (`super::toolgroup`),
            // behind whatever of the answer the pacer still holds -- the call
            // happened after that text, so its row goes under it.
            UiEvent::ToolStart {
                call_id,
                tool,
                shown,
            } => {
                // And on the band, where the activity row stops saying
                // `Thinking` and says `Running`, as upstream's phase does: a
                // turn that has gone quiet because a tool is taking a minute
                // looks exactly like a turn that has gone quiet, unless it says
                // so.
                if !self.cancelling {
                    self.activity.set(Work::Tool { name: tool });
                }
                self.retry = None;
                self.mark(Mark::ToolStart { id: call_id, shown });
            }
            UiEvent::ToolResult {
                call_id,
                ok,
                detail,
                outcome,
                changes,
                ..
            } => {
                // The tool is over, so the model has the turn again. The
                // clock is not restarted with it: what the row measures is the
                // turn, and a tool call is part of one.
                if !self.cancelling {
                    self.activity.set(Work::Thinking);
                }
                self.mark(Mark::ToolResult {
                    id: call_id,
                    ok,
                    detail: safe_one_line(&detail, TOOL_DETAIL_BYTES),
                    outcome,
                    changes,
                });
            }
            // What the user said about a decision, once the runtime has
            // really delivered it (`crate::agent::machine`'s
            // `flush_amendments`). A row rather than a hint, because it is part
            // of the conversation rather than a fact about this keystroke --
            // and it goes through [`Self::say`] like every other thing this
            // session writes, so it takes its place in the stream behind
            // whatever the pacer is still holding rather than overtaking the
            // answer it belongs after.
            //
            // **Flattened here, and by the same function the tool detail two
            // arms up uses.** `super::bridge::send_ui` already made every
            // sequence the terminal would obey a space, but it deliberately
            // keeps `\n`: a streamed answer's line breaks are rows the
            // transcript exists to make. An amendment is not a stream, it is
            // one sentence about one decision, and a draft can hold newlines
            // (`C-j` at the panel, or a pasted paragraph) -- so the row it
            // becomes is made one row here rather than left to the sender.
            UiEvent::ToolFeedback { text, .. } => {
                self.say(format!(
                    "{AMENDMENT_PREFIX} {}",
                    safe_one_line(&text, AMENDMENT_BYTES)
                ));
            }
            UiEvent::Notice(notice) => self.announce(notice),
            // The full transcript's alone, and taken above
            // ([`Self::full_transcript_sees`]): nothing of it is written into
            // the document.
            UiEvent::ToolDetail { .. } | UiEvent::FullDetail(_) => {}
            // The switch is **done** by the time this arrives: the file is
            // written, the configuration has been re-read from it, and the
            // runtime has swapped. So every field here is adopted rather than
            // predicted, which is the difference between what the session will
            // do and what the write intended.
            UiEvent::ProviderSelected {
                provider,
                model,
                missing_credential,
            } => {
                self.provider = provider;
                self.model = model;
                self.missing_credential = missing_credential;
                // The old provider's catalog is not this one's, and the
                // conversation the meter was counting was dropped with the
                // bundle. Keeping either would put another daemon's window --
                // or a dead conversation's tokens -- on the hint row.
                self.catalog.clear();
                self.catalog_known = CatalogKnown::Unasked;
                self.column.reset();
                self.context_used = None;
                // The conversation went with the bundle, and the session its
                // title named with it.
                self.forget_session_title();
                // Upstream's sentence for a finished switch
                // (`app_auth_runtime.zig:1383-1386`, topic `provider` at
                // `:1410-1414`), with xfx's one more fact: the runtime dropped
                // the conversation with the bundle, so the next prompt is the
                // first of a new one.
                self.announce(notice::Notice::neutral(
                    "provider",
                    format!(
                        "Switched to {} with {}. The next prompt starts a fresh conversation.",
                        provider.entry().name,
                        self.model
                    ),
                ));
                // A column standing open was built against the old provider's
                // `current`.
                if let Some(text) = self
                    .provider_picker
                    .as_ref()
                    .map(|open| open.text().to_string())
                {
                    self.provider_picker =
                        ProviderPicker::open(&text, (!missing_credential).then_some(provider));
                }
                self.render.request(Reason::Footer);
            }
            // What the selector made of a `/model` change, and the model and
            // the effort in force afterwards. **Taken rather than predicted**
            // for a query this side could not resolve: it was resolved
            // against the catalog on the runtime thread, and a band that
            // adopted the query it submitted would show `opus` while the next
            // turn was held in the id it resolved to.
            UiEvent::ModelAnswered {
                model,
                effort,
                outcome,
                next_turn,
                announced,
            } => {
                self.model = model;
                self.effort = effort;
                match outcome {
                    // Said already, the moment it was asked for
                    // ([`Self::request_change`]); this only moves the band's
                    // own record to what the runtime applied.
                    ModelAnswer::Applied { .. } | ModelAnswer::Unchanged if announced => {}
                    // Upstream's announcement, said for a model that was
                    // already in force too, as upstream says it. No caveat
                    // follows for an id the catalog does not list, or for a
                    // catalog that could not be read, because upstream adds
                    // none.
                    ModelAnswer::Applied { .. } | ModelAnswer::Unchanged => {
                        let line = switched(next_turn, &self.model, &self.effort);
                        self.announce(notice::Notice::neutral("", line));
                    }
                    // xfx's own words, from the selector. Nothing of the id is
                    // quoted back by this surface: the reason already carries
                    // whatever of it belongs in the document, and it crossed
                    // the channel inert like every other string on it.
                    ModelAnswer::Refused { reason } => {
                        self.announce(notice::Notice::error("model", reason));
                    }
                }
                // The hint row's model label follows the model in force,
                // whichever way the answer went.
                self.render.request(Reason::Footer);
            }
            // The catalog's rows, for `/model`'s menu and column if either is
            // waiting for them, for resolving a typed query on this side, and
            // for the context meter's denominator either way.
            UiEvent::CatalogLoaded { provider, entries } => {
                self.catalog = entries;
                self.catalog_known = CatalogKnown::Loaded;
                self.provider = provider;
                let note = (provider == ProviderId::Gateway)
                    .then_some(self.gateway_note)
                    .flatten();
                if let Some(menu) = self.model_menu.as_mut() {
                    menu.loaded(&self.catalog, note);
                    self.render.request(Reason::Modal);
                }
                // The column's rows arrived with it, and its height is fixed:
                // a repaint, and the band re-solved only in case the menu's
                // changed.
                self.refit();
                self.render.request(Reason::Modal);
                // The denominator may have arrived with it, and the effort
                // segment depends on whether the model lists levels.
                self.render.request(Reason::Footer);
            }
            // Nothing to list, and the menu and the column say so -- the menu
            // in upstream's `retry /model.` words when asking again could
            // help, which the next `/model` then does.
            UiEvent::CatalogUnread { retry, .. } => {
                self.catalog_known = CatalogKnown::Unread(retry);
                if let Some(menu) = self.model_menu.as_mut() {
                    menu.failed(retry);
                    self.refit();
                    self.render.request(Reason::Modal);
                }
                self.render.request(Reason::Modal);
                self.render.request(Reason::Footer);
            }
            // The numerator, and only the numerator: see [`Self::context_used`]
            // for why the output half is deliberately dropped here rather than
            // added to it.
            UiEvent::Usage { input, output } => {
                self.context_used = input;
                if let Some(output) = output {
                    self.tally.output = Some(self.tally.output.unwrap_or(0).saturating_add(output));
                }
                self.render.request(Reason::Footer);
            }
            // The turn has stopped and is waiting for a person. Everything
            // about that -- the panel, the rows it costs the document, the
            // focus, and the clock that stops while it is up -- follows from
            // this one field being `Some`.
            UiEvent::Approval(asked) => {
                self.retry = None;
                self.ask(asked.id, asked.request);
            }
            // The same three facts for a question the **model** asked: the
            // panel, the rows it costs the document, and the focus. What it is
            // not is an approval -- nothing is granted, and refusing it denies
            // nothing ([`super::question`]).
            UiEvent::Question(request) => {
                self.retry = None;
                self.ask_question(request);
            }
            UiEvent::TurnEnded { failure } => {
                // Behind whatever of this turn's answer is still in the pacer:
                // a conclusion that overtook the text it concludes would land
                // in the middle of the answer. The tool group goes first,
                // because it is part of the turn and the conclusion is under
                // it: whatever of it is still running was stopped with the
                // turn ([`Mark::TurnOver`]).
                //
                // A cancel that waited for a running call is answered now: the
                // call's own line if the turn ends with it still running, and
                // otherwise the turn's ([`Self::cancel_turn_with`]).
                let owed = self.cancel_owed.take();
                let stops_a_call = self.group.running();
                self.mark(Mark::TurnOver);
                if let Some(waiting) = owed {
                    if !stops_a_call {
                        self.write_cancelled();
                    }
                    if waiting {
                        self.write_notice(&notice::Notice::cancelled("queue", QUEUE_DROPPED));
                    }
                }
                // A summary only under a turn the runtime ran: `/model`, `/setup`
                // and a catalog listing end with this event too, and they
                // started no turn to summarize.
                let summary = self
                    .tally
                    .started
                    .is_some()
                    .then(|| self.tally.line(Instant::now()));
                // The same fact decides the bell: `/model`, `/setup` and a
                // catalog load end here too, and upstream rings for a turn
                // (`postTurnEndHandler`). Owed once the answer is presented,
                // which [`Self::settle_band`] notices.
                self.sound.turn_ended(summary.is_some());
                self.tally.started = None;
                // A turn this session stopped has already been answered for,
                // at the keystroke: no summary under an answer that was cut
                // off, and no `✗ system:` for a failure the user asked for --
                // upstream writes the one `■ Cancelled` line and nothing after
                // it (`fx-interrupt-40x120-cc-1.5`).
                self.cancel_covers = 0;
                let (failure, summary) = if std::mem::take(&mut self.cancelling) {
                    (None, None)
                } else {
                    (failure, summary)
                };
                match (failure, summary) {
                    // Upstream reports a request that failed as an error under
                    // `system` (`app_process_runtime.zig:58-63`).
                    (Some(failure), _) => {
                        self.announce(notice::Notice::error("system", unattributed(&failure)));
                    }
                    (None, Some(summary)) => {
                        self.mark(Mark::Summary(summary));
                        self.title_session();
                    }
                    (None, None) => self.mark(Mark::EndOfLine),
                }
                // Whichever way it went, the prompt this turn carried is no
                // longer the one a later conclusion is about.
                self.turn_title = None;
                // The gesture is **not** deferred with it. What the last Ctrl-C
                // was about ended when the turn did, whatever is left to paint,
                // and a session that kept remembering it would answer the
                // *next* turn's first Ctrl-C by leaving
                // (see [`Gestures::turn_ended`]). The pacer is a delay on the
                // text, not on what the keyboard means.
                // The row is about **a turn**, and this one is over: its
                // clock and its label stop here, whatever is queued behind it.
                // The next turn's row begins when the runtime says that turn
                // began and not before -- without that pair a queued prompt
                // would inherit the elapsed time of the turn it was waiting
                // for and report a number that was never about it.
                self.activity.end();
                self.retry = None;
                self.pacer.finish();
                self.gestures.turn_ended();
                // A column that wanted the catalog while this held the queue
                // asks for it now, rather than on the next keystroke.
                if self.catalog_known == CatalogKnown::Wanted
                    && self.help_menu.is_none()
                    && self.model_menu.is_none()
                    && self.column.active(self.editor.text()).is_some()
                {
                    self.want_catalog();
                }
                // A turn is what moves a branch under a session -- a tool
                // that ran `git checkout` -- so the end of one is when the
                // workspace segment reads `HEAD` again.
                if self.statusline.workspace {
                    self.workspace_identity.refresh();
                    self.render.request(Reason::Footer);
                }
            }
            // The conversation a reset asked to drop is gone, and nothing the
            // runtime sends from here on is about it -- once this is the fence
            // of the last reset sent. An earlier reset's fence ends nothing:
            // what follows it can be a prompt the later reset dropped.
            UiEvent::Fresh { through } => {
                if self.draining.is_some_and(|last| through >= last) {
                    self.draining = None;
                }
            }
            // The picker's listing, if the picker is still up to take it.
            UiEvent::Sessions(listing) => {
                if let Some(sessions) = self.sessions.as_mut() {
                    sessions.listed(listing);
                    self.refit();
                    self.render.request(Reason::Modal);
                }
            }
            // The runtime is in the picked session now: the picker and its
            // search go, and the screen is reset and replayed.
            UiEvent::Resumed(replay) => {
                self.close_sessions(true);
                self.hydrate(replay, true);
            }
            // The rename reached the conversation. The window takes the title
            // now, and the notice says whether the log has it too.
            UiEvent::Renamed { title, unsaved } => {
                self.session_title = Some(title.clone());
                self.retitle();
                self.announce(match unsaved {
                    None => notice::Notice::neutral("session", format!("renamed to \"{title}\"")),
                    Some(problem) => notice::Notice::warning(
                        "session",
                        format!("renamed for this process but not saved ({problem})"),
                    ),
                });
            }
            UiEvent::ResumeFailed { id, failure } => {
                if let Some(sessions) = self.sessions.as_mut() {
                    sessions.failed(id, failure);
                    self.refit();
                    self.render.request(Reason::Modal);
                }
            }
            // Not a row. The band is about to come down, and the message is for
            // a cooked terminal.
            UiEvent::Fatal(message) => {
                self.mark(Mark::EndOfLine);
                self.fatal = Some(message);
                self.leave();
            }
        }
    }

    /// Puts a question in front of the user, or refuses it on their behalf.
    ///
    /// The refusal is not a fallback to be tidied up later: a question painted
    /// with its choices below the last row of the screen would leave the
    /// session waiting for a keystroke about something the user cannot read.
    /// `ask` mode's own rule decides it -- a decision xfx was never given is a
    /// refusal.
    ///
    /// **Each surface answers the fit question for itself**, because they are
    /// two different questions. The band's panel takes rows *from the band*,
    /// so whether it fits depends on everything else the band is costing
    /// ([`layout::fits_approval`]); the review plane takes a screen of its own,
    /// so what it needs is that the screen can carry its three answers
    /// ([`ApprovalScreen::presents_choices`]). Asking the band's question about
    /// a question the band was never going to show is how a short window came
    /// to deny a change the plane can display whole.
    ///
    /// **A change the band cannot hold on this screen is reviewed on the
    /// plane** rather than refused: upstream moves a question too tall for its
    /// footer onto its review screen (`approval_screen.zig:208-223`,
    /// `needsScreen`), and a review is what the plane is for. A question with
    /// no change to review -- a command, a directory -- has nothing the plane
    /// would show better, so it is refused as before.
    fn ask(&mut self, id: ApprovalId, request: ApprovalRequest) {
        let (cols, rows) = (self.geometry.cols, self.geometry.rows);
        // Which plane the question belongs on, settled from the change itself
        // before anything is installed ([`approval::ApprovalSurface`]).
        let surface = approval::ApprovalSurface::for_request(&request);
        let reviewable = request.diff.is_some();
        let on_the_plane = |request: ApprovalRequest| {
            let screen = ApprovalScreen::new(request);
            screen
                .presents_choices(cols, rows)
                .then_some(Asked::Alternate(screen))
        };
        let composed = match surface {
            approval::ApprovalSurface::Inline => {
                let panel = Panel::new(request.clone());
                let height = panel.height(cols, rows);
                if layout::fits_approval(rows, cols, height) {
                    Some(Asked::Inline(panel))
                } else if reviewable {
                    on_the_plane(request)
                } else {
                    None
                }
            }
            approval::ApprovalSurface::Alternate => on_the_plane(request),
        };
        let Some(composed) = composed else {
            // **Before the owner moves.** A question that was never asked has
            // no screen to give back, and a session left owning a plane with
            // nothing on it would compose its next frame for nobody.
            self.announce(notice::Notice::error("permissions", PANEL_TOO_SMALL));
            // Under the id it was asked with, so the prompter parked on that
            // question takes it: a refusal addressed to nobody would be
            // consumed and ignored, and the session would wait for ever for an
            // answer to a question it had already refused on the user's behalf.
            self.work.control(TurnControl::Answer {
                id,
                answer: ApprovalAnswer::Deny,
                // Nobody was asked, so nobody said anything: the panel this
                // refusal stands in for was never painted and has no draft.
                feedback: None,
            });
            return;
        };
        // The two share one slot and one of them owns the focus, so the menu
        // goes **before** the question is installed rather than being left for
        // the geometry to prefer away: a menu still open behind a question is a
        // menu whose keys the panel is swallowing, which is a menu the user
        // cannot see and cannot use. Dismissed rather than merely dropped, so
        // the answer to the question does not put it back in front of a draft
        // the user has since stopped looking at.
        self.dismiss_picker();
        // **One surface is installed, never two.** The debug assertion is the
        // hazard written down rather than argued: a second question taking the
        // alternate plane while one is standing on it would be a `1049h` with no
        // `1049l` between the two, and the standing question -- whose prompter
        // is parked on the control channel -- would be left behind a screen
        // nobody gave back. It cannot be reached today, because a turn asks one
        // question at a time and waits for its answer, so this states the
        // hazard where the next edit reads it rather than adding a branch no
        // test can drive honestly.
        debug_assert!(
            !(self.owner == ScreenOwner::Alternate && self.alternate.is_some()),
            "a second question took a plane the first one is still on"
        );
        match composed {
            Asked::Inline(panel) => {
                self.panel = Some(panel);
                self.owner = ScreenOwner::Primary;
                // The hint row is the question's now ([`Self::band_rows`]), and
                // a refusal left on it from some earlier keystroke would stand
                // in front of the keys the question is waiting for.
                self.notice = None;
            }
            // The band keeps none of its rows for this one: the question is
            // painted on the other plane, and the band underneath stays exactly
            // the band the restore repaints when the answer gives it back.
            Asked::Alternate(screen) => {
                self.alternate = Some(screen);
                self.owner = ScreenOwner::Alternate;
            }
        }
        self.asked_id = Some(id);
        // The band just grew by the panel's rows, so the divider, the composer
        // and the caret are all somewhere else.
        self.refit();
        // **Composed the moment it is installed, and after the band has been
        // re-solved.** Before the refit the geometry still has `panel: 0` and
        // its band top is the divider, so the panel's control rows would be
        // placed below the last row of the screen and the composition would
        // report itself undisclosed. And it is here at all so that a keystroke
        // arriving before the next frame is answered with the right sentence:
        // otherwise `undisclosed` would still be reporting on the last frame
        // the band painted -- which had no question in it -- and a user who
        // typed early would be told to make their window bigger.
        self.intend_approval();
        self.render.request(Reason::Modal);
        // The turn is waiting on the user (`dispatchAttentionRequired(..,
        // .permission)`, `app_worker_runtime.zig:766-767`).
        self.sound.attention();
    }

    /// Which plane the session is composing frames for.
    ///
    /// Read by the loop that owns the transitions between the two
    /// (`super::event_loop`), which is what an owner state is *for*: the frame
    /// composer, the exit and the restore all have to ask whose screen it is
    /// before they write anything, and none of them can ask a field they cannot
    /// see.
    pub(crate) fn screen_owner(&self) -> ScreenOwner {
        self.owner
    }

    /// Whether a question is in front of the user, on either plane.
    ///
    /// One reading for both surfaces, because everything that consults it -- the
    /// focus, the frozen clock -- is about *a question being up* rather than
    /// about which surface is showing it.
    fn asking(&self) -> bool {
        self.panel.is_some() || self.alternate.is_some()
    }

    /// Whether **anything** in front of the user owns the focus.
    ///
    /// One reading for the two kinds of question, because everything that
    /// consults it is about *the user being asked something* rather than about
    /// what they are being asked: the keystrokes go there and not to the
    /// composer, and the turn's clock is stopped because the interval is
    /// measuring a person. [`Self::asking`] stays the narrower fact -- a
    /// permission question, on either plane -- because the plane and the
    /// approval channel are its alone.
    fn modal(&self) -> bool {
        self.asking() || self.ask.is_some()
    }

    /// The alternate plane's rows, top first: as many as the screen has.
    ///
    /// The question's when one owns that plane, the full transcript's while it
    /// is up ([`super::full_transcript`]) -- never both, since a question
    /// closes the transcript -- and empty when neither does, so a caller that
    /// asked at the wrong moment paints nothing rather than painting a screen
    /// out of a question that has been answered.
    pub(crate) fn screen_rows(&self) -> Vec<String> {
        let (cols, rows) = (self.geometry.cols, self.geometry.rows);
        if let Some(screen) = self.alternate.as_ref() {
            return screen.painted(cols, rows, &self.palette);
        }
        if self.full.is_open() {
            // The hint row the band would paint, as upstream's screen keeps
            // its statusline under the footer (`paint_plan.zig:1034-1045`).
            let status = self.painted(self.palette.statusline(), self.hint_row());
            return self.full.rows(cols, rows, self.palette, &status);
        }
        Vec::new()
    }

    /// Where the caret goes on that plane: the marked choice, or the full
    /// transcript's footer. The cases read it; the loop paints
    /// [`Self::screen_caret`].
    #[cfg(test)]
    pub(crate) fn screen_cursor(&self) -> (u16, u16) {
        self.screen_caret().at
    }

    /// The same, and whether it is shown: a question's caret is, on the
    /// choice or in the amendment the keys go to; the full transcript's is
    /// not, as upstream's is not (`paint_plan.zig:1047`).
    pub(crate) fn screen_caret(&self) -> Caret {
        if let Some(screen) = self.alternate.as_ref() {
            return Caret::from(screen.caret(self.geometry.cols, self.geometry.rows));
        }
        if self.full.is_open() {
            return Caret::hidden(self.full.caret(self.geometry.rows));
        }
        Caret::from((1, 0))
    }

    /// Gives the plane back without answering the question on it.
    ///
    /// The exit's, and only the exit's (`super::event_loop`'s `shut_down`): a
    /// session coming down with a question still up has to leave the terminal on
    /// the plane its user's shell is on, and there is nobody left to answer.
    /// Every ordinary way out of a question goes through [`Self::decide`], which
    /// releases the plane *with* the answer.
    pub(crate) fn release_screen(&mut self) {
        self.alternate = None;
        self.asked_id = None;
        self.invalidate_approval();
        self.full.close();
        self.owner = ScreenOwner::Primary;
    }

    // -----------------------------------------------------------------------
    // the full transcript
    // -----------------------------------------------------------------------

    /// Opens the full transcript on the alternate plane, at its tail.
    ///
    /// Refused while that plane is anyone else's (`transitionScreen`,
    /// `input_full_transcript_runtime.zig:83-99`: an owned alternate screen or
    /// an active approval). Ctrl-O at a question never reaches here anyway --
    /// the question has the keys (`Self::consume`) -- so this is the rule
    /// written where the next edit reads it rather than a branch a test can
    /// drive.
    fn open_full_transcript(&mut self) {
        if self.owner != ScreenOwner::Primary || self.modal() {
            return;
        }
        self.full.open();
        // The alternate plane, through the machinery the large-change review
        // uses: the loop pays what the primary plane owes, takes the plane in
        // the same write, and gives it back whole (`super::event_loop`'s
        // `paint_alternate`).
        self.owner = ScreenOwner::Alternate;
        self.render.request(Reason::Modal);
    }

    /// Closes it, giving the plane back unless a question has taken it over.
    fn close_full_transcript(&mut self) {
        if !self.full.is_open() {
            return;
        }
        self.full.close();
        if self.alternate.is_none() {
            self.owner = ScreenOwner::Primary;
        }
        self.render.request(Reason::Modal);
    }

    /// One keystroke while the full transcript is up
    /// (`input_full_transcript_runtime.zig:147-198`), and whether the screen
    /// took it.
    ///
    /// Text and pasted bytes are never the screen's: upstream's map names
    /// none, and they reach its composer, hidden behind the screen, as they
    /// reach this one -- the draft is there when the screen closes.
    fn view_full_transcript(&mut self, event: &Input) -> bool {
        let Input::Action(action) = event else {
            return false;
        };
        let (cols, rows) = (self.geometry.cols, self.geometry.rows);
        match self.full.key(*action, cols, rows, self.palette) {
            full_transcript::Reply::Close => self.close_full_transcript(),
            full_transcript::Reply::Moved => self.render.request(Reason::Modal),
            full_transcript::Reply::Held => {}
            full_transcript::Reply::Unbound => return false,
        }
        true
    }

    /// What the full transcript keeps of an event the document is not shown,
    /// before the event is applied.
    ///
    /// A question closes the screen first, so that [`Self::ask`] decides the
    /// plane on its own terms: a change too large for the band takes the
    /// alternate plane over from the transcript in one repaint, anything else
    /// gives it back -- upstream hands off or closes the same way
    /// (`transitionFullTranscriptForApproval`, `app_lifecycle.zig:1298-1316`).
    fn full_transcript_sees(&mut self, event: &UiEvent) {
        self.full.observe(event);
        if matches!(event, UiEvent::Approval(_) | UiEvent::Question(_)) {
            self.close_full_transcript();
        }
        // The screen shows what was just recorded, at the next frame.
        if self.full.is_open() {
            self.render.request(Reason::Modal);
        }
    }

    /// Tells the full transcript what a document write is about to say, in
    /// the document's order.
    ///
    /// Notices and the answer's text are told where they are written
    /// ([`Self::write_notice`], [`Self::write_answer`]), because both reach
    /// the document by roads other than a mark as well.
    fn record(&mut self, mark: &Mark) {
        match mark {
            Mark::Card(text) => self.full.prompt(text),
            Mark::Summary(line) => self.full.usage(line),
            // The one line said this way is what the user said at an
            // approval, which upstream's screen heads as a prompt.
            Mark::Line(line, role) => {
                let kind = if line.starts_with(AMENDMENT_PREFIX) {
                    full_transcript::Kind::Prompt
                } else {
                    full_transcript::Kind::Notice
                };
                self.full.line(line, *role, kind);
            }
            Mark::Block(rows) => {
                let rows: Vec<String> = rows.iter().map(|(row, _)| row.clone()).collect();
                self.full.block(&rows);
            }
            Mark::ToolStart { id, shown } => self.full.tool_started(id, shown),
            Mark::ToolResult {
                id,
                ok,
                detail,
                outcome,
                changes,
            } => self.full.tool_finished(id, *ok, detail, *outcome, *changes),
            Mark::TurnOver => self.full.turn_over(),
            Mark::Notice(_) | Mark::EndOfLine => {}
        }
    }

    // -----------------------------------------------------------------------
    // what makes an affirmative answerable
    // -----------------------------------------------------------------------

    /// What the frame about to be written would disclose about the standing
    /// question.
    ///
    /// Composed from the surface that is really installed, at the geometry the
    /// frame is about to be built from, so the claim and the bytes are one
    /// reading. With no question up it intends nothing, which clears whatever
    /// the last composition left pending.
    pub(crate) fn intend_approval(&mut self) {
        let (cols, rows) = (self.geometry.cols, self.geometry.rows);
        let intent = match (self.asked_id, self.panel.as_ref(), self.alternate.as_ref()) {
            (Some(id), Some(panel), _) => Intent::capture(
                id,
                Surface::Inline,
                cols,
                rows,
                panel
                    .compose(
                        cols,
                        rows,
                        self.geometry.band_top(),
                        // The activity row is the only thing `band_rows` puts
                        // ahead of the panel, so the offset is that one boolean
                        // and needs no method of its own.
                        u16::from(self.geometry.activity.is_some()),
                    )
                    .disclosure,
            ),
            (Some(id), _, Some(screen)) => Intent::capture(
                id,
                Surface::Alternate,
                cols,
                rows,
                screen.composition(cols, rows).disclosure,
            ),
            _ => None,
        };
        self.readiness.intend(intent);
    }

    /// What the terminal did with those bytes.
    pub(crate) fn approval_landed(&mut self, outcome: Outcome) {
        self.readiness.landed(outcome);
    }

    /// A write the screen refused: it may have left half a frame.
    pub(crate) fn approval_write_failed(&mut self) {
        self.readiness.write_failed();
    }

    /// Everything the screen said is no longer true.
    pub(crate) fn invalidate_approval(&mut self) {
        self.readiness.invalidate();
    }

    /// The post-write check, one tick after the write
    /// (`super::event_loop`'s `collect_facts`).
    pub(crate) fn reconcile_approval(&mut self) {
        self.readiness.reconcile(
            self.geometry.cols,
            self.geometry.rows,
            self.render.resize_pending(),
        );
    }

    /// Whether an affirmative answer to `id` may be taken on this screen.
    pub(crate) fn approval_ready(&self, id: ApprovalId) -> bool {
        self.readiness
            .ready(id, self.geometry.cols, self.geometry.rows)
    }

    /// Whether an amendment draft has the keys, on whichever surface is up.
    ///
    /// One reading for both, because everything that consults it is about
    /// *where the next character goes* rather than about which plane the
    /// question happened to land on ([`super::approval_amendment`]).
    fn drafting(&self) -> bool {
        self.panel.as_ref().is_some_and(Panel::drafting)
            || self
                .alternate
                .as_ref()
                .is_some_and(ApprovalScreen::drafting)
    }

    /// Says whatever the standing question owes the user, on the plane they can
    /// see.
    ///
    /// The same rule the readiness refusal follows and for the same reason:
    /// while the review plane is up it owns every row of the terminal, so a
    /// sentence written into the document is a sentence behind a screen nobody
    /// can look past. The band's own hint row is where it goes otherwise --
    /// which is where every other refused keystroke on this surface is
    /// reported ([`Self::notice`]).
    fn notice_at_the_question(&mut self) {
        let notice = match (self.panel.as_mut(), self.alternate.as_mut()) {
            (Some(panel), _) => panel.take_notice(),
            (_, Some(screen)) => screen.take_notice(),
            (None, None) => None,
        };
        let Some(notice) = notice else {
            return;
        };
        match self.alternate.as_mut() {
            Some(screen) => screen.set_status(notice.to_string()),
            None => {
                self.notice = Some(notice);
                self.render.request(Reason::Footer);
            }
        }
    }

    /// Offers one keystroke to the `/help` catalog, and says whether it was
    /// taken.
    ///
    /// The six keys [`HelpAction::of`] names, and nothing else: a letter, a
    /// Backspace or a C-u is an edit of the search and goes on to the composer
    /// with the catalog still up. The arrows **wrap** here, which the resume
    /// picker upstream does not; Tab and Shift-Tab change the tab, and with it
    /// the catalog's height, so the band is re-solved rather than repainted.
    fn offer_to_catalog(&mut self, event: &Input) -> bool {
        let Input::Action(action) = event else {
            return false;
        };
        let Some(request) = HelpAction::of(*action) else {
            return false;
        };
        let Some(menu) = self.help_menu.as_mut() else {
            return false;
        };
        match request {
            HelpAction::Up | HelpAction::Down => {
                // The window a move scrolls against is the one on the screen:
                // the rows the geometry gave the catalog.
                let visible = menu.visible_items(self.geometry.panel);
                if menu.step(request == HelpAction::Down, visible) {
                    self.render.request(Reason::Modal);
                }
                true
            }
            HelpAction::NextCategory | HelpAction::PreviousCategory => {
                menu.cycle(request == HelpAction::NextCategory);
                self.refit();
                self.render.request(Reason::Modal);
                true
            }
            HelpAction::Close => {
                self.close_catalog(true);
                true
            }
            HelpAction::Open => self.open_from_catalog(),
        }
    }

    /// Enter, while the catalog is up (`submitHelpMenuSelection`,
    /// `app_input_runtime.zig:2732-2762`), and whether the keystroke was spent.
    ///
    /// Three answers, in upstream's order:
    ///
    /// * **The composer already names a command** ([`help_menu::names_exactly`]):
    ///   the catalog closes and the Return goes on to submit the line as typed
    ///   -- `/version` typed into the search runs `/version`, whatever row the
    ///   search left marked.
    /// * **The marked command takes an argument**: it goes in the composer with
    ///   the space after it, the catalog closes, and nothing runs -- the next
    ///   thing typed is the argument.
    /// * **Otherwise** the marked command replaces the search and the Return
    ///   goes on to run it, as the completion menu's does
    ///   ([`PickerOutcome::Run`]).
    ///
    /// With nothing listed -- an empty tab, a search that found nothing -- the
    /// Return is spent and nothing happens.
    fn open_from_catalog(&mut self) -> bool {
        if help_menu::names_exactly(self.editor.text()) {
            self.close_catalog(false);
            return false;
        }
        let Some(spec) = self.help_menu.as_ref().and_then(HelpMenu::selected) else {
            return true;
        };
        self.close_catalog(false);
        if spec.has_args {
            // Not a slash word any more once the space is in it, so no
            // completion menu opens for it.
            self.replace_word(&picker::completed(spec.name));
            return true;
        }
        // Dismissed before the name is written, so the name does not open a
        // completion menu of its own on the way to being submitted.
        self.dismissed.dismiss(Trigger::Slash);
        self.replace_word(spec.name);
        false
    }

    /// Takes the catalog down, and its search with it when `clear_query`
    /// (`closeHelpMenu`, `app_input_runtime.zig:2314-2322`).
    ///
    /// Escape clears it: the search was the catalog's, and a composer left
    /// holding `ses` after the catalog that used it is gone would be a draft
    /// the user never meant to type. Enter does not, because what Enter does
    /// next is put a command where the search was or submit it as it stands.
    fn close_catalog(&mut self, clear_query: bool) {
        if self.help_menu.take().is_none() {
            return;
        }
        if clear_query && !self.editor.is_empty() {
            // Through the one funnel every whole-draft disposal takes, which
            // re-solves the band for the rows the catalog gave back.
            self.take_draft();
            self.amended(None);
            return;
        }
        self.refit();
        self.render.request(Reason::Modal);
    }

    /// Offers one keystroke to the `/resume` picker, and says whether it was
    /// taken.
    ///
    /// The six keys [`SessionAction::of`] names: the arrows move the mark and
    /// **stop** at the ends (`moveVisibleItems`, `app_session_runtime.zig:538-552`),
    /// Tab and Shift-Tab change the scope, Escape closes the picker and empties
    /// the search, and Enter asks the runtime for the marked session. Anything
    /// else is an edit of the search.
    fn offer_to_sessions(&mut self, event: &Input) -> bool {
        let Input::Action(action) = event else {
            return false;
        };
        let Some(request) = SessionAction::of(*action) else {
            return false;
        };
        let Some(sessions) = self.sessions.as_mut() else {
            return false;
        };
        match request {
            SessionAction::Up | SessionAction::Down => {
                let visible = sessions.visible_items(self.geometry.panel);
                if sessions.step(request == SessionAction::Down, visible) {
                    self.render.request(Reason::Modal);
                }
            }
            SessionAction::Scope => {
                sessions.toggle_scope();
                self.refit();
                self.render.request(Reason::Modal);
            }
            SessionAction::Close => self.close_sessions(true),
            SessionAction::Resume => self.resume_selected(),
        }
        true
    }

    /// Offers one keystroke to `/model`'s menu, and says whether it was taken.
    ///
    /// The six keys [`ModelAction::of`] names, and nothing else, as the
    /// catalog's: a letter, a Backspace or a C-u edits the search and goes on to
    /// the composer with the menu still up. Up and Down wrap; Tab and Shift-Tab
    /// change the vendor tab, and with it the menu's height, so the band is
    /// re-solved; Enter uses the marked model and Escape closes the menu and
    /// empties its search (`fx-model2-40x120-menu-esc`).
    fn offer_to_models(&mut self, event: &Input) -> bool {
        let Input::Action(action) = event else {
            return false;
        };
        let Some(request) = ModelAction::of(*action) else {
            return false;
        };
        let Some(menu) = self.model_menu.as_mut() else {
            return false;
        };
        match request {
            ModelAction::Up | ModelAction::Down => {
                let visible = menu.visible_items(self.geometry.panel);
                if menu.step(request == ModelAction::Down, visible) {
                    self.render.request(Reason::Modal);
                }
            }
            ModelAction::NextVendor | ModelAction::PreviousVendor => {
                if menu.cycle(request == ModelAction::NextVendor) {
                    self.refit();
                    self.render.request(Reason::Modal);
                }
            }
            ModelAction::Close => self.close_models(),
            ModelAction::Use => self.use_marked_model(),
        }
        true
    }

    /// Enter, while the picker is up (`submitSessionPickerSelection`,
    /// `app_input_runtime.zig:1788-1826`): the marked session, asked of the
    /// runtime. Nothing listed, a listing still loading, or a resume already on
    /// its way: the keystroke is spent and nothing is asked.
    fn resume_selected(&mut self) {
        let Some(id) = self.sessions.as_ref().and_then(SessionPicker::selected) else {
            return;
        };
        match self.work.submit(TurnWork::Resume(id.clone())) {
            Ok(()) => {
                if let Some(sessions) = self.sessions.as_mut() {
                    sessions.resuming(id);
                }
            }
            Err(rejected) => self.refused(rejected),
        }
    }

    /// Takes the picker down, and its search with it when `clear_query`.
    ///
    /// Escape clears it, as upstream's `cancelSessionMenu` does
    /// (`app_input_runtime.zig:3300-3307`): the search was the picker's.
    fn close_sessions(&mut self, clear_query: bool) {
        if self.sessions.take().is_none() {
            return;
        }
        if clear_query && !self.editor.is_empty() {
            self.take_draft();
            self.amended(None);
            return;
        }
        self.refit();
        self.render.request(Reason::Modal);
    }

    /// Enter, while `/model`'s menu is up (`submitModelMenuSelection`,
    /// `app_input_runtime.zig:2766-2785`).
    ///
    /// The menu closes and its search goes with it, and the marked model goes
    /// on exactly as upstream's does (`beginExactModelSelection`,
    /// `input_completion_runtime.zig:1260-1262`): into the inline effort
    /// column, with the composer reading `/model <id> `, for a model the
    /// catalog lists levels for (`fx-model2-40x120-menu-enter`); applied at
    /// once for one it lists none for. With nothing listed -- still loading,
    /// nothing found -- the Return is spent and nothing happens.
    fn use_marked_model(&mut self) {
        let Some(id) = self.model_menu.as_ref().and_then(ModelMenu::selected) else {
            return;
        };
        let id = id.to_string();
        self.close_models();
        let entry = self.catalog.iter().find(|entry| entry.id == id).cloned();
        match entry.map(|entry| self.column.choose(&entry)) {
            Some(Chosen::Efforts(text)) => self.write_column_text(&text),
            Some(Chosen::Commit(model)) => self.commit_column(model, None),
            // A row the menu listed is a row of the catalog, so this is not a
            // case the menu can reach; the id is still the user's choice.
            None => self.commit_column(id, None),
        }
    }

    /// The inline column's rows, under the argument they are about, exactly as
    /// many as the band gave it ([`model_column::height`]).
    fn column_rows(&self) -> Vec<String> {
        let text = self.editor.text();
        let Some(query) = self.column.active(text) else {
            return Vec::new();
        };
        let anchor = self.column_anchor(query.token_start);
        self.column.rows(
            &query,
            known(self.catalog_known, &self.catalog),
            anchor,
            self.geometry.cols,
            self.geometry.panel,
            self.palette,
        )
    }

    /// The screen column under the composer's byte `offset`: past the prompt,
    /// the cells to its left on the composer row it is on
    /// (`projectedAnchorColumn`, `input_presentation.zig:377-380`).
    fn column_anchor(&self, offset: usize) -> u16 {
        let text = self.editor.text();
        let rows = super::wrap::wrap(text, self.text_cols());
        let (_, cells) = super::wrap::cursor_point(text, &rows, offset);
        PROMPT_CELLS.saturating_add(cells)
    }

    /// Offers one keystroke to `/model`'s inline column, and says whether it
    /// was taken (`app_input_runtime.zig:262-340`, `:1490-1600`).
    ///
    /// Only while a column is open over a `/model ` line, and never while
    /// `/model`'s menu or the catalog has the composer as its search. Up and
    /// Down walk the open column, wrapping; Tab writes the marked row into the
    /// composer; Left steps back a column -- and is the caret's from the first;
    /// Escape puts the column away and leaves the text; Enter takes the marked
    /// row, or falls through to the composer's own submit when the column has
    /// nothing to give (the explicit form pasted whole, an id nothing lists).
    /// **A space is the column's** whether or not it advances, as upstream's
    /// is: it moves on from a model spelled exactly, and does nothing else.
    /// Home, End and the word moves throw a later column away and move the
    /// caret.
    fn offer_to_column(&mut self, event: &Input) -> bool {
        // **The keys first, and nothing copied for any other.** Every byte of
        // a paste is offered here on its way to the paste buffer, and a draft
        // can be megabytes: a check that copied the draft per offer would
        // make a paste cost its length times the draft's.
        let bound = matches!(
            event,
            Input::Text(' ')
                | Input::Action(
                    Action::Up
                        | Action::Down
                        | Action::Tab
                        | Action::Left
                        | Action::Escape
                        | Action::Submit
                        | Action::Home
                        | Action::End
                        | Action::WordLeft
                        | Action::WordRight
                )
        );
        if !bound || self.help_menu.is_some() || self.model_menu.is_some() {
            return false;
        }
        if self.column.active(self.editor.text()).is_none() {
            return false;
        }
        // A `/model` line, so short in every real case; owned, because what
        // the column does next writes the composer it was read from.
        let text = self.editor.text().to_string();
        let Some(query) = self.column.active(&text) else {
            return false;
        };
        match event {
            Input::Text(' ') => {
                self.advance_column(&query);
                true
            }
            Input::Action(Action::Up | Action::Down) => {
                let down = matches!(event, Input::Action(Action::Down));
                if self
                    .column
                    .step(&query, known(self.catalog_known, &self.catalog), down)
                {
                    self.render.request(Reason::Modal);
                    self.render.request(Reason::Footer);
                }
                true
            }
            Input::Action(Action::Tab) => {
                if let Some(text) = self
                    .column
                    .complete(&query, known(self.catalog_known, &self.catalog))
                {
                    self.write_column_text(&text);
                }
                true
            }
            Input::Action(Action::Left) => {
                match self
                    .column
                    .step_back(known(self.catalog_known, &self.catalog))
                {
                    Some(text) => {
                        self.write_column_text(&text);
                        true
                    }
                    None => false,
                }
            }
            Input::Action(Action::Escape) => {
                self.column.dismiss();
                self.refit();
                self.render.request(Reason::Modal);
                self.render.request(Reason::Footer);
                true
            }
            Input::Action(Action::Submit) => self.submit_column(&query),
            Input::Action(Action::Home | Action::End | Action::WordLeft | Action::WordRight) => {
                if self.column.pending().is_some() {
                    self.column.reset();
                }
                false
            }
            _ => false,
        }
    }

    /// A space at the column (`advanceModelPickerOnSpace`,
    /// `input_completion_runtime.zig:1123-1147`): from the model column, a
    /// model spelled exactly is taken as Enter would take it -- which for a
    /// model with no levels **makes the change**, so the rest of a typed
    /// `/model <id> <more>` arrives in an emptied composer and is sent as a
    /// prompt, exactly as upstream's is (`fx-model2-40x120-explicit-bad-effort`).
    /// From the effort column a space would open the speed column, which xfx
    /// does not have yet, so it does nothing.
    fn advance_column(&mut self, query: &model_column::Query<'_>) {
        if !self.editor.caret_at_end() {
            return;
        }
        if query.stage != model_column::Stage::Model {
            return;
        }
        let Some(entry) =
            Column::exact_model(query.query, known(self.catalog_known, &self.catalog)).cloned()
        else {
            return;
        };
        self.chose_model(&entry);
    }

    /// Enter at the column (`submitModelPicker`,
    /// `input_completion_runtime.zig:1161-1210`), and whether it was taken.
    fn submit_column(&mut self, query: &model_column::Query<'_>) -> bool {
        let catalog = known(self.catalog_known, &self.catalog);
        match query.stage {
            model_column::Stage::Model => {
                // Still loading: a query goes on to the composer's own submit
                // -- the router form resolves it -- and an empty one is spent.
                if catalog == Known::Loading {
                    return query.query.trim_matches([' ', '\t']).is_empty();
                }
                let Some(entry) = self.column.chosen_model(query, catalog).cloned() else {
                    return false;
                };
                self.chose_model(&entry);
                true
            }
            model_column::Stage::Effort => {
                let Some(model) = self.column.pending().map(str::to_string) else {
                    return false;
                };
                // Nothing that matches: the Return is spent, the column stays.
                if let Some(effort) = self.column.chosen_effort(query, catalog) {
                    self.commit_column(model, Some(effort));
                }
                true
            }
        }
    }

    /// What taking `entry` in the model column leads to: the effort column,
    /// or the change.
    fn chose_model(&mut self, entry: &CatalogEntry) {
        match self.column.choose(entry) {
            Chosen::Efforts(text) => self.write_column_text(&text),
            Chosen::Commit(model) => self.commit_column(model, None),
        }
    }

    /// Makes the change the column settled on, and empties the composer
    /// (`selectModelFromPicker` and `clearCurrent`).
    fn commit_column(&mut self, model: String, effort: Option<Effort>) {
        self.column.reset();
        self.clear_composer();
        self.request_change(ModelChange {
            name: model,
            effort,
            picker: true,
            announced: true,
            next_turn: self.turn_pending(),
        });
    }

    /// Puts `text` in the composer for the column, which has already set its
    /// own state for it (`setModelComposerText`).
    ///
    /// A boundary for undo, as a completion is ([`Self::replace_word`]): the
    /// text it replaced is gone with every delta that named its offsets.
    fn write_column_text(&mut self, text: &str) {
        self.take_draft();
        // Refused only by the byte budget, and a catalog id is bounded far
        // below it, into a composer this call has just emptied.
        let _ = self.editor.insert(text);
        self.column_edit = ColumnEdit::Written;
        self.amended(None);
    }

    /// Settles the inline column against what the composer now holds, and
    /// asks for the catalog it lists from when it is open and nothing has.
    ///
    /// Called from [`Self::moved`], beside the completion menu's reconcile, so
    /// every change to the draft is seen by both.
    fn reconcile_column(&mut self) {
        let edit = std::mem::take(&mut self.column_edit);
        // Nothing to settle for a draft that is no `/model` line, was none at
        // the last look, and finds the column at rest -- which is every
        // keystroke of an ordinary prompt, so it costs a prefix check and
        // never a copy of the draft.
        let shaped = model_column::shaped_line(self.editor.text());
        if !shaped && self.column_seen.is_empty() && self.column.at_rest() {
            return;
        }
        let at_end = self.editor.caret_at_end();
        if self.editor.text() != self.column_seen {
            // Only a `/model` line is remembered: it is the only text a later
            // column's edit rule is asked about.
            let now = if shaped {
                self.editor.text().to_string()
            } else {
                String::new()
            };
            let before = std::mem::replace(&mut self.column_seen, now);
            self.column
                .edited(&before, self.editor.text(), edit, at_end);
        }
        if self.help_menu.is_none()
            && self.model_menu.is_none()
            && self.column.active(self.editor.text()).is_some()
        {
            self.want_catalog();
        }
    }

    /// Takes `/model`'s menu down and empties its search
    /// (`closeModelMenu(app, true)`, `app_input_runtime.zig:2334-2344`).
    fn close_models(&mut self) {
        if self.model_menu.take().is_none() {
            return;
        }
        if !self.editor.is_empty() {
            // Through the one funnel every whole-draft disposal takes, which
            // re-solves the band for the rows the menu gave back.
            self.take_draft();
            self.amended(None);
            return;
        }
        self.refit();
        self.render.request(Reason::Modal);
    }

    /// One keystroke, while the `/statusline` picker has the focus.
    ///
    /// The arrows and Ctrl-J/Ctrl-K move the mark, wrapping; Enter, Left and
    /// Right flip the marked item **silently** -- the row is the answer, and
    /// nothing reaches the document unless the write failed
    /// (`submitCompactCommandMenuSelection` and
    /// `routeCompactCommandMenuEscapeAction`, `app_input_runtime.zig:2553-2690`,
    /// both into `applyStatuslineItem(.silent)`); Escape closes it.
    fn offer_to_statusline(&mut self, event: Input) {
        let Input::Action(action) = event else {
            return;
        };
        let Some(request) = StatuslineAction::of(action) else {
            return;
        };
        let Some(menu) = self.statusline_menu.as_mut() else {
            return;
        };
        match request {
            StatuslineAction::Up | StatuslineAction::Down => {
                menu.step(request == StatuslineAction::Down);
                self.render.request(Reason::Modal);
            }
            StatuslineAction::Flip => {
                let item = menu.selected();
                self.set_statusline(item, !self.statusline.get(item), false);
            }
            StatuslineAction::Close => {
                self.statusline_menu = None;
                // The composer's row comes back with the picker's going.
                self.refit();
                self.render.request(Reason::Modal);
            }
        }
    }

    /// Turns one footer segment on or off, keeps it in the profile, and --
    /// when `announce` -- says both (`applyStatuslineItem`,
    /// `app_commands.zig:3905-3929`).
    ///
    /// **The write is made here, on this thread**, and that is a decision: the
    /// profile is a local file of at most 64 KiB
    /// ([`crate::config::MAX_SETTINGS_BYTES`]) read once and replaced by one
    /// staged rename ([`crate::provider::profile::write_preference`]), and
    /// upstream persists from its command handler the same way. The
    /// alternative, the runtime's work queue, runs behind the turn in flight
    /// and refuses outright when a prompt is already waiting
    /// ([`Rejected::Busy`]) -- so a segment flipped mid-turn would neither be
    /// kept nor said until the turn was over, or at all.
    ///
    /// The change is in force for this process whether or not it was kept;
    /// a write that failed says so, in error, whichever path asked for it.
    fn set_statusline(&mut self, item: StatusLineItem, on: bool, announce: bool) {
        self.statusline.set(item, on);
        if item == StatusLineItem::Workspace && on {
            self.workspace_identity.refresh();
        }
        match interactive::save_statusline(self.settings_path.as_deref(), item, on) {
            Ok(()) if announce => self.announce(notice::Notice::neutral(
                interactive::STATUSLINE_TOPIC,
                interactive::STATUSLINE_SAVED,
            )),
            Ok(()) => {}
            Err(reason) => self.announce(notice::Notice::error(
                interactive::STATUSLINE_TOPIC,
                interactive::statusline_not_saved(&reason),
            )),
        }
        if announce {
            self.announce(notice::Notice::neutral(
                interactive::STATUSLINE_TOPIC,
                interactive::statusline_state(item, on),
            ));
        }
        // The hint row changed, and so has the picker's row when it is up.
        self.render.request(Reason::Footer);
        if self.statusline_menu.is_some() {
            self.render.request(Reason::Modal);
        }
    }

    /// Offers one keystroke to the completion menu, and says whether it was
    /// taken.
    ///
    /// **Before the editor and before the gestures**, which is the whole of
    /// what "the menu binds a key" means: an Up while a menu is open moves the
    /// mark rather than the caret, and an Escape closes the menu rather than
    /// arming the gesture that clears the composer.
    ///
    /// Enter is the one that is answered `false` on purpose. It puts the marked
    /// command in the composer in place of the word being typed, closes the
    /// menu, and then goes on to mean what it means everywhere else -- so `/he`
    /// and a Return runs `/help`, and so does a Down to a row and a Return
    /// (upstream resolves the submission to the marked row,
    /// `input_submit_runtime.zig:1087-1089`). A menu that swallowed it would
    /// make every command take two Returns.
    fn offer_to_picker(&mut self, event: &Input) -> bool {
        let Input::Action(action) = event else {
            return false;
        };
        let Some(request) = PickerAction::of(*action) else {
            return false;
        };
        let Some(picker) = self.picker.as_mut() else {
            return false;
        };
        match picker.apply(request) {
            PickerOutcome::Changed => {
                // The mark moved, which is a frame and nothing else: the band
                // is the same height and the draft is untouched.
                self.render.request(Reason::Modal);
                true
            }
            PickerOutcome::Complete(name) => {
                self.complete(name);
                true
            }
            PickerOutcome::Run(name) => {
                // Closed before the word is replaced, so the name written in
                // its place does not open a menu of its own on the way to
                // being submitted.
                self.dismissed.dismiss(Trigger::Slash);
                self.replace_word(name);
                false
            }
            PickerOutcome::Dismiss => {
                self.dismiss_picker();
                true
            }
        }
    }

    /// Puts a completed name in the composer, in place of the word that was
    /// being typed.
    ///
    /// The whole draft is the query ([`super::picker::Trigger::of`]), so this
    /// is a replacement rather than a splice -- and it goes through
    /// [`Self::take_draft`] like every other thing that empties the composer,
    /// so the paste bookkeeping cannot be left describing text that is gone.
    ///
    /// **The menu is not dismissed by it**, which is upstream's choice: the
    /// replacement starts a new menu episode (`composer_text_replacement.zig:88`,
    /// `resetInlinePickerEpisode`), so a completed name that is still a slash
    /// word -- `/vers` completed to `/version` -- shows a menu of that one
    /// command, with the mark on it, for Enter to run or Escape to close. A
    /// name completed with the space its argument needs has stopped being a
    /// slash word, and has no menu.
    fn complete(&mut self, name: &'static str) {
        self.replace_word(&picker::completed(name));
    }

    /// Puts `text` in the composer in place of the slash word.
    ///
    /// What Tab and Enter share: Tab leaves the composer holding the
    /// completion, Enter goes on to submit it.
    fn replace_word(&mut self, text: &str) {
        self.take_draft();
        // Refused only by the byte budget, and a command name is nine bytes at
        // its longest against a cap of eight mebibytes ([`editor::Editor`]) --
        // into a composer this call has just emptied.
        let _ = self.editor.insert(text);
        // **`None`, and that is the whole contract.** The draft this replaced
        // was the query, and it died at [`Self::take_draft`] above along with
        // every delta that named its offsets. Recording the insert alone would
        // give an undo that deletes the command name and leaves the composer
        // empty -- silently discarding the query the user typed -- so a
        // completion is a boundary and undo does not step back across it. An
        // xfx decision, not an upstream ruling: `input_completion_runtime.zig`'s
        // `historyBoundary` call sits on prompt recall.
        self.amended(None);
    }

    /// Closes the menu, and remembers that it was closed.
    ///
    /// Silent when there is none, so every caller can say "there is no menu
    /// now" without first asking whether there was one.
    fn dismiss_picker(&mut self) {
        let Some(picker) = self.picker.take() else {
            return;
        };
        self.dismissed.dismiss(picker.trigger());
        // The band just gave the menu's rows back to the document.
        self.refit();
        self.render.request(Reason::Modal);
    }

    /// Settles what menu the draft is asking for, if any.
    ///
    /// Called from [`Self::edited`] and from nowhere else, which is what makes
    /// "the menu is a view of the composer" true rather than intended: there is
    /// no path that changes the draft without coming through here, and none
    /// that changes the menu without the draft having changed.
    fn reconcile_picker(&mut self) {
        // **While the catalog is up the draft is its search, not a command
        // being typed**, so no completion menu opens for it -- `/version`
        // typed into the catalog narrows the catalog to one row
        // (`app_input_runtime.zig:2935-2938`, `syncHelpMenu`) -- and the
        // dismissal is left exactly as it was for when the catalog closes.
        if let Some(menu) = self.help_menu.as_mut() {
            menu.set_query(self.editor.text());
            self.picker = None;
            self.provider_picker = None;
            return;
        }
        // The same for `/model`'s menu (`syncModelMenu`,
        // `app_input_runtime.zig:2961-2965`): `gro` typed into it is a search
        // -- and so is `/provider ll`, which opens no column under it.
        if let Some(menu) = self.model_menu.as_mut() {
            menu.set_query(self.editor.text());
            self.picker = None;
            self.provider_picker = None;
            return;
        }
        // The same for the session picker: the draft is the session search,
        // and `/provider ll` typed into it opens no column under it.
        if let Some(sessions) = self.sessions.as_mut() {
            sessions.set_query(self.editor.text());
            self.picker = None;
            self.provider_picker = None;
            return;
        }
        // The `/provider` column next: a draft that is its argument has a
        // space in it, so it is never a slash word as well.
        if self.reconcile_provider_picker() {
            self.picker = None;
            // Still asked, so a completion menu Escape closed over `/provider`
            // is forgotten once the draft has become the column's: the trigger
            // changed kind, and that is what a dismissal lasts until.
            let _ = self.dismissed.admits(Trigger::of(self.editor.text()));
            return;
        }
        if !self.dismissed.admits(Trigger::of(self.editor.text())) {
            self.picker = None;
            return;
        }
        // Rebuilt only when the query really changed: an arrow key that moved
        // the caret inside the same word must not put the mark back on the
        // first row.
        if self
            .picker
            .as_ref()
            .is_some_and(|open| open.query() == self.editor.text())
        {
            return;
        }
        let query = self.editor.text().to_string();
        self.picker = Picker::open(&query);
    }

    /// Settles the `/provider` column for the draft, and says whether the
    /// draft is the column's -- open or dismissed.
    ///
    /// Rebuilt only when the text changed, so a caret move inside the argument
    /// leaves the mark where the arrows put it; any change of text puts it back
    /// on the first row, as upstream's does.
    fn reconcile_provider_picker(&mut self) -> bool {
        let text = self.editor.text();
        if !provider_picker::is_query(text) {
            self.provider_dismissed = false;
            self.provider_picker = None;
            return false;
        }
        if self.provider_dismissed {
            self.provider_picker = None;
            return true;
        }
        if self
            .provider_picker
            .as_ref()
            .is_some_and(|open| open.text() == text)
        {
            return true;
        }
        let current = (!self.missing_credential).then_some(self.provider);
        self.provider_picker = ProviderPicker::open(text, current);
        true
    }

    /// Offers one keystroke to the `/provider` column, and says whether it was
    /// taken.
    ///
    /// Upstream's keys (`provider_picker_runtime.zig:164-246`,
    /// `app_input_runtime.zig:264-296`):
    ///
    /// * **Up and Down** move the mark and wrap.
    /// * **Tab** writes the prefix and the chosen row into the composer and
    ///   commits nothing -- the column then shows that one row, marked.
    /// * **Enter** commits the chosen row ([`Self::commit_provider`]). With no
    ///   row it is not taken, so the line goes on to the router and is refused
    ///   there as upstream's is (`provider_picker_runtime.zig:255-257`).
    /// * **Right**, from the end of the text, is Enter. That is upstream's
    ///   binding and it is kept **only because a commit here has no leg outside
    ///   this machine's settings file and the provider it names**: it writes
    ///   `~/.xfx/settings.json` and, for llmux, asks the loopback daemon what it
    ///   serves -- exactly what Enter does, and nothing that opens a browser or
    ///   starts a sign-in. Upstream's Right on a subscription row starts an
    ///   OAuth flow in the user's browser (`app_auth_runtime.zig:1060-1118`);
    ///   a sign-in row here would have to take Right off before it shipped.
    ///   While a turn runs or a prompt waits Right is only a caret move, as
    ///   upstream's is while a stream runs (`app_input_runtime.zig:288-293`).
    /// * **Escape** closes the column and leaves the text.
    /// * **Left** is not taken: there is no column before this one to step
    ///   back into, so it moves the caret, as upstream's does in its first
    ///   column (`stepBack` answers `false` there, `:381-385`).
    fn offer_to_provider_picker(&mut self, event: &Input) -> bool {
        let Input::Action(action) = event else {
            return false;
        };
        let Some(column) = self.provider_picker.as_mut() else {
            return false;
        };
        match action {
            Action::Up | Action::Down => {
                if column.step(matches!(action, Action::Down)) {
                    self.render.request(Reason::Modal);
                }
                true
            }
            Action::Tab => {
                if let Some(text) = column.completion() {
                    self.replace_word(&text);
                }
                true
            }
            Action::Submit => self.commit_provider(),
            Action::Right if self.editor.caret_at_end() && !self.provider_work_busy() => {
                self.commit_provider()
            }
            Action::Escape => {
                self.provider_picker = None;
                self.provider_dismissed = true;
                // The band just gave the column's rows back to the document.
                self.refit();
                self.render.request(Reason::Modal);
                true
            }
            _ => false,
        }
    }

    /// Enter (or Right) on the `/provider` column, and whether the keystroke
    /// was spent.
    ///
    /// No row: not spent, and the line goes on to the router. A turn running
    /// or a prompt waiting ([`Self::provider_work_busy`]): spent on upstream's
    /// refusal, and the text stays where it is so the same
    /// Return works once the work is done (`reject_provider_picker_if_busy`,
    /// `app_input_runtime.zig:1593-1596`). Otherwise the switch.
    ///
    /// **Not a submitted line**: the composer is cleared and nothing is written
    /// into the prompt history, because upstream's column commits before its
    /// submit path ever sees the text (`app_input_runtime.zig:1593-1596`,
    /// ahead of `submitInput`).
    fn commit_provider(&mut self) -> bool {
        let Some(provider) = self
            .provider_picker
            .as_ref()
            .and_then(ProviderPicker::chosen)
        else {
            return false;
        };
        if self.provider_work_busy() {
            self.write_notice(&notice::Notice::neutral("provider", PROVIDER_BUSY));
            return true;
        }
        self.switch_provider(provider);
        true
    }

    /// Whether a provider switch has to wait: a turn is running, or a prompt
    /// is waiting for one -- upstream's `provider_work_busy(stream.active,
    /// queuedPromptCount())` (`auth_transition.zig:25-27`, asked at
    /// `app_auth_runtime.zig:430-439`).
    ///
    /// **Turns and prompts only, not the runtime's whole queue**
    /// ([`super::worker::WorkHandle::outstanding`]). That queue also holds a
    /// catalog load, a `/model` change and a provider switch, and none of
    /// them is a conversation a switch would cut in two: each runs to its end
    /// before the next piece of work starts, so a switch taken behind one is
    /// simply the next thing the runtime does. The two facts are read where
    /// the band already keeps them: the activity row's work, which a turn sets
    /// at `TurnStarted` and clears at its end, and the prompts sent whose turn
    /// has not started.
    fn provider_work_busy(&self) -> bool {
        self.activity.working() || !self.prompts.is_empty()
    }

    /// Hands a provider switch to the runtime, and says it is under way.
    ///
    /// The one place a switch starts, whether it came from the column or from
    /// `/setup llmux` typed out with the column dismissed. The composer is
    /// emptied only once the runtime has taken the work, so a refusal leaves
    /// the text for the next Return.
    ///
    /// **The provider already in force is no switch**: while a credential it
    /// accepts is configured -- the row's `current` -- choosing it again says
    /// upstream's `Already using <name>.` and hands the runtime nothing, so the
    /// profile is not rewritten and the conversation is not dropped
    /// (`decideProviderSwitch`'s `.no_change`, `auth_transition.zig:29-32`,
    /// said at `app_auth_runtime.zig:1159-1167`). Without such a credential
    /// it is a switch like any other: setup is what records one.
    fn switch_provider(&mut self, provider: ProviderId) {
        if provider == self.provider && !self.missing_credential {
            self.provider_picker = None;
            self.clear_composer();
            self.announce(notice::Notice::neutral(
                "provider",
                format!("Already using {}.", provider.entry().name),
            ));
            return;
        }
        if let Err(rejected) = self.work.submit(TurnWork::Setup(provider)) {
            self.refused(rejected);
            return;
        }
        self.provider_picker = None;
        self.clear_composer();
        // Upstream's word for the interval between the choice and the switch
        // (`beginPreparation`, `app_auth_runtime.zig:1239`): the runtime is
        // about to read the profile and, for llmux, ask the daemon what it
        // serves, and what the session became is said when that is done
        // ([`UiEvent::ProviderSelected`]).
        self.announce(notice::Notice::neutral(
            "provider",
            format!("Preparing {}.", provider.entry().name),
        ));
    }

    /// One keystroke, while a question has the focus.
    ///
    /// **Everything the panel does not bind is swallowed**, and that is the
    /// difference between a panel and a hint: a `1` typed at a question is an
    /// answer, not a character in a composer whose caret is somewhere else, and
    /// a Ctrl-D at one does not end a session that is holding a turn open
    /// waiting to be told what to do.
    ///
    /// **The two bail-out keys are upstream's** (`vercel-labs/fx@c1db919`,
    /// pinned by its e2e `tests/e2e/tui-permissions.test.ts:1099`):
    ///
    /// * **Ctrl-C refuses this call and nothing else** (`src/ui/input/runtime.zig:61`,
    ///   `3 => .deny`). The turn goes on, told no, exactly as if `3` had been
    ///   pressed -- minus whatever the deny draft held, because a key that says
    ///   nothing sends nothing (`approval_decision.zig:132-135`).
    /// * **Escape cancels the turn** (`app_input_runtime.zig:3139-3143`,
    ///   `cancelApprovalOperation`), and it is the whole of why this takes a
    ///   clock: the question is refused *with* the interrupt rather than
    ///   instead of it ([`Self::interrupt`]), because the prompter is the thing
    ///   parked on that channel and it turns a cancellation into a `Deny` and
    ///   hands the cancellation on to the loop that can act on it
    ///   (`super::approval::TuiPrompter`). Answering `Deny` and stopping there
    ///   would leave the user watching the turn they cancelled carry on, with
    ///   the prompt they had queued behind it running next.
    ///
    /// Both are the reverse of what xfx's panel did before (Escape refused the
    /// call, Ctrl-C stopped the turn): divergence 7 of `.prd/tui-phase4/ssot.md`.
    fn decide(&mut self, event: Input, now: Instant) {
        // A notice on the hint row is about the keystroke before this one; the
        // question's own hint takes the row back unless this one earns another
        // ([`Self::notice_at_the_question`]).
        if self.notice.take().is_some() {
            self.render.request(Reason::Footer);
        }
        // The two keys that walk a change too long for one screen, and they are
        // bound **only** where there is one to walk: a bounded diff is up to
        // 128 KiB (`crate::permission::ApprovalDiff`) and a screen is a few
        // dozen rows, so a review surface with no way past its first screenful
        // would be showing the head of a change and calling it the change. They
        // are `C-p` and `C-n` -- "the line before this one" and "the line after
        // it" everywhere else on this surface, and bound to nothing at all at a
        // question until now. The arrows are deliberately not reused: they walk
        // the choices, and a key that scrolled *and* chose would be a key that
        // answers by accident.
        if let Input::Action(Action::HistoryPrevious | Action::HistoryNext) = event {
            let (cols, rows) = (self.geometry.cols, self.geometry.rows);
            if let Some(screen) = self.alternate.as_mut() {
                let delta = if matches!(event, Input::Action(Action::HistoryPrevious)) {
                    -1
                } else {
                    1
                };
                screen.scroll_by(delta, cols, rows);
                self.render.request(Reason::Modal);
                return;
            }
        }
        // Whether an amendment has the keys, taken before anything is
        // translated: it is what turns a `1` from an answer into a character
        // and an editing key from nothing into an edit
        // ([`super::approval_amendment`]).
        let drafting = self.drafting();
        let action = match event {
            Input::Text(character) => approval::Action::Text(character),
            Input::Action(Action::Up) => approval::Action::Up,
            Input::Action(Action::Down) => approval::Action::Down,
            // **`C-p` and `C-n` are the arrows here**, collapsed at the
            // translation rather than given to the panel as two more keys
            // (`input_approval_runtime.zig:127-141` maps `cursor_up` and
            // `history_up` to one action).
            //
            // **A documented difference from that pin, on one surface.**
            // Reached only with no review plane up: on the plane these two
            // keys walk the change itself, and they are the only keys that do
            // -- a bounded diff is up to 128 KiB
            // (`crate::permission::ApprovalDiff`) and a screen is a few dozen
            // rows, so a review surface with no way past its first screenful
            // would be showing the head of a change and calling it the change.
            // The arrows are unaffected on both surfaces: they move the choice
            // and end editing everywhere, which is the property the pin exists
            // for and which
            // `up_and_down_move_the_choice_whether_or_not_a_draft_is_open`
            // asserts in all four spellings. Upstream has no second surface to
            // scroll, so this is a retention rather than a divergence.
            Input::Action(Action::HistoryPrevious) => approval::Action::Up,
            Input::Action(Action::HistoryNext) => approval::Action::Down,
            Input::Action(Action::Tab) => approval::Action::Tab,
            Input::Action(Action::Submit) => approval::Action::Submit,
            Input::Action(Action::Escape) => approval::Action::Escape,
            Input::Action(Action::Cancel) => approval::Action::Cancel,
            // The whole band is repainted every frame, so a redraw is a frame
            // here as much as anywhere else.
            Input::Action(Action::Redraw) => {
                self.external_damage();
                return;
            }
            // The draft's own keys, and **only while a draft has them**: with
            // no amendment open these stay what they have always been at a
            // question, which is swallowed. `Up` and `Down` are deliberately
            // not in the subset ([`approval::edits`]) -- they move the choice
            // and end editing, and routing them into the editor would take away
            // the only way out of a draft.
            Input::Action(editing) if drafting && approval::edits(editing) => {
                approval::Action::Edit(editing)
            }
            // Content, to the **draft's** assembler and never the composer's:
            // a modal surface that leaked paste bytes into the composer would
            // leave text behind the panel that the user never sees and cannot
            // delete. With no draft open they fall through to the swallow
            // below, exactly as they did before amendments existed.
            Input::PasteByte(byte) if drafting => approval::Action::PasteByte(byte),
            Input::Action(_) | Input::PasteByte(_) => return,
            // Answered before the focus is consulted ([`Self::consume`]), for
            // the reason the same arm in [`Self::answer`] gives.
            Input::Report(_) => return,
        };
        // Whichever surface is holding the question, and there is never more
        // than one ([`Self::ask`]). Both answer with the same function
        // (`super::approval::answered`), so which plane a change happened to be
        // large enough for cannot change what a key means.
        let cols = self.geometry.cols;
        let answered = match (self.panel.as_mut(), self.alternate.as_mut()) {
            (Some(panel), _) => panel.apply(action, cols),
            (_, Some(screen)) => screen.apply(action, cols),
            (None, None) => return,
        };
        // Said on whichever plane the user can see, and taken so it is said
        // once ([`Self::notice_at_the_question`]).
        self.notice_at_the_question();
        let (answer, amendable) = match answered {
            // The keystroke meant nothing here. No frame is owed for it: a
            // repaint of an unchanged band is bytes that say nothing.
            approval::Reply::Ignored => return,
            approval::Reply::Moved => {
                // The marker or a draft moved, which is a frame and nothing
                // else: the panel is the same height, so the band's geometry
                // and the receipt about it both still hold.
                self.render.request(Reason::Modal);
                return;
            }
            approval::Reply::Reshaped => {
                // A draft opened, closed or grew, so the panel has a different
                // number of rows: the band has to be re-solved, and the screen
                // a receipt was about is no longer the screen the user is
                // looking at. `invalidate` rather than `intend(None)` because it
                // is the direct name for the effect -- both revoke the seen
                // disclosure as well as the receipt
                // ([`super::approval_readiness::Readiness::invalidate`]) -- and
                // the next genuinely disclosed frame re-earns it.
                self.invalidate_approval();
                self.refit();
                self.render.request(Reason::Modal);
                return;
            }
            approval::Reply::Answer { answer, amendable } => (answer, amendable),
        };
        // **The gate, and only on the two answers that grant.** A `Deny`, an
        // Escape and a Ctrl-C stay answerable at every moment, ready or not:
        // gating them as well would leave a user who cannot read the question
        // with no way to say no to it either, which turns a safety check into a
        // session that cannot be got out of. Ctrl-C is already excluded by the
        // answer it produces -- the prompter turns a cancellation into a
        // refusal.
        if matches!(answer, ApprovalAnswer::Once | ApprovalAnswer::Always)
            && !self.asked_id.is_some_and(|id| self.approval_ready(id))
        {
            // **Said, not swallowed**, and which sentence it is matters: waiting
            // fixes one of these and only a bigger window fixes the other.
            let notice = if self.readiness.undisclosed() {
                APPROVAL_NOT_DISCLOSED
            } else {
                APPROVAL_NOT_READY
            };
            // **To whichever surface the user can actually see.** While the
            // review plane is up it owns every row of the terminal, and
            // [`Self::say`] writes into the document -- which is on the primary
            // buffer, behind it. A refusal written there is a refusal nobody can
            // read, and a key that appears to do nothing is a key the user
            // presses again.
            match self.alternate.as_mut() {
                Some(screen) => screen.set_status(notice.to_string()),
                None => self.announce(notice::Notice::warning("permissions", notice)),
            }
            // The question stays up and the keystroke is spent: nothing is
            // queued for a later grant, because an answer held until a frame
            // lands would be a grant given by the renderer rather than by the
            // person at the keyboard.
            self.render.request(Reason::Modal);
            return;
        }
        // **Past the gate, and only past it.** The amendment is taken here
        // rather than where the answer was produced, because a refused
        // affirmative leaves the question standing -- and a draft consumed on
        // the way to a refusal would be a sentence the user typed, cannot see
        // any more, and never sent. Escape and the interrupt take nothing and
        // drop both: `amendable` is what says which of the two refusals this is
        // ([`approval::Reply`]).
        let feedback = match (self.panel.as_mut(), self.alternate.as_mut()) {
            (Some(panel), _) => panel.take_feedback(answer, amendable),
            (_, Some(screen)) => screen.take_feedback(answer, amendable),
            (None, None) => None,
        };
        // The panel goes **before** anything is sent, so the band's next paint
        // is a band with no question in it whatever the runtime does next --
        // including asking a second question straight away. The screen goes back
        // with it, in the same statement and on every one of the four ways out
        // -- a digit, Enter on the marked choice, Escape, and the interrupt
        // below -- because an owner released on only some of them would be an
        // owner released on the paths somebody remembered.
        self.panel = None;
        self.alternate = None;
        // The identity the answer is addressed to, taken in the same statement
        // the surfaces are. `debug_assert`ed rather than defaulted: [`Self::ask`]
        // installs the two together and every exit clears them together, so a
        // question being answered without an id is a broken invariant and not a
        // case with a sensible answer -- and an id invented here would be an id
        // some other question could be waiting on.
        let asked = self.asked_id.take();
        debug_assert!(
            asked.is_some(),
            "a question was answered without the id it was asked under"
        );
        // The receipt was about a screen with this question on it, and the
        // question is coming off it.
        self.invalidate_approval();
        self.owner = ScreenOwner::Primary;
        match action {
            // Escape: one message, both meanings. `Deny` is what the prompter
            // answers a cancellation with on the far side, so sending
            // `Answer(Deny)` here as well would be the *only* thing the runtime
            // heard -- and the turn this question belongs to, and whatever was
            // queued behind it, would go on running after the user asked for it
            // to stop. **One message still**, and it carries no sentence: the
            // drafts were dropped above, so a cancel at a filled draft says
            // nothing on either channel.
            //
            // **The call this panel was asking about is stopped by this key**:
            // the prompter answers the stop by refusing it and handing the
            // interrupt on, so the call never runs (`super::approval`'s
            // `TuiPrompter`). The group says so now, above the sentence that
            // answers the key, as upstream's does the moment it is pressed
            // (`fx-approvals-cc-40x120-after-esc`); the refusal the runtime
            // reports for it next is the same stop in the tool's words, and is
            // dropped ([`Self::abandoned`]). That call's own `■ Cancelled …`
            // line is the turn's, so no second one is written for the turn
            // (`append_turn_cancellation`, `transcript/runtime.zig:5378-5383`).
            //
            // **No exit is armed by it**: upstream's `cancelApprovalOperation`
            // stops the turn and leaves the hint row the session's
            // (`fx-approvals-cc-40x120-after-esc`), because the Escape was an
            // answer to the question rather than half of a way out.
            approval::Action::Escape => {
                let stopped = self.settle_group();
                self.cancel_turn_with(stopped);
                self.settle_band(now);
            }
            // Ctrl-C and the rest are an answer about *this call* and nothing
            // more: the turn goes on, and is told no -- with whatever the user
            // said about it, which is context and never authority.
            _ => {
                if let Some(id) = asked {
                    self.work.control(TurnControl::Answer {
                        id,
                        answer,
                        feedback,
                    });
                }
            }
        }
        // The band gives the panel's rows back to the document. The clock
        // starts again on the next settle rather than here, for the reason
        // every other timed answer on this surface is settled there: what is
        // painted has to be the same reading that asked for the frame
        // ([`Self::tick_activity`]).
        self.refit();
        self.render.request(Reason::Modal);
    }

    /// Puts a batch of the model's questions in front of the user, or cancels
    /// it on their behalf.
    ///
    /// The refusal is the same rule the approval panel's is -- a decision xfx
    /// was never given is not one it may invent -- said in **its own sentence**
    /// ([`QUESTION_TOO_SMALL`]) rather than the permission panel's: a batch
    /// whose choices were below the last row of the screen would leave the turn
    /// waiting for an answer the user cannot give, and a batch that is cancelled
    /// grants nothing, denies nothing and changes nothing. The tool turns the
    /// cancellation into its own sentinel and the model is told to ask in
    /// freeform instead.
    fn ask_question(&mut self, request: QuestionRequest) {
        let panel = QuestionPanel::new(request);
        if !panel.presents_choices(self.geometry.cols, self.geometry.rows) {
            // **Before anything is installed**, mirroring [`Self::ask`]: a
            // question that was never asked has no rows to give back, and a band
            // solved for a panel nobody can answer is a band the composer has
            // lost rows to for the rest of the turn.
            self.announce(notice::Notice::cancelled("question", QUESTION_TOO_SMALL));
            self.work
                .control(TurnControl::QuestionCancelled { id: panel.id() });
            return;
        }
        // The menu and the question share one slot and the question owns the
        // focus, so the menu goes **before** the question is installed rather
        // than being left for the geometry to prefer away: a menu still open
        // behind a question is a menu whose keys the panel is swallowing.
        self.dismiss_picker();
        self.ask = Some(panel);
        // The band just grew by the panel's rows, so the divider, the composer
        // and the caret are all somewhere else.
        self.refit();
        self.render.request(Reason::Modal);
        // The model is waiting on the user (`app_worker_runtime.zig:1089-1097`).
        self.sound.attention();
    }

    /// One keystroke, while a batch of the model's questions has the focus.
    ///
    /// **Everything the panel does not bind is swallowed**, exactly as at a
    /// permission question: a `1` is an ordinal rather than a character in a
    /// composer the caret has left, and a Ctrl-D at one does not end a session
    /// that is holding a turn open waiting to be answered.
    ///
    /// **Escape and Ctrl-C both cancel the turn the batch belongs to**, as
    /// upstream's do (`vercel-labs/fx@c1db919`): Ctrl-C is the question's own
    /// `cancel` byte (`src/ui/input/runtime.zig:138`, `3 => .cancel`, answered
    /// `.cancelled` at `question_prompt.zig:327`), and Escape at a field with
    /// nothing in it reaches the same function (`app_input_runtime.zig:3137`);
    /// both are `cancelQuestionPrompt` (`input_question_runtime.zig:59-83`),
    /// which stops the worker's turn and answers the batch with nothing. The
    /// model is never told the user declined the question -- the call answers
    /// `aborted by user` (`crate::tools::question::ABORTED_OUTPUT`) -- and
    /// upstream's e2e pins exactly that (`tui-decision-prompts.test.ts:2454`).
    /// Divergence 7 of `.prd/tui-phase4/ssot.md`, which reverses xfx's old
    /// meaning (Escape declined the question and the turn went on).
    ///
    /// **One message, and it is the turn's** ([`Self::interrupt`]): the
    /// requester parked on this channel reads the interrupt as its question
    /// being over, answers the tool as interrupted, and hands the interrupt
    /// **back** to the loop that can stop the turn
    /// (`super::question::TuiQuestioner`). A `QuestionCancelled` in front of it
    /// would be the question declined -- the sentinel, and the calls behind the
    /// question run before anything had read the interrupt.
    ///
    /// **Whichever way the batch ends, the document keeps a record of it**,
    /// as upstream's does (`finalizeQuestionTranscript`,
    /// `input_question_runtime.zig:120-136`): every question with its answer
    /// hung under it, or `■ Cancelled` ([`super::question`]'s
    /// `answered_rows`, `cancelled_row`). Behind the stream rather than at
    /// once ([`Self::mark`]), because a question can be answered while the
    /// text the model wrote before asking it is still being released, and the
    /// record belongs after that text.
    ///
    /// **Escape at a draft with text in it arms rather than cancels**, as it
    /// does at the composer (`app_input_runtime.zig:3120-3135`): a second
    /// inside the window empties the draft, and only an Escape at an empty
    /// one cancels -- so a sentence half typed is not thrown away, with the
    /// batch, by one key.
    fn answer(&mut self, event: Input, now: Instant) {
        let act = match event {
            Input::Text(character) => question::Act::Text(character),
            // The whole band is repainted every frame, so a redraw is a frame
            // here as much as anywhere else.
            Input::Action(Action::Redraw) => {
                self.external_damage();
                return;
            }
            // Which keys a question binds is the question's own fact
            // ([`question::Act::of`]), so a key added to one is not a key this
            // module has to be told about twice.
            Input::Action(action) => match question::Act::of(action) {
                Some(act) => act,
                None => return,
            },
            // Content, to the question's own answer while it is open -- the
            // panel drops it whole while a choice is marked -- and never to the
            // composer, which is not what has the focus.
            Input::PasteByte(byte) => question::Act::PasteByte(byte),
            // Not a key at all, and never routed here: [`Self::consume`]
            // answers a theme report before the focus is consulted, precisely
            // so that a question having the keyboard cannot swallow one. The
            // arm exists because the match is exhaustive, and it returns
            // because a report reaching it would mean the routing above had
            // been taken apart.
            Input::Report(_) => return,
        };
        let (cols, rows) = (self.geometry.cols, self.geometry.rows);
        let Some(ask) = self.ask.as_mut() else {
            return;
        };
        let id = ask.id();
        if act == question::Act::Escape && ask.holds_draft() {
            // The composer's own window and its own warning on the hint row
            // ([`super::gesture`], [`ESCAPE_ARMED`]): one gesture, wherever
            // the draft it clears is.
            // The clear's window, not the interrupt's: the draft is what
            // this Escape is about, as at an idle composer.
            if self.gestures.escape(now, false, true) == Escape::Clear && ask.clear_draft() {
                self.settled_question();
            }
            self.settle_band(now);
            return;
        }
        match ask.apply(act, cols, rows) {
            // A keystroke that changed nothing must not repaint the whole band.
            question::Answered::Nothing => {}
            // **Re-solved, not merely repainted.** The panel's height is not
            // fixed: opening the freeform slot adds its draft row and leaving it
            // takes the row back, so a keystroke that only asked for a frame
            // would paint a block one row taller than the band solved for -- and
            // the caret, which is read out of the geometry, would be reported on
            // the divider.
            question::Answered::Redraw => self.settled_question(),
            question::Answered::Submitted(answers) => {
                // Composed from the panel before it goes: the questions are
                // its, and the rows are wrapped at the screen they will land on.
                let record = ask.answered_rows(&answers, cols, &self.palette);
                // **The panel goes before anything is sent**, so the band's next
                // paint has no question in it whatever the runtime does next --
                // including asking a second batch straight away. The order
                // [`Self::decide`] uses, for the same reason.
                self.ask = None;
                self.mark(Mark::Block(record));
                self.work
                    .control(TurnControl::QuestionAnswer { id, answers });
                self.settled_question();
            }
            // Escape or Ctrl-C: the turn's own stop, and **only** it.
            question::Answered::Cancelled => {
                self.ask = None;
                self.mark(Mark::Block(vec![(
                    question::cancelled_row(&self.palette),
                    DocumentRow::Plain,
                )]));
                self.settled_question();
                // **The question's `■ Cancelled` is the turn's line**, and
                // nothing follows it: upstream's `cancelQuestionPrompt` records
                // the question cancelled and stops the turn, with no line of
                // the turn's own after it and no exit armed -- the key was the
                // question's (`input_question_runtime.zig:59-80`;
                // `receipts/f3/fx-question-…-q2-esc`).
                self.cancel_turn_with(true);
                self.settle_band(now);
            }
        }
    }

    /// What the band owes whenever a question changed shape or left it: the
    /// rows re-solved, and the frame that moves everything below them.
    fn settled_question(&mut self) {
        self.refit();
        self.render.request(Reason::Modal);
    }

    /// Adds answer text to the stream, where the pacer releases it.
    fn stream(&mut self, text: &str) {
        // Upstream's phase moves to generating on the first content chunk
        // (`assistant_stream.zig:235-239`).
        if self.activity.working() {
            self.activity.set(Work::Generating);
        }
        // Counted as it arrives rather than as it is shown, as upstream counts
        // content (`telemetry.zig:116-120`): a summary written while the pacer
        // still holds the tail would otherwise leave that tail out.
        self.tally.streamed.consume(text);
        self.enqueued = self.enqueued.saturating_add(text.len());
        self.pacer.enqueue(text);
    }

    /// Says one of xfx's own lines, in the place the stream has reached.
    ///
    /// Immediately when nothing is waiting, which is every keystroke's case and
    /// most of a session's: a refusal, an echo or a `/help` that queued behind
    /// an answer would arrive after it. Behind the stream when something *is*
    /// waiting, because then the place a line belongs is not the end of the
    /// document -- it is the point the answer had reached when the line
    /// happened, and Phase 1 never goes back to insert one.
    ///
    /// Two lines do **not** come through here, and they are a pair: the
    /// cancelled line and the sentence saying the queue went with it
    /// ([`Self::cancel_turn`]). Both are what a Ctrl-C is answered with, so both
    /// are about the keystroke rather than about the answer, and the reason is
    /// written where they are.
    fn say(&mut self, line: String) {
        self.say_as(line, Role::Notice);
    }

    /// The same, for a line that is not xfx's own words.
    fn say_as(&mut self, line: String, role: Role) {
        self.mark(Mark::Line(line, role));
    }

    /// Says one of xfx's own notices, in the place the stream has reached --
    /// [`Self::say`]'s timing, upstream's shape ([`Self::write_notice`]).
    ///
    /// Everything xfx says about the session goes through here or, for the
    /// few sentences that answer a keystroke at once, straight to
    /// [`Self::write_notice`]. What still goes through [`Self::say`] is the
    /// tool trail -- `[tool] …` and the `[you] …` amendment that belongs to a
    /// tool's decision -- whose rendering is the tool rows' own unit.
    fn announce(&mut self, notice: notice::Notice) {
        self.mark(Mark::Notice(notice));
    }

    /// Records a document write at the stream position it was issued at.
    fn mark(&mut self, mark: Mark) {
        if self.pacer.pending() == 0 && self.marks.is_empty() {
            self.run_mark(mark);
            return;
        }
        self.marks.push_back((self.enqueued, mark));
    }

    /// Releases what this moment of the clock is worth, and runs whatever that
    /// carried the stream past.
    ///
    /// Once a turn, from [`Self::settle_band`], beside the two other answers
    /// only the passage of time produces. It reads no terminal and writes none:
    /// what it does is put text into the transcript and ask for the frame that
    /// owes.
    fn pace(&mut self, now: Instant) {
        let before = self.pacer.pending();
        if let Some(chunk) = self.pacer.tick(now) {
            let consumed = before.saturating_sub(self.pacer.pending());
            self.release(chunk, consumed);
        }
        self.run_due_marks();
    }

    /// Puts one emission into the document, **stopping at every mark it
    /// crossed**.
    ///
    /// One release is one tick's worth of bytes, and a tick's worth is a number
    /// the clock chose -- so it lands wherever it lands, including past the
    /// point a tool notice or a turn's conclusion belongs. Writing the whole of
    /// it and then running the marks would put those lines a few characters
    /// late: after the first word of the sentence that was supposed to follow
    /// them. The bytes are therefore written in pieces, each piece ending where
    /// the next mark falls due.
    ///
    /// `consumed` counts the **queue** bytes this emission carried, which is
    /// not `chunk.len()`: an emission may be prefixed with the attributes the
    /// last one left open, and those were never in the queue and have no
    /// position in it.
    fn release(&mut self, chunk: String, consumed: usize) {
        let (reopen, mut body) = chunk.split_at(chunk.len().saturating_sub(consumed));
        let mut reopen = reopen.to_string();
        loop {
            self.run_due_marks();
            if body.is_empty() {
                return;
            }
            // How far this piece may go. Zero is impossible: it would mean a
            // mark at the position already reached, and the line above has just
            // run every one of those.
            let room = self
                .marks
                .front()
                .map_or(body.len(), |(at, _)| at.saturating_sub(self.emitted));
            let (piece, tail) = body.split_at(room.min(body.len()));
            let mut text = std::mem::take(&mut reopen);
            text.push_str(piece);
            self.emitted = self.emitted.saturating_add(piece.len());
            // The answer, presented: the queue, the marks and the offsets above
            // are the raw stream, untouched, and what the screen gets is what
            // the markdown presenter makes of it.
            self.write_answer(&text);
            body = tail;
        }
    }

    /// Puts everything still waiting into the document at once.
    ///
    /// The exit's, and it is a **contract rather than tidiness**: this module
    /// holds text the runtime has already produced, Phase 1 never repaints a
    /// document row, and a band that came down over a full pacer would have
    /// eaten the end of the answer the user was reading. Called on every way
    /// out -- from the drain, so an interrupted turn's tail is painted as it is
    /// taken, and once more after the drain, for the session that had nothing
    /// left to drain and a queue to empty anyway.
    pub(crate) fn flush_paced(&mut self) {
        let before = self.pacer.pending();
        if let Some(chunk) = self.pacer.drain() {
            // Through the same splitter a tick's release goes through, so an
            // exit that writes a whole answer at once still puts the notices
            // and the conclusions inside it where they belong rather than all
            // of them at the end.
            self.release(chunk, before);
        }
        self.run_due_marks();
        // And the presenter's own hold: the line it has not seen the end of yet
        // is the end of the answer when there is no more stream to come.
        self.close_answer();
    }

    /// Runs the marks the stream has now reached.
    fn run_due_marks(&mut self) {
        while self
            .marks
            .front()
            .is_some_and(|(at, _)| *at <= self.emitted)
        {
            let Some((_, mark)) = self.marks.pop_front() else {
                return;
            };
            self.run_mark(mark);
        }
    }

    /// One of them.
    fn run_mark(&mut self, mark: Mark) {
        // Anything that is not the answer ends the answer's block. A notice
        // ends only the rows already written: the line the presenter is still
        // holding goes on under it ([`Self::write_notice`]).
        if !matches!(mark, Mark::Notice(_)) {
            self.close_answer();
        }
        // After the answer's close, which is the document's order: what the
        // presenter was holding is written -- and recorded
        // ([`Self::present`]) -- before the mark's own write.
        self.record(&mark);
        match mark {
            Mark::Card(text) => self.write_card(&text),
            Mark::Summary(line) => {
                self.write_document_line(&line, Role::Dim);
                self.end_transcript_line();
            }
            // One of xfx's own lines rides under an open tool group rather
            // than closing it ([`Self::trail_group`]).
            Mark::Line(line, role) if self.group.drawn() => {
                self.trail_group(Trailed::Line(line, role));
            }
            Mark::Line(line, role) => self.write_document_line(&line, role),
            Mark::Notice(notice) => self.write_notice(&notice),
            Mark::Block(rows) => self.write_block(&rows),
            Mark::EndOfLine => self.finish_document_line(),
            Mark::ToolStart { id, shown } => {
                self.group.start(id, shown);
                self.redraw_group();
            }
            Mark::ToolResult {
                id,
                ok,
                detail,
                outcome,
                changes,
            } => {
                if !self.abandoned.contains(&id)
                    && self.group.finish(&id, ok, &detail, outcome, changes)
                {
                    self.redraw_group();
                }
            }
            Mark::TurnOver => {
                let _ = self.settle_group();
                self.abandoned.clear();
            }
        }
    }

    /// Hands the transcript the group's block as it now stands, when it
    /// changed.
    ///
    /// A group that has no block yet starts one on a row of its own
    /// ([`Transcript::queue_block`] ends an open line first); one that has is
    /// rewritten where it is, because it is still the document's last thing --
    /// every other write closes it first ([`Self::close_group`]).
    fn redraw_group(&mut self) {
        let trailer = self.trailer_rows();
        let Some(rows) = self.group.redraw(self.palette, &trailer) else {
            return;
        };
        // What each row is, recorded with it for a theme report: the group's
        // own rows by the group, and the lines written under it plain.
        let mut kinds = self.group.kinds();
        kinds.resize(rows.len(), DocumentRow::Plain);
        if self.transcript.queue_block(rows, kinds) {
            self.owed();
        }
    }

    /// Closes the group's block, so that what comes next is written under it,
    /// and says whether there was one to close.
    ///
    /// Called by every write that is not the group's own -- the answer's text,
    /// a line of xfx's, a card -- through the two places they all pass
    /// ([`Self::finish_document_line`], [`Self::write_transcript_as`]). That
    /// is upstream's rule, said in terms of the one thing this transcript can
    /// do: a group is broken by the next thing shown that is not one of its
    /// calls (`tool_group_projection.zig:1280-1375`), and a block that is not
    /// the document's last thing can no longer be rewritten.
    ///
    /// A call still running when its block closes keeps its row as the block
    /// left it, and is drawn again -- under whatever closed it -- when its
    /// result arrives ([`ToolGroup::unsettled`]).
    fn close_group(&mut self) -> bool {
        if !self.group.drawn() {
            return false;
        }
        self.group = std::mem::take(&mut self.group).unsettled();
        // What was written under it went with the block, which keeps it.
        self.trailer.clear();
        // Straight to the transcript: the block ends as a line does, and its
        // last row is already the blank one that separates it from what
        // follows -- or the last of the lines written under it.
        if self.transcript.queue_end_line() {
            self.owed();
        }
        true
    }

    /// Stops what the group still has running, shows it stopped, and closes
    /// the group -- for a turn that is over, whichever way it ended.
    ///
    /// A call with no result when its turn ends did not finish: the runtime
    /// sends a result for every call that returned, and what ends a turn
    /// between a call's start and its result is a stop -- once the runtime
    /// has read one, its stream goes silent (`super::bridge`'s `send_ui`). So
    /// it is shown as upstream shows a call the user stopped: `Cancelled`,
    /// counted, with its own line under the group.
    ///
    /// Says whether it stopped anything -- that is, whether a call's own
    /// `■ Cancelled …` line is now in the document.
    fn settle_group(&mut self) -> bool {
        self.full.settle();
        let stopped = self.group.cancel_running();
        let any = !stopped.is_empty();
        if any {
            self.redraw_group();
        }
        self.abandoned.extend(stopped);
        self.close_group();
        self.group = ToolGroup::default();
        any
    }

    /// How much answer text is waiting to be released.
    ///
    /// Read by the loop, which stops taking `UiEvent`s while it is at
    /// [`PACED_BACKLOG`]. That is where the bound on this queue lives: the
    /// channel fills behind a UI that has stopped listening, the runtime parks
    /// in its `send().await`, and the socket feels it -- rather than a `String`
    /// here growing to the length of the answer.
    pub(crate) fn paced_backlog(&self) -> usize {
        self.pacer.pending()
    }

    /// Why the session is ending, when the runtime is why.
    pub(crate) fn fatal(&self) -> Option<&str> {
        self.fatal.as_deref()
    }

    /// Puts one whole line into the document, on rows of its own.
    ///
    /// A notice must not land in the middle of a sentence, so whatever the
    /// answer had open is closed first.
    fn write_document_line(&mut self, line: &str, role: Role) {
        self.close_answer();
        self.finish_document_line();
        self.write_transcript_as(line, role);
        self.end_transcript_line();
    }

    /// Puts one notice into the document, as upstream renders one
    /// (`renderSemanticNotice`, `transcript_blocks.zig:1758-1800`):
    ///
    /// * `<glyph> <topic>: <body>`, the topic and its colon dropped when there
    ///   is no topic -- the glyph and its space are the first row's gutter
    ///   ([`Gutter::Head`]), the topic is [`Role::Label`] and the body is
    ///   [`Role::Notice`];
    /// * every row after the first two columns in, whether the first line
    ///   wrapped onto it or the body broke onto it;
    /// * **one blank row above and one below**, and two notices in a row share
    ///   the one between them (`fx-basic-40x120-version-trailing-space.txt`):
    ///   the row above is written only when the document does not already end
    ///   on a blank one ([`Transcript::ends_blank`]), so neither a fresh screen
    ///   nor a card -- which ends on its own blank row -- grows a second.
    ///
    /// The block the answer has **written** is closed first, on its blank row,
    /// so a notice does not land in the middle of a paragraph's rows. What the
    /// presenter is still **holding** -- a line whose end has not arrived, or a
    /// finished one a setext underline could still make a heading -- stays
    /// held, and is written under the notice once the presenter lets it go,
    /// whole and wrapped as if no notice had come. That is upstream's order
    /// and the reason for it: its presenter keeps the unfinished line in its
    /// own buffer (`assistant_presentation.zig:207-217`) and gives it up only
    /// at a tool's start, a restart or the stream's end
    /// (`assistant_stream.zig:135-140`, `:300-312`), while a notice is an entry
    /// appended to the transcript beside it (`appendSemanticNoticeAtomic`,
    /// `store.zig:1343-1390`) that asks the presenter for nothing
    /// (`fx-notice-midstream-40x120-end.full`). Finishing the presenter here
    /// wrote half a line, the notice, and the rest as a second paragraph --
    /// `word` closing one row and `111` opening the next.
    ///
    /// **Under an open tool group it is one of the group's last rows**
    /// ([`Self::trail_group`]), and the group goes on growing above it.
    fn write_notice(&mut self, notice: &notice::Notice) {
        self.full.notice(notice);
        if self.group.drawn() {
            self.trail_group(Trailed::Notice(notice.clone()));
            return;
        }
        self.close_prose();
        self.finish_document_line();
        if queue_notice(&mut self.transcript, notice) {
            self.owed();
        }
    }

    /// Puts one of xfx's own lines under the open tool group, as the group's
    /// last rows rather than as rows after it.
    ///
    /// **One group per turn, broken by text and by nothing else**: upstream
    /// gives every call of a turn's active group one presentation id
    /// (`tool_presentation.zig:667-708`) and draws them all at the group's
    /// anchor (`tool_group_projection.zig:1294-1319`), so a line written while
    /// the group is open -- what the user said at an approval, a notice, the
    /// interrupt's sentence -- lands under the group while the group's later
    /// calls are still added above it (`fx-approvals-40x120-cmd2`: the fourth
    /// call is counted and drawn over `┃ use capitals`). The document here is
    /// append-only past its open block, so the line is made part of the block:
    /// laid out exactly as it would have been written ([`Self::trailer_rows`])
    /// and rewritten with the block until it closes.
    fn trail_group(&mut self, line: Trailed) {
        self.trailer.push(line);
        self.redraw_group();
    }

    /// The lines under the open group, laid out at this screen's width as the
    /// document would lay them out -- by a transcript of their own, through
    /// the same operations ([`queue_line`], [`queue_notice`]), so a notice's
    /// head, its wrap, its blank rows and the line's colours are the ones it
    /// would have had written on its own.
    fn trailer_rows(&self) -> Vec<String> {
        if self.trailer.is_empty() {
            return Vec::new();
        }
        // The group ends on a blank row, which is the row above the first of
        // these: a fresh transcript starts out "ending blank" for exactly that.
        let mut scratch = Transcript::new(self.geometry.cols);
        for line in &self.trailer {
            match line {
                Trailed::Line(text, role) => queue_line(&mut scratch, text, *role),
                Trailed::Notice(notice) => {
                    queue_notice(&mut scratch, notice);
                }
            }
        }
        let mut rows: Vec<String> = Vec::new();
        while scratch
            .emit_front(self.palette, |append| {
                // Each append rewrites the rows it does not scroll in, at the
                // bottom of what is already there ([`Append`]).
                let settled = append.rows.len() - append.scroll.min(append.rows.len());
                rows.truncate(rows.len().saturating_sub(settled));
                rows.extend(append.rows.iter().cloned());
                Landed::<std::convert::Infallible>::All
            })
            .is_some()
        {}
        rows
    }

    /// Puts rows that are already laid out into the document as one block:
    /// a blank row above it unless the document already ends on one, each row
    /// on a row of its own, and a blank row under it -- the spacing a notice
    /// gets ([`Self::write_notice`]), and the one upstream's question summary
    /// sits in (`fx-question2-40x120-answered`: the card, the summary, a blank
    /// row, the answer).
    ///
    /// The rows are written as they are, with no gutter: their indents are
    /// part of them, because an answer's hangs five columns in, under the
    /// question's text, and the document has no gutter that wide. Each is
    /// recorded as what it is ([`Transcript::queue_kind`]), for a theme report
    /// that later repaints it.
    fn write_block(&mut self, rows: &[(String, DocumentRow)]) {
        self.close_answer();
        self.finish_document_line();
        if self.transcript.queue_gutter(Gutter::None) {
            self.owed();
        }
        if !self.transcript.ends_blank() {
            self.end_transcript_line();
        }
        let mut current = DocumentRow::Plain;
        for (row, kind) in rows {
            if *kind != current {
                current = *kind;
                if self.transcript.queue_kind(current) {
                    self.owed();
                }
            }
            self.write_transcript_as(row, Role::Plain);
            self.end_transcript_line();
        }
        if current != DocumentRow::Plain && self.transcript.queue_kind(DocumentRow::Plain) {
            self.owed();
        }
        self.end_transcript_line();
    }

    /// Ends the document's current line, if there is one open.
    ///
    /// The guard is the difference between "end the line" and "leave a blank
    /// row": a transcript already at the start of a line has no unfinished row,
    /// and ending one there answers with a blank row of its own -- which is
    /// right for two breaks in an answer and wrong for two notices in a row.
    ///
    /// Asked of the transcript as it will be once everything queued has landed
    /// ([`Transcript::open_line`]) rather than as the terminal has it. The two
    /// differ exactly when a notice is written while an answer is still waiting
    /// for a screen -- which is the case this guard is for -- and the committed
    /// answer there would say "no line is open" about an answer that has one,
    /// and put the notice on the end of its sentence.
    fn finish_document_line(&mut self) {
        if self.close_group() {
            return;
        }
        if self.transcript.open_line() {
            self.end_transcript_line();
        }
    }

    /// Whether the session is on its way out.
    pub(crate) fn leaving(&self) -> bool {
        self.leaving.is_some()
    }

    /// What the process exits with, once the session is leaving.
    ///
    /// Asked of the shell rather than decided by the loop because the loop does
    /// not know *why* it is leaving, and the two reasons exit differently: a
    /// Ctrl-D and a `/quit` are a session that finished, and a second Ctrl-C is
    /// a session the user stopped. A caller reading `$?` is entitled to tell
    /// them apart, which is what 130 has meant since job control.
    pub(crate) fn exit_code(&self) -> ExitCode {
        match self.leaving {
            Some(Leaving::Interrupted) => ExitCode::from(INTERRUPTED_EXIT_CODE),
            Some(Leaving::Quit) | None => ExitCode::SUCCESS,
        }
    }

    /// Ends the session at the end of this turn of the loop.
    pub(crate) fn leave(&mut self) {
        self.leave_by(Leaving::Quit);
    }

    /// The line that names `id` as the session to come back to
    /// ([`resume_handoff`]), in this session's palette and at the width the
    /// terminal last reported -- upstream measures it against its own layout's
    /// columns (`resumeHandoffColumns`, `main.zig:918-920`).
    pub(crate) fn resume_handoff(&self, id: &crate::session::SessionId) -> String {
        resume_handoff(id.as_str(), self.screen.1, self.palette)
    }

    /// Ends the session, keeping the first reason it was given.
    ///
    /// First rather than last: an interrupted session that then reaches a
    /// `Fatal` on the way out is still an interrupted one, and a second reason
    /// arriving during the drain must not overwrite the one the user caused.
    fn leave_by(&mut self, why: Leaving) {
        self.leaving.get_or_insert(why);
    }

    /// Decodes `bytes` -- in the order the terminal delivered them -- and does
    /// what they mean.
    ///
    /// One [`Instant`] for the whole read rather than one per byte: the bytes
    /// of a single read arrived together, and the only thing the decoder times
    /// is how long a bare `ESC` has been alone, which a burst's own bytes must
    /// not be able to expire.
    pub(crate) fn route_bytes(&mut self, bytes: &[u8]) {
        let now = Instant::now();
        let mut events = Vec::new();
        for byte in bytes {
            self.decoder.feed(*byte, now, &mut events);
        }
        self.consume(events, now);
    }

    /// Resolves what only the passage of time resolves: a bare `ESC` that has
    /// gone quiet is the Escape key.
    ///
    /// Called once a turn, which is what makes [`super::input::Decoder`]'s
    /// timeout mean 50 ms rather than "until the next keystroke".
    pub(crate) fn settle_input(&mut self, now: Instant) {
        let mut events = Vec::new();
        self.decoder.flush(now, &mut events);
        self.consume(events, now);
    }

    /// Resolves what the band says about state nothing typed here changed: the
    /// queue's depth, and an armed Escape whose window has closed.
    ///
    /// Once a turn, beside [`Self::settle_input`] and for the same reason --
    /// both are answers that only arrive with the passage of time. Reading the
    /// count here rather than inside [`Self::band_rows`] is what makes the row
    /// and the frame that shows it agree: the frame is asked for by the change,
    /// so a change nobody asked a frame for would sit unpainted until the next
    /// keystroke, and a row painted from a fresher read than the one that
    /// triggered it would show a number no frame was owed for.
    pub(crate) fn settle_band(&mut self, now: Instant) {
        // A copy finishing is another answer only the other thread produces.
        self.settle_copies();
        // And so is a `/sound` save.
        self.settle_saves();
        let queued = self.work.queued();
        if queued != self.queued {
            self.queued = queued;
            self.render.request(Reason::Footer);
        }
        // **Nothing in hand is nothing that can still answer a cancel.** The
        // runtime reads a cancellation at the top of its loop before it takes
        // the next item (`super::worker`'s `turn_loop`, `biased`), so a prompt
        // the cancel covered can be dropped without ever saying it started --
        // and a cancel that landed after its turn had already concluded is
        // answered by nothing at all. Either way the place is given back, and
        // once none is held, no event about the cancelled work is coming.
        if self.cancelling && self.work.outstanding() == 0 {
            self.cancelling = false;
            self.cancel_owed = None;
            self.cancel_covers = 0;
        }
        let armed = self.gestures.escape_armed(now);
        if armed != self.escape_armed {
            self.escape_armed = armed;
            self.render.request(Reason::Footer);
        }
        // The two warnings a keystroke arms and the clock takes back: the
        // exit after a Ctrl-C, and the interrupt after an Escape over running
        // work. Read with the decision's own windows, so the row goes back to
        // the session's facts on the tick the keystroke stops doing it.
        let exit = self.gestures.exit_armed(now);
        if exit != self.exit_armed {
            self.exit_armed = exit;
            self.render.request(Reason::Footer);
        }
        let interrupt = self.gestures.interrupt_armed(now);
        if interrupt != self.interrupt_armed {
            self.interrupt_armed = interrupt;
            self.render.request(Reason::Footer);
        }
        // After the Escape: an armed one takes the slot the warning is in, and
        // the warning's clock must not run on a row that is not showing it.
        self.tick_warning(now);
        // What the turn is doing, on the row above the divider. Before the
        // pacer, because the row's arrival and departure move the divider and
        // an append measured against the other band would be placed a row out.
        self.tick_activity(now);
        // And the answer itself: the pacer holds text against a clock, so a
        // turn of the loop is what releases it. Last, so the rows it adds are
        // measured against a band this turn has already settled.
        self.pace(now);
        // Once nothing is left to present, a turn that ended is a bell
        // (upstream's `presentationFinished`): the reader has reached the end
        // the provider reached a while ago.
        if self.pacer.pending() == 0 && self.marks.is_empty() {
            self.sound.presented();
        }
    }

    /// Says how each `/sound` save that has finished went -- only when it
    /// failed, as upstream reports a commit with `announce_commit=false`
    /// (`app_commands.zig:4005-4021`): the level was said when it was typed.
    fn settle_saves(&mut self) {
        while let Some(saved) = self.sound.saved() {
            if let Err(reason) = saved.outcome {
                self.announce(notice::Notice::error(
                    crate::sound::TOPIC,
                    crate::sound::not_saved(saved.changed, &reason),
                ));
            }
        }
    }

    /// The terminal bells the session owes, taken by the loop that writes them
    /// (`super::event_loop`'s `commit_bells`).
    pub(crate) fn take_bells(&mut self) -> u16 {
        self.sound.take()
    }

    /// Bells the terminal refused without taking a byte, owed again.
    pub(crate) fn restore_bells(&mut self, rings: u16) {
        self.sound.restore(rings);
    }

    /// Says how each `/copy` that has finished went, in the order they
    /// finished.
    ///
    /// Once a turn, from [`Self::settle_band`]: a copy runs on its own thread
    /// ([`super::copy`]), so its outcome is a fact that arrives rather than one
    /// this thread waits for.
    fn settle_copies(&mut self) {
        while let Some(copied) = self.copies.finished() {
            self.announce(if copied {
                notice::Notice::neutral(clipboard::TOPIC, clipboard::COPIED)
            } else {
                notice::Notice::error(clipboard::TOPIC, clipboard::COPY_FAILED)
            });
        }
    }

    /// Settles the row that says what the turn is doing.
    ///
    /// Once a turn, from [`Self::settle_band`], for the reason the queue's
    /// depth is read there: what this row says is an answer only the clock and
    /// the other thread produce, so reading it here is what makes the row and
    /// the frame that shows it agree.
    ///
    /// **Whether there is a turn at all is the runtime's to say, and it says
    /// so in events**: `TurnStarted` and its conclusion ([`Self::apply`]), which
    /// arrive in order on one channel and therefore cannot disagree with each
    /// other. Nothing here consults the queue's depth. It could not: work in
    /// hand is not always a turn -- a `/model` and a `/new` travel on the same
    /// channel -- and the place a concluded turn holds is given back *after*
    /// its conclusion is sent (`super::worker`'s `turn_loop`), so a count read
    /// on this side would be one number or the other depending on which thread
    /// ran last. What is settled here is only the half that is this thread's:
    /// **when** the turn the runtime announced started being measured.
    fn tick_activity(&mut self, now: Instant) {
        if self.activity.working() && !self.activity.started() {
            self.activity.begin(now);
        }
        // **The clock stops while the question is up**, because that interval
        // measures the person rather than the model: a turn that spent four
        // minutes waiting to be told whether it could edit a file did not spend
        // four minutes thinking. One place decides it -- the field the panel
        // lives in -- so the row and the reason for it cannot disagree, and
        // both calls are idempotent (`super::activity`). Either plane: what the
        // interval measures is the person, and a person reading a change on a
        // screen of its own is no less a person than one reading a band.
        if self.modal() {
            self.activity.freeze(now);
        } else {
            self.activity.thaw(now);
        }
        if self.render.animate(self.activity.working(), now) {
            self.phase = (self.phase + 1) % PHASES;
        }
        let output = self
            .tally
            .output
            .unwrap_or_else(|| self.tally.streamed.estimate());
        let colours = super::activity::Colours {
            label: self.palette.activity(),
            dim: self.palette.dim(),
            reset: self.palette.reset(),
        };
        // **No row at all while a question is up**, as upstream's footer has
        // none (`render_input.zig:705`, `frameOwnedActivityProjection`): the
        // band is the question's then, and a clock that is not running beside
        // it would be a row reporting on a turn that is waiting for the person
        // reading it. The clock stopped above; the row comes back, at the
        // elapsed time it stopped at, when the question goes.
        //
        // A retry the turn is waiting on is the row, whole, in the warning
        // grey and with no marker (`fx-retry-40x120-r0.6.ansi`:
        // `ESC[38;5;252m⚠ Provider unavailable · … · retrying request in 1s`),
        // counting down on this clock.
        let row = match self.retry.as_ref() {
            _ if self.modal() => None,
            Some(standing) if self.activity.working() => {
                Some(self.painted(self.palette.warning(), standing.label(now)))
            }
            _ => self.activity.row(
                now,
                self.phase,
                self.geometry.cols,
                (self.tally.input, output),
                colours,
            ),
        };
        if row == self.activity_row {
            // A phase that turned over without changing the row is not a frame:
            // the band is repainted whole, and twenty of those a second for a
            // row that says the same thing is a cost paid on every link.
            return;
        }
        // A row that appeared or went away is a row the band gained or gave
        // back, so the geometry is re-solved before the frame is asked for --
        // `refit` is what moves the divider, and the caret with it.
        let appeared = row.is_some() != self.activity_row.is_some();
        self.activity_row = row;
        if appeared {
            self.refit();
        }
        self.render.request(Reason::Animation);
    }

    /// What a theme report says, applied to the palette.
    ///
    /// Three ways this changes nothing, and each of them is a decision rather
    /// than an omission:
    ///
    /// * **A locked session.** `XFX_THEME` outranks the terminal at launch
    ///   (`super::theme::detect`), so it outranks it here too -- a user who
    ///   named a palette is not overruled by the terminal they named it on.
    /// * **The mode it already has.** A terminal may re-report the same mode,
    ///   and an answer to [`super::theme::MODE_QUERY`] after a resume usually
    ///   is one; repainting the band for it would be a frame per report.
    /// * **The depth.** How exactly a colour may be asked for is a property of
    ///   the terminal program (`super::theme::depth_from_env`), and a
    ///   background that changed is not a terminal that changed.
    ///
    /// Otherwise the band is owed a frame, and the panel above it is owed one
    /// too when there is a panel: the palette paints the divider, the hint row
    /// and a refusal, and a question in the band is painted between them.
    /// And the rows the document is already showing are owed one too
    /// ([`Self::retint_debt`]) -- but only while this session can still say
    /// what is on them. Something that damaged the screen and has not reached
    /// the band yet ([`Self::damage_untold`]) means the shadow those rows
    /// would be repainted from describes a screen that is gone, and repainting
    /// from it is how erased history comes back. A reset whose handoff is
    /// still owed ([`Self::handoff`]) is the same: the rows are on their way
    /// into scrollback, which is never recoloured.
    fn retint(&mut self, mode: Mode) {
        if self.theme_locked || self.palette.mode == mode {
            return;
        }
        self.palette.mode = mode;
        // The answer's own styles follow the mode too, as upstream's theme
        // switch re-applies them (`theme.zig` `applyTheme`/`activate`).
        let theme = markdown::theme::builtin(mode == Mode::Light);
        self.markdown.apply_theme(&theme);
        self.markdown.activate_theme(theme);
        if !self.damage_untold && self.handoff.is_none() {
            self.retint_debt = true;
        }
        self.render.request(Reason::Footer);
        if self.slot().is_some() || self.owner == ScreenOwner::Alternate {
            self.render.request(Reason::Modal);
        }
    }

    /// The palette the band and the document are painted in now.
    pub(crate) fn palette(&self) -> Palette {
        self.palette
    }

    /// Whether the document rows on the screen are owed a recolour.
    pub(crate) fn owes_retint(&self) -> bool {
        self.retint_debt
    }

    /// Records that the recolour has been fully delivered, or that there was
    /// nothing knowable to recolour.
    ///
    /// Called by the loop only on a vector that landed or one that was never
    /// needed, so a refused write leaves the debt exactly where it was and the
    /// next tick offers it again -- there is no re-arming anywhere, because
    /// nothing takes it.
    pub(crate) fn retint_paid(&mut self) {
        self.retint_debt = false;
    }

    /// Records that something which is not the band has written on the screen.
    ///
    /// The **one** door for that fact: the frame the reason asks for is a whole
    /// repaint of the band, and what is already on the document's rows stops
    /// being knowable at the same instant -- so a visible recolour owed against
    /// it is dropped here rather than paid onto a screen that has changed
    /// underneath it. The prior ownership goes with it: nothing of what those
    /// rows held is carried across the damage.
    ///
    /// `pub(crate)` for the one raiser outside this module, and it is the one
    /// that matters most: a resume, where the terminal has been somebody
    /// else's in between (`super::event_loop`'s `adopt_resume`).
    pub(crate) fn external_damage(&mut self) {
        self.tail_unknown = true;
        // A handoff still owed was measured against rows this damage may have
        // changed; it carries the blank tail it was going to leave out.
        self.handoff = self.handoff.map(|_| 0);
        self.render.request(Reason::ExternalDamage);
        self.retint_debt = false;
        self.damage_untold = true;
    }

    /// Records that the band has been told, which is where its own `damaged`
    /// takes over ([`super::event_loop`]'s two `Band::invalidate` calls).
    pub(crate) fn damage_told(&mut self) {
        self.damage_untold = false;
    }

    /// Asks the terminal which way round it is now, the next time the loop
    /// writes.
    ///
    /// **Not a write of its own**, and that is the whole of why this is a flag.
    /// The only writer on this terminal is the loop's paint tick, which counts
    /// its failures against a budget and refuses to address a screen no band
    /// fits on; a query written from here would be a second writer with neither.
    ///
    /// Nothing is armed for a session whose palette is locked: there is no
    /// answer it would act on, and a query is a sequence written onto a user's
    /// terminal.
    pub(crate) fn resync_theme(&mut self) {
        if self.theme_locked {
            return;
        }
        self.theme_query = true;
    }

    /// Whether a theme query is owed, taken so it is written once.
    pub(crate) fn take_theme_query(&mut self) -> bool {
        std::mem::take(&mut self.theme_query)
    }

    /// Hands one back after [`Self::take_theme_query`] took it and the write
    /// never reached the terminal.
    ///
    /// A refused write moved no byte, so what it was offering is still owed,
    /// and nothing else ever sets this flag for a resume that has already been
    /// adopted.
    pub(crate) fn restore_theme_query(&mut self) {
        self.theme_query = true;
    }

    /// Whether the screen owes a resume's terminal reset
    /// (`super::frame::Band::wipe`). Asked rather than taken, as the handoff
    /// is: owed until one lands.
    pub(crate) fn owes_wipe(&self) -> bool {
        self.wipe
    }

    /// Records that the reset landed: the screen and its scrollback are blank,
    /// the document has no row on it, and the band floats back to the top
    /// under the replay the resume queued. A handoff owed against the old
    /// screen went with it.
    pub(crate) fn wiped(&mut self) {
        self.wipe = false;
        self.handoff = None;
        self.tail_unknown = false;
        self.document_rows = 0;
        self.refloat();
        self.render.request(Reason::Transcript);
    }

    /// Whether the screen owes a reset's scrollback handoff.
    ///
    /// Asked rather than taken: the handoff is owed until one lands
    /// ([`Self::handed_off`]), so a write the terminal refused leaves it owed
    /// without a second call that would have to remember to put it back.
    pub(crate) fn owes_handoff(&self) -> bool {
        self.handoff.is_some()
    }

    /// How many of the rows the handed-off document ends on are blank -- the
    /// spacing under its last summary or notice, which upstream does not carry
    /// into scrollback (`super::frame::Band::hand_off`).
    pub(crate) fn handoff_spacing(&self) -> u16 {
        self.handoff.unwrap_or(0)
    }

    /// Records that the handoff landed: the screen is blank, the document has
    /// no row on it, and the band floats back to the top under the welcome
    /// line the reset queued.
    ///
    /// **Here and not at the reset**, because until this moment the rows the
    /// old document holds are still on the screen. A band painted under a
    /// document of no rows before the handoff -- the frame that gives the
    /// plane back after a question, which paints from this geometry -- would
    /// be painted over them, and the handoff would then erase what it was
    /// supposed to carry into scrollback.
    pub(crate) fn handed_off(&mut self) {
        self.handoff = None;
        self.tail_unknown = false;
        self.document_rows = 0;
        self.refloat();
        // Every row of the band is gone from the screen with the document, so
        // the frame that follows is not optional whether or not the band moved.
        self.render.request(Reason::Transcript);
    }

    /// Applies decoded events in order.
    ///
    /// `now` is the read's own clock, handed down rather than read again per
    /// event: the bytes of one read arrived together, and a Ctrl-C burst whose
    /// two bytes timed each other out would be two unrelated keystrokes.
    fn consume(&mut self, events: Vec<Input>, now: Instant) {
        for event in events {
            // **Before the focus, and that is the point of putting it here.**
            // A theme report is not a keystroke: nobody typed it, and every
            // branch below is written to swallow what it does not bind -- so a
            // report routed after them would work at an empty composer and
            // stop working the moment a question was up, which is exactly the
            // kind of "works when you test it" this ordering exists to avoid.
            // It reaches no editor, no panel and no draft; what it moves is
            // which greys the next frame is painted in.
            if let Input::Report(mode) = event {
                self.retint(mode);
                continue;
            }
            // **Before the focus, as upstream disarms before it routes**
            // (`app_input_runtime.zig:954-962`): any keystroke but the two the
            // gestures time takes back an armed exit and an armed interrupt,
            // whatever has the keys. A paste is content rather than a
            // keystroke, and its bytes and its frame disarm nothing. An Escape
            // the composer will see disarms through [`Gestures::escape`]; one a
            // panel, a menu or the full transcript takes is a key like any
            // other here.
            let disarms = match &event {
                Input::Action(Action::Cancel | Action::PasteStart | Action::PasteEnd)
                | Input::Action(Action::Ignore)
                | Input::PasteByte(_)
                | Input::Report(_) => false,
                Input::Action(Action::Escape) => {
                    self.ask.is_some()
                        || self.asking()
                        || self.help_menu.is_some()
                        || self.picker.is_some()
                        || self.full.is_open()
                }
                _ => true,
            };
            if disarms && self.gestures.other_key() {
                self.render.request(Reason::Footer);
            }
            // The full transcript, while it is up, has the keys it binds;
            // everything else goes where it would have gone with the screen
            // closed -- the composer, the band's menus -- as upstream routes
            // it (`app_input_runtime.zig:973-994`, `:1013-1044`). It cannot
            // be up with a question: a question closes it
            // ([`Self::full_transcript_sees`]) and it does not open over one
            // ([`Self::open_full_transcript`]).
            if self.full.is_open() && self.view_full_transcript(&event) {
                continue;
            }
            // Whatever is in front of the user has the focus while it is up,
            // and this is the whole of what that means: nothing below runs, so
            // a `1` cannot be typed into the composer and a Ctrl-D cannot leave
            // a session with a turn waiting on an answer. On either plane, and
            // for either kind of question: the focus belongs to what is being
            // asked, not to the surface it happens to be asked on.
            //
            // The model's own question batch is read first only because it has
            // to be read somewhere -- the two can never both be up
            // ([`Self::slot`]) -- and the keys mean different things at each, so
            // the two branches stay separate rather than sharing one
            // translation.
            if self.ask.is_some() {
                self.answer(event, now);
                continue;
            }
            // Shift on Up and Down means something only to a question's
            // field; to every surface below it is the arrow it always was.
            let event = unshifted(event);
            if self.asking() {
                self.decide(event, now);
                continue;
            }
            // The `/statusline` picker has the focus whenever no question
            // does: everything is its, and what it does not bind it swallows
            // -- a letter, a Ctrl-C, a Ctrl-D -- as upstream's compact menus
            // do (`routeActiveModalInput`, `app_input_runtime.zig:1366-1395`).
            // Escape is the way out, and then the keys mean what they mean.
            if self.statusline_menu.is_some() {
                self.offer_to_statusline(event);
                continue;
            }
            // And the catalog or the completion menu -- never both -- takes
            // the keys it binds, before the editor and before the gestures see
            // them. Everything else falls through with it still up, because
            // each is a view of the draft and typing is what the draft is made
            // of.
            if self.offer_to_sessions(&event) {
                continue;
            }
            if self.offer_to_catalog(&event) {
                continue;
            }
            if self.offer_to_models(&event) {
                continue;
            }
            if self.offer_to_column(&event) {
                continue;
            }
            if self.offer_to_picker(&event) {
                continue;
            }
            if self.offer_to_provider_picker(&event) {
                continue;
            }
            match event {
                Input::Text(character) => self.type_character(character),
                Input::Action(action) => self.act(action, now),
                // Content, and it goes nowhere near the composer until the
                // frame closes: `super::paste` filters the bytes a terminal
                // would obey out of it, counts them against the budget, and
                // decides at `Action::PasteEnd` whether the composer gets the
                // text or a summary standing in for it. Nothing is painted per
                // byte either -- a frame per byte of a megabyte paste is a
                // session that stops answering the keyboard.
                Input::PasteByte(byte) => self.paste.byte(byte),
                // Handled at the top of this loop, before the focus.
                Input::Report(_) => {}
            }
        }
    }

    /// One typed character.
    ///
    /// A character the byte budget refuses changes nothing and says nothing:
    /// upstream flashes the composer (`input_limit_rejection.zig:4-23`) and
    /// this phase does not, which `docs/parity.md` records.
    fn type_character(&mut self, character: char) {
        let mut encoded = [0u8; 4];
        let typed = character.encode_utf8(&mut encoded);
        // **Against the prompt's budget, not the composer's.** What the draft
        // shows for a collapsed paste is 25 bytes standing for as much as 8
        // MiB, so a composer that counted only its own text would let a
        // keystroke build a prompt twice the size of the cap
        // ([`super::paste::fits`]). Refused silently, like every other
        // keystroke the budget refuses.
        //
        // One question rather than two. While a block was a *name*, a keystroke
        // could land inside a summary and release the megabytes behind it, so
        // this had to be asked again of the draft the edit would produce. A
        // block is a span now: the caret is never inside one, so a keystroke
        // can only ever add its own bytes and the cheap answer is the true one.
        if !paste::fits(
            self.editor.text().len(),
            self.editor.retained(),
            typed.len(),
        ) {
            return;
        }
        if let Some(delta) = self.editor.insert(typed) {
            self.column_edit = ColumnEdit::Typed;
            self.amended(Some(delta));
        }
    }

    /// The end of a paste: what the composer is given for it.
    ///
    /// One insert for the whole paste rather than one per byte, which is the
    /// difference between a paste and a very fast typist: the composer re-wraps
    /// and the band re-solves once, and a paste of a megabyte does not cost a
    /// megabyte of wraps on its way in.
    ///
    /// **A paste past the budget puts nothing in the composer at all.** There
    /// is no block registered for it ([`super::paste::Paste::finish`]), so the
    /// summary would be words rather than a stand-in for the text -- a draft
    /// that submitted `[Pasted text #1, 1 lines]` as a prompt is worse than a
    /// paste that plainly did not happen, and the hint row says which it was.
    fn pasted(&mut self) {
        let pasted = self
            .paste
            .finish(self.editor.text().len(), self.editor.retained());
        match pasted {
            Pasted::Refused(refusal) => {
                self.notice = Some(match refusal {
                    Refusal::Oversized => PASTE_REFUSED,
                    Refusal::Unnumbered => PASTE_UNNUMBERED,
                });
                self.render.request(Reason::Footer);
            }
            Pasted::Inline(text) => {
                // An empty paste is not an edit: asking for a frame and
                // re-solving the band for a composer nobody changed is the
                // repaint [`Self::clear_composer`] guards against for the same
                // reason.
                if text.is_empty() {
                    return;
                }
                if let Some(delta) = self.editor.insert(&text) {
                    self.amended(Some(delta));
                }
            }
            // The screen gets the summary and the text goes behind it as an
            // entity, so 1800 codepoints are never painted into a band and
            // never re-wrapped by the next keystroke -- and the block is that
            // run of the draft rather than those words wherever they appear.
            Pasted::Collapsed {
                summary,
                id,
                text,
                lines,
            } => {
                // No arm for a composer that refuses the summary, and that is
                // arithmetic rather than optimism: a block is only collapsed
                // past `COLLAPSE_ABOVE` codepoints, the budget admitted the
                // draft plus that text, and the composer's own cap is the same
                // number -- so the room left over is never smaller than a name
                // ([`super::paste`]'s
                // `a_collapsed_paste_the_budget_admits_always_fits_the_composer`).
                let Some(delta) = self
                    .editor
                    .insert_entity(&summary, EntityKind::Paste { id, text, lines })
                else {
                    return;
                };
                // **One delta for the whole paste**, recorded at the moment the
                // frame closed -- the only moment that knows the boundary. Its
                // weight carries the payload, so a paste the history cannot hold
                // is a boundary rather than an entry that lies
                // ([`super::edit_history`]).
                self.amended(Some(delta));
            }
        }
    }

    /// What one decoded action means.
    ///
    /// Exhaustive on purpose: an action added later has to be given a home
    /// here rather than falling into a wildcard that silently ignores it.
    fn act(&mut self, action: Action, now: Instant) {
        match action {
            // The composer's own, and **moves**: the caret goes somewhere
            // else and the text does not change, so a recalled line is still
            // the line on the screen and the walk stays open. Reading a
            // recalled prompt before stepping further back is exactly this.
            Action::Left
            | Action::Right
            | Action::Home
            | Action::End
            | Action::WordLeft
            | Action::WordRight => {
                let recorded = self.editor.apply(action, self.text_cols());
                debug_assert!(recorded.is_none(), "a caret move produced a delta");
                self.moved();
            }
            // The composer's own, and **edits**: the text is the user's now
            // rather than the recalled line's, so the walk is over
            // ([`Self::amended`]).
            Action::Backspace
            | Action::Delete
            | Action::DeleteWordLeft
            | Action::KillToEnd
            | Action::KillToStart => {
                let delta = self.editor.apply(action, self.text_cols());
                // The one deletion a later column survives
                // (`shouldPreserveModelPickerBackspace`).
                if action == Action::Backspace {
                    self.column_edit = ColumnEdit::Backspace;
                }
                self.amended(delta);
            }
            // The two keys that are the composer's until the caret has nowhere
            // left to go, and the history's exactly there.
            //
            // The edge is read off **the move itself** rather than off a second
            // wrap of the draft: `Editor::move_by_row` leaves the caret's row
            // alone precisely when the row it wanted is off the end of the
            // wrap, so a row that did not change is the first row for an `Up`
            // and the last one for a `Down` -- the same fact, measured by the
            // module that owns the wrap instead of restated by this one.
            // Never reaches here (`unshifted` folds them back before the
            // composer sees an event); kept as the arrows they are, so a route
            // that skipped the fold would still move the caret rather than drop
            // the key.
            Action::SelectUp => self.act(Action::Up, now),
            Action::SelectDown => self.act(Action::Down, now),
            Action::Up | Action::Down => {
                let cols = self.text_cols();
                let from = self.editor.point(cols).0;
                let recorded = self.editor.apply(action, cols);
                debug_assert!(recorded.is_none(), "a vertical move produced a delta");
                if self.editor.point(cols).0 != from {
                    self.moved();
                    return;
                }
                let step = if matches!(action, Action::Up) {
                    HistoryStep::Previous
                } else {
                    HistoryStep::Next
                };
                if !self.recall(step) {
                    // A recall that recalled nothing still owes a frame,
                    // because the keystroke was still applied: the move
                    // recorded the column this run of vertical motion is aiming
                    // for even though the caret could not move, and that is the
                    // state the frame after it is drawn from. Repaint only --
                    // [`Self::moved`] rather than [`Self::edited`] -- because
                    // nothing about the text changed, so the history is still
                    // the truth about the last edit
                    // ([`super::edit_history`]).
                    self.moved();
                }
            }
            // **The one editing action that adds text**, so it goes the way a
            // typed character goes rather than the way the moves and the
            // deletes do: through the budget. `C-j` inserts a newline and
            // nothing else (`super::editor::Editor::apply`), so routing it
            // here is the same edit with the same question asked first.
            Action::InsertNewline => self.type_character('\n'),
            Action::Submit => self.submit(),
            // Ctrl-C. With `ISIG` cleared the terminal generates no `SIGINT`,
            // so this byte is the only Ctrl-C a TUI session sees -- and what it
            // means is the line-oriented shell's rule, decided in
            // [`super::gesture`].
            Action::Cancel => self.interrupt(now),
            // A lone Escape does nothing here; the second one inside its window
            // clears the composer at an idle prompt and stops the turn over
            // running work, and the hint row says which in between
            // ([`super::gesture`]).
            //
            // "Running work" is the turn the activity row is about: upstream
            // arms its interrupt over an *active operation*
            // (`hasActiveOperation`, `app_input_runtime.zig:3168`), and the row
            // is this session's reading of exactly that -- from the runtime's
            // own `TurnStarted` to its conclusion or the keystroke that
            // stopped it. A prompt still waiting for the runtime to pick it up
            // is not yet an operation there is anything to interrupt in.
            Action::Escape => {
                match self
                    .gestures
                    .escape(now, self.activity.working(), !self.editor.is_empty())
                {
                    Escape::Nothing | Escape::Armed | Escape::InterruptArmed => {
                        self.settle_band(now)
                    }
                    Escape::Clear => {
                        self.clear_composer();
                        self.settle_band(now);
                    }
                    // Upstream's `cancelActiveOperation`, with no exit armed: the
                    // Escape pair is its own confirmation, and the row after it is
                    // the session's (`fx-interrupt2-40x120-esc2`).
                    Escape::Interrupt => {
                        self.cancel_turn();
                        self.settle_band(now);
                    }
                }
            }
            // The end of the session, but only from an empty composer: with
            // text under the caret Ctrl-D is the forward delete it is in every
            // shell that has both, and leaving would throw away a draft.
            //
            // **And not while a turn is running**: upstream ignores it while
            // an answer streams (`handleSemanticCtrlD`,
            // `app_input_runtime.zig:1750`). A key that sits beside Ctrl-C and
            // means "I am done" must not end a session in the middle of the
            // answer the user is reading; Ctrl-C is the key that stops a turn,
            // and it says so first. Read off the activity row, as upstream
            // reads its stream: a turn the user has already stopped is not
            // one, so a Ctrl-D after a Ctrl-C leaves as it always has.
            Action::Eof => {
                if self.editor.is_empty() {
                    if !self.activity.working() {
                        self.leave();
                    }
                } else {
                    let delta = self.editor.apply(Action::Delete, self.text_cols());
                    self.amended(delta);
                }
            }
            // The whole band is repainted every frame, so a redraw is a frame.
            Action::Redraw => self.external_damage(),
            // The frame around a paste. Everything between them is content
            // rather than keys, which is the whole of why a pasted newline does
            // not submit the composer and a pasted `0x03` does not cancel a
            // turn.
            Action::PasteStart => self.paste.begin(),
            Action::PasteEnd => self.pasted(),
            // `Tab` reaches here only with no menu and no question up -- both
            // bind it, and both are offered every keystroke before this runs --
            // and there is nothing else on this surface for it to complete. An
            // `Ignore` is a keystroke this session has no binding for at all:
            // an event rather than silence, precisely so that it accounts for
            // the bytes it was decoded from.
            // `C-p` and `C-n`, which are the recall **wherever the caret
            // is**: the arrows above cannot reach it from the middle of a
            // multi-row draft, which is the draft a user reaches for a recall
            // from. A step that recalls nothing changes nothing and asks for
            // no frame -- a keystroke that did not move the band must not
            // repaint it.
            Action::HistoryPrevious => {
                self.recall(HistoryStep::Previous);
            }
            Action::HistoryNext => {
                self.recall(HistoryStep::Next);
            }
            // **The composer's own history**, and neither of them is an
            // `amended`: an undo is not the user replacing a recalled line, it
            // is the user taking their own last edit back, and recording one
            // would clear the redo stack it is walking.
            //
            // The two fields are split at the call site rather than moved
            // through the history, which is two disjoint field borrows and what
            // keeps the transfer atomic: a caller that popped an entry and
            // forgot to stash it would silently destroy the redo path, so there
            // is no public pop ([`super::edit_history::EditHistory::undo`]).
            Action::Undo | Action::Redo => {
                let Shell {
                    edit_history,
                    editor,
                    ..
                } = self;
                let moved = match action {
                    Action::Undo => edit_history.undo(|delta| editor.revert(delta)),
                    _ => edit_history.redo(|delta| editor.replay(delta)),
                };
                if moved {
                    self.moved();
                }
            }
            // A yank **is** an edit: it puts text in the composer, so it ends
            // the recall walk like every other one and is itself undoable.
            Action::Yank => {
                let delta = self.yank_killed();
                if delta.is_some() {
                    self.amended(delta);
                }
            }
            // Shift-Tab reaches here only with no question, catalog or menu
            // up -- each is offered every keystroke first, and the catalog
            // binds it -- which is upstream's own order: the menus that cycle
            // a category take it, and otherwise it is the permission mode
            // (`app_input_runtime.zig:1175-1185`).
            Action::BackTab => self.cycle_mode(),
            // The full transcript (`input_full_transcript_runtime.zig:83-99`).
            Action::ToggleTranscript => self.open_full_transcript(),
            // The page keys are the full transcript's, and the composer has no
            // page of its own.
            Action::Tab | Action::PageUp | Action::PageDown | Action::Ignore => {}
        }
    }

    /// Puts the kill slot's text at the caret, under numbers of its own.
    ///
    /// **Fresh ids, and the summaries rewritten to say them.** A yank does not
    /// move a block, it makes another one: the original may still be in the
    /// draft, and `entity.rs:212-216` refuses two live blocks under one number
    /// -- so the payload is shared (`Arc`) and the name is not. The delta is
    /// [`super::edit_history::DeltaKind::Ordinary`] rather than `Kill`, so that
    /// recording it does not overwrite the slot it has just read.
    fn yank_killed(&mut self) -> Option<Delta> {
        let (killed, spans) = self.edit_history.killed()?;
        let mut text = killed.to_string();
        let mut entities = Entities::new();
        for span in spans {
            entities.register(span.clone());
        }
        let wanted = entities.len();
        // **Minted against a copy of the counter, and committed only if the
        // edit happens.** `renumber_recalled` writes the numbers it spends
        // straight back into whatever it is handed, and nothing in this session
        // ever rewinds that: a number spent is spent for good, deliberately, so
        // that two live blocks can never answer to one name
        // (`super::entity::Entities::register`'s assertion). Handed the
        // session's own counter, a yank the budget then refuses would burn a
        // name for an edit that did not happen -- press `C-y` at a full
        // composer often enough and the ids run out for pastes nobody made.
        let mut next = *self.paste.ids();
        entities.renumber_recalled(&mut text, &mut next);
        // The same question a typed character asks, and for the same reason: a
        // yank brings its payload back too, so the draft plus what its blocks
        // stand for has to stay inside one budget (`super::paste::fits`).
        if !paste::fits(
            self.editor.text().len(),
            self.editor.retained(),
            text.len().saturating_add(entities.retained()),
        ) {
            // Refused silently, like every other keystroke the budget refuses
            // (`Self::type_character`): the slot keeps what it holds, the draft
            // is exactly the text it was, and `next` is dropped with the
            // numbers it was going to spend still unspent.
            return None;
        }
        let delta = self.editor.insert_with_entities(&text, entities.spans())?;
        // The edit landed, so the names it put on the screen are this session's
        // now. Committed **after** it and not before, because the composer has
        // a refusal of its own (`super::editor::Editor::insert`) and a name on
        // no summary is the same leak by another door.
        *self.paste.ids() = next;
        if entities.len() < wanted {
            // The end of the id space, seen from the yank: those summaries are
            // words now, and a line that would be sent as its own description is
            // one the user has to be told about. Said after the insert for the
            // reason above -- and reachable only through it, since the budget
            // question above is the stricter of the two.
            self.notice = Some(YANK_UNNUMBERED);
        }
        Some(delta)
    }

    /// One Ctrl-C at the composer.
    ///
    /// What it means is [`super::gesture`]'s to decide, from whether the
    /// runtime has work in hand and whether the composer holds a draft.
    fn interrupt(&mut self, now: Instant) {
        let draft = !self.editor.is_empty();
        match self
            .gestures
            .interrupt(now, self.work.outstanding() > 0, draft)
        {
            Interrupt::Cancel => self.cancel_turn(),
            Interrupt::Clear => self.clear_composer(),
            Interrupt::Leave => self.leave_by(Leaving::Interrupted),
        }
        self.settle_band(now);
    }

    /// Stops the turn in hand, and says so the way upstream does.
    ///
    /// The cancellation goes out on the **control** channel, which is unbounded
    /// and read by the runtime *inside* the turn (`super::worker`'s `run_turn`),
    /// so it cannot queue behind the backlog of deltas it is trying to stop.
    /// The UI deliberately does not cancel anything itself: what it holds is the
    /// **session's** cancellation, and cancelling that would make every later
    /// turn be born cancelled (`super::bridge`'s `Cancellation::turn`) -- one
    /// Ctrl-C would end the conversation rather than the answer.
    ///
    /// **What the screen shows is decided here, at the keystroke, and not at
    /// the conclusion**: upstream publishes the cancellation the moment it is
    /// asked for (`presentActiveToolCancellation`, `transcript/runtime.zig:
    /// 4761-4763`), and a keystroke's answer that waited for the runtime to
    /// unwind -- or for a provider that has gone quiet to hang up -- would not
    /// be an answer to it. So the answer stops where the user stopped reading
    /// it ([`Self::abandon_answer`]), the row above the divider goes
    /// (`fx-interrupt-40x120-cmd-cc`: no activity row under the line), and the
    /// one line upstream writes for a cancelled turn takes the place of the
    /// rest ([`Self::write_cancelled`]).
    fn cancel_turn(&mut self) {
        self.cancel_turn_with(false);
    }

    /// The same, told whether the keystroke has already written a call's own
    /// `■ Cancelled …` line -- which is the turn's line too, so no second one
    /// is written (`append_turn_cancellation`, `transcript/runtime.zig:
    /// 5378-5383`).
    ///
    /// **A call that is still running is not settled by this keystroke**, and
    /// that is the difference from upstream's `fx-interrupt-40x120-cmd-cc`,
    /// where the command is killed at once. Here a running executor holds the
    /// runtime's thread until it returns, so the stop is read only after the
    /// command has finished (`super::worker`'s `raced_against_control` polls
    /// the control channel beside the turn, not inside a tool). A `Cancelled`
    /// row now would be the screen claiming a stop the command never had, and
    /// so would the turn's line: both wait for the turn to end. The call's
    /// real result settles its row, one the turn ends without is settled as
    /// stopped with its own line ([`Mark::TurnOver`]), and the turn's line is
    /// written then only when no call's line was ([`Self::cancel_owed`]). The
    /// activity row stays while the command runs, because it does.
    fn cancel_turn_with(&mut self, written: bool) {
        // Read **before** the message goes out, because after it the runtime
        // is dropping exactly these and the count is on its way to zero: what
        // the user is owed a sentence about is what was waiting when they
        // pressed the key.
        let waiting = self.work.queued() > 0;
        // Quoting what has been submitted **so far** is what keeps the drop
        // from reaching past this keystroke: a prompt typed while this message
        // is still in flight is a new intention, and the runtime is told
        // exactly where the old ones stop.
        self.work.control(TurnControl::Cancel {
            through: self.work.accepted(),
        });
        self.forget_catalog_request();
        // The turn this stops ends with no bell: the user is here, and
        // upstream's turn-end cue is silent for an interrupted turn.
        self.sound.interrupted();
        self.cancelling = true;
        self.abandon_answer();
        self.retry = None;
        if waiting {
            self.prompts.clear();
            // The waiting prompt's rows go with it.
            self.refit();
        }
        // A prompt the runtime took before it read this is still to say it
        // started; with nothing waiting behind it, it is the only entry left.
        self.cancel_covers = self.prompts.len();
        if !written && self.group.running() {
            self.cancel_owed = Some(waiting);
            return;
        }
        self.activity.end();
        if !written {
            self.write_cancelled();
        }
        if waiting {
            // The second half of the same answer to the same keystroke, and
            // immediate for the same reason: "and the queue went with it" is
            // only useful beside the line it qualifies. Upstream has no queue
            // to drop; xfx's is its own, and so is this sentence.
            self.write_notice(&notice::Notice::cancelled("queue", QUEUE_DROPPED));
        }
    }

    /// Takes the answer off the screen's to-do list where it stands.
    ///
    /// Everything the pacer is still holding was never painted, and the line
    /// the presenter is holding back for its end was never painted either --
    /// upstream paints neither after a cancel (`fx-interrupt-40x120-mid` and
    /// `-cc-0.3`: the un-ended words were not on the screen before the
    /// keystroke and are not after it). Both go; the conversation keeps them,
    /// because what the model is told is the runtime's record and not this
    /// screen. The lines xfx placed *behind* that text -- a tool row, a notice
    /// -- are not the answer's and are written now, at the point the stream
    /// has been declared to have reached, rather than dropped with it.
    fn abandon_answer(&mut self) {
        self.pacer.forget();
        self.emitted = self.enqueued;
        // Before the marks, which close the answer as they run: closing it
        // with the held line still in the presenter would paint that line.
        self.markdown.reset();
        self.run_due_marks();
        self.close_prose();
    }

    /// Upstream's line for a cancelled turn (`writeTurnCancellation` and
    /// `lifecycleTerminalLine`, `transcript/runtime.zig:633-680`, `:9951-9966`):
    /// `■` in the warning grey, `Cancelled` in the hint grey, and the question
    /// after it in the terminal's own colour -- with a blank row above and
    /// below, as `fx-interrupt-40x120-cc-0.3` lays it out.
    fn write_cancelled(&mut self) {
        self.finish_document_line();
        if self.transcript.queue_gutter(Gutter::None) {
            self.owed();
        }
        if !self.transcript.ends_blank() {
            self.end_transcript_line();
        }
        let line = cancelled_line(self.palette);
        self.write_transcript_as(&line, Role::Plain);
        self.end_transcript_line();
        self.end_transcript_line();
    }

    /// Empties the composer, and with it the paste blocks its summaries named.
    ///
    /// One operation rather than two call sites that must remember each other:
    /// a block that outlived the draft it was pasted into would hold the whole
    /// paste for the rest of the session, and a span into a buffer that has
    /// been emptied names bytes that are not there.
    fn take_draft(&mut self) -> String {
        // **Every stacked delta named the draft that is about to be gone.** Its
        // offsets are absolute into that text, so an undo after this would
        // `replace_range` past the end of an empty `String` -- a panic -- or
        // write a dead draft's bytes into an unrelated one. Cleared here, at the
        // single funnel, so every caller present and future inherits it and no
        // list of them has to be maintained ([`super::edit_history`]).
        //
        // The kill slot is deliberately **not** cleared: it is separate state
        // upstream too, and only a new session empties it
        // (`kill_ring.zig:71-84`). What was killed out of a submitted draft is
        // still yankable into the next one.
        self.edit_history.boundary();
        // The blocks go with the text, because they are runs of it: the
        // composer's own `take` clears them (`super::editor::Editor::take`), so
        // there is no second call site that has to remember to.
        self.editor.take()
    }

    /// Throws the draft away, if there is one.
    ///
    /// The guard is not tidiness: [`Self::edited`] asks for a frame and
    /// re-solves the band, and a keystroke that changed nothing must not be a
    /// repaint of the whole band on a link that may be a serial line.
    fn clear_composer(&mut self) {
        if self.editor.is_empty() {
            return;
        }
        self.take_draft();
        self.amended(None);
    }

    /// What one submitted line is, and what happens to it.
    ///
    /// **Decided by [`router::classify`]**, which is
    /// [`crate::interactive::classify`] -- the one reading of what a leading
    /// `/` means, shared with the line shell -- plus the one rule this surface
    /// adds on purpose: upstream's, that a command which takes nothing is
    /// matched **exactly**. The names are `interactive::SLASH_COMMANDS` and
    /// nothing else: this surface answers exactly what the registry
    /// advertises.
    ///
    /// **A command is not echoed** (`fx-basic-40x120-version.txt`: `/version`
    /// leaves `* version: 0.0.13` and nothing above it). Only a prompt becomes
    /// a card: it is the user's half of a conversation, and a command is not
    /// part of one.
    ///
    /// Most of them are answered on this thread. `/new` and a `/model` change
    /// go to the runtime as control messages ([`TurnControl`]) -- the
    /// conversation and the model they change live there and change *between*
    /// turns rather than under one, and neither may be refused for a full
    /// queue -- and `/setup` and a `/model` catalog load as [`TurnWork`],
    /// because they open a socket, which the thread holding the terminal must
    /// never wait on.
    fn submit(&mut self) {
        if self.editor.is_empty() {
            return;
        }
        let text = self.editor.text().to_string();
        let submitted = router::classify(&text);
        // **Before any arm below, because every one of them consumes the
        // draft**, and the draft is the only place this line exists: the
        // command arms clear the composer through `run_command`, the prompt arm
        // through `send`, and a line recorded afterwards would be recorded from
        // an empty editor. A command is recorded like anything else -- it is a
        // line the user typed, and running one again is the commonest reason to
        // reach for the recall. Whitespace and nothing else is the one line
        // that is not: it is consumed and never written down, so there is
        // nothing to come back to, and an entry for it would put a keypress
        // between the user and the line they really sent.
        //
        // `Blank` still ends the walk, for the same reason recording one does:
        // the draft the walk was standing beside has been consumed.
        if matches!(submitted, Submitted::Blank) {
            self.history.leave();
        } else {
            // **With the blocks its summaries name.** An entry is the line as
            // the composer held it, which for a collapsed paste is 25 bytes
            // standing for as much as 8 MiB; an entry that recorded only the
            // words would recall a summary that stands for nothing
            // (`super::entity::EntitySnapshot`).
            self.history.record(HistoryEntry::new(
                text.clone(),
                self.editor.entities().snapshots(),
            ));
        }
        match &submitted {
            // Whitespace and nothing else. The line is consumed -- the user
            // pressed Return and a Return that left the composer untouched
            // would look like a session that had stopped listening -- and
            // nothing is sent or written.
            Submitted::Blank => {
                self.take_draft();
                self.edited(None);
            }
            // Upstream's explicit form, applied with the composer consumed --
            // or refused with the draft kept, for the user to fix rather than
            // to type again (`fx-model3-40x120-paste-bad-effort`).
            Submitted::Command {
                command: interactive::Slash::Model,
                argument,
            } if self.explicit_model(argument).is_some() => match self.explicit_model(argument) {
                Some(Ok(change)) => {
                    self.take_draft();
                    self.edited(None);
                    self.gestures.submitted();
                    self.request_change(change);
                }
                _ => self.write_notice(&notice::Notice::error("", MODEL_USAGE)),
            },
            Submitted::Command { .. } => self.run_command(&submitted),
            // A line that begins with `/` and names nothing -- or names a
            // command that takes nothing, with something after it -- is a
            // mistake, not a question, and it is answered with upstream's
            // refusal (`commandUnknown`, `app_commands.zig:2162-2169`). It does
            // **not** reach the model: a typo'd command silently becoming a
            // prompt is how a user pays for a slip in tokens and in an answer
            // to a question they did not ask.
            //
            // Upstream sends `/nosuch with words` to the model as a prompt
            // (`shouldRouteUnknownSlashCommand`,
            // `input_submit_runtime.zig:926-934`, routes only an unknown name
            // with nothing after it); this surface refuses it like the bare
            // name, for the reason above.
            Submitted::UnknownCommand { .. } => {
                // Consumed like any other submitted line: a composer that kept
                // it would have the user's next line typed onto the end of it.
                // At once rather than behind the stream, as it always was: it
                // answers the keystroke.
                self.take_draft();
                self.edited(None);
                self.write_notice(&notice::Notice::error("command", UNKNOWN_COMMAND));
            }
            // **Expanded here, and only here.** What the composer holds for a
            // collapsed paste is a summary; what the user meant to send is the
            // text it stands for, so the prompt is expanded on its way to the
            // runtime and the *document* still shows the summary -- a screen
            // that echoed eight megabytes back at the user would be the paint
            // the collapse exists to prevent.
            Submitted::Prompt(prompt) => {
                self.send(self.expanded_prompt(&text, prompt), &text);
            }
        }
    }

    /// The prompt a draft really sends: its blocks put back, trimmed exactly
    /// as [`crate::interactive::classify`] trimmed the line.
    ///
    /// Expanded from the **whole** draft and then cut, rather than expanded
    /// from the trimmed line, because the spans are runs of the draft and a
    /// classifier that removed two leading spaces would have moved every one of
    /// them. The cut is exact: the bytes `classify` trimmed are whitespace, a
    /// summary is not, so no span can begin inside either end -- the expansion
    /// changes nothing in front of the first non-blank byte or behind the last.
    ///
    /// `classified` is what the classifier made of the same line, and it is
    /// what this hands back when the draft holds no blocks at all -- the two
    /// are the same string then, and that is asserted rather than assumed.
    fn expanded_prompt(&self, text: &str, classified: &str) -> String {
        if self.editor.entities().is_empty() {
            return classified.to_string();
        }
        let expanded = self.editor.expanded();
        let lead = text.len().saturating_sub(text.trim_start().len());
        let tail = text.len().saturating_sub(text.trim_end().len());
        expanded[lead..expanded.len().saturating_sub(tail)].to_string()
    }

    /// Offers a prompt to the runtime.
    ///
    /// The offer comes **before** the composer is cleared, which is the whole
    /// of the ordering: a submission the runtime will not take must not have
    /// already thrown the draft away. What it takes is echoed into the
    /// terminal's document, so a submission is something the session visibly
    /// did rather than something that vanished.
    fn send(&mut self, prompt: String, text: &str) {
        // Of the prompt as the runtime is given it, which is what the store
        // records and derives its own title from -- not of the composer's
        // summary of a paste.
        let title = title_of(&prompt);
        // Whether it will wait behind a turn that is running -- the turn the
        // activity row is about, as upstream's steer waits on an active
        // operation. Work the session has stopped or reset is not one: a
        // prompt typed after it goes out as soon as that work unwinds, and its
        // card is written at once, as it always was.
        let waits = self.activity.working();
        match self.work.submit(TurnWork::Submit(prompt)) {
            Ok(()) => {
                // Nothing is said about the turn here, and that is the point:
                // an accepted prompt may wait behind another turn for a minute
                // (`super::worker::WORK_LIMIT`), and the band already says so
                // on its hint row. The row above the divider is about the turn
                // the runtime is *running*, so it waits for the runtime to say
                // that this one is (`UiEvent::TurnStarted`).
                self.take_draft();
                self.edited(None);
                let mut estimate = Estimator::default();
                estimate.consume(text);
                if waits {
                    self.prompts
                        .push_back((estimate.estimate(), title, Some(text.to_string())));
                    self.refit();
                    self.render.request(Reason::Footer);
                } else {
                    self.prompts.push_back((estimate.estimate(), title, None));
                    self.echo(text);
                }
                self.gestures.submitted();
            }
            Err(rejected) => self.refused(rejected),
        }
    }

    /// What a submission the runtime would not take costs.
    ///
    /// A full queue is a **hint-row** refusal with the draft left in the
    /// composer: the two belong together, because "this one was not sent" is
    /// only useful next to the text that was not sent. A runtime that is gone
    /// is a document row instead -- it is not a condition that clears, and a
    /// hint row would scroll it away under whatever comes next.
    fn refused(&mut self, rejected: Rejected) {
        match rejected {
            Rejected::Busy => {
                self.notice = Some(QUEUE_REJECTED);
                self.render.request(Reason::Footer);
            }
            Rejected::Gone => self.announce(notice::Notice::error("system", GONE_NOTICE)),
        }
    }

    /// Puts a submitted line into the document, where the user can see what
    /// they sent.
    ///
    /// The line ends whether or not the last thing typed was a newline: what
    /// was submitted is finished, and a tail left open would be continued by
    /// the answer.
    fn echo(&mut self, text: &str) {
        // The user's own line, so xfx paints no colour onto it: what is written
        // here is what they typed, in bold behind the rail, as upstream's card.
        self.mark(Mark::Card(text.to_string()));
    }

    /// The card [`Mark::Card`] asks for.
    fn write_card(&mut self, text: &str) {
        self.finish_document_line();
        if self.transcript.queue_gutter(Gutter::Rail) {
            self.owed();
        }
        // Bold opened on every line rather than once around the text: a hard
        // break ends a row's attributes here, as upstream's card re-applies
        // its style per row (`user_message_card.zig:365,374`).
        let card = text
            .split('\n')
            .map(|line| format!("{BOLD}{line}{RESET}"))
            .collect::<Vec<_>>()
            .join("\n");
        self.write_transcript_as(&card, Role::Plain);
        self.end_transcript_line();
        if self.transcript.queue_gutter(Gutter::None) {
            self.owed();
        }
        self.end_transcript_line();
    }

    /// Released answer text, through the presenter.
    fn write_answer(&mut self, text: &str) {
        for event in self.markdown.feed(text) {
            self.present(event);
        }
    }

    /// One thing the presenter completed, in the shape upstream gives it.
    fn present(&mut self, event: markdown::Event) {
        self.full.presented(&event);
        let cols = self.geometry.cols;
        match event {
            markdown::Event::Text(text) => self.write_prose(&text),
            markdown::Event::CodeBlock(block) => {
                let theme = markdown::theme::builtin(self.palette.mode == Mode::Light);
                let rows = blocks::code_block_row_bytes(&block, cols, &theme);
                self.write_structure(&rows);
            }
            markdown::Event::Table(table) => {
                let rows = blocks::table_row_bytes(&table, cols);
                self.write_structure(&rows);
            }
            markdown::Event::ThematicRule => {
                let rule = blocks::render_thematic_rule(cols);
                self.write_structure(&rule);
            }
        }
    }

    /// Rendered prose, in the answer's two-column gutter.
    ///
    /// A block's head is trimmed of blank lines and its tail is held back
    /// rather than written, so every block ends on exactly one blank row however
    /// many line breaks it arrived with -- upstream trims and normalizes each
    /// rendered block the same way (`transcript_blocks.zig`,
    /// `trimAssistantBlockHead` and `normalizeOwnedRenderedBlock`).
    fn write_prose(&mut self, text: &str) {
        let mut text = text;
        if !self.answer.open {
            text = text.trim_start_matches('\n');
            if text.is_empty() {
                return;
            }
            self.finish_document_line();
            if self.transcript.queue_gutter(Gutter::Indent) {
                self.owed();
            }
            self.answer.open = true;
        }
        let body = text.trim_end_matches('\n');
        if !body.is_empty() {
            if self.answer.held > 0 {
                let breaks = "\n".repeat(self.answer.held);
                self.answer.held = 0;
                self.write_transcript_as(&breaks, Role::Plain);
            }
            self.write_transcript_as(body, Role::Plain);
        }
        self.answer.held += text.len() - body.len();
    }

    /// Ends an open prose block on one blank row.
    fn close_prose(&mut self) {
        if !self.answer.open {
            return;
        }
        self.answer = Answer::default();
        self.finish_document_line();
        if self.transcript.queue_gutter(Gutter::None) {
            self.owed();
        }
        self.end_transcript_line();
    }

    /// A code panel, a table or a rule: rows already laid out at the screen's
    /// width, gutter included, and a blank row under them.
    fn write_structure(&mut self, rows: &str) {
        self.close_prose();
        self.finish_document_line();
        for row in rows.trim_end_matches('\n').split('\n') {
            self.write_transcript_as(row, Role::Plain);
            self.end_transcript_line();
        }
        self.end_transcript_line();
    }

    /// Whatever the presenter is still holding, and the answer's block closed.
    fn close_answer(&mut self) {
        for event in self.markdown.finish() {
            self.present(event);
        }
        self.markdown.reset();
        self.close_prose();
    }

    /// One canonical command, with the rest of the line as its argument.
    ///
    /// The composer is cleared first for all of them: a command is not offered
    /// to anything that can refuse it, so there is no draft to keep.
    ///
    /// **Which handler runs is [`super::router`]'s to decide, not this
    /// module's.** That is this surface's only dispatch point; the
    /// line-oriented shell has an exhaustive `match` of its own
    /// (`crate::interactive`'s `run`), because its arms are `async` and reach
    /// for session state this side does not hold. What keeps the two from
    /// drifting is that both read the same registry and both dispatch
    /// exhaustively: another command stops both from compiling until both
    /// answer it.
    fn run_command(&mut self, submitted: &Submitted) {
        self.take_draft();
        self.edited(None);
        self.gestures.submitted();
        router::route(submitted, self);
    }
}

/// `event` with Shift taken off Up and Down.
///
/// The decoder keeps Shift on the vertical arrows because a question's
/// freeform field binds it -- the plain arrows walk the field's rows and the
/// shifted ones choose (`super::input::Action::SelectUp`) -- and that is the
/// only surface that does. Folded back here, once, before any other surface
/// reads the event, so the composer, the approvals and the menus see exactly
/// the keys they saw before Shift was kept.
fn unshifted(event: Input) -> Input {
    match event {
        Input::Action(Action::SelectUp) => Input::Action(Action::Up),
        Input::Action(Action::SelectDown) => Input::Action(Action::Down),
        other => other,
    }
}

/// What each canonical command does on this surface.
///
/// Most are answered on the UI thread; `/setup` and a `/model` catalog load are
/// handed to the runtime as [`TurnWork`], because they open a socket the
/// thread holding the terminal must never wait on. A `/model` change and the
/// conversation half of `/new`, `/clear` and `/reset` go there as control
/// messages rather than as work ([`TurnControl::Model`],
/// [`TurnControl::Reset`]): the model and the conversation live there and
/// change between turns, and neither may wait behind -- or be refused by -- the
/// queue.
impl CommandHandlers for Shell {
    /// Opens the catalog (`commandShowHelp`, `app_commands.zig:700-706`):
    /// `All`, the first command marked, the composer -- emptied by the submit
    /// that got here -- as its search.
    ///
    /// **Nothing is written to the document**: the catalog is the answer, and
    /// it is in the band. The line-oriented shell keeps its page
    /// ([`crate::interactive::help_text`]); it has no band to put a catalog in.
    fn help(&mut self) {
        // The completion menu goes first: the two share the slot, and the
        // catalog is what the user asked for. So does the session picker.
        self.picker = None;
        self.sessions = None;
        self.help_menu = Some(HelpMenu::open(self.editor.text()));
        // [`Self::run_command`] re-solved the band before routing here, for
        // a band with no catalog in it.
        self.refit();
        self.render.request(Reason::Modal);
    }

    fn new_session(&mut self) {
        self.reset_session();
    }

    fn clear(&mut self) {
        self.reset_session();
    }

    fn reset(&mut self) {
        self.reset_session();
    }

    /// Opens the session picker on the current workspace
    /// (`openSessionPickerWithScope`, `app_session_runtime.zig:2588-2650`), on
    /// `Loading sessions…` until the runtime has read the store.
    ///
    /// **Refused while anything is in hand** -- a turn running, a prompt
    /// waiting, a `/model` being answered -- with upstream's notice, at once:
    /// the conversation a resume would replace is the one that work is
    /// writing into. Upstream refuses while its response streams; xfx's
    /// outstanding count is that and the prompt queued behind it, which would
    /// otherwise run in the old conversation after the resume had switched.
    fn resume(&mut self) {
        if self.work.outstanding() > 0 {
            self.write_notice(&notice::Notice::neutral("session", RESUME_BUSY));
            return;
        }
        if let Err(rejected) = self.work.submit(TurnWork::Sessions) {
            self.refused(rejected);
            return;
        }
        // Everything that shares the slot goes: the picker is what the user
        // asked for.
        self.picker = None;
        self.help_menu = None;
        self.model_menu = None;
        self.provider_picker = None;
        self.sessions = Some(SessionPicker::open(self.editor.text()));
        self.refit();
        self.render.request(Reason::Modal);
    }

    /// `/rename <title>` (`handleRenameCommand`, `app_commands.zig:3815-3858`).
    ///
    /// The title is judged here, by the store's own rule
    /// ([`crate::session::validate_title`]), and refused at once in upstream's
    /// words: `✗ usage: /rename <title>` with no topic for an empty one,
    /// `✗ session: title is too long` and `✗ session: title must be printable
    /// text` for the other two. An accepted one goes to the runtime, which
    /// writes it into the conversation's session and answers with
    /// [`UiEvent::Renamed`]: the window takes the title then, and the notice
    /// says whether the log has it. A rename a Ctrl-C drops from the queue
    /// before it runs titles nothing.
    fn rename(&mut self, argument: &str) {
        let title = match crate::session::validate_title(argument) {
            Ok(title) => title.to_string(),
            Err(problem) => {
                let topic = if problem == crate::session::TitleError::Empty {
                    ""
                } else {
                    "session"
                };
                self.write_notice(&notice::Notice::error(
                    topic,
                    interactive::title_refusal(problem),
                ));
                return;
            }
        };
        if let Err(rejected) = self.work.submit(TurnWork::Rename(title)) {
            self.refused(rejected);
        }
    }

    fn model(&mut self, argument: &str) {
        self.use_model(argument);
    }

    /// Upstream's `* stats: ansi_bytes=…, redraws=…, debounced_resizes=…,
    /// footer_updates=0, stream_chunks=0` (`app_commands.zig:1009-1022`): the
    /// counters as they stand **before** this notice is written, as upstream's
    /// are -- its own bytes are counted by the frame that delivers them.
    fn stats(&mut self) {
        self.announce(notice::Notice::neutral("stats", self.metrics.body()));
    }

    /// Upstream's `* status: model=…` with one `key=value` row per fact under
    /// it (`showStatus`, `session_commands.zig:278-310`), from the snapshot
    /// the runtime last published ([`super::worker::StatusBoard`]) -- so the
    /// model is the one the selector holds, the configuration is the one a
    /// provider switch reloaded, and the turn and grant counts are the live
    /// conversation's rather than the store's last session. Read, never asked
    /// for: the runtime may be a minute into a turn.
    ///
    /// xfx's own facts stay (`provider`, `provider_url`, `sandbox`); upstream's
    /// `update_channel` and `ultrafast_requested` are about an updater and an
    /// Ultra mode xfx does not have, so they are not printed as if it did.
    ///
    /// Made inert here rather than at the channel, because this text never
    /// crosses it: the workspace path and a settings value quoted by
    /// `provider_rejected` are not xfx's to trust on a terminal.
    fn status(&mut self) {
        let body = self.work.status().notice_body();
        self.announce(notice::Notice::neutral(
            "status",
            bridge::inert(&body).into_owned(),
        ));
    }

    /// Upstream's `* version: 0.0.13` (`app_commands.zig:2153-2160`): the
    /// bare version, no `v` and no build. The build is the welcome line's to
    /// say, and the channel and revision are `xfx status`'s.
    fn version(&mut self) {
        self.announce(notice::Notice::neutral("version", crate::VERSION));
    }

    /// Upstream's `commandQuit` (`app_commands.zig:662-665`): the session
    /// ends with status 0, and on the way out names itself as the one to
    /// resume when it had a conversation (`requestResumeExit`, `:344-348`).
    /// The naming is the exit's, not this handler's -- `super::worker`'s
    /// `Worker::resumable` once the runtime has stopped, written by `super`'s
    /// `session` after the restore -- so Ctrl-D and the second Ctrl-C, which
    /// request the same handoff upstream (`app_input_runtime.zig:1691`,
    /// `:1752`), name it too.
    fn quit(&mut self) {
        self.leave();
    }

    fn setup(&mut self, argument: &str) {
        self.use_provider(argument);
    }

    /// Upstream's `/permissions` (`handlePermissions`,
    /// `session_commands.zig:367-394`, then `writeSessionPermissionRules`,
    /// `app_commands.zig:776-811`, when bare).
    ///
    /// Bare, it says what is in force -- the mode this shell holds, and the
    /// grants as the runtime last published them -- and that xfx keeps no
    /// saved-session rules. A mode or `reset` is handed to the runtime
    /// ([`Self::choose_mode`]) and answered here at once, as upstream answers
    /// it; anything else is the usage line.
    fn permissions(&mut self, argument: &str) {
        use crate::permission::slash::Request;
        match Request::parse(argument) {
            Request::Show => {
                let grants = self.work.grants();
                self.announce(permissions::status(self.mode, &grants));
                self.announce(permissions::saved_rules());
            }
            Request::Select(mode) => {
                if self.choose_mode(Change::Mode(mode), mode) {
                    self.announce(permissions::selected(mode));
                }
            }
            Request::Reset => {
                if self.choose_mode(Change::Reset, PermissionMode::Ask) {
                    self.announce(permissions::reset());
                }
            }
            Request::Usage => self.announce(permissions::usage()),
        }
    }

    /// Upstream's `commandHandleStatusline` (`app_commands.zig:2094-2107`):
    /// bare, the picker opens on its first row in place of the composer, and
    /// the menus that share its slot close; with an item, that item flips and
    /// both notices are said ([`Self::set_statusline`]); with anything else,
    /// upstream's usage line as an error with no topic.
    fn statusline(&mut self, argument: &str) {
        if argument.is_empty() {
            self.picker = None;
            self.help_menu = None;
            self.statusline_menu = Some(statusline::Menu::open());
            // [`Self::run_command`] re-solved the band before routing here,
            // for a band with a composer in it.
            self.refit();
            self.render.request(Reason::Modal);
            return;
        }
        let Some(item) = StatusLineItem::parse(argument) else {
            self.write_notice(&notice::Notice::error("", interactive::STATUSLINE_USAGE));
            return;
        };
        self.set_statusline(item, !self.statusline.get(item), true);
    }

    /// Upstream's `commandCopyLast` (`app_commands.zig:1974-2001`): the
    /// conversation's last reply onto the clipboard, raw.
    ///
    /// The reply is the one the runtime last published
    /// ([`WorkHandle::last_reply`]) -- read at once, so a `/copy` typed while a
    /// turn is running copies the reply before it, as upstream's does, rather
    /// than waiting for that turn. "Nothing to copy" is said here; how the
    /// copy went is said when its thread says so ([`Self::settle_copies`]).
    fn copy(&mut self) {
        let Some(reply) = self.work.last_reply() else {
            self.announce(notice::Notice::neutral(
                clipboard::TOPIC,
                clipboard::NO_REPLY,
            ));
            return;
        };
        if !self.copies.start(reply) {
            self.announce(notice::Notice::error(
                clipboard::TOPIC,
                clipboard::COPY_FAILED,
            ));
        }
    }

    /// Upstream's `handleNotificationsCommand` (`app_commands.zig:3974-4029`):
    /// a bare `/sound` toggles, `on` and `off` set, both switches move together
    /// and are saved, and the answer is `* sound: <level>`. Anything else is
    /// `✗ usage: /sound [on|off]`, said at once like `/setup`'s refusal.
    ///
    /// The switches move **here**, before the save has landed, as upstream's
    /// do: the next turn rings or not by what was typed, whatever the disk
    /// says. The save goes to its own thread ([`super::sound`]); a save that
    /// could not be started is said before the level, as upstream says a
    /// failed one, and a save that fails later is said when it does
    /// ([`Self::settle_saves`]).
    fn sound(&mut self, argument: &str) {
        let Some(request) = crate::sound::Request::parse(argument) else {
            self.write_notice(&notice::Notice::error("", crate::sound::USAGE));
            return;
        };
        let level = request.level(self.sound.notifications());
        let (changed, started) = self.sound.set(level);
        if let Err(reason) = started {
            self.announce(notice::Notice::error(
                crate::sound::TOPIC,
                crate::sound::not_saved(changed, &reason),
            ));
        }
        self.announce(notice::Notice::neutral(crate::sound::TOPIC, level.label()));
    }
}

impl Shell {
    /// Hands the runtime a change to the session's authority and, once it has
    /// it, moves this shell's own idea of the mode -- the hint row -- to
    /// `mode`. Answers whether it did.
    ///
    /// **Not a [`TurnWork`]**: see [`WorkHandle::change_permissions`] for the
    /// three ways that channel would get this wrong. A runtime that is gone has
    /// nothing to apply a mode to, so the row keeps the mode it had and the
    /// refusal says why.
    ///
    /// The warning follows the mode, as upstream's `updateYoloWarningForMode`
    /// does (`app_permission_runtime.zig:168-176`): armed on the way into full
    /// access for someone who has not seen it yet, and taken down on the way
    /// out.
    fn choose_mode(&mut self, change: Change, mode: PermissionMode) -> bool {
        if let Err(rejected) = self.work.change_permissions(change) {
            self.refused(rejected);
            return false;
        }
        self.mode = mode;
        if mode == PermissionMode::Yolo && !self.acknowledged {
            self.warning.arm();
        } else {
            self.warning.clear();
        }
        self.render.request(Reason::Footer);
        true
    }

    /// Shift-Tab with nothing in front of the composer that binds it: the next
    /// mode round ask, auto, full access (`toggleMode`,
    /// `app_permission_runtime.zig:135-142`), with no notice -- upstream's key
    /// says it on the hint row and nowhere else.
    fn cycle_mode(&mut self) {
        let next = crate::permission::slash::next(self.mode);
        self.choose_mode(Change::Mode(next), next);
    }

    /// The full-access warning, if the hint row carries it now: the sentence,
    /// or its compact form on a row too narrow for the sentence, or nothing.
    ///
    /// Upstream's `dangerStatusText` (`input_presentation.zig:574-592`): a
    /// question, a pending approval, an armed Escape and a refusal on the row
    /// all own the row and keep the warning off it, and a menu or the catalog
    /// replaces the row with its key help (`paint_plan.zig:946-972`). The
    /// clock ([`permissions::Warning::observe`]) is asked with this same
    /// answer, so the time spent is time the person could see it.
    fn warning_shown(&self) -> Option<&'static str> {
        if !self.warning.active() || self.mode != PermissionMode::Yolo {
            return None;
        }
        // Nor while an armed cue or the exit hint has the row: upstream's
        // `dangerStatusText` is empty while any of them is (`:580`).
        if self.slot().is_some()
            || self.modal()
            || self.notice.is_some()
            || self.escape_armed
            || self.interrupt_armed
            || self.exit_armed
        {
            return None;
        }
        // Nor while the other buffer owns the screen -- a question's review or
        // the full transcript -- where the band is not painted at all
        // (`yolo_warning_visible`, `app_render_runtime.zig:1833-1834`).
        if self.owner != ScreenOwner::Primary {
            return None;
        }
        let cols = usize::from(self.geometry.cols);
        [permissions::WARNING, permissions::WARNING_COMPACT]
            .into_iter()
            .find(|text| usize::from(super::wrap::width(text)) <= cols)
    }

    /// One look at the warning's clock, from [`Self::settle_band`].
    ///
    /// The first time it is on the screen is the moment the person has been
    /// shown it, and that is recorded at once (`noteFrameCommitted`,
    /// `app_permission_runtime.zig:99-111`) -- once, whether the write works or
    /// not: a failed write is said by the runtime, and a second attempt per
    /// frame would be a write per frame.
    fn tick_warning(&mut self, now: Instant) {
        let shown = self.warning_shown().is_some();
        if shown && !self.acknowledged {
            self.acknowledged = true;
            // A runtime that is gone has nothing to write with, and this is
            // not a keystroke anybody is owed a refusal for.
            let _ = self.work.change_permissions(Change::Acknowledge);
        }
        if self.warning.observe(now, shown) {
            self.render.request(Reason::Footer);
        }
    }

    /// `/model`, as upstream answers it.
    ///
    /// **Bare, it opens the menu** (`openModelBrowseCatalog`,
    /// `app_input_runtime.zig:2346-2355`), with the composer -- emptied by the
    /// submit that got here -- as its search. The rows are the ones this shell
    /// already holds when it holds an answer for the provider in force;
    /// otherwise the menu says `Loading models…` and the load is handed to the
    /// runtime thread, which owns the socket, as [`TurnWork::Catalog`] -- the
    /// one network call a bare `/model` makes, and never one this thread waits
    /// on. A load that failed is asked again: that is the `retry /model.` the
    /// menu said. The rows arrive as [`UiEvent::CatalogLoaded`].
    ///
    /// **With one word, it asks for that model** ([`Self::request_model`]):
    /// upstream's router form. Two or three words are the explicit form,
    /// answered before this is reached ([`Self::explicit_model`]).
    fn use_model(&mut self, argument: &str) {
        if argument.is_empty() {
            self.open_models();
            return;
        }
        // Before anything else, and in the order
        // [`crate::provider::model::ModelSelector::apply`] asks it in: what a
        // model id may be is one question with one answer, and this front end
        // does not get its own. Refused **here**, without reaching the runtime:
        // the far side records a model change in the session log, so an id with
        // a control character in it would be written down and read back by
        // every later resume of this session.
        if let Some(problem) = model_id_problem(argument) {
            // The line shell's own line for this, from `interactive`'s
            // `ModelOutcome::Refused` arm. Nothing of the argument is quoted
            // back: every one of these reasons is xfx's own words, which is
            // what keeps a hostile id out of the document.
            self.write_notice(&notice::Notice::error("model", problem));
            return;
        }
        self.request_model(argument);
    }

    /// Opens `/model`'s menu over the rows in hand, or asks for them.
    fn open_models(&mut self) {
        // The completion menu shares the slot, and the menu is what the user
        // asked for.
        self.picker = None;
        let note = (self.provider == ProviderId::Gateway)
            .then_some(self.gateway_note)
            .flatten();
        let menu = match self.catalog_known {
            CatalogKnown::Loaded => ModelMenu::ready(&self.catalog, note, self.editor.text()),
            // A load is on its way already: the menu waits for it rather than
            // asking twice.
            CatalogKnown::Asked => ModelMenu::loading(self.editor.text()),
            // Never asked, wanted by the column while the queue was full, or
            // failed -- in which case this is the retry upstream's sentence
            // asks for.
            CatalogKnown::Unasked | CatalogKnown::Wanted | CatalogKnown::Unread(_) => {
                if let Err(rejected) = self.work.submit(TurnWork::Catalog) {
                    self.refused(rejected);
                    return;
                }
                self.catalog_known = CatalogKnown::Asked;
                ModelMenu::loading(self.editor.text())
            }
        };
        self.model_menu = Some(menu);
        self.refit();
        self.render.request(Reason::Modal);
    }

    /// Forgets a catalog load that was asked for and may now never be
    /// answered: an interrupt drops queued work, and a load the runtime was
    /// running when it came sends nothing more. Left as it was, the menu and
    /// the column would say `loading` for the rest of the session and nothing
    /// would ask again; forgotten, the next `/model` or column asks afresh.
    fn forget_catalog_request(&mut self) {
        if self.catalog_known == CatalogKnown::Asked {
            self.catalog_known = CatalogKnown::Unasked;
        }
    }

    /// Asks the runtime for the catalog on the inline column's behalf, when
    /// nothing has yet.
    ///
    /// **Quietly, unlike [`Self::open_models`]**: the column opens on a
    /// keystroke -- the space after `/model` -- and a keystroke is not a
    /// command, so a full queue is not refused on the hint row. The column says
    /// `loading models...` and the request is made again by the next keystroke
    /// or the end of the turn that was holding the queue
    /// ([`CatalogKnown::Wanted`]). A catalog that could not be read is not asked
    /// again from here: the column says `unable to load models`, and `/model`
    /// is the gesture that retries.
    fn want_catalog(&mut self) {
        if !matches!(
            self.catalog_known,
            CatalogKnown::Unasked | CatalogKnown::Wanted
        ) {
            return;
        }
        match self.work.submit(TurnWork::Catalog) {
            Ok(()) => self.catalog_known = CatalogKnown::Asked,
            Err(Rejected::Busy) => self.catalog_known = CatalogKnown::Wanted,
            Err(gone @ Rejected::Gone) => self.refused(gone),
        }
    }

    /// Asks for `query`, typed after `/model`, with the router form's meaning
    /// (`handleModel` -> `resolveModelQuery`, `session_commands.zig:1147-1166`):
    /// an id or an alias spelled exactly, else the best fuzzy match, else the
    /// query as typed.
    ///
    /// **Resolved here when it can be**, because upstream says which model the
    /// next turn uses the moment the command is given -- `Next turn will use`
    /// while one streams -- and the band can only say that if it knows the id.
    /// It can when it holds the catalog's answer: the rows the runtime sent,
    /// resolved by the selector's own rule
    /// ([`crate::provider::model::resolve_in_order`]), or no rows at all, which
    /// the selector applies as typed. Then the id itself is what is sent, so
    /// the runtime applies exactly what was announced. With no answer in hand
    /// -- nothing asked yet, or a load that failed and is about to be asked
    /// again -- the query goes as typed, the runtime resolves it, and its
    /// answer is the announcement ([`UiEvent::ModelAnswered`]).
    fn request_model(&mut self, query: &str) {
        let resolved = match self.catalog_known {
            CatalogKnown::Loaded => Some(resolve_in_order(&self.catalog, query)),
            CatalogKnown::Unasked
            | CatalogKnown::Asked
            | CatalogKnown::Wanted
            | CatalogKnown::Unread(_) => None,
        };
        let announced = resolved.is_some();
        self.request_change(ModelChange {
            name: resolved.unwrap_or_else(|| query.to_string()),
            effort: None,
            picker: false,
            announced,
            next_turn: self.turn_pending(),
        });
    }

    /// Whether a turn is running or waiting to, so a change made now is for the
    /// turn after it.
    fn turn_pending(&self) -> bool {
        self.tally.started.is_some() || !self.prompts.is_empty()
    }

    /// Hands one model change to the runtime, and -- when this side knows the
    /// id -- says so at once.
    ///
    /// **A control message, never refused** ([`TurnControl::Model`]): a change
    /// typed while a turn runs and a prompt waits is put aside by the runtime
    /// and applied between the two, rather than refused as a full queue. The
    /// band's own record of the model and the effort moves with the
    /// announcement, and is moved again -- to the same values -- by the
    /// runtime's answer.
    fn request_change(&mut self, change: ModelChange) {
        if change.announced {
            self.model = change.name.clone();
            if let Some(effort) = &change.effort {
                self.effort = effort.clone();
            }
            let line = switched(change.next_turn, &self.model, &self.effort);
            self.announce(notice::Notice::neutral("", line));
            self.render.request(Reason::Footer);
        }
        self.work.control(TurnControl::Model(change));
    }

    /// Upstream's explicit form, `/model <id> <effort> [normal|fast|ultrafast]`
    /// (`parseExplicitModelSelection` and `validateExplicitModelSelection`,
    /// `app_input_runtime.zig:133-187`), judged when Enter reaches the
    /// composer: `None` when the line is not that form at all -- one word, or
    /// a second word that is no effort -- and it goes on to the router form.
    ///
    /// **The id is literal** -- never fuzzy-resolved (`paste-short-id`) -- and
    /// is judged against the catalog's row for it: the effort has to be one
    /// the row lists or the default, and a third word may only be `normal`,
    /// for a model that offers a speed to decline -- `fast` and `ultrafast`
    /// are refused, because xfx sends no speed yet (`docs/parity.md`). A model
    /// with no row in hand has no levels to choose from, as upstream's generic
    /// capabilities have none (`capabilitiesForModel`). What is refused is
    /// refused with upstream's usage line and the composer kept.
    fn explicit_model(&self, argument: &str) -> Option<Result<ModelChange, ()>> {
        let tokens: Vec<&str> = argument.split_whitespace().collect();
        // `parseExplicitModelSelection`'s order: a fifth word is refused
        // before anything is read, fewer than two is not this form, and a
        // second word that is no effort makes it the router form's.
        if tokens.len() > 4 {
            return Some(Err(()));
        }
        if tokens.len() < 2 {
            return None;
        }
        let effort = Effort::parse(tokens[1])?;
        if tokens.len() == 4 {
            return Some(Err(()));
        }
        let model = tokens[0];
        if model_id_problem(model).is_some() {
            return Some(Err(()));
        }
        let entry = known(self.catalog_known, &self.catalog).entry(model);
        let supported = match &entry {
            Some(entry) => effort.supported_by(entry),
            None => effort.is_default(),
        };
        if !supported {
            return Some(Err(()));
        }
        // The third word (`validateExplicitModelSelection`): `normal` -- no
        // speed, which is what xfx sends -- for a model that offers one to
        // turn down, as upstream refuses it for a model with neither; `fast`
        // and `ultrafast` are refused, because xfx cannot send them yet.
        if let Some(speed) = tokens.get(2) {
            let offers_speed = entry
                .as_ref()
                .is_some_and(|entry| entry.facts.fast || entry.facts.ultrafast);
            if !(speed.eq_ignore_ascii_case("normal") && offers_speed) {
                return Some(Err(()));
            }
        }
        // `selectModelFromPicker`: the effort goes with a model that lists
        // levels, and a model with none leaves the session's as it is.
        let effort = entry
            .filter(|entry| !Effort::options(entry).is_empty())
            .map(|_| effort);
        Some(Ok(ModelChange {
            name: model.to_string(),
            effort,
            picker: true,
            announced: true,
            next_turn: self.turn_pending(),
        }))
    }

    /// `/provider` (or `/setup`) as a submitted line: the column, or a switch.
    ///
    /// **Bare**, it is upstream's `commandProvider` (`app_commands.zig:734-745`,
    /// `app_auth_runtime.zig:441-470`): the composer is reseeded with the
    /// canonical `/provider ` -- whichever spelling was typed -- and the column
    /// opens under its argument. Refused, with nothing reseeded, while a turn
    /// runs or a prompt waits for one ([`Self::provider_work_busy`]).
    ///
    /// **With an argument** it reaches here only when the column did not take
    /// the Return: Escape had closed it, or the argument names no provider.
    /// The first is xfx's `/setup llmux` and it switches at once, as it always
    /// has; the second is upstream's router refusing text its column could not
    /// match (`command_router.zig:212-214`), in upstream's words rather than a
    /// usage line of xfx's own. Nothing of the argument is quoted back, for the
    /// reason `/model`'s refusal quotes nothing.
    ///
    /// The switch itself is the runtime's to do and to report
    /// ([`Self::switch_provider`]): it writes a file, may open a socket and
    /// re-reads the configuration, none of which may happen on the thread
    /// holding the terminal, and **none of which this side is entitled to
    /// predict** -- what the session became arrives as
    /// [`UiEvent::ProviderSelected`] after the reload.
    fn use_provider(&mut self, argument: &str) {
        let provider = if argument.is_empty() {
            None
        } else {
            let Some(provider) = ProviderId::parse(argument) else {
                self.write_notice(&notice::Notice::error("command", UNKNOWN_COMMAND));
                return;
            };
            Some(provider)
        };
        if self.provider_work_busy() {
            self.write_notice(&notice::Notice::neutral("provider", PROVIDER_BUSY));
            return;
        }
        match provider {
            Some(provider) => self.switch_provider(provider),
            None => {
                // A fresh column: a dismissal of an earlier one is not about
                // this command.
                self.provider_dismissed = false;
                self.replace_word(provider_picker::PROVIDER_PREFIX);
            }
        }
    }

    /// `/clear`, `/new` and `/reset`: one operation under three names, as
    /// upstream's are (`app_commands.zig:667-680`, all three reaching
    /// `resetSessionWithBackgroundPolicy`, whose policy is discarded at
    /// `app_session_runtime.zig:2001`).
    ///
    /// **Cancel first, and never wait in the queue.** One control message
    /// ([`TurnControl::Reset`]): a turn that is running is cancelled exactly as
    /// Ctrl-C cancels one, together with everything queued behind it, and the
    /// conversation is dropped the moment that turn has unwound -- rather than
    /// waiting for it to finish by itself, and without taking a place in the
    /// queue a prompt typed straight afterwards needs
    /// (`decideLiveSessionTransition`, `:137-170`). Unlike Ctrl-C it says
    /// nothing about it: upstream's reset writes no cancelled notice, and the
    /// screen the notice would land on is the fresh one.
    ///
    /// **The screen is handed to scrollback, not erased.** The document still
    /// on it goes into the terminal's own history and the welcome line is
    /// painted on row 1 of a blank screen with the band under it
    /// (`commitVisibleTranscriptBeforeFreshSession`, then
    /// `finishLiveSessionTransition`, `:1978`, `:2012-2018`). The bytes are the
    /// loop's to write ([`Self::owes_handoff`]); what is dropped here is
    /// everything owed against the old screen -- the appends not yet written
    /// and the pacer's text, so the partial answer of a cancelled turn does
    /// not follow it -- and what is queued is the welcome line.
    ///
    /// **No notice**, and the window title goes back to the workspace
    /// (`clearCachedSessionTitle`, `:1982`). The context meter's numerator goes
    /// with the conversation it measured; its denominator, the model's window,
    /// is still true and stays.
    fn reset_session(&mut self) {
        // Everything submitted so far belongs to the conversation being
        // dropped; a prompt typed after this is past the count and runs in the
        // new one.
        let through = self.work.accepted();
        self.work.control(TurnControl::Reset { through });
        self.forget_catalog_request();
        // The prompts the cancellation drops, and the turn it stops, will
        // title nothing: none of them belongs to the session that starts now.
        self.prompts.clear();
        self.forget_session_title();
        self.tally = Tally::default();
        self.activity.end();
        self.gestures.turn_ended();
        // A cancel this session was waiting to hear the end of belongs to the
        // conversation being dropped, and so does its line.
        self.cancelling = false;
        self.cancel_owed = None;
        self.cancel_covers = 0;
        self.retry = None;
        self.context_used = None;
        // The old screen's debts: the text the pacer holds, the lines placed
        // behind it, the answer block a cancelled turn left open, and -- once
        // its tail has been read below -- the rows not yet written. The stream
        // is declared arrived, so nothing later is held behind a position no
        // emission will reach.
        self.pacer.forget();
        self.marks.clear();
        self.emitted = self.enqueued;
        self.markdown.reset();
        self.answer = Answer::default();
        // The tool group goes whole, unlike a block a write merely closes
        // ([`Self::close_group`]): its block is the old screen's, and a call
        // still running belongs to the turn this stops -- its result is
        // drained, so a call kept here would sit unsettled in the next
        // session's first group.
        self.group = ToolGroup::default();
        self.trailer.clear();
        self.abandoned.clear();
        // A recolour owed against the old rows is owed against rows on their
        // way into scrollback, which is never recoloured ([`Self::retint`]).
        self.retint_debt = false;
        // The blank row the document ends on, when this session can say so:
        // everything it wrote has landed, so the transcript's own record of
        // its tail is a record of the screen, and no resize has left the rows
        // above the band to the terminal's reflow. Otherwise none is left out,
        // and the worst that costs is a blank row in history. A second reset
        // before the handoff has no document of its own and keeps the first's.
        if self.handoff.is_none() {
            let known = !self.transcript.owes() && !self.tail_unknown;
            self.handoff = Some(u16::from(known && self.transcript.ends_blank()));
        }
        self.transcript = Transcript::new(self.geometry.cols);
        // The full transcript is the document's record, and the document
        // starts again: the old one is in scrollback, not on this screen
        // (upstream's projection is of the session's entries, which the reset
        // replaces). The welcome below is its first row again.
        self.close_full_transcript();
        self.full = FullTranscript::default();
        self.welcome();
        // What the runtime still sends about the old conversation is dropped
        // until it says the conversation is gone.
        self.draining = Some(through);
        self.render.request(Reason::Footer);
    }

    /// What a draining session does with one event from the work a reset
    /// cancelled, or `false` when the event is to be shown as usual.
    ///
    /// Exhaustive on purpose, for the reason [`Self::apply`] is.
    fn drained(&mut self, event: &UiEvent) -> bool {
        match event {
            // The old conversation's words and numbers, on a screen that is no
            // longer about it.
            UiEvent::TurnStarted
            | UiEvent::Delta(_)
            | UiEvent::ToolStart { .. }
            | UiEvent::ToolResult { .. }
            | UiEvent::ToolDetail { .. }
            | UiEvent::FullDetail(_)
            | UiEvent::ToolFeedback { .. }
            | UiEvent::Notice(_)
            | UiEvent::Retrying { .. }
            | UiEvent::Usage { .. } => true,
            // A question the cancelled turn asked before it heard about the
            // cancellation. **Refused on the user's behalf rather than
            // dropped**: the prompter parks on the session's root and the
            // control channel (`super::approval`), and the cancellation it
            // would otherwise wake on may already have been consumed by the
            // turn -- so a question nobody answered could hold the runtime, and
            // the drop behind it, for good.
            UiEvent::Approval(asked) => {
                self.work.control(TurnControl::Answer {
                    id: asked.id,
                    answer: ApprovalAnswer::Deny,
                    feedback: None,
                });
                true
            }
            UiEvent::Question(request) => {
                self.work
                    .control(TurnControl::QuestionCancelled { id: request.id });
                true
            }
            // The cancelled work is over. What a conclusion does to the band
            // still happens -- the activity row is about a turn that has ended
            // -- and nothing is written: no summary and no failure for a turn
            // the user reset.
            UiEvent::TurnEnded { .. } => {
                self.tally = Tally::default();
                self.turn_title = None;
                self.activity.end();
                self.pacer.finish();
                self.gestures.turn_ended();
                true
            }
            // The fence itself, the runtime's own state, and a runtime that is
            // gone: shown as usual. A provider switch or a model change that
            // completed before the cancellation reached it really did happen.
            UiEvent::Fresh { .. }
            | UiEvent::Sessions(_)
            | UiEvent::Resumed(_)
            | UiEvent::ResumeFailed { .. }
            | UiEvent::Renamed { .. }
            | UiEvent::ProviderSelected { .. }
            | UiEvent::ModelAnswered { .. }
            | UiEvent::CatalogLoaded { .. }
            | UiEvent::CatalogUnread { .. }
            | UiEvent::Fatal(_) => false,
        }
    }

    /// The window title the session wants, when the band has not been told it
    /// yet.
    ///
    /// Taken by the loop before it builds a frame, because the band is the
    /// title's only writer (`super::frame::Band::set_title`): this side
    /// decides what the title says and the band decides when it is written.
    /// Asked for once per change rather than per frame, so a frame that changes
    /// nothing about the title allocates nothing for it.
    pub(crate) fn take_title(&mut self) -> Option<String> {
        if !std::mem::take(&mut self.title_owed) {
            return None;
        }
        Some(super::frame::title(
            self.session_title.as_deref(),
            &self.workspace,
        ))
    }

    /// Gives the session its title, from the prompt of the turn that just
    /// finished, when it has none yet.
    ///
    /// Upstream derives it when the first turn's history is committed and
    /// freezes it there (`ensureCachedSessionTitle`); a turn that failed is
    /// not one this surface can say was recorded, so it titles nothing.
    fn title_session(&mut self) {
        if self.session_title.is_some() {
            return;
        }
        if let Some(title) = self.turn_title.take() {
            self.session_title = Some(title);
            self.retitle();
        }
    }

    /// Forgets the session's title, for a conversation that is being dropped.
    ///
    /// And the running turn's candidate with it: a `/new` taken while a turn
    /// runs is queued behind that turn, so the turn belongs to the
    /// conversation being dropped, and its end must not title the next one.
    fn forget_session_title(&mut self) {
        self.turn_title = None;
        if self.session_title.take().is_some() {
            self.retitle();
        }
    }

    /// Owes the band the window title, and the frame that writes it.
    fn retitle(&mut self) {
        self.title_owed = true;
        self.render.request(Reason::Footer);
    }

    /// How far down the screen the document reaches ([`Self::document_rows`]).
    pub(crate) fn document_rows(&self) -> u32 {
        self.document_rows
    }

    /// Records what the appends that landed brought the document to, and
    /// moves the band under it.
    pub(crate) fn document_landed(&mut self, rows: u32) {
        self.document_rows = rows;
        self.refloat();
    }

    /// Records the rows a growing band scrolled off the top of the screen.
    pub(crate) fn document_carried(&mut self, rows: u16) {
        self.document_rows = self.document_rows.saturating_sub(u32::from(rows));
        self.refloat();
    }

    /// This session as it stands once the document has filled the screen: the
    /// band at the bottom, where every case written before the band floated
    /// still expects it.
    #[cfg(test)]
    pub(crate) fn anchored(mut self) -> Self {
        self.document_rows = u32::MAX;
        self.geometry = self.geometry.floated(self.document_rows);
        self
    }

    /// The band, moved to sit under the document as it now stands.
    fn refloat(&mut self) {
        let geometry = self.geometry.floated(self.document_rows);
        if geometry != self.geometry {
            self.geometry = geometry;
            self.render.request(Reason::Resize);
        }
    }

    /// One step of a walk back through what has been submitted.
    ///
    /// `true` when the composer holds a different line because of it, which is
    /// what the two arrow keys use to tell "this keystroke was the recall" from
    /// "this keystroke was a caret move that had nowhere to go".
    ///
    /// The draft is handed to [`History::navigate`] rather than read by it:
    /// entering a walk **captures** what is being typed, and the composer is
    /// the only thing that knows what that is.
    fn recall(&mut self, step: HistoryStep) -> bool {
        // The draft the walk stands aside carries its blocks too, so a walk
        // that comes back to a half-typed line comes back to the whole of it.
        let current = HistoryEntry::new(
            self.editor.text().to_string(),
            self.editor.entities().snapshots(),
        );
        let Some(entry) = self.history.navigate(step, current) else {
            return false;
        };
        // **Fresh numbers, and the draft is rewritten to say them.** The
        // entry's own numbers belong to a draft that has been sent; minting
        // them again would put two live blocks under one name, and the one on
        // the screen is the one a user would expect to be theirs. The allocator
        // is the session's ([`super::paste::Paste::ids`]), so a paste after
        // this recall cannot collide with what it just minted.
        let wanted = entry.entities().len();
        let mut text = entry.text().to_string();
        let mut entities = Entities::recalled(entry.entities());
        entities.renumber_recalled(&mut text, self.paste.ids());
        if entities.len() < wanted {
            // The end of the id space, seen from the recall: those summaries
            // are words now, and a line that will be sent as its own
            // description is a line the user has to be told about.
            self.notice = Some(RECALL_UNNUMBERED);
        }
        if !self.editor.set_text(&text, entities) {
            // Arithmetic rather than optimism, and taken seriously rather than
            // unwrapped: every entry and every captured draft came *out* of a
            // composer, so none of them can be past a cap the composer is
            // already inside. If one ever were, the editor kept the draft it
            // had -- so leaving the walk puts the session back exactly where
            // this keystroke found it.
            self.history.leave();
            return false;
        }
        // Nothing to forget beside the composer: the blocks the replaced draft
        // held were spans of that text and went with it
        // (`super::editor::Editor::set_text`), and the ones now in the composer
        // are the recalled entry's, under numbers this session has just minted.
        //
        // **A completed recall is a boundary, not a delta**
        // (`input_completion_runtime.zig:339-356`: `.moved =>
        // historyBoundary`, `.unchanged => {}`). The whole draft was replaced,
        // so every delta on the stack names offsets into a text that is gone.
        // `set_text` is the one whole-draft disposal that does not go through
        // [`Self::take_draft`], which is why the boundary is named here.
        self.edit_history.boundary();
        // Not `amended`: an edit ends the walk, and this *is* the walk.
        self.edited(None);
        true
    }

    /// What a change to the composer's **text** owes, over what a caret move
    /// owes.
    ///
    /// The extra obligation over [`Self::edited`] is one thing and it is the
    /// walk: the line on the
    /// screen is the user's own now rather than the one the history handed
    /// back, so the next step back begins at the newest entry again and comes
    /// back to *this* text. Split from [`Self::edited`] rather than folded into
    /// it because the two are different questions -- `Left` through a recalled
    /// prompt is how a user reads it before deciding to step further back, and
    /// a rule that ended the walk on every keystroke would make that
    /// impossible.
    fn amended(&mut self, delta: Option<Delta>) {
        self.history.leave();
        self.edited(delta);
    }

    /// What a change to the composer's **text** owes, over what a caret move
    /// owes: the history entry.
    ///
    /// **Every text change passes through here and no caret move does**, which
    /// is the whole of the split. A funnel written by the repaint path would
    /// record an entry after a `Left`, and an undo built on it would take a
    /// megabyte back a grapheme at a time.
    ///
    /// `None` is a text change with **nothing to record**, and there are two
    /// kinds: a whole-draft disposal, whose boundary was already taken at
    /// [`Self::take_draft`] or at the recall, and a mutation the composer
    /// refused or found nothing to do -- neither of which may clear the redo
    /// stack ([`super::edit_history`]).
    fn edited(&mut self, delta: Option<Delta>) {
        if let Some(delta) = delta {
            self.edit_history.record(delta);
        }
        self.moved();
    }

    /// What **any** keystroke that touched the composer owes: a frame, and a
    /// band the right height for the text it now holds.
    ///
    /// A caret move owes exactly this and nothing more. The menu is reconciled
    /// here rather than beside the text changes because it is a view of the
    /// draft *and* of the caret, and it is reconciled **before** the band is
    /// re-solved: the rows it takes are rows of the band, so a geometry solved
    /// from the menu the last keystroke wanted would put the divider a row out.
    fn moved(&mut self) {
        self.reconcile_picker();
        self.reconcile_column();
        self.refit();
        self.render.request(Reason::Footer);
    }

    /// Re-solves the band for the composer's current height.
    ///
    /// The cap is measured against the screen rather than against the band, so
    /// growing the composer cannot move its own ceiling
    /// ([`layout::input_row_limit`]). A screen that holds a band holds a capped
    /// composer on it too -- the cap is never more than `rows - 2`, so there is
    /// always a document row left -- which is why the refusal below is not a
    /// case this can reach. It is `solve`'s answer being taken seriously rather
    /// than unwrapped, and a band that cannot be re-solved keeps the height it
    /// had. `the_smallest_band_still_grows_by_the_rule_rather_than_by_luck` is
    /// where that claim is checked from the smallest screen up.
    fn refit(&mut self) {
        let (rows, cols) = (self.geometry.rows, self.geometry.cols);
        let Some(geometry) = self.fit(rows, cols) else {
            return;
        };
        if geometry == self.geometry {
            return;
        }
        self.geometry = geometry;
        // The divider moved, so every row of the band is somewhere else.
        self.render.request(Reason::Resize);
    }

    /// The band this shell wants on a screen of `rows` x `cols`, or `None` when
    /// no band fits on one.
    ///
    /// Every height the band has, derived from this shell's own state and from
    /// the screen it is given rather than from the screen it is on -- which is
    /// what lets [`Self::resize`] ask the question about a screen the geometry
    /// does not describe yet. The composer's is a function of the **width**,
    /// so a re-solve that measured the draft against the old one would put the
    /// divider where the old wrap said it went.
    fn fit(&self, rows: u16, cols: u16) -> Option<Geometry> {
        let limit = layout::input_row_limit(rows);
        let wrapped = self
            .editor
            .rows(cols.saturating_sub(PROMPT_CELLS).max(1))
            .len();
        let wanted = u16::try_from(wrapped.clamp(1, usize::from(limit))).unwrap_or(limit);
        // The band's other height: whether the turn's row is above the divider.
        // Carried through every re-solve rather than defaulted, or a keystroke
        // typed while a turn ran would take that row away and the frame after
        // it would put it back.
        let activity = self.activity_row.is_some();
        // The band's third height: the rows a pending decision -- or a
        // completion menu -- is taking, asked of the screen in question because
        // a narrower one wraps the summary onto more of them and a shorter one
        // shows fewer matches.
        //
        // The catalog's is a function of the composer's height as well
        // (`inlinePickerRowBudgetCapped`'s `input_extra`), so it is asked
        // again for each composer height the search below tries -- and so is
        // the `/provider` column's (`activeListPickerReservedRows`), which
        // also counts the turn's row: upstream has that row above its footer,
        // xfx has it in the band. Every other occupant's is asked once.
        let slot = self.slot();
        let fixed = match slot {
            Some(Slot::Question(panel)) => panel.height(cols, rows),
            Some(Slot::Ask(ask)) => ask.height(cols, rows),
            Some(Slot::Menu(picker)) => picker.height(rows),
            Some(Slot::Column) => model_column::height(rows),
            Some(Slot::Statusline(_)) => statusline::Menu::height(rows),
            Some(Slot::Catalog(_))
            | Some(Slot::Sessions(_))
            | Some(Slot::Models(_))
            | Some(Slot::Provider(_))
            | None => 0,
        };
        let panel = |input_rows: u16| match slot {
            Some(Slot::Catalog(menu)) => menu.height(rows, input_rows.saturating_sub(1)),
            Some(Slot::Sessions(sessions)) => sessions.height(rows, input_rows.saturating_sub(1)),
            Some(Slot::Models(menu)) => menu.height(rows, input_rows.saturating_sub(1)),
            Some(Slot::Provider(_)) => {
                ProviderPicker::height(rows, input_rows.saturating_sub(1) + u16::from(activity))
            }
            _ => fixed,
        };
        // **The composer gives way to the question, one row at a time.** A
        // panel and a tall draft together can want more rows than the screen
        // has, and the draft is the half that can afford to lose one: a
        // composer shown two rows shorter still shows the caret and the text
        // around it ([`editor::window`]), while a panel with its choices below
        // the last row of the screen is a question with no visible answers. The
        // search is bounded by the cap and always finds an answer for a panel
        // a one-row composer has room for ([`layout::solve_band`]), because
        // that is the last height it tries.
        //
        // **A question takes them all** -- a permission question and the
        // model's own batch alike: upstream's question replaces the composer
        // rather than sitting under it ([`layout::fits_approval`];
        // `fx-question-40x120-q1`), so the band is solved with none.
        if self.panel.is_some() || self.ask.is_some() {
            return layout::solve_band(rows, cols, 0, activity, fixed)
                .map(|geometry| geometry.floated(self.document_rows));
        }
        // **So does the `/statusline` picker** (`input_visible` is false under
        // a compact menu, `surface_frame.zig:384-393`), and it is the one
        // occupant that gives up rows of its own rather than refuse: on a
        // screen too short for all five -- with a turn's row above it, say --
        // it shows the marked item and as much around it as fits
        // ([`statusline::Menu::rows`]), down to the one row every band has.
        if matches!(slot, Some(Slot::Statusline(_))) {
            return (1..=fixed)
                .rev()
                .find_map(|panel| layout::solve_band(rows, cols, 0, activity, panel))
                .map(|geometry| geometry.floated(self.document_rows));
        }
        // **A waiting prompt's rows, under the turn's own** -- only with the
        // turn's row above them, which is the band they are a view of, and
        // only when the screen holds them with the composer: a band that
        // cannot is solved without them, and the hint row's `queued 1` is
        // still the waiting prompt's sign.
        let banner = if activity {
            u16::try_from(self.banner_rows(cols).len()).unwrap_or(0)
        } else {
            0
        };
        (1..=wanted)
            .rev()
            .find_map(|input_rows| {
                (banner > 0)
                    .then(|| {
                        layout::solve_band_with_banner(
                            rows,
                            cols,
                            input_rows,
                            panel(input_rows),
                            banner,
                        )
                    })
                    .flatten()
                    .or_else(|| {
                        layout::solve_band(rows, cols, input_rows, activity, panel(input_rows))
                    })
            })
            .map(|geometry| geometry.floated(self.document_rows))
    }

    /// The rows a waiting prompt is shown in, above the composer, without
    /// their colour: `┋ ` and the prompt, at most two rows of it, broken at a
    /// space and at its own line breaks, the last cut one cell short for `…`
    /// when there is more (`steering_message_layout`,
    /// `render_input.zig:518-577`; `composeSteeringMessageRows`,
    /// `input_presentation.zig:58-80`).
    fn banner_rows(&self, cols: u16) -> Vec<String> {
        let width = usize::from(cols.saturating_sub(2)).max(1);
        let mut rows = Vec::new();
        for (_, _, card) in &self.prompts {
            let Some(card) = card else {
                continue;
            };
            rows.extend(
                banner_layout(card, width)
                    .into_iter()
                    .map(|row| format!("{BANNER_MARK}{row}")),
            );
        }
        rows
    }

    /// Whether this session's row numbers are claims about a screen that may
    /// not exist -- and therefore whether anything may be written at all.
    ///
    /// Two intervals, and they are the same defect a tick apart. Both are
    /// **derived**, because a flag and the geometry are two answers to one
    /// question and the way they go wrong is that they disagree; here the state
    /// *is* the two numbers and the outstanding signal.
    ///
    /// * **A `SIGWINCH` nobody has resolved yet.** The signal means the
    ///   terminal has already changed size, and the band is deliberately not
    ///   re-solved for [`super::render_request::RESIZE_DEBOUNCE`] afterwards --
    ///   so for that whole interval the geometry describes the screen that was.
    ///   Withholding is not the same as answering the signal at once, which is
    ///   what the debounce exists to prevent: nothing is measured and nothing
    ///   is re-solved, the frame is simply owed until the deadline comes round.
    /// * **A screen no band fits on**, for as long as [`Resize::TooSmall`]
    ///   leaves it so: the terminal has reported a size, and it is one the band
    ///   cannot be solved for, so the geometry stays describing the screen it
    ///   was last solved for until the screen grows again.
    ///
    /// What it buys is that nothing is written on such a screen at all. A band
    /// painted from a stale geometry addresses rows the terminal no longer has,
    /// and a terminal answers a `CUP` past its last row by **clamping** it --
    /// silently -- so the whole band lands on the bottom row on top of itself.
    /// A document append is worse, because it cannot be taken back: it is a
    /// scroll, and what leaves the top of the screen is in the terminal's
    /// native scrollback for good.
    pub(crate) fn blind(&self) -> bool {
        self.render.resize_pending() || self.screen != (self.geometry.rows, self.geometry.cols)
    }

    /// Re-solves the band for a screen that changed size.
    ///
    /// The whole of what a `SIGWINCH` moves on this surface, and the boundary
    /// is what keeps it affordable: **the band and the unfinished line, and
    /// nothing above them.** Every finished row was written into the terminal's
    /// own document once and is in its native scrollback now, where the
    /// terminal re-wrapped it by rules xfx does not model -- repainting it
    /// would mean owning a viewport this phase deliberately does not own.
    ///
    /// Three answers, and two of them change nothing:
    ///
    /// * A screen the terminal **will not describe** -- `0x0`, which a pty
    ///   whose size was never set answers successfully -- is
    ///   [`Resize::Unchanged`]. At launch that reading is a refusal and the
    ///   band is solved from 24x80 (`super::term::window_size`); here there is
    ///   already a band on a screen of a known size, and replacing it with a
    ///   fallback would move the band for a measurement that said nothing.
    /// * A screen that is **the size it already was** is `Unchanged` too: a
    ///   terminal sends a winch for a font change, and a burst sends several
    ///   for one gesture.
    /// * A screen **no band fits on** is [`Resize::TooSmall`]. Nothing is
    ///   re-solved and, above all, nothing is answered: a pending question
    ///   refused here would be a decision the user never made, taken because a
    ///   window was dragged. The size is still recorded, so the band is
    ///   re-solved when the screen grows again -- including back to exactly the
    ///   size it left, which a shell that remembered only its geometry would
    ///   report as no news.
    ///
    /// Otherwise the band is re-solved, the unfinished line is re-wrapped, and
    /// one frame is asked for. The caller owes the shadow
    /// (`super::event_loop::resolve_resize`), which is a claim about a screen
    /// that no longer exists.
    pub(crate) fn resize(&mut self, rows: u16, cols: u16) -> Resize {
        if rows == 0 || cols == 0 || (rows, cols) == self.screen {
            return Resize::Unchanged;
        }
        self.screen = (rows, cols);
        self.document_rows = u32::MAX;
        self.tail_unknown = true;
        // And a handoff still owed -- deferred behind a question or a screen
        // no band fits on -- was measured against the rows the terminal has
        // just reflowed.
        self.handoff = self.handoff.map(|_| 0);
        let Some(geometry) = self.fit(rows, cols) else {
            return Resize::TooSmall;
        };
        self.geometry = geometry;
        self.transcript.resize_unfinished(cols);
        // A frame of every row is owed anyway, so the workspace segment reads
        // `HEAD` again on it, as upstream's does on every render
        // (`tui-startup.test.ts:175-240` moves the branch and resizes).
        if self.statusline.workspace {
            self.workspace_identity.refresh();
        }
        // Every row of the band is somewhere else, and so is every column.
        self.render.request(Reason::Resize);
        // And every document row the terminal has re-wrapped by rules this
        // crate does not model: a recolour owed against those cells is owed
        // against a screen that no longer exists. The caller invalidates the
        // shadow on this same road (`super::event_loop`'s `adopt_resize`),
        // which is where the band is told.
        self.retint_debt = false;
        self.damage_untold = true;
        Resize::Repaint(geometry)
    }

    /// How deep the composer's undo and redo stacks are.
    ///
    /// Named for [`Self::edit_history`] rather than for "history", because the
    /// other field of that name is the prompt recall and a helper called
    /// `depths` would be read as its walk.
    #[cfg(test)]
    fn edit_depths(&self) -> (usize, usize) {
        (
            self.edit_history.undo_depth(),
            self.edit_history.redo_depth(),
        )
    }

    /// The transcript's own committed answer to "how many rows is the
    /// unfinished line occupying right now" ([`Transcript::tail_rows`]),
    /// forwarded rather than duplicated -- test-only, and no production
    /// state or exposure: it reads the same field a real tick already
    /// reads, it does not add one. `pub(crate)` (not private) for the same
    /// reason `Transcript::tail_rows` already is: `super::event_loop`'s
    /// tests need it and are a sibling module, not a descendant of this one.
    #[cfg(test)]
    pub(crate) fn tail_rows(&self) -> usize {
        self.transcript.tail_rows()
    }
}

#[cfg(test)]
mod tests {
    use super::super::paste::MAX_PASTE_BYTES;
    use super::*;

    use std::collections::BTreeMap;
    use std::time::Duration;

    use tokio::sync::mpsc::{Receiver, UnboundedReceiver};

    use super::super::bridge::TurnControl;
    use super::super::edit_history::DeltaKind;
    use super::super::gesture::EXIT_WINDOW;
    use super::super::pacer::{MAX_CPS, MIN_CPS};
    use super::super::question::QuestionId;
    use crate::tools::question::{QuestionEntry, QuestionOption};

    /// The loop's own tick, in milliseconds (`super::super::event_loop::TICK`),
    /// which is how often a real session gives the pacer its clock.
    const TICK_MILLIS: u64 = 8;

    use crate::config::Environment;

    /// A configuration with nothing in it, from a home and a workspace that
    /// exist and hold no settings: the shell reads one field of it and a test
    /// that hand-built the struct would stop compiling every time an unrelated
    /// key was added.
    fn config(home: &std::path::Path, workspace: &std::path::Path) -> RuntimeConfig {
        RuntimeConfig::load_with(
            &Environment::new(Some(home.to_path_buf()), BTreeMap::new()),
            workspace,
        )
        .expect("load a configuration")
    }

    /// A shell, and the runtime end of the channels it submits through.
    ///
    /// The receivers are part of the fixture rather than dropped: dropping one
    /// closes its channel, and a submission to a closed channel is a
    /// [`Rejected::Gone`] -- so a fixture that let them go would put every case
    /// here on a path no real session takes. It derefs to the shell, which is
    /// what the cases are actually about.
    struct Fixture {
        shell: Shell,
        /// What the shell handed the runtime, in order.
        sent: Receiver<TurnWork>,
        /// What it told the runtime *about* that work: a cancellation, a
        /// shutdown, or the answer to a question.
        control: UnboundedReceiver<TurnControl>,
    }

    impl std::ops::Deref for Fixture {
        type Target = Shell;

        fn deref(&self) -> &Shell {
            &self.shell
        }
    }

    impl std::ops::DerefMut for Fixture {
        fn deref_mut(&mut self) -> &mut Shell {
            &mut self.shell
        }
    }

    impl Fixture {
        /// Everything the document owes, landed onto a terminal that takes
        /// everything, as the appends it was offered.
        ///
        /// What the loop's own drain does (`super::super::event_loop`'s
        /// `commit_document`) with the one screen a shell test can promise: the
        /// cases here are about *what* is owed and at what width, and the
        /// terminal's refusals are the loop's cases rather than these.
        ///
        /// An operation that asks the terminal for nothing -- ending a line the
        /// screen already has -- is landed without being offered, so it appears
        /// here as nothing at all. That is the same thing the old queue said by
        /// never holding it.
        fn take_pending(&mut self) -> Vec<Append> {
            let mut offered = Vec::new();
            while self
                .shell
                .emit_document_front(|append| {
                    offered.push(append.clone());
                    Landed::<std::convert::Infallible>::All
                })
                .is_some()
            {}
            offered
        }

        /// Everything the document owes, as the **text** of its rows.
        ///
        /// The roles' own sequences are taken back off here, because what the
        /// cases reached through this are about is what the document *says*:
        /// which line landed, in what order, on how many rows. What it is
        /// painted in is a claim of its own, and it is made against
        /// [`Self::take_pending`] -- the untouched rows -- by the cases under
        /// "the document's colours", which is why this may not be read as
        /// "colour is not checked".
        ///
        /// A **closed** list of sequences ([`unpainted`]) rather than a strip
        /// of every escape: a sequence inside a row's *text* is text as far as
        /// the transcript is concerned, and a helper that ate those would hide
        /// exactly what the cases holding one are about.
        fn document(&mut self) -> Vec<String> {
            self.take_pending()
                .into_iter()
                .flat_map(|append| append.rows)
                .map(|row| unpainted(&row))
                .collect()
        }

        /// The same, once everything the pacer is holding has been released.
        ///
        /// The exit path's view (`super::event_loop::run`), and the one to ask
        /// for whenever the claim is about *what the document ends up saying*
        /// rather than about when it says it: answer text now goes through the
        /// pacer, so a delta applied a microsecond ago is owed to nobody yet.
        fn released(&mut self) -> Vec<String> {
            self.shell.flush_paced();
            self.document()
        }

        /// Runs the pacer's clock forward from `start` by `millis`, one tick
        /// of the loop at a time, and hands back what the document was owed.
        fn paced(&mut self, start: Instant, millis: u64) -> Vec<String> {
            let mut rows = Vec::new();
            for at in 1..=millis / TICK_MILLIS {
                self.shell
                    .settle_band(start + Duration::from_millis(at * TICK_MILLIS));
                rows.extend(self.document());
            }
            rows
        }

        /// What the band's last row says, settled first.
        ///
        /// Settled rather than read raw, because the queue's depth is the other
        /// thread's number and the row shows the one this shell last took --
        /// which is the loop's own order (`super::event_loop`).
        /// The colour is checked here rather than dropped: what the cases
        /// below are about is the *wording*, and a helper that merely stripped
        /// the palette off would let the row lose its colour -- or its closing
        /// reset, which is the one that leaks -- without a single test
        /// noticing. Asserting the wrapper and returning the middle makes every
        /// caller a witness to it.
        fn hint(&mut self) -> String {
            self.shell.settle_band(Instant::now());
            let rows = self.shell.band_rows();
            let row = rows.last().cloned().expect("the band has a hint row");
            // An empty row carries no colour, because there is nothing on it to
            // colour ([`Shell::painted`]).
            if row.is_empty() {
                return row;
            }
            let inside = row
                .strip_prefix(PALETTE.statusline())
                .and_then(|row| row.strip_suffix(PALETTE.reset()))
                .unwrap_or_else(|| panic!("the hint row was not painted in the palette: {row:?}"));
            // The permission mode's accent, taken off with the colour it
            // lifts the mode out of and nothing else.
            ["auto", "full access"]
                .iter()
                .fold(inside.to_string(), |row, mode| {
                    row.replacen(
                        &format!("{}{mode}{}", PALETTE.activity(), PALETTE.statusline()),
                        mode,
                        1,
                    )
                })
        }

        /// Plays the runtime taking one piece of work off the channel, which is
        /// where a turn begins and where the one slot becomes free again.
        fn picks_up(&mut self) -> TurnWork {
            self.sent.try_recv().expect("the runtime had work to take")
        }

        /// The next thing the shell said on the control channel, if it said
        /// anything.
        fn controlled(&mut self) -> Option<TurnControl> {
            self.control.try_recv().ok()
        }

        /// The row the caret is on, on whichever plane the session is composing
        /// for.
        ///
        /// Asked through the caret rather than by looking for the marker,
        /// because the claim every question case makes is that the two agree:
        /// the marker is what the eye reads and the caret is what the terminal
        /// says, and a test that read the marker alone would pass with the
        /// caret left in the composer.
        ///
        /// The plane is consulted for exactly the same reason. A question the
        /// band cannot show is painted on the alternate screen
        /// (`super::approval_screen`), and its caret is a row number on *that*
        /// grid; reading it off the band would be reading a coordinate against
        /// the wrong screen.
        fn marked(&self) -> String {
            if self.shell.screen_owner() == ScreenOwner::Alternate {
                let row = usize::from(self.shell.screen_cursor().0);
                return self
                    .shell
                    .screen_rows()
                    .get(row - 1)
                    .map(|row| uncoloured(row))
                    .unwrap_or_else(|| panic!("the caret is not on a row of the approval screen"));
            }
            let offset = usize::from(
                self.shell
                    .cursor()
                    .0
                    .saturating_sub(self.shell.geometry.band_top()),
            );
            // A question's rows carry upstream's sequences -- a permission
            // question's and the model's batch's alike -- and what a question
            // case reads is the words; every other row is compared colour and
            // all, as it always was.
            let painted = self.shell.panel.is_some() || self.shell.ask.is_some();
            self.shell
                .band_rows()
                .get(offset)
                .map(|row| {
                    if painted {
                        uncoloured(row)
                    } else {
                        row.clone()
                    }
                })
                .unwrap_or_else(|| panic!("the caret is not on a row of the band"))
        }
    }

    /// `row` with every escape sequence taken out: the text a reader sees.
    ///
    /// A permission question's rows are painted in upstream's sequences
    /// (`super::super::approval_view`), and a case about *what the question
    /// says* would otherwise be asserting on where a colour run happens to
    /// start.
    fn uncoloured(row: &str) -> String {
        let mut out = String::with_capacity(row.len());
        let mut rest = row;
        while !rest.is_empty() {
            if let Some(len) = super::super::pacer::escape_at(rest) {
                rest = &rest[len..];
                continue;
            }
            let mut characters = rest.chars();
            out.extend(characters.next());
            rest = characters.as_str();
        }
        out
    }

    /// Every row of `rows`, [`uncoloured`].
    fn uncoloured_rows(rows: &[String]) -> Vec<String> {
        rows.iter().map(|row| uncoloured(row)).collect()
    }

    /// A composer row holding `text`, rail and all, as the band paints it.
    fn composer_row(text: &str) -> String {
        format!("{}{RAIL}{} {text}", PALETTE.hint(), PALETTE.reset())
    }

    /// The first composer row shown when rows above it are scrolled away.
    fn composer_row_below_hidden(text: &str) -> String {
        format!(
            "{}{RAIL_HIDDEN_ABOVE}{}{text}",
            PALETTE.hint(),
            PALETTE.reset()
        )
    }

    /// The divider row a `cols`-wide band paints, colour and all.
    fn divider(cols: usize) -> String {
        format!(
            "{}{}{}",
            PALETTE.divider(),
            "\u{2500}".repeat(cols),
            PALETTE.reset()
        )
    }

    /// What a fixture's hint row says when nothing has happened on it.
    ///
    /// Not "empty": the row carries the session's identity from the first
    /// frame. [`config`] loads from a home with no settings in it and an empty
    /// environment, so this fixture is a session with **no credential**, in the
    /// compiled-in permission mode (`config::PermissionMode::default`) and on
    /// the compiled-in model (`config::DEFAULT_MODEL`) -- all three of which
    /// the row is there to say.
    const IDLE_HINT: &str = "run `xfx setup` · auto · glm-5.2";

    /// The same row with `queued N` in its place in the order.
    fn queued_hint(depth: usize) -> String {
        format!("run `xfx setup` · queued {depth} · auto · glm-5.2")
    }

    /// The same row with the double-Escape warning flush against the last
    /// column of a `cols`-wide screen.
    fn armed_hint(cols: u16) -> String {
        let padding = usize::from(cols)
            - usize::from(super::super::wrap::width(IDLE_HINT))
            - usize::from(super::super::wrap::width(ESCAPE_ARMED));
        format!(
            "{IDLE_HINT}{}{}{}{ESCAPE_ARMED}",
            " ".repeat(padding),
            PALETTE.reset(),
            PALETTE.dim()
        )
    }

    /// One hint row, painted the way the band paints it.
    fn hint_row(text: &str) -> String {
        // The permission mode is lifted out of the row's grey in the accent
        // grey, and the row's grey resumed behind it (`render.zig:235-241`).
        let accented = ["auto", "yolo"].iter().fold(text.to_string(), |row, mode| {
            row.replacen(
                &format!("{mode} \u{b7} "),
                &format!(
                    "{}{mode}{} \u{b7} ",
                    PALETTE.activity(),
                    PALETTE.statusline()
                ),
                1,
            )
        });
        format!("{}{accented}{}", PALETTE.statusline(), PALETTE.reset())
    }

    /// A document row with the sequences a **role** or a **gutter** put on it
    /// taken off, and nothing else touched.
    ///
    /// Exactly the ones the fixture's palette can add to a transcript row --
    /// the hint grey of a card's rail (`transcript::Gutter::Rail`), xfx's own
    /// lines', a notice label's (plain or bold), a turn summary's dim, and the
    /// reset that closes any of them.
    /// Anything else a row carries is part of its text and is left where it
    /// is -- with two collisions to know about: a card's bold is closed by
    /// a reset spelled the same as the palette's, so that reset goes too; and
    /// the dark dim is spelled the same as the markdown presenter's dark
    /// inline code (`markdown::theme::FX_DARK`), so a case that streams inline
    /// code has to read [`Fixture::take_pending`] instead.
    fn unpainted(row: &str) -> String {
        row.replace(PALETTE.hint(), "")
            .replace(PALETTE.notice(), "")
            .replace(PALETTE.notice_label(), "")
            .replace(PALETTE.notice_heading(), "")
            .replace(PALETTE.dim(), "")
            .replace(PALETTE.reset(), "")
    }

    /// The words of a notice that wrapped, as one line: each row's two-column
    /// continuation indent and hanging space taken off, the blank rows around
    /// it dropped, and the rows joined with the one space each wrap consumed.
    fn joined(rows: &[String]) -> String {
        rows.iter()
            .map(|row| row.trim_end().strip_prefix("  ").unwrap_or(row.trim_end()))
            .filter(|row| !row.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// What one notice that fits on a row leaves in [`Fixture::document`] on a
    /// screen that already ends on a blank row: its row, `<glyph> <topic>:
    /// <body>`, and the blank row under it ([`Shell::write_notice`]).
    fn said(notice: notice::Notice) -> Vec<String> {
        vec![notice.text(), String::new()]
    }

    /// What a cancelled turn leaves in [`Fixture::document`] on a screen that
    /// already ends on a blank row: upstream's line, and the blank row under
    /// it ([`Shell::write_cancelled`]).
    fn cancelled() -> Vec<String> {
        vec![
            "\u{25a0} Cancelled \u{b7} What can xfx do differently?".to_string(),
            String::new(),
        ]
    }

    /// What the echo of a submitted `text` leaves in [`Fixture::document`]:
    /// upstream's card -- the rail, then the text in bold -- and the blank row
    /// under it ([`Mark::Card`]). The rail's grey and the resets are taken off
    /// by [`unpainted`]; the bold is the card's text and stays.
    fn card(text: &str) -> Vec<String> {
        vec![format!("{RAIL} {BOLD}{text}"), String::new()]
    }

    /// The card's row as it is handed to the terminal: the rail in the hint
    /// grey, a space, and the submitted text in bold.
    fn card_row(text: &str) -> String {
        format!(
            "{}{RAIL}{} {BOLD}{text}{RESET}",
            PALETTE.hint(),
            PALETTE.reset()
        )
    }

    /// The palette every fixture paints in.
    ///
    /// The default one, so a test that asserts on a band row asserts on what an
    /// undecided terminal really gets.
    const PALETTE: Palette = Palette {
        mode: super::super::theme::Mode::Dark,
        depth: super::super::theme::Depth::Ansi256,
    };

    fn shell(rows: u16, cols: u16) -> Fixture {
        // Unlocked, because the fixture every case here starts from is a
        // session whose palette the **terminal** decided -- which is also the
        // only kind a theme report may move.
        shell_with(rows, cols, PALETTE, false)
    }

    fn shell_with(rows: u16, cols: u16, palette: Palette, theme_locked: bool) -> Fixture {
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let (work, sent, control) = WorkHandle::detached();
        Fixture {
            shell: Shell::new(
                &config(home.path(), workspace.path()),
                crate::tui::layout::solve(rows, cols, 1).expect("a band"),
                palette,
                theme_locked,
                work,
            )
            .anchored(),
            sent,
            control,
        }
    }

    #[test]
    fn a_session_that_has_drawn_nothing_owes_a_frame() {
        let mut shell = shell(24, 80);
        assert!(
            shell.render.begin().is_some(),
            "a fresh session did not ask for its first frame, so the band \
             would appear only once something else changed"
        );
    }

    #[test]
    fn the_band_is_the_composer_a_rule_and_a_hint_row() {
        let shell = shell(24, 80);
        let rows = shell.band_rows();
        assert_eq!(
            rows.len(),
            usize::from(shell.geometry.band_rows()),
            "the band's rows and its geometry disagree, so every row below the \
             first missing one is painted a row too high"
        );
        assert_eq!(rows[0], composer_row(""), "the composer's rail");
        assert_eq!(rows[1], "", "the rule under it is blank with no menu up");
        assert_eq!(
            rows[2],
            hint_row(IDLE_HINT),
            "the hint row does not say what a turn would be run with"
        );
    }

    #[test]
    fn a_band_row_never_outgrows_the_screen_and_so_never_loses_its_reset() {
        // The painter's clip stops at the first cell that will not fit and
        // drops everything after it -- an escape sequence included. So a hint
        // row wider than the screen would reach the terminal with its colour
        // opened and never closed, on a surface whose rows above the band are
        // the user's own document and whose next occupant, after xfx exits, is
        // the user's shell. The row is cut *before* the colour is wrapped
        // around it for exactly that reason ([`Shell::painted`]).
        let mut shell = shell(24, 20);
        shell.notice = Some(QUEUE_REJECTED);
        assert!(
            QUEUE_REJECTED.chars().count() > 20,
            "the notice now fits, so this case no longer forces the cut"
        );
        let rows = shell.band_rows();
        let hint = rows.last().expect("a hint row");
        assert!(
            super::super::wrap::width(hint) <= 20,
            "the hint row is wider than the screen, so the painter's clip will \
             take its reset off and leave the colour open: {hint:?}"
        );
        assert!(
            hint.ends_with(PALETTE.reset()),
            "the hint row left its colour open: {hint:?}"
        );
    }

    #[test]
    fn a_refusal_that_is_cut_short_does_not_paint_the_warning_beside_it() {
        // The composed case, on the row a terminal really gets. Two things are
        // true at once here: a refusal is too wide for the side of the row it
        // has, and the double-Escape warning is armed on the other side. The
        // refusal is cut -- and the sequence that puts the row's own colour
        // back is emitted **after** the cut ([`super::super::hint::Notice`]),
        // because a closing sequence carried behind the notice's text would be
        // dropped with the text the clip dropped, and the warning would then be
        // painted in the refusal's colour.
        let mut shell = shell(24, 40);
        shell.notice = Some(QUEUE_REJECTED);
        shell.escape_armed = true;
        let rows = shell.band_rows();
        let row = rows.last().expect("a hint row").clone();

        let warning = row.find(ESCAPE_ARMED).expect("the warning is on the row");
        let before = &row[..warning];
        assert!(
            before.rfind(PALETTE.statusline()) > before.rfind(PALETTE.notice()),
            "the warning is painted in the refusal's colour: {row:?}"
        );
        // Non-vacuous in both directions: the refusal really was cut, and it
        // really was painted in its own colour before it.
        assert!(
            !row.contains("was not sent"),
            "the refusal fit, so this case no longer forces the cut: {row:?}"
        );
        assert!(
            before.contains(PALETTE.notice()),
            "the refusal was never painted at all: {row:?}"
        );
        assert_eq!(
            super::super::wrap::width(&row),
            40,
            "the row is not the screen it was solved for: {row:?}"
        );
        assert!(
            row.ends_with(PALETTE.reset()),
            "the hint row left its colour open: {row:?}"
        );
    }

    #[test]
    fn the_rules_span_the_screen_they_were_solved_for() {
        // A rule of a fixed width would leave a gap on a wide terminal and run
        // off a narrow one -- and with autowrap off, running off is silent.
        // The two rules are drawn only while a menu is up, so one is opened.
        for cols in [20u16, 80, 200] {
            let mut shell = shell(24, cols);
            shell.route_bytes(b"/");
            let rows = shell.band_rows();
            let rule_rows = [1, rows.len() - 2];
            for index in rule_rows {
                // Measured in **cells**, by the painter's own rule, because the
                // row carries the palette and an escape sequence is characters
                // that cost no columns.
                assert_eq!(
                    super::super::wrap::width(&rows[index]),
                    cols,
                    "the rule on row {index} did not span a {cols}-column screen: {rows:?}"
                );
                assert!(rows[index].starts_with(PALETTE.divider()), "{rows:?}");
            }
        }
    }

    #[test]
    fn a_taller_composer_gets_one_row_each_and_the_marker_stays_on_the_first() {
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let (work, _sent, _control) = WorkHandle::detached();
        let shell = Shell::new(
            &config(home.path(), workspace.path()),
            crate::tui::layout::solve(24, 80, 4).expect("a four-row composer"),
            PALETTE,
            false,
            work,
        )
        .anchored();
        let rows = shell.band_rows();
        assert_eq!(rows.len(), 6, "four composer rows, the rule, hint");
        assert_eq!(rows[0], composer_row(""));
        assert_eq!(
            &rows[1..5],
            &[
                "".to_string(),
                "".to_string(),
                "".to_string(),
                "".to_string()
            ]
        );
    }

    #[test]
    fn the_caret_sits_after_the_prompt_marker_on_the_composers_first_row() {
        let shell = shell(24, 80);
        assert_eq!(shell.cursor(), (22, 2));
        assert_eq!(
            shell.cursor().1,
            super::super::wrap::width(RAIL) + 1,
            "the caret and the marker were measured apart, so they can drift"
        );
    }

    #[test]
    fn answer_text_is_owed_to_the_document_and_asks_for_the_frame_that_follows_it() {
        let mut shell = shell(24, 80);
        // Take the first frame the session owes, so what is asked for below is
        // the transcript's own request rather than that one.
        let _first = shell.render.begin().expect("the first frame");
        shell.write_transcript("answered");

        assert!(
            shell.render.begin().is_some(),
            "an append scrolls the band off its own rows and no frame was asked \
             for, so the band would stay scrolled until something else moved"
        );
        assert_eq!(
            shell.take_pending(),
            vec![Append {
                scroll: 1,
                rows: vec!["answered".to_string()],
                kinds: Vec::new(),
            }]
        );
    }

    #[test]
    fn a_document_write_is_owed_once() {
        // The rows are the terminal's document after they are written, and a
        // second write of the same append would scroll a second time.
        let mut shell = shell(24, 80);
        shell.write_transcript("answered");
        assert_eq!(shell.take_pending().len(), 1);
        assert!(shell.take_pending().is_empty());
    }

    #[test]
    fn two_pushes_between_two_frames_are_two_writes_in_order() {
        // Not merged: the second append's rows are measured against a screen
        // the first one already scrolled, so replacing the pair with the later
        // one loses a row of the answer.
        let mut shell = shell(24, 80);
        shell.write_transcript("first\n");
        shell.write_transcript("second");
        assert_eq!(
            shell.take_pending(),
            vec![
                Append {
                    scroll: 2,
                    rows: vec!["first".to_string(), String::new()],
                    kinds: Vec::new(),
                },
                Append {
                    scroll: 0,
                    rows: vec!["second".to_string()],
                    kinds: Vec::new(),
                },
            ]
        );
    }

    #[test]
    fn ending_a_line_that_is_already_on_the_screen_owes_nothing_and_asks_for_nothing() {
        let mut shell = shell(24, 80);
        shell.write_transcript("answered");
        shell.take_pending();
        let _asked = shell
            .render
            .begin()
            .expect("the frame the append asked for");

        shell.end_transcript_line();
        assert!(
            shell.take_pending().is_empty(),
            "a line already on the screen was written again, which scrolls a \
             blank row into the document"
        );
        assert!(
            shell.render.begin().is_none(),
            "a whole-band repaint was asked for by a write that wrote nothing"
        );
    }

    #[test]
    fn the_transcript_wraps_to_the_screen_the_band_was_solved_for() {
        // A transcript measured against a different width wraps where the
        // terminal does not, and every row after the first is placed wrong.
        let mut shell = shell(24, 20);
        shell.write_transcript(&"x".repeat(25));
        assert_eq!(
            shell.take_pending(),
            vec![Append {
                scroll: 2,
                rows: vec!["x".repeat(20), "x".repeat(5)],
                kinds: Vec::new(),
            }]
        );
    }

    #[test]
    fn text_owed_to_the_document_is_wrapped_for_the_screen_it_is_written_on() {
        // The same claim across a resize the text lived through. A write is
        // queued on this side of the divider and lands when the loop has a
        // screen to write it on (`super::event_loop`'s `commit_document`); a
        // resize in between is a screen the text has never been on, so it is
        // measured against the one it really lands on. Rows frozen at the old
        // width are clipped by the painter, and this phase never repaints a
        // document row.
        let mut shell = shell(24, 80);
        shell.write_transcript(&"x".repeat(25));

        assert_eq!(shell.resize(24, 20), Resize::Repaint(shell.geometry));

        assert_eq!(
            shell.take_pending(),
            vec![Append {
                scroll: 2,
                rows: vec!["x".repeat(20), "x".repeat(5)],
                kinds: Vec::new(),
            }],
            "the text was written as the one 25-column row the wider screen \
             would have given it"
        );
    }

    #[test]
    fn a_line_that_ended_on_its_own_break_is_not_ended_a_second_time() {
        // The empty-tail case, which the projection has to answer the same way
        // the committed state used to: text ending in a break leaves the row
        // its successor starts on open, so finishing the line there writes
        // nothing -- and must ask for no frame either. The alternative is a
        // blank row in the document for every notice that follows a paragraph.
        let mut shell = shell(24, 80);
        let _first = shell.render.begin().expect("the first frame");
        shell.write_transcript("abc\n");
        assert_eq!(
            shell.take_pending(),
            vec![Append {
                scroll: 2,
                rows: vec!["abc".to_string(), String::new()],
                kinds: Vec::new(),
            }]
        );
        let _asked = shell.render.begin().expect("the frame the write asked for");

        shell.finish_document_line();
        assert!(
            shell.take_pending().is_empty(),
            "a line that ended on its own break was given a second blank row"
        );
        assert!(
            shell.render.begin().is_none(),
            "a whole-band repaint was asked for by a write that wrote nothing"
        );
    }

    // -----------------------------------------------------------------------
    // the composer
    // -----------------------------------------------------------------------

    #[test]
    fn what_is_typed_appears_in_the_composer_with_the_caret_after_it() {
        let mut shell = shell(24, 80);
        // The frame the session owes for existing, so that what is asked for
        // below is what the typing asked for.
        let _first = shell.render.begin().expect("the first frame");

        shell.route_bytes("hello \u{d55c}\u{ae00}".as_bytes());
        assert_eq!(shell.band_rows()[0], composer_row("hello \u{d55c}\u{ae00}"));
        // Two cells of gutter, six of "hello ", and two apiece for the glyphs.
        assert_eq!(shell.cursor(), (22, 12));
        assert!(
            shell.render.begin().is_some(),
            "typing asked for no frame, so the band would go on showing a \
             composer the session no longer has"
        );
    }

    #[test]
    fn a_caret_that_moved_asks_for_the_frame_that_moves_it() {
        // The band is repainted whole, and the caret is placed by the frame:
        // an arrow key that changed no text still changes where the terminal's
        // own cursor belongs.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"ab");
        let _typed = shell.render.begin().expect("the frame the typing owed");
        shell.route_bytes(b"\x1b[D");
        assert_eq!(shell.cursor(), (22, 3));
        assert!(
            shell.render.begin().is_some(),
            "the caret moved and no frame was asked for, so the terminal's \
             cursor would stay where the last frame left it"
        );
    }

    #[test]
    fn a_control_byte_that_means_nothing_here_is_not_typed_into_the_composer() {
        // The decoder's table is closed, and this is the half of that policy
        // the composer keeps: a tab, an unnamed C0 and a C1 scalar are
        // keystrokes with no binding, and a composer that took them would put a
        // control the terminal obeys into the transcript on submit.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"a\tb\x00c\xc2\x9bd");
        assert_eq!(shell.band_rows()[0], composer_row("abcd"));
    }

    #[test]
    fn ctrl_d_leaves_an_empty_composer_and_deletes_from_one_with_text_in_it() {
        let mut shell = shell(24, 80);
        shell.route_bytes(b"hello\x1b[A");
        assert!(!shell.leaving(), "an ordinary keystroke ended the session");

        // Home, then Ctrl-D: a forward delete, and the session stays.
        shell.route_bytes(&[0x01, 0x04]);
        assert!(
            !shell.leaving(),
            "Ctrl-D threw away a draft instead of deleting a character"
        );
        assert_eq!(shell.band_rows()[0], composer_row("ello"));

        shell.route_bytes(&[0x05, 0x15]);
        assert_eq!(
            shell.band_rows()[0],
            composer_row(""),
            "Ctrl-U left text behind"
        );
        shell.route_bytes(&[0x04]);
        assert!(shell.leaving(), "Ctrl-D did not leave an empty composer");
    }

    #[test]
    fn ctrl_d_leaves_from_the_middle_of_a_burst_as_well_as_from_its_own_read() {
        // A burst that ends a session arrives as one read, and a loop that only
        // looked at the first byte of it would keep waiting for input the
        // terminal has already delivered.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"\x04def");
        assert!(shell.leaving());
    }

    #[test]
    fn a_sequence_split_across_two_reads_is_still_one_keystroke() {
        // Why the decoder lives in the shell rather than in the loop: an arrow
        // key that arrived in two reads must not become an `ESC` and three
        // characters in the composer.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"ab\x1b");
        shell.route_bytes(b"[D");
        shell.route_bytes(b"X");
        assert_eq!(shell.band_rows()[0], composer_row("aXb"));
    }

    #[test]
    fn a_bare_escape_that_has_gone_quiet_stops_swallowing_the_next_keystroke() {
        // The turn's own flush, and the only thing that resolves an `ESC` the
        // user pressed on its own: until it does, the decoder is still waiting
        // to see whether an arrow key is arriving, and the next byte would be
        // read as the Alt-something this phase binds nothing to.
        let mut shell = shell(24, 80);
        shell.route_bytes(&[0x1b]);
        shell.settle_input(Instant::now() + Decoder::ESC_TIMEOUT);
        shell.route_bytes(b"a");
        assert_eq!(
            shell.band_rows()[0],
            composer_row("a"),
            "the keystroke after a settled Escape was eaten as a modifier"
        );
    }

    #[test]
    fn submitting_clears_the_composer_and_leaves_the_text_in_the_document() {
        let mut shell = shell(24, 80);
        let _first = shell.render.begin().expect("the first frame");
        shell.route_bytes(b"ask me\r");

        assert_eq!(
            shell.band_rows()[0],
            composer_row(""),
            "the composer kept the text"
        );
        assert_eq!(
            shell.take_pending(),
            vec![
                Append {
                    scroll: 1,
                    rows: vec![card_row("ask me")],
                    kinds: Vec::new(),
                },
                Append {
                    scroll: 1,
                    rows: vec![String::new()],
                    kinds: Vec::new(),
                },
            ]
        );
        assert!(
            shell.render.begin().is_some(),
            "a submission asked for no frame, so the band would keep showing \
             text the composer no longer holds"
        );
    }

    #[test]
    fn a_write_the_terminal_refused_stays_in_front_of_the_ones_owed_since() {
        // The claim `restore_pending` used to keep, now kept by the queue's own
        // shape: the loop can stop part-way through what is owed when the
        // terminal refuses a write (`super::event_loop::commit_document`), and
        // the tick that discovers it has already applied that tick's events. A
        // document is a sequence -- a row that lands after one queued behind it
        // is as wrong as a row that never lands -- so the refused one is still
        // the oldest thing owed.
        let mut shell = shell(24, 80);
        shell.write_transcript("older\n");
        let refused = shell.shell.emit_document_front(|append| {
            assert_eq!(
                append.rows.first().map(String::as_str),
                Some("older"),
                "the oldest write is not what was offered: {append:?}"
            );
            Landed::None(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "not now",
            ))
        });
        assert!(
            matches!(refused, Some(Err(_))),
            "a refusal was not reported to the loop: {refused:?}"
        );
        shell.write_transcript("newer\n");

        // The text of each write, in the order the document is owed them. Only
        // the first row of each is named: a break also opens the empty line
        // after it, which is a property of `Transcript` and not of this order.
        assert_eq!(
            shell
                .take_pending()
                .into_iter()
                .map(|append| append.rows.first().cloned().unwrap_or_default())
                .collect::<Vec<_>>(),
            vec!["older".to_string(), "newer".to_string()]
        );
    }

    #[test]
    fn submitting_an_empty_composer_writes_nothing_at_all() {
        // Otherwise every stray Return puts a blank row in the terminal's
        // document, and a document row cannot be taken back.
        let mut shell = shell(24, 80);
        shell.route_bytes(&[0x0d]);
        assert!(shell.take_pending().is_empty());
    }

    #[test]
    fn a_newline_is_composed_rather_than_submitted() {
        // C-j is the multi-line composer's whole existence: it has to reach the
        // editor as text, not as the Return that sends what has been written.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"first\x0asecond");
        assert!(
            shell.take_pending().is_empty(),
            "a newline submitted itself"
        );
        assert_eq!(
            &shell.band_rows()[0..2],
            &[composer_row("first"), composer_row("second")]
        );
    }

    // -----------------------------------------------------------------------
    // a paste
    // -----------------------------------------------------------------------

    #[test]
    fn a_pasted_newline_is_content_rather_than_the_key_that_submits() {
        // The whole reason the frame exists: without it a pasted stack trace
        // is one prompt per line, sent before the user can react and with real
        // side effects behind them.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"\x1b[200~first line\nsecond line\x1b[201~");

        assert!(
            shell.take_pending().is_empty(),
            "a paste submitted itself into the document"
        );
        assert!(
            shell.sent.try_recv().is_err(),
            "a pasted newline sent a prompt to the runtime"
        );
        assert_eq!(
            &shell.band_rows()[0..2],
            &[composer_row("first line"), composer_row("second line")]
        );
    }

    #[test]
    fn a_pasted_cancel_byte_and_escape_sequence_never_become_keys() {
        // A `0x03` between the markers must not cancel a turn or throw the
        // draft away, and an `ESC [ A` must not be obeyed as an arrow: the
        // decoder never offers either as a key, and the filter drops the bytes
        // that would be obeyed on the way out again.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"\x1b[200~a\x03b\x1b[Ac\x1b[201~");

        assert_eq!(
            shell.band_rows()[0],
            composer_row("ab[Ac"),
            "a control byte inside a paste was taken as the key it looks like"
        );
        assert!(
            shell.controlled().is_none(),
            "a pasted 0x03 asked the runtime to stop"
        );
    }

    #[test]
    fn a_large_paste_is_a_summary_on_the_screen_and_the_whole_text_on_the_wire() {
        // 1800 codepoints painted into a band is a band that has eaten the
        // screen -- and every later keystroke re-wraps whatever the composer
        // holds. The summary is what is shown; the text is what is sent.
        let mut shell = shell(24, 80);
        let block = format!("{}\n{}", "y".repeat(900), "z".repeat(900));
        shell.route_bytes(b"\x1b[200~");
        shell.route_bytes(block.as_bytes());
        shell.route_bytes(b"\x1b[201~");

        assert_eq!(
            shell.band_rows()[0],
            composer_row("[Pasted text #1, 2 lines]"),
            "the composer was given the block rather than a summary of it"
        );

        shell.route_bytes(&[0x0d]);
        assert_eq!(
            shell.picks_up(),
            TurnWork::Submit(block),
            "the summary was sent in place of what was pasted"
        );
        assert_eq!(
            shell.document(),
            card("[Pasted text #1, 2 lines]"),
            "the document echoed the whole block back at the user"
        );
    }

    #[test]
    fn a_summary_typed_by_hand_after_its_paste_was_sent_is_only_words() {
        // A summary is ordinary text in a composer, so a user can type one.
        // A block that outlived the draft it was pasted into would turn that
        // typing into a paste they sent a turn ago.
        let mut shell = shell(24, 80);
        let block = "y".repeat(1200);
        shell.route_bytes(b"\x1b[200~");
        shell.route_bytes(block.as_bytes());
        shell.route_bytes(b"\x1b[201~");
        shell.route_bytes(&[0x0d]);
        assert_eq!(
            shell.picks_up(),
            TurnWork::Submit(block),
            "the paste never reached the runtime, so this case proves nothing"
        );
        let _echo = shell.document();

        shell.route_bytes(b"[Pasted text #1, 1 lines]");
        shell.route_bytes(&[0x0d]);
        assert_eq!(
            shell.picks_up(),
            TurnWork::Submit("[Pasted text #1, 1 lines]".to_string()),
            "a block the draft no longer held was expanded into a later prompt"
        );
    }

    #[test]
    fn a_summary_the_user_typed_a_second_copy_of_is_only_words() {
        // The words of a summary are on the screen where the user can read and
        // retype them. **The block is the span the paste made**, so a second
        // copy of the name is text wherever it is: an expansion that matched
        // the words would send the paste as many times as the draft says its
        // name.
        let mut shell = shell(24, 80);
        let block = "y".repeat(1200);
        shell.route_bytes(b"\x1b[200~");
        shell.route_bytes(block.as_bytes());
        shell.route_bytes(b"\x1b[201~");
        shell.route_bytes(b" and [Pasted text #1, 1 lines]");
        shell.route_bytes(&[0x0d]);

        assert_eq!(
            shell.picks_up(),
            TurnWork::Submit(format!("{block} and [Pasted text #1, 1 lines]")),
            "a summary the user typed was sent as a second copy of the paste"
        );
    }

    #[test]
    fn a_summary_already_in_the_draft_is_not_the_one_the_paste_stands_behind() {
        // The draft held those words *before* anything was pasted, so no span
        // covers them -- and the run the paste itself put there is the block,
        // whichever of the two a search for the name would have found first.
        let mut shell = shell(24, 80);
        let block = "y".repeat(1200);
        shell.route_bytes(b"[Pasted text #1, 1 lines] ");
        shell.route_bytes(b"\x1b[200~");
        shell.route_bytes(block.as_bytes());
        shell.route_bytes(b"\x1b[201~");
        shell.route_bytes(&[0x0d]);

        assert_eq!(
            shell.picks_up(),
            TurnWork::Submit(format!("[Pasted text #1, 1 lines] {block}")),
            "the words the user had already typed were expanded in the \
             placeholder's place"
        );
    }

    #[test]
    fn a_copy_of_a_summary_after_the_caret_is_not_the_placeholder_either() {
        // The other side of the same question, and the one the old name-based
        // model had to count copies to answer: the paste lands *in front of*
        // words that already look like its summary, so the first copy in the
        // draft is the block and the second is text. A span needs no counting
        // -- it is the run the insertion made, and the words after it were
        // never it.
        let mut shell = shell(24, 80);
        let block = "y".repeat(1200);
        shell.route_bytes(b"[Pasted text #1, 1 lines]");
        shell.route_bytes(&[0x01]); // C-a: the caret goes back to the start
        shell.route_bytes(b"\x1b[200~");
        shell.route_bytes(block.as_bytes());
        shell.route_bytes(b"\x1b[201~");
        shell.route_bytes(&[0x0d]);

        assert_eq!(
            shell.picks_up(),
            TurnWork::Submit(format!("{block}[Pasted text #1, 1 lines]")),
            "the copies were counted in the whole draft rather than in front \
             of the caret"
        );
    }

    #[test]
    fn a_paste_that_would_make_the_prompt_oversized_is_refused() {
        // The two halves of a prompt have to be budgeted **together**. What a
        // collapsed block puts on the screen is 25 bytes, so a draft the user
        // can read the whole of can be standing in front of megabytes -- and
        // two ceilings that are 8 MiB each are one prompt of 16.
        let mut shell = shell(24, 80);
        // Put in whole rather than a keystroke at a time: the composer
        // re-wraps on every edit, so half a megabyte of keystrokes would be
        // quadratic here. This is the same call `type_character` makes.
        let typed = "x".repeat(500_000);
        assert!(
            shell.editor.insert(&typed).is_some(),
            "the draft could not be set up"
        );

        let block = "y".repeat(8_000_000);
        shell.route_bytes(b"\x1b[200~");
        shell.route_bytes(block.as_bytes());
        shell.route_bytes(b"\x1b[201~");
        let draft = shell.editor.text().len();
        let notice = shell.notice;

        shell.route_bytes(&[0x0d]);
        let TurnWork::Submit(prompt) = shell.picks_up() else {
            panic!("the draft was not submitted");
        };
        assert!(
            prompt.len() <= MAX_PASTE_BYTES,
            "the prompt is {} bytes, past the {MAX_PASTE_BYTES}-byte budget",
            prompt.len()
        );
        assert_eq!(draft, typed.len(), "the refused paste changed the draft");
        assert_eq!(
            notice,
            Some(PASTE_REFUSED),
            "the paste was dropped without a word"
        );
    }

    #[test]
    fn typing_stops_at_the_budget_the_drafts_hidden_blocks_are_using() {
        // The other way to the same oversized prompt: paste a block, then go
        // on typing. The composer's own cap counts what is on the screen, and
        // what is on the screen is 25 bytes standing for a great deal more.
        let mut shell = shell(24, 80);
        let block = "y".repeat(8_000_000);
        shell.route_bytes(b"\x1b[200~");
        shell.route_bytes(block.as_bytes());
        shell.route_bytes(b"\x1b[201~");
        assert_eq!(
            shell.editor.text(),
            "[Pasted text #1, 1 lines]",
            "the block was not collapsed, so this case proves nothing"
        );

        // Enough that the draft and the block it hides are together past the
        // budget, put in whole for the reason above.
        assert!(
            shell.editor.insert(&"x".repeat(400_000)).is_some(),
            "the draft could not be set up"
        );
        let full = shell.editor.text().len();

        shell.route_bytes(b"a");
        assert_eq!(
            shell.editor.text().len(),
            full,
            "a keystroke landed past the budget the draft's hidden block is \
             already using"
        );
    }

    #[test]
    fn a_summary_backspaced_away_gives_its_budget_back() {
        // Phase 1 lets a user backspace into a summary -- it is text in the
        // composer, not an atomic entity -- and nothing about that calls
        // `forget`. A block whose name is no longer anywhere in the draft can
        // never reach the prompt, so a budget that went on charging for it
        // would leave an **empty** composer that refuses to be typed in.
        let mut shell = shell(24, 80);
        // The whole budget in one block, so that charging for it after it can
        // no longer be sent refuses even a single character.
        // The whole budget less the name that will stand in for it, which is
        // the largest block a draft can hold: a collapsed paste is charged its
        // text and its name both.
        let block = "y".repeat(MAX_PASTE_BYTES - "[Pasted text #1, 1 lines]".len());
        shell.route_bytes(b"\x1b[200~");
        shell.route_bytes(block.as_bytes());
        shell.route_bytes(b"\x1b[201~");
        let summary = shell.editor.text().to_string();
        assert_eq!(summary, "[Pasted text #1, 1 lines]");

        for _ in 0..summary.chars().count() {
            shell.route_bytes(&[0x7f]);
        }
        assert!(shell.editor.is_empty(), "the summary is still in the draft");

        shell.route_bytes(b"a");
        assert_eq!(
            shell.editor.text(),
            "a",
            "an empty composer refused a keystroke, for megabytes that can no \
             longer reach the prompt"
        );
    }

    #[test]
    fn a_composed_newline_is_weighed_like_any_other_keystroke() {
        // `C-j` is the one editing action that *adds* text, so it is the one
        // that has to ask the budget the same question a typed character does.
        let mut shell = shell(24, 80);
        let block = "y".repeat(8_000_000);
        shell.route_bytes(b"\x1b[200~");
        shell.route_bytes(block.as_bytes());
        shell.route_bytes(b"\x1b[201~");
        let summary = shell.editor.text().len();

        assert!(
            shell
                .editor
                .insert(&"x".repeat(MAX_PASTE_BYTES - block.len() - summary))
                .is_some(),
            "the draft could not be set up"
        );
        let full = shell.editor.text().len();

        shell.route_bytes(&[0x0a]); // C-j
        assert_eq!(
            shell.editor.text().len(),
            full,
            "a composed newline landed past the budget the draft's hidden \
             block is already using"
        );
    }

    #[test]
    fn a_keystroke_in_front_of_a_name_that_survives_it_is_still_weighed_against_it() {
        // The other side of the prospective question. This keystroke lands
        // *before* the summary rather than inside it, so the name survives and
        // its megabytes are still going to be sent -- and the draft is at the
        // cap. A prospective draft that only looked at the text in front of the
        // caret would not find the name and would wave the keystroke through.
        let mut shell = shell(24, 80);
        let block = "y".repeat(8_000_000);
        shell.route_bytes(b"\x1b[200~");
        shell.route_bytes(block.as_bytes());
        shell.route_bytes(b"\x1b[201~");
        let summary = shell.editor.text().len();
        assert!(
            shell
                .editor
                .insert(&"x".repeat(MAX_PASTE_BYTES - block.len() - summary))
                .is_some(),
            "the draft could not be set up"
        );
        let full = shell.editor.text().len();

        shell.route_bytes(&[0x01]); // C-a: in front of the name
        shell.route_bytes(b"z");
        assert_eq!(
            shell.editor.text().len(),
            full,
            "a keystroke landed past the budget, in front of a name whose \
             block it does not release"
        );
    }

    #[test]
    fn a_paste_in_front_of_a_name_that_survives_it_is_still_weighed_against_it() {
        // The other side of the paste's prospective question, and the same one
        // `a_keystroke_in_front_of_a_name_that_survives_it_is_still_weighed_against_it`
        // asks of a keystroke. This paste lands *before* the summary rather
        // than inside it, so the name survives and its megabytes are still
        // going to be sent -- and the draft is at the cap.
        let mut shell = shell(24, 80);
        let block = "y".repeat(MAX_PASTE_BYTES - "[Pasted text #1, 1 lines]".len());
        shell.route_bytes(b"\x1b[200~");
        shell.route_bytes(block.as_bytes());
        shell.route_bytes(b"\x1b[201~");
        let full = shell.editor.text().to_string();

        shell.route_bytes(&[0x01]); // C-a: in front of the name
        shell.route_bytes(b"\x1b[200~short\x1b[201~");
        assert_eq!(
            shell.editor.text(),
            full,
            "a paste landed past the budget, in front of a name whose block it \
             does not release"
        );
    }

    /// A collapsed block in the composer, and the text it stands for.
    ///
    /// The paste is driven through the byte path rather than built, because
    /// what item 16 is about is what the keys do to it afterwards.
    fn collapsed(shell: &mut Shell, lines: usize) -> String {
        let text = if lines > 1 {
            let mut text = "y".repeat(1200);
            for _ in 1..lines {
                text.push('\n');
                text.push('y');
            }
            text
        } else {
            "y".repeat(1200)
        };
        shell.route_bytes(b"\x1b[200~");
        shell.route_bytes(text.as_bytes());
        shell.route_bytes(b"\x1b[201~");
        text
    }

    #[test]
    fn a_backspace_at_a_collapsed_pastes_edge_takes_the_whole_block() {
        // The narrowing item 16 closes. Phase 1 edited the summary's last
        // character, which left a damaged name on the screen standing for
        // nothing.
        let mut shell = shell(24, 80);
        let _block = collapsed(&mut shell, 1);
        assert_eq!(shell.editor.text(), "[Pasted text #1, 1 lines]");

        shell.route_bytes(&[0x7f]);
        assert!(
            shell.editor.is_empty(),
            "the backspace edited the name instead of removing the block: {:?}",
            shell.editor.text()
        );
        shell.route_bytes(b"hello");
        shell.route_bytes(&[0x0d]);
        assert_eq!(
            shell.picks_up(),
            TurnWork::Submit("hello".to_string()),
            "the removed block was sent anyway"
        );
    }

    #[test]
    fn a_forward_delete_at_a_collapsed_pastes_left_edge_takes_the_whole_block() {
        let mut shell = shell(24, 80);
        let _block = collapsed(&mut shell, 1);
        shell.route_bytes(&[0x01]); // C-a, to the left edge
        shell.route_bytes(&[0x04]); // C-d is a forward delete with text under it
        assert!(
            shell.editor.is_empty(),
            "the delete edited the name instead: {:?}",
            shell.editor.text()
        );
    }

    #[test]
    fn the_caret_steps_over_a_collapsed_paste_as_one_unit() {
        // One `Left` from the right edge is in front of the *whole* summary, so
        // the keystroke after it lands beside the block rather than in its
        // name.
        let mut shell = shell(24, 80);
        let block = collapsed(&mut shell, 1);
        shell.route_bytes(b"\x1b[D");
        shell.route_bytes(b"!");
        assert_eq!(
            shell.editor.text(),
            "![Pasted text #1, 1 lines]",
            "a left step landed inside the name"
        );
        shell.route_bytes(&[0x0d]);
        assert_eq!(
            shell.picks_up(),
            TurnWork::Submit(format!("!{block}")),
            "the block did not survive a keystroke beside it"
        );
    }

    #[test]
    fn a_recalled_paste_comes_back_as_a_block_with_a_fresh_number() {
        // The whole of the recall narrowing item 15 wrote down: an entry
        // carries the blocks its summaries named, and a recall renumbers them
        // so that no two live blocks answer to one name.
        let mut shell = shell(24, 80);
        let block = collapsed(&mut shell, 1);
        shell.route_bytes(&[0x0d]);
        assert_eq!(shell.picks_up(), TurnWork::Submit(block.clone()));

        shell.route_bytes(b"\x1b[A");
        assert_eq!(
            shell.editor.text(),
            "[Pasted text #2, 1 lines]",
            "the recalled summary kept a number this session had already used"
        );
        shell.route_bytes(&[0x0d]);
        assert_eq!(
            shell.picks_up(),
            TurnWork::Submit(block),
            "a recalled summary was sent as the words it looks like"
        );
    }

    #[test]
    fn a_draft_holds_far_more_blocks_than_the_old_cap_and_sends_every_one() {
        // `MAX_RETAINED_BLOCKS` was a bound on the *time* a keystroke cost,
        // because the old bookkeeping re-read the draft once per block. The
        // spans cost arithmetic, so the bound is gone -- and what proves it is
        // a draft holding many times the old cap whose every block is sent.
        const BLOCKS: usize = 100;
        let mut shell = shell(24, 80);
        let block = "y".repeat(1001);
        for _ in 0..BLOCKS {
            shell.route_bytes(b"\x1b[200~");
            shell.route_bytes(block.as_bytes());
            shell.route_bytes(b"\x1b[201~");
        }
        assert_eq!(shell.notice, None, "a paste past the old cap was refused");
        assert!(
            shell
                .editor
                .text()
                .contains(&format!("[Pasted text #{BLOCKS}, 1 lines]")),
            "the last block is not in the draft"
        );
        shell.route_bytes(&[0x0d]);
        let TurnWork::Submit(prompt) = shell.picks_up() else {
            panic!("nothing was submitted");
        };
        assert_eq!(
            prompt.matches(&block).count(),
            BLOCKS,
            "not every block reached the prompt"
        );
        assert!(
            !prompt.contains("Pasted text"),
            "a summary was sent in place of its block"
        );
    }

    #[test]
    fn a_history_entry_carries_the_blocks_its_summaries_named() {
        // Two lines with a block each: the walk back has to put the right one
        // behind the right summary, and neither may be the other's.
        let mut shell = shell(24, 80);
        let first = collapsed(&mut shell, 1);
        shell.route_bytes(&[0x0d]);
        assert_eq!(shell.picks_up(), TurnWork::Submit(first.clone()));
        let second = collapsed(&mut shell, 2);
        shell.route_bytes(&[0x0d]);
        assert_eq!(shell.picks_up(), TurnWork::Submit(second.clone()));

        shell.route_bytes(&[0x10]); // C-p: the newest line
        assert_eq!(
            shell.editor.expanded(),
            second,
            "the newest line came back without the block its summary named"
        );
        shell.route_bytes(&[0x10]); // C-p: the one before it
        assert_eq!(
            shell.editor.expanded(),
            first,
            "the older line came back with the newer line's block"
        );
        // Asked of the composer rather than of a third submission, because the
        // runtime holds two pieces of work at once (`super::worker::WORK_LIMIT`)
        // and both are still in hand: what `expanded` answers is exactly what
        // `submit` would send.
        assert_eq!(
            shell.editor.text(),
            "[Pasted text #4, 1 lines]",
            "the walk handed back numbers this session had already spent"
        );
    }

    #[test]
    fn a_draft_with_blank_edges_sends_the_block_and_the_trimming_both() {
        // The classifier trims a submitted line
        // (`crate::interactive::classify`) and the spans are runs of the draft
        // it trimmed, so the two have to be reconciled somewhere: expanding the
        // *trimmed* line with the untrimmed line's spans would splice the block
        // two bytes out of place. Expanded whole and cut instead, which is
        // exact because what is trimmed is whitespace and a summary is not.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"  ");
        let block = collapsed(&mut shell, 1);
        shell.route_bytes(b"  ");
        assert_eq!(shell.editor.text(), "  [Pasted text #1, 1 lines]  ");

        shell.route_bytes(&[0x0d]);
        assert_eq!(
            shell.picks_up(),
            TurnWork::Submit(block),
            "the prompt is the block with the line's own blank edges trimmed"
        );
    }

    #[test]
    fn one_framed_paste_is_one_transaction_however_many_reads_it_arrived_in() {
        // **The boundary an undo takes**, fixed at the only moment that knows
        // it. A paste is one gesture and a great many bytes, and the bytes
        // arrive in as many reads as the terminal feels like: a boundary
        // inferred later from the buffer would be a boundary per read, or per
        // grapheme, and an undo built on it would take a megabyte back a
        // character at a time. Migrated off the one-entry seam onto the delta
        // the history really holds, which is now the thing `C-_` walks.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"\x1b[200~");
        let chunk = vec![b'y'; 64];
        for _ in 0..40 {
            shell.route_bytes(&chunk);
        }
        shell.route_bytes(b"\x1b[201~");

        assert_eq!(
            shell.edit_depths(),
            (1, 0),
            "a framed paste did not record exactly one entry"
        );
        let delta = shell.edit_history.last().expect("the paste's delta");
        assert_eq!(delta.kind(), DeltaKind::Paste);
        assert_eq!(delta.at(), 0, "the draft before the paste was not empty");
        assert_eq!(delta.removed(), "", "a paste removed nothing");
        assert_eq!(delta.inserted(), "[Pasted text #1, 1 lines]");
        let entity = &delta.inserted_entities()[0];
        assert_eq!(entity.range(), 0..delta.inserted().len());
        assert_eq!(
            entity.text().len(),
            40 * 64,
            "the delta's entity does not carry the whole paste"
        );

        // And the next edit is its **own** entry rather than an extension of
        // this one: one paste is one thing to undo, and the keystroke after it
        // is another.
        shell.route_bytes(b"!");
        assert_eq!(shell.edit_depths(), (2, 0));
        assert_eq!(
            shell.edit_history.last().map(Delta::kind),
            Some(DeltaKind::Ordinary),
            "a keystroke after a paste was folded into the paste's transaction"
        );

        // The whole point of the boundary, driven: one `C-_` takes the
        // keystroke, the second takes the paste **whole**.
        shell.route_bytes(&[0x1f]);
        assert_eq!(shell.editor.text(), "[Pasted text #1, 1 lines]");
        shell.route_bytes(&[0x1f]);
        assert_eq!(shell.editor.text(), "");
        assert!(
            shell.editor.entities().is_empty(),
            "the block outlived the summary it stands for"
        );
    }

    #[test]
    fn a_caret_move_leaves_the_paste_boundary_standing_and_an_edit_takes_it_down() {
        // **A move is not an edit.** A keystroke that only moved the caret has
        // not changed the text, so it records nothing: a history that grew an
        // entry per `Left` would take the paste back a grapheme at a time --
        // which is the whole thing the boundary exists to prevent -- and reading
        // a pasted line before deciding to undo it would be what made the undo
        // impossible.
        let mut shell = shell(24, 80);
        // Two rows, so that `Up` and `Down` have somewhere to go inside the
        // draft: typed **before** the paste, because typing is an edit and the
        // entry this case is about is the paste's.
        shell.route_bytes(&[b'x'; 100]);
        let _block = collapsed(&mut shell, 1);
        assert_eq!(
            shell.edit_history.last().map(Delta::kind),
            Some(DeltaKind::Paste),
            "the paste did not record an entry, so this case proves nothing"
        );
        let depths = shell.edit_depths();

        // Every key the composer binds that moves the caret and nothing else.
        // `Up`/`Down` are here twice over: once inside a two-row draft, where
        // they move, and once at its edges, where they reach for the history,
        // find none and change nothing at all.
        for (name, keys) in [
            ("Left", &b"\x1b[D"[..]),
            ("Right", &b"\x1b[C"[..]),
            ("Home", &b"\x1b[H"[..]),
            ("End", &b"\x1b[F"[..]),
            ("WordLeft", &b"\x1b[1;5D"[..]),
            ("WordRight", &b"\x1b[1;5C"[..]),
            ("C-a", &[0x01][..]),
            ("C-e", &[0x05][..]),
            ("Up", &b"\x1b[A"[..]),
            ("Down", &b"\x1b[B"[..]),
            ("Up at the first row", &b"\x1b[A\x1b[A\x1b[A"[..]),
            ("Down at the last row", &b"\x1b[B\x1b[B\x1b[B"[..]),
        ] {
            shell.route_bytes(keys);
            assert_eq!(
                shell.edit_history.last().map(Delta::kind),
                Some(DeltaKind::Paste),
                "{name} recorded a history entry for a caret that only moved"
            );
            assert_eq!(
                shell.edit_depths(),
                depths,
                "{name} changed the depth of a history nothing was edited into"
            );
        }

        // And an edit -- any edit -- is its own entry on top of it.
        shell.route_bytes(b"!");
        assert_eq!(
            shell.edit_history.last().map(Delta::kind),
            Some(DeltaKind::Ordinary),
            "a keystroke after a paste recorded nothing of its own"
        );
    }

    #[test]
    fn every_kind_of_edit_takes_the_paste_boundary_down() {
        // The other half, once per family, because "an edit is its own entry" is
        // a claim about the funnel every text change goes through rather than
        // about the one keystroke that is easiest to test.
        let edits: [(&str, &[u8]); 5] = [
            ("a typed character", b"!"),
            ("a backspace", &[0x7f]),
            ("a kill to the start", &[0x15]),
            ("a composed newline", &[0x0a]),
            ("a word delete", &[0x17]),
        ];
        for (name, keys) in edits {
            let mut shell = shell(24, 80);
            let _block = collapsed(&mut shell, 1);
            assert_eq!(
                shell.edit_history.last().map(Delta::kind),
                Some(DeltaKind::Paste),
                "{name}: the paste did not record an entry"
            );
            shell.route_bytes(keys);
            assert_eq!(
                shell.edit_depths(),
                (2, 0),
                "{name} was folded into the paste's transaction"
            );
            assert!(
                !matches!(
                    shell.edit_history.last().map(Delta::kind),
                    Some(DeltaKind::Paste)
                ),
                "{name} left the paste as the newest entry"
            );
        }
    }

    // -----------------------------------------------------------------------
    // the composer's own history
    // -----------------------------------------------------------------------
    //
    // Driven through the real recording funnel -- `route_bytes` into
    // `Shell::act` -- rather than against a detached `EditHistory`, because
    // every claim here is about *which* keystrokes reach it and with what.

    #[test]
    fn an_undo_after_a_submit_restores_nothing_and_does_not_panic() {
        // Every stacked delta holds absolute offsets into a draft the submit
        // threw away, so an undo that survived one would `replace_range` past
        // the end of an empty `String`. The boundary is at `take_draft`, which
        // is the funnel every whole-draft disposal goes through.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"hello");
        assert_eq!(shell.edit_depths(), (5, 0));
        shell.route_bytes(&[0x0d]);
        let _taken = shell.sent.try_recv();

        assert_eq!(shell.edit_depths(), (0, 0), "the submit was not a boundary");
        shell.route_bytes(&[0x1f]);
        assert_eq!(shell.editor.text(), "", "a discarded draft is not undoable");
        assert_eq!(shell.edit_depths(), (0, 0));
    }

    #[test]
    fn clear_composer_is_the_same_boundary_as_a_submit() {
        // The same rule through the other door: an idle Ctrl-C throws the draft
        // away, and it inherits the boundary from `take_draft` rather than
        // taking one of its own.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"hello");
        shell.route_bytes(&[0x03]);
        assert_eq!(shell.editor.text(), "");
        assert_eq!(shell.edit_depths(), (0, 0));
        shell.route_bytes(&[0x1f]);
        assert_eq!(shell.editor.text(), "");
        assert_eq!(shell.edit_depths(), (0, 0));
    }

    #[test]
    fn a_slash_completion_is_a_boundary_and_never_a_half_undo() {
        // The whole draft was the query (`picker::Trigger::of`), so the insert
        // is the second half of a **replacement**. Recording only the insert
        // would give an undo that deletes the command name and leaves the
        // composer empty -- silently discarding what the user typed.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"/hel");
        shell.complete("help");
        assert_eq!(shell.editor.text(), picker::completed("help"));
        assert_eq!(
            shell.edit_depths(),
            (0, 0),
            "the completion recorded a delta"
        );

        shell.route_bytes(&[0x1f]);
        assert_eq!(
            shell.editor.text(),
            picker::completed("help"),
            "undo deleted the command name and lost the query"
        );
        assert_eq!(shell.edit_depths(), (0, 0));
    }

    #[test]
    fn a_recall_is_a_boundary_and_the_walk_survives_three_of_them() {
        // Two rules in one drive, because they are the same call site
        // (`Shell::recall`). A completed recall replaced the whole draft, so
        // every stacked delta names a text that is gone
        // (`input_completion_runtime.zig:339-356`) -- **and** the recall goes
        // through `edited`, not `amended`, so the walk it is a step of stays
        // open. A route through `amended` would call `history.leave()` and the
        // second `C-p` would hand back the newest line again forever.
        //
        // The third line is a **command**, which is a submitted line like any
        // other but is answered on this thread rather than by the runtime: the
        // runtime holds two pieces of work at a time (`super::worker`'s
        // `WORK_LIMIT`) and a third prompt would be refused, which would leave
        // its draft in the composer and make this a case about `Rejected::Busy`.
        let mut shell = shell(24, 80);
        submitted(&mut shell, "first line");
        submitted(&mut shell, "second line");
        submitted(&mut shell, "/help");
        shell.route_bytes(b"half typed");
        assert_eq!(shell.edit_depths(), (10, 0));

        shell.route_bytes(&[0x10]);
        assert_eq!(shell.editor.text(), "/help");
        assert_eq!(
            shell.edit_depths(),
            (0, 0),
            "the recall left deltas naming a draft that is gone"
        );
        shell.route_bytes(&[0x10]);
        assert_eq!(
            shell.editor.text(),
            "second line",
            "the second step back handed the newest line over again: the \
             recall ended its own walk"
        );
        shell.route_bytes(&[0x10]);
        assert_eq!(
            shell.editor.text(),
            "first line",
            "the third step back did not reach the oldest line"
        );

        // And an undo does not step back across any of it.
        shell.route_bytes(&[0x1f]);
        assert_eq!(shell.editor.text(), "first line");
        assert_eq!(shell.edit_depths(), (0, 0));

        // The walk still comes home to the draft it stood aside, which is the
        // other half of "the navigation is still live".
        shell.route_bytes(&[0x0e]);
        shell.route_bytes(&[0x0e]);
        shell.route_bytes(&[0x0e]);
        assert_eq!(shell.editor.text(), "half typed");
    }

    #[test]
    fn a_yank_is_itself_undoable_and_leaves_the_kill_slot_loaded() {
        // `C-y` is an edit like any other: it goes on the stack, so the undo
        // after it takes the **yank** back rather than the kill. And the delta
        // it records is `Ordinary`, so recording it does not overwrite the slot
        // it has just read -- the same text can be yanked again.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"one two");
        shell.route_bytes(&[0x17]); // C-w: kill "two"
        assert_eq!(shell.editor.text(), "one ");

        shell.route_bytes(&[0x19]); // C-y
        assert_eq!(shell.editor.text(), "one two");
        shell.route_bytes(&[0x1f]); // C-_
        assert_eq!(
            shell.editor.text(),
            "one ",
            "the undo took back the kill instead of the yank"
        );
        shell.route_bytes(&[0x19]);
        assert_eq!(
            shell.editor.text(),
            "one two",
            "an ordinary yank delta cleared the slot it read"
        );
    }

    #[test]
    fn backspace_and_delete_leave_the_kill_slot_alone() {
        // The other half of the kill/ordinary split, driven: only `C-k`, `C-u`
        // and `C-w` load the slot, so a backspace after a kill does not make
        // `C-y` yank one character.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"one two");
        shell.route_bytes(&[0x17]); // C-w: the slot holds "two"
        shell.route_bytes(b"xy");
        shell.route_bytes(&[0x7f]); // Backspace
        shell.route_bytes(&[0x01]); // C-a
        shell.route_bytes(&[0x1b, 0x5b, 0x33, 0x7e]); // Delete
        assert_eq!(shell.editor.text(), "ne x");
        shell.route_bytes(&[0x05]); // C-e
        shell.route_bytes(&[0x19]);
        assert_eq!(
            shell.editor.text(),
            "ne xtwo",
            "a backspace or a forward delete reloaded the kill ring"
        );
    }

    #[test]
    fn a_kill_and_its_undo_are_two_different_things_from_a_yank() {
        // An undo of the kill puts the text back where it was; a yank puts it
        // where the caret is. The two are driven against each other so that a
        // yank implemented as "undo the last kill" would fail.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"one two");
        shell.route_bytes(&[0x17]); // C-w
        shell.route_bytes(&[0x01]); // C-a: the caret is at the front now
        shell.route_bytes(&[0x19]); // C-y
        assert_eq!(
            shell.editor.text(),
            "twoone ",
            "the yank put the text back where it was killed from"
        );
        shell.route_bytes(&[0x1f]); // undo the yank
        assert_eq!(shell.editor.text(), "one ");
        shell.route_bytes(&[0x1f]); // undo the kill
        assert_eq!(
            shell.editor.text(),
            "one two",
            "the kill's own undo did not restore it in place"
        );
    }

    #[test]
    fn a_no_op_keystroke_does_not_clear_redo() {
        // Upstream's `prepare` has no empty-delta arm and its caller does
        // nothing on `.unchanged` (`edit_history.zig:85-95`). Here the mutator
        // answers `None` at `start >= end` (`Editor::delete`), so nothing is
        // recorded and the redo stack the user is standing on survives.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"a");
        shell.route_bytes(&[0x1f]);
        assert_eq!(shell.edit_depths(), (0, 1));

        shell.route_bytes(&[0x1b, 0x5b, 0x33, 0x7e]); // Delete, caret at the end
        assert_eq!(
            shell.edit_depths(),
            (0, 1),
            "a keystroke with nothing to do cleared the redo stack"
        );
        shell.act(Action::Redo, Instant::now());
        assert_eq!(shell.editor.text(), "a");
    }

    #[test]
    fn an_undo_never_records_itself_and_a_redo_puts_the_caret_back() {
        let mut shell = shell(24, 80);
        shell.route_bytes(b"abc");
        shell.route_bytes(&[0x01]); // C-a
        shell.route_bytes(b"Z");
        assert_eq!(shell.editor.text(), "Zabc");
        // Column 3: two cells of prompt marker, then the one byte in front of
        // the caret.
        assert_eq!(shell.cursor(), (22, 3));

        shell.route_bytes(&[0x1f]);
        assert_eq!(shell.editor.text(), "abc");
        assert_eq!(shell.cursor(), (22, 2), "the caret did not go back with it");
        assert_eq!(
            shell.edit_depths(),
            (3, 1),
            "the undo recorded itself as an edit"
        );

        shell.act(Action::Redo, Instant::now());
        assert_eq!(shell.editor.text(), "Zabc");
        assert_eq!(shell.cursor(), (22, 3));
        assert_eq!(shell.edit_depths(), (4, 0));
    }

    #[test]
    fn a_pastes_payload_is_counted_and_undone_whole() {
        // The delta carries the `Arc` behind the summary, so the history's byte
        // budget is measured against what a paste really costs -- and the undo
        // puts the block back under the number its summary says.
        let mut shell = shell(24, 80);
        let block = collapsed(&mut shell, 1);
        assert!(
            shell.edit_history.retained() > block.len(),
            "the paste's payload was not counted: {} bytes for a {}-byte block",
            shell.edit_history.retained(),
            block.len()
        );

        shell.route_bytes(&[0x1f]);
        assert_eq!(shell.editor.text(), "");
        assert!(shell.editor.entities().is_empty());
        shell.act(Action::Redo, Instant::now());
        assert_eq!(shell.editor.text(), "[Pasted text #1, 1 lines]");
        assert_eq!(
            shell.editor.entities().spans()[0].id(),
            1,
            "the redone block came back under a different number"
        );
        assert_eq!(
            shell.editor.expanded(),
            block,
            "the redone block does not stand for the text it did"
        );
    }

    #[test]
    fn a_refused_yank_spends_no_paste_number() {
        // **The allocation is transactional or it is a leak.** Renumbering a
        // yank's blocks mints ids out of the session's one counter
        // (`entity.rs`'s `renumber_recalled`, `*next = id`), and the counter is
        // never rewound: a number this session has spent is spent for good. So
        // a yank that is then refused by the budget has burned a number for an
        // edit that did not happen -- press `C-y` at a full composer often
        // enough and the session runs out of names for pastes it never made.
        let mut shell = shell(24, 80);
        let block = collapsed(&mut shell, 1);
        // C-u takes the line and the block on it, so the slot holds a summary
        // *and* the payload behind it -- which is what makes a yank a
        // renumbering rather than an insertion of plain text.
        shell.route_bytes(&[0x15]);
        assert_eq!(shell.editor.text(), "");
        let spent = *shell.paste.ids();
        assert_eq!(spent, 1, "the paste this case killed was #1");

        // A draft with no room for what the slot holds: the summary's own
        // bytes plus the payload behind it, against the one budget the two
        // share (`super::paste::fits`). Put in whole rather than a keystroke at
        // a time, like every other case here that needs a full composer.
        let yanked = "[Pasted text #1, 1 lines]".len() + block.len();
        assert!(
            shell
                .editor
                .insert(&"x".repeat(MAX_PASTE_BYTES - yanked + 1))
                .is_some(),
            "the draft could not be set up"
        );
        let draft = shell.editor.text().len();
        let caret = shell.editor.before_caret().len();
        let depths = shell.edit_depths();

        for attempt in 1..=3 {
            shell.route_bytes(&[0x19]);
            assert_eq!(
                shell.editor.text().len(),
                draft,
                "refusal {attempt} changed the draft"
            );
            assert_eq!(
                shell.editor.before_caret().len(),
                caret,
                "refusal {attempt} moved the caret"
            );
            assert_eq!(
                shell.edit_depths(),
                depths,
                "refusal {attempt} recorded a history entry"
            );
            let (text, spans) = shell
                .edit_history
                .killed()
                .expect("the slot is still loaded");
            assert_eq!(text, "[Pasted text #1, 1 lines]");
            assert_eq!(spans.len(), 1);
            assert_eq!(spans[0].id(), 1, "refusal {attempt} renumbered the slot");
            assert_eq!(
                spans[0].text().len(),
                block.len(),
                "refusal {attempt} damaged the killed payload"
            );
            assert_eq!(
                *shell.paste.ids(),
                spent,
                "refusal {attempt} spent a paste number on an edit that did not happen"
            );
        }

        // And with room again the yank takes the **next unspent** number -- #2,
        // not #5 -- carries the payload, and is undoable like any other edit.
        shell.route_bytes(&[0x03]); // an idle Ctrl-C throws the draft away
        assert_eq!(shell.editor.text(), "");
        shell.route_bytes(&[0x19]);
        assert_eq!(
            shell.editor.text(),
            "[Pasted text #2, 1 lines]",
            "the yank did not take the next unspent number"
        );
        assert_eq!(*shell.paste.ids(), 2);
        assert_eq!(
            shell.editor.expanded(),
            block,
            "the yanked summary stands for nothing"
        );
        assert_eq!(shell.edit_depths(), (1, 0));
        shell.route_bytes(&[0x1f]);
        assert_eq!(shell.editor.text(), "", "the yank was not undoable");
        assert!(shell.editor.entities().is_empty());
        shell.act(Action::Redo, Instant::now());
        assert_eq!(shell.editor.text(), "[Pasted text #2, 1 lines]");
        assert_eq!(shell.editor.expanded(), block);
    }

    #[test]
    fn a_killed_block_comes_back_under_its_own_number_and_a_yank_mints_a_new_one() {
        // Two live blocks may not answer to one number (`entity.rs:212-216`),
        // so a yank cannot re-register the id it read: the summary is rewritten
        // to say the new one and the payload is shared. An undo of the kill is
        // the other case -- it reverts to a draft in which the old id was live,
        // so that one comes back exactly as it was.
        let mut shell = shell(24, 80);
        let block = collapsed(&mut shell, 1);
        shell.route_bytes(&[0x15]); // C-u: kill the line, block and all
        assert_eq!(shell.editor.text(), "");

        shell.route_bytes(&[0x19]); // C-y
        assert_eq!(
            shell.editor.text(),
            "[Pasted text #2, 1 lines]",
            "the yanked block kept the number the killed one had"
        );
        assert_eq!(
            shell.editor.expanded(),
            block,
            "the payload did not come with it"
        );

        shell.route_bytes(&[0x1f]); // undo the yank
        assert_eq!(shell.editor.text(), "");
        shell.route_bytes(&[0x1f]); // undo the kill
        assert_eq!(
            shell.editor.text(),
            "[Pasted text #1, 1 lines]",
            "the undone kill did not restore the block's own number"
        );
        assert_eq!(shell.editor.expanded(), block);
    }

    #[test]
    fn a_paste_with_no_number_left_says_so_and_leaves_the_draft_alone() {
        // The end of the id space. Wrapping or saturating would put two live
        // blocks under one name -- and then a recall, which finds a block by
        // its number, would expand the wrong paste. Refused instead, and said,
        // because a paste that vanished without a word looks like a terminal
        // that never sent it.
        let mut shell = shell(24, 80);
        shell.paste = Paste::with_next(u32::MAX);
        shell.route_bytes(b"a draft worth keeping");
        shell.route_bytes(b"\x1b[200~");
        shell.route_bytes("y".repeat(1200).as_bytes());
        shell.route_bytes(b"\x1b[201~");

        assert_eq!(
            shell.editor.text(),
            "a draft worth keeping",
            "a paste with no number left changed the draft"
        );
        assert_eq!(shell.notice, Some(PASTE_UNNUMBERED));
        assert!(
            shell.hint().contains(PASTE_UNNUMBERED),
            "the refusal is not on the hint row: {:?}",
            shell.hint()
        );
    }

    #[test]
    fn a_pasted_line_is_not_folded_into_the_typed_words_that_look_like_it() {
        // The dedupe is adjacent-only and by text, which is right until a
        // summary is involved: the words are on the screen where a user can
        // type them, and the number the session mints next can make the two
        // lines identical. Folding them would drop the entry that stands on the
        // block and hand back the one that stands on nothing.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"[Pasted text #1, 1 lines]");
        shell.route_bytes(&[0x0d]);
        assert_eq!(
            shell.picks_up(),
            TurnWork::Submit("[Pasted text #1, 1 lines]".to_string())
        );
        let _echo = shell.document();

        let block = collapsed(&mut shell, 1);
        assert_eq!(
            shell.editor.text(),
            "[Pasted text #1, 1 lines]",
            "the paste did not take the number the typed line names, so this \
             case proves nothing"
        );
        shell.route_bytes(&[0x0d]);
        assert_eq!(shell.picks_up(), TurnWork::Submit(block.clone()));

        shell.route_bytes(&[0x10]); // C-p: one step back
        assert_eq!(
            shell.editor.text(),
            "[Pasted text #2, 1 lines]",
            "the pasted line was folded into the typed one: {:?}",
            shell.editor.text()
        );
        assert_eq!(
            shell.editor.expanded(),
            block,
            "the recalled line lost the block it stood on"
        );
    }

    #[test]
    fn a_recall_with_no_numbers_left_hands_back_words_and_says_so() {
        // The same exhaustion from the other side. A recalled block cannot keep
        // the number it was submitted under -- that number belongs to a draft
        // that is gone, and minting it again is the collision above -- so a
        // session with none left hands the line back as the words on the
        // screen, and says that is what it did.
        let mut shell = shell(24, 80);
        let block = collapsed(&mut shell, 1);
        shell.route_bytes(&[0x0d]);
        assert_eq!(shell.picks_up(), TurnWork::Submit(block));

        *shell.paste.ids() = u32::MAX;
        shell.route_bytes(&[0x10]); // C-p
        assert_eq!(
            shell.editor.text(),
            "[Pasted text #1, 1 lines]",
            "the recalled line is not the one that was submitted"
        );
        assert!(
            shell.editor.entities().is_empty(),
            "a block was kept under a number nobody minted"
        );
        assert_eq!(shell.notice, Some(RECALL_UNNUMBERED));
        assert_eq!(
            shell.editor.expanded(),
            "[Pasted text #1, 1 lines]",
            "the summary still stood for eight megabytes"
        );
    }

    #[test]
    fn an_empty_paste_is_not_an_edit() {
        // A frame is a repaint of the whole band on a link that may be a serial
        // line, and the band re-solves its height with it: a paste that put
        // nothing in the composer must cost neither.
        let mut shell = shell(24, 80);
        let _first = shell.render.begin().expect("the first frame");
        shell.route_bytes(b"\x1b[200~\x1b[201~");

        assert!(
            shell.render.begin().is_none(),
            "a paste that changed nothing asked for a whole-band repaint"
        );
    }

    #[test]
    fn a_paste_a_question_interrupted_does_not_leak_into_the_next_one() {
        // A question can arrive between two reads, and the panel swallows every
        // key it does not bind -- the tail of a paste already arriving
        // included, and its end marker with it. That leaves a paste with no
        // end, and the next `PasteStart` is the one moment at which what it
        // left is certainly stale.
        let mut shell = shell(24, 80);
        let _started = turn_running(&mut shell, b"edit the notes\r");
        shell.route_bytes(b"\x1b[200~abandoned");
        shell.apply(UiEvent::Approval(question(asked())));
        shell.route_bytes(b" tail\x1b[201~");
        assert!(
            shell.panel.is_some(),
            "the question was answered by the paste, so this case proves nothing"
        );
        shell.route_bytes(b"3");
        assert!(shell.panel.is_none(), "the question is still up");
        let _ = shell.document();

        shell.route_bytes(b"\x1b[200~fresh\x1b[201~");
        // Through the caret's own row rather than a fixed index: the band still
        // has the turn's row above the divider here.
        assert_eq!(
            shell.marked(),
            composer_row("fresh"),
            "a paste the question interrupted leaked into the next one"
        );
    }

    #[test]
    fn a_paste_past_the_budget_says_so_and_leaves_the_draft_alone() {
        // Refused whole rather than half-taken, and said rather than silent:
        // the composer's own budget refuses a keystroke silently because a
        // keystroke that changes nothing is its own feedback, and a paste that
        // vanished without a word looks like a terminal that never sent it.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"a draft worth keeping");
        shell.route_bytes(b"\x1b[200~");
        let chunk = vec![b'x'; 64 * 1024];
        for _ in 0..=(super::super::paste::MAX_PASTE_BYTES / chunk.len()) {
            shell.route_bytes(&chunk);
        }
        shell.route_bytes(b"\x1b[201~");

        assert_eq!(
            shell.editor.text(),
            "a draft worth keeping",
            "a paste that did not fit took the draft with it"
        );
        let hint = shell.hint();
        assert!(
            hint.contains(PASTE_REFUSED),
            "a paste larger than the budget vanished without a word: {hint:?}"
        );
    }

    // -----------------------------------------------------------------------
    // the band's own height
    // -----------------------------------------------------------------------

    #[test]
    fn the_band_grows_with_the_composer_and_shrinks_back_with_it() {
        let mut shell = shell(24, 80);
        assert_eq!(shell.geometry.input_first, 22);
        shell.route_bytes(b"one\x0atwo\x0athree");
        assert_eq!(shell.geometry.input_rows(), 3);
        assert_eq!(
            (shell.geometry.input_first, shell.geometry.content_bottom),
            (20, 19),
            "the composer took rows from the document without moving its \
             first row up, so the band and the document overlap"
        );
        assert_eq!(shell.cursor(), (22, 7), "the caret is on the last row");

        // Back to one row, and every row number back with it. A kill takes the
        // line it is on, so what empties a three-line draft is submitting it.
        shell.route_bytes(&[0x15]);
        assert_eq!(
            shell.geometry.input_rows(),
            3,
            "the kill took a whole draft"
        );
        shell.route_bytes(&[0x0d]);
        assert_eq!(shell.band_rows().len(), 3, "composer, rule, hint");
        assert_eq!(
            (shell.geometry.input_first, shell.geometry.content_bottom),
            (22, 21),
            "the band kept rows the document had back"
        );
    }

    #[test]
    fn the_composer_stops_growing_at_half_the_content_area_and_scrolls_instead() {
        // input_presentation.zig:201-220. On a 12-row screen the cap is five
        // rows, and the sixth line of a draft scrolls the first one out of the
        // window rather than taking a sixth row from the transcript.
        let mut shell = shell(12, 80);
        assert_eq!(crate::tui::layout::input_row_limit(12), 5);
        shell.route_bytes(b"one\x0atwo\x0athree\x0afour\x0afive");
        assert_eq!(shell.geometry.input_rows(), 5);
        assert_eq!(shell.geometry.input_first, 6);
        assert_eq!(
            shell.band_rows()[0..5],
            [
                composer_row("one"),
                composer_row("two"),
                composer_row("three"),
                composer_row("four"),
                composer_row("five"),
            ]
        );

        shell.route_bytes(b"\x0asix");
        assert_eq!(shell.geometry.input_rows(), 5, "the cap did not hold");
        assert_eq!(
            shell.band_rows()[0..5],
            [
                composer_row_below_hidden("two"),
                composer_row("three"),
                composer_row("four"),
                composer_row("five"),
                composer_row("six"),
            ],
            "the window did not follow the caret"
        );
        assert_eq!(
            shell.cursor(),
            (10, 5),
            "the caret is on the band's last composer row"
        );
    }

    #[test]
    fn a_draft_with_more_rows_than_a_u16_still_shows_its_end_and_places_the_caret() {
        // The composer's rows are counted in `usize` and narrowed only where a
        // terminal row is made. A count saturated at `u16::MAX` would leave the
        // window eleven rows short of row 65535 -- nowhere near the caret --
        // and the band would show rows the user is not typing on.
        //
        // The draft is inserted rather than typed because every keystroke
        // re-measures the whole composer (`refit`), so seventy thousand of them
        // is quadratic work for a fact about one of them. What is under test is
        // what the band makes of the text, and that is reached the same way.
        let mut shell = shell(24, 80);
        assert!(shell.editor.insert(&"x\n".repeat(70_000)).is_some());
        shell.edited(None);

        assert_eq!(shell.geometry.input_rows(), 11, "the cap");
        let rows = shell.band_rows();
        assert_eq!(
            rows[0],
            composer_row_below_hidden("x"),
            "the window is not showing the draft's end"
        );
        assert_eq!(
            rows[10],
            composer_row(""),
            "the last composer row is the empty one the caret is on: {rows:?}"
        );
        assert_eq!(
            shell.cursor(),
            (22, 2),
            "the caret is not on the band's last composer row"
        );
    }

    #[test]
    fn the_smallest_band_still_grows_by_the_rule_rather_than_by_luck() {
        // The cap is measured against the screen, so the row it names is one
        // `layout::solve` will really give: a screen that holds a band holds a
        // capped composer on it, and the composer never has to be clamped by
        // some second rule that could disagree with this one.
        for rows in crate::tui::layout::MIN_ROWS..=40 {
            let mut shell = shell(rows, 80);
            let limit = crate::tui::layout::input_row_limit(rows);
            shell.route_bytes("x\n".repeat(usize::from(limit) + 4).as_bytes());
            assert_eq!(
                shell.geometry.input_rows(),
                limit,
                "a {rows}-row screen grew its composer to something other than \
                 its cap"
            );
            assert!(
                shell.geometry.content_bottom >= 1,
                "a {rows}-row screen left no document above the band"
            );
            assert_eq!(
                shell.band_rows().len(),
                usize::from(shell.geometry.band_rows())
            );
        }
    }

    #[test]
    fn a_composer_wider_than_the_screen_wraps_into_the_gutter() {
        // The text is measured against the screen minus the marker, so a row is
        // never two cells too long for the terminal -- which the painter would
        // clip and the caret would not.
        let mut shell = shell(24, 20);
        shell.route_bytes(&[b'x'; 19]);
        assert_eq!(
            &shell.band_rows()[0..2],
            &[composer_row(&"x".repeat(18)), composer_row("x")]
        );
        assert_eq!(shell.cursor(), (22, 3));
        for row in shell.band_rows() {
            assert!(
                crate::tui::wrap::width(&row) <= 20,
                "a band row ran past the screen: {row:?}"
            );
        }
    }

    // -----------------------------------------------------------------------
    // the turn
    // -----------------------------------------------------------------------

    #[test]
    fn a_submitted_line_is_handed_to_the_runtime_and_echoed_into_the_document() {
        let mut shell = shell(24, 80);
        shell.route_bytes(b"say the marker");
        shell.route_bytes(&[0x0d]);

        assert_eq!(
            shell.sent.try_recv(),
            Ok(TurnWork::Submit("say the marker".to_string())),
            "the composer's text never reached the runtime"
        );
        assert_eq!(shell.document(), card("say the marker"));
        assert!(shell.editor.is_empty(), "the composer kept a sent draft");
    }

    #[test]
    fn one_prompt_may_wait_and_the_band_says_that_it_is_waiting() {
        // One turn runs at a time and one more prompt may wait: the second
        // submission is taken, and the whole difference between a queue and a
        // surprise is that the band says so for as long as it is there.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"first\r");
        assert_eq!(shell.picks_up(), TurnWork::Submit("first".to_string()));
        assert_eq!(shell.hint(), IDLE_HINT, "an empty queue was announced");

        shell.route_bytes(b"second\r");

        assert_eq!(shell.hint(), queued_hint(1));
        assert!(
            shell.editor.is_empty(),
            "a submission that was taken kept the draft"
        );
    }

    /// A shell with a turn the runtime has said is running, and the moment it
    /// began being measured.
    ///
    /// The two halves of a turn's row, in the order a session produces them:
    /// the prompt is submitted, the runtime picks it up and says so, and the
    /// next settle is what puts a clock on it. Written once because every case
    /// below needs it and because getting the order wrong is how a test starts
    /// asserting against a row that is not there yet.
    fn turn_running(shell: &mut Fixture, bytes: &[u8]) -> Instant {
        shell.route_bytes(bytes);
        shell.apply(UiEvent::TurnStarted);
        let started = Instant::now();
        shell.settle_band(started);
        let _ = shell.document();
        started
    }

    #[test]
    fn a_running_turn_says_what_it_is_doing_on_the_row_above_the_composer() {
        // The band gains two rows while a turn runs -- the activity row and a
        // blank row under it (`frame_layout.zig:62-75`) -- and they come off
        // the bottom of the document rather than moving the composer: the
        // caret must not jump because a turn started. The gap above the row is
        // the document's own blank row, the one the prompt's card ended on.
        let mut shell = shell(24, 80);
        let idle = shell.geometry;
        assert_eq!(idle.activity, None);
        assert_eq!(shell.band_rows()[0], composer_row(""));

        // `turn_running`, with the document it hands over kept.
        shell.route_bytes(b"ask something\r");
        shell.apply(UiEvent::TurnStarted);
        let started = Instant::now();
        shell.settle_band(started);
        let document = shell.document();
        assert_eq!(
            document.last().map(String::as_str),
            Some(""),
            "no blank row between the document and the activity row: {document:?}"
        );
        shell.settle_band(started + Duration::from_secs(2));

        let geometry = shell.geometry;
        assert_eq!(
            geometry.activity,
            Some(geometry.input_first - 2),
            "the row is not the one a blank row above the composer"
        );
        assert_eq!(
            Some(geometry.band_top()),
            geometry.activity,
            "the band put a blank row of its own above the activity row"
        );
        assert_eq!(geometry.divider, idle.divider, "the rule moved");
        assert_eq!(geometry.input_first, idle.input_first, "the caret moved");
        assert_eq!(geometry.content_bottom, idle.content_bottom - 2);

        let rows = shell.band_rows();
        assert_eq!(
            rows.len(),
            usize::from(geometry.band_rows()),
            "the band's rows and its geometry disagree: {rows:?}"
        );
        assert!(rows[0].contains("Thinking"), "{rows:?}");
        // The clock is the turn's own, from the settle that first measured it,
        // so this is two seconds exactly however long the rest of the test
        // takes.
        assert!(rows[0].contains("(2s)"), "{rows:?}");
        // Literal, not derived from `Palette::activity` itself: a check that
        // asked the accessor what it expects would pass for whatever
        // `theme.rs` happened to declare. `252` is upstream's own dark index
        // for this row (`shimmer_runtime.zig:262-291`'s
        // `permission_auto_style`, `render.zig:55,86,105`). Asked of the label
        // rather than of the row's first cell, which is a bare blank while the
        // marker is dark.
        assert!(
            rows[0].contains("\u{1b}[38;5;252m Thinking (2s)"),
            "the activity row is not painted in the dark palette's literal \
             foreground: {rows:?}"
        );
        assert!(
            rows[0].ends_with("\u{1b}[0m"),
            "the activity row's colour was left open: {rows:?}"
        );
        assert_eq!(
            rows[1], "",
            "no blank row between the activity row and the composer"
        );
        assert_eq!(
            rows[2],
            composer_row(""),
            "the activity row's colour leaked onto the composer below it: {rows:?}"
        );
        assert_eq!(rows.last().expect("a hint row"), &hint_row(IDLE_HINT));
    }

    #[test]
    fn the_activity_rows_literal_foreground_is_the_light_palettes_too() {
        // The dark index above is only half of what upstream pins: the light
        // row is `238` (`render.zig:55,86,105`), and a role that only matched
        // one of the two palettes would leave a light terminal reading
        // `permission_auto_style`'s dark grey.
        let mut shell = shell_with(24, 80, LIGHT, false);
        let started = turn_running(&mut shell, b"ask something\r");
        shell.settle_band(started + Duration::from_secs(2));

        let rows = shell.band_rows();
        assert!(
            rows[0].contains("Thinking") && rows[0].contains("(2s)"),
            "{rows:?}"
        );
        assert!(
            rows[0].contains("\u{1b}[38;5;238m Thinking (2s)"),
            "the activity row is not painted in the light palette's literal \
             foreground: {rows:?}"
        );
        assert!(
            rows[0].ends_with("\u{1b}[0m"),
            "the activity row's colour was left open: {rows:?}"
        );
    }

    #[test]
    fn a_submitted_prompt_that_is_still_waiting_says_nothing_about_a_turn() {
        // The row is about the turn the runtime is **running**. A prompt may
        // wait behind another for a minute, and the band already says so on its
        // hint row -- announcing `Thinking` for it would be the band claiming
        // work that has not started.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"ask something\r");
        shell.settle_band(Instant::now());

        assert_eq!(shell.geometry.activity, None);
        assert!(
            !shell.band_rows().iter().any(|row| row.contains("Thinking")),
            "{:?}",
            shell.band_rows()
        );
    }

    #[test]
    fn a_prompt_queued_behind_a_turn_does_not_restart_the_turns_clock() {
        // What the row measures is the turn that is running, and a second
        // prompt joining the queue is not an event in that turn's life.
        let mut shell = shell(24, 80);
        let started = turn_running(&mut shell, b"first\r");
        shell.settle_band(started + Duration::from_secs(2));
        assert!(
            shell.band_rows()[0].contains("(2s)"),
            "{:?}",
            shell.band_rows()
        );

        shell.route_bytes(b"second\r");
        shell.settle_band(started + Duration::from_secs(3));

        let rows = shell.band_rows();
        assert!(
            rows[0].contains("(3s)"),
            "the queued prompt restarted the running turn's clock: {rows:?}"
        );
    }

    #[test]
    fn the_turn_that_was_queued_gets_its_own_clock_rather_than_the_last_ones() {
        // The handoff. When the turn in flight ends and the runtime starts the
        // prompt that was queued behind it, the row is about **that** turn from
        // the moment it starts -- a row that carried the finished turn's
        // elapsed time forward would report a number that was never about the
        // turn the user is waiting for, and it would keep growing across every
        // queued prompt for the rest of the session.
        let mut shell = shell(24, 80);
        let started = turn_running(&mut shell, b"first\r");
        shell.route_bytes(b"second\r");
        shell.settle_band(started + Duration::from_secs(2));
        assert!(
            shell.band_rows()[0].contains("(2s)"),
            "{:?}",
            shell.band_rows()
        );

        shell.apply(UiEvent::TurnEnded { failure: None });
        shell.apply(UiEvent::TurnStarted);
        let _ = shell.document();
        shell.settle_band(started + Duration::from_secs(3));

        let rows = shell.band_rows();
        assert!(
            rows[0].contains("(0s)"),
            "the queued turn inherited the finished turn's clock: {rows:?}"
        );
        shell.settle_band(started + Duration::from_secs(5));
        assert!(
            shell.band_rows()[0].contains("(2s)"),
            "{:?}",
            shell.band_rows()
        );
    }

    #[test]
    fn a_turn_that_ends_takes_the_row_with_it_whatever_the_queue_still_holds() {
        // The conclusion is the end of the row, and it does not matter what
        // else the runtime has in hand: the *next* row waits for the runtime to
        // say that the next turn started. This is the interleaving a count
        // cannot survive -- the place a concluded turn holds is given back
        // after its conclusion is sent (`super::worker`'s `turn_loop`), so a
        // session reading a queue depth here would find the finished turn's own
        // place still claimed and start a clock for a turn that does not exist.
        let mut shell = shell(24, 80);
        let started = turn_running(&mut shell, b"only one\r");
        assert!(shell.geometry.activity.is_some());
        assert_eq!(
            shell.work.outstanding(),
            1,
            "the fixture is not in the pre-decrement state this case is about"
        );

        shell.apply(UiEvent::TurnEnded { failure: None });
        let _ = shell.document();
        shell.settle_band(started + Duration::from_millis(50));

        assert_eq!(shell.geometry.activity, None);
        assert!(
            !shell.band_rows().iter().any(|row| row.contains("Thinking")),
            "{:?}",
            shell.band_rows()
        );

        // And no phantom row afterwards, however long the place stays claimed,
        // and no frame owed for one.
        let _taken = shell.render.begin();
        for tick in 1..=125u64 {
            shell.settle_band(started + Duration::from_millis(50 + tick * TICK_MILLIS));
        }
        assert_eq!(shell.geometry.activity, None, "a turn's row came back");
        assert!(
            shell.render.begin().is_none(),
            "a frame was owed for a row that does not exist"
        );
    }

    #[test]
    fn a_command_the_runtime_runs_after_a_turn_is_not_the_model_thinking() {
        // `/model` travels on the work channel like a prompt, so the runtime
        // has work in hand for one -- but nothing is being answered. A session
        // that started a clock because a turn ended with something still
        // queued would say `Thinking` about a command it had already answered
        // on its own thread. (`/new` used to travel the same way; a reset is a
        // control message now, and takes no place in the queue.)
        let mut shell = shell(24, 80);
        let started = turn_running(&mut shell, b"ask something\r");
        shell.route_bytes(b"/model other\r");
        let _ = shell.document();
        assert!(
            shell.work.outstanding() > 1,
            "the command did not reach the runtime, so this proves nothing"
        );

        shell.apply(UiEvent::TurnEnded { failure: None });
        let _ = shell.document();
        shell.settle_band(started + Duration::from_millis(50));

        let rows = shell.band_rows();
        assert_eq!(
            shell.geometry.activity, None,
            "a queued command put a turn's row on the band: {rows:?}"
        );
        assert!(!rows.iter().any(|row| row.contains("Thinking")), "{rows:?}");
    }

    #[test]
    fn a_running_tool_takes_the_row_over_and_hands_it_back() {
        // A turn that has gone quiet because a tool is taking a minute looks
        // exactly like a turn that has gone quiet, unless the band says which:
        // `Running` while the tool runs (`activity_status.zig:105-111`); which
        // tool it is, the tool group's `└ Running ...` row says.
        let mut shell = shell(24, 80);
        let started = turn_running(&mut shell, b"ask something\r");

        shell.apply(tool_start(
            "1",
            "read_file",
            serde_json::json!({"path": "a.txt"}),
        ));
        shell.settle_band(started + Duration::from_millis(50));
        let rows = shell.band_rows();
        assert!(rows[0].contains("Running"), "{rows:?}");
        assert!(!rows[0].contains("Thinking"), "{rows:?}");

        shell.apply(tool_result("1", "read_file", true, ""));
        shell.settle_band(started + Duration::from_millis(100));
        let rows = shell.band_rows();
        assert!(rows[0].contains("Thinking"), "{rows:?}");
        assert!(!rows[0].contains("Running"), "{rows:?}");
        // The tool was part of the turn, so the turn's clock ran through it.
        shell.settle_band(started + Duration::from_secs(4));
        assert!(
            shell.band_rows()[0].contains("(4s)"),
            "{:?}",
            shell.band_rows()
        );
    }

    #[test]
    fn typing_while_a_turn_runs_does_not_take_the_row_away() {
        // Every re-solve carries the row's presence with it. Without that, a
        // keystroke would drop the row and the next settle would put it back --
        // a band that flickered a row wider and narrower on every character.
        let mut shell = shell(24, 80);
        let started = turn_running(&mut shell, b"ask something\r");
        let working = shell.geometry;

        shell.route_bytes(b"the next thing");

        assert_eq!(shell.geometry.activity, working.activity);
        assert_eq!(shell.geometry.content_bottom, working.content_bottom);
        shell.settle_band(started + Duration::from_millis(50));
        assert!(shell.band_rows()[0].contains("Thinking"));
    }

    #[test]
    fn the_marker_really_blinks_while_the_turn_runs() {
        // The blink is `activity::lit` counting phases and the phase is moved
        // on here, from the render request's animation tick. Without the
        // second half the row is lit for ever: a marker that never changes is
        // one the user cannot tell from a frozen band.
        let mut shell = shell(24, 80);
        let started = turn_running(&mut shell, b"ask something\r");
        let mut markers = std::collections::BTreeSet::new();
        // One second of the loop's own ticks, which is two of the marker's
        // half-periods.
        for tick in 0..=1000 / TICK_MILLIS {
            shell.settle_band(started + Duration::from_millis(tick * TICK_MILLIS));
            markers.insert(
                shell
                    .band_rows()
                    .first()
                    // The lit marker is painted in the activity role's colour
                    // (`Activity::row`), so its cell is past that colour rather
                    // than at the row's first character; the dark one is a bare
                    // blank cell.
                    .and_then(|row| {
                        row.strip_prefix(PALETTE.activity())
                            .unwrap_or(row)
                            .chars()
                            .next()
                    })
                    .expect("the activity row's first cell"),
            );
        }
        assert_eq!(
            markers.len(),
            2,
            "the marker did not blink in a whole second: {markers:?}"
        );
    }

    #[test]
    fn an_idle_band_asks_for_no_frames_at_all_however_long_it_sits_there() {
        // The animated row is the only thing in this phase with a clock of its
        // own, and it is not running: a session at an idle prompt must not
        // repaint twenty times a second for a row nothing is drawing.
        let mut shell = shell(24, 80);
        let start = Instant::now();
        let _first = shell.render.begin().expect("the first frame");
        for tick in 1..=250u64 {
            shell.settle_band(start + Duration::from_millis(tick * TICK_MILLIS));
        }
        assert!(
            shell.render.begin().is_none(),
            "an idle band asked for a frame nothing had changed for"
        );
    }

    #[test]
    fn the_row_asks_for_a_frame_when_it_changes_and_not_on_every_tick() {
        // Twenty repaints a second of a row that says the same thing is a cost
        // paid on every link; a row that changed and asked for nothing would
        // sit there stale until the next keystroke.
        let mut shell = shell(24, 80);
        let started = turn_running(&mut shell, b"ask something\r");
        let _asked = shell.render.begin().expect("the frame the row asked for");

        // Half a tick later nothing about the row can have changed.
        shell.settle_band(started + Duration::from_millis(4));
        assert!(
            shell.render.begin().is_none(),
            "a tick with nothing to show asked for a frame"
        );

        // A second later the elapsed time reads differently, and that is a
        // frame.
        shell.settle_band(started + Duration::from_millis(1000));
        assert!(
            shell.render.begin().is_some(),
            "the row changed and no frame was asked for, so it would sit stale"
        );
    }

    // -----------------------------------------------------------------------
    // the question
    // -----------------------------------------------------------------------

    /// The question a scripted edit puts to the user, in the shape
    /// `crate::permission::PermissionSession::ask` builds one.
    fn asked() -> ApprovalRequest {
        ApprovalRequest {
            tool: "edit_file",
            target: "notes.txt".to_string(),
            summary: "edit `notes.txt`: replace \"alpha\" with \"beta\"".to_string(),
            always_scope:
                "allow every future edit_file to `notes.txt` for the rest of this session"
                    .to_string(),
            diff: None,
            subject: crate::permission::ApprovalSubject::Path("notes.txt".to_string()),
        }
    }

    /// How many rows [`asked`] really needs on an eighty-column screen.
    ///
    /// **A changed coordinate, and this is why.** The panel is upstream's file
    /// question now (`super::super::approval_view`): the review of the change
    /// -- one notice row, because this fixture carries no diff -- the dotted
    /// rule, the header, the path and its question, the three answers and one
    /// row of this fixture's short hand-written scope. Below
    /// `approval::SPACIOUS_AT` rows there are no blank rows between the parts.
    /// Named once rather than spelled at each of its uses, so that the next
    /// change to the wording moves one number.
    const PANEL_ROWS: u16 = 8;

    /// The envelope a question reaches the shell in.
    ///
    /// `ApprovalId(1)` because that is what a session's first question is really
    /// asked under (`super::super::approval::TuiPrompter`'s counter starts at
    /// one), and every case here is a first question: an id invented for these
    /// fixtures would let a shell that answered under the wrong one pass.
    fn question(request: ApprovalRequest) -> super::super::approval::ApprovalAsked {
        super::super::approval::ApprovalAsked {
            id: ApprovalId(1),
            request,
        }
    }

    /// The same question about a change too big for the band's review: forty
    /// lines replaced, eighty rows (`approval::BAND_REVIEW_LINES`).
    fn asked_about_a_large_change() -> ApprovalRequest {
        let mut request = asked();
        request.diff = Some(crate::permission::ApprovalDiff {
            before: "alpha\n".repeat(40),
            after: "beta\n".repeat(40),
            first_line: 1,
            lines_below: 0,
        });
        request
    }

    /// Document rows as one sentence, with the wrap taken back out.
    ///
    /// A notice is written as a sentence and the document wraps it to the
    /// screen, so a case that asserted on one row would be asserting on where
    /// the wrap fell rather than on what was said -- and would then fail on a
    /// narrower fixture for a reason that has nothing to do with it. Every word
    /// is still pinned, because both sides are normalised the same way.
    fn unwrapped(rows: &[String]) -> String {
        rows.join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// `row` with every SGR sequence taken out.
    fn colourless(row: &str) -> String {
        let mut out = String::with_capacity(row.len());
        let mut rest = row;
        while let Some(at) = rest.find('\u{1b}') {
            out.push_str(&rest[..at]);
            let tail = &rest[at..];
            let end = super::super::pacer::colour_at(tail).unwrap_or(1);
            rest = &tail[end..];
        }
        out.push_str(rest);
        out
    }

    /// How the interrupt's notice opens in the document: `⊘ turn: ` and the
    /// sentence after it.
    /// The turn's own `■ Cancelled` line (claim changed: this was the line
    /// shell's interrupt notice, `⊘ turn: interrupted …`).
    const INTERRUPTED_ROW: &str = "\u{25a0} Cancelled \u{b7} What can xfx do differently?";

    /// A tool call starting, as the runtime announces it: with the row its
    /// own spec gives it (`crate::tools::ToolSpec::shown`).
    fn tool_start(id: &str, tool: &str, input: serde_json::Value) -> UiEvent {
        let spec = crate::tools::Registry::builtin()
            .spec(tool)
            .expect("a tool the registry has");
        UiEvent::ToolStart {
            call_id: id.to_string(),
            tool: tool.to_string(),
            shown: spec.shown(&input),
        }
    }

    /// A tool call finishing with nothing more to say than `ok`.
    fn tool_result(id: &str, tool: &str, ok: bool, detail: &str) -> UiEvent {
        tool_ended(id, tool, ok, detail, CallOutcome::Finished)
    }

    /// A tool call finishing as `outcome`.
    fn tool_ended(id: &str, tool: &str, ok: bool, detail: &str, outcome: CallOutcome) -> UiEvent {
        UiEvent::ToolResult {
            call_id: id.to_string(),
            tool: tool.to_string(),
            ok,
            detail: detail.to_string(),
            outcome,
            changes: None,
        }
    }

    impl Fixture {
        /// The document as a terminal holds it once everything owed has
        /// landed, with the colours taken off.
        ///
        /// Not [`Self::document`], which lists every row of every append: an
        /// open tool group is **rewritten** where it stands (`Append`'s
        /// settled rows), so its rows appear in that list once per rewrite.
        /// This replays the appends the way the screen takes them -- the
        /// settled rows of each replace the last ones written -- which is the
        /// view the group's cases are about.
        ///
        /// **Every** colour is taken off, not [`unpainted`]'s closed list: a
        /// command's words are coloured by the shell highlighter inside the
        /// row, and what these cases pin is the rows' words. The colours have
        /// cases of their own (`super::super::toolgroup`).
        fn written(&mut self) -> Vec<String> {
            let mut document: Vec<String> = Vec::new();
            for append in self.take_pending() {
                let settled = append.rows.len() - append.scroll.min(append.rows.len());
                document.truncate(document.len().saturating_sub(settled));
                document.extend(append.rows.iter().map(|row| colourless(row)));
            }
            document
        }
    }

    /// The frame that really disclosed the standing question, written and then
    /// reconciled.
    ///
    /// What `super::super::event_loop` does across two ticks -- compose, hand
    /// the bytes to the terminal, and check after the next drain of the resize
    /// signal -- said in the three calls that carry it. Spelled out in the
    /// fixture rather than granted by a switch on `Shell`: a `ready` that
    /// answered `true` under `cfg(test)` would make every case below a case
    /// about nothing.
    fn delivered(shell: &mut Fixture) {
        shell.intend_approval();
        shell.approval_landed(Outcome::Painted);
        shell.reconcile_approval();
    }

    /// A shell with a turn running and a question in front of the user, on a
    /// screen that has really shown it.
    fn asking(shell: &mut Fixture) -> Instant {
        let started = turn_running(shell, b"edit the notes\r");
        shell.apply(UiEvent::Approval(question(asked())));
        shell.settle_band(started);
        let _ = shell.document();
        delivered(shell);
        started
    }

    /// A question **`crate::permission` really built**, recorded or not.
    ///
    /// The hand-written [`asked`] carries a short always-scope; every scope a
    /// session builds carries an unconditional suffix on top of it
    /// (`crate::permission::policy`'s `always_scope_for`), and the cases below
    /// are about whether an *ordinary* screen can disclose an *ordinary*
    /// request. A fixture shorter than the product would answer yes for a panel
    /// that cuts.
    fn as_the_session_builds_it(durable: bool) -> ApprovalRequest {
        use crate::permission::{
            MutationKind, MutationPlan, PermissionMode, PermissionSession, Preimage,
            ProposedAction, TargetScope,
        };

        struct Recording(std::sync::Arc<std::sync::Mutex<Vec<ApprovalRequest>>>);

        impl crate::permission::ApprovalPrompter for Recording {
            fn request(
                &mut self,
                request: &ApprovalRequest,
            ) -> std::io::Result<crate::permission::ApprovalAnswer> {
                self.0.lock().expect("lock").push(request.clone());
                Ok(ApprovalAnswer::Deny)
            }
        }

        let plan = MutationPlan::new(
            MutationKind::Edit,
            // The long absolute path a real temporary workspace has.
            std::path::PathBuf::from(
                "/private/var/folders/f4/mj8750512wdb85799rpkct880000gn/T/.tmpqwGs5r/workspace/notes.txt",
            ),
            "notes.txt".to_string(),
            TargetScope::PrimaryWorkspace,
            Preimage::Absent,
            b"beta".to_vec(),
        );
        let asked = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let session = PermissionSession::new(PermissionMode::Ask);
        let session = if durable {
            session.with_durable_session("01K5Z8QF3V6TQ7B2N4H9J0XWRC")
        } else {
            session
        };
        let mut session = session.with_prompter(Box::new(Recording(std::sync::Arc::clone(&asked))));
        session.decide_with_feedback(ProposedAction::Mutation(&plan));
        let mut asked = asked.lock().expect("lock");
        asked.remove(0)
    }

    #[test]
    fn an_ordinary_screen_discloses_an_ordinary_request_and_takes_both_affirmatives() {
        // **The regression the whole slice turns on.** A session's always-scope
        // is three wrapped rows at eighty columns in both recording modes, and
        // the panel used to allot it two: the sentence was cut, no frame could
        // disclose the request, and every real approval on an ordinary terminal
        // was ungrantable. Driven with the request the session really builds,
        // at the size a terminal really opens at, for both affirmatives --
        // `Always` because it is the one that buys the rest of the session.
        for durable in [true, false] {
            for (typed, answer) in [
                (Input::Text('1'), ApprovalAnswer::Once),
                (Input::Text('2'), ApprovalAnswer::Always),
            ] {
                let mut shell = shell(24, 80);
                let started = turn_running(&mut shell, b"edit the notes\r");
                shell.apply(UiEvent::Approval(question(as_the_session_builds_it(
                    durable,
                ))));
                shell.settle_band(started);
                let _ = shell.document();
                assert!(
                    shell.geometry.panel > 0,
                    "durable={durable}: the question was refused for want of rows"
                );
                // The whole sentence is on the band, tail included: what
                // "always" buys is half the question.
                let painted = uncoloured_rows(&shell.band_rows()).join(" ");
                let tail = if durable {
                    "of this saved session"
                } else {
                    "the approval ends with this command"
                };
                assert!(
                    unwrapped(std::slice::from_ref(&painted)).contains(tail),
                    "durable={durable}: the always-scope was cut: {painted:?}"
                );

                delivered(&mut shell);
                shell.decide(typed, Instant::now());
                assert_eq!(
                    shell.controlled(),
                    Some(TurnControl::Answer {
                        id: ApprovalId(1),
                        answer,
                        feedback: None,
                    }),
                    "durable={durable}: a disclosed, committed and reconciled frame was refused"
                );
            }
        }
    }

    #[test]
    fn a_review_plane_that_cannot_disclose_says_so_on_its_own_screen_and_recovers_when_widened() {
        // The alternate half of the gate, and of the notice. While this plane is
        // up it owns every row the user can see, so a refusal written into the
        // document would be a refusal behind the screen.
        let mut shell = shell(24, 30);
        let mut request = as_the_session_builds_it(true);
        request.diff = Some(crate::permission::ApprovalDiff {
            before: "alpha\n".repeat(40),
            after: "beta\n".repeat(40),
            first_line: 1,
            lines_below: 0,
        });
        shell.apply(UiEvent::Approval(question(request)));
        assert_eq!(shell.screen_owner(), ScreenOwner::Alternate);
        delivered(&mut shell);

        shell.decide(Input::Text('1'), Instant::now());
        assert_eq!(
            shell.controlled(),
            None,
            "a screen that cannot show the request granted it"
        );
        assert!(
            shell.released().is_empty(),
            "the refusal went to the document, which is behind this plane"
        );
        let notice = unwrapped(&uncoloured_rows(&shell.screen_rows()));
        // The **leading clause**, not the whole sentence: the status line is
        // bounded to `STATUS_ROWS` on purpose, and thirty cells is narrow
        // enough that two rows do not hold all of it. A notice allowed to grow
        // to fit would be a notice pushing an answer off the screen to explain
        // why the answer could not be taken. What has to reach the user is
        // which of the two refusals this is, and that is its first clause.
        assert!(
            notice.contains("this screen cannot show the whole request"),
            "the plane that owns the screen never said why: {notice:?}"
        );
        assert!(
            notice.contains("3 Don't apply"),
            "the refusal took an answer off the screen: {notice:?}"
        );

        // Refusing is still possible at this size, which is what makes the gate
        // a gate rather than a trap.
        let mut trapped = shell.controlled();
        assert!(trapped.is_none());
        shell.decide(Input::Text('3'), Instant::now());
        trapped = shell.controlled();
        assert_eq!(
            trapped,
            Some(TurnControl::Answer {
                id: ApprovalId(1),
                answer: ApprovalAnswer::Deny,
                feedback: None,
            }),
            "a question that cannot be granted must still be refusable"
        );

        // And a wider screen recovers: the same question, disclosed whole, is
        // answerable once a frame for that screen has landed.
        drop(shell);
        let mut wide = self::shell(40, 120);
        let mut request = as_the_session_builds_it(true);
        request.diff = Some(crate::permission::ApprovalDiff {
            before: "alpha\n".repeat(40),
            after: "beta\n".repeat(40),
            first_line: 1,
            lines_below: 0,
        });
        wide.apply(UiEvent::Approval(question(request)));
        assert_eq!(wide.screen_owner(), ScreenOwner::Alternate);
        delivered(&mut wide);
        wide.decide(Input::Text('1'), Instant::now());
        assert_eq!(
            wide.controlled(),
            Some(TurnControl::Answer {
                id: ApprovalId(1),
                answer: ApprovalAnswer::Once,
                feedback: None,
            }),
            "a screen with room for the whole request still refused it"
        );
    }

    #[test]
    fn an_affirmative_before_the_frame_is_refused_and_not_replayed() {
        // The whole point of the gate: a grant is given against a screen, and
        // until a frame has really put this question on one there is no screen
        // for it to be given against.
        let mut shell = shell(24, 80);
        shell.apply(UiEvent::Approval(question(asked())));
        shell.decide(Input::Text('1'), Instant::now());
        assert_eq!(shell.controlled(), None, "an unread question was answered");
        let said = unwrapped(&shell.released());
        assert!(
            said.contains(&unwrapped(&[APPROVAL_NOT_READY.to_string()])),
            "the refusal was silent: {said:?}"
        );

        delivered(&mut shell);
        assert_eq!(
            shell.controlled(),
            None,
            "the early key was consumed, not queued for a later grant"
        );

        shell.decide(Input::Text('1'), Instant::now());
        assert_eq!(
            shell.controlled(),
            Some(TurnControl::Answer {
                id: ApprovalId(1),
                answer: ApprovalAnswer::Once,
                feedback: None,
            })
        );
    }

    #[test]
    fn an_affirmative_on_the_review_plane_waits_for_that_planes_own_frame() {
        // The same rule on the other surface, and it is not the same frame: the
        // review plane's composition is a whole screen rather than a band's
        // worth of rows, so a receipt earned by the band would be a receipt
        // about something the user is not looking at.
        let mut shell = shell(24, 80);
        shell.apply(UiEvent::Approval(question(asked_about_a_large_change())));
        assert_eq!(shell.screen_owner(), ScreenOwner::Alternate);
        assert!(
            !shell.screen_rows().is_empty(),
            "the plane has nothing on it, so this case proves nothing"
        );
        shell.decide(Input::Text('1'), Instant::now());
        assert_eq!(shell.controlled(), None, "an unread change was approved");

        delivered(&mut shell);
        shell.decide(Input::Text('1'), Instant::now());
        assert_eq!(
            shell.controlled(),
            Some(TurnControl::Answer {
                id: ApprovalId(1),
                answer: ApprovalAnswer::Once,
                feedback: None,
            })
        );
    }

    #[test]
    fn refusing_is_always_possible_before_any_frame_lands() {
        // A gate that held the refusals as well would leave a user who cannot
        // read the question with no way to say no to it either -- which turns a
        // safety check into a session that cannot be got out of.
        for key in [Input::Text('3'), Input::Action(Action::Cancel)] {
            let mut shell = shell(24, 80);
            shell.apply(UiEvent::Approval(question(asked())));
            shell.decide(key.clone(), Instant::now());
            assert_eq!(
                shell.controlled(),
                Some(TurnControl::Answer {
                    id: ApprovalId(1),
                    answer: ApprovalAnswer::Deny,
                    feedback: None,
                }),
                "{key:?} was gated"
            );
        }
        // Escape cancels the turn, and a cancel needs a turn to be about
        // ([`Shell::interrupt`] reads `WorkHandle::outstanding`), so this half
        // is driven with one running.
        let mut shell = shell(24, 80);
        let started = turn_running(&mut shell, b"edit the notes\r");
        shell.apply(UiEvent::Approval(question(asked())));
        shell.decide(Input::Action(Action::Escape), started);
        assert!(matches!(
            shell.controlled(),
            Some(TurnControl::Cancel { .. })
        ));
    }

    #[test]
    fn a_screen_that_cannot_disclose_the_request_says_so_and_still_refuses() {
        // Thirty cells cannot show the second choice whole, so no frame on this
        // screen will ever grant -- and the sentence has to say that rather than
        // "not yet", because waiting is not what fixes it.
        let mut shell = shell(24, 30);
        shell.apply(UiEvent::Approval(question(asked())));
        delivered(&mut shell);
        shell.decide(Input::Text('1'), Instant::now());
        assert_eq!(shell.controlled(), None);
        let said = unwrapped(&shell.released());
        assert!(
            said.contains(&unwrapped(&[APPROVAL_NOT_DISCLOSED.to_string()])),
            "the refusal did not say why: {said:?}"
        );

        shell.decide(Input::Text('3'), Instant::now());
        assert_eq!(
            shell.controlled(),
            Some(TurnControl::Answer {
                id: ApprovalId(1),
                answer: ApprovalAnswer::Deny,
                feedback: None,
            }),
            "a question that cannot be granted must still be refusable"
        );
    }

    /// The same, with a second prompt waiting behind the interrupted turn.
    ///
    /// Queued **before** the question arrives, because that is the only order a
    /// session can produce one in: the panel takes the focus when it appears,
    /// so nothing can be typed into the composer while it is up.
    fn asking_with_one_waiting(shell: &mut Fixture) -> Instant {
        let started = turn_running(shell, b"edit the notes\r");
        shell.route_bytes(b"queued while deciding\r");
        assert_eq!(shell.hint(), queued_hint(1), "the prompt was not taken");
        shell.apply(UiEvent::Approval(question(asked())));
        shell.settle_band(started);
        let _ = shell.document();
        started
    }

    #[test]
    fn a_question_takes_the_composers_rows_and_sits_between_two_rules() {
        // Upstream's question replaces the footer -- the composer, its blank
        // and the statusline -- with its own rules, rows and hint
        // (`render_input.zig:705`; `fx-approvals-40x120-edit-ask` rows 5-20).
        // So the bottom rule and the hint row stay put, the composer's row
        // becomes the rule that heads the question, and the panel's rows come
        // off the bottom of the document. (Renamed and re-coordinated: the
        // question sat *under* a still-painted composer before.)
        let mut shell = shell(24, 80);
        let working = {
            let started = turn_running(&mut shell, b"edit the notes\r");
            let _ = started;
            shell.geometry
        };
        assert_eq!(working.panel, 0);

        shell.apply(UiEvent::Approval(question(asked())));

        assert_eq!(
            shell.geometry.panel, PANEL_ROWS,
            "the band did not give the question the rows it measured"
        );
        assert_eq!(shell.geometry.divider, working.divider);
        assert_eq!(shell.geometry.input_rows(), 0, "the composer kept a row");
        assert_eq!(shell.geometry.input_first, working.input_first - PANEL_ROWS);
        assert_eq!(shell.geometry.hint, working.hint);
        assert_eq!(
            shell.geometry.content_bottom,
            working.content_bottom - PANEL_ROWS
        );

        let rows = shell.band_rows();
        assert_eq!(
            rows.len(),
            usize::from(shell.geometry.band_rows()),
            "the band painted a different number of rows than it solved for"
        );
        let painted = uncoloured_rows(&rows).join("\n");
        assert!(
            painted.contains("Permission needed · Review change"),
            "{painted}"
        );
        assert!(
            painted.contains("notes.txt  ·  Apply this change?"),
            "{painted}"
        );
        assert!(
            painted.contains("Apply + allow future edits to this file for this session"),
            "{painted}"
        );
        assert!(
            painted.contains("for the rest of this session"),
            "the panel never said what \"always\" would grant: {painted}"
        );
        assert!(
            !painted.contains(RAIL),
            "the composer was painted under the question: {painted}"
        );
        // And the hint row is the question's: what its keys do.
        assert_eq!(
            uncoloured(rows.last().expect("a hint row")),
            "  1–3 choose now    ↑↓ options    tab amend    enter confirm    esc cancel"
        );
    }

    #[test]
    fn a_question_over_the_catalog_takes_its_slot_and_its_hint_row_until_it_is_answered() {
        // The catalog and a question share the slot and the hint row, and the
        // question outranks it in both ([`Shell::slot`]): a hint row still
        // naming the catalog's keys would describe keys the question is
        // swallowing. Answered, the catalog -- which the question did not
        // close -- has both back.
        let mut shell = shell(24, 80);
        let started = turn_running(&mut shell, b"edit the notes\r");
        shell.route_bytes(b"/help\r");
        assert!(shell.help_menu.is_some(), "/help did not open the catalog");
        let catalog_help = format!(
            "{}{}{}",
            PALETTE.dim(),
            help_menu::key_help(80),
            PALETTE.reset()
        );
        assert_eq!(shell.band_rows().last(), Some(&catalog_help));

        shell.apply(UiEvent::Approval(question(asked())));
        shell.settle_band(started);
        let rows = shell.band_rows();
        let painted = uncoloured_rows(&rows).join("\n");
        assert!(
            painted.contains("Permission needed · Review change"),
            "{painted}"
        );
        assert_eq!(
            uncoloured(rows.last().expect("a hint row")),
            "  1–3 choose now    ↑↓ options    tab amend    enter confirm    esc cancel"
        );
        // And its keys: the ones both bind go to the question, not to the
        // catalog standing behind it.
        let catalog = shell.help_menu.clone();
        shell.route_bytes(&[0x1b, 0x5b, 0x42]); // Down
        shell.route_bytes(&[0x1b, 0x5b, 0x5a]); // Shift-Tab
        assert!(shell.marked().starts_with("  ❯ 2"), "{}", shell.marked());
        assert_eq!(
            shell.help_menu, catalog,
            "the catalog took a question's key"
        );

        delivered(&mut shell);
        shell.route_bytes(b"3");
        assert!(shell.panel.is_none(), "the answer left the question up");
        assert_eq!(shell.band_rows().last(), Some(&catalog_help));
    }

    #[test]
    fn the_caret_sits_on_the_choice_enter_would_take_rather_than_in_the_composer() {
        // Where the caret is *is* what the terminal says the focus is, and the
        // focus really has moved: the next keystroke does not reach the draft.
        let mut shell = shell(24, 80);
        let started = turn_running(&mut shell, b"edit the notes\r");
        shell.route_bytes(b"a draft");
        shell.apply(UiEvent::Approval(question(asked())));
        shell.settle_band(started);

        assert_eq!(shell.marked(), "  ❯ 1  Apply once");
        shell.route_bytes(&[0x1b, 0x5b, 0x42]); // Down
        assert!(
            shell.marked().starts_with("  ❯ 2  Apply + allow"),
            "the caret did not follow the marker: {:?}",
            shell.marked()
        );
        assert_eq!(shell.controlled(), None, "moving the marker answered");
    }

    #[test]
    fn a_digit_answers_the_question_and_the_band_gives_its_rows_straight_back() {
        for (typed, answer) in [
            (b'1', ApprovalAnswer::Once),
            (b'2', ApprovalAnswer::Always),
            (b'3', ApprovalAnswer::Deny),
        ] {
            let mut shell = shell(24, 80);
            let before = {
                asking(&mut shell);
                shell.geometry
            };
            assert_eq!(before.panel, PANEL_ROWS);

            shell.route_bytes(&[typed]);

            assert_eq!(
                shell.controlled(),
                Some(TurnControl::Answer {
                    id: ApprovalId(1),
                    answer,
                    feedback: None,
                }),
                "typing {} did not answer the question",
                typed as char
            );
            assert_eq!(shell.geometry.panel, 0, "the band kept the panel's rows");
            assert!(
                !shell.band_rows().join("\n").contains("Permission needed"),
                "an answered question stayed on the screen"
            );
            assert!(
                shell.editor.is_empty(),
                "the digit was typed into the composer as well"
            );
        }
    }

    #[test]
    fn enter_takes_the_marked_choice_and_tab_and_the_arrows_are_what_mark_it() {
        let mut shell = shell(24, 80);
        asking(&mut shell);

        // **Down rather than Tab, and that is a changed coordinate rather than
        // a loosened claim.** Tab stopped being a second spelling of Down when
        // the amendment draft arrived: on a draft-eligible choice it opens that
        // choice's draft (`super::super::approval_amendment`), and only on
        // `Always` does it still cycle. Both halves of the new Tab are asserted
        // at the same strength in
        // `tab_enters_the_draft_of_an_eligible_choice_and_cycles_past_an_ineligible_one`;
        // what this case is about is that **Enter takes what the marker is on**,
        // and the arrows are what move it.
        shell.route_bytes(&[0x1b, 0x5b, 0x42]); // Down, to the second choice
        assert_eq!(
            shell.controlled(),
            None,
            "an arrow answered instead of moving"
        );
        shell.route_bytes(&[0x1b, 0x5b, 0x41]); // Up, back to the first
        shell.route_bytes(&[0x1b, 0x5b, 0x42]); // Down, forward again
        shell.route_bytes(&[0x0d]);

        assert_eq!(
            shell.controlled(),
            Some(TurnControl::Answer {
                id: ApprovalId(1),
                answer: ApprovalAnswer::Always,
                feedback: None,
            }),
            "Enter did not take the marked choice"
        );
    }

    #[test]
    fn escape_at_a_question_cancels_the_turn_rather_than_arming_the_clear() {
        // The double-Escape gesture is the composer's, and the composer does
        // not have the focus. A first Escape that armed it would leave a
        // question up and a warning about a draft the user is not editing.
        // (Claim changed: Escape refused the call; it cancels the turn now, as
        // upstream's does -- divergence 7.)
        let mut shell = shell(24, 80);
        let started = turn_running(&mut shell, b"edit the notes\r");
        shell.route_bytes(b"a draft");
        shell.apply(UiEvent::Approval(question(asked())));
        shell.settle_band(started);
        let accepted = shell.work.accepted();

        shell.route_bytes(&[0x1b]);
        shell.settle_input(Instant::now() + Duration::from_millis(100));

        assert_eq!(
            shell.controlled(),
            Some(TurnControl::Cancel { through: accepted })
        );
        assert!(!shell.hint().contains(ESCAPE_ARMED), "the clear was armed");
        assert_eq!(
            shell.editor.text(),
            "a draft",
            "the refusal threw the draft away"
        );
    }

    #[test]
    fn escape_at_a_question_is_the_interrupt_that_cancels_the_turn() {
        // **The message on the wire is a cancellation, not an answer**, and the
        // difference is the whole of this case. The refusal of the question
        // comes back with it: the prompter is what is parked on this channel,
        // and it turns a cancellation into a `Deny` and hands the cancellation
        // on to the loop that stops the turn and drops the queue
        // (`super::approval::TuiPrompter`, and the composed proof in
        // `tests/tui.rs`). An `Answer(Deny)` sent from here would be the only
        // thing the runtime ever heard, and the cancelled turn would carry on.
        // (Renamed: this was Ctrl-C's; upstream gives it to Escape,
        // `app_input_runtime.zig:3139-3143`.)
        let mut shell = shell(24, 80);
        // A prompt is waiting behind the cancelled turn, so the message this
        // sends has to carry the watermark that says which waiting work the
        // keystroke was about.
        asking_with_one_waiting(&mut shell);
        let accepted = shell.work.accepted();

        shell.route_bytes(&[0x1b]);
        shell.settle_input(Instant::now() + Duration::from_millis(100));

        assert_eq!(
            shell.controlled(),
            Some(TurnControl::Cancel { through: accepted }),
            "the panel ate the interrupt and answered for it"
        );
        assert_eq!(
            shell.controlled(),
            None,
            "a second message went with it; the prompter answers the question \
             from the cancellation itself"
        );
        // And the user was told, in the line upstream answers a cancelled
        // turn with and the sentence about the queue (claim changed: the
        // first was the line shell's interrupt notice).
        let said = shell.released();
        assert!(
            said.contains(&cancelled()[0]),
            "the interrupt landed silently: {said:?}"
        );
        assert!(
            said.contains(&notice::Notice::cancelled("queue", QUEUE_DROPPED).text()),
            "the queue went with it and nobody said so: {said:?}"
        );
        assert_eq!(shell.geometry.panel, 0, "the question stayed on the screen");
        assert!(
            !shell.leaving(),
            "one Escape at a question left the session"
        );
        // An answer to the question rather than half of a way out: upstream's
        // `cancelApprovalOperation` arms no exit (`fx-approvals-cc-…-after-esc`).
        assert!(
            !shell.hint().contains(EXIT_ARMED),
            "an Escape at a question armed the exit"
        );
    }

    #[test]
    fn ctrl_c_at_a_question_answers_only_the_question_it_is_about() {
        // The other half of the pair: Ctrl-C is an answer about *this call*
        // and nothing more (`src/ui/input/runtime.zig:61`, `3 => .deny`;
        // upstream's e2e `tui-permissions.test.ts:1099`). A turn stopped by it
        // would make it the same key as Escape. (Renamed: this was Escape's.)
        let mut shell = shell(24, 80);
        asking_with_one_waiting(&mut shell);

        shell.route_bytes(&[0x03]);

        assert_eq!(
            shell.controlled(),
            Some(TurnControl::Answer {
                id: ApprovalId(1),
                answer: ApprovalAnswer::Deny,
                feedback: None,
            })
        );
        assert_eq!(
            shell.controlled(),
            None,
            "Ctrl-C cancelled the turn as well"
        );
        assert_eq!(
            shell.hint(),
            queued_hint(1),
            "Ctrl-C dropped the prompt that was waiting"
        );
        assert!(
            !shell.leaving(),
            "one Ctrl-C at a question left the session"
        );
    }

    #[test]
    fn a_keystroke_the_question_has_no_binding_for_reaches_nothing_at_all() {
        // The panel has the focus, so a Ctrl-D at one does not end a session
        // that is holding a turn open waiting to be told what to do, and typed
        // text does not accumulate in a composer whose caret is elsewhere.
        let mut shell = shell(24, 80);
        asking(&mut shell);

        shell.route_bytes(b"hello");
        shell.route_bytes(&[0x04]);
        shell.route_bytes(&[0x7f]);

        assert!(
            shell.editor.is_empty(),
            "the panel leaked into the composer"
        );
        assert!(!shell.leaving(), "Ctrl-D left a session with a question up");
        assert_eq!(shell.controlled(), None);
        assert!(
            shell.band_rows().join("\n").contains("Permission needed"),
            "the question went away without being answered"
        );
    }

    #[test]
    fn the_turns_clock_stops_while_the_question_is_up_and_starts_again_after_it() {
        // What that interval measures is the person, not the model. A turn that
        // spent four minutes waiting to be told whether it could edit a file
        // did not spend four minutes thinking, and a row that said so would be
        // the one number on the band nobody could trust.
        let mut shell = shell(24, 80);
        let started = turn_running(&mut shell, b"edit the notes\r");
        shell.settle_band(started + Duration::from_secs(2));
        let before = shell.activity_row.clone().expect("a running turn");
        assert!(before.contains("2s"), "{before:?}");

        // The question arrives, and the next tick of the loop is what stops the
        // clock -- the same seam every other timed row is settled on. The row
        // itself is off the band while the question is up, as upstream's is
        // (`render_input.zig:705`): what proves the clock stopped is the
        // number it comes back with.
        shell.apply(UiEvent::Approval(question(asked())));
        shell.settle_band(started + Duration::from_secs(2));
        shell.settle_band(started + Duration::from_secs(30));
        assert_eq!(
            shell.activity_row, None,
            "the turn's row was painted beside the question"
        );
        delivered(&mut shell);

        shell.route_bytes(b"1");
        shell.settle_band(started + Duration::from_secs(30));
        shell.settle_band(started + Duration::from_secs(33));
        let after = shell.activity_row.clone().expect("the turn goes on");
        assert!(
            after.contains("5s"),
            "the turn was charged for the time it spent waiting for a person: \
             {after:?}"
        );
    }

    // -----------------------------------------------------------------------
    // which plane the question belongs to
    // -----------------------------------------------------------------------

    #[test]
    fn a_change_too_big_for_the_band_gives_the_screen_to_the_approval_plane_and_an_answer_gives_it_back(
    ) {
        // The owner is a state of the *session*, not a property of the request:
        // it is taken when the question arrives and it has to be given back
        // when the question is answered, or the plane a later frame is composed
        // for would be one nobody is looking at.
        let mut shell = shell(24, 80);
        assert_eq!(shell.screen_owner(), ScreenOwner::Primary);
        let started = turn_running(&mut shell, b"edit the notes\r");

        shell.apply(UiEvent::Approval(question(asked_about_a_large_change())));
        shell.settle_band(started);
        assert_eq!(
            shell.screen_owner(),
            ScreenOwner::Alternate,
            "a change the band cannot show was left for the band to show"
        );
        delivered(&mut shell);

        shell.route_bytes(b"1");
        assert_eq!(
            shell.controlled(),
            Some(TurnControl::Answer {
                id: ApprovalId(1),
                answer: ApprovalAnswer::Once,
                feedback: None,
            })
        );
        assert_eq!(
            shell.screen_owner(),
            ScreenOwner::Primary,
            "an answered question kept the screen"
        );
    }

    #[test]
    fn a_small_mutation_keeps_the_primary_plane_and_is_asked_in_the_band() {
        // The common case, and the one a screen would be a regression for: the
        // document stays visible behind a question the band can hold whole.
        let mut shell = shell(24, 80);
        let started = turn_running(&mut shell, b"edit the notes\r");

        shell.apply(UiEvent::Approval(question(asked())));
        shell.settle_band(started);

        assert_eq!(shell.screen_owner(), ScreenOwner::Primary);
        assert_eq!(shell.geometry.panel, PANEL_ROWS);
        assert!(shell.band_rows().join("\n").contains("Permission needed"));
    }

    #[test]
    fn a_question_the_approval_plane_owns_still_takes_the_focus_and_every_choice() {
        // Routing is settled by the question having the focus, not by which
        // plane is painting it. Every one of the three answers, the marker
        // walk, and the refusal key -- because a surface that changed which
        // keystroke means what would be a second input model to get wrong.
        // (Ctrl-C rather than Escape for the refusal key: Escape cancels the
        // turn now, which `an_escape_at_a_question_the_approval_plane_owns_gives_the_screen_back_as_well`
        // pins on this plane.)
        for (typed, answer) in [
            (&b"1"[..], ApprovalAnswer::Once),
            (&b"2"[..], ApprovalAnswer::Always),
            (&b"3"[..], ApprovalAnswer::Deny),
            (&[0x03][..], ApprovalAnswer::Deny),
        ] {
            let mut shell = shell(24, 80);
            let started = turn_running(&mut shell, b"edit the notes\r");
            shell.route_bytes(b"a draft");
            shell.apply(UiEvent::Approval(question(asked_about_a_large_change())));
            shell.settle_band(started);
            assert_eq!(shell.screen_owner(), ScreenOwner::Alternate);
            assert_eq!(
                shell.marked(),
                "  ❯ 1  Apply once",
                "the caret left the question"
            );
            delivered(&mut shell);

            shell.route_bytes(typed);
            shell.settle_input(Instant::now() + Duration::from_millis(100));

            assert_eq!(
                shell.controlled(),
                Some(TurnControl::Answer {
                    id: ApprovalId(1),
                    answer,
                    feedback: None,
                }),
                "{typed:?} did not answer a question the approval plane owns"
            );
            assert_eq!(shell.screen_owner(), ScreenOwner::Primary);
            assert_eq!(
                shell.editor.text(),
                "a draft",
                "the keystroke fell through into the composer"
            );
        }
    }

    #[test]
    fn an_escape_at_a_question_the_approval_plane_owns_gives_the_screen_back_as_well() {
        // Escape is the one key that is not an answer on this surface -- it
        // cancels the turn and the prompter turns that into the refusal. The
        // screen has to come back on that path too, or a cancel would leave the
        // session composing frames for a plane with no question on it.
        // (Renamed: this was Ctrl-C's before divergence 7.)
        let mut shell = shell(24, 80);
        let started = turn_running(&mut shell, b"edit the notes\r");
        shell.apply(UiEvent::Approval(question(asked_about_a_large_change())));
        shell.settle_band(started);
        assert_eq!(shell.screen_owner(), ScreenOwner::Alternate);

        shell.route_bytes(&[0x1b]);
        shell.settle_input(Instant::now() + Duration::from_millis(100));

        assert_eq!(shell.screen_owner(), ScreenOwner::Primary);
        assert!(
            matches!(shell.controlled(), Some(TurnControl::Cancel { .. })),
            "the interrupt was answered instead of being passed on"
        );
    }

    #[test]
    fn a_short_screen_reviews_a_large_change_on_the_plane_that_can_show_it() {
        // **The band's fit is the band's question.** A change too big for the
        // band's review is never asked in the band at all -- it takes a plane of its
        // own, and every row of that plane is the question -- so measuring it
        // against the rows the band's panel would have needed is refusing on
        // behalf of a surface that was never going to be used. Ten rows is
        // exactly that case: the inline panel really does not fit there
        // (`a_screen_too_small_to_show_the_question_refuses_it_and_says_so`
        // pins the inline half at the same size), and the plane shows the
        // question whole.
        let mut shell = shell(10, 80);
        let started = turn_running(&mut shell, b"edit the notes\r");

        shell.apply(UiEvent::Approval(question(asked_about_a_large_change())));
        shell.settle_band(started);

        assert_eq!(shell.screen_owner(), ScreenOwner::Alternate);
        assert_eq!(
            shell.controlled(),
            None,
            "a question the plane can show was answered on the user's behalf"
        );
        let painted = uncoloured_rows(&shell.screen_rows()).join("\n");
        for choice in ["1  Apply once", "2  Apply + allow", "3  Don't apply"] {
            assert!(
                painted.contains(choice),
                "the plane dropped {choice:?} on a {}-row screen: {painted:?}",
                shell.geometry.rows
            );
        }
        assert!(
            !shell
                .released()
                .concat()
                .contains("this screen is too small"),
            "the session was told the screen was too small for a question it is showing"
        );
    }

    #[test]
    fn every_screen_a_session_can_run_on_can_ask_a_question_on_the_other_plane() {
        // The premise the refusal above rests on, driven rather than argued: a
        // session exists only on a screen the band fits on
        // (`layout::MIN_ROWS`/`MIN_COLS`), and on **every** such screen the
        // review plane can put all three answers in front of the user. So the
        // fail-closed branch in `ask` guards a case no live geometry reaches,
        // and the refusal it would produce is a hazard written down rather
        // than a behaviour a session can be walked into.
        for rows in layout::MIN_ROWS..=40 {
            for cols in [layout::MIN_COLS, 40, 80, 200] {
                let mut shell = shell(rows, cols);
                let started = turn_running(&mut shell, b"edit the notes\r");
                shell.apply(UiEvent::Approval(question(asked_about_a_large_change())));
                shell.settle_band(started);
                assert_eq!(
                    shell.screen_owner(),
                    ScreenOwner::Alternate,
                    "a {rows}x{cols} screen refused a question the plane can show"
                );
                assert_eq!(
                    shell.controlled(),
                    None,
                    "a {rows}x{cols} screen answered on the user's behalf"
                );
            }
        }
    }

    #[test]
    fn a_question_the_other_plane_owns_is_painted_there_and_not_in_the_band() {
        // The band underneath the question is an **ordinary** band, and that is
        // what makes the return cheap and correct: the frame that gives the
        // plane back repaints exactly this, at exactly these rows, so the
        // session comes back to the screen it left.
        //
        // 7A pinned the opposite -- the band painted an Approval-owned question
        // because there was no other surface to paint it on, and its own
        // mutation M32 guarded against this checkpoint half-landing there. This
        // is that guard turned over: the surface now exists, so the band gives
        // the rows back rather than keeping a second copy of the question.
        let mut shell = shell(24, 80);
        let started = turn_running(&mut shell, b"edit the notes\r");
        shell.apply(UiEvent::Approval(question(asked_about_a_large_change())));
        shell.settle_band(started);
        assert_eq!(shell.screen_owner(), ScreenOwner::Alternate);

        let painted = shell.band_rows().join("\n");
        assert!(
            !painted.contains("Permission needed"),
            "the question is up twice, on two planes: {painted:?}"
        );
        assert_eq!(
            shell.geometry.panel, 0,
            "the band kept rows for a question it is not painting"
        );

        // And the plane it *is* painted on carries the question, the change and
        // every answer.
        let screen = shell.screen_rows();
        assert_eq!(
            screen.len(),
            usize::from(shell.geometry.rows),
            "the alternate screen is not the whole terminal"
        );
        let screen = uncoloured_rows(&screen).join("\n");
        for needle in [
            "Permission needed",
            "notes.txt",
            "1  Apply once",
            "3  Don't apply",
        ] {
            assert!(screen.contains(needle), "{needle:?} is missing: {screen:?}");
        }
        assert!(
            screen.contains("+ beta"),
            "the change the screen exists to show is not on it: {screen:?}"
        );

        // **No row carries the bytes that take the plane.** The `1049` pair is
        // the frame composer's (`super::frame::Band`), written around a paint
        // rather than inside one; a row that carried it would be a row a
        // provider's text could carry it in.
        for rows in [shell.band_rows(), shell.screen_rows()] {
            let painted = rows.join("\n");
            assert!(!painted.contains("1049"), "{painted:?}");
            assert!(!painted.contains("\u{1b}[?"), "{painted:?}");
        }
    }

    #[test]
    fn an_answered_question_leaves_nothing_composing_for_the_other_plane() {
        // The surface goes with the answer, in the same statement the panel and
        // the owner go in. One left behind would keep composing a screen out of
        // a question that has been answered, and the next frame the loop asked
        // for on that plane would paint it.
        let mut shell = shell(24, 80);
        let started = turn_running(&mut shell, b"edit the notes\r");
        shell.apply(UiEvent::Approval(question(asked_about_a_large_change())));
        shell.settle_band(started);
        assert!(!shell.screen_rows().is_empty(), "nothing was installed");
        delivered(&mut shell);

        shell.route_bytes(b"1");

        assert_eq!(
            shell.controlled(),
            Some(TurnControl::Answer {
                id: ApprovalId(1),
                answer: ApprovalAnswer::Once,
                feedback: None,
            })
        );
        assert_eq!(shell.screen_owner(), ScreenOwner::Primary);
        assert!(
            shell.screen_rows().is_empty(),
            "an answered question is still composing rows for the other plane: {:?}",
            shell.screen_rows()
        );
    }

    #[test]
    fn an_exit_can_give_the_other_plane_back_without_answering_the_question() {
        // The exit's own door (`super::event_loop`'s `shut_down`). A session
        // coming down with a question up has to leave the terminal on the plane
        // its user's shell is on, and there is nobody left to answer -- so the
        // plane is released without an answer being invented for the runtime.
        let mut shell = shell(24, 80);
        let started = turn_running(&mut shell, b"edit the notes\r");
        shell.apply(UiEvent::Approval(question(asked_about_a_large_change())));
        shell.settle_band(started);
        assert_eq!(shell.screen_owner(), ScreenOwner::Alternate);

        shell.release_screen();

        assert_eq!(shell.screen_owner(), ScreenOwner::Primary);
        assert!(
            shell.screen_rows().is_empty(),
            "the plane was given back and the surface is still composing rows"
        );
        assert_eq!(
            shell.controlled(),
            None,
            "the exit answered a question on the user's behalf"
        );
    }

    #[test]
    fn a_change_longer_than_the_screen_can_be_walked_from_the_question() {
        // A bounded diff is up to 128 KiB and a screen is a few dozen rows, so
        // a review surface that could not be walked would show the head of a
        // change and call it the change.
        let mut shell = shell(24, 80);
        let started = turn_running(&mut shell, b"edit the notes\r");
        shell.apply(UiEvent::Approval(question(asked_about_a_large_change())));
        shell.settle_band(started);
        let first = shell.screen_rows().join("\n");

        // `C-p`, which is "the line before this one" everywhere else on this
        // surface and was bound to nothing at a question. (Order changed: the
        // review opens at its tail now, against the controls, so the first
        // walk is back toward the head.)
        shell.route_bytes(&[0x10]);
        let moved = shell.screen_rows().join("\n");
        assert_ne!(first, moved, "the change could not be walked");
        assert_eq!(
            shell.controlled(),
            None,
            "walking the change answered the question"
        );

        shell.route_bytes(&[0x0e]);
        assert_eq!(
            shell.screen_rows().join("\n"),
            first,
            "`C-n` did not walk back"
        );
        assert_eq!(shell.screen_owner(), ScreenOwner::Alternate);
    }

    #[test]
    fn a_screen_too_small_to_show_the_question_refuses_it_and_says_so() {
        // Never an allow. A panel with its choices below the last row of the
        // screen would leave the session waiting for a keystroke about a
        // question nobody can read, which is worse than a change that did not
        // happen.
        let mut shell = shell(10, 80);
        turn_running(&mut shell, b"edit the notes\r");
        shell.apply(UiEvent::Approval(question(asked())));

        assert_eq!(shell.geometry.panel, 0);
        assert_eq!(
            shell.controlled(),
            Some(TurnControl::Answer {
                id: ApprovalId(1),
                answer: ApprovalAnswer::Deny,
                feedback: None,
            })
        );
        // An error under `permissions`, its rows joined, because an 80-column
        // screen wraps the sentence and what is pinned is that it was said.
        assert_eq!(
            joined(&shell.released()),
            notice::Notice::error("permissions", PANEL_TOO_SMALL).text(),
            "the refusal is not the notice it should be"
        );
        assert!(!shell.band_rows().join("\n").contains("Permission needed"));
    }

    #[test]
    fn a_draft_at_its_cap_gives_rows_back_to_the_question_rather_than_hiding_it() {
        // A panel and a composer at its own cap can want more rows than a short
        // screen has. The draft is the half that can afford to lose one --  it
        // scrolls, and the caret stays visible -- while a question with its
        // choices off the bottom is a question with no answers.
        let mut shell = shell(14, 80);
        let limit = crate::tui::layout::input_row_limit(14);
        shell.route_bytes("x\n".repeat(usize::from(limit) + 2).as_bytes());
        assert_eq!(shell.geometry.input_rows(), limit);
        turn_running(&mut shell, &[]);

        shell.apply(UiEvent::Approval(question(asked())));

        assert_eq!(
            shell.geometry.panel, PANEL_ROWS,
            "the question was refused instead"
        );
        assert!(
            shell.geometry.input_rows() < limit,
            "the composer kept every row and the panel was painted off-screen"
        );
        assert_eq!(
            shell.band_rows().len(),
            usize::from(shell.geometry.band_rows())
        );
        assert!(
            shell.geometry.content_bottom >= 1,
            "the band took the whole screen"
        );
        // And the caret is still on the panel rather than on a composer row
        // that no longer exists.
        assert_eq!(shell.marked(), "  ❯ 1  Apply once");
    }

    #[test]
    fn a_question_that_arrives_while_a_prompt_is_queued_is_still_answerable() {
        // Scenario 10b, on this side of the channel: the answer travels on the
        // control channel, which the runtime drains *inside* a turn, so it does
        // not queue behind a submission the turn cannot dequeue until it ends.
        let mut shell = shell(24, 80);
        asking(&mut shell);
        // The composer has no focus, so the queued prompt is submitted the only
        // way it can be: by the runtime having taken one already.
        assert_eq!(
            shell.picks_up(),
            TurnWork::Submit("edit the notes".to_string())
        );

        shell.route_bytes(b"1");

        assert_eq!(
            shell.controlled(),
            Some(TurnControl::Answer {
                id: ApprovalId(1),
                answer: ApprovalAnswer::Once,
                feedback: None,
            }),
            "the answer went nowhere"
        );
        assert!(
            shell.sent.try_recv().is_err(),
            "the answer was sent as work rather than as control"
        );
    }

    // -----------------------------------------------------------------------
    // the model's own questions (`ask_user_question`)
    // -----------------------------------------------------------------------

    /// One choice, in the shape `crate::tools::question::parse` produces: the
    /// text is already terminal-safe encoded, and no freeform slot is in it --
    /// the panel appends that itself.
    fn choice(label: &str, description: Option<&str>) -> QuestionOption {
        QuestionOption {
            label: label.to_string(),
            description: description.map(str::to_string),
        }
    }

    /// A batch of two questions, named so the answers are predictable.
    fn a_batch() -> QuestionRequest {
        QuestionRequest {
            id: QuestionId(1),
            entries: vec![
                QuestionEntry {
                    question: "Which depth?".to_string(),
                    options: vec![
                        choice("Thorough", Some("reads every file")),
                        choice("Quick", None),
                    ],
                },
                QuestionEntry {
                    question: "Ship it?".to_string(),
                    options: vec![choice("Yes", None), choice("No", None)],
                },
            ],
        }
    }

    /// One question with the six choices the tool admits at most, which is seven
    /// rows once the freeform slot is appended -- more than a short screen has.
    fn six_option_batch() -> QuestionRequest {
        QuestionRequest {
            id: QuestionId(2),
            entries: vec![QuestionEntry {
                question: "Which file?".to_string(),
                options: (1..=6)
                    .map(|index| choice(&format!("file{index}.rs"), None))
                    .collect(),
            }],
        }
    }

    /// A shell with a turn running and a question batch in front of the user.
    ///
    /// The document is deliberately **not** drained here: a batch the screen
    /// cannot show says so in the document, and a fixture that swallowed the
    /// pending rows would leave that case asserting against an empty vector.
    fn asking_question(fixture: &mut Fixture, request: QuestionRequest) -> Instant {
        let started = turn_running(fixture, b"ask me\r");
        fixture.apply(UiEvent::Question(request));
        fixture.settle_band(started);
        started
    }

    #[test]
    fn a_question_takes_the_band_and_the_focus() {
        let mut fixture = shell(24, 80);
        asking_question(&mut fixture, a_batch());
        assert!(fixture
            .band_rows()
            .iter()
            .any(|row| row.contains("1) Thorough")));
        fixture.route_bytes(b"1");
        assert!(
            fixture.editor.is_empty(),
            "a digit answered rather than typing"
        );
    }

    #[test]
    fn the_answers_reach_the_runtime_in_order_under_the_request_id() {
        let mut fixture = shell(24, 80);
        asking_question(&mut fixture, a_batch());
        fixture.route_bytes(b"1");
        fixture.route_bytes(b"2");
        match fixture.control.try_recv().expect("an answer was sent") {
            TurnControl::QuestionAnswer { id, answers } => {
                assert_eq!(id, QuestionId(1));
                assert_eq!(answers, vec!["Thorough".to_string(), "No".to_string()]);
            }
            other => panic!("got {other:?}"),
        }
        assert!(
            fixture
                .band_rows()
                .iter()
                .all(|row| !row.contains("1) Thorough")),
            "the panel came down before the answer went out"
        );
    }

    #[test]
    fn a_partly_answered_batch_says_nothing_to_the_runtime_yet() {
        // One tool call, one answer document. A message per question would make
        // the requester's first read an answer to a batch that is still being
        // filled in -- and the model would be handed one answer for two
        // questions (`crate::tools::question::encode_answers` refuses that pair).
        let mut fixture = shell(24, 80);
        asking_question(&mut fixture, a_batch());
        fixture.route_bytes(b"1");
        assert_eq!(
            fixture.controlled(),
            None,
            "the first answer went out on its own"
        );
        // The panel's own title: an 80-column hint row has no room for the
        // batch's counter beside its keys (`input_presentation.zig:241`).
        assert!(
            fixture
                .band_rows()
                .iter()
                .any(|row| row.contains("Ship it?"))
                && !fixture
                    .band_rows()
                    .iter()
                    .any(|row| row.contains("Which depth?")),
            "the batch did not move to its second question: {:?}",
            fixture.band_rows()
        );
    }

    #[test]
    fn a_model_question_takes_the_composers_rows_and_gives_them_back() {
        // Upstream's question replaces the footer -- composer, blank and
        // statusline -- with its own rules, rows and hint
        // (`fx-question-40x120-q1`), as a permission question does; the draft
        // waits, unpainted, in the editor.
        let mut fixture = shell(40, 120);
        let started = turn_running(&mut fixture, b"ask me\r");
        fixture.route_bytes(b"half a thought");
        fixture.apply(UiEvent::Question(a_batch()));
        fixture.settle_band(started);
        assert_eq!(fixture.geometry.input_rows(), 0, "the composer kept rows");
        let rows = fixture.band_rows();
        assert!(
            !rows.iter().any(|row| row.contains(RAIL)),
            "the composer is painted behind the question: {rows:?}"
        );
        assert_eq!(
            rows.len(),
            usize::from(fixture.geometry.band_rows()),
            "the band painted a different number of rows than it solved for"
        );
        fixture.route_bytes(b"1");
        fixture.route_bytes(b"1");
        fixture.settle_band(started);
        assert_eq!(
            fixture.geometry.input_rows(),
            1,
            "the composer did not come back"
        );
        assert_eq!(fixture.editor.text(), "half a thought");
    }

    #[test]
    fn the_hint_row_under_a_model_question_is_the_questions_own() {
        // `fx-question-40x120-q1` row 14: the keys, and the batch's progress at
        // the right edge -- not the statusline.
        let mut fixture = shell(40, 120);
        asking_question(&mut fixture, a_batch());
        let hint = fixture.band_rows().last().cloned().expect("a hint row");
        assert_eq!(
            hint,
            format!(
                "{}1–3 choose now    ↑↓ options    tab questions    enter answer    esc cancel{}Question 1 of 2{}",
                PALETTE.dim(),
                " ".repeat(30),
                PALETTE.reset()
            )
        );
        fixture.route_bytes(b"\t");
        assert!(fixture
            .band_rows()
            .last()
            .expect("a hint row")
            .contains("Question 2 of 2"));
    }

    #[test]
    fn an_answered_batch_leaves_each_question_and_its_answer_in_the_document() {
        // `fx-question-40x120-q2-num` rows 5-9: under the card, each question
        // under its ordinal with the answer hung beneath it, and a blank row
        // before whatever the turn says next.
        let mut fixture = shell(40, 120);
        asking_question(&mut fixture, a_batch());
        let _ = fixture.document();
        fixture.route_bytes(b"1");
        assert_eq!(
            fixture.document(),
            Vec::<String>::new(),
            "half a batch is no record"
        );
        fixture.route_bytes(b"2");
        assert_eq!(
            fixture.document(),
            vec![
                "  1) Which depth?",
                "     Thorough",
                "  2) Ship it?",
                "     No",
                ""
            ]
        );
    }

    #[test]
    fn the_answer_rows_are_in_the_answer_grey_with_their_indent() {
        // `ESC[38;5;245m     SQLite` -- the hang painted with the answer, as
        // upstream writes the field (`question_ui.zig:478-496`).
        let mut fixture = shell(40, 120);
        asking_question(&mut fixture, a_batch());
        let _ = fixture.document();
        fixture.route_bytes(b"1");
        fixture.route_bytes(b"2");
        let rows: Vec<String> = fixture
            .take_pending()
            .into_iter()
            .flat_map(|append| append.rows)
            .collect();
        assert!(
            rows.contains(&format!(
                "{}     Thorough{}",
                PALETTE.answer(),
                PALETTE.reset()
            )),
            "{rows:?}"
        );
        assert!(rows.contains(&"  1) Which depth?".to_string()), "{rows:?}");
    }

    #[test]
    fn escape_and_ctrl_c_at_a_model_question_leave_cancelled_in_the_document() {
        // `fx-question2-40x120-q2-esc` row 14, for either key: one row, and the
        // questions are not repeated (`writeCancelledResolution`). What the
        // two keys tell the runtime is asserted on its own
        // (`escape_at_a_question_cancels_the_turn_and_sends_nothing_else`,
        // `ctrl_c_at_a_question_stops_the_turn_and_sends_nothing_else`). The
        // interrupt's own notice that follows it is the interrupt unit's
        // wording, so only the record's rows are pinned here.
        for key in [&[0x1b][..], &[0x03][..]] {
            let mut fixture = shell(40, 120);
            asking_question(&mut fixture, a_batch());
            let _ = fixture.document();
            fixture.route_bytes(key);
            fixture.settle_input(Instant::now() + Duration::from_millis(100));
            let document = fixture.document();
            assert_eq!(document[..2], ["■ Cancelled", ""], "{key:?}: {document:?}");
        }
    }

    #[test]
    fn escape_at_a_draft_with_text_arms_a_clear_and_only_an_empty_one_cancels() {
        // Upstream's composer contract, at the question's own draft
        // (`app_input_runtime.zig:3120-3135`): the first Escape arms and says
        // so on the hint row, the second inside the window empties the draft
        // and leaves the slot marked, and an Escape at the empty draft is the
        // cancel.
        let mut fixture = shell(40, 120);
        asking_question(&mut fixture, a_batch());
        fixture.route_bytes(b"3");
        fixture.route_bytes(b"MariaDB");
        let now = Instant::now();
        fixture.route_bytes(&[0x1b]);
        fixture.settle_input(now + Duration::from_millis(60));
        assert_eq!(
            fixture.controlled(),
            None,
            "one Escape threw the draft away"
        );
        let hint = fixture.band_rows().last().cloned().expect("a hint row");
        assert!(hint.contains("esc again to clear"), "{hint:?}");
        assert!(fixture.marked().contains("3) MariaDB"));

        fixture.route_bytes(&[0x1b]);
        fixture.settle_input(now + Duration::from_millis(120));
        assert_eq!(fixture.controlled(), None, "the second Escape cancelled");
        assert_eq!(fixture.marked(), "    3)  ", "the draft was not emptied");

        // Past the window, at an empty draft: the turn is cancelled.
        let accepted = fixture.work.accepted();
        fixture.route_bytes(&[0x1b]);
        fixture.settle_input(now + Duration::from_secs(5));
        assert_eq!(
            fixture.controlled(),
            Some(TurnControl::Cancel { through: accepted })
        );
    }

    #[test]
    fn a_bracketed_paste_at_an_open_answer_goes_into_it_and_shift_arrows_choose() {
        // Through the real decoder: `ESC[200~ … ESC[201~` into the field (e2e
        // `tui-decision-prompts.test.ts:2785`), the plain Up walks the answer
        // and `ESC[1;2A` -- Shift+Up -- chooses (`:2825`).
        let mut fixture = shell(40, 120);
        asking_question(&mut fixture, a_batch());
        fixture.route_bytes(b"3");
        fixture.route_bytes(b"\x1b[200~line-one\nxy\nline-three\x1b[201~");
        fixture.route_bytes(b"\x1b[A");
        fixture.route_bytes(b"!");
        fixture.route_bytes(b"\x1b[1;2A");
        assert_eq!(fixture.marked(), "    2) Quick");
        fixture.route_bytes(b"\x1b[1;2B");
        fixture.route_bytes(&[0x0d]);
        fixture.route_bytes(b"1");
        assert_eq!(
            fixture.controlled(),
            Some(TurnControl::QuestionAnswer {
                id: QuestionId(1),
                answers: vec!["line-one\nxy!\nline-three".to_string(), "Yes".to_string()],
            })
        );
        assert!(
            fixture.editor.is_empty(),
            "the paste or the keys reached the composer"
        );
    }

    #[test]
    fn shift_on_the_arrows_is_the_arrow_everywhere_but_a_question() {
        // The decoder keeps Shift on Up and Down for a question's field only;
        // the composer's recall still answers `ESC[1;2A` as Up.
        let mut fixture = shell(24, 80);
        fixture.route_bytes(b"first\r");
        let _ = fixture.picks_up();
        fixture.route_bytes(b"\x1b[1;2A");
        assert_eq!(fixture.editor.text(), "first", "Shift+Up did not recall");
    }

    #[test]
    fn escape_at_a_question_cancels_the_turn_and_sends_nothing_else() {
        // Upstream's Escape at a question (`app_input_runtime.zig:3137` ->
        // `cancelQuestionPrompt`, `input_question_runtime.zig:59-83`): the
        // turn is cancelled, with the one message Ctrl-C sends -- never a
        // `QuestionCancelled`, which would tell the model the question was
        // declined (fx e2e `tui-decision-prompts.test.ts:2454`). Divergence 7.
        let mut fixture = shell(24, 80);
        asking_question(&mut fixture, a_batch());
        let accepted = fixture.work.accepted();
        // A lone `ESC` is the Escape key only once it has gone quiet, which is
        // the decoder's own timeout and the settle is what resolves it.
        fixture.route_bytes(&[0x1b]);
        fixture.settle_input(Instant::now() + Duration::from_millis(100));
        assert_eq!(
            fixture.controlled(),
            Some(TurnControl::Cancel { through: accepted }),
            "Escape at a question left the turn running"
        );
        assert_eq!(
            fixture.controlled(),
            None,
            "a second message: the batch was declined as well as interrupted"
        );
        assert!(
            !fixture.band_rows().join("\n").contains("Which depth?"),
            "the question stayed up"
        );
    }

    #[test]
    fn ctrl_c_at_a_question_stops_the_turn_and_sends_nothing_else() {
        // **One message, and it is the turn's.** Ctrl-C means "stop the work"
        // here as everywhere else on this surface; the requester parked on this
        // channel reads the interrupt as its question being over, answers the
        // tool with the cancellation sentinel and hands the interrupt back to
        // the loop (`super::question::TuiQuestioner`).
        //
        // A `QuestionCancelled` in front of it is what this used to send, and it
        // is a real regression rather than a redundancy: the requester answers
        // on the first message it recognises and returns, so the calls behind
        // the question ran -- files written, another model request spent -- while
        // the interrupt was still sitting in the channel unread
        // (`super::worker`'s
        // `a_ctrl_c_at_a_question_stops_the_turn_before_any_later_call_or_request`).
        let mut fixture = shell(24, 80);
        asking_question(&mut fixture, a_batch());
        let accepted = fixture.work.accepted();

        fixture.route_bytes(&[0x03]);

        assert_eq!(
            fixture.controlled(),
            Some(TurnControl::Cancel { through: accepted }),
            "the turn was left running after the user asked everything to stop"
        );
        assert_eq!(
            fixture.controlled(),
            None,
            "a second message: the batch was answered or refused as well as interrupted"
        );
        assert!(
            !fixture.band_rows().join("\n").contains("Which depth?"),
            "the question stayed up"
        );
    }

    #[test]
    fn a_screen_too_small_for_the_question_cancels_it_rather_than_painting_half() {
        let mut fixture = shell(7, 20);
        asking_question(&mut fixture, a_batch());
        assert!(matches!(
            fixture.control.try_recv(),
            Ok(TurnControl::QuestionCancelled { .. })
        ));
        // `released`, not `document`: the notice is behind the pacer, which is
        // how the existing too-small assertions read it (`shell.rs:5816`,
        // `:6004`). Concatenated rather than compared row by row, because a
        // twenty-column screen is exactly the one that wraps this sentence --
        // and what the case is about is that the refusal was said, not where the
        // wrap fell.
        //
        // **Its own sentence, not the approval panel's.** A question asks for no
        // permission and refuses no change, so [`PANEL_TOO_SMALL`]'s wording
        // would tell the user two things that did not happen -- and it is the
        // wording rather than the row that a reader has to be able to trust.
        assert_eq!(
            joined(&fixture.released()),
            notice::Notice::cancelled("question", QUESTION_TOO_SMALL).text()
        );
        assert_ne!(
            QUESTION_TOO_SMALL, PANEL_TOO_SMALL,
            "the question borrowed the permission panel's refusal again"
        );
        assert_eq!(
            fixture.geometry.panel, 0,
            "the band kept rows for a question it refused"
        );
    }

    #[test]
    fn a_resize_that_shrinks_under_a_standing_question_keeps_the_selection_visible() {
        let mut fixture = shell(24, 80);
        let started = asking_question(&mut fixture, six_option_batch());
        for _ in 0..6 {
            fixture.route_bytes(b"\x1b[B");
        }
        assert!(matches!(fixture.resize(12, 60), Resize::Repaint(_)));
        fixture.settle_band(started);
        // The marked slot is its open field: the ordinal and the draft.
        assert!(
            fixture
                .band_rows()
                .iter()
                .any(|row| row.contains("    7) ")),
            "the marked choice survived the shrink: {:?}",
            fixture.band_rows()
        );
        assert!(
            fixture.control.try_recv().is_err(),
            "a resize is not an answer"
        );
    }

    #[test]
    fn a_question_dismisses_the_menu_and_leaves_the_approval_panel_alone() {
        let mut fixture = shell(24, 80);
        let started = turn_running(&mut fixture, b"ask me\r");
        // A menu really open, so the dismissal is a fact rather than a vacuous
        // `None`: the two share the band's slot and only one of them has the
        // focus, so a menu left behind a question is one whose keys the question
        // is swallowing.
        fixture.route_bytes(b"/mo");
        assert!(fixture.picker.is_some(), "the menu never opened");

        fixture.apply(UiEvent::Question(a_batch()));
        fixture.settle_band(started);

        assert!(fixture.picker.is_none() && fixture.panel.is_none());
        assert!(fixture
            .band_rows()
            .iter()
            .any(|row| row.contains("Which depth?")));
    }

    #[test]
    fn a_question_leaves_the_draft_and_the_caret_exactly_where_they_were() {
        // The composer is not what has the focus, and it is not what is being
        // answered: every keystroke of the batch -- ordinals, freeform text, the
        // Backspace inside it -- goes to the question, and the draft the user
        // was writing is still theirs when the band comes back.
        let mut fixture = shell(24, 80);
        let started = turn_running(&mut fixture, b"ask me\r");
        fixture.route_bytes(b"half a thought");
        fixture.route_bytes(&[0x1b, 0x5b, 0x44]); // Left, so the caret is not at the end
        let caret = fixture.cursor();

        fixture.apply(UiEvent::Question(a_batch()));
        fixture.settle_band(started);
        // A digit, then the freeform slot, then text and an editing key inside
        // it: everything that could reach a composer if the focus leaked.
        fixture.route_bytes(b"1");
        fixture.route_bytes(b"3");
        fixture.route_bytes("typed".as_bytes());
        fixture.route_bytes(&[0x7f]); // Backspace
        assert_eq!(
            fixture.editor.text(),
            "half a thought",
            "the question typed into the composer"
        );

        fixture.route_bytes(&[0x0d]); // Enter: the draft answers the second question
        assert!(matches!(
            fixture.controlled(),
            Some(TurnControl::QuestionAnswer { .. })
        ));
        assert_eq!(
            fixture.editor.text(),
            "half a thought",
            "the draft did not survive the batch"
        );
        assert_eq!(
            fixture.cursor(),
            caret,
            "the caret came back somewhere else in the draft"
        );
    }

    #[test]
    fn the_freeform_answer_is_the_text_that_was_typed_at_the_question() {
        // The other half of the same seam: the keystrokes really did build an
        // answer, so the case above is not passing because they went nowhere.
        let mut fixture = shell(24, 80);
        asking_question(&mut fixture, a_batch());
        fixture.route_bytes(b"3"); // the freeform slot of the first question
        fixture.route_bytes("네—ok".as_bytes());
        fixture.route_bytes(&[0x0d]); // Enter takes the draft
        fixture.route_bytes(b"1"); // and an ordinal ends the batch
        assert_eq!(
            fixture.controlled(),
            Some(TurnControl::QuestionAnswer {
                id: QuestionId(1),
                answers: vec!["네—ok".to_string(), "Yes".to_string()],
            })
        );
    }

    #[test]
    fn the_caret_sits_on_the_marked_choice_and_moves_into_the_draft_with_it() {
        // Where the caret is *is* what the terminal says the focus is, and at a
        // question it says two different things: on a choice, that a digit takes
        // one; in the draft, that a digit is a character.
        let mut fixture = shell(24, 80);
        asking_question(&mut fixture, a_batch());
        // Upstream's rows: no marker glyph, the description in its column.
        assert_eq!(
            fixture.marked(),
            "    1) Thorough                      reads every file"
        );
        fixture.route_bytes(&[0x1b, 0x5b, 0x42]); // Down
        assert_eq!(fixture.marked(), "    2) Quick");
        assert_eq!(fixture.controlled(), None, "moving the marker answered");

        fixture.route_bytes(b"\t"); // Tab is the *batch's* cycle, not the choices'
        assert!(fixture
            .band_rows()
            .iter()
            .any(|row| row.contains("Ship it?")));
        assert_eq!(fixture.controlled(), None, "Tab answered");

        fixture.route_bytes(b"\t"); // back to the first question, marker kept
        fixture.route_bytes(b"3"); // the freeform slot
        let (row, column) = fixture.cursor();
        // The slot's own row, over the blank row above the bottom rule, and
        // just past its `    3) `.
        assert_eq!(
            (row, column),
            (fixture.geometry.divider - 2, 7),
            "the caret is not in the slot's field"
        );
        fixture.route_bytes("ab".as_bytes());
        assert_eq!(
            fixture.cursor(),
            (row, column + 2),
            "the caret did not follow what was typed into the draft"
        );
    }

    #[test]
    fn the_turns_clock_stops_while_a_model_question_is_up_too() {
        // The frozen clock is about *a person being asked something*, not about
        // which kind of question it was: a turn that spent four minutes waiting
        // for someone to choose a depth did not spend four minutes thinking.
        let mut fixture = shell(24, 80);
        let started = turn_running(&mut fixture, b"ask me\r");
        fixture.settle_band(started + Duration::from_secs(2));
        let before = fixture.activity_row.clone().expect("a running turn");
        assert!(before.contains("2s"), "{before:?}");

        fixture.apply(UiEvent::Question(a_batch()));
        fixture.settle_band(started + Duration::from_secs(2));
        fixture.settle_band(started + Duration::from_secs(30));
        // Off the band while the question is up (`render_input.zig:705`); the
        // number it comes back with is the proof the clock stopped.
        assert_eq!(
            fixture.activity_row, None,
            "the turn's row was painted beside the question"
        );

        fixture.route_bytes(b"1");
        fixture.route_bytes(b"1");
        fixture.settle_band(started + Duration::from_secs(30));
        fixture.settle_band(started + Duration::from_secs(33));
        let after = fixture.activity_row.clone().expect("the turn goes on");
        assert!(
            after.contains("5s"),
            "the turn was charged for the time it spent waiting for a person: {after:?}"
        );
    }

    #[test]
    fn a_question_the_band_can_show_does_not_touch_the_other_plane() {
        // The batch is a band occupant and nothing else: it takes no screen, so
        // nothing has to be given back when it is answered. A question that
        // moved the owner would leave the session composing frames for a plane
        // the loop never entered.
        let mut fixture = shell(24, 80);
        asking_question(&mut fixture, a_batch());
        assert_eq!(fixture.screen_owner(), ScreenOwner::Primary);
        assert!(fixture.screen_rows().is_empty());
        assert!(
            fixture.geometry.panel > 0,
            "the band gave the question no rows"
        );
        assert_eq!(
            fixture.band_rows().len(),
            usize::from(fixture.geometry.band_rows()),
            "the band painted a different number of rows than it solved for"
        );
    }

    #[test]
    fn a_submission_the_runtime_will_not_take_keeps_the_draft_and_says_so() {
        // The ordering this is really about: the offer comes before the
        // composer is cleared, so a refusal cannot have already thrown the
        // draft away. The refusal is on the **hint row**, beside the text it is
        // about, and not in the document -- a document row scrolls away from
        // the draft it is explaining.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"first\r");
        let _first = shell.picks_up();
        shell.route_bytes(b"second\r");
        let _ = shell.document();

        shell.route_bytes(b"third");
        shell.route_bytes(&[0x0d]);

        assert_eq!(
            shell.editor.text(),
            "third",
            "a refused submission took the draft with it"
        );
        // In the notice's own colour rather than the row's: a refusal is about
        // the keystroke that just happened and the rest of the row is about the
        // state (`render.zig:34,76 system_notice_text_style`).
        // And closed again with the row's own colour rather than left for the
        // row's trailing reset to close, so the run this row opened ends where
        // the refusal does whether or not anything follows it.
        assert_eq!(
            shell.hint(),
            format!(
                "{}{QUEUE_REJECTED}{}",
                PALETTE.notice(),
                PALETTE.statusline()
            )
        );
        assert!(
            shell.document().is_empty(),
            "the refusal was written into the document as well"
        );
        assert_eq!(
            shell.picks_up(),
            TurnWork::Submit("second".to_string()),
            "the queued prompt is the one that was queued"
        );
        assert!(
            shell.sent.try_recv().is_err(),
            "the refused prompt reached the runtime anyway"
        );
    }

    #[test]
    fn a_ctrl_c_while_the_runtime_is_working_asks_it_to_stop_and_says_so() {
        // The cancellation goes out on the **control** channel, so it cannot
        // queue behind the deltas it is trying to stop -- and the document says
        // so at once, in upstream's line for a cancelled turn, so the user
        // watches the request land rather than inferring it from a stream that
        // may not stop for a while. Claim changed: the document line was the
        // line shell's interrupt notice; it is upstream's `■ Cancelled` line,
        // and the promise about the next press moved to the hint row.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"stream something\r");
        let _taken = shell.picks_up();
        let _ = shell.document();

        shell.route_bytes(&[0x03]);

        assert_eq!(
            shell.control.try_recv(),
            Ok(TurnControl::Cancel { through: 1 })
        );
        assert_eq!(shell.document(), cancelled());
        assert_eq!(
            shell.hint(),
            EXIT_ARMED,
            "the press that would leave was not announced"
        );
        assert!(!shell.leaving(), "one Ctrl-C ended the session");
    }

    #[test]
    fn the_cancelled_line_is_upstreams_marker_word_and_question_in_upstreams_greys() {
        // `fx-interrupt-40x120-cc-0.3.ansi`: `ESC[38;5;252m■ESC[39m
        // ESC[38;5;255mCancelledESC[39m · What can fx do differently?` -- the
        // marker in the warning grey, the word in the hint grey, the question
        // in the terminal's own colour. tmux re-serializes resets as `39`;
        // this crate writes `0`, the same pen.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"stream something\r");
        let _taken = shell.picks_up();
        let _ = shell.take_pending();

        shell.route_bytes(&[0x03]);

        let rows: Vec<String> = shell
            .take_pending()
            .into_iter()
            .flat_map(|append| append.rows)
            .collect();
        assert_eq!(
            rows,
            vec![
                "\u{1b}[38;5;252m\u{25a0}\u{1b}[0m \u{1b}[38;5;255mCancelled\u{1b}[0m \u{b7} What can xfx do differently?"
                    .to_string(),
                String::new(),
            ]
        );
        // And in the light palette, the same roles at upstream's light greys
        // (`theme.zig:122,129`).
        let light = Palette {
            mode: super::super::theme::Mode::Light,
            depth: super::super::theme::Depth::Ansi256,
        };
        assert_eq!(
            cancelled_line(light),
            "\u{1b}[38;5;238m\u{25a0}\u{1b}[0m \u{1b}[38;5;235mCancelled\u{1b}[0m \u{b7} What can xfx do differently?"
        );
    }

    #[test]
    fn an_interrupt_says_that_the_queue_goes_with_the_turn() {
        // One keystroke, two facts, and the second one is the one a user cannot
        // otherwise find out: the prompt they typed ahead is not going to run.
        // Saying nothing about it would make a dropped prompt indistinguishable
        // from one that quietly failed.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"first\r");
        let _running = shell.picks_up();
        shell.route_bytes(b"second\r");
        assert_eq!(shell.hint(), queued_hint(1));
        let _ = shell.document();

        shell.route_bytes(&[0x03]);

        assert_eq!(
            shell.control.try_recv(),
            Ok(TurnControl::Cancel { through: 2 }),
            "the interrupt did not reach back over both submissions"
        );
        // The cancelled line and the notice, sharing the blank row between
        // them.
        assert_eq!(
            shell.document(),
            [
                cancelled(),
                said(notice::Notice::cancelled("queue", QUEUE_DROPPED)),
            ]
            .concat()
        );
    }

    #[test]
    fn an_interrupt_with_nothing_waiting_does_not_say_a_queue_was_dropped() {
        // The other side of it, so the sentence above is not written every time:
        // a notice about a queue that was never there is noise the next real one
        // is read past.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"first\r");
        let _running = shell.picks_up();
        let _ = shell.document();

        shell.route_bytes(&[0x03]);

        assert_eq!(shell.document(), cancelled());
    }

    #[test]
    fn the_ctrl_c_that_stopped_a_turn_leaves_only_while_the_row_says_it_will() {
        // Claim changed (was `a_turn_that_ended_takes_the_interrupt_that_
        // stopped_it_with_it`). The intent is the same -- no Ctrl-C ends the
        // session by surprise -- and what keeps it now is the hint row: the
        // press that stopped the turn armed upstream's three-second exit and
        // the row says `press ctrl+c again to exit` for exactly that long,
        // whether or not the turn has concluded in between
        // (`fx-interrupt-40x120-cc-1.5`). Inside it a press leaves; after it a
        // press is a first press again.
        let now = Instant::now();
        let stopped = |shell: &mut Fixture| {
            shell.consume(
                b"first"
                    .iter()
                    .map(|b| Input::Text(char::from(*b)))
                    .collect(),
                now,
            );
            shell.consume(vec![Input::Action(Action::Submit)], now);
            let _running = shell.picks_up();
            shell.consume(vec![Input::Action(Action::Cancel)], now);
            assert_eq!(
                shell.control.try_recv(),
                Ok(TurnControl::Cancel { through: 1 })
            );
            shell.apply(UiEvent::TurnEnded { failure: None });
        };

        let mut inside = shell(24, 80);
        stopped(&mut inside);
        inside.settle_band(now + EXIT_WINDOW - Duration::from_millis(1));
        assert_eq!(
            inside.band_rows().last().map(|row| uncoloured(row)),
            Some(EXIT_ARMED.to_string()),
            "the row stopped announcing the exit before the window closed"
        );
        inside.consume(
            vec![Input::Action(Action::Cancel)],
            now + EXIT_WINDOW - Duration::from_millis(1),
        );
        assert!(inside.leaving(), "a press the row announced did not leave");

        let mut outside = shell(24, 80);
        stopped(&mut outside);
        outside.settle_band(now + EXIT_WINDOW);
        assert_ne!(
            outside.band_rows().last().map(|row| uncoloured(row)),
            Some(EXIT_ARMED.to_string()),
            "the row kept announcing an exit the next press would not make"
        );
        outside.consume(vec![Input::Action(Action::Cancel)], now + EXIT_WINDOW);
        assert!(
            !outside.leaving(),
            "the session left on a press the row had stopped announcing"
        );
    }

    /// A fixture with a turn the runtime has taken and started: `prompt`'s
    /// card on the screen, the activity row up, and the document taken.
    fn running(prompt: &str) -> Fixture {
        let mut shell = shell(24, 80);
        shell.route_bytes(prompt.as_bytes());
        shell.route_bytes(b"\r");
        let _taken = shell.picks_up();
        shell.apply(UiEvent::TurnStarted);
        shell.settle_band(Instant::now());
        let _ = shell.take_pending();
        shell
    }

    #[test]
    fn a_double_escape_over_a_running_turn_stops_it_and_the_row_warns_in_between() {
        // `fx-interrupt2-40x120-esc1`: `esc again to interrupt` flush with the
        // last column; `-esc2`: the cancelled line, and the row the session's
        // own again -- no exit armed by an Escape pair.
        let mut shell = running("stream lines");
        let now = Instant::now();

        shell.consume(vec![Input::Action(Action::Escape)], now);
        shell.settle_band(now);
        assert_eq!(
            shell.band_rows().last().map(|row| uncoloured(row)),
            Some(format!(
                "{IDLE_HINT}{}esc again to interrupt",
                " ".repeat(
                    80 - usize::from(super::super::wrap::width(IDLE_HINT))
                        - "esc again to interrupt".len()
                )
            )),
            "the second Escape's meaning was not announced"
        );
        assert_eq!(shell.controlled(), None, "one Escape stopped the turn");

        let second = now + super::super::gesture::INTERRUPT_WINDOW;
        shell.consume(vec![Input::Action(Action::Escape)], second);
        assert_eq!(shell.controlled(), Some(TurnControl::Cancel { through: 1 }));
        assert_eq!(shell.document(), cancelled());
        shell.settle_band(second);
        assert_eq!(
            shell.band_rows().last().map(|row| uncoloured(row)),
            Some(IDLE_HINT.to_string()),
            "an Escape pair armed something on its way out"
        );
        assert_eq!(shell.activity_row, None, "the row stayed up under the line");
    }

    #[test]
    fn the_right_hand_cue_is_painted_in_the_dim_grey_after_the_rows_run_is_closed() {
        // `fx-interrupt-40x120-esc1.ansi`: `…model-aESC[39m<spaces>ESC[38;5;245m
        // esc again to interrupt` -- the row's run closed, the cue in
        // `dim_style`. The same number as the row's grey in the dark palette,
        // so the light one is where the role shows (`theme.zig:123,128`: 241
        // for the row, 247 for the cue).
        let light = Palette {
            mode: super::super::theme::Mode::Light,
            depth: super::super::theme::Depth::Ansi256,
        };
        let mut shell = shell_with(24, 80, light, false);
        shell.escape_armed = true;
        let row = shell.band_rows().last().cloned().expect("a hint row");
        assert!(
            row.contains("\u{1b}[0m\u{1b}[38;5;247mesc again to clear"),
            "{row:?}"
        );
        assert!(row.starts_with("\u{1b}[38;5;241m"), "{row:?}");

        shell.escape_armed = false;
        shell.interrupt_armed = true;
        let row = shell.band_rows().last().cloned().expect("a hint row");
        assert!(
            row.contains("\u{1b}[0m\u{1b}[38;5;247mesc again to interrupt"),
            "{row:?}"
        );
    }

    #[test]
    fn the_interrupt_warning_gives_way_to_the_screen_rather_than_disappearing() {
        // `input_presentation.zig:510-525`: the widest of three cues that fits
        // beside the row, and the whole row when none does. The fixture's row
        // is 32 cells, so the three cues fit strictly beside it below 55, 50
        // and 40 columns.
        let mut shell = running("stream lines");
        let now = Instant::now();
        shell.consume(vec![Input::Action(Action::Escape)], now);
        assert_eq!(super::super::wrap::width(IDLE_HINT), 32);
        for (cols, expected) in [
            (55u16, "esc again to interrupt"),
            (54, "esc esc interrupt"),
            (50, "esc esc interrupt"),
            (49, "esc esc"),
            (40, "esc esc"),
            (39, "esc esc to interrupt"),
        ] {
            shell.resize(24, cols);
            shell.settle_band(now);
            let row = uncoloured(shell.band_rows().last().expect("a hint row"));
            assert!(
                row.ends_with(expected),
                "{cols} columns: {row:?} does not end with {expected:?}"
            );
            assert!(
                usize::from(super::super::wrap::width(&row)) <= usize::from(cols),
                "{cols} columns: the row overran the screen: {row:?}"
            );
        }
    }

    #[test]
    fn a_ctrl_c_over_a_running_turn_takes_the_draft_first_and_nothing_else() {
        // `fx-interrupt2-40x120-draft-cc`: the draft goes, the answer goes on,
        // the row is the session's; `-draft-cc2`: the next press stops the
        // turn and arms the exit.
        let mut shell = running("stream words");
        let now = Instant::now();
        shell.consume("draft text".chars().map(Input::Text).collect(), now);

        shell.consume(vec![Input::Action(Action::Cancel)], now);
        assert!(shell.editor.is_empty(), "the draft survived");
        assert_eq!(shell.controlled(), None, "the turn was stopped as well");
        shell.settle_band(now);
        assert_eq!(
            shell.band_rows().last().map(|row| uncoloured(row)),
            Some(IDLE_HINT.to_string()),
            "clearing a draft armed the exit"
        );
        assert!(shell.activity_row.is_some(), "the turn's row went");

        shell.consume(vec![Input::Action(Action::Cancel)], now);
        assert_eq!(shell.controlled(), Some(TurnControl::Cancel { through: 1 }));
        assert_eq!(shell.document(), cancelled());
        assert_eq!(shell.hint(), EXIT_ARMED);
        assert!(!shell.leaving());
    }

    #[test]
    fn an_idle_ctrl_c_says_the_next_one_leaves_and_the_row_comes_back_after() {
        // `fx-interrupt-40x120-idle-cc` and `-idle-cc-expired`, and the e2e
        // claim behind them (`tui-keybindings.test.ts:35-47`).
        let mut shell = shell(24, 80);
        let now = Instant::now();
        shell.consume(vec![Input::Action(Action::Cancel)], now);
        shell.settle_band(now);
        let row = shell.band_rows().last().cloned().expect("a hint row");
        assert_eq!(
            row,
            format!("{}{EXIT_ARMED}{}", PALETTE.statusline(), PALETTE.reset()),
            "the exit hint is upstream's statusline grey, whole row"
        );
        shell.settle_band(now + EXIT_WINDOW);
        assert_eq!(
            shell.band_rows().last().map(|row| uncoloured(row)),
            Some(IDLE_HINT.to_string())
        );
        assert!(!shell.leaving());
    }

    #[test]
    fn any_other_key_takes_the_exit_back() {
        // `disarmCtrlCExit` on every byte but `0x03`: a user who pressed
        // Ctrl-C and went on typing is not half way out of the session.
        let mut shell = shell(24, 80);
        let now = Instant::now();
        shell.consume(vec![Input::Action(Action::Cancel)], now);
        shell.consume(vec![Input::Text('a')], now);
        shell.settle_band(now);
        assert_eq!(
            shell.band_rows().last().map(|row| uncoloured(row)),
            Some(IDLE_HINT.to_string()),
            "a typed character left the exit armed"
        );
        shell.consume(vec![Input::Action(Action::Cancel)], now);
        assert!(!shell.leaving(), "the press after typing left");
        assert!(shell.editor.is_empty(), "and it did not clear the draft");
    }

    #[test]
    fn the_catalog_says_the_next_ctrl_c_leaves_in_place_of_its_keys() {
        // `composeHelpMenuHintRow` (`input_presentation.zig:696-703`) and
        // `receipts/c1/fx-help-40x120-ctrl-c`: the catalog stays up, and its
        // key help gives way to the warning in the statusline grey.
        let mut shell = shell(40, 120);
        shell.route_bytes(b"/help\r");
        assert!(shell.help_menu.is_some(), "the catalog did not open");
        let now = Instant::now();
        shell.consume(vec![Input::Action(Action::Cancel)], now);
        shell.settle_band(now);
        assert!(
            shell.help_menu.is_some(),
            "a first Ctrl-C closed the catalog"
        );
        assert_eq!(
            shell.band_rows().last().cloned(),
            Some(format!(
                "{}{EXIT_ARMED}{}",
                PALETTE.statusline(),
                PALETTE.reset()
            ))
        );
        shell.settle_band(now + EXIT_WINDOW);
        assert_eq!(
            shell.band_rows().last().cloned(),
            Some(format!(
                "{}{}{}",
                PALETTE.dim(),
                help_menu::key_help(120),
                PALETTE.reset()
            )),
            "the catalog's keys did not come back with the window"
        );
    }

    #[test]
    fn ctrl_d_is_not_an_exit_while_a_turn_is_running() {
        // `handleSemanticCtrlD`, `app_input_runtime.zig:1750`: ignored while
        // an answer streams -- and a turn the user has stopped is not one.
        let mut shell = running("stream please");
        let now = Instant::now();
        shell.consume(vec![Input::Action(Action::Eof)], now);
        assert!(!shell.leaving(), "Ctrl-D ended a session mid-answer");

        shell.consume(vec![Input::Action(Action::Cancel)], now);
        shell.consume(vec![Input::Action(Action::Eof)], now);
        assert!(
            shell.leaving(),
            "Ctrl-D did not leave after the turn was stopped"
        );
        assert_eq!(
            format!("{:?}", shell.exit_code()),
            format!("{:?}", ExitCode::SUCCESS),
            "a Ctrl-D exit is a finished session, not an interrupted one"
        );
    }

    #[test]
    fn what_a_cancelled_turn_was_still_saying_never_reaches_the_screen() {
        // Upstream paints nothing of an answer after the cancel
        // (`fx-interrupt-40x120-cc-0.3`, `fx-interrupt2-40x120-esc2`): not the
        // line it had not finished, not the text still being released, not the
        // deltas still crossing the channel -- and the conclusion that follows
        // adds no summary and no `✗ system:` for a failure the user asked for.
        let mut shell = running("stream please");
        shell.apply(UiEvent::Delta("a whole line\n".to_string()));
        shell.flush_paced();
        let shown = shell.document();
        assert!(
            shown.iter().any(|row| row.contains("a whole line")),
            "{shown:?}"
        );
        shell.apply(UiEvent::Delta("MARKER-HEAD word0 word1 ".to_string()));
        shell.settle_band(Instant::now());
        shell.route_bytes(&[0x03]);
        shell.apply(UiEvent::Delta("word2 MARKER-TAIL".to_string()));
        shell.apply(UiEvent::TurnEnded {
            failure: Some("xfx: the turn was cancelled".to_string()),
        });

        let after = shell.released();
        let text = after.join("\n");
        assert!(
            !text.contains("MARKER"),
            "the stopped answer was painted: {after:?}"
        );
        assert!(
            !text.contains("system"),
            "the cancel was reported as a failure: {after:?}"
        );
        assert_eq!(
            after.iter().filter(|row| row.contains("Cancelled")).count(),
            1,
            "{after:?}"
        );
        assert!(
            !after.iter().any(|row| row.trim_start().starts_with("0s")),
            "a summary was written under a cut-off answer: {after:?}"
        );
    }

    #[test]
    fn the_answer_after_a_cancelled_one_is_painted() {
        // The silence is the cancelled turn's and nothing else's: a prompt the
        // user sends next is answered on the screen, even when the runtime
        // never concluded the turn the cancel was aimed at.
        let mut shell = running("first");
        shell.route_bytes(&[0x03]);
        shell.route_bytes(b"second\r");
        let _second = shell.picks_up();
        shell.apply(UiEvent::TurnStarted);
        shell.apply(UiEvent::Delta("MARKER-SECOND\n".to_string()));
        let shown = shell.released();
        assert!(
            shown.iter().any(|row| row.contains("MARKER-SECOND")),
            "{shown:?}"
        );
    }

    #[test]
    fn a_retry_the_turn_waits_on_is_the_activity_row_counting_down() {
        // `fx-retry-40x120-r0.6`: the row, whole, in the warning grey, no
        // marker; `-r1.3`: the count gone once the attempt is under way;
        // `-r2.0`: the answer, and the row the turn's own again.
        let mut shell = running("try it");
        // The capture's width: at 80 columns the label is cut, as upstream's is.
        shell.resize(40, 120);
        let now = Instant::now();
        shell.apply(UiEvent::Retrying {
            cause: "Provider unavailable",
            diagnostic: "HTTP 500 \u{b7} server_error: upstream exploded".to_string(),
            delay: Duration::from_millis(250),
        });
        shell.settle_band(now);
        let label = "\u{26a0} Provider unavailable \u{b7} HTTP 500 \u{b7} server_error: upstream exploded \u{b7} retrying request";
        assert_eq!(
            shell.activity_row.as_deref(),
            Some(format!("{}{label} in 1s{}", PALETTE.warning(), PALETTE.reset()).as_str())
        );
        assert_eq!(PALETTE.warning(), "\u{1b}[38;5;252m");
        shell.settle_band(now + Duration::from_secs(1));
        assert_eq!(
            shell.activity_row.as_deref(),
            Some(format!("{}{label}{}", PALETTE.warning(), PALETTE.reset()).as_str())
        );

        shell.apply(UiEvent::Delta("Recovered answer.".to_string()));
        shell.settle_band(now + Duration::from_secs(1));
        let row = shell.activity_row.clone().expect("the turn's row");
        assert!(
            uncoloured(&row).contains("Generating"),
            "the retry outlived the answer that ended it: {row:?}"
        );
    }

    #[test]
    fn a_retry_row_is_cut_to_the_screen_with_its_colour_closed() {
        let mut shell = running("try it");
        shell.resize(24, 40);
        shell.apply(UiEvent::Retrying {
            cause: "Provider unavailable",
            diagnostic: "HTTP 500: unexpected request".to_string(),
            delay: Duration::from_secs(2),
        });
        shell.settle_band(Instant::now());
        let row = shell.activity_row.clone().expect("a row");
        assert!(row.ends_with(PALETTE.reset()), "{row:?}");
        assert_eq!(super::super::wrap::width(&row), 40, "{row:?}");
    }

    #[test]
    fn a_retry_goes_with_the_turn_and_never_outlives_a_cancel() {
        let now = Instant::now();
        let retrying = || UiEvent::Retrying {
            cause: "Rate limited",
            diagnostic: "HTTP 429".to_string(),
            delay: Duration::from_secs(5),
        };
        // The turn failed for good: the reason is in the document, the row is
        // gone with the turn.
        let mut ended = running("try it");
        ended.apply(retrying());
        ended.apply(UiEvent::TurnEnded {
            failure: Some("xfx: the Gateway returned HTTP 429".to_string()),
        });
        ended.settle_band(now);
        assert_eq!(ended.activity_row, None);

        // Stopped by the user: no retry row comes back under the line, even
        // from an event that was already crossing the channel.
        let mut stopped = running("try it");
        stopped.apply(retrying());
        stopped.route_bytes(&[0x03]);
        stopped.apply(retrying());
        stopped.settle_band(now);
        assert_eq!(stopped.activity_row, None);
        assert_eq!(stopped.document(), cancelled());
    }

    #[test]
    fn a_ctrl_c_at_a_models_question_leaves_only_the_questions_cancelled_line() {
        // `cancelQuestionPrompt` (`input_question_runtime.zig:59-80`) and
        // `receipts/f3/fx-question-…-q2-esc`: the question's `■ Cancelled` is
        // the turn's line; no second line and no exit armed follow it.
        let mut shell = running("ask me");
        shell.apply(UiEvent::Question(QuestionRequest {
            id: QuestionId(7),
            entries: vec![QuestionEntry {
                question: "Which database?".to_string(),
                options: vec![
                    QuestionOption {
                        label: "Postgres".to_string(),
                        description: None,
                    },
                    QuestionOption {
                        label: "SQLite".to_string(),
                        description: None,
                    },
                ],
            }],
        }));
        shell.settle_band(Instant::now());
        let _ = shell.released();

        shell.route_bytes(&[0x03]);

        assert!(
            matches!(shell.controlled(), Some(TurnControl::Cancel { .. })),
            "the turn was not stopped"
        );
        let written = shell.released();
        assert_eq!(
            written
                .iter()
                .filter(|row| row.contains("Cancelled"))
                .count(),
            1,
            "{written:#?}"
        );
        assert!(
            !written.iter().any(|row| row.contains("What can xfx")),
            "the turn's own line followed the question's: {written:#?}"
        );
        assert!(
            !shell.hint().contains(EXIT_ARMED),
            "a Ctrl-C at a question armed the exit"
        );
    }

    #[test]
    fn a_cancel_that_waits_for_a_running_call_survives_a_prompt_typed_meanwhile() {
        // The command holds the turn until it returns, so the turn's line
        // waits for the turn's end -- and a prompt typed while it waits is
        // queued behind it, not a reason to forget the line.
        let mut shell = running("run it");
        shell.apply(tool_start(
            "1",
            "terminal",
            serde_json::json!({"action": "exec", "command": "sleep 30"}),
        ));
        shell.route_bytes(&[0x03]);
        assert!(
            shell.activity_row.is_some(),
            "the row went while the command still runs"
        );
        shell.route_bytes(b"second\r");
        shell.apply(tool_ended(
            "1",
            "terminal",
            true,
            "",
            CallOutcome::Exited(0),
        ));
        shell.apply(UiEvent::Delta("MARKER-STOPPED\n".to_string()));
        shell.apply(UiEvent::TurnEnded {
            failure: Some("the turn was cancelled".to_string()),
        });
        shell.apply(UiEvent::TurnStarted);
        shell.apply(UiEvent::Delta("MARKER-SECOND\n".to_string()));
        shell.apply(UiEvent::TurnEnded { failure: None });
        let written = shell.released();
        let text = written.join("\n");
        assert!(!text.contains("MARKER-STOPPED"), "{written:#?}");
        assert!(!text.contains("system"), "{written:#?}");
        let line = text
            .find("\u{25a0} Cancelled \u{b7} What can xfx")
            .expect("the turn's line");
        let second = text.find("MARKER-SECOND").expect("the next answer");
        assert!(line < second, "{written:#?}");
    }

    #[test]
    fn a_prompt_the_cancel_dropped_before_it_started_does_not_silence_the_next() {
        // The runtime reads a cancellation before it takes the next item
        // (`super::worker`'s `turn_loop`, `biased`), so a prompt submitted
        // and not yet started can be dropped without ever saying it started.
        // Once no place is held, nothing about the cancelled work is coming.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"first\r");
        let _taken = shell.picks_up();
        shell.route_bytes(&[0x03]);
        assert!(matches!(
            shell.controlled(),
            Some(TurnControl::Cancel { .. })
        ));
        // Dropped at the top of the runtime's loop: its place goes back, and
        // no `TurnStarted` or conclusion is ever sent for it.
        shell.work.give_back();
        shell.settle_band(Instant::now());

        shell.route_bytes(b"second\r");
        shell.apply(UiEvent::TurnStarted);
        shell.apply(UiEvent::Delta("MARKER-SECOND\n".to_string()));
        let written = shell.released();
        assert!(
            written.iter().any(|row| row.contains("MARKER-SECOND")),
            "{written:#?}"
        );
    }

    #[test]
    fn a_cancel_that_landed_after_its_turn_ended_does_not_silence_the_next() {
        // The runtime gives its place back after the conclusion, so a press in
        // between is answered as a cancel no conclusion will follow.
        let mut shell = running("first");
        shell.apply(UiEvent::TurnEnded { failure: None });
        shell.route_bytes(&[0x03]);
        assert!(
            matches!(shell.controlled(), Some(TurnControl::Cancel { .. })),
            "the press was not answered from the running column"
        );
        shell.route_bytes(b"second\r");
        shell.apply(UiEvent::TurnStarted);
        shell.apply(UiEvent::Delta("MARKER-SECOND\n".to_string()));
        let written = shell.released();
        assert!(
            written.iter().any(|row| row.contains("MARKER-SECOND")),
            "{written:#?}"
        );
    }

    #[test]
    fn a_prompt_typed_while_an_answer_streams_waits_in_the_band_and_never_splits_its_line() {
        // `fx-steer-40x120-steer-tool`: the prompt typed while a turn runs is
        // the band's dim `┋ change tone` row, between the activity row and
        // the composer with a blank row either side, and its card is written
        // when it is delivered (`steer-done`), never in the middle of the
        // answer it was typed under. A card written at the keystroke would
        // finish the presenter (`Self::run_mark`) and cut the line it is
        // still holding in two.
        let mut shell = running("first");
        let now = Instant::now();
        shell.apply(UiEvent::Delta("half of a line ".to_string()));
        shell.settle_band(now + Duration::from_secs(5));

        shell.route_bytes(b"second\r");
        assert!(
            matches!(shell.picks_up(), TurnWork::Submit(prompt) if prompt == "second"),
            "the prompt was not queued"
        );
        shell.settle_band(now + Duration::from_secs(5));
        let band = uncoloured_rows(&shell.band_rows());
        let steer = band
            .iter()
            .position(|row| row == "\u{250b} second")
            .unwrap_or_else(|| panic!("no waiting row in the band: {band:#?}"));
        assert_eq!(band[steer - 1], "", "{band:#?}");
        assert_eq!(band[steer + 1], "", "{band:#?}");
        assert!(band[steer + 2].starts_with(RAIL), "{band:#?}");
        assert!(
            band[..steer - 1]
                .iter()
                .any(|row| row.contains("Generating")),
            "{band:#?}"
        );
        let painted = shell.band_rows()[steer].clone();
        assert_eq!(
            painted,
            format!("{}\u{250b} second{}", PALETTE.dim(), PALETTE.reset())
        );

        shell.apply(UiEvent::Delta("and the rest\n".to_string()));
        shell.apply(UiEvent::TurnEnded { failure: None });
        shell.apply(UiEvent::TurnStarted);
        shell.apply(UiEvent::Delta("MARKER-SECOND\n".to_string()));
        shell.apply(UiEvent::TurnEnded { failure: None });
        let written = shell.released();
        assert!(
            written
                .iter()
                .any(|row| row == "  half of a line and the rest"),
            "the held line was split: {written:#?}"
        );
        let card = written
            .iter()
            .position(|row| row.contains("second") && row.starts_with(RAIL))
            .unwrap_or_else(|| panic!("no card: {written:#?}"));
        let answer = written
            .iter()
            .position(|row| row.contains("and the rest"))
            .expect("the answer");
        let marker = written
            .iter()
            .position(|row| row.contains("MARKER-SECOND"))
            .expect("the second answer");
        assert!(answer < card && card < marker, "{written:#?}");
        shell.settle_band(now + Duration::from_secs(6));
        assert!(
            !uncoloured_rows(&shell.band_rows())
                .iter()
                .any(|row| row.starts_with('\u{250b}')),
            "the waiting row outlived its delivery"
        );
    }

    #[test]
    fn a_long_waiting_prompt_takes_two_rows_and_ends_in_an_ellipsis() {
        // `steering_message_layout` (`render_input.zig:518-577`): at most two
        // rows, broken at a space, the last cut one cell short for `…`.
        let mut shell = running("first");
        shell.resize(24, 40);
        let now = Instant::now();
        shell.route_bytes(
            b"one two three four five six seven eight nine ten eleven twelve thirteen fourteen\r",
        );
        shell.settle_band(now);
        let band = uncoloured_rows(&shell.band_rows());
        let rows: Vec<&String> = band
            .iter()
            .filter(|row| row.starts_with('\u{250b}'))
            .collect();
        assert_eq!(rows.len(), 2, "{band:#?}");
        assert_eq!(rows[0], "\u{250b} one two three four five six seven");
        assert!(rows[1].ends_with('\u{2026}'), "{band:#?}");
        assert!(
            rows.iter().all(|row| super::super::wrap::width(row) <= 40),
            "{band:#?}"
        );
    }

    #[test]
    fn a_cancel_drops_the_waiting_prompt_and_its_row() {
        let mut shell = running("first");
        shell.route_bytes(b"second\r");
        shell.settle_band(Instant::now());
        assert!(uncoloured_rows(&shell.band_rows())
            .iter()
            .any(|row| row == "\u{250b} second"));
        shell.route_bytes(&[0x03]);
        shell.settle_band(Instant::now());
        assert!(
            !uncoloured_rows(&shell.band_rows())
                .iter()
                .any(|row| row.starts_with('\u{250b}')),
            "the dropped prompt is still shown waiting"
        );
        assert!(
            !shell.released().iter().any(|row| row.contains("second")),
            "the dropped prompt's card was written"
        );
    }

    #[test]
    fn an_escape_at_an_empty_composer_does_not_warn_about_a_clear() {
        // `app_input_runtime.zig:3210-3213`: fx disarms; nothing is cleared,
        // so nothing is announced.
        let mut shell = shell(24, 80);
        shell.route_bytes(&[0x1b]);
        shell.settle_input(Instant::now() + Decoder::ESC_TIMEOUT);
        assert_eq!(shell.hint(), IDLE_HINT, "an empty composer armed the clear");
    }

    #[test]
    fn the_ctrl_c_that_stopped_one_turn_does_not_end_the_session_on_the_next() {
        // The chain must not outlive the turn it was about. Driven the way a
        // session really reaches it: stop a turn, let the turn end, ask another
        // question, stop that one too.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"first\r");
        let _first = shell.picks_up();
        shell.route_bytes(&[0x03]);
        assert_eq!(
            shell.control.try_recv(),
            Ok(TurnControl::Cancel { through: 1 })
        );

        shell.apply(UiEvent::TurnEnded { failure: None });

        shell.route_bytes(b"second\r");
        let _second = shell.picks_up();
        shell.route_bytes(&[0x03]);

        assert!(
            !shell.leaving(),
            "the first Ctrl-C of a new turn ended the session, because the \
             session still remembered being asked to stop the last one"
        );
        assert_eq!(
            shell.control.try_recv(),
            Ok(TurnControl::Cancel { through: 2 }),
            "the new turn was never asked to stop"
        );
    }

    #[test]
    fn a_second_ctrl_c_leaves_with_the_status_an_interrupted_process_leaves_with() {
        let mut shell = shell(24, 80);
        shell.route_bytes(b"stream something\r");
        let _taken = shell.picks_up();

        shell.route_bytes(&[0x03, 0x03]);

        assert!(shell.leaving(), "a second Ctrl-C did not end the session");
        assert_eq!(
            format!("{:?}", shell.exit_code()),
            format!("{:?}", ExitCode::from(130u8)),
            "an interrupted session exited like one that finished"
        );
    }

    #[test]
    fn a_ctrl_c_at_an_idle_prompt_throws_the_draft_away_rather_than_stopping_anything() {
        let mut shell = shell(24, 80);
        shell.route_bytes(b"half a thought");

        shell.route_bytes(&[0x03]);

        assert!(shell.editor.is_empty(), "the draft survived a Ctrl-C");
        assert!(
            shell.control.try_recv().is_err(),
            "a cancellation was sent with nothing running"
        );
        assert!(!shell.leaving());
    }

    #[test]
    fn ctrl_c_leaves_only_from_a_session_whose_exit_the_user_asked_for_twice() {
        // The idle chain is time-bounded, and this is where that matters: two
        // Ctrl-Cs a minute apart are two people clearing two drafts, not
        // somebody leaving. Driven through `route_bytes`'s own clock rather
        // than a sleep.
        // Both keystrokes on a clock the test holds. Reading `Instant::now()`
        // through `route_bytes` would put the second one inside the window by
        // however long the first call took, which is the difference this case
        // is about.
        let now = Instant::now();
        let mut outside = shell(24, 80);
        outside
            .shell
            .consume(vec![Input::Action(Action::Cancel)], now);
        outside
            .shell
            .consume(vec![Input::Action(Action::Cancel)], now + EXIT_WINDOW);
        assert!(
            !outside.leaving(),
            "two interrupts outside the window ended the session"
        );

        // And the other side of it, so this is not passing because nothing
        // leaves: inside the window, it does.
        let mut inside = shell(24, 80);
        inside
            .shell
            .consume(vec![Input::Action(Action::Cancel)], now);
        inside.shell.consume(
            vec![Input::Action(Action::Cancel)],
            now + EXIT_WINDOW - std::time::Duration::from_millis(1),
        );
        assert!(inside.leaving(), "two interrupts inside the window did not");
    }

    #[test]
    fn a_double_escape_clears_the_composer_and_the_band_warns_before_it_does() {
        let mut shell = shell(24, 80);
        shell.route_bytes(b"a draft worth keeping");

        shell.route_bytes(&[0x1b]);
        shell.settle_input(Instant::now() + Decoder::ESC_TIMEOUT);
        assert_eq!(
            shell.hint(),
            armed_hint(80),
            "the destructive half of the gesture was not announced"
        );
        assert_eq!(
            shell.editor.text(),
            "a draft worth keeping",
            "one Escape cleared the composer"
        );

        shell.route_bytes(&[0x1b]);
        shell.settle_input(Instant::now() + Decoder::ESC_TIMEOUT);
        assert!(shell.editor.is_empty(), "the second Escape did not clear");
    }

    #[test]
    fn two_alt_backspaces_delete_two_words_instead_of_clearing_the_draft() {
        // The hazard the decoder's one carve-out exists for, asserted from the
        // surface that would have paid for it: `ESC 0x7f` replayed would be two
        // Escapes inside the window, and a key that means "delete one word"
        // would throw the whole draft away.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"one two three");
        shell.route_bytes(&[0x1b, 0x7f, 0x1b, 0x7f]);
        assert_eq!(shell.editor.text(), "one ");
        assert_eq!(
            shell.hint(),
            IDLE_HINT,
            "the composer-clearing gesture was armed"
        );
    }

    // -----------------------------------------------------------------------
    // the canonical slash commands
    // -----------------------------------------------------------------------

    #[test]
    fn every_advertised_slash_command_is_answered_without_asking_the_model() {
        // The closed set, driven through the composer one name at a time. What
        // makes this the real claim rather than a handful of spot checks is
        // that it reads `interactive::SLASH_COMMANDS`: a name added there fails
        // here until the TUI answers it too.
        for name in crate::interactive::SLASH_COMMANDS {
            let mut shell = shell(24, 80);
            shell.route_bytes(name.as_bytes());
            shell.route_bytes(&[0x0d]);

            // `/provider`'s answer is its argument's prefix in the composer,
            // which is the one draft a command leaves behind on purpose.
            let left = if *name == "/provider" {
                provider_picker::PROVIDER_PREFIX
            } else {
                ""
            };
            assert_eq!(
                shell.editor.text(),
                left,
                "{name} left the composer holding it"
            );
            let document = shell.document();
            // **Not echoed** (`fx-basic-40x120-version.txt`): what a command
            // leaves is its own answer and nothing above it -- no card.
            assert!(
                !document.iter().any(|row| row.contains(RAIL)),
                "{name} was echoed into the document: {document:?}"
            );
            // `/quit` is the one whose answer is the session ending rather than
            // a row, `/help`, `/model`, `/provider` and a bare `/statusline`
            // the ones whose answer is in the band, and the reset's three names answer
            // with a fresh screen and say nothing (`fx-sessions-40x120-new.txt`);
            // every other one says something, as a notice.
            if *name == "/quit" {
                assert!(shell.leaving(), "/quit did not end the session");
            } else if *name == "/provider" {
                // Answered in the band too: the composer reseeded with the
                // argument's prefix and the provider column under it
                // (`fx-provider-40x120-open`).
                assert!(
                    shell.provider_picker.is_some(),
                    "/provider opened no picker"
                );
                assert!(
                    document.is_empty(),
                    "/provider wrote to the document: {document:?}"
                );
                assert!(!shell.leaving(), "{name} ended the session");
            } else if *name == "/model" {
                assert!(shell.model_menu.is_some(), "/model opened no menu");
                assert!(
                    document.is_empty(),
                    "/model wrote to the document: {document:?}"
                );
                assert!(!shell.leaving(), "{name} ended the session");
            } else if *name == "/help" {
                assert!(shell.help_menu.is_some(), "/help opened no catalog");
                assert!(
                    document.is_empty(),
                    "/help wrote to the document: {document:?}"
                );
                assert!(!shell.leaving(), "{name} ended the session");
            } else if *name == "/resume" {
                // The picker, in the band, waiting for the listing it asked
                // the runtime for (`fx-sessions-40x120-resume.txt`).
                assert!(shell.sessions.is_some(), "/resume opened no picker");
                assert!(
                    document.is_empty(),
                    "/resume wrote to the document: {document:?}"
                );
                assert_eq!(shell.picks_up(), TurnWork::Sessions);
            } else if *name == "/statusline" {
                assert!(
                    shell.statusline_menu.is_some(),
                    "/statusline opened no picker"
                );
                assert!(
                    document.is_empty(),
                    "/statusline wrote to the document: {document:?}"
                );
                assert!(!shell.leaving(), "{name} ended the session");
            } else if ["/clear", "/new", "/reset"].contains(name) {
                assert!(
                    document
                        .first()
                        .is_some_and(|row| row.contains("Run /help for commands")),
                    "{name} did not start a fresh screen: {document:?}"
                );
                assert!(shell.owes_handoff(), "{name} owed the screen nothing");
                assert!(!shell.leaving(), "{name} ended the session");
            } else {
                // Every other one a neutral answer: a notice, glyph first.
                assert!(
                    document
                        .first()
                        .is_some_and(|row| row.starts_with("* ") || row.starts_with("\u{2717} ")),
                    "{name} did not answer with a notice: {document:?}"
                );
                assert!(!shell.leaving(), "{name} ended the session");
            }
            assert!(
                !matches!(shell.sent.try_recv(), Ok(TurnWork::Submit(_))),
                "{name} was sent to the model as a prompt"
            );
        }
    }

    #[test]
    fn a_name_that_is_not_one_of_the_six_is_refused_rather_than_asked() {
        // Upstream's refusal (`app_commands.zig:2162-2169`), with no echo of
        // the line above it. A typo silently becoming a prompt is what this
        // costs tokens to prevent.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"/notacommand\r");

        let document = shell.document();
        assert_eq!(
            document,
            vec![
                "\u{2717} command: Unknown command. Try /help.".to_string(),
                String::new()
            ]
        );
        assert!(
            shell.sent.try_recv().is_err(),
            "a mistyped command was sent to the model"
        );
        assert!(
            shell.editor.is_empty(),
            "the refused line stayed in the composer, so the next one would be \
             typed onto the end of it"
        );
    }

    #[test]
    fn a_command_is_not_echoed_and_its_answer_is_upstreams_notice() {
        // `fx-basic-40x120-version.txt`: `/version` leaves `* version: 0.0.13`
        // and nothing above it -- no card, no blank row of the card's.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"/version\r");
        assert_eq!(
            shell.document(),
            said(notice::Notice::neutral("version", crate::VERSION))
        );
        assert!(
            !crate::VERSION.starts_with('v'),
            "upstream's body is the bare version"
        );
    }

    /// Turns the loop until a `/copy` that is running on its own thread has
    /// said how it went, and hands back what the document gained.
    fn copy_settled(shell: &mut Fixture) -> Vec<String> {
        let deadline = Instant::now() + std::time::Duration::from_secs(10);
        loop {
            shell.settle_band(Instant::now());
            let document = shell.document();
            if !document.is_empty() {
                return document;
            }
            assert!(Instant::now() < deadline, "the copy never said how it went");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    #[test]
    fn copy_with_no_reply_says_so_and_copies_nothing() {
        // `fx-basic-40x120-copy-empty.txt`: `* clipboard: No assistant reply
        // to copy.`, and no echo of `/copy` above it.
        let tool = crate::clipboard::fake::FakeTool::keeping();
        let mut shell = shell(24, 80);
        shell.copies = Copies::new(tool.clipboard());
        shell.route_bytes(b"/copy\r");
        assert_eq!(
            shell.document(),
            said(notice::Notice::neutral(
                "clipboard",
                "No assistant reply to copy."
            ))
        );
        assert_eq!(tool.copied(), None, "a copy ran with nothing to copy");
        assert!(shell.sent.try_recv().is_err(), "/copy reached the runtime");
    }

    #[test]
    fn copy_puts_the_raw_reply_on_the_clipboard_and_then_says_so() {
        // `fx-sessions-40x120-copy.txt` and its `od -c`: the markdown exactly
        // as the model wrote it, backticks kept, no line break added -- and a
        // tab the screen showed as a space (`bridge::inert`) is a tab here.
        let reply = "ALPHA answer line one.\n\nSecond paragraph with `code`.\tTAB";
        let tool = crate::clipboard::fake::FakeTool::keeping();
        let mut shell = shell(24, 80);
        shell.copies = Copies::new(tool.clipboard());
        shell.work.replied(reply);
        shell.route_bytes(b"/copy\r");
        assert_eq!(
            copy_settled(&mut shell),
            said(notice::Notice::neutral("clipboard", "Copied to clipboard."))
        );
        assert_eq!(
            tool.copied().expect("the clipboard tool was fed"),
            reply.as_bytes()
        );
        assert!(shell.sent.try_recv().is_err(), "/copy reached the runtime");
    }

    #[test]
    fn copy_that_the_clipboard_refused_is_an_error_notice() {
        // `fx-sessions-40x120-copy-fail.txt`: the fake `pbcopy` exited 1.
        let tool = crate::clipboard::fake::FakeTool::keeping();
        tool.fail();
        let mut shell = shell(24, 80);
        shell.copies = Copies::new(tool.clipboard());
        shell.work.replied("reply");
        shell.route_bytes(b"/copy\r");
        assert_eq!(
            copy_settled(&mut shell),
            said(notice::Notice::error(
                "clipboard",
                "Failed to copy to clipboard."
            ))
        );
    }

    #[test]
    fn copy_with_nowhere_to_copy_to_is_an_error_notice() {
        // A test's shell reaches no clipboard at all (`super::copy::NOWHERE`),
        // which is also what a machine without the tool looks like.
        let mut shell = shell(24, 80);
        shell.work.replied("reply");
        shell.route_bytes(b"/copy\r");
        assert_eq!(
            copy_settled(&mut shell),
            said(notice::Notice::error(
                "clipboard",
                "Failed to copy to clipboard."
            ))
        );
    }

    #[test]
    fn copy_takes_no_argument() {
        // `registry.zig:59-68`: `/copy` matches exactly, so `/copy now` is a
        // command nobody has -- and nothing is copied.
        let tool = crate::clipboard::fake::FakeTool::keeping();
        let mut shell = shell(24, 80);
        shell.copies = Copies::new(tool.clipboard());
        shell.work.replied("reply");
        shell.route_bytes(b"/copy now\r");
        assert_eq!(
            shell.document(),
            said(notice::Notice::error(
                "command",
                "Unknown command. Try /help."
            ))
        );
        shell.settle_band(Instant::now());
        assert_eq!(tool.copied(), None, "`/copy now` copied");
        assert!(
            shell.sent.try_recv().is_err(),
            "`/copy now` reached the runtime"
        );
    }

    #[test]
    fn copy_does_not_wait_behind_the_work_the_runtime_has_in_hand() {
        // Upstream answers `/copy` at once, mid-turn included, from the
        // history it already has. Here every place in the queue is taken, and
        // the copy still runs: it is not work, so it neither queues behind a
        // turn nor is refused for want of a place.
        let tool = crate::clipboard::fake::FakeTool::keeping();
        let mut shell = shell(24, 80);
        shell.copies = Copies::new(tool.clipboard());
        shell.route_bytes(b"first\r");
        shell.route_bytes(b"second\r");
        assert_eq!(shell.work.outstanding(), super::super::worker::WORK_LIMIT);
        let _ = shell.document();
        shell.work.replied("the answer before these two");
        shell.route_bytes(b"/copy\r");
        assert_eq!(
            copy_settled(&mut shell),
            said(notice::Notice::neutral("clipboard", "Copied to clipboard."))
        );
        assert_eq!(
            tool.copied().as_deref(),
            Some("the answer before these two".as_bytes())
        );
        assert_eq!(shell.picks_up(), TurnWork::Submit("first".to_string()));
        assert_eq!(shell.picks_up(), TurnWork::Submit("second".to_string()));
        assert!(shell.sent.try_recv().is_err(), "/copy reached the runtime");
    }

    // -----------------------------------------------------------------------
    // /sound
    // -----------------------------------------------------------------------

    /// A shell whose `/sound` starts `on` or not, and saves into `settings`.
    fn sounding(rows: u16, on: bool, settings: Option<std::path::PathBuf>) -> Fixture {
        let mut shell = shell(rows, 80);
        shell.sound = Sound::new(crate::config::Notifications::both(on), settings);
        shell
    }

    /// A profile of the case's own, and the settings file in it.
    fn profile() -> (tempfile::TempDir, std::path::PathBuf) {
        let home = tempfile::tempdir().expect("a home");
        let path = home.path().join(".xfx").join("settings.json");
        (home, path)
    }

    /// Waits for `path` to hold `expected`, or fails the case: the save is on
    /// a thread of its own.
    fn saved_as(path: &std::path::Path, expected: serde_json::Value) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let read = std::fs::read_to_string(path)
                .ok()
                .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());
            if read.as_ref() == Some(&expected) {
                return;
            }
            assert!(Instant::now() < deadline, "the file holds {read:?}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// The notice a save that failed settles into, waited for.
    fn save_settled(shell: &mut Fixture) -> Vec<String> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            shell.settle_band(Instant::now());
            let released = shell.released();
            if !released.is_empty() {
                return released;
            }
            assert!(Instant::now() < deadline, "the save never said how it went");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn sound_toggles_and_sets_and_answers_with_upstreams_notice() {
        // `fx-sound-40x120-{bare,on,off,bare2}.txt` from a session started
        // off: `* sound: on`, `on`, `off`, and a bare toggle from off is `on`
        // again -- every one a neutral notice, no echo, nothing saved said.
        let (_home, path) = profile();
        let mut shell = sounding(24, false, Some(path.clone()));
        for (line, level) in [
            ("/sound", "on"),
            ("/sound on", "on"),
            ("/sound off", "off"),
            ("/sound", "on"),
            ("/sound", "off"),
            ("/sound  on ", "on"),
        ] {
            shell.route_bytes(line.as_bytes());
            shell.route_bytes(b"\r");
            assert_eq!(
                shell.document(),
                said(notice::Notice::neutral("sound", level)),
                "{line:?}"
            );
            assert_eq!(
                shell.sound.notifications(),
                crate::config::Notifications::both(level == "on"),
                "{line:?}"
            );
        }
        assert!(shell.sent.try_recv().is_err(), "/sound reached the runtime");
        assert!(shell.editor.is_empty());
        // Both switches, in the file the next session reads.
        saved_as(
            &path,
            serde_json::json!({"notifications": {"turn_end": true, "attention_required": true}}),
        );
        // And a successful save says nothing more.
        shell.settle_band(Instant::now());
        assert!(shell.released().is_empty());
    }

    #[test]
    fn sound_with_a_word_that_is_not_a_level_is_the_usage_line_and_moves_nothing() {
        // `fx-sound-40x120-bogus.txt`: `✗ usage: /sound [on|off|max]`, no
        // topic -- here without the `max` this build does not have, which is
        // why `max` itself is one of the refusals.
        let (_home, path) = profile();
        let mut shell = sounding(24, true, Some(path.clone()));
        for line in ["/sound bogus", "/sound max", "/sound ON", "/sound on off"] {
            shell.route_bytes(line.as_bytes());
            shell.route_bytes(b"\r");
            assert_eq!(
                shell.document(),
                said(notice::Notice::error("", "usage: /sound [on|off]")),
                "{line:?}"
            );
            assert_eq!(
                shell.sound.notifications(),
                crate::config::Notifications::both(true),
                "{line:?} moved the switches"
            );
        }
        shell.settle_band(Instant::now());
        assert!(!path.exists(), "a refused level was saved");
    }

    #[test]
    fn a_save_that_failed_says_so_after_the_level() {
        // `reportUserSettingsFailure` (`session_commands.zig:110-133`): an
        // error under `sound`, saying the switch is in force anyway. Upstream
        // says it before the level because it saves before it answers; here
        // the save is on its own thread, so it is said when the thread says.
        let (home, path) = profile();
        std::fs::create_dir_all(path.parent().expect("a profile")).expect("profile");
        std::fs::write(&path, "{ not json").expect("seed");
        let mut shell = sounding(24, false, Some(path.clone()));
        shell.route_bytes(b"/sound\r");
        assert_eq!(
            shell.document(),
            said(notice::Notice::neutral("sound", "on"))
        );
        let failed = joined(&save_settled(&mut shell));
        assert!(
            failed.starts_with(
                "\u{2717} sound: active for this process but not saved to user settings ("
            ),
            "{failed}"
        );
        assert!(failed.contains("not valid JSON"), "{failed}");
        assert_eq!(std::fs::read(&path).expect("read"), b"{ not json");
        // The switch is in force all the same.
        assert_eq!(
            shell.sound.notifications(),
            crate::config::Notifications::both(true)
        );
        drop(home);
    }

    #[test]
    fn a_save_with_nowhere_to_go_is_said_before_the_level() {
        // The one failure known before the level is said, so it is said in
        // upstream's order. A shell a test built has no settings file.
        let mut shell = shell(24, 80);
        let before = shell.sound.notifications();
        shell.route_bytes(b"/sound\r");
        let level = if before.turn_end { "off" } else { "on" };
        let said_rows = shell.document();
        assert_eq!(
            joined(&said_rows),
            format!(
                "\u{2717} sound: active for this process but not saved to user settings \
                 (no home directory is set) * sound: {level}"
            )
        );
    }

    /// One whole turn: started, answered, ended, and presented to the end.
    fn a_whole_turn(shell: &mut Fixture, failure: Option<&str>) {
        let started = turn_running(shell, b"ask something\r");
        shell.apply(UiEvent::Delta("an answer".to_string()));
        shell.apply(UiEvent::TurnEnded {
            failure: failure.map(str::to_string),
        });
        let mut now = started;
        while shell.pacer.pending() > 0 || !shell.marks.is_empty() {
            now += Duration::from_millis(TICK_MILLIS);
            shell.settle_band(now);
        }
        shell.settle_band(now);
    }

    #[test]
    fn a_turn_that_ends_rings_once_its_answer_is_on_the_screen() {
        // `postTurnEndHandler` and the presentation queue
        // (`hooks/notifications.zig:74-118`, `:224-240`): a bell for a turn
        // that answered and for one that failed, and not before the reader has
        // been given the end of the answer.
        for failure in [None, Some("the provider said no")] {
            let mut shell = sounding(24, true, None);
            let started = turn_running(&mut shell, b"ask something\r");
            shell.apply(UiEvent::Delta(
                "a long enough answer to be paced".to_string(),
            ));
            shell.apply(UiEvent::TurnEnded {
                failure: failure.map(str::to_string),
            });
            if failure.is_none() {
                assert!(shell.pacer.pending() > 0, "nothing was left to present");
                shell.settle_band(started);
                assert_eq!(shell.take_bells(), 0, "rang ahead of the answer");
            }
            let mut now = started;
            while shell.pacer.pending() > 0 || !shell.marks.is_empty() {
                now += Duration::from_millis(TICK_MILLIS);
                shell.settle_band(now);
            }
            shell.settle_band(now);
            assert_eq!(shell.take_bells(), 1, "{failure:?}");
            assert_eq!(shell.take_bells(), 0, "{failure:?} rang twice");
        }
    }

    #[test]
    fn a_turn_ends_in_silence_when_sound_is_off_or_the_user_stopped_it() {
        let mut quiet = sounding(24, false, None);
        a_whole_turn(&mut quiet, None);
        assert_eq!(quiet.take_bells(), 0, "sound off rang");

        // Ctrl-C stops the turn: its end is the user's, as upstream's
        // `.interrupted => return` has it -- and the next turn rings again.
        let mut shell = sounding(24, true, None);
        let started = turn_running(&mut shell, b"ask something\r");
        shell.route_bytes(&[0x03]);
        shell.apply(UiEvent::TurnEnded {
            failure: Some("the turn was cancelled".to_string()),
        });
        shell.settle_band(started + Duration::from_secs(5));
        assert_eq!(shell.take_bells(), 0, "an interrupted turn rang");
        a_whole_turn(&mut shell, None);
        assert_eq!(shell.take_bells(), 1, "the interrupt outlived its turn");
    }

    #[test]
    fn work_that_ran_no_turn_rings_nothing() {
        // `/model`, `/setup` and a catalog load end with the same event and
        // ran no turn: no summary, and no bell.
        let mut shell = sounding(24, true, None);
        shell.apply(UiEvent::TurnEnded { failure: None });
        shell.settle_band(Instant::now());
        assert_eq!(shell.take_bells(), 0);
    }

    #[test]
    fn a_question_that_opens_rings_at_once() {
        // `dispatchAttentionRequired` for a permission prompt and for the
        // model's questions (`app_worker_runtime.zig:766-767`, `:1089-1097`),
        // and with no wait for the pacer: the turn is stopped on the user.
        let mut shell = sounding(24, true, None);
        turn_running(&mut shell, b"edit the notes\r");
        shell.apply(UiEvent::Approval(question(asked())));
        assert_eq!(shell.take_bells(), 1, "the permission prompt");

        let mut fixture = sounding(24, true, None);
        turn_running(&mut fixture, b"ask me\r");
        fixture.apply(UiEvent::Question(a_batch()));
        assert_eq!(fixture.take_bells(), 1, "the model's question");

        // On the other plane too: a change too large for the band.
        let mut wide = sounding(40, true, None);
        turn_running(&mut wide, b"edit the notes\r");
        wide.apply(UiEvent::Approval(question(asked_about_a_large_change())));
        assert_eq!(wide.screen_owner(), ScreenOwner::Alternate);
        assert_eq!(wide.take_bells(), 1, "the review on the other plane");
    }

    #[test]
    fn a_question_nobody_is_shown_or_sound_off_rings_nothing() {
        // A screen too small refuses the question on the user's behalf; there
        // is nobody to call.
        let mut small = sounding(10, true, None);
        turn_running(&mut small, b"edit the notes\r");
        small.apply(UiEvent::Approval(question(asked())));
        assert_eq!(small.geometry.panel, 0);
        assert_eq!(small.take_bells(), 0);

        let mut quiet = sounding(24, false, None);
        turn_running(&mut quiet, b"edit the notes\r");
        quiet.apply(UiEvent::Approval(question(asked())));
        assert_eq!(quiet.take_bells(), 0);
    }

    #[test]
    fn sound_moves_what_the_next_turn_does() {
        // The switch is in force from the keystroke: the turn after `/sound`
        // rings, and the one after `/sound off` does not.
        let mut shell = sounding(24, false, None);
        shell.route_bytes(b"/sound\r");
        a_whole_turn(&mut shell, None);
        assert_eq!(shell.take_bells(), 1);
        shell.route_bytes(b"/sound off\r");
        a_whole_turn(&mut shell, None);
        assert_eq!(shell.take_bells(), 0);
    }

    #[test]
    fn a_command_that_takes_nothing_runs_only_when_nothing_follows_it() {
        // `fx-basic-40x120-version-trailing-space.txt`, in its order: the two
        // with an argument are unknown commands, the one with a trailing
        // space runs -- and two notices in a row share one blank row.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"/help extra\r");
        shell.route_bytes(b"/version now\r");
        shell.route_bytes(b"/version \r");
        let unknown = notice::Notice::error("command", "Unknown command. Try /help.");
        assert_eq!(
            shell.document(),
            [
                said(unknown.clone()),
                said(unknown),
                said(notice::Notice::neutral("version", crate::VERSION)),
            ]
            .concat()
        );
        assert!(
            shell.sent.try_recv().is_err(),
            "a refused command reached the runtime"
        );
        assert!(
            shell.editor.is_empty(),
            "the refused line stayed in the composer"
        );
    }

    /// A snapshot every field of which this file spells, so the case that
    /// reads it back can say the whole notice literally.
    fn a_status(history_turns: u64, grants: u64) -> crate::output::StatusSnapshot {
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let mut snapshot = crate::output::StatusSnapshot::new(
            &config(home.path(), workspace.path()),
            crate::build_info(),
            crate::output::SessionFacts::default(),
        );
        snapshot.model = "acme/model-9".to_string();
        snapshot.provider = "gateway".to_string();
        snapshot.provider_url = None;
        snapshot.provider_rejected = None;
        snapshot.build_channel = "debug".to_string();
        snapshot.build_revision = Some("0123456789ab".to_string());
        snapshot.auth = "AI_GATEWAY_API_KEY".to_string();
        snapshot.auth_refreshable = false;
        snapshot.auth_help = None;
        snapshot.permission_mode = "auto".to_string();
        snapshot.workspace = "/w/work".to_string();
        snapshot.history_turns = history_turns;
        snapshot.session_permission_grants = grants;
        snapshot.agent_step_limit = 0;
        snapshot
    }

    // -----------------------------------------------------------------------
    // `/permissions` and Shift-Tab
    // -----------------------------------------------------------------------

    /// A 40x120 shell in `mode`, whose person has or has not been shown the
    /// full-access warning, and the runtime end of the channel a change to the
    /// session's authority goes down.
    fn permissions_shell(
        mode: PermissionMode,
        acknowledged: bool,
    ) -> (Fixture, UnboundedReceiver<Change>) {
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let mut config = config(home.path(), workspace.path());
        config.permission_mode = mode;
        config.yolo_acknowledged = acknowledged;
        let (work, sent, control, changes) = WorkHandle::detached_with_changes();
        let fixture = Fixture {
            shell: Shell::new(
                &config,
                crate::tui::layout::solve(40, 120, 1).expect("a band"),
                PALETTE,
                false,
                work,
            )
            .anchored(),
            sent,
            control,
        };
        (fixture, changes)
    }

    /// Every change the shell has handed the runtime so far, in order.
    fn changes_sent(changes: &mut UnboundedReceiver<Change>) -> Vec<Change> {
        std::iter::from_fn(|| changes.try_recv().ok()).collect()
    }

    #[test]
    fn a_bare_permissions_is_upstreams_two_notices_and_changes_nothing() {
        // `cap/fx-permissions-40x120-bare.txt`, less the two usage rows that
        // name `remember` and `revoke`, which xfx does not have.
        let (mut shell, mut changes) = permissions_shell(PermissionMode::Auto, true);
        shell.route_bytes(b"/permissions\r");
        assert_eq!(
            shell.document(),
            vec![
                "* permissions: mode=auto".to_string(),
                "  configured rules: (none)".to_string(),
                "  session grants: (none)".to_string(),
                "  usage: /permissions [ask|auto|full-access|reset]".to_string(),
                String::new(),
                "* permissions: saved-session permission rules: none".to_string(),
                String::new(),
            ]
        );
        assert!(changes_sent(&mut changes).is_empty());
        assert!(
            shell.sent.try_recv().is_err(),
            "it was handed to the runtime"
        );
        assert_eq!(shell.work.outstanding(), 0, "it took a place in the queue");
    }

    #[test]
    fn a_bare_permissions_lists_the_grants_the_runtime_published() {
        let (mut shell, _changes) = permissions_shell(PermissionMode::Ask, true);
        shell.work.publish_grants(vec![
            " - write_file -> notes.md".to_string(),
            " - terminal -> ls in /w".to_string(),
        ]);
        shell.route_bytes(b"/permissions\r");
        let document = shell.document();
        assert_eq!(
            document[..6],
            [
                "* permissions: mode=ask".to_string(),
                "  configured rules: (none)".to_string(),
                "  session grants:".to_string(),
                "   - write_file -> notes.md".to_string(),
                "   - terminal -> ls in /w".to_string(),
                "  usage: /permissions [ask|auto|full-access|reset]".to_string(),
            ],
            "{document:?}"
        );
    }

    #[test]
    fn each_mode_word_reaches_the_runtime_and_is_said_as_upstream_says_it() {
        // `cap/fx-permissions-40x120-{auto,full,ask}.txt`, and every spelling
        // of the third mode upstream reads (`types.zig:2396-2403`).
        let (mut shell, mut changes) = permissions_shell(PermissionMode::Ask, true);
        for (typed, mode, said, row) in [
            (
                "auto",
                PermissionMode::Auto,
                "* permissions: mode set to auto",
                "auto · ",
            ),
            (
                "full-access",
                PermissionMode::Yolo,
                "! permissions: mode set to full access",
                "full access · ",
            ),
            (
                "ask",
                PermissionMode::Ask,
                "* permissions: mode set to ask",
                "ask · ",
            ),
            (
                "yolo",
                PermissionMode::Yolo,
                "! permissions: mode set to full access",
                "full access · ",
            ),
            (
                "Full Access",
                PermissionMode::Yolo,
                "! permissions: mode set to full access",
                "full access · ",
            ),
            (
                "AUTO",
                PermissionMode::Auto,
                "* permissions: mode set to auto",
                "auto · ",
            ),
        ] {
            shell.route_bytes(format!("/permissions {typed}\r").as_bytes());
            assert_eq!(
                shell.document(),
                vec![said.to_string(), String::new()],
                "{typed}"
            );
            assert_eq!(
                changes_sent(&mut changes),
                vec![Change::Mode(mode)],
                "{typed}"
            );
            assert!(shell.hint().contains(row), "{typed}: {:?}", shell.hint());
            // Nothing went down the work channel: a mode change is not queued
            // work and holds no place a prompt could need.
            assert!(shell.sent.try_recv().is_err(), "{typed}");
            assert_eq!(shell.work.outstanding(), 0, "{typed}");
        }
    }

    #[test]
    fn the_full_access_notice_is_upstreams_warning_tone() {
        // `cap/fx-permissions-40x120-full.ansi`, row 14:
        // `ESC[38;5;252m! permissions:ESC[38;5;250m mode set to full access`.
        let (mut shell, _changes) = permissions_shell(PermissionMode::Auto, true);
        shell.route_bytes(b"/permissions full-access\r");
        // The transcript closes each run it writes; what reaches the screen
        // is the label grey over the glyph and the topic, then the body's.
        let painted: Vec<String> = shell
            .take_pending()
            .into_iter()
            .flat_map(|append| append.rows)
            .map(|row| row.replace(PALETTE.reset(), ""))
            .collect();
        assert!(
            painted.iter().any(|row| row
                == &format!(
                    "{label}! {label}permissions:{body} mode set to full access",
                    label = PALETTE.notice_label(),
                    body = PALETTE.notice()
                )),
            "{painted:?}"
        );
    }

    #[test]
    fn reset_is_ask_and_says_the_grants_went() {
        // `cap/fx-permissions-40x120-reset.txt`.
        let (mut shell, mut changes) = permissions_shell(PermissionMode::Yolo, true);
        shell.route_bytes(b"/permissions reset\r");
        assert_eq!(
            shell.document(),
            vec![
                "* permissions: permissions reset to ask, session grants cleared".to_string(),
                String::new(),
            ]
        );
        assert_eq!(changes_sent(&mut changes), vec![Change::Reset]);
        assert!(shell.hint().contains("ask · "), "{:?}", shell.hint());
        shell.route_bytes(b"/permissions RESET\r");
        assert_eq!(changes_sent(&mut changes), vec![Change::Reset]);
    }

    #[test]
    fn an_argument_nobody_understands_is_the_usage_line_and_changes_nothing() {
        // `cap/fx-permissions-40x120-bogus.txt`: `✗ usage: …`, no topic.
        // `remember` and `revoke` are upstream's and answered the same way.
        let (mut shell, mut changes) = permissions_shell(PermissionMode::Auto, true);
        for typed in [
            "bogus",
            "remember allow write_file {}",
            "revoke 1",
            "reset now",
        ] {
            shell.route_bytes(format!("/permissions {typed}\r").as_bytes());
            assert_eq!(
                shell.document(),
                vec![
                    "\u{2717} usage: /permissions [ask|auto|full-access|reset]".to_string(),
                    String::new(),
                ],
                "{typed}"
            );
        }
        assert!(changes_sent(&mut changes).is_empty());
        assert!(shell.hint().contains("auto · "), "{:?}", shell.hint());
    }

    #[test]
    fn shift_tab_cycles_ask_auto_full_access_and_says_it_only_on_the_row() {
        // `toggleMode` (`app_permission_runtime.zig:135-142`): no notice, the
        // hint row at once, and the same change a typed mode makes.
        let (mut shell, mut changes) = permissions_shell(PermissionMode::Ask, true);
        let mut seen = Vec::new();
        for _ in 0..3 {
            shell.route_bytes(b"\x1b[Z");
            seen.push(shell.hint());
        }
        assert!(seen[0].contains("auto · "), "{seen:?}");
        assert!(seen[1].contains("full access · "), "{seen:?}");
        assert!(seen[2].contains("ask · "), "{seen:?}");
        assert_eq!(
            changes_sent(&mut changes),
            vec![
                Change::Mode(PermissionMode::Auto),
                Change::Mode(PermissionMode::Yolo),
                Change::Mode(PermissionMode::Ask),
            ]
        );
        assert!(
            shell.document().is_empty(),
            "a key said something in the document"
        );
        // The kitty spelling is the same key.
        shell.route_bytes(b"\x1b[9;2u");
        assert_eq!(
            changes_sent(&mut changes),
            vec![Change::Mode(PermissionMode::Auto)]
        );
    }

    #[test]
    fn shift_tab_belongs_to_the_catalog_while_it_is_up_and_to_the_draft_never() {
        // The catalog binds it (a category back), as upstream's does first
        // (`app_input_runtime.zig:1176`); a draft is left as it was.
        let (mut shell, mut changes) = permissions_shell(PermissionMode::Ask, true);
        shell.route_bytes(b"/help\r");
        shell.route_bytes(b"\x1b[Z");
        assert!(
            changes_sent(&mut changes).is_empty(),
            "the catalog let it through"
        );
        assert_eq!(shell.mode, PermissionMode::Ask);

        let (mut shell, mut changes) = permissions_shell(PermissionMode::Ask, true);
        shell.route_bytes(b"half a thought");
        shell.route_bytes(b"\x1b[Z");
        assert_eq!(
            changes_sent(&mut changes),
            vec![Change::Mode(PermissionMode::Auto)]
        );
        assert_eq!(shell.editor.text(), "half a thought", "the draft moved");
    }

    #[test]
    fn a_mode_the_runtime_cannot_take_leaves_the_row_alone_and_says_why() {
        let (mut shell, changes) = permissions_shell(PermissionMode::Ask, true);
        drop(changes);
        shell.route_bytes(b"/permissions auto\r");
        shell.route_bytes(b"\x1b[Z");
        assert!(shell.hint().contains("ask · "), "{:?}", shell.hint());
        let document = shell.document();
        assert!(
            !document.iter().any(|row| row.contains("mode set to")),
            "a change nobody applied was announced: {document:?}"
        );
        assert!(
            document.iter().any(|row| row.contains(GONE_NOTICE)),
            "{document:?}"
        );
    }

    #[test]
    fn the_mode_is_lifted_out_of_the_row_as_upstream_lifts_it() {
        // `cap/fx-permissions-40x120-{full,ask}.ansi`: `full access` and `auto`
        // in 252 then back to the row's 245; `ask` in the row's own.
        let (mut shell, _changes) = permissions_shell(PermissionMode::Ask, true);
        let raw = |shell: &mut Fixture| {
            shell.settle_band(Instant::now());
            shell.band_rows().last().cloned().expect("a hint row")
        };
        let row = raw(&mut shell);
        assert!(row.starts_with(PALETTE.statusline()), "{row:?}");
        assert!(row.contains(" · ask · "), "{row:?}");
        assert!(!row.contains(PALETTE.activity()), "{row:?}");
        shell.route_bytes(b"/permissions full-access\r");
        let row = raw(&mut shell);
        assert!(
            row.contains(&format!(
                " · {}full access{} · ",
                PALETTE.activity(),
                PALETTE.statusline()
            )),
            "{row:?}"
        );
    }

    #[test]
    fn full_access_shows_its_warning_once_and_for_four_seconds_on_the_screen() {
        // `cap/fx-permissions-40x120-full.ansi`, row 18: flush with the last
        // column, in upstream's `red_style` grey. Shown the first time this
        // person is in full access, recorded the moment it is shown
        // (`noteFrameCommitted`), and gone after 4 s of being seen.
        let (mut shell, mut changes) = permissions_shell(PermissionMode::Auto, false);
        assert!(!shell.hint().contains(permissions::WARNING));
        shell.route_bytes(b"/permissions full-access\r");
        let started = Instant::now();
        shell.settle_band(started);
        let row = shell.band_rows().last().cloned().expect("a hint row");
        assert!(
            row.ends_with(&format!(
                "{}{}{}",
                PALETTE.danger(),
                permissions::WARNING,
                PALETTE.reset()
            )),
            "{row:?}"
        );
        assert_eq!(
            super::super::wrap::width(&unpainted(
                &row.replace(PALETTE.statusline(), "")
                    .replace(PALETTE.activity(), "")
            )),
            120
        );
        assert_eq!(
            changes_sent(&mut changes),
            vec![Change::Mode(PermissionMode::Yolo), Change::Acknowledge],
            "the warning was shown and not recorded as shown"
        );
        shell.settle_band(started + Duration::from_millis(3_999));
        assert!(shell.warning_shown().is_some(), "gone before its time");
        shell.settle_band(started + Duration::from_millis(4_000));
        assert!(shell.warning_shown().is_none(), "still up after 4 s");
        assert!(!shell.hint().contains("Full access"));
        // Once per person: back out and in again, and it is not shown again,
        // and nothing more is recorded.
        shell.route_bytes(b"\x1b[Z");
        shell.route_bytes(b"\x1b[Z\x1b[Z");
        assert!(shell.hint().contains("full access · "));
        assert!(!shell.hint().contains("Full access"));
        assert_eq!(
            changes_sent(&mut changes),
            vec![
                Change::Mode(PermissionMode::Ask),
                Change::Mode(PermissionMode::Auto),
                Change::Mode(PermissionMode::Yolo),
            ]
        );
    }

    #[test]
    fn a_person_already_warned_is_not_warned_again_and_one_never_warned_is_at_launch() {
        let (mut warned, mut changes) = permissions_shell(PermissionMode::Ask, true);
        warned.route_bytes(b"/permissions full-access\r");
        assert!(!warned.hint().contains("Full access"));
        assert_eq!(
            changes_sent(&mut changes),
            vec![Change::Mode(PermissionMode::Yolo)]
        );

        // A session that starts in full access owes it too
        // (`initializeYoloWarning`).
        let (mut launched, mut changes) = permissions_shell(PermissionMode::Yolo, false);
        assert!(
            launched.hint().ends_with(permissions::WARNING),
            "{:?}",
            launched.hint()
        );
        assert_eq!(changes_sent(&mut changes), vec![Change::Acknowledge]);
        // And leaving full access takes it down at once.
        launched.route_bytes(b"\x1b[Z");
        assert!(!launched.hint().contains("Full access"));
    }

    #[test]
    fn the_warning_waits_behind_whatever_owns_the_row_without_spending_its_time() {
        // Upstream pauses it behind a menu (`yolo-permission-mode.test.ts:322-
        // 336`): the time spent is time it was seen.
        let (mut shell, _changes) = permissions_shell(PermissionMode::Yolo, false);
        let at = |millis: u64| Instant::now() + Duration::from_millis(millis);
        let started = at(0);
        let after = |millis: u64| started + Duration::from_millis(millis);
        shell.settle_band(started);
        assert!(shell.warning_shown().is_some());
        // The loop settles after every read, so the catalog taking the row is
        // seen a tick after the key, not ten seconds later.
        shell.route_bytes(b"/help\r");
        assert!(
            shell.warning_shown().is_none(),
            "the catalog's row carried it"
        );
        shell.settle_band(after(100));
        shell.settle_band(after(10_000));
        // Taken down the way the catalog is left, then seen again.
        shell.help_menu = None;
        shell.settle_band(after(10_000));
        assert!(
            shell.warning_shown().is_some(),
            "time behind the catalog was spent"
        );
        // 100 ms were spent before the catalog: 3.9 s are left, not 4.
        shell.settle_band(after(13_800));
        assert!(shell.warning_shown().is_some());
        shell.settle_band(after(13_900));
        assert!(shell.warning_shown().is_none(), "it outlived its 4 s");
        // The full transcript owns the other buffer and the band is not
        // painted under it, so it pauses the clock too.
        let (mut under, _changes) = permissions_shell(PermissionMode::Yolo, false);
        under.settle_band(started);
        assert!(under.warning_shown().is_some());
        under.route_bytes(b"\x0f");
        assert!(
            under.full.is_open(),
            "Ctrl-O did not open the full transcript"
        );
        assert!(
            under.warning_shown().is_none(),
            "seen under the full transcript"
        );
        // Seen hidden on the loop's next tick, as the catalog was above.
        under.settle_band(after(100));
        under.settle_band(after(20_000));
        under.close_full_transcript();
        under.settle_band(after(20_000));
        assert!(
            under.warning_shown().is_some(),
            "time under the full transcript was spent"
        );
        // And the narrow form on a row too narrow for the sentence.
        let (mut narrow, _changes) = permissions_shell(PermissionMode::Yolo, false);
        narrow.geometry = crate::tui::layout::solve(24, 40, 1).expect("a band");
        assert_eq!(narrow.warning_shown(), Some(permissions::WARNING_COMPACT));
    }

    #[test]
    fn status_is_one_neutral_notice_of_what_the_runtime_last_published() {
        // `fx-basic-40x120-status.txt`: `* status: model=…` and one `k=v` row
        // per fact under it, two columns in, no `[status]` prefix. xfx keeps
        // its own `provider` and `sandbox` rows and has no `update_channel` or
        // `ultrafast_requested` to print.
        let mut shell = shell(40, 120);
        shell.work.publish_status(a_status(2, 1));
        shell.route_bytes(b"/status\r");

        assert_eq!(
            shell.document(),
            vec![
                "* status: model=acme/model-9".to_string(),
                "  provider=gateway".to_string(),
                "  build_channel=debug".to_string(),
                "  build_revision=0123456789ab".to_string(),
                "  auth=AI_GATEWAY_API_KEY".to_string(),
                "  auth_refreshable=false".to_string(),
                "  permission_mode=auto".to_string(),
                format!("  sandbox={}", crate::output::SANDBOX_LABEL),
                "  workspace=/w/work".to_string(),
                "  history_turns=2".to_string(),
                "  session_permission_grants=1".to_string(),
                "  agent_step_limit=0".to_string(),
                String::new(),
            ]
        );
        // Read, never asked for: nothing went to the runtime, so a `/status`
        // typed mid-turn neither waits behind the turn nor shows as queued.
        assert!(
            shell.sent.try_recv().is_err(),
            "/status was handed to the runtime"
        );
        assert_eq!(
            shell.work.outstanding(),
            0,
            "/status took a place in the queue"
        );
    }

    #[test]
    fn status_reads_the_live_conversation_each_time_it_is_asked() {
        // The counts are the conversation the runtime is holding, as it last
        // said -- not the store's last session, and not what the band was told
        // at launch.
        let mut shell = shell(40, 120);
        shell.work.publish_status(a_status(0, 0));
        shell.route_bytes(b"/status\r");
        let before = shell.document();
        assert!(
            before.contains(&"  history_turns=0".to_string()),
            "{before:?}"
        );

        shell.work.publish_status(a_status(3, 2));
        shell.route_bytes(b"/status\r");
        let after = shell.document();
        assert!(
            after.contains(&"  history_turns=3".to_string()),
            "{after:?}"
        );
        assert!(
            after.contains(&"  session_permission_grants=2".to_string()),
            "{after:?}"
        );
    }

    #[test]
    fn status_makes_a_quoted_path_inert_before_it_reaches_the_terminal() {
        // The workspace is a directory name and a rejected provider quotes a
        // settings value; neither is xfx's text, and this one never crosses
        // the channel that would have cleaned it.
        let mut shell = shell(40, 120);
        let mut snapshot = a_status(0, 0);
        snapshot.workspace = "/w/\u{1b}]2;owned\u{7}x".to_string();
        shell.work.publish_status(snapshot);
        shell.route_bytes(b"/status\r");
        let document = shell.document();
        assert!(
            !document
                .iter()
                .any(|row| row.contains('\u{1b}') || row.contains('\u{7}')),
            "{document:?}"
        );
        assert!(
            document.contains(&"  workspace=/w/ ]2;owned x".to_string()),
            "{document:?}"
        );
    }

    #[test]
    fn stats_is_upstreams_five_counters_as_they_stand_before_it_is_written() {
        // `fx-stats-40x120-after.txt`: one neutral row, upstream's five keys in
        // upstream's order, the last two always nought.
        let mut shell = shell(40, 120);
        shell.metrics = Metrics {
            ansi_bytes: 1732,
            redraws: 2,
            debounced_resizes: 1,
        };
        shell.route_bytes(b"/stats\r");
        assert_eq!(
            shell.document(),
            said(notice::Notice::neutral(
                "stats",
                "ansi_bytes=1732, redraws=2, debounced_resizes=1, footer_updates=0, \
                 stream_chunks=0"
            ))
        );
        assert!(shell.sent.try_recv().is_err(), "/stats reached the runtime");
        // Reporting is not writing: the counters are the loop's to move.
        assert_eq!(shell.metrics.ansi_bytes, 1732);
    }

    #[test]
    fn status_and_stats_take_nothing_after_their_names() {
        // `fx-stats-40x120-extra.txt`: `/stats extra` is an unknown command,
        // and neither row accepts a payload upstream (`commands.zig:473,475`).
        // A trailing space is still the command.
        let mut shell = shell(40, 120);
        shell.work.publish_status(a_status(0, 0));
        shell.route_bytes(b"/stats extra\r");
        shell.route_bytes(b"/status now\r");
        let unknown = notice::Notice::error("command", "Unknown command. Try /help.");
        assert_eq!(
            shell.document(),
            [said(unknown.clone()), said(unknown)].concat()
        );
        shell.route_bytes(b"/stats \r");
        let document = shell.document();
        assert!(
            document[0].starts_with("* stats: ansi_bytes="),
            "{document:?}"
        );
        assert!(
            shell.sent.try_recv().is_err(),
            "a refused command reached the runtime"
        );
    }

    #[test]
    fn a_lone_name_nobody_has_is_unknown_and_one_with_words_is_a_prompt() {
        // Upstream refuses an unknown name only when it is the whole line
        // (`shouldRouteUnknownSlashCommand`, `input_submit_runtime.zig:926-934`);
        // with words after it, the line goes to the model as written.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"/nosuch\r");
        let unknown = notice::Notice::error("command", "Unknown command. Try /help.");
        assert_eq!(shell.document(), said(unknown));
        assert!(
            shell.sent.try_recv().is_err(),
            "a lone mistyped command was sent to the model"
        );
        shell.route_bytes(b"/nosuch with words\r");
        assert_eq!(
            shell.picks_up(),
            TurnWork::Submit("/nosuch with words".to_string())
        );
    }

    #[test]
    fn a_prompt_is_still_echoed_as_its_card() {
        // The other half of the rule: only a command goes without its echo.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"say the marker\r");
        assert_eq!(
            shell.picks_up(),
            TurnWork::Submit("say the marker".to_string())
        );
        assert_eq!(shell.document(), card("say the marker"));
    }

    #[test]
    fn a_notice_has_a_blank_row_above_it_unless_the_document_already_ends_on_one() {
        // A row that ends on its text -- the user's own amendment, read back --
        // so the notice after it opens a blank row first; the notice's own
        // blank row under it is the one the next notice stands on. (This case
        // used a tool row; a tool call is a group now, which ends on a blank
        // row of its own -- `a_notice_under_a_tool_group_shares_its_blank_row`.)
        let mut shell = shell(24, 80);
        shell.apply(UiEvent::ToolFeedback {
            call_id: "1".to_string(),
            text: "keep the header".to_string(),
        });
        shell.apply(UiEvent::Notice(notice::Notice::neutral("tool", "said")));
        assert_eq!(
            shell.document(),
            vec![
                format!("{AMENDMENT_PREFIX} keep the header"),
                String::new(),
                "* tool: said".to_string(),
                String::new()
            ]
        );
    }

    #[test]
    fn a_notice_under_a_tool_group_shares_its_blank_row() {
        // The group ends on its blank row, so the notice's blank row above is
        // that one -- one row between them, as between two notices.
        let mut shell = shell(24, 80);
        shell.apply(tool_start(
            "1",
            "read_file",
            serde_json::json!({"path": "a.txt"}),
        ));
        shell.apply(tool_result("1", "read_file", true, ""));
        shell.apply(UiEvent::Notice(notice::Notice::neutral("tool", "said")));
        assert_eq!(
            shell.written(),
            [
                "\u{25cf} 1 tool call \u{b7} 1 read",
                "\u{2514} Read a.txt",
                "",
                "* tool: said",
                "",
            ]
        );
    }

    #[test]
    fn a_notice_body_of_several_lines_is_its_head_and_rows_two_columns_in() {
        // `* status: model=…` and its `k=v` rows, as `fx-basic-40x120-status`
        // shows them; and a first line that wraps continues two columns in.
        let mut shell = shell(24, 20);
        shell.apply(UiEvent::Notice(notice::Notice::neutral(
            "status",
            "model=a\nhistory_turns=0\n",
        )));
        shell.apply(UiEvent::Notice(notice::Notice::error(
            "command",
            "Unknown command. Try /help.",
        )));
        assert_eq!(
            shell.document(),
            vec![
                "* status: model=a",
                "  history_turns=0",
                "",
                "\u{2717} command: Unknown ",
                "  command. Try ",
                "  /help.",
                "",
            ]
        );
    }

    #[test]
    fn an_informational_notice_has_upstreams_bold_label() {
        // `notice_information_style` is `system_notice_label_style`
        // (`app_render_runtime.zig:530`), bold 252 on dark.
        let mut shell = shell(24, 80);
        shell.apply(UiEvent::Notice(notice::Notice::new(
            notice::Tone::Information,
            "tip",
            "worth knowing",
        )));
        assert_eq!(
            shell.take_pending()[0].rows,
            vec![format!(
                "\u{1b}[1;38;5;252mi {RESET}\u{1b}[1;38;5;252mtip:{RESET}\
                 \u{1b}[38;5;250m worth knowing{RESET}"
            )]
        );
    }

    #[test]
    fn the_window_title_is_the_workspace_until_the_first_turn_titles_the_session() {
        // `app_session_runtime.zig:3608-3629`: `fx v0.0.13 | work` at launch,
        // the first prompt's text once its turn has finished, and the
        // fallback again once the session is dropped (`/new`).
        let mut shell = shell(24, 80);
        let fallback = shell.take_title().expect("the launch owes a title");
        assert!(
            fallback.starts_with(&format!("xfx v{} | ", crate::VERSION)),
            "{fallback:?}"
        );
        assert_eq!(
            shell.take_title(),
            None,
            "a title nobody changed was owed again"
        );

        turn_running(&mut shell, b"say the marker\nand more\r");
        let _ = shell.picks_up();
        assert_eq!(
            shell.take_title(),
            None,
            "a running turn titled the session"
        );
        shell.apply(UiEvent::TurnEnded { failure: None });
        assert_eq!(shell.take_title().as_deref(), Some("say the marker"));

        // Frozen at the first prompt, as upstream's derivation is.
        turn_running(&mut shell, b"another question\r");
        let _ = shell.picks_up();
        shell.apply(UiEvent::TurnEnded { failure: None });
        assert_eq!(shell.take_title(), None);
    }

    #[test]
    fn dropping_the_conversation_takes_the_window_title_back_to_the_workspace() {
        // `/new` and a provider switch both start the next prompt in a fresh
        // conversation, and upstream's `/new` clears the cached title
        // (`clearCachedSessionTitle`, `app_session_runtime.zig:1982`).
        let mut shell = shell(24, 80);
        let fallback = shell.take_title().expect("the launch owes a title");
        turn_running(&mut shell, b"say the marker\r");
        let _ = shell.picks_up();
        shell.apply(UiEvent::TurnEnded { failure: None });
        assert_eq!(shell.take_title().as_deref(), Some("say the marker"));

        shell.route_bytes(b"/new\r");
        assert_eq!(shell.take_title(), Some(fallback.clone()));

        // A prompt that was waiting when `/new` was taken belongs to the
        // conversation `/new` drops, so its turn -- if it started before the
        // reset reached it -- titles nothing.
        let mut queued = shell_with(24, 80, PALETTE, false);
        let _ = queued.take_title();
        queued.route_bytes(b"asked before the new session\r");
        queued.route_bytes(b"/new\r");
        let _ = queued.picks_up();
        queued.apply(UiEvent::TurnStarted);
        queued.apply(UiEvent::TurnEnded { failure: None });
        assert_eq!(
            queued.take_title(),
            None,
            "a prompt from the dropped conversation titled the new one"
        );

        let mut switched = shell_with(24, 80, PALETTE, false);
        let _ = switched.take_title();
        turn_running(&mut switched, b"say the marker\r");
        let _ = switched.picks_up();
        switched.apply(UiEvent::TurnEnded { failure: None });
        assert!(switched.take_title().is_some());
        switched.apply(UiEvent::ProviderSelected {
            provider: ProviderId::Llmux,
            model: "fable".to_string(),
            missing_credential: false,
        });
        assert!(
            switched
                .take_title()
                .is_some_and(|title| title.starts_with("xfx v")),
            "a provider switch kept the dropped conversation's title"
        );
    }

    #[test]
    fn a_failed_first_turn_does_not_title_the_session() {
        let mut shell = shell(24, 80);
        let _ = shell.take_title();
        turn_running(&mut shell, b"say the marker\r");
        let _ = shell.picks_up();
        shell.apply(UiEvent::TurnEnded {
            failure: Some("xfx: the gateway refused".to_string()),
        });
        assert_eq!(shell.take_title(), None);
        // And the next turn that does finish is the one that titles it.
        turn_running(&mut shell, b"second try\r");
        let _ = shell.picks_up();
        shell.apply(UiEvent::TurnEnded { failure: None });
        assert_eq!(shell.take_title().as_deref(), Some("second try"));
    }

    // -----------------------------------------------------------------------
    // /quit, /exit, and the way back in
    // -----------------------------------------------------------------------

    #[test]
    fn quit_and_exit_end_the_session_with_status_zero_and_say_nothing() {
        // `commandQuit` (`app_commands.zig:662-665`) writes no notice: what
        // the session says on the way out is the handoff, after the restore,
        // and that is not a document row.
        for line in [&b"/quit\r"[..], b"/exit\r", b"/quit \r"] {
            let mut shell = shell(24, 80);
            shell.route_bytes(line);
            assert!(shell.leaving(), "{line:?} did not end the session");
            assert_eq!(
                format!("{:?}", shell.exit_code()),
                format!("{:?}", ExitCode::SUCCESS),
                "{line:?}"
            );
            assert_eq!(shell.document(), Vec::<String>::new(), "{line:?}");
            assert!(
                shell.sent.try_recv().is_err(),
                "{line:?} reached the runtime"
            );
        }
    }

    #[test]
    fn quit_or_exit_with_an_argument_is_an_unknown_command_and_leaves_nothing() {
        // `/quit` takes nothing (`commands.zig:498`), so with a word after it
        // the line names no command (`command_router.zig:121-133`) and is
        // refused in upstream's words -- the session goes on.
        for line in [&b"/quit now\r"[..], b"/exit please\r", b"/exit  now \r"] {
            let mut shell = shell(24, 80);
            shell.route_bytes(line);
            assert!(!shell.leaving(), "{line:?} ended the session");
            assert_eq!(
                shell.document(),
                said(notice::Notice::error("command", UNKNOWN_COMMAND)),
                "{line:?}"
            );
            assert!(
                shell.sent.try_recv().is_err(),
                "{line:?} reached the runtime"
            );
            assert!(shell.editor.is_empty(), "{line:?} stayed in the composer");
        }
    }

    #[test]
    fn the_handoff_is_upstreams_dim_line_naming_the_xfx_command() {
        // `fx-sessions-40x120-quit.ansi` row 10:
        // `ESC[38;5;245mContinue session with: fx --resume PWuUVCd_N1-Z`, and
        // upstream's own reset and line feed (`render.zig:806-809`).
        let id = "1759800000000-0123456789abcdef";
        assert_eq!(
            resume_handoff(id, 120, PALETTE),
            format!("\u{1b}[38;5;245mContinue session with: xfx --resume {id}\u{1b}[0m\n")
        );
        // The light theme's dim grey (`theme.zig:128`).
        let light = Palette {
            mode: super::super::theme::Mode::Light,
            ..PALETTE
        };
        assert_eq!(
            resume_handoff(id, 120, light),
            format!("\u{1b}[38;5;247mContinue session with: xfx --resume {id}\u{1b}[0m\n")
        );
    }

    #[test]
    fn the_handoff_breaks_before_the_command_when_one_row_cannot_hold_it() {
        // `render.zig:803-804`: one row when label, space and command fit the
        // screen exactly, and otherwise the command on the next row, two in.
        let id = "abc";
        let whole = "Continue session with: xfx --resume abc".len();
        let width = u16::try_from(whole).expect("short");
        assert_eq!(
            resume_handoff(id, width, PALETTE),
            "\u{1b}[38;5;245mContinue session with: xfx --resume abc\u{1b}[0m\n"
        );
        assert_eq!(
            resume_handoff(id, width - 1, PALETTE),
            "\u{1b}[38;5;245mContinue session with:\n  xfx --resume abc\u{1b}[0m\n"
        );
        // Measured against the screen the session is on.
        let shell = shell(24, 40);
        let id = crate::session::SessionId::parse("1759800000000-0123456789abcdef").expect("an id");
        assert_eq!(
            shell.resume_handoff(&id),
            format!("\u{1b}[38;5;245mContinue session with:\n  xfx --resume {id}\u{1b}[0m\n")
        );
    }

    #[test]
    fn a_resumed_session_says_so_and_its_title_is_the_windows() {
        // `writeResumeNotice` (`app_session_runtime.zig:4657-4672`):
        // `* session resumed: <title>`, and the cached title -- the window's --
        // is the session's.
        let mut shell = shell(24, 80);
        let _ = shell.take_title();
        shell.hydrate(replay(Some("first question alpha"), Vec::new()), false);
        assert_eq!(
            shell.document(),
            said(notice::Notice::neutral(
                "session resumed",
                "first question alpha"
            ))
        );
        assert_eq!(shell.take_title().as_deref(), Some("first question alpha"));
        // Its first turn here does not retitle it: the title is the saved
        // session's first prompt, not this launch's.
        turn_running(&mut shell, b"a follow-up\r");
        let _ = shell.picks_up();
        shell.apply(UiEvent::TurnEnded { failure: None });
        assert_eq!(shell.take_title(), None);
    }

    #[test]
    fn a_resumed_session_with_no_title_is_untitled_and_keeps_the_fallback() {
        // `session_display_metadata.zig:14`; and only a real title is cached,
        // so the window stays on the workspace.
        let mut shell = shell(24, 80);
        let fallback = shell.take_title().expect("the launch owes a title");
        shell.hydrate(replay(None, Vec::new()), false);
        assert_eq!(
            shell.document(),
            said(notice::Notice::neutral(
                "session resumed",
                "Untitled session"
            ))
        );
        assert_eq!(shell.take_title(), None, "{fallback:?} was owed again");
        // A title read off a log is foreign text: what a terminal would obey
        // in it is a space by the time it is written.
        let mut hostile = self::shell(24, 80);
        let _ = hostile.take_title();
        hostile.hydrate(
            replay(Some("evil\u{1b}]2;owned\u{7} title"), Vec::new()),
            false,
        );
        let document = hostile.document();
        assert!(
            document.iter().all(|row| !row.contains('\u{1b}')),
            "{document:?}"
        );
        assert!(
            hostile
                .take_title()
                .is_some_and(|title| !title.contains('\u{1b}') && !title.contains('\u{7}')),
            "the window title carried a control"
        );
    }

    /// A saved session as the runtime hands it over, with no model to
    /// change.
    fn replay(title: Option<&str>, turns: Vec<HydratedTurn>) -> Hydration {
        Hydration {
            title: title.map(str::to_string),
            turns,
            model: None,
        }
    }

    /// One saved turn that concluded.
    fn saved(
        user: &str,
        steps: Vec<HydratedStep>,
        seconds: u64,
        output: Option<u64>,
    ) -> HydratedTurn {
        HydratedTurn {
            user: user.to_string(),
            steps,
            ending: Ending::Concluded { seconds, output },
            started_at_ms: 0,
            concluded_at_ms: None,
            details: Vec::new(),
        }
    }

    impl Fixture {
        /// The document as a terminal holds it once everything owed has
        /// landed, **with its colours** -- the bytes of each row, after the
        /// rewrites a tool group's block makes ([`Self::written`]'s replay,
        /// with nothing taken off).
        fn painted_document(&mut self) -> Vec<String> {
            let mut document: Vec<String> = Vec::new();
            for append in self.take_pending() {
                let settled = append.rows.len() - append.scroll.min(append.rows.len());
                document.truncate(document.len().saturating_sub(settled));
                document.extend(append.rows.iter().cloned());
            }
            document
        }
    }

    #[test]
    fn a_resumed_session_replays_its_turns_as_the_live_ones_were_written() {
        // `fx-sessions-40x120-resumed.txt`: the notice, then each turn as it
        // was shown -- the card, the answer through the presenter, the dim
        // summary. Compared byte for byte, colours included, against the same
        // two turns run live through the events a runtime sends.
        let first = "ALPHA answer line one.\n\nSecond paragraph with `code`.";
        let second = "BETA answer.";
        let mut live = shell(40, 120);
        for (prompt, answer) in [
            ("first question alpha", first),
            ("second question beta", second),
        ] {
            live.route_bytes(format!("{prompt}\r").as_bytes());
            let _ = live.picks_up();
            live.apply(UiEvent::TurnStarted);
            live.apply(UiEvent::Delta(answer.to_string()));
            live.apply(UiEvent::Usage {
                input: Some(6),
                output: Some(40),
            });
            live.apply(UiEvent::TurnEnded { failure: None });
            live.shell.flush_paced();
        }
        let live_rows = live.painted_document();

        let mut resumed = shell(40, 120);
        resumed.hydrate(
            replay(
                Some("first question alpha"),
                vec![
                    saved(
                        "first question alpha",
                        vec![HydratedStep::Text(first.to_string())],
                        0,
                        Some(40),
                    ),
                    saved(
                        "second question beta",
                        vec![HydratedStep::Text(second.to_string())],
                        0,
                        Some(40),
                    ),
                ],
            ),
            true,
        );
        let rows = resumed.painted_document();
        // The notice is the one `write_notice` paints everywhere: `* ` and
        // the topic and the body, all in the notice grey.
        assert!(rows[0].starts_with(PALETTE.notice()), "{rows:?}");
        assert_eq!(rows[1], "");
        assert_eq!(
            rows[2..],
            live_rows[..],
            "the replay is not the live document"
        );
        // And in words: upstream's rows under the notice.
        assert_eq!(
            rows.iter().map(|row| colourless(row)).collect::<Vec<_>>(),
            [
                "* session resumed: first question alpha",
                "",
                "\u{2503} first question alpha",
                "",
                "  ALPHA answer line one.",
                "",
                "  Second paragraph with code.",
                "",
                "  0s (\u{2191}6 \u{2193}40)",
                "",
                "\u{2503} second question beta",
                "",
                "  BETA answer.",
                "",
                "  0s (\u{2191}5 \u{2193}40)",
                "",
            ]
        );
        assert!(resumed.owes_wipe(), "the picker's resume reset nothing");
        assert_eq!(
            resumed.take_title().as_deref(),
            Some("first question alpha")
        );
    }

    #[test]
    fn ctrl_o_after_a_resume_shows_the_replayed_session_and_not_the_one_it_replaced() {
        // Upstream's full transcript after a resume holds the saved turns
        // (`tui-resume.test.ts:900-960`). The replay goes through the writes
        // the record is kept by, so the record is the replayed document.
        let mut shell = shell(40, 120);
        shell.route_bytes(b"MARKER-OLD prompt\r");
        let _ = shell.picks_up();
        shell.work.give_back();
        shell.apply(UiEvent::TurnStarted);
        shell.apply(UiEvent::Delta("MARKER-OLD answer".to_string()));
        shell.apply(UiEvent::TurnEnded { failure: None });
        shell.shell.flush_paced();
        let _ = shell.written();
        shell.apply(UiEvent::Resumed(replay(
            Some("first question alpha"),
            vec![saved(
                "first question alpha",
                vec![HydratedStep::Text("ALPHA answer line one.".to_string())],
                0,
                Some(40),
            )],
        )));
        let document: Vec<String> = shell.written();
        shell.route_bytes(b"\x0f");
        let screen = full_screen(&shell).join("\n");
        assert!(screen.contains("first question alpha"), "{screen}");
        assert!(screen.contains("ALPHA answer line one."), "{screen}");
        assert!(screen.contains("session resumed"), "{screen}");
        assert!(!screen.contains("MARKER-OLD"), "{screen}");
        assert!(
            !document.iter().any(|row| row.contains("MARKER-OLD")),
            "{document:?}"
        );
    }

    #[test]
    fn a_resumed_turn_fills_the_full_transcript_with_its_calls_details_at_the_logs_times() {
        // Upstream's resume replays a call through the adapters a live one
        // went through, its output included (`writeCompletedToolResult`,
        // `app_session_runtime.zig:4970-5030`), and stamps the turn's entries
        // with its start and its conclusion (`setCreatedAtMs`, `:4753-4810`).
        let input = serde_json::json!({"path": "notes.txt"});
        let shown = crate::tools::Registry::builtin()
            .spec("read_file")
            .expect("read_file")
            .shown(&input);
        let steps = vec![
            crate::session::TurnStep::Assistant {
                text: "Looking.".to_string(),
                tool_calls: vec![crate::session::RecordedToolCall {
                    id: "c1".into(),
                    name: "read_file".into(),
                    input,
                }],
                raw_content: Vec::new(),
                responses_state: Vec::new(),
                wire: None,
            },
            crate::session::TurnStep::ToolResult {
                call_id: "c1".into(),
                tool: "read_file".into(),
                ok: true,
                output:
                    "<path>notes.txt</path>\n<content>\n1\tONLY-IN-THE-LOG\u{1b}[2J\n</content>"
                        .into(),
            },
        ];
        let mut turn = saved(
            "look",
            vec![
                HydratedStep::Text("Looking.".to_string()),
                HydratedStep::ToolStart {
                    id: "c1".into(),
                    shown,
                },
                HydratedStep::ToolResult {
                    id: "c1".into(),
                    ok: true,
                    detail: String::new(),
                    outcome: CallOutcome::Finished,
                },
                HydratedStep::Text("Done.".to_string()),
            ],
            2,
            Some(40),
        );
        // 2026-10-02 00:00:00 UTC, and two seconds later.
        turn.started_at_ms = 1_790_899_200_000;
        turn.concluded_at_ms = Some(1_790_899_202_000);
        turn.details = full_transcript::details_of(&steps);
        let mut shell = shell(40, 120);
        shell.hydrate(replay(Some("look"), vec![turn]), false);
        shell.route_bytes(b"\x0f");
        let screen = full_screen(&shell);
        let headers: Vec<&str> = screen
            .iter()
            .filter(|row| row.contains(" UTC \u{b7} "))
            .map(String::as_str)
            .collect();
        assert_eq!(
            headers[1..],
            [
                "  2026-10-02 00:00:00.000 UTC \u{b7} Prompt",
                "  2026-10-02 00:00:00.000 UTC \u{b7} Response",
                "  2026-10-02 00:00:00.000 UTC \u{b7} Tool",
                "  2026-10-02 00:00:02.000 UTC \u{b7} Response",
                "  2026-10-02 00:00:02.000 UTC \u{b7} Usage",
            ],
            "{screen:#?}"
        );
        // What the read returned, which the document never showed -- made
        // inert, as a live turn's would be at the channel.
        assert!(
            screen.contains(&"\u{2502}  1    ONLY-IN-THE-LOG [2J".to_string()),
            "{screen:#?}"
        );
        assert!(
            !shell
                .written()
                .iter()
                .any(|row| row.contains("ONLY-IN-THE-LOG")),
            "the document was given what only the full transcript shows"
        );
    }

    #[test]
    fn a_replayed_tool_group_is_the_live_group_and_a_stopped_turn_ends_on_its_reason() {
        let read = crate::tools::Registry::builtin()
            .spec("read_file")
            .expect("read_file")
            .shown(&serde_json::json!({"path": "notes.txt"}));
        let mut live = shell(40, 120);
        live.route_bytes(b"look\r");
        let _ = live.picks_up();
        live.apply(UiEvent::TurnStarted);
        live.apply(UiEvent::Delta("Looking.".to_string()));
        live.apply(tool_start(
            "c1",
            "read_file",
            serde_json::json!({"path": "notes.txt"}),
        ));
        live.apply(tool_result("c1", "read_file", true, "notes.txt"));
        live.apply(tool_start(
            "c2",
            "read_file",
            serde_json::json!({"path": "gone.txt"}),
        ));
        live.apply(tool_result(
            "c2",
            "read_file",
            false,
            "no such path: `gone.txt`",
        ));
        live.apply(UiEvent::Delta("Done.".to_string()));
        live.apply(UiEvent::TurnEnded { failure: None });
        live.shell.flush_paced();
        let live_rows = live.painted_document();

        let mut resumed = shell(40, 120);
        resumed.hydrate(
            replay(
                Some("look"),
                vec![
                    saved(
                        "look",
                        vec![
                            HydratedStep::Text("Looking.".to_string()),
                            HydratedStep::ToolStart {
                                id: "c1".to_string(),
                                shown: read.clone(),
                            },
                            HydratedStep::ToolResult {
                                id: "c1".to_string(),
                                ok: true,
                                detail: String::new(),
                                outcome: CallOutcome::Finished,
                            },
                            HydratedStep::ToolStart {
                                id: "c2".to_string(),
                                shown: crate::tools::Shown {
                                    target: Some("gone.txt".to_string()),
                                    ..read
                                },
                            },
                            HydratedStep::ToolResult {
                                id: "c2".to_string(),
                                ok: false,
                                detail: "no such path: `gone.txt`".to_string(),
                                outcome: CallOutcome::Finished,
                            },
                            HydratedStep::Text("Done.".to_string()),
                        ],
                        0,
                        None,
                    ),
                    HydratedTurn {
                        user: "and then".to_string(),
                        steps: Vec::new(),
                        ending: Ending::Interrupted("xfx: the turn was cancelled".to_string()),
                        started_at_ms: 0,
                        concluded_at_ms: None,
                        details: Vec::new(),
                    },
                ],
            ),
            false,
        );
        let rows = resumed.painted_document();
        // The live turn's summary estimated what streamed, as the replay
        // does when the log recorded no usage: the same row.
        assert_eq!(rows[2..2 + live_rows.len()], live_rows[..], "{rows:?}");
        let words: Vec<String> = rows.iter().map(|row| colourless(row)).collect();
        assert!(
            words.contains(&"\u{25cf} 2 tool calls \u{b7} 2 read \u{b7} 1 failed".to_string()),
            "{words:?}"
        );
        // The stopped turn: its card and the sentence it ended on, under
        // `system` as the live conclusion was, and no summary.
        assert_eq!(
            words[2 + live_rows.len()..],
            [
                "\u{2503} and then",
                "",
                "\u{2717} system: the turn was cancelled",
                "",
            ]
        );
        assert!(!resumed.owes_wipe(), "a launch's resume reset the terminal");
    }

    #[test]
    fn resume_opens_the_picker_lists_what_the_runtime_read_and_enter_asks_for_the_marked_one() {
        use super::super::session_picker::{ListedSession, Listing, ResumeFailure};
        let mut shell = shell(40, 120);
        shell.route_bytes(b"/resume\r");
        assert_eq!(shell.picks_up(), TurnWork::Sessions);
        shell.work.give_back();
        let band = shell.band_rows();
        assert!(
            band.iter()
                .any(|row| visible_text(row) == "  Loading sessions\u{2026}"),
            "{band:?}"
        );
        let session = |id: &str, title: &str| ListedSession {
            id: id.to_string(),
            title: Some(title.to_string()),
            workspace: "/x/work".to_string(),
            updated_at_ms: 0,
            turns: 1,
        };
        shell.apply(UiEvent::Sessions(Some(Listing {
            current: vec![
                session("b", "second question beta"),
                session("a", "first question alpha"),
            ],
            all: Vec::new(),
            now_ms: 0,
        })));
        let band: Vec<String> = shell
            .band_rows()
            .iter()
            .map(|row| visible_text(row))
            .collect();
        assert!(
            band.contains(&"Sessions 2  [Current workspace]  All workspaces".to_string()),
            "{band:?}"
        );
        assert_eq!(
            band.last().map(String::as_str),
            Some("\u{2191}\u{2193} navigate     tab scope     enter resume     esc close")
        );
        // Down, Down: the mark stops on the last row; typing narrows.
        shell.route_bytes(b"\x1b[B\x1b[B");
        shell.route_bytes(b"\r");
        assert_eq!(shell.picks_up(), TurnWork::Resume("a".to_string()));
        // A second Enter while that is on its way asks for nothing.
        shell.route_bytes(b"\r");
        assert!(
            shell.sent.try_recv().is_err(),
            "a second resume was asked for"
        );
        shell.work.give_back();
        shell.apply(UiEvent::ResumeFailed {
            id: "a".to_string(),
            failure: ResumeFailure::OpenElsewhere,
        });
        let band: Vec<String> = shell
            .band_rows()
            .iter()
            .map(|row| visible_text(row))
            .collect();
        assert!(
            band.contains(
                &"  This session is open in another xfx. Close it there, then press enter to retry."
                    .to_string()
            ),
            "{band:?}"
        );
        // Enter retries; the runtime answers with the session, and the picker,
        // its search and the old screen go.
        shell.route_bytes(b"first");
        shell.route_bytes(b"\r");
        assert_eq!(shell.picks_up(), TurnWork::Resume("a".to_string()));
        shell.apply(UiEvent::Resumed(replay(
            Some("first question alpha"),
            Vec::new(),
        )));
        assert!(shell.sessions.is_none());
        assert!(shell.editor.is_empty(), "the search outlived the picker");
        assert!(shell.owes_wipe());
        assert_eq!(shell.geometry.panel, 0);
    }

    #[test]
    fn a_resume_with_nothing_searched_gives_the_band_its_rows_back() {
        use super::super::session_picker::{ListedSession, Listing};
        let mut shell = shell(40, 120);
        shell.route_bytes(b"/resume\r");
        let _ = shell.picks_up();
        shell.work.give_back();
        shell.apply(UiEvent::Sessions(Some(Listing {
            current: vec![ListedSession {
                id: "a".to_string(),
                title: Some("first question alpha".to_string()),
                workspace: "/x/work".to_string(),
                updated_at_ms: 0,
                turns: 1,
            }],
            all: Vec::new(),
            now_ms: 0,
        })));
        assert!(shell.geometry.panel > 0);
        shell.route_bytes(b"\r");
        assert_eq!(shell.picks_up(), TurnWork::Resume("a".to_string()));
        shell.apply(UiEvent::Resumed(replay(
            Some("first question alpha"),
            Vec::new(),
        )));
        assert!(shell.sessions.is_none());
        assert_eq!(shell.geometry.panel, 0, "the picker's rows outlived it");
        // The band is the idle one again: composer, blank row, hint row.
        assert_eq!(shell.band_rows().len(), 3, "{:?}", shell.band_rows());
    }

    #[test]
    fn escape_closes_the_picker_and_its_search_and_tab_switches_the_scope() {
        use super::super::session_picker::{ListedSession, Listing};
        let mut shell = shell(40, 120);
        shell.route_bytes(b"/resume\r");
        let _ = shell.picks_up();
        let elsewhere = ListedSession {
            id: "o".to_string(),
            title: Some("elsewhere".to_string()),
            workspace: "/y/other".to_string(),
            updated_at_ms: 0,
            turns: 2,
        };
        shell.apply(UiEvent::Sessions(Some(Listing {
            current: Vec::new(),
            all: vec![elsewhere],
            now_ms: 0,
        })));
        let band: Vec<String> = shell
            .band_rows()
            .iter()
            .map(|row| visible_text(row))
            .collect();
        assert!(
            band.contains(&"  No sessions found.".to_string()),
            "{band:?}"
        );
        // Enter on nothing does nothing.
        shell.route_bytes(b"\r");
        assert!(shell.sent.try_recv().is_err());
        assert!(shell.sessions.is_some());
        // Shift-Tab is the other scope too.
        shell.route_bytes(b"\x1b[Z");
        let band: Vec<String> = shell
            .band_rows()
            .iter()
            .map(|row| visible_text(row))
            .collect();
        assert!(
            band.contains(&"Sessions 1  Current workspace  [All workspaces]".to_string()),
            "{band:?}"
        );
        // The search is the composer; Escape takes both down.
        shell.route_bytes(b"zzz");
        assert_eq!(shell.editor.text(), "zzz");
        shell.route_bytes(&[0x1b]);
        shell.settle_input(Instant::now() + Decoder::ESC_TIMEOUT);
        assert!(shell.sessions.is_none());
        assert!(shell.editor.is_empty(), "the search outlived the picker");
        assert_eq!(shell.geometry.panel, 0);
    }

    #[test]
    fn resume_is_refused_while_a_turn_is_in_hand_and_opens_nothing() {
        // `fx-busy-40x120-resume-while-streaming.txt`.
        let mut shell = shell(40, 120);
        let _ = turn_running(&mut shell, b"stream one\r");
        shell.route_bytes(b"/resume\r");
        assert!(
            shell.sessions.is_none(),
            "a picker opened over a running turn"
        );
        assert_eq!(
            shell.document(),
            said(notice::Notice::neutral(
                "session",
                "resume is unavailable until the response finishes"
            ))
        );
    }

    #[test]
    fn rename_refuses_in_upstreams_words_and_titles_the_window_at_once() {
        // `fx-basic-40x120-rename-empty.txt` and `-rename-title.txt`.
        let mut shell = shell(24, 120);
        let _ = shell.take_title();
        shell.route_bytes(b"/rename\r");
        assert_eq!(
            shell.document(),
            said(notice::Notice::error("", "usage: /rename <title>"))
        );
        assert_eq!(shell.document(), Vec::<String>::new());
        assert!(
            shell.sent.try_recv().is_err(),
            "an empty title reached the runtime"
        );
        shell.route_bytes(format!("/rename {}\r", "x".repeat(241)).as_bytes());
        assert_eq!(
            shell.document(),
            said(notice::Notice::error("session", "title is too long"))
        );
        CommandHandlers::rename(&mut *shell, "a\tb");
        assert_eq!(
            shell.document(),
            said(notice::Notice::error(
                "session",
                "title must be printable text"
            ))
        );
        assert_eq!(
            shell.take_title(),
            None,
            "a refused title titled the window"
        );

        shell.route_bytes(b"/rename   My Title  \r");
        assert_eq!(shell.picks_up(), TurnWork::Rename("My Title".to_string()));
        shell.work.give_back();
        assert_eq!(
            shell.take_title(),
            None,
            "titled before the runtime renamed it"
        );
        shell.apply(UiEvent::Renamed {
            title: "My Title".to_string(),
            unsaved: None,
        });
        assert_eq!(shell.take_title().as_deref(), Some("My Title"));
        assert_eq!(
            visible_text(&shell.document()[0]),
            "* session: renamed to \"My Title\""
        );
        // A store that refused: the title is this process's, and the warning
        // says so (`app_commands.zig:3833-3846`).
        shell.apply(UiEvent::Renamed {
            title: "Kept here".to_string(),
            unsaved: Some("cannot record the `session_renamed` session event".to_string()),
        });
        assert_eq!(shell.take_title().as_deref(), Some("Kept here"));
        assert_eq!(
            visible_text(&shell.document()[0]),
            "! session: renamed for this process but not saved \
             (cannot record the `session_renamed` session event)"
        );
        // A first turn afterwards does not retitle it: the user's title
        // outranks the prompt's, as it does in the store.
        turn_running(&mut shell, b"first question alpha\r");
        let _ = shell.picks_up();
        shell.apply(UiEvent::TurnEnded { failure: None });
        assert_eq!(shell.take_title(), None);
    }

    #[test]
    fn a_line_with_a_slash_that_does_not_lead_it_is_a_prompt() {
        // `classify` reads the *first* character, so this is a question about
        // paths and not a command at all.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"what does a/b mean\r");
        assert_eq!(
            shell.picks_up(),
            TurnWork::Submit("what does a/b mean".to_string())
        );
    }

    /// The research harness's catalog (`account-model/scenarios/models.json`),
    /// in the menu's order -- the rows both binaries were captured over.
    fn harness_rows() -> Vec<CatalogEntry> {
        let row = |id: &str, efforts: &[&str]| CatalogEntry {
            id: id.to_string(),
            aliases: Vec::new(),
            name: None,
            efforts: efforts.iter().map(|effort| effort.to_string()).collect(),
            max_context: None,
            facts: Default::default(),
        };
        vec![
            row("openai/gpt-6-astra", &["low", "medium", "high", "xhigh"]),
            row("anthropic/claude-opus-5", &["low", "medium", "high", "max"]),
            row("fake/model-a", &[]),
            row("xai/grok-4.7", &[]),
        ]
    }

    /// A 40x120 shell holding the harness's catalog, as the captures do.
    fn harness_shell() -> Fixture {
        let mut shell = shell(40, 120);
        shell.apply(UiEvent::CatalogLoaded {
            provider: ProviderId::Gateway,
            entries: harness_rows(),
        });
        let _ = shell.document();
        shell
    }

    /// The next model change the shell put on the control channel.
    fn change(shell: &mut Fixture) -> ModelChange {
        match shell.controlled() {
            Some(TurnControl::Model(change)) => change,
            other => panic!("expected a model change, got {other:?}"),
        }
    }

    /// A pasted line, which is not typing: no column stage survives it, and no
    /// space in it advances anything.
    fn pasted(shell: &mut Fixture, text: &str) {
        shell.route_bytes(format!("\x1b[200~{text}\x1b[201~").as_bytes());
    }

    #[test]
    fn model_with_one_word_asks_the_runtime_and_bare_model_opens_the_menu() {
        // With no catalog in hand the band cannot resolve a query, so it goes
        // as typed and the runtime's answer is the announcement. The space
        // after `/model` opened the column, which asked for the catalog -- the
        // one piece of *work* in this, because it opens a socket.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"/model second-model\r");
        assert_eq!(shell.picks_up(), TurnWork::Catalog);
        assert_eq!(
            change(&mut shell),
            ModelChange {
                name: "second-model".to_string(),
                effort: None,
                picker: false,
                announced: false,
                next_turn: false,
            },
            "the model change never reached the thread that owns the session log"
        );
        assert!(
            shell.sent.try_recv().is_err(),
            "a model change took a place in the queue"
        );
        assert!(
            shell.document().is_empty(),
            "the band announced an id it could not know"
        );
        // The thread that owns the rule answers, and that answer -- not the
        // submission -- is what the band's own model becomes.
        shell.apply(UiEvent::ModelAnswered {
            model: "second-model".to_string(),
            effort: Effort::Auto,
            outcome: ModelAnswer::Applied { unverified: None },
            next_turn: false,
            announced: false,
        });
        assert_eq!(
            shell.document(),
            said(notice::Notice::neutral(
                "",
                "Switched to second-model (effort: default, speed: normal)"
            ))
        );

        // Bare, it opens the menu and writes nothing: the answer is in the
        // band (`fx-model-40x120-open`). The load the column asked for is
        // still on its way, so the menu waits for it rather than asking twice.
        shell.route_bytes(b"/model\r");
        assert!(
            shell.document().is_empty(),
            "a bare /model wrote to the document"
        );
        assert_eq!(
            menu(&shell)
                .iter()
                .map(|row| visible_text(row))
                .collect::<Vec<_>>(),
            ["Models 0  [All]", "", "Loading models\u{2026}"]
        );
        assert!(
            shell.sent.try_recv().is_err(),
            "a load already on its way was asked for again"
        );
        // A session that has asked nothing yet asks the runtime: **the one
        // network call a bare `/model` makes**.
        let mut fresh = self::shell(24, 80);
        fresh.shell.use_model("");
        assert_eq!(fresh.picks_up(), TurnWork::Catalog);
    }

    #[test]
    fn a_query_the_band_cannot_resolve_is_the_runtimes_to_answer() {
        // A query is resolved where the catalog is, and with none in hand this
        // side cannot know which id it becomes. A band that adopted the query
        // it submitted would put `opus` on the hint row while the next turn
        // was held in `anthropic/claude-opus-5`.
        let mut shell = shell(24, 80);
        let in_force = shell.shell.model.clone();
        shell.shell.use_model("opus");
        assert_eq!(change(&mut shell).name, "opus");
        assert_eq!(
            shell.shell.model, in_force,
            "the band adopted a model nothing has applied yet"
        );
        assert!(
            shell.document().is_empty(),
            "the band reported a model change the runtime has not made"
        );

        // A refusal -- only an unusable id gets one now -- leaves the band on
        // the model the session is really in.
        shell.apply(UiEvent::ModelAnswered {
            model: in_force.clone(),
            effort: Effort::Auto,
            outcome: ModelAnswer::Refused {
                reason: "a model id cannot contain control characters".to_string(),
            },
            next_turn: false,
            announced: false,
        });
        assert_eq!(shell.shell.model, in_force);
        assert_eq!(
            shell.document(),
            said(notice::Notice::error(
                "model",
                "a model id cannot contain control characters"
            ))
        );

        // And the answer moves it, to what the runtime resolved, with the
        // effort the session is in. **No caveat** follows a selection the
        // catalog could not confirm: upstream applies an id nothing lists
        // without a word (`fx-model-fuzzy-40x120-unknown`).
        shell.apply(UiEvent::ModelAnswered {
            model: "anthropic/claude-opus-5".to_string(),
            effort: Effort::Named("low".to_string()),
            outcome: ModelAnswer::Applied {
                unverified: Some("nothing answered".to_string()),
            },
            next_turn: false,
            announced: false,
        });
        assert_eq!(shell.shell.model, "anthropic/claude-opus-5");
        assert_eq!(
            shell.document(),
            said(notice::Notice::neutral(
                "",
                "Switched to anthropic/claude-opus-5 (effort: low, speed: normal)"
            ))
        );
        // No catalog has been read, so nothing says the model does not take
        // the level: the footer shows it (`active_capabilities_pending`).
        assert!(
            shell.hint().ends_with("auto \u{b7} opus 5 \u{b7} low"),
            "the footer does not follow it: {}",
            shell.hint()
        );
        // An unchanged model is announced the same way, as upstream's is.
        shell.apply(UiEvent::ModelAnswered {
            model: "anthropic/claude-opus-5".to_string(),
            effort: Effort::Auto,
            outcome: ModelAnswer::Unchanged,
            next_turn: false,
            announced: false,
        });
        assert_eq!(
            shell.document(),
            said(notice::Notice::neutral(
                "",
                "Switched to anthropic/claude-opus-5 (effort: default, speed: normal)"
            ))
        );
        // A `-fast` id is fast by its name (`intrinsic_fast`).
        shell.apply(UiEvent::ModelAnswered {
            model: "zai/glm-5-fast".to_string(),
            effort: Effort::Auto,
            outcome: ModelAnswer::Applied { unverified: None },
            next_turn: false,
            announced: false,
        });
        assert_eq!(
            shell.document(),
            said(notice::Notice::neutral(
                "",
                "Switched to zai/glm-5-fast (effort: default, speed: fast)"
            ))
        );
    }

    #[test]
    fn a_query_the_band_can_resolve_is_announced_at_once_as_the_id_it_names() {
        // `fx-model-fuzzy-40x120-opus`: `/model opus` -> `Switched to
        // anthropic/claude-opus-5 (effort: default, speed: normal)`, said the
        // moment it is given. The band holds the rows, resolves by the
        // selector's own rule, and sends the **id** -- so the runtime applies
        // exactly what was said.
        let mut shell = harness_shell();
        shell.shell.use_model("opus");
        assert_eq!(
            change(&mut shell),
            ModelChange {
                name: "anthropic/claude-opus-5".to_string(),
                effort: None,
                picker: false,
                announced: true,
                next_turn: false,
            }
        );
        assert_eq!(
            shell.document(),
            said(notice::Notice::neutral(
                "",
                "Switched to anthropic/claude-opus-5 (effort: default, speed: normal)"
            ))
        );
        assert_eq!(shell.shell.model, "anthropic/claude-opus-5");
        // The runtime's answer for an announced change moves nothing it has
        // not already moved, and says nothing again.
        shell.apply(UiEvent::ModelAnswered {
            model: "anthropic/claude-opus-5".to_string(),
            effort: Effort::Auto,
            outcome: ModelAnswer::Applied { unverified: None },
            next_turn: false,
            announced: true,
        });
        assert!(
            shell.document().is_empty(),
            "the change was announced twice"
        );
        // An id nothing lists is applied as typed, and announced as typed
        // (`fx-model-fuzzy-40x120-unknown`).
        shell.shell.use_model("nonexistent-xyz");
        assert_eq!(change(&mut shell).name, "nonexistent-xyz");
        assert_eq!(
            shell.document(),
            said(notice::Notice::neutral(
                "",
                "Switched to nonexistent-xyz (effort: default, speed: normal)"
            ))
        );
        // A provider with no catalog applies any query as typed, so that is
        // announceable too.
        let mut empty = self::shell(24, 80);
        empty.apply(UiEvent::CatalogLoaded {
            provider: ProviderId::Llmux,
            entries: Vec::new(),
        });
        empty.shell.use_model("fable");
        assert!(change(&mut empty).announced);
    }

    #[test]
    fn a_model_asked_for_while_a_turn_runs_is_for_the_next_turn_and_never_refused() {
        // Upstream's `Next turn will use` (`announceModelSelection`), said at
        // once. **A control message**, so a turn running and a prompt waiting
        // -- a full queue -- do not refuse it: the runtime puts it aside and
        // applies it between the two.
        let mut shell = harness_shell();
        let _started = turn_running(&mut shell, b"ask something\r");
        let _turn = shell.picks_up();
        shell.route_bytes(b"waiting\r");
        let _ = shell.document();
        shell.shell.use_model("opus");
        assert_eq!(shell.notice, None, "a model change was refused as busy");
        let asked = change(&mut shell);
        assert!(asked.next_turn && asked.announced, "{asked:?}");
        assert_eq!(
            shell.document(),
            said(notice::Notice::neutral(
                "",
                "Next turn will use anthropic/claude-opus-5 (effort: default, speed: normal)"
            ))
        );

        // A query the band cannot resolve is worded for the moment it was
        // asked in, even when the answer comes after the turn.
        let mut unknown = self::shell(24, 80);
        let _started = turn_running(&mut unknown, b"ask something\r");
        let _turn = unknown.picks_up();
        unknown.shell.use_model("second-model");
        assert!(change(&mut unknown).next_turn);
        unknown.apply(UiEvent::TurnEnded { failure: None });
        let _ = unknown.document();
        unknown.apply(UiEvent::ModelAnswered {
            model: "second-model".to_string(),
            effort: Effort::Auto,
            outcome: ModelAnswer::Applied { unverified: None },
            next_turn: true,
            announced: false,
        });
        assert_eq!(
            unknown.document(),
            said(notice::Notice::neutral(
                "",
                "Next turn will use second-model (effort: default, speed: normal)"
            ))
        );
    }

    #[test]
    fn an_explicit_form_that_does_not_hold_is_upstreams_usage_line_and_the_composer_keeps_it() {
        // `fx-model3-40x120-paste-*`: an effort the model does not list, a
        // speed it cannot have, four words, and an id that is not literal are
        // all `✗ usage: …`, no topic, the text kept for the user to fix, and
        // nothing sent.
        for line in [
            "/model fake/model-a high",
            "/model anthropic/claude-opus-5 high ultrafast",
            "/model openai/gpt-6-astra xhigh fast extra",
            "/model claude-opus-5 low",
            // xfx sends no speed yet, so a third word cannot be honoured
            // (`docs/parity.md`, the speed column).
            "/model openai/gpt-6-astra xhigh fast",
        ] {
            let mut shell = harness_shell();
            pasted(&mut shell, line);
            shell.route_bytes(b"\r");
            assert_eq!(
                shell.document(),
                said(notice::Notice::error(
                    "",
                    "usage: /model <id> <effort> [normal|fast|ultrafast]"
                )),
                "{line}"
            );
            assert_eq!(shell.editor.text(), line);
            assert!(
                shell.controlled().is_none(),
                "a refused /model was sent: {line}"
            );
            assert!(shell.model_menu.is_none());
        }
    }

    #[test]
    fn a_third_word_is_normal_for_a_model_with_a_speed_to_decline_and_nothing_else_yet() {
        // `validateExplicitModelSelection`: `normal` declines a speed, so it
        // needs a model that has one; `fast` and `ultrafast` need the speed
        // column xfx does not have yet.
        let mut rows = harness_rows();
        rows[1].facts.fast = true;
        let mut shell = shell(40, 120);
        shell.apply(UiEvent::CatalogLoaded {
            provider: ProviderId::Gateway,
            entries: rows,
        });
        let _ = shell.document();
        pasted(&mut shell, "/model anthropic/claude-opus-5 high NORMAL");
        shell.route_bytes(b"\r");
        let made = change(&mut shell);
        assert_eq!(
            (made.name.as_str(), made.effort),
            (
                "anthropic/claude-opus-5",
                Some(Effort::Named("high".to_string()))
            )
        );
        for refused in [
            "/model anthropic/claude-opus-5 high fast",
            "/model openai/gpt-6-astra low normal",
            "/model anthropic/claude-opus-5 high slow",
        ] {
            let _ = shell.document();
            pasted(&mut shell, refused);
            shell.route_bytes(b"\r");
            assert!(shell.controlled().is_none(), "{refused}");
            assert_eq!(shell.editor.text(), refused);
            shell.shell.clear_composer();
        }
        // A second word that is no effort is not this form at all: it is the
        // router form's, and its own refusal (`.none`).
        let _ = shell.document();
        pasted(&mut shell, "/model fake a/b");
        shell.route_bytes(b"\r");
        assert_eq!(
            shell.document(),
            said(notice::Notice::error(
                "model",
                "/model takes one model id, with no spaces in it"
            ))
        );
    }

    #[test]
    fn an_explicit_form_that_holds_is_applied_with_its_effort_and_announced() {
        // `fx-model3-40x120-paste-ok`, less the speed xfx cannot send yet.
        let mut shell = harness_shell();
        pasted(&mut shell, "/model openai/gpt-6-astra xhigh");
        shell.route_bytes(b"\r");
        assert_eq!(
            change(&mut shell),
            ModelChange {
                name: "openai/gpt-6-astra".to_string(),
                effort: Some(Effort::Named("xhigh".to_string())),
                picker: true,
                announced: true,
                next_turn: false,
            }
        );
        assert_eq!(
            shell.document(),
            said(notice::Notice::neutral(
                "",
                "Switched to openai/gpt-6-astra (effort: xhigh, speed: normal)"
            ))
        );
        assert!(shell.editor.is_empty(), "the composer kept an applied line");
        assert!(
            shell
                .hint()
                .ends_with("auto \u{b7} gpt-6-astra \u{b7} xhigh"),
            "{}",
            shell.hint()
        );
        // The effort is the session's: a model that does not list it hides it
        // on the footer and keeps it for the next one that does.
        shell.shell.use_model("fake/model-a");
        let _ = change(&mut shell);
        assert_eq!(
            shell.document(),
            said(notice::Notice::neutral(
                "",
                "Switched to fake/model-a (effort: xhigh, speed: normal)"
            ))
        );
        assert!(
            shell.hint().ends_with("auto \u{b7} model-a"),
            "{}",
            shell.hint()
        );
        // `auto` is every model's, and a model with no levels keeps the
        // session's effort (`selectModelFromPicker`).
        pasted(&mut shell, "/model xai/grok-4.7 auto");
        shell.route_bytes(b"\r");
        assert_eq!(change(&mut shell).effort, None);
    }

    #[test]
    fn the_menu_lists_the_rows_in_hand_and_walks_them_with_upstreams_keys() {
        let mut shell = shell(40, 120);
        shell.shell.apply(UiEvent::CatalogLoaded {
            provider: ProviderId::Gateway,
            entries: vec![
                entry("one", Some(400_000), &[]),
                CatalogEntry {
                    id: "anthropic/claude-opus-5".to_string(),
                    ..entry("two", Some(1_000_000), &[])
                },
            ],
        });
        let _ = shell.document();
        // With rows in hand the menu opens on them, and asks for nothing.
        shell.route_bytes(b"/model\r");
        assert!(
            shell.sent.try_recv().is_err(),
            "rows in hand were asked for"
        );
        let rows: Vec<String> = menu(&shell).iter().map(|row| visible_text(row)).collect();
        assert_eq!(
            rows,
            [
                "Models 2  [All]  Anthropic  Others",
                "",
                "  vendor/one               400K context",
                "  anthropic/claude-opus-5  1M context",
            ]
        );
        assert_eq!(
            shell
                .band_rows()
                .last()
                .map(|row| visible_text(row))
                .as_deref(),
            Some("\u{2191}\u{2193} navigate     tab provider     enter use     esc close")
        );
        // The caret stays in the composer: the menu is a view of it.
        assert_eq!(
            shell.cursor(),
            (shell.geometry.input_first, PROMPT_CELLS),
            "the menu took the caret"
        );

        // Down marks the second; Up wraps back past the first to it again.
        shell.route_bytes(b"\x1b[B");
        assert!(
            menu(&shell)[3].contains(PALETTE.subtitle()),
            "{:?}",
            menu(&shell)
        );
        shell.route_bytes(b"\x1b[A\x1b[A");
        assert!(
            menu(&shell)[3].contains(PALETTE.subtitle()),
            "{:?}",
            menu(&shell)
        );

        // Tab is the next vendor and re-solves the band for one row.
        shell.route_bytes(b"\t");
        let rows: Vec<String> = menu(&shell).iter().map(|row| visible_text(row)).collect();
        assert_eq!(rows[0], "Models 1  All  [Anthropic]  Others");
        assert_eq!(rows.len(), 3);
        // Shift-Tab goes back.
        shell.route_bytes(b"\x1b[Z");
        assert_eq!(
            visible_text(&menu(&shell)[0]),
            "Models 2  [All]  Anthropic  Others"
        );

        // Typing is the search, and opens no completion menu.
        shell.route_bytes(b"opus");
        assert!(shell.picker.is_none());
        let rows: Vec<String> = menu(&shell).iter().map(|row| visible_text(row)).collect();
        assert_eq!(rows[0], "Models 1  [All]  Anthropic  Others");

        // Enter uses the marked row, by its exact id, and the menu and its
        // search go. The row lists no levels, so the change is made at once
        // (`beginModelPickerOptions` for a model with none).
        shell.route_bytes(b"\r");
        let used = change(&mut shell);
        assert_eq!(
            (used.name.as_str(), used.picker, used.announced),
            ("anthropic/claude-opus-5", true, true)
        );
        assert!(shell.model_menu.is_none());
        assert!(shell.editor.is_empty(), "the search outlived the menu");
        assert!(menu(&shell).is_empty(), "the band kept the menu's rows");
    }

    #[test]
    fn escape_closes_the_menu_empties_the_search_and_brings_the_hint_row_back() {
        // `fx-model2-40x120-menu-esc`.
        let mut shell = shell(40, 120);
        shell.route_bytes(b"/model\r");
        let _ = shell.picks_up();
        shell.route_bytes(b"gro");
        shell.route_bytes(&[0x1b]);
        shell.settle_input(Instant::now() + Decoder::ESC_TIMEOUT);
        assert!(shell.model_menu.is_none(), "Escape left the menu up");
        assert!(shell.editor.is_empty(), "the search outlived the menu");
        assert_eq!(shell.geometry.panel, 0, "the band kept the menu's rows");
        assert_eq!(shell.hint(), IDLE_HINT, "the clear gesture was armed");
    }

    #[test]
    fn a_catalog_that_arrives_or_fails_is_what_the_open_menu_shows() {
        let mut shell = shell(40, 120);
        shell.route_bytes(b"/model\r");
        assert_eq!(shell.picks_up(), TurnWork::Catalog);
        // Enter on a menu still loading has nothing to use, and is spent.
        shell.route_bytes(b"\r");
        assert!(shell.sent.try_recv().is_err());
        assert!(shell.model_menu.is_some());

        shell.apply(UiEvent::CatalogUnread {
            provider: ProviderId::Gateway,
            retry: None,
        });
        let rows: Vec<String> = menu(&shell).iter().map(|row| visible_text(row)).collect();
        assert_eq!(rows, ["Models 0  [All]", "", "Unable to load models."]);

        // A catalog that arrives fills it, the note under it -- the fixture's
        // environment has no key, so there is none to name.
        shell.apply(UiEvent::CatalogLoaded {
            provider: ProviderId::Gateway,
            entries: vec![entry("one", None, &[])],
        });
        let rows: Vec<String> = menu(&shell).iter().map(|row| visible_text(row)).collect();
        assert_eq!(rows, ["Models 1  [All]", "", "  vendor/one"]);

        // An empty one says so.
        shell.apply(UiEvent::CatalogLoaded {
            provider: ProviderId::Gateway,
            entries: Vec::new(),
        });
        let rows: Vec<String> = menu(&shell).iter().map(|row| visible_text(row)).collect();
        assert_eq!(rows, ["Models 0  [All]", "", "No models available."]);
        assert!(
            shell.document().is_empty(),
            "the menu's answers went to the document"
        );
    }

    #[test]
    fn the_note_under_a_gateway_menu_names_the_key_it_was_asked_with() {
        let mut llmux = shell(40, 120);
        let mut gateway = shell(40, 120);
        gateway.shell.gateway_note = Some("Note: Gateway catalog is authenticated with an API key");
        gateway.route_bytes(b"/model\r");
        let _ = gateway.picks_up();
        gateway.apply(UiEvent::CatalogLoaded {
            provider: ProviderId::Gateway,
            entries: vec![entry("one", None, &[])],
        });
        let rows: Vec<String> = menu(&gateway).iter().map(|row| visible_text(row)).collect();
        assert_eq!(
            rows,
            [
                "Models 1  [All]",
                "",
                "  vendor/one",
                "",
                "Note: Gateway catalog is authenticated with an API key",
            ]
        );
        // A llmux catalog has no note.
        llmux.shell.gateway_note = Some("Note: Gateway catalog is authenticated with an API key");
        llmux.apply(UiEvent::CatalogLoaded {
            provider: ProviderId::Llmux,
            entries: vec![entry("one", None, &[])],
        });
        llmux.route_bytes(b"/model\r");
        assert_eq!(menu(&llmux).len(), 3, "{:?}", menu(&llmux));
    }

    #[test]
    fn a_model_id_the_line_shell_would_refuse_is_refused_here_too() {
        // What a model id may be is one question with one answer
        // (`provider::model::model_id_problem`, which is what
        // `ModelSelector::apply` asks), and this front end does not get its
        // own. The stake is not tidiness: the runtime writes a model change
        // into the session log, so an id refused everywhere else would be
        // recorded here and read back by every later resume of this session --
        // and a control character in it is a control character in a file xfx
        // replays.
        //
        // Called rather than typed: a control byte at the composer is a
        // keystroke, and the question here is about the argument a command
        // carries.
        let too_long = "m".repeat(201);
        for (argument, refusal) in [
            (
                "two words",
                "/model takes one model id, with no spaces in it",
            ),
            ("bad\u{7}id", "a model id cannot contain control characters"),
            (too_long.as_str(), "that model id is too long"),
        ] {
            let mut shell = shell(24, 80);
            let in_force = shell.shell.model.clone();
            shell.use_model(argument);

            assert_eq!(
                shell.document(),
                said(notice::Notice::error("model", refusal)),
                "the refusal is not the line shell's words"
            );
            assert!(
                shell.sent.try_recv().is_err(),
                "a model id xfx refuses was submitted anyway, and the runtime \
                 records what it is given"
            );
            assert_eq!(
                shell.shell.model, in_force,
                "the session is talking to a model it refused"
            );
        }
    }

    // -----------------------------------------------------------------------
    // `/model`'s inline column
    // -----------------------------------------------------------------------

    /// The column's rows, colours off and trailing blanks trimmed.
    fn column(shell: &Shell) -> Vec<String> {
        menu(shell)
            .iter()
            .map(|row| visible_text(row).trim_end().to_string())
            .collect()
    }

    #[test]
    fn a_space_after_model_opens_the_models_under_the_argument() {
        // `fx-model-40x120-space`: four rows from column nine, the first
        // marked, the band six rows high, the hint row the session's own.
        let mut shell = harness_shell();
        shell.route_bytes(b"/model ");
        assert_eq!(
            column(&shell),
            [
                "         openai/gpt-6-astra",
                "         anthropic/claude-opus-5",
                "         fake/model-a",
                "         xai/grok-4.7",
                "",
                ""
            ]
        );
        assert!(
            menu(&shell)[0].contains(PALETTE.subtitle()),
            "the first row is not marked: {:?}",
            menu(&shell)
        );
        assert_eq!(shell.geometry.panel, 6);
        assert!(
            shell.sent.try_recv().is_err(),
            "rows in hand were asked for"
        );
        // The caret stays in the composer: the column is a view of it.
        assert_eq!(shell.cursor().1, PROMPT_CELLS + 7);
        // Typing filters it (`fx-model-40x120-filter-gpt`).
        shell.route_bytes(b"GPT");
        assert_eq!(column(&shell)[0], "         openai/gpt-6-astra");
        assert_eq!(column(&shell)[1], "");
        // Nothing matching says so (`picker_presentation.zig:883-891`).
        shell.route_bytes(b"zz");
        assert_eq!(column(&shell)[0], "         no matching models");
        // And `/model` with no space is a command name, with no column.
        let mut bare = harness_shell();
        bare.route_bytes(b"/model");
        assert!(bare.column.active(bare.editor.text()).is_none());
    }

    #[test]
    fn the_column_asks_for_the_catalog_once_and_says_where_it_is() {
        let mut shell = shell(40, 120);
        shell.route_bytes(b"/model ");
        assert_eq!(shell.picks_up(), TurnWork::Catalog);
        assert_eq!(column(&shell)[0], "         loading models...");
        shell.route_bytes(b"x");
        assert!(shell.sent.try_recv().is_err(), "asked twice");
        shell.apply(UiEvent::CatalogUnread {
            provider: ProviderId::Gateway,
            retry: Some(CatalogRetry::Unreachable),
        });
        assert_eq!(column(&shell)[0], "         unable to load models");
        // The column does not ask again by itself; `/model` does, and the
        // menu says upstream's retryable sentence while it waits for nothing.
        shell.route_bytes(b"y");
        assert!(
            shell.sent.try_recv().is_err(),
            "the column retried on its own"
        );
    }

    #[test]
    fn a_full_queue_does_not_refuse_the_column_and_the_end_of_the_turn_asks_again() {
        // The column opens on a keystroke, and a keystroke is not a command:
        // no `queue full` on the hint row, `loading models...` in the column,
        // and the catalog asked for once there is room.
        let mut shell = shell(40, 120);
        let _started = turn_running(&mut shell, b"first\r");
        shell.route_bytes(b"second\r");
        let _ = shell.document();
        shell.route_bytes(b"/model ");
        assert_eq!(shell.notice, None, "the column's ask was refused out loud");
        assert_eq!(column(&shell)[0], "         loading models...");
        let _first = shell.picks_up();
        let _second = shell.picks_up();
        // The runtime gives the turn's place back as it ends it.
        shell.work.finished_one();
        shell.apply(UiEvent::TurnEnded { failure: None });
        assert_eq!(shell.picks_up(), TurnWork::Catalog);
    }

    #[test]
    fn enter_on_a_model_with_levels_opens_the_effort_column_and_the_footer_previews_it() {
        // `fx-model-40x120-gpt-enter`: the composer reads the id and a space,
        // `default` and the levels in the catalog's order from column 28, and
        // the footer names the pending model.
        let mut shell = harness_shell();
        shell.route_bytes(b"/model gpt\r");
        assert_eq!(shell.editor.text(), "/model openai/gpt-6-astra ");
        assert_eq!(
            column(&shell),
            [
                format!("{}default", " ".repeat(28)),
                format!("{}low", " ".repeat(28)),
                format!("{}medium", " ".repeat(28)),
                format!("{}high", " ".repeat(28)),
                format!("{}xhigh", " ".repeat(28)),
                String::new()
            ]
        );
        assert!(
            shell.controlled().is_none(),
            "Enter on a model with levels applied it"
        );
        assert!(
            shell.hint().ends_with("auto \u{b7} gpt-6-astra"),
            "{}",
            shell.hint()
        );
        // Down marks `low`, and the footer previews it.
        shell.route_bytes(b"\x1b[B");
        assert!(
            shell.hint().ends_with("auto \u{b7} gpt-6-astra \u{b7} low"),
            "{}",
            shell.hint()
        );
        // Enter makes the change with it -- no speed column yet, so this is
        // where it commits -- and empties the composer.
        shell.route_bytes(b"\r");
        assert_eq!(
            change(&mut shell),
            ModelChange {
                name: "openai/gpt-6-astra".to_string(),
                effort: Some(Effort::Named("low".to_string())),
                picker: true,
                announced: true,
                next_turn: false,
            }
        );
        assert_eq!(
            shell.document(),
            said(notice::Notice::neutral(
                "",
                "Switched to openai/gpt-6-astra (effort: low, speed: normal)"
            ))
        );
        assert!(shell.editor.is_empty());
        assert_eq!(shell.geometry.panel, 0, "the column outlived the change");
        assert!(
            shell.hint().ends_with("auto \u{b7} gpt-6-astra \u{b7} low"),
            "{}",
            shell.hint()
        );
    }

    #[test]
    fn a_level_typed_out_is_taken_and_one_the_model_lacks_is_nothing() {
        let mut shell = harness_shell();
        shell.route_bytes(b"/model gpt\r");
        shell.route_bytes(b"xh");
        assert_eq!(column(&shell)[0], format!("{}xhigh", " ".repeat(28)));
        shell.route_bytes(b"\x7f\x7f");
        assert_eq!(column(&shell).len(), 6);
        assert_eq!(shell.column.pending(), Some("openai/gpt-6-astra"));
        shell.route_bytes(b"max");
        assert_eq!(
            column(&shell)[0],
            format!("{}no matching effort", " ".repeat(28))
        );
        // Enter with nothing that matches is spent, and the column stays.
        shell.route_bytes(b"\r");
        assert!(shell.controlled().is_none());
        assert_eq!(shell.editor.text(), "/model openai/gpt-6-astra max");
    }

    #[test]
    fn left_steps_back_to_the_model_column_with_the_mark_on_the_model() {
        // `fx-model2-40x120-menu-enter-left`.
        let mut shell = harness_shell();
        shell.route_bytes(b"/model claude\r");
        assert_eq!(shell.editor.text(), "/model anthropic/claude-opus-5 ");
        shell.route_bytes(b"\x1b[D");
        assert_eq!(shell.editor.text(), "/model anthropic/claude-opus-5");
        assert_eq!(
            column(&shell),
            ["         anthropic/claude-opus-5", "", "", "", "", ""]
        );
        // Nothing is pending any more, so the footer previews nothing.
        assert!(!shell.hint().contains("opus"), "{}", shell.hint());
        // From the model column Left is the caret's.
        shell.route_bytes(b"\x1b[D");
        assert_eq!(shell.editor.text(), "/model anthropic/claude-opus-5");
        assert_eq!(shell.cursor().1, PROMPT_CELLS + 29);
    }

    #[test]
    fn a_typed_exact_id_and_a_space_commits_a_model_with_no_levels_and_the_rest_is_a_prompt() {
        // Trap 1 (`fx-model2-40x120-explicit-bad-effort`): typed key by key,
        // the space after an exact id is the column's, and for a model with no
        // levels it makes the change -- so `high` arrives in an empty composer
        // and goes as a prompt.
        let mut shell = harness_shell();
        shell.route_bytes(b"/model fake/model-a high\r");
        let made = change(&mut shell);
        assert_eq!((made.name.as_str(), made.picker), ("fake/model-a", true));
        assert_eq!(shell.picks_up(), TurnWork::Submit("high".to_string()));
        // A space that names nothing exactly is swallowed, as upstream's is.
        let mut other = harness_shell();
        other.route_bytes(b"/model fake ");
        assert_eq!(other.editor.text(), "/model fake");
        // A model with levels moves on to them.
        other.take_draft();
        other.amended(None);
        other.route_bytes(b"/model xai/grok-4.7 ");
        assert!(other.controlled().is_some());
        other.route_bytes(b"/model openai/gpt-6-astra ");
        assert_eq!(other.editor.text(), "/model openai/gpt-6-astra ");
        assert_eq!(other.column.pending(), Some("openai/gpt-6-astra"));
    }

    #[test]
    fn escape_puts_the_column_away_and_enter_is_the_router_form() {
        // `fx-model-fuzzy-40x120-opus`: with the column dismissed, Enter sends
        // the line, and `opus` is resolved -- here, against the rows in hand.
        let mut shell = harness_shell();
        shell.route_bytes(b"/model opus");
        shell.route_bytes(&[0x1b]);
        shell.settle_input(Instant::now() + Decoder::ESC_TIMEOUT);
        assert!(menu(&shell).is_empty(), "Escape left the column up");
        assert_eq!(shell.editor.text(), "/model opus", "Escape took the text");
        shell.route_bytes(b"\r");
        assert_eq!(change(&mut shell).name, "anthropic/claude-opus-5");
        assert_eq!(
            shell.document(),
            said(notice::Notice::neutral(
                "",
                "Switched to anthropic/claude-opus-5 (effort: default, speed: normal)"
            ))
        );
        // An id nothing lists goes the same way with the column up: it has no
        // row to give, so Enter is the composer's.
        let mut unknown = harness_shell();
        unknown.route_bytes(b"/model nonexistent-xyz\r");
        assert_eq!(change(&mut unknown).name, "nonexistent-xyz");
    }

    #[test]
    fn tab_writes_the_marked_row_into_the_composer() {
        let mut shell = harness_shell();
        shell.route_bytes(b"/model gr\t");
        assert_eq!(shell.editor.text(), "/model xai/grok-4.7");
        assert_eq!(column(&shell)[0], "         xai/grok-4.7");
        let mut efforts = harness_shell();
        efforts.route_bytes(b"/model gpt\r\x1b[B\x1b[B\t");
        assert_eq!(efforts.editor.text(), "/model openai/gpt-6-astra medium");
        assert_eq!(efforts.column.pending(), Some("openai/gpt-6-astra"));
    }

    #[test]
    fn a_paste_puts_the_column_back_to_the_models() {
        let mut shell = harness_shell();
        shell.route_bytes(b"/model gpt\r");
        assert!(shell.column.pending().is_some());
        pasted(&mut shell, "low");
        assert_eq!(shell.column.pending(), None, "a paste was taken for typing");
    }

    #[test]
    fn enter_on_a_menu_row_with_levels_goes_on_into_the_effort_column() {
        // `fx-model2-40x120-menu-enter`: `beginExactModelSelection`.
        let mut shell = harness_shell();
        shell.route_bytes(b"/model\r");
        shell.route_bytes(b"\x1b[B\r");
        assert!(shell.model_menu.is_none());
        assert_eq!(shell.editor.text(), "/model anthropic/claude-opus-5 ");
        assert_eq!(
            column(&shell),
            [
                format!("{}default", " ".repeat(33)),
                format!("{}low", " ".repeat(33)),
                format!("{}medium", " ".repeat(33)),
                format!("{}high", " ".repeat(33)),
                format!("{}max", " ".repeat(33)),
                String::new()
            ]
        );
        assert!(shell.controlled().is_none());
    }

    #[test]
    fn a_failed_catalog_says_upstreams_retry_sentence_and_the_next_model_asks_again() {
        // `retryableFailureText` (`model_menu_presentation.zig:435-443`), and
        // `retry /model.` made true.
        let mut shell = shell(40, 120);
        shell.route_bytes(b"/model\r");
        assert_eq!(shell.picks_up(), TurnWork::Catalog);
        shell.apply(UiEvent::CatalogUnread {
            provider: ProviderId::Gateway,
            retry: Some(CatalogRetry::Unreachable),
        });
        assert_eq!(
            visible_text(&menu(&shell)[2]),
            "Could not reach AI Gateway; retry /model."
        );
        shell.route_bytes(&[0x1b]);
        shell.settle_input(Instant::now() + Decoder::ESC_TIMEOUT);
        shell.route_bytes(b"/model\r");
        assert_eq!(
            shell.picks_up(),
            TurnWork::Catalog,
            "the retry was not asked for"
        );
    }

    #[test]
    fn an_interrupt_forgets_a_catalog_load_it_may_have_dropped() {
        let mut shell = shell(40, 120);
        let _started = turn_running(&mut shell, b"first\r");
        shell.route_bytes(b"/model ");
        let _first = shell.picks_up();
        assert_eq!(shell.picks_up(), TurnWork::Catalog);
        // Claim changed with F4: over a running turn, a Ctrl-C with a draft in
        // the composer takes the draft and nothing else (`app_input_runtime.zig:
        // 1675-1712`), so the turn is stopped by the second one.
        shell.route_bytes(&[0x03]);
        assert!(
            shell.controlled().is_none(),
            "the draft's Ctrl-C stopped the turn"
        );
        shell.route_bytes(&[0x03]);
        assert!(matches!(
            shell.controlled(),
            Some(TurnControl::Cancel { .. })
        ));
        // The runtime stops the turn and drops the load queued behind it,
        // giving both places back -- and sends nothing about the load.
        shell.work.finished_one();
        shell.work.finished_one();
        shell.route_bytes(b"/model ");
        assert_eq!(
            shell.picks_up(),
            TurnWork::Catalog,
            "a dropped load was never asked again"
        );
    }

    // -----------------------------------------------------------------------
    // `/clear`, `/new` and `/reset`
    // -----------------------------------------------------------------------

    /// The three names of the one reset, as typed.
    const RESETS: [&[u8]; 3] = [b"/clear\r", b"/new\r", b"/reset\r"];

    /// What a reset leaves owed to the document: the welcome line and the
    /// blank row under it, exactly as a session that has just started owes
    /// them -- and nothing else, because upstream's reset writes no notice.
    fn welcomed() -> Vec<Append> {
        let mut fresh = shell(24, 80);
        fresh.welcome();
        fresh.take_pending()
    }

    #[test]
    fn every_name_of_the_reset_reaches_the_runtime_because_the_conversation_lives_there() {
        for command in RESETS {
            let mut shell = shell(24, 80);
            shell.route_bytes(command);
            assert_eq!(
                shell.controlled(),
                Some(TurnControl::Reset { through: 0 }),
                "{}",
                String::from_utf8_lossy(command)
            );
            assert!(
                shell.sent.try_recv().is_err(),
                "a reset took a place in the queue: {}",
                String::from_utf8_lossy(command)
            );
        }
    }

    #[test]
    fn a_reset_forgets_what_it_owed_the_old_screen_and_owes_the_handoff() {
        // The rows the old screen was still owed and the text the pacer holds
        // belong to a document that is about to be handed to scrollback; written
        // after it they would land on the fresh screen under the welcome line.
        for command in RESETS {
            let mut shell = shell(24, 80);
            shell.apply(UiEvent::Delta(
                "an answer that goes to scrollback".to_string(),
            ));
            shell.shell.flush_paced();
            assert!(
                shell.shell.owes_document(),
                "nothing was owed to begin with"
            );
            shell.apply(UiEvent::Delta("and one still being released".to_string()));

            shell.route_bytes(command);
            assert_eq!(
                shell.shell.paced_backlog(),
                0,
                "the stream survived the screen it was measured against"
            );
            assert!(shell.owes_handoff(), "the screen was never handed off");
            // No notice: upstream's reset says nothing (`fx-sessions-…-new.txt`).
            assert_eq!(
                shell.take_pending(),
                welcomed(),
                "a row owed to the old screen survived, or the reset said something"
            );
            // The old document's rows are still on the screen until the handoff
            // lands, so the band is still placed under them.
            assert_eq!(shell.document_rows(), u32::MAX);
            assert!(shell.owes_handoff(), "asking took the handoff");

            shell.handed_off();
            assert!(!shell.owes_handoff());
            assert_eq!(
                shell.document_rows(),
                0,
                "the band was not floated back to the top of the blank screen"
            );

            // After the runtime has said the conversation is gone, the next
            // answer opens a row of its own under the welcome line rather than
            // continuing the line a cancelled answer left open.
            shell.apply(UiEvent::Fresh { through: 0 });
            shell.apply(UiEvent::Delta("the next answer".to_string()));
            shell.shell.flush_paced();
            assert_eq!(
                shell.take_pending(),
                vec![
                    Append {
                        scroll: 1,
                        rows: vec!["  the next answer".to_string()],
                        kinds: Vec::new(),
                    },
                    Append {
                        scroll: 1,
                        rows: vec![String::new()],
                        kinds: Vec::new(),
                    },
                ],
                "{}",
                String::from_utf8_lossy(command)
            );
        }
    }

    #[test]
    fn a_reset_mid_turn_cancels_the_turn_and_does_not_wait_behind_it() {
        // Upstream cancels and applies once the worker is idle
        // (`decideLiveSessionTransition`, `app_session_runtime.zig:137-170`);
        // a reset queued behind the turn would let the answer finish first.
        let mut shell = shell(24, 80);
        let _ = shell.take_title();
        let started = turn_running(&mut shell, b"ask something\r");
        let _turn = shell.picks_up();
        shell.apply(UiEvent::Delta("the start of an answer".to_string()));

        shell.route_bytes(b"/clear\r");
        assert_eq!(
            shell.controlled(),
            Some(TurnControl::Reset { through: 1 }),
            "the running turn was not reached, or the reset reached past it"
        );
        assert!(
            shell.sent.try_recv().is_err(),
            "the reset was queued behind the turn"
        );
        assert_eq!(shell.shell.notice, None, "the reset was refused");
        // No cancelled notice and no partial answer: the welcome alone.
        assert_eq!(shell.take_pending(), welcomed());
        shell.settle_band(started + Duration::from_millis(50));
        assert_eq!(
            shell.geometry.activity, None,
            "the activity row outlived the reset"
        );

        // A prompt typed straight away is taken -- the reset costs it no place
        // -- and goes out past the reset's count, into the new conversation.
        shell.route_bytes(b"straight away\r");
        assert_eq!(
            shell.shell.notice, None,
            "the prompt after a reset was refused"
        );
        assert_eq!(
            shell.picks_up(),
            TurnWork::Submit("straight away".to_string())
        );
        // Its card is on the fresh screen, under the welcome line.
        assert_eq!(shell.document(), card("straight away"));

        // What the cancelled turn still sends belongs to the conversation the
        // reset handed to scrollback.
        shell.apply(UiEvent::Delta("the rest of it".to_string()));
        shell.apply(tool_start(
            "call_1",
            "read_file",
            serde_json::json!({"path": "notes.txt"}),
        ));
        shell.apply(UiEvent::Notice(notice::Notice::warning("tool", "a stray")));
        shell.apply(UiEvent::Usage {
            input: Some(9_000),
            output: Some(40),
        });
        shell.apply(UiEvent::TurnEnded {
            failure: Some("xfx: interrupted".to_string()),
        });
        shell.shell.flush_paced();
        shell.settle_band(started + Duration::from_millis(100));
        assert!(
            shell.take_pending().is_empty(),
            "the cancelled turn wrote onto the fresh screen"
        );
        assert_eq!(shell.shell.context_used, None);
        assert_eq!(
            shell.take_title(),
            None,
            "the cancelled turn's conclusion titled the fresh session"
        );
        assert_eq!(shell.geometry.activity, None);

        // The fence, and what follows it is the new conversation's: the turn
        // the prompt typed straight away started.
        shell.apply(UiEvent::Fresh { through: 1 });
        shell.apply(UiEvent::TurnStarted);
        shell.apply(UiEvent::Delta("a fresh answer".to_string()));
        shell.shell.flush_paced();
        assert_eq!(
            shell.document(),
            vec!["  a fresh answer".to_string(), String::new()]
        );
        shell.apply(UiEvent::TurnEnded { failure: None });
        assert_eq!(
            shell.take_title().as_deref(),
            Some("straight away"),
            "the first turn of the fresh session did not title it"
        );
    }

    #[test]
    fn clear_new_and_reset_leave_exactly_the_same_session() {
        // One operation under three names (`app_commands.zig:667-680`): from
        // the same session, each sends the same messages, owes the same
        // screen and leaves the same band.
        let after = |command: &[u8]| {
            let mut shell = shell(24, 80);
            let _ = shell.take_title();
            turn_running(&mut shell, b"ask something\r");
            let _turn = shell.picks_up();
            shell.apply(UiEvent::Usage {
                input: Some(1_234),
                output: None,
            });
            shell.apply(UiEvent::Delta("half an answer".to_string()));
            shell.route_bytes(command);
            let controls: Vec<_> = std::iter::from_fn(|| shell.controlled()).collect();
            let sent: Vec<_> = std::iter::from_fn(|| shell.sent.try_recv().ok()).collect();
            (
                controls,
                sent,
                shell.take_pending(),
                shell.owes_handoff(),
                shell.take_title(),
                shell.shell.context_used,
                shell.band_rows(),
                shell.shell.notice,
            )
        };
        let clear = after(b"/clear\r");
        assert_eq!(after(b"/new\r"), clear, "/new is not /clear");
        assert_eq!(after(b"/reset\r"), clear, "/reset is not /clear");
        assert_eq!(clear.0, vec![TurnControl::Reset { through: 1 }]);
        assert!(clear.1.is_empty(), "{:?}", clear.1);
        assert!(clear.3, "no handoff was owed");
    }

    #[test]
    fn a_reset_with_a_prompt_waiting_drops_it_and_is_never_refused() {
        // A turn in flight and a prompt behind it are everything the queue
        // holds (`super::worker::WORK_LIMIT`). The reset takes no place in it,
        // so it is not refused the way a third prompt is, and the count it
        // quotes covers the waiting prompt too: that prompt belonged to the
        // conversation being dropped.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"first\r");
        let _first = shell.picks_up();
        shell.route_bytes(b"second\r");
        let _ = shell.document();

        shell.route_bytes(b"/new\r");
        assert_eq!(
            shell.controlled(),
            Some(TurnControl::Reset { through: 2 }),
            "the waiting prompt was not dropped with the turn"
        );
        assert_eq!(
            shell.shell.notice, None,
            "a reset was refused like a prompt"
        );
        assert_eq!(shell.take_pending(), welcomed());
    }

    #[test]
    fn a_question_the_cancelled_turn_asks_after_a_reset_is_refused_for_the_user() {
        // The prompter parks on the control channel (`super::approval`), and
        // the reset it would wake on may already have been consumed by the
        // turn. Dropped silently, the question could hold the runtime -- and
        // the fence the session is draining to -- for good.
        let mut shell = shell(24, 80);
        turn_running(&mut shell, b"edit the notes\r");
        let _turn = shell.picks_up();
        shell.route_bytes(b"/reset\r");
        assert_eq!(shell.controlled(), Some(TurnControl::Reset { through: 1 }));

        shell.apply(UiEvent::Approval(question(asked())));
        assert_eq!(
            shell.controlled(),
            Some(TurnControl::Answer {
                id: ApprovalId(1),
                answer: ApprovalAnswer::Deny,
                feedback: None,
            })
        );
        shell.apply(UiEvent::Question(a_batch()));
        assert_eq!(
            shell.controlled(),
            Some(TurnControl::QuestionCancelled { id: QuestionId(1) })
        );
        assert_eq!(shell.geometry.panel, 0, "a question took the band");
        assert_eq!(shell.screen_owner(), ScreenOwner::Primary);
        assert_eq!(shell.take_pending(), welcomed());
    }

    #[test]
    fn the_blank_row_a_document_ends_on_is_left_out_only_when_it_is_known() {
        // Upstream's history ends on the turn's summary and the next welcome
        // line follows it at once (`fx-reset-40x120-clear.full.txt`): the
        // blank row under the summary is spacing above the footer, not part of
        // the transcript it hands over. xfx leaves it out when its own record
        // of the document's tail is a record of the screen -- and carries it
        // when it is not, because erasing a row that turned out to hold text
        // would lose it, while carrying a blank one costs a blank row.
        let ends_blank = |shell: &mut Fixture| {
            shell.write_transcript("a row");
            shell.end_transcript_line();
            shell.end_transcript_line();
        };

        let mut landed = shell(24, 80);
        ends_blank(&mut landed);
        let _ = landed.take_pending();
        landed.route_bytes(b"/clear\r");
        assert_eq!(landed.handoff_spacing(), 1);
        // A second reset before the handoff landed has no document of its
        // own: it hands off the first one's, spacing and all.
        landed.route_bytes(b"/new\r");
        assert_eq!(landed.handoff_spacing(), 1);

        let mut text_last = shell(24, 80);
        text_last.write_transcript("a row");
        text_last.end_transcript_line();
        let _ = text_last.take_pending();
        text_last.route_bytes(b"/clear\r");
        assert_eq!(text_last.handoff_spacing(), 0, "a row of text was left out");

        let mut unwritten = shell(24, 80);
        ends_blank(&mut unwritten);
        unwritten.route_bytes(b"/clear\r");
        assert_eq!(
            unwritten.handoff_spacing(),
            0,
            "rows that never reached the screen were taken as its tail"
        );

        let mut reflowed = shell(24, 80);
        ends_blank(&mut reflowed);
        let _ = reflowed.take_pending();
        assert!(matches!(reflowed.resize(30, 100), Resize::Repaint(_)));
        reflowed.route_bytes(b"/clear\r");
        assert_eq!(
            reflowed.handoff_spacing(),
            0,
            "a tail the terminal reflowed was taken as known"
        );
        // The same when the reflow comes after the reset, while its handoff
        // is still owed: the spacing was measured against rows that moved.
        let mut deferred = shell(24, 80);
        ends_blank(&mut deferred);
        let _ = deferred.take_pending();
        deferred.route_bytes(b"/clear\r");
        assert_eq!(deferred.handoff_spacing(), 1);
        assert!(matches!(deferred.resize(30, 100), Resize::Repaint(_)));
        assert_eq!(
            deferred.handoff_spacing(),
            0,
            "a reflow under an owed handoff kept its spacing"
        );
        assert!(deferred.owes_handoff(), "the reflow dropped the handoff");
        let mut damaged = shell(24, 80);
        ends_blank(&mut damaged);
        let _ = damaged.take_pending();
        damaged.route_bytes(b"/new\r");
        damaged.external_damage();
        assert_eq!(damaged.handoff_spacing(), 0);
        assert!(damaged.owes_handoff());

        // A handoff leaves a screen this session wrote every row of.
        reflowed.handed_off();
        ends_blank(&mut reflowed);
        let _ = reflowed.take_pending();
        reflowed.route_bytes(b"/reset\r");
        assert_eq!(reflowed.handoff_spacing(), 1);
    }

    #[test]
    fn a_reset_takes_an_open_tool_group_with_it() {
        // A turn whose call is still running has its group open as the
        // document's last block. A write that merely closes the group keeps
        // the unsettled call to draw it again under whatever closed it
        // (`ToolGroup::unsettled`); a reset must not, because that call's
        // result is drained and the call would sit in the next session's
        // first group for good.
        for command in RESETS {
            let mut shell = shell(24, 80);
            turn_running(&mut shell, b"read the old notes\r");
            let _turn = shell.picks_up();
            shell.apply(tool_start(
                "call_1",
                "read_file",
                serde_json::json!({"path": "old-notes.txt"}),
            ));
            assert!(
                shell
                    .document()
                    .iter()
                    .any(|row| row.contains("old-notes.txt")),
                "the group was never open, so this proves nothing"
            );

            shell.route_bytes(command);
            assert_eq!(
                shell.take_pending(),
                welcomed(),
                "the open group's block reached the fresh screen"
            );

            // The new session's first turn, and its first call: the group is
            // that call alone.
            shell.apply(UiEvent::Fresh { through: 1 });
            shell.apply(UiEvent::TurnStarted);
            shell.apply(tool_start(
                "call_2",
                "read_file",
                serde_json::json!({"path": "new-notes.txt"}),
            ));
            let rows = shell.document();
            assert!(
                rows.iter().any(|row| row.contains("new-notes.txt")),
                "{}: {rows:?}",
                String::from_utf8_lossy(command)
            );
            assert!(
                !rows.iter().any(|row| row.contains("old-notes.txt")),
                "{}: the old turn's call trailed into the new session: {rows:?}",
                String::from_utf8_lossy(command)
            );
        }
    }

    #[test]
    fn only_the_last_resets_fence_ends_the_draining() {
        // Two resets with a prompt between them: the second drops that prompt,
        // so its turn -- if it started before the second reset reached it --
        // reports after the first fence and belongs to a screen the second
        // reset handed to scrollback.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"/new\r");
        assert_eq!(shell.controlled(), Some(TurnControl::Reset { through: 0 }));
        shell.route_bytes(b"in between\r");
        let _taken = shell.picks_up();
        shell.route_bytes(b"/new\r");
        assert_eq!(shell.controlled(), Some(TurnControl::Reset { through: 1 }));
        let _ = shell.document();

        shell.apply(UiEvent::Fresh { through: 0 });
        shell.apply(UiEvent::TurnStarted);
        shell.apply(UiEvent::Delta("dropped by the second reset".to_string()));
        shell.apply(UiEvent::TurnEnded { failure: None });
        shell.shell.flush_paced();
        assert!(
            shell.take_pending().is_empty(),
            "the first fence let the dropped prompt's turn onto the screen"
        );

        shell.apply(UiEvent::Fresh { through: 1 });
        shell.apply(UiEvent::Delta("an answer".to_string()));
        shell.shell.flush_paced();
        assert_eq!(
            shell.document(),
            vec!["  an answer".to_string(), String::new()]
        );
    }

    #[test]
    fn a_submitted_line_that_is_only_whitespace_is_consumed_and_sent_nowhere() {
        let mut shell = shell(24, 80);
        shell.route_bytes(b"   \r");
        assert!(
            shell.editor.is_empty(),
            "a blank line stayed in the composer"
        );
        assert!(
            shell.document().is_empty(),
            "a blank line reached the document"
        );
        assert!(
            shell.sent.try_recv().is_err(),
            "a blank line reached the runtime"
        );
    }

    #[test]
    fn a_submission_to_a_runtime_that_is_gone_says_that_instead_of_busy() {
        let mut shell = shell(24, 80);
        // The runtime end, dropped: the thread is gone and its channel with it.
        let (dead, work_rx, control_rx) = WorkHandle::detached();
        drop(work_rx);
        drop(control_rx);
        shell.shell.work = dead;

        shell.route_bytes(b"anything");
        shell.route_bytes(&[0x0d]);

        assert_eq!(
            shell.document(),
            said(notice::Notice::error("system", GONE_NOTICE))
        );
    }

    #[test]
    fn the_answer_arrives_as_deltas_and_lands_in_the_document() {
        let mut shell = shell(24, 80);
        shell.apply(UiEvent::TurnStarted);
        shell.apply(UiEvent::Delta("MARKER-TURN-".to_string()));
        shell.apply(UiEvent::Delta("ONE".to_string()));
        shell.apply(UiEvent::TurnEnded { failure: None });

        // The presenter writes a line once it has the whole of it, so the row
        // the document keeps is the whole answer rather than its first
        // fragment -- in the answer's two-column gutter, its block ended on a
        // blank row, and the clean end's summary and its own blank row under
        // that. No prompt was sent, so in is 0; out is the estimate of what
        // streamed, counted as it arrived rather than as the pacer released
        // it (`telemetry.zig:116-120`) -- the turn ends with the answer still
        // held, and the summary still counts it.
        let document = shell.released();
        assert_eq!(
            document,
            ["  MARKER-TURN-ONE", "", "  0s (\u{2191}0 \u{2193}4)", ""],
            "{document:?}"
        );
    }

    #[test]
    fn work_that_started_no_turn_ends_without_a_summary() {
        // `/model`, `/setup` and a catalog listing conclude with the same event
        // a turn does, and fx writes no summary under a local command.
        let mut shell = shell(24, 80);
        shell.apply(UiEvent::Notice(super::super::notice::Notice::neutral(
            "model", "other",
        )));
        shell.apply(UiEvent::TurnEnded { failure: None });
        let document = shell.released();
        assert!(
            !document.iter().any(|row| row.trim_start().ends_with("0s")),
            "{document:?}"
        );
    }

    #[test]
    fn every_line_of_a_multi_line_card_is_bold() {
        let mut fixture = shell(24, 80);
        fixture.echo("first\nsecond");
        let document = fixture.released();
        let bold: Vec<&String> = document.iter().filter(|row| row.contains(BOLD)).collect();
        assert_eq!(bold.len(), 2, "{document:?}");
    }

    #[test]
    fn an_answer_is_released_over_several_frames_rather_than_dumped() {
        // What the pacer is for. A provider sends a burst; a UI that appended
        // it whole shows the answer as a jump, and this one shows it as a
        // stream -- `pacer::MIN_CPS` at the slowest, so a fragment this size
        // takes a tenth of a second and not one frame.
        //
        // In paragraphs, because the presenter writes a line once it has seen
        // what comes after it (`Shell::write_answer`; a prose line waits for
        // the next one, which may make it a setext heading): each paragraph
        // here -- its one-letter line and the blank line that ends it -- is
        // exactly what one tick of the floor rate pays for, so the first tick
        // shows the first paragraph and nothing after it.
        let mut shell = shell(24, 80);
        let start = Instant::now();
        let per_tick = usize::try_from(MIN_CPS).expect("a rate") * 8 / 1000;
        let lines: Vec<String> = (b'a'..b'u')
            .map(|letter| char::from(letter).to_string().repeat(per_tick - 2))
            .collect();
        let answer: String = lines.iter().map(|line| format!("{line}\n\n")).collect();
        shell.apply(UiEvent::Delta(answer));
        // The clock's first reading. Nothing is owed for it, which is the
        // difference between a pacer and a queue.
        shell.settle_band(start);
        assert!(
            shell.document().is_empty(),
            "the whole delta was appended the instant it arrived"
        );
        let first = shell.paced(start, TICK_MILLIS);
        assert_eq!(
            first,
            vec![format!("  {}", lines[0])],
            "one tick released more than the floor rate pays for"
        );
        // and the rest of it arrives, over the ticks that pay for it
        let rows = shell.paced(start + Duration::from_millis(TICK_MILLIS), 200);
        assert_eq!(rows.last(), Some(&format!("  {}", lines[19])));
    }

    #[test]
    fn a_turn_that_ended_drains_what_is_left_of_it_faster() {
        // The wiring claim for `pacer::DRAIN_TARGET`: `TurnEnded` reaches the
        // pacer, so what is left of a finished answer is aimed at a fifth of a
        // second instead of at a second and a half. Stated as the contrast
        // between two identical sessions rather than as "empty by then",
        // because the rate is recomputed against a backlog that is shrinking
        // as it is spent -- the target is what it aims at, and the floor is
        // what finishes it.
        let answer = "y".repeat(1200);
        let start = Instant::now();

        let mut running = shell(24, 80);
        running.apply(UiEvent::Delta(answer.clone()));
        running.settle_band(start);
        running.paced(start, 200);

        let mut ended = shell(24, 80);
        ended.apply(UiEvent::Delta(answer.clone()));
        ended.apply(UiEvent::TurnEnded { failure: None });
        ended.settle_band(start);
        ended.paced(start, 200);

        assert!(
            ended.shell.paced_backlog() < running.shell.paced_backlog(),
            "a turn that ended was still paced at reading speed: {} left against {}",
            ended.shell.paced_backlog(),
            running.shell.paced_backlog()
        );
        // and the ceiling still holds over the drain, whatever the deadline
        // asks for: two hundred milliseconds buy `MAX_CPS` fifths of a second
        // and not the whole backlog.
        let ceiling = usize::try_from(MAX_CPS).expect("a rate") * 200 / 1000;
        assert!(
            ended.shell.paced_backlog() >= answer.len().saturating_sub(ceiling),
            "the drain outran the ceiling: {} left of {}",
            ended.shell.paced_backlog(),
            answer.len()
        );
    }

    #[test]
    fn a_tool_that_refused_says_so_where_the_user_can_read_it() {
        // A denial nobody can see is the same as no denial at all, whatever
        // the reason for it: a rule in the configuration, a screen too small to
        // ask on, or a `3` typed at the panel. Said as upstream's tool group
        // says it -- a row and a count (`fx-approvals-cc-40x120-after-cc`) --
        // and a refusal the tool made on its own as a `Failed` row with its
        // reason.
        let mut shell = shell(24, 80);
        shell.apply(tool_start(
            "c1",
            "write_file",
            serde_json::json!({"path": "c.txt", "content": "x"}),
        ));
        shell.apply(tool_ended(
            "c1",
            "write_file",
            false,
            "write_file was not permitted: no approval channel",
            CallOutcome::Denied,
        ));
        shell.apply(tool_start(
            "c2",
            "read_file",
            serde_json::json!({"path": "gone.txt"}),
        ));
        shell.apply(tool_result(
            "c2",
            "read_file",
            false,
            "no such path: `gone.txt`",
        ));

        assert_eq!(
            shell.written(),
            vec![
                "\u{25cf} 2 tool calls \u{b7} 1 read \u{b7} 1 write \u{b7} 1 failed \u{b7} 1 denied"
                    .to_string(),
                "\u{251c} Denied file".to_string(),
                "\u{2514} Failed gone.txt: no such path: `gone.txt`".to_string(),
                String::new(),
            ]
        );
    }

    #[test]
    fn a_tool_group_spans_the_turns_steps_and_is_closed_by_its_text() {
        // Upstream's `tools-auto` shape: a sentence, calls across two steps in
        // one group, then the answer (`fx-tools-auto-40x120-t10`). The group
        // is not broken by the step boundary -- only by text.
        let mut shell = shell(24, 80);
        shell.apply(UiEvent::Delta(
            "I will look at the files first.\n".to_string(),
        ));
        shell.apply(tool_start(
            "1",
            "read_file",
            serde_json::json!({"path": "a.txt"}),
        ));
        shell.apply(tool_result("1", "read_file", true, ""));
        shell.apply(tool_start(
            "2",
            "glob_files",
            serde_json::json!({"pattern": "**/*.rs"}),
        ));
        shell.apply(tool_result("2", "glob_files", true, ""));
        shell.apply(tool_start(
            "3",
            "terminal",
            serde_json::json!({"action": "exec", "command": "ls"}),
        ));
        shell.apply(tool_ended(
            "3",
            "terminal",
            true,
            "",
            CallOutcome::Exited(0),
        ));
        shell.apply(UiEvent::Delta("All done.".to_string()));
        shell.apply(UiEvent::TurnEnded { failure: None });
        shell.flush_paced();

        let written = shell.written();
        assert_eq!(
            written[..8],
            [
                "  I will look at the files first.",
                "",
                "\u{25cf} 3 tool calls \u{b7} 1 read \u{b7} 1 list \u{b7} 1 command",
                "\u{251c} Read a.txt",
                "\u{251c} Matched **/*.rs",
                "\u{2514} Ran ls",
                "",
                "  All done.",
            ],
            "{written:#?}"
        );
    }

    #[test]
    fn a_running_command_is_the_groups_last_row_until_its_result_rewrites_it() {
        // `└ Running <command>` under the group while it runs
        // (`fx-tools-single-40x120-s1`), and the same row -- not a new one --
        // once it has: the block is rewritten where it stands.
        let mut shell = shell(24, 80);
        shell.apply(tool_start(
            "1",
            "terminal",
            serde_json::json!({"action": "exec", "command": "sleep 4"}),
        ));
        assert_eq!(
            shell.written(),
            [
                "\u{25cf} 1 tool call \u{b7} 1 command",
                "\u{2514} Running sleep 4",
                ""
            ]
        );

        shell.apply(tool_ended(
            "1",
            "terminal",
            true,
            "",
            CallOutcome::Exited(3),
        ));
        let appends = shell.take_pending();
        assert_eq!(appends.len(), 1, "{appends:?}");
        assert_eq!(
            appends[0].scroll, 0,
            "the settled row was added rather than rewritten"
        );
        assert_eq!(
            appends[0]
                .rows
                .iter()
                .map(|row| colourless(row))
                .collect::<Vec<_>>(),
            [
                "\u{25cf} 1 tool call \u{b7} 1 command \u{b7} 1 failed",
                "\u{2514} Exited 3 sleep 4",
                ""
            ]
        );
    }

    #[test]
    fn a_file_change_has_no_row_while_its_approval_is_pending() {
        // `fx-approvals-40x120-write-ask`: the change being reviewed is not in
        // the group until it is decided, and then it is, with what it changed.
        let mut shell = shell(24, 80);
        shell.apply(tool_start(
            "1",
            "edit_file",
            serde_json::json!({"path": "a.txt", "old_string": "a", "new_string": "b"}),
        ));
        assert!(shell.written().is_empty(), "a pending change was drawn");
        shell.apply(UiEvent::ToolResult {
            call_id: "1".to_string(),
            tool: "edit_file".to_string(),
            ok: true,
            detail: "Edited a.txt".to_string(),
            outcome: CallOutcome::Finished,
            changes: Some(LineChanges {
                added: 1,
                removed: 1,
            }),
        });
        assert_eq!(
            shell.written(),
            [
                "\u{25cf} 1 tool call \u{b7} 1 edit",
                "\u{2514} Edited a.txt +1 / -1",
                ""
            ]
        );
    }

    #[test]
    fn stopping_the_turn_at_an_approval_cancels_the_call_it_was_asking_about() {
        // `fx-approvals-cc-40x120-after-esc`: Esc at a question stops the
        // turn (`app_input_runtime.zig:3139-3143`), and the call the panel was
        // asking about never runs, so the row says `Cancelled`, the header
        // counts it, and the call gets its own line under the group -- at the
        // keystroke. That line is the turn's too: no second `■ Cancelled`
        // follows it (`append_turn_cancellation`, `transcript/runtime.zig:
        // 5378-5383`). Claim changed: an interrupt notice used to follow.
        let mut shell = shell(24, 80);
        let started = turn_running(&mut shell, b"edit the notes\r");
        shell.apply(tool_start(
            "1",
            "edit_file",
            serde_json::json!({"path": "notes.txt", "old_string": "a", "new_string": "b"}),
        ));
        shell.apply(UiEvent::Approval(question(asked())));
        shell.settle_band(started);
        delivered(&mut shell);
        let _ = shell.written();

        // A lone Escape is a key only once nothing follows it.
        shell.route_bytes(&[0x1b]);
        shell.settle_input(Instant::now() + Duration::from_millis(100));
        let written = shell.written();
        assert_eq!(
            written[..5],
            [
                "\u{25cf} 1 tool call \u{b7} 1 edit \u{b7} 1 cancelled",
                "\u{2514} Cancelled file",
                "",
                "\u{25a0} Cancelled file \u{b7} What can xfx do differently?",
                "",
            ],
            "{written:#?}"
        );
        assert!(
            !written.iter().any(|row| row.starts_with(INTERRUPTED_ROW)),
            "a second cancelled line followed the call's own: {written:#?}"
        );

        // The panel answers the stop by refusing the call, and the runtime
        // reports that refusal next: the same stop in the tool's words, about
        // a row the screen has already settled, so it is not drawn again.
        shell.apply(tool_ended(
            "1",
            "edit_file",
            false,
            "edit_file was not permitted: the user declined",
            CallOutcome::Denied,
        ));
        shell.apply(UiEvent::TurnEnded {
            failure: Some("the turn was cancelled".to_string()),
        });
        let after = shell.written();
        assert!(
            !after.iter().any(|row| row.contains("Denied")),
            "{after:#?}"
        );
    }

    #[test]
    fn ctrl_c_at_an_approval_leaves_the_calls_row_to_its_refusal() {
        // Ctrl-C at a question refuses this call and the turn goes on
        // (`src/ui/input/runtime.zig:61`, `3 => .deny`), so nothing is stopped
        // at the keystroke: no `Cancelled` row and no `■` line. The refusal
        // the runtime reports is what settles the row, as `Denied`, in the
        // same group the turn's next call is counted in.
        let mut shell = shell(24, 80);
        let started = turn_running(&mut shell, b"edit the notes\r");
        shell.apply(tool_start(
            "1",
            "edit_file",
            serde_json::json!({"path": "notes.txt", "old_string": "a", "new_string": "b"}),
        ));
        shell.apply(UiEvent::Approval(question(asked())));
        shell.settle_band(started);
        delivered(&mut shell);
        let _ = shell.written();

        shell.route_bytes(&[0x03]);
        let written = shell.written();
        assert!(
            !written
                .iter()
                .any(|row| row.contains("Cancelled") || row.starts_with('\u{25a0}')),
            "{written:#?}"
        );

        shell.apply(tool_ended(
            "1",
            "edit_file",
            false,
            "edit_file was not permitted: the user declined",
            CallOutcome::Denied,
        ));
        shell.apply(tool_start(
            "2",
            "terminal",
            serde_json::json!({"action": "exec", "command": "echo next"}),
        ));
        assert_eq!(
            shell.written(),
            [
                "\u{25cf} 2 tool calls \u{b7} 1 edit \u{b7} 1 command \u{b7} 1 denied",
                "\u{251c} Denied file",
                "\u{2514} Running echo next",
                "",
            ]
        );
    }

    #[test]
    fn stopping_the_turn_while_a_command_runs_leaves_its_row_to_its_result() {
        // A running executor holds the runtime's thread until it returns, so
        // the stop is read after the command has finished: a `Cancelled` row at
        // the keystroke would claim a stop the command never had. The sentence
        // that answers the key goes under the group, and the command's real
        // result settles its row above it when it comes. Claim changed: the
        // answer to the key is the turn's `■ Cancelled` line, written once the
        // turn has ended -- after the command's real row -- rather than an
        // interrupt notice at the keystroke.
        let mut shell = shell(24, 80);
        let _ = turn_running(&mut shell, b"run it\r");
        shell.apply(tool_start(
            "1",
            "terminal",
            serde_json::json!({"action": "exec", "command": "sleep 30"}),
        ));
        let _ = shell.written();

        shell.route_bytes(&[0x03]);
        shell.apply(tool_ended(
            "1",
            "terminal",
            true,
            "",
            CallOutcome::Exited(0),
        ));
        shell.apply(UiEvent::TurnEnded {
            failure: Some("the turn was cancelled".to_string()),
        });
        let written = shell.written();
        assert!(
            !written
                .iter()
                .any(|row| row.contains("Cancelled sleep") || row.contains("cancelled")),
            "a stop the command never had was drawn: {written:#?}"
        );
        let notice = written
            .iter()
            .position(|row| row.starts_with(INTERRUPTED_ROW))
            .expect("the interrupt notice");
        assert_eq!(
            written[notice - 3..notice],
            [
                "\u{25cf} 1 tool call \u{b7} 1 command",
                "\u{2514} Ran sleep 30",
                ""
            ],
            "{written:#?}"
        );
    }

    #[test]
    fn a_call_its_turn_ended_under_is_shown_cancelled() {
        // The runtime sends a result for every call that ran to its end; a
        // turn that concludes without one stopped it.
        let mut shell = shell(24, 80);
        shell.apply(tool_start(
            "1",
            "write_file",
            serde_json::json!({"path": "c.txt", "content": "x"}),
        ));
        shell.apply(UiEvent::TurnEnded {
            failure: Some("the turn was cancelled".to_string()),
        });
        let written = shell.written();
        assert_eq!(
            written,
            [
                "\u{25cf} 1 tool call \u{b7} 1 write \u{b7} 1 cancelled",
                "\u{2514} Cancelled file",
                "",
                "\u{25a0} Cancelled file \u{b7} What can xfx do differently?",
                "",
                "\u{2717} system: the turn was cancelled",
                "",
            ]
        );
    }

    #[test]
    fn a_line_written_while_a_call_runs_stays_under_the_group_its_result_updates() {
        // One group per turn, broken by text and nothing else
        // (`tool_presentation.zig:667-708`): a notice written while a call
        // runs lands under the group, and the call's result settles its row
        // above the notice rather than starting a second group under it.
        let mut shell = shell(24, 80);
        shell.apply(tool_start(
            "1",
            "terminal",
            serde_json::json!({"action": "exec", "command": "make"}),
        ));
        shell.apply(UiEvent::Notice(notice::Notice::neutral(
            "",
            "something worth saying",
        )));
        shell.apply(tool_ended(
            "1",
            "terminal",
            true,
            "",
            CallOutcome::Exited(0),
        ));
        assert_eq!(
            shell.written(),
            [
                "\u{25cf} 1 tool call \u{b7} 1 command",
                "\u{2514} Ran make",
                "",
                "* something worth saying",
                "",
            ]
        );
    }

    #[test]
    fn what_the_user_said_at_an_approval_does_not_split_the_group() {
        // `fx-approvals-40x120-cmd2`: the amendment is read back under the
        // group, and the next call of the same turn is counted in the same
        // header and drawn above it. Text is what breaks a group.
        let mut shell = shell(24, 80);
        shell.apply(tool_start(
            "1",
            "terminal",
            serde_json::json!({"action": "exec", "command": "echo hi"}),
        ));
        shell.apply(tool_ended(
            "1",
            "terminal",
            true,
            "",
            CallOutcome::Exited(0),
        ));
        shell.apply(UiEvent::ToolFeedback {
            call_id: "1".to_string(),
            text: "use capitals".to_string(),
        });
        shell.apply(tool_start(
            "2",
            "terminal",
            serde_json::json!({"action": "exec", "command": "echo again"}),
        ));
        assert_eq!(
            shell.written(),
            [
                "\u{25cf} 2 tool calls \u{b7} 2 commands",
                "\u{251c} Ran echo hi",
                "\u{2514} Running echo again",
                "",
                &format!("{AMENDMENT_PREFIX} use capitals"),
            ]
        );
        // And the answer closes it, under the line.
        shell.apply(tool_ended(
            "2",
            "terminal",
            true,
            "",
            CallOutcome::Exited(0),
        ));
        shell.apply(UiEvent::Delta("Done.\n\n".to_string()));
        shell.flush_paced();
        let written = shell.written();
        assert_eq!(
            written[..7],
            [
                "\u{25cf} 2 tool calls \u{b7} 2 commands",
                "\u{251c} Ran echo hi",
                "\u{2514} Ran echo again",
                "",
                &format!("{AMENDMENT_PREFIX} use capitals"),
                "  Done.",
                "",
            ],
            "{written:#?}"
        );
    }

    #[test]
    fn a_question_the_model_asked_leaves_no_row_in_the_group() {
        // `statusNamesAsk`: the question surface answers it, not a tool row.
        let mut shell = shell(24, 80);
        shell.apply(tool_start(
            "q",
            "ask_user_question",
            serde_json::json!({"questions": []}),
        ));
        shell.apply(tool_result("q", "ask_user_question", true, "answered"));
        assert!(shell.written().is_empty());
    }

    #[test]
    fn two_notices_in_a_row_share_the_one_blank_row_between_them() {
        // `Transcript::end_line` answers a request to end a line that is
        // already ended with a blank row of its own -- which is right for two
        // breaks in an answer and wrong for two notices. A notice's blank row
        // above is written only when the document does not already end on one
        // (`Transcript::ends_blank`), so two notices in a row are separated by
        // exactly one, as upstream's are
        // (`fx-basic-40x120-version-trailing-space.txt`) -- not by the two each
        // would bring on its own.
        let mut shell = shell(24, 80);
        shell.apply(UiEvent::Notice(notice::Notice::neutral("", "first")));
        shell.apply(UiEvent::Notice(notice::Notice::neutral("", "second")));

        assert_eq!(shell.document(), vec!["* first", "", "* second", ""]);
    }

    #[test]
    fn a_notice_that_lands_mid_answer_gets_a_row_of_its_own() {
        let mut shell = shell(24, 80);
        shell.apply(UiEvent::Delta(
            "a whole paragraph\n\nhalf a sentence".to_string(),
        ));
        shell.apply(UiEvent::Notice(notice::Notice::warning(
            "tool",
            "read_file ok",
        )));

        // The rows the answer has written end their block on its blank row,
        // which is the notice's blank row above as well. The half sentence
        // is not one of them: the presenter is holding it for its end, and it
        // goes on under the notice -- upstream's order
        // (`fx-notice-midstream-40x120-end.full`) -- rather than being cut off
        // at the notice and finished as a paragraph of its own.
        assert_eq!(
            shell.released(),
            vec![
                "  a whole paragraph",
                "",
                "! tool: read_file ok",
                "",
                "  half a sentence",
                ""
            ],
            "a notice was written into the middle of the answer's row"
        );
    }

    #[test]
    fn a_notice_waits_for_the_answer_it_arrived_behind() {
        // The ordering the pacer makes possible to get wrong. A tool notice is
        // xfx's own text and does not go through the queue, so a notice written
        // the moment it arrives would overtake the answer it belongs after and
        // land in the middle of a sentence the user is still reading. It is
        // held at the position the stream had reached instead.
        let mut shell = shell(24, 80);
        let start = Instant::now();
        // A whole paragraph, so the presenter writes it as soon as the pacer
        // has released its end: text the presenter is still holding for what
        // comes next -- an unfinished line, or a finished one that a setext
        // underline could still turn into a heading -- goes on under whatever
        // notice arrives meanwhile, and could not land early.
        shell.apply(UiEvent::Delta("the first half\n\n".to_string()));
        shell.apply(UiEvent::Notice(notice::Notice::warning(
            "tool",
            "read_file ok",
        )));
        // A whole paragraph -- its line and the blank line that ends it -- so
        // the presenter writes it as soon as the pacer has released that end
        // rather than at the answer's close.
        shell.apply(UiEvent::Delta("and the second\n\n".to_string()));
        shell.settle_band(start);
        assert!(
            shell.document().is_empty(),
            "the notice was written before any of the answer was"
        );

        let rows = shell.paced(start, 400);
        let notice = rows
            .iter()
            .position(|row| row == "! tool: read_file ok")
            .expect("the notice");
        // The notice ends the answer's block where the stream had reached, so
        // what is above it is the first half and the blank row that block
        // ends on, and nothing else.
        assert_eq!(
            rows[..notice],
            ["  the first half", ""],
            "the notice landed before the text it came after: {rows:?}"
        );
        assert_eq!(
            rows.last().map(String::as_str),
            Some("  and the second"),
            "the text after the notice was lost or joined to it: {rows:?}"
        );
    }

    #[test]
    fn a_line_of_xfx_s_own_is_written_at_once_while_nothing_is_streaming() {
        // The other half of the rule, and the common case: with an empty queue
        // the position a mark belongs at is *now*, and making a refusal or an
        // echo wait for a clock tick would put the pacer's delay on the
        // keyboard.
        let mut shell = shell(24, 80);
        shell.apply(UiEvent::Notice(notice::Notice::neutral("", "said at once")));
        assert_eq!(shell.document(), vec!["* said at once", ""]);
    }

    #[test]
    fn a_turn_that_failed_says_why_in_the_document() {
        let mut shell = shell(24, 80);
        shell.apply(UiEvent::Delta("part of an answer".to_string()));
        shell.apply(UiEvent::TurnEnded {
            failure: Some("xfx: the turn was cancelled".to_string()),
        });

        // An error under `system`, upstream's topic for a request that failed
        // (`app_process_runtime.zig:58-63`), with the line shell's `xfx: `
        // taken off: the glyph says whose sentence it is.
        assert_eq!(
            shell.released(),
            vec![
                "  part of an answer",
                "",
                "\u{2717} system: the turn was cancelled",
                ""
            ]
        );
        assert!(!shell.leaving(), "a failed turn ended the session");
        assert_eq!(shell.fatal(), None);
    }

    #[test]
    fn a_fatal_ends_the_session_and_is_remembered_rather_than_painted() {
        // It is not a document row: the band is about to come down, and the
        // message belongs on a terminal that has been given back first.
        let mut shell = shell(24, 80);
        shell.apply(UiEvent::Fatal("a turn panicked".to_string()));

        assert!(shell.leaving(), "a fatal did not end the session");
        assert_eq!(shell.fatal(), Some("a turn panicked"));
        assert!(
            !shell.document().iter().any(|row| row.contains("panicked")),
            "the fatal was painted into a band that is about to be taken down"
        );
    }

    // -----------------------------------------------------------------------
    // the screen changing size under the band
    // -----------------------------------------------------------------------

    #[test]
    fn a_wider_screen_rewraps_the_composer_and_unfinished_tail() {
        // The whole of what a resize owns on this side of the divider. The
        // composer's height is a function of the width it is wrapped to, so a
        // band re-solved without re-measuring the draft would put the divider
        // where the *old* wrap said it went; and the unfinished line's rows are
        // what the next append measures its scroll against.
        let mut shell = shell(24, 40);
        // Thirty-eight cells of draft on a screen whose composer has 38: two
        // rows here, one row at eighty.
        shell.route_bytes("x".repeat(50).as_bytes());
        assert_eq!(shell.geometry.input_rows(), 2);
        let narrow_top = shell.geometry.input_first;
        shell.apply(UiEvent::Delta("y".repeat(50)));
        let _ = shell.released();

        assert_eq!(
            shell.resize(24, 80),
            Resize::Repaint(shell.geometry),
            "a screen that really changed size was reported as no news"
        );

        assert_eq!(shell.geometry.cols, 80);
        assert_eq!(
            shell.geometry.input_rows(),
            1,
            "the draft was re-solved against the width it no longer has"
        );
        assert!(
            shell.geometry.input_first > narrow_top,
            "the composer's first row did not move when it stopped wrapping"
        );
        // The composer's rows, as the band now paints them: one row, not two.
        let rows = shell.band_rows();
        assert_eq!(
            rows.len(),
            usize::from(shell.geometry.band_rows()),
            "the band painted a number of rows the geometry does not own"
        );
        // And the tail: one more delta writes the whole unfinished line at the
        // new width rather than at the old one.
        shell.apply(UiEvent::Delta("z".to_string()));
        let tail = shell.released();
        assert!(
            tail.iter().all(|row| super::super::wrap::width(row) <= 80),
            "the tail was re-wrapped to something other than the screen: {tail:?}"
        );
        assert!(
            !tail.iter().any(|row| row.len() == 40),
            "a row wrapped to the old width survived the resize: {tail:?}"
        );
    }

    #[test]
    fn zero_by_zero_after_launch_is_no_new_information() {
        // A pty whose size was never set answers `0x0` successfully. At launch
        // that is a refusal and the band is solved from 24x80
        // (`super::term::window_size`); once a session is running it is the
        // opposite -- there *is* a band on a screen of a known size, and
        // replacing it with a fallback would move the band for a measurement
        // that said nothing.
        let mut shell = shell(40, 100);
        let before = shell.geometry;
        assert_eq!(shell.resize(0, 0), Resize::Unchanged);
        assert_eq!(shell.geometry, before);
        assert_eq!(shell.resize(0, 100), Resize::Unchanged);
        assert_eq!(shell.resize(40, 0), Resize::Unchanged);
        assert_eq!(shell.geometry, before);
        assert!(
            shell.render.begin().is_none() || before == shell.geometry,
            "a measurement that said nothing asked for a frame"
        );
    }

    #[test]
    fn a_resize_to_the_size_the_screen_already_is_asks_for_nothing() {
        // A `SIGWINCH` a terminal sends for a font change, or the second of a
        // burst the first already answered. Repainting for one would make an
        // idle session's cost a function of how often its terminal talks.
        let mut shell = shell(24, 80);
        let _ = shell.render.begin();
        assert_eq!(shell.resize(24, 80), Resize::Unchanged);
        assert!(
            shell.render.begin().is_none(),
            "a resize to the size the screen already is asked for a frame"
        );
    }

    #[test]
    fn a_resize_asks_for_one_frame_and_names_a_resize_as_the_reason() {
        let mut shell = shell(24, 80);
        let _ = shell.render.begin();
        assert!(matches!(shell.resize(30, 100), Resize::Repaint(_)));
        assert!(
            shell.render.begin().is_some(),
            "the band moved and nothing asked for a frame"
        );
    }

    #[test]
    fn resize_keeps_the_question_and_its_answer_channel() {
        // A resize is not an answer. The panel is the only thing on this
        // surface holding a turn open, and a re-solve that dropped it would
        // leave the runtime parked on a channel nobody will ever write to --
        // while the user watches a band with no question in it.
        let mut shell = shell(24, 80);
        asking(&mut shell);
        assert_eq!(shell.geometry.panel, PANEL_ROWS);

        assert!(matches!(shell.resize(30, 100), Resize::Repaint(_)));

        assert_eq!(
            shell.geometry.panel, PANEL_ROWS,
            "the question lost its rows when the screen changed size"
        );
        assert!(
            shell.band_rows().join("\n").contains("Permission needed"),
            "the question is no longer painted: {:?}",
            shell.band_rows()
        );
        assert_eq!(
            shell.controlled(),
            None,
            "the resize answered the question on the user's behalf"
        );
        // And the answer still reaches the runtime afterwards -- once a frame
        // for the screen's **new** size has landed. The resize revoked the
        // receipt the question earned at 24x80, which is the whole of
        // `Readiness::reconcile`'s post-write check.
        delivered(&mut shell);
        shell.route_bytes(b"1");
        assert_eq!(
            shell.controlled(),
            Some(TurnControl::Answer {
                id: ApprovalId(1),
                answer: ApprovalAnswer::Once,
                feedback: None,
            }),
            "the keystroke that answers the question stopped reaching the runtime"
        );
    }

    #[test]
    fn a_screen_too_small_for_the_band_keeps_the_question_until_it_grows_again() {
        // The one case a re-solve has no answer for. Refusing the question here
        // would be a decision the user never made, taken because a window was
        // dragged; and answering it would be worse. So the shell keeps
        // everything it holds, paints nothing new, and re-solves when the
        // screen can hold a band again.
        let mut shell = shell(24, 80);
        asking(&mut shell);
        let before = shell.geometry;

        assert_eq!(shell.resize(4, 80), Resize::TooSmall);
        assert_eq!(
            shell.geometry, before,
            "the band was solved for a screen that cannot hold one"
        );
        assert_eq!(
            shell.controlled(),
            None,
            "a window that got smaller answered the question"
        );

        // And the screen grows again. The size it comes back to is the one it
        // left, which is exactly the case a shell that only remembered its
        // geometry would report as "no news".
        assert!(matches!(shell.resize(24, 80), Resize::Repaint(_)));
        assert_eq!(shell.geometry.panel, PANEL_ROWS);
        assert!(
            shell.band_rows().join("\n").contains("Permission needed"),
            "the question was not painted after the screen grew back"
        );
        shell.route_bytes(b"1");
        assert_eq!(
            shell.controlled(),
            Some(TurnControl::Answer {
                id: ApprovalId(1),
                answer: ApprovalAnswer::Once,
                feedback: None,
            }),
            "the decision no longer reaches the runtime"
        );
    }

    #[test]
    fn a_resize_keeps_the_turns_row_above_the_composer() {
        // The band's other height. A re-solve that defaulted it would take the
        // activity row away from a turn that is still running, and the next
        // settle would put it back -- one frame of the band jumping for every
        // resize.
        let mut shell = shell(24, 80);
        let started = turn_running(&mut shell, b"say something\r");
        shell.settle_band(started);
        assert!(shell.geometry.activity.is_some());

        assert!(matches!(shell.resize(30, 100), Resize::Repaint(_)));
        assert!(
            shell.geometry.activity.is_some(),
            "the running turn lost its row when the screen changed size"
        );
        assert_eq!(
            shell.geometry.activity,
            Some(shell.geometry.input_first - 2)
        );
    }

    #[test]
    fn the_caret_stays_in_the_composer_across_a_resize() {
        // The band's rows and the caret are derived from one geometry, so a
        // resize that moved one without the other would leave the terminal's
        // cursor on the divider or below the hint row.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"hello");
        assert!(matches!(shell.resize(30, 100), Resize::Repaint(_)));
        let (row, cells) = shell.cursor();
        assert_eq!(row, shell.geometry.input_first);
        assert_eq!(cells, PROMPT_CELLS + 5);
    }

    #[test]
    fn a_truncated_report_does_not_throw_the_draft_away() {
        // What the decoder's phantom Escape costs where it is really paid. A
        // terminal answering `OSC 11` and being cut off mid-report ends the
        // string with an `ESC`, and the byte behind it can be another one -- a
        // keystroke, or the head of a sequence still arriving. Two Escapes
        // inside `gesture::CLEAR_WINDOW` are the gesture that empties the
        // composer, so a decoder that invents the first one lets a report xfx
        // asked for delete what the user was writing.
        //
        // The clock is read **after** each burst rather than before: the bare
        // `ESC` this hands back is resolved by `Decoder::ESC_TIMEOUT` measured
        // from the byte's own arrival, and a deadline computed from before it
        // is a few microseconds short of firing at all -- which would make this
        // case pass by never resolving the escape it is about.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"a draft worth keeping");
        assert_eq!(shell.editor.text(), "a draft worth keeping");

        shell.route_bytes(b"\x1b]11;rgb:0000/0000/0000\x1b\x1b");
        let reported = Instant::now();
        shell.settle_input(reported + Duration::from_millis(60));
        assert_eq!(
            shell.editor.text(),
            "a draft worth keeping",
            "a truncated background report emptied the composer"
        );

        // And the gesture still belongs to the user: the escape the report
        // handed back is one press, so their own next Escape is the second and
        // it clears the draft as it always did.
        shell.route_bytes(b"\x1b");
        let typed = Instant::now();
        shell.settle_input(typed + Duration::from_millis(60));
        assert_eq!(
            shell.editor.text(),
            "",
            "the double-Escape gesture stopped working after a truncated report"
        );
    }

    // -----------------------------------------------------------------------
    // the inline slash menu
    // -----------------------------------------------------------------------

    /// The band's rows between the two rules under the composer: what the
    /// menu, if there is one, put there. Empty while no rule is drawn.
    ///
    /// Read off the painted band rather than off the field, because the claim
    /// every case below makes is about what is on the screen -- and a menu the
    /// geometry gave rows to but nothing painted into is the exact defect the
    /// one-iterator rule exists to prevent.
    fn menu(shell: &Shell) -> Vec<String> {
        let rows = shell.band_rows();
        let rule = divider(usize::from(shell.geometry.cols));
        let Some(first) = rows.iter().position(|row| *row == rule) else {
            return Vec::new();
        };
        let last = rows
            .iter()
            .rposition(|row| *row == rule)
            .expect("a first rule is a rule");
        rows[first + 1..last].to_vec()
    }

    #[test]
    fn typing_a_slash_word_opens_the_menu_in_the_bands_elastic_slot() {
        let mut narrowed = shell(24, 80);
        narrowed.route_bytes(b"/he");

        assert!(
            narrowed.geometry.panel > 0,
            "the band gave the menu no rows: {:?}",
            narrowed.geometry
        );
        let rows = menu(&narrowed);
        assert_eq!(
            usize::from(narrowed.geometry.panel),
            rows.len(),
            "the rows the band solved for and the rows it painted disagree"
        );

        // A bare slash names every command, which is what pins the geometry
        // against the **whole** menu rather than against whatever the last
        // keystroke happened to leave: a band re-solved before the menu was
        // reconciled would be one keystroke behind, and on this query that is
        // the difference between a full menu and none. Upstream's budget on
        // this screen: a header, its blank row and six commands, and a
        // narrower query keeps the same height (`/he` above).
        let mut bare = shell(24, 80);
        bare.route_bytes(b"/");
        assert_eq!(bare.geometry.panel, 8, "{:?}", menu(&bare));
        assert_eq!(usize::from(bare.geometry.panel), menu(&bare).len());
        assert_eq!(narrowed.geometry.panel, bare.geometry.panel);
        let header = menu(&bare)[0].clone();
        assert!(
            header.starts_with(&format!(
                "{}Commands {} \u{b7} type to filter",
                PALETTE.dim(),
                crate::interactive::SLASH_COMMANDS.len()
            )),
            "{header:?}"
        );
        assert!(header.ends_with(&format!("1\u{2013}6{}", PALETTE.reset())));
        assert_eq!(menu(&bare)[1], "", "the blank row under the header");
        assert!(
            rows.iter().any(|row| row.contains("/help")),
            "the menu does not offer the command the draft names: {rows:?}"
        );
        assert_eq!(
            rows[0],
            format!("{}Commands 1{}", PALETTE.dim(), PALETTE.reset())
        );
        // The first match is the marked one, and the mark is brightness: bold
        // hint grey, no caret glyph.
        assert!(
            rows[2].starts_with(&format!("{}  /help", PALETTE.subtitle())),
            "the first match is not the marked one: {rows:?}"
        );
    }

    #[test]
    fn the_menu_never_steals_the_composers_caret() {
        let mut shell = shell(24, 80);
        shell.route_bytes(b"/mo");

        assert!(shell.geometry.panel > 0, "there is no menu to steal it");
        assert_eq!(
            shell.cursor(),
            (shell.geometry.input_first, PROMPT_CELLS + 3),
            "the caret left the text the user is typing"
        );
        // And the row it is on is the composer's, holding the draft.
        assert_eq!(shell.marked(), composer_row("/mo"));
    }

    #[test]
    fn escape_dismisses_the_menu_without_arming_the_clear_gesture() {
        let mut shell = shell(24, 80);
        shell.route_bytes(b"/he");
        assert!(shell.geometry.panel > 0, "there is no menu to dismiss");

        shell.route_bytes(&[0x1b]);
        shell.settle_input(Instant::now() + Decoder::ESC_TIMEOUT);

        assert_eq!(shell.geometry.panel, 0, "the menu is still taking rows");
        assert!(menu(&shell).is_empty(), "{:?}", menu(&shell));
        // The draft is untouched -- Escape closed a menu, it did not edit.
        assert_eq!(shell.editor.text(), "/he");
        // And the gesture is where it was: a second Escape must not clear a
        // composer whose owner was only closing a menu.
        assert_eq!(shell.hint(), IDLE_HINT, "the clear gesture was armed");
    }

    #[test]
    fn a_dismissal_survives_query_growth_until_the_draft_stops_being_a_command() {
        let mut shell = shell(24, 80);
        shell.route_bytes(b"/he");
        shell.route_bytes(&[0x1b]);
        shell.settle_input(Instant::now() + Decoder::ESC_TIMEOUT);
        assert_eq!(shell.geometry.panel, 0);

        shell.route_bytes(b"l");
        assert_eq!(
            shell.geometry.panel, 0,
            "the menu came back on the next letter of the word it was dismissed for"
        );
        assert_eq!(shell.editor.text(), "/hel");

        // The word goes, and the dismissal with it.
        shell.route_bytes(&[0x15]);
        assert!(shell.editor.is_empty());
        shell.route_bytes(b"/h");
        assert!(
            shell.geometry.panel > 0,
            "a new slash word inherited the old one's dismissal"
        );
    }

    #[test]
    fn tab_completes_the_marked_command_and_the_menu_follows_the_word() {
        let mut with_argument = shell(24, 80);
        with_argument.route_bytes(b"/mod");
        assert!(with_argument.geometry.panel > 0);

        with_argument.route_bytes(b"\t");
        assert_eq!(
            with_argument.editor.text(),
            "/model ",
            "a command that takes an argument was completed without room for one"
        );
        // The completion menu is gone with the word it was completing; what
        // the band shows now is `/model`'s own column under the argument
        // position the completion made room for (`fx-model-40x120-space`) --
        // `loading models...`, with the catalog asked for.
        assert!(
            with_argument.picker.is_none(),
            "the menu outlived the word it was completing"
        );
        assert!(
            with_argument
                .column
                .active(with_argument.editor.text())
                .is_some(),
            "the completed /model opened no column"
        );
        assert_eq!(with_argument.picks_up(), TurnWork::Catalog);

        // A command that takes no argument is completed exactly, and the word
        // is still a slash word, so upstream's menu follows it: one command,
        // marked, for the next Return to run (`resetInlinePickerEpisode`).
        let mut exact = shell(24, 80);
        exact.route_bytes(b"/vers");
        exact.route_bytes(b"\t");
        assert_eq!(exact.editor.text(), "/version");
        let rows = menu(&exact);
        assert_eq!(
            rows[0],
            format!("{}Commands 1{}", PALETTE.dim(), PALETTE.reset())
        );
        assert!(rows[2].starts_with(&format!("{}  /version", PALETTE.subtitle())));
        exact.route_bytes(&[0x0d]);
        assert_eq!(exact.geometry.panel, 0, "the menu outlived the submission");
        let document = exact.document();
        assert_eq!(
            document,
            said(notice::Notice::neutral("version", crate::VERSION)),
            "the completed command did not run, or was echoed: {document:?}"
        );
    }

    #[test]
    fn enter_runs_the_marked_row_in_one_return() {
        // Upstream resolves the submission to the marked row
        // (`input_submit_runtime.zig:1087-1089`): a partial word runs the
        // command the menu marked for it, and a row moved to runs that row --
        // one Return either way, with the menu gone.
        let mut partial = shell(24, 80);
        partial.route_bytes(b"/ne");
        assert!(partial.geometry.panel > 0);
        partial.route_bytes(&[0x0d]);
        assert_eq!(
            partial.geometry.panel, 0,
            "the menu outlived the submission"
        );
        assert_eq!(
            partial.controlled(),
            Some(TurnControl::Reset { through: 0 }),
            "Enter ran the typed word instead of the marked command"
        );
        assert!(partial.editor.is_empty());

        // `/help`, `/clear`, `/new`: two Downs from a bare slash.
        let mut moved = shell(24, 80);
        moved.route_bytes(b"/");
        moved.route_bytes(b"\x1b[B\x1b[B");
        moved.route_bytes(&[0x0d]);
        assert_eq!(moved.geometry.panel, 0);
        assert_eq!(
            moved.controlled(),
            Some(TurnControl::Reset { through: 0 }),
            "Enter did not take the row"
        );
        assert!(moved.editor.is_empty());
    }

    #[test]
    fn a_slash_word_that_names_nothing_opens_no_menu() {
        let mut shell = shell(24, 80);
        shell.route_bytes(b"/zzz");
        assert_eq!(shell.geometry.panel, 0, "{:?}", menu(&shell));
        assert!(menu(&shell).is_empty());
    }

    #[test]
    fn a_question_takes_the_slot_the_menu_was_using() {
        // The one order a session can produce both in: a turn is running, the
        // user starts typing a command at it, and the turn asks a question.
        let mut shell = shell(24, 80);
        let started = turn_running(&mut shell, b"edit the notes\r");
        shell.route_bytes(b"/he");
        assert!(shell.geometry.panel > 0, "there is no menu to displace");

        shell.apply(UiEvent::Approval(question(asked())));
        shell.settle_band(started);
        let _ = shell.document();

        let rows = menu(&shell);
        assert!(
            rows.iter().any(|row| row.contains(approval::TITLE)),
            "the question did not take the slot: {rows:?}"
        );
        assert!(
            !rows
                .iter()
                .any(|row| row.contains("show available slash commands")),
            "the menu and the question were painted together: {rows:?}"
        );
        assert_eq!(
            shell.geometry.panel,
            Panel::new(asked()).height(shell.geometry.cols, shell.geometry.rows),
            "the slot is the question's height rather than the menu's"
        );
        // The question has the focus, so the caret is on it rather than in the
        // composer the menu left the draft in.
        assert_eq!(shell.cursor().1, 0, "the caret stayed in the composer");

        // And answering it does not put the menu back in front of a draft the
        // user stopped looking at a question ago: the menu was *dismissed*,
        // not merely covered.
        shell.route_bytes(b"3");
        assert_eq!(
            shell.geometry.panel,
            0,
            "the answered question left something in the slot: {:?}",
            menu(&shell)
        );
        assert!(menu(&shell).is_empty(), "{:?}", menu(&shell));
        assert_eq!(shell.editor.text(), "/he", "the draft did not survive");
    }

    #[test]
    fn the_slot_never_holds_both_a_question_and_a_menu() {
        // The premise `Shell::slot`'s ordering rests on, driven rather than
        // argued: the order in that function decides which of the two wins, and
        // it can only ever be exercised by a session that has both. This walks
        // the one sequence that comes closest -- a turn running, a menu open, a
        // question arriving, the answer, and typing afterwards -- and requires
        // the two never to be up together at any step of it.
        let mut shell = shell(24, 80);
        let both = |shell: &Shell, at: &str| {
            assert!(
                !(shell.panel.is_some() && shell.picker.is_some()),
                "a question and a menu were up together {at}"
            );
        };
        let started = turn_running(&mut shell, b"edit the notes\r");
        both(&shell, "while a turn started");
        shell.route_bytes(b"/he");
        both(&shell, "with a menu open");
        shell.apply(UiEvent::Approval(question(asked())));
        shell.settle_band(started);
        both(&shell, "when the question arrived");
        // Every keystroke goes to the question while it is up, so nothing can
        // open a menu behind it -- including the letters of a slash word.
        shell.route_bytes(b"/quit");
        both(&shell, "while the question had the focus");
        assert!(!shell.leaving(), "a keystroke leaked past the question");
        shell.route_bytes(b"3");
        both(&shell, "once the question was answered");
        shell.route_bytes(b"l");
        both(&shell, "with the draft edited again");
    }

    #[test]
    fn a_menu_that_offers_nothing_binds_nothing() {
        // A menu with no matches is not a menu with an empty list -- it is no
        // menu at all, and the keys it would have bound go on meaning what they
        // mean. Driven rather than asserted about the field, because the way a
        // zero-match menu goes wrong is arithmetic: a mark taken modulo an
        // empty list divides by zero.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"/zzz");
        assert_eq!(shell.geometry.panel, 0, "{:?}", menu(&shell));

        shell.route_bytes(b"\x1b[A");
        shell.route_bytes(b"\t");
        assert_eq!(shell.editor.text(), "/zzz", "the draft was edited");
        assert_eq!(shell.geometry.panel, 0, "{:?}", menu(&shell));
    }

    #[test]
    fn the_mark_survives_a_keystroke_that_did_not_change_the_word() {
        // The mark is state, and the list it indexes is derived: a rebuild on
        // every edit would put the mark back on the first row every time the
        // caret moved, which is a menu the user cannot walk down and then
        // correct a letter in.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"/");
        shell.route_bytes(b"\x1b[B");
        // A caret move, and nothing else: the word is what it was.
        shell.route_bytes(b"\x1b[D");
        assert_eq!(shell.editor.text(), "/");

        shell.route_bytes(b"\t");
        assert_eq!(
            shell.editor.text(),
            "/clear",
            "the mark was reset by a keystroke that changed no letter"
        );
    }

    #[test]
    fn the_shortest_screen_still_shows_a_menu_while_a_turn_is_running() {
        // The band's tightest case, and the one a reserve that is a row short
        // breaks silently: the smallest screen a band fits on, with a turn
        // running -- so the activity row is taking one of its rows -- and the
        // query that offers the most matches. A menu that wanted two rows here
        // makes `layout::solve_band` refuse the whole band (`content_bottom`
        // reaches 0), `fit` answers `None`, `refit` keeps the geometry it had,
        // and the session is then left with a menu that binds Up, Down, Tab,
        // Enter and Esc while painting **nothing** -- keys taken away from the
        // composer by something the user cannot see.
        //
        // Every number below is written out rather than derived from the
        // reserve, because a test that recomputed the constant it is protecting
        // would pass for whatever that constant became.
        let mut shell = shell(layout::MIN_ROWS, layout::MIN_COLS);
        let started = turn_running(&mut shell, b"say it\r");
        assert!(
            shell.geometry.activity.is_some(),
            "there is no turn running, so this is not the case under test"
        );
        shell.route_bytes(b"/");
        shell.settle_band(started);

        // Every command matches a bare slash, and one row is what upstream's
        // budget gives this screen (`inlinePickerRowBudget`: seven rows less
        // ten is less than one, and a budget is never less than one) -- too
        // few for the header, so the row is the marked command: the window
        // bounds the menu rather than the band refusing it.
        assert_eq!(
            shell.geometry.panel, 1,
            "a seven-row screen with a turn on it did not get its one menu row: {:?}",
            shell.geometry
        );
        assert_eq!(menu(&shell).len(), 1, "{:?}", menu(&shell));
        assert!(
            menu(&shell)[0].starts_with(&format!("{}  /help", PALETTE.subtitle())),
            "{:?}",
            menu(&shell)
        );

        // And the band really is the band this screen and this state solve to,
        // rather than the one the previous keystroke left standing. This is the
        // assertion a refused `fit` fails: `refit` keeps a stale geometry, so
        // the two stop agreeing.
        assert_eq!(
            shell.fit(layout::MIN_ROWS, layout::MIN_COLS),
            Some(shell.geometry),
            "the band was not re-solved for the screen it is on"
        );
        assert_eq!(
            (
                shell.geometry.activity,
                shell.geometry.panel_first(),
                shell.geometry.divider,
                shell.geometry.input_first,
                shell.geometry.hint,
                shell.geometry.content_bottom,
            ),
            (Some(2), 5, 6, 3, 7, 1),
            "the seven rows are not laid out as a turn, the composer, a rule, \
             a menu, a rule and a hint with one document row left: {:?}",
            shell.geometry
        );

        // The composer keeps its row and the caret stays in it.
        assert_eq!(shell.geometry.input_rows(), 1, "the composer lost its row");
        assert_eq!(shell.cursor(), (3, PROMPT_CELLS + 1));
        assert_eq!(shell.marked(), composer_row("/"));
        // And the band paints exactly the rows it solved for.
        assert_eq!(
            shell.band_rows().len(),
            usize::from(shell.geometry.band_rows())
        );
    }

    #[test]
    fn the_menu_fits_the_shortest_screen_a_band_fits_on() {
        // A question can be refused on a screen too small for it; a completion
        // never is, because it can always show fewer matches. The claim is that
        // the smallest band still has a menu **and** a composer on it.
        let mut shell = shell(layout::MIN_ROWS, layout::MIN_COLS);
        shell.route_bytes(b"/");

        assert_eq!(
            shell.geometry.panel, 1,
            "the shortest screen got no menu at all, or more than it can hold"
        );
        let rows = menu(&shell);
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert!(
            rows[0].starts_with(&format!("{}  /help", PALETTE.subtitle())),
            "{rows:?}"
        );
        assert_eq!(shell.geometry.input_rows(), 1, "the composer lost its row");
        assert_eq!(
            shell.cursor(),
            (shell.geometry.input_first, PROMPT_CELLS + 1)
        );
    }

    #[test]
    fn the_hint_row_is_the_menus_key_help_while_it_is_up() {
        // Upstream replaces the hint row while the slash menu is up
        // (`paint_plan.zig:970-971`): the menu's keys, in the dim grey, and
        // nothing of the hint beside them. Escape gives the row back.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"/");
        let rows = shell.band_rows();
        assert_eq!(
            rows.last().expect("a hint row"),
            &format!(
                "{}\u{2191}\u{2193} navigate     enter use     esc close{}",
                PALETTE.dim(),
                PALETTE.reset()
            )
        );
        shell.route_bytes(&[0x1b]);
        shell.settle_input(Instant::now() + Decoder::ESC_TIMEOUT);
        assert_eq!(shell.hint(), IDLE_HINT, "Escape did not give the hint back");

        // The dim grey is its own role: a light session paints the key help
        // in the light dim grey, not in the statusline grey the hint uses.
        let mut light = shell_with(24, 80, LIGHT, true);
        light.route_bytes(b"/");
        let rows = light.band_rows();
        let last = rows.last().expect("a hint row");
        assert!(last.starts_with(LIGHT.dim()), "{last:?}");
        assert_ne!(LIGHT.dim(), LIGHT.statusline());
    }

    #[test]
    fn a_draft_that_opens_a_menu_is_one_composer_row_on_the_narrowest_screen() {
        // The menu's budget assumes it (`Picker::layout`: upstream's
        // `input_extra` is zero here), so it is driven rather than argued:
        // every name and alias in full, which is the longest a query that
        // matches anything can be, on the narrowest screen a band fits on.
        for spec in crate::interactive::SLASH_REGISTRY {
            for name in std::iter::once(&spec.name).chain(spec.aliases) {
                let mut shell = shell(24, layout::MIN_COLS);
                shell.route_bytes(name.as_bytes());
                assert!(shell.geometry.panel > 0, "{name} opened no menu");
                assert_eq!(shell.geometry.input_rows(), 1, "{name}");
            }
        }
    }

    // -----------------------------------------------------------------------
    // the /help catalog
    // -----------------------------------------------------------------------

    /// A session with the catalog up, the way a user gets one: `/help` typed,
    /// the completion menu running it on the Return.
    /// `All`'s order: every category's commands, in the categories' order.
    fn catalog_order() -> Vec<&'static str> {
        crate::interactive::SLASH_CATEGORIES
            .iter()
            .flat_map(|category| {
                crate::interactive::SLASH_REGISTRY
                    .iter()
                    .filter(move |spec| spec.category == *category)
                    .map(|spec| spec.name)
            })
            .collect()
    }

    fn with_catalog(rows: u16, cols: u16) -> Fixture {
        let mut shell = shell(rows, cols);
        shell.route_bytes(b"/help\r");
        let _echo = shell.document();
        assert!(shell.help_menu.is_some(), "/help did not open the catalog");
        shell
    }

    /// A band row's text with every sequence taken out, the bold included.
    fn visible_text(row: &str) -> String {
        let mut out = String::new();
        let mut rest = row;
        while !rest.is_empty() {
            if let Some(len) = super::super::pacer::escape_at(rest) {
                rest = &rest[len..];
                continue;
            }
            let mut characters = rest.chars();
            out.extend(characters.next());
            rest = characters.as_str();
        }
        out
    }

    /// The catalog's key help as the hint row paints it on a 120-column screen.
    fn catalog_key_help() -> String {
        format!(
            "{}\u{2191}\u{2193} navigate     tab category     enter open     esc close{}",
            PALETTE.dim(),
            PALETTE.reset()
        )
    }

    #[test]
    fn help_opens_the_catalog_in_the_band_and_writes_nothing_to_the_document() {
        let mut shell = shell(40, 120);
        // The handler alone: what the document gets for the submitted line is
        // the submit's business, and `/help`'s own answer is the band.
        shell.help();
        assert!(shell.document().is_empty(), "/help wrote to the document");
        let rows = menu(&shell);
        assert_eq!(rows.len(), 2 + crate::interactive::SLASH_REGISTRY.len());
        assert_eq!(usize::from(shell.geometry.panel), rows.len());
        assert!(
            visible_text(&rows[0]).starts_with(&format!(
                "Commands {}  [All]  General  Session",
                crate::interactive::SLASH_REGISTRY.len()
            )),
            "{rows:?}"
        );
        assert_eq!(rows[1], "", "the blank row under the header");
        // The summary at 2 + the widest name + 4, the widest read off the
        // registry: `/permissions` moved it, and the next long name will.
        let widest = crate::interactive::SLASH_REGISTRY
            .iter()
            .map(|spec| spec.name.len())
            .max()
            .expect("a registry");
        assert_eq!(
            rows[2],
            format!(
                "  {bright}/help{reset}{gap}{bright}show available slash commands{reset}",
                bright = PALETTE.subtitle(),
                reset = PALETTE.reset(),
                gap = " ".repeat(widest + 4 - "/help".len()),
            )
        );
        assert_eq!(
            rows[2..]
                .iter()
                .map(|row| visible_text(row)
                    .split_whitespace()
                    .next()
                    .map(str::to_string))
                .collect::<Vec<_>>(),
            crate::interactive::SLASH_CATEGORIES
                .iter()
                .flat_map(|category| crate::interactive::SLASH_REGISTRY
                    .iter()
                    .filter(move |spec| spec.category == *category))
                .map(|spec| Some(spec.name.to_string()))
                .collect::<Vec<_>>()
        );
        assert_eq!(shell.band_rows().last(), Some(&catalog_key_help()));
        // The caret stays in the composer: the catalog is a view of it.
        assert_eq!(
            shell.cursor(),
            (shell.geometry.input_first, PROMPT_CELLS),
            "the catalog took the caret"
        );
    }

    #[test]
    fn a_slash_word_typed_into_the_catalog_is_its_search_and_opens_no_menu() {
        let mut shell = with_catalog(40, 120);
        shell.route_bytes(b"/version");
        assert!(
            shell.picker.is_none(),
            "the completion menu opened over the catalog"
        );
        let rows = menu(&shell);
        assert_eq!(rows.len(), 3, "{rows:?}");
        assert!(visible_text(&rows[0]).starts_with("Commands 1  [All]"));
        assert_eq!(
            visible_text(&rows[2]),
            "  /version    show the xfx version",
            "the summary column is the filtered list's, 2 + 8 + 4"
        );
        assert_eq!(shell.band_rows().last(), Some(&catalog_key_help()));
    }

    #[test]
    fn enter_on_a_command_typed_in_full_runs_it_whatever_the_list_says() {
        // On the Session tab `/exit` finds nothing -- but it is a name, so
        // Enter submits it as typed before it asks the list anything.
        let mut shell = with_catalog(40, 120);
        shell.route_bytes(b"\t\t/exit");
        let rows = menu(&shell);
        assert!(visible_text(&rows[0]).contains("[Session]"), "{rows:?}");
        assert_eq!(
            rows[2],
            format!("{}No commands found.{}", PALETTE.dim(), PALETTE.reset())
        );
        shell.route_bytes(b"\r");
        assert!(shell.help_menu.is_none());
        assert!(shell.leaving(), "the typed command was not run");
    }

    #[test]
    fn enter_on_a_row_runs_its_command_in_one_return() {
        // `/quit`'s row of `All`: that many Downs and a Return.
        let mut shell = with_catalog(40, 120);
        let at = catalog_order()
            .iter()
            .position(|name| *name == "/quit")
            .expect("/quit is listed");
        for _ in 0..at {
            shell.route_bytes(b"\x1b[B");
        }
        assert!(
            menu(&shell)[2 + at].starts_with(&format!("  {}/quit", PALETTE.subtitle())),
            "{:?}",
            menu(&shell)
        );
        shell.route_bytes(b"\r");
        assert!(shell.help_menu.is_none());
        assert!(shell.leaving(), "the marked command was not run");
        assert_eq!(shell.editor.text(), "");
    }

    #[test]
    fn enter_on_a_command_that_takes_an_argument_leaves_room_for_it_and_runs_nothing() {
        // Account's one command is `/provider`. What the space opens is the
        // command's own argument picker, as upstream's does after `/login `
        // (`fx-help-40x120-enter-args`): the provider column, not the
        // completion menu, and nothing run or sent.
        let mut shell = with_catalog(40, 120);
        shell.route_bytes(b"\t\t\t");
        assert!(visible_text(&menu(&shell)[2]).starts_with("  /provider"));
        shell.route_bytes(b"\r");
        assert!(shell.help_menu.is_none());
        assert_eq!(shell.editor.text(), "/provider ");
        assert!(shell.picker.is_none(), "a word with a space opened a menu");
        assert!(
            shell.provider_picker.is_some(),
            "the argument has no picker"
        );
        assert_eq!(shell.geometry.panel, 6);
        assert!(shell.document().is_empty(), "something ran");
        assert!(shell.sent.try_recv().is_err(), "something was sent");
    }

    #[test]
    fn enter_on_an_empty_tab_does_nothing() {
        let mut shell = with_catalog(40, 120);
        // Agents, the ninth tab after `All`.
        shell.route_bytes(b"\t\t\t\t\t\t\t\t\t");
        let rows = menu(&shell);
        assert!(visible_text(&rows[0]).starts_with("Commands 0"), "{rows:?}");
        assert!(visible_text(&rows[0]).contains("[Agents]"), "{rows:?}");
        assert_eq!(shell.geometry.panel, 3);
        shell.route_bytes(b"\r");
        assert!(shell.help_menu.is_some(), "an empty tab's Return closed it");
        assert!(!shell.leaving());
        assert!(shell.document().is_empty());
    }

    #[test]
    fn up_and_down_wrap_and_tab_and_shift_tab_resize_the_catalog() {
        let mut shell = with_catalog(40, 120);
        shell.route_bytes(b"\x1b[A");
        let rows = menu(&shell);
        let last = rows.len() - 1;
        // The last in the catalog's order: the last category's last command.
        let final_name = crate::interactive::SLASH_CATEGORIES
            .iter()
            .flat_map(|category| {
                crate::interactive::SLASH_REGISTRY
                    .iter()
                    .filter(move |spec| spec.category == *category)
            })
            .last()
            .expect("a registry")
            .name;
        assert!(
            rows[last].starts_with(&format!("  {}{final_name}", PALETTE.subtitle())),
            "Up from the first row is the last: {rows:?}"
        );
        shell.route_bytes(b"\x1b[B");
        assert!(menu(&shell)[2].starts_with(&format!("  {}/help", PALETTE.subtitle())));
        // General's commands, and the band gives the rows back.
        let general = crate::interactive::SLASH_REGISTRY
            .iter()
            .filter(|spec| spec.category == "General")
            .count();
        shell.route_bytes(b"\t");
        assert_eq!(usize::from(shell.geometry.panel), 2 + general);
        assert_eq!(menu(&shell).len(), 2 + general);
        // Shift-Tab, as a terminal spells it, is back to `All`.
        shell.route_bytes(b"\x1b[Z");
        let count = crate::interactive::SLASH_REGISTRY.len();
        assert_eq!(usize::from(shell.geometry.panel), 2 + count);
        assert!(visible_text(&menu(&shell)[0]).starts_with(&format!("Commands {count}  [All]")));
    }

    #[test]
    fn escape_closes_the_catalog_and_its_search_without_arming_the_clear() {
        let mut shell = with_catalog(40, 120);
        shell.route_bytes(b"ses");
        assert_eq!(
            menu(&shell).len(),
            2 + 6,
            "/clear, /new, /reset, /resume, /rename and /copy"
        );
        shell.route_bytes(&[0x1b]);
        shell.settle_input(Instant::now() + Decoder::ESC_TIMEOUT);
        assert!(shell.help_menu.is_none());
        assert_eq!(shell.editor.text(), "", "the search outlived the catalog");
        assert_eq!(shell.geometry.panel, 0);
        assert_eq!(shell.hint(), IDLE_HINT, "the clear gesture was armed");
    }

    #[test]
    fn ctrl_u_empties_the_search_and_keeps_the_catalog() {
        let mut shell = with_catalog(40, 120);
        shell.route_bytes(b"ses");
        shell.route_bytes(&[0x15]);
        assert_eq!(shell.editor.text(), "");
        assert!(shell.help_menu.is_some(), "C-u closed the catalog");
        assert!(visible_text(&menu(&shell)[0]).starts_with(&format!(
            "Commands {}",
            crate::interactive::SLASH_REGISTRY.len()
        )));
    }

    #[test]
    fn the_catalog_fits_a_short_screen_and_scrolls_rather_than_overflowing() {
        // 15 rows: a budget of five, so a header, its blank row and three
        // commands -- and the mark walks the window down rather than off it.
        let mut shell = with_catalog(15, 80);
        assert_eq!(shell.geometry.panel, 5);
        for _ in 0..4 {
            shell.route_bytes(b"\x1b[B");
        }
        let rows = menu(&shell);
        assert_eq!(rows.len(), 5);
        // Four Downs mark the fifth command, on the window's last row, with
        // the two before it above it.
        let order = catalog_order();
        assert!(
            rows[4].starts_with(&format!("  {}{}", PALETTE.subtitle(), order[4])),
            "{rows:?}"
        );
        assert!(
            visible_text(&rows[2]).starts_with(&format!("  {}", order[2])),
            "{rows:?}"
        );
        // The key help is the one that fits 80 columns.
        assert!(visible_text(shell.band_rows().last().expect("a hint row"))
            .starts_with("\u{2191}\u{2193} navigate     tab category"));
    }

    // -----------------------------------------------------------------------
    // provider switching, the catalog browser and the context meter
    // -----------------------------------------------------------------------

    /// A second shell, for the half of a claim the first one cannot hold at the
    /// same time.
    fn fixture_with_no_catalog() -> Fixture {
        shell(24, 80)
    }

    fn entry(name: &str, window: Option<u64>, efforts: &[&str]) -> CatalogEntry {
        CatalogEntry {
            id: format!("vendor/{name}"),
            aliases: vec![name.to_string()],
            name: None,
            efforts: efforts.iter().map(|effort| effort.to_string()).collect(),
            max_context: window,
            facts: Default::default(),
        }
    }

    #[test]
    fn setup_hands_the_whole_switch_to_the_runtime_and_predicts_nothing() {
        // The UI thread writes no file, opens no socket and re-reads no
        // configuration. What it does is name a provider and wait to be told
        // what the session became -- which is the difference between showing
        // what the configuration resolved to and showing what the write meant.
        let mut shell = shell(24, 80);
        let before = shell.shell.model.clone();
        shell.route_bytes(b"/setup llmux\r");
        assert_eq!(shell.picks_up(), TurnWork::Setup(ProviderId::Llmux));
        assert_eq!(
            shell.shell.model, before,
            "the shell changed the model before the runtime had reloaded anything"
        );
        assert_eq!(
            shell.shell.provider,
            ProviderId::Gateway,
            "the shell changed the provider before the runtime had reloaded anything"
        );
    }

    #[test]
    fn an_argument_that_names_no_provider_is_an_unknown_command_and_never_reaches_the_runtime() {
        // Refused here rather than sent, for `/model`'s reason: the far side of
        // this one writes a file, and a queue place spent on a word that cannot
        // be a provider is a place a real prompt could have had. The column
        // finds no row for it, so the Return goes on to the router, which
        // answers in upstream's words (`fx-provider-40x120-arg-nomatch-enter`)
        // -- not a usage line of xfx's own, which this used to be.
        let mut shell = shell(24, 80);
        for argument in ["nonesuch", "llmux extra", "gateway gateway", "zz"] {
            for name in ["/provider", "/setup"] {
                shell.route_bytes(format!("{name} {argument}\r").as_bytes());
                assert!(
                    shell.sent.try_recv().is_err(),
                    "`{name} {argument}` reached the runtime"
                );
                assert!(shell.editor.is_empty(), "`{name} {argument}` stayed");
                assert!(shell.provider_picker.is_none());
            }
        }
        let document = shell.document();
        let refusals: Vec<&String> = document
            .iter()
            .filter(|row| *row == "\u{2717} command: Unknown command. Try /help.")
            .collect();
        assert_eq!(refusals.len(), 8, "{document:?}");
        // Nothing of the argument is quoted back, and the submitted line is
        // not echoed either, so nothing of it is there at all.
        assert!(
            !document
                .iter()
                .any(|row| row.contains("nonesuch") || row.contains("extra")),
            "the refusal quoted the argument back: {document:?}"
        );
        assert!(
            !document.iter().any(|row| row.contains("/setup takes")),
            "the line shell's usage line reached the band: {document:?}"
        );
    }

    #[test]
    fn a_trailing_space_and_a_return_commit_the_marked_row() {
        // **Changed claim.** `/setup ` and a Return used to be a usage error;
        // the space is the column's trigger now, so the Return is the column's
        // and takes its marked row, the first -- as upstream's `/setup ` does
        // (`fx-provider2-40x120-setup-space`, then Enter).
        for line in ["/setup \r", "/provider   \r", "  /SETUP \r"] {
            let mut shell = shell(24, 80);
            shell.route_bytes(line.as_bytes());
            assert_eq!(
                shell.picks_up(),
                TurnWork::Setup(ProviderId::Gateway),
                "{line:?}"
            );
            assert!(shell.editor.is_empty(), "{line:?}");
            assert!(shell.provider_picker.is_none(), "{line:?}");
        }
    }

    /// A `/provider` row as the column paints it at `anchor`: the label in
    /// `style` and closed, then the dim ` · current` when it is current.
    fn provider_row(anchor: usize, label: &str, marked: bool, current: bool) -> String {
        let style = if marked {
            PALETTE.subtitle()
        } else {
            PALETTE.dim()
        };
        let mut row = format!("{}{style}{label}{}", " ".repeat(anchor), PALETTE.reset());
        if current {
            row.push_str(&format!(
                "{}{} current{}",
                PALETTE.dim(),
                " \u{b7}",
                PALETTE.reset()
            ));
        }
        row
    }

    #[test]
    fn a_bare_provider_or_setup_reseeds_the_composer_and_opens_the_column_under_it() {
        // `fx-provider-40x120-open` and `setup-open`: either spelling and a
        // Return leave the canonical `/provider ` in the composer, and the
        // column under its argument -- column 12 -- between the two rules,
        // six rows tall, the first row marked and the credentialed active
        // provider annotated. The hint row stays the statusline.
        for typed in ["/provider\r", "/setup\r"] {
            let mut shell = shell(40, 120);
            shell.shell.missing_credential = false;
            shell.route_bytes(typed.as_bytes());
            assert_eq!(shell.editor.text(), "/provider ", "{typed:?}");
            assert!(shell.picker.is_none(), "a completion menu is up too");
            let rows = shell.band_rows();
            assert_eq!(visible_text(&rows[0]), "\u{2503} /provider ");
            assert_eq!(rows[1], divider(120));
            assert_eq!(
                rows[2..8],
                [
                    provider_row(12, "gateway", true, true),
                    provider_row(12, "llmux", false, false),
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                ]
            );
            assert_eq!(rows[8], divider(120));
            assert_eq!(rows.len(), 10);
            assert_eq!(shell.hint(), "auto \u{b7} glm-5.2", "{typed:?}");
            // The caret stays in the composer, after the space.
            assert_eq!(
                shell.cursor(),
                (shell.geometry.input_first, PROMPT_CELLS + 10)
            );
            assert!(shell.document().is_empty(), "{typed:?} wrote a row");
            assert!(shell.sent.try_recv().is_err(), "{typed:?} sent something");
        }
    }

    #[test]
    fn the_setup_prefix_typed_with_its_space_keeps_its_spelling_and_its_column() {
        // `fx-provider2-40x120-setup-space`: typed rather than run, `/setup `
        // stays `/setup ` and the column sits under its argument, column 9.
        // A session with no credential has no current provider.
        let mut shell = shell(40, 120);
        shell.route_bytes(b"/setup ");
        assert_eq!(shell.editor.text(), "/setup ");
        assert_eq!(
            menu(&shell)[..2],
            [
                provider_row(9, "gateway", true, false),
                provider_row(9, "llmux", false, false),
            ]
        );
        // The same column after leading blanks, anchored where the argument
        // really is.
        let mut blanks = self::shell(40, 120);
        blanks.route_bytes(b"  /provider ");
        assert_eq!(menu(&blanks)[0], provider_row(14, "gateway", true, false));
    }

    #[test]
    fn the_arrows_wrap_and_enter_commits_the_marked_row_without_a_history_entry() {
        // `fx-provider-40x120-down1` .. `down3-wrap`, then Enter: the commit
        // is the runtime's `TurnWork::Setup`, the composer is cleared, the
        // column is gone, and upstream's `Preparing …` is said. The command
        // that opened the column is in the history; the commit is not -- it
        // never was a submitted line (`app_input_runtime.zig:1593-1596`).
        let mut shell = shell(40, 120);
        shell.route_bytes(b"/provider\r");
        shell.route_bytes(b"\x1b[B");
        assert_eq!(menu(&shell)[1], provider_row(12, "llmux", true, false));
        shell.route_bytes(b"\x1b[B");
        assert_eq!(menu(&shell)[0], provider_row(12, "gateway", true, false));
        shell.route_bytes(b"\x1b[A");
        assert_eq!(menu(&shell)[1], provider_row(12, "llmux", true, false));
        assert!(shell.sent.try_recv().is_err(), "a move committed");

        shell.route_bytes(b"\r");
        assert_eq!(shell.picks_up(), TurnWork::Setup(ProviderId::Llmux));
        assert!(shell.editor.is_empty());
        assert!(shell.provider_picker.is_none());
        assert_eq!(shell.geometry.panel, 0, "the column kept its rows");
        assert_eq!(
            shell.document(),
            vec!["* provider: Preparing llmux.".to_string(), String::new()]
        );
        shell.route_bytes(b"\x1b[A");
        assert_eq!(shell.editor.text(), "/provider", "the recall");
    }

    #[test]
    fn setup_llmux_typed_whole_still_switches_at_once_with_or_without_the_column() {
        // Trap 2 of the research: `/setup llmux` and a Return is xfx's
        // complete command. With the column up it is the one filtered row;
        // with the column dismissed it is the router's, and it acts the same.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"/setup llmux\r");
        assert_eq!(shell.picks_up(), TurnWork::Setup(ProviderId::Llmux));

        let mut dismissed = self::shell(24, 80);
        dismissed.route_bytes(b"/setup LLMUX");
        escape(&mut dismissed);
        assert!(
            dismissed.provider_picker.is_none(),
            "Escape left the column up"
        );
        assert_eq!(
            dismissed.editor.text(),
            "/setup LLMUX",
            "Escape took the text"
        );
        dismissed.route_bytes(b"\r");
        assert_eq!(dismissed.picks_up(), TurnWork::Setup(ProviderId::Llmux));
        assert!(dismissed.editor.is_empty());
        assert_eq!(
            dismissed.document(),
            vec!["* provider: Preparing llmux.".to_string(), String::new()]
        );
    }

    #[test]
    fn typing_filters_the_column_and_tab_writes_the_row_without_committing() {
        // `fx-provider-40x120-arg-co` and `arg-co-tab`.
        let mut shell = shell(40, 120);
        shell.route_bytes(b"/provider LL");
        assert_eq!(
            menu(&shell)[..2],
            [provider_row(12, "llmux", true, false), String::new()]
        );
        shell.route_bytes(b"\t");
        assert_eq!(shell.editor.text(), "/provider llmux");
        assert_eq!(menu(&shell)[0], provider_row(12, "llmux", true, false));
        assert!(shell.sent.try_recv().is_err(), "Tab committed");
        // And a Tab with no row writes nothing.
        let mut unmatched = self::shell(40, 120);
        unmatched.route_bytes(b"/provider zz\t");
        assert_eq!(unmatched.editor.text(), "/provider zz");
        assert_eq!(
            visible_text(&menu(&unmatched)[0]),
            format!("{}no matching providers", " ".repeat(12))
        );
    }

    #[test]
    fn escape_closes_the_column_until_the_draft_stops_being_its() {
        // `fx-provider-40x120-esc` and `arg-esc`: the text stays and the band
        // gives the rows back; editing the argument does not bring the column
        // back, and leaving the command does forget the dismissal.
        let mut shell = shell(40, 120);
        shell.route_bytes(b"/provider ");
        escape(&mut shell);
        assert_eq!(shell.editor.text(), "/provider ");
        assert!(shell.provider_picker.is_none());
        assert_eq!(shell.geometry.panel, 0);
        shell.route_bytes(b"ll");
        assert!(shell.provider_picker.is_none(), "an edit reopened it");
        shell.route_bytes(&[0x15]);
        shell.route_bytes(b"/setup ");
        assert!(shell.provider_picker.is_some(), "the dismissal outlived it");
    }

    #[test]
    fn the_completion_menus_dismissal_is_forgotten_once_the_word_becomes_the_column() {
        // Escape over `/provider` closes the completion menu; the space makes
        // the draft the column's, which is a change of trigger, so a Backspace
        // back to the slash word offers the menu again.
        let mut shell = shell(40, 120);
        shell.route_bytes(b"/provider");
        assert!(shell.picker.is_some());
        escape(&mut shell);
        assert!(shell.picker.is_none());
        shell.route_bytes(b" ");
        assert!(shell.provider_picker.is_some());
        shell.route_bytes(&[0x7f]);
        assert!(shell.provider_picker.is_none());
        assert!(shell.picker.is_some(), "the menu stayed dismissed");
    }

    #[test]
    fn left_moves_the_caret_and_right_from_the_end_is_enter() {
        // No column before this one, so Left is the editor's; Right commits
        // only from the end of the text, and is a caret move anywhere else.
        let mut shell = shell(40, 120);
        shell.route_bytes(b"/provider ll");
        shell.route_bytes(b"\x1b[D");
        assert!(!shell.editor.caret_at_end(), "Left did not move the caret");
        assert!(shell.provider_picker.is_some(), "Left closed the column");
        shell.route_bytes(b"\x1b[C");
        assert!(shell.editor.caret_at_end());
        assert!(shell.sent.try_recv().is_err(), "Right committed mid-text");
        shell.route_bytes(b"\x1b[C");
        assert_eq!(shell.picks_up(), TurnWork::Setup(ProviderId::Llmux));
        assert!(shell.editor.is_empty());
        // With no row, Right is only Right.
        let mut unmatched = self::shell(40, 120);
        unmatched.route_bytes(b"/provider zz\x1b[C");
        assert_eq!(unmatched.editor.text(), "/provider zz");
        assert!(unmatched.sent.try_recv().is_err());
    }

    #[test]
    fn while_work_is_in_hand_the_command_is_refused_and_the_column_commits_nothing() {
        // `fx-provider2-40x120-busy`: upstream's sentence, neutral, and no
        // column. A column the draft opens anyway (upstream's opens on typed
        // text too) refuses its Return the same way and keeps the text; Right
        // is a caret move.
        let mut shell = shell(40, 120);
        let _started = turn_running(&mut shell, b"ask something\r");
        let _ = shell.picks_up();
        shell.route_bytes(b"/provider\r");
        assert!(shell.provider_picker.is_none(), "the column opened");
        assert!(shell.editor.is_empty(), "the composer was reseeded");
        let busy = "* provider: Provider switching is unavailable until active and \
                    queued work finishes.";
        assert_eq!(shell.document(), vec![busy.to_string(), String::new()]);

        shell.route_bytes(b"/setup llmux");
        assert!(shell.provider_picker.is_some());
        shell.route_bytes(b"\x1b[C");
        shell.route_bytes(b"\r");
        assert_eq!(shell.editor.text(), "/setup llmux", "the text was lost");
        assert!(shell.provider_picker.is_some());
        assert!(shell.sent.try_recv().is_err(), "a switch was queued");
        assert_eq!(shell.document(), vec![busy.to_string(), String::new()]);
    }

    #[test]
    fn a_prompt_waiting_for_its_turn_is_busy_too_and_the_turns_end_frees_the_column() {
        // Upstream's rule has two facts (`provider_work_busy(stream.active,
        // queuedPromptCount())`): the second is a prompt sent whose turn has
        // not started -- no `TurnStarted` yet -- and it refuses as the first.
        let mut shell = shell(40, 120);
        shell.route_bytes(b"ask something\r");
        let _ = shell.picks_up();
        let _ = shell.document();
        shell.route_bytes(b"/provider\r");
        assert!(
            shell.provider_picker.is_none(),
            "a waiting prompt let it open"
        );
        assert!(shell
            .document()
            .iter()
            .any(|row| row.starts_with("* provider: Provider switching is unavailable")));

        // The turn starts and ends: nothing is running or waiting any more.
        shell.apply(UiEvent::TurnStarted);
        shell.apply(UiEvent::TurnEnded { failure: None });
        let _ = shell.released();
        shell.route_bytes(b"/provider\r");
        assert!(
            shell.provider_picker.is_some(),
            "the ended turn still blocked it"
        );
    }

    #[test]
    fn a_catalog_load_or_a_model_change_in_the_queue_does_not_block_the_column() {
        // **Changed claim.** The runtime's whole queue used to be the busy
        // rule; upstream's is a turn or a prompt only. A bare `/model` puts a
        // catalog load on the runtime's queue (`TurnWork::Catalog`, its menu
        // then closed with Escape) and `/model other` a `TurnWork::Model`;
        // neither starts a turn, so the column opens behind either, and the
        // switch it commits is queued after it.
        for model in [b"/model\r".as_slice(), b"/model other\r".as_slice()] {
            let mut shell = shell(40, 120);
            shell.route_bytes(model);
            if shell.model_menu.is_some() {
                escape(&mut shell);
                assert!(shell.model_menu.is_none(), "Escape left the menu up");
            }
            assert!(shell.work.outstanding() > 0, "nothing was queued");
            let _ = shell.document();
            shell.route_bytes(b"/provider\r");
            assert!(
                shell.provider_picker.is_some(),
                "{model:?} blocked the column"
            );
            shell.route_bytes(b"\x1b[B\r");
            let _ = shell.picks_up();
            assert_eq!(shell.picks_up(), TurnWork::Setup(ProviderId::Llmux));
            assert!(!shell
                .document()
                .iter()
                .any(|row| row.contains("Provider switching is unavailable")));
        }
    }

    #[test]
    fn a_provider_prefix_typed_into_the_model_menu_is_its_search_not_a_column() {
        // The model menu's search is the composer (`syncModelMenu`), as the
        // catalog's is, so `/provider ll` typed into it opens no column under
        // it; once the menu is gone the same text is the column's.
        let mut shell = shell(40, 120);
        shell.route_bytes(b"/model\r");
        assert!(shell.model_menu.is_some());
        shell.route_bytes(b"/provider ll");
        assert!(shell.provider_picker.is_none(), "a column under the menu");
        assert!(matches!(shell.slot(), Some(Slot::Models(_))));
        escape(&mut shell);
        shell.route_bytes(b"/provider ll");
        assert!(shell.provider_picker.is_some());
    }

    #[test]
    fn choosing_the_provider_in_force_is_already_using_and_hands_the_runtime_nothing() {
        // `app_auth_runtime.zig:1159-1167`: the current provider with a
        // credential it accepts is no switch. Nothing reaches the runtime, so
        // the profile is not rewritten and the conversation is not dropped.
        for typed in [
            b"/provider\r\r".as_slice(),
            b"/setup gateway\r".as_slice(),
            b"/provider ga\x1b[C".as_slice(),
        ] {
            let mut shell = shell(40, 120);
            shell.shell.missing_credential = false;
            shell.route_bytes(typed);
            assert!(
                shell.sent.try_recv().is_err(),
                "{typed:?} reached the runtime"
            );
            assert!(shell.editor.is_empty(), "{typed:?}");
            assert!(shell.provider_picker.is_none(), "{typed:?}");
            assert_eq!(
                shell.document(),
                vec![
                    "* provider: Already using Vercel AI Gateway.".to_string(),
                    String::new()
                ],
                "{typed:?}"
            );
        }
        // The same command typed past a dismissed column says the same.
        let mut dismissed = self::shell(40, 120);
        dismissed.shell.missing_credential = false;
        dismissed.route_bytes(b"/setup gateway");
        escape(&mut dismissed);
        dismissed.route_bytes(b"\r");
        assert!(dismissed.sent.try_recv().is_err());
        // And with no credential the active provider is not `current`, so
        // choosing it is setup's to record, as before.
        let mut keyless = self::shell(40, 120);
        keyless.route_bytes(b"/provider\r\r");
        assert_eq!(keyless.picks_up(), TurnWork::Setup(ProviderId::Gateway));
    }

    #[test]
    fn current_is_the_active_provider_only_while_a_credential_authorizes_it() {
        let mut shell = shell(40, 120);
        // The fixture's session has no key: nothing is current.
        shell.route_bytes(b"/provider ");
        assert!(menu(&shell).iter().all(|row| !row.contains("current")));
        // A switch that lands while the column is up moves the annotation
        // with it.
        shell.apply(UiEvent::ProviderSelected {
            provider: ProviderId::Llmux,
            model: "fable".to_string(),
            missing_credential: false,
        });
        assert_eq!(menu(&shell)[1], provider_row(12, "llmux", false, true));
        assert_eq!(menu(&shell)[0], provider_row(12, "gateway", true, false));
    }

    #[test]
    fn a_selected_provider_replaces_the_model_the_credential_fact_and_the_catalog() {
        // Everything the previous provider's session knew that is not this
        // one's. A catalog left standing would put another daemon's context
        // window under this daemon's model, and a stale credential fact would
        // go on telling a keyless local session to run `xfx setup`. Wide
        // enough that the notice is one row.
        let mut shell = shell(24, 120);
        shell.shell.missing_credential = true;
        shell.shell.model = "old".to_string();
        shell.shell.apply(UiEvent::CatalogLoaded {
            provider: ProviderId::Gateway,
            entries: vec![entry("old", Some(100_000), &[])],
        });
        shell.shell.apply(UiEvent::Usage {
            input: Some(50_000),
            output: Some(10),
        });
        assert!(shell.shell.context_meter().is_some(), "the meter was armed");

        shell.shell.apply(UiEvent::ProviderSelected {
            provider: ProviderId::Llmux,
            model: "fable".to_string(),
            missing_credential: false,
        });
        assert_eq!(shell.shell.provider, ProviderId::Llmux);
        assert_eq!(shell.shell.model, "fable");
        assert!(!shell.shell.missing_credential);
        assert!(
            shell.shell.catalog.is_empty(),
            "the old provider's catalog survived the switch"
        );
        assert_eq!(
            shell.shell.context_meter(),
            None,
            "the dropped conversation's tokens survived the switch"
        );
        let document = shell.document();
        // **Changed claim**: upstream's sentence for a finished switch
        // (`Switched to {route} with {model}.`, `app_auth_runtime.zig:1385`)
        // and xfx's fact about the conversation, rather than `[shell]`-era
        // `key=value` words.
        assert!(
            document.iter().any(|row| row
                == "* provider: Switched to llmux with fable. The next prompt starts a \
                    fresh conversation."),
            "{document:?}"
        );
    }

    #[test]
    fn a_catalog_that_arrives_with_no_menu_up_writes_nothing_and_is_kept() {
        // The catalog used to be a document notice, a row per model. It is
        // `/model`'s menu now, as upstream's is, so rows that arrive with no
        // menu up are kept -- for the next `/model` and for the context meter
        // -- and the document gets nothing.
        let mut shell = shell(24, 80);
        shell.shell.apply(UiEvent::CatalogLoaded {
            provider: ProviderId::Llmux,
            entries: vec![
                entry("fable", Some(1_000_000), &["low", "high"]),
                entry("plain", None, &[]),
            ],
        });
        assert!(shell.document().is_empty(), "{:?}", shell.document());
        assert_eq!(shell.shell.catalog.len(), 2);
        // And the next `/model` lists them without asking again: the window
        // where it was published, nothing where it was not.
        shell.route_bytes(b"/model\r");
        assert!(shell.sent.try_recv().is_err());
        let rows: Vec<String> = menu(&shell).iter().map(|row| visible_text(row)).collect();
        assert_eq!(
            rows,
            [
                "Models 2  [All]",
                "",
                "  vendor/fable  1M context",
                "  vendor/plain",
            ]
        );
    }

    #[test]
    fn usage_and_max_context_drive_one_context_meter() {
        // The numerator decides whether there is a meter and the denominator
        // decides its form. Absent is not zero: a provider that publishes no
        // usage leaves the segment off the row rather than putting a nought on
        // it, and a provider that publishes no window gets the numerator alone
        // -- upstream's `12k` (`render.zig:448-451`), which is the Gateway's
        // case -- rather than a percentage of a window nobody stated.
        //
        // **Restated for `/statusline`**: this used to say a numerator with no
        // denominator is no meter at all. The segment is shown here because
        // the user turned it on; it is off by default ([`Shell::hint_row`]).
        let mut shell = shell(24, 80);
        shell.shell.statusline.context = true;
        shell.shell.model = "fable".to_string();

        assert_eq!(shell.shell.context_meter(), None, "nothing measured yet");

        // A denominator with no numerator is not a meter.
        shell.shell.apply(UiEvent::CatalogLoaded {
            provider: ProviderId::Llmux,
            entries: vec![entry("fable", Some(200_000), &[])],
        });
        assert_eq!(shell.shell.context_meter(), None);
        assert!(!shell.hint().contains('%'), "{}", shell.hint());

        // A numerator with no denominator is the numerator, and no percentage.
        let mut bare = fixture_with_no_catalog();
        bare.shell.statusline.context = true;
        bare.shell.model = "fable".to_string();
        bare.shell.apply(UiEvent::Usage {
            input: Some(12_345),
            output: Some(9),
        });
        assert_eq!(bare.shell.context_meter(), Some((12_345, None)));
        let row = bare.hint();
        assert!(row.ends_with(" \u{b7} 12k"), "{row}");
        assert!(!row.contains('%'), "{row}");

        // Both, and exactly one segment.
        shell.shell.apply(UiEvent::Usage {
            input: Some(12_345),
            output: Some(9),
        });
        assert_eq!(shell.shell.context_meter(), Some((12_345, Some(200_000))));
        let row = shell.hint();
        assert!(row.contains("12k/200k 6%"), "{row}");
        assert_eq!(row.matches('%').count(), 1, "{row}");

        // A measurement of nought is no meter either (`render.zig:441`).
        shell.shell.apply(UiEvent::Usage {
            input: Some(0),
            output: None,
        });
        assert_eq!(shell.shell.context_meter(), None);
    }

    #[test]
    fn the_meter_counts_the_input_tokens_and_not_the_output_ones() {
        // The policy, pinned literally because it is a judgement rather than an
        // arithmetic fact. What a context meter answers is "how much of the
        // window does the next request carry", and the next request carries the
        // conversation the provider has just counted as its *input*. Adding the
        // output would count this turn's answer twice -- once as output now, and
        // again inside the input of the turn after it.
        let mut shell = shell(24, 80);
        shell.shell.statusline.context = true;
        shell.shell.model = "fable".to_string();
        shell.shell.apply(UiEvent::CatalogLoaded {
            provider: ProviderId::Llmux,
            entries: vec![entry("fable", Some(100_000), &[])],
        });
        shell.shell.apply(UiEvent::Usage {
            input: Some(30_000),
            output: Some(20_000),
        });
        assert_eq!(
            shell.shell.context_meter(),
            Some((30_000, Some(100_000))),
            "the meter added the output tokens to the input ones"
        );
        assert!(shell.hint().contains("30k/100k"), "{}", shell.hint());
    }

    #[test]
    fn a_turn_that_published_no_usage_leaves_the_meter_off() {
        let mut shell = shell(24, 80);
        shell.shell.model = "fable".to_string();
        shell.shell.apply(UiEvent::CatalogLoaded {
            provider: ProviderId::Llmux,
            entries: vec![entry("fable", Some(100_000), &[])],
        });
        shell.shell.apply(UiEvent::Usage {
            input: None,
            output: None,
        });
        assert_eq!(shell.shell.context_meter(), None);
        assert!(!shell.hint().contains('%'), "{}", shell.hint());
    }

    #[test]
    fn the_denominator_follows_the_model_in_force() {
        // The catalog is a list; the meter is about one row of it, and which row
        // is decided by the model a turn will actually talk to -- by id or by
        // alias, because the model in force is whatever the profile spelled.
        let mut shell = shell(24, 80);
        shell.shell.apply(UiEvent::CatalogLoaded {
            provider: ProviderId::Llmux,
            entries: vec![
                entry("small", Some(8_000), &[]),
                entry("large", Some(900_000), &[]),
            ],
        });
        shell.shell.apply(UiEvent::Usage {
            input: Some(4_000),
            output: None,
        });
        shell.shell.model = "small".to_string();
        assert_eq!(shell.shell.context_meter(), Some((4_000, Some(8_000))));
        shell.shell.model = "vendor/large".to_string();
        assert_eq!(
            shell.shell.context_meter(),
            Some((4_000, Some(900_000))),
            "the id spelling did not select the same row its alias does"
        );
        shell.shell.model = "absent-from-this-catalog".to_string();
        assert_eq!(
            shell.shell.context_meter(),
            Some((4_000, None)),
            "a model the catalog does not publish was given a window anyway"
        );
    }
    #[test]
    fn a_new_session_clears_the_meter_it_measured_and_keeps_the_window() {
        // `/new` drops the conversation on the runtime thread
        // (`super::worker`'s `drop_conversation`), and the meter's numerator is
        // a measurement **of that conversation**. Left standing it would report
        // the old conversation's tokens against the fresh one -- a number the
        // provider never said about the session on the screen, which is the one
        // thing this row must never do.
        //
        // The **denominator stays**. It is the model's context window, and
        // `/new` changes neither the provider nor the model: clearing it would
        // throw away a fact that is still true and cost a socket to learn again.
        let mut shell = shell(24, 80);
        shell.shell.statusline.context = true;
        shell.shell.model = "fable".to_string();
        shell.shell.apply(UiEvent::CatalogLoaded {
            provider: ProviderId::Llmux,
            entries: vec![entry("fable", Some(200_000), &[])],
        });
        shell.shell.apply(UiEvent::Usage {
            input: Some(12_345),
            output: Some(9),
        });
        assert_eq!(shell.shell.context_meter(), Some((12_345, Some(200_000))));
        assert!(shell.hint().contains("12k/200k"), "{}", shell.hint());

        shell.route_bytes(b"/new\r");
        assert_eq!(
            shell.controlled(),
            Some(TurnControl::Reset { through: 0 }),
            "the conversation drop never reached the thread that owns it"
        );

        assert_eq!(
            shell.shell.context_used, None,
            "the old conversation's tokens survived the session that ended"
        );
        assert_eq!(shell.shell.context_meter(), None);
        assert!(!shell.hint().contains('%'), "{}", shell.hint());
        assert_eq!(
            shell.shell.catalog.len(),
            1,
            "the model's context window was thrown away with the conversation"
        );
        // Both halves again, and the row is a meter again -- which is what
        // proves the denominator really was kept rather than merely unread.
        // After the runtime has said the conversation is gone: a measurement
        // arriving before that is the old conversation's.
        shell.shell.apply(UiEvent::Fresh { through: 0 });
        shell.shell.apply(UiEvent::Usage {
            input: Some(7),
            output: None,
        });
        assert_eq!(shell.shell.context_meter(), Some((7, Some(200_000))));
    }

    // -----------------------------------------------------------------------
    // `/statusline`: the picker, the argument form, and the segments
    // -----------------------------------------------------------------------

    /// A shell whose home and workspace outlive it, for the cases that write
    /// the profile.
    ///
    /// Not [`shell`]: that fixture's home is a directory deleted before the
    /// case runs, and a profile write into it would create it again, outside
    /// any test's cleanup.
    fn shell_at_home(root: &std::path::Path, rows: u16, cols: u16) -> Fixture {
        let home = root.join("home");
        let workspace = root.join("work");
        std::fs::create_dir_all(&home).expect("a home");
        std::fs::create_dir_all(&workspace).expect("a workspace");
        let (work, sent, control) = WorkHandle::detached();
        Fixture {
            shell: Shell::new(
                &config(&home, &workspace),
                crate::tui::layout::solve(rows, cols, 1).expect("a band"),
                PALETTE,
                false,
                work,
            )
            .anchored(),
            sent,
            control,
        }
    }

    /// The profile a [`shell_at_home`] under `root` writes, or `""` while it
    /// has written none.
    fn profile_at(root: &std::path::Path) -> String {
        std::fs::read_to_string(root.join("home").join(".xfx").join("settings.json"))
            .unwrap_or_default()
    }

    /// The picker's five rows as the band paints them, with `marked` the
    /// marked item's index (`cap/fx-statusline-40x120-bare.ansi`, rows 4-8).
    fn statusline_rows(line: StatusLine, marked: usize) -> Vec<String> {
        let (bright, dim, reset) = (PALETTE.subtitle(), PALETTE.dim(), PALETTE.reset());
        let item = |label: &str, on: bool, index: usize| {
            let style = if index == marked { bright } else { dim };
            let (off, on) = if on { (dim, bright) } else { (bright, dim) };
            format!(
                "  {style}{label}{reset}{}{off}off{reset}  {on}on{reset}",
                " ".repeat(13 - label.len())
            )
        };
        vec![
            format!("{bright}Status line{reset}"),
            String::new(),
            item("Context", line.context, 0),
            item("Session", line.session, 1),
            item("Workspace", line.workspace, 2),
        ]
    }

    /// The picker's key help on a screen `cols` wide, painted.
    fn statusline_keys(cols: u16) -> String {
        format!(
            "{}{}{}",
            PALETTE.dim(),
            statusline::key_help(cols),
            PALETTE.reset()
        )
    }

    /// Escape, once the decoder has waited long enough to know it is one.
    fn escape(shell: &mut Fixture) {
        shell.route_bytes(&[0x1b]);
        shell.settle_input(Instant::now() + Decoder::ESC_TIMEOUT);
    }

    fn line(context: bool, session: bool, workspace: bool) -> StatusLine {
        StatusLine {
            context,
            session,
            workspace,
        }
    }

    /// The two notices the argument form says about `item` being `on`.
    fn flipped(item: &str, on: bool) -> Vec<String> {
        [
            said(notice::Notice::neutral(
                "statusline",
                interactive::STATUSLINE_SAVED,
            )),
            said(notice::Notice::neutral(
                "statusline",
                format!("{item}: {}", if on { "on" } else { "off" }),
            )),
        ]
        .concat()
    }

    #[test]
    fn a_bare_statusline_opens_upstreams_picker_in_place_of_the_composer() {
        // `cap/fx-statusline-40x120-bare`: a rule where the composer was, the
        // title, a blank, three items with the first marked, a rule and the
        // key help -- and nothing written anywhere.
        let root = tempfile::tempdir().expect("a root");
        let mut shell = shell_at_home(root.path(), 40, 120);
        shell.route_bytes(b"/statusline\r");
        assert!(shell.document().is_empty(), "the picker wrote a notice");
        assert!(shell.statusline_menu.is_some());
        assert_eq!(shell.geometry.input_rows(), 0, "the composer kept its row");
        assert_eq!(shell.geometry.panel, 5);
        let mut expected = vec![divider(120)];
        expected.extend(statusline_rows(StatusLine::default(), 0));
        expected.push(divider(120));
        expected.push(statusline_keys(120));
        assert_eq!(shell.band_rows(), expected);
        // The caret is on the marked row, at its start.
        assert_eq!(shell.cursor(), (shell.geometry.panel_first() + 2, 0));
        assert_eq!(profile_at(root.path()), "", "opening it wrote the profile");
        assert!(
            shell.sent.try_recv().is_err(),
            "/statusline reached the runtime"
        );
    }

    #[test]
    fn the_pickers_keys_move_the_mark_flip_silently_and_close() {
        let root = tempfile::tempdir().expect("a root");
        let mut shell = shell_at_home(root.path(), 40, 120);
        shell.route_bytes(b"/statusline\r");
        let panel = |shell: &Fixture| shell.band_rows()[1..6].to_vec();

        // Down: the mark moves (`cap/fx-statusline-40x120-down1`).
        shell.route_bytes(b"\x1b[B");
        assert_eq!(panel(&shell), statusline_rows(line(false, false, false), 1));
        assert_eq!(shell.cursor(), (shell.geometry.panel_first() + 3, 0));

        // Enter: the marked item flips, the row says so, and **nothing** is
        // said in the document (`cap/fx-statusline-40x120-enter`,
        // `tui-slash-menu.test.ts:1816`). It is kept at once, partially.
        shell.route_bytes(b"\r");
        assert!(shell.statusline.session);
        assert_eq!(panel(&shell), statusline_rows(line(false, true, false), 1));
        assert!(
            shell.document().is_empty(),
            "the picker's flip said something"
        );
        assert_eq!(
            profile_at(root.path()),
            "{\n  \"statusLine\": {\n    \"session\": true\n  }\n}\n"
        );

        // Right and Left flip it too: two values, so either way is a flip.
        shell.route_bytes(b"\x1b[C");
        assert!(!shell.statusline.session);
        shell.route_bytes(b"\x1b[D");
        assert!(shell.statusline.session);
        assert!(shell.document().is_empty());
        assert!(profile_at(root.path()).contains("\"session\": true"));

        // Ctrl-K and Up go up, Ctrl-J and Down go down, wrapping both ways.
        shell.route_bytes(&[0x0b]);
        assert_eq!(panel(&shell), statusline_rows(line(false, true, false), 0));
        shell.route_bytes(b"\x1b[A");
        assert_eq!(panel(&shell), statusline_rows(line(false, true, false), 2));
        shell.route_bytes(&[0x0a]);
        assert_eq!(panel(&shell), statusline_rows(line(false, true, false), 0));

        // Everything else is swallowed: no letter reaches the hidden composer,
        // Ctrl-D does not leave, Ctrl-C does not reach the runtime, and Tab and
        // Backspace do nothing (`routeActiveModalInput`).
        shell.route_bytes(b"abc\t\x7f\x04\x03");
        assert!(shell.editor.is_empty(), "{:?}", shell.editor.text());
        assert!(!shell.leaving(), "Ctrl-D left from behind the picker");
        assert_eq!(shell.controlled(), None);
        assert!(shell.statusline_menu.is_some());
        assert!(shell.document().is_empty());

        // Escape: the picker is gone, the composer is back, and the clear
        // gesture was not armed by it (`cap/fx-statusline-40x120-esc`).
        escape(&mut shell);
        assert!(shell.statusline_menu.is_none());
        assert_eq!(shell.geometry.input_rows(), 1);
        assert_eq!(shell.geometry.panel, 0);
        assert_eq!(shell.hint(), IDLE_HINT);
        assert!(shell.document().is_empty());
    }

    #[test]
    fn the_argument_form_flips_one_item_and_says_both_notices() {
        // `cap/fx-statusline-40x120-context`: the saved line, a blank, the
        // item's new value -- and the footer unchanged until a turn has
        // measured something.
        let root = tempfile::tempdir().expect("a root");
        let mut shell = shell_at_home(root.path(), 40, 120);
        shell.route_bytes(b"/statusline context\r");
        assert_eq!(shell.document(), flipped("context", true));
        assert!(shell.statusline.context);
        assert_eq!(
            profile_at(root.path()),
            "{\n  \"statusLine\": {\n    \"context\": true\n  }\n}\n"
        );
        assert_eq!(shell.hint(), IDLE_HINT, "a meter before a measurement");
        // The Gateway publishes no window, so the numerator stands alone
        // (`fx-statusline-turn-40x120-after-turn`: `· 1k ·`).
        shell.apply(UiEvent::Usage {
            input: Some(1_200),
            output: None,
        });
        assert_eq!(shell.hint(), format!("{IDLE_HINT} \u{b7} 1k"));

        // Again, and it is off, kept off, and gone from the row.
        shell.route_bytes(b"/statusline context\r");
        assert_eq!(shell.document(), flipped("context", false));
        assert!(!shell.statusline.context);
        assert!(profile_at(root.path()).contains("\"context\": false"));
        assert_eq!(shell.hint(), IDLE_HINT);
        assert!(
            shell.statusline_menu.is_none(),
            "the argument form opened it"
        );
    }

    #[test]
    fn an_argument_that_names_no_item_is_upstreams_usage_error() {
        // `cap/fx-statusline-40x120-bogus`, and the match is exact and
        // case-sensitive (`parseStatuslineItem`).
        let root = tempfile::tempdir().expect("a root");
        let mut shell = shell_at_home(root.path(), 40, 120);
        for argument in ["bogus", "Context", "context please", "sandbox"] {
            shell.route_bytes(format!("/statusline {argument}\r").as_bytes());
            assert_eq!(
                shell.document(),
                said(notice::Notice::error(
                    "",
                    "usage: /statusline [context|session|workspace]"
                )),
                "{argument:?}"
            );
        }
        assert_eq!(shell.statusline, StatusLine::default());
        assert_eq!(profile_at(root.path()), "", "a refusal wrote the profile");
    }

    #[test]
    fn the_segments_follow_the_model_in_upstreams_order_and_only_when_on() {
        // `buildHintLine` (`render.zig:440-470`), and
        // `fx-statusline-turn-40x120-after-turn`: `auto · model-a · say the
        // marker · 1k · <workspace>`. Off, none of them is there whatever the
        // session knows.
        let root = tempfile::tempdir().expect("a root");
        let mut shell = shell_at_home(root.path(), 40, 160);
        shell.session_title = Some("say the marker".to_string());
        shell.apply(UiEvent::Usage {
            input: Some(1_200),
            output: None,
        });
        assert_eq!(shell.hint(), IDLE_HINT, "a segment was on by default");

        let workspace = shell.workspace.to_string_lossy().into_owned();
        shell.route_bytes(b"/statusline session\r");
        assert_eq!(shell.hint(), format!("{IDLE_HINT} \u{b7} say the marker"));
        shell.route_bytes(b"/statusline workspace\r");
        assert_eq!(
            shell.hint(),
            format!("{IDLE_HINT} \u{b7} say the marker \u{b7} {workspace}")
        );
        shell.route_bytes(b"/statusline context\r");
        assert_eq!(
            shell.hint(),
            format!("{IDLE_HINT} \u{b7} say the marker \u{b7} 1k \u{b7} {workspace}")
        );
        // A title is the first line of a prompt: what a terminal would obey
        // in it is not painted.
        shell.session_title = Some("say \u{1b}[31mred".to_string());
        assert!(
            shell.hint().contains(" \u{b7} say [31mred \u{b7} "),
            "{}",
            shell.hint()
        );
    }

    #[test]
    fn the_branch_is_read_at_launch_and_again_at_the_end_of_a_turn() {
        // `tui-startup.test.ts:175-240`: the nearest `HEAD`, read again when
        // something can have moved it -- here a turn, which is what runs `git
        // checkout` under a session -- and not on every frame.
        let root = tempfile::tempdir().expect("a root");
        let git = root.path().join("work").join(".git");
        std::fs::create_dir_all(&git).expect(".git");
        std::fs::write(git.join("HEAD"), "ref: refs/heads/first\n").expect("HEAD");
        std::fs::create_dir_all(root.path().join("home").join(".xfx")).expect("profile home");
        std::fs::write(
            root.path().join("home").join(".xfx").join("settings.json"),
            "{\"statusLine\":{\"workspace\":true}}\n",
        )
        .expect("a profile");
        let mut shell = shell_at_home(root.path(), 40, 160);
        let workspace = shell.workspace.to_string_lossy().into_owned();
        assert_eq!(
            shell.hint(),
            format!("{IDLE_HINT} \u{b7} {workspace} (first)"),
            "the profile's segment was not on at launch, or its branch unread"
        );
        std::fs::write(git.join("HEAD"), "ref: refs/heads/second\n").expect("HEAD");
        assert!(shell.hint().ends_with("(first)"), "read on a frame");
        shell.apply(UiEvent::TurnEnded { failure: None });
        assert_eq!(
            shell.hint(),
            format!("{IDLE_HINT} \u{b7} {workspace} (second)")
        );
    }

    #[test]
    fn what_the_picker_and_the_argument_write_is_what_the_next_launch_reads() {
        // The round trip, through the real loader in the same home: the three
        // writes land in one object, and a session started from it shows all
        // three on -- in its footer and in its picker.
        let root = tempfile::tempdir().expect("a root");
        let mut shell = shell_at_home(root.path(), 40, 120);
        shell.route_bytes(b"/statusline context\r");
        shell.route_bytes(b"/statusline workspace\r");
        shell.route_bytes(b"/statusline\r");
        shell.route_bytes(b"\x1b[B\r");
        escape(&mut shell);
        let written: serde_json::Value =
            serde_json::from_str(&profile_at(root.path())).expect("the profile is JSON");
        assert_eq!(
            written,
            serde_json::json!({"statusLine": {"context": true, "session": true, "workspace": true}})
        );

        let reloaded = config(&root.path().join("home"), &root.path().join("work"));
        assert_eq!(reloaded.statusline, line(true, true, true));
        assert!(
            reloaded.diagnostics.is_empty(),
            "{:?}",
            reloaded.diagnostics
        );
        let mut next = shell_at_home(root.path(), 40, 120);
        assert_eq!(next.statusline, line(true, true, true));
        next.route_bytes(b"/statusline\r");
        assert_eq!(
            next.band_rows()[1..6].to_vec(),
            statusline_rows(line(true, true, true), 0)
        );
    }

    #[test]
    fn a_write_that_fails_keeps_the_change_for_this_process_and_says_so() {
        // `reportUserSettingsFailure` (`session_commands.zig:110-133`): the
        // segment is on for this session either way, and the failure is said
        // on both paths -- the picker is silent only about success.
        let root = tempfile::tempdir().expect("a root");
        let settings = root.path().join("home").join(".xfx").join("settings.json");
        std::fs::create_dir_all(settings.parent().expect("a parent")).expect("profile home");
        std::fs::write(&settings, "{not json").expect("an unreadable profile");
        let mut shell = shell_at_home(root.path(), 40, 200);
        let not_saved = |document: &[String]| {
            document.first().is_some_and(|row| {
                row.starts_with(
                    "\u{2717} statusline: active for this process but not saved to user \
                     settings (it is not valid JSON",
                )
            })
        };

        shell.route_bytes(b"/statusline context\r");
        let document = shell.document();
        assert!(not_saved(&document), "{document:?}");
        assert_eq!(document[2], "* statusline: context: on", "{document:?}");
        assert!(shell.statusline.context);

        shell.route_bytes(b"/statusline\r");
        shell.route_bytes(b"\r");
        let document = shell.document();
        assert!(not_saved(&document), "{document:?}");
        assert_eq!(document.len(), 2, "the picker said more than the failure");
        assert!(!shell.statusline.context, "the flip was not in force");
        assert_eq!(
            std::fs::read_to_string(&settings).expect("read"),
            "{not json",
            "the operator's file was written over"
        );
    }

    #[test]
    fn a_short_screen_gives_the_picker_fewer_rows_rather_than_refusing_it() {
        // Seven rows hold the hint, two rules, one row of document and three
        // of the picker: the title, the blank and the marked item.
        let root = tempfile::tempdir().expect("a root");
        let mut shell = shell_at_home(root.path(), 7, 80);
        shell.route_bytes(b"/statusline\r");
        assert_eq!(shell.geometry.panel, 3);
        let rows = uncoloured_rows(&shell.band_rows());
        assert_eq!(rows[1..4], ["Status line", "", "  Context      off  on"]);
        shell.route_bytes(b"\x1b[B");
        let rows = uncoloured_rows(&shell.band_rows());
        assert_eq!(rows[3], "  Session      off  on");
        assert_eq!(shell.cursor(), (shell.geometry.panel_first() + 2, 0));
    }

    // -----------------------------------------------------------------------
    // walking back through what has been submitted
    // -----------------------------------------------------------------------

    /// Clears what a submitted line leaves behind, so the next submission is
    /// not refused by a queue with the last one still in it.
    ///
    /// The runtime holds one piece of work and one behind it
    /// (`super::worker::WORK_LIMIT`), and a case here that sent three lines
    /// without taking any of them would be a case about `Rejected::Busy`
    /// instead of about the recall.
    fn submitted(shell: &mut Fixture, line: &str) {
        shell.route_bytes(line.as_bytes());
        shell.route_bytes(&[0x0d]);
        let _echo = shell.document();
        let _taken = shell.sent.try_recv();
    }

    #[test]
    fn ctrl_p_walks_back_through_the_submitted_lines_and_ctrl_n_returns_the_draft() {
        // The whole of item 15 on this surface: what was sent comes back,
        // newest first, and the half-typed line the walk began from is what it
        // comes back to.
        let mut shell = shell(24, 80);
        submitted(&mut shell, "first line");
        submitted(&mut shell, "second line");
        shell.route_bytes(b"half typed");

        shell.route_bytes(&[0x10]);
        assert_eq!(shell.band_rows()[0], composer_row("second line"));
        assert_eq!(
            shell.cursor(),
            (22, 13),
            "the caret is not at the end of the recalled line"
        );
        shell.route_bytes(&[0x10]);
        assert_eq!(shell.band_rows()[0], composer_row("first line"));
        shell.route_bytes(&[0x10]);
        assert_eq!(
            shell.band_rows()[0],
            composer_row("first line"),
            "the walk wrapped past the oldest line"
        );
        shell.route_bytes(&[0x0e]);
        assert_eq!(shell.band_rows()[0], composer_row("second line"));
        shell.route_bytes(&[0x0e]);
        assert_eq!(
            shell.band_rows()[0],
            composer_row("half typed"),
            "the draft the walk began from was thrown away"
        );
    }

    #[test]
    fn a_recall_asks_for_the_frame_that_shows_it_and_a_recall_of_nothing_asks_for_none() {
        // The band is repainted whole, so a recall that did not ask for a frame
        // would leave the terminal showing a draft the composer no longer
        // holds; and a `C-p` at a session with nothing to recall must not
        // repaint the band on a link that may be a serial line.
        let mut shell = shell(24, 80);
        let _first = shell.render.begin().expect("the first frame");
        shell.route_bytes(&[0x10]);
        assert!(
            shell.render.begin().is_none(),
            "a recall that recalled nothing repainted the whole band"
        );
        submitted(&mut shell, "sent");
        let _submission = shell.render.begin();
        shell.route_bytes(&[0x10]);
        assert!(
            shell.render.begin().is_some(),
            "a recall asked for no frame, so the band would go on showing the \
             draft the composer no longer holds"
        );
    }

    #[test]
    fn a_submitted_command_is_walked_back_to_like_any_other_line() {
        // A command is a line the user typed, and the commonest reason to
        // reach for the recall is to run one again. It is recorded before the
        // draft is consumed, which is the only moment its text still exists.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"/version\r");
        let _echo = shell.document();
        shell.route_bytes(&[0x10]);
        // Asked through the caret rather than through a band row: the recalled
        // draft is a slash word, so the completion menu opens over the rows
        // above the rule -- and the caret staying in the composer is the same
        // claim the menu's own cases make.
        assert_eq!(shell.marked(), composer_row("/version"));
    }

    #[test]
    fn a_line_the_runtime_refused_is_still_walked_back_to() {
        // The refusal leaves the draft in the composer and says so on the hint
        // row, so the line was submitted -- and a history that recorded only
        // what the runtime accepted would forget exactly the line the user is
        // most likely to want back.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"first\r");
        let _first = shell.picks_up();
        shell.route_bytes(b"second\r");
        let _ = shell.document();
        shell.route_bytes(b"third\r");
        assert_eq!(
            shell.shell.notice,
            Some(QUEUE_REJECTED),
            "the third line was not refused, so this case proves nothing"
        );
        assert_eq!(
            shell.band_rows()[0],
            composer_row("third"),
            "a refused submission threw the draft away"
        );
        // Cleared first, or the draft the refusal left standing would satisfy
        // this case without a single line having been recorded.
        shell.route_bytes(b"\x15");
        assert_eq!(shell.band_rows()[0], composer_row(""));
        shell.route_bytes(&[0x10]);
        assert_eq!(shell.band_rows()[0], composer_row("third"));
    }

    #[test]
    fn a_blank_line_is_never_walked_back_to() {
        // Return on whitespace consumes the line and writes nothing, so there
        // is nothing to come back to -- and an entry for it would put a press
        // of `C-p` between the user and the line they really sent.
        let mut shell = shell(24, 80);
        submitted(&mut shell, "real");
        submitted(&mut shell, "   ");
        shell.route_bytes(&[0x10]);
        assert_eq!(shell.band_rows()[0], composer_row("real"));
    }

    #[test]
    fn the_up_arrow_moves_the_caret_until_the_first_row_and_only_then_walks_back() {
        // The edge rule. An arrow inside a multi-row draft is the movement it
        // is in every editor; it becomes the recall exactly where it would
        // otherwise do nothing at all.
        let mut shell = shell(24, 80);
        submitted(&mut shell, "sent");
        shell.route_bytes(b"top\x0abottom");
        assert_eq!(
            &shell.band_rows()[0..2],
            &[composer_row("top"), composer_row("bottom")]
        );
        assert_eq!(
            shell.marked(),
            composer_row("bottom"),
            "the caret did not start on the draft's last row"
        );

        shell.route_bytes(b"\x1b[A");
        assert_eq!(
            shell.marked(),
            composer_row("top"),
            "the arrow did not move the caret up a row of the draft"
        );
        assert_eq!(
            &shell.band_rows()[0..2],
            &[composer_row("top"), composer_row("bottom")],
            "an arrow inside the draft walked the history instead of the rows"
        );

        shell.route_bytes(b"\x1b[A");
        assert_eq!(
            shell.band_rows()[0],
            composer_row("sent"),
            "the arrow at the first row did not reach the history"
        );
    }

    #[test]
    fn the_down_arrow_moves_the_caret_until_the_last_row_and_only_then_walks_forward() {
        let mut shell = shell(24, 80);
        submitted(&mut shell, "sent");
        shell.route_bytes(b"half typed");
        shell.route_bytes(b"\x1b[A");
        assert_eq!(
            shell.band_rows()[0],
            composer_row("sent"),
            "the arrow at a one-row draft's only row did not reach the history"
        );
        shell.route_bytes(b"\x1b[B");
        assert_eq!(
            shell.band_rows()[0],
            composer_row("half typed"),
            "the arrow at the last row did not bring the draft back"
        );

        // And inside a taller draft it is a caret move at every row but the
        // last, and a walk with nothing in it at the last.
        shell.route_bytes(b"\x15");
        shell.route_bytes(b"top\x0abottom");
        shell.route_bytes(b"\x1b[A");
        shell.route_bytes(b"\x1b[B");
        assert_eq!(
            shell.marked(),
            composer_row("bottom"),
            "the arrow did not move back down"
        );
        assert_eq!(
            &shell.band_rows()[0..2],
            &[composer_row("top"), composer_row("bottom")]
        );
        shell.route_bytes(b"\x1b[B");
        assert_eq!(
            &shell.band_rows()[0..2],
            &[composer_row("top"), composer_row("bottom")],
            "a Down at the last row of a draft nobody had recalled into changed it"
        );
    }

    #[test]
    fn ctrl_p_walks_back_from_a_row_the_arrow_would_only_move_in() {
        // The difference between the two keys, and the reason there are two:
        // the caret is on the last of two rows, so `Up` there is a movement --
        // and `C-p` is the recall wherever the caret is.
        let mut shell = shell(24, 80);
        submitted(&mut shell, "sent");
        shell.route_bytes(b"top\x0abottom");
        shell.route_bytes(&[0x10]);
        assert_eq!(
            shell.band_rows()[0],
            composer_row("sent"),
            "C-p from the last row of a two-row draft only moved the caret"
        );
    }

    #[test]
    fn typing_after_a_recall_starts_the_next_walk_at_the_newest_line() {
        // An edit means the line on the screen is the user's own now, so the
        // walk that produced it is over -- and the edited line is what the next
        // walk comes back to.
        let mut shell = shell(24, 80);
        submitted(&mut shell, "first");
        submitted(&mut shell, "second");
        shell.route_bytes(&[0x10]);
        shell.route_bytes(&[0x10]);
        assert_eq!(shell.band_rows()[0], composer_row("first"));

        shell.route_bytes(b"!");
        assert_eq!(shell.band_rows()[0], composer_row("first!"));
        shell.route_bytes(&[0x10]);
        assert_eq!(
            shell.band_rows()[0],
            composer_row("second"),
            "the walk carried on from the line the edit had left behind"
        );
        shell.route_bytes(&[0x0e]);
        assert_eq!(
            shell.band_rows()[0],
            composer_row("first!"),
            "the edited line was not what the walk came back to"
        );
    }

    #[test]
    fn a_deletion_after_a_recall_leaves_the_walk_too() {
        // The other half of "an edit leaves the walk": a backspace changes the
        // text as surely as a keystroke does, and a rule that only watched for
        // typing would leave the session walking a list its composer had
        // stopped agreeing with.
        let mut shell = shell(24, 80);
        submitted(&mut shell, "first");
        submitted(&mut shell, "second");
        shell.route_bytes(&[0x10]);
        shell.route_bytes(&[0x10]);
        assert_eq!(shell.band_rows()[0], composer_row("first"));
        shell.route_bytes(&[0x7f]);
        assert_eq!(shell.band_rows()[0], composer_row("firs"));
        shell.route_bytes(&[0x10]);
        assert_eq!(
            shell.band_rows()[0],
            composer_row("second"),
            "a deletion left the walk where it was"
        );
    }

    #[test]
    fn a_caret_move_after_a_recall_keeps_the_walk_open() {
        // The mirror of the two cases above, and the one an implementation that
        // simply left the walk on every keystroke would fail: moving the caret
        // through a recalled line is how a user reads it before deciding to
        // step further back.
        let mut shell = shell(24, 80);
        submitted(&mut shell, "first");
        submitted(&mut shell, "second");
        shell.route_bytes(&[0x10]);
        assert_eq!(shell.band_rows()[0], composer_row("second"));
        shell.route_bytes(b"\x1b[D");
        shell.route_bytes(&[0x10]);
        assert_eq!(
            shell.band_rows()[0],
            composer_row("first"),
            "a caret move ended the walk, so the recall began again at the newest line"
        );
    }

    #[test]
    fn a_double_escape_after_a_recall_leaves_the_walk_too() {
        // The gesture that throws the whole draft away is a change to the
        // composer's text like any other, so the walk it was made during is
        // over. A rule that watched only for keystrokes that *add* text would
        // leave the session walking from a position its empty composer had
        // stopped agreeing with.
        let mut shell = shell(24, 80);
        submitted(&mut shell, "first");
        submitted(&mut shell, "second");
        shell.route_bytes(&[0x10]);
        shell.route_bytes(&[0x10]);
        assert_eq!(shell.band_rows()[0], composer_row("first"));

        shell.route_bytes(&[0x1b]);
        shell.settle_input(Instant::now() + Decoder::ESC_TIMEOUT);
        shell.route_bytes(&[0x1b]);
        shell.settle_input(Instant::now() + Decoder::ESC_TIMEOUT);
        assert!(
            shell.editor.is_empty(),
            "the second Escape did not clear the recalled line"
        );

        shell.route_bytes(&[0x10]);
        assert_eq!(
            shell.band_rows()[0],
            composer_row("second"),
            "the cleared composer went on walking from where the recall had reached"
        );
    }

    #[test]
    fn an_inline_paste_after_a_recall_leaves_the_walk_too() {
        // A paste small enough to land as text is an edit, and the path it
        // takes into the composer is not the one a keystroke takes.
        let mut shell = shell(24, 80);
        submitted(&mut shell, "first");
        submitted(&mut shell, "second");
        shell.route_bytes(&[0x10]);
        shell.route_bytes(&[0x10]);
        assert_eq!(shell.band_rows()[0], composer_row("first"));

        shell.route_bytes(b"\x1b[200~");
        shell.route_bytes(b" and more");
        shell.route_bytes(b"\x1b[201~");
        assert_eq!(shell.band_rows()[0], composer_row("first and more"));

        shell.route_bytes(&[0x10]);
        assert_eq!(
            shell.band_rows()[0],
            composer_row("second"),
            "a pasted edit left the walk where it was"
        );
    }

    #[test]
    fn a_forward_delete_after_a_recall_leaves_the_walk_too() {
        // Ctrl-D with text under the caret is the forward delete rather than
        // the end of the session, and it reaches the editor by a path of its
        // own -- so it owes the same thing every other edit owes.
        let mut shell = shell(24, 80);
        submitted(&mut shell, "first");
        submitted(&mut shell, "second");
        shell.route_bytes(&[0x10]);
        shell.route_bytes(&[0x10]);
        assert_eq!(shell.band_rows()[0], composer_row("first"));

        // Home first, which is a caret move and must leave the walk open --
        // otherwise this case would be about the move rather than the delete.
        shell.route_bytes(&[0x01]);
        shell.route_bytes(&[0x04]);
        assert_eq!(shell.band_rows()[0], composer_row("irst"));

        shell.route_bytes(&[0x10]);
        assert_eq!(
            shell.band_rows()[0],
            composer_row("second"),
            "a forward delete left the walk where it was"
        );
    }

    #[test]
    fn a_collapsed_paste_after_a_recall_leaves_the_walk_too() {
        // The other paste path: past `COLLAPSE_ABOVE` codepoints the composer
        // is given a summary instead of the text, and that is still an edit to
        // the draft.
        let mut shell = shell(24, 80);
        submitted(&mut shell, "first");
        submitted(&mut shell, "second");
        shell.route_bytes(&[0x10]);
        shell.route_bytes(&[0x10]);
        assert_eq!(shell.band_rows()[0], composer_row("first"));

        shell.route_bytes(b"\x1b[200~");
        shell.route_bytes("y".repeat(1200).as_bytes());
        shell.route_bytes(b"\x1b[201~");
        assert_eq!(
            shell.band_rows()[0],
            composer_row("first[Pasted text #1, 1 lines]")
        );

        shell.route_bytes(&[0x10]);
        assert_eq!(
            shell.band_rows()[0],
            composer_row("second"),
            "a collapsed paste left the walk where it was"
        );
    }

    #[test]
    fn an_arrow_at_the_edge_with_nothing_to_recall_still_asks_for_its_frame() {
        // What the arrow key did before it could reach the history, and must
        // go on doing: the keystroke was applied even though the caret could
        // not move -- the run of vertical motion recorded the column it is
        // aiming for -- so the band is still owed the frame that keystroke
        // asked for.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"one row");
        let _typed = shell.render.begin().expect("the frame the typing owed");
        shell.route_bytes(b"\x1b[A");
        assert!(
            shell.render.begin().is_some(),
            "an arrow at the edge of a draft with nothing behind it asked for \
             no frame at all"
        );
    }

    #[test]
    fn a_paste_the_walk_stood_aside_comes_back_as_the_block_with_a_new_number() {
        // **The narrowing item 15 recorded, closed.** A walk captures the
        // half-typed draft and hands it back at the near end, and until item 16
        // what came back was the summary's *words*: the block died with the
        // draft the recall replaced, so the line the user had been composing
        // would have been sent as its own description.
        //
        // The draft now travels with its blocks, and what comes back is
        // renumbered like any other recall -- the number a user reads is one
        // this session has minted once.
        let mut shell = shell(24, 80);
        submitted(&mut shell, "earlier");
        let block = "y".repeat(1200);
        shell.route_bytes(b"\x1b[200~");
        shell.route_bytes(block.as_bytes());
        shell.route_bytes(b"\x1b[201~");
        assert_eq!(
            shell.band_rows()[0],
            composer_row("[Pasted text #1, 1 lines]"),
            "the paste was not collapsed, so this case proves nothing"
        );

        shell.route_bytes(&[0x10]);
        assert_eq!(shell.band_rows()[0], composer_row("earlier"));
        shell.route_bytes(&[0x0e]);
        assert_eq!(
            shell.band_rows()[0],
            composer_row("[Pasted text #2, 1 lines]"),
            "the draft the walk began from did not come back, or came back \
             under a number this session had already used"
        );

        shell.route_bytes(&[0x0d]);
        assert_eq!(
            shell.picks_up(),
            TurnWork::Submit(block),
            "the block the walk stood aside was sent as the words it looks like"
        );
    }

    #[test]
    fn a_block_does_not_survive_into_a_line_the_walk_hands_back() {
        // The sharp half of releasing the blocks on a recall, and the half
        // `Paste::reconcile` cannot reach: a summary's words are on the screen
        // where anyone can read and type them, so a **recorded line** can
        // carry them. Reconciling would find that name in the draft the recall
        // installed and keep the block alive, and the recalled line would then
        // be expanded into megabytes nobody pasted into it.
        let mut shell = shell(24, 80);
        submitted(&mut shell, "[Pasted text #1, 1 lines]");
        let block = "y".repeat(1200);
        shell.route_bytes(b"\x1b[200~");
        shell.route_bytes(block.as_bytes());
        shell.route_bytes(b"\x1b[201~");
        assert_eq!(
            shell.band_rows()[0],
            composer_row("[Pasted text #1, 1 lines]"),
            "the paste was not collapsed, so this case proves nothing"
        );

        shell.route_bytes(&[0x10]);
        assert_eq!(
            shell.band_rows()[0],
            composer_row("[Pasted text #1, 1 lines]"),
            "the recalled line is not the one whose words match the summary"
        );
        shell.route_bytes(&[0x0d]);
        assert_eq!(
            shell.picks_up(),
            TurnWork::Submit("[Pasted text #1, 1 lines]".to_string()),
            "a block survived the recall and expanded into the line the walk handed back"
        );
    }

    #[test]
    fn a_recalled_line_is_sent_as_itself() {
        // The end of the gesture, and the claim the band rows above cannot
        // make: what the runtime is given is the recalled line, and submitting
        // it records it once rather than twice.
        let mut shell = shell(24, 80);
        submitted(&mut shell, "ask me again");
        shell.route_bytes(&[0x10]);
        shell.route_bytes(&[0x0d]);
        assert_eq!(
            shell.picks_up(),
            TurnWork::Submit("ask me again".to_string())
        );
        let _echo = shell.document();
        shell.route_bytes(&[0x10]);
        assert_eq!(shell.band_rows()[0], composer_row("ask me again"));
        shell.route_bytes(&[0x10]);
        assert_eq!(
            shell.band_rows()[0],
            composer_row("ask me again"),
            "the same line sent twice running became two entries"
        );
    }

    // -----------------------------------------------------------------------
    // the amendment draft, at the shell
    // -----------------------------------------------------------------------

    /// Types `text` at whatever has the focus, one decoded character at a time.
    fn type_at_the_question(shell: &mut Fixture, text: &str) {
        for character in text.chars() {
            shell.decide(Input::Text(character), Instant::now());
        }
    }

    /// A shell with a question up, a committed frame behind it, and the deny
    /// draft open with `text` in it.
    ///
    /// Two Downs rather than a digit, because a digit answers: the marker walks
    /// to `3. No` and Tab opens that side's amendment.
    fn a_filled_deny_draft(shell: &mut Fixture, text: &str) {
        asking(shell);
        shell.decide(Input::Action(Action::Down), Instant::now());
        shell.decide(Input::Action(Action::Down), Instant::now());
        shell.decide(Input::Action(Action::Tab), Instant::now());
        type_at_the_question(shell, text);
    }

    /// The rows of the band with the palette taken off, joined.
    fn band(shell: &Fixture) -> String {
        unwrapped(&uncoloured_rows(&shell.shell.band_rows()))
    }

    #[test]
    fn up_and_down_move_the_choice_whether_or_not_a_draft_is_open() {
        // Four cases, because `input_approval_runtime.zig:127-141` maps
        // `cursor_up`/`history_up` to one action and `cursor_down`/`history_down`
        // to another **regardless of whether an amendment is open**. The shell
        // collapses each pair at the translation rather than giving the panel
        // four keys to keep in step -- and neither spelling may ever reach the
        // editor, because they are the only way out of a draft.
        for open in [false, true] {
            for (up, down) in [
                (Action::Up, Action::Down),
                (Action::HistoryPrevious, Action::HistoryNext),
            ] {
                let mut shell = shell(24, 80);
                asking(&mut shell);
                if open {
                    shell.decide(Input::Action(Action::Tab), Instant::now());
                    type_at_the_question(&mut shell, "y");
                }
                shell.decide(Input::Action(down), Instant::now());
                assert!(
                    shell.marked().contains("❯ 2  Apply + allow"),
                    "open={open} {down:?}: the marker did not move: {:?}",
                    shell.marked()
                );
                shell.decide(Input::Action(up), Instant::now());
                assert!(
                    shell.marked().contains("❯ 1  Apply once"),
                    "open={open} {up:?}: the marker did not come back: {:?}",
                    shell.marked()
                );
                // And nothing of either key reached the draft: what was typed
                // is still exactly what was typed.
                if open {
                    assert!(
                        band(&shell).contains("Apply once, y "),
                        "an arrow was routed into the editor: {:?}",
                        band(&shell)
                    );
                }
            }
        }
    }

    #[test]
    fn opening_a_draft_revokes_a_committed_readiness_receipt() {
        // The screen that was proven is no longer the screen the user is
        // looking at: the panel just grew a row. `invalidate` clears the
        // previously-seen disclosure as well as the receipt
        // (`super::super::approval_readiness::Readiness::invalidate`), and the
        // next genuinely disclosed frame re-earns it -- which is the intended
        // cost rather than a gap.
        let mut shell = shell(24, 80);
        asking(&mut shell);
        assert!(
            shell.approval_ready(ApprovalId(1)),
            "the fixture never earned a receipt, so this case proves nothing"
        );

        shell.decide(Input::Action(Action::Tab), Instant::now());
        assert!(
            !shell.approval_ready(ApprovalId(1)),
            "a draft opened on top of a proven screen and the receipt survived"
        );

        // The affirmative is refused until it is re-earned, and **the draft is
        // untouched by the refusal**: the question is still up and the sentence
        // is still the user's.
        type_at_the_question(&mut shell, "only the one file");
        // Enter rather than `1`: with a draft open a digit is a character
        // (`digits_typed_into_a_draft_are_characters_and_digits_outside_one_are_answers`),
        // and Submit is the affirmative that is still an affirmative there.
        shell.decide(Input::Action(Action::Submit), Instant::now());
        assert_eq!(
            shell.controlled(),
            None,
            "an affirmative was taken on a screen no frame had disclosed"
        );
        assert!(
            band(&shell).contains("only the one file"),
            "the refused keystroke took the draft with it: {:?}",
            band(&shell)
        );

        // Re-earned, and now the same key answers -- carrying the sentence.
        delivered(&mut shell);
        shell.decide(Input::Action(Action::Submit), Instant::now());
        assert_eq!(
            shell.controlled(),
            Some(TurnControl::Answer {
                id: ApprovalId(1),
                answer: ApprovalAnswer::Once,
                feedback: Some("only the one file".to_string()),
            }),
            "the re-earned frame did not let the amended answer through"
        );
    }

    #[test]
    fn ctrl_c_at_a_filled_deny_draft_refuses_without_the_sentence_but_enter_sends_it() {
        // The pair, on the wire, in one case so it cannot drift. Same draft
        // text, two exits. (Renamed: the refusal that says nothing is Ctrl-C's
        // now, `approval_decision.zig:132-135`; Escape cancels the turn.)
        let mut escaped = shell(24, 80);
        a_filled_deny_draft(&mut escaped, "never touch the fixtures");
        escaped.decide(Input::Action(Action::Cancel), Instant::now());
        assert_eq!(
            escaped.controlled(),
            Some(TurnControl::Answer {
                id: ApprovalId(1),
                answer: ApprovalAnswer::Deny,
                feedback: None,
            }),
            "a user who bailed out of the panel was made to say the sentence in it"
        );

        let mut submitted = shell(24, 80);
        a_filled_deny_draft(&mut submitted, "never touch the fixtures");
        submitted.decide(Input::Action(Action::Submit), Instant::now());
        assert_eq!(
            submitted.controlled(),
            Some(TurnControl::Answer {
                id: ApprovalId(1),
                answer: ApprovalAnswer::Deny,
                feedback: Some("never touch the fixtures".to_string()),
            }),
            "a chosen refusal did not carry its reason"
        );
    }

    #[test]
    fn escape_at_a_draft_cancels_the_turn_and_sends_no_feedback() {
        // One message, both meanings, and the sentence in neither. The prompter
        // turns the cancellation into the refusal on the far side
        // (`super::super::approval::TuiPrompter`), so an `Answer` sent here as
        // well would be the only thing the runtime heard -- and the turn would
        // go on running after the user asked for it to stop. (Renamed: this
        // was Ctrl-C's before divergence 7.)
        let mut shell = shell(24, 80);
        a_filled_deny_draft(&mut shell, "stop, this is wrong");
        shell.decide(Input::Action(Action::Escape), Instant::now());

        let mut said = Vec::new();
        while let Some(control) = shell.controlled() {
            said.push(control);
        }
        assert!(
            said.iter()
                .all(|control| !matches!(control, TurnControl::Answer { .. })),
            "the interrupt sent a second message the prompter would read as an answer: {said:?}"
        );
        assert!(
            said.iter()
                .any(|control| matches!(control, TurnControl::Cancel { .. })),
            "the interrupt did not stop the turn the question belonged to: {said:?}"
        );
    }

    #[test]
    fn a_cancelled_draft_does_not_reach_the_next_request() {
        // The leak test. Both drafts are dropped with the question, so the
        // second question is asked at a panel with nothing in it -- and an
        // answer to it carries nothing.
        let mut shell = shell(24, 80);
        a_filled_deny_draft(&mut shell, "stop, this is wrong");
        shell.decide(Input::Action(Action::Cancel), Instant::now());
        while shell.controlled().is_some() {}

        let started = turn_running(&mut shell, b"edit the notes again\r");
        shell.apply(UiEvent::Approval(super::super::approval::ApprovalAsked {
            id: ApprovalId(2),
            request: asked(),
        }));
        shell.settle_band(started);
        let _ = shell.document();
        delivered(&mut shell);
        assert!(
            !band(&shell).contains("stop, this is wrong"),
            "the cancelled draft is still painted at the next question: {:?}",
            band(&shell)
        );

        shell.decide(Input::Text('1'), Instant::now());
        assert_eq!(
            shell.controlled(),
            Some(TurnControl::Answer {
                id: ApprovalId(2),
                answer: ApprovalAnswer::Once,
                feedback: None,
            }),
            "one question's sentence was handed to the next one"
        );
    }

    #[test]
    fn no_panel_key_reaches_the_composer_and_no_composer_edit_reaches_a_closed_panel() {
        // Both directions. A modal surface that leaked paste bytes into the
        // composer would leave text behind the panel that the user never sees
        // and cannot delete; a composer whose keys reached a closed panel would
        // be answering a question nobody is being asked.
        let mut shell = shell(24, 80);
        // The composer is filled **after** the turn is submitted and before the
        // question arrives: `asking` submits whatever the composer holds, so a
        // draft typed in front of it would be the prompt rather than the text
        // this case is about.
        let started = turn_running(&mut shell, b"edit the notes\r");
        shell.route_bytes(b"a draft the composer already had");
        let before = shell.shell.editor.text().to_string();
        assert!(!before.is_empty(), "the composer fixture is empty");
        shell.apply(UiEvent::Approval(question(asked())));
        shell.settle_band(started);
        let _ = shell.document();
        delivered(&mut shell);
        shell.decide(Input::Action(Action::Tab), Instant::now());

        shell.decide(Input::Action(Action::PasteStart), Instant::now());
        for byte in b"pasted at the panel" {
            shell.decide(Input::PasteByte(*byte), Instant::now());
        }
        shell.decide(Input::Action(Action::PasteEnd), Instant::now());
        type_at_the_question(&mut shell, " and typed");

        assert_eq!(
            shell.shell.editor.text(),
            before,
            "a keystroke at the panel reached the composer"
        );
        assert!(
            band(&shell).contains("pasted at the panel and typed"),
            "the paste did not reach the draft it was typed at: {:?}",
            band(&shell)
        );

        // The other direction: with the question answered, the panel is gone
        // and the same keys are the composer's again.
        delivered(&mut shell);
        shell.decide(Input::Action(Action::Submit), Instant::now());
        let _ = shell.controlled();
        shell.consume(vec![Input::Text('!')], Instant::now());
        assert_eq!(
            shell.shell.editor.text(),
            format!("{before}!"),
            "a composer keystroke was swallowed by a question that has gone"
        );
    }

    #[test]
    fn an_oversized_paste_at_a_draft_is_refused_atomically_and_the_hint_row_says_so() {
        // Refused **whole**: the alternative -- the first 4096 bytes of
        // somebody's file, silently -- is a draft the user did not type. The
        // sentence goes on the band's own hint row, which is where every other
        // refused keystroke on this surface is reported.
        let mut shell = shell(24, 80);
        asking(&mut shell);
        shell.decide(Input::Action(Action::Tab), Instant::now());
        type_at_the_question(&mut shell, "keep this");

        shell.decide(Input::Action(Action::PasteStart), Instant::now());
        for _ in 0..=super::super::approval_amendment::MAX_FEEDBACK_BYTES {
            shell.decide(Input::PasteByte(b'x'), Instant::now());
        }
        shell.decide(Input::Action(Action::PasteEnd), Instant::now());

        assert!(
            shell
                .hint()
                .contains(super::super::approval_amendment::PASTE_REFUSED),
            "the refusal was silent: {:?}",
            shell.hint()
        );
        assert!(
            band(&shell).contains("keep this"),
            "the draft was truncated rather than left alone: {:?}",
            band(&shell)
        );
        assert!(
            !band(&shell).contains("xxxxxxxxxx"),
            "part of the refused paste reached the draft: {:?}",
            band(&shell)
        );

        // And a paste the budget admits still works, so a refusal is about one
        // paste rather than about the rest of the question.
        shell.decide(Input::Action(Action::PasteStart), Instant::now());
        for byte in b" and this" {
            shell.decide(Input::PasteByte(*byte), Instant::now());
        }
        shell.decide(Input::Action(Action::PasteEnd), Instant::now());
        assert!(
            band(&shell).contains("keep this and this"),
            "a refusal poisoned the next paste: {:?}",
            band(&shell)
        );
    }

    #[test]
    fn a_draft_on_the_review_plane_says_its_refusal_on_the_plane_the_user_can_see() {
        // While the review plane is up it owns every row of the terminal, so a
        // notice written into the document is a notice behind a screen nobody
        // can look past -- the same rule the readiness refusal follows.
        let mut shell = shell(40, 100);
        let started = turn_running(&mut shell, b"rewrite the notes\r");
        shell.apply(UiEvent::Approval(question(asked_about_a_large_change())));
        shell.settle_band(started);
        let _ = shell.document();
        assert_eq!(shell.screen_owner(), ScreenOwner::Alternate);

        shell.decide(Input::Action(Action::Tab), Instant::now());
        type_at_the_question(&mut shell, "rename the flag first");
        let painted = unwrapped(&shell.screen_rows());
        assert!(
            painted.contains("rename the flag first"),
            "the plane is not painting the draft it is taking keys for: {painted:?}"
        );

        shell.decide(Input::Action(Action::PasteStart), Instant::now());
        for _ in 0..=super::super::approval_amendment::MAX_FEEDBACK_BYTES {
            shell.decide(Input::PasteByte(b'x'), Instant::now());
        }
        shell.decide(Input::Action(Action::PasteEnd), Instant::now());
        let painted = unwrapped(&shell.screen_rows());
        assert!(
            painted.contains(super::super::approval_amendment::PASTE_REFUSED),
            "the refusal was written behind the plane: {painted:?}"
        );

        // And the plane's own answer carries the plane's own draft.
        delivered(&mut shell);
        shell.decide(Input::Action(Action::Submit), Instant::now());
        assert_eq!(
            shell.controlled(),
            Some(TurnControl::Answer {
                id: ApprovalId(1),
                answer: ApprovalAnswer::Once,
                feedback: Some("rename the flag first".to_string()),
            }),
            "a question that landed on the review plane could not be amended"
        );
        assert_eq!(shell.screen_owner(), ScreenOwner::Primary);
    }

    #[test]
    fn a_submitted_amendment_reaches_the_document_as_the_users_own_row() {
        // **Where the user reads back what they said.** The row is written when
        // the runtime says the sentence was delivered -- the same flush that
        // journals it and puts it in the prompt -- and not when the key was
        // pressed, so the transcript can never show a sentence the model was
        // not told. It goes through `say` like every other thing this session
        // writes, so it takes its place in the stream behind whatever the pacer
        // is still holding.
        let mut shell = shell(24, 80);
        let started = turn_running(&mut shell, b"draft the plan\r");
        shell.apply(tool_start(
            "c1",
            "write_file",
            serde_json::json!({"path": "plan.md", "content": "x"}),
        ));
        shell.apply(tool_result("c1", "write_file", true, "wrote plan.md"));
        shell.apply(UiEvent::ToolFeedback {
            call_id: "c1".to_string(),
            text: "keep the header comment".to_string(),
        });
        shell.settle_band(started);
        let said = unwrapped(&shell.released());
        assert!(
            said.contains("keep the header comment"),
            "the user's own sentence never reached the document: {said:?}"
        );
        // Labelled as theirs rather than as the tool's report of what it did:
        // the two are different claims and the row that carries one must not
        // read as the other.
        assert!(
            said.contains(&format!("{AMENDMENT_PREFIX} keep the header comment")),
            "the sentence is not labelled as the user's: {said:?}"
        );
        assert!(
            !said.contains("Wrote plan.md keep the header"),
            "the sentence was folded into the tool's own row: {said:?}"
        );
    }

    #[test]
    fn an_amendment_takes_one_document_row() {
        // The band places rows by number, so a sentence spread over two would
        // be a row the layout does not know about. What keeps it to one is
        // `super::super::bridge`'s `made_inert`, which turns every control the
        // channel carries into a space -- asserted there, where that function
        // is; asserted here is that this end really writes one row.
        let mut shell = shell(24, 80);
        let started = turn_running(&mut shell, b"draft the plan\r");
        // A draft really can hold newlines -- `C-j` at the panel, or a pasted
        // paragraph -- and `super::super::bridge`'s `made_inert` deliberately
        // keeps them, because a streamed answer's breaks are rows the
        // transcript exists to make. So the sentence arrives with one in it,
        // and what is asserted is that this end makes it a row.
        shell.apply(UiEvent::ToolFeedback {
            call_id: "c1".to_string(),
            text: "first\nsecond\nthird".to_string(),
        });
        shell.settle_band(started);
        let rows = shell.released();
        let carrying: Vec<&String> = rows.iter().filter(|row| row.contains("first")).collect();
        assert_eq!(carrying.len(), 1, "{rows:?}");
        assert!(
            carrying[0].contains("third"),
            "the sentence was split across rows: {rows:?}"
        );
        assert!(
            !rows.iter().any(|row| row.contains('\n')),
            "a document row carries a break the band did not place: {rows:?}"
        );
    }

    #[test]
    fn the_arrows_leave_a_draft_on_the_review_plane_even_though_c_p_scrolls_it() {
        // The documented difference, pinned from both sides. On the review
        // plane `C-p`/`C-n` walk the change rather than the choices -- the only
        // keys that can, since a bounded diff is 128 KiB and a screen is a few
        // dozen rows -- so the property the upstream pin is really about has to
        // be carried by the arrows there: **they move the choice and end
        // editing**, whether or not an amendment is open, so a user is never
        // inside a draft with no way out but a refusal.
        let mut shell = shell(40, 100);
        let started = turn_running(&mut shell, b"rewrite the notes\r");
        shell.apply(UiEvent::Approval(question(asked_about_a_large_change())));
        shell.settle_band(started);
        let _ = shell.document();
        assert_eq!(shell.screen_owner(), ScreenOwner::Alternate);

        shell.decide(Input::Action(Action::Tab), Instant::now());
        type_at_the_question(&mut shell, "rename the flag first");
        assert!(shell.drafting(), "the plane did not open a draft");

        // `C-p` scrolls the change and leaves the draft open: it is not a
        // choice key here.
        shell.decide(Input::Action(Action::HistoryPrevious), Instant::now());
        shell.decide(Input::Action(Action::HistoryNext), Instant::now());
        assert!(
            shell.drafting(),
            "the plane's scroll keys ended the editing and took the only walk of the change with it"
        );

        // The arrows do end it, and keep the text.
        shell.decide(Input::Action(Action::Down), Instant::now());
        assert!(
            !shell.drafting(),
            "an arrow did not end the editing on the plane"
        );
        let painted = unwrapped(&shell.screen_rows());
        assert!(
            painted.contains("rename the flag first"),
            "leaving the draft threw its text away: {painted:?}"
        );
        shell.decide(Input::Action(Action::Up), Instant::now());
        assert!(!shell.drafting(), "an arrow re-opened a draft");
    }
    // -----------------------------------------------------------------------
    // the terminal's own background, while the session runs
    // -----------------------------------------------------------------------

    /// The other palette, at the depth every fixture here paints in.
    ///
    /// Built from `super::super::theme` rather than spelled in greys, because
    /// the module under test is this one: what a light session paints is
    /// `theme`'s fact and `theme`'s tests, and what has to be true here is that
    /// the band's rows come out of *that* palette rather than the one the
    /// session started in.
    const LIGHT: Palette = Palette {
        mode: super::super::theme::Mode::Light,
        depth: super::super::theme::Depth::Ansi256,
    };

    /// The bytes a terminal with mode 2031 sends when its background goes
    /// light, typed onto the session's own input the way the terminal sends
    /// them -- through `route_bytes`, which is the decoder's only door.
    const WENT_LIGHT: &[u8] = b"\x1b[?997;2n";
    const WENT_DARK: &[u8] = b"\x1b[?997;1n";

    #[test]
    fn a_background_that_changes_repaints_the_band_in_the_other_palette() {
        // Both directions, because an arm wired to the wrong mode paints a
        // light terminal in the dark greys and nothing in the session notices:
        // the rows are still there, still readable to whoever wrote the test,
        // and invisible on the screen they were painted for.
        let mut shell = shell(24, 80);
        let _ = shell.render.begin();
        shell.route_bytes(WENT_LIGHT);

        let rows = shell.band_rows();
        assert!(
            rows[0].starts_with(LIGHT.hint()) && rows[0] != composer_row(""),
            "the composer's rail was not repainted in the light palette: {:?}",
            rows[0]
        );
        assert!(
            rows.last()
                .expect("a hint row")
                .starts_with(LIGHT.statusline()),
            "the hint row was not repainted in the light palette: {rows:?}"
        );
        assert!(
            shell.render.begin().is_some(),
            "the band was left owing no frame, so the new palette would reach \
             the screen only when something else asked for one"
        );

        let _ = shell.render.begin();
        shell.route_bytes(WENT_DARK);
        assert!(
            shell.band_rows()[0].starts_with(PALETTE.hint()),
            "the band did not come back to the dark palette"
        );
        assert!(shell.render.begin().is_some(), "the way back owed no frame");
    }

    #[test]
    fn a_report_that_says_what_the_palette_already_says_costs_no_frame() {
        // A terminal may re-report the mode it already reported -- and the
        // answer to the query a resume asks (`Shell::resync_theme`) usually is
        // exactly that. Repainting for it would be a frame per report, on a
        // band whose rows would come out byte-identical.
        let mut shell = shell(24, 80);
        let _ = shell.render.begin();
        shell.route_bytes(WENT_DARK);
        assert!(
            shell.render.begin().is_none(),
            "a report that changed nothing asked for a frame"
        );
    }

    #[test]
    fn a_user_who_named_the_palette_outranks_the_terminal_here_too() {
        // `XFX_THEME` outranks the terminal's answer at launch
        // (`theme::detect`), and a session that then followed the terminal's
        // *notifications* would be honouring the variable for exactly as long
        // as nobody switched their system theme. The lock is carried in, so
        // this case sets no environment variable and races nothing.
        let mut shell = shell_with(24, 80, PALETTE, true);
        let _ = shell.render.begin();
        shell.route_bytes(WENT_LIGHT);

        assert!(
            shell.band_rows()[0].starts_with(PALETTE.hint()),
            "a locked session followed the terminal: {:?}",
            shell.band_rows()[0]
        );
        assert!(
            shell.render.begin().is_none(),
            "a locked session repainted for a report it ignored"
        );
    }

    #[test]
    fn a_report_owes_the_document_a_recolour_and_a_locked_session_owes_none() {
        // The band is repainted every frame and the document is not, so the
        // rows already on the screen are a debt of their own -- paid by the
        // loop's writer against the band's shadow, which is the only thing
        // that knows which of those rows it wrote. A locked session has
        // nothing to pay: the report never moved its palette.
        let mut shell = shell(24, 80);
        assert!(!shell.owes_retint(), "a fresh session owed a recolour");
        shell.route_bytes(WENT_LIGHT);
        assert!(
            shell.owes_retint(),
            "a report left the document in the palette the session has left"
        );
        shell.retint_paid();
        assert!(
            !shell.owes_retint(),
            "the debt outlived the vector that paid it"
        );

        let mut locked = shell_with(24, 80, PALETTE, true);
        locked.route_bytes(WENT_LIGHT);
        assert!(
            !locked.owes_retint(),
            "a session the user decided the palette for owed a recolour for a \
             report it ignored"
        );
    }

    #[test]
    fn a_report_behind_something_that_damaged_the_screen_owes_no_recolour() {
        // The rows a recolour would be painted from are the band's shadow, and
        // a `/clear` or a Ctrl-L means that shadow describes a screen that is
        // gone -- but the band is only *told* when the next frame is built,
        // which is after the recolour would have been paid. So the arming
        // asks, and a report that arrives behind either owes nothing at all.
        let ctrl_l: fn(&mut Fixture) = |shell| shell.damage_told();
        let handed_off: fn(&mut Fixture) = |shell| shell.handed_off();
        // A reset is the same question from the other side: the rows are
        // still on the screen, but on their way into scrollback, which is
        // never recoloured -- and the shadow the handoff leaves holds none.
        for (damage, told) in [(vec![0x0c], ctrl_l), (b"/clear\r".to_vec(), handed_off)] {
            let mut shell = shell(24, 80);
            shell.route_bytes(&damage);
            shell.route_bytes(WENT_LIGHT);
            assert!(
                !shell.owes_retint(),
                "a report behind {damage:?} owed a recolour of a screen that \
                 is gone"
            );
            // And once the band has been told, the next report is ordinary
            // again: what it would repaint is what the band really knows.
            told(&mut shell);
            shell.route_bytes(WENT_DARK);
            assert!(
                shell.owes_retint(),
                "the session stopped recolouring the document for good"
            );
        }
    }

    #[test]
    fn a_background_that_changed_is_not_a_terminal_that_changed() {
        // The depth is a property of the terminal *program* -- whether it
        // renders `38;2` or quantizes it (`theme::depth_from_env`) -- and a
        // background going light is not a new terminal. A retint that rebuilt
        // the whole palette from a default would silently downgrade a
        // truecolor session to 256 colours, which is the same five greys and
        // therefore invisible to every case that only reads the mode.
        let direct = Palette {
            mode: super::super::theme::Mode::Dark,
            depth: super::super::theme::Depth::TrueColor,
        };
        let mut shell = shell_with(24, 80, direct, false);
        shell.route_bytes(WENT_LIGHT);

        let wanted = Palette {
            mode: super::super::theme::Mode::Light,
            depth: super::super::theme::Depth::TrueColor,
        };
        assert!(
            shell.band_rows()[0].starts_with(wanted.hint()),
            "the flip lost the terminal's colour depth: {:?}",
            shell.band_rows()[0]
        );
    }

    #[test]
    fn a_question_in_front_of_the_user_neither_swallows_the_report_nor_moves_under_it() {
        // The ordering `consume` exists to keep. Every surface that takes the
        // focus swallows what it does not bind, so a report routed after them
        // would work at an empty composer and stop working the moment a
        // question was up -- and the question is exactly when a session sits
        // still long enough for a background to change under it.
        //
        // The other half is that the report is not a keystroke *at* the
        // question: the marked choice and the standing batch are what they were
        // before it arrived.
        let mut shell = shell(24, 80);
        asking_question(&mut shell, a_batch());
        let before = shell.marked();
        let _ = shell.render.begin();

        shell.route_bytes(WENT_LIGHT);

        assert_eq!(
            shell.marked(),
            before,
            "the report moved the choice the user was on"
        );
        assert_eq!(
            shell.controlled(),
            None,
            "the report answered the question on the user's behalf"
        );
        // The question's own hint row, in the dim grey upstream writes it in
        // (`input_presentation.zig:552`) -- the light one now.
        let rows = shell.band_rows();
        assert!(
            rows.last().expect("a hint row").starts_with(LIGHT.dim()),
            "the question's band was not repainted in the new palette: {rows:?}"
        );
        assert!(
            shell.render.begin().is_some(),
            "a question standing over a palette change owed no frame"
        );
    }

    #[test]
    fn a_question_on_the_other_plane_keeps_its_draft_across_a_palette_change() {
        // The same claim on the surface the band cannot show, where the danger
        // is bigger: an amendment is text the user typed and has not sent, and
        // a report routed into `decide` as an unknown key would at best be
        // swallowed and at worst reach the draft.
        let mut shell = shell(40, 100);
        let started = turn_running(&mut shell, b"rewrite the notes\r");
        shell.apply(UiEvent::Approval(question(asked_about_a_large_change())));
        shell.settle_band(started);
        let _ = shell.document();
        assert_eq!(shell.screen_owner(), ScreenOwner::Alternate);
        shell.decide(Input::Action(Action::Tab), Instant::now());
        type_at_the_question(&mut shell, "rename the flag first");
        let before = shell.marked();
        let _ = shell.render.begin();

        shell.route_bytes(WENT_LIGHT);

        assert!(shell.drafting(), "the report closed the draft");
        assert_eq!(shell.marked(), before, "the report moved the draft");
        assert!(
            unwrapped(&shell.screen_rows()).contains("rename the flag first"),
            "the report took the draft's text with it"
        );
        assert!(
            shell.render.begin().is_some(),
            "the surface the question is on owed no repaint after a palette change"
        );
    }

    #[test]
    fn only_a_session_the_terminal_decides_for_asks_the_terminal_again() {
        // What a resume arms (`event_loop::collect_facts`), and the one session
        // it must not arm for. A locked palette has nothing for the answer to
        // change, and a query is a sequence written onto somebody's terminal.
        let mut shell = shell(24, 80);
        shell.resync_theme();
        assert!(shell.take_theme_query(), "a resume armed no query");
        assert!(
            !shell.take_theme_query(),
            "one resume would have put two queries on the wire"
        );

        let mut locked = shell_with(24, 80, PALETTE, true);
        locked.resync_theme();
        assert!(
            !locked.take_theme_query(),
            "a session the user decided for asked the terminal anyway"
        );
    }

    // -----------------------------------------------------------------------
    // the document's colours
    // -----------------------------------------------------------------------
    //
    // Read through `take_pending` rather than `document`, which takes the
    // roles' sequences back off: these are the cases that state what those
    // sequences are, so a helper that removed them would leave nothing to
    // assert. The greys are spelled out rather than asked of the palette --
    // an expectation built from the accessor passes whatever the accessor
    // says, including nothing.

    /// The dark palette's hint grey, which a card's rail is painted in:
    /// `theme::DARK`'s `255`.
    const HINT_DARK: &str = "\u{1b}[38;5;255m";
    /// What xfx's own lines are painted in, dark: `theme::DARK`'s `250`.
    const NOTICE_DARK: &str = "\u{1b}[38;5;250m";
    /// And light: `theme::LIGHT`'s `241`.
    const NOTICE_LIGHT: &str = "\u{1b}[38;5;241m";
    /// An outcome notice's label, dark and light (`theme.zig:86-88`,
    /// `:129-131`).
    const LABEL_DARK: &str = "\u{1b}[38;5;252m";
    const LABEL_LIGHT: &str = "\u{1b}[38;5;238m";
    /// What closes any of them.
    const ENDED: &str = "\u{1b}[0m";

    /// Every row the document is owed, as it is handed to the terminal.
    fn painted_rows(shell: &mut Fixture) -> Vec<String> {
        shell
            .take_pending()
            .into_iter()
            .flat_map(|append| append.rows)
            .collect()
    }

    #[test]
    fn the_answer_a_turn_streams_is_painted_as_the_answer() {
        // The model's text reads as the model's text, as upstream presents
        // it: through the markdown presenter, in the two-column gutter, in the
        // terminal's own foreground with only the markdown's attributes on it,
        // and closed by the blank row every block ends on. The breaks this
        // catches are the answer bypassing the presenter (the asterisks on the
        // screen) and the answer painted in one of xfx's greys, which would be
        // this surface claiming the model's words as its own lines.
        let mut shell = shell(24, 80);
        shell.apply(UiEvent::Delta("a **streamed** answer".to_string()));
        shell.shell.flush_paced();

        assert_eq!(
            painted_rows(&mut shell),
            vec![
                "  a \u{1b}[1mstreamed\u{1b}[22m answer".to_string(),
                String::new()
            ]
        );
    }

    #[test]
    fn xfx_s_own_line_is_painted_as_xfx_s_and_the_echo_of_a_prompt_is_not() {
        // The two things that reach the document through the same call. The
        // echo is the **user's** text: painting it in xfx's grey would be this
        // surface claiming to have said what they typed.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"hello\r");
        shell.apply(UiEvent::Notice(notice::Notice::error(
            "tool",
            "read_file refused",
        )));
        shell.shell.flush_paced();

        // The echo is upstream's card: the rail is the band's hint grey, and
        // the text behind it is bold in the terminal's own foreground. The
        // notice is upstream's too: glyph and topic in the label grey, the
        // body in the notice grey (`✗ command:ESC[38;5;250m Unknown…`).
        assert_eq!(
            painted_rows(&mut shell),
            vec![
                format!("{HINT_DARK}{RAIL}{ENDED} {BOLD}hello{ENDED}"),
                String::new(),
                format!(
                    "{LABEL_DARK}\u{2717} {ENDED}{LABEL_DARK}tool:{ENDED}\
                     {NOTICE_DARK} read_file refused{ENDED}"
                ),
                String::new(),
            ],
            "the echo and the notice were not told apart"
        );
    }

    #[test]
    fn a_notice_written_while_a_line_streams_lands_above_it_and_leaves_it_whole() {
        // `fx-notice-midstream-40x120-end.full`: a refusal and a `/version`
        // typed while the answer's second paragraph streams land after the
        // paragraph the answer had finished and **before** the one it had
        // not, and that one arrives whole, wrapped as it would have been with
        // no notice at all. The presenter holds a line until its end arrives
        // (`crate::markdown::processor`); a notice that made it give the line
        // up wrote half a paragraph, the notice, and the rest as a second
        // paragraph -- `word` closing one row and `111` opening the next.
        let mut shell = shell(40, 120);
        let start = Instant::now();
        let words: Vec<String> = (10..=19)
            .chain(100..=139)
            .map(|n| format!("word{n}"))
            .collect();
        let paragraph = format!("MARKER-A {}", words.join(" "));
        let (said, unsaid) = paragraph.split_at(paragraph.find("word111").expect("a word") + 4);
        shell.apply(UiEvent::Delta(format!(
            "Opening line of the answer.\n\n{said}"
        )));
        // Every append, in order, as the terminal is handed them: an append
        // rewrites the unfinished rows of the one before it, so the document
        // is what replaying them leaves rather than their rows end to end.
        let mut appends = Vec::new();
        for at in 1..=3000 / TICK_MILLIS {
            shell.settle_band(start + Duration::from_millis(at * TICK_MILLIS));
            appends.extend(shell.take_pending());
        }
        shell.route_bytes(b"/notacommand\r");
        shell.route_bytes(b"/version\r");
        appends.extend(shell.take_pending());
        shell.apply(UiEvent::Delta(format!(
            "{unsaid}\n\nClosing line of the answer."
        )));
        shell.shell.flush_paced();
        appends.extend(shell.take_pending());
        let mut rows: Vec<String> = Vec::new();
        for append in appends {
            let kept = (rows.len() + append.scroll).saturating_sub(append.rows.len());
            rows.truncate(kept);
            rows.extend(append.rows);
        }

        // A full row: 120 cells, and the space after `word104` hung past the
        // margin, as the wrap hangs every space it breaks on -- the painter's
        // clip leaves it off the wire (`super::super::frame::clip`).
        let full = format!("  MARKER-A {}", words[..15].join(" "));
        assert_eq!(full.len(), 120, "the fixture's first row is not a full one");
        assert_eq!(
            rows,
            vec![
                "  Opening line of the answer.".to_string(),
                String::new(),
                format!(
                    "{LABEL_DARK}\u{2717} {ENDED}{LABEL_DARK}command:{ENDED}\
                     {NOTICE_DARK} Unknown command. Try /help.{ENDED}"
                ),
                String::new(),
                format!(
                    "{NOTICE_DARK}* {ENDED}{NOTICE_DARK}version:{ENDED}\
                     {NOTICE_DARK} {}{ENDED}",
                    crate::VERSION
                ),
                String::new(),
                format!("{full} "),
                format!("  {} ", words[15..29].join(" ")),
                format!("  {} ", words[29..43].join(" ")),
                format!("  {}", words[43..].join(" ")),
                String::new(),
                "  Closing line of the answer.".to_string(),
                String::new(),
            ],
            "the notices split the line the answer was in the middle of"
        );
    }

    #[test]
    fn the_full_transcript_keeps_a_notice_mid_answer_as_an_entry_before_the_line_it_interrupted() {
        // `fx-fulltranscript-notice-40x120-full-top`: Response `Opening line…`,
        // the refusal, the version, and **one** Response holding the whole
        // paragraph and the closing line -- upstream's presenter keeps the
        // line it is in and its notice is an entry beside it
        // (`store.zig:1343-1390`). Recorded from the raw text, the record
        // cut the paragraph at each notice: `…word110 word` | Error |
        // `111 … ` | Notice | `…`.
        let mut shell = shell(40, 120);
        let start = Instant::now();
        let words: Vec<String> = (10..=19)
            .chain(100..=139)
            .map(|n| format!("word{n}"))
            .collect();
        let paragraph = format!("MARKER-A {}", words.join(" "));
        let (said, unsaid) = paragraph.split_at(paragraph.find("word111").expect("a word") + 4);
        shell.apply(UiEvent::Delta(format!(
            "Opening line of the answer.\n\n{said}"
        )));
        for at in 1..=3000 / TICK_MILLIS {
            shell.settle_band(start + Duration::from_millis(at * TICK_MILLIS));
        }
        shell.route_bytes(b"/notacommand\r");
        let (said_more, unsaid) = unsaid.split_at(unsaid.find("word122").expect("a word"));
        shell.apply(UiEvent::Delta(said_more.to_string()));
        for at in 3000 / TICK_MILLIS..=6000 / TICK_MILLIS {
            shell.settle_band(start + Duration::from_millis(at * TICK_MILLIS));
        }
        shell.route_bytes(b"/version\r");
        // Ended by a blank line, so the presenter gives the closing line up
        // without a turn's end.
        shell.apply(UiEvent::Delta(format!(
            "{unsaid}\n\nClosing line of the answer.\n\n"
        )));
        shell.shell.flush_paced();
        shell.route_bytes(b"\x0f");

        let screen = full_screen(&shell);
        let kinds: Vec<&str> = screen
            .iter()
            .filter_map(|row| row.split_once(" UTC \u{b7} ").map(|(_, kind)| kind))
            .collect();
        assert_eq!(
            kinds,
            ["Response", "Error", "Notice", "Response"],
            "{screen:#?}"
        );
        let second = screen
            .iter()
            .rposition(|row| row.ends_with(" UTC \u{b7} Response"))
            .expect("the second response");
        assert_eq!(
            screen[second + 1..second + 7],
            // Each wrapped row with the space it broke on hung past it, as
            // the document's rows are.
            [
                format!("  MARKER-A {} ", words[..15].join(" ")),
                format!("  {} ", words[15..29].join(" ")),
                format!("  {} ", words[29..43].join(" ")),
                format!("  {}", words[43..].join(" ")),
                String::new(),
                "  Closing line of the answer.".to_string(),
            ],
            "{screen:#?}"
        );
        let first = screen
            .iter()
            .position(|row| row.ends_with(" UTC \u{b7} Response"))
            .expect("the first response");
        assert_eq!(screen[first + 1], "  Opening line of the answer.");
        assert_eq!(screen[first + 2], "");
    }

    #[test]
    fn a_fresh_launchs_session_record_is_on_the_full_transcript_and_nowhere_else() {
        // `writeSessionAssemblyNotice` (`app_bootstrap_runtime.zig:640-642`),
        // `visibility = .full_only`: under the welcome on the screen, never in
        // the document.
        let mut shell = shell(40, 120);
        let before = shell.written();
        shell.session_assembled();
        assert_eq!(shell.written(), before, "the document was given the record");
        shell.route_bytes(b"\x0f");
        let screen = full_screen(&shell);
        let at = screen
            .iter()
            .position(|row| row.starts_with("* session: "))
            .expect("the session record");
        assert!(
            screen[at - 1].ends_with(" UTC \u{b7} Notice"),
            "{screen:#?}"
        );
        assert_eq!(
            screen[at],
            format!(
                "* session: provider: {} \u{b7} model: {} \u{b7} effort: auto",
                shell.provider.label(),
                shell.model
            )
        );
        // Wrapped at the screen's width, two columns in, as upstream's
        // `+7` / `more)` is at 120 columns.
        assert_eq!(
            screen[at + 1..at + 4],
            [
                format!(
                    "  tools: {} advertised ({}, +1 ",
                    crate::tools::ADVERTISED_TOOLS.len(),
                    crate::tools::ADVERTISED_TOOLS[..8].join(", ")
                ),
                "  more)".to_string(),
                "  mcp: none".to_string(),
            ]
        );
        // A reset starts the record again without it, as upstream's clear
        // drops its records (`transcript/runtime.zig:4967`).
        shell.route_bytes(b"\x0f");
        shell.route_bytes(b"/new\r");
        shell.route_bytes(b"\x0f");
        assert!(
            !full_screen(&shell)
                .iter()
                .any(|row| row.starts_with("* session: ")),
            "{:#?}",
            full_screen(&shell)
        );
    }

    #[test]
    fn an_unfinished_line_before_a_call_is_recorded_above_the_call_as_the_document_writes_it() {
        // The presenter gives a line up at a call's start (`run_mark`'s
        // `close_answer`), so the document has `Looking.` above the group;
        // the record follows the document's order, and `Done.` is an answer
        // of its own under the group rather than more of the first.
        let mut shell = shell(40, 120);
        shell.route_bytes(b"look\r");
        let _ = shell.picks_up();
        shell.apply(UiEvent::TurnStarted);
        shell.apply(UiEvent::Delta("Looking.".to_string()));
        shell.apply(tool_start(
            "c1",
            "read_file",
            serde_json::json!({"path": "notes.txt"}),
        ));
        shell.apply(tool_result("c1", "read_file", true, "notes.txt"));
        shell.apply(UiEvent::Delta("Done.".to_string()));
        shell.apply(UiEvent::TurnEnded { failure: None });
        shell.shell.flush_paced();
        shell.route_bytes(b"\x0f");
        let screen = full_screen(&shell);
        let kinds: Vec<&str> = screen
            .iter()
            .filter_map(|row| row.split_once(" UTC \u{b7} ").map(|(_, kind)| kind))
            .collect();
        assert_eq!(
            kinds,
            ["Prompt", "Response", "Tool", "Response", "Usage"],
            "{screen:#?}"
        );
        assert!(screen.contains(&"  Looking.".to_string()), "{screen:#?}");
        assert!(screen.contains(&"  Done.".to_string()), "{screen:#?}");
    }

    #[test]
    fn a_notice_inside_an_answer_keeps_its_own_colour_and_its_own_place() {
        // Ordering and colour together: the notice belongs at the point the
        // answer had reached when it happened (`Shell::mark`), and the answer
        // that follows it is the answer again rather than more of the notice.
        let mut shell = shell(24, 80);
        // A whole paragraph before it: text the presenter is still holding
        // would go on under the notice rather than above it
        // ([`Shell::write_notice`]).
        shell.apply(UiEvent::Delta("before\n\n".to_string()));
        shell.apply(UiEvent::Notice(notice::Notice::neutral("tool", "ran")));
        shell.apply(UiEvent::Delta("after".to_string()));
        shell.shell.flush_paced();

        // The answer's block ends on a blank row, which is the notice's blank
        // row above; the notice's own blank row below is what the answer that
        // resumes starts under. A neutral label is the body's grey.
        assert_eq!(
            painted_rows(&mut shell),
            vec![
                "  before".to_string(),
                String::new(),
                format!("{NOTICE_DARK}* {ENDED}{NOTICE_DARK}tool:{ENDED}{NOTICE_DARK} ran{ENDED}"),
                String::new(),
                "  after".to_string(),
                String::new(),
            ]
        );
    }

    #[test]
    fn an_answer_the_pacer_was_still_holding_lands_in_the_palette_it_lands_in() {
        // The palette is read at the write and not at the delta, so what was
        // queued while the terminal was dark is written for the terminal the
        // user has now. Nothing of the stream is rewritten to make that true
        // -- there is nothing to rewrite, because no colour was ever put into
        // it. The answer carries no palette colour of its own, so the subject
        // is xfx's own line held behind it: a notice waits at the place the
        // stream had reached (`Shell::mark`) with its role beside it.
        let mut shell = shell(24, 80);
        // A whole paragraph, so it is written above the notice rather than
        // held under it ([`Shell::write_notice`]).
        shell.apply(UiEvent::Delta("held while it was dark\n\n".to_string()));
        shell.apply(UiEvent::Notice(notice::Notice::warning(
            "tool",
            "read_file ok",
        )));
        let holding = shell.shell.paced_backlog();
        assert!(holding > 0, "the pacer released the delta immediately");

        shell.route_bytes(WENT_LIGHT);
        assert_eq!(
            shell.shell.paced_backlog(),
            holding,
            "a theme report moved the stream"
        );

        shell.shell.flush_paced();
        assert_eq!(
            painted_rows(&mut shell),
            vec![
                "  held while it was dark".to_string(),
                String::new(),
                format!(
                    "{LABEL_LIGHT}! {ENDED}{LABEL_LIGHT}tool:{ENDED}\
                     {NOTICE_LIGHT} read_file ok{ENDED}"
                ),
                String::new(),
            ]
        );
    }

    #[test]
    fn a_theme_report_changes_no_byte_of_what_the_answer_says() {
        // The same stream, cut by the pacer in the same places, with a report
        // in the middle of it: what the report may change is the sequence
        // around a row and nothing inside one.
        let mut shell = shell(24, 80);
        shell.apply(UiEvent::Delta("first half ".to_string()));
        shell.route_bytes(WENT_LIGHT);
        shell.apply(UiEvent::Delta("second half".to_string()));
        shell.shell.flush_paced();

        // Spelled whole rather than stripped: the answer carries no palette
        // sequence for a report to change, so the rows are exactly the text.
        assert_eq!(
            painted_rows(&mut shell),
            vec!["  first half second half".to_string(), String::new()]
        );
    }

    // -----------------------------------------------------------------------
    // the full transcript (`C-o`)
    // -----------------------------------------------------------------------

    /// The full transcript's screen with every colour off, top first.
    fn full_screen(shell: &Fixture) -> Vec<String> {
        shell
            .screen_rows()
            .iter()
            .map(|row| colourless(row))
            .collect()
    }

    /// A finished turn that ran one command, whose output only the journal
    /// carries, and a short answer after it.
    fn a_turn_that_ran_a_command(shell: &mut Fixture) {
        shell.route_bytes(b"run the loop\r");
        let _ = shell.picks_up();
        shell.apply(UiEvent::TurnStarted);
        let input = serde_json::json!({"action": "exec", "command": "echo boom; exit 3"});
        shell.apply(UiEvent::ToolDetail {
            call_id: "c1".into(),
            detail: full_transcript::Detail::Arguments(vec![
                full_transcript::Argument {
                    name: "action".into(),
                    value: "exec".into(),
                    text: true,
                },
                full_transcript::Argument {
                    name: "command".into(),
                    value: "echo boom; exit 3".into(),
                    text: true,
                },
            ]),
        });
        shell.apply(tool_start("c1", "terminal", input));
        shell.apply(tool_ended(
            "c1",
            "terminal",
            true,
            "echo boom; exit 3 (exit 3)",
            CallOutcome::Exited(3),
        ));
        shell.apply(UiEvent::ToolDetail {
            call_id: "c1".into(),
            detail: full_transcript::Detail::Command {
                output: "boom".into(),
                status: Some("exit code 3".into()),
            },
        });
        shell.apply(UiEvent::Delta("All done.\n".into()));
        shell.apply(UiEvent::TurnEnded { failure: None });
        shell.shell.flush_paced();
    }

    #[test]
    fn ctrl_o_opens_the_full_transcript_on_the_other_plane_and_each_closing_key_gives_it_back() {
        // `input_full_transcript_runtime.zig:177-198`: Ctrl-O toggles, and
        // Escape and Ctrl-C close (upstream's e2e
        // `tui-full-transcript-brutal.test.ts:791`).
        for (close, settle) in [
            (&b"\x0f"[..], false),
            (b"\x1b", true),
            (b"\x03", false),
            (b"\x1b[111;5u", false),
        ] {
            let mut shell = shell(40, 120);
            a_turn_that_ran_a_command(&mut shell);
            let _ = shell.written();
            shell.route_bytes(b"\x0f");
            assert_eq!(shell.screen_owner(), ScreenOwner::Alternate, "{close:?}");
            let screen = full_screen(&shell);
            assert_eq!(screen.len(), 40);
            assert_eq!(screen[37], format!("\u{2503} {}", full_transcript::FOOTER));
            // The band's own hint row, under the footer
            // (`paint_plan.zig:1034-1045`).
            let hint = colourless(shell.band_rows().last().expect("a hint row"));
            assert_eq!(screen[39], hint);
            assert_eq!(shell.screen_cursor(), (39, 0));

            shell.route_bytes(close);
            if settle {
                shell.settle_input(Instant::now() + Duration::from_millis(100));
            }
            assert_eq!(
                shell.screen_owner(),
                ScreenOwner::Primary,
                "{close:?} kept the screen"
            );
            assert!(shell.screen_rows().is_empty());
            // A Ctrl-C that closes the screen is not an interrupt.
            assert_eq!(shell.controlled(), None, "{close:?} reached the runtime");
        }
    }

    #[test]
    fn the_full_transcript_shows_what_a_command_printed_and_the_document_does_not() {
        // `fx-tools-single-40x120-full` rows 18-24 against `full-esc`: the
        // output is the full screen's alone (upstream's e2e
        // `tui-command-permissions.test.ts:1118`).
        let mut shell = shell(40, 120);
        a_turn_that_ran_a_command(&mut shell);
        let document = shell.written();
        assert!(
            !document
                .iter()
                .any(|row| row.contains("boom\u{0}") || row == "\u{2502} boom"),
            "the inline transcript showed the command's output: {document:?}"
        );
        shell.route_bytes(b"\x0f");
        let screen = full_screen(&shell);
        let at = screen
            .iter()
            .position(|row| row == "\u{2514} Exited 3 echo boom; exit 3")
            .unwrap_or_else(|| panic!("no row for the call: {screen:?}"));
        assert!(screen[at - 1] == "\u{25cf} 1 tool call \u{b7} 1 command \u{b7} 1 failed");
        assert!(screen[at - 2].ends_with(" UTC \u{b7} Tool"), "{screen:?}");
        // The command is on the row and `exec` is the only action there is,
        // so neither argument is listed again.
        assert_eq!(
            &screen[at + 1..at + 4],
            ["\u{2502} boom", "\u{2502} exit code 3", ""]
        );
        let prompt = screen
            .iter()
            .position(|row| row == "\u{2503} run the loop")
            .expect("the prompt");
        assert!(screen[prompt - 1].ends_with(" UTC \u{b7} Prompt"));
        let answer = screen
            .iter()
            .position(|row| row == "  All done.")
            .expect("the answer");
        assert!(screen[answer - 1].ends_with(" UTC \u{b7} Response"));
        assert!(prompt < at && at < answer);
        assert!(screen.iter().any(|row| row.ends_with(" UTC \u{b7} Usage")));
    }

    #[test]
    fn the_full_transcripts_rows_for_a_card_a_notice_and_an_answer_are_the_documents_own() {
        // The full transcript lays its entries out with its own transcript
        // (`super::full_transcript`'s `card`, `notice_rows`, `answer`); this
        // is what keeps those from drifting from what the document wrote.
        let mut shell = shell(40, 80);
        shell.route_bytes(b"a prompt\r");
        let _ = shell.picks_up();
        shell.apply(UiEvent::TurnStarted);
        shell.apply(UiEvent::Notice(notice::Notice::warning(
            "session",
            "two lines\nof it",
        )));
        shell.apply(UiEvent::Delta(
            "Some **bold** prose.\n\n```rust\nfn main() {}\n```\n".into(),
        ));
        shell.apply(UiEvent::TurnEnded { failure: None });
        shell.shell.flush_paced();
        let document: Vec<String> = shell.written();
        shell.route_bytes(b"\x0f");
        let screen = full_screen(&shell);
        // Every row of the record but its headers is a row the document has,
        // in the same order.
        let record: Vec<&String> = screen[..36]
            .iter()
            .filter(|row| !row.ends_with(" UTC \u{b7} Prompt"))
            .filter(|row| !row.contains(" UTC \u{b7} "))
            .filter(|row| !row.is_empty())
            .collect();
        let written: Vec<&String> = document
            .iter()
            .filter(|row| !row.is_empty())
            .filter(|row| !row.starts_with("x\u{1d487}x"))
            .collect();
        assert_eq!(record, written);
    }

    #[test]
    fn the_screens_keys_move_it_and_every_other_key_reaches_the_hidden_composer_as_upstreams() {
        // Upstream's map binds the page keys, the arrows, Escape, Ctrl-C and
        // Ctrl-O (`input_full_transcript_runtime.zig:147-198`); everything
        // else -- Home and End included -- goes on to its composer behind the
        // screen (`app_input_runtime.zig:973-994`), and so it does here.
        let mut shell = shell(24, 80);
        for index in 0..20 {
            shell.apply(UiEvent::Notice(notice::Notice::neutral(
                "n",
                format!("{index}"),
            )));
        }
        let _ = shell.written();
        shell.route_bytes(b"draft");
        shell.route_bytes(b"\x0f");
        let tail = full_screen(&shell);
        assert_eq!(tail[19], "* n: 19");
        shell.route_bytes(b"\x1b[5~");
        let paged = full_screen(&shell);
        assert_ne!(paged, tail, "PgUp did not move the screen");
        shell.route_bytes(b"\x1b[6~");
        assert_eq!(full_screen(&shell), tail, "PgDn did not come back");
        shell.route_bytes(b"\x1b[A");
        assert_ne!(full_screen(&shell), tail, "Up did not move the screen");
        shell.route_bytes(b"\x1b[B");
        assert_eq!(full_screen(&shell), tail, "Down did not come back");

        // Home, End and text: the screen stays where it is and the composer
        // takes them -- the caret to the start, `more ` typed there.
        shell.route_bytes(b"\x1b[Hmore \x1b[F!");
        assert_eq!(full_screen(&shell), tail, "a composer key moved the screen");
        assert!(shell.full.is_open(), "a composer key closed the screen");
        assert_eq!(shell.editor.text(), "more draft!");
        // A paste is the composer's too.
        shell.route_bytes(b"\x1b[200~ pasted\x1b[201~");
        assert_eq!(shell.editor.text(), "more draft! pasted");
        assert!(shell.sent.try_recv().is_err(), "nothing was submitted yet");

        // Enter submits it, from behind the screen, and the screen shows the
        // prompt it sent as soon as the document is given it.
        shell.route_bytes(b"\r");
        assert!(
            matches!(shell.sent.try_recv(), Ok(TurnWork::Submit(_))),
            "Enter behind the screen did not submit"
        );
        assert!(shell.full.is_open());
        assert!(
            full_screen(&shell)
                .iter()
                .any(|row| row == "\u{2503} more draft! pasted"),
            "{:?}",
            full_screen(&shell)
        );
        shell.route_bytes(b"\x0f");
        assert_eq!(shell.screen_owner(), ScreenOwner::Primary);
    }

    #[test]
    fn a_question_closes_the_full_transcript_and_a_large_change_takes_the_plane_over() {
        // `transitionFullTranscriptForApproval` (`app_lifecycle.zig:1298-1316`):
        // a question that needs the screen is handed it, any other closes it.
        let mut shell = shell(24, 80);
        let started = turn_running(&mut shell, b"edit the notes\r");
        shell.route_bytes(b"\x0f");
        assert_eq!(shell.screen_owner(), ScreenOwner::Alternate);
        shell.apply(UiEvent::Approval(question(asked_about_a_large_change())));
        shell.settle_band(started);
        assert_eq!(shell.screen_owner(), ScreenOwner::Alternate);
        assert!(
            shell.screen_rows().join("\n").contains("Permission needed"),
            "the plane did not go to the question"
        );
        assert!(!shell.full.is_open());

        let mut shell = self::shell(24, 80);
        let started = turn_running(&mut shell, b"edit the notes\r");
        shell.route_bytes(b"\x0f");
        shell.apply(UiEvent::Approval(question(asked())));
        shell.settle_band(started);
        assert_eq!(shell.screen_owner(), ScreenOwner::Primary);
        assert!(shell.band_rows().join("\n").contains("Permission needed"));

        let mut shell = self::shell(24, 80);
        let _ = turn_running(&mut shell, b"ask me\r");
        shell.route_bytes(b"\x0f");
        shell.apply(UiEvent::Question(a_batch()));
        assert_eq!(shell.screen_owner(), ScreenOwner::Primary);
        assert!(!shell.full.is_open());
    }

    #[test]
    fn ctrl_o_at_a_question_opens_nothing() {
        // `fx-approvals-cc-40x120-edit-ctrl-o`: the review stays as it was.
        let mut shell = shell(24, 80);
        let _ = asking(&mut shell);
        let before = shell.band_rows();
        shell.route_bytes(b"\x0f");
        assert_eq!(shell.screen_owner(), ScreenOwner::Primary);
        assert!(!shell.full.is_open());
        assert_eq!(shell.band_rows(), before);
    }

    #[test]
    fn a_reset_starts_the_full_transcript_again_with_the_document() {
        // `/clear` = `/new` = `/reset` hands the old document to scrollback and
        // starts a fresh one under the welcome line (C2); the full transcript
        // is that document's record, so it starts again too -- and a detail the
        // runtime still sends about the old conversation is dropped with the
        // rest of it.
        for command in RESETS {
            let mut shell = shell(40, 120);
            a_turn_that_ran_a_command(&mut shell);
            shell.route_bytes(command);
            shell.apply(UiEvent::ToolDetail {
                call_id: "c1".into(),
                detail: full_transcript::Detail::Result("late".into()),
            });
            shell.route_bytes(b"\x0f");
            let screen = full_screen(&shell);
            assert!(screen[0].starts_with("x\u{1d487}x"), "{screen:?}");
            assert!(
                screen[1..36].iter().all(String::is_empty),
                "{} kept the old conversation: {screen:?}",
                String::from_utf8_lossy(command)
            );
        }
    }

    #[test]
    fn the_exit_gives_back_a_plane_the_full_transcript_was_holding() {
        let mut shell = shell(24, 80);
        shell.route_bytes(b"\x0f");
        assert_eq!(shell.screen_owner(), ScreenOwner::Alternate);
        shell.release_screen();
        assert_eq!(shell.screen_owner(), ScreenOwner::Primary);
        assert!(!shell.full.is_open());
    }
}
