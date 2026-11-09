//! Terminal bytes in, typed events out.
//!
//! With `ISIG`, `ICANON` and `IEXTEN` cleared there is no line discipline left
//! to assemble a keystroke: what arrives on standard input is bytes, and a
//! `Ctrl-C`, an arrow key and the letter `a` differ only in what those bytes
//! are. This module is the whole of that translation -- one stage machine, fed
//! **one byte at a time**, writing typed [`Input`] events into the caller's
//! buffer.
//!
//! Three invariants carry the design, and each of them is a bug that a simpler
//! decoder has:
//!
//! * **No event amplification.** A stream of *n* bytes never produces more than
//!   *n* events (`input_action.zig:143-160`), so a paste cannot turn a bounded
//!   read into unbounded work. The per-byte form of that rule -- one byte, at
//!   most one event -- holds everywhere except two documented handbacks, and
//!   both are paid for in advance by bytes that produced nothing: the escape
//!   replay below, and the release of a paste-end prefix that turned out to be
//!   content. The end of a paste is not one byte's event at all: it is said
//!   when its delivery is over ([`Decoder::end_delivery`], below), and the six
//!   bytes of the end marker, which produced nothing, pay for it.
//! * **A bare ESC followed by a control byte emits `Escape` *and* replays the
//!   byte** (`terminal_action_decoder.zig:105-114`). No escape sequence carries
//!   a C0 in that position, so the ESC was the user's Escape key and the byte
//!   after it is its own keystroke; swallowing either loses a keystroke the
//!   user really typed. **`ESC 0x7f`, `ESC CR` and `ESC LF` are the
//!   exceptions**: they are how a terminal spells Alt-Backspace and Alt-Enter,
//!   and they decode as the single [`Action::DeleteWordBackward`] the same key
//!   has in its `CSI u` spelling and the single [`Action::InsertNewline`]
//!   Alt-Enter has in its
//!   `CSI u` spelling (`escape_parser.zig` at `c1db919`, :515-519). Replayed,
//!   either key would be an `Escape` as well -- so two of them would be two
//!   Escapes inside the double-Esc window and would clear the whole composer
//!   ([`super::gesture`]), which is the opposite of what a delete-one-word or a
//!   newline key can have meant -- and the Alt-Enter would submit besides.
//! * **An unknown sequence resolves to [`Action::Ignore`], never to a phantom
//!   `Escape`.** A decoder that gives up on `CSI > 4 ; 2 m` by emitting the ESC
//!   it swallowed cancels whatever `Escape` cancels, on input the user did not
//!   type at all. The discard is bounded ([`MOST_BYTES`]) so a terminal that
//!   never sends a final byte cannot park the decoder forever.
//!
//! A lone ESC is the one genuinely ambiguous byte, because it is both a key and
//! the first byte of every sequence. It resolves only after
//! [`Decoder::ESC_TIMEOUT`] of quiet, which is what [`Decoder::flush`] is for:
//! the loop's turn is [`super::event_loop::TICK`] = 8 ms, so an Escape the user
//! pressed is delivered within a tick of the timeout, and a sequence still
//! arriving is never cut in half by the clock. Only the bare ESC is timed --
//! an unfinished `CSI` waits for its final byte however long the terminal
//! takes, because a `CSI` split across two reads is an ordinary arrow key under
//! load.
//!
//! # The control policy
//!
//! **No C0 byte and no control scalar ever leaves here as [`Input::Text`].** A
//! control that reaches the composer reaches the transcript on submit, and a
//! transcript row is written to the terminal as it stands: an `ESC [ 2 J` typed
//! into the band would be *obeyed* rather than shown
//! ([`super::frame::row_text`] strips CR and LF and nothing else). So the table
//! below is closed -- a C0 byte is a **binding** or it is [`Action::Ignore`],
//! and never text. `0x09` is the one worth naming: it is [`Action::Tab`], a
//! binding the approval panel walks its choices with
//! ([`super::approval::Panel`]) and the composer has none for, rather than the
//! tab character it would otherwise become. Naming it changes nothing about the
//! policy -- what the policy forbids is a control reaching the composer as
//! *text*, and an action never does. The same rule covers the C1 range, which
//! arrives as perfectly valid UTF-8: `U+0085` and `U+009B` are control scalars
//! and are ignored, not typed.
//!
//! That is the first half of the policy the ledger records. The second half is
//! an allowlist at the render layer, and it belongs there rather than here,
//! because the sequences it admits are written *by* the session into a
//! transcript row and never arrive through this door at all.
//!
//! # Paste is data, not keys
//!
//! Between `CSI 200 ~` and `CSI 201 ~` every byte is content
//! (`terminal_action_decoder.zig:38-40`): it comes out as [`Input::PasteByte`]
//! and *nothing* in a paste is decoded, so a pasted `Ctrl-C` cannot cancel a
//! turn and a pasted `ESC [ A` cannot move the caret. The contract that goes
//! with that is the consumer's, and it is not optional: **a `PasteByte` is an
//! uninterpreted byte, so whatever turns a run of them into text is what must
//! refuse the controls.** What this module guarantees is only that the byte
//! arrives as `PasteByte(0x1b)` rather than as `Action::Escape` -- data, not a
//! key -- and that the guarantee holds for every byte between the markers.
//!
//! Both markers are recognized by the **exact bytes** of their parameters
//! rather than by a number those bytes parse to. The end marker has to be
//! matched byte by byte anyway -- it arrives one byte at a time -- so a start
//! marker matched loosely would be a start no end could pair with: `CSI 0200 ~`
//! and `CSI 200 ; 1 ~` parse as 200 and open a paste that only the exact
//! `CSI 201 ~` closes, which latches the session into paste mode on a stream
//! the terminal never framed. `csi` therefore compares raw parameter bytes, and
//! the same strictness covers every other sequence it reads: the numbers in a
//! key report are read only from their one canonical spelling, so no two
//! spellings ever reach the same key.
//!
//! The end marker is matched byte by byte, so its prefix has to be held back:
//! typing `ESC`, `[`, `2` into the composer as they arrive would be wrong when
//! the `0 1 ~` follows. A prefix that breaks was content after all and is
//! released, in order, ahead of the byte that broke it. At most five bytes are
//! ever held, and a paste the terminal abandons without an end marker keeps
//! them -- a bounded loss inside a stream that is already malformed, and the
//! alternative, releasing them on a timer, breaks every paste whose marker
//! lands across two reads.
//!
//! # A paste ends with its delivery
//!
//! **Input that arrives behind a paste's end marker in the same delivery is
//! taken for the clipboard's, not the user's**, and it is not read as keys. A
//! *delivery* is everything the terminal had written by the time the loop
//! found nothing more to read -- one read or many, since a read takes at most
//! `super::event_loop::BURST_BYTES`, and across the 32-read cap of one turn --
//! which is upstream's delivery epoch: it settles a paste only once a
//! readiness check finds the input drained (`event_loop.zig:94-119
//! @481104fd`). A person cannot press a key inside that window; a clipboard
//! that carries `ESC [ 201 ~` followed by a newline can, and without this rule
//! the newline would submit the composer as if typed.
//!
//! **The window is the terminal's, not the clipboard's.** A terminal that
//! writes one paste with a moment between two of its writes -- a paste longer
//! than the pty's input queue arrives as the terminal refills it -- ends the
//! delivery at that moment, and the bytes after it are read as keys. That is
//! fx v0.0.16's boundary too, and it was xfx's before this unit. A
//! terminal that strips an end marker out of pasted text closes the gap where
//! it opens (`docs/parity.md`, the `full-screen TUI` row).
//!
//! So the marker does not end the paste by itself. Once it has matched, every
//! further byte is counted and **nothing else is done with it** -- not decoded,
//! not a key, not a second marker, not a new paste's start -- until the caller
//! says the delivery is over ([`Decoder::end_delivery`]). Then the paste
//! stands, whatever followed it, and the caller learns how many bytes did
//! ([`Delivery::Pasted`], which is [`Action::PasteEnd`]; fx v0.0.16's
//! `Settlement.finish{dropped_suffix_bytes}`, `paste_framing.zig:58-68,
//! 102-107,159-165@481104fd`). What it says about those bytes, and to which
//! surface, is the caller's (`input_paste_runtime.zig:168-198`). A marker
//! split across two reads is not input behind the marker -- it is the marker
//! -- and a key in a later delivery is a key.
//!
//! fx c1db919 refused the whole paste instead (`Settlement.reject`); v0.0.16
//! keeps it (9e0eab41), and so does this.
//!
//! # Feeding it
//!
//! [`Decoder::feed`] takes the bytes in arrival order and no others. The launch
//! probe's [`super::probe::CursorProbe::take_deferred`] bytes were typed on a
//! terminal that was already raw and no second read will produce them, so they
//! are the loop's *first* bytes: they go through `feed`, in order, before the
//! first wait, exactly as bytes read afterwards do. Wiring that is the caller's
//! -- the task that routes input into the editor owns `event_loop::run` and the
//! decoder it lives in -- and this module's half of the obligation is that
//! `feed` is a byte-at-a-time sink with no notion of which read a byte came
//! off. The caller has two things to get right: the order, and **when a
//! delivery is over**, which only the loop's readiness check knows.

use std::time::{Duration, Instant};

/// What a keystroke means to the composer and the session.
///
/// One flat vocabulary rather than a key plus modifiers: the decoder is the
/// only thing that knows `ESC [ 1 ; 5 C` from `ESC [ C`, and an editor that had
/// to re-derive "this one is a word move" would be a second place for the two
/// spellings to disagree.
// Task 9's editor is the first consumer of these.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    Left,
    Right,
    Up,
    Down,
    /// Up and Down with Shift held: `CSI 1;<mask> A/B` and the kitty arrow
    /// reports with a mask that has Shift in it.
    ///
    /// Upstream reads them as a selection move (`extend_selection`), which
    /// only a question's freeform field gives a meaning of its own -- the
    /// choice above or below, where the plain arrows walk the field's rows
    /// (`src/ui/input/runtime.zig:175-188`, `input_question_runtime.zig:150-185`,
    /// `c1db919`). Everywhere else they are the arrows they always were here:
    /// the shell folds them back before any other surface sees them
    /// (`super::shell::Shell`'s `folded`).
    SelectUp,
    SelectDown,
    Home,
    End,
    WordLeft,
    /// Alt-b, in each of its spellings: `ESC b`, kitty's `CSI 98;3u` and
    /// xterm's `CSI 27;3;98~`.
    ///
    /// Upstream decodes these to its own `.word_left` action, and the
    /// modified arrows -- Ctrl-Left, Alt-Left -- to a composer move instead
    /// (`escape_parser.zig:83-92`, `:243-246`, `:528-533@481104fd`). Only a
    /// question at a choice tells the two apart: `.word_left` is the previous
    /// question there, as Left is, and the arrows do nothing
    /// (`input_question_runtime.zig:190-199@481104fd`). Everywhere else this is
    /// [`Action::WordLeft`]: the shell folds it back before any other surface
    /// sees it (`super::shell::Shell`'s `folded`), as it does
    /// [`Action::SelectUp`].
    AltB,
    WordRight,
    Backspace,
    Delete,
    DeleteWordLeft,
    /// Alt-Backspace: the word to the left by **word characters** -- letters,
    /// digits, `_` and everything from `U+00C0` up -- and what separates it
    /// from the caret (`delete_word_left`, `wordLeftRange`,
    /// `composer_deletion.zig:103-136`). A deletion, not a kill. Not
    /// [`Action::DeleteWordLeft`], which is `C-w`'s whitespace word and goes
    /// to the kill slot (`delete_whitespace_word_left`, `shortcuts.zig:23`).
    DeleteWordBackward,
    /// The word to the right of the caret and the gap after it: Alt-d, and
    /// Delete with Alt or Ctrl (`escape_parser.zig` at `c1db919`, :175-180,
    /// :251, :549). A deletion, not a kill (`composer_deletion.zig:138-164`).
    DeleteWordRight,
    KillToEnd,
    KillToStart,
    Submit,
    InsertNewline,
    /// `C-j` (`0x0a`): a newline in the composer, exactly as
    /// [`Action::InsertNewline`] -- and the next row of a menu while one is up
    /// (`app_input_runtime.zig:1220-1235`, `routePickerControlByte`).
    ///
    /// Named for the key rather than folded into `InsertNewline`, because
    /// upstream binds the menu move to the **byte**: Alt-Enter and Shift-Enter
    /// are newlines everywhere, and only the key that is `0x0a` walks a menu.
    ControlJ,
    /// `C-k` (`0x0b`): [`Action::KillToEnd`] in the composer, and the row
    /// above in a menu while one is up -- the other half of `ControlJ`, for the
    /// same reason (Super-Delete kills to the end and walks no menu).
    ControlK,
    /// Page Up: `CSI 5 ~`, kitty's keypad Page Up, and either with a modifier
    /// (`escape_parser.zig` at `c1db919`, :169-173, :632, :708).
    ///
    /// One key, routed by the surface it lands on: the full transcript pages
    /// itself (`keyForAction`, `input_full_transcript_runtime.zig:156-179`),
    /// a menu moves its mark a page of the composer's row cap, and the
    /// composer moves the caret that far (`routeComposerVertical` with `page`,
    /// `input_completion_runtime.zig:209-262`) -- never a history step.
    PageUp,
    /// Page Down, the mirror of [`Action::PageUp`].
    PageDown,
    /// The tab key. Bound by the approval panel, where it cycles the marked
    /// choice, and by the slash completion menu, where it takes the marked
    /// command (`super::picker`). With neither of those up it still does
    /// nothing: the composer has no completion of its own for it to drive, and
    /// a tab typed at a bare prompt is a keystroke this session has no binding
    /// for rather than a character.
    Tab,
    /// Shift-Tab: `CSI Z`, `CSI 1;<modifiers> Z`, and kitty's `CSI 9;2u`.
    ///
    /// Upstream decodes all three to one action, `toggle_permission_mode`
    /// (`escape_parser.zig` at `c1db919`, :254, :569, :723-725), and lets an
    /// open catalog have it first: the `/help` catalog's previous tab
    /// (`app_input_runtime.zig:1175-1177`, `super::help_menu`). Named for the
    /// key rather than for that fallback because xfx has no permission-mode
    /// toggle -- with no catalog up it does nothing, as [`Action::Tab`] does.
    BackTab,
    /// `C-p` (`0x10`, and kitty's Ctrl+P): the model catalog at the composer
    /// (`open_model_catalog`, `escape_parser.zig` at `c1db919`, :271), and
    /// the choice above / a review walked back at a permission question.
    ///
    /// Named for the key, as [`Action::ControlJ`] is, because what it does is
    /// the surface's to say. It is **not** the recall any more (ssot
    /// divergence 8): the line before this one is the `Up` arrow's, at the
    /// first row of the draft.
    ControlP,
    /// The line after it, and the draft the walk began from once there are no
    /// more. `C-n`, and `Down` at the last row.
    HistoryNext,
    /// Take the last edit back. `C-_` (`0x1f`), and Super+Z on a terminal that
    /// speaks either of the two CSI-u families (`shortcuts.zig:23-24`,
    /// `escape_parser.zig:122-123`).
    Undo,
    /// Put it back again.
    ///
    /// **No control byte**, and none may be invented: `shortcuts.zig`'s control
    /// table has no redo arm, so the only spellings are the two Super+Shift+Z
    /// sequences (`runtime.zig:3018,3025`). `C-z` (`0x1a`) is not this key --
    /// upstream binds nothing to it and neither does this decoder.
    Redo,
    /// The full transcript: open it, or close it again. `C-o` (`0x0f`), and
    /// kitty's Ctrl+O report (`escape_parser.zig` at `c1db919`, :111-114,
    /// :270).
    ToggleTranscript,
    /// Insert what was last killed. `C-y` (`0x19`, `shortcuts.zig:84-85`).
    Yank,
    /// Attach the clipboard's image at the caret. `C-v` (`0x16`), as
    /// upstream's composer binds it (`app_input_runtime.zig:1561-1567`, at
    /// `c1db919`): the same read `/paste` makes.
    PasteImage,
    /// Select the whole draft: Super+A on a terminal that reports Super
    /// (`select_all`, `escape_parser.zig` at `c1db919`, :223-225).
    SelectAll,
    /// Put the selection on the clipboard: Super+C (`copy_selection`,
    /// :226-228).
    CopySelection,
    /// Put the selection on the clipboard and take it out of the draft:
    /// Super+X (`cut_selection`, :229-231).
    CutSelection,
    Escape,
    Cancel,
    Eof,
    Redraw,
    PasteStart,
    /// A paste that stands. Not the end marker's own event: a paste's end is
    /// said when its delivery is over ([`Decoder::end_delivery`]), whatever
    /// followed the marker in it -- what did is counted, never an event.
    PasteEnd,
    /// Something well-formed that means nothing here.
    ///
    /// An event rather than silence, because "the terminal sent a sequence this
    /// session has no binding for" and "the terminal sent nothing" are
    /// different facts, and only the first one accounts for the bytes that were
    /// read.
    Ignore,
}

/// One decoded event.
// Task 9's editor is the first consumer.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Input {
    /// A character the user typed. Never a control scalar -- see the module's
    /// control policy.
    Text(char),
    Action(Action),
    /// One uninterpreted byte from between the bracketed-paste markers.
    PasteByte(u8),
    /// The terminal saying which way round its background is now
    /// ([`super::theme::notification`]).
    ///
    /// **Not a key, and that is the whole of why it is a variant rather than an
    /// [`Action`].** Nobody typed it: it arrives because mode 2031 was set
    /// ([`super::term::MODE_SET`]) or because the session asked
    /// ([`super::theme::MODE_QUERY`]), and what it asks for is a repaint rather
    /// than an edit. Routed as an action it would have to be given a meaning at
    /// every surface that binds keys -- and swallowed at the two that swallow
    /// everything they do not bind, which is precisely where a live theme
    /// change would stop working.
    Report(super::theme::Mode),
}

