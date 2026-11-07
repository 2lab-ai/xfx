//! Code block completions, line intake, emphasis matching, tables, and
//! thematic rules (`assistant_presentation.zig:2970-3935`).

use std::cell::Cell;

use super::{assert_renders_in_linear_time, code_only, contains, rule_only, text};
use crate::markdown::ansi::{DIM_CLOSE, DIM_OPEN, HORIZONTAL_RULE_WIDTH, TABLE_HORIZ};
use crate::markdown::block_parse::is_pipe_line;
use crate::markdown::text_util as tu;
use crate::markdown::{
    parse_table_payload, CodeBlockPayload, MarkdownProcessor, Styles, TableColumnAlign,
};

#[test]
fn tilde_code_fence_completion_retains_language_and_literal_source() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();
    let mut captured: Option<CodeBlockPayload> = None;
    let mut code = |block: CodeBlockPayload, _: &mut Vec<u8>| captured = Some(block);

    processor.push_with_completions(
        "Before block.\n~~~  Zig example\n  const value = **1**;\n```\n~~~\nAfter block.\n",
        &mut out,
        code_only(&mut code),
    );

    assert_eq!(text(&out), "Before block.\nAfter block.\n");
    let block = captured.expect("a completed code block");
    assert_eq!(block.language, "Zig");
    assert_eq!(block.code, "  const value = **1**;\n```\n");
}

#[test]
fn unterminated_tilde_code_fence_flushes_its_semantic_payload() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();
    let mut captured: Option<CodeBlockPayload> = None;
    let mut code = |block: CodeBlockPayload, _: &mut Vec<u8>| captured = Some(block);

    processor.push_with_completions("~~~text\nlast line", &mut out, code_only(&mut code));
    processor.flush_with_completions(&mut out, code_only(&mut code));

    let block = captured.expect("a completed code block");
    assert_eq!(block.language, "text");
    assert_eq!(block.code, "last line\n");
}

#[test]
fn markdown_completion_normalizes_crlf_across_chunks_before_capturing_code_fences() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();
    let mut captured: Option<CodeBlockPayload> = None;
    let mut code = |block: CodeBlockPayload, _: &mut Vec<u8>| captured = Some(block);

    processor.push_with_completions(
        "Before block.\r\n```  Zig example\r",
        &mut out,
        code_only(&mut code),
    );
    processor.push_with_completions(
        "\n  const value = **1**;\r\n```\r\nAfter block.\r\n",
        &mut out,
        code_only(&mut code),
    );

    assert_eq!(text(&out), "Before block.\nAfter block.\n");
    let block = captured.expect("a completed code block");
    assert_eq!(block.language, "Zig");
    assert_eq!(block.code, "  const value = **1**;\n");
}

#[test]
fn indented_code_completion_deindents_literal_lines_and_reprocesses_its_terminator() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();
    let mut captured: Option<CodeBlockPayload> = None;
    let mut code = |block: CodeBlockPayload, _: &mut Vec<u8>| captured = Some(block);

    processor.push_with_completions(
        "\n    | literal | pipe |\n    ```zig\n    const value = **1**;\n        nested\nplain text\n",
        &mut out,
        code_only(&mut code),
    );

    let block = captured.expect("a completed code block");
    assert_eq!(block.language, "");
    assert_eq!(
        block.code,
        "| literal | pipe |\n```zig\nconst value = **1**;\n    nested\n"
    );
    assert_eq!(text(&out), "\nplain text\n");
    assert!(!processor.in_code_block);
}

#[test]
fn indented_code_completion_accepts_a_response_leading_tab_and_flushes_at_eof() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();
    let mut captured: Option<CodeBlockPayload> = None;
    let mut code = |block: CodeBlockPayload, _: &mut Vec<u8>| captured = Some(block);

    processor.push_with_completions("\t  tail", &mut out, code_only(&mut code));
    processor.flush_with_completions(&mut out, code_only(&mut code));

    let block = captured.expect("a completed code block");
    assert_eq!(block.language, "");
    assert_eq!(block.code, "  tail\n");
    assert_eq!(out.len(), 0);
}

