---
title: 0.0.16 delta -- PASTE-GAP round 2 receipt (a second paste stays framed; dropped input is said while it lasts)
type: receipt
tags: [xfx, tui, phase5, input, paste, security]
---

# PASTE-GAP round 2

Round 1 closed at `fdad4996`, and both engines approved it with no MUST.
The dispatcher sent round 2 for opus SHOULD 1 (a benign double paste could
submit) and the other SHOULDs. Every claim below names a file in this
directory, or in `..` for round 1's.

| commit | what |
|---|---|
| `1937f080` | RED, tests only, on `fdad4996`'s source: six cases (`red-unit.txt`) |
| `227e00d5` | GREEN for those: a start marker in the quarantine begins a paste; the repeat; the pins; `write_raw` and the two mebibyte pty cases; wording |
| `de61bcd6` | RED, tests only, on `227e00d5`'s source: four cases for the same hole through the count behind the end marker (`red-unit-behind.txt`), found by this round's real-process run |
| `8ccf2590` | GREEN for those: the markers are followed through `behind` and the quarantine |
| `45dffff4` | test only: a start marker inside a paste being dropped is its content. **Round 2's code head**: the measurements, mutants, sweep and gate ran here (banner `v0.1.0-45dffff`) |
| the commits after | this receipt; receipt files only |

Round 2 adds 209 non-test source lines; the unit's total is 311.

## 1. The double paste (opus SHOULD 1)

The hole had two doors:
- **A start marker after the settle, in the quarantine.** It was dropped with
  everything else. A gap of `PASTE_QUIET` inside that second paste then ended
  the quarantine, and the rest of the paste was read as keys.
- **A start marker before the settle, behind the end marker in the same
  delivery.** It was counted, which is fx's rule, and never followed. The
  same gap had the same effect. `measure.txt`: round 2's first code commit,
  `227e00d5`, still submitted in this case (3 of 3). It is the case a real
  terminal makes when one write carries both pastes' markers.

The design is the dispatcher's "follow the framing while you drop the bytes",
plus one difference, argued here:
- **Behind an end marker (the same delivery):** every byte is counted, as fx
  does (`paste_framing.zig:102-107,527-546@481104fd`), and the markers are
  followed (`src/tui/input.rs:647`, `follow` :742).
  - A start marker opens a paste that is being dropped, as fx drops it, until
    that paste's own end marker.
  - No quiet ends the quarantine while that paste is open (`whole`, :606).
  - This is the dispatcher's design.
- **In the quarantine, outside a dropped paste:** a start marker **begins a
  real paste** (:653), as fx and the base decode that marker after any gap.
  This is where it departs from the dispatcher's design, and why:
  - **Security is equal.** The bytes after a start marker are a paste's
    content either way, never keys.
  - **A benign second paste is kept** instead of dropped. gpt6 listed the
    discarded second paste as a cost of round 1; this removes it.
  - **fx parity.** It is what fx does with that sequence once a delivery has
    ended.
  - **Same window.** Its end marker settles it, and a fresh quarantine
    starts.
  - **Measured.** `measure.txt` `late`: round 1 sent `paste` by itself.
    `45dffff4` sent nothing, and the composer held
    `early-paste-textsecond-paste\n/version`.
- **Fail-closed, and why nothing a clipboard holds can wedge the input:**
  - **No nesting.** Inside any paste, dropped or real, only its end marker is
    read (`follow`, as `pasted` reads). A second start marker inside it is
    content, not a frame within a frame. The terminal writes one end marker
    after every paste, and it closes whatever the clipboard opened. With
    depth counting, two embedded start markers would leave a frame open on
    clipboard content alone. Pinned:
    `a_start_marker_inside_a_paste_being_dropped_is_its_content_and_one_end_closes_it`
    (:3458).
  - **A half-read marker holds the quarantine.** The next byte completes or
    breaks it and is dropped with it:
    `a_start_marker_split_across_the_quiet_holds_the_quarantine` (:3372) and
    `…_split_across_the_settle_…` (:3508).
  - **Cost:** an Escape pressed inside the window holds the quarantine until
    the next key, and that key is dropped too. Round 1's
    `an_escape_sequence_behind_the_end_marker_…` was changed to show this.
  - **An unterminated paste holds the input until an end marker comes.** It
    is held as content (begun in the quarantine) or dropped (begun behind the
    end marker). That matches an unterminated paste in fx, which has no
    timeout (`paste_framing.zig:502-511@481104fd`; capture survives epochs;
    `app_input_runtime.zig:9998` shields it from the escape timeout), and at
    the base. Pinned at :3345 and :3483.
  - **Liveness**, stated in `burst`'s doc (`src/tui/event_loop.rs:635`) and
    in parity.md:
    - input itself, because each dropped byte restarts the quiet;
    - a half-read marker, until the next byte;
    - a second paste, until its own end marker, which a terminal always
      writes;
    - an outside signal ends the session either way.
