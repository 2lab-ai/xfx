//! Soft-wrap: text in, rows out, measured in cells.
//!
//! Four rules live here. [`composer`] is the composer's: upstream's composer
//! layout (`visual_layout.zig`), which keeps a badge or a pasted block whole,
//! breaks a word wider than a row where it starts, and puts a tab on a tab
//! stop -- with its caret ([`composer_point`]) measured by the same rule, so
//! the caret is never a row away from the character it is supposed to sit on.
//! [`wrap`] is the transcript's rows' and the panels'. Two transcript rows
//! have upstream rules of their own: a notice's ([`wrap_notice`]) and a
//! submitted prompt's card ([`wrap_card`]). The assistant's answer is none
//! of these: upstream wraps it with its assistant wrap, and so does xfx
//! ([`super::prose`], which measures with this module's [`width`] and tiles
//! the text into the same [`Row`]s).
//!
//! A [`Row`] is a **range of the text**, not a copy of it. The caller already
//! owns the string; handing back byte offsets is what lets the composer map a
//! cursor into a row ([`composer_point`]) without a second pass over anything.
//!
//! [`wrap`]'s three rules, each a decision rather than an accident:
//!
//! * **A word moves whole.** A word that does not fit what is left of a row
//!   starts the next one (`visual_layout.zig:278-282`); a word longer than a
//!   whole row is the exception, and breaks at the grapheme that would cross
//!   the margin, because the alternative is a row with nothing on it -- where
//!   [`composer`] breaks it from the row it began on.
//! * **Spaces hang.** The run of spaces that ends a row is *measured* -- so a
//!   [`Row`]'s `width` may be wider than `cols` -- and left on that row rather
//!   than carried down, because the painter clips them
//!   (`visual_layout.zig:146-148`) and a continuation row that began with a
//!   column of blanks would be the visible cost of a character nobody can see.
//! * **Cells, not bytes and not `char`s.** A terminal paints cells, so a wide
//!   glyph costs two and never straddles the margin. Control characters cost
//!   none: they are the escape bytes and line breaks that live *in* the text
//!   and paint nothing. The **tab** is the exception, and item 16 is why: a
//!   paste is the one way a tab reaches the composer, and a character measured
//!   at no cells but written into the text puts the caret a column away from
//!   the glyph it is supposed to sit on. It measures [`TAB_WIDTH`] here and is
//!   painted as that many spaces ([`expand_tabs`], `super::frame::row_text`),
//!   so measure and paint are one number in two places rather than two answers.

use std::borrow::Cow;

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// How many cells a tab paints.
///
/// **A fixed run, not a tab stop.** Every measurement here is of one cluster at
/// a time ([`width`]), so a width that depended on the column the tab happens
/// to be in could not be expressed -- and a wrap that answered "it depends"
/// would disagree with the painter on any row it broke. Four rather than eight
/// because the composer is a band a few rows tall inside a gutter: one
/// character taking a tenth of an eighty-column row is indentation that has
/// eaten the line it indents.
pub(crate) const TAB_WIDTH: u16 = 4;

/// `text` with every tab replaced by the cells it measures.
///
/// The painter's half of [`TAB_WIDTH`]: the composer hands its rows over
/// already expanded, so what reaches the terminal is spaces the wrap has
/// already counted rather than a control the terminal would obey with a tab
/// stop of its own.
///
/// Borrowed when there is no tab, which is every row of nearly every session:
/// the allocation belongs to the pasted indentation that needs it.
pub(crate) fn expand_tabs(text: &str) -> Cow<'_, str> {
    if !text.contains('\t') {
        return Cow::Borrowed(text);
    }
    Cow::Owned(text.replace('\t', &" ".repeat(usize::from(TAB_WIDTH))))
}

/// One visual row: the half-open byte range of the text it shows, and the
/// cells it takes.
///
/// `width` is what the text really measures, which is not always what fits: a
/// row that ends in hanging spaces is wider than the screen on purpose, and the
/// painter is what cuts it (`super::frame::clip`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Row {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) width: u16,
}

/// How many cells `text` paints.
///
/// Control characters count as none. `unicode_width` gives a lone control
/// character a cell of its own -- reasonable for a string that will be printed
/// with its controls visible, wrong for one a terminal will *obey* -- so the
/// clusters made only of them are dropped here rather than measured.
///
/// **An escape sequence is free whole**, not just its `ESC`. `\x1b[31m` is one
/// instruction the terminal executes and four characters it draws nothing for,
/// and measuring the `[31m` as text is how a row with a colour in it loses four
/// columns of the answer to something invisible.
///
/// **Every** sequence, not only the ones a row is allowed to carry, and that is
/// the load-bearing half. Whether a sequence may reach the terminal is
/// [`super::frame::row_text`]'s question and it is asked *after* this one; if
/// this measured a rejected `\x1b[2J` as three printing characters, a narrow
/// row could break in the middle of it -- and then the removal, which only
/// knows how to take a **whole** sequence out, would find half of one on each
/// row and leave the printable tail of it on the screen. One tokenizer,
/// [`super::pacer::escape_at`], answers "how many bytes travel together" for
/// this, for [`super::frame::clip`], and for the removal; the allowlist decides
/// only what is kept.
pub(crate) fn width(text: &str) -> u16 {
    // Printable ASCII (space through `~`, empty allowed) is exactly its own
    // byte length in cells -- no control, no combining mark, no escape
    // sequence, no multi-byte grapheme can be in it. `grid::tokenize_row`,
    // `super::frame::clip` and `super::check`'s decoder already hand this
    // function one grapheme-segmented cluster at a time, most of which are
    // one printable ASCII byte, so the general path's grapheme segmentation
    // and per-cluster `unicode_width` lookup below is repeated work for an
    // answer this loop already knows before it starts. Anything outside that
    // range -- a single non-ASCII byte anywhere in `text` -- falls through
    // untouched: this is a faster route to the same answer, not a second
    // answer.
    if text.bytes().all(|byte| (0x20..=0x7e).contains(&byte)) {
        return u16::try_from(text.len()).unwrap_or(u16::MAX);
    }
    let cells: usize = painting(text)
        .map(|painted| cluster_cells(painted.cluster))
        .sum();
    u16::try_from(cells).unwrap_or(u16::MAX)
}

/// The grapheme cluster `text` begins with, or `None` when it is empty.
///
/// `text.graphemes(true).next()`, and the same answer, reached without
/// segmenting where it is already known: a printable ASCII character (space
/// through `~`) followed by another ASCII byte, or by nothing, is a cluster on
/// its own. No printable ASCII character joins the one after it, and every
/// character that can join the one in front of it -- a combining or spacing
/// mark, a joiner, a variation selector -- is outside ASCII, so the byte after
/// the first is all that has to be looked at. Anything else is segmented
/// exactly as before. The painter's tokenizer cuts a row this way one cluster
/// at a time ([`super::frame::clip`], [`super::grid::tokenize_row`]), and a row
/// of plain text is nearly all such clusters.
pub(crate) fn first_cluster(text: &str) -> Option<&str> {
    match text.as_bytes() {
        [first, after @ ..]
            if (0x20..=0x7e).contains(first) && after.first().is_none_or(u8::is_ascii) =>
        {
            text.get(..1)
        }
        _ => text.graphemes(true).next(),
    }
}

/// One cluster that paints something, and the bytes it travels with.
///
/// `start` is where the escape sequences *in front of* the cluster begin, and
/// that is the whole reason this is a struct rather than a pair: a break taken
/// at `start` moves a colour down with the word it colours, and a break taken
/// after them would leave the colour on the row above and cut the answer's
/// attributes off the text they belong to. For a sequence that will be removed
/// rather than kept it matters more than cosmetically -- a break inside one is
/// a fragment the removal cannot recognize.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Painted<'a> {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) cluster: &'a str,
}

/// The clusters of `text` that paint something, with the escape sequences
/// stepped over.
///
/// A sequence is not a cluster to be measured, wrapped after, or broken inside:
/// it belongs to the text around it. Its bytes stay in the row, because a
/// [`Row`] is a range and the ranges tile the text -- a sequence between two
/// clusters is inside whichever row those clusters put it in.
pub(crate) fn painting(text: &str) -> impl Iterator<Item = Painted<'_>> {
    let mut cursor = 0usize;
    std::iter::from_fn(move || {
        let start = cursor;
        while cursor < text.len() {
            let rest = &text[cursor..];
            if let Some(len) = super::pacer::escape_at(rest) {
                cursor += len;
                continue;
            }
            let cluster = rest.graphemes(true).next().unwrap_or(rest);
            cursor += cluster.len();
            return Some(Painted {
                start,
                end: cursor,
                cluster,
            });
        }
        None
    })
}

/// The rows `text` occupies on a screen `cols` wide, in order, tiling the whole
/// of it.
///
/// Every byte of `text` is in exactly one row, the first row starts at 0, the
/// last ends at `text.len()`, and a row's `end` is the next row's `start` --
/// including the line break that ended a row, which belongs to the row it ended
/// rather than to the one it began. There is always at least one row: a text
/// that ends in a newline has an empty last row, which is where the caret goes
/// after the newline is typed.
pub(crate) fn wrap(text: &str, cols: u16) -> Vec<Row> {
    // A zero-column screen is not one the layout will ever produce
    // (`layout::MIN_COLS`), and a zero budget here would be an empty row per
    // grapheme forever.
    let cols = cols.max(1);
    let mut rows = Vec::new();
    // Where the row being built starts, and what it has taken so far.
    let mut start = 0usize;
    let mut used = 0u16;
    // Where the last word began, when a space has been seen on this row since
    // the row began. That is the point a break moves the word from.
    let mut word: Option<usize> = None;
    let mut after_space = false;

    for painted in painting(text) {
        let Painted {
            start: index,
            end,
            cluster,
        } = painted;
        if is_line_break(cluster) {
            // A hard break always breaks, and takes its own bytes with it --
            // and `end` rather than `index + cluster.len()`, because a colour
            // in front of the break travels with it and a row that ended
            // between the two would end inside an escape sequence.
            rows.push(Row {
                start,
                end,
                width: used,
            });
            start = end;
            used = 0;
            word = None;
            after_space = false;
            continue;
        }
        let cells = cluster_width(cluster);
        if cluster == " " {
            // Hanging: a space past the margin is measured and stays where it
            // is, so the row it ends can be wider than the screen.
            used = used.saturating_add(cells);
            after_space = true;
            continue;
        }
        if after_space {
            word = Some(index);
            after_space = false;
        }
        // `index > start` is what stops a single grapheme wider than the whole
        // row from pushing an empty row and trying again forever.
        if used.saturating_add(cells) > cols && index > start {
            let split = match word {
                // The word began on this row, so the whole of it moves down.
                Some(at) if at > start => at,
                // Either no word boundary on this row at all, or the row *is*
                // the middle of one word: break at the grapheme that would
                // cross the margin.
                _ => index,
            };
            rows.push(Row {
                start,
                end: split,
                width: width(&text[start..split]),
            });
            start = split;
            // Whatever of the word had already been counted onto the row that
            // just ended is now the beginning of this one.
            used = width(&text[split..index]);
            word = None;
        }
        used = used.saturating_add(cells);
    }
    rows.push(Row {
        start,
        end: text.len(),
        width: used,
    });
    rows
}

