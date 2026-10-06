//! The one way the TUI puts bytes on a terminal, and the receipt it keeps.
//!
//! Everything the session writes while it holds the terminal goes through
//! [`Sink::emit`], and an emit is **counted**: it asks the descriptor for the
//! whole vector, adds up what the kernel says it took, and reports a failure
//! against that number. There is no buffer and no flush, which is what makes
//! the number mean anything -- a writer that had buffered the vector could
//! neither say how much of it the terminal has nor promise that the rest will
//! not arrive later, on top of whatever the session wrote next.
//!
//! Three failures, because a caller has to tell them apart:
//!
//! * [`Emit::Rejected`] -- the output check refused the vector. Nothing was
//!   offered to any descriptor, so the screen is exactly as it was.
//! * [`Emit::ZeroProgress`] -- the descriptor failed and took nothing. Same
//!   screen, different reason, and the caller's existing frame budget is what
//!   both of them spend.
//! * [`Emit::Partial`] -- the descriptor took some of the vector and then
//!   failed. What reached the terminal may be incomplete, and no vector this
//!   session could write is known to fix that: where the prefix stopped inside
//!   the vector is knowable, what the terminal made of it is not.
//!
//! **An accepted count is a kernel receipt and nothing more.** It says the
//! bytes left this process; it does not say a terminal parsed them, a screen
//! shows them, or a line discipline is back. Nothing in this module may be read
//! as any of those.

use std::io;
use std::os::fd::{BorrowedFd, RawFd};

/// What became of a vector that did not go out whole.
#[derive(Debug)]
pub(crate) enum Emit {
    /// The output check refused it. Not an I/O failure, and it consumed no
    /// output: the caller's budget treats it exactly as a write the descriptor
    /// refused, which is the policy that was already there.
    Rejected(io::Error),
    /// The descriptor failed before it took a single byte.
    ZeroProgress(io::Error),
    /// The descriptor took `delivered` bytes of the vector and then failed.
    Partial { delivered: usize, cause: io::Error },
}

impl Emit {
    /// A vector the output check refused, whatever shape the refusal arrived
    /// in. It consumed no output, so it is not a delivery failure at all --
    /// which is exactly why it has to be a variant rather than a bare
    /// `io::Error` that a caller might read as one.
    pub(crate) fn rejected(refusal: impl Into<io::Error>) -> Self {
        Self::Rejected(refusal.into())
    }

    /// The failure as the rest of the session reports it.
    ///
    /// A refusal and a zero-progress failure travel as themselves -- callers
    /// read their `kind` and the checker's own words, and a wrapper here would
    /// take those away. A partial keeps its count, as an error payload rather
    /// than as a mutable receipt somewhere: the number belongs to the failure
    /// that carries it and to nothing that outlives it.
    pub(crate) fn into_error(self) -> io::Error {
        match self {
            Self::Rejected(err) | Self::ZeroProgress(err) => err,
            Self::Partial { delivered, cause } => {
                io::Error::new(cause.kind(), Prefix { delivered, cause })
            }
        }
    }
}

/// A failed emit, said once, wherever it is read from: the error a caller
/// converts it into and the value itself carry the same sentence.
impl std::fmt::Display for Emit {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rejected(err) | Self::ZeroProgress(err) => err.fmt(out),
            Self::Partial { delivered, cause } => describe_prefix(out, *delivered, cause),
        }
    }
}

/// What a prefix on a terminal is reported as.
///
/// It names the count and the cause and nothing else. No row, no title, no
/// byte of the vector: a diagnostic that quoted what it failed to write would
/// put the user's own text -- and whatever a provider put in it -- into a
/// message that ends up in logs. It claims nothing about the screen either:
/// what a terminal did with an incomplete vector is not knowable from here.
fn describe_prefix(
    out: &mut std::fmt::Formatter<'_>,
    delivered: usize,
    cause: &io::Error,
) -> std::fmt::Result {
    write!(
        out,
        "the terminal accepted {delivered} bytes of this write and then failed \
         ({cause}); what reached it may be incomplete"
    )
}

/// The payload a converted [`Emit::Partial`] carries, so the count survives
/// as structure rather than only as prose.
#[derive(Debug)]
struct Prefix {
    delivered: usize,
    cause: io::Error,
}

impl std::fmt::Display for Prefix {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        describe_prefix(out, self.delivered, &self.cause)
    }
}

