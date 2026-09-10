//! `ask_user_question`. Bounds, sentinels, the terminal-safe encoding and the
//! answer document are upstream's (`vercel-labs/fx@580a0c5d`
//! `src/tools/agent/ask_user_question.zig`, `src/core/shared/text_utils.zig`,
//! `src/core/agent/question_answer.zig`).
use std::borrow::Cow;
use std::fmt::Write as _;

use serde_json::{Map, Value};

use super::spec::{
    ArraySpec, InputSchema, PermissionKind, Property, PropertyKind, ToolContext, ToolInput,
    ToolResult, ToolSpec,
};

pub const CANCEL_SENTINEL: &str = "(user cancelled the question)";
pub const NOT_AVAILABLE_SENTINEL: &str =
    "(ask_user_question is only available in the interactive shell; ask the user freeform instead)";
pub const FREEFORM_LABEL: &str = "Other";
pub const MIN_QUESTIONS: usize = 1;
pub const MAX_QUESTIONS: usize = 4;
pub const MIN_OPTIONS: usize = 2;
pub const MAX_OPTIONS: usize = 6;
/// xfx-local size bounds, measured **after** encoding. Upstream leaves these
/// open; xfx refuses rather than truncates, because the canonical question and
/// answer are what the model reads back and a silent clip changes their meaning.
pub const MAX_QUESTION_ENCODED_BYTES: usize = 512;
pub const MAX_LABEL_ENCODED_BYTES: usize = 128;
pub const MAX_DESCRIPTION_ENCODED_BYTES: usize = 256;
pub const MAX_FREEFORM_ENCODED_BYTES: usize = 4096;
/// The largest document `encode_answers` can produce under those bounds, proved
/// against `ToolLimits::default().max_output_bytes` by a test, so the registry's
/// backstop clip (`src/tools/mod.rs:145`) can never make a result invalid JSON.
pub const MAX_ENCODED_RESULT_BYTES: usize = 36_977;

/// Non-printing codepoints, exactly upstream's set (`text_utils.zig:593-599`).
fn is_non_printing(character: char) -> bool {
    matches!(character as u32,
        0x80..=0x9f | 0x200b..=0x200f | 0x2028..=0x202e | 0x2060..=0x206f | 0xfeff)
}

fn needs_escape(character: char) -> bool {
    let point = character as u32;
    point < 0x20 || point == 0x7f || is_non_printing(character)
}

