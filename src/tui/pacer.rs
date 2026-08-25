//! The pacer: answer text in as fast as the provider sends it, out at a rate a
//! person can read.
//!
//! A provider does not stream at a human rate. It arrives in bursts -- a
//! kilobyte in one frame, nothing for two hundred milliseconds, three kilobytes
//! in the next -- and a UI that appended each burst the instant it landed would
//! show an answer as a series of jumps. Upstream's answer is a queue and a
//! clock: text is enqueued as it arrives and released at
//! [`cps`] bytes a second, which is the **backlog** divided by
//! [`BACKLOG_TARGET`] and clamped between [`MIN_CPS`] and [`MAX_CPS`]. So the
//! rate follows the answer: a short one trickles at the floor, a long one runs
//! at the ceiling, and both are steady rather than bursty.
//!
//! Three rules make that safe rather than merely pretty:
//!
//! * **A finished turn drains.** [`finish`](Pacer::finish) switches the divisor
//!   to [`DRAIN_TARGET`], so what is left of an answer whose turn is over is on
//!   the screen in about a fifth of a second rather than at reading speed. The
//!   ceiling still applies -- a very large backlog takes longer than the target,
//!   which is the deliberate cost of never flooding a serial line.
//! * **An escape sequence is emitted whole or not at all.** The budget is
//!   counted in bytes, and a sequence that does not fit what is left of one
//!   waits for the next tick (`pacer.zig:339`). Half a `CSI` on a terminal is
//!   not half a colour: it is a terminal that swallows the next character it is
//!   given.
//! * **What the band's repaint closed is re-opened.** Every frame repaints the
//!   whole band, and a painter that ends its rows with a reset (Task 15's
//!   palette; `plan:3515`) turns the terminal's attributes off between two
//!   halves of one sentence. [`SgrState`] remembers what the emitted text left
//!   open and [`reopen`](SgrState::reopen) is prefixed to the next emission, so
//!   the second half of a bold line is still bold.
//!
//! **Nothing is dropped and nothing is unbounded.** The queue is a plain
//! `String`, so the bound is not here: it is at the door. `Shell` refuses to
//! take another `UiEvent` off the channel while [`pending`](Pacer::pending) is
//! at or past `shell::PACED_BACKLOG`, the channel then fills, and the runtime
//! parks in its `send().await` -- real backpressure to the socket instead of a
//! buffer that grows until the answer ends. The other end of the promise is
//! [`drain`](Pacer::drain): a session on its way out emits the whole queue at
//! once, because Phase 1 never repaints a document row and text still held here
//! when the band comes down would be text the user never sees.

use std::time::{Duration, Instant};

use unicode_segmentation::UnicodeSegmentation;

/// The slowest a stream is released, whatever the backlog says.
///
/// A short answer still has to move: a backlog of a hundred bytes divided by
/// [`BACKLOG_TARGET`] is sixty-six bytes a second, and a sentence delivered at
/// that rate reads as a stall rather than as a stream.
pub(crate) const MIN_CPS: u32 = 400;

/// The fastest, whatever the backlog says.
///
/// The ceiling is the band's protection rather than the reader's: every
/// emission is an append plus a frame, and a rate the painter cannot keep up
/// with turns a smooth stream into a queue of frames nobody has painted yet.
pub(crate) const MAX_CPS: u32 = 5000;

/// How long the backlog of a running turn is aimed at taking.
pub(crate) const BACKLOG_TARGET: Duration = Duration::from_millis(1500);

/// And of a turn that is over.
pub(crate) const DRAIN_TARGET: Duration = Duration::from_millis(200);

/// How many units of [`Pacer::credit`] make one byte.
///
/// A billion, because that is what makes earning time exact: a rate in bytes a
/// second times an interval in nanoseconds is a whole number of these, and the
/// nanosecond is the finest an `Instant` has.
const A_BYTE: i128 = 1_000_000_000;

/// The queue, the clock, and what the last emission left open.
pub(crate) struct Pacer {
    /// The text waiting to be released, oldest first.
    queue: String,
    /// When the clock was last read, or `None` before the first tick.
    last: Option<Instant>,
    /// What the ticks so far are owed and have not been paid, in **billionths
    /// of a byte** ([`A_BYTE`]).
    ///
    /// The whole reason a fraction is kept at all. A budget is a rate in bytes
    /// a second against an interval, so it is almost never a whole number of
    /// bytes: the loop ticks every 8 ms and [`MIN_CPS`] is 400 a second, which
    /// is **3.2 bytes a tick**. Rounding that to 3 and throwing the rest away
    /// is not a rounding error, it is a *rate* error -- the same fraction is
    /// discarded every tick, so the stream runs at 375 bytes a second rather
    /// than 400 and the gap grows without bound over an answer. Other rates err
    /// the other way and run fast.
    ///
    /// Carried instead, so the cumulative release after N ticks is the rate
    /// times the elapsed time to within half a byte, whatever the tick length
    /// and whatever the rate.
    ///
    /// **The unit is the nanosecond's, and that is the point of it being this
    /// small.** A rate in bytes a second against an interval in nanoseconds is
    /// an exact whole number of billionths of a byte -- no division happens
    /// when time is earned, so there is no remainder to lose. Measured in
    /// thousandths it was the same defect one unit down: a caller ticking every
    /// 900 microseconds would truncate every interval to zero milliseconds and
    /// earn nothing at all, forever, and an integer-millisecond test could not
    /// see it. `Instant` has nothing finer than the nanosecond to offer, so
    /// there is no unit below this one for the bug to move down to again.
    ///
    /// Signed, because the rounding may pay half a byte forward; bounded either
    /// way, because what is not spent is capped at the backlog it could have
    /// been spent on.
    credit: i128,
    /// Whether the turn that is filling the queue has ended.
    finished: bool,
    /// Whether the clock is stopped.
    paused: bool,
    /// The attributes the emitted text has left open.
    sgr: SgrState,
}

impl Pacer {
    pub(crate) fn new() -> Self {
        Self {
            queue: String::new(),
            last: None,
            credit: 0,
            finished: false,
            paused: false,
            sgr: SgrState::default(),
        }
    }

    /// Adds answer text to the queue.
    ///
    /// Text arriving **un-finishes** the pacer: a second turn's deltas can land
    /// behind the tail of the first one's, and draining the second at the first
    /// one's deadline would show it faster than the turn that is still running
    /// deserves.
    pub(crate) fn enqueue(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.finished = false;
        self.queue.push_str(text);
    }

    /// The turn is over: what is left is aimed at [`DRAIN_TARGET`] instead.
    pub(crate) fn finish(&mut self) {
        self.finished = true;
    }

    /// Stops and restarts the clock.
    ///
    /// Both edges reset it, so time that passed while a decision was pending is
    /// discarded rather than spent in one burst on the tick after the answer
    /// (`main.zig:2582`). Task 17's approval panel is the caller; nothing in
    /// this phase pauses, because nothing in this phase can ask a question.
    #[allow(dead_code)]
    pub(crate) fn pause(&mut self, paused: bool) {
        if self.paused != paused {
            self.paused = paused;
            self.last = None;
            self.credit = 0;
        }
    }

    /// How many bytes are still waiting.
    pub(crate) fn pending(&self) -> usize {
        self.queue.len()
    }

