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
use super::approval_screen::ApprovalScreen;
use super::bridge::{TurnControl, TurnWork, UiEvent};
use super::editor::{self, Editor};
use super::entity::{Entities, EntityKind};
use super::gesture::{Escape, Gestures, Interrupt, INTERRUPTED_EXIT_CODE};
use super::hint::{self, Hint, Notice};
use super::history::{History, HistoryEntry, HistoryStep};
use super::input::{Action, Decoder, Input};
use super::layout::{self, Geometry};
use super::pacer::Pacer;
use super::paste::{self, Paste, Pasted, Refusal};
use super::picker::{self, Dismissed, Picker, PickerAction, PickerOutcome, Trigger};
use super::render_request::{Reason, RenderRequest};
use super::router::{self, CommandHandlers};
use super::theme::Palette;
use super::transaction::{EditTransaction, LastTransaction};
use super::transcript::{Append, Transcript};
use super::worker::{Rejected, WorkHandle};
use crate::config::{PermissionMode, RuntimeConfig};
use crate::interactive::{self, Submitted};
use crate::output::safe_one_line;
use crate::permission::{ApprovalAnswer, ApprovalRequest};
use crate::provider::model::model_id_problem;
use crate::provider::model::CatalogEntry;
use crate::provider::ProviderId;

/// The divider's rule, one cell wide, repeated across the screen.
const RULE: char = '\u{2500}';

/// What the composer puts in front of what is typed.
const PROMPT: &str = "> ";

/// What its continuation rows are written into instead: the same two cells,
/// blank, so a wrapped row starts under the row above it rather than two
/// columns to the left of it.
const GUTTER: &str = "  ";

/// How many cells [`PROMPT`] occupies, and therefore where an empty composer's
/// caret sits.
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
const QUEUE_DROPPED: &str = "xfx: dropping what was queued behind it as well.";

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
const PANEL_TOO_SMALL: &str =
    "xfx: this screen is too small to ask for permission, so the change was refused";

/// What the user is told when the runtime thread is not there to take work.
///
/// Told apart from [`QUEUE_REJECTED`] on purpose: a runtime that is gone is not
/// a runtime with a queue, and a fatal event is already on its way to say so.
/// In the document rather than on the hint row, because unlike a full queue it
/// is not a condition that clears.
const GONE_NOTICE: &str = "xfx: the runtime is gone; that line was not sent";

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

/// What `/clear` leaves behind after it has erased the screen.
///
/// The line-oriented shell prints its banner and a `kept` line here
/// (`interactive.rs:451-456`); the TUI has no banner, and the identity that
/// banner carries belongs on the hint row. What is left is the promise the
/// command's own summary makes -- that clearing the screen is not clearing the
/// conversation -- said where a user who just watched their transcript vanish
/// is looking.
const CLEARED_NOTICE: &str = "xfx: cleared the screen; the conversation is kept";

/// What `/new` says, in the words the line-oriented shell says them in
/// (`interactive.rs:461-463`).
const NEW_SESSION_NOTICE: &str = "[shell] new session; the next prompt starts a fresh conversation";

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
    /// Settled once, at launch, from the terminal xfx was started in
    /// ([`super::theme`]) and never re-asked: following a background that
    /// changes mid-session is Phase 3. Held here rather than consulted per
    /// frame for the reason [`Self::model`] is -- one field, not a borrow of
    /// the launch.
    palette: Palette,
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
    /// What the last edit did, in the terms an undo would need.
    ///
    /// Overwritten by every edit ([`Self::edited`]) and read by nothing on a
    /// Phase-2 path: it is the seam item 18 builds its stack on, and what it is
    /// here to fix now is the boundary only this moment knows -- one framed
    /// paste is one transaction, whatever it weighed
    /// ([`super::transaction`]).
    transaction: LastTransaction,
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
    /// The answer text that has not ended its line yet, and the rows it has put
    /// on the screen.
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
    /// The document writes this session owes, oldest first.
    ///
    /// A `Vec` rather than one `Append`, because two pushes can land between
    /// two frames and their scrolls do not merge: the second one's rows are
    /// measured against a screen the first one already moved.
    pending: Vec<Append>,
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

/// One of xfx's own document writes, waiting for its place in the stream.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Mark {
    /// A whole line, on rows of its own.
    Line(String),
    /// The end of the answer's line, and nothing else. What a turn that ended
    /// without a failure owes: the next thing written starts on a new row.
    EndOfLine,
}

