//! The streaming presenter (`assistant_presentation.zig`).
//!
//! Text arrives in arbitrary chunks. A line is held until its `\n` arrives
//! and is then rendered at once, so prose streams line by line; a fenced or
//! indented code block, a pipe table, and a thematic rule are held until
//! complete and handed to the caller whole when the caller asks for them
//! through [`MarkdownCompletions`]. Without a completion each renders inline
//! instead: code behind a dim `│ ` rule, a table as aligned rows, a rule as a
//! dim line [`ansi::HORIZONTAL_RULE_WIDTH`] cells wide.
//!
//! The presenter applies no indentation of its own. Every leading space it
//! writes comes from the source (list and quote indents) or from a construct
//! that owns it (a definition body's two-space lead, a footnote's
//! continuation marker); the indent the transcript paints assistant text at
//! belongs to the caller.

use std::cell::RefCell;

use super::ansi::{self, Styles};
use super::block_parse as bp;
use super::block_render;
use super::inline_render::{self, RenderContext};
use super::payload::{CodeBlockPayload, FootnoteSink, MarkdownCompletions, TablePayload};
use super::text_util as tu;
use super::theme::Theme;

/// One unit of presenter output (`assistant_presentation.zig:29-76`).
///
/// [`MarkdownProcessor::feed`] and [`MarkdownProcessor::finish`] return these
/// in order: rendered prose accumulates into one `Text` until a block
/// completes, and that block follows the text before it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Rendered prose: ANSI styling and OSC 8 links, `\n`-separated lines.
    Text(String),
    Table(TablePayload),
    CodeBlock(CodeBlockPayload),
    ThematicRule,
}

impl Event {
    /// Whether text queued before this event must be presented first.
    pub fn requires_text_drain(&self) -> bool {
        !matches!(self, Event::Text(_))
    }

    /// Roughly how many bytes this event keeps alive, counted the way
    /// upstream counts them: a cell costs its bytes plus one slice header.
    pub fn retained_byte_count(&self) -> usize {
        match self {
            Event::Text(text) => text.len(),
            Event::Table(table) => {
                let mut total = table.alignments.len();
                for row in &table.rows {
                    total = total.saturating_add(
                        row.cells.len().saturating_mul(std::mem::size_of::<&[u8]>()),
                    );
                    for cell in &row.cells {
                        total = total.saturating_add(cell.len());
                    }
                }
                total
            }
            Event::CodeBlock(block) => block.language.len().saturating_add(block.code.len()),
            Event::ThematicRule => 1,
        }
    }
}

/// Parses a pipe table into a payload with rendered cells, numbering its
/// links from `link_id` (`assistant_presentation.zig:82-84`).
///
/// Upstream numbers links from one process-wide counter; here the caller
/// owns it, and hands the same counter to every call that should share one
/// sequence.
pub fn parse_table_payload(buf: &str, styles: &Styles, link_id: &mut u32) -> TablePayload {
    let mut cx = RenderContext {
        styles,
        link_id,
        footnotes: None,
    };
    block_render::parse_table_payload(&mut cx, buf.as_bytes())
}

struct Footnote {
    label: Vec<u8>,
    number: Option<usize>,
    body: Vec<u8>,
    has_definition: bool,
}

/// Footnotes seen so far, numbered in first-reference order.
#[derive(Default)]
struct Footnotes {
    notes: Vec<Footnote>,
    next_number: usize,
}

impl Footnotes {
    fn find_or_append(&mut self, label: &[u8]) -> usize {
        if let Some(index) = self.notes.iter().position(|note| note.label == label) {
            return index;
        }
        self.notes.push(Footnote {
            label: label.to_vec(),
            number: None,
            body: Vec::new(),
            has_definition: false,
        });
        self.notes.len() - 1
    }

    fn clear(&mut self) {
        self.notes.clear();
        self.next_number = 0;
    }
}

