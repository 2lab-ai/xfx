//! One owner per signal, and nothing in a handler that is not async-signal-safe.
//!
//! A handler restores and re-raises, or it sets a flag. It never repaints and
//! never returns into the UI's frame state: a cooked terminal with a stale band
//! is a recoverable annoyance, a terminal left raw is not
//! (`.prd/03-tui-port.md` §"Signals").
//!
//! Ownership has a **beginning**, and it is the part that is easy to leave out.
//! Between the `tcsetattr` that makes the terminal raw and the last `sigaction`
//! that gives a signal its owner there is a window in which a `SIGTERM` finds
//! the default disposition and a raw terminal, and kills xfx without restoring
//! anything. The window is closed by blocking those signals across the whole
//! transition -- see [`block_owned`], whose result [`install`] consumes -- so
//! that a signal arriving during it is *held* and delivered into the handler a
//! moment later, rather than taken by the kernel.
//!
//! The stop signal needs more than that, because it has an ending the others do
//! not: `SIGTSTP` does not end the process, it hands the terminal back and
//! *pauses*, and the session has to re-take the terminal when it comes back. If
//! it were delivered anywhere but inside a wait, there would be an instant in
//! which the terminal is cooked and the code believes it is reading a raw one --
//! and nothing would ever say otherwise, because the notification is a flag and
//! the reader is parked. So it stays blocked from [`block_owned`] until
//! [`wait_for_input`], which lets it in with `pselect(2)` for the length of the
//! wait and no longer.
//!
//! The invariant the rest of this module exists to keep:
//!
//! > While the terminal is raw, `SIGTSTP` is either blocked or the process is
//! > inside [`wait_for_input`].
//!
//! Which makes the stop always land where a resume can be answered: the wait
//! returns `EINTR`, the caller re-enters raw mode, and only then waits again.

use std::io;
use std::marker::PhantomData;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::time::Duration;

use rustix::io::FdFlags;

use super::term;

static RESUMED: AtomicBool = AtomicBool::new(false);
static WINCH: AtomicBool = AtomicBool::new(false);
/// The write end, for the handlers. `-1` until [`install`] runs, and `-1` again
/// once [`release`] has run.
static POKE: AtomicI32 = AtomicI32::new(-1);

/// The signals whose default disposition would destroy something while the
/// terminal is raw: three that end the process and one that stops it, each
/// leaving a terminal nobody will cook again.
///
/// `SIGCONT` and `SIGWINCH` are deliberately absent. Their handlers only set a
/// flag, and their defaults -- resume, and ignore -- damage nothing, so there
/// is nothing to protect the transition from.
const OWNED_DEATHS: [libc::c_int; 4] = [libc::SIGINT, libc::SIGTERM, libc::SIGHUP, libc::SIGTSTP];

/// Every signal this module takes an owner's responsibility for, and hands back
/// in [`release`].
const OWNED: [libc::c_int; 6] = [
    libc::SIGINT,
    libc::SIGTERM,
    libc::SIGHUP,
    libc::SIGTSTP,
    libc::SIGCONT,
    libc::SIGWINCH,
];

/// Every handler, in one table, so that "the set is installed or it is not" is
/// a loop rather than six statements with six chances to grow a seventh that
/// forgets the failure path.
const HANDLERS: [(libc::c_int, extern "C" fn(libc::c_int)); 6] = [
    (libc::SIGINT, restore_and_reraise),
    (libc::SIGTERM, restore_and_reraise),
    (libc::SIGHUP, restore_and_reraise),
    (libc::SIGTSTP, stop_for_job_control),
    (libc::SIGCONT, flag_resumed),
    (libc::SIGWINCH, flag_winch),
];

/// The self-pipe a handler pokes so a parked `poll(2)` wakes now rather than at
/// the next tick.
pub(crate) struct Wakeup {
    read: OwnedFd,
    write: OwnedFd,
}

