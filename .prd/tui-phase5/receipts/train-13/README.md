---
title: 0.0.16 delta -- P5 train 13 receipt (HISTORY-NOTICES-COPY-FX, QUESTION-RESUME, the approval stack into one branch)
type: receipt
tags: [xfx, phase5, fx-0.0.16, train]
---

# P5 train 13: five closed units, three merges, one branch

Branch `train/p5-1110b`, from train 12's final head `f53606d1` on
`train/p5-1110a` (PR #90, open), as the brief set it. Train 12 had not
merged when this train started: `git fetch origin main` then found
`origin/main` at `df46777e` (#89, train 11), `f53606d1` not in it (that
output was not kept; `fetch.txt`, taken again later, finds the same). Each unit was closed by its own
two-engine review and the dispatcher's gate; this train combines them. The
program goal (the user's words): «최신 fx랑 비교해서 ui쪽으로 모든 기능 구현하고
기본적으로 최신 fx ui를 그대로 구현해줘» (fx v0.0.16, pin `481104fd`).

Oracle: fx v0.0.16 (`481104fd`), executable sha256
`8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked
by each sweep before and after it ran, `FX_AUTO_UPGRADE=0`
(`sweep-runs.txt`).

Model: train 12's receipt (`../train-12/`) and the dispatcher's two reviews
of it. Its scripts are copied here, each saying in its first lines what
changed; the new ones say so too. What those reviews found is carried, not
repeated: every figure here rests on a file of this folder, named where it
is used; counts are checked against the lists they count (the screens'
partition, the token control's cases, the rows); the path scans run over
this folder with the brief's eight terms and shapes.py's (`pathcheck.sh`
before every commit, `final-scan.sh` part 3); "no code after the code head"
is `final-scan.txt` part 4, taken at the commit before the last, and
`gates-files.txt`, at the commit before it.

## Units and merges

| # | merge | units it carries (branch @ tip) | base | conflicts |
|---|---|---|---|---|
| 1 | `e776a6b0` | HISTORY-NOTICES-COPY-FX (`fix/p5-history-notices-copy-fx` @ `714f6283`) | `32afd307` | `docs/parity.md`, one block of three rows (forecast) |
| 2 | `9acb47a0` | QUESTION-RESUME (`fix/p5-question-resume` @ `d0e08d8c`) | `32afd307` | `docs/parity.md`, one block, one row (forecast) |
| 3 | `bbdde09c` | APPROVAL-VERTICAL-SPELLINGS (`fix/p5-approval-vertical-spellings` @ `a37a969e`), carrying APPROVAL-HORIZONTAL-KEYS @ `2593f49c` and QUESTION-WORD-LEFT @ `f686b736` | `32afd307` | `docs/parity.md`, two blocks, and the ledger, twelve (forecast) |

Every base is git's merge base. AT-SCOPE-INTERIM, AT-FOLDERS and PAGE-KEYS
are not here (train 14). Every merge tree passed `cargo check
--all-targets` with and without `fault-injection`, `cargo fmt --check` and
clippy (both) before it was committed (`check-merges.txt`). Every
resolution is a script's (`rowsmerge.py`, `blocks.py`), redone by
`reproduce.sh` in a clone to each merge commit's tree, all three "no
difference" (`resolutions.txt`), and checked from both ends -- ours ->
merged is exactly theirs' edits, theirs -> merged exactly ours'
(`rowcheck.py`, `filecheck.py`, `resolutions.txt`). No merge needed a
semantic fix: every tree built as git merged it.

After the merges, in order:

| commit | what |
|---|---|
| `ea35d548` | two join tests, appended at their modules' ends (join checks 1 and 2) |
| `e1e15efd` | 24 references re-pointed, numbers only -- **the code head: the gate, the sweeps and every job ran here**. Its title says "the 24 references the merges moved": 22 were, 2 were wrong at the base already (its body and the section below give the split) |
| `8a2dbd29`, `3e81637f`, `1df65ee0` | the merges' scripts and checks and the join jobs; the gate, the probe and the selection; the sweeps and their compares |
| the receipt's last three | the reruns and this README; then `gates-files.txt` (`gates-files.sh` in a clone at the commit before it); then `final-scan.txt` alone (`final-scan.sh` at the commit before it, which it does not list) |

**No code after the code head**: `final-scan.txt` part 4 lists every commit
after `e1e15efd` through the one before it, each with the files it touches
outside this folder: none; `gates-files.txt` says what `git diff --stat
e1e15efd HEAD` holds outside this folder at its commit: nothing. The last
commit adds `final-scan.txt` alone.

## The conflicts

**Merge 1, `docs/parity.md`, one block** (train 12's `rowsmerge.py`): the
`/model` and `/effort` rows ours' alone (46 and 6 edits since `32afd307`), the
`/provider` row the unit's alone (its 7). `rowcheck.py` true both ways for
all three. The ledger, `interactive.rs`, `provider/mod.rs`, `shell.rs`,
`worker.rs` and `tests/tui.rs`, which both sides changed, merged clean;
`filecheck.py` true both ways for each.

**Merge 2, `docs/parity.md`, one block**, the full-screen TUI row: ours' 96
edits and the unit's 8, none overlapping or meeting; `rowcheck.py` true
both ways. The ledger, `bridge.rs`, `shell.rs` and `worker.rs` merged
clean; `filecheck.py` true both ways.

**Merge 3, `docs/parity.md`, two blocks**: the `ask_user_question` row
(ours' 51 edits, the unit's 2) and the full-screen TUI row (ours' 104, the
unit's 5); `rowcheck.py` true both ways for both.

**Merge 3, the ledger, twelve diff3 blocks** (`blocks.py`, `remerge-3.txt`):
- **eleven `numbers`** (train 11's rule): ours and theirs the same once
  every `:N` is masked, each side having moved one `src/tui/shell.rs`
  number by its own lines: ours kept, then re-pointed. They are
  `:13866`/`:13578` (`answer-drawn-wider-named`), `:16840`/`:16534` twice,
  `:41113`/`:39893` six times, and `:10480`/`:10339` twice (one block also
  `:1747`/`:1840`, the bare `queue_card` number).
- **one `ours`**, a rule added here (`rewrite` with the sides swapped):
  the base's cause `submit-multi-card-whole`, which theirs only re-pointed
  (`:13187` -> `:13267`) and ours replaced with
  `preview-frame-before-the-turn-start` (SUBMIT-CARD-PREVIEW, `8ea0498e`,
  between `32afd307` and the base): ours kept -- the block's three base lines and four of
  ours (the token check: "deleted by ours (theirs changed only its
  path:line numbers), kept so").
- `filecheck.py --numbers-masked`: ours -> merged 0 hunks, theirs -> merged
  exactly ours' 28 (`resolutions.txt`).

**Merge 3's code files merged clean**: `approval.rs`, `input.rs`,
`question.rs`, `shell.rs`. `filecheck.py` reads all four true both ways,
`input.rs` and `shell.rs` each on one end only after an allowance added
here: git's aligner put one kept line (a `}`, a `band(&shell)` call) on the
other side of an insertion, so one end read `+54, the kept line, +1` where
the other read `+21, the kept line, +34`. Hunks at most three unchanged
lines apart are read as one region, old text and new; the two ends then
match. **The allowance's negative control** (`resolutions.txt`): the same
check on a clone of merge 3 with one line of that insertion changed reads
False on both ends, exit 1.

**Overlap** (`overlap.txt`): the nearest hunks of the two sides in each
both-changed code file are 21 base lines apart or more (`bridge.rs` at
merge 2; `input.rs` 31 at merge 3); the ledger's 2 and 26 at merges 1 and
2. Hunks touch only in `docs/parity.md` (each merge) and the ledger
(merge 3): the blocks above.

## The token check (`token_check.py`, `token-check.txt`, `token-self-test.txt`, `token_control.py`, `ledger_numbers.py`)

Train 7's `token_check.py`, byte for byte (`cmp` against `../train-7/`, the
file's first line): **merges 1 and 2 OK; merge 3, one entry handed back
"read by hand"**: `amendment-card-after-stop`, "both sides changed it
differently". Both changed its numbers alone, but one is a bare `:N` after a
path, which its pattern does not mask. `ledger_numbers.py` masks every
`:N`: base, ours, theirs, the merge and the head are **the same entry**,
with `:10277 :1026 :10277 :1658` (base), `:10480 ... :1747` (ours, kept),
`:10339 ... :1840` (theirs) and `:10558 ... :1929` (the head, re-pointed).
The rows both sides changed: merge 2's full-screen TUI row (46 clauses
whole and contiguous, each side's tokens in its order) and merge 3's
`ask_user_question` (24) and full-screen TUI (50) rows; nine more ledger
entries at merge 3 "both sides changed its path:line numbers alone".

**The negative control** (`--self-test`): **15 of 15, 19 of 19 and 25 of 25
found**. `token_control.py` plants the two token cases on every merged row
both sides changed (merge 1 has none): **11 of 11 found** -- four cases on
each full-screen TUI row, three on `ask_user_question`, where it printed no
"a token theirs deleted" case.

## The references (`refs*.py`, `refs_content.py`, `repoint.py`, `repointed.py`)

**At the merges' head `bbdde09c`, before any re-point** (`refs-*-pre.txt`,
five sides: the base `f53606d1` and the three tips): `refs.py` 6 (side,
reference) pairs MOVED; outside the historical plans
(`docs/superpowers/plans/`), `refs_all.py` 7, `refs_short.py` 2,
`refs_bare.py` 4 references; `refs_prd.py` 0.

**The content check** (`refs_content.py`, new; APPROVAL-VERTICAL-SPELLINGS'
`refs.py` is the model): every ledger, `docs/parity.md`, `capfile.py`,
`smoke-tui.sh` and `shell.rs` reference read against **the commit that
wrote its numbers** (git blame, followed through a merge into the parent
that holds the ref, and through a numbers-only edit that left it), the
cited line and two either side compared with the head's. At `bbdde09c`
(`content-bbdde09c.txt`) the ledger has 14 WRONG, `docs/parity.md` 5,
`capfile.py` 1, `smoke-tui.sh` 6, `shell.rs` 1; at the base `f53606d1`
(`content-f53606d1.txt`) 2, 0, 0, 5, 0. APPROVAL-VERTICAL-SPELLINGS' own
`refs.py`, unchanged (`avs-refs.txt`): 13 of 16 wrong at `bbdde09c`, 1 at
the base.

**24 re-pointed in `e1e15efd`, numbers only** (21 lines out, 21 in, each
the same once its numbers are masked; `repoint-plan.txt`): every WRONG
reference the content check, `refs_bare.py` and APPROVAL-VERTICAL-SPELLINGS'
`refs.py` found in the ledger, `docs/parity.md` and `capfile.py` whose text
stands at exactly one place at the head, and the merges' moves in
`smoke-tui.sh` and `shell.rs`:
- **22 moved by the merges**: the ledger's 12 `src/tui/shell.rs` numbers
  (ours', kept at merge 3: `:13866`, `:16840` twice, `:41113` six times,
  `:10480` twice, `:4541`), moved by the units' lines above them;
  `docs/parity.md`'s bare `:1364-1375` after `src/session/store.rs`
  (HISTORY-NOTICES-COPY-FX's deleted lines) and its two rows'
  `scripts/smoke-tui.sh:10462-10477` with their bare `:10514-10524`
  (QUESTION-RESUME's, written on `32afd307`; the base side's 21 lines more
  in `smoke-tui.sh` since then, PASTE-GAP's among them, move them);
  `capfile.py`'s `:15057-15082` and bare
  `:15075-15077`; `smoke-tui.sh`'s `input.rs:1063`; `shell.rs`'s own
  `shell.rs:6070` and bare `:6258`.
- **2 wrong at the base already**, in the ledger:
  `.prd/tui-phase4/receipts/d1/README.md:258` (its text at `:259`) and
  `amendment-card-after-stop`'s bare `:1747`, which train 11's re-point
  (`6ae87955`) wrote where a doc comment stands; APPROVAL-VERTICAL-SPELLINGS'
  `refs.py` table names the line it means (`queue_card`, `:1929`), and
  `repoint.py` takes that text for it, the one entry not mapped from its
  frame.

**After the re-point**, at `e1e15efd`:
- **`repointed.py`: 24 of 24 the same code** (`repointed.txt`).
- **`refs.py` 0 MOVED; `refs_prd.py` 0**; `refs_all.py`, `refs_short.py`
  and `refs_bare.py` **0 outside the historical plans** (16, 69 and 63
  inside them, each "moved" only on a side older than the base).
- **APPROVAL-VERTICAL-SPELLINGS' `refs.py`: 16 checked, 0 wrong.**
- **The content check** (`content-head.txt`, blame ignoring the
  re-point, so each re-pointed ref is read against the frame that wrote
  its numbers): `docs/parity.md`, `capfile.py` and `shell.rs` 0 WRONG; the
  ledger 2: `:1929` (its frame wrote the wrong line, above; `refs.py`'s
  text is there) and `.prd/07-fx-parity-program.md:19` (below);
  `smoke-tui.sh` 5 (below). Read as it is, the ledger has 1.

**The dispatcher's six** (APPROVAL-READINESS-SETTLED's finding): at
APPROVAL-VERTICAL-SPELLINGS' own head `a37a969e`, six ledger refs into
`src/tui/shell.rs` name `:39893`, which holds `screen[at + 1..at + 4],`;
the line they mean, `"  tools: {} advertised ({},",`, is at `:39896`
(`avs-refs.txt`, 6 of 17 wrong at `a37a969e`; `content-a37a969e.txt`, the
same six). **They were moved by `a37a969e` itself, not by these merges**:
its round 3 added a test comment after its `refs.txt` was taken. They never
entered this train: each is one side of one of merge 3's eleven `numbers`
blocks, where ours' `:41113` was kept, and the re-point moved that to
`:42431` (checked above, `repointed.txt`).

## Join checks

Two tests were added (`ea35d548`), each where no unit's tests meet; every
other meeting is covered by the units' own tests, named below and run by
name at the head (`named.sh`, `named.txt`: 31 passed, 0 failed), and by
the gate's suites.

### 1. HISTORY-NOTICES-COPY-FX x QUESTION-RESUME: the resume path

**Each half has its tests; none covers both.** HISTORY-NOTICES-COPY-FX's:
the two carry tests (`carry_tests.rs`), five `tests/sessions.rs` tests and
two pty tests (`tests/tui.rs`) it flipped to "no line" -- 9, all ok at the
head (`named.txt`). Their sessions hold no question batch. QUESTION-RESUME's:
its 13 lib tests of the decoder, the hydration and the shell's rows, and
`a_hostile_log_reaches_the_ui_inert_at_launch_and_from_the_picker` -- 14,
all ok. Their sessions hold no sealed state.

**The join test** `a_resumed_sealed_step_that_asked_replays_its_batch_and_says_nothing_of_its_reasoning`
(`src/tui/worker.rs`): one saved session whose step llmux's `fable` sealed
on the Anthropic wire also called `ask_user_question`, answered. Resumed
through `Resumption::open`, the launch's path: its hydration is the batch's
resolution (`Answered`, one question) and no `Asked` row; its request
history on the Gateway wire, and on the same wire under another model,
holds the call and its answer document and **no notice** (the source of
every `! session:` line); on the authority that sealed it the state goes
too. **Its teeth** (`join_mutants.py`, `jobs-mutant.txt`, a clone, the join
test alone under `timeout -k 10 900`): with HISTORY-NOTICES-COPY-FX's
notice put back, it fails (`worker.rs:19882`, the notices); with
QUESTION-RESUME's decoder made to decode nothing, it fails
(`worker.rs:19860`: `[Asked, Text("DONE")]`, no `Answered`). Clean clone
after.

### 2. QUESTION-RESUME x the question keys, and APPROVAL-VERTICAL-SPELLINGS' decoder

**What the question surface reads, spelling by spelling** (`probe.sh`,
`probe_table.py`, `probe.txt`): 443 spellings -- the arrows, Home and End
plain, `ESC O`, and `CSI 1;m` for every modifier 1-16; kitty's arrows and
keypad codes plain and with every modifier; `ESC b`/`f`/`B`/`F` and their
kitty and xterm forms with every modifier; every C0 byte and DEL. At
QUESTION-WORD-LEFT's head `f686b736` the question reads the decoder's own
output; at the head it reads `unspelled` of it. **The head's decoder reads
113 of them otherwise than `f686b736`'s** -- the approval stack's
reclassified spellings: `ComposerLeft`/`Right` 19 each,
`DraftStart`/`DraftEnd` 48, `ComposerUp`/`Down` 9 each, `AltF` 9 -- **and
through `unspelled` the question reads all 443 exactly as at `f686b736`**.
A cross-commit measurement, so a probe in two throwaway clones, not a
committed test. The units' own tests of each end, ok at the head
(`named.txt`): QUESTION-WORD-LEFT's five (its fold test among them, which
APPROVAL-VERTICAL-SPELLINGS extended to its six actions) and
APPROVAL-VERTICAL-SPELLINGS' decoder test.

**A resumed question's draft**: the join test
`a_question_after_a_resume_that_replayed_a_batch_takes_alt_b_as_before`
(`src/tui/shell.rs`): a session resumed with a batch of four replayed
(QUESTION-RESUME's `resumed_rows`), then a new batch asked -- Alt-b goes
back a question at a choice; in the draft `ab cd`, Alt-b, Shift-Alt-b and
Shift-Alt-f are the word moves (`ab zcd`, `ab yzcd`, `ab yzcdx`), as
QUESTION-WORD-LEFT's own test reads them on a fresh screen. **Its teeth**:
with the question panel reading Alt-b as the plain word left again, it
fails (`shell.rs:53087`, `jobs-mutant.txt`).

### 3. HISTORY-NOTICES-COPY-FX x the approval stack in `shell.rs` (`shared.py`, `shared.txt`)

Train 9's form (each function's sha256, 12 hex digits, per commit), the
functions found from the hunks: every hunk of each side against the base
`32afd307` given to the innermost function it falls in.
- **`shell.rs`: no function both changed.** HISTORY-NOTICES-COPY-FX's one
  (the test `a_history_notice_the_screen_takes_is_painted_after_a_stop_and_dropped_only_with_its_conversation`),
  QUESTION-RESUME's one (`replay_turn`) and the approval stack's six
  (`decide`, `consume_inputs`, `act`, `unshifted` -- renamed `folded`, so
  gone on its side and at the head -- and two tests): **the head holds each
  side's text byte for byte**, 8 of 8. Hunks outside any one function: 0,
  1 and 3 -- QUESTION-RESUME's new tests between two (base `:28877`), the
  stack's new functions and doc lines between two (`:13709`, the renamed
  `unshifted`'s doc and head `:13713-13718`) and its new tests (`:22331`).
- The same over `question.rs` (QUESTION-RESUME's one, the stack's two) and
  `worker.rs` (HISTORY-NOTICES-COPY-FX's four, QUESTION-RESUME's three): no
  function both changed; each side's held byte for byte.
- And the files whole: `filecheck.py` true both ways for `shell.rs` at all
  three merges (`resolutions.txt`).

### 4. `docs/parity.md`, the ledger and `capfile.py`

The token check and its negative control, the re-points, `repointed.py`,
`refs_bare.py` and the content-comparing check: the two sections above.

### 5. Network hermeticity (`net.py`, `jobs-net.txt`, `connect.log`)

Trains 9's to 12's method (FLAKE-CATALOG-EVENT's `connect_logger.py`, a
loopback logger that answers 403 and forwards nothing; the four proxy
variables at it, `NO_PROXY`, `no_proxy`, `ALL_PROXY`, `all_proxy` unset), a
clone at `e1e15efd`, one hold of the lock. **Three test binaries ran under
the logger**: lib and `tui`, as train 12's, and `sessions`, which
HISTORY-NOTICES-COPY-FX changed:

| run | result | CONNECTs logged during it |
|---|---|---|
| `cargo test --locked --lib --no-fail-fast` | 4761 passed, 0 failed, 7 ignored | **2**, both `CONNECT w.example:443` |
| `cargo test --locked --test tui --no-fail-fast` | 188 passed, 0 failed | **0** |
| `cargo test --locked --test sessions --no-fail-fast` | 65 passed, 0 failed | **0** |

**No CONNECT to the Gateway host**; the two `w.example` lines are the
known MCP probe (trains 6 to 12). The other integration binaries ran in the
gate, with no logger. No unit of this train adds a network path: HISTORY-
NOTICES-COPY-FX deletes lines from the request's history building,
QUESTION-RESUME decodes a saved document and draws rows, the approval stack
is keys.

## Commits partway along the branch (`midbranch_scan.py`, `final-scan.txt`)

The brief's check over **every commit of `f53606d1..HEAD`** -- the units'
54 (HISTORY-NOTICES-COPY-FX 13, QUESTION-RESUME 14, the approval stack 27)
and this train's -- each as `git show --format=` prints it (a merge as its
combined diff), every hit classified by `midbranch_scan.py` (train 12's,
the brief's two new terms added: a wrapped scratch root's tail
`-<uid>/t<n>/` and a sweep run name; the account's name and uid read at
run time, never written; no hit printed but its kind and file):
- **No real user name or credential**: kind NAME 0 and CREDENTIAL 0 in every
  commit and in the range; no commit message holds the account's name. The
  train goes on.
- **0 lines hit, in every commit and in the range**
  (`git diff f53606d1..HEAD`): none of the three units' commits holds one
  (all 54 read; each unit's own range from `32afd307` 0 too), and none of
  this train's. So no hit to classify as a placeholder or a scratch root.
- `final-scan.txt` part 2 has the counts at the receipt's last commit; part
  1, each commit this train adds (the merges as their combined diffs and as
  everything they bring), 0 each and the range 0.

## Gate (`gates.sh`, `gates.txt`, `gate-counts.txt`)

`gates.sh` (train 12's, unchanged but for its first lines: ci.yml and
sweep.yml did not change since train 12's head): every step of
`.github/workflows/ci.yml`'s gate job and `sweep.yml`'s two harness tests,
in a `git clone --no-local` at `e1e15efd`, one after another, **the whole
script inside one hold of the suite lock**, each step's exit taken from `$?`
on the line right after its command, the tree clean before and after.
TMPDIR: the harness scratch root itself, a short one (the brief: fourteen
tests fail under a long TMPDIR, TMPDIR-ROBUST). **22 steps, 0 failed, none
run again** (`gates.txt`):

| step | at `e1e15efd` |
|---|---|
| `cargo fmt --check`; clippy; clippy `--features fault-injection` | ok; ok; ok |
| `cargo test --locked --no-fail-fast` | **5685 passed, 0 failed, 7 ignored** |
| the same `--features fault-injection` | **5722 passed, 0 failed, 7 ignored** |
| `check-xfx-identity` / `check-no-stubs` / `check-no-secrets` | ok / ok / ok |
| `check-no-local-paths` / `test-check-no-local-paths` | ok (19856 tracked files; 8 marked fixture lines, 80 placeholder lines) / ok (10 cases, 28 plants) |
| `check-preview-contract` / `check-tui-preflight-cost` / `check-usage-clock` | ok / ok (200x300 full replay 1.78 ms mean; budget 32) / ok |
| `test-check-no-secrets` / `test-gate-env` / `test-preview-contract` | ok / ok (64 cases) / ok |
| `fxcap/test-sweep.sh` / `fxcap/test-capture.sh` | 72 + 27 OK / 107 OK |
| release build; `fault-injection` release build | ok; ok |
| `smoke.sh` / `smoke-tui.sh` | 47 checks, 0 failures / 37 scenarios + the oracle, 1167 checks, 0 failures |

The counts are `gate-counts.txt`'s (each binary's `test result` line and
their sums, from the step logs). Load at the steps' starts 8.2-35.2.
**Every side's tests are in** (`test-counts.txt`, train 7's count: lines
holding `#[test]` under `src` and `tests`): the base 5567;
HISTORY-NOTICES-COPY-FX +0 (5452 against its base's 5452: it flipped and
re-based tests), QUESTION-RESUME +13, the approval stack +16 (QUESTION-WORD-
LEFT +5, APPROVAL-HORIZONTAL-KEYS +5, APPROVAL-VERTICAL-SPELLINGS +6), the
join tests +2: **5598 at the head**, +31. `cargo test` 5685 against train
12's 5654 at the same base code (`../train-12/gate-counts.txt`): **+31**; with fault
injection 5722 against 5691, the same +31. The harness's `def test_`: 206
at the base and the head; the units add none (176 on each, as on their base
`32afd307`); `test-capture.sh` 107 and `test-sweep.sh` 72, as train 12's.

## Sweeps

**Builds** (debug, as trains 4 to 12), each in its own `git clone
--no-local`, each swept with its own tree's harness, scenarios and ledger
(`sweep.sh`, train 12's). fx: the v0.0.16 binary, sha256 `8e6cda2e...`
before and after each sweep, `FX_AUTO_UPGRADE=0`; 40x120, `--jobs 2`; each
sweep inside one hold of the suite lock, each in its own slot, so pairs of
them overlapped: the base's from 11:05:06 (+09:00) for 29m47s, the head's
from 11:31:39 for 30m39s, the extra head's from 12:01:29 and base's from
12:03:00, about 8m each; loads at start 15.1-22.2, peaks 37.9 and 46.3
(their `run.json` lines in `sweep-runs.txt`). Two
selections, each swept at the head and at the base: the brief's
(`selection.txt`) and the extra (`selection-extra.txt`, the decision
below). Each base sweep was rescored with the head's ledger (`sweep.sh
--rescore` on a copy): the pass line the same before and after, and **pair
by pair 0 verdicts moved** in both (`sweep-runs.txt`). Every xfx snap that
shows the banner shows the build swept: `v0.1.0-e1e15ef` on all of the
head's (330 and 68), `v0.1.0-f53606d` on all of the base's (256 and 68;
`sweep-runs.txt`, "banners").

| sweep | scenarios | pairs | pass | failed | timeout |
|---|---|---|---|---|---|
| head `e1e15efd`, the brief's selection (`sweep-head.md`) | 69 | 373 | **197** (52.8%) | 0 | 0 |
| base `f53606d1`, the same (`sweep-base.md`, rescored) | 64 | 296 | 175 (59.1%) | 0 | 0 |
| head, the extra selection (`sweep-head-x.md`) | 18 | 110 | 42 (38.2%) | 0 | 0 |
| base, the same (`sweep-base-x.md`, rescored) | 18 | 110 | 42 (38.2%) | 0 | 0 |

The head's five scenarios more are the units' new ones, 77 pairs:
`p5-question-word-left` 16 of 16 and `p5-question-resume` 4 of 4 pass;
`p5-approval-horizontal-keys` 0 of 29 and `p5-approval-vertical-keys` 0 of
27, each pair unresolved under `approval-grant-scope` alone (a user
decision, as both units registered); `p5-question-resume-ctrlo` 0 of 1,
under `question-resume-ctrlo-detail` alone, as QUESTION-RESUME registered.
So 175 + 2 + 20 = 197.

### Against the base, pair by pair (`compare-base.txt`, `compare-base-x.txt`, `screen-moves-base*.txt`, `screen-classes*.txt`, `screen-partition.txt`, `gained-rows*.txt`)

**The brief's selection, 296 shared pairs: 175 -> 177; 2 up, 0 down**, both
QUESTION-RESUME's fix: `p5-input-question20` `resumed` (unresolved ->
identical) and `resumed.full` (unresolved -> normalized-identical) -- the
twenty resolutions fx replays, which the base did not draw (its 36 and 44
differing rows gone). The base's ledger cause for them,
`question-resume-replay`, is retired at the head. Rerun 3 times on each
tree (`reruns.txt`): **`resumed` and `resumed.full` pass 3 of 3 at the
head, 0 of 3 at the base**; the other nine pairs pass 3 of 3 on both.

**The extra selection, 110 shared pairs: 42 -> 42; 0 moved, 0 rows
gained.**

**The screens** (`screen-partition.txt` ties the counts together):
compare.py finds fx's `.ansi` moved in 30 shared snaps and xfx's in 54;
screen_moves.py reads their `.txt`: fx's 30 are 17 in clock-like or
run-local text alone, 4 beyond it and 9 in colour alone; xfx's 54 are 24, 25
and 5. The 29 beyond it (`screen-classes.txt`): 22 a saved session's id, 3
HISTORY-NOTICES-COPY-FX's notice gone from `provider-carry-back`'s `turn2`,
`turn3` and `-stop`'s `next` (`... so that reasoning was not carried over`;
the rows under it move up), 2 QUESTION-RESUME's resolutions in
`p5-input-question20`'s `resumed` and `resumed.full`, 2 the build id. In
the extra pair compare.py finds fx's `.ansi` moved in 56 and xfx's in 64;
every `.txt` move there is clock-like or run-local text (fx's 57: one
`.txt` moved where its `.ansi` did not) but xfx's 3, each a saved session's
id (`screen-classes-x.txt`). **Rows gained** (`gained-rows.txt`): 3
pairs, `provider-carry-back` `turn2`, `turn3` and `-stop` `next`,
unresolved on both sides ("main" in the file is the base): the notice's
two rows gone, the switch's own divergence (fx's sign-in line, its
cancelled preparation; HISTORY-NOTICES-COPY-FX's README, "Not identical to
fx, and not this unit's") now meets other rows. The extra pair gained none.

### Against each unit's own sweep (`compare-units.txt`, the head's two sweeps read as one)

| unit (its sweep, as its receipt commits it) | shared | pass, unit -> head | moved |
|---|---|---|---|
| HISTORY-NOTICES-COPY-FX (`sweep-head.md`) | 26 scenarios | 96 -> 95 | **1 down**: `resume-paging` 3 -> 2 of 8 |
| QUESTION-RESUME (`sweep-diff.txt`, its head column) | 121 pairs | 87 -> 87 | 0 down; `p5-question-resume` `live` normalized-identical -> identical |
| QUESTION-WORD-LEFT (`sweep-head.md`) | 11 scenarios | 63 -> 65 | 0 down; `p5-input-question20` 9 -> 11 of 11 |
| APPROVAL-HORIZONTAL-KEYS (`sweep-head.md`) | 13 scenarios | 71 -> 73 | 0 down; `p5-input-question20` 9 -> 11 of 11 |
| APPROVAL-VERTICAL-SPELLINGS (`sweep-head.md`) | 7 scenarios | 20 -> 20 | 0 |

- **`resume-paging` (down against HISTORY-NOTICES-COPY-FX's sweep)**: its
  `page1` passed on the unit's sweep and is unresolved at the head -- and
  on the base, the same one row under `session-age`, kind nondeterminism:
  the picker's age column, a clock read about a minute after the sessions
  were made (row 24: fx `now`, xfx ` 1m`, on both; `compare-base.txt` finds
  no move there). The unit's own README says so of its
  pass (its lines 190-193: "It is not this unit's"). Rerun 3 times on each
  tree (`reruns.txt`): `page1` **0 of 3 at the head and 0 of 3 at the
  base**, and every other pair of the scenario the same on both (2 of 8
  pass). The unit's pass was the moment, not its code. Not a drop by the
  train.
- **`p5-question-resume` `live`**: both pass. The unit's committed captures
  of it are raw identical (its `green.txt:35`), as the head's sweep reads
  it; its sweep read normalized-identical on its own run, whose captures it
  did not keep. No code reached that snap between the unit's head and
  this one.
- **`p5-input-question20` against QUESTION-WORD-LEFT and
  APPROVAL-HORIZONTAL-KEYS**: QUESTION-RESUME's two `resumed` pairs, which
  those units' sweeps (on `32afd307`, before it) could not pass.

**So no pair that passes on the base fails at the head**, and against the
units' own sweeps the one pair lower is `resume-paging` `page1`, a clock
read the base fails too. Every other move is a unit's change (QUESTION-
RESUME's resolutions; HISTORY-NOTICES-COPY-FX's notice gone), or run-local
text.

## Decisions, for the user's veto

- **The sweep's selection** (`selection.py`, `selection.txt`): the brief's
  globs, and the scenarios named by the ledger entries a merge changed
  against its first parent -- `P1-carry-notice`'s `provider*` and
  `at-resize-band-at-the-bottom`'s `p5-input-at-resize` (merge 1);
  `question-resume-ctrlo-detail`'s and `question-resume-replay`'s, already
  in the globs (merge 2). Merge 3 changed no entry against its first
  parent: its twelve blocks kept ours'. The twelve entries whose numbers
  the merges moved (merge 3's blocks among them; the re-point changed
  their `xfx` and `source` numbers, which the sweep does not score with --
  `scripts/fxcap/ledger.py` reads them for the file and the range,
  `_sources`) name 18 scenarios more: **swept too, as a second pair**
  (`selection-extra.txt`, `sweep-*-x.md`). Left out: `session-record`'s
  `scenarios = ["*"]`, every scenario, though it sat in six of merge 3's
  twelve blocks. Why that is safe: the merge kept ours' entry and the
  re-point changed its `xfx` numbers alone; a cause's `xfx` is read only by
  `scripts/fxcap/ledger.py:212-219` (a defect names its source lines,
  checked by `_sources`, `:246-263`: the file exists and holds the range)
  and printed in the scoreboard's cause table (`scripts/fxcap/sweep.py:
  890-893`) -- no verdict reads it. The head's sweeps loading the
  re-pointed ledger without an error is `_sources` passing on every
  re-pointed ref. A sweep of every scenario is the fallback if this
  reading is vetoed.
- `blocks.py`'s `ours` rule (merge 3's one block): `rewrite` with the sides
  swapped.
- `filecheck.py`'s joined-region allowance, with its negative control.
- `check-merge.sh` without the suite lock: cargo check, fmt and clippy are
  not heavy jobs by the P5 common brief.
- Two join tests committed, each where no unit's tests meet; the decoder
  half of join check 2 a probe (two commits), not a test.
- Two references wrong at the base re-pointed with the merges' (the
  content check over every ledger ref is the brief's); `:1747` from
  `refs.py`'s table, not a frame.
- `net.py` adds the `sessions` test binary, which HISTORY-NOTICES-COPY-FX
  changed.

## Found on the way, not changed here

- **`.prd/07-fx-parity-program.md:19`**, cited by the ledger's first pin's
  note (`parity-ledger.toml:28`), names a line that no longer holds its
  text, nor does any line of that file (the closest, ratio 0.28,
  `content-head.txt`): the program's point 5 was rewritten after the note
  cited it. Wrong at the base too. Read by hand; owner REF-LINT.
- **`scripts/smoke-tui.sh` holds five references wrong at the base already**
  (`term.rs:32-52` twice, `src/main.rs:44-49`, `src/tui/check.rs:1084`,
  `editor.rs:351`; `content-f53606d1.txt`, the same five at the head):
  older than this train; owner REF-LINT.
- **`refs_content.py` cannot see a ref whose frame already named the wrong
  line** (it reads `SAME`): `:1747` was such a one, found only by
  `refs.py`'s hand table. Both are run here.
- **token_check.py masks the numbers of a `path:N` only**, so a bare `:N`
  both sides re-pointed reads "changed differently" (`ledger_numbers.py`
  reads it); as train 12 found for `.prd/` paths.

## Redaction

The repository is public. Every file here taken from a gate log, a job, the
logger, a sweep or a clone went through `redacted.py` (train 12's,
unchanged but for its first line): `scripts/fxcap/redact.py`'s
`Redactor.write`, every path under a local root to `<local-path>` or a
named root (`<scratch>`, `<repo>`, `<smoke>`, `<sweep-head>`, ...), a
checkout's path cut short to `<checkout>/`, the account's name to `<user>`,
and train 12's tail rule (a token holding a run directory's name or the
harness root's uid part becomes `<local-path>` whole). The scripts take the
oracle's path, the suite lock and the scratch directories as arguments or
variables, and spell the brief's pattern with bracket classes. **Every
commit this train adds was gated on the brief's check** -- its eight terms,
the two train 12 missed (a wrapped scratch-root tail `-<3 digits>/t<n>/`
and a sweep run name) among them -- before it was made (`pathcheck.sh`,
`set -o pipefail` and an explicit `if`): a merge as the combined diff `git
show` prints of it, read from a probe commit of the staged tree (`git
commit-tree`, on no branch), any other commit as its staged lines; and
`shapes.py`'s three shapes over the staged lines and over every file of
this folder. Each receipt commit also passed `check-no-local-paths.sh`.
`final-scan.sh` at the receipt's last commit (`final-scan.txt`).

## Files

- Merges: `rowsmerge.py` (train 12's), `blocks.py` (its `ours` rule
  new), `rowcheck.py`, `filecheck.py` (its joined-region allowance new),
  `reproduce.sh`, `check-merge.sh` / `check-merges.txt`, `resolutions.txt`,
  `remerge-1.txt` .. `remerge-3.txt`, `overlap.py` / `overlap.txt`;
  `pathcheck.sh`, `shapes.py` (the brief's check before every commit).
- `merge-checks.sh`: writes `resolutions.txt`, the token check and its
  control, the remerge-diffs, the reference scans, the content checks,
  `avs-refs.txt`, the re-points' check, the overlaps and `shared.txt`.
- `token_check.py` (train 7's, byte for byte), `token_control.py`,
  `ledger_numbers.py`, `token-check.txt`, `token-self-test.txt`.
- References: `refs.py`, `refs_all.py`, `refs_short.py`, `refs_bare.py`,
  `refs_prd.py` (trains 7's to 12's); `refs-*-pre.txt` (at `bbdde09c`),
  `refs-*-head.txt` (at `e1e15efd`); `refs_content.py` (new) and
  `content-a37a969e.txt`, `content-f53606d1.txt`, `content-bbdde09c.txt`,
  `content-head.txt`; `avs-refs.txt` (APPROVAL-VERTICAL-SPELLINGS' `refs.py`
  at four commits); `repoint.py`, `repoint-plan.txt`, `repointed.py`,
  `repointed.txt`.
- Join checks: `named.sh` / `named.txt`; `jobs.sh` (parts `net`, `mutant`),
  `join_mutants.py`, `jobs-mutant.txt`, `jobs-net.txt`, `net.py`,
  `connect.log`; `probe.sh`, `probe-test.rs.in`, `probe_table.py`,
  `probe.txt`; `shared.py` / `shared.txt`.
- `gates.sh`, `gates_txt.py`, `gates.txt`, `gate_counts.sh`,
  `gate-counts.txt`; `test_counts.sh`, `test-counts.txt`; `gates-files.sh`,
  `gates-files.txt`; `final-scan.sh`, `midbranch_scan.py`, `final-scan.txt`;
  `redacted.py`; `fetch.sh` / `fetch.txt`.
- Sweeps: `selection.py`, `selection.txt`, `selection-extra.txt`,
  `sweep.sh`, `sweep-runs.sh` / `sweep-runs.txt`, `sweep-*.md`;
  `sweep-compare.sh`, `compare.py`, `screen_moves.py`, `screen_classes.py`,
  `screen_partition.py`, `gained_rows.py`, `banners.py`, `unit_compare.py`
  (its `diff` mode new), `compare-*.txt`, `screen-moves-*.txt`,
  `screen-classes*.txt`, `screen-partition.txt`, `gained-rows*.txt`;
  `sweep-rerun.sh`, `reruns.sh`, `rerun_verdicts.py`, `reruns_txt.sh`,
  `reruns.txt`.
