# APPROVAL-PASTE-SPACES receipt: an amendment's paste is one line, as fx's

fx v0.0.16 normalizes a paste into an approval's amendment draft before it
inserts it. CR LF and a lone CR become LF, and then every LF and every Tab
becomes one space (`normalizeApprovalAmendmentPasteInPlace`,
`input_paste_runtime.zig:19-25@481104fd`). Then it checks the draft's 4096
bytes against what the paste has become. xfx kept the breaks and tabs, so a
draft could hold several lines, and it painted a pasted CR raw. It also
refused a paste at its 4097th byte as the bytes arrived, so a CR LF counted
two bytes. This unit copies fx's rule for the amendment. The composer and a
question's field keep their own rules, which are already fx's.

| commit | what |
|---|---|
| `a5cf9567` | RED: the unit tests and the shell test, the COMPOSER-SUPER-ARROWS amendment block flipped to fx's 0, scenario `p5-approval-paste-spaces`. Tests and the scenario only |
| `9a73d0ba` | **the code head**: `DraftPaste` (the 8 MiB capture bound, `one_line`, the budget after it), the module's and `MAX_FEEDBACK_BYTES`' docs, one comment in `Shell::decide`; MODIFIED-PAGE-KEYS' page-key test pastes a one-line draft now |
| `b9afbe3b` | tests only: an image path and a long paste in a draft; the scenario's `cursor` line dropped (it read where each binary parks the terminal's cursor) |
| `1f5d32a3`, `bf518f5f` | `docs/parity.md`; this folder |
| `4a8f0a6c` | a comment only: the module doc names the decoder's half of "no key types a break into a draft" (`Input::Text` is never a control character, `src/tui/input.rs`) |
| the commits after | this folder; no `src/` change |

The source change outside the test modules is +42/-11 over two files
(`srcsize.txt`, `srcsize.py`): approval_amendment.rs +40/-9 (most of it
comments), shell.rs +2/-2; +44/-11 with `4a8f0a6c`'s comment. That is under
the brief's 300.

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
- **No other text field** in fx takes a paste of its own. The sign-in code
  is printing bytes only and is not normalized.

So **only the amendment** turns breaks into spaces. That is the one rule
this unit copies.

## The copy (`src/tui/approval_amendment.rs`)

- `DraftPaste::byte` keeps `super::paste::accepted`'s bytes (unchanged;
  fx's composer and amendment set, `paste_framing.zig:124-126`). It now stops at the
  composer's `MAX_PASTE_BYTES`, as fx's capture does, not at 4096.
- `DraftPaste::finish`, in order:
  1. lossy decode (unchanged; Found below);
  2. `one_line`, which is `transcript::normalize`, the composer's own CR
     rule, then LF and Tab to a space;
  3. `fits(held, len) <= 4096`, the budget, last.
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
    band's rows hold `ef    gh\rijz`. That is a pasted **CR in a painted row**,
    which goes to the terminal raw.
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
- `green.txt`: the unit's tests at the head.

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

## Gate (`gate.txt`)

GATE-PENDING

## Parity (`docs/parity.md`)

- **The approval row's draft paragraph states the copied rule.** It covers
  the bytes kept, CR LF and a lone CR to LF, LF and Tab to a space, the
  8 MiB capture and the 4 KiB budget on what the paste becomes. It says the
  composer and the question keep their own rules, and that no other field
  takes a paste. It names this receipt, and what was there before.
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
  one.

## Found, not changed (recorded in `docs/parity.md`, owners proposed)

- **AMEND-DRAFT-WINDOW.** A draft longer than its row is painted on up to
  three rows windowed on the caret (`MAX_DRAFT_ROWS`). fx's is one row
  scrolled sideways to keep the caret in view, with what follows the caret
  after it while there is room (`approvalDraftWindow`,
  `approval_ui.zig:1601-1650`: `before` cut from the left until the caret
  fits, `after` cut to the width left, `:1626-1640`).
  This scenario's `deny-*` pairs show it. Before this unit a pasted break
  could make a draft that tall too; now only its length can.
- **PASTE-UNSUPPORTED-BYTES.** A paste that is not UTF-8 is decoded with its
  bad bytes replaced, in a draft, a question's field and the composer
  alike. fx refuses it whole (`input_paste_runtime.zig:250-266`).
- **QUESTION-PASTE-LIMIT.** A question's field holds its paste with no bound
  while it arrives (`question.rs`, `Act::PasteByte` pushes every byte). fx's
  stops at 8 MiB (`paste_framing.zig:130-135`). A paste or a character past
  the field's 4096 bytes is refused silently. fx's says
  `That edit exceeds the input limit and was not applied.`
  (`input_paste_runtime.zig:285-290`; `app_input_runtime.zig:1156`). The
  unbounded buffer is a memory hole a terminal can feed.