impl Wakeup {
    /// Both ends non-blocking and close-on-exec.
    ///
    /// `pipe2` is not portable (macOS has no such call), so the flags are set
    /// with `fcntl` afterwards. Non-blocking on the **write** end is the
    /// load-bearing one: a full pipe must make the handler drop the byte, not
    /// block the UI thread with the terminal raw. Close-on-exec keeps a spawned
    /// command from inheriting either end.
    pub(crate) fn new() -> io::Result<Self> {
        let (read, write) = rustix::pipe::pipe()?;
        for end in [read.as_fd(), write.as_fd()] {
            let flags = rustix::fs::fcntl_getfl(end)?;
            rustix::fs::fcntl_setfl(end, flags | rustix::fs::OFlags::NONBLOCK)?;
            rustix::io::fcntl_setfd(end, FdFlags::CLOEXEC)?;
        }
        Ok(Self { read, write })
    }

    /// The second descriptor the event loop waits on: a handler's poke wakes
    /// the wait now rather than at the next tick.
    pub(crate) fn read_fd(&self) -> BorrowedFd<'_> {
        self.read.as_fd()
    }

    /// Reads until `EAGAIN` and discards everything: the bytes are a wakeup,
    /// never data.
    ///
    /// It runs in the same turn of the loop that waits on the read end, and
    /// that pairing is not optional: a poked pipe nobody read stays readable,
    /// and the wait it is part of returns immediately, forever.
    pub(crate) fn drain(&self) {
        let mut scratch = [0u8; 64];
        while let Ok(read) = rustix::io::read(&self.read, &mut scratch) {
            if read == 0 {
                break;
            }
        }
    }
}

/// Proof that no owned death signal can be delivered right now.
///
/// It exists to be a *token*: [`block_owned`] is the only way to make one and
/// [`install`] is the only thing that takes one, so "every `sigaction` happens
/// while delivery is blocked" is a fact the compiler keeps rather than a
/// comment an edit can quietly falsify. Lifting the block is `Drop`'s job and
/// nothing else's, which is what makes the error paths safe: a `capture` or an
/// `enter_raw` that fails between the block and the installation drops the
/// token on the way out and leaves the mask as it was found.
pub(crate) struct Blocked {
    previous: libc::sigset_t,
    /// `Blocked` is **not** `Send`, and the mask is the reason: `pthread_sigmask`
    /// changes the calling *thread*'s mask, so a token carried to another thread
    /// would be proving something about a thread that never blocked anything.
    /// Keeping it on the thread that made it keeps the proof honest.
    _thread_bound: PhantomData<*const ()>,
}

/// Blocks the owned death signals on the calling thread, returning the proof.
///
/// `pthread_sigmask` rather than `sigprocmask`: POSIX leaves the latter's
/// behaviour unspecified in a process that has more than one thread, and this
/// binary has a Tokio runtime elsewhere in it. The UI thread is the one that
/// owns the terminal, and it is the one whose delivery has to stop.
pub(crate) fn block_owned() -> io::Result<Blocked> {
    // SAFETY: every argument is a valid, initialized `sigset_t` this function
    // owns for the whole call, and the signal numbers are the platform's own.
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        for signal in OWNED_DEATHS {
            libc::sigaddset(&mut set, signal);
        }
        let mut previous: libc::sigset_t = std::mem::zeroed();
        // `pthread_sigmask` reports failure as the errno itself, not as -1.
        let code = libc::pthread_sigmask(libc::SIG_BLOCK, &set, &mut previous);
        if code != 0 {
            return Err(io::Error::from_raw_os_error(code));
        }
        Ok(Blocked {
            previous,
            _thread_bound: PhantomData,
        })
    }
}

impl Blocked {
    /// Lifts the block on the signals whose default would *end* the process,
    /// and keeps holding the one whose default would *stop* it.
    ///
    /// A pending `SIGINT`, `SIGTERM`, or `SIGHUP` is delivered on the way out of
    /// here, into the handler that was just installed -- which is the point of
    /// having held it. `SIGTSTP` is not, because there is nowhere safe to take a
    /// stop outside a wait; it stays blocked until [`wait_for_input`] lets it in
    /// atomically.
    ///
    /// Returns the mask a wait runs under: the one the thread arrived with, in
    /// which the stop is deliverable. If the caller had already blocked the stop
    /// themselves it stays blocked there too -- their mask is theirs.
    fn lift_deaths_keeping_the_stop(self) -> io::Result<libc::sigset_t> {
        let previous = self.previous;
        // `Drop` must not run: it restores `previous` wholesale, which would let
        // the stop through *here* -- the window this whole design closes.
        std::mem::forget(self);
        // SAFETY: `waiting` is a copy of a set this thread's own `pthread_sigmask`
        // wrote, valid for the whole call.
        unsafe {
            let mut waiting = previous;
            libc::sigaddset(&mut waiting, libc::SIGTSTP);
            let code = libc::pthread_sigmask(libc::SIG_SETMASK, &waiting, std::ptr::null_mut());
            if code != 0 {
                // Deliberately no fallback: leaving the owned signals blocked
                // for the few instructions between here and the caller's restore
                // is strictly safer than unblocking them onto a raw terminal.
                return Err(io::Error::from_raw_os_error(code));
            }
        }
        Ok(previous)
    }
}

