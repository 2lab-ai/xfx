//! xfx's own sentences about the session, in the shape upstream gives its own.
//!
//! Upstream says everything that is not an answer, a card or a tool as a
//! **semantic notice**: a tone, a topic and a body (`types.zig:39-42`,
//! `c1db919`), rendered as `<glyph> <topic>: <body>` with the glyph keyed by the
//! tone (`types.zig:49-58`) and an empty topic dropping the `topic:` label
//! (`transcript_blocks.zig:1765-1771`). `/version` is `* version: 0.0.13`, a
//! command nobody has is `✗ command: Unknown command. Try /help.`
//! (`app_commands.zig:2153-2169`).
//!
//! This module is the **value**. Where it is written -- one blank row above
//! and below, the glyph as the first row's gutter and two columns in front of
//! every row after it -- is `super::shell`'s (`write_notice`) and
//! `super::transcript`'s ([`super::transcript::Gutter::Head`]), and what colour
//! each part is painted in is [`Tone::label`]'s. The value carries no colour and
//! no layout, so the same notice reads the same at any width and in either
//! palette, and a test can say what a notice *is* without saying how wide the
//! screen was.

use super::theme::Palette;

/// What kind of thing a notice says, which is what decides its glyph and the
/// colour of its label (`types.zig:49-58`, `app_render_runtime.zig:530-534`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tone {
    /// A fact: `/version`, a model in force, a setting that changed.
    Neutral,
    /// Something worth reading that is not a result: upstream's bold label.
    #[cfg_attr(not(test), allow(dead_code))]
    Information,
    /// Something the user asked for that happened.
    Success,
    /// Something that happened with a caveat the user should know about.
    Warning,
    /// Something that did not happen.
    Error,
    /// Something the user stopped.
    Cancelled,
}

impl Tone {
    /// The status glyph that leads the notice (`types.zig:49-58`).
    ///
    /// Upstream's choice, and its reason is upstream's: tool activity owns `●`,
    /// so a notice uses a git-style status mark that can never be read as one,
    /// and the neutral `*` avoids the middot the footer already separates
    /// with. Every one of them is a single cell, which is what lets the glyph
    /// and the space after it be the line's two-column gutter.
    pub(crate) fn glyph(self) -> &'static str {
        match self {
            Tone::Neutral => "*",
            Tone::Information => "i",
            Tone::Success => "\u{2713}",
            Tone::Warning => "!",
            Tone::Error => "\u{2717}",
            Tone::Cancelled => "\u{2298}",
        }
    }

    /// What the glyph and the topic are painted in (`noticeLabelStyle`,
    /// `transcript_blocks.zig:1707-1716`, wired at
    /// `app_render_runtime.zig:530-534`).
    ///
    /// The body is always the notice grey ([`Palette::notice`]); only the label
    /// says the tone. A neutral notice's label is the body's own grey, so
    /// `* version: 0.0.13` is one run of one colour, which is what the capture
    /// shows (`ESC[38;5;250m* version: 0.0.13`).
    pub(crate) fn label(self, palette: Palette) -> &'static str {
        match self {
            Tone::Neutral => palette.notice(),
            Tone::Information => palette.notice_heading(),
            Tone::Success | Tone::Warning | Tone::Error => palette.notice_label(),
            Tone::Cancelled => palette.dim(),
        }
    }
}

/// One notice: a tone, a topic, and what it says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Notice {
    pub(crate) tone: Tone,
    /// What the notice is about, without the colon. Empty for a notice that is
    /// only its body (`✗ usage: /rename <title>` is an error with no topic).
    pub(crate) topic: String,
    /// What it says. A line break in it is a row of its own, written two
    /// columns in from the glyph.
    pub(crate) body: String,
}

impl Notice {
    pub(crate) fn new(tone: Tone, topic: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            tone,
            topic: topic.into(),
            body: body.into(),
        }
    }

    pub(crate) fn neutral(topic: impl Into<String>, body: impl Into<String>) -> Self {
        Self::new(Tone::Neutral, topic, body)
    }

    pub(crate) fn warning(topic: impl Into<String>, body: impl Into<String>) -> Self {
        Self::new(Tone::Warning, topic, body)
    }

    pub(crate) fn error(topic: impl Into<String>, body: impl Into<String>) -> Self {
        Self::new(Tone::Error, topic, body)
    }

    pub(crate) fn cancelled(topic: impl Into<String>, body: impl Into<String>) -> Self {
        Self::new(Tone::Cancelled, topic, body)
    }

    /// The label after the glyph: the topic and its colon, or nothing.
    pub(crate) fn label(&self) -> String {
        if self.topic.is_empty() {
            String::new()
        } else {
            format!("{}:", self.topic)
        }
    }

    /// The body with the line breaks it ended on dropped, as upstream trims it
    /// (`transcript_blocks.zig:1773`).
    pub(crate) fn said(&self) -> &str {
        self.body.trim_end_matches(['\r', '\n'])
    }

    /// The notice as one string with no colour, in the shape the document
    /// shows it before it is wrapped: what a log line or a test reads.
    #[cfg(test)]
    pub(crate) fn text(&self) -> String {
        let mut lines = self.said().split('\n');
        let mut text = format!("{} {}", self.tone.glyph(), self.label());
        let first = lines.next().unwrap_or_default();
        if !self.topic.is_empty() && !first.is_empty() {
            text.push(' ');
        }
        text.push_str(first);
        for line in lines {
            text.push_str("\n  ");
            text.push_str(line);
        }
        text
    }
}

