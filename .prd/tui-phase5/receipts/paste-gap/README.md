---
title: 0.0.16 delta -- PASTE-GAP receipt (input after a paste stays quarantined until the input is quiet)
type: receipt
tags: [xfx, tui, phase5, input, paste, security]
---

# PASTE-GAP: a readiness gap no longer turns the rest of a clipboard into keys

**Security (S).** Base `a832f4d7` (PASTE-AFTER-END, which copied fx's rule:
input behind an embedded `ESC[201~` is dropped **within one delivery**).
Branch `fix/p5-paste-gap`. fx read only at the pin `481104fd`
(`fx-src.txt`). Every claim below names a file in this directory.

| commit | what |
|---|---|
| `0b65e602` | RED, tests only: four cases on the base source (`red-unit.txt`) |
| `ea2b8a60` | GREEN: the quarantine, the review leftovers, parity.md, the pty harness pause. **The binary every head measurement, the head sweep and the mutants used** (banner `v0.1.0-ea2b8a6`, checked in the binary) |
| `1fe2496a` | this receipt, but for the gate |
| the last commit | the gate's log (`gates.txt`) and this README's §9; receipt files only |

## 1. Measured

**The terminals** (`terminals.md`; source at pinned commits, the tmux rows also
measured in `measure.txt`). No layer can be relied on to strip the marker:
kitty, Alacritty, VTE, foot and Windows Terminal strip it; WezTerm's
single-pass replace lets a nested marker through; xterm and iTerm2 strip by
default but let the user turn it off; tmux `paste-buffer` vis-escapes it
(3.7 and later) unless `-S`; Terminal.app is closed and unverifiable. **tmux
in front of xfx strips nothing**: it ends a paste at the first embedded
`201~` (`tty-keys.c:213-214,641-646@7f2a35ad`) and forwards the rest as keys.

**The gap** (`gap-probe.txt`, `gap_probe.py`: a reader shaped like xfx's turn
on a raw pty, a writer process). A raw pty takes 1022 bytes before its writer
waits (`../paste-after-end/pty-queue.txt`). Paste sizes 1 KiB to 1 MiB: at
64 KiB the paste reached the reader as up to 54 deliveries, at 1 MiB as up to
403; the longest refill gap was 3.7 ms, at load averages up to 20.6 on 18
cores. (A run with one `yes` per core under the exclusive lock never got the
lock -- every slot was held by other suites for 15 minutes -- and measured
nothing; it is not claimed.)

**A real xfx** (`measure.txt`, `paste_xfx.py`). One paste
`ESC[200~ early-paste-text ESC[201~ <body> CR ESC[201~` -- a clipboard with an
embedded marker and a trailing newline -- 1 KiB to 1 MiB:

