# RULE-CONFIRM-KEYS receipt: a `/permissions` rule confirmation takes fx's approval keys

fx v0.0.16 asks `/permissions remember` and `revoke` in its approval prompt,
confirmation-only, so the keys there are the approval's over two choices.
xfx's confirmation is a band of its own (`src/tui/rule_confirm.rs`) with its
own key map, which took Enter, `1`, `2`, Up, Down, C-p, C-n, Escape and
Ctrl-L and swallowed the rest. APPROVAL-CTRL-J found the gap (its README,
"Not taken here"). This unit gives the band the approval's keys by reading
them through the approval's own translation, as fx does. The base is
APPROVAL-CTRL-J's head `1a0adaec` (branch `fix/p5-approval-ctrl-j`, not yet
on `main`).

| commit | what |
|---|---|
| `ec44d180` | RED: five shell tests and the scenario `p5-rule-confirm-keys`; tests only in `src/` |
| `c5f989db` | **the fix**: `Shell::decide`'s key translation becomes `Shell::approval_action`, which the rule confirmation reads with the spelled event and no draft; `RuleConfirm::apply` takes an `approval::Action`; `rule_confirm::Key` and the band's own map are deleted; the two parity sentences; the ledger's moved refs |
| `4b3dc4af` | the idle Escape test asserts no stop; an Escape-disarm line `c5f989db` added is taken back out (no key reaches it: typing the command disarms both gestures first); the scenario `p5-rule-confirm-keys-turn`; moved refs |
| `91706a67` | RED 2, from that scenario's fx captures: the in-turn Escape test asserts no activity row under the confirmation and no cancellation row; tests only |
| `071b26e0` | round 1's code head: `Shell::modal` counts a rule confirmation; its Escape over a running turn stops it with no cancellation row; the parity sentence; moved refs |
| `bfa9ceec`, `18f5dd91` | this receipt; `docs/parity.md`'s reason for fx's Escape, the card's row as still apart, a citation named with its file; the ledger cause `rule-confirm-card-after-stop`; the quick checks rerun with it tracked; no `src/` change |
| `d1ebdd45`, `aa40d1f1` | round 1's own review: the scenario `p5-rule-confirm-keys-cmd` (Escape over a running command), both sweeps rerun over the same 30 scenarios and one ledger, the parity sentence's running-command clause; no `src/` change |
| `e13010e6` | **round 2, the code head**: `rule-confirm-card-after-stop` narrowed to its rows (patterns, `kind`, refs, spacing); `Shell::stop_owed`, the stop guard named and shared, `written = true` commented; four tests (Escape over a manual compaction, no second stop, work typed since a stop stopped, the clock held under a confirmation); the readiness decision's scope names the rule confirmation; the Escape wording; RULE-CONFIRM-SCREEN's citation; moved refs |
| `7e2e177d` | `permissions-btab.cap`: an `until shown <mode> · model-a` before each Shift-Tab snap |
| the commit after | this receipt, round 2 |

Captures, all three scenarios at 40x120: `fx/` is fx v0.0.16 (sha256 `8e6cda2e…`,
`FX_AUTO_UPGRADE=0`; `capture.sh` checked its bytes after each run), `red/`
the base's binary (row 1 `x𝒇x v0.1.0-1a0adae [debug]`, built at `1a0adaec`),
`xfx/` the code head's (row 1 `x𝒇x v0.1.0-e13010e [debug]`, built after
the commit; round 1's were `071b26e`'s). Hashes, banners and guard lines: `binaries.txt`.

## fx's rule (`fx-src.txt`, written by `fx_src.sh`: every line read with `git show 481104fd:<path>`)

