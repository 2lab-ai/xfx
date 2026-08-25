//! Which of two palettes the band paints in, decided once at start-up.
//!
//! A band drawn at the bottom of the terminal's *normal* buffer shares the
//! screen with the user's shell, so its rows have to read against a background
//! this process did not choose and cannot see. Three things can say what that
//! background is, and they are consulted in the order of how much each one
//! knows (`theme_detection.zig:22-37`):
//!
//! 1. [`ENV`] -- the user said so. Exactly `light` or `dark`, case-insensitively
//!    (`theme_detection.zig:15-20`); anything else is not an answer and is
//!    ignored rather than guessed at.
//! 2. The terminal's own answer to an `OSC 11` background query ([`QUERY`]),
//!    read back off standard input with a [`DEADLINE`].
//! 3. [`COLORFGBG`], which some terminals export and most do not.
//!
//! and when none of them says anything, **dark** -- the assumption the terminal
//! world defaults to, and the one upstream falls back on
//! (`theme_detection.zig:36`).
//!
//! The query is not asked at all when [`ENV`] already decided, because a
//! terminal that will not answer costs the deadline and there is nothing left
//! for the answer to change. When it *is* asked it shares one read with the
//! launch's cursor report ([`super::probe`]) rather than opening a second one:
//! see [`QUERY`] for why that is exact rather than merely cheaper.
//!
//! What this module is **not** is upstream's live theme monitor. Following a
//! background that changes while xfx runs needs mode 2031 and a `DSR ?996n`
//! (`theme_monitor.zig`), and that is Phase 3; a session here paints in the
//! palette it started in until it ends.

use std::time::Duration;

/// The variable that fixes the palette without asking the terminal.
///
/// Upstream's `FX_THEME` (`theme_detection.zig:16`), under xfx's own prefix.
pub(crate) const ENV: &str = "XFX_THEME";

/// The variable some terminals export their foreground and background indices
/// in (`theme_detection.zig:31`).
pub(crate) const COLORFGBG: &str = "COLORFGBG";

/// The variable a terminal claims 24-bit colour in
/// (`theme_protocol.zig:44-48`).
pub(crate) const COLORTERM: &str = "COLORTERM";

/// The variable a terminal names itself in (`theme_protocol.zig:49-51`).
pub(crate) const TERM_PROGRAM: &str = "TERM_PROGRAM";

/// The background colour query (`OSC 11 ; ? ST`), as upstream spells it
/// (`terminal.zig:9`).
///
/// **Written immediately before the launch's `CSI 6n`, and that ordering is the
/// whole of why a terminal which does not implement this one costs nothing.**
/// A terminal answers the queries in its input stream in the order it parsed
/// them, so the cursor report is a *fence*: when it arrives without a
/// background reply in front of it, the background reply is not late, it is
/// never coming, and the read can stop. Upstream does the same thing with a
/// primary device attributes request instead
/// (`terminal.zig:10-11 theme_background_query_with_fence`,
/// `theme_monitor.zig:181-183`); here the cursor report is already being asked
/// for and already being waited on, so it is the fence and no third query is
/// written.
///
/// The cost is therefore paid only by a terminal that answers *neither*, which
/// waits [`DEADLINE`] once -- not once per query.
pub(crate) const QUERY: &str = "\u{1b}]11;?\u{1b}\\";

/// What every answer to [`QUERY`] begins with, and nothing else does.
///
/// The `OSC` number **is** the reply's identity: `11` is the question this
/// module asked, and a string that opens with anything else -- a `0` retitling
/// the window, a `10` reporting the foreground -- is a different conversation
/// that happens to share the stream. [`super::probe`] uses this to tell the two
/// apart, because the one it consumes it must consume and the other it must
/// give back untouched.
///
/// Deliberately the envelope and not the body: a terminal that answers `11`
/// with something [`parse_osc11`] cannot read has still *answered*, and the
/// bytes of a malformed answer are not keystrokes. They are consumed, the parse
/// returns `None`, and [`detect`] asks `COLORFGBG` next.
pub(crate) const REPLY_PREFIX: &str = "\u{1b}]11;";

