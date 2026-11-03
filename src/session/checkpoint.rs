//! The saved form of a context compaction, and the text the model reads in
//! place of the turns it covers.
//!
//! Upstream's `checkpoint.zig` (`vercel-labs/fx@c1db9191
//! src/core/compactor/checkpoint.zig`): every compacted turn with its user
//! messages and final reply **word for word**, a summary of what the
//! assistant did in between, a line for each tool call, and the session's
//! rules, facts, decisions and status as entries that are only ever added.
//! The JSON field names are upstream's `Payload` field names (`:116-147`), so
//! the record says the same thing upstream's would.
//!
//! Two of upstream's fields have nothing to hold here and are absent rather
//! than written empty: the skills and MCP tools a session used (xfx has
//! neither), and the folded ledgers L1, L2, ... -- upstream folds an earlier
//! compaction away only into a saved record (`summarize.zig:173-185`, "Without
//! a store it cannot be saved, so it stays"), and xfx keeps no record store a
//! model could open. So every compaction here keeps the turns of the ones
//! before it, which is upstream's own behaviour for a session it cannot save
//! records for.
//!
//! **Typed, not a string.** Upstream keeps the payload as an opaque string
//! behind a version marker; a session event here has no free-form blob
//! ([`super::event`]), so the payload is a type and the log is its JSON.
//!
//! This file does no I/O. [`render`] is the one place the model's text is
//! written, so a resumed session and the session that compacted read the same
//! bytes.

use serde::{Deserialize, Serialize};

/// The letters of entry IDs, in the order their sections are shown: rules,
/// facts, decisions, status, open (`checkpoint.zig:79`).
pub const ENTRY_KINDS: &str = "RFDSO";

/// What follows an entry a later one replaces, before the later one's ID
/// (`checkpoint.zig:378`).
pub const REPLACED_MARK: &str = " (replaced by ";

/// The opening of the text the model reads (`render`, `checkpoint.zig:277-278`).
pub const OPENING: &str = "<compacted_conversation>\nThis is the earlier part of this \
conversation, compacted. The user's messages and the assistant's final replies shown here are \
exact. What the assistant did in between is summarized, and each tool call has a line saying \
what it was and what it showed.\n\n";

/// Its closing line.
pub const CLOSING: &str = "</compacted_conversation>\n";

/// One tool call as the compacted conversation lists it (`checkpoint.zig:29-37`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolLine {
    /// T<number>, counted across the whole session.
    pub number: usize,
    /// What code knows of the call: the tool, what it was given and how it
    /// ended, like `read_file notes.txt (5 lines)`.
    pub line: String,
    /// Why it was used and what it showed, from the model; may be empty.
    #[serde(default)]
    pub why: String,
}

/// One compacted turn (`checkpoint.zig:42-60`). Once written it never changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompactedTurn {
    /// The turn's number, counted across the whole session.
    pub number: usize,
    /// The message that started the turn, exact.
    pub users: Vec<String>,
    /// What the assistant did before its final reply, summarized.
    #[serde(default)]
    pub work: String,
    /// The assistant's final reply, exact; empty when the turn ended without
    /// one.
    #[serde(default, rename = "final")]
    pub final_reply: String,
    /// The turn's tool calls are T<first_tool> through T<last_tool>; zero when
    /// it made none.
    #[serde(default)]
    pub first_tool: usize,
    #[serde(default)]
    pub last_tool: usize,
    #[serde(default)]
    pub tools: Vec<ToolLine>,
}

/// One entry of the session's rules, facts, decisions, status or open
/// questions (`checkpoint.zig:108-113`). Entries are only ever added; a newer
/// one may say it replaces an older one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// Like `R3`. Its letter names its section.
    pub id: String,
    /// The whole entry as written, starting with its ID.
    pub text: String,
}

/// The earlier part of a turn a compaction cut inside of: what it did so
/// far, summarized, while its first user message stays after the checkpoint
/// with the rest of it (`OpenTurn`, `checkpoint.zig:64-75`). The next
/// compaction carries it on when it compacts more of that turn.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenTurn {
    /// Messages the user added while it ran, exact. xfx has none to add --
    /// what the user says at an approval is a note, not a message -- so this
    /// stays empty; kept for the payload's shape.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub users: Vec<String>,
    /// What the assistant has done so far, summarized. (Upstream also keeps
    /// the part's exact text, for the record it saves once the turn is
    /// compacted to its end; xfx saves no records.)
    #[serde(default)]
    pub work: String,
    #[serde(default)]
    pub first_tool: usize,
    #[serde(default)]
    pub last_tool: usize,
    #[serde(default)]
    pub tools: Vec<ToolLine>,
}

