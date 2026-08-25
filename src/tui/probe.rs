//! Where the shell left the cursor -- and which way round its colours are --
//! asked of the terminal itself.
//!
//! A band drawn at the bottom of the *normal* buffer shares the screen with
//! whatever the shell printed before xfx ran. Nothing in this process knows
//! what that was or how far down it reached, and there is exactly one authority
//! that does: the terminal. So the session asks it -- `CSI 6n`, the cursor
//! position report -- and pushes the rows above the answer into scrollback, so
//! the band opens on lines that were the terminal's to give.
//!
//! The answer arrives **on standard input**, in the same stream as the user's
//! keystrokes and indistinguishable from them until it is parsed. Two
//! consequences shape everything here:
//!
//! * A terminal that does not implement the query answers nothing at all, so
//!   the read has a [`DEADLINE`] and the session starts on row 1 when it
//!   passes. Row 1 is the answer that assumes the least: it pushes nothing and
//!   paints over nothing.
//! * A keystroke typed while the query is in flight lands in the same read.
//!   It is *deferred*, never dropped -- [`CursorProbe::take_deferred`] hands
//!   back, in arrival order, every byte the probe read and did not use.
//!
//! The state machine keeps the input decoder's discipline: one byte in produces
//! at most one event out, and a candidate reply that turns out to be something
//! else gives back every byte it swallowed rather than eating a prefix
//! (`vercel-labs/fx@ef1d0d0 src/core/terminal/terminal_action_decoder.zig:105-114`).
//!
//! It recognizes a second shape for the same reason it recognizes the first:
//! the launch asks the terminal for its background colour too
//! ([`super::theme::QUERY`]), and that answer arrives on the same stream, in
//! the same read, indistinguishable from a keystroke until it is parsed. It is
//! *not* parsed here -- what an `OSC` reply means belongs to whoever asked the
//! question -- but it is recognized, because a reply the machine did not know
//! about would be deferred into the session's composer as typed text.

use std::collections::VecDeque;
use std::io::{self, Write};
use std::os::fd::{AsFd, AsRawFd, BorrowedFd};
use std::time::{Duration, Instant};

/// The cursor position report request (`CSI 6n`).
pub(crate) const QUERY: &str = "\x1b[6n";

/// How long a terminal has to answer before the session assumes row 1.
///
/// Long enough for a local terminal emulator and a multiplexer in front of it,
/// short enough that a terminal which will never answer costs a tenth of a
/// second once, at launch (`shell_runtime.zig:178-206`).
pub(crate) const DEADLINE: Duration = Duration::from_millis(100);

/// The longest byte string that can still become a reply.
///
/// `ESC [ 65535 ; 65535 R` is fourteen bytes, so sixteen is the shape's own
/// bound with room to spare: a run longer than this is not a truncated answer
/// but something else entirely, and is handed back as such.
const MOST_BYTES: usize = 16;

/// The same bound for the other shape this machine recognizes, an `OSC` reply.
///
/// Upstream's own buffer for a background answer (`theme_detection.zig:43`),
/// which is four times the longest well-formed one -- `ESC ] 1 1 ; r g b :` and
/// three four-digit components with separators and a terminator is 26 bytes.
/// A run longer than this is not a background reply and is handed back as such.
const MOST_OSC_BYTES: usize = 64;

/// What one byte turned out to be.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Probed {
    /// Part of a reply that is not finished yet. The probe is holding it.
    Consumed,
    /// A complete `CSI row;col R`, one-based as the terminal counts.
    Answer(u16, u16),
    /// A complete answer to [`super::theme::QUERY`], from its `ESC ]` through
    /// its terminator.
    ///
    /// Handed over whole rather than parsed here, because what a reply *means*
    /// belongs to whoever asked the question: this machine knows only which
    /// question it is an answer to ([`super::theme::is_background_reply`]).
    ///
    /// **Only that one.** Any other complete `OSC` -- a window title, a
    /// foreground report, a string a program printed before xfx started -- is
    /// handed back byte by byte like any other broken candidate. Consuming it
    /// would lose those bytes *and* fill the slot the real answer is waiting
    /// for, and the invariant this machine exists to keep is that every byte it
    /// reads is either the background answer, the cursor report, or the
    /// session's, in the order it arrived.
    Background(String),
    /// Not part of a reply, and the caller's to keep. When a candidate breaks
    /// this is the **first** byte it had swallowed; the rest go back through
    /// the machine ahead of any newer input, so nothing is lost and nothing is
    /// reordered.
    NotMine(u8),
}

/// Where in `ESC [ <row> ; <col> R` the machine is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    /// Nothing swallowed.
    Idle,
    /// `ESC`, waiting for `[`.
    Esc,
    /// `ESC [`, waiting for the first digit of the row.
    Csi,
    /// Inside the row's digits, waiting for another or for `;`.
    Row,
    /// Inside the column's digits, waiting for another or for `R`.
    Col,
    /// `ESC ]`, inside an `OSC` string's body.
    Osc,
    /// An `ESC` inside that body, waiting for the `\` that ends the string.
    OscEsc,
}