/// The longest a sequence may be before the decoder stops trying to understand
/// it (`escape_parser.zig:37,234-278`).
///
/// A bound rather than a buffer that grows: every sequence this session reads
/// fits in a handful of bytes, and a terminal emitting more than this is
/// emitting something else. What matters as much is that the giving-up path is
/// bounded *too*, so the bytes after it are keystrokes again rather than input
/// the decoder eats for as long as it keeps arriving.
const MOST_BYTES: usize = 32;

/// The same bound for a **string** sequence, which is a different shape and
/// therefore a different number.
///
/// An `OSC` carries text rather than a handful of parameters, and the longest
/// one a terminal volunteers is a clipboard report (`OSC 52`), whose body is
/// base64 of a selection somebody made. A bound sized like [`MOST_BYTES`] would
/// turn containment into a leak -- the head of the report would be swallowed
/// and its tail typed into the composer -- so this is sized past any report a
/// terminal really sends and still finite, because a terminal that opens a
/// string and never closes it must not own the keyboard for the rest of the
/// session. Two things keep that bound from mattering in practice: a C0 byte
/// ends the string at once, so every binding a user could reach for -- Ctrl-C,
/// Return, Backspace -- is answered while one is open, and an `ESC` that is not
/// the first half of an `ST` ends it too, so an arrow key does the same.
const MOST_STRING_BYTES: usize = 4096;

/// The end of a bracketed paste, matched byte by byte.
const PASTE_END: &[u8] = b"\x1b[201~";

/// A multi-byte UTF-8 scalar, partly assembled.
#[derive(Debug, Clone, Copy)]
struct Partial {
    buf: [u8; 4],
    /// How many bytes are in hand.
    len: usize,
    /// How many *continuation* bytes the leading byte promised.
    need: usize,
}

/// Where in a sequence the machine is.
#[derive(Debug, Clone, Copy)]
enum Stage {
    /// Between sequences: the next byte is a character, a binding, or an ESC.
    Ground,
    /// An ESC with nothing after it yet, and when it arrived.
    Esc { at: Instant },
    /// `ESC [`, reading parameters into [`Decoder::params`].
    Csi,
    /// `ESC O`, waiting for the one byte that names the key.
    Ss3,
    /// `ESC ]`, inside a string sequence's body: `seen` bytes of it so far, and
    /// `esc` for an `ESC` that may be the first half of the `ST` that ends it.
    ///
    /// The same two states `super::probe` keeps for the one `OSC` a launch
    /// recognizes, and for the same reason: a string ends at a `BEL` or at
    /// `ESC \`, so an `ESC` inside one cannot be decided until the byte after
    /// it arrives.
    Osc { seen: usize, esc: bool },
    /// A sequence that outgrew [`MOST_BYTES`]: its bytes go nowhere until a
    /// final byte arrives or this counter reaches the same bound.
    Discard { seen: usize },
    /// A scalar the leading byte of which has arrived.
    Utf8(Partial),
}

/// What the end of a delivery made of a paste ([`Decoder::end_delivery`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Delivery {
    /// No paste ended in it: there was none, or the one that is open goes on
    /// into the next delivery.
    Quiet,
    /// A paste ended: it stands, and [`Action::PasteEnd`] is owed to whatever
    /// gathered it. `dropped` more bytes followed its end marker in the same
    /// delivery, and none of them was read as anything
    /// (`Settlement.finish{dropped_suffix_bytes}`, `paste_framing.zig:58-68,
    /// 159-165@481104fd`).
    Pasted { dropped: usize },
}

/// The byte-at-a-time terminal input decoder.
// Task 9's editor is the first caller.
#[allow(dead_code)]
pub(crate) struct Decoder {
    stage: Stage,
    /// The parameter and intermediate bytes of the `CSI` being read, without
    /// its final byte.
    params: Vec<u8>,
    pasting: bool,
    /// How many bytes of [`PASTE_END`] have matched so far. The bytes
    /// themselves are known, so the count is the whole of the held prefix.
    matched: usize,
    /// How many bytes have arrived behind a paste's end marker in the delivery
    /// that carried it: `Some(0)` the moment the marker completes, and `None`
    /// again once that delivery is over ([`Self::end_delivery`]). While it is
    /// `Some`, a byte is counted here and nothing else is done with it
    /// (`end_candidate`'s `unsafe_suffix_bytes`, `paste_framing.zig:102-107
    /// @481104fd`).
    behind: Option<usize>,
}

// Task 9's editor is the first caller of these.
#[allow(dead_code)]
impl Decoder {
    /// How long a bare ESC waits for the sequence it might have started.
    ///
    /// Long enough for the rest of an arrow key to arrive through a
    /// multiplexer, short enough that pressing Escape feels like pressing a key
    /// rather than waiting for one.
    pub(crate) const ESC_TIMEOUT: Duration = Duration::from_millis(50);

    pub(crate) fn new() -> Self {
        Self {
            stage: Stage::Ground,
            params: Vec::new(),
            pasting: false,
            matched: 0,
            behind: None,
        }
    }

    /// Whether a paste owns the input: between its markers, and behind its
    /// end marker until the delivery that carried the marker is over.
    pub(crate) fn pasting(&self) -> bool {
        self.pasting || self.behind.is_some()
    }

    /// The delivery the bytes fed so far arrived in is over: the terminal had
    /// nothing more to read. Settles a paste whose end marker arrived in it
    /// (`settleDeliveryEpoch`, `paste_framing.zig:159-165@481104fd`).
    ///
    /// Only the caller knows when that is -- the loop's readiness check
    /// (`super::event_loop`'s `burst`) -- and it is said once per drained
    /// burst, not once per read: a paste larger than one read is one delivery,
    /// and so is its end marker and an Enter one read behind it.
    pub(crate) fn end_delivery(&mut self) -> Delivery {
        match self.behind.take() {
            None => Delivery::Quiet,
            Some(dropped) => Delivery::Pasted { dropped },
        }
    }

    /// Feeds one byte, in arrival order, appending whatever it completed.
    pub(crate) fn feed(&mut self, byte: u8, now: Instant, out: &mut Vec<Input>) {
        // Behind a paste's end marker, in the delivery that carried it: the
        // byte is counted, and that is all. Not decoded, so not a key -- an
        // Enter here is taken for the clipboard's, not the user's -- and not a marker
        // either: a second end marker or a new paste's start is input behind
        // this one's like any other (`paste_framing.zig:102-107,527-546`,
        // `app_input_runtime.zig:9399-9418@481104fd`).
        if let Some(behind) = self.behind.as_mut() {
            *behind = behind.saturating_add(1);
            return;
        }
        // First, and before any stage is consulted: inside a paste there is
        // nothing to decode but the end marker.
        if self.pasting {
            self.pasted(byte, out);
            return;
        }
        // A bare ESC that has gone quiet is the Escape key, whatever arrives
        // next. The loop flushes every tick so this is normally settled
        // already; it is here so that a caller which feeds a burst without
        // flushing still cannot read one keystroke as part of another.
        self.flush(now, out);
        match self.stage {
            Stage::Ground => self.ground(byte, now, out),
            Stage::Esc { .. } => self.after_escape(byte, now, out),
            Stage::Csi => self.in_csi(byte, now, out),
            Stage::Ss3 if carriable(byte) => {
                self.stage = Stage::Ground;
                out.push(Input::Action(ss3(byte)));
            }
            // A byte no sequence can carry: the sequence is abandoned and the
            // byte is decoded on its own. `in_csi` and `discarding` make the
            // same check for themselves.
            Stage::Ss3 => self.intruded(byte, now, out),
            Stage::Osc { seen, esc } => self.in_osc(byte, now, seen, esc, out),
            Stage::Discard { seen } => self.discarding(byte, now, seen, out),
            Stage::Utf8(partial) => self.in_utf8(byte, now, partial, out),
        }
    }

    /// Resolves what only the passage of time can resolve.
    ///
    /// Exactly one thing: a bare ESC older than
    /// [`ESC_TIMEOUT`](Self::ESC_TIMEOUT) is the Escape key. An unfinished
    /// `CSI` is deliberately **not** resolved here -- it is a sequence still
    /// arriving, and a clock that cut it in half would turn an arrow key under
    /// load into two events the user never typed.
    pub(crate) fn flush(&mut self, now: Instant, out: &mut Vec<Input>) {
        if let Stage::Esc { at } = self.stage {
            if now.saturating_duration_since(at) >= Self::ESC_TIMEOUT {
                self.stage = Stage::Ground;
                out.push(Input::Action(Action::Escape));
            }
        }
    }

    /// One byte between the paste markers.
    fn pasted(&mut self, byte: u8, out: &mut Vec<Input>) {
        if byte == PASTE_END[self.matched] {
            self.matched += 1;
            if self.matched == PASTE_END.len() {
                // The marker, but not yet the paste's end: that is the
                // delivery's to say ([`Self::end_delivery`]), once it is known
                // whether anything followed.
                self.matched = 0;
                self.pasting = false;
                self.behind = Some(0);
            }
            return;
        }
        // The prefix broke, so what was held back was content. It goes out
        // first, in order, and the byte that broke it may open a prefix of its
        // own -- pasted text carrying `ESC [ 2` immediately before the real end
        // marker is the case that needs this.
        for held in PASTE_END.iter().take(self.matched) {
            out.push(Input::PasteByte(*held));
        }
        if byte == PASTE_END[0] {
            self.matched = 1;
        } else {
            self.matched = 0;
            out.push(Input::PasteByte(byte));
        }
    }

    /// One byte with no sequence in progress.
    fn ground(&mut self, byte: u8, now: Instant, out: &mut Vec<Input>) {
        match byte {
            0x1b => self.stage = Stage::Esc { at: now },
            // The closed control table. Everything it does not name is
            // `Ignore`, which is what keeps a control out of the text.
            0x00..=0x1f | 0x7f => out.push(Input::Action(control(byte))),
            0x20..=0x7e => out.push(Input::Text(byte as char)),
            // A leading byte promises its continuations. Anything else up here
            // -- a stray continuation, an overlong lead, a byte no scalar
            // starts with -- is dropped rather than shown as a replacement
            // character the user did not type.
            0xc2..=0xf4 => {
                let need = match byte {
                    0xc2..=0xdf => 1,
                    0xe0..=0xef => 2,
                    _ => 3,
                };
                let mut buf = [0u8; 4];
                buf[0] = byte;
                self.stage = Stage::Utf8(Partial { buf, len: 1, need });
            }
            _ => {}
        }
    }

    /// The byte after a bare ESC.
    fn after_escape(&mut self, byte: u8, now: Instant, out: &mut Vec<Input>) {
        match byte {
            b'[' => {
                self.params.clear();
                self.stage = Stage::Csi;
            }
            b'O' => self.stage = Stage::Ss3,
            // A string sequence -- `OSC` -- which is the terminal talking about
            // itself rather than a key: a background report it was asked for,
            // a foreground one, a clipboard. Its body is *text*, and a decoder
            // that stopped here would put `11;rgb:1e1e/1e1e/1e1e` in the
            // composer and the user's clipboard with it. It is consumed to its
            // terminator instead ([`Self::in_osc`]).
            b']' => {
                self.stage = Stage::Osc {
                    seen: 0,
                    esc: false,
                }
            }
            // Alt-Backspace, which every terminal in use spells this way, and
            // the one exception to the replay below. Replayed it would be
            // `Escape` + `Backspace`, so two Alt-Backspaces would be two
            // Escapes inside the double-Esc window and the gesture that throws
            // the whole draft away would fire on a keystroke that means delete
            // *one word* and keep the rest (`super::gesture`). One keystroke,
            // one action: upstream's `delete_word_left` (:547-552), which
            // is not Ctrl-W's whitespace kill.
            0x7f | 0x08 => {
                self.stage = Stage::Ground;
                out.push(Input::Action(Action::DeleteWordBackward));
            }
            // Alt-Enter, in the spelling a terminal without a key protocol
            // uses (`escape_parser.zig` at `c1db919`, :515-519), and the second
            // exception for the same reason as the first: replayed it would be
            // `Escape` + `Submit`, a prompt sent on the key that means "a new
            // line, not yet" -- and two of them the double-Esc gesture as well.
            // One keystroke, the action its `CSI 13 ; 3 u` spelling has.
            b'\r' | b'\n' => {
                self.stage = Stage::Ground;
                out.push(Input::Action(Action::InsertNewline));
            }
            // No sequence carries a C0 in this position, so the ESC was the
            // Escape key and this byte is its own keystroke. Both of them, in
            // that order.
            0x00..=0x1f => {
                self.stage = Stage::Ground;
                out.push(Input::Action(Action::Escape));
                self.ground(byte, now, out);
            }
            // Alt-b, Alt-f and Alt-d in the spelling a terminal without a key
            // protocol uses (`escape_parser.zig` at `c1db919`, :529-546): the
            // word moves the modified arrows already make, and the forward
            // word delete. Lower case only, as upstream reads them. Alt-b is
            // its own action, which only a question's choice reads differently
            // from the arrows ([`Action::AltB`]).
            b'b' => {
                self.stage = Stage::Ground;
                out.push(Input::Action(Action::AltB));
            }
            b'f' => {
                self.stage = Stage::Ground;
                out.push(Input::Action(Action::WordRight));
            }
            b'd' => {
                self.stage = Stage::Ground;
                out.push(Input::Action(Action::DeleteWordRight));
            }
            // Any other `ESC` + a printable byte is an Alt key this session
            // binds nothing to. Ignored rather than replayed: replaying would
            // put the character in the composer *and* fire whatever `Escape`
            // fires, which is the one thing the user cannot have meant by a
            // single keystroke.
            _ => {
                self.stage = Stage::Ground;
                out.push(Input::Action(Action::Ignore));
            }
        }
    }

    /// One byte inside `ESC [ ...`.
    fn in_csi(&mut self, byte: u8, now: Instant, out: &mut Vec<Input>) {
        if !carriable(byte) {
            self.intruded(byte, now, out);
            return;
        }
        if self.params.len() >= MOST_BYTES {
            self.discarding(byte, now, 0, out);
            return;
        }
        match byte {
            // Parameter and intermediate bytes.
            0x20..=0x3f => self.params.push(byte),
            // `carriable` has already bounded the byte to `0x20..=0x7e`, so
            // everything left is `0x40..=0x7e`: the final byte, which is what
            // the sequence means.
            _ => {
                self.stage = Stage::Ground;
                // Before the key table, because this is not a key. A terminal
                // that was asked to report background changes
                // ([`super::term::MODE_SET`]) sends `CSI ? 997 ; N n` on its
                // own, and the parameters are matched exactly
                // ([`super::theme::notification`]) -- anything else with this
                // final byte falls through and becomes the
                // [`Action::Ignore`] every unbound sequence becomes, which is
                // what keeps the keystroke behind it a keystroke.
                if byte == b'n' {
                    if let Some(mode) = super::theme::notification(&self.params) {
                        out.push(Input::Report(mode));
                        return;
                    }
                }
                let input = csi(&self.params, byte);
                if input == Input::Action(Action::PasteStart) {
                    self.pasting = true;
                    self.matched = 0;
                }
                out.push(input);
            }
        }
    }

    /// One byte inside `ESC ] ...`.
    ///
    /// The string is consumed whole and reported as one [`Action::Ignore`] --
    /// the same answer a well-formed sequence with no binding gets, for the
    /// same reason: "the terminal sent a report" and "the terminal sent
    /// nothing" are different facts, and only the first accounts for the bytes
    /// that were read. **Nothing of the body is ever emitted**, which is the
    /// whole point: an `OSC 52` reply carries a clipboard and an `OSC 11` reply
    /// carries a colour, and neither is something the user typed.
    ///
    /// Three things end one, and the last two are why a report cannot cost a
    /// keystroke:
    ///
    /// * a `BEL`, or the `ESC \` of an `ST` -- the two terminators a terminal
    ///   really writes;
    /// * an `ESC` that turns out **not** to be an `ST`, which ends the string;
    ///   what that `ESC` then is depends on the byte behind it -- an introducer
    ///   when a sequence can begin with that byte, so the arrow key behind a
    ///   truncated report is still an arrow key, and **nothing at all** when it
    ///   is a C0 or `DEL`, because an Escape synthesized there is a keystroke
    ///   nobody made and two of them empty the composer;
    /// * any other C0 byte or `DEL`, which no string carries -- so a Ctrl-C
    ///   pressed while a report is open cancels the turn instead of being eaten
    ///   by it ([`Self::intruded`], which is the same rule a `CSI` has).
    ///
    /// And [`MOST_STRING_BYTES`] bounds the rest, so a terminal that never
    /// closes one gives the keyboard back.
    fn in_osc(&mut self, byte: u8, now: Instant, seen: usize, esc: bool, out: &mut Vec<Input>) {
        if esc {
            if byte == b'\\' {
                self.stage = Stage::Ground;
                out.push(Input::Action(Action::Ignore));
                return;
            }
            // The `ESC` was not the first half of an `ST`. The string is over,
            // and what that `ESC` is depends entirely on the byte behind it.
            out.push(Input::Action(Action::Ignore));
            // Anything a real escape sequence can carry in this position -- a
            // `[`, an `O`, another `]`, or the printable byte of an Alt-key:
            // the `ESC` is that sequence's introducer and the two are decoded
            // together, because handing back only the byte would put a bare `[`
            // in the composer where a cursor key was. A byte above `DEL` goes
            // here too, and deliberately: `ESC` + a scalar's lead byte is
            // Alt-something, which this phase ignores rather than typing, and
            // the tail of a report the terminal truncated is not text the user
            // asked for.
            if !matches!(byte, 0x00..=0x1f | 0x7f) {
                self.stage = Stage::Esc { at: now };
                self.after_escape(byte, now, out);
                return;
            }
            // A C0 or `DEL`. **No `ESC` is handed back here**, and that is the
            // whole of the difference: [`Self::after_escape`] answers "an `ESC`
            // with a control behind it" with *the Escape key and then that
            // control*, which is right for a key the user pressed and wrong for
            // this one -- the `ESC` came out of a report xfx asked for, and
            // nothing in the stream is evidence anybody pressed Escape.
            //
            // The synthesized press is not cosmetic. Two Escapes inside
            // `super::gesture::CLEAR_WINDOW` are the gesture that empties the
            // composer, so `OSC ... ESC ESC` -- a report truncated by a real
            // Escape key -- would count as both presses and throw the draft
            // away; and `OSC ... ESC` + Ctrl-C would arm that gesture on the
            // way to cancelling the turn. So the string ends and the byte is
            // decoded as the keystroke it is, on its own.
            self.stage = Stage::Ground;
            self.ground(byte, now, out);
            return;
        }
        match byte {
            0x07 => {
                self.stage = Stage::Ground;
                out.push(Input::Action(Action::Ignore));
            }
            0x1b => {
                self.stage = Stage::Osc {
                    seen: seen.saturating_add(1),
                    esc: true,
                };
            }
            0x00..=0x1f | 0x7f => self.intruded(byte, now, out),
            _ if seen >= MOST_STRING_BYTES => self.intruded(byte, now, out),
            _ => {
                self.stage = Stage::Osc {
                    seen: seen.saturating_add(1),
                    esc: false,
                };
            }
        }
    }

