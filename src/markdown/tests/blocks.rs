//! Lists, task items, headings, fences, and blockquotes
//! (`assistant_presentation.zig:2388-2969`).

use std::cell::Cell;

use super::{contains, rule_only, text};
use crate::markdown::block_parse as bp;
use crate::markdown::MarkdownProcessor;

#[test]
fn heading_keeps_inline_emphasis_without_nested_bold_markers() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("# **Strong** *emphasis*\n", &mut out);
    assert_eq!(
        text(&out),
        "\x1b[1m\x1b[4mStrong \x1b[3memphasis\x1b[23m\x1b[24m\x1b[22m\n"
    );
}

#[test]
fn hash_without_trailing_space_is_literal() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("#notATag\n", &mut out);
    assert_eq!(text(&out), "#notATag\n");
}

#[test]
fn unordered_list_dash_gets_bullet() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("- first item\n", &mut out);
    assert_eq!(text(&out), "\x1b[2m\u{2022} \x1b[22mfirst item\n");
}

#[test]
fn unordered_list_asterisk_gets_bullet() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("* second item\n", &mut out);
    assert_eq!(text(&out), "\x1b[2m\u{2022} \x1b[22msecond item\n");
}

#[test]
fn unordered_list_literal_bullet_gets_dim_marker() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("\u{2022} third item\n  \u{2022} nested\n", &mut out);
    assert_eq!(
        text(&out),
        concat!(
            "\x1b[2m\u{2022} \x1b[22mthird item\n",
            "  \x1b[2m\u{2022} \x1b[22mnested\n",
        )
    );
}

#[test]
fn unordered_list_plus_marker_and_tab_separator_get_bullet() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("+ plus item\n-\ttabbed item\n+not a list\n", &mut out);
    assert_eq!(
        text(&out),
        concat!(
            "\x1b[2m\u{2022} \x1b[22mplus item\n",
            "\x1b[2m\u{2022} \x1b[22mtabbed item\n",
            "+not a list\n",
        )
    );
}

#[test]
fn ordered_list_accepts_paren_markers_and_rejects_long_numbers() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("1) first\n12) twelfth\n1234567890. too long\n", &mut out);
    assert_eq!(
        text(&out),
        concat!(
            "\x1b[2m1)\x1b[22m first\n",
            "\x1b[2m12)\x1b[22m twelfth\n",
            "1234567890. too long\n",
        )
    );
}

#[test]
fn atx_heading_strips_closing_hashes_and_allows_small_indent() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push(
        "## Title ##\n   ## Indented\n## Keep#\n## Trail ##   \n    ## code\n",
        &mut out,
    );
    assert_eq!(
        text(&out),
        concat!(
            "\x1b[1mTitle\x1b[22m\n",
            "\x1b[1mIndented\x1b[22m\n",
            "\x1b[1mKeep#\x1b[22m\n",
            "\x1b[1mTrail\x1b[22m\n",
            "    ## code\n",
        )
    );
}

#[test]
fn longer_code_fence_contains_shorter_fences() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("````md\n```zig\ninner\n```\n````\nafter\n", &mut out);
    assert_eq!(
        text(&out),
        concat!(
            "\x1b[2m\u{2502} \x1b[22m```zig\n",
            "\x1b[2m\u{2502} \x1b[22minner\n",
            "\x1b[2m\u{2502} \x1b[22m```\n",
            "after\n",
        )
    );
    assert_eq!(processor.code_fence, None);
}

#[test]
fn code_fence_language_skips_the_whole_marker_run() {
    assert_eq!(bp::code_fence_language(b"````md"), b"md");
    assert_eq!(bp::code_fence_language(b"```   zig extra"), b"zig");
    assert_eq!(bp::code_fence_language(b"~~~"), b"");
}

#[test]
fn closing_fence_needs_matching_marker_length_and_nothing_after_it() {
    let open = bp::CodeFence {
        marker: b'`',
        run: 4,
        indent: 0,
    };
    assert!(bp::closes_code_fence(b"````", open));
    assert!(bp::closes_code_fence(b"`````  ", open));
    assert!(!bp::closes_code_fence(b"```", open));
    assert!(!bp::closes_code_fence(b"~~~~", open));
    assert!(!bp::closes_code_fence(b"```` trailing", open));
}

