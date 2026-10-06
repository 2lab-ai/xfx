//! What a completed block hands its caller (`payload.zig`).
//!
//! Upstream passes completions as `ctx` pointers plus function pointers that
//! take ownership of the payload only on success (`payload.zig:75-96`). Here a
//! completion is a closure that takes the payload by value; there is no
//! failure path to hand ownership back on.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableColumnAlign {
    Left,
    Right,
    Center,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TableRow {
    /// Each cell already rendered: inline markdown is ANSI, links are OSC 8.
    pub cells: Vec<String>,
}

/// A pipe table, parsed and with its cells rendered. Row 0 is the header;
/// rows may be ragged, and `column_count` is the widest row.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TablePayload {
    pub rows: Vec<TableRow>,
    pub alignments: Vec<TableColumnAlign>,
    pub column_count: usize,
}

/// A fenced or indented code block: the fence's first info word and the
/// literal source, every line `\n`-terminated.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CodeBlockPayload {
    pub language: String,
    pub code: String,
}

/// Receives a completed table, and may write into the presenter's pending
/// output first (upstream's callers drain it there).
pub type TableCompletion<'a> = dyn FnMut(TablePayload, &mut Vec<u8>) + 'a;
/// Receives a completed code block.
pub type CodeBlockCompletion<'a> = dyn FnMut(CodeBlockPayload, &mut Vec<u8>) + 'a;
/// Receives a thematic rule (`---`, `***`, `___`).
pub type ThematicRuleCompletion<'a> = dyn FnMut(&mut Vec<u8>) + 'a;

/// Which completed blocks the caller takes over instead of the presenter
/// rendering them inline (`payload.zig:92-96`).
///
/// A present `thematic_rule` completion also turns on the two constructs
/// that need one line of lookahead, Setext headings and definition lists,
/// exactly as upstream does: a line that might be a Setext title is held
/// until the next line decides it.
#[derive(Default)]
pub struct MarkdownCompletions<'a> {
    pub table: Option<&'a mut TableCompletion<'a>>,
    pub code: Option<&'a mut CodeBlockCompletion<'a>>,
    pub thematic_rule: Option<&'a mut ThematicRuleCompletion<'a>>,
}

/// Numbers a footnote label at its first reference (`payload.zig:105-108`).
pub(super) trait FootnoteSink {
    fn register(&mut self, label: &[u8]) -> usize;
}
