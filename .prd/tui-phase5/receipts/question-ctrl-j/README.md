# QUESTION-CTRL-J receipt: C-j answers a model question as Enter does

fx v0.0.16 answers a model question on C-j (byte `0x0a`) exactly as on Enter,
at a choice and in the freeform field; xfx's question swallowed C-j in every
spelling. APPROVAL-CTRL-J (closed at `1a0adaec`, on `fix/p5-approval-ctrl-j`)
found the gap and named this unit (its README, "Not taken here"). This unit
copies fx: `question::Act::of` reads `Action::ControlJ` as `Act::Submit`, the
act Enter is. The base is train 13's head `afd891af` (`train/p5-1110b`), which
holds QUESTION-WORD-LEFT and QUESTION-RESUME.

| commit | what |
|---|---|
| `4f3c1515` | RED: five shell tests and one `question.rs` test, the scenarios `p5-question-ctrl-j` and `p5-question-ctrl-j-panel` with their turns; tests only in `src/` |
| `cab6870c` | **the fix**: `Act::of`'s one arm; `Act::Submit`'s and `Action::ControlJ`'s docs; a comment on `/mcp`'s held-Enter rule (since rewritten, `e66c5490`) |
| `b7861e54` | `docs/parity.md`'s sentence and the nine ledger refs the unit moved; no `src/` change |
| `6d8dc766` | the tests' key byte renamed `MODEL_QUESTION_KEY` (from `KEY`, a name another branch could add to the same `mod tests`), and the six ledger refs it moved; test code only |
| `244d3248` | the parity sentence's readiness clause narrowed to what was read; docs only |
| `ae121e4b` | RED: a C-j the model's question takes counts for `/mcp`'s held-Enter rule as Enter does; one the menu takes does not; tests only |
| `e66c5490` | **the second fix**: `Shell::consume_inputs` counts that C-j (Decisions, below) |
| `87daec96` | **the code head**: the held rule's counting said in `/mcp`'s row and the C-j sentence, and the nine ledger refs the fix moved; no `src/` change. The captures, the mutants, the head sweep and the gate name it |
| `7d585321` … `181dd86b` | round 1's receipt; no `src/` change |
| `998a434b` | **round 2** (the reviews of `181dd86b`: gpt6 APPROVE, opus APPROVE-WITH-FIXES, no MUST on the branch): tests and docs only -- the `/model` picker test, the held-rule cases in one loop, the matrix's doc, `McpEnters`' doc, the meta-prefix gap owned in `docs/parity.md`, fourteen ledger refs (numbers only); no product line (`diffstat.txt`'s last lines) |
| the commits after | round 2's receipt: the panel scenario recaptured with `gwlog`, `sent.txt`, the pulse row, mutants M10-M13, the probe, the checks; no `src/` change |

Captures, both scenarios at 40x120: `fx/` is fx v0.0.16 (sha256 `8e6cda2e…`,
`FX_AUTO_UPGRADE=0`; `capture.sh` checked its bytes after each run), `red/`
the base's binary (row 1 `x𝒇x v0.1.0-afd891a [debug]`, built at `afd891af`
before any commit), `xfx/` the code head's (row 1 `x𝒇x v0.1.0-87daec9
[debug]`, built at `87daec96` after that commit). Hashes, banners and the guard lines:
`binaries.txt`. The request bodies stay out (their system prompt names the
run's directory); `sent.txt` is each run's answer documents, read from them by
`sent.py`.

## fx's rule (`fx-src.txt`, written by `fx_src.sh`: every line read with `git show 481104fd:<path>`)

**The byte.** A byte read with no sequence open is a raw byte (`appendRaw`,
`terminal_action_decoder.zig:207-212`). With a question up, `withQuestionInput`
gives it a question action (`src/ui/input/runtime.zig:229-259`, applied to
every byte, `:664-674`): `questionActionFromRawByte` asks the focused-editor
route first only while the freeform slot is marked, and that route keeps no
newline (`:199-209`; `fromFocusedEditorControlByte`, `shortcuts.zig:31-41`, of
`'\n' => .insert_newline`, `:26`), so `0x0a` falls to `questionActionFromByte`,
where `'\r', '\n' => .submit` (`src/ui/input/runtime.zig:141`). The question's
branch of `routeActiveModalInput` applies it and returns for every byte
(`app_input_runtime.zig:1136-1160`). `.submit` is `advanceOnDecision`
(`question_prompt.zig:329`): it stores the marked choice, or the field's text as
it stands (`:641-648`), then moves to the next unanswered question, cycling
(`:657-665`), or ends the batch once none is left (`:666-667`), which is
submitted at once (`.all_decided => submitQuestionBatch`,
`input_question_runtime.zig:47-50`). So:

| where | fx v0.0.16 | xfx base | xfx head |
|---|---|---|---|
| at a choice | takes the marked choice, as Enter (`src/ui/input/runtime.zig:141`; `question_prompt.zig:329`) | nothing | **takes it, as Enter** |
| in the freeform field | sends the field as it stands, as Enter, and never inserts a newline: the byte is `.submit` there too (`src/ui/input/runtime.zig:199-209`), and the field takes no byte under `0x20` (`insertFreeformByte`, `question_prompt.zig:388`); the answer is the field's text (`:641-648`) | nothing | **sends it, as Enter** |
| several questions | the next unanswered question, cycling, or the batch sent once none is left (`question_prompt.zig:657-667`); no review step (`input_question_runtime.zig:47-50`) | nothing | **the same** |
| in a read with other keys | each byte decoded against the prompt the key before left (`prepareTerminalDecode` per byte, `app_input_runtime.zig:741`, `:754`; `question_freeform_selected`, `:576`), so `3`, letters and C-j in one write open the field, type and send | nothing | **the same**, and a C-j behind a paste's end marker dropped as Enter is |
| a menu or picker open under the question | the question takes the byte: its branch runs before the MCP menu and the compact menus (`/status`'s panel among them, whose C-j is the next row; `app_input_runtime.zig:1186-1216`, `:1205`), and the picker's C-j yields (`composerPickerSurfaceVisible`, `:1110-1116`; `:873` before `:879`) | nothing to either | **answers the question; the panel does not move** |
| a readiness check | none at a question (`input_question_runtime.zig:16-57`); fx's is a file approval's (`input_approval_runtime.zig:191-218`) | none | **none** |

**The spellings.** A kitty `CSI <k>;<m>u` or an xterm `CSI 27;<m>;<k>~` (read
into a third field and handed to the same function, `escape_parser.zig:668-674`,
`:740-752`) of `j` or `J` with Ctrl held is the byte `0x0a`
(`kittyUnicodeKeyAction`, `escape_parser.zig:255-258`), whatever else is held:
no branch ahead of that rule takes a `j` (`:208-254`), and Caps and Num Lock are
masked first (`:187`); a kitty report's press and repeat act, its release is
`.ignore` (`:774-777`). The replayed byte takes the raw route above
(`app_input_runtime.zig:950-953`, `:746-756`). This is APPROVAL-CTRL-J's finding
for the approval, read again at the same lines. Not C-j to fx: a kitty release;
`ESC LF` (Alt-C-j without a key protocol), which is `.insert_newline`
(`escape_parser.zig:515-519`), no question action (`questionActionFromDecoded`,
`src/ui/input/runtime.zig:211-227`, whose freeform map has no newline,
`:155-176`) and nothing in the question's switch
(`input_question_runtime.zig:149-200`); and Alt-j, which is `.ignore`
(`escape_parser.zig:265`). xfx's decoder already read every one of them as fx
does: each fx spelling is `Action::ControlJ`, each of the three is not
(`every_ctrl_j_spelling_…` and `keys_fx_does_not_read_as_ctrl_j_…` below drive
them through it).

**The captures agree** (`fx/`): C-j at the first of three questions moves to
the second (`ctrl-j`); Down and C-j in one write take `No` and move to the third
(`down-ctrl-j`); `3ab` opens the field and types (`field`), and C-j sends the
batch, `Thorough`, `No`, `ab` (`field-ctrl-j`; `sent.txt`); a kitty Ctrl+J takes
`Keep` and moves on (`kitty-ctrl-j`); a kitty release and `ESC LF` leave the
second question as it was (`not-ctrl-j`); an xterm Ctrl+J sends the batch
(`xterm-ctrl-j`); Tab then C-j answers the second question and cycles back to
the first (`tab-ctrl-j`); and `3xy` with C-j in one write sends `xy`, `Three`
(`end`). With `/status` open when the question arrives, C-j answers it and the
panel is back as it was (`p5-question-ctrl-j-panel`'s `question-ctrl-j`, `end`).

## What xfx did at the base

`question::Act::of` had no arm for `Action::ControlJ`
(`src/tui/question.rs:228-258` at `afd891af`, `_ => None`), and
`Shell::answer` returns on `None` (`src/tui/shell.rs:10027-10030` at
`afd891af`), so C-j at a question did nothing in every spelling, at a choice
and in the field, while no surface below the question saw it either (the
question has the keys, `Shell::consume_inputs`).

## The change (`diffstat.txt`: outside the `mod tests` blocks +31 / -9 -- the `Act::of` arm and the held rule's `answers` match, the rest comments; eight `#[test]`s added, none removed)

