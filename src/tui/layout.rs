//! Where the band's rows are on a screen whose size is somebody else's fact.
//!
//! The band is upstream's footer (`c1db919`, `footer_layout.zig:39-96` with
//! `composer_top_chrome_rows = 0`): the composer, a rule under it, and a hint
//! row at the bottom of the **normal** buffer. The rule is blank until a menu
//! or a question is up; then the menu's rows sit between a second rule, right
//! under the composer, and that one -- below the text being typed, not above
//! it. While a turn is running, a transient activity row sits above the
//! composer with one blank row on either side of it (`activity_placement.zig`:
//! `gap_above_rows`, `footer_clearance_rows`). Everything above the band stays
//! the terminal's own document. So a layout is a handful of row numbers, and
//! [`solve_band`] is the one place they are derived from one another; nothing
//! else in the TUI may compute a row from `rows - 1`.
//!
//! Nothing here queries anything. The screen's size arrives as two arguments
//! ([`super::term::window_size`] is what asks the terminal), which is what makes
//! every row number below a unit test rather than a claim about the window the
//! developer happened to have open.
//!
//! **One pass, closed-form, not upstream's fixed point.** Upstream re-solves the
//! transcript's occupied extent and release floor until ownership stops moving
//! (`frame_fixed_point.zig:21-93`). This module derives the rows once from the
//! screen and the band's own contents; `super::frame` keeps the document's
//! bottom directly above whatever this returns.

/// The rows the composer starts with, before anything has been typed into it.
pub(crate) const INITIAL_INPUT_ROWS: u16 = 1;

/// The shortest screen a band fits on: one composer row, the rule, a hint row,
/// and four rows of document left to be worth drawing over -- which is also the
/// shortest screen on which a running turn's row and a one-row menu between its
/// two rules still fit together ([`super::picker`]'s reserve).
pub(crate) const MIN_ROWS: u16 = 7;

/// The narrowest screen a band fits on.
pub(crate) const MIN_COLS: u16 = 20;

/// The blank row the activity row keeps from the composer below it
/// (`footer_gap_after_activity`, `frame_layout.zig:62-75`) -- on a screen with
/// room for it. A screen without it keeps the row and drops the blank. There is
/// none above: the gap there is the document's own, the blank row every block
/// of the transcript ends on (`gap_above_activity` is the gap between a user
/// turn and the answer, which xfx's card already wrote).
const ACTIVITY_PADDING: u16 = 1;

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
    /// Above the composer, with a blank row of the band's on either side, and
    /// `None` whenever nothing is running: the row is the evidence that
    /// something is happening, so a session with nothing in flight does not own
    /// it and the document does ([`super::activity`]).
    pub(crate) activity: Option<u16>,
    /// The blank rows on either side of the activity row: one each, or none on
    /// a screen too short for them.
    pub(crate) activity_padding: u16,
    /// How many rows the menu or question occupies, and `0` when there is none.
    ///
    /// A count rather than a first/last pair, because the panel's rows are the
    /// only ones in the band that are not addressed individually: they are
    /// painted in order between the rule under the composer and the bottom
    /// rule, from the one list the panel produces.
    pub(crate) panel: u16,
    /// The bottom rule, directly above the hint row: blank while there is no
    /// panel, a rule while there is.
    pub(crate) divider: u16,
    pub(crate) input_first: u16,
    pub(crate) input_last: u16,
    pub(crate) hint: u16,
}

impl Geometry {
    /// The band's top row, and therefore the row an exit clears from.
    ///
    /// The activity row while a turn is running, and the composer's first row
    /// otherwise.
    pub(crate) fn band_top(&self) -> u16 {
        self.activity.unwrap_or(self.input_first)
    }

    /// The panel's first row, or the bottom rule when there is no panel.
    pub(crate) fn panel_first(&self) -> u16 {
        self.divider.saturating_sub(self.panel)
    }

    /// The rule between the composer and the panel, while there is a panel.
    #[allow(dead_code)]
    pub(crate) fn picker_rule(&self) -> Option<u16> {
        (self.panel > 0).then(|| self.input_last + 1)
    }

    /// How many rows the band owns, from its top row to the hint row.
    pub(crate) fn band_rows(&self) -> u16 {
        // Every row from the top of the band to the bottom of the screen, and
        // the subtraction cannot underflow because `solve_band` is the only
        // constructor and it never places that row below the hint row.
        self.hint - self.band_top() + 1
    }

    /// How many rows the composer occupies.
    pub(crate) fn input_rows(&self) -> u16 {
        self.input_last - self.input_first + 1
    }

