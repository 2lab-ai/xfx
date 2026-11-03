//! What a `SKILL.md` says about itself: the frontmatter rules, byte for byte.
//!
//! A port of upstream's skill contract (`vercel-labs/fx@c1db9191
//! src/core/skills/skill_contract.zig`), because the same file has to mean the
//! same skill in both products: a name one accepts and the other refuses is a
//! skill that silently vanishes when the user switches. The rules are small and
//! deliberately strict -- a `name:` and an optional `description:` between two
//! `---` lines, a quoted value only when it is quoted on both ends, and a
//! description block only in the three forms upstream decodes (`>`, `>-`, `|`)
//! -- and everything else is a named refusal rather than a guess.
//!
//! The file is read as **bytes**: a skill directory is somebody else's text,
//! and a name that is not UTF-8 is a refusal with a cause
//! ([`InvalidCause::InvalidUtf8`]), not a panic or a lossy conversion.

use std::fs::File;
use std::io::{self, Read};

/// The most a frontmatter header may take, closing line included
/// (`max_frontmatter_bytes`, `skill_contract.zig:4`).
pub const MAX_FRONTMATTER_BYTES: usize = 64 * 1024;

/// The longest name a skill may have (`max_name_bytes`, `:5`).
pub const MAX_NAME_BYTES: usize = 256;

/// Why a candidate's metadata was refused (`InvalidMetadataCause`, `:164-175`).
///
/// The tag a diagnostic names is upstream's own (`@tagName`), so a report from
/// either product names the same cause in the same words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidCause {
    FrontmatterTooLong,
    MissingClosingDelimiter,
    MissingName,
    DuplicateRecognizedKey,
    InvalidName,
    NameTooLong,
    MalformedQuote,
    UnsupportedMultiline,
    InvalidUtf8,
    ControlByte,
}

impl InvalidCause {
    /// Upstream's name for the cause.
    pub fn tag(self) -> &'static str {
        match self {
            Self::FrontmatterTooLong => "frontmatter_too_long",
            Self::MissingClosingDelimiter => "missing_closing_delimiter",
            Self::MissingName => "missing_name",
            Self::DuplicateRecognizedKey => "duplicate_recognized_key",
            Self::InvalidName => "invalid_name",
            Self::NameTooLong => "name_too_long",
            Self::MalformedQuote => "malformed_quote",
            Self::UnsupportedMultiline => "unsupported_multiline",
            Self::InvalidUtf8 => "invalid_utf8",
            Self::ControlByte => "control_byte",
        }
    }
}

/// What the frontmatter said about the file as a whole (`MetadataStatus`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// The file does not open with `---`: its directory's name is its name.
    NoFrontmatter,
    Valid,
    Invalid(InvalidCause),
}

/// One of the three description blocks upstream decodes
/// (`BlockDescriptionStyle`, `:183-187`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlockStyle {
    /// `>`: lines folded into one, a trailing newline kept.
    FoldedClip,
    /// `>-`: folded, no trailing newline.
    FoldedStrip,
    /// `|`: lines kept, a trailing newline kept.
    LiteralClip,
}

/// A description written as a block, still in its raw lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Block {
    style: BlockStyle,
    base_indent: usize,
}

/// One file's frontmatter, read (`ParsedSkillFile`, `:201-207`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed<'a> {
    name: Option<&'a [u8]>,
    description: Option<&'a [u8]>,
    block: Option<Block>,
    /// What follows the closing line, leading line breaks dropped -- or the
    /// whole file when there is no frontmatter to close.
    pub body: &'a [u8],
    pub status: Status,
}

/// A skill's name and description, decoded (`SkillMetadata`, `:209-226`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metadata {
    pub name: String,
    pub description: String,
}

