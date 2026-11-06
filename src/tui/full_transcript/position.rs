//! Where the full transcript's reader is: upstream's
//! `transcript_presentation.State` (`vercel-labs/fx@481104fd
//! src/core/output/transcript_presentation.zig:52-305`), the parts the screen
//! uses.
//!
//! The offset counts rows **from the top** of the page, and `follow_tail` says
//! the screen sits at its end. A page that rebuilds while the reader is not at
//! the end -- a call settled and its heading lost a row, a `* network:` record
//! landed above, the answer landed below -- would show other rows at the same
//! offset, so the position is pinned to the entry that holds it, and to how
//! far into that entry it is, and found there again on the rebuilt page
//! (026a8f4e, b1a53b01). A width change finds the same entry again on the
//! page laid out at the new width.
//!
//! Left out: upstream's `anchor` (the entry an approval's transcript opens
//! at) and the depth and snapshot it keeps for the inline screen. xfx's screen
//! always opens at the tail and has nothing to anchor it elsewhere.

/// Where an entry starts on a page: the id it has, and its first row.
///
/// Ids rise in the order the page shows its entries. [`RECORD`] is every
/// synthetic full-detail record's, as upstream's never-issued id 0 is: a
/// record cannot be found again by it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ItemRow {
    pub(super) id: usize,
    pub(super) row: usize,
}

/// The id every full-detail record shares (`transcript_presentation.zig:
/// 156-158@481104fd`).
pub(super) const RECORD: usize = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Reading {
    pub(super) scroll_rows: usize,
    pub(super) follow_tail: bool,
    pub(super) bookmark: Option<usize>,
    pub(super) bookmark_intra_row: usize,
    pub(super) bookmark_pending: bool,
    pub(super) projection_cols: Option<u16>,
}

impl Default for Reading {
    /// At the tail, as the screen opens (`State`'s defaults, `:52-61`).
    fn default() -> Self {
        Self {
            scroll_rows: 0,
            follow_tail: true,
            bookmark: None,
            bookmark_intra_row: 0,
            bookmark_pending: false,
            projection_cols: None,
        }
    }
}

impl Reading {
    /// `rows` up (toward the head) or down; the screen stops following
    /// (`scroll`, `:133-142`).
    pub(super) fn scroll(self, up: bool, rows: usize) -> Self {
        let mut next = self;
        next.follow_tail = false;
        next.bookmark_pending = false;
        next.scroll_rows = if up {
            next.scroll_rows.saturating_sub(rows)
        } else {
            next.scroll_rows.saturating_add(rows)
        };
        next
    }

    /// The page is laid out at `cols`: at a width it was not laid out at
    /// before, the entry last read is found again (`prepare_projection`,
    /// `:115-122`).
    pub(super) fn prepare_projection(self, cols: u16) -> Self {
        let mut next = self;
        if next
            .projection_cols
            .is_some_and(|previous| previous != cols)
        {
            next = next.queue_bookmark();
        }
        next.projection_cols = Some(cols);
        next
    }

    /// Pins the reading position to the entry it shows on the page
    /// `item_rows` measured, so the next selection keeps that entry in view
    /// (`pin_reading_position`, `:144-164`). Not while the screen follows the
    /// tail or a bookmark is already waiting; and a position inside the
    /// full-detail records keeps its offset, since only records come before
    /// them.
    pub(super) fn pin_reading_position(self, item_rows: &[ItemRow]) -> Self {
        if self.follow_tail || self.bookmark_pending {
            return self;
        }
        let Some(item) = item_rows
            .iter()
            .take_while(|item| item.row <= self.scroll_rows)
            .last()
        else {
            return self;
        };
        if item.id == RECORD {
            return self;
        }
        let mut next = self;
        next.bookmark = Some(item.id);
        next.bookmark_intra_row = self.scroll_rows - item.row;
        next.bookmark_pending = true;
        next
    }