| mode | base `a832f4d7` | head `ea2b8a60` |
|---|---|---|
| pty, the paste one write | 1 KiB, 4 KiB: dropped (one delivery). **64 KiB: the paste's own CR submitted it in 2 of 3 runs** (14 848 and 31 488 bytes dropped, the rest typed); 1 MiB: submitted | every size, every run: nothing typed, nothing submitted, the warning |
| tmux `paste-buffer -p` | the marker arrives as the text `^[[201~` inside one paste: no keys, no warning | the same |
| outer terminal -> tmux -> xfx | 1 KiB dropped; **4 KiB, 64 KiB, 1 MiB: submitted in every run** (8 of 8): tmux forwards the rest as keys, and the first delivery ends within the first 1 KiB of the body (4 KiB at 1 MiB, that size's token spacing) | every size, every run: nothing typed, nothing submitted, the warning |

So the answer to "does a real path exist" is **yes**, on a plain pty for large
pastes and through tmux for every paste of 4 KiB or more: case (a).

## 2. The decision

**Candidate 1, the quarantine, with three corrections to its wording**; the
reasons are in `docs/parity.md`'s `full-screen TUI` paste paragraph, which
records it as a security divergence from fx.

- **Rejected: a CR after a gap is text.** The keys that act with no Enter would
  still be keys -- `a_choice_after_a_readiness_gap_behind_a_paste_answers_nothing`
  (`src/tui/shell.rs:37940`) is a `1` answering an approval on the base
  (`red-unit.txt`) -- and Ctrl-C, Ctrl-D, a double Escape and Tab likewise; and
  a 64 KiB paste reached the probe as up to 54 deliveries, so its newline need
  not be in the first one after the paste.
- **Every paste, not only one whose delivery dropped bytes.** The first
  delivery ends near the 1022-byte queue (`measure.txt`: 999, 1024); a
  clipboard whose embedded marker ends right there settles with nothing
  dropped. Mutant 6 is that narrower trigger, and it is killed.
- **Quiet is two empty checks, not the clock.** The quarantine ends only when
  two readiness checks `PASTE_QUIET` apart find nothing with no byte fed
  between them (`src/tui/input.rs:522`), so a loop that stalls between two
  reads of a paste still reads it as one. Mutant 5 is the clock version.
- **fx's settle point is kept.** The paste still lands at the first empty
  check, as fx's does; only what follows it is held. So no capture changes
  (the sweep, below), and the window in which a question can open between a
  paste's bytes and its `PasteEnd` is no wider than at the base.
- **The bound:** `Decoder::PASTE_QUIET` = 50 ms (`src/tui/input.rs:484`),
  `ESC_TIMEOUT`'s bound, 13 times the longest gap measured. **The cost:** a key
  pressed within 50 ms (plus a turn, 8 ms) of a paste's last byte is dropped,
  with the paste's warning. **Not covered:** a terminal that pauses 50 ms or
  more inside one paste (a slow ssh link can) ends the quarantine there, as fx's
  delivery ends at any pause.

## 3. The code (`ea2b8a60`)

- `src/tui/input.rs`
  - the module doc, `# And stays quarantined until the input is quiet` (:121);
  - `Delivery::Ignored { dropped }` (:424): input was dropped in a quarantine, the warning is owed, once a paste;
  - `Quarantine` (:430) and the decoder's `quarantine` (:462);
  - `PASTE_QUIET` (:484);
  - `end_delivery(now)` (:522) starts the quarantine where it settles the paste, and ends it;
  - `feed` (:567) counts a quarantined byte and restarts the quiet.
- `src/tui/shell.rs`: `end_delivery` -> `end_delivery_at(now)` (:10375), `Ignored` is the same warning (:10384).
- `src/tui/event_loop.rs`: `burst` is unchanged; its doc says where the boundary now is (:628).

## 4. RED -> GREEN

- `red-unit.txt` (`0b65e602`, the base source): 4 of 4 fail --
  - the decoder reads `x`, an Enter and an arrow after the gap;
  - the composer sends a prompt (`/version` and an Enter typed after the gap);
  - an approval is answered `Once` by `1` + Enter;
  - the loop's own turn (`burst`, two turns) submits the draft.
- `green-unit.txt` (`ea2b8a60`): those four and the new pins, 12 of 12 `ok`:
  - the quiet rule (`:3005`): a byte restarts the quiet; a 10 s stall with a read in it is not quiet;
  - the warning owed once (`:3069`);
  - the noise property test (`:2798`), now with quiet steps; 207 of its streams reached a quarantine, floor 16;
  - the burst arms and the modal cases (§5).
- `measure.txt`: the base rows above against the head rows.

## 5. The review leftovers of PASTE-AFTER-END

1. **The overclaim** ("A person cannot press a key inside that window") is
   gone; the module doc now says every byte in that window is the clipboard's
   whatever its origin (`src/tui/input.rs:115`), and `feed`'s comment
   likewise.
2. **`burst`'s other arms**, through the seam:
   - `a_turn_cut_short_by_a_signal_or_a_read_that_would_wait_settles_nothing`
     (`src/tui/event_loop.rs:16090`): a readiness `Interrupted`, a read
     `INTR` and a read `WOULDBLOCK` end the turn with the paste unsettled, and
     an Enter in the next turn is still behind it;
   - `a_read_of_nothing_leaves_the_session_and_settles_nothing` (`:16138`):
     `Ok(0)` leaves and settles nothing.
3. **The citation** at `shell.rs` `end_delivery`: the function at
   `app_input_runtime.zig:790-813@481104fd` is
   `settleTerminalPasteDeliveryEpochWithLimits`; the name is corrected
   (`src/tui/shell.rs:10357`) and the lines are in `fx-src.txt` §2.
4. **Over-width comments**: `input.rs`'s `feed` comment and `tests/tui.rs:3367`
   rewrapped.
5. **A modal mid-delivery** (`src/tui/shell.rs:37963`, `:38003`):
   - a question that opens between a paste's bytes and its end takes the
     `PasteEnd`: nothing is answered, the warning is said, and the question
     takes its own key afterwards. The paste the composer gathered is dropped
     rather than applied behind the panel; fx fixes the owner at `beginPaste`
     (`fx-src.txt` §3) and applies it. Recorded in parity.md. Not changed here:
     it is reachable only when a delivery spans two turns (the 32-read cap or a
     signal), this unit does not widen it, and owner routing is a change to the
     focus rules rather than to the input boundary;
   - a question **cannot** close in that window: it closes only on its own keys
     (`decide`, `answer`), and nothing there is a key. Pinned with the turn's
     end taken mid-delivery: the draft that took the start keeps `keepnew`.

## 6. Test harnesses

A test's next key after a paste comes a millisecond after the screen shows it,
a person's later than 50 ms. `tests/support/pty.rs` (`PASTE_PAUSE`, :49;
`type_bytes`, :339) and `scripts/smoke-tui.sh` (`PASTE_PAUSE`, :2576; `send`,
:3033) wait 250 ms before the write that follows one carrying `ESC[201~`. It is
taken whole at that write, after whatever the test waited to see: counted from
the paste's own write, six pty tests failed on a loaded machine
(`harness-pause.txt`) -- likely because a debug build there takes most of
250 ms to read and settle a paste of a few kilobytes, which was not measured.
What one write carries behind an end marker is still written with it, so the
same-delivery pty tests are unchanged. No fxcap scenario types straight after a
paste (a `wait` comes between), so none needed anything.

