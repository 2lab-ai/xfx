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
| `071b26e0` | **the code head**: `Shell::modal` counts a rule confirmation; its Escape over a running turn stops it with no cancellation row; the parity sentence; moved refs |
| `bfa9ceec`, `18f5dd91` | this receipt; `docs/parity.md`'s reason for fx's Escape, the card's row as still apart, a citation named with its file; the ledger cause `rule-confirm-card-after-stop`; the quick checks rerun with it tracked; no `src/` change |
| the commit after | round 1's own review: the scenario `p5-rule-confirm-keys-cmd` (Escape over a running command), both sweeps rerun over the same 30 scenarios and one ledger, the parity sentence's running-command clause; no `src/` change |

Captures, all three scenarios at 40x120: `fx/` is fx v0.0.16 (sha256 `8e6cda2e…`,
`FX_AUTO_UPGRADE=0`; `capture.sh` checked its bytes after each run), `red/`
the base's binary (row 1 `x𝒇x v0.1.0-1a0adae [debug]`, built at `1a0adaec`),
`xfx/` the code head's (row 1 `x𝒇x v0.1.0-071b26e [debug]`, built after
the commit). Hashes, banners and guard lines: `binaries.txt`.

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
leaving the prompt up (`app_input_runtime.zig:2761-2764`). **But fx took the
first branch at every Escape traced, idle ones included**: `probe/trace.txt`
(fx's own `FX_TRACE_LOG` of `probe/probe.cap`, eight scopes) shows `[input] cancel
approval operation queued=0` -- that function's own line
(`input_approval_runtime.zig:421-424`) -- at an idle Escape after a
`/permissions remember` and again after a finished turn; that function
clears the prompt (`:428`), and `fx/fx-p5-rule-confirm-keys-40x120-esc`
shows an idle one closing it. **Why**: while an approval prompt is up fx's `syncState` marks its stream active (`modal_active and !app.stream.active` -> `app.stream.active = true`, `app_worker_runtime.zig:729`, `:753-757`), so `hasActiveOperation` holds at every rule confirmation and the decoder latches it with the Escape byte (`terminal_action_decoder.zig:42-45`, `:180-182`). `probe/esc-all-trace.txt` (every scope, `probe/esc-all.cap`) shows it: the Escape's `cancel_requested` line reads `stream_active=true` with no turn ever started. At an idle confirmation that route only clears the prompt: there is no turn to stop. So xfx's band keeps closing on an idle
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
long for the band takes fx's review screen (`needsScreen`), where xfx's band
wraps it whole; proposed owner RULE-CONFIRM-SCREEN.

## The change (`diffstat.txt`)

2 files, +412 / -132; outside the `mod tests` blocks `src/tui/shell.rs` +130 / -99 and `src/tui/rule_confirm.rs` +26 / -24 -- most of `shell.rs`'s being `Shell::decide`'s head moved below `Shell::approval_action`: 80 of its added lines (non-blank) are a removed line again, by `added.py`'s reading. Five `#[test]`s added, none removed; `rule_confirm.rs`'s key test is rewritten in place.

- `Shell::approval_action`, moved out of `Shell::decide`, whose head moves
  below it (most of the removed and added lines outside the tests are that
  move); `Shell::offer_to_rule_confirm` reads it, and Escape's stop is
  there; `Shell::modal` counts `rule_confirm` (`src/tui/shell.rs`).
- `consume_inputs` hands the rule confirmation the spelled event, ahead of
  `folded`.
- `RuleConfirm::apply(approval::Action)` (`src/tui/rule_confirm.rs`).

## Decisions (each reversible)

- **Escape stops a running turn or compaction only**, read as xfx's composer
  Escape reads fx's operation in hand (`self.activity.working() ||
  self.compacting()`), not fx's `hasActiveOperation`, which counts a
  provider preparation too. A stop already requested is not asked again
  (`Shell::cancel_turn`'s rule).
- **An idle Escape closes the band**, copying the captures and the trace,
  not the branch the code reads as taken (above).
- **`Shell::modal` counts the rule confirmation**, not a narrower check at
  the activity row: every reader of `modal` is about an approval being up
  (the clock freeze, the activity row, the MCP status and full-access
  warning slots, `C-o`), and fx's rule confirmation is its approval prompt.
  The sweep is the check that nothing else moved.

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

- At the code head `071b26e0` (`green.txt`): the unit's tests and their
  neighbours (`rule_confirm remember revoke escape approval ctrl_j_
  keys_fx`) **288 passed, 0 failed**; `cargo test --locked --test parity`
  26 passed.
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
  `docs/parity.md`. The turn is stopped on both, and neither keystroke
  writes a cancellation row.
- **Not pinned by a shell test**: the running-command case. Its `Cancelled`
  rows come from the runtime's report of the killed command, which the
  shell fixture does not run; the `-cmd` pair is its receipt.

## Mutants (`mutants.py`, `mutants.txt`)

At the code head `071b26e0`, on a fresh `git clone --no-local` throwaway of
the branch (remote removed) under `$TMPDIR/mut/`, one at a time, the files
restored after each, under the suite lock; the clone's tree was clean after
the run. **11 of 11 killed**, one per key the unit changes and one per
Escape rule:

| mutant | killed by |
|---|---|
| M1 C-j swallowed at a rule confirmation | the C-j test |
| M2 Ctrl-C swallowed | the Ctrl-C test, `rule_confirm`'s key test |
| M3 Tab moves nothing | the move test, `rule_confirm`'s key test |
| M4 Left, Right, Alt-b and Alt-f move nothing | the move test |
| M5 C-p and C-n move the mark again | the do-nothing test |
| M6 the folded event again (Shift-, Ctrl-, Alt-, Super-Up and -Down move) | the do-nothing test, the move test |
| M7 Ctrl-L repaints again | the do-nothing test |
| M8 Escape stops nothing over a running turn | the in-turn Escape test |
| M9 Escape stops a turn whether or not one runs | the idle Escape test |
| M10 an activity row under a rule confirmation again | the in-turn Escape test |
| M11 Escape's stop writes the turn's cancellation row | the in-turn Escape test |

## Sweep (`sweep-base.md`, `sweep-head.md`, `sweep-compare.txt`)

`scripts/fxcap/sweep.sh --only 'approvals*' --only 'p5-approval*' --only
'p5-input-approval*' --only 'permissions*' --only 'p5-permissions*' --only
'p5-panels-perm*' --only 'allowlist*' --only 'rules-*' --only
settings-perm --only 'p5-rule-confirm*'` at 40x120, `--jobs 4`: every
`/permissions` and approval scenario, 30, this branch's scenarios and
ledger as committed with this receipt (one ledger sha256, `a81acdd9…`, on
both sides), once with the base's binary (`v0.1.0-1a0adae`) and once with
the code head's (`v0.1.0-071b26e`), each in a suite-lock slot; compared
pair by pair with APPROVAL-READINESS-SETTLED's `cmp_sweeps.py`, unchanged.
A first pair of sweeps is not the receipt: the base's had loaded the ledger
before the cause was added, while its scoreboard hashed the file after, and
neither had the `-cmd` scenario; both were run again.

- **base 92 / 238, head 108 / 238**; pairs with rows no cause covers: 12 at
  the base, 9 at the head.
- This unit's changes, all to normalized-identical but two:
  `p5-rule-confirm-keys`' 13 pairs (failed at the base, its run stopped,
  above); `p5-rule-confirm-keys-cmd`'s 4 (unresolved, 2-8 uncaused rows
  each); `p5-rule-confirm-keys-turn`'s `prompt` (2). Its `esc` and `end`
  are unresolved on both sides with every row under
  `rule-confirm-card-after-stop`.
- **Two drops, both fx's side, not reproduced**: `permissions-btab`'s
  `btab-auto` and `btab-full-again`, where fx's status row stayed `ask ·
  model-a` after two more Shift-Tabs and xfx's read `auto` and `full
  access`, as it should (`sweep-compare.txt`'s last lines). The scenario
  has no rule confirmation and the key reaches no code this unit changed;
  run alone three times under the lock with the head's binary it passed 7
  of 7 each time (`sweep-compare.txt`'s end), as both its pairs did in the
  first pair of sweeps.
- No other pair changed class, cause or count. The 7 other pairs with
  uncaused rows at the head are the same 7 as at the base, with the same
  counts (`p5-input-approval` `cmd-ask` 15, `edit-ask` 17, `after` 13, as
  at APPROVAL-CTRL-J; `rules-read` `ask` 1, `ask2` 1, `listed` 10;
  `rules-read-auto` `read-ask` 1); this unit touches no row of them.

## Gate (`gate.txt`)

The brief's gate, once, under the suite lock with a short `TMPDIR`, at the
code head `071b26e0` (`src/` and `tests/` as committed; this receipt
untracked; `docs/parity.md` with one later edit, a citation named with
its file; the ledger as committed). Every step exits 0, and `src/` and `tests/` were unchanged at
the end:

| step | result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy --locked --all-targets -- -D warnings` | clean |
| `cargo clippy --locked --all-targets --features fault-injection -- -D warnings` | clean |
| `cargo test --locked --no-fail-fast` | 5581 passed, 0 failed, 7 ignored (15 result lines) |
| `cargo test --locked --features fault-injection --no-fail-fast` | 5618 passed, 0 failed, 7 ignored (15 result lines) |
| `scripts/check-xfx-identity.sh`, `check-no-stubs.sh`, `check-no-secrets.sh` | exit 0 each |

5581 is APPROVAL-CTRL-J's 5576 plus this unit's five (`diffstat.txt`);
5618 its 5613 plus the same five. No test here builds a `Runtime`. After the
gate, `docs/parity.md` gained the trace's reason and the card's row, and
the ledger the cause, and in round 1's review the running-command clause:
`cargo test --locked --test parity` passed on each (`green.txt`;
`gate.txt`'s last lines), and the three checks ran again with this
receipt committed (`gate.txt`'s last lines). Every commit's added lines were scanned for
local paths before it was made (`git diff --cached`).

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
- The ledger's refs into `src/tui/shell.rs`, by content, with
  APPROVAL-VERTICAL-SPELLINGS' `refs.py`, unchanged (`python3 -I
  ../approval-vertical-spellings/refs.py 1a0adaec`):
  `refs.txt`, 17 checked, 0 wrong. `--fix` re-pointed the ones each code
  commit moved. One new entry, the cause `rule-confirm-card-after-stop`
  (above); it approves nothing.

## Safety

No new string reaches the terminal; nothing writes a file, runs a process or
reaches the network beyond what Confirm already did. The keys added are an
affirmative (C-j, as Enter) and refusals (Ctrl-C; Escape's stop). The
newline habit APPROVAL-CTRL-J describes applies here too: a C-j typed for a
new line just as a confirmation opens confirms it, as in fx.
