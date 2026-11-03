//! Escape sequences, glyphs, and limits the presenter writes (`ansi.zig`).
//!
//! Upstream keeps the themed slots -- the inline code and completed-task
//! colours -- in mutable module globals that `setInlineCodeTheme` and
//! `applyTheme` overwrite (`ansi.zig:13-31`), and reads the link colour from a
//! second global, the active theme (`inline_render.zig:546`). Both live in one
//! owned [`Styles`] value here, so two presenters can paint in two palettes
//! and a test cannot leak its palette into another running beside it. The two
//! halves stay separately settable because upstream sets them separately:
//! switching the inline code palette does not touch the link colour.

use std::borrow::Cow;

use super::theme::{self, Theme};

pub const BOLD_OPEN: &str = "\x1b[1m";
pub const BOLD_CLOSE: &str = "\x1b[22m";
pub const ITALIC_OPEN: &str = "\x1b[3m";
pub const ITALIC_CLOSE: &str = "\x1b[23m";
pub const DIM_OPEN: &str = "\x1b[2m";
pub const DIM_CLOSE: &str = "\x1b[22m";
pub const UNDERLINE_OPEN: &str = "\x1b[4m";
pub const UNDERLINE_CLOSE: &str = "\x1b[24m";
pub const STRIKE_OPEN: &str = "\x1b[9m";
pub const STRIKE_CLOSE: &str = "\x1b[29m";

/// Keeps table intersections aligned with row separators.
pub const TABLE_COLUMN_SEP: &str = " \u{2502} ";
pub const TABLE_HORIZ: &str = "\u{2500}";
pub const TABLE_JUNCTION: &str = "\u{2500}\u{253c}\u{2500}";
pub const VERTICAL_RULE_PREFIX: &str = "\u{2502} ";
pub const BULLET_MARKER: &str = "\u{2022} ";
pub const TASK_PENDING_MARKER: &str = "\u{2610}";
pub const TASK_COMPLETED_MARKER: &str = "\u{2713}";

pub const MAX_PIPE_BUFFER_BYTES: usize = 32 * 1024;
pub const HORIZONTAL_RULE_WIDTH: usize = 60;
/// OSC 8 links longer than this fall back to literal rendering.
pub const MAX_LINK_URL_BYTES: usize = 2083;

/// The themed colours the presenter paints with.
///
/// [`Styles::default`] is upstream's start-up state: every slot from
/// [`theme::FX_DARK`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Styles {
    inline_code_open: Cow<'static, str>,
    inline_code_close: &'static str,
    task_completed_open: Cow<'static, str>,
    task_completed_close: &'static str,
    /// What upstream reads through `shared_theme.current()`.
    active: Theme,
}

impl Default for Styles {
    fn default() -> Self {
        Self::new(theme::FX_DARK)
    }
}

impl Styles {
    /// Every slot from one theme: `applyTheme` plus `activate`.
    pub fn new(active: Theme) -> Self {
        let mut styles = Self {
            inline_code_open: Cow::Borrowed(""),
            inline_code_close: "",
            task_completed_open: Cow::Borrowed(""),
            task_completed_close: "",
            active,
        };
        let theme = styles.active.clone();
        styles.apply_theme(&theme);
        styles
    }

    /// Picks the builtin inline code palette for a background
    /// (`ansi.zig:20-22`). The link colour is not touched.
    pub fn set_inline_code_theme(&mut self, light: bool) {
        self.apply_theme(&theme::builtin(light));
    }

    /// Takes the inline code and completed-task slots from `theme`
    /// (`ansi.zig:24-31`). Themed slots may carry bold, italic, or a
    /// background, so each close resets everything its open sets.
    pub fn apply_theme(&mut self, theme: &Theme) {
        self.inline_code_open = theme.inline_code_open.clone();
        self.task_completed_open = theme.task_completed_open.clone();
        self.inline_code_close = theme::closing_for(&theme.inline_code_open);
        self.task_completed_close = theme::closing_for(&theme.task_completed_open);
    }

    /// Makes `theme` the one links are coloured from (`theme.zig:173-175`).
    pub fn activate(&mut self, theme: Theme) {
        self.active = theme;
    }

    /// The active theme.
    pub fn active(&self) -> &Theme {
        &self.active
    }

    pub(super) fn inline_code_open(&self) -> &str {
        &self.inline_code_open
    }

    pub(super) fn inline_code_close(&self) -> &str {
        self.inline_code_close
    }

    pub(super) fn task_completed_open(&self) -> &str {
        &self.task_completed_open
    }

    pub(super) fn task_completed_close(&self) -> &str {
        self.task_completed_close
    }

    pub(super) fn link_style(&self) -> &str {
        &self.active.link_style
    }
}

pub(super) fn write_dim(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(DIM_OPEN.as_bytes());
    out.extend_from_slice(bytes);
    out.extend_from_slice(DIM_CLOSE.as_bytes());
}

/// A dim rule [`HORIZONTAL_RULE_WIDTH`] cells wide (`ansi.zig:53-58`).
pub fn write_horizontal_rule(out: &mut Vec<u8>) {
    out.extend_from_slice(DIM_OPEN.as_bytes());
    for _ in 0..HORIZONTAL_RULE_WIDTH {
        out.extend_from_slice(TABLE_HORIZ.as_bytes());
    }
    out.extend_from_slice(DIM_CLOSE.as_bytes());
}
