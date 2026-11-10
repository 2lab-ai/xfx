# APPROVAL-READINESS-SETTLED receipt: no affirmative until a frame shows the change it was read with

A key that moved a permission question's marker, edited its draft or walked
its review, read in the same delivery as the Enter after it, was answered
before any frame showed the change: Down and Enter in one read granted
`Always` under a frame that marked `Apply once`; a letter and Enter sent text
no frame painted. fx v0.0.16 refuses such an affirmative at a **file**
approval until the frame is drawn. xfx now refuses it at **every** approval
(the dispatcher's decision, below). APPROVAL-VERTICAL-SPELLINGS found the
gap; its head `a37a969e` (branch `fix/p5-approval-vertical-spellings`, not
yet on `main`) is this unit's base.

| commit | what |
|---|---|
| `4322bebe` | RED: five new shell tests, one case of the one-read test flipped, the scenario `p5-approval-readiness-settled`; tests only in `src/` |
| `67e40758` | **the code head**: the readiness layer's stamp (`src/tui/approval_readiness.rs`), the shell's call sites and the silent refusal (`src/tui/shell.rs`), `ApprovalScreen::scroll_by` saying whether it moved, four readiness unit tests, two test-only corrections to the RED commit's cases and one older test given its frame (below), `docs/parity.md`, the ledger's moved refs |
| the commit after | this receipt, and the ledger cause `approval-settled-every-approval`; no `src/` change |

Captures, all `p5-approval-readiness-settled` at 40x120: `fx/` is fx v0.0.16
(sha256 `8e6cda2e…`, `FX_AUTO_UPGRADE=0`; `capture.sh` checked its bytes after
the run); `red/` the base's binary (row 1 `x𝒇x v0.1.0-a37a969 [debug]`, built
at `a37a969e` before any commit, sha256 `3f992666d894…`); `xfx/` the code
head's (row 1 `x𝒇x v0.1.0-67e4075 [debug]`, sha256 `2756d8a32cc1…`). No `gwlog`
step, so no request body is in the receipt. The hashes and banners: `binaries.txt`.

## fx's rule (`fx-src.txt`, written by `fx_src.sh`: every line read with `git show 481104fd:<path>`)

- **Which keys ask for a modal frame.** Every route that changes what the
  approval shows calls `requestActiveSurfaceFrame(app)`, which requests
  `.modal` (`input_approval_runtime.zig:28-30`): a focused draft edit in an
  open amendment, Super-Up/-Down included (`:79-89`, `:86`); Left/Right and
  Alt-b/Alt-f at a choice (`:91-110`, `:98`, `:108`); Up and Down anywhere
  (`:111-126`, `:117`, `:125`); a review wheel that can scroll (`:159-177`,
  `:176`); and a raw approval action whose event is `.redraw` (`:56-62`) --
  `move_choice`, `tab`, `backspace` and `edit_draft` in an amendment, an
  inserted character (`approval_decision.zig:142-174`, `:196-212`). A typed
  scalar asks for it too (`app_input_runtime.zig:1251-1255`). An action the
  approval does not take (`input_approval_runtime.zig:128`, `else => {}`) and
  an event `.none` (`:58`) ask for nothing.
- **What "pending" means and when it clears.** `settledForApproval` is false
  while `.modal` (or `.resize`, `.external_damage`, `.first_frame`) is
  pending, while an attempt that took one is in flight, or while a resize is
  unsettled (`render_request.zig:172-177`, `:363-370`, `:216-218`).
  `beginAttempt` moves the pending reasons into the attempt and marks it as
  affecting the approval (`:272-296`); only `commit` clears that
  (`:298-324`), and `restore` puts the reasons back (`:326-351`). `commit` is
  called only for a frame whose bytes were written whole
  (`app_render_runtime.zig:1002-1032`; `.committed` against a partial write,
  `terminal_diff.zig:15-23`), and a frame abandoned because input is waiting
  restores them (`app_render_runtime.zig:986-1000`). So: drawn **and**
  delivered to the terminal. fx's own test walks it (`render_request.zig:591-627`).
  Every byte of a read, and of up to 32 reads (`event_loop.zig:16`), is
  handled before the frame (`event_loop.zig:94-119`), so a key and the Enter
  after it in one write meet a pending frame.