/// `raw` with every sequence a terminal would obey rendered as a literal escape.
///
/// Lossless and idempotent: nothing is dropped, nothing is clipped, and `\` is
/// printable ASCII so an already-encoded string re-encodes to itself
/// (`text_utils.zig:533-557`). Upstream's invalid-UTF-8 branch has no analogue
/// here: a `&str` is valid UTF-8 by construction.
pub fn terminal_safe(raw: &str) -> Cow<'_, str> {
    if !raw.chars().any(needs_escape) {
        return Cow::Borrowed(raw);
    }
    let mut out = String::with_capacity(raw.len() + 16);
    for character in raw.chars() {
        let point = character as u32;
        if point < 0x20 || point == 0x7f {
            let _ = write!(out, "\\x{point:02x}");
        } else if is_non_printing(character) {
            let _ = write!(out, "\\u{{{point:04x}}}");
        } else {
            out.push(character);
        }
    }
    Cow::Owned(out)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionOption {
    pub label: String,
    pub description: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionEntry {
    pub question: String,
    pub options: Vec<QuestionOption>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AskUserQuestionInput {
    pub entries: Vec<QuestionEntry>,
    pub raw: Value,
}

impl AskUserQuestionInput {
    /// Undecoded arguments. The decoder has no context and cannot see whether a
    /// requester exists, and availability outranks a parse error, so the raw
    /// value travels to `execute` and is parsed there.
    pub fn raw(value: Value) -> Self {
        Self {
            entries: Vec::new(),
            raw: value,
        }
    }
}

/// Shows a batch of questions to whoever is running xfx and returns their
/// answers, one per question in order, or `None` if they declined to answer
/// at all.
///
/// The tools layer never names the TUI: this is the seam `execute` calls
/// through, injected on [`ToolContext`] by whichever front end built one. A
/// non-interactive run builds a context with no requester at all, and
/// `execute`'s availability check is what such a caller relies on -- it never
/// sees a requester that cannot really ask anyone anything.
pub trait QuestionRequester: Send + Sync {
    /// `entries` are already terminal-safe encoded, exactly as the user will
    /// see them. The answers returned are not yet bounded or encoded --
    /// `execute` does that on the way out, because the requester is a trait
    /// and a non-TUI implementation is not to be trusted with the model-visible
    /// document's shape.
    fn request(&self, entries: &[QuestionEntry]) -> Option<Vec<String>>;
}

fn bad(detail: &str) -> String {
    format!("(ask_user_question: {detail})")
}

/// Encoded, then bounded with a refusal. Never truncated.
pub(crate) fn bounded(raw: &str, cap: usize, what: &str) -> Result<String, String> {
    let encoded = terminal_safe(raw).into_owned();
    if encoded.len() > cap {
        return Err(bad(&format!("{what} is longer than {cap} encoded bytes")));
    }
    Ok(encoded)
}

/// Refuses a field the advertised closed schema does not contain.
fn only(object: &Map<String, Value>, allowed: &[&str], what: &str) -> Result<(), String> {
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(bad(what));
    }
    Ok(())
}

pub fn parse(value: &Value) -> Result<AskUserQuestionInput, String> {
    let args = value
        .as_object()
        .ok_or_else(|| bad("invalid arguments; provide {questions}"))?;
    only(
        args,
        &["questions"],
        "an argument this tool does not accept was sent",
    )?;
    let questions = args
        .get("questions")
        .ok_or_else(|| bad("missing required array \"questions\""))?
        .as_array()
        .ok_or_else(|| bad("\"questions\" must be an array"))?;
    if questions.len() < MIN_QUESTIONS || questions.len() > MAX_QUESTIONS {
        return Err(bad("provide 1 to 4 questions"));
    }
    let mut entries = Vec::with_capacity(questions.len());
    for item in questions {
        let item = item.as_object().ok_or_else(|| {
            bad("each question must be an object with a \"question\" and \"options\"")
        })?;
        only(
            item,
            &["question", "options"],
            "a question has a field this tool does not accept",
        )?;
        let text = item
            .get("question")
            .ok_or_else(|| bad("each question requires a \"question\" string"))?
            .as_str()
            .ok_or_else(|| bad("question \"question\" must be a string"))?;
        if text.trim().is_empty() {
            return Err(bad("question text must not be empty"));
        }
        let question = bounded(text.trim(), MAX_QUESTION_ENCODED_BYTES, "question text")?;
        let options = item
            .get("options")
            .ok_or_else(|| bad("each question requires an \"options\" array"))?
            .as_array()
            .ok_or_else(|| bad("\"options\" must be an array"))?;
        if options.len() < MIN_OPTIONS || options.len() > MAX_OPTIONS {
            return Err(bad("provide 2 to 6 options per question"));
        }
        let mut built: Vec<QuestionOption> = Vec::with_capacity(options.len());
        for option in options {
            let option = option
                .as_object()
                .ok_or_else(|| bad("each option must be an object with a \"label\""))?;
            only(
                option,
                &["label", "description"],
                "an option has a field this tool does not accept",
            )?;
            let raw_label = option
                .get("label")
                .ok_or_else(|| bad("each option requires a \"label\" string"))?
                .as_str()
                .ok_or_else(|| bad("option \"label\" must be a string"))?;
            if raw_label.trim().is_empty() {
                return Err(bad("option labels must not be empty"));
            }
            let label = bounded(raw_label.trim(), MAX_LABEL_ENCODED_BYTES, "option label")?;
            // After encoding, as upstream compares them, and ASCII-case
            // insensitively: two labels a user cannot tell apart are one choice.
            if built
                .iter()
                .any(|held| held.label.eq_ignore_ascii_case(&label))
            {
                return Err(bad("option labels must be unique within a question"));
            }
            // A non-string description is discarded, not refused
            // (`ask_user_question.zig:176-182`).
            let description = match option.get("description").and_then(Value::as_str) {
                Some(text) if !text.trim().is_empty() => Some(bounded(
                    text.trim(),
                    MAX_DESCRIPTION_ENCODED_BYTES,
                    "option description",
                )?),
                _ => None,
            };
            built.push(QuestionOption { label, description });
        }
        entries.push(QuestionEntry {
            question,
            options: built,
        });
    }
    Ok(AskUserQuestionInput {
        entries,
        raw: value.clone(),
    })
}

/// One element of the answer document.
///
/// A struct rather than a `serde_json::Map`, and that is not a style choice:
/// this crate does not enable serde_json's `preserve_order` (`Cargo.toml:42`),
/// so a `Map` is a `BTreeMap` and would emit `"answer"` before `"question"`.
/// Upstream's document is question-first (`question_answer.zig:23-28`), and a
/// derived struct serializes in declaration order.
#[derive(serde::Serialize)]
struct AnsweredQuestion<'a> {
    question: &'a str,
    answer: &'a str,
}

