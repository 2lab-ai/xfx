# TUI Phase 3 — delta edit history and kill ring (P3-EDIT, one slice)

Worktree base: `98a7324` (`fix: adopt alternate surface only after frame delivery`), branch
`plan/tui-phase3`. Requirements: `.prd/tui-phase3/ssot.md` row P3-EDIT, `.prd/tui-phase3/loop.md`
§Verified upstream contract excerpts (first bullet).

Upstream: `https://github.com/vercel-labs/fx`, port pin
`580a0c5da9386317251968c09c1cee69e763487a`, research revision
`ef1d0d0c6a1a87a621fc54d23f86ffec51755779`. Upstream `file:line` citations were read at that pin
(HTTPS, then the controller's pinned archive) and are settled — §Upstream rulings. Revision 4 of
this plan answers external review M1–M6, S1–S4 and the rev-3 scoped pass N1–N4 + C1; §Review
dispositions records where.

## For agentic workers

Planning output only. No task starts without a dispatcher hand-off naming the task ID and owned
files. Undo is `0x1f`, yank is `0x19` (`shortcuts.zig:23-24,84-85`). **Redo has no control byte and
none may be invented** — only the two encoding families in §S5. Ctrl-Z (`0x1a`) is not undo:
upstream's control table has no arm for it and `input.rs:644-664` leaves it `Action::Ignore`.

Delivery: ordinary local commits and a PR on this branch are within the user's standing
authorisation (`rules/DEV.md` §4) once the gate is green and an external reviewer has passed the
change; a writer commits only under its own dispatch. User-gated: `v*` tags, stable release,
production deploy, data deletion.

## Goal

Ladder item 18: a user types, deletes, kills, pastes, yanks, undoes and redoes in the composer, and
the draft — text, entity blocks and caret — returns exactly to the state it had, under a history
that cannot grow without bound.

Today the composer has **no history**. `src/tui/transaction.rs` is an explicit one-entry seam:
`InsertPaste` keeps `before`/`after` as **whole draft copies** (`transaction.rs:44-49`) and its only
reader is `#[cfg(test)]` (`transaction.rs:80`). `Editor::apply` (`editor.rs:251-308`) mutates with
no record. `Shell::edited` (`shell.rs:2589-2592`) is the single funnel every text change passes and
no caret move does (`shell.rs:2570-2573`, `:1942-1948`, `:1911`).

Done: undo and redo hold at most 100 entries and 1 048 576 retained bytes **together**; a delta over
that byte cap resets both stacks instead of being stored; a recorded edit clears redo; eviction
drops the oldest undo entry first; kills (`C-k`, `C-u`, `C-w`) fill a single-slot kill ring that
`C-y` yanks — and the yank is itself undoable — while `Backspace`/`Delete` leave the slot untouched;
every whole-draft disposal (submit, `/clear`, recall, slash completion) is a **boundary**, not a
delta; a paste is one transaction and its payload and id survive undo and redo when its delta fits
the history budget. An oversized paste is a boundary (§S4). No whole-draft copy per keystroke, and
no undo that can panic or restore bytes into a draft that no longer exists.

## Upstream rulings (settled; do not re-research)

1. **Redo binding.** `escape_parser.zig:122-123`: keycode `z`/`Z` with the super bit set is `.undo`,
   or `.redo` when shift is also set; `super_modifier = 0x08` (`escape_parser.zig:42`). Wire
   spellings are pinned by `runtime.zig:3018` `"[122;10u"` and `runtime.zig:3025` `"[27;10;122~"`,
   both `=> .redo`. The modifier parameter is xterm's `1 + mask`, so `10` is mask `9` = super `0x08`
   | shift `0x01`, and Super+Z alone is `9`. `shortcuts.zig::fromControlByte` (`:11-30`) has no redo
   arm — redo is a CSI key, not a control byte.
2. **Consecutive kills do not coalesce.** `kill_ring.zig:86-116`: `deleteRange` builds a fresh
   payload with `capturePayload`, then `var previous = self.*; self.* = next; previous.deinit(alloc)`
   — replace and free, no append arm.
3. **A completed prompt-recall move is a history boundary, not a delta.**
   `input_completion_runtime.zig:339-356`: `.moved => app.input_runtime.historyBoundary(app.alloc)`,
   `.unchanged => {}`. The controller's check places that site on `navigatePromptHistory`, so this
   ruling covers **recall only** — it does not speak to slash completion (§S2 takes that as an xfx
   default) — and it independently confirms the no-op rule.

Ruling (1) is the **actual, promoted** CSI-u requirement for P3-KEYS: two named families carrying
one product action. It is not a licence to claim a full CSI-u matrix.

## Architecture

One new focused module plus narrow seam edits. No Phase-3 umbrella framework.

| File | Role |
|---|---|
| `src/tui/edit_history.rs` (new) | `Delta`, `DeltaKind`, `EditHistory`, caps, eviction, boundary, kill slot, pure-logic tests |
| `src/tui/transaction.rs` | deleted; its paste-boundary rule and **its tests** move to `DeltaKind::Paste` |
| `src/tui/editor.rs` | text mutators return `Option<Delta>`; `revert`/`replay` |
| `src/tui/input.rs` | `Action::Undo`, `Action::Redo`, `Action::Yank`; `control(0x1f)`, `control(0x19)`; `csi` gains the two §S5 families |
| `src/tui/shell.rs` | `edit_history: EditHistory` field, `amended(Option<Delta>)` → `edited(Option<Delta>)`, boundary at `take_draft`, two new `act` arms |
| `src/tui/mod.rs` | `mod edit_history;` |
| `tests/tui.rs`, `scripts/smoke-tui.sh` | PTY and release-binary scenarios |

**The field is `edit_history`, never `history`.** `Shell` already has `history: History`
(`shell.rs:348`, built at `:584`, walked at `:2170`, `:2546`, and left by `amended` at `:2571`) —
that is the **prompt-recall** history and it is a different thing. The new field is
`edit_history: EditHistory`. Every snippet, task and test below uses `edit_history`; a binding named
`history` in this slice would silently resolve to prompt recall, which has no `undo`.

**Ownership of the transfer.** `Shell` owns `editor` and `edit_history` as disjoint fields, so the
compiler does permit `let delta = self.edit_history.pop_undo(); self.editor.revert(&delta);` after
the fields are split. An earlier revision claimed otherwise; that claim was wrong and is withdrawn.
The reason to encapsulate the transfer is not borrowck but **atomicity**: a caller that pops an
entry and forgets to stash it silently destroys the redo path. So `EditHistory` exposes

```rust
pub(crate) fn undo(&mut self, apply: impl FnOnce(&Delta)) -> bool;
pub(crate) fn redo(&mut self, apply: impl FnOnce(&Delta)) -> bool;
```

which pop, call `apply`, push onto the other stack, and return whether anything was there. The shell
splits its own fields at the call site — `let Shell { edit_history, editor, .. } = self;` then
`edit_history.undo(|delta| editor.revert(delta))` — which is two disjoint field borrows and
compiles. There is no public pop, so the two stacks cannot drift.

`Delta`'s fields are private to `edit_history`; `editor.rs` is a sibling module, so it gets a
`pub(crate) fn Delta::new(..)` constructor (§S1) and is the **only** producer. Readers use
`pub(crate)` accessors, preserving the no-arithmetic-on-our-offsets rule `entity.rs:113-116` states.

## Tech Stack

Rust 2021; std plus the crate's existing `unicode-segmentation` (`editor.rs:470-476`) and
`std::sync::Arc` (paste payloads are `Arc<str>`, `entity.rs:91-96`). No new dependency. Unit tests
in `edit_history.rs::tests`, `editor.rs::tests`, `input.rs::tests` and `shell.rs::tests`; PTY tests
in `tests/tui.rs`.

## Spec

### S1 `Delta`

```rust
#[derive(Debug, Clone)]
pub(crate) struct Delta { /* all fields private to this module */ }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeltaKind { Ordinary, Kill, Paste }

impl Delta {
    /// The editor is the only producer: it is the module that owns the offsets.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        at: usize,
        removed: String,
        inserted: String,
        removed_entities: Vec<Span>,
        inserted_entities: Vec<Span>,
        caret_before: usize,
        caret_after: usize,
        kind: DeltaKind,
    ) -> Self;

    // Read side. `editor.rs` is a sibling module, so private fields are unreachable there:
    // `revert`/`replay` need all four of the first group to `replace_range` and restore spans.
    pub(crate) fn at(&self) -> usize;
    pub(crate) fn removed(&self) -> &str;
    pub(crate) fn inserted(&self) -> &str;
    pub(crate) fn removed_entities(&self) -> &[Span];
    pub(crate) fn inserted_entities(&self) -> &[Span];
    pub(crate) fn caret_before(&self) -> usize;
    pub(crate) fn caret_after(&self) -> usize;
    pub(crate) fn kind(&self) -> DeltaKind;
    pub(crate) fn weight(&self) -> usize;
}
```

`Span` is `Clone` and carries the id (`entity.rs:100-105`, `EntityKind::Paste { id, text: Arc<str>,
lines }` at `:86-96`). `EntitySnapshot` is unusable here — no id field (`entity.rs:468-484`) — and
`Entities::register` `debug_assert!`s id uniqueness (`entity.rs:212-216`). Storing `Span` preserves
the id an undo must put back; the clone is cheap because the payload is the `Arc`.

`at` is the **widened** start: `Editor::delete` replaces `entities.delete_touching(start..end)`
rather than the requested range (`editor.rs:374-376`), so the delta records `taken`. Applying a
delta is a `replace_range` on a boundary the editor produced, never a re-derived one.

Upstream's retained size is `removed.len() +| inserted.len()` exactly (`edit_history.zig:86`),
because upstream stores raw bytes. xfx also holds `Arc<str>` payloads, so `weight()` =
`removed.len() + inserted.len()` **plus**, per span in either vector, `span.text().len() +
size_of::<Span>()`. Over-counts a shared `Arc` — deliberately the safe direction. This is
`loop.md:67`'s requirement and an xfx-local bound, **not** upstream parity.

### S2 `EditHistory`

```rust
const MAX_ENTRIES: usize = 100;            // edit_history.zig:5
const MAX_RETAINED_BYTES: usize = 1 << 20; // edit_history.zig:6  (1024 * 1024)
const MAX_KILL_BYTES: usize = 1 << 20;     // xfx-local, separate budget -- S3
```

- `record(delta)`: `if delta.weight() > MAX_RETAINED_BYTES { self.boundary(); return; }` — upstream
  `prepare` returns `.boundary` above the cap (`edit_history.zig:86-87`) and `commit` maps
  `.boundary` to `reset`, clearing **both** stacks (`:99-101`, `:71-75`). Otherwise: clear redo, add
  the weight, push onto undo, then evict from the **front** while `undo.len() > MAX_ENTRIES ||
  retained > MAX_RETAINED_BYTES` (`:102-114`, `orderedRemove(0)`).
- **One accounting rule, and only one.** `retained` is changed by exactly four operations:
  `record` **adds** the new delta's weight **and subtracts the weight of every redo entry it
  clears**; eviction subtracts; `boundary` zeroes. Upstream is the same shape — `clearStack`
  subtracts as it frees, which is why `reset` can assert `retained_bytes == 0`
  (`edit_history.zig:71-75`). Missing the subtraction is not cosmetic: undo fifty entries, then type
  one character, and fifty entries' worth of bytes stay counted forever, evicting live undo entries
  early and making `retained()` unable to return to 0 except through a boundary.
  An undo↔redo **transfer** is the one thing that changes nothing: it moves an entry between two
  stacks whose bytes are counted **together**, so no pop-subtract, no stash-add, no underflow, no
  leak. Covered by `recording_after_an_undo_reclaims_the_cleared_redo_bytes` and
  `an_undo_redo_walk_leaves_the_retained_total_alone`.
- `boundary()` clears both stacks and zeroes `retained`. It is the operation an oversized delta
  takes and the operation every whole-draft disposal takes. It does **not** touch the kill slot:
  upstream's kill ring is separate state and only `resetForSession` empties it
  (`kill_ring.zig:71-84`).
- **Every whole-draft disposal is a boundary.** The single funnel is `Shell::take_draft`
  (`shell.rs:2114-2118`), which calls `Editor::take` (`editor.rs:311-318`) — that wipes the text,
  clears the entities and zeroes the caret. Every delta on the stack holds absolute offsets into the
  discarded draft, so a later `replace_range` would index past an empty `String` (panic) or write
  into an unrelated draft. `take_draft` therefore calls `self.edit_history.boundary()`, and its own
  doc comment already claims to be the point "every other thing that empties the composer" passes
  (`shell.rs:2110-2113`). **Every caller, present and future, inherits it** and needs no boundary of
  its own — six today (`shell.rs:1296`, `:2130`, `:2188`, `:2207`, `:2263`, `:2313`), and
  `Editor::take` has no other caller, so the funnel is the whole surface. Do not maintain the list.
- **Preserve both edit routes.** `fn amended(&mut self, delta: Option<Delta>)` calls
  `self.history.leave(); self.edited(delta);`: ordinary text edits and yank use this route to end
  prompt recall. Existing direct `edited()` callers at base `shell.rs:2189`, `:2208`, `:2264`,
  `:2314`, `:2555` become `edited(None)`, never `amended(None)`. In particular recall must keep its
  navigation walk active. Update all existing `amended` call sites mechanically rather than relying
  on a hand-maintained count. Completion and clear use `amended(None)` after their boundary;
  successful yank uses `amended(Some(delta))`. `Editor::apply(&mut self, action: Action, cols: u16)`
  changes from `()` to `Option<Delta>` so the keystroke mutation reaches this recording route.
  Add a shell regression driving three consecutive prompt-history recalls to prove the direct
  `edited(None)` route does not terminate navigation after the first step.
- **Slash completion records nothing — an xfx default, not an upstream ruling.**
  `Shell::complete` (`shell.rs:1295-1307`) is `take_draft()`, then
  `let _ = self.editor.insert(&picker::completed(name))`, then `amended(None)`. The whole draft was
  the query (`picker::Trigger::of`), so the insert is the second half of a **replacement**, not an
  insertion into what the user had. Recording only the insert would give an undo that deletes the
  command name and leaves the composer empty, silently discarding the query. Upstream ruling (3) is
  about prompt recall and is **not** evidence for this call site (C1). The boundary is instead
  forced structurally: `complete` goes through `take_draft` (`shell.rs:1296`), where every stacked
  delta's offsets die anyway. Undo does not step back across a completion, and no insert-only delta
  pretends it does. `clear_composer` (`shell.rs:2126-2132`) likewise ends in `amended(None)`.
  Recall (`Editor::set_text`, `editor.rs:225-228`) takes the boundary contract too, and there
  ruling (3) *is* the evidence.
- **The no-op guard lives in the caller.** Upstream `prepare` has no empty-delta arm
  (`edit_history.zig:85-95`) and its caller does nothing on `.unchanged`. In xfx the mutators return
  `Option<Delta>` and yield `None` at `Editor::delete`'s `start >= end` early return
  (`editor.rs:366-368`, the same guard as `kill_ring.zig:93`) and at the byte-budget refusals in
  `insert` (`editor.rs:160-163`), `insert_entity` (`:191-196`) and `set_text` (`:225-228`).
  `amended(None)` records nothing, so **a refused or empty mutation never clears redo**. `record`
  debug-asserts a delta is not empty on both sides.

### S3 kill ring — one slot, separately bounded

`KillToEnd`, `KillToStart`, `DeleteWordLeft` produce `DeltaKind::Kill`; `Backspace` and `Delete`
produce `DeltaKind::Ordinary`. The slot is its own field with its own budget:

```rust
struct Killed { text: String, entities: Vec<Span> }
// `killed: Option<Killed>`; its bytes are NOT part of `retained`.
```

Separate on purpose: upstream's kill ring is a distinct `State` (`kill_ring.zig:45-50`) independent
of `edit_history.State` (`edit_history.zig:59-61`). Folding killed text into the history budget
would let a large kill evict undo entries unrelated to it. `record` overwrites the slot **only** for
`Kill`, and overwriting replaces it whole (`kill_ring.zig:113-115` swaps and deinits the previous).
A kill whose weight exceeds `MAX_KILL_BYTES` clears the slot rather than truncating it. Truncation
would yank a broken summary marker; retaining the previous slot would yank unrelated older content,
not recover the oversized new kill. The new kill is not recoverable through this bounded slot.

**A yank is an edit.** `Action::Yank` inserts the slot's text at the caret, re-registering its
entities with **fresh ids** from the shell's id counter (forced by `entity.rs:212-216`: the yanked
block coexists with the original if the original is still in the draft) and cloning the `Arc<str>`
payload. The resulting delta is `DeltaKind::Ordinary` — ordinary so that `record` does not overwrite
the kill slot the yank just read — and it goes through `amended(Some(delta))`, not `edited`, so the
yank is undoable on its own **and** ends the prompt-recall walk like every other text change
(`shell.rs:2565-2573`). Undo of the *kill* restores the original ids, because it reverts to a draft
state in which those ids were live.

