//! Escapes, emphasis, code spans, entities, and heading styles
//! (`assistant_presentation.zig:1230-1782`).

use std::cell::Cell;
use std::time::Duration;

use super::{contains, rule_only, text};
use crate::markdown::{parse_table_payload, render_table_payload, MarkdownProcessor, Styles};

#[test]
fn backslash_escapes_keep_inline_punctuation_literal() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push(
        "literal \\*em\\* \\*\\*bold\\*\\* \\_italic\\_ \\_\\_strong\\_\\_ \\~\\~strike\\~\\~ \\`code\\` \\[docs](https://example.com) \\\\ \\| \\! \\# \\>\n",
        &mut out,
    );

    assert_eq!(
        text(&out),
        "literal *em* **bold** _italic_ __strong__ ~~strike~~ `code` [docs](https://example.com) \\ | ! # >\n"
    );
    assert!(!contains(&out, "\x1b"));
}

#[test]
fn backslash_escapes_survive_heading_preprocessing_and_code_spans() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("## \\*\\*literal bold\\*\\* and `\\*code\\*`\n", &mut out);
    assert_eq!(
        text(&out),
        "\x1b[1m**literal bold** and \x1b[38;5;245m\\*code\\*\x1b[39m\x1b[22m\n"
    );
}

#[test]
fn completed_markdown_lines_consume_one_terminal_hard_break_backslash() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push(
        concat!(
            "plain\\\n",
            "# heading\\\n",
            "- list\\\n",
            "> quote\\\n",
            "two\\\\\n",
            "three\\\\\\\n",
        ),
        &mut out,
    );

    assert_eq!(
        text(&out),
        concat!(
            "plain\n",
            "\x1b[1m\x1b[4mheading\x1b[24m\x1b[22m\n",
            "\x1b[2m\u{2022} \x1b[22mlist\n",
            "\x1b[2m\u{2502} \x1b[22mquote\n",
            "\x1b[2m\u{2502} \x1b[22mtwo\\\n",
            "\x1b[2m\u{2502} \x1b[22mthree\\\n",
        )
    );
}

#[test]
fn terminal_hard_break_backslashes_preserve_eof_and_code_content() {
    {
        let mut processor = MarkdownProcessor::new();
        let mut out = Vec::new();
        let mut rule = |_: &mut Vec<u8>| {};

        processor.push_with_completions("eof\\", &mut out, rule_only(&mut rule));
        processor.flush_with_completions(&mut out, rule_only(&mut rule));
        assert_eq!(text(&out), "eof\\");
    }

    {
        let mut processor = MarkdownProcessor::new();
        let mut out = Vec::new();

        processor.push("```\ncode\\\n```\n", &mut out);
        assert_eq!(text(&out), "\x1b[2m\u{2502} \x1b[22mcode\\\n");
    }
}

#[test]
fn setext_and_invalid_pipe_fallback_retain_source_line_completion() {
    {
        let mut processor = MarkdownProcessor::new();
        let mut out = Vec::new();
        let calls = Cell::new(0usize);
        let mut rule = |_: &mut Vec<u8>| calls.set(calls.get() + 1);

        processor.push_with_completions("Setext\\\n---\n", &mut out, rule_only(&mut rule));
        assert_eq!(text(&out), "\x1b[1mSetext\x1b[22m\n");
        assert_eq!(calls.get(), 0);
    }

    {
        let mut processor = MarkdownProcessor::new();
        let mut out = Vec::new();

        processor.push("| prior\\\n| eof\\", &mut out);
        processor.flush(&mut out);
        assert_eq!(text(&out), "| prior\n| eof\\");
    }
}

#[test]
fn link_labels_unescape_visible_punctuation() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    let id_before = processor.next_link_id();
    processor.push("[docs \\*literal\\*](https://example.com)\n", &mut out);

    let expected = format!(
        "\x1b]8;id=fx-{id_before};https://example.com\x1b\\\x1b[38;5;75m\x1b[4mdocs *literal*\x1b[24m\x1b[39m\x1b]8;;\x1b\\\n"
    );
    assert_eq!(text(&out), expected);
}

