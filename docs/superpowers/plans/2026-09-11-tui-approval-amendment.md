# TUI Approval Amendment Feedback Implementation Plan

The second half of P3-APPROVAL. Readiness (identity, disclosure, commit receipts) is
implemented and locally committed at `666e0b2`; this plan does not revisit it.

What is missing is the user's own sentence. Upstream lets the user amend a decision with
text and delivers that text to the model **as subsequent user context after the tool
result** -- never as an edit to the tool's arguments and never as a reason to mint
authority the user did not grant.

Pinned upstream, with the two revisions kept apart because they are two different
claims:

- Research revision `ef1d0d0c6a1a87a621fc54d23f86ffec51755779`,
  `approval_decision.zig::apply`: Tab enters an eligible allow/deny amendment draft;
  Enter submits the selected decision; numeric keys insert draft text while amending
  rather than submit. `materializeResponse` carries optional nonempty feedback
  **separately** from the decision.
- Port pin `580a0c5da9386317251968c09c1cee69e763487a`, downstream:
  `tool_admission.zig:2039-2050` retains the existing file authorization on an allowed
  decision and copies feedback separately; `orchestrator.zig:6598-6618` appends the
  denial result **then** the feedback; `:7431-7458` appends the committed-file result
  **then** the feedback; `appendPermissionFeedbackAfterToolResult` (`:1950-1970`) also
  emits the user-feedback event.

These are two revisions, not one. Nothing here claims they are identical.

## Global constraints

- **Feedback is context, not authority.** The original grant is unchanged by feedback.
  A denial with feedback is still a denial and still writes nothing. An allow with
  feedback still runs the **original** arguments. No code path may read feedback to
  widen, narrow, or re-key a `Grant`, an `ExecutionAuthority`, or a `PolicyDecision`.
- **`auto` and `yolo` produce no feedback.** They never reach `PermissionSession::ask`
  (`src/permission/policy.rs:589-594`), so there is nothing to carry and no default to
  invent.
- **No session-global pending slot.** Feedback is a value on a per-call envelope,
  created and consumed inside one tool call. An early return, a panic, or a
  cancellation must be structurally incapable of handing one call's feedback to
  another. Review by checking for the *absence* of any `Option<String>` field on
  `PermissionSession`, `ToolContext`, or `TurnMachine`.
- **One rendering owner, one editor.** Draft rows are composed by the existing
  `Panel::compose` and reused by the alternate plane.
- **Blank is absent.** An empty or whitespace-only draft produces `None`; no empty user
  message reaches the wire.
- Counts here are *derived at implementation time*, never copied.
  `scripts/smoke-tui.sh:13-16` states the printed totals are summed from the scenario
  list itself, so record the observed before/after pair rather than asserting a literal.

## File structure

| File | Change |
|---|---|
| `src/permission/policy.rs` | `ApprovalResponse`; additive `ApprovalPrompter::respond`; `Decided`; `ask`/`decide` migration |
| `src/tools/spec.rs` | `ToolResult::feedback` + `with_feedback` |
| `src/tui/worker.rs`, `tests/permissions.rs` | Unit 1's compiler-guided rename fallout (approval tests, prompter fixtures) |
| `src/tools/mutate.rs` | decision site takes the envelope, attaches feedback to both outcomes |
| `src/tools/terminal.rs` | same, for commands |
| `src/agent/machine.rs` | per-step feedback buffer, flushed after all results |
| `src/session/event.rs` | `SessionEvent::ToolFeedback` |
| `src/session/store.rs` | `TurnStep::ToolFeedback`, reduce arm, `history_messages` arm |
| `src/tui/approval.rs` | `Drafts`/`Draft`/`DraftPaste`, `Edit` + `PasteByte` actions, split `answered` arms, `respond` impl |
| `src/output.rs` | `SessionStepRow::Feedback` + its mapping and render arms |
| `src/tui/approval_screen.rs` | draft rows on the alternate plane |
| `src/tui/bridge.rs` | `TurnControl::Answer { feedback }` |
| `src/tui/shell.rs` | translation of composer edit actions into the panel while a draft is active |
| `scripts/smoke-tui.sh` | scenario 25 |
| `docs/parity.md`, `.prd/06-qa-harness.md` | current-tense reconciliation |

Three units. Unit 1 and Unit 2 are the end-to-end feedback transport (policy -> tool ->
machine -> session -> wire) and are testable with no TUI at all. Unit 3 is the UI that
produces a feedback string, and it is separate because a draft geometry is a screen
fact and the transport is not.

---

## Unit 1: The decision envelope

The trait change is **additive**; the session's own method is a **complete migration**.
Two different choices, for two different reasons.