/// Whether a complete `OSC` string is an answer to [`QUERY`].
pub(crate) fn is_background_reply(text: &str) -> bool {
    text.starts_with(REPLY_PREFIX)
}

/// How long the terminal has to answer before the launch stops waiting
/// (`theme_detection.zig:45`).
///
/// Longer than [`super::probe::DEADLINE`], and the launch waits the longer of
/// the two because the two queries share one read.
pub(crate) const DEADLINE: Duration = Duration::from_millis(200);

/// Which way round the terminal's colours are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    Dark,
    Light,
}

/// How exactly a colour can be asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Depth {
    /// The 256-colour palette, which every terminal in use has.
    Ansi256,
    /// Direct 24-bit colour, for a terminal that said it has it.
    TrueColor,
}

/// The colours the band paints its own rows in.
///
/// Three roles, because three rows carry colour in this phase: the rule that
/// separates the document from the band, the hint row at the bottom, and a
/// refusal shown on it. The composer's own rows carry none, which is upstream's
/// choice too -- `input_bar_style` is empty in both themes
/// (`render.zig:69,88`).
///
/// Every accessor answers with a whole SGR sequence rather than with a colour
/// number, so a painter concatenates and never formats, and
/// [`reset`](Self::reset) is the only way a run ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Palette {
    pub(crate) mode: Mode,
    pub(crate) depth: Depth,
}

/// What ends every run of colour this crate writes.
///
/// `CSI 0 m` rather than `CSI m`: the two mean the same thing to a terminal,
/// and the explicit parameter is the one a reader of the byte stream can be
/// sure about.
const RESET: &str = "\u{1b}[0m";

/// The greys upstream paints these three roles in, by 256-colour index
/// (`render.zig:28,29,34` dark and `render.zig:70,71,76` light).
///
/// The third is upstream's `system_notice_text_style`, which is what a refusal
/// on the hint row is: xfx's own words about something that did not happen.
const DARK: [u8; 3] = [240, 255, 250];
const LIGHT: [u8; 3] = [250, 235, 241];

/// Where in one of those triples each role sits.
const DIVIDER: usize = 0;
const HINT: usize = 1;
const NOTICE: usize = 2;

impl Palette {
    /// The rule between the document and the band.
    pub(crate) fn divider(&self) -> &'static str {
        self.paint(DIVIDER)
    }

    /// The band's last row.
    pub(crate) fn hint(&self) -> &'static str {
        self.paint(HINT)
    }

    /// A refusal shown on that row.
    pub(crate) fn notice(&self) -> &'static str {
        self.paint(NOTICE)
    }

    /// What ends a run, in either mode and at either depth.
    pub(crate) fn reset(&self) -> &'static str {
        RESET
    }

    /// One role's sequence.
    ///
    /// A table rather than a `match` per accessor, because the thing that must
    /// be true of this module is that the two modes have the *same shape* and
    /// differ only in the number -- and a table is that claim written down.
    fn paint(&self, role: usize) -> &'static str {
        let index = match self.mode {
            Mode::Dark => DARK[role],
            Mode::Light => LIGHT[role],
        };
        match self.depth {
            Depth::Ansi256 => ansi256(index),
            Depth::TrueColor => truecolor(index),
        }
    }
}

/// The 256-colour foreground sequence for one index.
///
/// Spelled out per index rather than formatted, because these are `&'static
/// str`: a painter concatenates them into a row and nothing allocates to paint
/// a frame. The set is closed on purpose -- it is exactly [`DARK`] and
/// [`LIGHT`], and the render allowlist ([`super::pacer::colour_at`]) is written
/// to the same closed shape.
fn ansi256(index: u8) -> &'static str {
    match index {
        235 => "\u{1b}[38;5;235m",
        240 => "\u{1b}[38;5;240m",
        241 => "\u{1b}[38;5;241m",
        250 => "\u{1b}[38;5;250m",
        255 => "\u{1b}[38;5;255m",
        // Unreachable from `paint`, whose only inputs are the two tables above.
        // A grey a future palette adds and forgets to spell here reads as no
        // colour at all rather than as a wrong one.
        _ => "",
    }
}

