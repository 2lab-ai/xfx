# APPROVAL-PASTE-SPACES receipt: an amendment's paste is one line, as fx's

fx v0.0.16 normalizes a paste into an approval's amendment draft before it
inserts it. CR LF and a lone CR become LF, and then every LF and every Tab
becomes one space (`normalizeApprovalAmendmentPasteInPlace`,
`input_paste_runtime.zig:19-25@481104fd`). Then it checks the draft's 4096
bytes against what the paste has become. xfx kept the breaks and tabs, so a
draft could hold several lines, its Home went to a line's start, and a lone
CR stayed in the rows the band composed (the painter dropped it). It also
refused a paste at its 4097th byte as the bytes arrived, so a CR LF counted
two bytes. This unit copies fx's rule for the amendment. The composer and a
question's field keep their own rules, which are already fx's.

| commit | what |
|---|---|
| `a5cf9567` | RED: the unit tests and the shell test, the COMPOSER-SUPER-ARROWS amendment block flipped to fx's 0, scenario `p5-approval-paste-spaces`. Tests and the scenario only |
| `9a73d0ba` | **the code head**: `DraftPaste` (the 8 MiB capture bound, `one_line`, the budget after it), the module's and `MAX_FEEDBACK_BYTES`' docs, one comment in `Shell::decide`; MODIFIED-PAGE-KEYS' page-key test still pastes two lines, which now become one |
| `b9afbe3b` | tests only: an image path and a long paste in a draft; the scenario's `cursor` line dropped (it read where each binary parks the terminal's cursor) |
| `1f5d32a3`, `bf518f5f` | `docs/parity.md`; this folder |
| `4a8f0a6c` | a comment only: the module doc names the decoder's half of "no key types a break into a draft" (`Input::Text` is never a control character, `src/tui/input.rs`) |
| `2804b108` | tests only: Super-A in a draft makes no selection a paste replaces; the multibyte test's comment names the bound it now meets (round 1, advisor) |
| `13f1436f` | the ledger's six refs into `shell.rs` re-pointed (Refs) |
| `f5d2de76` | `docs/parity.md`: fx's draft window is one row scrolled to the caret |
| `f438a5b6` | a test comment's fx citation given its file (the last round-1 `src/` commit, tests only); the ledger's refs re-pointed again |
| `e26ce67b` | this folder; **round 1's head, gated by the dispatcher** (Gate) |
| `0e2b242b` | round 2 RED, tests only (Round 2) |
| `53557ed6` | **round 2's code head**: Home and End in a draft are its edges; a paste of more than twice the room refused before decoding |
| `276fc5d7` | the ledger: `amend-draft-window`, the refs re-pointed; the round 2 mutant runner |
| `45ced900`, `ac276d9f` | `docs/parity.md`; this folder. `ac276d9f` is **round 2's head, gated by the dispatcher** |
| `802dd239` | round 3, tests and comments only (Round 3) |
| the commits after | the ledger, `docs/parity.md`, this folder; no `src/` change |

The source change outside the test modules (`srcsize.txt`, `srcsize.py`,
from the RED commit `a5cf9567`):
- the round-1 code head `9a73d0ba`: +42/-11 (approval_amendment.rs +40/-9,
  most of it comments; shell.rs +2/-2); +44/-11 with `4a8f0a6c`'s comment;
- the round-2 code head `53557ed6`: **+73/-11** (approval_amendment.rs
  +71/-9, shell.rs +2/-2): Home and End, the early refusal, their comments;
- round 3's `802dd239`, comments only outside the tests: +76/-12.

That is under the brief's 300.

## fx's rule, paste form by paste form (`fx-src.txt`, written by `fx_src.sh`: every line read with `git show 481104fd:<path>`)

**Who takes a paste.** `beginPaste` picks the owner when the start marker
arrives (`input_paste_runtime.zig:105-115`):
- a question with its field selected: `.question_freeform`;
- a question at a choice: `.decision_prompt`;
- an approval with a draft open: `.approval_amendment`;
- an approval at a choice: `.decision_prompt`;
- sign-in code entry: `.auth_code`;
- otherwise the composer.

