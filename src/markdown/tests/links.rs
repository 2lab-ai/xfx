//! Events, links, images, autolinks, and bare URLs
//! (`assistant_presentation.zig:86-1229`).

use super::{contains, text};
use crate::markdown::ansi::MAX_LINK_URL_BYTES;
use crate::markdown::display_width;
use crate::markdown::{parse_table_payload, CodeBlockPayload, Event, MarkdownProcessor, Styles};

#[test]
fn assistant_presentation_event_clone_owns_nested_payloads() {
    let mut text_source = Event::Text("ok".to_owned());
    let text_event = text_source.clone();
    if let Event::Text(source) = &mut text_source {
        source.replace_range(0..1, "x");
    }
    assert_eq!(text_event, Event::Text("ok".to_owned()));

    let mut block_source = Event::CodeBlock(CodeBlockPayload {
        language: "zig".to_owned(),
        code: "zig".to_owned(),
    });
    let block = block_source.clone();
    if let Event::CodeBlock(source) = &mut block_source {
        source.code.replace_range(0..1, "b");
    }
    let Event::CodeBlock(block) = block else {
        panic!("cloned a code block");
    };
    assert_eq!(block.language, "zig");
    assert_eq!(block.code, "zig");

    let mut link_id = 1;
    let table = parse_table_payload(
        "| Name |\n|---|\n| api |\n",
        &Styles::default(),
        &mut link_id,
    );
    let source_cell = table.rows[1].cells[0].as_ptr();
    let table_event = Event::Table(table.clone());
    let Event::Table(cloned) = &table_event else {
        panic!("cloned a table");
    };
    assert_eq!(cloned.rows[1].cells[0], "api");
    assert_ne!(cloned.rows[1].cells[0].as_ptr(), source_cell);
    assert!(!Event::Text("text".to_owned()).requires_text_drain());
    assert!(Event::Table(table).requires_text_drain());
    assert!(Event::ThematicRule.requires_text_drain());
    assert_eq!(Event::ThematicRule.retained_byte_count(), 1);
}

#[test]
fn markdown_link_is_blue_and_underlined_inside_its_osc_8_scope() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    let id_before = processor.next_link_id();
    processor.push("see [docs](https://example.com) please\n", &mut out);

    let expected = format!(
        "see \x1b]8;id=fx-{id_before};https://example.com\x1b\\\x1b[38;5;75m\x1b[4mdocs\x1b[24m\x1b[39m\x1b]8;;\x1b\\ please\n"
    );
    assert_eq!(text(&out), expected);
}

#[test]
fn markdown_link_destination_keeps_balanced_parentheses() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    let id_before = processor.next_link_id();
    processor.push(
        "[w](https://en.wikipedia.org/wiki/Foo_(bar)) tail\n",
        &mut out,
    );

    let expected = format!(
        "\x1b]8;id=fx-{id_before};https://en.wikipedia.org/wiki/Foo_(bar)\x1b\\\x1b[38;5;75m\x1b[4mw\x1b[24m\x1b[39m\x1b]8;;\x1b\\ tail\n"
    );
    assert_eq!(text(&out), expected);
}

#[test]
fn markdown_link_drops_its_title_and_unwraps_angle_destinations() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    let id_before = processor.next_link_id();
    processor.push(
        "[t](https://example.com \"Title text\") [s](https://example.com/s 'single') [a](<https://example.com/a b>)\n",
        &mut out,
    );

    let expected = format!(
        concat!(
            "\x1b]8;id=fx-{0};https://example.com\x1b\\\x1b[38;5;75m\x1b[4mt\x1b[24m\x1b[39m\x1b]8;;\x1b\\ ",
            "\x1b]8;id=fx-{1};https://example.com/s\x1b\\\x1b[38;5;75m\x1b[4ms\x1b[24m\x1b[39m\x1b]8;;\x1b\\ ",
            "\x1b]8;id=fx-{2};https://example.com/a b\x1b\\\x1b[38;5;75m\x1b[4ma\x1b[24m\x1b[39m\x1b]8;;\x1b\\\n",
        ),
        id_before,
        id_before + 1,
        id_before + 2
    );
    assert_eq!(text(&out), expected);
}