/// The same shade, said exactly, for a terminal that can take it.
///
/// All five indices are on xterm's greyscale ramp, where index `i` in
/// `232..=255` is the level `8 + (i - 232) * 10` on all three channels -- so
/// these are not approximations of the sequences above, they are the same
/// colours spelled in the notation a truecolor terminal reads without
/// consulting a palette. That is the whole of what [`Depth::TrueColor`] buys
/// here: indices 232-255 are a *palette*, and a terminal whose theme has
/// remapped them paints a band grey xfx did not choose.
///
/// Upstream reserves truecolor for the diff markers, where the fallback index
/// really is an approximation of a brand colour
/// (`render.zig:41-48`); this is the same "exact when the terminal said it can"
/// rule applied to the greys.
fn truecolor(index: u8) -> &'static str {
    match index {
        235 => "\u{1b}[38;2;38;38;38m",
        240 => "\u{1b}[38;2;88;88;88m",
        241 => "\u{1b}[38;2;98;98;98m",
        250 => "\u{1b}[38;2;188;188;188m",
        255 => "\u{1b}[38;2;238;238;238m",
        _ => "",
    }
}

/// What a whole `OSC 11` reply says the background is, when it is one
/// (`theme_protocol.zig:11-40`).
///
/// The reply carries the background as three hexadecimal components of one to
/// four digits each, and the mode is the perceived luminance of them:
/// `(299 r + 587 g + 114 b) / 1000` over half of `0xffff` is light. The
/// weights are the ITU-R BT.601 luma coefficients, which is what upstream uses
/// and what makes a saturated blue read as dark while a yellow of the same
/// arithmetic mean reads as light.
///
/// Anything that is not exactly this shape is `None` -- not a guess, and not a
/// dark. The caller's next question is `COLORFGBG`, and "the terminal did not
/// answer" has to be tellable from "the terminal said dark" for that question
/// to be asked at all.
pub(crate) fn parse_osc11(reply: &str) -> Option<Mode> {
    let body = reply.strip_prefix("\u{1b}]11;rgb:")?;
    let body = body
        .strip_suffix("\u{1b}\\")
        .or_else(|| body.strip_suffix('\u{07}'))?;
    let mut components = body.split('/');
    let mut channel = || component(components.next());
    let (red, green, blue) = (channel()?, channel()?, channel()?);
    if components.next().is_some() {
        return None;
    }
    // Each channel is at most `0xffff`, so the weighted sum is at most
    // `0xffff * 1000` and stays inside a `u32` without a widening step.
    let luminance = (red * 299 + green * 587 + blue * 114) / 1000;
    Some(if luminance > 32768 {
        Mode::Light
    } else {
        Mode::Dark
    })
}

/// One hexadecimal component of an `OSC 11` reply, scaled to sixteen bits
/// (`theme_protocol.zig:78-84`).
///
/// A terminal may answer in one, two, three or four digits per channel, and
/// `f`, `ff`, `fff` and `ffff` all mean *full*. Scaling by the width's own
/// maximum rather than by shifting is what makes that true: `f` becomes
/// `0xffff` and not `0x000f`.
fn component(part: Option<&str>) -> Option<u32> {
    let part = part?;
    if part.is_empty() || part.len() > 4 {
        return None;
    }
    let value = u32::from_str_radix(part, 16).ok()?;
    // `part.len()` is 1..=4, so the shift is 4..=16 and the maximum is at least
    // 15 -- never zero, so the division below is safe.
    let maximum = (1u32 << (part.len() * 4)) - 1;
    Some(value * 0xffff / maximum)
}

/// What `COLORFGBG` says the background is, when it says anything
/// (`theme_protocol.zig:68-76`).
///
/// The value is a **semicolon**-separated list whose last field is the
/// background index -- `rxvt` writes `fg;bg` and some terminals write
/// `fg;cursor;bg` -- and an index of 8 or more is one of the bright half of the
/// sixteen, which is a light background.
///
/// Semicolons only, because that is the whole of the separator upstream reads
/// (`theme_protocol.zig:69-73` scans for `;` and for nothing else) and there is
/// no terminal on record writing this variable any other way. A colon form
/// would be a shape invented here rather than parsed from anything real.
///
/// `None` rather than upstream's `false` for a value that is not a list of
/// numbers, because this answers a question in a chain: a garbled `COLORFGBG`
/// must not be reported as the terminal having said *dark*.
pub(crate) fn from_colorfgbg(value: &str) -> Option<Mode> {
    let (_, background) = value.rsplit_once(';')?;
    let background: u8 = background.parse().ok()?;
    Some(if background >= 8 {
        Mode::Light
    } else {
        Mode::Dark
    })
}

