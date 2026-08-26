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

use std::io::{self, Write};
use std::os::fd::{BorrowedFd, RawFd};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::thread::ThreadId;

use rustix::termios::{
    tcgetattr, tcgetwinsize, tcsetattr, ControlModes, InputModes, LocalModes, OptionalActions,
    SpecialCodeIndex, Termios, Winsize,
};

/// The modes the TUI turns on when it takes the terminal.
///
/// modifyOtherKeys, the kitty keyboard push, bracketed paste, autowrap off
/// (`terminal.zig:4-13`) and `XTWINOPS 22 ; 2` -- the push of the terminal's
/// own **window title** onto its title stack, so the one the band sets
/// (`OSC 2`, `super::frame::title`) is borrowed rather than taken. The window
/// title only, rather than the icon name with it, because that is the only one
/// xfx sets: a push that claimed more than the pop gives back is a stack entry
/// left behind on every exit.
///
/// The push is the **last** thing in the mode set and the pop is the first
/// thing in every restore below, so the title a session sets exists only
/// between the two -- and a terminal that models no title stack ignores both
/// and keeps whatever its user gave it.
///
/// Mouse reporting is deliberately absent: the wheel stays the terminal's own
/// scrollback (`terminal.zig:135-142`).
pub(crate) const MODE_SET: &str = "\x1b[>4;2m\x1b[>1u\x1b[?2004h\x1b[?7l\x1b[22;2t";

/// The same, without the kitty keyboard push, which breaks key input under
/// tmux (`terminal.zig:29-34`).
pub(crate) const MODE_SET_TMUX: &str = "\x1b[>4;2m\x1b[?2004h\x1b[?7l\x1b[22;2t";

/// The normal exit's restore sequence, with **no** `1049l`: the main surface
/// was never on the alternate screen (`app_lifecycle.zig:39-41`).
pub(crate) const RESTORE: &str = "\x1b[23;2t\x1b[>4;0m\x1b[<u\x1b[?2004l\x1b[?7h\x1b[?25h";

/// The same for tmux, which was never given the push to pop.
pub(crate) const RESTORE_TMUX: &str = "\x1b[23;2t\x1b[>4;0m\x1b[?2004l\x1b[?7h\x1b[?25h";

/// The restore sequence for an exit that is *not* the planned one, which leads
/// with `1049l` defensively: a crash may have happened while a surface xfx does
/// not own was on screen (`app_lifecycle.zig:36-38`).
pub(crate) const ABNORMAL_RESTORE: &str =
    "\x1b[?1049l\x1b[23;2t\x1b[>4;0m\x1b[<u\x1b[?2004l\x1b[?7h\x1b[?25h";

/// The abnormal restore for tmux.
pub(crate) const ABNORMAL_RESTORE_TMUX: &str =
    "\x1b[?1049l\x1b[23;2t\x1b[>4;0m\x1b[?2004l\x1b[?7h\x1b[?25h";

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
    let bytes = if TMUX.load(Ordering::Acquire) {
        ABNORMAL_RESTORE_TMUX
    } else {
        ABNORMAL_RESTORE
    }
    .as_bytes();
    // SAFETY: `write` is async-signal-safe, the fd was recorded at entry and is
    // owned by the process for its whole life, and the buffer is 'static.
    unsafe {
        libc::write(owned.output, bytes.as_ptr().cast(), bytes.len());
    }
    let _ = restore_attrs(owned);
}

/// The normal exit, in upstream's order (`app_lifecycle.zig:578-593`): write the
/// restore sequence, `tcsetattr` the saved `termios`, then move to the band's
/// top and clear downward.
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
/// error, so there is no `?` between here and the end of the function.
pub(crate) fn shutdown(band_top: Option<u16>) -> io::Result<()> {
    let Some(owned) = OWNED.get() else {
        return Ok(());
    };
    // The locked stdout is the same descriptor as `owned.output`; it is used
    // rather than the raw fd because this path may take a lock and buffer,
    // which the signal path may not.
    let mut out = io::stdout().lock();
    shutdown_with(&mut out, owned, TMUX.load(Ordering::Acquire), band_top)
}