- **Owed warning.** What the quarantine dropped before a start marker is still
  said: on its own while the new paste is open, or folded into that paste's
  `Pasted { dropped }` (:3532).

**RED.** `red-unit.txt` (6 of 6 fail on `fdad4996`) and
`red-unit-behind.txt` (4 of 4 fail on `227e00d5`). Each fails on the intended
assertion: keys after the gap, a prompt sent, the quarantine ending with a
paste open, the warning said once.

**GREEN.** `green-unit.txt`, 18 `ok` at `45dffff4`, every round-2 case among
them. `measure.txt`, double paste:

| build | `double` | `late` |
|---|---|---|
| round 1 (`ea2b8a60`) | sends `paste`, 3 of 3 | sends `paste`, 3 of 3 |
| `45dffff4` | nothing sent, 3 of 3 | nothing sent, 3 of 3; the second paste kept as pasted text |

`227e00d5` also sends in `double`, 3 of 3.

## 2. The restart mutant (opus SHOULD 2)

`a_quarantine_ends_at_the_first_empty_check_at_or_past_the_quiet` (:3564):
- six empty checks `TICK` apart before the deadline, each still quarantined;
- the first check at or past `PASTE_QUIET` (56 ms) ends it.

Mutant 7 (`Some(_) => quarantine.quiet_since = Some(now)`) dies in the unit
tests (`mutants.txt`).

## 3. The constant (opus SHOULD 3)

`paste_quiet_stays_within_what_was_measured` (:3590):
- **Lower bound:** `PASTE_QUIET >= 4 x 3.716 ms`. That is the longest refill
  gap measured (`../gap-probe.txt`); four times covers a slower machine's
  refill inside one paste.
- **Upper bound:** `(PASTE_QUIET + TICK) x 4 <= 250 ms`. That is both
  harnesses' `PASTE_PAUSE`, so paced test input clears the window four times
  over on a loaded machine.
- **And** `PASTE_REPEAT > PASTE_QUIET`.

50 ms sits inside both bounds and stays (dispatcher decision).

## 4. Wording (opus SHOULD 4, gpt6 SHOULD 3)

The window is now described as about `PASTE_QUIET` + `TICK` + the turn's work,
counted from when xfx reads the paste's last byte, and restarted by each
dropped byte. Fast legitimate input inside it is dropped and the warning is
said; the harnesses deliberately model paced input. This wording is in:
- `docs/parity.md`, the `full-screen TUI` paste paragraph;
- `src/tui/input.rs`'s module doc (:137) and `PASTE_QUIET`'s doc;
- `Shell::route_bytes`'s doc;
- `tests/support/pty.rs` `PASTE_PAUSE` (:52);
- `scripts/smoke-tui.sh`'s `PASTE_PAUSE`.

"A person's next key comes later than 50 ms" is gone from all of them.

The two receipt sentences the dispatcher named were already accurate in round
1's README:
- ../README.md:197-202 names `p5-input-paste-amend`'s 6/6/6/6/6/5 rows;
- ../README.md:149-151 says the six pty tests failed **with the pause counted
  from the paste's own write**, which is what `../harness-pause.txt` records.
  There was never a run with no pause.

The overclaims ("every paste pair 0 rows", "without it, 6 failed") were in
round 1's hand-back report, not in the receipt.

## 5. Visibility (gpt6 SHOULD 1)

While dropped input keeps arriving, xfx says
`! input: Input is still being ignored until it stops for a moment.` at most
once a second (`Decoder::PASTE_REPEAT`, `src/tui/input.rs:544`;
`PASTE_STILL`, `src/tui/shell.rs:553`):
- it goes on the plane the user can see, the document or the review plane's
  status row, through the same `said_after_paste` (:12168) as the first
  warning;
- nothing is decoded on a timer.

**Why not a persistent notice line:**
- the band's notice is cleared by the next keystroke (`shell.rs`'s `decide`,
  :9331);
- while a question is up, its hint row belongs to the question (:5082);
- so a line there would be invisible at an approval under attack, which is
  where it matters.

**The cost is bounded:**
- a 1 MiB hostile paste through tmux adds one or two rows;
- a held key under 50 ms autorepeat adds one row a second while held.

Pinned at the shell (`input_that_keeps_arriving_inside_the_quarantine_is_said_again_once_a_second`,
`shell.rs:18978`): the first warning, one repeat a second later, none once the
input stops, and a key works again after the quiet. It is RED on `fdad4996`.

## 6. Unpaced pty path (gpt6 SHOULD 2, opus SHOULD 5)