### S4 paste stays one transaction

`Editor::insert_entity` becomes `-> Option<Delta>` (today `Option<Span>`, `editor.rs:191-203`,
consumed at `shell.rs:1900-1905`); the span the caller still needs is read back from
`delta.inserted_entities()[0]`. The paste path records one `DeltaKind::Paste` at the moment the
framed paste lands — the same knowable point `transaction.rs:9-11` fixes today — with `inserted` =
the summary marker and `inserted_entities` = the one span. Never per read-chunk, never per grapheme.
Its weight includes the payload, so a paste over 1 MiB is a boundary by §S2: a history that cannot
hold the paste must not claim it can undo it.

`EditTransaction::InsertPaste { before, after, entity }` (`shell.rs:1911`) and the whole of
`transaction.rs` are deleted, but **their proof is migrated, not dropped**: every test that reads
`LastTransaction::last()` is rewritten against the recorded `Delta`. Verified members of that set
include `one_framed_paste_is_one_transaction_however_many_reads_it_arrived_in` (`shell.rs:3965`),
`a_caret_move_leaves_the_paste_boundary_standing_and_an_edit_takes_it_down` (`:4013`) and
`every_kind_of_edit_takes_the_paste_boundary_down` (`:4074`). T3 closes the set mechanically:
`rg 'LastTransaction|EditTransaction' src/ tests/` must return nothing but the deletions, and the
migrated cases must still assert one delta per framed paste across multi-read arrivals.