/// Feeds one byte of a possible `CSI row;col R`.
pub(crate) struct CursorProbe {
    state: State,
    /// The bytes of the candidate in hand, so a break can hand them all back.
    held: Vec<u8>,
    /// Bytes waiting to go through the machine: what a break handed back, then
    /// whatever arrived after it. Read from the front, so arrival order is the
    /// order the events come out in.
    queued: VecDeque<u8>,
    row: u16,
    col: u16,
    /// Every byte the read loop took off the terminal and the probe did not
    /// use, in arrival order.
    deferred: Vec<u8>,
    /// The first complete `OSC` string the terminal sent, if it sent one.
    ///
    /// The **first**, because the launch asks exactly one `OSC` question and a
    /// second string arriving unbidden must not be able to overwrite the
    /// answer to it.
    background: Option<String>,
}

impl CursorProbe {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Idle,
            held: Vec::new(),
            queued: VecDeque::new(),
            row: 0,
            col: 0,
            deferred: Vec::new(),
            background: None,
        }
    }

    /// The `OSC` reply the terminal sent during the launch read, if it sent
    /// one.
    ///
    /// Borrowed rather than taken: the launch reads it once, and a probe that
    /// gave it up would make "did the terminal answer" unaskable a second time
    /// -- which is exactly the question a re-measured launch
    /// (`super::MEASUREMENTS`) asks.
    pub(crate) fn background(&self) -> Option<&str> {
        self.background.as_deref()
    }

    /// Feeds one byte; `Probed::Answer` when a reply completes.
    ///
    /// One byte in, at most one event out. A byte handed in while the machine
    /// still has handed-back bytes queued waits its turn behind them, which is
    /// what keeps the events in arrival order; whatever is still queued when
    /// the input ends comes out of
    /// [`take_deferred`](Self::take_deferred) rather than being dropped.
    pub(crate) fn feed(&mut self, byte: u8) -> Probed {
        self.queued.push_back(byte);
        self.step()
    }

    /// Runs the machine over one queued byte.
    fn step(&mut self) -> Probed {
        let Some(byte) = self.queued.pop_front() else {
            return Probed::Consumed;
        };
        // The two shapes have different lengths, so they have different
        // bounds; a cursor report's sixteen would break every well-formed
        // background reply.
        let most = match self.state {
            State::Osc | State::OscEsc => MOST_OSC_BYTES,
            _ => MOST_BYTES,
        };
        if self.held.len() >= most {
            return self.hand_back(byte);
        }
        match (self.state, byte) {
            (State::Idle, 0x1b) => self.swallow(byte, State::Esc),
            (State::Esc, b'[') => self.swallow(byte, State::Csi),
            (State::Esc, b']') => self.swallow(byte, State::Osc),
            // `BEL` and `ESC \` both end an `OSC`, and terminals disagree
            // about which they send -- upstream stops its read on either
            // (`theme_detection.zig:58`).
            (State::Osc, 0x07) => self.finish_string(byte),
            (State::Osc, 0x1b) => self.swallow(byte, State::OscEsc),
            (State::OscEsc, b'\\') => self.finish_string(byte),
            // Printable text only. A control byte inside the body is not
            // something a terminal puts in a reply, and treating it as one
            // would let a burst of keystrokes that happened to begin `ESC ]`
            // be swallowed up to the bound instead of handed back.
            (State::Osc, 0x20..=0x7e) => self.swallow(byte, State::Osc),
            (State::Csi | State::Row, b'0'..=b'9') => match extend(self.row, byte) {
                Some(row) => {
                    self.row = row;
                    self.swallow(byte, State::Row)
                }
                // A row that cannot be a `u16` is not a cursor position.
                None => self.hand_back(byte),
            },
            (State::Row, b';') => self.swallow(byte, State::Col),
            (State::Col, b'0'..=b'9') => match extend(self.col, byte) {
                Some(col) => {
                    self.col = col;
                    self.swallow(byte, State::Col)
                }
                None => self.hand_back(byte),
            },
            (State::Col, b'R') => {
                let answer = Probed::Answer(self.row, self.col);
                self.reset();
                answer
            }
            _ => self.hand_back(byte),
        }
    }

    /// Completes an `OSC` string with the byte that terminated it, and keeps it
    /// only if it is the one the launch asked for.
    ///
    /// A string that is **not** an answer to
    /// [`super::theme::QUERY`] goes back through [`hand_back`](Self::hand_back)
    /// exactly as a broken candidate does -- first byte first, the rest to the
    /// front of the queue -- so it reaches the session's decoder unchanged and
    /// in order. It is not this machine's to eat: it did not ask for it.
    ///
    /// The `String` is lossy rather than fallible because it cannot lose
    /// anything: every byte in hand was admitted by the arms above, which take
    /// only `ESC`, `BEL` and printable ASCII, so it is already valid UTF-8 and
    /// the replacement character is unreachable.
    fn finish_string(&mut self, byte: u8) -> Probed {
        // Read before the byte is committed, so a string that is not ours is
        // handed back with its terminator still the byte that broke it.
        let ours = {
            let mut text = String::from_utf8_lossy(&self.held).into_owned();
            text.push(byte as char);
            super::theme::is_background_reply(&text).then_some(text)
        };
        match ours {
            Some(text) => {
                self.reset();
                Probed::Background(text)
            }
            None => self.hand_back(byte),
        }
    }

    /// Keeps `byte` as part of the candidate.
    fn swallow(&mut self, byte: u8, next: State) -> Probed {
        self.held.push(byte);
        self.state = next;
        Probed::Consumed
    }

    /// Gives up on the candidate, first byte first.
    ///
    /// The rest of what was swallowed goes back to the **front** of the queue,
    /// ahead of `byte` itself and of anything newer, and runs through the
    /// machine again: a second `ESC` that broke the first candidate is the
    /// start of the next one, and a machine that dropped it would lose a real
    /// escape sequence.
    fn hand_back(&mut self, byte: u8) -> Probed {
        let mut swallowed = std::mem::take(&mut self.held);
        self.reset();
        self.queued.push_front(byte);
        if swallowed.is_empty() {
            // Nothing was in hand, so `byte` itself is the event and is not
            // waiting for a second pass.
            self.queued.pop_front();
            return Probed::NotMine(byte);
        }
        for held in swallowed.drain(1..).rev() {
            self.queued.push_front(held);
        }
        Probed::NotMine(swallowed[0])
    }

    fn reset(&mut self) {
        self.state = State::Idle;
        self.held.clear();
        self.row = 0;
        self.col = 0;
    }

    /// Writes the launch's queries. Flushed, because nothing answers a
    /// question that is still in a buffer.
    ///
    /// **The background query goes first and the cursor report second**, in one
    /// write, and that order is load-bearing rather than tidy: a terminal
    /// answers the queries in its input stream in the order it parsed them, so
    /// a cursor report arriving with no background reply in front of it proves
    /// the terminal has no answer to give rather than merely not having given
    /// one yet. See [`super::theme::QUERY`].
    fn ask(out: &mut impl Write, background: bool) -> io::Result<()> {
        // Built whole and issued as **one** `write_all`, which is the ordering
        // argument made good rather than merely stated. The fence works because
        // the terminal parses the two queries in the order they reach it; two
        // writes put that order at the mercy of whatever is between this and
        // the descriptor -- a buffer that flushes between them, a short write,
        // a signal -- and a terminal that saw the cursor report first would
        // answer it first, at which point the reply that arrives with nothing
        // in front of it is the *cursor's* and the fence proves nothing.
        let mut queries = String::with_capacity(super::theme::QUERY.len() + QUERY.len());
        if background {
            queries.push_str(super::theme::QUERY);
        }
        queries.push_str(QUERY);
        out.write_all(queries.as_bytes())?;
        out.flush()
    }

    /// Asks the terminal where the cursor is and reads the answer.
    ///
    /// `None` means the terminal did not answer before `deadline` -- the
    /// ordinary case for a terminal without the query, and one the caller
    /// treats as row 1 rather than as a failure. An `Err` is a descriptor that
    /// broke, which is a different thing and is reported.
    pub(crate) fn read_reply(
        &mut self,
        background: bool,
        deadline: Instant,
    ) -> io::Result<Option<(u16, u16)>> {
        Self::ask(&mut io::stdout().lock(), background)?;
        let stdin = io::stdin();
        self.read_answer(stdin.as_fd(), deadline)
    }

    /// The read above against an explicit descriptor, so the deadline and the
    /// deferral are tests rather than claims.
    fn read_answer(
        &mut self,
        fd: BorrowedFd<'_>,
        deadline: Instant,
    ) -> io::Result<Option<(u16, u16)>> {
        let mut chunk = [0u8; 64];
        loop {
            if !readable_before(fd, deadline)? {
                // The deadline. What was read stays the caller's.
                self.defer_the_rest();
                return Ok(None);
            }
            let read = match rustix::io::read(fd, &mut chunk) {
                // A terminal with no writer left has nothing more to say, and
                // the session is about to end anyway.
                Ok(0) => {
                    self.defer_the_rest();
                    return Ok(None);
                }
                Ok(read) => read,
                Err(rustix::io::Errno::INTR) => continue,
                Err(err) => return Err(err.into()),
            };
            for (index, byte) in chunk[..read].iter().enumerate() {
                let event = self.feed(*byte);
                if let Some(answer) = self.record(event) {
                    // Whatever shared the read with the answer was typed, not
                    // answered: it belongs to the session, in the order it
                    // arrived and behind anything already queued.
                    self.queued.extend(&chunk[index + 1..read]);
                    self.defer_the_rest();
                    return Ok(Some(answer));
                }
            }
            // A break hands bytes back for a second pass, so the machine can be
            // one or more bytes behind the read that fed it. The answer may be
            // in that backlog, and waiting on the terminal for a byte already
            // in hand would spend the whole deadline.
            while !self.queued.is_empty() {
                let event = self.step();
                if let Some(answer) = self.record(event) {
                    self.defer_the_rest();
                    return Ok(Some(answer));
                }
            }
        }
    }

    /// Keeps what the event says to keep: a byte that is not the terminal's
    /// answer is the session's, and is deferred rather than dropped.
    fn record(&mut self, event: Probed) -> Option<(u16, u16)> {
        match event {
            Probed::Consumed => None,
            Probed::NotMine(byte) => {
                self.deferred.push(byte);
                None
            }
            Probed::Background(text) => {
                self.background.get_or_insert(text);
                None
            }
            Probed::Answer(row, column) => Some((row, column)),
        }
    }

    /// Moves everything still in hand into the deferred bytes, in order: the
    /// truncated candidate the machine was holding, then whatever was queued
    /// behind it and never stepped.
    fn defer_the_rest(&mut self) {
        let held = std::mem::take(&mut self.held);
        self.deferred.extend(held);
        self.deferred.extend(self.queued.drain(..));
        self.reset();
    }

    /// Every byte the probe read off the terminal and did not use, in arrival
    /// order, leaving the probe with none.
    ///
    /// The caller must answer these before its first wait: they were typed on a
    /// terminal that is already raw, and no second read will produce them
    /// again.
    #[must_use]
    pub(crate) fn take_deferred(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.deferred)
    }
}