`ApprovalPrompter` gets a defaulted method, because every existing implementor
(`TtyPrompter` at `src/permission/policy.rs:391`, the `RecordingPrompter` fixtures in
that file's tests, the prompters in `tests/permissions.rs`) is *correct* when it
produces no feedback -- a line-oriented `y/a/n` prompt has no draft.

There is **no recursion hazard** in the default body. `respond`'s default calls
`request`, and for an old implementor `request` is that implementor's own real body, so
the call terminates there. `TuiPrompter` overrides **both**: `respond` carries the real
implementation and `request` delegates to it (§Transport out of the TUI). A type that
overrode neither would not compile, since `request` has no default.

`PermissionSession::decide` is **renamed**, because a caller that keeps the old name
silently drops the user's sentence. Making the old spelling impossible is cheaper than
reviewing every future caller for whether it remembered.

**The rename is compiler-guided and scoped to `PermissionSession::decide` alone** -- not
a textual rename. `src/tui/shell.rs:1593` has an unrelated private
`Shell::decide(&mut self, event: Input, now: Instant)` which must not be touched, and
`src/tui/theme.rs:333` has a `decided`. Delete the old method, build, repair exactly
what the compiler names. Unit 1's writer therefore owns, beyond the product files:
`src/tui/worker.rs` and its approval tests, `tests/permissions.rs`, the in-file
`mod tests` of `src/permission/policy.rs`, and every approval/shell fixture that
constructs a prompter. Those are part of this delivery, not follow-up work.

```rust
// src/permission/policy.rs

/// What the user answered, and anything they said about it.
///
/// The two travel together and mean different things. The answer is authority.
/// The feedback is *context*, delivered to the model after the result, and it
/// changes neither the arguments that ran nor the authority that let them
/// (`tool_admission.zig:2039-2050`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalResponse {
    pub answer: ApprovalAnswer,
    /// Nonempty when the user amended the decision. Never `Some("")`: a blank
    /// draft is an absent one, decided at the surface that owns the draft.
    pub feedback: Option<String>,
}

impl ApprovalResponse {
    /// An answer with nothing said about it.
    pub fn plain(answer: ApprovalAnswer) -> Self {
        Self { answer, feedback: None }
    }
}

pub trait ApprovalPrompter: Send {
    fn request(&mut self, request: &ApprovalRequest) -> io::Result<ApprovalAnswer>;

    /// The same question, on a channel that can also carry a sentence.
    ///
    /// Defaulted rather than required: a prompter that cannot offer a draft --
    /// `TtyPrompter`'s `y/a/n` line, every test fixture -- is *correct* when it
    /// reports none.
    fn respond(&mut self, request: &ApprovalRequest) -> io::Result<ApprovalResponse> {
        Ok(ApprovalResponse::plain(self.request(request)?))
    }
}

/// A resolved decision and whatever the user said while resolving it.
///
/// Returned by value, per call. There is deliberately no field anywhere that
/// holds feedback between calls: a session-scoped slot would survive an early
/// return and be read by the next tool as though it had been typed at that one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decided {
    pub decision: PolicyDecision,
    pub feedback: Option<String>,
}
```

`PolicyDecision` itself is **not** changed. Its variants are matched exhaustively in
both tool call sites and across `tests/permissions.rs`; adding a field would churn every
one of those matches to carry a value only two of them use, and would put user prose
inside the type whose whole job is to be the authority answer.

`ask` (currently `src/permission/policy.rs:597-636`) calls `respond` instead of
`request`, keeps its `Always` grant behavior exactly as it is, and returns `Decided`:

- `Once` -> `Allow { source: InteractiveOnce }`, feedback carried.
- `Always` -> `self.grant(Grant::new(tool, target))` **unchanged**, then
  `Allow { source: InteractiveAlways }`, feedback carried. The grant stays keyed by
  tool and target; feedback is not part of the key and cannot be.
- `Deny` -> `Deny { cause: UserDenied, reason }`, feedback carried.
- `Err` -> `Deny { cause: ApprovalChannelFailed, .. }`, `feedback: None`: a channel
  that failed did not deliver a sentence.
- No prompter -> `Deny { cause: NoApprovalChannel, .. }`, `feedback: None`.

`decide` becomes `decide_with_feedback(&mut self, action) -> Decided`, and every
non-ask path wraps its existing `PolicyDecision` with `feedback: None`. `evaluate`
is untouched: it is pure and asks nobody.

`ToolResult` (`src/tools/spec.rs:188-206`) gains one field:

```rust
pub struct ToolResult {
    pub ok: bool,
    pub output: String,
    pub detail: String,
    pub fatal: bool,
    /// What the user said when they answered the approval this call needed.
    ///
    /// Delivered as a **separate user message after this result**, never merged
    /// into `output`: merging would let a user's sentence be read as the tool's
    /// own report of what it did.
    pub feedback: Option<String>,
}

impl ToolResult {
    /// Attaches what the user said. Blank is absent.
    pub fn with_feedback(mut self, feedback: Option<String>) -> Self {
        self.feedback = feedback.filter(|text| !text.trim().is_empty());
        self
    }
}
```

`success`, `failure`, and `revoked` set `feedback: None`. Every other construction site
is found by **the compiler**, not by a grep: the fields are `pub` and the new one has no
`Default`, so each remaining struct literal is a hard error naming its own line. A grep
would be weaker evidence than the build already gives.

### Unit 1 call sites

`src/tools/mutate.rs:589-636` and `src/tools/terminal.rs:190-215` share one shape: one
decision, then mint, then consume, then apply -- and **failures can occur after the
decision**. Feedback must survive all of them, because the user said it about a call
that was attempted, whatever the attempt did.

```rust
// src/tools/mutate.rs, replacing the `let decision = ...` / `let source = ...` pair
let Decided { decision, feedback } = context
    .permissions()
    .decide_with_feedback(ProposedAction::Mutation(&plan));
let source = match decision {
    PolicyDecision::Allow { source } => source,
    PolicyDecision::Deny { reason, .. } => {
        return ToolResult::failure(format!("{tool} was not permitted: {reason}"))
            .with_feedback(feedback)
    }
    PolicyDecision::Prompt => {
        return ToolResult::failure(format!(
            "{tool} was not permitted: the approval was never resolved"
        ))
        .with_feedback(feedback)
    }
};
```

Every subsequent `return` on this path -- `revoked` on a failed `consume`, the `Stale`
and `Failed` arms of `namespace::apply`, and the success arm -- takes
`.with_feedback(feedback.clone())` or moves it, whichever the borrow allows. The
`is_noop` early return above the decision does **not**: it returns before anyone was
asked. `src/tools/terminal.rs` takes the identical shape for `ProposedAction::Command`.

`feedback` is **not** passed into `mint_mutation`, `mint_command`, `consume`, or
`namespace::apply`. Those take the plan that was judged, unchanged.

### Unit 1 tests

1. `an_allowed_call_runs_the_original_arguments_and_carries_the_sentence` -- a prompter
   returning `Once` plus `"use tabs not spaces"`; assert the file on disk is exactly the
   staged bytes of the original plan, and `result.feedback == Some("use tabs not spaces")`.
2. `a_denied_call_changes_no_file_and_still_carries_the_sentence` -- `Deny` plus text;
   assert the target does not exist (or is byte-identical to its preimage) and the
   feedback survives on the failure result.
3. `an_always_answer_grants_exactly_the_tool_and_target_it_did_before` -- answer
   `Always` with feedback; assert `session.grants()` equals the no-feedback case.
   This is the "feedback is not authority" test.
4. `a_blank_draft_is_no_feedback` -- prompter returns `Some("   ")`; assert
   `result.feedback.is_none()`.
5. `a_prompter_that_cannot_amend_still_compiles_and_reports_nothing` -- an implementor
   of only `request`; assert `respond` yields `feedback: None`. This pins that the
   default method is correct for old implementors rather than merely present.
6. `a_failed_approval_channel_carries_no_sentence` -- prompter returns `Err`; assert
   `Deny { cause: ApprovalChannelFailed }` and `feedback: None`.
7. `an_authority_that_went_stale_after_the_decision_still_carries_the_sentence` --
   uses the existing race interlude (`context.run_race_interlude()`) to make
   `namespace::apply` return `Stale`; assert `revoked`, `fatal == true`, and feedback
   present. This is the "failures can occur after the decision" case.

RED first, against a compiling `ApprovalResponse` whose `feedback` is hard-wired to
`None`. That produces real assertion failures on tests 1, 2, 4 and 7, and real passes
on 3, 5 and 6 -- which is the correct shape, not a padded failure count. Do not
manufacture a compile error and call it RED.

---

## Unit 2: Transport and replay

This unit decides **where** the sentence goes on the wire, and it is the one place a
wrong choice produces an invalid request rather than a cosmetic defect.

### The ordering constraint, from the encoders

`src/llmux/protocol.rs:24-32` states the two load-bearing properties of the Anthropic
wire: consecutive same-role messages are **merged**, and within a merged user message
the `tool_result` blocks **lead**. `wire_messages` (`:202-221`) implements the merge.
The gateway wire's validator (`src/gateway/protocol.rs:316-355`) enforces only that no
tool result is an orphan and no call id repeats -- there is no adjacency rule there, and
`a_tool_result_may_correlate_to_a_call_from_an_earlier_step` (`:639`) pins that.

So a `Message::user(feedback)` placed **after** all tool results of a step is valid on
both wires and merges into the same user message with the results ahead of it. Placed
*between* two results of a multi-call batch it would also validate, but the merge would
hoist the later result above the sentence and scramble which result the sentence was
about. Therefore: **buffer per step, flush after the loop.**

### `machine.rs`

The per-call loop at `src/agent/machine.rs:546-588` journals the result, pushes
`Message::tool_result` into `self.suffix`, checks `result.fatal` (`:574`), and
`yield_once()`s (`:588`). Declare a **function-local** buffer beside the loop -- not on
`self` -- and fill it immediately after the existing `suffix.push`:

```rust
let mut amendments: Vec<(String, String)> = Vec::new();
// ... inside the loop, after `self.suffix.push(Message::tool_result(..))`:
if let Some(text) = result.feedback.clone() {
    amendments.push((call.id.clone(), text));
}
```

Flush it in exactly two places, through one helper so the two cannot drift: before the
`return Err(TurnError::ToolAuthorityRevoked { .. })` at `:574` (a turn that ends still
owes the journal what the user said), and after the `for` loop before `Ok(())`.

```rust
/// Puts every amendment of this step into the journal and the prompt, in call
/// order, **after** every result of the step.
///
/// After, because `llmux/protocol.rs:24-32` merges consecutive user messages and
/// hoists `tool_result` blocks to the front of the merged one: a sentence
/// interleaved between two results of a batch would arrive above the result it
/// was about. The `call_id` keeps the origin, since the position no longer can.
fn flush_amendments(
    &mut self,
    journal: &mut dyn TurnJournal,
    amendments: Vec<(String, String)>,
) {
    for (call_id, text) in amendments {
        journal.record(SessionEvent::ToolFeedback { call_id, text: text.clone() });
        self.suffix.push(Message::user(text));
    }
}
```

`&mut dyn TurnJournal` is the real parameter type, taken from the enclosing
`execute_tool_calls` (`src/agent/machine.rs:470-475`). No new trait is introduced.

**Exactly two flush sites: normal end and `fatal`.** No third is added. Every other exit
drops the buffer, and that is the contract rather than an accident of where the code
happens to return.

**Disposition on each exit, stated so it is testable rather than implied.** Only calls
that *completed* -- ran, produced a `ToolResult`, and had that result journaled and
pushed -- can have contributed to the buffer, because the push happens directly after
the result push.

| Exit | Journaled | On the next request |
|---|---|---|
| normal end of the batch | every completed call's sentence | every one, in call order, after all results |
| `fatal` at `:574` | every completed call's sentence, **including the fatal call's own** | nothing: the turn ends with `TurnError::ToolAuthorityRevoked` and sends no request. A resumed session shows the records; `history_messages` still drops the incomplete group |
| turn cancelled -- observed at the `yield_once()` boundary (`:588`), returned at the **next iteration's** check (`:524`, `TurnError::Cancelled`) | **nothing** -- the buffer is dropped unflushed | nothing, now or on resume |
| unsupported tool at `:549` (`TurnError::ToolCallUnsupported`) | **nothing** -- same drop, same reason | nothing |
| panic | nothing -- the buffer unwinds with the frame | nothing |

**The last call needs an explicit guard, or the cancellation row is false.** Cancellation
is *observed* at `yield_once()` but *acted on* at the top of the next iteration (`:524`).
After the final call there is no next iteration: the loop ends and control reaches the
normal flush, which would journal and send sentences from a turn the user just stopped.
So the normal flush site checks first:

```rust
// after the `for` loop, before flushing
if self.request.cancel.is_cancelled() {
    drop(amendments);
    return Err(TurnError::Cancelled);
}
self.flush_amendments(journal, amendments);
```

This is a guard on the **existing** flush site, not a new pathway: it uses the same
`self.request.cancel.is_cancelled()` predicate the loop head already uses at `:524`, and
it makes the one-call case behave like every multi-call one.

The journal-versus-wire split is the point: a *fatal* turn still records what the user
said, so a resumed session can show it, while sending no request; a *cancelled* turn
records nothing, because the user stopped it. Nothing survives into a later turn as loose
state -- the only carrier is the journal record, bound to its `call_id` and to a group
that must be complete before `history_messages` will flush it.

The buffer is local to the step: a panic unwinds it, a cancellation drops it, the two
flush sites consume it once. There is no path where step *n*'s buffer is visible to
step *n+1*.

### `session/event.rs` and `session/store.rs`

**Do not reuse `SessionEvent::UserMessage`.** `src/session/store.rs:1774` reduces
`UserMessage` by pushing a **new `HistoryTurn`**. A mid-tool `UserMessage` would
therefore close the turn, and every remaining `ToolResult` of the batch would land in a
turn with no assistant tool call ahead of it -- an orphan result, rejected by
`validate` at `src/gateway/protocol.rs:339-345` on the next resume.

```rust
// src/session/event.rs, beside ToolResult
/// What the user said when they answered the approval one call needed.
///
/// Correlated by `call_id` and **not** a `UserMessage`: `UserMessage` opens a
/// turn (`store.rs:1774`), and opening one here would leave the rest of the
/// batch's results in a turn with no call to answer.
ToolFeedback { call_id: String, text: String },
```

Add the `"tool_feedback"` arm to `SessionEvent::kind`.

```rust
// src/session/store.rs, beside TurnStep::ToolResult
ToolFeedback { call_id: String, text: String },
```

Reduce (`apply`, near `:1804`): push `TurnStep::ToolFeedback` onto the current turn's
steps, exactly as the `ToolResult` arm does. Like `ToolResult`, it is an error when no
turn is open.

`history_messages` (`:474-556`) gains one arm inside the `for step` match:

```rust
TurnStep::ToolFeedback { text, .. } => {
    // Pushed into `pending`, and **`awaiting` is untouched**: this is not an
    // answer to a call, so it cannot complete a group. Because the machine
    // emits it only after every result of its step, `awaiting` is already
    // empty here and the existing flush below carries it out with its group.
    pending.push(Message::user(text.clone()));
}
```

The `if awaiting.is_empty() { out.append(&mut pending) }` tail is unchanged, and that is
what makes the partial-batch case correct for free: a session that died after *k* of
*n* results has a non-empty `awaiting`, so `pending` never flushes and the feedback is
dropped **with** its incomplete group. No orphan result, no orphaned sentence.

### The inspector: `session show` must display it

`src/output.rs:684` declares `SessionStepRow`, `:770-789` maps `TurnStep` onto it
exhaustively, and `:873-878` renders the rows. A new `TurnStep` variant makes that match
non-exhaustive, and the failure mode of "fix the build" is a `_ => continue` that makes
a user's own sentence the one thing `xfx session show` hides.

So the variant is **shown**, not skipped:

```rust
// src/output.rs, beside SessionStepRow::Tool
Feedback {
    /// Which call the user said it about.
    call_id: String,
    /// What they said, verbatim. Not withheld: unlike `Tool::output`, this is
    /// the user's own text, and a session view that hid it would be hiding the
    /// half of the exchange the user wrote.
    text: String,
},
```

with the mapping arm at `:789` and a render arm at `:878` that labels the row as user
feedback and names its `call_id`. `tests/sessions.rs` gains a case asserting the row
appears in `session show` output, with its text and its correlation, for a session whose
journal holds one `ToolFeedback`.

### Schema version stays at 1

`EVENT_SCHEMA_VERSION` (`src/session/event.rs:41`) is **not** bumped, and
`EventEnvelope::validate`'s equality check at `:252-256` is why. Bumping it would make
every existing log unreadable by the new binary, to solve a problem in the other
direction.

The consequence is real and belongs in the record rather than in a surprise: a **new**
binary reads every old log, but an **older** binary reading a log that contains a
`tool_feedback` event rejects that frame as an unknown variant. Preview and stable can
therefore be mixed forward but not backward across an amended session. Note this in
`docs/parity.md` alongside the amendment entry; do not paper over it with a bump that
trades a downgrade caveat for a total one.

### Unit 2 tests

8. `feedback_reaches_the_prompt_after_the_result_it_was_about` -- one call, one
   amendment; assert `suffix` is `[tool_result, user(text)]` in that order.
9. `a_multi_call_batch_puts_every_result_before_every_sentence` -- three calls, two
   amended; assert all three `tool_result` messages precede both user messages, and
   the two sentences are in call order.
10. `an_amended_batch_renders_to_a_valid_request_on_both_wires` -- build the prompt from
    9 and assert `CompletionRequest::body()` is `Ok` (`validate` is `pub(crate)`, so an
    integration test must go through `body`, which calls it at
    `src/gateway/protocol.rs:304-308`); then assert the llmux `wire_messages` output has
    one merged user message whose `tool_result` blocks lead its text block. This is the
    balance test.
11. `a_resumed_session_replays_the_same_order_with_no_orphan_calls` -- journal the events
    of 9, reduce, `history_messages`; assert message-for-message equality with the live
    `suffix`.
12. `a_batch_that_died_after_the_first_of_three_results_replays_neither_the_group_nor_its_sentence`
    -- journal `Assistant{3 calls}`, one `ToolResult`, one `ToolFeedback`; assert
    `history_messages` yields the user turn and nothing else.
13. `a_fatal_result_journals_the_sentence_and_sends_no_request` -- a revoked authority
    mid-batch, amended. Two assertions, because the disposition table has two columns:
    the `ToolFeedback` event **is** in the journal before `TurnError::ToolAuthorityRevoked`,
    **and** no further provider request was made. Then reduce that journal and assert
    `history_messages` drops the incomplete group, feedback included -- the logged/replayed
    distinction, proven rather than described.
14. `an_output_limit_does_not_clip_a_sentence_into_the_result` -- an amended call whose
    `output` is at the tool output cap; assert `output` is unchanged by the presence of
    feedback, that the feedback is a separate message, and that no truncation marker
    lands inside the feedback text. Feedback is not subject to the tool output limit
    because it is not tool output.
14b. `session_show_displays_an_amendment_with_its_call_id` -- in `tests/sessions.rs`:
    a journal with one `ToolFeedback`, rendered through `session show`; assert the text
    and its correlation appear. Guards against the non-exhaustive-match repair being a
    `_ => continue` that hides the user's own words.
14c. `a_cancelled_final_call_neither_journals_nor_sends_its_sentence` -- **one** call,
    amended, cancellation set deterministically after its result is pushed and before the
    loop ends. Assert `Err(TurnError::Cancelled)`, **no** `ToolFeedback` in the journal,
    and no further provider request. This is the case the loop-head check at `:524`
    cannot reach, so it fails without the normal-flush guard.
14d. `a_cancelled_multi_call_batch_drops_every_buffered_sentence` -- three calls, first
    two amended, cancellation set after the second result. Assert the turn returns
    `Cancelled` at the next iteration's `:524` check, that neither sentence is journaled,
    and that a resume replays neither. Together with 13 (fatal journals) and 8-9 (normal
    sends), the three flush outcomes are each pinned by their own test.

---

## Unit 3: The draft surface

Two **independent** drafts per question: one for the allow side (choice 0, `Once`) and
one for the deny side (choice 2, `Deny`). Choice 1 (`Always`) is not draft-eligible --
`Always` is the answer that buys the rest of the session, and the panel's own copy
(`always_scope`) is the sentence the user is answering; an amendment attached to it
would read as a condition on a grant that is keyed by tool and target and cannot carry
one.

Key semantics, as settled:

| Key | Not editing | Editing a draft |
|---|---|---|
| `Tab` | selected choice is draft-eligible -> enter its draft; otherwise cycle to the next choice (existing behavior) | swallowed -- Tab enters a draft, it does not leave one |
| `Up` / `Down` | move the marked choice (existing behavior) | **leave editing** and move the marked choice; **both drafts keep their text** |
| `1` `2` `3` | choose, without moving the marker (existing `answered` behavior) | **draft characters** |
| other `Text(c)` | swallowed (existing) | inserted |
| `Submit` | submit the marked choice | submit the marked choice |
| `Escape` | `Deny`, **no feedback** (existing) | `Deny`, **no feedback**; both drafts discarded |
| `Cancel` (Ctrl-C) | `Deny` + the existing put-back so the turn still stops | stop the turn, **drop both drafts**, no feedback |

`Escape` and `Enter`-on-`Deny` are **two different refusals** and the difference is
deliberate. `Escape` is the fail-safe exit -- a decision xfx was not given -- so it
sends `Deny` with `feedback: None` even when the deny draft holds text: a user bailing
out of a panel did not choose to say that sentence to the model. Submitting `3. No` with
Enter is a chosen refusal and **does** send the deny draft. Assert both in one test so
the pair cannot drift.

Leaving a draft is **arrow navigation**, pinned at `580a0c5d`:
`core/permissions/approval_decision.zig:214-225` -- `moveChoice` updates `selected` and
then calls `amendment.clearActive()`, so moving the choice is what ends editing; its
tests at `:285-302` drive tab / type `y` / `move_choice.previous` -> choice 2 / tab /
type `n` and assert the two drafts survive separately.
`core/app/input_approval_runtime.zig:127-141` maps `cursor_up` and `history_up` to
`move_choice.previous` and down to `.next` **regardless of whether an amendment is
open**, so `Up`/`Down` are never draft-editing keys on either surface.

No new `ClearActive` key is introduced, and `Escape` keeps its existing meaning. xfx's
`Escape` = deny-and-discard is a local fail-safe retention, stated here so review reads
it as a deliberate retention rather than a gap.

On submit, the panel consumes **only** the draft belonging to the submitted choice and
discards the other. A user who typed a reason for refusing and then allowed does not
send the refusal reason.

### Editing keys and bounds

A draft holds an `Editor` for its text, **but `Editor` alone does not implement the
editing contract.** `Editor::apply` (`src/tui/editor.rs:306`) returns `None` for
`Undo`, `Redo`, `Yank`, `PasteStart` and `PasteEnd` -- the arm at `:381-394` says so in
its own comment: those keys are the *session's*, answered by `Shell` out of an
`EditHistory` and a `Paste`, not by the buffer. So "same by construction" would have
been false for exactly the five keys that need the most care, and this plan does not
claim it. `Editor` gives the draft its motion, insertion, deletion, grapheme handling
and caret arithmetic; **`Drafts` owns the rest, per draft.**

```rust
/// One side's amendment: its text, its own undo history, and its own kill slot.
///
/// Per draft, not per panel: two drafts that shared a history would let an undo
/// typed at the deny side pull back a delta recorded on the allow side.
pub(crate) struct Draft {
    editor: super::editor::Editor,
    history: super::edit_history::EditHistory,
}
```

`Draft` mirrors the shell's own wiring, which is already built and reviewed (P3-EDIT):
an insertion returns `Option<Delta>` from `Editor::insert` / `Editor::apply`, the
`Delta` goes to `EditHistory::record`, and `Undo`/`Redo` call
`EditHistory::undo(|d| editor.revert(d))` and `redo(|d| editor.replay(d))`. The kill
keys (`KillToEnd`, `KillToStart`) are `Editor::apply`'s own and record normally.
**Undo, redo and yank stay functional in a draft**; they are not quietly dropped, and no
test for them is deleted.

Yank takes its text from `killed`, **not** `killed_text`, which is `#[cfg(test)]`
(`src/tui/edit_history.rs:327-330`) and would not exist in a release build. The spans
are discarded, because a draft carries no entities:

```rust
// owned before the mutable borrow: `killed()` borrows `self.history`
// immutably and `insert` needs `self.editor` mutably.
Action::Yank => {
    let text = self.history.killed().map(|(text, _)| text.to_owned());
    if let Some(text) = text {
        if let Some(delta) = self.editor.insert(&text) {
            self.history.record(delta);
        }
    }
}
```

**Paste is a focused draft-local assembler, not `super::paste::Paste`.** Upstream does
support pasting into an amendment (`insertAmendmentSlice`, `approval_decision.zig:180-193`),
so paste is not dropped wholesale -- but the composer's assembler is the wrong tool
here: it collapses anything past `COLLAPSE_ABOVE` into an **entity summary** with a
block id (`Pasted::Collapsed`, `src/tui/paste.rs:153-168`), and a feedback string that
reached the model as `[#3 pasted 40 lines]` would be a summary standing in for text the
user believes they sent. Feedback carries no entities.

```rust
/// Bytes arriving between the bracketed-paste markers, for one draft.
///
/// Bounded at `MAX_FEEDBACK_BYTES`, which is also the draft's own cap, because
/// a feedback string has one budget rather than two. Refused **atomically**: an
/// oversized paste inserts nothing and the panel says so, rather than leaving
/// the first 4096 bytes of somebody's file in the draft.
pub(crate) struct DraftPaste {
    buffer: Vec<u8>,
    refused: bool,
}

impl DraftPaste {
    pub(crate) fn begin(&mut self) { self.buffer.clear(); self.refused = false; }

    /// One content byte. Filtered by `super::paste::accepted`, which is reused:
    /// which bytes are content is one fact (`paste_framing.zig:112-135`).
    pub(crate) fn byte(&mut self, byte: u8) {
        if !super::paste::accepted(byte) { return; }
        if self.buffer.len() >= MAX_FEEDBACK_BYTES { self.refused = true; return; }
        self.buffer.push(byte);
    }

    /// The finished text, or `None` when it was refused.
    ///
    /// `from_utf8_lossy` before the length check, because that is where the
    /// decoded size is first known: a non-UTF-8 byte becomes a three-byte
    /// replacement scalar, so the encoded bound is not the decoded one.
    pub(crate) fn finish(&mut self, draft_len: usize) -> Option<String> {
        let bytes = std::mem::take(&mut self.buffer);
        if self.refused { return None; }
        let text = String::from_utf8_lossy(&bytes).into_owned();
        (draft_len.saturating_add(text.len()) <= MAX_FEEDBACK_BYTES).then_some(text)
    }
}
```

`MAX_FEEDBACK_BYTES = 4096`, checked **before** insertion on both the raw and the
decoded size, and a refusal is visible on the panel's hint row rather than silent.
`super::paste::MAX_PASTE_BYTES` (8 MiB, `paste.rs:86`) is the composer's budget and is
not this one.

`approval::Action` gains two variants:

```rust
/// An editing key, meaningful only while a draft is open.
Edit(super::input::Action),
/// One raw byte of a bracketed paste, while a draft is open.
///
/// Separate because the composer's byte route feeds `super::paste::Paste` and
/// would collapse a large paste into an entity; a draft takes its bytes here.
PasteByte(u8),
```

**`Shell` translation, exactly.** Only while a draft is open, and only for:
`Left`, `Right`, `Home`, `End`, `WordLeft`, `WordRight`, `Backspace`, `Delete`,
`DeleteWordLeft`, `KillToEnd`, `KillToStart`, `Undo`, `Redo`, `Yank`, `PasteStart`,
`PasteEnd`, plus raw paste bytes as `PasteByte`.

`HistoryPrevious` maps to `approval::Action::Up` and `HistoryNext` maps to
`approval::Action::Down` -- the runtime pin (`input_approval_runtime.zig:127-141`) makes
`cursor_up`/`history_up` one action and `cursor_down`/`history_down` another, so the
shell collapses each pair *at the translation* rather than giving the panel four keys to
keep in step. `Up` and `Down` are therefore **never** in the `Edit` subset: they move the
choice and end editing whether or not a draft is open, so routing them into the `Editor`
would take away the user's only way out.

While a draft is open, paste bytes go to `DraftPaste` and **must not** reach the
composer's `Paste`: a modal surface that leaked bytes into the composer would leave text
behind the panel that the user never sees and cannot delete. Assert that direction, and
the reverse -- with no draft open, no panel key reaches the composer and no composer
edit reaches a closed panel.

`answered` (`src/tui/approval.rs:499-522`) stays the one place key meaning lives for
both surfaces: its signature grows a `&mut Drafts` and the digit arms branch on whether
a draft is open. No second copy for the alternate plane -- the module's own doc comment
gives the reason.

Its current `Action::Down | Action::Tab` arm (`:511`) **must be split**, because the two
keys stop meaning the same thing:

```rust
// moves the choice, and ends editing without losing either draft
Action::Down => {
    drafts.leave();
    *selected = (*selected + 1) % CHOICES.len();
    None
}
// Up is the same, mod-decrementing as it does today
Action::Tab if drafts.editing() => None, // enters a draft; never leaves one
Action::Tab => {
    if drafts.enter(*selected).is_none() {
        // not draft-eligible (choice 1, `Always`): the existing cycle
        *selected = (*selected + 1) % CHOICES.len();
    }
    None
}
```

`drafts.leave()` ends editing and keeps both buffers; only `submit` and the two refusal
keys dispose of them.

### Geometry and readiness

Opening, closing, or growing a draft changes the panel's row count, which invalidates
any committed-screen readiness receipt for this request at these dimensions: the screen
that was proven is no longer the screen the user is looking at. Call
**`Readiness::invalidate`** (`src/tui/approval_readiness.rs:259`) on every draft
transition that changes the composed row count.

`invalidate` rather than `intend(None)` because it is the direct name for the effect,
and there is no weakening either way: `intend` at `:185-191` calls `invalidate`, and
`invalidate` at `:259-263` clears the previously-seen disclosure as well as the receipt.
Any claim that `intend(None)` preserves *seen* is wrong -- both revoke identically. The
next genuinely disclosed frame re-earns readiness, which is the intended cost.

The panel's height stays a function of the composition (`Panel::compose` remains the
single source -- "the height is the length of this"). Draft rows are allotted from the
same measured budget the scope rows use, so a long draft in an 80x24 window truncates
the draft's own display, never the target, the controls, or the scope. Full
subject/controls/scope disclosure remains a readiness requirement and is not relaxed.