#[test]
fn markdown_link_with_unbalanced_or_spaced_destination_stays_literal() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push(
        "[u](https://e.com/(x) [v](https://e.com/a b) [w](https://e.com \"open)\n",
        &mut out,
    );
    assert_eq!(
        text(&out),
        "[u](https://e.com/(x) [v](https://e.com/a b) [w](https://e.com \"open)\n"
    );
}

#[test]
fn markdown_image_renders_its_alt_text_with_an_image_marker_inside_one_osc_8_scope() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    let id_before = processor.next_link_id();
    processor.push(
        "see ![architecture diagram](https://example.com/diagram.png) please\n",
        &mut out,
    );

    let expected = format!(
        "see \x1b]8;id=fx-{id_before};https://example.com/diagram.png\x1b\\\x1b[38;5;75m\x1b[4m\u{25a7} architecture diagram\x1b[24m\x1b[39m\x1b]8;;\x1b\\ please\n"
    );
    assert_eq!(text(&out), expected);
}

#[test]
fn markdown_image_uses_a_stable_fallback_for_empty_alt_text() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    let id_before = processor.next_link_id();
    processor.push("![](https://example.com/diagram.png)\n", &mut out);

    let expected = format!(
        "\x1b]8;id=fx-{id_before};https://example.com/diagram.png\x1b\\\x1b[38;5;75m\x1b[4m\u{25a7} image\x1b[24m\x1b[39m\x1b]8;;\x1b\\\n"
    );
    assert_eq!(text(&out), expected);
}

#[test]
fn markdown_image_unescapes_alt_punctuation_through_the_existing_link_emitter() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    let id_before = processor.next_link_id();
    processor.push(
        "![architecture \\*diagram\\*](https://example.com/diagram.png)\n",
        &mut out,
    );

    let expected = format!(
        "\x1b]8;id=fx-{id_before};https://example.com/diagram.png\x1b\\\x1b[38;5;75m\x1b[4m\u{25a7} architecture *diagram*\x1b[24m\x1b[39m\x1b]8;;\x1b\\\n"
    );
    assert_eq!(text(&out), expected);
}

#[test]
fn escaped_and_malformed_markdown_images_remain_literal_without_osc_8() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push(
        "\\![alt](https://example.com/diagram.png) and \\!\n",
        &mut out,
    );
    assert_eq!(
        text(&out),
        "![alt](https://example.com/diagram.png) and !\n"
    );
    assert!(!contains(&out, "\x1b]8;"));

    out.clear();
    processor.push("![alt](https://example.com/diagram.png\n", &mut out);
    assert_eq!(text(&out), "![alt](https://example.com/diagram.png\n");
    assert!(!contains(&out, "\x1b]8;"));

    out.clear();
    processor.push("![alt](https://example.com/\x1bdiagram.png)\n", &mut out);
    assert_eq!(text(&out), "![alt](https://example.com/\x1bdiagram.png)\n");
    assert!(!contains(&out, "\x1b]8;"));

    let oversized_url = "a".repeat(MAX_LINK_URL_BYTES + 1);
    let oversized = format!("![alt]({oversized_url})\n");
    out.clear();
    processor.push(&oversized, &mut out);
    assert_eq!(text(&out), oversized);
    assert!(!contains(&out, "\x1b]8;"));
}

#[test]
fn markdown_images_preserve_code_isolation_chunk_buffering_and_heading_underline() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("`![literal](https://example.com/literal.png)`\n", &mut out);
    assert_eq!(
        text(&out),
        "\x1b[38;5;255m![literal](https://example.com/literal.png)\x1b[39m\n"
    );
    assert!(!contains(&out, "\x1b]8;"));

    out.clear();
    processor.push("![architecture", &mut out);
    assert_eq!(out.len(), 0);
    processor.push("](https://example.com/diagram.png)\n", &mut out);
    assert!(contains(&out, "\u{25a7} architecture"));

    out.clear();
    let id_before = processor.next_link_id();
    processor.push(
        "### before ![diagram](https://example.com/diagram.png) after\n",
        &mut out,
    );
    let expected = format!(
        "\x1b[4mbefore \x1b]8;id=fx-{id_before};https://example.com/diagram.png\x1b\\\x1b[38;5;75m\x1b[4m\u{25a7} diagram\x1b[24m\x1b[39m\x1b]8;;\x1b\\\x1b[4m after\x1b[24m\n"
    );
    assert_eq!(text(&out), expected);
}

