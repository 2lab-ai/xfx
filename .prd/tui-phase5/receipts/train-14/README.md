---
title: 0.0.16 delta -- P5 train 14 receipt (the `@` stack, CTRLO-OTHER-PICKERS, PAGE-KEYS-SETTINGS-COLUMN and IMAGE-LOSS-COPY-FX into one branch)
type: receipt
tags: [xfx, phase5, fx-0.0.16, train]
---

# P5 train 14: nine closed units, four merges, one branch

Branch `train/p5-1111a`, from main at `dcfa5146` (PR #92, train 13, merged
2026-11-10 18:58Z), as the brief set it. Each unit was closed by its own
two-engine review and the dispatcher's gate; this train combines them. The
program goal (the user's words): «최신 fx랑 비교해서 ui쪽으로 모든 기능 구현하고
기본적으로 최신 fx ui를 그대로 구현해줘» (fx v0.0.16, pin `481104fd`).

Oracle: fx v0.0.16 (`481104fd`), executable sha256
`8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked
by every sweep and capture run before and after it ran, `FX_AUTO_UPGRADE=0`
(`sweep-runs.txt`, `anchor.txt`, `joins.txt`, `reruns.txt`).

Model: train 13's receipt (`../train-13/` and its `join-main/`) and the
dispatcher's three reviews of it. Its scripts are copied here, each saying in
its first lines what changed for this train; the new ones say so too. What
those reviews asked is carried: every figure rests on a file of this folder,
named where it is used; counts are checked against the lists they count; the
path scans run over this folder (`pathcheck.sh` before every commit,
`final-scan.sh`); "no code after the code head" is shown by `final-scan.txt`
part 4, which reads through the receipt's second-to-last commit and names
it, while the last commit stores `final-scan.txt` alone (`git show --stat`
of it); each scan names the revision it read.

**Where this train is not train 13.** Train 13's merges built as git merged
them. Three of these did not, and one built but broke a unit's behaviour:
each merge's semantic fixes are written down and redone (`semantic.py`), and
the blocks no rule fits are resolved by hand and redone too (`eventloop.py`,
`byhand.py`). The meeting that broke was not the one the brief forecast for
join check 3: it is CTRLO-OTHER-PICKERS x PAGE-KEYS-SETTINGS-COLUMN x
CTRLO-AT-KEYS in `Shell::consume_inputs` and `Shell::offer_to_files` (join
check 3, below).

## Units and merges

Each merge is a stack top and carries the units under it; every base is
git's merge base of the merge's two parents.

| # | merge | units it carries (branch @ tip) | base | conflicts |
|---|---|---|---|---|
| 1 | `d548c078` | the `@` stack, oldest first: AT-FOLDERS `b4cc300f`, AT-SCOPE-INTERIM `f4aa7dcc`, AT-LAUNCH-WALK `5a83bc4a`, CTRLO-AT-KEYS `cb7c9c18`, AT-RANK `4a5dc3c7`, AT-RANK-PERF `4130f4ac` (`feat/p5-at-rank-perf`; 146 commits) | `af2b6137` | `docs/parity.md` 1 row, the ledger 14 blocks, `src/tui/event_loop.rs` 4 blocks (forecast) |
| 2 | `ecad8518` | CTRLO-OTHER-PICKERS (`feat/p5-ctrlo-other-pickers` @ `e7e1bcf6`; 24 commits, on CTRLO-AT-KEYS) | `cb7c9c18` | `docs/parity.md` 9 rows in 3 blocks, the ledger 15, `event_loop.rs` 1 (forecast) |
| 3 | `7a1773d9` | PAGE-KEYS-SETTINGS-COLUMN (`fix/p5-page-keys-settings-column` @ `3ea16930`; 15 commits) | `3d3d56c8` | `docs/parity.md`, `capfile.py` 1, the ledger 13 (forecast); `src/tui/shell.rs` 5 (merge 2 brought them) |
| 4 | `fb6dfdb8` | IMAGE-LOSS-COPY-FX (`fix/p5-image-loss-copy-fx` @ `bde1ddfa`; 19 commits) | `714f6283` | the ledger 6 (forecast) |

The commit counts are `git rev-list --count <base>..<tip>` (the stack's
78 + 26 + 12 + 6 + 12 + 12). Not merged, as the brief has it: the approval
stack, QUESTION-CTRL-J, TEST-ENV-ISOLATION, LOCAL-PATHS-WRAP, the decoder,
profile and slash lines, CATALOG-WAIT-CLOCK (train 15).

After the merges, in order (`git log --first-parent dcfa5146..`):

| commit | what |
|---|---|
| `594fddc8` | **code**: one C-j/C-k check under the full transcript where merges 2 and 3 left two; two doc comments the merges made false (join check 3) |
| `8f622949` | the ledger: `at-plan-run-directory` deleted (join check 5) |
| `04e4491d` | 79 references re-pointed, the plan's 81 less the 2 left, numbers only but for two spans (join check 6; its message counts 78 and two more by hand, of which `shell.rs:2703` is one of the 78) |
| `09b13511` | nine lines of two units' receipts: a sweep's run directory name as a placeholder (the brief's path check, "Commits partway") |
| `16fff02d` | the ledger: `at-ranking-anchor` deleted (join check 5) |
| `6bc758a0` | one re-point of `04e4491d` corrected (`repointed.py` found it) |
| `725b3c90` | **tests**: PAGE-KEYS' case over `/model`'s menu under the full transcript takes fx's expectation, C-j adds nothing (join check 3; the first gate, at `6bc758a0`, failed it) |
| `3a2830fb` | the six ledger references `725b3c90`'s 19 lines moved, re-pointed |
| `b3dffdad` | two bare continuations on the line after their path, re-pointed (`refs_bare.py` read them MOVED; join check 6): **the code head the gate, the sweeps and every job of the assembly ran at** |
| the receipt's commits, rounds 1 and 2 | this folder only |
| `583cc13a` | **the join of main** `3570a182` (PR #94, WORKSPACE-INDEX-AT-PLAN): its harness, tests and receipt; one conflicting file, the ledger, three blocks by `blocks.py` ("The join of main") |
| `3e073a96` | the seven `capture.sh` references the join moved, re-pointed, numbers only |
| `d765f057` | this folder: PR #93's CI Sweep red read (`ci-08066ac9/`); the path scans read subfolders |
| `42935f45` | the ledger's at-plan comment rewritten (comments only, the same nine lines): **the code head since the join**; no `src` changed after `b3dffdad` (`join-main/checks.txt`) |
| the receipt's commits after it | this folder only (`final-scan.txt` part 4, through the second-to-last of them; the last stores `final-scan.txt` alone) |

The first gate ran at `6bc758a0` and failed one case (join check 3); the
head's first main sweep and join checks 1 and 2 read a build of
`6bc758a0` too. They were stopped part way, and the gate, the head's
sweeps, the joins and the network job queued at `3a2830fb` were stopped
before they started, when `b3dffdad` was found to be needed; each ran from
the start at `b3dffdad`. Nothing of the stopped runs is used here.

## The conflicts and their resolutions (`resolutions.txt`, `check-merges.txt`)

Every resolution is a script's, redone by `reproduce.sh` in a clone to
each merge commit's tree: **all four "no difference"**, each resolver's own
lines (one per block or row) printed under its merge (`resolutions.txt`).
Before each commit the staged tree passed `cargo check --all-targets` with
and without `fault-injection`, `cargo fmt --check` and both clippy runs
(`check-merge.sh`, no lock, as train 13); each tree checked is the
committed tree (`check-merges.txt` names it, `git rev-parse <c>^{tree}`
agrees for all seven). Merge 1's recorded run is the second on its staged
tree, cached (0 s); the first compiled it and was not kept.

- `blocks.py` is train 13's with its `numbers` rule read token by token, as
  train 13's join of main read it: each `:N` the side that moved it wins, and
  one both moved keeps ours' (main's, or the train's so far), which the
  re-point after the last merge moves to the merged code.
- `rowsmerge.py` is train 13's with one option, `--drop`, used once
  (merge 3, below).
- `eventloop.py` (merge 1) and `byhand.py` (merge 3) hold the blocks no rule
  fits, each found by the sha256 of its three sides and replaced by the
  text written in the script, so a different merge stops them.
- `semantic.py` holds each merge's fixes to text git merged clean, each a
  text replaced with its count of places, marked `build` (the tree does not
  compile without it) or `behaviour`.

**Merge 1** (`d548c078`, the `@` stack onto main):
- `docs/parity.md`, the `@` file picker row: main's 6 edits and the stack's
  32, none meeting (`rowsmerge.py`).
- The ledger, 14 blocks: 13 `numbers` (in two, a second token moved by ours
  alone: `full_transcript.rs:3061`, the bare `:1927`, ours kept), 1 `ours`:
  `preview-frame-before-the-turn-start`, main's SUBMIT-CARD-PREVIEW cause
  that replaced the base's, which the stack had only re-pointed (train 13's
  merge 3 met the same block from the other side).
- `src/tui/event_loop.rs`, 4 blocks, by hand (`eventloop.py`): main
  borrows the tick's `Reconciled` (`&reconciled`), writes a submitted card's
  frame after its preview's frame (`take_preview_shown`) and asks the
  `/model` column on the catalog's clock (`catalog_now`); the stack gives
  `commit_frame_led` a `held` flag, holds the tick's frame for a folder
  listing and asks the column nothing in a held tick. Each block keeps both:
  `commit_frame`'s call with both arguments; `commit_frame_led`'s doc, both
  sentences; `if !held { let catalog_now = ...; settle_column(catalog_now) }`
  (`Shell::catalog_now` only reads the clock); `commit_tick` with the
  stack's held call and comment, then main's card frame, passed the tick's
  `frame_held`. The card frame cannot run in a held tick -- `preview_shown`
  is set only by `Shell::band_landed`, called only from `landed_band`, and a
  held frame lands no band -- and the script's comment says so. A decision
  (`frame_held`, not `false`), for the veto below.
- Merged clean and read from both ends (`merge-ends.txt`): `ci.yml`
  (AT-RANK-PERF's `check-at-search-cost.sh` step), `Cargo.toml`,
  `Cargo.lock`, `scripts/test-gate-env.sh`, `src/tui/check.rs`,
  `frame.rs`, `mod.rs`, `shell.rs`.

**Merge 2** (`ecad8518`, CTRLO-OTHER-PICKERS):
- `docs/parity.md`, 3 blocks, 9 rows, no edits meeting.
- The ledger, 15 blocks: 14 `numbers`, 1 `ours` (the same cause).
- `src/tui/event_loop.rs`, 1 block: both appended tests at the test module's
  end; `blocks.py --append`, main's 567 lines then the unit's 44.
- **Build fix** (`semantic.py 2`): three of the unit's new tests in
  `src/tui/shell.rs` apply a `UiEvent::CatalogLoaded` whose `due` is an
  `Instant`, as at the unit's base; main's CATALOG-REFRESH (train 11,
  `2810f53c`) made it a `CatalogDue` and wrote every such test line as
  `CatalogDue::at(catalog_now() + ...)` (35 at main). The three take that
  form.

**Merge 3** (`7a1773d9`, PAGE-KEYS-SETTINGS-COLUMN):
- `src/tui/shell.rs`, 5 blocks, by hand (`byhand.py`), **ours in all five**.
  Both sides moved `anchor_at` to the caret's row on their own:
  CTRLO-OTHER-PICKERS folded `column_anchor` into it and deleted it ("one
  anchor function", `3369622a`, its round 2), PAGE-KEYS routed `anchor_at`
  through `column_anchor` (`0312cf42`) and corrected both docs' fx lines
  (`8e809d40`, comments). Ours' `anchor_at` body is theirs' `column_anchor`
  body line for line; ours' docs cite the lines theirs corrected to
  (`visual_layout.zig:705-713`, `input_presentation.zig:383-386@481104fd`).
- The ledger, 13 blocks: 11 `numbers`, 1 `ours` (the same cause), 1 by hand
  (`byhand.py`): the base's `at-ranking` and `at-folder-row`, which ours
  deleted (AT-RANK `a5b1cb59`, AT-FOLDERS `9294e3f2`) and wrote
  `at-plan-run-directory` in place of, and between which theirs added
  `at-ranking-anchor`: ours' lines, a blank line, theirs' entry whole. (The
  merge's message says "12 `numbers`"; it is 11 and the `ours` block,
  `resolutions.txt`.)
- `scripts/fxcap/capfile.py`, 1 block, `numbers`.
- `docs/parity.md`, 3 blocks, 13 rows; **one edit dropped**
  (`rowsmerge.py --drop`): CTRLO-OTHER-PICKERS' `/settings` sentence that
  `C-j` is a line break in the menu's search, "where fx's menu moves on
  `C-j` instead (SETTINGS-CJ-CK)". PAGE-KEYS binds `C-j`/`C-k` on the
  menu as fx's moves (`SettingsAction::Row`, `src/tui/settings.rs`), and its
  own edit at the same token says so; the dropped sentence describes the
  menu before it.
- **Behaviour fix** (`semantic.py 3`, join check 3 below): PAGE-KEYS'
  C-j/C-k check offered both keys to the `@` list first under the full
  transcript; the call goes.
- **Build fixes** (`semantic.py 3`): PAGE-KEYS' test helper `walked<T>`
  (`01d661ba`) has the name of AT-FOLDERS' `walked` (`d9b9cdd8`, its round 6)
  in the same test module: it is `rows_walked`, its five calls with it; and
  three of its tests take main's `CatalogDue`, as in merge 2.

**Merge 4** (`fb6dfdb8`, IMAGE-LOSS-COPY-FX):
- The ledger, 6 blocks, each `numbers`.
- **Build fix** (`semantic.py 4`): the unit deletes `ReplayedHistory::notices`
  and `Conversation::unsaid` -- the history holds no notice any more -- and
  its own tests' `notices.is_empty()` asserts with them. Train 13's join test
  `a_resumed_sealed_step_that_asked_replays_its_batch_and_says_nothing_of_its_reasoning`
  (main, `src/tui/worker.rs`) asserts it too: that assert goes, the test's
  comment says why nothing can say it, the rest of the test stays.

## The token check (`token_check.py`, `token-check.txt`, `token-self-test.txt`, `token_control.py`)

`token_check.py` is train 7's, byte for byte (`token-check.txt`'s first
line says so, `cmp` against `../train-7/`): every `docs/parity.md` row and
every ledger entry of each merge read against its base and both sides. It
**exits 1, on two rows, both read here**:

- **Merge 2, `/model`: "2 of 21 clauses not whole".** No token is missing
  or extra (the row's multiset is the one the sides owe). The two clauses
  are the two sides' edits that meet: ours (merge 1, main's MODEL-FUZZY
  text) rewrote "The composer, emptied, is its search (id or vendor, any
  case);" into the search's whole description, ending
  "`model_menu_presentation.zig:301-333`);", and theirs
  (CTRLO-OTHER-PICKERS) put "Ctrl-O over it shows nothing: ..." right after
  "any case);". The merge has theirs' sentence after ours' description and
  before "Up and Down", so ours' clause has theirs' text on its right where
  it had "Up", and theirs' clause has ours' text on its left where it had
  "case);" -- each clause's inner text whole, each one flank token the
  other side's. The sentence reads in order: what the search matches, then
  what Ctrl-O does over the menu, then its keys.
- **Merge 3, `/settings`: 29 tokens missing (26 kinds), none extra** --
  exactly the tokens of the sentence `rowsmerge.py --drop` takes out
  (above; its line in `resolutions.txt` quotes it), counted the same.

**The negative control** (`token-self-test.txt`): `token_check.py
--self-test` on each merge, **146 of 146 planted cases found** (29, 45, 45,
27); `token_control.py` (train 13's) plants its two cases on the merged
text of every row both sides changed -- where `--self-test`'s plant finds
nothing to replace, ten rows here -- **34 of 34 found**.

## Join checks

Each with its evidence. No join test is added: every meeting is reached by
a unit's own cases, named under each check and read from the gate's own
runs (`named.sh`, `named.txt`).

### 1. The `@` stack x main's trains 12 and 13 (`joins.sh`, `joins_txt.py`, `joins-txt.sh`, `joins.txt`, `functions.txt`)

**INPUT-ROW-LIMIT's page against the `@` list.** The stack and
CTRLO-OTHER-PICKERS were built on the old `input_row_limit` (`f7ffc4bb11a6`,
4 lines, at `af2b6137`, `cb7c9c18` and `e7e1bcf6`); main's is INPUT-ROW-
LIMIT's (`2d380a6191ba`, 9 lines), and the head holds main's
(`functions.txt`). The `@` list takes its page from it (`offer_to_files`,
`pages.txt`'s `:6548`, a line every side holds). INPUT-ROW-LIMIT's cases
pass at the head, in both runs (`named.txt`, J1):
`at_41_rows_a_page_of_the_at_list_is_19_paths`, the composer's cap at 41
rows and an eight-row screen's one-row page. In the head's main sweep
`p5-input-row-limit`'s `at` and `at-pgdn` are normalized-identical, and
every `p5-input-at*` pair is as at the base or better ("Sweeps", below).

**LOCAL-PATHS against the `@` list's absolute paths, at a 24-byte and a
49-byte TMPDIR** (49 is CI's TMPDIR, train 12's ci-socket receipt; CI's
sweep itself runs under `$RUNNER_TEMP`, below):
`joins.sh tmpdir` swept `p5-input-row-limit*`, `p5-input-at*` and
`workspace*` (63 pairs) on the head's build with TMPDIR, and so every run
root, under `/tmp/claude-<uid>/` at each length, fx's sha256 the pin before
and after (`joins.txt`):

| TMPDIR | pass | against the head's main sweep (its own TMPDIR) | capture lines holding the TMPDIR / matching the brief's pattern |
|---|---|---|---|
| 24 bytes | 51 of 63 | the same 63 verdicts | 0 / 0 (71 lines hold a placeholder) |
| 49 bytes | 49 of 63 | 4 differ | 0 / 0 (70 lines hold a placeholder) |

Redaction holds at both lengths: no capture line holds the run's TMPDIR or
any path the brief's pattern names, and `workspace-index`'s `at-plan` row
13 -- the `@` list's added folder by its absolute path -- is a placeholder
as wide as the path on both sides (77 columns at 24 bytes, 102 at 49).
The four that differ at 49 bytes:

- **`workspace` `clear-row`, `picker-down1`, `picker-one`: unresolved at 49
  bytes, normalized-identical at 24 and in the sweep.** Row 27, the
  workspace picker's row for the added folder, cut by the picker at the
  screen's edge. **Row 27 is on file** (`joins.txt`: each pair, each side,
  both trees, both lengths, with the placeholder's root and width, and the
  sweep's own reading of the row). At 24 bytes both sides capture
  `<scratch:real___...>/extra    Active · Saved`: the run directory's
  placeholder, 62 columns, and the folder whole. At 49 bytes both capture
  `<tmp:real___...>/fx… Active · Saved`, the same text: the run directory
  is cut before its end, so the root `roots.py` finds is the capture's
  TMPDIR, `<tmp>/fxsw.<8>/<NNN>` (74 columns), and `/fx…`, the cut
  `fxcap.` name's first letters, is left; the ledger's `product-name`
  normalization reads fx's `fx` as `xfx` (`\bfx\b`, fx's side only), and
  the sweep's reading is fx `/xfx…` against xfx `/fx…`. **Main's, not the train's**: the base's build,
  run the same way (`joins.sh tmpdir` on the base sweep's clone, the same
  three globs, 53 pairs there), has the same 24-byte verdicts as its own
  sweep and at 49 bytes these same three pairs and no other unresolved
  where its sweep passes (`joins.txt`, its last part). ("Found on the
  way": no unit here or queued in the brief owns a path the picker cuts
  with `…`; LOCAL-PATHS-WRAP, train 15, is a root wrapped behind a
  continuation mark, and its README names `…` no continuation.)
- **`workspace-index` `at-plan`: normalized-identical at 49 bytes,
  unresolved at 24 and in the head's sweep** -- join check 5's flip (the
  bold letters inside the placeholder follow the run directories' random
  names).

**Which tests cover it**: INPUT-ROW-LIMIT's three above; the stack's own
`@` cases and LOCAL-PATHS' harness tests (`test-capture.sh`,
`test-sweep.sh`, in the gate).

### 2. AT-LAUNCH-WALK's launch-time index (`joins.txt`, `walk_rows.py`, `named.txt`)

`p5-input-row-limit` swept five times one after another on the head's build
(`joins.sh walk`, `sweep-rerun.sh`, `--jobs 1`), in the same hold of the
suite lock as join check 1, other workers' gates and sweeps running beside
it (load at each start 8.21, 7.41, 11.30, 11.07, 12.50). **Row 5 of `at`,
each run:**

| run | load | fx | xfx | `at` | `at-pgdn` |
|---|---|---|---|---|---|
| 1 | 8.21 | `  items/f-01.txt` | `  items/f-01.txt` | normalized-identical | normalized-identical |
| 2 | 7.41 | `  items/f-01.txt` | `  items/f-01.txt` | normalized-identical | normalized-identical |
| 3 | 11.30 | `  items/f-01.txt` | `  items/f-01.txt` | normalized-identical | normalized-identical |
| 4 | 11.07 | `  items/f-01.txt` | `  items/f-01.txt` | normalized-identical | normalized-identical |
| 5 | 12.50 | `  items/f-01.txt` | `  items/f-01.txt` | normalized-identical | normalized-identical |

Never `indexing files...`: the index is ready by the snap at the head.
**These runs cannot tell the head from the base**: the base passes the
same pair in its own sweep too (`sweep-base.md:133` and
`sweep-head.md:153`, the same `p5-input-row-limit` line), so the five readings show the head
ready, not that the train made it so. What does tell them apart is
AT-LAUNCH-WALK's own cases of the launch walk, which the gate ran at the
head, ok in both `cargo test` runs (`named.txt`, J2):
`the_launch_starts_the_index_walk_fresh_or_resumed` (`src/tui/mod.rs:1204`),
`the_launch_walks_a_root_once_and_an_empty_root_never`
(`src/tui/file_index.rs:2260`) and
`a_fresh_sessions_first_at_after_the_launch_walk_is_whole_shows_its_rows_at_once`
(`src/tui/shell.rs:38632`).

### 3. CTRLO-OTHER-PICKERS x CTRLO-AT-KEYS x PAGE-KEYS-SETTINGS-COLUMN x main's `shell.rs` (`functions.txt`, `shared.txt`, `red3.txt`, `gate-6bc758a0.txt`)

**The functions** (`functions.py`: each function's sha256, 12 hex digits,
per commit, of its body from its `fn` line, doc comments not included).
`offer_to_pickers`, `composer_newline` and `composer_control` are
CTRLO-OTHER-PICKERS' own: absent at every other side, and its text at every
merge and at the head (the join commit rewrote the last two's doc comments
only, below). `act`: main's and CTRLO-OTHER-PICKERS' changes, both kept at
merge 2 and the same at every commit after it.
`consume_inputs`: main's, CTRLO-OTHER-PICKERS' and PAGE-KEYS', each read
at its merge from both ends (`shared.py`): merge 1 main's alone (the stack
did not touch it), the head holding its text; **merge 2 true both ways**
(4 and 1 edits; `act` 2 and 1; `settle_band` 2 and 1); **merge 3 false
both ways** for `consume_inputs`, and for `anchor_at` (ours by hand,
above). So the merged versions do not keep every side's change, and these
are the lines each lost:

- **Merge 3's behaviour fix.** At its base PAGE-KEYS' check sends `C-j`/`C-k`
  under the full transcript to `offer_to_files` first ("The `@` list still
  sees them: its keys there are unit CTRLO-AT-KEYS's"), because the `@`
  list then took them. CTRLO-AT-KEYS made `offer_to_files` refuse them under
  the screen (its two early returns, `cb7c9c18`, `offer_to_files` 117
  lines), and CTRLO-OTHER-PICKERS moved that refusal into `consume_inputs`
  (`composer_control`, after `offer_to_mcp`) and deleted it from
  `offer_to_files` (109 lines). As git merged merge 3, PAGE-KEYS' check runs
  first and hands the keys to an `offer_to_files` that no longer refuses
  them: **the `@` list moves under Ctrl-O again.** `red3.sh` shows it: merge
  3 redone with its build fixes alone fails CTRLO-AT-KEYS' two cases
  (`c_j_under_the_full_transcript_is_a_new_line_and_closes_the_list`: left
  `"@src"`, right `"@src\n"`; `c_k_..._kills_to_the_lines_end_and_keeps_the_list`:
  left `"@src tail"`, right `"@src"`); with the behaviour fix the three pass,
  and at `594fddc8` too (`red3.txt`). The fix drops the call to
  `offer_to_files` from PAGE-KEYS' check, which is what its comment defers
  to; the two lines it loses are that call and its `}`.
- **The join commit `594fddc8`.** Merged, the two checks overlap: PAGE-KEYS'
  catches every `C-j`/`C-k` with the screen up before `offer_to_mcp`, so
  CTRLO-OTHER-PICKERS' `to_the_composer`, after it, is never true when
  reached. One stays, at PAGE-KEYS' place, with CTRLO-OTHER-PICKERS'
  predicate (`composer_control`), and the other's three lines and comment go
  (`functions.txt`, `consume_inputs`, `m4 -> join`). **Why PAGE-KEYS'
  place:** fx's raw byte path asks its modal surfaces first
  (`routeActiveModalInput`, `app_input_runtime.zig:873@481104fd`), whose MCP
  host menu passes both bytes on (`routeMcpHostMenuByte` returns `false`
  for a byte under `0x20`, `:1186`, `:1812-1824`), then
  `routePickerControlByte` (`:879`), which refuses them under the screen
  (`composerPickerSurfaceVisible`, `:1094`, `:1110-1116`): the composer gets
  them with `/mcp`'s menu up. So C-j/C-k over `/mcp`'s menu under Ctrl-O
  are the composer's, as PAGE-KEYS' case asserts (round 3, `/mcp`'s menu
  in `under_the_full_transcript_no_other_list_takes_a_row_key`), and not
  swallowed, as CTRLO-OTHER-PICKERS' doc said; its menu still swallows
  every other key under the screen (`offer_to_mcp`, opus-C). No key is
  routed otherwise than at merge 4.
- **Two doc comments the merges made false**, rewritten in the join
  commit: `composer_control` said `/mcp`'s menu takes the keys under the
  screen; `composer_newline` said the settings menu and the model column
  bind no `C-j` (SETTINGS-CJ-CK, MODEL-COLUMN-CJ-CK) -- PAGE-KEYS binds both,
  and only an empty settings list still hands `C-j` on (`offer_to_settings`'
  "a move that moved nothing is not taken").
- **A case of PAGE-KEYS' that the merge made fail, fixed in `725b3c90`
  (tests only).** `under_the_full_transcript_the_model_menu_takes_no_row_key`
  asserts C-j types a newline over `/model`'s menu under the screen; fx's
  `insert_newline` adds nothing while `/model`'s menu, the session picker
  or `/skills`' menu is up (`app_input_runtime.zig:514@481104fd`), which
  CTRLO-OTHER-PICKERS copied (`composer_newline`'s `held`). The first gate,
  at `6bc758a0`, failed that case alone, in both `cargo test` runs
  (`gate-6bc758a0.txt`: 4926 and 4949 passed, 1 failed each); merge 3 is
  the first commit that holds both the case and `composer_newline`. The case's expectation is now fx's: its helper takes `held`,
  and the `/model` menu's call passes it. The 19 lines it adds moved six
  ledger references, re-pointed in `3a2830fb`.

**Which tests cover the meeting now** (`named.sh`, `named.txt`, read from
the gate's own two `cargo test` runs; J3 there): CTRLO-AT-KEYS' seven cases
(the `@` list under the screen), CTRLO-OTHER-PICKERS' fourteen (the burst:
C-j held over `/model`'s, the session and `/skills`' menus, the catalog's
and settings menu's marks kept; the slash menu, the `/provider` column and
`/mcp`'s arguments under the screen), PAGE-KEYS' five full-transcript cases
(every other list, `/mcp`'s menu among them; `/model`'s menu as `725b3c90`
has it), MCP-C's `the_menu_takes_no_key_while_the_full_transcript_covers_it`:
**27 cases, each ok in both runs**. Each side's own tests reach the
meeting; no join test is added.

### 4. PAGE-KEYS-SETTINGS-COLUMN x INPUT-ROW-LIMIT (main) (`pages.sh`, `pages.txt`)

PAGE-KEYS' base `3d3d56c8` is INPUT-ROW-LIMIT's head itself, the commit
main merged in train 12 (`12b56ab8`; `pages.txt`'s first line). Every page
a list takes is `layout::input_row_limit(rows)`: **20 lines of
`src/tui/shell.rs` at the head call it**, and blame names each one's
commit (`pages.txt`; round 1 read 16, missing four tests' `crate::tui::layout::`
form): 15 are held by every side (the catalog, `/skills`, the `@` list,
sessions, `/model`, `/effort`, `/fork`, the picker, `/provider`, `act`,
and five tests); **5 are PAGE-KEYS' own** -- `offer_to_settings`,
`offer_to_column`, `offer_to_mcp_args` (`c4bfa8b1`), `composer_rows`
(`2cbf4d41`) and its test helper `pages_as_fx_does` (`01d661ba`), held by
no other side; the four round 1 missed are tests. `functions.py`
over its four production functions and `input_row_limit`: **`input_row_limit` is one text at
PAGE-KEYS' base, its tip, main, merge 3 and the head** (`2d380a6191ba`,
9 lines), and each of PAGE-KEYS' four is its tip's text at merge 3 and at
the head. So each page PAGE-KEYS sized is the size it was built and tested
with. Its seven page cases (`named.txt`, J4: the five `at_24_rows_*`,
whose helper merge 3 renamed `rows_walked`, and the two taller-composer
ones)
are ok in both `cargo test` runs, and its scenarios (`p5-page-keys-anchor`,
`-column`, `-mcp`, `-settings`, all new) were swept at the head: 57 pairs,
45 pass, the 12 others `p5-page-keys-settings`' under `sound-max`, the
same Sound level `max` row as above (`new-scenarios.txt`).

### 5. The ledger (`anchor.txt`, `8f622949`, `16fff02d`)

- **`at-plan-run-directory` (AT-RANK's, `e2cba14b`), deleted in
  `8f622949`.** Its `fx_row` and `xfx_row`,
  `^  /\S+/fxcap\.XXXXXX/extra/notes/$`, read the run directory as an
  absolute path. Since LOCAL-PATHS (train 12, on main) capture.sh writes a
  run's root as a placeholder as wide as the path (`<scratch___>`,
  `scripts/fxcap/roots.py`), which neither row can match, so on this train
  the cause covers nothing. Measured at `04e4491d` (`anchor.txt`): row 13
  of `workspace-index` `at-plan` reads `   <scratch___...___>/extra/notes/`
  on both sides, and the pair is normalized-identical under no cause. A
  comment where the cause stood says so, beside AT-RANK's own for
  `at-ranking`. The pair itself passes by the run, not always: its bold
  letters inside the placeholder still follow the random run directories,
  which the cause was written for, on main's code too; its owner is
  WORKSPACE-INDEX-AT-PLAN ("Sweeps", and the decision below).
- **`at-ranking-anchor` (PAGE-KEYS-SETTINGS-COLUMN's), deleted in
  `16fff02d`.** It was `at-ranking` on the unit's anchor scenario
  (`p5-page-keys-anchor` `file-one`, owner I-U2), and merge 1 brought
  AT-RANK, which closed I-U2. Swept at `04e4491d` with the cause still in
  the ledger (`anchor.txt`): `file-one` normalized-identical at 40x120 and
  at 24x60, the size the unit swept it at, under no cause. The other cause on
  that scenario, `model-column-clip` (MODEL-COLUMN-CLIP, a queued unit),
  still covers `model-wrapped` at 24x60 and stays.
- **CTRLO-OTHER-PICKERS' nine `p5-ctrlo-burst-*` pairs, confirmed**
  (`new-scenarios.txt`, the head's sweep): **3 pass** -- `p5-ctrlo-burst-model`'s
  `typed`, `burst-cj`, `burst-ck`, normalized-identical -- and **6 do not, each
  row under a cause older than the unit**, no row uncovered:
  `p5-ctrlo-burst-help`'s three under `menu-count`, `menu-shift` and
  `C7-help-entries` (the catalog's commands fx has and xfx has not: `Commands
  21`/`19` and `36`/`33`, `/login`, `/logout`; 13 rows in `typed`, 8 in each
  burst), `p5-ctrlo-burst-settings`' three under `sound-max` (Sound level's
  `max`, one row each). Each of the four causes is in main's ledger and in the
  unit's base's (`cb7c9c18`). The unit's README counts 9 rows in each help
  burst: its `pairs.txt` is its own reader, without the ledger, and its ninth,
  `/version show the fx version`, is the same row once the ledger's
  `product-name` reads fx's `fx` as `xfx`.
- The merges' other ledger changes are the units' and main's, read entry
  by entry by the token check (`token-check.txt`).

### 6. References (`refs*.py`, `content.sh`, `refs_content.py`, `repoint.py`, `repointed.py`)

**Before any re-point**, at `8f622949` (the merges, the join commit and the
first ledger commit; `refs-*-pre.txt`, six sides: main and the four tips):
`refs.py` 29 (side, reference) pairs MOVED; outside the historical plans
(`docs/superpowers/plans/`), `refs_all.py` 24, `refs_short.py` 16,
`refs_bare.py` 5; `refs_prd.py` 0.

**The content check over every reference** (`content.sh`, new: train 13
ran `refs_content.py` over seven files; this runs it over every tracked
file that holds a reference by its own pattern, outside `.prd/` and the
historical plans -- 55 files, 790 references). **289 are checked**: each
is read against the commit that wrote its numbers (its frame), the cited
line and two either side standing at the head's number (at the head 243
SAME, 41 SAME-ROW, 3 SAME-LINES, 2 WRONG, `content-head.txt`'s last line).
The other 501 are not checked: 500 point into fx's tree and are counted
only (`refs_content.py`'s "other product"), and 1 is unread (`UNREAD`, a
test's number past any file's end).

| at | WRONG | `content-*.txt` |
|---|---|---|
| main `dcfa5146` | 35 | `content-dcfa5146.txt` |
| the `@` stack `4130f4ac` / CTRLO-OTHER-PICKERS `e7e1bcf6` / PAGE-KEYS `3ea16930` / IMAGE-LOSS `bde1ddfa` | 57 / 57 / 35 / 41 | `content-<tip>.txt` |
| `8f622949`, before the re-point | **80** | `content-8f622949.txt` |
| the test fix `725b3c90` | 8 | `content-725b3c90.txt` |
| the code head `b3dffdad`, as it is | **2** | `content-head.txt` |
| the same, blame ignoring the four re-points | 14: twelve of the entries set by hand, by text or as the extra (below; 10 `hand`, 1 `text`, the `extra`), read against frames that named other text, and the 2 `left` | `content-head-ignoring.txt` |

**Where the 80 came from** (`repoint-plan.txt`, `repoint.py --derive` at
`8f622949`; a ref is a side's when that side's content check reads it WRONG
with the same citing file, frame and numbers): **43 moved by the merges**,
35 WRONG on main already, 2 on IMAGE-LOSS' head. Train 13's ten (its
`join-main/README.md`: the ledger's `.prd/07-fx-parity-program.md:19`, five
in `smoke-tui.sh`, four in `tests/tui.rs`) are among main's 35.

**How each was moved** (`04e4491d`, one corrected in `6bc758a0`; 81 plan
lines):
- 57 `auto`: the frame's cited lines and two either side stand at exactly
  one place of the head's file -- the new number;
- 8 `lines`: the cited lines alone stand at one place, a neighbour changed;
- 11 `hand` and 2 `text`, read by hand, each with the text it now names
  (`repoint.py`'s table): among them `smoke-tui.sh`'s `term.rs:32-52`
  twice (the mode set and restore constants, now `:56-113`) and train 13's
  `shell.rs:2703` ("one `editor.insert()` call", `Shell::pasted`'s inline
  arm, `:13043`); the ledger's `.prd/07-fx-parity-program.md:19` pinned to
  the commit whose line 19 it quotes (`:19@6bc31872`; `refs_prd.py` reads
  29 references at the head, 30 before, for that pin);
- 2 `left`, with why: `src/tools/question.rs`'s `Cargo.toml:42` sits in a
  comment that says the crate does not enable serde_json's
  `preserve_order`; main's `:42` already enabled it (`content-dcfa5146.txt`),
  so the claim is out of date, and at the head the number is too: `:42`
  holds `"http2",` (`content-head.txt`) and serde_json's line is `:46`
  ("Found on the way"); and
  `tests/session_compat.rs`' `src/session/event.rs:98-192` names commit
  `14ad04202fe5`'s file (`14ad04202fe5 src/session/event.rs:98-192`), whose
  98-192 are `SessionEvent` as it says -- the check reads it against the
  head;
- 1 `extra`: train 13's other `smoke-tui.sh` ref, `input.rs:1152` for
  `C-_`, which the check reads SAME because its frame already named the
  wrong line: `0x1f => Action::Undo,` is `:1347`. Train 13's README names
  these two `smoke-tui.sh` lines as the content check's blind spot (its
  third such ref is the ledger's `:1747`, which train 13 fixed).

**`repointed.py`** (train 13's check over this plan; `repointed.txt`):
**81 plan lines, 81 ok** at `6bc758a0`, where the plan's numbers stand;
`04e4491d` and `6bc758a0` 74 lines out, 74 in, the same once numbers are
masked and the two text spans put back. It found the one wrong hand
number: `04e4491d` wrote `frame.rs:2733-2772` for `place()`, whose doc
starts at `:2734`; `6bc758a0` corrected it. **`3a2830fb`**: the six
`src/tui/shell.rs:46692` the test fix moved (to `:46711`, the same
`"  tools: {} advertised ({},",`), 6 lines out and 6 in, numbers only;
`content-725b3c90.txt` reads exactly those six WRONG beyond the two left.

**`b3dffdad`, two the content check cannot read.** `refs_content.py` reads
a bare `:N` on its path's own line only; `refs_bare.py` reads four lines
on. At `3a2830fb` it read two MOVED outside the historical plans
(`refs-bare-3a2830fb.txt`): `scripts/fxcap/capfile.py:181`'s
`:15214-15216`, the full transcript's `if self.owner !=
ScreenOwner::Primary {` and its two lines in `warning_shown` -- right on
main, moved by the merges, its path on `:180` re-pointed in `04e4491d` and
it not -- now `:15609-15611`; and `src/tui/shell.rs:24980`'s `:6272` after
`shell.rs:6178` (re-pointed in `04e4491d` from `:6084`), moved with its
partner by its text to `:6384` (what the pair names is wrong since it was
written, "Found on the way"). 2 lines out, 2 in, numbers only
(`repointed.txt`'s last line).

**At the code head `b3dffdad`** (`refs-*-head.txt`): **`refs.py` 0 MOVED
(89 references); `refs_prd.py` 0**; outside the historical plans
`refs_all.py` 1, `refs_short.py` 0, `refs_bare.py` 0. The one is
`scripts/fxcap/test_sweep.py`'s `cells.py:100000`, a test's number past
any file's end (the content check lists it `UNREAD`, out of range, as train
13's head had it). Inside the historical plans `refs_all.py` 25,
`refs_short.py` 69 and `refs_bare.py` 68 entries: those plans are outside
the content check's and the re-point's scope, as in train 13, and are not
read here. `refs.py` has no `--fix`: the fixing is `repoint.py --apply`,
and `refs.py` reads 0 after it.

### 7. Network hermeticity (`jobs.sh net`, `net.py`, `jobs-net.txt`, `connect.log`)

Train 13's method (FLAKE-CATALOG-EVENT's `connect_logger.py`, a loopback
logger that answers 403 and forwards nothing; the four proxy variables at
it, `NO_PROXY`, `no_proxy`, `ALL_PROXY`, `all_proxy` unset), a clone at
`b3dffdad`, one hold of the suite lock, the same three test binaries:

| run | result | CONNECTs logged during it |
|---|---|---|
| `cargo test --locked --lib --no-fail-fast` | 4927 passed, 0 failed, 7 ignored | **2**, both `CONNECT w.example:443` |
| `cargo test --locked --test tui --no-fail-fast` | 189 passed, 0 failed | **0** |
| `cargo test --locked --test sessions --no-fail-fast` | 67 passed, 0 failed | **0** |

**No CONNECT to the Gateway host**; the two `w.example` lines are the
known MCP probe (trains 6 to 13). The other integration binaries ran in
the gate, with no logger. No unit of this train adds a network path: the
`@` stack walks and ranks local folders, CTRLO-OTHER-PICKERS and
PAGE-KEYS route keys, IMAGE-LOSS-COPY-FX changes what a request's history
holds of an image whose copy is gone.

## Commits partway along the branch (`midbranch_scan.py`, `final-scan.sh`, `final-scan.txt`)

The brief's check over **every commit of `dcfa5146..HEAD`**: the units'
204 (the `@` stack 146, CTRLO-OTHER-PICKERS 24, PAGE-KEYS 15, IMAGE-LOSS 19,
each since its base; `final-scan.txt` part 2's first line) and this
train's, each as `git show --format=` prints it (a merge as its combined
diff), every hit classified by `midbranch_scan.py` (train 13's; the
account's name and uid read at run time, never written; no hit printed but
its kind and file):

- **No real user name or credential**: NAME 0 and CREDENTIAL 0 in every
  commit and in the range; no commit message holds the account's name. The
  train goes on.
- **The units' own commits hold 9 lines that hit, 12 matches, every one
  SCRATCH: a sweep run directory's name** (`fxsw.` and eight characters), in
  AT-FOLDERS' `sweep-base.md` and `sweep-head.md` and AT-SCOPE-INTERIM's
  `at-plan.diff`, `samepair.txt` and `sweep-head.md` (the `workspace-index`
  at-plan row, which runs through the run directory). Scratch roots, not
  names. They are in history, which this train does not rewrite; at the
  head each is `fxsw.<run-N>` (`09b13511`), so **the range,
  `git diff dcfa5146..HEAD`, reads 0**.
- Every commit this train adds reads 0, the merges as their combined diffs
  and as everything they bring (part 1).
- Beyond the brief's terms, `shapes.py`'s run-name shape (a capture's
  `fxcap.` and six) matches 29 added lines of the range, all the units'
  (`final-scan.txt` part 1, by file: AT-RANK's `at-plan-sweep/` captures
  and README, AT-SCOPE-INTERIM's `samepair.txt` and README, AT-RANK-PERF's
  README, a `src/tui/file_index.rs` test): random directory suffixes, kept
  as the units committed them; each commit of this train that rewrote a
  line holding one carried it and added none (`pathcheck.sh`'s
  `carried.py`).

## Gate (`gates.sh`, `gates.txt`, `gate-counts.txt`, `gate-6bc758a0.txt`)

`gates.sh` (train 13's, with the one step ci.yml's gate job gained since,
AT-RANK-PERF's `check-at-search-cost.sh`, where ci.yml runs it): every step
of `.github/workflows/ci.yml`'s gate job and `sweep.yml`'s two harness
tests, in a `git clone --no-local` at **`b3dffdad`**, one after another,
**the whole script inside one hold of the suite lock** (one normal slot),
each step's exit taken from `$?` on the line right after its command, the
tree clean before and after; TMPDIR the harness scratch root itself.
**23 steps, 0 failed, none run again** (`gates.txt`); with 0 failed in both
`cargo test` runs, no catalog-wait timing test (CATALOG-WAIT-CLOCK's flake
class) needed its rerun:

| step | at `b3dffdad` |
|---|---|
| `cargo fmt --check`; clippy; clippy `--features fault-injection` | ok; ok; ok |
| `cargo test --locked --no-fail-fast` | **5854 passed, 0 failed, 7 ignored** |
| the same `--features fault-injection` | **5891 passed, 0 failed, 7 ignored** |
| `check-xfx-identity` / `check-no-stubs` / `check-no-secrets` | ok / ok / ok |
| `check-no-local-paths` / `test-check-no-local-paths` | ok (22064 tracked files; 8 marked fixture lines, 80 placeholder lines) / ok (10 cases, 28 plants) |
| `check-preview-contract` / `check-tui-preflight-cost` | ok / ok (200x300 full replay 1.16 ms mean; budget 32) |
| `check-at-search-cost` / `check-usage-clock` | ok (every keystroke, 1460 searches: p50 0.51 ms, p95 3.06 ms) / ok |
| `test-check-no-secrets` / `test-gate-env` / `test-preview-contract` | ok (13 cases) / ok (66 cases) / ok (28 mutants) |
| `fxcap/test-sweep.sh` / `fxcap/test-capture.sh` | 76 + 28 OK / 109 OK |
| release build; `fault-injection` release build | ok; ok |
| `smoke.sh` / `smoke-tui.sh` | 47 checks, 0 failures / 37 scenarios + the oracle, 1167 checks, 0 failures |

The counts are `gate-counts.txt`'s (each binary's `test result` line and
their sums, from the step logs). Load at the steps' starts 5.5-24.8.

**Every side's tests are in** (`test-counts.txt`, train 7's count: lines
holding `#[test]` under `src` and `tests`): main 5612; the `@` stack +105
(5489 against its base `af2b6137`'s 5384), CTRLO-OTHER-PICKERS +27 (5486
against `cb7c9c18`'s 5459), PAGE-KEYS +12 (5474 against `3d3d56c8`'s 5462),
IMAGE-LOSS +10 (5462 against `714f6283`'s 5452): **5766 at the head, main +
154**, each merge adding exactly its side's count (5717, 5744, 5756, 5766)
and no commit after them changing it. `cargo test` 5854 against main's 5699
(train 13's join-main gate at `0dbfaf39`, whose code is main's: `main-gate.sh`, `main-gate.txt`):
**+155** (`main-gate.txt`, binary by binary): `src/lib.rs` +152 -- under
`src/` the range adds 155 `#[test]` and deletes 4, and adds one
`#[tokio::test]`, which train 7's count does not
read -- `tests/sessions.rs` +2, `tests/tui.rs` +1; with fault injection 5891 against 5736, the same
+155. The harness's `def test_`: 212 on main, 213 at the head (IMAGE-LOSS
+1), the two harness suites' 104 + 109.

**The first gate** (`gate-6bc758a0.txt`), at `6bc758a0` before the test
fix, was stopped after its fifth step: `cargo test` and the fault-injection
run each failed one case, the same one,
`under_the_full_transcript_the_model_menu_takes_no_row_key` (join check 3),
4926 and 4949 passing.

## Sweeps

**Builds** (debug, as trains 4 to 13), each in its own `git clone
--no-local`, each swept with its own tree's harness, scenarios and ledger
(`sweep.sh`, train 13's). fx: the v0.0.16 binary, sha256 `8e6cda2e...`
before and after each sweep, `FX_AUTO_UPGRADE=0`; 40x120, `--jobs 2`; each
sweep inside one hold of the suite lock (`sweep-runs.txt`: each sweep's
lines, its `run.json`'s start, wall time and loads). Two selections, each
swept at the head `b3dffdad` and at the base `dcfa5146` (`selections.sh`,
`selections.txt`: both made again here, the same names):
- **the main selection** (`selection-all.txt`, `selection.py`): the
  brief's twelve globs, every scenario named by a ledger entry the four
  merges or the first ledger commit changed, every scenario a merge added
  (each unit's own), and train 13's extra, the scenarios the re-point's
  entries name; `*` (`session-record`, `stats-bytes`) left out;
- **the units' selection** (`selection-units.txt`, `selection_units.py`):
  the 93 scenarios the units' own nine committed head sweeps took (154 in
  all) that the first does not, so every scenario a unit swept is swept
  here (AT-RANK-PERF committed no sweep).

Each base sweep was rescored with the head's ledger (`rescore.sh`,
`sweep.sh --rescore` on a copy): the pass line the same before and after,
and **pair by pair 0 verdicts moved** in both (`sweep-runs.txt`; the
ledger's sha256 `1b43f828`, the head's). Every xfx snap that shows the
banner names the build swept: `b3dffda` on all of the head's (494 and
477), `dcfa514` on all of the base's (327 and 477; `banners.py`, which now
reads main's banner form of 2026-11-10, `x𝒇x v0.2.0 (debug <date>-<rev>)`,
too). Loads at the sweeps' starts 9.05-17.68, peaks 14.17-23.48; walls
20m31s to 36m06s (`sweep-runs.txt`).