impl std::error::Error for Prefix {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}

/// Somewhere a checked vector can be sent, whole or not at all.
pub(crate) trait Sink {
    /// Delivers `bytes`, or says how much of them is on the terminal.
    fn emit(&mut self, bytes: &[u8]) -> Result<(), Emit>;
}

/// One `write` syscall, and no interpretation of it.
///
/// Separate from [`Sink`] so that the counting above is written **once** and is
/// the same code in production and under test: a case scripts the syscalls, not
/// the accounting.
pub(crate) trait RawWrite {
    /// Offers `bytes` to the descriptor once, and answers what it took.
    fn write_once(&mut self, bytes: &[u8]) -> io::Result<usize>;
}

/// The counting itself: offer, add up, and classify what stopped it.
///
/// `Interrupted` is retried, because a signal that landed between two syscalls
/// took nothing and means nothing to the vector. `WouldBlock` is **not**: it
/// returns on the spot, whichever side of the first accepted byte it arrives
/// on. A spin would burn the UI thread on a descriptor somebody else made
/// non-blocking, and a retry of the whole vector after a prefix would put that
/// prefix on the terminal twice.
///
/// A count larger than what was offered is a broken wrapper rather than a
/// terminal, and it is **refused rather than capped**. Capping it would end the
/// loop and return `Ok`: a receipt saying the whole vector reached the terminal,
/// minted from the one answer this process knows to be false. What the failure
/// carries instead is the count from *before* that call -- the last number that
/// can be stood behind -- because nothing about the call that produced the
/// claim is knowable, including how much of it really went out.
pub(crate) fn emit_counted<R: RawWrite + ?Sized>(raw: &mut R, bytes: &[u8]) -> Result<(), Emit> {
    let mut delivered = 0usize;
    while delivered < bytes.len() {
        match raw.write_once(&bytes[delivered..]) {
            // A `write` that succeeded and moved nothing: the slice would not
            // shrink and the loop would never end.
            Ok(0) => {
                return Err(classify(
                    delivered,
                    io::Error::new(
                        io::ErrorKind::WriteZero,
                        "the terminal took none of the bytes it was offered",
                    ),
                ))
            }
            // More than it was offered. `delivered` stays where it was, so the
            // failure reports only what was accountable before this call.
            Ok(taken) if taken > bytes.len() - delivered => {
                return Err(classify(
                    delivered,
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "the write reported more bytes than it was offered",
                    ),
                ))
            }
            Ok(taken) => {
                delivered += taken;
            }
            Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
            Err(err) => return Err(classify(delivered, err)),
        }
    }
    Ok(())
}

/// Which failure a stopped emit is, which is decided by one thing: whether any
/// byte of this vector is already on the terminal.
fn classify(delivered: usize, cause: io::Error) -> Emit {
    if delivered == 0 {
        Emit::ZeroProgress(cause)
    } else {
        Emit::Partial { delivered, cause }
    }
}

/// The terminal itself, written one syscall at a time.
///
/// It **borrows** the descriptor the session's screen state leaves by rather
/// than owning a second one: `super::term` recorded that descriptor at entry
/// and is still the authority on it, and a sink that had dup'd it would be a
/// second lifetime to reason about on every exit path. Constructed per write
/// site, held for no longer than the call, and carrying no state of its own --
/// there is nothing here for two of them to disagree about.
pub(crate) struct RawTty {
    fd: RawFd,
}

impl RawTty {
    /// The descriptor the mode set, the band and the restore all go to.
    pub(crate) fn stdout() -> Self {
        Self {
            fd: libc::STDOUT_FILENO,
        }
    }
}

impl RawTty {
    /// The syscall itself, with nothing in front of it.
    fn write_now(&mut self, bytes: &[u8]) -> io::Result<usize> {
        // SAFETY: the descriptor is the process's own standard output, which it
        // holds for its whole life; `borrow_raw` takes no ownership of it.
        let fd = unsafe { BorrowedFd::borrow_raw(self.fd) };
        rustix::io::write(fd, bytes).map_err(io::Error::from)
    }
}

