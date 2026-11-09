---
title: 0.0.16 delta -- AT-FOLDERS round 3 (the bell with the frame; fx's Ctrl-O listing lifecycle)
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, input, file-picker]
---

# AT-FOLDERS round 3

Round 2's head `cde372ab` was reviewed by two engines: gpt6 found one MUST,
and opus approved with fixes, with one MUST and two SHOULDs. The
dispatcher's decisions: the bell is fx's frame's, and xfx copies fx's Ctrl-O
listing lifecycle exactly. fx's lines are quoted in
`../../at-height/fx-src.txt`, extended from line 1546. Lines 1-1544 are
byte-identical to round 2's (checked with `cmp`). `fx-src.txt:N` below
means that file.

| commit | what |
|---|---|
| `36ad6efd` | RED (`red-unit.txt`): 3 cases, and a test-only `/sound` seam (`Shell::sound_for_test`) |
| `bfa2f2eb` | the fix, the quotes, `docs/parity.md`, the round-2 and round-1 README corrections -- **the code head** |
| `971ca1a7` | `mutants.py` |
| `7e872bf9` | the ledger's 12 `shell.rs` refs re-pointed (`refs.py`) |
| `32ceaf92` | test only: the Ctrl-O case holds the index walk -- two mutants had survived on the walk's publication re-listing the folder by itself (§5) |
| `db8b88b5` | the ledger's refs after it (`refs.txt`: 16 checked, 0 wrong) -- **the gate's commit** |
| (receipt commit) | this folder |

## 1. The bell is held with the frame (opus r2 MUST)

**fx at the pin:**
- `flushRequestedFrame` runs `flushNotifications` after the frame is
  committed, under `.notification` (`app_render_runtime.zig:1059-1063`,
  `fx-src.txt:1546-1573`).
- That is `flushNotifications`' only caller in `src` (`main.zig:788`,
  `fx-src.txt:1574-1581`; the other hit is a test double).
- It drives the provider's flush, which delivers the ready notifications to
  the player (`notifications.zig:185-203`, `fx-src.txt:1704-1724`).
- The player's emit is `writeNotificationBell` (`:283-286`,
  `fx-src.txt:1725-1729`), and that is its only caller outside a test.

So fx's held frame holds its bell, and the bell follows the frame that is
committed. Round 2 said otherwise. That claim is corrected where it was made:
`../round2/README.md` §1 and its table, `../README.md` §3, `docs/parity.md`,
and `commit_or_hold`'s and `commit_bells`' docs.

**xfx, from `bfa2f2eb`.** `commit_tick` leaves the bells owed on a held tick.
The tick that releases the hold commits its frame, then `commit_bells` rings
them.

## 2. fx's Ctrl-O listing lifecycle (gpt6 r2 MUST)

**fx at the pin:**
- `collectFilePickerFacts` and the hold both harvest with
  `filePickerEligible` (`input_completion_runtime.zig:814`, `:821`;
  `fx-src.txt:1668-1674`, `:1209-1238`). It is false while the full
  transcript is up (`:783`).
- An ineligible harvest stops the task, drops a pending request, clears
  `directory_request` and sets `lookup_dirty` (`directory_completion_job.zig:
  140-151`, `fx-src.txt:1532-1544`).
- A stopped or superseded task's output is dropped (`:191-217`,
  `fx-src.txt:1675-1703`).
- Once the list is eligible, the dirty lookup is made again.

**xfx, from `bfa2f2eb`:**
- `Shell::settle_files` (each turn, and from the hold) does this while the
  full transcript is up: `Files::abandon_listing` stops the listing in flight
  and forgets it, so its receiver is dropped and a late answer is never
  taken. The list is marked dirty (`FilePicker::mark_lookup_dirty`).
- Once the transcript closes, the dirty list is refreshed: the folder is
  looked up again, which starts a fresh listing. The hold re-arms on it.
- Round 2's `!self.full.is_open()` in the hold's `in_flight` stays: the list
  still says `Loading` behind the transcript, and nothing is in flight.
- `docs/parity.md`'s divergence sentence (`cde372ab`) is gone. The row now
  says the listing is stopped and the folder listed again.

## 3. The two SHOULDs (opus r2)

- **The report's caller.** `loopCollectFacts` runs `WorkerAppRuntime.tick`
  (`main.zig:2983`, `fx-src.txt:1582-1599`). That runs `syncState`
  (`app_worker_runtime.zig:809-827`, `:657-665`; `fx-src.txt:1600-1631`),
  which calls `reportProgramStatus`, and that writes the report
  (`:758-791`, `fx-src.txt:1632-1667`). All of it is outside
  `flushRequestedFrame`.
