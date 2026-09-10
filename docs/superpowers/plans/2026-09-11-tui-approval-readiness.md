# TUI Approval Readiness and Request Identity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** An approval answer is accepted only after a frame that really disclosed *this* request's target, controls and always-scope on *this* screen was written, flushed and reconciled; and every request carries an identity a stale keystroke cannot match.

**Architecture:** A TUI-only `ApprovalId` is minted once per `ApprovalPrompter::request` call and rides the `UiEvent`/`TurnControl` envelope; `crate::permission` is untouched. A new `src/tui/approval_readiness.rs` holds the receipt state machine. Disclosure is decided **at composition, from the source text and its allotment** — never by comparing an already-clipped string — because every truncation on both surfaces is silent: `fitted` takes `allotted` wrapped rows and appends an ellipsis (`approval.rs:458-483`), `heading` takes `SUBJECT_ROWS` and `SUMMARY_ROWS` (`approval_screen.rs:298-314`), and `compose` truncates the heading again into whatever the choices left (`approval_screen.rs:374-375`).

**Tech Stack:** Rust 2021, `cargo test --locked --all-targets`, `scripts/smoke-tui.sh`.

**Spec:** `.prd/tui-phase3/ssot.md` row `P3-APPROVAL`; `.prd/tui-phase3/loop.md:68` and §"Architecture rulings — 2026-09-10".

## Global Constraints

- Scope is `P3-APPROVAL` readiness plus request identity **only**. Amendment drafts are a later slice: do not build them, and do not shape the types so adding a feedback payload later forces identity to be reworked.
- `P3-COMMIT` self-check, byte decoding and recovery are out of scope. No shadow decoder.
- Do not change `src/permission/policy.rs`'s `ApprovalRequest` (`:323-341`), `ApprovalAnswer` (`:344-352`), `ApprovalPrompter` (`:363-365`) or `PermissionSession::ask` (`:597-636`).
- Preserve UI-thread terminal ownership, one question at a time, normal-buffer scrollback and restoration. `/clear` stays user-invoked; no automatic scrollback erasure.
- Deny, Escape and Ctrl-C stay answerable at every moment, ready or not.
- **No new runtime test hooks and no sleeps.** No `#[allow(dead_code)]`, no `#[ignore]`, no skipped assertions. No credentials or network in fixtures.
- **Workers do not stage, commit or push.** Each task ends by handing back an uncommitted tree with its gate output. The controller commits after the external review gate.
- **One writer, one review delivery, three tasks.** The tasks are ordered units of work with their own RED/GREEN cycles, not three separately shippable trees: Task 1's and Task 2's `pub(crate)` items have no consumer until Task 3 wires them, so an all-targets clippy run between them fails on dead code. Each task still writes its failing test first, makes it pass, and runs `cargo fmt --check`; **`cargo clippy` runs once, in Task 3, when the state has its callers.** Never reach for `#[allow(dead_code)]` or `-A dead_code` to make an intermediate task lint clean -- the warning is telling the truth until Task 3 lands.
- Gate commands are those in `.prd/tui-phase2/loop.md` §Gate contract; the release build variants and `scripts/smoke.sh` / `scripts/smoke-tui.sh` are separate commands from the default suite.

---

## File Structure

| File | Responsibility |
|---|---|
| `src/tui/approval_readiness.rs` *(new)*, `src/tui/mod.rs` | `ApprovalId`, `Surface`, `Disclosure`, `Intent`, `Outcome`, `Readiness`. Pure state; no I/O, no `Shell`, no `Band`. |
| `src/tui/approval.rs` | `fitted` reports completeness; `Panel::compose` emits rows plus `Disclosure`; `TuiPrompter` mints the id and discards answers that are not its own. |
| `src/tui/approval_screen.rs` | `Composed` carries `Disclosure` through `heading`/`choices`/`compose`. |
| `src/tui/bridge.rs`, `src/tui/worker.rs` | `UiEvent::Approval(ApprovalAsked{id,request})`, `TurnControl::Answer{id,answer}`, and the worker's exhaustive arms. |
| `src/tui/shell.rs` | Stores the asked id; readiness field; `intend_approval`/`approval_landed`/`approval_write_failed`/`invalidate_approval`/`reconcile_approval`/`approval_ready`; the gate in `decide`. |
| `src/tui/event_loop.rs` | Reports every frame outcome; calls `reconcile_approval` after `resolve_resize`. |
| `scripts/smoke-tui.sh` | Scenario `24-approval-readiness`, registered in both lists. |

---

## Task 1: The readiness state machine

**Files:**
- Create: `src/tui/approval_readiness.rs`; Modify: `src/tui/mod.rs` (`mod approval_readiness;`)
- Test: in-module `#[cfg(test)] mod tests`

**Interfaces produced** (all `pub(crate)`):