impl RawWrite for RawTty {
    fn write_once(&mut self, bytes: &[u8]) -> io::Result<usize> {
        // The one point a build asked for a torn frame produces one, and it is
        // here rather than in the band: what the restoration matrix needs is a
        // real prefix on a real terminal, taken by the real counting, so that
        // everything downstream of it is the product's own behaviour. A shipped
        // binary contains neither this branch nor the enum behind it.
        #[cfg(feature = "fault-injection")]
        if let Some(answer) = super::fault::partial_frame_answer(bytes.len()) {
            return match answer {
                super::fault::Prefix::Takes(count) => self.write_now(&bytes[..count]),
                super::fault::Prefix::Fails => Err(io::Error::from_raw_os_error(libc::EIO)),
            };
        }
        // P3-DIAGNOSTIC's matrix row: a screen that refuses every band frame
        // and takes nothing of any of them, while the mode-set and restore
        // sequences -- which do not open a frame -- pass through untouched.
        #[cfg(feature = "fault-injection")]
        if let Some(err) = super::fault::frame_refusal_answer(bytes) {
            return Err(err);
        }
        self.write_now(bytes)
    }
}

impl Sink for RawTty {
    fn emit(&mut self, bytes: &[u8]) -> Result<(), Emit> {
        emit_counted(self, bytes)
    }
}

