---
title: 0.0.16 delta -- P5 U5 receipt (a paste with input behind its end marker is kept; the input is ignored)
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, input, paste, security]
---

# P5 U5 PASTE-AFTER-END: fx v0.0.16 keeps the paste and ignores what followed its end marker

Spec: the research branch's `.prd/tui-phase5/research/input.md` §2.4, rule
I7, unit U5 (leads, re-derived here from fx's source). Oracle: fx v0.0.16
(`481104fd`, executable sha256 `8e6cda2e…`, checked by `capture.sh` and
`sweep.py` before and after every run, `FX_AUTO_UPGRADE=0`). Base: `main`
`712dd8d1`. Branch `feat/p5-paste-after-end`. **Security-sensitive (S)**:
clipboard bytes behind an embedded end marker must never run or be
submitted.

**Builds.** The base's debug build (`v0.1.0-712dd8d`, sha256 `cbfa3ee8…`)
and the GREEN code commit's, **`1a76ea00`** (`v0.1.0-1a76ea0`, sha256
`7283c1c8…`), each built after its commit; every xfx capture here is one of
the two (row 1 of each `.txt` says which). No commit after `1a76ea00`
changes a file the binary is built from: they are a scenario, two tests
and this receipt.

| commit | what |
|---|---|
| `c078237f` | RED, tests only: the decoder, shell and pty cases on the new rule, and three scenarios (`p5-input-paste-amend`, `-field`, `-behind`) |
| `1a76ea00` | GREEN: `Delivery::Pasted { dropped }`, the warning, the refusal path deleted; parity.md, smoke-tui scenario 7, comments. The binary every head capture and the head sweep used |
| `649f08ab` | `paste-suffix.cap` gets back the Enter byte `1a76ea00`'s header rewrite dropped (a scenario, not the binary) |
| `78b224cf` | test only: an Enter one read behind the end marker (the surviving mutant 12). **The full gate ran here** |
| `30ba1e6e` | test only: an overflowed amendment with input behind it on the review plane ends on the warning (pins the README's and parity.md's review-plane claim) |
| the last commit | this receipt, nothing else |

## fx's rule (`fx-src.txt`, every line read at the pin)

- **The state.** `paste_framing.zig:53-68,140-165@481104fd`: once the exact
  `ESC[201~` matches, the boundary is `end_candidate`; settling the delivery
  is `finish{dropped_suffix_bytes}` whatever followed. c1db919 had a third
  state, `unsafe`, settled as `reject` (removed by 9e0eab41).
- **"After"** is every byte from the one after the marker's `~` until the
  terminal has nothing more to read: each is `unsafe_suffix_bytes +|= 1` and
  nothing else (`:102-107`), the paste owns every byte while it is active
  (`main.zig:3127-3139`), and the delivery settles only when `pollInput(0)`
  finds nothing (`event_loop.zig:94-119`; still readable after 32 reads, it
  goes on unsettled, `:109-112`). One read or many; not "the rest of one
  read()".
- **The notice**: `input_paste_runtime.zig:168-198` finishes the paste first
  (`:189`; an overflow or unsupported bytes is refused and says so, `:210-266`)
  and then, if anything was dropped, writes `{topic "input", tone .warning,
  "Input that arrived right after the paste was ignored."}` (`:190-196`);
  `.warning` is `!` (`types.zig:50-58`). An overflow with a suffix is two
  notices, the limit's then the warning (`input_limit_feedback.zig:5-43`).
- **The surfaces**: the owner is the question's freeform field, a question
  (`decision_prompt`), an amendment, an approval (`decision_prompt`), sign-in
  code entry or the composer (`input_paste_runtime.zig:105-115`), and the
  settlement is the same for every owner. xfx's TUI has no code entry, so its
  surfaces are the other four.
- **Edge cases**, each also on the oracle's screen (`fx/`): a second paste in
  the same delivery is counted, never begun (`fx-p5-input-paste-behind-…-paste-behind`:
  `┃ keep new`, the arrow and `two` ignored); an empty paste with
  `/version CR ESC[201~` behind it runs nothing (`…-version-behind`: `┃ keep`,
  one warning); a marker split across reads is the marker
  (`paste_framing.zig:299-312`, settle `none` while capturing, `:161-162`).

## What changed in xfx (`1a76ea00`)

xfx already had fx's delivery epoch (the decoder's `behind` count and the
loop's `read_burst` drain, from X1); only the meaning of a counted suffix
changed.

- `src/tui/input.rs:387-397` `Delivery` is fx's `Settlement`:
  `Quiet | Pasted { dropped }`. `end_delivery` (`:453-458`) settles any paste
  whose marker arrived as `Pasted`, so a paste that ended is always its
  `PasteEnd`. **`feed`'s early return for a byte behind the marker
  (`:468-471`) is unchanged**: it is the security invariant (mutant 1).
- `src/tui/shell.rs:10367-10377` `end_delivery`: `PasteEnd` first, then
  `ignored_behind_paste` (`:12140-12153`) when `dropped > 0` --
  `Notice::warning("input", PASTE_SUFFIX)` in the document, or the review
  plane's status row while it is up. `PASTE_SUFFIX` (`:547`) is fx's text.