| sweep | scenarios | pairs | pass | failed | timeout |
|---|---|---|---|---|---|
| head `b3dffdad`, the main selection (`sweep-head.md`) | 92 | 561 | **348** (62.0%) | 0 | 0 |
| base `dcfa5146`, the same (`sweep-base.md`, rescored) | 67 | 389 | 215 (55.3%) | 0 | 0 |
| head, the units' selection (`sweep-head-u.md`) | 79 | 544 | **372** (68.4%) | 0 | 0 |
| base, the same (`sweep-base-u.md`, rescored) | 79 | 544 | 372 (68.4%) | 0 | 0 |

The head's 25 scenarios more are the units' new ones, 172 pairs, 122 pass
(`new-scenarios.txt`), each scenario's unresolved pairs with their causes.
Their unresolved rows are all under causes but in seven scenarios, each
its unit's own, whose README reads the rows: IMAGE-LOSS-COPY-FX's four
`p5-image-loss-*` (5 pairs; `../image-loss-copy-fx/README.md`, each
scenario against fx) and CTRLO-OTHER-PICKERS' `provider-ctrlo`,
`skills-ctrlo` and `skills-ctrlo-dollar` (16 pairs; the gated sign-in
providers and the source tabs its README's table names,
`../ctrlo-other-pickers/README.md`). So 215 + 11 + 122 =
348. The units' selection names 93 scenarios: 79 give pairs, the 14
`p5-lifecycle-osc-*` read the wire alone (each `ran`, no snap to pair, at
the head and the base, as AT-LAUNCH-WALK's own sweep has them,
`compare-units.txt`).