- **What fx does with the refused affirmative.** It requests the frame,
  writes a trace line and returns (`input_approval_runtime.zig:207-217`):
  dropped -- not held for the frame, not told to the user. The draft and the
  choice stay as they were.
- **Deny is exempt**: the check opens `if (decision != .deny and
  app.approval_prompt.request.?.file != null)` (`input_approval_runtime.zig:191-192`).
- **File approvals only**: the same line. A request carries `command` or
  `file` (`permission_request.zig:29-42`, `:39-40`); a run-command request
  has no `file`, and fx answers it at once. The readiness it checks then
  needs a settled render state and a committed screen for this request at
  this size (`approval_screen.zig:600-607`; `approval_readiness.zig:15-39`,
  `:65-75`).

**The captures agree** (`fx/`): at the edit, Down and Enter in one write
leave the question up with choice 2 marked (`edit-down-enter`), Left and
Enter with choice 1 marked (`edit-left-enter`), and the next Enter answers
it (`write-ask` follows); at the write, `x` and Enter leave `Apply once, x`
up (`write-letter-enter`), and so do Super-Up and Enter
(`write-super-up-enter`); at the first command Down Down Enter is a refusal
(`cmd2-ask` follows); at the second command Down and Enter answer `Always`
at once and the turn ends (`cmd2-down-enter`: `Done with approvals.`). The
edit's refusal is also the evidence that one `key` line, one tmux
`send-keys`, reaches the program in one read: had the Enter come after a
frame, fx would have answered it.

## What xfx did at the base

The gate checked only the readiness receipt (`Shell::decide`'s
`matches!(answer, ApprovalAnswer::Once | ApprovalAnswer::Always) &&
!…approval_ready(id)`). A key whose reply is `Moved` asked for a modal frame
and kept the receipt, and the receipt's identity is the question, the plane,
the size and the layout -- not the marker, the caret or the draft. So an
Enter in the same read answered the state the key left.

## The change (`git diff --stat a37a969e 67e40758 -- src`: 3 files, +491 / -55, most of it tests and comments)

- **The readiness layer** (`src/tui/approval_readiness.rs`): `Readiness`
  counts the changes to what the question shows (`revision`, bumped by
  `changed()`); `intend` stamps each composition with it, and the stamp rides
  pending, provisional and receipt. `ready` asks, beside everything it asked
  before, that the receipt's stamp is the current count: a frame composed
  after the last change has landed and been reconciled. `unshown` says the
  receipt holds but is stale. A repaint that wrote nothing (`Unchanged`)
  keeps a receipt of the same screen, as before, and now takes the
  composition's stamp: the screen already holds what is current (Down then
  Up, a caret sent where it was). The identity (`Intent`'s fields) is
  unchanged; the stamp is compared apart from it (`same_screen`).
- **The shell** (`src/tui/shell.rs`) calls `approval_changed()` -- one
  method, the one place a subject check would go -- on every `Reply::Moved`
  (the marker; a draft's caret or text: a letter, C-u, C-w, a yank, a paste's
  end, Super-Up/-Down), from both planes, and on a review-plane `C-p`/`C-n`
  that moved the window (`scroll_by` now says whether it did). A swallowed
  key calls nothing. `Reshaped` still revokes, as before.
- **The gate**: an affirmative that is not `ready` but is `unshown` is spent
  silently, a modal frame requested: fx's refusal. Any other not-ready
  affirmative keeps xfx's two sentences (`APPROVAL_NOT_READY`,
  `APPROVAL_NOT_DISCLOSED`). `Deny`, Escape and Ctrl-C are not gated, as
  before and as fx's `decision != .deny`.