```rust
pub(crate) struct ApprovalId(pub u64);                     // Clone, Copy, Debug, PartialEq, Eq
pub(crate) enum Surface { Inline, Alternate }              // Clone, Copy, Debug, PartialEq, Eq
pub(crate) enum Outcome { Painted, Unchanged }

/// What one composition really put on the screen, decided by the composition
/// itself. Every field is a claim about the *source* text, not about a string
/// that has already been cut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Disclosure {
    /// Absolute terminal row, one-based, of the last control row.
    pub last_control_row: u16,
    /// The three control labels reached the screen with nothing cut off.
    pub controls_whole: bool,
    /// The always-scope line reached the screen with nothing cut off.
    pub scope_whole: bool,
    /// Tool and target were rendered in full: no row dropped, no ellipsis.
    pub subject_whole: bool,
    /// The change, or the notice standing in for it, is on the screen now.
    pub change_visible: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Intent { /* id, surface, cols, rows, disclosure */ }

impl Intent {
    /// `None` when this composition could not disclose the question, whatever
    /// the terminal then does with the bytes.
    pub(crate) fn capture(
        id: ApprovalId,
        surface: Surface,
        cols: u16,
        rows: u16,
        disclosure: Disclosure,
    ) -> Option<Intent>;
}

#[derive(Default)]
pub(crate) struct Readiness { /* pending, provisional, receipt, seen */ }

impl Readiness {
    pub(crate) fn intend(&mut self, intent: Option<Intent>);
    pub(crate) fn landed(&mut self, outcome: Outcome);
    pub(crate) fn reconcile(&mut self, cols: u16, rows: u16, resize_pending: bool);
    pub(crate) fn write_failed(&mut self);
    pub(crate) fn invalidate(&mut self);
    pub(crate) fn ready(&self, id: ApprovalId, cols: u16, rows: u16) -> bool;
    /// Why an affirmative is impossible at this size, for the notice.
    pub(crate) fn undisclosed(&self) -> bool;
}
```

**Contract, clause by clause from `.prd/tui-phase3/loop.md:68`:**