#[test]
fn table_payload_measures_markdown_images_by_their_visible_label() {
    let mut link_id = 1;
    let table = parse_table_payload(
        concat!(
            "| Asset | Status |\n",
            "| --- | --- |\n",
            "| ![diagram](https://example.com/diagram.png) | ready |\n",
        ),
        &Styles::default(),
        &mut link_id,
    );

    let cell = &table.rows[1].cells[0];
    assert!(cell.contains("\u{25a7} diagram"));
    assert!(!cell.contains("![diagram]"));
    assert_eq!(
        display_width::visible_width_ignoring_ansi("\u{25a7} diagram".as_bytes()),
        display_width::visible_width_ignoring_ansi(cell.as_bytes())
    );
}

#[test]
fn table_payload_links_use_the_shared_osc_8_identifier_sequence() {
    let styles = Styles::default();
    let mut link_id = 1;
    let id_before = link_id;

    let first = parse_table_payload(
        concat!(
            "| Link |\n",
            "|------|\n",
            "| [first](https://first.example) |\n"
        ),
        &styles,
        &mut link_id,
    );

    let second = parse_table_payload(
        concat!(
            "| Link |\n",
            "|------|\n",
            "| [second](https://second.example) |\n"
        ),
        &styles,
        &mut link_id,
    );

    let first_id = format!("\x1b]8;id=fx-{id_before};https://first.example");
    assert!(first.rows[1].cells[0].contains(&first_id));

    let second_id = format!(
        "\x1b]8;id=fx-{};https://second.example",
        id_before.wrapping_add(1)
    );
    assert!(second.rows[1].cells[0].contains(&second_id));
}

#[test]
fn bare_url_is_underlined_and_leaves_sentence_punctuation_literal() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    let id_before = processor.next_link_id();
    processor.push("visit https://example.com/docs, now\n", &mut out);

    let expected = format!(
        "visit \x1b]8;id=fx-{id_before};https://example.com/docs\x1b\\\x1b[38;5;75m\x1b[4mhttps://example.com/docs\x1b[24m\x1b[39m\x1b]8;;\x1b\\, now\n"
    );
    assert_eq!(text(&out), expected);
}

#[test]
fn bare_url_is_recognized_after_streamed_input_chunks() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    let id_before = processor.next_link_id();
    processor.push("visit https", &mut out);
    assert_eq!(out.len(), 0);
    processor.push("://example.com/docs\n", &mut out);

    let expected = format!(
        "visit \x1b]8;id=fx-{id_before};https://example.com/docs\x1b\\\x1b[38;5;75m\x1b[4mhttps://example.com/docs\x1b[24m\x1b[39m\x1b]8;;\x1b\\\n"
    );
    assert_eq!(text(&out), expected);
}

#[test]
fn angle_autolinks_use_literal_uri_and_email_labels() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    let id_before = processor.next_link_id();
    processor.push(
        "see <https://example.com/docs\\_literal> and <dev@example.com> plus <git+ssh://example.com/repo>\n",
        &mut out,
    );

    let expected = format!(
        concat!(
            "see \x1b]8;id=fx-{0};https://example.com/docs\\_literal\x1b\\\x1b[38;5;75m\x1b[4mhttps://example.com/docs\\_literal\x1b[24m\x1b[39m\x1b]8;;\x1b\\ and ",
            "\x1b]8;id=fx-{1};mailto:dev@example.com\x1b\\\x1b[38;5;75m\x1b[4mdev@example.com\x1b[24m\x1b[39m\x1b]8;;\x1b\\ plus ",
            "\x1b]8;id=fx-{2};git+ssh://example.com/repo\x1b\\\x1b[38;5;75m\x1b[4mgit+ssh://example.com/repo\x1b[24m\x1b[39m\x1b]8;;\x1b\\\n",
        ),
        id_before,
        id_before.wrapping_add(1),
        id_before.wrapping_add(2)
    );
    assert_eq!(text(&out), expected);
}

