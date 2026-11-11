---
title: 0.0.16 delta -- CTRLO-AT-KEYS (the @ list's keys under Ctrl-O)
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, input, file-picker, full-transcript]
---

# CTRLO-AT-KEYS

Base: `5a83bc4a` (AT-LAUNCH-WALK, closed, not yet on `main`). Under Ctrl-O
-- the full transcript -- xfx's `@` list spent `C-j` and `C-k`, where fx's
composer takes them. This unit copies where fx v0.0.16 (`481104fd`,
executable sha256 `8e6cda2e…`) routes every key the list could see there,
and puts Tab's eligibility check first, as fx's is (AT-HEIGHT r3 opus
SHOULD 2). AT-HEIGHT's trusted gate, AT-SCOPE-INTERIM's epoch and
AT-LAUNCH-WALK's walk are not touched: the change is two early returns in
`Shell::offer_to_files`.

| commit | what |
|---|---|
| `8509f872` | RED: one case per key, the Tab-over-`Files unavailable` case, and the scenario `p5-ctrlo-at-keys` (`red-unit.txt`) |
| `732e1962` | the fix -- **the code head** |
| `4bf1eebf` | `docs/parity.md`, the `@` file picker row; AT-HEIGHT's and AT-FOLDERS' receipts point here (§7) |
| `1e41a773` | the ledger's 12 `shell.rs` refs re-pointed (`refs.txt`) -- **the head's build** for the captures, the sweep and the mutants' clone |
| `58725489` | this folder's runners, fx quotes, captures, unit runs, mutants and refs -- **the gate's commit** |
| (receipt) | this README, `gate.txt`, `fxcites.txt`, the sweep's files (`sweep-*.md`, `sweep-compare.txt`, `compare.py`'s last listing); `fx_src.sh`'s ranges for §9 appended (`fx-src.txt` regenerated, its first 555 lines unchanged) and `fxcites.py`'s fourth input, the scenario |

