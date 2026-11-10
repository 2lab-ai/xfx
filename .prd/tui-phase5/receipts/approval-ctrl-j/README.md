# APPROVAL-CTRL-J receipt: C-j answers a permission question as Enter does

fx v0.0.16 answers a permission question on C-j (byte `0x0a`) exactly as on
Enter, at a choice and in a draft; xfx's question swallowed it in every
spelling. APPROVAL-DRAFT-KEYS found the gap (its README, "Not taken here";
its opus review, point 4). This unit copies fx: C-j is an affirmative where
Enter is, through APPROVAL-READINESS-SETTLED's readiness check exactly as
Enter, and on choice 3 a refusal, answered at once. The base is
APPROVAL-DRAFT-KEYS' head `d7916178` (branch `fix/p5-approval-draft-keys`,
not yet on `main`).

| commit | what |
|---|---|
| `66ea82f3` | RED: five shell tests, the scenarios `p5-approval-ctrl-j` and `p5-approval-ctrl-j-panel`; tests only in `src/` |
| `2cf89566` | **the fix**: `Shell::decide` reads `Action::ControlJ` as `approval::Action::Submit`; `ControlJ`'s and `approval::edits`' docs; `docs/parity.md`'s approval sentence; the ledger's moved refs |
| `9dd02bc3` | **the code head**: two fx citations the unit's lines carry pointed at the pin (comments only: the RED test's `:103-108` read as `shortcuts.zig`'s, and `ControlJ`'s doc cited c1db919's `app_input_runtime.zig:1220-1235` with no pin); the ledger's moved refs |
| the commit after | this receipt; no `src/` change |