**Draft width is the panel's own inner width, one expression, shared by three
consumers.** The panel indents every row but the title by `INDENT` (two cells,
`INDENT_CELLS`), and `fitted` computes its wrap budget as
`cols.saturating_sub(INDENT_CELLS).max(1)` (`src/tui/approval.rs`). The draft uses the
identical value:

```rust
/// The columns a draft's text has, which is the panel's inner width.
///
/// One function, called by all three of the draft's consumers -- the `Editor`
/// that applies a key at a width, the rows the panel paints, and the caret the
/// shell places -- because a caret computed at one width and a wrap computed at
/// another is a caret standing on the wrong cell.
const fn draft_cols(cols: u16) -> u16 {
    cols.saturating_sub(INDENT_CELLS).max(1)
}
```

`Editor::apply(action, draft_cols(cols))`, `Editor::rows(draft_cols(cols))` and
`Editor::point(draft_cols(cols))` -- never a bare `cols` at any of the three.

**Deny is never blocked.** Unready gates the affirmative answers only, as today.

Both surfaces get drafts. The alternate `ApprovalScreen` composes its draft rows from
the same helper; the primary `say` path is invisible there, so its not-ready and
undisclosed notices stay on the plane the screen owns.

### Transport out of the TUI

```rust
// src/tui/bridge.rs
Answer {
    id: super::approval_readiness::ApprovalId,
    answer: ApprovalAnswer,
    /// The amendment typed at the draft for `answer`, if there was one. The
    /// other side's draft is discarded at submit and never travels.
    feedback: Option<String>,
},
```

