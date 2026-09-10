//! What the composer did, small enough to hold and exact enough to take back.
//!
//! A draft is up to eight megabytes ([`super::editor::MAX_COMPOSER_BYTES`]) and
//! a keystroke changes a handful of bytes of it, so the unit of history here is
//! the **difference** rather than the draft: what was removed, what went in, and
//! where. That is upstream's shape too -- `edit_history.zig:86` counts a delta as
//! `removed.len + inserted.len` -- and it is the whole reason a hundred entries
//! can be held at once. A history of whole-draft copies would be a gigabyte for
//! the same hundred keystrokes, which is why the one-entry seam this module
//! replaces held exactly one and never grew a stack.
//!
//! Three rules make it bounded rather than merely small:
//!
//! * **The two stacks are budgeted together.** [`MAX_ENTRIES`] entries and
//!   [`MAX_RETAINED_BYTES`] bytes are the sum of undo and redo, because an entry
//!   moved from one to the other is the same bytes in the same process
//!   (`edit_history.zig:5-6`). Moving one therefore changes nothing at all: no
//!   subtract on the pop, no add on the push, and no way for the total to drift.
//! * **A delta too big to hold is a boundary, not an entry.** Upstream's
//!   `prepare` answers `.boundary` above the byte cap and `commit` maps that to
//!   `reset` (`edit_history.zig:86-87,99-101`), which clears *both* stacks. A
//!   history that cannot hold the paste must not claim it can undo it.
//! * **Recording clears redo, and gives its bytes back.** `clearStack` subtracts
//!   as it frees, which is what lets `reset` assert the total is zero
//!   (`edit_history.zig:71-75`). Undoing fifty entries and then typing one
//!   character must not leave fifty entries' worth of bytes counted forever.
//!
//! The kill ring is beside the history rather than in it, with a budget of its
//! own: upstream's is a distinct `State` (`kill_ring.zig:45-50`) from
//! `edit_history.State` (`edit_history.zig:59-61`), and folding killed text into
//! the history's bytes would let one large kill evict undo entries that have
//! nothing to do with it. It holds one slot, and a second kill **replaces** it
//! rather than appending to it (`kill_ring.zig:86-116` builds a fresh payload,
//! swaps, and frees the previous -- there is no append arm).

use std::mem::size_of;

use super::entity::Span;

/// The most entries the two stacks hold between them (`edit_history.zig:5`).
pub(crate) const MAX_ENTRIES: usize = 100;

/// The most bytes they retain between them (`edit_history.zig:6`).
pub(crate) const MAX_RETAINED_BYTES: usize = 1 << 20;

/// The most a single kill may park in the slot.
///
/// xfx-local, and separate from [`MAX_RETAINED_BYTES`] on purpose. A kill over
/// this **empties** the slot rather than truncating it: half a killed line
/// yanked back is text nobody killed, and a truncated summary would yank a
/// block marker standing for bytes that are no longer behind it.
pub(crate) const MAX_KILL_BYTES: usize = 1 << 20;

/// What kind of edit made a delta, for the two questions the kind decides:
/// whether it loads the kill slot, and whether it is one gesture or many.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeltaKind {
    /// A typed character, a backspace, a forward delete, a yank.
    Ordinary,
    /// `C-k`, `C-u`, `C-w`: what it removed goes to the kill slot.
    Kill,
    /// One framed paste, however many reads it arrived in -- the boundary the
    /// one-entry seam this module replaces used to hold on its own.
    Paste,
}

/// One difference between two drafts.
///
/// Produced by [`super::editor::Editor`] and by nothing else: the offsets in it
/// are the editor's own, widened where the editor widened them, and a second
/// producer would be a second place that does arithmetic on them
/// (`entity.rs:113-116`).
#[derive(Debug, Clone)]
pub(crate) struct Delta {
    at: usize,
    removed: String,
    inserted: String,
    removed_entities: Vec<Span>,
    inserted_entities: Vec<Span>,
    caret_before: usize,
    caret_after: usize,
    kind: DeltaKind,
}

