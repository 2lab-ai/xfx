//! Where the band's rows are on a screen whose size is somebody else's fact.
//!
//! The band is three things at the bottom of the **normal** buffer -- a
//! divider, the composer, and a hint row -- with a fourth above the divider
//! while a turn is running ([`super::activity`]) and a block of rows above
//! *that* while a turn is waiting for a decision ([`super::approval`]), and
//! everything above the band stays the terminal's own document. So a layout is
//! a handful of row numbers, and [`solve_band`] is the one place they are
//! derived from one another; nothing else in the TUI may compute a row from
//! `rows - 1`.
//!
//! Nothing here queries anything. The screen's size arrives as two arguments
//! ([`super::term::window_size`] is what asks the terminal), which is what makes
//! every row number below a unit test rather than a claim about the window the
//! developer happened to have open.
//!
//! **One pass, not a fixed point.** Upstream re-solves until the composer's row
//! count and the content area it is measured against agree
//! (`input_presentation.zig:201-205`); this phase solves once, from the row
//! count it is handed, and [`input_row_limit`] measures the cap against the
//! content area a *one-row* composer leaves. The two answers differ only for a
//! composer already at the cap, and the convergence is Phase 3 item 22.

/// The rows the composer starts with, before anything has been typed into it.
pub(crate) const INITIAL_INPUT_ROWS: u16 = 1;

/// The shortest screen a band fits on: a divider, one composer row, a hint row,
/// and three rows of document left to be worth drawing over.
pub(crate) const MIN_ROWS: u16 = 6;

/// The narrowest screen a band fits on.
pub(crate) const MIN_COLS: u16 = 20;

/// Every row number the band is painted from, one-based as the terminal counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Geometry {
    pub(crate) rows: u16,
    pub(crate) cols: u16,
    /// The last row the transcript may occupy: everything at or above it is the
    /// terminal's document and the band never writes there.
    pub(crate) content_bottom: u16,
    /// The row that says what the turn is doing, while there is a turn.
    ///
    /// Directly above the divider -- or above the approval panel, when one is
    /// up -- and `None` whenever nothing is running: the row is the evidence
    /// that something is happening, so a session with nothing in flight does
    /// not own it and the document does ([`super::activity`]).
    pub(crate) activity: Option<u16>,
    /// How many rows the approval panel occupies, and `0` when there is none.
    ///
    /// A count rather than a first/last pair, because the panel's rows are the
    /// only ones in the band that are not addressed individually: they are
    /// painted in order between the activity row and the divider, from the one
    /// list [`super::approval::Panel::rows`] produces.
    pub(crate) panel: u16,
    /// The rule under the document, and the row the composer's first row
    /// follows.
    pub(crate) divider: u16,
    pub(crate) input_first: u16,
    pub(crate) input_last: u16,
    pub(crate) hint: u16,
}

impl Geometry {
    /// The band's top row, and therefore the row an exit clears from.
    ///
    /// The activity row when there is one, then the panel's first row, then the
    /// divider: a band that reported its divider while painting rows above it
    /// would leave them on the terminal at exit and let an append write over
    /// them while the session ran.
    pub(crate) fn band_top(&self) -> u16 {
        self.activity.unwrap_or_else(|| self.panel_first())
    }

    /// The panel's first row, or the divider when there is no panel.
    pub(crate) fn panel_first(&self) -> u16 {
        self.divider.saturating_sub(self.panel)
    }

    /// How many rows the band owns, from its top row to the hint row.
    pub(crate) fn band_rows(&self) -> u16 {
        // Every row from the top of the band to the bottom of the screen, and
        // the subtraction cannot underflow because `solve_with` is the only
        // constructor and it never places that row below the hint row.
        self.hint - self.band_top() + 1
    }

    /// How many rows the composer occupies.
    pub(crate) fn input_rows(&self) -> u16 {
        self.input_last - self.input_first + 1
    }
}