impl Drop for Blocked {
    /// Puts back the mask that was there before, rather than an empty one: what
    /// xfx blocked is xfx's to unblock, and a caller who had blocked something
    /// of their own must still have it blocked afterwards.
    ///
    /// This is the *abandon* path -- a transition that failed before the
    /// handlers were in place. The successful one goes through
    /// [`Blocked::lift_deaths_keeping_the_stop`], which does not run this.
    fn drop(&mut self) {
        // SAFETY: `previous` is the set this thread's own `pthread_sigmask`
        // wrote, and it is still owned by this value.
        unsafe {
            libc::pthread_sigmask(libc::SIG_SETMASK, &self.previous, std::ptr::null_mut());
        }
    }
}

/// What a completed installation hands the session.
///
/// Two things, because they are the same fact seen from two sides: whether the
/// session was already resumed before it began, and the proof that the stop
/// signal is still held. It is `!Send` for the same reason [`Blocked`] is -- the
/// mask it describes belongs to one thread.
#[must_use]
pub(crate) struct Held {
    /// The resume flag as it stood the moment the death signals were let
    /// through, which is the last instant anything could have changed it before
    /// the first wait.
    resumed: bool,
    /// The mask a wait runs under: the one the process arrived with, in which
    /// the stop signal is deliverable.
    during_wait: libc::sigset_t,
    _thread_bound: PhantomData<*const ()>,
}

impl Held {
    /// Whether raw mode has to be entered again before the session announces
    /// itself.
    ///
    /// The snapshot is **authoritative**, and the mask is why: between the
    /// moment it was taken and the first [`wait_for_input`], `SIGTSTP` cannot be
    /// delivered, so nothing can stop and resume this process in between and
    /// leave the answer stale. What it still catches is a bare `SIGCONT` --
    /// an operator's `kill -CONT` on a process that was never stopped -- whose
    /// handler sets the flag anyway; re-entering raw mode for one of those costs
    /// a mode set and is right for the same reason the real case is.
    pub(crate) fn stopped_before_the_session_began(&self) -> bool {
        self.resumed
    }
}

impl Drop for Held {
    /// Gives the stop signal back at the end of the session.
    ///
    /// The handlers are still installed here and the caller's restore has not
    /// run yet, so a `SIGTSTP` landing in this gap is *handled*: it cooks the
    /// terminal and stops, and the restore that follows on resume is idempotent.
    fn drop(&mut self) {
        // SAFETY: `during_wait` is a set this thread's own `pthread_sigmask`
        // wrote, and it is still owned by this value.
        unsafe {
            libc::pthread_sigmask(libc::SIG_SETMASK, &self.during_wait, std::ptr::null_mut());
        }
    }
}