### S5 bindings

`input::control` (`input.rs:644-664`) gains exactly two arms: `0x1f => Action::Undo`,
`0x19 => Action::Yank`. **No redo arm.**

`input::csi` (`input.rs:684-716`) gains the two families ruling (1) pins, and only those:

- `CSI <code> ; <modifier> u` — pinned vector `[122;10u`. A new final byte for this decoder.
- `CSI 27 ; <modifier> ; <code> ~` — pinned vector `[27;10;122~`. A new **params shape under an
  existing final byte**: `b'~'` matches five literal spellings today (`input.rs:687-694`), so this
  is an added arm there.

Decoding, matching `escape_parser.zig:122-123`: `code` is `122` (`z`) or `90` (`Z`); `mask =
modifier - 1`; super is `mask & 0x08`; with super set the action is `Action::Redo` when
`mask & 0x01` is set, `Action::Undo` otherwise. Everything else under both families is
`Action::Ignore`.

The existing `modifier` helper (`input.rs:726`) is **not** reusable as-is: it accepts only the empty
params or a literal `1;` prefix (`input.rs:731`), which is the cursor-key encoding, not `122;10`. T4
adds a sibling helper in the same byte-exact style — digits matched as bytes, bounded length, no
normalisation — so that `122;010` stays `Ignore` (`input.rs:666-683` forbids accepting spellings no
terminal emits). Do not loosen `modifier` to serve both.