/// A vector in memory takes everything, which is what most cases want from a
/// screen: they are about the bytes, not about the delivery.
///
/// **Test builds only.** Production has one sink and it is [`RawTty`]; an
/// infallible `Sink` compiled into a release binary is a screen that can never
/// refuse, which is exactly the thing every case in this module exists to rule
/// out being reachable by accident.
#[cfg(test)]
impl Sink for Vec<u8> {
    fn emit(&mut self, bytes: &[u8]) -> Result<(), Emit> {
        self.extend_from_slice(bytes);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::VecDeque;

    /// The vector every case below offers. Ten bytes of nothing in particular:
    /// what this module does to a slice is the same whatever the slice means,
    /// and an escape sequence here would invite the reader to believe the sink
    /// knows what one is.
    const VECTOR: &[u8] = b"0123456789";

    /// What the next syscall on a [`Scripted`] terminal does.
    enum Answer {
        /// Takes up to this many bytes of what it was offered, and says so.
        /// `Takes(0)` is the `write` that succeeded and moved nothing.
        Takes(usize),
        /// Takes nothing and **claims** this many, which is the one answer a
        /// syscall wrapper may never be believed about.
        Claims(usize),
        /// Fails, having taken nothing.
        Fails(io::Error),
    }

    /// A terminal scripted one syscall at a time.
    ///
    /// It is the real algorithm under test: [`emit_counted`] is what production
    /// runs against a descriptor, and what this runs against a list of answers.
    /// An unscripted call takes everything it is offered, so a case scripts
    /// only the answers it is about -- and a run that made *more* calls than
    /// its script named is visible in `calls` rather than silently absorbed.
    struct Scripted {
        answers: VecDeque<Answer>,
        taken: Vec<u8>,
        calls: usize,
    }

    impl Scripted {
        fn new(answers: impl IntoIterator<Item = Answer>) -> Self {
            Self {
                answers: answers.into_iter().collect(),
                taken: Vec::new(),
                calls: 0,
            }
        }
    }

    impl RawWrite for Scripted {
        fn write_once(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.calls += 1;
            match self
                .answers
                .pop_front()
                .unwrap_or(Answer::Takes(bytes.len()))
            {
                Answer::Takes(count) => {
                    let taken = count.min(bytes.len());
                    self.taken.extend_from_slice(&bytes[..taken]);
                    Ok(taken)
                }
                Answer::Claims(count) => Ok(count),
                Answer::Fails(err) => Err(err),
            }
        }
    }

    impl Sink for Scripted {
        fn emit(&mut self, bytes: &[u8]) -> Result<(), Emit> {
            emit_counted(self, bytes)
        }
    }

    fn broke() -> io::Error {
        io::Error::from_raw_os_error(libc::EIO)
    }

    #[test]
    fn one_emit_is_the_whole_vector_however_many_syscalls_it_takes() {
        // "One emit" is not "one `write`": a terminal may take a vector in
        // pieces and that is an ordinary success. This is the case that keeps
        // the single-emit assertions elsewhere honest -- none of them is a
        // claim about syscalls.
        let mut terminal = Scripted::new([Answer::Takes(3), Answer::Takes(1)]);

        terminal
            .emit(VECTOR)
            .expect("a terminal that took every byte");

        assert_eq!(terminal.taken, VECTOR);
        assert_eq!(
            terminal.calls, 3,
            "the vector did not go out in the pieces the script named"
        );
    }

    #[test]
    fn the_bytes_the_terminal_took_before_it_failed_are_counted_exactly() {
        let mut terminal = Scripted::new([Answer::Takes(5), Answer::Fails(broke())]);

        let failure = terminal
            .emit(VECTOR)
            .expect_err("the scripted failure never arrived");

        let Emit::Partial { delivered, cause } = failure else {
            panic!("a failure after five accepted bytes was not partial: {failure:?}");
        };
        assert_eq!(delivered, 5);
        assert_eq!(cause.raw_os_error(), Some(libc::EIO), "{cause}");
        assert_eq!(terminal.taken, b"01234");
    }

    #[test]
    fn no_tail_of_a_failed_emit_reaches_the_terminal_on_a_later_one() {
        // The property a buffered writer cannot have. Nothing is held between
        // emits -- there is no buffer to hold it in -- so the next vector this
        // sink is given is the only thing the next vector puts on the wire.
        // The application's own rule, that a partial frame ends the session
        // rather than offering another vector, is a separate fact and is
        // asserted where it lives (`super::event_loop`).
        let mut terminal = Scripted::new([Answer::Takes(5), Answer::Fails(broke())]);
        terminal.emit(VECTOR).expect_err("the scripted failure");
        assert_eq!(terminal.taken, b"01234");

        terminal
            .emit(b"ABC")
            .expect("a terminal taking bytes again");

        assert_eq!(
            terminal.taken, b"01234ABC",
            "the failed vector's tail came back out of a buffer"
        );
    }

    #[test]
    fn a_signal_inside_a_write_costs_a_syscall_and_no_bytes() {
        let mut terminal = Scripted::new([
            Answer::Takes(4),
            Answer::Fails(io::Error::from(io::ErrorKind::Interrupted)),
        ]);

        terminal
            .emit(VECTOR)
            .expect("an interrupted write is the kernel's, not a failure");

        assert_eq!(terminal.taken, VECTOR);
        assert_eq!(
            terminal.calls, 3,
            "the interruption cost more than one retry"
        );
    }

    #[test]
    fn a_terminal_that_would_block_before_taking_a_byte_is_zero_progress() {
        // And it is asked **once**. A spin here would burn the UI thread on a
        // descriptor somebody else made non-blocking, and the script's exhaustion
        // makes a second call visible: an unscripted one takes everything.
        let mut terminal =
            Scripted::new([Answer::Fails(io::Error::from(io::ErrorKind::WouldBlock))]);

        let failure = terminal.emit(VECTOR).expect_err("a terminal that was full");

        let Emit::ZeroProgress(cause) = failure else {
            panic!("a terminal that took nothing reported progress: {failure:?}");
        };
        assert_eq!(cause.kind(), io::ErrorKind::WouldBlock);
        assert_eq!(terminal.calls, 1, "the emit spun on a full terminal");
        assert!(terminal.taken.is_empty());
    }

    #[test]
    fn a_terminal_that_would_block_after_taking_a_prefix_is_partial() {
        // The deliberate sacrifice: on an inherited non-blocking descriptor a
        // prefix followed by `EAGAIN` is *recoverable* in principle, and this
        // unit refuses to recover it. Retrying the whole vector would put the
        // prefix on the terminal twice, and this unit has no resumable state.
        let mut terminal = Scripted::new([
            Answer::Takes(6),
            Answer::Fails(io::Error::from(io::ErrorKind::WouldBlock)),
        ]);

        let failure = terminal
            .emit(VECTOR)
            .expect_err("a terminal that filled up");

        let Emit::Partial { delivered, cause } = failure else {
            panic!("a block after six accepted bytes was not partial: {failure:?}");
        };
        assert_eq!(delivered, 6);
        assert_eq!(cause.kind(), io::ErrorKind::WouldBlock);
        assert_eq!(terminal.calls, 2, "the emit spun after the prefix");
        assert_eq!(terminal.taken, b"012345");
    }

    #[test]
    fn a_write_that_takes_nothing_and_calls_it_success_fails_the_emit() {
        // `Ok(0)` is the loop that never ends: the slice does not shrink, so a
        // `while` over it would ask again for ever.
        let mut terminal = Scripted::new([Answer::Takes(2), Answer::Takes(0)]);

        let failure = terminal
            .emit(VECTOR)
            .expect_err("a write that moved nothing");

        let Emit::Partial { delivered, cause } = failure else {
            panic!("a zero write after two accepted bytes was not partial: {failure:?}");
        };
        assert_eq!(delivered, 2);
        assert_eq!(cause.kind(), io::ErrorKind::WriteZero);
        assert_eq!(terminal.calls, 2, "the emit asked again after a zero write");
    }

    #[test]
    fn a_zero_write_before_any_progress_is_zero_progress() {
        let mut terminal = Scripted::new([Answer::Takes(0)]);

        let failure = terminal
            .emit(VECTOR)
            .expect_err("a write that moved nothing");

        let Emit::ZeroProgress(cause) = failure else {
            panic!("a first zero write reported progress: {failure:?}");
        };
        assert_eq!(cause.kind(), io::ErrorKind::WriteZero);
        assert_eq!(terminal.calls, 1);
    }

    #[test]
    fn a_first_syscall_that_claims_more_than_it_was_offered_delivers_nothing() {
        // A count larger than the slice is a broken wrapper, not a terminal,
        // and the one thing an emit may never do with it is *believe* it. A
        // capped claim would end the loop and report `Ok` -- a receipt saying
        // the whole vector reached the terminal, minted from the one answer
        // that is known to be false.
        let mut terminal = Scripted::new([Answer::Claims(VECTOR.len() + 64)]);

        let failure = terminal
            .emit(VECTOR)
            .expect_err("an impossible count was reported as a delivery");

        let Emit::ZeroProgress(cause) = failure else {
            panic!("an unbelievable count was counted as progress: {failure:?}");
        };
        assert_eq!(cause.kind(), io::ErrorKind::InvalidData, "{cause}");
        assert!(terminal.taken.is_empty());
        assert_eq!(terminal.calls, 1, "the emit kept writing past the vector");
    }

    #[test]
    fn an_impossible_count_after_a_prefix_reports_the_prefix_and_not_the_claim() {
        // The other side of the same refusal, and the one that decides what a
        // caller is told: four bytes were accountably taken, then the wrapper
        // said something impossible. The failure carries the **four** -- the
        // last count this process can stand behind -- and never the claim.
        let mut terminal = Scripted::new([Answer::Takes(4), Answer::Claims(VECTOR.len())]);

        let failure = terminal
            .emit(VECTOR)
            .expect_err("an impossible count was reported as a delivery");

        let Emit::Partial { delivered, cause } = failure else {
            panic!("a prefix followed by an impossible count was not partial: {failure:?}");
        };
        assert_eq!(delivered, 4, "the wrapper's own claim was believed");
        assert_eq!(cause.kind(), io::ErrorKind::InvalidData, "{cause}");
        assert_eq!(terminal.taken, b"0123");
        assert_eq!(terminal.calls, 2, "the emit asked again after the claim");
    }

    #[test]
    fn a_partial_failure_reports_what_was_accepted_and_claims_nothing_else() {
        let err = Emit::Partial {
            delivered: 37,
            cause: broke(),
        }
        .into_error();

        let text = err.to_string();
        assert!(
            text.contains("37"),
            "the failure does not say how much was accepted: {text}"
        );
        assert!(
            !text.contains("restored"),
            "a partial write claimed a restoration: {text}"
        );
        assert_eq!(
            err.kind(),
            broke().kind(),
            "the failure changed kind: {err}"
        );
        let cause = err
            .get_ref()
            .and_then(std::error::Error::source)
            .and_then(|cause| cause.downcast_ref::<io::Error>())
            .expect("the partial failure dropped the cause it was given");
        assert_eq!(
            cause.raw_os_error(),
            Some(libc::EIO),
            "the cause stopped being the cause: {cause}"
        );
    }

    #[test]
    fn a_refusal_and_a_refused_syscall_reach_the_caller_as_themselves() {
        // Both take the budget road, and the callers that read `kind` or the
        // checker's own words are reading the error this conversion produced.
        let rejected = Emit::Rejected(io::Error::other("the output check refused this vector"))
            .into_error()
            .to_string();
        assert_eq!(rejected, "the output check refused this vector");

        let zero =
            Emit::ZeroProgress(io::Error::new(io::ErrorKind::BrokenPipe, "gone")).into_error();
        assert_eq!(zero.kind(), io::ErrorKind::BrokenPipe);
        assert_eq!(zero.to_string(), "gone");
    }
}