#[test]
fn backslash_escapes_stay_literal_in_setext_headings() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();
    let calls = Cell::new(0usize);
    let mut rule = |_: &mut Vec<u8>| calls.set(calls.get() + 1);

    processor.push_with_completions(
        "\\*\\*literal bold\\*\\*\n---\n",
        &mut out,
        rule_only(&mut rule),
    );

    assert_eq!(text(&out), "\x1b[1m**literal bold**\x1b[22m\n");
    assert_eq!(calls.get(), 0);
}

#[test]
fn bold_double_asterisk_wraps_with_ansi() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("hi **there** friend\n", &mut out);
    assert_eq!(text(&out), "hi \x1b[1mthere\x1b[22m friend\n");
}

#[test]
fn italic_single_asterisk_wraps_with_ansi() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("use *this* quickly\n", &mut out);
    assert_eq!(text(&out), "use \x1b[3mthis\x1b[23m quickly\n");
}

#[test]
fn underscore_emphasis_styles_valid_spans_and_preserves_invalid_markers() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push(
        "paragraph _italic_ and __bold__ with snake_case, snake__case, _ spaced_, and __ spaced__.\n",
        &mut out,
    );
    assert_eq!(
        text(&out),
        "paragraph \x1b[3mitalic\x1b[23m and \x1b[1mbold\x1b[22m with snake_case, snake__case, _ spaced_, and __ spaced__.\n"
    );
}

#[test]
fn underscore_emphasis_renders_in_list_blockquote_and_table_cells() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("- _item_\n> __quote__\n", &mut out);
    assert_eq!(
        text(&out),
        concat!(
            "\x1b[2m\u{2022} \x1b[22m\x1b[3mitem\x1b[23m\n",
            "\x1b[2m\u{2502} \x1b[22m\x1b[1mquote\x1b[22m\n",
        )
    );

    let mut link_id = 1;
    let table = parse_table_payload(
        concat!(
            "| Name | Value |\n",
            "|------|-------|\n",
            "| _row_ | __cell__ |\n"
        ),
        &Styles::default(),
        &mut link_id,
    );
    out.clear();
    render_table_payload(&table, &mut out);
    assert!(contains(&out, "\x1b[3mrow\x1b[23m"));
    assert!(contains(&out, "\x1b[1mcell\x1b[22m"));
}

#[test]
fn underscore_formatted_bare_urls_retain_path_underscores_and_matching_closers() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    let id_before = processor.next_link_id();
    processor.push(
        "_https://example.com/snake_case_ tail __https://example.com/snake_case__ tail\n",
        &mut out,
    );

    let expected = format!(
        concat!(
            "\x1b[3m\x1b]8;id=fx-{0};https://example.com/snake_case\x1b\\\x1b[38;5;75m\x1b[4mhttps://example.com/snake_case\x1b[24m\x1b[39m\x1b]8;;\x1b\\\x1b[23m tail ",
            "\x1b[1m\x1b]8;id=fx-{1};https://example.com/snake_case\x1b\\\x1b[38;5;75m\x1b[4mhttps://example.com/snake_case\x1b[24m\x1b[39m\x1b]8;;\x1b\\\x1b[22m tail\n",
        ),
        id_before,
        id_before.wrapping_add(1)
    );
    assert_eq!(text(&out), expected);
}

