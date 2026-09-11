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
//! The palette a session **starts** in is decided that way, and it does not
//! stay decided: a terminal whose user switches their system theme mid-session
//! says so, and this module carries that half of the protocol too -- mode 2031
//! asks to be told ([`super::term::MODE_SET`]), [`MODE_QUERY`] asks outright,
//! and [`notification`] reads the answer either arrives as. What is *not* here
//! is upstream's live RGB monitor (`theme_monitor.zig`): a notification carries
//! a [`Mode`] and nothing else, so a session following one is following which
//! way round the terminal is rather than re-reading its background colour.

use std::time::Duration;

use super::check::Color;

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

/// The question a running session asks: *which way round are you now?*
///
/// `DSR ? 996 n`, upstream's `theme_monitor.zig:288-289` pair with mode 2031:
/// the mode asks the terminal to volunteer a [`notification`] when its
/// background changes, and this asks for one outright. A session needs both
/// because the mode only covers changes that happen while somebody is listening
/// -- a process that was stopped, handed the terminal back, and resumed was not.
///
/// **Not written at launch by anything but [`super::probe`]**, and not written
/// at all by a session whose palette [`ENV`] already decided: a terminal that
/// does not implement 2031 answers neither this nor [`QUERY`], and a decided
/// session has nothing for either answer to change.
pub(crate) const MODE_QUERY: &str = "\u{1b}[?996n";