**One prompt, confirmation-only.** `handlePermissionRuleManagement` installs
the change as an approval request with `.amendment_allowed = false` and
`.confirmation_only = true` (`app_commands.zig:678-693`, `:732-741`), and
`syncRuleManagement` sets `setConfirmationOnly(true)`
(`approval_prompt.zig:119-146`). That changes only the decision: two choices
(`optionCount`, `approval_decision.zig:227-229`), choice 0 `once` and
anything else `deny` (`selectedDecision`, `:75-80`), a mark on choice 3 reset
to 0 (`:82-85`). **No draft**: Tab opens none where amendment is not allowed,
so it moves the mark (`.tab`, `:142-151`), and a letter is nothing
(`insertAscii`, `:196-202`). **No review screen** at the band's sizes: the
request has no file (`needsScreen`, `approval_screen.zig:208-223`; a display
too long for the band would take it, below). **No readiness gate**: the
submit goes to `submitRuleManagementChoice` before the file approval's check
(`input_approval_runtime.zig:185-188`, `:191-192`). The screen is the
approval band: `Permission needed · Choose one` with `Permission rule` on
the right (`approvalKind`, `approval_ui.zig:1841-1843`), `1. Confirm` /
`2. Cancel` (`:1688-1693`), hint `1–2 choose    enter confirm    esc cancel`
(`:1735-1742`).

**The keys are the approval's** (`routeActiveModalInput`,
`app_input_runtime.zig:1161-1185`; `routeResolvedEscapeAction`, `:964-971`):

| key | fx v0.0.16 | xfx base | xfx head |
|---|---|---|---|
| Enter, `1`, `2` | the marked choice / Confirm / Cancel | same | same |
| C-j, and every Ctrl+J spelling | Enter (`src/ui/input/runtime.zig:64`; `.submit`, `approval_decision.zig:141`) | nothing | **Enter** |
| Ctrl-C | `.deny` (`src/ui/input/runtime.zig:63`), so Cancel: `saved-session permission change cancelled` (`input_approval_runtime.zig:267-275`) | nothing | **Cancel** |
| Up, Down | move, wrapping over two (`moveChoice`, `approval_decision.zig:214-229`) | move | move |
| Tab | moves (above) | nothing | **moves** |
| Left, Alt-b / Right, Alt-f | previous / next (`input_approval_runtime.zig:90-110`) | nothing | **move** |
| C-p, C-n | nothing (`Shell::decide`'s reasons, now `approval_action`'s) | **moved the mark** | nothing |
| Shift-, Ctrl-, Alt-, Super-Up and -Down | nothing (`:128`) | **moved the mark** (the folded event) | nothing |
| `3`, a letter, Ctrl-L | nothing (`approval_decision.zig:157-158`, `:202`; `src/ui/input/runtime.zig:98-108`, `:61-73`) | nothing; **Ctrl-L repainted** | nothing |
| Escape, idle | closes it, saying nothing (below) | same | same |
| Escape over a running turn | closes it and stops the turn, writing no cancellation row of its own (below) | closed it; **the turn went on** | **stops it, no row** |