1. `capture` returns `None` unless `controls_whole && scope_whole && subject_whole && rows > 0 && cols > 0 && last_control_row > 0 && last_control_row <= rows`. The row is one-based, so a zero is not a row on any screen -- it is an unset field, and an unset field must never read as a disclosed control. `change_visible` is recorded, **not required** — the clause is "changed content *or its notice*, visible now **or previously observed** for that screen state".
2. `landed(Painted)` moves the pending intent to *provisional*, never straight to the receipt.
3. `landed(Unchanged)` keeps an existing receipt only when the pending intent equals it; otherwise it clears both. A zero-byte repaint never grants first-time readiness.
4. `reconcile(cols, rows, resize_pending)` promotes provisional → receipt only when `!resize_pending` and the dimensions still equal the intent's. **When `resize_pending` is true it clears the provisional, the receipt and `seen` — all three.** This is the post-write check. A `SIGWINCH` that lands inside the write is drained one tick later at `event_loop.rs:619-620` (`signals::take_winch()` → `render.mark_resize`), and `resize_pending` then stays true for the whole `RESIZE_DEBOUNCE` (`render_request.rs:163-165`) while `geometry` still holds the *old* size. Dropping only the provisional would leave the previous receipt matching those unchanged dimensions, so an affirmative typed during the debounce would grant against a screen that has already reflowed. Clearing `seen` with it is what stops a later `NoChange` repaint from resurrecting that receipt. A redraw after the resize settles earns a new one normally.
5. `write_failed` clears pending, provisional and receipt: a refused write may have left half a frame.
6. `invalidate` clears everything. Callers: external damage, `/clear`, and the surface being taken down.
7. `ready` requires a receipt for this id at these dimensions, and `receipt.change_visible || seen == Some((id, cols, rows))`. `seen` is set when a promoted receipt had `change_visible`.
8. Intent equality for retention covers id, surface, dimensions and disclosure — **not the cursor**. A marker that moved repaints and re-grants through `Painted`; it must not invalidate.
9. `undisclosed()` is true when the last `intend` was `None`, so the shell can say *why* an affirmative is impossible instead of gating silently.

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn full() -> Disclosure {
        Disclosure {
            last_control_row: 20,
            controls_whole: true,
            scope_whole: true,
            subject_whole: true,
            change_visible: true,
        }
    }

    fn intent(id: u64) -> Option<Intent> {
        Intent::capture(ApprovalId(id), Surface::Inline, 80, 24, full())
    }

    /// A receipt, the way every caller earns one: intend, land, reconcile.
    fn granted(id: u64) -> Readiness {
        let mut readiness = Readiness::default();
        readiness.intend(intent(id));
        readiness.landed(Outcome::Painted);
        readiness.reconcile(80, 24, false);
        readiness
    }

    #[test]
    fn a_committed_and_reconciled_frame_grants_its_own_request_only() {
        let readiness = granted(7);
        assert!(readiness.ready(ApprovalId(7), 80, 24));
        assert!(!readiness.ready(ApprovalId(8), 80, 24));
    }

    #[test]
    fn nothing_short_of_a_reconciled_write_grants() {
        let mut readiness = Readiness::default();
        readiness.intend(intent(7));
        assert!(!readiness.ready(ApprovalId(7), 80, 24), "an intent is not a receipt");
        readiness.landed(Outcome::Painted);
        assert!(!readiness.ready(ApprovalId(7), 80, 24), "a write is not yet reconciled");
        readiness.reconcile(80, 24, false);
        assert!(readiness.ready(ApprovalId(7), 80, 24));
    }

    #[test]
    fn a_winch_that_landed_inside_the_write_refuses_the_promotion() {
        let mut readiness = Readiness::default();
        readiness.intend(intent(7));
        readiness.landed(Outcome::Painted);
        readiness.reconcile(80, 24, true);
        assert!(!readiness.ready(ApprovalId(7), 80, 24));
        readiness.reconcile(80, 24, false);
        assert!(!readiness.ready(ApprovalId(7), 80, 24), "the provisional was dropped, not parked");
    }

    #[test]
    fn a_winch_revokes_the_standing_receipt_at_the_very_same_dimensions() {
        // `geometry` still says 80x24 for the whole debounce
        // (`render_request.rs:163-165`), so a receipt kept here would match and
        // grant against a screen that has already reflowed.
        let mut readiness = granted(7);
        readiness.reconcile(80, 24, true);
        assert!(!readiness.ready(ApprovalId(7), 80, 24), "the old receipt survived the signal");

        // And no repaint that writes nothing may bring it back: `seen` went
        // with it, so this is a first grant and a first grant needs bytes.
        readiness.intend(intent(7));
        readiness.landed(Outcome::Unchanged);
        readiness.reconcile(80, 24, false);
        assert!(!readiness.ready(ApprovalId(7), 80, 24), "a no-change repaint resurrected it");

        // A real redraw once the resize has settled earns readiness again.
        readiness.intend(intent(7));
        readiness.landed(Outcome::Painted);
        readiness.reconcile(80, 24, false);
        assert!(readiness.ready(ApprovalId(7), 80, 24));
    }

    #[test]
    fn a_first_frame_that_wrote_nothing_never_grants() {
        let mut readiness = Readiness::default();
        readiness.intend(intent(7));
        readiness.landed(Outcome::Unchanged);
        readiness.reconcile(80, 24, false);
        assert!(!readiness.ready(ApprovalId(7), 80, 24));
    }

    #[test]
    fn a_no_change_repaint_retains_only_a_receipt_of_the_same_identity() {
        let mut readiness = granted(7);
        readiness.intend(intent(7));
        readiness.landed(Outcome::Unchanged);
        assert!(readiness.ready(ApprovalId(7), 80, 24), "same identity, still proven");
        readiness.intend(intent(9));
        readiness.landed(Outcome::Unchanged);
        assert!(!readiness.ready(ApprovalId(7), 80, 24));
        assert!(!readiness.ready(ApprovalId(9), 80, 24));
    }

    #[test]
    fn a_moved_marker_does_not_invalidate_but_a_changed_screen_does() {
        let mut readiness = granted(7);
        readiness.intend(intent(7));
        readiness.landed(Outcome::Painted);
        readiness.reconcile(80, 24, false);
        assert!(readiness.ready(ApprovalId(7), 80, 24), "the cursor is not part of identity");
        let taller = Disclosure { last_control_row: 21, ..full() };
        readiness.intend(Intent::capture(ApprovalId(7), Surface::Inline, 80, 24, taller));
        readiness.landed(Outcome::Unchanged);
        assert!(!readiness.ready(ApprovalId(7), 80, 24), "a different layout is a different screen");
    }

    #[test]
    fn a_failed_write_grants_nothing_and_revokes_what_was_granted() {
        let mut readiness = granted(7);
        readiness.intend(intent(7));
        readiness.write_failed();
        assert!(!readiness.ready(ApprovalId(7), 80, 24));
    }

    #[test]
    fn damage_and_a_changed_size_both_close_the_gate() {
        let mut readiness = granted(7);
        assert!(!readiness.ready(ApprovalId(7), 100, 24), "a different width is a different screen");
        readiness.invalidate();
        assert!(!readiness.ready(ApprovalId(7), 80, 24));
    }

    #[test]
    fn an_undisclosed_composition_is_capturable_by_nothing_and_says_so() {
        for cut in [
            Disclosure { controls_whole: false, ..full() },
            Disclosure { scope_whole: false, ..full() },
            Disclosure { subject_whole: false, ..full() },
            Disclosure { last_control_row: 25, ..full() },
            // One-based, so zero is no row at all.
            Disclosure { last_control_row: 0, ..full() },
        ] {
            assert!(Intent::capture(ApprovalId(7), Surface::Inline, 80, 24, cut).is_none());
        }
        let mut readiness = Readiness::default();
        readiness.intend(None);
        assert!(readiness.undisclosed(), "the shell needs a reason to give the user");
    }

    #[test]
    fn a_change_counts_as_seen_only_after_this_request_really_showed_it() {
        let hidden = Disclosure { change_visible: false, ..full() };
        let away = |id| Intent::capture(ApprovalId(id), Surface::Alternate, 80, 24, hidden);

        let mut never = Readiness::default();
        never.intend(away(7));
        never.landed(Outcome::Painted);
        never.reconcile(80, 24, false);
        assert!(!never.ready(ApprovalId(7), 80, 24), "this request never showed the change");

        let mut seen = granted(7);
        seen.intend(away(7));
        seen.landed(Outcome::Painted);
        seen.reconcile(80, 24, false);
        assert!(seen.ready(ApprovalId(7), 80, 24), "scrolled away, but observed at this size");
    }
}
```

- [ ] **Step 2: Run the tests and confirm they fail on assertions**

Write the types with real signatures and inert bodies first (`capture` returning `None`, `ready` returning `false`), so nothing fails on a missing name.
Run: `cargo test --locked --lib tui::approval_readiness`
Expected: compiles; the granting, retention and reconciliation cases fail on assertions.

- [ ] **Step 3: Implement, then rerun**

Run: `cargo test --locked --lib tui::approval_readiness` — PASS. Then `cargo fmt --check`. **No clippy run here**: nothing consumes `Readiness` until Task 3, so an all-targets lint would fail on dead code that Task 3 removes by using it.

---

## Task 2: Disclosure metadata and request identity

**Files:** `src/tui/approval.rs`, `src/tui/approval_screen.rs`, `src/tui/bridge.rs:112-147` and `:384-417`, `src/tui/shell.rs:1117`, `:1202-1266`, `:1520-1533`, `src/tui/worker.rs`

**Interfaces produced:**
- `pub(crate) struct ApprovalAsked { pub id: ApprovalId, pub request: ApprovalRequest }` in `src/tui/approval.rs`
- `UiEvent::Approval(ApprovalAsked)`; `TurnControl::Answer { id: ApprovalId, answer: ApprovalAnswer }`
- `pub(crate) struct Composition { pub rows: Vec<String>, pub disclosure: Disclosure }`
- `Panel::compose(&self, cols: u16, terminal_rows: u16, band_top: u16, offset: u16) -> Composition`
- `ApprovalScreen::composition(&self, cols: u16, terminal_rows: u16) -> Composition`
- `fn fitted(text: &str, cols: u16, rows: u16) -> Fitted` where `pub(crate) struct Fitted { pub rows: Vec<String>, pub complete: bool }`

**One authoritative composition per surface, and exactly what each returns.**

- `Panel`: `compose` becomes the single construction. `rows(cols, terminal_rows)` returns `compose(cols, terminal_rows, 0, 0).rows` and `height` measures that, exactly as they relate today (`approval.rs:318-365`). **`caret_row(&self, terminal_rows)` is left alone**: it has no `cols` parameter, it is derived from `Shape::first_choice()` plus the selection rather than from any row, and its two callers (`shell.rs:863`, `approval.rs:907`) pass no width. Changing its signature to route it through `compose` would be churn with no reader — the `band_top`/`offset` arguments exist only to place the disclosure's absolute rows, and readiness is the only caller that needs them.
- `ApprovalScreen`: the existing private `compose` stays the single construction and grows a fourth field, `Composed { rows, caret, viewport, disclosure }`. `rows()` (`:242-244`), `caret()` (`:252-254`), `scroll_by()` (`:225-235`) and `presents_choices()` (`:274-289`) keep reading it exactly as they do now; `caret` and `viewport` stay private to the module. The new `composition(cols, terminal_rows)` is a projection — `Composition { rows: composed.rows, disclosure: composed.disclosure }` — so there is one layout and no second definition of it.

**How each `Disclosure` field is computed — from the source, never from a clipped string:**

| Field | Inline (`Panel`) | Alternate (`ApprovalScreen`) |
|---|---|---|
| `subject_whole` | The summary carries the target (`policy.rs:328-330`); `fitted`'s `complete` = `wrap(text, budget).len() <= allotted` (`approval.rs:460-472`), so an ellipsised or dropped row makes it false. | `safe_rows(tool+target).len() <= SUBJECT_ROWS` before the `.take` at `approval_screen.rs:304`, **and** those rows survived `compose`'s second truncation at `:374-375`. |
| `controls_whole` | For each of `approval::labels(request.tool)`: `clip(&format!("{marker}{label}"), cols) == format!("{marker}{label}")` — the clip at `approval.rs:354-356` was a no-op. | The same equality against the clip at `approval_screen.rs:393`, and all three rows survived `choices.truncate(height)` at `:372`. |
| `scope_whole` | `fitted(&format!("2 = {always_scope}"), ..).complete` (`approval.rs:345-349`). | `safe_rows(&format!("2 = {always_scope}"), budget).len() <= 1`, the `.take(1)` at `:339`. |
| `change_visible` | `true` exactly when `subject_whole`: an inline question is only ever chosen for a change the summary shows whole (`approval.rs:193-202`). | At least one change row is inside the viewport — `viewport > 0 && scroll < change_rows.len()` (`:376-386`). |
| `last_control_row` | `band_top + offset + local_index + 1`, one-based. | `local_index + 1`. |

**Why "no clip at all" replaces the marker-length rule.** `presents_choices` (`approval_screen.rs:274-289`) compares an already-clipped label against clipped rows and only requires the row be longer than its marker, so `2. Yes, and don't ask again for th` satisfies it — the disclosure that distinguishes "this call" from "the rest of the session" is exactly the part that got cut. Readiness demands the clip changed nothing. The same rule defeats a maliciously long target chosen to share a visible prefix with a benign one: the longer target ellipsises or drops a row, `subject_whole` is false, and no receipt exists at that width. `presents_choices` keeps its own weaker job — deciding whether to ask at all — unchanged.