Captures, both scenarios at 40x120: `fx/` is fx v0.0.16 (sha256 `8e6cda2e…`,
`FX_AUTO_UPGRADE=0`; `capture.sh` checked its bytes after each run), `red/`
the base's binary (row 1 `x𝒇x v0.1.0-d791617 [debug]`, built at `d7916178`
before any commit), `xfx/` the code head's (row 1 `x𝒇x v0.1.0-9dd02bc
[debug]`, built after the commit). Hashes, banners and the guard lines:
`binaries.txt`. The scratch root in the `/status` rows' workspace path is
written `<tmp>` on all three sides. The request bodies stay out (their system
prompt names the run's directory); `sent.txt` is what each run sent as
feedback, read from them by `sent.py` (APPROVAL-DRAFT-KEYS').

## fx's rule (`fx-src.txt`, written by `fx_src.sh`: every line read with `git show 481104fd:<path>`)

**The byte.** A byte read with no sequence open is a raw byte (`appendRaw`,
`terminal_action_decoder.zig:207-212`); `0x0a` carries the composer's
`.insert_newline` (`shortcuts.zig:26`). At an approval, `withApprovalInput`
asks the focused-editor route first, and `fromFocusedEditorControlByte` keeps
no newline (`shortcuts.zig:31-41`), so the byte falls to
`approvalActionFromByte`, where `'\r', '\n' => .submit`
(`src/ui/input/runtime.zig:61-73`, `:64`; `:98-108`, `:115-120`). `.submit`
is the marked choice's decision whether or not a draft is open
(`approval_decision.zig:141`), and the question takes it
(`app_input_runtime.zig:1161-1184`). So:

| where | fx v0.0.16 | xfx base | xfx head |
|---|---|---|---|
| at a choice | answers the marked choice, as Enter (`src/ui/input/runtime.zig:64`; `approval_decision.zig:141`) | nothing | **answers, as Enter** |
| in an open draft, on the band and on the review plane | answers the marked choice and sends the draft: the same `.submit` (the review plane is a surface, not another key route, `app_input_runtime.zig:1161-1185`) | nothing | **answers and sends the draft** |
| with a move or a draft edit in the same read | refused while the modal frame they asked for is pending, spent silently (`input_approval_runtime.zig:191-218`) | nothing | **refused, as Enter** |
| at Deny (choice 3) | a refusal, never gated (`decision != .deny`, `input_approval_runtime.zig:191`), the deny draft sent | nothing | **a refusal at once, the draft sent** |
| a menu open under the question | the question takes the byte: its branch runs before the MCP menu and the compact menus (`/status`'s panel among them, whose C-j is the next row; `app_input_runtime.zig:1186-1216`, `:1205`); the picker's C-j yields to it (`composerPickerSurfaceVisible`, `:1110-1116`), and so does the full transcript (`screenOwnsInput`, `input_full_transcript_runtime.zig:30-34`) | nothing to either | **answers the question; the panel does not move** |

**The spellings.** A kitty `CSI <k>;<m>u` or an xterm `CSI 27;<m>;<k>~`
(read into a third field and handed to the same function,
`escape_parser.zig:668-674`, `:740-752`) of `j` or `J` with Ctrl held is the
byte `0x0a` (`kittyUnicodeKeyAction`, `escape_parser.zig:255-258`), whatever
else is held -- Shift (only `a b e f` are a selection move,
`escape_parser.zig:235-242`), Alt (only `b f d`, `:243-251`), Super (only
`a c x z r`, `:223-234`, `:253`) -- and with Caps or Num Lock, masked first
(`:187`); a kitty report's press and repeat act, its release is `.ignore`
(`:774-777`). The replayed byte takes the raw route above
(`app_input_runtime.zig:950-953`, `:746-756`). Not C-j to fx: a kitty
release; `ESC LF` (Alt-C-j without a key protocol), which is `.insert_newline`
(`escape_parser.zig:515-519`), no draft action (`src/ui/input/runtime.zig:94`)
and nothing at the question (`input_approval_runtime.zig:128`); and Alt-j,
which is `.ignore` (`escape_parser.zig:265`).

**The brief's lead about a menu does not hold at the pin.** It said C-j with
a menu open "moves through it, or nothing". fx's approval takes the byte
before every menu (the table's last row), so C-j answers the question and the
menu stays where it was. xfx's question has the keys the same way: every menu
is offered a key only after `Shell::consume_inputs` has handed it to the
question. The panel test and the panel scenario pin fx's answer.

**The captures agree** (`fx/`): Down and C-j in one write leave the edit
question up with choice 2 marked (`edit-down-ctrl-j`), Up and C-j with choice
1 (`edit-up-ctrl-j`), and the next C-j answers `Apply once` (`write-ask`
follows); at the write, `x` and C-j leave `Apply once, x` up
(`write-letter-ctrl-j`), and the next C-j sends it (`cmd-ask` follows;
`sent.txt`, fx `["x"]`); Down Down C-j at the first command is a refusal at
once (`cmd2-ask`: `Denied echo hi > out.txt && cat out.txt`); a kitty Ctrl+J
at the second command answers `Yes` and the turn ends (`end`). With `/status`
open when the question arrives, C-j answers it and the panel is back
(`p5-approval-ctrl-j-panel`'s `approval-ctrl-j`).

## The change (`diffstat.txt`: 3 files, +320 / -5; outside the `mod tests` blocks +23 / -5, one line of code and the rest comments)

- **`Shell::decide`** (`src/tui/shell.rs`): `Input::Action(Action::Submit |
  Action::ControlJ) => approval::Action::Submit`, the arm Enter had, ahead of
  the draft's arm, which would have swallowed it. Both planes answer with the
  same `approval::answered`, so C-j means one thing on each; the answer then
  meets Enter's gate (`Once` and `Always` held back while a change is
  unshown, `Deny` never).
- The decoder is unchanged: every spelling above was already `ControlJ`
  (`control`'s `0x0a`; `kitty_key`'s Ctrl-letter rule, reached from
  `report`'s and `tilde_report`'s fields and event types). The tests drive
  each through it.
- Docs: `Action::ControlJ` (`src/tui/input.rs`) says it is Enter at a
  question; `approval::edits` names it among the panel's own keys.

## Decisions (each reversible)

- **The `/mcp` menu's held-Enter rule counts `Submit` only, not a C-j that
  answers a question** (`McpEnters`, `Shell::consume_inputs`). The rule is
  about a held Enter repeating into an MCP confirmation; a held C-j repeats
  as C-j, which fx's MCP menu does not take (`routeMcpHostMenuByte`,
  `app_input_runtime.zig:1820`) and xfx's reads as Down, so it cannot confirm
  one. fx has no such rule.
- **Not taken here, recorded in `docs/parity.md` with a proposed owner**:
  - fx's model question takes C-j as Enter too (`questionActionFromByte`,
    `src/ui/input/runtime.zig:141`); xfx's does nothing with it
    (`question::Act::of` has no `ControlJ`): QUESTION-CTRL-J.
  - fx's `/permissions` rule confirmation is an approval prompt (it installs
    a request there, `approval_prompt.zig:137-145`) answered through the same
    `submitPermissionChoice` (`input_approval_runtime.zig:185-188`), so C-j
    confirms it; xfx's confirmation (`Shell::offer_to_rule_confirm`) does
    nothing with it: RULE-CONFIRM-KEYS.
- fx's sign-in and API-key entries take `'\r', '\n'` too, ahead of the
  question (`routeAuthPickerByte`, `app_auth_runtime.zig:688-728`); xfx has
  no such entry, so there is nothing to copy.

## RED (`red.txt`, `pairs.txt`)

At `66ea82f3` (`git diff d7916178 66ea82f3 -- src` adds tests only), four
tests fail and one passes:
- `ctrl_j_answers_as_enter_does_at_a_choice_and_in_a_draft_on_both_planes`:
  all eleven spellings, at choice 1, at choice 2 and in a draft, on the band
  and on the review plane, answered nothing (66 cases).
- `ctrl_j_read_with_a_change_waits_for_the_frame_as_enter_does`: the C-j
  after the frame answered nothing -- after a move on both planes and at a
  command, and in a draft on both planes (55 cases). Its in-one-read
  refusals pass at the base trivially.
- `ctrl_j_at_deny_refuses_at_once_as_enter_does`: Down Down C-j at an edit and
  at a command, and `x` and C-j in the deny draft on both planes, refused
  nothing (44).
- `ctrl_j_at_a_question_over_the_status_panel_answers_the_question_not_the_panel`:
  C-j scrolled the panel before the question (it is the panel's key) and
  answered nothing at the question, on both planes (22).
- `keys_fx_does_not_read_as_ctrl_j_still_do_nothing_at_a_question` passes, by
  construction: its three keys are nothing to fx and to the base alike.

The pairs at the base (`pairs.txt`, `scope.py red`): **5 of 12** identical.
`edit-ask`, `edit-down-ctrl-j` and `edit-up-ctrl-j` match because the base's
C-j does nothing and fx's is refused there; from `write-ask` on the base is
still at the edit, and the panel scenario's question is still up at
`approval-ctrl-j` and `end`. `sent.txt`: the base sent no feedback.

## GREEN (`green.txt`, `pairs.txt`, `sent.txt`)

- At the code head `9dd02bc3`, the unit's tests and their neighbours
  (APPROVAL-DRAFT-KEYS' round-2 filter with `ctrl_j_` and
  `keys_fx_does_not_read_as_ctrl_j` added): **488 passed, 0 failed, 1
  ignored** (a release-only timing receipt) -- that filter's 481 plus this
  unit's five and two older menu tests the `ctrl_j_` word also matches
  (`the_keys_it_binds_are_the_arrows_ctrl_j_and_k_escape_and_enter`,
  `the_page_keys_and_ctrl_j_k_move_the_resume_pickers_mark_and_stop_at_the_ends`,
  both passing: C-j still walks a menu with no question up).
  `cargo test --locked --test parity`: 26 passed.
- **The pairs** (`scope.py xfx`: every ledger normalization applied, the rows
  of `approval-grant-scope`, `tool-schema` and the three `/status`
  divergences set aside on each side, styles compared): **12 of 12
  identical** -- `edit-ask`, `edit-down-ctrl-j` and `edit-up-ctrl-j` (the
  move drawn, the C-j refused), `write-ask` (the next C-j answered),
  `write-letter-ctrl-j` (`Apply once, x` up), `cmd-ask` (`x` sent), `cmd2-ask`
  (`Denied …`), `end` (a kitty Ctrl+J answered); and the panel scenario's
  `open-before-approval`, `approval-arrives`, `approval-ctrl-j` and `end`.
- **The text sent** (`sent.txt`): fx `["x"]` after the write, the head
  `["x"]`, the base nothing.

## Mutants (`mutants.py`, `mutants.txt`)

At the code head `9dd02bc3`, on a fresh `git clone --no-local` throwaway of
the branch (remote removed) under `$TMPDIR/mut/`, one at a time, the files
restored after each, under the suite lock; the clone's tree was clean after
the run. **10 of 10 killed**:

| mutant | killed by |
|---|---|
| M1 C-j not gated by readiness | the change test alone |
| M2 C-j at Deny gated | the Deny test alone |
| M3a kitty's `CSI u` Ctrl+J dropped | the four C-j tests |
| M3b xterm's `CSI 27;m;k~` Ctrl+J dropped | the four C-j tests |
| M3c a Ctrl+J with another modifier held not replayed (Ctrl alone only) | the four C-j tests |
| M4 C-j answers at a choice only, not in a draft | the answer, change and Deny tests |
| M5 C-j moves the choice down, as a menu's C-j | the four C-j tests |
| M6 C-j swallowed on the review plane | the four C-j tests |
| M7 C-j goes to the `/status` panel under the question | the panel test alone |
| M8 C-j swallowed at a question again (the base) | the four C-j tests |

M1-M3 are the brief's three, M3 split by spelling so each of kitty's, xterm's
and the extra modifiers is pinned on its own; M4-M7 are where C-j answers
(a draft, as an answer and not a move, the review plane, ahead of a menu);
M8 is the fix taken out.

## Sweep (`sweep-base.md`, `sweep-head.md`, `sweep-compare.txt`)

`scripts/fxcap/sweep.sh --only 'approvals*' --only 'p5-approval*' --only
'p5-input-approval*'` at 40x120, `--jobs 4`, this branch's scenarios and
ledger, once with the base's binary (`v0.1.0-d791617`) and once with the code
head's (`v0.1.0-9dd02bc`), both in one suite-lock slot; compared pair by pair
with APPROVAL-READINESS-SETTLED's `cmp_sweeps.py`, unchanged.

- **base 3 / 110, head 4 / 110, drops 0.** Pairs with rows no cause covers:
  10 at the base, 3 at the head.
- The unit's two scenarios: at the head every one of their 12 pairs has 0
  rows no cause covers -- `end` normalized-identical, the panel's
  `approval-ctrl-j` and `end` approved divergences (the `/status` rows), the
  rest `approval-grant-scope` and, at a command, `tool-schema`. At the base
  `write-ask` and `write-letter-ctrl-j` had 15 such rows each, `cmd-ask` 26,
  `cmd2-ask` 27, `end` 26 and the panel's `approval-ctrl-j` and `end` 24 each
  (a question behind).
- No other pair changed class, cause or count. The 3 pairs with uncaused rows
  at the head are the same 3 as at the base, with the same counts
  (`p5-input-approval` `cmd-ask` 15, `edit-ask` 17, `after` 13), as at
  APPROVAL-DRAFT-KEYS; this unit touches no row of that scenario.

## Gate (`gate.txt`)

The brief's gate, once, under the suite lock with a short `TMPDIR`, at the
code head `9dd02bc3` (`src/` and `tests/` as committed; this receipt was in
the working tree, untracked). Every step exits 0, and `src/` and `tests/`
were unchanged at the end:

| step | result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy --locked --all-targets -- -D warnings` | clean |
| `cargo clippy --locked --all-targets --features fault-injection -- -D warnings` | clean |
| `cargo test --locked --no-fail-fast` | 5576 passed, 0 failed, 7 ignored (15 result lines) |
| `cargo test --locked --features fault-injection --no-fail-fast` | 5613 passed, 0 failed, 7 ignored (15 result lines) |
| `scripts/check-xfx-identity.sh`, `check-no-stubs.sh`, `check-no-secrets.sh` | exit 0 each |