#[test]
fn fenced_code_inside_a_list_item_drops_the_item_indentation() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push(
        "1. step\n   ```sh\n   ls -la\n     nested\n   ```\n- next\n",
        &mut out,
    );
    assert_eq!(
        text(&out),
        concat!(
            "\x1b[2m1.\x1b[22m step\n",
            "\x1b[2m\u{2502} \x1b[22mls -la\n",
            "\x1b[2m\u{2502} \x1b[22m  nested\n",
            "\x1b[2m\u{2022} \x1b[22mnext\n",
        )
    );
}

#[test]
fn tab_indented_fence_inside_a_list_item_closes_on_a_tab_indented_fence() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("1. item\n\t```\n\tcode\n\t```\n\tprose after\n", &mut out);
    assert_eq!(
        text(&out),
        concat!(
            "\x1b[2m1.\x1b[22m item\n",
            "\x1b[2m\u{2502} \x1b[22mcode\n",
            "\tprose after\n",
        )
    );
}

// Known cost of the plus rule: a loose nested list written as "- a", a blank
// line, then "    + nested" renders the nested line as indented code, while
// "-" or "*" at the same indent still become list items.
#[test]
fn indented_plus_line_after_a_blank_stays_indented_code() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push(
        "    + value\n    second\ntext\n- a\n    + nested\n",
        &mut out,
    );
    processor.flush(&mut out);
    assert_eq!(
        text(&out),
        concat!(
            "\x1b[2m\u{2502} \x1b[22m+ value\n",
            "\x1b[2m\u{2502} \x1b[22msecond\n",
            "text\n",
            "\x1b[2m\u{2022} \x1b[22ma\n",
            "    \x1b[2m\u{2022} \x1b[22mnested\n",
        )
    );
}

#[test]
fn heading_keeps_a_backslash_exposed_by_removing_closing_hashes() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push(
        "## C:\\ ###\n## trailing\\\n> ## C:\\ ###\n> ## quoted\\\n",
        &mut out,
    );
    assert_eq!(
        text(&out),
        concat!(
            "\x1b[1mC:\\\x1b[22m\n",
            "\x1b[1mtrailing\x1b[22m\n",
            "\x1b[2m\u{2502} \x1b[22m\x1b[1mC:\\\x1b[22m\n",
            "\x1b[2m\u{2502} \x1b[22m\x1b[1mquoted\x1b[22m\n",
        )
    );
}

#[test]
fn blockquote_renders_headings_and_list_items_inside_the_quote() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push(
        "> ## Note\n> - one\n> 2. two\n> - [x] done\n> plain\n",
        &mut out,
    );
    assert_eq!(
        text(&out),
        concat!(
            "\x1b[2m\u{2502} \x1b[22m\x1b[1mNote\x1b[22m\n",
            "\x1b[2m\u{2502} \x1b[22m\x1b[2m\u{2022} \x1b[22mone\n",
            "\x1b[2m\u{2502} \x1b[22m\x1b[2m2.\x1b[22m two\n",
            "\x1b[2m\u{2502} \x1b[22m\x1b[38;5;252m\u{2713}\x1b[39m done\n",
            "\x1b[2m\u{2502} \x1b[22mplain\n",
        )
    );
}

#[test]
fn literal_bullet_without_trailing_space_stays_prose() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("\u{2022}item\n\u{2022}\n", &mut out);
    assert_eq!(text(&out), "\u{2022}item\n\u{2022}\n");
}

#[test]
fn ordered_list_keeps_number_marker() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("1. step one\n", &mut out);
    assert_eq!(text(&out), "\x1b[2m1.\x1b[22m step one\n");
}

#[test]
fn task_list_items_render_static_pending_and_completed_markers() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push(
        concat!(
            "- [ ] pending **task**\n",
            "- [x] done\n",
            "  * [X] nested done\n",
            "- [ ]\n",
        ),
        &mut out,
    );
    assert_eq!(
        text(&out),
        concat!(
            "\x1b[2m\u{2610} \x1b[22mpending \x1b[1mtask\x1b[22m\n",
            "\x1b[38;5;252m\u{2713}\x1b[39m done\n",
            "  \x1b[38;5;252m\u{2713}\x1b[39m nested done\n",
            "\x1b[2m\u{2610}\x1b[22m\n",
        )
    );
}

