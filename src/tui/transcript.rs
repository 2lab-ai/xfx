//! The transcript: text in, rows the terminal's own document keeps.
//!
//! xfx does not own a transcript viewport. Everything above the band's divider
//! is the terminal's document, and a row this module hands over is written
//! there **once** -- scrolled in with a literal newline so that when it leaves
//! the top of the screen it is in the terminal's native scrollback, where the
//! user's wheel and the user's `less` can still reach it, and where it stays
//! after xfx exits. Nothing here ever rewrites one -- **including on a
//! resize**: a terminal that changed size re-wrapped its own document by rules
//! xfx does not model, and the one text this module still holds is the
//! unfinished line ([`Transcript::resize_unfinished`]).
//!
//! That is what the state here is for. A stream of text does not arrive one
//! finished line at a time: a delta can be three characters that lengthen a row
//! already on the screen, or a hundred that wrap it onto four more. So the
//! module keeps the **unfinished line** -- the tail -- and how many rows of the
//! screen it currently occupies, and answers every landed operation with an
//! [`Append`]: how many rows to scroll in, and the rows to write. A tail that
//! grew without wrapping scrolls nothing and is simply written again a little
//! longer.
//!
//! Once a line is finished it is gone from here. There is nothing to remember
//! about it, because nothing will ever repaint it.
//!
//! # Text is held as text until it lands
//!
//! What is queued here is the **raw logical operation** -- this text was added,
//! this line ended -- and not the rows it makes ([`Op`]). Rows are measured
//! from the committed tail at the width the screen has *at the moment the
//! terminal is offered them*, by the one call that offers them
//! ([`Transcript::emit_front`]), and the state moves only for an operation the
//! terminal really took.
//!
//! The alternative -- measuring at enqueue time, which is what this module did
//! until the width could change underneath -- freezes a row string against a
//! screen that no longer exists. A refused write is retried on the next tick,
//! and a terminal that narrowed in between gets rows wrapped for the wider
//! screen: the painter clips them to the columns there are now
//! (`super::frame`'s `clip`), this phase never repaints a document row, and
//! what fell off the end is gone from the session rather than from the frame.
//! Text that the terminal has not seen is not the terminal's, so it is kept as
//! text.

use std::collections::VecDeque;

use super::wrap;

/// What one push owes the terminal's document.
///
/// `scroll` is how many rows the screen must move up to make room; `rows` is
/// the whole of the unfinished line as it now stands, top first, to be written
/// on the `rows.len()` document rows immediately above the divider. The two are
/// different numbers on purpose: a push that only lengthened the last row
/// scrolls nothing and rewrites one row, and a push that wrapped it scrolls one
/// and rewrites both.
///
/// **`scroll` is a `usize`, and counting rows in a `u16` anywhere on this path
/// is a bug.** A row count is bounded by the *text*, not by the screen: one
/// 8 MiB composer submission (`editor::MAX_COMPOSER_BYTES`) wrapped on a narrow
/// terminal is well past 65535 rows. A count that saturated there would leave
/// `scroll` smaller than the rows it describes, and the renderer -- which
/// derives "already on the screen" from `rows.len() - scroll` -- would treat
/// the difference as settled and never paint it. That is silent, permanent
/// loss of the beginning of an answer, the same class as a batch scroll that
/// outruns the document area.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Append {
    pub(crate) scroll: usize,
    pub(crate) rows: Vec<String>,
}

impl Append {
    /// An append that asks the terminal for nothing.
    fn nothing() -> Self {
        Self {
            scroll: 0,
            rows: Vec::new(),
        }
    }

    /// Whether this append would move the terminal at all.
    ///
    /// The guard is not tidiness: an append that scrolls nothing and writes no
    /// rows would still cost a write, a preflight check and the frame that has
    /// to follow a scroll -- and a frame the band did not need is a repaint of
    /// the whole band on a link that may be a serial line. It is also what
    /// makes a state transition free: ending a line that is already on the
    /// screen changes what this module holds and asks the terminal for
    /// nothing.
    fn is_nothing(&self) -> bool {
        self.scroll == 0 && self.rows.is_empty()
    }
}

/// What the terminal did with the one append it was offered.
///
/// The three answers are the three that are really different to the queue, and
/// they are spelled as an outcome rather than as an `io::Result` because the
/// decision they drive is this module's: whether the operation that produced
/// the append is done with, still owed, or must never be offered again.
///
/// * [`Landed::All`] -- every byte reached the terminal. The operation is
///   committed and dropped.
/// * [`Landed::None`] -- the write moved **no** bytes (a refusal, or a
///   descriptor that took zero). The terminal is exactly as it was, so the
///   operation stays queued, in front of everything queued since, and is
///   measured again -- at whatever width the screen has then -- on the next
///   attempt.
/// * [`Landed::Prefix`] -- part of the vector is on the terminal and the rest
///   is not. There is nothing this module can write that is known to complete
///   it: replaying the operation would put the prefix down twice, and adopting
///   its state would claim rows the terminal never took. So the operation is
///   dropped **without** its state being adopted, the error is handed back, and
///   the session ends on it (`super::event_loop`'s `disposed`).
#[derive(Debug)]
pub(crate) enum Landed<E> {
    All,
    None(E),
    Prefix(E),
}

/// How many rows of one unfinished line this module keeps in hand.
///
/// The bound that makes a streamed answer affordable, and it exists because
/// **only the composer had one**: `editor::MAX_COMPOSER_BYTES` caps what a
/// person can type, and nothing capped what a provider can send without a line
/// break in it. Every push re-wraps the tail and allocates a `String` for each
/// of its rows, so a line that grows to N bytes costs N-squared to display --
/// and `super::pacer` makes that worse by a constant of two hundred, because it
/// turns one delta into a push every eight milliseconds.
///
/// Generous rather than tight: this many rows is more than a screen holds
/// several times over, so the freeze is reached only by text that is really
/// unbroken, and the work one push can do is bounded at this many rows however
/// long the answer runs.
const MAX_TAIL_ROWS: usize = 256;

/// One thing the document was told, in the shape it was told it.
///
/// **Logical rather than visual, and that is the whole point of the queue.** A
/// `Push` carries the text itself -- normalized, and with a carried CRLF
/// already resolved, because both of those are answers about the *stream* and
/// the stream is gone by the time the rows are measured. Nothing here knows how
/// wide the screen is; that question is asked once, at the moment the terminal
/// is offered the rows ([`Transcript::emit_front`]).
enum Op {
    /// This text arrived. Never empty: an operation that adds no bytes would
    /// ask the terminal for nothing and is not queued at all.
    Push(String),
    /// The line ended. Carries no bytes of its own and is queued anyway,
    /// because it is a **state transition**: the line it closes stops being
    /// rewritable, and the text after it starts a row of its own.
    EndLine,
}

