//! The four read-only tools: `list_files`, `glob_files`, `grep_files`, and
//! `read_file`.
//!
//! Every one of them obeys the same three rules, and the rules are the reason
//! these are safe to run without asking:
//!
//! - **Nothing is opened before it is proven in scope.** A path becomes a
//!   [`crate::workspace::ResolvedPath`] first; only then is it read. A walk
//!   never follows a symlink, so a link inside the workspace cannot be used to
//!   walk out of it.
//! - **Every output is bounded and says where it stopped.** A truncated read, a
//!   capped listing, and a windowed search each carry an explicit sentence
//!   saying what was left out and how to ask for it. Silent truncation would
//!   teach the model that it had seen a whole file.
//! - **Every output is deterministic.** Entries and matches are sorted before
//!   they are capped, so the same tree produces the same bytes and a cap keeps
//!   the first N rather than an arbitrary N.
//!
//! Formats mirror upstream so a model prompted for `fx` reads xfx's results the
//! same way (`vercel-labs/fx@580a0c5d src/tools/filesystem/read_file.zig:324-372`,
//! `list_files.zig:80-115`, `glob_files.zig:200-245`, `grep_files.zig:303-530`).

use std::fs;
use std::path::{Path, PathBuf};

use globset::{Glob, GlobMatcher};
use ignore::WalkBuilder;
use serde_json::Value;

use crate::workspace::{is_ignored_directory, PathError};

use super::spec::{
    clip, nonblank, object, optional_bool, optional_enum, optional_integer, optional_string,
    required_string, InputSchema, PermissionKind, Property, PropertyKind, ToolContext, ToolInput,
    ToolLimits, ToolResult, ToolSpec,
};

/// Appended to a line that was clipped by `read_file`
/// (`read_file.zig:19`).
const LINE_TRUNCATED_SUFFIX: &str = "... (line truncated)";

// ---------------------------------------------------------------------------
// descriptions
// ---------------------------------------------------------------------------
//
// These are xfx's own words, in upstream's shape: what the tool does, when to
// use it, when not to. They describe what *this* build does -- there is no
// mention of `~`, of external paths, or of a permission prompt, because xfx
// reads the primary workspace and explicitly added directories and nothing
// else.

const LIST_FILES_DESCRIPTION: &str = "List the entries of one directory, one level deep, without reading file contents. Paths are relative to the workspace root, or absolute inside an authorized root; anything else is refused. Directories end in /, symlinks in @. Entries named .git, node_modules, dist, build, coverage, .next, zig-out, or .zig-cache are always omitted; everything else, including dotfiles, is listed. When to use: inspect a known folder, confirm a name, or choose the next path to read. When NOT to use: recursive discovery, content search, or a shell ls.";

const GLOB_FILES_DESCRIPTION: &str = "Find file paths matching a glob pattern below one directory, with mode=count for an exact count without listing. Paths are relative to the workspace root, or absolute inside an authorized root. The search does not see everything: symlinks are not followed, build directories such as .git and node_modules are pruned, paths excluded by .gitignore are skipped, and hidden dot-paths are skipped unless the pattern itself names one (for example .github/**/*.yml). Results are sorted, and a capped or incomplete search says so on its own ... line. When to use: locate files by name, extension, or directory shape. When NOT to use: search file contents, read a file, or count non-file things.";

const GREP_FILES_DESCRIPTION: &str = "Search text files for a literal substring, optionally narrowed by path and include glob, with modes for matching lines, files with matches, or counts, plus head_limit/offset paging and bounded context_lines. Paths are relative to the workspace root, or absolute inside an authorized root. Regular expressions are not supported: the pattern is matched literally. The search does not see every file: symlinks are not followed, build directories such as .git and node_modules are pruned, paths excluded by .gitignore are skipped, hidden dot-paths are skipped unless the include glob itself names one, and files that are not UTF-8 text or are above the size cap are not searched. Any file skipped for the last two reasons is counted on a ... skipped line, so no matches means no matches among the files actually searched. When to use: find an exact symbol, string, or usage site. When NOT to use: filename lookup, reading a known path, or regex search.";

const READ_FILE_DESCRIPTION: &str = "Read one UTF-8 text file as bounded, line-numbered output, with an optional start_line/line_count range. Paths are relative to the workspace root, or absolute inside an authorized root. Output states how many of the file's lines it showed, so a partial read is never mistaken for the whole file. When to use: inspect an exact known path. When NOT to use: list a directory, search many files, or read binary data.";

const PATH_DESCRIPTION: &str =
    "Path relative to the workspace root, or an absolute path inside an authorized root.";

// ---------------------------------------------------------------------------
// specs
// ---------------------------------------------------------------------------

pub const LIST_FILES: ToolSpec = ToolSpec::new(
    "list_files",
    LIST_FILES_DESCRIPTION,
    PermissionKind::ReadOnly,
    InputSchema {
        properties: &[Property {
            name: "path",
            kind: PropertyKind::String,
            description: "Directory to list. Defaults to the workspace root.",
            allowed: &[],
        }],
        required: &[],
    },
    decode_list_files,
    validate_list_files,
    execute_list_files,
);

pub const GLOB_FILES: ToolSpec = ToolSpec::new(
    "glob_files",
    GLOB_FILES_DESCRIPTION,
    PermissionKind::ReadOnly,
    InputSchema {
        properties: &[
            Property {
                name: "pattern",
                kind: PropertyKind::String,
                description: "Glob pattern to match, such as src/**/*.rs or *.md.",
                allowed: &[],
            },
            Property {
                name: "path",
                kind: PropertyKind::String,
                description: "Directory to search below. Defaults to the workspace root.",
                allowed: &[],
            },
            Property {
                name: "mode",
                kind: PropertyKind::String,
                description:
                    "Use matches to list paths, or count for an exact count without listing.",
                allowed: &["matches", "count"],
            },
        ],
        required: &["pattern"],
    },
    decode_glob_files,
    validate_glob_files,
    execute_glob_files,
);