/// The band's rows for a screen of `rows` x `cols` holding an `input_rows`-tall
/// composer and nothing running, or `None` when the screen cannot hold one.
///
/// The band a session opens on and comes back to: [`solve_with`] is the same
/// derivation with the activity row asked for, and the two are one function so
/// that "is there a row above the divider" cannot be answered differently by
/// the layout and by whatever paints it.
pub(crate) fn solve(rows: u16, cols: u16, input_rows: u16) -> Option<Geometry> {
    solve_with(rows, cols, input_rows, false)
}

/// The same, with the activity row asked for and no approval panel.
pub(crate) fn solve_with(
    rows: u16,
    cols: u16,
    input_rows: u16,
    activity: bool,
) -> Option<Geometry> {
    solve_band(rows, cols, input_rows, activity, 0)
}

/// Whether a screen of `rows` x `cols` could hold a `panel`-row approval panel
/// at all.
///
/// Asked with a **one-row** composer and the activity row present, which is the
/// band a panel always appears in front of: a turn is running, and a draft
/// taller than one row is something the caller can scroll rather than a reason
/// to refuse the question. A `false` is not a smaller panel -- it is a screen
/// on which xfx cannot ask, and `super::shell` refuses on the user's behalf
/// rather than painting a question with its choices off the bottom.
pub(crate) fn fits_panel(rows: u16, cols: u16, panel: u16) -> bool {
    solve_band(rows, cols, 1, true, panel).is_some()
}

/// The same, saying whether the turn's activity row is on the screen and how
/// many rows the approval panel is taking.
///
/// The order is the derivation: the hint row is the last row of the screen, the
/// composer sits on the rows above it, the divider is the row above the
/// composer, the panel -- while a decision is pending -- takes the rows above
/// *that*, the activity row -- when there is work -- is the row above those,
/// and the document ends one row above whichever of the three is highest. A
/// `None` is a refusal the caller reports by name -- a band painted onto a
/// screen that cannot hold it would write over the user's shell output and then
/// clear it on the way out, which is the one thing this module exists to
/// prevent.
pub(crate) fn solve_band(
    rows: u16,
    cols: u16,
    input_rows: u16,
    activity: bool,
    panel: u16,
) -> Option<Geometry> {
    if rows < MIN_ROWS || cols < MIN_COLS || input_rows == 0 {
        return None;
    }
    let hint = rows;
    let input_last = rows - 1;
    let input_first = (input_last + 1).checked_sub(input_rows)?;
    let divider = input_first.checked_sub(1)?;
    // The panel sits directly above the rule, so the question is next to the
    // keys that answer it and the divider stays where it always is -- the row
    // above the composer. Its rows come off the document exactly as a taller
    // composer's do.
    let panel_first = divider.checked_sub(panel)?;
    // The activity row takes its row from the document, exactly as a composer
    // that grew by one would: it is a row of the band while it exists, and the
    // rows a shrinking band gives back are erased by the next thing painted
    // (`super::frame`'s `release`).
    let activity = if activity {
        Some(panel_first.checked_sub(1)?)
    } else {
        None
    };
    let content_bottom = activity.unwrap_or(panel_first).checked_sub(1)?;
    // A composer tall enough to leave no document at all is refused rather than
    // clamped: the caller asked for rows the screen does not have, and silently
    // giving it fewer would put the caret somewhere it did not ask for.
    if content_bottom == 0 {
        return None;
    }
    Some(Geometry {
        rows,
        cols,
        content_bottom,
        activity,
        panel,
        divider,
        input_first,
        input_last,
        hint,
    })
}