- **The feedback case falls out of the same condition**: every draft edit is
  `Moved` (`approval::answered`), so a letter, C-u, C-w or a yank read with
  Enter, or a paste's end and an Enter with no frame between, is held back
  (`an_edit_and_enter_in_one_read_send_nothing_until_a_frame_shows_the_draft`).
  A letter that wraps the draft onto another row is `Reshaped`: the receipt
  is revoked and the Enter told `APPROVAL_NOT_READY`, as at the base.

## The decision: every approval, stricter than fx

The dispatcher's (2026-11-10, reversible, kept for the user's veto): fx's
one condition applies to **every** approval, not only file approvals. fx's
run-command approval lets Down and Enter in one read answer `Always`
(`fx/fx-p5-approval-readiness-settled-40x120-cmd2-down-enter.txt`); a grant
widened past what the frame showed is a safety matter on any approval.
**The one-place revert**: in `Shell::approval_changed`, call
`self.readiness.changed()` only while the question's subject is
`crate::permission::ApprovalSubject::Path`. Mutant M2 is that code; it
compiles, and only the command test catches it (`mutants.txt`). Recorded in
`docs/parity.md`'s approval sentence and the ledger cause
`approval-settled-every-approval` (the one pair it makes differ).

Two more decisions, each reversible:
- **Silent, as fx.** xfx's `APPROVAL_NOT_READY` says "the question is not on
  the screen yet", which is false here: the question is on the screen, and
  the frame asked for shows the change the next Enter answers. The silence is
  also what lets the pairs be identical.
- **A walk that cannot move holds nothing back.** fx's wheel asks for a frame
  whenever the review can scroll (`input_approval_runtime.zig:173-176`); xfx
  marks a change only when the window moved, so `C-n` at the tail, where the
  plane opens, and Enter still answer. (The review plane's `C-p`/`C-n` are
  xfx's own binding, recorded in `docs/parity.md` already.)

## RED (`red.txt`)

At `4322bebe`, the base's code with the tests added (`git diff a37a969e
4322bebe -- src` adds tests only): five tests fail, one passes.

- The move test at an edit and at a command: Down, Right, Alt-f, Down Down Up
  and Down Down Down, each with Enter in one read, all answered (`Always`
  four times, `Once`) at both.
- The edit test: a letter, C-u, C-w, C-w then a yank, each with Enter in one
  read, and a paste then Enter with no frame between, all sent (`ab cdx`,
  nothing, `ab `, `ab cd`, `ab cd zz`).
- The review-plane walk: the moving walk and Enter answered `Once`.
- The one-read test's flipped case: Super-Up and Enter in a draft answered
  `Once` with `ab cd`.
- `a_refusal_read_with_a_move_still_refuses_at_once` passes at the base too:
  it guards the exemption, not the fix.

Two test-only corrections came with the code commit, after that run: the
walk test's no-move case used `C-p`, which moves at the tail where the plane
opens, so it became `C-n` and the moving case `C-p`; and the move cases'
marker check matched `❯ 2  ` (an edit's) where a command's reads `❯ 2. Yes`.
At the base both tests failed before either line (`red.txt`), and mutant M1,
the condition removed, fails all seven tests that rest on it at the head
(`mutants.txt`).

The pair at the base (`red.txt`'s end, `scope.py red`): 1 of 10 identical.
The base answered the edit's Down and Enter, so from `edit-down-enter` on
its screens are a question ahead, and the scenario's later keys run past its
turns (`red/xfx-p5-approval-readiness-settled-40x120-write-super-up-enter.txt`:
a provider error, the fake gateway's turns used up).

## GREEN (`green.txt`)

- At the code head, the unit's tests and their neighbours: 423 passed, 0
  failed: the five tests, the flipped one-read test, the
  APPROVAL-VERTICAL-SPELLINGS no-op tests unchanged
  (`a_no_op_vertical_spelling_keeps_the_receipt_and_enter_answers_the_choice_shown`
  among them), and four readiness unit tests: a change holds every grant
  until a frame composed after it is proven; a frame composed before a change
  does not settle it; a repaint that wrote nothing settles a change it
  already shows; a change with nothing proven is not `unshown`.
- **The pairs** (`scope.py xfx`: the ledger's `approval-grant-scope` rows set
  aside on each side, row 1 left out, styles compared): **6 of 6
  file-approval pairs identical** -- `edit-ask`, `edit-down-enter`,
  `edit-left-enter`, `write-ask`, `write-letter-enter`,
  `write-super-up-enter`. Of the other four, `cmd-ask` and `cmd2-ask` differ
  in one row, the command's comment (fx `$ # shell.run profile=user
  shell=/bin/zsh`, xfx `$ # terminal.exec cwd=.`: each product's own tool,
  the ledger's `tool-schema` cause, as at the base); `cmd2-down-enter` is
  the decision above; `end` differs only in the turn's seconds, which the
  ledger's `turn-wall-time` normalization reads as equal. The sweep below
  classifies each with the whole ledger.

One older test moved: `enter_takes_the_marked_choice_and_tab_and_the_arrows_are_what_mark_it`
sent Down, Up, Down and Enter with no frame between and expected `Always`;
it now delivers the frame before the Enter, which its claim (Enter takes what
the marker is on) needs. APPROVAL-VERTICAL-SPELLINGS' M14 (Super-Up/-Down
in a draft revoking the receipt) guarded the case this unit flips; that
Enter is now held back by the stamp rather than a revocation, so the receipt
survives the caret's move and the next frame settles it.