5576 is APPROVAL-DRAFT-KEYS' round-1 gate count, 5569, plus its round 2's
two tests and this unit's five (`diffstat.txt`: `git diff 09077f5f d7916178` and
`git diff d7916178 9dd02bc3` over `src tests` add two and five `#[test]`s
and remove none); 5613 is its 5606 plus the same seven. No test here builds a
`Runtime`. `check-no-secrets.sh` ran again with this receipt staged (it reads
`git ls-files`, the index): exit 0, 18168 tracked files; and
`check-xfx-identity.sh` and `check-no-stubs.sh` once it was committed: exit
0 each (`gate.txt`'s last two lines). Every commit's added lines were scanned for local paths before it
was made (`git diff --cached`).

## Parity (`docs/parity.md`) and the ledger

- The full-screen TUI row's approval sentence: "five differences" is now
  four, without C-j; the clause that left C-j to a unit of its own is
  replaced by what is copied (the byte, the spellings, the readiness check,
  Deny, the menu under the question), **the newline habit, as in fx** -- a
  user who types C-j for a new line in a draft, or whose terminal sends LF for
  a modified Enter, answers the marked choice (`Apply once` at choice 1) with
  the draft as it stands once a frame has shown it; the readiness check holds
  it back only within one read -- and the two proposed units above.
  `fx_src.sh` checks those citations with the unit's added lines and this
  file (run as its header says; `fx-src-check.txt`): 90 checked, 0 outside
  `fx-src.txt`'s ranges.
- The ledger's refs into `src/tui/shell.rs`, by content, with
  APPROVAL-VERTICAL-SPELLINGS' `refs.py` (`python3 -I refs.py d7916178`, from
  the repo root): 17 checked, 0 wrong (`refs.txt`). `--fix` re-pointed twelve
  at `2cf89566` (the fix's 14 lines moved six, and the tests' lines the six
  `tools: {} advertised` ones, 40642 to 40952) and those six again at
  `9dd02bc3` (to 40953). No new ledger entry: the scenarios leave out the one
  move the every-approval decision would show (Down and C-j at the second
  command), and every other difference is covered by an entry for every
  scenario.

## Safety

No new string reaches the terminal; nothing writes a file, runs a process or
reaches the network. The change adds an affirmative key, held to exactly
Enter's gate: no C-j grants what a frame has not shown (the readiness check,
at every approval), and a refusal stays answerable at every moment. The risk
it brings is fx's own and is written down: the newline habit above.
