//! Where the band's rows are on a screen whose size is somebody else's fact.
//!
//! The band is three things at the bottom of the **normal** buffer -- a
//! divider, the composer, and a hint row -- and everything above the divider
//! stays the terminal's own document. So a layout is five row numbers, and
//! [`solve`] is the one place they are derived from one another; nothing else
//! in the TUI may compute a row from `rows - 1`.
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
    /// The band's top row, and therefore the row an exit clears from.
    pub(crate) divider: u16,
    pub(crate) input_first: u16,
    pub(crate) input_last: u16,
    pub(crate) hint: u16,
}

impl Geometry {
    /// How many rows the band owns, divider and hint row included.
    pub(crate) fn band_rows(&self) -> u16 {
        // Every row from the divider to the bottom of the screen, and the
        // subtraction cannot underflow because `solve` is the only constructor
        // and it never places the divider below the hint row.
        self.hint - self.divider + 1
    }

    /// How many rows the composer occupies.
    pub(crate) fn input_rows(&self) -> u16 {
        self.input_last - self.input_first + 1
    }
}

/// The band's rows for a screen of `rows` x `cols` holding an `input_rows`-tall
/// composer, or `None` when the screen cannot hold one.
///
/// The order is the derivation: the hint row is the last row of the screen, the
/// composer sits on the rows above it, the divider is the row above the
/// composer, and the document ends one row above that. A `None` is a refusal
/// the caller reports by name -- a band painted onto a screen that cannot hold
/// it would write over the user's shell output and then clear it on the way
/// out, which is the one thing this module exists to prevent.
pub(crate) fn solve(rows: u16, cols: u16, input_rows: u16) -> Option<Geometry> {
    if rows < MIN_ROWS || cols < MIN_COLS || input_rows == 0 {
        return None;
    }
    let hint = rows;
    let input_last = rows - 1;
    let input_first = (input_last + 1).checked_sub(input_rows)?;
    let divider = input_first.checked_sub(1)?;
    let content_bottom = divider.checked_sub(1)?;
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