impl FootnoteSink for Footnotes {
    fn register(&mut self, label: &[u8]) -> usize {
        let index = self.find_or_append(label);
        let note = &mut self.notes[index];
        match note.number {
            Some(number) => number,
            None => {
                self.next_number += 1;
                note.number = Some(self.next_number);
                self.next_number
            }
        }
    }
}

#[derive(Clone, Copy)]
struct ActiveFootnote {
    index: usize,
    append_body: bool,
}

/// The render context over a processor's own colours, link counter, and
/// footnotes. A macro rather than a method so the borrow stays on those
/// three fields and the caller can keep mutating the rest.
macro_rules! cx {
    ($processor:ident) => {
        RenderContext {
            styles: &$processor.styles,
            link_id: &mut $processor.next_link_id,
            footnotes: Some(&mut $processor.footnotes),
        }
    };
}

/// The streaming markdown presenter (`assistant_presentation.zig:142-622`).
pub struct MarkdownProcessor {
    line_buf: Vec<u8>,
    pending_top_level_line: Vec<u8>,
    pipe_buf: Vec<u8>,
    code_buf: Vec<u8>,
    code_language: Vec<u8>,
    pub(super) in_code_block: bool,
    /// Set for fenced blocks; `None` while inside an indented code block.
    pub(super) code_fence: Option<bp::CodeFence>,
    in_pipe_block: bool,
    pipe_last_line_has_lf: bool,
    active_blockquote: Option<bp::BlockquotePrefix>,
    active_definition: bool,
    footnotes: Footnotes,
    active_footnote: Option<ActiveFootnote>,
    previous_line_was_blank: bool,
    styles: Styles,
    next_link_id: u32,
}

impl Default for MarkdownProcessor {
    fn default() -> Self {
        Self::with_styles(Styles::default())
    }
}

impl MarkdownProcessor {
    /// A presenter in upstream's start-up palette, numbering links from 1.
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_styles(styles: Styles) -> Self {
        Self {
            line_buf: Vec::new(),
            pending_top_level_line: Vec::new(),
            pipe_buf: Vec::new(),
            code_buf: Vec::new(),
            code_language: Vec::new(),
            in_code_block: false,
            code_fence: None,
            in_pipe_block: false,
            pipe_last_line_has_lf: false,
            active_blockquote: None,
            active_definition: false,
            footnotes: Footnotes::default(),
            active_footnote: None,
            previous_line_was_blank: true,
            styles,
            next_link_id: 1,
        }
    }

    /// Whether the line this presenter holds, the one it has not seen the
    /// end of, carries `text`. An empty `text` is nothing to look for, and
    /// is not held.
    #[cfg(test)]
    pub(crate) fn holds(&self, text: &str) -> bool {
        let text = text.as_bytes();
        !text.is_empty()
            && [&self.line_buf, &self.pending_top_level_line]
                .iter()
                .any(|held| held.windows(text.len()).any(|window| window == text))
    }

    pub fn styles(&self) -> &Styles {
        &self.styles
    }

    pub fn styles_mut(&mut self) -> &mut Styles {
        &mut self.styles
    }

    /// [`Styles::set_inline_code_theme`] on this presenter's palette.
    pub fn set_inline_code_theme(&mut self, light: bool) {
        self.styles.set_inline_code_theme(light);
    }

    /// [`Styles::apply_theme`] on this presenter's palette. Like upstream's
    /// `applyTheme`, this sets the inline code and completed-task slots only;
    /// the link colour follows [`Self::activate_theme`].
    pub fn apply_theme(&mut self, theme: &Theme) {
        self.styles.apply_theme(theme);
    }

    /// [`Styles::activate`] on this presenter's palette: the theme links are
    /// coloured from (upstream's `shared_theme.activate`).
    pub fn activate_theme(&mut self, theme: Theme) {
        self.styles.activate(theme);
    }

    /// The id the next OSC 8 link will carry.
    pub fn next_link_id(&self) -> u32 {
        self.next_link_id
    }