/// What [`ENV`] says, when it says one of the two things it may say
/// (`theme_detection.zig:15-20`).
///
/// Case-insensitive, and `None` for everything else. A variable set to
/// `Light`, `DARK` or `1` is three different situations and only the first two
/// are answers; the third is a user who meant something this program does not
/// implement, and guessing at it would be worse than asking the terminal.
fn from_env(value: &str) -> Option<Mode> {
    if value.eq_ignore_ascii_case("light") {
        Some(Mode::Light)
    } else if value.eq_ignore_ascii_case("dark") {
        Some(Mode::Dark)
    } else {
        None
    }
}

/// Whether the palette is already decided without asking the terminal.
///
/// The launch consults this *before* it writes anything, because the query it
/// would otherwise write is the one thing here that costs time.
pub(crate) fn decided(env_theme: Option<&str>) -> bool {
    env_theme.and_then(from_env).is_some()
}

/// The mode, from every source in the order they outrank each other
/// (`theme_detection.zig:22-37`).
pub(crate) fn detect(env_theme: Option<&str>, osc: Option<Mode>, colorfgbg: Option<&str>) -> Mode {
    env_theme
        .and_then(from_env)
        .or(osc)
        .or_else(|| colorfgbg.and_then(from_colorfgbg))
        .unwrap_or(Mode::Dark)
}

