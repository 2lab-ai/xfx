//! Failures this build was asked to produce, so that the restoration matrix has
//! something to drive.
//!
//! Compiled only under the `fault-injection` feature, which is off by default:
//! a shipped binary contains neither the enum nor the branches that consult it,
//! so there is no environment variable a user can set to make a release fail.

/// Where a deliberate failure can be asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fault {
    /// Before the terminal is touched at all: the exit must write nothing.
    BeforeRaw,
    /// After raw mode is entered: the exit must restore before it reports.
    AfterRaw,
    /// While the terminal is raw: the panic hook must restore before the
    /// report is printed.
    UiFrame,
    /// A panic on a thread that does **not** own the terminal: the hook must
    /// leave the terminal exactly as the owner left it.
    NonOwnerPanic,
    /// A panic **inside a turn**, on the runtime thread: it must reach the user
    /// as data rather than as a second writer on the terminal.
    WorkerTurn,
    /// A UI too slow to keep up with what it asked for: it fills the `UiEvent`
    /// channel and parks the producer in `send().await`, which is the state the
    /// drain protocol has to get a session out of.
    SlowUi,
    /// A panic on the UI thread **while the approval screen owns the alternate
    /// buffer**: the hook must give that buffer back as well as the line
    /// discipline.
    ///
    /// It is a row of its own rather than a variant of [`Self::UiFrame`]
    /// because the state it is taken in is one no keystroke can produce and no
    /// other fault reaches: the terminal is on the buffer this session took,
    /// and the restore that answers it has to be the one that leads with
    /// `1049l`. Injected after the entering frame has been written, flushed and
    /// recorded, and before the answer that would give the plane back.
    AlternatePanic,
    /// A terminal that takes **part** of one band frame and then stops taking
    /// bytes at all.
    ///
    /// The one failure this build can produce that a refusing screen cannot:
    /// every other injected write failure leaves the terminal exactly as it
    /// was, and this one leaves it holding a prefix of a synchronized frame.
    /// It is answered by the sink itself (`super::deliver`), so what the
    /// session runs here is the real counting against the real descriptor --
    /// the prefix is genuinely on the user's terminal -- and the containment
    /// that follows is the product's, not the harness's.
    PartialFrame,
    /// The same torn prefix as [`Self::PartialFrame`], but a synchronous
    /// recovery attempt's cleanup vector and rebuild are both answered for
    /// real afterwards, so a session that catches the tear can be watched
    /// recovering end to end on a real descriptor.
    ///
    /// A row of its own rather than a flag on [`Self::PartialFrame`] because
    /// the two must measure opposite things: that one keeps containing a
    /// tear no recovery in this build gets past (its own cleanup vector is
    /// refused too), this one is the band tear recovery is actually asked to
    /// survive.
    PartialFrameOnce,
    /// A terminal that takes the opener of the first update that carries
    /// **document rows** and part of those rows -- not the band behind them --
    /// and then stops, refusing the recovery's cleanup vector too.
    ///
    /// The tear no rebuild can answer: rows may already be in the terminal's
    /// scrollback, so the session ends on it (upstream's
    /// `DocumentAppendInterrupted`, `render_engine/terminal_diff.zig:648-689`)
    /// -- and with the cleanup refused, the update it opened is still open on
    /// the terminal when the exit's restore is written, which is what this row
    /// is for: the restore has to close it (`super::term::RESTORE`).
    PartialRows,
    /// [`Self::PartialFrame`]'s tear, armed on the first **repaint of the
    /// alternate plane** the session already owns rather than on a band frame:
    /// the prefix, the original failure, and the recovery attempt's own cleanup
    /// vector refused too, so the session has to end on it.
    ///
    /// Armed past the frame that takes the plane, and never on the one that
    /// gives it back: a `1049h` or `1049l` the terminal took part of is a plane
    /// transition, which stays contained and fatal and is not what this row
    /// measures.
    PartialAlternate,
    /// [`Self::PartialFrameOnce`]'s tear on that same repaint: the prefix and
    /// the original failure, and then the descriptor is the terminal's own
    /// again, so the cleanup and the rebuild behind it both land and the
    /// question is still on the screen -- and still answerable -- afterwards.
    PartialAlternateOnce,
    /// A screen that refuses every band frame it is shown, from the first one
    /// on, and takes not one byte of any of them.
    ///
    /// The other road out of `disposed` (`super::event_loop`): every other
    /// injected write failure either takes a prefix ([`Self::PartialFrame`])
    /// or answers once, but this one answers *every* frame offered -- so the
    /// only thing that ends the session is the frame budget already sitting
    /// in `FrameFailures`, exactly as it would for a screen that is merely
    /// gone. It answers only a vector that opens a frame: the mode-set and
    /// restore sequences do not, so raw mode is entered and given back for
    /// real and the matrix's exit assertions are still measuring the
    /// product's own restore rather than a terminal this fault silenced.
    RefusesFrames,
    /// **Not a failure**: the Phase-1 whole-band painter, kept as the reference
    /// the cell diff is judged against.
    ///
    /// It is here rather than behind a flag of its own because that is exactly
    /// the property scenario 13 needs -- the two painters selectable on a real
    /// terminal by the harness and by *nothing else*. This enum is compiled
    /// only under `fault-injection`, so a released binary contains neither the
    /// variant nor the branch in `super::frame::Band::commit` that reads it:
    /// there is no environment variable a user can set to get the slow painter
    /// back, and therefore no second painter that has to keep working in the
    /// field.
    FullPaintReference,
    /// A `/trace` report whose bytes are written and synced and not yet
    /// published under its name: the report's thread waits there a minute, so
    /// a test can kill the process at exactly that moment and find no report
    /// (`crate::trace::write_report`).
    TraceBeforePublish,
}