## 7. Mutants (`mutants.py`, `mutants.txt`)

Each on a throwaway `git clone --no-local` of the branch at `ea2b8a60` (never
the worktree; `GIT_DIR`, `GIT_INDEX_FILE`, `GIT_WORK_TREE` unset), one textual
change, then `cargo test --locked --lib tui::` and the two pty paste tests,
each run in its own process group with a 900 s limit, the change undone after.
Under the suite lock. **10 of 10 killed.**

| # | the rule reverted | killed by |
|---|---|---|
| 1 | **fx's boundary: no quarantine after a paste** | 7 tests |
| 2 | a quarantined byte is decoded | 7 |
| 3 | a byte inside the quarantine does not restart the quiet | 1 (the quiet rule) |
| 4 | one empty check is quiet enough | 2 |
| 5 | quiet is the clock since the last byte, not two empty checks | 2 (the stall, the noise) |
| 6 | the brief's narrower trigger: a quarantine only after a paste whose delivery dropped bytes | 7 |
| 7 | input dropped in a quarantine is never said | 3 (shell and loop) |
| 8 | a quarantine says its warning at every check that dropped something | 2 |
| 9 | a paste whose delivery was said is said again by its quarantine | 1 (owed once) |
| 10 | PASTE-AFTER-END's mutant 1: the bytes behind the marker are decoded | 23 |

Mutants 4 and 5 are killed by the decoder's cases with explicit times alone:
the shell and loop gap cases put no idle check inside the gap, which is the
shape the measured gaps have (all under one 8 ms turn, `gap-probe.txt`).

## 8. The sweep (`sweep-base.md`, `sweep-head.md`, `sweep-compare.txt`)

The 41 input scenarios PASTE-AFTER-END swept (every scenario that pastes, every
`p5-input-*`, `question*` and `approvals*`; the list is in each scoreboard's
header) on `a832f4d7`'s build and on `ea2b8a60`'s, `--jobs 2`, the same ledger
(sha256 `530ac71a…`) and the same scenario files, fx v0.0.16's hash checked by
the sweep. Under the suite lock, one after the other.

- **base 200 / 253, head 200 / 253, drops 0**; identical 10 / 10,
  normalized-identical 190 / 190 (`sweep-base.md`, `sweep-head.md`).
- `sweep-compare.txt`: **no pair changed class, cause or uncaused count.**
  That is the design: the paste lands at fx's check, and nothing a scenario
  types comes within 50 ms of a paste (no `.cap` file has a `key`, `type`,
  `paste` or `relaunch` line straight after a line that pastes: a `wait`
  comes between).
- `green.txt`: `cells.py` over the head sweep's own paste captures. Every
  `p5-input-paste`, `p5-input-paste-behind`, `p5-input-paste-field` and
  `paste-suffix` pair differs in 0 rows. `p5-input-paste-amend` differs in
  6/6/6/6/6/5, PASTE-AFTER-END's grant-scope rows
  (`../paste-after-end/amend-rows.txt`), on the base as on the head. Row 1 of
  the head captures is `v0.1.0-ea2b8a6`, of the base's `v0.1.0-a832f4d`.

## 9. Gate (`gates.txt`)

Every CI step of `ci.yml`, in order, at **`c62ce382`**, under the suite lock
(one slot for the run). That head is a receipt-only commit: every file the
binary is built from is as at `ea2b8a60`. **All exit 0.**

| step | result |
|---|---|
| fmt, clippy, fault-injection clippy | exit 0 |
| `cargo test --locked --all-targets` | 14 binaries, **5459 passed**, 0 failed (PASTE-AFTER-END: 5449; +10, this unit's new tests) |
| with the fault injector | 15 binaries, **5497 passed**, 0 failed (5487; +10) |
| no-stubs, no-secrets, gate-env, identity | exit 0 |
| preview contract and its mutants, preflight cost, usage clock | exit 0 |
| release and faulty builds, `smoke.sh` | exit 0 |
| `smoke-tui.sh` | 37 scenarios + the oracle, **1167 checks, 0 failures**; `7-multiline-and-paste-framing` and `21-paste-entities` `ok` |

Every new test is `ok` in the `test` log, and so are the two pty paste tests
and the 32-read carry-over (`gates.txt`). A first queued run never got a slot
under its 3 h bound and was stopped before any step; this one ran with a 6 h
bound.

## parity.md

`full-screen TUI`, the paste paragraph:
- "That is the whole of the guarantee" became the divergence: what fx does,
  what was measured, the quarantine and its bound, its cost, what it does not
  cover, and why the newline-only hardening was rejected;
- "a key in a later delivery is a key" became "a key once the input has been
  quiet is a key";
- the mid-delivery modal difference, with fx's `beginPaste`.