    /// The same band, sitting directly under a document `document` rows tall
    /// -- or at the bottom of the screen once the two no longer fit above it.
    ///
    /// Upstream's frame is only as tall as what it holds: the transcript's
    /// rows, then the footer, from the row the session started on, and the
    /// footer is the frame's last rows (`frame_layout.zig:255-296`). So on a
    /// screen the transcript has not filled yet the footer floats under its
    /// last row with blank rows below the hint, and follows the transcript
    /// down until it reaches the bottom row. Every row of the band moves by
    /// the same amount; the screen's size does not move at all, and a
    /// document of no rows puts the band's top on the first row.
    pub(crate) fn floated(&self, document: u32) -> Geometry {
        let document = u16::try_from(document).unwrap_or(u16::MAX);
        let floor = self.rows.min(document.saturating_add(self.band_rows()));
        // Measured from wherever the band is now, so floating a floated band
        // is the same as floating an anchored one.
        let lift = i32::from(self.rows) - i32::from(floor);
        let shift = lift - (i32::from(self.rows) - i32::from(self.hint));
        let moved = |row: u16| u16::try_from(i32::from(row) - shift).unwrap_or(row);
        Geometry {
            content_bottom: moved(self.content_bottom),
            activity: self.activity.map(moved),
            divider: moved(self.divider),
            input_first: moved(self.input_first),
            input_last: moved(self.input_last),
            hint: moved(self.hint),
            ..*self
        }
    }
}

/// The band's rows for a screen of `rows` x `cols` holding an `input_rows`-tall
/// composer and nothing running, or `None` when the screen cannot hold one.
pub(crate) fn solve(rows: u16, cols: u16, input_rows: u16) -> Option<Geometry> {
    solve_with(rows, cols, input_rows, false)
}

/// The same, with the activity row asked for and no panel.
pub(crate) fn solve_with(
    rows: u16,
    cols: u16,
    input_rows: u16,
    activity: bool,
) -> Option<Geometry> {
    solve_band(rows, cols, input_rows, activity, 0)
}

/// Whether a screen of `rows` x `cols` could hold a `panel`-row question at
/// all.
///
/// Asked with a **one-row** composer and the activity row present, which is the
/// band a question always appears in front of: a turn is running, and a draft
/// taller than one row is something the caller can scroll rather than a reason
/// to refuse the question. A `false` is a screen on which xfx cannot ask, and
/// `super::shell` refuses on the user's behalf rather than painting a question
/// with its choices off the bottom.
pub(crate) fn fits_panel(rows: u16, cols: u16, panel: u16) -> bool {
    solve_band(rows, cols, 1, true, panel).is_some()
}

/// The same, saying whether the turn's activity row is on the screen and how
/// many rows the panel is taking.
///
/// The order is the derivation, bottom up: the hint row is the last row of the
/// screen, the bottom rule is the row above it, the panel -- while there is one
/// -- takes the rows above *that* with a second rule above them, the composer
/// sits on the rows above whichever rule is highest, the activity row and its
/// two blank rows -- when there is work -- are above the composer, and the
/// document ends one row above the band's top. A `None` is a refusal the caller
/// reports by name -- a band painted onto a screen that cannot hold it would
/// write over the user's shell output and then clear it on the way out, which
/// is the one thing this module exists to prevent.
pub(crate) fn solve_band(
    rows: u16,
    cols: u16,
    input_rows: u16,
    activity: bool,
    panel: u16,
) -> Option<Geometry> {
    if !activity {
        return solve_padded(rows, cols, input_rows, None, panel);
    }
    solve_padded(rows, cols, input_rows, Some(ACTIVITY_PADDING), panel)
        .or_else(|| solve_padded(rows, cols, input_rows, Some(0), panel))
}

