//! Upstream's presenter tests (`assistant_presentation.zig:86-3935`), ported
//! with their inputs and expected bytes unchanged.
//!
//! Upstream reads OSC 8 link ids off one process-wide counter, so its tests
//! capture the counter before rendering; here each test reads its own
//! presenter's counter the same way. A completion upstream passes as a
//! `ctx` plus a function pointer is a closure here, and a capture that
//! upstream mutates through `ctx` is a `Cell` the closure updates.
//!
//! Four upstream tests are not ported because they exercise allocation
//! failure, which has no counterpart here: `setext lookahead frees
//! allocation failures` (:2176), `definition lists free allocation failures`
//! (:2213), `footnotes free allocation failures` (:2380), and `code block
//! payload clone frees language if code allocation fails` (:3223).

use super::payload::{CodeBlockCompletion, ThematicRuleCompletion};
use super::MarkdownCompletions;

mod blocks;
mod code_tables;
mod events;
mod inline;
mod links;
mod setext_footnotes;

/// The rendered bytes as text; every expected value in these tests is UTF-8.
fn text(out: &[u8]) -> &str {
    std::str::from_utf8(out).expect("presenter output is UTF-8")
}

fn contains(out: &[u8], needle: &str) -> bool {
    text(out).contains(needle)
}

/// Only the thematic rule completion, which also enables Setext headings and
/// definition lists.
fn rule_only<'a>(rule: &'a mut ThematicRuleCompletion<'a>) -> MarkdownCompletions<'a> {
    MarkdownCompletions {
        thematic_rule: Some(rule),
        ..MarkdownCompletions::default()
    }
}

/// Only the code block completion.
fn code_only<'a>(code: &'a mut CodeBlockCompletion<'a>) -> MarkdownCompletions<'a> {
    MarkdownCompletions {
        code: Some(code),
        ..MarkdownCompletions::default()
    }
}
