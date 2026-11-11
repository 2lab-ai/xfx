---
title: 0.0.16 delta -- AT-FOLDERS round 4 (one eligibility predicate; a listing that stops between entries)
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, input, file-picker]
---

# AT-FOLDERS round 4

Round 3's head `ec242827` was reviewed by two engines: opus approved with
fixes (one MUST, three SHOULDs) and gpt6 approved (two SHOULDs). The
dispatcher's decisions: one eligibility predicate with fx's terms at every
acquisition site; cooperative cancel in the listing; revert round 3's title
change if a production-sized band shows no RED; gpt6's idle tick. fx's lines
are quoted in `../../at-height/fx-src.txt`, extended from line 1730. Lines
1-1729 are byte-identical to round 3's (checked with `cmp`). `fx-src.txt:N`
below means that file.

| commit | what |
|---|---|
| `15ada93f` | RED (`red-unit.txt`): 4 cases (3 shell, 1 file_index), test-only seams in `file_index.rs`, the event-loop cases' bands sized and an idle tick added |
| `ccaa31b5` | RED: the skills-menu case (§1, the one more ineligible state) |
| `6cc6b1a5` | the fix, the quotes, `docs/parity.md`, round 3's README correction, `title-revert.txt` -- **the code head** |
| `35ab438f` | the ledger's 12 `shell.rs` refs re-pointed (`refs.txt`: 16 checked, 0 wrong) |
| `73da7050` | `mutants.py` -- **the gate's commit** |
| (receipt commit) | this folder |

## 1. One eligibility predicate (opus r3 MUST, SHOULD 3)

**fx at the pin:**
- `filePickerEligible` is false while a question prompt, an approval
  prompt or the auth picker is active, while the full transcript is up, or
  while a catalog menu owns the surface (`input_completion_runtime.zig:
  775-786`, `fx-src.txt:1753-1764`). The catalog terms are the help menu,
  the settings menu, the skills menu while visible, the model menu and the
  session picker (`catalogMenuOwnsSurface`, `:754-768`, `fx-src.txt:1732-1746`).
- `prepareFilePicker`, "the sole acquisition boundary", returns at once
  while it is false (`:848-853`, `fx-src.txt:1769-1774`), before the
  episode's index refresh (`:855-866`) and before the lookup.
- Tab returns before its retry while it is false (`:949`,
  `fx-src.txt:1793`); Enter on the list and the list owning the surface ask
  it too (`:1034-1056`, `fx-src.txt:1805-1827`).

**xfx:** `Shell::file_picker_eligible` (shell.rs:6158), term for term:

| fx term | xfx |
|---|---|
| `question_prompt.isActive()` | `self.ask.is_some()` (the model's question batch) |
| `approval_prompt.isActive()` | `self.asking()` (an approval, on either plane) |
| `auth.pickerView().active` | **none**: xfx has no auth picker, `/login` is deferred (`docs/parity.md:53`) |
| `shell.fullTranscriptActive()` | `self.full.is_open()` |
| help, settings menus | `help_menu`, `settings_menu` is `Some` |
| `skills.menuVisible()` | `skills_menu` is `Some` and `SkillsMenu::visible` |
| `model_cache.menu.active` | `model_menu` is `Some` |
| `session_picker.active` | `sessions` is `Some` |
| `runtime_profile.allows(.file_index)` | **none**: xfx always has the file index |

**Every site where xfx starts directory work or holds a frame for it.** The
shell's only production calls into `Files` (grep of `self.files.` above
`mod tests`) are `retry`, `lookup`, `abandon_listing`, `begin_episode` and
`poll`; `Files` spawns a thread only in `start_walk` (file_index.rs:1027,
reached from `begin_episode` and `retry`) and `lookup` (:1166).
`set_extra_roots` (shell.rs:2604, :8420) only stops a walk.
- `refresh_file_picker` (shell.rs:6877-6886): while ineligible it marks the
  list dirty and looks nothing up. The poll in `settle_files` (:8659) and
  Tab's second call reach the lookup only through it.
- Tab's retry (:6219): `None if eligible && … Unavailable`.
- The list opening in `reconcile_picker` (:9124-9140): while ineligible
  the list takes the query, begins no episode and looks nothing up; it
  owes both (`FilePicker::owe_episode`, `mark_lookup_dirty`). An owed
  episode carries across query changes.
- `settle_files` (:8643-8658): ineligible, a listing in flight is told to
  stop and the list marked dirty; eligible again, the owed episode is begun
  and the lookup runs, once.
- The frame hold (:8686): `in_flight` needs the list eligible.
- Also on the predicate, as fx's are: the distrust block (:6201,
  `reconcileFilePicker` `:798-799`) and Enter's arm (:6236, `:1055`).
  - **Corrected in round 5:** this round's report said Enter under a shown
    skills menu now goes on to the composer. It does not: the skills menu
    takes Enter before the list does, in xfx's loop and in fx's
    (`../round5/README.md` §4).

**Which ineligible states can hold the `@` list.**
- The full transcript: the keys it leaves unbound reach the composer
  (shell.rs:12153; Tab is unbound, full_transcript.rs:5138).
- A shown skills menu: a typed `$` and then a pasted `dem @src/` keep the
  mention menu on its `$` word and open the list
  (`an_at_word_under_a_shown_skills_menu_reads_nothing_until_it_goes`).
- A question or an approval: no. `ask` and `ask_question` close the list
  (shell.rs:5071, :9843), the dismissal keeps it closed for that word
  (:9091), and every key goes to the question while it is up (:12168).
  So no case for them, and a mutant dropping `ask`/`asking` would survive.