    /// One byte of a sequence that outgrew its bound.
    fn discarding(&mut self, byte: u8, now: Instant, seen: usize, out: &mut Vec<Input>) {
        if !carriable(byte) {
            self.intruded(byte, now, out);
            return;
        }
        let seen = seen + 1;
        if (0x40..=0x7e).contains(&byte) || seen >= MOST_BYTES {
            self.stage = Stage::Ground;
            out.push(Input::Action(Action::Ignore));
        } else {
            self.stage = Stage::Discard { seen };
        }
    }

    /// Abandons the sequence in progress, because `byte` cannot be part of one,
    /// and decodes that byte on its own.
    ///
    /// The sequence is [`Action::Ignore`] -- it is unknown, and emitting the
    /// `ESC` it swallowed would be the phantom escape again -- and the byte is
    /// whatever it means in [`Stage::Ground`]. Eating it with the sequence
    /// would lose a keystroke the user really typed, which is the same defect
    /// the escape replay exists to prevent.
    fn intruded(&mut self, byte: u8, now: Instant, out: &mut Vec<Input>) {
        self.stage = Stage::Ground;
        out.push(Input::Action(Action::Ignore));
        self.ground(byte, now, out);
    }

    /// One byte of a multi-byte scalar.
    fn in_utf8(&mut self, byte: u8, now: Instant, partial: Partial, out: &mut Vec<Input>) {
        let Partial { mut buf, len, need } = partial;
        if !(0x80..=0xbf).contains(&byte) {
            // The scalar was truncated. Its bytes are dropped -- there is no
            // character in them to show -- but this byte is a fresh keystroke
            // and is decoded as one rather than eaten with them.
            self.stage = Stage::Ground;
            self.ground(byte, now, out);
            return;
        }
        buf[len] = byte;
        let len = len + 1;
        if len <= need {
            self.stage = Stage::Utf8(Partial { buf, len, need });
            return;
        }
        self.stage = Stage::Ground;
        // `from_utf8` rather than assembling the scalar by hand: it is what
        // rejects an overlong encoding and a surrogate half, both of which are
        // byte sequences a terminal can send and neither of which is a
        // character.
        let Ok(text) = std::str::from_utf8(&buf[..len]) else {
            return;
        };
        let Some(scalar) = text.chars().next() else {
            return;
        };
        out.push(if scalar.is_control() {
            // The C1 controls are spelled in valid UTF-8 and a terminal obeys
            // some of them. They are keystrokes with no binding, not text.
            Input::Action(Action::Ignore)
        } else {
            Input::Text(scalar)
        });
    }
}

/// Whether an escape sequence can carry this byte at all.
///
/// Everything between a `CSI` and its final byte is printable ASCII: the
/// parameter, intermediate and final ranges tile `0x20..=0x7e` and nothing
/// else. A C0, a DEL or a high byte in the middle of a sequence is therefore
/// not part of it -- it is a keystroke that arrived while a malformed sequence
/// was still open, and it is decoded rather than eaten (see
/// [`Decoder::intruded`]).
fn carriable(byte: u8) -> bool {
    (0x20..=0x7e).contains(&byte)
}

/// What one C0 byte or DEL means (`shortcuts.zig:11-30`).
///
/// Closed on purpose: a byte with no row here is [`Action::Ignore`] and never a
/// character. `0x09` is the one worth naming -- a tab that became *text* would
/// be a control in the composer, then in the transcript, and finally on the
/// wire back to the terminal, so it is a named action instead
/// ([`Action::Tab`]).
fn control(byte: u8) -> Action {
    match byte {
        0x01 => Action::Home,
        0x02 => Action::Left,
        0x03 => Action::Cancel,
        0x04 => Action::Eof,
        0x05 => Action::End,
        0x06 => Action::Right,
        0x08 | 0x7f => Action::Backspace,
        0x09 => Action::Tab,
        0x0a => Action::ControlJ,
        0x0b => Action::ControlK,
        0x0c => Action::Redraw,
        0x0d => Action::Submit,
        0x0e => Action::HistoryNext,
        // `controlByteFeatureAction` (`escape_parser.zig` at `c1db919`, :270).
        0x0f => Action::ToggleTranscript,
        0x10 => Action::ControlP,
        0x15 => Action::KillToStart,
        0x16 => Action::PasteImage,
        0x17 => Action::DeleteWordLeft,
        0x19 => Action::Yank,
        0x1f => Action::Undo,
        _ => Action::Ignore,
    }
}

/// What a completed `ESC [ <params> <final>` means.
///
/// The parameters are matched **as bytes, in the exact shapes a terminal sends
/// a key in**, and -- outside a key report -- never parsed into numbers that
/// are then compared. A numeric comparison accepts spellings no terminal
/// emits -- `CSI 0200 ~` and
/// `CSI 200 ; 1 ~` both *parse* as 200 -- and the paste markers are where that
/// stops being cosmetic: [`Decoder::pasted`] matches the end marker byte for
/// byte, so a decoder that opened a paste on `CSI 0200 ~` could never be closed
/// by the same spelling. A stream that opens one is latched into paste mode,
/// and every keystroke after it is content. Exact both ways, or the two halves
/// disagree. The one place numbers are read is a key report, whose key is any
/// Unicode scalar and cannot be a table of spellings; there each field is
/// accepted only in its canonical spelling first ([`decimal`]), which makes the
/// reading one-to-one and keeps the same property.
///
/// Everything that is not one of these shapes is [`Action::Ignore`], which is
/// the same answer the bounded discard gives and for the same reason: an input
/// this session cannot name is not one it should guess at. `CSI 999 C` is not
/// an arrow key, and a private-parameter sequence (`?`, `>`, `<`, `=`) is a
/// terminal talking about itself rather than a key at all -- both fail the
/// shape check without needing a case of their own.
///
/// An [`Input`] rather than an [`Action`] because a key report can be a
/// character: a keypad digit, or Shift+Space ([`replayed`]).
fn csi(params: &[u8], final_byte: u8) -> Input {
    match final_byte {
        // The tilde keys, each one exactly one spelling.
        b'~' => match params {
            b"1" => Input::Action(Action::Home),
            b"3" => Input::Action(Action::Delete),
            b"4" => Input::Action(Action::End),
            // The page keys (`escape_parser.zig` at `c1db919`, :632-633).
            b"5" => Input::Action(Action::PageUp),
            b"6" => Input::Action(Action::PageDown),
            b"200" => Input::Action(Action::PasteStart),
            b"201" => Input::Action(Action::PasteEnd),
            // xterm's `modifyOtherKeys` spelling of a key report -- a
            // **params shape** under a final byte that already carries five
            // literal ones -- or a tilde key with a modifier.
            _ => tilde_report(params),
        },
        // The cursor keys, bare or with one modifier.
        b'A' | b'B' | b'C' | b'D' | b'H' | b'F' => {
            let Some(modifier) = modifier(params) else {
                return Input::Action(Action::Ignore);
            };
            // The modifier is one more than a bitmask.
            Input::Action(cursor(final_byte, modifier - 1))
        }
        // Kitty's spelling of a key report.
        b'u' => report(params),
        // Shift-Tab, bare or with a modifier (`escape_parser.zig:569`,
        // `:723-725`): upstream reads any modifier as the same key, and a
        // modifier field of `1` -- none at all -- is not a spelling it has.
        b'Z' => match modifier(params) {
            Some(1) if params.is_empty() => Input::Action(Action::BackTab),
            Some(mask) if mask > 1 => Input::Action(Action::BackTab),
            _ => Input::Action(Action::Ignore),
        },
        _ => Input::Action(Action::Ignore),
    }
}

/// What a cursor key means under a modifier mask: this decoder's main-row
/// arrow table, and the one a keypad arrow is resolved through as well
/// ([`kitty_key`]).
///
/// Shift is dropped from Left and Right along with the selection it would have
/// extended, so what is left there is "ctrl or alt means by word". On Up and
/// Down it is kept, as [`Action::SelectUp`] and [`Action::SelectDown`].
fn cursor(letter: u8, mask: u8) -> Action {
    let word = mask & (ALT | CTRL) != 0;
    let shifted = mask & SHIFT != 0;
    match letter {
        b'A' if shifted => Action::SelectUp,
        b'B' if shifted => Action::SelectDown,
        b'A' => Action::Up,
        b'B' => Action::Down,
        b'C' if word => Action::WordRight,
        b'C' => Action::Right,
        b'D' if word => Action::WordLeft,
        b'D' => Action::Left,
        b'H' => Action::Home,
        _ => Action::End,
    }
}

/// `CSI <code> u`, `CSI <code> ; <modifier> u` or
/// `CSI <code> ; <modifier> : <event> u` -- kitty's key report
/// (`escape_parser.zig` at `c1db919`, :613-619, :686-692 and :756-780).
///
/// The event type is the one rule here that is not [`kitty_key`]'s: a press
/// (`1`) or a repeat (`2`) is the key, and a release (`3`) -- or anything else
/// -- is not a keystroke at all, because a release that acted would close a
/// panel or move the caret a second time for one key. Upstream reads it on
/// every report (`c1db919`; `ef1d0d0` read it on keycode 27 alone, :570).
///
/// Stricter than upstream in the grammar only, never in what a well-formed
/// report means: a field upstream would fold into a number from a spelling no
/// terminal emits -- a leading zero, a modifier of `0`, a third `;` field --
/// is [`Action::Ignore`] here, for the reason [`csi`] gives.
fn report(params: &[u8]) -> Input {
    let ignore = Input::Action(Action::Ignore);
    let mut fields = params.split(|byte| *byte == b';');
    let (Some(code), modifier, None) = (fields.next(), fields.next(), fields.next()) else {
        return ignore;
    };
    let Some(code) = decimal(code, KEYCODE_DIGITS) else {
        return ignore;
    };
    let Some(modifier) = modifier else {
        return kitty_key(code, 0);
    };
    let mut parts = modifier.split(|byte| *byte == b':');
    let (Some(mask), event, None) = (parts.next(), parts.next(), parts.next()) else {
        return ignore;
    };
    let Some(mask) = key_modifiers(mask) else {
        return ignore;
    };
    match event {
        None | Some(b"1" | b"2") => kitty_key(code, mask),
        Some(_) => ignore,
    }
}

/// `CSI 27 ; <modifier> ; <code> ~`, the other spelling of the same report,
/// resolved by the same function (`escape_parser.zig` at `c1db919`, :746-752).
///
/// Upstream discards the first field (:606-611 and :668-674 overwrite it), so
/// it would read `CSI 28 ; 10 ; 122 ~` as this key too; `27` is the only value
/// a terminal sends there, and the only one this decoder accepts.
fn tilde_report(params: &[u8]) -> Input {
    let ignore = Input::Action(Action::Ignore);
    let mut fields = params.split(|byte| *byte == b';');
    match (fields.next(), fields.next(), fields.next(), fields.next()) {
        (Some(b"27"), Some(mask), Some(code), None) => {
            match (key_modifiers(mask), decimal(code, KEYCODE_DIGITS)) {
                (Some(mask), Some(code)) => kitty_key(code, mask),
                _ => ignore,
            }
        }
        (Some(key), Some(mask), None, None) => match key_modifiers(mask) {
            Some(mask) => modified_tilde(key, mask),
            None => ignore,
        },
        _ => ignore,
    }
}

/// `CSI <key> ; <modifier> ~`: a tilde key held with a modifier
/// (`escape_parser.zig` at `c1db919`, :694-718).
///
/// Delete is upstream's `forwardDeleteKeyAction` (:175-180): Super kills to
/// the end of the line, Alt or Ctrl deletes the word to the right, and Shift
/// alone is nothing. The page keys are a page move with any modifier
/// (`composerMove(.page_up, ...)`, :708-709) -- Shift's selection extension is
/// dropped with the selection, as the arrows drop it ([`cursor`]). Home and End
/// with a modifier are upstream's draft and line jumps, which this composer
/// does not have, and stay [`Action::Ignore`].
///
/// A modifier field of `1` -- no modifier at all -- is not a spelling any
/// terminal sends for these keys, and is refused as the bare `CSI 1 ; 1 Z` is
/// ([`csi`]): the bare key has its one spelling.
fn modified_tilde(key: &[u8], mask: u8) -> Input {
    let mods = mask & 0x3f;
    if mask == 0 {
        return Input::Action(Action::Ignore);
    }
    Input::Action(match key {
        b"3" => forward_delete(mods),
        b"5" => Action::PageUp,
        b"6" => Action::PageDown,
        _ => Action::Ignore,
    })
}

/// Delete under a modifier mask (`forwardDeleteKeyAction`,
/// `escape_parser.zig` at `c1db919`, :175-180), shared by the legacy tilde
/// spelling and the keypad's.
fn forward_delete(mods: u8) -> Action {
    if mods & SUPER != 0 {
        Action::KillToEnd
    } else if mods & (ALT | CTRL) != 0 {
        Action::DeleteWordRight
    } else if mods == 0 {
        Action::Delete
    } else {
        Action::Ignore
    }
}

/// The longest keycode field: seven digits hold every Unicode scalar.
const KEYCODE_DIGITS: usize = 7;

/// A decimal field in the one spelling a terminal emits it in, or `None`:
/// digits only, no leading zero, and at most `most` of them.
///
/// The leading-zero refusal is what makes the reading one-to-one -- `099` and
/// `99` would otherwise be two spellings of one key -- and it refuses `0`
/// itself, which is no key and no modifier.
fn decimal(field: &[u8], most: usize) -> Option<u32> {
    if field.is_empty()
        || field.len() > most
        || field.first() == Some(&b'0')
        || !field.iter().all(u8::is_ascii_digit)
    {
        return None;
    }
    Some(
        field
            .iter()
            .fold(0, |value, digit| value * 10 + u32::from(digit - b'0')),
    )
}

/// A key report's modifier field, undone: kitty's `1 + mask` over eight bits --
/// shift, alt, ctrl, super, hyper, meta, caps lock, num lock -- so `1` to
/// `256`, and nothing past it.
fn key_modifiers(field: &[u8]) -> Option<u8> {
    let value = decimal(field, 3)?;
    u8::try_from(value.checked_sub(1)?).ok()
}

// The modifier bits (`escape_parser.zig` at `c1db919`, :61-64).
const SHIFT: u8 = 0x01;
const ALT: u8 = 0x02;
const CTRL: u8 = 0x04;
const SUPER: u8 = 0x08;

// Kitty's functional-key codes this port names (`escape_parser.zig` at
// `c1db919`, :38-60). KP_SEPARATOR, KP_INSERT and KP_BEGIN are absent there
// too: they have no main-row key to behave as.
const KITTY_UP: u32 = 57352;
const KITTY_DOWN: u32 = 57353;
const KP_0: u32 = 57399;
const KP_9: u32 = 57408;
const KP_ENTER: u32 = 57414;
const KP_PAGE_UP: u32 = 57421;
const KP_PAGE_DOWN: u32 = 57422;
const KP_DELETE: u32 = 57426;

/// The character a keypad key types (`escape_parser.zig` at `c1db919`,
/// :121-132).
fn keypad_character(code: u32) -> Option<u8> {
    if (KP_0..=KP_9).contains(&code) {
        return u8::try_from(code - KP_0).ok().map(|digit| b'0' + digit);
    }
    match code {
        57409 => Some(b'.'), // KP_DECIMAL
        57410 => Some(b'/'), // KP_DIVIDE
        57411 => Some(b'*'), // KP_MULTIPLY
        57412 => Some(b'-'), // KP_SUBTRACT
        57413 => Some(b'+'), // KP_ADD
        57415 => Some(b'='), // KP_EQUAL
        _ => None,
    }
}

/// The cursor-key letter a keypad navigation key is (:134-144).
fn keypad_navigation(code: u32) -> Option<u8> {
    match code {
        57419 => Some(b'A'), // KP_UP
        57420 => Some(b'B'), // KP_DOWN
        57418 => Some(b'C'), // KP_RIGHT
        57417 => Some(b'D'), // KP_LEFT
        57423 => Some(b'H'), // KP_HOME
        57424 => Some(b'F'), // KP_END
        _ => None,
    }
}

