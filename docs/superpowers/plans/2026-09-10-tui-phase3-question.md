# P3-QUESTION Implementation Plan (`ask_user_question`)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship `ask_user_question` end to end — the model asks 1–4 multiple-choice questions, the TUI shows one at a time with ordinal choices plus a synthetic `Other` freeform slot, and the ordered question/answer JSON (or the cancellation sentinel) becomes the tool result the next model request carries.

**Architecture:** A real registry entry with its own permission kind — not an approval variant, not a permission grant. Data model, terminal-safe encoder, parser, answer encoder and requester trait live in `src/tools/question.rs`; the executor calls an *optional* `Arc<dyn QuestionRequester>` injected on `ToolContext` by a builder, so the tools layer never names the TUI. The TUI implements that trait over the **existing** `ControlChannel` — one receiver, one runtime, the park protocol `TuiPrompter` already uses — with a fail-closed request id that makes a stale answer discardable. The panel is a focused new module rendered in the band's panel slot.

**Tech Stack:** Rust 2021, `serde_json`, `tokio` current-thread runtime on the worker thread, `bridge::park_on` / `bridge::send_ui`.

**Spec:** `.prd/tui-phase3/ssot.md` row P3-QUESTION; `.prd/03-tui-port.md` ladder 19; `.prd/tui-phase3/loop.md` §Architecture rulings + §Verified upstream contract excerpts; `.prd/06-qa-harness.md`. Upstream pin `vercel-labs/fx@580a0c5d`: `src/tools/agent/ask_user_question.zig`, `src/core/shared/text_utils.zig`, `src/core/agent/question_answer.zig`, `src/core/agent/question_prompt.zig`, `src/builtins/tools.zig:1238-1268`.

## Global Constraints

- Every upstream citation is pinned at `580a0c5da9386317251968c09c1cee69e763487a`. Never claim latest-upstream parity.
- **Byte-exact sentinels.** Cancel: `(user cancelled the question)`. Unavailable: `(ask_user_question is only available in the interactive shell; ask the user freeform instead)`.
- Bounds: 1–4 questions; 2–6 options each; trimmed non-empty question text and labels; ASCII-case-insensitive unique labels **after** encoding. Availability is checked **before** parsing (`ask_user_question.zig:100-104`).
- **Canonical text is never truncated to fit a screen.** Questions, labels, descriptions and answers are terminal-safe *encoded* (lossless, literal `\xNN` / `\u{NNNN}`), then **size-validated with a refusal**. Window and wrap are the panel's, and the panel never edits the canonical string.
- The tool mints **no** authority. Do not reuse `ApprovalRequest`, `ApprovalAnswer`, `Panel`, `ApprovalPrompter`.
- Preserve UI-thread terminal ownership and worker-runtime separation. **No second receiver on the control channel, no second runtime.**
- All eight existing tool schemas stay byte-identical in `advertisement()` output.
- Out of scope: `permission_request_id`, approval readiness, amendment drafts, commit self-check, multi-select.
- No credentials, no live network in any fixture.
- Writer does **not** stage, commit, push or merge. Evidence to `$XFX_EVIDENCE` outside the worktree.
- Gate = `.prd/tui-phase2/loop.md` §Gate contract verbatim, plus both release builds, `scripts/smoke.sh`, `scripts/smoke-tui.sh`. Counts are observations, never expectations.

## Verified source facts this plan is built on

