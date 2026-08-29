//! The boundary an undo would take, recorded one edit deep.
//!
//! Phase 3's item 18 is a kill ring and an undo stack; this is the **seam** it
//! will be built on, and it is here now for one reason: a paste is the edit
//! whose boundary is not obvious. A typed character is one keystroke and one
//! unit of undo, but a paste is a megabyte that arrived as one gesture, and an
//! undo that took it back a grapheme at a time -- or a byte at a time, or a
//! chunk per read of the terminal -- would be an undo nobody could use. So the
//! rule is fixed at the point the paste lands, where it is knowable, rather
//! than inferred later from a buffer that no longer says how its bytes got
//! there: **one framed paste is one transaction, whatever it weighed**.
//!
//! What is deliberately absent is the stack. There is no `C-z` on this surface
//! and this type holds exactly one transaction, overwritten by the next edit
//! (`super::shell::Shell::edited`): a stack that nothing pops is a memory cost
//! and an invitation to half-implement undo, and the one thing that must be
//! true *now* is that when the stack arrives it finds paste boundaries already
//! marked correctly. `tests/tui.rs` and the cargo case in `super::shell` are
//! this phase's only readers, which is what `.prd/06-qa-harness.md`'s scenario
//! 21 points at for the undo clause rather than driving a `C-z` that does not
//! exist.

use super::entity::Span;

/// What the last edit did, in the terms an undo would need.
///
/// The fields are written by this phase and read by item 18. They are held
/// rather than derived because the draft after an edit does not say what the
/// draft before it was: `before` is the whole text, which is the honest price
/// of an undo that can put back a deletion as well as an insertion.
///
/// What that costs is **two copies of a draft** -- `before` and `after`, each
/// up to the composer's whole budget -- and they live until the next change to
/// the **text** overwrites this with [`EditTransaction::Other`]. Caret
/// navigation is not such a change: arrows, word moves, `Home`/`End` and a
/// vertical move with nowhere to go all leave the boundary (and the two copies)
/// standing, which is the point -- reading a pasted line before deciding to
/// undo it must not be what makes the undo impossible
/// (`super::shell::Shell::moved` beside `super::shell::Shell::edited`).
#[allow(dead_code)]
#[derive(Debug)]
pub(crate) enum EditTransaction {
    /// One framed paste that collapsed into a block: the draft on either side
    /// of it, and the entity it put there.
    InsertPaste {
        before: String,
        after: String,
        entity: Span,
    },
    /// Any other edit. Named rather than absent, because "the last edit was not
    /// a paste" is the fact an undo needs; `None` would mean "nothing has been
    /// edited yet", which is a different thing.
    Other,
}

/// The one transaction this phase remembers.
#[derive(Debug, Default)]
pub(crate) struct LastTransaction(Option<EditTransaction>);

impl LastTransaction {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Overwrites whatever was remembered.
    ///
    /// Every composer edit records something here, which is what makes "the
    /// last transaction" true rather than "the last transaction anybody
    /// bothered to record": an edit that left the field alone would leave a
    /// paste boundary standing in front of a draft the paste is no longer the
    /// last change to.
    pub(crate) fn record(&mut self, transaction: EditTransaction) {
        self.0 = Some(transaction);
    }

    /// What the last edit was, for the case that pins the boundary.
    ///
    /// Test-only on purpose: nothing on a Phase-2 path may *act* on this, and a
    /// reader in the shell would be a half-built undo.
    #[cfg(test)]
    pub(crate) fn last(&self) -> Option<&EditTransaction> {
        self.0.as_ref()
    }
}
