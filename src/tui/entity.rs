//! What a draft's collapsed pastes are, while it holds them and after it is
//! put away.
//!
//! A [`super::paste`] block used to be a **name**: the composer held the words
//! `[Pasted text #1, 2 lines]` and the block behind them was found by searching
//! the draft for that string. A name is text a user can read, retype and paste
//! back, so that model needed an arbitration for which copy of the words was
//! the placeholder, it charged the budget for blocks a keystroke had already
//! made unsendable, and it re-read the whole draft once per block on every
//! keystroke -- which is why it needed a cap on how many blocks a draft could
//! hold.
//!
//! This module replaces the name with a **span**: the block is a run of *this*
//! draft, at these bytes, and it stays that run because every edit moves it.
//! What follows from that is the whole of item 16:
//!
//! * **A block is one unit.** Every motion steps over it
//!   ([`Entities::step_over`]) and every delete that so much as overlaps it
//!   takes the whole of it ([`Entities::delete_touching`]), so a caret is never
//!   inside a summary and a backspace at its edge removes the block rather than
//!   editing its name.
//! * **Words are only words.** A second copy of a summary -- typed, or pasted
//!   back off the screen -- is not at the span, so it is never expanded and it
//!   can never steal the block. There is nothing left to arbitrate.
//! * **The bookkeeping is arithmetic.** An edit shifts integers
//!   ([`Entities::shift_after_insert`]); the draft is read once at submit
//!   ([`Entities::expand`]) and once at a recall
//!   ([`Entities::renumber_recalled`]) and not at all in between. That is what
//!   [`scans`] counts, and what let the retained-block cap go.
//!
//! # Two lives, one shape
//!
//! [`Entities`] is the **live** set, measured against the composer's own text.
//! [`EntitySnapshot`] is what a [`super::history`] entry keeps: the same runs,
//! measured against the entry's fixed text, because the composer moves on and
//! the blocks beside it die with the draft they were pasted into. A recall
//! turns the snapshots back into spans and gives every one of them a **fresh**
//! number from the session's own allocator, so two live blocks can never answer
//! to one name.

use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::Arc;

/// How many times the paste bookkeeping has read the whole draft.
///
/// **The portable half of item 16's cost claim.** The cap this work removed
/// (`MAX_RETAINED_BLOCKS`) existed because the old model re-read the draft once
/// per retained block on every keystroke; the receipt that removing it is safe
/// is not a stopwatch -- a stopwatch measures this machine -- but a count that
/// does not move with the number of blocks.
///
/// Every function in this module that is handed the whole draft notes one scan,
/// and there are exactly two: [`Entities::expand`] and
/// [`Entities::renumber_recalled`]. A thread-local rather than a global,
/// because test threads run in parallel and a shared counter would report
/// another case's work as this one's.
#[cfg(test)]
pub(crate) mod scans {
    use std::cell::Cell;

    thread_local! {
        static TAKEN: Cell<usize> = const { Cell::new(0) };
    }

    pub(crate) fn note() {
        TAKEN.with(|taken| taken.set(taken.get().saturating_add(1)));
    }

    pub(crate) fn reset() {
        TAKEN.with(|taken| taken.set(0));
    }

    pub(crate) fn taken() -> usize {
        TAKEN.with(Cell::get)
    }
}

/// What a span stands for.
///
/// An enum with one variant, because the composer's other entities -- an
/// attachment, a file reference -- are the same span with different contents,
/// and a `Span` that assumed "paste" would have to be replaced rather than
/// extended when one arrives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EntityKind {
    /// A collapsed paste: the number its summary says, the text it stands for,
    /// and how many lines that text has.
    Paste {
        id: u32,
        /// `Arc<str>` rather than `String`: the same paste is in the composer
        /// and in every history entry that ever held it, and the byte budget
        /// allows it to be eight megabytes.
        text: Arc<str>,
        lines: usize,
    },
}

/// One entity: the run of the draft it occupies, and what it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Span {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) kind: EntityKind,
}

impl Span {
    /// The run of the draft the summary occupies.
    // The cases below are the readers: what the production code needs of a span
    // is answered by the methods on [`Entities`], and a caller handed a range
    // would be a caller doing arithmetic on offsets this module maintains.
    #[allow(dead_code)]
    pub(crate) fn range(&self) -> Range<usize> {
        self.start..self.end
    }