/// The exit above, against an explicit screen and an explicit ownership record,
/// so that "the line discipline is restored even when the screen cannot be
/// written" is a test rather than a claim.
fn shutdown_with(
    out: &mut impl Write,
    owned: &Owned,
    tmux: bool,
    band_top: Option<u16>,
) -> io::Result<()> {
    let restore = if tmux { RESTORE_TMUX } else { RESTORE };
    let screen = write!(out, "{restore}").and_then(|()| out.flush());
    let attrs = restore_attrs(owned);
    let cleanup = match band_top {
        // Leaves the transcript in scrollback and the cursor on a clean line.
        Some(top) => writeln!(out, "\x1b[{top};1H\x1b[J\x1b[?25h").and_then(|()| out.flush()),
        None => Ok(()),
    };
    screen.and(attrs).and(cleanup)
}

/// The terminal's dimensions, or 24x80 when it will not say. A terminal query,
/// so it lives here; `layout::solve` takes rows and columns as arguments and
/// stays pure, which is what makes its unit tests possible.
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

/// The dimensions a `TIOCGWINSZ` answer means.
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

#[cfg(test)]
mod tests {
    use super::*;

    use std::os::fd::{AsFd, AsRawFd, OwnedFd};

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

    impl Write for BrokenScreen {
        fn write(&mut self, _bytes: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "the screen went away",
            ))
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "the screen went away",
            ))
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
            "\u{1b}[>4;2m\u{1b}[>1u\u{1b}[?2004h\u{1b}[?7l\u{1b}[22;2t"
        );
        assert_eq!(
            MODE_SET_TMUX,
            "\u{1b}[>4;2m\u{1b}[?2004h\u{1b}[?7l\u{1b}[22;2t"
        );
        assert_eq!(
            RESTORE,
            "\u{1b}[23;2t\u{1b}[>4;0m\u{1b}[<u\u{1b}[?2004l\u{1b}[?7h\u{1b}[?25h"
        );
        assert_eq!(
            RESTORE_TMUX,
            "\u{1b}[23;2t\u{1b}[>4;0m\u{1b}[?2004l\u{1b}[?7h\u{1b}[?25h"
        );
        assert_eq!(
            ABNORMAL_RESTORE,
            "\u{1b}[?1049l\u{1b}[23;2t\u{1b}[>4;0m\u{1b}[<u\u{1b}[?2004l\u{1b}[?7h\u{1b}[?25h"
        );
        assert_eq!(
            ABNORMAL_RESTORE_TMUX,
            "\u{1b}[?1049l\u{1b}[23;2t\u{1b}[>4;0m\u{1b}[?2004l\u{1b}[?7h\u{1b}[?25h"
        );
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
        let err = shutdown_with(&mut BrokenScreen, &owned, false, Some(21))
            .expect_err("a screen that refuses every write must be reported");

        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe, "{err}");
        assert_eq!(
            live(input.as_fd()),
            words(&saved),
            "the terminal was left raw because the screen failed first"
        );
    }

    #[test]
    fn a_session_with_no_band_moves_no_cursor_and_erases_nothing_on_the_way_out() {
        let (_master, input) = open_pty();
        let saved = tcgetattr(&input).expect("read the terminal");
        enter_raw(input.as_fd(), &saved).expect("enter raw mode");
        let owned = owned_over(&input, &input, saved.clone());

        let mut screen = Vec::new();
        shutdown_with(&mut screen, &owned, false, None).expect("shut down");
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
        shutdown_with(&mut screen, &owned, false, Some(21)).expect("shut down");
        let text = String::from_utf8(screen).expect("the screen bytes are utf-8");

        assert_eq!(text, format!("{RESTORE}\u{1b}[21;1H\u{1b}[J\u{1b}[?25h\n"));
    }

    #[test]
    fn a_tmux_session_exits_through_the_tmux_restore() {
        let (_master, input) = open_pty();
        let saved = tcgetattr(&input).expect("read the terminal");
        let owned = owned_over(&input, &input, saved);

        let mut screen = Vec::new();
        shutdown_with(&mut screen, &owned, true, None).expect("shut down");

        assert_eq!(
            String::from_utf8(screen).expect("the screen bytes are utf-8"),
            RESTORE_TMUX
        );
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
}
