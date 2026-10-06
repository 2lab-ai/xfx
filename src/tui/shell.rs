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
use super::bridge::{ModelAnswer, TurnControl, TurnWork, UiEvent};
use super::edit_history::{Delta, EditHistory};
use super::editor::{self, Editor};
use super::entity::{Entities, EntityKind};
use super::gesture::{Escape, Gestures, Interrupt, INTERRUPTED_EXIT_CODE};
use super::help_menu::{self, HelpAction, HelpMenu};
use super::hint::{self, Hint, Notice};
use super::history::{History, HistoryEntry, HistoryStep};
use super::input::{Action, Decoder, Input};
use super::layout::{self, Geometry};
use super::notice::{self, unattributed};
use super::pacer::Pacer;
use super::paste::{self, Paste, Pasted, Refusal};
use super::picker::{self, Dismissed, Picker, PickerAction, PickerOutcome, Trigger};
use super::question::{self, QuestionPanel, QuestionRequest};
use super::render_request::{Reason, RenderRequest};
use super::router::{self, CommandHandlers};
use super::theme::{Mode, Palette};
use super::toolgroup::ToolGroup;
use super::transcript::{Append, Gutter, Landed, Role, Transcript};
use super::worker::{Rejected, WorkHandle};
use crate::config::{PermissionMode, RuntimeConfig};
use crate::interactive::{self, Submitted};
use crate::markdown::{self, MarkdownProcessor};
use crate::output::safe_one_line;
use crate::permission::{ApprovalAnswer, ApprovalRequest};
use crate::provider::model::model_id_problem;
use crate::provider::model::CatalogEntry;
use crate::provider::ProviderId;
use crate::session::store::title_of;
use crate::tools::{LineChanges, Outcome as CallOutcome, Shown};

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
/// Present tense, like `app::INTERRUPT_NOTICE` beside it, because both are the
/// *request* landing rather than a report of what the runtime has finished
/// doing: the drop itself is `super::worker`'s `abandon_pending`, on the far
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

/// What the user is told when the screen cannot hold the question.
///
/// A refusal rather than a squeezed panel, and it is the same rule the panel's
/// own Esc is: **a decision xfx was never given is a refusal**. A question
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
     make the window larger, or press 3 or esc to refuse";

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

/// What `/clear` leaves behind after it has erased the screen.
///
/// The line-oriented shell prints its banner and a `kept` line here
/// (`interactive.rs:451-456`); the TUI has no banner, and the identity that
/// banner carries belongs on the hint row. What is left is the promise the
/// command's own summary makes -- that clearing the screen is not clearing the
/// conversation -- said where a user who just watched their transcript vanish
/// is looking.
///
/// A neutral notice under the topic `screen`.
const CLEARED_NOTICE: &str = "cleared the screen; the conversation is kept";

/// What `/new` says, in the words the line-oriented shell says them in
/// (`interactive.rs:461-463`), as a neutral notice under the topic `session`.
const NEW_SESSION_NOTICE: &str = "new session; the next prompt starts a fresh conversation";

/// What a line that names no command -- or names one with something after it
/// that the command does not take -- is answered with: upstream's own
/// sentence, an error under the topic `command` (`app_commands.zig:2162-2169`).
const UNKNOWN_COMMAND: &str = "Unknown command. Try /help.";

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