pub fn encode_answers(entries: &[QuestionEntry], answers: &[String]) -> Result<String, String> {
    if entries.len() != answers.len() {
        return Err(bad("the answers did not match the questions"));
    }
    let document: Vec<AnsweredQuestion<'_>> = entries
        .iter()
        .zip(answers)
        .map(|(entry, answer)| AnsweredQuestion {
            question: &entry.question,
            answer: answer.as_str(),
        })
        .collect();
    serde_json::to_string(&document).map_err(|err| bad(&err.to_string()))
}

// ---------------------------------------------------------------------------
// the spec
// ---------------------------------------------------------------------------

const LABEL_DESCRIPTION: &str =
    "The option's label. Shown to the user, and returned verbatim as the answer when chosen.";
const OPTION_DESCRIPTION_DESCRIPTION: &str =
    "Optional detail shown beneath the label to help the user choose.";
const QUESTION_TEXT_DESCRIPTION: &str = "The question text, shown to the user verbatim.";
const OPTIONS_DESCRIPTION: &str = "2 to 6 labeled choices for this question.";
const QUESTIONS_DESCRIPTION: &str =
    "1 to 4 questions to ask in order, each answered before the next is shown.";

/// Adapted from upstream's `ask_user_question_description`
/// (`vercel-labs/fx@580a0c5d` `src/builtins/tools.zig:358-359`), with the
/// `approval_request_id`/permission-screen sentence and its matching
/// "when NOT to use" clause dropped: this xfx tool mints no authority
/// (`PermissionKind::Interaction`, `question.rs` tests) and has no
/// `permission_request_id` argument in this schema, so there is nothing for
/// those sentences to refer to. This is a labeled adaptation, not a verbatim
/// quote.
const DESCRIPTION: &str = "Ask the user 1 to 4 multiple-choice questions in interactive runs \
    only when a concrete decision blocks progress after local files, git state, or tool output \
    cannot answer it. When to use: choose among precise, mutually exclusive paths before \
    acting, especially destructive or user-preference decisions. When NOT to use: discoverable \
    facts, trivial yes/no checks, open-ended discussion, or noninteractive runs; noninteractive \
    runs should surface a blocker in freeform text instead.";

static OPTION_SCHEMA: InputSchema = InputSchema {
    properties: &[
        Property {
            name: "label",
            kind: PropertyKind::String,
            description: LABEL_DESCRIPTION,
            allowed: &[],
            array: None,
        },
        Property {
            name: "description",
            kind: PropertyKind::String,
            description: OPTION_DESCRIPTION_DESCRIPTION,
            allowed: &[],
            array: None,
        },
    ],
    required: &["label"],
};

static QUESTION_SCHEMA: InputSchema = InputSchema {
    properties: &[
        Property {
            name: "question",
            kind: PropertyKind::String,
            description: QUESTION_TEXT_DESCRIPTION,
            allowed: &[],
            array: None,
        },
        Property {
            name: "options",
            kind: PropertyKind::Array,
            description: OPTIONS_DESCRIPTION,
            allowed: &[],
            array: Some(ArraySpec {
                items: &OPTION_SCHEMA,
                min_items: MIN_OPTIONS,
                max_items: MAX_OPTIONS,
            }),
        },
    ],
    required: &["question", "options"],
};