    /// Releases what this moment is worth, or `None` when that is nothing.
    ///
    /// The elapsed time is turned into [`credit`](Self::credit) at the rate the
    /// backlog asks for, and the credit is spent in whole bytes with the
    /// remainder kept. So a tick too short to be worth a byte is not a tick
    /// thrown away, and a tick worth three and a fifth bytes does not silently
    /// become a rate of three: over any run of ticks what has been released is
    /// the rate times the time, to within half a byte.
    ///
    /// What the budget buys and the emission could not take -- a sequence too
    /// long for it, a glyph that would have been cut in half -- goes back to
    /// the credit rather than being spent on nothing, capped at the backlog so
    /// that waiting for one sequence cannot bank a burst.
    pub(crate) fn tick(&mut self, now: Instant) -> Option<String> {
        if self.paused {
            return None;
        }
        if self.queue.is_empty() {
            // **The clock moves whether or not there is anything to release.**
            // Leaving `last` behind on an idle tick is how a session banks the
            // whole of its idleness: a prompt sitting untouched for a minute
            // and then answered would earn a minute's worth of credit on the
            // first tick after the first delta, and the answer would arrive in
            // one write -- the exact opposite of pacing, and reached by the
            // commonest thing a user does, which is pause between questions.
            //
            // Advancing here rather than re-priming in `enqueue` is what keeps
            // the continuous-stream arithmetic untouched: while a stream is
            // running the queue is never empty, so this branch is not on that
            // path at all. It does put an obligation on the caller --
            // **`tick` must be called every turn of the loop, not only when
            // something is expected** -- which `super::shell::Shell::pace` does
            // unconditionally from `settle_band`.
            self.last = Some(now);
            return None;
        }
        let Some(last) = self.last else {
            // The first tick is a reading of the clock and nothing else: there
            // is no elapsed time to spend, and pretending there is would empty
            // whatever the queue happened to hold at startup in one write.
            self.last = Some(now);
            return None;
        };
        let elapsed = now.saturating_duration_since(last);
        self.last = Some(now);
        let rate = i128::from(cps(self.queue.len(), self.finished));
        // A rate in bytes a second against an interval in **nanoseconds** is an
        // exact count of billionths of a byte, which is what the credit is
        // measured in -- so no division happens here and there is no remainder
        // to lose. Truncating the interval to whole milliseconds first, which
        // is what this used to do, threw away everything under a millisecond on
        // every tick.
        let nanos = i128::try_from(elapsed.as_nanos()).unwrap_or(i128::MAX);
        self.credit = self.credit.saturating_add(rate.saturating_mul(nanos));
        // Rounded rather than truncated, and it is the *cumulative* credit
        // being rounded rather than one tick's worth, so the halves cancel
        // instead of accumulating. The credit left behind is in
        // `[-A_BYTE/2, A_BYTE/2)`, which is what keeps the sum honest and this
        // expression non-negative.
        let budget = usize::try_from((self.credit + A_BYTE / 2) / A_BYTE).unwrap_or(0);
        self.credit -= i128::try_from(budget).unwrap_or(i128::MAX) * A_BYTE;
        let text = self.take(budget);
        let unspent = budget.saturating_sub(text.len());
        self.credit += i128::try_from(unspent).unwrap_or(0) * A_BYTE;
        // Nobody can be owed more bytes than are waiting to be sent. Without
        // this, a tick that could not place an unfinished escape sequence would
        // keep banking its budget, and the sequence's completion would arrive
        // as one burst of everything behind it.
        self.credit = self
            .credit
            .min(i128::try_from(self.queue.len()).unwrap_or(i128::MAX) * A_BYTE);
        if text.is_empty() {
            return None;
        }
        Some(self.wrapped(text))
    }

    /// Releases the whole queue at once, or `None` when it is empty.
    ///
    /// The exit's, and the reason it exists is the one that makes every other
    /// rule here safe: this module holds text the runtime has already produced,
    /// Phase 1 never repaints a document row, and a session that came down
    /// holding a backlog would have eaten the end of an answer. Not a tick with
    /// a large clock -- a pause must not be able to hold text back from an exit
    /// either.
    pub(crate) fn drain(&mut self) -> Option<String> {
        if self.queue.is_empty() {
            return None;
        }
        let text = std::mem::take(&mut self.queue);
        self.last = None;
        self.credit = 0;
        Some(self.wrapped(text))
    }

    /// Throws the queue away, because the screen it was going to be written on
    /// is being erased.
    ///
    /// `/clear` is the only caller and the only justification: the user asked
    /// for the screen *and* the terminal's scrollback to go, and text held here
    /// belongs to the answer they are erasing. Dribbling it onto the blank
    /// screen afterwards would be the surprise, not the loss.
    pub(crate) fn forget(&mut self) {
        self.queue.clear();
        self.last = None;
        self.credit = 0;
        self.sgr = SgrState::default();
    }

    /// One emission: the attributes the last one left open, then the text.
    ///
    /// The prefix is unconditional rather than asked of the band, and that is a
    /// simplification with a proof behind it rather than a shortcut. Re-opening
    /// an attribute the terminal already has is a no-op -- SGR is idempotent --
    /// so the only cost of writing it when no repaint intervened is its bytes;
    /// and in this phase a repaint always does intervene, because every append
    /// asks for a frame (`shell::Shell::owe`). Asking the band would be a
    /// second thing to keep true for no change in what reaches the screen.
    fn wrapped(&mut self, text: String) -> String {
        let reopen = self.sgr.reopen();
        self.sgr.observe(&text);
        if reopen.is_empty() {
            return text;
        }
        let mut out = reopen;
        out.push_str(&text);
        out
    }

    /// Takes up to `budget` bytes off the front of the queue, cutting only
    /// where the terminal can be cut.
    ///
    /// Two kinds of boundary, and neither is negotiable. A grapheme cluster is
    /// one glyph, and half of one is a different glyph or none. An escape
    /// sequence is one instruction, and half of one is a terminal waiting for
    /// the rest of it -- which it will take from the next thing anybody writes,
    /// band rows included.
    fn take(&mut self, budget: usize) -> String {
        // The rest of an unfinished sequence is still on the socket. Waiting
        // for it is right while more text is coming and wrong once it is not:
        // nothing would ever complete it, and the tail of the answer would be
        // held forever behind it.
        let unfinished = if self.finished {
            Unfinished::Take
        } else {
            Unfinished::Wait
        };
        let end = cut(&self.queue, budget, unfinished);
        let tail = self.queue.split_off(end);
        std::mem::replace(&mut self.queue, tail)
    }
}

/// What a cut should do with a sequence at the end of the text that has not
/// finished arriving.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unfinished {
    /// Stop before it. More text is coming and will complete it.
    Wait,
    /// Take it. Either nothing more is coming, or the caller is dividing text
    /// it already holds whole and the next piece continues it.
    Take,
}

/// How much of `text` may be taken without cutting one of the terminal's own
/// units in half.
///
/// At most `limit` bytes, and never inside a grapheme cluster or an escape
/// sequence -- so the answer is always a `char` boundary, which is the least of
/// it. **Zero is a real answer**: it is what a limit smaller than the first
/// unit gets, and a caller that must make progress has to say what it wants
/// done about that rather than assume it away.
///
/// One function because there are two callers with the same question and no
/// reason for two answers: [`Pacer::take`], dividing the queue by what a tick
/// can afford, and `super::bridge::slices`, dividing a delta by what one
/// `UiEvent` may carry.
pub(crate) fn cut(text: &str, limit: usize, unfinished: Unfinished) -> usize {
    let mut end = 0usize;
    while end < limit && end < text.len() {
        let step = unit_at(&text[end..], unfinished);
        if step == 0 || end + step > limit {
            break;
        }
        end += step;
    }
    end
}

