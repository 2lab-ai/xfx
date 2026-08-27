//! What has been submitted, and the draft a recall stood aside.
//!
//! Upstream keeps text-only snapshots of submitted prompts and captures the
//! draft on the way into the walk (`composer_history.zig:445-540`), and this is
//! the same shape for the same two reasons.
//!
//! * **A recall must not cost what is being typed.** The half-written line is
//!   captured the moment the first step back happens, and it is what the walk
//!   comes back to when it reaches the near end again. A history that only
//!   replaced the composer would make the arrow key a destructive gesture, and
//!   there is no undo on this surface yet.
//! * **The walk is a position, not a mode.** [`History::cursor`] is `None`
//!   whenever the composer holds the user's own line -- which is the state a
//!   fresh session, a submitted line and an edited recall all leave it in -- so
//!   there is no flag that can disagree with where the walk had got to.
//!
//! # What an entry is
//!
//! The line **as the composer held it**, which for a collapsed paste is the
//! summary rather than the megabytes behind it. That is what makes an entry
//! cheap enough to keep a hundred of, and it is why [`HistoryEntry`] carries a
//! `Vec<`[`EntitySnapshot`]`>` beside the text: the blocks a summary names die
//! with the draft (`super::paste::Paste::forget`), so an entry that meant to
//! put one back would have to have remembered it. This phase records the empty
//! vector -- a recalled summary is words -- and item 21 fills it in.
//!
//! # The two ends of the walk
//!
//! Neither wraps. Stepping back from the oldest line recalls nothing and stays
//! there, because a walk that wrapped would put the newest line under a key the
//! user was pressing to reach the oldest one; stepping forward from the newest
//! hands the captured draft back and ends the walk, because the draft is what
//! is newer than every line that has been sent.

use std::collections::VecDeque;

use super::entity::EntitySnapshot;

/// The most lines a session remembers (`composer_history.zig:445-540`).
///
/// A bound rather than a buffer that grows: a session left open for a day is a
/// session whose oldest lines nobody is walking back to, and the entries hold
/// text a user pasted.
pub(crate) const MAX_ENTRIES: usize = 100;

/// Which way a recall is walking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HistoryStep {
    /// Older: `C-p`, and `Up` from the composer's first row.
    Previous,
    /// Newer: `C-n`, and `Down` from its last row.
    Next,
}

/// One line, as the composer held it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct HistoryEntry {
    text: String,
    /// The collapsed pastes the line stood on.
    ///
    /// Empty on every entry this phase records -- see the module's note on what
    /// an entry is.
    #[allow(dead_code)]
    entities: Vec<EntitySnapshot>,
}

impl HistoryEntry {
    pub(crate) fn new(text: String, entities: Vec<EntitySnapshot>) -> Self {
        Self { text, entities }
    }

    /// The line's text.
    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    /// The collapsed pastes it stood on.
    // Item 21 is the first reader: what it is for is putting a block back when
    // the summary that names it is recalled.
    #[allow(dead_code)]
    pub(crate) fn entities(&self) -> &[EntitySnapshot] {
        &self.entities
    }
}

/// The lines a session has submitted, and where a recall has got to in them.
pub(crate) struct History {
    /// Newest first, so a step back is a step **up** the index and the eviction
    /// is at the far end.
    entries: VecDeque<HistoryEntry>,
    /// Which entry the composer is showing, while it is showing one.
    ///
    /// `None` is "the composer holds the user's own line", which is the only
    /// state in which [`Self::draft`] means nothing -- the two move together
    /// and every path that clears one clears the other.
    cursor: Option<usize>,
    /// What the composer held when the walk began.
    draft: Option<HistoryEntry>,
}

impl History {
    pub(crate) fn new() -> Self {
        Self {
            entries: VecDeque::new(),
            cursor: None,
            draft: None,
        }
    }

    /// Remembers a submitted line, and ends whatever walk produced it.
    ///
    /// The walk ending is not tidiness: the line has been consumed, so the
    /// position the walk had reached is a position in a list that has just
    /// grown a new head, and a next step back from it would skip the line the
    /// user just sent.
    ///
    /// **Adjacent repeats are one entry.** A user who sends `/help` twice does
    /// not want two presses of `Up` to get past it; a `/help` with something
    /// else between the two really was typed twice, and both are kept, because
    /// the walk is what was typed rather than a set of what was typed.
    pub(crate) fn record(&mut self, entry: HistoryEntry) {
        self.leave();
        if self
            .entries
            .front()
            .is_some_and(|newest| newest.text == entry.text)
        {
            return;
        }
        self.entries.push_front(entry);
        while self.entries.len() > MAX_ENTRIES {
            self.entries.pop_back();
        }
    }