- Deleted, being the refusal and nothing else: `Shell::paste_refused`,
  `refuse_paste` on the approval panel, the amendment drafts, the review
  plane and the question panel, and `Paste::{overflowed, discard}`. The GREEN
  commit's `src` diff is 109 lines in, 159 out (`git diff --stat c078237f
  1a76ea00 -- src`, which counts `input.rs`'s test helper and dropped-count
  assertions too).

**One decision.** On the review plane there is one status row for what fx
writes as two document rows (an overflowed amendment: the limit, then the
warning); xfx shows the later one, the order fx writes them in. Pinned by
`shell.rs`
`an_oversized_paste_with_input_behind_it_on_the_review_plane_ends_on_the_warning`
(`30ba1e6e`: the warning on the plane, the limit's sentence not, `keep`
alone in the draft, nothing answered, nothing in the document); the plain
case by `a_paste_with_input_behind_it_on_the_review_plane_is_kept_and_said_on_the_plane`
(the warning, `keepnew`).

## RED (`c078237f`, tests only, on the base source)

- `red-unit.txt` (run on the working tree before `c078237f` was committed,
  the base source with the RED tests): 15 of its 18 lib tests fail: every
  decoder case (the Enter, a printable key, an arrow, a second paste, a split
  marker with an Enter behind it, an Enter one read behind) leaves no
  `PasteEnd`; every shell case (composer `priornew`, the empty paste with
  `/version` behind it, two reads of one delivery, the overflow's two
  notices, the question's field `keepnew` and choice, the amendment
  `keepnew`, the approval with no draft, the review plane) finds the paste
  refused and the error. Of the three `ok` lines, two are guards that pass on
  both sides (a paste with nothing behind it, the paste flag); the third,
  `…reaches_no_draft_and_answers_nothing`, is the **old** X1 amendment test,
  which the first edit had left in place and which was deleted before the
  commit -- it is not in `c078237f`.
- `red-pty.txt`: `tests/tui.rs`
  `a_paste_whose_end_marker_shares_a_read_with_enter_is_kept_and_submits_nothing`
  times out waiting for `! input: …` and `┃ priornew`.
- `red.txt`: the capture pairs, base against fx: every snap after a paste
  differs (`✗ input: Paste was not applied…` and `┃ prior`/`keep` against
  fx's `! input: Input that arrived right after the paste was ignored.` and
  `priornew`/`keepnew`); `wire.txt`: the base sent the answer `keep` and the
  amendment `keep` where fx sent `keepnew`.

## GREEN (`1a76ea00`)

`green.txt` (`cells.py`, row 1 not compared), head against fx:

| scenario | snaps | rows differing |
|---|---|---|
| `p5-input-paste` | paste-enter, its full | 0, 0 |
| `paste-suffix` | paste-enter, its full | 0, 0 |
| `p5-input-paste-behind` (new) | version-behind, paste-behind, its full | 0, 0, 0 |
| `p5-input-paste-field` (new) | q, choice-paste-enter, field-keep, field-paste-enter, answered | 0 each |
| `p5-input-paste-amend` (new) | asked, choice-paste-enter, amend-keep, amend-paste-enter, answered, end | 6, 6, 6, 6, 6, 5 |

The amendment pair's rows are **not this unit's**: `amend-rows.txt` lists
every one, and each is the band's grant scope (the ledger's
`approval-grant-scope` cause: option 2's wording, xfx's `2 = allow …` and
`--resume-id` rows) or a band row those two extra rows push down. All of
them are already there in `asked`, before any paste; the paste's rows
(`! input: …` twice, `❯ 1  Apply once, keepnew`, `┃ keepnew`) are
identical. `wire.txt`: head sends `keepnew` in both places, as fx.

Unit tests: `green-unit.txt` (the RED tests, every `tui::input`,
`tui::paste`, `tui::question` and `tui::approval*` test: 295 passed) and the
whole lib (4530 passed, in the gate).

## Mutants (`mutants.py`, `mutants.txt`)

Each on a throwaway `git clone --no-local` of the branch (never the
worktree; `GIT_DIR`, `GIT_INDEX_FILE`, `GIT_WORK_TREE` unset), one textual
change, then `cargo test --locked --lib tui::` and the pty paste tests, each
run in its own process group with a 900 s limit, the change undone after.

Round 1, on `649f08ab` (the code of `1a76ea00`): 11 of 12 killed.

| # | the rule reverted | killed by |
|---|---|---|
| 1 | **the bytes behind the marker are decoded: a trailing CR submits** (`feed`'s early return removed) | 17 tests |
| 2 | an ESC behind the marker is decoded, the rest counted | 3 |
| 3 | the count behind the marker is 1 whatever followed | 2 |
| 4 | a paste with input behind it never settles | 17 |
| 5 | c1db919's rule: such a paste is not applied | 8 |
| 6 | no warning | 10 |
| 7 | the warning with nothing behind the paste too | 20 |
| 8 | the warning before the paste is settled | 2 (round 2: 3) |
| 9 | the warning is an error (c1db919's tone) | 9 |
| 10 | c1db919's sentence | 10 |
| 11 | on the review plane the warning is not said | 1 |
| 12 | a delivery is one read: the loop settles after every read | survived (round 2: 1) |

Mutant 12 survived because no test drove `event_loop::read_burst` across two
reads: every decoder and shell case calls `end_delivery` itself.
`78b224cf` adds `tests/tui.rs`
`an_enter_one_read_behind_a_paste_s_end_marker_is_behind_it_too` (one
129-byte write, end marker on byte 128, the Enter alone in the second read;
3 of 3 green on the head).

Round 2, on `30ba1e6e`: **mutant 12 killed** by that test (and by it alone),
and **mutant 8** again, now by 3 tests: `30ba1e6e`'s review-plane overflow
case kills it too, since a warning said first is then replaced by the
limit's sentence on the plane's one status row. **12 of 12 killed.**

## The sweep (`sweep-base.md`, `sweep-head.md`, `sweep-compare.txt`)

The 41 input scenarios -- every scenario that pastes (a `paste`/`pastex`
line or a `[200~` in a `type`), every `p5-input-*`, `question*` and
`approvals*` -- on the base build and on `1a76ea00`, `--jobs 2`, the same
ledger (sha256 `530ac71a…`) and the same scenario files (the worktree's, the
three new ones included), the fx binary's hash checked before and after each
run. `cmp_sweeps.py` (T-U4's, with the uncaused rows cut to a count) compares
them pair by pair:

- **base 189 / 253 pass, head 200 / 253, drops 0.**
- The 11 new passes: `p5-input-paste` (2), `paste-suffix` (2),
  `p5-input-paste-behind` (3), `p5-input-paste-field` (4, of its 5; `q`, before
  any paste, passed on the base too). Each went from `unresolved-defect` to
  `normalized-identical`.
- `p5-input-paste-amend`: its paste rows leave the uncaused list (2-6 per snap
  to 0) and the pairs stay `unresolved-defect` under `approval-grant-scope`
  alone, a `decision` cause that approves nothing until the user decides
  (the ledger's `why`). `end` keeps one uncaused row, `xfx 17 (blank)`: a
  blank row above the composer after Escape cancels the write approval,
  which the base pair has at the same row -- not this unit's.
- No other pair changed class, cause or uncaused count.

## Retired and re-pointed

- `docs/parity.md` `full-screen TUI`: the paste paragraph now says fx
  v0.0.16's rule (kept, warned, the overflow's two notices, the review
  plane's status row) with `@481104fd` lines, and drops "(fx v0.0.16 keeps
  such a paste …; that is a later fx than this row's.)"; the amendment's
  "refused whole too when input follows its end marker" became "kept with
  what followed its end marker in the same delivery ignored".
- **No ledger cause existed** for the paste pairs (`grep -i paste
  scripts/fxcap/parity-ledger.toml` finds nothing at the base), so none is
  retired; they were unresolved defects, and now they are identical.
- `input.rs`'s module doc and `Action::PasteEnd`'s, the shell's two doc
  comments, `smoke-tui.sh` scenario 7 (the draft now holds the paste, the
  later Enter sends it, the needle is the warning), the pty comments at
  `tests/tui.rs` "ignored, not pressed" and `paste-suffix.cap`'s header.
- `paste-suffix.cap` (Phase 4's X1 scenario) is now the same keystrokes as
  `p5-input-paste.cap`, and is kept so no pair leaves the sweep; deleting it
  is a follow-up for whoever owns the sweep's list. Its re-worded header lost
  its Enter to a text-mode rewrite in `1a76ea00` (the head capture showed `┃
  priornew` with no notice); `649f08ab` puts the byte back, and the pair in
  `green.txt` is from after that.
- `.prd/tui-phase4/receipts/x1/` is X1's receipt as it was and is not edited.

## Gates (`gates.txt`, `gates-r2.txt`)

Every CI step of `ci.yml`, in order, at **`78b224cf`**, under the suite lock
(one slot for the run): fmt, clippy (plain and fault-injection), `cargo test
--locked --all-targets` (14 binaries, 5447 passed, 0 failed) and with the
fault injector (15 binaries, 5485 passed, 0 failed), no-stubs, no-secrets,
gate-env, identity, the preview contract and its mutants, the preflight
cost, the usage clock, the release and faulty builds, `smoke.sh` (47
checks) and `smoke-tui.sh` (37 scenarios + the oracle, 1167 checks, 0
failures; scenario 7, rewritten here, `ok`). All exit 0.

After the gate: `30ba1e6e` changes one test module (`shell.rs`'s tests), so
fmt, both clippies and both `--lib` suites ran again there (`gates-r2.txt`);
the receipt commit changes only this directory, so no-secrets, identity and
no-stubs ran again on it (`gates-r2.txt`). Nothing after `78b224cf` changes a
file the binary is built from.
