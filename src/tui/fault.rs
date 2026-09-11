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
            Self::RefusesFrames => "frame-refusal",
            Self::FullPaintReference => "full-paint-reference",
        }
    }
}

/// Whether this run was asked to fail at `point`.
pub(crate) fn injected(point: Fault) -> bool {
    std::env::var_os(FAULT_ENV).is_some_and(|value| value == point.name())
}

/// How far [`Fault::PartialFrame`] has got: `0` before anything armed it, `1`
/// with the prefix owed, `2` with the failure owed, `3` once it is spent.
///
/// A count rather than a flag because the fault is **two** answers to two
/// syscalls, in order, and exactly once in the life of a process: after that
/// the descriptor is the terminal's own again, which is what lets the exit
/// below it restore for real and be measured.
static PARTIAL_FRAME: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// What the injected terminal does to the next `write`.
pub(crate) enum Prefix {
    /// Takes this many bytes of what it was offered -- really writes them --
    /// and says so.
    Takes(usize),
    /// Fails, having taken nothing this time.
    Fails,
}

/// Arms the prefix fault, if this run asked for it and has not had it yet.
///
/// Called where the *first band frame* is about to be built, so the vector it
/// lands on is a frame rather than the mode set: a session that lost the mode
/// set would be a startup failure, which is a row the matrix already has.
pub(crate) fn arm_partial_frame() {
    if !injected(Fault::PartialFrame) {
        return;
    }
    let _ = PARTIAL_FRAME.compare_exchange(
        0,
        1,
        std::sync::atomic::Ordering::AcqRel,
        std::sync::atomic::Ordering::Acquire,
    );
}

/// What an armed prefix fault answers for a write of `len` bytes.
///
/// `None` once, and for ever after, the two answers are spent.
pub(crate) fn partial_frame_answer(len: usize) -> Option<Prefix> {
    use std::sync::atomic::Ordering;

    match PARTIAL_FRAME.load(Ordering::Acquire) {
        // Half the vector, and at least one byte of it: a "prefix" of nothing
        // would be a zero-progress failure, which is the other case entirely.
        1 => {
            PARTIAL_FRAME.store(2, Ordering::Release);
            Some(Prefix::Takes((len / 2).max(1).min(len)))
        }
        2 => {
            PARTIAL_FRAME.store(3, Ordering::Release);
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
/// `Some` only for a vector that opens a frame, so a mode-set or restore
/// sequence -- neither of which does -- reaches the real descriptor
/// unchanged.
///
/// Unlike [`partial_frame_answer`] this has no state and no budget of its
/// own to spend: it says the same thing every time it is asked, for as long
/// as this run was asked for it, and the session's own [`FrameFailures`
/// budget](super::event_loop) is what turns a run of these into a session
/// that ends.
pub(crate) fn frame_refusal_answer(bytes: &[u8]) -> Option<std::io::Error> {
    if injected(Fault::RefusesFrames) && bytes.starts_with(FRAME_BEGIN) {
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
/// thread is created after [`super::signals::block_owned`], so it inherits a
/// mask with the owned signals blocked and can never take one from the UI
/// thread -- the standing single-threaded-startup constraint is about a thread
/// that *waits*, and this one only dies. And it is joined before the caller
/// continues, so the session that follows is not racing the panic it asked for:
/// what the terminal looks like afterwards is a settled fact rather than a
/// timing question.
pub(crate) fn panic_off_the_ui_thread() {
    let worker = std::thread::spawn(|| panic!("a turn came apart off the ui thread"));
    // `Err` is the whole point of this call, so the payload is dropped rather
    // than resumed: resuming it here would move the panic back onto the UI
    // thread and measure the opposite of what is being asked.
    let outcome = worker.join();
    assert!(outcome.is_err(), "the injected worker panic did not happen");
}