/// The variable a run is asked to fail through.
const FAULT_ENV: &str = "XFX_TUI_FAULT";

impl Fault {
    /// The spelling that names this point on the command line.
    fn name(self) -> &'static str {
        match self {
            Self::BeforeRaw => "before-raw",
            Self::AfterRaw => "after-raw",
            Self::UiFrame => "ui-frame",
            Self::NonOwnerPanic => "non-owner-panic",
            Self::WorkerTurn => "worker-turn",
            Self::SlowUi => "slow-ui",
            Self::AlternatePanic => "alternate-panic",
            Self::PartialFrame => "partial-frame",
            Self::PartialFrameOnce => "partial-frame-once",
            Self::PartialRows => "partial-rows",
            Self::PartialAlternate => "partial-alternate",
            Self::PartialAlternateOnce => "partial-alternate-once",
            Self::RefusesFrames => "frame-refusal",
            Self::FullPaintReference => "full-paint-reference",
            Self::TraceBeforePublish => "trace-before-publish",
        }
    }
}

/// Whether this run was asked to fail at `point`.
pub(crate) fn injected(point: Fault) -> bool {
    std::env::var_os(FAULT_ENV).is_some_and(|value| value == point.name())
}

/// How far the armed fault has got: `0` before anything armed it, `1` with
/// the prefix owed, `2` with the original failure owed, `3` with a recovery
/// cleanup vector owed a refusal too (the containing faults only -- see
/// [`PARTIAL_FRAME_ONCE`]), `4` once it is spent for good.
///
/// A count rather than a flag because the fault is a fixed *sequence* of
/// answers, in order, and exactly once in the life of a process: after the
/// last one the descriptor is the terminal's own again, which is what lets
/// the exit below it restore for real and be measured.
///
/// One sequence for both of the vectors a tear can be armed on -- a band
/// frame ([`arm_partial_frame`]) and a repaint of the owned alternate plane
/// ([`arm_partial_alternate`]) -- because the answers are the same answers
/// and only the moment they start differs; a run asks for exactly one fault,
/// so the two never compete for it.
static PARTIAL_FRAME: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// Which kind of fault armed [`PARTIAL_FRAME`]: `0` unset, `1` a containing
/// one ([`Fault::PartialFrame`], [`Fault::PartialAlternate`]), `2` a
/// recovering one ([`Fault::PartialFrameOnce`], [`Fault::PartialAlternateOnce`]).
///
/// A separate cell rather than folding the choice into [`PARTIAL_FRAME`]'s
/// own numbering because the two kinds share every state up to and
/// including the original failure -- only what happens to the *next* write,
/// the recovery attempt's own cleanup vector, differs.
static PARTIAL_FRAME_ONCE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// Where in the next torn write the tear lands, when the vector is an update
/// whose parts are known ([`mark_update`]); `usize::MAX` for "half of it".
static TEAR_AT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(usize::MAX);