A `.decision_prompt` paste keeps no bytes and is dropped at its end
(`paste_framing.zig:110-113`; `input_paste_runtime.zig:270`). Before
`beginPaste`, the decoded paste start closes the composer's menus
(`app_input_runtime.zig:941-942`, `:1992-1996`).

**How many bytes are kept as they arrive.** The amendment and the question
are given the `max_input_len` that `beginPaste` is called with
(`input_paste_runtime.zig:116-120`). That is `input_limits.composer_bytes`,
8 MiB (`app_input_runtime.zig:637-644`; `paste_framing.zig:17-35`).
Past it a byte is counted as overflow, not kept (`paste_framing.zig:130-135`).

**Which bytes are content** (`paste_framing.zig:114-136`), per owner:
- composer and amendment: CR, LF, Tab and every byte from 0x20 up but 0x7f;
- question: the same, but a Tab is kept as a space (`:118-121`);
- sign-in code: printing bytes only.

Every other byte is dropped as it arrives: NUL, ESC, the other C0 bytes and
DEL. An ESC still starts the end marker's match.

**At the end** (`finishPaste`, `input_paste_runtime.zig:209-266`):
1. With overflow, the paste is refused whole, and the refusal is said
   (`:210-235`; for an amendment, `report(.approval_amendment)`).
2. The text is normalized (`:237-248`):
   - the composer and the question: `normalizeLineEndingsInPlace` alone;
   - the amendment: `normalizeApprovalAmendmentPasteInPlace` (`:19-25`);
   - a sign-in code: nothing.
3. Text that is not UTF-8 is refused whole:
   `Pasted text contains unsupported bytes and was not added to the draft.`
   (`isModelSafeText`, `text_utils.zig:28-31`; `input_paste_runtime.zig:250-266`).
4. The owner inserts it (`:268-359`).

**`normalizeLineEndingsInPlace`** (`text_utils.zig:82-98`): every CR becomes
LF, and an LF right after a CR is dropped. So CR LF is one LF and a lone CR
is one LF. LF CR is two LFs, and so is CR CR LF (fx's own cases,
`:1543-1562`). The amendment's rule then makes every LF and every Tab a space
(`input_paste_runtime.zig:21-23`). fx's own case: `one\r\ntwo\rthree\n\tfour`
is `one two three  four` (`:684-689`).

**The amendment's budget and its feedback.** `finishPaste` is called with
the owner's limit (`app_input_runtime.zig:804-807`). For the amendment
that is `decision_bytes`, 4096 (`paste_framing.zig:28-33`).
`insertAmendmentSlice` inserts the normalized text at the caret if
`canInsert(draft.len, bytes.len, 4096)`, and otherwise returns
`.limit_exceeded` (`approval_decision.zig:178-194`; `editor_state.zig:217-225`;
`approval_amendment.zig:53-58`). That is
`That edit exceeds the input limit and was not applied.` under `input`
(`input_paste_runtime.zig:321-327`; `input_limit_feedback.zig:35-41`). It is
said once per run of refusals, and every paste starts a run
(`input_paste_runtime.zig:127`).

**Each paste form, in the amendment:**
- **A large paste and an image path** are the composer's alone
  (`finalizePastedBlock`, `input_paste_runtime.zig:373-450`). In an
  amendment both are text, normalized like any other.
- **A selection**: a draft has none. fx's `DraftAction` has no selection
  (`approval_decision.zig:18-31`), so a paste goes in at the caret. Nor
  does xfx's: `approval::edits` admits no `SelectAll`, so `Shell::decide`
  swallows Super-A in a draft, and nothing else makes a selection there.

**Every other field fx shows:**
- **A question's field** keeps its breaks. Its Tab is a space from the
  capture (`paste_framing.zig:118-121`), and CR LF and a lone CR are LF
  (`input_paste_runtime.zig:238-241`). It refuses past 4096 in the same
  sentence (`:285-290`; `question_prompt.zig:572-585`).
- **The composer** keeps its breaks and its tabs
  (`input_paste_runtime.zig:238-241`). It is the
  only owner that collapses a large paste or reads an image path
  (`input_paste_runtime.zig:373-552`).