/// What one key report means: upstream's `kittyUnicodeKeyAction`
/// (`escape_parser.zig` at `c1db919`, :185-266), branch for branch and in its
/// order, because the order is part of the meaning -- Ctrl+Alt+D is
/// `delete_word_right` before Ctrl could turn it into `0x04`, and Ctrl+Super+C
/// is `copy_selection` before Ctrl could turn it into a cancel.
///
/// Every upstream result lands on an action this session already has:
///
/// * a **remapped byte** goes through [`replayed`] -- Ctrl+letter is the
///   letter's control byte and therefore [`control`]'s row for it, and a
///   keypad digit is the digit typed;
/// * a **keypad navigation key** is resolved through [`cursor`], which is this
///   decoder's main-row arrow table -- upstream's own rule that a keypad key
///   behaves as its main-row key (:116-118), read against xfx's main row;
/// * `escape`, `insert_newline`, `undo`/`redo`, `word_left`/`word_right`,
///   `delete_word_left`, `delete_word_right`,
///   `delete_to_line_start`/`delete_to_line_end`, `delete_next` and the page
///   keys are [`Action::Escape`], [`Action::InsertNewline`],
///   [`Action::Undo`]/[`Action::Redo`], [`Action::WordLeft`]/[`Action::WordRight`],
///   [`Action::DeleteWordBackward`], [`Action::DeleteWordRight`],
///   [`Action::KillToStart`]/[`Action::KillToEnd`], [`Action::Delete`] and
///   [`Action::PageUp`]/[`Action::PageDown`], a page key with a modifier
///   included -- upstream's `composerMove` to a page (:171), whose selection
///   extension this composer does not have;
/// * `toggle_full_transcript` -- Ctrl+O and nothing else held (:111-114,
///   :252) -- is [`Action::ToggleTranscript`], and `select_all`,
///   `copy_selection` and `cut_selection` (:223-231) are
///   [`Action::SelectAll`], [`Action::CopySelection`] and
///   [`Action::CutSelection`].
///
/// What has no action here is [`Action::Ignore`], each named where it falls:
/// every `composerMove` that extends a selection (:238-241, :244,
/// :248), an `o` under any other modifiers (:111-114) and
/// `open_all_sessions` (:253). `toggle_permission_mode` (:254) is
/// [`Action::BackTab`], the key it is spelled with. Upstream's
/// `meta_prefixed` -- a key report behind a second `ESC` -- has no counterpart:
/// this decoder reads `ESC ESC` as the Escape key and then whatever follows,
/// so it is always `false` here and its branches are not carried.
fn kitty_key(code: u32, modifiers: u8) -> Input {
    let ignore = Input::Action(Action::Ignore);
    // Caps Lock and Num Lock are states, not modifiers (:186-187).
    let mods = modifiers & 0x3f;
    if code == 27 && mods == 0 {
        return Input::Action(Action::Escape);
    }
    // Hyper and Meta survive the mask, and a keypad key held with either one
    // is not resolved as the keypad at all (:189-191).
    if mods & !(SHIFT | ALT | CTRL | SUPER) == 0 {
        if let Some(byte) = keypad_character(code) {
            // Only a bare or shifted keypad press types (:193-195); anything
            // else falls through to the rest of the table.
            if mods & !SHIFT == 0 {
                return replayed(byte);
            }
        } else if let Some(letter) = keypad_navigation(code) {
            return Input::Action(cursor(letter, mods));
        } else if code == KP_PAGE_UP || code == KP_PAGE_DOWN {
            // `page_up`/`page_down`, or a `composerMove` to them with a
            // modifier (:169-173): a page either way here, because the
            // modifier's selection is one this composer does not have.
            return Input::Action(if code == KP_PAGE_UP {
                Action::PageUp
            } else {
                Action::PageDown
            });
        } else if code == KP_ENTER {
            return kitty_key(13, modifiers);
        } else if code == KP_DELETE {
            // The main-row `ESC[3~` family (:175-180).
            return Input::Action(forward_delete(mods));
        }
    }
    if code == KITTY_UP || code == KITTY_DOWN {
        let letter = if code == KITTY_UP { b'A' } else { b'B' };
        // Upstream's modified arrow table answers every modifier but Hyper and
        // Meta alone, which are `.ignore` (:208-217).
        if mods != 0 && mods & (SHIFT | ALT | CTRL | SUPER) == 0 {
            return ignore;
        }
        return Input::Action(cursor(letter, mods));
    }
    if code == 13 && mods == CTRL {
        return replayed(b'\r');
    }
    if code == 13 && mods & (SHIFT | ALT) != 0 {
        return Input::Action(Action::InsertNewline);
    }
    if code == 32 && mods == SHIFT {
        return replayed(b' ');
    }
    // A letter, either case: the branches below compare the keycode against
    // ASCII, and only `b` and `f` under Alt (:243, :247) are lowercase alone.
    let ascii = u8::try_from(code).ok().filter(u8::is_ascii);
    let is = |wanted: u8| ascii.is_some_and(|byte| byte.to_ascii_lowercase() == wanted);
    if mods & SUPER != 0 {
        // `select_all`, `copy_selection`, `cut_selection` (:223-231), with
        // whatever else is held.
        if is(b'a') {
            return Input::Action(Action::SelectAll);
        }
        if is(b'c') {
            return Input::Action(Action::CopySelection);
        }
        if is(b'x') {
            return Input::Action(Action::CutSelection);
        }
        if is(b'z') {
            return Input::Action(if mods & SHIFT == 0 {
                Action::Undo
            } else {
                Action::Redo
            });
        }
    }
    // A `composerMove` that extends a selection (:235-242).
    if mods & (SHIFT | CTRL) == (SHIFT | CTRL) && (is(b'a') || is(b'b') || is(b'e') || is(b'f')) {
        return ignore;
    }
    if mods & ALT != 0 && matches!(ascii, Some(b'b' | b'f')) {
        // With Shift, the same move extending a selection (:244, :248).
        if mods & SHIFT != 0 {
            return ignore;
        }
        // Alt-b is upstream's `.word_left` (:245), not a composer move:
        // [`Action::AltB`].
        return Input::Action(if ascii == Some(b'b') {
            Action::AltB
        } else {
            Action::WordRight
        });
    }
    // `delete_word_right` (:251).
    if is(b'd') && mods & ALT != 0 {
        return Input::Action(Action::DeleteWordRight);
    }
    // `toggle_full_transcript` for Ctrl+O alone, and nothing for an `o` under
    // any other modifiers (`ctrlOKeyAction`, :111-114, :252).
    if is(b'o') {
        return Input::Action(if mods == CTRL {
            Action::ToggleTranscript
        } else {
            Action::Ignore
        });
    }
    // `open_all_sessions` (:253).
    if is(b'r') && mods & SUPER != 0 {
        return ignore;
    }
    // `toggle_permission_mode` (:254): Tab with Shift, whatever else is held.
    if code == 9 && mods & SHIFT != 0 {
        return Input::Action(Action::BackTab);
    }
    // Ctrl and a letter or `_` is that key's control byte (:255-259); the
    // low five bits are the arithmetic upstream spells as `- 96` and `- 64`.
    if mods & CTRL != 0 {
        if let Some(byte @ (b'a'..=b'z' | b'A'..=b'Z' | b'_')) = ascii {
            return replayed(byte & 0x1f);
        }
    }
    if code == 127 && mods & SUPER != 0 {
        return Input::Action(Action::KillToStart);
    }
    if code == 127 && mods & ALT != 0 {
        return Input::Action(Action::DeleteWordBackward);
    }
    match code {
        13 => replayed(b'\r'),
        9 => replayed(b'\t'),
        127 => replayed(0x7f),
        _ => ignore,
    }
}

/// A byte a key report stands for, decoded as though it had been typed:
/// upstream's `remapped_byte`.
///
/// The same closed table [`Decoder::ground`] uses, so the control policy holds
/// here too -- a control byte is [`control`]'s binding or [`Action::Ignore`],
/// and only printable ASCII is ever text.
fn replayed(byte: u8) -> Input {
    match byte {
        0x00..=0x1f | 0x7f => Input::Action(control(byte)),
        0x20..=0x7e => Input::Text(char::from(byte)),
        _ => Input::Action(Action::Ignore),
    }
}

/// The modifier a cursor-key sequence carries, or `None` when its parameters
/// are not a shape a terminal sends a cursor key in.
///
/// Two shapes and no others: nothing at all (`CSI C`), or the literal `1;` and
/// one modifier in `1..=16` (`CSI 1 ; 5 C`), which is the whole of xterm's
/// modifier encoding. `CSI 2 ; 5 C`, `CSI 1 ; 5 ; 9 C` and `CSI 1 ; 5 : 3 C`
/// are not cursor keys and do not become movement by being nearly one.
fn modifier(params: &[u8]) -> Option<u8> {
    if params.is_empty() {
        return Some(1);
    }
    let digits = params.strip_prefix(&b"1;"[..])?;
    if digits.is_empty() || digits.len() > 2 || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let value = digits
        .iter()
        .fold(0u8, |value, digit| value * 10 + (digit - b'0'));
    (1..=16).contains(&value).then_some(value)
}