/// How many bytes the **indivisible unit** at the head of `text` takes: a whole
/// escape sequence, or a whole grapheme cluster.
///
/// Zero only for empty text, or for a sequence that has not finished arriving
/// when the caller said it would rather [`Unfinished::Wait`] for the rest. Any
/// other non-empty text has a first unit of at least one byte, which is what
/// lets a caller dividing a string guarantee progress.
///
/// **Neither unit has a maximum size.** A grapheme cluster is a base and as
/// many combining marks as somebody cared to write; an escape sequence runs to
/// its final byte, and an `OSC` runs to a `BEL` or an `ESC \` that a hostile or
/// broken producer need never send. Nothing in this crate bounds either on the
/// provider's path -- `super::input`'s `MOST_BYTES` is 32, but that is the
/// **keyboard** decoder and provider text does not pass through it. A caller
/// that needs a bound has to decide what to do when one unit is bigger than it
/// (`super::bridge::slices` does, and says so).
pub(crate) fn unit_at(text: &str, unfinished: Unfinished) -> usize {
    if text.is_empty() {
        return 0;
    }
    if text.starts_with(ESCAPE) {
        return match escape(text) {
            Escape::Complete(len) => len,
            Escape::Incomplete if unfinished == Unfinished::Wait => 0,
            Escape::Incomplete => text.len(),
        };
    }
    text.graphemes(true).next().map_or(text.len(), str::len)
}

/// How fast a backlog of `backlog` bytes is released, in bytes a second.
///
/// The backlog over the target it is aimed at, held inside the clamps. Named
/// for upstream's "characters per second" (`pacer.zig:312-318`); the unit here
/// is the byte, because the byte is what the queue is measured in and what an
/// escape sequence is counted in.
pub(crate) fn cps(backlog: usize, draining: bool) -> u32 {
    let target = if draining {
        DRAIN_TARGET
    } else {
        BACKLOG_TARGET
    };
    let millis = target.as_millis().max(1);
    let rate = (backlog as u128 * 1000) / millis;
    let clamped = rate.clamp(u128::from(MIN_CPS), u128::from(MAX_CPS));
    u32::try_from(clamped).unwrap_or(MAX_CPS)
}

/// The byte that begins every sequence this module refuses to split.
const ESCAPE: char = '\u{1b}';

/// What is at the front of the queue when it begins with an `ESC`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Escape {
    /// A whole sequence, this many bytes long.
    Complete(usize),
    /// The beginning of one. The rest has not arrived.
    Incomplete,
}

/// The escape sequence at the head of `text`, which must begin with `ESC`.
///
/// Deliberately generous about *what* a sequence is and exact about where one
/// ends: the question here is only "how many bytes travel together", and the
/// answer has to be right for sequences this crate will never write, because
/// the text is a provider's. A shape this does not know is one byte -- the
/// `ESC` alone -- which is the reading that cannot swallow text that follows.
fn escape(text: &str) -> Escape {
    let bytes = text.as_bytes();
    let Some(&second) = bytes.get(1) else {
        return Escape::Incomplete;
    };
    match second {
        // CSI: parameter and intermediate bytes, then one final byte.
        b'[' => {
            for (index, &byte) in bytes.iter().enumerate().skip(2) {
                match byte {
                    0x20..=0x3f => {}
                    0x40..=0x7e => return Escape::Complete(index + 1),
                    // Anything else ends the sequence without finishing it: the
                    // `ESC` travels alone and the rest is ordinary text.
                    _ => return Escape::Complete(1),
                }
            }
            Escape::Incomplete
        }
        // OSC: a string terminated by `BEL` or by `ESC \`.
        b']' => {
            for (index, &byte) in bytes.iter().enumerate().skip(2) {
                if byte == 0x07 {
                    return Escape::Complete(index + 1);
                }
                if byte == 0x1b {
                    return match bytes.get(index + 1) {
                        Some(b'\\') => Escape::Complete(index + 2),
                        Some(_) => Escape::Complete(1),
                        None => Escape::Incomplete,
                    };
                }
            }
            Escape::Incomplete
        }
        // `ESC` + intermediates + a final byte, such as a charset selection.
        0x20..=0x2f => {
            for (index, &byte) in bytes.iter().enumerate().skip(2) {
                match byte {
                    0x20..=0x2f => {}
                    0x30..=0x7e => return Escape::Complete(index + 1),
                    _ => return Escape::Complete(1),
                }
            }
            Escape::Incomplete
        }
        // A two-byte escape.
        0x30..=0x7e => Escape::Complete(2),
        // A control byte after an `ESC` is not part of a sequence.
        _ => Escape::Complete(1),
    }
}

/// Whether `text` -- a whole sequence, as [`escape`] measured one -- is an
/// `SGR`: `CSI`, digits and separators, and a final `m`.
///
/// The one shape a row is allowed to carry, and the reason the test is on the
/// *whole* sequence rather than on its first bytes: `CSI ? 2 5 l` and `CSI 2 J`
/// are the same three opening bytes as a colour, and they hide the cursor and
/// erase the screen.
///
/// **This is the SGR *grammar*, not the render allowlist.** It answers "is this
/// sequence an SGR at all", which is the question [`SgrState`] asks of a
/// provider's stream -- every attribute a provider can open, so that the model
/// of what is switched on stays a model of the whole vocabulary. What a *row*
/// may carry is the narrower question, and [`is_palette_sgr`] is that one.
pub(crate) fn is_sgr(text: &str) -> bool {
    let Some(body) = text.strip_prefix("\u{1b}[") else {
        return false;
    };
    let Some(params) = body.strip_suffix('m') else {
        return false;
    };
    params
        .bytes()
        .all(|byte| byte.is_ascii_digit() || byte == b';' || byte == b':')
}

/// How many bytes the colour instruction at the head of `text` takes, when
/// that is what is there.
///
/// The one question the painters ask this module. A colour is the single shape
/// of escape sequence a row may carry ([`super::frame::row_text`]), and it is
/// also the single shape that must cost no columns and never be cut
/// ([`super::wrap::width`]) -- two callers, one answer, so a row cannot be
/// measured by one rule and cut by another.
pub(crate) fn colour_at(text: &str) -> Option<usize> {
    if !text.starts_with(ESCAPE) {
        return None;
    }
    match escape(text) {
        Escape::Complete(len) if is_palette_sgr(&text[..len]) => Some(len),
        _ => None,
    }
}