- The catalog, settings, `/model`'s menu, the session picker: no.
  `reconcile_picker` closes the list while each is up. Same consequence.

## 2. Cooperative cancel (opus r3 SHOULD 2, gpt6 r3 SHOULD 1)

**fx at the pin:** `Task.stop` sets `cancel` (`directory_completion_job.zig:
68-78`), checked after the lookup (`:96`, `:108`); `completeCancellable`
checks it between steps, "cooperative and cannot interrupt an in-flight
filesystem call" (`path_completion.zig:43-146`, `fx-src.txt:1881-1985`).

**xfx:** the listing thread passes its stop flag to `list_directory`
(file_index.rs:448), which asks it before `read_dir` and before each entry
(`list_directory_until`, :458-470). The docs that said a listing stops
(`abandon_listing`, `Drop`) are now true, and say it is cooperative.
- `a_listing_stops_between_entries_once_told_to`: 2000 files. Unstopped:
  all 2000. Stopped from the start: `Err`. Told after 100 entries: asked 102
  times, `Err`.
- `abandoning_a_listing_tells_it_to_stop`, and the Ctrl-O case now asserts
  the abandoned listing's flag is set.

## 3. The title (opus r3 SHOULD 1)

Both event-loop cases now size their bands as a session does (`mod.rs:479`).
With round 3's `Band::build_frame` hunk taken out, both pass
(`title-revert.txt`): the hunk has no RED. It is reverted, its mutant
`first-frame-retells-title` is dropped, and `../round3/README.md` §3 says no
round-2 defect was shown. `Band::title_landed`, which `write_title_alone`
uses, stays.

## 4. The idle tick (gpt6 r3 SHOULD 2)

`a_turn_inside_a_listing` runs one idle tick after the release; the case
requires no bell there, held or not. Mutant `bells-never-drained` (owed
bells kept after they ring) is killed by it and by
`owed_bells_go_out_after_the_frame_on_their_own_and_are_counted`.

## 5. RED → GREEN (`red-unit.txt`)

On round 3's code: 5 cases FAILED --
`no_listing_starts_while_the_list_is_not_eligible` (a listing started under
the transcript: 1, expected 0), `tab_under_the_full_transcript_asks_for_no_read`
(1, expected 0), `an_at_word_typed_under_the_full_transcript_reads_nothing_until_it_closes`
(2, expected 0), `an_at_word_under_a_shown_skills_menu_reads_nothing_until_it_goes`
(2, expected 0), `a_listing_stops_between_entries_once_told_to` (`Ok(2000)`,
expected `Err`). The other four pass there: they pin. After the RED commit
the skills case's last part (after the menu goes) pressed Escape the
decoder's way (`escape`); its RED assertion is unchanged. The Tab case
gained one assertion (no listing once the transcript closes, before Tab).

## 6. Mutants (`mutants.txt`)

One `--no-local` clone at `73da7050`, under the suite lock: **55 run, 55
killed**, 6 BROKEN, all accounted for.
- Round 4's 15, one per rule: each gate site (poll, Tab, open, the owed
  episode both ways, the relist, the hold, the abandon, Enter), the
  predicate's transcript and skills terms, the cancel unwired and not asked
  between entries, an abandon that does not stop, bells never drained.
  15 killed.
- Rounds 3, 2 and 1 re-run at this head: 4, 8 and 28 killed. BROKEN:
  `first-frame-retells-title` (dropped, §3); `ctrl-o-never-relists` and
  `ctrl-o-holds`, re-texted here as `eligible-never-relists` and
  `hold-ignores-eligibility`; round 1's three, re-texted in round 2.
- Not mutated: the predicate's question, approval, catalog, settings,
  `/model` and session-picker terms. The list cannot be open under them
  (§1), so no case can see them, and such a mutant would survive.

## 7. Pairs and sweep: not re-run

The predicate changes a cell only while the `@` list is up under an
ineligible surface. A grep of the pairs' and the sweep's scenario files
(`../sweep-head.md`) for `type`/`paste` lines with an `@` word finds ten
(filepicker, -keys, -narrow, p5-input-at, -at-dir, -at-folders, -at-gate,
-at-height, -at-resize, workspace-index). None presses C-o or types
`/help`, `/settings`, `/model`, `/resume` or `$`, and none drives a
question or an approval (filepicker's only gateway file has none). The cancel
changes only a listing already abandoned; the title revert changes only an
unsized band, which no session has. So round 1's pairs (`../pairs.txt`) and
sweep (`../sweep-compare.txt`) stand for this code.

## 8. The gate (`gate.txt`)

Every CI step (`../gates.sh`) ran in a `--no-local` clone at `73da7050`, in
one hold of the suite lock, with `TMPDIR=<scratch>/atfg4`. **20 steps, 0
failed**, and the tree was clean before and after.
- fmt, and both clippies.
- `cargo test`: 5496 passed, 0 failed. With fault injection: 5533 passed, 0
  failed.
- The checks and their self-tests; the fxcap self-tests (27 sweep tests, 68
  capture tests).
- Both release builds; smoke: 47 checks, 0 failures.
- smoke-tui: 37 scenarios and the oracle, 1168 checks, 0 failures.

A full `cargo test --lib` started under the lock before the fix commit was
stopped while it waited for a slot. Its output is not kept.

## 9. Refs and paths

`refs.txt`: 16 checked, 0 wrong. `git diff af2b6137..HEAD` adds no local
path (the brief's grep prints 0 at every commit of this round).