/// Installs every handler the TUI owns, without `SA_RESTART`, and lifts the
/// block on everything except the stop.
///
/// The missing `SA_RESTART` is deliberate: a `pselect(2)` or `read(2)` parked
/// when a signal arrives must return `EINTR` so the UI thread gets a turn.
///
/// `blocked` is taken **by value** because the ordering is the point. It cannot
/// be produced except by [`block_owned`], and nothing is let through until the
/// last `sigaction` has returned, so a signal that arrived any time after the
/// terminal went raw is still pending here and is delivered into a handler the
/// instant this function lets it through.
///
/// On failure the terminal is given back **before** any mask is: a half-installed
/// set means some of these signals still have their default disposition, and
/// unblocking first would let one of them end the process on a terminal that is
/// still raw. The caller's restore runs afterwards as well, and a second restore
/// costs nothing -- but it must not be the first one.
pub(crate) fn install(wakeup: &Wakeup, blocked: Blocked) -> io::Result<Held> {
    POKE.store(wakeup.write.as_raw_fd(), Ordering::Release);
    if let Err(err) = install_handlers(&blocked) {
        term::restore_pair();
        // Explicit and after the restore, because the order is the fix.
        drop(blocked);
        return Err(err);
    }
    let during_wait = match blocked.lift_deaths_keeping_the_stop() {
        Ok(mask) => mask,
        Err(err) => {
            // Same ordering as above: the terminal goes back before a mask does.
            term::restore_pair();
            return Err(err);
        }
    };
    // The delivery point for everything except the stop: a `SIGTERM` held across
    // the transition ends the process on the line above. The flag is read after
    // it, and it cannot change again until the first wait -- see
    // `Held::stopped_before_the_session_began`.
    Ok(Held {
        resumed: take_resumed(),
        during_wait,
        _thread_bound: PhantomData,
    })
}

/// Waits until one of `fds` has something to read or `timeout` passes, letting
/// the stop signal in for the length of the wait and no longer.
///
/// `pselect(2)` rather than `poll(2)`, because the unmask and the wait have to
/// be **one** operation. With a plain `poll` the sequence would be unmask, then
/// wait, and a `SIGTSTP` arriving between the two would be delivered outside the
/// wait -- exactly the window this call exists to close. `pselect` installs the
/// mask, waits, and puts the old mask back, and the kernel does not let anything
/// in between. The event loop's fixed tick is expressed as this call's
/// `timeout` for that reason and no other: a `poll` with an 8 ms timeout would
/// be the same wait with the window back in it.
///
/// `pselect` and not `ppoll`, because macOS has no `ppoll(2)`. `pselect` is
/// POSIX, is a real syscall on both targets (Darwin links `pselect$1050`), and
/// is the portable spelling of the same atomicity.
///
/// `Ok(true)` means at least one descriptor is readable; `Ok(false)` means the
/// timeout passed with nothing to read. `ErrorKind::Interrupted` means a signal
/// was delivered *inside* the wait, which is where every stop and every resume
/// in this design happens. A `None` timeout waits without a deadline.
pub(crate) fn wait_for_input(
    fds: &[BorrowedFd<'_>],
    timeout: Option<Duration>,
    held: &Held,
) -> io::Result<bool> {
    let mut highest = -1;
    for fd in fds {
        let raw = fd.as_raw_fd();
        // `FD_SET` on a descriptor at or past `FD_SETSIZE` writes outside the
        // set. The TUI only ever waits on standard input and its own self-pipe,
        // but the check is what makes the `unsafe` below true for every caller
        // rather than for the current one.
        if raw < 0 || raw as usize >= libc::FD_SETSIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "the descriptor is out of range for pselect",
            ));
        }
        highest = highest.max(raw);
    }
    if highest < 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a wait on no descriptors would never end",
        ));
    }
    let deadline = timeout.map(|timeout| libc::timespec {
        tv_sec: libc::time_t::try_from(timeout.as_secs()).unwrap_or(libc::time_t::MAX),
        // `subsec_nanos` is under a billion by construction, so this is the
        // whole remainder rather than a clamp of it.
        tv_nsec: libc::c_long::from(timeout.subsec_nanos()),
    });
    // SAFETY: `readable` is zeroed before use and every descriptor in it was
    // just range-checked; the mask is a set this thread wrote; `deadline` is
    // owned here and lives across the call, and a null one means "no deadline";
    // the null write/error sets mean "not interested".
    let waited = unsafe {
        let mut readable: libc::fd_set = std::mem::zeroed();
        libc::FD_ZERO(&mut readable);
        for fd in fds {
            libc::FD_SET(fd.as_raw_fd(), &mut readable);
        }
        libc::pselect(
            highest + 1,
            &mut readable,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            deadline
                .as_ref()
                .map_or(std::ptr::null(), |deadline| deadline),
            &held.during_wait,
        )
    };
    if waited < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(waited > 0)
}