`Shell::act` (`shell.rs:1924-1948`) gains **two** arms, not one:

```rust
Action::Undo | Action::Redo => {
    // `edit_history`, never `history`: the latter is prompt recall (`shell.rs:348`).
    let Shell { edit_history, editor, .. } = self;
    let moved = match action {
        Action::Undo => edit_history.undo(|delta| editor.revert(delta)),
        _ => edit_history.redo(|delta| editor.replay(delta)),
    };
    // No record, and no `amended`: an undo must not clear its own redo, and it is not the
    // user replacing a recalled line -- it is the user taking their own last edit back.
    if moved { self.moved(); }
}
Action::Yank => {
    let delta = self.yank_killed();      // Option<Delta>: Ordinary, fresh entity ids
    self.amended(delta);                 // a yank is a text change like any other
}
```

`Editor::apply`'s exhaustive no-op group (`editor.rs:296-306`) lists the three new actions so the
composer never sees them.

## Tasks

- **T1** `edit_history.rs`: `Delta` + constructor, `EditHistory`, §S2 and §S3, tests first, failing.
- **T2** `editor.rs`: mutators return `Option<Delta>` (`insert_entity` too, §S4); `revert`/`replay`
  with caret restoration.
- **T3** `shell.rs`: the `edit_history: EditHistory` field (**not** `history`, `:348`),
  `amended(Option<Delta>)` → `edited(Option<Delta>)` with all six existing `amended` call sites
  updated (`:1837`, `:1874`, `:1906`, `:1948`, `:2014`, `:2131`), `self.edit_history.boundary()` in
  `take_draft` (`:2114-2118`), `complete` (`:1295-1307`) and `clear_composer` (`:2126-2132`) passing
  `None`, recall boundary, paste-path migration, the two `act` arms, `transaction.rs` deletion
  **with** its tests migrated (§S4's `rg` gate).
- **T4** `input.rs`: two control arms, the two §S5 CSI families with the new modifier helper, and
  the decoder tests below.
- **T5** PTY scenario in `tests/tui.rs` and a `scripts/smoke-tui.sh` row.

## Tests

Executable shapes. Surfaces used below were checked against the tree: `Decoder::feed(&mut self,
byte: u8, now: Instant, out: &mut Vec<Input>)` is **one byte at a time** (`input.rs:289`),
`Input::Action(..)` (`input.rs:180`), `Editor::entities()` (`editor.rs:107`) → `Entities::spans()`
(`entity.rs:181`) and `is_empty()` (`entity.rs:168`), `csi(params: &[u8], final_byte: u8)`
(`input.rs:684`), `control(byte: u8)` (`input.rs:644`), `MAX_COMPOSER_BYTES = 8 * 1024 * 1024`
(`editor.rs:73`).

```rust
#[test]
fn a_delta_over_the_byte_cap_resets_both_stacks() {
    let mut edits = EditHistory::new();
    edits.record(typed("a", 0));                       // helper: Ordinary, caret 0 -> 1
    assert!(edits.undo(|_| {}), "one entry to walk back over");
    edits.record(inserted_bytes(MAX_RETAINED_BYTES + 1));
    assert!(!edits.undo(|_| {}), "the oversized delta is a boundary, not an entry");
    assert!(!edits.redo(|_| {}), "the boundary clears redo too");
    assert_eq!(edits.retained(), 0);
}

#[test]
fn an_undo_redo_walk_leaves_the_retained_total_alone() {
    let mut edits = EditHistory::new();
    edits.record(typed("a", 0));
    let after_record = edits.retained();
    assert!(edits.undo(|_| {}));
    assert_eq!(edits.retained(), after_record, "a transfer moves bytes, it does not spend them");
    assert!(edits.redo(|_| {}));
    assert_eq!(edits.retained(), after_record);
}

#[test]
fn recording_after_an_undo_reclaims_the_cleared_redo_bytes() {
    // The N4 regression: `record` clears redo, and those bytes are in `retained`.
    let mut edits = EditHistory::new();
    let one = typed("a", 0);
    let weight = one.weight();
    edits.record(one);
    edits.record(typed("b", 1));
    assert_eq!(edits.retained(), weight * 2);
    assert!(edits.undo(|_| {}));
    assert!(edits.undo(|_| {}));
    assert_eq!(edits.retained(), weight * 2, "both entries are still held, on the redo stack");
    edits.record(typed("c", 0));
    assert_eq!(edits.retained(), weight, "exactly the new entry: the two cleared ones were freed");
    assert_eq!(edits.undo_depth(), 1);
    assert_eq!(edits.redo_depth(), 0);
}

#[test]
fn eviction_drops_the_oldest_undo_entry_first() {
    let mut edits = EditHistory::new();
    for index in 0..=MAX_ENTRIES { edits.record(typed("x", index)); }
    assert_eq!(edits.undo_depth(), MAX_ENTRIES);
    let mut carets = Vec::new();
    while edits.undo(|delta| carets.push(delta.caret_before())) {}
    assert_eq!(carets.last().copied(), Some(1), "entry 0 was evicted, entry 1 is oldest");
}

#[test]
fn consecutive_kills_replace_the_slot_rather_than_accumulating() {
    let mut edits = EditHistory::new();
    edits.record(killed("first ", 6));
    edits.record(killed("second", 6));
    assert_eq!(edits.killed_text(), Some("second"), "kill_ring.zig:113-115 swaps and frees");
}

#[test]
fn a_kill_fills_the_slot_and_an_ordinary_delete_does_not() {
    let mut edits = EditHistory::new();
    edits.record(killed("word", 4));
    edits.record(typed_backspace("d", 3));
    assert_eq!(edits.killed_text(), Some("word"));
}
```

Shell-level cases, driven through the real recording funnel rather than a detached history
(`Shell::act` and the `#[cfg(test)]` shell fixture already used at `shell.rs:3965`). The depth
helper is `edit_depths() -> (usize, usize)` — undo, redo — named for the new field so it cannot be
read as prompt-recall depth. The rev-3 case `a_refused_paste_records_nothing_and_leaves_redo_alone`
is **dropped rather than repaired**: a paste that large goes down `Pasted::Collapsed`
(`shell.rs:1881-1905`), where only a short summary is inserted, so the case would have passed while
exercising a mechanism its name denies. `a_no_op_keystroke_does_not_clear_redo` below carries the
same rule through a refusal that is verified to fire.

```rust
#[test]
fn an_undo_after_a_submit_restores_nothing_and_does_not_panic() {
    let mut shell = shell_for_tests();
    type_text(&mut shell, "hello");
    submit(&mut shell);                       // reaches `take_draft` (`shell.rs:2114-2118`)
    shell.act(Action::Undo, Instant::now());  // panicked before the boundary existed
    assert_eq!(shell.draft(), "", "a discarded draft is not undoable");
    assert_eq!(shell.edit_depths(), (0, 0));
}

#[test]
fn clear_composer_is_the_same_boundary_as_a_submit() {
    let mut shell = shell_for_tests();
    type_text(&mut shell, "hello");
    shell.act(Action::Cancel, Instant::now());   // routes to clear_composer (`shell.rs:2126`)
    shell.act(Action::Undo, Instant::now());
    assert_eq!(shell.draft(), "");
    assert_eq!(shell.edit_depths(), (0, 0));
}

#[test]
fn a_slash_completion_is_a_boundary_and_never_a_half_undo() {
    let mut shell = shell_for_tests();
    type_text(&mut shell, "/hel");
    complete(&mut shell, "help");                // take_draft + insert (`shell.rs:1295-1307`)
    assert_eq!(shell.draft(), picker::completed("help"));
    shell.act(Action::Undo, Instant::now());
    assert_eq!(shell.draft(), picker::completed("help"),
        "undo must not delete the command name and leave the query lost");
    assert_eq!(shell.edit_depths(), (0, 0));
}

#[test]
fn a_yank_is_itself_undoable_and_leaves_the_kill_slot_loaded() {
    let mut shell = shell_for_tests();
    type_text(&mut shell, "one two");
    shell.act(Action::DeleteWordLeft, Instant::now());   // kill: slot = "two"
    assert_eq!(shell.draft(), "one ");
    shell.act(Action::Yank, Instant::now());
    assert_eq!(shell.draft(), "one two");
    shell.act(Action::Undo, Instant::now());
    assert_eq!(shell.draft(), "one ", "the undo takes back the yank, not the kill");
    shell.act(Action::Yank, Instant::now());
    assert_eq!(shell.draft(), "one two", "an ordinary yank delta did not clear the slot");
}

#[test]
fn a_no_op_keystroke_does_not_clear_redo() {
    let mut shell = shell_for_tests();
    type_text(&mut shell, "a");
    shell.act(Action::Undo, Instant::now());
    shell.act(Action::Delete, Instant::now());   // caret at end: `start >= end` (`editor.rs:366`)
    assert_eq!(shell.edit_depths(), (0, 1));
    shell.act(Action::Redo, Instant::now());
    assert_eq!(shell.draft(), "a");
}
```

Decoder cases:

```rust
#[test]
fn super_shift_z_is_redo_in_both_pinned_encodings() {
    assert_eq!(csi(b"122;10", b'u'), Action::Redo);      // runtime.zig:3018
    assert_eq!(csi(b"27;10;122", b'~'), Action::Redo);   // runtime.zig:3025
    assert_eq!(csi(b"122;9", b'u'), Action::Undo, "super without shift");
    assert_eq!(csi(b"90;10", b'u'), Action::Redo, "escape_parser.zig:122 accepts Z");
    assert_eq!(csi(b"122;1", b'u'), Action::Ignore, "no super bit is not a redo");
    assert_eq!(csi(b"122;010", b'u'), Action::Ignore, "spellings no terminal emits");
    assert_eq!(csi(b"27;10;121", b'~'), Action::Ignore, "y is not z");
    assert_eq!(csi(b"1", b'~'), Action::Home, "the existing tilde spellings still decode");
    assert_eq!(control(0x1a), Action::Ignore, "Ctrl-Z is still not undo");
}

#[test]
fn a_redo_sequence_split_across_reads_decodes_once() {
    let mut decoder = Decoder::new();
    let mut out = Vec::new();
    for byte in b"\x1b[122;10u" {                 // `feed` takes one byte (`input.rs:289`)
        decoder.feed(*byte, Instant::now(), &mut out);
    }
    assert_eq!(out, vec![Input::Action(Action::Redo)]);
}
```

Editor cases:

```rust
#[test]
fn undo_of_a_paste_restores_the_block_with_its_original_id() {
    let mut editor = Editor::new();
    let delta = editor
        .insert_entity("[#1 12 lines]", EntityKind::Paste { id: 1, text: "…".into(), lines: 12 })
        .expect("the paste fits");                        // Option<Delta> per S4
    let id = delta.inserted_entities()[0].id();
    editor.revert(&delta);
    assert!(editor.entities().is_empty());
    editor.replay(&delta);
    assert_eq!(editor.entities().spans()[0].id(), id);
    assert_eq!(editor.caret(), delta.caret_after());
}
```

Same shape for `byte_cap_evicts_until_under`, `paste_payload_counts_toward_retained_bytes`,
`a_new_edit_clears_redo`, `undo_then_redo_restores_text_and_caret`,
`an_oversized_kill_clears_the_slot`, `kill_bytes_are_not_history_bytes`,
`a_delta_round_trips_over_an_entity_boundary`, the migrated §S4 transaction cases, and the PTY case
`edit_history_undo_and_yank_on_a_real_terminal` (type `one two`, `C-w`, assert the grid lost the
word, `C-y`, assert it returned, `C-_`, assert the **yank** was undone and the word is gone again;
raw bytes plus grid excerpts are the evidence `.prd/06-qa-harness.md` demands). The PTY case drives
undo and yank through their control bytes; the redo families are driven at the decoder, since a PTY
test cannot make a terminal send a Super chord.

## Gate commands

Read `.prd/tui-phase2/loop.md` §Gate contract for the exact invocations; do not guess them from this
file. `.prd/tui-phase3/loop.md:43-50` records the `edcc2f5` baseline: default all-targets 1797
passed / 0 failed / 1 ignored, fault TUI 79 passed, fault lib 1163 passed / 1 ignored, `fmt` + both
clippy modes + four check scripts exit 0; release qualification also builds both release variants
and runs CLI smoke (46 checks) and TUI smoke (511 checks). At R2's dirty tree: 1801 / 79 / 1167.
**Observations, not expected literals** — rerun and report deltas.

## Global constraints

UI thread owns terminal mechanics; the history is pure state and touches no writer. Worker
separation and bounded channels unchanged. No credentials and no live network in fixtures. One
writer at a time; no controller stress during another agent's mutation sweep. External review plus a
directly rerun gate precede integration. The two §S5 families are the only CSI-u surface this slice
adds — P3-KEYS keeps its own promotion measurement and this plan makes no full-matrix claim.
User-visible acceptance is unchanged from revision 2: undo on `0x1f`, yank on `0x19`, caps 100
entries / 1 MiB.

## Review dispositions (revision 4)

Rev-3 scoped pass:

| Item | Disposition | Where |
|---|---|---|
| N1 `history` name collision | Fixed — new field is `edit_history: EditHistory`; `history: History` (`shell.rs:348`) is prompt recall and is left alone. Every snippet, task and test renamed; the test helper is `edit_depths()` | §Architecture opening paragraph, §S5 `act` snippet, T3, all shell tests |
| N2 `Yank` skipped `amended` | Fixed — `amended(Option<Delta>)` specified as `self.history.leave(); self.edited(delta);` (`shell.rs:2570-2573`); Yank routes through `amended(Some(delta))`; `complete` and `clear_composer` call `amended(None)` explicitly; nothing in the slice calls `edited` directly | §S2 `amended` bullet and slash-completion bullet, §S3, §S5, T3 |
| N3 accessor set too small | Fixed — `at`, `removed`, `inserted`, `removed_entities` added beside `inserted_entities`, the carets, `kind` and `weight` | §S1 |
| N4 `record` must subtract cleared redo | Fixed — the rule is now four operations, `record` adds *and* subtracts the redo stack it clears, citing `clearStack`/`reset` (`edit_history.zig:71-75`); new exact-byte regression | §S2 accounting bullet; test `recording_after_an_undo_reclaims_the_cleared_redo_bytes` |
| C1 completion mis-citation | Corrected — ruling (3) is `navigatePromptHistory`, so it covers recall only; the slash-completion boundary is relabelled an **xfx default** justified structurally by the `take_draft` funnel (`shell.rs:1296`) | §Upstream rulings item 3, §S2 slash-completion bullet, M2 row below |
| M6 caveat (refused paste) | Case **dropped**, not repaired — an 8 MiB paste goes down `Pasted::Collapsed` (`shell.rs:1881-1905`), so the test would have passed on a mechanism its name denies. `a_no_op_keystroke_does_not_clear_redo` carries the rule | §Tests shell preamble |
| Six `take_draft` callers | Fixed — the bullet now says every caller present and future, lists today's six as a parenthetical and says not to maintain the list | §S2 whole-draft-disposal bullet |

Rev-2 pass (carried forward):

| Item | Disposition | Where |
|---|---|---|
| M1 `take_draft` uncovered | Fixed | §S2 whole-draft-disposal bullet; T3; tests `an_undo_after_a_submit_…`, `clear_composer_is_the_same_boundary_…` |
| M2 `complete` lying delta | Fixed — boundary contract, no insert-only delta. Attribution corrected by C1: xfx default forced by the `take_draft` funnel, **not** upstream ruling (3) | §S2 slash-completion bullet; T3; test `a_slash_completion_is_a_boundary_…` |
| M3 yank not undoable | Fixed — `Yank` split into its own arm with an `Ordinary` delta; routed through `amended` per N2 | §S3, §S5 `act` arms; test `a_yank_is_itself_undoable_…` |
| M4 `Delta` unconstructible | Fixed — `pub(crate) Delta::new`, editor is sole producer; read side completed by N3 | §S1, §Architecture |
| M5 accounting contradiction | Fixed — single rule, transfers are no-ops; redo-clear subtraction completed by N4. Borrowck claim withdrawn; the closure API is justified by atomicity, not the compiler | §Architecture, §S2 accounting bullet |
| M6 detached refusal test | Fixed — the funnel-driven `a_no_op_keystroke_does_not_clear_redo` carries it; the paste variant is dropped | §Tests shell block |
| S1 `insert_entity` type | Fixed — `Option<Delta>`, span read from `inserted_entities()` | §S4, editor test |
| S2 transaction tests | Fixed — migrate, with an `rg` gate closing the set; three members named | §S4, T3 |
| S3 helper surfaces | Fixed — verified in-tree; `Decoder::feed` corrected to per-byte | §Tests preamble, fragmented-read test |
| S4 modifier reuse | Partly declined, with reason — `modifier` (`input.rs:726-731`) only accepts the cursor-key `1;` shape; a sibling helper in the same byte-exact style is specified instead of loosening it | §S5 |
