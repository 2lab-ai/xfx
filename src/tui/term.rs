//! The terminal xfx borrows, and the exact shape in which it gives it back.
//!
//! Two things live here because a signal handler needs them: the `termios` the
//! process captured **once, before raw mode**, and the compile-time-constant
//! restore strings. A handler allocates nothing, takes no lock, and touches
//! nothing else (`.prd/03-tui-port.md` §"Signals"; the constants are upstream's,
//! `vercel-labs/fx@ef1d0d0 src/core/app/app_lifecycle.zig:36-44`).
//!
//! The terminal is **two** descriptors here, not one. Raw mode is a property of
//! the descriptor input arrives on; the mode sequences are screen state on the
//! descriptor output leaves by. They are the same terminal in every ordinary
//! invocation and different ones in a redirected session, so they are tracked
//! apart -- a restore that went back to the wrong one would leave the input raw
//! and stamp the input terminal's attributes onto the output terminal.

use std::io;
use std::os::fd::{BorrowedFd, RawFd};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::thread::ThreadId;

use super::deliver::{Emit, RawTty, Sink};

use rustix::termios::{
    tcgetattr, tcgetwinsize, tcsetattr, ControlModes, InputModes, LocalModes, OptionalActions,
    SpecialCodeIndex, Termios, Winsize,
};

/// The modes the TUI turns on when it takes the terminal.
///
/// modifyOtherKeys, the kitty keyboard push, bracketed paste, autowrap off
/// (`terminal.zig:4-13`), the theme-change subscription, and
/// `XTWINOPS 22 ; 2` -- the push of the terminal's own **window title** onto its
/// title stack, so the one the band sets (`OSC 2`, `super::frame::title`) is
/// borrowed rather than taken. The window title only, rather than the icon name
/// with it, because that is the only one xfx sets: a push that claimed more than
/// the pop gives back is a stack entry left behind on every exit.
///
/// `?2031h` is the subscription upstream's live theme monitor runs on
/// (`theme_monitor.zig`): a terminal that has it reports `CSI ? 997 ; N n` when
/// its background changes, and [`super::input`] turns that into a repaint. It is
/// written under tmux as well as outside it, and that is a **request** rather
/// than a claim about what tmux does with it -- a terminal that does not
/// implement the mode ignores it, and the kitty push is left out of the tmux set
/// only because there is evidence it breaks key input there and none of any
/// breakage here. Every restore below turns it back off: a subscription this
/// session leaves running writes reports onto whatever runs next.
///
/// The push is the **last** thing in the mode set and the pop is the first
/// thing in every restore below, so the title a session sets exists only
/// between the two -- and a terminal that models no title stack ignores both
/// and keeps whatever its user gave it.
///
/// Mouse reporting is deliberately absent: the wheel stays the terminal's own
/// scrollback (`terminal.zig:135-142`).
pub(crate) const MODE_SET: &str = "\x1b[>4;2m\x1b[>1u\x1b[?2004h\x1b[?7l\x1b[?2031h\x1b[22;2t";

/// The same, without the kitty keyboard push, which breaks key input under
/// tmux (`terminal.zig:29-34`).
pub(crate) const MODE_SET_TMUX: &str = "\x1b[>4;2m\x1b[?2004h\x1b[?7l\x1b[?2031h\x1b[22;2t";

/// The normal exit's restore sequence, with **no** `1049l`: the main surface
/// was never on the alternate screen (`app_lifecycle.zig:39-41`).
pub(crate) const RESTORE: &str =
    "\x1b[23;2t\x1b[>4;0m\x1b[<u\x1b[?2004l\x1b[?2031l\x1b[?7h\x1b[?25h";

/// The same for tmux, which was never given the push to pop.
pub(crate) const RESTORE_TMUX: &str = "\x1b[23;2t\x1b[>4;0m\x1b[?2004l\x1b[?2031l\x1b[?7h\x1b[?25h";

/// The restore sequence for an exit that is *not* the planned one, which leads
/// with `1049l` defensively: a crash may have happened while a surface xfx does
/// not own was on screen (`app_lifecycle.zig:36-38`).
pub(crate) const ABNORMAL_RESTORE: &str =
    "\x1b[?1049l\x1b[23;2t\x1b[>4;0m\x1b[<u\x1b[?2004l\x1b[?2031l\x1b[?7h\x1b[?25h";

/// The abnormal restore for tmux.
pub(crate) const ABNORMAL_RESTORE_TMUX: &str =
    "\x1b[?1049l\x1b[23;2t\x1b[>4;0m\x1b[?2004l\x1b[?2031l\x1b[?7h\x1b[?25h";

/// The dimensions a terminal that will not answer is treated as having.
const DEFAULT_ROWS: u16 = 24;
const DEFAULT_COLS: u16 = 80;

/// What the process captured on the way in, and who is allowed to give it back.
struct Owned {
    /// The attributes [`capture`] read, and the ones every restore installs.
    termios: Termios,
    /// The descriptor raw mode was entered on, which is the only descriptor
    /// [`termios`](Self::termios) describes and therefore the only one it may
    /// be installed back onto.
    input: RawFd,
    /// The descriptor the mode sequences were written to, and the one an
    /// async-signal-safe restore writes its bytes back to.
    output: RawFd,
    ui_thread: ThreadId,
}

static OWNED: OnceLock<Owned> = OnceLock::new();
static TMUX: AtomicBool = AtomicBool::new(false);

pub(crate) fn under_tmux() -> bool {
    std::env::var_os("TMUX").is_some_and(|value| !value.is_empty())
}

pub(crate) fn capture(fd: BorrowedFd<'_>) -> io::Result<Termios> {
    tcgetattr(fd).map_err(io::Error::from)
}

/// Raw mode, as upstream defines it (`shell_runtime.zig:108-138`).
///
/// Not `cfmakeraw`: that clears `OPOST` as well, and output processing is not
/// xfx's to take -- the acceptance matrix names `c_lflag`, `c_iflag`, `CS8`,
/// and `VMIN`/`VTIME`, and nothing else.
pub(crate) fn raw_from(saved: &Termios) -> Termios {
    let mut raw = saved.clone();
    raw.input_modes.remove(
        InputModes::BRKINT
            | InputModes::ICRNL
            | InputModes::INPCK
            | InputModes::ISTRIP
            | InputModes::IXON
            | InputModes::IXOFF,
    );
    raw.control_modes.insert(ControlModes::CS8);
    raw.local_modes
        .remove(LocalModes::ECHO | LocalModes::ICANON | LocalModes::IEXTEN | LocalModes::ISIG);
    raw.special_codes[SpecialCodeIndex::VMIN] = 1;
    raw.special_codes[SpecialCodeIndex::VTIME] = 0;
    raw
}

pub(crate) fn enter_raw(fd: BorrowedFd<'_>, saved: &Termios) -> io::Result<()> {
    tcsetattr(fd, OptionalActions::Flush, &raw_from(saved)).map_err(io::Error::from)
}

/// Records what a handler will need, and who owns the terminal.
///
/// Write-once and never written again, so a handler can reach it without a lock
/// (`.prd/03-tui-port.md` §"Signals"). Called immediately **before** raw mode is
/// entered, not after: this is the moment the UI thread takes ownership for
/// panic purposes, and a panic hook armed against it has to already be in place
/// when the `tcsetattr` lands. `input` must be the descriptor `saved` was read
/// from and raw mode is about to be entered on; `output` is where the mode
/// sequences will go.
pub(crate) fn adopt(input: RawFd, output: RawFd, saved: Termios, tmux: bool) {
    TMUX.store(tmux, Ordering::Release);
    let _ = OWNED.set(Owned {
        termios: saved,
        input,
        output,
        ui_thread: std::thread::current().id(),
    });
}

/// The thread that took the terminal, for a panic hook that must tell whether
/// it is running on it.
///
/// `None` before [`adopt`], which is what makes a panic from a process that
/// never took the terminal restore nothing.
pub(crate) fn ui_thread() -> Option<ThreadId> {
    OWNED.get().map(|owned| owned.ui_thread)
}