#[test]
fn ordered_task_list_items_retain_their_number_markers() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("1. [ ] first\n2. [X] done\n", &mut out);
    assert_eq!(
        text(&out),
        concat!(
            "\x1b[2m1.\x1b[22m \x1b[2m\u{2610} \x1b[22mfirst\n",
            "\x1b[2m2.\x1b[22m \x1b[38;5;252m\u{2713}\x1b[39m done\n",
        )
    );
}

#[test]
fn task_list_syntax_remains_literal_outside_its_bounded_grammar() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push(
        concat!(
            "- [x]done\n",
            "- [-] unsupported\n",
            "- [ ]pending\n",
            "ordinary [x] prose\n",
            "```text\n",
            "- [x] literal code\n",
            "```\n",
        ),
        &mut out,
    );
    assert_eq!(
        text(&out),
        concat!(
            "\x1b[2m\u{2022} \x1b[22m[x]done\n",
            "\x1b[2m\u{2022} \x1b[22m[-] unsupported\n",
            "\x1b[2m\u{2022} \x1b[22m[ ]pending\n",
            "ordinary [x] prose\n",
            "\x1b[2m\u{2502} \x1b[22m- [x] literal code\n",
        )
    );
}

#[test]
fn task_list_prefix_is_retained_across_streamed_input_chunks() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("- [x]", &mut out);
    assert_eq!(out.len(), 0);
    processor.push(" streamed task\n", &mut out);
    assert_eq!(text(&out), "\x1b[38;5;252m\u{2713}\x1b[39m streamed task\n");
}

#[test]
fn blockquote_gets_a_dim_vertical_rule_and_renders_inline_markdown() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("  > **important** note\n", &mut out);
    assert_eq!(
        text(&out),
        "  \x1b[2m\u{2502} \x1b[22m\x1b[1mimportant\x1b[22m note\n"
    );
}

#[test]
fn lazy_nested_blockquote_continuations_retain_their_rules() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("> > first **quoted** line\n", &mut out);
    processor.push(
        "lazy second line\n> > explicit third line\nlazy fourth line\n\noutside\n",
        &mut out,
    );
    assert_eq!(
        text(&out),
        concat!(
            "\x1b[2m\u{2502} \x1b[22m\x1b[2m\u{2502} \x1b[22mfirst \x1b[1mquoted\x1b[22m line\n",
            "\x1b[2m\u{2502} \x1b[22m\x1b[2m\u{2502} \x1b[22mlazy second line\n",
            "\x1b[2m\u{2502} \x1b[22m\x1b[2m\u{2502} \x1b[22mexplicit third line\n",
            "\x1b[2m\u{2502} \x1b[22m\x1b[2m\u{2502} \x1b[22mlazy fourth line\n\n",
            "outside\n",
        )
    );
}

#[test]
fn lazy_blockquote_continuations_retain_inline_links() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push(
        "> source\nlazy [link](https://example.com/lazy)\n",
        &mut out,
    );
    assert!(contains(&out, "\n\x1b[2m\u{2502} \x1b[22mlazy "));
    assert!(contains(&out, "\x1b]8;id=fx-"));
    assert!(contains(&out, "https://example.com/lazy"));
}

#[test]
fn lazy_blockquotes_stop_before_structural_rows_and_preserve_eof_backslashes() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();
    let calls = Cell::new(0usize);
    let mut rule = |_: &mut Vec<u8>| calls.set(calls.get() + 1);

    processor.push_with_completions(
        "> quote\nlazy continuation\n===\n---\noutside\n> quote again\n- list item\n> quote at EOF\nlazy EOF\\",
        &mut out,
        rule_only(&mut rule),
    );
    processor.flush_with_completions(&mut out, rule_only(&mut rule));
    assert_eq!(
        text(&out),
        concat!(
            "\x1b[2m\u{2502} \x1b[22mquote\n",
            "\x1b[2m\u{2502} \x1b[22mlazy continuation\n",
            "\x1b[2m\u{2502} \x1b[22m===\n",
            "outside\n",
            "\x1b[2m\u{2502} \x1b[22mquote again\n",
            "\x1b[2m\u{2022} \x1b[22mlist item\n",
            "\x1b[2m\u{2502} \x1b[22mquote at EOF\n",
            "\x1b[2m\u{2502} \x1b[22mlazy EOF\\",
        )
    );
    assert_eq!(calls.get(), 1);
}