/// Whether `text` is one of the shapes the band's own painters emit.
///
/// **The render allowlist, narrowed to the palette that exists**
/// ([`super::theme`]), which is a foreground colour and the reset that ends it:
///
/// * `CSI 38 ; 5 ; <n> m` -- a 256-colour foreground.
/// * `CSI 38 ; 2 ; <r> ; <g> ; <b> m` -- a direct-colour foreground.
/// * `CSI 0 m`, and the bare `CSI m` that means the same thing.
///
/// Task 13 left the whole SGR vocabulary trusted because there was no palette
/// yet to narrow it to, and said so in the note this replaces. There is one
/// now, and it emits three shapes -- so the set a row may carry is those three.
/// The attributes this drops are the ones with a cost: `conceal` makes a row
/// invisible, `blink` makes it move, `reverse` and a background colour repaint
/// the whole width of it. None of them can reach a row from a Phase-1 path --
/// `super::bridge::inert` spaces a provider's escapes at the channel -- but an
/// allowlist wider than what is emitted is untested surface, and that is the
/// only argument it has to answer.
///
/// **Widening obligation.** Two things will need this set to grow, and both
/// must grow it deliberately: Task 16, if a hint-row segment wants an intensity
/// (upstream's identity tag is `CSI 1 ; 38 ; 5 ; 255 m`, `render.zig:31`), and
/// whatever admits a provider's own colour to the document -- the `colored and
/// hyperlinked TTY output` row of `docs/parity.md` calls that out of scope
/// today. Until then [`SgrState`] models attributes this will not render, which
/// fails in the safe direction: an attribute that is dropped is a row that is
/// plainer than it could be, never a terminal doing something nobody asked for.
fn is_palette_sgr(text: &str) -> bool {
    let Some(body) = text.strip_prefix("\u{1b}[") else {
        return false;
    };
    let Some(params) = body.strip_suffix('m') else {
        return false;
    };
    let mut params = params.split(';');
    let Some(first) = params.next() else {
        return false;
    };
    // The number of parameters each shape carries after its first, and `0`
    // for the two spellings of a reset.
    let wanted = match first {
        "" | "0" => 0,
        "38" => match params.next() {
            Some("5") => 1,
            Some("2") => 3,
            _ => return false,
        },
        _ => return false,
    };
    let mut seen = 0usize;
    for param in params {
        // A parameter a terminal reads as a number, and nothing else: an empty
        // one is a zero to a terminal, and a `38;5;` with the index left off is
        // not a colour this crate would write.
        if param.is_empty() || !param.bytes().all(|byte| byte.is_ascii_digit()) {
            return false;
        }
        seen += 1;
    }
    seen == wanted
}

/// How many bytes the escape sequence at the head of `text` takes, complete or
/// not, when that is what is there.
///
/// The complement of [`colour_at`] for the painter that has to *remove* what it
/// may not write: a truncated sequence is measured to the end of the text
/// rather than left behind, because the bytes of half a `CSI` are exactly the
/// ones that would take the rest of the sequence from the row after it.
pub(crate) fn escape_at(text: &str) -> Option<usize> {
    if !text.starts_with(ESCAPE) {
        return None;
    }
    match escape(text) {
        Escape::Complete(len) => Some(len),
        Escape::Incomplete => Some(text.len()),
    }
}

/// What the emitted text has left switched on.
///
/// Kept as a small set of **slots** rather than as a list of the sequences that
/// were seen, and that is the difference between a model and a recording. A
/// recording of a stream that changed colour a thousand times is a thousand
/// sequences long and grows with the answer; the slots are nine, because
/// turning a colour on twice leaves one colour on. Every parameter is
/// canonicalized into its own sequence, so `CSI 1;31 m` and `CSI 1 m CSI 31 m`
/// leave the same state -- which is what makes the state a fact about the
/// terminal rather than about how the provider chose to spell it.
///
/// A parameter this does not model is **dropped** rather than kept: replaying a
/// sequence whose meaning is unknown is how a re-open turns into a second
/// instruction, and the cost of not replaying one is a lost attribute rather
/// than a terminal doing something nobody asked for.
#[derive(Debug, Default, Clone)]
pub(crate) struct SgrState {
    /// The open slots, in the order they were opened, so a replay reproduces
    /// the order the provider wrote.
    open: Vec<(Slot, String)>,
}

/// One thing SGR can be in two states about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    Intensity,
    Italic,
    Underline,
    Blink,
    Reverse,
    Conceal,
    Strike,
    Foreground,
    Background,
}

impl SgrState {
    /// Reads `chunk` for the attributes it opens and closes.
    pub(crate) fn observe(&mut self, chunk: &str) {
        let mut rest = chunk;
        while let Some(at) = rest.find(ESCAPE) {
            rest = &rest[at..];
            let len = match escape(rest) {
                Escape::Complete(len) => len,
                Escape::Incomplete => return,
            };
            let (sequence, tail) = rest.split_at(len);
            if is_sgr(sequence) {
                self.apply(sequence);
            }
            rest = tail;
        }
    }

    /// The sequences that put a fresh terminal back into this state.
    pub(crate) fn reopen(&self) -> String {
        self.open
            .iter()
            .map(|(_, sequence)| sequence.as_str())
            .collect()
    }

    /// Applies one SGR sequence, parameter by parameter.
    fn apply(&mut self, sequence: &str) {
        let body = sequence
            .strip_prefix("\u{1b}[")
            .and_then(|body| body.strip_suffix('m'))
            .unwrap_or_default();
        // `CSI m` with no parameters at all is `CSI 0 m`.
        if body.is_empty() {
            self.open.clear();
            return;
        }
        let params: Vec<&str> = body.split(';').collect();
        let mut index = 0;
        while index < params.len() {
            // An empty parameter is a zero, which is how `CSI ;31 m` resets
            // before it colours.
            let code: u16 = params[index].parse().unwrap_or(0);
            // The extended colours carry their own parameters: `38;5;n` and
            // `38;2;r;g;b`. Taken whole or not at all, because a `2` left
            // behind as a parameter of its own is "dim".
            let taken = if matches!(code, 38 | 48 | 58) {
                match params.get(index + 1).map(|kind| kind.parse::<u16>()) {
                    Some(Ok(5)) => 3,
                    Some(Ok(2)) => 5,
                    _ => 1,
                }
            } else {
                1
            };
            let end = (index + taken).min(params.len());
            self.parameter(code, &params[index..end]);
            index = end.max(index + 1);
        }
    }

    /// One parameter, with whatever parameters belong to it.
    fn parameter(&mut self, code: u16, params: &[&str]) {
        match code {
            0 => self.open.clear(),
            1 | 2 => self.set(Slot::Intensity, params),
            3 => self.set(Slot::Italic, params),
            4 => self.set(Slot::Underline, params),
            5 | 6 => self.set(Slot::Blink, params),
            7 => self.set(Slot::Reverse, params),
            8 => self.set(Slot::Conceal, params),
            9 => self.set(Slot::Strike, params),
            22 => self.clear(Slot::Intensity),
            23 => self.clear(Slot::Italic),
            24 => self.clear(Slot::Underline),
            25 => self.clear(Slot::Blink),
            27 => self.clear(Slot::Reverse),
            28 => self.clear(Slot::Conceal),
            29 => self.clear(Slot::Strike),
            30..=38 | 90..=97 => self.set(Slot::Foreground, params),
            39 => self.clear(Slot::Foreground),
            40..=48 | 100..=107 => self.set(Slot::Background, params),
            49 => self.clear(Slot::Background),
            // Everything else -- the underline colours, the fonts, the
            // proportional-spacing pair -- is left out of the model on purpose.
            _ => {}
        }
    }