/// One more digit of a decimal parameter, or `None` when it would not fit.
fn extend(value: u16, digit: u8) -> Option<u16> {
    value.checked_mul(10)?.checked_add(u16::from(digit - b'0'))
}

/// Waits until `fd` has something to read or `deadline` passes.
///
/// `pselect(2)` with a null mask, which is `select` with the thread's mask left
/// exactly as it is -- and that is the point rather than an omission. The
/// session's invariant is that while the terminal is raw, `SIGTSTP` is either
/// blocked or the process is inside [`wait_for_input`](super::signals::wait_for_input);
/// this wait keeps the first clause, because the stop is still blocked from
/// `block_owned` through `lift_deaths_keeping_the_stop`. Letting it in here
/// instead would park a stopped session in a call with no resume behind it: the
/// re-raw on `SIGCONT` lives in the event loop, not in a launch probe. The
/// deadline is what makes that safe to say -- this wait cannot outlast a tenth
/// of a second, so nothing waits on a held stop for long.
fn readable_before(fd: BorrowedFd<'_>, deadline: Instant) -> io::Result<bool> {
    let raw = fd.as_raw_fd();
    // `FD_SET` on a descriptor at or past `FD_SETSIZE` writes outside the set.
    if raw < 0 || raw as usize >= libc::FD_SETSIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the descriptor is out of range for pselect",
        ));
    }
    loop {
        let Some(left) = deadline.checked_duration_since(Instant::now()) else {
            return Ok(false);
        };
        let timeout = libc::timespec {
            tv_sec: libc::time_t::try_from(left.as_secs()).unwrap_or(libc::time_t::MAX),
            // `subsec_nanos` is under a billion by construction, so this is the
            // whole remainder rather than a clamp of it.
            tv_nsec: libc::c_long::from(left.subsec_nanos()),
        };
        // SAFETY: `readable` is zeroed before use and `raw` was just
        // range-checked; `timeout` is owned here and lives across the call; the
        // null write and error sets mean "not interested", and the null mask
        // means "leave this thread's mask exactly as it is".
        let waited = unsafe {
            let mut readable: libc::fd_set = std::mem::zeroed();
            libc::FD_ZERO(&mut readable);
            libc::FD_SET(raw, &mut readable);
            libc::pselect(
                raw + 1,
                &mut readable,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &timeout,
                std::ptr::null(),
            )
        };
        if waited < 0 {
            let err = io::Error::last_os_error();
            // A `SIGCONT` or a `SIGWINCH` is not the end of the wait: the
            // handlers carry no `SA_RESTART`, so an interrupted wait resumes
            // against the same deadline rather than reporting a failure the
            // caller would read as a broken terminal.
            if err.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(err);
        }
        return Ok(waited > 0);
    }
}