**Why the row offset is a parameter.** `band_rows` puts the activity row before the panel when the geometry has one (`shell.rs:664-677`), so a panel-local index is not a terminal row. The shell passes `geometry.band_top()` and the count of rows it pushed before the panel; the composition returns absolute rows, which is what `capture`'s `last_control_row <= rows` guard is about.

**Why the id is minted in the prompter.** `PermissionSession::ask` (`policy.rs:597-636`) builds the request and immediately matches the three answers; nothing there consults readiness. `TuiPrompter` is `Clone` (`approval.rs:624`) and already holds an `Arc<ControlChannel>`, so it takes an `Arc<AtomicU64>` exactly as `TuiQuestioner` does (`question.rs:710`, `:725`, `:731-745`): one counter across clones, `checked_add` failing closed. Minting per `request()` call is what makes two identical-looking questions distinguishable.

- [ ] **Step 1: Census the callers**

```bash
rg -n 'UiEvent::Approval|TurnControl::Answer' src/ tests/
```

Both variants are consumed in roughly fifty places — product arms and test fixtures across `src/tui/` and `tests/` — and the compiler names every one of them: reshaping an enum variant makes each site a hard error, so the census is exhaustive by construction rather than by this grep. Expect to touch all of them; that churn is the cost of the envelope, not a sign of a wrong turn. Repair only the shape at each site. `shell.rs:1117` is the arm that reaches `Shell::ask`, which now takes `(id, request)`, and the shell's own `asking()` helper (`shell.rs:5600-5602`) is one of the fixtures that constructs the event.

- [ ] **Step 2: Write the failing tests**

In `src/tui/approval.rs` tests, using that module's own existing request fixtures — `request(tool)` at `:743-754` and `with_diff(bytes)` at `:755` — and **not** a new one:

