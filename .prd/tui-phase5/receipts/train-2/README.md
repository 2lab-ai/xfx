---
title: 0.0.16 delta -- P5 train 2 receipt (seven closed units into one branch)
type: receipt
tags: [xfx, phase5, fx-0.0.16, train]
---

# P5 train 2: seven closed units, one branch

Branch `train/p5-1107b`, from `origin/main` `4e68d4d3`; `origin/main`
`6066d67e` (#75, FLAKE-STREAMMID) merged in after the six unit merges.
Every unit was closed by its own two-engine review; this train combines
them and re-designs nothing. Where two units met in the same code the merge
keeps both, and this README says how, conflict by conflict.

Oracle: fx v0.0.16 (`481104fd`, executable sha256
`8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked
by `capture.sh` before and after every run, `FX_AUTO_UPGRADE=0`).

## Merges

Each is `git merge --no-ff`, in this order. "Base" is git's merge base.

| # | unit | branch @ tip | merge | base | conflicts |
|---|---|---|---|---|---|
| 1 | L-U4 | `docs/p5-cli-ledger` @ `b6f155ef` | `9661799b` | `b0a78fea` | `docs/parity.md` |
| 2 | L-U3 + L-U3b/c/d | `feat/p5-tui-unsaved` @ `73137e00` | `3577bddb` | `b0a78fea` | `docs/parity.md`, the ledger |
| 3 | T-U3-TC + b/c/d | `feat/p5-inline-code-truecolor` @ `b0c5dc6a` | `ca616c38` | `02b4619b` | `docs/parity.md` |
| 4 | T-U6 + b/c (`8837634a`), WIDTH-DISPUTED + r2/r3/d | `fix/p5-width-disputed` @ `b12f7794` | `a3515eaa` | `039fb5c8` | `docs/parity.md` |
| 5 | T-U1-SAFE + r2/c | `fix/p5-ctrlo-safe-rows` @ `9cdc899e` | `333df935` | `56979c59` | `docs/parity.md` |
| 6 | CALLROW-SAFE + r2/c/d | `fix/p5-call-row-spelling` @ `65022336` | `f2a6d646` | `151dde9f` | `src/tui/toolgroup.rs`, `docs/parity.md` |
| 7 | `origin/main` (#75) | `6066d67e` | `587f94ee` | `4e68d4d3` | none |

After them: `37f5275a` (a ledger line #75 moved), `554c234f` (the submit
snaps held), `8cd00233`, `bca3e793` and `97644085` (the ledger, below),
`f4af7b0a` (this receipt), and round 2's `5f2d0b88` (the MCP row's bound,
two tests, the anchored selectors; "Round 2" below) and its receipt commit.
The main sweep's binary is `bca3e793`'s, and the ledger and receipt commits
after it change no compiled file. Round 2 changes only an MCP row's reason
(the MCP arm, `server_spelled`, a new function in `src/mcp/mod.rs`), so
`p5-mcp-*` and `p5-input-mcp` were swept again at `5f2d0b88`. Every
scoreboard here is rescored with the ledger as committed (`sweep.sh
--rescore`).

Where a `docs/parity.md` hunk held whole table rows, each row was merged
against the merge base by itself: a row one side changed is that side's; a
row both changed is merged word by word (`git merge-file` over one word per
line), and the merged row is checked to be each side's row plus the other
side's change.

### 1. L-U4 (`docs/p5-cli-ledger`)

- `docs/parity.md`, one hunk holding two rows. L-U4 changed `full-screen TUI`
  (L3, the retry re-verified at `481104fd`); `main` had changed `full
  transcript (C-o)` since `b0a78fea` (T-U1/U2/U5). Each row changed on one
  side only, so each side's row is taken. Checked: the merged file's diff
  from `4e68d4d3` is exactly L-U4's diff from `b0a78fea`.

### 2. L-U3 + L-U3b/c/d (`feat/p5-tui-unsaved`)

- `docs/parity.md`, three hunks, resolved row by row against `b0a78fea`:
  - one side only: `session` and `ask --add-dir` (L-U3), the three MCP rows
    (`main`'s MCP v2);
  - both sides, both pure additions, merged word by word with `git
    merge-file` (one word per line), with no conflict:
    - `sessions`: L-U4's `sessions convert` sentences, L-U3's "With no usable
      `HOME` it lists nothing";
    - `profile home ~/.xfx`: `main`'s `--project` text, L-U3's no-`HOME`
      paragraph.
- `scripts/fxcap/parity-ledger.toml`: both sides appended `[[cause]]`s at the
  end of the file (`main`: `U7-cancelled-glyph`, `U5-transcript-local-time`,
  `mcp-call-error-text`; L-U3: `launch-refusal-row`). All four kept.

### 3. T-U3-TC + b/c/d (`feat/p5-inline-code-truecolor`)

- `docs/parity.md`, one hunk: `notifications and status line` (`main` only)
  and `colored and hyperlinked TTY output` (T-U3-TC only). Row by row.
- `src/tui/toolgroup.rs`, `theme.rs`, `event_loop.rs` merged without a
  conflict. T-U3-TC's `docs/parity.md:241` citation is still the
  colored-TTY row.

### 4. T-U6 + b/c, then WIDTH-DISPUTED + r2/r3/d (`fix/p5-width-disputed`)

- `docs/parity.md`, one hunk: `full transcript (C-o)` (WIDTH-DISPUTED/T-U6
  only) and `full-screen TUI`, which both changed: L-U4's L3 sentence, and
  T-U6's "A prompt's card is written in the frame that clears the composer"
  and the review's lossier-forms sentence. Merged word by word, no conflict.
- `src/tui/full_transcript.rs`, `wrap.rs`, `drawn_wider.rs`,
  `approval_view.rs`, the ledger and `.prd/.../transcript-wrap/` (T-U6's
  round 4 of `check.py`, built on `main`'s round 3) merged without a
  conflict.

### 5. T-U1-SAFE + r2/c (`fix/p5-ctrlo-safe-rows`)

- `docs/parity.md`, one hunk: `full-screen TUI` (ours only), and `full
  transcript (C-o)`, which both changed: T-U6b's "A change's lines are
  written as fx v0.0.16 writes them" and WIDTH-DISPUTED-d's "**Open
  (T-U1-SAFE)**" sentence on our side; T-U1-SAFE's "Every one of them is the
  model's ..." and its "**Open, CALLROW-SAFE**" sentence on theirs. Merged
  word by word, no conflict. (Both open sentences are closed later: the
  first in the ledger commit below, the second at merge 6.)
- `src/tui/full_transcript.rs`, `wrap.rs` and `prose.rs` (the overlap with
  WIDTH-DISPUTED), `shell.rs`, `sweep.py`, `ledger.py`, `ci_sweep.py` and
  the ledger merged without a conflict.

### 6. CALLROW-SAFE + r2/c/d (`fix/p5-call-row-spelling`)

CALLROW-SAFEd (`65022336`, `e8986a68` plus one commit) had landed, so the
branch was merged at `65022336`. Its base here is T-U1-SAFE round 1
(`151dde9f`), so the conflicts are T-U1-SAFE r2/c and `main` against
CALLROW-SAFE.

- `src/tui/toolgroup.rs`, five hunks, every one `main`'s MCP v2 (#72,
  merged after CALLROW-SAFE's base) against CALLROW-SAFE; T-U3-TC's lines
  merged clean:
  1. imports: `main` added `mcp`, CALLROW-SAFE dropped `safe_one_line`,
     which nothing uses any more: `use crate::tools::{mcp, Activity,
     LineChanges, Outcome, Shown};`.
  2. `main`'s `calls_mcp` and `runs_a_command` kept, with CALLROW-SAFE's doc
     line for `target`.
  3. `target()`: `main`'s MCP arm first (the server's name through
     `crate::mcp::terminal_safe`, unchanged), then CALLROW-SAFE's
     `spelled(target, activity)` for every other call.
  4. `phrase()`, a failure: `main`'s MCP arm (its reason through
     `server_spelled`), then CALLROW-SAFE's arm for every other call
     (flatten, then test for empty). The two differ in their spelling, not
     in their test. As merged, the MCP arm tested the reason before spelling
     it, but the channel had flattened it already (`bridge.rs:515`), so a
     reason of white space alone showed `Failed mcp_x` with no `: ` on the
     live path either way. Since round 2 (below) the MCP arm tests after its
     own spelling, fx's rule, whatever path reaches it.
  5. tests: both added after the same test; `main`'s MCP tests, then
     CALLROW-SAFE's.
- The seam with no conflict, which round 2 (below) had to fix. CALLROW-SAFE's
  `bridge.rs` spells every call's argument and reason as it crosses
  (`spelled_shown`, `toolgroup::flattened`: fx's `encodeTerminalSafe` set
  and U+061C), MCP calls included. `main`'s MCP row then spells the name
  with `crate::mcp::terminal_safe` and the reason with `server_spelled`,
  whose set is wider: `approval_view::command_text` also names the
  default-ignorable, drawn-wider and uncounted code points (U+FE0F, U+00AD,
  U+3164, ...) that the channel leaves raw.
  - For a character both name, both write the same token, and neither
    touches printable ASCII, so a `\` is never spelled twice and nothing raw
    gets through.
  - An MCP name is an alias of `[A-Za-z0-9_-]` (`src/mcp/names.rs`), which
    neither pass changes.
  - A reason could be cut wrong: the row's 256-byte bound ran a character at
    a time over the channel's spelled text, so where the wider set carried a
    reason past the bound the cut could fall inside one of the channel's
    escapes (RED below: the row ended `\x1b\...`). Round 2 cuts between
    tokens.
- `docs/parity.md`: `full-screen TUI` merged word by word (L-U4, T-U6, and
  CALLROW-SAFE's "each argument spelled as fx spells a command's row"). `full
  transcript (C-o)`: one word-level conflict, at the open item CALLROW-SAFE
  closes. Base `**Open:** the`; our side `A spelling is not kept whole: ...
  each of them printable ASCII. **Open, CALLROW-SAFE (before train 2
  merges):** the`; CALLROW-SAFE `The`. Kept T-U1-SAFE's sentence and took
  CALLROW-SAFE's `The`.
- The ledger merged without a conflict but kept `ctrl-o-call-row-spelling`:
  T-U1-SAFE r2/c had changed it (kind defect to kind unit, with an expiry)
  where CALLROW-SAFE deleted round 1's. Deleted here (below).
- `src/tui/full_transcript.rs`: a T-U1-SAFE r2 test comment named that
  cause; it now names CALLROW-SAFE's test,
  `no_calls_row_hands_the_terminal_a_control_or_a_mark_at_any_width`.
- `p5-transcript-safe`'s `.cap`, turns, xfx turns and `safe.txt` merged to
  T-U1-SAFE r2/c's, which add U+061C and U+2067; CALLROW-SAFE measured round
  1's (ledger, below).

### 7. `origin/main` `6066d67e` (#75)

No conflict. The ledger keeps #75's deletion of `stream-position` and every
train entry. `tmux-dead-pane`'s `capture.sh` line moved again (below).

**The scenarios that snapped mid-stream.** A static walk of every `.cap`
the train adds or edits, for a snap or a key less than 2 s after an Enter
with no `held` in between, finds one train scenario that does it while a
turn streams: `p5-transcript-submit-multi` (T-U6), `submit-0.15`, 0.15 s
after Enter. It shares `p5-transcript-submit-turns.json` with `main`'s
`p5-transcript-submit`, whose `submit-0.15` at this head was unresolved
with no cause (row 36: fx `~ Generating (<s>) (↑4 ↓4)`, xfx `~ Thinking
(<s>) (↑4 ↓0)`). `554c234f` holds the second answer after its first chunk
(`holdAt` 1) and both scenarios snap at `held` + 1 s, then `release`, as #75
holds the others; the label stays, so the pairs keep their names. Re-swept:
9 of 9 pairs pass, and the gateway logs `hold 1 after 1 event(s)` on all four
runs. The other flagged train scenarios (`p5-lifecycle-relhome-image`,
`-unsaved`) type or snap 1-1.5 s after the Enter of a slash command or a
one-chunk, 50 ms turn.

**What relied on `stream-position`.** Rescored with the train's ledger, one
train scenario had rows only that cause covered: `p5-transcript-diffctl`'s
summary row (`<wall> (↑3 ↓80)` against `(↑3 ↓120)`). It is not where a
stream stands: T-U6 gave xfx its own turns, which read `ctl.txt` before the
edit (xfx's `edit_file` refuses a file it has not read in full), so the
answers differ. No hold makes them the same. The cause
`diffctl-xfx-reads-first` (kind scenario) names those rows and the extra
`Read ctl.txt` rows, which were unattributed on T-U6's branch too.

## Ledger

Every entry from every side is kept, except the two the brief closes. Three
are added, each to attribute rows the merge put side by side; none changes
a unit's decision.

**Deleted.**
- `ctrl-o-call-row-spelling` (cause; T-U1-SAFE's, kind unit, expiring
  2026-11-10, owned by CALLROW-SAFE): CALLROW-SAFE closes it (merge 6).
- `ctrl-o-result-format-raw` (cause, kind defect, owner T-U1-SAFE, added by
  WIDTH-DISPUTED-d), in `8cd00233`. Its resolution: "T-U1-SAFE's branch
  lands ... this cause, which then covers nothing, is deleted in that
  merge." The check, on a sweep of `p5-transcript-diffodd`, `-diffwide2`,
  `-safe`, `-safetab`, `p5-call-row-*` and `-diffctl` (the code of this
  head, `f2a6d646`'s build):
  - the rows it named on `p5-transcript-diffodd` `full` (read rows 3 and 4:
    fx `bidi \u{202e} reversed`, `zero\u{200b}width \u{feff} bom`) are fx's:
    all three `diffodd` pairs are normalized-identical;
  - its `xfx_row` (a rail row holding U+200B-200F, U+2028-202E,
    U+2060-206F or U+FEFF raw) matches no xfx row of any capture of
    `diffodd`, `diffwide2`, `safe` or `safetab`;
  - its `fx_row` still meets one fx row that differs, `diffwide2` `full` row
    9, read row 2. xfx's row (12) now has `zwj` U+FFA0 `\u{200d}y` as fx's
    does; what still differs is the three blank cells fx's frame leaves
    before `x`, which `frame-variation-selector-anchor` names (and
    `ctrl-o-diff-row-clipped` covers that snap first);
  - rescored with the ledger before and after the deletion, no pair's
    verdict, causes or unattributed rows change (28 pairs).
  `frame-variation-selector-anchor`'s note on row 2's second reason now
  says it is closed, and `docs/parity.md`'s "**Open (T-U1-SAFE)**" sentence,
  this cause's twin, is removed.

**Added.**
- `call-row-arabic-letter-mark` (divergence, unsafe-to-copy, owner
  CALLROW-SAFE, expires 2026-12-04 as `call-row-raw-argument` does).
  CALLROW-SAFE's reviewed decision (`call-row-raw-argument`'s reason) is to
  name U+061C `\u{061c}` on each of a call's rows, heading included, where
  fx writes it as itself. T-U1-SAFE's round 2 put U+061C into
  `p5-transcript-safe`'s command after CALLROW-SAFE forked from round 1, so
  that scenario's command row (the transcript's, and Ctrl-O's heading)
  differs from fx's there. Checked on `end` and `end.full`: the one
  differing row is row 8, and with `\u{061c}` read back as U+061C xfx's row
  is fx's text. Its selectors name that whole row, `^` to `$`, one form per
  side (anchored in round 2). With it, `end`/`end.full` are
  approved-divergence and `full`/`full-pgup` have no row without a cause.
- `diffctl-xfx-reads-first` (cause, kind scenario, owner T-U6): see merge 7.
- `upgrade-update-alias` (cause, kind defect, owner "unowned (the upgrade
  command, deferred ...)", as `subagent-tool` names the deferred subagent
  tool): L-U4's `p5-cli-ledger-update-help` records fx's help for `update`,
  `upgrade` and `help update`; its receipt took fx's captures alone. Swept
  against xfx, every snap is xfx's `error: unrecognized subcommand` against
  fx's help (`whole_snap`).

**Re-pointed** (each checked: the cited line's text in the frame the entry
was written in equals the text at the new line):

| entry | cited (frame) | now | moved by |
|---|---|---|---|
| `tmux-dead-pane` | `scripts/fxcap/capture.sh:756` (`main`) | `:786` | L-U3 (+8), #75's `held`/`release` (+22); `main`'s own `:756` is stale since #75 |
| `P1-carry-notice` (why) | `src/session/store.rs:992` (`main`) | `:1000` | L-U3 |
| `session-record`, `transcript-page-offset`, `full-transcript-shift` | `src/tui/shell.rs:36836` (`main`) | `:37272` | L-U3, T-U1-SAFE, CALLROW-SAFE |
| `safe-page-edge` | `src/tui/shell.rs:32298` (T-U1-SAFE) | `:37272` | stale in its own frame (at `9cdc899e` `:32298` is `shell.copies = ...`); copied from its base `56979c59`, whose siblings train 1 re-pointed in `fd3f5f02`; pointed at its siblings' line, the `* session:` record's `tools: {} advertised` |
| `stats-bytes` | `src/tui/shell.rs:1584` (`main`) | `:1583` | L-U3, CALLROW-SAFE |
| `banner` | `src/tui/shell.rs:185` (`main`) | `:184` | CALLROW-SAFE |
| `submit-multi-card-whole` (why) | `src/tui/shell.rs:11812` (T-U6) | `:12192` | `main`, L-U3, CALLROW-SAFE |
| `ctrl-o-diff-row-clipped` | `src/tui/full_transcript.rs:1958` (WIDTH-DISPUTED) | `:2240` | `main`, T-U1-SAFE, CALLROW-SAFE |
| `ctrl-o-command-marks` | `src/tui/full_transcript.rs:486` (T-U1-SAFE) | `:492` | WIDTH-DISPUTED, CALLROW-SAFE |
| `call-row-raw-argument`, `call-row-page-edge` | `src/tui/toolgroup.rs:600` (CALLROW-SAFE) | `:657` | `main`'s MCP v2, T-U3-TC |
| `call-row-source-cut` | `src/tui/toolgroup.rs:648` (CALLROW-SAFE) | `:705` | the same |
| `cancelled-marker` | `src/tui/toolgroup.rs:583` (`main`) | `:618` | CALLROW-SAFE |
| `command-size-bound` | `src/tools/spec.rs:219` (CALLROW-SAFE) | `:214` | `main`'s MCP v2 |
| `docs/parity.md` `sessions` (L-U4) | `src/session/store.rs:1356-1367` | `:1364-1375` | L-U3 |

The ledger's twelve-odd `docs/parity.md:<row>` citations are unchanged: no
merge adds or removes a row above them (L-U4's section is at the end of the
file), and `:237`/`:238` are still `full-screen TUI` and `full transcript
(C-o)`. Method: for every `path:line` in the ledger and in each
`docs/parity.md` row, the frame is the first of `origin/main`, the train's
previous head and the unit's tip whose text holds the citation with its
context; the line is mapped through `git diff --no-index -U0` from that
frame's file to the merged one, and a citation whose lines moved unchanged
is re-pointed. None was edited in place except
`ctrl-o-result-format-raw`'s (`railed`, changed by T-U1-SAFE), which is
deleted.
- `launch-refusal-row` (L-U3's decision cause) covers rows 2-3 and now 41:
  aligned, xfx's `r00-resume.full` keeps one blank row more at the bottom,
  the same one-row shift. The train's xfx captures of every L-U3 pair are
  L-U3's own (below), and L-U3's harness and ledger, run from a clone at
  `73137e00` over this sweep, classify the pair the same way, row 41
  included.
- `edit-not-utf8` (cause, kind defect, owner "unowned (edit_file on a file
  that is not UTF-8 ...)"): T-U6's `p5-transcript-diffbad`, where fx v0.0.16
  edits a file that is not UTF-8 and xfx refuses it, recorded by T-U6 in
  `docs/parity.md:158` with no ledger entry (`whole_snap`, as
  `subagent-tool`).

## Gates

Per merge, on the merge commit, with `TMPDIR=<tmp>/train2`:

| after | head | fmt | clippy | clippy, fault-injection | identity | stubs | secrets | ledger loads, sources checked |
|---|---|---|---|---|---|---|---|---|
| 1 L-U4 | `9661799b` | ok | ok | ok | ok | ok | ok | -- |
| 2 L-U3 | `3577bddb` | ok | ok | ok | ok | ok | ok | ok |
| 3 T-U3-TC | `ca616c38` | ok | ok | ok | ok | ok | ok | ok |
| 4 WIDTH-DISPUTED | `a3515eaa` | ok | ok | ok | ok | ok | ok | ok |
| 5 T-U1-SAFE | `333df935` | ok | ok | ok | ok | ok | ok | ok |
| 6 CALLROW-SAFE | `f2a6d646` | ok | ok | ok | ok | ok | ok | ok |
| ledger, scenarios, #75 | `8cd00233` | ok | ok | ok | ok | ok | ok | ok |

No merge needed a fix to build: every one compiled and linted at its merge
commit (merge 6's `toolgroup.rs` resolution included).

After all merges, one at a time, after an idle check (no other `cargo` or
xfx test process):

| gate | head | result |
|---|---|---|
| `cargo test --locked --no-fail-fast` | `f2a6d646` | 5153 passed, 0 failed, 7 ignored |
| `cargo test --locked --no-fail-fast` | `8cd00233` | 5153 passed, 0 failed, 7 ignored |
| `cargo test --locked --features fault-injection --no-fail-fast` | `8cd00233` | 5190 passed, 0 failed, 7 ignored |
| `scripts/fxcap/test-sweep.sh` | `8cd00233` | 50 + 26 tests OK |
| `scripts/fxcap/test-capture.sh` | `8cd00233` | 42 tests OK |
| `scripts/fxcap/test-sweep.sh` (the ledger changed after `8cd00233`) | `97644085` + this receipt | 50 + 26 tests OK |
| `scripts/check-xfx-identity.sh` / `check-no-stubs.sh` / `check-no-secrets.sh`, `cargo fmt --check`, the ledger loading with its sources checked | `97644085` + this receipt | ok (12052 tracked files) / `check-no-stubs: ok` / ok (12052) ; fmt ok ; 22 divergences, 47 causes |

`git diff f2a6d646 8cd00233 -- src tests build.rs Cargo.toml Cargo.lock`
is empty (#75 and the train's own commits after merge 6 are scenarios, the
ledger and `docs/parity.md`).

## Sweep

**Builds.** Head: `bca3e793`, a debug build (`x𝒇x v0.1.0-bca3e79 [debug]`
on row 1 of every xfx snap, sha256 `5aa36d71…`). Base: `origin/main`
`6066d67e`, built in a scratch clone (`v0.1.0-6066d67`). fx: the v0.0.16
reference binary, sha256 `8e6cda2e…`, `FX_AUTO_UPGRADE=0`.

**Runs.**
- `scripts/fxcap/ci-sweep.sh --base 6066d67e --pin v0.0.16 --jobs 2`
  (`ci-report.md`): its selection (core, every scenario the train adds or
  edits, every scenario a receipt it adds names), 50 scenarios, 200 pairs:
  111 pass at the head, 83 new, **0 regressions**; the base swept the 7
  scenarios with a pair it snaps and the head does not pass, and every such
  pair fails there too. `sweep-head.md` is the head's scoreboard rescored
  with the committed ledger.
- The brief's globs (`p5-transcript-*`, `p5-toolgroup-*`, `p5-toolstop-*`,
  `p5-inline-code-*`, `p5-lifecycle-*`, `p5-call-row-*`, `p5-cli-*`) are 57
  scenarios; 39 are in that selection. The other 18 (`p5-lifecycle-`
  `compactfail`, `compactok`, `login`, `login2`, `osc`, `retry`, `retryok`,
  `suspend`, `update`, `v1bad`, `v1resume`; `p5-transcript-` `anchor`,
  `anchormid`, `question`, `resize`, `tools`, `tz`, `tzsys`), which no train
  unit touches, ran with `sweep.sh` on both builds (`sweep-extra.md`,
  `sweep-extra-base.md`): 95 pairs, 22 pass on each, **every pair the same
  verdict on both**, and the rows no entry names the same on both but one
  (`p5-lifecycle-retry` `r04a-esc-armed`, fx's own countdown, `in 7s`
  against `in 6s`).
- CALLROW-SAFE's receipt has 40x60 pairs, so `p5-transcript-safe` and
  `p5-call-row-safe` ran at 40x60 too (`sweep-40x60.md`).

### Against each unit's receipt

"Pass" is identical or normalized-identical, the sweep's own measure.
Every pair a receipt passes, the train passes, but CALLROW-SAFE's two
`p5-transcript-safe` `end` pairs at 40x120, explained in the table.

| unit | pairs | receipt | train (`bca3e793`, committed ledger) | xfx captures vs the unit's own |
|---|---|---|---|---|
| L-U4 | `p5-cli-ledger-update-help`, 3 | fx's captures only; no pair | 3 unresolved, every row `upgrade-update-alias` (the deferred `upgrade`) | -- (fx only) |
| L-U3 + b/c/d | `p5-lifecycle-relhome*`, 17 | 15 normalized-identical, `u02-status` cascade, `r00-resume.full` unresolved (`launch-refusal-row`) | **the same**: 15, cascade, `r00-resume.full` (`launch-refusal-row`, row 41 included) | 17 of 17 the same |
| T-U3-TC + b/c/d, truecolor | the 4 `-tc` inline-code, 4 tool-group, 4 stopped-call scenarios, 36 | 2 normalized-identical, 34 unresolved, every row attributed (`truecolor-grey-notation`, `toolgroup-retint-256`, `cancelled-marker`) | **the same**: 2, 34, no row without a cause | 36 of 36 the same |
| T-U3-TC, 256 colours | T-U3's 6 scenarios, 22 | 18 identical, the 4 Ctrl-O pairs differ (entry times, session record) | 18 pass (4 identical, 14 normalized), the same 4 differ -- `p5-transcript-md`/`-md-light` `full`/`full-pgup`, rows the same at the base | -- |
| T-U6 + b/c | `diffctl`, `diffodd`, `diffask`, `diffbad`, `submit`, `submit-multi`, `tallfirst`, 25 | `check.py`: 34 claims hold; its own captures with this ledger: 14 pass | **17 pass**: + `diffask` `full`, `diffodd` `full` (T-U1-SAFE's read rows) and `submit-multi` `submit-0.15` (held); 8 unresolved, attributed: `diffctl` ×4 (`diffctl-xfx-reads-first`), `diffbad` ×3 (`edit-not-utf8`), `diffask` `ask` (`approval-grant-scope`) | 22 of 34 the same; the 12 others below |
| WIDTH-DISPUTED + r2/r3/d | `diffwide`, `diffwide2`, 6 | `green.txt`: 11 claims hold; its own captures: 4 pass, both `full` attributed (`ctrl-o-diff-row-clipped`) | **the same**: 4, both `full` attributed | 5 of 6 the same |
| T-U1-SAFE + r2/c | `p5-transcript-safe`, `-safetab`, 6 | 1 normalized-identical, 5 unresolved (4 of them `ctrl-o-call-row-spelling`, CALLROW-SAFE's) | 1 normalized-identical; `safe` `end`, `end.full` **approved-divergence**; 3 unresolved, every row attributed | 1 of 6 the same |
| CALLROW-SAFE + r2/c/d, 40x120 | `p5-call-row-long`, `-safe`, `p5-transcript-safe`, 7 | `call-row-*`: 1 approved, 2 unresolved attributed; `p5-transcript-safe` (round 1's scenario) `end`, `end.full` normalized-identical, `full`, `full-pgup` attributed | `call-row-*` **the same**; `p5-transcript-safe` `end`, `end.full` **approved-divergence** (`call-row-arabic-letter-mark`): T-U1-SAFE's round 2 put U+061C into the scenario, which CALLROW-SAFE names where fx does not; with `\u{061c}` read back as U+061C the row is fx's | `call-row-*` 3 of 3 the same |
| CALLROW-SAFE, 40x60 | `p5-call-row-safe`, `p5-transcript-safe`, 6 | `call-row-safe` `end` approved, `full` attributed; `transcript-safe` `end`, `end.full` normalized-identical, `full`, `full-pgup` attributed | `call-row-safe` **the same**; `transcript-safe` `end`, `end.full` **normalized-identical** (the row is cut before the mark at 60 columns); `full`, `full-pgup` unresolved, and their U+061C rows, wrapped at 60 columns, are pieces no selector names (below) | `call-row-safe` 2 of 2 the same |

**The units' own captures** (`units.txt`), each difference with its cause:
- T-U6's `diffask`/`diffctl` `full` rows 20-21 and `diffodd` `full` rows
  16-19: the read rows, spelled now (`before\x1b[2Jcleared`, `second
  \u{009b}line`, `bidi \u{202e} reversed`, `crlf line\x0d`, `del \x7f`): T-U1-SAFE,
  and fx's (both `full` pairs pass). `diffwide` `full` rows 1-12:
  WIDTH-DISPUTED's margin, the same as WIDTH-DISPUTED's own capture.
  `submit`/`submit-multi` `submit-0.15` row 36: the activity row at the held
  chunk (`554c234f`). `frame*`/`wireend` are `check.py`'s, from the wire.
- WIDTH-DISPUTED's `diffwide2` `full` row 12: `zwj` U+FFA0 `\u{200d}y`,
  T-U1-SAFE's spelling (this is the row `ctrl-o-result-format-raw` named).
- T-U1-SAFE's `safe` `end`/`end.full` row 8 and `full`/`full-pgup` heading:
  the call row, spelled by CALLROW-SAFE. `full` row 1, `full-pgup` rows 5-6,
  `safetab` `full` rows 2-3: `tools: 12 advertised`/`+4 more` for 13/+5:
  `main`'s MCP v2 (#72, `mcp_features` deferred) after T-U1-SAFE's base.
- CALLROW-SAFE's `p5-transcript-safe`, both sizes: round 1's scenario
  against round 2's -- the command `: '...G\u{061c}H\u{2067}I'` for `printf
  '%s\n' '...G'`, its argument, output and read rows with the two marks --
  and the tool count above.

**Rows no entry names**, all in scenarios no train unit adds or edits, and
the same at the base: `h1-resize` `small`/`large` (core, xfx's repaint after
SIGWINCH), `p5-transcript-cancel` ×7 (fx's `shell` tool and the mode row),
`p5-transcript-md`/`-md-light` `full`/`full-pgup`, `p5-transcript-records`
`start` (row 5's launch notice), and 41 pairs of the 18 extra scenarios
(lifecycle research scenarios xfx has no surface for -- `/login`, OSC
progress, retry banners, `update`, the v1 store -- and
`p5-transcript-question` `asked`/`narrow`). One train pair at a size the
brief does not ask for: `p5-transcript-safe` `full`/`full-pgup` at 40x60,
where the command, argument, output and read rows carrying U+061C wrap, and
`ctrl-o-command-marks`' and `call-row-arabic-letter-mark`'s selectors, which
name those rows whole as they stand at 120 columns, do not reach the pieces
(`G\` / `u{061c}H...`). Both decisions cover those rows; the selectors are
width-bound, as `ctrl-o-command-marks` was in T-U1-SAFE.

## Round 2: the resolution review's fixes

Both engines approved with fixes. Code head `5f2d0b88`.

### MUST: the bridge-to-MCP-row seam

- **The defect.** See merge 6: `main`'s MCP row cut a reason the channel had
  already spelled one character at a time, so its 256-byte bound could fall
  inside one of the channel's escapes.
- **The fix** (`5f2d0b88`):
  - `crate::mcp::terminal_safe_respelled` is `terminal_safe`'s tokens and
    bound, with each escape the channel writes (`\xNN`, `\u{N...}`, as
    `tools::question::terminal_safe` writes them) taken as one token that is
    never cut.
  - `server_spelled` uses it, so the row is what one pass over the server's
    words gives: `main`'s wider set and `main`'s bound. **One exception**,
    since round 3: server text shaped like an escape (`\x` and two hex
    digits, or `\u{`, one to six hex digits and `}`) that straddles the cut
    (byte 253) is kept whole, as the channel's own escapes are, so the row
    ends before it -- up to 9 bytes earlier than one pass, which cuts inside
    it. Both are printable. Pinned by
    `escape_shaped_server_text_at_the_bound_is_kept_whole_and_the_row_ends_before_it`:
    - 252 `A` + a literal `\x1b` + `Z`: one pass ends `A\...`, the row
      ends `A...`;
    - through the channel, 244 `A` + U+00AD + a literal `\x1b` + `Z`: the
      channel leaves U+00AD raw (251 bytes, under its bound), and the row
      names it in 8, which carries the literal across the cut. One pass ends
      `\u{00ad}\...`, the row `\u{00ad}...`.
  - The MCP arm tests the reason after spelling it.
  - CALLROW-SAFE's non-MCP rows are untouched: the change is the MCP arm,
    `server_spelled` and a new function (`git diff f4af7b0a 5f2d0b88 --
    src` is `src/mcp/mod.rs`, the new function; `src/tui/toolgroup.rs`, the
    MCP arm, `server_spelled` and a test; and `src/tui/shell.rs`, a test).
- **Why not have the channel leave an MCP reason to the row.** At
  `ToolResult` the channel has no presentation to tell an MCP call by, only
  the name, and a name is no test: `mcp_select_tool` is a registry meta tool
  (`src/tools/mcp.rs:120`). Spelling every reason as it crosses is also
  CALLROW-SAFE's reviewed guarantee to every consumer of the event.
- **RED**, at `f4af7b0a` with the test alone added (`seam-red.txt`):
  `tui::shell::tests::an_mcp_calls_reason_through_the_channel_is_the_rows_own_spelling_cut_once`
  drives `UiEvent::ToolStart`/`ToolResult{..}.made_inert()` for
  `mcp_web_echo` into the shell. The reason holds a literal `\`, a tab,
  U+00AD, U+202E, BEL, 155 `A`, six U+FE0F and three ESC. The document's row
  ends `\u{fe0f}`x6 `\x1b\...` where one pass over the words ends `\x1b...`
  (`shell.rs`'s first row assertion).
- **GREEN**, at `5f2d0b88` (`seam-green.txt`):
  - that test passes: the row is the one-pass row in the document and on
    Ctrl-O (its reason holds no escape-shaped text);
  - three reasons of white space alone (`"   "`, `" \t\r\n "`, VT+FF) give
    `└ Failed mcp_web_echo` with no `: ` on both views.
  - `tui::toolgroup::tests::an_mcp_reason_spelled_by_the_channel_is_cut_as_one_pass_cuts_the_raw_one`:
    the channel's spelling then the row's equals one pass over the raw words
    at every length from 150 to 299, for three tails. Between them the tails
    hold U+FE0F, ESC, U+00AD, U+202E, C1, DEL, a literal `\`, U+061C, U+FEFF,
    U+200B, U+3164, VT and white space. The result is a fixed point of the
    row's spelling.
  - The same test covers text that only looks like an escape (a server's
    literal `\x1b`), with no equality claimed: it reads the same as the
    channel's own escape once spelled, so the row keeps it whole, within the
    bound, and is a fixed point. The channel's own bound, CALLROW-SAFE's,
    may cut it as it cuts any printable text. Its exact rows at the cut are
    pinned by round 3's test (above).
  - Round 3: `an_mcp_reason_with_nothing_left_once_spelled_is_no_reason`
    gives the MCP row a raw VT (`\u{b}`) straight, not through the channel,
    and wants `└ Failed mcp_web_echo`. With the arm's order reverted to test
    before spelling (`toolgroup.rs:251-252`), it fails, `"└ Failed
    mcp_web_echo: "` against `"└ Failed mcp_web_echo"` (`order-mutant.txt`;
    run in the worktree and put back, as the mutant removes no guard). The
    shell test's blank cases pass in either order, since the channel has
    flattened them already.
  - `main`'s `a_servers_words_in_a_failed_mcp_calls_row_are_written_out_not_obeyed`
    passes unchanged.
- **fx for white space alone.** `failureStatusDetail`'s dynamic-tool branch
  returns null when `encodeTerminalSafeInline` leaves nothing
  (`src/core/agent/runtime/tool_presentation.zig:1490-1493@481104fd`), and
  the row is then the base alone (`:1283`). Before this train xfx's live
  row was `Failed mcp_x: `: `main`'s channel blanked controls but kept white
  space, so the arm saw a non-empty reason.
- **The sweep cannot show this.** `p5-mcp-*`'s failure reasons are JSON
  envelopes (cause `mcp-call-error-text`). It is still swept below, at this
  code.

### SHOULDs

1. **`call-row-arabic-letter-mark`'s selectors** name the whole row, `^`
   to `$`, as `ctrl-o-command-marks`' do. Rescored at `bca3e793`'s sweep: no
   pair's verdict, causes or unattributed rows change.
2. **T-U6's `check.py` on the train's captures** (`t6-check.txt`):
   - Which captures: `bca3e793`'s build, held `submit-0.15` snaps included,
     `frames.py`'s replays of their wires, and a copy of `check.py` whose
     `BUILD["xfx"]` is `bca3e79`, the build of these captures.
   - Result: **35 claims, 35 hold**, as T-U6's `green.txt`. Findings: 2 where
     T-U6 had 4. `diffodd` `full`'s raw U+202E/U+200B/U+FEFF are gone
     (T-U1-SAFE), and xfx's `diffwide` line 3 ends `TAIL` as fx's does
     (WIDTH-DISPUTED).
   - The T7 claim `same ... submit-0.15` holds on the held snap. Its label
     now names the snap taken at the held first chunk, not 0.15 s after
     Enter. T-U6's receipt and its captures, which are 0.15 s ones, are left
     as they were.
3. **40x60 `p5-transcript-safe` `full`/`full-pgup`: out of scope.**
   - The brief's sweep size is 40x120, and so is CI's.
   - At 60 columns the U+061C rows wrap, and both divergences' selectors
     name whole rows (anchored since SHOULD 1).
   - Naming the pieces (`G\`, `│ u{061c}H...`, `│ {2067}i`) would take
     selectors loose enough to cover other rows of the scenario, which is
     what SHOULD 1 removes.
   - Those rows show; they hide nothing.

### The MCP sweep

Builds: base `6066d67e` (`v0.1.0-6066d67`); the train at `f4af7b0a`
(`v0.1.0-f4af7b0`, before the fix) and at `5f2d0b88` (`v0.1.0-5f2d0b8`,
the fix). fx v0.0.16. `sweep.sh --jobs 2 --only 'p5-mcp-*' --only
p5-input-mcp` at 40x120, and `--size 40x60 --only p5-mcp-menu-new`, one
sweep per build (`sweep-mcp-base-*.md`, `sweep-mcp-fix-*.md`). Counts are
identical-or-normalized / approved / total, as the epic-2 integration
receipt counts them.

| scenario | epic-2 integration | base `6066d67e` | train `f4af7b0a` | train `5f2d0b88` |
|---|---|---|---|---|
| `p5-mcp-*` (20), 40x120 | 85 / 1 / 146 | 87 / 0 / 146 | 87 / 0 / 146 | **87 / 0 / 146** |
| `p5-mcp-menu-new` 40x60 | 22 / 1 / 34 | 22 / 1 / 34 | 22 / 1 / 34 | **22 / 1 / 34** |
| `p5-input-mcp` | 9 / 0 / 9 | 9 / 0 / 9 | 9 / 0 / 9 | **9 / 0 / 9** |

- Per scenario, every build has epic-2's count. There are two exceptions,
  and each is one pair more passing on the base as well: `p5-mcp-live` 3 /
  0 / 6, and `p5-mcp-menu-new` 23 / 0 / 34 at 40x120 (its approved pair
  passes). Those come from `main` since epic-2, not from the train.
- Pair by pair, both train builds give the base's verdict on all 155 pairs
  at 40x120 and all 34 at 40x60, with the same rows unattributed: no
  regression, and no pair passes on one build and not on another.
- `p5-mcp-menu-old` (10) and `p5-mcp-fail` (4), which read 8 and 3 on a
  first run of the epic-2 preview, read 10 and 4 on all three builds here,
  so nothing needed a re-run.

### Round 2 gates (`5f2d0b88`, one at a time)

| gate | result |
|---|---|
| fmt, clippy, clippy with fault-injection | ok |
| identity / stubs / secrets, the ledger loading with its sources checked | ok |
| `scripts/fxcap/test-sweep.sh` | 50 + 26 OK |
| `cargo test --locked --no-fail-fast` | 5155 passed, 0 failed, 7 ignored |
| `cargo test --locked --features fault-injection --no-fail-fast` | 5192 passed, 0 failed, 7 ignored |

### Round 3 (tests and wording)

The equality claim is qualified (above) and pinned by
`escape_shaped_server_text_at_the_bound_is_kept_whole_and_the_row_ends_before_it`.
The test-after-spelling order is pinned by
`an_mcp_reason_with_nothing_left_once_spelled_is_no_reason`, whose mutant
fails. `terminal_safe_respelled`'s doc comment names the exception. The
shell test's comment now says the channel's text is 224 bytes. Product code
is unchanged: the diff is two tests, two comments and this receipt.

| gate | result |
|---|---|
| fmt, clippy, clippy with fault-injection | ok |
| identity / stubs / secrets | ok, with this receipt staged |
| `cargo test --locked --lib --exact`, one at a time: `an_mcp_reason_spelled_by_the_channel_is_cut_as_one_pass_cuts_the_raw_one`, `a_servers_words_in_a_failed_mcp_calls_row_are_written_out_not_obeyed`, the two new tests, `an_mcp_calls_reason_through_the_channel_is_the_rows_own_spelling_cut_once` | 1 passed each |

## Files

- `ci-report.md`: `ci-sweep.sh`'s report as it ran (`bca3e793`'s ledger).
- `sweep-head.md`: its head sweep, rescored with the committed ledger.
- `sweep-extra.md`, `sweep-extra-base.md`: the 18 scenarios outside that
  selection, head and base, rescored with the committed ledger.
- `sweep-40x60.md`: CALLROW-SAFE's two 40x60 scenarios.
- `units.txt`: each unit's GREEN xfx captures against the train's.
- `seam-red.txt`, `seam-green.txt`: round 2's seam test at `f4af7b0a` with
  the test alone added, and at `5f2d0b88`.
- `t6-check.txt`: T-U6's `check.py` on the train's captures.
- `order-mutant.txt`: round 3's order test against the arm with its order reverted.
- `sweep-mcp-base-120.md`, `sweep-mcp-base-60.md`, `sweep-mcp-fix-120.md`,
  `sweep-mcp-fix-60.md`: the MCP sweeps on the base and at `5f2d0b88`.

Every file went through `scripts/fxcap/redact.py` (the sweep's own writer,
and the comparison's both sides).