/// The rows a **notice**'s text occupies on a screen `cols` wide: upstream's
/// notice wrap (`scanNoticeLine`,
/// `vercel-labs/fx@c1db9191 src/ui/render_engine/transcript_blocks.zig:1651-1702`),
/// which is not [`wrap`]'s.
///
/// Two differences, and both are upstream's:
///
/// * **A path breaks after a `/` or a `\`.** A notice that names a file --
///   `Trace saved at /tmp/…/fx-trace-….md.` -- keeps as much of the path as
///   fits on the row it began on and carries the rest down from after its last
///   separator, rather than moving the whole path down as one word.
/// * **A break at a space drops the space.** The spaces at a break are on
///   neither row: the row ends before them and the next one starts after them,
///   where [`wrap`] hangs them on the row they end. So the rows do **not**
///   tile the text, and a caller that needs every byte placed is not a notice.
///
/// A word longer than a whole row breaks at the cluster that would cross the
/// margin, as in [`wrap`]; a single cluster wider than the row is a row of its
/// own. A hard line break ends a row and is on neither. Text with nothing in
/// it is one empty row.
pub(crate) fn wrap_notice(text: &str, cols: u16) -> Vec<Row> {
    let cols = cols.max(1);
    let mut rows = Vec::new();
    let mut start = 0usize;
    loop {
        let (row, next) = notice_row(text, start, cols);
        rows.push(row);
        // `next > start` always, so this ends: every row takes at least one
        // cluster, a space, or a break.
        if next >= text.len() {
            return rows;
        }
        start = next;
    }
}

/// One notice row from `start`, and where the row after it begins
/// (`scanNoticeLine`'s `end` and `next`).
fn notice_row(text: &str, start: usize, cols: u16) -> (Row, usize) {
    let row = |end: usize, width: u16| Row { start, end, width };
    let mut used = 0u16;
    // The last place this row may end, the place the next one would begin,
    // and the cells the row has there.
    let mut last_break: Option<(usize, usize, u16)> = None;
    for painted in painting(&text[start..]) {
        let index = start + painted.start;
        let end = start + painted.end;
        let cluster = painted.cluster;
        let at = end - cluster.len();
        if is_line_break(cluster) {
            return (row(index, used), end);
        }
        if cluster == " " || cluster == "\t" {
            let next = at
                + text[at..]
                    .bytes()
                    .take_while(|byte| *byte == b' ' || *byte == b'\t')
                    .count();
            let cells = cluster_width(cluster);
            if used.saturating_add(cells) > cols {
                return (row(index, used), next);
            }
            last_break = Some((index, next, used));
            used = used.saturating_add(cells);
            continue;
        }
        let cells = cluster_width(cluster);
        if used.saturating_add(cells) > cols {
            if let Some((end, next, width)) = last_break {
                if end > start {
                    return (row(end, width), next);
                }
            }
            if index == start {
                return (row(end, cells), end);
            }
            return (row(index, used), index);
        }
        used = used.saturating_add(cells);
        if cluster == "/" || cluster == "\\" {
            last_break = Some((end, end, used));
        }
    }
    (row(text.len(), used), text.len())
}

/// The rows a submitted prompt's **card** occupies on a screen `cols` wide:
/// upstream's `wrapCut`, looped over each line
/// (`vercel-labs/fx@c1db9191 src/ui/assistant/user_message_card.zig:45-77`,
/// `:358-382`), which is neither [`wrap`]'s nor [`wrap_notice`]'s.
///
/// Each row is cut from what is left of its line, and the cut is one rule:
///
/// * **The longest run that fits** is measured first -- every cluster that
///   still has room, with the escape sequences and the clusters that take no
///   cell carried along wherever they fall, the ones in front of the cluster
///   that does not fit included. When that is the rest of the line, it is the
///   row.
/// * **Otherwise the row ends at the last space or tab inside that run**, and
///   the next row begins one byte after it: exactly one blank is dropped per
///   break, so of a run of spaces the others stay where they fell -- at the end
///   of the row, or at the start of the next one, the row's bold over them.
///   The run stops *before* a space that would cross the margin, so a word that
///   ends at the margin is not followed by a space inside it: the break is at
///   an earlier space, which takes the word down with it, or, with none, the
///   word is the row and the next one begins **on** the space -- whose cut
///   finds it as its last blank when nothing after it fits beside it, keeps
///   nothing before it, and leaves a row with nothing on it.
/// * **With no blank inside it, the run is the row**, a word longer than a
///   whole row broken at the cluster that would cross; and a single cluster
///   wider than the whole row is a row of its own.
///
/// A hard line break ends a row and is on neither; an empty line is one empty
/// row, and so is an empty text. The rows do **not** tile the text.
///
/// A **tab** is a blank the cut may break at and takes no cell in its
/// measurement ([`card_cells`]), as upstream's; a row's `width` counts it the
/// same way, and what it paints is the caller's (the screen's tab stops).
///
/// A blank is found among the clusters rather than the bytes, so a space inside
/// an escape sequence -- an OSC 8 target -- is never a break; upstream scans
/// the bytes, and the only sequences a card carries (its bold, a badge's link,
/// whose target is percent-encoded, `super::hyperlink`) have no space in them,
/// so the two read the same breaks.
pub(crate) fn wrap_card(text: &str, cols: u16) -> Vec<Row> {
    let cols = cols.max(1);
    let mut rows = Vec::new();
    let mut line_start = 0usize;
    for line in text.split('\n') {
        let line_end = line_start + line.len();
        if line.is_empty() {
            rows.push(Row {
                start: line_start,
                end: line_start,
                width: 0,
            });
        }
        let mut start = line_start;
        while start < line_end {
            let (keep, skip) = card_cut(&text[start..line_end], cols);
            let end = start + keep;
            rows.push(Row {
                start,
                end,
                width: card_width(&text[start..end]),
            });
            // `skip > 0` always, so this ends: a cut keeps a cluster, or drops
            // a blank, or both.
            start += skip;
        }
        line_start = line_end + 1;
    }
    rows
}

/// One card row's cut of `text` at `cols`: how many bytes the row keeps, and
/// how many it consumes (`wrapCut`'s `keep_bytes` and `skip_bytes`,
/// `user_message_card.zig:57-77`).
fn card_cut(text: &str, cols: u16) -> (usize, usize) {
    let mut used = 0u16;
    // Where the longest run that fits ends, the last blank inside it, and
    // where the first cluster ends.
    let mut fits = text.len();
    let mut blank: Option<usize> = None;
    let mut first: Option<usize> = None;
    for painted in painting(text) {
        let at = painted.end - painted.cluster.len();
        first.get_or_insert(painted.end);
        let cells = card_cells(painted.cluster);
        // `used` never passes `cols`: a cluster is only counted when it fits.
        if cells > cols - used {
            fits = at;
            break;
        }
        used += cells;
        if matches!(painted.cluster.as_bytes().first(), Some(b' ' | b'\t')) {
            blank = Some(at);
        }
    }
    if fits == text.len() {
        return (fits, fits);
    }
    if let Some(blank) = blank {
        return (blank, blank + 1);
    }
    if fits == 0 {
        // Not even the first cluster fits, and nothing is in front of it: it
        // is the row on its own, so the cut moves (`:66-75`).
        let unit = first.unwrap_or(text.len());
        return (unit, unit);
    }
    (fits, fits)
}

/// One cluster's cells as the card's cut counts them: [`cluster_width`], but
/// a **tab is none**. Upstream measures every control at no cells
/// (`display_width.zig`'s `displayUnitAt`) and writes the tab to the terminal
/// as it is, which advances to the screen's next stop; the transcript paints
/// it as those spaces (`super::prose::expand_tabs`).
fn card_cells(cluster: &str) -> u16 {
    if cluster == "\t" {
        return 0;
    }
    cluster_width(cluster)
}

/// The cells a card row takes as its cut measured it: a tab at none
/// ([`card_cells`]).
fn card_width(row: &str) -> u16 {
    let cells: usize = painting(row)
        .map(|painted| usize::from(card_cells(painted.cluster)))
        .sum();
    u16::try_from(cells).unwrap_or(u16::MAX)
}

/// Which row a byte offset is on, and how many cells are to the left of it
/// there.
///
/// A cursor sitting exactly on a soft-wrap boundary belongs to the **following**
/// row (`visual_layout.zig:432-437`): the character it is about to type lands
/// there, so that is where the caret has to be. A cursor at the very end of the
/// text is on the last row, which is the one case where an offset equal to a
/// row's `end` stays on that row.
///
/// `text` is a parameter, and the plan's signature (`rows` and `cursor` only)
/// is not implementable without it: a column is a count of **cells**, and no
/// number of byte offsets can tell you how many cells the bytes between them
/// paint. See `a_cursor_after_a_wide_glyph_is_a_column_of_cells_rather_than_of_bytes`.
///
/// # The caller's obligation
///
/// **`cursor` must be a grapheme-cluster boundary of `text`.** Two things rest
/// on it, and they fail differently:
///
/// * An offset that is not even a `char` boundary **panics** the slice. This
///   function does not clamp to one, deliberately: a caret silently moved to a
///   neighbouring byte is a caret that lies about where the next character will
///   land, and Task 9's editor already moves by grapheme
///   (`unicode_segmentation`) precisely so it cannot produce one.
/// * An offset that is a `char` boundary *inside* a cluster -- between an `e`
///   and the combining accent that composes with it, or between two scalars of
///   a ZWJ emoji -- does not panic and is not meaningful either: it reports a
///   column in the middle of a cell the terminal draws as one glyph.
///
/// So this is Task 9 and Task 10's invariant to keep, not this function's to
/// repair. `a_composed_grapheme_is_one_column_however_many_bytes_it_took` pins
/// what the boundaries themselves must report.
// Task 10's editor is the first caller -- `Up`/`Down` and the caret both ask
// this question -- and it is here rather than there because it is the inverse
// of `wrap`, and an inverse that lives in another module drifts from it.
#[allow(dead_code)]
pub(crate) fn cursor_point(text: &str, rows: &[Row], cursor: usize) -> (usize, u16) {
    for (index, row) in rows.iter().enumerate() {
        let last = index + 1 == rows.len();
        if cursor >= row.end && !last {
            continue;
        }
        if cursor >= row.end {
            // The end of the last row: the row's own measurement, which is
            // what a hanging run of spaces makes different from `cols`.
            return (index, row.width);
        }
        return (index, width(&text[row.start..cursor.max(row.start)]));
    }
    (0, 0)
}

