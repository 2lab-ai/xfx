//! The words of a prompt that name image files.
//!
//! Upstream reads a pasted text and a submitted prompt for paths to images and
//! attaches each one it can, its badge in place of the path: a paste at once,
//! before Enter (`handlePastedBytes`, `input_paste_runtime.zig:370-377`,
//! `:445-622`), and a prompt's typed paths as it is sent
//! (`extractInlineImageAttachments`, `input_submit_runtime.zig:756-776`,
//! `image_attachments.zig:1708-1760`). This module is the reading -- which
//! words are paths, and how each is loaded; what a failure says differs
//! between the two, and that is the shell's.
//!
//! A word is cut as a shell would cut it, quotes and backslash escapes holding
//! it together (`token_end`, `file_picker_path.zig:36-52`), and it is a path
//! to an image by its spelling alone, before anything is asked of the
//! filesystem (`splitImagePathToken`, `image_attachments.zig:1887-1918`):
//!
//! - it ends in `.png`, `.jpg`, `.jpeg`, `.gif` or `.webp`, in any case, once
//!   a balanced pair of outer quotes is set aside and an escaped `.` is not
//!   counted as one;
//! - a leading `@` is the file picker's and is not part of the path, and `@@`
//!   is no path at all;
//! - sentence punctuation after it -- `,` `.` `;` `:` `!` `?`, outside quotes
//!   and not escaped -- stays as text after the badge;
//! - a quoted `@"…"` whose only escapes are `\\` and `\"`, the spelling the
//!   picker writes, is the file's exact name once those are undone, and is
//!   loaded as it stands; any other quoted `@` word keeps the rules above.

use std::ops::Range;
use std::path::{Path, PathBuf};

use super::file_index::is_terminal_safe;
use super::file_picker::{parse_at, token_end, Status};
use crate::image::{AttachError, MediaType};

/// One word of a prompt that names an image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImagePath {
    /// The run of the text the badge replaces: the word, without the sentence
    /// punctuation after it.
    pub(crate) range: Range<usize>,
    /// The path as the loader is given it.
    pub(crate) path: String,
    /// Whether [`Self::path`] is a canonical `@"…"` payload with its escapes
    /// undone: the file's name as it stands, never normalized again.
    pub(crate) literal: bool,
}

/// What the spelling of one word says, before its place in the text.
#[derive(Debug, PartialEq, Eq)]
struct Split<'a> {
    path: String,
    suffix: &'a str,
    literal: bool,
}

fn is_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r')
}

fn is_sentence_punctuation(byte: u8) -> bool {
    matches!(byte, b',' | b'.' | b';' | b':' | b'!' | b'?')
}

/// Every word of `text` that names an image, in the order they stand.
pub(crate) fn find(text: &str) -> Vec<ImagePath> {
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    let mut index = 0;
    loop {
        while index < bytes.len() && is_whitespace(bytes[index]) {
            index += 1;
        }
        if index == bytes.len() {
            break;
        }
        let start = index;
        // A word ends at whitespace outside quotes, or at the end: a boundary
        // either way, whatever an escape stepped over on the way.
        index = token_end(bytes, start);
        let Some(word) = text.get(start..index) else {
            continue;
        };
        if let Some(split) = split(word) {
            found.push(ImagePath {
                range: start..index - split.suffix.len(),
                path: split.path,
                literal: split.literal,
            });
        }
    }
    found
}

/// Whether `text` holds a word that names an image (`hasImagePathToken`,
/// `image_attachments.zig:1969-1979`): a paste that does is read for images
/// rather than collapsed, whatever its length.
pub(crate) fn any(text: &str) -> bool {
    !find(text).is_empty()
}

/// The file `word` names, and its bytes: `/image`'s loader, given the path
/// with its `@` and its punctuation taken off
/// (`load_inline_image_attachment`, `image_attachments.zig:1880-1885`).
pub(crate) fn load(
    workspace: &Path,
    home: Option<&Path>,
    word: &ImagePath,
) -> Result<(PathBuf, MediaType, Vec<u8>), AttachError> {
    if word.literal {
        crate::image::load_literal(workspace, home, &word.path)
    } else {
        crate::image::load(workspace, home, &word.path)
    }
}

/// Whether a typed path that could not be attached is left as the words it is
/// rather than stopping the prompt (`extractInlineImageAttachments`,
/// `image_attachments.zig:1728-1738`): a file that is not there, is not an
/// image, is not a file, or a path that means nothing. Anything else -- too
/// large, outside the workspace, unreadable -- is said, and the prompt waits.
pub(crate) fn passed_over_at_submit(problem: &AttachError) -> bool {
    matches!(
        problem,
        AttachError::NotFound
            | AttachError::Unsupported
            | AttachError::Other("HomeNotSet" | "InvalidPath" | "NotRegularFile")
    )
}