/// Puts the captured attributes back on the descriptor they were captured from.
///
/// The single place the line discipline is restored, so "the restore targets
/// the input descriptor" is one fact in one function rather than a convention
/// each exit path has to remember.
fn restore_attrs(owned: &Owned) -> io::Result<()> {
    // SAFETY: the fd is the one this process captured at entry; `borrow_raw`
    // does not take ownership and the descriptor outlives the call.
    let fd = unsafe { BorrowedFd::borrow_raw(owned.input) };
    tcsetattr(fd, OptionalActions::Flush, &owned.termios).map_err(io::Error::from)
}

/// The restore pair, from a context that may allocate nothing.
///
/// Escape bytes restore *screen* state and go to the output descriptor; only
/// `tcsetattr` restores the line discipline, on the input descriptor, and POSIX
/// lists it among the async-signal-safe functions. Both return values are
/// ignored on purpose: there is no one left to report to.
// Called from the signal handlers and from the panic hook. It is written here
// because it is the half of the restore contract that reads what `adopt`
// recorded, and the two belong to one another.
pub(crate) fn restore_pair() {
    let Some(owned) = OWNED.get() else { return };
    let bytes = abnormal_restore(TMUX.load(Ordering::Acquire)).as_bytes();
    // SAFETY: `write` is async-signal-safe, the fd was recorded at entry and is
    // owned by the process for its whole life, and the buffer is 'static.
    unsafe {
        libc::write(owned.output, bytes.as_ptr().cast(), bytes.len());
    }
    let _ = restore_attrs(owned);
}

/// The bytes a panic and both death signals write, whichever plane was on the
/// screen.
///
/// One function rather than a branch at each of the three call sites, because
/// the property those sites share is the whole of what makes them correct: an
/// exit that does not know what is on the screen may not ask, so it leads with
/// `1049l` unconditionally. An approval screen that was up when the process died
/// is therefore given back by exactly the same bytes as a session that never
/// took one, and neither path has to consult a state a handler may not read.
pub(crate) fn abnormal_restore(tmux: bool) -> &'static str {
    if tmux {
        ABNORMAL_RESTORE_TMUX
    } else {
        ABNORMAL_RESTORE
    }
}

/// The normal exit, in upstream's order (`app_lifecycle.zig:578-593`): write the
/// restore sequence, `tcsetattr` the saved `termios`, then move to the band's
/// top and clear downward.
///
/// `screen` is the size of the screen `band_top` is a row **of**, taken from the
/// band's own shadow rather than from a fresh `TIOCGWINSZ`: the cleanup line's
/// coordinates were solved against the screen the band was painted on, and a
/// terminal that has since stopped describing itself -- a pty whose size was
/// unset answers `0x0` successfully -- would otherwise be answered with the
/// launch fallback and the exit measured against a screen nothing is on.
///
/// `band_top` is `None` for a session that drew no band, and then the last step
/// is **skipped entirely**: with no band the only row to clear from is the
/// screen's first, and `CUP(1,1)` + `ED` would erase a screen xfx never drew
/// on. The caller supplies the top of what its band actually painted
/// (`frame::Band::painted_top`), so a session that left before its first frame
/// still erases nothing.
///
/// Every step is attempted even when an earlier one failed, and the first error
/// is the one returned. A terminal left raw is worse than an unreported write
/// error, so there is no `?` between here and the end of the function. The one
/// exception is a restore segment the terminal took only **part** of, which
/// costs the cleanup segment and nothing else -- the `termios` still goes back.
/// See [`shutdown_with`].
/// `on_alternate` is the plane the terminal is still on, asked of the band
/// rather than of the session ([`super::frame::Band::on_alternate`]): what has
/// to be given back is what was really written, and a session that has released
/// the plane in its own state may still have those bytes on the screen. A
/// planned exit ordinarily finds this `false` -- the loop gives the plane back
/// in one frame the instant the question is answered -- and this is the
/// backstop for an exit that got out some other way. It is **conditional**,
/// unlike [`abnormal_restore`]: a `1049l` written by a session that never took
/// the alternate buffer swaps in, on a terminal that models one, a screen its
/// user was not looking at.
pub(crate) fn shutdown(
    band_top: Option<u16>,
    on_alternate: bool,
    screen: (u16, u16),
) -> io::Result<()> {
    let Some(owned) = OWNED.get() else {
        return Ok(());
    };
    // The same descriptor as `owned.output`, written the way every other byte
    // this session put on the terminal was written: one counted emit, no
    // buffer. The old locked stdout is gone from this path because a buffered
    // writer cannot say how much of a vector the terminal took -- and on the
    // exit, that number is what decides whether the cleanup below may be
    // written at all.
    let mut out = RawTty::stdout();
    shutdown_with(
        &mut out,
        owned,
        TMUX.load(Ordering::Acquire),
        band_top,
        on_alternate,
        screen,
    )
}

// A test's hand on the exit's first segment, between the moment it is built and
// the moment it is checked.
//
// Compiled into test builds only, and thread-local so that two tests running
// side by side cannot see each other's. It exists because "a segment the check
// refuses is not written, and the restore below it still runs" is a claim about
// a segment that gets refused -- and the one this function builds is two
// constants, which by construction never does.
#[cfg(test)]
thread_local! {
    static TAMPER: std::cell::Cell<Option<fn(&mut String)>> = const { std::cell::Cell::new(None) };
}

/// Runs `body` with `tamper` applied to the exit's restore segment.
#[cfg(test)]
fn tampering<T>(tamper: fn(&mut String), body: impl FnOnce() -> T) -> T {
    TAMPER.with(|hook| hook.set(Some(tamper)));
    let outcome = body();
    TAMPER.with(|hook| hook.set(None));
    outcome
}

/// The exit above, against an explicit screen and an explicit ownership record,
/// so that "the line discipline is restored even when the screen cannot be
/// written" is a test rather than a claim.
fn shutdown_with(
    out: &mut impl Sink,
    owned: &Owned,
    tmux: bool,
    band_top: Option<u16>,
    on_alternate: bool,
    screen_size: (u16, u16),
) -> io::Result<()> {
    let restore = if tmux { RESTORE_TMUX } else { RESTORE };
    // The plane first, and everything else after it. Every sequence in
    // `RESTORE` is about the surface the user is left looking at -- the title
    // popped, the autowrap put back, the cursor shown -- so one written while
    // the alternate buffer is still up is one restored for the wrong screen.
    let leave = if on_alternate { "\x1b[?1049l" } else { "" };
    // **Two segments across the `termios` boundary, and they may not be
    // merged.** The check sits inside each of them and changes neither the
    // order nor the all-attempt rule below: a refused segment is not written,
    // the line discipline is still put back, the later segment is still
    // attempted, and the first error is still the one returned. A refusal that
    // short-circuited any of that could leave a terminal raw, which is worse
    // than any screen it could save. The one outcome that does hold the later
    // segment back is a restore the terminal took only *part* of, and the
    // paragraph below is the whole of that exception.
    let first = format!("{leave}{restore}");
    #[cfg(test)]
    let first = {
        let mut first = first;
        if let Some(tamper) = TAMPER.with(std::cell::Cell::get) {
            tamper(&mut first);
        }
        first
    };
    let screen = check_restore(&first, tmux, on_alternate, screen_size)
        .map_err(Emit::rejected)
        .and_then(|()| out.emit(first.as_bytes()));
    let attrs = restore_attrs(owned);
    // **The one place the all-attempt rule bends, and only for one outcome.**
    // A segment the check refused, or one the descriptor took nothing of, left
    // the terminal exactly as it was: the cleanup below is as safe to write as
    // it ever was, and it is still attempted. A segment the terminal took a
    // *prefix* of is different in kind -- what reached the terminal may be
    // incomplete, and the cleanup is a `CUP` to a row and an erase from it.
    // Offered after an incomplete vector, those are bytes whose effect this
    // session cannot predict on a screen it no longer describes, so they are
    // not offered. The line discipline is put back
    // either way, because a terminal left raw is worse than any screen this
    // could have saved -- and none of that is a claim that the restore
    // *worked*: what a terminal holding a prefix is showing is not knowable
    // from here.
    let took_a_prefix = matches!(screen, Err(Emit::Partial { .. }));
    let cleanup = match band_top {
        // Leaves the transcript in scrollback and the cursor on a clean line.
        Some(top) if !took_a_prefix => {
            let line = format!("\x1b[{top};1H\x1b[J\x1b[?25h\n");
            check_cleanup(&line, top, screen_size)
                .map_err(Emit::rejected)
                .and_then(|()| out.emit(line.as_bytes()))
                .map_err(Emit::into_error)
        }
        _ => Ok(()),
    };
    screen.map_err(Emit::into_error).and(attrs).and(cleanup)
}