```rust
#[test]
fn two_identical_questions_are_asked_under_different_ids_across_clones() {
    let (mut prompter, mut events, _control, answers) = a_prompter();
    let mut clone = prompter.clone();
    // The counter starts at 1, so the two calls take 1 and then 2.
    answers
        .send(TurnControl::Answer { id: ApprovalId(1), answer: ApprovalAnswer::Once })
        .expect("the channel is open");
    prompter.request(&request("edit_file")).expect("answered");
    answers
        .send(TurnControl::Answer { id: ApprovalId(2), answer: ApprovalAnswer::Once })
        .expect("the channel is open");
    clone.request(&request("edit_file")).expect("answered");
    assert_ne!(
        asked_id(&mut events),
        asked_id(&mut events),
        "the same sentence twice is two questions"
    );
}

#[test]
fn an_answer_carrying_another_requests_id_is_consumed_and_ignored() {
    let (mut prompter, mut events, control, answers) = a_prompter();
    answers
        .send(TurnControl::Answer { id: ApprovalId(9_999), answer: ApprovalAnswer::Always })
        .expect("the channel is open");
    answers
        .send(TurnControl::Answer { id: ApprovalId(1), answer: ApprovalAnswer::Deny })
        .expect("the channel is open");
    assert_eq!(prompter.request(&request("edit_file")).expect("answered"), ApprovalAnswer::Deny);
    assert!(control.waiting().is_none(), "the stale answer was consumed, not put back");
    let _ = asked_id(&mut events);
}

#[test]
fn a_clipped_always_label_is_not_a_disclosed_control() {
    let panel = Panel::new(request("edit_file"));
    let wide = panel.compose(80, 24, 1, 0);
    assert!(wide.disclosure.controls_whole && wide.disclosure.scope_whole);
    // 30 cells cuts the second label mid-sentence, which is where the
    // difference between one call and the whole session is written.
    let narrow = panel.compose(30, 24, 1, 0);
    assert!(!narrow.disclosure.controls_whole, "a half-shown grant read as disclosed");
}

#[test]
fn an_ellipsised_summary_is_not_a_disclosed_subject() {
    let mut asked = request("edit_file");
    asked.summary = "write_file wants to replace ".to_string() + &"deep/".repeat(60) + "notes.md";
    let panel = Panel::new(asked);
    assert!(!panel.compose(80, 24, 1, 0).disclosure.subject_whole);
}
```

Two new helpers go beside those fixtures. `ControlChannel::new` takes only the receiver (`approval.rs:519-527`) and the type has **no sender accessor** — do not add one; a production API existing solely for tests is exactly the wrong direction. The test keeps the sending half itself:

```rust
/// A prompter, the events it sends, the channel it waits on, and the sending
/// half of that channel -- which the test keeps because `ControlChannel` owns
/// only the receiver.
fn a_prompter() -> (
    TuiPrompter,
    mpsc::Receiver<UiEvent>,
    Arc<ControlChannel>,
    mpsc::UnboundedSender<TurnControl>,
) {
    let (events, incoming) = mpsc::channel(8);
    let (answers, replies) = mpsc::unbounded_channel();
    let control = ControlChannel::new(replies);
    let prompter = TuiPrompter::new(events, Arc::clone(&control), Cancellation::new(crate::gateway::CancelToken::new()));
    (prompter, incoming, control, answers)
}

/// The id of the next question on the wire. **Receives** rather than peeks, so
/// two calls read the first and second questions instead of the same one twice.
fn asked_id(events: &mut mpsc::Receiver<UiEvent>) -> ApprovalId {
    match events.try_recv().expect("a question was sent") {
        UiEvent::Approval(asked) => asked.id,
        other => panic!("not a question: {other:?}"),
    }
}
```

`mpsc` is already imported in that test module (`approval.rs:741`). Build the `Cancellation` the way the module's existing prompter cases do rather than assuming its constructor.

In `src/tui/approval_screen.rs` tests, alongside the existing `asked_about(before, after)` fixture:

```rust
#[test]
fn a_truncated_subject_is_not_disclosed_however_wide_the_screen_is() {
    let mut request = asked_about("one", "two");
    request.target = "a/".repeat(120) + "notes.txt";
    let composed = ApprovalScreen::new(request).composition(80, 24);
    assert!(!composed.disclosure.subject_whole, "SUBJECT_ROWS silently dropped the tail");
    assert!(composed.disclosure.controls_whole, "the choices are still whole");
}

#[test]
fn a_screen_too_short_for_the_heading_discloses_no_subject() {
    // Six rows: the choices block takes what it may never lose
    // (`approval_screen.rs:371-373`) and `heading.truncate(room)` at `:374-375`
    // leaves room only for the title, so the subject rows never reach the
    // screen at all.
    let composed = ApprovalScreen::new(asked_about("one", "two")).composition(80, 6);
    assert_eq!(composed.rows.len(), 6, "the composition still fills the plane");
    assert!(!composed.disclosure.subject_whole, "the heading was truncated into the choices' room");
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test --locked --lib tui::approval`
Expected: FAIL on the id and disclosure assertions, not on unresolved names.

- [ ] **Step 4: Implement**

Reshape the two enum variants. Give `TuiPrompter` an `Arc<AtomicU64>` **created in `TuiPrompter::new` (`approval.rs:640-650`)** — one counter per prompter, cloned with the prompter, so every clone mints from it — and a `mint` that returns `Err(nobody_to_ask())` when `checked_add` is exhausted, as `question.rs:731-745` does.

Match arms in the wait loop, in this order: `Some(TurnControl::Answer { id: answered, answer }) if answered == id => return Ok(answer)`, then `Some(TurnControl::Answer { .. }) => continue`, then the existing question arms, and only then the `Some(stop) => { give_back; Deny }` catch-all at `approval.rs:715-718`. **The stale-answer arm must sit above that catch-all**: below it, a stale `Answer` would fall into `stop`, be pushed back onto the channel and refuse the live question — the opposite of consuming it.

Change `fitted` to return `Fitted`. Add `Panel::compose` and `ApprovalScreen::composition`, leaving `Panel::caret_row` and `presents_choices` untouched. `Shell::ask` stores the id beside the installed surface and sends `Answer { id, answer: Deny }` on the too-small refusal (`shell.rs:1220-1227`); `Shell::decide` sends `Answer { id, answer }` (`shell.rs:1532`).