    /// Continues a link id sequence another presenter (or
    /// [`parse_table_payload`]) advanced, so ids stay distinct across them.
    pub fn set_next_link_id(&mut self, id: u32) {
        self.next_link_id = id;
    }

    /// Forgets every partial line, held block, and footnote. The palette and
    /// the link id sequence carry on.
    pub fn reset(&mut self) {
        self.line_buf.clear();
        self.pending_top_level_line.clear();
        self.pipe_buf.clear();
        self.code_buf.clear();
        self.code_language.clear();
        self.footnotes.clear();
        self.in_code_block = false;
        self.code_fence = None;
        self.in_pipe_block = false;
        self.pipe_last_line_has_lf = false;
        self.active_blockquote = None;
        self.active_definition = false;
        self.active_footnote = None;
        self.previous_line_was_blank = true;
    }

    /// Feeds a chunk with every completed block taken over by the caller, and
    /// returns what it completed. Upstream's streaming caller drives the
    /// presenter exactly this way (`assistant_stream.zig:435-457`).
    pub fn feed(&mut self, chunk: &str) -> Vec<Event> {
        self.collect_events(|processor, out, completions| {
            processor.push_with_completions(chunk, out, completions);
        })
    }

    /// Ends the stream: renders a partial last line without its newline,
    /// closes any open block, and appends footnote definitions.
    pub fn finish(&mut self) -> Vec<Event> {
        self.collect_events(|processor, out, completions| {
            processor.flush_with_completions(out, completions);
        })
    }

