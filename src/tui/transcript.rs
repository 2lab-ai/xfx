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

use unicode_segmentation::UnicodeSegmentation;

use super::notice::Tone;
use super::prose::{self, Continuation, Opening, ProseRow};
use super::theme::{DocumentRow, Palette};
use super::wrap;

/// What a text *is*, which is what decides the colour the document row carrying
/// it is written in.
///
/// Carried beside the text rather than spelled into it, and that is the whole
/// of why this module holds it. A colour written into the queue is a colour
/// chosen at the moment the bytes arrived: the palette the session is in when
/// the row is finally handed over is the one the row has to read against, and
/// between those two moments a terminal may have changed its background
/// (`super::theme::notification`). So the text is kept as text, the meaning is
/// kept beside it, and the sequence is made at the one moment the palette is
/// known -- [`Transcript::emit_front`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Role {
    /// Text this module paints nothing onto: the echo of what the user
    /// submitted, and anything else that is not xfx's to colour. A row of it
    /// is handed over as the bytes it arrived as, sequences included -- which
    /// is a property of this path rather than a claim about what reaches it,
    /// since a provider's escapes are already spaced at the channel
    /// (`super::bridge`'s `inert`).
    Plain,
    /// One of xfx's own lines about the session: a notice's body, and the
    /// lines that are not notices yet (the tool rows).
    Notice,
    /// A notice's topic and its colon, in the colour its tone gives the
    /// label (`noticeLabelStyle`, `transcript_blocks.zig:1707-1716`). The
    /// glyph in front of it is the line's gutter ([`Gutter::Head`]), painted
    /// in the same colour.
    Label(Tone),
    /// Upstream's quiet rows: the turn summary under an answer, in the
    /// palette's dim foreground (`theme.zig`, `dim`).
    Dim,
}

impl Role {
    /// What opens a run of this role, when it opens one at all.
    ///
    /// `None` for [`Role::Plain`], and that is a different answer from an empty
    /// sequence: a plain row is handed over as the bytes it is, with no run to
    /// close, so text that carries its own attributes reaches the terminal
    /// exactly as it arrived.
    fn paint(self, palette: Palette) -> Option<&'static str> {
        match self {
            Role::Plain => None,
            Role::Notice => Some(palette.notice()),
            Role::Label(tone) => Some(tone.label(palette)),
            Role::Dim => Some(palette.dim()),
        }
    }
}

/// Which bytes of the unfinished line mean what.
///
/// **Spans of the tail, not a history.** What is kept is one entry per *run* of
/// a role, so a hundred thousand paced chunks of one answer are one entry: they
/// coalesce as they arrive ([`Roles::extend`]), a finished line drops them with
/// the tail, and a freeze trims them with it. The bound is therefore the tail's
/// own ([`MAX_TAIL_ROWS`]) and not the length of the session.
///
/// Spans rather than a role per operation because a role is a property of
/// **bytes**: one logical line can hold the echo of a prompt and the answer
/// that continues it, and the rows it wraps onto have to keep each part in its
/// own colour rather than take the colour of whatever extended the line last.
#[derive(Debug, Clone, Default)]
struct Roles {
    /// One run each, in order, each ending where the next begins. `end` is an
    /// offset into the tail, so the last one's `end` is the tail's length.
    spans: Vec<Span>,
}

/// One run of one role, ending at `end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Span {
    end: usize,
    role: Role,
}

impl Roles {
    /// Records that the next `bytes` of the tail are `role`.
    ///
    /// Coalescing is the bound: text that means the same thing as the text
    /// before it extends that run instead of adding one.
    fn extend(&mut self, bytes: usize, role: Role) {
        if bytes == 0 {
            return;
        }
        let end = self.len() + bytes;
        match self.spans.last_mut() {
            Some(last) if last.role == role => last.end = end,
            _ => self.spans.push(Span { end, role }),
        }
    }

    /// Records that the next bytes of the tail are what `range` of `source`
    /// is, run by run.
    fn extend_from(&mut self, source: &Roles, range: std::ops::Range<usize>) {
        let mut begin = 0usize;
        for span in &source.spans {
            let low = begin.max(range.start);
            let high = span.end.min(range.end);
            if low < high {
                self.extend(high - low, span.role);
            }
            begin = span.end;
            if begin >= range.end {
                break;
            }
        }
    }

    /// How many bytes are described.
    fn len(&self) -> usize {
        self.spans.last().map_or(0, |span| span.end)
    }

    /// Forgets everything, for a line that ended.
    fn clear(&mut self) {
        self.spans.clear();
    }

    /// Drops the first `bytes`, which a freeze has just dropped from the tail.
    fn trim_front(&mut self, bytes: usize) {
        self.spans.retain(|span| span.end > bytes);
        for span in &mut self.spans {
            span.end -= bytes;
        }
    }

    /// What the byte at `offset` means.
    ///
    /// [`Role::Plain`] past the end: a span table that ever fell short of its
    /// tail would paint nothing rather than paint the wrong thing.
    fn at(&self, offset: usize) -> Role {
        self.spans
            .iter()
            .find(|span| offset < span.end)
            .map_or(Role::Plain, |span| span.role)
    }

    /// The one role the whole of `range` is, when it is only one.
    ///
    /// The answer a row asks first: a row of one role is written as one run,
    /// which is the common case and the one that costs nothing to find.
    fn only(&self, range: std::ops::Range<usize>) -> Option<Role> {
        if range.start >= range.end {
            return Some(Role::Plain);
        }
        let span = self.spans.iter().find(|span| range.start < span.end);
        match span {
            Some(span) if range.end <= span.end => Some(span.role),
            Some(_) => None,
            // Past every span: plain to the end.
            None => Some(Role::Plain),
        }
    }
}

/// What the bytes of one queued push mean: one role for all of them -- the
/// stream's case, which costs nothing to carry -- or a run per role, for a line
/// xfx composed out of parts (a notice's label and its body).
#[derive(Clone, Copy)]
enum Source<'a> {
    One(Role),
    Runs(&'a Roles),
}

impl Source<'_> {
    /// Records `range` of the push's bytes into `roles`, in the role(s) the
    /// push gave them.
    fn extend(self, roles: &mut Roles, range: std::ops::Range<usize>) {
        match self {
            Source::One(role) => roles.extend(range.len(), role),
            Source::Runs(runs) => roles.extend_from(runs, range),
        }
    }
}

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
///
/// `kinds` is what each row is, for the theme report that may later repaint
/// it (`super::theme::DocumentRow`): **empty when every row is plain**, which
/// is most appends, and otherwise one per row. Recorded with the row as it is
/// written rather than read back off its painted cells, because two of the
/// document's greys are one number on a dark screen and two on a light one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Append {
    pub(crate) scroll: usize,
    pub(crate) rows: Vec<String>,
    pub(crate) kinds: Vec<DocumentRow>,
    /// How many of `rows`' last rows a block that shrank wrote blank and
    /// gives back: once the append has landed they are not the document's,
    /// and the band moves up over them (zero for every append but such a
    /// rewrite).
    pub(crate) release: usize,
}