impl Delta {
    /// The editor is the only producer: it is the module that owns the offsets.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        at: usize,
        removed: String,
        inserted: String,
        removed_entities: Vec<Span>,
        inserted_entities: Vec<Span>,
        caret_before: usize,
        caret_after: usize,
        kind: DeltaKind,
    ) -> Self {
        Self {
            at,
            removed,
            inserted,
            removed_entities,
            inserted_entities,
            caret_before,
            caret_after,
            kind,
        }
    }

    /// Where the change begins -- the **widened** start, when an entity made the
    /// editor take more than the keystroke asked for (`editor.rs:374-376`).
    pub(crate) fn at(&self) -> usize {
        self.at
    }

    /// What the draft lost.
    pub(crate) fn removed(&self) -> &str {
        &self.removed
    }

    /// What the draft gained.
    pub(crate) fn inserted(&self) -> &str {
        &self.inserted
    }

    /// The blocks that went with the removed text, at their offsets in the draft
    /// that still held them.
    ///
    /// Whole [`Span`]s rather than `EntitySnapshot`s, which carry no id
    /// (`entity.rs:468-484`): an undo has to put the *same* number back, because
    /// the summary on the screen says it out loud. The clone is cheap -- the
    /// payload is an `Arc` (`entity.rs:91-96`).
    pub(crate) fn removed_entities(&self) -> &[Span] {
        &self.removed_entities
    }

    /// The blocks the inserted text brought, at their offsets in the draft that
    /// holds them now.
    pub(crate) fn inserted_entities(&self) -> &[Span] {
        &self.inserted_entities
    }

    /// Where the caret was before the change.
    pub(crate) fn caret_before(&self) -> usize {
        self.caret_before
    }

    /// Where the change left it.
    pub(crate) fn caret_after(&self) -> usize {
        self.caret_after
    }

    pub(crate) fn kind(&self) -> DeltaKind {
        self.kind
    }

    /// What holding this costs.
    ///
    /// Upstream counts `removed.len +| inserted.len` (`edit_history.zig:86`)
    /// because it stores raw bytes. xfx also holds `Arc<str>` payloads behind
    /// its summaries, so those are counted too, at their full length and once
    /// per span: an over-count of a shared `Arc` is the safe direction for a cap
    /// whose job is to refuse to hold too much.
    pub(crate) fn weight(&self) -> usize {
        let text = self.removed.len().saturating_add(self.inserted.len());
        self.removed_entities
            .iter()
            .chain(self.inserted_entities.iter())
            .fold(text, |bytes, span| {
                bytes
                    .saturating_add(span.text().len())
                    .saturating_add(size_of::<Span>())
            })
    }

    /// Whether this delta changed anything at all.
    fn is_empty(&self) -> bool {
        self.removed.is_empty() && self.inserted.is_empty()
    }
}

/// The one slot `C-y` yanks from.
#[derive(Debug)]
struct Killed {
    text: String,
    /// The blocks inside the killed text, at offsets **relative to it** rather
    /// than to the draft it came out of: what a yank does with them is put them
    /// somewhere else entirely.
    entities: Vec<Span>,
}

/// The undo stack, the redo stack, and the kill slot.
#[derive(Debug, Default)]
pub(crate) struct EditHistory {
    /// Oldest first: eviction takes the front (`edit_history.zig:102-114`).
    undo: Vec<Delta>,
    redo: Vec<Delta>,
    /// The bytes held by the two stacks **together**.
    retained: usize,
    killed: Option<Killed>,
}

impl EditHistory {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Remembers one edit, and forgets whatever it made impossible.
    ///
    /// The order matters and it is upstream's: the kill slot is loaded first,
    /// because a kill too big for the **history** is still a kill and the slot
    /// has a budget of its own; then the oversized delta becomes a boundary
    /// (`edit_history.zig:86-87,99-101`); then redo is cleared and its bytes
    /// given back, the entry is pushed, and the two caps evict from the front.
    pub(crate) fn record(&mut self, delta: Delta) {
        debug_assert!(
            !delta.is_empty(),
            "an edit that changed nothing was recorded as history"
        );
        let weight = delta.weight();
        if delta.kind() == DeltaKind::Kill {
            // Replaced whole, never appended to: consecutive kills do not
            // coalesce (`kill_ring.zig:86-116`). A kill past the slot's own cap
            // **empties** it -- a truncated payload behind a summary would yank
            // a marker standing for bytes that are no longer there.
            self.killed = (weight <= MAX_KILL_BYTES).then(|| Killed {
                text: delta.removed().to_string(),
                entities: delta
                    .removed_entities()
                    .iter()
                    .map(|span| Span {
                        start: span.start.saturating_sub(delta.at()),
                        end: span.end.saturating_sub(delta.at()),
                        kind: span.kind.clone(),
                    })
                    .collect(),
            });
        }
        if weight > MAX_RETAINED_BYTES {
            self.boundary();
            return;
        }
        // **The redo bytes come back.** Without this, undoing fifty entries and
        // typing one character leaves fifty entries' worth counted forever --
        // evicting live undo entries early and leaving `retained` unable to
        // reach zero except through a boundary (`edit_history.zig:71-75`).
        for cleared in self.redo.drain(..) {
            self.retained = self.retained.saturating_sub(cleared.weight());
        }
        self.retained = self.retained.saturating_add(weight);
        self.undo.push(delta);
        while self.undo.len() > MAX_ENTRIES || self.retained > MAX_RETAINED_BYTES {
            // Oldest first (`orderedRemove(0)`), and there is always one to
            // take: a single delta past the byte cap was answered above, so a
            // stack over the cap holds at least two entries.
            let Some(evicted) = (!self.undo.is_empty()).then(|| self.undo.remove(0)) else {
                break;
            };
            self.retained = self.retained.saturating_sub(evicted.weight());
        }
    }

