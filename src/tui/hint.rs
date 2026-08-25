//! What the band's last row says, and what it gives up when the screen is
//! narrow.
//!
//! One row, at the bottom of the band, carrying the facts a turn is about to be
//! run *with* -- what will be asked, of which model, under which authority, and
//! what is already waiting. Upstream calls it the hint line and builds it out of
//! segments joined by [`SEPARATOR`] (`render.zig:391-460`); this is that line,
//! with the segments xfx has.
//!
//! Three decisions carry the module.
//!
//! * **The order is upstream's, and the order is a priority.** A missing
//!   credential leads, because nothing else on the row matters on a machine
//!   where every turn will refuse; then what is already queued, then the
//!   permission mode, the model, and the context meter. Left is more important
//!   than right, which is what makes "drop from the right" the right rule for a
//!   screen too narrow to hold everything -- the segments that survive are the
//!   ones the user needed most.
//! * **A row is budgeted here rather than clipped by the painter.** The painter
//!   cuts a row that overruns the screen (`super::frame::clip`) at the first
//!   cell that will not fit, which on this row would leave half a number and the
//!   *state* segments -- the model, the mode -- gone before the meter that is
//!   merely nice to have. So the segments are dropped whole, from the right,
//!   and the clip is left as the last line of defence for a screen too narrow
//!   for even one of them. Upstream reaches the same end from the other side by
//!   asking whether the permission mode still fits *in front of* the model
//!   before it appends it (`leadingPermissionModeFits`, `render.zig:269-278`),
//!   which is why the mode is the one segment that can be missing from the
//!   middle of the row: among the two state segments the model is the last to
//!   go.
//! * **What is said, and what is left out, are two different lists.** A notice
//!   takes the left of the row *whole*, because a refusal is about the keystroke
//!   that just happened and everything else on the row is about a state that is
//!   still true a keystroke later. The right-hand slot is the exception in the
//!   other direction: it is the warning that a second Escape throws the draft
//!   away, and it is reserved before the left side is measured, because a
//!   warning about a destructive key that vanished on a narrow terminal would be
//!   the one row of this band that costs the user something.
//!
//! # What is not here
//!
//! Upstream's statusline carries three more items -- the session title, the
//! workspace identity and the git branch -- and all three are opt-in and off by
//! default (`settings_catalog.zig:53-55`). xfx has no settings surface to turn
//! them on with, so they are absent rather than defaulted. The reasoning-effort
//! and fast-mode markers (`render.zig:430-435`) belong to model capabilities xfx
//! does not model.

use crate::config::PermissionMode;

/// What joins two segments (`render.zig:255`, `:267`).
pub(crate) const SEPARATOR: &str = " · ";

/// What a session with no usable credential is told to do.
///
/// Upstream's is `run /login` (`render.zig:417`), which is its own in-band
/// sign-in; xfx's credentials are set up by a subcommand, and `xfx setup` with
/// no target answers with the two providers it can set up (`cli.rs:311-312`).
/// It names no provider on purpose: which one is missing is a fact this row
/// does not carry ([`Hint::missing_credential`] is a yes or a no), and a row
/// that guessed would send a Gateway user to the daemon's setup.
pub(crate) const MISSING_CREDENTIAL: &str = "run `xfx setup`";

/// How many blank cells separate the left of the row from the right-aligned
/// slot at its end, at the very least.
///
/// One, rather than a [`SEPARATOR`]: the slot is not another segment of the
/// same sentence -- it is a warning that happens to share the row -- and on the
/// screen it is the *gap* that says so.
const GAP: usize = 1;

