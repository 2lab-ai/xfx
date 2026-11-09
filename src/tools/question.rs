//! `ask_user_question`. Bounds, sentinels, the terminal-safe encoding and the
//! answer document are upstream's (`vercel-labs/fx@580a0c5d`
//! `src/tools/agent/ask_user_question.zig`, `src/core/shared/text_utils.zig`,
//! `src/core/agent/question_answer.zig`).
use std::borrow::Cow;
use std::fmt::Write as _;

use serde_json::{Map, Value};

use super::spec::{
    Activity, ArraySpec, InputSchema, Outcome, PermissionKind, Presentation, Property,
    PropertyKind, ToolContext, ToolInput, ToolResult, ToolSpec,
};

pub const CANCEL_SENTINEL: &str = "(user cancelled the question)";
/// What a call the user stopped the turn at answers, in upstream's words
/// (`aborted_tool_output`, `src/core/session/session.zig:64`, `c1db919`): a
/// question ended by the interrupt is not a question the user declined, and
/// the model is never told the [`CANCEL_SENTINEL`] about it.
pub const ABORTED_OUTPUT: &str = "aborted by user";
pub const NOT_AVAILABLE_SENTINEL: &str =
    "(ask_user_question is only available in the interactive shell; ask the user freeform instead)";
pub const FREEFORM_LABEL: &str = "Other";
pub const MIN_QUESTIONS: usize = 1;
/// Upstream's `max_questions` (`question_answer.zig:7-9@481104fd`, 28cf53b5):
/// the schema's `maxItems`, the description, the argument refusal and the
/// worst-case document below all share it.
pub const MAX_QUESTIONS: usize = 20;
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
/// backstop clip (`src/tools/mod.rs:346`) can never make a result invalid JSON.
pub const MAX_ENCODED_RESULT_BYTES: usize = 184_881;

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