`Session::write_raw` (`tests/support/pty.rs:358`) takes no `PASTE_PAUSE`
before it and arms none after it. `type_bytes` uses it internally.

Two in-CI pty cases write `hostile_paste` (`tests/tui.rs:3417`) raw in 4 KiB
parts:
- the payload is 1 MiB: `x`, an embedded `ESC[201~`, then a token, `1` and CR
  every 512 bytes with `0xff` filler between, then the terminal's end marker;
- `a_mebibyte_…_types_nothing_and_sends_nothing` (:3446): no request; the
  composer holds `x` plus the test's own `z`; only `xz` is sent;
- `a_mebibyte_…_answers_no_approval` (:3489): with an `ESC[200~` three
  quarters through, the notes file is unchanged; the test's own `1` then
  answers and the edit runs.

Bounds:
- time: `WAIT` per write and per wait, and a fixed payload;
- memory: 1 MiB in, and the filler keeps a regression from painting the
  screen.

Results: 3 of 3 green at load average 34. Mutant 12 (fx's boundary) shows they
detect it (§7).

## 7. Mutants (`mutants.py`, `mutants.txt`)

Each mutant ran on a throwaway `git clone --no-local` at `45dffff4`, never on
the worktree, with `GIT_DIR`, `GIT_INDEX_FILE` and `GIT_WORK_TREE` unset. Each
made one textual change, then ran `cargo test --locked --lib tui::` and the
four pty paste tests (the two mebibyte cases among them), each in its own
process group with a 900 s limit; the change was undone after. They ran under
the suite lock. **12 of 12 killed**, and every failing test is named in
`mutants.txt`.

| # | the rule reverted | killed by |
|---|---|---|
| 1 | round 1: a start marker in the quarantine is dropped with the rest | 11 |
| 2 | the quarantine ends with a marker half read | 3 (the split-marker cases) |
| 3 | the quarantine ends with a second paste still being dropped | 3 (the `behind` cases) |
| 4 | behind the end marker the markers are not followed | 5 |
| 5 | a paste being dropped is never closed (only start markers followed) | 1 |
| 6 | what the quarantine dropped before a start marker is never said | 1 (the owed-warning case) |
| 7 | **every empty check restarts the quiet** (opus SHOULD 2) | 5, the new unit test `a_quarantine_ends_at_the_first_empty_check_at_or_past_the_quiet` among them |
| 8 | input that keeps arriving is never said again | 2 |
| 9 | it is said again at every check | 4 |
| 10 | the repeat is said in fx's words | 1 (the shell repeat case) |
| 11 | a paste begun in the quarantine is not a paste: its bytes are decoded | 8 |
| 12 | **fx's boundary: no quarantine at all** | 20, **both mebibyte pty cases among them** |

**Mutant 12 and the pty cases** (`mutant12-repro.txt`):
- I applied it by hand to the clone and ran the four pty paste tests alone.
- Both mebibyte cases fail on their own assertions: "the clipboard behind
  the end marker approved the edit", and the composer case's body newlines
  sent prompts the fake gateway had no reply for.
- The two same-delivery cases pass, as they should.
- `mutants.txt` also lists those two same-delivery cases among mutant 12's
  killers; that run was at load average about 45, so those two entries are
  load, not detection.

## 8. Sweep (`sweep-head.md`, `sweep-compare.txt`)

I swept the same 41 input scenarios as round 1, on `45dffff4`'s build, under
the suite lock. The comparison is against round 1's base sweep, `a832f4d7`,
with the same ledger (sha256 `530ac71a…`) and the same scenario files.

- **base 200 / 253, head 200 / 253, drops 0** (`sweep-head.md`). That head
  sweep also had identical 10 and normalized-identical 190.
- `sweep-compare.txt`: **no pair changed class, cause or uncaused count.**
- `green.txt`, `cells.py` over this sweep's own paste captures (row 1 is
  `v0.1.0-45dffff`):
  - every `p5-input-paste`, `p5-input-paste-behind`, `p5-input-paste-field`
    and `paste-suffix` pair differs in **0 rows**;
  - **`p5-input-paste-amend` differs in 6/6/6/6/6/5 rows**. Those are
    PASTE-AFTER-END's grant-scope rows (`../../paste-after-end/amend-rows.txt`),
    and the base has them too.

## 9. Gate (`gates.txt`)

Pending the fault-injection re-run; filled in the last commit from `gates.txt`.

## parity.md

The `full-screen TUI` paste paragraph:
- the repeat, with its sentence;
- a second paste is still a paste, its grammar, and its liveness;
- the dropped-paste path behind an end marker, measured in `measure.txt`;
- the bound pinned between two measured figures;
- the window, its cost, and the harnesses' paced-input contract.