/// The unfinished line, what the screen already shows of it, and the text that
/// has not been offered to the screen at all.
pub(crate) struct Transcript {
    /// The screen's width, which is what the rows are wrapped to.
    ///
    /// Moved only by [`Transcript::resize_unfinished`]: the rows already handed
    /// over were written at the width they were written at, and the terminal
    /// owns them now. What this width decides is how the **unfinished** line
    /// and every operation still queued wrap when they are next measured.
    cols: u16,
    /// The line that has not ended yet, **as of the last operation the terminal
    /// took**. Never holds a line break: the breaks are what a push splits on.
    tail: String,
    /// How many rows of the screen the tail occupies **now** -- that is, how
    /// many rows an append that landed already wrote and the next one may write
    /// over. Not the same as the number of rows the tail's text wraps to: after
    /// a line ends the tail is empty and occupies nothing, and a wrap of an
    /// empty string is still one row.
    ///
    /// A `usize` for the reason [`Append::scroll`] is one: the tail's rows are
    /// bounded by the text, not by the screen.
    painted: usize,
    /// The operations the document has been told about and the terminal has not
    /// been given, oldest first.
    ///
    /// A queue rather than one operation, because two deltas can arrive between
    /// two frames and their scrolls do not merge: the second one is measured
    /// against a screen the first one has already moved. Raw text rather than
    /// rows, because the width it will be written at is not known until it is
    /// written.
    queue: VecDeque<Op>,
    /// Whether a line is open once **everything queued** has landed.
    ///
    /// The committed answer to that question is `painted > 0`, and it is the
    /// wrong one to ask at enqueue time: a caller deciding whether to end a
    /// line ([`super::shell::Shell`]'s `finish_document_line`) is deciding
    /// about the document as it will be after the text it has already queued,
    /// not as the terminal has it so far. Read the committed one there and a
    /// notice queued behind an unfinished answer grows a blank row, or loses
    /// the break that keeps it off the end of a sentence.
    ///
    /// Maintained at enqueue, and it is a projection rather than a second
    /// source of truth: applying the queue in order moves `painted` to exactly
    /// what this says, so an empty queue means the two agree. The one exception
    /// is a [`Landed::Prefix`], which drops an operation whose state was never
    /// adopted. That disagreement is bounded by what a session does after a
    /// torn write rather than by nobody reading it: the error is fatal, so
    /// there is **no later normal output or adoption** -- the loop stops
    /// painting and comes down. The shell's own drain still runs and still
    /// applies events (`Shell::apply`), so this field may still be read and
    /// written; what it can no longer do is decide a row the terminal is
    /// given.
    open: bool,
    /// Whether the last non-empty push ended on a carriage return.
    ///
    /// A CRLF that arrives in two pieces would otherwise be two line breaks:
    /// the CR becomes one here, and the LF that opens the next push would be
    /// another. The flag makes the pair one break whichever read they were
    /// split across, which is the same promise [`normalize`] makes inside one.
    ///
    /// The promise it is really keeping is stronger and is what the tests
    /// state: **a push is invariant under chunking.** `push(a); push(b)` writes
    /// what `push(a + b)` writes, for every place a stream could be cut. That
    /// is why the flag is consulted against the *raw* next chunk and why an
    /// empty push does not disturb it.
    split_crlf: bool,
}

impl Transcript {
    pub(crate) fn new(cols: u16) -> Self {
        Self {
            cols,
            tail: String::new(),
            painted: 0,
            queue: VecDeque::new(),
            open: false,
            split_crlf: false,
        }
    }

    /// Queues `text` for the document, and says whether it asks for anything.
    ///
    /// A line break inside `text` finishes the line before it, exactly as
    /// [`queue_end_line`](Self::queue_end_line) does, and the part after it
    /// becomes the new tail; one push may therefore finish several lines, and
    /// the [`Append`] it eventually makes covers all of them -- `rows` is every
    /// row from the first one it changes down to the last one it writes.
    ///
    /// `false` when the push asks the terminal for nothing, which is what the
    /// caller's "does this owe a frame" is: an empty delta, or one whose only
    /// byte was the second half of a CRLF the last one already answered. The
    /// **stream** questions are settled here rather than at emit time, because
    /// they are answers about the order the bytes arrived in and nothing later
    /// can reconstruct that.
    pub(crate) fn queue_push(&mut self, text: &str) -> bool {
        // A push with nothing in it is a chunk boundary and nothing else. It
        // must be **transparent**: clearing the carry here would turn the CR
        // that ended the last chunk into a break of its own, and the LF that
        // opens the chunk after this one into a second.
        if text.is_empty() {
            return false;
        }
        let normalized = normalize(text);
        // Decided against the **raw** chunk, not the normalized one. Only a
        // chunk that really begins with an LF is the other half of the CR that
        // ended the last one; a chunk beginning with another CR is a break of
        // its own, and stripping the newline `normalize` just made of it would
        // swallow it. The invariant both halves of this serve:
        // `push(a); push(b)` writes what `push(a + b)` writes, wherever the
        // stream was cut.
        let carried = self.split_crlf && text.starts_with('\n');
        self.split_crlf = text.ends_with('\r');
        let mut body = normalized.as_str();
        if carried {
            body = body.strip_prefix('\n').unwrap_or(body);
        }
        if body.is_empty() {
            return false;
        }
        self.queue.push_back(Op::Push(body.to_string()));
        // Every push leaves a line open, including one that ends on a break:
        // the row the next text starts on is the line this push opened, and it
        // is a row of the screen as soon as the push lands.
        self.open = true;
        true
    }

    /// Queues the end of the current line, and says whether it asks for a row.
    ///
    /// Usually it asks for nothing: the line is already on the screen exactly
    /// as it stands, and all that changes is that this module stops holding it.
    /// The exception is a line with nothing on it -- two breaks in a row --
    /// which has no row of its own yet and gets one, because a blank line in an
    /// answer is a blank line on the screen.
    ///
    /// Queued **either way**. A line that ends without a row of its own still
    /// ends: the text after it may not be written onto the row this one is on,
    /// and the rows it leaves behind are the terminal's from that moment. An
    /// end that wrote nothing and was therefore not recorded would put the next
    /// answer on the end of the last one's last line.
    // Task 10's submit, and Task 12's end-of-turn, are the callers. It is not
    // folded into `queue_push("\n")` because ending a line is what a *caller*
    // knows and a byte in a stream is not: a turn ends without a trailing
    // newline in the text.
    pub(crate) fn queue_end_line(&mut self) -> bool {
        // A CR that ended the last push has been answered by this break.
        self.split_crlf = false;
        let blank_row = !self.open;
        self.queue.push_back(Op::EndLine);
        self.open = false;
        blank_row
    }

    /// Whether a line is open once everything queued has landed.
    // `super::shell::Shell::finish_document_line` is the caller: it is how
    // "end the line" is told apart from "leave a blank row", and it is asked
    // about the document the queued text is going to make rather than about the
    // rows the terminal has so far.
    pub(crate) fn open_line(&self) -> bool {
        self.open
    }