/// The length of the write [`TEAR_AT`] was worked out for. A mark is a fact
/// about **one** vector: an update the output check refused before it reached
/// the terminal leaves its mark behind, and the write that comes next -- of
/// another length -- is cut at its own half instead of at a point measured on
/// bytes it does not have.
static TEAR_LEN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[cfg(test)]
thread_local! {
    /// The parts the last [`mark_update`] on this thread was told, armed or
    /// not: what lets a test read where the composer said the rows end.
    static MARKED: std::cell::Cell<Option<(usize, usize, usize)>> = const { std::cell::Cell::new(None) };
}

/// The parts the last [`mark_update`] on this thread was told: `(opener,
/// rows_end, len)`.
#[cfg(test)]
pub(crate) fn marked() -> Option<(usize, usize, usize)> {
    MARKED.with(std::cell::Cell::get)
}

/// Says where the parts of the update about to be written lie: its opener
/// ends at `opener`, its document rows at `rows_end`, and it is `len` bytes
/// long. Called right before that update's one write.
///
/// What it decides is only **where an armed tear lands**, and it is what makes
/// each row of the matrix mean what its name says however long the launch's
/// welcome line happens to be:
///
/// * [`Fault::PartialFrame`] and [`Fault::PartialFrameOnce`] tear a **band
///   frame**, so they land halfway into the band's part of the update, past
///   every row ([`band_tear`]) -- a tear there is the band tear their rows
///   recover from, or contain, wherever the rows end. An update with no rows
///   is all band, and is cut at its half as before.
/// * [`Fault::PartialRows`] tears **inside the rows**, halfway between the
///   opener and the band: armed here, on the first update that carries any,
///   as a containing fault whose cleanup is refused too.
///
/// The boundary is the composer's own -- the length of the vector when the
/// band's bytes were appended -- rather than one found again in the bytes:
/// nothing a row can contain is then mistaken for where the band begins.
pub(crate) fn mark_update(opener: usize, rows_end: usize, len: usize) {
    use std::sync::atomic::Ordering;

    #[cfg(test)]
    MARKED.with(|marked| marked.set(Some((opener, rows_end, len))));
    // Whatever an earlier update left here was about that update.
    TEAR_AT.store(usize::MAX, Ordering::Release);
    if rows_end <= opener {
        return;
    }
    let at = if injected(Fault::PartialRows) && rows_end > opener + 1 {
        arm(1);
        Some(opener + (rows_end - opener) / 2)
    } else {
        band_tear(rows_end, len)
    };
    if let Some(at) = at.filter(|_| PARTIAL_FRAME.load(Ordering::Acquire) == 1) {
        TEAR_LEN.store(len, Ordering::Release);
        TEAR_AT.store(at, Ordering::Release);
    }
}

/// Halfway into the band's part of an update `len` bytes long whose rows end
/// at `rows_end` -- past every row and short of the end -- or `None` when the
/// update has no band after its rows.
pub(crate) fn band_tear(rows_end: usize, len: usize) -> Option<usize> {
    (len > rows_end).then(|| rows_end + (len - rows_end).div_ceil(2))
}

/// How much of a write of `len` bytes the tear takes: the marked point when
/// the mark was made for a write of this length, half of it otherwise -- and
/// never all of it or none, either of which would be another fault entirely.
fn tear_for(len: usize, marked_at: usize, marked_len: usize) -> usize {
    let at = if marked_at != usize::MAX && marked_len == len {
        marked_at
    } else {
        len / 2
    };
    at.max(1).min(len.saturating_sub(1).max(1))
}