- **The `/` and `@` pickers** filter on the composer's own text. A paste
  closes their menus and then goes to the composer (above). The model
  picker borrows the composer too, and keeps an image path there as text
  (`input_paste_runtime.zig:362-384`).
- **`/rename`** takes its title as the command's argument, in the composer
  (`commands.zig:448`).
- **Sign-in code entry** (`.auth_code`) is fx's one other paste owner. It
  keeps printing bytes only (`paste_framing.zig:127`) and normalizes
  nothing (`input_paste_runtime.zig:246`), up to `max_manual_code_bytes`
  (`:118`). xfx has no sign-in (#88), so it has no such field.
- **No other text field** in fx takes a paste of its own.

So **only the amendment** turns breaks into spaces. That is the one rule
this unit copies.

## The copy (`src/tui/approval_amendment.rs`)

- `DraftPaste::byte` keeps `super::paste::accepted`'s bytes (unchanged;
  fx's composer and amendment set, `paste_framing.zig:124-126`). It now stops at the
  composer's `MAX_PASTE_BYTES`, as fx's capture does, not at 4096.
- `DraftPaste::finish`, in order:
  1. (round 2) refused when the bytes are more than twice the room left:
     the text can only be longer than that (Decisions);
  2. lossy decode (unchanged; Found below);
  3. `one_line`, which is `transcript::normalize`, the composer's own CR
     rule, then LF and Tab to a space;
  4. `fits(held, len) <= 4096`, the budget, last.
- (round 2) `Draft::apply`: Home and `LineStart` (Super-Left, Shift-Home)
  put the caret at 0, and End and `LineEnd` at the draft's length, as fx's
  `cursorHome`/`cursorEnd` (`approval_amendment.zig:84-92`;
  `src/ui/input/runtime.zig:82-83`), not at the caret line's edges.
- The refusal is unchanged: `INPUT_LIMIT` once per run, and every paste
  starts a run.
- Both planes answer through `approval::answered`, so the band and the
  review plane share the rule.
- Not changed:
  - the question's field (`question.rs`'s `pasted`), which is already fx's;
  - the composer (`paste.rs`), which keeps breaks and tabs as fx's does.

## The oracle (`fx/`, `captures.txt`)

