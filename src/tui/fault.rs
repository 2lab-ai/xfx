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
    // Consumed by the worker and the frame budget, which are Task 11 of this
    // plan. They are named here rather than there because the environment
    // variable's vocabulary is this one enum, and a second place to add a
    // spelling is a second place for the two to disagree.
    #[allow(dead_code)]
    WorkerTurn,
    #[allow(dead_code)]
    SlowUi,
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
        }
    }
}

/// Whether this run was asked to fail at `point`.
pub(crate) fn injected(point: Fault) -> bool {
    std::env::var_os(FAULT_ENV).is_some_and(|value| value == point.name())
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