/// Everything the row is drawn from.
///
/// A value rather than a borrow of the shell, so that "what would the hint row
/// say now" is a question with an answer a unit test can ask.
#[derive(Debug, Clone)]
pub(crate) struct Hint<'a> {
    /// Whether the configured provider has nothing to authenticate with, in
    /// which case every turn this session runs will refuse.
    pub(crate) missing_credential: bool,
    /// How many submissions are waiting behind the one in flight.
    pub(crate) queued: usize,
    /// How much authority a turn will have before it has to ask.
    pub(crate) mode: PermissionMode,
    /// The model a turn will talk to, as configured -- provider prefix and all.
    /// [`compact_model`] is what shortens it.
    pub(crate) model: &'a str,
    /// How much of the model's context window the conversation has spent, as
    /// `(used, total)` in tokens.
    ///
    /// `None` when nobody has measured it, which on every Phase-1 path is
    /// always: no [`super::bridge::UiEvent`] carries a usage number, and the
    /// Gateway publishes no context window to be the denominator either (only
    /// the llmux catalog does, `provider::model::CatalogEntry::max_context`).
    /// The row therefore says nothing about context rather than reporting a
    /// zero nobody measured -- the same rule, and the same staging, as the
    /// activity row's token count ([`super::activity::Activity::tokens`]).
    pub(crate) context_used: Option<(u64, u64)>,
    /// A refusal to show instead of the left-hand segments, if one has just
    /// happened.
    pub(crate) notice: Option<Notice<'a>>,
    /// What to put flush with the last column, if anything.
    pub(crate) right: Option<&'a str>,
}

/// A refusal, and the colours it is said in.
///
/// Three fields rather than one pre-painted string, and the third is the whole
/// reason this is a type: **a colour a caller opens in the text is a colour the
/// clip can eat.** [`super::frame::clip`] stops at the first cell that will not
/// fit and drops everything after it, escape sequences included -- so a notice
/// wider than the left side of the row loses whatever was written *behind* its
/// text, and anything [`row`] then puts to the right of it inherits the
/// refusal's colour instead of the row's. The way out is structural: the text
/// is the only part that is clippable, and the return to the row's own colour
/// is emitted by the composer, after the cut.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Notice<'a> {
    /// What it says.
    pub(crate) text: &'a str,
    /// What opens its own colour, or nothing for a caller that has no palette.
    ///
    /// In front of the text and therefore inside the clip, which is where it
    /// belongs: a colour costs no cells, and a clip that cut at zero cells
    /// would leave nothing for it to apply to anyway.
    pub(crate) style: &'a str,
    /// What puts the row's own colour back afterwards.
    pub(crate) resume: &'a str,
}

/// The short name of a model, for a row that has to say five other things
/// (`render.zig:219-244`).
///
/// The provider prefix goes, because a session talks to one provider and the
/// row already says nothing else about it; then `claude-` goes, because it is
/// on every model of the family that needs shortening; and then the family name
/// is separated from its version by a space rather than by the hyphen the id
/// spells it with. What is left of the id is kept **verbatim** -- upstream
/// concatenates the remainder (`render.zig:238`), so `claude-opus-4.7` is `opus
/// 4.7` and `claude-opus-4-7` is `opus 4-7`. A model whose id is not of that
/// shape is its own bare name, which is what makes this safe to run over any
/// string a user can configure.
pub(crate) fn compact_model(model: &str) -> String {
    // Everything after the **last** `/`, like upstream's scan (`render.zig:220-224`):
    // a provider prefix is a prefix, and an id that carries two of them is
    // named by the last one.
    let bare = model.rsplit('/').next().unwrap_or(model);
    let Some(family) = bare.strip_prefix("claude-") else {
        return bare.to_string();
    };
    for (prefix, label) in [
        ("opus-", "opus "),
        ("sonnet-", "sonnet "),
        ("haiku-", "haiku "),
    ] {
        if let Some(version) = family.strip_prefix(prefix) {
            return format!("{label}{version}");
        }
    }
    family.to_string()
}