/// Reads one file's frontmatter (`parseSkillFile`, `:248-351`).
pub fn parse(content: &[u8]) -> Parsed<'_> {
    let Some(header_start) = header_start(content) else {
        return Parsed {
            name: None,
            description: None,
            block: None,
            body: content,
            status: Status::NoFrontmatter,
        };
    };
    let bounded = &content[..content.len().min(MAX_FRONTMATTER_BYTES + 1)];
    let Some((header_end, body_start)) = closing_delimiter(bounded, header_start) else {
        let cause = if content.len() > MAX_FRONTMATTER_BYTES {
            InvalidCause::FrontmatterTooLong
        } else {
            InvalidCause::MissingClosingDelimiter
        };
        return Parsed {
            name: None,
            description: None,
            block: None,
            body: content,
            status: Status::Invalid(cause),
        };
    };
    if body_start > MAX_FRONTMATTER_BYTES {
        return Parsed {
            name: None,
            description: None,
            block: None,
            body: content,
            status: Status::Invalid(InvalidCause::FrontmatterTooLong),
        };
    }

    let header = &content[header_start..header_end];
    let body = trim_start(&content[body_start..], b"\r\n");

    let mut name: Option<&[u8]> = None;
    let mut description: Option<&[u8]> = None;
    let mut block: Option<Block> = None;
    let mut cause: Option<InvalidCause> = None;
    let mut saw_name = false;
    let mut saw_description = false;
    let mut previous_recognized = false;

    let mut offset = 0usize;
    while let Some(line) = header_line(header, offset) {
        offset = line.next;
        let trimmed = trim(line.bytes, b" \t");
        if trimmed.is_empty() || trimmed[0] == b'#' {
            continue;
        }
        if previous_recognized && matches!(line.bytes[0], b' ' | b'\t') {
            first(&mut cause, InvalidCause::UnsupportedMultiline);
            previous_recognized = false;
            continue;
        }
        let Some(colon) = trimmed.iter().position(|byte| *byte == b':') else {
            previous_recognized = false;
            continue;
        };
        let key = trim(&trimmed[..colon], b" \t");
        let raw = trim(&trimmed[colon + 1..], b" \t");
        if key == b"name" {
            previous_recognized = true;
            if saw_name {
                first(&mut cause, InvalidCause::DuplicateRecognizedKey);
            }
            saw_name = true;
            let (value, invalid) = recognized_value(raw);
            name = Some(value);
            if let Some(invalid) = invalid {
                first(&mut cause, invalid);
            }
        } else if key == b"description" {
            if saw_description {
                first(&mut cause, InvalidCause::DuplicateRecognizedKey);
            }
            saw_description = true;
            if let Some(style) = block_style(raw) {
                let parsed = block_description(header, offset, style);
                description = Some(parsed.value);
                block = parsed.block;
                offset = parsed.next;
                previous_recognized = false;
                if let Some(invalid) = parsed.cause {
                    first(&mut cause, invalid);
                }
            } else {
                previous_recognized = true;
                let (value, invalid) = recognized_value(raw);
                description = Some(value);
                block = None;
                if let Some(invalid) = invalid {
                    first(&mut cause, invalid);
                }
            }
        } else {
            previous_recognized = false;
        }
    }

    match name {
        Some(value) => {
            if let Some(invalid) = invalid_name_cause(value) {
                first(&mut cause, invalid);
            }
        }
        None => first(&mut cause, InvalidCause::MissingName),
    }
    if block.is_none() {
        if let Some(value) = description {
            if let Some(invalid) = invalid_text_cause(value) {
                first(&mut cause, invalid);
            }
        }
    }
    if cause.is_some() {
        block = None;
    }
    Parsed {
        name,
        description,
        block,
        body,
        status: cause.map_or(Status::Valid, Status::Invalid),
    }
}