impl Append {
    /// An append that asks the terminal for nothing.
    fn nothing() -> Self {
        Self {
            scroll: 0,
            rows: Vec::new(),
            kinds: Vec::new(),
            release: 0,
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

/// `count` rows of `kind`, sparsely: nothing at all for plain rows ([`Append`]).
fn kinds_of(kind: DocumentRow, count: usize) -> Vec<DocumentRow> {
    if kind == DocumentRow::Plain {
        Vec::new()
    } else {
        vec![kind; count]
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

/// What every row of a line starts with, in upstream's transcript
/// (`c1db919`).
///
/// A property of the **line**, not of the text: the rows a line wraps onto
/// each carry it, and the text is wrapped to the width that is left. It is
/// how a submitted prompt becomes upstream's card -- the rail
/// (`user_message_card.zig:319-325`) -- and how the assistant's answer sits
/// in its two-column gutter (`assistant_wrap.zig:706,744`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Gutter {
    /// Nothing: xfx's own lines, the welcome line.
    #[default]
    None,
    /// `┃` in the hint colour and a space.
    Rail,
    /// Two blank columns: the assistant's answer, wrapped by upstream's
    /// assistant wrap ([`super::prose`]) -- no orphan on a line's last row,
    /// and a list item's, a quote's or an indented paragraph's continuation
    /// rows lined up under its text.
    Indent,
    /// The rest of an answer line whose first rows a freeze has handed to the
    /// terminal ([`freeze`]): two blank columns, as [`Gutter::Indent`], and
    /// the continuation that line's marker set -- which the text still held
    /// here no longer begins with, so it is carried instead of read. Becomes
    /// [`Gutter::Indent`] again when the line ends.
    Carried(Option<Continuation>),
    /// A notice's: its glyph and a space in front of the **first** row, in
    /// the label's colour, and two blank columns in front of every row after
    /// it -- the rows the first line wraps onto and every line after it.
    ///
    /// Upstream composes `<glyph> <topic>: <body>` as one logical line, wraps
    /// it at the screen's width and indents every continuation row by two
    /// (`renderSemanticNotice`, `transcript_blocks.zig:1758-1800`;
    /// `noticeContinuationIndent`, `:1718-1725`). Every glyph is one cell
    /// (`super::notice::Tone::glyph`), so the glyph and its space are exactly
    /// a two-column gutter: the text wrapped at `cols - 2` behind it is the
    /// same rows as upstream's line wrapped at `cols`, with the glyph standing
    /// where the indent stands on the rows below it.
    ///
    /// **One-shot.** Only the first row of the next line carries the glyph;
    /// once that line has ended -- or a freeze has dropped its first row --
    /// the gutter is [`Gutter::NoticeIndent`] until the next one is queued
    /// ([`Self::continued`]).
    ///
    /// Its text wraps by upstream's notice rule rather than the prose one
    /// ([`super::wrap::wrap_notice`]): a path breaks after a separator.
    Head(Tone),
    /// The rest of a notice: two blank columns, as [`Gutter::Indent`] is, and
    /// the notice's own wrap, as [`Gutter::Head`] has -- every line of a
    /// notice is wrapped as upstream wraps the whole of it
    /// (`renderSemanticNotice`, `transcript_blocks.zig:1781-1798`).
    NoticeIndent,
}

impl Gutter {
    /// How many columns the gutter takes from every row.
    pub(crate) fn width(self) -> u16 {
        match self {
            Gutter::None => 0,
            Gutter::Rail
            | Gutter::Indent
            | Gutter::Carried(_)
            | Gutter::Head(_)
            | Gutter::NoticeIndent => 2,
        }
    }

    /// What the first row of the line opens with. Self-contained: whatever it
    /// opens it closes, so the text's own sequences start from a clean pen.
    fn prefix(self, palette: Palette) -> String {
        match self {
            Gutter::None => String::new(),
            Gutter::Rail => format!("{}\u{2503}{} ", palette.hint(), palette.reset()),
            Gutter::Indent | Gutter::Carried(_) | Gutter::NoticeIndent => "  ".to_string(),
            // The space inside the run, as upstream's label is: one run of one
            // colour from the glyph to the colon, so a capture reads the label
            // as the single span upstream's does.
            Gutter::Head(tone) => {
                format!(
                    "{}{} {}",
                    tone.label(palette),
                    tone.glyph(),
                    palette.reset()
                )
            }
        }
    }

    /// The gutter the rows **after** this line's first one carry, and the one
    /// the next line begins with.
    ///
    /// Itself for every gutter but a notice's head, whose glyph belongs to one
    /// row only, and an answer line's carried continuation, which belongs to
    /// that line only.
    fn continued(self) -> Gutter {
        match self {
            Gutter::Head(_) => Gutter::NoticeIndent,
            Gutter::Carried(_) => Gutter::Indent,
            other => other,
        }
    }

    /// The gutter the rest of a line is wrapped with once a freeze has dropped
    /// `line`'s first rows: [`Self::continued`], except that an answer line
    /// keeps the continuation its marker set ([`Gutter::Carried`]).
    ///
    /// `line` is the text held **before** the drop: the whole line from its
    /// start, unless an earlier freeze already carried its continuation.
    fn frozen(self, line: &str) -> Gutter {
        match self {
            Gutter::Indent => Gutter::Carried(prose::continuation_at(line, 0)),
            Gutter::Carried(carried) => Gutter::Carried(carried),
            other => other.continued(),
        }
    }

    /// Whether this gutter's text is the assistant's answer, which upstream
    /// wraps with its assistant wrap ([`super::prose`]).
    fn is_answer(self) -> bool {
        matches!(self, Gutter::Indent | Gutter::Carried(_))
    }

    /// The rows `text` occupies at `cols` behind this gutter: a notice's by
    /// upstream's notice rule, the answer's by upstream's assistant wrap, and
    /// everything else's by the composer's.
    fn wrap(self, text: &str, cols: u16) -> Vec<ProseRow> {
        let bare = |rows: Vec<wrap::Row>| {
            rows.into_iter()
                .map(|row| ProseRow { row, lead: None })
                .collect()
        };
        match self {
            Gutter::Head(_) | Gutter::NoticeIndent => bare(wrap::wrap_notice(text, cols)),
            Gutter::None | Gutter::Rail => bare(wrap::wrap(text, cols)),
            Gutter::Indent => prose::prose(text, cols, self.width(), Opening::Line),
            Gutter::Carried(carried) => {
                prose::prose(text, cols, self.width(), Opening::Continued(carried))
            }
        }
    }
}

/// One thing the document was told, in the shape it was told it.
///
/// **Logical rather than visual, and that is the whole point of the queue.** A
/// `Push` carries the text itself -- normalized, and with a carried CRLF
/// already resolved, because both of those are answers about the *stream* and
/// the stream is gone by the time the rows are measured. Nothing here knows how
/// wide the screen is; that question is asked once, at the moment the terminal
/// is offered the rows ([`Transcript::emit_front`]).
enum Op {
    /// This text arrived, and what it is. Never empty: an operation that adds
    /// no bytes would ask the terminal for nothing and is not queued at all.
    Push(String, Role),
    /// The same for a text made of runs that mean different things, with the
    /// runs beside it ([`Transcript::queue_push_runs`]). One operation, so the
    /// line it writes lands once rather than once per part.
    Runs(String, Roles),
    /// The line ended. Carries no bytes of its own and is queued anyway,
    /// because it is a **state transition**: the line it closes stops being
    /// rewritable, and the text after it starts a row of its own.
    EndLine,
    /// Lines from here on carry this gutter. Queued only at a line boundary
    /// ([`Transcript::queue_gutter`]), so no row already on the screen is
    /// re-prefixed.
    Gutter(Gutter),
    /// The open block, whole, as it now stands: rows that are each **one**
    /// screen row and replace the ones the block had, and what each of them
    /// is ([`Transcript::queue_block`]).
    Block(Vec<String>, Vec<DocumentRow>),
    /// Lines from here on are rows of this kind ([`Transcript::queue_kind`]).
    /// Queued only at a line boundary, like a gutter.
    Kind(DocumentRow),
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
    /// What the tail's bytes mean, run by run, and therefore what each row it
    /// wraps onto is painted in. Always describes exactly [`Self::tail`]'s
    /// bytes: the two are cleared, extended and trimmed together.
    roles: Roles,
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
    /// Whether the document ends on a **blank row**, or holds nothing at all,
    /// once everything queued has landed.
    ///
    /// The other projection a caller asks about the document it is about to
    /// write after, and for the same reason [`Self::open`] is one: a notice
    /// wants one blank row above it (`fx-basic-40x120-version-trailing-space`:
    /// two notices in a row share one), and whether the row above already is
    /// one is a question about the text queued so far, not about what the
    /// terminal has. `true` for a fresh transcript, so the first thing written
    /// after a launch or a `/clear` is not pushed down a row by nothing.
    blank: bool,
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
    /// The gutter the tail's line -- and every line after it until the next
    /// [`Op::Gutter`] -- is written with, **as of the last operation the
    /// terminal took**.
    gutter: Gutter,
    /// What the rows of the lines written from here on are
    /// ([`Self::queue_kind`]).
    kind: DocumentRow,
    /// Whether what [`Self::painted`] counts is an open **block** rather than
    /// a line of text, as of the last operation the terminal took.
    ///
    /// The two are rewritten by different rules: a line's rows are its text
    /// wrapped, and a block's are given ([`Op::Block`]), one screen row each,
    /// so a resize re-measures the first and leaves the second's count alone.
    block: bool,
    /// How many of the open block's first rows are no longer rewritten: the
    /// block's own [`MAX_TAIL_ROWS`] bound, for the reason a line has one
    /// ([`freeze_block`]).
    block_frozen: usize,
    /// Whether a block is open once **everything queued** has landed -- the
    /// enqueue-side projection of [`Self::block`], for the reason
    /// [`Self::open`] is one.
    block_open: bool,
}

impl Transcript {
    /// How many of the document's last rows are the open line or block this
    /// module may still write over ([`Self::painted`]).
    pub(crate) fn open_rows(&self) -> usize {
        self.painted
    }

    pub(crate) fn new(cols: u16) -> Self {
        Self {
            cols,
            tail: String::new(),
            roles: Roles::default(),
            painted: 0,
            queue: VecDeque::new(),
            open: false,
            blank: true,
            split_crlf: false,
            gutter: Gutter::None,
            kind: DocumentRow::Plain,
            block: false,
            block_frozen: 0,
            block_open: false,
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
    pub(crate) fn queue_push(&mut self, text: &str, role: Role) -> bool {
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
        // Text never continues a block: whatever follows one starts on a row
        // of its own, and the block's last row is left as it stands.
        if self.block_open {
            self.queue_end_line();
        }
        self.queue.push_back(Op::Push(body.to_string(), role));
        // Every push leaves a line open, including one that ends on a break:
        // the row the next text starts on is the line this push opened, and it
        // is a row of the screen as soon as the push lands.
        self.open = true;
        // The line this push leaves open is empty exactly when the push ended
        // on a break: that row is on the screen, and it is blank.
        self.blank = body.ends_with('\n');
        true
    }

    /// Queues text made of runs that mean different things -- a notice's label
    /// and its body -- as **one** operation, and says whether it asks for
    /// anything.
    ///
    /// One push of the whole rather than one per run because each push is an
    /// append, and an append rewrites the line it lengthens: three pushes for
    /// `✗ command:`, ` ` and the body would write the same row three times.
    ///
    /// Each run is normalized on its own, so a CR that ends one run and an LF
    /// that opens the next are two breaks rather than one; the caller is xfx
    /// composing its own words at a label's edge, not a stream cut at an
    /// arbitrary byte, which is what [`Self::queue_push`] is for. The carry a
    /// stream left is honoured exactly as there.
    pub(crate) fn queue_push_runs(&mut self, runs: &[(&str, Role)]) -> bool {
        let mut body = String::new();
        let mut roles = Roles::default();
        for (text, role) in runs {
            let normalized = normalize(text);
            roles.extend(normalized.len(), *role);
            body.push_str(&normalized);
        }
        let mut raw = runs
            .iter()
            .map(|(text, _)| *text)
            .filter(|text| !text.is_empty());
        let Some(first) = raw.next() else {
            return false;
        };
        let last = raw.next_back().unwrap_or(first);
        let carried = self.split_crlf && first.starts_with('\n');
        self.split_crlf = last.ends_with('\r');
        if carried && body.starts_with('\n') {
            body.remove(0);
            roles.trim_front(1);
        }
        if body.is_empty() {
            return false;
        }
        // As for a single push: text never continues a block.
        if self.block_open {
            self.queue_end_line();
        }
        self.open = true;
        self.blank = body.ends_with('\n');
        self.queue.push_back(Op::Runs(body, roles));
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
        // A line with nothing open is the blank row this end writes. An open
        // one ends as whatever it holds, which [`Self::queue_push`] already
        // said: blank when the push that opened it ended on a break.
        if blank_row {
            self.blank = true;
        }
        self.open = false;
        self.block_open = false;
        blank_row
    }

    /// Queues the open block as it now stands, and says whether it asks for
    /// anything.
    ///
    /// **The one thing in the document that is rewritten after it has been
    /// written**, and it is the tail's rule stretched to more than one line
    /// rather than an exception to it: a block is the last thing in the
    /// document for as long as it is open, exactly as the unfinished line is,
    /// so the rows it occupies are the rows immediately above the band and an
    /// append can rewrite them where they are ([`Append`]). It closes the way a
    /// line does -- [`Self::queue_end_line`], or anything else queued after it
    /// -- and from then on it is the terminal's like every finished row.
    ///
    /// Each row is one screen row: it is never wrapped, and one wider than the
    /// screen it lands on is cut with an ellipsis there ([`ellipsized`]). A
    /// block that is given fewer rows than it already occupies keeps the rest
    /// as blank rows rather than leaving their old text on the screen.
    ///
    /// Starts on a row of its own: an open line of text is ended first.
    pub(crate) fn queue_block(&mut self, rows: Vec<String>, kinds: Vec<DocumentRow>) -> bool {
        if rows.is_empty() {
            return false;
        }
        if self.open && !self.block_open {
            self.queue_end_line();
        }
        // The document ends as the block's last row does: a tool group's is
        // the blank row it ends on, so a notice written under it does not
        // grow a second one ([`Self::ends_blank`]).
        self.blank = rows.last().is_some_and(String::is_empty);
        self.queue.push_back(Op::Block(rows, kinds));
        self.open = true;
        self.block_open = true;
        true
    }

    /// Queues a change of gutter for the lines that follow.
    ///
    /// Ends the open line first, if there is one: a gutter is a property of a
    /// whole line, and a line half-written with one and half with another would
    /// be two shapes on the screen for one sentence.
    pub(crate) fn queue_gutter(&mut self, gutter: Gutter) -> bool {
        let asked = self.open && self.queue_end_line();
        self.queue.push_back(Op::Gutter(gutter));
        asked
    }

    /// Queues what the rows of the lines that follow are, for a theme report
    /// that later repaints them ([`DocumentRow`]).
    ///
    /// Ends the open line first, as [`Self::queue_gutter`] does and for its
    /// reason: a kind belongs to a whole line.
    pub(crate) fn queue_kind(&mut self, kind: DocumentRow) -> bool {
        let asked = self.open && self.queue_end_line();
        self.queue.push_back(Op::Kind(kind));
        asked
    }

    /// Whether a line is open once everything queued has landed.
    // `super::shell::Shell::finish_document_line` is the caller: it is how
    // "end the line" is told apart from "leave a blank row", and it is asked
    // about the document the queued text is going to make rather than about the
    // rows the terminal has so far.
    pub(crate) fn open_line(&self) -> bool {
        self.open
    }

    /// Whether the document ends on a blank row -- or is empty -- once
    /// everything queued has landed ([`Self::blank`]).
    // `super::shell::Shell::write_notice` is the caller: a notice has one blank
    // row above it, and two notices in a row share the one between them.
    pub(crate) fn ends_blank(&self) -> bool {
        self.blank
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
    ///
    /// `palette` is the session's **now**, and it is a parameter for the same
    /// reason the width is measured here: a row is painted at the moment it is
    /// handed over. Text queued while the terminal was dark and released after
    /// its user switched to light is written in the light palette, without a
    /// byte of what is queued being rewritten.
    pub(crate) fn emit_front<E>(
        &mut self,
        palette: Palette,
        emit: impl FnOnce(&Append) -> Landed<E>,
    ) -> Option<Result<(), E>> {
        let prepared = self.prepare_front(palette)?;
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
    fn prepare_front(&self, palette: Palette) -> Option<Prepared> {
        match self.queue.front()? {
            Op::Push(body, role) => Some(self.prepare_push(body, Source::One(*role), palette)),
            Op::Runs(body, runs) => Some(self.prepare_push(body, Source::Runs(runs), palette)),
            Op::EndLine => Some(self.prepare_end_line(palette)),
            Op::Gutter(gutter) => Some(Prepared {
                append: Append::nothing(),
                tail: self.tail.clone(),
                roles: self.roles.clone(),
                painted: self.painted,
                gutter: *gutter,
                kind: self.kind,
                block: self.block,
                block_frozen: self.block_frozen,
            }),
            Op::Kind(kind) => Some(Prepared {
                append: Append::nothing(),
                tail: self.tail.clone(),
                roles: self.roles.clone(),
                painted: self.painted,
                gutter: self.gutter,
                kind: *kind,
                block: self.block,
                block_frozen: self.block_frozen,
            }),
            Op::Block(rows, kinds) => Some(self.prepare_block(rows, kinds)),
        }
    }

    /// What the open block would write as `rows`, and the state it would
    /// leave.
    ///
    /// Only ever in front of a committed tail that is empty or is this block:
    /// [`Self::queue_block`] ends an open line before it queues one.
    fn prepare_block(&self, rows: &[String], kinds: &[DocumentRow]) -> Prepared {
        let (frozen, painted) = if self.block {
            (self.block_frozen, self.painted)
        } else {
            (0, 0)
        };
        let mut written: Vec<String> = rows
            .iter()
            .skip(frozen)
            .map(|row| ellipsized(row, self.cols))
            .collect();
        // A block that lost rows must not leave their text behind: the rows it
        // occupied are still its until a write clears them -- and once that
        // write has landed they are given back, so the band moves up over
        // them rather than leaving blank rows between the block and itself
        // (a group collapsed while it is open, `fx-settings-collapse-40x120-running`).
        // Not past rows already frozen: those are not rewritten, so what the
        // block keeps is not the rows it has.
        let fresh = written.len();
        let released = if frozen == 0 {
            painted.saturating_sub(fresh)
        } else {
            0
        };
        written.resize(written.len().max(painted), String::new());
        let kinds = if kinds.iter().all(|kind| *kind == DocumentRow::Plain) {
            Vec::new()
        } else {
            (frozen..frozen + written.len())
                .map(|index| kinds.get(index).copied().unwrap_or(DocumentRow::Plain))
                .collect()
        };
        let append = Append {
            scroll: written.len() - painted,
            rows: written,
            kinds,
            release: released,
        };
        let (frozen, painted) = freeze_block(frozen, append.rows.len() - released);
        Prepared {
            append,
            tail: String::new(),
            roles: Roles::default(),
            painted,
            gutter: self.gutter,
            kind: self.kind,
            block: true,
            block_frozen: frozen,
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
    fn prepare_push(&self, body: &str, source: Source<'_>, palette: Palette) -> Prepared {
        let painted = self.painted;
        let mut tail = self.tail.clone();
        // Beside the committed spans for the reason the tail is cloned: a write
        // the terminal refuses must leave this module holding exactly what it
        // held before.
        let mut roles = self.roles.clone();
        let mut rows = Vec::new();
        let mut segments = body.split('\n');
        // `split` yields the text itself when there is no break in it, so the
        // first segment always exists and always joins the tail.
        let first = segments.next().unwrap_or_default();
        tail.push_str(first);
        source.extend(&mut roles, 0..first.len());
        // Where the next segment begins in `body`: past this one and its break.
        let mut offset = first.len() + 1;
        let mut gutter = self.gutter;
        for next in segments {
            // Everything before the break is a finished line. Its rows are
            // written once, here, and never again.
            rows.append(&mut texts(&tail, &roles, self.cols, gutter, palette));
            // A notice's glyph went with the line it headed.
            gutter = gutter.continued();
            // The line is over, so its offsets are too: the text after the
            // break starts a row of its own and a span table of its own.
            tail.clear();
            roles.clear();
            tail.push_str(next);
            source.extend(&mut roles, offset..offset + next.len());
            offset += next.len() + 1;
        }
        let mut last = texts(&tail, &roles, self.cols, gutter, palette);
        let mut open_rows = last.len();
        rows.append(&mut last);
        // After the append is built, because what is frozen is what the *next*
        // push may no longer rewrite; this one has already said what it owes.
        if let Some(after) = freeze(
            &mut tail,
            &mut roles,
            &mut open_rows,
            inner(self.cols, gutter),
            gutter,
        ) {
            // The row a notice's glyph stood on is the terminal's now, and the
            // tail this keeps begins on a continuation row: re-wrapped as a
            // head, it would grow a second glyph -- and re-wrapped as an
            // answer line's start, it would lose the indent its marker set.
            gutter = after;
        }

        Prepared {
            roles,
            // The rows already on the screen are the first `painted` of these
            // -- the old tail is a prefix of the text they came from -- so they
            // are rewritten where they are and everything past them is new.
            append: Append {
                scroll: rows.len().saturating_sub(painted),
                kinds: kinds_of(self.kind, rows.len()),
                rows,
                release: 0,
            },
            tail,
            painted: open_rows,
            gutter,
            kind: self.kind,
            block: false,
            block_frozen: 0,
        }
    }

    /// What ending the line would write, and the empty tail it would leave.
    fn prepare_end_line(&self, palette: Palette) -> Prepared {
        // A block's rows are already on the screen as they stand, and an empty
        // tail would otherwise be measured as the one row a wrap of nothing is.
        if self.block {
            return Prepared {
                append: Append::nothing(),
                tail: String::new(),
                roles: Roles::default(),
                painted: 0,
                gutter: self.gutter,
                kind: self.kind,
                block: false,
                block_frozen: 0,
            };
        }
        let rows = texts(&self.tail, &self.roles, self.cols, self.gutter, palette);
        let scroll = rows.len().saturating_sub(self.painted);
        Prepared {
            append: if scroll == 0 {
                Append::nothing()
            } else {
                Append {
                    scroll,
                    kinds: kinds_of(self.kind, rows.len()),
                    rows,
                    release: 0,
                }
            },
            tail: String::new(),
            roles: Roles::default(),
            painted: 0,
            // The line a notice's glyph headed is over.
            gutter: self.gutter.continued(),
            kind: self.kind,
            block: false,
            block_frozen: 0,
        }
    }

    /// Adopts a candidate the terminal took, and drops the operation it came
    /// from.
    fn commit(&mut self, prepared: Prepared) {
        self.queue.pop_front();
        self.tail = prepared.tail;
        self.roles = prepared.roles;
        self.painted = prepared.painted;
        self.gutter = prepared.gutter;
        self.kind = prepared.kind;
        self.block = prepared.block;
        self.block_frozen = prepared.block_frozen;
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
        // A block's rows are one screen row each at any width, so the count
        // the next append measures against is the one it already has.
        if self.block {
            return;
        }
        self.painted = self.gutter.wrap(&self.tail, inner(cols, self.gutter)).len();
    }
}

/// The open block's frozen prefix and painted count once it holds `rows` rows
/// past the prefix it had.
///
/// The bound [`freeze`] gives a line, for the same reason: every rewrite of a
/// block re-places every row it still holds, so a block that grew for ever
/// would cost more for every row it gained. Rows past [`MAX_TAIL_ROWS`] from
/// the bottom are more than any screen shows, so they are on the terminal's
/// scrollback already and no rewrite would reach them anyway -- dropping them
/// changes what this module holds and not what the terminal shows.
fn freeze_block(frozen: usize, rows: usize) -> (usize, usize) {
    if rows <= MAX_TAIL_ROWS {
        return (frozen, rows);
    }
    (frozen + (rows - MAX_TAIL_ROWS), MAX_TAIL_ROWS)
}

/// One block row as it is written at `cols`: whole when it fits, and cut one
/// cell short with an ellipsis in that cell when it does not
/// (`vercel-labs/fx@c1db9191 src/ui/transcript/tool_group_projection.zig:376-392`,
/// `clipSummary`).
///
/// Cut by the painter's own function (`super::frame::clip`), so the row this
/// makes is the row the painter puts on the screen rather than one it cuts
/// again. A cut inside a run of colour leaves it open, so the row is closed
/// after the ellipsis, as upstream closes it.
pub(crate) fn ellipsized(row: &str, cols: u16) -> String {
    if wrap::width(row) <= cols {
        return row.to_string();
    }
    if cols == 0 {
        return String::new();
    }
    let mut cut = format!("{}\u{2026}", super::frame::clip(row, cols - 1));
    if cut.contains('\u{1b}') && !cut.ends_with("\u{1b}[0m") {
        cut.push_str("\u{1b}[0m");
    }
    cut
}

/// The width a line's text is wrapped to: the screen less its gutter, and
/// never nothing.
fn inner(cols: u16, gutter: Gutter) -> u16 {
    cols.saturating_sub(gutter.width()).max(1)
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
/// **The answer's wrap looks further ahead, and is settled anyway.** Upstream's
/// assistant wrap ([`super::prose`]) moves a word down to keep a line's last
/// word company, so the break before a line's last row depends on where the
/// line *ends*. An answer line is never appended to after it is pushed: the
/// presenter holds a line until its newline has arrived and hands it over
/// whole (`crate::markdown`'s processor), so a freeze measures the whole line
/// and what it keeps is that line's last row.
///
/// The alternative -- ending the line early -- was rejected: a break the
/// provider did not write lands in the middle of a row, and the answer grows a
/// short line every few thousand characters.
///
/// Answers with the gutter the rest of the line is wrapped with when it dropped
/// anything: a line whose first row is no longer held here is a line whose
/// next re-wrap starts on a continuation row ([`Gutter::frozen`]), and `None`
/// when it dropped nothing.
fn freeze(
    tail: &mut String,
    roles: &mut Roles,
    open_rows: &mut usize,
    cols: u16,
    gutter: Gutter,
) -> Option<Gutter> {
    if *open_rows <= MAX_TAIL_ROWS {
        return None;
    }
    let rows = gutter.wrap(tail, cols);
    let last = rows.last()?;
    let kept = last.row.start;
    if kept == 0 {
        return None;
    }
    // Asked before the drop, while the tail still begins where the line does.
    let after = gutter.frozen(tail);
    tail.drain(..kept);
    // With the tail and by the same offset, because the spans describe *these*
    // bytes: a table left whole would paint the kept row in the role of text
    // that is the terminal's now.
    roles.trim_front(kept);
    // Asked rather than assumed to be one: the answer is what the *next*
    // append measures its scroll against, and a count larger than the rows the
    // tail really occupies would make that scroll too small -- which is the
    // renderer treating unpainted rows as settled, the silent loss this whole
    // path is counted in `usize` to avoid.
    *open_rows = after.wrap(tail, cols).len();
    Some(after)
}

/// `text`, wrapped, as the strings an append writes -- each row painted in the
/// roles its own bytes carry.
///
/// **Wrapped first and painted second**, and the order is the claim: the rows
/// are measured from the text alone, so a role costs no cell and moves no
/// break. What the sequences are added to is the row that measurement produced.
///
/// **A line that carries its own sequences is made row-by-row whole.** xfx's
/// own styled lines -- the welcome line, the assistant's rendered markdown --
/// are [`Role::Plain`] text with SGR inside, and a wrap can fall inside a run.
/// A terminal's pen survives a `CUP`, so a row handed over as its bytes would
/// colour the start of the next row with whatever the last one left open --
/// and the output check (`super::check`) would refuse the frame, rightly. So
/// each such row opens with the state its line had reached where it begins
/// (`super::pacer::SgrState`, the same model the wire checker reads colours
/// with), and closes whatever is still open at the point the painter cuts.
fn texts(text: &str, roles: &Roles, cols: u16, gutter: Gutter, palette: Palette) -> Vec<String> {
    let rows = body_rows(text, roles, inner(cols, gutter), gutter, palette);
    if gutter == Gutter::None {
        return rows;
    }
    let head = gutter.prefix(palette);
    let rest = gutter.continued();
    let continuation = rest.prefix(palette);
    rows.into_iter()
        .enumerate()
        // The indent goes only where there is something to indent, as
        // upstream's does (`assistant_wrap.zig:390-407`); the rail is on every
        // row of the card, blank ones included, and a notice's glyph is on its
        // first row whatever that row holds.
        .map(|(index, row)| {
            let (prefix, gutter) = if index == 0 {
                (&head, gutter)
            } else {
                (&continuation, rest)
            };
            if matches!(
                gutter,
                Gutter::Indent | Gutter::Carried(_) | Gutter::NoticeIndent
            ) && row.is_empty()
            {
                row
            } else {
                format!("{prefix}{row}")
            }
        })
        .collect()
}

/// The text's own rows at `cols`, before any gutter.
fn body_rows(
    text: &str,
    roles: &Roles,
    cols: u16,
    gutter: Gutter,
    palette: Palette,
) -> Vec<String> {
    let styled = text.contains('\u{1b}');
    let mut state = super::pacer::SgrState::default();
    let mut seen = 0usize;
    // The screen's width, for the answer's tab stops: they are columns of the
    // screen, gutter included.
    let screen = usize::from(cols) + usize::from(gutter.width());
    gutter
        .wrap(text, cols)
        .into_iter()
        .map(|laid| {
            let row = laid.row;
            // A continuation row's lead, in front of everything the row
            // reopens -- upstream writes the indent, or the quote's rules,
            // and then the pen (`assistant_wrap.zig:703-725`). It is there
            // only when it leaves room, so the text's own cut is the rest.
            let lead = laid.lead.map(Continuation::lead).unwrap_or_default();
            let cols = cols.saturating_sub(laid.lead_width());
            let slice = &text[row.start..row.end];
            // The answer's tabs, as the spaces to their stops: the painter's
            // four-cell tab is the composer's.
            let slice = if gutter.is_answer() {
                let first = usize::from(gutter.width()) + usize::from(laid.lead_width()) + 1;
                prose::expand_tabs(slice, first, screen)
            } else {
                std::borrow::Cow::Borrowed(slice)
            };
            let slice = slice.as_ref();
            let cut = drawn(slice, row.width, cols);
            let plain = roles.only(row.start..row.end) == Some(Role::Plain);
            if !(styled && plain) {
                // A plain row is written as it is; only a role paints.
                if plain {
                    return format!("{lead}{slice}");
                }
                return paint(slice, roles, row.start, cut, palette);
            }
            state.observe(&text[seen.min(row.start)..row.start]);
            let reopen = format!("{lead}{}", state.reopen());
            let (shown, past) = slice.split_at(cut);
            // The spaces a wrap leaves at the end of a row that another row
            // follows are the break, not part of the run they fell inside:
            // upstream's wrap drops them (`assistant_wrap.zig:1306-1324`,
            // `alpha beta` -> `alpha` / `beta`), so the cell after the last
            // word is a blank one there. Written after the close here, they
            // are blank cells too -- where an underline under them, or a link
            // a click on them would follow, is a cell upstream does not paint.
            let hung = if row.end < text.len() {
                shown.len() - shown.trim_end_matches([' ', '\n']).len()
            } else {
                0
            };
            let (shown, spaces) = shown.split_at(shown.len() - hung);
            state.observe(shown);
            // A reset for the attributes and a close for a link, each only
            // when the row leaves it open: a wrap inside a link's text closes
            // the link on this row and the reopen above carries it onto the
            // next, as upstream's wrap does (`assistant_wrap.zig:1306-1311`).
            let close = state.close();
            state.observe(past);
            seen = row.end;
            format!("{reopen}{shown}{close}{spaces}{past}")
        })
        .collect()
}

/// How much of a row the painter will really put on the wire.
///
/// **A row can be wider than the screen it was measured for.** `wrap` hangs a
/// trailing space past the margin rather than breaking on it, and a single
/// cluster wider than the whole row stays on the row it would not fit
/// -- so a row's width is not bounded by `cols`, and the painter
/// (`super::frame::row_text`) cuts what it is given at the first cluster that
/// does not fit. **Everything after that cut is not written**, including a
/// reset at the end of the row: a run this module opened and closed *past* the
/// cut reaches the terminal opened and never closed, and this phase never
/// repaints a document row to take it back.
///
/// So the cut is asked for here, before anything is painted, and it is asked of
/// `super::frame::clip` -- the painter's own function, for the reason its doc
/// gives: a row built to a width and cut by a different rule than the painter's
/// is a row the two disagree about, and the shorter answer is the one on the
/// screen.
///
/// The common row is not wider than the screen and pays nothing for this.
fn drawn(row: &str, width: u16, cols: u16) -> usize {
    if width <= cols {
        return row.len();
    }
    super::frame::clip(row, cols).len()
}

/// One row, opened and closed for each run of a role it carries.
///
/// `at` is where the row begins in the text the spans describe, which is what
/// makes a continuation row keep the colour the row above opened: a role is a
/// property of the bytes, and the bytes of row two are inside the same run.
///
/// A run is closed before anything unpainted, at the end of the row, and at
/// `drawn` -- the point the painter cuts ([`drawn`]). Past that point the bytes
/// are handed over **as they are**: they are not drawn, so painting them can
/// only produce a sequence the terminal never sees the end of. Nothing is
/// dropped here either, because the rows still have to tile the text they were
/// wrapped from -- what changes is only where the colour stops.
fn paint(text: &str, roles: &Roles, at: usize, drawn: usize, palette: Palette) -> String {
    // Nothing on the row, so nothing to colour -- and an empty row wearing a
    // colour is still bytes on a link.
    if text.is_empty() {
        return String::new();
    }
    let (painted, past) = text.split_at(drawn);
    // None of this row is drawn -- one cluster wider than the whole screen, or
    // a screen narrower than a glyph. A run opened here would be cut off in
    // front of its own reset, which is the leak this guards against, and it
    // would paint nothing even if it survived.
    if painted.is_empty() {
        return text.to_string();
    }
    if let Some(role) = roles.only(at..at + painted.len()) {
        return match role.paint(palette) {
            None => text.to_string(),
            Some(open) => format!("{open}{painted}{}{past}", palette.reset()),
        };
    }
    // More than one role on this row. The runs are found by **grapheme
    // cluster** rather than by byte, so a combining mark that arrived in a
    // later chunk than the letter it composes with cannot be split off into a
    // run of its own: a cluster takes the role of the byte it starts at, and
    // the terminal is given one glyph in one colour.
    let mut row = String::with_capacity(text.len());
    let mut run = roles.at(at);
    let mut start = 0usize;
    for (offset, _) in painted.grapheme_indices(true) {
        let role = roles.at(at + offset);
        if role == run {
            continue;
        }
        push_run(&mut row, &painted[start..offset], run, palette);
        run = role;
        start = offset;
    }
    push_run(&mut row, &painted[start..], run, palette);
    row.push_str(past);
    row
}

/// One run of one role onto the row being built.
fn push_run(row: &mut String, text: &str, role: Role, palette: Palette) {
    if text.is_empty() {
        return;
    }
    match role.paint(palette) {
        None => row.push_str(text),
        Some(open) => {
            row.push_str(open);
            row.push_str(text);
            row.push_str(palette.reset());
        }
    }
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
/// The palette the one-shot fixtures paint in when a case did not name one.
///
/// A real one rather than a blank: the cases that hand it over are about text
/// that is [`Role::Plain`], and a plain row must come out carrying no sequence
/// of this module's **even when a palette is there to write**.
#[cfg(test)]
const DARK_256: Palette = Palette {
    mode: super::theme::Mode::Dark,
    depth: super::theme::Depth::Ansi256,
};

#[cfg(test)]
impl Transcript {
    /// Queues plain `text` and lands it at once, answering with what it wrote.
    ///
    /// Plain because that is what the cases reached through here are about --
    /// which rows a text makes at a width -- and because a role paints nothing
    /// onto them, so they read as the bytes they always did. A case about a
    /// *role* lands it at a palette of its own ([`Self::push_as`]).
    pub(crate) fn push(&mut self, text: &str) -> Append {
        self.push_as(text, Role::Plain, DARK_256)
    }

    /// The same, for text that means something.
    pub(crate) fn push_as(&mut self, text: &str, role: Role, palette: Palette) -> Append {
        self.land_own(palette, |transcript| {
            transcript.queue_push(text, role);
        })
    }

    /// Ends the current line and lands that, answering with what it wrote.
    pub(crate) fn end_line(&mut self) -> Append {
        self.end_line_at(DARK_256)
    }

    /// The same, painted in a palette the case chose.
    pub(crate) fn end_line_at(&mut self, palette: Palette) -> Append {
        self.land_own(palette, Self::queue_end_line_ignored)
    }

    /// How many rows of the screen the unfinished line occupies.
    pub(crate) fn tail_rows(&self) -> usize {
        self.painted
    }

    /// How many runs of a role the unfinished line is held as.
    ///
    /// The number the bound is about: it must follow the *text* that is still
    /// this module's, not the number of chunks that text arrived in.
    pub(crate) fn role_runs(&self) -> usize {
        self.roles.spans.len()
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
    fn land_own(&mut self, palette: Palette, enqueue: impl FnOnce(&mut Self)) -> Append {
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
            .emit_front(palette, |append| {
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
    roles: Roles,
    painted: usize,
    gutter: Gutter,
    kind: DocumentRow,
    block: bool,
    block_frozen: usize,
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

    // -----------------------------------------------------------------------
    // roles
    // -----------------------------------------------------------------------

    /// What xfx's own lines are painted in, dark -- written out rather than
    /// asked for: a case that read it off the palette would pass whatever the
    /// palette said, including nothing at all.
    const NOTICE_DARK: &str = "\u{1b}[38;5;250m";
    /// The light one's.
    const NOTICE_LIGHT: &str = "\u{1b}[38;5;241m";
    /// The quiet rows' grey (a turn's summary), dark.
    const DIM_DARK: &str = "\u{1b}[38;5;245m";
    /// What ends every one of them.
    const RESET: &str = "\u{1b}[0m";

    /// The light palette at 256 colours, for the cases about a mode switch.
    const LIGHT_256: Palette = Palette {
        mode: super::super::theme::Mode::Light,
        depth: super::super::theme::Depth::Ansi256,
    };

    /// `row` with the sequences this module writes taken back off, so a case
    /// can ask what the row *says* beside asking what it is painted in.
    ///
    /// The four are exactly what a role can add ([`Role::paint`] and the reset
    /// that closes it) and nothing else: a generic escape strip would also eat
    /// a provider's own attributes, which are text as far as this module is
    /// concerned and must survive every one of these cases.
    fn unpainted(row: &str) -> String {
        let mut text = row.to_string();
        for sequence in [
            NOTICE_DARK,
            NOTICE_LIGHT,
            DIM_DARK,
            // The dark notice and the dark dim said the way a direct-colour
            // terminal reads them.
            "\u{1b}[38;2;188;188;188m",
            "\u{1b}[38;2;138;138;138m",
            RESET,
        ] {
            text = text.replace(sequence, "");
        }
        text
    }

    #[test]
    fn a_summary_carries_the_palette_onto_every_row_it_wraps_onto() {
        // The break this catches: a dim row that arrives unpainted, or one
        // whose continuation row loses the colour the row above opened --
        // which is the whole of a turn's summary after the first row reading
        // as the terminal's default foreground. (The answer itself carries no
        // palette colour; xfx's own lines have this case as
        // `a_notice_that_wraps_carries_its_colour_onto_every_row`.)
        let mut transcript = Transcript::new(4);
        let append = transcript.push_as("abcdefgh", Role::Dim, DARK_256);

        assert_eq!(
            append.rows,
            vec![
                format!("{DIM_DARK}abcd{RESET}"),
                format!("{DIM_DARK}efgh{RESET}"),
            ]
        );
        // and the colour cost the row no cell: the text is wrapped at four
        // columns, not at four bytes of sequence plus four columns.
        assert_eq!(
            append
                .rows
                .iter()
                .map(|row| unpainted(row))
                .collect::<Vec<_>>(),
            vec!["abcd".to_string(), "efgh".to_string()]
        );
        assert_eq!(append.scroll, 2);
    }

    /// The rows of `appends`, as the **painter** puts them on the wire.
    ///
    /// `super::super::frame::row_text` is the real one: it removes what a row
    /// may not carry and cuts the rest to the screen. A case that asserted on
    /// the row this module hands over would be asserting about a string the
    /// terminal never sees the whole of.
    fn on_the_wire(appends: Vec<Append>, cols: u16) -> Vec<String> {
        appends
            .into_iter()
            .flat_map(|append| append.rows)
            .map(|row| super::super::frame::row_text(&row, cols).into_owned())
            .collect()
    }

    /// The foreground those rows leave a terminal in, read by the decoder the
    /// wire checker uses (`super::super::pacer::SgrState`) rather than by
    /// anything in this module.
    ///
    /// A different question from the per-row model a grid keeps: this is what
    /// the **stream** has left switched on after the last byte of the last
    /// row, which is exactly what the next row is drawn in.
    fn foreground(rows: &[String]) -> super::super::check::Color {
        let mut state = super::super::pacer::SgrState::default();
        for row in rows {
            state.observe(row);
        }
        state.color().expect("a slot this crate could have written")
    }

    #[test]
    fn a_row_wider_than_the_screen_closes_its_colour_where_the_painter_cuts() {
        // `wrap` hangs trailing spaces past the margin, so a row can be wider
        // than the screen it was measured for -- and `frame::clip` stops at
        // the first cluster that would not fit. A reset written at the end of
        // such a row is therefore **not on the wire**, and the run it was
        // meant to close runs on into the next document row. This phase never
        // repaints a document row, so it runs until something else writes a
        // colour.
        const COLS: u16 = 20;
        let mut transcript = Transcript::new(COLS);
        transcript.queue_push(&format!("{}   \n", "x".repeat(20)), Role::Dim);
        transcript.queue_push("a plain row after it", Role::Plain);
        let wire = on_the_wire(drain(&mut transcript, DARK_256), COLS);

        assert_eq!(
            wire[0],
            format!("{DIM_DARK}{}{RESET}", "x".repeat(20)),
            "the dim row's colour was not closed inside the cut"
        );
        assert_eq!(
            wire.last().expect("the plain row"),
            "a plain row after it",
            "the plain row was not handed over as itself"
        );
        assert_eq!(
            foreground(&wire),
            super::super::check::Color::Default,
            "the terminal was left painting in the dim row's colour"
        );
    }

    #[test]
    fn a_link_whose_text_wraps_is_closed_on_each_row_and_opened_again_on_the_next() {
        // Upstream's wrap (`assistant_wrap.zig:1306-1311`): the row the link
        // breaks on ends `\x1b[0m\x1b]8;;\x1b\\`, and the next opens the
        // attributes and then the link again -- so a click on either half goes
        // to the target, and no link is open across the `CUP` between them.
        const OPEN: &str = "\u{1b}]8;id=fx-1;https://example.com\u{1b}\\";
        const CLOSE: &str = "\u{1b}]8;;\u{1b}\\";
        let mut transcript = Transcript::new(6);
        let rows = transcript
            .push_as(
                &format!("{OPEN}\u{1b}[4mabc def\u{1b}[24m{CLOSE} tail"),
                Role::Plain,
                DARK_256,
            )
            .rows;
        assert_eq!(
            rows,
            vec![
                // The break space after `abc` is outside the link and the
                // underline: a blank cell, as upstream's wrap leaves it.
                format!("{OPEN}\u{1b}[4mabc{RESET}{CLOSE} "),
                format!("\u{1b}[4m{OPEN}def\u{1b}[24m{CLOSE} "),
                "tail".to_string(),
            ]
        );
        for row in rows.iter().map(|row| super::super::frame::row_text(row, 6)) {
            assert_eq!(
                row.matches("\u{1b}]8;id=").count(),
                row.matches(CLOSE).count(),
                "{row:?} leaves a link open"
            );
        }
    }

    #[test]
    fn a_notice_wider_than_the_screen_closes_its_colour_too() {
        // The same cut, for the other role that carries one. A notice is
        // usually xfx's own short line, and a long one is exactly where this
        // would be found by a user rather than by a case.
        const COLS: u16 = 20;
        let mut transcript = Transcript::new(COLS);
        transcript.queue_push(&format!("{}   \n", "n".repeat(20)), Role::Notice);
        transcript.queue_push("a plain row after it", Role::Plain);
        let wire = on_the_wire(drain(&mut transcript, DARK_256), COLS);

        assert_eq!(wire[0], format!("{NOTICE_DARK}{}{RESET}", "n".repeat(20)));
        assert_eq!(foreground(&wire), super::super::check::Color::Default);
    }

    #[test]
    fn a_notice_that_wraps_carries_its_colour_onto_every_row() {
        // Wrapping is not the answer's alone: a notice long enough to wrap
        // must keep its colour on the continuation row, and close it there.
        let mut transcript = Transcript::new(8);
        let rows = transcript
            .push_as("a notice that wraps", Role::Notice, DARK_256)
            .rows;

        assert_eq!(
            rows,
            vec![
                // The space this row hangs past the margin is kept, after the
                // reset: the rows still tile the text, and what the painter
                // cuts is exactly the part that is not drawn ([`drawn`]).
                format!("{NOTICE_DARK}a notice{RESET} "),
                format!("{NOTICE_DARK}that {RESET}"),
                format!("{NOTICE_DARK}wraps{RESET}"),
            ]
        );
        assert_eq!(
            rows.iter().map(|row| unpainted(row)).collect::<Vec<_>>(),
            vec![
                "a notice ".to_string(),
                "that ".to_string(),
                "wraps".to_string()
            ]
        );
    }

    #[test]
    fn a_role_that_changes_past_the_margin_opens_nothing_the_painter_would_cut() {
        // The mixed-role version of the cut: the run that would be opened for
        // the hanging spaces is entirely past the point the painter stops, so
        // opening it would put a sequence on the wire whose reset is not. What
        // the terminal is given ends with the first role's own reset.
        const COLS: u16 = 20;
        let mut transcript = Transcript::new(COLS);
        transcript.queue_push(&"x".repeat(20), Role::Notice);
        transcript.queue_push("   ", Role::Dim);
        let appends = drain(&mut transcript, DARK_256);
        let held = appends
            .last()
            .expect("the dim push's append")
            .rows
            .last()
            .expect("one row")
            .clone();
        let wire = on_the_wire(appends, COLS);

        assert_eq!(
            unpainted(&held),
            format!("{}   ", "x".repeat(20)),
            "the row stopped tiling the text it was wrapped from"
        );
        assert_eq!(
            wire.last().expect("the row"),
            &format!("{NOTICE_DARK}{}{RESET}", "x".repeat(20))
        );
        assert_eq!(foreground(&wire), super::super::check::Color::Default);
    }

    #[test]
    fn the_unfinished_line_is_repainted_in_the_palette_the_next_push_lands_in() {
        // Stated rather than avoided. The tail is the one text this module
        // still holds, and every push rewrites the whole of it -- so a push
        // that arrives after the terminal went light repaints the rows of the
        // unfinished line in the light palette, including the part that was
        // written while it was dark. That is the *owned* rows only: a finished
        // line is the terminal's and is never touched again
        // (`a_finished_line_is_left_in_the_document_and_the_next_one_starts_fresh`),
        // and U-B owns whatever repaints rows already on the screen.
        let mut transcript = Transcript::new(80);
        assert_eq!(
            transcript.push_as("half ", Role::Notice, DARK_256).rows,
            vec![format!("{NOTICE_DARK}half {RESET}")]
        );
        assert_eq!(
            transcript.push_as("and half", Role::Notice, LIGHT_256).rows,
            vec![format!("{NOTICE_LIGHT}half and half{RESET}")],
            "the rewritten tail kept the palette the first half landed in"
        );
    }

    #[test]
    fn a_row_is_opened_once_however_many_chunks_the_answer_arrived_in() {
        // The pacer turns one answer into a push every eight milliseconds
        // (`super::super::pacer`), and a sequence per chunk would put two
        // hundred of them on one row -- and would make the document a session
        // holds depend on how the socket cut the stream.
        let mut whole = Transcript::new(8);
        let at_once = whole.push_as("one answer", Role::Notice, DARK_256);

        let mut chunked = Transcript::new(8);
        let mut rows = Vec::new();
        for chunk in ["on", "e ans", "w", "er"] {
            chunked.queue_push(chunk, Role::Notice);
        }
        for append in drain(&mut chunked, DARK_256) {
            rows = append.rows;
        }

        assert_eq!(rows, at_once.rows, "the chunking changed the document");
        assert_eq!(
            rows,
            vec![
                format!("{NOTICE_DARK}one {RESET}"),
                format!("{NOTICE_DARK}answer{RESET}"),
            ]
        );
        for row in &rows {
            assert_eq!(
                row.matches(NOTICE_DARK).count(),
                1,
                "{row:?} was opened twice"
            );
            assert_eq!(row.matches(RESET).count(), 1, "{row:?} was closed twice");
        }
    }

    #[test]
    fn a_thousand_chunks_of_one_answer_are_held_as_one_run() {
        // The bound that keeps the metadata a property of the retained text.
        let mut transcript = Transcript::new(80);
        for _ in 0..1000 {
            transcript.queue_push("x", Role::Plain);
        }
        drain(&mut transcript, DARK_256);
        assert_eq!(transcript.role_runs(), 1);
    }

    #[test]
    fn a_line_the_user_began_is_not_recoloured_by_the_answer_that_extends_it() {
        // The echo of a submitted line and the answer that continues the same
        // logical line are different things, and the one that arrives second
        // must not repaint the one that is already on the screen.
        let mut transcript = Transcript::new(80);
        transcript.queue_push("you said", Role::Plain);
        transcript.queue_push(" and xfx answered", Role::Notice);
        let appends = drain(&mut transcript, DARK_256);

        assert_eq!(
            appends.last().expect("the answer's own append").rows,
            vec![format!("you said{NOTICE_DARK} and xfx answered{RESET}")]
        );
    }

    #[test]
    fn a_role_that_changes_mid_line_does_not_break_the_row() {
        // A break inserted where the role changed would put xfx's own sentence
        // on a row of its own -- a line the provider never wrote, and a row
        // the next append would measure its scroll against.
        let mut transcript = Transcript::new(80);
        transcript.queue_push("answer", Role::Dim);
        transcript.queue_push("[notice]", Role::Notice);
        let appends = drain(&mut transcript, DARK_256);
        let rows = appends.last().expect("the notice's append").rows.clone();

        assert_eq!(
            rows,
            vec![format!(
                "{DIM_DARK}answer{RESET}{NOTICE_DARK}[notice]{RESET}"
            )]
        );
        assert_eq!(unpainted(&rows[0]), "answer[notice]");
        assert_eq!(transcript.tail_rows(), 1, "the role change took a row");
    }

    #[test]
    fn a_break_inside_a_push_starts_the_next_line_with_its_own_roles() {
        // The offsets are the line's. A table left whole across a hard break
        // would describe bytes that are the terminal's now, and paint the new
        // line by the old line's runs.
        let mut transcript = Transcript::new(80);
        transcript.queue_push("said", Role::Plain);
        transcript.queue_push(" first\r\nanswer", Role::Notice);
        let appends = drain(&mut transcript, DARK_256);
        let rows: Vec<String> = appends.into_iter().flat_map(|append| append.rows).collect();

        assert_eq!(
            rows,
            vec![
                "said".to_string(),
                format!("said{NOTICE_DARK} first{RESET}"),
                format!("{NOTICE_DARK}answer{RESET}"),
            ]
        );
        assert_eq!(transcript.role_runs(), 1, "the finished line's runs stayed");
    }

    #[test]
    fn a_combining_mark_that_arrives_with_another_role_keeps_its_glyph_whole() {
        // A chunk boundary can fall between a letter and the mark that
        // composes with it. Opening a colour between the two would put a
        // sequence inside one grapheme cluster: terminals draw that as a
        // separate glyph, and the width the row was wrapped at is wrong by a
        // cell. The cluster takes the role of the byte it starts at.
        let mut transcript = Transcript::new(80);
        transcript.queue_push("e", Role::Plain);
        transcript.queue_push("\u{301}rror", Role::Notice);
        let appends = drain(&mut transcript, DARK_256);
        let row = appends
            .last()
            .expect("the second push's append")
            .rows
            .last()
            .expect("one row")
            .clone();

        assert_eq!(row, format!("e\u{301}{NOTICE_DARK}rror{RESET}"));
        assert_eq!(unpainted(&row), "e\u{301}rror");
        assert_eq!(
            wrap::width(&unpainted(&row)),
            5,
            "the glyph was split into two cells"
        );
    }

    #[test]
    fn a_frozen_tail_keeps_the_roles_of_the_bytes_it_kept() {
        // The tail stops growing at MAX_TAIL_ROWS, and what it drops it must
        // drop from **both** halves of itself. The roles either side of the
        // freeze are deliberately different: a span table left at the old
        // offsets describes bytes that are the terminal's now, so the row that
        // was kept would be painted in the role of the rows that were dropped.
        let mut transcript = Transcript::new(4);
        // Exactly the cap, none of it painted.
        transcript.push_as(&"x".repeat(4 * MAX_TAIL_ROWS), Role::Plain, DARK_256);
        assert_eq!(transcript.tail_rows(), MAX_TAIL_ROWS);

        // One row more, and it is a notice's. The freeze keeps this row.
        transcript.push_as("yyyy", Role::Notice, DARK_256);
        assert_eq!(transcript.tail_rows(), 1, "the tail was not frozen");
        assert_eq!(
            transcript.role_runs(),
            1,
            "the dropped rows' runs are still held"
        );
        assert_eq!(
            transcript.push_as("z", Role::Notice, DARK_256).rows,
            vec![
                format!("{NOTICE_DARK}yyyy{RESET}"),
                format!("{NOTICE_DARK}z{RESET}")
            ],
            "the kept row lost the role of the text still in it"
        );
    }

    #[test]
    fn text_the_terminal_refused_is_painted_in_the_palette_the_session_has_now() {
        // The whole reason the palette is a parameter of the write. A refusal
        // leaves the operation queued; the user switches their terminal to
        // light before the retry; the row that lands must read against the
        // background it lands on -- and nothing of what is queued is rewritten
        // to make that true.
        let mut transcript = Transcript::new(80);
        transcript.queue_push("a notice", Role::Notice);
        let refused = transcript.emit_front(DARK_256, |append| {
            assert_eq!(append.rows, vec![format!("{NOTICE_DARK}a notice{RESET}")]);
            Landed::None("the screen said no")
        });
        assert!(matches!(refused, Some(Err("the screen said no"))));

        let appends = drain(&mut transcript, LIGHT_256);
        assert_eq!(
            appends
                .into_iter()
                .flat_map(|append| append.rows)
                .collect::<Vec<_>>(),
            vec![format!("{NOTICE_LIGHT}a notice{RESET}")],
            "the retry was painted for the palette the refusal happened in"
        );
    }

    #[test]
    fn a_terminal_that_takes_direct_colour_is_given_the_same_rows() {
        // The two depths are one palette said twice
        // (`super::super::theme::truecolor`), so the longer sequence must not
        // move a break: the rows are measured from the text and the sequences
        // are added to them.
        let direct = Palette {
            mode: super::super::theme::Mode::Dark,
            depth: super::super::theme::Depth::TrueColor,
        };
        let mut transcript = Transcript::new(4);
        let rows = transcript.push_as("abcdefgh", Role::Notice, direct).rows;

        assert_eq!(
            rows,
            vec![
                format!("\u{1b}[38;2;188;188;188mabcd{RESET}"),
                format!("\u{1b}[38;2;188;188;188mefgh{RESET}"),
            ]
        );
        assert_eq!(
            rows.iter().map(|row| unpainted(row)).collect::<Vec<_>>(),
            vec!["abcd".to_string(), "efgh".to_string()]
        );
    }

    #[test]
    fn a_sequence_inside_the_text_is_text_to_this_module() {
        // Whatever bytes a role's text is made of, this module neither removes
        // nor re-orders them, and does not read one as a role change: what the
        // role adds is the run around the row.
        //
        // **A fixture, not a sample of production input.** `super::super::bridge`'s
        // `inert` spaces every escape out of a model or notice event before it
        // reaches the shell, and the painter would drop this one anyway -- a
        // `CSI 1 m` is not on the render allowlist
        // (`super::super::pacer::colour_at`), so it is asserted here, on the
        // rows this module hands over, and nowhere downstream.
        let mut transcript = Transcript::new(80);
        let rows = transcript
            .push_as("plain \u{1b}[1mbold\u{1b}[0m", Role::Notice, DARK_256)
            .rows;

        assert_eq!(
            rows,
            vec![format!("{NOTICE_DARK}plain \u{1b}[1mbold\u{1b}[0m{RESET}")]
        );
    }

    #[test]
    fn plain_text_is_handed_over_as_the_bytes_it_arrived_as() {
        // The user's echo, and anything else that is not xfx's to colour: a
        // palette is available at every one of these writes and none of its
        // roles may reach the row. The text's own bytes are kept; the one
        // thing added is the reset that closes a sequence the row left open,
        // because a terminal's pen survives the `CUP` to the next row.
        let mut transcript = Transcript::new(80);
        let rows = transcript
            .push_as("what the user typed \u{1b}[31m", Role::Plain, DARK_256)
            .rows;

        assert_eq!(
            rows,
            vec!["what the user typed \u{1b}[31m\u{1b}[0m".to_string()]
        );
        assert!(!rows[0].contains(DARK_256.notice()));
        assert!(!rows[0].contains(DARK_256.dim()));
    }

    #[test]
    fn the_first_fragment_takes_one_row_of_the_screen() {
        let mut transcript = Transcript::new(80);
        assert_eq!(
            transcript.push("answer"),
            Append {
                scroll: 1,
                rows: vec!["answer".to_string()],
                kinds: Vec::new(),
                release: 0,
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
                rows: vec!["answer".to_string()],
                kinds: Vec::new(),
                release: 0,
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
                rows: vec!["abcd".to_string(), "efgh".to_string()],
                kinds: Vec::new(),
                release: 0,
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
                rows: vec!["second".to_string()],
                kinds: Vec::new(),
                release: 0,
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
                rows: vec!["first".to_string(), "second".to_string()],
                kinds: Vec::new(),
                release: 0,
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
                rows: vec!["second!".to_string()],
                kinds: Vec::new(),
                release: 0,
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
                rows: vec!["a".to_string(), String::new(), "b".to_string()],
                kinds: Vec::new(),
                release: 0,
            }
        );
    }

    /// The rows an answer's `lines` make at `cols`, in the answer's gutter,
    /// as the document ends up holding them.
    fn answer_rows(cols: u16, lines: &str) -> Vec<String> {
        let mut transcript = Transcript::new(cols);
        transcript.queue_gutter(Gutter::Indent);
        transcript.queue_push(lines, Role::Plain);
        transcript.queue_end_line();
        replay(drain(&mut transcript, DARK_256))
    }

    #[test]
    fn an_answer_line_moves_a_word_down_rather_than_end_on_an_orphan() {
        // D2c's `fast-unconfirmed` at 40x120: upstream's row ends `... a
        // standard or` and the next is `lower tier.`, because `tier.` is the
        // last word of its line and overflows, so the word before it goes down
        // with it (`assistant_wrap.zig:269-298`, `isFinalWordOnLine`).
        let rows = answer_rows(
            120,
            "Ultrafast was requested, but Gateway did not confirm it was served; \
             this response may have used a standard or lower tier.\n\
             MARKER-ULTRA-UNCONFIRMED",
        );
        assert_eq!(
            rows.iter()
                .map(|row| row.trim_end().to_string())
                .collect::<Vec<_>>(),
            [
                "  Ultrafast was requested, but Gateway did not confirm it was served; \
                 this response may have used a standard or",
                "  lower tier.",
                "  MARKER-ULTRA-UNCONFIRMED",
            ]
        );
    }

    #[test]
    fn an_answer_link_or_style_that_wraps_is_closed_and_reopened_behind_the_lead() {
        // Upstream's wrap at a break (`assistant_wrap.zig:682-727`): close the
        // pen and the link, then the gutter and the continuation's lead, then
        // the pen and the link again -- so no link is open across the `CUP`
        // and the lead is never underlined.
        const OPEN: &str = "\u{1b}]8;id=fx-1;https://example.com\u{1b}\\";
        const CLOSE: &str = "\u{1b}]8;;\u{1b}\\";
        // "reopens an OSC 8 link after a word-balanced wrap", at six columns.
        let rows = answer_rows(8, &format!("{OPEN}\u{1b}[4malpha beta\u{1b}[24m{CLOSE}"));
        assert_eq!(
            rows,
            [
                format!("  {OPEN}\u{1b}[4malpha{RESET}{CLOSE} "),
                format!("  \u{1b}[4m{OPEN}beta\u{1b}[24m{CLOSE}"),
            ]
        );
        // "keeps styled list continuations unstyled at indentation".
        let rows = answer_rows(
            9,
            "\u{1b}[2m\u{2022} \u{1b}[22m\u{1b}[1mabcdefghij\u{1b}[22m",
        );
        assert_eq!(
            rows,
            [
                format!("  \u{1b}[2m\u{2022} \u{1b}[22m\u{1b}[1mabcde{RESET}"),
                "    \u{1b}[1mfghij\u{1b}[22m".to_string(),
            ]
        );
        // "repeats every styled nested blockquote rule on word-aware
        // continuations": the rules, dim, ahead of the bold.
        let rule = "\u{1b}[2m\u{2502} \u{1b}[22m";
        let rows = answer_rows(
            20,
            &format!("{rule}{rule}\u{1b}[1malpha beta gamma delta\u{1b}[22m"),
        );
        assert_eq!(
            rows,
            [
                format!("  {rule}{rule}\u{1b}[1malpha beta{RESET} "),
                format!("  {rule}{rule}\u{1b}[1mgamma delta\u{1b}[22m"),
            ]
        );
        for row in rows
            .iter()
            .map(|row| super::super::frame::row_text(row, 20))
        {
            assert_eq!(
                row.matches("\u{1b}]8;id=").count(),
                row.matches(CLOSE).count(),
                "{row:?} leaves a link open"
            );
        }
    }

    #[test]
    fn an_answer_tab_is_the_spaces_to_the_screens_next_stop() {
        // `assistant_wrap.zig:163-174`: the gutter's two columns and `a` put
        // the tab at screen column 4, and the next stop is column 9.
        assert_eq!(answer_rows(80, "a\tb"), ["  a     b"]);
        // The composer's wrap is what everything else keeps: four cells.
        let mut transcript = Transcript::new(80);
        assert_eq!(transcript.push("a\tb").rows, ["a\tb"]);
    }

    #[test]
    fn a_frozen_answer_line_keeps_its_continuation_rows_lined_up() {
        // A list item long enough to freeze: the rows the document ends up
        // with are the rows one wrap of the whole item gives, the indent under
        // the bullet included -- on the row a freeze kept and re-wrapped too.
        let cols = 12;
        let mut item = "\u{1b}[2m\u{2022} \u{1b}[22m".to_string();
        for index in 0..1200 {
            item.push_str(&format!("w{index} "));
        }
        let item = item.trim_end().to_string();
        let whole: Vec<String> = prose::prose(&item, cols - 2, 2, Opening::Line)
            .iter()
            .map(|laid| {
                let lead = laid.lead.map(Continuation::lead).unwrap_or_default();
                unpainted(&format!("  {lead}{}", &item[laid.row.start..laid.row.end]))
                    .trim_end()
                    .to_string()
            })
            .collect();
        assert!(
            whole.len() > MAX_TAIL_ROWS + 1,
            "the case is too short to freeze"
        );
        let rows: Vec<String> = answer_rows(cols, &item)
            .iter()
            .map(|row| unpainted(row).trim_end().to_string())
            .collect();
        assert_eq!(rows, whole);
        assert!(
            rows[1..].iter().all(|row| row.starts_with("    w")),
            "{rows:?}"
        );
        // And a resize re-measures the kept row with the carried indent.
        let mut transcript = Transcript::new(cols);
        transcript.queue_gutter(Gutter::Indent);
        transcript.queue_push(&item, Role::Plain);
        drain(&mut transcript, DARK_256);
        assert!(matches!(
            transcript.gutter,
            Gutter::Carried(Some(Continuation::Indent(2)))
        ));
        transcript.resize_unfinished(9);
        assert_eq!(transcript.tail_rows(), 1);
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
            document.truncate(document.len().saturating_sub(append.release));
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
            transcript.queue_push(chunk, Role::Plain);
        }
        replay(drain(&mut transcript, DARK_256))
    }

    /// Everything queued, offered to a terminal that takes all of it, as the
    /// appends it was offered.
    ///
    /// What the loop's drain does (`super::super::event_loop`'s
    /// `commit_document`) with a screen that never refuses.
    fn drain(transcript: &mut Transcript, palette: Palette) -> Vec<Append> {
        let mut offered = Vec::new();
        while transcript
            .emit_front(palette, |append| {
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
                rows: vec!["a".to_string(), String::new()],
                kinds: Vec::new(),
                release: 0,
            },
            "the line and the row its successor starts on"
        );
        assert_eq!(
            split.push("\nb"),
            Append {
                scroll: 0,
                rows: vec!["b".to_string()],
                kinds: Vec::new(),
                release: 0,
            },
            "the leading newline was answered a second time"
        );

        let mut whole = Transcript::new(80);
        assert_eq!(
            whole.push("a\r\nb"),
            Append {
                scroll: 2,
                rows: vec!["a".to_string(), "b".to_string()],
                kinds: Vec::new(),
                release: 0,
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
                rows: vec!["abcdefghijklmnopq".to_string()],
                kinds: Vec::new(),
                release: 0,
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
                rows: vec!["abcdefghij".to_string(), "klmnopqrs".to_string()],
                kinds: Vec::new(),
                release: 0,
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
        assert!(transcript.queue_push(&format!("{a}{b}{c}\n"), Role::Plain));

        transcript.resize_unfinished(40);
        let appends = drain(&mut transcript, DARK_256);

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
        assert!(transcript.queue_push(&format!("{a}{b}{c}\n"), Role::Plain));

        transcript.resize_unfinished(80);
        let appends = drain(&mut transcript, DARK_256);

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
        transcript.queue_push("abcdefghijklmnopqrstuvwxyz", Role::Plain);

        let refused = transcript.emit_front(DARK_256, |_| Landed::None("the screen said no"));
        assert!(matches!(refused, Some(Err(_))), "{refused:?}");
        assert_eq!(
            transcript.tail_rows(),
            0,
            "a refused write moved the rows this module claims are painted"
        );
        assert!(transcript.owes(), "a refused write stopped being owed");

        transcript.resize_unfinished(10);
        let appends = drain(&mut transcript, DARK_256);
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
        transcript.queue_push("def", Role::Plain);

        let torn = transcript.emit_front(DARK_256, |_| Landed::Prefix("half of it landed"));
        assert!(matches!(torn, Some(Err(_))), "{torn:?}");
        assert!(!transcript.owes(), "the torn write is still owed");

        transcript.queue_push("xyz", Role::Plain);
        let appends = drain(&mut transcript, DARK_256);
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
        assert!(transcript.queue_push("an answer", Role::Plain));
        assert!(
            !transcript.queue_end_line(),
            "ending a line that has text on it asks for no row of its own"
        );
        assert!(transcript.queue_push("a notice", Role::Plain));

        let appends = drain(&mut transcript, DARK_256);
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

        transcript.queue_push("an answer", Role::Plain);
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
        transcript.queue_push("a line\n", Role::Plain);
        assert!(transcript.open_line());
        assert!(
            !transcript.queue_end_line(),
            "the row the last break opened was counted as a second blank line"
        );
        // Which the drain confirms costs nothing: the blank row is already
        // written by the push that opened it.
        let appends = drain(&mut transcript, DARK_256);
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
        assert!(
            !transcript.queue_push("", Role::Plain),
            "an empty push asked for a row"
        );
        assert!(!transcript.owes(), "an empty push was queued");

        assert!(transcript.queue_push("a\r", Role::Plain));
        assert!(
            !transcript.queue_push("\n", Role::Plain),
            "the second half of a CRLF asked for a row of its own"
        );
        assert_eq!(
            drain(&mut transcript, DARK_256).len(),
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
        queued.queue_push("a\r", Role::Plain);
        queued.queue_end_line();
        queued.queue_push("\nb", Role::Plain);
        let held = replay(drain(&mut queued, DARK_256));

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
            transcript.queue_push(&format!("line {index}\n"), Role::Plain);
        }

        let appends = drain(&mut transcript, DARK_256);
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

    // -----------------------------------------------------------------------
    // the open block (a tool group)
    // -----------------------------------------------------------------------

    fn rows(texts: &[&str]) -> Vec<String> {
        texts.iter().map(|text| (*text).to_string()).collect()
    }

    #[test]
    fn rows_carry_what_they_are_from_the_kind_queued_before_their_line() {
        // A plain line has no record at all (most appends), an answer line's
        // rows all say so, and the kind changes only at a line boundary, so
        // the line before it keeps its own.
        let mut transcript = Transcript::new(20);
        transcript.queue_push("  1) Proceed?", Role::Plain);
        transcript.queue_kind(DocumentRow::Answer);
        transcript.queue_push("     a long answer that wraps", Role::Plain);
        transcript.queue_end_line();
        transcript.queue_kind(DocumentRow::Plain);
        transcript.queue_push("after", Role::Plain);
        transcript.queue_end_line();
        let appends = drain(&mut transcript, DARK_256);
        let recorded: Vec<(String, DocumentRow)> = appends
            .iter()
            .flat_map(|append| {
                append.rows.iter().enumerate().map(|(index, row)| {
                    (
                        row.clone(),
                        append.kinds.get(index).copied().unwrap_or_default(),
                    )
                })
            })
            .collect();
        assert!(
            recorded.contains(&("  1) Proceed?".to_string(), DocumentRow::Plain)),
            "{recorded:?}"
        );
        let answer: Vec<&(String, DocumentRow)> = recorded
            .iter()
            .filter(|(row, _)| row.contains("answer") || row.contains("wraps"))
            .collect();
        assert!(!answer.is_empty(), "{recorded:?}");
        assert!(
            answer.iter().all(|(_, kind)| *kind == DocumentRow::Answer),
            "{recorded:?}"
        );
        assert!(
            recorded.contains(&("after".to_string(), DocumentRow::Plain)),
            "{recorded:?}"
        );
        assert!(
            appends
                .iter()
                .filter(|append| append.rows.iter().all(|row| !row.contains("answer")))
                .all(|append| append.kinds.is_empty()),
            "a plain append carried a record: {appends:?}"
        );
    }

    #[test]
    fn a_block_carries_each_rows_kind_past_the_rows_it_froze() {
        let mut transcript = Transcript::new(40);
        transcript.queue_block(
            rows(&[
                "\u{25cf} header",
                "\u{2514} call",
                "",
                "\u{25a0} Cancelled call",
                "",
            ]),
            vec![
                DocumentRow::ToolGroup,
                DocumentRow::ToolGroup,
                DocumentRow::Plain,
                DocumentRow::ToolStopped,
                DocumentRow::Plain,
            ],
        );
        let appends = drain(&mut transcript, DARK_256);
        assert_eq!(
            appends[0].kinds,
            vec![
                DocumentRow::ToolGroup,
                DocumentRow::ToolGroup,
                DocumentRow::Plain,
                DocumentRow::ToolStopped,
                DocumentRow::Plain,
            ]
        );
        // An all-plain block records nothing.
        transcript.queue_block(rows(&["a", "b"]), vec![DocumentRow::Plain; 2]);
        assert!(drain(&mut transcript, DARK_256)
            .iter()
            .all(|append| append.kinds.is_empty()));
    }

    #[test]
    fn a_block_is_rewritten_where_it_stands_and_scrolls_only_the_rows_it_gained() {
        let mut transcript = Transcript::new(40);
        transcript.queue_block(rows(&["header 1", "\u{2514} one", ""]), Vec::new());
        transcript.queue_block(
            rows(&["header 2", "\u{251c} one", "\u{2514} two", ""]),
            Vec::new(),
        );
        transcript.queue_block(
            rows(&["header 2", "\u{251c} one", "\u{2514} two!", ""]),
            Vec::new(),
        );
        let appends = drain(&mut transcript, DARK_256);
        assert_eq!(
            appends
                .iter()
                .map(|append| append.scroll)
                .collect::<Vec<_>>(),
            [3, 1, 0],
            "a rewrite scrolled rows the block already had"
        );
        assert_eq!(
            replay(appends),
            ["header 2", "\u{251c} one", "\u{2514} two!", ""],
            "the block was not rewritten in place"
        );
        assert_eq!(transcript.tail_rows(), 4);
    }

    #[test]
    fn whatever_follows_a_block_starts_on_a_row_of_its_own_and_leaves_it_alone() {
        let mut transcript = Transcript::new(40);
        transcript.queue_push("the answer so far", Role::Plain);
        transcript.queue_block(rows(&["header", "\u{2514} call", ""]), Vec::new());
        transcript.queue_push("after", Role::Plain);
        transcript.queue_end_line();
        let appends = drain(&mut transcript, DARK_256);
        assert_eq!(
            replay(appends),
            ["the answer so far", "header", "\u{2514} call", "", "after"],
            "a block shared a row with the text around it"
        );
        // Closed by the text: a block queued now is a new one, under it.
        transcript.queue_block(rows(&["second"]), Vec::new());
        let appends = drain(&mut transcript, DARK_256);
        assert_eq!(appends.len(), 1);
        assert_eq!(appends[0].scroll, 1, "the second block rewrote the first");
    }

    #[test]
    fn a_block_that_lost_rows_blanks_the_ones_it_gave_up_and_gives_them_back() {
        // Blanked, because only a write clears a row; given back, because a
        // block is the document's last thing and what comes next -- the band,
        // or the next row -- belongs right under what it still has, as
        // upstream's re-render puts it.
        let mut transcript = Transcript::new(40);
        transcript.queue_block(rows(&["a", "b", "c"]), Vec::new());
        transcript.queue_block(rows(&["a"]), Vec::new());
        let appends = drain(&mut transcript, DARK_256);
        assert_eq!(appends[1].rows, ["a", "", ""]);
        assert_eq!((appends[1].scroll, appends[1].release), (0, 2));
        assert_eq!(replay(appends), ["a"]);
        assert_eq!(transcript.tail_rows(), 1);
        // And it grows again from the row it kept.
        transcript.queue_block(rows(&["a", "b"]), Vec::new());
        let appends = drain(&mut transcript, DARK_256);
        assert_eq!((appends[0].scroll, appends[0].release), (1, 0));
        assert_eq!(transcript.tail_rows(), 2);
    }

    #[test]
    fn a_block_row_wider_than_the_screen_is_cut_with_an_ellipsis_and_closed() {
        // `clipSummary`: one cell short, the ellipsis in it, and a colour the
        // cut left open closed after it.
        let mut transcript = Transcript::new(10);
        transcript.queue_block(
            vec![
                "\u{1b}[38;5;245m\u{251c} Ran something long\u{1b}[0m".to_string(),
                "short".to_string(),
            ],
            Vec::new(),
        );
        let written = replay(drain(&mut transcript, DARK_256));
        assert_eq!(
            written[0], "\u{1b}[38;5;245m\u{251c} Ran som\u{2026}\u{1b}[0m",
            "the row was not cut where upstream cuts it"
        );
        assert_eq!(super::super::wrap::width(&written[0]), 10);
        assert_eq!(written[1], "short");
    }

    #[test]
    fn a_resize_leaves_an_open_blocks_row_count_alone() {
        // One screen row each, at any width: re-measuring them as wrapped text
        // would make the next rewrite scroll rows the block already has.
        let mut transcript = Transcript::new(40);
        transcript.queue_block(rows(&["header", "\u{2514} call", ""]), Vec::new());
        drain(&mut transcript, DARK_256);
        transcript.resize_unfinished(5);
        assert_eq!(transcript.tail_rows(), 3);
        transcript.queue_block(rows(&["header", "\u{2514} done", ""]), Vec::new());
        let appends = drain(&mut transcript, DARK_256);
        assert_eq!(appends[0].scroll, 0);
        // Cut at the width it lands on: four cells and the ellipsis.
        assert_eq!(appends[0].rows[1], "\u{2514} do\u{2026}");
    }

    #[test]
    fn a_block_past_the_bound_stops_rewriting_its_oldest_rows() {
        // The bound a line has, for a block: every rewrite re-places every row
        // still held, so the rows past it -- more than any screen shows -- are
        // the terminal's.
        let mut transcript = Transcript::new(40);
        let tall: Vec<String> = (0..MAX_TAIL_ROWS + 10)
            .map(|n| format!("row {n}"))
            .collect();
        transcript.queue_block(tall.clone(), Vec::new());
        drain(&mut transcript, DARK_256);
        assert_eq!(transcript.tail_rows(), MAX_TAIL_ROWS);
        let mut taller = tall;
        taller.push("one more".to_string());
        transcript.queue_block(taller, Vec::new());
        let appends = drain(&mut transcript, DARK_256);
        assert_eq!(appends[0].scroll, 1);
        assert_eq!(appends[0].rows.len(), MAX_TAIL_ROWS + 1);
        assert_eq!(
            appends[0].rows[0], "row 10",
            "a frozen row was written again"
        );
    }

    /// What a notice's label is painted in on the dark palette, literal: the
    /// outcome grey (`theme.zig:86-88`).
    const LABEL_DARK: &str = "\u{1b}[38;5;252m";

    #[test]
    fn a_notice_head_puts_the_glyph_on_its_first_row_and_two_columns_on_the_rest() {
        // `renderSemanticNotice` (`transcript_blocks.zig:1758-1800`): the line is
        // `✗ command: Unknown command. Try /help.`, wrapped at the screen's
        // width, every continuation row indented by two. At 20 columns the
        // text behind the glyph has 18, so it breaks at the space after
        // `Unknown`, which is on neither row, and after the `/` of `/help`,
        // where upstream's notice wrap breaks a path (`scanNoticeLine`,
        // `transcript_blocks.zig:1674-1702`).
        let mut transcript = Transcript::new(20);
        transcript.queue_gutter(Gutter::Head(Tone::Error));
        // One operation for the label and the body, as the shell queues it.
        assert!(transcript.queue_push_runs(&[
            ("command:", Role::Label(Tone::Error)),
            (" ", Role::Notice),
            ("Unknown command. Try /help.", Role::Notice),
        ]));
        transcript.queue_end_line();
        transcript.queue_gutter(Gutter::None);
        let appends = drain(&mut transcript, DARK_256);
        assert_eq!(
            appends
                .iter()
                .filter(|append| !append.rows.is_empty())
                .count(),
            1,
            "the notice's line was written more than once: {appends:?}"
        );
        assert_eq!(
            screen(&appends),
            vec![
                format!(
                    "{LABEL_DARK}\u{2717} {RESET}{LABEL_DARK}command:{RESET}\
                     {NOTICE_DARK} Unknown{RESET}"
                ),
                format!("  {NOTICE_DARK}command. Try /{RESET}"),
                format!("  {NOTICE_DARK}help.{RESET}"),
            ]
        );
    }

    #[test]
    fn a_notice_heads_one_line_and_the_lines_after_it_are_continuations() {
        // A multi-line body (`* status: model=…` and its `k=v` rows) is one
        // head and then rows two columns in: the glyph is the first row's
        // only, however the lines arrive -- in one push or one per line.
        for chunks in [
            &["version:", " 1\nsecond\n\nthird"][..],
            &["version:", " 1", "\nsecond", "\n", "\nthird"][..],
        ] {
            let mut transcript = Transcript::new(40);
            transcript.queue_gutter(Gutter::Head(Tone::Neutral));
            for chunk in chunks {
                let role = if chunk.ends_with(':') {
                    Role::Label(Tone::Neutral)
                } else {
                    Role::Notice
                };
                transcript.queue_push(chunk, role);
            }
            transcript.queue_end_line();
            transcript.queue_gutter(Gutter::None);
            transcript.queue_push("after", Role::Plain);
            transcript.queue_end_line();
            let shown: Vec<String> = screen(&drain(&mut transcript, DARK_256))
                .iter()
                .map(|row| strip(row))
                .collect();
            assert_eq!(
                shown,
                vec!["* version: 1", "  second", "", "  third", "after"],
                "{chunks:?}"
            );
        }
    }

    #[test]
    fn a_frozen_notice_line_does_not_grow_a_second_glyph() {
        // The freeze drops the rows a line can no longer change, and the first
        // one -- the glyph's -- goes with them. What the line keeps begins on a
        // continuation row, so a re-wrap of it is indented, not headed.
        let mut transcript = Transcript::new(6);
        transcript.queue_gutter(Gutter::Head(Tone::Neutral));
        drain(&mut transcript, DARK_256);
        transcript.push_as(&"x".repeat(4 * (MAX_TAIL_ROWS + 1)), Role::Notice, DARK_256);
        assert_eq!(transcript.tail_rows(), 1, "the tail was not frozen");
        let rows = transcript.push_as("y", Role::Notice, DARK_256).rows;
        assert_eq!(
            rows,
            vec![
                format!("  {NOTICE_DARK}xxxx{RESET}"),
                format!("  {NOTICE_DARK}y{RESET}")
            ]
        );
    }

    #[test]
    fn whether_the_document_ends_on_a_blank_row_is_asked_of_the_queue() {
        // The projection a notice reads to decide whether it needs a blank row
        // above it: two notices in a row share the one between them.
        let mut transcript = Transcript::new(80);
        assert!(
            transcript.ends_blank(),
            "nothing written is not a row to put a gap under"
        );
        transcript.queue_push("a line", Role::Notice);
        transcript.queue_end_line();
        assert!(!transcript.ends_blank(), "a line of text read as blank");
        transcript.queue_end_line();
        assert!(transcript.ends_blank(), "the blank row was not counted");
        // A push that ends on a break leaves the row it opened empty, and
        // ending that row is ending a blank one.
        transcript.queue_push("an answer\n", Role::Plain);
        transcript.queue_end_line();
        assert!(transcript.ends_blank());
        transcript.queue_push("more", Role::Plain);
        assert!(!transcript.ends_blank());
    }

    /// The document a run of appends leaves, as the renderer would: each one
    /// rewrites the rows its tail already had and scrolls in the rest.
    fn screen(appends: &[Append]) -> Vec<String> {
        let mut document: Vec<String> = Vec::new();
        for append in appends {
            let held = append.rows.len() - append.scroll;
            document.truncate(document.len() - held);
            document.extend(append.rows.iter().cloned());
        }
        document
    }

    /// A row with every sequence taken out of it.
    fn strip(row: &str) -> String {
        let mut out = String::new();
        let mut chars = row.chars();
        while let Some(character) = chars.next() {
            if character == '\u{1b}' {
                for terminator in chars.by_ref() {
                    if terminator == 'm' {
                        break;
                    }
                }
                continue;
            }
            out.push(character);
        }
        out
    }
}