    /// The text the summary stands for.
    pub(crate) fn text(&self) -> &Arc<str> {
        match &self.kind {
            EntityKind::Paste { text, .. } => text,
        }
    }

    /// The number the summary says out loud.
    pub(crate) fn id(&self) -> u32 {
        match &self.kind {
            EntityKind::Paste { id, .. } => *id,
        }
    }

    /// How many lines the text has.
    pub(crate) fn lines(&self) -> usize {
        match &self.kind {
            EntityKind::Paste { lines, .. } => *lines,
        }
    }
}

/// Which way a motion is going, for the one question a motion asks of the
/// entities: is there a unit here, and where is its far side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Direction {
    Backward,
    Forward,
}

/// The entities a draft holds, in the order they appear in it.
#[derive(Debug, Default)]
pub(crate) struct Entities {
    /// Sorted by [`Span::start`] and never overlapping, which is what lets
    /// every question below be answered by one walk.
    spans: Vec<Span>,
    /// Which span a paste number names.
    ///
    /// Not a convenience: it is what makes "one live block per number" a
    /// property rather than an intention. A recall mints fresh numbers
    /// ([`Self::renumber_recalled`]) precisely so this map can never need two
    /// entries for one id, and a lookup that found the wrong one would expand
    /// somebody else's paste.
    by_paste: BTreeMap<u32, usize>,
}