/// Whether a grapheme cluster is a hard line break.
///
/// `"\r\n"` is **one** cluster, which is why this is a function rather than a
/// comparison against `'\n'`: a CRLF matched as two would end a row on the CR
/// and open an empty one for the LF.
///
/// A **lone** carriage return is not one. What a bare CR means is a policy, and
/// it has exactly one owner -- [`super::transcript::normalize`], which turns it
/// into a newline before any of this text reaches here -- so restating it here
/// would be a second answer to the same question with nothing keeping the two
/// in step. Reaching this function it is an ordinary control character,
/// measured at no cells and stripped by the painter.
pub(crate) fn is_line_break(cluster: &str) -> bool {
    cluster == "\n" || cluster == "\r\n"
}

/// One cluster's cells, as [`width`] counts them.
pub(crate) fn cluster_width(cluster: &str) -> u16 {
    u16::try_from(cluster_cells(cluster)).unwrap_or(u16::MAX)
}

/// One cluster's cells: none when it paints nothing.
///
/// The tab is the one control with a width, because it is the one control a
/// draft may hold and the painter draws it ([`TAB_WIDTH`]).
///
/// **A text-presentation selector does not narrow its base.** A character and
/// U+FE0E take the cells the character takes on its own -- two for a wide one
/// such as the hint row's `⚡︎` -- which is upstream's count
/// (`display_width.zig:251-256`, `c1db919`; `crate::markdown::display_width`
/// is its port) and what a terminal advances by. The width table's own answer
/// for the pair is one cell, and a row measured by it would put whatever
/// follows the sign a cell to the left of where the terminal paints it.
///
/// **A cluster that holds a soft hyphen or a Hangul filler takes the cells
/// upstream counts** ([`counted_as_upstream`]), where the width table counts
/// them as none; **and so does one that holds a scalar a terminal was measured
/// drawing wider than the width table counted it, where upstream counts it
/// wider too** ([`FX_COUNTED`]: `U+302E` after a letter, a vowel sign such as
/// Bengali `U+09BE`, a regional indicator).
fn cluster_cells(cluster: &str) -> usize {
    if cluster == "\t" {
        return usize::from(TAB_WIDTH);
    }
    if cluster.chars().all(char::is_control) {
        return 0;
    }
    if cluster
        .chars()
        .any(|character| counted_as_upstream(character) || fx_counted(character))
    {
        return upstream_cells(cluster).max(cluster.width());
    }
    let mut chars = cluster.chars();
    if let (Some(base), Some('\u{fe0e}'), None) = (chars.next(), chars.next(), chars.next()) {
        return unicode_width::UnicodeWidthChar::width(base).unwrap_or(0);
    }
    cluster.width()
}

/// A cluster holding one of [`counted_as_upstream`]'s or [`FX_COUNTED`]'s
/// code points, counted a display unit at a time as upstream counts a line
/// (`displayUnitAt`, `src/core/shared/display_width.zig:234-263@481104fd`;
/// [`crate::markdown::display_width::display_unit_at`] is its port): an RGI
/// emoji sequence whole, a character and its variation selector together,
/// and every other code point alone -- but no unit under the width table's
/// own count of it.
///
/// A unit, not a code point, for the regional indicators: upstream counts one
/// alone as two cells (`emoji_presentation_ranges`,
/// `unicode_display_data.zig:204@481104fd`) and a pair that is an RGI flag as
/// two cells whole, so a flag counted a code point at a time would be four.
///
/// The one place the two disagree in such a cluster is a skin-tone modifier
/// (`U+1F3FB`-`U+1F3FF`): upstream counts it as none wherever it stands (a
/// lone modifier is no RGI sequence; `super::tool_status` records the same
/// disagreement), the width table two after anything that is not its base,
/// and both terminals draw it so, on a cell pair of its own -- `U+FFA0` with a
/// modifier after it is three cells on tmux 3.7b, where upstream's count is
/// one (`.prd/tui-phase5/receipts/width-disputed/clusters.txt`). Every other
/// code point these clusters were measured with -- the 14, `U+0301`,
/// `U+FE0E`, `U+034F`, `U+200D`, the jamo, and every scalar of
/// [`FX_COUNTED`] -- the table counts no more than upstream does, so for them
/// this is upstream's count exactly.
fn upstream_cells(cluster: &str) -> usize {
    let bytes = cluster.as_bytes();
    let mut cells = 0;
    let mut index = 0;
    while index < bytes.len() {
        let unit = crate::markdown::display_width::display_unit_at(bytes, index);
        // A unit of a valid `str` ends on a character boundary; walked on to
        // one regardless, so a slice here can never split a character.
        let mut end = (index + unit.byte_len.max(1)).min(bytes.len());
        while !cluster.is_char_boundary(end) {
            end += 1;
        }
        let text = &cluster[index..end];
        let mut chars = text.chars();
        let table = match (chars.next(), chars.next()) {
            (Some(character), None) => {
                unicode_width::UnicodeWidthChar::width(character).unwrap_or(0)
            }
            _ => text.width(),
        };
        cells += unit.cell_width.max(table);
        index = end;
    }
    cells
}

/// Whether a cluster holding `character` is counted as upstream counts it --
/// one code point at a time ([`crate::markdown::display_width`], the port of
/// `display_width.zig`) -- rather than by the width table: a soft hyphen
/// (`U+00AD`), the Hangul fillers (`U+115F`, `U+1160`, `U+3164`, `U+FFA0`)
/// and `U+FFF0`-`U+FFF8`.
///
/// The width table counts every one of them as none (Unicode's
/// `Default_Ignorable_Code_Point`), and so it counts the cluster one stands
/// in -- `U+3164`, `U+FFA0`, `U+115F` and `U+1160` join the mark, the joiner
/// or the jamo after them; the soft hyphen and `U+FFF0`-`U+FFF8` are clusters
/// of their own -- as the rest of the cluster alone. But terminals do not all
/// draw them as none: tmux 3.7b gives the soft hyphen, `U+FFA0` and
/// `U+FFF0`-`U+FFF8` a cell each and `U+115F` two; libghostty-vt gives the
/// soft hyphen one. A row clipped at the margin by the table's count ran past
/// it, and the terminal wrote the row's tail into its last cell
/// (`p5-transcript-diffwide`, Ctrl-O: `…TAI8` where fx's row ends `…TAIL`).
///
/// Upstream counts each code point of such a cluster on its own (`runeWidth`,
/// `src/core/shared/display_width.zig:275-282@481104fd`: one cell, two where
/// East Asian Width makes it wide, `wide_ranges`, `unicode_display_data.zig:41`,
/// `:85`; none for a combining mark or a joiner, `:409-421`). That count was
/// measured against both terminals for each of these alone and with
/// `U+0301`, `U+FE0E`, `U+034F` or `U+200D` after it, and in the conjoining
/// jamo `U+115F U+1160`, `U+1100 U+1160`, `U+115F U+1161` and
/// `U+1100 U+1160 U+11A8` (`.prd/tui-phase5/receipts/width-disputed/
/// clusters.txt`): never fewer cells than either draws, so a row measured by
/// it never runs past the margin **in those forms**. With a skin-tone
/// modifier after it the count is the code points' own, each no less than
/// the width table's ([`upstream_cells`]): `U+FFA0 U+1F3FD` three cells, as
/// tmux draws it, where upstream's count is one; measured too, and no
/// terminal draws more. **Not closed:** with `U+FE0F` after a soft hyphen,
/// `U+FFA0` or `U+FFF0`-`U+FFF8`, tmux draws one cell more than this count
/// (and libghostty-vt no more than it) -- the selector after a character
/// that takes no emoji presentation, which tmux widens by a cell when the
/// character before it draws narrower than two cells: `U+115F U+FE0F` stays
/// two, as `U+115F` alone does, where `U+1160 U+FE0F` and `U+3164 U+FE0F`
/// take one (clusters.txt). It is `super::drawn_wider`'s `U+FE0F`, which
/// fx counts as this does: no count closes it. A file's or a tool's line on
/// Ctrl-O names it (`super::full_transcript`'s `drawn_wider_named`), the
/// review names it, and an answer and the composer write it as fx does
/// (`scripts/fxcap/parity-ledger.toml`, `ctrl-o-drawn-wider-named`;
/// `.prd/tui-phase5/receipts/ctrlo-wider/`).
///
/// **It is more than they draw, often**, and that is upstream's count kept
/// rather than a terminal's: `U+1160` one cell and `U+3164` two where both
/// terminals draw none; `U+115F`, `U+FFA0` and `U+FFF0`-`U+FFF8` one or two
/// where libghostty-vt draws none; a jamo cluster with a filler one or two
/// more than both. A row with one of them is laid out as if it were that
/// much wider [추론, but for two, captured: in `p5-transcript-diffwide2`'s
/// Ctrl-O line 2, the text after the `U+3164` and `U+1160` stands three cells
/// left of where the grid holds it -- fx's frame re-anchors there and shows
/// the three cells blank (the WIDTH-DISPUTED receipt's `green.txt:18`); its
/// line 5, four `U+1160 U+1F3FD` counted three cells each where tmux draws
/// two, is cut after two of them, 108 of the row's 112 cells
/// (`green.txt:21`)]: the frame may skip the `EL`
/// that would clear the right edge of a row it thinks is full
/// (`super::frame`'s `place`) and leave a stale cell there; a partial row
/// update (`super::grid`'s row diff) may leave cells the terminal never moved
/// over; and the composer's caret, placed by [`composer_cells`], sits that
/// many cells right of the text before it. None of them can put a byte past the margin.
fn counted_as_upstream(character: char) -> bool {
    matches!(
        u32::from(character),
        0x00ad | 0x115f | 0x1160 | 0x3164 | 0xffa0 | 0xfff0..=0xfff8
    )
}

