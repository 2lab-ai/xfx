//! Setext headings, definition lists, and footnotes
//! (`assistant_presentation.zig:1783-2384`).

use std::cell::Cell;

use super::{contains, rule_only, text};
use crate::markdown::MarkdownProcessor;

#[test]
fn setext_headings_use_atx_styles_and_bypass_thematic_rule_completion() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();
    let calls = Cell::new(0usize);
    let mut rule = |_: &mut Vec<u8>| calls.set(calls.get() + 1);

    processor.push_with_completions("Workspace *overview*\n", &mut out, rule_only(&mut rule));
    assert_eq!(out.len(), 0);

    processor.push_with_completions(
        "===\nInstallation **guide**\n---\n",
        &mut out,
        rule_only(&mut rule),
    );
    assert_eq!(
        text(&out),
        concat!(
            "\x1b[1m\x1b[4mWorkspace \x1b[3moverview\x1b[23m\x1b[24m\x1b[22m\n",
            "\x1b[1mInstallation guide\x1b[22m\n",
        )
    );
    assert_eq!(calls.get(), 0);

    processor.push_with_completions("\n---\n", &mut out, rule_only(&mut rule));
    assert_eq!(calls.get(), 1);
}

#[test]
fn setext_candidate_flushes_at_eof_and_reset_discards_it() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();
    let calls = Cell::new(0usize);
    let mut rule = |_: &mut Vec<u8>| calls.set(calls.get() + 1);

    processor.push_with_completions("ordinary title\n", &mut out, rule_only(&mut rule));
    assert_eq!(out.len(), 0);
    processor.flush_with_completions(&mut out, rule_only(&mut rule));
    assert_eq!(text(&out), "ordinary title\n");

    out.clear();
    processor.push_with_completions("discarded title\n", &mut out, rule_only(&mut rule));
    processor.reset();
    processor.push_with_completions("---\n", &mut out, rule_only(&mut rule));
    assert_eq!(calls.get(), 1);
    assert_eq!(out.len(), 0);
}

#[test]
fn definition_lists_render_adjacent_markers_through_thematic_completion() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();
    let mut rule = |_: &mut Vec<u8>| {};

    processor.push_with_completions(
        "Status\n: **Running**\n: second definition\n",
        &mut out,
        rule_only(&mut rule),
    );
    processor.flush_with_completions(&mut out, rule_only(&mut rule));

    assert_eq!(
        text(&out),
        concat!(
            "Status\n",
            "\x1b[2m  \x1b[22m\x1b[1mRunning\x1b[22m\n",
            "\x1b[2m  \x1b[22msecond definition\n",
        )
    );
}

#[test]
fn definition_state_stops_before_a_fenced_code_block() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();
    let mut rule = |_: &mut Vec<u8>| {};

    processor.push_with_completions(
        "Status\n: accepted\n```\ncode\n```\n: stale\n",
        &mut out,
        rule_only(&mut rule),
    );
    processor.flush_with_completions(&mut out, rule_only(&mut rule));

    assert!(contains(&out, "\x1b[2m  \x1b[22maccepted\n"));
    assert!(contains(&out, "\n: stale\n"));
}

#[test]
fn definition_state_stops_before_a_pipe_table() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();
    let mut rule = |_: &mut Vec<u8>| {};

    processor.push_with_completions(
        "Status\n: accepted\n| Name |\n| --- |\n| value |\n: stale\n",
        &mut out,
        rule_only(&mut rule),
    );
    processor.flush_with_completions(&mut out, rule_only(&mut rule));

    assert!(contains(&out, "\x1b[2m  \x1b[22maccepted\n"));
    assert!(contains(&out, "\n: stale\n"));
}

#[test]
fn definition_marker_cannot_skip_a_fenced_code_block() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();
    let mut rule = |_: &mut Vec<u8>| {};

    processor.push_with_completions(
        "Status\n```\ncode\n```\n: stale\n",
        &mut out,
        rule_only(&mut rule),
    );
    processor.flush_with_completions(&mut out, rule_only(&mut rule));

    assert!(contains(&out, "Status\n"));
    assert!(contains(&out, "\n: stale\n"));
}

#[test]
fn definition_marker_cannot_skip_a_pipe_table() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();
    let mut rule = |_: &mut Vec<u8>| {};

    processor.push_with_completions(
        "Status\n| Name |\n| --- |\n| value |\n: stale\n",
        &mut out,
        rule_only(&mut rule),
    );
    processor.flush_with_completions(&mut out, rule_only(&mut rule));

    assert!(contains(&out, "Status\n"));
    assert!(contains(&out, "\n: stale\n"));
}

#[test]
fn definition_markers_without_a_term_stay_literal() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();
    let mut rule = |_: &mut Vec<u8>| {};

    processor.push_with_completions(
        ": orphan\n: second marker\n",
        &mut out,
        rule_only(&mut rule),
    );
    processor.flush_with_completions(&mut out, rule_only(&mut rule));

    assert_eq!(text(&out), ": orphan\n: second marker\n");
}

#[test]
fn definition_markers_require_a_separator_and_adjacency() {
    {
        let mut processor = MarkdownProcessor::new();
        let mut out = Vec::new();
        let mut rule = |_: &mut Vec<u8>| {};

        processor.push_with_completions(
            "Status\n:no-space\nEmpty\n: \t\nSeparated\n\n: body\n",
            &mut out,
            rule_only(&mut rule),
        );
        processor.flush_with_completions(&mut out, rule_only(&mut rule));
        assert_eq!(
            text(&out),
            "Status\n:no-space\nEmpty\n: \t\nSeparated\n\n: body\n"
        );
    }

    {
        let mut processor = MarkdownProcessor::new();
        let mut out = Vec::new();

        processor.push("Direct\n: body\n", &mut out);
        assert_eq!(text(&out), "Direct\n: body\n");
    }
}