    /// One step of a walk, from a composer that currently holds `current`.
    ///
    /// `None` is "there is nothing there", and it is the answer at both ends:
    /// past the oldest line, and forward from a composer that never stepped
    /// back at all. Nothing moves on a `None` -- in particular the draft is
    /// **not** captured by a step that recalls nothing, so a `C-p` at a session
    /// with no history is a keystroke that changes no state rather than one
    /// that quietly arms the next `C-n` to wipe the composer.
    pub(crate) fn navigate(
        &mut self,
        step: HistoryStep,
        current: HistoryEntry,
    ) -> Option<HistoryEntry> {
        match step {
            HistoryStep::Previous => {
                let wanted = match self.cursor {
                    None => 0,
                    Some(at) => at.checked_add(1)?,
                };
                // Read **before** anything is recorded, so the refusal at the
                // oldest line leaves the walk exactly where it was.
                let entry = self.entries.get(wanted)?.clone();
                if self.cursor.is_none() {
                    self.draft = Some(current);
                }
                self.cursor = Some(wanted);
                Some(entry)
            }
            HistoryStep::Next => {
                let at = self.cursor?;
                let Some(wanted) = at.checked_sub(1) else {
                    // The near end: what is newer than the newest line is the
                    // draft the walk began from, and handing it back is what
                    // ends the walk. An absent draft is an empty composer,
                    // which is the same answer -- the field is `Some` for every
                    // walk `Previous` can start, and this is that fact taken
                    // seriously rather than unwrapped.
                    self.cursor = None;
                    return Some(self.draft.take().unwrap_or_default());
                };
                let entry = self.entries.get(wanted)?.clone();
                self.cursor = Some(wanted);
                Some(entry)
            }
        }
    }