**Escape.** fx's Escape at an approval with an operation in hand is
`cancelApprovalOperation` (`resolveEscape`, `app_input_runtime.zig:2689`,
`:2712-2716`): the turn is stopped and the prompt cleared with no notice
(`input_approval_runtime.zig:401-436`), and no turn cancellation is written,
where Ctrl-C's `cancelActiveOperation` writes one
(`input_interrupt_runtime.zig:118-126`). With none in hand the code reads as
leaving the prompt up (`app_input_runtime.zig:2761-2764`). **But fx takes the
first branch at every Escape after the prompt's first frame, idle ones
included**: `probe/trace.txt`
(fx's own `FX_TRACE_LOG` of `probe/probe.cap`, eight scopes) shows `[input] cancel
approval operation queued=0` -- that function's own line
(`input_approval_runtime.zig:421-424`) -- at an idle Escape after a
`/permissions remember` and again after a finished turn; that function
clears the prompt (`:428`), and `fx/fx-p5-rule-confirm-keys-40x120-esc`
shows an idle one closing it. **Why**: while an approval prompt is up fx's `syncState` marks its stream active (`modal_active and !app.stream.active` -> `app.stream.active = true`, `app_worker_runtime.zig:729`, `:753-757`), so `hasActiveOperation` holds at every rule confirmation and the decoder latches it with the Escape byte (`terminal_action_decoder.zig:42-45`, `:180-182`). `probe/esc-all-trace.txt` (every scope, `probe/esc-all.cap`) shows it: the Escape's `cancel_requested` line reads `stream_active=true` with no turn ever started. At an idle confirmation that route only clears the prompt: there is no turn to stop. `syncState` has no return before that line but one for a pending model
question (`app_worker_runtime.zig:691-727`, `:721`), so an Escape read
before the first sync after the prompt is published would latch `false`
and leave it up; no typist is that fast, and both traced Escapes took the
first branch. So xfx's band keeps closing on an idle
Escape, as it did, and over a running turn now stops the turn without a row
(`fx/fx-p5-rule-confirm-keys-turn-40x120-esc`: the turn's partial answer
and its activity row gone, no `▪ Cancelled`; `-end`: nothing after the
stream is let go). Over a running command the call's own report still
says so, on both: `└ Cancelled sleep 6` and `▪ Cancelled sleep 6 · What
can fx do differently?` (`fx/fx-p5-rule-confirm-keys-cmd-40x120-esc`):
the stop kills the command and its result is the call's, not the
keystroke's. While the confirmation is up over a running turn fx's
band has no activity row (`-prompt`), as under any approval; xfx's had one.

**Copied by delegation.** fx has no rule-confirmation key map: the approval's
is the rule confirmation's. xfx now has one map too. `Shell::decide`'s
`event → approval::Action` match is `Shell::approval_action(event, drafting)`
(its arms unchanged but `return` → `return None`), and
`Shell::offer_to_rule_confirm` reads the spelled event -- ahead of `folded`,
which had turned Shift-Up into Up -- through it with `drafting = false`.
`RuleConfirm::apply` answers an `approval::Action` over two choices, and
`rule_confirm::Key` is gone. A key a later unit gives the approval reaches
the rule confirmation with it.

**Not copied** (`docs/parity.md`'s `/permissions` row): a rule display too
long for the band takes fx's review screen, where xfx's band wraps it whole;
proposed owner RULE-CONFIRM-SCREEN. fx's frame asks `needsScreen` of every
approval prompt (`app_render_runtime.zig:1160-1167`), and a rule prompt's
`command` is its display (`.command = prepared.display_identity`,
`app_commands.zig:690`; `approvalCommand`, `approval_screen.zig:447-449`),
so a display that does not fit inline takes the screen (`:208-223`). Read
from the code; no capture of a long display is taken here.

## The change (`diffstat.txt`)

At the code head `e13010e6`: 2 files, +547 / -133; outside the `mod tests` blocks `src/tui/shell.rs` +146 / -100 and `src/tui/rule_confirm.rs` +26 / -24 -- most of `shell.rs`'s being `Shell::decide`'s head moved below `Shell::approval_action` (80 of round 1's added lines, non-blank, were a removed line again, by `added.py`'s reading). Eight `#[test]`s added (five in round 1, three in round 2, one of which pins two rules), none removed; `rule_confirm.rs`'s key test is rewritten in place.

- `Shell::approval_action`, moved out of `Shell::decide`, whose head moves
  below it (most of the removed and added lines outside the tests are that
  move); `Shell::offer_to_rule_confirm` reads it, and Escape's stop is
  there, behind `Shell::stop_owed` (round 2: `Shell::cancel_turn`'s
  guard, named and shared); `Shell::modal` counts `rule_confirm`
  (`src/tui/shell.rs`).
- `consume_inputs` hands the rule confirmation the spelled event, ahead of
  `folded`.
- `RuleConfirm::apply(approval::Action)` (`src/tui/rule_confirm.rs`).

## Decisions (each reversible)

- **Escape stops a running turn or compaction only**, read as xfx's composer
  Escape reads fx's operation in hand (`self.activity.working() ||
  self.compacting()`), not fx's `hasActiveOperation`, which counts a
  provider preparation too. The compaction half is fx's: its
  `cancelApprovalTurn` stops the worker's compaction as well as its turn
  (`stopCompactionActivityLocked`, `worker_runtime.zig:777-791`). A stop
  already requested is not asked again, and work typed since it is stopped
  (`Shell::stop_owed`, the composer's rule; fx asks again only what is not
  already cancelled, `input_approval_runtime.zig:421-434`). The stop is
  `cancel_turn_with(true)`: no `▪ Cancelled` row of its own and no group
  settled by the key, as fx's route presents no tool cancellation; a
  running command's report says it (`-cmd`).
- **An idle Escape closes the band**, copying the captures and the trace,
  not the branch the code reads as taken (above).
- **`Shell::modal` counts the rule confirmation**, not a narrower check at
  the activity row: each of its six readers is about an approval being up,
  and fx's rule confirmation is its approval prompt. `open_full_transcript`
  (refused; C-o never reaches it there, the confirmation swallows it); the
  clock freeze (now held under a confirmation, fx's `syncWaitingClock`,
  `app_worker_runtime.zig:760`; pinned by
  `the_turns_clock_stops_while_a_rule_confirmation_is_up_over_it`); the
  activity row (pinned by the in-turn Escape test); the repaint reason
  after a paste the band ignores (Modal for Footer, which no frame reads
  differently); the MCP status and the full-access warning slots (the
  confirmation's own hint has the row, and the warning already read the
  slot as taken).

## RED (`red.txt`, `red-pairs.txt`)

Two RED commits, each tests only in `src/` against the code of its parent
(`red.txt`, the unit's tests run at each):
- at `ec44d180` (the base's code) five tests fail:
  `ctrl_j_at_a_rule_confirmation_answers_as_enter_does_on_either_choice`
  (all eleven Ctrl+J spellings of APPROVAL-CTRL-J's table answered nothing
  on Cancel, and on Confirm the next line's Enter cancelled: 22 cases),
  `ctrl_c_at_a_rule_confirmation_cancels_it_and_arms_no_exit`,
  `tab_and_the_horizontal_keys_move_a_rule_confirmations_mark_wrapping`
  (no move for any of the eight keys), `keys_fx_reads_as_nothing_leave_a_rule_confirmation_as_it_was`
  (C-p, C-n and the eight modified Up and Down moved the mark; Ctrl-L
  marked the screen damaged; `3`, `j`, `k` did nothing, as in fx), and
  `escape_at_a_rule_confirmation_over_a_running_turn_stops_the_turn` (no
  stop sent). The existing idle Escape test passes there, as it should.
- at `91706a67` (the code of `4b3dc4af`) the in-turn Escape test fails on
  the activity row under the confirmation (`Some(26)`); its no-row half is
  pinned by mutant M11.
- Round 2's three tests pin behaviour already at round 1's head (Escape
  over a manual compaction, no second stop and work typed since a stop,
  the clock under a confirmation); they are pinned by mutants M12-M14 and
  M10 instead of a RED commit.

The pairs at the base (`red-pairs.txt`, `scope.py red`): **3 of 20**
identical (`prompt`, `right`, `alt-f`: the moves that land where fx's
started). The base's run of `p5-rule-confirm-keys` stopped at its second
`until`: its C-j was swallowed, the typed second `remember` went to the
confirmation still up, whose Enter confirmed the first -- no second
confirmation came (`binaries.txt`'s last lines) -- so its last five snaps
are MISSING. In `p5-rule-confirm-keys-turn` the base showed an activity row
under the confirmation and let the turn finish after the Escape; in
`p5-rule-confirm-keys-cmd` the activity row and, after the Escape, the
command still running, then finished.

## GREEN (`green.txt`, `pairs.txt`)

- At the code head `e13010e6` (`green.txt`): the unit's tests and their
  neighbours (`rule_confirm remember revoke escape approval ctrl_j_
  keys_fx clock`) **320 passed, 0 failed**; `cargo test --locked --test
  parity` 26 passed.
- **The pairs** (`pairs.txt`, `scope.py xfx`: every ledger normalization,
  no entry set aside, styles compared): **18 of 20 identical** -- all 13 of
  `p5-rule-confirm-keys` (`prompt`, `tab`, `right`, `left`, `alt-f`,
  `alt-b`, `nothing-keys`, `ctrl-j-cancel`, `ctrl-j`, `ctrl-c`, `esc`,
  `kitty-ctrl-j`, and `turn`, the read denied under the rule a kitty Ctrl+J
  confirmed), `p5-rule-confirm-keys-turn`'s `prompt` (no activity row),
  and all 4 of `p5-rule-confirm-keys-cmd` (`prompt`, and `esc`, `esc-2`,
  `done`: the command cancelled by the Escape, its report the call's).
  Its `esc` and `end` differ in rows 4-7 only, by the card's blank row
  (fx's composer at row 4, xfx's at 5): every card xfx queues ends on a
  blank row (`queue_card`), fx's does not when nothing follows it. That is
  `amendment-card-after-stop`'s class, not a key; recorded as the ledger
  cause `rule-confirm-card-after-stop` (owner AMEND-CARD-ROW) and in
  `docs/parity.md`. Round 2 narrows that cause to those rows: rows 4-7
  whose text is the composer's `┃`, the status row or blank, on either
  side, so a row of a turn left running stays uncaused (the sweep below). The turn is stopped on both, and neither keystroke
  writes a cancellation row.
- **Not pinned by a shell test**: the running-command case. Its `Cancelled`
  rows come from the runtime's report of the killed command, which the
  shell fixture does not run; the `-cmd` pair is its receipt.

## Mutants (`mutants.py`, `mutants.txt`)

At `7e2e177d` (the code as at `e13010e6`), on a fresh `git clone
--no-local` throwaway of the branch (remote removed) under `$TMPDIR/mut/`,
one at a time, the files restored after each, under the suite lock; the
clone's tree was clean after the run. **14 of 14 killed**, one per key the
unit changes and one per Escape rule:

| mutant | killed by |
|---|---|
| M1 C-j swallowed at a rule confirmation | the C-j test |
| M2 Ctrl-C swallowed | the Ctrl-C test, `rule_confirm`'s key test |
| M3 Tab moves nothing | the move test, `rule_confirm`'s key test |
| M4 Left, Right, Alt-b and Alt-f move nothing | the move test |
| M5 C-p and C-n move the mark again | the do-nothing test |
| M6 the folded event again (Shift-, Ctrl-, Alt-, Super-Up and -Down move) | the do-nothing test, the move test |
| M7 Ctrl-L repaints again | the do-nothing test |
| M8 Escape stops nothing over a running turn | the in-turn, compaction and stop-guard Escape tests |
| M9 Escape stops a turn whether or not one runs | the idle Escape test |
| M10 an activity row under a rule confirmation again | the in-turn Escape test, the clock test |
| M11 Escape's stop writes the turn's cancellation row | the in-turn Escape test |
| M12 Escape stops a running turn only, not a compaction | the compaction test, the stop-guard test |
| M13 Escape asks a second stop while one is requested | the stop-guard test |
| M14 Escape stops no work typed since an earlier stop | the stop-guard test |

## Sweep (`sweep-base.md`, `sweep-head.md`, `sweep-compare.txt`)

`scripts/fxcap/sweep.sh --only 'approvals*' --only 'p5-approval*' --only
'p5-input-approval*' --only 'permissions*' --only 'p5-permissions*' --only
'p5-panels-perm*' --only 'allowlist*' --only 'rules-*' --only
settings-perm --only 'p5-rule-confirm*'` at 40x120, `--jobs 4`: every
`/permissions` and approval scenario, 30, this branch's scenarios as at
`7e2e177d`, once with the base's binary (`v0.1.0-1a0adae`) and once with
round 2's code head's (`v0.1.0-e13010e`), each in a suite-lock slot. The
ledger's first narrowing left the blank row out of its patterns, which
left one blank row of each head `-turn` pair uncaused, so both sweeps were
then classified again with the ledger as committed (`sweep.sh --rescore`,
the same captures; one ledger sha256, `1669c283…`, in both scoreboards).
Compared pair by pair with APPROVAL-READINESS-SETTLED's `cmp_sweeps.py`,
unchanged.

- **base 92 / 238, head 110 / 238, drops 0**; pairs with rows no cause
  covers: 14 at the base, 7 at the head.
- **`p5-rule-confirm-keys-turn` now shows the bug at the base**: its `esc`
  keeps the activity row (`~ Generating …`, row 5) uncaused and its `end`
  the answer's rows (`  Working on it.`, `  <wall> (↑4 ↓40)`); its
  `prompt` the activity row too. At the head `prompt` is
  normalized-identical, and `esc` and `end` are unresolved with every row
  under `rule-confirm-card-after-stop`, none uncaused.
- The rest of this unit's changes, all to normalized-identical:
  `p5-rule-confirm-keys`' 13 pairs (failed at the base, its run stopped,
  above); `p5-rule-confirm-keys-cmd`'s 4 (unresolved, 2-8 uncaused rows
  each).
- `permissions-btab` holds all 7 pairs on both sides with its new `until`
  lines (`sweep-compare.txt`); round 1's two fx-side drops are gone.
- No other pair changed class, cause or count. The 7 pairs with uncaused
  rows at the head are the same 7 as at the base, with the same counts
  (`p5-input-approval` `cmd-ask` 15, `edit-ask` 17, `after` 13, as at
  APPROVAL-CTRL-J; `rules-read` `ask` 1, `ask2` 1, `listed` 10;
  `rules-read-auto` `read-ask` 1); this unit touches no row of them.

## Gate (`gate.txt`)

Round 2's gate, once, under the suite lock with a short `TMPDIR`, at
`7e2e177d` (the code as at the code head `e13010e6`; `src/` and `tests/` as
committed; this receipt edited, uncommitted). Every step exits 0, and
`src/` and `tests/` were unchanged at the end. Each test step's 15 raw
`test result:` lines are kept in `gate.txt`, then their sum:

| step | result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy --locked --all-targets -- -D warnings` | clean |
| `cargo clippy --locked --all-targets --features fault-injection -- -D warnings` | clean |
| `cargo test --locked --no-fail-fast` | 5584 passed, 0 failed, 7 ignored (15 result lines) |
| `cargo test --locked --features fault-injection --no-fail-fast` | 5621 passed, 0 failed, 7 ignored (15 result lines) |
| `scripts/check-xfx-identity.sh`, `check-no-stubs.sh`, `check-no-secrets.sh` | exit 0 each |

5584 is round 1's 5581 (APPROVAL-CTRL-J's 5576 and round 1's five) plus
round 2's three (`diffstat.txt`); 5621 is round 1's 5618 plus the same
three. No test here builds a `Runtime`. The three checks ran again with
this receipt committed (`gate.txt`'s last lines). Every commit's added
lines were scanned for local paths before it was made (`git diff --cached`).

## Parity (`docs/parity.md`) and the ledger

- The `/permissions` row: the sentence that gave the band `1`, Enter, `2`
  and a silent Escape now says its keys are the approval's -- C-j and
  Ctrl-C, Tab and the horizontal moves, what does nothing, Escape's stop
  over a turn with no row of its own (a running command's report still
  says `Cancelled`), no activity row under it, no readiness check --
  and records the review screen as not copied (RULE-CONFIRM-SCREEN) and
  the card's row after the Escape as still apart (AMEND-CARD-ROW).
- The full-screen TUI row's approval sentence: the clause that left
  RULE-CONFIRM-KEYS proposed now says xfx's confirmation reads the
  approval's keys.
- `fx_src.sh` checks those citations with the unit's added lines that are
  not moves (`added.py`) and this file (`fx-src-check.txt`).
- The ledger's refs into `src/tui/shell.rs`, by content, with this
  receipt's `refs.py` (APPROVAL-VERTICAL-SPELLINGS' with the new cause's
  `queue_card` refs added; `python3 -I refs.py 1a0adaec`): `refs.txt`, 19
  checked, 0 wrong.
  `--fix` re-pointed the ones each code commit moved. One new entry, the
  cause `rule-confirm-card-after-stop` (above, `kind = ["unit",
  "defect"]`, its `xfx`/`fx` refs, its row patterns); it approves nothing.
- The readiness decision's scope (round 2): the ledger's
  `approval-settled-every-approval` and the `/permissions` row each gain a
  sentence naming the rule confirmation, a persisted grant reachable by a
  move and C-j or Enter in one write with no readiness check, as in fx;
  the behaviour is unchanged and the decision stays the user's.

## Safety

No new string reaches the terminal; nothing writes a file, runs a process or
reaches the network beyond what Confirm already did. The keys added are an
affirmative (C-j, as Enter) and refusals (Ctrl-C; Escape's stop). The
newline habit APPROVAL-CTRL-J describes applies here too: a C-j typed for a
new line just as a confirmation opens confirms it, as in fx.