static INPUT_SCHEMA: InputSchema = InputSchema {
    properties: &[Property {
        name: "questions",
        kind: PropertyKind::Array,
        description: QUESTIONS_DESCRIPTION,
        allowed: &[],
        array: Some(ArraySpec {
            items: &QUESTION_SCHEMA,
            min_items: MIN_QUESTIONS,
            max_items: MAX_QUESTIONS,
        }),
    }],
    required: &["questions"],
};

fn decode(value: &Value) -> Result<ToolInput, String> {
    // Total on purpose: availability outranks a parse error and only `execute`
    // can see whether a requester exists. `parse` is the validator, and
    // `the_schema_is_validated_execute_side_because_the_decoder_is_total`
    // proves it runs on this side.
    Ok(ToolInput::AskUserQuestion(AskUserQuestionInput::raw(
        value.clone(),
    )))
}

fn validate(_input: &ToolInput) -> Result<(), String> {
    Ok(())
}

fn execute(input: &ToolInput, context: &ToolContext) -> ToolResult {
    let ToolInput::AskUserQuestion(input) = input else {
        return ToolResult::failure("ask_user_question received another tool's input");
    };
    let Some(requester) = context.questioner() else {
        return ToolResult::success(NOT_AVAILABLE_SENTINEL, "ask_user_question unavailable");
    };
    let parsed = match parse(&input.raw) {
        Ok(parsed) => parsed,
        Err(body) => return ToolResult::failure(body),
    };
    let Some(answers) = requester.request(&parsed.entries) else {
        return ToolResult::success(CANCEL_SENTINEL, "the question was cancelled");
    };
    if answers.len() != parsed.entries.len() {
        return ToolResult::failure(bad("the answers did not match the questions"));
    }
    let mut encoded = Vec::with_capacity(answers.len());
    for answer in &answers {
        match bounded(answer, MAX_FREEFORM_ENCODED_BYTES, "answer") {
            Ok(answer) => encoded.push(answer),
            Err(body) => return ToolResult::failure(body),
        }
    }
    match encode_answers(&parsed.entries, &encoded) {
        Ok(document) if document.len() <= context.limits().max_output_bytes => ToolResult::success(
            document,
            format!("answered {} question(s)", parsed.entries.len()),
        ),
        Ok(_) => ToolResult::failure(bad("the answers were too long to return")),
        Err(problem) => ToolResult::failure(problem),
    }
}