- **`question::Act::of`** (`src/tui/question.rs:260` at the head):
  `Action::Submit | Action::ControlJ => Some(Self::Submit)`. Every place a
  question's Enter goes, C-j goes: `QuestionPanel::apply`'s `Act::Submit` (the
  marked choice, or the draft), `record` (the next unanswered question, cycling,
  or the batch), `Shell::answer` (the panel down, the record, the answer sent).
  The shell's routing needed nothing for it: `unspelled` and `folded` leave `ControlJ` alone,
  the paste quarantine is the decoder's and byte-level, and no readiness check
  stands at a question on either side.
- The decoder is unchanged: every spelling above was already `ControlJ`.
- **`Shell::consume_inputs`** (`e66c5490`): `/mcp`'s held-Enter rule counts a
  C-j the model's question takes, as the Enter it answers as; a C-j the menu
  takes still counts for nothing (Decisions, below). Six lines for three.
- Docs: `Act::Submit`, `Action::ControlJ` (`src/tui/input.rs:348-358`), and the
  held rule's comment.

## Decisions (each reversible)

- **`/mcp`'s held-Enter rule counts a C-j the model's question takes, as the
  Enter it answers as** (`e66c5490`; `McpEnters`, the `answers` match above
  `let held` in `Shell::consume_inputs`). The brief: whatever gating xfx puts
  on Enter at the question applies to C-j too. The rule's own doc says why it
  is not only about a held key's repeats: "timing cannot tell a slow key repeat
  from a second tap, and on a question that removes, rejects or approves the
  safe reading wins" (opus-Md R2), and
  `an_enter_another_surface_took_counts_for_the_held_rule` pins the shape (and,
  since round 2, the same shape for C-j) --
  the model's panel took the answering key, the menu's question was painted
  again, a tap 200 ms later is not an answer. The window is how soon a person
  can have read the repainted question, whichever key answered the one before.
  And the newline habit is the way in: a C-j meant as a new line answers the
  panel, and the Enter the user was already typing lands on `Remove`. A C-j the
  menu itself takes is its Down, nothing at a confirmation, and counts for
  nothing (`a_ctrl_j_the_menu_took_does_not_count_for_the_held_rule`).
  `McpEnters`' doc and `enter`'s say so too (round 2).
  **This differs from APPROVAL-CTRL-J**, which left the approval's C-j
  uncounted on a "the rule is about repeats" reading; the merge of the two is
  where to make them one, and by the rule's own doc the approval should count
  it too (its readiness check holds only a C-j before the frame, not one after
  it).