/// A compacted conversation: what replaces the turns a checkpoint covers
/// (`Payload`, `checkpoint.zig:128-154`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Compacted {
    /// The session's entries, oldest first.
    #[serde(default)]
    pub entries: Vec<Entry>,
    /// The compacted turns, oldest first.
    pub turns: Vec<CompactedTurn>,
    /// The earlier part of the turn the checkpoint cut inside of, when it
    /// cut inside one. **Absent from the frame when there is none**, so a
    /// checkpoint at whole turns is written exactly as before it existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open: Option<OpenTurn>,
    /// Turns and tool calls are numbered through these counts; a turn in
    /// progress takes no number until it is compacted to its end.
    pub turn_count: usize,
    pub tool_count: usize,
}

impl Compacted {
    /// The highest number of each kind of entry it holds, in [`ENTRY_KINDS`]
    /// order (`highestIds`, `checkpoint.zig:158-168`).
    pub fn highest_ids(&self) -> [usize; 5] {
        highest_ids(&self.entries)
    }
}

/// [`Compacted::highest_ids`] over any list of entries.
pub fn highest_ids(entries: &[Entry]) -> [usize; 5] {
    let mut highest = [0usize; 5];
    for entry in entries {
        if !is_entry_id(&entry.id) {
            continue;
        }
        let kind = ENTRY_KINDS
            .find(entry.id.as_bytes()[0] as char)
            .expect("an entry id starts with a kind");
        if let Ok(number) = entry.id[1..].parse::<usize>() {
            highest[kind] = highest[kind].max(number);
        }
    }
    highest
}

/// Whether `word` is an entry ID, such as `R3` (`isEntryId`,
/// `checkpoint.zig:100-104`).
pub fn is_entry_id(word: &str) -> bool {
    let bytes = word.as_bytes();
    bytes.len() >= 2
        && ENTRY_KINDS.as_bytes().contains(&bytes[0])
        && bytes[1..].iter().all(u8::is_ascii_digit)
}

/// The IDs an entry says it replaces, like `S1` in `...; replaces S1`
/// (`replacedIds`, `checkpoint.zig:83-97`).
pub fn replaced_ids(text: &str) -> Vec<String> {
    let lower = text.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut at = 0usize;
    while let Some(found) = lower[at..].find("replaces ") {
        at += found + "replaces ".len();
        for raw in text[at..]
            .split([' ', ',', ';'])
            .filter(|word| !word.is_empty())
        {
            let word = raw.trim_end_matches(['.', ')', ']']);
            if word == "and" {
                continue;
            }
            if !is_entry_id(word) {
                break;
            }
            out.push(word.to_string());
        }
    }
    out
}

/// What the model reads for `compacted` (`render`, `checkpoint.zig:275-302`).
///
/// Upstream's text with the three parts xfx has nothing for left out, each
/// of which upstream also leaves out when it has nothing for it: the earlier
/// ledgers' line (no fold, `:279-288`), the skills and MCP tools used
/// (`appendUsed` returns on none), and the line naming saved records the
/// model can open (`appendSavedLine` returns for a payload that is not
/// saved). The check note is absent because nothing here marks a note
/// unconfirmed (`appendCheckNote` returns on none).
pub fn render(compacted: &Compacted) -> String {
    let mut text = String::from(OPENING);
    for turn in &compacted.turns {
        render_turn(&mut text, turn);
    }
    if let Some(open) = &compacted.open {
        // `checkpoint.zig:290-295`.
        text.push_str("Turn in progress, whose first user message follows this:\n");
        for user in &open.users {
            text.push_str(&format!(
                "User, added while the assistant worked:\n{user}\n\n"
            ));
        }
        if !open.work.is_empty() {
            text.push_str(&format!("Assistant, in between so far:\n{}\n\n", open.work));
        }
        render_tools(
            &mut text,
            "Its tools so far",
            &open.tools,
            open.first_tool,
            open.last_tool,
        );
    }
    render_entries(&mut text, &compacted.entries);
    text.push_str(CLOSING);
    text
}