pub const GREP_FILES: ToolSpec = ToolSpec::new(
    "grep_files",
    GREP_FILES_DESCRIPTION,
    PermissionKind::ReadOnly,
    InputSchema {
        properties: &[
            Property {
                name: "pattern",
                kind: PropertyKind::String,
                description: "Literal substring to search for. Not a regular expression.",
                allowed: &[],
            },
            Property {
                name: "path",
                kind: PropertyKind::String,
                description: "Directory to search below. Defaults to the workspace root.",
                allowed: &[],
            },
            Property {
                name: "include",
                kind: PropertyKind::String,
                description:
                    "Glob applied to candidate paths before any file is read, such as *.rs.",
                allowed: &[],
            },
            Property {
                name: "case_insensitive",
                kind: PropertyKind::Boolean,
                description: "Match without regard to case.",
                allowed: &[],
            },
            Property {
                name: "mode",
                kind: PropertyKind::String,
                description: "Use matches for lines, files_with_matches for paths, or count for exact counts.",
                allowed: &["matches", "files_with_matches", "count"],
            },
            Property {
                name: "head_limit",
                kind: PropertyKind::Integer,
                description: "Positive maximum results to return. Defaults to the output cap.",
                allowed: &[],
            },
            Property {
                name: "offset",
                kind: PropertyKind::Integer,
                description: "Zero-based result offset for paging. Defaults to 0.",
                allowed: &[],
            },
            Property {
                name: "context_lines",
                kind: PropertyKind::Integer,
                description: "Lines to show before and after each match. Bounded by the tool.",
                allowed: &[],
            },
        ],
        required: &["pattern"],
    },
    decode_grep_files,
    validate_grep_files,
    execute_grep_files,
);

pub const READ_FILE: ToolSpec = ToolSpec::new(
    "read_file",
    READ_FILE_DESCRIPTION,
    PermissionKind::ReadOnly,
    InputSchema {
        properties: &[
            Property {
                name: "path",
                kind: PropertyKind::String,
                description: PATH_DESCRIPTION,
                allowed: &[],
            },
            Property {
                name: "start_line",
                kind: PropertyKind::Integer,
                description: "1-based first line to return. Defaults to 1.",
                allowed: &[],
            },
            Property {
                name: "line_count",
                kind: PropertyKind::Integer,
                description: "Positive number of lines to return. Defaults to the read cap.",
                allowed: &[],
            },
        ],
        required: &["path"],
    },
    decode_read_file,
    validate_read_file,
    execute_read_file,
);

// ---------------------------------------------------------------------------
// list_files
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListFilesInput {
    pub path: Option<String>,
}

fn decode_list_files(input: &Value) -> Result<ToolInput, String> {
    let object = object("list_files", input)?;
    Ok(ToolInput::ListFiles(ListFilesInput {
        path: optional_string("list_files", object, "path")?,
    }))
}

fn validate_list_files(input: &ToolInput) -> Result<(), String> {
    let ToolInput::ListFiles(input) = input else {
        return Err(mismatched("list_files"));
    };
    if let Some(path) = &input.path {
        nonblank("list_files", "path", path)?;
    }
    Ok(())
}

fn execute_list_files(input: &ToolInput, context: &ToolContext) -> ToolResult {
    let ToolInput::ListFiles(input) = input else {
        return ToolResult::failure(mismatched("list_files"));
    };
    let requested = input.path.as_deref().unwrap_or(".");
    let resolved = match context.scope().resolve_existing(requested) {
        Ok(resolved) => resolved,
        Err(err) => return refusal("list_files", err),
    };
    let display = context.scope().display_path(resolved.absolute());

    let entries = match fs::read_dir(resolved.absolute()) {
        Ok(entries) => entries,
        Err(err) => {
            return ToolResult::failure(format!("list_files cannot read `{display}`: {err}"))
        }
    };

    let mut visible: Vec<(String, &'static str)> = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                return ToolResult::failure(format!("list_files cannot read `{display}`: {err}"))
            }
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        if is_ignored_directory(&name) {
            continue;
        }
        // `file_type` here does not follow the link, so a symlink is reported
        // as a symlink rather than as whatever it points at.
        let suffix = match entry.file_type() {
            Ok(kind) if kind.is_symlink() => "@",
            Ok(kind) if kind.is_dir() => "/",
            _ => "",
        };
        visible.push((name, suffix));
    }
    // Sorted before it is capped: the cap must keep the first N names, not N
    // names in whatever order the filesystem happened to return.
    visible.sort();

    let limit = context.limits().max_list_entries;
    let truncated = visible.len() > limit;
    visible.truncate(limit);

    let mut out = format!("{display}:\n");
    for (name, suffix) in &visible {
        out.push_str(&format!("- {name}{suffix}\n"));
    }
    if visible.is_empty() {
        out.push_str("(empty)\n");
    } else if truncated {
        out.push_str(&format!("... and more entries (showing first {limit})\n"));
    }
    let detail = format!("{display} ({} entries)", visible.len());
    ToolResult::success(out, detail)
}

// ---------------------------------------------------------------------------
// glob_files
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobFilesInput {
    pub pattern: String,
    pub path: Option<String>,
    pub count_only: bool,
}