/// The scalars a terminal was measured to draw wider than xfx counted them
/// and that upstream counts as more cells than xfx did, one code point at a
/// time: 3,838 in 64 ranges, first and last code point, in order.
///
/// Read off `super::drawn_wider`'s sweep before it was taken again
/// (`scripts/drawn-wider/sweep.sh` at `e6cde019`, the 3,881 scalars that table
/// listed then), set against upstream's own count of each alone (`runeWidth`,
/// `src/core/shared/display_width.zig:275-282@481104fd`):
/// `.prd/tui-phase5/receipts/ctrlo-wider/`. They are combining and spacing
/// marks the width table counts as none or as part of the character before
/// them (`U+0605`, `U+09BE`, `U+302E`, `U+FF9E`, ...), the regional
/// indicators `U+1F1E6`-`U+1F1FF` (one cell alone to the width table, two to
/// upstream, which holds them in `emoji_presentation_ranges`,
/// `unicode_display_data.zig:204@481104fd`), and the unassigned
/// default-ignorable `U+E0000`, `U+E0080`-`U+E00FF` and `U+E01F0`-`U+E0FFF`.
/// Upstream's count of every one of them is no less than either terminal
/// draws it.
#[rustfmt::skip]
pub(super) const FX_COUNTED: [(u32, u32); 64] = [
    (0x00605, 0x00605), (0x0070f, 0x0070f), (0x00890, 0x00891), (0x008e2, 0x008e2), (0x009be, 0x009be),
    (0x009d7, 0x009d7), (0x00b3e, 0x00b3e), (0x00b57, 0x00b57), (0x00bbe, 0x00bbe), (0x00bd7, 0x00bd7),
    (0x00cc0, 0x00cc0), (0x00cc2, 0x00cc2), (0x00cc7, 0x00cc8), (0x00cca, 0x00ccb), (0x00cd5, 0x00cd6),
    (0x00d3e, 0x00d3e), (0x00d4e, 0x00d4e), (0x00d57, 0x00d57), (0x00dcf, 0x00dcf), (0x00ddf, 0x00ddf),
    (0x01715, 0x01715), (0x01734, 0x01734), (0x01b35, 0x01b35), (0x01b3b, 0x01b3b), (0x01b3d, 0x01b3d),
    (0x01b43, 0x01b44), (0x01baa, 0x01baa), (0x01bf2, 0x01bf3), (0x0302e, 0x0302f), (0x0a8fa, 0x0a8fa),
    (0x0a953, 0x0a953), (0x0a9c0, 0x0a9c0), (0x0ff9e, 0x0ff9f), (0x111c0, 0x111c0), (0x111c2, 0x111c3),
    (0x11235, 0x11235), (0x1133e, 0x1133e), (0x1134d, 0x1134d), (0x11357, 0x11357), (0x113b8, 0x113b8),
    (0x113c2, 0x113c2), (0x113c5, 0x113c5), (0x113c7, 0x113c9), (0x113cf, 0x113cf), (0x113d1, 0x113d1),
    (0x114b0, 0x114b0), (0x114bd, 0x114bd), (0x115af, 0x115af), (0x116b6, 0x116b6), (0x11930, 0x11930),
    (0x1193d, 0x1193d), (0x1193f, 0x1193f), (0x11941, 0x11941), (0x11a84, 0x11a89), (0x11d46, 0x11d46),
    (0x11f02, 0x11f02), (0x11f41, 0x11f41), (0x16ff0, 0x16ff1), (0x1d165, 0x1d166), (0x1d16d, 0x1d172),
    (0x1f1e6, 0x1f1ff), (0xe0000, 0xe0000), (0xe0080, 0xe00ff), (0xe01f0, 0xe0fff),
];