/// `splitImagePathToken` (`image_attachments.zig:1887-1918`).
fn split(raw: &str) -> Option<Split<'_>> {
    let trimmed = raw.trim_matches([' ', '\t', '\r', '\n']);
    let bytes = trimmed.as_bytes();
    if trimmed.starts_with("@\"") {
        let token = parse_at(bytes, 0)?;
        if token.status != Status::Complete || token.end != bytes.len() {
            return None;
        }
        if token.canonical_escapes {
            let decoded = decode(trimmed.get(token.path_start..token.path_end)?)?;
            let dot = decoded.rfind('.')?;
            if !supported(&decoded[dot..]) {
                return None;
            }
            return Some(Split {
                suffix: trimmed.get(token.quote_end?..)?,
                path: decoded,
                literal: true,
            });
        }
        // Any other escape keeps the older reading, `\.` above all.
    }
    let path_start = usize::from(trimmed.starts_with('@'));
    if path_start == 1 && trimmed[1..].starts_with('@') {
        return None;
    }
    let suffix_start = trailing_punctuation_start(bytes);
    if path_start >= suffix_start {
        return None;
    }
    let path = trimmed.get(path_start..suffix_start)?;
    if !supported(extension_ignoring_escapes(strip_quotes(path))) {
        return None;
    }
    Some(Split {
        path: path.to_string(),
        suffix: trimmed.get(suffix_start..)?,
        literal: false,
    })
}

/// A canonical payload's escapes undone (`decode_into`,
/// `file_picker_path.zig:140-156`), or `None` for one that ends in a lone
/// backslash or could not be painted.
fn decode(payload: &str) -> Option<String> {
    if !is_terminal_safe(payload) {
        return None;
    }
    let mut out = String::with_capacity(payload.len());
    let mut chars = payload.chars();
    while let Some(scalar) = chars.next() {
        if scalar == '\\' {
            out.push(chars.next()?);
        } else {
            out.push(scalar);
        }
    }
    Some(out)
}

/// Where the sentence punctuation after a path begins
/// (`trailingSentencePunctuationStart`, `image_attachments.zig:1924-1952`):
/// a quoted or escaped byte is never punctuation.
fn trailing_punctuation_start(bytes: &[u8]) -> usize {
    let mut suffix_start = bytes.len();
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    for (index, &byte) in bytes.iter().enumerate() {
        if escaped {
            suffix_start = index + 1;
            escaped = false;
            continue;
        }
        if byte == b'\\' {
            suffix_start = index + 1;
            escaped = true;
            continue;
        }
        if let Some(open) = quote {
            suffix_start = index + 1;
            if byte == open {
                quote = None;
            }
            continue;
        }
        if byte == b'"' || byte == b'\'' {
            suffix_start = index + 1;
            quote = Some(byte);
            continue;
        }
        if !is_sentence_punctuation(byte) {
            suffix_start = index + 1;
        }
    }
    suffix_start
}

fn strip_quotes(text: &str) -> &str {
    let bytes = text.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    {
        &text[1..text.len() - 1]
    } else {
        text
    }
}

/// From the last `.` that is not escaped (`extensionIgnoringEscapes`,
/// `image_attachments.zig:1981-1992`).
fn extension_ignoring_escapes(raw: &str) -> &str {
    let bytes = raw.as_bytes();
    let mut dot = None;
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\' && index + 1 < bytes.len() {
            index += 2;
            continue;
        }
        if bytes[index] == b'.' {
            dot = Some(index);
        }
        index += 1;
    }
    dot.map_or("", |at| &raw[at..])
}