/// The whole row, at most `cols` cells wide.
pub(crate) fn row(hint: &Hint<'_>, cols: u16) -> String {
    let budget = usize::from(cols);
    let right = hint.right.unwrap_or_default();
    let right_cells = cells(right);
    // The right-hand slot is reserved before the left is measured, so a narrow
    // screen drops a segment rather than the warning.
    let limit = if right.is_empty() {
        budget
    } else {
        budget.saturating_sub(right_cells + GAP)
    };

    let mut segments = segments(hint, limit);
    // From the right, whole segments at a time, down to one: a row with a
    // single segment too wide for the screen is the painter's problem below,
    // and an empty row would say less than a cut one.
    while segments.len() > 1 && cells(&segments.join(SEPARATOR)) > limit {
        segments.pop();
    }
    // Here is where a count becomes a terminal width, and the clamp is what
    // makes the conversion exact rather than merely infallible: `limit` is at
    // most `budget`, which came from a `u16`.
    let left_cols = u16::try_from(limit).unwrap_or(cols);
    let mut row = super::frame::clip(&segments.join(SEPARATOR), left_cols).to_string();

    // The row's own colour, back on: **after** the cut, so a notice too wide
    // for its side cannot take it away with the text the clip dropped, and
    // **before** the padding and the slot below, so nothing to the right of a
    // refusal is ever painted in the refusal's colour.
    if let Some(notice) = hint.notice {
        row.push_str(notice.resume);
    }

    if !right.is_empty() {
        // Flush with the last column. The left side was budgeted against
        // `limit`, so this is at least [`GAP`] whenever anything survived on
        // it -- and zero only when a single oversized segment was cut to the
        // screen, where the clip below is what keeps the row on it.
        let padding = budget.saturating_sub(cells(&row) + right_cells);
        row.extend(std::iter::repeat_n(' ', padding));
        row.push_str(right);
    }
    super::frame::clip(&row, cols).to_string()
}

/// The left-hand segments, in upstream's order, for a left side `limit` cells
/// wide.
fn segments(hint: &Hint<'_>, limit: usize) -> Vec<String> {
    let mut segments = Vec::with_capacity(5);
    // A refusal is the left side, whole. Nothing after this line runs, because
    // the row is now about the keystroke rather than about the session.
    if let Some(notice) = hint.notice {
        // The opening colour travels with the text; the closing one does not
        // ([`Notice::resume`], emitted by [`row`] after the cut).
        push(&mut segments, format!("{}{}", notice.style, notice.text));
        return segments;
    }
    if hint.missing_credential {
        push(&mut segments, MISSING_CREDENTIAL.to_string());
    }
    if hint.queued > 0 {
        // `queued 1` in upstream's own words (`render.zig:419-422`), and the
        // whole difference between a queue and a surprise.
        push(&mut segments, format!("queued {}", hint.queued));
    }
    let model = compact_model(hint.model);
    let mode = hint.mode.label();
    if leading_permission_mode_fits(limit, mode, &model) {
        push(&mut segments, mode.to_string());
    }
    push(&mut segments, model);
    if let Some((used, total)) = hint.context_used {
        push(&mut segments, context(used, total));
    }
    segments
}

/// Appends a segment, unless it is empty.
///
/// Upstream's `appendStatusSegment` returns immediately on an empty segment
/// (`render.zig:254`), and the reason is visible on the row: a segment that
/// contributed nothing but still took a separator would leave a `·` with
/// nothing on one side of it.
fn push(segments: &mut Vec<String>, segment: String) {
    if !segment.is_empty() {
        segments.push(segment);
    }
}

/// Whether the permission mode still fits in front of the model
/// (`render.zig:269-278`).
///
/// Asked before the mode is appended rather than after the row is too long,
/// which is what makes the mode -- not the model -- the segment that gives way
/// on a narrow screen. The model is what a user checks before pressing Return;
/// the mode is what they set once and rarely change.
fn leading_permission_mode_fits(limit: usize, mode: &str, model: &str) -> bool {
    if limit == 0 {
        return false;
    }
    cells(mode) + cells(SEPARATOR) + cells(model) <= limit
}

