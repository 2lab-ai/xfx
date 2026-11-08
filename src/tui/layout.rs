//! Where the band's rows are on a screen whose size is somebody else's fact.
//!
//! The band is upstream's footer (`c1db919`, `footer_layout.zig:39-96` with
//! `composer_top_chrome_rows = 0`): the composer, a rule under it, and a hint
//! row at the bottom of the **normal** buffer. The rule is blank until a menu
//! or a question is up; then the menu's rows sit between a second rule, right
//! under the composer, and that one -- below the text being typed, not above
//! it. While a turn is running, a transient activity row sits above the
//! composer with one blank row under it; the gap above it is the document's own
//! trailing blank row (`frame_layout.zig:62-75`: `gap_above_activity`,
//! `footer_gap_after_activity`). Everything above the band stays
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
/// two rules still fit together ([`super::picker`]'s budget, upstream's
/// `inlinePickerRowBudget`, gives a menu one row on any screen this short).
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
    /// Above the composer, with a blank row of the band's under it, and
    /// `None` whenever nothing is running: the row is the evidence that
    /// something is happening, so a session with nothing in flight does not own
    /// it and the document does ([`super::activity`]).
    pub(crate) activity: Option<u16>,
    /// The blank row under the activity row: one, or none on a screen too short
    /// for it.
    pub(crate) activity_padding: u16,
    /// The blank row at the band's top, over a panel that has taken the
    /// composer's rows, when the document does not end on one of its own:
    /// one, or none ([`Geometry::gapped`]).
    pub(crate) gap: u16,
    /// The rows at the band's top that show a submitted prompt's newest card
    /// rows, the blank row under them included, for the one frame before the
    /// card itself is written ([`Geometry::previewed`]): none at any other
    /// time.
    pub(crate) preview: u16,
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
    /// otherwise -- or the blank row above a panel that has taken the
    /// composer's, while the band has one ([`Self::gapped`]) -- or the first
    /// row of a submitted prompt's preview above all of them, while it has
    /// one ([`Self::previewed`]).
    pub(crate) fn band_top(&self) -> u16 {
        self.activity.unwrap_or(self.input_first) - self.gap - self.preview
    }

    /// The same band, anchored, with `rows` at its top for a submitted
    /// prompt's newest card rows and the blank row under them -- what upstream
    /// paints into the transcript band under the flow endpoint for the frame
    /// that clears the composer, before the card is written
    /// (`PendingCardPaintContext`, `src/core/app/app_render_runtime.zig:
    /// 168-207@481104fd`) -- in place of any it had: `0` takes them away.
    /// `None` when they would leave no document row.
    pub(crate) fn previewed(&self, rows: u16) -> Option<Geometry> {
        let anchored = self.floated(u32::MAX);
        let top = anchored.activity.unwrap_or(anchored.input_first);
        let content_bottom = top
            .checked_sub(anchored.gap + rows + 1)
            .filter(|bottom| *bottom > 0)?;
        Some(Geometry {
            preview: rows,
            content_bottom,
            ..anchored
        })
    }

    /// The same band with the blank row upstream keeps between the document
    /// and a panel that has taken the composer's rows -- or without it --
    /// anchored to the bottom of the screen, for the caller to float.
    ///
    /// Upstream's footer asks its frame for one blank row above it whenever a
    /// modal hides the composer (`top_gap_rows = if (viewer_active or
    /// modal_active) 1 else 0`, `src/ui/footer/surface_frame.zig:686@481104fd`;
    /// `modal_active` is an approval, a question or a compact command menu --
    /// `/statusline`, `/status`, `/usage`, `/workspace` --, `:392-395`,
    /// `render_input.zig:427-433`). The frame takes the larger of that and the
    /// transcript's own gap under its tail
    /// (`idle_gap = @max(transcript_gap, input.footer.top_gap_rows)`,
    /// `src/ui/render_engine/frame_layout.zig:388-395@481104fd`). xfx's
    /// transcript gap is a row of the document -- the blank row a block ends
    /// on -- so the band adds its own only when the document does not end on
    /// one: under a fork's `original session …` line, which ends with its
    /// line break alone (`frame_layout.zig:949-968`, "footer request reserves
    /// a boundary gap after a user tail"). Closing the panel gives the row
    /// back with the panel's.
    ///
    /// Only on a band with no turn's row: a running turn's activity takes the
    /// branch above the idle gap (`.thinking`, `:369-385`), whose gap above
    /// the row is the document's here (`ACTIVITY_PADDING`). And only on a
    /// screen with a document row left beside it: upstream solves the footer
    /// first and gives the gap only the rows it leaves
    /// (`available_rows >= footer_height +| min_body_rows +| idle_gap`,
    /// `:393`), so a short screen keeps the panel's rows and drops the blank.
    pub(crate) fn gapped(&self, document_ends_blank: bool) -> Geometry {
        let anchored = self.floated(u32::MAX);
        let top = anchored.activity.unwrap_or(anchored.input_first);
        let owed =
            !document_ends_blank && anchored.input_rows() == 0 && anchored.activity.is_none();
        let gap = u16::from(owed && top > 2);
        Geometry {
            gap,
            content_bottom: top.saturating_sub(gap + anchored.preview + 1).max(1),
            ..anchored
        }
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

    /// How many rows the composer occupies: none while a permission question
    /// has taken the band ([`fits_approval`]).
    pub(crate) fn input_rows(&self) -> u16 {
        // Added before it is subtracted: with no composer rows `input_last` is
        // the row **above** `input_first`, and the other order underflows.
        (self.input_last + 1) - self.input_first
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

/// Whether a screen of `rows` x `cols` could hold a `panel`-row **permission**
/// question.
///
/// Asked with **no** composer rows, because that is the band a permission
/// question is painted in: upstream's question replaces the footer -- the
/// composer, its blank and the statusline -- with its own rules, rows and hint
/// (`render_input.zig:705`; capture `fx-approvals-40x120-edit-ask`), and the
/// draft the composer holds is kept, unpainted, until the answer gives the band
/// back.
pub(crate) fn fits_approval(rows: u16, cols: u16, panel: u16) -> bool {
    solve_band(rows, cols, 0, true, panel).is_some()
}

/// The same, saying whether the turn's activity row is on the screen and how
/// many rows the panel is taking.
///
/// The order is the derivation, bottom up: the hint row is the last row of the
/// screen, the bottom rule is the row above it, the panel -- while there is one
/// -- takes the rows above *that* with a second rule above them, the composer
/// sits on the rows above whichever rule is highest, the activity row and the
/// blank row under it -- when there is work -- are above the composer, and the
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

/// The same, with `more` rows of the activity row's own under it -- a
/// recovery's label wrapped onto them (`activityProjectionRows`,
/// `render_input.zig:689-697`) -- and a waiting prompt's `banner` rows, with
/// the blank row that keeps them from the composer, under the activity row's
/// own blank row (`steering_composer_gap_rows`, `render_input.zig:490-491`;
/// capture `fx-steer-40x120-steer-tool`: the activity row, a blank, `┋ change
/// tone`, a blank, the composer). Either may be zero. `None` when the screen
/// cannot hold them, and the caller then asks for less: a waiting prompt is
/// also said on the hint row, so its rows are a view of it rather than its
/// only sign, and a recovery's first row cut at the edge still says it.
pub(crate) fn solve_band_tall(
    rows: u16,
    cols: u16,
    input_rows: u16,
    panel: u16,
    more: u16,
    banner: u16,
) -> Option<Geometry> {
    let banner = if banner > 0 { banner + 1 } else { 0 };
    solve_padded(
        rows,
        cols,
        input_rows,
        Some(ACTIVITY_PADDING + more + banner),
        panel,
    )
}

/// The derivation, with the activity row's padding decided by the caller.
fn solve_padded(
    rows: u16,
    cols: u16,
    input_rows: u16,
    activity: Option<u16>,
    panel: u16,
) -> Option<Geometry> {
    // No composer rows is a band only while a panel has taken them
    // ([`fits_approval`]): a band with neither has nowhere to put the caret.
    if rows < MIN_ROWS || cols < MIN_COLS || (input_rows == 0 && panel == 0) {
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
        gap: 0,
        preview: 0,
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
    fn a_permission_question_takes_the_composers_rows_and_nothing_else_may() {
        // Upstream's question replaces the footer -- composer, blank,
        // statusline -- with its rules, its rows and its hint
        // (`render_input.zig:705`), so the band is solved with no composer row:
        // the rule that heads the question is where the composer was.
        let idle = solve_band(24, 80, 1, true, 0).expect("24x80 fits a band");
        let asking =
            solve_band(24, 80, 0, true, 8).expect("and a question in place of the composer");
        assert_eq!(asking.input_rows(), 0, "the composer kept a row");
        assert_eq!((asking.divider, asking.hint), (idle.divider, idle.hint));
        assert_eq!(asking.panel_first(), asking.divider - 8);
        assert_eq!(asking.picker_rule(), Some(asking.panel_first() - 1));
        assert_eq!(
            asking.band_top(),
            asking.activity.expect("a turn is running")
        );
        assert_eq!(asking.content_bottom, idle.content_bottom - 8);
        assert!(fits_approval(24, 80, 8));
        // A band with neither a composer nor a question has nowhere for the
        // caret, and is not one.
        assert_eq!(solve_band(24, 80, 0, true, 0), None);
    }

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
    fn a_turn_puts_its_row_above_the_composer_with_a_blank_row_under_it() {
        // The composer, the rule and the hint row do not move when a turn
        // starts -- the caret must not jump -- and the two rows it costs come
        // off the bottom of the document.
        let idle = solve(24, 80, 1).expect("a band");
        let working = solve_with(24, 80, 1, true).expect("a band with a turn in it");
        assert_eq!(idle.activity, None);
        assert_eq!(
            working.activity,
            Some(20),
            "a blank row under it, then the composer"
        );
        assert_eq!(
            working.band_top(),
            20,
            "the row itself is the band's top: the gap above it is the document's"
        );
        assert_eq!(working.divider, idle.divider);
        assert_eq!(working.input_first, idle.input_first);
        assert_eq!(working.hint, idle.hint);
        assert_eq!(working.content_bottom, idle.content_bottom - 2);
        assert_eq!(working.band_rows(), idle.band_rows() + 2);
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
    fn a_short_screen_keeps_the_activity_row_and_drops_its_blank_row() {
        // A three-row composer: with a shorter one the one blank row fits on
        // every screen a band does.
        let roomy = solve_with(8, 20, 3, true).expect("room for the padding");
        assert_eq!(roomy.activity_padding, 1);
        assert_eq!(roomy.activity, Some(roomy.input_first - 2));
        assert_eq!(roomy.content_bottom, 1);
        let tight = solve_with(7, 20, 3, true).expect("the row without its padding");
        assert_eq!(tight.activity_padding, 0);
        assert_eq!(tight.activity, Some(tight.input_first - 1));
        assert_eq!(tight.band_top(), tight.activity.expect("a turn"));
        assert_eq!(tight.content_bottom, 1);
        assert!(
            solve_with(7, 20, 4, true).is_none(),
            "no row at all is left"
        );
    }

    #[test]
    fn a_panel_in_place_of_the_composer_keeps_a_blank_row_over_a_document_without_one() {
        // `top_gap_rows = 1` under a modal (`surface_frame.zig:686@481104fd`),
        // taken as the larger of it and the transcript's own gap
        // (`frame_layout.zig:388-395`; "footer request reserves a boundary
        // gap after a user tail", `:949-968`).
        let modal = solve_band(24, 80, 0, false, 8).expect("a panel in place of the composer");
        let gapped = modal.gapped(false);
        assert_eq!(gapped.gap, 1);
        assert_eq!(
            gapped.band_top(),
            modal.band_top() - 1,
            "the blank is the band's top"
        );
        assert_eq!(
            gapped.content_bottom,
            modal.content_bottom - 1,
            "and a document row"
        );
        assert_eq!(
            (gapped.panel_first(), gapped.divider, gapped.hint),
            (modal.panel_first(), modal.divider, modal.hint),
            "a row under it moved"
        );
        assert_eq!(gapped.band_rows(), modal.band_rows() + 1);
        assert_eq!(
            gapped.content_bottom + gapped.band_rows(),
            gapped.rows,
            "the document and the band no longer tile the screen"
        );
        // The larger of the two gaps, never their sum: a document that ends on
        // its blank row needs none of the band's, and one that comes to end on
        // it gives the band's back.
        assert_eq!(modal.gapped(true), modal);
        assert_eq!(gapped.gapped(true), modal);
        assert_eq!(gapped.gapped(false), gapped, "asked twice, grew twice");
        // Only over a panel that took the composer's rows, and only with no
        // turn's row: a composer, a menu under one and a running turn keep
        // their own gaps.
        let composer = solve(24, 80, 1).expect("a band");
        assert_eq!(composer.gapped(false), composer);
        let menu = solve_band(24, 80, 1, false, 8).expect("a menu under the composer");
        assert_eq!(menu.gapped(false), menu);
        let working = solve_band(24, 80, 0, true, 8).expect("a question in a turn");
        assert_eq!(working.gapped(false), working);
    }

    #[test]
    fn the_blank_row_over_a_panel_floats_with_the_band_and_gives_way_to_the_panel() {
        let modal = solve_band(24, 80, 0, false, 8).expect("a panel in place of the composer");
        // Under a three-row document the band floats up with its blank row as
        // its top, from wherever it was solved.
        let floated = modal.gapped(false).floated(3);
        assert_eq!((floated.band_top(), floated.content_bottom), (4, 3));
        assert_eq!(floated.panel_first(), modal.gapped(false).panel_first() - 9);
        assert_eq!(modal.floated(3).gapped(false).floated(3), floated);
        assert_eq!(floated.gapped(true).floated(3).band_top(), 4);
        // A screen with one document row beside the panel keeps the panel's
        // rows and drops the blank (`available_rows >= footer_height +|
        // min_body_rows +| idle_gap`, `frame_layout.zig:393`).
        let tight = solve_band(MIN_ROWS, MIN_COLS, 0, false, 3).expect("three panel rows");
        assert_eq!(tight.content_bottom, 1);
        assert_eq!(tight.gapped(false), tight);
        let room = solve_band(MIN_ROWS, MIN_COLS, 0, false, 2).expect("two panel rows");
        assert_eq!(room.gapped(false).gap, 1);
        assert_eq!(room.gapped(false).content_bottom, 1);
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
        // A turn's two rows, the composer, two rules, the panel and the hint
        // row, and one row of document.
        // On a short screen the turn's row keeps its place and drops the blank
        // row under it before a question is refused.
        let fits = |rows, cols, panel| solve_band(rows, cols, 1, true, panel).is_some();
        assert!(fits(24, 80, 8));
        assert!(fits(12, 80, 6));
        assert!(!fits(11, 80, 6));
        assert!(fits(24, 80, 18));
        assert!(!fits(24, 80, 19));
        assert!(!fits(24, 19, 8), "a screen too narrow for any band");
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
        assert_eq!(working.activity, Some(5));
        assert_eq!((working.input_first, working.hint), (7, 9));
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