| Fact | Where |
|---|---|
| `ToolSpec::input_schema(&self) -> InputSchema` already exists and returns **by value**. Do not change it. | `src/tools/spec.rs:490` |
| `ToolContext::with_limits(scope, limits)` is a **constructor**, not a builder. A limited context is constructed first, then `.with_permissions(..).with_questioner(..)`. No `with_limits_for_test` is added. | `src/tools/spec.rs:371` |
| The shell test fixture is `fn shell(rows: u16, cols: u16) -> Fixture`; resizing is `Shell::resize(rows, cols) -> Resize` — **rows first**. There is no `Fixture::new()` and no `Fixture::with_geometry`. | `src/tui/shell.rs:3108`, `:2858` |
| No test hardcodes the advertised-tool count; `tests/tool_loop.rs:188,194`, `tests/llmux.rs:1636`, `tests/parity.rs:280` all derive it from `ADVERTISED_TOOLS`. | as cited |
| **`tests/parity.rs` does break.** `the_tools_prose_splits_them_the_way_the_permission_system_does` (`:349-380`) asserts the prose's read-only/mutating/command groups union to *every* advertised tool; `the_tools_prose_declares_the_number_of_tools_that_exist` (`:330`) reads the literal `8` from `docs/parity.md:96`. A fourth kind needs a fourth prose group and a fourth `prose_group` call. | as cited |
| xfx has **no** faithful terminal-safe encoder. `output::safe_one_line` (`src/output.rs:1124`) is lossy — controls become spaces, the text is trimmed, and a `…` is appended past the cap; `tui::bridge::inert` (`:634`) also maps to spaces; `workspace/context.rs:617` writes `&#x..;` for another surface. A small focused encoder is written in Task 1. | as cited |
| Upstream's encoder: `byte < 0x20 \|\| == 0x7f` → `\xNN` lowercase width 2; other ASCII literal; non-printing codepoint → `\u{NNNN}` lowercase min width 4; else literal. Non-printing = `U+0080..=009F`, `U+200B..=200F`, `U+2028..=202E`, `U+2060..=206F`, `U+FEFF`. | `text_utils.zig:533-599` |
| The encoder is **idempotent**: `\` (0x5c) is printable ASCII and returned literally, so re-encoding `\x0a` leaves it alone. | `text_utils.zig:538-540` |
| Upstream's expected output is literal: question `Q\x0a\x1b[31m?`, labels `Alpha\x0aFake` and `\x1b[31mRed\x1b[0m`, description `Desc\x09Gap`. | `ask_user_question.zig:283-289` |
| `encodeJson` encodes the **encoded** question text, so display text and result text are one string. | `question_answer.zig:12-33` |
| `ToolLimits::default().max_output_bytes` is `256 * 1024`. | `src/tools/spec.rs:280` |

## File Structure

| File | Responsibility |
|---|---|
| `src/tools/question.rs` (create) | `terminal_safe`, bounds, sentinels, `QuestionEntry`/`QuestionOption`/`AskUserQuestionInput`, parser, answer encoder, `QuestionRequester`, executor, `ASK_USER_QUESTION`. |
| `src/tools/spec.rs` (modify) | `PermissionKind::Interaction`; `PropertyKind::Array` + `ArraySpec`; `ToolInput::AskUserQuestion`; `ToolContext` questioner field/builder/accessor. |
| `src/tools/mod.rs` (modify) | `pub mod question`; `ADVERTISED_TOOLS` + `BUILTIN_TOOLS` gain the tool, last. |
| `src/interactive.rs` (modify) | `open_conversation` gains a questioner parameter; the line shell (`:662`) passes `None`. |
| `src/app.rs` (modify) | Comment only: it builds a `ToolContext` directly (`:434-436`) and never calls `open_conversation`, so a non-interactive run has no questioner and answers with the availability sentinel. |
| `src/tui/question.rs` (create) | `QuestionId`/`QuestionRequest`, `QuestionPanel`, `TuiQuestioner`. |
| `src/tui/bridge.rs` (modify) | `UiEvent::Question` + `made_inert` arm; `TurnControl::QuestionAnswer`/`QuestionCancelled`. |
| `src/tui/approval.rs` (modify) | `ControlChannel::{answered, give_back, rearm}` become `pub(crate)`. |
| `src/tui/shell.rs` (modify) | `ask` field, `Slot::Ask`, `modal()`, `ask_question`/`answer`, too-small refusal. |
| `src/tui/worker.rs` (modify) | Build `TuiQuestioner`; pass to `open_conversation`; turn loop discards stale question control. |
| `tests/parity.rs` (modify) | Fourth permission group in the prose split. |
| `scripts/smoke-tui.sh` (modify) | `ask_then_finish`, `tool_result_text`, `scenario_23`, `scenario_23b`, registry + list. |
| `docs/parity.md`, `.prd/03-tui-port.md`, `.prd/06-qa-harness.md` (modify) | Implemented truth and the documented narrowings. |

---

### Task 1: Terminal-safe encoder, data model, parser, answer encoder

**Files:** Create `src/tools/question.rs`; modify `src/tools/mod.rs:18-21` (`pub mod question;`); tests inline.

**Interfaces produced:** `terminal_safe(&str) -> Cow<'_, str>`; `QuestionOption { label: String, description: Option<String> }`; `QuestionEntry { question: String, options: Vec<QuestionOption> }`; `AskUserQuestionInput { entries: Vec<QuestionEntry>, raw: Value }` with `AskUserQuestionInput::raw(Value)`; `parse(&Value) -> Result<AskUserQuestionInput, String>`; `encode_answers(&[QuestionEntry], &[String]) -> Result<String, String>`; the consts below.

- [ ] **Step 1: Write the failing tests** (table-driven; the escaper's cases are bytes, not prose). Three `#[cfg(test)]` builders first, so no case hand-rolls a batch:

```rust
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
    QuestionEntry { question: question.to_string(), options: Vec::new() }
}
```

```rust
#[test]
fn the_encoder_is_upstreams_lossless_one() {
    // `text_utils.zig:533-599`; expectations are upstream's own
    // (`ask_user_question.zig:283-289`).
    for (raw, expected) in [
        ("Q\n\u{1b}[31m?", r"Q\x0a\x1b[31m?"),
        ("Alpha\nFake", r"Alpha\x0aFake"),
        ("Desc\tGap", r"Desc\x09Gap"),
        ("\u{7f}", r"\x7f"),
        ("\u{9b}", r"\u{009b}"),          // C1: a CSI on a decoding terminal
        ("a\u{200b}b", r"a\u{200b}b"),
        ("\u{feff}", r"\u{feff}"),
        ("네—ok", "네—ok"),               // printable non-ASCII stays literal
        (r"C:\path", r"C:\path"),         // a backslash is printable ASCII
    ] {
        assert_eq!(terminal_safe(raw), expected, "encoding {raw:?}");
    }
}

#[test]
fn the_encoder_is_idempotent_and_borrows_when_nothing_changes() {
    assert!(matches!(terminal_safe("plain text"), Cow::Borrowed(_)));
    let once = terminal_safe("a\nb").into_owned();
    assert_eq!(terminal_safe(&once), once, "re-encoding an encoded string changes nothing");
}

#[test]
fn parsing_refuses_out_of_bounds_batches_with_the_upstream_bodies() {
    for (args, expected) in [
        (qs(0), "(ask_user_question: provide 1 to 4 questions)"),
        (qs(5), "(ask_user_question: provide 1 to 4 questions)"),
        (os(1), "(ask_user_question: provide 2 to 6 options per question)"),
        (os(7), "(ask_user_question: provide 2 to 6 options per question)"),
        (json!({"questions": [{"question": "   ", "options": [{"label": "a"}, {"label": "b"}]}]}),
         "(ask_user_question: question text must not be empty)"),
        // A literal space, not a tab: the encoder turns a tab into `\x09`, so
        // `SHIP\x09IT` and `Ship it` are genuinely different labels and would
        // not exercise the duplicate rule at all.
        (json!({"questions": [{"question": "q",
                "options": [{"label": "Ship it"}, {"label": "SHIP IT"}]}]}),
         "(ask_user_question: option labels must be unique within a question)"),
    ] { assert_eq!(parse(&args).unwrap_err(), expected, "for {args}"); }
    assert!(parse(&qs(1)).is_ok() && parse(&qs(4)).is_ok()
         && parse(&os(2)).is_ok() && parse(&os(6)).is_ok());
    // The other side of the same rule: encoding is what the comparison sees.
    assert!(parse(&json!({"questions": [{"question": "q",
        "options": [{"label": "Ship it"}, {"label": "SHIP\tIT"}]}]})).is_ok(),
        "a tab makes the label distinct once it is `\\x09`");
}

#[test]
fn overlong_text_is_refused_rather_than_truncated() {
    // The canonical question is what the model reads back in the result; a plan
    // that clipped it would change the question's meaning silently.
    let long = "x".repeat(MAX_QUESTION_ENCODED_BYTES + 1);
    assert_eq!(parse(&json!({"questions": [{"question": long,
        "options": [{"label": "a"}, {"label": "b"}]}]})).unwrap_err(),
        "(ask_user_question: question text is longer than 512 encoded bytes)");
    // Interior newlines between two ordinary characters. A label of newlines
    // alone trims to empty and is refused for emptiness instead, which would
    // not exercise this bound at all: 1 + 32*4 + 1 = 130 encoded bytes.
    let escapes = format!("a{}b", "\n".repeat(32));
    assert_eq!(parse(&json!({"questions": [{"question": "q",
        "options": [{"label": escapes}, {"label": "b"}]}]})).unwrap_err(),
        "(ask_user_question: option label is longer than 128 encoded bytes)");
}

#[test]
fn the_closed_schema_is_enforced_here_because_only_this_side_can_enforce_it() {
    // `additionalProperties: false` is advertised (`spec.rs:98-101`); a provider
    // that does not enforce it must not make xfx accept a field it never offered.
    assert_eq!(parse(&json!({"questions": [{"question": "q", "why": "extra",
        "options": [{"label": "a"}, {"label": "b"}]}]})).unwrap_err(),
        "(ask_user_question: a question has a field this tool does not accept)");
    assert_eq!(parse(&json!({"questions": [], "permission_request_id": "x"})).unwrap_err(),
        "(ask_user_question: an argument this tool does not accept was sent)");
}

#[test]
fn a_non_string_description_is_discarded_rather_than_refused() {
    let parsed = parse(&json!({"questions": [{"question": "q",
        "options": [{"label": "a", "description": 7}, {"label": "b"}]}]})).expect("parsed");
    assert_eq!(parsed.entries[0].options[0].description, None);
}

#[test]
fn answers_encode_in_question_order_with_json_escaping() {
    let entries = vec![entry("Which depth?"), entry("Ship it?")];
    assert_eq!(encode_answers(&entries, &["Thorough".into(), "Yes \"now\"".into()]).unwrap(),
        r#"[{"question":"Which depth?","answer":"Thorough"},{"question":"Ship it?","answer":"Yes \"now\""}]"#);
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
        + (MAX_QUESTIONS - 1);                              // the commas between them
    assert_eq!(MAX_ENCODED_RESULT_BYTES, WORST);
    assert_eq!(WORST, 36_977);
    assert!(WORST <= ToolLimits::default().max_output_bytes);
}
```

- [ ] **Step 2: Run and watch it fail** — `cargo test --locked --lib tools::question`. Expected: FAIL, unresolved module.

- [ ] **Step 3: Write the module**

```rust
//! `ask_user_question`. Bounds, sentinels, the terminal-safe encoding and the
//! answer document are upstream's (`vercel-labs/fx@580a0c5d`
//! `src/tools/agent/ask_user_question.zig`, `src/core/shared/text_utils.zig`,
//! `src/core/agent/question_answer.zig`).
use std::borrow::Cow;
use std::fmt::Write as _;
use serde_json::{Map, Value};

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
    if !raw.chars().any(needs_escape) { return Cow::Borrowed(raw); }
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
pub struct QuestionOption { pub label: String, pub description: Option<String> }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionEntry { pub question: String, pub options: Vec<QuestionOption> }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AskUserQuestionInput { pub entries: Vec<QuestionEntry>, pub raw: Value }

impl AskUserQuestionInput {
    /// Undecoded arguments. The decoder has no context and cannot see whether a
    /// requester exists, and availability outranks a parse error, so the raw
    /// value travels to `execute` and is parsed there.
    pub fn raw(value: Value) -> Self { Self { entries: Vec::new(), raw: value } }
}

fn bad(detail: &str) -> String { format!("(ask_user_question: {detail})") }

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
    if object.keys().any(|key| !allowed.contains(&key.as_str())) { return Err(bad(what)); }
    Ok(())
}

pub fn parse(value: &Value) -> Result<AskUserQuestionInput, String> {
    let args = value.as_object().ok_or_else(|| bad("invalid arguments; provide {questions}"))?;
    only(args, &["questions"], "an argument this tool does not accept was sent")?;
    let questions = args.get("questions").ok_or_else(|| bad("missing required array \"questions\""))?
        .as_array().ok_or_else(|| bad("\"questions\" must be an array"))?;
    if questions.len() < MIN_QUESTIONS || questions.len() > MAX_QUESTIONS {
        return Err(bad("provide 1 to 4 questions"));
    }
    let mut entries = Vec::with_capacity(questions.len());
    for item in questions {
        let item = item.as_object()
            .ok_or_else(|| bad("each question must be an object with a \"question\" and \"options\""))?;
        only(item, &["question", "options"], "a question has a field this tool does not accept")?;
        let text = item.get("question").ok_or_else(|| bad("each question requires a \"question\" string"))?
            .as_str().ok_or_else(|| bad("question \"question\" must be a string"))?;
        if text.trim().is_empty() { return Err(bad("question text must not be empty")); }
        let question = bounded(text.trim(), MAX_QUESTION_ENCODED_BYTES, "question text")?;
        let options = item.get("options").ok_or_else(|| bad("each question requires an \"options\" array"))?
            .as_array().ok_or_else(|| bad("\"options\" must be an array"))?;
        if options.len() < MIN_OPTIONS || options.len() > MAX_OPTIONS {
            return Err(bad("provide 2 to 6 options per question"));
        }
        let mut built: Vec<QuestionOption> = Vec::with_capacity(options.len());
        for option in options {
            let option = option.as_object()
                .ok_or_else(|| bad("each option must be an object with a \"label\""))?;
            only(option, &["label", "description"], "an option has a field this tool does not accept")?;
            let raw_label = option.get("label").ok_or_else(|| bad("each option requires a \"label\" string"))?
                .as_str().ok_or_else(|| bad("option \"label\" must be a string"))?;
            if raw_label.trim().is_empty() { return Err(bad("option labels must not be empty")); }
            let label = bounded(raw_label.trim(), MAX_LABEL_ENCODED_BYTES, "option label")?;
            // After encoding, as upstream compares them, and ASCII-case
            // insensitively: two labels a user cannot tell apart are one choice.
            if built.iter().any(|held| held.label.eq_ignore_ascii_case(&label)) {
                return Err(bad("option labels must be unique within a question"));
            }
            // A non-string description is discarded, not refused
            // (`ask_user_question.zig:176-182`).
            let description = match option.get("description").and_then(Value::as_str) {
                Some(text) if !text.trim().is_empty() =>
                    Some(bounded(text.trim(), MAX_DESCRIPTION_ENCODED_BYTES, "option description")?),
                _ => None,
            };
            built.push(QuestionOption { label, description });
        }
        entries.push(QuestionEntry { question, options: built });
    }
    Ok(AskUserQuestionInput { entries, raw: value.clone() })
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
    if entries.len() != answers.len() { return Err(bad("the answers did not match the questions")); }
    let document: Vec<AnsweredQuestion<'_>> = entries.iter().zip(answers)
        .map(|(entry, answer)| AnsweredQuestion {
            question: &entry.question,
            answer: answer.as_str(),
        })
        .collect();
    serde_json::to_string(&document).map_err(|err| bad(&err.to_string()))
}
```

- [ ] **Step 4: Run** — `cargo test --locked --lib tools::question`. Expected: PASS, 8 tests. Report; do not commit.

---

### Task 2: Bounded arrays of closed objects in the static schema

**Files:** Modify `src/tools/spec.rs:54-116`; add `array: None` to **every** `Property` literal in the crate — the compiler enumerates them, and the known ones are `src/tools/read.rs` (15), `src/tools/mutate.rs` (6), `src/tools/terminal.rs` (3) and `src/tools/spec.rs`'s own test literals at `:642` and `:648` (2), 26 in all. Tests inline in `spec.rs` and `mod.rs`.

**Interfaces produced:** `PropertyKind::Array` (label `"array"`); `pub struct ArraySpec { pub items: &'static InputSchema, pub min_items: usize, pub max_items: usize }` (`Debug, Clone, Copy`); `Property.array: Option<ArraySpec>`; `InputSchema::to_json` renders `items`/`minItems`/`maxItems` for array properties, unchanged for scalars. **`ToolSpec::input_schema` already exists (`spec.rs:490`) and returns by value — leave its signature alone.**

- [ ] **Step 1: Pin the eight existing schemas first.** Add to `src/tools/mod.rs` tests, run once with an empty `PINNED` to print the actual serialisations, paste them in, confirm it passes **at the current tip**.

```rust
#[test]
fn the_eight_original_schemas_are_byte_identical() {
    // Captured at the tip before the array extension. A byte that moves here is
    // a change to what every model already sees.
    const PINNED: &[(&str, &str)] = &[/* (name, serde_json::to_string(advertisement)) per tool */];
    let registry = Registry::builtin();
    assert_eq!(PINNED.len(), 8);
    for (name, expected) in PINNED {
        let spec = registry.spec(name).expect("an advertised tool");
        assert_eq!(&serde_json::to_string(&spec.advertisement()).unwrap(), expected,
                   "`{name}`'s advertised schema moved");
    }
}
```

- [ ] **Step 2: Write the failing tests for the new shape**

```rust
#[test]
fn a_bounded_array_of_closed_objects_renders_items_and_bounds() {
    static ITEM: InputSchema = InputSchema {
        properties: &[Property { name: "label", kind: PropertyKind::String,
                                 description: "d", allowed: &[], array: None }],
        required: &["label"] };
    static OUTER: InputSchema = InputSchema {
        properties: &[Property { name: "questions", kind: PropertyKind::Array,
                                 description: "d", allowed: &[],
                                 array: Some(ArraySpec { items: &ITEM, min_items: 1, max_items: 4 }) }],
        required: &["questions"] };
    let field = &OUTER.to_json()["properties"]["questions"];
    assert_eq!(field["type"], "array");
    assert_eq!(field["minItems"], 1);
    assert_eq!(field["maxItems"], 4);
    assert_eq!(field["items"]["type"], "object");
    assert_eq!(field["items"]["additionalProperties"], false);
    assert_eq!(field["items"]["required"][0], "label");
}

#[test]
fn no_advertised_schema_nests_deeper_than_two_array_edges() {
    // `depth` counts **object levels**, so the question tool's own schema --
    // outer object, question object, option object -- is 3, which is two array
    // edges. The bound is on the edges; naming it 2 while counting objects
    // would fail on the very schema this task exists to allow.
    fn depth(schema: InputSchema) -> usize {
        1 + schema.properties.iter().filter_map(|property| property.array)
            .map(|array| depth(*array.items)).max().unwrap_or(0)
    }
    for spec in Registry::builtin().specs() {
        assert!(depth(spec.input_schema()) <= 3, "`{}` nests too deep", spec.name());
    }
    // And the eight scalar-only tools stay flat, so the bound is not vacuous.
    for name in ["list_files", "read_file", "write_file", "terminal"] {
        assert_eq!(depth(Registry::builtin().spec(name).unwrap().input_schema()), 1);
    }
}
```

- [ ] **Step 3: Run and watch it fail** — `cargo test --locked --lib tools::spec`. Expected: FAIL, `no variant named Array`.

- [ ] **Step 4: Implement.** Add the variant, the struct, the field, and in `to_json` after the `enum` branch:

```rust
if let Some(spec) = property.array {
    rendered.insert("items".to_string(), spec.items.to_json());
    rendered.insert("minItems".to_string(), Value::from(spec.min_items));
    rendered.insert("maxItems".to_string(), Value::from(spec.max_items));
}
```

- [ ] **Step 5: Run** — `cargo test --locked --lib tools:: && cargo clippy --locked --all-targets -- -D warnings`. Expected: PASS, byte-identity guard included. Report; do not commit.

---

### Task 3: The interaction kind, the requester seam, the executor

**Files:** Modify `src/tools/spec.rs:36-51`, `:124-134`, `:336-441`; `src/tools/question.rs`; `src/tools/mod.rs:38-60`; `src/interactive.rs:498-524`; `src/app.rs:434-436`; `docs/parity.md:96-99,138`; `tests/parity.rs:349-380`.

**Interfaces produced:** `PermissionKind::Interaction` with `requires_authority()` = `matches!(self, MutateFile | RunCommand)`; `pub trait QuestionRequester: Send + Sync { fn request(&self, entries: &[QuestionEntry]) -> Option<Vec<String>>; }`; `ToolContext::with_questioner(self, Arc<dyn QuestionRequester>) -> Self` and `questioner(&self) -> Option<&Arc<dyn QuestionRequester>>` plus a `Debug` field `has_questioner`; `ToolInput::AskUserQuestion(AskUserQuestionInput)`; `pub static ASK_USER_QUESTION: ToolSpec`; `open_conversation(store, config, model, permissions, questioner: Option<Arc<dyn QuestionRequester>>, cancel)`.

**Test helpers, defined once and reused.** `Scripted(Answering)` implements `QuestionRequester`, returning `None` (`Answering::Cancel`), a fixed `Vec<String>` (`Answering::With`), or recording its entries into an `Arc<Mutex<Vec<QuestionEntry>>>` (`Answering::Record`). `fn scripted(a: Answering) -> (TempDir, ToolContext)` builds `AccessScope::primary_only(dir.path())` then `ToolContext::new(scope).with_questioner(Arc::new(Scripted(a)))`. `fn tight(a: Answering, bytes: usize) -> (TempDir, ToolContext)` is the same but starts from `ToolContext::with_limits(scope, ToolLimits { max_output_bytes: bytes, ..ToolLimits::default() })` — **`with_limits` is a constructor, so it comes first**. `fn run(args, ctx) -> ToolResult` calls `Registry::builtin().execute(&call("ask_user_question", args), ctx).unwrap()`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn without_a_requester_the_tool_says_so_before_it_parses() {
    let dir = tempfile::tempdir().unwrap();
    let context = ToolContext::new(AccessScope::primary_only(dir.path()).unwrap());
    let result = run(&json!({"questions": []}), &context);   // a malformed batch, no questioner
    assert!(result.ok);
    assert_eq!(result.output, NOT_AVAILABLE_SENTINEL, "availability outranks the parse error");
}

#[test]
fn the_schema_is_validated_execute_side_because_the_decoder_is_total() {
    let (_dir, context) = scripted(Answering::Cancel);
    for (args, expected) in [
        (json!({"questions": []}), "(ask_user_question: provide 1 to 4 questions)"),
        (json!({"questions": [{"question": "q", "why": "x",
                "options": [{"label": "a"}, {"label": "b"}]}]}),
         "(ask_user_question: a question has a field this tool does not accept)"),
        (json!({"nope": 1}), "(ask_user_question: an argument this tool does not accept was sent)"),
    ] {
        let result = run(&args, &context);
        assert!(!result.ok, "a batch the model got wrong is a refusal it can correct");
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
    assert_eq!(result.output,
        r#"[{"question":"Which depth?","answer":"Thorough"},{"question":"Ship it?","answer":"later"}]"#);
}

#[test]
fn the_requester_is_handed_the_encoded_entries_it_will_display() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let (_dir, context) = scripted(Answering::Record(Arc::clone(&seen)));
    let _ = run(&json!({"questions": [{"question": "a\u{1b}[31mb",
        "options": [{"label": "x"}, {"label": "y"}]}]}), &context);
    assert_eq!(seen.lock().unwrap()[0].question, r"a\x1b[31mb");
}

#[test]
fn an_answer_from_the_requester_is_encoded_and_bounded_before_it_leaves() {
    // The requester is a trait. A non-TUI implementation returning a control
    // sequence or a megabyte must not reach the model or the registry's clip.
    let (_dir, context) = scripted(Answering::With(vec!["ok\u{1b}[2J".into()]));
    assert_eq!(run(&one_question(), &context).output,
        r#"[{"question":"Which depth?","answer":"ok\\x1b[2J"}]"#);
    let (_dir, context) = scripted(Answering::With(vec!["x".repeat(MAX_FREEFORM_ENCODED_BYTES + 1)]));
    let refused = run(&one_question(), &context);
    assert!(!refused.ok);
    assert_eq!(refused.output, "(ask_user_question: answer is longer than 4096 encoded bytes)");
}

#[test]
fn a_result_the_callers_limit_cannot_carry_is_refused_before_the_registry_clips_it() {
    // `Registry::execute` clips past `max_output_bytes` (`src/tools/mod.rs:145`),
    // and clipped JSON is not JSON.
    let (_dir, context) = tight(Answering::With(vec!["a moderate answer".into()]), 64);
    let result = run(&one_question(), &context);
    assert!(!result.ok);
    assert_eq!(result.output, "(ask_user_question: the answers were too long to return)");
    assert!(result.output.len() <= 64, "the refusal itself fits the limit");
}

#[test]
fn the_question_tool_mints_no_authority_and_every_kind_is_named() {
    assert_eq!(Registry::builtin().spec("ask_user_question").expect("advertised").permission(),
               PermissionKind::Interaction);
    for spec in Registry::builtin().specs() {
        // No wildcard: a new kind must be decided here rather than defaulting.
        match spec.permission() {
            PermissionKind::ReadOnly | PermissionKind::Interaction =>
                assert!(!spec.permission().requires_authority()),
            PermissionKind::MutateFile | PermissionKind::RunCommand =>
                assert!(spec.permission().requires_authority()),
        }
    }
}
```

- [ ] **Step 2: Run and watch them fail** — `cargo test --locked --lib tools::question`. Expected: FAIL, `no variant Interaction`.

- [ ] **Step 3: Implement**

```rust
fn decode(value: &Value) -> Result<ToolInput, String> {
    // Total on purpose: availability outranks a parse error and only `execute`
    // can see whether a requester exists. `parse` is the validator, and
    // `the_schema_is_validated_execute_side_...` proves it runs on this side.
    Ok(ToolInput::AskUserQuestion(AskUserQuestionInput::raw(value.clone())))
}

fn validate(_input: &ToolInput) -> Result<(), String> { Ok(()) }

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
        Ok(document) if document.len() <= context.limits().max_output_bytes =>
            ToolResult::success(document, format!("answered {} question(s)", parsed.entries.len())),
        Ok(_) => ToolResult::failure(bad("the answers were too long to return")),
        Err(problem) => ToolResult::failure(problem),
    }
}
```

`INPUT_SCHEMA`: one required `questions` array, `min_items: 1`, `max_items: 4`; `items` = closed object `{question: String, options: Array}` required `["question", "options"]`; `options` = `min_items: 2`, `max_items: 6` over closed `{label: String, description: String}` required `["label"]`. Description text is upstream's minus the `permission_request_id` sentences. **`permission_request_id` is deliberately not advertised** — that route is P3-APPROVAL.

Add `"ask_user_question"` last in `ADVERTISED_TOOLS` and `question::ASK_USER_QUESTION` last in `BUILTIN_TOOLS` (`tools.zig:1375`).

**`docs/parity.md` and `tests/parity.rs` change together** — verified above:
- `docs/parity.md:96-99`: `8` → `9`, and add `and 1 interaction (\`ask_user_question\`).` to the group list.
- `docs/parity.md:138`: `deferred` → `implemented`, naming the port pin and **five** differences, each as a verified statement about what this port does rather than a claim about what upstream refuses:
  1. `permission_request_id` is not advertised; the approval-screen route is P3-APPROVAL.
  2. The line-oriented shell returns the availability sentinel, because fx's interactive shell is xfx's *TUI*, not xfx's line shell.
  3. Encoded-size bounds on questions, labels, descriptions and answers, refused rather than truncated, with the `max_output_bytes` reason.
  4. **Unicode in `Other`.** xfx's freeform takes whatever the shell's decoder delivers as `Input::Text(char)`, so a multi-byte character is one insertion. Upstream's prompt action is a single byte — `question_prompt.zig:322` is `.insert_ascii => |byte| self.insertFreeformByte(alloc, byte, max_len)`. **This port did not trace upstream's paste or IME route into that prompt**, so the row records xfx's behaviour and the one upstream line it verified, and does *not* say upstream refuses Unicode.
  5. **Ordinals outside the visible window are refused.** Upstream's `selectOrdinal` bounds only against the option count (`question_prompt.zig:389`, `if (index >= entry.options.items.len) return .none;`) because its prompt has no window. xfx windows the options on a short screen, so it additionally refuses a number the user cannot see — an xfx safety choice, named as one.
- `tests/parity.rs`: add `let (interaction_count, interaction) = prose_group("interaction");`, its count and `tools_of_kind(PermissionKind::Interaction)` assertions, and include it in the union loop and the final sum.

Thread the questioner: `ToolContext` gains `questioner: Option<Arc<dyn QuestionRequester>>` (`Clone` still derives), the builder, the accessor, the `Debug` field. `open_conversation` gains the parameter and calls `.with_questioner(..)` when `Some`.

**Every call site, enumerated** (the compiler will confirm; these are the ones that exist today):
- `src/interactive.rs:662` — the line shell. Passes `None`, with a comment naming the availability sentinel and why (fx's interactive shell is xfx's TUI).
- `src/tui/worker.rs:593` — the real TUI session. Passes `Some(questioner)` from Task 5.
- `src/tui/worker.rs:2376`, `:2585`, `:2820` — worker **tests**. Pass `None`; they do not exercise the question path.
- `src/app.rs:434-436` builds a `ToolContext` **directly and does not call `open_conversation` at all**, so it gains no argument — only a comment saying a non-interactive run has no questioner and therefore answers with the availability sentinel.

- [ ] **Step 4: Run** — `cargo test --locked --lib tools:: && ./scripts/check-no-stubs.sh && cargo test --locked --all-targets`. Expected: PASS. Report; do not commit.

---

### Task 4: The question panel **and** its band wiring

**One task, not two.** `src/tui/question.rs` is a **private** module (`mod question;`), so its `pub(crate)` items are dead code until the shell installs a panel. Splitting the panel from the wiring would leave the first half failing `clippy --all-targets -- -D warnings`, and the only ways to pass it separately — `#[allow(dead_code)]` or a `pub` the crate does not want — are scaffolding this plan will not carry. Both halves therefore land together and the clippy gate runs once, at the end.

**Files:** Create `src/tui/question.rs`; modify `src/tui/mod.rs` (`mod question;`), `src/tui/bridge.rs:112-172`, `:237-262`, `:349-367`, `src/tui/shell.rs:433`, `:544-549`, `:640-655`, `:790-795`, `:1040-1060`, `:1220`, `:1792-1810`, `:2777`; tests inline in both modules.

**Interfaces:**
- Consumes: `super::editor::Editor` (`new`, `apply(Action, cols)`, `text()`, `rows(cols)`, `point(cols)`), `super::layout::fits_panel`, `super::frame::clip`, `crate::tools::question::{QuestionEntry, QuestionOption, terminal_safe, FREEFORM_LABEL, MAX_FREEFORM_ENCODED_BYTES}`.
- Produces: `QuestionId(pub u64)` (`Clone, Copy, PartialEq, Eq, Debug`); `QuestionRequest { pub id: QuestionId, pub entries: Vec<QuestionEntry> }` (`Clone, Debug, PartialEq, Eq`); `pub(crate) enum Act { Text(char), Up, Down, Tab, Submit, Escape, Cancel, Backspace, Left, Right, Home, End }` (`Clone, Copy, Debug`); `pub(crate) enum Answered { Nothing, Redraw, Submitted(Vec<String>), Cancelled }` (`Debug`); `QuestionPanel::{new(QuestionRequest), id(), rows(cols, terminal_rows) -> Vec<String>, height(cols, terminal_rows) -> u16, presents_choices(cols, terminal_rows) -> bool, apply(Act, cols, terminal_rows) -> Answered, caret(cols) -> Option<(u16, u16)>}` plus `#[cfg(test)] fn draft(&self) -> Option<&str>` (the current entry's freeform text, `None` when `Other` is not selected); `UiEvent::Question(QuestionRequest)`; `TurnControl::QuestionAnswer { id: QuestionId, answers: Vec<String> }`; `TurnControl::QuestionCancelled { id: QuestionId }`; `Shell::ask_question(QuestionRequest)`; `Slot::Ask(&'a QuestionPanel)`.

**Shape rules** — stated once, read by `height`, `rows` and `apply`:
- Rows: 1 title (`Which depth? (1 of 2)`), up to `visible` option rows, then 1 draft row while `Other` is selected.
- `visible = min(options.len(), max(2, terminal_rows / 3))`, clamped so the whole panel satisfies `layout::fits_panel`.
- `top` is adjusted in `apply` so `selected` always lies in `top..top + visible`. **Row text is windowed and clipped for the screen only** (`super::frame::clip`, trailing `…`); the canonical `QuestionEntry` strings are never rewritten.
- Ordinals are absolute (`1.`..`7.`); `Act::Text(digit)` returns `Answered::Nothing` when the index falls outside the window.
- The freeform slot is appended once at construction as `QuestionOption { label: FREEFORM_LABEL.into(), description: None }` and identified by **index** (`options.len() - 1`), never by label comparison.
- While `Other` is selected, `Act::Text(c)` inserts into that entry's draft `Editor` (digits included — upstream's freeform editing) and is refused silently when `terminal_safe(draft).len()` would exceed `MAX_FREEFORM_ENCODED_BYTES`; the executor re-checks the same bound, because the panel is not the only possible requester. `caret(cols)` returns the draft caret only then.
- `Act::Submit` mirrors `question_prompt.zig:603-634`: record the answer (label, or the draft's text for the freeform slot), advance to the next **unanswered** entry cycling from the current one; when none remain, return `Answered::Submitted(answers)` in **entry order**.
- `Act::Escape` and `Act::Cancel` both return `Answered::Cancelled`; the shell tells them apart, the panel does not.

- [ ] **Step 1: Write the failing tests.** `two()` = two entries, 2 options each; `six()` = one entry, 6 options; and one shared driver, so every submitting case reads the same way:

```rust
fn submitted(panel: &mut QuestionPanel, acts: &[Act], cols: u16, rows: u16) -> Vec<String> {
    let mut last = Answered::Nothing;
    for act in acts { last = panel.apply(*act, cols, rows); }
    match last {
        Answered::Submitted(answers) => answers,
        other => panic!("expected a submitted batch, got {other:?}"),
    }
}
```

```rust
#[test]
fn one_question_is_visible_at_a_time_with_ordinals_and_a_freeform_slot() {
    let text = two().rows(60, 40).join("\n");
    for expected in ["Which depth?", "1. Thorough", "2. Quick", "3. Other",
                     "reads every file", "1 of 2"] {
        assert!(text.contains(expected), "the panel does not show {expected:?}:\n{text}");
    }
    assert!(!text.contains("Ship it?"), "the second question is not shown yet");
}

#[test]
fn a_model_ordinal_submits_at_once() {
    // A fresh panel: no freeform slot is selected, so a digit is a choice.
    let mut panel = two();
    assert!(matches!(panel.apply(Act::Text('1'), 60, 40), Answered::Redraw));
    assert!(panel.rows(60, 40).join("\n").contains("2 of 2"), "the batch advanced");
    assert_eq!(submitted(&mut panel, &[Act::Text('2')], 60, 40),
               vec!["Thorough".to_string(), "No".to_string()],
               "the last answer submits the batch");
}

#[test]
fn the_freeform_ordinal_only_opens_editing_and_later_digits_are_text() {
    // Its own panel, because once `Other` is selected a digit is *typing*
    // (`question_prompt.zig:322` -- `insert_ascii`), not a second choice. A
    // case that selected `Other` and then pressed `1` would be asserting about
    // a draft, not about an ordinal.
    let mut panel = two();
    assert!(matches!(panel.apply(Act::Text('3'), 60, 40), Answered::Redraw),
            "`Other` opens the editor rather than answering (question_prompt.zig:392)");
    assert!(panel.rows(60, 40).join("\n").contains("1 of 2"), "and nothing was answered");
    panel.apply(Act::Text('1'), 60, 40);
    assert_eq!(panel.draft(), Some("1"), "the digit was typed into the draft");
    // Leaving the slot restores the ordinal meaning.
    panel.apply(Act::Up, 60, 40);
    assert_eq!(panel.draft(), None);
    assert!(matches!(panel.apply(Act::Text('1'), 60, 40), Answered::Redraw));
    assert_eq!(submitted(&mut panel, &[Act::Text('1')], 60, 40)[0], "Thorough");
}

#[test]
fn arrows_move_and_enter_takes_the_marked_choice() {
    let mut panel = two();
    panel.apply(Act::Down, 60, 40);
    assert!(matches!(panel.apply(Act::Submit, 60, 40), Answered::Redraw));
    assert_eq!(submitted(&mut panel, &[Act::Text('1')], 60, 40)[0], "Quick");
}

#[test]
fn other_takes_unicode_text_and_an_empty_draft_is_still_an_answer() {
    let mut typed: Vec<Act> = vec![Act::Text('3')];
    typed.extend("네—ok".chars().map(Act::Text));
    typed.extend([Act::Submit, Act::Text('1')]);
    assert_eq!(submitted(&mut two(), &typed, 60, 40)[0], "네—ok");
    assert_eq!(submitted(&mut two(), &[Act::Text('3'), Act::Submit, Act::Text('1')], 60, 40)[0],
               "", "upstream accepts an empty freeform");
}

#[test]
fn a_draft_at_the_cap_refuses_the_next_character_silently() {
    let mut panel = two();
    panel.apply(Act::Text('3'), 60, 40);
    for _ in 0..MAX_FREEFORM_ENCODED_BYTES { panel.apply(Act::Text('x'), 60, 40); }
    assert!(matches!(panel.apply(Act::Text('x'), 60, 40), Answered::Nothing));
    assert_eq!(submitted(&mut panel, &[Act::Submit, Act::Text('1')], 60, 40)[0].len(),
               MAX_FREEFORM_ENCODED_BYTES);
}

#[test]
fn escape_cancels_the_whole_batch_not_the_current_question() {
    let mut panel = two();
    panel.apply(Act::Text('1'), 60, 40);
    assert!(matches!(panel.apply(Act::Escape, 60, 40), Answered::Cancelled));
}

#[test]
fn tab_cycles_between_questions_and_keeps_each_draft() {
    let mut panel = two();
    panel.apply(Act::Text('3'), 60, 40);
    // A character no label, question or ordinal contains, asserted against the
    // draft itself: `contains('a')` would pass on the standing `Thorough` row
    // and prove nothing about whether the draft survived.
    panel.apply(Act::Text('§'), 60, 40);
    assert_eq!(panel.draft(), Some("§"));
    panel.apply(Act::Tab, 60, 40);
    assert!(panel.rows(60, 40).join("\n").contains("Ship it?"));
    assert_eq!(panel.draft(), None, "the second question has no freeform slot selected");
    panel.apply(Act::Tab, 60, 40);
    assert_eq!(panel.draft(), Some("§"), "the draft survived the cycle");
}

#[test]
fn an_ordinal_for_a_choice_the_window_is_not_showing_is_refused() {
    let mut panel = six();
    assert!(!panel.rows(60, 12).join("\n").contains("7. Other"), "the window is bounded");
    assert!(matches!(panel.apply(Act::Text('7'), 60, 12), Answered::Nothing),
            "a choice the user cannot see cannot be taken by number");
}

#[test]
fn the_window_follows_the_selection_and_a_tiny_screen_is_refused() {
    let mut panel = six();
    for _ in 0..6 { panel.apply(Act::Down, 60, 12); }
    assert!(panel.rows(60, 12).join("\n").contains("7. Other"), "the marked choice is visible");
    assert!(!panel.presents_choices(20, 4), "a four-row screen cannot hold a question");
    assert!(panel.rows(20, 4).len() as u16 <= panel.height(20, 4), "rows never exceed the height");
}

#[test]
fn a_long_question_is_clipped_for_the_screen_and_not_in_the_answer() {
    let mut panel = QuestionPanel::new(one_entry(&"L".repeat(400)));
    assert_eq!(panel.id(), QuestionId(1));
    assert!(panel.rows(40, 40).iter().all(|row| row.chars().count() <= 40));
    assert_eq!(submitted(&mut panel, &[Act::Text('1')], 40, 40)[0], "Yes",
               "the answer is the label, whole");
}
```

- [ ] **Step 2: Run and watch them fail** — `cargo test --locked --lib tui::question`. Expected: FAIL, module missing.

- [ ] **Step 3: Implement the panel** to the shape rules above. Do **not** run clippy yet: the module is private and nothing outside its own tests uses it, so `dead_code` fires until Step 7 installs it. The compile-and-test check below is the gate for this step.

- [ ] **Step 4: Run** — `cargo test --locked --lib tui::question`. Expected: PASS, 11 tests.

- [ ] **Step 5: Write the failing wiring tests.** One shared setup, in the shape `asking` (`shell.rs:5379`) builds an approval one, using the real fixture — `shell(rows, cols)` (`shell.rs:3108`) and `Shell::resize(rows, cols)` (`:2858`, **rows first**), following `resize_keeps_the_question_and_its_answer_channel` (`:7039`):

```rust
/// A shell with a turn running and a question batch in front of the user.
fn asking_question(fixture: &mut Fixture, request: QuestionRequest) -> Instant {
    let started = turn_running(fixture, b"ask me\r");
    fixture.apply(UiEvent::Question(request));
    fixture.settle_band(started);
    started
}
```

```rust
#[test]
fn a_question_takes_the_band_and_the_focus() {
    let mut fixture = shell(24, 80);
    asking_question(&mut fixture, a_batch());
    assert!(fixture.band_rows().iter().any(|row| row.contains("1. Thorough")));
    fixture.route_bytes(b"1");
    assert!(fixture.editor.is_empty(), "a digit answered rather than typing");
}

#[test]
fn the_answers_reach_the_runtime_in_order_under_the_request_id() {
    let mut fixture = shell(24, 80);
    asking_question(&mut fixture, a_batch());
    fixture.route_bytes(b"1");
    fixture.route_bytes(b"2");
    match fixture.control.try_recv().expect("an answer was sent") {
        TurnControl::QuestionAnswer { id, answers } => {
            assert_eq!(id, QuestionId(1));
            assert_eq!(answers, vec!["Thorough".to_string(), "No".to_string()]);
        }
        other => panic!("got {other:?}"),
    }
    assert!(fixture.band_rows().iter().all(|row| !row.contains("1. Thorough")),
            "the panel came down before the answer went out");
}

#[test]
fn escape_at_a_question_cancels_the_batch_and_arms_nothing_else() {
    let mut fixture = shell(24, 80);
    asking_question(&mut fixture, a_batch());
    fixture.route_bytes(b"\x1b");
    assert!(matches!(fixture.control.try_recv(),
                     Ok(TurnControl::QuestionCancelled { id: QuestionId(1) })));
    assert!(fixture.control.try_recv().is_err(), "Escape is not also a clear");
}

#[test]
fn a_screen_too_small_for_the_question_cancels_it_rather_than_painting_half() {
    let mut fixture = shell(6, 20);
    asking_question(&mut fixture, a_batch());
    assert!(matches!(fixture.control.try_recv(), Ok(TurnControl::QuestionCancelled { .. })));
    // `released`, not `document`: the notice is behind the pacer, which is how
    // the existing too-small assertions read it (`shell.rs:5816`, `:6004`).
    assert_eq!(fixture.released(), vec![PANEL_TOO_SMALL.to_string()]);
}

#[test]
fn a_resize_that_shrinks_under_a_standing_question_keeps_the_selection_visible() {
    let mut fixture = shell(24, 80);
    let started = asking_question(&mut fixture, six_option_batch());
    for _ in 0..6 { fixture.route_bytes(b"\x1b[B"); }
    assert!(matches!(fixture.resize(12, 60), Resize::Repaint(_)));
    fixture.settle_band(started);
    assert!(fixture.band_rows().iter().any(|row| row.contains("7. Other")),
            "the marked choice survived the shrink");
    assert!(fixture.control.try_recv().is_err(), "a resize is not an answer");
}

#[test]
fn a_question_dismisses_the_menu_and_leaves_the_approval_panel_alone() {
    let mut fixture = shell(24, 80);
    asking_question(&mut fixture, a_batch());
    assert!(fixture.picker.is_none() && fixture.panel.is_none());
}
```

- [ ] **Step 6: Run and watch them fail** — `cargo test --locked --lib tui::shell`. Expected: FAIL, `no variant Question`.

- [ ] **Step 7: Implement the wiring**

- `UiEvent::Question(QuestionRequest)` with a `made_inert` arm running `inert_owned` over every question text, label and description. Task 1's encoder has already removed controls, so the arm is a no-op belt on an encoded payload — kept because `made_inert` is total by design and a variant it did not name would be an exemption.
- The two `TurnControl` variants.
- `Shell` gains `ask: Option<QuestionPanel>`. `Slot` gains `Ask(&'a QuestionPanel)`; `slot()` orders approval, then ask, then menu, with `debug_assert!(!(self.panel.is_some() && self.ask.is_some()))` — a turn asks one thing at a time. `band_rows` and `fit` gain matching arms.
- `ask_question(request)`: build the panel; if `!panel.presents_choices(cols, rows)` then `say(PANEL_TOO_SMALL)` and `work.control(TurnControl::QuestionCancelled { id })` **before** installing anything, mirroring `Shell::ask`; otherwise `dismiss_picker()`, install, `refit()`, `render.request(Reason::Modal)`.
- `consume`: route to `answer(event)` when `self.ask.is_some()`, before the existing `asking()` branch. Add `fn modal(&self) -> bool { self.asking() || self.ask.is_some() }` and use it wherever `asking()` gates the frozen activity clock and the caret.
- `answer(event)`: map `Input` to `Act` (characters to `Act::Text`; `Up`/`Down`/`Tab`/`Submit`/`Escape`/`Cancel`; `Backspace`/`Left`/`Right`/`Home`/`End` for the draft; `Redraw` → `Reason::ExternalDamage`; everything else returns). `Answered::Redraw` → `Reason::Modal`. On `Submitted`/`Cancelled`, **take the panel first**, then send the control message — the order `decide` uses. `Act::Cancel` sends `QuestionCancelled` and then falls through to the existing interrupt gesture so the turn still stops.

- [ ] **Step 8: Run the whole gate for this task, clippy included.** The panel is now installed, so `dead_code` has nothing left to report and no `allow` was needed anywhere.

Run: `cargo test --locked --lib tui:: && cargo test --locked --all-targets && cargo clippy --locked --all-targets -- -D warnings`
Expected: PASS. Report; do not commit.

---

### Task 5: `TuiQuestioner` over the one control channel

**Files:** Modify `src/tui/approval.rs:552-585` (`answered`, `give_back`, `rearm` → `pub(crate)`); `src/tui/question.rs`; `src/tui/worker.rs:444-446`, `:588-600`, `:1287-1304`; tests inline.

**Interfaces produced:** `pub(crate) struct TuiQuestioner { events: Sender<UiEvent>, control: Arc<ControlChannel>, cancel: Cancellation, next: Arc<AtomicU64> }` with `new(events, control, cancel)` and `impl QuestionRequester`.

**Three hazards this task exists to close:**
1. **A wait on `answered()` alone can hang.** The session's root can be cancelled while the control `Sender` is still alive, so the receiver never closes and `answered()` never returns. The wait is a `tokio::select!` against `token.cancelled()`.
2. **`answered()` consumes the loop's waker when it polls ready** (`approval.rs:557-563`), so every exit — the cancelled one included — must `rearm()`, or the turn loop is left unwoken and the next control message never reaches it.
3. **`fetch_add` wraps**, which would reuse a request id. Identity is allocated with a checked update and fails closed.

- [ ] **Step 1: Write the failing tests.** The harness is real, not hypothetical — spelled out once:

```rust
struct Harness {
    questioner: TuiQuestioner,
    /// Kept so a test can occupy the UI channel's permits before the request.
    events_tx: Sender<UiEvent>,
    events_rx: Receiver<UiEvent>,
    control_tx: UnboundedSender<TurnControl>,
    control: Arc<ControlChannel>,
    cancel: Cancellation,
}

impl Harness {
    fn new(ui_capacity: usize) -> Self {
        let (events_tx, events_rx) = tokio::sync::mpsc::channel(ui_capacity);
        let (control_tx, control_rx) = tokio::sync::mpsc::unbounded_channel();
        let control = ControlChannel::new(control_rx);
        // `Cancellation::new` takes the turn-mirror token it resets
        // (`bridge.rs:427`); it is not a no-argument constructor.
        let cancel = Cancellation::new(CancelToken::new());
        Self {
            questioner: TuiQuestioner::new(events_tx.clone(), Arc::clone(&control), cancel.clone()),
            events_tx, events_rx, control_tx, control, cancel,
        }
    }

    /// Runs one `request` on another thread while the test thread drives the
    /// channels. `park_on` needs no runtime (`bridge.rs:574`), so this is the
    /// shape the worker thread uses.
    ///
    /// The fields are destructured first **on purpose**: `drive` needs the
    /// event receiver mutably while the spawned closure holds the questioner,
    /// and `&self.questioner` alongside `drive(&mut self)` is E0502. Split
    /// borrows of disjoint fields are not.
    fn request_while(
        &mut self,
        drive: impl FnOnce(&mut Receiver<UiEvent>, &UnboundedSender<TurnControl>, &Cancellation),
    ) -> Option<Vec<String>> {
        let Harness { questioner, events_rx, control_tx, cancel, .. } = self;
        std::thread::scope(|scope| {
            let handle = scope.spawn(|| questioner.request(&entries()));
            drive(events_rx, control_tx, cancel);
            handle.join().expect("the requester thread")
        })
    }
}

/// A waker that counts, so a test can assert a wake happened rather than infer
/// it. Built the way `approval.rs`'s own `inert_waker` (`:959`) is.
struct Counting(Arc<AtomicUsize>);

impl std::task::Wake for Counting {
    fn wake(self: Arc<Self>) { self.0.fetch_add(1, Ordering::SeqCst); }
    fn wake_by_ref(self: &Arc<Self>) { self.0.fetch_add(1, Ordering::SeqCst); }
}
```

```rust
#[test]
fn a_stale_answer_or_a_stale_approval_is_discarded_and_the_wait_continues() {
    let answers = Harness::new(4).request_while(|events_rx, control_tx, _cancel| {
        events_rx.blocking_recv().expect("the question was shown");
        control_tx.send(TurnControl::Answer(ApprovalAnswer::Deny)).unwrap();
        control_tx.send(TurnControl::QuestionAnswer {
            id: QuestionId(999), answers: vec!["stale".into()] }).unwrap();
        control_tx.send(TurnControl::QuestionAnswer {
            id: QuestionId(1), answers: vec!["fresh".into()] }).unwrap();
    });
    assert_eq!(answers, Some(vec!["fresh".to_string()]));
}

#[test]
fn the_panels_own_cancellation_answers_with_no_answer() {
    let answers = Harness::new(4).request_while(|events_rx, control_tx, _cancel| {
        events_rx.blocking_recv().unwrap();
        control_tx.send(TurnControl::QuestionCancelled { id: QuestionId(1) }).unwrap();
    });
    assert_eq!(answers, None, "no answer is the cancellation sentinel");
}

#[test]
fn an_interrupt_and_a_shutdown_cancel_the_question_and_are_handed_back() {
    for stop in [TurnControl::Cancel { through: 3 }, TurnControl::Shutdown] {
        let mut harness = Harness::new(4);
        let expected = format!("{stop:?}");
        let answers = harness.request_while(|events_rx, control_tx, _cancel| {
            events_rx.blocking_recv().unwrap();
            control_tx.send(stop).unwrap();
        });
        assert_eq!(answers, None);
        assert_eq!(format!("{:?}", harness.control.waiting().expect("handed back")), expected,
                   "the stop still belongs to the loop that can act on it");
    }
}

#[test]
fn a_session_cancelled_after_the_question_is_shown_does_not_hang() {
    // The regression this select! exists for: the control sender is still
    // alive, so `answered()` alone would never return.
    let answers = Harness::new(4).request_while(|events_rx, control_tx, cancel| {
        events_rx.blocking_recv().expect("the question was shown");
        assert!(!control_tx.is_closed(), "the sender is deliberately still alive");
        cancel.cancel();
    });
    assert_eq!(answers, None);
}

#[test]
fn a_cancelled_question_wakes_the_turn_loop_it_parked() {
    // The loop registers **first** and is Pending: that is the waker the
    // question's wait is about to consume, and the one a missing `rearm` would
    // strand. Queueing a message afterwards and finding `recv` ready would not
    // prove anything -- an unwoken loop's channel still holds its message.
    let mut harness = Harness::new(4);
    let control = Arc::clone(&harness.control);          // cloned, so `harness` stays borrowable
    let woken = Arc::new(AtomicUsize::new(0));
    let waker = std::task::Waker::from(Arc::new(Counting(Arc::clone(&woken))));
    let mut context = std::task::Context::from_waker(&waker);
    let mut parked = Box::pin(control.recv());
    assert!(parked.as_mut().poll(&mut context).is_pending(), "the loop is parked on the channel");
    assert_eq!(woken.load(Ordering::SeqCst), 0);

    let answers = harness.request_while(|events_rx, _control_tx, cancel| {
        events_rx.blocking_recv().expect("the question was shown");
        cancel.cancel();
    });
    assert_eq!(answers, None);
    assert_eq!(woken.load(Ordering::SeqCst), 1,
               "the cancelled exit rearmed the loop's waker rather than stranding it");
}

#[test]
fn a_ui_that_is_gone_before_the_question_is_shown_cancels_it() {
    let mut harness = Harness::new(4);
    drop(std::mem::replace(&mut harness.events_rx, tokio::sync::mpsc::channel(1).1));
    assert_eq!(harness.questioner.request(&entries()), None);
}

#[test]
fn a_full_ui_channel_makes_the_question_wait_rather_than_be_dropped() {
    let mut harness = Harness::new(1);
    // Occupied **before** the request, so `send_ui` must wait for room. A
    // `blocking_recv` on an empty channel here would deadlock instead.
    harness.events_tx.blocking_send(UiEvent::Notice("holding the one permit".into())).unwrap();
    let answers = harness.request_while(|events_rx, control_tx, _cancel| {
        std::thread::sleep(Duration::from_millis(20));
        assert!(matches!(events_rx.blocking_recv(), Some(UiEvent::Notice(_))),
                "the occupying event comes out first, freeing the permit");
        assert!(matches!(events_rx.blocking_recv(), Some(UiEvent::Question(_))),
                "and the question arrived once there was room, rather than being dropped");
        control_tx.send(TurnControl::QuestionAnswer {
            id: QuestionId(1), answers: vec!["ok".into()] }).unwrap();
    });
    assert_eq!(answers, Some(vec!["ok".to_string()]));
}

#[test]
fn identity_allocation_fails_closed_rather_than_reusing_a_request_id() {
    let harness = Harness::new(4);
    harness.questioner.set_next_for_test(u64::MAX);
    assert_eq!(harness.questioner.request(&entries()), None,
               "an id that cannot be minted is a cancelled question, not a reused one");
}
```

- [ ] **Step 2: Run and watch it fail** — `cargo test --locked --lib tui::question`. Expected: FAIL, `TuiQuestioner` not found.

- [ ] **Step 3: Implement**

```rust
impl QuestionRequester for TuiQuestioner {
    fn request(&self, entries: &[QuestionEntry]) -> Option<Vec<String>> {
        // Fail closed: `fetch_add` wraps, and a reused id would let a stale
        // answer be accepted as a fresh one.
        let id = QuestionId(self.next
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |held| held.checked_add(1))
            .ok()?);
        let token = self.cancel.token();
        let request = QuestionRequest { id, entries: entries.to_vec() };
        if park_on(send_ui(&self.events, &token, UiEvent::Question(request))).is_err() {
            // No UI to ask, or no session to ask in. The honest result is the
            // cancellation sentinel, which the turn can still carry.
            return None;
        }
        park_on(async {
            loop {
                let message = tokio::select! {
                    biased;
                    // The session's root, not a turn's. The control sender
                    // outlives a cancelled session, so waiting on `answered()`
                    // alone would never return.
                    () = token.cancelled() => { self.control.rearm(); return None; }
                    message = self.control.answered() => message,
                };
                match message {
                    Some(TurnControl::QuestionAnswer { id: seen, answers }) if seen == id =>
                        return Some(answers),
                    Some(TurnControl::QuestionCancelled { id: seen }) if seen == id => return None,
                    // A keystroke on a panel that has already gone, or an
                    // approval answer nobody is waiting for: consumed, so the
                    // next question does not inherit it -- the turn loop's rule.
                    Some(TurnControl::QuestionAnswer { .. } | TurnControl::QuestionCancelled { .. }
                         | TurnControl::Answer(_)) => continue,
                    // The interrupt and the shutdown belong to the loop that can
                    // act on them (`approval::ControlChannel::put_back`);
                    // `give_back` rearms.
                    Some(stop) => { self.control.give_back(stop); return None; }
                    // The UI dropped its sender. `answered` rearmed on the ready
                    // poll; rearming again is spurious and harmless, and keeps
                    // "every exit rearms" true without a reader having to check.
                    None => { self.control.rearm(); return None; }
                }
            }
        })
    }
}
```

`ControlChannel::rearm` becomes `pub(crate)` alongside `answered` and `give_back`. `waiting()` is currently `#[cfg(test)]` on the module; widen it to `#[cfg(test)] pub(crate)` so `tui::question`'s tests can use it, and say in the report that the existing approval tests still compile. `set_next_for_test` is `#[cfg(test)]`.

Build it in `worker::spawn` beside `TuiPrompter`, sharing the same `Arc<ControlChannel>`, `events_tx.clone()` and session `Cancellation`; wrap in `Arc` once and hand a clone to `open_conversation`. Add the two discard arms to the turn loop's `select!` (`worker.rs:1287-1304`) with the comment the existing `Answer(_)` arm carries.

- [ ] **Step 4: Run** — `cargo test --locked --lib tui::question && cargo test --locked --all-targets && cargo test --locked --features fault-injection --test tui`. Expected: PASS. Report; do not commit.

---

### Task 6: PTY scenarios 23 and 23b, docs, full gate

**Files:** Modify `scripts/smoke-tui.sh`; `.prd/06-qa-harness.md`; `.prd/03-tui-port.md`; `docs/parity.md`.

**Interfaces consumed:** `start_fixture`, `run.trial/marker/nonce/require`, `trial.send/wait_for/wait_until/grid/text`, `fixture.bodies()`, `tool_call`, `finish`, `content_only`, `caret_on` — as `scenario_10` uses them.
**Produces:** `fixtures.ask_then_finish(marker)`; `tool_result_text(body, call_id)`; `scenario_23` (`23-question-panel`), `scenario_23b` (`23b-question-cancelled`).

- [ ] **Step 1: Add the fixture and the result reader**

```python
def ask_then_finish(marker):
    """Ask a two-question batch, then say `marker`.

    Two questions rather than one because the result is an *ordered* document: a
    one-question batch passes whether or not order is preserved.
    """
    return [
        {"events": [tool_call("call-0", "ask_user_question", {"questions": [
            {"question": "Which depth?", "options": [
                {"label": "Thorough", "description": "reads every file"},
                {"label": "Quick"}]},
            {"question": "Anything else?", "options": [
                {"label": "No"}, {"label": "Yes"}]}]}), finish("tool-calls")]},
        content_only(marker),
    ]
```

`tool_result_text(body, call_id)` goes beside the existing body readers: parse the captured request JSON and return the `tool_result` content correlated to `call_id`, so both scenarios read the same way.

- [ ] **Step 2: Write scenario 23**

```python
def scenario_23(run):
    """A real question tool call, answered by ordinal and by `Other`."""
    marker = run.marker("question")
    fixture = start_fixture(run, fixtures.ask_then_finish(marker), name="question")
    trial = run.trial("question", gateway=fixture, mode="ask").settled()
    trial.send("decide for me " + run.nonce + "\r")
    trial.wait_for("Which depth?")

    grid = trial.grid("panel")
    for expected, why in [("1. Thorough", "the first model option is offered by number"),
                          ("2. Quick", "so is the second"),
                          ("3. Other", "the synthetic freeform slot is last"),
                          ("reads every file", "the option's description is shown"),
                          ("1 of 2", "the batch says which question this is")]:
        run.require(grid.find(expected) is not None, why)
    run.require(grid.find("Anything else?") is None, "one question is visible at a time")

    trial.send(b"1")                                   # a model ordinal submits at once
    trial.wait_for("Anything else?")
    trial.grid("second")
    trial.send(b"3")                                   # `Other` only opens editing
    trial.wait_until("the freeform slot to take the caret", lambda _t: caret_on(trial, "3. Other"))
    freeform = "다시 묻지 마 — später"
    trial.send(freeform.encode("utf-8"))
    trial.grid("freeform")
    trial.send(b"\r")
    trial.wait_for(marker)

    asked = [body for body in fixture.bodies() if "ask_user_question" in body]
    run.require(asked, "the fixture saw the tool call it scripted")
    payload = json.loads(tool_result_text(asked[-1], "call-0"))
    run.require(payload == [{"question": "Which depth?", "answer": "Thorough"},
                            {"question": "Anything else?", "answer": freeform}],
                "the next model request carries both answers, in question order, unclipped")
    run.require(any(run.nonce in body for body in fixture.bodies()),
                "the nonce this run minted is in the request xfx sent")
    grid = trial.grid("answered")
    run.require(grid.find(marker) is not None, "the fixture's own marker is rendered")
    run.require(grid.text().count(marker) == 1, "response-only: the marker appears once")
    run.require(grid.find("1. Thorough") is None and grid.find("Which depth?") is None,
                "the panel came down and is not left standing behind the answer")
    trial.send(b"\x04")
    run.require(trial.session.wait_exit() == ("exited", 0), "the session left at 0")
    fixture.stop()
```

- [ ] **Step 3: Write scenario 23b**

```python
def scenario_23b(run):
    """Escape cancels the batch, and the model is told exactly that."""
    marker = run.marker("cancelled")
    fixture = start_fixture(run, fixtures.ask_then_finish(marker), name="cancelled")
    trial = run.trial("cancelled", gateway=fixture, mode="ask").settled()
    trial.send("decide for me " + run.nonce + "\r")
    trial.wait_for("Which depth?")
    trial.grid("panel")
    trial.send(b"\x1b")
    trial.wait_for(marker)
    asked = [body for body in fixture.bodies() if "ask_user_question" in body]
    run.require(tool_result_text(asked[-1], "call-0") == "(user cancelled the question)",
                "the cancellation sentinel is byte-exact")
    grid = trial.grid("after")
    run.require(grid.find("Which depth?") is None, "the panel came down on Escape")
    run.require(grid.find(marker) is not None, "and the turn carried on")
    trial.send(b"\x04")
    run.require(trial.session.wait_exit() == ("exited", 0), "the session left at 0")
    fixture.stop()
```

- [ ] **Step 4: Register both** — add `"23-question-panel": scenario_23,` and `"23b-question-cancelled": scenario_23b,` to `SCENARIOS`, and both names to `scenarios=(...)`. `--list` reconciles runner and shell list before anything is driven.

- [ ] **Step 5: Release qualification**

```bash
cargo build --release --locked
cargo build --release --locked --features fault-injection
./scripts/smoke.sh
./scripts/smoke-tui.sh --list
./scripts/smoke-tui.sh
```

Expected: exit 0; 25 scenarios + the oracle; record the observed check total as an observation.

- [ ] **Step 6: Full gate, serially, in this one tree**

```bash
E=${XFX_EVIDENCE:?set XFX_EVIDENCE outside the worktree}
cargo fmt --check > "$E/fmt.log" 2>&1 && echo FMT-OK &&
cargo clippy --locked --all-targets -- -D warnings > "$E/clippy.log" 2>&1 && echo CLIPPY-OK &&
cargo clippy --locked --all-targets --features fault-injection -- -D warnings > "$E/clippy-fault.log" 2>&1 && echo CLIPPY-FAULT-OK &&
cargo test --locked --all-targets > "$E/default.log" 2>&1 && echo DEFAULT-OK &&
cargo test --locked --features fault-injection --test tui > "$E/fault-tui.log" 2>&1 && echo FAULT-TUI-OK &&
cargo test --locked --lib --features fault-injection > "$E/fault-lib.log" 2>&1 && echo FAULT-LIB-OK &&
./scripts/check-no-stubs.sh > "$E/no-stubs.log" 2>&1 && echo NO-STUBS-OK &&
./scripts/check-no-secrets.sh > "$E/no-secrets.log" 2>&1 && echo NO-SECRETS-OK &&
./scripts/check-xfx-identity.sh > "$E/identity.log" 2>&1 && echo IDENTITY-OK &&
./scripts/check-preview-contract.sh > "$E/preview-contract.log" 2>&1 && echo PREVIEW-CONTRACT-OK
```

- [ ] **Step 7: Reconcile the documents.** `.prd/03-tui-port.md`: move ladder item 19 out of the Phase-3 "not implemented" list with the narrowings stated. `.prd/06-qa-harness.md`: add scenarios 23 and 23b with their positive discriminators. `docs/parity.md` was finished in Task 3; re-read it against the shipped surface.

- [ ] **Step 8: Hand back** — files touched, every added test name, observed gate numbers, both smoke totals, raw evidence paths. **No stage, no commit, no push, no merge.** External review and a controller gate rerun come next; the `.prd/tui-phase3/loop.md` gap-matrix row is the controller's.

---

## Self-Review

**Spec coverage.** P3-QUESTION's three clauses map to Tasks 3+5 (real request path), Task 4 (ordinals and `Other`), Tasks 1+6 (ordered answers in the next model request; byte-exact cancellation sentinel). Ladder 19's two halves are both in Task 4. "Keep question state in focused modules rather than adding substantial new logic to the already large shell" is why `src/tui/question.rs` owns the panel and the requester while `shell.rs` gains a field, a slot arm and a router. "A real tool path, not an approval variant" is `PermissionKind::Interaction` plus the no-authority test. "A model ordinal submits directly while `Other` only opens editing" is `a_model_ordinal_submits_at_once` and `the_freeform_ordinal_only_opens_editing_and_later_digits_are_text` — two panels, because once `Other` is selected a digit is typing rather than a second ordinal. "A panel without a reachable product trigger does not satisfy P3-QUESTION" is scenario 23 on a release binary.

**Placeholder scan.** No TBD, no "handle edge cases", no unresolved helper. Every helper this plan names is either verified to exist (facts table) or defined here (`terminal_safe`, `bounded`, `only`, `Scripted`/`scripted`/`tight`/`run`, `Harness`).

**Type consistency.** `QuestionEntry`/`QuestionOption`/`AskUserQuestionInput` are defined in Task 1 and unchanged in 3, 4, 5. `QuestionId`/`QuestionRequest`/`Act`/`Answered` are defined in Task 4 and unchanged in 5. `QuestionRequester::request` has one signature throughout. `TurnControl::QuestionAnswer { id, answers }` and `QuestionCancelled { id }` are spelled identically in 4 and 5. `ToolSpec::input_schema` is used by value, as declared. `Cancellation::new` is called with a `CancelToken`, as declared (`bridge.rs:427`).

**Residual risk after all six tasks.** The requester blocks the runtime thread through `park_on`, as `TuiPrompter` already does; if tool execution ever becomes concurrent, this seam needs revisiting. Backpressure is covered by a unit test only — driving a full `UiEvent` channel deterministically through a real PTY is not something the existing harness offers.