    /// The first row the screen shows on a page of `total_rows`, `visible_rows`
    /// of them at a time: a waiting bookmark's, the tail's when it follows,
    /// and otherwise the offset it had, kept on the page
    /// (`select_visual_offset`, `:166-202`, without the anchor).
    pub(super) fn select_visual_offset(
        self,
        total_rows: usize,
        visible_rows: usize,
        item_rows: &[ItemRow],
    ) -> (Self, usize) {
        let mut next = self;
        let max_offset = total_rows.saturating_sub(visible_rows);
        let offset = if next.bookmark_pending {
            next.bookmark_pending = false;
            next.follow_tail = false;
            match resolve_bookmark_row(item_rows, next.bookmark) {
                None => next.scroll_rows.min(max_offset),
                Some(item_row) => {
                    let item_end =
                        next_bookmark_item_row(item_rows, item_row).unwrap_or(total_rows);
                    let max_intra_row = item_end.saturating_sub(item_row).saturating_sub(1);
                    item_row
                        .saturating_add(next.bookmark_intra_row.min(max_intra_row))
                        .min(max_offset)
                }
            }
        } else if next.follow_tail {
            max_offset
        } else {
            next.scroll_rows.min(max_offset)
        };
        next.scroll_rows = offset;
        next.follow_tail = offset == max_offset;
        (next.refresh_bookmark(item_rows, offset), offset)
    }

    /// `queue_bookmark`, `:248-252`.
    fn queue_bookmark(self) -> Self {
        let mut next = self;
        next.bookmark_pending = !next.follow_tail && next.bookmark.is_some();
        next
    }

    /// The entry at `offset` and how far into it, skipping the records, which
    /// cannot be found again (`refresh_bookmark`, `:254-275`).
    fn refresh_bookmark(self, item_rows: &[ItemRow], offset: usize) -> Self {
        let selected = item_rows
            .iter()
            .take_while(|item| item.row <= offset)
            .filter(|item| item.id != RECORD)
            .last()
            .or_else(|| item_rows.first());
        let Some(item) = selected else {
            return self;
        };
        let mut next = self;
        next.bookmark = Some(item.id);
        next.bookmark_intra_row = offset.saturating_sub(item.row);
        next
    }
}

/// The row of entry `id`, or of the nearest entry before it, or after it, when
/// it is not on the page (`resolve_bookmark_row`, `:278-298`).
fn resolve_bookmark_row(item_rows: &[ItemRow], id: Option<usize>) -> Option<usize> {
    let target = id?;
    let mut preceding: Option<ItemRow> = None;
    let mut following: Option<ItemRow> = None;
    for item in item_rows {
        if item.id == target {
            return Some(item.row);
        }
        if item.id < target {
            if preceding.is_none_or(|kept| item.id > kept.id) {
                preceding = Some(*item);
            }
        } else if following.is_none_or(|kept| item.id < kept.id) {
            following = Some(*item);
        }
    }
    preceding.or(following).map(|item| item.row)
}