    /// Opens `slot`, spelled as the one sequence that reproduces it.
    fn set(&mut self, slot: Slot, params: &[&str]) {
        let sequence = format!("\u{1b}[{}m", params.join(";"));
        match self.open.iter_mut().find(|(open, _)| *open == slot) {
            Some(entry) => entry.1 = sequence,
            None => self.open.push((slot, sequence)),
        }
    }

    /// Closes `slot`. Nothing is replayed for it, because a fresh terminal is
    /// already in the state the closing sequence asks for.
    fn clear(&mut self, slot: Slot) {
        self.open.retain(|(open, _)| *open != slot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rate_is_the_backlog_over_a_second_and_a_half_inside_its_clamps() {
        // pacer.zig:312-318 and the constants at :10-13
        assert_eq!(cps(100, false), MIN_CPS, "a small backlog still moves");
        assert_eq!(cps(3000, false), 2000);
        assert_eq!(cps(100_000, false), MAX_CPS);
    }

    #[test]
    fn a_finished_turn_drains_inside_two_hundred_milliseconds() {
        assert_eq!(cps(1000, true), 5000);
        assert_eq!(cps(100_000, true), MAX_CPS);
    }

    #[test]
    fn a_tick_emits_elapsed_times_the_rate_and_no_more() {
        let mut pacer = Pacer::new();
        let start = Instant::now();
        pacer.enqueue(&"x".repeat(1500)); // 1000 cps
        assert_eq!(
            pacer.tick(start),
            None,
            "the first tick has no elapsed time"
        );
        let emitted = pacer
            .tick(start + Duration::from_millis(100))
            .expect("a chunk");
        assert_eq!(emitted.len(), 100);
        assert_eq!(pacer.pending(), 1400);
    }

    #[test]
    fn an_incomplete_escape_sequence_waits_for_the_next_tick() {
        // pacer.zig:339: sequences are emitted atomically or not at all.
        //
        // The brief's version of this case reads the clock for the first time
        // *inside* the assertion, which makes the budget the distance between
        // two `Instant::now()` calls plus four milliseconds rather than four.
        // The extra tick primes the clock, exactly as the case above does, so
        // the budget is two bytes on any machine.
        let mut pacer = Pacer::new();
        let start = Instant::now();
        pacer.enqueue("ab\u{1b}[31mcd");
        assert_eq!(
            pacer.tick(start),
            None,
            "the first tick has no elapsed time"
        );
        let emitted = pacer
            .tick(start + Duration::from_millis(4))
            .expect("a chunk");
        assert_eq!(
            emitted, "ab",
            "half an escape sequence reached the terminal"
        );
    }

    #[test]
    fn a_paused_pacer_emits_nothing_and_loses_nothing() {
        // The clock freezes while a decision is pending (`main.zig:2582`).
        let mut pacer = Pacer::new();
        let start = Instant::now();
        pacer.enqueue(&"x".repeat(600));
        pacer.pause(true);
        assert_eq!(pacer.tick(start + Duration::from_millis(500)), None);
        pacer.pause(false);
        assert_eq!(pacer.pending(), 600);
    }

    #[test]
    fn open_attributes_are_reopened_after_another_painter_resets_them() {
        // The trap upstream left in a comment: the band's repaint writes
        // `\x1b[0m` between emissions, and the next chunk must not inherit
        // plain text where the model asked for bold red.
        let mut state = SgrState::default();
        state.observe("\u{1b}[1m\u{1b}[31mred");
        assert_eq!(state.reopen(), "\u{1b}[1m\u{1b}[31m");
        state.observe("\u{1b}[0mplain");
        assert_eq!(state.reopen(), "");
    }

    #[test]
    fn a_whole_escape_sequence_travels_on_the_tick_that_can_afford_it() {
        // The other half of the atomicity rule: waiting is not dropping. The
        // sequence and the text after it arrive once the budget covers them.
        let mut pacer = Pacer::new();
        let start = Instant::now();
        pacer.enqueue("ab\u{1b}[31mcd");
        assert_eq!(pacer.tick(start), None);
        assert_eq!(
            pacer.tick(start + Duration::from_millis(4)).as_deref(),
            Some("ab")
        );
        assert_eq!(
            pacer
                .tick(start + Duration::from_millis(30))
                .as_deref()
                .expect("the sequence and its text"),
            "\u{1b}[31mcd"
        );
        assert_eq!(pacer.pending(), 0);
    }

    /// Runs `ticks` ticks `period` milliseconds apart with the backlog held at
    /// `backlog`, and reports the cumulative bytes released after each one.
    ///
    /// Held rather than allowed to fall, because the rate is a function of the
    /// backlog: a queue that drains as it is measured changes the very number
    /// the case is about. What is emitted is put straight back, so `cps` reads
    /// the same value on every tick and the claim is about the arithmetic
    /// rather than about the decay.
    fn steady(backlog: usize, steps: &[Duration]) -> Vec<(Duration, usize)> {
        let mut pacer = Pacer::new();
        let start = Instant::now();
        pacer.enqueue(&"x".repeat(backlog));
        assert_eq!(pacer.tick(start), None, "the clock's first reading");
        let mut cumulative = 0usize;
        let mut elapsed = Duration::ZERO;
        steps
            .iter()
            .enumerate()
            .map(|(index, step)| {
                elapsed += *step;
                let released = pacer.tick(start + elapsed).map_or(0, |text| text.len());
                cumulative += released;
                pacer.enqueue(&"x".repeat(released));
                assert_eq!(
                    pacer.pending(),
                    backlog,
                    "the backlog moved on tick {}",
                    index + 1
                );
                (elapsed, cumulative)
            })
            .collect()
    }

    /// What a rate owes after an interval: the exact product, rounded once.
    fn owed(rate: u64, elapsed: Duration) -> usize {
        let nanos = u128::from(rate) * elapsed.as_nanos();
        usize::try_from((nanos + 500_000_000) / 1_000_000_000).expect("a count")
    }

    #[test]
    fn a_run_of_ticks_releases_the_rate_and_not_the_rounding() {
        // A budget is a rate in bytes a second against an interval in
        // milliseconds, so it is almost never a whole number of bytes -- and at
        // the loop's own 8 ms tick `MIN_CPS` is **3.2 bytes**. Rounding each
        // tick on its own and dropping the remainder is not a rounding error,
        // it is a rate error: the same fifth of a byte goes every tick, the
        // stream runs at 375 bytes a second instead of 400, and after a
        // thousand ticks it is two hundred bytes behind. Other rates err the
        // other way and run fast.
        //
        // So the claim is cumulative rather than per tick: after N ticks the
        // bytes released are the rate times the elapsed time, rounded once.
        // Four cases, chosen so the per-tick fraction rounds *down* (3.2, 1.2),
        // rounds *up* (4.8, 2.8), and is checked at the floor rate and above
        // it.
        for (backlog, rate, period) in [
            (600usize, 400u64, 8u64),
            (900, 600, 8),
            (600, 400, 3),
            (600, 400, 7),
        ] {
            assert_eq!(
                cps(backlog, false),
                u32::try_from(rate).expect("a rate"),
                "the case does not hold the rate it claims"
            );
            let steps = vec![Duration::from_millis(period); 200];
            let seen = steady(backlog, &steps);
            for (index, (elapsed, cumulative)) in seen.iter().enumerate() {
                assert_eq!(
                    *cumulative,
                    owed(rate, *elapsed),
                    "{rate} bytes a second on a {period} ms tick, after {} of them",
                    index + 1
                );
            }
            // and the drift a per-tick rounding would leave is really there to
            // be caught: this is what the naive budget would have delivered.
            let ticks = u64::try_from(steps.len()).expect("a count");
            let naive = usize::try_from(ticks * ((rate * period + 500) / 1000)).expect("a count");
            let honest = seen.last().expect("a tick").1;
            if naive != honest {
                assert!(
                    honest.abs_diff(naive) > 10,
                    "the case is too short for the drift to show"
                );
            }
        }
    }

    #[test]
    fn a_tick_shorter_than_a_millisecond_is_still_time() {
        // The same systematic loss as the case above, one unit further down,
        // and invisible to every test that scripts its clock in whole
        // milliseconds. The interval used to be truncated to `as_millis()`
        // before it was turned into credit, so a caller ticking every 900
        // microseconds earned **zero** on every tick -- not a slow stream, a
        // stopped one, forever. Credit is counted in billionths of a byte
        // against an interval in nanoseconds now, which is exact.
        let rate = 400u64;
        let step = Duration::from_micros(900);
        let steps = vec![step; 400];
        let seen = steady(600, &steps);
        for (index, (elapsed, cumulative)) in seen.iter().enumerate() {
            assert_eq!(
                *cumulative,
                owed(rate, *elapsed),
                "after {} ticks of 900 microseconds",
                index + 1
            );
        }
        assert!(
            seen.last().expect("a tick").1 > 100,
            "nothing was released at all across 360 milliseconds"
        );
    }

    #[test]
    fn a_clock_that_does_not_tick_evenly_is_paid_for_what_it_measured() {
        // A real loop does not tick on a metronome: a turn is `TICK` at its
        // longest and immediate when input or a poked wakeup arrives, so the
        // intervals are a jumble of sub-millisecond and multi-millisecond ones.
        // What is owed is a function of the elapsed time and nothing else.
        let rate = 400u64;
        let pattern = [
            Duration::from_nanos(1),
            Duration::from_micros(300),
            Duration::from_micros(1700),
            Duration::from_micros(900),
            Duration::from_millis(8),
            Duration::from_micros(50),
            Duration::from_millis(5),
        ];
        let steps: Vec<Duration> = pattern.iter().copied().cycle().take(210).collect();
        let seen = steady(600, &steps);
        for (index, (elapsed, cumulative)) in seen.iter().enumerate() {
            assert_eq!(
                *cumulative,
                owed(rate, *elapsed),
                "after {} uneven ticks ({elapsed:?})",
                index + 1
            );
        }
    }

    #[test]
    fn an_idle_gap_is_not_credit_the_next_answer_can_spend() {
        // A session is idle far more of the time than it is streaming, and the
        // clock does not stop while it is. If an idle tick left `last` behind,
        // the interval the next answer's first tick measured would be *the
        // whole pause* -- so a user who asked a question, read the answer, and
        // came back a minute later would have their next answer delivered in
        // one write. The pacer would be off for exactly as long as anybody was
        // using xfx like a person.
        //
        // Filled, drained, idled on a scripted clock, then filled again.
        let mut pacer = Pacer::new();
        let start = Instant::now();
        let mut at = 0u64;
        pacer.enqueue("a first answer");
        assert_eq!(pacer.tick(start), None);
        while pacer.pending() > 0 {
            at += 8;
            assert!(at < 1000, "the first answer never drained");
            pacer.tick(start + Duration::from_millis(at));
        }

        // Eight hundred milliseconds of a loop that has nothing to do, ticked
        // exactly as the real one ticks it.
        for _ in 0..100 {
            at += 8;
            assert_eq!(
                pacer.tick(start + Duration::from_millis(at)),
                None,
                "an empty queue released something"
            );
        }

        // And the next turn's first delta. Six hundred bytes is `MIN_CPS`, so
        // one tick of it is three bytes and not six hundred.
        pacer.enqueue(&"x".repeat(600));
        at += 8;
        let released = pacer
            .tick(start + Duration::from_millis(at))
            .expect("a chunk");
        assert_eq!(
            released.len(),
            3,
            "the pause was banked and paid to the answer that followed it"
        );
    }

    #[test]
    fn a_tick_that_released_nothing_does_not_pay_for_its_time_twice() {
        // The other half of carrying the clock. The elapsed interval is turned
        // into credit *before* anything is emitted, so the clock has to move
        // whether or not the emission could spend it -- leaving it where it was
        // "because nothing came out" charges the same milliseconds again on the
        // next tick, and a queue of wide glyphs (which cannot be released one
        // byte at a time) drains at twice the rate it was asked for.
        //
        // Five glyphs, fifteen bytes, four hundred bytes a second: thirty-seven
        // and a half milliseconds of stream, and the ticks are one millisecond
        // apart so most of them release nothing at all.
        let mut pacer = Pacer::new();
        let start = Instant::now();
        pacer.enqueue("\u{c548}\u{b155}\u{d558}\u{c138}\u{c694}");
        assert_eq!(pacer.tick(start), None);
        let mut drained_at = None;
        for millis in 1..=80u64 {
            pacer.tick(start + Duration::from_millis(millis));
            if pacer.pending() == 0 {
                drained_at = Some(millis);
                break;
            }
        }
        assert_eq!(
            drained_at,
            Some(37),
            "fifteen bytes at four hundred a second is thirty-seven \
             milliseconds of stream"
        );
    }

    #[test]
    fn credit_banked_while_a_sequence_was_unfinished_is_not_a_burst() {
        // What the cap on unspent credit is for. A tick whose budget could not
        // place an escape sequence gives that budget back -- it was not spent
        // on anything -- and a stream that waits a second for the rest of a
        // sequence would otherwise be owed a second's worth of bytes the
        // instant it arrives, and pay it out in one write. Nobody can be owed
        // more than is waiting to be sent.
        let mut pacer = Pacer::new();
        let start = Instant::now();
        pacer.enqueue("\u{1b}[");
        assert_eq!(pacer.tick(start), None);
        assert_eq!(
            pacer.tick(start + Duration::from_millis(1000)),
            None,
            "an unfinished sequence was released while the turn was running"
        );

        pacer.enqueue(&format!("31m{}", "x".repeat(1000)));
        let released = pacer
            .tick(start + Duration::from_millis(1008))
            .expect("the sequence and what it fits")
            .len();
        assert!(
            released < 20,
            "a second of banked credit came out in one write: {released} bytes"
        );
    }

    #[test]
    fn a_tick_that_cannot_afford_a_byte_keeps_the_time_for_the_next_one() {
        // A tick worth less than one whole byte is still worth something. The
        // interval it measured becomes credit like any other, and what the
        // rounding cannot spend stays on the books -- so two ticks of a
        // millisecond at four hundred bytes a second pay for the byte that one
        // of them could not. A loop ticking faster than the rate can afford is
        // the ordinary case, not the exception: at 8 ms and `MIN_CPS` a tick is
        // three and a fifth bytes, and every fifth one of those fifths is a
        // byte somebody is owed.
        let mut pacer = Pacer::new();
        let start = Instant::now();
        pacer.enqueue("abcd"); // 400 cps: one byte per 2.5 ms
        assert_eq!(pacer.tick(start), None);
        assert_eq!(pacer.tick(start + Duration::from_millis(1)), None);
        assert_eq!(
            pacer.tick(start + Duration::from_millis(2)).as_deref(),
            Some("a"),
            "the two milliseconds since the first tick were thrown away"
        );
    }

    #[test]
    fn nothing_is_cut_in_the_middle_of_a_glyph() {
        // A budget of one byte cannot pay for a three-byte cluster, and paying
        // for part of it would put a byte on the terminal that is not a
        // character at all.
        let mut pacer = Pacer::new();
        let start = Instant::now();
        pacer.enqueue("\u{c548}\u{b155}");
        assert_eq!(pacer.tick(start), None);
        assert_eq!(
            pacer.tick(start + Duration::from_millis(3)),
            None,
            "a budget of one byte took part of a glyph"
        );
        assert_eq!(
            pacer.tick(start + Duration::from_millis(8)).as_deref(),
            Some("\u{c548}")
        );
    }

    #[test]
    fn a_finished_turn_releases_a_sequence_that_will_never_be_completed() {
        // The deadlock the "wait for the rest of it" rule would otherwise be:
        // a provider that stopped mid-sequence is a provider that is not going
        // to finish it, and the text *after* it in the queue is the answer.
        let mut pacer = Pacer::new();
        let start = Instant::now();
        pacer.enqueue("ab\u{1b}[");
        assert_eq!(pacer.tick(start), None);
        assert_eq!(
            pacer.tick(start + Duration::from_millis(4)).as_deref(),
            Some("ab")
        );
        assert_eq!(
            pacer.tick(start + Duration::from_millis(30)),
            None,
            "an unfinished sequence left while the turn is still running"
        );
        pacer.finish();
        assert_eq!(
            pacer.tick(start + Duration::from_millis(60)).as_deref(),
            Some("\u{1b}["),
            "the queue was held forever behind a sequence nothing will finish"
        );
    }

    #[test]
    fn the_exit_takes_everything_that_is_left() {
        // Phase 1 never repaints a document row, so what is still here when the
        // band comes down is text the user will never see.
        let mut pacer = Pacer::new();
        pacer.enqueue(&"x".repeat(9000));
        pacer.pause(true);
        assert_eq!(
            pacer.drain().expect("the whole queue").len(),
            9000,
            "a paused pacer held an answer back from the exit"
        );
        assert_eq!(pacer.pending(), 0);
        assert_eq!(pacer.drain(), None);
    }

    #[test]
    fn an_emission_carries_the_attributes_the_one_before_it_left_open() {
        // The band repaints between two emissions and its rows end with a
        // reset, so the second half of a bold sentence has to say so again.
        let mut pacer = Pacer::new();
        let start = Instant::now();
        pacer.enqueue("\u{1b}[1mbold");
        assert_eq!(pacer.tick(start), None);
        let first = pacer.drain().expect("the text");
        assert_eq!(first, "\u{1b}[1mbold");
        pacer.enqueue(" more");
        assert_eq!(
            pacer.drain().as_deref(),
            Some("\u{1b}[1m more"),
            "the second half of the sentence was painted plain"
        );
    }

    #[test]
    fn a_reset_the_provider_wrote_is_not_reopened() {
        let mut pacer = Pacer::new();
        pacer.enqueue("\u{1b}[1mbold\u{1b}[0m");
        assert_eq!(pacer.drain().as_deref(), Some("\u{1b}[1mbold\u{1b}[0m"));
        pacer.enqueue("plain");
        assert_eq!(pacer.drain().as_deref(), Some("plain"));
    }

    #[test]
    fn the_state_is_a_set_of_slots_rather_than_a_recording_of_the_stream() {
        // A thousand colour changes leave one colour on. This is what keeps the
        // re-open bounded by the alphabet instead of by the answer's length.
        let mut state = SgrState::default();
        for code in 30..=37 {
            state.observe(&format!("\u{1b}[{code}m"));
        }
        for code in 40..=47 {
            state.observe(&format!("\u{1b}[{code}m"));
        }
        assert_eq!(state.reopen(), "\u{1b}[37m\u{1b}[47m");
    }

    #[test]
    fn one_sequence_with_several_parameters_sets_each_of_them() {
        // `CSI 1;31 m` and `CSI 1 m CSI 31 m` are the same instruction, so they
        // have to leave the same state -- otherwise what is re-opened depends
        // on how the provider chose to spell it.
        let mut joined = SgrState::default();
        joined.observe("\u{1b}[1;31m");
        let mut apart = SgrState::default();
        apart.observe("\u{1b}[1m\u{1b}[31m");
        assert_eq!(joined.reopen(), apart.reopen());
        assert_eq!(joined.reopen(), "\u{1b}[1m\u{1b}[31m");
    }

    #[test]
    fn an_extended_colour_keeps_the_parameters_that_belong_to_it() {
        // `38;5;n` and `38;2;r;g;b` are one instruction. Split, the `2` becomes
        // "dim" and the components become colours of their own.
        let mut state = SgrState::default();
        state.observe("\u{1b}[38;2;10;20;30m");
        assert_eq!(state.reopen(), "\u{1b}[38;2;10;20;30m");
        state.observe("\u{1b}[38;5;200m");
        assert_eq!(state.reopen(), "\u{1b}[38;5;200m");
    }

    #[test]
    fn the_off_codes_close_what_their_on_codes_opened() {
        let mut state = SgrState::default();
        state.observe("\u{1b}[1m\u{1b}[4m\u{1b}[31m\u{1b}[41m");
        assert_eq!(state.reopen(), "\u{1b}[1m\u{1b}[4m\u{1b}[31m\u{1b}[41m");
        state.observe("\u{1b}[24m\u{1b}[39m");
        assert_eq!(state.reopen(), "\u{1b}[1m\u{1b}[41m");
        state.observe("\u{1b}[22m\u{1b}[49m");
        assert_eq!(state.reopen(), "");
    }

    #[test]
    fn a_sequence_that_is_not_a_colour_changes_no_attribute() {
        // The whole point of the allowlist being on the *whole* sequence: these
        // share their opening bytes with a colour and none of them is one.
        let mut state = SgrState::default();
        state.observe("\u{1b}[2J\u{1b}[?25l\u{1b}[H\u{1b}]0;title\u{7}");
        assert_eq!(state.reopen(), "");
        assert!(!is_sgr("\u{1b}[?25l"));
        assert!(!is_sgr("\u{1b}[2J"));
        assert!(!is_sgr("\u{1b}]0;title\u{7}"));
        assert!(is_sgr("\u{1b}[38;5;200m"));
        assert!(is_sgr("\u{1b}[1m"));
        assert!(is_sgr("\u{1b}[m"), "a reset is a colour instruction too");
    }

    #[test]
    fn a_row_may_carry_only_the_shapes_the_palette_paints_in() {
        // The render allowlist is the palette's own three shapes, and it is
        // narrower than the SGR grammar above on purpose: an attribute nothing
        // emits is surface nothing tests. Asserted through `colour_at`, because
        // that -- not the predicate -- is what `frame::row_text` and
        // `wrap::width` both ask.
        for painted in [
            "\u{1b}[38;5;240m",
            "\u{1b}[38;5;255m",
            "\u{1b}[38;2;88;88;88m",
            "\u{1b}[0m",
            "\u{1b}[m",
        ] {
            assert_eq!(
                colour_at(painted),
                Some(painted.len()),
                "the palette cannot paint {painted:?}"
            );
        }
        // Every one of these is a well-formed SGR the grammar accepts, and none
        // of them is a shape this crate writes. The first four are the ones
        // with a cost on a shared screen; the last two are well-formed colours
        // in spellings the palette does not use.
        for refused in [
            "\u{1b}[5m",
            "\u{1b}[7m",
            "\u{1b}[8m",
            "\u{1b}[48;5;196m",
            "\u{1b}[1m",
            "\u{1b}[31m",
            "\u{1b}[1;38;5;255m",
            "\u{1b}[38;5m",
            "\u{1b}[38;5;m",
            "\u{1b}[38;2;1;2m",
        ] {
            assert!(is_sgr(refused), "{refused:?} is not an SGR at all");
            assert_eq!(
                colour_at(refused),
                None,
                "a row was allowed to carry {refused:?}"
            );
        }
    }

    #[test]
    fn a_bare_csi_m_is_a_reset() {
        let mut state = SgrState::default();
        state.observe("\u{1b}[1m");
        state.observe("\u{1b}[m");
        assert_eq!(state.reopen(), "");
    }

    #[test]
    fn every_shape_of_sequence_is_measured_to_its_own_end() {
        // What "atomically" means, per shape. A length that ran short would cut
        // a sequence; one that ran long would swallow the text after it.
        assert_eq!(escape("\u{1b}[31mrest"), Escape::Complete(5));
        assert_eq!(escape("\u{1b}[?25lrest"), Escape::Complete(6));
        assert_eq!(escape("\u{1b}]0;title\u{7}rest"), Escape::Complete(10));
        assert_eq!(escape("\u{1b}]0;title\u{1b}\\rest"), Escape::Complete(11));
        assert_eq!(escape("\u{1b}(Brest"), Escape::Complete(3));
        assert_eq!(escape("\u{1b}Mrest"), Escape::Complete(2));
        assert_eq!(escape("\u{1b}"), Escape::Incomplete);
        assert_eq!(escape("\u{1b}[31"), Escape::Incomplete);
        assert_eq!(escape("\u{1b}]0;title"), Escape::Incomplete);
        // An `ESC` followed by a control byte is an `ESC` and then that byte.
        assert_eq!(escape("\u{1b}\u{1b}[31m"), Escape::Complete(1));
        assert_eq!(escape("\u{1b}\n"), Escape::Complete(1));
    }

    /// Everything a stream of emissions put on the terminal, ticked every
    /// `step` milliseconds until the queue is empty.
    fn paced(stream: &str, step: u64) -> String {
        let mut pacer = Pacer::new();
        let start = Instant::now();
        pacer.enqueue(stream);
        pacer.finish();
        let mut seen = String::new();
        let mut at = 0u64;
        while pacer.pending() > 0 {
            at += step;
            assert!(at <= 5000, "{step} ms ticks never drained the queue");
            if let Some(chunk) = pacer.tick(start + Duration::from_millis(at)) {
                seen.push_str(&chunk);
            }
        }
        seen
    }

    /// `text` with every SGR sequence taken out of it, and the sequences.
    fn without_colour(text: &str) -> (String, Vec<String>) {
        let (mut plain, mut colours) = (String::new(), Vec::new());
        let mut rest = text;
        while let Some(at) = rest.find(ESCAPE) {
            let (head, tail) = rest.split_at(at);
            plain.push_str(head);
            let len = match escape(tail) {
                Escape::Complete(len) => len,
                Escape::Incomplete => tail.len(),
            };
            let (sequence, next) = tail.split_at(len);
            if is_sgr(sequence) {
                colours.push(sequence.to_string());
            } else {
                plain.push_str(sequence);
            }
            rest = next;
        }
        plain.push_str(rest);
        (plain, colours)
    }

    #[test]
    fn every_byte_enqueued_is_emitted_once_and_in_order() {
        // The property the budget arithmetic rests on: pacing is a delay, not a
        // filter. Ticked at every length from one millisecond to twenty, with
        // the cutting hazards in the stream -- a wide glyph, an OSC string, a
        // two-byte escape -- and no colour, so the emission is the input byte
        // for byte.
        let stream = "alpha \u{c548}\u{b155} \u{1b}]0;t\u{7} \u{1b}(B omega";
        for step in 1..=20u64 {
            assert_eq!(paced(stream, step), stream, "{step} ms ticks changed it");
        }
    }

    #[test]
    fn pacing_a_coloured_answer_adds_re_opens_and_nothing_else() {
        // The one thing an emission is allowed to add. Strip the colour from
        // both sides and the text has to be identical byte for byte -- so no
        // ordinary character is lost, doubled or moved by the re-open -- and
        // every sequence the provider wrote is still there, in order, inside a
        // stream that only ever re-opens what was already on.
        let stream = "plain \u{1b}[1mbold \u{1b}[31mred\u{1b}[0m after";
        let (expected, written) = without_colour(stream);
        for step in 1..=20u64 {
            let seen = paced(stream, step);
            let (plain, colours) = without_colour(&seen);
            assert_eq!(plain, expected, "{step} ms ticks changed the text");
            for sequence in &written {
                assert!(
                    colours.contains(sequence),
                    "{step} ms ticks dropped {sequence:?}"
                );
            }
            for sequence in &colours {
                assert!(
                    written.contains(sequence),
                    "{step} ms ticks invented {sequence:?}"
                );
            }
        }
    }

    #[test]
    fn a_long_backlog_is_released_no_faster_than_the_ceiling() {
        // The band's protection. A tick of a whole second against a backlog
        // that would ask for a hundred thousand a second still releases
        // `MAX_CPS`.
        let mut pacer = Pacer::new();
        let start = Instant::now();
        pacer.enqueue(&"x".repeat(200_000));
        pacer.finish();
        assert_eq!(pacer.tick(start), None);
        let emitted = pacer
            .tick(start + Duration::from_millis(1000))
            .expect("a chunk");
        assert_eq!(emitted.len(), usize::try_from(MAX_CPS).expect("a count"));
    }

    #[test]
    fn text_arriving_behind_a_finished_turn_is_paced_again() {
        // A second turn's deltas can land behind the tail of the first one's,
        // and the first one's deadline is not theirs.
        let mut pacer = Pacer::new();
        pacer.finish();
        pacer.enqueue(&"x".repeat(1500));
        assert_eq!(cps(pacer.pending(), false), 1000);
        let start = Instant::now();
        assert_eq!(pacer.tick(start), None);
        assert_eq!(
            pacer
                .tick(start + Duration::from_millis(100))
                .expect("a chunk")
                .len(),
            100,
            "the second turn drained at the first turn's deadline"
        );
    }

    #[test]
    fn a_cleared_screen_takes_the_queue_with_it() {
        let mut pacer = Pacer::new();
        pacer.enqueue("\u{1b}[1mfor a screen that is about to be erased");
        pacer.forget();
        assert_eq!(pacer.pending(), 0);
        assert_eq!(pacer.drain(), None);
        pacer.enqueue("after");
        assert_eq!(
            pacer.drain().as_deref(),
            Some("after"),
            "an attribute from the erased screen was re-opened on the new one"
        );
    }
}