Every fx line cited here is printed in `fx-src.txt` (`fx_src.sh`, at
`481104fd`); "fx-src.txt:N" is its line. `fxcites.py` (AT-LAUNCH-WALK's)
checks every fx citation in this README, in the parity row's changed text,
in the lines this unit added under `src/` and in the scenario's header
against `fx_src.sh`'s ranges
(`fxcites.txt`).

## 1. fx's rule

How a key reaches anything in fx:

- A plain byte carries the composer shortcut its control code names
  (`appendRaw`, `terminal_action_decoder.zig:207-212`, fx-src.txt:3-8):
  `C-k` (11) is `delete_to_line_end`, `C-j` (10) `insert_newline`
  (`shortcuts.zig:19`, `:26`; fx-src.txt:19, 26); Tab (9) and Enter (13)
  carry none.
- A raw byte goes to the full transcript first (`app_input_runtime.zig:872`,
  fx-src.txt:81), which binds only 3 and 12 (`keyForByte`,
  `input_full_transcript_runtime.zig:147-154`); then to a modal
  (`app_input_runtime.zig:873`), then to the picker control bytes
  `C-j`/`C-k` (`:879`), then to the composer (`:881-893`).
- `routePickerControlByte` returns before any picker's move while the
  transcript is up: `composerPickerSurfaceVisible` is false then
  (`app_input_runtime.zig:1093-1116`, the transcript at `:1113`;
  fx-src.txt:144-167).
- A decoded key (an arrow, a page key, Escape) goes to the transcript's map
  first: Escape at `app_input_runtime.zig:915-919`, every other key it binds
  at `:930` (fx-src.txt:125-140). The map (`keyForAction`,
  `input_full_transcript_runtime.zig:156-178`): Up and Down scroll it
  (`:161-162`), the page keys page it (`:165-166`), Escape closes it
  (`:163`; `closeScreen`, `:129-145`), and `routeKey` does each
  (`:181-197`) (fx-src.txt:334-402).
- After every key fx reconciles the list (`app_input_runtime.zig:597`,
  fx-src.txt:43), which distrusts it while it is not eligible
  (`input_completion_runtime.zig:798-799`), and it is not eligible under the
  full transcript (`filePickerEligible`, `:775-786`, `:783`).

So, under Ctrl-O with the `@` list open:

| key | fx | the list |
|---|---|---|
| Tab | the composer's Tab: with a file query, `autocompleteFilePickerSelection` (`app_input_runtime.zig:1317-1321`), which returns at the eligibility check (`input_completion_runtime.zig:949`) before it resolves a row (`:950`) or retries (`:953`); a frame is asked for (`app_input_runtime.zig:1321`) | kept; nothing chosen, nothing read again |
| Enter | the composer's Enter: `submitFilePickerOnEnter` answers nothing unless the list owns the surface (`input_completion_runtime.zig:1034-1037`, `:1054-1056`), and the line is sent (`app_input_runtime.zig:1440`) | closes: the composer is empty |
| C-j | the composer's `insert_newline` (`app_input_runtime.zig:1442-1445`, `:513-521`) | closes: the caret has left the `@` word |
| C-k | the composer's `delete_to_line_end`, a kill (`app_input_runtime.zig:479-488`) | kept: its word is before the caret |
| Up, Down, PgUp, PgDn | the transcript's: it scrolls (`input_full_transcript_runtime.zig:161-166`, `:188-195`) | kept, the bar where it was |
| Escape | the transcript's: it closes, and the key goes no further (`input_full_transcript_runtime.zig:163`; `app_input_runtime.zig:915-919`) | kept, on the composer's plane again |
| a letter | the composer's: typed (`app_input_runtime.zig:1447-1478`) | a new query, prepared only once the list is eligible (`prepareFilePicker`, `input_completion_runtime.zig:853`) |

**The eligibility check and the re-read.** `autocompleteFilePickerSelection`
reconciles (`input_completion_runtime.zig:948`), returns `.inactive` while
the list is not eligible (`:949`), and only then resolves the row (`:950`)
and, with nothing to choose on a list that could not read, retries
(`:953`; fx-src.txt:507-512); `retry` asks for the index's refresh
(`file_completion_state.zig:230-233`, fx-src.txt:552-555). The same order in
`prepareFilePicker`: the eligibility return (`input_completion_runtime.zig:853`)
before the index refresh (`:854-866`).

**Not under the transcript.** A list not eligible for another reason keeps
`C-j` and `C-k`: `composerPickerSurfaceVisible` is true, so
`routeNonSlashPickerMove` runs -- a catalog menu's move first and, with
none, the file query takes the key and moves nothing
(`input_completion_runtime.zig:395-410`; `:407-409`, fx-src.txt:422-424).

**fx's own screens** (`fx/`, the scenario `p5-ctrlo-at-keys` at 40x120 in
`p5-input-at-height-ws`): `tab`, `down` and `esc` show `@f-` with the list
and its bar on `items/f-01.txt`; `letter` shows `@f-z` and `no matching
files`; `ck` shows `@f-` with the list, ` tail` gone; `cj` shows `@f-` over
an empty second row, and no list. AT-HEIGHT's `ctrlo-probe/` holds the
Enter pair (`enter-rows`, `enter-nomatch`).

## 2. The copy

`Shell::offer_to_files` (`src/tui/shell.rs`), after the distrust block,
which already distrusts the list and asks for a frame while it is not
eligible:

- **`C-j`/`C-k` under the transcript return `false`**, so the key goes on to
  the composer (`act`: `ControlJ` is `type_character('\n')`, `ControlK` the
  editor's kill to the line's end), as fx's routing hands them on. Keyed on
  `self.full.is_open()`, fx's `composerPickerSurfaceVisible`, not on "not
  eligible": a question, an approval or a rule change waiting takes every
  key before the list in xfx (`Shell::consume`), as fx's modal does
  (`app_input_runtime.zig:873`), so the transcript is the one state of the
  three that reaches the list.
- **Tab returns at once while the list is not eligible**, before
  `files.chosen()` and the retry, as fx's check
  (`input_completion_runtime.zig:949`) comes before `:950` and `:953`. The frame fx asks for is the distrust block's. The retry arm no
  longer reads `eligible` itself.