/// [`terminal_safe`], for an **answer**: the same encoding with the line feed
/// left as it is.
///
/// A freeform answer can hold a line break -- a pasted paragraph -- and
/// upstream hands it to the model as one (`insertFreeformSlice`,
/// `question_prompt.zig:573-583`; e2e `tests/e2e/tui-decision-prompts.test.ts:2785`
/// asserts the newline reaches the request). Every other control is still
/// named, so the answer document carries nothing a terminal would obey but
/// the break.
pub fn answer_safe(raw: &str) -> Cow<'_, str> {
    if !raw
        .chars()
        .any(|character| character != '\n' && needs_escape(character))
    {
        return Cow::Borrowed(raw);
    }
    let mut out = String::with_capacity(raw.len() + 16);
    for line in raw.split_inclusive('\n') {
        let (body, broke) = match line.strip_suffix('\n') {
            Some(body) => (body, true),
            None => (line, false),
        };
        out.push_str(&terminal_safe(body));
        if broke {
            out.push('\n');
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

/// What the person a batch was put to did with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    /// One answer per question, in order.
    Answered(Vec<String>),
    /// They declined to answer at all -- or the batch could not be put to
    /// them -- and the turn goes on: the model is told so with
    /// [`CANCEL_SENTINEL`].
    Declined,
    /// The turn the batch belonged to was stopped while it was up, which is
    /// how both Escape and Ctrl-C end a batch (upstream's
    /// `cancelQuestionPrompt`, `src/core/app/input_question_runtime.zig:59-83`,
    /// `c1db919`). The call answers [`ABORTED_OUTPUT`] and the turn ends with
    /// it.
    Interrupted,
}

/// Shows a batch of questions to whoever is running xfx and returns what they
/// did with it ([`Reply`]).
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
    fn request(&self, entries: &[QuestionEntry]) -> Reply;
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

/// [`bounded`], for an answer: encoded by [`answer_safe`], so a line break the
/// user pasted reaches the model as one.
fn bounded_answer(raw: &str) -> Result<String, String> {
    let encoded = answer_safe(raw).into_owned();
    if encoded.len() > MAX_FREEFORM_ENCODED_BYTES {
        return Err(bad(&format!(
            "answer is longer than {MAX_FREEFORM_ENCODED_BYTES} encoded bytes"
        )));
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
        // `question_count_sentinel` (`ask_user_question.zig:19-22@481104fd`).
        return Err(bad(&format!(
            "provide {MIN_QUESTIONS} to {MAX_QUESTIONS} questions"
        )));
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

/// One question of a saved answer document, as [`decode_answers`] reads it
/// back: the question as the model was shown it and the answer it was sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedAnswer {
    pub question: String,
    pub answer: String,
}

/// A call's answer document read back off a saved session, or `None` for
/// anything else: upstream's `decodeJson` (`question_answer.zig:43-74@481104fd`),
/// which a resume runs on a successful `ask_user_question` result before it
/// draws the batch's resolutions (`writeAnsweredQuestionResult`,
/// `app_session_runtime.zig:4589-4607`).
///
/// The document is file input -- anyone who can write the session log can
/// change it -- and this is its whole bound, as upstream's is: one JSON array
/// of [`MIN_QUESTIONS`] to [`MAX_QUESTIONS`] objects, each with a string
/// `question` and a string `answer`, other keys ignored as `object.get`
/// ignores them. Anything else is `None`, never a panic: text that is not JSON
/// (a sentinel, a refusal), JSON with more after it, an array of none or of
/// more than twenty, an element that is not an object (`:63`), a field missing
/// or not a string -- upstream's own rejects (`question_answer.zig:101-111`,
/// `:113-141`). Any key given twice is refused as well, `question`, `answer`
/// or one nobody reads: upstream's `std.json` default
/// (`duplicate_field_behavior`) is taken to refuse it, unread at the pin, and
/// refusing it is the reading that is never looser than upstream's.
///
/// Read by hand rather than derived: a derived struct also takes serde's
/// positional form, so `[["Q","A"]]` would be an answer. The array is read
/// element by element and refused at the twenty-first, before it is built;
/// each element is read as an object only ([`Element`]). Neither goes through
/// `serde_json::Value`, whose parse stops at a depth of 128: a key nobody
/// reads is skipped ([`serde::de::IgnoredAny`]) at any depth, as upstream's
/// heap-grown parse keeps it. Nothing here is made safe for a terminal: the
/// rows that show it are (`crate::tui::question`'s `resolution_rows`).
pub fn decode_answers(output: &str) -> Option<Vec<SavedAnswer>> {
    serde_json::from_str::<Batch>(output)
        .ok()
        .map(|batch| batch.0)
}

/// The document's array, read by [`BatchVisitor`].
struct Batch(Vec<SavedAnswer>);

impl<'de> serde::Deserialize<'de> for Batch {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_seq(BatchVisitor)
    }
}

struct BatchVisitor;

impl<'de> serde::de::Visitor<'de> for BatchVisitor {
    type Value = Batch;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "an array of {MIN_QUESTIONS} to {MAX_QUESTIONS} answered questions"
        )
    }

    fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Batch, A::Error> {
        let mut answers = Vec::new();
        loop {
            if answers.len() == MAX_QUESTIONS {
                // The twenty-first is refused before it is read into
                // anything: only whether there is one is asked.
                return match seq.next_element::<serde::de::IgnoredAny>()? {
                    Some(_) => Err(serde::de::Error::invalid_length(MAX_QUESTIONS + 1, &self)),
                    None => Ok(Batch(answers)),
                };
            }
            match seq.next_element::<Element>()? {
                Some(Element(answer)) => answers.push(answer),
                None if answers.len() < MIN_QUESTIONS => {
                    return Err(serde::de::Error::invalid_length(answers.len(), &self))
                }
                None => return Ok(Batch(answers)),
            }
        }
    }
}

/// One element of the array: an object and nothing else
/// (`question_answer.zig:63@481104fd`).
struct Element(SavedAnswer);

impl<'de> serde::Deserialize<'de> for Element {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(ElementVisitor)
    }
}

struct ElementVisitor;