/// The derivation, with the activity row's padding decided by the caller.
fn solve_padded(
    rows: u16,
    cols: u16,
    input_rows: u16,
    activity: Option<u16>,
    panel: u16,
) -> Option<Geometry> {
    if rows < MIN_ROWS || cols < MIN_COLS || input_rows == 0 {
        return None;
    }
    let activity_padding = activity.unwrap_or(0);
    let hint = rows;
    let divider = rows - 1;
    let panel_first = divider.checked_sub(panel)?;
    // With a panel, the composer ends above the rule that heads it.
    let input_last = if panel > 0 {
        panel_first.checked_sub(2)?
    } else {
        divider.checked_sub(1)?
    };
    let input_first = (input_last + 1).checked_sub(input_rows)?;
    let activity = match activity {
        Some(padding) => Some(input_first.checked_sub(padding + 1)?),
        None => None,
    };
    let band_top = activity.unwrap_or(input_first);
    let content_bottom = band_top.checked_sub(1)?;
    // A band tall enough to leave no document at all is refused rather than
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
        activity_padding,
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
#[allow(dead_code)]
pub(crate) fn input_row_limit(rows: u16) -> u16 {
    let content_bottom = rows.saturating_sub(3);
    content_bottom / 2 + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_band_is_the_composer_a_rule_and_a_hint_row_at_the_bottom() {
        let geometry = solve(24, 80, 1).expect("24x80 fits a band");
        assert_eq!(geometry.hint, 24, "the hint row is the last row");
        assert_eq!(geometry.divider, 23, "the rule is directly above it");
        assert_eq!((geometry.input_first, geometry.input_last), (22, 22));
        assert_eq!(geometry.content_bottom, 21);
        assert_eq!(geometry.band_top(), 22, "the composer is the band's top");
        assert_eq!(geometry.picker_rule(), None);
    }

    #[test]
    fn a_taller_composer_takes_rows_from_the_transcript_and_nothing_else() {
        let geometry = solve(24, 80, 4).expect("24x80 fits a four-row composer");
        assert_eq!(geometry.hint, 24);
        assert_eq!(geometry.divider, 23);
        assert_eq!((geometry.input_first, geometry.input_last), (19, 22));
        assert_eq!(geometry.content_bottom, 18);
    }

    #[test]
    fn the_composer_may_take_half_the_content_area_plus_one_row() {
        assert_eq!(input_row_limit(24), 11);
        assert_eq!(input_row_limit(10), 4);
        assert_eq!(input_row_limit(6), 2);
    }

    #[test]
    fn a_terminal_too_small_for_a_band_is_refused_rather_than_painted_over() {
        assert!(solve(6, 80, 1).is_none());
        assert!(solve(24, 19, 1).is_none());
        assert!(solve(24, 80, 99).is_none());
    }

    #[test]
    fn the_smallest_screen_a_band_fits_on_still_leaves_a_document_above_it() {
        let geometry = solve(MIN_ROWS, MIN_COLS, 1).expect("the smallest band");
        assert_eq!(geometry.input_first, 5);
        assert_eq!(geometry.content_bottom, 4);
        assert!(solve(MIN_ROWS - 1, MIN_COLS, 1).is_none());
        assert!(solve(MIN_ROWS, MIN_COLS - 1, 1).is_none());
    }

    #[test]
    fn a_composer_that_would_leave_no_document_is_refused_rather_than_clamped() {
        assert_eq!(
            solve(7, 80, 4).expect("a four-row composer").content_bottom,
            1
        );
        assert!(solve(7, 80, 5).is_none());
        assert!(solve(24, 80, 0).is_none(), "a composer with no rows");
    }

    #[test]
    fn a_turn_puts_its_row_above_the_composer_between_two_blank_rows() {
        // The composer, the rule and the hint row do not move when a turn
        // starts -- the caret must not jump -- and the three rows it costs come
        // off the bottom of the document.
        let idle = solve(24, 80, 1).expect("a band");
        let working = solve_with(24, 80, 1, true).expect("a band with a turn in it");
        assert_eq!(idle.activity, None);
        assert_eq!(
            working.activity,
            Some(20),
            "a blank row under it, then the composer"
        );
        assert_eq!(working.band_top(), 19, "a blank row above it");
        assert_eq!(working.divider, idle.divider);
        assert_eq!(working.input_first, idle.input_first);
        assert_eq!(working.hint, idle.hint);
        assert_eq!(working.content_bottom, idle.content_bottom - 3);
        assert_eq!(working.band_rows(), idle.band_rows() + 3);
    }

    #[test]
    fn the_document_and_the_band_still_tile_the_screen_while_a_turn_runs() {
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
    fn a_short_screen_keeps_the_activity_row_and_drops_its_blank_rows() {
        let roomy = solve_with(9, 20, 2, true).expect("room for the padding");
        assert_eq!(roomy.activity_padding, 1);
        assert_eq!(roomy.content_bottom, 2);
        let tight = solve_with(7, 20, 2, true).expect("the row without its padding");
        assert_eq!(tight.activity_padding, 0);
        assert_eq!(tight.activity, Some(tight.input_first - 1));
        assert_eq!(tight.band_top(), tight.activity.expect("a turn"));
        assert_eq!(tight.content_bottom, 2);
        assert!(
            solve_with(7, 20, 4, true).is_none(),
            "no row at all is left"
        );
    }

    #[test]
    fn a_panel_sits_under_the_composer_between_two_rules() {
        // Upstream's picker: the composer stays put relative to the panel above
        // the bottom rule, and the rows the panel costs come off the document.
        let geometry = solve_band(24, 80, 1, false, 6).expect("a band with a menu");
        assert_eq!(geometry.hint, 24);
        assert_eq!(geometry.divider, 23);
        assert_eq!(geometry.panel_first(), 17);
        assert_eq!(geometry.picker_rule(), Some(16));
        assert_eq!((geometry.input_first, geometry.input_last), (15, 15));
        assert_eq!(geometry.band_top(), 15);
        assert_eq!(geometry.content_bottom, 14);
        assert_eq!(
            geometry.band_rows(),
            10,
            "composer, rule, six rows, rule, hint"
        );
    }

    #[test]
    fn a_question_and_a_turn_still_tile_the_screen() {
        let asking = solve_band(24, 80, 1, true, 8).expect("a band with a question in it");
        assert_eq!(asking.panel_first(), asking.divider - 8);
        assert_eq!(asking.picker_rule(), Some(asking.panel_first() - 1));
        assert_eq!(asking.input_last, asking.panel_first() - 2);
        assert_eq!(asking.activity, Some(asking.input_first - 2));
        assert_eq!(
            asking.content_bottom + asking.band_rows(),
            asking.rows,
            "the document and a band with a question in it do not tile the screen"
        );
    }

    #[test]
    fn a_band_with_no_panel_puts_its_first_panel_row_where_the_rule_is() {
        let geometry = solve(24, 80, 1).expect("a band");
        assert_eq!(geometry.panel, 0);
        assert_eq!(geometry.panel_first(), geometry.divider);
    }

    #[test]
    fn a_screen_with_no_room_for_the_panel_is_refused_rather_than_squeezed() {
        // A turn's three rows, the composer, two rules, the panel and the hint
        // row, and one row of document.
        // On a short screen the turn's row keeps its place and drops the blank
        // rows around it before a question is refused.
        assert!(fits_panel(24, 80, 8));
        assert!(fits_panel(12, 80, 6));
        assert!(!fits_panel(11, 80, 6));
        assert!(fits_panel(24, 80, 18));
        assert!(!fits_panel(24, 80, 19));
        assert!(!fits_panel(24, 19, 8), "a screen too narrow for any band");
        assert!(solve_band(16, 80, 6, true, 8).is_none());
        assert_eq!(
            solve_band(16, 80, 1, true, 8)
                .expect("a question with the turn's padding")
                .activity_padding,
            1
        );
    }

    #[test]
    fn the_band_owns_every_row_from_its_top_to_the_bottom_of_the_screen() {
        let one = solve(24, 80, 1).expect("a band");
        assert_eq!(one.band_rows(), 3, "composer, rule, hint");
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

    #[test]
    fn a_band_floats_under_a_short_document_and_reaches_the_bottom_with_a_long_one() {
        let anchored = solve(24, 80, 1).expect("24x80 fits a band");
        let empty = anchored.floated(0);
        assert_eq!(
            (
                empty.band_top(),
                empty.divider,
                empty.hint,
                empty.content_bottom
            ),
            (1, 2, 3, 0),
            "no document: the composer is the first row"
        );
        let header = anchored.floated(2);
        assert_eq!((header.band_top(), header.hint), (3, 5));
        assert_eq!(
            header.content_bottom, 2,
            "the document ends on the row above"
        );
        assert_eq!(header.rows, 24, "the screen does not move");
        assert_eq!(
            anchored.floated(21),
            anchored,
            "21 + 3 rows tile the screen"
        );
        assert_eq!(anchored.floated(400), anchored);
        assert_eq!(
            header.floated(21),
            anchored,
            "floating is from wherever it is"
        );
        assert_eq!(anchored.floated(2).floated(2), header);
    }

    #[test]
    fn a_floating_band_keeps_its_top_and_moves_its_bottom_as_it_grows() {
        let working = solve_band(24, 80, 1, true, 0)
            .expect("a turn fits")
            .floated(4);
        assert_eq!(working.band_top(), 5);
        assert_eq!(working.activity, Some(6));
        assert_eq!((working.input_first, working.hint), (8, 10));
        let menu = solve_band(24, 80, 1, false, 6)
            .expect("a menu fits")
            .floated(4);
        assert_eq!(menu.band_top(), 5);
        assert_eq!(menu.panel_first(), 7);
        assert_eq!((menu.divider, menu.hint), (13, 14));
        let tall = solve_band(24, 80, 1, false, 18).expect("a tall menu fits");
        assert_eq!(
            tall.floated(4),
            tall,
            "a band too tall to float stays at the bottom"
        );
    }
}