### Against the base, pair by pair (`compare-base*.txt`, `screen-moves-base*.txt`, `screen-classes*.txt`, `screen-partition.txt`, `gained-rows*.txt`, `reruns.txt`)

**The main selection, 389 shared pairs: 215 -> 226; 12 up, 1 down, 1 moved
within passing** (`compare-base.txt`):
- **12 up, the `@` stack's**, each unresolved -> normalized-identical,
  its differing rows all gone and none new: AT-FOLDERS' eight (a folder's
  own row in the list) -- `p5-input-at` `at-src`, `at-src-enter`,
  `at-src-tab`, `p5-input-at-dir` `dir-enter`, `filepicker` `at-dir`,
  `at-dir-down`, `at-dir-cj`, `at-tab`, as its README lists them
  (`../at-folders/README.md`, its sweep) -- and AT-RANK's four (fx's
  ranking): `p5-input-at` `at-st`, `at-st-down12`, `at-st-up6`, `at-stat`
  (`../at-rank/README.md`, §7).
- **1 down: `workspace-index` `at-plan`**, normalized-identical ->
  unresolved, row 13, attributes only -- **a flip that is older than the
  train, traced, and owned by WORKSPACE-INDEX-AT-PLAN; fixed on main by
  PR #94 and joined here in `583cc13a`; at the joined head it passes on
  every run, row 13 the same on both sides ("The join of main")**. Rerun 3 times on
  each tree it passes 3 of 3 at the head and 3 of 3 at the base
  (`reruns.txt`); of the head's six runs of it here (the sweep, join check
  1's two TMPDIR runs, the three reruns) four pass, of the base's six, six
  (`reruns.txt`, `joins.txt`) -- too few runs to tell the trees apart. Row
  13 is the added folder by its absolute path, one placeholder on both
  sides, the same text; what differs is which of its letters are bold.
  **The mechanism**, traced by WORKSPACE-INDEX-AT-PLAN's receipt (`fix/p5-workspace-index-at-plan` @ `495fbb40`, `.prd/tui-phase5/receipts/workspace-index-at-plan/README.md`,
  §2): the sweep runs a unit's fx and xfx captures in one directory,
  `<tmp>/fxsw.<8>/<NNN>`, and capture.sh makes each run its own
  `fxcap.XXXXXX`, six random letters per side; `@plan` matches the folder
  only through its whole lower-cased path, so whether its row is listed at
  all, and which letters are bold, depends on which letters those random
  names hold; and LOCAL-PATHS' placeholder keeps a root's colours and
  attributes where they stood (`scripts/fxcap/roots.py`'s doc), so the bold
  survives inside it. **On main's code too**: that receipt's twelve copies
  of the scenario differ in 2 of 12 for xfx `8f921425` at a 24-byte TMPDIR
  (its `red/8f921425-24.txt`) and 9 of 12 for `d2b2ca75` at the gate's
  25-byte one (`red/d2b2ca75-25.txt`), both on main; and main's own CI sweep
  at `d2b2ca75` read the pair unresolved (its first paragraph). Its fix, one run path for
  both sides, is its PR's. The cause AT-RANK wrote for it,
  `at-plan-run-directory`, matched only the absolute path, so on this
  train it could not cover the row with or without its deletion: its rows
  cannot match the placeholder row (join check 5). CI's sweep fails a snap
  pair the base kept identical that the head does not, cause or no cause
  (`.github/workflows/sweep.yml:8-11`), so no cause would change CI's
  reading of this pair either.
- **`settings-collapse-open` `after-esc`, normalized-identical ->
  identical**: fx's own screen moved, the same fx binary in both sweeps --
  its row 19's spinner frame (`•` or a blank) and elapsed clock (`4s`,
  `3s`) -- and at the head it fell on xfx's exactly. Passing either way:
  rerun 3 times on each tree, `after-esc` passes 3 of 3 on both, as
  normalized-identical or identical by the run (`reruns.txt`).