    /// Whether the document is owed anything at all.
    // Asked without taking anything, by the frame that would hand the terminal
    // to a question on the other buffer, where a document row cannot be written
    // (`super::event_loop`'s `commit_frame`). A queued operation that will write
    // no bytes still counts: it is drained by the same call that would have
    // written one, which costs the barrier nothing and keeps this answer a
    // property of the queue rather than of a materialization nobody asked for.
    pub(crate) fn owes(&self) -> bool {
        !self.queue.is_empty()
    }

    /// Offers the **oldest** queued operation to the terminal, at the width the
    /// screen has now, and keeps exactly what the terminal took.
    ///
    /// `None` when nothing is owed. Otherwise `emit` is called at most once,
    /// with the rows that operation makes *from the committed tail at the
    /// current width* -- and the state moves only on [`Landed::All`], so a
    /// refusal leaves this module exactly as it was and the next attempt
    /// measures the same text against the screen it finds then. That is the
    /// whole repair: rows are a function of the width they are written at, and
    /// the only moment that width is knowable is the moment of the write.
    ///
    /// **One operation per call, and synchronously.** The candidate never
    /// escapes the borrow, and what that buys is exactly this: the **logical**
    /// resize and `/clear` -- the two things that change what a row's width
    /// means -- reach this module through `&mut` calls
    /// ([`Self::resize_unfinished`], a fresh [`Transcript`]), and neither can
    /// run between the measurement and the write while this borrow is held.
    ///
    /// It is **not** a claim that no signal is delivered meanwhile. A handler
    /// may run at any instant; what none of them does is touch this state --
    /// a `SIGWINCH` records the fact in an atomic and pokes a pipe
    /// (`super::signals`'s `flag_winch`), and the resize it stands for is
    /// applied later by the loop, through the same `&mut` road. A whole-queue
    /// plan would have no such window to rely on: it would be either recomputed
    /// after every one of those or wrong, and "recompute the plan" is the bug
    /// this replaced wearing a larger hat.
    ///
    /// An operation that asks the terminal for nothing -- ending a line that is
    /// already on the screen -- commits without calling `emit` at all. It is a
    /// state transition, not a write, and offering it would cost a preflight
    /// check and a frame for a vector with no bytes in it.
    pub(crate) fn emit_front<E>(
        &mut self,
        emit: impl FnOnce(&Append) -> Landed<E>,
    ) -> Option<Result<(), E>> {
        let prepared = self.prepare_front()?;
        if prepared.append.is_nothing() {
            self.commit(prepared);
            return Some(Ok(()));
        }
        match emit(&prepared.append) {
            Landed::All => {
                self.commit(prepared);
                Some(Ok(()))
            }
            // Nothing reached the terminal, so nothing here may move: the
            // operation is still owed and the candidate built for a screen that
            // refused it is simply dropped.
            Landed::None(err) => Some(Err(err)),
            // Some of it reached the terminal and the rest did not. The
            // operation is dropped **and its state is not adopted**: the tail
            // this module keeps describes rows that landed, and half a vector
            // landed. Replaying it would write the prefix twice.
            Landed::Prefix(err) => {
                self.queue.pop_front();
                Some(Err(err))
            }
        }
    }

    /// The candidate the front operation makes right now, or `None` when the
    /// queue is empty.
    fn prepare_front(&self) -> Option<Prepared> {
        match self.queue.front()? {
            Op::Push(body) => Some(self.prepare_push(body)),
            Op::EndLine => Some(self.prepare_end_line()),
        }
    }

    /// What a push would write, and the tail it would leave.
    ///
    /// Built beside the committed tail rather than in it, so a write that is
    /// refused leaves nothing half-applied.
    ///
    /// **What that costs, stated exactly: one clone of the committed tail per
    /// attempt.** The tail is the only thing copied, and [`freeze`] bounds it
    /// at [`MAX_TAIL_ROWS`] rows of the current width, so the clone is bounded
    /// by that and not by the answer's length or by how much is queued behind
    /// it. An attempt is a call, so a write refused three times pays it three
    /// times -- which is the price of measuring against the screen that is
    /// really there. It is **not** claimed to be equal to what the old shape
    /// cost, and nothing here is a timing or complexity guarantee: the wall
    /// clock belongs to `scripts/check-tui-preflight-cost.sh`, and what the
    /// cases in this module pin is the number of rows an operation produces.
    fn prepare_push(&self, body: &str) -> Prepared {
        let painted = self.painted;
        let mut tail = self.tail.clone();
        let mut rows = Vec::new();
        let mut segments = body.split('\n');
        // `split` yields the text itself when there is no break in it, so the
        // first segment always exists and always joins the tail.
        tail.push_str(segments.next().unwrap_or_default());
        for next in segments {
            // Everything before the break is a finished line. Its rows are
            // written once, here, and never again.
            rows.append(&mut texts(&tail, self.cols));
            tail.clear();
            tail.push_str(next);
        }
        let mut last = texts(&tail, self.cols);
        let mut open_rows = last.len();
        rows.append(&mut last);
        // After the append is built, because what is frozen is what the *next*
        // push may no longer rewrite; this one has already said what it owes.
        freeze(&mut tail, &mut open_rows, self.cols);

        Prepared {
            // The rows already on the screen are the first `painted` of these
            // -- the old tail is a prefix of the text they came from -- so they
            // are rewritten where they are and everything past them is new.
            append: Append {
                scroll: rows.len().saturating_sub(painted),
                rows,
            },
            tail,
            painted: open_rows,
        }
    }

    /// What ending the line would write, and the empty tail it would leave.
    fn prepare_end_line(&self) -> Prepared {
        let rows = texts(&self.tail, self.cols);
        let scroll = rows.len().saturating_sub(self.painted);
        Prepared {
            append: if scroll == 0 {
                Append::nothing()
            } else {
                Append { scroll, rows }
            },
            tail: String::new(),
            painted: 0,
        }
    }

    /// Adopts a candidate the terminal took, and drops the operation it came
    /// from.
    fn commit(&mut self, prepared: Prepared) {
        self.queue.pop_front();
        self.tail = prepared.tail;
        self.painted = prepared.painted;
    }

    /// Re-wraps the unfinished line for a screen that changed width.
    ///
    /// **The only thing on this side of the divider a resize may touch**, and
    /// the boundary is what the whole module is built on. Every finished line
    /// was written into the terminal's own document once and is in its native
    /// scrollback now, where the terminal re-wrapped it by rules xfx does not
    /// model and no repaint of this phase's could reach it. The tail is the one
    /// text still held here, and the rows it occupies are what the **next**
    /// append measures its scroll against: left at the old width
    /// they would be too few after a narrowing -- the renderer would treat rows
    /// nobody painted as settled, which is the silent loss this path counts in
    /// `usize` to avoid -- and too many after a widening, which scrolls a row
    /// that is already on the screen.
    ///
    /// The width it records is also the width every **queued** operation is
    /// measured at from here on ([`Self::emit_front`]). Text the terminal has
    /// not been given is not the terminal's, so a resize reaches it: that is
    /// the difference between this module's queue and the rows it has already
    /// handed over, and it is the whole of the repair.
    ///
    /// A width that did not change is left alone rather than re-measured: a
    /// `SIGWINCH` for a font change, or one that moved only the row count, is
    /// a tail exactly where it was.
    pub(crate) fn resize_unfinished(&mut self, cols: u16) {
        if cols == self.cols {
            return;
        }
        self.cols = cols;
        // Asked only of a tail that is really on the screen. A wrap of an empty
        // string is still one row, so a transcript holding nothing would
        // otherwise claim a document row that is not its own -- and the next
        // push would rewrite it.
        if self.painted == 0 {
            return;
        }
        self.painted = wrap::wrap(&self.tail, cols).len();
    }
}