`TuiPrompter` implements `respond` as the real body -- the existing
`request` (`src/tui/approval.rs:782`) moves into it almost unchanged -- and `request`
becomes `self.respond(request).map(|response| response.answer)`, so there is one
implementation and no chance of the two disagreeing about a stale id. All four existing
early-outs keep their answers and gain `feedback: None`:

- `Err(Stopped::Cancelled)` on the send -> `Deny`, nothing said.
- `Err(Stopped::UiGone)` -> `nobody_to_ask()`.
- root `token.cancelled()` in the select -> `rearm()`, `Deny`, nothing said.
- a `TurnControl::Answer` whose id is not this question's -> **still consumed**, for the
  reason the existing comment gives, and now the stale answer's feedback is consumed
  with it rather than inherited.

### Unit 3 tests

15. `tab_enters_the_draft_of_an_eligible_choice_and_cycles_past_an_ineligible_one`.
16. `moving_the_choice_leaves_editing_and_keeps_both_drafts` -- the upstream test at
    `approval_decision.zig:285-302`, transcribed: Tab, type `y`, `Up` to choice 2, Tab,
    type `n`; assert editing ended on the `Up`, that the marked choice moved, and that
    the two drafts hold `y` and `n` separately. Then submit `Deny` and assert only `n`
    travels.