/// One turn (`renderTurn`, `checkpoint.zig:304-320`).
fn render_turn(text: &mut String, turn: &CompactedTurn) {
    if turn.number > 0 {
        text.push_str(&format!("Turn {}\n", turn.number));
    }
    for (index, user) in turn.users.iter().enumerate() {
        label(text, "User", turn.number);
        if index > 0 {
            text.push_str(", added while the assistant worked");
        }
        text.push_str(&format!(":\n{user}\n\n"));
    }
    if !turn.work.is_empty() {
        label(text, "Assistant", turn.number);
        text.push_str(&format!(", in between:\n{}\n\n", turn.work));
    }
    render_tools(text, "Tools", &turn.tools, turn.first_tool, turn.last_tool);
    if !turn.final_reply.is_empty() {
        label(text, "Assistant", turn.number);
        text.push_str(&format!(", final reply:\n{}\n\n", turn.final_reply));
    }
}

/// A line for each tool call, or only their range for a turn written before
/// the lines (`appendTools`, `checkpoint.zig:324-338`).
fn render_tools(text: &mut String, heading: &str, tools: &[ToolLine], first: usize, last: usize) {
    if !tools.is_empty() {
        text.push_str(&format!("{heading}:\n"));
        for tool in tools {
            text.push_str(&format!("  T{} {}", tool.number, tool.line));
            if !tool.why.is_empty() {
                text.push_str(&format!(": {}", tool.why));
            }
            text.push('\n');
        }
        text.push('\n');
    } else if first > 0 {
        text.push_str(&format!("{heading}: "));
        range(text, 'T', first, last);
        text.push_str("\n\n");
    }
}

/// The entries under their sections, each in the order added, an entry a
/// later one replaces marked so (`appendEntries`, `checkpoint.zig:342-372`).
fn render_entries(text: &mut String, entries: &[Entry]) {
    let mut replaced_by: Vec<Option<&str>> = vec![None; entries.len()];
    for entry in entries {
        for id in replaced_ids(&entry.text) {
            for (older, slot) in entries.iter().zip(replaced_by.iter_mut()) {
                if older.id == id {
                    *slot = Some(entry.id.as_str());
                }
            }
        }
    }
    let sections: [(&str, &str); 4] = [
        ("Rules of the session:", "R"),
        ("Facts of the session:", "F"),
        ("Decisions:", "D"),
        ("Status and open:", "SO"),
    ];
    for (heading, kinds) in sections {
        let mut written = 0usize;
        for (entry, replacer) in entries.iter().zip(&replaced_by) {
            let Some(first) = entry.id.chars().next() else {
                continue;
            };
            if !kinds.contains(first) {
                continue;
            }
            if written == 0 {
                text.push_str(&format!("{heading}\n"));
            }
            text.push_str(&entry.text);
            if let Some(id) = replacer {
                text.push_str(&format!("{REPLACED_MARK}{id})"));
            }
            text.push('\n');
            written += 1;
        }
        if written > 0 {
            text.push('\n');
        }
    }
}

fn label(text: &mut String, who: &str, number: usize) {
    text.push_str(who);
    if number > 0 {
        text.push_str(&format!(" {number}"));
    }
}