/// Stops holding the rows of an unfinished line that can no longer change.
///
/// **Not a truncation, and the difference is a property of greedy wrapping.** A
/// row's break is decided by the first cluster that crosses the margin, and that
/// cluster is on the row *after* it -- so every row of a wrapped text except the
/// last is already settled, and appending to the text cannot move it. Those rows
/// are on the screen, this phase never repaints a document row, and nothing
/// would ever have rewritten them: dropping them from the tail changes what this
/// module *holds* and not what the terminal shows.
/// `the_rows_the_document_gets_are_the_ones_a_single_wrap_would_give` is that
/// claim, checked against one unbroken wrap of the whole text.
///
/// The alternative -- ending the line early -- was rejected: a break the
/// provider did not write lands in the middle of a row, and the answer grows a
/// short line every few thousand characters.
fn freeze(tail: &mut String, open_rows: &mut usize, cols: u16) {
    if *open_rows <= MAX_TAIL_ROWS {
        return;
    }
    let rows = wrap::wrap(tail, cols);
    let Some(last) = rows.last() else {
        return;
    };
    let kept = last.start;
    if kept == 0 {
        return;
    }
    tail.drain(..kept);
    // Asked rather than assumed to be one: the answer is what the *next*
    // append measures its scroll against, and a count larger than the rows the
    // tail really occupies would make that scroll too small -- which is the
    // renderer treating unpainted rows as settled, the silent loss this whole
    // path is counted in `usize` to avoid.
    *open_rows = wrap::wrap(tail, cols).len();
}

/// `text`, wrapped, as the strings an append writes.
fn texts(text: &str, cols: u16) -> Vec<String> {
    wrap::wrap(text, cols)
        .into_iter()
        .map(|row| text[row.start..row.end].to_string())
        .collect()
}

/// The one-shot shape this module had before the width could change under a
/// write, kept for the cases that are about what a text *means* rather than
/// about when it lands.
///
/// **Not production, and it may not become production.** Nothing in a session
/// may queue text and land it in the same breath: the width it lands at is the
/// loop's answer at the moment of the write ([`Transcript::emit_front`]), and a
/// caller that took both decisions is the bug this module was rebuilt to make
/// unrepresentable. What the cases here ask -- and the two other modules'
/// (`super::bridge`, `super::frame`) that drive a transcript against a real
/// band -- is which rows a given text makes at a given width, which is one call
/// either way.
#[cfg(test)]
impl Transcript {
    /// Queues `text` and lands it at once, answering with what it wrote.
    pub(crate) fn push(&mut self, text: &str) -> Append {
        self.land_own(|transcript| {
            transcript.queue_push(text);
        })
    }

    /// Ends the current line and lands that, answering with what it wrote.
    pub(crate) fn end_line(&mut self) -> Append {
        self.land_own(Self::queue_end_line_ignored)
    }

    /// How many rows of the screen the unfinished line occupies.
    pub(crate) fn tail_rows(&self) -> usize {
        self.painted
    }

    /// [`Self::queue_end_line`] with its answer dropped, so it has the shape
    /// [`Self::land_own`] takes.
    fn queue_end_line_ignored(&mut self) {
        self.queue_end_line();
    }

    /// Queues **one** operation and lands exactly what that operation wrote.
    ///
    /// The guards are what keep this fixture from quietly becoming a different
    /// thing than the one-shot call it replaces:
    ///
    /// * The queue must be **empty on entry**. A helper that landed whatever
    ///   was already waiting would let a case queue three operations, ask for
    ///   one, and be handed a fourth's worth of rows -- an aggregation the
    ///   production path cannot do and a legacy assertion would not notice.
    /// * `enqueue` may queue **one operation or none**, and none is a real
    ///   answer rather than a failure: an empty push, and one whose only byte
    ///   was the second half of a CRLF, both ask the terminal for nothing and
    ///   are deliberately not queued ([`Self::queue_push`]). Asserting "one is
    ///   queued" would delete exactly the cases that exist to check that.
    /// * So at most one operation is landed, and the append handed back is that
    ///   operation's own -- [`Append::nothing`] when there was none, or when
    ///   the one operation asked the terminal for nothing.
    fn land_own(&mut self, enqueue: impl FnOnce(&mut Self)) -> Append {
        debug_assert!(
            self.queue.is_empty(),
            "a one-shot fixture was used on a transcript that is already \
             holding {} operation(s): it would land them too",
            self.queue.len()
        );
        enqueue(self);
        debug_assert!(
            self.queue.len() <= 1,
            "one call queued {} operations",
            self.queue.len()
        );
        let mut written = Append::nothing();
        while self
            .emit_front(|append| {
                written = append.clone();
                Landed::<std::convert::Infallible>::All
            })
            .is_some()
        {}
        written
    }
}

/// One candidate write: the rows the front operation makes at the width the
/// screen has now, and the state that becomes true if the terminal takes them.
///
/// Private, and it never leaves [`Transcript::emit_front`]'s borrow. A caller
/// holding one of these could hold it across a resize or a `/clear` and then
/// commit rows measured against a screen that is gone -- which is the failure
/// this whole shape exists to make unrepresentable, not one to reintroduce at
/// the seam.
struct Prepared {
    append: Append,
    tail: String,
    painted: usize,
}

