//! OSC 8 hyperlinks: the one string sequence a row may carry beside a colour.
//!
//! Upstream's markdown presenter wraps every link in an answer -- an inline
//! `[text](target)`, a bare URL, an autolink, a URL in a code span -- in an
//! OSC 8 hyperlink with a process-wide id (`id=fx-N`,
//! `inline_render.zig:436-460,535-560`), and its image badge is one to the file
//! (`image_attachments.zig:1336-1340`). Its terminal writer spells both the same
//! way, and only that way (`emitHyperlinkOpen`, `core/terminal/engine.zig:3262-3272`):
//!
//! * open: `ESC ] 8 ; <params> ; <uri> ESC \`, where `<params>` is empty or
//!   `id=<id>`;
//! * close: `ESC ] 8 ; ; ESC \`.
//!
//! The string terminator is `ST` (`ESC \`), never `BEL`. A sequence spelled any
//! other way is not one this crate writes, and [`link_at`] does not recognize it.
//!
//! **The URI is somebody else's text**, and that is what makes this module a
//! gate rather than a parser. A link's target comes from a model's answer or a
//! file's name, and an `OSC` string ends at the first `BEL` or `ST`: a target
//! carrying either would close the hyperlink early and hand everything after it
//! to the terminal as instructions. So a link is admitted only when it is
//! **inert** ([`is_inert_uri`]):
//!
//! * **no control character at all** -- C0 (`ESC` and `BEL` among them), `DEL`
//!   and C1 (`U+009C` is `ST` to a terminal that reads 8-bit controls). Not a
//!   list of terminators: every control goes, for the reason the window title's
//!   do (`super::frame::title`) -- there is no control a URI has a use for;
//! * **at most [`MAX_URI_BYTES`] bytes**, upstream's own cap on a link's target
//!   (`ansi.max_link_url_bytes`, checked at `inline_render.zig:917-921`);
//! * **one of four schemes**: `http`, `https`, `file`, `mailto`, compared
//!   without case as RFC 3986 says a scheme is. Upstream has no scheme rule for
//!   an inline link -- it checks the length and the control bytes only
//!   (`isValidLinkUrl`, `inline_render.zig:917-921`), so `[x](javascript:...)`,
//!   `[x](ftp://...)` and `[x](README.md)` are links there. Here they are text:
//!   a terminal hands a clicked URI to whatever the system opens that scheme
//!   with, and a target a model chose is not one this crate should make one
//!   click away. Bare URLs and code-span URLs are `http`/`https` in upstream
//!   already, and an email autolink is `mailto:`.
//!
//! The parameters are held to the one shape upstream writes, `id=` and a short
//! run of `[A-Za-z0-9._-]`: a `:` would begin a second key and a `;` would end
//! the parameters early.
//!
//! What this module does **not** decide is where a link ends. A link must be
//! closed on the row it opened on -- tmux ends an active hyperlink at an
//! automatic wrap (`engine.zig:2400-2406`), and a link left open across a `CUP`
//! would make whatever the next write paints a part of it. That is the
//! painter's to keep (`super::frame::row_text`, `super::grid::Grid::diff`) and
//! the output check's to refuse (`super::check`).

use std::path::Path;

/// What opens every OSC 8 sequence.
pub(crate) const PREFIX: &str = "\u{1b}]8;";

/// The string terminator upstream writes, and the only one admitted.
pub(crate) const TERMINATOR: &str = "\u{1b}\\";

/// The close: an OSC 8 with no parameters and no target.
pub(crate) const CLOSE: &str = "\u{1b}]8;;\u{1b}\\";

/// The longest target a link may carry: upstream's presenter's cap
/// (`ansi.max_link_url_bytes`), which a longer target falls back to literal
/// text under.
pub(crate) const MAX_URI_BYTES: usize = crate::markdown::ansi::MAX_LINK_URL_BYTES;

/// The longest `id` a link's parameters may name. Upstream's are `fx-` and a
/// counter, a dozen bytes at most.
const MAX_ID_BYTES: usize = 64;