/// Every `sigaction` the TUI owns.
///
/// It takes the block by reference and never uses it: the parameter is the
/// requirement, spelled so that a future edit cannot install a handler outside
/// the window the mask protects without first removing an argument it would
/// have to explain.
fn install_handlers(_blocked: &Blocked) -> io::Result<()> {
    // SAFETY: every handler installed below is `extern "C"`, allocates nothing,
    // takes no lock, and calls only `write`, `signal`, `raise`, and an atomic
    // swap -- each of which POSIX lists as async-signal-safe.
    unsafe {
        for (signal, func) in HANDLERS {
            handler(signal, func)?;
        }
        // The UI owns one terminal; a failed write is handled by its return
        // value, not by dying.
        libc::signal(libc::SIGPIPE, libc::SIG_IGN);
    }
    Ok(())
}

/// Reinstalls the SIGTSTP handler after a resume.
///
/// Not optional: [`stop_for_job_control`] resets the disposition to default
/// before raising, so after one `SIGTSTP` a second would stop the process with
/// the terminal still raw. `sigaction` on the UI thread is unconstrained, which
/// is why this belongs here and not in the handler.
///
/// It needs no [`Blocked`] because it reopens no window: the terminal is cooked
/// from the moment the stop handler restored it until `resume` enters raw mode
/// again, and `resume` calls this **first**, so the default disposition and a
/// raw terminal are never true at the same time.
pub(crate) fn install_tstp() -> io::Result<()> {
    // SAFETY: as `install`; the handler is the same one, reinstated.
    unsafe { handler(libc::SIGTSTP, stop_for_job_control) }
}

/// Gives every owned signal back, once the terminal has been given back.
///
/// Two things end here. The dispositions: after `term::shutdown` there is
/// nothing left for a handler to restore, and a `SIGTERM` arriving during the
/// exit should end the process rather than write another restore sequence over
/// a terminal the user has back. And the poke: the handlers are process-wide
/// state, but the pipe they were given is a **session-local** descriptor, so a
/// `SIGCONT` or `SIGWINCH` arriving after the session ended would otherwise
/// write a byte into whatever the process had opened at that number since.
///
/// The order is the safety argument. Dispositions first, so no *new* handler
/// invocation can begin; then the poke, so no handler can read a stale
/// descriptor. A handler already executing cannot be in flight, because in this
/// phase every handler runs on the UI thread and the UI thread is the one
/// running this function -- a property a later phase that gives another thread
/// an unblocked signal would have to re-establish.
///
/// `SIGPIPE` is left alone on purpose: the Rust runtime sets it to `SIG_IGN`
/// before `main`, so "restoring the default" would hand back a process more
/// fragile than the one xfx was given.
pub(crate) fn release() {
    // SAFETY: `signal` with `SIG_DFL` on a platform signal number; a failure is
    // reported by return value and there is nothing to do about one here.
    unsafe {
        for signal in OWNED {
            libc::signal(signal, libc::SIG_DFL);
        }
    }
    POKE.store(-1, Ordering::Release);
}