/// What the injected terminal does to the next `write`.
pub(crate) enum Prefix {
    /// Takes this many bytes of what it was offered -- really writes them --
    /// and says so.
    Takes(usize),
    /// Fails, having taken nothing this time.
    Fails,
}

/// Arms the prefix fault, if this run asked for either variant of it and has
/// not had it yet.
///
/// Called where the *first band frame* is about to be built, so the vector it
/// lands on is a frame rather than the mode set: a session that lost the mode
/// set would be a startup failure, which is a row the matrix already has.
pub(crate) fn arm_partial_frame() {
    if injected(Fault::PartialFrame) {
        arm(1);
    } else if injected(Fault::PartialFrameOnce) {
        arm(2);
    }
}

/// Arms the same prefix fault on the first **repaint of the alternate plane**
/// the session already owns, if this run asked for either alternate variant
/// and has not had it yet.
///
/// Called right before that repaint's one emit, and only for a repaint that
/// has bytes: the vector it lands on is then the repaint, never the `1049h`
/// frame that took the plane before it -- nor the `1049l` that gives it back,
/// which an idle tick that armed it would have handed the tear to instead.
pub(crate) fn arm_partial_alternate() {
    if injected(Fault::PartialAlternate) {
        arm(1);
    } else if injected(Fault::PartialAlternateOnce) {
        arm(2);
    }
}

/// Starts [`PARTIAL_FRAME`]'s sequence, once in the life of the process, for a
/// fault of kind `once` ([`PARTIAL_FRAME_ONCE`]'s numbering).
fn arm(once: u8) {
    use std::sync::atomic::Ordering;

    if PARTIAL_FRAME
        .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        PARTIAL_FRAME_ONCE.store(once, Ordering::Release);
    }
}

/// What an armed prefix fault answers for a write of `len` bytes.
///
/// `None` once, and for ever after, the sequence is spent.
pub(crate) fn partial_frame_answer(len: usize) -> Option<Prefix> {
    use std::sync::atomic::Ordering;

    match PARTIAL_FRAME.load(Ordering::Acquire) {
        // Half the vector, and at least one byte of it: a "prefix" of nothing
        // would be a zero-progress failure, which is the other case entirely.
        1 => {
            PARTIAL_FRAME.store(2, Ordering::Release);
            // Where `mark_update` put it, and half the vector otherwise.
            let at = TEAR_AT.swap(usize::MAX, Ordering::AcqRel);
            let marked_len = TEAR_LEN.swap(0, Ordering::AcqRel);
            Some(Prefix::Takes(tear_for(len, at, marked_len)))
        }
        2 => {
            // The original tear. A containing fault (`PartialFrame`,
            // `PartialAlternate`) still owes a refusal to the very next vector
            // -- the recovery attempt's own fixed cleanup write -- so that a
            // session running under it never observes a successful recovery.
            // A recovering one (`PartialFrameOnce`, `PartialAlternateOnce`) is
            // spent here instead: the cleanup and the rebuild behind it both
            // reach the real descriptor.
            let next = if PARTIAL_FRAME_ONCE.load(Ordering::Acquire) == 1 {
                3
            } else {
                4
            };
            PARTIAL_FRAME.store(next, Ordering::Release);
            Some(Prefix::Fails)
        }
        3 => {
            PARTIAL_FRAME.store(4, Ordering::Release);
            Some(Prefix::Fails)
        }
        _ => None,
    }
}

/// The escape prefix a band frame opens with (`frame::BEGIN_FRAME`).
///
/// A private copy of the same bytes, not an import: this module answers "is
/// this vector a frame" without owning any of `frame`'s knowledge of what a
/// frame contains -- the same reason `event_loop`'s own P3-WRAP benchmark
/// keeps a private copy of `frame::ERASE_LINE` rather than exposing either
/// constant beyond the module that defines it.
const FRAME_BEGIN: &[u8] = b"\x1b[?2026h\x1b[?25l";