`p5-approval-paste-spaces` runs at the edit's question.
- **Choice 1's draft** (`amend-*`): a paste of `ab` LF `cd` CR LF `ef` Tab
  `gh` CR `ij`, sent byte for byte with its markers by `send-keys -H`.
  (tmux's `paste-buffer` turns an LF into a CR.)
  - The paste is `ab cd ef gh ij` (`amend-pasted`), and a `z` follows it.
  - Super-Left and a `y` give `yab cd ef gh ijz`: the caret went to 0.
  - End, then ` and\tthe\nrest` through tmux's own paste-buffer, whose LF
    arrives as a CR, gives `yab cd ef gh ijz and the rest`.
- **Choice 3's draft** (`deny-*`), the budget:
  - 4093 bytes are pasted (`deny-held`);
  - a paste of `a` CR LF `b` is taken: 4096 bytes, `...xa b` (`deny-fits`);
  - a `c` is refused, said once (`deny-letter-refused`);
  - a paste of CR LF, one byte past the budget, is refused and said again
    (`deny-paste-refused`: two refusals).

## RED (`red.txt`, `red/`, `red-pairs.txt`)

- `red.txt`: the tests at `a5cf9567` over the base's code. 5 of the 13 run
  fail:
  - `a_paste_into_a_draft_is_one_line_as_fxs`: fx's own case is
    `one\r\ntwo\rthree\n\tfour` in the draft;
  - `a_paste_is_measured_against_the_budget_after_it_is_normalized`:
    4093 bytes for 4096 (`a\r\nb` refused);
  - `a_bracketed_paste_becomes_draft_text_and_never_an_entity`: the LF kept;
  - `super_left_right_and_the_draft_edges_in_an_amendment_are_fxs`:
    `zab\ncd` for `zab cd`;
  - `a_paste_into_an_amendment_is_one_line_on_either_plane_as_fxs`: the
    band's composed rows (`band_rows`) hold `ef    gh\rijz`: the draft is three
    rows, and a lone CR is inside the third. The painter (`frame::row_text`)
    drops it, so the terminal never gets it: `red-pairs.txt` shows that row
    painted as `ef    ghij` (round 2, opus SHOULD-2; round 1 read it as raw on
    the wire [wrong]).
- `red-pairs.txt` (`pairs.sh red`, the base's binary `v0.1.0-df45025`): of 9
  pairs, 1 SCOPE (`asked`) and 1 WINDOW (`deny-held`, before any CR). The
  other 7 differ:
  - every `amend-*` draft is three rows, `ab` / `cd` / `ef    ghij`, with
    the Tab painted as a tab stop;
  - Super-Left goes to the third line's start (`yef   ghijz`);
  - `deny-fits` says the refusal (the paste was refused at its 4097th byte),
    and `c` is then taken;
  - `deny-paste-refused` says it once, where fx says it twice.

## GREEN (`green-pairs.txt`, `green.txt`, `xfx/`)

- `green-pairs.txt` (`pairs.sh xfx`, the code head's binary `v0.1.0-9a73d0b`):
  of 9 pairs, 5 are SCOPE and 4 are WINDOW. None is DIFF.
  - **SCOPE**: `asked` and the four `amend-*` pairs differ only in rows 19
    and 21-25, `asked`'s rows. Those are the grant scope's sentence and the
    saved session's id (`approval-grant-scope`). Every draft is fx's, letter
    for letter.
  - **WINDOW**: the four `deny-*` pairs. Each side says the refusal the same
    number of times (0, 0, 1, 2), and the draft's last painted row ends in
    the same 12 characters (`xxxxxxxxxa b` from `deny-fits` on). What differs
    is how a 4093-byte draft is painted (Found below). fx's draft is one row,
    scrolled sideways to keep the caret in view; with the caret at the end,
    as here, the row ends at it. xfx's is three rows windowed on it. So xfx has two
    more rows, and every row under them moves (the rows `cells.py` lists for
    each: the draft, the scope rows, the divider and the hint).
- `green.txt`: round 1's 15 tests at `f438a5b6`, the last `src/` commit of
  round 1 (rounds 2 and 3 change `src/` again; their runs are under Round 2
  and Round 3), on both suites: 15 + 15 ok.

## Mutants (`mutants.txt`, `mutants.py`)

On a `--no-local` clone at `9a73d0ba`, under the suite lock, one mutant at
a time, each running `cargo test --lib -- tui::`: **8 of 8 killed**, and the
clone was clean after the run.

| mutant | killed by |
|---|---|
| M1 an LF stays an LF | the byte-class test, the budget test, the entity test, both shell tests, the page-key test |
| M2 a Tab stays a Tab | the byte-class test, the two-plane shell test |
| M3 no CR normalization (CR LF and a lone CR kept raw) | the byte-class test, the budget test, the two-plane shell test |
| M4 a run of breaks and tabs is one space | the byte-class test (fx's `three  four`, `ab  cd`), the budget test |
| M5 the paste refused at its 4097th raw byte (the base's bound) | the budget test |
| M6 the budget judged on the decoded text before it is normalized | the budget test |
| M7 the question's field takes the amendment's rule | four tests that paste a multi-line answer into a question, the page-key test, and one more (`+1 more`) |
| M8 the composer takes the amendment's rule | 26 tests, the event loop's card and preview tests first (the runner lists 6, `+20 more`) |

M6's and M7's lists also name
`tui::worker::tests::a_stop_in_the_catalog_wait_of_a_turn_that_would_ask_twenty_questions_ends_it_as_the_stop`,
a catalog-wait test (the known flake class). Each mutant is killed without
it.

`b9afbe3b`'s two pins came after the run. They would kill M1 and M3 again
(each pastes an LF), and no mutant is left that they alone would kill.

## Sweep (`sweep-base.md`, `sweep-head.md`, `sweep-compare.txt`)

The command:

```
scripts/fxcap/sweep.sh --only 'approval*' --only 'p5-approval*'
  --only 'p5-super-arrows*' --only 'question*' --only 'p5-question*'
  --only 'paste*' --only 'p5-input-paste*'
```

These are the brief's six families plus `p5-input-paste*`. That family holds
the one older scenario that pastes into an amendment, `p5-input-paste-amend`.
The sweep ran at 40x120 with `--jobs 2`, on this branch's scenarios
(`b9afbe3b`) and its unchanged ledger. It ran once with the base's binary
(`v0.1.0-df45025`) and once with the code head's (`v0.1.0-9a73d0b`), both in
one suite-lock slot. That is 22 scenarios and 211 pairs.
APPROVAL-VERTICAL-SPELLINGS' `cmp_sweeps.py` (unchanged, run from its
folder) compares them pair by pair (`sweep-compare.txt`):

- **base 107 / 211, head 107 / 211, drops 0.** No pair changes class.
- **Only this unit's scenario changes**, in its uncaused rows. Every pair is
  `approval-grant-scope`'s on both sides, so none can pass.
  - The four `amend-*` pairs go from 4 uncaused rows to 0: the draft is fx's.
  - `deny-fits` goes from 6 to 4, and `deny-paste-refused` from 5 to 4. The
    refusal rows now agree. The 4 left in each `deny-*` pair are the draft's
    window (Found).
- **The other 21 scenarios' 202 pairs** keep their class, causes and
  uncaused counts. Two have uncaused rows on both sides, older than this
  unit: `p5-input-paste-amend`'s `end` (1) and `p5-super-arrows-approval`'s
  `amend-super-right` (2, COMPOSER-SUPER-ARROWS' capture-trim artifact).
  The paste into an amendment in `p5-input-paste-amend` is `new`, which
  has no break, so this unit does not change it.
- No scenario was refused, failed or timed out (`sweep-base.md`,
  `sweep-head.md`: ran 22, failed 0, timeout 0).

## Gate

GATE: dispatcher. Round 3's head is gated by the dispatcher after this
report; I ran no full gate.

The previous gate, at `ac276d9f` (round 2's head), is the dispatcher's
(`round3/gate-ac276d9f.txt`): 22 steps, every one exit 0, `cargo test` 5729
passed and 0 failed, with `fault-injection` 5766 and 0, smoke-tui 1167
checks and 0 failures, the tree unchanged after.

The one before it, at `e26ce67b` (round 1's head), is also the dispatcher's
(`round2/gate-e26ce67b.txt`): 22 steps, every one exit 0, `cargo test` 5723
passed and 0 failed, with `fault-injection` 5760 and 0, smoke-tui 1167
checks and 0 failures, the tree unchanged after.

## Refs (`refs.txt`, `refs.py`)

`refs.py` is COMPOSER-SUPER-ARROWS', with its paragraph about this unit's
files. It checks the ledger's refs into the files this unit changes, by
content, against the base (`refs.py df45025c`). Of 16, 6 had moved: the
`tools: {} advertised` line in `shell.rs`' tests, 43422 to 43492, below
this unit's new shell tests. `--fix` re-pointed them. `13f1436f` wrote
43471, the number before `2804b108`'s test block above it; `f438a5b6`
re-pointed them to 43492, and round 2's C1 test moved them to 43544
(`276fc5d7`). Round 2 adds the two refs of `amend-draft-window`, into
`approval_amendment.rs:82` (`MAX_DRAFT_ROWS`). Round 3 splits that cause
in two and points each at `MAX_DRAFT_ROWS` (now `:84`) and `Draft::viewport`
(`:217`), six refs in all. `refs.txt` reads 22 checked, 0 wrong.

## Parity (`docs/parity.md`)

- **The approval row's draft paragraph states the copied rule.** It covers
  the bytes kept, CR LF and a lone CR to LF, LF and Tab to a space, the
  8 MiB capture and the 4 KiB budget on what the paste becomes. It says the
  composer and the question keep their own rules, names the sign-in code
  entry with its rule (round 2, gpt6 SHOULD 1), and says no other field
  takes a paste. It names this receipt, and what was there before: the
  breaks kept, a lone CR left in the composed rows (dropped by the painter;
  round 2, opus SHOULD-2), and the refusal at the 4097th raw byte.
- The APPROVAL-VERTICAL-SPELLINGS sentence that proposed this unit as owner
  ("a bracketed paste into a draft keeps its line breaks and tabs ...") is
  now the fact. A draft is one line, so its Home and End reach the draft's
  edges.
- The COMPOSER-SUPER-ARROWS clause ("which xfx lets hold a line break") now
  says that Home and Super-Left are the draft's start, as fx's
  `.cursor_home` is.
- No row names APPROVAL-PASTE-SPACES as an owner. It appears only as the
  attribution of the rule.
- Three findings are recorded with proposed owners (Found).

## Decisions

- **The capture bound is fx's 8 MiB, not a smaller derived one.** Any raw
  paste over 8192 bytes would be refused anyway, since CR LF at most halves
  it. fx says the same sentence for an overflow and for the budget, so a
  smaller bound would change nothing on screen. The copy is still literal,
  so the code reads like the fx lines it cites.
- **`transcript::normalize`, not a second CR rule.** `paste.rs`' module doc
  already sends every CR rule there. fx's `normalizeLineEndingsInPlace` is
  that rule on bytes. CR is ASCII, so decoding first changes nothing.
- **Lossy decoding is left as it is.** fx's UTF-8 refusal is one rule for
  three owners, the composer, the question and the amendment. Copying it
  for the amendment alone would split it (Found).
- **The test that types an LF into a draft stays**
  (`super_up_and_down_are_the_whole_drafts_edges`). It pins the draft's
  edges independently of the paste. Its comment now says that no key types
  one. Round 2's Home/End test does the same.
- **The early refusal is exact** (round 2, opus NIT-7). Decoding never makes
  text shorter: a scalar keeps its bytes, and an ill-formed run of at most
  three bytes becomes one three-byte U+FFFD. Making it one line shortens
  only a CR LF, two bytes to one. So the text is at least half the bytes,
  and a paste of more than twice the room can never fit. The refusal is
  the same `None`, so `Drafts::edit` says the same sentence in the same
  once-per-run order; the test that compares the two paths shows it. It
  saves decoding 8 MiB into about 24 MiB only to refuse them.
- **Home and End call the editor's draft edges** rather than adding a
  draft-only editor mode: `Editor::apply`'s `DraftStart`/`DraftEnd` are
  already the caret at 0 and at the length, and a move records no delta.

## Round 2 (gpt6 SHOULD 1-2, opus SHOULD-1-2 and NIT-1-7)

| item | where | what |
|---|---|---|
| the 8 MiB capture bound (both SHOULD) | `the_capture_holds_eight_mebibytes_as_fxs_and_refuses_the_next_byte`, `a_cr_lf_paste_of_twice_the_room_is_taken_as_spaces` (`src/tui/approval_amendment.rs` tests) | white-box: after `MAX_PASTE_BYTES` bytes the buffer holds exactly that many and is not refused; one more refuses it, `finish` is `None`, the next paste is its own; through the draft, `INPUT_LIMIT` once. 8190 bytes of CR LF become 4095 spaces, 8192 become 4096, and with one byte held they are refused |
| sign-in code (gpt6 SHOULD 1) | `docs/parity.md` (`45ced900`); fx's rule above | named, with its rule |
| the base's CR (opus SHOULD-2) | `docs/parity.md` (`45ced900`); this README's intro and RED | a lone CR stayed in the composed rows, and the painter dropped it |
| C1 (NIT-1) | `c1_controls_in_a_paste_are_kept_in_the_draft_as_fxs`; `a_c1_control_in_an_amendment_paste_reaches_the_draft_and_no_painted_row` (`src/tui/shell.rs`) | U+0085 and U+009B kept in the draft and sent, as fx's; no row `frame::row_text` writes holds one, on either plane |
| Home and End (NIT-6) | `Draft::apply` (`53557ed6`); `home_and_end_in_a_draft_are_its_edges_as_fxs_cursor_home` | the draft's edges directly, as fx's `cursorHome`/`cursorEnd` |
| the early refusal (NIT-7) | `DraftPaste::finish` (`53557ed6`); `a_paste_refused_on_its_bytes_alone_is_said_as_one_refused_on_its_text` | taken: the same draft, sentence and once-per-run order on both paths (Decisions) |
| NIT-2, NIT-3 | `docs/parity.md` (`45ced900`); Found | the composer's lossy decode; the question's encoded budget |
| NIT-4 | ledger cause `amend-draft-window` (`276fc5d7`); `round2/rescore-compare.txt` | the `deny-*` pairs' draft rows caused; their uncaused rows go from 4 to 0 each, drops 0 |
| NIT-5 | this README | the table, :16, the refs paragraph, `## Gate` |

- **RED** (`round2/red.txt`): `0e2b242b`, tests only, over `e26ce67b`'s code.
  Of the 17 run, 1 fails: Home in a two-line draft gives `(1, 0)` for
  `(0, 0)`. The other new tests pin rules the code already had. Their RED
  is the forced mutants below, each of which fails at least one of them.
- **GREEN** (`round2/green.txt`): the unit's 21 tests at the head, 21 + 21
  on both suites; every `tui::` lib test at `53557ed6`'s `src/` under the
  lock, 3119 passed and 0 failed (round 1's run was 3113; +6 tests).
- **Mutants** (`round2/mutants.txt`, `round2/mutants.py`): a `--no-local`
  clone at `53557ed6`, under the suite lock. **16 of 16 killed**: round 1's
  eight and eight new, the clone clean after the run.
  - M9 (bound deleted) is killed by the white-box test.
  - M10 (4098) and M11 (8191) are killed by it and by the CR LF test.
  - M12 (early refusal at exactly twice the room) is killed by the 8192-byte
    CR LF paste. M13 (early refusal taken as empty and silent) is killed by
    that and by the early-against-late test.
  - M14 and M15 (Home and End the caret line's edges) are killed by the
    Home/End test. M16 (C1 dropped) is killed by both C1 tests.
- **Ledger** (`round2/rescore-compare.txt`, `round2/test-sweep.txt`): round
  1's head sweep rescored with the new cause: 107/211 pass on both, drops 0,
  pairs with uncaused rows 6 to 2 (the two older ones). `test-sweep.sh`: 72
  and 27 tests, OK.
- **Checks**: `fx-src-check.txt` (131 citations over this README and the
  added lines, 0 outside; two ranges added, `approval_amendment.zig:84-92`
  and `input_limit_rejection.zig:1-30`); `cargo fmt --check` and both clippy
  runs clean at `53557ed6`; `refs.txt` 18 checked, 0 wrong.

## Round 3 (round-2 NITs: gpt6 NTH 1-2, the dispatcher's A-G)

| item | where | what |
|---|---|---|
| A | `a_cr_lf_paste_of_twice_the_room_is_taken_as_spaces`' comment | 8190 bytes catch a bound from 4098 to 8189; 8190 and 8191 are caught by the 8192-byte paste |
| B | `the_capture_holds_eight_mebibytes_as_fxs_and_refuses_the_next_byte` | `assert_eq!(cap, 8 * 1024 * 1024)`, fx's `composer_bytes` (`paste_framing.zig:18`) |
| C | `a_paste_refused_on_its_bytes_alone_is_said_as_one_refused_on_its_text` | three Backspaces before the third paste, so it has 3 bytes of room and takes the early and the late path again (round 2's third paste had none, so both took the early one) |
| D | the ledger: `amend-draft-window` (`deny-held`) and `amend-draft-window-tail` (the other three) | a lost `a b` tail is a row no cause covers (`round3/ledger-split.txt`) |
| E | the two causes' `why` | the 4093-byte draft ends in `x`; `...xa b` is the 4096-byte one; `Draft::viewport` cited |
| F | `src/tui/approval_amendment.rs` module doc, `src/tui/shell.rs` `Shell::decide` | Home and Super-Left reach the draft's edges through `Draft::apply`, not through "a draft holds no line break" |
| G | `srcsize.txt`, this README (size, GREEN, table), `docs/parity.md` | the size from `a5cf9567` to `53557ed6` (+73/-11) and to `802dd239` (+76/-12); round 1's GREEN line; one doubled "where upstream's" |

- **GREEN** (`round3/green.txt`): the amendment's 16 tests and the
  three shell amendment-paste tests at `802dd239`, 19 + 19 on both suites.
- **M12 and M13 against the changed test** (`round3/m12-m13.txt`, a
  `--no-local` clone at `802dd239`, under the suite lock): both killed. M13
  (the early refusal takes the paste as empty and silent) is killed by the
  changed early/late test, and by the CR LF test. M12 (early at exactly
  twice the room) is killed by the CR LF test's 8192-byte paste. The
  early/late test cannot see M12: refusing a paste at exactly twice the
  room early or late gives the same `None` there.
- **The ledger split** (`round3/ledger-split.txt`). Round 1's sweep captures
  were lost to the /tmp cleanup, so `p5-approval-paste-spaces` was swept
  again at 40x120, against xfx built at `802dd239` (banner
  `v0.1.0-802dd23`). Its 9 pairs were classified three ways:
  - with `e26ce67b`'s ledger, which has no draft-window cause;
  - rescored with `ac276d9f`'s ledger, which has the one cause;
  - rescored with round 3's, which has the two.

  From no cause to the split, each `deny-*` pair goes from 4 uncaused rows
  to 0, with drops 0. From one cause to two, nothing changes but the causes'
  names. `round3/tail_check.py` asks the ledger's own `covering` about each
  draft row, as captured and with its `a b` tail lost. Round 3's ledger
  gets all 10 cases right; `ac276d9f`'s left four rows without a tail
  covered.
- **Checks**: `fx-src-check.txt` 134 citations, 0 outside; `refs.txt` 22
  checked, 0 wrong; `cargo fmt --check` and both clippy runs clean at
  `802dd239`'s `src/`; `test-sweep.sh` OK (72 and 27 tests) on the split
  ledger.

## Found, not changed (recorded in `docs/parity.md`, owners proposed)

- **AMEND-DRAFT-WINDOW.** A draft longer than its row is painted on up to
  three rows windowed on the caret (`MAX_DRAFT_ROWS`). fx's is one row
  scrolled sideways to keep the caret in view, with what follows the caret
  after it while there is room (`approvalDraftWindow`,
  `approval_ui.zig:1601-1650`: `before` cut from the left until the caret
  fits, `after` cut to the width left, `:1626-1640`).
  This scenario's `deny-*` pairs show it, and since round 2 the ledger says
  so (kind unit and defect, owner AMEND-DRAFT-WINDOW). Since round 3 it is
  two causes: `amend-draft-window` for `deny-held` (fx's row ends in `x`)
  and `amend-draft-window-tail` for the other three (fx's row ends in
  `a b`, xfx's two full rows of `x` and a last row ending `a b`), so a draft
  that lost its `a b` is a row no cause covers (`round3/ledger-split.txt`). Before this unit a pasted break could make
  a draft that tall too; now only its length can.
- **PASTE-UNSUPPORTED-BYTES.** A paste that is not UTF-8 is decoded with its
  bad bytes replaced, in a draft, a question's field and the composer
  alike. The composer's decode is lossy on purpose (`src/tui/paste.rs`,
  `finish_with`: "a paste in an encoding this session does not read must
  not be lost whole to one bad byte"), so copying fx is a decision about
  that, not a slip. fx refuses it whole (`input_paste_runtime.zig:250-266`).
- **QUESTION-PASTE-LIMIT.** A question's field holds its paste with no bound
  while it arrives (`question.rs`, `Act::PasteByte` pushes every byte). fx's
  stops at 8 MiB (`paste_framing.zig:130-135`). A paste or a character past
  the field's budget is refused silently; that budget is 4096 bytes **once
  encoded** for the tool's answer (`MAX_FREEFORM_ENCODED_BYTES`,
  `src/tools/question.rs`), where fx's is 4096 bytes of the field's own text
  (`question_prompt.zig:572-585`). fx's says
  `That edit exceeds the input limit and was not applied.`
  (`input_paste_runtime.zig:285-290`; `app_input_runtime.zig:1156`). The
  unbounded buffer is more than a parity gap: it is a memory hole a
  terminal can feed, the class the composer's and the draft's capture
  bounds exist for (`super::paste`'s module doc). So this one should be
  queued as a defect, not only filed [추론: no run fed it; the code path is
  `QuestionPanel::apply`'s `Act::PasteByte` arm, `held.push(byte)` with no
  check].