17. `digits_typed_into_a_draft_are_characters_and_digits_outside_one_are_answers` --
    the numeric-draft test, both directions in one assertion pair.
18. `up_and_down_move_the_choice_whether_or_not_a_draft_is_open` -- drive `Up`/`Down`
    (and their `HistoryPrevious`/`HistoryNext` spellings) in both states; assert the
    marker moves in all four cases and that neither ever reaches the `Editor`.
19. `ctrl_c_at_a_draft_stops_the_turn_and_sends_no_feedback` -- assert the answer is a
    refusal, `feedback: None`, and the `TurnControl` is still put back for the loop.
20. `a_cancelled_draft_does_not_reach_the_next_request` -- ask a second question after
    19 and assert its response carries `None`. The leak test.
21. `a_stale_approval_id_is_consumed_with_its_feedback` -- deliver an `Answer` for a
    retired id carrying text; assert the live question is unaffected and the text
    appears nowhere.
22. `opening_a_draft_revokes_a_committed_readiness_receipt` -- prove readiness, open a
    draft, assert the receipt is gone and an affirmative is refused until re-proven.
23. `a_draft_never_shortens_the_target_the_controls_or_the_scope` -- 80x24 with a real
    durable-policy request and a long draft; assert the disclosure metadata still
    reports the target, all three controls and the scope as complete.
