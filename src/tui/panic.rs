//! Who restores the terminal when the process is coming apart.
//!
//! A panic is not a signal: it unwinds past the `term::shutdown` the ordinary
//! exit runs, so without a hook a panicking session leaves the terminal raw and
//! prints its own report into it. The hook here restores first and reports
//! second, which is the only order in which the report is readable.
//!
//! It restores **only** for the thread that owns the terminal. A hook that
//! restored unconditionally would let a worker's panic cook the terminal while
//! the UI thread is still painting, which is the same double-writer bug the
//! single-writer rule exists to prevent.
//!
//! # And it **prints** only for a panic nobody else will report
//!
//! Not restoring is half of what a non-owner owes; the other half is not
//! writing. The default report is a `write` to standard error, and standard
//! error is the same raw terminal the UI thread is painting a band onto -- so a
//! runtime thread whose panic is printed by the default hook is a second writer
//! on that terminal even though it restored nothing. The report lands in the
//! middle of a frame, before any restore, and the user reads it out of a torn
//! band or not at all.
//!
//! `crate::tui::worker` catches its turns' panics and hands them to the UI as a
//! `UiEvent::Fatal`, which is printed once, by the UI, after the terminal has
//! been given back. [`caught_on_this_thread`] is how the hook is told that: for
//! the length of the poll that may panic, this thread's panics are *somebody
//! else's to report*, and the hook stays silent. Every other thread's panic is
//! reported exactly as it always was -- including the deliberate one the
//! restoration matrix takes on a thread that owns nothing.

use std::cell::Cell;

use super::term;

thread_local! {
    /// Whether a panic on this thread is going to be caught and reported as
    /// data by whoever caught it.
    ///
    /// A `Cell<bool>` with a `const` initializer, so reading it from inside a
    /// panic hook allocates nothing and runs no lazy initializer.
    static REPORTED_ELSEWHERE: Cell<bool> = const { Cell::new(false) };
}

/// Marks this thread's panics as somebody else's to report, until it is dropped.
///
/// The previous value is restored rather than cleared, so a nested marking
/// cannot silently un-mark the region it is inside.
#[must_use]
pub(crate) struct CaughtHere(bool);

impl Drop for CaughtHere {
    fn drop(&mut self) {
        let previous = self.0;
        REPORTED_ELSEWHERE.with(|flag| flag.set(previous));
    }
}

/// See [`CaughtHere`]. Held for exactly as long as the catch covers.
pub(crate) fn caught_on_this_thread() -> CaughtHere {
    CaughtHere(REPORTED_ELSEWHERE.with(|flag| flag.replace(true)))
}

/// Whether the panic now unwinding this thread will be reported by its catcher.
///
/// `try_with` rather than `with`: a panic raised while this thread's local
/// storage is being torn down would otherwise panic *again* inside the hook.
/// The fallback is `false` -- report it -- because a thread far enough along to
/// have lost its locals is not a thread inside a `catch_unwind` that will say
/// anything about it.
fn reported_elsewhere() -> bool {
    REPORTED_ELSEWHERE.try_with(Cell::get).unwrap_or(false)
}

/// Installs the hook that restores the terminal, then lets the previous hook
/// report the panic as it always would.
///
/// Called once, immediately after [`term::adopt`]: the hook restores what
/// `adopt` recorded, so a hook installed before it would run and restore
/// nothing.
pub(crate) fn install_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if term::ui_thread() == Some(std::thread::current().id()) {
            // Before the previous hook prints, so the message lands on a
            // cooked terminal and is readable rather than painted into a torn
            // band. The abnormal restore pair is the right one here: this is
            // by definition not the planned exit.
            term::restore_pair();
            previous(info);
            return;
        }
        // A thread that owns nothing restores nothing -- and writes nothing
        // either, when its panic is already on its way to the UI as data. The
        // terminal is still raw and its owner is still painting; a report
        // written here would be a second writer on it, and a duplicate of the
        // one the UI prints after the restore.
        if reported_elsewhere() {
            return;
        }
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mark_is_this_thread_s_alone_and_lasts_exactly_as_long_as_the_token() {
        assert!(!reported_elsewhere(), "a fresh thread starts unmarked");
        {
            let _marked = caught_on_this_thread();
            assert!(reported_elsewhere());
            // Another thread's panics are still its own to report: the flag is
            // thread-local because the catch is.
            let elsewhere = std::thread::spawn(reported_elsewhere)
                .join()
                .expect("read the flag on another thread");
            assert!(!elsewhere, "one thread's catch silenced another thread");
        }
        assert!(!reported_elsewhere(), "the mark outlived its token");
    }

    #[test]
    fn a_nested_mark_gives_the_region_it_is_inside_its_value_back() {
        let outer = caught_on_this_thread();
        {
            let _inner = caught_on_this_thread();
            assert!(reported_elsewhere());
        }
        assert!(
            reported_elsewhere(),
            "an inner token cleared the outer one's mark"
        );
        drop(outer);
        assert!(!reported_elsewhere());
    }
}