/// Whether the terminal can be asked for an exact colour
/// (`theme_protocol.zig:44-53`).
///
/// Truecolor is **claimed**, never assumed: `COLORTERM` containing `truecolor`
/// or `24bit` is the claim, and nothing else is. Apple Terminal is not believed
/// even when it makes the claim, because it is the one mainstream terminal that
/// quantizes a `38;2` to its own palette rather than rendering it
/// (`theme_protocol.zig:42-43`).
///
/// **Two deliberate divergences from upstream, both toward
/// [`Depth::Ansi256`]:** upstream returns truecolor for a terminal that claims
/// nothing (`theme_protocol.zig:52`, and its test at `:63-66`), and lets a
/// `COLORTERM` claim outrank the program name so that Apple Terminal *with*
/// `COLORTERM=truecolor` is believed (`:46`, test at `:55-57`). Both are safe
/// for upstream because its only truecolor use is a diff marker whose 256-colour
/// fallback is a visibly different green. Here the two depths are the same five
/// greys in two notations, so guessing wrong costs a terminal that quantizes
/// silently -- and 256 colours render correctly on every terminal that has
/// truecolor, while the converse is false. The conservative answer is the one
/// that cannot be wrong on screen.
pub(crate) fn depth_from_env(colorterm: Option<&str>, term_program: Option<&str>) -> Depth {
    if term_program.is_some_and(|value| value == "Apple_Terminal") {
        return Depth::Ansi256;
    }
    let claimed =
        colorterm.is_some_and(|value| value.contains("truecolor") || value.contains("24bit"));
    if claimed {
        Depth::TrueColor
    } else {
        Depth::Ansi256
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_osc_eleven_reply_is_light_above_half_luminance() {
        // theme_protocol.zig:11-40 -- luminance > 32768 is light.
        assert_eq!(
            parse_osc11("\u{1b}]11;rgb:ffff/ffff/ffff\u{1b}\\"),
            Some(Mode::Light)
        );
        assert_eq!(
            parse_osc11("\u{1b}]11;rgb:0000/0000/0000\u{1b}\\"),
            Some(Mode::Dark)
        );
        assert_eq!(
            parse_osc11("\u{1b}]11;rgb:1c1c/1c1c/1c1c\u{07}"),
            Some(Mode::Dark)
        );
        assert_eq!(parse_osc11("not a reply"), None);
    }

    #[test]
    fn colorfgbg_is_read_when_the_terminal_will_not_answer() {
        // theme_protocol.zig:68-76
        assert_eq!(from_colorfgbg("15;0"), Some(Mode::Dark));
        assert_eq!(from_colorfgbg("0;15"), Some(Mode::Light));
        assert_eq!(from_colorfgbg("nonsense"), None);
    }

    #[test]
    fn the_precedence_is_the_environment_then_the_query_then_colorfgbg_then_dark() {
        // theme_detection.zig:22-37
        assert_eq!(
            detect(Some("light"), Some(Mode::Dark), Some("15;0")),
            Mode::Light
        );
        assert_eq!(detect(None, Some(Mode::Light), Some("15;0")), Mode::Light);
        assert_eq!(detect(None, None, Some("0;15")), Mode::Light);
        assert_eq!(detect(None, None, None), Mode::Dark);
        assert_eq!(detect(Some("nonsense"), None, None), Mode::Dark);
    }

    #[test]
    fn truecolor_is_gated_on_colorterm_and_apple_terminal_is_downgraded() {
        // theme_protocol.zig:44-53
        assert_eq!(depth_from_env(Some("truecolor"), None), Depth::TrueColor);
        assert_eq!(depth_from_env(Some("24bit"), None), Depth::TrueColor);
        assert_eq!(
            depth_from_env(Some("truecolor"), Some("Apple_Terminal")),
            Depth::Ansi256
        );
        assert_eq!(depth_from_env(None, None), Depth::Ansi256);
    }

    #[test]
    fn the_two_palettes_differ_and_both_end_a_run_the_same_way() {
        let dark = Palette {
            mode: Mode::Dark,
            depth: Depth::Ansi256,
        };
        let light = Palette {
            mode: Mode::Light,
            depth: Depth::Ansi256,
        };
        assert_eq!(dark.reset(), "\u{1b}[0m");
        assert_eq!(light.reset(), "\u{1b}[0m");
        assert_ne!(
            dark.hint(),
            light.hint(),
            "the two palettes paint identically"
        );
        assert_ne!(dark.divider(), light.divider());
    }

    #[test]
    fn every_shade_is_spelled_the_same_in_both_notations() {
        // The greys are upstream's, by 256-colour index (`render.zig:28,29,34`
        // dark, `:70,71,76` light), and the direct-colour spelling of each is
        // that index's own level on xterm's greyscale ramp -- index `i` in
        // `232..=255` is `8 + (i - 232) * 10` on all three channels. So these
        // are not two palettes but one, said twice, and the pairs are written
        // out because that claim is only checkable against the numbers: a
        // transposed digit in either column reads as a plausible grey and shows
        // up nowhere else.
        for (mode, role, index, level) in [
            (Mode::Dark, DIVIDER, 240u32, 88u32),
            (Mode::Dark, HINT, 255, 238),
            (Mode::Dark, NOTICE, 250, 188),
            (Mode::Light, DIVIDER, 250, 188),
            (Mode::Light, HINT, 235, 38),
            (Mode::Light, NOTICE, 241, 98),
        ] {
            assert_eq!(
                level,
                8 + (index - 232) * 10,
                "index {index} is not the ramp level this pair claims"
            );
            let ansi = Palette {
                mode,
                depth: Depth::Ansi256,
            };
            let direct = Palette {
                mode,
                depth: Depth::TrueColor,
            };
            assert_eq!(
                ansi.paint(role),
                format!("\u{1b}[38;5;{index}m"),
                "{mode:?} role {role} at 256 colours"
            );
            assert_eq!(
                direct.paint(role),
                format!("\u{1b}[38;2;{level};{level};{level}m"),
                "{mode:?} role {role} at direct colour"
            );
        }
        // and the two a reader will look for first, spelled outright
        let dark = Palette {
            mode: Mode::Dark,
            depth: Depth::TrueColor,
        };
        let light = Palette {
            mode: Mode::Light,
            depth: Depth::TrueColor,
        };
        assert_eq!(dark.divider(), "\u{1b}[38;2;88;88;88m");
        assert_eq!(light.hint(), "\u{1b}[38;2;38;38;38m");
    }
}