/// The schemes a link may point into.
const SCHEMES: [&str; 4] = ["http", "https", "file", "mailto"];

/// One OSC 8 sequence, as [`link_at`] found it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hyperlink<'a> {
    /// A link opens: everything written until the next close, or the next
    /// open, is part of it.
    Open { params: &'a str, uri: &'a str },
    /// The link that was open, closed.
    Close,
}

/// How many bytes the hyperlink sequence at the head of `text` takes, and what
/// it is, when it is one this crate may write.
///
/// `None` for anything else -- another `OSC`, a `BEL`-terminated OSC 8, one
/// whose target is not inert, a close with parameters -- and the caller then
/// treats it as the escape it is (`super::frame::row_text` drops it whole).
///
/// The extent is the one [`super::pacer::escape_at`] measures, for every
/// sequence this admits: the parameters and the target hold no control
/// character, so the first `ESC` in the body is the terminator's and nothing
/// before it ends the string.
pub(crate) fn link_at(text: &str) -> Option<(usize, Hyperlink<'_>)> {
    let body = text.strip_prefix(PREFIX)?;
    let end = body.find('\u{1b}')?;
    if !body[end..].starts_with(TERMINATOR) {
        return None;
    }
    let (params, uri) = body[..end].split_once(';')?;
    let len = PREFIX.len() + end + TERMINATOR.len();
    if uri.is_empty() {
        return params.is_empty().then_some((len, Hyperlink::Close));
    }
    (is_inert_params(params) && is_inert_uri(uri)).then_some((len, Hyperlink::Open { params, uri }))
}

/// The open sequence for `uri` under `params`, as upstream spells it.
pub(crate) fn open(params: &str, uri: &str) -> String {
    format!("{PREFIX}{params};{uri}{TERMINATOR}")
}

/// Whether a link's parameters are empty or the one `id=` upstream writes.
fn is_inert_params(params: &str) -> bool {
    if params.is_empty() {
        return true;
    }
    let Some(id) = params.strip_prefix("id=") else {
        return false;
    };
    !id.is_empty()
        && id.len() <= MAX_ID_BYTES
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

/// Whether `uri` is a target a link may carry: short, free of every control
/// character, and in one of the four schemes.
pub(crate) fn is_inert_uri(uri: &str) -> bool {
    !uri.is_empty()
        && uri.len() <= MAX_URI_BYTES
        && !uri.chars().any(char::is_control)
        && scheme_allowed(uri)
}

/// Whether `uri` begins with a scheme -- RFC 3986's `ALPHA *( ALPHA / DIGIT /
/// "+" / "-" / "." ) ":"` -- and it is one of [`SCHEMES`].
///
/// A target with no scheme is a relative reference, which a terminal resolves
/// against nothing anybody chose; it is refused with the rest.
fn scheme_allowed(uri: &str) -> bool {
    let Some((scheme, _)) = uri.split_once(':') else {
        return false;
    };
    let mut bytes = scheme.bytes();
    if !bytes.next().is_some_and(|byte| byte.is_ascii_alphabetic()) {
        return false;
    }
    if !bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.')) {
        return false;
    }
    SCHEMES
        .iter()
        .any(|allowed| scheme.eq_ignore_ascii_case(allowed))
}