/// `T3` or `T3–T9` (`appendRange`, `checkpoint.zig:408-411`).
fn range(text: &mut String, prefix: char, first: usize, last: usize) {
    text.push_str(&format!("{prefix}{first}"));
    if last > first {
        text.push_str(&format!("\u{2013}{prefix}{last}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape of `receipts/w4`'s compacted turn: one read, its note, one
    /// fact.
    fn probe() -> Compacted {
        Compacted {
            entries: vec![Entry {
                id: "F1".into(),
                text: "F1 (T1): notes.txt holds SEED_VALUE_42".into(),
            }],
            turns: vec![CompactedTurn {
                number: 1,
                users: vec!["question one".into()],
                work: "Read notes.txt and found SEED_VALUE_42.".into(),
                final_reply: "ANSWER_1: I read the notes.".into(),
                first_tool: 1,
                last_tool: 1,
                tools: vec![ToolLine {
                    number: 1,
                    line: "read_file notes.txt (5 lines)".into(),
                    why: "read the notes; it shows SEED_VALUE_42".into(),
                }],
            }],
            open: None,
            turn_count: 1,
            tool_count: 1,
        }
    }

    #[test]
    fn the_text_is_upstreams_for_a_payload_that_is_not_saved() {
        // fx c1db919's request after `/compact` (`receipts/w4/README.md`,
        // "The next request"), less the "Saved word for word" line upstream writes only for
        // a session whose turns a model can open again (`appendSavedLine`).
        let expected = "<compacted_conversation>\nThis is the earlier part of this conversation, \
compacted. The user's messages and the assistant's final replies shown here are exact. What the \
assistant did in between is summarized, and each tool call has a line saying what it was and what \
it showed.\n\nTurn 1\nUser 1:\nquestion one\n\nAssistant 1, in between:\nRead notes.txt and found \
SEED_VALUE_42.\n\nTools:\n  T1 read_file notes.txt (5 lines): read the notes; it shows \
SEED_VALUE_42\n\nAssistant 1, final reply:\nANSWER_1: I read the notes.\n\nFacts of the session:\n\
F1 (T1): notes.txt holds SEED_VALUE_42\n\n</compacted_conversation>\n";
        assert_eq!(render(&probe()), expected);
    }

    #[test]
    fn a_turn_with_no_work_shows_only_its_exact_messages() {
        // A text-only turn compacted with no model call
        // (`receipts/w4/fx-compact-text`).
        let compacted = Compacted {
            entries: Vec::new(),
            turns: vec![CompactedTurn {
                number: 1,
                users: vec!["question one".into()],
                work: String::new(),
                final_reply: "ANSWER_1".into(),
                first_tool: 0,
                last_tool: 0,
                tools: Vec::new(),
            }],
            open: None,
            turn_count: 1,
            tool_count: 0,
        };
        assert_eq!(
            render(&compacted),
            format!(
                "{OPENING}Turn 1\nUser 1:\nquestion one\n\nAssistant 1, final reply:\nANSWER_1\n\n{CLOSING}"
            )
        );
    }

    #[test]
    fn an_entry_a_later_one_replaces_is_marked_and_sections_keep_their_order() {
        let compacted = Compacted {
            entries: vec![
                Entry {
                    id: "S1".into(),
                    text: "S1 (T1): the build fails".into(),
                },
                Entry {
                    id: "R1".into(),
                    text: "R1 (turn 1): \"never skip the tests\"".into(),
                },
                Entry {
                    id: "S2".into(),
                    text: "S2 (T2): the build passes; replaces S1".into(),
                },
                Entry {
                    id: "O1".into(),
                    text: "O1 (turn 2): push it?".into(),
                },
            ],
            ..Compacted::default()
        };
        let text = render(&compacted);
        assert!(text.contains(
            "Rules of the session:\nR1 (turn 1): \"never skip the tests\"\n\n\
             Status and open:\nS1 (T1): the build fails (replaced by S2)\n\
             S2 (T2): the build passes; replaces S1\nO1 (turn 2): push it?\n\n"
        ));
    }

    #[test]
    fn replaced_ids_reads_every_id_after_replaces() {
        assert_eq!(replaced_ids("S3: done; replaces S1 and O2."), ["S1", "O2"]);
        assert_eq!(replaced_ids("Replaces D4, D5"), ["D4", "D5"]);
        assert!(replaced_ids("replaces nothing").is_empty());
    }

    #[test]
    fn the_payload_keeps_upstreams_field_names() {
        let json = serde_json::to_value(probe()).expect("serializes");
        assert_eq!(json["turns"][0]["final"], "ANSWER_1: I read the notes.");
        assert_eq!(json["turn_count"], 1);
        assert_eq!(json["tool_count"], 1);
        let back: Compacted = serde_json::from_value(json).expect("reads back");
        assert_eq!(back, probe());
    }

    #[test]
    fn the_highest_ids_count_each_kind() {
        let mut compacted = probe();
        compacted.entries.push(Entry {
            id: "R4".into(),
            text: "R4: x".into(),
        });
        assert_eq!(compacted.highest_ids(), [4, 1, 0, 0, 0]);
    }
}
