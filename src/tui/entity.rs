//! What a recorded draft remembers about the pastes it had collapsed.
//!
//! A [`super::paste`] block lives beside the composer and dies with it
//! (`Paste::forget`): it is a span of *the draft on the screen*, and a draft
//! that has been submitted or replaced is a draft whose summaries name nothing.
//! A history entry outlives exactly that moment -- it is the line as it was,
//! kept so it can be put back -- so the blocks it stood on have to be
//! remembered by the entry rather than borrowed from a composer that has moved
//! on.
//!
//! This module is that memory, and in this phase it is only the **carrier**:
//! [`super::history`] holds a `Vec` of these on every entry, `super::shell`
//! records an empty one on every line, and a recalled summary is therefore
//! words rather than a stand-in for eight megabytes -- which is the narrowing
//! `docs/parity.md` records and which item 21 closes, in the same change that
//! fills these fields in and gives the paste ids their renumbering.

use std::sync::Arc;

/// One collapsed paste, as the line it was in remembers it.
///
/// The span is in **bytes of the entry's own text**, not of the composer's:
/// the entry is the fixed thing and the composer is what changes under it, so
/// a span measured against the composer would be a span that means something
/// different every keystroke.
// Item 21's paste atomicity is the first writer and the first reader of every
// field here; this phase records the empty vector and reads the type, which is
// what keeps the shape of a history entry from changing under that work.
#[allow(dead_code)]
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

#[allow(dead_code)]
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
    pub(crate) fn span(&self) -> std::ops::Range<usize> {
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