/// The name and description a parsed file stands for, with `fallback` -- its
/// directory's name -- as the name of a file with no frontmatter
/// (`resolveMetadata`, `:233-246`).
pub fn resolve(parsed: &Parsed<'_>, fallback: &str) -> Result<Metadata, InvalidCause> {
    match parsed.status {
        Status::NoFrontmatter => match invalid_name_cause(fallback.as_bytes()) {
            Some(cause) => Err(cause),
            None => Ok(Metadata {
                name: fallback.to_string(),
                description: String::new(),
            }),
        },
        Status::Invalid(cause) => Err(cause),
        Status::Valid => {
            // Both were checked to be UTF-8 without control bytes on the way
            // to `Valid` -- a block's lines one by one -- so the conversions
            // below cannot fail; they are spelled as fallible rather than
            // trusted so a future rule that loosens the check cannot panic.
            let name = std::str::from_utf8(parsed.name.unwrap_or_default())
                .map_err(|_| InvalidCause::InvalidUtf8)?
                .to_string();
            let raw = parsed.description.unwrap_or_default();
            let description = match parsed.block {
                Some(block) => decode_block(raw, block),
                None => raw.to_vec(),
            };
            let description =
                String::from_utf8(description).map_err(|_| InvalidCause::InvalidUtf8)?;
            Ok(Metadata { name, description })
        }
    }
}

/// Why `name` cannot be a skill's name, if it cannot (`invalidSkillNameCause`,
/// `:648-654`).
pub fn invalid_name_cause(name: &[u8]) -> Option<InvalidCause> {
    if name.is_empty() {
        return Some(InvalidCause::MissingName);
    }
    if name.len() > MAX_NAME_BYTES {
        return Some(InvalidCause::NameTooLong);
    }
    if let Some(cause) = invalid_text_cause(name) {
        return Some(cause);
    }
    let name = std::str::from_utf8(name).ok()?;
    (!is_managed_name(name)).then_some(InvalidCause::InvalidName)
}

/// Whether `name` can be one directory under the managed install root: not
/// empty, not too long, not `.` or `..`, not absolute, and no `/` or `\`
/// (`validateManagedSkillName`, `:656-662`).
pub fn is_managed_name(name: &str) -> bool {
    !(name.is_empty()
        || name.len() > MAX_NAME_BYTES
        || name == "."
        || name == ".."
        || name.starts_with('/')
        || name.contains('/')
        || name.contains('\\'))
}

/// What a candidate's metadata is read from: its first bytes, up to one past
/// the header cap (`readMetadataPrefix`, `:560-590`).
///
/// `Ok(None)` is upstream's `StreamTooLong`: a file that opens a header and
/// has not closed it inside the cap, with more of it left -- a candidate
/// skipped as oversized rather than parsed as a header with no end.
pub fn read_metadata_prefix(file: &mut File, size: u64) -> io::Result<Option<Vec<u8>>> {
    let readable = usize::try_from(size)
        .unwrap_or(usize::MAX)
        .min(MAX_FRONTMATTER_BYTES + 1);
    let mut content = Vec::with_capacity(readable);
    file.take(readable as u64).read_to_end(&mut content)?;
    if content.len() < readable {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "SKILL.md ended before its recorded size",
        ));
    }
    let complete = content.len() as u64 >= size;
    if let Some(start) = header_start(&content) {
        if closing_delimiter(&content, start).is_none() && !complete {
            return Ok(None);
        }
    }
    Ok(Some(content))
}

/// Where the header begins, past its opening line (`frontmatterHeaderStart`,
/// `:518-523`).
fn header_start(content: &[u8]) -> Option<usize> {
    if content.starts_with(b"---\r\n") {
        Some(5)
    } else if content.starts_with(b"---\n") {
        Some(4)
    } else if content == b"---" {
        Some(3)
    } else {
        None
    }
}

/// Where the header ends and the body begins: the first line that is exactly
/// `---`, a CR before its break allowed (`findClosingDelimiter`, `:592-613`).
fn closing_delimiter(content: &[u8], start: usize) -> Option<(usize, usize)> {
    let mut line_start = start;
    while line_start <= content.len() {
        let newline = content[line_start..].iter().position(|byte| *byte == b'\n');
        let line_end = newline.map_or(content.len(), |offset| line_start + offset);
        let raw = &content[line_start..line_end];
        let line = if newline.is_some() && raw.last() == Some(&b'\r') {
            &raw[..raw.len() - 1]
        } else {
            raw
        };
        if line == b"---" {
            let body = if newline.is_some() {
                line_end + 1
            } else {
                line_end
            };
            return Some((line_start, body));
        }
        newline?;
        line_start = line_end + 1;
    }
    None
}