/// The line-oriented shell's attribution, taken off a sentence that is about to
/// get a glyph and a topic instead.
///
/// The runtime's sentences are shared with `xfx ask` and the line shell, where
/// nothing but the words says they are xfx's own -- so they begin `xfx: `. On
/// this surface the glyph says that, and a notice reading `✗ xfx: …` would be
/// saying it twice.
pub(crate) fn unattributed(text: &str) -> &str {
    text.strip_prefix("xfx: ").unwrap_or(text)
}

#[cfg(test)]
mod tests {
    use super::super::theme::Mode;
    use super::*;

    #[test]
    fn every_tone_leads_with_upstreams_glyph() {
        // `types.zig:49-58`, literal.
        for (tone, glyph) in [
            (Tone::Neutral, "*"),
            (Tone::Information, "i"),
            (Tone::Success, "\u{2713}"),
            (Tone::Warning, "!"),
            (Tone::Error, "\u{2717}"),
            (Tone::Cancelled, "\u{2298}"),
        ] {
            assert_eq!(tone.glyph(), glyph, "{tone:?}");
            // The glyph and its space are the line's two-column gutter, which
            // is only true of a glyph one cell wide.
            assert_eq!(super::super::wrap::width(glyph), 1, "{tone:?}");
        }
    }

    #[test]
    fn the_shape_is_glyph_topic_colon_body_and_an_empty_topic_drops_the_label() {
        assert_eq!(
            Notice::neutral("version", "0.1.0").text(),
            "* version: 0.1.0"
        );
        assert_eq!(
            Notice::error("command", "Unknown command. Try /help.").text(),
            "\u{2717} command: Unknown command. Try /help."
        );
        // `fx-basic-40x120-rename-empty.txt`: `✗ usage: /rename <title>`.
        assert_eq!(
            Notice::error("", "usage: /rename <title>").text(),
            "\u{2717} usage: /rename <title>"
        );
        assert_eq!(
            Notice::neutral("status", "model=a\nhistory_turns=0\n").text(),
            "* status: model=a\n  history_turns=0"
        );
    }

    #[test]
    fn each_label_is_painted_in_upstreams_style_for_its_tone_in_both_palettes() {
        // `app_render_runtime.zig:530-534` against `theme.zig:82-97` (dark) and
        // `:125-140` (light): information is `system_notice_label_style`, the
        // three outcomes are `red/green/warning_style`, cancelled is
        // `dim_style`, and neutral is `system_notice_text_style`.
        let dark = Palette { mode: Mode::Dark };
        let light = Palette { mode: Mode::Light };
        for (tone, on_dark, on_light) in [
            (Tone::Neutral, "\u{1b}[38;5;250m", "\u{1b}[38;5;241m"),
            (
                Tone::Information,
                "\u{1b}[1;38;5;252m",
                "\u{1b}[1;38;5;238m",
            ),
            (Tone::Success, "\u{1b}[38;5;252m", "\u{1b}[38;5;238m"),
            (Tone::Warning, "\u{1b}[38;5;252m", "\u{1b}[38;5;238m"),
            (Tone::Error, "\u{1b}[38;5;252m", "\u{1b}[38;5;238m"),
            (Tone::Cancelled, "\u{1b}[38;5;245m", "\u{1b}[38;5;247m"),
        ] {
            assert_eq!(tone.label(dark), on_dark, "{tone:?} on dark");
            assert_eq!(tone.label(light), on_light, "{tone:?} on light");
        }
    }

    #[test]
    fn the_attribution_the_line_shell_needs_is_taken_off_and_nothing_else_is() {
        assert_eq!(unattributed("xfx: a reason"), "a reason");
        assert_eq!(unattributed("a reason"), "a reason");
        assert_eq!(unattributed("xfx:x"), "xfx:x");
        assert_eq!(unattributed("not xfx: here"), "not xfx: here");
    }
}