## Mutants (`mutants.py`, `mutants.txt`): 10 of 10 killed

On a `git clone --no-local` throwaway of the branch at `67e40758` (remote
removed) under `$TMPDIR/mut/`, one at a time, the file restored after each,
under the suite lock; the clone's tree clean after the run.

| mutant | killed by |
|---|---|
| M1 the condition removed (`ready` ignores the stamp) | 7 tests: both readiness change tests, the move tests at an edit and at a command, the edit test, the walk test, the one-read test |
| M2 the condition restricted to file approvals (the one-place revert) | the move test at a command |
| M3 Deny gated too | the refusal test, and `an_approval_arriving_under_the_panel_takes_the_keys_and_the_screen_and_gives_them_back` |
| M4 pending never cleared (every composition stamped 0) | 15 tests |
| M5 a no-op key marks the question changed | both APPROVAL-VERTICAL-SPELLINGS no-op tests |
| M6 a repaint that wrote nothing keeps the old stamp | the readiness `Unchanged` test |
| M7 the unshown refusal told as `APPROVAL_NOT_READY` | both move tests |
| M8 a walk that moved holds nothing back | the walk test |
| M9 a walk that could not move holds the Enter back | the walk test |
| M10 a frame composed before the change settles it (stamped at the write) | the readiness test for it |

M1-M5 are the brief's five; M6-M10 what the condition rests on.

## Sweep (`sweep-base.md`, `sweep-head.md`, `sweep-compare.txt`)

`scripts/fxcap/sweep.sh --only '*approval*' --only composer-keys --only
question-field` at 40x120, `--jobs 4`, this branch's scenarios and ledger,
once with the base's binary (`v0.1.0-a37a969`) and once with the code
head's (`v0.1.0-67e4075`), both in one suite-lock slot; compared pair by
pair with APPROVAL-VERTICAL-SPELLINGS' `cmp_sweeps.py`, unchanged.

- **base 20 / 108, head 21 / 108, drops 0.** Pairs with rows no cause
  covers: 11 at the base, 3 at the head.
- `p5-approval-readiness-settled`: at the head every one of its 10 pairs has
  0 rows no cause covers, from 19-27 in eight of them at the base. `end` is
  normalized-identical; the six file-approval pairs are covered by
  `approval-grant-scope` alone; `cmd-ask` and `cmd2-ask` by it and
  `tool-schema` (the command's comment row, each product's own tool);
  `cmd2-down-enter` by the new `approval-settled-every-approval`, whole snap
  -- on both sides, since both sweeps read this ledger.