24. `no_panel_key_reaches_the_composer_and_no_composer_edit_reaches_a_closed_panel` --
    including paste bytes: with a draft open, assert the composer's `Paste` received
    nothing and its text is unchanged.
25. `escape_at_a_filled_deny_draft_refuses_without_the_sentence_but_enter_sends_it` --
    the M6 pair in one test: same draft text, two exits, `feedback: None` versus
    `Some(text)`.
26. `undo_redo_and_yank_work_inside_a_draft` -- type, kill to end, undo, redo, yank;
    assert the draft's text and caret at each step against the `Draft`'s own
    `EditHistory`. Asserted on **real behavior**, never by comparing against
    `Editor::apply`, which returns `None` for all three (`src/tui/editor.rs:381-394`)
    and would make the test pass on a draft that did nothing.
27. `a_bracketed_paste_becomes_draft_text_and_never_an_entity` -- feed real bytes through
    `PasteStart` / `PasteByte` / `PasteEnd`; assert the draft text equals the pasted
    bytes, that the caret is after them, that no entity was minted, and that the string
    the prompter returns is the text itself rather than any summary.
28. `an_oversized_paste_is_refused_atomically_with_a_visible_notice` -- 4097 bytes;
    assert the draft is **unchanged** (not truncated), a hint row states the refusal, and
    a subsequent in-bounds paste still works.