/// What the band's elastic slot is holding.
///
/// A borrow rather than a state: which of the two it is follows from the fields
/// ([`Shell::slot`]), so there is no third answer for a flag to drift into.
#[derive(Debug, Clone, Copy)]
enum Slot<'a> {
    /// A decision the turn is waiting on. It owns the focus.
    Question(&'a Panel),
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
        work: WorkHandle,
    ) -> Self {
        Self {
            geometry,
            screen: (geometry.rows, geometry.cols),
            palette,
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
            transaction: LastTransaction::new(),
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
            pending: Vec::new(),
            work,
            gestures: Gestures::default(),
            notice: None,
            escape_armed: false,
            queued: 0,
            activity: Activity::new(),
            phase: 0,
            panel: None,
            alternate: None,
            owner: ScreenOwner::Primary,
            picker: None,
            dismissed: Dismissed::default(),
            activity_row: None,
            clearing: false,
            fatal: None,
            leaving: None,
        }
    }

    /// The band's rows, top first, starting at the band's own top row -- the
    /// activity row while a turn is running, and the divider otherwise.
    ///
    /// Exactly as many rows as the band owns: the writer places them by
    /// counting down from the divider, so a row missing here would shift every
    /// row below it up by one.
    pub(crate) fn band_rows(&self) -> Vec<String> {
        let mut rows = Vec::with_capacity(usize::from(self.geometry.band_rows()));
        // What the turn is doing, above the rule, and only while the geometry
        // says the band owns that row: the row's presence and its text are one
        // fact settled together ([`Self::tick_activity`]), and a band that
        // painted a row the geometry did not give it would push its hint row
        // off the bottom of the screen.
        if self.geometry.activity.is_some() {
            rows.push(self.activity_row.clone().unwrap_or_default());
        }
        // The question -- or, when there is no question, the completion menu --
        // in the rows the geometry gave the slot and only while it gave them:
        // the block's height and the band's are one fact settled together
        // ([`Self::refit`]), so rows painted without the geometry's agreement
        // would push the hint row off the bottom of the screen. The order is
        // the one [`Self::fit`] solved with, and it is the whole of "they are
        // mutually exclusive": a question outranks a menu.
        if self.geometry.panel > 0 {
            match self.slot() {
                Some(Slot::Question(panel)) => {
                    rows.extend(panel.rows(self.geometry.cols, self.geometry.rows));
                }
                Some(Slot::Menu(picker)) => {
                    rows.extend(picker.rows(self.geometry.cols, self.geometry.rows));
                }
                None => {}
            }
        }
        rows.push(self.painted(
            self.palette.divider(),
            std::iter::repeat_n(RULE, usize::from(self.geometry.cols)).collect(),
        ));
        // The composer's own rows, as many of them as the window shows, each in
        // the gutter the marker owns.
        let composer = self.editor.rows(self.text_cols());
        for index in self.window(composer.len()) {
            let marker = if index == 0 { PROMPT } else { GUTTER };
            rows.push(format!("{marker}{}", composer[index]));
        }
        // A composer shorter than the band it is in. The remaining rows are the
        // band's, so they are written -- blank -- rather than left out: a row
        // the frame does not place is a row the last frame's text stays on.
        // Counted against the band's own height rather than against the
        // composer's, because the rows above the rule are the band's too.
        while rows.len() + 1 < usize::from(self.geometry.band_rows()) {
            rows.push(String::new());
        }
        // The hint row: what a turn will be run with, in upstream's order and
        // budgeted to the screen ([`super::hint`]).
        rows.push(self.painted(self.palette.hint(), self.hint_row()));
        rows
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
            resume: self.palette.hint(),
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
        if let Some(panel) = self.panel.as_ref() {
            return Some(Slot::Question(panel));
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
                return (
                    self.geometry
                        .panel_first()
                        .saturating_add(panel.caret_row(self.geometry.rows)),
                    0,
                );
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

    /// Adds answer text to the transcript.
    ///
    /// Nothing is written here. What the text costs the terminal is queued and
    /// a frame is asked for, because the append and the frame that follows it
    /// are one turn's worth of work and the loop is the only thing that owns
    /// the screen.
    // The composer's submit is the first caller -- it echoes the line the user
    // sent so the loop is visibly closed -- and Task 12's deltas are the next.
    pub(crate) fn write_transcript(&mut self, text: &str) {
        let append = self.transcript.push(text);
        self.owe(append);
    }

    /// Ends the transcript's current line, leaving it in the document.
    // The composer's submit is the first caller, and Task 12's end-of-turn is
    // the next: a turn ends whether or not the last delta carried a newline.
    pub(crate) fn end_transcript_line(&mut self) {
        let append = self.transcript.end_line();
        self.owe(append);
    }

    /// Records a document write, unless it is one that would write nothing.
    ///
    /// The guard is not tidiness: an append that scrolls nothing and writes no
    /// rows would still cost the loop a frame, and a frame the band did not
    /// need is a repaint of the whole band on a link that may be a serial line.
    fn owe(&mut self, append: Append) {
        if append.scroll == 0 && append.rows.is_empty() {
            return;
        }
        self.pending.push(append);
        // An append scrolls the screen out from under the band, so the frame
        // that follows it is not optional.
        self.render.request(Reason::Transcript);
    }

    /// Takes the document writes this session owes, oldest first.
    pub(crate) fn take_pending(&mut self) -> Vec<Append> {
        std::mem::take(&mut self.pending)
    }

    /// Gives back writes the loop took and did not attempt.
    ///
    /// [`Self::take_pending`] hands over **everything** owed, and a loop that
    /// stops part-way through the batch -- a refused write ends the tick -- is
    /// holding rows that moved no bytes at all. They are still owed: Phase 1
    /// never repaints a document row, so a batch dropped on the floor here is
    /// gone from the session for good.
    ///
    /// **Ahead of anything owed since**, which is where they were. The document
    /// is a sequence, and a row that lands after one queued behind it is as
    /// wrong as a row that never lands.
    pub(crate) fn restore_pending(&mut self, untried: Vec<Append>) {
        if untried.is_empty() {
            return;
        }
        let queued_since = std::mem::replace(&mut self.pending, untried);
        self.pending.extend(queued_since);
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
            UiEvent::TurnStarted => self.activity.set(Work::Thinking),
            // The answer, as it arrives -- into the pacer rather than into the
            // document, so a provider that sends a kilobyte in one frame and
            // nothing in the next is still read at one speed.
            UiEvent::Delta(text) => self.stream(&text),
            // The same two sentences `xfx ask --tool-notices` puts on the
            // diagnostic stream (`output.rs:1154-1174`), so a tool means the
            // same thing on both surfaces.
            UiEvent::ToolStart { tool, .. } => {
                // And on the band, where the row above the divider stops saying
                // `Thinking` and names what is running instead: a turn that has
                // gone quiet because a tool is taking a minute looks exactly
                // like a turn that has gone quiet, unless it says so.
                self.activity.set(Work::Tool { name: tool.clone() });
                self.say(format!("[tool] {tool} running"));
            }
            UiEvent::ToolResult {
                tool, ok, detail, ..
            } => {
                let line = if ok {
                    format!("[tool] {tool} ok")
                } else {
                    format!(
                        "[tool] {tool} refused: {}",
                        safe_one_line(&detail, TOOL_DETAIL_BYTES)
                    )
                };
                // The tool is over, so the model has the turn again. The
                // clock is not restarted with it: what the row measures is the
                // turn, and a tool call is part of one.
                self.activity.set(Work::Thinking);
                self.say(line);
            }
            UiEvent::Notice(text) => self.say(text),
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
                self.say(format!(
                    "[shell] provider={} model={}; the next prompt starts a fresh conversation",
                    provider.label(),
                    self.model
                ));
                self.render.request(Reason::Footer);
            }
            // The browser. Rendered as document rows rather than into the band's
            // elastic slot: a catalog is a list the user reads and scrolls back
            // to, and the slot is for the two things a keystroke is *about*.
            UiEvent::CatalogLoaded { provider, entries } => {
                self.catalog = entries;
                self.provider = provider;
                self.say(format!("[shell] catalog={} shown", self.catalog.len()));
                for line in self.catalog_rows() {
                    self.say(line);
                }
                // The denominator may have arrived with it.
                self.render.request(Reason::Footer);
            }
            // The numerator, and only the numerator: see [`Self::context_used`]
            // for why the output half is deliberately dropped here rather than
            // added to it.
            UiEvent::Usage { input, .. } => {
                self.context_used = input;
                self.render.request(Reason::Footer);
            }
            // The turn has stopped and is waiting for a person. Everything
            // about that -- the panel, the rows it costs the document, the
            // focus, and the clock that stops while it is up -- follows from
            // this one field being `Some`.
            UiEvent::Approval(request) => self.ask(request),
            UiEvent::TurnEnded { failure } => {
                // Behind whatever of this turn's answer is still in the pacer:
                // a conclusion that overtook the text it concludes would land
                // in the middle of the answer.
                match failure {
                    Some(failure) => self.say(failure),
                    None => self.mark(Mark::EndOfLine),
                }
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
                    "[shell]   {} context={context} efforts={efforts}",
                    entry.preferred_name()
                )
            })
            .collect()
    }

    /// Puts a question in front of the user, or refuses it on their behalf.
    ///
    /// The refusal is not a fallback to be tidied up later: a panel that did
    /// not fit would be painted with its choices below the last row of the
    /// screen, and the session would sit waiting for a keystroke about a
    /// question the user cannot read. `ask` mode's own rule decides it -- a
    /// decision xfx was never given is a refusal.
    fn ask(&mut self, request: ApprovalRequest) {
        // Which plane the question belongs on, settled from the change itself
        // before anything is installed ([`approval::ApprovalSurface`]).
        let surface = approval::ApprovalSurface::for_request(&request);
        let panel = Panel::new(request);
        let rows = panel.height(self.geometry.cols, self.geometry.rows);
        if !layout::fits_panel(self.geometry.rows, self.geometry.cols, rows) {
            // **Before the owner moves.** A question that was never asked has
            // no screen to give back, and a session left owning a plane with
            // nothing on it would compose its next frame for nobody.
            self.say(PANEL_TOO_SMALL.to_string());
            self.work.control(TurnControl::Answer(ApprovalAnswer::Deny));
            return;
        }
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
        match surface {
            approval::ApprovalSurface::Inline => {
                self.panel = Some(panel);
                self.owner = ScreenOwner::Primary;
            }
            // The band keeps none of its rows for this one: the question is
            // painted on the other plane, and the band underneath stays exactly
            // the band the restore repaints when the answer gives it back.
            approval::ApprovalSurface::Alternate => {
                self.alternate = Some(ApprovalScreen::new(panel.into_request()));
                self.owner = ScreenOwner::Approval;
            }
        }
        // The band just grew by the panel's rows, so the divider, the composer
        // and the caret are all somewhere else.
        self.refit();
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
        self.owner = ScreenOwner::Primary;
    }

    /// Offers one keystroke to the completion menu, and says whether it was
    /// taken.
    ///
    /// **Before the editor and before the gestures**, which is the whole of
    /// what "the menu binds a key" means: an Up while a menu is open moves the
    /// mark rather than the caret, and an Escape closes the menu rather than
    /// arming the gesture that clears the composer.
    ///
    /// Enter is the one that is answered `false` on purpose. It closes the menu
    /// -- the line is being run, and a menu about a draft that is on its way to
    /// the runtime is about nothing -- and then goes on to mean what it means
    /// everywhere else. A menu that swallowed it would make every command take
    /// two Returns.
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
            PickerOutcome::Dismiss => {
                self.dismiss_picker();
                matches!(request, PickerAction::Escape)
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
    fn complete(&mut self, name: &'static str) {
        self.take_draft();
        // Refused only by the byte budget, and a command name is nine bytes at
        // its longest against a cap of eight mebibytes ([`editor::Editor`]) --
        // into a composer this call has just emptied.
        let _ = self.editor.insert(&picker::completed(name));
        // The menu has done what it was for. Dismissed rather than closed,
        // because the text it just wrote is still a slash word: a menu that
        // re-opened on its own completion would be one the user cannot leave by
        // taking something from it.
        self.dismissed.dismiss(Trigger::Slash);
        self.amended();
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
        let action = match event {
            Input::Text(character) => approval::Action::Text(character),
            Input::Action(Action::Up) => approval::Action::Up,
            Input::Action(Action::Down) => approval::Action::Down,
            Input::Action(Action::Tab) => approval::Action::Tab,
            Input::Action(Action::Submit) => approval::Action::Submit,
            Input::Action(Action::Escape) => approval::Action::Escape,
            Input::Action(Action::Cancel) => approval::Action::Cancel,
            // The whole band is repainted every frame, so a redraw is a frame
            // here as much as anywhere else.
            Input::Action(Action::Redraw) => {
                self.render.request(Reason::ExternalDamage);
                return;
            }
            Input::Action(_) | Input::PasteByte(_) => return,
        };
        // Whichever surface is holding the question, and there is never more
        // than one ([`Self::ask`]). Both answer with the same function
        // (`super::approval::answered`), so which plane a change happened to be
        // large enough for cannot change what a key means.
        let answered = match (self.panel.as_mut(), self.alternate.as_mut()) {
            (Some(panel), _) => panel.apply(action),
            (_, Some(screen)) => screen.apply(action),
            (None, None) => return,
        };
        let Some(answer) = answered else {
            // The marker moved, which is a frame and nothing else.
            self.render.request(Reason::Modal);
            return;
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
        self.owner = ScreenOwner::Primary;
        match action {
            // One message, both meanings. `Deny` is what the prompter answers a
            // cancellation with on the far side, so sending `Answer(Deny)` here
            // as well would be the *only* thing the runtime heard -- and the
            // turn this question belongs to, and whatever was queued behind it,
            // would go on running after the user asked everything to stop.
            approval::Action::Cancel => self.interrupt(now),
            // Esc and the rest are an answer about *this call* and nothing
            // more: the turn goes on, and is told no.
            _ => self.work.control(TurnControl::Answer(answer)),
        }
        // The band gives the panel's rows back to the document. The clock
        // starts again on the next settle rather than here, for the reason
        // every other timed answer on this surface is settled there: what is
        // painted has to be the same reading that asked for the frame
        // ([`Self::tick_activity`]).
        self.refit();
        self.render.request(Reason::Modal);
    }

    /// Adds answer text to the stream, where the pacer releases it.
    fn stream(&mut self, text: &str) {
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
        self.mark(Mark::Line(line));
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
            self.write_transcript(&text);
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
        match mark {
            Mark::Line(line) => self.write_document_line(&line),
            Mark::EndOfLine => self.finish_document_line(),
        }
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
    fn write_document_line(&mut self, line: &str) {
        self.finish_document_line();
        self.write_transcript(line);
        self.end_transcript_line();
    }

    /// Ends the document's current line, if there is one open.
    ///
    /// The guard is the difference between "end the line" and "leave a blank
    /// row": a transcript already at the start of a line has no unfinished row,
    /// and [`Transcript::end_line`] answers a second request for one with a
    /// blank row of its own -- which is right for two breaks in an answer and
    /// wrong for two notices in a row.
    fn finish_document_line(&mut self) {
        if self.transcript.tail_rows() > 0 {
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
        if self.asking() {
            self.activity.freeze(now);
        } else {
            self.activity.thaw(now);
        }
        if self.render.animate(self.activity.working(), now) {
            self.phase = (self.phase + 1) % PHASES;
        }
        let row = self.activity.row(now, self.phase, self.geometry.cols);
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

    /// Whether the screen owes a `/clear`, taken so it is written once.
    pub(crate) fn take_clearing(&mut self) -> bool {
        std::mem::take(&mut self.clearing)
    }

    /// Applies decoded events in order.
    ///
    /// `now` is the read's own clock, handed down rather than read again per
    /// event: the bytes of one read arrived together, and a Ctrl-C burst whose
    /// two bytes timed each other out would be two unrelated keystrokes.
    fn consume(&mut self, events: Vec<Input>, now: Instant) {
        for event in events {
            // The panel has the focus while it is up, and this is the whole of
            // what that means: nothing below runs, so a `1` cannot be typed
            // into the composer and a Ctrl-D cannot leave a session with a turn
            // waiting on an answer. On either plane: the focus belongs to the
            // question, not to the surface it happens to be asked on.
            if self.asking() {
                self.decide(event, now);
                continue;
            }
            // And the completion menu takes the five keys it binds, before the
            // editor and before the gestures see them. Everything else falls
            // through with the menu still up, because a menu is a view of the
            // draft and typing is what the draft is made of.
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
        if self.editor.insert(typed) {
            self.amended();
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
                if self.editor.insert(&text) {
                    self.amended();
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
                // Kept for the transaction below, before the edit that makes it
                // stale. Two copies of a draft that can be 8 MiB, held until
                // the next keystroke overwrites them, which is the price of an
                // undo that can put an insertion back
                // ([`super::transaction`]).
                let before = self.editor.text().to_string();
                // No arm for a composer that refuses the summary, and that is
                // arithmetic rather than optimism: a block is only collapsed
                // past `COLLAPSE_ABOVE` codepoints, the budget admitted the
                // draft plus that text, and the composer's own cap is the same
                // number -- so the room left over is never smaller than a name
                // ([`super::paste`]'s
                // `a_collapsed_paste_the_budget_admits_always_fits_the_composer`).
                let Some(entity) = self
                    .editor
                    .insert_entity(&summary, EntityKind::Paste { id, text, lines })
                else {
                    return;
                };
                self.amended();
                // **After the edit**, because `amended` runs through
                // [`Self::edited`], which records `Other` for every change to
                // the composer. One paste is one boundary however many bytes
                // and however many reads of the terminal it arrived in.
                self.transaction.record(EditTransaction::InsertPaste {
                    before,
                    after: self.editor.text().to_string(),
                    entity,
                });
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
                self.editor.apply(action, self.text_cols());
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
                self.editor.apply(action, self.text_cols());
                self.amended();
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
                self.editor.apply(action, cols);
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
                    // nothing about the text changed, so the undo boundary the
                    // last edit left is still the truth about the last edit
                    // ([`super::transaction`]).
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
                    self.editor.apply(Action::Delete, self.text_cols());
                    self.amended();
                }
            }
            // The whole band is repainted every frame, so a redraw is a frame.
            Action::Redraw => self.render.request(Reason::ExternalDamage),
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
            Action::Tab | Action::Ignore => {}
        }
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
                self.write_document_line(crate::app::INTERRUPT_NOTICE);
                if waiting {
                    // The second half of the same answer to the same keystroke,
                    // and immediate for the same reason: "and the queue went
                    // with it" is only useful beside the sentence it qualifies.
                    // Held back, it would arrive detached from the notice it
                    // belongs to and after text the user asked to stop.
                    self.write_document_line(QUEUE_DROPPED);
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
        self.amended();
    }

    /// What one submitted line is, and what happens to it.
    ///
    /// **Decided by [`crate::interactive::classify`] and by nothing else**, so
    /// the two surfaces cannot disagree about what a leading `/` means. That is
    /// the whole reason the routing is a call rather than a `match` of its own:
    /// a command grammar whose answer depends on which front end you typed it
    /// into is exactly the nondeterminism a command surface must not have. The
    /// names are `interactive::SLASH_COMMANDS` and nothing else: this surface
    /// answers exactly what the registry advertises.
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
        let submitted = interactive::classify(&text);
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
                self.edited();
            }
            Submitted::Command { .. } => {
                self.echo(&text);
                self.run_command(&submitted);
            }
            // A line that begins with `/` and names nothing is a mistake, not a
            // question, and it is answered with the same refusal the
            // line-oriented shell gives it (`interactive.rs:194-200`). It does
            // **not** reach the model: a typo'd command silently becoming a
            // prompt is how a user pays for a slip in tokens and in an answer
            // to a question they did not ask.
            Submitted::UnknownCommand { token } => {
                // Consumed like any other submitted line, and for the reason
                // the echo above gives: it is in the document now, and a
                // composer that kept it would have the user's next line typed
                // onto the end of it.
                let refusal = interactive::unknown_command_message(token);
                self.take_draft();
                self.edited();
                self.echo(&text);
                self.write_document_line(&refusal);
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
        match self.work.submit(TurnWork::Submit(prompt)) {
            Ok(()) => {
                // Nothing is said about the turn here, and that is the point:
                // an accepted prompt may wait behind another turn for a minute
                // (`super::worker::WORK_LIMIT`), and the band already says so
                // on its hint row. The row above the divider is about the turn
                // the runtime is *running*, so it waits for the runtime to say
                // that this one is (`UiEvent::TurnStarted`).
                self.take_draft();
                self.edited();
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
            Rejected::Gone => self.say(GONE_NOTICE.to_string()),
        }
    }

    /// Puts a submitted line into the document, where the user can see what
    /// they sent.
    ///
    /// The line ends whether or not the last thing typed was a newline: what
    /// was submitted is finished, and a tail left open would be continued by
    /// the answer.
    fn echo(&mut self, text: &str) {
        self.say(text.to_string());
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
        self.edited();
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
    fn help(&mut self) {
        for line in interactive::help_text().lines() {
            self.say(line.to_string());
        }
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
        self.say(NEW_SESSION_NOTICE.to_string());
    }

    fn clear(&mut self) {
        self.clear_screen();
    }

    fn model(&mut self, argument: &str) {
        self.use_model(argument);
    }

    fn version(&mut self) {
        self.say(interactive::version_line());
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
    /// With no argument it **reports**; with one it applies from the next turn
    /// on and is recorded in the session log, so a resumed conversation
    /// continues in the model it was actually held in -- which is
    /// [`TurnWork::Model`]'s whole job on the far side (`super::worker`'s
    /// `Runtime::use_model`).
    ///
    /// **Narrower than the line shell in one way, and it is a boundary rather
    /// than an omission**: that shell loads the provider's catalog to report
    /// with, and prints it. Reaching an endpoint is the parallel plan's, not
    /// this one's, and the load is asynchronous on a thread that must not wait
    /// for anything -- so the TUI reports the model in force and lists no
    /// catalog. `docs/parity.md` says so.
    fn use_model(&mut self, argument: &str) {
        if argument.is_empty() {
            self.say(format!(
                "[shell] model={} provider={}",
                self.model,
                self.provider.label()
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
            self.write_document_line(&format!("xfx: {problem}"));
            return;
        }
        if argument == self.model {
            self.say(format!("[shell] model={} unchanged", self.model));
            return;
        }
        if let Err(rejected) = self.work.submit(TurnWork::Model(argument.to_string())) {
            self.refused(rejected);
            return;
        }
        self.model = argument.to_string();
        self.say(format!("[shell] model={}", self.model));
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
            self.write_document_line(&interactive::setup_usage());
            return;
        };
        if let Err(rejected) = self.work.submit(TurnWork::Setup(provider)) {
            self.refused(rejected);
            return;
        }
        self.say(format!("[shell] setting up {}", provider.label()));
    }

    /// `/clear`: the screen, its scrollback, and what the band remembers of
    /// both.
    ///
    /// Three things go together and none of them is optional. The **bytes** are
    /// the loop's to write, because the loop owns the writer. The **transcript**
    /// is reset, because it counts the rows it has put on the screen and every
    /// one of them is about to stop existing -- an append measured against the
    /// old count would place its rows around a row that is no longer there. And
    /// the document writes already owed are **dropped**, because they were owed
    /// against that same screen.
    fn clear_screen(&mut self) {
        self.pending.clear();
        self.transcript = Transcript::new(self.geometry.cols);
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
        // Every row of the band is gone from the screen too, so the next frame
        // is a repaint of the whole thing rather than an optional one.
        self.render.request(Reason::ExternalDamage);
        self.say(CLEARED_NOTICE.to_string());
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
        // Not `amended`: an edit ends the walk, and this *is* the walk.
        self.edited();
        true
    }

    /// What a change to the composer's **text** owes, over what a caret move
    /// owes.
    ///
    /// The extra obligation is one thing and it is the walk: the line on the
    /// screen is the user's own now rather than the one the history handed
    /// back, so the next step back begins at the newest entry again and comes
    /// back to *this* text. Split from [`Self::edited`] rather than folded into
    /// it because the two are different questions -- `Left` through a recalled
    /// prompt is how a user reads it before deciding to step further back, and
    /// a rule that ended the walk on every keystroke would make that
    /// impossible.
    fn amended(&mut self) {
        self.history.leave();
        self.edited();
    }

    /// What a change to the composer's **text** owes, over what a caret move
    /// owes: the undo boundary.
    ///
    /// **Every text change passes through here and no caret move does**, which
    /// is the whole of the split. What an item-18 stack needs to know is what
    /// the last change to the text was; a field written by the repaint path
    /// would say "an ordinary edit" after a `Left`, and an undo built on it
    /// would take a megabyte back a grapheme at a time. The paste path records
    /// its own boundary *after* calling through here ([`Self::pasted`]), so one
    /// framed paste is one transaction and the keystroke after it is another.
    ///
    /// Nothing else is owed. While a block was a name this also had to re-read
    /// the draft once per block to see which of them the edit had damaged; a
    /// block is a span now and the edit already moved it (`super::entity`).
    fn edited(&mut self) {
        self.transaction.record(EditTransaction::Other);
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
        let panel = match self.slot() {
            Some(Slot::Question(panel)) => panel.height(cols, rows),
            Some(Slot::Menu(picker)) => picker.height(cols, rows),
            None => 0,
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
            .find_map(|input_rows| layout::solve_band(rows, cols, input_rows, activity, panel))
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
        let Some(geometry) = self.fit(rows, cols) else {
            return Resize::TooSmall;
        };
        self.geometry = geometry;
        self.transcript.resize_unfinished(cols);
        // Every row of the band is somewhere else, and so is every column.
        self.render.request(Reason::Resize);
        Resize::Repaint(geometry)
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
    use super::super::gesture::EXIT_WINDOW;
    use super::super::pacer::{MAX_CPS, MIN_CPS};

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
        /// Everything the document owes, as the text of its rows.
        fn document(&mut self) -> Vec<String> {
            self.shell
                .take_pending()
                .into_iter()
                .flat_map(|append| append.rows)
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
                .strip_prefix(PALETTE.hint())
                .and_then(|row| row.strip_suffix(PALETTE.reset()))
                .unwrap_or_else(|| panic!("the hint row was not painted in the palette: {row:?}"));
            inside.to_string()
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
        format!("{}{text}{}", PALETTE.hint(), PALETTE.reset())
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
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let (work, sent, control) = WorkHandle::detached();
        Fixture {
            shell: Shell::new(
                &config(home.path(), workspace.path()),
                crate::tui::layout::solve(rows, cols, 1).expect("a band"),
                PALETTE,
                work,
            ),
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
    fn the_band_is_a_rule_the_composer_and_a_hint_row() {
        let shell = shell(24, 80);
        let rows = shell.band_rows();
        assert_eq!(
            rows.len(),
            usize::from(shell.geometry.band_rows()),
            "the band's rows and its geometry disagree, so every row below the \
             first missing one is painted a row too high"
        );
        assert_eq!(rows[0], divider(80), "the divider");
        assert_eq!(rows[1], "> ", "the composer's prompt marker");
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
            before.rfind(PALETTE.hint()) > before.rfind(PALETTE.notice()),
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
    fn the_divider_spans_the_screen_it_was_solved_for() {
        // A rule of a fixed width would leave a gap on a wide terminal and run
        // off a narrow one -- and with autowrap off, running off is silent.
        for cols in [20u16, 80, 200] {
            let shell = shell(24, cols);
            // Measured in **cells**, by the painter's own rule, because the
            // row carries the palette now and an escape sequence is characters
            // that cost no columns.
            assert_eq!(
                super::super::wrap::width(&shell.band_rows()[0]),
                cols,
                "the rule did not span a {cols}-column screen"
            );
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
            work,
        );
        let rows = shell.band_rows();
        assert_eq!(rows.len(), 6, "divider, four composer rows, hint");
        assert_eq!(rows[1], "> ");
        assert_eq!(
            &rows[2..5],
            &["".to_string(), "".to_string(), "".to_string()]
        );
    }

    #[test]
    fn the_caret_sits_after_the_prompt_marker_on_the_composers_first_row() {
        let shell = shell(24, 80);
        assert_eq!(shell.cursor(), (23, 2));
        assert_eq!(
            shell.cursor().1,
            u16::try_from(PROMPT.chars().count()).expect("the marker's width"),
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
        assert_eq!(shell.band_rows()[1], "> hello \u{d55c}\u{ae00}");
        // Two cells of gutter, six of "hello ", and two apiece for the glyphs.
        assert_eq!(shell.cursor(), (23, 12));
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
        assert_eq!(shell.cursor(), (23, 3));
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
        assert_eq!(shell.band_rows()[1], "> abcd");
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
        assert_eq!(shell.band_rows()[1], "> ello");

        shell.route_bytes(&[0x05, 0x15]);
        assert_eq!(shell.band_rows()[1], "> ", "Ctrl-U left text behind");
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
        assert_eq!(shell.band_rows()[1], "> aXb");
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
            shell.band_rows()[1],
            "> a",
            "the keystroke after a settled Escape was eaten as a modifier"
        );
    }

    #[test]
    fn submitting_clears_the_composer_and_leaves_the_text_in_the_document() {
        let mut shell = shell(24, 80);
        let _first = shell.render.begin().expect("the first frame");
        shell.route_bytes(b"ask me\r");

        assert_eq!(shell.band_rows()[1], "> ", "the composer kept the text");
        assert_eq!(
            shell.take_pending(),
            vec![Append {
                scroll: 1,
                rows: vec!["ask me".to_string()]
            }]
        );
        assert!(
            shell.render.begin().is_some(),
            "a submission asked for no frame, so the band would keep showing \
             text the composer no longer holds"
        );
    }

    #[test]
    fn writes_given_back_unattempted_go_ahead_of_the_ones_owed_since() {
        // The loop takes the whole batch and can stop part-way through it when
        // the terminal refuses one (`super::event_loop::commit_frame`), so what
        // comes back was never offered to the screen. It comes back *in front*:
        // the tick that gives it back has already applied this tick's events,
        // and a document is a sequence -- a row that lands after one queued
        // behind it is as wrong as a row that never lands.
        let mut shell = shell(24, 80);
        shell.write_transcript("older\n");
        let untried = shell.take_pending();
        assert_eq!(untried.len(), 1, "the fixture owes one write, not a batch");
        shell.write_transcript("newer\n");
        shell.restore_pending(untried);

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
        assert_eq!(&shell.band_rows()[1..3], &["> first", "  second"]);
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
        assert_eq!(&shell.band_rows()[1..3], &["> first line", "  second line"]);
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
            shell.band_rows()[1],
            "> ab[Ac",
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
            shell.band_rows()[1],
            "> [Pasted text #1, 2 lines]",
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
            vec!["[Pasted text #1, 2 lines]".to_string()],
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
        assert!(shell.editor.insert(&typed), "the draft could not be set up");

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
            shell.editor.insert(&"x".repeat(400_000)),
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
                .insert(&"x".repeat(MAX_PASTE_BYTES - block.len() - summary)),
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
                .insert(&"x".repeat(MAX_PASTE_BYTES - block.len() - summary)),
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
        // **The boundary an undo will take**, fixed at the only moment that
        // knows it. A paste is one gesture and a great many bytes, and the
        // bytes arrive in as many reads as the terminal feels like: a boundary
        // inferred later from the buffer would be a boundary per read, or per
        // grapheme, and an undo built on it would take a megabyte back a
        // character at a time. There is no `C-z` on this surface -- item 18
        // brings the stack -- so this case is the seam's only reader, which is
        // what `.prd/06-qa-harness.md`'s scenario 21 points at.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"\x1b[200~");
        let chunk = vec![b'y'; 64];
        for _ in 0..40 {
            shell.route_bytes(&chunk);
        }
        shell.route_bytes(b"\x1b[201~");

        let Some(EditTransaction::InsertPaste {
            before,
            after,
            entity,
        }) = shell.transaction.last()
        else {
            panic!(
                "a framed paste did not record one insert transaction: {:?}",
                shell.transaction.last()
            );
        };
        assert_eq!(before, "", "the draft before the paste was not recorded");
        assert_eq!(after, "[Pasted text #1, 1 lines]");
        assert_eq!(entity.range(), 0..after.len());
        assert_eq!(
            entity.text().len(),
            40 * 64,
            "the transaction's entity does not carry the whole paste"
        );

        // And the next edit overwrites it, because what an undo needs to know
        // is what the *last* change was.
        shell.route_bytes(b"!");
        assert!(
            matches!(shell.transaction.last(), Some(EditTransaction::Other)),
            "a keystroke after a paste left the paste boundary standing: {:?}",
            shell.transaction.last()
        );
    }

    #[test]
    fn a_caret_move_leaves_the_paste_boundary_standing_and_an_edit_takes_it_down() {
        // **A move is not an edit.** The boundary an undo would take is a fact
        // about the last change to the *text*; a keystroke that only moved the
        // caret has not changed the text, so a stack built on this seam would
        // find "the last edit was an ordinary one" after a `Left` and take the
        // paste back a grapheme at a time -- which is the whole thing the
        // boundary exists to prevent.
        let mut shell = shell(24, 80);
        // Two rows, so that `Up` and `Down` have somewhere to go inside the
        // draft: typed **before** the paste, because typing is an edit and the
        // boundary this case is about is the paste's.
        shell.route_bytes(&[b'x'; 100]);
        let _block = collapsed(&mut shell, 1);
        assert!(
            matches!(
                shell.transaction.last(),
                Some(EditTransaction::InsertPaste { .. })
            ),
            "the paste did not record a boundary, so this case proves nothing"
        );

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
            assert!(
                matches!(
                    shell.transaction.last(),
                    Some(EditTransaction::InsertPaste { .. })
                ),
                "{name} moved the caret and took the paste boundary down with it: {:?}",
                shell.transaction.last()
            );
        }

        // And an edit -- any edit -- does take it down, because after one the
        // last change to the text is not the paste.
        shell.route_bytes(b"!");
        assert!(
            matches!(shell.transaction.last(), Some(EditTransaction::Other)),
            "a keystroke after a paste left the paste boundary standing: {:?}",
            shell.transaction.last()
        );
    }

    #[test]
    fn every_kind_of_edit_takes_the_paste_boundary_down() {
        // The other half, once per family, because "an edit overwrites it" is a
        // claim about the funnel every text change goes through rather than
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
            assert!(
                matches!(
                    shell.transaction.last(),
                    Some(EditTransaction::InsertPaste { .. })
                ),
                "{name}: the paste did not record a boundary"
            );
            shell.route_bytes(keys);
            assert!(
                matches!(shell.transaction.last(), Some(EditTransaction::Other)),
                "{name} left the paste boundary standing: {:?}",
                shell.transaction.last()
            );
        }
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
        shell.apply(UiEvent::Approval(asked()));
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
            "> fresh",
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
        assert_eq!(shell.geometry.divider, 22);
        shell.route_bytes(b"one\x0atwo\x0athree");
        assert_eq!(shell.geometry.input_rows(), 3);
        assert_eq!(
            (shell.geometry.divider, shell.geometry.content_bottom),
            (20, 19),
            "the composer took rows from the document without moving the \
             divider, so the band and the document overlap"
        );
        assert_eq!(shell.cursor(), (23, 7), "the caret is on the last row");

        // Back to one row, and every row number back with it. A kill takes the
        // line it is on, so what empties a three-line draft is submitting it.
        shell.route_bytes(&[0x15]);
        assert_eq!(
            shell.geometry.input_rows(),
            3,
            "the kill took a whole draft"
        );
        shell.route_bytes(&[0x0d]);
        assert_eq!(shell.band_rows().len(), 3, "divider, composer, hint");
        assert_eq!(
            (shell.geometry.divider, shell.geometry.content_bottom),
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
        assert_eq!(shell.geometry.divider, 6);
        assert_eq!(
            shell.band_rows()[1..6],
            [
                "> one".to_string(),
                "  two".to_string(),
                "  three".to_string(),
                "  four".to_string(),
                "  five".to_string(),
            ]
        );

        shell.route_bytes(b"\x0asix");
        assert_eq!(shell.geometry.input_rows(), 5, "the cap did not hold");
        assert_eq!(
            shell.band_rows()[1..6],
            [
                "  two".to_string(),
                "  three".to_string(),
                "  four".to_string(),
                "  five".to_string(),
                "  six".to_string(),
            ],
            "the window did not follow the caret"
        );
        assert_eq!(
            shell.cursor(),
            (11, 5),
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
        assert!(shell.editor.insert(&"x\n".repeat(70_000)));
        shell.edited();

        assert_eq!(shell.geometry.input_rows(), 11, "the cap");
        let rows = shell.band_rows();
        assert_eq!(rows[1], "  x", "the window is not showing the draft's end");
        assert_eq!(
            rows[11], "  ",
            "the last composer row is the empty one the caret is on: {rows:?}"
        );
        assert_eq!(
            shell.cursor(),
            (23, 2),
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
            &shell.band_rows()[1..3],
            &["> ".to_string() + &"x".repeat(18), "  x".to_string()]
        );
        assert_eq!(shell.cursor(), (23, 3));
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
        assert_eq!(shell.document(), vec!["say the marker".to_string()]);
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
    fn a_running_turn_says_what_it_is_doing_on_the_row_above_the_divider() {
        // The band gains a row while a turn runs, and it comes off the bottom
        // of the document rather than moving the composer: the caret must not
        // jump because a turn started.
        let mut shell = shell(24, 80);
        let idle = shell.geometry;
        assert_eq!(idle.activity, None);
        assert_eq!(shell.band_rows()[0], divider(80));

        let started = turn_running(&mut shell, b"ask something\r");
        shell.settle_band(started + Duration::from_secs(2));

        let geometry = shell.geometry;
        assert_eq!(
            geometry.activity,
            Some(geometry.divider - 1),
            "the row is not the one directly above the divider"
        );
        assert_eq!(geometry.divider, idle.divider, "the divider moved");
        assert_eq!(geometry.input_first, idle.input_first, "the caret moved");
        assert_eq!(geometry.content_bottom, idle.content_bottom - 1);

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
        assert!(rows[0].contains("2s"), "{rows:?}");
        assert_eq!(rows[1], divider(80), "the rule moved");
        assert_eq!(rows.last().expect("a hint row"), &hint_row(IDLE_HINT));
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
            shell.band_rows()[0].contains("2s"),
            "{:?}",
            shell.band_rows()
        );

        shell.route_bytes(b"second\r");
        shell.settle_band(started + Duration::from_secs(3));

        let rows = shell.band_rows();
        assert!(
            rows[0].contains("3s"),
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
            shell.band_rows()[0].contains("2s"),
            "{:?}",
            shell.band_rows()
        );

        shell.apply(UiEvent::TurnEnded { failure: None });
        shell.apply(UiEvent::TurnStarted);
        let _ = shell.document();
        shell.settle_band(started + Duration::from_secs(3));

        let rows = shell.band_rows();
        assert!(
            rows[0].contains("0s"),
            "the queued turn inherited the finished turn's clock: {rows:?}"
        );
        shell.settle_band(started + Duration::from_secs(5));
        assert!(
            shell.band_rows()[0].contains("2s"),
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
        // exactly like a turn that has gone quiet, unless the band says which.
        let mut shell = shell(24, 80);
        let started = turn_running(&mut shell, b"ask something\r");

        shell.apply(UiEvent::ToolStart {
            call_id: "1".to_string(),
            tool: "read_file".to_string(),
        });
        shell.settle_band(started + Duration::from_millis(50));
        assert!(
            shell.band_rows()[0].contains("read_file"),
            "{:?}",
            shell.band_rows()
        );

        shell.apply(UiEvent::ToolResult {
            call_id: "1".to_string(),
            tool: "read_file".to_string(),
            ok: true,
            detail: String::new(),
        });
        shell.settle_band(started + Duration::from_millis(100));
        let rows = shell.band_rows();
        assert!(rows[0].contains("Thinking"), "{rows:?}");
        assert!(!rows[0].contains("read_file"), "{rows:?}");
        // The tool was part of the turn, so the turn's clock ran through it.
        shell.settle_band(started + Duration::from_secs(4));
        assert!(
            shell.band_rows()[0].contains("4s"),
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
                    .and_then(|row| row.chars().next())
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

    /// The same question about a change too big for the band's own summary.
    fn asked_about_a_large_change() -> ApprovalRequest {
        let mut request = asked();
        request.diff = Some(crate::permission::ApprovalDiff {
            before: "a".repeat(4_000),
            after: "b".repeat(4_000),
        });
        request
    }

    /// A shell with a turn running and a question in front of the user.
    fn asking(shell: &mut Fixture) -> Instant {
        let started = turn_running(shell, b"edit the notes\r");
        shell.apply(UiEvent::Approval(asked()));
        shell.settle_band(started);
        let _ = shell.document();
        started
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
        shell.apply(UiEvent::Approval(asked()));
        shell.settle_band(started);
        let _ = shell.document();
        started
    }

    #[test]
    fn a_question_takes_the_rows_above_the_rule_and_leaves_the_composer_where_it_was() {
        // The band grows upward for a question exactly as it does for a turn:
        // the divider, the composer and everything below stay put, and the rows
        // come off the bottom of the document. A panel that moved the composer
        // would make answering it start with finding where the caret went.
        let mut shell = shell(24, 80);
        let working = {
            let started = turn_running(&mut shell, b"edit the notes\r");
            let _ = started;
            shell.geometry
        };
        assert_eq!(working.panel, 0);

        shell.apply(UiEvent::Approval(asked()));

        assert_eq!(
            shell.geometry.panel, 8,
            "a 24-row screen gets the compact panel"
        );
        assert_eq!(shell.geometry.divider, working.divider);
        assert_eq!(shell.geometry.input_first, working.input_first);
        assert_eq!(shell.geometry.hint, working.hint);
        assert_eq!(shell.geometry.content_bottom, working.content_bottom - 8);

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
        shell.apply(UiEvent::Approval(asked()));
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
            assert_eq!(before.panel, 8);

            shell.route_bytes(&[typed]);

            assert_eq!(
                shell.controlled(),
                Some(TurnControl::Answer(answer)),
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

        shell.route_bytes(b"\t"); // the second choice
        assert_eq!(shell.controlled(), None, "Tab answered instead of moving");
        shell.route_bytes(&[0x1b, 0x5b, 0x41]); // Up, back to the first
        shell.route_bytes(&[0x1b, 0x5b, 0x42]); // Down, forward again
        shell.route_bytes(&[0x0d]);

        assert_eq!(
            shell.controlled(),
            Some(TurnControl::Answer(ApprovalAnswer::Always)),
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
        shell.apply(UiEvent::Approval(asked()));
        shell.settle_band(started);

        shell.route_bytes(&[0x1b]);
        shell.settle_input(Instant::now() + Duration::from_millis(100));

        assert_eq!(
            shell.controlled(),
            Some(TurnControl::Answer(ApprovalAnswer::Deny))
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
            said.contains(&crate::app::INTERRUPT_NOTICE.to_string()),
            "the interrupt landed silently: {said:?}"
        );
        assert!(
            said.contains(&QUEUE_DROPPED.to_string()),
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
            Some(TurnControl::Answer(ApprovalAnswer::Deny))
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
        // clock -- the same seam every other timed row is settled on.
        shell.apply(UiEvent::Approval(asked()));
        shell.settle_band(started + Duration::from_secs(2));
        shell.settle_band(started + Duration::from_secs(30));
        assert_eq!(
            shell.activity_row.as_deref(),
            Some(before.as_str()),
            "the clock ran while xfx was waiting for the user"
        );

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

        shell.apply(UiEvent::Approval(asked_about_a_large_change()));
        shell.settle_band(started);
        assert_eq!(
            shell.screen_owner(),
            ScreenOwner::Approval,
            "a change the band cannot show was left for the band to show"
        );

        shell.route_bytes(b"1");
        assert_eq!(
            shell.controlled(),
            Some(TurnControl::Answer(ApprovalAnswer::Once))
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

        shell.apply(UiEvent::Approval(asked()));
        shell.settle_band(started);

        assert_eq!(shell.screen_owner(), ScreenOwner::Primary);
        assert_eq!(shell.geometry.panel, 8);
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
            shell.apply(UiEvent::Approval(asked_about_a_large_change()));
            shell.settle_band(started);
            assert_eq!(shell.screen_owner(), ScreenOwner::Approval);
            assert_eq!(shell.marked(), "> 1. Yes", "the caret left the question");

            shell.route_bytes(typed);
            shell.settle_input(Instant::now() + Duration::from_millis(100));

            assert_eq!(
                shell.controlled(),
                Some(TurnControl::Answer(answer)),
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
        shell.apply(UiEvent::Approval(asked_about_a_large_change()));
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
    fn a_question_refused_because_it_does_not_fit_never_takes_the_screen() {
        // The refusal happens before anything is installed, so there is nothing
        // to give back -- and an owner moved by a question that was never asked
        // would strand the session on an empty plane.
        let mut shell = shell(10, 80);
        turn_running(&mut shell, b"edit the notes\r");

        shell.apply(UiEvent::Approval(asked_about_a_large_change()));

        assert_eq!(shell.screen_owner(), ScreenOwner::Primary);
        assert_eq!(
            shell.controlled(),
            Some(TurnControl::Answer(ApprovalAnswer::Deny))
        );
        assert_eq!(shell.released(), vec![PANEL_TOO_SMALL.to_string()]);
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
        shell.apply(UiEvent::Approval(asked_about_a_large_change()));
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
        shell.apply(UiEvent::Approval(asked_about_a_large_change()));
        shell.settle_band(started);
        assert!(!shell.screen_rows().is_empty(), "nothing was installed");

        shell.route_bytes(b"1");

        assert_eq!(
            shell.controlled(),
            Some(TurnControl::Answer(ApprovalAnswer::Once))
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
        shell.apply(UiEvent::Approval(asked_about_a_large_change()));
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
        shell.apply(UiEvent::Approval(asked_about_a_large_change()));
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
        shell.apply(UiEvent::Approval(asked()));

        assert_eq!(shell.geometry.panel, 0);
        assert_eq!(
            shell.controlled(),
            Some(TurnControl::Answer(ApprovalAnswer::Deny))
        );
        assert_eq!(shell.released(), vec![PANEL_TOO_SMALL.to_string()]);
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

        shell.apply(UiEvent::Approval(asked()));

        assert_eq!(shell.geometry.panel, 8, "the question was refused instead");
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
            Some(TurnControl::Answer(ApprovalAnswer::Once)),
            "the answer went nowhere"
        );
        assert!(
            shell.sent.try_recv().is_err(),
            "the answer was sent as work rather than as control"
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
            format!("{}{QUEUE_REJECTED}{}", PALETTE.notice(), PALETTE.hint())
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
        assert_eq!(
            shell.document(),
            vec![crate::app::INTERRUPT_NOTICE.to_string()]
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
        assert_eq!(
            shell.document(),
            vec![
                crate::app::INTERRUPT_NOTICE.to_string(),
                QUEUE_DROPPED.to_string()
            ]
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
            vec![crate::app::INTERRUPT_NOTICE.to_string()]
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
            if *name == "/clear" {
                // The one command whose echo is deliberately not there: the row
                // it would have been written on is part of what the command
                // erased.
                assert_eq!(document, vec![CLEARED_NOTICE.to_string()]);
                continue;
            }
            assert!(
                document.first().map(String::as_str) == Some(*name),
                "{name} was not echoed into the document: {document:?}"
            );
            // `/quit` is the one whose answer is the session ending rather than
            // a row; every other one says something.
            if *name == "/quit" {
                assert!(shell.leaving(), "/quit did not end the session");
            } else {
                assert!(document.len() > 1, "{name} answered with nothing");
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
        // The same refusal the line-oriented shell gives, from the same call,
        // because a command surface that answers differently depending on which
        // front end you typed it into is the one thing it must never be. A typo
        // silently becoming a prompt is what this costs tokens to prevent.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"/notacommand\r");

        let document = shell.document();
        assert_eq!(document.first().map(String::as_str), Some("/notacommand"));
        assert!(
            document
                .iter()
                .any(|row| row.contains("is not an xfx command")),
            "{document:?}"
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

        shell.route_bytes(b"/model\r");
        let document = shell.document();
        assert!(
            document
                .iter()
                .any(|row| row.starts_with("[shell] model=second-model")),
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
                "xfx: /model takes one model id, with no spaces in it",
            ),
            (
                "bad\u{7}id",
                "xfx: a model id cannot contain control characters",
            ),
            (too_long.as_str(), "xfx: that model id is too long"),
        ] {
            let mut shell = shell(24, 80);
            let in_force = shell.shell.model.clone();
            shell.use_model(argument);

            assert_eq!(
                shell.document(),
                vec![refusal.to_string()],
                "the refusal is not the line the line shell gives"
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
            !shell.shell.pending.is_empty(),
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
        assert_eq!(
            document,
            vec![CLEARED_NOTICE.to_string()],
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
            vec![Append {
                scroll: 1,
                rows: vec!["the next answer".to_string()]
            }],
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

        assert_eq!(shell.document(), vec![GONE_NOTICE.to_string()]);
    }

    #[test]
    fn the_answer_arrives_as_deltas_and_lands_in_the_document() {
        let mut shell = shell(24, 80);
        shell.apply(UiEvent::Delta("MARKER-TURN-".to_string()));
        shell.apply(UiEvent::Delta("ONE".to_string()));
        shell.apply(UiEvent::TurnEnded { failure: None });

        // The last row is rewritten in place as it lengthens, so the row the
        // document keeps is the whole answer rather than its first fragment.
        assert_eq!(
            shell.released().last().map(String::as_str),
            Some("MARKER-TURN-ONE")
        );
    }

    #[test]
    fn an_answer_is_released_over_several_frames_rather_than_dumped() {
        // What the pacer is for. A provider sends a burst; a UI that appended
        // it whole shows the answer as a jump, and this one shows it as a
        // stream -- `pacer::MIN_CPS` at the slowest, so a fragment this size
        // takes a tenth of a second and not one frame.
        let mut shell = shell(24, 80);
        let start = Instant::now();
        shell.apply(UiEvent::Delta("x".repeat(60)));
        // The clock's first reading. Nothing is owed for it, which is the
        // difference between a pacer and a queue.
        shell.settle_band(start);
        assert!(
            shell.document().is_empty(),
            "the whole delta was appended the instant it arrived"
        );
        let first = shell.paced(start, TICK_MILLIS);
        assert_eq!(
            first.last().map(String::len),
            Some(usize::try_from(MIN_CPS).expect("a rate") * 8 / 1000),
            "one tick released more than the floor rate pays for"
        );
        // and the rest of it arrives, over the ticks that pay for it
        let rows = shell.paced(start + Duration::from_millis(TICK_MILLIS), 200);
        assert_eq!(
            rows.last().map(String::as_str),
            Some("x".repeat(60).as_str())
        );
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
        // ask on, or a `3` typed at the panel.
        let mut shell = shell(24, 80);
        shell.apply(UiEvent::ToolStart {
            call_id: "c1".to_string(),
            tool: "write_file".to_string(),
        });
        shell.apply(UiEvent::ToolResult {
            call_id: "c1".to_string(),
            tool: "write_file".to_string(),
            ok: false,
            detail: "no approval channel".to_string(),
        });

        assert_eq!(
            shell.document(),
            vec![
                "[tool] write_file running".to_string(),
                "[tool] write_file refused: no approval channel".to_string(),
            ]
        );
    }

    #[test]
    fn two_notices_in_a_row_do_not_put_a_blank_row_between_them() {
        // `Transcript::end_line` answers a request to end a line that is
        // already ended with a blank row of its own -- which is right for two
        // breaks in an answer and wrong for two notices. Without the guard in
        // `finish_document_line` every notice after the first costs the
        // document an empty row it can never take back.
        let mut shell = shell(24, 80);
        shell.apply(UiEvent::Notice("first".to_string()));
        shell.apply(UiEvent::Notice("second".to_string()));

        assert_eq!(
            shell.document(),
            vec!["first".to_string(), "second".to_string()]
        );
    }

    #[test]
    fn a_notice_that_lands_mid_answer_gets_a_row_of_its_own() {
        let mut shell = shell(24, 80);
        shell.apply(UiEvent::Delta("half a sentence".to_string()));
        shell.apply(UiEvent::Notice("[tool] read_file ok".to_string()));

        assert_eq!(
            shell.released(),
            vec![
                "half a sentence".to_string(),
                "[tool] read_file ok".to_string()
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
        shell.apply(UiEvent::Delta("the first half ".to_string()));
        shell.apply(UiEvent::Notice("[tool] read_file ok".to_string()));
        shell.apply(UiEvent::Delta("and the second".to_string()));
        shell.settle_band(start);
        assert!(
            shell.document().is_empty(),
            "the notice was written before any of the answer was"
        );

        let rows = shell.paced(start, 400);
        let notice = rows
            .iter()
            .position(|row| row == "[tool] read_file ok")
            .expect("the notice");
        assert_eq!(
            rows[notice - 1],
            "the first half ",
            "the notice landed before the text it came after: {rows:?}"
        );
        assert_eq!(
            rows.last().map(String::as_str),
            Some("and the second"),
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
        shell.apply(UiEvent::Notice("said at once".to_string()));
        assert_eq!(shell.document(), vec!["said at once".to_string()]);
    }

    #[test]
    fn a_turn_that_failed_says_why_in_the_document() {
        let mut shell = shell(24, 80);
        shell.apply(UiEvent::Delta("part of an answer".to_string()));
        shell.apply(UiEvent::TurnEnded {
            failure: Some("the turn was cancelled".to_string()),
        });

        assert_eq!(
            shell.released(),
            vec![
                "part of an answer".to_string(),
                "the turn was cancelled".to_string()
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
        let narrow_divider = shell.geometry.divider;
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
            shell.geometry.divider > narrow_divider,
            "the divider did not move when the composer stopped wrapping"
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
        assert_eq!(shell.geometry.panel, 8);

        assert!(matches!(shell.resize(30, 100), Resize::Repaint(_)));

        assert_eq!(
            shell.geometry.panel, 8,
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
        // And the answer still reaches the runtime afterwards.
        shell.route_bytes(b"1");
        assert_eq!(
            shell.controlled(),
            Some(TurnControl::Answer(ApprovalAnswer::Once)),
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
        assert_eq!(shell.geometry.panel, 8);
        assert!(
            shell.band_rows().join("\n").contains("Permission needed"),
            "the question was not painted after the screen grew back"
        );
        shell.route_bytes(b"1");
        assert_eq!(
            shell.controlled(),
            Some(TurnControl::Answer(ApprovalAnswer::Once)),
            "the decision no longer reaches the runtime"
        );
    }

    #[test]
    fn a_resize_keeps_the_turns_row_above_the_divider() {
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
        assert_eq!(shell.geometry.activity, Some(shell.geometry.divider - 1));
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

    /// The band's rows above the divider: what the menu, if there is one, put
    /// there.
    ///
    /// Read off the painted band rather than off the field, because the claim
    /// every case below makes is about what is on the screen -- and a menu the
    /// geometry gave rows to but nothing painted into is the exact defect the
    /// one-iterator rule exists to prevent.
    fn menu(shell: &Shell) -> Vec<String> {
        let rows = shell.band_rows();
        let rule = divider(usize::from(shell.geometry.cols));
        let at = rows
            .iter()
            .position(|row| *row == rule)
            .expect("the band has a divider");
        // Past the row a running turn owns, which is above the slot rather than
        // in it: a reader that counted it would report an idle band as one with
        // a one-row menu on it the moment a turn started.
        let from = usize::from(u8::from(shell.geometry.activity.is_some()));
        rows[from..at].to_vec()
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
        // the difference between a full menu and none.
        let mut bare = shell(24, 80);
        bare.route_bytes(b"/");
        assert_eq!(
            usize::from(bare.geometry.panel),
            crate::interactive::SLASH_COMMANDS.len(),
            "a bare slash did not offer every command: {:?}",
            menu(&bare)
        );
        assert_eq!(usize::from(bare.geometry.panel), menu(&bare).len());
        assert!(
            rows.iter().any(|row| row.contains("/help")),
            "the menu does not offer the command the draft names: {rows:?}"
        );
        assert!(
            rows[0].starts_with("> "),
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
        assert_eq!(shell.marked(), format!("{PROMPT}/mo"));
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
    fn tab_completes_the_marked_command_and_closes_the_menu() {
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
            "the menu outlived the completion"
        );

        // A command that takes no argument is completed exactly, so the next
        // Return runs it.
        let mut exact = shell(24, 80);
        exact.route_bytes(b"/vers");
        exact.route_bytes(b"\t");
        assert_eq!(exact.editor.text(), "/version");
        assert_eq!(exact.geometry.panel, 0);
        exact.route_bytes(&[0x0d]);
        let document = exact.document();
        assert_eq!(document.first().map(String::as_str), Some("/version"));
        assert!(document.len() > 1, "{document:?}");
    }

    #[test]
    fn enter_runs_the_line_rather_than_taking_the_marked_row() {
        // The menu is open on an exact name, and the marked row is a different
        // command: Enter must still run what was typed.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"/new");
        assert!(shell.geometry.panel > 0);
        shell.route_bytes(&[0x0d]);

        assert_eq!(shell.geometry.panel, 0, "the menu outlived the submission");
        assert_eq!(
            shell.picks_up(),
            TurnWork::New,
            "Enter took the menu's row instead of running the line"
        );
        assert!(shell.editor.is_empty());
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

        shell.apply(UiEvent::Approval(asked()));
        shell.settle_band(started);
        let _ = shell.document();

        let rows = menu(&shell);
        assert!(
            rows.iter().any(|row| row.contains(approval::TITLE)),
            "the question did not take the slot: {rows:?}"
        );
        assert!(
            !rows.iter().any(|row| row.contains("list these commands")),
            "the menu and the question were painted together: {rows:?}"
        );
        assert_eq!(
            shell.geometry.panel,
            approval::COMPACT_ROWS,
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
        shell.apply(UiEvent::Approval(asked()));
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
            "/new",
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

        // Every command matches a bare slash, and one row is what this screen
        // has for them: the window bounds the menu rather than the band
        // refusing it.
        assert_eq!(
            shell.geometry.panel, 1,
            "a six-row screen with a turn on it did not get its one menu row: {:?}",
            shell.geometry
        );
        assert_eq!(menu(&shell).len(), 1, "{:?}", menu(&shell));
        assert!(menu(&shell)[0].starts_with("> /help"), "{:?}", menu(&shell));

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
            (Some(2), 3, 4, 5, 6, 1),
            "the six rows are not laid out as a turn, a menu, a rule, a \
             composer and a hint with one document row left: {:?}",
            shell.geometry
        );

        // The composer keeps its row and the caret stays in it.
        assert_eq!(shell.geometry.input_rows(), 1, "the composer lost its row");
        assert_eq!(shell.cursor(), (5, PROMPT_CELLS + 1));
        assert_eq!(shell.marked(), format!("{PROMPT}/"));
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
        assert!(rows[0].starts_with("> /help"), "{rows:?}");
        assert_eq!(shell.geometry.input_rows(), 1, "the composer lost its row");
        assert_eq!(
            shell.cursor(),
            (shell.geometry.input_first, PROMPT_CELLS + 1)
        );
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
        // xfx's own words: the *refusal* quotes nothing of the argument, which
        // is what keeps a hostile one out of the document. The submitted line
        // itself is echoed like every other one, and that row is the user's own
        // keystrokes rather than xfx repeating them back as guidance.
        let refusals: Vec<&String> = document
            .iter()
            .filter(|row| row.starts_with("xfx: "))
            .collect();
        assert_eq!(refusals.len(), 4, "{document:?}");
        assert!(
            !refusals.iter().any(|row| row.contains("nonesuch")),
            "the refusal quoted the argument back: {refusals:?}"
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
            document
                .iter()
                .any(|row| row.contains("provider=llmux") && row.contains("fresh conversation")),
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
        let document = shell.document();
        assert!(
            document.iter().any(|row| row == "[shell] catalog=2 shown"),
            "{document:?}"
        );
        assert!(
            document
                .iter()
                .any(|row| row == "[shell]   fable context=1000000 efforts=low,high"),
            "{document:?}"
        );
        // A provider really may publish neither, and an empty column would read
        // as a rendering fault rather than as an absent fact.
        assert!(
            document
                .iter()
                .any(|row| row == "[shell]   plain context=unknown efforts=none"),
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
        assert!(!shell.hint().contains("Context"), "{}", shell.hint());

        // A numerator with no denominator is not a meter either.
        let mut bare = fixture_with_no_catalog();
        bare.shell.model = "fable".to_string();
        bare.shell.apply(UiEvent::Usage {
            input: Some(12_345),
            output: Some(9),
        });
        assert_eq!(bare.shell.context_meter(), None);
        assert!(!bare.hint().contains("Context"), "{}", bare.hint());

        // Both, and exactly one segment.
        shell.shell.apply(UiEvent::Usage {
            input: Some(12_345),
            output: Some(9),
        });
        assert_eq!(shell.shell.context_meter(), Some((12_345, 200_000)));
        let row = shell.hint();
        assert!(row.contains("Context: 12k/200k 6%"), "{row}");
        assert_eq!(row.matches("Context:").count(), 1, "{row}");
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
        assert!(!shell.hint().contains("Context"), "{}", shell.hint());
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
        assert!(shell.hint().contains("Context"), "{}", shell.hint());

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
        assert!(!shell.hint().contains("Context"), "{}", shell.hint());
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
        assert_eq!(shell.band_rows()[1], "> second line");
        assert_eq!(
            shell.cursor(),
            (23, 13),
            "the caret is not at the end of the recalled line"
        );
        shell.route_bytes(&[0x10]);
        assert_eq!(shell.band_rows()[1], "> first line");
        shell.route_bytes(&[0x10]);
        assert_eq!(
            shell.band_rows()[1],
            "> first line",
            "the walk wrapped past the oldest line"
        );
        shell.route_bytes(&[0x0e]);
        assert_eq!(shell.band_rows()[1], "> second line");
        shell.route_bytes(&[0x0e]);
        assert_eq!(
            shell.band_rows()[1],
            "> half typed",
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
        assert_eq!(shell.marked(), "> /version");
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
            shell.band_rows()[1],
            "> third",
            "a refused submission threw the draft away"
        );
        // Cleared first, or the draft the refusal left standing would satisfy
        // this case without a single line having been recorded.
        shell.route_bytes(b"\x15");
        assert_eq!(shell.band_rows()[1], "> ");
        shell.route_bytes(&[0x10]);
        assert_eq!(shell.band_rows()[1], "> third");
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
        assert_eq!(shell.band_rows()[1], "> real");
    }

    #[test]
    fn the_up_arrow_moves_the_caret_until_the_first_row_and_only_then_walks_back() {
        // The edge rule. An arrow inside a multi-row draft is the movement it
        // is in every editor; it becomes the recall exactly where it would
        // otherwise do nothing at all.
        let mut shell = shell(24, 80);
        submitted(&mut shell, "sent");
        shell.route_bytes(b"top\x0abottom");
        assert_eq!(&shell.band_rows()[1..3], &["> top", "  bottom"]);
        assert_eq!(
            shell.marked(),
            "  bottom",
            "the caret did not start on the draft's last row"
        );

        shell.route_bytes(b"\x1b[A");
        assert_eq!(
            shell.marked(),
            "> top",
            "the arrow did not move the caret up a row of the draft"
        );
        assert_eq!(
            &shell.band_rows()[1..3],
            &["> top", "  bottom"],
            "an arrow inside the draft walked the history instead of the rows"
        );

        shell.route_bytes(b"\x1b[A");
        assert_eq!(
            shell.band_rows()[1],
            "> sent",
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
            shell.band_rows()[1],
            "> sent",
            "the arrow at a one-row draft's only row did not reach the history"
        );
        shell.route_bytes(b"\x1b[B");
        assert_eq!(
            shell.band_rows()[1],
            "> half typed",
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
            "  bottom",
            "the arrow did not move back down"
        );
        assert_eq!(&shell.band_rows()[1..3], &["> top", "  bottom"]);
        shell.route_bytes(b"\x1b[B");
        assert_eq!(
            &shell.band_rows()[1..3],
            &["> top", "  bottom"],
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
            shell.band_rows()[1],
            "> sent",
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
        assert_eq!(shell.band_rows()[1], "> first");

        shell.route_bytes(b"!");
        assert_eq!(shell.band_rows()[1], "> first!");
        shell.route_bytes(&[0x10]);
        assert_eq!(
            shell.band_rows()[1],
            "> second",
            "the walk carried on from the line the edit had left behind"
        );
        shell.route_bytes(&[0x0e]);
        assert_eq!(
            shell.band_rows()[1],
            "> first!",
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
        assert_eq!(shell.band_rows()[1], "> first");
        shell.route_bytes(&[0x7f]);
        assert_eq!(shell.band_rows()[1], "> firs");
        shell.route_bytes(&[0x10]);
        assert_eq!(
            shell.band_rows()[1],
            "> second",
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
        assert_eq!(shell.band_rows()[1], "> second");
        shell.route_bytes(b"\x1b[D");
        shell.route_bytes(&[0x10]);
        assert_eq!(
            shell.band_rows()[1],
            "> first",
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
        assert_eq!(shell.band_rows()[1], "> first");

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
            shell.band_rows()[1],
            "> second",
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
        assert_eq!(shell.band_rows()[1], "> first");

        shell.route_bytes(b"\x1b[200~");
        shell.route_bytes(b" and more");
        shell.route_bytes(b"\x1b[201~");
        assert_eq!(shell.band_rows()[1], "> first and more");

        shell.route_bytes(&[0x10]);
        assert_eq!(
            shell.band_rows()[1],
            "> second",
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
        assert_eq!(shell.band_rows()[1], "> first");

        // Home first, which is a caret move and must leave the walk open --
        // otherwise this case would be about the move rather than the delete.
        shell.route_bytes(&[0x01]);
        shell.route_bytes(&[0x04]);
        assert_eq!(shell.band_rows()[1], "> irst");

        shell.route_bytes(&[0x10]);
        assert_eq!(
            shell.band_rows()[1],
            "> second",
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
        assert_eq!(shell.band_rows()[1], "> first");

        shell.route_bytes(b"\x1b[200~");
        shell.route_bytes("y".repeat(1200).as_bytes());
        shell.route_bytes(b"\x1b[201~");
        assert_eq!(shell.band_rows()[1], "> first[Pasted text #1, 1 lines]");

        shell.route_bytes(&[0x10]);
        assert_eq!(
            shell.band_rows()[1],
            "> second",
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
            shell.band_rows()[1],
            "> [Pasted text #1, 1 lines]",
            "the paste was not collapsed, so this case proves nothing"
        );

        shell.route_bytes(&[0x10]);
        assert_eq!(shell.band_rows()[1], "> earlier");
        shell.route_bytes(&[0x0e]);
        assert_eq!(
            shell.band_rows()[1],
            "> [Pasted text #2, 1 lines]",
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
            shell.band_rows()[1],
            "> [Pasted text #1, 1 lines]",
            "the paste was not collapsed, so this case proves nothing"
        );

        shell.route_bytes(&[0x10]);
        assert_eq!(
            shell.band_rows()[1],
            "> [Pasted text #1, 1 lines]",
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
        assert_eq!(shell.band_rows()[1], "> ask me again");
        shell.route_bytes(&[0x10]);
        assert_eq!(
            shell.band_rows()[1],
            "> ask me again",
            "the same line sent twice running became two entries"
        );
    }
}