#[test]
fn indented_code_requires_a_blank_boundary_and_yields_to_list_and_lazy_quote_content() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();
    let mut captured: Option<CodeBlockPayload> = None;
    let mut code = |block: CodeBlockPayload, _: &mut Vec<u8>| captured = Some(block);

    processor.push_with_completions(
        "paragraph\n    continuation\n\n   three spaces\n\n    - list item\n> quote\n    lazy continuation\n\nafter quote\n",
        &mut out,
        code_only(&mut code),
    );

    assert!(captured.is_none());
    assert!(contains(&out, "    continuation"));
    assert!(contains(&out, "   three spaces"));
    assert!(contains(
        &out,
        "\x1b[2m\u{2502} \x1b[22m    lazy continuation"
    ));
    assert!(!contains(&out, "\x1b[2m\u{2502} \x1b[22m- list item"));
}

#[test]
fn reset_clears_an_unfinished_indented_code_block() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("    stale code\n", &mut out);
    assert!(processor.in_code_block);

    processor.reset();
    assert!(!processor.in_code_block);
    processor.push("after reset\n", &mut out);

    assert_eq!(
        text(&out),
        "\x1b[2m\u{2502} \x1b[22mstale code\nafter reset\n"
    );
}

#[test]
fn markdown_completion_flushes_an_unterminated_code_fence() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();
    let mut captured: Option<CodeBlockPayload> = None;
    let mut code = |block: CodeBlockPayload, _: &mut Vec<u8>| captured = Some(block);

    processor.push_with_completions("```text\nlast line", &mut out, code_only(&mut code));
    processor.flush_with_completions(&mut out, code_only(&mut code));

    let block = captured.expect("a completed code block");
    assert_eq!(block.language, "text");
    assert_eq!(block.code, "last line\n");
}

#[test]
fn line_intake_buffers_partial_lines_and_preserves_standalone_carriage_returns() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push(">", &mut out);
    assert_eq!(out.len(), 0);

    processor.push("\n", &mut out);
    assert_eq!(text(&out), "\x1b[2m\u{2502} \x1b[22m\n");

    out.clear();
    processor.push("**bol", &mut out);
    assert_eq!(out.len(), 0);
    processor.push("d** rest\n", &mut out);
    assert_eq!(text(&out), "\x1b[1mbold\x1b[22m rest\n");

    out.clear();
    processor.push("run `zig", &mut out);
    assert_eq!(out.len(), 0);
    processor.push(" build` now\n", &mut out);
    assert_eq!(text(&out), "run \x1b[38;5;255mzig build\x1b[39m now\n");

    out.clear();
    processor.push("left\rright\n", &mut out);
    assert_eq!(text(&out), "left\rright\n");
}

#[test]
fn flush_emits_pending_line_without_newline() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("partial", &mut out);
    processor.flush(&mut out);
    assert_eq!(text(&out), "partial");
}

#[test]
fn flush_keeps_an_unmatched_emphasis_opener_literal() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("oops **never closed", &mut out);
    processor.flush(&mut out);
    assert_eq!(text(&out), "oops **never closed");
}

#[test]
fn flush_keeps_an_unpaired_backtick_literal() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("run `zig build", &mut out);
    processor.flush(&mut out);
    assert_eq!(text(&out), "run `zig build");
}

#[test]
fn emphasis_pairs_by_the_delimiter_stack_and_unmatched_runs_stay_literal() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push(
        concat!(
            "*not a list item\n",
            "**bold** then **open\n",
            "~~gone~~ and ~~stays\n",
            "a ` b **bold**\n",
            "**bold with `code` inside** *it*\n",
            "***both*** and **`code`**\n",
            "See *italic** plain\n",
            "text **** here **bold** and ~~~~ then ~~gone~~ and *** end\n",
            "**a __b__ c** and *x _y_ z*\n",
            "foo*bar*baz and snake_case_name and 3 * 5 = 15\n",
        ),
        &mut out,
    );
    assert_eq!(
        text(&out),
        concat!(
            "*not a list item\n",
            "\x1b[1mbold\x1b[22m then **open\n",
            "\x1b[9mgone\x1b[29m and ~~stays\n",
            "a ` b \x1b[1mbold\x1b[22m\n",
            "\x1b[1mbold with \x1b[38;5;255mcode\x1b[39m inside\x1b[22m \x1b[3mit\x1b[23m\n",
            "\x1b[3m\x1b[1mboth\x1b[22m\x1b[23m and \x1b[1m\x1b[38;5;255mcode\x1b[39m\x1b[22m\n",
            "See \x1b[3mitalic\x1b[23m* plain\n",
            "text **** here \x1b[1mbold\x1b[22m and ~~~~ then \x1b[9mgone\x1b[29m and *** end\n",
            "\x1b[1ma \x1b[1mb\x1b[22m\x1b[1m c\x1b[22m and \x1b[3mx \x1b[3my\x1b[23m\x1b[3m z\x1b[23m\n",
            "foo\x1b[3mbar\x1b[23mbaz and snake_case_name and 3 * 5 = 15\n",
        )
    );
}