/// What `ESC O <byte>` means: the keys a `CSI` also spells, in the form a
/// terminal in application-cursor mode sends them
/// (`escape_parser.zig:535-553`).
fn ss3(byte: u8) -> Action {
    match byte {
        b'A' => Action::Up,
        b'B' => Action::Down,
        b'C' => Action::Right,
        b'D' => Action::Left,
        b'H' => Action::Home,
        b'F' => Action::End,
        _ => Action::Ignore,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `bytes` as one delivery: fed, and then the delivery ended.
    fn decode(bytes: &[u8]) -> Vec<Input> {
        let mut decoder = Decoder::new();
        let now = Instant::now();
        let mut out = Vec::new();
        for byte in bytes {
            decoder.feed(*byte, now, &mut out);
        }
        deliver(&mut decoder, &mut out);
        out
    }

    /// Ends a delivery the way the shell does (`super::super::shell::Shell`'s
    /// `end_delivery`): a paste that ended in it is its [`Action::PasteEnd`],
    /// whatever followed its marker.
    fn deliver(decoder: &mut Decoder, out: &mut Vec<Input>) -> Delivery {
        let delivery = decoder.end_delivery();
        if let Delivery::Pasted { .. } = delivery {
            out.push(Input::Action(Action::PasteEnd));
        }
        delivery
    }

    /// What `csi` decodes a sequence to, for the cases that are an action.
    ///
    /// Shadows the module's own `csi` on purpose: that one returns an
    /// [`Input`] because a key report can be a character, and every case
    /// written against it here is about an action -- so a sequence that typed
    /// a character instead fails loudly rather than comparing unequal to some
    /// action by accident.
    fn csi(params: &[u8], final_byte: u8) -> Action {
        match super::csi(params, final_byte) {
            Input::Action(action) => action,
            other => panic!("{params:?} {final_byte:?} decoded to {other:?}, not an action"),
        }
    }

    #[test]
    fn the_two_editing_control_bytes_are_bound_and_ctrl_z_is_not() {
        // `0x1f` undo and `0x19` yank (`shortcuts.zig:23-24,84-85`). Redo has
        // **no** control byte -- upstream's table has no arm for it -- and
        // `0x1a` is not undo: inventing either would bind a key on the strength
        // of what it looks like it should do.
        assert_eq!(control(0x1f), Action::Undo);
        assert_eq!(control(0x19), Action::Yank);
        assert_eq!(control(0x1a), Action::Ignore, "Ctrl-Z is still not undo");
        for byte in 0x00u8..=0x1f {
            assert_ne!(
                control(byte),
                Action::Redo,
                "{byte:#04x} was bound to a redo no control byte spells"
            );
        }
        assert_eq!(
            decode(&[0x1f, 0x19]),
            vec![Input::Action(Action::Undo), Input::Action(Action::Yank),]
        );
    }

    #[test]
    fn super_shift_z_is_redo_in_both_pinned_encodings() {
        // The two families ruling (1) pins, and only those
        // (`runtime.zig:3018,3025`; `escape_parser.zig:42,122-123`). The
        // modifier parameter is xterm's `1 + mask`, so `10` is super | shift and
        // `9` is super alone.
        assert_eq!(csi(b"122;10", b'u'), Action::Redo);
        assert_eq!(csi(b"27;10;122", b'~'), Action::Redo);
        assert_eq!(csi(b"122;9", b'u'), Action::Undo, "super without shift");
        assert_eq!(csi(b"27;9;122", b'~'), Action::Undo);
        assert_eq!(
            csi(b"90;10", b'u'),
            Action::Redo,
            "escape_parser.zig:122 accepts Z"
        );
        assert_eq!(csi(b"90;9", b'u'), Action::Undo);
    }

    #[test]
    fn everything_else_under_the_two_families_is_ignored() {
        // The refusals matter as much as the two acceptances: a decoder that
        // took a spelling no terminal emits would be a decoder that guessed.
        assert_eq!(
            csi(b"122;1", b'u'),
            Action::Ignore,
            "no super bit is not a redo"
        );
        assert_eq!(
            csi(b"122;5", b'u'),
            Action::Ignore,
            "ctrl is not super: Ctrl-Z, which binds nothing"
        );
        assert_eq!(
            csi(b"122;010", b'u'),
            Action::Ignore,
            "spellings no terminal emits"
        );
        assert_eq!(csi(b"0122;10", b'u'), Action::Ignore);
        assert_eq!(
            csi(b"122;17", b'u'),
            Action::Ignore,
            "hyper alone is not super"
        );
        assert_eq!(
            csi(b"122;257", b'u'),
            Action::Ignore,
            "past the modifier range"
        );
        assert_eq!(
            csi(b"122;0", b'u'),
            Action::Ignore,
            "a modifier is 1 + a mask"
        );
        assert_eq!(csi(b"121;10", b'u'), Action::Ignore, "y is not z");
        assert_eq!(csi(b"122", b'u'), Action::Ignore, "a bare z binds nothing");
        assert_eq!(csi(b"122;10;3", b'u'), Action::Ignore, "a third parameter");
        assert_eq!(csi(b"", b'u'), Action::Ignore);
        assert_eq!(csi(b"27;10;121", b'~'), Action::Ignore, "y is not z");
        assert_eq!(csi(b"28;10;122", b'~'), Action::Ignore, "27 is the family");
        assert_eq!(csi(b"27;10", b'~'), Action::Ignore, "no keycode");
        assert_eq!(
            csi(b"27;10;122;1", b'~'),
            Action::Ignore,
            "a fourth parameter"
        );
        // And the five literal tilde spellings still decode, because the new
        // arm is under the same final byte.
        assert_eq!(csi(b"1", b'~'), Action::Home);
        assert_eq!(csi(b"3", b'~'), Action::Delete);
        assert_eq!(csi(b"4", b'~'), Action::End);
        assert_eq!(csi(b"200", b'~'), Action::PasteStart);
        assert_eq!(csi(b"201", b'~'), Action::PasteEnd);
    }

    #[test]
    fn captured_ctrl_c_d_u_tilde_chords_decode_to_the_bound_control_actions() {
        // A real terminal (WezTerm, `enable_kitty_keyboard` on) was captured
        // sending Ctrl-C, Ctrl-D and Ctrl-U as `ESC[27;5;<code>~`, modifier
        // `5` being ctrl alone -- driven through the terminal's own `SendKey`
        // action, not a physical keypress, and logged raw in
        // `.prd/tui-phase3/loop.md:416` (R125). `control` is where each
        // letter is already bound; this is the literal captured spelling,
        // replayed straight into `csi`.
        assert_eq!(csi(b"27;5;99", b'~'), Action::Cancel, "captured Ctrl-C");
        assert_eq!(csi(b"27;5;100", b'~'), Action::Eof, "captured Ctrl-D");
        assert_eq!(
            csi(b"27;5;117", b'~'),
            Action::KillToStart,
            "captured Ctrl-U"
        );
    }

    #[test]
    fn a_ctrl_letter_is_upstreams_table_in_a_strict_spelling() {
        // Bounded on every axis, not just the three captured letters.

        // The modifier parameter is `1 + mask` (shift 1, alt 2, ctrl 4,
        // super 8), and what a combination means is upstream's
        // `kittyUnicodeKeyAction` (`escape_parser.zig` at `c1db919`,
        // :185-266): `2` is shift alone and `3` alt alone, neither of which
        // makes `c` anything; `13` is ctrl *and* super, which upstream reads
        // as `copy_selection` before Ctrl is consulted (:226-228).
        assert_eq!(csi(b"27;2;99", b'~'), Action::Ignore, "shift is not ctrl");
        assert_eq!(csi(b"27;3;99", b'~'), Action::Ignore, "alt is not ctrl");
        assert_eq!(
            csi(b"27;13;99", b'~'),
            Action::CopySelection,
            "ctrl+super+c is copy_selection, ahead of Ctrl"
        );
        assert_eq!(csi(b"27;1;99", b'~'), Action::Ignore, "no modifier at all");

        // A letter `control` does not bind stays `Ignore`, the same answer
        // `control` itself gives -- this does not invent a binding `control`
        // refused.
        assert_eq!(
            csi(b"27;5;103", b'~'),
            Action::Ignore,
            "g has no control byte"
        );
        assert_eq!(
            csi(b"27;5;122", b'~'),
            Action::Ignore,
            "Ctrl-Z is still not undo"
        );

        // `Ctrl-_` is Undo in both spellings: herdr was captured sending its
        // `u` spelling (`ESC[95;5u`), and upstream resolves the two spellings
        // with the one function (`escape_parser.zig` at `c1db919`, :256), so
        // the tilde spelling cannot stay refused while the other is bound.
        assert_eq!(csi(b"27;5;95", b'~'), Action::Undo, "Ctrl-_ is undo");

        // Every spelling of Ctrl-C's code that is not the exact captured
        // bytes `99`, refused for two different reasons. A leading zero
        // (`099`) and an oversized leading zero (`0099`) are the genuine
        // numeric-parse-leniency case: a numeric parse folds digits into a
        // value and does not care how many leading zeros came first, so
        // both would parse to `99` and be wrongly accepted -- the canonical-
        // spelling check (`decimal`) is what refuses them here. A trailing
        // digit (`990`), no
        // code at all, and Kitty's colon-separated event-qualifier suffix
        // for a release or a repeat (`99:2`) are not that: `990` parses to
        // a different number than `99`, and an empty or suffixed field
        // does not parse as `99` either, so a numeric parse would have
        // refused these three on its own merits too. They are tested here
        // for a different property -- that the match is exact-length and
        // exact-shape, not just exact-value, so none of these is mistaken
        // for a byte-string prefix or extension of the real code.
        assert_eq!(
            csi(b"27;5;099", b'~'),
            Action::Ignore,
            "a leading zero is not the canonical spelling"
        );
        assert_eq!(
            csi(b"27;5;0099", b'~'),
            Action::Ignore,
            "a leading zero, oversized too"
        );
        assert_eq!(
            csi(b"27;5;990", b'~'),
            Action::Ignore,
            "a trailing digit is a different code"
        );
        assert_eq!(csi(b"27;5;", b'~'), Action::Ignore, "no code at all");
        assert_eq!(
            csi(b"27;5;99:2", b'~'),
            Action::Ignore,
            "a release/repeat event-qualifier suffix, not a plain code"
        );

        // A fourth field is refused the same way it already is for Super+Z
        // (`everything_else_under_the_two_families_is_ignored`), because the
        // shape check is `tilde_report`'s, ahead of the key lookup.
        assert_eq!(csi(b"27;5;99;1", b'~'), Action::Ignore, "a fourth field");

        // The `u` spelling of the same key is the same key: herdr's
        // libghostty encoder sends Ctrl-C as exactly these bytes
        // (`.prd/tui-phase3/receipts/2026-10-06-herdr-csi-u-keys.md`), and
        // upstream routes both spellings through one function
        // (`escape_parser.zig` at `c1db919`, :686-692 and :746-752).
        assert_eq!(
            csi(b"99;5", b'u'),
            Action::Cancel,
            "the u spelling of Ctrl-C is Ctrl-C"
        );
        // And the grammar is as strict under `u` as under `~`.
        assert_eq!(csi(b"099;5", b'u'), Action::Ignore, "a leading zero");
        assert_eq!(csi(b"99;05", b'u'), Action::Ignore, "a leading zero");
        assert_eq!(csi(b"99;5;1", b'u'), Action::Ignore, "a third field");
        assert_eq!(csi(b"99;", b'u'), Action::Ignore, "an empty modifier");
    }

    /// The herdr receipt, row for row: the key that was sent, the bytes that
    /// arrived, and the action they have to decode to.
    ///
    /// The bytes are the receipt's, literal for literal
    /// (`.prd/tui-phase3/receipts/2026-10-06-herdr-csi-u-keys.md`: herdr 0.9.3,
    /// libghostty-vt's key encoder, kitty flag 1 negotiated). The action is what
    /// upstream's `kittyUnicodeKeyAction` (`escape_parser.zig` at `c1db919`,
    /// :185-266) resolves the key to, carried onto the action xfx already has
    /// for it. The receipt's last row -- Home, End and Delete -- has no bytes:
    /// herdr refused those key names, so nothing was captured for them.
    const HERDR_RECEIPT: &[(&str, &[u8], Action)] = &[
        ("ctrl+c", b"\x1b[99;5u", Action::Cancel),
        ("ctrl+d", b"\x1b[100;5u", Action::Eof),
        ("ctrl+u", b"\x1b[117;5u", Action::KillToStart),
        ("esc", b"\x1b[27u", Action::Escape),
        ("enter", b"\x0d", Action::Submit),
        ("tab", b"\x09", Action::Tab),
        ("backspace", b"\x7f", Action::Backspace),
        ("alt+enter", b"\x1b[13;3u", Action::InsertNewline),
        ("ctrl+a", b"\x1b[97;5u", Action::Home),
        ("ctrl+e", b"\x1b[101;5u", Action::End),
        ("ctrl+k", b"\x1b[107;5u", Action::ControlK),
        ("ctrl+w", b"\x1b[119;5u", Action::DeleteWordLeft),
        ("ctrl+y", b"\x1b[121;5u", Action::Yank),
        ("ctrl+p", b"\x1b[112;5u", Action::ControlP),
        ("ctrl+n", b"\x1b[110;5u", Action::HistoryNext),
        ("up", b"\x1b[A", Action::Up),
        ("down", b"\x1b[B", Action::Down),
        ("left", b"\x1b[D", Action::Left),
        ("right", b"\x1b[C", Action::Right),
        // `toggle_permission_mode` (:254), which is Shift-Tab's action.
        ("shift+tab", b"\x1b[9;2u", Action::BackTab),
        ("super+z", b"\x1b[122;9u", Action::Undo),
        ("super+shift+z", b"\x1b[122;10u", Action::Redo),
        ("ctrl+b", b"\x1b[98;5u", Action::Left),
        ("ctrl+f", b"\x1b[102;5u", Action::Right),
        ("ctrl+h", b"\x1b[104;5u", Action::Backspace),
        ("ctrl+i", b"\x1b[105;5u", Action::Tab),
        ("ctrl+j", b"\x1b[106;5u", Action::ControlJ),
        ("ctrl+l", b"\x1b[108;5u", Action::Redraw),
        ("ctrl+m", b"\x1b[109;5u", Action::Submit),
        ("ctrl+_", b"\x1b[95;5u", Action::Undo),
        // Upstream remaps only `_` and the letters under Ctrl (:255-259), so
        // these two fall through to its final `.ignore` (:265).
        ("ctrl+/", b"\x1b[47;5u", Action::Ignore),
        ("ctrl+-", b"\x1b[45;5u", Action::Ignore),
        ("alt+backspace", b"\x1b[127;3u", Action::DeleteWordBackward),
        ("alt+left", b"\x1b[1;3D", Action::WordLeft),
        ("alt+right", b"\x1b[1;3C", Action::WordRight),
        ("ctrl+left", b"\x1b[1;5D", Action::WordLeft),
        ("ctrl+right", b"\x1b[1;5C", Action::WordRight),
        ("shift+enter", b"\x1b[13;2u", Action::InsertNewline),
        ("ctrl+enter", b"\x1b[13;5u", Action::Submit),
        // Upstream's `.word_left` (:243-246@481104fd), which a question's
        // choice reads apart from the modified arrows above.
        ("alt+b", b"\x1b[98;3u", Action::AltB),
        ("alt+f", b"\x1b[102;3u", Action::WordRight),
    ];

    #[test]
    fn every_key_the_herdr_receipt_captured_decodes_to_its_binding() {
        // Every row is checked before anything is asserted, so a failure names
        // all the keys that are wrong rather than the first one.
        let wrong: Vec<String> = HERDR_RECEIPT
            .iter()
            .filter_map(|(key, bytes, action)| {
                let got = decode(bytes);
                (got != vec![Input::Action(*action)])
                    .then(|| format!("{key} {bytes:?}: wanted {action:?}, got {got:?}"))
            })
            .collect();
        assert!(
            wrong.is_empty(),
            "{} of {} receipt rows decode wrongly:\n{}",
            wrong.len(),
            HERDR_RECEIPT.len(),
            wrong.join("\n")
        );
    }

    #[test]
    fn a_kitty_event_type_acts_on_press_and_repeat_and_never_on_release() {
        // `escape_parser.zig` at `c1db919`, :676-684 and :756-780: a colon
        // after the modifier carries the event type, and only a press (1) or
        // a repeat (2) is a keystroke -- a release must not close a panel or
        // move the caret. `ef1d0d0` took the colon on keycode 27 alone (:570);
        // `c1db919` takes it on every `u` report, which is what is ported.
        assert_eq!(
            decode(b"\x1b[27;1:1u"),
            vec![Input::Action(Action::Escape)],
            "an Escape press"
        );
        assert_eq!(
            decode(b"\x1b[27;1:2u"),
            vec![Input::Action(Action::Escape)],
            "an Escape repeat"
        );
        assert_eq!(
            decode(b"\x1b[27;1:3u"),
            vec![Input::Action(Action::Ignore)],
            "an Escape release"
        );
        assert_eq!(
            decode(b"\x1b[99;5:1u"),
            vec![Input::Action(Action::Cancel)],
            "a Ctrl-C press"
        );
        assert_eq!(
            decode(b"\x1b[99;5:3u"),
            vec![Input::Action(Action::Ignore)],
            "a Ctrl-C release"
        );
        // The same strict grammar as every other field: one canonical
        // spelling, under `u` only, and nothing after it.
        for stream in [
            &b"\x1b[27;1:01u"[..],
            b"\x1b[27;1:u",
            b"\x1b[27;1:1:1u",
            b"\x1b[27;1:1;1u",
            b"\x1b[27;:1u",
            b"\x1b[27:1;1u",
            b"\x1b[27;1:1~",
            b"\x1b[27;1;27:1~",
        ] {
            assert_eq!(
                decode(stream),
                vec![Input::Action(Action::Ignore)],
                "{stream:?}"
            );
        }
    }

    #[test]
    fn the_lock_bits_are_not_modifiers() {
        // `escape_parser.zig` at `c1db919`, :186-187: Caps Lock (64) and Num
        // Lock (128) are states, not modifiers, so Ctrl-C with either lit is
        // still Ctrl-C and a bare Escape with Caps Lock on is still Escape.
        assert_eq!(
            decode(b"\x1b[99;69u"),
            vec![Input::Action(Action::Cancel)],
            "ctrl + caps lock"
        );
        assert_eq!(
            decode(b"\x1b[99;133u"),
            vec![Input::Action(Action::Cancel)],
            "ctrl + num lock"
        );
        assert_eq!(
            decode(b"\x1b[27;65u"),
            vec![Input::Action(Action::Escape)],
            "caps lock alone"
        );
        assert_eq!(
            decode(b"\x1b[27;5;99~"),
            decode(b"\x1b[27;69;99~"),
            "the tilde spelling strips them too"
        );
        // Eight modifier bits and no more: `1 + 255` is the largest modifier
        // field a terminal can send.
        assert_eq!(
            decode(b"\x1b[99;257u"),
            vec![Input::Action(Action::Ignore)],
            "past eight bits"
        );
        assert_eq!(
            decode(b"\x1b[99;0u"),
            vec![Input::Action(Action::Ignore)],
            "a modifier is 1 + a mask"
        );
    }

    #[test]
    fn a_u_form_key_split_across_reads_decodes_once() {
        // One byte per feed, with the loop's tick between them: the shape a
        // terminal under load really delivers a key report in.
        let mut decoder = Decoder::new();
        let now = Instant::now();
        let mut out = Vec::new();
        for byte in b"\x1b[100;5u" {
            decoder.feed(*byte, now, &mut out);
            decoder.flush(now, &mut out);
        }
        assert_eq!(out, vec![Input::Action(Action::Eof)]);
    }

    #[test]
    fn the_keypad_decodes_as_the_main_row_it_mirrors() {
        // `escape_parser.zig` at `c1db919`, :116-118 and :185-207: a keypad key
        // behaves as its main-row equivalent. The main row here is this
        // decoder's own cursor-key table, so a keypad arrow with Shift is the
        // plain move the main-row arrow is (`csi`'s cursor-key arm) rather
        // than a selection this session does not have.
        for (bytes, expected) in [
            (&b"\x1b[57399u"[..], Input::Text('0')),
            (b"\x1b[57408u", Input::Text('9')),
            (b"\x1b[57409u", Input::Text('.')),
            (b"\x1b[57413;2u", Input::Text('+')),
            (b"\x1b[57415u", Input::Text('=')),
            // Only a bare or shifted keypad press types (:193-195).
            (b"\x1b[57399;5u", Input::Action(Action::Ignore)),
            (b"\x1b[57417u", Input::Action(Action::Left)),
            (b"\x1b[57418;5u", Input::Action(Action::WordRight)),
            (b"\x1b[57419;2u", Input::Action(Action::SelectUp)),
            (b"\x1b[57423u", Input::Action(Action::Home)),
            (b"\x1b[57424u", Input::Action(Action::End)),
            (b"\x1b[57414u", Input::Action(Action::Submit)),
            (b"\x1b[57414;2u", Input::Action(Action::InsertNewline)),
            (b"\x1b[57414;5u", Input::Action(Action::Submit)),
            (b"\x1b[57426u", Input::Action(Action::Delete)),
            (b"\x1b[57426;9u", Input::Action(Action::KillToEnd)),
            (b"\x1b[57352u", Input::Action(Action::Up)),
            (b"\x1b[57353;5u", Input::Action(Action::Down)),
            // Shift+Space types the space (:222).
            (b"\x1b[32;2u", Input::Text(' ')),
            // Hyper (16) is not a keypad modifier (:189-191), so it falls
            // through to `.ignore`.
            (b"\x1b[57399;17u", Input::Action(Action::Ignore)),
        ] {
            assert_eq!(decode(bytes), vec![expected], "{bytes:?}");
        }
    }

    #[test]
    fn what_upstream_resolves_to_a_key_xfx_does_not_have_is_ignored() {
        // Each of these is a real upstream result (`escape_parser.zig` at
        // `c1db919`) with no action in this session's vocabulary, and none is
        // invented for it.
        for (bytes, upstream) in [
            (
                &b"\x1b[97;6u"[..],
                "composerMove line_start + selection :238",
            ),
            (
                b"\x1b[98;6u",
                "composerMove character_left + selection :239",
            ),
            (b"\x1b[101;6u", "composerMove line_end + selection :240"),
            (
                b"\x1b[102;6u",
                "composerMove character_right + selection :241",
            ),
            (b"\x1b[98;4u", "composerMove word_left + selection :244"),
            (b"\x1b[102;4u", "composerMove word_right + selection :248"),
            (
                b"\x1b[111;6u",
                "ctrlOKeyAction with Shift beside Ctrl :111-114",
            ),
            (b"\x1b[111;3u", "ctrlOKeyAction with Alt :111-114"),
            (b"\x1b[114;9u", "open_all_sessions :253"),
            (b"\x1b[57426;2u", "forward delete with Shift :179"),
            (b"\x1b[3;2~", "main-row Delete with Shift alone :179, :697"),
            (b"\x1b[1;5~", "composerMove draft_start :699-702"),
            (b"\x1b[4;3~", "composerMove line_end :703-706"),
            (b"\x1b[57352;17u", "kitty Up with Hyper alone :214"),
        ] {
            assert_eq!(
                decode(bytes),
                vec![Input::Action(Action::Ignore)],
                "{bytes:?} ({upstream})"
            );
        }
    }

    #[test]
    fn alt_b_is_its_own_action_and_the_modified_left_arrows_are_word_left() {
        // Upstream's `.word_left` is Alt-b's alone (`escape_parser.zig:243-246`,
        // `:528-533@481104fd`; kitty and xterm's report share the keycode
        // table); Ctrl-Left and Alt-Left are a composer move (`:83-92`). Only
        // a question's choice reads the two apart ([`Action::AltB`]).
        for (bytes, spelling) in [
            (&b"\x1bb"[..], "ESC b"),
            (b"\x1b[98;3u", "kitty"),
            (b"\x1b[27;3;98~", "modifyOtherKeys"),
        ] {
            assert_eq!(
                decode(bytes),
                vec![Input::Action(Action::AltB)],
                "{spelling}"
            );
        }
        for bytes in [
            &b"\x1b[1;3D"[..],
            b"\x1b[1;5D",
            b"\x1b[1;7D",
            b"\x1b[57417;5u",
        ] {
            assert_eq!(
                decode(bytes),
                vec![Input::Action(Action::WordLeft)],
                "{bytes:?}"
            );
        }
        // Shift with it is a selection move upstream, which xfx has none of.
        assert_eq!(decode(b"\x1b[98;4u"), vec![Input::Action(Action::Ignore)]);
    }

    #[test]
    fn the_forward_word_delete_and_the_page_keys_decode_in_every_spelling() {
        // Each is a real upstream result (`escape_parser.zig` at `c1db919`)
        // that has an action here now.
        for (bytes, action, upstream) in [
            (&b"\x1bd"[..], Action::DeleteWordRight, "ESC d :541-546"),
            (b"\x1bb", Action::AltB, "ESC b :529-534"),
            (b"\x1bf", Action::WordRight, "ESC f :535-540"),
            (b"\x1b[100;3u", Action::DeleteWordRight, "kitty Alt-d :251"),
            (
                b"\x1b[100;7u",
                Action::DeleteWordRight,
                "kitty Ctrl+Alt-d, ahead of Ctrl-D :251",
            ),
            (b"\x1b[3;3~", Action::DeleteWordRight, "Alt-Delete :175-180"),
            (
                b"\x1b[3;5~",
                Action::DeleteWordRight,
                "Ctrl-Delete :175-180",
            ),
            (b"\x1b[3;9~", Action::KillToEnd, "Super-Delete :175-180"),
            (
                b"\x1b[57426;5u",
                Action::DeleteWordRight,
                "keypad Delete :178",
            ),
            (b"\x1b[5~", Action::PageUp, "page_up :632"),
            (b"\x1b[6~", Action::PageDown, "page_down :633"),
            (b"\x1b[5;5~", Action::PageUp, "composerMove page_up :708"),
            (
                b"\x1b[6;2~",
                Action::PageDown,
                "composerMove page_down :709",
            ),
            (b"\x1b[57421u", Action::PageUp, "keypad page_up :169-173"),
            (
                b"\x1b[57422;5u",
                Action::PageDown,
                "composerMove page_down :171",
            ),
            (
                b"\x0a",
                Action::ControlJ,
                "C-j :1220-1235 of app_input_runtime",
            ),
            (b"\x0b", Action::ControlK, "C-k, the same"),
        ] {
            assert_eq!(
                decode(bytes),
                vec![Input::Action(action)],
                "{bytes:?} ({upstream})"
            );
        }
        // Upper case is not the binding, as upstream reads it.
        assert_eq!(decode(b"\x1bB"), vec![Input::Action(Action::Ignore)]);
        assert_eq!(decode(b"\x1bD"), vec![Input::Action(Action::Ignore)]);
    }

    #[test]
    fn super_a_c_and_x_are_the_composers_selection_keys() {
        // `escape_parser.zig` at `c1db919`, :223-231: Super with `a`, `c` or
        // `x` in either case, whatever else is held, in both report spellings.
        for (bytes, action) in [
            (&b"\x1b[97;9u"[..], Action::SelectAll),
            (b"\x1b[65;10u", Action::SelectAll),
            (b"\x1b[99;9u", Action::CopySelection),
            (b"\x1b[99;13u", Action::CopySelection),
            (b"\x1b[120;9u", Action::CutSelection),
            (b"\x1b[27;9;120~", Action::CutSelection),
        ] {
            assert_eq!(decode(bytes), vec![Input::Action(action)], "{bytes:?}");
        }
    }

    #[test]
    fn ctrl_o_and_the_page_keys_are_the_full_transcripts_in_every_spelling_upstream_reads() {
        // `escape_parser.zig` at `c1db919`: the control byte (:270), kitty's
        // Ctrl+O and only Ctrl+O (:111-114, :252), the tilde page keys
        // (:632-633) and the bare keypad ones (:169-173). One action per key
        // whatever the spelling, so the screen has one binding to answer.
        for (bytes, action) in [
            (&b"\x0f"[..], Action::ToggleTranscript),
            (b"\x1b[111;5u", Action::ToggleTranscript),
            (b"\x1b[79;5u", Action::ToggleTranscript),
            (b"\x1b[27;5;111~", Action::ToggleTranscript),
            (b"\x1b[5~", Action::PageUp),
            (b"\x1b[6~", Action::PageDown),
            (b"\x1b[57421u", Action::PageUp),
            (b"\x1b[57422u", Action::PageDown),
        ] {
            assert_eq!(decode(bytes), vec![Input::Action(action)], "{bytes:?}");
        }
        // A spelling of the tilde key no terminal sends is not a page key, for
        // the reason every tilde key is matched as bytes (`csi`).
        assert_eq!(csi(b"05", b'~'), Action::Ignore);
        assert_eq!(csi(b"5;1", b'~'), Action::Ignore);
    }

    #[test]
    fn alt_enter_in_its_legacy_spelling_inserts_a_newline_rather_than_submitting() {
        // `escape_parser.zig` at `c1db919`, :515-519: `ESC CR` and `ESC LF`
        // are Alt-Enter, and Alt-Enter is a newline -- the same key the `u`
        // spelling (`ESC[13;3u`) is. Replayed, it would be an Escape and then
        // a Submit: a prompt sent on a key that means "not yet", and two of
        // them inside the double-Esc window would clear the draft as well.
        assert_eq!(
            decode(b"\x1b\r"),
            vec![Input::Action(Action::InsertNewline)]
        );
        assert_eq!(
            decode(b"\x1b\n"),
            vec![Input::Action(Action::InsertNewline)]
        );
        assert_eq!(
            decode(b"\x1b\r\x1b\r"),
            vec![
                Input::Action(Action::InsertNewline),
                Input::Action(Action::InsertNewline)
            ],
            "two Alt-Enters armed the composer-clearing gesture"
        );
        // Alt-Backspace keeps its own carve-out.
        assert_eq!(
            decode(&[0x1b, 0x7f]),
            vec![Input::Action(Action::DeleteWordBackward)]
        );
    }

    #[test]
    fn a_captured_ctrl_letter_split_across_reads_decodes_once() {
        // The same one-byte-at-a-time discipline
        // `a_redo_sequence_split_across_reads_decodes_once` proves for the
        // two Super+Z spellings, proved here for the three captured
        // Ctrl-letter tilde chords -- `feed` is a byte at a time, which is
        // the shape a terminal really delivers a chord in.
        for (sequence, action) in [
            (&b"\x1b[27;5;99~"[..], Action::Cancel),
            (&b"\x1b[27;5;100~"[..], Action::Eof),
            (&b"\x1b[27;5;117~"[..], Action::KillToStart),
        ] {
            let mut decoder = Decoder::new();
            let now = Instant::now();
            let mut out = Vec::new();
            for byte in sequence {
                decoder.feed(*byte, now, &mut out);
            }
            assert_eq!(
                out,
                vec![Input::Action(action)],
                "{sequence:?} did not decode to one {action:?}"
            );
        }
    }

    #[test]
    fn a_pasted_captured_ctrl_letter_is_content_not_a_cancel() {
        // Paste is data, not keys (module doc, "Paste is data, not keys"):
        // the captured Ctrl-C spelling between the paste markers comes back
        // byte for byte as `PasteByte`, the same as `0x03` already does in
        // `a_paste_still_owns_every_byte_between_its_markers`, and not as
        // `Action::Cancel`.
        let out = decode(b"\x1b[200~\x1b[27;5;99~\x1b[201~");
        let pasted: Vec<u8> = out
            .iter()
            .filter_map(|event| match event {
                Input::PasteByte(byte) => Some(*byte),
                _ => None,
            })
            .collect();
        assert_eq!(
            pasted,
            b"\x1b[27;5;99~".to_vec(),
            "a captured Ctrl-C inside a paste was decoded instead of pasted"
        );
        assert!(
            !out.contains(&Input::Action(Action::Cancel)),
            "a pasted Ctrl-C cancelled something: {out:?}"
        );
    }

    #[test]
    fn a_redo_sequence_split_across_reads_decodes_once() {
        // `feed` takes one byte at a time, which is the shape a terminal really
        // delivers a chord in: a decoder that resolved on a read boundary would
        // read one keystroke as two.
        for sequence in [&b"\x1b[122;10u"[..], &b"\x1b[27;10;122~"[..]] {
            let mut decoder = Decoder::new();
            let now = Instant::now();
            let mut out = Vec::new();
            for byte in sequence {
                decoder.feed(*byte, now, &mut out);
            }
            assert_eq!(
                out,
                vec![Input::Action(Action::Redo)],
                "{sequence:?} did not decode to one redo"
            );
        }
    }

    #[test]
    fn shift_tab_is_one_key_in_all_three_spellings() {
        // `escape_parser.zig` at `c1db919`: `CSI Z` (:569), `CSI 1;<m> Z` with
        // a modifier (:723-725), and kitty's Tab with Shift (:254).
        for bytes in [
            &b"\x1b[Z"[..],
            b"\x1b[1;2Z",
            b"\x1b[1;6Z",
            b"\x1b[9;2u",
            b"\x1b[9;6u",
        ] {
            assert_eq!(
                decode(bytes),
                vec![Input::Action(Action::BackTab)],
                "{bytes:?}"
            );
        }
        // A modifier field that says "no modifier" is not a spelling upstream
        // has, and a parameter that is not a modifier is not Shift-Tab.
        for bytes in [&b"\x1b[1;1Z"[..], b"\x1b[2Z", b"\x1b[1;Z"] {
            assert_eq!(
                decode(bytes),
                vec![Input::Action(Action::Ignore)],
                "{bytes:?}"
            );
        }
        assert_eq!(decode(b"\x1b[9u"), vec![Input::Action(Action::Tab)]);
    }

    #[test]
    fn arrows_arrive_as_csi_and_as_ss3() {
        // escape_parser.zig:456-464, :535-553
        assert_eq!(decode(b"\x1b[A"), vec![Input::Action(Action::Up)]);
        assert_eq!(decode(b"\x1b[D"), vec![Input::Action(Action::Left)]);
        assert_eq!(decode(b"\x1bOC"), vec![Input::Action(Action::Right)]);
    }

    #[test]
    fn home_end_and_the_tilde_keys_are_decoded() {
        // escape_parser.zig:526-533
        assert_eq!(decode(b"\x1b[H"), vec![Input::Action(Action::Home)]);
        assert_eq!(decode(b"\x1b[F"), vec![Input::Action(Action::End)]);
        assert_eq!(decode(b"\x1b[1~"), vec![Input::Action(Action::Home)]);
        assert_eq!(decode(b"\x1b[3~"), vec![Input::Action(Action::Delete)]);
        assert_eq!(decode(b"\x1b[4~"), vec![Input::Action(Action::End)]);
    }

    #[test]
    fn a_modified_arrow_moves_by_word() {
        // escape_parser.zig:52-88; Phase 1 keeps the ctrl/alt word moves and
        // drops the selection bit, which has no selection to extend yet.
        assert_eq!(decode(b"\x1b[1;5C"), vec![Input::Action(Action::WordRight)]);
        assert_eq!(decode(b"\x1b[1;3D"), vec![Input::Action(Action::WordLeft)]);
    }

    #[test]
    fn bracketed_paste_markers_need_exactly_three_digits() {
        // escape_parser.zig:520-524
        assert_eq!(
            decode(b"\x1b[200~"),
            vec![Input::Action(Action::PasteStart)]
        );
        assert_eq!(decode(b"\x1b[201~"), vec![Input::Action(Action::PasteEnd)]);
        assert_eq!(decode(b"\x1b[20~"), vec![Input::Action(Action::Ignore)]);
    }

    #[test]
    fn the_paste_markers_are_matched_by_their_exact_spelling() {
        // A parameter that merely *parses* as 200 is not the marker. The end
        // marker is matched byte for byte inside a paste, so a start matched
        // loosely opens a paste that spelling can never close: the session
        // latches, and every keystroke after it becomes content.
        for opener in [
            &b"\x1b[0200~"[..],
            &b"\x1b[00200~"[..],
            &b"\x1b[200;1~"[..],
            &b"\x1b[2000~"[..],
            &b"\x1b[ 200~"[..],
        ] {
            let mut decoder = Decoder::new();
            let now = Instant::now();
            let mut out = Vec::new();
            for byte in opener {
                decoder.feed(*byte, now, &mut out);
            }
            assert_eq!(out, vec![Input::Action(Action::Ignore)], "{opener:?}");
            assert!(!decoder.pasting(), "a paste was opened by {opener:?}");
        }
        for closer in [&b"\x1b[0201~"[..], &b"\x1b[201;1~"[..], &b"\x1b[2010~"[..]] {
            assert_eq!(
                decode(closer),
                vec![Input::Action(Action::Ignore)],
                "{closer:?}"
            );
        }
        // The exact spellings, and the same strictness from inside a paste: a
        // variant end marker is content, and the real one still closes.
        let mut decoder = Decoder::new();
        let now = Instant::now();
        let mut out = Vec::new();
        for byte in b"\x1b[200~\x1b[0201~\x1b[201~" {
            decoder.feed(*byte, now, &mut out);
        }
        assert_eq!(
            deliver(&mut decoder, &mut out),
            Delivery::Pasted { dropped: 0 }
        );
        assert!(!decoder.pasting(), "the exact end marker did not close");
        assert_eq!(
            out.iter()
                .filter(|event| matches!(event, Input::PasteByte(_)))
                .count(),
            7,
            "the variant end marker was not content: {out:?}"
        );
        assert_eq!(
            out.first(),
            Some(&Input::Action(Action::PasteStart)),
            "{out:?}"
        );
        assert_eq!(
            out.last(),
            Some(&Input::Action(Action::PasteEnd)),
            "{out:?}"
        );
    }

    #[test]
    fn a_sequence_is_a_key_only_in_the_exact_shapes_a_terminal_sends_one() {
        // `CSI 999 C` is a cursor-forward *command*, not the Right arrow, and
        // `CSI 1;5;9 C` is a shape this session cannot name. Reading either as
        // movement is guessing, and the answer to input this decoder cannot
        // name is the same everywhere: `Ignore`.
        for stream in [
            &b"\x1b[999C"[..],
            &b"\x1b[1;5;9C"[..],
            &b"\x1b[2;5C"[..],
            &b"\x1b[;5C"[..],
            &b"\x1b[1;C"[..],
            &b"\x1b[1;5:3C"[..],
            &b"\x1b[1;17C"[..],
            // The tilde keys are exact in the same way, and for the same
            // reason the paste markers are: one spelling each, so a stream
            // cannot reach a binding by a route the rest of the decoder does
            // not recognize.
            &b"\x1b[01~"[..],
            &b"\x1b[03~"[..],
            &b"\x1b[04~"[..],
            &b"\x1b[1;1~"[..],
            &b"\x1b[3;05~"[..],
            &b"\x1b[3;5;1~"[..],
            &b"\x1b[30~"[..],
            &b"\x1b[~"[..],
        ] {
            assert_eq!(
                decode(stream),
                vec![Input::Action(Action::Ignore)],
                "{stream:?}"
            );
        }
        // The two shapes that are keys, including the shift modifier that is
        // dropped down to a plain move -- on Left and Right; on Up and Down it
        // is the selection move a question's field binds.
        assert_eq!(decode(b"\x1b[C"), vec![Input::Action(Action::Right)]);
        assert_eq!(decode(b"\x1b[1;2C"), vec![Input::Action(Action::Right)]);
        assert_eq!(decode(b"\x1b[1;2A"), vec![Input::Action(Action::SelectUp)]);
        assert_eq!(
            decode(b"\x1b[1;2B"),
            vec![Input::Action(Action::SelectDown)]
        );
        assert_eq!(decode(b"\x1b[1;6A"), vec![Input::Action(Action::SelectUp)]);
        assert_eq!(decode(b"\x1b[1;5A"), vec![Input::Action(Action::Up)]);
        assert_eq!(decode(b"\x1b[1;5C"), vec![Input::Action(Action::WordRight)]);
        assert_eq!(decode(b"\x1b[1;16D"), vec![Input::Action(Action::WordLeft)]);
        assert_eq!(decode(b"\x1b[1;5H"), vec![Input::Action(Action::Home)]);
        assert_eq!(decode(b"\x1b[1~"), vec![Input::Action(Action::Home)]);
        assert_eq!(decode(b"\x1b[3~"), vec![Input::Action(Action::Delete)]);
        assert_eq!(decode(b"\x1b[4~"), vec![Input::Action(Action::End)]);
    }

    #[test]
    fn an_unknown_csi_is_discarded_and_never_becomes_a_phantom_escape() {
        // terminal_action_decoder.zig test :342-360
        assert_eq!(decode(b"\x1b[>4;2m"), vec![Input::Action(Action::Ignore)]);
        let long: Vec<u8> = b"\x1b["
            .iter()
            .copied()
            .chain(std::iter::repeat_n(b'0', 64))
            .collect();
        let events = decode(&long);
        assert!(
            events
                .iter()
                .all(|event| *event != Input::Action(Action::Escape)),
            "a bounded discard produced an Escape: {events:?}"
        );
    }

    #[test]
    fn a_byte_no_sequence_can_carry_is_not_eaten_by_one() {
        // Everything between `CSI` and its final byte is printable ASCII, so a
        // C0 arriving mid-sequence is a keystroke and not part of it. The
        // sequence is abandoned as unknown and the keystroke survives: a
        // Ctrl-C the user typed is a Ctrl-C whatever the terminal was in the
        // middle of saying.
        for stream in [&b"\x1b[1\x03"[..], &b"\x1bO\x03"[..]] {
            assert_eq!(
                decode(stream),
                vec![Input::Action(Action::Ignore), Input::Action(Action::Cancel)],
                "{stream:?}"
            );
        }
        // And after the bound has already given up on the sequence.
        let long: Vec<u8> = b"\x1b["
            .iter()
            .copied()
            .chain(std::iter::repeat_n(b'0', 40))
            .chain(std::iter::once(0x03))
            .collect();
        assert_eq!(
            decode(&long).last(),
            Some(&Input::Action(Action::Cancel)),
            "a keystroke was eaten by a sequence the decoder had already \
             stopped reading"
        );
    }

    #[test]
    fn a_truncated_scalar_does_not_eat_the_key_that_interrupted_it() {
        // `0xed` promises two continuation bytes and `0x03` is not one of
        // them. The half-scalar is dropped -- there is no character in it --
        // and the byte that ended it is decoded on its own.
        assert_eq!(decode(&[0xed, 0x03]), vec![Input::Action(Action::Cancel)]);
    }

    #[test]
    fn a_sequence_that_never_ends_gives_the_keyboard_back() {
        // The other half of the bound: an unbounded discard hands the keyboard
        // to a terminal that emits parameters and never a final byte, because
        // every keystroke after it is read as more of a sequence that is never
        // going to finish.
        let mut decoder = Decoder::new();
        let now = Instant::now();
        let mut out = Vec::new();
        for byte in b"\x1b[".iter().chain(std::iter::repeat_n(&b'0', 128)) {
            decoder.feed(*byte, now, &mut out);
        }
        out.clear();
        decoder.feed(b'a', now, &mut out);
        assert_eq!(
            out,
            vec![Input::Text('a')],
            "a keystroke after an unterminated sequence was eaten by it"
        );
    }

    #[test]
    fn a_bare_escape_followed_by_a_control_byte_emits_both_in_order() {
        // terminal_action_decoder.zig:105-114 -- the replay is why Esc keeps
        // its meaning when the byte after it is not part of a sequence.
        assert_eq!(
            decode(&[0x1b, 0x03]),
            vec![Input::Action(Action::Escape), Input::Action(Action::Cancel)]
        );
    }

    #[test]
    fn alt_backspace_deletes_a_word_instead_of_firing_two_escapes() {
        // The one carve-out in the replay rule, and it is a safety property
        // rather than a nicety. `ESC 0x7f` is how every terminal in use spells
        // Alt-Backspace, and the replay would decode one keystroke as
        // `Escape` + `Backspace` -- so two Alt-Backspaces are two Escapes
        // inside the double-Esc window, and the gesture that clears the
        // composer would fire on a keystroke whose whole purpose is to delete
        // one word and keep the rest.
        assert_eq!(
            decode(&[0x1b, 0x7f]),
            vec![Input::Action(Action::DeleteWordBackward)]
        );
        assert_eq!(
            decode(&[0x1b, 0x7f, 0x1b, 0x7f]),
            vec![
                Input::Action(Action::DeleteWordBackward),
                Input::Action(Action::DeleteWordBackward)
            ],
            "two Alt-Backspaces armed the composer-clearing gesture"
        );
        // `ESC 0x08` is the same key on a terminal whose Backspace sends
        // `C-h` (`escape_parser.zig` at `c1db919`, :549).
        assert_eq!(
            decode(&[0x1b, 0x08]),
            vec![Input::Action(Action::DeleteWordBackward)]
        );
        // And the replay itself is untouched for every other byte in that
        // position, which is what keeps this a carve-out rather than a repeal.
        assert_eq!(
            decode(&[0x1b, 0x02]),
            vec![Input::Action(Action::Escape), Input::Action(Action::Left)]
        );
    }

    #[test]
    fn a_lone_escape_resolves_only_after_the_quiet_timeout() {
        let mut decoder = Decoder::new();
        let start = Instant::now();
        let mut out = Vec::new();
        decoder.feed(0x1b, start, &mut out);
        assert!(out.is_empty(), "Esc resolved before the timeout: {out:?}");
        decoder.flush(start + Duration::from_millis(10), &mut out);
        assert!(out.is_empty());
        decoder.flush(start + Decoder::ESC_TIMEOUT, &mut out);
        assert_eq!(out, vec![Input::Action(Action::Escape)]);
    }

    #[test]
    fn the_emacs_control_table_is_ported_verbatim() {
        // shortcuts.zig:11-30
        for (byte, action) in [
            (0x01, Action::Home),
            (0x05, Action::End),
            (0x02, Action::Left),
            (0x06, Action::Right),
            (0x04, Action::Eof),
            // `C-k` and `C-j` are the kill and the newline in the composer,
            // and walk a menu that is up, so they decode to their keys
            // (`Action::ControlK`, `Action::ControlJ`).
            (0x0b, Action::ControlK),
            (0x15, Action::KillToStart),
            (0x17, Action::DeleteWordLeft),
            (0x0c, Action::Redraw),
            (0x0a, Action::ControlJ),
            (0x0d, Action::Submit),
            (0x03, Action::Cancel),
            (0x7f, Action::Backspace),
            (0x08, Action::Backspace),
        ] {
            assert_eq!(
                decode(&[byte]),
                vec![Input::Action(action)],
                "byte {byte:#04x}"
            );
        }
    }

    #[test]
    fn one_byte_produces_at_most_one_event() {
        // input_action.zig:143-160. The escape replay above is the one
        // documented pair, and it is two events for *two* bytes.
        let mut decoder = Decoder::new();
        let now = Instant::now();
        for byte in b"\x1b[1;5Cabc\x1b[200~x\x1b[201~" {
            let mut out = Vec::new();
            decoder.feed(*byte, now, &mut out);
            assert!(out.len() <= 1, "byte {byte:#04x} produced {out:?}");
        }
    }

    #[test]
    fn text_arrives_as_characters_and_multibyte_utf8_is_assembled() {
        assert_eq!(decode("a".as_bytes()), vec![Input::Text('a')]);
        assert_eq!(decode("한".as_bytes()), vec![Input::Text('한')]);
        // A partial sequence produces nothing until it completes, and an
        // invalid one is dropped rather than becoming a replacement character
        // the user did not type (`text_scalar.zig:146-180`).
        assert_eq!(decode(&[0xff]), vec![]);
    }

    #[test]
    fn between_the_paste_markers_every_byte_is_content() {
        let mut decoder = Decoder::new();
        let now = Instant::now();
        let mut out = Vec::new();
        for byte in b"\x1b[200~a\x03\x1b[Ab\x1b[201~" {
            decoder.feed(*byte, now, &mut out);
        }
        let inner: Vec<Input> = out
            .into_iter()
            .filter(|event| !matches!(event, Input::Action(Action::PasteStart | Action::PasteEnd)))
            .collect();
        assert!(
            inner
                .iter()
                .all(|event| matches!(event, Input::PasteByte(_))),
            "a pasted control byte was decoded as a key: {inner:?}"
        );
    }

    // --- the control policy (BINDING, `progress.md` "T10 control policy") ---

    #[test]
    fn every_control_byte_and_del_decodes_to_its_exact_row() {
        // The whole C0 range plus DEL, one row each, asserted as an exact
        // equality. A test that only sampled the range, or only asserted that
        // nothing became text, would pass with a binding flipped -- `0x11`
        // answering `Cancel` is as wrong as `0x09` answering `Text`, and only
        // an exhaustive table says so. `0x09` is `Tab` and not `Ignore`: the
        // approval panel walks its choices with it, and the composer has no
        // binding for it, so it is still a keystroke that types nothing.
        let named = [
            (0x01u8, Action::Home),
            (0x02, Action::Left),
            (0x03, Action::Cancel),
            (0x04, Action::Eof),
            (0x05, Action::End),
            (0x06, Action::Right),
            (0x08, Action::Backspace),
            (0x09, Action::Tab),
            (0x0a, Action::ControlJ),
            (0x0b, Action::ControlK),
            (0x0c, Action::Redraw),
            (0x0d, Action::Submit),
            (0x0e, Action::HistoryNext),
            (0x0f, Action::ToggleTranscript),
            (0x10, Action::ControlP),
            (0x15, Action::KillToStart),
            (0x16, Action::PasteImage),
            (0x17, Action::DeleteWordLeft),
            (0x19, Action::Yank),
            (0x1f, Action::Undo),
            (0x7f, Action::Backspace),
        ];
        for byte in (0x00u8..=0x1f).chain(std::iter::once(0x7f)) {
            let expected = if byte == 0x1b {
                // ESC is the one byte that resolves on the clock rather than on
                // arrival, so on its own it is not an event yet.
                Vec::new()
            } else {
                let action = named
                    .iter()
                    .find(|(named, _)| *named == byte)
                    .map_or(Action::Ignore, |(_, action)| *action);
                vec![Input::Action(action)]
            };
            assert_eq!(
                decode(&[byte]),
                expected,
                "byte {byte:#04x} does not decode to its row: a control is a \
                 binding or it is `Ignore`, and never text"
            );
        }
    }

    #[test]
    fn the_two_history_keys_are_bindings_rather_than_arrow_keys() {
        // `C-p` and `C-n` are the recall on every shell that has one, and they
        // are **unconditional** here in a way the arrow keys are not: an arrow
        // is the composer's until the caret has nowhere left to go
        // (`super::shell`), so a decoder that answered these two with `Up` and
        // `Down` would make the recall unreachable from the middle of a
        // multi-row draft -- which is exactly the draft a user reaches for it
        // from.
        assert_eq!(decode(&[0x10]), vec![Input::Action(Action::ControlP)]);
        assert_eq!(decode(&[0x0e]), vec![Input::Action(Action::HistoryNext)]);
        assert_ne!(
            decode(&[0x10]),
            decode(b"\x1b[A"),
            "the recall key decodes to the same action the up arrow does"
        );
        assert_ne!(
            decode(&[0x0e]),
            decode(b"\x1b[B"),
            "the recall key decodes to the same action the down arrow does"
        );
    }

    #[test]
    fn a_c1_control_spelled_in_utf8_is_not_text_either() {
        // U+0085 is one keystroke away from being a line break the terminal
        // obeys, and it arrives as two perfectly valid UTF-8 bytes.
        assert_eq!(
            decode("\u{85}".as_bytes()),
            vec![Input::Action(Action::Ignore)]
        );
        assert_eq!(
            decode("\u{9b}".as_bytes()),
            vec![Input::Action(Action::Ignore)]
        );
    }

    // --- paste: content, in order, and bounded ---

    #[test]
    fn a_broken_end_marker_prefix_comes_back_as_content_in_order() {
        // The bytes of an unfinished `ESC [ 2 0 1 ~` are held back, because a
        // marker that arrives one byte at a time must not be typed into the
        // composer. When the prefix breaks they were content after all, and
        // they come out ahead of the byte that broke it.
        let mut decoder = Decoder::new();
        let now = Instant::now();
        let mut out = Vec::new();
        for byte in b"\x1b[200~\x1b[2X\x1b[201~" {
            decoder.feed(*byte, now, &mut out);
        }
        deliver(&mut decoder, &mut out);
        assert_eq!(
            out,
            vec![
                Input::Action(Action::PasteStart),
                Input::PasteByte(0x1b),
                Input::PasteByte(b'['),
                Input::PasteByte(b'2'),
                Input::PasteByte(b'X'),
                Input::Action(Action::PasteEnd),
            ]
        );
    }

    #[test]
    fn a_prefix_broken_by_another_escape_still_finds_the_end_marker() {
        // Pasted text ending in `ESC [ 2` immediately before the real end
        // marker. The byte that breaks the prefix opens one of its own, and a
        // decoder that did not notice would never see the marker at all: the
        // paste would stay open and every keystroke after it would be content.
        let mut decoder = Decoder::new();
        let now = Instant::now();
        let mut out = Vec::new();
        for byte in b"\x1b[200~\x1b[2\x1b[201~" {
            decoder.feed(*byte, now, &mut out);
        }
        deliver(&mut decoder, &mut out);
        assert_eq!(
            out,
            vec![
                Input::Action(Action::PasteStart),
                Input::PasteByte(0x1b),
                Input::PasteByte(b'['),
                Input::PasteByte(b'2'),
                Input::Action(Action::PasteEnd),
            ]
        );
        assert!(!decoder.pasting(), "the paste never ended");
    }

    #[test]
    fn no_stream_produces_more_events_than_it_has_bytes() {
        // The honest form of the one-byte invariant. Two cases hand out more
        // than one event for a single byte -- the escape replay, and the flush
        // of a broken paste-end prefix -- and both are paid for by bytes that
        // produced none, so the total never grows.
        for stream in [
            b"\x1b[200~\x1b[2X\x1b[201~".to_vec(),
            b"\x1b\x03\x1b\x1b\x1b[201~".to_vec(),
            b"\x1b[200~\x1b[201".to_vec(),
            "\x1b[1;5C한\x1b[200~a\x1b[201~\x04".as_bytes().to_vec(),
        ] {
            let mut decoder = Decoder::new();
            let now = Instant::now();
            let mut out = Vec::new();
            for byte in &stream {
                decoder.feed(*byte, now, &mut out);
            }
            deliver(&mut decoder, &mut out);
            assert!(
                out.len() <= stream.len(),
                "{} bytes produced {} events: {out:?}",
                stream.len(),
                out.len()
            );
        }
    }

    /// A deterministic byte source: xorshift64\*, seeded per case.
    ///
    /// Written out rather than pulled in, because a decoder property that only
    /// holds when an external generator is present is a property that stops
    /// being checked the first time the dependency is trimmed. Same seeds,
    /// same streams, on every machine and every run.
    struct Noise(u64);

    impl Noise {
        fn next(&mut self) -> u64 {
            let mut state = self.0;
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            self.0 = state;
            state.wrapping_mul(0x2545_f491_4f6c_dd1d)
        }
    }

    #[test]
    fn the_cumulative_bound_holds_at_every_prefix_of_deterministic_noise() {
        // The invariant, checked where hand-written cases cannot reach: the
        // overlaps. An intruding byte inside a sequence that is inside a paste
        // whose end-marker prefix is half-matched, with a timeout flush landing
        // between two bytes of it -- each one is accounted for on its own
        // above, and this is what says the accounting composes.
        //
        // The alphabet is weighted towards the bytes that make those overlaps
        // happen: uniform noise is almost all `Ignore` and would exercise
        // nothing.
        const ALPHABET: &[u8] = b"\x1b[O~;0123456789ABCDHFmx \x03\x04\x0d\x7f\xed\x80\xc2\x85";
        for seed in 1..=64u64 {
            let mut noise = Noise(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
            let mut decoder = Decoder::new();
            let start = Instant::now();
            let mut out = Vec::new();
            let mut fed = 0usize;
            let mut behind = 0usize;
            for step in 0..512 {
                let roll = noise.next();
                let bytes: &[u8] = match roll % 16 {
                    0 => b"\x1b[200~",
                    1 => b"\x1b[201~",
                    2 => b"\x1b[2",
                    3 => b"\x1b[0200~",
                    _ => {
                        let at = usize::try_from(roll >> 8).unwrap_or(0) % ALPHABET.len();
                        &ALPHABET[at..=at]
                    }
                };
                for byte in bytes {
                    decoder.feed(*byte, start, &mut out);
                    fed += 1;
                    assert!(
                        out.len() <= fed,
                        "seed {seed} step {step}: {fed} bytes produced {} events",
                        out.len()
                    );
                }
                // A tick, sometimes past the escape timeout. The `Escape` a
                // flush emits is paid for by the ESC byte that produced none,
                // so it cannot break the bound either.
                if roll.is_multiple_of(5) {
                    decoder.flush(start + Decoder::ESC_TIMEOUT * 2, &mut out);
                    assert!(
                        out.len() <= fed,
                        "seed {seed} step {step}: a flush pushed the events past \
                         the {fed} bytes that were fed"
                    );
                }
                // And the end of a delivery, half the time, on bits the chunk
                // was not chosen by: right behind an end marker, a paste with
                // nothing behind it; a chunk later, one with input behind it.
                // The `PasteEnd` either owes is paid for by the marker.
                if (roll >> 6).is_multiple_of(2) {
                    if let Delivery::Pasted { dropped: 1.. } = deliver(&mut decoder, &mut out) {
                        behind += 1;
                    }
                    assert!(
                        out.len() <= fed,
                        "seed {seed} step {step}: a delivery's end pushed the events \
                         past the {fed} bytes that were fed"
                    );
                }
            }
            // And the control policy over the same streams: whatever the noise
            // spelled, none of it arrived as a control character in the text.
            assert!(
                !out.iter()
                    .any(|event| matches!(event, Input::Text(text) if text.is_control())),
                "seed {seed}: a control scalar was decoded as text"
            );
            // A property test that stopped reaching the interesting states
            // would keep passing and say nothing, so each stream states what it
            // covered. Floors well under what the generator produces, not exact
            // counts: this is a vacuousness guard, not a change detector.
            let counted =
                |wanted: &dyn Fn(&Input) -> bool| out.iter().filter(|e| wanted(e)).count();
            let pasted = counted(&|event| matches!(event, Input::PasteByte(_)));
            let opened = counted(&|event| *event == Input::Action(Action::PasteStart));
            let closed = counted(&|event| *event == Input::Action(Action::PasteEnd));
            let ignored = counted(&|event| *event == Input::Action(Action::Ignore));
            let typed = counted(&|event| matches!(event, Input::Text(_)));
            assert!(
                pasted >= 100
                    && opened >= 5
                    && closed >= 1
                    && behind >= 1
                    && ignored >= 5
                    && typed >= 30,
                "seed {seed} exercised too little to mean anything: {pasted} pasted \
                 bytes, {opened} opened, {closed} closed, {behind} with input behind, {ignored} \
                 ignored, {typed} typed"
            );
        }
    }

    #[test]
    fn the_paste_flag_is_set_from_the_start_marker_to_the_end_of_the_delivery() {
        let mut decoder = Decoder::new();
        let now = Instant::now();
        let mut out = Vec::new();
        assert!(!decoder.pasting());
        for byte in b"\x1b[200~" {
            decoder.feed(*byte, now, &mut out);
        }
        assert!(decoder.pasting(), "the start marker did not open a paste");
        decoder.feed(0x03, now, &mut out);
        assert!(decoder.pasting());
        for byte in b"\x1b[201~" {
            decoder.feed(*byte, now, &mut out);
        }
        // Behind its end marker the paste still owns the input, until the
        // delivery that carried the marker is over.
        assert!(
            decoder.pasting(),
            "the end marker gave the input back early"
        );
        assert_eq!(
            deliver(&mut decoder, &mut out),
            Delivery::Pasted { dropped: 0 }
        );
        assert!(
            !decoder.pasting(),
            "the delivery's end did not close the paste"
        );
    }

    // --- a paste ends with its delivery (fx v0.0.16, `paste_framing.zig:58-165@481104fd`) ---

    /// `reads` fed one after another as **one** delivery, and then its end.
    fn delivered(reads: &[&[u8]]) -> (Vec<Input>, Delivery) {
        let mut decoder = Decoder::new();
        let now = Instant::now();
        let mut out = Vec::new();
        for read in reads {
            for byte in *read {
                decoder.feed(*byte, now, &mut out);
            }
        }
        let delivery = deliver(&mut decoder, &mut out);
        assert!(!decoder.pasting(), "a settled paste still owns the input");
        (out, delivery)
    }

    /// What a paste of `new` leaves before its delivery says how it ended.
    fn pasted_new() -> Vec<Input> {
        vec![
            Input::Action(Action::PasteStart),
            Input::PasteByte(b'n'),
            Input::PasteByte(b'e'),
            Input::PasteByte(b'w'),
        ]
    }

    /// What a paste of `new` that stands leaves: its bytes, then its end --
    /// and nothing else, whatever arrived behind its marker.
    fn kept_new() -> Vec<Input> {
        let mut kept = pasted_new();
        kept.push(Input::Action(Action::PasteEnd));
        kept
    }

    #[test]
    fn an_enter_in_the_read_that_ended_a_paste_keeps_the_paste_and_is_never_a_key() {
        // fx v0.0.16's capture (`fx-p5-input-paste-40x120-paste-enter`): one
        // buffer, the paste and an Enter behind it. The Enter is the
        // clipboard's, so it is not `Submit`; the paste it followed stands
        // (`app_input_runtime.zig:9420-9431@481104fd`).
        let (out, delivery) = delivered(&[b"\x1b[200~new\x1b[201~\r"]);
        assert_eq!(
            out,
            kept_new(),
            "the Enter got through, or the paste did not stand"
        );
        assert_eq!(delivery, Delivery::Pasted { dropped: 1 });
    }

    #[test]
    fn a_printable_key_behind_the_end_marker_is_dropped_rather_than_typed() {
        let (out, delivery) = delivered(&[b"\x1b[200~new\x1b[201~x"]);
        assert_eq!(out, kept_new(), "the byte behind the marker was typed");
        assert_eq!(delivery, Delivery::Pasted { dropped: 1 });
    }

    #[test]
    fn an_escape_sequence_behind_the_end_marker_is_counted_and_never_decoded() {
        // Not an arrow key, not a second marker, not a new paste: every byte
        // behind the end marker is the same thing (`paste_framing.zig:102-107,
        // 527-546@481104fd`).
        let (out, delivery) = delivered(&[b"\x1b[200~new\x1b[201~\x1b[A"]);
        assert_eq!(
            out,
            kept_new(),
            "the sequence behind the marker was decoded"
        );
        assert!(
            !out.contains(&Input::Action(Action::Up)),
            "an arrow behind the marker moved something: {out:?}"
        );
        assert_eq!(delivery, Delivery::Pasted { dropped: 3 });

        let behind = b"\x1b[201~/version\r\x1b[200~x\x1b[201~";
        let (out, delivery) = delivered(&[b"\x1b[200~new\x1b[201~", behind]);
        assert_eq!(out, kept_new(), "a marker behind the marker was obeyed");
        assert_eq!(
            delivery,
            Delivery::Pasted {
                dropped: behind.len()
            }
        );

        // And the machine is at ground again once the delivery is over: the
        // same arrow, in a delivery of its own, is the arrow.
        let mut decoder = Decoder::new();
        let now = Instant::now();
        let mut out = Vec::new();
        for byte in b"\x1b[200~new\x1b[201~\x1b[" {
            decoder.feed(*byte, now, &mut out);
        }
        assert_eq!(
            deliver(&mut decoder, &mut out),
            Delivery::Pasted { dropped: 2 }
        );
        assert_eq!(
            out,
            kept_new(),
            "the half sequence behind the marker was read"
        );
        out.clear();
        for byte in b"\x1b[A" {
            decoder.feed(*byte, now, &mut out);
        }
        assert_eq!(out, vec![Input::Action(Action::Up)]);
    }

    #[test]
    fn a_second_paste_in_the_delivery_that_ended_the_first_is_not_taken() {
        // Its start marker arrives behind the first one's end marker, so it is
        // counted like any byte there: no second `PasteStart`, none of its
        // bytes, and its own end marker no second `PasteEnd`.
        let behind = b"\x1b[200~two\x1b[201~\r";
        let mut bytes = b"\x1b[200~new\x1b[201~".to_vec();
        bytes.extend(behind);
        let (out, delivery) = delivered(&[&bytes]);
        assert_eq!(out, kept_new(), "the second paste or its Enter was taken");
        assert_eq!(
            delivery,
            Delivery::Pasted {
                dropped: behind.len()
            }
        );
    }

    #[test]
    fn a_paste_with_nothing_behind_its_end_marker_stands() {
        let (out, delivery) = delivered(&[b"\x1b[200~new\x1b[201~"]);
        assert_eq!(out, kept_new());
        assert_eq!(delivery, Delivery::Pasted { dropped: 0 });
    }

    #[test]
    fn an_end_marker_split_across_two_deliveries_is_the_marker_and_the_next_key_a_key() {
        // A marker that lands across two reads -- or two deliveries -- is not
        // input behind the marker: it *is* the marker. The paste stays open
        // over the first delivery's end and stands at the second's, and an
        // Enter in a delivery after that is the user's own key
        // (`paste_framing.zig:161-162,299-312@481104fd`, its test `:502-511`).
        let mut decoder = Decoder::new();
        let now = Instant::now();
        let mut out = Vec::new();
        for byte in b"\x1b[200~new\x1b[20" {
            decoder.feed(*byte, now, &mut out);
        }
        assert_eq!(deliver(&mut decoder, &mut out), Delivery::Quiet);
        assert!(decoder.pasting(), "a delivery's end closed an open paste");
        for byte in b"1~" {
            decoder.feed(*byte, now, &mut out);
        }
        assert_eq!(
            deliver(&mut decoder, &mut out),
            Delivery::Pasted { dropped: 0 }
        );
        assert_eq!(out, kept_new(), "the split marker was taken as content");

        out.clear();
        decoder.feed(b'\r', now, &mut out);
        assert_eq!(deliver(&mut decoder, &mut out), Delivery::Quiet);
        assert_eq!(out, vec![Input::Action(Action::Submit)]);

        // The same marker split across two reads of **one** delivery.
        let (out, delivery) = delivered(&[b"\x1b[200~new\x1b[2", b"01~"]);
        assert_eq!(out, kept_new());
        assert_eq!(delivery, Delivery::Pasted { dropped: 0 });

        // And split across two deliveries with an Enter behind its second
        // half: the Enter is behind the marker, so it is counted.
        let mut decoder = Decoder::new();
        let mut out = Vec::new();
        for byte in b"\x1b[200~new\x1b[2" {
            decoder.feed(*byte, now, &mut out);
        }
        assert_eq!(deliver(&mut decoder, &mut out), Delivery::Quiet);
        for byte in b"01~\r" {
            decoder.feed(*byte, now, &mut out);
        }
        assert_eq!(
            deliver(&mut decoder, &mut out),
            Delivery::Pasted { dropped: 1 }
        );
        assert_eq!(
            out,
            kept_new(),
            "the Enter behind a split marker got through"
        );
    }

    #[test]
    fn a_delivery_is_every_read_up_to_the_drain_and_not_one_read() {
        // A read takes at most `BURST_BYTES`, so a paste whose end marker is
        // the last bytes of one read can have its Enter at the start of the
        // next -- the same write, the same delivery. Settling per read would
        // let that Enter through.
        let (out, delivery) = delivered(&[b"\x1b[200~new\x1b[201~", b"\r"]);
        assert_eq!(out, kept_new(), "the Enter one read behind got through");
        assert_eq!(delivery, Delivery::Pasted { dropped: 1 });
    }

    #[test]
    fn an_escape_that_went_quiet_resolves_before_the_byte_that_follows_it() {
        // The loop flushes on every turn, so this is a second lock on the same
        // door: a byte that arrives a timeout later than the ESC cannot be
        // read as part of a sequence with it.
        let mut decoder = Decoder::new();
        let start = Instant::now();
        let mut out = Vec::new();
        decoder.feed(0x1b, start, &mut out);
        decoder.feed(b'[', start + Decoder::ESC_TIMEOUT, &mut out);
        assert_eq!(
            out,
            vec![Input::Action(Action::Escape), Input::Text('[')],
            "a quiet Escape swallowed the keystroke that came after it"
        );
    }

    #[test]
    fn an_unfinished_sequence_is_not_resolved_by_the_timeout() {
        // Only a bare ESC is ambiguous. A `CSI` that has not reached its final
        // byte is a sequence still arriving, and resolving it on a clock would
        // break every arrow key that lands across two reads.
        let mut decoder = Decoder::new();
        let start = Instant::now();
        let mut out = Vec::new();
        for byte in b"\x1b[1" {
            decoder.feed(*byte, start, &mut out);
        }
        decoder.flush(start + Decoder::ESC_TIMEOUT * 4, &mut out);
        assert!(
            out.is_empty(),
            "an arriving sequence was cut short: {out:?}"
        );
        decoder.feed(b';', start, &mut out);
        decoder.feed(b'5', start, &mut out);
        decoder.feed(b'C', start, &mut out);
        assert_eq!(out, vec![Input::Action(Action::WordRight)]);
    }

    #[test]
    fn a_theme_notification_is_a_report_rather_than_a_key() {
        // The two spellings upstream's monitor sends (`theme_monitor.zig:288-289`),
        // and what they have to become here: a `Report`, so that the surfaces
        // which swallow every key they do not bind cannot swallow this. Both
        // directions, because one arm mapped to the wrong mode repaints a light
        // terminal in the dark greys and nothing else in the session notices.
        assert_eq!(
            decode(b"\x1b[?997;1n"),
            vec![Input::Report(super::super::theme::Mode::Dark)]
        );
        assert_eq!(
            decode(b"\x1b[?997;2n"),
            vec![Input::Report(super::super::theme::Mode::Light)]
        );
    }

    #[test]
    fn a_notification_split_across_two_reads_is_still_one_report() {
        // A report is eight bytes of terminal output and nothing makes it
        // arrive in one read: under load the kernel hands over whatever it has.
        // Split, and with a whole `ESC_TIMEOUT` of quiet in between -- which is
        // the clock the decoder does resolve things on -- it is still the one
        // event, and it is still not two keystrokes.
        let mut decoder = Decoder::new();
        let start = Instant::now();
        let mut out = Vec::new();
        for byte in b"\x1b[?99" {
            decoder.feed(*byte, start, &mut out);
        }
        assert!(out.is_empty(), "half a report became events: {out:?}");
        let later = start + Decoder::ESC_TIMEOUT * 4;
        decoder.flush(later, &mut out);
        assert!(out.is_empty(), "the clock cut a report in half: {out:?}");
        for byte in b"7;2n" {
            decoder.feed(*byte, later, &mut out);
        }
        assert_eq!(
            out,
            vec![Input::Report(super::super::theme::Mode::Light)],
            "a report that arrived in two reads was not one report"
        );
    }

    #[test]
    fn the_same_bytes_inside_a_paste_are_content() {
        // The paste rule is not "every sequence but this one": between the
        // markers there is nothing to decode, so a file carrying the *text* of
        // a theme report pastes as those characters rather than repainting the
        // band. Uninterpreted bytes, exactly as `Ctrl-C` and an arrow key are.
        let events = decode(b"\x1b[200~\x1b[?997;2n\x1b[201~");
        let expected: Vec<Input> = std::iter::once(Input::Action(Action::PasteStart))
            .chain(b"\x1b[?997;2n".iter().map(|byte| Input::PasteByte(*byte)))
            .chain(std::iter::once(Input::Action(Action::PasteEnd)))
            .collect();
        assert_eq!(events, expected);
    }

    #[test]
    fn a_report_the_protocol_does_not_spell_that_way_is_ignored_and_costs_no_keystroke() {
        // Every near miss, and the same two things have to be true of each: it
        // is not a `Report` -- a band repainted on a sequence no terminal sends
        // is a band repainted on noise -- and the key behind it still reaches
        // the composer. The second half is what a decoder that "gave up" by
        // discarding to the next final byte would fail.
        for malformed in [
            // A mode this protocol has no meaning for.
            &b"\x1b[?997;3n"[..],
            // The *query*, echoed back rather than answered.
            b"\x1b[?996;1n",
            // The report with a parameter nobody sends.
            b"\x1b[?997;2;0n",
            // The report without its private marker, which is a different
            // sequence entirely.
            b"\x1b[997;2n",
            // The report with a leading zero, which parses to the same number
            // and is not the spelling.
            b"\x1b[?0997;2n",
        ] {
            let events = decode(&[malformed, b"k"].concat());
            assert_eq!(
                events,
                vec![Input::Action(Action::Ignore), Input::Text('k')],
                "{malformed:?}"
            );
        }
    }

    /// Whatever `bytes` put in the composer.
    fn typed(bytes: &[u8]) -> String {
        decode(bytes)
            .into_iter()
            .filter_map(|event| match event {
                Input::Text(character) => Some(character),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_terminal_talking_about_itself_never_reaches_the_composer() {
        // The three a terminal really volunteers: the foreground and background
        // reports it answers `OSC 10`/`OSC 11` with, and the clipboard an
        // `OSC 52` carries. Every one of them is a *string* -- so a decoder
        // that only knew `CSI` put `11;rgb:0000/0000/0000` into the draft, and
        // the clipboard of whoever pressed the wrong key with it.
        for report in [
            &b"\x1b]10;rgb:c7c7/c7c7/c7c7\x07"[..],
            &b"\x1b]11;rgb:1e1e/1e1e/1e1e\x1b\\"[..],
            &b"\x1b]52;c;aGVsbG8gd29ybGQ=\x07"[..],
        ] {
            assert_eq!(
                typed(report),
                "",
                "the body of {report:?} was typed into the composer"
            );
            assert_eq!(
                decode(report),
                vec![Input::Action(Action::Ignore)],
                "{report:?} was not accounted for as one sequence"
            );
        }
    }

    #[test]
    fn a_report_between_two_keystrokes_costs_neither_of_them() {
        // What a terminal really sends while a person is typing: the answer to
        // a query xfx asked, a cursor report behind it, and the keystroke that
        // was on its way. The first two are the terminal talking about itself
        // and the third is the user.
        assert_eq!(
            decode(b"a\x1b]11;rgb:0000/0000/0000\x1b\\\x1b[12;34Rb"),
            vec![
                Input::Text('a'),
                Input::Action(Action::Ignore),
                Input::Action(Action::Ignore),
                Input::Text('b'),
            ],
            "a report between two keystrokes ate one of them"
        );
    }

    #[test]
    fn an_osc_that_never_terminates_hands_the_next_keystroke_back() {
        // The `ESC` inside a string is either the first half of an `ST` or the
        // end of the conversation. When it is not followed by a backslash the
        // string is abandoned, and what that `ESC` then is depends on the byte
        // behind it: **decoded as an introducer only when that byte is neither
        // a C0 nor `DEL`** -- so the arrow key a terminal put on the wire
        // behind a malformed report is still an arrow key. A C0 or `DEL` is
        // grounded on its own instead, with no `ESC` handed back and therefore
        // no phantom Escape
        // (`a_truncated_report_does_not_invent_an_escape_key_in_front_of_a_control`).
        assert_eq!(
            decode(b"\x1b]11;rgb:0000\x1b[A"),
            vec![Input::Action(Action::Ignore), Input::Action(Action::Up)],
            "a malformed terminator swallowed the keystroke behind it"
        );
        // And a bare `ESC x` behind one is Alt-x: one unknown keystroke, not
        // the letter x in the draft.
        assert_eq!(typed(b"\x1b]52;c;AAAA\x1bx"), "");
    }

    #[test]
    fn a_control_byte_is_a_keystroke_however_long_a_report_has_been_open() {
        // The same rule `CSI` has (`Decoder::intruded`): an `OSC` string is
        // printable text, so a C0 in the middle of one is not part of it -- it
        // is the key the user pressed while the terminal was still talking, and
        // a Ctrl-C that cancelled nothing because a report had not been closed
        // is a turn the user could not stop.
        assert_eq!(
            decode(b"\x1b]52;c;AAAA\x03"),
            vec![Input::Action(Action::Ignore), Input::Action(Action::Cancel)],
            "Ctrl-C was eaten by an unterminated report"
        );
        assert_eq!(
            decode(b"\x1b]0;title\x7f"),
            vec![
                Input::Action(Action::Ignore),
                Input::Action(Action::Backspace)
            ],
            "Backspace was eaten by an unterminated report"
        );
    }

    #[test]
    fn a_report_that_never_ends_stops_eating_keystrokes() {
        // The bound `MOST_BYTES` puts on a `CSI`, sized for a string: a
        // terminal that opens an `OSC` and never closes it must not own the
        // keyboard for the rest of the session. The byte that overruns the
        // bound is the first keystroke again.
        let mut overlong = b"\x1b]".to_vec();
        overlong.extend(std::iter::repeat_n(b'A', MOST_STRING_BYTES));
        overlong.push(b'z');
        let out = decode(&overlong);
        assert_eq!(
            out.last(),
            Some(&Input::Text('z')),
            "the keystroke after an overlong report was eaten: {out:?}"
        );
        assert_eq!(
            typed(&overlong),
            "z",
            "the body of an overlong report reached the composer"
        );
    }

    #[test]
    fn the_bound_is_long_enough_for_the_reports_a_terminal_really_sends() {
        // A bound that cut a real `OSC 52` in half would turn containment into
        // a leak: the tail of the clipboard would be typed. Sized against the
        // longest string a terminal volunteers rather than against a `CSI`'s
        // handful of parameters.
        let mut clipboard = b"\x1b]52;c;".to_vec();
        clipboard.extend(std::iter::repeat_n(b'A', 2048));
        clipboard.push(0x07);
        assert_eq!(
            decode(&clipboard),
            vec![Input::Action(Action::Ignore)],
            "a clipboard report a terminal really sends was cut short"
        );
    }

    #[test]
    fn a_paste_still_owns_every_byte_between_its_markers() {
        // The one place an `OSC` is *not* a terminal talking: between the paste
        // markers every byte is content, so pasted text carrying `ESC ]` is
        // pasted bytes rather than a report that swallows the rest of the
        // paste.
        let out = decode(b"\x1b[200~\x1b]52;c;A\x07\x1b[201~");
        let pasted: Vec<u8> = out
            .iter()
            .filter_map(|event| match event {
                Input::PasteByte(byte) => Some(*byte),
                _ => None,
            })
            .collect();
        assert_eq!(
            pasted,
            b"\x1b]52;c;A\x07".to_vec(),
            "a report inside a paste was decoded instead of pasted"
        );
    }

    #[test]
    fn a_truncated_report_does_not_invent_an_escape_key_in_front_of_a_control() {
        // The `ESC` that ends a malformed string is the **terminal's**, not the
        // user's: it arrived inside a report xfx asked for. Handing the byte
        // after it to the bare-`ESC` decoder makes that decoder answer the
        // question it is built to answer -- "an `ESC` with a C0 behind it is the
        // Escape key and then that key" -- and the Escape it synthesizes is a
        // keystroke nobody made.
        //
        // Ctrl-C is the sharpest of them: the phantom arms the double-Escape
        // gesture and the real byte then cancels the turn, so one truncated
        // report leaves a session one keystroke away from throwing the draft
        // away.
        assert_eq!(
            decode(b"\x1b]52;c;AAAA\x1b\x03"),
            vec![Input::Action(Action::Ignore), Input::Action(Action::Cancel)],
            "a truncated report put an Escape nobody typed in front of Ctrl-C"
        );
        // And `DEL`, which the bare-`ESC` decoder reads as Alt-Backspace: here
        // there was no Alt, so it is the Backspace it is.
        assert_eq!(
            decode(b"\x1b]0;title\x1b\x7f"),
            vec![
                Input::Action(Action::Ignore),
                Input::Action(Action::Backspace)
            ],
            "a truncated report turned a Backspace into Alt-Backspace"
        );
    }

    #[test]
    fn a_truncated_report_ending_in_an_escape_is_one_escape_and_not_two() {
        // `OSC ... ESC ESC`: the first `ESC` ends the string, and the second is
        // whatever it is -- the Escape key, or the head of a sequence still
        // arriving. One of them, once. Two would be the double-Escape gesture
        // (`super::gesture`), and the gesture throws the whole draft away.
        let mut decoder = Decoder::new();
        let start = Instant::now();
        let mut out = Vec::new();
        for byte in b"\x1b]11;rgb:0000\x1b\x1b" {
            decoder.feed(*byte, start, &mut out);
        }
        assert_eq!(
            out,
            vec![Input::Action(Action::Ignore)],
            "the second escape was resolved before the timeout could say what it is"
        );
        decoder.flush(start + Decoder::ESC_TIMEOUT, &mut out);
        assert_eq!(
            out,
            vec![Input::Action(Action::Ignore), Input::Action(Action::Escape)],
            "a truncated report spent the user's double-Escape gesture for them"
        );
    }

    #[test]
    fn the_escape_a_truncated_report_hands_back_still_carries_a_sequence() {
        // The bound on the two cases above: only a **C0 or `DEL`** stops going
        // through the bare-`ESC` decoder. Everything a real escape sequence can
        // begin with still does, which is what keeps the arrow key behind a
        // truncated report an arrow key rather than a bare `[` in the composer.
        assert_eq!(
            decode(b"\x1b]11;rgb:0000\x1b[A"),
            vec![Input::Action(Action::Ignore), Input::Action(Action::Up)]
        );
        assert_eq!(
            decode(b"\x1b]11;rgb:0000\x1bOB"),
            vec![Input::Action(Action::Ignore), Input::Action(Action::Down)]
        );
        // And a printable byte is still Alt-something rather than text: a byte
        // out of a report the terminal truncated is not something the user
        // typed, so it does not reach the draft.
        assert_eq!(typed(b"\x1b]52;c;AAAA\x1bx"), "");
    }
}