29. `a_multibyte_paste_is_not_cut_mid_character` -- a paste whose byte count crosses the
    cap in the middle of a multi-byte scalar; assert the refusal is atomic and no
    replacement scalar reaches the draft. Also assert the decoded-size check, since
    `from_utf8_lossy` can grow a non-UTF-8 byte threefold.
30. `a_draft_wraps_and_places_its_caret_at_the_panel_inner_width` -- assert
    `draft_cols(cols)` is the same value `fitted` uses; then drive a long CJK/emoji draft
    at 80 and at 40 columns and assert the painted rows and the caret cell agree at both,
    across a resize between them.

---

## Release scenario 25

Added to `scripts/smoke-tui.sh` beside the existing rows. One scenario, two trials, on a
real pseudoterminal against a release binary, with fixture-only credentials and the
response-only marker the harness already uses (`smoke-tui.sh:33-40`: no live credential,
no network, nothing written into the repository).

**Trial A -- allowed with an amendment.** The fixture model asks for a `write_file`. The
scenario presses `Tab` to enter the allow draft, types a unique phrase that appears
nowhere else in the harness, presses `Enter` on `1. Yes`, and then asserts, in this
order:

1. the file on disk holds the **original** staged bytes, byte for byte;
2. the captured next request contains the `tool_result` for that call **and** a user
   message equal to the phrase, with the result ahead of it;