fn decode_glob_files(input: &Value) -> Result<ToolInput, String> {
    let object = object("glob_files", input)?;
    Ok(ToolInput::GlobFiles(GlobFilesInput {
        pattern: required_string("glob_files", object, "pattern")?,
        path: optional_string("glob_files", object, "path")?,
        count_only: optional_enum("glob_files", object, "mode", &["matches", "count"])?
            .is_some_and(|mode| mode == "count"),
    }))
}

fn validate_glob_files(input: &ToolInput) -> Result<(), String> {
    let ToolInput::GlobFiles(input) = input else {
        return Err(mismatched("glob_files"));
    };
    nonblank("glob_files", "pattern", &input.pattern)?;
    if let Some(path) = &input.path {
        nonblank("glob_files", "path", path)?;
    }
    Ok(())
}

fn execute_glob_files(input: &ToolInput, context: &ToolContext) -> ToolResult {
    let ToolInput::GlobFiles(input) = input else {
        return ToolResult::failure(mismatched("glob_files"));
    };
    let matcher = match compile_glob("glob_files", "pattern", &input.pattern) {
        Ok(matcher) => matcher,
        Err(reason) => return ToolResult::failure(reason),
    };
    let root = match search_root("glob_files", context, input.path.as_deref()) {
        Ok(root) => root,
        Err(result) => return result,
    };

    let walk = walk_files(&root, context.limits(), wants_hidden(&input.pattern));

    let mut matches: Vec<String> = Vec::new();
    for candidate in &walk.files {
        let relative = relative_to(&root, candidate);
        if !glob_matches(&matcher, &relative) {
            continue;
        }
        matches.push(context.scope().display_path(candidate));
    }
    matches.sort();

    let pattern = &input.pattern;
    let mut out = if input.count_only {
        format!("[glob] count {} matches for {pattern}\n", matches.len())
    } else if matches.is_empty() {
        format!("[glob] no matches for {pattern}\n")
    } else {
        let limit = context.limits().max_glob_matches;
        let truncated = matches.len() > limit;
        let shown = &matches[..matches.len().min(limit)];
        let mut listed = format!("[glob] {} matches for {pattern}\n", shown.len());
        for path in shown {
            listed.push_str(&format!(" - {path}\n"));
        }
        if truncated {
            listed.push_str(&format!("... truncated to first {limit} matches\n"));
        }
        listed
    };
    out.push_str(&walk.notes(context.limits()));

    let detail = format!("{} matches for {pattern}", matches.len());
    ToolResult::success(out, detail)
}

// ---------------------------------------------------------------------------
// grep_files
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrepMode {
    Matches,
    FilesWithMatches,
    Count,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrepFilesInput {
    pub pattern: String,
    pub path: Option<String>,
    pub include: Option<String>,
    pub case_insensitive: bool,
    pub mode: GrepMode,
    pub head_limit: Option<usize>,
    pub offset: usize,
    pub context_lines: usize,
}

fn decode_grep_files(input: &Value) -> Result<ToolInput, String> {
    let object = object("grep_files", input)?;
    let mode = match optional_enum(
        "grep_files",
        object,
        "mode",
        &["matches", "files_with_matches", "count"],
    )?
    .as_deref()
    {
        Some("files_with_matches") => GrepMode::FilesWithMatches,
        Some("count") => GrepMode::Count,
        _ => GrepMode::Matches,
    };
    Ok(ToolInput::GrepFiles(GrepFilesInput {
        pattern: required_string("grep_files", object, "pattern")?,
        path: optional_string("grep_files", object, "path")?,
        include: optional_string("grep_files", object, "include")?,
        case_insensitive: optional_bool("grep_files", object, "case_insensitive")?,
        mode,
        head_limit: optional_integer("grep_files", object, "head_limit", 1)?,
        offset: optional_integer("grep_files", object, "offset", 0)?.unwrap_or(0),
        context_lines: optional_integer("grep_files", object, "context_lines", 0)?.unwrap_or(0),
    }))
}

fn validate_grep_files(input: &ToolInput) -> Result<(), String> {
    let ToolInput::GrepFiles(input) = input else {
        return Err(mismatched("grep_files"));
    };
    nonblank("grep_files", "pattern", &input.pattern)?;
    if let Some(path) = &input.path {
        nonblank("grep_files", "path", path)?;
    }
    if let Some(include) = &input.include {
        nonblank("grep_files", "include", include)?;
    }
    Ok(())
}

/// One matching line.
struct GrepMatch {
    path: String,
    absolute: PathBuf,
    line_number: usize,
    line: String,
}

fn execute_grep_files(input: &ToolInput, context: &ToolContext) -> ToolResult {
    let ToolInput::GrepFiles(input) = input else {
        return ToolResult::failure(mismatched("grep_files"));
    };
    let include = match input.include.as_deref() {
        Some(pattern) => match compile_glob("grep_files", "include", pattern) {
            Ok(matcher) => Some(matcher),
            Err(reason) => return ToolResult::failure(reason),
        },
        None => None,
    };
    let root = match search_root("grep_files", context, input.path.as_deref()) {
        Ok(root) => root,
        Err(result) => return result,
    };

    let hidden = input.include.as_deref().is_some_and(wants_hidden);
    let walk = walk_files(&root, context.limits(), hidden);

    let limits = context.limits();
    let mut candidates: Vec<(String, PathBuf)> = walk
        .files
        .iter()
        .filter(|candidate| match &include {
            Some(matcher) => glob_matches(matcher, &relative_to(&root, candidate)),
            None => true,
        })
        .map(|candidate| {
            (
                context.scope().display_path(candidate),
                candidate.to_path_buf(),
            )
        })
        .collect();
    candidates.sort_by(|a, b| a.0.cmp(&b.0));

    let needle = if input.case_insensitive {
        input.pattern.to_lowercase()
    } else {
        input.pattern.clone()
    };

    let mut matches: Vec<GrepMatch> = Vec::new();
    let mut matching_files = 0usize;
    let mut scan_capped = false;
    let mut skipped = ScanSkips::default();
    'files: for (display, absolute) in &candidates {
        let text = match read_searchable(absolute, limits.max_grep_file_bytes) {
            FileText::Text(text) => text,
            // Counted rather than dropped. A file the search could not read is
            // the difference between "there is no match" and "there is no match
            // in what I looked at", and only the caller can tell which one
            // matters.
            FileText::Unsearchable => {
                skipped.unsearchable += 1;
                continue;
            }
            FileText::Unreadable => {
                skipped.unreadable += 1;
                continue;
            }
        };
        let mut file_matched = false;
        for (index, line) in text.lines().enumerate() {
            let haystack = if input.case_insensitive {
                line.to_lowercase()
            } else {
                line.to_string()
            };
            if !haystack.contains(&needle) {
                continue;
            }
            file_matched = true;
            if matches.len() >= limits.max_grep_scan {
                scan_capped = true;
                break 'files;
            }
            matches.push(GrepMatch {
                path: display.clone(),
                absolute: absolute.clone(),
                line_number: index + 1,
                line: line.to_string(),
            });
        }
        if file_matched {
            matching_files += 1;
        }
    }

    let mut out = match input.mode {
        GrepMode::Count => format!(
            "[grep] count {} matching lines in {matching_files} files for {}\n",
            matches.len(),
            input.pattern
        ),
        GrepMode::FilesWithMatches => render_files_with_matches(input, &matches, limits),
        GrepMode::Matches => render_matches(input, &matches, limits),
    };
    // Fixed order, so the same tree always produces the same bytes: what was
    // cut short, then what was skipped, then what was never reached.
    if scan_capped {
        out.push_str(&format!(
            "... stopped after {} matches; narrow the pattern or the path\n",
            limits.max_grep_scan
        ));
    }
    out.push_str(&skipped.notes());
    out.push_str(&walk.notes(limits));

    let detail = format!("{} matches for {}", matches.len(), input.pattern);
    ToolResult::success(out, detail)
}