- [ ] **Step 5: Run the whole build**

Run: `cargo test --locked --all-targets` — PASS, no non-exhaustive-match or unreachable-pattern warnings. Then `cargo fmt --check`. **No clippy run here either**: `Disclosure` is produced but not yet read, and Task 3 is what reads it. Carry straight on to Task 3 in the same tree.

---

## Task 3: The gate, the frame receipts, and terminal evidence

**Files:** `src/tui/shell.rs`, `src/tui/event_loop.rs:889-923`, `:1013-1112`, `:619-622`, `scripts/smoke-tui.sh`

**Interfaces produced on `Shell`:** `intend_approval(&mut self)`, `approval_landed(&mut self, Outcome)`, `approval_write_failed(&mut self)`, `invalidate_approval(&mut self)`, `reconcile_approval(&mut self)`, `approval_ready(&self, ApprovalId) -> bool`; plus

```rust
pub(crate) const APPROVAL_NOT_READY: &str =
    "the question is not on the screen yet - press it again once you can read it";
pub(crate) const APPROVAL_NOT_DISCLOSED: &str =
    "this screen cannot show the whole request, so it cannot be approved here; \
     make the window larger, or press 3 or esc to refuse";
```

**Wiring, exact:**

1. `commit_band` (`event_loop.rs:889-923`): bind `let rows = shell.band_rows();` **once**, call `shell.intend_approval()` before `band.commit`, pass `&rows`. Replace `Ok(_)` with `Ok(Commit::Painted) => shell.approval_landed(Outcome::Painted)` and `Ok(Commit::NoChange) => shell.approval_landed(Outcome::Unchanged)`, both keeping `failures.succeeded()`. The `Err` arm calls `shell.approval_write_failed()` before `shell.render.restore(attempt)`. The `attempt.damaged()` branch calls `shell.invalidate_approval()` beside `band.invalidate`.
2. `paint_alternate` `(false, Approval)` (`:1048-1076`): `intend_approval()` before the write; `approval_landed(Outcome::Painted)` after `band.frame_landed`; `approval_write_failed()` in the `Err` arm.
3. `paint_alternate` `(true, Approval)` (`:1078-1108`): same intent; the empty-bytes early return at `:1090-1093` calls `approval_landed(Outcome::Unchanged)`; the `Ok` arm `Painted`; the `Err` arm `approval_write_failed()`.
4. `(true, Primary)` restore (`:1027-1045`) needs nothing: `Shell::decide` and `release_screen` take the surface down and both call `invalidate_approval`.
5. Add `shell.reconcile_approval();` immediately after `resolve_resize(shell, band, now, ...)` at `event_loop.rs:622`. That is the reconciliation point named in Task 1 clause 4: `signals::take_winch()` is drained at `:619` on the tick after the write, so a signal that landed inside the write is visible here and refuses the promotion.
6. `intend_approval` reads the installed surface and its id, calls `Panel::compose(cols, rows, self.geometry.band_top(), u16::from(self.geometry.activity.is_some()))` -- the activity row is the only thing `band_rows` pushes ahead of the panel (`shell.rs:664-677`), so the offset is that one boolean and needs no new method -- or `ApprovalScreen::composition(cols, rows)`, and passes `Intent::capture(..)` — an `Option` — straight to `readiness.intend`. With no question up it calls `intend(None)`.
7. In `decide` (`shell.rs:1459-1541`), after `answered` yields `Some(answer)` and **before** the surfaces are cleared: when the answer is `Once` or `Always` and `!self.approval_ready(id)`, call `self.say(APPROVAL_NOT_DISCLOSED.to_string())` when `readiness.undisclosed()` and `self.say(APPROVAL_NOT_READY.to_string())` otherwise -- `say` takes an owned `String` (`shell.rs:1693-1695`) -- then request `Reason::Modal` and `return`. The question stays up, the keystroke is spent, nothing is queued for a later grant. `Deny` and the `Cancel` interrupt skip the gate.

- [ ] **Step 1: Write the failing shell tests**

Use the module's real fixture — `shell(rows, cols)` at `shell.rs:3329`, which derefs to `Shell`; `controlled()` at `:3239` reads the control channel; `released()` at `:3187` is the document.

```rust
#[test]
fn an_affirmative_before_the_frame_is_refused_and_not_replayed() {
    let mut shell = shell(24, 80);
    shell.apply(UiEvent::Approval(ApprovalAsked { id: ApprovalId(1), request: asked() }));
    shell.decide(Input::Text('1'), Instant::now());
    assert_eq!(shell.controlled(), None, "an unread question was answered");
    assert!(shell.released().iter().any(|row| row.contains(APPROVAL_NOT_READY)));

    shell.intend_approval();
    shell.approval_landed(Outcome::Painted);
    shell.reconcile_approval();
    assert_eq!(shell.controlled(), None, "the early key was consumed, not queued");

    shell.decide(Input::Text('1'), Instant::now());
    assert_eq!(
        shell.controlled(),
        Some(TurnControl::Answer { id: ApprovalId(1), answer: ApprovalAnswer::Once })
    );
}

#[test]
fn refusing_is_always_possible_before_any_frame_lands() {
    for key in [Input::Text('3'), Input::Action(Action::Escape)] {
        let mut shell = shell(24, 80);
        shell.apply(UiEvent::Approval(ApprovalAsked { id: ApprovalId(1), request: asked() }));
        shell.decide(key, Instant::now());
        assert_eq!(
            shell.controlled(),
            Some(TurnControl::Answer { id: ApprovalId(1), answer: ApprovalAnswer::Deny })
        );
    }
    let mut shell = shell(24, 80);
    shell.apply(UiEvent::Approval(ApprovalAsked { id: ApprovalId(1), request: asked() }));
    shell.decide(Input::Action(Action::Cancel), Instant::now());
    assert!(matches!(shell.controlled(), Some(TurnControl::Cancel { .. })));
}

#[test]
fn a_screen_that_cannot_disclose_the_request_says_so_and_still_refuses() {
    // Narrow enough that the second label cannot be shown whole, so no frame
    // on this screen can ever grant.
    let mut shell = shell(24, 30);
    shell.apply(UiEvent::Approval(ApprovalAsked { id: ApprovalId(1), request: asked() }));
    shell.intend_approval();
    shell.approval_landed(Outcome::Painted);
    shell.reconcile_approval();
    shell.decide(Input::Text('1'), Instant::now());
    assert_eq!(shell.controlled(), None);
    assert!(shell.released().iter().any(|row| row.contains(APPROVAL_NOT_DISCLOSED)));

    shell.decide(Input::Text('3'), Instant::now());
    assert_eq!(
        shell.controlled(),
        Some(TurnControl::Answer { id: ApprovalId(1), answer: ApprovalAnswer::Deny }),
        "a question that cannot be granted must still be refusable"
    );
}
```

