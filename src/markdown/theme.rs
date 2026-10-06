//! The slice of upstream's theme table the markdown presenter reads.
//!
//! Upstream keeps one process-wide active theme (`theme.zig:167-175`) that
//! core producers read through `current()`. Only five of its slots reach this
//! module -- the inline code and completed-task colours, the link colour, the
//! light flag, and the syntax palette -- so only those are ported here, with
//! the builtin values byte-for-byte (`theme.zig:74-158`). Nothing in this
//! module holds a global: the caller owns a [`Theme`] and hands it to
//! [`super::Styles`] or to [`super::code_highlight::highlight`].

use std::borrow::Cow;

/// The colours code highlighting paints with (`theme.zig:19-33`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxPalette {
    /// `syntax: false` in a native theme turns highlighting off; the
    /// highlighter then passes text through.
    pub enabled: bool,
    pub keyword_style: Cow<'static, str>,
    pub string_style: Cow<'static, str>,
    pub number_style: Cow<'static, str>,
    pub comment_style: Cow<'static, str>,
    /// Command words (the grammar's `variable.function`).
    pub function_style: Cow<'static, str>,
    /// `$VAR`, `${VAR}`, and `~` references.
    pub variable_style: Cow<'static, str>,
    /// `&&`, `|`, redirects, glob stars.
    pub operator_style: Cow<'static, str>,
}

/// The theme slots this module reads (a subset of `theme.zig:35-71`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    pub name: Cow<'static, str>,
    pub light: bool,
    pub inline_code_open: Cow<'static, str>,
    pub task_completed_open: Cow<'static, str>,
    pub link_style: Cow<'static, str>,
    pub syntax: SyntaxPalette,
}

/// Upstream's dark builtin (`theme.zig:74-115`).
pub const FX_DARK: Theme = Theme {
    name: Cow::Borrowed("fx-dark"),
    light: false,
    inline_code_open: Cow::Borrowed("\x1b[38;5;245m"),
    task_completed_open: Cow::Borrowed("\x1b[38;5;252m"),
    link_style: Cow::Borrowed("\x1b[38;5;75m"),
    syntax: SyntaxPalette {
        enabled: true,
        keyword_style: Cow::Borrowed("\x1b[38;5;252m"),
        string_style: Cow::Borrowed("\x1b[38;5;250m"),
        number_style: Cow::Borrowed("\x1b[38;5;250m"),
        comment_style: Cow::Borrowed("\x1b[38;5;245m"),
        function_style: Cow::Borrowed("\x1b[38;5;252m"),
        variable_style: Cow::Borrowed("\x1b[38;5;252m"),
        operator_style: Cow::Borrowed("\x1b[38;5;252m"),
    },
};

/// Upstream's light builtin (`theme.zig:117-158`).
pub const FX_LIGHT: Theme = Theme {
    name: Cow::Borrowed("fx-light"),
    light: true,
    inline_code_open: Cow::Borrowed("\x1b[38;5;247m"),
    task_completed_open: Cow::Borrowed("\x1b[38;5;238m"),
    link_style: Cow::Borrowed("\x1b[38;5;25m"),
    syntax: SyntaxPalette {
        enabled: true,
        keyword_style: Cow::Borrowed("\x1b[38;5;238m"),
        string_style: Cow::Borrowed("\x1b[38;5;241m"),
        number_style: Cow::Borrowed("\x1b[38;5;241m"),
        comment_style: Cow::Borrowed("\x1b[38;5;243m"),
        function_style: Cow::Borrowed("\x1b[38;5;238m"),
        variable_style: Cow::Borrowed("\x1b[38;5;238m"),
        operator_style: Cow::Borrowed("\x1b[38;5;238m"),
    },
};

/// The builtin theme for a background (`theme.zig:160-162`).
pub fn builtin(light: bool) -> Theme {
    if light {
        FX_LIGHT
    } else {
        FX_DARK
    }
}

/// True when an SGR open sets `param` ("1" bold, "3" italic, "48"
/// background), parsed from the parameter list rather than substring matched
/// (`theme.zig:327-356`). Handles slots built from several concatenated SGR
/// escapes.
pub fn sgr_has_param(open: &str, param: &str) -> bool {
    let mut rest = open;
    while let Some(start) = rest.find("\x1b[") {
        let after = &rest[start + 2..];
        let Some(end) = after.find('m') else {
            return false;
        };
        let mut parts = after[..end].split(';');
        let mut skip = 0usize;
        while let Some(part) = parts.next() {
            if skip > 0 {
                skip -= 1;
                continue;
            }
            if part == "38" || part == "48" {
                if part == param {
                    return true;
                }
                // Colour introducer: 5;n consumes one parameter, 2;r;g;b
                // three. Skipping keeps RGB triples from reading as
                // bold/italic.
                let Some(mode) = parts.next() else {
                    break;
                };
                if mode == "2" {
                    skip = 3;
                } else if mode == "5" {
                    skip = 1;
                }
                continue;
            }
            if part == param {
                return true;
            }
        }
        rest = &after[end + 1..];
    }
    false
}

/// The close that fully neutralizes an SGR open: always resets the
/// foreground, and resets background, bold, and italic only when the open set
/// them (`theme.zig:361-376`).
pub fn closing_for(open: &str) -> &'static str {
    let has_bg = sgr_has_param(open, "48");
    let has_bold = sgr_has_param(open, "1");
    let has_italic = sgr_has_param(open, "3");
    if has_bg {
        if has_bold && has_italic {
            return "\x1b[39m\x1b[49m\x1b[22m\x1b[23m";
        }
        if has_bold {
            return "\x1b[39m\x1b[49m\x1b[22m";
        }
        if has_italic {
            return "\x1b[39m\x1b[49m\x1b[23m";
        }
        return "\x1b[39m\x1b[49m";
    }
    if has_bold && has_italic {
        return "\x1b[39m\x1b[22m\x1b[23m";
    }
    if has_bold {
        return "\x1b[39m\x1b[22m";
    }
    if has_italic {
        return "\x1b[39m\x1b[23m";
    }
    "\x1b[39m"
}