/// Files a grep pass could not search, by reason.
///
/// Two counters rather than one: "this file is a JPEG" and "this file could not
/// be opened" are different facts, and collapsing them would make one of the two
/// notes a lie.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct ScanSkips {
    /// Above the size cap, or not UTF-8 text.
    unsearchable: usize,
    /// Present as a candidate but unreadable.
    unreadable: usize,
}

impl ScanSkips {
    /// The lines that qualify the result. Empty when nothing was skipped, so a
    /// clean search carries no noise -- and, conversely, an unqualified
    /// `no matches` means every candidate really was searched.
    fn notes(self) -> String {
        let mut notes = String::new();
        if self.unsearchable > 0 {
            notes.push_str(&format!(
                "... skipped {} {} (too large or not text)\n",
                self.unsearchable,
                plural_files(self.unsearchable)
            ));
        }
        if self.unreadable > 0 {
            notes.push_str(&format!(
                "... skipped {} {} (could not be read)\n",
                self.unreadable,
                plural_files(self.unreadable)
            ));
        }
        notes
    }
}

fn plural_files(count: usize) -> &'static str {
    if count == 1 {
        "file"
    } else {
        "files"
    }
}

/// The window `head_limit`/`offset` select out of `total` results.
fn window(input: &GrepFilesInput, total: usize, limits: &ToolLimits) -> (usize, usize) {
    let limit = input
        .head_limit
        .unwrap_or(limits.max_grep_matches)
        .min(limits.max_grep_matches);
    let start = input.offset.min(total);
    let end = start.saturating_add(limit).min(total);
    (start, end)
}

fn render_matches(input: &GrepFilesInput, matches: &[GrepMatch], limits: &ToolLimits) -> String {
    let (start, end) = window(input, matches.len(), limits);
    let pattern = &input.pattern;
    let mut out = String::new();
    if matches.is_empty() {
        out.push_str(&format!("[grep] no matches for {pattern}\n"));
    } else if start == end {
        out.push_str(&format!(
            "[grep] no matches for {pattern} at offset {} ({} total matches)\n",
            input.offset,
            matches.len()
        ));
    } else {
        if start == 0 && end == matches.len() {
            out.push_str(&format!("[grep] {} matches for {pattern}\n", end - start));
        } else {
            out.push_str(&format!(
                "[grep] {} matches for {pattern} (showing {}-{end} of {})\n",
                end - start,
                start + 1,
                matches.len()
            ));
        }
        let context_lines = input.context_lines.min(limits.max_context_lines);
        for entry in &matches[start..end] {
            write_match(&mut out, entry, context_lines, limits);
        }
    }
    if end < matches.len() {
        out.push_str(&format!(
            "... more matches available; use offset {end} to continue\n"
        ));
    }
    out
}

fn render_files_with_matches(
    input: &GrepFilesInput,
    matches: &[GrepMatch],
    limits: &ToolLimits,
) -> String {
    let mut files: Vec<&str> = Vec::new();
    for entry in matches {
        if !files.contains(&entry.path.as_str()) {
            files.push(&entry.path);
        }
    }
    let (start, end) = window(input, files.len(), limits);
    let pattern = &input.pattern;
    let mut out = String::new();
    if files.is_empty() {
        out.push_str(&format!("[grep] no files with matches for {pattern}\n"));
    } else if start == end {
        out.push_str(&format!(
            "[grep] no files with matches for {pattern} at offset {} ({} total files)\n",
            input.offset,
            files.len()
        ));
    } else {
        if start == 0 && end == files.len() {
            out.push_str(&format!(
                "[grep] {} files with matches for {pattern}\n",
                end - start
            ));
        } else {
            out.push_str(&format!(
                "[grep] {} files with matches for {pattern} (showing {}-{end} of {})\n",
                end - start,
                start + 1,
                files.len()
            ));
        }
        for path in &files[start..end] {
            out.push_str(&format!(" - {path}\n"));
        }
    }
    if end < files.len() {
        out.push_str(&format!(
            "... more files available; use offset {end} to continue\n"
        ));
    }
    out
}