/// The `file://` target for `path`, as upstream's image badge spells it: every
/// byte outside `A-Z a-z 0-9 - . _ ~ /` percent-encoded (`writePercentEncodedPath`
/// and `isUriPathSafe`, `image_attachments.zig:1632-1651`).
///
/// `None` for a path that is not absolute -- a `file://` URI names a path from
/// the root, and a relative one would name whatever the terminal's working
/// directory makes of it -- and for one whose target would not be admitted
/// ([`is_inert_uri`]), which is a path longer than a link may be.
pub(crate) fn file_uri(path: &Path) -> Option<String> {
    use std::os::unix::ffi::OsStrExt;

    if !path.is_absolute() {
        return None;
    }
    let mut uri = String::from("file://");
    for &byte in path.as_os_str().as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/') {
            uri.push(char::from(byte));
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    is_inert_uri(&uri).then_some(uri)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOCS: &str = "\u{1b}]8;id=fx-1;https://example.com/docs\u{1b}\\";

    #[test]
    fn upstreams_open_and_close_are_links() {
        assert_eq!(
            link_at(DOCS),
            Some((
                DOCS.len(),
                Hyperlink::Open {
                    params: "id=fx-1",
                    uri: "https://example.com/docs"
                }
            ))
        );
        assert_eq!(link_at(CLOSE), Some((CLOSE.len(), Hyperlink::Close)));
        // The badge's: no parameters at all.
        let badge = "\u{1b}]8;;file:///tmp/a.png\u{1b}\\[Image 1]";
        assert_eq!(
            link_at(badge),
            Some((
                badge.len() - "[Image 1]".len(),
                Hyperlink::Open {
                    params: "",
                    uri: "file:///tmp/a.png"
                }
            ))
        );
    }

    #[test]
    fn the_extent_is_the_one_the_escape_scanner_measures() {
        for sequence in [DOCS, CLOSE, "\u{1b}]8;;mailto:a@b.c\u{1b}\\"] {
            let text = format!("{sequence}tail");
            assert_eq!(
                link_at(&text).map(|(len, _)| len),
                super::super::pacer::escape_at(&text),
                "{sequence:?}"
            );
        }
    }

    #[test]
    fn the_four_schemes_are_admitted_whatever_their_case() {
        for uri in [
            "http://example.com",
            "https://example.com/a?b=c#d",
            "file:///Users/me/a.png",
            "mailto:someone@example.com",
            "HTTPS://EXAMPLE.COM",
            "File:///x",
        ] {
            assert!(is_inert_uri(uri), "{uri}");
        }
    }

    #[test]
    fn every_other_scheme_and_a_relative_target_are_refused() {
        for uri in [
            "javascript:alert(1)",
            "JavaScript:alert(1)",
            "data:text/html,<script>",
            "vbscript:x",
            "ftp://example.com/f",
            "ssh://host",
            "x-man-page://ls",
            "README.md",
            "./README.md",
            "/etc/passwd",
            "//example.com/a",
            ":no-scheme",
            "1http://x",
            "ht tp://x",
            "h\u{e9}ttp://x",
        ] {
            assert!(!is_inert_uri(uri), "{uri}");
            let sequence = open("id=fx-1", uri);
            assert_eq!(link_at(&sequence), None, "{uri}");
        }
    }

    #[test]
    fn a_target_with_any_control_character_is_refused() {
        // C0 including the two string terminators' first bytes, DEL, and C1 --
        // `U+009C` is `ST` itself to a terminal that reads 8-bit controls, and
        // `U+009B` is `CSI`.
        for control in [
            '\u{0}', '\u{7}', '\u{8}', '\t', '\n', '\r', '\u{1b}', '\u{7f}', '\u{80}', '\u{9b}',
            '\u{9c}', '\u{9d}',
        ] {
            let uri = format!("https://example.com/{control}x");
            assert!(!is_inert_uri(&uri), "{control:?}");
        }
    }

    #[test]
    fn a_bel_terminated_link_is_not_this_crates_spelling() {
        assert_eq!(link_at("\u{1b}]8;;https://example.com\u{7}"), None);
        assert_eq!(link_at("\u{1b}]8;;\u{7}"), None);
        // A BEL inside an ST-terminated body is a control in the target.
        assert_eq!(link_at("\u{1b}]8;;https://a\u{7}b\u{1b}\\"), None);
    }

    #[test]
    fn an_escape_inside_the_target_ends_nothing_and_admits_nothing() {
        // An `ESC` that is not the terminator's: refused, and the scanner
        // measures this as a lone `ESC`, so nothing after it is executed --
        // `super::frame`'s tests show what reaches the screen.
        let text = "\u{1b}]8;;https://a\u{1b}[2Jb\u{1b}\\";
        assert_eq!(link_at(text), None);
        // A terminator inside what the author meant as the target ends the
        // link there; what follows is not part of it, and is judged on its own.
        let text = "\u{1b}]8;;https://a\u{1b}\\\u{1b}[2J";
        assert_eq!(
            link_at(text),
            Some((
                text.len() - "\u{1b}[2J".len(),
                Hyperlink::Open {
                    params: "",
                    uri: "https://a"
                }
            ))
        );
    }

    #[test]
    fn an_unterminated_link_is_not_a_link() {
        assert_eq!(link_at("\u{1b}]8;;https://example.com"), None);
        assert_eq!(link_at("\u{1b}]8;;https://example.com\u{1b}"), None);
        assert_eq!(link_at("\u{1b}]8;id=fx-1"), None);
        assert_eq!(link_at("\u{1b}]8"), None);
    }

    #[test]
    fn a_target_longer_than_upstreams_cap_is_refused() {
        let at_cap = format!(
            "https://e.x/{}",
            "a".repeat(MAX_URI_BYTES - "https://e.x/".len())
        );
        assert_eq!(at_cap.len(), MAX_URI_BYTES);
        assert!(is_inert_uri(&at_cap));
        let over = format!("{at_cap}a");
        assert!(!is_inert_uri(&over));
        assert_eq!(link_at(&open("", &over)), None);
    }

    #[test]
    fn parameters_are_upstreams_id_and_nothing_else() {
        for params in ["id=fx-1", "id=fx-4294967295", "id=a.b_c-D", ""] {
            assert!(
                link_at(&open(params, "https://example.com")).is_some(),
                "{params}"
            );
        }
        for params in [
            "id=",
            "id=a:b=c",
            "id=a b",
            "id=\u{e9}",
            "x=1",
            "id=fx-1:x=2",
            &format!("id={}", "a".repeat(MAX_ID_BYTES + 1)),
        ] {
            assert_eq!(
                link_at(&open(params, "https://example.com")),
                None,
                "{params}"
            );
        }
        // A `;` in the parameters ends them: what is left is the target, and
        // it has to stand on its own.
        assert_eq!(link_at(&open("id=a;javascript:x", "y")), None);
    }

    #[test]
    fn a_close_with_parameters_is_not_upstreams_close() {
        assert_eq!(link_at("\u{1b}]8;id=fx-1;\u{1b}\\"), None);
    }

    #[test]
    fn other_operating_system_commands_are_not_links() {
        for text in [
            "\u{1b}]2;title\u{1b}\\",
            "\u{1b}]52;c;aGVsbG8=\u{1b}\\",
            "\u{1b}]7;file:///tmp\u{1b}\\",
            "\u{1b}]80;;https://x\u{1b}\\",
            "\u{1b}]8:;https://x\u{1b}\\",
        ] {
            assert_eq!(link_at(text), None, "{text:?}");
        }
    }

    #[test]
    fn a_file_target_is_percent_encoded_as_upstreams_badge_is() {
        // `image_attachments.zig:2016` and `:2028`.
        assert_eq!(
            file_uri(Path::new("/Users/me/CleanShot 2026.png")).as_deref(),
            Some("file:///Users/me/CleanShot%202026.png")
        );
        assert_eq!(
            file_uri(Path::new("/tmp/a#b?c%d.png")).as_deref(),
            Some("file:///tmp/a%23b%3Fc%25d.png")
        );
        // Every byte of a name a user or a tool chose -- controls, the
        // terminators, non-ASCII -- comes out as `%XX`, so the target is
        // inert whatever the name was.
        let hostile = "/tmp/\u{1b}]8;;x\u{7}\u{9c}\u{e9}.png";
        let uri = file_uri(Path::new(hostile)).expect("an absolute path");
        assert!(uri.bytes().all(|byte| byte.is_ascii_graphic()), "{uri}");
        assert_eq!(uri, "file:///tmp/%1B%5D8%3B%3Bx%07%C2%9C%C3%A9.png");
    }

    #[test]
    fn a_relative_or_overlong_file_has_no_target() {
        assert_eq!(file_uri(Path::new("image-1-abc.bin")), None);
        let long = format!("/{}", "a".repeat(MAX_URI_BYTES));
        assert_eq!(file_uri(Path::new(&long)), None);
    }
}