/// How much of the context window the conversation has spent
/// (`render.zig:441-452`).
///
/// Thousands, because the row has no room for six digits and nobody reads them:
/// what the number is for is "how close am I to the end of it", and the
/// percentage beside it is that question answered directly.
fn context(used: u64, total: u64) -> String {
    // Saturating rather than wrapping: the product is only near the edge for a
    // token count no conversation can reach, and a percentage that wrapped
    // would be a small number where a large one belongs. A window of zero is a
    // number a provider can publish (`render.zig:445` guards it too), and it
    // reads as nought per cent rather than as a division.
    let percent = used.saturating_mul(100).checked_div(total).unwrap_or(0);
    format!("Context: {}k/{}k {percent}%", used / 1000, total / 1000)
}

/// How many cells `text` occupies, by the painter's own measure.
///
/// One rule for measuring and cutting ([`super::wrap::width`] and
/// [`super::frame::clip`] share it), so a row cannot be budgeted by one and cut
/// by another. The `u16` it answers in saturates past 65535 cells, which fails
/// in the safe direction here: a segment measured as wider than it is gets
/// dropped, and the row is clipped to the screen either way.
fn cells(text: &str) -> usize {
    usize::from(super::wrap::width(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::config::PermissionMode;

    fn hint() -> Hint<'static> {
        Hint {
            missing_credential: false,
            queued: 0,
            mode: PermissionMode::Ask,
            model: "anthropic/claude-opus-4-7",
            context_used: None,
            notice: None,
            right: None,
        }
    }

    #[test]
    fn the_model_label_drops_the_provider_and_the_family_prefix() {
        // render.zig:219-244, and the two spellings of a version that reach it:
        // upstream's own case is a dot (`render.zig:950`), the plan's is a
        // hyphen, and neither is rewritten -- what follows the family name is
        // the id's own text.
        assert_eq!(compact_model("anthropic/claude-opus-4.7"), "opus 4.7");
        assert_eq!(compact_model("anthropic/claude-opus-4-7"), "opus 4-7");
        assert_eq!(compact_model("anthropic/claude-sonnet-4.5"), "sonnet 4.5");
        assert_eq!(compact_model("anthropic/claude-haiku-4.5"), "haiku 4.5");
        // A `claude-` model of no named family keeps what is left of its id,
        // and a model of no family at all is its own bare name.
        assert_eq!(compact_model("anthropic/claude-3-5-turbo"), "3-5-turbo");
        assert_eq!(compact_model("openai/gpt-5"), "gpt-5");
        assert_eq!(compact_model("zai/glm-5.2"), "glm-5.2");
        assert_eq!(compact_model("plain"), "plain");
    }

    #[test]
    fn the_segments_are_in_upstreams_order() {
        let mut hint = hint();
        hint.queued = 1;
        hint.context_used = Some((12_000, 200_000));
        let row = row(&hint, 200);
        let queued = row.find("queued 1").expect("queued");
        let mode = row.find("ask").expect("the permission mode");
        let model = row.find("opus 4-7").expect("the model");
        let context = row.find("Context:").expect("the context");
        assert!(queued < mode && mode < model && model < context, "{row:?}");
        assert!(row.contains("Context: 12k/200k 6%"), "{row:?}");
        assert_eq!(
            row, "queued 1 · ask · opus 4-7 · Context: 12k/200k 6%",
            "the segments are joined by something other than upstream's separator"
        );
    }

    #[test]
    fn a_missing_credential_leads_because_nothing_else_matters_without_one() {
        let mut hint = hint();
        hint.missing_credential = true;
        assert!(
            row(&hint, 200).starts_with("run `xfx setup"),
            "{:?}",
            row(&hint, 200)
        );
    }

    #[test]
    fn a_narrow_terminal_drops_segments_from_the_right_and_never_overflows() {
        let mut hint = hint();
        hint.context_used = Some((12_000, 200_000));
        for cols in [12u16, 20, 40, 80] {
            let row = row(&hint, cols);
            assert!(
                crate::tui::wrap::width(&row) <= cols,
                "{cols} columns overflowed: {row:?}"
            );
        }
        assert!(
            row(&hint, 20).contains("opus"),
            "the model label is the last thing to go"
        );
        // The two ends of the rule, exactly: at twenty columns the meter is
        // gone and the mode is still there; at twelve the mode has given way in
        // front of the model rather than the model behind it.
        assert_eq!(row(&hint, 20), "ask · opus 4-7");
        assert_eq!(row(&hint, 12), "opus 4-7");
    }

    #[test]
    fn a_notice_overrides_the_left_side_without_losing_the_right() {
        let mut hint = hint();
        hint.notice = Some(Notice {
            text: "one prompt is already queued; this one was not sent",
            style: "",
            resume: "",
        });
        hint.right = Some("esc again to clear");
        let row = row(&hint, 120);
        assert!(row.starts_with("one prompt is already queued"), "{row:?}");
        assert!(row.ends_with("esc again to clear"), "{row:?}");
        // Nothing of the state survives beside it, which is the override: the
        // model is on the row a keystroke later, and the refusal is not.
        assert!(!row.contains("opus"), "{row:?}");
        assert!(!row.contains("ask"), "{row:?}");
    }

    #[test]
    fn one_segment_wider_than_the_screen_is_cut_rather_than_left_to_overflow() {
        // The last line of defence, and the case the dropping cannot answer:
        // there is nothing left to drop. A row wider than the terminal with
        // autowrap off is not a wrapped row -- it is the band writing over the
        // column the terminal keeps for itself -- so the row is cut by the
        // painter's own rule (`super::frame::clip`) rather than handed over
        // long.
        let mut hint = hint();
        hint.notice = Some(Notice {
            text: "one prompt is already queued; this one was not sent",
            style: "",
            resume: "",
        });
        for cols in [1u16, 8, 20] {
            let row = row(&hint, cols);
            assert_eq!(
                crate::tui::wrap::width(&row),
                cols,
                "the only segment there was did not fill the screen it was cut to: {row:?}"
            );
        }
    }

    #[test]
    fn a_screen_narrower_than_the_warning_itself_is_still_not_overflowed() {
        // The one case the budget cannot answer by dropping something: the
        // right-hand slot is reserved *before* the left is measured, so a
        // screen too narrow for the slot itself leaves the left side nothing
        // and the slot too wide -- and what keeps the row on the terminal there
        // is the clip at the end of it.
        let mut hint = hint();
        hint.right = Some("esc again to clear");
        for cols in [1u16, 10, 17] {
            let row = row(&hint, cols);
            assert!(
                crate::tui::wrap::width(&row) <= cols,
                "{cols} columns overflowed: {row:?}"
            );
        }
    }

    #[test]
    fn a_notice_too_wide_for_its_side_still_hands_the_row_back_its_colour() {
        // The defect this shape exists to prevent: the clip stops at the first
        // cell that will not fit and drops **everything** after it, escape
        // sequences included. A closing sequence written behind the notice's
        // own text would be exactly that -- and the warning to the right of it
        // would then be painted in the refusal's colour, on a row whose reset
        // is the only thing standing between it and the user's own shell.
        //
        // Real sequences rather than markers, because the budget is the claim:
        // a colour costs no cells (`super::wrap::width`), so admitting one must
        // not move where the text is cut.
        let style = "\u{1b}[38;5;250m";
        let resume = "\u{1b}[38;5;255m";
        let mut hint = hint();
        hint.notice = Some(Notice {
            text: "one prompt is already queued; this one was not sent",
            style,
            resume,
        });
        hint.right = Some("esc again to clear");
        let row = row(&hint, 40);

        let cut = row.find(resume).expect("the row took its own colour back");
        let warning = row.find("esc again to clear").expect("the warning");
        assert!(
            cut < warning,
            "the warning is painted in the refusal's colour: {row:?}"
        );
        assert!(
            row.find(style).expect("the refusal's colour") < cut,
            "the row took a colour back that had never been opened: {row:?}"
        );
        // And the cut really happened, so this is the wide case rather than a
        // notice that happened to fit.
        assert!(
            !row.contains("was not sent"),
            "the notice fit, so this case no longer forces the cut: {row:?}"
        );
        assert_eq!(
            crate::tui::wrap::width(&row),
            40,
            "the colours moved where the text was cut: {row:?}"
        );
    }

    #[test]
    fn the_right_hand_slot_is_flush_with_the_last_column() {
        // Right-aligned rather than appended, because the row it shares is the
        // one whose left side changes length every time the queue does: a
        // warning that moved with it would be a warning the eye has to look
        // for.
        let mut hint = hint();
        hint.right = Some("esc again to clear");
        for cols in [40u16, 80, 120] {
            let row = row(&hint, cols);
            assert_eq!(
                crate::tui::wrap::width(&row),
                cols,
                "the slot is not against the last column: {row:?}"
            );
            assert!(row.ends_with("esc again to clear"), "{row:?}");
            assert!(row.starts_with("ask · opus 4-7 "), "{row:?}");
        }
    }

    #[test]
    fn the_right_hand_slot_survives_a_screen_too_narrow_for_the_segments() {
        // The reservation, from the side that proves it: the warning is about a
        // key that is already armed and that throws the draft away, so it is
        // the last thing on this row worth dropping.
        let mut hint = hint();
        hint.right = Some("esc again to clear");
        let row = row(&hint, 24);
        assert!(
            row.ends_with("esc again to clear"),
            "a narrow screen dropped the warning rather than a segment: {row:?}"
        );
        assert!(crate::tui::wrap::width(&row) <= 24, "{row:?}");
    }

    #[test]
    fn a_session_that_has_measured_no_context_says_nothing_about_it() {
        // A row that said `Context: 0k/0k 0%` would be reporting a measurement
        // nobody took: no Phase-1 event carries a usage number, so the segment
        // is absent rather than zero. The formatting above and this absence are
        // both pinned, so the day a usage event arrives this is a call site
        // rather than a feature.
        let hint = hint();
        assert!(hint.context_used.is_none());
        assert!(
            !row(&hint, 200).contains("Context"),
            "{:?}",
            row(&hint, 200)
        );
    }

    #[test]
    fn every_mode_is_said_in_the_word_every_other_renderer_says_it_in() {
        // `PermissionMode::label` is "the stable wire label used by every
        // renderer" (`config.rs:112-119`), so the band says what `xfx status`
        // says. Upstream spells `yolo` in capitals and colours `auto`
        // (`render.zig:245-251`); both are styling this palette has no role
        // for, and one grammar across the front ends is worth more here than
        // one of them shouting.
        for mode in [
            PermissionMode::Ask,
            PermissionMode::Auto,
            PermissionMode::Yolo,
        ] {
            let mut hint = hint();
            hint.mode = mode;
            assert!(
                row(&hint, 80).contains(mode.label()),
                "the {mode:?} row does not say what mode it is"
            );
        }
    }

    #[test]
    fn a_zero_context_window_is_a_percentage_of_nothing_rather_than_a_panic() {
        // The denominator is a provider's number, and `render.zig:445` guards
        // it for the same reason: a catalog that published `0` would divide by
        // it.
        let mut hint = hint();
        hint.context_used = Some((12_000, 0));
        assert!(row(&hint, 80).contains("Context: 12k/0k 0%"), "{hint:?}");
    }

    #[test]
    fn an_empty_segment_takes_no_separator_with_it() {
        // `render.zig:254`. The model is the segment that can be empty -- a
        // configured id of `anthropic/` compacts to nothing -- and a row that
        // kept its separator would show a `·` with one side missing.
        let mut hint = hint();
        hint.model = "anthropic/";
        hint.queued = 2;
        assert_eq!(row(&hint, 80), "queued 2 · ask");
    }
}