/// `text` with every line break spelled the one way the document accepts.
///
/// A CRLF becomes an LF and a bare CR becomes one too (`frame_scroll_plan.zig:8-12`).
/// The document only ever receives LFs because that is the only byte that
/// scrolls it: a bare CR would rewind the terminal's cursor to the first column
/// of a row this module believes it has already finished writing, and the next
/// row placed would overwrite it.
pub(crate) fn normalize(text: &str) -> String {
    if !text.contains('\r') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        if character != '\r' {
            out.push(character);
            continue;
        }
        // A CR and the LF that follows it are one break, not two.
        if characters.peek() == Some(&'\n') {
            characters.next();
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_fragment_takes_one_row_of_the_screen() {
        let mut transcript = Transcript::new(80);
        assert_eq!(
            transcript.push("answer"),
            Append {
                scroll: 1,
                rows: vec!["answer".to_string()]
            }
        );
    }

    #[test]
    fn a_fragment_that_fits_the_tail_repaints_it_without_scrolling() {
        let mut transcript = Transcript::new(80);
        transcript.push("ans");
        assert_eq!(
            transcript.push("wer"),
            Append {
                scroll: 0,
                rows: vec!["answer".to_string()]
            }
        );
    }

    #[test]
    fn a_fragment_that_wraps_the_tail_scrolls_by_exactly_the_rows_it_added() {
        let mut transcript = Transcript::new(4);
        transcript.push("abcd");
        assert_eq!(
            transcript.push("efgh"),
            Append {
                scroll: 1,
                rows: vec!["abcd".to_string(), "efgh".to_string()]
            }
        );
    }

    #[test]
    fn a_finished_line_is_left_in_the_document_and_the_next_one_starts_fresh() {
        let mut transcript = Transcript::new(80);
        transcript.push("first");
        transcript.end_line();
        assert_eq!(transcript.tail_rows(), 0);
        assert_eq!(
            transcript.push("second"),
            Append {
                scroll: 1,
                rows: vec!["second".to_string()]
            }
        );
    }

    #[test]
    fn carriage_returns_are_normalized_so_a_row_cannot_overwrite_itself() {
        // frame_scroll_plan.zig:8-12 -- the document only ever receives
        // CR-before-LF bytes, and a bare CR would rewind the terminal's cursor
        // over a row this module believes it has already written.
        assert_eq!(normalize("a\r\nb"), "a\nb");
        assert_eq!(normalize("a\rb"), "a\nb");
        assert_eq!(normalize("plain"), "plain");
    }

    #[test]
    fn a_line_that_is_already_on_the_screen_is_finished_without_a_write() {
        // The rows are the terminal's now. Rewriting them would cost a scroll
        // that pushes a blank row into the document.
        let mut transcript = Transcript::new(80);
        transcript.push("first");
        assert_eq!(transcript.end_line(), Append::nothing());
    }

    #[test]
    fn a_break_inside_a_push_finishes_the_line_before_it() {
        let mut transcript = Transcript::new(80);
        assert_eq!(
            transcript.push("first\nsecond"),
            Append {
                scroll: 2,
                rows: vec!["first".to_string(), "second".to_string()]
            }
        );
        assert_eq!(
            transcript.tail_rows(),
            1,
            "only the unfinished line is still this module's"
        );
        // and the finished one is not rewritten by what follows
        assert_eq!(
            transcript.push("!"),
            Append {
                scroll: 0,
                rows: vec!["second!".to_string()]
            }
        );
    }

    #[test]
    fn a_blank_line_between_two_answers_takes_a_row_of_its_own() {
        // Two breaks in a row. Without a row for the empty line between them
        // the paragraph break the model wrote disappears.
        let mut transcript = Transcript::new(80);
        assert_eq!(
            transcript.push("a\n\nb"),
            Append {
                scroll: 3,
                rows: vec!["a".to_string(), String::new(), "b".to_string()]
            }
        );
    }

    /// The document a sequence of appends leaves behind, replayed exactly as
    /// `frame::render_append` applies one: scroll by `scroll`, then write
    /// `rows` onto the last `rows.len()` lines of what is there.
    fn replay(appends: impl IntoIterator<Item = Append>) -> Vec<String> {
        let mut document: Vec<String> = Vec::new();
        for append in appends {
            let kept = (document.len() + append.scroll).saturating_sub(append.rows.len());
            document.truncate(kept);
            document.extend(append.rows);
        }
        document
    }

    /// The document a sequence of pushes leaves behind when each one lands
    /// before the next arrives.
    fn document(cols: u16, chunks: &[&str]) -> Vec<String> {
        let mut transcript = Transcript::new(cols);
        replay(chunks.iter().map(|chunk| transcript.push(chunk)))
    }

    /// The same sequence, queued in full and landed afterwards -- the shape a
    /// session that could not write for a few ticks really has.
    fn queued_document(cols: u16, chunks: &[&str]) -> Vec<String> {
        let mut transcript = Transcript::new(cols);
        for chunk in chunks {
            transcript.queue_push(chunk);
        }
        replay(drain(&mut transcript))
    }

    /// Everything queued, offered to a terminal that takes all of it, as the
    /// appends it was offered.
    ///
    /// What the loop's drain does (`super::super::event_loop`'s
    /// `commit_document`) with a screen that never refuses.
    fn drain(transcript: &mut Transcript) -> Vec<Append> {
        let mut offered = Vec::new();
        while transcript
            .emit_front(|append| {
                offered.push(append.clone());
                Landed::<std::convert::Infallible>::All
            })
            .is_some()
        {}
        offered
    }

    #[test]
    fn a_push_is_invariant_under_chunking() {
        // The property the carry exists for. A stream is cut wherever the
        // socket cut it, so every split of the same bytes must leave the same
        // document -- and the CR cases are the ones where a naive carry gets it
        // wrong in *both* directions: swallowing a break that was really there,
        // or inventing one that was not.
        for stream in [
            "a\r\nb",
            "a\r\rb",
            "a\r\r\nb",
            "a\rb\r\nc\r",
            "\r\n",
            "\r",
            "one\r\ntwo\r\nthree",
            "no breaks at all",
        ] {
            let whole = document(80, &[stream]);
            for at in 0..=stream.len() {
                if !stream.is_char_boundary(at) {
                    continue;
                }
                let (head, tail) = stream.split_at(at);
                assert_eq!(
                    document(80, &[head, tail]),
                    whole,
                    "{stream:?} split at {at} left a different document"
                );
                // and a chunk boundary that carries no bytes changes nothing
                assert_eq!(
                    document(80, &[head, "", tail]),
                    whole,
                    "{stream:?} split at {at} around an empty push"
                );
            }
        }
    }

    #[test]
    fn a_chunk_that_opens_with_its_own_carriage_return_keeps_its_break() {
        // The carry is not "strip the next newline you see". `a\r` + `\rb` is
        // two breaks: the first chunk's CR and the second chunk's own. Reading
        // the *normalized* chunk cannot tell them apart, because `normalize`
        // has already turned both into LFs.
        assert_eq!(
            document(80, &["a\r", "\rb"]),
            vec!["a".to_string(), String::new(), "b".to_string()]
        );
        assert_eq!(document(80, &["a\r", "\rb"]), document(80, &["a\r\rb"]));
    }

    #[test]
    fn a_chunk_that_opens_with_a_crlf_of_its_own_is_one_break_not_two() {
        // `a\r` + `\r\nb`: the first chunk's CR is one break and the second
        // chunk's CRLF is one more -- not two, which is what dropping
        // `normalize`'s CRLF pairing would give, and not zero, which is what
        // stripping on the carry alone would give.
        assert_eq!(
            document(80, &["a\r", "\r\nb"]),
            vec!["a".to_string(), String::new(), "b".to_string()]
        );
        assert_eq!(document(80, &["a\r", "\r\nb"]), document(80, &["a\r\r\nb"]));
    }

    #[test]
    fn an_empty_push_between_the_halves_of_a_crlf_is_transparent() {
        // A delta that carried no text at all still arrives as a push. If it
        // cleared the carry, the LF that follows becomes a second break and the
        // answer grows a blank line the model never wrote.
        assert_eq!(
            document(80, &["a\r", "", "\nb"]),
            vec!["a".to_string(), "b".to_string()]
        );
        assert_eq!(document(80, &["a\r", "", "\nb"]), document(80, &["a\r\nb"]));
    }

    #[test]
    fn a_crlf_split_across_two_pushes_is_one_line_break() {
        // A stream is cut wherever the socket cut it. A CR that ends one push
        // and an LF that opens the next are the same break, and answering both
        // would put a blank row in the middle of an answer.
        let mut split = Transcript::new(80);
        assert_eq!(
            split.push("a\r"),
            Append {
                scroll: 2,
                rows: vec!["a".to_string(), String::new()]
            },
            "the line and the row its successor starts on"
        );
        assert_eq!(
            split.push("\nb"),
            Append {
                scroll: 0,
                rows: vec!["b".to_string()]
            },
            "the leading newline was answered a second time"
        );

        let mut whole = Transcript::new(80);
        assert_eq!(
            whole.push("a\r\nb"),
            Append {
                scroll: 2,
                rows: vec!["a".to_string(), "b".to_string()]
            }
        );
    }

    #[test]
    fn a_push_with_nothing_in_it_asks_the_terminal_for_nothing() {
        // Not a scroll of one blank row: an empty delta is a delta that said
        // nothing, and the wrap of an empty string is still one row.
        let mut transcript = Transcript::new(80);
        assert_eq!(transcript.push(""), Append::nothing());
        assert_eq!(transcript.tail_rows(), 0);
        transcript.push("text");
        assert_eq!(transcript.push(""), Append::nothing());
        assert_eq!(transcript.tail_rows(), 1);
    }

    #[test]
    fn no_row_of_an_append_carries_a_line_break() {
        // A row that still held one would move the terminal's cursor off the
        // row it was placed on, and every row written after it would land a row
        // too high (`frame::render_append`).
        let mut transcript = Transcript::new(12);
        for text in ["one\r\ntwo", "\rthree\n", "a very long line that wraps\n"] {
            for row in transcript.push(text).rows {
                assert!(
                    !row.contains(['\r', '\n']),
                    "a line break survived into a document row: {row:?}"
                );
            }
        }
    }

    #[test]
    fn a_wrapped_tail_scrolls_once_per_row_it_gained() {
        // The number that matters: scroll by fewer rows than were added and the
        // band paints over the transcript; by more and the document grows blank
        // rows nobody wrote.
        let mut transcript = Transcript::new(4);
        assert_eq!(transcript.push("abcdefghijkl").scroll, 3);
        assert_eq!(transcript.tail_rows(), 3);
        assert_eq!(transcript.push("mnop").scroll, 1);
    }

    #[test]
    fn a_line_with_more_rows_than_a_u16_is_counted_in_full() {
        // The count is a property of the text. `editor::MAX_COMPOSER_BYTES` is
        // 8 MiB, and a submission anywhere near it, echoed here and wrapped on
        // a narrow terminal, is past 65535 rows long before it is unusual.
        // Counting these in a `u16` saturates, and `Append::scroll` then
        // understates the rows it carries -- which the renderer reads as "the
        // difference was already painted" and never paints. This is the count
        // path, at the boundary and past it.
        let boundary = usize::from(u16::MAX);
        for rows in [boundary - 1, boundary, boundary + 1, boundary + 2] {
            let mut transcript = Transcript::new(1);
            let append = transcript.push(&"x".repeat(rows));
            assert_eq!(append.rows.len(), rows, "{rows} rows of text");
            assert_eq!(
                append.scroll, rows,
                "a fresh line of {rows} rows scrolled in fewer than it wrote"
            );
            // The rows this module still *holds* are bounded (`MAX_TAIL_ROWS`)
            // and the rows it *counted* are not, which is the whole of the
            // distinction: the append above carries every one of them.
            assert!(transcript.tail_rows() <= MAX_TAIL_ROWS);
            // and the next push is still measured against them, so the
            // saturation cannot reappear one delta later. Replayed as
            // `frame::render_append` applies an append -- scroll, then write
            // the rows onto the last `rows.len()` lines -- the document holds
            // the whole line and the delta after it, which a `scroll` that
            // understated its rows would have silently cut the front off.
            let mut document = vec!["x".to_string(); rows];
            let after = transcript.push("y");
            assert_eq!(after.scroll, 1, "the delta after {rows} rows");
            let kept = (document.len() + after.scroll).saturating_sub(after.rows.len());
            document.truncate(kept);
            document.extend(after.rows);
            assert_eq!(document.len(), rows + 1, "{rows} rows lost their front");
            assert_eq!(document.concat(), format!("{}y", "x".repeat(rows)));
        }
    }

    #[test]
    fn an_unfinished_line_stops_growing_however_long_it_gets() {
        // The bound Task 7's review ledgered against this task. Only the
        // composer has a byte cap (`editor::MAX_COMPOSER_BYTES`); a provider's
        // line has none, and Task 13's pacer pushes into it every 8 ms. Every
        // push re-wraps the whole tail and allocates a `String` per row of it,
        // so an answer with no line break in it costs the *square* of its own
        // length to display -- 25 GB of work for a megabyte, at which point the
        // UI stops keeping up with the stream it is pacing.
        let mut transcript = Transcript::new(10);
        for _ in 0..400 {
            transcript.push(&"x".repeat(100));
        }
        assert!(
            transcript.tail_rows() <= MAX_TAIL_ROWS,
            "the unfinished line holds {} rows",
            transcript.tail_rows()
        );
    }

    #[test]
    fn the_rows_the_document_gets_are_the_ones_a_single_wrap_would_give() {
        // What makes the bound invisible rather than a truncation: greedy
        // wrapping settles every row but the last, so a row this module stops
        // holding is a row nothing would ever have changed. The claim is the
        // strong one -- the document a bounded transcript builds from a hundred
        // pushes is row for row the document one unbroken wrap of the whole
        // text would give -- and it is checked at a width where the freeze
        // happens many times over.
        let cols = 7;
        let mut text = String::new();
        for index in 0..900 {
            text.push_str(&format!("word{index} "));
        }
        let chunks: Vec<&str> = text
            .as_bytes()
            .chunks(13)
            .map(|chunk| std::str::from_utf8(chunk).expect("ascii"))
            .collect();
        let expected: Vec<String> = wrap::wrap(&text, cols)
            .into_iter()
            .map(|row| text[row.start..row.end].to_string())
            .collect();
        assert!(
            expected.len() > MAX_TAIL_ROWS * 2,
            "the case is too short to freeze anything"
        );
        assert_eq!(document(cols, &chunks), expected);
    }

    #[test]
    fn the_rows_of_an_append_are_the_text_that_was_pushed() {
        // Every character in, every character out, in order -- across wrapping
        // and across breaks. A wrap that dropped a byte would be invisible in
        // the counts above.
        let mut transcript = Transcript::new(7);
        // The document the terminal ends up holding, replayed from the appends
        // exactly as `frame::render_append` applies them: scroll by `scroll`,
        // then write `rows` onto the last `rows.len()` rows of what is there.
        let mut document: Vec<String> = Vec::new();
        for text in ["alpha bra", "vo\ncharlie ", "delta\n", "echo"] {
            let append = transcript.push(text);
            let kept = (document.len() + append.scroll).saturating_sub(append.rows.len());
            document.truncate(kept);
            document.extend(append.rows);
        }
        assert_eq!(
            document,
            vec!["alpha ", "bravo", "charlie ", "delta", "echo"]
        );
        assert_eq!(
            document.join("").replace(' ', ""),
            "alphabravocharliedeltaecho",
            "a character was dropped or repeated between the wrap and the append"
        );
    }

    #[test]
    fn a_wider_screen_rewraps_the_unfinished_tail_and_nothing_else() {
        // The one thing on this side of the divider a resize may touch. Every
        // finished line is in the terminal's own document -- and its own
        // scrollback -- where the terminal re-wrapped it by rules xfx does not
        // model; the tail is the only text this module still holds, and the
        // rows it occupies are what the *next* push measures its scroll
        // against.
        let mut transcript = Transcript::new(10);
        let append = transcript.push("abcdefghijklmno");
        assert_eq!(append.rows, vec!["abcdefghij", "klmno"]);
        assert_eq!(transcript.tail_rows(), 2);

        transcript.resize_unfinished(20);
        assert_eq!(
            transcript.tail_rows(),
            1,
            "the tail still claims the rows it had on the narrow screen, so \
             the next push would scroll a row that is already there"
        );
        assert_eq!(
            transcript.push("pq"),
            Append {
                scroll: 0,
                rows: vec!["abcdefghijklmnopq".to_string()]
            },
            "the tail was not re-wrapped to the screen it is now on"
        );
    }

    #[test]
    fn a_narrower_screen_gives_the_tail_the_rows_it_now_needs() {
        // The other direction, which is the one that loses text when it is
        // wrong: a `painted` smaller than the rows the tail really occupies
        // makes the next append's scroll too small, and the renderer treats
        // rows nobody painted as settled.
        let mut transcript = Transcript::new(20);
        transcript.push("abcdefghijklmnopq");
        assert_eq!(transcript.tail_rows(), 1);

        transcript.resize_unfinished(10);
        assert_eq!(transcript.tail_rows(), 2);
        assert_eq!(
            transcript.push("rs"),
            Append {
                scroll: 0,
                rows: vec!["abcdefghij".to_string(), "klmnopqrs".to_string()]
            },
            "the tail was not re-wrapped to the narrower screen"
        );
    }

    #[test]
    fn a_resize_with_no_unfinished_line_claims_no_row() {
        // A wrap of an empty string is still one row, so a resize that simply
        // re-measured would give a transcript holding nothing a row of the
        // screen -- and the next push would rewrite a document row that is not
        // its own.
        let mut transcript = Transcript::new(20);
        transcript.push("done");
        transcript.end_line();
        assert_eq!(transcript.tail_rows(), 0);
        transcript.resize_unfinished(10);
        assert_eq!(
            transcript.tail_rows(),
            0,
            "a transcript with nothing unfinished took a row on resize"
        );
    }

    #[test]
    fn a_resize_to_the_width_it_already_has_changes_nothing() {
        // A `SIGWINCH` that only changed the row count, or a font change that
        // changed neither: the tail is where it was and re-measuring it would
        // be work for nothing.
        let mut transcript = Transcript::new(10);
        transcript.push("abcdefghijkl");
        let before = transcript.tail_rows();
        transcript.resize_unfinished(10);
        assert_eq!(transcript.tail_rows(), before);
    }

    // -----------------------------------------------------------------------
    // text that has not landed yet
    // -----------------------------------------------------------------------

    #[test]
    fn a_finished_line_that_never_landed_is_wrapped_for_the_narrower_screen() {
        // The repair, at the seam it lives on. A line finished at 80 columns
        // and not yet written is not the terminal's: it has no rows anywhere,
        // so a screen that narrowed before it is written re-wraps it whole. The
        // alternative -- rows frozen at 80 and clipped to 40 by the painter --
        // loses the middle 40 columns of the line from the session, because
        // this phase never repaints a document row.
        let a = "A".repeat(40);
        let b = "B".repeat(40);
        let c = "C".repeat(10);
        let mut transcript = Transcript::new(80);
        assert!(transcript.queue_push(&format!("{a}{b}{c}\n")));

        transcript.resize_unfinished(40);
        let appends = drain(&mut transcript);

        assert_eq!(appends.len(), 1, "one push, one append: {appends:?}");
        assert_eq!(
            appends[0].rows,
            vec![a, b, c, String::new()],
            "the line kept the 80-column wrap it was queued at"
        );
        assert_eq!(
            appends[0].scroll, 4,
            "the scroll has to cover the rows the new width really needs"
        );
    }

    #[test]
    fn a_finished_line_that_never_landed_is_wrapped_for_the_wider_screen_too() {
        // The other direction. It loses nothing, which is why it is the one a
        // fix could forget: the answer simply stays in a 40-column column of a
        // screen that is now twice that, for ever, because nothing repaints it.
        let a = "A".repeat(40);
        let b = "B".repeat(40);
        let c = "C".repeat(10);
        let mut transcript = Transcript::new(40);
        assert!(transcript.queue_push(&format!("{a}{b}{c}\n")));

        transcript.resize_unfinished(80);
        let appends = drain(&mut transcript);

        assert_eq!(appends.len(), 1, "one push, one append: {appends:?}");
        assert_eq!(
            appends[0].rows,
            vec![format!("{a}{b}"), c, String::new()],
            "the 80 columns the screen now has were written as two 40-column \
             rows"
        );
    }

    #[test]
    fn a_write_the_terminal_took_nothing_of_leaves_this_module_exactly_as_it_was() {
        // What makes the re-measurement above reachable at all: a refusal moves
        // no state here, so the operation is still the oldest thing owed and is
        // built again -- against whatever width the next attempt finds -- from
        // the same text.
        let mut transcript = Transcript::new(20);
        transcript.queue_push("abcdefghijklmnopqrstuvwxyz");

        let refused = transcript.emit_front(|_| Landed::None("the screen said no"));
        assert!(matches!(refused, Some(Err(_))), "{refused:?}");
        assert_eq!(
            transcript.tail_rows(),
            0,
            "a refused write moved the rows this module claims are painted"
        );
        assert!(transcript.owes(), "a refused write stopped being owed");

        transcript.resize_unfinished(10);
        let appends = drain(&mut transcript);
        assert_eq!(
            appends[0].rows,
            vec![
                "abcdefghij".to_string(),
                "klmnopqrst".to_string(),
                "uvwxyz".to_string(),
            ],
            "the retry was not measured against the screen it landed on"
        );
    }

    #[test]
    fn a_write_the_terminal_tore_is_neither_replayed_nor_believed() {
        // `Landed::Prefix`: some of the vector is on the screen and some is
        // not. Replaying the operation would put the prefix down twice, and
        // adopting the tail it would have produced would mean this module
        // believes the terminal holds text it may never have received. So the
        // operation is dropped and the state is not moved -- and the session
        // ends on the error, which is why this is the only place the two can
        // disagree.
        let mut transcript = Transcript::new(80);
        transcript.push("abc");
        transcript.queue_push("def");

        let torn = transcript.emit_front(|_| Landed::Prefix("half of it landed"));
        assert!(matches!(torn, Some(Err(_))), "{torn:?}");
        assert!(!transcript.owes(), "the torn write is still owed");

        transcript.queue_push("xyz");
        let appends = drain(&mut transcript);
        assert_eq!(
            appends[0].rows,
            vec!["abcxyz".to_string()],
            "the torn write's text was adopted as if the terminal had taken it, \
             or replayed as if it had not"
        );
    }

    #[test]
    fn a_line_ended_before_its_text_lands_is_still_a_line_of_its_own() {
        // The case a session reaches whenever a notice is written while an
        // answer is waiting for a screen: the answer, the end of its line and
        // the notice are all queued before any of them is written. The end
        // carries no bytes and is queued anyway -- dropped, the notice would be
        // written onto the end of the answer's own row.
        let mut transcript = Transcript::new(80);
        assert!(transcript.queue_push("an answer"));
        assert!(
            !transcript.queue_end_line(),
            "ending a line that has text on it asks for no row of its own"
        );
        assert!(transcript.queue_push("a notice"));

        let appends = drain(&mut transcript);
        assert_eq!(
            appends
                .iter()
                .map(|append| (append.scroll, append.rows.clone()))
                .collect::<Vec<_>>(),
            vec![
                (1, vec!["an answer".to_string()]),
                (1, vec!["a notice".to_string()]),
            ],
            "the end of the line did not reach the terminal as a line break"
        );
    }

    #[test]
    fn whether_a_line_is_open_is_asked_of_the_queue_rather_than_of_the_screen() {
        // The projection `super::shell::Shell::finish_document_line` reads. The
        // committed answer -- how many rows the *landed* tail occupies -- says
        // "no line is open" about an answer that is queued and unwritten, and a
        // caller that believed it would end a line nobody had started and put a
        // blank row in the middle of the document.
        let mut transcript = Transcript::new(80);
        assert!(
            !transcript.open_line(),
            "a fresh transcript has a line open"
        );

        transcript.queue_push("an answer");
        assert!(
            transcript.open_line(),
            "text nobody has written yet leaves no line open"
        );
        assert_eq!(
            transcript.tail_rows(),
            0,
            "the committed side moved before the write did"
        );

        transcript.queue_end_line();
        assert!(
            !transcript.open_line(),
            "the end of the line did not close it"
        );

        // And a push that ends on a break still leaves one open: the row its
        // successor starts on is a row of the screen as soon as it lands.
        transcript.queue_push("a line\n");
        assert!(transcript.open_line());
        assert!(
            !transcript.queue_end_line(),
            "the row the last break opened was counted as a second blank line"
        );
        // Which the drain confirms costs nothing: the blank row is already
        // written by the push that opened it.
        let appends = drain(&mut transcript);
        assert_eq!(
            appends.last().map(|append| append.rows.clone()),
            Some(vec!["a line".to_string(), String::new()]),
            "the finish wrote a second blank row: {appends:?}"
        );
    }

    #[test]
    fn an_operation_with_no_bytes_in_it_leaves_nothing_owed() {
        // An empty delta and the second half of a CRLF both reach this module
        // as pushes, and neither asks the terminal for anything. Queued anyway
        // they would wedge the drain: an append with no rows and no scroll that
        // the loop offers, the terminal takes, and the band pays a frame for.
        let mut transcript = Transcript::new(80);
        assert!(!transcript.queue_push(""), "an empty push asked for a row");
        assert!(!transcript.owes(), "an empty push was queued");

        assert!(transcript.queue_push("a\r"));
        assert!(
            !transcript.queue_push("\n"),
            "the second half of a CRLF asked for a row of its own"
        );
        assert_eq!(
            drain(&mut transcript).len(),
            1,
            "the carry-only push was queued as an operation"
        );
        assert!(!transcript.owes());
    }

    #[test]
    fn holding_text_until_it_lands_does_not_change_what_the_document_says() {
        // The property that makes the queue a delay and not a second
        // behaviour: on a screen that never changes width, a session that
        // wrote every chunk as it arrived and one that could not write for a
        // while and then wrote everything leave the **same document** -- across
        // breaks, blank lines, CRLFs split at every offset, and the empty
        // pushes between them.
        for stream in [
            "a\r\nb",
            "a\r\rb",
            "a\r\r\nb",
            "a\rb\r\nc\r",
            "\r\n",
            "\r",
            "one\r\ntwo\r\nthree",
            "a paragraph\n\nand another\n\nand a third\n",
            "no breaks at all",
        ] {
            for at in 0..=stream.len() {
                if !stream.is_char_boundary(at) {
                    continue;
                }
                let (head, tail) = stream.split_at(at);
                for chunks in [vec![head, tail], vec![head, "", tail]] {
                    assert_eq!(
                        queued_document(12, &chunks),
                        document(12, &chunks),
                        "{stream:?} split at {at} said something different when \
                         it was held"
                    );
                }
            }
        }
    }

    #[test]
    fn a_line_ended_while_the_stream_is_queued_answers_the_carry_it_was_holding() {
        // The end of a line is a break, so a CR waiting for its LF has been
        // answered by it -- and the LF that opens the next chunk is a break of
        // its own. Decided when the operation is queued, because it is an
        // answer about the order the bytes arrived in and the queue is the only
        // thing that still knows it.
        let mut queued = Transcript::new(80);
        queued.queue_push("a\r");
        queued.queue_end_line();
        queued.queue_push("\nb");
        let held = replay(drain(&mut queued));

        let mut landed = Transcript::new(80);
        let written = replay([landed.push("a\r"), landed.end_line(), landed.push("\nb")]);

        assert_eq!(
            held, written,
            "the carry was answered at a different moment"
        );
    }

    #[test]
    fn a_queue_nothing_has_written_costs_one_pass_per_operation() {
        // The bound the shape has to keep: each attempt materializes the
        // **front** operation only. A drain that rebuilt everything owed on
        // every write -- or an operation that carried the whole queue's text --
        // would cost the square of the backlog, and the backlog is what a
        // session that cannot write for a few hundred ticks is made of.
        let operations = 2_000;
        let mut transcript = Transcript::new(40);
        for index in 0..operations {
            transcript.queue_push(&format!("line {index}\n"));
        }

        let appends = drain(&mut transcript);
        assert_eq!(appends.len(), operations, "one operation, one append");
        assert_eq!(
            appends
                .iter()
                .map(|append| append.rows.len())
                .sum::<usize>(),
            operations * 2,
            "each finished line is one row plus the row its successor starts \
             on -- anything more is an operation that re-wrote what an earlier \
             one already said"
        );
    }
}