/// How many rows the launch has to scroll away.
///
/// Everything above the cursor is the shell's, and all of it goes into
/// scrollback: the session then owns a screen that begins at row 1, which is
/// what the transcript viewport is anchored to (`.prd/03-tui-port.md:43-44`,
/// `.prd/research/tui-core.md:30,114`). So the count is the number of rows
/// above the cursor -- capped at the screen, because a cursor row reported
/// larger than the window cannot mean more scrolling than the screen holds.
///
/// This is the **amount**. [`push`] is the mechanics, and the two are separate
/// because only the second one is about where the cursor happens to be.
pub(crate) fn scrollback_push(cursor_row: u16, rows: u16) -> u16 {
    cursor_row.min(rows).saturating_sub(1)
}

/// Scrolls the shell's output above the band into the terminal's scrollback.
///
/// **The cursor is moved to the bottom row first, and that is the whole of the
/// mechanics.** A linefeed scrolls a terminal only when the cursor is already
/// on the bottom margin; anywhere else it just moves the cursor down a row and
/// the screen does not move at all. Emitting the count from wherever the shell
/// left the cursor would therefore displace `max(0, 2r - R - 1)` rows instead
/// of `r - 1` -- nothing whatsoever from row 2 of a 24-row screen -- and the
/// band would open on top of output that is still there.
///
/// `CUP` to the bottom row, then one literal newline per row, is also exactly
/// how upstream scrolls during a session (`writeTerminalScroll`,
/// `terminal_diff.zig`: `\x1b[{bottom};1H` then `'\n'` x rows), and what its
/// launch does (`pushLaunchRowsIntoScrollback`, `app_lifecycle.zig:556-583`).
///
/// A push of nothing writes nothing: there is no reason to move the shell's
/// cursor when there is nothing above it to save.
pub(crate) fn push(out: &mut impl Write, cursor_row: u16, rows: u16) -> io::Result<()> {
    let lines = scrollback_push(cursor_row, rows);
    if lines == 0 {
        return Ok(());
    }
    write!(out, "\x1b[{rows};1H")?;
    for _ in 0..lines {
        writeln!(out)?;
    }
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::os::fd::OwnedFd;

    #[test]
    fn a_complete_reply_is_read_a_byte_at_a_time() {
        let mut probe = CursorProbe::new();
        let mut answer = None;
        for byte in b"\x1b[12;34R" {
            if let Probed::Answer(row, col) = probe.feed(*byte) {
                answer = Some((row, col));
            }
        }
        assert_eq!(answer, Some((12, 34)));
    }

    #[test]
    fn a_byte_that_is_not_part_of_a_reply_is_handed_back_for_the_decoder() {
        let mut probe = CursorProbe::new();
        assert!(matches!(probe.feed(b'a'), Probed::NotMine(b'a')));
        // A partial reply that turns out to be something else hands back every
        // byte it swallowed, in order, so a keystroke typed during the probe is
        // not eaten.
        assert!(matches!(probe.feed(0x1b), Probed::Consumed));
        assert!(matches!(probe.feed(b'['), Probed::Consumed));
        assert!(matches!(probe.feed(b'A'), Probed::NotMine(0x1b)));
    }

    /// A terminal, to the extent the launch push can move one.
    ///
    /// The byte-level test below says which bytes go on the wire; this says
    /// what they *do* to a screen, which is the claim that actually matters and
    /// the one a linefeed count cannot make on its own. It models the three
    /// things the push uses and refuses everything else loudly, so a future
    /// edit that emits a fourth thing cannot be silently unmodelled.
    ///
    /// Only the rules the push depends on: a linefeed on the bottom margin
    /// scrolls the screen by one and the top row leaves for scrollback; a
    /// linefeed anywhere else only moves the cursor down; `CUP` places it.
    struct Screen {
        rows: u16,
        /// One mark per row, top first: the shell's line that was printed
        /// there, or nothing.
        lines: Vec<Option<u16>>,
        cursor_row: u16,
        /// The rows that have left the top of the screen -- native scrollback.
        scrolled_off: Vec<Option<u16>>,
    }

    impl Screen {
        /// A screen a shell has printed `through - 1` lines on, leaving the
        /// cursor on row `through` -- which is the state a `CSI 6n` at launch
        /// reports and the only state the push is ever handed.
        fn shell_printed(rows: u16, through: u16) -> Self {
            let mut lines = vec![None; usize::from(rows)];
            for row in 1..through.min(rows) {
                lines[usize::from(row) - 1] = Some(row);
            }
            Self {
                rows,
                lines,
                cursor_row: through.min(rows),
                scrolled_off: Vec::new(),
            }
        }

        fn feed(&mut self, bytes: &[u8]) {
            let mut rest = bytes;
            while let Some((byte, tail)) = rest.split_first() {
                match byte {
                    b'\n' => {
                        self.linefeed();
                        rest = tail;
                    }
                    b'\r' => rest = tail,
                    0x1b => {
                        let (row, after) = parse_cup(rest).unwrap_or_else(|| {
                            panic!("the push wrote {rest:?}, which this screen does not model")
                        });
                        self.cursor_row = row.clamp(1, self.rows);
                        rest = after;
                    }
                    other => {
                        panic!("the push wrote byte {other:#04x}, which this screen does not model")
                    }
                }
            }
        }

        /// The one rule the whole correction turns on.
        fn linefeed(&mut self) {
            if self.cursor_row < self.rows {
                self.cursor_row += 1;
                return;
            }
            self.scrolled_off.push(self.lines.remove(0));
            self.lines.push(None);
        }

        /// How many rows really left the screen.
        fn displaced(&self) -> usize {
            self.scrolled_off.len()
        }

        /// The shell's lines that are still on the screen, top first. The band
        /// may not open above any of them.
        fn preserved_on_screen(&self) -> Vec<u16> {
            self.lines.iter().flatten().copied().collect()
        }

        /// The topmost row a session may draw on without covering the shell:
        /// row 1 when nothing of the shell's is left, and the row below the
        /// last surviving line otherwise.
        fn first_row_free_for_the_band(&self) -> u16 {
            self.lines
                .iter()
                .rposition(Option::is_some)
                .map_or(1, |index| u16::try_from(index).expect("a row index") + 2)
        }
    }

    /// `CSI <row> ; <col> H`, and nothing else with an escape in it.
    fn parse_cup(bytes: &[u8]) -> Option<(u16, &[u8])> {
        let rest = bytes.strip_prefix(b"\x1b[")?;
        let end = rest.iter().position(|byte| *byte == b'H')?;
        let (parameters, tail) = rest.split_at(end);
        let text = std::str::from_utf8(parameters).ok()?;
        let (row, column) = text.split_once(';')?;
        assert_eq!(column, "1", "the push left the cursor off the first column");
        Some((row.parse().ok()?, &tail[1..]))
    }

    /// What the push does to a screen the shell printed `through - 1` lines on.
    fn pushed(rows: u16, through: u16) -> (Screen, Vec<u8>) {
        let mut wire = Vec::new();
        push(&mut wire, through, rows).expect("write the push");
        let mut screen = Screen::shell_printed(rows, through);
        screen.feed(&wire);
        (screen, wire)
    }

    #[test]
    fn the_push_really_scrolls_the_screen_it_is_given() {
        // Row 2 of 24: one line of shell output above the cursor, and the whole
        // point of the correction. A linefeed from row 2 scrolls nothing; the
        // push has to put the cursor on the bottom margin first, and this
        // asserts the screen moved rather than the byte count.
        let (screen, _wire) = pushed(24, 2);
        assert_eq!(screen.displaced(), 1);
        assert_eq!(screen.preserved_on_screen(), Vec::<u16>::new());
        assert_eq!(screen.first_row_free_for_the_band(), 1);
        assert_eq!(screen.scrolled_off, vec![Some(1)]);

        // The bottom row: everything the shell printed goes to scrollback.
        let (screen, _wire) = pushed(24, 24);
        assert_eq!(screen.displaced(), 23);
        assert_eq!(screen.preserved_on_screen(), Vec::<u16>::new());
        assert_eq!(screen.first_row_free_for_the_band(), 1);

        // The boundary a four-row band cares about: the last row from which the
        // band's top (row 21) would not yet be covering anything.
        let (screen, _wire) = pushed(24, 20);
        assert_eq!(screen.displaced(), 19);
        assert_eq!(screen.preserved_on_screen(), Vec::<u16>::new());
        assert_eq!(screen.first_row_free_for_the_band(), 1);

        // And a cursor row past the window: never more than the screen holds.
        let (screen, _wire) = pushed(24, 30);
        assert_eq!(screen.displaced(), 23);
        assert_eq!(screen.preserved_on_screen(), Vec::<u16>::new());
    }

    #[test]
    fn a_launch_on_row_one_writes_nothing_and_moves_nothing() {
        // Nothing was printed, so there is nothing to save and no reason to
        // move the shell's cursor.
        let (screen, wire) = pushed(24, 1);
        assert!(
            wire.is_empty(),
            "the push wrote {wire:?} for an empty screen"
        );
        assert_eq!(screen.displaced(), 0);
        assert_eq!(screen.cursor_row, 1);
    }

    #[test]
    fn the_push_is_a_move_to_the_bottom_row_and_then_one_newline_per_row() {
        let (_screen, wire) = pushed(24, 4);
        assert_eq!(wire, b"\x1b[24;1H\n\n\n");
    }

    /// The model is only worth its assertions if it can tell the two mechanics
    /// apart, so here it is told the wrong one on purpose.
    #[test]
    fn the_screen_model_says_a_linefeed_off_the_bottom_margin_scrolls_nothing() {
        let mut screen = Screen::shell_printed(24, 2);
        screen.feed(b"\n");
        assert_eq!(screen.displaced(), 0, "a linefeed from row 2 scrolled");
        assert_eq!(screen.preserved_on_screen(), vec![1]);
        assert_eq!(screen.first_row_free_for_the_band(), 2);
    }

    #[test]
    fn the_push_moves_the_shell_output_above_the_cursor_and_no_further() {
        // On row 1 there is nothing above to push.
        assert_eq!(scrollback_push(1, 24), 0);
        // Nine lines of shell output above the cursor: nine newlines put them
        // above the band, and never more than the screen holds.
        assert_eq!(scrollback_push(10, 24), 9);
        assert_eq!(scrollback_push(24, 24), 23);
        assert_eq!(scrollback_push(30, 24), 23);
    }

    /// Every event a run of bytes produces, and every byte it gives back.
    ///
    /// The second half is the property the whole machine turns on: each byte
    /// fed in is either part of an answer or handed back, in the order it
    /// arrived, and nothing is both or neither.
    fn run(bytes: &[u8]) -> (Vec<Probed>, Vec<u8>) {
        let mut probe = CursorProbe::new();
        let mut seen = Vec::new();
        for byte in bytes {
            seen.push(probe.feed(*byte));
        }
        // A break hands bytes back for a second pass, so the events lag the
        // input by however many are still queued. A run ends the way
        // `read_answer` ends one: by stepping until nothing is queued.
        while !probe.queued.is_empty() {
            seen.push(probe.step());
        }
        probe.defer_the_rest();
        let mut given_back: Vec<u8> = seen
            .iter()
            .filter_map(|event| match event {
                Probed::NotMine(byte) => Some(*byte),
                _ => None,
            })
            .collect();
        given_back.extend(probe.take_deferred());
        (seen, given_back)
    }

    #[test]
    fn the_bytes_a_broken_candidate_gives_back_come_out_in_arrival_order() {
        // `ESC [ A` is three bytes the machine swallowed two of. The first
        // comes back as the event; the other two follow it, ahead of the `b`
        // that arrived afterwards -- so a caller that keeps every handed-back
        // byte reconstructs the stream exactly, `ESC [ A b`.
        let (events, given_back) = run(b"\x1b[Ab");
        assert_eq!(
            &events[..4],
            &[
                Probed::Consumed,
                Probed::Consumed,
                Probed::NotMine(0x1b),
                Probed::NotMine(b'['),
            ]
        );
        assert_eq!(given_back, b"\x1b[Ab");
    }

    #[test]
    fn a_second_escape_starts_the_next_candidate_rather_than_being_dropped() {
        // The handed-back bytes go through the machine again, which is the only
        // reason the reply here is ever seen: it begins inside the run the
        // first `ESC` gave up on.
        let (events, given_back) = run(b"\x1b\x1b[5;9R");
        assert!(
            events.contains(&Probed::Answer(5, 9)),
            "the reply after a broken candidate was lost: {events:?}"
        );
        // Only the escape that broke: the six bytes of the reply were the
        // probe's, and it kept exactly those.
        assert_eq!(given_back, b"\x1b");
    }

    #[test]
    fn a_parameter_too_large_for_the_shape_is_not_a_cursor_position() {
        let (events, given_back) = run(b"\x1b[123456;1R");
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, Probed::Answer(..))),
            "a row that cannot be a u16 was reported as one: {events:?}"
        );
        assert_eq!(given_back, b"\x1b[123456;1R");
    }

    #[test]
    fn a_run_longer_than_the_shape_allows_is_given_back_whole() {
        // Seventeen bytes of digits: a reply cannot be this long, so the
        // candidate is abandoned at the bound rather than buffered forever.
        let (events, given_back) = run(b"\x1b[1111111111111111;1R");
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, Probed::Answer(..))),
            "an over-long run was reported as a cursor position: {events:?}"
        );
        assert_eq!(given_back, b"\x1b[1111111111111111;1R");
    }

    /// A pipe standing in for the terminal: the write end is the terminal
    /// answering, the read end is what the probe reads.
    fn pipe() -> (OwnedFd, OwnedFd) {
        rustix::pipe::pipe().expect("a pipe")
    }

    /// A terminal that remembers how it was written to, not only what.
    ///
    /// The `ask` contract has two halves and a `Vec<u8>` can only check one of
    /// them: the bytes, and the number of calls it took to put them there.
    #[derive(Default)]
    struct Counting {
        bytes: Vec<u8>,
        writes: usize,
        flushes: usize,
    }

    impl Write for Counting {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            self.writes += 1;
            self.bytes.extend_from_slice(buffer);
            Ok(buffer.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            self.flushes += 1;
            Ok(())
        }
    }

    #[test]
    fn asking_writes_the_queries_and_nothing_else() {
        let mut wire = Counting::default();
        CursorProbe::ask(&mut wire, false).expect("write the query");
        assert_eq!(wire.bytes, QUERY.as_bytes());
        assert_eq!(wire.writes, 1);
        assert_eq!(wire.flushes, 1);
    }

    #[test]
    fn the_background_query_goes_first_so_the_cursor_report_is_its_fence() {
        // The order is the whole reason one read answers both questions: a
        // terminal answers in the order it parsed, so a cursor report with no
        // background reply in front of it is proof there will not be one.
        // Written the other way round it would prove nothing, and a terminal
        // without `OSC 11` would cost the deadline on every launch.
        let mut wire = Counting::default();
        CursorProbe::ask(&mut wire, true).expect("write the queries");
        let bytes = String::from_utf8(wire.bytes).expect("the queries are text");
        assert_eq!(bytes, format!("{}{QUERY}", super::super::theme::QUERY));
        // **In one write**, so the order the terminal parses them in is this
        // function's to decide and not a buffer's: the fence is an ordering
        // argument, and an argument about order that is issued in two pieces is
        // not made.
        assert_eq!(
            wire.writes, 1,
            "the two queries were written separately, so nothing here decides \
             which one the terminal parses first"
        );
        assert_eq!(wire.flushes, 1);
    }

    #[test]
    fn both_answers_and_a_keystroke_can_share_one_read() {
        // What a terminal that implements both queries really sends, in the
        // order it was asked, with a key pressed while they were in flight.
        // All three have to come out right: the background is kept, the cursor
        // report ends the read, and the keystroke is the session's.
        let (read, write) = pipe();
        rustix::io::write(&write, b"\x1b]11;rgb:ffff/ffff/ffff\x1b\\\x1b[7;1Rx")
            .expect("the terminal answers");
        let mut probe = CursorProbe::new();
        let answer = probe
            .read_answer(read.as_fd(), Instant::now() + DEADLINE)
            .expect("read the answers");
        assert_eq!(answer, Some((7, 1)));
        assert_eq!(
            probe.background(),
            Some("\u{1b}]11;rgb:ffff/ffff/ffff\u{1b}\\")
        );
        assert_eq!(
            probe.take_deferred(),
            b"x".to_vec(),
            "the key typed during the queries was swallowed"
        );
    }

    #[test]
    fn a_string_the_launch_did_not_ask_for_is_the_sessions_and_is_kept_in_order() {
        // The conservation law, on the one stream that can carry all four
        // things at once: a window title the shell was still writing when xfx
        // started, the answer to the question this module *did* ask, the cursor
        // report that fences it, and a keystroke behind all of them.
        //
        // A machine that consumed every terminated `OSC` would fail this twice
        // over and in the two worst directions: the title's bytes would vanish
        // from the session's input, and the title would take the background
        // slot -- so the real reply, arriving second, would be dropped by the
        // first-wins rule and the palette would be decided by a window title.
        let (read, write) = pipe();
        rustix::io::write(
            &write,
            b"\x1b]0;title\x07\x1b]11;rgb:ffff/ffff/ffff\x1b\\\x1b[7;1Rxy",
        )
        .expect("the terminal answers");
        let mut probe = CursorProbe::new();
        let answer = probe
            .read_answer(read.as_fd(), Instant::now() + DEADLINE)
            .expect("read the answers");

        assert_eq!(answer, Some((7, 1)));
        assert_eq!(
            probe.background(),
            Some("\u{1b}]11;rgb:ffff/ffff/ffff\u{1b}\\"),
            "a string the launch never asked for took the background's place"
        );
        assert_eq!(
            probe.take_deferred(),
            b"\x1b]0;title\x07xy".to_vec(),
            "the bytes that were the session's were eaten or reordered"
        );
    }

    #[test]
    fn a_bel_ends_a_background_reply_as_well_as_a_string_terminator() {
        // Terminals disagree about which one they send, so both are read
        // (`theme_detection.zig:58`).
        let mut probe = CursorProbe::new();
        let mut seen = None;
        for byte in b"\x1b]11;rgb:1c1c/1c1c/1c1c\x07" {
            if let Probed::Background(text) = probe.feed(*byte) {
                seen = Some(text);
            }
        }
        assert_eq!(seen.as_deref(), Some("\u{1b}]11;rgb:1c1c/1c1c/1c1c\u{07}"));
    }

    #[test]
    fn a_terminal_that_never_terminates_its_string_loses_no_byte_of_it() {
        // The ordinary shape of a terminal that does not implement the query
        // and a user typing over the top of it: nothing completes, the deadline
        // passes, and every byte read is still the session's -- in the order it
        // arrived, which is what the decoder is owed.
        let (read, write) = pipe();
        rustix::io::write(&write, b"\x1b]11;rgb:ffff").expect("a truncated reply");
        let mut probe = CursorProbe::new();
        let answer = probe
            .read_answer(read.as_fd(), Instant::now() + Duration::from_millis(20))
            .expect("wait out the deadline");
        assert_eq!(answer, None);
        assert_eq!(probe.background(), None, "half a reply was read as one");
        assert_eq!(probe.take_deferred(), b"\x1b]11;rgb:ffff".to_vec());
    }

    #[test]
    fn a_control_byte_inside_a_string_gives_the_whole_candidate_back() {
        // `ESC ]` is two bytes a user can type, and a machine that swallowed
        // everything after them until a terminator would eat the rest of the
        // session's input on a terminal that never sends one.
        let mut probe = CursorProbe::new();
        assert!(matches!(probe.feed(0x1b), Probed::Consumed));
        assert!(matches!(probe.feed(b']'), Probed::Consumed));
        assert!(matches!(probe.feed(b'1'), Probed::Consumed));
        assert!(matches!(probe.feed(0x04), Probed::NotMine(0x1b)));
        // and the rest of what it had swallowed comes back behind it, in order
        let mut given_back = vec![];
        for _ in 0..3 {
            if let Probed::NotMine(byte) = probe.step() {
                given_back.push(byte);
            }
        }
        assert_eq!(given_back, b"]1\x04");
    }

    #[test]
    fn a_string_longer_than_a_background_reply_is_not_one() {
        // The bound is the shape's, so a run past it is handed back rather than
        // buffered without end.
        let mut probe = CursorProbe::new();
        let long: Vec<u8> = b"\x1b]"
            .iter()
            .copied()
            .chain(std::iter::repeat_n(b'a', MOST_OSC_BYTES))
            .collect();
        let mut given_back = None;
        for byte in &long {
            if let Probed::NotMine(byte) = probe.feed(*byte) {
                // The **first** one: a break hands the candidate back first
                // byte first, and everything after it comes out of the backlog
                // as later bytes are fed.
                given_back.get_or_insert(byte);
            }
        }
        assert_eq!(given_back, Some(0x1b), "the run was swallowed whole");
    }

    #[test]
    fn a_keystroke_that_shares_the_read_with_the_answer_is_deferred_not_eaten() {
        let (read, write) = pipe();
        rustix::io::write(&write, b"\x1b[7;1R\x04").expect("the terminal answers");
        let mut probe = CursorProbe::new();
        let answer = probe
            .read_answer(read.as_fd(), Instant::now() + DEADLINE)
            .expect("read the answer");
        assert_eq!(answer, Some((7, 1)));
        assert_eq!(
            probe.take_deferred(),
            vec![0x04],
            "the Ctrl-D typed during the query was swallowed"
        );
    }

    #[test]
    fn a_reply_that_follows_a_broken_candidate_in_the_same_read_is_still_found() {
        // The machine ends the read three bytes behind the terminal, because a
        // broken candidate handed those three back for a second pass. The
        // answer is inside that backlog: a loop that only ever stepped once per
        // byte read would still be waiting for it when the deadline passed.
        let (read, write) = pipe();
        rustix::io::write(&write, b"\x1b[?\x1b[7;1R").expect("noise, then the answer");
        let mut probe = CursorProbe::new();
        let answer = probe
            .read_answer(read.as_fd(), Instant::now() + Duration::from_millis(40))
            .expect("read the answer");
        assert_eq!(answer, Some((7, 1)));
        assert_eq!(probe.take_deferred(), b"\x1b[?".to_vec());
    }

    #[test]
    fn a_terminal_that_never_answers_gives_up_at_the_deadline() {
        // The write end stays open for the whole call, so the wait ends on the
        // deadline rather than on an end of file -- which is the case a
        // terminal without the query really presents.
        let (read, _write) = pipe();
        let deadline = Instant::now() + Duration::from_millis(40);
        let mut probe = CursorProbe::new();
        let answer = probe
            .read_answer(read.as_fd(), deadline)
            .expect("give up cleanly");
        assert_eq!(answer, None);
        assert!(
            Instant::now() >= deadline,
            "the probe gave up before the deadline it was given"
        );
    }

    #[test]
    fn what_was_typed_before_a_reply_that_never_came_is_still_the_callers() {
        let (read, write) = pipe();
        rustix::io::write(&write, b"hi\x1b[").expect("a keystroke and half a reply");
        let mut probe = CursorProbe::new();
        let answer = probe
            .read_answer(read.as_fd(), Instant::now() + Duration::from_millis(40))
            .expect("give up cleanly");
        assert_eq!(answer, None);
        // The truncated candidate included: it was typed, not answered.
        assert_eq!(probe.take_deferred(), b"hi\x1b[".to_vec());
    }
}