#[test]
fn emphasis_lookahead_agrees_with_links_code_spans_and_bare_urls() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    // A backtick inside link text is not a code opener and a star inside a
    // code span is not a closer.
    processor.push(
        "[use `](https://example.com) **bold** `code`\n[use `](https://example.com) *open `code*`\n",
        &mut out,
    );
    let first = tu::nth_line(&out, 0).expect("first line");
    assert!(first.ends_with(b" \x1b[1mbold\x1b[22m \x1b[38;5;255mcode\x1b[39m"));
    let second = tu::nth_line(&out, 1).expect("second line");
    assert!(second.ends_with(b" *open \x1b[38;5;255mcode*\x1b[39m"));

    // Emphasis around a multi backtick code span still pairs.
    out.clear();
    processor.push("See _a ``b`c`` d_ end\n", &mut out);
    assert_eq!(
        text(&out),
        "See \x1b[3ma \x1b[38;5;255mb`c\x1b[39m d\x1b[23m end\n"
    );

    // URL punctuation is never read as markup, and a delimiter that ends a
    // URL still closes the span around it.
    out.clear();
    processor.push("https://example.com/`x*y` **bold** `code`\n", &mut out);
    assert!(contains(&out, ";https://example.com/`x*y`\x1b\\"));
    assert!(text(&out).ends_with(" \x1b[1mbold\x1b[22m \x1b[38;5;255mcode\x1b[39m\n"));
    // A trailing delimiter is trimmed from the URL and closes the span.
    out.clear();
    processor.push("*see https://example.com/a* and `code`\n", &mut out);
    assert!(text(&out).starts_with("\x1b[3msee \x1b]8;id=fx-"));
    assert!(text(&out).ends_with(
        ";https://example.com/a\x1b\\\x1b[38;5;75m\x1b[4mhttps://example.com/a\x1b[24m\x1b[39m\x1b]8;;\x1b\\\x1b[23m and \x1b[38;5;255mcode\x1b[39m\n"
    ));
    // Like a GFM autolink, a URL extends to the next whitespace regardless
    // of active styles, so interior punctuation belongs to the URL and the
    // rest of the line is read after it.
    out.clear();
    processor.push(
        "*https://example.com/a*`some code` **bold** `x`\n",
        &mut out,
    );
    assert!(text(&out).starts_with("*\x1b]8;"));
    assert!(contains(&out, ";https://example.com/a*`some\x1b\\"));
    assert!(text(&out).ends_with(" code\x1b[38;5;255m**bold**\x1b[39mx`\n"));
    out.clear();
    processor.push("text **** https://example.com\n", &mut out);
    assert!(text(&out).starts_with("text **** \x1b]8;"));
}

#[test]
fn emphasis_flanking_reads_neighbouring_code_points_not_bytes() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    // An em dash, curly quotes, and a fullwidth comma are punctuation, so
    // the marker beside them opens or closes; CJK letters are word
    // characters, so a star still delimits between them while an underscore
    // does not. Letters outside ASCII such as the micro sign are still
    // letters, so an underscore between them stays intraword.
    processor.push("a\u{b5}_b_ and x\u{aa}_y_ and \u{3031}_z_\n", &mut out);
    assert_eq!(text(&out), "a\u{b5}_b_ and x\u{aa}_y_ and \u{3031}_z_\n");
    out.clear();
    processor.push(
        "a\u{2014}_b_ \u{201c}*q*\u{201d} \u{4e2d}*\u{5f37}*\u{8abf} \u{4e2d}_\u{5f37}_\u{8abf} **x**\u{ff0c}\n",
        &mut out,
    );
    assert_eq!(
        text(&out),
        "a\u{2014}\x1b[3mb\x1b[23m \u{201c}\x1b[3mq\x1b[23m\u{201d} \u{4e2d}\x1b[3m\u{5f37}\x1b[23m\u{8abf} \u{4e2d}_\u{5f37}_\u{8abf} \x1b[1mx\x1b[22m\u{ff0c}\n"
    );
}