#[test]
fn lazy_blockquotes_end_before_headings_and_fences() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();
    let mut rule = |_: &mut Vec<u8>| {};

    processor.push_with_completions(
        "> quote before heading\n# heading\nheading outside\n> quote before fence\n```zig\ncode\n```\nafter fence\n",
        &mut out,
        rule_only(&mut rule),
    );
    processor.flush_with_completions(&mut out, rule_only(&mut rule));
    assert_eq!(
        text(&out),
        concat!(
            "\x1b[2m\u{2502} \x1b[22mquote before heading\n",
            "\x1b[1m\x1b[4mheading\x1b[24m\x1b[22m\n",
            "heading outside\n",
            "\x1b[2m\u{2502} \x1b[22mquote before fence\n",
            "\x1b[2m\u{2502} \x1b[22mcode\n",
            "after fence\n",
        )
    );
}

#[test]
fn reset_clears_lazy_blockquote_state() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("> quote\n", &mut out);
    processor.reset();
    processor.push("next turn\n", &mut out);
    assert_eq!(text(&out), "\x1b[2m\u{2502} \x1b[22mquote\nnext turn\n");
}

#[test]
fn flushing_ends_lazy_blockquote_state() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("> quote\n", &mut out);
    processor.flush(&mut out);
    processor.push("next stream\n", &mut out);
    assert_eq!(text(&out), "\x1b[2m\u{2502} \x1b[22mquote\nnext stream\n");
}

#[test]
fn nested_blockquotes_render_every_valid_marker_and_preserve_malformed_inner_syntax() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push(
        concat!(
            "  > > **deep** note\n",
            "> > > third depth\n",
            "> >\n",
            ">> compact\n",
            "> >not\n",
        ),
        &mut out,
    );
    assert_eq!(
        text(&out),
        concat!(
            "  \x1b[2m\u{2502} \x1b[22m\x1b[2m\u{2502} \x1b[22m\x1b[1mdeep\x1b[22m note\n",
            "\x1b[2m\u{2502} \x1b[22m\x1b[2m\u{2502} \x1b[22m\x1b[2m\u{2502} \x1b[22mthird depth\n",
            "\x1b[2m\u{2502} \x1b[22m\x1b[2m\u{2502} \x1b[22m\n",
            ">> compact\n",
            "\x1b[2m\u{2502} \x1b[22m>not\n",
        )
    );
}

#[test]
fn nested_blockquote_remains_buffered_until_its_terminating_newline() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("> > **chunked", &mut out);
    assert_eq!(out.len(), 0);
    processor.push(" nested**\n", &mut out);
    assert_eq!(
        text(&out),
        "\x1b[2m\u{2502} \x1b[22m\x1b[2m\u{2502} \x1b[22m\x1b[1mchunked nested\x1b[22m\n"
    );
}

#[test]
fn blockquote_without_a_marker_separator_stays_literal() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push(">not a blockquote\n", &mut out);
    assert_eq!(text(&out), ">not a blockquote\n");
}

#[test]
fn code_fence_toggles_block_state() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("```zig\nconst x = 1;\n```\n", &mut out);
    assert_eq!(text(&out), "\x1b[2m\u{2502} \x1b[22mconst x = 1;\n");
    assert!(!processor.in_code_block);
}

#[test]
fn code_block_preserves_inline_markers_literally() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("```\nfoo **bar** baz\n```\n", &mut out);
    assert_eq!(text(&out), "\x1b[2m\u{2502} \x1b[22mfoo **bar** baz\n");
}

#[test]
fn tilde_code_fence_keeps_literal_code_and_ignores_backtick_fence() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("~~~zig\nconst value = **1**;\n```\n~~~\n", &mut out);

    assert_eq!(
        text(&out),
        concat!(
            "\x1b[2m\u{2502} \x1b[22mconst value = **1**;\n",
            "\x1b[2m\u{2502} \x1b[22m```\n",
        )
    );
    assert!(!processor.in_code_block);
}