- **The newline habit, as in fx** (below, Safety): kept, because fx does it; the
  revert is the one arm.
- **Names apart from APPROVAL-CTRL-J's**: that branch adds `const CTRL_J` and
  tests named `ctrl_j_…_at_a_question` to the same `mod tests`; this unit's are
  `MODEL_QUESTION_CTRL_J` and `…model_question…`, so the two merge without a
  duplicate.
- **`fx_src.sh` reads a diff line for its comment only** (the text after `//`
  or a leading `#`): APPROVAL-CTRL-J's check took `":1"` in the tests' code and
  `"delayMs":100` in the turns for bare refs after a `.zig` citation.

## RED (`red-tests.txt`, `red.txt`)

At `4f3c1515` (`git diff afd891af 4f3c1515 -- src` adds tests only), five tests
fail and one passes:
- `ctrl_j_answers_a_model_question_as_enter_does_at_a_choice_and_in_the_field`:
  six scripts (at a choice; a move first; the field; an empty field; Tab first,
  cycling back; the field sending the batch), each run with Enter and with the
  13 spellings of `MODEL_QUESTION_CTRL_J` (the byte; kitty `j` and `J`; Ctrl
  with Shift, Alt, Super and all three; Caps Lock; Num Lock; a press; a repeat;
  two xterm forms), compared write for write (what was sent, which question is
  shown, the caret's row): Enter sent each script's batch, every spelling
  nothing.
- `ctrl_j_in_one_read_answers_a_model_question_as_enter_does_in_that_read`:
  Down and two C-j in one write; `3ab`, C-j and `2` in one write; a C-j behind
  a paste's end marker, then one of its own. The first two differ at the base;
  in the third the first two writes match (behind the marker both keys are
  dropped) and the third does not (`red-tests.txt`'s last entry).
- `every_ctrl_j_spelling_fx_reads_as_the_byte_answers_a_model_question`: the
  whole matrix without a meta prefix, 1025 spellings (the byte; `j` and `J`
  with Ctrl and each subset of Shift, Alt, Super, Hyper and Meta, in each lock
  state, as kitty with no event type, a press and a repeat, and as xterm), each
  at a choice and in the field: 2050 of 2050 answered nothing. The name's
  "every" is that matrix: the meta-prefixed spellings are not in it (Round 2,
  below).
- `ctrl_j_at_a_model_question_over_the_status_panel_answers_the_question_not_the_panel`:
  C-j scrolled the panel before the question (its key) and answered nothing at
  the question (13 spellings).
- `ctrl_j_is_enter_to_a_model_question_and_the_newline_keys_are_not`
  (`src/tui/question.rs`): `Act::of(ControlJ)` was `None`.
- `keys_fx_does_not_read_as_ctrl_j_do_nothing_at_a_model_question` passes, by
  construction: a kitty release, `ESC LF` and kitty Alt-j are nothing to fx and
  to the base alike, at a choice and in the field.

The held rule's pair, at `ae121e4b` (`red-tests-held.txt`; the code there is
`87daec96`'s less the second fix):
`a_ctrl_j_the_model_question_took_counts_for_the_held_rule_as_enter_does`
fails -- "C-j then Enter answered Remove", the file without `keep` -- and
`a_ctrl_j_the_menu_took_does_not_count_for_the_held_rule` passes there, as it
must at every head (it pins the scope). Round 2 folded the first into the older
`an_enter_another_surface_took_counts_for_the_held_rule`, now a loop over Enter
and C-j (its name kept: `../mcp-menu/mutants_md.py` runs it by name).

The pairs at the base (`red.txt`, `pairs.py red`): **3 of 14** identical --
`three`, and the panel scenario's `open-before-question` and
`question-arrives`, the screens before any C-j. From `ctrl-j` on the base never
leaves the first batch: `ask two` is typed into its open field and the Enter
after it answers the first question (`kitty-ctrl-j` shows question 2 of 3), and
the scenario's later lines land in the same batch (`end` shows the first
question's field holding `abask two3xy`), so every later pair cascades. `sent.txt`: the base sent no answer document.

## GREEN (`green.txt`, `sent.txt`)

- **The pairs** (`pairs.py xfx`: every ledger normalization applied, the rows of
  the three `/status` divergences set aside on each side, styles compared):
  **14 of 14 identical**. By `.prd/tui-phase4/receipts/m1/cells.py` raw (row 1
  left out, no ledger), 11 of 14: the three that differ are the panel's
  `open-before-question`, `question-ctrl-j` and `end`, on five `/status` rows
  each, every one covered by a ledger entry: `Session` (normalization
  `session-id`), `Build` (normalization `panel-build`), and `Sandbox`, `Step
  limit` and `Updates` (divergences `status-panel-sandbox`, `-step-limit`,
  `-updates`); `green.txt`'s third section lists every differing row with its
  entry. The activity row's pulse marker (`•` or a blank, whichever phase a
  snap catches) is the sixth kind such a row can differ on, covered by the
  normalization `activity-pulse`: round 1's capture of `open-before-question`
  differed there (fx `  Generating`, xfx `• Generating`); round 2's recapture
  caught both in the same phase, and the listing tags such a row when it
  differs.
- **The text sent** (`sent.txt`, its directories named as the receipt's
  `fx/`, `red/`, `xfx/`): fx and the head sent the same three answer documents
  in `p5-question-ctrl-j`, `Thorough`/`No`/`ab`, `Keep`/`Keep`, `xy`/`Three`,
  and the same one in the panel scenario, `Thorough` (round 2 added its
  `gwlog`); the base none in either.
- **The tests**: round 1's eight pass at `87daec96` in the gate's full run
  (`gate.txt`). At `998a434b` the unit's tests -- round 2 added the picker test
  and folded one held-rule test into another, so eight `#[test]`s still --
  pass with their neighbours (`green.txt`'s last section: `cargo test --locked
  --lib -- model_question model_picker held_rule`, 15 passed, 0 failed).

## Mutants (`mutants.py`, `mutants.txt`)

At `998a434b` (round 2; round 1's run at `87daec96` killed M1-M9 the same
way, 12 of 12), on a fresh `git clone --no-local` of the branch (remote
removed) under `$TMPDIR/mut/`, one at a time, the files restored after each,
under the suite lock; per mutant `cargo test --locked --lib -- model_question
question:: input::tests held_rule`. **16 of 16 killed**, the clone's tree clean
after the run:

| mutant | killed by |
|---|---|
| M1 C-j dropped at a choice | answer, one-read, matrix, panel, picker and held-rule tests |
| M2 C-j dropped in the field | answer, one-read and matrix tests |
| M3a kitty's `CSI u` Ctrl+J dropped | answer, one-read, matrix, panel and picker tests; the decoder's herdr-receipt table |
| M3b xterm's `CSI 27;m;k~` Ctrl+J dropped | answer, one-read, matrix, panel and picker tests |
| M3c a Ctrl+J with another modifier held not replayed (Ctrl alone only) | answer, one-read, matrix, panel and picker tests |
| M3d a kitty press or repeat (an event-type field) dropped | answer, one-read, matrix, panel and picker tests; the decoder's event-type test |
| M4 C-j moves the choice down, as a menu's C-j | answer, one-read, matrix, panel, picker, held-rule and `Act::of`'s tests |
| M5 C-j in the field is a newline, as in the composer | answer, one-read and matrix tests |
| M6 C-j goes to the `/status` panel under the question | panel test |
| M7 C-j swallowed at a model question again (the base) | answer, one-read, matrix, panel, picker, held-rule and `Act::of`'s tests |
| M8 a C-j the model's question takes not counted for the held rule (`b7861e54`'s code) | held-rule test |
| M9 a C-j the menu takes counted for the held rule too | held-rule scope test |
| M10 a kitty key release acts, as a press does | guard test; the decoder's event-type test |
| M11 `ESC LF` read as C-j | guard test; the decoder's legacy Alt-Enter test |
| M12 kitty Alt-j read as C-j (Alt taken for Ctrl on a letter) | guard test; the decoder's Ctrl-letter table |
| M13 C-j goes to the `/model` picker under the question | picker test |

M1-M3 are the brief's three, M3 split by spelling so kitty's, xterm's, the
extra modifiers and kitty's event types are each pinned on their own; M4-M6 are
where and how C-j answers (as an answer and not a move, as an answer and not a
newline, ahead of a menu); M7 is the fix taken out; M8-M9 are the held rule's
two sides; round 2's M10-M12 are the guard side, a key fx does not read as C-j
made C-j, and M13 a picker under the question. The names are the tests above:
answer, one-read, matrix, panel and picker (the shell C-j tests); guard,
`keys_fx_does_not_read_as_ctrl_j_do_nothing_at_a_model_question`; held-rule,
`an_enter_another_surface_took_counts_for_the_held_rule` (the loop over Enter
and C-j), and held-rule scope; and `Act::of`'s in `src/tui/question.rs`. Each
guard mutant is killed by the guard test as well as by a decoder test.

## Sweep (`sweep-base.md`, `sweep-head.md`, `sweep-compare.txt`)

`scripts/fxcap/sweep.sh --only 'question*' --only 'p5-question*' --only
'p5-input-question*'` at 40x120, `--jobs 4`, this branch's scenarios and
ledger: 11 scenarios, 81 snap pairs. Once with the base's binary
(`v0.1.0-afd891a`) and once with the code head's (`v0.1.0-87daec9`), each in
its own suite-lock slot; the scoreboards are the sweep's own, redacted by it;
compared pair by pair with APPROVAL-READINESS-SETTLED's `cmp_sweeps.py` (a
pair-listing appended).

- **base 68 / 81, head 77 / 81, drops 0.** Pairs with rows no cause covers: 11
  at the base, 0 at the head.
- The nine that moved to passing are this unit's `p5-question-ctrl-j` pairs
  from `ctrl-j` on; its `three` passes at both. The panel scenario's
  `question-ctrl-j` and `end` moved from unresolved to
  approved-divergence-cascade (the `/status` divergences' rows, as its
  `open-before-question` is at both); `question-arrives` passes at both.
- The two scoreboards name different ledger hashes (line 6 of each): the base
  sweep read the ledger as `244d3248` has it (sha256 `e92c917f…`), the head
  sweep as `87daec96` has it (`2067edc0…`). They differ only in the nine
  `src/tui/shell.rs` ref numbers `87daec96` re-pointed (`git diff 244d3248
  87daec96 -- scripts/fxcap/parity-ledger.toml`: 9 lines out, 9 in, the
  same lines once those numbers are masked), so no normalization, divergence or
  cause pattern differs and the classification is the same on both sides.
- No other pair changed class, cause or count. The four pairs that do not pass
  at the head are the panel's three cascades and `p5-question-resume-ctrlo`
  `resumed-ctrlo`, unresolved at both under its own cause,
  `question-resume-ctrlo-detail` (QUESTION-RESUME's; each scoreboard's
  `p5-question-resume-ctrlo` row and its cause table).

## Gate (`gate.txt`)

The brief's gate, once, under the suite lock with a short `TMPDIR`, at the code
head `87daec96` (`src/`, `tests/` and `docs/` as committed; this receipt was in
the working tree, untracked). Every step exits 0, and `src/` and `tests/` were
unchanged at the end:

| step | result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy --locked --all-targets -- -D warnings` | clean |
| `cargo clippy --locked --all-targets --features fault-injection -- -D warnings` | clean |
| `cargo test --locked --no-fail-fast` | 5693 passed, 0 failed, 7 ignored (15 result lines) |
| `cargo test --locked --features fault-injection --no-fail-fast` | 5730 passed, 0 failed, 7 ignored (15 result lines) |
| `scripts/check-xfx-identity.sh`, `check-no-stubs.sh`, `check-no-secrets.sh` | exit 0 each |

5693 and 5730 are train 13's 5685 and 5722 (`../train-13/gate-counts.txt`, at
`e1e15efd`; `git diff e1e15efd afd891af -- src tests` adds and removes no
`#[test]`) plus this unit's eight (`diffstat.txt`). No test here builds a
`Runtime`: the shell tests use the shell fixture and `McpCase`, the others the
panel alone. The same gate ran first at `b7861e54`, before the rename and the
held-rule commits: 5691 and 5728, every step exit 0 (`gate.txt`'s last lines).
After this receipt was committed, `check-no-secrets.sh` (it reads the index),
`check-xfx-identity.sh`, `check-no-stubs.sh` and `check-no-local-paths.sh` ran
again with it tracked (`gate.txt`'s last lines). Every commit's added lines were
scanned for local paths before it was made (`git diff --cached`).

## Parity (`docs/parity.md`) and the ledger

- `ask_user_question`'s row gains a sentence after the batch-of-20 clause:
  **C-j answers a model question as Enter does**, with what is copied (the byte,
  the field, the spellings and what is not C-j, the batch, a read with other
  keys, a menu under the question), the newline habit and its one-arm revert,
  and that xfx's own `/mcp` held-Enter rule counts such a C-j; since round 2,
  also the meta-prefixed spellings not copied, what they do on xfx, and their
  owner META-PREFIX-DECODE (queued). `/mcp`'s row says the same where it lists
  what the rule counts. No ledger entry: no capture differs at the head.
  `fx_src.sh` checks its citations with the unit's added lines and this file
  (`fx-src-check.txt`: 135 checked, 0 outside `fx-src.txt`'s ranges).
- The ledger's refs into `src/tui/shell.rs`, by content, with
  APPROVAL-VERTICAL-SPELLINGS' `refs.py` (`python3 -I refs.py afd891af`, from
  the repo root): 16 checked, 0 wrong (`refs.txt`, at `998a434b`). `--fix`
  re-pointed nine at `b7861e54`, the six `tools` refs again at `6d8dc766`, nine
  at `87daec96`, and fourteen (on thirteen ledger lines) at `998a434b`
  (round 2's doc lines at `McpEnters` moved every ref after them); numbers only (`refs.txt`'s first
  line).

## Safety

No new string reaches the terminal; nothing writes a file, runs a process or
reaches the network. The change adds an answering key to a surface that already
answered on Enter, with exactly Enter's effect, and makes `/mcp`'s held-Enter
rule hold back one more Enter (the one within 500 ms of a C-j that answered the
model's question), never fewer.

**The risk it brings is fx's own: the newline habit.** C-j is the composer's
newline (`Action::ControlJ`, `src/tui/editor.rs`), and once a question is up it
takes every key (`Shell::consume_inputs` hands each to `Shell::answer` while
`self.ask` is set), with no readiness check at a question on either side. So:
- a user writing a multi-line prompt while a turn runs who presses C-j for a new
  line just after a question appears answers it -- choice 1, the marked one --
  where the base swallowed that C-j;
- a C-j typed for a new line in the freeform field sends the answer as it
  stands (fx's field takes no byte under `0x20` as text,
  `question_prompt.zig:388`);
- so does an LF a terminal sends for a modified Enter.

fx does the same: its raw byte reaches `questionActionFromByte` whatever the
user meant by it (`src/ui/input/runtime.zig:141`, `:199-209`). **The revert**
is one arm: drop `Action::ControlJ` from `Action::Submit | Action::ControlJ`
in `question::Act::of` (`src/tui/question.rs:260` at the head), and C-j is
swallowed at a question again, as mutant M7 shows; that departs from fx, so it
is the user's to make. `docs/parity.md`'s sentence says the same.

## Round 2 (the reviews of `181dd86b`)

gpt6: APPROVE. opus: APPROVE-WITH-FIXES, no MUST on this branch; its three
MUSTs are for the train-14 join with APPROVAL-CTRL-J (below) and are not done
here. Tests, docs and receipt only (`diffstat.txt`'s last lines: no product
line between `87daec96` and `998a434b`), so the captures, the sweeps and the
gate stand at `87daec96`.

- **A picker under the question** (gpt6 SHOULD 1):
  `ctrl_j_at_a_model_question_over_the_model_picker_answers_the_question_not_the_picker`
  opens `/model`'s picker with three rows, checks its C-j moves the mark, asks a
  question over it, and for each of the 13 spellings: C-j answers the question
  (`Thorough`), nothing else reaches the control channel or the runtime, the
  picker's mark has not moved, and once the question is gone C-j moves the
  mark again. fx: the picker's C-j is its next row (`pickerControlDelta`,
  `app_input_runtime.zig:1102-1108`) and yields to a question
  (`composerPickerSurfaceVisible`, `:1110-1116`). Mutant M13 (C-j given to the
  picker first) is killed by it alone.
- **`McpEnters`' doc** (opus SHOULD 1): the struct's and `enter`'s docs say a
  C-j the model's question takes counts, and the menu's does not.
- **The matrix's claim** (opus NIT 1): `ctrl_j_matrix`'s doc says every
  spelling *without a meta prefix*, and names the meta-prefixed ones. fx reads
  `ESC ESC [106;5u` as the byte too (`escape_parser.zig:552-555`, `:686-691`,
  `:255-258`); xfx's decoder, with no meta prefix, reads Escape and then C-j.
  `probe-meta-prefix.txt` (a probe run at `998a434b`, not committed as a test):
  at a choice the Escape cancels the batch and its turn and the C-j is then a
  newline in the composer, where fx takes choice 1; in the field with text the
  Escape arms the clear and the C-j takes the text and moves on, as fx's does.
  `docs/parity.md`'s `ask_user_question` row says so, owner
  **META-PREFIX-DECODE** (queued); the row's two other mentions of that unit say
  "queued" for "proposed".
- **The held-rule tests in one loop** (opus NIT 2), the older test's name
  kept: another receipt's script (`../mcp-menu/mutants_md.py`) runs it by name.
- **The receipt** (opus NITs 4-6): the pulse row and `activity-pulse` (GREEN);
  `sent.txt`'s directories named as the receipt's, and the panel scenario's
  answer in it, its scenario given a `gwlog` and recaptured on all three sides
  with the same binaries (`binaries.txt`'s third guard line; pairs 14 of 14 and
  3 of 14 again); the four check scripts at the final commit (`gate.txt`'s last
  lines).
- **Guard-side mutants** (opus NIT 7): M10-M12 make a key fx does not read as
  C-j into C-j (a kitty release acting, `ESC LF` as C-j, kitty Alt-j as C-j),
  each killed by `keys_fx_does_not_read_as_ctrl_j_…` and a decoder test; M13
  hands C-j to the `/model` picker under the question, killed by the picker
  test alone (Mutants, above: 16 of 16).
- Not changed: opus NIT 3, `MCP_ENTER_TOO_SOON` calling a C-j "the last one"
  -- xfx's own string, the user's wording to choose.
- Run for round 2: `cargo fmt --check`, both clippy runs, and the touched tests
  (`green.txt`'s last section); the full gate stands at `87daec96`, whose
  product code `998a434b` keeps.

## Merging beside APPROVAL-CTRL-J

Both units edit `Action::ControlJ`'s doc (`src/tui/input.rs`) at the same two
lines: that one adds "Enter at a permission question", this one "Enter at a
model question", after the same re-pointed menu citation, and the second
paragraph is word for word the same. The merged doc names both. That unit's
`docs/parity.md` full-screen TUI row says "QUESTION-CTRL-J, proposed"; once
both are in, that clause is stale and should point at this row. And the held
rule: this unit counts a C-j the model's question takes, that one does not
count a C-j an approval takes (it changed no code there; its README's
Decisions). The `answers` match is where an approval's C-j would be added
(`Input::Action(Action::ControlJ) => self.ask.is_some() || self.asking()`), if
the merge makes the two one, as Decisions above argues.

## Files

| file | what |
|---|---|
| `fx/`, `red/`, `xfx/` | the captures: fx v0.0.16, xfx at the base, xfx at `87daec96` |
| `binaries.txt` | the three binaries' sha256, banners, and `capture.sh`'s guard lines |
| `fx_src.sh`, `fx-src.txt`, `fx-src-check.txt` | every fx line cited, at `481104fd`, and the citation check over this file, the added lines and the parity sentence |
| `pairs.py`, `red.txt`, `green.txt` | the pair comparison and its results at the base and the head; `green.txt` also has the tests at the head |
| `red-tests.txt` | the RED tests at `4f3c1515`, each failure cut to 14 lines, and the one-read test's paste case in full |
| `red-tests-held.txt` | the held rule's RED at `ae121e4b` (the files' bytes and paths cut) |
| `sent.py`, `sent.txt` | the answer documents each run sent |
| `diffstat.txt` | the diff's size, inside and outside `mod tests`, and every code line outside it |
| `refs.txt` | the ledger refs, checked by content |
| `probe-meta-prefix.txt` | round 2's probe of fx's meta-prefixed Ctrl+J on xfx, its source and output |
| `mutants.py`, `mutants.txt` | the mutants and their results |
| `cmp_sweeps.py`, `sweep-base.md`, `sweep-head.md`, `sweep-compare.txt` | the sweeps' scoreboards and every pair's verdict at both |
| `gate.txt` | the gate |