- **A held tick that owes a title at its start:**
  `event_loop::a_held_tick_that_owes_a_title_writes_the_title_then_the_report`.
  The launch's title is owed, and a typed-ahead `@tree/` holds the first
  frame. The held tick publishes exactly `title …`, then `idle`.
  - Its last assertion found a round-2 defect: the band's first frame wrote
    the title again, because a band that has never painted invalidates
    itself, and that forgot the title it had told on its own.
  - `Band::build_frame` now keeps a told title across that invalidate, only
    while the band has never painted. On every other path the band's
    `shown_title` is unchanged.

## 4. RED → GREEN (`red-unit.txt`)

On round 2's code (`36ad6efd`: `cde372ab`'s source plus the test-only
seam), 3 of 3 fail by assertion:
- `event_loop::a_frame_held_for_a_folder_holds_its_bell_but_no_report_or_title`
  (round 2's held-turn case, reshaped):
  - The bell is now owed by the pacer's drain (`/sound` on; `settle_band`
    sees nothing left to present). `restore_bells` is gone from it.
  - Round 2 rang `bell x1` inside the hold.
  - The case now requires no frame and no bell inside the hold;
    `["frame", "bell x1"]` on the tick that releases it; and the unheld run
    with the same reports and title.
- `event_loop::a_held_tick_that_owes_a_title_writes_the_title_then_the_report`:
  the release frame repeated `title xfx v0.1.0 | work`.
- `shell::a_listing_out_when_ctrl_o_opens_is_dropped_and_the_folder_listed_again`:
  - Over `@src/`, Ctrl-O opens while the first listing is out, and closes
    with the query as it was: the **same folder** throughout.
  - The first listing then answers late, with `late.txt`.
  - Round 2 took it (`["src", "src/late.txt"]`). At `bfa2f2eb` the rows are
    the fresh listing's: `src`, `src/deep`, `src/lib.rs`, `src/main.rs`.

All pass at `bfa2f2eb`, and at `32ceaf92` with the Ctrl-O case tightened.
So do round 2's cases, the Ctrl-O no-hold case among them.

## 5. Mutants (`mutants.txt`)

Every run is in a `git clone --no-local`, never the worktree, under the
suite lock, with 900 s per mutant.

- **Round 3's 6** (`mutants.py`):
  - the bells rung on a held tick (item 1);
  - the told title forgotten by the first frame;
  - for item 2: the listing kept under Ctrl-O, the folder never listed
    again, the list never marked, and the abandoned answer kept.
- **Run 1, at `971ca1a7`: 4 killed, 2 survived.**
  - The survivors were `ctrl-o-never-relists` and `ctrl-o-not-marked`.
  - The Ctrl-O case let the index walk's publication refresh the list, and
    that listed the folder again on its own.
  - `32ceaf92` holds the walk in the case, so only the dirty lookup can list
    the folder again.
- **Run 2, at `db8b88b5`: 6 run, 6 killed.**
- **Round 2's 9 and round 1's 31, at `971ca1a7`:** 9 of 9, and 28 of 28.
  The 3 round-1 mutants whose text round 2 moved are BROKEN as written;
  they run re-texted in round 2's set.
- **So at this head: 43 run, 43 killed.**

## 6. Pairs and sweep: not re-run

No cell at any snap can change:
- **The bells** move only on a held tick, and only by when they ring. A bell
  is not a cell.
- **The kept title** applies only to a band that has never painted and has
  written its title on its own. That happens only when the launch's first
  frame is held, and no pair scenario types an `@` word before its first
  frame. A title is not a cell either.
- **The Ctrl-O lifecycle** changes only what happens to a listing in flight
  while the full transcript is up. No scenario in the sets types an `@` word
  and presses Ctrl-O (`../round2/README.md` §5).

So round 1's 23 pairs and sweep still stand for this code.

## 7. The gate (`gate.txt`)

Every CI step (`../gates.sh`) ran in a `--no-local` clone at `db8b88b5`, in
one hold of the suite lock, with `TMPDIR=<scratch>/atfg3`. **20 steps, 0
failed**, and the tree was clean before and after.
- fmt, and both clippies.
- `cargo test`: 5490 passed, 0 failed. With fault injection: 5527 passed, 0
  failed.
- The checks and their self-tests, and the fxcap self-tests (68 capture
  tests).
- Both release builds, and smoke.
- smoke-tui: 37 scenarios and the oracle, 1168 checks, 0 failures.

A first gate at `bfa2f2eb` was stopped after step 3 when the mutant run
showed the Ctrl-O case too weak. Its output is not kept.