/// Writes one match and, when asked, the lines around it.
fn write_match(out: &mut String, entry: &GrepMatch, context_lines: usize, limits: &ToolLimits) {
    // No accounting here: this file already produced a match, so it was
    // searchable a moment ago. A failure now costs context, not correctness.
    let surrounding = if context_lines > 0 {
        read_searchable(&entry.absolute, limits.max_grep_file_bytes)
    } else {
        FileText::Unsearchable
    };
    if let Some(text) = surrounding.text() {
        let first = entry.line_number.saturating_sub(context_lines).max(1);
        write_context(out, entry, text, first, entry.line_number, limits);
    }
    out.push_str(&format!(" - {}:{}: ", entry.path, entry.line_number));
    push_clipped(out, &entry.line, limits.max_read_line_len);
    if let Some(text) = surrounding.text() {
        write_context(
            out,
            entry,
            text,
            entry.line_number + 1,
            entry.line_number + context_lines + 1,
            limits,
        );
    }
}

fn write_context(
    out: &mut String,
    entry: &GrepMatch,
    text: &str,
    first: usize,
    end_exclusive: usize,
    limits: &ToolLimits,
) {
    for (index, line) in text.lines().enumerate() {
        let number = index + 1;
        if number < first {
            continue;
        }
        if number >= end_exclusive {
            break;
        }
        out.push_str(&format!("   {}:{number}- ", entry.path));
        push_clipped(out, line, limits.max_read_line_len);
    }
}

fn push_clipped(out: &mut String, line: &str, limit: usize) {
    match clip(line, limit) {
        Some(clipped) => {
            out.push_str(clipped);
            out.push_str("...");
        }
        None => out.push_str(line),
    }
    out.push('\n');
}

/// Whether a candidate file could be turned into searchable text.
enum FileText {
    Text(String),
    /// Above the size cap, or not UTF-8 text.
    Unsearchable,
    /// Could not be read at all.
    Unreadable,
}

impl FileText {
    /// The text, when there is any. Used where a skip needs no accounting,
    /// such as re-reading a file for context lines around a match that was
    /// already found in it.
    fn text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            _ => None,
        }
    }
}

/// Reads a candidate file, saying why it cannot be searched when it cannot.
///
/// The reason is returned rather than swallowed. Silently dropping a file makes
/// `no matches` ambiguous, which is the one thing a search result must not be.
fn read_searchable(path: &Path, max_bytes: usize) -> FileText {
    let Ok(bytes) = fs::read(path) else {
        return FileText::Unreadable;
    };
    if bytes.len() > max_bytes || !is_model_safe(&bytes) {
        return FileText::Unsearchable;
    }
    match String::from_utf8(bytes) {
        Ok(text) => FileText::Text(text),
        // `is_model_safe` already proved this is UTF-8, so this arm is defensive
        // rather than expected; it still reports rather than panics.
        Err(_) => FileText::Unsearchable,
    }
}

// ---------------------------------------------------------------------------
// read_file
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadFileInput {
    pub path: String,
    pub start_line: usize,
    pub line_count: Option<usize>,
}

fn decode_read_file(input: &Value) -> Result<ToolInput, String> {
    let object = object("read_file", input)?;
    Ok(ToolInput::ReadFile(ReadFileInput {
        path: required_string("read_file", object, "path")?,
        start_line: optional_integer("read_file", object, "start_line", 1)?.unwrap_or(1),
        line_count: optional_integer("read_file", object, "line_count", 1)?,
    }))
}

fn validate_read_file(input: &ToolInput) -> Result<(), String> {
    let ToolInput::ReadFile(input) = input else {
        return Err(mismatched("read_file"));
    };
    nonblank("read_file", "path", &input.path)
}