/// The tallest composer a screen of `rows` will grow one to.
///
/// Half the content area plus a row (`input_presentation.zig:201-205`), and the
/// content area is the one a **one-row** composer leaves -- so the cap is a
/// property of the screen rather than of the composer's current height, and
/// growing the composer cannot move its own ceiling.
// Task 9 is the growth cap's first reader; the number belongs here because it
// is derived from the same rows `solve` is, and deriving it twice is how the
// two answers drift apart.
#[allow(dead_code)]
pub(crate) fn input_row_limit(rows: u16) -> u16 {
    let content_bottom = rows.saturating_sub(3);
    content_bottom / 2 + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_band_is_a_divider_the_composer_and_a_hint_row_at_the_bottom() {
        let geometry = solve(24, 80, 1).expect("24x80 fits a band");
        assert_eq!(geometry.hint, 24, "the hint row is the last row");
        assert_eq!(geometry.input_last, 23);
        assert_eq!(geometry.input_first, 23);
        assert_eq!(geometry.divider, 22);
        assert_eq!(geometry.content_bottom, 21);
    }

    #[test]
    fn a_taller_composer_takes_rows_from_the_transcript_and_nothing_else() {
        let geometry = solve(24, 80, 4).expect("24x80 fits a four-row composer");
        assert_eq!(geometry.hint, 24);
        assert_eq!((geometry.input_first, geometry.input_last), (20, 23));
        assert_eq!(geometry.divider, 19);
        assert_eq!(geometry.content_bottom, 18);
    }

    #[test]
    fn the_composer_may_take_half_the_content_area_plus_one_row() {
        // input_presentation.zig:201-205, measured against the content area a
        // one-row composer leaves -- Phase 1 approximates the fixed point with
        // one pass, and says so.
        assert_eq!(input_row_limit(24), 11);
        assert_eq!(input_row_limit(10), 4);
        assert_eq!(input_row_limit(6), 2);
    }

    #[test]
    fn a_terminal_too_small_for_a_band_is_refused_rather_than_painted_over() {
        assert!(solve(5, 80, 1).is_none());
        assert!(solve(24, 19, 1).is_none());
        assert!(solve(24, 80, 99).is_none());
    }

    #[test]
    fn the_smallest_screen_a_band_fits_on_still_leaves_a_document_above_it() {
        // The boundary the two minima name, from both sides. Without this,
        // raising `MIN_ROWS` to a number that leaves no content area would pass
        // every test above.
        let geometry = solve(MIN_ROWS, MIN_COLS, 1).expect("the smallest band");
        assert_eq!(geometry.divider, 4);
        assert_eq!(geometry.content_bottom, 3);
        assert!(
            geometry.content_bottom >= 1,
            "the smallest screen a band is allowed on has no document on it"
        );
        assert!(solve(MIN_ROWS - 1, MIN_COLS, 1).is_none());
        assert!(solve(MIN_ROWS, MIN_COLS - 1, 1).is_none());
    }

    #[test]
    fn a_composer_that_would_leave_no_document_is_refused_rather_than_clamped() {
        // Three rows of composer on a six-row screen leaves exactly one row of
        // document; four leaves none, and is a refusal rather than a band that
        // quietly took the whole screen.
        assert_eq!(
            solve(6, 80, 3)
                .expect("a three-row composer")
                .content_bottom,
            1
        );
        assert!(solve(6, 80, 4).is_none());
        assert!(solve(24, 80, 0).is_none(), "a composer with no rows");
    }

    #[test]
    fn a_turn_puts_one_more_row_above_the_divider_and_takes_it_from_the_document() {
        // The activity row is the band's while it exists: the divider and the
        // composer do not move -- the caret must not jump when a turn starts --
        // and the row it costs comes off the bottom of the document.
        let idle = solve(24, 80, 1).expect("a band");
        let working = solve_with(24, 80, 1, true).expect("a band with a turn in it");
        assert_eq!(idle.activity, None);
        assert_eq!(
            working.activity,
            Some(21),
            "the row directly above the divider"
        );
        assert_eq!(working.divider, idle.divider);
        assert_eq!(working.input_first, idle.input_first);
        assert_eq!(working.hint, idle.hint);
        assert_eq!(working.content_bottom, idle.content_bottom - 1);
        assert_eq!(working.band_top(), 21);
        assert_eq!(idle.band_top(), idle.divider);
        assert_eq!(working.band_rows(), idle.band_rows() + 1);
    }

    #[test]
    fn the_document_and_the_band_still_tile_the_screen_while_a_turn_runs() {
        // The property the row's arithmetic has to keep whatever else moves: a
        // gap would be a row nothing ever paints, and an overlap would be the
        // band writing into the terminal's own document.
        for input_rows in [1u16, 2, 4] {
            let geometry = solve_with(24, 80, input_rows, true).expect("a band with a turn in it");
            assert_eq!(
                geometry.content_bottom + geometry.band_rows(),
                geometry.rows,
                "{input_rows} composer rows and a turn do not tile the screen"
            );
        }
    }

    #[test]
    fn a_screen_with_no_room_for_the_activity_row_is_refused_rather_than_squeezed() {
        // The same refusal a composer one row too tall gets, and for the same
        // reason: a band that took the last document row would be a band with
        // no document above it at all.
        assert_eq!(
            solve(6, 20, 3)
                .expect("a three-row composer")
                .content_bottom,
            1
        );
        assert!(solve_with(6, 20, 3, true).is_none());
        assert_eq!(
            solve_with(6, 20, 2, true)
                .expect("a two-row composer with a turn in it")
                .content_bottom,
            1
        );
    }

    #[test]
    fn a_pending_decision_takes_its_rows_from_the_document_and_leaves_the_rule_alone() {
        // The panel is the band's while it exists, and it sits between the
        // activity row and the rule: the divider, the composer and the caret
        // must not move when a question appears, or answering it would mean
        // first finding where the composer went.
        let working = solve_with(24, 80, 1, true).expect("a band with a turn in it");
        let asking = solve_band(24, 80, 1, true, 8).expect("a band with a question in it");
        assert_eq!(asking.divider, working.divider);
        assert_eq!(asking.input_first, working.input_first);
        assert_eq!(asking.hint, working.hint);
        assert_eq!(asking.panel_first(), asking.divider - 8);
        assert_eq!(
            asking.activity,
            Some(asking.panel_first() - 1),
            "the activity row is not directly above the panel"
        );
        assert_eq!(asking.band_top(), asking.activity.expect("a turn"));
        assert_eq!(asking.band_rows(), working.band_rows() + 8);
        assert_eq!(asking.content_bottom, working.content_bottom - 8);
        assert_eq!(
            asking.content_bottom + asking.band_rows(),
            asking.rows,
            "the document and a band with a question in it do not tile the screen"
        );
    }

    #[test]
    fn a_band_with_no_panel_puts_its_first_panel_row_where_the_divider_is() {
        // The seam every reader of `panel_first` depends on: with no panel the
        // rows between it and the divider are none, so a painter counting from
        // it and a painter counting from the divider write the same rows.
        let geometry = solve(24, 80, 1).expect("a band");
        assert_eq!(geometry.panel, 0);
        assert_eq!(geometry.panel_first(), geometry.divider);
        assert_eq!(geometry.band_top(), geometry.divider);
    }

    #[test]
    fn a_screen_with_no_room_for_the_panel_is_refused_rather_than_squeezed() {
        // What `fits_panel` is asked before a question is ever painted. A panel
        // whose choices were off the bottom of the screen would be a question
        // with no visible answers, on a session that is waiting for one.
        assert!(fits_panel(24, 80, 8));
        assert!(fits_panel(11, 80, 6));
        assert!(!fits_panel(10, 80, 6));
        assert!(!fits_panel(24, 80, 20));
        assert!(!fits_panel(24, 19, 8), "a screen too narrow for any band");
        // The composer is not what decides it: the question is asked against a
        // one-row composer, because a taller draft scrolls. On a short screen a
        // composer at its own cap and a panel together want more rows than
        // there are -- and the panel is still admitted, because `super::shell`
        // gives the composer's rows back one at a time until it fits.
        assert_eq!(input_row_limit(14), 6);
        assert!(solve_band(14, 80, 6, true, 8).is_none());
        assert!(fits_panel(14, 80, 8));
    }

    #[test]
    fn the_band_owns_every_row_from_the_divider_to_the_bottom_of_the_screen() {
        let one = solve(24, 80, 1).expect("a band");
        assert_eq!(one.band_rows(), 3, "divider, composer, hint");
        assert_eq!(one.input_rows(), 1);

        let four = solve(24, 80, 4).expect("a four-row composer");
        assert_eq!(four.band_rows(), 6);
        assert_eq!(four.input_rows(), 4);
        assert_eq!(
            four.content_bottom + four.band_rows(),
            four.rows,
            "the document and the band do not tile the screen"
        );
    }
}