**The screens** (`screen-partition.txt` ties the counts together):
compare.py finds fx's `.ansi` moved in 76 shared snaps and xfx's in 110;
by class, fx's are 70 in clock-like or run-local text alone, 0 beyond it,
7 in colour or attributes alone (77 in all: one `.txt` moved where its
`.ansi` did not); xfx's 70, 32 and 8. xfx's 32 beyond it
(`screen-classes.txt`): 16 a saved session's id, the 12 `@` snaps above,
and 4 `fulltranscript-notice` snaps' `* version:` line, the build's own id
(`2026-11-10-1858-dcfa5146b520` / `...-2057-b3dffdad3e0b`). The colour
moves on both sides: `image-paste`'s five (the OSC 8 link target's
snapshot directory, a random name each run), `workspace-index` `at-plan`
(below); fx's `settings-collapse-open` `after-esc` (above); xfx's
`model-column-reask` `col-held` and `p5-transcript-cancel` `cmd-running`
(a spinner's frame, an elapsed clock). **Rows gained** (`gained-rows.txt`):
2 pairs, `workspace-index` `at-plan` (above) and `p5-transcript-question`
`full-60`, unresolved on both, whose one new row is the build id in xfx's
wrapped banner row.

**The units' selection, 544 shared pairs: 372 -> 372; 0 moved, 0 rows
gained** (`compare-base-u.txt`, `gained-rows-u.txt`). Its screens
(`screen-partition.txt`, `screen-classes-u.txt`, `screen-moves-base-u.txt`):
fx's `.ansi` moved in 49 (24 clock-like or run-local, 20 beyond, 5 colour
alone), xfx's in 54 (45, 9, 0). The 29 beyond (`screen-moves-base-u.txt`
lists each): fx's 20 are 19 saved sessions' `--resume` ids and
`p5-lifecycle-compactok` `c02-compacting`'s spinner frame and clock (`
Compacting (0s)` / `• Compacting (1s)`); xfx's 9 are its build id where it
prints it (`help`'s five `* version:` snaps, `p5-lifecycle-relhome-unsaved`'s
`/status` Build row and its `.full` banner) and two `/status` snaps' saved
session id and Build row. No verdict moved.

### Against each unit's own sweep (`compare-units.txt`, the head's two sweeps read as one)

Every unit's committed head sweep (nine: AT-SCOPE-INTERIM's two rounds,
AT-RANK-PERF none) against the head's two sweeps, scenario by scenario
(`unit_compare.py board`; each unit swept at 40x120, as these did). Every
scenario a unit swept is in a head sweep; the 14 `p5-lifecycle-osc-*`
AT-LAUNCH-WALK swept read the wire alone and give no pair on either side
("not in this sweep's selection" there means no pair):

| unit (its sweep) | shared | pass, unit -> head | lower at the head |
|---|---|---|---|
| AT-FOLDERS | 26 scenarios | 99 -> 114 | 0 |
| AT-SCOPE-INTERIM (round 1 / round 3) | 12 / 12 | 56 -> 60 / 56 -> 60 | 0 / 0 |
| AT-LAUNCH-WALK | 28 (of its 42; the 14 others wire-only) | 84 -> 88 | 0 |
| CTRLO-AT-KEYS | 18 | 60 -> 64 | 0 |
| AT-RANK | 20 | 78 -> 78 | 0 |
| CTRLO-OTHER-PICKERS | 85 | 382 -> 384 | **2**: `fulltranscript-notice` 1 -> 0 of 4, `help` 9 -> 5 of 19 |
| PAGE-KEYS-SETTINGS-COLUMN | 37 | 166 -> 169 | **1**: `fulltranscript-notice` 1 -> 0 of 4 |
| IMAGE-LOSS-COPY-FX | 19 | 86 -> 86 | 0 |

- **`fulltranscript-notice` `end` and four `help` snaps** (`ctrl-c`'s
  version rows among its others; `enter-exact`, `enter-row`,
  `esc-after-ctrlc`, `typed-exact`): each passes on the unit's sweep and is
  unresolved at the head **and at the base**, the same rows
  (`compare-base.txt` and `compare-base-u.txt`: no move there). The row is
  `* version: <build>` on both sides, attributes only: xfx's version since
  main's format of 2026-11-10 is `0.2.0 (debug <date>-<rev>)`, fx's
  `0.0.16`; the ledger's `product-version` reads both texts as `<build>`
  (its `why` names that format) and the colour that spans the version is
  as wide as each product's text. Both units built `0.1.0`, the base and
  the head `0.2.0` (`sweep-runs.txt`, each tree's `Cargo.toml`). Main's,
  not the train's.
- **Up**: `p5-input-at` 3 -> 7 of 7 against the four units under AT-RANK
  (five sweeps; AT-RANK's ranking is above them in the stack, its four
  snaps above); against AT-FOLDERS, `p5-input-model`, `p5-input-paste` and
  `p5-input-question`, all passing at the head (AT-FOLDERS is the stack's
  oldest; everything since its base is in the head); against
  CTRLO-OTHER-PICKERS, `model-anchor` and `p5-mcp-menu-new`; against
  PAGE-KEYS, `model-column-idle` 0 -> 3 of 3. Four scenarios have more pairs at the
  head than in the unit's sweep (`model-column-reask` 3 -> 4,
  `p5-ctrlo-fork` 7 -> 9, `p5-ctrlo-help` 7 -> 9, `provider-ctrlo` 5 -> 7)
  and pass no fewer.

**So no pair that passes on the base fails at the head but
`workspace-index` `at-plan`, main's own flip (WORKSPACE-INDEX-AT-PLAN's,
above), which passes 3 of 3 on the rerun at both trees**, and
against the units' own sweeps the pairs lower at the head are lower at the
base too: main's version line. Every other move is a unit's change, or
run-local text.

## Decisions, for the user's veto

Each with its revert.

- **C-j/C-k over `/mcp`'s menu under the full transcript are the
  composer's** (merges 2 and 3, `594fddc8`). CTRLO-OTHER-PICKERS had them
  swallowed by the menu (its check after `offer_to_mcp`), PAGE-KEYS gave them
  to the composer (its check before it); the merged tree keeps PAGE-KEYS'
  place, fx's order (join check 3). **Revert**: move the one check below
  `offer_to_mcp`, and PAGE-KEYS' `/mcp`-menu step in
  `under_the_full_transcript_no_other_list_takes_a_row_key` with it.
- **The `@` list takes neither key under the screen** (merge 3's
  behaviour fix): CTRLO-AT-KEYS' rule, which PAGE-KEYS' comment defers to.
  **Revert**: none that keeps CTRLO-AT-KEYS' cases; putting the
  `offer_to_files` call back fails both (`red3.txt`).
- **C-j over `/model`'s menu under the screen adds nothing**
  (`725b3c90`): fx's `insert_newline` (`:514`), CTRLO-OTHER-PICKERS'
  `composer_newline`; PAGE-KEYS' case expected a newline. **Revert**: drop
  the `/model` menu from `composer_newline`'s `held`, and the case's `held`
  with it.
- **One anchor function** (merge 3, `byhand.py`): CTRLO-OTHER-PICKERS'
  `anchor_at` with the caret-row test inline, PAGE-KEYS' `column_anchor`
  not restored; the two are the same text. **Revert**: put back
  `column_anchor` and route `anchor_at` through it, as PAGE-KEYS had it.
- **The `/settings` row's dropped sentence** (merge 3, `rowsmerge.py
  --drop`). **Revert**: run merge 3's rows without `--drop`; the row then
  stops, to be read by hand.
- **The card's frame gets the tick's `frame_held`** (merge 1,
  `eventloop.py`), not `false`; equal in every reachable tick (above).
  **Revert**: `false`.
- **`at-plan-run-directory` and `at-ranking-anchor` deleted** (join check
  5; the first as the brief has it). **Revert**: `git revert 8f622949
  16fff02d`. `at-plan-run-directory`'s rows cannot match the placeholder
  row, so it covers nothing back (join check 5; `anchor.txt` ran at
  `04e4491d`, after its deletion, and shows the pair passing once without
  it); `at-ranking-anchor`'s snap passes without it (`anchor.txt`, with the
  cause still in the ledger). What `at-plan-run-directory` was written for
  is still there: `workspace-index` `at-plan` flips on the run directories'
  random names, on main as here ("Sweeps"). **Its owner is
  WORKSPACE-INDEX-AT-PLAN** (one run path for both sides), merged as PR
  #94 and joined here in `583cc13a`; the ledger comment where the cause
  stood (`scripts/fxcap/parity-ledger.toml:664-672`, which called the snap
  identical) is rewritten in `42935f45`, the same nine lines.
- **Every WRONG reference re-pointed**, the 35 on main and the 2 on
  IMAGE-LOSS' head too, not only the 43 the merges moved (join check 6);
  and the two bare continuations `refs_bare.py` read MOVED (`b3dffdad`),
  one of them wrong on main already. **Revert**: put back the 37's numbers
  (`repoint-plan.txt`'s `main` and `image-loss` lines), and `src/tui/shell.rs:24980`'s
  `:6384` to `:6272`.
- **A run name in two units' receipts written as a placeholder**
  (`09b13511`): the brief's check over the range must read 0. **Revert**:
  `git revert 09b13511`; the range then reads 9.
- **The sweeps** (`selection.py`, `selection_units.py`): the brief's globs,
  every scenario named by a ledger entry a merge or a ledger commit changed,
  every scenario a merge added, the scenarios the re-point's entries name
  (train 13's extra), and every scenario the units' own head sweeps took;
  `session-record`'s and `stats-bytes`' `*` left out. Both of the head's
  sweeps ran at the code head `b3dffdad` (`sweep-runs.txt`, each
  `run.json`'s `repo_head`). **Revert**: a sweep of every scenario at the
  head and the base.
- **`check-merge.sh` without the suite lock**, as train 13. **Revert**:
  run it under the lock.
- **No join test.** Each meeting is reached by a unit's own cases (join
  checks 1 to 4 name them). **Revert**: a test per meeting.

## Found on the way, not changed here

- **`src/tools/question.rs:301-305`** says "this crate does not enable
  serde_json's `preserve_order` (`Cargo.toml:42`)", and `Cargo.toml:46`
  enables it (`serde_json = { version = "1.0", features = ["preserve_order"] }`).
  The claim is out of date, and at the head the number is too: `:42` is
  `"http2",` (`content-head.txt`; main's `:42` was serde_json's,
  `content-dcfa5146.txt`). Both are
  src after the code head: the re-point, and whether the struct it explains
  is still needed, are queued as TRAIN14-FOLLOWUPS.
- **`src/tui/shell.rs:24979-24980`** says a question's too-small refusal is
  read from `released()` "how the existing too-small assertions read it
  (`shell.rs:6178`, `:6384`)". Neither line is a too-small assertion: at
  main the pair (`:6084`, `:6272`) is the `true` that ends `use_skill` and
  `offer_to_settings`' `let mut repaint = false;`, and the two re-points
  moved each with its own text. Where the pair was written (`866de4f4`,
  as `:5816`, `:6004`), they named a `#[test]` line and a blank one there. The one such assertion at the head is
  `:23758-23762`, in `a_screen_too_small_to_show_the_question_refuses_it_and_says_so`;
  which two the comment meant is its writer's.