fn execute_read_file(input: &ToolInput, context: &ToolContext) -> ToolResult {
    let ToolInput::ReadFile(input) = input else {
        return ToolResult::failure(mismatched("read_file"));
    };
    let limits = context.limits();
    let resolved = match context.scope().resolve_existing(&input.path) {
        Ok(resolved) => resolved,
        Err(err) => return refusal("read_file", err),
    };
    let display = context.scope().display_path(resolved.absolute());

    let metadata = match fs::metadata(resolved.absolute()) {
        Ok(metadata) => metadata,
        Err(err) => {
            return ToolResult::failure(format!("read_file cannot stat `{display}`: {err}"))
        }
    };
    if metadata.is_dir() {
        return ToolResult::failure(format!(
            "read_file cannot read `{display}`: it is a directory; use list_files"
        ));
    }
    // Everything that is not a regular file is refused here rather than in the
    // read below: opening a writer-less FIFO -- or a device that answers slowly,
    // or a socket -- parks the thread for as long as the turn lasts, and a tool
    // call that never returns is a turn the user cannot get out of. `metadata`
    // follows symlinks, so a symlink to a regular file is still a regular file;
    // which paths may be reached at all is `resolve_existing`'s business. This
    // stat and the read below are two separate pathname resolutions, so a target
    // swapped for a FIFO between them can still block: what this closes is the
    // stable-path hazard, not the race, which is the bargain a cooperative
    // workspace makes everywhere else too.
    if !metadata.is_file() {
        return ToolResult::failure(format!(
            "read_file cannot read `{display}`: it is not a regular file"
        ));
    }

    let bytes = match fs::read(resolved.absolute()) {
        Ok(bytes) => bytes,
        Err(err) => {
            return ToolResult::failure(format!("read_file cannot read `{display}`: {err}"))
        }
    };
    let file_bytes = bytes.len();
    let snapshot_complete = file_bytes <= limits.max_read_bytes;
    let snapshot = &bytes[..file_bytes.min(limits.max_read_bytes)];

    // A snapshot capped mid-character is not a binary file, so the incomplete
    // trailing character is dropped rather than reported as corruption.
    let text = match std::str::from_utf8(snapshot) {
        Ok(text) if !snapshot.contains(&0) => text,
        Err(err) if !snapshot_complete && err.error_len().is_none() => {
            // `valid_up_to` is a character boundary by construction.
            std::str::from_utf8(&snapshot[..err.valid_up_to()]).unwrap_or("")
        }
        // The bytes are named and counted rather than shown. Dumping them would
        // fill the model's context with noise it cannot use and might not
        // survive as valid UTF-8 on the wire.
        Ok(_) | Err(_) => return binary_result(&display, file_bytes),
    };

    // The model may ask for fewer lines than the cap but never for more: a
    // request for a whole 10,000-line file still returns 400 and says so
    // (`read_file.zig:162-163`).
    let wanted = input
        .line_count
        .unwrap_or(limits.max_read_lines)
        .min(limits.max_read_lines);
    let selection = select_lines(text, input.start_line, wanted, limits);

    let mut out = format!("<path>{display}</path>\n<content>\n");
    if selection.lines.is_empty() {
        if selection.total_lines > 0 && input.start_line > selection.total_lines {
            out.push_str(&format!(
                "... [start_line {} is beyond end of file; total lines {}]\n",
                input.start_line, selection.total_lines
            ));
        }
    } else {
        let width = digits(selection.lines[selection.lines.len() - 1].0);
        for (number, line) in &selection.lines {
            out.push_str(&number.to_string());
            for _ in 0..width - digits(*number) {
                out.push(' ');
            }
            out.push('\t');
            out.push_str(line);
            out.push('\n');
        }
    }

    let complete_view = !selection.truncated
        && input.start_line == 1
        && selection.lines.len() == selection.total_lines;
    if (!complete_view || !snapshot_complete)
        && (!selection.lines.is_empty() || selection.truncated)
    {
        if snapshot_complete {
            out.push_str(&format!(
                "... [showing {} of {} lines; use start_line/line_count to read more.]\n",
                selection.lines.len(),
                selection.total_lines
            ));
        } else {
            out.push_str(&format!(
                "... [showing {} of at least {} lines; file snapshot was capped before EOF.]\n",
                selection.lines.len(),
                selection.total_lines
            ));
        }
    }
    out.push_str("</content>");

    // A completed read is the proof a later mutation rests on. A capped snapshot
    // is not recorded at all -- xfx never saw the whole file, so it has nothing
    // to compare against later -- while a windowed or clipped view is recorded
    // as incomplete, so `write_file` can say which of the three things is wrong.
    if snapshot_complete {
        super::mutate::record_read(
            context,
            resolved.absolute(),
            &metadata,
            &bytes,
            complete_view,
        );
    }

    let detail = format!(
        "{display} ({} of {} lines)",
        selection.lines.len(),
        selection.total_lines
    );
    ToolResult::success(out, detail)
}

/// What `read_file` says about a file it will not show.
fn binary_result(display: &str, file_bytes: usize) -> ToolResult {
    ToolResult::success(
        format!(
            "<path>{display}</path>\n<content>binary or non-utf8 file omitted ({file_bytes} bytes)</content>"
        ),
        format!("{display} (binary, {file_bytes} bytes)"),
    )
}

/// The lines a read will show, and what it had to leave out.
struct LineSelection {
    lines: Vec<(usize, String)>,
    total_lines: usize,
    /// Some line was clipped, some line was dropped, or the byte budget ran out.
    truncated: bool,
}

/// Walks `text` once, counting every line and keeping the requested window.
///
/// Counting continues past the window because the sentinel has to say how many
/// lines the file actually has; stopping early would let xfx report "showing 400
/// of 400" for a 4000-line file (`read_file.zig:267-301`).
fn select_lines(
    text: &str,
    start_line: usize,
    wanted: usize,
    limits: &ToolLimits,
) -> LineSelection {
    let mut lines: Vec<(usize, String)> = Vec::new();
    let mut total_lines = 0usize;
    let mut truncated = false;
    let mut stop_keeping = false;
    let mut width = 1usize;
    let mut rendered = 0usize;

    let mut start = 0usize;
    let mut number = 1usize;
    while start < text.len() {
        let end = text[start..]
            .find('\n')
            .map(|offset| start + offset)
            .unwrap_or(text.len());
        let line = &text[start..end];
        total_lines = number;

        if !stop_keeping && number >= start_line {
            if lines.len() >= wanted {
                truncated = true;
                stop_keeping = true;
            } else {
                let display = match clip(line, limits.max_read_line_len) {
                    Some(clipped) => {
                        truncated = true;
                        format!("{clipped}{LINE_TRUNCATED_SUFFIX}")
                    }
                    None => line.to_string(),
                };
                // The number column widens as the numbers do, so the budget has
                // to be recomputed for the lines already kept.
                let next_width = digits(number);
                if next_width > width {
                    rendered += lines.len() * (next_width - width);
                    width = next_width;
                }
                let cost = width + 1 + display.len() + 1;
                if rendered + cost > limits.max_output_bytes {
                    truncated = true;
                    stop_keeping = true;
                } else {
                    rendered += cost;
                    lines.push((number, display));
                }
            }
        }

        if end == text.len() {
            break;
        }
        start = end + 1;
        number += 1;
    }

    LineSelection {
        lines,
        total_lines,
        truncated,
    }
}