    /// Takes the last edit back, if there is one.
    ///
    /// The entry is handed to `apply` and pushed onto the other stack in one
    /// operation, which is why there is no public pop: a caller that took an
    /// entry and forgot to stash it would silently destroy the redo path. The
    /// **transfer changes no bytes at all** -- the two stacks are counted
    /// together, so moving an entry between them is not a spend.
    pub(crate) fn undo(&mut self, apply: impl FnOnce(&Delta)) -> bool {
        let Some(delta) = self.undo.pop() else {
            return false;
        };
        apply(&delta);
        self.redo.push(delta);
        true
    }

    /// Puts back an edit an undo took, if there is one.
    pub(crate) fn redo(&mut self, apply: impl FnOnce(&Delta)) -> bool {
        let Some(delta) = self.redo.pop() else {
            return false;
        };
        apply(&delta);
        self.undo.push(delta);
        true
    }

    /// The draft every entry names is gone: so are the entries.
    ///
    /// The kill slot is deliberately untouched -- it is separate state upstream
    /// and only a new session empties it (`kill_ring.zig:71-84`).
    pub(crate) fn boundary(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.retained = 0;
    }

    /// The bytes the two stacks hold between them.
    ///
    /// The three depth/byte readers are the cases' -- what production needs of
    /// this module is that the caps hold, not what the numbers are.
    #[cfg(test)]
    pub(crate) fn retained(&self) -> usize {
        self.retained
    }

    #[cfg(test)]
    pub(crate) fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    #[cfg(test)]
    pub(crate) fn redo_depth(&self) -> usize {
        self.redo.len()
    }

    /// What a `C-y` would yank, and the blocks inside it.
    pub(crate) fn killed(&self) -> Option<(&str, &[Span])> {
        self.killed
            .as_ref()
            .map(|killed| (killed.text.as_str(), killed.entities.as_slice()))
    }

    /// The text alone, for the cases that are about the slot rather than about
    /// what is in it.
    #[cfg(test)]
    pub(crate) fn killed_text(&self) -> Option<&str> {
        self.killed.as_ref().map(|killed| killed.text.as_str())
    }