impl Entities {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }

    pub(crate) fn len(&self) -> usize {
        self.spans.len()
    }

    // The two readers of the list and of the number index. Item 18's undo is
    // the first production caller of either -- it has to find the entity a
    // transaction names -- and until then they are how the cases below check
    // the invariants this module exists to keep.
    #[allow(dead_code)]
    pub(crate) fn spans(&self) -> &[Span] {
        &self.spans
    }

    /// The span a paste number names, while it is still in the draft.
    #[allow(dead_code)]
    pub(crate) fn of(&self, id: u32) -> Option<&Span> {
        self.by_paste.get(&id).map(|index| &self.spans[*index])
    }

    /// Every entity is gone -- the composer has been emptied or replaced.
    pub(crate) fn clear(&mut self) {
        self.spans.clear();
        self.by_paste.clear();
    }

    /// What the draft is holding out of sight: the bytes its blocks stand for.
    ///
    /// The budget's real subject. A collapsed block shows some 25 bytes and
    /// stands for as much as the whole cap, so a draft bounded on its own and a
    /// set of blocks bounded on their own are two ceilings that add up to twice
    /// the number either of them names -- and what leaves the session is their
    /// sum.
    pub(crate) fn retained(&self) -> usize {
        self.spans.iter().fold(0usize, |bytes, span| {
            bytes.saturating_add(span.text().len())
        })
    }

    /// Records a new entity: the caller has just put its summary into the draft
    /// at `span`'s bytes.
    pub(crate) fn register(&mut self, span: Span) {
        debug_assert!(
            !self.by_paste.contains_key(&span.id()),
            "two live blocks would answer to #{}",
            span.id()
        );
        debug_assert!(
            self.spans
                .iter()
                .all(|other| other.end <= span.start || span.end <= other.start),
            "a new entity overlaps one the draft already holds"
        );
        let at = self.spans.partition_point(|other| other.start < span.start);
        self.spans.insert(at, span);
        self.reindex();
    }

    /// `len` bytes went into the draft at `at`.
    ///
    /// Three answers, and the middle one is what makes an edge a place the
    /// caret can sit:
    ///
    /// * text at or before a span's first byte moves it along;
    /// * text at or after its last byte leaves it where it is -- so an
    ///   insertion at either **edge** of a unit lands beside it rather than in
    ///   it;
    /// * text **inside** it destroys it. That is unreachable from the keyboard
    ///   -- no motion lands the caret inside a unit -- and it is defined rather
    ///   than assumed away, because a span whose bytes are no longer the
    ///   summary it was measured on would expand a paste into the middle of a
    ///   word.
    pub(crate) fn shift_after_insert(&mut self, at: usize, len: usize) {
        let held = self.spans.len();
        self.spans
            .retain(|span| !(span.start < at && at < span.end));
        for span in &mut self.spans {
            if span.start >= at {
                span.start = span.start.saturating_add(len);
                span.end = span.end.saturating_add(len);
            }
        }
        // **Only when the list changed.** Moving spans changes neither their
        // numbers nor their order, so the index still points where it did --
        // and rebuilding it on every keystroke would put a `log n` back into
        // the cost this model exists to make constant.
        if self.spans.len() != held {
            self.reindex();
        }
    }

    /// The draft is about to lose `range`: what it really has to lose.
    ///
    /// **Any overlap takes the whole entity**, which is the whole of paste
    /// atomicity: a backspace at the right edge asks about one grapheme and is
    /// told to remove the block, and a kill that clips a summary removes it
    /// rather than leaving a damaged name behind. The answer is a range rather
    /// than a flag because the caller has to delete exactly what this widened
    /// to -- a caller that deleted its own range and asked this to forget the
    /// entity would leave the summary's other bytes on the screen.
    ///
    /// An **empty** range is not an overlap: a delete at the end of the text is
    /// a keystroke with nothing to do, not a keystroke that eats the block
    /// beside it.
    pub(crate) fn delete_touching(&mut self, range: Range<usize>) -> Range<usize> {
        let mut start = range.start;
        let mut end = range.end;
        if range.start < range.end {
            // Ascending, so a widening that reaches the next span is seen by
            // the same pass that caused it.
            for span in &self.spans {
                if span.start < end && start < span.end {
                    start = start.min(span.start);
                    end = end.max(span.end);
                }
            }
        }
        let taken = end.saturating_sub(start);
        let held = self.spans.len();
        self.spans
            .retain(|span| !(span.start < end && start < span.end));
        for span in &mut self.spans {
            if span.start >= end {
                span.start = span.start.saturating_sub(taken);
                span.end = span.end.saturating_sub(taken);
            }
        }
        // The index is rebuilt only when a block really went, for the reason
        // [`Self::shift_after_insert`] gives.
        if self.spans.len() != held {
            self.reindex();
        }
        start..end
    }

    /// The far side of the unit at `from`, for a motion going `direction`.
    ///
    /// `None` is "there is no unit here", which is every ordinary keystroke:
    /// the caller then moves by grapheme as it always did.
    pub(crate) fn step_over(&self, from: usize, direction: Direction) -> Option<usize> {
        match direction {
            Direction::Backward => self
                .spans
                .iter()
                .find(|span| span.start < from && from <= span.end)
                .map(|span| span.start),
            Direction::Forward => self
                .spans
                .iter()
                .find(|span| span.start <= from && from < span.end)
                .map(|span| span.end),
        }
    }

    /// The unit `at` is **strictly** inside, if it is inside one.
    ///
    /// The edges are not inside: they are where the caret sits when it is
    /// beside a unit. What this answers is the vertical moves' question -- the
    /// row above can put the wanted column in the middle of a summary, and a
    /// caret cannot be there.
    pub(crate) fn inside(&self, at: usize) -> Option<&Span> {
        self.spans
            .iter()
            .find(|span| span.start < at && at < span.end)
    }

    /// `draft` with every block put back where its summary stands.
    ///
    /// One pass, whatever the draft holds: the spans are sorted, so the text
    /// between them is copied in order and each block goes in once. Nothing
    /// that is put in is looked at again, so a paste whose *text* contains
    /// another summary -- a pasted transcript is the ordinary way that happens
    /// -- cannot expand a second time.
    pub(crate) fn expand(&self, draft: &str) -> String {
        #[cfg(test)]
        scans::note();
        if self.spans.is_empty() {
            return draft.to_string();
        }
        let mut out = String::with_capacity(draft.len().saturating_add(self.retained()));
        let mut cut = 0usize;
        for span in &self.spans {
            out.push_str(&draft[cut..span.start]);
            out.push_str(span.text());
            cut = span.end;
        }
        out.push_str(&draft[cut..]);
        out
    }

    /// What a history entry has to remember to be able to put these blocks
    /// back.
    pub(crate) fn snapshots(&self) -> Vec<EntitySnapshot> {
        self.spans
            .iter()
            .map(|span| {
                EntitySnapshot::new(span.start, span.end, Arc::clone(span.text()), span.lines())
            })
            .collect()
    }

    /// The spans an entry remembered, ready to be renumbered into a draft.
    ///
    /// The numbers are **provisional** -- the entry's own belong to a draft
    /// that has been submitted and forgotten, and reusing one would put two
    /// live blocks under one name. [`Self::renumber_recalled`] is what makes
    /// them real, and it is the only thing that should be called on the result.
    pub(crate) fn recalled(snapshots: &[EntitySnapshot]) -> Self {
        let mut entities = Self::new();
        for (index, snapshot) in snapshots.iter().enumerate() {
            let id = u32::try_from(index).unwrap_or(u32::MAX);
            entities.spans.push(Span {
                start: snapshot.span().start,
                end: snapshot.span().end,
                kind: EntityKind::Paste {
                    id,
                    text: Arc::clone(snapshot.text()),
                    lines: snapshot.lines(),
                },
            });
        }
        entities.reindex();
        entities
    }

    /// Gives every recalled block a number this session has not used, and
    /// rewrites the draft to say it.
    ///
    /// **Checked, and a refusal rather than a wrap.** `next` is the session's
    /// one allocator -- the same counter a paste spends
    /// (`super::paste::Paste`) -- so a number minted here can never be minted
    /// again. At the end of the space the remaining blocks are **dropped**:
    /// their summaries stay in the draft as the words they look like, which is
    /// the only honest thing a draft can show for a block it cannot name. The
    /// caller sees a shorter set of entities and says so on the hint row.
    ///
    /// The draft is rebuilt rather than patched, because a new number can be a
    /// digit longer or shorter than the old one and every span after it moves
    /// by that difference.
    pub(crate) fn renumber_recalled(&mut self, draft: &mut String, next: &mut u32) {
        #[cfg(test)]
        scans::note();
        if self.spans.is_empty() {
            return;
        }
        let mut out = String::with_capacity(draft.len());
        let mut kept: Vec<Span> = Vec::with_capacity(self.spans.len());
        let mut cut = 0usize;
        let mut exhausted = false;
        for span in std::mem::take(&mut self.spans) {
            if exhausted {
                continue;
            }
            let Some(id) = next.checked_add(1) else {
                exhausted = true;
                continue;
            };
            let name = super::paste::summary(id, span.lines());
            out.push_str(&draft[cut..span.start]);
            let start = out.len();
            out.push_str(&name);
            cut = span.end;
            *next = id;
            kept.push(Span {
                start,
                end: out.len(),
                kind: EntityKind::Paste {
                    id,
                    text: Arc::clone(span.text()),
                    lines: span.lines(),
                },
            });
        }
        out.push_str(&draft[cut..]);
        *draft = out;
        self.spans = kept;
        self.reindex();
    }

    /// Rebuilds the number index from the spans.
    ///
    /// Rebuilt rather than patched: every mutation above can move, drop or
    /// renumber several spans at once, and an index maintained in pieces is an
    /// index that disagrees with the list it points into.
    fn reindex(&mut self) {
        self.by_paste.clear();
        for (index, span) in self.spans.iter().enumerate() {
            self.by_paste.insert(span.id(), index);
        }
    }
}