#[test]
fn heading_with_unmatched_strong_marker_keeps_it_literal() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("## 2 ** 8 and **strong** and __also__\n", &mut out);
    assert_eq!(text(&out), "\x1b[1m2 ** 8 and strong and also\x1b[22m\n");
}

#[test]
fn long_lines_of_unmatched_or_unbalanced_delimiters_render_in_linear_time() {
    // Deeply nested successful pairs must not re-walk consumed ranges.
    let out = assert_renders_in_linear_time("nested pairs", 32_000, |n| {
        let mut line = "*a ".repeat(n);
        line.push_str(&"b* ".repeat(n));
        line.push('\n');
        line
    });
    assert!(text(&out).starts_with("\x1b[3ma \x1b[3ma "));
    // A closer that matches part of its run and then fails to place the rest
    // must cache that failed remainder instead of rescanning other openers.
    let out = assert_renders_in_linear_time("residual closers", 32_000, |n| {
        let mut line = "*a ".repeat(n);
        line.push_str(&"_b c__ ".repeat(n));
        line.push('\n');
        line
    });
    assert!(contains(&out, "\x1b[3mb c\x1b[23m_ "));

    let shapes: [(&str, &str, usize, &str); 11] = [
        ("", "*a _b ~~c ", 40_000, "\n"),
        ("", "a* b_ c~~ ", 40_000, "\n"),
        ("", "*a https://example.com/b _c ~~d ", 20_000, "\n"),
        ("text ", "*", 64 * 1024, "x\n"),
        ("https://example.com ", "*", 64 * 1024, "\n"),
        ("*a", "*", 64 * 1024, "x\n"),
        ("", "[", 64 * 1024, "\n"),
        ("", "![", 32 * 1024, "\n"),
        ("", "[", 64 * 1024, "]\n"),
        ("", "![", 32 * 1024, "]\n"),
        ("", "[^", 32 * 1024, "]\n"),
    ];
    for (prefix, unit, repeat, suffix) in shapes {
        let label = format!("{prefix:?}{unit:?}..{suffix:?}");
        assert_renders_in_linear_time(&label, repeat, |n| {
            let mut line = String::from(prefix);
            line.push_str(&unit.repeat(n));
            line.push_str(suffix);
            line
        });
    }
}

#[test]
fn header_with_inline_markdown() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("## Using `fx` in CI\n", &mut out);
    assert_eq!(
        text(&out),
        "\x1b[1mUsing \x1b[38;5;255mfx\x1b[39m in CI\x1b[22m\n"
    );
}

#[test]
fn inline_code_ignores_bold_markers_inside() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("use `**literal**` here\n", &mut out);
    assert_eq!(text(&out), "use \x1b[38;5;255m**literal**\x1b[39m here\n");
}

#[test]
fn pipe_table_header_separator_has_junction_aligned_with_column_pipes() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    let input = concat!(
        "| Name | Age |\n",
        "|------|-----|\n",
        "| Ana  | 30  |\n",
        "| Bob  | 7   |\n",
    );
    processor.push(input, &mut out);
    processor.flush(&mut out);

    let expected = concat!(
        "\x1b[1mName\x1b[22m \u{2502} \x1b[1mAge\x1b[22m\n",
        "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{253c}\u{2500}\u{2500}\u{2500}\u{2500}\n",
        "Ana  \u{2502} 30 \n",
        "Bob  \u{2502} 7  \n",
    );
    assert_eq!(text(&out), expected);
}

#[test]
fn single_column_pipe_table_has_no_junction_on_separator() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    let input = concat!("| Header |\n", "|--------|\n", "| data   |\n");
    processor.push(input, &mut out);
    processor.flush(&mut out);

    assert!(!contains(&out, "\u{253c}"));
    assert!(contains(
        &out,
        "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}"
    ));
}

#[test]
fn pipe_table_with_markdown_in_cells_aligns_dividers_across_all_rows() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    let input = concat!(
        "| Aspect | Value |\n",
        "|--------|-------|\n",
        "| **bold** | x |\n",
        "| plain    | y |\n",
    );
    processor.push(input, &mut out);
    processor.flush(&mut out);

    let expected = concat!(
        "\x1b[1mAspect\x1b[22m \u{2502} \x1b[1mValue\x1b[22m\n",
        "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{253c}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\n",
        "\x1b[1mbold\x1b[22m   \u{2502} x    \n",
        "plain  \u{2502} y    \n",
    );
    assert_eq!(text(&out), expected);
}