/// One header line, its break and a CR before it dropped (`headerLineAt`).
struct HeaderLine<'a> {
    start: usize,
    bytes: &'a [u8],
    next: usize,
}

fn header_line(header: &[u8], start: usize) -> Option<HeaderLine<'_>> {
    if start >= header.len() {
        return None;
    }
    let newline = header[start..].iter().position(|byte| *byte == b'\n');
    let end = newline.map_or(header.len(), |offset| start + offset);
    let raw = &header[start..end];
    let bytes = if raw.last() == Some(&b'\r') {
        &raw[..raw.len() - 1]
    } else {
        raw
    };
    Some(HeaderLine {
        start,
        bytes,
        next: if newline.is_some() { end + 1 } else { end },
    })
}

/// A single-line value: unquoted, or quoted on both ends with the same quote
/// (`parseRecognizedValue`, `:620-634`).
fn recognized_value(value: &[u8]) -> (&[u8], Option<InvalidCause>) {
    if matches!(value.first(), Some(b'|' | b'>')) {
        return (value, Some(InvalidCause::UnsupportedMultiline));
    }
    let quote = |byte: Option<&u8>| matches!(byte, Some(b'\'' | b'"'));
    if quote(value.first()) || quote(value.last()) {
        if value.len() >= 2 && value[0] == value[value.len() - 1] {
            return (&value[1..value.len() - 1], None);
        }
        return (value, Some(InvalidCause::MalformedQuote));
    }
    (value, None)
}

/// Whether a value is text a model may be shown: UTF-8 with no control byte
/// (`invalidTextCause`, `:640-646`).
fn invalid_text_cause(value: &[u8]) -> Option<InvalidCause> {
    if std::str::from_utf8(value).is_err() {
        return Some(InvalidCause::InvalidUtf8);
    }
    value
        .iter()
        .any(|byte| *byte < 0x20 || *byte == 0x7f)
        .then_some(InvalidCause::ControlByte)
}

fn block_style(value: &[u8]) -> Option<BlockStyle> {
    match value {
        b">" => Some(BlockStyle::FoldedClip),
        b">-" => Some(BlockStyle::FoldedStrip),
        b"|" => Some(BlockStyle::LiteralClip),
        _ => None,
    }
}

/// A description block, read from the line after its key
/// (`parse_block_description`, `:382-440`).
struct ParsedBlock<'a> {
    value: &'a [u8],
    block: Option<Block>,
    next: usize,
    cause: Option<InvalidCause>,
}

fn block_description(header: &[u8], start: usize, style: BlockStyle) -> ParsedBlock<'_> {
    let mut base: Option<usize> = None;
    let mut cause: Option<InvalidCause> = None;
    let mut decodable = true;
    let mut offset = start;
    let mut end = header.len();
    while let Some(line) = header_line(header, offset) {
        offset = line.next;
        if is_blank(line.bytes) {
            continue;
        }
        let indent = leading_spaces(line.bytes);
        if indent == 0 {
            if line.bytes[0] == b'\t' {
                first(&mut cause, InvalidCause::UnsupportedMultiline);
                decodable = false;
                continue;
            }
            end = line.start;
            offset = line.start;
            break;
        }
        if indent < line.bytes.len() && line.bytes[indent] == b'\t' {
            first(&mut cause, InvalidCause::UnsupportedMultiline);
            decodable = false;
            continue;
        }
        match base {
            Some(established) if indent < established => {
                first(&mut cause, InvalidCause::UnsupportedMultiline);
                decodable = false;
                continue;
            }
            Some(_) => {}
            None => base = Some(indent),
        }
        let content = &line.bytes[base.unwrap_or(0)..];
        if let Some(invalid) = invalid_text_cause(content) {
            first(&mut cause, invalid);
        }
    }
    ParsedBlock {
        value: &header[start..end],
        block: decodable.then_some(Block {
            style,
            base_indent: base.unwrap_or(0),
        }),
        next: offset,
        cause,
    }
}