- The other keys did what fx's do already at the base: the transcript's map
  (`src/tui/full_transcript.rs`, `FullTranscript::key`) takes Up, Down, the
  page keys and Escape before the list sees them; Enter was AT-HEIGHT's
  (round 3); a letter is `Input::Text`, which `offer_to_files` never takes.

**What the base had of the Tab rule.** AT-HEIGHT r3 opus SHOULD 2 (Tab under
Ctrl-O over an unreadable list re-reads) was written at AT-HEIGHT's head.
AT-FOLDERS' round 4 (`6cc6b1a5`) then gated the retry on `eligible`
(`../at-folders/round4/README.md`, its lines 39 and 66) and pinned it for a
folder listing (`tab_under_the_full_transcript_asks_for_no_read`). So at the
base Tab already read nothing under Ctrl-O. This unit moves the check ahead
of the row, as fx's is, and pins it for an indexed list too (§3). The two
orders give the same result in every state that reaches the arm -- under
ineligibility `chosen()` is `None`, the distrust block having run -- which
is why the mutant that puts the base's order back survives (§5).

## 3. RED (`red-unit.txt`) and GREEN (`green-unit.txt`)

At `8509f872` (the base's code with the cases), the nine run alone: **2
failed, 7 passed**. At `1e41a773`: 9 passed.

| case | base | head (fx) |
|---|---|---|
| `c_j_under_the_full_transcript_is_a_new_line_and_closes_the_list` | **RED**: `@src`, the list up | `@src\n`, no list, none after the return, nothing sent |
| `c_k_under_the_full_transcript_kills_to_the_lines_end_and_keeps_the_list` | **RED**: `@src tail` | `@src`, the list for `src` and its bar kept; C-y yanks ` tail` back |
| `tab_under_the_full_transcript_chooses_nothing_and_keeps_the_list` | pin | `@src`, the bar where it was |
| `the_arrows_and_page_keys_under_the_full_transcript_are_its_own` | pin | Down, Up, PgDn, PgUp: the transcript stays up, the bar where it was |
| `escape_under_the_full_transcript_closes_it_and_keeps_the_list` | pin | the transcript closed, `@src` and its bar kept |
| `a_letter_under_the_full_transcript_is_typed_and_looked_up_once_it_closes` | pin | `@srcz`, no lookup under the transcript, `Empty` once it closes |
| `tab_under_the_full_transcript_over_files_unavailable_walks_nothing` | pin | `Files unavailable` before and after; no refresh asked; 2 walks where the composer's plane gives 3 |
| `enter_under_the_full_transcript_goes_on_to_the_composer` | pin (AT-HEIGHT's, one assertion added) | sent, and the list gone |
| `tab_under_the_full_transcript_asks_for_no_read` | pin (AT-FOLDERS') | a folder that could not be read is not listed again |

The pins pass on the base, as the base's captures do (§4). Of the brief's
"Tab, Enter, C-j and C-k now do nothing", only C-j and C-k were a gap at
this base: Tab doing nothing is fx's result, and Enter was fixed by
AT-HEIGHT.

The Tab-over-`Files unavailable` case is the first half of
AT-SCOPE-INTERIM's `tab_in_the_interim_retries_and_still_says_files_unavailable`
with Ctrl-O around the Tab: the launch walk A held, two changes, `@plan`
over A's adopted index of the old epoch, then B, the second change's walk,
which the case drives. Nothing is queued behind B, so a retry would ask for
a third walk once the list is eligible again. There are two, and once B is
whole the list shows `plan.md` from it. The mutants that move the check
after the re-read die on it (§5).

## 4. Pairs (`pairs.py`, `pairs.txt`; `fx/`, `xfx/`, `xfx-base/`)

`p5-ctrlo-at-keys` (committed in `8509f872`): `@f-` typed after the launch,
then one key at a time, each between a Ctrl-O and its return, the line
emptied (`C-e C-u`) before a step that needs its own draft, so no step's
result carries into the next. `ck` types ` tail`, Home and `@f-` first
(`tail-open`). Captured with `scripts/fxcap/capture.sh` at 40x120: fx
v0.0.16 (the sha guard printed unchanged), the base's debug build
(`v0.1.0-5a83bc4`) and the head's (`v0.1.0-1e41a77`, built after the commit
`1e41a773`), each banner in `pairs.txt`'s first lines.

| snap | base | head |
|---|---|---|
| `open`, `tab`, `down`, `esc`, `letter`, `tail-open` | identical | identical |
| `ck` | 1 row differs (` tail` kept) | identical |
| `cj` | 13 rows differ (the list up, no second row) | identical |

`cells.py` compares each cell's character and attributes, so the bar's row
is compared too: on `tab`, `down` and `esc` it is on `items/f-01.txt` on
all three sides.

## 5. Mutants (`mutants.py`, `mutants.txt`)

`mutants.py` on a `git clone --no-local` throwaway at `1e41a773`, one at a
time, `cargo test --locked --lib -- tui::shell tui::event_loop
tui::full_transcript tui::file_picker tui::file_index`, a short TMPDIR, the
whole run in one hold of the suite lock, 900 s each. None failed to build;
the clone was clean after (its last line, 0). **11 run, 9 killed.**
"Killed by" summarises; the log lists every killer.

| mutant | what it breaks | killed by |
|---|---|---|
| `cj-routing-reverted` | C-j under Ctrl-O spent again | the C-j case, and only it |
| `ck-routing-reverted` | C-k under Ctrl-O spent again | the C-k case, and only it |
| `cj-ck-handed-on-always` | the hand-on without the transcript term | two older cases where C-j/C-k walk the list on the composer's plane |
| `cj-ck-handed-on-whenever-ineligible` | keyed on "not eligible" | **SURVIVED**: equivalent (`equivalent-mutants.txt`) |
| `tab-check-after-the-reread` | an unreadable list read again before the check | the Tab-over-`Files unavailable` case, and AT-FOLDERS' folder case |
| `tab-check-gone` | no check, an ungated retry | the same two |
| `tab-base-order` | the base's order: the row first, the retry gated | **SURVIVED**: equivalent (§2, `equivalent-mutants.txt`) |
| `enter-taken-under-the-transcript` | AT-HEIGHT's Enter rule removed | the Enter case |
| `letter-spent-under-the-transcript` | a letter spent while a list is open | the letter case and two older under-the-transcript cases |
| `escape-not-the-transcripts` | Escape unbound in the transcript's map, so the list's Escape closes the list | the Escape case, and two older transcript cases |
| `down-not-the-transcripts` | Down unbound in the transcript's map | the transcript's own cases (`full_transcript::tests`, and the shell case that its keys move it) -- not the arrows case, which asserts the list: a Down the list spends leaves the bar where the transcript's scroll does |

The two survivors are equivalent in every state that reaches the line they
change: `equivalent-mutants.txt` gives the lines.

## 6. Sweep (`sweeps.sh`; `sweep-base.md`, `sweep-head.md`, `sweep-compare.txt`)

`sweep.sh` against fx v0.0.16 (sha256 `8e6cda2e...`, checked before and
after each run), the base's xfx (`v0.1.0-5a83bc4`) and the head's
(`v0.1.0-1e41a77`), each banner and sha256 in its sweep's header; 40x120,
`--jobs 2`, `--only 'at*' 'p5-input-at*' 'filepicker*' 'p5-ctrlo*'`: 18
scenarios, 84 pairs (no scenario is named `at*`; `p5-ctrlo*` is the seven
`p5-ctrlo-wider-*` and this unit's), both sweeps inside one hold of the
suite lock, the head's scenarios and ledger for both.

- **base 58/84 → head 60/84: 2 moved to passing, 0 dropped, 0 other
  verdict changes.** The two are `p5-ctrlo-at-keys ck` and `cj`
  (unresolved on the base, normalized-identical on the head), as the pairs
  of §4.
- The 24 pairs that do not pass on the head are the base's 26 less those
  two (`sweep-compare.txt`'s last lines): `p5-input-at`'s four `at-st`
  snaps and `p5-input-at-resize`'s five (the ledger's `at-ranking` and
  `at-resize-band-at-the-bottom`, AT-HEIGHT's README), and fifteen
  `p5-ctrlo-wider-*` snaps -- eleven approved divergences and four
  unresolved (`p5-ctrlo-wider-ri` x3, `p5-ctrlo-wider-inline-fam
  resized`), the same verdicts on both sides: the wide-character family's,
  not this unit's.

## 7. The parity row, the ledger, the earlier receipts

- `docs/parity.md`, the `@` file picker row (`4bf1eebf`): "where fx's
  composer takes Tab, C-j and C-k there (C-j a new line) is unit
  CTRLO-AT-KEYS's -- xfx spends them" becomes the copy -- the transcript's
  keys its own, the rest the composer's, each key's effect on the list, and
  Tab returning at the eligibility check. The receipt pointer names this
  folder. The row stays `partial` for its other narrower items.
- The ledger (`1e41a773`): the 12 `shell.rs` refs past the new lines,
  re-pointed by `refs.py` (AT-LAUNCH-WALK's, by text): `refs.txt`, **17
  checked, 0 wrong**.
- AT-HEIGHT's README (two places) and its `ctrlo-probe/README.md`, and
  AT-FOLDERS' §11, say the routing they named as this unit's has been
  copied, and point here.
- No help entry, registry count or `check-no-stubs` row names these keys.

## 8. Gate (`gates.sh`, `gate.txt`)

`gates.sh` (AT-LAUNCH-WALK's, unchanged but for its first comment line) in
a `--no-local` clone at `58725489`, the whole script in one hold of the
suite lock, TMPDIR a short scratch directory: **20 steps, 0 failed**, the
tree clean before and after.

- fmt, clippy and clippy with `fault-injection`;
- `cargo test`: **5545 passed, 0 failed**; with `fault-injection`: **5582
  passed, 0 failed** (summed over each run's 15 test binaries). That is
  AT-LAUNCH-WALK round 1's 5536 and 5573 (`../at-launch-walk/gate.txt`),
  its round 2's two cases (its §12), and this unit's seven new cases (the
  Enter case gained an assertion, not a case);
- the identity, stubs, secrets, preview-contract, preflight-cost and
  usage-clock checks, and their self-tests; the fxcap sweep and capture
  self-tests; both release builds;
- smoke, and smoke-tui: 37 scenarios and the oracle, 1168 checks, 0
  failures.

## 9. Look-ahead, and what is not this unit's

- **Next break on the path, looked for.** `C-j` and `C-k` handed on pass the
  later `offer_*` before `act`: `offer_to_picker`, `offer_to_provider_picker`
  and `offer_to_mcp_args` bind them (`src/tui/shell.rs`). With the `@` list
  open none of them is up -- the cases' text (`@src\n`, the kill) shows the
  key reached the composer. Read, not run: none of the three looks at the
  full transcript, so each, open without the `@` list under Ctrl-O, would
  take the key where fx's `routePickerControlByte` takes none
  (`app_input_runtime.zig:1093-1116`). The same class of gap for the other
  pickers, not this unit's list: a queue proposal for the dispatcher.
- **Shift-Up/Down under Ctrl-O, traced: no gap.** fx decodes a shifted
  arrow to a composer move with `extend_selection` (`escape_parser.zig:66-71`,
  `:99-107`), whose shortcut it carries (`shortcuts.zig:61`). The
  transcript's map binds no such move but the redraw
  (`input_full_transcript_runtime.zig:167-170`), so the key reaches the
  composer shortcut (`app_input_runtime.zig:997-1002`), the `.move` arm
  (`:310-330`) and `routeComposerVertical`, which offers it to
  `routeNonSlashPickerMove` first (`input_completion_runtime.zig:230`): with
  a file query the list takes it and, not eligible, moves nothing
  (`:407-410`). xfx: the transcript's map sees `SelectUp`/`SelectDown` and
  binds neither, `unshifted` folds them to the arrows (`Shell::consume`),
  and the list spends them (distrusted, `FilePicker::step`). The same
  result; no case pins it.
- **The screen under Ctrl-O** is the transcript's: a key that edits the
  composer there shows on the return, as fx's does, and
  `a_key_handed_on_under_the_full_transcript_writes_nothing` still passes
  in the gate.

## 10. Open questions

None that needs a user decision. One queue proposal for the dispatcher: the
slash, provider and `/mcp`-arguments pickers under Ctrl-O (§9).