#[test]
fn underscore_formatted_urls_require_exact_active_markers() {
    let no_link_inputs = [
        "snake_https://example.com\n",
        "snake__https://example.com\n",
        "_prefix__https://example.com\n",
        "__prefix__https://example.com\n",
    ];

    for input in no_link_inputs {
        let mut processor = MarkdownProcessor::new();
        let mut out = Vec::new();

        processor.push(input, &mut out);
        assert!(!contains(&out, "\x1b]8;"), "{input:?}");
    }

    // Trailing underscores are outside the URL, as in GFM autolinks, so the
    // mismatched runs pair on their shared length and the extra underscore
    // stays literal next to the styled link.
    let mismatched_run_cases = [
        (
            "_https://example.com/path__ tail\n",
            ";https://example.com/path\x1b\\",
            "\x1b[3m\x1b]8;",
            "\x1b[23m_ tail\n",
        ),
        (
            "__https://example.com/path_ tail\n",
            ";https://example.com/path\x1b\\",
            "_\x1b[3m\x1b]8;",
            "\x1b[23m tail\n",
        ),
    ];

    for (input, url, prefix, tail) in mismatched_run_cases {
        let mut processor = MarkdownProcessor::new();
        let mut out = Vec::new();

        processor.push(input, &mut out);
        assert!(contains(&out, url), "{input:?}");
        assert!(text(&out).starts_with(prefix), "{input:?}");
        assert!(text(&out).ends_with(tail), "{input:?}");
    }
}

#[test]
fn underscore_emphasis_preserves_code_literals_and_keeps_unpaired_markers_literal() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("_unclosed\n__unclosed\n`_literal_ __literal__`\n", &mut out);
    assert_eq!(
        text(&out),
        concat!(
            "_unclosed\n",
            "__unclosed\n",
            "\x1b[38;5;245m_literal_ __literal__\x1b[39m\n",
        )
    );
}

#[test]
fn headings_preserve_literal_underscores_and_suppress_valid_double_underscore_strong_markers() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("## __Strong__ snake__case __ spaced__\n", &mut out);
    assert_eq!(
        text(&out),
        "\x1b[1mStrong snake__case __ spaced__\x1b[22m\n"
    );
}

#[test]
fn cross_form_nested_strong_spans_reassert_the_remaining_bold_style() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push(
        "**outer __inner__ suffix** __outer **inner** suffix__\n",
        &mut out,
    );
    assert_eq!(
        text(&out),
        concat!(
            "\x1b[1mouter \x1b[1minner\x1b[22m\x1b[1m suffix\x1b[22m ",
            "\x1b[1mouter \x1b[1minner\x1b[22m\x1b[1m suffix\x1b[22m\n",
        )
    );
}

#[test]
fn cross_form_nested_italic_spans_reassert_the_remaining_italic_style() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("*outer _inner_ suffix* _outer *inner* suffix_\n", &mut out);
    assert_eq!(
        text(&out),
        concat!(
            "\x1b[3mouter \x1b[3minner\x1b[23m\x1b[3m suffix\x1b[23m ",
            "\x1b[3mouter \x1b[3minner\x1b[23m\x1b[3m suffix\x1b[23m\n",
        )
    );
}

#[test]
fn table_payload_headers_reassert_outer_bold_after_inline_strong_closes() {
    let mut link_id = 1;
    let table = parse_table_payload(
        concat!(
            "| prefix __strong__ suffix |\n",
            "|------|\n",
            "| value |\n"
        ),
        &Styles::default(),
        &mut link_id,
    );
    let mut out = Vec::new();

    render_table_payload(&table, &mut out);
    assert!(contains(
        &out,
        "\x1b[1mprefix \x1b[1mstrong\x1b[22m\x1b[1m suffix\x1b[22m"
    ));
}

#[test]
fn double_backtick_code_span_keeps_inner_backticks_and_trims_one_padding_space() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("use `` a ` b `` and ``x`` here\n", &mut out);
    assert_eq!(
        text(&out),
        "use \x1b[38;5;245ma ` b\x1b[39m and \x1b[38;5;245mx\x1b[39m here\n"
    );
}