- **At a 49-byte TMPDIR three `workspace` pairs fail, on main as at the
  head** (join check 1; row 27 of each in `joins.txt`): the picker cuts the
  added folder's path with `…` before the run directory ends, so `roots.py`
  finds only the capture's TMPDIR root and leaves the cut `fxcap.` name's
  first letters, `/fx…`, which the ledger's `product-name` normalization
  then reads as `xfx` on fx's side. **Whether CI's sweep would meet it is
  not known.** 49 bytes is CI's TMPDIR, but CI's sweep runs under
  `--tmp "$RUNNER_TEMP"` (`.github/workflows/sweep.yml:167`), and
  WORKSPACE-INDEX-AT-PLAN's receipt reads RUNNER_TEMP from CI's log as 24
  bytes, which makes CI's run directory 55 wide (its §2,
  `ci/runner-temp.txt`). Here the run directory is 62 wide at the 24-byte
  TMPDIR, where the folder stands whole and the three pass, and at 49 bytes
  the capture's TMPDIR alone is 74 (`joins.txt`). No CI sweep of these
  three was read for this train. Whose it is (the normalization inside a
  path, or the root reading a cut path) is for the dispatcher.
- **The content check reads a bare `:N` on its path's own line only**
  (`refs_content.py`, "on the same line"), so a continuation on the next
  line is `refs_bare.py`'s alone; that is how `b3dffdad`'s two were found.
