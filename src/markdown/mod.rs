//! The streaming markdown presenter for assistant text.
//!
//! A behavioral port of upstream's `src/core/agent/assistant_presentation.zig`
//! and its `presentation/` helpers, plus `src/core/shared/display_width.zig`,
//! at vercel-labs/fx `c1db9191`. Upstream's tests are ported beside the code
//! with their inputs and expected bytes unchanged.
//!
//! [`MarkdownProcessor`] takes model text in whatever chunks it arrives and
//! turns it into terminal bytes: ANSI styling for emphasis, headings, lists,
//! quotes, and inline code, OSC 8 hyperlinks for links, and aligned tables.
//! Prose streams line by line. A code block, a table, and a thematic rule are
//! held until complete; [`MarkdownProcessor::feed`] returns them as
//! [`Event`]s so the caller can lay them out itself, which is how upstream's
//! streaming caller uses the presenter. [`render_code_block_payload`] and
//! [`render_table_payload`] are the presenter's own inline rendering of those
//! payloads.
//!
//! Upstream keeps three pieces of process-wide mutable state here: the
//! themed escape slots, the active theme, and the OSC 8 link id counter.
//! None of them is global in this port. The colours live in a [`Styles`]
//! value each presenter owns, and the link counter lives in the presenter
//! (or is passed explicitly, for [`parse_table_payload`]), so a caller that
//! wants one id sequence across several presenters hands the counter along
//! with [`MarkdownProcessor::set_next_link_id`].

pub mod ansi;
pub mod code_highlight;
pub mod code_highlight_languages;
pub mod display_width;
pub mod theme;

mod block_parse;
mod block_render;
mod inline_render;
mod payload;
mod processor;
mod text_util;
mod unicode_classes;
mod unicode_display_data;

pub use ansi::{write_horizontal_rule, Styles};
pub use block_render::{render_code_block_payload, render_table_payload, write_table_header_cell};
pub use display_width::{visible_width, visible_width_ignoring_ansi};
pub use payload::{
    CodeBlockCompletion, CodeBlockPayload, MarkdownCompletions, TableColumnAlign, TableCompletion,
    TablePayload, TableRow, ThematicRuleCompletion,
};
pub use processor::{parse_table_payload, Event, MarkdownProcessor};
pub use theme::Theme;

#[cfg(test)]
mod tests;