#[test]
fn code_span_closes_only_on_a_backtick_run_of_the_same_length() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("`a``b` and ``` lonely **bold**\n", &mut out);
    assert_eq!(
        text(&out),
        "\x1b[38;5;245ma``b\x1b[39m and ``` lonely \x1b[1mbold\x1b[22m\n"
    );
}

#[test]
fn numeric_entities_for_control_characters_stay_literal() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push(
        "x&#27;[2Ky &#x1b;[31m &#7; &#127;&#x9b; &#0; &#x41;\n",
        &mut out,
    );
    assert_eq!(
        text(&out),
        "x&#27;[2Ky &#x1b;[31m &#7; &#127;&#x9b; \u{fffd} A\n"
    );
    assert!(!out.contains(&0x1b));
}

#[test]
fn entity_lookup_is_bounded_and_a_long_ampersand_line_renders_in_bounded_time() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    // A semicolon farther than the longest accepted name never forms an
    // entity.
    processor.push("&ampersand; &amp;\n", &mut out);
    assert_eq!(text(&out), "&ampersand; &\n");

    out.clear();
    let mut line = "&".repeat(64 * 1024);
    line.push('\n');
    let started = super::thread_cpu_time();
    processor.push(&line, &mut out);
    let elapsed = super::thread_cpu_time() - started;
    assert!(elapsed < Duration::from_millis(500), "{elapsed:?}");
    assert_eq!(text(&out), line);
}

#[test]
fn code_span_content_is_not_entity_decoded_but_prose_is() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push(
        "a &amp; b &lt;c&gt; &quot;d&quot; &#39;e&#39; &#x2192; `&amp;` &unknown; &amp\n",
        &mut out,
    );
    assert_eq!(
        text(&out),
        "a & b <c> \"d\" 'e' \u{2192} \x1b[38;5;245m&amp;\x1b[39m &unknown; &amp\n"
    );
}

#[test]
fn heading_strips_emphasis_markers_around_a_multi_backtick_code_span() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("## Run ``**raw**`` now\n", &mut out);
    assert_eq!(
        text(&out),
        "\x1b[1mRun \x1b[38;5;245m**raw**\x1b[39m now\x1b[22m\n"
    );
}

#[test]
fn inline_code_backticks_wrap_with_ansi() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("run `zig build` now\n", &mut out);
    assert_eq!(text(&out), "run \x1b[38;5;245mzig build\x1b[39m now\n");
}

#[test]
fn inline_code_uses_the_selected_light_background() {
    let mut processor = MarkdownProcessor::new();
    processor.set_inline_code_theme(true);
    let mut out = Vec::new();

    processor.push("run `zig build` now\n", &mut out);
    assert_eq!(text(&out), "run \x1b[38;5;247mzig build\x1b[39m now\n");
}

#[test]
fn stray_asterisk_between_spaces_stays_literal() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("3 * 5 = 15\n", &mut out);
    assert_eq!(text(&out), "3 * 5 = 15\n");
}

#[test]
fn headings_use_level_specific_ansi_styles() {
    let cases = [
        (
            "# Workspace overview\n",
            "\x1b[1m\x1b[4mWorkspace overview\x1b[24m\x1b[22m\n",
        ),
        ("## Installation\n", "\x1b[1mInstallation\x1b[22m\n"),
        ("### macOS\n", "\x1b[4mmacOS\x1b[24m\n"),
        ("#### Shell setup\n", "\x1b[1m\x1b[2mShell setup\x1b[22m\n"),
        (
            "##### Optional tools\n",
            "\x1b[2m\x1b[4mOptional tools\x1b[24m\x1b[22m\n",
        ),
        (
            "###### Troubleshooting\n",
            "\x1b[2mTroubleshooting\x1b[22m\n",
        ),
    ];

    for (input, expected) in cases {
        let mut processor = MarkdownProcessor::new();
        let mut out = Vec::new();

        processor.push(input, &mut out);
        assert_eq!(text(&out), expected);
        assert!(!contains(&out, "\x1b[1;4m"));
    }
}