    fn collect_events(
        &mut self,
        run: impl FnOnce(&mut Self, &mut Vec<u8>, MarkdownCompletions<'_>),
    ) -> Vec<Event> {
        let events = RefCell::new(Vec::new());
        let mut out = Vec::new();
        {
            let mut table = |table: TablePayload, out: &mut Vec<u8>| {
                drain_text(&events, out);
                events.borrow_mut().push(Event::Table(table));
            };
            let mut code = |block: CodeBlockPayload, out: &mut Vec<u8>| {
                drain_text(&events, out);
                events.borrow_mut().push(Event::CodeBlock(block));
            };
            let mut rule = |out: &mut Vec<u8>| {
                drain_text(&events, out);
                events.borrow_mut().push(Event::ThematicRule);
            };
            run(
                self,
                &mut out,
                MarkdownCompletions {
                    table: Some(&mut table),
                    code: Some(&mut code),
                    thematic_rule: Some(&mut rule),
                },
            );
        }
        drain_text(&events, &mut out);
        events.into_inner()
    }

    /// Renders every complete line of `input` into `out`; nothing is taken
    /// over, so code, tables, and rules render inline.
    pub fn push(&mut self, input: &str, out: &mut Vec<u8>) {
        self.push_with_completions(input, out, MarkdownCompletions::default());
    }

    pub fn push_with_completions(
        &mut self,
        input: &str,
        out: &mut Vec<u8>,
        mut completions: MarkdownCompletions<'_>,
    ) {
        for &byte in input.as_bytes() {
            if byte == b'\n' {
                let mut line = std::mem::take(&mut self.line_buf);
                let line_end = line.len() - usize::from(line.last() == Some(&b'\r'));
                self.handle_line(&line[..line_end], true, out, &mut completions);
                line.clear();
                self.line_buf = line;
            } else {
                self.line_buf.push(byte);
            }
        }
    }

    pub fn flush(&mut self, out: &mut Vec<u8>) {
        self.flush_with_completions(out, MarkdownCompletions::default());
    }

    pub fn flush_with_completions(
        &mut self,
        out: &mut Vec<u8>,
        mut completions: MarkdownCompletions<'_>,
    ) {
        let had_partial_line = !self.line_buf.is_empty();
        let len_before = out.len();
        if !self.line_buf.is_empty() {
            let mut line = std::mem::take(&mut self.line_buf);
            self.handle_line(&line, false, out, &mut completions);
            line.clear();
            self.line_buf = line;
        }
        self.flush_pending_top_level_line(out);
        if had_partial_line && out.len() > len_before && out.last() == Some(&b'\n') {
            out.pop();
        }
        if self.in_pipe_block {
            self.finalize_pipe_block(out, &mut completions);
        }
        if self.in_code_block {
            self.finalize_code_block(out, &mut completions);
            self.in_code_block = false;
            self.code_fence = None;
        }
        self.flush_footnotes(out);
        self.active_blockquote = None;
        self.active_definition = false;
        self.active_footnote = None;
    }

    fn handle_line(
        &mut self,
        line: &[u8],
        line_has_lf: bool,
        out: &mut Vec<u8>,
        completions: &mut MarkdownCompletions<'_>,
    ) {
        self.handle_line_body(line, line_has_lf, out, completions);
        self.previous_line_was_blank = tu::is_blank_markdown_line(line);
    }

    fn handle_line_body(
        &mut self,
        line: &[u8],
        line_has_lf: bool,
        out: &mut Vec<u8>,
        completions: &mut MarkdownCompletions<'_>,
    ) {
        if self.in_code_block {
            self.active_definition = false;
            if let Some(fence) = self.code_fence {
                if bp::closes_code_fence(line, fence) {
                    self.finalize_code_block(out, completions);
                    self.in_code_block = false;
                    self.code_fence = None;
                    return;
                }
            } else if !tu::is_blank_markdown_line(line) && !bp::has_indented_code_prefix(line) {
                self.finalize_code_block(out, completions);
                self.in_code_block = false;
                self.handle_line(line, line_has_lf, out, completions);
                return;
            }
            let code_line = if let Some(fence) = self.code_fence {
                bp::strip_fence_indent(line, fence.indent)
            } else if !tu::is_blank_markdown_line(line) {
                bp::deindent_code_line(line)
            } else {
                line
            };
            self.append_code_line(code_line, line_has_lf, out, completions);
            return;
        }

        if self.in_pipe_block {
            self.active_definition = false;
            if bp::is_pipe_line(line)
                && self.pipe_buf.len() + line.len() < ansi::MAX_PIPE_BUFFER_BYTES
            {
                self.pipe_buf.extend_from_slice(line);
                self.pipe_buf.push(b'\n');
                self.pipe_last_line_has_lf = line_has_lf;
                return;
            }
            self.finalize_pipe_block(out, completions);
        }

        if let Some(active) = self.active_footnote {
            if let Some(body) = bp::footnote_continuation_body(line) {
                if active.append_body {
                    let note = &mut self.footnotes.notes[active.index];
                    note.body.push(b'\n');
                    note.body.extend_from_slice(body);
                }
                return;
            }
            self.active_footnote = None;
        }

        if let Some(definition) = bp::parse_footnote_definition(line) {
            self.active_definition = false;
            self.flush_pending_top_level_line(out);
            self.begin_footnote_definition(definition.label, definition.body);
            return;
        }

        if completions.thematic_rule.is_some() && !self.pending_top_level_line.is_empty() {
            if let Some(level) = bp::parse_setext_underline(line) {
                self.active_definition = false;
                let mut pending = std::mem::take(&mut self.pending_top_level_line);
                block_render::write_heading(
                    &mut cx!(self),
                    level,
                    tu::without_terminal_hard_break_marker(&pending, true),
                    out,
                );
                out.push(b'\n');
                pending.clear();
                self.pending_top_level_line = pending;
                return;
            }
            if let Some(body) = bp::definition_marker_body(line) {
                self.flush_pending_top_level_line(out);
                block_render::write_definition_line(&mut cx!(self), body, line_has_lf, out);
                self.active_definition = true;
                return;
            }
            self.active_definition = false;
            self.flush_pending_top_level_line(out);
        }

        if let Some(blockquote) = self.active_blockquote {
            self.active_definition = false;
            if bp::is_lazy_blockquote_continuation(line) {
                block_render::write_blockquote_line(
                    &mut cx!(self),
                    blockquote,
                    line,
                    line_has_lf,
                    out,
                );
                out.push(b'\n');
                return;
            }
            self.active_blockquote = None;
        }

        // An indented list marker continues a list, except that a plus sign
        // in indented-code position stays code so diff-style lines are
        // preserved.
        let indented_list_item = bp::has_indented_code_prefix(line)
            && (bp::parse_unordered_list(line).is_some() || bp::parse_ordered_list(line).is_some());
        let plus_in_code_position = indented_list_item
            && self.previous_line_was_blank
            && tu::left_trim(line).first() == Some(&b'+');
        if indented_list_item && !plus_in_code_position {
            self.active_definition = false;
            self.process_line(line, line_has_lf, out);
            out.push(b'\n');
            return;
        }

        if self.previous_line_was_blank && bp::has_indented_code_prefix(line) {
            self.active_definition = false;
            self.in_code_block = true;
            self.code_fence = None;
            self.code_language.clear();
            self.append_code_line(bp::deindent_code_line(line), line_has_lf, out, completions);
            return;
        }

        if !self.in_code_block && bp::is_pipe_line(line) {
            self.active_definition = false;
            self.in_pipe_block = true;
            self.pipe_buf.extend_from_slice(line);
            self.pipe_buf.push(b'\n');
            self.pipe_last_line_has_lf = line_has_lf;
            return;
        }

        if let Some(fence) = bp::parse_code_fence(line) {
            self.active_definition = false;
            self.in_code_block = true;
            self.code_fence = Some(fence);
            self.code_language.clear();
            self.code_language
                .extend_from_slice(bp::code_fence_language(line));
            return;
        }

        if self.active_definition {
            if let Some(body) = bp::definition_marker_body(line) {
                block_render::write_definition_line(&mut cx!(self), body, line_has_lf, out);
                return;
            }
        }
        self.active_definition = false;

        if line_has_lf && completions.thematic_rule.is_some() && bp::is_setext_candidate(line) {
            self.pending_top_level_line.extend_from_slice(line);
            return;
        }

        if bp::is_horizontal_rule(line) {
            if let Some(deliver) = completions.thematic_rule.as_mut() {
                deliver(out);
                return;
            }
        }

        self.process_line(line, line_has_lf, out);
        out.push(b'\n');
    }

    fn append_code_line(
        &mut self,
        line: &[u8],
        line_has_lf: bool,
        out: &mut Vec<u8>,
        completions: &mut MarkdownCompletions<'_>,
    ) {
        if completions.code.is_some() {
            self.code_buf.extend_from_slice(line);
            self.code_buf.push(b'\n');
            return;
        }
        self.process_line(line, line_has_lf, out);
        out.push(b'\n');
    }

    fn flush_pending_top_level_line(&mut self, out: &mut Vec<u8>) {
        if self.pending_top_level_line.is_empty() {
            return;
        }
        let mut pending = std::mem::take(&mut self.pending_top_level_line);
        self.process_line(&pending, true, out);
        out.push(b'\n');
        pending.clear();
        self.pending_top_level_line = pending;
    }

    fn process_line(&mut self, line: &[u8], line_has_lf: bool, out: &mut Vec<u8>) {
        if self.in_code_block {
            ansi::write_dim(out, ansi::VERTICAL_RULE_PREFIX.as_bytes());
            out.extend_from_slice(line);
            return;
        }

        if bp::is_horizontal_rule(line) {
            ansi::write_horizontal_rule(out);
            return;
        }

        if let Some(header) =
            bp::parse_header(tu::without_terminal_hard_break_marker(line, line_has_lf))
        {
            block_render::write_heading(&mut cx!(self), header.level, header.content, out);
            return;
        }

        if let Some(parsed) = bp::parse_blockquote(line) {
            let blockquote = bp::BlockquotePrefix {
                indent: parsed.indent.len(),
                depth: parsed.depth,
            };
            self.active_blockquote =
                bp::is_blockquote_paragraph(parsed.content).then_some(blockquote);
            block_render::write_blockquote_line(
                &mut cx!(self),
                blockquote,
                parsed.content,
                line_has_lf,
                out,
            );
            return;
        }

        if block_render::write_list_line(&mut cx!(self), line, line_has_lf, out) {
            return;
        }

        inline_render::write_inline(
            &mut cx!(self),
            tu::without_terminal_hard_break_marker(line, line_has_lf),
            out,
            false,
        );
    }

    fn finalize_pipe_block(
        &mut self,
        out: &mut Vec<u8>,
        completions: &mut MarkdownCompletions<'_>,
    ) {
        let mut buf = std::mem::take(&mut self.pipe_buf);
        if bp::is_valid_table(&buf) {
            if let Some(deliver) = completions.table.as_mut() {
                let table = block_render::parse_table_payload(&mut cx!(self), &buf);
                deliver(table, out);
            } else {
                block_render::render_table(&mut cx!(self), &buf, out);
            }
        } else {
            let mut start = 0;
            while start < buf.len() {
                let end = tu::index_of_scalar_pos(&buf, start, b'\n').unwrap_or(buf.len());
                let line_has_lf = if end + 1 == buf.len() {
                    self.pipe_last_line_has_lf
                } else {
                    true
                };
                self.process_line(&buf[start..end], line_has_lf, out);
                if line_has_lf {
                    out.push(b'\n');
                }
                start = end + 1;
            }
        }
        buf.clear();
        self.pipe_buf = buf;
        self.in_pipe_block = false;
        self.pipe_last_line_has_lf = false;
    }

    fn finalize_code_block(
        &mut self,
        out: &mut Vec<u8>,
        completions: &mut MarkdownCompletions<'_>,
    ) {
        if let Some(deliver) = completions.code.as_mut() {
            let block = CodeBlockPayload {
                language: tu::into_string(std::mem::take(&mut self.code_language)),
                code: tu::into_string(std::mem::take(&mut self.code_buf)),
            };
            deliver(block, out);
        }
        self.code_buf.clear();
        self.code_language.clear();
    }

    fn begin_footnote_definition(&mut self, label: &[u8], body: &[u8]) {
        let index = self.footnotes.find_or_append(label);
        let note = &mut self.footnotes.notes[index];
        if note.has_definition {
            self.active_footnote = Some(ActiveFootnote {
                index,
                append_body: false,
            });
            return;
        }
        note.body.extend_from_slice(body);
        note.has_definition = true;
        self.active_footnote = Some(ActiveFootnote {
            index,
            append_body: true,
        });
    }

    fn flush_footnotes(&mut self, out: &mut Vec<u8>) {
        let has_note = self
            .footnotes
            .notes
            .iter()
            .any(|note| note.number.is_some() && note.has_definition);
        if has_note {
            while out.last() == Some(&b'\n') {
                out.pop();
            }
            if out.is_empty() {
                out.push(b'\n');
            } else {
                out.extend_from_slice(b"\n\n");
            }

            let mut number = 1;
            while number <= self.footnotes.next_number {
                // A body can reference a footnote not seen before, which
                // numbers it and so extends this loop.
                for index in 0..self.footnotes.notes.len() {
                    let note = &self.footnotes.notes[index];
                    if note.number != Some(number) || !note.has_definition {
                        continue;
                    }
                    let body = note.body.clone();
                    block_render::write_footnote_definition_marker(out, number);
                    block_render::write_footnote_body(&mut cx!(self), &body, out, number);
                    break;
                }
                number += 1;
            }
        }
        self.footnotes.clear();
    }
}

fn drain_text(events: &RefCell<Vec<Event>>, out: &mut Vec<u8>) {
    if !out.is_empty() {
        events
            .borrow_mut()
            .push(Event::Text(tu::into_string(std::mem::take(out))));
    }
}