/// The restore segment against what it says it gives back: the modes, the title
/// stack, the plane, and a cursor the user can see.
fn check_restore(
    bytes: &str,
    tmux: bool,
    on_alternate: bool,
    (rows, columns): (u16, u16),
) -> io::Result<()> {
    let plane = if on_alternate {
        super::check::PlaneKind::Alternate
    } else {
        super::check::PlaneKind::Primary
    };
    super::check::preflight(
        &super::check::TerminalModel::seed_modes(
            rows,
            columns,
            super::check::ModeSet::announced(tmux),
            1,
            plane,
        ),
        bytes.as_bytes(),
        &super::check::Declared::new(
            super::check::Intent::Modes {
                modes: super::check::ModeSet::restored(tmux),
                // **One pop, for the one push this vector is seeded against.**
                // The seed below is a depth of one because that is what this
                // segment is written to answer, so what is proved here is local
                // and is exactly that: *this* vector pops once, and a restore
                // that lost its `23;2t` is refused.
                //
                // It is **not** a claim about the stack's depth across a
                // session. Unit A holds nothing between vectors by design, and
                // the mode set is re-announced on every `SIGCONT`
                // (`mod.rs`'s `resume`) -- each of which pushes again. A
                // stop/continue pair is balanced, because the stop handler's
                // own `ABNORMAL_RESTORE` carries a `23;2t` of its own; a
                // `SIGCONT` with no stop in front of it is a separate lead for
                // whoever takes session lifecycle on, and nothing here
                // establishes it either way.
                title_stack: 0,
                plane: super::check::PlaneKind::Primary,
                cursor_visible: Some(true),
            },
            // The `1049l` in front of the restore is the one transition this
            // segment may make, and only while the band is still on the
            // borrowed buffer: `RESTORE` carries none of its own, and a leave
            // written by a session that never took a plane swaps in, on a
            // terminal that models two, a screen its user was not looking at.
            super::check::Footprint::none(super::check::PlaneKind::Primary).moving(
                if on_alternate {
                    super::check::PlaneMove::Give
                } else {
                    super::check::PlaneMove::Stay
                },
            ),
        ),
    )?;
    Ok(())
}

/// The cleanup segment against the rows it may erase and the row it may leave
/// the caret on.
///
/// The erase runs from the band's top row to the screen's last and **no
/// higher**: everything above is the terminal's own document, and an `ED 2`
/// here would take the user's session with it. The trailing linefeed scrolls
/// exactly one row when the band began on the last row of the screen and none
/// otherwise -- not "may scroll": a row that leaves the top of the screen is in
/// native scrollback for good.
fn check_cleanup(bytes: &str, top: u16, (rows, columns): (u16, u16)) -> io::Result<()> {
    let scrolled = u32::from(top >= rows);
    super::check::preflight(
        &super::check::TerminalModel::seed_foreign(rows, columns, None),
        bytes.as_bytes(),
        &super::check::Declared::new(
            super::check::Intent::Cleanup {
                top,
                caret: (top.saturating_add(1).min(rows), 1),
                cursor_visible: true,
            },
            super::check::Footprint::new(
                super::check::PlaneKind::Primary,
                vec![
                    super::check::Seg::Erase(top..=rows),
                    super::check::Seg::Scroll { rows: scrolled },
                ],
            ),
        ),
    )?;
    Ok(())
}

/// The terminal's dimensions, or 24x80 when it will not say. **The reading a
/// launch takes**; a running session takes [`reported_window_size`] instead,
/// which keeps a refusal a refusal. A terminal query, so it lives here;
/// `layout::solve` takes rows and columns as arguments and stays pure, which
/// is what makes its unit tests possible.
///
/// **Asked of standard output, and that is the ruling rather than the
/// accident.** This module keeps the two descriptors apart because a redirected
/// session can put them on different terminals, and each fact then belongs to
/// whichever descriptor it is a fact *about*:
///
/// * The line discipline is a property of the descriptor input arrives on, so
///   `termios` is captured from, and restored onto, standard input.
/// * Screen state -- the mode set, the band, the restore -- is a property of
///   the descriptor output leaves by, so those bytes go to standard output.
/// * A band's geometry is screen state. It is the *output* terminal the band
///   has to fit inside, so its size is asked of standard output. Asking
///   standard input would size the band to a screen it will never be drawn on,
///   which is why "the raw-mode descriptor" is not automatically the right
///   answer here.
///
/// The launch cursor probe is the one query that spans both -- it writes `CSI
/// 6n` to standard output and reads the answer off standard input -- and it can
/// only be answered when the two are the same terminal. When they are not, the
/// query goes to one device and nothing arrives from the other, the probe's
/// deadline passes, and the session starts at row 1: it pushes nothing and
/// paints over nothing. That is the correct degradation and it needs no
/// detection, which is why this phase does not try to tell the two cases apart.
pub(crate) fn window_size() -> (u16, u16) {
    size_or_default(tcgetwinsize(io::stdout()))
}

/// The terminal's dimensions as it reports them, or `(0, 0)` when it reports
/// nothing usable. **The reading a running session takes**, and the one
/// [`window_size`] must not be used for.
///
/// The same ioctl on the same descriptor, and a different question, because the
/// caller is in a different position. A **launch** has no band and has to solve
/// one from some number, so a terminal that will not say its size is answered
/// with 24x80: a guess is the only thing that lets a session start at all. A
/// **running** session already has a band on a screen it measured, so the same
/// refusal is not a screen to move to -- it is a reading to ignore. Answering
/// it with the startup fallback would take a 40x132 session, whose terminal
/// declined to answer a single `TIOCGWINSZ` (or which is on a pty whose size
/// was never set, which answers `0x0` *successfully*), and move its band onto
/// rows 22 to 24 of a screen that is nothing of the kind.
///
/// `(0, 0)` rather than an `Option` because that is what the one caller means
/// by it and what its own contract already refuses: `super::shell::Shell::resize`
/// answers a zero in either dimension with `Resize::Unchanged`, so "the
/// terminal said nothing" and "the terminal said nothing new" arrive at the
/// same place by the same door.
pub(crate) fn reported_window_size() -> (u16, u16) {
    size_as_reported(tcgetwinsize(io::stdout()))
}

/// The dimensions a `TIOCGWINSZ` answer means **at launch**.
///
/// A zero is treated exactly like a refusal: a pty whose size was never set
/// answers `0x0` successfully, and a layout solved against zero rows would
/// place the band outside the screen rather than decline to draw it.
fn size_or_default(size: Result<Winsize, rustix::io::Errno>) -> (u16, u16) {
    match size {
        Ok(size) if size.ws_row > 0 && size.ws_col > 0 => (size.ws_row, size.ws_col),
        _ => (DEFAULT_ROWS, DEFAULT_COLS),
    }
}