3. the phrase does not appear inside the tool result's `output`;
4. full grid, caret position and `termios` trio, as every other scenario records them.

**Trial B -- denied with an amendment.** Same request; `Tab` into the deny draft, type a
second unique phrase, submit `3. No`. Assert the target does not exist, the next request
carries the refusal result followed by the phrase, and the grid/caret/termios trio.

Both phrases are chosen so that a grep for them across the evidence directory
distinguishes "the sentence reached the wire" from "the sentence was echoed on screen".

Scenario 20's assertions were made viewport-aware in R11; if scenario 25's panel
geometry changes any shared fixture, re-derive rather than adjust literals.

## Gate

Per `.prd/tui-phase2/loop.md` §Gate contract, one target directory, sequential, with a
separate faulty build for the fault-injection rows:

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

Unit 3 additionally builds both release variants and runs

```bash
scripts/smoke.sh <path-to-xfx> "$E/cli"
scripts/smoke-tui.sh <path-to-xfx> --faulty <path-to-xfx-with-faults> "$E/tui"
```

Record the scenario and check totals the harness prints before and after scenario 25 as
**observations**. The tree currently reports 27 scenarios and 584 checks; the harness
sums those from its own list, so a literal here would be a claim this plan made rather
than a receipt.

## Order and closure

Unit 1 -> Unit 2 -> Unit 3. Unit 1's tests are the only ones that can run before the
enum plumbing exists; Unit 2 makes the wire correct with no UI; Unit 3 is the only unit
that needs a terminal. One writer per unit, RED with real assertion failures against
compiling inert code, an independent review per unit, and a final external review plus
controller gate before the commit. No user pause between units.

What this plan does **not** close: P3-COMMIT, P3-LAYOUT, P3-THEME, P3-KEYS,
P3-ACTIVITY, P3-DIAGNOSTIC, P3-WRAP, and P3-SHIP. Landing all three units closes
P3-APPROVAL locally; the preview carrier is still separate work.
