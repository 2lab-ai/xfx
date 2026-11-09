---
title: 0.0.16 delta -- PASTE-GAP round 3 receipt (an honest notice per held state; Escape; the read cap)
type: receipt
tags: [xfx, tui, phase5, input, paste, security]
---

# PASTE-GAP round 3

Round 2 closed at `e059456b`; both engines found no MUST. The dispatcher sent
round 3 because the suppression notice could say something false. Every
figure below rests on a file in this directory or the one named.

| commit | what |
|---|---|
| `5c29f363` | RED, tests only, on `e059456b`'s source: five cases (`red-unit.txt`) |
| `8b4766f9` | GREEN: `Hold` and its sentences, the lone-Escape release, `Decoder::still` at the read cap, the pty barriers, wording. **The code head**: the mutants and the gate ran here (banner `v0.1.0-8b4766f`) |
| the commits after | this receipt (and one sentence of round 2's); receipt files only |

Round 3 adds 148 non-test source lines; the unit's total is 423.

## 1. A notice for each held state (both engines)

The repeat now says what holds the quarantine at the moment it is said
(`Hold`, `src/tui/input.rs:475`; `hold()`, :598):

| what holds it | sentence (`src/tui/shell.rs`) |
|---|---|
| the quiet | `Input is still being ignored until it stops for a moment.` (`PASTE_STILL`) |
| an open paste begun behind the end marker (`dropping_paste`) | `Input is still being ignored until the paste's end arrives.` (`PASTE_UNTIL_END`, :558) |
| a marker read past its first byte (`following > 1`) | `Input is still being ignored until one more key and a moment's pause.` (`PASTE_UNTIL_KEY`, :562) |

The first warning is still fx's past-tense sentence, which claims nothing
about the future. The tests:
- `the_repeat_while_a_dropped_paste_is_open_says_it_waits_for_the_paste_s_end`
  (`shell.rs:19064`):
  - a dropped paste opens in the first paste's delivery;
  - input keeps arriving for 1.2 s, then stops for 1, 5 and 10 s;
  - **the quarantine is still on** and a key typed after it is dropped;
  - every repeat is the open-paste sentence: four notices in all, including
    one for the stream's last fifth of a second, said a second after the
    previous one.
- `the_repeat_while_a_marker_is_half_read_says_it_waits_for_one_more_key`
  (:19110):
  - the repeat falls due while `ESC [` is held;
  - it says the marker sentence, and pauses of 1 and 5 s release nothing;
  - once a byte breaks the marker, the next repeat is the quiet's sentence,
    and the quiet then ends it.

## 2. Escape: option (a) (opus SHOULD 1)

`end_delivery` now treats a lone held ESC (`following == 1`) as `whole`
(`input.rs:684`). Once the input has been quiet for `PASTE_QUIET`:
- that ESC is dropped and the quarantine ends (:690), as the stage machine
  resolves a lone ESC after `ESC_TIMEOUT`;
- if this quarantine had said nothing, its first warning is owed at the next
  check.

`ESC [` and longer still hold.

Pinned:
- `a_lone_escape_in_the_quarantine_is_dropped_once_the_input_is_quiet`
  (:3639): released by the quiet, the owed warning, then `\r` is a key;
- `escape_pressed_and_pressed_again_in_the_quarantine_still_lets_the_quiet_end_it`
  (:3718): an Escape every tick for 200 ms, then a pause, releases with none
  of them a key; `ESC [` held through the same pause does not release;
- `a_start_marker_split_across_the_quiet_holds_the_quarantine` (split after
  `ESC[20`) still passes (`green-unit.txt`).

parity.md:237 and the module doc now say "a marker read past its first byte"
holds, and a lone Escape does not.

The residual class is unchanged: a split right after a lone ESC needs the
terminal to pause 50 ms or more inside one paste, which is the documented
residual.

## 3. The read cap (opus SHOULD 2)

At the 32-read cap, with input still readable, `burst` now calls
`Shell::still_at` (`event_loop.rs:676`), which calls `Decoder::still`
(`input.rs:613`). That says the owed warning (the first, or a repeat in its
hold's words) through the same `owed_warning` (:627) the empty check uses.
It does not touch the quiet, settle a paste, or end a quarantine.

Before a paste has settled (`behind`) there is nothing to say: the paste and
its warning land together at the check that settles it, as in fx.

Pinned:
- `a_stream_that_fills_every_read_after_a_paste_is_still_said_at_the_read_cap`
  (`event_loop.rs:16179`): a clean paste, then a turn of 32 × 128 + 1 bytes;
  the warning is said at the cap, and the composer and the work channel are
  unchanged.
- `still_says_what_the_quarantine_owes_and_settles_and_ends_nothing`
  (`input.rs:3672`):
  - nothing is said before the settle;
  - the first warning, then nothing within `PASTE_REPEAT`, then the repeat in
    the marker's words;
  - nothing ends 10 s later.

## 4. The pty tests' order (opus SHOULD 3)

- **Approval case:** after the hostile paste, Down (`tests/tui.rs:3516`) moves
  the question's marker, and the test waits for `❯ 2` before reading the notes
  file. That key is written after the whole paste, so once it is on the screen
  every byte of the paste has been read. A paste that had answered would have
  taken the question down first.
- **Composer case:** `request_count() == 0` now comes after the `┃ xz`
  barrier (:3469).
- `pty-runs.txt`: three raw runs at `8b4766f9`, both cases `ok` each time,
  with the load average in the file (16.6 to 18.2 at the start of each).

## 5. The 1 MiB comment (gpt6 SHOULD 2)

`hostile_paste`'s doc now says where its embedded start marker lands depends
on where the deliveries end (behind the first end marker, or inside the
quarantine), that an unpaced write forces neither, and that each branch is
pinned by its own unit case.

## 6. Nits

- **"3 of 3 at load 34"** had no raw log. Round 2's README sentence now says
  so (`../round2/README.md`, §6), and `pty-runs.txt` is the raw log for the
  round-3 versions.
- **The `owed` doc** no longer says "always said". It says what `owed` keeps:
  the first warning of a quarantine that ended before it said one.
- **The 250 ms pin** is not tied. The comment in
  `paste_quiet_stays_within_what_was_measured`, `tests/support/pty.rs`'s
  `PASTE_PAUSE` and `scripts/smoke-tui.sh` now say plainly that the three are
  spelled out separately and nothing checks they agree.

## 7. RED -> GREEN

- `red-unit.txt` (`5c29f363`, `e059456b`'s source): 5 of 5 fail:
  - both repeats say "until it stops for a moment";
  - a lone Escape, and Escape pressed again, outlast the quiet;
  - the cap says nothing.
- `green-unit.txt` (`8b4766f9`): those five, the two new pins and the cases
  they sit beside, 9 `ok`.

## 8. Mutants (`mutants.py`, `mutants.txt`)

Each mutant ran on a throwaway `git clone --no-local` at `8b4766f9`, never on
the worktree, with `GIT_DIR`, `GIT_INDEX_FILE` and `GIT_WORK_TREE` unset. Each
made one textual change, then ran `cargo test --locked --lib tui::` and the
four pty paste tests, each in its own process group with a 900 s limit; the
change was undone after. They ran under the suite lock. **12 of 12 killed**,
and every failing test is named in `mutants.txt`.

| # | the rule reverted | killed by |
|---|---|---|
| 1 | **item 2: a lone ESC is held like a longer marker** | 2 (the two Escape cases) |
| 2 | a lone ESC the quiet drops is never said | 1 |
| 3 | **item 3: the read cap says nothing** | 1 (the read-cap case) |
| 4 | the read cap settles and ends as an empty check does | 2 (`still`'s pin, and round 2's 32-read carry-over) |
| 5 | **item 1: an open dropped paste's repeat says "until it stops"** | 1 (the open-paste shell case) |
| 6 | **item 1: a half-read marker's repeat says "until it stops"** | 2 |
| 7 | round 1: a start marker in the quarantine is dropped | 16 |
| 8 | the quarantine ends with a marker read past its first byte | 5 (`a_start_marker_split_across_the_quiet_…` among them) |
| 9 | the quarantine ends with a second paste still being dropped | 4 |
| 10 | behind the end marker the markers are not followed | 6 |
| 11 | every empty check restarts the quiet | 5 (its unit case among them) |
| 12 | fx's boundary: no quarantine at all | 23; the pty cases among them are the two mebibyte cases, and only they |

## 9. The sweep: not run

The dispatcher's rule is to sweep only if the screen can change. No screen
the 41 input scenarios reach changes:
- the new sentences are said only when dropped input keeps arriving for a
  second, or at a read cap inside a quarantine;
- the lone-Escape release needs an Escape within about 58 ms of a paste;
- no scenario types straight after a paste (a `wait` always comes between,
  `../README.md` §8), and none streams after one.

## 10. Gate (`gates.txt`)

Every CI step of `ci.yml` ran in order at **`8b4766f9`**, under the suite
lock. **All 17 exit 0** (`gates.txt`).

| step | result |
|---|---|
| fmt, clippy, fault-injection clippy | exit 0 |
| `cargo test --locked --all-targets` | 14 binaries, **5481 passed**, 0 failed |
| with the fault injector | 15 binaries, **5519 passed**, 0 failed |
| no-stubs, no-secrets, gate-env, identity, preview contract and its mutants, preflight cost, usage clock | exit 0 |
| release and faulty builds, `smoke.sh` | exit 0 |
| `smoke-tui.sh` | 37 scenarios + the oracle, **1167 checks, 0 failures**; scenarios 7 and 21 (the paste ones) `ok` |

Round 3's tests and the unit's pty paste cases are all `ok` in the `test`
log.

## parity.md

The `full-screen TUI` paste paragraph:
- the repeat's three sentences, said at a check that finds nothing and at
  the read cap;
- "a marker read past its first byte" holds, and a lone Escape does not.