#[test]
fn rejected_angle_candidates_suppress_nested_link_emission() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    let input = concat!(
        "<not-an-autolink https://inner.example>\n",
        "\\<not-an-autolink https://inner.example>\n",
        "<<https://inner.example>>\n",
        "<[inner](https://inner.example)>\n",
        "<[inner](https://one.example) https://two.example>\n",
        "\\<https://escaped.example>\n",
        "<a:x> <dev@bad-.example> <https://has space>\n",
        "<not-an-autolink https://unterminated.example\n",
        "`<https://code.example>`\n",
    );
    let expected = concat!(
        "<not-an-autolink https://inner.example>\n",
        "<not-an-autolink https://inner.example>\n",
        "<<https://inner.example>>\n",
        "<[inner](https://inner.example)>\n",
        "<[inner](https://one.example) https://two.example>\n",
        "<https://escaped.example>\n",
        "<a:x> <dev@bad-.example> <https://has space>\n",
        "<not-an-autolink https://unterminated.example\n",
        "\x1b[38;5;255m<https://code.example>\x1b[39m\n",
    );

    processor.push(input, &mut out);
    assert_eq!(text(&out), expected);
    assert!(!contains(&out, "\x1b]8;"));
}

#[test]
fn angle_email_autolink_applies_the_destination_cap_including_mailto() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    let suffix = "@a.com";
    let local_len = MAX_LINK_URL_BYTES - "mailto:".len() - suffix.len() + 1;
    let input = format!("<{}{suffix}>\n", "a".repeat(local_len));

    processor.push(&input, &mut out);
    assert_eq!(text(&out), input);
    assert!(!contains(&out, "\x1b]8;"));
}

#[test]
fn standalone_url_in_code_is_linked_while_excluded_boundaries_stay_literal() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    let id_before = processor.next_link_id();
    processor.push(
        "`https://code.example` wordhttps://word.example <<https://angle.example>>\n",
        &mut out,
    );
    let expected = format!(
        "\x1b[38;5;255m\x1b]8;id=fx-{id_before};https://code.example\x1b\\https://code.example\x1b]8;;\x1b\\\x1b[39m wordhttps://word.example <<https://angle.example>>\n"
    );
    assert_eq!(text(&out), expected);
}

#[test]
fn inline_code_urls_keep_literal_bytes_and_trailing_punctuation_outside_links() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    let id_before = processor.next_link_id();
    processor.push(
        "See `https://example.com/path_~*.` and `https://example.com/a\\_b`.\n",
        &mut out,
    );
    let expected = format!(
        concat!(
            "See \x1b[38;5;255m\x1b]8;id=fx-{0};https://example.com/path_~*\x1b\\https://example.com/path_~*\x1b]8;;\x1b\\.\x1b[39m and ",
            "\x1b[38;5;255m\x1b]8;id=fx-{1};https://example.com/a\\_b\x1b\\https://example.com/a\\_b\x1b]8;;\x1b\\\x1b[39m.\n",
        ),
        id_before,
        id_before.wrapping_add(1)
    );
    assert_eq!(text(&out), expected);
}

#[test]
fn inline_code_urls_preserve_literal_trailing_uri_punctuation() {
    let urls = [
        "https://example.com/a!",
        "https://example.com/a?",
        "https://example.com/a;",
        "https://example.com/a:",
        "https://example.com/a,",
    ];
    for url in urls {
        let mut processor = MarkdownProcessor::new();
        let mut out = Vec::new();
        let input = format!("Open `{url}`\n");
        let id_before = processor.next_link_id();
        processor.push(&input, &mut out);
        let expected = format!(
            "Open \x1b[38;5;255m\x1b]8;id=fx-{id_before};{url}\x1b\\{url}\x1b]8;;\x1b\\\x1b[39m\n"
        );
        assert_eq!(text(&out), expected);
    }
}