/// Whether `character` is one of [`FX_COUNTED`]'s.
fn fx_counted(character: char) -> bool {
    let code = u32::from(character);
    FX_COUNTED
        .binary_search_by(|&(first, last)| {
            if last < code {
                std::cmp::Ordering::Less
            } else if first > code {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

/// The cells the composer's gutter takes in front of every row, the first
/// and every continuation alike (`inputPrefix`, `visual_layout.zig:689-694`):
/// what a tab stop is counted from.
const COMPOSER_GUTTER: u16 = 2;

/// The composer's rows on a screen whose text is `cols` wide, tiling the
/// whole of `text`: upstream's composer layout (`Iterator`,
/// `visual_layout.zig:100-330`), which is [`wrap`]'s rule with four
/// differences.
///
/// * **A word wider than a whole row starts where it is.** A word moves to
///   the next row only when it fits there (`shouldSoftWrapWord`, `:265-282`);
///   one wider than a row is broken at the margin from the row it began on,
///   rather than leaving that row nearly empty.
/// * **An entity is one unit** (`peekRawUnit`, `:174-231`). `units` are the
///   draft's badges and pasted-text blocks, sorted: each is placed whole, on
///   this row if it fits and on the next if not, and never broken at the
///   spaces inside its own words. A word ends where one begins, and the text
///   after one starts a word (`consumeRawUnit`, `:244-247`; `wordEndsAt`,
///   `:299-310`).
/// * **A tab advances to the next tab stop**: every eighth column of the
///   screen, gutter included, and none at the right margin
///   (`tabAdvance`, `:714-723`). It ends a word, as a space does
///   (`isWordBreakByte`, `:331-333`), and is painted as that many spaces
///   ([`composer_expand`]).
/// * **Nothing wider than the row is refused.** A unit wider than a whole row
///   takes the row it starts and the next unit begins a new one
///   (`oversized_empty_row`, `:251-255`), as [`wrap`]'s too-wide glyph does.
///
/// Spaces hang past the margin as in [`wrap`] (`shouldBreakBefore`,
/// `:134-138`), a line break ends its row and belongs to it, and a cluster
/// that paints nothing never begins a row.
pub(crate) fn composer(text: &str, cols: u16, units: &[std::ops::Range<usize>]) -> Vec<Row> {
    let available = cols.max(1);
    let mut rows = Vec::new();
    let mut start = 0usize;
    let mut used = 0u16;
    let mut at_word_start = true;
    let mut force_wrap = false;
    let mut units = units.iter().peekable();
    let mut index = 0usize;
    while index < text.len() {
        while units.peek().is_some_and(|unit| unit.end <= index) {
            units.next();
        }
        let unit = units
            .peek()
            .filter(|unit| unit.start == index)
            .map(|unit| (*unit).clone());
        let (end, cells, text_cluster) = match &unit {
            Some(unit) => (unit.end, width(&text[unit.clone()]), None),
            None => {
                let cluster = text[index..]
                    .graphemes(true)
                    .next()
                    .unwrap_or(&text[index..]);
                let cells = if cluster == "\t" {
                    composer_tab(used, cols)
                } else {
                    cluster_width(cluster)
                };
                (index + cluster.len(), cells, Some(cluster))
            }
        };
        if text_cluster.is_some_and(is_line_break) {
            rows.push(Row {
                start,
                end,
                width: used,
            });
            start = end;
            index = end;
            used = 0;
            at_word_start = true;
            force_wrap = false;
            continue;
        }
        let space = text_cluster == Some(" ");
        let word_moves = text_cluster.is_some_and(|cluster| cluster != "\t")
            && at_word_start
            && used > 0
            && !force_wrap
            && used < available
            && {
                let word = word_width(text, index, units.peek().map(|unit| unit.start), available);
                word > available - used && word <= available
            };
        let crosses = cells > 0
            && (force_wrap || (used > 0 && (used >= available || used + cells > available)));
        if !space && (word_moves || crosses) {
            rows.push(Row {
                start,
                end: index,
                width: used,
            });
            start = index;
            used = 0;
            at_word_start = true;
            force_wrap = false;
            continue;
        }
        if used == 0 && cells > available {
            force_wrap = true;
        }
        used = used.saturating_add(cells);
        at_word_start = match text_cluster {
            Some(cluster) => cluster == " " || cluster == "\t",
            None => true,
        };
        index = end;
    }
    rows.push(Row {
        start,
        end: text.len(),
        width: used,
    });
    rows
}

/// The cells of the word that begins at `index`, counted until a space, a
/// tab, a line break or the next unit (`next_unit`), and no further than one
/// past `cap` (`measureWordWidth`, `visual_layout.zig:284-297`).
fn word_width(text: &str, index: usize, next_unit: Option<usize>, cap: u16) -> u16 {
    let mut cells = 0u16;
    let mut at = index;
    for cluster in text[index..].graphemes(true) {
        if next_unit == Some(at) || cluster == " " || cluster == "\t" || is_line_break(cluster) {
            break;
        }
        cells = cells.saturating_add(cluster_width(cluster));
        if cells > cap {
            break;
        }
        at += cluster.len();
    }
    cells
}

/// How far a tab at `column` of a composer row `cols` wide advances: to the
/// next eighth column of the screen, counted with the gutter, and none at
/// the right margin or past it (`tabAdvance`, `visual_layout.zig:714-723`;
/// `nextTabStopColumn`, `display_width.zig:290-300`).
pub(crate) fn composer_tab(column: u16, cols: u16) -> u16 {
    let screen = cols.max(1).saturating_add(COMPOSER_GUTTER);
    // One-based, as upstream counts, and never past the right margin.
    let at = COMPOSER_GUTTER
        .saturating_add(column)
        .min(screen - 1)
        .saturating_add(1);
    if at >= screen {
        return 0;
    }
    let stop = ((at - 1) / 8 + 1).saturating_mul(8).saturating_add(1);
    stop.min(screen) - at
}

/// The cells a composer row paints from its start to `offset`: [`width`],
/// with each tab the advance it has where it stands ([`composer_tab`]).
pub(crate) fn composer_cells(text: &str, row: &Row, offset: usize, cols: u16) -> u16 {
    let end = offset.clamp(row.start, row.end);
    let part = &text[row.start..end];
    if !part.contains('\t') {
        return width(part);
    }
    part.graphemes(true).fold(0u16, |used, cluster| {
        let cells = if cluster == "\t" {
            composer_tab(used, cols)
        } else {
            cluster_width(cluster)
        };
        used.saturating_add(cells)
    })
}

/// A composer row's text as it is painted: each tab the spaces it advances
/// ([`composer_tab`]), so the painter writes the cells the layout counted.
pub(crate) fn composer_expand<'a>(row: &'a str, cols: u16) -> Cow<'a, str> {
    if !row.contains('\t') {
        return Cow::Borrowed(row);
    }
    let mut out = String::with_capacity(row.len() + 8);
    let mut used = 0u16;
    for cluster in row.graphemes(true) {
        if cluster == "\t" {
            let cells = composer_tab(used, cols);
            out.extend(std::iter::repeat_n(' ', usize::from(cells)));
            used = used.saturating_add(cells);
        } else {
            out.push_str(cluster);
            used = used.saturating_add(cluster_width(cluster));
        }
    }
    Cow::Owned(out)
}

/// Which composer row a byte offset is on, and the cells to the left of it
/// there: [`cursor_point`]'s rule -- an offset on a soft-wrap boundary belongs
/// to the following row (`pendingResolvesAt`, `visual_layout.zig:415-417`) --
/// measured with the composer's tab stops ([`composer_cells`]).
pub(crate) fn composer_point(text: &str, rows: &[Row], cursor: usize, cols: u16) -> (usize, u16) {
    for (index, row) in rows.iter().enumerate() {
        let last = index + 1 == rows.len();
        if cursor >= row.end && !last {
            continue;
        }
        if cursor >= row.end {
            return (index, row.width);
        }
        return (index, composer_cells(text, row, cursor, cols));
    }
    (0, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts<'a>(text: &'a str, rows: &[Row]) -> Vec<&'a str> {
        rows.iter().map(|row| &text[row.start..row.end]).collect()
    }

    // -----------------------------------------------------------------------
    // the composer's layout: upstream's own cases (`visual_layout.zig`), whose
    // `cols` counts the two-cell gutter this module's does not
    // -----------------------------------------------------------------------

    #[test]
    fn the_composer_wraps_whole_words_that_fit_the_next_row() {
        // `:1248-1269`, at 12 columns: ten for the text.
        let text = "hello brave world";
        let rows = composer(text, 10, &[]);
        assert_eq!(texts(text, &rows), ["hello ", "brave ", "world"]);
        assert_eq!(composer_point(text, &rows, "hello ".len(), 10), (1, 0));
        // `:1306-1316`: the space a word wraps after stays on its row.
        let text = "aaaa bbbbbb cc";
        assert_eq!(
            texts(text, &composer(text, 10, &[])),
            ["aaaa ", "bbbbbb cc"]
        );
    }

    #[test]
    fn the_composer_breaks_a_word_wider_than_a_row_where_it_starts() {
        // `:1271-1281`: an 11-cell word after `ab ` fills the first row
        // rather than leaving it nearly empty -- where `wrap` moves it down.
        let text = "ab cdefghijklm";
        let rows = composer(text, 10, &[]);
        assert_eq!(texts(text, &rows), ["ab cdefghi", "jklm"]);
        assert_eq!(rows[0].width, 10);
        assert_eq!(texts(text, &wrap(text, 10)), ["ab ", "cdefghijkl", "m"]);
    }

    #[test]
    fn the_composer_hangs_the_spaces_at_its_margin() {
        // `:1283-1304`.
        for text in ["abcdefghij kl", "abcdefghij   kl"] {
            let rows = composer(text, 10, &[]);
            assert_eq!(texts(text, &rows)[1], "kl", "{text:?}");
        }
    }

    #[test]
    fn a_soft_wrap_boundary_in_the_composer_belongs_to_the_following_row() {
        // `:802-813`, and a wide glyph at the margin (`:825-828`).
        let text = "abcd";
        let rows = composer(text, 3, &[]);
        assert_eq!(texts(text, &rows), ["abc", "d"]);
        assert_eq!(composer_point(text, &rows, 3, 3), (1, 0));
        let text = "a\u{754c}";
        let rows = composer(text, 2, &[]);
        assert_eq!(texts(text, &rows), ["a", "\u{754c}"]);
        assert_eq!(composer_point(text, &rows, 1, 2), (1, 0));
    }

    #[test]
    fn a_composer_unit_is_never_broken_at_its_own_spaces() {
        // `peekRawUnit` (`:174-231`): a badge or a block is one unit. At 13
        // cells, `a [Image 1]` puts the badge on the next row whole; `wrap`
        // breaks it after `[Image`.
        let text = "aaaaa [Image 1]";
        let badge = 6..text.len();
        let rows = composer(text, 13, std::slice::from_ref(&badge));
        assert_eq!(texts(text, &rows), ["aaaaa ", "[Image 1]"]);
        assert_eq!(texts(text, &wrap(text, 13)), ["aaaaa [Image ", "1]"]);
        // A word straight after a unit is a word of its own
        // (`consumeRawUnit`, `:244-247`), and moves whole.
        let text = "[Image 1]abcdef";
        let rows = composer(text, 12, std::slice::from_ref(&(0..9)));
        assert_eq!(texts(text, &rows), ["[Image 1]", "abcdef"]);
        // And a word ends where a unit begins (`wordEndsAt`, `:299-310`):
        // `ab` is measured alone, so it stays.
        let text = "x ab[Image 1]";
        let rows = composer(text, 10, std::slice::from_ref(&(4..13)));
        assert_eq!(texts(text, &rows), ["x ab", "[Image 1]"]);
    }

    #[test]
    fn a_composer_unit_wider_than_its_row_takes_the_row() {
        // `:1083-1114`: the badge is placed, and what follows starts a row.
        let text = "[Image 1]x";
        let rows = composer(text, 8, std::slice::from_ref(&(0..9)));
        assert_eq!(texts(text, &rows), ["[Image 1]", "x"]);
        assert_eq!(composer_point(text, &rows, 9, 8), (1, 0));
    }

    #[test]
    fn a_composer_tab_advances_to_the_next_tab_stop() {
        // `:889-899`, at 10 columns: eight for the text.
        assert_eq!(composer_tab(0, 8), 6);
        assert_eq!(composer_tab(6, 8), 1);
        assert_eq!(composer_tab(7, 8), 0);
        assert_eq!(composer("\tX", 8, &[]).len(), 1);
        assert_eq!(composer("aaaaaa\t\u{754c}", 8, &[]).len(), 2);
        assert_eq!(composer("aaaaaa\tXY", 8, &[]).len(), 2);
        let rows = composer("\tX", 8, &[]);
        assert_eq!(composer_point("\tX", &rows, 1, 8), (0, 6));
        assert_eq!(composer_expand("a\tbb\tc", 78), "a     bb      c");
        // `:1005-1018`: at the margin a tab advances nothing and breaks
        // nothing.
        let rows = composer("\tX", 1, &[]);
        assert_eq!(rows.len(), 1);
        assert_eq!(composer_point("\tX", &rows, 1, 1), (0, 0));
        assert_eq!(composer_point("\tX", &rows, 2, 1), (0, 1));
    }

    #[test]
    fn a_notice_path_breaks_after_its_last_separator_that_fits() {
        // `fx-trace-40x120-copy-fail.txt` rows 15-16: fx's notice breaks the
        // saved path after `fxtmp/` rather than moving it down whole, at 118
        // columns behind the glyph's two.
        let text = "Clipboard copy failed. Trace saved at \
                    /private/tmp/claude-501/fxcap.oJd3Kr/fxtmp/\
                    fx-trace-2026-10-06-213606-c51761bbc7ce.md. \
                    Review and redact it before sharing.";
        assert_eq!(
            texts(text, &wrap_notice(text, 118)),
            [
                "Clipboard copy failed. Trace saved at /private/tmp/claude-501/fxcap.oJd3Kr/fxtmp/",
                "fx-trace-2026-10-06-213606-c51761bbc7ce.md. Review and redact it before sharing.",
            ]
        );
        // The prose wrap moves the path down whole, and hangs the space.
        assert_eq!(
            texts(text, &wrap(text, 118))[0],
            "Clipboard copy failed. Trace saved at "
        );
        // A backslash is a separator too.
        assert_eq!(
            texts(
                "see C:\\Users\\me\\file",
                &wrap_notice("see C:\\Users\\me\\file", 14)
            ),
            ["see C:\\Users\\", "me\\file"]
        );
    }

    #[test]
    fn a_card_row_is_wrap_cuts_keep_and_the_next_begins_at_its_skip() {
        // `wrapCut` (`user_message_card.zig:57-77`), row by row: fx's
        // `card-wrap` receipt at 118 cells, `A-R2b` -- a word that fills the
        // row, then a space and a token as wide as the row: the word, an
        // empty row, the token, and the tail that starts with its blank.
        let word = "=".repeat(118);
        let token = "p".repeat(118);
        let text = format!("{word} {token} end");
        let rows = wrap_card(&text, 118);
        assert_eq!(texts(&text, &rows), [&word[..], "", &token[..], " end"]);
        assert_eq!(
            rows.iter().map(|row| row.width).collect::<Vec<_>>(),
            [118, 0, 118, 4]
        );
        // An escape in front of the cluster that does not fit stays on the
        // row before it, as upstream's prefix takes it; the row's own text is
        // what was measured.
        let text = "abcd\u{1b}[0mef";
        assert_eq!(texts(text, &wrap_card(text, 4)), ["abcd\u{1b}[0m", "ef"]);
        // A space inside a sequence is not a break: the cut is at the one
        // in front of it.
        let text = "ab \u{1b}]8;;file:///a b\u{1b}\\cdef";
        assert_eq!(
            texts(text, &wrap_card(text, 4)),
            ["ab", "\u{1b}]8;;file:///a b\u{1b}\\cdef"]
        );
        // A cluster wider than the whole row is a row of its own, and the
        // empty text one empty row.
        assert_eq!(
            texts("\u{6f22}a", &wrap_card("\u{6f22}a", 1)),
            ["\u{6f22}", "a"]
        );
        assert_eq!(
            wrap_card("", 10),
            [Row {
                start: 0,
                end: 0,
                width: 0
            }]
        );
    }

    #[test]
    fn a_notice_break_at_a_space_drops_the_spaces_from_both_rows() {
        // `scanNoticeLine` (`transcript_blocks.zig:1674-1690`): the row ends
        // before the run and the next begins after it.
        let text = "alpha beta gamma";
        let rows = wrap_notice(text, 7);
        assert_eq!(texts(text, &rows), ["alpha", "beta", "gamma"]);
        assert_eq!(
            rows.iter().map(|row| row.width).collect::<Vec<_>>(),
            [5, 4, 5]
        );
        // Upstream's own quirk, kept: each space is asked `width + 1 >
        // max_width` in turn, so of a run of two the first, which still fits,
        // stays on the row the second ends.
        assert_eq!(
            texts("alpha  beta", &wrap_notice("alpha  beta", 6)),
            ["alpha ", "beta"]
        );
        // A space that would itself cross the margin ends the row there.
        assert_eq!(
            texts("abcdef ghi", &wrap_notice("abcdef ghi", 6)),
            ["abcdef", "ghi"]
        );
    }

    #[test]
    fn a_notice_word_longer_than_a_row_breaks_where_it_crosses_and_nothing_is_lost() {
        assert_eq!(
            texts("abcdefghij", &wrap_notice("abcdefghij", 4)),
            ["abcd", "efgh", "ij"]
        );
        // A wide glyph never straddles the margin, and one wider than the row
        // is a row of its own.
        assert_eq!(
            texts("ab\u{4e2d}", &wrap_notice("ab\u{4e2d}", 3)),
            ["ab", "\u{4e2d}"]
        );
        assert_eq!(
            texts("\u{4e2d}a", &wrap_notice("\u{4e2d}a", 1)),
            ["\u{4e2d}", "a"]
        );
        // Nothing is one empty row, as for prose.
        assert_eq!(
            wrap_notice("", 10),
            [Row {
                start: 0,
                end: 0,
                width: 0
            }]
        );
        // A short text is one row, its trailing spaces kept when they fit.
        assert_eq!(texts("ok ", &wrap_notice("ok ", 10)), ["ok "]);
    }

    #[test]
    fn the_first_cluster_is_the_segmentations_first_cluster() {
        // The shortcut's whole claim, checked against the segmentation it
        // stands in for on every pair it can take: each printable ASCII
        // character followed by every ASCII byte, controls and escape
        // included, and by nothing at all.
        for first in 0x20u8..=0x7e {
            let alone = String::from(char::from(first));
            assert_eq!(first_cluster(&alone), alone.graphemes(true).next());
            for second in 0x00u8..=0x7f {
                let pair: String = [char::from(first), char::from(second)].iter().collect();
                assert_eq!(
                    first_cluster(&pair),
                    pair.graphemes(true).next(),
                    "{first:#04x} then {second:#04x}"
                );
            }
        }
        // And the cases the byte after the first is there for: a printable
        // character that a combining mark, a variation selector and keycap, a
        // joiner or a spacing mark makes part of something longer.
        for joined in [
            "e\u{301}x",
            "#\u{fe0f}\u{20e3}",
            "a\u{200d}b",
            "k\u{93f}",
            "\r\n",
            "\u{1f468}\u{200d}\u{1f469}",
            "",
        ] {
            assert_eq!(
                first_cluster(joined),
                joined.graphemes(true).next(),
                "{joined:?}"
            );
        }
    }

    #[test]
    fn a_text_presentation_selector_leaves_its_base_at_its_own_width() {
        // Upstream's table (`display_width.zig:695-697`), which the markdown
        // port carries: the painter's measure agrees with it.
        for (text, cells) in [
            ("\u{2600}\u{fe0e}", 1),
            ("\u{231a}\u{fe0e}", 2),
            ("\u{26a1}\u{fe0e}", 2),
        ] {
            assert_eq!(usize::from(width(text)), cells, "{text:?}");
            assert_eq!(
                crate::markdown::display_width::visible_width(text.as_bytes()),
                cells,
                "{text:?}"
            );
        }
    }

    #[test]
    fn the_soft_hyphen_and_the_fillers_take_the_cells_fx_counts() {
        // fx v0.0.16's count of each (`runeWidth`,
        // `src/core/shared/display_width.zig:275-282@481104fd`): none of
        // them is a zero-width continuation (`:409-421`), so each is one
        // cell, or two where `wide_ranges` holds it
        // (`unicode_display_data.zig:41`, `0x1100-0x115F`; `:85`,
        // `0x3131-0x318E`). The terminals draw them so wide or narrower
        // (tmux 3.7b: 1, 2, 0, 0, 1, 1; libghostty-vt: 1, 0, 0, 0, 0, 0),
        // never wider: a row measured by fx's count never runs past the
        // margin on either.
        let fx: &[(u32, usize)] = &[
            (0x00ad, 1),
            (0x115f, 2),
            (0x1160, 1),
            (0x3164, 2),
            (0xffa0, 1),
            (0xfff0, 1),
            (0xfff1, 1),
            (0xfff2, 1),
            (0xfff3, 1),
            (0xfff4, 1),
            (0xfff5, 1),
            (0xfff6, 1),
            (0xfff7, 1),
            (0xfff8, 1),
        ];
        // Every one that is counted otherwise, as (code point, painter, port,
        // fx): the painter's `width` alone and between two letters, and the
        // markdown port's.
        let mut wrong = Vec::new();
        for &(code, cells) in fx {
            let alone = char::from_u32(code).expect("a scalar").to_string();
            let between = format!("a{alone}b");
            let painter = (usize::from(width(&alone)), usize::from(width(&between)) - 2);
            let port = crate::markdown::display_width::visible_width(between.as_bytes()) - 2;
            if painter != (cells, cells) || port != cells {
                wrong.push((format!("U+{code:04X}"), painter, port, cells));
            }
        }
        assert_eq!(wrong, []);
        // `p5-transcript-diffwide` line 3: 104 cells of `w`, four soft
        // hyphens and `TAIL5678` -- 116 cells, so a 112-cell row ends `TAIL`.
        let line = format!("{}{}TAIL5678", "w".repeat(104), "\u{ad}".repeat(4));
        assert_eq!(width(&line), 116);
        assert_eq!(
            super::super::frame::clip(&line, 112),
            format!("{}{}TAIL", "w".repeat(104), "\u{ad}".repeat(4))
        );
    }

    #[test]
    fn a_cluster_that_holds_a_soft_hyphen_or_a_filler_takes_the_cells_fx_counts() {
        // fx counts a line one code point at a time (`displayUnitAt`,
        // `display_width.zig:234-263@481104fd`), so a filler with a mark
        // after it is the filler's cells and none for the mark. The painter
        // measures a grapheme cluster -- `U+3164`, `U+FFA0`, `U+115F` and
        // `U+1160` join the mark or the jamo after them; the soft hyphen and
        // `U+FFF0`-`U+FFF8` are clusters of their own (Grapheme_Cluster_Break
        // Control) -- so the text is what must count as the port counts it
        // ([`crate::markdown::display_width`], the oracle here).
        let counted: Vec<char> = [0x00ad, 0x115f, 0x1160, 0x3164, 0xffa0]
            .into_iter()
            .chain(0xfff0..=0xfff8)
            .map(|code| char::from_u32(code).expect("a scalar"))
            .collect();
        let mut clusters: Vec<String> = Vec::new();
        for c in &counted {
            clusters.push(c.to_string());
            for mark in ['\u{301}', '\u{fe0e}', '\u{34f}', '\u{200d}'] {
                clusters.push(format!("{c}{mark}"));
            }
        }
        // Conjoining jamo with a filler in them, one cluster each.
        for jamo in [
            "\u{115f}\u{1160}",
            "\u{1100}\u{1160}",
            "\u{115f}\u{1161}",
            "\u{1100}\u{1160}\u{11a8}",
        ] {
            clusters.push(jamo.to_string());
        }
        let mut wrong = Vec::new();
        for cluster in &clusters {
            let port = crate::markdown::display_width::visible_width(cluster.as_bytes());
            let painter = usize::from(width(cluster));
            let between = usize::from(width(&format!("a{cluster}b"))) - 2;
            if (painter, between) != (port, port) {
                let codes: Vec<String> = cluster
                    .chars()
                    .map(|c| format!("{:04X}", u32::from(c)))
                    .collect();
                wrong.push((codes.join(" "), painter, port));
            }
        }
        assert_eq!(
            wrong,
            Vec::<(String, usize, usize)>::new(),
            "(cluster, painter, fx)"
        );
        // Never fewer cells than either terminal was measured to advance
        // for the cluster between two markers (tmux 3.7b, libghostty-vt
        // 1.3.2; .prd/tui-phase5/receipts/width-disputed/clusters.txt).
        let measured: &[(&str, usize, usize)] = &[
            ("\u{ad}\u{301}", 1, 1),
            ("\u{115f}\u{34f}", 2, 0),
            ("\u{1160}\u{301}", 0, 0),
            ("\u{3164}\u{fe0e}", 0, 0),
            ("\u{ffa0}\u{301}", 1, 0),
            ("\u{ffa0}\u{200d}", 1, 0),
            ("\u{fff8}\u{fe0e}", 1, 0),
            ("\u{115f}\u{1160}", 2, 0),
            ("\u{1100}\u{1160}", 2, 2),
            ("\u{1100}\u{1160}\u{11a8}", 2, 2),
        ];
        for &(cluster, tmux, ghostty) in measured {
            let painter = usize::from(width(cluster));
            assert!(
                painter >= tmux.max(ghostty),
                "{cluster:?}: {painter} < {tmux}/{ghostty}"
            );
        }
    }

    #[test]
    fn a_filler_cluster_never_counts_fewer_cells_than_the_width_table_or_a_terminal_draws() {
        // The port counts a skin-tone modifier as none (it is no RGI sequence
        // alone), where the width table gives one two cells after anything
        // that is not its base -- and both terminals draw it so, on a cell
        // pair of its own. A filler before it must not take the cluster under
        // the table's count, as the port alone would (opus-b, round 2).
        let fillers = [0x00ad, 0x115f, 0x1160, 0x3164, 0xffa0, 0xfff0, 0xfff8];
        let marks = [
            0x0301, 0xfe0e, 0xfe0f, 0x034f, 0x200d, 0x1f3fb, 0x1f3fc, 0x1f3fd, 0x1f3fe, 0x1f3ff,
        ];
        let mut under = Vec::new();
        for filler in fillers {
            for mark in marks {
                let text: String = [filler, mark]
                    .iter()
                    .map(|&c| char::from_u32(c).expect("a scalar"))
                    .collect();
                let table = text.width();
                let painter = usize::from(width(&text));
                if painter < table {
                    under.push((format!("{filler:04X} {mark:04X}"), painter, table));
                }
            }
        }
        assert_eq!(
            under,
            Vec::<(String, usize, usize)>::new(),
            "(cluster, painter, unicode-width)"
        );
        // And never under what tmux 3.7b and libghostty-vt 1.3.2 were
        // measured to advance for it (.prd/tui-phase5/receipts/
        // width-disputed/clusters.txt).
        let measured: &[(u32, u32, usize, usize)] = &[
            (0xffa0, 0x1f3fd, 3, 2),
            (0x1160, 0x1f3fd, 2, 2),
            (0x115f, 0x1f3fd, 4, 2),
            (0x3164, 0x1f3fd, 2, 2),
            (0xfff0, 0x1f3fd, 3, 2),
            (0x00ad, 0x1f3fd, 3, 3),
            (0x115f, 0xfe0f, 2, 0),
            (0x1160, 0xfe0f, 1, 0),
            (0x3164, 0xfe0f, 1, 0),
        ];
        let mut short = Vec::new();
        for &(filler, mark, tmux, ghostty) in measured {
            let text: String = [filler, mark]
                .iter()
                .map(|&c| char::from_u32(c).expect("a scalar"))
                .collect();
            let painter = usize::from(width(&text));
            if painter < tmux.max(ghostty) {
                short.push((format!("{filler:04X} {mark:04X}"), painter, tmux, ghostty));
            }
        }
        assert_eq!(
            short,
            Vec::<(String, usize, usize, usize)>::new(),
            "(cluster, painter, tmux, libghostty-vt)"
        );
    }

    /// The `[` `]` and `a` `b` forms `super::drawn_wider`'s sweep and the
    /// tests here measure a scalar in.
    fn framed(character: char) -> (usize, usize) {
        (
            usize::from(width(&format!("[{character}]"))) - 2,
            usize::from(width(&format!("a{character}b"))) - 2,
        )
    }

    #[test]
    fn the_scalars_a_terminal_drew_wider_take_the_cells_fx_counts() {
        // fx counts each alone (`runeWidth`,
        // `src/core/shared/display_width.zig:275-282@481104fd`; the port,
        // `crate::markdown::display_width`, the oracle here), between two
        // markers and between two letters alike: `U+0605` joins the
        // character after it and `U+302E` the one before it in a cluster,
        // and still takes its own cells.
        let mut scalars = 0;
        let mut wrong = Vec::new();
        for &(first, last) in &FX_COUNTED {
            for code in first..=last {
                let character = char::from_u32(code).expect("a scalar");
                let fx =
                    crate::markdown::display_width::visible_width(character.to_string().as_bytes());
                let painter = framed(character);
                if painter != (fx, fx) {
                    wrong.push((format!("U+{code:04X}"), painter, fx));
                }
                scalars += 1;
            }
        }
        assert_eq!(scalars, 3838);
        assert!(FX_COUNTED.windows(2).all(|pair| pair[0].1 + 1 < pair[1].0));
        assert_eq!(
            wrong.len(),
            0,
            "(scalar, painter, fx): {:?}",
            &wrong[..wrong.len().min(12)]
        );
        // Some, by name: fx's count, and what tmux 3.7b and libghostty-vt
        // 1.3.2 advance for each between two markers (the sweep's
        // `tmux.txt` and `ghostty.txt` in the receipt): never more.
        for (code, fx, tmux, ghostty) in [
            (0x0605, 1, 0, 1),
            (0x09be, 1, 0, 1),
            (0x302e, 2, 2, 1),
            (0xff9e, 1, 1, 1),
            (0x1d165, 1, 0, 1),
            (0x1f1e6, 2, 1, 2),
            (0xe0000, 1, 1, 0),
            (0xe0fff, 1, 1, 0),
        ] {
            let character = char::from_u32(code).expect("a scalar");
            assert_eq!(framed(character), (fx, fx), "U+{code:04X}");
            assert!(fx >= tmux && fx >= ghostty, "U+{code:04X}");
        }
    }

    #[test]
    fn a_cluster_with_one_of_them_takes_the_cells_fx_counts_and_a_flag_stays_two() {
        // fx's units (`displayUnitAt`, `display_width.zig:234-263@481104fd`):
        // an RGI sequence whole, two cells; else a code point at a time.
        for (text, cells) in [
            // Bengali `কা`: `U+0995` and its vowel sign, one cluster the
            // width table counts as one cell.
            ("\u{995}\u{9be}", 2),
            ("a\u{302e}", 3),
            ("\u{ff76}\u{ff9e}", 2),
            ("\u{605}\u{661}", 2),
            ("\u{1f1e6}", 2),
            // A flag is one RGI sequence: two cells, as before.
            ("\u{1f1fa}\u{1f1f8}", 2),
            // A pair that is no flag is two units of two.
            ("\u{1f1e6}\u{1f1e6}", 4),
        ] {
            assert_eq!(usize::from(width(text)), cells, "{text:?}");
            assert_eq!(
                crate::markdown::display_width::visible_width(text.as_bytes()),
                cells,
                "{text:?}"
            );
        }
        // Every pair of regional indicators: a flag (an RGI flag sequence,
        // upstream's trie) stays two cells, the width table's count of every
        // pair; any other pair is fx's four.
        let mut flags = 0;
        let mut wrong = Vec::new();
        for first in 0x1f1e6..=0x1f1ffu32 {
            for second in 0x1f1e6..=0x1f1ffu32 {
                let pair: String = [first, second]
                    .iter()
                    .map(|&c| char::from_u32(c).expect("a scalar"))
                    .collect();
                let flag = crate::markdown::display_width::rgi_sequence_len(pair.as_bytes(), 0)
                    == pair.len();
                flags += usize::from(flag);
                let cells = if flag { 2 } else { 4 };
                if usize::from(width(&pair)) != cells || pair.width() != 2 {
                    wrong.push((format!("{first:X} {second:X}"), width(&pair), cells));
                }
            }
        }
        // Emoji 17.0's 259 RGI flag sequences (`emoji-sequences.txt`, the
        // version `unicode_display_data.zig@481104fd` was generated from).
        assert_eq!(flags, 259);
        assert_eq!(wrong, Vec::<(String, u16, usize)>::new());
    }

    #[test]
    fn a_scalar_counted_two_cells_is_cut_placed_and_measured_at_two_by_every_consumer() {
        // The four `FX_COUNTED` scalars the count took from none to two cells
        // (East Asian Wide marks: U+302E-302F, U+16FF0-16FF1) and a regional
        // indicator, which went from one to two, through the painter's cut
        // (`frame::clip`), the composer's caret (`composer_cells`) and the
        // status line's measure (`picker::cells`): each agrees with fx's count
        // (the port, `crate::markdown::display_width`) wherever the scalar
        // stands -- alone, joined to a letter before it, doubled, after a
        // space -- and no cut keeps more cells than it was given or stops
        // short of a cluster that would have fit.
        let port = |text: &str| crate::markdown::display_width::visible_width(text.as_bytes());
        for c in [
            '\u{302e}',
            '\u{302f}',
            '\u{16ff0}',
            '\u{16ff1}',
            '\u{1f1e6}',
        ] {
            for text in [
                c.to_string(),
                format!("ab{c}cd"),
                format!("{c}{c}x"),
                format!("x {c}"),
                format!("{}{c}", "a".repeat(37)),
            ] {
                let cells = port(&text);
                assert_eq!(usize::from(width(&text)), cells, "{text:?}");
                assert_eq!(super::super::picker::cells(&text), cells, "{text:?}");
                for cols in 0..=u16::try_from(cells + 1).expect("a short text") {
                    let cut = super::super::frame::clip(&text, cols);
                    assert!(
                        port(cut) <= usize::from(cols),
                        "{text:?} at {cols}: {cut:?}"
                    );
                    if let Some(next) = first_cluster(&text[cut.len()..]) {
                        assert!(
                            port(cut) + port(next) > usize::from(cols),
                            "{text:?} at {cols}: {cut:?} stops short of {next:?}"
                        );
                    }
                }
                // The caret after every cluster of a draft one row holds, at
                // 40 columns of text and at the row's last cell.
                for cols in [40u16, u16::try_from(cells).expect("a short text").max(2)] {
                    let rows = composer(&text, cols, &[]);
                    for row in &rows {
                        let mut at = row.start;
                        while at < row.end {
                            let cluster = first_cluster(&text[at..]).expect("a cluster");
                            at += cluster.len();
                            assert_eq!(
                                usize::from(composer_cells(&text, row, at, cols)),
                                port(&text[row.start..at]),
                                "{text:?} at {cols}, caret at {at}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn an_odd_run_of_regional_indicators_counts_no_fewer_cells_than_fx() {
        // fx pairs a run along the line, an RGI flag whole where its trie
        // holds one (`displayUnitAt`, `display_width.zig:246-247@481104fd`);
        // the painter counts a grapheme cluster at a time, and the clusters
        // pair from the run's start (UAX #29, GB12-13). Z U S, where U S is a
        // flag and Z U none: fx Z 2 and U S 2, four; xfx [Z U] 4 and [S] 2,
        // six (`scripts/fxcap/parity-ledger.toml`, `ri-run-pairing`;
        // `p5-ctrlo-wider-ri`). Never fewer: every run of three, between
        // letters.
        let port = |text: &str| crate::markdown::display_width::visible_width(text.as_bytes());
        let zus = "\u{1f1ff}\u{1f1fa}\u{1f1f8}";
        assert_eq!((usize::from(width(zus)), port(zus)), (6, 4));
        let indicator = |n: u32| char::from_u32(0x1f1e6 + n).expect("an indicator");
        let mut differ = 0;
        for a in 0..26 {
            for b in 0..26 {
                for c in 0..26 {
                    let text = format!("x{}{}{}y", indicator(a), indicator(b), indicator(c));
                    let (painter, fx) = (usize::from(width(&text)), port(&text));
                    assert!(painter >= fx, "{text:?}: {painter} < {fx}");
                    differ += usize::from(painter != fx);
                }
            }
        }
        assert!(differ > 0);
    }

    #[test]
    fn no_other_scalar_is_counted_otherwise() {
        // Every scalar the drawn-wider sweep measures that is neither one of
        // `FX_COUNTED` nor one of the fourteen `counted_as_upstream` names
        // keeps the count it had: the width table's, cluster by cluster, a
        // text-presentation selector leaving its base at its own width.
        fn table(text: &str) -> usize {
            text.graphemes(true)
                .map(|cluster| {
                    let mut chars = cluster.chars();
                    if cluster.chars().all(char::is_control) {
                        0
                    } else if let (Some(base), Some('\u{fe0e}'), None) =
                        (chars.next(), chars.next(), chars.next())
                    {
                        unicode_width::UnicodeWidthChar::width(base).unwrap_or(0)
                    } else {
                        cluster.width()
                    }
                })
                .sum()
        }
        let mut measured = 0;
        let mut moved = Vec::new();
        for code in 0..0x11_0000u32 {
            if code < 0x20 || (0x7f..=0x9f).contains(&code) {
                continue;
            }
            let Some(character) = char::from_u32(code) else {
                continue;
            };
            measured += 1;
            if fx_counted(character) || counted_as_upstream(character) {
                continue;
            }
            for text in [format!("[{character}]"), format!("a{character}b")] {
                if usize::from(width(&text)) != table(&text) {
                    moved.push(format!("U+{code:04X}"));
                }
            }
        }
        assert_eq!(measured, 1_111_999);
        assert_eq!(moved, Vec::<String>::new());
    }

    #[test]
    fn a_word_that_does_not_fit_the_rest_of_a_row_wraps_whole() {
        // visual_layout.zig:278-282
        let text = "alpha bravo";
        assert_eq!(texts(text, &wrap(text, 8)), vec!["alpha ", "bravo"]);
    }

    #[test]
    fn spaces_hang_past_the_right_margin_instead_of_wrapping() {
        // visual_layout.zig:146-148: the painter clips them, so a continuation
        // row starts at the word rather than at a column of blanks.
        let text = "alpha     bravo";
        let rows = wrap(text, 6);
        assert_eq!(texts(text, &rows), vec!["alpha     ", "bravo"]);
        assert_eq!(
            rows[0].width, 10,
            "a hanging space is measured, then clipped"
        );
    }

    #[test]
    fn a_word_wider_than_a_row_splits_per_grapheme() {
        let text = "abcdefghij";
        assert_eq!(texts(text, &wrap(text, 4)), vec!["abcd", "efgh", "ij"]);
    }

    #[test]
    fn a_hard_newline_always_breaks() {
        let text = "a\nb";
        assert_eq!(texts(text, &wrap(text, 80)), vec!["a\n", "b"]);
    }

    #[test]
    fn a_wide_glyph_costs_two_cells_and_never_straddles_the_margin() {
        let text = "\u{c548}\u{b155}\u{d558}\u{c138}\u{c694}";
        let rows = wrap(text, 5);
        assert_eq!(
            texts(text, &rows),
            vec!["\u{c548}\u{b155}", "\u{d558}\u{c138}", "\u{c694}"]
        );
    }

    #[test]
    fn a_cursor_at_a_soft_wrap_boundary_belongs_to_the_following_row() {
        // visual_layout.zig:432-437
        let text = "alpha bravo";
        let rows = wrap(text, 8);
        assert_eq!(cursor_point(text, &rows, 6), (1, 0));
        assert_eq!(cursor_point(text, &rows, 11), (1, 5));
    }

    #[test]
    fn a_cursor_after_a_wide_glyph_is_a_column_of_cells_rather_than_of_bytes() {
        // Why `cursor_point` takes the text. Each of these glyphs is three
        // bytes and two cells, so a column derived from `cursor - row.start`
        // -- the only column the plan's `(rows, cursor)` signature can compute
        // -- puts the caret at column 6 on a row that is 4 cells wide, three
        // columns past the character it is supposed to sit on.
        let text = "\u{c548}\u{b155}";
        let rows = wrap(text, 80);
        assert_eq!(cursor_point(text, &rows, text.len()), (0, 4));
        assert_eq!(cursor_point(text, &rows, 3), (0, 2));
    }

    #[test]
    fn a_composed_grapheme_is_one_column_however_many_bytes_it_took() {
        // A cluster is one cell's worth of caret movement whatever it cost in
        // bytes: `e` + a combining acute is three bytes and one column, and a
        // ZWJ family emoji is eighteen bytes and two. A caret stepped in bytes,
        // or in `char`s, lands somewhere the terminal drew nothing.
        let acute = "e\u{301}";
        let family = "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}";
        let text = format!("{acute}{family}!");
        let rows = wrap(&text, 80);
        assert_eq!(rows.len(), 1, "one row: {rows:?}");

        assert_eq!(cursor_point(&text, &rows, 0), (0, 0));
        assert_eq!(
            cursor_point(&text, &rows, acute.len()),
            (0, 1),
            "the accent is composed onto the letter, not a column of its own"
        );
        assert_eq!(
            cursor_point(&text, &rows, acute.len() + family.len()),
            (0, 3),
            "the family emoji is one cluster and two cells"
        );
        assert_eq!(cursor_point(&text, &rows, text.len()), (0, 4));
    }

    #[test]
    fn a_cursor_before_a_hard_newline_stays_on_the_row_that_break_ended() {
        // The break's own bytes belong to the row it ended, so an offset
        // *inside* the break is still that row -- and the offset after it is
        // the first column of the next one, which is where the caret goes the
        // instant the newline is typed.
        let text = "ab\ncd";
        let rows = wrap(text, 80);
        assert_eq!(cursor_point(text, &rows, 2), (0, 2));
        assert_eq!(cursor_point(text, &rows, 3), (1, 0));
        assert_eq!(cursor_point(text, &rows, 5), (1, 2));
    }

    #[test]
    fn a_text_that_ends_in_a_newline_has_an_empty_row_after_it() {
        // Otherwise the caret after a typed newline is reported on the row the
        // newline ended, which is the row above the one it is really on.
        let text = "line\n";
        let rows = wrap(text, 80);
        assert_eq!(texts(text, &rows), vec!["line\n", ""]);
        assert_eq!(cursor_point(text, &rows, 5), (1, 0));
    }

    #[test]
    fn an_empty_text_is_one_empty_row_rather_than_no_rows() {
        let rows = wrap("", 80);
        assert_eq!(
            rows,
            vec![Row {
                start: 0,
                end: 0,
                width: 0
            }]
        );
        assert_eq!(cursor_point("", &rows, 0), (0, 0));
    }

    #[test]
    fn the_rows_tile_the_text_with_no_gap_and_no_overlap() {
        // The property every caller rests on: a row is a *range*, so a break
        // that dropped or repeated a byte would show up as text the transcript
        // never printed or printed twice, and no single example above would
        // catch it.
        for text in [
            "",
            "a",
            "alpha bravo charlie delta",
            "alpha     bravo",
            "abcdefghij",
            "a\nb\n",
            "\u{c548}\u{b155}\u{d558}\u{c138}\u{c694} ok",
            "   leading",
            "trailing   ",
        ] {
            for cols in [1u16, 2, 4, 6, 8, 80] {
                let rows = wrap(text, cols);
                assert!(!rows.is_empty(), "{text:?} at {cols} produced no rows");
                assert_eq!(rows[0].start, 0, "{text:?} at {cols}");
                assert_eq!(
                    rows.last().expect("a row").end,
                    text.len(),
                    "{text:?} at {cols} lost the end of the text"
                );
                for pair in rows.windows(2) {
                    assert_eq!(
                        pair[0].end, pair[1].start,
                        "{text:?} at {cols} has a gap or an overlap between rows"
                    );
                    assert!(
                        pair[0].start < pair[0].end,
                        "{text:?} at {cols} produced an empty row in the middle"
                    );
                }
            }
        }
    }

    #[test]
    fn a_glyph_wider_than_the_whole_row_takes_the_row_rather_than_looping() {
        // Two cells will not fit one column, and a break that fired anyway
        // would push an empty row and meet the same glyph again.
        let text = "\u{c548}\u{b155}";
        let rows = wrap(text, 1);
        assert_eq!(texts(text, &rows), vec!["\u{c548}", "\u{b155}"]);
        assert_eq!(rows[0].width, 2, "the glyph is measured, then clipped");
    }

    #[test]
    fn control_characters_cost_no_cells() {
        // They are obeyed by the terminal rather than painted, and
        // `unicode_width` gives a lone control character a column of its own.
        assert_eq!(width("a\nb"), 2);
        assert_eq!(width("\r\n"), 0);
        assert_eq!(width(""), 0);
        assert_eq!(width("\u{c548}"), 2);
    }

    #[test]
    fn a_tab_measures_the_cells_the_painter_writes_for_it() {
        // Item 16: a paste is the one way a tab reaches a draft, and Phase 1
        // measured it at nothing and painted nothing -- which is consistent
        // until you ask where the caret goes, because the *text* is still there
        // and every offset after it is a column short. One number, and the
        // painter's expansion is the same one.
        // The number is spelled out as well as read from the constant, for the
        // reason `history`'s cap is: a test that took the width from the thing
        // enforcing it would pass for any width, including one that puts a
        // tenth of an eighty-column row under one character.
        assert_eq!(TAB_WIDTH, 4);
        assert_eq!(width("\t"), TAB_WIDTH);
        assert_eq!(width("a\tb"), TAB_WIDTH + 2);
        let painted = format!("a{}b", " ".repeat(usize::from(TAB_WIDTH)));
        assert_eq!(expand_tabs("a\tb"), painted);
        assert_eq!(
            width(&painted),
            width("a\tb"),
            "the painted row is a different number of cells from the measured one"
        );
        // Borrowed when there is nothing to expand, which is every row of
        // nearly every session.
        assert!(matches!(
            expand_tabs("plain"),
            std::borrow::Cow::Borrowed(_)
        ));
        // And the cells are cells the break is taken on: four of them do not
        // fit beside `ab` in a four-column row, so the tab takes a row of its
        // own -- which a control measured at nothing never could.
        let text = "ab\tcd";
        let rows = wrap(text, 4);
        assert_eq!(texts(text, &rows), vec!["ab", "\t", "cd"]);
    }

    #[test]
    fn an_escape_sequence_is_free_whole_rather_than_only_its_escape_byte() {
        // What Task 13 changed, and why. Counting only the `ESC` as a control
        // left `[31m` measured as four printing characters, so a row with two
        // colours in it lost eight columns of text to instructions that paint
        // nothing -- and `frame::clip`, which counted the `ESC` as a column of
        // its own, then cut the row somewhere else again. One sequence, one
        // answer, no cells.
        assert_eq!(width("\u{1b}[31m"), 0);
        assert_eq!(width("\u{1b}[1;38;5;200mred\u{1b}[0m"), 3);
        // **Every** sequence, not only the ones a row may keep. Measuring a
        // rejected one as text is how a narrow row comes to break inside it,
        // and a removal that only knows how to take a whole sequence out then
        // leaves the printable half of one on the screen.
        assert_eq!(width("\u{1b}[2J"), 0, "the erase was measured as text");
        assert_eq!(width("a\u{1b}[?1049hb"), 2);
        assert_eq!(width("a\u{1b}]0;title\u{7}b"), 2);
        // A sequence that has not finished is still not text: the bytes that
        // have arrived travel together and paint nothing.
        assert_eq!(width("ab\u{1b}[3"), 2);
    }

    #[test]
    fn a_colour_never_takes_the_break_that_belongs_to_the_text_around_it() {
        // A row is broken where the *text* crosses the margin. A sequence that
        // took a break point with it would put the colour on one row and the
        // word it colours on the next, and -- because a break is where a row's
        // bytes end -- could cut the sequence itself in half.
        let text = "alpha \u{1b}[31mbravo";
        assert_eq!(
            texts(text, &wrap(text, 8)),
            vec!["alpha ", "\u{1b}[31mbravo"]
        );
        let split = "abcd\u{1b}[31mefgh";
        assert_eq!(
            texts(split, &wrap(split, 4)),
            vec!["abcd", "\u{1b}[31mefgh"],
            "a row ended inside an escape sequence"
        );
    }

    // --- P3-WRAP: `width`'s ASCII fast path, held to the general path's own
    // answers -------------------------------------------------------------
    //
    // `wrap.rs:102-106`'s `width` re-segments into graphemes and re-measures a
    // cluster at a time for every call, and `grid.rs:210-214` /
    // `check.rs:1689-1694` already hand it a single already-segmented ASCII
    // cluster most of the time -- so the printable-ASCII case pays a
    // segmentation and a `unicode_width` lookup for an answer that is always
    // its own byte length. Every literal below was read from the *actual*
    // current behaviour (`cargo test --lib tui::wrap::tests -- --nocapture` on
    // a throwaway probe, not invented), so a fast path that changes any one of
    // them is a regression, not an optimization.

    #[test]
    fn ascii_fast_path_matches_the_general_path_on_printable_bytes() {
        // Printable ASCII only (0x20..=0x7e): the fast path's whole claim is
        // that this is always the byte length, empty string included.
        assert_eq!(width(""), 0);
        assert_eq!(width("a"), 1);
        assert_eq!(width(" "), 1);
        assert_eq!(width("~"), 1);
        assert_eq!(width("hello, world!"), 13);
        // The overflow boundary `width` already saturates at: 65535 bytes of
        // `a` fits `u16` exactly, 65536 does not and saturates to `u16::MAX`.
        assert_eq!(width(&"a".repeat(65535)), 65535);
        assert_eq!(width(&"a".repeat(65536)), u16::MAX);
        assert_eq!(width(&"a".repeat(70000)), u16::MAX);
    }

    #[test]
    fn ascii_fast_path_never_claims_a_control_escape_or_unicode_byte() {
        // Anything outside 0x20..=0x7e must fall through to the general path
        // untouched -- these are the actual answers that path already gives,
        // not new behaviour the fast path is allowed to introduce.
        assert_eq!(width("\u{7f}"), 0, "DEL is a control character");
        assert_eq!(width("\r"), 0, "a bare CR is a control character here");
        assert_eq!(width("\r\n"), 0, "CRLF is one line-break cluster");
        assert_eq!(width("\n"), 0);
        assert_eq!(width("\t"), TAB_WIDTH);
        assert_eq!(width("\u{1b}[31m"), 0, "an escape sequence paints nothing");
        assert_eq!(
            width("\u{c548}\u{b155}\u{d558}\u{c138}\u{c694}"),
            10,
            "five wide Hangul glyphs, two cells each"
        );
        let acute = "e\u{301}";
        assert_eq!(width(acute), 1, "a combining mark composes onto its base");
        let family = "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}";
        assert_eq!(width(family), 2, "one ZWJ cluster, two cells");
        // A control byte is not ASCII text even when every other byte around
        // it is: the whole string must fall through, not just the one byte.
        assert_eq!(width("a\nb"), 2);
        assert_eq!(width("ab\u{1b}[3"), 2, "an unfinished escape sequence");
    }
}
