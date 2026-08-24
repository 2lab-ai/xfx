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
//! single-writer rule exists to prevent. Phase 1 has one thread and the
//! comparison is always true there; it is written now because the hook is
//! process-global and installed once, and the arrival of a second thread is not
//! the moment to be discovering that it was never guarded.

use super::term;

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
        }
        previous(info);
    }));
}