Repeat the first case for the alternate surface, built with the module's own `asked_about_a_large_change()` (`shell.rs:5590-5597`), whose 4 KiB diff is what routes it to the alternate plane (`ApprovalSurface::for_request`, `approval.rs:193-202`); drive `screen_rows()` rather than `band_rows()`.

- [ ] **Step 2: Write the failing event-loop tests — the causal boundary**

In `src/tui/event_loop.rs` tests, using the fixture `shell()` at `:1246` and the existing writer doubles `FlakyScreen { refusals, kind, written }` at `:1199-1218` and `BrokenScreen` at `:1180-1196`. That test module has no approval fixture of its own, so add this one to it — it does not import the shell module's, which is private to those tests:

```rust
/// A question the band can ask on its own rows: no diff, so
/// `ApprovalSurface::for_request` keeps it inline (`approval.rs:193-202`).
fn a_question() -> crate::permission::ApprovalRequest {
    crate::permission::ApprovalRequest {
        tool: "edit_file",
        target: "notes.txt".to_string(),
        summary: "edit `notes.txt`: replace \"alpha\" with \"beta\"".to_string(),
        always_scope: "allow every future edit_file to `notes.txt` for the rest of this session"
            .to_string(),
        diff: None,
    }
}
```

```rust
#[test]
fn a_refused_frame_grants_no_readiness_and_the_next_one_does() {
    let mut fixture = shell();
    fixture.shell.apply(UiEvent::Approval(ApprovalAsked {
        id: ApprovalId(1),
        request: a_question(),
    }));
    let mut band = Band::new();
    let mut failures = FrameFailures::default();
    let mut screen =
        FlakyScreen { refusals: 1, kind: io::ErrorKind::WouldBlock, written: Vec::new() };

    commit_band(&mut fixture.shell, &mut band, &mut screen, &mut failures, Instant::now())
        .expect("a refused frame is counted, not fatal");
    fixture.shell.reconcile_approval();
    assert!(
        !fixture.shell.approval_ready(ApprovalId(1)),
        "bytes the screen refused granted a receipt"
    );

    commit_band(&mut fixture.shell, &mut band, &mut screen, &mut failures, Instant::now())
        .expect("the second frame lands");
    fixture.shell.reconcile_approval();
    assert!(fixture.shell.approval_ready(ApprovalId(1)));
    assert!(!screen.written.is_empty(), "nothing was ever written, so this proves nothing");
}

#[test]
fn a_winch_between_the_write_and_the_reconcile_revokes_the_frame() {
    let mut fixture = shell();
    fixture.shell.apply(UiEvent::Approval(ApprovalAsked {
        id: ApprovalId(1),
        request: a_question(),
    }));
    let mut band = Band::new();
    let mut failures = FrameFailures::default();
    let mut screen =
        FlakyScreen { refusals: 0, kind: io::ErrorKind::WouldBlock, written: Vec::new() };

    commit_band(&mut fixture.shell, &mut band, &mut screen, &mut failures, Instant::now())
        .expect("the frame lands");
    // What `event_loop.rs:619-620` does with a signal drained after the write.
    fixture.shell.render.mark_resize(Instant::now());
    fixture.shell.reconcile_approval();
    assert!(!fixture.shell.approval_ready(ApprovalId(1)));
}
```

- [ ] **Step 3: Run both test sets to verify they fail**

Run: `cargo test --locked --lib tui::shell tui::event_loop`
Expected: FAIL — today the early `1` answers, and a refused frame leaves no receipt to check because no receipt exists at all.

- [ ] **Step 4: Implement the wiring and the gate, then run the first full lint**

```bash
cargo test --locked --all-targets
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo clippy --locked --all-targets --features fault-injection -- -D warnings
```

Expected: tests PASS with zero failures; both clippy modes exit 0. **This is the run the two earlier tasks deferred** — every `pub(crate)` item added in Tasks 1 and 2 now has a caller, so a dead-code warning here is a real one: something the plan said would be wired was not. Find the unwired caller rather than silencing the lint.

- [ ] **Step 5: Add release scenario 24**

Add `scenario_24` to `scripts/smoke-tui.sh` beside `scenario_23b` (`:6105`), register `"24-approval-readiness": scenario_24,` in `SCENARIOS` (`:6243-6270`), and add the same name to the shell script's own list beside `23b-question-cancelled` (`:6364`) — the runner compares the two.

