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
//!   content.
//! * **A bare ESC followed by a control byte emits `Escape` *and* replays the
//!   byte** (`terminal_action_decoder.zig:105-114`). No escape sequence carries
//!   a C0 in that position, so the ESC was the user's Escape key and the byte
//!   after it is its own keystroke; swallowing either loses a keystroke the
//!   user really typed.
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
//! below is closed -- a C0 byte is a binding or it is [`Action::Ignore`], and
//! `0x09` is `Ignore` like every other byte the table does not name, rather
//! than the tab character it would otherwise become. The same rule covers the
//! C1 range, which arrives as perfectly valid UTF-8: `U+0085` and `U+009B` are
//! control scalars and are ignored, not typed.
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
//! the same strictness covers every other sequence it reads.
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
//! off, so "in order" is the only thing the caller has to get right.

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
    Home,
    End,
    WordLeft,
    WordRight,
    Backspace,
    Delete,
    DeleteWordLeft,
    KillToEnd,
    KillToStart,
    Submit,
    InsertNewline,
    Escape,
    Cancel,
    Eof,
    Redraw,
    PasteStart,
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
    /// A sequence that outgrew [`MOST_BYTES`]: its bytes go nowhere until a
    /// final byte arrives or this counter reaches the same bound.
    Discard { seen: usize },
    /// A scalar the leading byte of which has arrived.
    Utf8(Partial),
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
        }
    }

    /// Whether the decoder is between paste markers.
    pub(crate) fn pasting(&self) -> bool {
        self.pasting
    }

    /// Feeds one byte, in arrival order, appending whatever it completed.
    pub(crate) fn feed(&mut self, byte: u8, now: Instant, out: &mut Vec<Input>) {
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
                self.matched = 0;
                self.pasting = false;
                out.push(Input::Action(Action::PasteEnd));
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
            // No sequence carries a C0 or a DEL in this position, so the ESC
            // was the Escape key and this byte is its own keystroke. Both of
            // them, in that order.
            0x00..=0x1f | 0x7f => {
                self.stage = Stage::Ground;
                out.push(Input::Action(Action::Escape));
                self.ground(byte, now, out);
            }
            // `ESC` + a printable byte is Alt-something, and this phase binds
            // no Alt key that is not an arrow. Ignored rather than replayed:
            // replaying would put the character in the composer *and* fire
            // whatever `Escape` fires, which is the one thing the user cannot
            // have meant by a single keystroke.
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
                let action = csi(&self.params, byte);
                if action == Action::PasteStart {
                    self.pasting = true;
                    self.matched = 0;
                }
                out.push(Input::Action(action));
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
/// character. `0x09` is the one worth naming -- a tab that became text would be
/// a control in the composer, then in the transcript, and finally on the wire
/// back to the terminal.
fn control(byte: u8) -> Action {
    match byte {
        0x01 => Action::Home,
        0x02 => Action::Left,
        0x03 => Action::Cancel,
        0x04 => Action::Eof,
        0x05 => Action::End,
        0x06 => Action::Right,
        0x08 | 0x7f => Action::Backspace,
        0x0a => Action::InsertNewline,
        0x0b => Action::KillToEnd,
        0x0c => Action::Redraw,
        0x0d => Action::Submit,
        0x15 => Action::KillToStart,
        0x17 => Action::DeleteWordLeft,
        _ => Action::Ignore,
    }
}

/// What a completed `ESC [ <params> <final>` means.
///
/// The parameters are matched **as bytes, in the exact shapes a terminal sends
/// a key in**, and never parsed into numbers that are then compared. A numeric
/// comparison accepts spellings no terminal emits -- `CSI 0200 ~` and
/// `CSI 200 ; 1 ~` both *parse* as 200 -- and the paste markers are where that
/// stops being cosmetic: [`Decoder::pasted`] matches the end marker byte for
/// byte, so a decoder that opened a paste on `CSI 0200 ~` could never be closed
/// by the same spelling. A stream that opens one is latched into paste mode,
/// and every keystroke after it is content. Exact both ways, or the two halves
/// disagree.
///
/// Everything that is not one of these shapes is [`Action::Ignore`], which is
/// the same answer the bounded discard gives and for the same reason: an input
/// this session cannot name is not one it should guess at. `CSI 999 C` is not
/// an arrow key, and a private-parameter sequence (`?`, `>`, `<`, `=`) is a
/// terminal talking about itself rather than a key at all -- both fail the
/// shape check without needing a case of their own.
fn csi(params: &[u8], final_byte: u8) -> Action {
    match final_byte {
        // The tilde keys, each one exactly one spelling.
        b'~' => match params {
            b"1" => Action::Home,
            b"3" => Action::Delete,
            b"4" => Action::End,
            b"200" => Action::PasteStart,
            b"201" => Action::PasteEnd,
            _ => Action::Ignore,
        },
        // The cursor keys, bare or with one modifier.
        b'A' | b'B' | b'C' | b'D' | b'H' | b'F' => {
            let Some(modifier) = modifier(params) else {
                return Action::Ignore;
            };
            // The modifier is one more than a bitmask: 1 shift, 2 alt, 4 ctrl.
            // Shift is dropped along with the selection it would have extended,
            // so what is left is "ctrl or alt means by word".
            let word = (modifier - 1) & 0b110 != 0;
            match final_byte {
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
        _ => Action::Ignore,
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

    fn decode(bytes: &[u8]) -> Vec<Input> {
        let mut decoder = Decoder::new();
        let now = Instant::now();
        let mut out = Vec::new();
        for byte in bytes {
            decoder.feed(*byte, now, &mut out);
        }
        out
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
            &b"\x1b[3;5~"[..],
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
        // dropped down to a plain move.
        assert_eq!(decode(b"\x1b[C"), vec![Input::Action(Action::Right)]);
        assert_eq!(decode(b"\x1b[1;2C"), vec![Input::Action(Action::Right)]);
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
            (0x0b, Action::KillToEnd),
            (0x15, Action::KillToStart),
            (0x17, Action::DeleteWordLeft),
            (0x0c, Action::Redraw),
            (0x0a, Action::InsertNewline),
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
        // an exhaustive table says so.
        let named = [
            (0x01u8, Action::Home),
            (0x02, Action::Left),
            (0x03, Action::Cancel),
            (0x04, Action::Eof),
            (0x05, Action::End),
            (0x06, Action::Right),
            (0x08, Action::Backspace),
            (0x0a, Action::InsertNewline),
            (0x0b, Action::KillToEnd),
            (0x0c, Action::Redraw),
            (0x0d, Action::Submit),
            (0x15, Action::KillToStart),
            (0x17, Action::DeleteWordLeft),
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
                pasted >= 100 && opened >= 5 && closed >= 5 && ignored >= 5 && typed >= 30,
                "seed {seed} exercised too little to mean anything: {pasted} pasted \
                 bytes, {opened} opened, {closed} closed, {ignored} ignored, {typed} typed"
            );
        }
    }

    #[test]
    fn the_paste_flag_is_set_between_the_markers_and_nowhere_else() {
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
        assert!(!decoder.pasting(), "the end marker did not close the paste");
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
}