/// The same answer read **after** launch: the size, or nothing at all.
///
/// A zero and a refusal are one fact here too, and it is the opposite fact:
/// neither says anything about the screen the band is already on, so neither
/// may move it. See [`reported_window_size`].
fn size_as_reported(size: Result<Winsize, rustix::io::Errno>) -> (u16, u16) {
    match size {
        Ok(size) if size.ws_row > 0 && size.ws_col > 0 => (size.ws_row, size.ws_col),
        _ => (0, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::os::fd::{AsFd, AsRawFd, OwnedFd};

    use super::super::deliver::{emit_counted, RawWrite};

    use rustix::termios::OutputModes;

    /// Every terminal word a restore is judged on. `Termios` is not `PartialEq`
    /// and its private speed fields make a whole-struct comparison impossible,
    /// so the comparison is the words upstream's matrix names.
    type Words = (InputModes, OutputModes, ControlModes, LocalModes, u8, u8);

    fn words(termios: &Termios) -> Words {
        (
            termios.input_modes,
            termios.output_modes,
            termios.control_modes,
            termios.local_modes,
            termios.special_codes[SpecialCodeIndex::VMIN],
            termios.special_codes[SpecialCodeIndex::VTIME],
        )
    }

    /// The words a terminal has *right now*.
    fn live(fd: BorrowedFd<'_>) -> Words {
        words(&tcgetattr(fd).expect("read the terminal"))
    }

    /// A pty pair. Both ends are returned because the line discipline is reset
    /// when the last descriptor on either side closes, so a test that asserts
    /// on `termios` has to keep them alive for its whole body.
    fn open_pty() -> (OwnedFd, OwnedFd) {
        let master =
            rustix::pty::openpt(rustix::pty::OpenptFlags::RDWR).expect("open a pty master");
        rustix::pty::grantpt(&master).expect("grant the pty slave");
        rustix::pty::unlockpt(&master).expect("unlock the pty slave");
        let name = rustix::pty::ptsname(&master, Vec::new()).expect("name the pty slave");
        // `O_NOCTTY`: reading a terminal's settings must not make it this
        // process's controlling terminal.
        let slave = rustix::fs::open(
            name.to_str().expect("the slave name is utf-8"),
            rustix::fs::OFlags::RDWR | rustix::fs::OFlags::NOCTTY,
            rustix::fs::Mode::empty(),
        )
        .expect("open the pty slave");
        (master, slave)
    }

    /// The ownership record the process builds at entry, over descriptors a
    /// test owns instead of over the process's own standard streams.
    fn owned_over(input: &OwnedFd, output: &OwnedFd, termios: Termios) -> Owned {
        Owned {
            termios,
            input: input.as_raw_fd(),
            output: output.as_raw_fd(),
            ui_thread: std::thread::current().id(),
        }
    }

    /// A cooked terminal, as the kernel hands one out.
    ///
    /// Read from a real pty rather than assembled in place: `Termios` keeps the
    /// line speeds in private fields and implements no `Default`, so the only
    /// way to hold one is to ask a terminal for it. The modes raw mode must
    /// clear are then set explicitly, because the system defaults leave
    /// `ISTRIP`, `INPCK`, and `IXOFF` clear already and an assertion that raw
    /// mode cleared a bit nobody had set proves nothing.
    fn cooked() -> Termios {
        let (_master, slave) = open_pty();
        let mut termios = tcgetattr(&slave).expect("read the pty's termios");
        termios.input_modes.insert(
            InputModes::BRKINT
                | InputModes::ICRNL
                | InputModes::INPCK
                | InputModes::ISTRIP
                | InputModes::IXON
                | InputModes::IXOFF,
        );
        termios
            .local_modes
            .insert(LocalModes::ECHO | LocalModes::ICANON | LocalModes::IEXTEN | LocalModes::ISIG);
        termios.special_codes[SpecialCodeIndex::VMIN] = 0;
        termios.special_codes[SpecialCodeIndex::VTIME] = 4;
        termios
    }

    /// A screen that has gone away, for the exit path that must not depend on
    /// one.
    struct BrokenScreen;

    impl RawWrite for BrokenScreen {
        fn write_once(&mut self, _bytes: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "the screen went away",
            ))
        }
    }

    impl Sink for BrokenScreen {
        fn emit(&mut self, bytes: &[u8]) -> Result<(), Emit> {
            emit_counted(self, bytes)
        }
    }

    #[test]
    fn the_fixture_is_a_terminal_raw_mode_would_have_work_to_do() {
        // Guards every test below that starts from `cooked()`: a fixture that
        // was already raw would let them pass while proving nothing. Every bit
        // `raw_from` is required to clear is required to be set here first.
        let cooked = cooked();
        for mode in [
            InputModes::BRKINT,
            InputModes::ICRNL,
            InputModes::INPCK,
            InputModes::ISTRIP,
            InputModes::IXON,
            InputModes::IXOFF,
        ] {
            assert!(
                cooked.input_modes.contains(mode),
                "the fixture never set input mode {mode:?}"
            );
        }
        for mode in [
            LocalModes::ECHO,
            LocalModes::ICANON,
            LocalModes::IEXTEN,
            LocalModes::ISIG,
        ] {
            assert!(
                cooked.local_modes.contains(mode),
                "the fixture never set local mode {mode:?}"
            );
        }
        assert_eq!(cooked.special_codes[SpecialCodeIndex::VMIN], 0, "VMIN");
        assert_eq!(cooked.special_codes[SpecialCodeIndex::VTIME], 4, "VTIME");
    }

    #[test]
    fn raw_mode_clears_exactly_the_bits_upstream_clears() {
        let raw = raw_from(&cooked());
        for mode in [
            InputModes::BRKINT,
            InputModes::ICRNL,
            InputModes::INPCK,
            InputModes::ISTRIP,
            InputModes::IXON,
            InputModes::IXOFF,
        ] {
            assert!(
                !raw.input_modes.contains(mode),
                "input mode {mode:?} survived"
            );
        }
        for mode in [
            LocalModes::ECHO,
            LocalModes::ICANON,
            LocalModes::IEXTEN,
            LocalModes::ISIG,
        ] {
            assert!(
                !raw.local_modes.contains(mode),
                "local mode {mode:?} survived"
            );
        }
        assert!(
            raw.control_modes.contains(ControlModes::CS8),
            "CS8 is not set"
        );
        assert_eq!(raw.special_codes[SpecialCodeIndex::VMIN], 1);
        assert_eq!(raw.special_codes[SpecialCodeIndex::VTIME], 0);
    }

    #[test]
    fn raw_mode_leaves_output_processing_alone() {
        // `cfmakeraw` would clear OPOST too. Upstream's acceptance matrix names
        // c_lflag, c_iflag, CS8 and VMIN/VTIME and nothing else, and the band
        // writer positions every row with CUP, so there is no reason to take a
        // fourth word away from the terminal.
        let cooked = cooked();
        assert_eq!(raw_from(&cooked).output_modes, cooked.output_modes);
    }

    #[test]
    fn the_mode_sets_and_restores_are_exactly_these_bytes_in_exactly_this_order() {
        // Spelled out independently of the declarations, so this pins the whole
        // sequence -- every escape, and the order they arrive in -- rather than
        // comparing a constant with itself.
        assert_eq!(
            MODE_SET,
            "\u{1b}[>4;2m\u{1b}[>1u\u{1b}[?2004h\u{1b}[?7l\u{1b}[?2031h\u{1b}[22;2t"
        );
        assert_eq!(
            MODE_SET_TMUX,
            "\u{1b}[>4;2m\u{1b}[?2004h\u{1b}[?7l\u{1b}[?2031h\u{1b}[22;2t"
        );
        assert_eq!(
            RESTORE,
            "\u{1b}[23;2t\u{1b}[>4;0m\u{1b}[<u\u{1b}[?2004l\u{1b}[?2031l\u{1b}[?7h\u{1b}[?25h"
        );
        assert_eq!(
            RESTORE_TMUX,
            "\u{1b}[23;2t\u{1b}[>4;0m\u{1b}[?2004l\u{1b}[?2031l\u{1b}[?7h\u{1b}[?25h"
        );
        assert_eq!(
            ABNORMAL_RESTORE,
            "\u{1b}[?1049l\u{1b}[23;2t\u{1b}[>4;0m\u{1b}[<u\u{1b}[?2004l\u{1b}[?2031l\
             \u{1b}[?7h\u{1b}[?25h"
        );
        assert_eq!(
            ABNORMAL_RESTORE_TMUX,
            "\u{1b}[?1049l\u{1b}[23;2t\u{1b}[>4;0m\u{1b}[?2004l\u{1b}[?2031l\u{1b}[?7h\u{1b}[?25h"
        );
    }

    /// `DECSET`/`DECRST 2031`, spelled here rather than imported for the reason
    /// [`PUSH_TITLE`] is.
    const SUBSCRIBE: &str = "\u{1b}[?2031h";
    const UNSUBSCRIBE: &str = "\u{1b}[?2031l";

    #[test]
    fn every_session_subscribes_to_theme_changes_once_and_unsubscribes_once() {
        // Mode 2031 is a *subscription*, which is what makes both halves of
        // this a bug rather than a preference. A session that never set it
        // hears nothing when its user switches their system theme and paints
        // the rest of the session in the wrong greys; a session that left it
        // set hands the next program a terminal that keeps sending
        // `CSI ? 997 ; N n` reports nobody asked for -- into a shell, that is
        // text on the user's prompt.
        //
        // **Under tmux too.** The kitty push is left out there because it
        // breaks key input (`tmux_never_gets_the_kitty_push_or_the_pop`); this
        // one has no such evidence against it, and a mode a terminal does not
        // implement is a mode it ignores. Asking and being ignored costs the
        // eight bytes; not asking costs the feature.
        for set in [MODE_SET, MODE_SET_TMUX] {
            assert_eq!(set.matches(SUBSCRIBE).count(), 1, "{set:?}");
            assert!(
                !set.contains(UNSUBSCRIBE),
                "a mode set unsubscribed: {set:?}"
            );
        }
        for restore in [
            RESTORE,
            RESTORE_TMUX,
            ABNORMAL_RESTORE,
            ABNORMAL_RESTORE_TMUX,
        ] {
            assert_eq!(restore.matches(UNSUBSCRIBE).count(), 1, "{restore:?}");
            assert!(
                !restore.contains(SUBSCRIBE),
                "a restore subscribed: {restore:?}"
            );
        }
    }

    #[test]
    fn the_subscription_is_taken_and_given_back_inside_the_title_the_session_borrows() {
        // The two orderings this module already keeps, asserted against the new
        // mode rather than re-asserted about the old ones: the title push is
        // the **last** thing a mode set does and the pop is the **first** thing
        // a restore does, so anything paired has to sit inside that pair. A
        // `?2031h` written after the push, or a `?2031l` written before the
        // pop, would be a mode moved while the title stack was in a state this
        // session is in the middle of changing.
        for set in [MODE_SET, MODE_SET_TMUX] {
            assert!(
                set.find(SUBSCRIBE) < set.find(PUSH_TITLE),
                "the subscription came after the title push: {set:?}"
            );
        }
        for restore in [
            RESTORE,
            RESTORE_TMUX,
            ABNORMAL_RESTORE,
            ABNORMAL_RESTORE_TMUX,
        ] {
            assert!(
                restore.find(POP_TITLE) < restore.find(UNSUBSCRIBE),
                "the unsubscribe came before the title pop: {restore:?}"
            );
        }
    }

    /// `XTWINOPS 22 ; 2` and `23 ; 2`, spelled here rather than imported for the
    /// reason every needle in this module's tests is: a test that read the
    /// constant it is checking would pass for whatever the module declared.
    const PUSH_TITLE: &str = "\u{1b}[22;2t";
    const POP_TITLE: &str = "\u{1b}[23;2t";

    #[test]
    fn every_session_pushes_the_terminals_title_once_and_pops_it_once() {
        // The title is the user's, borrowed. A push without a pop leaves `xfx`
        // on the window for the rest of that terminal's life; a pop without a
        // push takes away a title xfx never set, and a stack entry that belongs
        // to whatever ran before it.
        for set in [MODE_SET, MODE_SET_TMUX] {
            assert_eq!(set.matches(PUSH_TITLE).count(), 1, "{set:?}");
            assert!(
                !set.contains(POP_TITLE),
                "a mode set popped a title: {set:?}"
            );
        }
        for restore in [
            RESTORE,
            RESTORE_TMUX,
            ABNORMAL_RESTORE,
            ABNORMAL_RESTORE_TMUX,
        ] {
            assert_eq!(restore.matches(POP_TITLE).count(), 1, "{restore:?}");
            assert!(
                !restore.contains(PUSH_TITLE),
                "a restore pushed a title: {restore:?}"
            );
        }
    }

    #[test]
    fn the_title_is_given_back_before_anything_else_a_restore_does() {
        // Ordering, because one of these restores is written from a signal
        // handler onto a terminal that may be about to lose its process: the
        // sooner the user's own title is back, the smaller the window in which
        // a second failure leaves it as xfx's.
        for restore in [RESTORE, RESTORE_TMUX] {
            assert!(restore.starts_with(POP_TITLE), "{restore:?}");
        }
        for restore in [ABNORMAL_RESTORE, ABNORMAL_RESTORE_TMUX] {
            // Behind the defensive `1049l` and nothing else: a title popped on
            // the alternate screen would be popped for the wrong surface.
            assert!(
                restore.starts_with(&format!("\u{1b}[?1049l{POP_TITLE}")),
                "{restore:?}"
            );
        }
    }

    #[test]
    fn the_normal_restore_never_leaves_an_alternate_screen_that_was_never_entered() {
        assert!(!RESTORE.contains("1049"), "normal restore: {RESTORE:?}");
        assert!(
            !RESTORE_TMUX.contains("1049"),
            "normal restore: {RESTORE_TMUX:?}"
        );
        assert!(
            ABNORMAL_RESTORE.contains("\u{1b}[?1049l"),
            "abnormal restore drops its guard"
        );
        assert!(
            ABNORMAL_RESTORE_TMUX.contains("\u{1b}[?1049l"),
            "abnormal restore drops its guard"
        );
    }

    #[test]
    fn tmux_never_gets_the_kitty_push_or_the_pop() {
        assert!(MODE_SET.contains("\u{1b}[>1u"));
        assert!(!MODE_SET_TMUX.contains("\u{1b}[>1u"));
        assert!(!RESTORE_TMUX.contains("\u{1b}[<u"));
        assert!(!ABNORMAL_RESTORE_TMUX.contains("\u{1b}[<u"));
    }

    #[test]
    fn the_mode_set_enables_no_mouse_reporting_on_the_main_surface() {
        // The negative upstream pins in `terminal.zig:135-142`: the wheel must
        // stay the terminal's own scrollback.
        for mouse in ["1000h", "1002h", "1003h", "1006h"] {
            assert!(!MODE_SET.contains(mouse), "{mouse} is in {MODE_SET:?}");
            assert!(
                !MODE_SET_TMUX.contains(mouse),
                "{mouse} is in {MODE_SET_TMUX:?}"
            );
        }
    }

    #[test]
    fn the_restore_puts_the_attributes_back_on_the_descriptor_they_came_from() {
        let (_input_master, input) = open_pty();
        let (_output_master, output) = open_pty();

        // The two terminals are left in different states, so a restore aimed at
        // the wrong descriptor is visible from both sides: the input would stay
        // raw, and the output would acquire the input's attributes.
        let mut changed = tcgetattr(&output).expect("read the output terminal");
        changed.local_modes.remove(LocalModes::ECHO);
        tcsetattr(&output, OptionalActions::Flush, &changed).expect("set the output terminal");
        let output_before = live(output.as_fd());

        let saved = tcgetattr(&input).expect("read the input terminal");
        assert!(
            saved.local_modes.contains(LocalModes::ECHO),
            "the input terminal was not cooked to begin with"
        );
        enter_raw(input.as_fd(), &saved).expect("enter raw mode");
        assert_ne!(
            live(input.as_fd()),
            words(&saved),
            "raw mode changed nothing, so the restore proves nothing"
        );

        restore_attrs(&owned_over(&input, &output, saved.clone())).expect("restore");

        assert_eq!(
            live(input.as_fd()),
            words(&saved),
            "the descriptor the attributes came from was not restored"
        );
        assert_eq!(
            live(output.as_fd()),
            output_before,
            "the restore was stamped onto the output terminal"
        );
    }

    #[test]
    fn a_screen_that_cannot_be_written_still_gets_its_line_discipline_back() {
        let (_master, input) = open_pty();
        let saved = tcgetattr(&input).expect("read the terminal");
        enter_raw(input.as_fd(), &saved).expect("enter raw mode");

        let owned = owned_over(&input, &input, saved.clone());
        let err = shutdown_with(&mut BrokenScreen, &owned, false, Some(21), false, (24, 80))
            .expect_err("a screen that refuses every write must be reported");

        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe, "{err}");
        assert_eq!(
            live(input.as_fd()),
            words(&saved),
            "the terminal was left raw because the screen failed first"
        );
    }

    /// A screen that takes `prefix` bytes of what it is offered, fails once,
    /// and takes everything after that.
    ///
    /// The failure is deliberately not permanent: a screen that refused
    /// everything afterwards could not tell "the exit skipped the cleanup" from
    /// "the exit offered the cleanup and the screen refused it". Here anything
    /// offered after the failure **lands**, so bytes past the prefix are proof
    /// that an offer was made.
    struct HalfDeaf {
        prefix: usize,
        failed: bool,
        written: Vec<u8>,
    }

    impl HalfDeaf {
        fn taking(prefix: usize) -> Self {
            Self {
                prefix,
                failed: false,
                written: Vec::new(),
            }
        }
    }

    impl RawWrite for HalfDeaf {
        fn write_once(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.failed || bytes.is_empty() {
                self.written.extend_from_slice(bytes);
                return Ok(bytes.len());
            }
            if self.prefix > 0 {
                let taken = self.prefix.min(bytes.len());
                self.prefix -= taken;
                self.written.extend_from_slice(&bytes[..taken]);
                return Ok(taken);
            }
            self.failed = true;
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "the screen stopped taking bytes",
            ))
        }
    }

    impl Sink for HalfDeaf {
        fn emit(&mut self, bytes: &[u8]) -> Result<(), Emit> {
            emit_counted(self, bytes)
        }
    }

    #[test]
    fn a_partial_frame_earlier_in_the_session_does_not_hold_back_the_exits_cleanup() {
        // The distinction the exception below is **only** about, stated as the
        // pair it has to be told apart from. A frame that ended in a prefix is
        // why the session is exiting; it is not a reason to withhold the exit's
        // own second segment, because the restore in front of that segment went
        // out whole and the terminal took it. The skip belongs to one case --
        // the exit's *first* segment taken in part -- and to no other.
        //
        // Driven on one screen in one order, because that is what makes it a
        // distinction rather than two unrelated cases: the same sink takes a
        // prefix of a frame, fails, and is then handed the exit.
        let (_master, input) = open_pty();
        let saved = tcgetattr(&input).expect("read the terminal");
        enter_raw(input.as_fd(), &saved).expect("enter raw mode");
        let owned = owned_over(&input, &input, saved.clone());

        let mut screen = HalfDeaf::taking(4);
        let earlier = screen
            .emit(b"\x1b[22;1Ha frame the screen stopped taking")
            .expect_err("the screen took the whole of a frame it was meant to stop taking");
        assert!(
            matches!(earlier, Emit::Partial { delivered: 4, .. }),
            "the earlier frame was not a prefix, so this case proves nothing: {earlier:?}"
        );
        screen.written.clear();

        shutdown_with(&mut screen, &owned, false, Some(21), false, (24, 80))
            .expect("an exit onto a screen that is taking bytes again");

        assert_eq!(
            String::from_utf8_lossy(&screen.written),
            format!("{RESTORE}\u{1b}[21;1H\u{1b}[J\u{1b}[?25h\n"),
            "the exit withheld a segment because of a failure that was not its own"
        );
        assert_eq!(
            live(input.as_fd()),
            words(&saved),
            "the exit did not put the line discipline back"
        );
    }

    #[test]
    fn an_exit_whose_restore_lands_in_part_gives_the_line_discipline_back_and_writes_no_cleanup() {
        // The one place the all-attempt rule bends, and only here. What reached
        // the terminal may be incomplete, so the cleanup line's `CUP` would be
        // read by a terminal this session can no longer describe, and its erase
        // addresses rows on the strength of that reading. The line discipline is
        // still put back, because a terminal left raw is worse than any screen
        // this could have saved.
        let (_master, input) = open_pty();
        let saved = tcgetattr(&input).expect("read the terminal");
        enter_raw(input.as_fd(), &saved).expect("enter raw mode");
        let owned = owned_over(&input, &input, saved.clone());

        let mut screen = HalfDeaf::taking(4);
        let err = shutdown_with(&mut screen, &owned, false, Some(21), false, (24, 80))
            .expect_err("a restore the screen took part of must be reported");

        assert_eq!(
            screen.written,
            RESTORE.as_bytes()[..4],
            "the exit wrote past the prefix the screen took: {:?}",
            String::from_utf8_lossy(&screen.written)
        );
        assert!(
            err.to_string().contains('4'),
            "the failure does not say how much the screen accepted: {err}"
        );
        assert_eq!(
            live(input.as_fd()),
            words(&saved),
            "a partly written restore left the terminal raw"
        );
    }

    #[test]
    fn an_exit_whose_restore_took_nothing_at_all_still_attempts_the_cleanup() {
        // The contrast that keeps the skip above from spreading. This screen
        // failed the restore having taken **none** of it, so the terminal is
        // exactly where it was: the cleanup line means what it always meant,
        // and the all-attempt rule still governs. The same fake, asked for a
        // prefix of nothing, so the two cases differ in one number.
        let (_master, input) = open_pty();
        let saved = tcgetattr(&input).expect("read the terminal");
        enter_raw(input.as_fd(), &saved).expect("enter raw mode");
        let owned = owned_over(&input, &input, saved.clone());

        let mut screen = HalfDeaf::taking(0);
        let err = shutdown_with(&mut screen, &owned, false, Some(21), false, (24, 80))
            .expect_err("a restore the screen refused must be reported");

        assert_eq!(
            String::from_utf8_lossy(&screen.written),
            "\u{1b}[21;1H\u{1b}[J\u{1b}[?25h\n",
            "the cleanup line was skipped for a restore the screen never took"
        );
        assert_eq!(
            err.kind(),
            io::ErrorKind::BrokenPipe,
            "the first failure was not the one reported: {err}"
        );
        assert_eq!(
            live(input.as_fd()),
            words(&saved),
            "a refused restore left the terminal raw"
        );
    }

    #[test]
    fn a_session_with_no_band_moves_no_cursor_and_erases_nothing_on_the_way_out() {
        let (_master, input) = open_pty();
        let saved = tcgetattr(&input).expect("read the terminal");
        enter_raw(input.as_fd(), &saved).expect("enter raw mode");
        let owned = owned_over(&input, &input, saved.clone());

        let mut screen = Vec::new();
        shutdown_with(&mut screen, &owned, false, None, false, (24, 80)).expect("shut down");
        let text = String::from_utf8(screen).expect("the screen bytes are utf-8");

        assert_eq!(text, RESTORE, "the exit wrote more than the restore");
        assert!(
            !text.contains("\u{1b}[J"),
            "a screen xfx never drew on was erased: {text:?}"
        );
        assert_eq!(
            live(input.as_fd()),
            words(&saved),
            "the terminal is still raw"
        );
    }

    #[test]
    fn a_session_with_a_band_clears_from_its_top_downward() {
        let (_master, input) = open_pty();
        let saved = tcgetattr(&input).expect("read the terminal");
        enter_raw(input.as_fd(), &saved).expect("enter raw mode");
        let owned = owned_over(&input, &input, saved.clone());

        let mut screen = Vec::new();
        shutdown_with(&mut screen, &owned, false, Some(21), false, (24, 80)).expect("shut down");
        let text = String::from_utf8(screen).expect("the screen bytes are utf-8");

        assert_eq!(text, format!("{RESTORE}\u{1b}[21;1H\u{1b}[J\u{1b}[?25h\n"));
    }

    #[test]
    fn a_tmux_session_exits_through_the_tmux_restore() {
        let (_master, input) = open_pty();
        let saved = tcgetattr(&input).expect("read the terminal");
        let owned = owned_over(&input, &input, saved);

        let mut screen = Vec::new();
        shutdown_with(&mut screen, &owned, true, None, false, (24, 80)).expect("shut down");

        assert_eq!(
            String::from_utf8(screen).expect("the screen bytes are utf-8"),
            RESTORE_TMUX
        );
    }

    // -----------------------------------------------------------------------
    // the plane an exit may still be on
    // -----------------------------------------------------------------------

    #[test]
    fn a_normal_exit_from_the_alternate_screen_leaves_it_before_it_restores_anything() {
        // The ordinary restore carries no `1049l`, because the main surface
        // never takes the alternate screen. An approval that did take it is the
        // one case that has to be given back, and it has to be given back
        // *first*: every sequence in `RESTORE` is about the plane the user is
        // left looking at, and a title popped or an autowrap restored on the
        // alternate buffer is one restored for the wrong surface.
        let (_master, input) = open_pty();
        let saved = tcgetattr(&input).expect("read the terminal");
        enter_raw(input.as_fd(), &saved).expect("enter raw mode");
        let owned = owned_over(&input, &input, saved.clone());

        let mut screen = Vec::new();
        shutdown_with(&mut screen, &owned, false, Some(22), true, (24, 80)).expect("shut down");
        let text = String::from_utf8(screen).expect("the screen bytes are utf-8");

        assert!(
            text.starts_with("\u{1b}[?1049l"),
            "the exit restored the terminal on a plane it had not given back: {text:?}"
        );
        assert_eq!(
            text.matches("\u{1b}[?1049l").count(),
            1,
            "the exit left the alternate screen twice: {text:?}"
        );
        assert_eq!(
            text,
            format!("\u{1b}[?1049l{RESTORE}\u{1b}[22;1H\u{1b}[J\u{1b}[?25h\n"),
            "the exit wrote something other than the leave and the ordinary restore"
        );
        assert_eq!(
            live(input.as_fd()),
            words(&saved),
            "the terminal is still raw"
        );
    }

    #[test]
    fn an_exit_that_never_took_the_other_plane_still_leaves_nothing_to_give_back() {
        // The other half, and the one a defensive `1049l` on every exit would
        // break: a session that stayed on the normal buffer must not reset an
        // alternate screen it never entered -- on a terminal that models one,
        // that swaps in a buffer the user was not looking at.
        let (_master, input) = open_pty();
        let saved = tcgetattr(&input).expect("read the terminal");
        let owned = owned_over(&input, &input, saved);

        let mut screen = Vec::new();
        shutdown_with(&mut screen, &owned, false, None, false, (24, 80)).expect("shut down");

        assert_eq!(
            String::from_utf8(screen).expect("the screen bytes are utf-8"),
            RESTORE
        );
    }

    #[test]
    fn ui_panic_while_alternate_is_owned_restores_ownership_and_terminal_state() {
        // A panic and both death signals leave through the same pair
        // ([`restore_pair`]): the abnormal restore, which leads with `1049l`
        // whichever plane was on the screen, and then the captured `termios`.
        // The sequence is defensive by design -- an exit that does not know
        // what is on the screen may not ask -- so an approval screen that was
        // up when the process died is given back by exactly the same bytes.
        for tmux in [false, true] {
            let restore = abnormal_restore(tmux);
            assert!(
                restore.starts_with("\u{1b}[?1049l"),
                "the abnormal restore does not leave the alternate screen first: {restore:?}"
            );
            assert!(
                restore.ends_with("\u{1b}[?25h"),
                "the abnormal restore does not give the cursor back: {restore:?}"
            );
        }

        // And the line discipline with it, on the descriptor it was taken from.
        let (_master, input) = open_pty();
        let saved = tcgetattr(&input).expect("read the terminal");
        enter_raw(input.as_fd(), &saved).expect("enter raw mode");
        assert_ne!(
            live(input.as_fd()),
            words(&saved),
            "raw mode changed nothing, so the restore proves nothing"
        );
        restore_attrs(&owned_over(&input, &input, saved.clone())).expect("restore");
        assert_eq!(live(input.as_fd()), words(&saved));
    }

    #[test]
    fn sigterm_and_sighup_while_alternate_is_owned_restore_ownership_and_terminal_state() {
        // Each signal separately, and both through the one pair a handler may
        // use. This is the seam; `tests/tui.rs` drives the two signals at a real
        // process that really is on the alternate screen.
        assert_eq!(
            abnormal_restore(false),
            ABNORMAL_RESTORE,
            "a handler would write something other than the abnormal restore"
        );
        assert_eq!(abnormal_restore(true), ABNORMAL_RESTORE_TMUX);
        for restore in [abnormal_restore(false), abnormal_restore(true)] {
            assert_eq!(
                restore.matches("\u{1b}[?1049l").count(),
                1,
                "a handler leaves the alternate screen more than once: {restore:?}"
            );
        }
    }

    #[test]
    fn a_terminal_that_will_not_say_its_size_is_twenty_four_by_eighty() {
        assert_eq!(
            size_or_default(Err(rustix::io::Errno::NOTTY)),
            (DEFAULT_ROWS, DEFAULT_COLS)
        );
    }

    #[test]
    fn a_zero_dimension_is_a_refusal_rather_than_a_size() {
        for (rows, cols) in [(0, 80), (24, 0), (0, 0)] {
            assert_eq!(
                size_or_default(Ok(Winsize {
                    ws_row: rows,
                    ws_col: cols,
                    ws_xpixel: 0,
                    ws_ypixel: 0,
                })),
                (DEFAULT_ROWS, DEFAULT_COLS),
                "{rows}x{cols} was taken for a size"
            );
        }
    }

    #[test]
    fn a_terminal_that_answers_is_taken_at_its_word() {
        assert_eq!(
            size_or_default(Ok(Winsize {
                ws_row: 40,
                ws_col: 132,
                ws_xpixel: 0,
                ws_ypixel: 0,
            })),
            (40, 132)
        );
    }

    #[test]
    fn a_reported_size_keeps_a_zero_rather_than_inventing_a_screen() {
        // The post-launch reading. A pty whose size was never set answers `0x0`
        // successfully, and a running session already has a band on a screen of
        // a known size -- so a zero is *no new information*, and the caller
        // that gets it leaves the band exactly where it is.
        for (rows, cols) in [(0, 80), (24, 0), (0, 0)] {
            assert_eq!(
                size_as_reported(Ok(Winsize {
                    ws_row: rows,
                    ws_col: cols,
                    ws_xpixel: 0,
                    ws_ypixel: 0,
                })),
                (0, 0),
                "{rows}x{cols} was answered with a screen the terminal never described"
            );
        }
    }

    #[test]
    fn a_terminal_that_will_not_say_its_size_after_launch_says_nothing_at_all() {
        // A refusal and a zero are the same fact on this side of the launch,
        // and it is not the fact they are at launch: there the band has to be
        // solved from *something*, here there is already one.
        assert_eq!(size_as_reported(Err(rustix::io::Errno::NOTTY)), (0, 0));
        assert_eq!(size_as_reported(Err(rustix::io::Errno::BADF)), (0, 0));
    }

    #[test]
    fn a_reported_size_a_terminal_really_gives_is_taken_at_its_word() {
        assert_eq!(
            size_as_reported(Ok(Winsize {
                ws_row: 40,
                ws_col: 132,
                ws_xpixel: 0,
                ws_ypixel: 0,
            })),
            (40, 132)
        );
    }

    #[test]
    fn the_launch_and_the_running_session_read_the_same_refusal_differently() {
        // The whole reason there are two functions, stated as the one thing
        // that must never become true of them: that they agree. A launch has no
        // band and must solve one from a number, so a refusal is 24x80 there; a
        // running session has a band on a screen it measured, so a refusal
        // there is a reading to ignore rather than a screen to move to. One
        // function serving both would move a 40x132 session's band onto rows
        // 22-24 because its terminal declined to answer once.
        let refused: [Result<Winsize, rustix::io::Errno>; 2] = [
            Err(rustix::io::Errno::NOTTY),
            Ok(Winsize {
                ws_row: 0,
                ws_col: 0,
                ws_xpixel: 0,
                ws_ypixel: 0,
            }),
        ];
        for reading in refused {
            assert_eq!(size_or_default(reading), (DEFAULT_ROWS, DEFAULT_COLS));
            assert_eq!(size_as_reported(reading), (0, 0));
            assert_ne!(
                size_or_default(reading),
                size_as_reported(reading),
                "the launch fallback and the post-launch reading became one answer"
            );
        }
    }

    #[test]
    fn a_size_the_terminal_gives_means_the_same_thing_to_both_of_them() {
        // The other side of it: the two differ **only** on a reading that says
        // nothing. A session whose window really is 40x132 is told so by either.
        let real = Ok(Winsize {
            ws_row: 40,
            ws_col: 132,
            ws_xpixel: 0,
            ws_ypixel: 0,
        });
        assert_eq!(size_or_default(real), size_as_reported(real));
    }

    #[test]
    fn a_cleanup_line_the_check_refuses_is_not_written_and_the_terminal_is_still_cooked() {
        // The band's top row on a screen that no longer has one -- a `CUP` a
        // terminal answers by clamping, silently, onto a row the exit would
        // then erase from. The refusal costs the cleanup line and nothing else:
        // the restore went out, the line discipline is back, and the error is
        // reported rather than swallowed.
        let (_master, input) = open_pty();
        let saved = tcgetattr(&input).expect("read the terminal");
        enter_raw(input.as_fd(), &saved).expect("enter raw mode");
        let owned = owned_over(&input, &input, saved.clone());

        let mut screen = Vec::new();
        let err = shutdown_with(&mut screen, &owned, false, Some(30), false, (24, 80))
            .expect_err("a cleanup addressed off the screen");

        let text = String::from_utf8(screen).expect("the screen bytes are utf-8");
        assert_eq!(text, RESTORE, "the refused cleanup line was written anyway");
        assert!(
            err.to_string().contains("output check refused"),
            "the exit swallowed the refusal: {err}"
        );
        assert_eq!(
            live(input.as_fd()),
            words(&saved),
            "a refused cleanup line left the terminal raw"
        );
    }

    #[test]
    fn a_restore_that_left_the_cursor_hidden_is_refused() {
        // `?25h` is the last sequence of every restore, and a constant is
        // exactly where a regression hides: a session that exited with the
        // cursor still hidden leaves the user's shell with no caret.
        let damaged = RESTORE.replace("\u{1b}[?25h", "");
        let refused = check_restore(&damaged, false, false, (24, 80))
            .expect_err("a restore that kept the cursor hidden");
        assert!(
            refused.to_string().contains("cursor visibility"),
            "the refusal did not name the cursor: {refused}"
        );
        check_restore(RESTORE, false, false, (24, 80)).expect("the restore as it stands");
    }

    #[test]
    fn a_restore_that_leaves_a_mode_on_is_refused() {
        // Every mode this session turned on is one the terminal's own user gets
        // back. A restore that dropped the bracketed-paste reset would leave a
        // shell pasting in a mode it never asked for.
        let damaged = RESTORE.replace("\u{1b}[?2004l", "");
        assert!(check_restore(&damaged, false, false, (24, 80)).is_err());
    }

    #[test]
    fn a_restore_that_did_not_pop_the_title_stack_is_refused() {
        // The window title this session set is *borrowed*: the mode set pushes
        // the terminal's own onto its title stack and the restore pops it back.
        // A restore that lost its pop leaves the user's window wearing a title
        // xfx chose, and the entry behind it unreachable.
        //
        // **What this pins is local to this vector**: one pop against the one
        // push it is seeded with. It says nothing about how many pushes a
        // session that was stopped and continued has made -- Unit A keeps no
        // state between vectors, and a depth across them would need one.
        let damaged = RESTORE.replace("\u{1b}[23;2t", "");
        let refused = check_restore(&damaged, false, false, (24, 80))
            .expect_err("a restore that kept the title it borrowed");
        assert!(
            refused.to_string().contains("title stack"),
            "the refusal did not name the title stack: {refused}"
        );
        check_restore(RESTORE, false, false, (24, 80)).expect("the restore as it stands");
    }

    #[test]
    fn a_restore_written_while_the_borrowed_plane_is_up_must_give_it_back() {
        // `RESTORE` carries no `1049l` on purpose, so the leave in front of it
        // is what the plane depends on: without it every sequence after it is
        // restored for the buffer the user is not looking at.
        assert!(check_restore(RESTORE, false, true, (24, 80)).is_err());
        check_restore(&format!("\u{1b}[?1049l{RESTORE}"), false, true, (24, 80))
            .expect("the leave in front of the restore");
    }

    #[test]
    fn a_cleanup_line_that_erased_above_the_band_is_refused() {
        // `ED 0` erases from the caret down; an `ED 2` would take the whole
        // screen, and everything above the band's top row is the terminal's own
        // document -- answers the user is still reading.
        let refused = check_cleanup("\u{1b}[21;1H\u{1b}[2J\u{1b}[?25h\n", 21, (24, 80))
            .expect_err("an exit that erased the document");
        assert!(
            refused.to_string().contains("outside the footprint"),
            "the refusal did not name the footprint: {refused}"
        );
        check_cleanup("\u{1b}[21;1H\u{1b}[J\u{1b}[?25h\n", 21, (24, 80))
            .expect("the cleanup line as it stands");
    }

    #[test]
    fn the_cleanup_lines_last_linefeed_scrolls_exactly_when_the_band_began_on_the_last_row() {
        // From the bottom row the trailing linefeed scrolls the screen by one
        // and the caret stays where it is; from anywhere else it walks the
        // caret down and moves nothing. Both are asserted, because a scroll
        // nobody declared puts a document row into native scrollback for good.
        check_cleanup("\u{1b}[24;1H\u{1b}[J\u{1b}[?25h\n", 24, (24, 80))
            .expect("a band that began on the last row");
        check_cleanup("\u{1b}[21;1H\u{1b}[J\u{1b}[?25h\n", 21, (24, 80))
            .expect("a band that began above it");
        // And a second linefeed -- one scroll too many from the bottom row.
        assert!(check_cleanup("\u{1b}[24;1H\u{1b}[J\u{1b}[?25h\n\n", 24, (24, 80)).is_err());
    }

    #[test]
    fn a_restore_segment_that_moved_the_caret_is_refused() {
        // The restore declares modes, a plane and a title stack -- it declares
        // no caret at all, and it moves none. A `CUP` inside it puts the user's
        // shell cursor wherever the sequence says, on a screen this session is
        // in the middle of giving back.
        let damaged = format!("{RESTORE}\u{1b}[5;1H");
        let refused = check_restore(&damaged, false, false, (24, 80))
            .expect_err("a restore that moved the caret");
        assert!(
            refused.to_string().contains("caret"),
            "the refusal did not name the caret: {refused}"
        );
    }

    #[test]
    fn a_restore_segment_that_erased_the_scrollback_is_refused() {
        // The same shape as the caret above and the worst of them: `ED 3` moves
        // no cell, so nothing about the screen's contents disagrees, and what
        // it takes is every document row the session ever scrolled away.
        let damaged = format!("{RESTORE}\u{1b}[3J");
        assert!(check_restore(&damaged, false, false, (24, 80)).is_err());
    }

    #[test]
    fn a_cleanup_line_that_erased_the_scrollback_is_refused() {
        let damaged = "\u{1b}[21;1H\u{1b}[J\u{1b}[3J\u{1b}[?25h\n";
        assert!(check_cleanup(damaged, 21, (24, 80)).is_err());
    }

    #[test]
    fn a_first_segment_the_check_refuses_still_restores_the_terminal_and_attempts_the_second() {
        // The conjunction the plan names and the first implementation left
        // untested: it is a **check** refusal on segment one rather than a
        // write failure, and the rule is the same -- the line discipline goes
        // back, the cleanup line is still attempted, and the error is returned
        // rather than swallowed. A refusal that short-circuited any of that
        // would leave a terminal raw, which is worse than any screen it saves.
        let (_master, input) = open_pty();
        let saved = tcgetattr(&input).expect("read the terminal");
        enter_raw(input.as_fd(), &saved).expect("enter raw mode");
        let owned = owned_over(&input, &input, saved.clone());

        let mut screen = Vec::new();
        let err = tampering(
            |restore| {
                // The cursor left hidden: refused by the check, and by nothing
                // else in this function.
                *restore = restore.replace("\u{1b}[?25h", "");
            },
            || shutdown_with(&mut screen, &owned, false, Some(21), false, (24, 80)),
        )
        .expect_err("a restore segment the check refused");

        let text = String::from_utf8(screen).expect("the screen bytes are utf-8");
        assert!(
            !text.contains("\u{1b}[23;2t"),
            "the refused restore segment was written anyway: {text:?}"
        );
        assert_eq!(
            text, "\u{1b}[21;1H\u{1b}[J\u{1b}[?25h\n",
            "the second segment was skipped because the first was refused"
        );
        assert!(
            err.to_string().contains("output check refused"),
            "the exit swallowed the refusal: {err}"
        );
        assert_eq!(
            live(input.as_fd()),
            words(&saved),
            "a refused first segment left the terminal raw"
        );
    }
}
