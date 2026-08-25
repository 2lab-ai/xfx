//! What the band is a picture of.
//!
//! The event loop reads bytes and writes frames; everything between the two --
//! what the band's rows say, where the caret is, and whether the session is
//! leaving -- is here, so that "what would the band look like now" is a
//! question about a value rather than about a terminal.
//!
//! In this phase that value is small. The band is a divider, the composer, and
//! a hint row the session owns and leaves empty until the phase that fills it;
//! there is no turn yet, and the task that adds one adds it here. What is
//! already true is the shape: the rows are produced top-down from the geometry,
//! and the caret is reported in the composer's own coordinates rather than
//! derived a second time by whatever draws it.
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

use std::process::ExitCode;
use std::time::Instant;

use super::bridge::{TurnControl, TurnWork, UiEvent};
use super::editor::{self, Editor};
use super::gesture::{Escape, Gestures, Interrupt, INTERRUPTED_EXIT_CODE};
use super::input::{Action, Decoder, Input};
use super::layout::{self, Geometry};
use super::render_request::{Reason, RenderRequest};
use super::transcript::{Append, Transcript};
use super::worker::{Rejected, WorkHandle};
use crate::config::RuntimeConfig;
use crate::interactive::{self, Slash, Submitted};
use crate::output::safe_one_line;

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

/// What the user is told when the runtime thread is not there to take work.
///
/// Told apart from [`QUEUE_REJECTED`] on purpose: a runtime that is gone is not
/// a runtime with a queue, and a fatal event is already on its way to say so.
/// In the document rather than on the hint row, because unlike a full queue it
/// is not a condition that clears.
const GONE_NOTICE: &str = "xfx: the runtime is gone; that line was not sent";

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

/// The UI's state: what the band shows, and what it owes the screen.
pub(crate) struct Shell {
    pub(crate) geometry: Geometry,
    pub(crate) render: RenderRequest,
    /// The model a turn will talk to.
    ///
    /// Read from the configuration once, at startup, rather than consulted per
    /// frame: the hint row renders a compact form of it and a `/model` change
    /// replaces it, and both want one field rather than a borrow of the whole
    /// configuration.
    model: String,
    /// The text being composed, and where the caret is in it.
    editor: Editor,
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
    pub(crate) fn new(config: &RuntimeConfig, geometry: Geometry, work: WorkHandle) -> Self {
        Self {
            geometry,
            // A session that has drawn nothing owes a frame. Requesting it here
            // rather than in the loop is what keeps "the band appears" a
            // property of having a shell at all.
            render: {
                let mut render = RenderRequest::default();
                render.request(Reason::FirstFrame);
                render
            },
            model: config.model.clone(),
            editor: Editor::new(),
            decoder: Decoder::new(),
            // Wrapped to the screen the band was solved for: the document rows
            // and the band rows share a terminal, and a transcript measured
            // against a different width would wrap where the screen does not.
            transcript: Transcript::new(geometry.cols),
            pending: Vec::new(),
            work,
            gestures: Gestures::default(),
            notice: None,
            escape_armed: false,
            queued: 0,
            clearing: false,
            fatal: None,
            leaving: None,
        }
    }

    /// The band's rows, top first, starting at the divider.
    ///
    /// Exactly as many rows as the band owns: the writer places them by
    /// counting down from the divider, so a row missing here would shift every
    /// row below it up by one.
    pub(crate) fn band_rows(&self) -> Vec<String> {
        let mut rows = Vec::with_capacity(usize::from(self.geometry.band_rows()));
        rows.push(std::iter::repeat_n(RULE, usize::from(self.geometry.cols)).collect());
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
        while rows.len() <= usize::from(self.geometry.input_rows()) {
            rows.push(String::new());
        }
        // The hint row. Two of its segments exist in this phase -- what the
        // queue is holding, and whether a second Escape would throw the draft
        // away -- and a refusal takes the left of it whole, because a refusal
        // is about the keystroke that just happened and the rest is about the
        // state. Task 16 replaces the joining with upstream's, on the same
        // three facts.
        rows.push(self.hint_row());
        rows
    }