impl<'de> serde::de::Visitor<'de> for ElementVisitor {
    type Value = Element;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("an object with a string `question` and a string `answer`")
    }

    // `visit_map` alone: an array, a string or a number is the visitor's
    // default refusal (`invalid type`), never a positional answer.
    //
    // Any key given twice is refused, the two read and every other alike:
    // upstream's object is one map, which its parse refuses to fill twice
    // under the default `duplicate_field_behavior`.
    fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Element, A::Error> {
        let (mut question, mut answer) = (None, None);
        let mut others = std::collections::HashSet::new();
        while let Some(key) = map.next_key::<String>()? {
            let slot = match key.as_str() {
                "question" => &mut question,
                "answer" => &mut answer,
                _ => {
                    map.next_value::<serde::de::IgnoredAny>()?;
                    if !others.insert(key) {
                        return Err(serde::de::Error::custom("a key given twice"));
                    }
                    continue;
                }
            };
            if slot.is_some() {
                return Err(serde::de::Error::custom(format!("`{key}` given twice")));
            }
            *slot = Some(map.next_value::<String>()?);
        }
        Ok(Element(SavedAnswer {
            question: question.ok_or_else(|| serde::de::Error::missing_field("question"))?,
            answer: answer.ok_or_else(|| serde::de::Error::missing_field("answer"))?,
        }))
    }
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
    "1 to 20 questions to ask in order, each answered before the next is shown.";

