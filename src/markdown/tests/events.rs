//! [`MarkdownProcessor::feed`] and [`MarkdownProcessor::finish`]. Upstream
//! has no event API of its own; its streaming caller builds the same
//! sequence from completions (`assistant_stream.zig:435-509`), so these
//! tests pin that this port's version drains text before every held block.

use crate::markdown::{
    CodeBlockPayload, Event, MarkdownProcessor, TableColumnAlign, TablePayload, TableRow,
};

fn cells(values: &[&str]) -> TableRow {
    TableRow {
        cells: values.iter().map(|value| (*value).to_owned()).collect(),
    }
}

#[test]
fn feed_returns_text_before_each_completed_block_in_source_order() {
    let mut processor = MarkdownProcessor::new();

    let events = processor.feed(concat!(
        "Intro **bold**\n",
        "```rust\n",
        "fn main() {}\n",
        "```\n",
        "| a | b |\n",
        "|---|---|\n",
        "| 1 | 2 |\n",
        "after\n",
        "\n",
        "---\n",
    ));

    assert_eq!(
        events,
        vec![
            Event::Text("Intro \x1b[1mbold\x1b[22m\n".to_owned()),
            Event::CodeBlock(CodeBlockPayload {
                language: "rust".to_owned(),
                code: "fn main() {}\n".to_owned(),
            }),
            Event::Table(TablePayload {
                rows: vec![cells(&["a", "b"]), cells(&["1", "2"])],
                alignments: vec![TableColumnAlign::Left, TableColumnAlign::Left],
                column_count: 2,
            }),
            Event::Text("after\n\n".to_owned()),
            Event::ThematicRule,
        ]
    );
    assert!(processor.finish().is_empty());
}

#[test]
fn finish_renders_the_partial_line_and_closes_an_open_fence() {
    let mut processor = MarkdownProcessor::new();

    assert!(processor.feed("Streaming par").is_empty());
    assert_eq!(
        processor.feed("tial\n```\ncode"),
        vec![Event::Text("Streaming partial\n".to_owned())]
    );
    assert_eq!(
        processor.finish(),
        vec![Event::CodeBlock(CodeBlockPayload {
            language: String::new(),
            code: "code\n".to_owned(),
        })]
    );
}

#[test]
fn link_ids_continue_from_a_handed_over_sequence() {
    let mut processor = MarkdownProcessor::new();
    processor.set_next_link_id(7);

    let mut events = processor.feed("[a](https://a.example)\n");
    events.extend(processor.finish());

    let [Event::Text(rendered)] = events.as_slice() else {
        panic!("one text event, got {events:?}");
    };
    assert!(
        rendered.contains("\x1b]8;id=fx-7;https://a.example"),
        "{rendered:?}"
    );
    assert_eq!(processor.next_link_id(), 8);
}