    /// The composer's line is the user's own again.
    ///
    /// What an edit after a recall means, and what a submitted line leaves
    /// behind. The captured draft goes with the position: it was a stand-in for
    /// a line the user has now replaced, and a draft that outlived its walk
    /// would be handed back by a later `C-n` in place of what is on the screen.
    pub(crate) fn leave(&mut self) {
        self.cursor = None;
        self.draft = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One recorded line, with no entities on it -- which is every entry this
    /// phase records ([`HistoryEntry`]).
    fn entry(text: &str) -> HistoryEntry {
        HistoryEntry::new(text.to_string(), Vec::new())
    }

    /// What a recall handed back, as text, so a case reads as the lines it is
    /// about rather than as the struct they arrive in.
    fn recalled(entry: Option<HistoryEntry>) -> Option<String> {
        entry.map(|entry| entry.text().to_string())
    }

    #[test]
    fn entering_history_stands_the_draft_aside_and_leaving_hands_it_back() {
        // The whole of item 15: what is half-typed when the first recall
        // happens is not thrown away, and it is what the walk comes back to.
        let mut history = History::new();
        history.record(entry("first"));
        history.record(entry("second"));

        assert_eq!(
            recalled(history.navigate(HistoryStep::Previous, entry("half typed"))),
            Some("second".to_string()),
            "the first step back did not reach the newest line"
        );
        assert_eq!(
            recalled(history.navigate(HistoryStep::Previous, entry("second"))),
            Some("first".to_string())
        );
        assert_eq!(
            recalled(history.navigate(HistoryStep::Next, entry("first"))),
            Some("second".to_string())
        );
        assert_eq!(
            recalled(history.navigate(HistoryStep::Next, entry("second"))),
            Some("half typed".to_string()),
            "the draft the walk began from was lost"
        );
    }

    #[test]
    fn the_draft_is_handed_back_once_and_the_walk_is_over() {
        // A second step forward from the draft is not a second draft: history
        // has been left, and there is nothing newer than what the composer
        // already holds.
        let mut history = History::new();
        history.record(entry("only"));
        assert_eq!(
            recalled(history.navigate(HistoryStep::Previous, entry("half typed"))),
            Some("only".to_string())
        );
        assert_eq!(
            recalled(history.navigate(HistoryStep::Next, entry("only"))),
            Some("half typed".to_string())
        );
        assert_eq!(
            recalled(history.navigate(HistoryStep::Next, entry("half typed"))),
            None,
            "a second step forward invented a line the composer never held"
        );
    }

    #[test]
    fn stepping_back_stops_at_the_oldest_line_rather_than_wrapping() {
        let mut history = History::new();
        history.record(entry("oldest"));
        history.record(entry("newest"));
        assert_eq!(
            recalled(history.navigate(HistoryStep::Previous, entry("draft"))),
            Some("newest".to_string())
        );
        assert_eq!(
            recalled(history.navigate(HistoryStep::Previous, entry("newest"))),
            Some("oldest".to_string())
        );
        assert_eq!(
            recalled(history.navigate(HistoryStep::Previous, entry("oldest"))),
            None,
            "the walk wrapped past the oldest line"
        );
        // And the refusal left the walk where it was: one step forward is the
        // line above the oldest, not the draft.
        assert_eq!(
            recalled(history.navigate(HistoryStep::Next, entry("oldest"))),
            Some("newest".to_string()),
            "the refused step moved the recall anyway"
        );
    }

    #[test]
    fn a_step_forward_that_never_stepped_back_recalls_nothing() {
        // C-n at a composer nobody recalled into is a keystroke with nothing to
        // do -- and it must not hand back an empty draft that would erase what
        // is being typed.
        let mut history = History::new();
        history.record(entry("first"));
        assert_eq!(
            recalled(history.navigate(HistoryStep::Next, entry("half typed"))),
            None
        );
    }

    #[test]
    fn a_recall_with_nothing_recorded_captures_no_draft() {
        // The capture happens on the way *into* history, so a step back that
        // reaches nothing must not swallow the draft on its way to refusing.
        let mut history = History::new();
        assert_eq!(
            recalled(history.navigate(HistoryStep::Previous, entry("half typed"))),
            None
        );
        assert_eq!(
            recalled(history.navigate(HistoryStep::Next, entry("half typed"))),
            None,
            "a refused recall left the session inside history"
        );
    }

    #[test]
    fn the_same_line_submitted_twice_running_is_recorded_once() {
        // Adjacent only. Two `/help`s in a row are one line to walk back
        // through; a `/help` on either side of something else is two, because
        // the walk is what was typed and it really was typed twice.
        let mut history = History::new();
        history.record(entry("same"));
        history.record(entry("same"));
        assert_eq!(
            recalled(history.navigate(HistoryStep::Previous, entry(""))),
            Some("same".to_string())
        );
        assert_eq!(
            recalled(history.navigate(HistoryStep::Previous, entry("same"))),
            None,
            "the newest line was recorded twice"
        );

        let mut history = History::new();
        history.record(entry("same"));
        history.record(entry("between"));
        history.record(entry("same"));
        assert_eq!(
            recalled(history.navigate(HistoryStep::Previous, entry(""))),
            Some("same".to_string())
        );
        assert_eq!(
            recalled(history.navigate(HistoryStep::Previous, entry("same"))),
            Some("between".to_string())
        );
        assert_eq!(
            recalled(history.navigate(HistoryStep::Previous, entry("between"))),
            Some("same".to_string()),
            "a repeat that was not adjacent was folded into the first one"
        );
    }

    #[test]
    fn the_hundred_and_first_line_evicts_the_oldest_one() {
        // The cap is a literal, asserted as one: a bound that drifted would
        // make the walk longer or shorter than what is documented, and nothing
        // else in this module would notice.
        assert_eq!(MAX_ENTRIES, 100);
        let mut history = History::new();
        for index in 0..MAX_ENTRIES + 1 {
            history.record(entry(&format!("line {index}")));
        }
        // Bounded rather than `while let`: the walk stopping is one of the
        // claims here, and a loop that trusted it would hang forever on an
        // implementation that wrapped instead of ending -- a test that cannot
        // fail is not a test.
        let mut walked = Vec::new();
        let mut current = entry("draft");
        for _ in 0..MAX_ENTRIES + 10 {
            let Some(recalled) = history.navigate(HistoryStep::Previous, current.clone()) else {
                break;
            };
            current = recalled.clone();
            walked.push(recalled.text().to_string());
        }
        assert_eq!(
            walked.len(),
            MAX_ENTRIES,
            "the walk is not {MAX_ENTRIES} lines long, or it never ended"
        );
        assert_eq!(walked.first().map(String::as_str), Some("line 100"));
        assert_eq!(
            walked.last().map(String::as_str),
            Some("line 1"),
            "the oldest line was kept and a newer one was evicted instead"
        );
    }

    #[test]
    fn recording_a_line_forgets_where_a_recall_had_got_to() {
        // Submitting a recalled line consumes the draft, so the walk that
        // produced it is over: the next step back starts at the newest line
        // again rather than continuing from the middle of the old walk.
        let mut history = History::new();
        history.record(entry("first"));
        history.record(entry("second"));
        assert_eq!(
            recalled(history.navigate(HistoryStep::Previous, entry("draft"))),
            Some("second".to_string())
        );
        history.record(entry("third"));
        assert_eq!(
            recalled(history.navigate(HistoryStep::Previous, entry(""))),
            Some("third".to_string()),
            "the recall carried on from where the submitted walk had reached"
        );
    }

    #[test]
    fn leaving_starts_the_next_walk_at_the_newest_line_with_the_draft_in_hand() {
        // What an edit after a recall means: the line on the screen is the
        // user's now, not the recalled one, so it is what a later walk comes
        // back to.
        let mut history = History::new();
        history.record(entry("first"));
        history.record(entry("second"));
        assert_eq!(
            recalled(history.navigate(HistoryStep::Previous, entry("draft"))),
            Some("second".to_string())
        );
        assert_eq!(
            recalled(history.navigate(HistoryStep::Previous, entry("second"))),
            Some("first".to_string())
        );
        history.leave();
        assert_eq!(
            recalled(history.navigate(HistoryStep::Previous, entry("first edited"))),
            Some("second".to_string()),
            "the walk carried on from the line the edit left behind"
        );
        assert_eq!(
            recalled(history.navigate(HistoryStep::Next, entry("second"))),
            Some("first edited".to_string()),
            "the edited line was not what the walk came back to"
        );
    }
}