/// A block's text, folded or kept as its style says
/// (`decode_block_description`, `:473-511`).
fn decode_block(raw: &[u8], block: Block) -> Vec<u8> {
    let mut last_nonblank_next = 0usize;
    let mut offset = 0usize;
    while let Some(line) = header_line(raw, offset) {
        offset = line.next;
        if !is_blank(line.bytes) {
            last_nonblank_next = line.next;
        }
    }
    if last_nonblank_next == 0 {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(raw.len());
    let mut previous_nonblank = false;
    let mut first_line = true;
    offset = 0;
    while offset < last_nonblank_next {
        let Some(line) = header_line(raw, offset) else {
            break;
        };
        offset = line.next;
        let nonblank = !is_blank(line.bytes);
        if !first_line {
            let separator: &[u8] = match block.style {
                BlockStyle::LiteralClip => b"\n",
                BlockStyle::FoldedClip | BlockStyle::FoldedStrip => {
                    if previous_nonblank && nonblank {
                        b" "
                    } else {
                        b"\n"
                    }
                }
            };
            out.extend_from_slice(separator);
        }
        if nonblank {
            out.extend_from_slice(&line.bytes[block.base_indent.min(line.bytes.len())..]);
        }
        previous_nonblank = nonblank;
        first_line = false;
    }
    match block.style {
        BlockStyle::FoldedClip | BlockStyle::LiteralClip => out.push(b'\n'),
        BlockStyle::FoldedStrip => {}
    }
    out
}

fn is_blank(line: &[u8]) -> bool {
    trim(line, b" \t").is_empty()
}

fn leading_spaces(line: &[u8]) -> usize {
    line.iter().take_while(|byte| **byte == b' ').count()
}

fn first(slot: &mut Option<InvalidCause>, cause: InvalidCause) {
    if slot.is_none() {
        *slot = Some(cause);
    }
}

fn trim<'a>(bytes: &'a [u8], set: &[u8]) -> &'a [u8] {
    let start = bytes
        .iter()
        .position(|byte| !set.contains(byte))
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|byte| !set.contains(byte))
        .map_or(start, |last| last + 1);
    &bytes[start..end.max(start)]
}