#[test]
fn escaped_pipes_stay_inside_table_cells_while_even_backslashes_retain_delimiters() {
    let mut link_id = 1;
    let table = parse_table_payload(
        concat!(
            "| Command | Result |\n",
            "| --- | --- |\n",
            "| printf \\| grep | works |\n",
        ),
        &Styles::default(),
        &mut link_id,
    );

    assert_eq!(table.column_count, 2);
    assert_eq!(table.rows[1].cells[0], "printf | grep");
    assert!(!is_pipe_line(b"literal \\| pipe"));
    assert!(is_pipe_line(b"literal \\\\| delimiter"));
}

#[test]
fn three_column_pipe_table_aligns_junctions_with_every_column_pipe() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    let input = concat!(
        "| A | B  | C   |\n",
        "|---|----|-----|\n",
        "| 1 | 22 | 333 |\n"
    );
    processor.push(input, &mut out);
    processor.flush(&mut out);

    let expected_separator = concat!(
        "\u{2500}\u{2500}\u{253c}\u{2500}\u{2500}\u{2500}\u{2500}",
        "\u{253c}\u{2500}\u{2500}\u{2500}\u{2500}\n",
    );
    assert!(contains(&out, expected_separator));
    assert!(contains(&out, "1 \u{2502} 22 \u{2502} 333\n"));
}

#[test]
fn pipe_table_without_separator_falls_back_to_plain_lines() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    let input = "| just one |\n| stray pipes |\n";
    processor.push(input, &mut out);
    processor.flush(&mut out);
    assert_eq!(text(&out), "| just one |\n| stray pipes |\n");
}

#[test]
fn pipe_table_preceded_by_paragraph_and_followed_by_paragraph() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    let input = concat!(
        "Before table.\n",
        "| a | b |\n",
        "|---|---|\n",
        "| 1 | 2 |\n",
        "After table.\n",
    );
    processor.push(input, &mut out);

    assert!(text(&out).starts_with("Before table.\n"));
    assert!(text(&out).ends_with("After table.\n"));
}

#[test]
fn borderless_gfm_table_no_leading_trailing_pipes_is_detected_and_rendered() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    let input = concat!("Name | Age\n", "-----|-----\n", "Ana  | 30\n", "Bob  | 7\n");
    processor.push(input, &mut out);
    processor.flush(&mut out);

    assert!(contains(&out, "\u{2502}"));
    assert!(contains(&out, "\u{253c}"));
    assert!(contains(&out, "\x1b[1mName\x1b[22m"));
}

#[test]
fn borderless_separator_with_spaces_around_pipe_is_accepted() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    let input = concat!(
        "Caracter\u{ed}stica | Vercel | Cloudflare\n",
        "-------------- | ------ | ----------\n",
        "Enfoque principal | Frontend | Edge compute\n",
    );
    processor.push(input, &mut out);
    processor.flush(&mut out);

    assert!(contains(&out, "\u{2502}"));
    assert!(contains(&out, "\u{253c}"));
}

#[test]
fn paragraph_with_single_inline_pipe_falls_back_to_plain_no_separator() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    let input = concat!(
        "Use cmd | grep foo to filter.\n",
        "Then review the output.\n"
    );
    processor.push(input, &mut out);

    assert!(!contains(&out, "\u{2502}"));
    assert!(contains(&out, "Use cmd | grep foo"));
    assert!(contains(&out, "Then review the output."));
}

#[test]
fn header_with_nested_bold_stays_fully_bold_across_the_whole_line() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("## **Directory** Structure\n", &mut out);
    assert_eq!(text(&out), "\x1b[1mDirectory Structure\x1b[22m\n");
}

#[test]
fn header_keeps_italic_and_code_inline() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("# *note* about `fx`\n", &mut out);
    assert_eq!(
        text(&out),
        "\x1b[1m\x1b[4m\x1b[3mnote\x1b[23m about \x1b[38;5;255mfx\x1b[39m\x1b[24m\x1b[22m\n"
    );
}

#[test]
fn strikethrough_wraps_with_ansi() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("was ~~wrong~~ now right\n", &mut out);
    assert_eq!(text(&out), "was \x1b[9mwrong\x1b[29m now right\n");
}