/// One collapsed paste, as the line it was in remembers it.
///
/// The span is in **bytes of the entry's own text**, not of the composer's:
/// the entry is the fixed thing and the composer is what changes under it, so
/// a span measured against the composer would be a span that means something
/// different every keystroke.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EntitySnapshot {
    /// Where the summary begins in the entry's text.
    start: usize,
    /// Where it ends. `start..end` is the run the summary occupies.
    end: usize,
    /// The text the summary stands for.
    ///
    /// `Arc<str>` rather than `String`, because the same paste can be in the
    /// composer and in a hundred entries at once: a history that copied it
    /// would hold a hundred copies of what the byte budget allows to be eight
    /// megabytes.
    text: Arc<str>,
    /// How many lines that text has -- the number the summary says out loud.
    lines: usize,
}

impl EntitySnapshot {
    pub(crate) fn new(start: usize, end: usize, text: Arc<str>, lines: usize) -> Self {
        Self {
            start,
            end,
            text,
            lines,
        }
    }

    /// The run of the entry's text the summary occupies.
    pub(crate) fn span(&self) -> Range<usize> {
        self.start..self.end
    }

    /// The text the summary stands for.
    pub(crate) fn text(&self) -> &Arc<str> {
        &self.text
    }

    /// How many lines that text has.
    pub(crate) fn lines(&self) -> usize {
        self.lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::paste::summary;

    /// One block, as the composer registers it.
    fn kind(id: u32, text: &str) -> EntityKind {
        EntityKind::Paste {
            id,
            text: Arc::from(text),
            lines: text.lines().count(),
        }
    }

    /// A draft made of `before` and one block's summary, and the entities that
    /// name it.
    fn one(before: &str, id: u32, text: &str) -> (String, Entities) {
        let name = summary(id, text.lines().count());
        let mut draft = String::from(before);
        let start = draft.len();
        draft.push_str(&name);
        let mut entities = Entities::new();
        entities.register(Span {
            start,
            end: draft.len(),
            kind: kind(id, text),
        });
        (draft, entities)
    }

    /// A draft holding `count` blocks separated by a space, numbered from one.
    fn many(count: usize) -> (String, Entities) {
        let mut draft = String::new();
        let mut entities = Entities::new();
        for index in 0..count {
            let id = u32::try_from(index + 1).expect("a test never mints that many");
            let name = summary(id, 1);
            if index > 0 {
                draft.push(' ');
            }
            let start = draft.len();
            draft.push_str(&name);
            entities.register(Span {
                start,
                end: draft.len(),
                kind: kind(id, "yyy"),
            });
        }
        (draft, entities)
    }

    fn span(entities: &Entities, index: usize) -> std::ops::Range<usize> {
        entities.spans()[index].range()
    }

    #[test]
    fn text_typed_in_front_of_a_block_moves_it_and_text_behind_it_does_not() {
        // The whole of what a span is for: the block is a run of *this* draft,
        // and the draft changes under it every keystroke.
        let (_draft, mut entities) = one("see ", 1, "y\ny");
        let was = span(&entities, 0);
        entities.shift_after_insert(0, 2);
        assert_eq!(span(&entities, 0), was.start + 2..was.end + 2);
        entities.shift_after_insert(was.end + 2, 3);
        assert_eq!(
            span(&entities, 0),
            was.start + 2..was.end + 2,
            "text typed behind the block moved it"
        );
        // At its own first byte the text goes *in front of* the block: the
        // caret sits between what was typed and the unit it is beside.
        entities.shift_after_insert(was.start + 2, 1);
        assert_eq!(span(&entities, 0), was.start + 3..was.end + 3);
    }

    #[test]
    fn text_that_lands_inside_a_block_destroys_it_rather_than_stretching_it() {
        // Unreachable from the keyboard -- the caret never lands inside a unit
        // -- and defined anyway, because the alternative is a span whose bytes
        // are no longer the summary it was measured on.
        let (_draft, mut entities) = one("", 1, "y");
        entities.shift_after_insert(3, 1);
        assert!(
            entities.is_empty(),
            "a block swallowed text that was written into its name"
        );
    }

    #[test]
    fn a_delete_that_overlaps_a_block_takes_the_whole_of_it() {
        // Both edges, because both are what a user reaches for: Backspace at
        // the right edge and Delete at the left one.
        let (draft, mut entities) = one("see ", 1, "y");
        let whole = span(&entities, 0);
        let taken = entities.delete_touching(draft.len() - 1..draft.len());
        assert_eq!(taken, whole, "the backspace edited the name instead");
        assert!(entities.is_empty());

        let (_draft, mut entities) = one("see ", 1, "y");
        let whole = span(&entities, 0);
        let taken = entities.delete_touching(whole.start..whole.start + 1);
        assert_eq!(taken, whole, "the delete edited the name instead");
        assert!(entities.is_empty());
    }

    #[test]
    fn a_delete_that_only_abuts_a_block_leaves_it_alone() {
        // The edges are the unit's own bytes, so a deletion that stops at one
        // has not touched it: an empty range is not an overlap either.
        let (draft, mut entities) = one("see ", 1, "y");
        let whole = span(&entities, 0);
        assert_eq!(
            entities.delete_touching(whole.start - 1..whole.start),
            whole.start - 1..whole.start
        );
        assert_eq!(span(&entities, 0), whole.start - 1..whole.end - 1);

        let mut entities = one("see ", 1, "y").1;
        let whole = span(&entities, 0);
        assert_eq!(
            entities.delete_touching(whole.end..whole.end),
            whole.end..whole.end
        );
        assert_eq!(span(&entities, 0), whole, "an empty range took a block");
        assert_eq!(draft.len(), whole.end);
    }

    #[test]
    fn the_blocks_after_a_deleted_one_move_up_by_what_it_took() {
        let (_draft, mut entities) = many(3);
        let first = span(&entities, 0);
        let second = span(&entities, 1);
        let taken = entities.delete_touching(first.start..first.start + 1);
        assert_eq!(taken, first.clone());
        assert_eq!(entities.len(), 2);
        assert_eq!(
            span(&entities, 0),
            second.start - first.len()..second.end - first.len()
        );
    }

    #[test]
    fn a_move_steps_over_the_whole_unit_from_either_edge() {
        let (draft, entities) = one("see ", 1, "y");
        let whole = span(&entities, 0);
        assert_eq!(
            entities.step_over(whole.end, Direction::Backward),
            Some(whole.start),
            "a step back from the right edge landed inside the name"
        );
        assert_eq!(
            entities.step_over(whole.start, Direction::Forward),
            Some(whole.end),
            "a step forward from the left edge landed inside the name"
        );
        assert_eq!(entities.step_over(whole.start, Direction::Backward), None);
        assert_eq!(entities.step_over(whole.end, Direction::Forward), None);
        assert_eq!(entities.step_over(0, Direction::Forward), None);
        assert_eq!(draft.len(), whole.end);
    }

    #[test]
    fn a_point_inside_a_unit_is_reported_and_its_edges_are_not() {
        let (_draft, entities) = one("see ", 1, "y");
        let whole = span(&entities, 0);
        assert!(entities.inside(whole.start).is_none());
        assert!(entities.inside(whole.end).is_none());
        assert!(entities.inside(whole.start + 1).is_some());
    }

    #[test]
    fn every_block_goes_back_where_it_stands_and_words_that_look_like_one_do_not() {
        // The whole reason a span replaces a name search: the draft below holds
        // *three* copies of `[Pasted text #1, 1 lines]` and exactly one of them
        // is the block.
        let (mut draft, mut entities) = one(&format!("{} see ", summary(1, 1)), 1, "y");
        let tail = format!(" {}", summary(1, 1));
        draft.push_str(&tail);
        let name = summary(1, 1);
        assert_eq!(draft.matches(&name).count(), 3);
        assert_eq!(
            entities.expand(&draft),
            format!("{name} see y {name}"),
            "a typed copy of the name was expanded as if it were the block"
        );
        entities.clear();
        assert_eq!(entities.expand(&draft), draft);
    }

    #[test]
    fn expansion_reads_the_draft_once_however_many_blocks_it_holds() {
        // The portable half of item 16's cost claim, and what makes the
        // retained-block cap unnecessary rather than merely inconvenient: the
        // work is one pass over the draft, not one pass per block.
        for count in [64usize, 1000] {
            let (draft, entities) = many(count);
            scans::reset();
            let prompt = entities.expand(&draft);
            assert_eq!(
                scans::taken(),
                1,
                "{count} blocks cost {} reads of the draft",
                scans::taken()
            );
            assert_eq!(
                prompt.matches("yyy").count(),
                count,
                "not every block was put back"
            );
            assert!(
                !prompt.contains("Pasted text"),
                "a summary survived: {prompt:.60}"
            );
        }
    }

    #[test]
    fn a_keystroke_reads_the_draft_no_times_however_many_blocks_it_holds() {
        // The bookkeeping an edit owes is arithmetic on the spans, and the
        // draft is not read at all -- which is the claim `MAX_RETAINED_BLOCKS`
        // existed to make false.
        for count in [64usize, 1000] {
            let (_draft, mut entities) = many(count);
            scans::reset();
            entities.shift_after_insert(0, 1);
            let last = entities.spans().last().expect("a block").range();
            entities.step_over(last.end, Direction::Backward);
            entities.delete_touching(last.end - 1..last.end);
            assert_eq!(
                scans::taken(),
                0,
                "{count} blocks made an edit read the draft {} time(s)",
                scans::taken()
            );
            assert_eq!(entities.len(), count - 1);
        }
    }

    #[test]
    fn a_recalled_line_takes_fresh_numbers_and_the_draft_says_them() {
        // A recalled entry is a *new* draft: the numbers it shows have to be
        // numbers this session has not used, and the summaries on the screen
        // have to be the ones it just minted -- including when the new number
        // is a digit longer than the old one, which moves every span after it.
        let (first, mut entities) = one("see ", 1, "y");
        let mut draft = first;
        draft.push_str(" and ");
        let start = draft.len();
        draft.push_str(&summary(2, 2));
        entities.register(Span {
            start,
            end: draft.len(),
            kind: kind(2, "z\nz"),
        });

        let mut next = 9u32;
        entities.renumber_recalled(&mut draft, &mut next);
        assert_eq!(
            next, 11,
            "the session's numbering did not move with the recall"
        );
        assert_eq!(
            draft,
            format!("see {} and {}", summary(10, 1), summary(11, 2)),
            "the draft still shows the numbers the entry was recorded with"
        );
        assert_eq!(
            entities.expand(&draft),
            "see y and z\nz",
            "the spans no longer line up with the names they were rewritten as"
        );
        assert!(entities.of(10).is_some() && entities.of(11).is_some());
        assert!(
            entities.of(1).is_none(),
            "an old number still names a block"
        );
    }

    #[test]
    fn a_recall_that_runs_out_of_numbers_drops_the_blocks_it_cannot_name() {
        // Checked, never wrapped and never reused: a second block answering to
        // a live number is a recall that expands somebody else's paste.
        //
        // **Two blocks**, because the interesting one is the second: the first
        // fails at the allocator, and every block after it has to be dropped by
        // the exhaustion the first one found rather than tried again.
        let (mut draft, mut entities) = many(2);
        let words = draft.clone();
        let mut next = u32::MAX;
        entities.renumber_recalled(&mut draft, &mut next);
        assert_eq!(next, u32::MAX, "a number was reused or wrapped");
        assert!(
            entities.is_empty(),
            "a block was kept under a number nobody minted"
        );
        assert_eq!(draft, words, "the draft was rewritten anyway");
        assert_eq!(entities.expand(&draft), words, "the summary is words now");
    }

    #[test]
    fn a_number_names_exactly_one_live_block() {
        let (_draft, mut entities) = many(3);
        assert_eq!(
            entities.of(1).map(|span| span.range()),
            Some(span(&entities, 0))
        );
        assert_eq!(
            entities.of(3).map(|span| span.range()),
            Some(span(&entities, 2))
        );
        assert!(entities.of(4).is_none());
        let first = span(&entities, 0);
        entities.delete_touching(first.start..first.start + 1);
        assert!(
            entities.of(1).is_none(),
            "a deleted block still answers to its number"
        );
        assert!(entities.of(2).is_some());
    }

    #[test]
    fn what_an_entry_remembers_comes_back_as_the_spans_it_stood_on() {
        // The carrier item 15 recorded empty, filled in and read back: the
        // recalled draft expands to what the original one would have sent.
        let (draft, entities) = many(2);
        let snapshots = entities.snapshots();
        assert_eq!(snapshots.len(), 2);
        assert_eq!(snapshots[0].span(), span(&entities, 0));
        assert_eq!(&**snapshots[0].text(), "yyy");

        let mut recalled_draft = draft.clone();
        let mut recalled = Entities::recalled(&snapshots);
        let mut next = 2u32;
        recalled.renumber_recalled(&mut recalled_draft, &mut next);
        assert_eq!(
            recalled.expand(&recalled_draft),
            entities.expand(&draft),
            "the recalled line does not send what the recorded one would have"
        );
        assert_eq!(
            recalled_draft,
            format!("{} {}", summary(3, 1), summary(4, 1))
        );
    }
}
