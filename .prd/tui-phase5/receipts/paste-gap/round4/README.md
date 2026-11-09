---
title: 0.0.16 delta -- PASTE-GAP round 4 receipt (every notice true; option (a)'s position named)
type: receipt
tags: [xfx, tui, phase5, input, paste, security]
---

# PASTE-GAP round 4

Round 3 closed at `1842e308`. gpt6 found one MUST, which the dispatcher
confirmed in the code. The dispatcher made opus SHOULD 1 a must as well,
because it puts a false message on the screen. Every figure below rests on a
file in this directory or the one named.

| commit | what |
|---|---|
| `b7a227b9` | RED, tests only, on `1842e308`'s source: four cases (`red-unit.txt`) |
| `0cc8084e` | GREEN: the two framing sentences, the hold-change owe, the named position and its pin, the stale wording. **The code head**: the mutants and the gate ran here (banner `v0.1.0-0cc8084`) |
| the commits after | this receipt; receipt files only |

Round 4 adds 60 non-test source lines; the unit's total is 458.

## 1. The marker sentence (gpt6 MUST)

From a held `ESC [`, a byte that continues the marker (`2`, then `0`) keeps
the hold, so "until one more key and a moment's pause" was false. The two
framing sentences now say only what must come before a pause can end
anything, which stays true however many keys that takes:

| hold | sentence (`src/tui/shell.rs`) |
|---|---|
| the quiet | `Input is still being ignored until it stops for a moment.` (unchanged) |
| an open dropped paste | `Input is still being ignored; a pause won't end it until the paste's end arrives.` (`PASTE_UNTIL_END`, :561) |
| a marker read past its first byte | `Input is still being ignored; a pause won't end it until the escape sequence it began is finished or broken.` (`PASTE_UNTIL_KEY`, :567) |

Also reworded:
- `Hold::Marker`'s doc (`src/tui/input.rs:496`) and `Hold::Paste`'s;
- the module doc (:168-177), `burst`'s doc and parity.md:237;
- the stale "no marker part-matched" (`end_delivery`'s doc) and "a start
  marker half read" (a test comment).

The regression the dispatcher asked for, in two places:
- `a_byte_that_continues_a_held_marker_keeps_it_held_through_a_pause`
  (`input.rs:3701`): `ESC [ 2`, then quiets of `PASTE_QUIET` and of 10 s,
  release nothing; then `x` breaks it, a pause follows, and the quarantine
  ends with none of it a key.
- `a_key_that_continues_a_held_marker_keeps_the_marker_sentence_true`
  (`shell.rs:19166`): the repeat falls due while `ESC [` is held, then `2`
  and `0` follow and input stops for 1, 5 and 10 s. Nothing is released, and
  the marker sentence is said once and stays. Breaking it and a pause end it.

## 2. A hold changed by held bytes alone (opus SHOULD 1)

The quarantine now records what its last repeat said holds it (`said_hold`,
`input.rs:514`). `owed_warning` owes a repeat when bytes were dropped, and
now also when the hold has changed since that sentence (`changed`, :656). It
waits for the same `PASTE_REPEAT` after the last sentence, and records the
new hold once it is said (:666).

The first warning sets no hold. It is fx's past-tense sentence and promises
nothing about the future, so nothing on screen goes false after it.

`a_hold_changed_by_held_bytes_alone_is_said_once_in_its_own_words`
(`shell.rs:19223`):
- input streams until the quiet's repeat is said;
- then `ESC [` arrives, and nothing after it;
- checks follow every 200 ms for 10 s;
- the marker sentence is said **once**, the quarantine still holds, and
  nothing more is said.

## 3. The position option (a) gave up, named and pinned

parity.md:237 and the module doc (`input.rs:179`) now state it:
- a queue-empty gap of `PASTE_QUIET` right after an ESC ends the quarantine;
- if the gap falls inside the terminal's own end marker, its tail `[201~` is
  typed, and its `2` answers **Always** at an approval with no draft open
  (`approval.rs:646-648`, `CHOICES[1]` at :113-117);
- a second paste's start marker split there is read as keys, its newline a
  Submit;
- it is the same class as the 50 ms residual, which the clipboard cannot
  cause.

The test is
`what_option_a_gives_up_a_lone_escape_then_a_gap_then_a_marker_s_tail_is_keys`
(`input.rs:3731`). A lone ESC is released by the quiet, then:
- `[201~` decodes as exactly `[`, `2`, `0`, `1`, `~`;
- `[200~ab\r` decodes as `[`, `2`, `0`, `0`, `~`, `a`, `b`, Submit.

## 4. `PASTE_UNTIL_END` and `PASTE_UNTIL_KEY` consistent (item 4)

Both now take the same form, "a pause won't end it until …", which names
only the prerequisite and is true however that state then ends. The pause
that follows is the quiet's to say: `PASTE_STILL` is said again if the quiet
is what holds the quarantine next.

The four sentence tests now use a 160-column screen, so the longer sentences
fit on one row of `said()`'s unwrapped comparison. At 80 columns the
document wraps them with a hanging indent, as it wraps every long notice.

## 5. RED -> GREEN

- `red-unit.txt` (`b7a227b9`, on `1842e308`'s source): 4 of 4 fail:
  - the two framing sentences have round 3's wording;
  - the continuing-key case shows "one more key";
  - the hold-change case shows only the stale quiet sentence.
- `green-unit.txt` (`0cc8084e`): those four, both new decoder pins, and the
  cases they sit beside, 10 `ok`.

## 6. Mutants (`mutants.py`, `mutants.txt`)

Each mutant ran on a throwaway `git clone --no-local` at `0cc8084e`, never on
the worktree, with `GIT_DIR`, `GIT_INDEX_FILE` and `GIT_WORK_TREE` unset. Each
made one textual change, then ran `cargo test --locked --lib tui::` and the
four pty paste tests, each in its own process group with a 900 s limit; the
change was undone after. They ran under the suite lock in two runs.
**10 of 10 killed**, and every failing test is named in `mutants.txt`.

| # | the rule reverted | killed by |
|---|---|---|
| 1 | **item 2: a hold changed by held bytes alone is never owed** | 1 (the hold-change case) |
| 2 | the hold behind a repeat is never recorded, so no change is ever seen | 1 (the same) |
| 9 | **item 2: the hold is recorded at the first repeat only, so a change is owed again and again** | 1 (the same; it asserts the sentence once in 10 s) |
| 3 | **item 1: the marker sentence promises one more key** | 3 (every case that says it) |
| 4 | **item 4: the dropped-paste sentence is round 3's** | 1 |
| 5 | `ESC [` (two held bytes) is released by the quiet like a lone ESC | 4 |
| 10 | **item 1: a byte that continues a held marker (`ESC [ 2`) lets the quiet end it** | 5, `a_byte_that_continues_a_held_marker_keeps_it_held_through_a_pause` among them |
| 6 | round 2: a lone ESC is held like a longer marker | 3, the option (a) residual pin among them |
| 7 | the read cap says nothing | 1 |
| 8 | fx's boundary: no quarantine at all | 27; the pty cases among them are the two mebibyte cases |

**Two labels were corrected after run 1, and two mutants added.** Mutant 2
was first labelled "a hold change is owed at every check after it", but
deleting `said_hold = again` leaves `said_hold` at `None`, so a change is never
seen: it tests "never owed", as mutant 1 does. Mutant 9 is the "owed
once" half. Mutant 5 was first labelled as the continuing byte, but
`following <= 2` releases `ESC [`, two bytes, and `ESC [ 2` is three. Mutant
10 is the continuing byte. Neither mutant's text changed, only its label.

## 7. The sweep: not run

No cell any of the 41 input scenarios reaches can change:
- the sentences changed only in the two framing states, which need a second
  paste behind the first or `ESC [` inside the quarantine;
- the hold-change owe needs a repeat, which needs input streaming for a
  second after a paste;
- no scenario types straight after a paste (a `wait` always comes between,
  `../README.md` §8), and none streams after one.

## 8. Gate (`gates.txt`)

Every CI step of `ci.yml` ran in order at **`0cc8084e`**, under the suite
lock. **All 17 exit 0** (`gates.txt`).

| step | result |
|---|---|
| fmt, clippy, fault-injection clippy | exit 0 |
| `cargo test --locked --all-targets` | 14 binaries, **5485 passed**, 0 failed |
| with the fault injector | 15 binaries, **5523 passed**, 0 failed |
| no-stubs, no-secrets, gate-env, identity, preview contract and its mutants, preflight cost, usage clock | exit 0 |
| release and faulty builds, `smoke.sh` | exit 0 |
| `smoke-tui.sh` | 37 scenarios + the oracle, **1167 checks, 0 failures**; scenarios 7 and 21 (the paste ones) `ok` |

Round 4's tests and the unit's pty paste cases are all `ok` in the `test`
log.

## parity.md

The `full-screen TUI` paste paragraph:
- the two framing sentences, in full;
- a hold change is said;
- a continuing byte keeps a marker held;
- **what option (a) gives up**, as in §3.