fn digits(mut value: usize) -> usize {
    let mut count = 1;
    while value >= 10 {
        value /= 10;
        count += 1;
    }
    count
}

// ---------------------------------------------------------------------------
// shared helpers
// ---------------------------------------------------------------------------

/// The result of one bounded, symlink-free walk.
struct WalkResult {
    files: Vec<PathBuf>,
    /// The candidate cap was reached before the tree was exhausted.
    incomplete: bool,
    /// Entries the walker could not read at all.
    unreadable: usize,
}

impl WalkResult {
    /// The lines that say what the walk could not see. Empty when it saw
    /// everything, so a normal result carries no noise.
    fn notes(&self, limits: &ToolLimits) -> String {
        let mut notes = String::new();
        if self.incomplete {
            notes.push_str(&format!(
                "... candidate list may be incomplete; candidate cap {} reached before all files were discovered\n",
                limits.max_candidates
            ));
        }
        if self.unreadable > 0 {
            notes.push_str(&format!(
                "... skipped {} unreadable {}\n",
                self.unreadable,
                if self.unreadable == 1 {
                    "entry"
                } else {
                    "entries"
                }
            ));
        }
        notes
    }
}

/// Walks `root` for regular files, bounded and deterministic.
///
/// `follow_links` stays off, so a symlink is never traversed: that is what makes
/// a walk unable to leave the scope even though only the root was proven in it.
/// `.gitignore` is applied, and the always-ignored directory names are pruned,
/// so a search sees the project rather than its build output.
///
/// "Regular files" is load-bearing rather than tidy: every caller either reads
/// what this yields or offers it to the model as something to read, and opening
/// a writer-less FIFO parks the turn for as long as it lasts.
fn walk_files(root: &Path, limits: &ToolLimits, hidden: bool) -> WalkResult {
    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(!hidden)
        .follow_links(false)
        .parents(false)
        .ignore(false)
        .git_ignore(true)
        // A machine-wide `~/.gitignore` would make the same tree produce
        // different results on two machines.
        .git_global(false)
        .git_exclude(false)
        // `.gitignore` applies whether or not the directory is a repository
        // checkout, so results do not change the moment `git init` is run.
        .require_git(false)
        .sort_by_file_name(Ord::cmp)
        .filter_entry(|entry| {
            !entry.file_type().is_some_and(|kind| kind.is_dir())
                || !is_ignored_directory(&entry.file_name().to_string_lossy())
        });

    let mut files = Vec::new();
    let mut unreadable = 0usize;
    let mut incomplete = false;
    for entry in builder.build() {
        match entry {
            Ok(entry) => {
                if !entry.file_type().is_some_and(|kind| kind.is_file()) {
                    continue;
                }
                if files.len() >= limits.max_candidates {
                    incomplete = true;
                    break;
                }
                files.push(entry.into_path());
            }
            Err(_) => unreadable += 1,
        }
    }
    WalkResult {
        files,
        incomplete,
        unreadable,
    }
}

/// Resolves the directory a search runs below.
fn search_root(
    tool: &str,
    context: &ToolContext,
    requested: Option<&str>,
) -> Result<PathBuf, ToolResult> {
    let requested = requested.unwrap_or(".");
    let resolved = context
        .scope()
        .resolve_existing(requested)
        .map_err(|err| refusal(tool, err))?;
    let metadata = fs::metadata(resolved.absolute()).map_err(|err| {
        ToolResult::failure(format!(
            "{tool} cannot stat `{}`: {err}",
            context.scope().display_path(resolved.absolute())
        ))
    })?;
    if !metadata.is_dir() {
        return Err(ToolResult::failure(format!(
            "{tool} needs a directory to search below; `{}` is a file",
            context.scope().display_path(resolved.absolute())
        )));
    }
    Ok(resolved.absolute().to_path_buf())
}

fn compile_glob(tool: &str, field: &str, pattern: &str) -> Result<GlobMatcher, String> {
    Glob::new(pattern)
        .map(|glob| glob.compile_matcher())
        .map_err(|err| format!("{tool} field `{field}` is not a valid glob: {err}"))
}

/// Whether `relative` matches, by whole path or by file name.
///
/// The file-name fallback is upstream's behavior: a model that writes `*.rs`
/// means "Rust files", not "Rust files in the top directory only"
/// (`vercel-labs/fx@580a0c5d src/tools/filesystem/grep_files.zig:1060-1066`).
fn glob_matches(matcher: &GlobMatcher, relative: &str) -> bool {
    if matcher.is_match(relative) {
        return true;
    }
    match relative.rsplit_once('/') {
        Some((_, name)) => matcher.is_match(name),
        None => false,
    }
}

/// Whether a glob deliberately reaches into a dot-directory or dot-file.
///
/// Hidden entries are skipped unless the model asked for one by name, which is
/// upstream's rule (`glob_files.zig:336-338`).
fn wants_hidden(pattern: &str) -> bool {
    pattern.split('/').any(|segment| segment.starts_with('.'))
}