#[test]
fn definition_bodies_preserve_lf_and_eof_hard_break_behavior() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();
    let mut rule = |_: &mut Vec<u8>| {};

    processor.push_with_completions("Status\n: line\\\n: eof\\", &mut out, rule_only(&mut rule));
    processor.flush_with_completions(&mut out, rule_only(&mut rule));

    assert_eq!(
        text(&out),
        concat!(
            "Status\n",
            "\x1b[2m  \x1b[22mline\n",
            "\x1b[2m  \x1b[22meof\\",
        )
    );
}

#[test]
fn definition_state_resets_with_the_processor() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();
    let mut rule = |_: &mut Vec<u8>| {};

    processor.push_with_completions("Status\n: accepted\n", &mut out, rule_only(&mut rule));
    processor.reset();
    out.clear();
    processor.push_with_completions(": stale\n", &mut out, rule_only(&mut rule));
    processor.flush_with_completions(&mut out, rule_only(&mut rule));

    assert_eq!(text(&out), ": stale\n");
}

#[test]
fn setext_lookahead_leaves_structural_predecessors_as_standalone_rules() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();
    let calls = Cell::new(0usize);
    let mut rule = |_: &mut Vec<u8>| calls.set(calls.get() + 1);

    processor.push_with_completions(
        "# ATX heading\n---\n- list item\n---\n> quoted text\n---\nordinary prose\n___\n",
        &mut out,
        rule_only(&mut rule),
    );

    assert_eq!(calls.get(), 4);
    assert_eq!(
        text(&out),
        concat!(
            "\x1b[1m\x1b[4mATX heading\x1b[24m\x1b[22m\n",
            "\x1b[2m\u{2022} \x1b[22mlist item\n",
            "\x1b[2m\u{2502} \x1b[22mquoted text\n",
            "ordinary prose\n",
        )
    );
}

#[test]
fn footnote_references_emit_dim_markers_and_defer_definitions() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push(
        concat!(
            "The cache is scoped to this request.[^cache]\n",
            "[^cache]: It is discarded after the request completes.\n",
        ),
        &mut out,
    );
    processor.flush(&mut out);

    assert_eq!(
        text(&out),
        concat!(
            "The cache is scoped to this request.\x1b[2m[1]\x1b[22m\n\n",
            "\x1b[2m[1] \x1b[22mIt is discarded after the request completes.\n",
        )
    );
}

#[test]
fn table_cell_footnote_references_project_deferred_definitions() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push(
        concat!(
            "| Component | State |\n",
            "| --- | --- |\n",
            "| api | [^state] |\n",
            "[^state]: The state comes from the deployment record.\n",
        ),
        &mut out,
    );
    processor.flush(&mut out);

    assert!(!contains(&out, "[^state]"));
    assert!(contains(&out, "\x1b[2m[1] \x1b[22m"));
    assert!(text(&out)
        .ends_with("\n\n\x1b[2m[1] \x1b[22mThe state comes from the deployment record.\n"));
}

#[test]
fn footnotes_normalize_the_final_separator_after_blank_source_lines() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push(
        concat!(
            "The cache is scoped to this request.[^cache]\n\n",
            "[^cache]: It is discarded after the request completes.\n",
        ),
        &mut out,
    );
    processor.flush(&mut out);

    assert_eq!(
        text(&out),
        concat!(
            "The cache is scoped to this request.\x1b[2m[1]\x1b[22m\n\n",
            "\x1b[2m[1] \x1b[22mIt is discarded after the request completes.\n",
        )
    );
}

#[test]
fn footnotes_use_first_reference_order_and_keep_the_first_definition() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push(
        concat!(
            "[^second]: The second definition arrives first.\n",
            "[^first]: The first definition arrives second.\n",
            "First[^first], second[^second], and first again[^first].\n",
            "[^first]: This duplicate definition is ignored.\n",
        ),
        &mut out,
    );
    processor.flush(&mut out);

    assert_eq!(
        text(&out),
        concat!(
            "First\x1b[2m[1]\x1b[22m, second\x1b[2m[2]\x1b[22m, and first again\x1b[2m[1]\x1b[22m.\n\n",
            "\x1b[2m[1] \x1b[22mThe first definition arrives second.\n",
            "\x1b[2m[2] \x1b[22mThe second definition arrives first.\n",
        )
    );
}

#[test]
fn footnote_definitions_format_multiline_bodies_and_retain_eof_separators() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push(
        concat!(
            "[^cache]: First **formatted** line.\n",
            "  Second continuation line.\n",
            "\tThird continuation line.\n",
            "The cache is scoped to this request.[^cache]",
        ),
        &mut out,
    );
    processor.flush(&mut out);

    assert_eq!(
        text(&out),
        concat!(
            "The cache is scoped to this request.\x1b[2m[1]\x1b[22m\n\n",
            "\x1b[2m[1] \x1b[22mFirst \x1b[1mformatted\x1b[22m line.\n",
            "\x1b[2m    \x1b[22mSecond continuation line.\n",
            "\x1b[2m    \x1b[22mThird continuation line.\n",
        )
    );
}

#[test]
fn escaped_malformed_and_code_footnote_candidates_remain_literal() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push(
        concat!(
            "\\[^escaped] `[^code]` [^] [^missing]:\n\n",
            "    [^block]: literal code\n",
        ),
        &mut out,
    );
    processor.flush(&mut out);

    assert!(!contains(&out, "[1]"));
    assert!(contains(&out, "[^escaped]"));
    assert!(contains(&out, "[^code]"));
    assert!(contains(&out, "[^missing]:"));
    assert!(contains(&out, "[^block]: literal code"));
}