```python
def scenario_24(run):
    """A committed frame is what makes an affirmative answerable, and a resize
    takes that back until the next one lands.

    Synchronized on the **committed** grid throughout (`on_the_grid`), which is
    what makes this deterministic without a sleep: it feeds the emulator only as
    far as the last complete frame, so an assertion after it is an assertion
    about a screen the terminal really showed.
    """
    marker = run.marker("readiness")
    fixture = start_fixture(run, fixtures.edit_then_finish(marker), name="readiness")
    trial = run.trial("readiness", gateway=fixture, mode="ask", notes=True).settled()
    trial.send("edit the notes " + run.nonce + "\r")

    # Positive control: the question is on the screen, whole, and `1` is taken.
    grid = on_the_grid(trial, PERMISSION_TITLE, "the approval frame to be committed")
    run.require(grid.find(ALWAYS_WORDING) is not None, "the always-scope is disclosed whole")
    run.require(grid.find("3. No") is not None, "the refusal is on the screen")
    trial.grid("disclosed")
    trial.send(b"1")
    on_the_grid(trial, marker, "the turn to carry on past the granted call")
    run.require(
        carried_results(fixture, "call-0") != [],
        "the answered call produced no tool result, so the positive control proves nothing",
    )

    # A resize revokes the receipt: the same key is refused until a frame for
    # the new screen has landed.
    second = start_fixture(run, fixtures.edit_then_finish(marker), name="resized")
    resized = run.trial("resized", gateway=second, mode="ask", notes=True).settled()
    resized.send("edit the notes " + run.nonce + "\r")
    on_the_grid(resized, PERMISSION_TITLE, "the approval frame before the resize")
    resized.resize(30, 100)
    resized.send(b"1")
    grid = on_the_grid(resized, APPROVAL_NOT_READY, "the refusal of an answer to a moved screen")
    run.require(
        grid.find(PERMISSION_TITLE) is not None,
        "the refused answer took the question down; a refusal must leave it standing",
    )
    resized.grid("revoked")
    on_the_grid(resized, ALWAYS_WORDING, "the repaint on the new screen")
    resized.send(b"1")
    on_the_grid(resized, marker, "the turn to carry on once the frame had landed")
    second.stop()
    fixture.stop()
```

`APPROVAL_NOT_READY` is a literal in this script, beside the other product wordings it pins; keep it byte-identical to the Rust constant. `trial.resize(rows, cols)` is the harness's own helper (`scripts/smoke-tui.sh:2203`, used as `trial.resize(30, 120)` at `:4194`), so `resized.resize(30, 100)` is thirty rows by a hundred columns.

**What this scenario does and does not prove.** It proves, on a real terminal against a release binary, that a disclosed committed frame makes `1` answerable, and that a resize revokes that until the next frame lands. It does **not** prove the pre-commit case: `on_the_grid` can only wait for something already on the screen, so there is no way to schedule a keystroke strictly between "the request was installed" and "its first frame was committed" without a purpose-built seam — and the tool call is produced asynchronously, so a byte sent with the prompt can reach the composer before the `UiEvent` ever arrives and make the scenario pass for the wrong reason. That boundary is proven instead by the deterministic `commit_band` tests in Step 2, where the refused write is the cause and the receipt is read directly. Recorded as a limitation rather than papered over with timing.

- [ ] **Step 6: Run the release gates**

```bash
cargo build --locked --release
cargo build --locked --release --features fault-injection
scripts/smoke.sh
scripts/smoke-tui.sh
scripts/smoke-tui.sh 24-approval-readiness
```

Expected: every scenario passes; the printed scenario count is 27, and the check total rises by scenario 24's own `run.require` calls. Record the evidence directory the script prints.

- [ ] **Step 7: Hand back**

Report the gate outputs, the scenario count, the evidence directory and the grids named `disclosed` and `revoked`. Do not stage, commit or push; the controller commits after external review.

---

## Self-Review

**Spec coverage of `loop.md:68`.** Settled resize → clause 4's reconcile plus `ready`'s dimension check. Nonzero dimensions → `capture`'s guard. Same request ID → `ApprovalId` minted per `request()` call. File identity and all decision controls visible → `subject_whole`, `controls_whole`, `scope_whole`, each computed from source text against its allotment. Changed content or its notice, now or previously observed → `change_visible` plus `seen`. "A successful write by itself is insufficient" → `capture` returning `None`, and the provisional-until-reconciled promotion. `loop.md:55`'s `NoChange` ruling → clauses 2, 3 and 8 plus wiring item 1.

**Placeholder scan.** Every step names its file, command and expected result. The only named-but-unread helper is `trial.resize`, and Step 5 says to read the existing resize scenario rather than invent one.

**Type consistency.** `ApprovalId`, `Surface`, `Disclosure`, `Intent`, `Outcome`, `Readiness`, `Fitted`, `Composition`, `ApprovalAsked`, `APPROVAL_NOT_READY` and `APPROVAL_NOT_DISCLOSED` are each defined once and used under the same name throughout. `Composed` keeps `caret` and `viewport` private and gains `disclosure`; `Composition` is a projection of it, never a second layout. Every test helper is either the module's own or defined here: `shell(rows, cols)` (`shell.rs:3329`), `asked()` (`:5577`), `asked_about_a_large_change()` (`:5590`), `asking()` (`:5600`), `controlled()` (`:3239`), `released()` (`:3187`); `shell()` (`event_loop.rs:1246`), `FlakyScreen`/`BrokenScreen` (`:1180-1218`) and the `a_question()` written out in Task 3 Step 2; `request(tool)` (`approval.rs:743`), `with_diff(bytes)` (`:755`) and the three new prompter helpers spelled out in Task 2 Step 2; `asked_about` (`approval_screen.rs:406`); `on_the_grid`/`run.require`/`run.marker`/`start_fixture`/`carried_results` (`scripts/smoke-tui.sh`).