- **`refs_content.py` reads `tests/session_compat.rs:57` WRONG** although
  it names commit `14ad04202fe5`'s file correctly: its pattern has no form
  for a commit written before the path.

## Round 2: the reviews of `1a6278e8`

Two gpt6 reviews and one opus review; no MUST. This round is receipt text
only (this folder), so the code head stays `b3dffdad` and no gate is run
again (`final-scan.txt` part 4: every commit after it touches this folder
alone).
- **opus S1**: the sentence naming `6bc758a0` and `3a2830fb` as the head
  sweeps' revisions is gone; both ran at `b3dffdad` ("Decisions").
- **S2**: the scans name both revisions: `final-scan.txt` reads through the
  receipt's second-to-last commit, the last stores it alone (the opening,
  the commit table, "Redaction").
- **S3**: the `at-plan` flip's mechanism, traced by WORKSPACE-INDEX-AT-PLAN
  on main's code too, and that unit as its owner ("Sweeps", join check 5,
  "Decisions"). The ledger comment at `parity-ledger.toml:664-672` was left
  for that unit's PR in round 2; after the join it is rewritten here
  (`42935f45`, "The join of main").
- **S4**: row 27 of the three `workspace` pairs, both sides, both trees,
  both lengths, is in `joins.txt` (`joins_txt.py`); the claim that CI's
  sweep would read them unresolved is withdrawn: CI's sweep runs under
  `$RUNNER_TEMP`, its run directory 55 wide (join check 1, "Found on the
  way").
- **S5**: join check 2 names AT-LAUNCH-WALK's three launch-walk cases as
  the evidence the five sweep runs cannot give (`named.sh`, `named.txt`).
- **NIT 1**: `question.rs`' `Cargo.toml:42` is wrong at the head in number
  too. **NIT 4**: of the 790 references 289 are checked. **NIT 5**:
  `input_row_limit` has 20 call lines (`pages.sh` reads every call now).
  **NIT 6**: the unkept first `check-merge.sh` run's time is dropped.
- **Not here, queued as TRAIN14-FOLLOWUPS** (src or tests after the code
  head): `question.rs:303`'s re-point, the helper's mark assert after C-j
  when `held`, `session_compat.rs:57`'s pin, `shell.rs:24979-24980`'s pair,
  and `tests/tui.rs:2600`'s list.

## The join of main (`join-main/`, `resolutions.txt`)

PR #93's CI Sweep at `08066ac9` read one regression, `workspace-index`
`at-plan`; `ci-08066ac9/` shows it is the flip WORKSPACE-INDEX-AT-PLAN
traced on main (144 of 144 sides list the folder exactly when their own
path holds `p`, `l`, `a`, `n` in order, which the random run-directory
names decided, on the head and on the base alike). That unit merged as PR #94 (main `3570a182`), and
the train takes main in:

- **`583cc13a`, an ordinary merge** of `3570a182` into `08066ac9` (no
  rebase). main brings `scripts/fxcap/` (`capture.sh`, `sweep.py`,
  `roots.py`, their tests, the ledger) and WORKSPACE-INDEX-AT-PLAN's
  receipt; no `src`. **One conflicting file, the ledger, three blocks,
  each resolved by `blocks.py`** (`resolutions.txt`, its fifth merge):
  `run-directory`'s `why` and `source` by `rewrite` (main's new text and
  its two refs; ours had moved only `:681 -> :685`); `capture-tmpdir`'s and
  `tmux-dead-pane`'s `source` by `numbers` (the five `capture.sh` numbers
  both sides moved keep ours'). `reproduce.sh` redoes the join to the
  same tree, as it does the four merges.
