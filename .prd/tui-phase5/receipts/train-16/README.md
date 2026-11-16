---
title: 0.0.16 delta -- P5 train 16 receipt (the profile and slash lines, TEST-ENV-ISOLATION, LOCAL-PATHS-WRAP, APPROVAL-PASTE-SPACES and META-PREFIX-DECODE into one branch, on train 15)
type: receipt
tags: [xfx, phase5, fx-0.0.16, train]
---

# P5 train 16: eleven closed units, six merges, one branch

**Round 6** (round 5's review, approved: "Round 6" below): the clean
copy's doc says what a parent with no `PATH` gets (`bbb2c109`), and `CARGO`
is a compile-time name only `tests/support/isolated.rs` may read and only
integration test crates may compile with (`23819f7a`), with two self-test
fixtures, RED then GREEN; no product line; **the code head is
`23819f7a`**; Linux clippy and the gate both ways ran there.

**Round 5** (PR #102's CI: "Round 5" below): three causes, each the
runners' and not the code paths' -- a Linux-only clippy lint, an
integration test's `cargo` under rustup with the clean copy's empty
`HOME`, and the env check's self-test under a `TMPDIR` reached through a
link -- reproduced here, fixed in `ac5882fb`, `3ed1c082` and `96cbaab2`,
RED then GREEN; round 5's code head was `3ed1c082`; the gate ran there twice,
as before and through rustup's proxies; one product line, the same value.

**Round 3** (the join of main, "The join of main" below): main moved to
`e0aedbf6` (#101, #99, #100; no `src/`, no `tests/`), merged in `75d1318c`
with one conflict in ci.yml, and a docs-only join commit `a4452753`, round
3's code head; the gate ran again there, every CI step of the joined
ci.yml on the macos-15 row's terms.

**Round 2** (the reviews of round 1's head `7d63b764`: `gpt6-merges-1-3`,
`gpt6-merges-4-6-joins`, `opus`): round 2's code head was **`6137bd3d`**,
three commits on `2b78aab6`, every one tests or comments
(`round2-numstat.txt`: product code 0 lines out, 0 in). Opus's MUST (a comma
list's ten stale numbers) is re-pointed in `adb809f5` and the scans read
comma lists now (join check 4); opus's S1 and gpt6's SHOULD are two join
cases (join check 2); opus's S2 is the network section's code path and two
new controls (join check 5); the NITs and NTHs are fixed where they stand.
The gate, the mutants, the network job and the merge checks ran again at
`6137bd3d`; the sweeps stand (Sweeps).

Branch `train/p5-1116`, from **train 15's head `866747ff`** (`train/p5-1111b`,
reviewed and gated, waiting only for its PR), as the brief set it: train 16
stacks on train 15 because APPROVAL-PASTE-SPACES and META-PREFIX-DECODE sit on
the decoder line train 15 brings. Each unit was closed by its own two-engine
review and the dispatcher's gate; this train combines them. The program goal
(the user's words): «최신 fx랑 비교해서 ui쪽으로 모든 기능 구현하고
기본적으로 최신 fx ui를 그대로 구현해줘» (fx v0.0.16, pin `481104fd`).

Oracle: fx v0.0.16, executable sha256
`8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked
by each sweep and capture before and after it ran, `FX_AUTO_UPGRADE=0`.

Model: train 15's receipt (`../train-15/`) and the dispatcher's reviews of
it. Its scripts are copied here, each saying in its first lines what changed
for this train (most: nothing but that line); the new ones say so too.
Train 15's scripts for checks this train has no use for are not copied
(`cw_runs.py`, `phrases.sh`, `red-r2.sh`, `rerun-job.sh`: the reruns ran
on the sweeps' own builds); train 14's
`semantic.py` is back, for merge 3. What those reviews asked is carried:
every figure rests on a file of this folder, named where it is used; counts
are checked against the lists they count; the path scans run over this
folder (`pathcheck.sh` before every commit, `final-scan.sh`); "no code after
the code head" is shown by `final-scan.txt` part 4 at the receipt's last
commit; the doc and ledger merges are checked mechanically (the token check
and its controls, `merge_keep.py`, `owners.py`, `parity-join.txt`). Every
log this receipt cites was copied into this folder as it was made; the
scratch root held only raw bulk (the 2026-11-16 lesson: macOS's periodic
`/tmp` cleanup).

## Units and merges

Each merge is a stack top and carries the units under it; every base is
git's merge base of the merge's two parents (`merge-ends.txt` names each).
The chains are `git log`'s (`prescan.txt` counts each range):

| # | merge | units it carries (branch @ tip; commits since its base) | base | conflicts |
|---|---|---|---|---|
| 1 | `b2d044c9` | the profile line, oldest first: PROFILE-REFUSE-AT-LAUNCH `0a334851` (13), PROFILE-REFUSE-TYPED-KEYS `9c64ddd3` (15), **PERMISSION-MODE-CASE `f74d2fd5` (9)**, PROFILE-JSON-LIMITS `bacb1052` (12) (`fix/p5-profile-json-limits`; 49) | `3943d667` | `docs/parity.md` 4 blocks, 13 rows; the ledger 1 and `p5-panels-badsettings.cap` 1, `rewrite`; `tests/sessions.rs` 1 `numbers`; `tests/tui.rs` 2, `numbers` and `ours` |
| 2 | `e8b004a5` | the slash line, oldest first: SLASH-LEADING-WS `4bb8af77` (13), SLASH-ARG-SKILLS `b9b11596` (12), SLASH-MIXED-SKILLS `32eb858f` (11) (`fix/p5-slash-mixed-skills`; 36) | `afd891af` | `src/tui/shell.rs` 1, by hand; the ledger 13 (12 `numbers`, 1 `ours`); `docs/parity.md` 3 blocks, 17 rows |
| 3 | `849f5017` | TEST-ENV-ISOLATION (`fix/p5-test-env-isolation` @ `7c3f6c1e`; 51) | `32afd307` | `CONTRIBUTING.md` 1, `src/tui/worker.rs` 1, `tests/support/mod.rs` 1, by hand; **and 9 semantic fixes** (`semantic.py`: the merged tree failed fmt and both clippy runs) |
| 4 | `dd2ac93d` | LOCAL-PATHS-WRAP (`fix/p5-local-paths-wrap` @ `dd9f5f0a`; 14, one of them its own join of `71d3e02b`) | `71d3e02b` | `scripts/fxcap/roots.py` 1, by hand |
| 5 | `71787971` | APPROVAL-PASTE-SPACES (`fix/p5-approval-paste-spaces` @ `ca496c10`; 21) | `df45025c` | `src/tui/approval_amendment.rs` 1, by hand; the ledger 6 `numbers`; `docs/parity.md` 1 block, 5 rows |
| 6 | `1bce5348` | META-PREFIX-DECODE (`fix/p5-meta-prefix-decode` @ `6d94fbc0`; 18) | `df45025c` | the ledger 6 `numbers`; `docs/parity.md` 2 blocks, 7 rows, two of ours' words dropped (`--drop`) |

The counts are `git rev-list --count <from>..<to>`: the profile line's 13 +
15 + 9 + 12 = 49, the slash line's 13 + 12 + 11 = 36; 189 commits in the six
ranges (`prescan.txt`). **PERMISSION-MODE-CASE is on the profile line**
(`git merge-base --is-ancestor f74d2fd5 bacb1052` yes, `... 32eb858f` no),
not on the slash line where the brief listed it; merge 1 brought it. Not
merged, as the brief has it: COMPACT-MENU-HELD-KEYS, PROFILE-LAYER-DROP,
CI-PTY-WAIT, LOCAL-PATHS-EXIT-TRAP and every open unit.

After the merges, in order (`git log --first-parent 866747ff..`):

| commit | what |
|---|---|
| `622b2865` | **the join commit, docs and tests**: join check 1's MUSTs 2-4 (MUST 1 is merge 3's), join check 2's three join tests, the stale owner clauses, the `/permissions full` scenario's xfx timing |
| `c637ed4c` | the 42 references the merges and TEST-ENV-ISOLATION moved, re-pointed, numbers only (join check 4) |
| `15c9744b` | four more the content check reads SAME, read by hand (refs.py, refs_bare.py) |
| `3d1376b2` | `ci.yml`'s `script_tests` comment, comment only |
| `2b78aab6` | one of `c637ed4c`'s 42, a pinned reference, put back: round 1's code head, where the sweeps ran |
| `adb809f5` | round 2: the ten numbers after the first in `tests/tui.rs:2721`'s comma list, re-pointed, numbers only (opus's MUST 1) |
| `1ddeda00` | round 2: two join rows, tests only -- the prefixed C-j at choice 2, and the meta prefix split across the pair's flush in an amendment -- put in the middle of `src/tui/shell.rs`' tests, which moved a cited line (`content-1ddeda00.txt`) |
| `6137bd3d` | round 2: the same two cases moved to the end of the tests module, so `src/tui/shell.rs` is one insertion before its last line against `2b78aab6` and no cited line moves; the choice-2 row its own case: round 2's code head, where the mutants and the network job ran |
| `75d1318c` | round 3: **the join of main** `e0aedbf6` (#101 CI-TOOLCHAIN-PIN, #99 LOCAL-PATHS-EXIT-TRAP, #100 train 15), `--no-ff`; ci.yml's one conflict by `byhand.py 7`, its toolchain step's name by `semantic.py 7` (The join of main) |
| `a4452753` | round 3: CONTRIBUTING.md's and README.md's gate lists name the five script tests, docs only: round 3's code head |
| `ac5882fb` | round 5: `open_to_others`' mask in `st_mode`'s own type (CI's Linux clippy) |
| `96cbaab2` | round 5: `check-env-reads.py` finds a file whichever way its root is spelt; its self-test reaches its fixtures through a link (CI's aarch64 macOS self-test) |
| `3ed1c082` | round 5: an integration test's clean copy puts the cargo that built it first on PATH (CI's x86_64 macOS Tests): round 5's code head |
| `bbb2c109` | round 6: the clean copy's doc says what a parent with no `PATH` gets (the reviewer's NTH), doc only |
| `23819f7a` | round 6: `CARGO` scoped to `tests/support/isolated.rs` and to integration test crates, in both checks, with two self-test fixtures: **the code head** both round-6 gates ran at |
| the receipt's commits | this folder only (`final-scan.txt` part 4) |

## The units' own commits, before any merge (`prescan.sh`, `prescan.txt`)

The brief's mid-branch check came first, since a real name or credential
stops the train: `midbranch_scan.py` (train 15's, with one optional
argument: where the documented homes are read, since TEST-ENV-ISOLATION's
tip predates `check-no-local-paths.sh`) over every commit of the six
ranges, each as `git show --format=` prints it (LOCAL-PATHS-WRAP's own merge
as its combined diff), and each range whole; and every commit message by the
brief's pattern. **189 commits: 0 lines hit, NAME 0, CREDENTIAL 0, no
message holding the pattern or the account's name**, in every range. There
was nothing to classify as a placeholder or a scratch root. The scan again
at the end, over every commit of `866747ff..` (the units' and this train's):
`final-scan.txt` part 2.

## The conflicts and their resolutions (`resolutions.txt`, `check-merges.txt`)

Every resolution is a script's, redone by `reproduce.sh` in a clone to each
merge commit's tree: **all six "no difference"**, each resolver's own lines
(one per block, row or fix) printed under its merge (`resolutions.txt`).
Before each commit the staged tree passed `cargo check --all-targets` with
and without `fault-injection`, `cargo fmt --check` and both clippy runs
(`check-merge.sh`, no lock, as trains 13 to 15; `check-merges.txt`; merge 3's
first run, which failed, kept beside its second). The tools: `blocks.py`,
`rowsmerge.py` (its `--drop` used at merge 6), `byhand.py` (this train's
table, five merges' blocks) and `semantic.py` (train 14's, its fixes this
train's: merge 3).

**Merge 1** (`b2d044c9`, the profile line onto train 15):
- The ledger, 1 block, and `scripts/fxcap/scenarios/p5-panels-badsettings.cap`,
  1 block: `rewrite` -- ours (train 15's re-point `da8e4c1a`) moved only
  `:3820-3837` to `:3828-3845`, in text PROFILE-REFUSE-AT-LAUNCH rewrote; its
  text is taken and its numbers re-pointed after (join check 4).
- `tests/sessions.rs` 1 and `tests/tui.rs` 2: `numbers` and `ours`.
- `docs/parity.md`, 4 blocks, 13 rows, token by token, none meeting.

**Merge 2** (`e8b004a5`, the slash line):
- `src/tui/shell.rs`, `Shell::offer_to_mcp_args`' doc, 1 block (`byhand.py
  2`): ours (PAGE-KEYS-SETTINGS-COLUMN, train 14, `c4bfa8b1`) added PgUp and PgDn and the
  grown composer's rule; theirs (SLASH-ARG-SKILLS) made the completions
  "`/mcp `'s or a table's", Tab's space after a table's line and Escape the
  slash menu's dismissal. Both, the body below merged clean holding both.
- The ledger, 13 blocks: 12 `numbers`, 1 `ours`.
- `docs/parity.md`, 3 blocks, 17 rows, token by token, none meeting.

**Merge 3** (`849f5017`, TEST-ENV-ISOLATION, built on main #88):
- `CONTRIBUTING.md`, "The gate" (`byhand.py 3`): the script tests CI runs
  on one row per OS -- three on each side, ours' `test-check-no-local-paths.sh`
  and theirs' `check-env-reads.py --self-test` -- four now, in theirs'
  wording ("the OS's own tools"); `ci.yml` merged clean and runs all four.
- `src/tui/worker.rs`, `recording()`: ours' `Runtime::with_environment`
  over the test's own home (CATALOG-REFRESH `44d7f84c`, the `env` above it
  merged clean), not theirs' `a_runtime(..)`, which builds the same `env`
  and would leave ours' unused; neither reads the process.
- `tests/support/mod.rs`: both added a module where the base has none;
  theirs' `isolated`, then ours' `profiles` with its comment -- rustfmt's
  order (ours then theirs fails `cargo fmt --check`: `check-merges.txt`,
  `m3-first`).
- **The merged tree did not build its lints** (`check-merges.txt`,
  `m3-first`: fmt 1, both clippy runs 101). The unit's `clippy.toml` refuses
  `std::process::Command::new` and `std::env::var_os` but where an allow
  lets one through, every `tests/*.rs` now declares `mod support;`, and its
  `check-env-reads.py` holds every compile-time name to a list -- rules
  everything newer than #88 was written without. `semantic.py 3`, 9 fixes:
  - `tests/cli.rs` declared `#[path = "support/profiles.rs"] mod profiles;`
    (PROFILE-REFUSE-AT-LAUNCH) and now `mod support;` too, whose `mod.rs` the
    profile line gave `pub mod profiles;`: the file loaded twice
    (`clippy::duplicate_mod`). It takes `use support::profiles;`, as
    `tests/interactive.rs` does.
  - The profile line's three `mkfifo` children: `super::command` in
    `tests/support/profiles.rs` (the unit's one integration-test `Command`,
    as `tests/tool_loop.rs` makes its FIFO), `crate::process::command` in
    `src/config.rs`' two tests (as `src/image.rs`' and `src/tui/shell.rs`'
    tests make theirs).
  - `src/tui/file_index.rs`' @ benchmark (AT-RANK-PERF, train 14) read
    `XFX_AT_BENCH_TREE` with `std::env::var_os`: now `env_exception!`, the
    unit's way for a named variable a test is handed, the name allowed
    (ALLOWED_NAMES) and listed for that file (EXCEPTIONS). The check refused
    it with ALLOWED_NAMES missing (`envreads.txt`, run 1). Through
    `process_env!` a unit test reads its case's fake process, where the
    person's tree would never be.
  - **Join MUST 1**: `src/lib.rs` reads `env!("XFX_BUILD_ID")` (#96), which
    build.rs sets: COMPILE_TIME_NAMES gains it. build.rs no longer sets
    `XFX_BUILD_REVISION` (#96 folded the revision into the id), so that name
    leaves the list -- left, an `option_env!` of it would read the person's
    own environment at compile time -- and the build-script comment names
    what build.rs reads and runs now (`git_build_id`).
- Then the merged tree: `check-merge.sh` 0 failed (`check-merges.txt`,
  `m3`); `check-env-reads.py` OK (`envreads.txt`, run 2); **the whole
  `cargo test --all-targets`, 0 failed** (`cm/m3-test.txt`): no test from
  trains 13 to 15 or the profile and slash lines reads the process where
  the unit's guard would refuse it.

**Merge 4** (`dd2ac93d`, LOCAL-PATHS-WRAP): `scripts/fxcap/roots.py`'s
docstring, 1 block (`byhand.py 4`): theirs' two new paragraphs, with ours'
(#94, WORKSPACE-INDEX-AT-PLAN) words for the run directory's name -- `fxcap.`
and six digits, where it had been mktemp's tail -- and its example, made in
the sentence theirs moved. `survivors` reads the name it is given, either
shape; `test_capture.py RootsRewrite` 29 OK on the staged tree
(`cm/m4-roots.txt`).

**Merge 5** (`71787971`, APPROVAL-PASTE-SPACES, on the decoder line):
- `src/tui/approval_amendment.rs`, `Draft::apply` (`byhand.py 5`): ours
  (train 15's APPROVAL-DRAFT-KEYS) took the draft's history out -- no
  `Undo`, `Redo` or `Yank` -- so `apply` is one `Editor::apply` with its
  delta dropped; theirs, written before that stack, kept the history's arms
  and added Home and every line-start spelling to the draft's start, End's
  to its end. Merged: ours' function, theirs' two arms a mapping ahead of
  the one call, theirs' comment less "A move records no delta".
- The ledger, 6 blocks, `numbers`; `docs/parity.md`, 1 block, 5 rows.

**Merge 6** (`1bce5348`, META-PREFIX-DECODE, on the decoder line):
- The ledger, 6 blocks, `numbers`.
- `docs/parity.md`, 2 blocks, 7 rows. In two rows ours' edit lay inside
  theirs': train 15 had turned "(proposed)" to "(queued)" in the owner
  clauses META-PREFIX-DECODE was named in -- `ask_user_question`'s
  "Owner unit: **META-PREFIX-DECODE** (proposed), since xfx's decoder has
  no meta prefix ...", and the full-screen TUI row's "(owner
  **META-PREFIX-DECODE**, proposed: the decoder has no meta prefix ...)" --
  and the unit, their owner, deleted the first and rewrote the second
  around its fix. `rowsmerge.py --drop` leaves ours' word out of each
  (`resolutions.txt`, merge 6's two lines).

## The token check (`token_check.py`, `token-check.txt`, `token-self-test.txt`, `merge-keep.txt`, `owners.txt`, `parity-join.txt`)

`token_check.py` is train 7's, byte for byte (`token-check.txt`'s first
line, `cmp` against `../train-7/`): every `docs/parity.md` row and every
ledger entry of each merge read against its base and both sides. Over the
six merges it **exits 1, on four lines, two merges' two each, each read
here** (merges 1, 3, 4 and 5 OK):

1. **Merge 2, full-screen TUI row, "2 of 106 clauses not whole"**: two
   insertions at one place. Main (PAGE-KEYS-SETTINGS-COLUMN) wrote "... its
   argument completion are absent. The word has to start the draft: ...
   not fixed there)."; the slash line rewrote "absent." 's sentence and
   inserted its own after it ("Upstream's fuzzy matching and its categories
   are absent. The word may follow blanks, as upstream's does: ..."). The
   merge holds both, the slash line's first, so each side's clause has the
   other's text on one flank. The first is the sentence join MUST 2 drops
   (`622b2865`).
2. **Merge 2, the ledger's `amendment-card-after-stop`**, "both sides
   changed it differently": both moved its `shell.rs` numbers alone (the
   entry is one text on the base, both sides and the merge once every `:N`
   is masked); ours kept (`blocks.py`'s `ours`), re-pointed after (`:11127`
   at the head).
3. **Merge 6, `ask_user_question`, "missing `(queued),`"** and
4. **merge 6, full-screen TUI row, "missing `queued:`"**: the two words
   `--drop` left out, where META-PREFIX-DECODE deleted or rewrote the owner
   clauses around them (merge 6, above).

- **The negative controls** (`token-self-test.txt`). train 7's
  `--self-test`, as it is, on each merge: merges 3 and 4 42 of 42 found;
  merge 1 50 of 51; merges 2, 5 and 6 stopped by an exception (`StopIteration`,
  its line 606) -- two cases its planting cannot make on this train's texts
  ("Found on the way"). With
  the two plants fixed (`token_check16.py`, its check's output the same as
  `token_check.py`'s): **288 of 288 planted cases found** (51, 56, 42, 42, 47 and 50; no row skipped). `token_control.py` (train 13's)
  plants its two cases on the merged text of every row both sides changed:
  **38 of 38 found**. `word_control.py` plants a word in `why` of every
  cause entry both sides changed in numbers alone: **20 of 20 found** (10,
  5 and 5 at merges 2, 5 and 6; merges 1, 3 and 4 have no such entry, and
  the script exits 1 on "0 of 0", by its rule). A divergence entry has no
  `why` (it carries `reason`), and the script skips it without counting it
  (`word_control.py`'s `planted == m`): at merge 2 the divergence
  `answer-drawn-wider-named` (`token-check.txt`) got no word from it.
  `token_check16.py`'s self-test plants one in that same entry, and finds
  it (`token-self-test.txt`, "a numbers-only entry
  (answer-drawn-wider-named) with a word changed: found").
- **`merge_keep.py`** (the dispatcher's, from train 14's review): every unit
  (a cell, a sentence, a ledger line; 25 characters or more) new on either
  side, looked for in the merge. As written 23 are not there, 17 of them
  ledger lines whose numbers the other side moved (kept by `numbers` or
  `rewrite` and re-pointed after); with digits read as `N`, **6**: merge 2's
  two pairs of sentences each made one by token-merging -- the full-screen
  TUI row's "The three lists that took none ... page so too: ..." (train 15)
  with the slash line's "and the tables beside them" in its last item, and
  the `/mcp` row's "Up, Down, `C-j` and `C-k` move the mark, ... Tab writes
  the marked line, Escape closes it", into which PAGE-KEYS-SETTINGS-COLUMN
  put the page keys and SLASH-ARG-SKILLS its "-- and the slash menu, fx's one
  `.slash` kind -- while the draft begins with `/`" for "for the line"
  (each side's words whole in the merged sentence, read in the head's rows)
  -- and merge 6's two owner clauses META-PREFIX-DECODE retired.
  None else on any side.
- **Stale owner clauses, mechanically** (`owners.py`, `owners.txt`): every
  mention of the eleven units in `docs/parity.md`, the ledger and the
  tracked source at the code head, and whether `proposed`, `queued`,
  `owner` or `not fixed` stands within 80 characters: **one hit, and it is
  not stale** -- "Owner unit: **QUESTION-RESUME-CTRLO** (queued). **`ESC ESC
  [ D` leaves a choice alone, as fx's does** (META-PREFIX-DECODE)": the
  queued unit is QUESTION-RESUME-CTRLO. No unit of the eleven is named as
  the owner of an open difference.
- **The join commit's and the re-points' `docs/parity.md` edits, word by
  word** (`parity-join.sh`, `parity-join.txt`: `git diff --word-diff=plain`
  of each commit, the rows each changed, and the join's sentences as they
  stand: 5 of 5 found -- round 1's file ended in a Python `SyntaxError`
  there, gpt6's NTH, its code now `parity_sentences.py`).

## Join checks

Each with its evidence. **Three join tests are added** (join check 2, in
`622b2865`), where no unit's tests reach the meeting, and **two more in
round 2** (`6137bd3d`: the prefixed C-j at choice 2, the prefix split
across the pair's flush in a draft); every other meeting is
reached by the units' own tests, run in the gate (`unit-tests.txt`), and the
one join check 1 needs, `/permissions full`, by SLASH-ARG-SKILLS' own.

### 1. The join MUSTs decided at the train split (STATUS, 2026-11-11 05:33Z)

- **TEST-ENV-ISOLATION's COMPILE_TIME_NAMES gains `XFX_BUILD_ID`**: merge
  3's semantic fix (above); the env check OK at merge 3, at merge 6
  (`envreads.txt`) and in the gate. Beyond "gains": `XFX_BUILD_REVISION`,
  which build.rs no longer sets, leaves the list (a decision, below).
- **PAGE-KEYS-SETTINGS-COLUMN's "SLASH-LEADING-WS not fixed here"
  sentence** goes (`622b2865`; `parity-join.txt`): "The word has to start
  the draft: ... Uncaptured; owner SLASH-LEADING-WS (found by
  PAGE-KEYS-SETTINGS-COLUMN's review, not fixed there)." The slash line's
  own sentence stands in the same row: "The word may follow blanks, as
  upstream's does: ...". Merge 2 had put both in the row (the token check's
  merge-2 difference, below); the row now reads "... when it takes none.
  **Every line the composer submits is remembered ...**".
- **`p5-mode-case-full` moves to the slash line, with parity.md:96**
  (`/permissions`, line 96 still), and **PERMISSION-MODE-CASE's SHOULD 1
  meets SLASH-ARG-SKILLS** (its opus review: whichever lands second revises
  the row, attributes the two snaps, re-sweeps). Read in both receipts:
  PERMISSION-MODE-CASE recorded `/permissions full` as the usage in xfx,
  "xfx has no argument completion", the two `p5-mode-case-full` snaps under
  no cause; SLASH-ARG-SKILLS gives `/permissions ` fx's argument table, its
  Enter the marked row in place of what was typed, and pins `/permissions
  full` + Enter as full access
  (`an_argument_nobody_understands_is_the_usage_line_and_changes_nothing`,
  `fx-p5-slash-arg-skills-40x120-permissions-full-enter`). At the joined
  head:
  - The scenario as PERMISSION-MODE-CASE wrote it (its xfx side `wait 1`,
    written for no menu) read `typed` normalized-identical and `full`
    unresolved, one row: xfx's hint row still holding the full-access notice
    fx's had let go (`mode-case-full.txt`, run 1) -- the mode set on both.
  - `622b2865` gives both sides fx's `until shown` / `until gone Full
    access`, the scenario's comment says the menu is xfx's too, and the row's
    sentence now says `/permissions full` takes the menu's `full-access` on
    both: **both pairs normalized-identical** (`mode-case-full.txt`, run 2;
    and in the head sweep, below). No cause is needed, so none is written.
  - PERMISSION-MODE-CASE's README section 8 is that unit's record at its
    head and is left as it is; this receipt supersedes it.

### 2. APPROVAL-PASTE-SPACES x META-PREFIX-DECODE, and the decoder's other meetings (`mutants.txt`, `parity-join.txt`)

Both units sit on the decoder line apart, after train 15's approval stack
was written, so no unit's test reaches where they meet. fx, from both
units' `fx-src.txt` and `src/core/app/input_approval_runtime.zig:71-128@481104fd`:
a second `ESC` sets the meta prefix, which the arrow table reads beside
Alt's bit and no Home, End or tilde stage reads; an amendment takes Home,
End and `line_start`/`draft_start` as its edges (`cursorHome`/`cursorEnd`),
the word moves as its own, a paragraph move as nothing.

- **New test `a_meta_prefixed_key_in_an_amendment_is_its_alt_spelling_as_fxs`**:
  from `ab cd |ef`, one prefixed key in one read, then a letter: Home, End,
  SS3 Home and End, tilde Home and End, Ctrl-Home and -End and Super-Left to
  the draft's edges; Left and SS3 Left a word left, Right a word right; Up
  and Down nothing -- 14 spellings, and none of them the Escape that would
  close the draft.
- **And META-PREFIX-DECODE meets train 15's two C-j units**: fx reads a
  Ctrl+J report behind the prefix (`ESC ESC [106;5u`) as the byte -- the
  report carries the prefix, the Ctrl+letter rule ignores it
  (`escape_parser.zig:552-555`, `:686-691`, `:255-258@481104fd`), and so
  does xfx's decoder now (`kitty_key`). QUESTION-CTRL-J's matrix doc and
  `docs/parity.md` still called those spellings uncopied, owner
  META-PREFIX-DECODE (queued). **New tests
  `every_meta_prefixed_ctrl_j_report_answers_a_model_question_as_the_byte`**
  (every report of `ctrl_j_matrix`, 1024, behind the prefix, at a choice and
  in the field) and
  **`a_meta_prefixed_ctrl_j_report_answers_a_permission_question_as_the_byte`**
  (`CTRL_J`'s ten reports behind the prefix, at a choice and in a draft, on
  both planes); the matrix doc and the two rows say so (`parity-join.txt`).
- **Round 2, choice 2** (opus's S1), **new test
  `a_meta_prefixed_ctrl_j_report_at_choice_2_answers_always_as_the_byte`**:
  the same ten reports behind the prefix, after Down and the frame, on both
  planes, each answering choice 2, the session's `Always`, as Enter and a
  bare C-j there (`.submit` is `selectedDecision()`,
  `approval_decision.zig:141@481104fd`).
- **Round 2: the meta prefix split across the pair's flush in an amendment**
  (gpt6's SHOULD), **new test
  `a_meta_prefixed_key_split_by_the_pair_flush_in_an_amendment_cancels_as_fxs`**.
  From `ab cd |ef`, `ESC ESC` and `[ D` in two reads, the clock the case's
  own (each read fed at a given instant and settled first, as the loop
  does). At 29 ms the pair is still the prefix: the prefixed Left, a word
  left, and the answer `Apply once` with "ab zcd ef". At 30 ms the pair's
  flush is one Escape and the pair one lone ESC
  (`terminal_action_decoder.zig:174-183`, `main.zig:187@481104fd`;
  `Decoder::flush`); with the turn running, fx's Escape at an approval,
  amending or not, is `resolveEscape`'s cancel (`cancel_pending` is
  `hasActiveOperation`, `app_input_runtime.zig:575`; `:2689`,
  `:2712-2716`), and `cancelApprovalOperation` cancels the turn and clears
  the question (`input_approval_runtime.zig:401-430`). The case checks the
  same three things in xfx: **the turn's message is the cancel**
  (`TurnControl::Cancel`, one message, no answer after it), **no sentence
  is sent** (the draft goes with the question), and **the composer owns the
  prompt again**: `[ D` at 40 ms reads behind the lone ESC as Left there and
  `z` is its text. xfx agrees with fx at each step.
- **Where round 2's cases sit.** `1ddeda00` first put them in the middle of
  `src/tui/shell.rs`' tests module, and the content check there read 7 WRONG
  more: the ledger's `src/tui/shell.rs:50939`, cited seven times, moved +96
  (`content-1ddeda00.txt`). `6137bd3d` moves both to the module's end,
  which closes the file, and leaves the C-j join test as round 1 had it:
  against `2b78aab6` the file is one insertion of 116 lines before its last
  (`round2-numstat.txt`), so every line a reference names is where round 1
  checked it, and nothing is re-pointed for them.
- **Mutants** (`mutants.sh`, round 2: a `git clone --no-local` at the code
  head `6137bd3d`, 900 s each, the five join tests and their five
  neighbours each time; `mutants.txt`): the unmutated run passes all 10.
  MA (the second `ESC` no longer sets the prefix) and MB (the prefix not
  read as Alt's bit) fail the amendment test and the split case's 29 ms
  half. MC (the Ctrl+letter rule refuses a report behind the prefix)
  fails the three C-j join tests alone. MD fails the amendment test and its
  two amendment neighbours (MD: End in a draft to its start,
  APPROVAL-PASTE-SPACES' mapping as the merge made it). The two new mutants
  fail the split case alone,
  each at "the pair's Escape left the turn running": ME (the pair resolves
  only at the lone ESC's 50 ms, not at `PAIR_TIMEOUT`, so `[ D` at 40 ms is
  the prefixed Left) and MF (an Escape in an open draft no longer reaches
  the question, which stays up with its draft). **6 of 6 killed**, the
  clone clean after. (Round 1's run, at `622b2865`: MA to MD, 4 of 4.)
- `docs/parity.md`: the draft's keys are fx's with no difference left
  (APPROVAL-PASTE-SPACES closed the one train 15 left it, "a paste's line
  breaks and tabs kept"), and a meta-prefixed key in a draft is its Alt
  spelling (`parity-join.txt`).
- **The 30 ms pair stage** (META-PREFIX-DECODE round 3) is the decoder's
  alone, pinned there (`the_pairs_stage_is_30_ms_from_the_second_esc_and_keeps_one_esc`,
  `the_pair_resolves_at_fxs_30_ms_and_a_lone_esc_at_xfxs_50`, in
  `src/tui/input.rs`); no surface reads a byte or a clock, the amendment
  takes the decoded actions alone (`Draft::apply`, `approval::edits`). The
  first join test feeds each prefixed key in one read, inside the stage;
  round 1 named the split pair in a draft as driven by no test, and **round
  2's split case drives it** (above), at the shell, across the flush.
- **A grant path the join opens** (a decision, below): `ESC ESC [106;5u` at
  a permission prompt cancelled the turn before (the Escape first) and now
  answers the marked choice, as fx does: choice 1's `Apply once`, or
  choice 2's `Always` for the session (round 2's choice-2 case). Neither unit alone
  does this.

### 3. The profile line x the slash line x the rest: the shared files (`merge-ends.txt`, `shared.txt`, `functions.txt`, `moved-edits.txt`, `unit-tests.txt`)

- **Each merge from both ends, file by file** (`merge_ends.py`, every file
  both sides changed against the merge base: the lines ours -> merged
  removes and adds must be exactly base -> theirs', and theirs -> merged
  base -> ours'). 53 files both sides changed over the six merges (13, 4,
  26, 2, 4, 4); **11 of them disagree once every `:N` is masked, and so do
  2 files one side alone changed** (merge 3's `tests/support/profiles.rs`
  and `scripts/check-env-reads.py`, the semantic fixes' files:
  `merge-ends.txt`'s "one side's file differs from that side"), 13 in all
  (`merge_ends.py`'s per-merge "disagreeing with numbers masked", 1, 1, 7,
  1, 2 and 1, counts both kinds), and **each is a block
  this receipt resolves by hand, by row or by a semantic fix** -- merge 1's
  and merge 2's `docs/parity.md` (rows merged token by token make lines
  neither side has), merge 3's `CONTRIBUTING.md` and `src/tui/worker.rs`
  (by hand), `src/config.rs`, `src/tui/file_index.rs`, `tests/cli.rs`,
  `tests/support/profiles.rs` and `scripts/check-env-reads.py` (the semantic
  fixes; the last two one side's files), merge 4's `scripts/fxcap/roots.py`
  and merge 5's `src/tui/approval_amendment.rs` (by hand), merge 5's and
  merge 6's `docs/parity.md` (rows; merge 6's `--drop`). Every other file
  both changed holds both sides' edits exactly, the ledger's among them once
  numbers are masked; every file one side alone changed is that side's.
- **Every function two sides of a merge changed** (`shared.py` over every
  `.rs` file both sides changed: 36 (merge, file) pairs; `shared.txt`), read
  from both ends: 15 functions both sides changed; **4 not kept both ways,
  each a resolution named above** -- merge 1's
  `a_live_theme_notification_repaints_the_band_without_losing_the_draft`
  (`tests/tui.rs`, `blocks.py`'s `ours`: the profile line moved only a
  number in a comment main rewrote, re-pointed after), merge 3's
  `recording` (by hand: ours' line), merge 5's `Draft::apply` (by hand: a
  function theirs changed and ours rewrote) and merge 5's `Shell::decide`
  (the comment that moved, next).
- **The functions the join checks read** (`functions.txt`, each one's
  sha256 at every commit): the decoder at the head is META-PREFIX-DECODE's
  byte for byte (`after_escape`, `flush`, `kitty_key`, `prefixed`, from
  merge 6 on), so the meta prefix the join tests drive is the unit's, as
  gated; `Shell::consume_inputs` is the base's at every merge and at the
  head; `Shell::approval_action` changes once, at merge 5 (the comment,
  next); `Draft::apply` is merge 5's by-hand text from there on;
  `offer_to_mcp_args` merge 2's.
- **Merge 5's comment that moved**: APPROVAL-PASTE-SPACES rewrote a comment
  of the draft arm inside `Shell::decide`; train 15's RULE-CONFIRM-KEYS had
  moved that arm into `Shell::approval_action`. git applied the unit's
  edit to the moved text: its 3 lines out and 3 in are exactly the edits
  that take `approval_action` from merge 4 to merge 5, and `decide` at merge
  5 is merge 4's byte for byte (`moved_edits.py`, `moved-edits.txt`). That is
  why `shared.py` reads `decide` "not kept both ways".
- **Each unit's own tests on the head** (`unit-tests.sh`, `unit_tests.py`:
  each unit's tests added or changed in its range, read in the gate's two
  `cargo test` runs): **241 (unit, test) pairs, 236 ok in both runs, 0
  gone, 0 lost** (round 1: 238 and 233; round 2's `round2` range adds its
  two cases and the C-j join test it touched, all ok). The other 5 are TEST-ENV-ISOLATION's and have no `ok` in a
  run by design, each read (`unit-tests-notes.txt`): three `#[ignore]` roles
  a re-run binary plays (`sentinel_runner_*` twice, `a_case_marks_where_it_runs`),
  whose runners passed in both runs; `xfx_counts`, ignored, run by
  `scripts/drawn-wider/sweep.sh`; and
  `quitting_mid_stream_with_a_slow_ui_still_exits_and_still_publishes_the_log`,
  in `tests/tui.rs`' `#[cfg(feature = "fault-injection")] mod faults`, ok in
  the fault-injection run. (`unit_tests.py` read the step logs by `*-test.log`,
  which this gate's `05-check-env-reads-self-test.log` also matched; it now
  takes `<nn>-<step>.log` exactly.) The tests the head holds that a run has
  no line for (`run_absent.py`, `run-absent.txt`): 39 in the plain run, 2 in
  the fault-injection run, **each held by the base too** -- none of them this
  train's.
- The shared files the brief names: `src/tui/shell.rs` (merges 2, 3, 5 and
  6), the settings (`src/config.rs`, merges 1 and 3), slash parsing
  (`src/tui/mod.rs`, `src/interactive.rs`: merges 1, 2 and 3),
  `docs/parity.md` and the ledger (merges 1, 2, 5 and 6, the token check),
  each merged from both ends above and rechecked by its units' tests in the
  gate.

### 4. References (`refs*.py`, `content.sh`, `refs_content.py`, `repoint.py`, `repointed.py`)

**Round 2: the scans read comma lists** (opus's MUST). Round 1's "0 moved"
was false: `tests/tui.rs:2721` cites
`tests/tui.rs:1930,2191,2766,3113,3236,3928,7648,7722,8515,9216,9412`, the
merges moved all eleven, and `c637ed4c` moved the first alone, because
`refs_content.py` and `refs_bare.py` read `path:N` and `:N` and not a
list's `,M`. Both now read each `,M` (and `,M-K`) straight after a ref as a
ref into the same file (kind `list`; a list whose last number has `@` after
it is pinned and skipped), and `NUMS` masks a number with its list. Every
file below was made again with them, by `merge-checks.sh` at the code head
`6137bd3d`; round 1's figures, where they differ, are in brackets.

**Before any re-point**, at the join commit `622b2865` (`refs-*-pre.txt`,
seven sides: the base `866747ff` and the six tips): `refs.py` 17 (side,
reference) pairs MOVED; `refs_all.py` 55, `refs_short.py` 75,
`refs_bare.py` 99 [78] moved or changed on some side, of them 22, 5 and 20
[7] outside the historical plans (`docs/superpowers/plans/`); `refs_prd.py`
0. refs_bare's 13 more are the list's ten (each `base MOVED`, onto the head
numbers `adb809f5` wrote) and `event_loop.rs:8409`'s three (below).

**The content check over every reference** (`content.sh`, train 14's; each
reference read against the commit that wrote its numbers, the cited line and
two either side):

| at | WRONG | `content-*.txt` |
|---|---|---|
| the base `866747ff` | 8 [2]: train 14's two, and six list numbers wrong before this train (below) | `content-866747ff.txt` |
| the profile line `bacb1052` / the slash line `32eb858f` / TEST-ENV-ISOLATION `7c3f6c1e` / LOCAL-PATHS-WRAP `dd9f5f0a` / APPROVAL-PASTE-SPACES `ca496c10` / META-PREFIX-DECODE `6d94fbc0` | 43 / 49 / 74 / 46 / 47 / 47 [32 / 38 / 63 / 35 / 36 / 36] | `content-<tip>.txt` |
| the join commit `622b2865`, before the re-points | **60** [44] | `content-622b2865.txt` |
| the first re-point `c637ed4c` | 18 [2]: the base's 8 and the list's ten | `content-c637ed4c.txt` |
| round 2's first placing of its cases, `1ddeda00` | 15: the base's 8 and the ledger's `src/tui/shell.rs:50939` seven times (join check 2) | `content-1ddeda00.txt` |
| the code head `6137bd3d`, as it is | **8** [2 at `2b78aab6`]: the base's 8 | `content-head.txt` |
| the same, blame ignoring the three re-points | 9 [3]: the same 8, and docs/parity.md's `src/gateway/mod.rs:1009-1010`, read against the join commit, whose tree had it moved already (the blind spot `15c9744b` fixed by hand, below) | `content-head-ignoring.txt` |

The tips read 43 to 74 WRONG because each was built on an older base -- #88
`32afd307` (TEST-ENV-ISOLATION), train 12's CI rounds `3943d667` and
`71d3e02b` (the profile line, LOCAL-PATHS-WRAP), train 13's head `afd891af`
(the slash line), the decoder line `df45025c` (APPROVAL-PASTE-SPACES,
META-PREFIX-DECODE): what decides is the head.

**Where the 60 came from** (`repoint-plan.txt`, `repoint.py --derive` at
`622b2865` with today's tables; a ref is a side's when that side's content
check reads it WRONG with the same citing file, frame and numbers): **46
moved by the merges** -- 36 round 1 found and the list's ten -- **6 WRONG
on TEST-ENV-ISOLATION's tip already** (its own moves of
`src/tui/theme.rs`, five `scripts/smoke-tui.sh` refs, and of
`src/mcp/transport.rs`), **8 WRONG at the base** (train 14's two and the six
list numbers). By how: 41 `auto` (the frame's cited lines and two either
side stand at one place of the head's file), 10 `text` (the list, TEXT's
span), 9 `left` (train 14's two, the pinned one, the six), and EXTRA's
four. **Round 1's plan, as applied** (it saw no list, so 44): 42 `auto`,
applied in `c637ed4c` (`repoint-plan-applied.txt`, `repoint-apply-1.txt`,
as run): 36 lines out, 36 in, the same once numbers are masked, holding 42
references; each of the 44 plan lines checked at `c637ed4c`, 44 ok
(`repointed.txt`).

**One of the 42 was wrong to move** (found reading the re-points word by
word, `parity-join.txt`): docs/parity.md's MCP client runtime row cites
`58b463c8:src/mcp/transport.rs:1318-1320` -- that commit's file, whose
1318-1320 are `is_answer`, as the sentence says. The content check reads the
path against the head, where TEST-ENV-ISOLATION moved those lines, so it
called the ref WRONG on the unit's tip. **`2b78aab6` puts it back**, and
`repoint.py` now leaves it, with why (its LEAVE table).

**At the first re-point, the scans read four more** the content check
calls SAME (`refs-*-repoint1.txt`; outside the historical plans), each read
by hand at the head and re-pointed in `15c9744b` (`repoint.py`'s EXTRA
table, `repoint-plan-2-applied.txt` as run; numbers only, 4 lines out and 4
in; `extra_check.py` at `15c9744b` and at the head, 4 of 4: the new number
once in the citing text, the old nowhere, the cited lines the ones named,
`extra-check.txt`, `repointed.txt`. The table's text for the fourth was
corrected after: the frame's `:6418` and the head's `:6423` are both `let
mut repaint = false;`, the number as applied):
- `refs.py`: docs/parity.md's `src/gateway/mod.rs:1005-1006` -> `:1009-1010`
  (`is_retryable_status`; merge 3 put four lines above it). The row's last
  writer is the join commit, whose tree has it moved already, so the content
  check's frame agrees with the head -- its blind spot.
- `refs_bare.py`: two bare continuations of refs the first re-point moved,
  `scripts/fxcap/capfile.py`'s `:15785-15787` -> `:15860-15862` (the full
  transcript's `if self.owner != ScreenOwner::Primary`) and
  `src/tui/shell.rs`' `:6418` -> `:6423`.
- `refs_bare.py`: `src/tui/shell.rs`' "`turn_loop`'s `fetch_sub`, `:3535`"
  -> `:3594`. Not moved by this train: wrong since train 11's `6ae87955`
  wrote it, where `bridge::send_terminal(events, answer)` stood (that
  frame's `:3561` was the `fetch_sub`). The content check, reading the
  frame's text, calls it SAME; its frame text is at `:3568` now, which would
  keep it wrong. Re-pointed to the line it names, `turn_loop`'s
  `queue.outstanding.fetch_sub(1, Ordering::Release)`.

**Round 2, the list re-pointed** (`adb809f5`; repoint.py's TEXT, applied by
`--apply-spans`, `repoint-apply-3.txt`): the ten numbers after the first,
each the base's number mapped through `git diff -U0 866747ff` of
`tests/tui.rs` (+121 through 7722, +115 after), 2312, 2887, 3234, 3357,
4049, 7769, 7843, 8630, 9331 and 9527. Numbers only: 1 line out and 1 in,
the same once numbers and lists are masked (`repointed.txt`).
`extra_check.py --text` at `adb809f5` and at the head (`extra-check.txt`):
the new span once and the old nowhere, and for each of the eleven the
base's number mapped to the one written, and the head's line the base's
line, `session.type_bytes(&[0x15, 0x04]);` (3234's `&[0x05, 0x15, 0x04]`):
11 of 11.

**Round 2, the lists left** (`repoint.py`'s LEAVE, with why; read WRONG at
the base already): `src/tui/event_loop.rs:8409`'s
`frame.rs:2631,1156,1626,1696,1758` (four numbers; `frame.rs` is one blob
at the base and the head; train 14's `04e4491d` moved the first alone, the
same blind spot) and `scripts/smoke-tui.sh`'s `src/tui/term.rs:56,65` at
:251 and :1568 (`:65` was `RESTORE` when `3dd16a11` wrote it; `RESTORE` is
at :97-98 at the base and the head). A third `term.rs:56,65`, at :2613,
is as wrong and reads SAME: its frame `ff736932` wrote `:65` when the line
was already a doc comment, the check's blind spot. The other list sites at
the head read right (`src/tui/permissions.rs:30,34` in
`scripts/fxcap/capfile.py`) or cite another product's or a `.prd` file.

**At the code head `6137bd3d`** (`refs-*-head.txt`, against the base and
the six tips): `refs_prd.py` 0 MOVED; `refs_short.py` 0 outside the
historical plans; and four readings left, each by name:
- `refs.py` 6 pairs and `refs_all.py`'s first entry: **one reference, the
  pinned `58b463c8:` one above**, which these scans also read against the
  head; its pinned lines are right.
- `refs_all.py`: `scripts/fxcap/test_sweep.py`'s `cells.py:100000`, a
  test's number past any file's end (the content check's `UNREAD`), as in
  trains 13 to 15; `src/process.rs`' `library/std/src/process.rs:1096-1098`,
  the standard library's file, which the scan's path pattern reads as this
  repository's.
- `refs_bare.py`: `scripts/fxcap/parity-ledger.toml`'s `:116` after
  `docs/parity.md`, a row reference: row 116 is `/sound`'s at the base and
  at the head, its text changed by the slash line (the scan's `CHANGED`).
- `refs_bare.py`: `event_loop.rs:8409`'s `,1626`, `,1696` and `,1758`,
  MOVED on the six tips alone, each built before the base; the base holds
  them as the head does (the list left, above).

**The content check at the head: 8 WRONG, each left on purpose and named**
(`content-head.txt`; `repoint-plan-head.txt`: **8 WRONG, all 8 from the
base, 0 from the merges**): train 14's two (`src/tools/question.rs:303`'s
`Cargo.toml:42` and `tests/session_compat.rs:57`'s
`src/session/event.rs:98-192`, both TRAIN14-FOLLOWUPS') and the six list
numbers above. **0 moved**: every reference the merges moved is
re-pointed, comma lists included, and round 2's cases move none (join
check 2). The same scans and the
content check run again with the receipt in its final form
(`gates-files.txt`, which names the commit it read).

### 5. Network hermeticity (`jobs.sh net`, `net.py`, `jobs-net.txt`, `connect.log`)

Train 15's method, and a second part this train needs. **The method's
reach changed with TEST-ENV-ISOLATION**: every integration test binary now
re-runs its cases in a copy whose environment is `PATH`, `TMPDIR`, an empty
`HOME` and a few cargo names (`tests/support/isolated.rs`'s module doc; its
cases in the gate), so no proxy variable reaches the tui and sessions runs'
xfx -- the loopback logger is blind there by construction. **And the lib
run is blind by construction too** (round 2, opus's S2; round 1 said "not
traced"): every client xfx builds for an endpoint with a bearer goes
through `crate::gateway::bearer_rules` (the MCP client too,
`src/mcp/transport.rs:1792`), which under `cfg(test)` calls
`crate::test_env::proxies` (`src/gateway/mod.rs:125`), and that starts from
`builder.no_proxy()` and adds only the case's fake process's proxies
(`src/test_env.rs:277-303`: `fake.vars`, never the process) -- the process's own proxy variables never
reach it, except in a case's own copy that names
`XFX_TEST_PROXY_CASE_SERVER` (`:274-276`, `src/test_net.rs:39-41`), whose
environment the case builds. TEST-ENV-ISOLATION pins it
(`an_https_client_takes_no_proxy_from_the_process`,
`src/test_env/tests.rs:248`). That is why the known MCP probe's two
`CONNECT w.example:443`, logged by trains 6 to 15, are gone. So part 1 is
kept for its method and its counts, and part 2 is the evidence.

Part 1 (`net.py`, train 15's): a `git clone --no-local` at the code head
`6137bd3d`, one hold of the suite lock; FLAKE-CATALOG-EVENT's
`connect_logger.py` (a loopback logger that answers 403 and forwards
nothing) with the four proxy variables at it and `NO_PROXY`, `no_proxy`,
`ALL_PROXY`, `all_proxy` unset:

| run | result | CONNECTs logged during it |
|---|---|---|
| `cargo test --locked --lib --no-fail-fast` | 5120 passed, 0 failed, 7 ignored | 0 |
| `cargo test --locked --test tui --no-fail-fast` | 192 passed, 0 failed | 0 |
| `cargo test --locked --test sessions --no-fail-fast` | 67 passed, 0 failed | 0 |

Part 2: **every test binary** -- `cargo test --locked --all-targets
--no-fail-fast` -- under `sandbox-exec` with a profile that refuses every
outbound connection but loopback (unix sockets untouched), so a test that
needed a non-loopback connection would fail. **Round 2 adds the attempts**
(opus's S2): the kernel reports each connection the profile refuses in the
unified log (`Sandbox: <name>(<pid>) deny(1) network-outbound
remote:*:<port>`, the address withheld), and `log stream`, net.py's own
child, records every such report from before the first control to after
the last. The controls are planted connections, each a `curl` in its own
`sandbox-exec` under the same profile, beside the run (not inside cargo's
process tree), its pid matched to its report:

| control | result | its denial report |
|---|---|---|
| IPv4 `curl http://192.0.2.1/` (TEST-NET-1, RFC 5737), under the profile | exit 7 at once | reported |
| the same, outside it | exit 28 after 3.0 s (timed out) | none |
| IPv6 `curl http://[2001:db8::1]/` (RFC 3849), under the profile | exit 7 at once | reported |
| the same, outside it | exit 7 at once (no IPv6 route here) | none |
| IPv4 and IPv6 again, 60 s into the run, under it (the run still going after both) | exit 7, exit 7 | both reported |
| IPv4 after the run, under it | exit 7 | reported |

For IPv6 curl's exit tells nothing (7 inside and out); **the report is the
IPv6 control**. The run: `cargo test --locked --all-targets --no-fail-fast`
under the profile, exit 0, 15 binaries, **6062 passed, 0 failed** -- the
gate's plain run's 6062 -- in 119 s. **The reports: 5, every one a planted
pid's; 0 from any other process during the run or around it**
(`jobs-net.txt`; the raw stream stays in the scratch root, since it holds
local paths).

So what part 2 shows, and no more: **no test of the head needs a
connection off this machine** (one would have failed, as the controls'
`curl` did), and **no test made a TCP or UDP connection to a non-loopback
address that the profile refused and the kernel reported**, IPv4 or IPv6,
while the reports were being read (the planted ones, inside the run, were
seen). Not shown: a name lookup (it goes to the system resolver through a
unix socket the profile leaves open, so a lookup with no connection after it
is invisible here), and a count of repeated refusals from one process,
which the kernel may fold into one report. The job ran in one hold of the
suite lock (`jobs-net.txt`, its start at load 27.0), beside the gate.

No unit of this train adds a network path: the profile line reads and
refuses settings, the slash line completes arguments, TEST-ENV-ISOLATION
isolates tests, LOCAL-PATHS-WRAP is the capture harness's, and
APPROVAL-PASTE-SPACES and META-PREFIX-DECODE route keys.

## The join of main (round 3)

Main moved to **`e0aedbf6`** while the train was reviewed: four commits on
the train's base `866747ff` (`git log 866747ff..e0aedbf6`) -- #98's
preview epoch (`afeaea7c`), **#101 CI-TOOLCHAIN-PIN** (`a196722b`: the
gate installs and defaults to Rust 1.99.1), **#99 LOCAL-PATHS-EXIT-TRAP**
(`e78b556c`, a squash: a `bash32` matrix key on the macOS script-test row,
three steps -- "The gate scripts run /bin/bash", "Which bash the gate
scripts run", "A gate script that stops early fails" -- and nine gate
scripts, `test-early-stop.sh` and `early-stop-env.sh` among them) and
**#100**, train 15 itself, as a merge (`e0aedbf6`). No `src/` and no
`tests/` (`join-numstat.txt`: main's side is 651 lines in and 22 out of
gate scripts and CI, the epoch file's one line, **product code 0 out, 0
in**). The mid-branch scan over the four (`join-prescan.txt`,
`midbranch_scan.py` as `prescan.sh` runs it): 0 lines hit, NAME 0,
CREDENTIAL 0, no message holding the pattern.

**The merge, `75d1318c`** ("Merge main (e0aedbf6: #101, #99, #100) into
train 16", `--no-ff`): `ci.yml` the one file both sides changed, and in it
one conflict block -- the `script_tests` comment, which both rewrote. Ours
(TEST-ENV-ISOLATION) added `check-env-reads.py --self-test` and what it
exercises; theirs (EXIT-TRAP) made the list one sentence with
`test-early-stop.sh` and dropped "bash 3.2" (macOS runs the scripts under
3.2 through `bash32` now). Merged by `byhand.py 7`: theirs' sentence with
the five scheduled script tests, ours' "own tools" clause with theirs' grep words, ours'
"no build of xfx". And one line git merged clean that was wrong: ours had
renamed the toolchain step "Install the stable toolchain", theirs makes it
install and default to 1.99.1 -- a step named stable that installs the pin;
**the name is "Install the pinned toolchain" again** (`semantic.py 7`).
`reproduce.sh` redoes it to the merge commit's tree, as the six before
(`resolutions.txt`: seven "no difference"; that commit also holds the two
scripts' merge-7 entries, taken from it by name).

**Every step of both sides is in it** (`join_steps.py`, `join-steps.txt`,
read with Ruby's YAML, as `test-gate-env.sh` reads it): ours' 27 and
theirs' 28 steps all stand with their `if`, `run` and `uses`, each side's
in its own order; the checkout's `with` holds theirs' keys and ours'
`persist-credentials: false`; the five matrix rows hold both sides' keys
(`bash32` on `macos-15`). The comment lines not kept whole are the
conflict block's alone. The order around the new steps (30 in all):

| # | step | if |
|---|---|---|
| 2 | Install the pinned toolchain (1.99.1) | |
| 3 | Confirm the runner really is this target | |
| 4 | The gate scripts run /bin/bash | `matrix.bash32` |
| 5 | Which bash the gate scripts run | `matrix.script_tests` |
| 6, 7 | Formatting, Lints | `matrix.part != 'release'` |
| 8 | Every read of the environment clippy lets through is listed | `matrix.part != 'release'` |
| 9 | The environment check refuses every way round it | `matrix.script_tests` |
| 10-19 | Tests ... The preview contract catches every mutant it exists for | as on each side |
| 20 | A gate script that stops early fails | `matrix.script_tests` |

So the `/bin/bash` step and the which-bash step come before the first
script test (step 15), which `test-gate-env.sh`'s `ci-bash32` case
requires, and TEST-ENV-ISOLATION's two env steps sit after the lints, as on
its side; they run python, not a `./scripts/test-*.sh`, so the case's
"first script test" is unchanged by them.

**The join commit, `a4452753`, docs only**: CONTRIBUTING.md's gate says it
is exactly what CI runs and named four script tests, README.md's three; at
the join CI runs five (TEST-ENV-ISOLATION's self-test and #99's
`test-early-stop.sh`, the macOS row under `/bin/bash`). Both name the five,
each gives `test-early-stop.sh` its line, and README's gate gains the two
env-check lines CONTRIBUTING's had since merge 3. Main changed no doc, no
`docs/parity.md` row and no ledger entry (`git diff --name-only 866747ff
e0aedbf6`), so the token check has no block of the join to read.

**References** (`join-refs.txt`): the content check at the joined code head
reads the same 8 WRONG as at the train's code head and at main, each left
by name (join check 4); the five scans against the join's two sides read 0
references the train's side had and the join moved (`train MOVED` or
`CHANGED`: 0 in each); their `main MOVED` readings are the train's own
re-points since main's base. **0 moved by the join**; nothing to re-point.
`actionlint` 1.7.12 over ci.yml and every workflow: exit 0
(`join-actionlint.txt`). `check-env-reads.py` reads no workflow, and #99's
one new variable, `BASH_ENV`, is a shell script's (`early-stop-env.sh`):
nothing in `src/` or `tests/` reads it, so the check needs no line.

**The readers, at the joined code head** (`join-readers.txt`, from the
gate's own logs; `join-readers.sh`):

**Two different fives** (round 4, gpt6's join review, SHOULD 3). The five
script tests ci.yml schedules -- the ones the comment above, CONTRIBUTING.md
and README.md name -- are `test-gate-env.sh`, `test-preview-contract.sh`,
`test-check-no-local-paths.sh`, `test-early-stop.sh` and
`check-env-reads.py --self-test`, each a step under `if:
matrix.script_tests`. The five `test-gate-env.sh`'s `ci-script-tests-per-os`
case checks per OS are its own list: `test-gate-env`,
`test-preview-contract`, `test-check-no-secrets` (no step of its own; it is
reached through `test-gate-env.sh`'s nested run), `test-check-no-local-paths`
and `test-early-stop`. That case does not read the env self-test. **What
puts the env self-test on one row per OS is ci.yml's own `if:
matrix.script_tests` on its step (step 9)**, which selects `ubuntu-24.04`
and `macos-15` alone: `join-steps.txt`'s last lines read it, and the five
other steps under that condition, from the merged YAML, apart from any
reader.

- **`test-gate-env.sh`** (main's, 95 cases, exit 0): `workflow-steps` (all 10
  steps that run git drop `GIT_*` first: the checkout comment and
  `persist-credentials` add no git line), `ci-script-tests-per-os` (its
  own five, above, each on a macOS and a Linux row), **`ci-bash32`** (the macOS
  script-test rows run `/bin/bash`; the pin and the which-bash step before
  the first script test, TEST-ENV-ISOLATION's two steps between them
  reading nothing it counts), `ci-checkout-assertion`. Its one other line
  is its note that this grep reads `GREP_OPTIONS`, as on main.
- **`test-early-stop.sh`** (#99's): ok, 15 cases, under `/bin/bash` 3.2.57.
- `test-check-no-secrets.sh` 23 cases, `test-preview-contract.sh` 35 mutants
  caught, `check-preview-contract.sh` ok, `test-check-no-local-paths.sh` 31
  cases, `check-no-secrets.sh` and `check-xfx-identity.sh` ok over 25840
  tracked files, `smoke.sh` 47 checks, 0 failures (main changed each).
- **`check-env-reads.py`** OK (40 `env_exception!` invocations in 27 files,
  74 compile-time reads of 6 names: the same as at `6137bd3d`), and its
  `--self-test` 66 fixtures, 0 not as expected: main brings no read to list.
- The three new steps' own output: rustc, cargo, clippy and rustfmt 1.99.1
  (0.1.99, 1.10.0); `/bin/bash is bash 3.2.57(1)-release`; and
  `#!/usr/bin/env bash` runs the `bash` link the step put first, bash
  3.2.57.
- actionlint, exit 0 (above).

**The gate at `a4452753`** (`gates.sh`, which now mirrors the joined
ci.yml on the macos-15 row, the row this machine is: the pinned toolchain
-- the cargo and rustc it runs must be 1.99.1, as they were for rounds 1
and 2 -- the `/bin/bash` step putting a `bash` link first on PATH for every
step after it, the which-bash step, and `test-early-stop.sh` after the
preview contract's test; `ci-steps.txt` at the joined head): **29 steps, 0 failed, none run again**
(`gates.txt`, `gate-counts.txt`; a fresh `git clone --no-local`, one hold of
the suite lock, load 11.7 to 51.1 at the steps' starts): `cargo test`
**6062 passed, 0 failed, 8 ignored**, with fault injection **6099 passed,
0 failed, 8 ignored** -- `6137bd3d`'s, binary by binary (the join brings no
test; `test-lists.txt` stands); smoke-tui 37 scenarios and the oracle,
1167 checks, 0 failures; `fxcap/test-capture.sh` 145 OK. Each unit's tests
on the head (`unit-tests.txt`, `unit-tests-notes.txt`, `run-absent.txt`,
`gate-derived.sh`, which now finds the two `cargo test` logs by name: they
are steps 09 and 10 now): the same as at `6137bd3d`. A first gate at the
merge `75d1318c` was stopped by me in its seventh step, its six before
exit 0, when the gate lists' docs were found short; nothing of it is used.
`gates-files.txt` holds the file checks with the receipt in its final form.

**The sweeps stand**: the join brings no product line (`join-numstat.txt`:
over `6137bd3d..a4452753`, product code 0 out, 0 in; gate scripts and CI,
the epoch file and the two docs), no scenario, ledger or harness file.

## Round 6: round 5's review (`round6-check.txt`, `round6-numstat.txt`)

Round 5's review (gpt6, two personas) approved, with no MUST. Two small
items, tests and the check only, on top of `a3f8ee26`:

**1. The reviewer's NTH: `toolchain_first` when the parent has no `PATH`**
(`bbb2c109`, doc only). It returns nothing and the copy gets no `PATH`, so
a command xfx runs gets `minimal_environment`'s system default, which holds
no rustup toolchain. Kept, and the module doc now says so. A `PATH` of the
toolchain directory alone would be worse: that function would take it as
given, and every other program a case starts would be missing. Sharing the
system default would mean a product line: it is a literal inside
`minimal_environment`, and the tests' own `path()` falls back to empty.

**2. The dispatcher's finding: `CARGO` in COMPILE_TIME_NAMES held in any
file** (`23819f7a`). Round 5 let an `env!("CARGO")` in `src/` -- the
builder's absolute cargo path, baked into the product binary -- pass both
the clippy-report check and the dep-info check. It is now
COMPILE_TIME_SCOPED. An `env!`/`option_env!` of it passes only in
`tests/support/isolated.rs`, and a crate's dep-info may hold it only for a
crate whose root source is under `tests/`: dep-info is keyed by the crate,
and every integration test binary compiles that file in. The header's
contract says so. Two new self-test fixtures:
- `env!("CARGO")` in a `src/` module of the fixtures' library is refused
  by both checks ("a core::env! of CARGO, a compile-time name only
  tests/support/isolated.rs may read"; "compiled with CARGO ..., a
  compile-time name only crates under tests/ may read");
- in `tests/support/isolated.rs`, compiled into an integration test, it is
  accepted.

**RED**: with round 5's global table put back, the `src/` fixture goes
unrefused (68 fixtures, 1 not as expected). **GREEN**: 0. The real check
is OK, with 75 compile-time reads of 7 names, as in round 5.

**Product lines: 0** (`round6-numstat.txt`: the env check +47/-4, the test
support's doc +9). The sweeps stand.

**Linux clippy** at `23819f7a` (`linux-clippy.sh`): 0 of 4 runs failed, both Linux targets, plain and with fault-injection (`round5-linux-clippy.txt`, its round-6 lines).

**The gate, both ways, at the code head `23819f7a`** (each a fresh `git
clone --no-local`, one hold of the suite lock):
| run | how | steps | `cargo test` / with fault injection |
|---|---|---|---|
| through rustup's proxies (`gates-rustup.txt`, `gate-counts-rustup.txt`) | rustup's own multiplexer first on PATH, no fallback default, `RUSTUP_TOOLCHAIN=1.99.1`, `TMPDIR` given through the `/tmp` link | **29, 0 failed** (load 11.0 to 57.5) | **6062 passed / 6099 passed**, 8 ignored each |
| as before (`gates.txt`, `gate-counts.txt`) | Homebrew's 1.99.1 on this Mac's PATH, `TMPDIR` resolved | **29, 0 failed** (load 22.9 to 49.5) | **6062 passed / 6099 passed**, 8 ignored each |

The two ran side by side. In both, the env check is OK and its self-test
reads 68 fixtures, 0 not as expected. **The session store's flock race did
not show** in either run's step 09 (round 5's `round5-flake.txt`).
`gates-first.txt` and `gate-counts-first.txt` stay round 5's record of
that race. `unit-tests.txt`, `unit-tests-notes.txt` and `run-absent.txt`
are made again at `23819f7a`, from the as-before run's logs. No push.

## Round 5: PR #102's CI (`round5-causes.txt`, `round5-linux-clippy.txt`, `round5-numstat.txt`)

PR #102's CI run 43880950535 failed on 4 of 6 rows, the release row and the
sweep passing. The gate here had missed all three causes, for one reason:
they are the runners', not the code paths'. This Mac's `cargo` on PATH is
Homebrew's 1.99.1, not rustup's proxy; this Mac is not Linux; and its
`TMPDIR` had been given resolved. CI's lines for each, from its job logs, are
in `round5-causes.txt`; each cause was reproduced here, fixed in its own
commit, and run RED then GREEN.

**1. Linux rows (x86_64 and aarch64), Lints**: clippy's `useless_conversion`
at `src/config.rs:2639`, `u32::from(stat.st_mode) & 0o022 != 0` (the profile
line, `02c32c3f`): `mode_t` is a `u16` on macOS and a `u32` on Linux, so the
conversion is one on macOS alone. **Confirmed.** The Linux check from this
Mac (`linux-clippy.sh`): rustup's 1.99.1 with both Linux std's, `cargo
clippy --locked --all-targets --target <t> -- -D warnings`, plain and with
fault-injection. A check links nothing, but **a build script blocks it**:
ring's compiles C for the target, and this Mac has no Linux C compiler and
no Linux libc headers. Apple's clang as the target's CC, with ring's own
cross-building switch `RING_CORE_NOSTDLIBINC` and `-nostdlibinc` (and
`-ffreestanding` on x86_64), builds it; those flags reach ring's C alone,
and no cfg of xfx's stands in the way. **RED at `8be4693b`: 4 of 4 runs
failed, on that lint alone. Fix (`ac5882fb`)**: `stat.st_mode & 0o022 !=
0`, the mask in `st_mode`'s own type on both OSes -- no conversion, no
cfg, no allow. **GREEN: 0 of 4**, at the code head `3ed1c082`; the lib
compiling, every target after it is checked, and nothing else is
Linux-only.

**2. x86_64 macOS (checks), Tests**:
`a_config_alias_cannot_redirect_the_admitted_cargo_builtins` panicked at
`tests/permissions.rs:609`, its `cargo metadata` child saying "rustup
could not choose a version of cargo to run ... no default is configured".
**Cause confirmed and corrected**: TEST-ENV-ISOLATION's copy is the cause,
but passing rustup's variables does not fix it. On the runners `cargo` is
rustup's proxy, which finds its toolchains and default under `HOME`; the
copy gives each case an empty `HOME`. And **a command xfx runs gets an
environment of `HOME` and `PATH` alone** (`permission::authority`'s
`minimal_environment`, "built rather than inherited"), so a
`RUSTUP_HOME`/`RUSTUP_TOOLCHAIN` handed to the copy never reaches the
command. I tried it first: still RED. Main passes because it has no copy.
rustup does set both, resolved, plus `CARGO_HOME` and `CARGO`, for the
cargo it runs (a probe, `round5-causes.txt`).
- **Reproduced** with rustup's own multiplexer first on PATH (no fallback
  default) and `RUSTUP_TOOLCHAIN=1.99.1`: the same panic at the same line,
  the same message.
- Under Homebrew's rustup proxy the case passes, because Homebrew's carries
  a fallback default ("stable"): the proxy downloaded stable, six
  components, into the copy's empty `HOME`. A second defect behind the
  first.
- **Fix (`3ed1c082`), tests only**: the copy puts the directory of the cargo
  that built the binary (`env!("CARGO")`) first on its `PATH`, so a case's
  `cargo` and `rustc` are that toolchain's own binaries, under any rustup
  or none. **No variable is let through**: `HOME` stays the empty one, and
  `PATH` gains a directory of program files -- no configuration, proxy or
  credential. `check-env-reads.py` lists `CARGO` among the compile-time
  names (cargo's own) and the copy's `PATH` read for
  `tests/support/isolated.rs`; the check is OK (41 `env_exception!`
  invocations, 75 compile-time reads of 7 names), and its self-test still
  refuses every way round it (66, 0 not as expected).
- **GREEN** under the multiplexer: the case passes, and the whole
  permissions binary passes 159.
- Not done: passing `RUSTUP_HOME` in xfx's own command environment. That
  would fix a person whose rustup lives off `~/.rustup`, but it is a product
  decision, not this train's; queued.

**3. aarch64 macOS, the env check's self-test**: "66 fixtures, 4 not as
expected", the four that rest on a compilation's dep-info. **Cause
corrected: not the toolchain.** Under rustup's multiplexer, TMPDIR
resolved, the self-test passes. The cause is `TMPDIR`'s spelling. macOS's
temporary directories sit under `/var`, a link to `/private/var`, and cargo
reports a package's paths resolved (getcwd). So `os.path.relpath(src_path,
root)` keyed every compiled file `../../private/...`, where no fixture --
and in the real check, no EXCEPTIONS entry -- finds it. The real check
would fail the same way, silently, on a checkout reached through a link.
- **RED at `8be4693b`**: Homebrew's cargo, `TMPDIR` given through the
  `/tmp` link -- the same four lines as CI's.
- **Fix (`96cbaab2`)**: `relative()`, which reads a path against the root
  as given and as resolved, at every place a cargo-reported path is keyed
  (record_compiled, classify, artifact_problems, cfg_problems,
  build_script_problems).
- **And the self-test pins it**: it now reaches its fixtures' package
  through a link of its own, so every machine runs the case. With
  `relative()` put back to plain relpath it reads CI's four, `TMPDIR`
  resolved; with it, 0, `TMPDIR` resolved or through the link.

**Product lines** (`round5-numstat.txt`): one, `open_to_others`' expression,
1 out and 1 in, with 3 comment lines. The rest is the env check (+35 -8)
and the test support (+25). **The sweeps stand**: for every 16-bit
`st_mode` (macOS's, where the sweeps run) the two expressions give the
same answer, 0 of 65536 differing; on Linux `u32::from` was the identity.
So the built binary decides every profile's mode exactly as before; a
rerun would read the same verdicts.

**The gate, both ways, at the code head `3ed1c082`** (each a fresh `git
clone --no-local`, one hold of the suite lock):
| run | how | steps | `cargo test` / with fault injection |
|---|---|---|---|
| **through rustup's proxies** (`gates-rustup.txt`, `gate-counts-rustup.txt`) | rustup's own multiplexer first on PATH, no fallback default, `RUSTUP_TOOLCHAIN=1.99.1`, `TMPDIR` given through the `/tmp` link -- CI's macOS conditions, all three | **29, 0 failed** | **6062 passed / 6099 passed**, 8 ignored each |
| as before (`gates.txt`, `gate-counts.txt`) | this Mac's PATH (Homebrew's 1.99.1), `TMPDIR` resolved; the second run | **29, 0 failed** | **6062 passed / 6099 passed**, 8 ignored each |
| as before, the first run (`gates-first.txt`, `gate-counts-first.txt`) | the same, at loads of 64 to 101 beside the rustup gate and other workers' jobs | 29, **1 failed** (step 09) | 6061 passed, **1 failed** / 6099 passed |

(Round 6 replaced `gates.txt`, `gate-counts.txt`, `gates-rustup.txt` and
`gate-counts-rustup.txt` with its own two runs at `23819f7a`, the same
counts; the figures here are round 5's, at `3ed1c082`.)

The rustup gate is round 5's receipt for causes 2 and 3: its step 09 is
the whole suite, the permissions case among its passes, and its step 08
the self-test with the fixtures under a link and `TMPDIR` through one (66,
0 not as expected). In both full runs the env check reads 41
`env_exception!` invocations and 75 compile-time reads of 7 names, OK;
`test-gate-env.sh` 95 cases; `test-early-stop.sh` 15; smoke 47 checks and
smoke-tui 1167, 0 failures.

**The first run's one failure was not this round's** (`round5-flake.txt`).
`session::store::tests::a_prompt_images_copy_gone_or_not_a_file_refuses_the_resume_in_upstreams_words_and_changes_nothing`
(IMAGE-LOSS-COPY-FX's) got "already open for writing by another xfx
process" where it wanted "saved session is unreadable", on its second,
non-waiting resume, at load 63.8. The session writer's lock is an
`flock` on the log's open file description, and its doc names the race:
a child that another thread of the same lib-test process forked inherits
the description, and so the lock, until it execs, so a descheduled one
makes a session nobody writes look busy. The non-waiting resume has no
grace period. That this is what happened is inferred, not observed. The
case alone passes 30 of 30 at the same tree; the rustup gate's step 09,
run beside it, passed; the rerun passed. No file round 5 changed touches
the session store or its tests. A flake to catalogue, not fixed here: the
fix -- a grace for the non-waiting resume, or no fork while a log is open
-- is the store's, and a product change.

The test lists are round 3's: no test added or removed (`test-lists.txt`
stands; `unit-tests.txt`, `unit-tests-notes.txt` and `run-absent.txt`
again at `3ed1c082`, from the second run's logs). **No push**: the
dispatcher pushes and watches CI.

## Gate (`gates.sh`, `gates.txt`, `gate-counts.txt`, `gate-files.sh`, `ci-steps.sh`, `ci-steps.txt`)

**Round 3: the gate below is round 2's, at `6137bd3d`; the gate at the
joined code head `a4452753`, 29 steps with the join's four, is in "The join
of main" above, with the same test counts. Round 5: the gates at the code
head `3ed1c082`, as before and through rustup's proxies, are in "Round 5"
above. Round 6: the gates at the code head `23819f7a` are in "Round 6"
above; `gates.txt` and `gate-counts.txt` are now its as-before run, and
`gates-rustup.txt` and `gate-counts-rustup.txt` its rustup run.**

`gates.sh` is train 15's with TEST-ENV-ISOLATION's two CI steps added where
`ci.yml` runs them, after the lints: `check-env-reads.py` and its
`--self-test`. `ci-steps.txt` holds the step list it is held to: `ci.yml`'s
gate job and `sweep.yml`, each step as `name | if | run`, at the base
`866747ff` and at the head, and the lines that differ -- `ci.yml` 4 (the
toolchain step renamed, its commands the same, and the two new steps),
`sweep.yml` 0 -- then gates.sh's 25 steps. Every step, in a fresh `git
clone --no-local` at the code head **`6137bd3d`** (round 2; round 1's at
`2b78aab6` was the same 25 steps, 0 failed, 6060 and 6097 passed), one
after another, **the whole script inside one hold of the suite lock** (one
normal slot), each step's exit taken from `$?` on the line right after its
command, the tree clean before and after, TMPDIR the harness scratch root.
**25 steps, 0 failed, none run again** (`gates.txt`), at a load of 13.2 to
43.7 at the steps' starts (the network job and the merge checks beside
it). With
CATALOG-WAIT-CLOCK in the base, a catalog-wait failure would have been a
finding: there was none.

(A first gate at `3d1376b2` was stopped by me during its sixth step, its
five before exit 0, when the pinned reference was found (join check 4);
nothing of it is used here. Its scratch copy is not kept. In round 2 a
gate at `1ddeda00` was stopped by me during its seventh step, its six
before exit 0, when that commit's cases were found to move a cited line;
nothing of it is used either.)

| step | at `6137bd3d` |
|---|---|
| `cargo fmt --check`; clippy; clippy `--features fault-injection` | ok; ok; ok |
| `check-env-reads.py` / its `--self-test` | OK (40 `env_exception!` invocations in 27 files, 74 compile-time reads of 6 names, 52 test binaries started with a marker no parent makes) / 66 fixtures, 0 not as expected |
| `cargo test --locked --no-fail-fast` | **6062 passed, 0 failed, 8 ignored** (round 1: 6060; round 2's two cases) |
| the same `--features fault-injection` | **6099 passed, 0 failed, 8 ignored** (round 1: 6097) |
| `check-xfx-identity` / `check-no-stubs` / `check-no-secrets` | ok / ok / ok |
| `check-no-local-paths` / `test-check-no-local-paths` | ok (25749 tracked files, round 1's receipt among them; 8 marked fixture lines, 84 placeholder lines) / ok (31 cases) |
| `check-preview-contract` / `check-tui-preflight-cost` | ok / ok (200x300 full replay 1.21 ms mean; budget 32) |
| `check-at-search-cost` / `check-usage-clock` | ok (every keystroke, 1460 searches: p50 0.35 ms, p95 2.50 ms) / ok |
| `test-check-no-secrets` / `test-gate-env` / `test-preview-contract` | ok (13 cases) / ok (66 cases) / ok (28 mutants) |
| `fxcap/test-sweep.sh` / `fxcap/test-capture.sh` | 79 + 28 OK / 145 OK |
| release build; `fault-injection` release build | ok; ok |
| `smoke.sh` / `smoke-tui.sh` | 47 checks, 0 failures / 37 scenarios + the oracle, 1167 checks, 0 failures |

The counts are `gate-counts.txt`'s (each binary's `test result` line and
their sums). **Every side's tests are in** (`test-lists.txt`: every test
each binary lists, `cargo test --all-targets -- --list`, at the base and the
head; `test-counts.txt`: the `#[test]` lines and `tests_kept.py`): the head
`6137bd3d` lists **6070, the base 5949: +121 -- 122 tests the head lists
that the base does not, one the base lists that the head does not**
(`process::tests::no_child_process_is_made_any_other_way`, which
TEST-ENV-ISOLATION deleted); the same +121 with fault injection (6107 against
5986), and the `#[test]` lines' 5855 -> 5976. The 122 are the units' and the
join's (`tests_kept.py`: profile line 36, slash line 32, TEST-ENV-ISOLATION
27, APPROVAL-PASTE-SPACES 9, META-PREFIX-DECODE 13, the join 3) and round
2's two cases -- exactly the two the head lists that round 1's head
`2b78aab6` did not, and none the other way (`test-lists.txt`'s last
lines) -- and **0 tests any side added are missing at the head**. The
gate's 6062 passed and 8 ignored are those 6070, binary by binary (round
1: 6060 of 6068). Train 15's gate at `2c575f52`
passed 5942 with 7 ignored, its one doc test among the passed
(`../train-15/gate-counts.txt`), and its round 2 added one lib test: so the
base runs 5943 passed and 7 ignored, its 5949 listed and the doc test. The
+119 passed since is the +121, less one newly ignored test
(`a_case_marks_where_it_runs`) and the doc test, which TEST-ENV-ISOLATION's
`doctest = false` no longer runs (`test-lists.txt`'s last lines).

## Sweeps

**Round 2: the sweeps stand, run at `2b78aab6`.** What round 2 changed
outside this folder (`round2-numstat.txt`, `round2_numstat.py`: every line
of `git diff -U0 2b78aab6 6137bd3d`, by class): `src/tui/shell.rs` +116
inside its `#[cfg(test)] mod tests` (the two join cases), `tests/tui.rs`
one comment line out and one in (the list's numbers); **product code 0
out, 0 in**. No scenario, ledger entry or harness file changed, so the
debug build a sweep drives and the scoring it gets are `2b78aab6`'s; the
one thing that differs is the banner's build id. Not swept again.

**Builds** (debug, as trains 4 to 15), each in its own `git clone
--no-local`, each swept with its own tree's harness, scenarios and ledger
(`sweep.sh`) -- the head `2b78aab6` and **the base `866747ff`**, train 15's
head. fx: the v0.0.16 binary, sha256 `8e6cda2e...` before and after each
sweep, `FX_AUTO_UPGRADE=0`; 40x120, `--jobs 2`; each sweep inside one hold
of the suite lock, **the two side by side** (`sweep-runs.txt`: each sweep's
lines, its `run.json`'s start, wall time, loads and the xfx it ran). **One
selection, both sweeps** (`selection-sweep.txt`; `selections.sh`,
`selections.txt`: made again here, the same names):
- the main selection (`selection-all.txt`, `selection.py`): the brief's
  twelve globs (`approval*`, `p5-approval*`, `composer*`, `p5-modified-*`,
  `p5-cancel*`, `cancel*`, `slash*`, `p5-slash*`, `p5-mode-*`, `profile*`,
  `p5-profile*`, `settings*`; `cancel*`, `profile*` and `p5-profile*` name no
  scenario at either tree), every scenario named by a ledger entry the
  merges or the join commit changed, every scenario a merge added, and the
  scenarios the two re-points' entries name; `*` (`session-record`,
  `stats-bytes`) left out;
- the units' selection (`selection-units.txt`, `selection_units.py`): the
  156 scenarios the units' seven committed head sweeps took that the first
  does not, so every scenario a unit swept is swept here (PROFILE-REFUSE-
  AT-LAUNCH's and PERMISSION-MODE-CASE's boards read by their `--only`
  globs; PROFILE-REFUSE-TYPED-KEYS, PROFILE-JSON-LIMITS, TEST-ENV-ISOLATION
  and LOCAL-PATHS-WRAP committed no sweep).

At the base 210 scenarios of 432 are selected, at the head 223 of 445
(`selections.txt`); all but two give pairs (221 at the head, 208 at the
base: `p5-lifecycle-osc-hostile` and `-states` take no snap, their OSC
reports read by their own check). The base sweep was rescored with
the head's ledger (`rescore.sh`): the pass line the same before and after,
and **pair by pair 0 verdicts moved** (`sweep-runs.txt`; the ledger's sha256
`123720b4`, the head's). Every xfx snap that shows the banner names the
build swept: `2b78aab` on all 1354 of the head's, `866747f` on all 1249 of
the base's (`banners.py`). **Both sweeps started at load 8.96 and peaked at
46.65** (each `run.json`), walls 74m48s (head) and 71m50s (base); they ran
side by side, so each carried the other's load, and the network job
(08:53-08:57Z) and the test lists (`test_lists.sh`) ran beside them.

| sweep | scenarios | pairs | pass | failed | timeout |
|---|---|---|---|---|---|
| head `2b78aab6` (`sweep-head.md`) | 223 | 1514 | **971** (64.1%) | 0 | 0 |
| base `866747ff` (`sweep-base.md`, rescored) | 210 | 1408 | 879 (62.4%) | 0 | 0 |

The head's 13 scenarios more are the units' new ones, 106 pairs, 91 pass
(`new-scenarios.txt`), so 880 + 91 = 971.

**Rows no cause covers** (each scoreboard's list; `uncovered.py`,
`uncovered.txt`): 164 pairs at the base, 165 at the head. The base's alone:
`p5-panels-badsettings` `launch`, which passes at the head (below). The
head's alone: `p5-slash-leading-ws` `vers-run` and `vers-recall`, a scenario
new to this sweep -- the `* version:` row, its text the same once
normalized, its cell 16 grey in fx and plain in xfx: **the base's own
version-row difference**, the same cell in the base sweep's
`p5-panels-sweep-a` `version` (uncovered there too). SLASH-LEADING-WS's own
sweep, on train 13's xfx 0.1.0, passed both; #96's version line came after.
Not the train's; named for its owner.

### Against the base, pair by pair (`compare-base.txt`, `screen-moves-base.txt`, `screen-classes.txt`, `screen-partition.txt`, `gained-rows.txt`, `reruns.txt`)

**1408 shared pairs: 879 -> 880; 1 up, 0 down, none moved within passing**
(`compare-base.txt`: "verdicts moved: 1"; "passed on the reference, not on
the train: 0"):
- **1 up, the profile line's**: `p5-panels-badsettings` `launch`,
  unresolved -> normalized-identical: a malformed settings file now refuses
  the launch as fx's does (PROFILE-REFUSE-AT-LAUNCH), fx's pane dead and
  xfx's too, where the base launched and said `* config: 1 configuration
  issue`. **Rerun 3 times on each tree** (`reruns.txt`): normalized-identical
  3 of 3 at the head, unresolved 3 of 3 at the base.
- **Rows gained, 9 pairs** (`gained-rows.txt`): `p5-mcp-menu-new`
  `approve-confirm` (approved on both), LOCAL-PATHS-WRAP's placeholder where
  the base's row held a run's path; `p5-transcript-question` `full-60`, the
  banner row's build id, as in train 15; and seven `p5-panels-sweep-b`
  pairs, unresolved on both, whose `* stats:` row reads another
  `ansi_bytes` -- fx's moved too (5035 at the base's sweep, 5100 at the
  head's), a byte count of the run. **Rerun 3 times on each tree**
  (`reruns.txt`): every pair's verdict the same in all six runs (4 of 11
  pass each time, on both trees), and the row's `ansi_bytes` takes either
  value on either tree -- xfx's 4267 or 4390 at the head and at the base,
  fx's 5100 or 4993 -- a byte count the run decides, not the build.

**The screens** (`screen-partition.txt` ties the counts together):
compare.py finds fx's `.ansi` moved in 142 shared snaps and xfx's in 298.
The classes read the `.txt` too (`screen-classes.txt`), so they hold 144 of
fx's and 299 of xfx's: fx's 103 in clock-like or run-local text alone, 39
beyond it, 2 in colour or attributes alone (2 of the 144 a `.txt` move
whose `.ansi` did not: 142 + 2); xfx's 99, 200 and 0 (1 such move: 298 +
1). Of the moves beyond run-local
text, `screen-moves-base.txt` lists 225 such lines (`screen-classes.txt`): 206 a saved session's id
(`--resume` ids, `/status`'s Session row), and 19 others -- 17 the `*
version:` line's build id (`...-0615-866747ff7feb` / `...-0759-2b78aab658dd`;
`fulltranscript-notice`, `p5-panels-sweep-a`), the `p5-mcp-menu-new`
placeholder and `p5-panels-badsettings` `launch`, above.

### Against each unit's own sweep (`compare-units.txt`)

Every unit's committed head sweep with a per-scenario table (six;
PROFILE-REFUSE-AT-LAUNCH's summary has none, and its 83 scenarios are read
against the base above) against the head's, scenario by scenario:

| unit (its sweep) | shared scenarios | pass, unit -> head | lower at the head |
|---|---|---|---|
| PERMISSION-MODE-CASE | 3 | 5 -> 7 | 0 (`p5-mode-case-full` 0 -> 2, join check 1) |
| SLASH-LEADING-WS | 8 | 44 -> 42 | **1**: `p5-slash-leading-ws` 10 -> 8 |
| SLASH-ARG-SKILLS | 56 | 226 -> 224 | **1**: the same |
| SLASH-MIXED-SKILLS | 12 | 94 -> 92 | **1**: the same |
| APPROVAL-PASTE-SPACES | 22 | 107 -> 107 | 0 |
| META-PREFIX-DECODE | 37 | 255 -> 272 | 0 |

**`p5-slash-leading-ws`'s two** are `vers-run` and `vers-recall`, the base's
version row above: the units' sweeps ran on train 13's head (xfx 0.1.0,
before #96's version line). **Rerun 3 times at the head** (`reruns.txt`):
the eight other pairs normalized-identical 3 of 3, `vers-run` and
`vers-recall` unresolved 3 of 3, their version row as in the sweep.

**So no pair that passes on the base fails at the head**, and against the
units' own sweeps the one lower scenario is lower by the base's own row.

## Decisions, for the user's veto

Each with its revert.

- **`XFX_BUILD_REVISION` leaves COMPILE_TIME_NAMES** (merge 3), beyond the
  MUST's "gains `XFX_BUILD_ID`": build.rs no longer sets it (#96), so an
  `env!` of it could not build and an `option_env!` of it would read the
  person's environment at compile time. **Revert**: put the line back in
  `scripts/check-env-reads.py` beside `XFX_BUILD_ID`.
- **`XFX_AT_BENCH_TREE` is an allowed name, read through `env_exception!`**
  (merge 3): the @ benchmark's tree, a test knob a person sets; through
  `process_env!` the test would never see it. **Revert**: drop the
  benchmark's tree option (the test then builds fx's inventory alone) and
  the two lines in the check.
- **`recording()` keeps main's `Runtime::with_environment`** (merge 3), not
  TEST-ENV-ISOLATION's `a_runtime`: the same environment, and the `env`
  above it is read. **Revert**: theirs' line and the `env` lines deleted.
- **One `Draft::apply` without a history, with APPROVAL-PASTE-SPACES' edges
  as a mapping** (merge 5). **Revert**: none that builds -- the history is
  gone with train 15's APPROVAL-DRAFT-KEYS.
- **The `/permissions full` scenario waits for the notice on both sides**
  (`622b2865`): PERMISSION-MODE-CASE's xfx-only `wait 1` was written for a
  composer with no menu. **Revert**: the scenario's two `fx:` prefixes and
  `xfx: wait 1` back; its `full` pair is then unresolved again, on the
  notice row alone.
- **A meta-prefixed Ctrl+J at a permission prompt now answers it** -- `ESC
  ESC [106;5u` answers the **marked** choice, as Enter and a bare C-j do
  (`Action::ControlJ` -> `approval::Action::Submit`, `Shell::approval_action`;
  Submit answers the selected choice, `approval.rs`'s `Action::Submit`):
  `Apply once` at choice 1, and **at choice 2 the session's `Always`**, which
  stops asking for the rest of the session (the C-j join test pins choice
  1, round 2's choice-2 case choice 2, on both planes). The base read the Escape first, which cancels the turn
  there. It is fx v0.0.16's (the
  report carries the prefix and the Ctrl+letter rule ignores it,
  `escape_parser.zig:552-555`, `:686-691`, `:255-258@481104fd`), and it is
  the same decision as train 15's stray C-j (its README's "A stray C-j at a
  permission prompt now grants `Apply once`", before the user since
  2026-11-10): a C-j answers a permission question as Enter does. **The same
  one-line revert stops both granting**: in `Shell::approval_action`
  (`src/tui/shell.rs`), drop `| Action::ControlJ` from
  `Input::Action(Action::Submit | Action::ControlJ) => approval::Action::Submit`
  -- the decoder hands both spellings over as `Action::ControlJ`. What the
  revert does, exactly: both spellings are then **swallowed** -- with no
  draft open by the `Input::Action(_) => return None` arm, in a draft by the
  editing arm, C-j being none of a draft's keys (`approval::edits`) -- so
  the prefixed spelling does **not** go back to the base's Escape and cancel
  (that came from the decoder reading the pair as Escape, which
  META-PREFIX-DECODE fixed). And it **reaches rule confirmations too**: a
  `/permissions` rule change is confirmed in the same prompt and reads its
  keys through the same function (its doc comment), so C-j stops confirming
  one, as train 15's entry says ("the permission question and the rule
  confirmation alike then swallow C-j again").
- **Five join tests** (three in `622b2865`, where no unit's tests reach
  the meeting; round 2's choice-2 and split cases, `6137bd3d`). **Revert**:
  drop them; the meetings then have no case.
- **Every reference the merges moved re-pointed** (`c637ed4c`, `15c9744b`,
  and round 2's `adb809f5`: the comma list's ten), and one pre-existing
  wrong one with them (`:3535` -> `:3594`, read by hand); the pinned one put
  back (`2b78aab6`). **Revert**: `git revert adb809f5 2b78aab6 15c9744b
  c637ed4c`.
- **Two comma lists wrong before this train are named, not re-pointed**
  (round 2; `repoint.py`'s LEAVE, with why): `src/tui/event_loop.rs:8409`'s
  `frame.rs:2631,1156,1626,1696,1758` (four numbers; train 14's re-point
  moved the first alone) and `scripts/smoke-tui.sh`'s
  `src/tui/term.rs:56,65` at :251, :1568 and :2613 (`:65` was `RESTORE`,
  now at :97-98). The merges moved neither: `frame.rs` is the same blob at
  the base and the head, and the base's content check reads them WRONG
  already (but :2613, its blind spot). Queued with train 14's two.
  **Revert** (to fix them here instead): their numbers, by hand, in one
  commit, re-gated.
- **`ci.yml`'s `script_tests` comment** names the env self-test and what it
  exercises (`622b2865`, `3d1376b2`), comment only. **Revert**: `git revert
  3d1376b2` and the comment hunk of `622b2865`.
- **The unit receipts are left as their heads wrote them**: where this
  train supersedes one (PERMISSION-MODE-CASE's section 8, QUESTION-CTRL-J's
  `probe-meta-prefix.txt` reading), this receipt says so. **Revert**: none
  needed.
- **`CARGO` scoped, not global** (round 6, `23819f7a`): an `env!`/`option_env!`
  of it passes in `tests/support/isolated.rs` alone, and a crate's dep-info
  may hold it only for a crate under `tests/`. **Revert**: `git revert
  23819f7a`; an `env!("CARGO")` in `src/` then passes both checks again.
- **The clean copy's toolchain on PATH** (round 5, `3ed1c082`): an
  integration test's copy puts the directory of the cargo that built it
  first on PATH, rather than handing on rustup's variables, which a
  command xfx runs never receives. **Revert**: `git revert 3ed1c082`; a
  case's `cargo` under rustup then fails on the runners again.
- **`RUSTUP_HOME` in xfx's own command environment, not changed** (round
  5): a person whose rustup lives off `~/.rustup` has a `cargo` that
  fails under xfx's terminal tool, as the case did; passing it is a
  product decision, queued, not this train's.
- **The env check keys files whichever way a root is spelt** (round 5,
  `96cbaab2`), and its self-test runs under a link of its own. **Revert**:
  `git revert 96cbaab2`; CI's aarch64 macOS row fails its self-test again.
- **`open_to_others` in `st_mode`'s own type** (round 5, `ac5882fb`): no
  conversion, no cfg, no allow; the same answer for every mode.
  **Revert**: `git revert ac5882fb`; the Linux rows' clippy refuses it.
- **The join of main's ci.yml** (round 3, `75d1318c`): the `script_tests`
  comment names all five script tests in theirs' sentence, and the
  toolchain step is "Install the pinned toolchain" again, as main has it,
  since it installs 1.99.1. **Revert**: the comment either side's way
  (`byhand.py 7`), the name ours' (`semantic.py 7`) -- a step called stable
  that installs the pin.
- **The gate lists name five script tests** (round 3, `a4452753`, docs
  only): CONTRIBUTING.md and README.md, with `test-early-stop.sh`'s line and
  README's two env-check lines. **Revert**: `git revert a4452753`.
- **`gates.sh` mirrors the macos-15 row** (round 3): the row with
  `script_tests` and `bash32`, the one this machine is, so its three new
  steps run here as there, `/bin/bash` first on PATH for the rest; the
  Linux row's bash is not run. **Revert**: drop the bash32 step's PATH line.
- **`check-merge.sh` without the suite lock**, as trains 13 to 15 (cargo
  check, fmt and clippy are not the heavy jobs the lock is for); every test
  run, the env checks, the mutants, the captures, the gate, the sweeps and
  the jobs ran under it. **Revert**: run it under the lock.
- **The two sweeps side by side**, in two slots of the lock, so their loads
  match (train 15's base and head ran at very different loads). **Revert**:
  run them one after the other.

## Found on the way

Changed here:
- `src/tui/shell.rs`' "`turn_loop`'s `fetch_sub`, `:3535`" had never named
  the `fetch_sub` (train 11's frame): `:3594` now (join check 4).
- `scripts/check-env-reads.py`'s build-script comment named
  `XFX_BUILD_REVISION` and `git_revision`, which #96 renamed: `XFX_BUILD_ID`
  and `git_build_id` (merge 3).
- `ci.yml`'s `script_tests` comment did not name the env self-test
  TEST-ENV-ISOLATION put on those rows (`622b2865`, `3d1376b2`).
- **No reference scan read a comma list's later numbers** (round 2, opus's
  MUST): `refs_content.py` and `refs_bare.py` matched `path:N` and `:N`
  alone, so `tests/tui.rs:1930,2191,...` had its first number re-pointed and
  ten left. Both now read each `,M` as a ref into the same file (join check
  4); train 14's re-point had left `event_loop.rs:8409`'s list the same way.
- `parity-join.sh`'s last section ended in a Python `SyntaxError` (gpt6's
  NTH): its inline `-c` quoting; the code is `parity_sentences.py` now, 5
  of 5 sentences found.
- **train 7's `token_check.py --self-test` could not plant two of its cases
  on this train's texts** (`token-self-test.txt`): at merge 1 its first
  changed row is one both sides changed, so its "a token a side added,
  dropped" mutated nothing and read MISSED; at merges 2, 5 and 6 the longest
  run a side wrote in the full-screen TUI row is not found whole in the
  merged row by `row.split(" ")`, and its `next(...)` stopped the
  self-test there -- at merge 2 the slash line's 869-word run, which
  PAGE-KEYS-SETTINGS-COLUMN's sentence follows at its 869th word (the token
  check's difference 1), and at merges 5 and 6 ours' 939-word run, whose
  two spaces before `/he` ("`  /he`") `split(" ")` reads as an empty word
  that the run's own tokens do not hold (printed by a reading of the self-test's own
  functions; not an edit). `token_check16.py` fixes the
  two plants and nothing else (its check's output over the six merges is
  `token_check.py`'s byte for byte); its self-test is the one counted.

Not changed here:
- **The two references train 14 left WRONG on purpose are WRONG still**
  (`content-head.txt`): `src/tools/question.rs:303`'s `Cargo.toml:42` and
  `tests/session_compat.rs:57`'s `src/session/event.rs:98-192`, both
  TRAIN14-FOLLOWUPS'; and that queue's `src/tui/shell.rs` pair after the
  too-small test (`shell.rs:6217`, `:6423` now, at `:28019-28020`), whose
  numbers name no too-small assertion -- kept on their frame's text, as
  train 15 kept them.
- **Two comma lists wrong before this train** (round 2; `repoint.py`'s
  LEAVE): `src/tui/event_loop.rs:8409`'s `frame.rs:2631,1156,1626,1696,1758`
  (the head's lines for d3f61f5e's text: `place(...)` :1682, `fn row_text`
  :2874, `fn clip` :2970, the call in `place` rewritten) and
  `scripts/smoke-tui.sh`'s `src/tui/term.rs:56,65` at :251, :1568 and
  :2613 (`RESTORE` is at :97-98). Named for their owner, with train 14's
  two.
- `refs_content.py` reads a commit-pinned reference
  (`<commit>:path:N`) against the head: the one such ref it moved is put
  back (join check 4), and `repoint.py` leaves it by name. A rule for the
  prefix belongs in the check, not in a train.
- The reference scans read `library/std/src/process.rs:1096-1098` (the
  standard library's, in `src/process.rs`' comment) as this repository's
  `src/process.rs`.
- The `/permissions` row's sentence `622b2865` rewrote no longer says fx
  saves the history line as `/permissions full-access`; what xfx saves
  there is not checked here. A round-2 item.
- QUESTION-CTRL-J's review SHOULD 2 (one table of the Ctrl+J spellings for
  the two questions) is still a follow-up, as train 15 named it; the two
  join tests here read `ctrl_j_matrix` and `CTRL_J` as they stand.

## Commits partway along the branch (`prescan.txt`, `midbranch_scan.py`, `final-scan.sh`, `final-scan.txt`)

The brief's check over **every commit of `866747ff..HEAD`**: the units'
own (each stack's range since its merge base with this train's base, as
`prescan.txt` read them before any merge) and this train's, each as `git
show --format=` prints it (a merge as its combined diff), every hit
classified by `midbranch_scan.py` (the account's name and uid read at run
time, never written; no hit printed but its kind and file):

- **No real user name or credential**: NAME 0 and CREDENTIAL 0 in every
  commit and in the range; no commit message holds the account's name
  (`prescan.txt` before the merges, `final-scan.txt` part 2 at the end). The
  train went on.
- **No hit at all**: the brief's count is 0 in every commit of the range --
  the units' 189 and this train's -- and over the range itself
  (`final-scan.txt` parts 1 and 2). There was nothing to classify as a
  placeholder or a scratch root, and nothing to rewrite.
- **The dispatcher's per-commit check** (`final-scan.txt` part 5): every
  commit of `git rev-list --no-merges HEAD --not afeaea7c` (origin/main
  locally; train 15's commits among them), its added lines and its
  message, and every merge commit's own message: 0 hits.
- Beyond the brief's terms, `shapes.py`'s run-name shape (a capture's
  `fxcap.` and six) matches 8 added lines of the range, all
  LOCAL-PATHS-WRAP's `scripts/fxcap/test_capture.py` (`final-scan.txt` part
  1, by file): hand-written fixture names under placeholder roots
  (`/tmp/fxt/...`, the test's own `d`), as the unit committed them.
- "No code after the code head": `final-scan.txt` part 4 reads every commit
  after the code head `a4452753` (round 3's; round 1's and round 2's
  receipt commits sit before the later code commits, so the part reads from
  the last code commit on) and finds no file touched outside this folder; the
  last commit stores `final-scan.txt` alone (`git show --stat` of it).
  Round 1's receipt commits (`37b292a8`, `f2cb84d6`, `7d63b764`) and round
  2's (`34c97078`, `1b3fbdc2`, `ac0b0ff5`) touched this folder alone too
  (`round2-numstat.txt`'s and `join-numstat.txt`'s last lines).

## Redaction

The repository is public. Every file here taken from a gate log, a job, the
logger, a sweep, a capture or a clone went through `redacted.py` (train
15's, unchanged but for its first line): `scripts/fxcap/redact.py`'s
`Redactor.write`, every path under a local root to `<local-path>` or a named
root (`<scratch>`, `<repo>`, `<sweep-head>`, `<sweep-base>`, ...), a
checkout's path cut short to `<checkout>/`, the account's name to `<user>`,
and train 12's tail rule. The scripts take the oracle's path, the suite lock
and the scratch directories as arguments or variables, and spell the
brief's pattern with bracket classes; two example run names in
`byhand.py` are spelt in two pieces, so no file here holds a run name's
shape. **Every commit this train adds was gated on the brief's check**
before it was made (`pathcheck.sh`, under `set -o pipefail` and an explicit
`if`; its output for each merge and the join commit in `cm/`): a merge as
the combined diff `git show` prints of it, read from a probe commit of the
staged tree; any other commit as its staged lines; each commit's message
too; and `shapes.py`'s three shapes over the staged lines and over every
file of this folder. `final-scan.sh` reads every commit and the range at the
end, and the file checks run with the receipt in its final form
(`gates-files.sh`, `gates-files.txt`).

## Files

- Before the merges: `prescan.sh` / `prescan.txt` (the units' own commits),
  `chains.sh` / `chains.txt` (each merge's units, from `git log`).
- Merges: `rowsmerge.py`, `blocks.py`, `byhand.py` (this train's table),
  `semantic.py` (merge 3), `reproduce.sh`, `resolutions.txt`,
  `check-merge.sh` / `check-merges.txt` and its runs in `cm/` (each merge's
  and the join commit's `check-merge.sh` output and `pathcheck.sh` output;
  merge 3's and merge 6's whole `cargo test`, `m3-test.txt`,
  `m6-test.txt`; merge 4's `RootsRewrite`, `m4-roots.txt`), `envreads.txt`,
  `merge_ends.py` / `merge-ends.txt`, `remerge-1.txt` .. `remerge-6.txt`,
  `overlap.py` / `overlap.txt`, `merge_keep.py` / `merge-keep.txt`;
  `pathcheck.sh`, `shapes.py`, `carried.py` (the brief's check before every
  commit).
- `merge-checks.sh`: writes `check-merges.txt`, `resolutions.txt`,
  `merge-ends.txt`, the token check and its controls, `merge-keep.txt`, the
  remerge-diffs, the reference scans, the content checks, the two re-point
  plans and their check, `shared.txt`, `functions.txt`, `moved-edits.txt`,
  `overlap.txt`, `test-counts.txt`.
- The token check: `token_check.py` (train 7's, byte for byte),
  `token_check16.py` (its self-test's two plants fixed), `token_control.py`,
  `word_control.py`, `token-check.txt`, `token-self-test.txt`; `owners.py` /
  `owners.txt`; `parity-join.sh`, `parity_sentences.py` / `parity-join.txt`.
- References: `refs.py`, `refs_all.py`, `refs_short.py`, `refs_bare.py`,
  `refs_prd.py`; `refs-*-pre.txt` (at `622b2865`), `refs-*-repoint1.txt` (at
  `c637ed4c`), `refs-*-head.txt` (at the code head); `refs_content.py`,
  `content.sh` and `content-*.txt` (`content-1ddeda00.txt`: round 2's
  first placing of its cases); `repoint.py`, `repoint-plan.txt`,
  `repoint-plan-2.txt` and `repoint-plan-head.txt` (derived again with
  today's tables, at `622b2865`, `c637ed4c` and the code head),
  `repoint-plan-applied.txt`, `repoint-apply-1.txt`,
  `repoint-plan-2-applied.txt` and `repoint-apply-3.txt` (as run before
  `c637ed4c`, `15c9744b` and `adb809f5`), `repointed.py`, `repointed.txt`,
  `extra_check.py` / `extra-check.txt` (EXTRA's four and TEXT's span).
- Join checks: `mode-case-full.txt` (join check 1); `mutants.sh` /
  `mutants.txt` (join check 2); `shared.py` / `shared.txt`, `functions.py`
  / `functions.txt`, `moved_edits.py` / `moved-edits.txt`, `unit_tests.py`,
  `unit-tests.sh` / `unit-tests.txt`, `unit-tests-notes.txt`,
  `gate-derived.sh` (round 2: those two and `run-absent.txt` from the gate's
  logs), `tests_kept.py`
  and `test_counts.sh` (in `test-counts.txt`), `run_absent.py` /
  `run-absent.txt` (join check 3); `scenarios.sh`, `scenarios.py` /
  `scenarios.txt` (the join checks in the sweeps); `jobs.sh`, `net.py`,
  `jobs-net.txt`, `connect.log` (join check 5).
- Gate: `gates.sh`, `ci_steps.py`, `ci-steps.sh` / `ci-steps.txt`,
  `gates_txt.py`, `gate-files.sh`, `gates.txt`, `gate_counts.sh`,
  `gate-counts.txt`; `gates-files.sh`, `gates-files.txt` (the file checks and
  the reference checks with the receipt in its final form).
- Sweeps: `selection.py`, `selection_units.py`, `selections.sh`,
  `selection-all.txt`, `selection-units.txt`, `selection-sweep.txt`,
  `selections.txt`; `sweep.sh`, `rescore.sh`, `sweep-runs.sh` /
  `sweep-runs.txt`, `sweep-*.md`; `sweep-compare.sh`, `compare.py`,
  `screen_moves.py`, `screen_classes.py`, `screen_partition.py`,
  `gained_rows.py`, `new_scenarios.py`, `banners.py`, `unit_compare.py`,
  `compare-base.txt`, `screen-moves-base.txt`, `screen-classes.txt`,
  `screen-partition.txt`, `gained-rows.txt`, `new-scenarios.txt`,
  `compare-units.txt`, `uncovered.py` / `uncovered.txt`; `sweep-rerun.sh`, `reruns.sh`,
  `rerun_verdicts.py`, `reruns_txt.sh`, `reruns.txt`.
- `final-scan.sh`, `midbranch_scan.py`, `final-scan.txt`; `redacted.py`.
- Round 6: `round6-check.txt` (the env check, the self-test and its RED
  with `CARGO` global again), `round6-numstat.txt`; the round-6 Linux runs
  appended to `round5-linux-clippy.txt`.
- Round 5, PR #102's CI: `round5-causes.txt` (CI's lines for each cause,
  the probes, every RED and GREEN), `linux-clippy.sh` /
  `round5-linux-clippy.txt`, `round5-numstat.txt`, `round5-flake.txt` (the
  first gate's one failure, the session store's flock race),
  `gates-rustup.txt` / `gate-counts-rustup.txt` (the gate through rustup's
  proxies) and `gates-first.txt` / `gate-counts-first.txt` (the first run as
  before); `gate-files.sh` takes a suffix now.
- Round 3, the join of main: `join-prescan.txt` (main's four commits),
  `join_steps.py` / `join-steps.txt` (every step of both sides kept),
  `join-actionlint.txt`, `join-refs.txt`, `join-numstat.txt` (by
  `round2_numstat.py`, its classes widened), `join-readers.sh` /
  `join-readers.txt`; `byhand.py`, `semantic.py` and `reproduce.sh` carry
  merge 7; `gates.sh` mirrors the joined ci.yml.
- Round 2: `round2_numstat.py` / `round2-numstat.txt` (what round 2 changed
  outside this folder, by class); `test_lists.sh` (`REUSE_BASE`),
  `test_lists_txt.sh` / `test-lists.txt`; `gate_counts.sh`'s test-log glob
  fixed (round 1's `gate-counts.txt` had an empty block for the env
  self-test's log). The reviews it answers are the dispatcher's, outside the
  repository.