fn trim_start<'a>(bytes: &'a [u8], set: &[u8]) -> &'a [u8] {
    let start = bytes
        .iter()
        .position(|byte| !set.contains(byte))
        .unwrap_or(bytes.len());
    &bytes[start..]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolved(content: &str) -> Result<Metadata, InvalidCause> {
        resolve(&parse(content.as_bytes()), "fallback")
    }

    #[test]
    fn a_full_frontmatter_names_the_skill_and_the_body_follows_it() {
        // Upstream's own case (`skill_contract.zig` "parseSkillFile with full
        // frontmatter").
        let content = "---\nname: my-skill\ndescription: Helps with testing\n---\n\n# My Skill\n\nDo the thing.";
        let parsed = parse(content.as_bytes());
        assert_eq!(parsed.status, Status::Valid);
        assert_eq!(parsed.body, b"# My Skill\n\nDo the thing.");
        assert_eq!(
            resolved(content).unwrap(),
            Metadata {
                name: "my-skill".to_string(),
                description: "Helps with testing".to_string(),
            }
        );
    }

    #[test]
    fn description_blocks_fold_keep_and_strip_as_upstreams() {
        // `description blocks preserve folded literal and trailing newline
        // semantics`, upstream's three forms with its expected bytes.
        let cases = [
            (
                "---\nname: folded\ndescription: >\n  Fold this\n  onto one line.\n\n  Keep this paragraph.\n---\nBody",
                "Fold this onto one line.\n\nKeep this paragraph.\n",
            ),
            (
                "---\nname: folded-strip\ndescription: >-\n  Fold without\n  a trailing newline.\n---\nBody",
                "Fold without a trailing newline.",
            ),
            (
                "---\nname: literal\ndescription: |\n  Keep this\n  on two lines.\n---\nBody",
                "Keep this\non two lines.\n",
            ),
        ];
        for (content, expected) in cases {
            assert_eq!(
                resolved(content).unwrap().description,
                expected,
                "{content}"
            );
        }
    }

    #[test]
    fn a_file_without_frontmatter_is_named_by_its_directory() {
        let parsed = parse(b"# Just instructions\n");
        assert_eq!(parsed.status, Status::NoFrontmatter);
        assert_eq!(
            resolve(&parsed, "dir-name").unwrap(),
            Metadata {
                name: "dir-name".to_string(),
                description: String::new(),
            }
        );
        assert_eq!(resolve(&parsed, ".."), Err(InvalidCause::InvalidName));
    }

    #[test]
    fn every_refusal_names_its_first_cause() {
        let cases = [
            ("---\nname: x\n", InvalidCause::MissingClosingDelimiter),
            ("---\ndescription: d\n---\n", InvalidCause::MissingName),
            (
                "---\nname: a\nname: b\n---\n",
                InvalidCause::DuplicateRecognizedKey,
            ),
            ("---\nname: a/b\n---\n", InvalidCause::InvalidName),
            ("---\nname: 'a\n---\n", InvalidCause::MalformedQuote),
            (
                "---\nname: a\n  more\n---\n",
                InvalidCause::UnsupportedMultiline,
            ),
            ("---\nname: |\n---\n", InvalidCause::UnsupportedMultiline),
            (
                "---\nname: a\ndescription: x\u{7f}y\n---\n",
                InvalidCause::ControlByte,
            ),
        ];
        for (content, cause) in cases {
            assert_eq!(resolved(content), Err(cause), "{content:?}");
        }
        let long = format!("---\nname: {}\n---\n", "a".repeat(MAX_NAME_BYTES + 1));
        assert_eq!(resolved(&long), Err(InvalidCause::NameTooLong));
        let invalid = b"---\nname: a\xff\n---\n";
        assert_eq!(
            resolve(&parse(invalid), "x"),
            Err(InvalidCause::InvalidUtf8)
        );
    }

    #[test]
    fn quoted_values_lose_their_quotes_and_comments_are_skipped() {
        let content =
            "---\n# a comment\nname: \"quoted\"\ndescription: 'it''s'\nother: kept out\n---\n";
        assert_eq!(
            resolved(content).unwrap(),
            Metadata {
                name: "quoted".to_string(),
                description: "it''s".to_string(),
            }
        );
    }

    #[test]
    fn a_crlf_file_parses_as_its_lf_twin() {
        let content = "---\r\nname: crlf\r\ndescription: Windows\r\n---\r\n\r\nBody\r\n";
        let parsed = parse(content.as_bytes());
        assert_eq!(parsed.status, Status::Valid);
        assert_eq!(parsed.body, b"Body\r\n");
        assert_eq!(resolved(content).unwrap().description, "Windows");
    }

    #[test]
    fn a_header_past_the_cap_is_too_long() {
        let content = format!(
            "---\nname: big\ndescription: {}\n---\n",
            "x".repeat(MAX_FRONTMATTER_BYTES)
        );
        assert_eq!(resolved(&content), Err(InvalidCause::FrontmatterTooLong));
    }

    #[test]
    fn managed_names_are_one_directory() {
        for good in ["demo", "release-notes", "a.b", "x_y"] {
            assert!(is_managed_name(good), "{good}");
        }
        for bad in ["", ".", "..", "/abs", "a/b", "a\\b", "../escape"] {
            assert!(!is_managed_name(bad), "{bad}");
        }
    }

    #[test]
    fn an_unclosed_header_bigger_than_the_cap_is_oversized_not_parsed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("SKILL.md");
        let text = format!("---\nname: x\n{}", "y".repeat(MAX_FRONTMATTER_BYTES + 10));
        std::fs::write(&path, &text).unwrap();
        let mut file = File::open(&path).unwrap();
        assert_eq!(
            read_metadata_prefix(&mut file, text.len() as u64).unwrap(),
            None
        );
        let small = "---\nname: x\n---\nbody";
        std::fs::write(&path, small).unwrap();
        let mut file = File::open(&path).unwrap();
        assert_eq!(
            read_metadata_prefix(&mut file, small.len() as u64)
                .unwrap()
                .as_deref(),
            Some(small.as_bytes())
        );
    }
}