- **The keep-check** (`join-main/token-check.txt`): `token_check.py`
  (train 7's, byte for byte) over the join, **OK, exit 0** -- every parity
  row and ledger entry either side changed is in the merge, the three
  conflicted entries read as above, the ledger's 11 comment lines added as
  expected. Its self-test finds 24 of 25 planted cases; the one it misses,
  "a numbers-only entry with a word changed", plants nothing: it writes
  `owner = "X`, and a `[[normalization]]` has no `owner`. `word_control.py`
  plants the word in `why` instead, on both numbers-only entries: **2 of 2
  found**. `token_control.py`: no `docs/parity.md` row both sides changed.
- **`3e073a96`, the seven references the join moved** (the content check
  at the join, `join-main/content-583cc13a.txt`: 9 WRONG, the seven and the
  two left on purpose): run-directory's `capture.sh:442-474` and `:818`
  (main's) to `:446-478` and `:822`; capture-tmpdir's `:330-332`,
  `:383-389`, `:984`, `:991` (ours) to `:333-335`, `:386-392`, `:1121`,
  `:1128`; tmux-dead-pane's `:1161` to `:1301`. Each line's text at the
  head is in `join-main/repointed.txt`; 3 lines out, 3 in, the same once
  numbers are masked. `:386-392` is the paragraph main rewrote two lines
  of; main's own number for it, `:382-388`, moved by the train's four
  `homemode` comment lines, agrees.
- **After it** (`join-main/content-42935f45.txt`, `refs-*-head.txt`, at
  the code head against main `3570a182` and the train's `08066ac9`): the
  content check **2 WRONG**, the two left on purpose, 791 references;
  `refs.py` 0 MOVED; outside the historical plans `refs_all.py`,
  `refs_short.py`, `refs_bare.py` 0; `refs_prd.py` 0.
- **`42935f45`, the ledger's at-plan comment** (`parity-ledger.toml:664-672`,
  comments only): it said AT-RANK made all five of its snaps identical,
  at-plan among them; it now says at-plan was identical only when both
  runs' names agreed, with `ci-08066ac9/read.txt` and WORKSPACE-INDEX-AT-PLAN's
  receipt, and that since that unit a sweep unit's two runs share one name.
  Written in the same nine lines, so no later ledger line moves.
- **The checks at `42935f45`** (`join-main/checks.sh`, `checks.txt`, its
  first lines the code since `b3dffdad`: the join's six harness files; one
  hold of the suite lock; no `src` changed since the gate's `b3dffdad`, so
  not the full gate), in a `git clone --no-local` at `42935f45`, tree
  clean, **each exit 0**: `cargo test --locked`, 5854 passed, 0 failed, 7
  ignored over 15 binaries, the gate's plain count at `b3dffdad`
  (`gate-counts.txt`); `scripts/fxcap/test-sweep.sh`, 79 and 28 tests OK
  (the gate's 76 and 28: `test_sweep.py`'s three new tests from main);
  `scripts/fxcap/test-capture.sh`, 130 tests OK (the gate's 109:
  `test_capture.py`'s 21 new tests from main).
- **`workspace-index` at the joined head** (the same hold, then
  `checks_l.sh` in another; `checks.txt`, read by `checks_txt.py`): the
  scenario as the Sweep selects it (`--only workspace-index`), three runs
  at a 24-byte TMPDIR and three at a 49-byte one, each directly under the
  system's `/private/tmp` and, like CI's RUNNER_TEMP, holding no `l`; then
  its twelve copies at each length (`rep24`, `rep49`); then twelve at a
  24-byte TMPDIR holding an `l` (`rep24l`). **`at-plan` passes in 42 of 42
  units** (normalized-identical), `at-all` in 42 of 42, fx's sha256 the pin
  before and after every sweep. Since WORKSPACE-INDEX-AT-PLAN the sweep's
  root and every run directory are named by digits (`sweep.py`
  `digit_dir`, `capture.sh` `make_run`) and a unit's two runs share one
  (FXCAP_RUN), so row 13 follows the TMPDIR alone and is the same on both
  sides: with no `l` the folder is listed on neither side in all 30 units,
  with one on both sides in all 12, and row 13 is the same text and the
  same attributes (its `.ansi` row) on both sides in all 42.

## Redaction

The repository is public. Every file here taken from a gate log, a job, the
logger, a sweep, a capture or a clone went through `redacted.py` (train
13's, unchanged but for its first line): `scripts/fxcap/redact.py`'s
`Redactor.write`, every path under a local root to `<local-path>` or a
named root (`<scratch>`, `<repo>`, `<smoke>`, `<gate-logs>`,
`<sweep-head>`, ...), a checkout's path cut short to `<checkout>/`, the
account's name to `<user>`, and train 12's tail rule (a token holding a run
directory's name or the harness root's uid part becomes `<local-path>`
whole). The scripts take the oracle's path, the suite lock and the scratch
directories as arguments or variables, and spell the brief's pattern with
bracket classes. **Every commit this train adds was gated on the brief's
check** before it was made (`pathcheck.sh`, under `set -o pipefail` and an
explicit `if`): a merge as the combined diff `git show` prints of it, read
from a probe commit of the staged tree (`git commit-tree`, on no branch),
any other commit as its staged lines; and `shapes.py`'s three shapes over
the staged lines and over every file of this folder. `final-scan.sh`
through the receipt's second-to-last commit, the one that stores
`gates-files.txt` (`final-scan.txt` names it; the last commit stores
`final-scan.txt` alone), and the file checks with the receipt in its
final form but those two files (`gates-files.sh`, `gates-files.txt`, which
names the commit it read).

## Files

- Merges: `rowsmerge.py` (its `--drop` new), `blocks.py` (its `numbers`
  rule per token), `eventloop.py`, `byhand.py`, `semantic.py` (new),
  `reproduce.sh`, `resolutions.txt`, `check-merge.sh` / `check-merges.txt`,
  `merge_ends.py` / `merge-ends.txt`, `remerge-1.txt` .. `remerge-4.txt`,
  `overlap.py` / `overlap.txt`; `red3.sh` / `red3.txt`; `pathcheck.sh`,
  `shapes.py`, `carried.py` (the brief's check before every commit).
- `merge-checks.sh`: writes `check-merges.txt`, `resolutions.txt`,
  `merge-ends.txt`, the token check and its control, the remerge-diffs, the
  reference scans, the content checks, the re-point plan and its check,
  `shared.txt`, `functions.txt`, `overlap.txt`, `test-counts.txt`.
- `token_check.py` (train 7's, byte for byte), `token_control.py`,
  `token-check.txt`, `token-self-test.txt`.
- References: `refs.py`, `refs_all.py`, `refs_short.py`, `refs_bare.py`,
  `refs_prd.py` (trains 7's to 13's); `refs-*-pre.txt` (at `8f622949`),
  `refs-*-head.txt` (at `b3dffdad`), `refs-bare-3a2830fb.txt`;
  `refs_content.py`, `content.sh` (new) and `content-*.txt`; `repoint.py`,
  `repoint-plan.txt`, `repointed.py`, `repointed.txt`.
- Join checks: `joins.sh`, `joins_txt.py`, `joins-txt.sh` / `joins.txt`,
  `walk_rows.py`; `functions.py` /
  `functions.txt`, `shared.py` / `shared.txt`; `pages.sh` / `pages.txt`;
  `anchor.sh` / `anchor.txt`; `named.sh`, `named.py` / `named.txt`;
  `jobs.sh` (part `net`), `net.py`, `jobs-net.txt`, `connect.log`.
- Gate: `gates.sh`, `gates_txt.py`, `gates.txt`, `gate-6bc758a0.txt`,
  `gate_counts.sh`, `gate-counts.txt`; `test_counts.sh`, `test-counts.txt`;
  `gates-files.sh`, `gates-files.txt`; `main-gate.sh`, `main-gate.txt`.
- Sweeps: `selection.py`, `selection_units.py`, `selections.sh`,
  `selection-all.txt`, `selection-units.txt`, `selections.txt`;
  `sweep.sh`, `rescore.sh`, `sweep-runs.sh` / `sweep-runs.txt`,
  `sweep-*.md`; `sweep-compare.sh`, `compare.py`, `screen_moves.py`,
  `screen_classes.py`, `screen_partition.py`, `gained_rows.py`,
  `new_scenarios.py`, `banners.py`, `unit_compare.py`, `compare-*.txt`,
  `screen-moves-*.txt`, `screen-classes*.txt`, `screen-partition.txt`,
  `gained-rows*.txt`, `new-scenarios.txt`, `compare-units.txt`;
  `sweep-rerun.sh`, `reruns.sh`, `rerun_verdicts.py`, `reruns_txt.sh`,
  `reruns.txt`.
- `final-scan.sh`, `midbranch_scan.py`, `final-scan.txt`; `redacted.py`.
- PR #93's CI Sweep red at `08066ac9`, read: `ci-08066ac9/` (its own
  README names its files).
- The join of main: `join-main/` -- `merge-checks.sh` (writes
  `token-check.txt`, `content-583cc13a.txt`, `content-42935f45.txt`,
  `refs-head.txt`, `refs-all-head.txt`, `refs-short-head.txt`,
  `refs-bare-head.txt`, `refs-prd-head.txt`, `repointed.txt`),
  `word_control.py`; `checks.sh`, `checks_l.sh`, `checks_txt.py`,
  `checks-txt.sh` / `checks.txt`;
  `reproduce.sh`'s fifth merge and `resolutions.txt`.