/// What [`Fault::RefusesFrames`] answers for a write of exactly these bytes:
/// `Some` only for a vector that opens a frame (an update, with or without the
/// window title written ahead of it), so a mode-set or restore
/// sequence -- neither of which does -- reaches the real descriptor
/// unchanged.
///
/// Unlike [`partial_frame_answer`] this has no state and no budget of its
/// own to spend: it says the same thing every time it is asked, for as long
/// as this run was asked for it, and the session's own [`FrameFailures`
/// budget](super::event_loop) is what turns a run of these into a session
/// that ends.
pub(crate) fn frame_refusal_answer(bytes: &[u8]) -> Option<std::io::Error> {
    // Anywhere in it rather than at its head: a frame that retitles the window
    // writes the title ahead of its update (`frame::Band::retitle`).
    if injected(Fault::RefusesFrames)
        && bytes
            .windows(FRAME_BEGIN.len())
            .any(|window| window == FRAME_BEGIN)
    {
        Some(std::io::Error::from_raw_os_error(libc::EIO))
    } else {
        None
    }
}

/// Panics on a thread that is not the one holding the terminal, and waits for
/// it to finish coming apart.
///
/// This is the only second thread a Phase-1 TUI ever has, and it exists so that
/// the panic hook's ownership test is a *test*: with one thread the comparison
/// is always true and deleting it changes nothing observable. It is
/// feature-gated with everything else here, so a shipped build still starts and
/// stays single-threaded.
///
/// Two properties keep it from disturbing the contract it is measuring. The
/// thread is started the way every other thread of a session is
/// ([`super::signals::spawn_unsignalled`]), with the owned signals blocked, so
/// it can never take one from the UI thread -- the standing
/// single-threaded-startup constraint is about a thread that *waits*, and this
/// one only dies. And it is joined before the caller
/// continues, so the session that follows is not racing the panic it asked for:
/// what the terminal looks like afterwards is a settled fact rather than a
/// timing question.
pub(crate) fn panic_off_the_ui_thread() {
    let worker = super::signals::spawn_unsignalled("xfx-fault", || -> () {
        panic!("a turn came apart off the ui thread")
    })
    .expect("a thread for the injected panic");
    // `Err` is the whole point of this call, so the payload is dropped rather
    // than resumed: resuming it here would move the panic back onto the UI
    // thread and measure the opposite of what is being asked.
    let outcome = worker.join();
    assert!(outcome.is_err(), "the injected worker panic did not happen");
}

#[cfg(test)]
mod tests {
    use super::{band_tear, tear_for};

    #[test]
    fn a_band_tear_lands_past_every_row_however_long_the_rows_are() {
        // A first update of 12 opener bytes, 300 of rows and a 40-byte band:
        // half the write is inside the rows, half the band's part is not.
        let (opener, rows_end, len) = (12, 312, 352);
        let at = band_tear(rows_end, len).expect("a band after the rows");
        assert!(rows_end < at && at < len, "{at}");
        assert_eq!(at, 332);
        assert!(
            len / 2 < rows_end,
            "the premise: half the write is in the rows"
        );
        // And with no rows the band is the whole update after the opener.
        assert_eq!(band_tear(opener, len), Some(182));
        assert_eq!(
            band_tear(len, len),
            None,
            "an update with no band after its rows"
        );
    }

    #[test]
    fn a_mark_is_honoured_only_for_the_write_it_was_made_for() {
        assert_eq!(tear_for(352, 332, 352), 332);
        // A refused update left its mark; the next write is another length.
        assert_eq!(tear_for(200, 332, 352), 100);
        // Unmarked: half, and never none or all of a write.
        assert_eq!(tear_for(200, usize::MAX, 0), 100);
        assert_eq!(tear_for(1, usize::MAX, 0), 1);
        assert_eq!(tear_for(2, 2, 2), 1);
        assert_eq!(tear_for(2, 0, 2), 1);
    }
}