/// # Safety
///
/// `func` must be async-signal-safe for every context `signal` can be delivered
/// in, because it is installed with an empty mask and no `SA_RESTART`.
unsafe fn handler(signal: libc::c_int, func: extern "C" fn(libc::c_int)) -> io::Result<()> {
    let mut action: libc::sigaction = std::mem::zeroed();
    action.sa_sigaction = func as usize;
    libc::sigemptyset(&mut action.sa_mask);
    action.sa_flags = 0;
    if libc::sigaction(signal, &action, std::ptr::null_mut()) != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

extern "C" fn restore_and_reraise(signal: libc::c_int) {
    term::restore_pair();
    // SAFETY: both calls are async-signal-safe, and the disposition being
    // restored is the default one, so the re-raise terminates this process with
    // the signal rather than re-entering this function.
    unsafe {
        libc::signal(signal, libc::SIG_DFL);
        libc::raise(signal);
    }
}

extern "C" fn stop_for_job_control(_signal: libc::c_int) {
    term::restore_pair();
    // SAFETY: as above. `SIGSTOP` is raised rather than `SIGTSTP` re-raised
    // because only an unblockable stop is a *genuine* stop; the disposition
    // reset is what keeps `install_tstp` honest work rather than ceremony.
    unsafe {
        libc::signal(libc::SIGTSTP, libc::SIG_DFL);
        libc::raise(libc::SIGSTOP);
    }
}

extern "C" fn flag_resumed(_signal: libc::c_int) {
    if !RESUMED.swap(true, Ordering::AcqRel) {
        poke();
    }
}

extern "C" fn flag_winch(_signal: libc::c_int) {
    if !WINCH.swap(true, Ordering::AcqRel) {
        poke();
    }
}

/// One byte, best effort. A full pipe means a wakeup is already queued, and the
/// *fact* of the signal lives in the atomic rather than in the byte.
fn poke() {
    let fd = POKE.load(Ordering::Acquire);
    if fd >= 0 {
        let byte = [1u8];
        // SAFETY: `write` is async-signal-safe; a short write and `EAGAIN` are
        // both ignored on purpose.
        unsafe {
            libc::write(fd, byte.as_ptr().cast(), 1);
        }
    }
}

pub(crate) fn take_resumed() -> bool {
    RESUMED.swap(false, Ordering::AcqRel)
}

/// Whether the window changed size since this was last asked.
///
/// Phase 1 does not re-layout: the event loop takes the flag and drops it, and
/// `docs/parity.md` says so. The one thing that does act on it is the *launch*
/// measurement, which cannot: it reads where the cursor is and how big the
/// screen is and pushes the shell's output above the band from both, so a
/// resize landing between the two readings would aim the push at a row that no
/// longer exists. Re-layout is Phase 2 item 12.
pub(crate) fn take_winch() -> bool {
    WINCH.swap(false, Ordering::AcqRel)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Mutex;

    /// Serializes the tests that install the real handler set.
    ///
    /// Masks are per-thread and need no lock, but dispositions and [`POKE`] are
    /// the *process's*, so two of these running at once would each see the
    /// other's teardown.
    static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

    /// Whether `signal` is blocked on the calling thread right now.
    fn masked(signal: libc::c_int) -> bool {
        // SAFETY: a `NULL` `set` makes this a pure query, and `current` is a
        // valid out-parameter for the whole call.
        unsafe {
            let mut current: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut current);
            assert_eq!(
                libc::pthread_sigmask(libc::SIG_BLOCK, std::ptr::null(), &mut current),
                0,
                "read this thread's signal mask"
            );
            libc::sigismember(&current, signal) == 1
        }
    }

    /// Whether `signal` has been raised and not yet delivered.
    fn pending(signal: libc::c_int) -> bool {
        // SAFETY: `set` is a valid out-parameter this function owns.
        unsafe {
            let mut set: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut set);
            assert_eq!(libc::sigpending(&mut set), 0, "read the pending set");
            libc::sigismember(&set, signal) == 1
        }
    }

    /// Takes a pending signal off this thread without running any disposition.
    ///
    /// Required rather than tidy: this is a test binary, whose `SIGTERM`
    /// disposition is the default one, so unblocking with a `SIGTERM` still
    /// pending would kill the runner.
    fn consume(signal: libc::c_int) {
        // SAFETY: `signal` is pending and blocked on this thread -- asserted by
        // the caller -- so `sigwait` returns immediately rather than parking.
        unsafe {
            let mut set: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut set);
            libc::sigaddset(&mut set, signal);
            let mut taken: libc::c_int = 0;
            assert_eq!(libc::sigwait(&set, &mut taken), 0, "sigwait");
            assert_eq!(taken, signal, "sigwait took the wrong signal");
        }
    }

    #[test]
    fn a_death_signal_that_arrives_while_the_terminal_changes_is_held_not_taken() {
        // The window this closes is the one between `tcsetattr` and the last
        // `sigaction`. There is no assertion that can be laundered into passing
        // here: if the block were not real, `raise` would run the default
        // disposition and the test *binary* would die by SIGTERM, which the
        // runner reports as a signal death rather than a failed assertion.
        let blocked = block_owned().expect("block the owned signals");
        // SAFETY: `raise` targets the calling thread, where SIGTERM is now
        // blocked, so it can reach neither a handler nor a default disposition.
        unsafe {
            libc::raise(libc::SIGTERM);
        }
        assert!(
            pending(libc::SIGTERM),
            "SIGTERM was neither taken nor held, so the block did nothing"
        );
        consume(libc::SIGTERM);
        drop(blocked);
        assert!(
            !masked(libc::SIGTERM),
            "the block outlived the token that proves it"
        );
    }

    #[test]
    fn every_signal_whose_default_would_strand_a_raw_terminal_is_in_the_block() {
        // Names the set, so removing one of them is a failing test rather than
        // a window nobody notices until a supervisor sends that one signal.
        let blocked = block_owned().expect("block the owned signals");
        for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP, libc::SIGTSTP] {
            assert!(
                masked(signal),
                "signal {signal} can still be delivered while the terminal changes"
            );
        }
        drop(blocked);
    }

    /// Whether `T` is `Send`, answered at compile time.
    ///
    /// Rust has no negative bounds, so the answer comes from the inherent
    /// constant applying only when the bound holds and the trait's default
    /// answering when it does not.
    struct IsSend<T>(std::marker::PhantomData<T>);
    trait Fallback {
        const SEND: bool = false;
    }
    impl<T> Fallback for IsSend<T> {}
    impl<T: Send> IsSend<T> {
        const SEND: bool = true;
    }

    #[test]
    fn the_proof_cannot_travel_to_a_thread_that_never_blocked_anything() {
        // `const` blocks, so this is decided when the crate is compiled: a
        // `Blocked` that became `Send` would not fail this test, it would fail
        // the build.
        const {
            assert!(
                !IsSend::<Blocked>::SEND,
                "`Blocked` is Send, so a per-thread mask could be proven by the wrong thread"
            );
        }
        const {
            assert!(
                !IsSend::<Held>::SEND,
                "`Held` is Send, so a wait could run under another thread's mask"
            );
        }
        // The control. Without it this would pass for a check that answers
        // `false` for every type, including one that really is `Send`.
        const {
            assert!(IsSend::<u8>::SEND, "the check itself is broken");
        }
    }

    /// Puts the process's dispositions and the poke back, so a test that
    /// installed the real handler set does not leave it on the runner.
    struct Installed;

    impl Drop for Installed {
        fn drop(&mut self) {
            release();
        }
    }

    #[test]
    fn a_session_stopped_before_it_read_a_byte_is_reported_by_the_installation() {
        // `install` reads the resume flag on the far side of letting the death
        // signals through, and `hold` branches on what it says: re-enter raw
        // mode, or announce. An answer of `false` for a session that was resumed
        // is a session announcing itself on a cooked terminal.
        let _serialized = ONE_AT_A_TIME
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let wakeup = Wakeup::new().expect("open the self-pipe");
        let installed = Installed;

        flag_resumed(libc::SIGCONT);
        let held =
            install(&wakeup, block_owned().expect("block")).expect("install the handler set");
        assert!(
            held.stopped_before_the_session_began(),
            "a resume that happened during the transition was not reported, \
             so the session would announce itself on a cooked terminal"
        );
        drop(held);

        // And the ordinary session, which must not re-enter raw mode it never
        // left: the flag is taken, not merely read, so it is false again here.
        let quiet = install(&wakeup, block_owned().expect("block")).expect("install again");
        assert!(
            !quiet.stopped_before_the_session_began(),
            "an undisturbed session was told to resume"
        );
        drop(quiet);

        drop(installed);
        assert_eq!(
            POKE.load(Ordering::Acquire),
            -1,
            "the handlers were left holding a descriptor the session is about to close"
        );
    }

    #[test]
    fn the_installation_lets_the_deaths_through_and_keeps_holding_the_stop() {
        let _serialized = ONE_AT_A_TIME
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // Fixture guard: a runner that arrived with these blocked would make
        // every assertion below vacuous.
        for signal in [libc::SIGTSTP, libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
            assert!(
                !masked(signal),
                "this thread arrived with signal {signal} blocked"
            );
        }

        let wakeup = Wakeup::new().expect("open the self-pipe");
        let installed = Installed;
        let held =
            install(&wakeup, block_owned().expect("block")).expect("install the handler set");

        assert!(
            masked(libc::SIGTSTP),
            "the stop was let out of the transition, so it can be delivered \
             somewhere other than inside the wait"
        );
        for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
            assert!(
                !masked(signal),
                "signal {signal} was left blocked, so a supervisor could not end xfx at all"
            );
        }

        drop(held);
        assert!(
            !masked(libc::SIGTSTP),
            "the session kept the stop after it ended"
        );
        drop(installed);
    }

    #[test]
    fn a_stop_pending_before_the_wait_is_delivered_inside_it() {
        // This is the window the whole design turns on, made reproducible: a
        // signal that is *already pending and blocked* when the wait begins is
        // indistinguishable, from the wait's point of view, from one that
        // arrives a nanosecond after the caller last looked at the flag. If
        // `wait_for_input` does not hand its mask to the kernel, such a signal
        // is never delivered at all and the session parks forever on a terminal
        // it no longer owns.
        //
        // `SIGWINCH` stands in for `SIGTSTP` because the mechanism under test is
        // the atomicity, not the disposition -- and a real `SIGTSTP` here would
        // stop the test binary with no thread left running to resume it
        // (`SIGSTOP` stops them all). The handler is the real one.
        let _serialized = ONE_AT_A_TIME
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let wakeup = Wakeup::new().expect("open the self-pipe");
        let installed = Installed;
        let held =
            install(&wakeup, block_owned().expect("block")).expect("install the handler set");

        // Blocked *after* `block_owned` recorded the mask, so the mask the wait
        // runs under is one in which it is deliverable -- exactly the shape the
        // stop has.
        let outside_the_wait = unsafe {
            let mut winch: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut winch);
            libc::sigaddset(&mut winch, libc::SIGWINCH);
            let mut before: libc::sigset_t = std::mem::zeroed();
            assert_eq!(
                libc::pthread_sigmask(libc::SIG_BLOCK, &winch, &mut before),
                0,
                "block SIGWINCH outside the wait"
            );
            before
        };
        take_winch();
        // SAFETY: `raise` targets this thread, where SIGWINCH is blocked.
        unsafe {
            libc::raise(libc::SIGWINCH);
        }
        assert!(pending(libc::SIGWINCH), "the signal is not pending");

        // A byte turns up shortly. It is the reason a broken wait *fails* here
        // instead of hanging: without the mask the signal never arrives, the
        // wait sits until the byte does, and the assertion below reports a
        // readable descriptor rather than an interruption.
        let (read, write) = rustix::pipe::pipe().expect("open a pipe");
        let writer = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(300));
            let _ = rustix::io::write(&write, b"x");
        });

        let outcome = wait_for_input(&[read.as_fd()], None, &held);

        writer.join().expect("the writer thread");
        // SAFETY: `outside_the_wait` is the mask this thread had before the test
        // blocked SIGWINCH on top of it.
        unsafe {
            libc::pthread_sigmask(libc::SIG_SETMASK, &outside_the_wait, std::ptr::null_mut());
        }
        take_winch();
        drop(held);
        drop(installed);

        let err = outcome.expect_err(
            "the wait returned readable, so the pending signal was never let in: \
             the mask was not handed to the kernel and the wait is not atomic",
        );
        assert_eq!(
            err.kind(),
            io::ErrorKind::Interrupted,
            "the wait failed for a reason that is not a delivered signal: {err}"
        );
    }

    #[test]
    fn lifting_the_block_puts_the_previous_mask_back_rather_than_an_empty_one() {
        // Something unrelated is blocked first, so "restored" cannot be
        // confused with "cleared".
        // SAFETY: valid sets, owned for the whole call.
        let entry = unsafe {
            let mut mine: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut mine);
            libc::sigaddset(&mut mine, libc::SIGUSR1);
            let mut entry: libc::sigset_t = std::mem::zeroed();
            assert_eq!(
                libc::pthread_sigmask(libc::SIG_BLOCK, &mine, &mut entry),
                0,
                "block SIGUSR1"
            );
            entry
        };

        {
            let _blocked = block_owned().expect("block the owned signals");
            assert!(masked(libc::SIGTERM), "the owned signal was not blocked");
            assert!(
                masked(libc::SIGUSR1),
                "the block replaced the caller's mask instead of adding to it"
            );
        }
        assert!(!masked(libc::SIGTERM), "the block was never lifted");
        assert!(
            masked(libc::SIGUSR1),
            "lifting the block cleared a mask xfx never set"
        );

        // SAFETY: `entry` is the mask this thread had before the test touched
        // it, and putting it back is what keeps the runner's other tests
        // running on the thread they were handed.
        unsafe {
            libc::pthread_sigmask(libc::SIG_SETMASK, &entry, std::ptr::null_mut());
        }
    }
}