#[test]
fn non_url_and_unsafe_inline_code_stays_literal() {
    let cases = [
        "`not a URL`\n",
        "`curl https://example.com`\n",
        "`https://example.com more`\n",
        "`http://`\n",
        "`https://example.com\x07`\n",
        "`<https://example.com>`\n",
    ];
    for input in cases {
        let mut processor = MarkdownProcessor::new();
        let mut out = Vec::new();
        processor.push(input, &mut out);
        assert!(!contains(&out, "\x1b]8;"), "{input:?}");
    }

    let input = format!("`https://{}`\n", "a".repeat(MAX_LINK_URL_BYTES + 1));
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();
    processor.push(&input, &mut out);
    assert!(!contains(&out, "\x1b]8;"));
}

#[test]
fn unsafe_bare_url_renders_literally() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    let input = format!("https://{}\n", "a".repeat(MAX_LINK_URL_BYTES + 1));

    processor.push(&input, &mut out);
    assert_eq!(text(&out), input);
    assert!(!contains(&out, "\x1b]8;"));
}

#[test]
fn bare_urls_leave_closing_emphasis_delimiters_for_the_inline_scanner() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    let id_before = processor.next_link_id();
    processor.push(
        "**https://bold.example** *https://italic.example* ~~https://strike.example~~ tail\n",
        &mut out,
    );

    let expected = format!(
        concat!(
            "\x1b[1m\x1b]8;id=fx-{0};https://bold.example\x1b\\\x1b[38;5;75m\x1b[4mhttps://bold.example\x1b[24m\x1b[39m\x1b]8;;\x1b\\\x1b[22m ",
            "\x1b[3m\x1b]8;id=fx-{1};https://italic.example\x1b\\\x1b[38;5;75m\x1b[4mhttps://italic.example\x1b[24m\x1b[39m\x1b]8;;\x1b\\\x1b[23m ",
            "\x1b[9m\x1b]8;id=fx-{2};https://strike.example\x1b\\\x1b[38;5;75m\x1b[4mhttps://strike.example\x1b[24m\x1b[39m\x1b]8;;\x1b\\\x1b[29m tail\n",
        ),
        id_before,
        id_before.wrapping_add(1),
        id_before.wrapping_add(2)
    );
    assert_eq!(text(&out), expected);
}

#[test]
fn malformed_formatted_link_suppresses_bare_url_recognition() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("see [docs](https://example.com missing close\n", &mut out);
    assert_eq!(text(&out), "see [docs](https://example.com missing close\n");
    assert!(!contains(&out, "\x1b]8;"));
}

#[test]
fn heading_underline_resumes_after_a_link_closes_its_local_underline() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    let id_before = processor.next_link_id();
    processor.push(
        "### before [link](https://example.com) after\nbody\n",
        &mut out,
    );

    let expected = format!(
        "\x1b[4mbefore \x1b]8;id=fx-{id_before};https://example.com\x1b\\\x1b[38;5;75m\x1b[4mlink\x1b[24m\x1b[39m\x1b]8;;\x1b\\\x1b[4m after\x1b[24m\nbody\n"
    );
    assert_eq!(text(&out), expected);
}

#[test]
fn two_markdown_links_get_distinct_ids() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push(
        "[a](https://a.example) and [b](https://b.example)\n",
        &mut out,
    );
    assert_eq!(text(&out).matches("id=fx-").count(), 2);
}

#[test]
fn url_over_osc_8_size_cap_renders_literally() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    let url = "a".repeat(MAX_LINK_URL_BYTES + 1);
    let input = format!("see [x]({url}) end\n");

    processor.push(&input, &mut out);
    assert!(!contains(&out, "\x1b]8;"));
    assert!(contains(&out, "[x]("));
}

#[test]
fn unmatched_bracket_renders_literally() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("list [1, 2, 3] of items\n", &mut out);
    assert_eq!(text(&out), "list [1, 2, 3] of items\n");
}

#[test]
fn link_with_empty_url_renders_literally() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("see [docs]() now\n", &mut out);
    assert_eq!(text(&out), "see [docs]() now\n");
}

#[test]
fn plain_text_passes_through_unchanged() {
    let mut processor = MarkdownProcessor::new();
    let mut out = Vec::new();

    processor.push("hello world\n", &mut out);
    assert_eq!(text(&out), "hello world\n");
}