    /// What the band's last row says.
    ///
    /// Not budgeted: a row wider than the screen is clipped by the painter like
    /// every other band row (`super::frame`'s `row_text`), so nothing overflows
    /// -- but on a narrow terminal the warning is what falls off the end rather
    /// than the segment a reader would have chosen to drop. Task 16 is where
    /// the segments learn to give way from the right.
    fn hint_row(&self) -> String {
        let mut row = String::new();
        if let Some(notice) = self.notice {
            row.push_str(notice);
        } else if self.queued > 0 {
            row.push_str("queued ");
            row.push_str(&self.queued.to_string());
        }
        if self.escape_armed {
            if !row.is_empty() {
                row.push_str(GUTTER);
            }
            row.push_str(ESCAPE_ARMED);
        }
        row
    }

    /// Where the caret goes: the terminal's own row, and the number of cells to
    /// the left of it on that row.
    pub(crate) fn cursor(&self) -> (u16, u16) {
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

    /// Shows what the runtime just did.
    ///
    /// Exhaustive on purpose: a [`UiEvent`] added later has to be given a home
    /// here rather than falling into a wildcard that drops it -- and a dropped
    /// event is a tool that ran invisibly or a turn that ended without saying
    /// so.
    pub(crate) fn apply(&mut self, event: UiEvent) {
        match event {
            // The answer, as it arrives. A delta that only lengthens the last
            // row rewrites that row where it already is.
            UiEvent::Delta(text) => self.write_transcript(&text),
            // The same two sentences `xfx ask --tool-notices` puts on the
            // diagnostic stream (`output.rs:1154-1174`), so a tool means the
            // same thing on both surfaces.
            UiEvent::ToolStart { tool, .. } => {
                self.write_document_line(&format!("[tool] {tool} running"))
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
                self.write_document_line(&line);
            }
            UiEvent::Notice(text) => self.write_document_line(&text),
            // Unreachable in this phase, and provably so rather than by
            // omission: the runtime thread builds its `PermissionSession` with
            // no prompter at all (`super::worker`), so nothing on the far side
            // of this channel can ask a question. Task 17 attaches the prompter
            // and the panel that answers it.
            UiEvent::Approval(_) => {}
            UiEvent::TurnEnded { failure } => {
                self.finish_document_line();
                if let Some(failure) = failure {
                    self.write_document_line(&failure);
                }
                // Whatever the last Ctrl-C was about ended with this turn. A
                // session that kept remembering it would answer the *next*
                // turn's first Ctrl-C by leaving -- see [`Gestures::turn_ended`].
                self.gestures.turn_ended();
            }
            // Not a row. The band is about to come down, and the message is for
            // a cooked terminal.
            UiEvent::Fatal(message) => {
                self.finish_document_line();
                self.fatal = Some(message);
                self.leave();
            }
        }
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
            match event {
                Input::Text(character) => self.type_character(character),
                Input::Action(action) => self.act(action, now),
                // Task 18's `paste` module is what turns these into text: a
                // pasted byte is not a keystroke, and until there is something
                // that filters it, letting one into the composer would put a
                // control the terminal obeys into the transcript on submit.
                Input::PasteByte(_) => {}
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
        if self.editor.insert(character.encode_utf8(&mut encoded)) {
            self.edited();
        }
    }

    /// What one decoded action means.
    ///
    /// Exhaustive on purpose: an action added later has to be given a home
    /// here rather than falling into a wildcard that silently ignores it.
    fn act(&mut self, action: Action, now: Instant) {
        match action {
            // The composer's own.
            Action::Left
            | Action::Right
            | Action::Up
            | Action::Down
            | Action::Home
            | Action::End
            | Action::WordLeft
            | Action::WordRight
            | Action::Backspace
            | Action::Delete
            | Action::DeleteWordLeft
            | Action::KillToEnd
            | Action::KillToStart
            | Action::InsertNewline => {
                self.editor.apply(action, self.text_cols());
                self.edited();
            }
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
                    self.edited();
                }
            }
            // The whole band is repainted every frame, so a redraw is a frame.
            Action::Redraw => self.render.request(Reason::ExternalDamage),
            // Not this task's: the paste markers are Task 18's, and an `Ignore`
            // is a keystroke this session has no binding for -- an event rather
            // than silence precisely so that it accounts for the bytes it was
            // decoded from.
            Action::PasteStart | Action::PasteEnd | Action::Ignore => {}
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
                self.write_document_line(crate::app::INTERRUPT_NOTICE);
                if waiting {
                    self.write_document_line(QUEUE_DROPPED);
                }
            }
            Interrupt::Clear => self.clear_composer(),
            Interrupt::Leave => self.leave_by(Leaving::Interrupted),
        }
        self.settle_band(now);
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
        self.editor.take();
        self.edited();
    }

    /// What one submitted line is, and what happens to it.
    ///
    /// **Decided by [`crate::interactive::classify`] and by nothing else**, so
    /// the two surfaces cannot disagree about what a leading `/` means. That is
    /// the whole reason the routing is a call rather than a `match` of its own:
    /// a command grammar whose answer depends on which front end you typed it
    /// into is exactly the nondeterminism a command surface must not have. The
    /// names are `interactive::SLASH_COMMANDS`, unchanged and unextended --
    /// this phase adds no command and no slash name.
    ///
    /// Nothing here reaches the provider. Five of the six are answered on this
    /// thread; `/model <id>` and `/new` go to the runtime as
    /// [`TurnWork`] -- not because they need a turn, but because the model and
    /// the conversation they change live there and have to change *between*
    /// turns rather than under one.
    fn submit(&mut self) {
        if self.editor.is_empty() {
            return;
        }
        let text = self.editor.text().to_string();
        match interactive::classify(&text) {
            // Whitespace and nothing else. The line is consumed -- the user
            // pressed Return and a Return that left the composer untouched
            // would look like a session that had stopped listening -- and
            // nothing is sent or written.
            Submitted::Blank => {
                self.editor.take();
                self.edited();
            }
            Submitted::Command { command, argument } => {
                self.echo(&text);
                self.run_command(command, &argument);
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
                self.editor.take();
                self.edited();
                self.echo(&text);
                let refusal = interactive::unknown_command_message(&token);
                self.write_document_line(&refusal);
            }
            Submitted::Prompt(prompt) => self.send(prompt, &text),
        }
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
                self.editor.take();
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
            Rejected::Gone => self.write_document_line(GONE_NOTICE),
        }
    }

    /// Puts a submitted line into the document, where the user can see what
    /// they sent.
    ///
    /// The line ends whether or not the last thing typed was a newline: what
    /// was submitted is finished, and a tail left open would be continued by
    /// the answer.
    fn echo(&mut self, text: &str) {
        self.write_transcript(text);
        self.end_transcript_line();
    }

    /// One of the six, with the rest of the line as its argument.
    ///
    /// The composer is cleared first for all of them: a command is not offered
    /// to anything that can refuse it, so there is no draft to keep.
    fn run_command(&mut self, command: Slash, argument: &str) {
        self.editor.take();
        self.edited();
        self.gestures.submitted();
        match command {
            Slash::Quit => self.leave(),
            Slash::Help => {
                for line in interactive::help_text().lines() {
                    self.write_document_line(line);
                }
            }
            Slash::Version => {
                let line = interactive::version_line();
                self.write_document_line(&line);
            }
            Slash::Model => self.use_model(argument),
            Slash::Clear => self.clear_screen(),
            Slash::New => {
                if let Err(rejected) = self.work.submit(TurnWork::New) {
                    self.refused(rejected);
                    return;
                }
                self.write_document_line(NEW_SESSION_NOTICE);
            }
        }
    }

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
            let line = format!("[shell] model={}", self.model);
            self.write_document_line(&line);
            return;
        }
        if argument == self.model {
            let line = format!("[shell] model={} unchanged", self.model);
            self.write_document_line(&line);
            return;
        }
        if let Err(rejected) = self.work.submit(TurnWork::Model(argument.to_string())) {
            self.refused(rejected);
            return;
        }
        self.model = argument.to_string();
        let line = format!("[shell] model={}", self.model);
        self.write_document_line(&line);
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
        self.clearing = true;
        // Every row of the band is gone from the screen too, so the next frame
        // is a repaint of the whole thing rather than an optional one.
        self.render.request(Reason::ExternalDamage);
        self.write_document_line(CLEARED_NOTICE);
    }

    /// What every change to the composer owes: a frame, and a band the right
    /// height for the text it now holds.
    fn edited(&mut self) {
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
        let limit = layout::input_row_limit(self.geometry.rows);
        let rows = self.editor.rows(self.text_cols()).len();
        let wanted = u16::try_from(rows.clamp(1, usize::from(limit))).unwrap_or(limit);
        if wanted == self.geometry.input_rows() {
            return;
        }
        if let Some(geometry) = layout::solve(self.geometry.rows, self.geometry.cols, wanted) {
            self.geometry = geometry;
            // The divider moved, so every row of the band is somewhere else.
            self.render.request(Reason::Resize);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::BTreeMap;

    use tokio::sync::mpsc::{Receiver, UnboundedReceiver};

    use super::super::bridge::TurnControl;
    use super::super::gesture::EXIT_WINDOW;

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
        _control: UnboundedReceiver<TurnControl>,
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

        /// What the band's last row says, settled first.
        ///
        /// Settled rather than read raw, because the queue's depth is the other
        /// thread's number and the row shows the one this shell last took --
        /// which is the loop's own order (`super::event_loop`).
        fn hint(&mut self) -> String {
            self.shell.settle_band(Instant::now());
            let rows = self.shell.band_rows();
            rows.last().cloned().expect("the band has a hint row")
        }

        /// Plays the runtime taking one piece of work off the channel, which is
        /// where a turn begins and where the one slot becomes free again.
        fn picks_up(&mut self) -> TurnWork {
            self.sent.try_recv().expect("the runtime had work to take")
        }
    }

    fn shell(rows: u16, cols: u16) -> Fixture {
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let (work, sent, _control) = WorkHandle::detached();
        Fixture {
            shell: Shell::new(
                &config(home.path(), workspace.path()),
                crate::tui::layout::solve(rows, cols, 1).expect("a band"),
                work,
            ),
            sent,
            _control,
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
        assert_eq!(rows[0], "\u{2500}".repeat(80), "the divider");
        assert_eq!(rows[1], "> ", "the composer's prompt marker");
        assert_eq!(rows[2], "", "the hint row is owned and empty");
    }

    #[test]
    fn the_divider_spans_the_screen_it_was_solved_for() {
        // A rule of a fixed width would leave a gap on a wide terminal and run
        // off a narrow one -- and with autowrap off, running off is silent.
        for cols in [20u16, 80, 200] {
            let shell = shell(24, cols);
            assert_eq!(shell.band_rows()[0].chars().count(), usize::from(cols));
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
        assert_eq!(shell.hint(), "", "an empty queue was announced");

        shell.route_bytes(b"second\r");

        assert_eq!(shell.hint(), "queued 1");
        assert!(
            shell.editor.is_empty(),
            "a submission that was taken kept the draft"
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
        assert_eq!(shell.hint(), QUEUE_REJECTED);
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
            shell._control.try_recv(),
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
        assert_eq!(shell.hint(), "queued 1");
        let _ = shell.document();

        shell.route_bytes(&[0x03]);

        assert_eq!(
            shell._control.try_recv(),
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
            shell._control.try_recv(),
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
            shell._control.try_recv(),
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
            shell._control.try_recv(),
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
            shell._control.try_recv(),
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
            shell._control.try_recv().is_err(),
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
            ESCAPE_ARMED,
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
        assert_eq!(shell.hint(), "", "the composer-clearing gesture was armed");
    }

    // -----------------------------------------------------------------------
    // the six slash commands
    // -----------------------------------------------------------------------

    #[test]
    fn every_advertised_slash_command_is_answered_without_asking_the_model() {
        // The closed set, driven through the composer one name at a time. What
        // makes this the real claim rather than six spot checks is that it
        // reads `interactive::SLASH_COMMANDS`: a seventh name added there
        // fails here until the TUI answers it too.
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
                .any(|row| row == "[shell] model=second-model"),
            "a bare /model did not report the model in force: {document:?}"
        );
        assert!(
            shell.sent.try_recv().is_err(),
            "a bare /model asked the runtime for something"
        );
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
        assert!(
            !shell.shell.pending.is_empty(),
            "nothing was owed to begin with"
        );

        shell.route_bytes(b"/clear\r");

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
            shell.document().last().map(String::as_str),
            Some("MARKER-TURN-ONE")
        );
    }

    #[test]
    fn a_tool_that_refused_says_so_where_the_user_can_read_it() {
        // The fail-closed half of this phase: `ask` mode has no approval
        // channel in the TUI yet, so a mutation is denied -- and a denial
        // nobody can see is the same as no denial at all.
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
            shell.document(),
            vec![
                "half a sentence".to_string(),
                "[tool] read_file ok".to_string()
            ],
            "a notice was written into the middle of the answer's row"
        );
    }

    #[test]
    fn a_turn_that_failed_says_why_in_the_document() {
        let mut shell = shell(24, 80);
        shell.apply(UiEvent::Delta("part of an answer".to_string()));
        shell.apply(UiEvent::TurnEnded {
            failure: Some("the turn was cancelled".to_string()),
        });

        assert_eq!(
            shell.document(),
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
}