/// Adapted from upstream's `ask_user_question_description`
/// (`vercel-labs/fx@580a0c5d` `src/builtins/tools.zig:358-359`), with the
/// `approval_request_id`/permission-screen sentence and its matching
/// "when NOT to use" clause dropped: this xfx tool mints no authority
/// (`PermissionKind::Interaction`, `question.rs` tests) and has no
/// `permission_request_id` argument in this schema, so there is nothing for
/// those sentences to refer to. This is a labeled adaptation, not a verbatim
/// quote. Its opening words are v0.0.16's own, the count included
/// (`ask_user_question_description`, `builtins/tools.zig:119-122@481104fd`).
const DESCRIPTION: &str = "Ask the user 1-20 multiple-choice questions in interactive runs \
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
    let answers = match requester.request(&parsed.entries) {
        Reply::Answered(answers) => answers,
        Reply::Declined => {
            return ToolResult::success(CANCEL_SENTINEL, "the question was cancelled")
        }
        Reply::Interrupted => {
            return ToolResult::failure(ABORTED_OUTPUT).ending(Outcome::Cancelled)
        }
    };
    if answers.len() != parsed.entries.len() {
        return ToolResult::failure(bad("the answers did not match the questions"));
    }
    let mut encoded = Vec::with_capacity(answers.len());
    for answer in &answers {
        match bounded_answer(answer) {
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

/// The call's verbs (`action_label = "Asking"`, `completed_action_label =
/// "Asked"`, `builtins/tools.zig:615-616@481104fd`). No live row says either:
/// a question is never part of a tool group. A resume's row for a call that
/// left no answered batch says the second (`crate::tui::question`'s
/// `asked_row`).
pub const PRESENTATION: Presentation = Presentation::new(Activity::Ask, "Asking", "Asked", "");

/// `ask_user_question`. Mints no authority: `PermissionKind::Interaction`
/// requires none, and the executor above never touches the filesystem or a
/// process. `permission_request_id` is deliberately not advertised -- that
/// route is P3-APPROVAL.
pub const ASK_USER_QUESTION: ToolSpec = ToolSpec::new(
    "ask_user_question",
    DESCRIPTION,
    PermissionKind::Interaction,
    PRESENTATION,
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
        Interrupt,
        With(Vec<String>),
        Record(Arc<Mutex<Vec<QuestionEntry>>>),
    }

    struct Scripted(Answering);

    impl QuestionRequester for Scripted {
        fn request(&self, entries: &[QuestionEntry]) -> Reply {
            match &self.0 {
                Answering::Cancel => Reply::Declined,
                Answering::Interrupt => Reply::Interrupted,
                Answering::With(answers) => Reply::Answered(answers.clone()),
                Answering::Record(seen) => {
                    seen.lock().unwrap().extend_from_slice(entries);
                    Reply::Declined
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
            // Upstream's bound and its words at both ends
            // (`ask_user_question.zig:19-22`, `:191`, `:261@481104fd`).
            (qs(0), "(ask_user_question: provide 1 to 20 questions)"),
            (qs(21), "(ask_user_question: provide 1 to 20 questions)"),
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
                && parse(&qs(5)).is_ok()
                && parse(&qs(20)).is_ok()
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

    fn saved(question: &str, answer: &str) -> SavedAnswer {
        SavedAnswer {
            question: question.into(),
            answer: answer.into(),
        }
    }

    /// The document `encode_answers` writes for `count` questions.
    fn document(count: usize) -> String {
        let entries: Vec<QuestionEntry> = (1..=count)
            .map(|index| entry(&format!("Question {index}?")))
            .collect();
        let answers: Vec<String> = (1..=count).map(|index| format!("Answer {index}")).collect();
        encode_answers(&entries, &answers).unwrap()
    }

    #[test]
    fn a_saved_answer_document_reads_back_in_order_with_its_escapes() {
        // `question_answer.zig:76-99@481104fd`: the encoder's document decodes
        // to the same pairs, in order, its escapes undone.
        let entries = vec![entry("Which depth?"), entry("Ship it?")];
        let written =
            encode_answers(&entries, &["Thorough".into(), "Yes \"now\"\nlater".into()]).unwrap();
        assert_eq!(
            decode_answers(&written),
            Some(vec![
                saved("Which depth?", "Thorough"),
                saved("Ship it?", "Yes \"now\"\nlater")
            ])
        );
        // Whitespace around the document is JSON's, and a key the decoder
        // does not read is ignored, as `object.get` ignores it.
        assert_eq!(
            decode_answers(" [{\"answer\":\"A\",\"question\":\"Q\",\"extra\":[1,{}]}]\n"),
            Some(vec![saved("Q", "A")])
        );
    }

    #[test]
    fn a_saved_document_of_one_to_twenty_questions_decodes_and_no_other_count_does() {
        // `question_answer.zig:59` and `:113-141@481104fd`: the bound is the
        // tool's own, both ends of it.
        assert_eq!(MIN_QUESTIONS, 1);
        assert_eq!(decode_answers(&document(1)).map(|got| got.len()), Some(1));
        assert_eq!(decode_answers(&document(4)).map(|got| got.len()), Some(4));
        let twenty = decode_answers(&document(MAX_QUESTIONS)).expect("a full batch");
        assert_eq!(twenty.len(), MAX_QUESTIONS);
        assert_eq!(twenty[19], saved("Question 20?", "Answer 20"));
        assert_eq!(decode_answers(&document(MAX_QUESTIONS + 1)), None);
        assert_eq!(decode_answers("[]"), None);
    }

    #[test]
    fn the_twenty_first_element_is_refused_at_its_first_byte_unread() {
        // The array's visitor asks only whether a twenty-first element is
        // there and refuses it with the count, before its value is read:
        // one that is malformed, unclosed or enormous gives the count's error,
        // never the parse error reading it would give.
        let twenty = document(MAX_QUESTIONS);
        let open = &twenty[..twenty.len() - 1];
        let huge = format!("\"{}\"", "x".repeat(8 << 20));
        for tail in [
            r#"{"question":"#.to_string(),
            "[".repeat(100_000),
            huge,
            "\u{1}not json".to_string(),
        ] {
            let source = format!("{open},{tail}");
            let refused = serde_json::from_str::<Batch>(&source)
                .err()
                .expect("twenty-one is refused");
            assert_eq!(
                refused.classify(),
                serde_json::error::Category::Data,
                "{refused}"
            );
            assert!(
                refused.to_string().starts_with("invalid length 21"),
                "{refused}"
            );
        }
    }

    #[test]
    fn a_skipped_value_is_not_checked_as_upstreams_parse_checks_it() {
        // Looser than upstream, and only here: upstream's `std.json` refuses
        // a key given twice in **any** object and an unpaired surrogate in
        // **any** string (`duplicate_field_behavior = .@"error"`,
        // `std/json/static.zig:20-26`, applied in `dynamic.zig:149-156`;
        // `Scanner.zig:25-32`, Zig 0.16.0), so both of these are null there.
        // A key nobody reads is skipped here without either check
        // (`IgnoredAny`), so both decode. Only a hand-forged log holds them,
        // and the rows that show them are inert (`docs/parity.md`).
        for output in [
            r#"[{"question":"Q","answer":"A","x":{"k":1,"k":2}}]"#,
            r#"[{"question":"Q","answer":"A","x":"\ud800"}]"#,
        ] {
            assert_eq!(
                decode_answers(output),
                Some(vec![saved("Q", "A")]),
                "{output}"
            );
        }
    }

    #[test]
    fn anything_but_an_answer_document_is_none_and_never_a_panic() {
        // Upstream's rejects (`question_answer.zig:101-111@481104fd`) first,
        // then the rest of what a session file could hold instead.
        for output in [
            CANCEL_SENTINEL,
            "[]",
            r#"[{"question":"Q"}]"#,
            r#"[{"question":"Q","answer":1}]"#,
            r#"[{"question":"Q","answer":"A"}] trailing"#,
            ABORTED_OUTPUT,
            NOT_AVAILABLE_SENTINEL,
            "(ask_user_question: provide 1 to 20 questions)",
            "",
            "null",
            r#"{"question":"Q","answer":"A"}"#,
            r#"["Q"]"#,
            r#"[{"question":"Q","answer":"A"},7]"#,
            r#"[{"answer":"A"}]"#,
            r#"[{"question":null,"answer":"A"}]"#,
            r#"[{"question":"Q","answer":["A"]}]"#,
            r#"[{"question":"Q","answer":"A","answer":"B"}]"#,
            r#"[{"question":"Q","answer":"A"}"#,
            r#"[{"question":"\ud800","answer":"A"}]"#,
            // An element that is a JSON array, not an object, is refused
            // whatever it holds (`if (item != .object) return null`,
            // `question_answer.zig:63@481104fd`): a struct's positional form
            // is not a saved answer.
            r#"[["Q","A"]]"#,
            r#"[["Q","A","x"]]"#,
            r#"[{"question":"Q","answer":"A"},["Q","A"]]"#,
            r#"[["Q","A"],{"question":"Q","answer":"A"}]"#,
            // A key given twice, read or not.
            r#"[{"question":"Q","question":"R","answer":"A"}]"#,
            r#"[{"question":"Q","answer":"A","x":1,"x":2}]"#,
        ] {
            assert_eq!(decode_answers(output), None, "{output}");
        }
        // A key nobody reads is skipped whatever it holds, a hundred
        // thousand arrays deep included, without a frame per level: no stack
        // to overflow. Upstream's `std.json` keeps it (its stack grows on the
        // heap) and `object.get` ignores it, so it decodes there too.
        let deep = format!(
            "[{{\"question\":\"Q\",\"answer\":\"A\",\"x\":{}{}}}]",
            "[".repeat(100_000),
            "]".repeat(100_000)
        );
        assert_eq!(decode_answers(&deep), Some(vec![saved("Q", "A")]));
        // Deep where an answer is read is not an answer.
        let nested = format!("{}{}", "[".repeat(100_000), "]".repeat(100_000));
        assert_eq!(decode_answers(&nested), None);
    }

    #[test]
    fn the_cap_is_upstreams_twenty_in_the_description_the_schema_and_the_refusal() {
        // One bound, shared as upstream shares `max_questions`
        // (`question_answer.zig:7-9`, `builtins/tools.zig:119-122`, `:607`,
        // `ask_user_question.zig:19-22@481104fd`), and each place it is
        // spelled is tied to the constant here, so moving one moves them all.
        assert_eq!(MAX_QUESTIONS, 20);
        assert!(
            DESCRIPTION.starts_with("Ask the user 1-20 multiple-choice questions"),
            "{DESCRIPTION}"
        );
        assert!(DESCRIPTION.starts_with(&format!(
            "Ask the user {MIN_QUESTIONS}-{MAX_QUESTIONS} multiple-choice questions"
        )));
        assert!(QUESTIONS_DESCRIPTION
            .starts_with(&format!("{MIN_QUESTIONS} to {MAX_QUESTIONS} questions")));
        let advertised = Registry::builtin()
            .advertisement()
            .into_iter()
            .find(|schema| schema["name"] == "ask_user_question")
            .expect("advertised");
        let questions = &advertised["inputSchema"]["properties"]["questions"];
        assert_eq!(questions["minItems"], json!(1), "{questions}");
        assert_eq!(questions["maxItems"], json!(20), "{questions}");
        assert_eq!(
            advertised["description"].as_str(),
            Some(DESCRIPTION),
            "{advertised}"
        );
        assert_eq!(
            parse(&qs(MAX_QUESTIONS + 1)).unwrap_err(),
            format!("(ask_user_question: provide {MIN_QUESTIONS} to {MAX_QUESTIONS} questions)")
        );
    }

    #[test]
    fn a_full_batch_of_six_options_each_is_asked_once_and_one_more_question_is_refused_unasked() {
        // Upstream's own case (`ask_user_question.zig:432-531@481104fd`): every
        // question of a full batch reaches the requester in order with all six
        // of its options, question `i` is answered with option `i % 6`, and the
        // document comes back whole and in order. One more question is refused
        // before anyone is asked.
        fn batch(count: usize) -> Value {
            json!({"questions": (1..=count).map(|question| json!({
                "question": format!("Question {question}?"),
                "options": (1..=MAX_OPTIONS).map(|option| json!({
                    "label": format!("Q{question} option {option}"),
                    "description": format!("Consequence {question}.{option}"),
                })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>()})
        }
        struct ByIndex(Arc<Mutex<Vec<Vec<QuestionEntry>>>>);
        impl QuestionRequester for ByIndex {
            fn request(&self, entries: &[QuestionEntry]) -> Reply {
                self.0.lock().unwrap().push(entries.to_vec());
                Reply::Answered(
                    entries
                        .iter()
                        .enumerate()
                        .map(|(index, entry)| entry.options[index % MAX_OPTIONS].label.clone())
                        .collect(),
                )
            }
        }
        let asked = Arc::new(Mutex::new(Vec::new()));
        let dir = tempfile::tempdir().unwrap();
        let context = ToolContext::new(AccessScope::primary_only(dir.path()).unwrap())
            .with_questioner(Arc::new(ByIndex(Arc::clone(&asked))));

        let result = run(&batch(MAX_QUESTIONS), &context);
        assert!(result.ok, "{}", result.output);
        {
            let asked = asked.lock().unwrap();
            assert_eq!(asked.len(), 1, "one batch, asked once");
            assert_eq!(asked[0].len(), 20);
            for (index, entry) in asked[0].iter().enumerate() {
                assert_eq!(entry.question, format!("Question {}?", index + 1));
                assert_eq!(entry.options.len(), MAX_OPTIONS);
            }
        }
        let document: Vec<Value> = serde_json::from_str(&result.output).unwrap();
        assert_eq!(document.len(), 20);
        for (index, pair) in document.iter().enumerate() {
            assert_eq!(
                pair,
                &json!({
                    "question": format!("Question {}?", index + 1),
                    "answer": format!("Q{} option {}", index + 1, index % MAX_OPTIONS + 1),
                })
            );
        }

        let refused = run(&batch(MAX_QUESTIONS + 1), &context);
        assert!(!refused.ok);
        assert_eq!(
            refused.output,
            "(ask_user_question: provide 1 to 20 questions)"
        );
        assert_eq!(asked.lock().unwrap().len(), 1, "nobody was asked again");
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
        // Twenty questions (`max_questions`, `question_answer.zig:9@481104fd`):
        // five times the old four-question bound of 36 977.
        assert_eq!(WORST, 184_881);
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
                "(ask_user_question: provide 1 to 20 questions)",
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
    fn an_interrupted_batch_is_aborted_and_never_the_cancel_sentinel() {
        // Upstream's interrupted call (`session.zig:64`), and the claim its e2e
        // pins (`tui-decision-prompts.test.ts:2454`): the model is never told
        // the user declined a question they stopped the turn at.
        let (_dir, context) = scripted(Answering::Interrupt);
        let result = run(&two_questions(), &context);
        assert!(!result.ok);
        assert_eq!(result.output, ABORTED_OUTPUT);
        assert_eq!(result.output, "aborted by user");
        assert_eq!(result.outcome, Outcome::Cancelled);
        assert!(!result.output.contains(CANCEL_SENTINEL));
    }

    #[test]
    fn a_line_break_in_a_freeform_answer_reaches_the_model_as_one() {
        // e2e `tui-decision-prompts.test.ts:2785`: a pasted paragraph is
        // answered with its newline. Every other control is still named.
        let (_dir, context) = scripted(Answering::With(vec![
            "pasted with cmd-v\nand this stays on the next line\t\u{1b}".into(),
        ]));
        assert_eq!(
            run(&one_question(), &context).output,
            r#"[{"question":"Which depth?","answer":"pasted with cmd-v\nand this stays on the next line\\x09\\x1b"}]"#
        );
        assert_eq!(answer_safe("a\nb"), "a\nb");
        assert_eq!(answer_safe("a\rb\n"), "a\\x0db\n");
        assert_eq!(answer_safe("plain"), "plain");
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
        // `Registry::execute` clips past `max_output_bytes` (`src/tools/mod.rs:346`),
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
        // `execute` returns to the same `max_output_bytes` (`mod.rs:346`), so
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