fn supported(extension: &str) -> bool {
    [".png", ".jpg", ".jpeg", ".gif", ".webp"]
        .iter()
        .any(|known| extension.eq_ignore_ascii_case(known))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split_of(raw: &str) -> Option<(String, String, bool)> {
        split(raw).map(|split| (split.path, split.suffix.to_string(), split.literal))
    }

    fn some(path: &str, suffix: &str, literal: bool) -> Option<(String, String, bool)> {
        Some((path.to_string(), suffix.to_string(), literal))
    }

    #[test]
    fn only_unquoted_sentence_punctuation_is_left_after_the_path() {
        // `image_attachments.zig:2650-2672`.
        assert_eq!(
            split_of("/tmp/photo.png,"),
            some("/tmp/photo.png", ",", false)
        );
        assert_eq!(
            split_of("@~/Desktop/test.png"),
            some("~/Desktop/test.png", "", false)
        );
        assert_eq!(
            split_of("@\"/tmp/quoted image.webp\"?!"),
            some("/tmp/quoted image.webp", "?!", true)
        );
        assert_eq!(
            split_of("\"/tmp/quoted image.webp\"?!"),
            some("\"/tmp/quoted image.webp\"", "?!", false)
        );
        assert_eq!(split_of("@@/tmp/photo.png"), None);
        assert_eq!(split_of("\"/tmp/photo.png,\""), None);
        assert_eq!(split_of("/tmp/photo.png\\,"), None);
        assert_eq!(split_of("/tmp/photo.txt,"), None);
    }

    #[test]
    fn a_canonical_at_word_is_the_files_exact_name_and_any_other_keeps_the_old_rules() {
        // `image_attachments.zig:2613-2648`: what the picker writes for each
        // name comes back as that name.
        for name in ["photo.png", ".png", "a\\b.png", "a\"b.png", " leading.png"] {
            let encoded = super::super::file_picker::encode(name, true, false, true);
            assert_eq!(split_of(&encoded), some(name, "", true), "{encoded}");
        }
        // A selected `photo.png,` is that file, which is no image.
        let selected = super::super::file_picker::encode("photo.png,", false, false, true);
        assert_eq!(selected, "@\"photo.png,\"");
        assert_eq!(split_of(&selected), None);
        for invalid in [
            "@\"photo.png\"other.png",
            "@\"photo.png",
            "@\"photo\\.png\"",
            "(@\"photo.png\")",
        ] {
            assert_eq!(split_of(invalid), None, "{invalid}");
        }
        assert_eq!(split_of("@photo.png,"), some("photo.png", ",", false));
    }

    #[test]
    fn a_path_is_known_by_its_spelling_before_the_filesystem_is_asked() {
        // `image_attachments.zig:2540-2548`.
        assert!(any("hello /tmp/CleanShot\\ 2026.png"));
        assert!(any("/tmp/photo.JPG"));
        assert!(any("/tmp/photo.JPG,"));
        assert!(any("\"/tmp/quoted image.webp\""));
        assert!(any("\"/tmp/quoted image.webp\"!"));
        assert!(any("'/tmp/quoted image.gif'"));
        assert!(!any("hello /tmp/not-image.txt"));
        assert!(!any("[Image 1] what is in this image?"));
    }

    #[test]
    fn each_word_is_found_where_it_stands_with_its_punctuation_left_out() {
        // `image_attachments.zig:2550-2576`: whitespace around and between
        // the words is the text's, and so is the comma.
        let source = " \tExplain\n  block @./first.png,\n$review @./second.png  \r\n";
        let found = find(source);
        let words: Vec<&str> = found
            .iter()
            .map(|word| &source[word.range.clone()])
            .collect();
        assert_eq!(words, ["@./first.png", "@./second.png"]);
        assert_eq!(found[0].path, "./first.png");
        let escaped = find("look this /Users/me/CleanShot\\ 2026.png now");
        assert_eq!(escaped.len(), 1);
        assert_eq!(escaped[0].path, "/Users/me/CleanShot\\ 2026.png");
        assert_eq!(escaped[0].range, 10..39);
    }

    fn workspace_with(files: &[(&str, &[u8])]) -> tempfile::TempDir {
        let root = tempfile::TempDir::new().expect("a workspace");
        for (name, bytes) in files {
            std::fs::write(root.path().join(name), bytes).expect("a file");
        }
        root
    }

    #[test]
    fn a_word_loads_as_slash_image_would_load_its_path() {
        let shot = crate::image::fixture::SHOT_PNG;
        let root = workspace_with(&[("photo.png", shot), ("CleanShot 2026.png", shot)]);
        let workspace = root.path().canonicalize().expect("real");
        for text in [
            "inspect @photo.png now",
            "look this CleanShot\\ 2026.png now",
            "look this \"CleanShot 2026.png\" now",
            "look this 'CleanShot 2026.png' now",
            "look this @\"CleanShot 2026.png\" now",
        ] {
            let word = &find(text)[0];
            let (path, media_type, bytes) = load(&workspace, None, word).expect(text);
            assert!(path.starts_with(&workspace), "{text}: {path:?}");
            assert_eq!(media_type, MediaType::Png);
            assert_eq!(bytes, shot);
        }
    }

    #[test]
    fn what_a_typed_path_that_fails_does_is_upstreams_choice_per_failure() {
        // Missing, unsupported, a directory, `~other`: left as words
        // (`image_attachments.zig:2495-2538`). Too large or outside the
        // workspace: said.
        let root = workspace_with(&[("notes.png", b"not an image")]);
        std::fs::create_dir(root.path().join("photos.png")).expect("a directory");
        let workspace = root.path().canonicalize().expect("real");
        let outcome = |text: &str| load(&workspace, None, &find(text)[0]).map(|_| ());
        for text in [
            "missing.png",
            "notes.png",
            "@photos.png",
            "~other/test.png",
            "~/test.png",
        ] {
            let problem = outcome(text).expect_err(text);
            assert!(passed_over_at_submit(&problem), "{text}: {problem:?}");
        }
        assert!(!passed_over_at_submit(&AttachError::TooLarge));
        assert!(!passed_over_at_submit(&AttachError::Other(
            "PathOutsideWorkspace"
        )));
        assert!(!passed_over_at_submit(&AttachError::Other("AccessDenied")));
    }
}