fn relative_to(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

/// Whether bytes are safe to put in front of a model as text.
fn is_model_safe(bytes: &[u8]) -> bool {
    !bytes.contains(&0) && std::str::from_utf8(bytes).is_ok()
}

/// The model-visible form of a path refusal.
fn refusal(tool: &str, err: PathError) -> ToolResult {
    ToolResult::failure(format!("{tool} refused the path: {err}"))
}

/// A decoded input reached the wrong executor.
///
/// Unreachable through [`ToolSpec::run`], which pairs each decoder with its own
/// executor, but stated rather than `unreachable!()`: a panic inside a tool call
/// would take the whole turn down with no result to show for it.
fn mismatched(tool: &str) -> String {
    format!("{tool} received arguments that belong to another tool")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn line_numbers_widen_without_renumbering() {
        assert_eq!(digits(1), 1);
        assert_eq!(digits(9), 1);
        assert_eq!(digits(10), 2);
        assert_eq!(digits(400), 3);
        assert_eq!(digits(1_000), 4);
    }

    #[test]
    fn a_trailing_newline_does_not_create_an_extra_line() {
        let limits = ToolLimits::default();
        let selection = select_lines("a\nb\nc\n", 1, 400, &limits);
        assert_eq!(selection.total_lines, 3);
        assert_eq!(selection.lines.len(), 3);
        assert!(!selection.truncated);

        // A file that does not end in a newline still has its last line.
        let selection = select_lines("a\nb", 1, 400, &limits);
        assert_eq!(selection.total_lines, 2);
        assert_eq!(selection.lines[1].1, "b");
    }

    #[test]
    fn counting_continues_past_the_window_so_the_total_is_true() {
        let limits = ToolLimits::default();
        let text: String = (1..=50).map(|n| format!("{n}\n")).collect();
        let selection = select_lines(&text, 1, 5, &limits);
        assert_eq!(selection.lines.len(), 5);
        assert_eq!(
            selection.total_lines, 50,
            "the total is the file's, not the window's"
        );
        assert!(selection.truncated);
    }

    #[test]
    fn the_byte_budget_stops_a_read_before_it_floods_the_model() {
        let limits = ToolLimits {
            max_output_bytes: 40,
            ..ToolLimits::default()
        };
        let text: String = (1..=100).map(|_| "0123456789\n".to_string()).collect();
        let selection = select_lines(&text, 1, 400, &limits);
        assert!(selection.truncated);
        assert!(selection.lines.len() < 100, "{}", selection.lines.len());
        assert_eq!(selection.total_lines, 100);
    }

    #[test]
    fn a_glob_matches_by_whole_path_or_by_file_name() {
        let matcher = compile_glob("t", "pattern", "*.rs").expect("valid glob");
        assert!(glob_matches(&matcher, "main.rs"));
        assert!(glob_matches(&matcher, "src/main.rs"));
        assert!(!glob_matches(&matcher, "src/main.md"));
    }

    #[test]
    fn a_hidden_segment_in_a_pattern_opts_into_hidden_entries() {
        assert!(wants_hidden(".github/**"));
        assert!(wants_hidden("**/.xfx.json"));
        assert!(wants_hidden(".*"));
        assert!(!wants_hidden("src/**/*.rs"));
    }

    #[test]
    fn a_search_that_skipped_nothing_says_nothing() {
        // The load-bearing half: silence has to mean "everything was searched",
        // so an empty note is the only thing that may follow a clean search.
        assert_eq!(ScanSkips::default().notes(), "");
    }

    #[test]
    fn a_skip_note_agrees_with_itself_about_singular_and_plural() {
        assert_eq!(
            ScanSkips {
                unsearchable: 1,
                unreadable: 0,
            }
            .notes(),
            "... skipped 1 file (too large or not text)\n"
        );
        assert_eq!(
            ScanSkips {
                unsearchable: 3,
                unreadable: 0,
            }
            .notes(),
            "... skipped 3 files (too large or not text)\n"
        );
        // Two causes, two lines, always in this order.
        assert_eq!(
            ScanSkips {
                unsearchable: 2,
                unreadable: 1,
            }
            .notes(),
            "... skipped 2 files (too large or not text)\n\
             ... skipped 1 file (could not be read)\n"
        );
    }

    #[test]
    fn an_unreadable_candidate_is_not_reported_as_the_wrong_kind_of_skip() {
        // Reasons are counted separately so neither note can claim something
        // untrue about the other's files.
        let missing = read_searchable(Path::new("/nonexistent/xfx/candidate"), 1024);
        assert!(matches!(missing, FileText::Unreadable));
        assert!(missing.text().is_none());
    }

    #[test]
    fn binary_content_is_never_treated_as_text() {
        assert!(is_model_safe(b"plain text"));
        assert!(!is_model_safe(b"has\x00nul"));
        assert!(!is_model_safe(&[0xff, 0xfe]));
    }

    #[test]
    fn every_decoder_refuses_a_non_object_argument() {
        for (decode, tool) in [
            (decode_list_files as fn(&Value) -> _, "list_files"),
            (decode_glob_files, "glob_files"),
            (decode_grep_files, "grep_files"),
            (decode_read_file, "read_file"),
        ] {
            let err = decode(&json!(["not", "an", "object"])).expect_err("refused");
            assert_eq!(err, format!("{tool} arguments must be a JSON object"));
        }
    }

    #[test]
    fn a_grep_window_is_bounded_by_the_output_cap_whatever_the_model_asks() {
        let limits = ToolLimits {
            max_grep_matches: 5,
            ..ToolLimits::default()
        };
        let input = GrepFilesInput {
            pattern: "x".to_string(),
            path: None,
            include: None,
            case_insensitive: false,
            mode: GrepMode::Matches,
            head_limit: Some(1_000),
            offset: 0,
            context_lines: 0,
        };
        assert_eq!(window(&input, 100, &limits), (0, 5));

        // An offset past the end selects nothing rather than panicking.
        let input = GrepFilesInput {
            offset: 500,
            ..input
        };
        assert_eq!(window(&input, 100, &limits), (100, 100));
    }
}