- No other pair changed class, cause or count: `p5-approval-horizontal-keys`
  29 and `p5-approval-vertical-keys` 27 pairs under `approval-grant-scope`
  alone, `composer-keys` 8 / 11, `question-field` 10 / 10.
- The 3 pairs with uncaused rows at the head are the same 3 as at the base,
  with the same counts (`p5-input-approval` `cmd-ask` 15, `edit-ask` 17,
  `after` 13); this unit touches no row of that scenario.

## Gate (`gate.txt`)

The brief's gate, once, under the suite lock with a short `TMPDIR`, at the
code head `67e40758` (`src/` and `tests/` as committed; the ledger's new
cause was in the working tree, and the parity test read it). Every step
exits 0, and `src/` and `tests/` were unchanged at the end:

| step | result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy --locked --all-targets -- -D warnings` | clean |
| `cargo clippy --locked --all-targets --features fault-injection -- -D warnings` | clean |
| `cargo test --locked --no-fail-fast` | 5564 passed, 0 failed, 7 ignored (15 result lines) |
| `cargo test --locked --features fault-injection --no-fail-fast` | 5601 passed, 0 failed, 7 ignored (15 result lines) |
| `scripts/check-xfx-identity.sh`, `check-no-stubs.sh`, `check-no-secrets.sh` | exit 0 each |

5564 is APPROVAL-VERTICAL-SPELLINGS' gate count, 5554, plus its round-2
one-read test and this unit's nine (`git diff a37a969e 67e40758 -- src`
adds nine `#[test]`s and removes none: five shell, four readiness). No test
here builds a `Runtime`. Every commit's added lines were scanned for local
paths before it was made (`git diff --cached`).

## Parity (`docs/parity.md`) and the ledger

- The full-screen TUI row's approval sentence: the clause "Also not changed,
  found by this unit: when an affirmative may be taken" is replaced by what
  is copied (fx's condition, the keys that ask for its frame, when it clears,
  its silent refusal, its Deny exemption) and where xfx is stricter (every
  approval, the decision, the revert). `fx_src.sh` checks those citations
  with the unit's added lines (run as its header says; `fx-src-check.txt`): 86 citations checked, 0 outside `fx-src.txt`'s ranges.
- The ledger's refs into `src/tui/shell.rs`, by content, with
  APPROVAL-VERTICAL-SPELLINGS' `refs.py` (`python3 -I refs.py a37a969e`,
  from the repo root): 17 checked, 0 wrong (`refs.txt`). At the base six of
  them -- the `tools: {} advertised` assertion of `session-record`,
  `transcript-page-offset`, `toolgroup-ctrlo-record-short`, `safe-page-edge`,
  `cancel-page-edge` and `full-transcript-shift` -- named a line 3 off (39893
  for 39896): `a37a969e`'s test comment above them moved them after its
  `refs.txt` was taken. `--fix` re-pointed the twelve this unit moved, those
  six included. No ledger ref names `approval_readiness.rs` or
  `approval_screen.rs`.
- A new cause, `approval-settled-every-approval` (kind `decision`, the
  dispatcher's interim choice; it approves nothing): this scenario's
  `cmd2-down-enter`, whole snap.

## Safety

No new string reaches the terminal; nothing writes a file, runs a process or
reaches the network. The change only withholds answers: `ready` asks
strictly more than before, so no affirmative refused at the base is taken
now, and the three refusals stay answerable at every moment.

## Not changed

- A command question's comment row (`shell.run` against `terminal.exec`):
  each product's own tool, the ledger's `tool-schema` cause, and not this
  unit's.
- When the receipt is missing (no frame yet, a resize, a reshape), xfx still
  tells the user (`APPROVAL_NOT_READY`, `APPROVAL_NOT_DISCLOSED`) where fx
  writes only its trace line; so at the base, and recorded in
  `docs/parity.md` already ("When a screen cannot show the whole request the
  session says which of the two it is").