/// Erases the screen, its scrollback, and puts the cursor home.
///
/// The same three sequences `/clear` writes on the line-oriented path
/// (`interactive.rs:85`), and the only bytes this session writes that are not a
/// frame or a document append. `3J` is the one that matters here: xfx's answers
/// live in the terminal's *own* scrollback (`super::frame`), so a `/clear` that
/// erased only the visible screen would leave the transcript one wheel-turn
/// away and mean something different on this surface than it does on the other.
pub(crate) const CLEAR_SCREEN: &str = "\u{1b}[H\u{1b}[2J\u{1b}[3J";

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
    /// Read once and never changed: the mode is settled by the configuration
    /// and no command in the palette moves it -- the slash names are the line
    /// shell's and none of them is `/permission`.
    mode: PermissionMode,
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
    /// Taken when a question arrives that the band's summary cannot show, and
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
    /// What that row says, as of the last settle, or `None` while there is no
    /// work to say anything about.
    ///
    /// Cached for the reason [`Self::escape_armed`] is -- [`Self::band_rows`]
    /// has no clock -- and for one more that is this row's own: whether the
    /// band **has** the row is a fact of the geometry, and the geometry is
    /// re-solved from exactly this field, so the row's presence and its text
    /// cannot disagree.
    activity_row: Option<String>,
    /// Whether the screen owes a `/clear`.
    ///
    /// Taken by the loop, which owns the writer. A `bool` rather than a queued
    /// write because two clears in one turn are one clear.
    clearing: bool,
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
    prompts: VecDeque<(u64, Option<String>)>,
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
    /// and the band stays at the bottom. A `/clear` starts it again at none. A
    /// resize sets it past any screen, because where a terminal reflows the
    /// rows already on it is the terminal's own business and a band floated
    /// under a guess would be painted over them.
    document_rows: u32,
    /// Why the session is ending, when it is ending because the runtime cannot
    /// go on. Printed by the caller **after** the terminal has been restored.
    fatal: Option<String>,
    /// How the session is ending, once it is.
    leaving: Option<Leaving>,
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
    /// A question with a change too large for the band to show.
    Approval,
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
    /// A screen of its own, for a change the band's summary cannot show.
    Alternate(ApprovalScreen),
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
        let mut line = format!("  {}", compact_duration(seconds));
        let output = self.output.unwrap_or_else(|| self.streamed.estimate());
        if self.input != 0 || output != 0 {
            line.push_str(&format!(
                " (\u{2191}{} \u{2193}{})",
                compact_tokens(self.input),
                compact_tokens(output)
            ));
        }
        line
    }
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
    /// The `/help` catalog. Like the menu it owns nothing but rows, and the
    /// composer is its search.
    Catalog(&'a HelpMenu),
    /// A completion menu for what the composer holds. It owns nothing but rows.
    Menu(&'a Picker),
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
            missing_credential: crate::provider::resolve_credential_for(config.provider, config)
                .is_none(),
            provider: config.provider,
            catalog: Vec::new(),
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
            activity_row: None,
            clearing: false,
            markdown: MarkdownProcessor::with_styles(markdown::ansi::Styles::new(
                markdown::theme::builtin(palette.mode == Mode::Light),
            )),
            answer: Answer::default(),
            group: ToolGroup::default(),
            abandoned: Vec::new(),
            tally: Tally::default(),
            prompts: VecDeque::new(),
            session_title: None,
            turn_title: None,
            title_owed: true,
            workspace: config.workspace_root.clone(),
            document_rows: 0,
            fatal: None,
            leaving: None,
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
            rows.extend(std::iter::repeat_n(String::new(), padding));
        }
        // The composer's own rows, as many of them as the window shows, each
        // behind upstream's rail: `┃` in the hint colour and a space, or `┃↑`
        // on the first row shown when rows above it are scrolled out of view
        // (`input_presentation.zig:977-992`).
        let composer = self.editor.rows(self.text_cols());
        let window = self.window(composer.len());
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
                Some(Slot::Question(panel)) => panel.rows(self.geometry.cols, self.geometry.rows),
                Some(Slot::Ask(ask)) => ask.rows(self.geometry.cols, self.geometry.rows),
                // In the rows the geometry gave it, which are the rows its own
                // height asked for: one layout answers both
                // (`composeHelpMenuRow` is handed `menuRowCount`'s answer).
                Some(Slot::Catalog(menu)) => {
                    menu.rows(self.geometry.cols, self.geometry.panel, self.palette)
                }
                Some(Slot::Menu(picker)) => {
                    picker.rows(self.geometry.cols, self.geometry.rows, self.palette)
                }
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
        // up, the keys it binds.
        rows.push(self.hint_or_key_help());
        rows
    }

    /// The band's last row, painted.
    ///
    /// **Replaced, not annotated, while the menu is up**: upstream paints the
    /// slash menu's key help in that row instead of the hint, in the dim grey
    /// rather than the statusline's (`paint_plan.zig:970-971`), and with no
    /// notice or armed gesture beside it. A question does not get this row --
    /// the slot is the question's then ([`Self::slot`]), and so is the hint.
    fn hint_or_key_help(&self) -> String {
        match self.slot() {
            Some(Slot::Menu(_)) => self.painted(
                self.palette.dim(),
                picker::key_help(self.geometry.cols).to_string(),
            ),
            // The catalog's own key help, in the same dim grey
            // (`composeHelpMenuHintRow`, `input_presentation.zig:696-721`).
            Some(Slot::Catalog(_)) => self.painted(
                self.palette.dim(),
                help_menu::key_help(self.geometry.cols).to_string(),
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
        let notice = self.notice.map(|text| Notice {
            text,
            style: self.palette.notice(),
            resume: self.palette.statusline(),
        });
        hint::row(
            &Hint {
                missing_credential: self.missing_credential,
                queued: self.queued,
                mode: self.mode,
                model: &self.model,
                context_used: self.context_meter(),
                notice,
                // The armed half of the double-Escape gesture goes to the
                // right-hand slot: it is a warning about what the *next*
                // keystroke would do rather than another fact about the
                // session, and a warning that moved left and right with the
                // queue's depth would be one the eye has to look for.
                right: self.escape_armed.then_some(ESCAPE_ARMED),
                accent: (self.palette.activity(), self.palette.statusline()),
            },
            self.geometry.cols,
        )
    }

    /// The two numbers of the context meter, or nothing at all.
    ///
    /// **Both or neither**, and that is the whole rule. The numerator is a
    /// completed turn's `input_tokens` and the denominator is the catalog's
    /// `max_context` for the model in force; either can legitimately be absent
    /// -- a provider need not publish usage, and the Gateway publishes no
    /// catalog to hold a window at all -- and a row that filled in a missing
    /// half with a zero would report a measurement nobody took. So a missing
    /// half removes the segment rather than defaulting it, which is the same
    /// rule the activity row's token count follows
    /// ([`super::activity::Activity::tokens`]).
    fn context_meter(&self) -> Option<(u64, u64)> {
        let used = self.context_used?;
        // Matched by id **or alias**, because the model in force is whatever the
        // operator or the profile spelled and the catalog publishes both
        // ([`CatalogEntry::matches`]).
        let total = self
            .catalog
            .iter()
            .find(|entry| entry.matches(&self.model))
            .and_then(|entry| entry.max_context)?;
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
        if let Some(menu) = self.help_menu.as_ref() {
            return Some(Slot::Catalog(menu));
        }
        self.picker.as_ref().map(Slot::Menu)
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
        let line = welcome_line(self.palette);
        self.write_transcript_as(&line, Role::Plain);
        self.end_transcript_line();
        self.end_transcript_line();
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
        match event {
            // A turn is running, which is what the row above the divider is
            // about. The clock is not read here: the *label* is this event's
            // and the *moment* is the next settle's, so the row is timed by the
            // same clock every other row of the band is settled against
            // ([`Self::tick_activity`]).
            UiEvent::TurnStarted => {
                self.activity.set(Work::Thinking);
                let (input, title) = self.prompts.pop_front().unwrap_or_default();
                self.tally.begin(Instant::now(), input);
                self.turn_title = title;
            }
            // The answer, as it arrives -- into the pacer rather than into the
            // document, so a provider that sends a kilobyte in one frame and
            // nothing in the next is still read at one speed.
            UiEvent::Delta(text) => self.stream(&text),
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
                self.activity.set(Work::Tool { name: tool });
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
                self.activity.set(Work::Thinking);
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
                self.context_used = None;
                // The conversation went with the bundle, and the session its
                // title named with it.
                self.forget_session_title();
                self.announce(notice::Notice::neutral(
                    "provider",
                    format!(
                        "{} model={}; the next prompt starts a fresh conversation",
                        provider.label(),
                        self.model
                    ),
                ));
                self.render.request(Reason::Footer);
            }
            // What the selector made of a `/model <id>`, and the model in force
            // afterwards. **Taken rather than predicted**: the catalog decides,
            // the catalog is on the runtime thread, and a band that adopted the
            // id it submitted would show a model the provider does not publish
            // and report it to the next bare `/model`.
            UiEvent::ModelAnswered { model, outcome } => {
                self.model = model;
                match outcome {
                    ModelAnswer::Applied { unverified } => {
                        // Upstream's announcement, with no topic
                        // (`announceModelSelection`,
                        // `session_commands.zig:1266-1279`) -- without the
                        // `(effort: …, speed: …)` it ends with, which are
                        // two settings xfx does not send.
                        self.announce(notice::Notice::neutral(
                            "",
                            format!("Switched to {}", self.model),
                        ));
                        // The caveat *after* the change, because it is a caveat
                        // on it: the selection stands, and what could not be
                        // done is check it. One sentence for both front ends
                        // (`provider::model::unverified_notice`), so a daemon
                        // that is down reads the same wherever `/model` was
                        // typed.
                        if let Some(reason) = unverified {
                            self.announce(notice::Notice::warning(
                                "model",
                                unattributed(&crate::provider::model::unverified_notice(&reason)),
                            ));
                        }
                    }
                    ModelAnswer::Unchanged => {
                        self.announce(notice::Notice::neutral(
                            "model",
                            format!("{} unchanged", self.model),
                        ));
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
            // The browser. Rendered as document rows rather than into the band's
            // elastic slot: a catalog is a list the user reads and scrolls back
            // to, and the slot is for the two things a keystroke is *about*.
            UiEvent::CatalogLoaded { provider, entries } => {
                self.catalog = entries;
                self.provider = provider;
                // One notice, its rows under the head: a catalog is one thing
                // said, and a notice per row would put a blank row between
                // every model.
                let mut body = format!("{} shown", self.catalog.len());
                for line in self.catalog_rows() {
                    body.push('\n');
                    body.push_str(&line);
                }
                self.announce(notice::Notice::neutral("catalog", body));
                // The denominator may have arrived with it.
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
            UiEvent::Approval(asked) => self.ask(asked.id, asked.request),
            // The same three facts for a question the **model** asked: the
            // panel, the rows it costs the document, and the focus. What it is
            // not is an approval -- nothing is granted, and refusing it denies
            // nothing ([`super::question`]).
            UiEvent::Question(request) => self.ask_question(request),
            UiEvent::TurnEnded { failure } => {
                // Behind whatever of this turn's answer is still in the pacer:
                // a conclusion that overtook the text it concludes would land
                // in the middle of the answer. The tool group goes first,
                // because it is part of the turn and the conclusion is under
                // it: whatever of it is still running was stopped with the
                // turn ([`Mark::TurnOver`]).
                self.mark(Mark::TurnOver);
                // A summary only under a turn the runtime ran: `/model`, `/setup`
                // and a catalog listing end with this event too, and they
                // started no turn to summarize.
                let summary = self
                    .tally
                    .started
                    .is_some()
                    .then(|| self.tally.line(Instant::now()));
                self.tally.started = None;
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
                self.pacer.finish();
                self.gestures.turn_ended();
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

    /// One document row per catalog entry, in the order the provider published
    /// them.
    ///
    /// The line shell's own row for the same fact
    /// (`interactive`'s `print_catalog`), so a model looks the same whichever
    /// surface you browse it from. `unknown` and `none` are said rather than
    /// left blank: a provider really may publish neither a window nor a set of
    /// effort levels, and an empty column would read as a rendering fault.
    fn catalog_rows(&self) -> Vec<String> {
        self.catalog
            .iter()
            .map(|entry| {
                let context = entry
                    .max_context
                    .map(|window| window.to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                let efforts = if entry.efforts.is_empty() {
                    "none".to_string()
                } else {
                    entry.efforts.join(",")
                };
                format!(
                    "{} context={context} efforts={efforts}",
                    entry.preferred_name()
                )
            })
            .collect()
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
    /// ([`layout::fits_panel`]); the review plane takes a screen of its own, so
    /// what it needs is that the screen can carry its three answers
    /// ([`ApprovalScreen::presents_choices`]). Asking the band's question about
    /// a question the band was never going to show is how a short window came
    /// to deny a change the plane can display whole.
    fn ask(&mut self, id: ApprovalId, request: ApprovalRequest) {
        // Which plane the question belongs on, settled from the change itself
        // before anything is installed ([`approval::ApprovalSurface`]).
        let surface = approval::ApprovalSurface::for_request(&request);
        let composed = match surface {
            approval::ApprovalSurface::Inline => {
                let panel = Panel::new(request);
                let rows = panel.height(self.geometry.cols, self.geometry.rows);
                layout::fits_panel(self.geometry.rows, self.geometry.cols, rows)
                    .then_some(Asked::Inline(panel))
            }
            approval::ApprovalSurface::Alternate => {
                let screen = ApprovalScreen::new(request);
                screen
                    .presents_choices(self.geometry.cols, self.geometry.rows)
                    .then_some(Asked::Alternate(screen))
            }
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
            !(self.owner == ScreenOwner::Approval && self.alternate.is_some()),
            "a second question took a plane the first one is still on"
        );
        match composed {
            Asked::Inline(panel) => {
                self.panel = Some(panel);
                self.owner = ScreenOwner::Primary;
            }
            // The band keeps none of its rows for this one: the question is
            // painted on the other plane, and the band underneath stays exactly
            // the band the restore repaints when the answer gives it back.
            Asked::Alternate(screen) => {
                self.alternate = Some(screen);
                self.owner = ScreenOwner::Approval;
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
    /// Empty when no question owns that plane, so a caller that asked at the
    /// wrong moment paints nothing rather than painting a screen out of a
    /// question that has been answered.
    pub(crate) fn screen_rows(&self) -> Vec<String> {
        self.alternate
            .as_ref()
            .map(|screen| screen.rows(self.geometry.cols, self.geometry.rows))
            .unwrap_or_default()
    }

    /// Where the caret goes on that plane: the marked choice.
    pub(crate) fn screen_cursor(&self) -> (u16, u16) {
        self.alternate.as_ref().map_or((1, 0), |screen| {
            screen.caret(self.geometry.cols, self.geometry.rows)
        })
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
        self.owner = ScreenOwner::Primary;
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

    /// One keystroke, while a question has the focus.
    ///
    /// **Everything the panel does not bind is swallowed**, and that is the
    /// difference between a panel and a hint: a `1` typed at a question is an
    /// answer, not a character in a composer whose caret is somewhere else, and
    /// a Ctrl-D at one does not end a session that is holding a turn open
    /// waiting to be told what to do.
    ///
    /// **Ctrl-C is the exception to "the panel answers it", and it is the whole
    /// of why this takes a clock.** A question does not stop being a turn: the
    /// key means what it means everywhere else on this surface -- stop the work
    /// and drop what is queued behind it ([`Self::interrupt`]) -- and the
    /// refusal of the question comes back *with* it rather than instead of it,
    /// because the prompter is the thing parked on that channel and it turns a
    /// cancellation into a `Deny` and hands the cancellation on to the loop that
    /// can act on it (`super::approval::TuiPrompter`). Answering `Deny` here and
    /// stopping there would leave the user watching the turn they interrupted
    /// carry on, with the prompt they had queued behind it running next.
    fn decide(&mut self, event: Input, now: Instant) {
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
            // One message, both meanings. `Deny` is what the prompter answers a
            // cancellation with on the far side, so sending `Answer(Deny)` here
            // as well would be the *only* thing the runtime heard -- and the
            // turn this question belongs to, and whatever was queued behind it,
            // would go on running after the user asked everything to stop.
            // **One message still**, and it carries no sentence: the drafts
            // were dropped above, so an interrupt at a filled draft says
            // nothing on either channel.
            // **The call this panel was asking about is stopped by this key**:
            // the prompter answers the stop by refusing it and handing the
            // interrupt on, so the call never runs (`super::approval`'s
            // `TuiPrompter`). The group says so now, above the sentence that
            // answers the key, as upstream's does the moment it is pressed
            // (`fx-approvals-cc-40x120-after-esc`); the refusal the runtime
            // reports for it next is the same stop in the tool's words, and is
            // dropped ([`Self::abandoned`]).
            approval::Action::Cancel => {
                self.settle_group();
                self.interrupt(now);
            }
            // Esc and the rest are an answer about *this call* and nothing
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
    }

    /// One keystroke, while a batch of the model's questions has the focus.
    ///
    /// **Everything the panel does not bind is swallowed**, exactly as at a
    /// permission question: a `1` is an ordinal rather than a character in a
    /// composer the caret has left, and a Ctrl-D at one does not end a session
    /// that is holding a turn open waiting to be answered.
    ///
    /// **Escape and Ctrl-C both take the panel down and they mean different
    /// things, so they send different messages -- one each.**
    ///
    /// Escape declines the *question*: `QuestionCancelled` under the batch's own
    /// id, the model is told so with its own sentinel, and the turn carries on
    /// with whatever it was doing. Ctrl-C stops the *turn*
    /// ([`Self::interrupt`]), and sends only that: the requester parked on this
    /// channel treats an interrupt as its question being over, answers the tool
    /// with the same sentinel, and hands the interrupt **back** to the loop that
    /// can stop the turn (`super::question::TuiQuestioner`).
    ///
    /// Sending both, as this once did, is what made a Ctrl-C at a question a
    /// declined question that also stopped a turn *later*: the requester
    /// answered on the first message and returned while the second was still in
    /// the channel, so the calls behind the question ran before anything had
    /// read the interrupt. One key, one message, and the turn's own stop is the
    /// one the machine's boundary can see (`crate::agent::machine`'s
    /// `execute_tool_calls`).
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
            // Content rather than keys, and there is nothing here for it to be
            // content of: the composer is not what has the focus.
            Input::PasteByte(_) => return,
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
                // **The panel goes before anything is sent**, so the band's next
                // paint has no question in it whatever the runtime does next --
                // including asking a second batch straight away. The order
                // [`Self::decide`] uses, for the same reason.
                self.ask = None;
                self.work
                    .control(TurnControl::QuestionAnswer { id, answers });
                self.settled_question();
            }
            question::Answered::Cancelled => {
                self.ask = None;
                if matches!(act, question::Act::Cancel) {
                    // The turn's own stop, and **only** it: the requester reads
                    // this as its question being over and gives it back for the
                    // loop to act on. A `QuestionCancelled` in front of it would
                    // release the turn to run the rest of its step before the
                    // interrupt had been read by anyone.
                    self.settled_question();
                    self.interrupt(now);
                } else {
                    self.work.control(TurnControl::QuestionCancelled { id });
                    self.settled_question();
                }
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
    /// interrupt notice and the sentence saying the queue went with it
    /// ([`Self::interrupt`]). Both are what a Ctrl-C is answered with, so both
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
        // Anything that is not the answer ends the answer's block.
        self.close_answer();
        match mark {
            Mark::Card(text) => self.write_card(&text),
            Mark::Summary(line) => {
                self.write_document_line(&line, Role::Dim);
                self.end_transcript_line();
            }
            Mark::Line(line, role) => self.write_document_line(&line, role),
            Mark::Notice(notice) => self.write_notice(&notice),
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
                self.settle_group();
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
        let Some(rows) = self.group.redraw(self.palette) else {
            return;
        };
        if self.transcript.queue_block(rows) {
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
        // Straight to the transcript: the block ends as a line does, and its
        // last row is already the blank one that separates it from what
        // follows.
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
    fn settle_group(&mut self) {
        let stopped = self.group.cancel_running();
        if !stopped.is_empty() {
            self.redraw_group();
        }
        self.abandoned.extend(stopped);
        self.close_group();
        self.group = ToolGroup::default();
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
    /// Like [`Self::write_document_line`], whatever the answer had open is
    /// closed first: a notice does not land in the middle of a sentence.
    fn write_notice(&mut self, notice: &notice::Notice) {
        self.close_answer();
        self.finish_document_line();
        // The gutter in force is the one the row above is written with.
        if self.transcript.queue_gutter(Gutter::None) {
            self.owed();
        }
        if !self.transcript.ends_blank() {
            self.end_transcript_line();
        }
        if self.transcript.queue_gutter(Gutter::Head(notice.tone)) {
            self.owed();
        }
        // One push for the label and the whole body: the head is one row's,
        // and every line after the first is written two columns in by the
        // gutter itself (`Gutter::continued`), so nothing here splits lines.
        let label = notice.label();
        let body = notice.said();
        let gap = if label.is_empty() || body.is_empty() {
            ""
        } else {
            " "
        };
        if self.transcript.queue_push_runs(&[
            (&label, Role::Label(notice.tone)),
            (gap, Role::Notice),
            (body, Role::Notice),
        ]) {
            self.owed();
        }
        self.end_transcript_line();
        if self.transcript.queue_gutter(Gutter::None) {
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
        let queued = self.work.queued();
        if queued != self.queued {
            self.queued = queued;
            self.render.request(Reason::Footer);
        }
        let armed = self.gestures.escape_armed(now);
        if armed != self.escape_armed {
            self.escape_armed = armed;
            self.render.request(Reason::Footer);
        }
        // What the turn is doing, on the row above the divider. Before the
        // pacer, because the row's arrival and departure move the divider and
        // an append measured against the other band would be placed a row out.
        self.tick_activity(now);
        // And the answer itself: the pacer holds text against a clock, so a
        // turn of the loop is what releases it. Last, so the rows it adds are
        // measured against a band this turn has already settled.
        self.pace(now);
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
        let row = if self.modal() {
            None
        } else {
            self.activity.row(
                now,
                self.phase,
                self.geometry.cols,
                (self.tally.input, output),
                colours,
            )
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
    /// from it is how erased history comes back.
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
        if !self.damage_untold {
            self.retint_debt = true;
        }
        self.render.request(Reason::Footer);
        if self.slot().is_some() || self.owner == ScreenOwner::Approval {
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
    /// The pair [`Self::take_clearing`] and [`Self::restore_clearing`] make,
    /// for the same reason: a refused write moved no byte, so what it was
    /// offering is still owed, and nothing else ever sets this flag for a
    /// resume that has already been adopted.
    pub(crate) fn restore_theme_query(&mut self) {
        self.theme_query = true;
    }

    /// Whether the screen owes a `/clear`, taken so it is written once.
    pub(crate) fn take_clearing(&mut self) -> bool {
        std::mem::take(&mut self.clearing)
    }

    /// Hands a `/clear` back after [`Self::take_clearing`] took it and the
    /// write never reached the terminal.
    ///
    /// Sets the flag and nothing else -- it must not call
    /// [`Self::clear_screen`] again: that would drop the pending appends and
    /// pacer text a **second** time, discarding whatever arrived between the
    /// take and this call, and repeat a notice the screen never saw the first
    /// half of. The bytes are the only thing that failed to land; the state
    /// `clear_screen` already changed did not un-happen.
    pub(crate) fn restore_clearing(&mut self) {
        self.clearing = true;
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
            if self.asking() {
                self.decide(event, now);
                continue;
            }
            // And the catalog or the completion menu -- never both -- takes
            // the keys it binds, before the editor and before the gestures see
            // them. Everything else falls through with it still up, because
            // each is a view of the draft and typing is what the draft is made
            // of.
            if self.offer_to_catalog(&event) {
                continue;
            }
            if self.offer_to_picker(&event) {
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
            // A lone Escape does nothing here; the second one inside the window
            // clears the composer, and the hint row says so in between.
            Action::Escape => match self.gestures.escape(now) {
                Escape::Armed => self.settle_band(now),
                Escape::Clear => {
                    self.clear_composer();
                    self.settle_band(now);
                }
            },
            // The end of the session, but only from an empty composer: with
            // text under the caret Ctrl-D is the forward delete it is in every
            // shell that has both, and leaving would throw away a draft.
            Action::Eof => {
                if self.editor.is_empty() {
                    self.leave();
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
            Action::Tab | Action::BackTab | Action::Ignore => {}
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

    /// One Ctrl-C.
    ///
    /// The cancellation goes out on the **control** channel, which is unbounded
    /// and read by the runtime *inside* the turn (`super::worker`'s `run_turn`),
    /// so it cannot queue behind the backlog of deltas it is trying to stop.
    /// The UI deliberately does not cancel anything itself: what it holds is the
    /// **session's** cancellation, and cancelling that would make every later
    /// turn be born cancelled (`super::bridge`'s `Cancellation::turn`) -- one
    /// Ctrl-C would end the conversation rather than the answer.
    fn interrupt(&mut self, now: Instant) {
        match self.gestures.interrupt(now, self.work.outstanding() > 0) {
            Interrupt::Cancel => {
                // Read **before** the message goes out, because after it the
                // runtime is dropping exactly these and the count is on its way
                // to zero: what the user is owed a sentence about is what was
                // waiting when they pressed the key.
                let waiting = self.work.queued() > 0;
                // Quoting what has been submitted **so far** is what keeps the
                // drop from reaching past this keystroke: a prompt typed while
                // this message is still in flight is a new intention, and the
                // runtime is told exactly where the old ones stop.
                self.work.control(TurnControl::Cancel {
                    through: self.work.accepted(),
                });
                // A call that is already running is **not** settled by this
                // keystroke, and that is the difference from upstream's
                // `fx-interrupt-40x120-cmd-cc`, where the command is killed at
                // once. Here a running executor holds the runtime's thread
                // until it returns, so the stop is read only after the command
                // has finished (`super::worker`'s `raced_against_control`
                // polls the control channel beside the turn, not inside a
                // tool). A `Cancelled` row now would be the screen claiming a
                // stop the command never had; the call's real result settles
                // it, and a turn that ends without one settles it as stopped
                // ([`Mark::TurnOver`]). A call parked at an approval is the
                // other case, and is settled where that key is read
                // ([`approval::Action::Cancel`]).
                //
                // The same sentence the line-oriented shell writes for the same
                // keystroke (`app::INTERRUPT_NOTICE`), so that the request is
                // something the user watched land rather than something they
                // have to infer from the stream stopping -- which, for a
                // provider that has gone quiet without hanging up, it may not.
                //
                // **Written at once rather than through [`Self::say`]**, and
                // so is the sentence below it: they are the **two** document
                // lines of this session that are, and they are a pair rather
                // than one rule and an exception. Every other line is *about
                // the answer* and belongs at the point of it the stream had
                // reached. These two are about the **keystroke** -- they are
                // what the user's Ctrl-C is answered with, both of them -- and
                // a keystroke's answer that waited for thirteen seconds of
                // paced text would not be an answer to it at all.
                //
                // The cost is that they land inside the answer rather than
                // after it, which is what "stop here" means; the pacer is told
                // the turn is over a moment later, so what is left of that
                // answer follows at the drain rate rather than at reading
                // speed.
                self.write_notice(&notice::Notice::cancelled(
                    "turn",
                    unattributed(crate::app::INTERRUPT_NOTICE),
                ));
                if waiting {
                    // The second half of the same answer to the same keystroke,
                    // and immediate for the same reason: "and the queue went
                    // with it" is only useful beside the sentence it qualifies.
                    // Held back, it would arrive detached from the notice it
                    // belongs to and after text the user asked to stop.
                    self.write_notice(&notice::Notice::cancelled("queue", QUEUE_DROPPED));
                    self.prompts.clear();
                }
            }
            Interrupt::Clear => self.clear_composer(),
            Interrupt::Leave => self.leave_by(Leaving::Interrupted),
        }
        self.settle_band(now);
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
    /// Most of them are answered on this thread. `/new`, `/model` and `/setup`
    /// go to the runtime as [`TurnWork`] -- `/new` and `/model <id>` because
    /// the conversation and the model they change live there and have to change
    /// *between* turns rather than under one, and `/setup` and a bare `/model`
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
                self.prompts.push_back((estimate.estimate(), title));
                self.echo(text);
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

/// What each canonical command does on this surface.
///
/// Most are answered on the UI thread; `/new`, `/model` and `/setup` are
/// handed to the runtime as [`TurnWork`] -- because the model and the
/// conversation they change live there and have to change *between* turns
/// rather than under one, and because `/setup` and a bare `/model` open a
/// socket the thread holding the terminal must never wait on.
impl CommandHandlers for Shell {
    /// Opens the catalog (`commandShowHelp`, `app_commands.zig:700-706`):
    /// `All`, the first command marked, the composer -- emptied by the submit
    /// that got here -- as its search.
    ///
    /// **Nothing is written to the document**: the catalog is the answer, and
    /// it is in the band. The line-oriented shell keeps its page
    /// ([`interactive::help_text`]); it has no band to put a catalog in.
    fn help(&mut self) {
        // The completion menu goes first: the two share the slot, and the
        // catalog is what the user asked for.
        self.picker = None;
        self.help_menu = Some(HelpMenu::open(self.editor.text()));
        // [`Self::run_command`] re-solved the band before routing here, for
        // a band with no catalog in it.
        self.refit();
        self.render.request(Reason::Modal);
    }

    fn new_session(&mut self) {
        if let Err(rejected) = self.work.submit(TurnWork::New) {
            self.refused(rejected);
            return;
        }
        // **After the offer was taken, and not before.** The far side drops the
        // conversation (`super::worker`'s `TurnWork::New` arm), and the meter's
        // numerator is a measurement *of that conversation* -- so it goes with
        // it, and a `/new` the runtime refused leaves a session whose
        // measurement is still about the conversation on the screen.
        //
        // The denominator stays: it is the model's context window, and `/new`
        // changes neither the provider nor the model. Clearing it would discard
        // a fact that is still true and cost a socket to learn again.
        self.context_used = None;
        self.render.request(Reason::Footer);
        // The session the title named is the one being dropped
        // (`clearCachedSessionTitle`, `app_session_runtime.zig:1982`) -- and so
        // is every prompt already waiting: the work runs in order, so each of
        // them is answered in the conversation `/new` drops, and none of them
        // may title the one it starts.
        self.forget_session_title();
        for (_, title) in &mut self.prompts {
            *title = None;
        }
        self.announce(notice::Notice::neutral("session", NEW_SESSION_NOTICE));
    }

    fn clear(&mut self) {
        self.clear_screen();
    }

    fn model(&mut self, argument: &str) {
        self.use_model(argument);
    }

    /// Upstream's `* version: 0.0.13` (`app_commands.zig:2153-2160`): the
    /// bare version, no `v` and no build. The build is the welcome line's to
    /// say, and the channel and revision are `xfx status`'s.
    fn version(&mut self) {
        self.announce(notice::Notice::neutral("version", crate::VERSION));
    }

    fn quit(&mut self) {
        self.leave();
    }

    fn setup(&mut self, argument: &str) {
        self.use_provider(argument);
    }
}

impl Shell {
    /// `/model`, with the line-oriented shell's meaning.
    ///
    /// With no argument it **reports** and asks for the catalog; with one it
    /// hands the id to the thread that owns the rule and paints nothing about
    /// the result. [`TurnWork::Model`] is answered by
    /// `crate::provider::model::ModelSelector::apply` on the runtime thread
    /// (`super::worker`'s `run_model`), which is where the whole catalog is --
    /// so whether an id is applied, refused as one the provider does not
    /// publish, or applied unverified is **not a question this side can
    /// answer**. What it does answer is the one question that is about the
    /// argument rather than about the catalog: what a model id may be
    /// ([`model_id_problem`], the first thing `apply` asks), refused here so
    /// that an id carrying a control character never reaches the log the far
    /// side writes it into.
    ///
    /// The result comes back as [`UiEvent::ModelAnswered`], which is what moves
    /// this shell's own model and paints the line.
    ///
    /// **Narrower than the line shell in one way, and it is a boundary rather
    /// than an omission**: that shell loads the provider's catalog to report
    /// with, and prints it inline. The load is asynchronous on a thread that
    /// must not wait for anything, so the TUI reports the model in force at
    /// once and the rows arrive afterwards as their own event.
    fn use_model(&mut self, argument: &str) {
        if argument.is_empty() {
            // Upstream's bare `/model` report is `* model: <id>`
            // (`handleModel`); the provider rides beside it because on this
            // surface it is the one other thing a model id is relative to.
            self.announce(notice::Notice::neutral(
                "model",
                format!("{} provider={}", self.model, self.provider.label()),
            ));
            // **The one network call `/model` makes**, and it is made on the
            // runtime thread rather than here: the UI thread sits in
            // `pselect(2)` holding the terminal, and a daemon that is not
            // answering must not be something it waits for. The rows arrive
            // later as [`UiEvent::CatalogLoaded`], which is why this method
            // returns having painted only the report.
            if let Err(rejected) = self.work.submit(TurnWork::Catalog) {
                self.refused(rejected);
            }
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
        // **And nothing else is decided here.** Whether the id is the one
        // already in force, one the provider publishes, or one it does not, is
        // the selector's to say -- and it says all three from the same catalog
        // (`ModelSelector::apply`). A short-circuit for "the same model again"
        // on this side would be a second reading of a fact this side keeps a
        // copy of rather than owns.
        if let Err(rejected) = self.work.submit(TurnWork::Model(argument.to_string())) {
            self.refused(rejected);
        }
    }

    /// `/setup <provider>`, with the line-oriented shell's meaning.
    ///
    /// The command hands the whole transaction to the runtime thread and paints
    /// nothing about its result: it writes a file, opens a socket and re-reads a
    /// configuration, none of which may happen on the thread holding the
    /// terminal, and **none of which this side is entitled to predict.** What
    /// the session becomes arrives as [`UiEvent::ProviderSelected`] *after* the
    /// reload, so the model and the credential fact this shell shows are the
    /// ones the configuration really resolved to -- not the ones the write
    /// intended, which differ exactly when a layer above the profile outranks
    /// it.
    ///
    /// The name is parsed here only to refuse an argument that names no
    /// provider, which costs the runtime nothing to be told and would otherwise
    /// spend a queue place to come back as a failure.
    fn use_provider(&mut self, argument: &str) {
        let Some(provider) = ProviderId::parse(argument) else {
            // xfx's own words, derived from the providers this build has
            // (`interactive::setup_usage`), so a build that grows one does not
            // grow a sentence that forgets it. Nothing of the argument is quoted
            // back, for the reason `/model`'s refusal quotes nothing.
            self.write_notice(&notice::Notice::error(
                "",
                unattributed(&interactive::setup_usage()),
            ));
            return;
        };
        if let Err(rejected) = self.work.submit(TurnWork::Setup(provider)) {
            self.refused(rejected);
            return;
        }
        self.announce(notice::Notice::neutral(
            "provider",
            format!("setting up {}", provider.label()),
        ));
    }

    /// `/clear`: the screen, its scrollback, and what the band remembers of
    /// both.
    ///
    /// Three things go together and none of them is optional. The **bytes** are
    /// the loop's to write, because the loop owns the writer. The **transcript**
    /// is reset, because it counts the rows it has put on the screen and every
    /// one of them is about to stop existing -- an append measured against the
    /// old count would place its rows around a row that is no longer there. And
    /// the document writes already owed are **dropped** with it, because they
    /// were owed against that same screen.
    ///
    /// One statement for both, and that is the point: the owed text lives in
    /// the transcript, so there is no second place to forget to empty and no
    /// order between them to get wrong.
    fn clear_screen(&mut self) {
        self.transcript = Transcript::new(self.geometry.cols);
        // Its block went with the screen; a call still running is drawn again
        // on the cleared one when its result arrives.
        self.group = std::mem::take(&mut self.group).unsettled();
        // The stream goes with them, and it is the one place this session
        // drops text the runtime produced. The rows it was going to be written
        // on are being erased *and* taken out of the terminal's scrollback at
        // the user's request; letting the rest of that answer dribble onto the
        // blank screen afterwards would be the surprise, not the loss. The
        // marks go with it because their places do -- and the stream is
        // declared arrived, so nothing later is held behind a position no
        // emission will ever reach.
        self.pacer.forget();
        self.marks.clear();
        self.emitted = self.enqueued;
        self.clearing = true;
        // The band starts again under nothing, as it did at launch.
        self.document_rows = 0;
        self.refloat();
        // Every row of the band is gone from the screen too, so the next frame
        // is a repaint of the whole thing rather than an optional one -- and
        // the document's rows are gone with it, which is what takes any
        // visible recolour owed against them down ([`Self::external_damage`]).
        self.external_damage();
        self.announce(notice::Notice::neutral("screen", CLEARED_NOTICE));
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
        // again for each composer height the search below tries; every other
        // occupant's is asked once.
        let slot = self.slot();
        let fixed = match slot {
            Some(Slot::Question(panel)) => panel.height(cols, rows),
            Some(Slot::Ask(ask)) => ask.height(cols, rows),
            Some(Slot::Menu(picker)) => picker.height(rows),
            Some(Slot::Catalog(_)) | None => 0,
        };
        let panel = |input_rows: u16| match slot {
            Some(Slot::Catalog(menu)) => menu.height(rows, input_rows.saturating_sub(1)),
            _ => fixed,
        };
        // **The composer gives way to the question, one row at a time.** A
        // panel and a tall draft together can want more rows than the screen
        // has, and the draft is the half that can afford to lose one: a
        // composer shown two rows shorter still shows the caret and the text
        // around it ([`editor::window`]), while a panel with its choices below
        // the last row of the screen is a question with no visible answers. The
        // search is bounded by the cap and always finds an answer for a panel
        // that [`layout::fits_panel`] admitted, because that is the same
        // question asked of a one-row composer.
        (1..=wanted)
            .rev()
            .find_map(|input_rows| {
                layout::solve_band(rows, cols, input_rows, activity, panel(input_rows))
            })
            .map(|geometry| geometry.floated(self.document_rows))
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
        let Some(geometry) = self.fit(rows, cols) else {
            return Resize::TooSmall;
        };
        self.geometry = geometry;
        self.transcript.resize_unfinished(cols);
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
            ["auto", "yolo"]
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
            if self.shell.screen_owner() == ScreenOwner::Approval {
                let row = usize::from(self.shell.screen_cursor().0);
                return self
                    .shell
                    .screen_rows()
                    .get(row - 1)
                    .cloned()
                    .unwrap_or_else(|| panic!("the caret is not on a row of the approval screen"));
            }
            let offset = usize::from(
                self.shell
                    .cursor()
                    .0
                    .saturating_sub(self.shell.geometry.band_top()),
            );
            self.shell
                .band_rows()
                .get(offset)
                .cloned()
                .unwrap_or_else(|| panic!("the caret is not on a row of the band"))
        }
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
        format!("{IDLE_HINT}{}{ESCAPE_ARMED}", " ".repeat(padding))
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
                rows: vec!["answered".to_string()]
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
                    rows: vec!["first".to_string(), String::new()]
                },
                Append {
                    scroll: 0,
                    rows: vec!["second".to_string()]
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
                rows: vec!["x".repeat(20), "x".repeat(5)]
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
                rows: vec!["x".repeat(20), "x".repeat(5)]
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
                rows: vec!["abc".to_string(), String::new()]
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
                    rows: vec![card_row("ask me")]
                },
                Append {
                    scroll: 1,
                    rows: vec![String::new()]
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
        // `/new` and `/model` travel on the work channel like a prompt, so the
        // runtime has work in hand for one -- but nothing is being answered.
        // A session that started a clock because a turn ended with something
        // still queued would say `Thinking` about a command it had already
        // answered on its own thread.
        for command in [b"/new\r".as_slice(), b"/model other\r".as_slice()] {
            let mut shell = shell(24, 80);
            let started = turn_running(&mut shell, b"ask something\r");
            shell.route_bytes(command);
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
        }
    }

    /// How many rows [`asked`] really needs on an eighty-column screen.
    ///
    /// **A changed coordinate, and this is why.** It used to be eight, which
    /// was the compact entry of a three-row table the panel no longer has: its
    /// heights are measured from the request now
    /// (`super::super::approval`'s `Shape::for_request`), because every
    /// always-scope `crate::permission` builds is three wrapped rows at eighty
    /// columns and a fixed two cut it. This fixture carries the short
    /// hand-written scope, so what it demands is a title, one row of summary,
    /// the three answers and one row of scope. Named once rather than spelled
    /// at each of its seven uses, so that the next change to the wording moves
    /// one number.
    const PANEL_ROWS: u16 = 6;

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

    /// The same question about a change too big for the band's own summary.
    fn asked_about_a_large_change() -> ApprovalRequest {
        let mut request = asked();
        request.diff = Some(crate::permission::ApprovalDiff {
            before: "a".repeat(4_000),
            after: "b".repeat(4_000),
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
    const INTERRUPTED_ROW: &str = "\u{2298} turn: interrupted";

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
                let painted = shell.band_rows().join(" ");
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
            before: "a".repeat(4_000),
            after: "b".repeat(4_000),
        });
        shell.apply(UiEvent::Approval(question(request)));
        assert_eq!(shell.screen_owner(), ScreenOwner::Approval);
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
        let notice = unwrapped(&shell.screen_rows());
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
            notice.contains("3. No (esc)"),
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
            before: "a".repeat(4_000),
            after: "b".repeat(4_000),
        });
        wide.apply(UiEvent::Approval(question(request)));
        assert_eq!(wide.screen_owner(), ScreenOwner::Approval);
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
        assert_eq!(shell.screen_owner(), ScreenOwner::Approval);
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
        for key in [Input::Text('3'), Input::Action(Action::Escape)] {
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
        // Ctrl-C needs a turn to be about ([`Shell::interrupt`] reads
        // `WorkHandle::outstanding`), so this half is driven with one running.
        let mut shell = shell(24, 80);
        let started = turn_running(&mut shell, b"edit the notes\r");
        shell.apply(UiEvent::Approval(question(asked())));
        shell.decide(Input::Action(Action::Cancel), started);
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
    fn a_question_takes_rows_between_the_composer_and_the_rule() {
        // Upstream's picker slot (`footer_layout.zig:85-96`): the question sits
        // under the composer between two rules, so the bottom rule and the hint
        // row stay put and the composer moves up by the panel and the rule that
        // heads it. Those rows come off the bottom of the document.
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
        assert_eq!(
            shell.geometry.input_first,
            working.input_first - PANEL_ROWS - 1
        );
        assert_eq!(shell.geometry.hint, working.hint);
        assert_eq!(
            shell.geometry.content_bottom,
            working.content_bottom - PANEL_ROWS - 1
        );

        let rows = shell.band_rows();
        assert_eq!(
            rows.len(),
            usize::from(shell.geometry.band_rows()),
            "the band painted a different number of rows than it solved for"
        );
        let painted = rows.join("\n");
        assert!(painted.contains("Permission needed"), "{painted}");
        assert!(
            painted.contains("replace \"alpha\" with \"beta\""),
            "{painted}"
        );
        assert!(
            painted.contains("don't ask again for this request"),
            "{painted}"
        );
        assert!(
            painted.contains("for the rest of this session"),
            "the panel never said what \"always\" would grant: {painted}"
        );
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

        assert_eq!(shell.marked(), "> 1. Yes");
        shell.route_bytes(&[0x1b, 0x5b, 0x42]); // Down
        assert!(
            shell.marked().starts_with("> 2. Yes, and"),
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
    fn escape_at_a_question_refuses_it_rather_than_arming_the_clear() {
        // The double-Escape gesture is the composer's, and the composer does
        // not have the focus. A first Escape that armed it would leave a
        // question up and a warning about a draft the user is not editing.
        let mut shell = shell(24, 80);
        let started = turn_running(&mut shell, b"edit the notes\r");
        shell.route_bytes(b"a draft");
        shell.apply(UiEvent::Approval(question(asked())));
        shell.settle_band(started);

        shell.route_bytes(&[0x1b]);
        shell.settle_input(Instant::now() + Duration::from_millis(100));

        assert_eq!(
            shell.controlled(),
            Some(TurnControl::Answer {
                id: ApprovalId(1),
                answer: ApprovalAnswer::Deny,
                feedback: None,
            })
        );
        assert!(!shell.hint().contains(ESCAPE_ARMED), "the clear was armed");
        assert_eq!(
            shell.editor.text(),
            "a draft",
            "the refusal threw the draft away"
        );
    }

    #[test]
    fn ctrl_c_at_a_question_is_the_interrupt_it_is_everywhere_else() {
        // **The message on the wire is a cancellation, not an answer**, and the
        // difference is the whole of this case. The refusal of the question
        // comes back with it: the prompter is what is parked on this channel,
        // and it turns a cancellation into a `Deny` and hands the cancellation
        // on to the loop that stops the turn and drops the queue
        // (`super::approval::TuiPrompter`, and the composed proof in
        // `tests/tui.rs`). An `Answer(Deny)` sent from here would be the only
        // thing the runtime ever heard, and the interrupted turn would carry on.
        let mut shell = shell(24, 80);
        // A prompt is waiting behind the interrupted turn, so the message this
        // sends has to carry the watermark that says which waiting work the
        // keystroke was about.
        asking_with_one_waiting(&mut shell);
        let accepted = shell.work.accepted();

        shell.route_bytes(&[0x03]);

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
        // And the user was told, in the two sentences a Ctrl-C is answered with
        // wherever it is typed.
        let said = shell.released();
        assert!(
            said.contains(
                &notice::Notice::cancelled("turn", unattributed(crate::app::INTERRUPT_NOTICE))
                    .text()
            ),
            "the interrupt landed silently: {said:?}"
        );
        assert!(
            said.contains(&notice::Notice::cancelled("queue", QUEUE_DROPPED).text()),
            "the queue went with it and nobody said so: {said:?}"
        );
        assert_eq!(shell.geometry.panel, 0, "the question stayed on the screen");
        assert!(
            !shell.leaving(),
            "one Ctrl-C at a question left the session"
        );
    }

    #[test]
    fn escape_at_a_question_answers_only_the_question_it_is_about() {
        // The other half of the pair: Esc is an answer about *this call* and
        // nothing more. A turn stopped by it would make the two refusals mean
        // different things, and the panel says `3. No (esc)` about only one.
        let mut shell = shell(24, 80);
        asking_with_one_waiting(&mut shell);

        shell.route_bytes(&[0x1b]);
        shell.settle_input(Instant::now() + Duration::from_millis(100));

        assert_eq!(
            shell.controlled(),
            Some(TurnControl::Answer {
                id: ApprovalId(1),
                answer: ApprovalAnswer::Deny,
                feedback: None,
            })
        );
        assert_eq!(shell.controlled(), None, "Esc cancelled the turn as well");
        assert_eq!(
            shell.hint(),
            queued_hint(1),
            "Esc dropped the prompt that was waiting"
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
            ScreenOwner::Approval,
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
        // walk, and the refusal keys -- because a surface that changed which
        // keystroke means what would be a second input model to get wrong.
        for (typed, answer) in [
            (&b"1"[..], ApprovalAnswer::Once),
            (&b"2"[..], ApprovalAnswer::Always),
            (&b"3"[..], ApprovalAnswer::Deny),
            (&[0x1b][..], ApprovalAnswer::Deny),
        ] {
            let mut shell = shell(24, 80);
            let started = turn_running(&mut shell, b"edit the notes\r");
            shell.route_bytes(b"a draft");
            shell.apply(UiEvent::Approval(question(asked_about_a_large_change())));
            shell.settle_band(started);
            assert_eq!(shell.screen_owner(), ScreenOwner::Approval);
            assert_eq!(shell.marked(), "> 1. Yes", "the caret left the question");
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
    fn an_interrupt_at_a_question_the_approval_plane_owns_gives_the_screen_back_as_well() {
        // Ctrl-C is the one key that is not an answer on this surface -- it
        // stops the turn and the prompter turns that into the refusal. The
        // screen has to come back on that path too, or an interrupt would leave
        // the session composing frames for a plane with no question on it.
        let mut shell = shell(24, 80);
        let started = turn_running(&mut shell, b"edit the notes\r");
        shell.apply(UiEvent::Approval(question(asked_about_a_large_change())));
        shell.settle_band(started);
        assert_eq!(shell.screen_owner(), ScreenOwner::Approval);

        shell.route_bytes(&[0x03]);

        assert_eq!(shell.screen_owner(), ScreenOwner::Primary);
        assert!(
            matches!(shell.controlled(), Some(TurnControl::Cancel { .. })),
            "the interrupt was answered instead of being passed on"
        );
    }

    #[test]
    fn a_short_screen_reviews_a_large_change_on_the_plane_that_can_show_it() {
        // **The band's fit is the band's question.** A change too big for the
        // summary is never asked in the band at all -- it takes a plane of its
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

        assert_eq!(shell.screen_owner(), ScreenOwner::Approval);
        assert_eq!(
            shell.controlled(),
            None,
            "a question the plane can show was answered on the user's behalf"
        );
        let painted = shell.screen_rows().join("\n");
        for choice in ["1. Yes", "2. Yes, and", "3. No (esc)"] {
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
                    ScreenOwner::Approval,
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
        assert_eq!(shell.screen_owner(), ScreenOwner::Approval);

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
        let screen = screen.join("\n");
        for needle in ["Permission needed", "notes.txt", "1. Yes", "3. No (esc)"] {
            assert!(screen.contains(needle), "{needle:?} is missing: {screen:?}");
        }
        assert!(
            screen.contains("aaaa"),
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
        assert_eq!(shell.screen_owner(), ScreenOwner::Approval);

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

        // `C-n`, which is "the line after this one" everywhere else on this
        // surface and was bound to nothing at a question.
        shell.route_bytes(&[0x0e]);
        let moved = shell.screen_rows().join("\n");
        assert_ne!(first, moved, "the change could not be walked");
        assert_eq!(
            shell.controlled(),
            None,
            "walking the change answered the question"
        );

        shell.route_bytes(&[0x10]);
        assert_eq!(
            shell.screen_rows().join("\n"),
            first,
            "`C-p` did not walk back"
        );
        assert_eq!(shell.screen_owner(), ScreenOwner::Approval);
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
        assert_eq!(shell.marked(), "> 1. Yes");
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
            .any(|row| row.contains("1. Thorough")));
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
                .all(|row| !row.contains("1. Thorough")),
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
        assert!(
            fixture.band_rows().iter().any(|row| row.contains("2 of 2")),
            "the batch did not move to its second question: {:?}",
            fixture.band_rows()
        );
    }

    #[test]
    fn escape_at_a_question_cancels_the_batch_and_arms_nothing_else() {
        let mut fixture = shell(24, 80);
        asking_question(&mut fixture, a_batch());
        // A lone `ESC` is the Escape key only once it has gone quiet, which is
        // the decoder's own timeout and the settle is what resolves it
        // (`escape_at_a_question_refuses_it_rather_than_arming_the_clear`).
        fixture.route_bytes(&[0x1b]);
        fixture.settle_input(Instant::now() + Duration::from_millis(100));
        assert!(matches!(
            fixture.control.try_recv(),
            Ok(TurnControl::QuestionCancelled { id: QuestionId(1) })
        ));
        assert!(
            fixture.control.try_recv().is_err(),
            "Escape is not also a clear"
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
        assert!(
            fixture
                .band_rows()
                .iter()
                .any(|row| row.contains("7. Other")),
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
        assert_eq!(fixture.marked(), "> 1. Thorough - reads every file");
        fixture.route_bytes(&[0x1b, 0x5b, 0x42]); // Down
        assert_eq!(fixture.marked(), "> 2. Quick");
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
        assert_eq!(
            row,
            fixture.geometry.divider - 1,
            "the caret is not on the draft row"
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
        // queue behind the deltas it is trying to stop -- and the notice is the
        // line shell's own, so the user watches the request land rather than
        // inferring it from a stream that may not stop for a while.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"stream something\r");
        let _taken = shell.picks_up();
        let _ = shell.document();

        shell.route_bytes(&[0x03]);

        assert_eq!(
            shell.control.try_recv(),
            Ok(TurnControl::Cancel { through: 1 })
        );
        // The line shell's words, as a cancelled notice: the glyph says the
        // sentence is xfx's, so its `xfx: ` comes off.
        assert_eq!(
            shell.document(),
            said(notice::Notice::cancelled(
                "turn",
                "interrupted -- stopping the turn; another Ctrl-C before it stops exits."
            ))
        );
        assert!(
            crate::app::INTERRUPT_NOTICE.ends_with(
                "interrupted -- stopping the turn; another Ctrl-C before it stops exits."
            ),
            "the two surfaces no longer say the same words"
        );
        assert!(!shell.leaving(), "one Ctrl-C ended the session");
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
        // Two notices, sharing the blank row between them.
        assert_eq!(
            shell.document(),
            [
                said(notice::Notice::cancelled(
                    "turn",
                    unattributed(crate::app::INTERRUPT_NOTICE)
                )),
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

        assert_eq!(
            shell.document(),
            said(notice::Notice::cancelled(
                "turn",
                unattributed(crate::app::INTERRUPT_NOTICE)
            ))
        );
    }

    #[test]
    fn a_turn_that_ended_takes_the_interrupt_that_stopped_it_with_it() {
        // The window this closes is small and entirely real: the runtime gives
        // its place back **after** the terminal event (`super::worker`'s
        // `turn_loop`), so for a moment the UI has been told the turn is over
        // while the count still says something is in hand. A session that kept
        // remembering the Ctrl-C that stopped that turn would read the next one
        // -- a keystroke the user meant as "clear the prompt" -- as the second
        // half of an exit, and leave with 130.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"first\r");
        let _running = shell.picks_up();
        shell.route_bytes(&[0x03]);
        assert_eq!(
            shell.control.try_recv(),
            Ok(TurnControl::Cancel { through: 1 })
        );

        // The turn concludes. Nothing is submitted in between -- that is the
        // whole point: a later submission would reset the gesture by itself and
        // this claim would be about `submitted` rather than about the boundary.
        shell.apply(UiEvent::TurnEnded { failure: None });
        let _ = shell.document();

        shell.route_bytes(&[0x03]);

        assert!(
            !shell.leaving(),
            "the session left on the first Ctrl-C after a turn ended, because \
             it still remembered the one that stopped that turn"
        );
        assert_eq!(
            shell.control.try_recv(),
            Ok(TurnControl::Cancel { through: 1 }),
            "the keystroke did nothing at all"
        );
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

            assert!(
                shell.editor.is_empty(),
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
            // a row, and `/help` the one whose answer is the catalog in the
            // band; every other one says something, as a notice.
            if *name == "/quit" {
                assert!(shell.leaving(), "/quit did not end the session");
            } else if *name == "/help" {
                assert!(shell.help_menu.is_some(), "/help opened no catalog");
                assert!(
                    document.is_empty(),
                    "/help wrote to the document: {document:?}"
                );
                assert!(!shell.leaving(), "{name} ended the session");
            } else {
                // A bare `/setup` is a usage error and every other one a
                // neutral answer; either way a notice, glyph first.
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
        let _ = shell.picks_up();

        // A prompt that was waiting when `/new` was taken is answered in the
        // conversation `/new` drops, so its turn titles nothing.
        let mut queued = shell_with(24, 80, PALETTE, false);
        let _ = queued.take_title();
        queued.route_bytes(b"asked before the new session\r");
        queued.route_bytes(b"/new\r");
        let _ = queued.picks_up();
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

    #[test]
    fn model_with_an_argument_reaches_the_runtime_and_model_without_one_reports() {
        let mut shell = shell(24, 80);
        shell.route_bytes(b"/model second-model\r");
        assert_eq!(
            shell.picks_up(),
            TurnWork::Model("second-model".to_string()),
            "the model change never reached the thread that owns the session log"
        );
        // The thread that owns the rule answers, and that answer -- not the
        // submission -- is what the band's own model becomes.
        shell.apply(UiEvent::ModelAnswered {
            model: "second-model".to_string(),
            outcome: ModelAnswer::Applied { unverified: None },
        });

        shell.route_bytes(b"/model\r");
        let document = shell.document();
        assert!(
            document
                .iter()
                .any(|row| row.starts_with("* model: second-model")),
            "a bare /model did not report the model in force: {document:?}"
        );
        // And it asks the runtime for the catalog -- **the one network call
        // `/model` makes**, and the reason it is a piece of work rather than
        // something answered on this thread: the UI thread owns the terminal and
        // may not wait for a daemon. The report above is painted first and
        // without waiting for it, so a provider that is down still answers
        // `/model`.
        assert_eq!(
            shell.picks_up(),
            TurnWork::Catalog,
            "a bare /model did not ask the runtime to load the catalog"
        );
        assert!(
            shell.sent.try_recv().is_err(),
            "a bare /model asked the runtime for more than the catalog"
        );
    }

    #[test]
    fn the_model_the_band_shows_is_the_one_the_runtime_applied_and_never_a_prediction() {
        // The **catalog-membership** refusal lives where the catalog is
        // (`provider::model::ModelSelector::apply`, on the runtime thread), so
        // this side cannot know whether an id will be taken. A band that
        // adopted the id it submitted would put a model the provider does not
        // publish on the hint row, report it to a bare `/model`, and disagree
        // with the model the next turn is actually held in.
        let mut shell = shell(24, 80);
        let in_force = shell.shell.model.clone();

        shell.route_bytes(b"/model second-model\r");

        assert_eq!(
            shell.picks_up(),
            TurnWork::Model("second-model".to_string()),
            "the model change never reached the thread that owns the rule"
        );
        assert_eq!(
            shell.shell.model, in_force,
            "the band adopted a model nothing has accepted yet"
        );
        let document = shell.document();
        assert!(
            !document.iter().any(|row| row.contains("second-model")),
            "the band reported a model change the runtime has not made: {document:?}"
        );

        // The answer is what moves it, and the answer carries the model in
        // force -- so a refusal leaves the band showing the model the session
        // is really in rather than the one it asked for.
        shell.apply(UiEvent::ModelAnswered {
            model: in_force.clone(),
            outcome: ModelAnswer::Refused {
                reason: "gateway does not publish second-model in its catalog".to_string(),
            },
        });
        assert_eq!(shell.shell.model, in_force);
        assert_eq!(
            shell.document(),
            said(notice::Notice::error(
                "model",
                "gateway does not publish second-model in its catalog"
            )),
            "the refusal is not the line shell's words"
        );

        // And a selection moves it, with the caveat both surfaces say when the
        // catalog could not confirm it.
        shell.apply(UiEvent::ModelAnswered {
            model: "second-model".to_string(),
            outcome: ModelAnswer::Applied {
                unverified: Some("nothing answered".to_string()),
            },
        });
        assert_eq!(shell.shell.model, "second-model");
        // Joined, because the document wraps to the screen: what is being
        // pinned is the two lines' text, not where an 80-column band broke the
        // second one.
        // Upstream's announcement (`announceModelSelection`), no topic.
        let document = shell.document();
        assert_eq!(
            document.first().map(String::as_str),
            Some("* Switched to second-model"),
            "{document:?}"
        );
        // Joined as the words they are, because the document wraps to the
        // screen and the caveat is longer than 80 columns.
        let document = joined(&document);
        // The literal, not the function that produces it: a test that asked
        // `unverified_notice` what it says would agree with any answer it gave.
        assert!(
            document.contains(
                "! model: nothing answered, so this model was not checked against \
                 the provider's catalog"
            ),
            "the caveat is not the sentence the line shell gives: {document:?}"
        );
        // And it really is the sentence both front ends say, which is the half
        // a literal alone cannot pin.
        assert!(
            document.contains(unattributed(&crate::provider::model::unverified_notice(
                "nothing answered"
            ))),
            "the two front ends say an unchecked selection differently: {document:?}"
        );
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

    #[test]
    fn new_reaches_the_runtime_because_the_conversation_lives_there() {
        let mut shell = shell(24, 80);
        shell.route_bytes(b"/new\r");
        assert_eq!(shell.picks_up(), TurnWork::New);
    }

    #[test]
    fn clear_forgets_the_rows_it_is_about_to_erase_and_owes_the_erase_itself() {
        // Three things, and none of them is optional: the bytes, the
        // transcript's memory of what is on the screen, and the appends already
        // owed against a screen that is about to stop existing.
        let mut shell = shell(24, 80);
        shell.apply(UiEvent::Delta(
            "an answer nobody will see again".to_string(),
        ));
        shell.shell.flush_paced();
        assert!(
            shell.shell.owes_document(),
            "nothing was owed to begin with"
        );
        // And a second answer still in the pacer, which is the fourth thing a
        // clear has to forget: text released onto the blank screen afterwards
        // would be the tail of an answer the user asked to have erased, landing
        // under a notice that says the screen was cleared.
        shell.apply(UiEvent::Delta("and one still being released".to_string()));

        shell.route_bytes(b"/clear\r");
        assert_eq!(
            shell.shell.paced_backlog(),
            0,
            "the stream survived the screen it was measured against"
        );

        assert!(shell.take_clearing(), "the screen was never asked to clear");
        assert!(
            !shell.take_clearing(),
            "the clear was owed twice, so it would be written twice"
        );
        let document = shell.document();
        // A notice at the top of a blank screen: no blank row above it -- a
        // fresh transcript has nothing to put one under -- and its own below.
        assert_eq!(
            document,
            said(notice::Notice::neutral("screen", CLEARED_NOTICE)),
            "a row measured against the erased screen survived the clear"
        );

        // And the transcript's own memory: the line it had open is gone, so the
        // next delta opens a **new** row on a blank screen -- an append that
        // still believed the old row was up there would rewrite it in place and
        // put the next answer on the end of one nobody can see.
        shell.apply(UiEvent::Delta("the next answer".to_string()));
        shell.shell.flush_paced();
        assert_eq!(
            shell.take_pending(),
            vec![
                Append {
                    scroll: 1,
                    // The answer as it is handed over, spelled out: this case
                    // reads the rows as they are handed over rather than
                    // through [`Fixture::document`] -- in its two-column
                    // gutter, with no palette colour of xfx's on it.
                    rows: vec!["  the next answer".to_string()]
                },
                // and the blank row its block ends on
                Append {
                    scroll: 1,
                    rows: vec![String::new()]
                },
            ],
            "the first delta after a clear was written onto a row the clear erased"
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
        // `fx-approvals-cc-40x120-after-esc`: the call the panel was asking
        // about never runs, so the row says `Cancelled`, the header counts it,
        // and the call gets its own line under the group -- at the keystroke,
        // above the sentence that answers it.
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
            written[5..]
                .iter()
                .any(|row| row.starts_with(INTERRUPTED_ROW)),
            "{written:#?}"
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
    fn stopping_the_turn_while_a_command_runs_leaves_its_row_to_its_result() {
        // A running executor holds the runtime's thread until it returns, so
        // the stop is read after the command has finished: a `Cancelled` row at
        // the keystroke would claim a stop the command never had. The block
        // keeps saying `Running` above the sentence that answers the key, and
        // the command's real result is drawn under it when it comes.
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
            !written.iter().any(|row| row.contains("Cancelled")),
            "a stop the command never had was drawn: {written:#?}"
        );
        let notice = written
            .iter()
            .position(|row| row.starts_with(INTERRUPTED_ROW))
            .expect("the interrupt notice");
        assert_eq!(
            written[notice + 2..notice + 5],
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
    fn a_line_written_while_a_call_runs_closes_the_group_and_its_result_draws_it_again() {
        // The document is append-only past its last block: a row written
        // under a running call's group leaves that block as it stood, and the
        // call's result is drawn under the row rather than above it.
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
                "\u{2514} Running make",
                "",
                "* something worth saying",
                "",
                "\u{25cf} 1 tool call \u{b7} 1 command",
                "\u{2514} Ran make",
                "",
            ]
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
        shell.apply(UiEvent::Delta("half a sentence".to_string()));
        shell.apply(UiEvent::Notice(notice::Notice::warning(
            "tool",
            "read_file ok",
        )));

        // The answer's block ends on its blank row, which is the notice's
        // blank row above as well.
        assert_eq!(
            shell.released(),
            vec!["  half a sentence", "", "! tool: read_file ok", ""],
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
        shell.apply(UiEvent::Delta("the first half ".to_string()));
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
            ["  the first half ", ""],
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
        assert_eq!(
            with_argument.geometry.panel, 0,
            "the menu outlived the word it was completing"
        );

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
            partial.picks_up(),
            TurnWork::New,
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
            moved.picks_up(),
            TurnWork::New,
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
            visible_text(&rows[0]).starts_with("Commands 7  [All]  General  Session"),
            "{rows:?}"
        );
        assert_eq!(rows[1], "", "the blank row under the header");
        assert_eq!(
            rows[2],
            format!(
                "  {bright}/help{reset}       {bright}show available slash commands{reset}",
                bright = PALETTE.subtitle(),
                reset = PALETTE.reset()
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
            ["/help", "/clear", "/version", "/quit", "/new", "/setup", "/model"]
                .map(|name| Some(name.to_string()))
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
        // `/quit` is the fourth row of `All`: three Downs and a Return.
        let mut shell = with_catalog(40, 120);
        shell.route_bytes(b"\x1b[B\x1b[B\x1b[B");
        assert!(
            menu(&shell)[5].starts_with(&format!("  {}/quit", PALETTE.subtitle())),
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
        // Account's one command is `/setup`.
        let mut shell = with_catalog(40, 120);
        shell.route_bytes(b"\t\t\t");
        assert!(visible_text(&menu(&shell)[2]).starts_with("  /setup"));
        shell.route_bytes(b"\r");
        assert!(shell.help_menu.is_none());
        assert_eq!(shell.editor.text(), "/setup ");
        assert!(shell.picker.is_none(), "a word with a space opened a menu");
        assert_eq!(shell.geometry.panel, 0);
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
        assert!(
            rows[8].starts_with(&format!("  {}/model", PALETTE.subtitle())),
            "Up from the first row is the last: {rows:?}"
        );
        shell.route_bytes(b"\x1b[B");
        assert!(menu(&shell)[2].starts_with(&format!("  {}/help", PALETTE.subtitle())));
        // General is four commands, and the band gives the rows back.
        shell.route_bytes(b"\t");
        assert_eq!(shell.geometry.panel, 2 + 4);
        assert_eq!(menu(&shell).len(), 6);
        // Shift-Tab, as a terminal spells it, is back to `All`.
        shell.route_bytes(b"\x1b[Z");
        assert_eq!(shell.geometry.panel, 2 + 7);
        assert!(visible_text(&menu(&shell)[0]).starts_with("Commands 7  [All]"));
    }

    #[test]
    fn escape_closes_the_catalog_and_its_search_without_arming_the_clear() {
        let mut shell = with_catalog(40, 120);
        shell.route_bytes(b"ses");
        assert_eq!(menu(&shell).len(), 2 + 2, "/clear and /new");
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
        assert!(visible_text(&menu(&shell)[0]).starts_with("Commands 7"));
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
        assert!(
            rows[4].starts_with(&format!("  {}/new", PALETTE.subtitle())),
            "{rows:?}"
        );
        assert!(visible_text(&rows[2]).starts_with("  /version"), "{rows:?}");
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
    fn a_setup_argument_that_names_no_provider_never_reaches_the_runtime() {
        // Refused here rather than sent, for `/model`'s reason: the far side of
        // this one writes a file, and a queue place spent on a word that cannot
        // be a provider is a place a real prompt could have had.
        let mut shell = shell(24, 80);
        for argument in ["", "nonesuch", "  ", "llmux extra"] {
            shell.route_bytes(format!("/setup {argument}\r").as_bytes());
            assert!(
                shell.sent.try_recv().is_err(),
                "`/setup {argument}` reached the runtime"
            );
        }
        let document = shell.document();
        assert!(
            document
                .iter()
                .any(|row| row.contains("gateway") && row.contains("llmux")),
            "the refusal does not name the providers this build can set up: {document:?}"
        );
        // xfx's own words: the refusal quotes nothing of the argument, which
        // is what keeps a hostile one out of the document -- and the submitted
        // line is not echoed either, so nothing of it is there at all.
        let refusals: Vec<&String> = document
            .iter()
            .filter(|row| row.starts_with("\u{2717} /setup takes a provider"))
            .collect();
        assert_eq!(refusals.len(), 4, "{document:?}");
        assert!(
            !document.iter().any(|row| row.contains("nonesuch")),
            "the refusal quoted the argument back: {document:?}"
        );
    }

    #[test]
    fn a_selected_provider_replaces_the_model_the_credential_fact_and_the_catalog() {
        // Everything the previous provider's session knew that is not this
        // one's. A catalog left standing would put another daemon's context
        // window under this daemon's model, and a stale credential fact would
        // go on telling a keyless local session to run `xfx setup`.
        let mut shell = shell(24, 80);
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
        assert!(
            document.iter().any(|row| row
                == "* provider: llmux model=fable; the next prompt starts a fresh conversation"),
            "{document:?}"
        );
    }

    #[test]
    fn the_catalog_browser_renders_a_context_and_an_effort_column_per_row() {
        let mut shell = shell(24, 80);
        shell.shell.apply(UiEvent::CatalogLoaded {
            provider: ProviderId::Llmux,
            entries: vec![
                entry("fable", Some(1_000_000), &["low", "high"]),
                entry("plain", None, &[]),
            ],
        });
        // One notice, a row per model under its head.
        let document = shell.document();
        assert_eq!(
            document,
            vec![
                "* catalog: 2 shown",
                "  fable context=1000000 efforts=low,high",
                // A provider really may publish neither, and an empty column
                // would read as a rendering fault rather than as an absent
                // fact.
                "  plain context=unknown efforts=none",
                "",
            ],
            "{document:?}"
        );
    }

    #[test]
    fn usage_and_max_context_drive_one_context_meter() {
        // Both halves or no meter. Absent is not zero: a provider that publishes
        // no usage, and a provider that publishes no window, each leave the
        // segment off the row rather than putting a nought on it.
        let mut shell = shell(24, 80);
        shell.shell.model = "fable".to_string();

        assert_eq!(shell.shell.context_meter(), None, "nothing measured yet");

        // A denominator with no numerator is not a meter.
        shell.shell.apply(UiEvent::CatalogLoaded {
            provider: ProviderId::Llmux,
            entries: vec![entry("fable", Some(200_000), &[])],
        });
        assert_eq!(shell.shell.context_meter(), None);
        assert!(!shell.hint().contains('%'), "{}", shell.hint());

        // A numerator with no denominator is not a meter either.
        let mut bare = fixture_with_no_catalog();
        bare.shell.model = "fable".to_string();
        bare.shell.apply(UiEvent::Usage {
            input: Some(12_345),
            output: Some(9),
        });
        assert_eq!(bare.shell.context_meter(), None);
        assert!(!bare.hint().contains('%'), "{}", bare.hint());

        // Both, and exactly one segment.
        shell.shell.apply(UiEvent::Usage {
            input: Some(12_345),
            output: Some(9),
        });
        assert_eq!(shell.shell.context_meter(), Some((12_345, 200_000)));
        let row = shell.hint();
        assert!(row.contains("12k/200k 6%"), "{row}");
        assert_eq!(row.matches('%').count(), 1, "{row}");
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
            Some((30_000, 100_000)),
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
        assert_eq!(shell.shell.context_meter(), Some((4_000, 8_000)));
        shell.shell.model = "vendor/large".to_string();
        assert_eq!(
            shell.shell.context_meter(),
            Some((4_000, 900_000)),
            "the id spelling did not select the same row its alias does"
        );
        shell.shell.model = "absent-from-this-catalog".to_string();
        assert_eq!(
            shell.shell.context_meter(),
            None,
            "a model the catalog does not publish was given a window anyway"
        );
    }
    #[test]
    fn a_new_session_clears_the_meter_it_measured_and_keeps_the_window() {
        // `/new` drops the conversation on the runtime thread
        // (`super::worker`'s `TurnWork::New` arm), and the meter's numerator is
        // a measurement **of that conversation**. Left standing it would report
        // the old conversation's tokens against the fresh one -- a number the
        // provider never said about the session on the screen, which is the one
        // thing this row must never do.
        //
        // The **denominator stays**. It is the model's context window, and
        // `/new` changes neither the provider nor the model: clearing it would
        // throw away a fact that is still true and cost a socket to learn again.
        let mut shell = shell(24, 80);
        shell.shell.model = "fable".to_string();
        shell.shell.apply(UiEvent::CatalogLoaded {
            provider: ProviderId::Llmux,
            entries: vec![entry("fable", Some(200_000), &[])],
        });
        shell.shell.apply(UiEvent::Usage {
            input: Some(12_345),
            output: Some(9),
        });
        assert_eq!(shell.shell.context_meter(), Some((12_345, 200_000)));
        assert!(shell.hint().contains("12k/200k"), "{}", shell.hint());

        shell.route_bytes(b"/new\r");
        assert_eq!(
            shell.picks_up(),
            TurnWork::New,
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
        shell.shell.apply(UiEvent::Usage {
            input: Some(7),
            output: None,
        });
        assert_eq!(shell.shell.context_meter(), Some((7, 200_000)));
    }

    #[test]
    fn a_new_session_the_runtime_refused_keeps_the_conversation_and_its_meter() {
        // The other side of the same ordering `send` has: nothing is thrown
        // away for a submission the runtime would not take. A refused `/new`
        // means the conversation is still there, so its measurement is still
        // about the session on the screen and clearing it would be a lie in the
        // opposite direction.
        let mut shell = shell(24, 80);
        shell.shell.model = "fable".to_string();
        shell.shell.apply(UiEvent::CatalogLoaded {
            provider: ProviderId::Llmux,
            entries: vec![entry("fable", Some(200_000), &[])],
        });
        shell.shell.apply(UiEvent::Usage {
            input: Some(12_345),
            output: None,
        });

        // Fill the queue: one in flight and one waiting is everything the
        // session holds (`super::worker::WORK_LIMIT`).
        shell.route_bytes(b"first\r");
        let _first = shell.picks_up();
        shell.route_bytes(b"second\r");
        let _ = shell.document();

        shell.route_bytes(b"/new\r");
        assert_eq!(
            shell.shell.notice,
            Some(QUEUE_REJECTED),
            "the refused /new did not say so on the hint row"
        );
        assert_eq!(
            shell.shell.context_meter(),
            Some((12_345, 200_000)),
            "a /new the runtime refused cleared the meter of a conversation that is still there"
        );
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
        unwrapped(&shell.shell.band_rows())
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
                    shell.marked().contains("2. Yes, and"),
                    "open={open} {down:?}: the marker did not move: {:?}",
                    shell.marked()
                );
                shell.decide(Input::Action(up), Instant::now());
                assert!(
                    shell.marked().contains("1. Yes"),
                    "open={open} {up:?}: the marker did not come back: {:?}",
                    shell.marked()
                );
                // And nothing of either key reached the draft: what was typed
                // is still exactly what was typed.
                if open {
                    assert!(
                        band(&shell).contains(" y "),
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
    fn escape_at_a_filled_deny_draft_refuses_without_the_sentence_but_enter_sends_it() {
        // The pair, on the wire, in one case so it cannot drift. Same draft
        // text, two exits.
        let mut escaped = shell(24, 80);
        a_filled_deny_draft(&mut escaped, "never touch the fixtures");
        escaped.decide(Input::Action(Action::Escape), Instant::now());
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
    fn ctrl_c_at_a_draft_stops_the_turn_and_sends_no_feedback() {
        // One message, both meanings, and the sentence in neither. The prompter
        // turns the cancellation into the refusal on the far side
        // (`super::super::approval::TuiPrompter`), so an `Answer` sent here as
        // well would be the only thing the runtime heard -- and the turn would
        // go on running after the user asked everything to stop.
        let mut shell = shell(24, 80);
        a_filled_deny_draft(&mut shell, "stop, this is wrong");
        shell.decide(Input::Action(Action::Cancel), Instant::now());

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
        assert_eq!(shell.screen_owner(), ScreenOwner::Approval);

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
        assert_eq!(shell.screen_owner(), ScreenOwner::Approval);

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
        for damage in [b"/clear\r".to_vec(), vec![0x0c]] {
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
            shell.damage_told();
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
        let rows = shell.band_rows();
        assert!(
            rows.last()
                .expect("a hint row")
                .starts_with(LIGHT.statusline()),
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
        assert_eq!(shell.screen_owner(), ScreenOwner::Approval);
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
    fn a_notice_inside_an_answer_keeps_its_own_colour_and_its_own_place() {
        // Ordering and colour together: the notice belongs at the point the
        // answer had reached when it happened (`Shell::mark`), and the answer
        // that follows it is the answer again rather than more of the notice.
        let mut shell = shell(24, 80);
        shell.apply(UiEvent::Delta("before".to_string()));
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
        shell.apply(UiEvent::Delta("held while it was dark".to_string()));
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
}