/// `ask_user_question`. Mints no authority: `PermissionKind::Interaction`
/// requires none, and the executor above never touches the filesystem or a
/// process. `permission_request_id` is deliberately not advertised -- that
/// route is P3-APPROVAL.
pub const ASK_USER_QUESTION: ToolSpec = ToolSpec::new(
    "ask_user_question",
    DESCRIPTION,
    PermissionKind::Interaction,
    INPUT_SCHEMA,
    decode,
    validate,
    execute,
);

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use tempfile::TempDir;

    use super::*;
    use crate::gateway::protocol::ToolCall;
    use crate::tools::{Registry, ToolLimits};
    use crate::workspace::AccessScope;
    use serde_json::json;

    /// A batch of `count` well-formed questions, each with two distinct options.
    fn qs(count: usize) -> Value {
        json!({"questions": (0..count).map(|index| json!({
            "question": format!("q{index}"),
            "options": [{"label": "a"}, {"label": "b"}]})).collect::<Vec<_>>()})
    }

    /// One well-formed question with `count` distinct options.
    fn os(count: usize) -> Value {
        json!({"questions": [{"question": "q",
            "options": (0..count).map(|index| json!({"label": format!("o{index}")}))
                .collect::<Vec<_>>()}]})
    }

    /// A parsed entry with no options: the encoder's cases are about text, not choices.
    fn entry(question: &str) -> QuestionEntry {
        QuestionEntry {
            question: question.to_string(),
            options: Vec::new(),
        }
    }

    /// One well-formed question, named so an answer's document is predictable.
    fn one_question() -> Value {
        json!({"questions": [
            {"question": "Which depth?", "options": [{"label": "Thorough"}, {"label": "Quick"}]},
        ]})
    }

    /// Two well-formed questions, named so an answer's document is predictable.
    fn two_questions() -> Value {
        json!({"questions": [
            {"question": "Which depth?", "options": [{"label": "Thorough"}, {"label": "Quick"}]},
            {"question": "Ship it?", "options": [{"label": "now"}, {"label": "later"}]},
        ]})
    }

    /// A scripted answer to a batch: decline, answer with fixed text, or
    /// record the entries the executor showed and decline.
    enum Answering {
        Cancel,
        With(Vec<String>),
        Record(Arc<Mutex<Vec<QuestionEntry>>>),
    }

    struct Scripted(Answering);

    impl QuestionRequester for Scripted {
        fn request(&self, entries: &[QuestionEntry]) -> Option<Vec<String>> {
            match &self.0 {
                Answering::Cancel => None,
                Answering::With(answers) => Some(answers.clone()),
                Answering::Record(seen) => {
                    seen.lock().unwrap().extend_from_slice(entries);
                    None
                }
            }
        }
    }

    /// A context built with `a` as its requester and the shipped limits.
    fn scripted(a: Answering) -> (TempDir, ToolContext) {
        let dir = tempfile::tempdir().unwrap();
        let scope = AccessScope::primary_only(dir.path()).unwrap();
        (
            dir,
            ToolContext::new(scope).with_questioner(Arc::new(Scripted(a))),
        )
    }

    /// The same as `scripted`, but with `max_output_bytes` set to `bytes`
    /// rather than the shipped default. `with_limits` is a constructor, so it
    /// comes first, and `with_questioner` still runs after it.
    fn tight(a: Answering, bytes: usize) -> (TempDir, ToolContext) {
        let dir = tempfile::tempdir().unwrap();
        let scope = AccessScope::primary_only(dir.path()).unwrap();
        let context = ToolContext::with_limits(
            scope,
            ToolLimits {
                max_output_bytes: bytes,
                ..ToolLimits::default()
            },
        )
        .with_questioner(Arc::new(Scripted(a)));
        (dir, context)
    }

    fn call(name: &str, args: &Value) -> ToolCall {
        ToolCall {
            id: "c1".to_string(),
            name: name.to_string(),
            input: args.clone(),
        }
    }

    /// Runs `ask_user_question` through the real registry, the way a turn does.
    fn run(args: &Value, context: &ToolContext) -> ToolResult {
        Registry::builtin()
            .execute(&call("ask_user_question", args), context)
            .unwrap()
    }

    #[test]
    fn the_encoder_is_upstreams_lossless_one() {
        // `text_utils.zig:533-599`; expectations are upstream's own
        // (`ask_user_question.zig:283-289`).
        for (raw, expected) in [
            ("Q\n\u{1b}[31m?", r"Q\x0a\x1b[31m?"),
            ("Alpha\nFake", r"Alpha\x0aFake"),
            ("Desc\tGap", r"Desc\x09Gap"),
            ("\u{7f}", r"\x7f"),
            ("\u{9b}", r"\u{009b}"), // C1: a CSI on a decoding terminal
            ("a\u{200b}b", r"a\u{200b}b"),
            ("\u{feff}", r"\u{feff}"),
            ("네—ok", "네—ok"),       // printable non-ASCII stays literal
            (r"C:\path", r"C:\path"), // a backslash is printable ASCII
        ] {
            assert_eq!(terminal_safe(raw), expected, "encoding {raw:?}");
        }
    }

    #[test]
    fn the_encoder_is_idempotent_and_borrows_when_nothing_changes() {
        assert!(matches!(terminal_safe("plain text"), Cow::Borrowed(_)));
        let once = terminal_safe("a\nb").into_owned();
        assert_eq!(
            terminal_safe(&once),
            once,
            "re-encoding an encoded string changes nothing"
        );
    }

    #[test]
    fn parsing_refuses_out_of_bounds_batches_with_the_upstream_bodies() {
        for (args, expected) in [
            (qs(0), "(ask_user_question: provide 1 to 4 questions)"),
            (qs(5), "(ask_user_question: provide 1 to 4 questions)"),
            (
                os(1),
                "(ask_user_question: provide 2 to 6 options per question)",
            ),
            (
                os(7),
                "(ask_user_question: provide 2 to 6 options per question)",
            ),
            (
                json!({"questions": [{"question": "   ", "options": [{"label": "a"}, {"label": "b"}]}]}),
                "(ask_user_question: question text must not be empty)",
            ),
            // A literal space, not a tab: the encoder turns a tab into `\x09`, so
            // `SHIP\x09IT` and `Ship it` are genuinely different labels and would
            // not exercise the duplicate rule at all.
            (
                json!({"questions": [{"question": "q",
                        "options": [{"label": "Ship it"}, {"label": "SHIP IT"}]}]}),
                "(ask_user_question: option labels must be unique within a question)",
            ),
        ] {
            assert_eq!(parse(&args).unwrap_err(), expected, "for {args}");
        }
        assert!(
            parse(&qs(1)).is_ok()
                && parse(&qs(4)).is_ok()
                && parse(&os(2)).is_ok()
                && parse(&os(6)).is_ok()
        );
        // The other side of the same rule: encoding is what the comparison sees.
        assert!(
            parse(&json!({"questions": [{"question": "q",
                "options": [{"label": "Ship it"}, {"label": "SHIP\tIT"}]}]}))
            .is_ok(),
            "a tab makes the label distinct once it is `\\x09`"
        );
    }

    #[test]
    fn overlong_text_is_refused_rather_than_truncated() {
        // The canonical question is what the model reads back in the result; a plan
        // that clipped it would change the question's meaning silently.
        let long = "x".repeat(MAX_QUESTION_ENCODED_BYTES + 1);
        assert_eq!(
            parse(&json!({"questions": [{"question": long,
            "options": [{"label": "a"}, {"label": "b"}]}]}))
            .unwrap_err(),
            "(ask_user_question: question text is longer than 512 encoded bytes)"
        );
        // Interior newlines between two ordinary characters. A label of newlines
        // alone trims to empty and is refused for emptiness instead, which would
        // not exercise this bound at all: 1 + 32*4 + 1 = 130 encoded bytes.
        let escapes = format!("a{}b", "\n".repeat(32));
        assert_eq!(
            parse(&json!({"questions": [{"question": "q",
            "options": [{"label": escapes}, {"label": "b"}]}]}))
            .unwrap_err(),
            "(ask_user_question: option label is longer than 128 encoded bytes)"
        );
    }

    #[test]
    fn the_closed_schema_is_enforced_here_because_only_this_side_can_enforce_it() {
        // `additionalProperties: false` is advertised (`spec.rs:98-101`); a provider
        // that does not enforce it must not make xfx accept a field it never offered.
        assert_eq!(
            parse(&json!({"questions": [{"question": "q", "why": "extra",
            "options": [{"label": "a"}, {"label": "b"}]}]}))
            .unwrap_err(),
            "(ask_user_question: a question has a field this tool does not accept)"
        );
        assert_eq!(
            parse(&json!({"questions": [], "permission_request_id": "x"})).unwrap_err(),
            "(ask_user_question: an argument this tool does not accept was sent)"
        );
    }

    #[test]
    fn a_non_string_description_is_discarded_rather_than_refused() {
        let parsed = parse(&json!({"questions": [{"question": "q",
            "options": [{"label": "a", "description": 7}, {"label": "b"}]}]}))
        .expect("parsed");
        assert_eq!(parsed.entries[0].options[0].description, None);
    }

    #[test]
    fn answers_encode_in_question_order_with_json_escaping() {
        let entries = vec![entry("Which depth?"), entry("Ship it?")];
        assert_eq!(
            encode_answers(&entries, &["Thorough".into(), "Yes \"now\"".into()]).unwrap(),
            r#"[{"question":"Which depth?","answer":"Thorough"},{"question":"Ship it?","answer":"Yes \"now\""}]"#
        );
        assert!(encode_answers(&entries, &["only one".into()]).is_err());
    }

    #[test]
    fn the_worst_case_result_fits_the_shipped_output_limit() {
        // An upper bound, and tight: after `terminal_safe` no control characters
        // remain, so serde can only expand `"` and `\`, to two bytes each. The
        // element's fixed overhead is counted byte by byte rather than rounded:
        //   {  "question"  :  ""  ,  "answer"  :  ""  }
        //   1 +   10     + 1 + 2 + 1 +   8    + 1 + 2 + 1  = 27
        const PER_QUESTION: usize =
            27 + 2 * MAX_QUESTION_ENCODED_BYTES + 2 * MAX_FREEFORM_ENCODED_BYTES;
        const WORST: usize = 2                                  // the enclosing []
            + MAX_QUESTIONS * PER_QUESTION
            + (MAX_QUESTIONS - 1); // the commas between them
        assert_eq!(MAX_ENCODED_RESULT_BYTES, WORST);
        assert_eq!(WORST, 36_977);
        assert!(WORST <= ToolLimits::default().max_output_bytes);
    }

    #[test]
    fn without_a_requester_the_tool_says_so_before_it_parses() {
        let dir = tempfile::tempdir().unwrap();
        let context = ToolContext::new(AccessScope::primary_only(dir.path()).unwrap());
        let result = run(&json!({"questions": []}), &context); // a malformed batch, no questioner
        assert!(result.ok);
        assert_eq!(
            result.output, NOT_AVAILABLE_SENTINEL,
            "availability outranks the parse error"
        );
    }

    #[test]
    fn the_schema_is_validated_execute_side_because_the_decoder_is_total() {
        let (_dir, context) = scripted(Answering::Cancel);
        for (args, expected) in [
            (
                json!({"questions": []}),
                "(ask_user_question: provide 1 to 4 questions)",
            ),
            (
                json!({"questions": [{"question": "q", "why": "x",
                    "options": [{"label": "a"}, {"label": "b"}]}]}),
                "(ask_user_question: a question has a field this tool does not accept)",
            ),
            (
                json!({"nope": 1}),
                "(ask_user_question: an argument this tool does not accept was sent)",
            ),
        ] {
            let result = run(&args, &context);
            assert!(
                !result.ok,
                "a batch the model got wrong is a refusal it can correct"
            );
            assert_eq!(result.output, expected);
        }
    }

    #[test]
    fn a_cancelled_batch_is_the_cancel_sentinel_and_not_a_refusal() {
        let (_dir, context) = scripted(Answering::Cancel);
        let result = run(&two_questions(), &context);
        assert!(result.ok, "the user declining is not the model's mistake");
        assert_eq!(result.output, CANCEL_SENTINEL);
    }

    #[test]
    fn answers_reach_the_model_as_ordered_question_answer_json() {
        let (_dir, context) = scripted(Answering::With(vec!["Thorough".into(), "later".into()]));
        let result = run(&two_questions(), &context);
        assert!(result.ok);
        assert_eq!(
            result.output,
            r#"[{"question":"Which depth?","answer":"Thorough"},{"question":"Ship it?","answer":"later"}]"#
        );
    }

    #[test]
    fn the_requester_is_handed_the_encoded_entries_it_will_display() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (_dir, context) = scripted(Answering::Record(Arc::clone(&seen)));
        let _ = run(
            &json!({"questions": [{"question": "a\u{1b}[31mb",
                "options": [{"label": "x"}, {"label": "y"}]}]}),
            &context,
        );
        assert_eq!(seen.lock().unwrap()[0].question, r"a\x1b[31mb");
    }

    #[test]
    fn an_answer_from_the_requester_is_encoded_and_bounded_before_it_leaves() {
        // The requester is a trait. A non-TUI implementation returning a control
        // sequence or a megabyte must not reach the model or the registry's clip.
        let (_dir, context) = scripted(Answering::With(vec!["ok\u{1b}[2J".into()]));
        assert_eq!(
            run(&one_question(), &context).output,
            r#"[{"question":"Which depth?","answer":"ok\\x1b[2J"}]"#
        );
        let (_dir, context) = scripted(Answering::With(vec![
            "x".repeat(MAX_FREEFORM_ENCODED_BYTES + 1)
        ]));
        let refused = run(&one_question(), &context);
        assert!(!refused.ok);
        assert_eq!(
            refused.output,
            "(ask_user_question: answer is longer than 4096 encoded bytes)"
        );
    }

    #[test]
    fn a_result_the_callers_limit_cannot_carry_is_refused_before_the_registry_clips_it() {
        // `Registry::execute` clips past `max_output_bytes` (`src/tools/mod.rs:145`),
        // and clipped JSON is not JSON.
        //
        // NOTE: the plan's original fixture answer ("a moderate answer") serializes
        // this document to 58 bytes, which does not exceed the 64-byte limit below
        // and would not exercise this refusal at all (measured directly). A
        // 40-byte answer produces an 81-byte document instead, which does.
        let (_dir, context) = tight(Answering::With(vec!["x".repeat(40)]), 64);
        let result = run(&one_question(), &context);
        assert!(!result.ok);
        assert_eq!(
            result.output,
            "(ask_user_question: the answers were too long to return)"
        );
        assert!(
            result.output.len() <= 64,
            "the refusal itself fits the limit"
        );
    }

    #[test]
    fn a_requester_that_returns_the_wrong_answer_count_is_refused_through_the_registry() {
        // Run through `Registry::execute`, not `execute` directly, so this
        // proves the refusal survives the same dispatch a real turn uses.
        let (_dir, context) = scripted(Answering::With(vec!["only one".into()]));
        let result = run(&two_questions(), &context);
        assert!(
            !result.ok,
            "a requester that hands back the wrong count is not the model's mistake to silently absorb"
        );
        assert_eq!(
            result.output,
            "(ask_user_question: the answers did not match the questions)"
        );
    }

    #[test]
    fn the_result_exactly_at_the_output_limit_succeeds_and_one_byte_short_of_it_fails() {
        // The bound is measured against the real, computed serialized document
        // rather than a guessed byte count, so this proves the `<=` in
        // `execute`'s final match, not just "some limit exists somewhere".
        //
        // The answer is 40 `x`s, not something short like "ok": the refusal
        // message itself is 56 bytes, and `Registry::execute` clips whatever
        // `execute` returns to the same `max_output_bytes` (`mod.rs:145`), so
        // a one-byte-short limit under ~56 would truncate the refusal text
        // too and this test would be asserting against a clipped message
        // instead of the one `execute` actually produced.
        let answer = "x".repeat(40);
        let entries = vec![entry("Which depth?")];
        let document = encode_answers(&entries, std::slice::from_ref(&answer)).unwrap();
        let exact = document.len();

        let (_dir, context) = tight(Answering::With(vec![answer.clone()]), exact);
        let result = run(&one_question(), &context);
        assert!(result.ok, "a document exactly the size of the limit fits");
        assert_eq!(result.output, document);

        let (_dir, context) = tight(Answering::With(vec![answer]), exact - 1);
        let result = run(&one_question(), &context);
        assert!(
            !result.ok,
            "a limit one byte short of the real document must not fit"
        );
        assert_eq!(
            result.output,
            "(ask_user_question: the answers were too long to return)"
        );
    }

    #[test]
    fn the_requester_survives_a_context_clone_and_the_with_permissions_builder() {
        let (_dir, context) = scripted(Answering::With(vec!["Thorough".into()]));

        let cloned = context.clone();
        assert!(
            cloned.questioner().is_some(),
            "a clone must carry the same requester, not start over with none"
        );
        assert!(run(&one_question(), &cloned).ok);

        let rebuilt = context.with_permissions(crate::permission::PermissionSession::default());
        assert!(
            rebuilt.questioner().is_some(),
            "`with_permissions` replaces the session, not the requester set before it"
        );
        assert!(run(&one_question(), &rebuilt).ok);
    }

    #[test]
    fn the_question_tool_mints_no_authority_and_every_kind_is_named() {
        assert_eq!(
            Registry::builtin()
                .spec("ask_user_question")
                .expect("advertised")
                .permission(),
            PermissionKind::Interaction
        );
        for spec in Registry::builtin().specs() {
            // No wildcard: a new kind must be decided here rather than defaulting.
            match spec.permission() {
                PermissionKind::ReadOnly | PermissionKind::Interaction => {
                    assert!(!spec.permission().requires_authority())
                }
                PermissionKind::MutateFile | PermissionKind::RunCommand => {
                    assert!(spec.permission().requires_authority())
                }
            }
        }
    }
}