    /// The newest undo entry, for the paste-boundary cases that migrated onto
    /// it from the one-entry seam.
    #[cfg(test)]
    pub(crate) fn last(&self) -> Option<&Delta> {
        self.undo.last()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A typed character at `caret`.
    fn typed(text: &str, caret: usize) -> Delta {
        Delta::new(
            caret,
            String::new(),
            text.to_string(),
            Vec::new(),
            Vec::new(),
            caret,
            caret + text.len(),
            DeltaKind::Ordinary,
        )
    }

    /// A backspace that took `text`, ending at `caret`.
    fn typed_backspace(text: &str, caret: usize) -> Delta {
        Delta::new(
            caret,
            text.to_string(),
            String::new(),
            Vec::new(),
            Vec::new(),
            caret + text.len(),
            caret,
            DeltaKind::Ordinary,
        )
    }

    /// A `C-w`/`C-k`/`C-u` that took `text`, ending at `caret`.
    fn killed(text: &str, caret: usize) -> Delta {
        Delta::new(
            caret,
            text.to_string(),
            String::new(),
            Vec::new(),
            Vec::new(),
            caret + text.len(),
            caret,
            DeltaKind::Kill,
        )
    }

    /// An insertion of exactly `bytes` bytes.
    fn inserted_bytes(bytes: usize) -> Delta {
        typed(&"x".repeat(bytes), 0)
    }

    #[test]
    fn a_delta_over_the_byte_cap_resets_both_stacks() {
        let mut edits = EditHistory::new();
        edits.record(typed("a", 0));
        assert!(edits.undo(|_| {}), "one entry to walk back over");
        edits.record(inserted_bytes(MAX_RETAINED_BYTES + 1));
        assert!(
            !edits.undo(|_| {}),
            "the oversized delta is a boundary, not an entry"
        );
        assert!(!edits.redo(|_| {}), "the boundary clears redo too");
        assert_eq!(edits.retained(), 0);
    }

    #[test]
    fn an_undo_redo_walk_leaves_the_retained_total_alone() {
        let mut edits = EditHistory::new();
        edits.record(typed("a", 0));
        let after_record = edits.retained();
        assert!(edits.undo(|_| {}));
        assert_eq!(
            edits.retained(),
            after_record,
            "a transfer moves bytes, it does not spend them"
        );
        assert!(edits.redo(|_| {}));
        assert_eq!(edits.retained(), after_record);
    }

    #[test]
    fn recording_after_an_undo_reclaims_the_cleared_redo_bytes() {
        // `record` clears redo, and those bytes are in `retained`: without the
        // subtraction, fifty undone entries stay counted forever and evict live
        // ones early (`edit_history.zig:71-75`'s `clearStack`).
        let mut edits = EditHistory::new();
        let one = typed("a", 0);
        let weight = one.weight();
        edits.record(one);
        edits.record(typed("b", 1));
        assert_eq!(edits.retained(), weight * 2);
        assert!(edits.undo(|_| {}));
        assert!(edits.undo(|_| {}));
        assert_eq!(
            edits.retained(),
            weight * 2,
            "both entries are still held, on the redo stack"
        );
        edits.record(typed("c", 0));
        assert_eq!(
            edits.retained(),
            weight,
            "exactly the new entry: the two cleared ones were freed"
        );
        assert_eq!(edits.undo_depth(), 1);
        assert_eq!(edits.redo_depth(), 0);
    }

    #[test]
    fn a_new_edit_clears_redo() {
        let mut edits = EditHistory::new();
        edits.record(typed("a", 0));
        assert!(edits.undo(|_| {}));
        assert_eq!(edits.redo_depth(), 1);
        edits.record(typed("b", 0));
        assert_eq!(edits.redo_depth(), 0, "a recorded edit clears redo");
        assert!(!edits.redo(|_| {}));
    }

    #[test]
    fn eviction_drops_the_oldest_undo_entry_first() {
        let mut edits = EditHistory::new();
        for index in 0..=MAX_ENTRIES {
            edits.record(typed("x", index));
        }
        assert_eq!(edits.undo_depth(), MAX_ENTRIES);
        let mut carets = Vec::new();
        while edits.undo(|delta| carets.push(delta.caret_before())) {}
        assert_eq!(
            carets.last().copied(),
            Some(1),
            "entry 0 was evicted, entry 1 is oldest"
        );
    }

    #[test]
    fn byte_cap_evicts_until_under() {
        // Entries well inside the entry cap and well past the byte cap: the
        // second condition has to evict on its own.
        let mut edits = EditHistory::new();
        let chunk = MAX_RETAINED_BYTES / 4;
        for _ in 0..6 {
            edits.record(inserted_bytes(chunk));
        }
        assert!(
            edits.retained() <= MAX_RETAINED_BYTES,
            "the byte cap held {} bytes",
            edits.retained()
        );
        assert_eq!(edits.undo_depth(), 4, "four chunks is the most that fits");
    }

    #[test]
    fn consecutive_kills_replace_the_slot_rather_than_accumulating() {
        let mut edits = EditHistory::new();
        edits.record(killed("first ", 6));
        edits.record(killed("second", 6));
        assert_eq!(
            edits.killed_text(),
            Some("second"),
            "kill_ring.zig:113-115 swaps and frees"
        );
    }

    #[test]
    fn a_kill_fills_the_slot_and_an_ordinary_delete_does_not() {
        let mut edits = EditHistory::new();
        edits.record(killed("word", 4));
        edits.record(typed_backspace("d", 3));
        assert_eq!(edits.killed_text(), Some("word"));
    }

    #[test]
    fn an_oversized_kill_clears_the_slot() {
        let mut edits = EditHistory::new();
        edits.record(killed("word", 4));
        edits.record(killed(&"y".repeat(MAX_KILL_BYTES + 1), 0));
        assert_eq!(
            edits.killed_text(),
            None,
            "a kill too big to hold left a truncated one in the slot"
        );
    }

    #[test]
    fn kill_bytes_are_not_history_bytes() {
        // The slot is its own budget: a kill that fills it must not evict undo
        // entries that have nothing to do with it (`kill_ring.zig:45-50`).
        let mut edits = EditHistory::new();
        edits.record(typed("a", 0));
        let one = edits.retained();
        edits.record(killed(&"y".repeat(1024), 0));
        assert_eq!(
            edits.retained(),
            one + 1024,
            "the kill's own delta is history; a second copy in the slot is not"
        );
    }

    #[test]
    fn a_boundary_leaves_the_kill_slot_loaded() {
        // Only `resetForSession` empties the ring (`kill_ring.zig:71-84`): a
        // submit throws the draft away, and what was killed out of it is still
        // yankable into the next one.
        let mut edits = EditHistory::new();
        edits.record(killed("word", 4));
        edits.boundary();
        assert_eq!(edits.undo_depth(), 0);
        assert_eq!(edits.retained(), 0);
        assert_eq!(edits.killed_text(), Some("word"));
    }
}