/// The first row below `current_row` where an entry starts
/// (`next_bookmark_item_row`, `:300-305`).
fn next_bookmark_item_row(item_rows: &[ItemRow], current_row: usize) -> Option<usize> {
    item_rows
        .iter()
        .find(|item| item.row > current_row)
        .map(|item| item.row)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(rows: &[(usize, usize)]) -> Vec<ItemRow> {
        rows.iter().map(|&(id, row)| ItemRow { id, row }).collect()
    }

    fn reading(scroll_rows: usize) -> Reading {
        Reading {
            scroll_rows,
            follow_tail: false,
            ..Reading::default()
        }
    }

    #[test]
    fn scrolling_saturates_and_leaves_the_tail() {
        // `transcript_presentation.zig:331-340@481104fd`.
        let at_start = Reading {
            scroll_rows: 1,
            ..Reading::default()
        }
        .scroll(true, 3);
        assert_eq!(at_start.scroll_rows, 0);
        assert!(!at_start.follow_tail);
        let at_end = reading(usize::MAX - 1).scroll(false, 3);
        assert_eq!(at_end.scroll_rows, usize::MAX);
    }

    #[test]
    fn a_bookmark_is_clamped_to_its_entry_and_a_missing_one_takes_its_neighbour() {
        // `transcript_presentation.zig:363-395@481104fd`.
        let item_rows = items(&[(10, 1), (30, 8)]);
        let clamped = Reading {
            bookmark: Some(10),
            bookmark_intra_row: 99,
            bookmark_pending: true,
            ..reading(4)
        }
        .select_visual_offset(14, 4, &item_rows);
        assert_eq!(clamped.1, 7);

        let neighbor = Reading {
            bookmark: Some(20),
            bookmark_pending: true,
            ..reading(4)
        }
        .select_visual_offset(14, 4, &item_rows);
        assert_eq!(neighbor.1, 1);
        assert_eq!(neighbor.0.bookmark, Some(10));

        let following = Reading {
            bookmark: Some(5),
            bookmark_intra_row: 1,
            bookmark_pending: true,
            ..reading(4)
        }
        .select_visual_offset(14, 4, &item_rows);
        assert_eq!(following.1, 2);
        assert_eq!(following.0.bookmark, Some(10));
    }

    #[test]
    fn the_reading_position_is_pinned_to_its_entry_for_a_rebuilt_page() {
        // `transcript_presentation.zig:397-448@481104fd`, case by case.
        let outgoing = items(&[(1, 0), (2, 2), (3, 5)]);
        // Entry 2 grew by four rows on the rebuilt page.
        let rebuilt = items(&[(1, 0), (2, 2), (3, 9)]);
        // Row 6 is one row into entry 3 on the outgoing page.
        let reading6 = reading(6);
        assert_eq!(reading6.select_visual_offset(20, 4, &rebuilt).1, 6);
        assert_eq!(
            reading6
                .pin_reading_position(&outgoing)
                .select_visual_offset(20, 4, &rebuilt)
                .1,
            10
        );

        let following_tail = Reading::default();
        assert_eq!(
            following_tail,
            following_tail.pin_reading_position(&outgoing)
        );
        let queued = Reading {
            bookmark: Some(2),
            bookmark_pending: true,
            ..reading(0)
        };
        assert_eq!(queued, queued.pin_reading_position(&outgoing));
        // Upstream's third case, a pending anchor (`:426-432`), has no
        // counterpart: this screen has no anchor.
        assert_eq!(reading6, reading6.pin_reading_position(&[]));

        // A position inside the synthetic full-detail records after the
        // welcome banner keeps its offset instead of snapping back to the
        // banner.
        let with_records = items(&[(1, 0), (RECORD, 2), (RECORD, 4), (5, 6)]);
        let in_records = reading(5);
        let kept = in_records.pin_reading_position(&with_records);
        assert_eq!(in_records, kept);
        assert_eq!(kept.select_visual_offset(20, 4, &with_records).1, 5);
    }

    #[test]
    fn a_bookmark_skips_the_records_unless_there_is_nothing_else() {
        // `transcript_presentation.zig:464-476@481104fd`.
        let item_rows = items(&[(RECORD, 1), (RECORD, 4), (10, 8)]);
        let selected = Reading::default().select_visual_offset(14, 4, &item_rows);
        assert_eq!(selected.0.bookmark, Some(10));
        let records_only = items(&[(RECORD, 1)]);
        let degenerate = Reading::default().select_visual_offset(8, 4, &records_only);
        assert_eq!(degenerate.0.bookmark, Some(RECORD));
    }

    #[test]
    fn a_width_change_finds_the_entry_last_read_on_the_new_page() {
        // `prepare_projection` (`:115-122`) queues the bookmark the last
        // selection refreshed; the page laid out narrower puts the entry
        // lower, and the screen follows it there.
        let wide = items(&[(1, 0), (2, 2), (3, 5)]);
        let (read, offset) = reading(6)
            .prepare_projection(120)
            .select_visual_offset(20, 4, &wide);
        assert_eq!(
            (offset, read.bookmark, read.bookmark_intra_row),
            (6, Some(3), 1)
        );
        let narrow = items(&[(1, 0), (2, 3), (3, 9)]);
        let (moved, offset) = read
            .prepare_projection(80)
            .select_visual_offset(30, 4, &narrow);
        assert_eq!(offset, 10);
        assert!(!moved.follow_tail);
        // The tail is followed across a width change, and nothing is queued.
        let (tail, offset) = Reading::default()
            .prepare_projection(120)
            .select_visual_offset(20, 4, &wide);
        assert!(tail.follow_tail);
        assert_eq!(offset, 16);
        let (tail, offset) = tail
            .prepare_projection(80)
            .select_visual_offset(30, 4, &narrow);
        assert!(tail.follow_tail && !tail.bookmark_pending);
        assert_eq!(offset, 26);
    }
}