#[test]
fn stray_tildes_between_spaces_stay_literal() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("path is ~~ /tmp\n", &mut out);
    assert_eq!(text(&out), "path is ~~ /tmp\n");
}

#[test]
fn horizontal_rule_with_dashes_renders_dim_line() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("---\n", &mut out);
    assert!(text(&out).starts_with(DIM_OPEN));
    assert!(text(&out).ends_with(&format!("{DIM_CLOSE}\n")));
    let glyphs = text(&out).matches(TABLE_HORIZ).count();
    assert_eq!(glyphs, HORIZONTAL_RULE_WIDTH);
}

#[test]
fn thematic_rule_completion_handles_newline_and_eof_without_matching_h6_content() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();
    let calls = Cell::new(0usize);
    let mut rule = |_: &mut Vec<u8>| calls.set(calls.get() + 1);

    processor.push_with_completions("---\n", &mut out, rule_only(&mut rule));
    assert_eq!(calls.get(), 1);
    assert_eq!(out.len(), 0);

    processor.push_with_completions("___", &mut out, rule_only(&mut rule));
    processor.flush_with_completions(&mut out, rule_only(&mut rule));
    assert_eq!(calls.get(), 2);
    assert_eq!(out.len(), 0);

    let mut h6 = String::from("###### ");
    h6.push_str(&TABLE_HORIZ.repeat(HORIZONTAL_RULE_WIDTH));
    h6.push('\n');
    processor.push_with_completions(&h6, &mut out, rule_only(&mut rule));
    assert_eq!(calls.get(), 2);
    assert!(!out.is_empty());
}

#[test]
fn horizontal_rule_with_asterisks_and_spaces_also_renders() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("* * *\n", &mut out);
    assert!(contains(&out, TABLE_HORIZ));
    assert!(!contains(&out, "*"));
}

#[test]
fn hyphen_space_line_is_a_list_not_a_rule() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("- item\n", &mut out);
    assert!(contains(&out, "\u{2022}"));
    assert!(!contains(&out, "\u{2500}"));
}

#[test]
fn nested_unordered_list_preserves_indent() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("  - nested item\n", &mut out);
    assert_eq!(text(&out), "  \x1b[2m\u{2022} \x1b[22mnested item\n");
}

#[test]
fn nested_ordered_list_preserves_indent() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("    1. sub-step\n", &mut out);
    assert_eq!(text(&out), "    \x1b[2m1.\x1b[22m sub-step\n");
}

#[test]
fn right_aligned_gfm_column_pads_cell_on_the_left() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    let input = concat!(
        "| Name | Score |\n",
        "|:-----|------:|\n",
        "| Ana  |     5 |\n"
    );
    processor.push(input, &mut out);
    processor.flush(&mut out);

    assert!(contains(&out, "    5\n"));
}

#[test]
fn center_aligned_gfm_column_pads_both_sides() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    let input = concat!(
        "| Name | Note |\n",
        "|------|:----:|\n",
        "| Ana  | ok   |\n"
    );
    processor.push(input, &mut out);
    processor.flush(&mut out);

    assert!(contains(&out, " ok \n"));
}

#[test]
fn pipe_table_parsing_retains_styled_cells_alignment_and_ragged_rows() {
    let mut link_id = 1;
    let table = parse_table_payload(
        concat!(
            "| Name | State | Count |\n",
            "|:-----|:-----:|------:|\n",
            "| **api** | ready | 7 | extra |\n",
            "| worker | |\n",
        ),
        &Styles::default(),
        &mut link_id,
    );

    assert_eq!(table.column_count, 4);
    assert_eq!(table.rows.len(), 3);
    assert_eq!(table.alignments[1], TableColumnAlign::Center);
    assert_eq!(table.alignments[2], TableColumnAlign::Right);
    assert_eq!(table.rows[1].cells[0], "\x1b[1mapi\x1b[22m");
    assert_eq!(table.rows[1].cells[3], "extra");
    assert_eq!(table.rows[2].cells.len(), 2);
}

#[test]
fn pipe_table_inside_code_block_stays_literal() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    let input = concat!(
        "```\n",
        "| not | a |\n",
        "|-----|---|\n",
        "| table | really |\n",
        "```\n",
    );
    processor.push(input, &mut out);

    assert!(contains(&out, "| not | a |"));
    assert!(contains(&out, "| table | really |"));
}