/// What a `CSI ? 997 ; N n` says the terminal's background just became
/// (`theme_monitor.zig:288-289`).
///
/// `params` is the sequence's parameter bytes **with its private-marker
/// prefix**, exactly as the decoder framed them, and the match is on those
/// bytes rather than on numbers they parse to. That is the same strictness
/// [`super::input`] gives the paste markers and for the same reason: `?0997;1`
/// and `?997;1;0` parse to the same pair and are not sequences any terminal
/// sends, so reading them as a theme change would be inventing a report out of
/// a stream that carried none. `None` is every one of those, and the caller
/// answers it the way it answers any sequence it has no binding for.
///
/// Two values and no third: `1` is dark and `2` is light. A `?997;3` is a
/// terminal saying something this protocol has no meaning for, and guessing at
/// it would repaint the band on a report nobody made.
pub(crate) fn notification(params: &[u8]) -> Option<Mode> {
    match params {
        b"?997;1" => Some(Mode::Dark),
        b"?997;2" => Some(Mode::Light),
        _ => None,
    }
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
/// Four of the roles are the band's, because four of its rows carry colour in
/// this phase: the rule that separates the document from the band, the hint row
/// at the bottom, a refusal shown on it, and the activity row a running turn
/// adds above the rule. The composer's own rows carry none, which is upstream's
/// choice too -- `input_bar_style` is empty in both themes
/// (`render.zig:69,88`). The fifth is the **document's**: the answer text a
/// transcript row is made of ([`Self::body`]).
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

/// The greys upstream paints these four roles in, by 256-colour index
/// (`render.zig:28,29,34` dark and `render.zig:70,71,76` light, for the
/// first three).
///
/// The third is upstream's `system_notice_text_style`, which is what a refusal
/// on the hint row is: xfx's own words about something that did not happen.
///
/// The fourth is the activity row's. Upstream paints its thinking marker and
/// the label/elapsed beside it in `permission_auto_style`
/// (`shimmer_runtime.zig:262-291`), whose dark/light greys are
/// `render.zig:55,86,105`'s `252`/`238` -- pinned here exactly, because that
/// is the only fact upstream settles about this row's colour. What is *not*
/// borrowed is the name: a running turn and a granted permission happen to
/// share upstream's grey, not upstream's meaning, so this crate calls the
/// role what it is -- an ongoing, neutral turn status -- and never
/// `permission_auto`.
/// The fifth is the answer text itself, which upstream paints in the same grey
/// as the hint row (`render.zig:29` dark and `:71` light -- `255` and `235`).
/// Spelled as a role of its own rather than read off [`HINT`], because the two
/// are the same colour and not the same decision: a phase that retints the hint
/// row must not silently retint every row of the document with it.
const DARK: [u8; 5] = [240, 255, 250, 252, 255];
const LIGHT: [u8; 5] = [250, 235, 241, 238, 235];

/// Where in one of those tuples each role sits.
const DIVIDER: usize = 0;
const HINT: usize = 1;
const NOTICE: usize = 2;
const ACTIVITY: usize = 3;
const BODY: usize = 4;

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

    /// The row a running turn adds above the rule.
    ///
    /// A neutral, ongoing-turn status -- what is running, or that a decision
    /// on it is pending -- and nothing more: not a success, an error, a
    /// permission grant or a progress percentage. See [`DARK`]/[`LIGHT`] for
    /// why this shares upstream's colour without sharing upstream's name for
    /// it.
    pub(crate) fn activity(&self) -> &'static str {
        self.paint(ACTIVITY)
    }

    /// The answer text a document row carries.
    ///
    /// Not a band role: the rows this paints are the terminal's own document
    /// ([`super::transcript`]), and they are painted at the moment they are
    /// handed over rather than repainted with the band.
    pub(crate) fn body(&self) -> &'static str {
        self.paint(BODY)
    }

    /// What ends a run, in either mode and at either depth.
    pub(crate) fn reset(&self) -> &'static str {
        RESET
    }

    /// What this palette repaints an **already painted** document cell in, or
    /// `None` for a cell it has nothing to say about.
    ///
    /// Two of the five roles reach the document -- the answer text
    /// ([`Self::body`]) and xfx's own lines ([`Self::notice`]) -- and a cell is
    /// recognised by the colour it is *holding*, in **either** mode, at this
    /// palette's own depth. Both sides map onto the mode in force rather than
    /// old-to-new, and that is what makes a run of reports converge: a chained
    /// pair would take a session reported dark, light and dark again back to
    /// light, because the second report would find cells the first one never
    /// reached.
    ///
    /// `None` is most of a screen, and each of its three shapes is a decision.
    /// A colour this crate did not paint is somebody else's -- the user's own
    /// echo carries none at all -- and is left exactly as it is. A cell already
    /// in this palette's grey is not a cell to rewrite, which is what makes a
    /// report that flipped and flipped back cost no bytes. And a spelling at
    /// the other depth is not this session's painting: the depth is a property
    /// of the terminal program and does not move under a running session
    /// (`depth_from_env`).
    pub(crate) fn document_retint(&self, colour: Color) -> Option<&'static str> {
        let role = [NOTICE, BODY].into_iter().find(|&role| {
            [Mode::Dark, Mode::Light]
                .into_iter()
                .any(|mode| self.shade(role, mode) == Some(colour))
        })?;
        if self.shade(role, self.mode) == Some(colour) {
            return None;
        }
        Some(self.paint(role))
    }

    /// One role's colour in `mode`, at this palette's depth, **as a value**.
    ///
    /// The comparison side of [`Self::document_retint`], and it does not go
    /// through [`Self::paint`] on purpose: what a cell holds was decoded from
    /// the wire into a [`Color`], so a match made on the bytes this module
    /// happens to spell a colour with would be comparing a sequence with
    /// itself. The direct spelling is xterm's greyscale ramp -- index `i` in
    /// `232..=255` is level `8 + (i - 232) * 10` on all three channels -- which
    /// is the same claim [`truecolor`] makes per index, said here as
    /// arithmetic. `None` for an index off that ramp, which is a colour this
    /// palette could not have painted at this depth.
    fn shade(&self, role: usize, mode: Mode) -> Option<Color> {
        let index = match mode {
            Mode::Dark => DARK[role],
            Mode::Light => LIGHT[role],
        };
        match self.depth {
            Depth::Ansi256 => Some(Color::Indexed(index)),
            Depth::TrueColor => {
                let level = index.checked_sub(232)?.checked_mul(10)?.checked_add(8)?;
                Some(Color::Rgb(level, level, level))
            }
        }
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
        238 => "\u{1b}[38;5;238m",
        240 => "\u{1b}[38;5;240m",
        241 => "\u{1b}[38;5;241m",
        250 => "\u{1b}[38;5;250m",
        252 => "\u{1b}[38;5;252m",
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
        238 => "\u{1b}[38;2;68;68;68m",
        240 => "\u{1b}[38;2;88;88;88m",
        241 => "\u{1b}[38;2;98;98;98m",
        250 => "\u{1b}[38;2;188;188;188m",
        252 => "\u{1b}[38;2;208;208;208m",
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
        assert_ne!(dark.activity(), light.activity());
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
            // `shimmer_runtime.zig:262-291` / `render.zig:55,86,105`: the
            // activity row's literal indices, pinned rather than derived.
            (Mode::Dark, ACTIVITY, 252, 208),
            // `render.zig:29,71`: the answer text's own grey, which is the hint
            // row's number under a name of its own.
            (Mode::Dark, BODY, 255, 238),
            (Mode::Light, DIVIDER, 250, 188),
            (Mode::Light, HINT, 235, 38),
            (Mode::Light, NOTICE, 241, 98),
            (Mode::Light, ACTIVITY, 238, 68),
            (Mode::Light, BODY, 235, 38),
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
        // The activity row's direct-colour spelling, literal: `252` and `238`
        // are `208` and `68` on the ramp, not a number this test asked the
        // accessor to confirm about itself.
        assert_eq!(dark.activity(), "\u{1b}[38;2;208;208;208m");
        assert_eq!(light.activity(), "\u{1b}[38;2;68;68;68m");
    }

    #[test]
    fn a_document_cell_is_retinted_from_either_mode_onto_the_one_in_force() {
        // Both sides map onto the mode the session is in, rather than old to
        // new: a chained pair would take a terminal reported dark, light and
        // dark again back to *light*, because the second report would find the
        // cells the first one never reached.
        //
        // The colours are literals on both sides -- what a cell holds and what
        // it is to be repainted in -- so nothing here is an expectation this
        // mapper computed about itself.
        let light = Palette {
            mode: Mode::Light,
            depth: Depth::Ansi256,
        };
        assert_eq!(
            light.document_retint(Color::Indexed(255)),
            Some("\u{1b}[38;5;235m"),
            "the dark answer grey was not taken to the light one"
        );
        assert_eq!(
            light.document_retint(Color::Indexed(250)),
            Some("\u{1b}[38;5;241m"),
            "the dark notice grey was not taken to the light one"
        );
        assert_eq!(
            light.document_retint(Color::Indexed(235)),
            None,
            "a cell already in the palette's own grey was rewritten"
        );
        let dark = Palette {
            mode: Mode::Dark,
            depth: Depth::Ansi256,
        };
        assert_eq!(
            dark.document_retint(Color::Indexed(235)),
            Some("\u{1b}[38;5;255m")
        );
        assert_eq!(
            dark.document_retint(Color::Indexed(241)),
            Some("\u{1b}[38;5;250m")
        );
        assert_eq!(dark.document_retint(Color::Indexed(255)), None);
    }

    #[test]
    fn a_direct_colour_session_matches_and_repaints_in_the_same_notation() {
        // The same two pairs at the other depth, spelled out: `255` and `235`
        // are `238` and `38` on xterm's ramp, and `250` and `241` are `188` and
        // `98`. A session that matched one notation and repainted in the other
        // would leave every answer row holding a colour nothing recognises the
        // next time the terminal changes.
        let light = Palette {
            mode: Mode::Light,
            depth: Depth::TrueColor,
        };
        assert_eq!(
            light.document_retint(Color::Rgb(238, 238, 238)),
            Some("\u{1b}[38;2;38;38;38m")
        );
        assert_eq!(
            light.document_retint(Color::Rgb(188, 188, 188)),
            Some("\u{1b}[38;2;98;98;98m")
        );
        assert_eq!(light.document_retint(Color::Rgb(38, 38, 38)), None);
        // And nothing across the two depths: an indexed cell is not one this
        // session painted, and neither is a direct one in a 256-colour session.
        assert_eq!(light.document_retint(Color::Indexed(255)), None);
        assert_eq!(
            Palette {
                mode: Mode::Light,
                depth: Depth::Ansi256,
            }
            .document_retint(Color::Rgb(238, 238, 238)),
            None
        );
    }

    #[test]
    fn the_colours_this_crate_does_not_own_in_the_document_are_left_alone() {
        // The band's own three roles are painted by the band's own frame, and
        // the user's echo and anything the terminal was already holding are
        // nobody's to repaint here: a retint that reached them would rewrite a
        // shell's output in xfx's greys.
        let light = Palette {
            mode: Mode::Light,
            depth: Depth::Ansi256,
        };
        for foreign in [
            Color::Default,
            Color::Indexed(240), // the divider's
            Color::Indexed(252), // the activity row's
            Color::Indexed(238),
            Color::Indexed(31),
            Color::Rgb(1, 2, 3),
        ] {
            assert_eq!(
                light.document_retint(foreign),
                None,
                "{foreign:?} was treated as a colour this crate painted"
            );
        }
    }

    #[test]
    fn the_document_body_reads_against_the_background_the_session_is_on() {
        // What a transcript row is painted with, and the one thing about it
        // this module decides: the two modes must not paint it the same, or a
        // light terminal gets the dark palette's near-white answer text on
        // white.
        let dark = Palette {
            mode: Mode::Dark,
            depth: Depth::Ansi256,
        };
        let light = Palette {
            mode: Mode::Light,
            depth: Depth::Ansi256,
        };
        assert_eq!(dark.body(), "\u{1b}[38;5;255m");
        assert_eq!(light.body(), "\u{1b}[38;5;235m");
        assert_ne!(dark.body(), light.body());
    }
}
