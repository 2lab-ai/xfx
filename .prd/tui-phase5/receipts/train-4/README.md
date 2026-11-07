---
title: 0.0.16 delta -- P5 train 4 receipt (MCP-E, TC-NOTATION, FLAKE-HINT4 into one branch)
type: receipt
tags: [xfx, phase5, fx-0.0.16, train]
---

# P5 train 4: three closed units, one branch

Branch `train/p5-1108a`, from `origin/main` `e6cde019` (PR #79, train 3), as
the brief expected; `origin/main` had not moved when it was fetched. Each
unit was closed by its own two-engine review and gate; this train combines
them. The program goal (the user's words): «최신 fx랑 비교해서 ui쪽으로 모든
기능 구현하고 기본적으로 최신 fx ui를 그대로 구현해줘» (fx v0.0.16, pin
`481104fd`).

Oracle: fx v0.0.16 (`481104fd`), executable sha256
`8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked
by `capture.sh` before and after every run, `FX_AUTO_UPGRADE=0`.

**At a glance.**
- Merges: 1 and 3 are git's own (empty remerge-diffs). Merge 2 resolved
  two things by hand: 11 ledger hunks that were `path:line` numbers alone
  (the code each names, re-pointed and checked), and a train 3 test that
  ran on `theme::Depth`, which TC-NOTATION deleted (one run on the one
  palette).
- Join checks: FLAKE-HINT4's lint refused three MCP-E scenarios (a `/`
  typed inside the full-access warning's window); converted as
  FLAKE-HINT4 converted `p5-mcp-turn`, 0 races over 332 scenarios, the
  lint unchanged. MCP surfaces in builtin greys: a new join test (5 of 5
  mutants caught) and a `-tc` twin whose captures are byte-identical to its
  256-colour twin's. No product line changed.
- Gate at `b7d47a93`: 20 steps, 0 failed (`cargo test` 5279, with fault
  injection 5316).
- Sweep, 108 scenarios at the head: 309 of 526 pass, 0 failed, 0 timeout.
  Against `main` 246 → 287 over the 502 shared pairs, one down: a timing
  pass on `main`'s side (`h1-models2` `after`, the old fixed-clock
  `models2`). Against each unit's own sweep: 0 dropped.

## Units and merges

Each is `git merge --no-ff`, in this order; each message names the unit and
its head.

| # | unit | branch @ tip | merge | base | conflicts |
|---|---|---|---|---|---|
| 1 | MCP-E + b/c/d | `feat/p5-mcp-discover` @ `d56ab017` | `a76f4e37` | `6c93f359` | none |
| 2 | TC-NOTATION + b | `feat/p5-tc-notation` @ `e1d4016b` | `066a913d` | `ec390074` | ledger (11 hunks, line numbers alone); one semantic (a train 3 test on a deleted type) |
| 3 | FLAKE-HINT4 r2 | `fix/p5-flake-hint4` @ `31a6ef79` | `e3035c92` | `e6cde019` | none textual; the join check below |

Bases are git's merge bases: MCP-E already held `main` `6c93f359` (its
`7ff9864b`); TC-NOTATION forked from TG-REDRAW's tip `ec390074`, which train 3
merged into `main`; FLAKE-HINT4 already held `main` `e6cde019` (its
`4d264ffe`).

After the merges, in order:

| commit | what |
|---|---|
| `d2e25b0a` | the join check's fix: three MCP-E scenarios converted to FLAKE-HINT4's `until` (below) |
| `a5b45e77` | the `file:line` references merge 3 moved, re-pointed (below) |
| `4714be5e` | the MCP-E x TC-NOTATION join test (below) |
| `3bd15c89` | `p5-mcp-discover-stdio-modern-tc`, its capture twin under `COLORTERM=truecolor` |
| `b7d47a93` | this receipt, first part (the gate ran here) |
| the next | this receipt, the rest |

`git show --remerge-diff` of each merge is `remerge-1.txt` .. `remerge-3.txt`.
Merges 1 and 3 have an empty body: each is git's automatic result,
unchanged. Merge 2's names two files, `parity-ledger.toml` and
`src/tui/event_loop.rs`, and nothing else.

### Merge 1, MCP-E

Files both sides changed since `6c93f359` (`overlap.txt`): `docs/parity.md`
(MCP-E's `MCP client runtime` row; `main`'s `full transcript (C-o)` and
`colored and hyperlinked TTY output` rows, 16 unchanged lines from the
nearest) and the ledger (MCP-E's three normalization sources re-pointed
into `capture.sh`; `main`'s train 3 entries, 229 lines away). Auto-merged.
Every other MCP-E file (`src/mcp/*`, its 18 `p5-mcp-discover-*` scenarios,
`mcphttp.ts`, `capture.sh`, `sweep.py`, `test_capture.py`, `test_sweep.py`,
`tests/support/mcp_discover_fixture.sh`) was untouched by `main` since
`6c93f359`.

### Merge 2, TC-NOTATION: two resolutions

TC-NOTATION's base is `ec390074` (TG-REDRAW's tip); `main` holds that and,
since it, DIFF-WRAP, ENGINE-VS-ANCHOR, train 3's join tests and re-points,
and #78. Nine files both sides changed (`overlap.txt`): `docs/parity.md`,
the ledger, `scripts/smoke-tui.sh`, `check.rs`, `event_loop.rs`, `frame.rs`,
`full_transcript.rs`, `grid.rs`, `shell.rs`. All but the ledger merged
textually (nearest hunks: `docs/parity.md` 2 lines, `smoke-tui.sh` 8,
`event_loop.rs` 77, the rest 160 to 1028).

**1. The ledger: 11 conflict hunks, each a `path:line` number alone.** Both
sides re-pointed the same references, from different frames:

- `main` (train 3's `b4e0d06e`) pointed each at the code its entry is
  about, in `main`'s file (train 3 `repointed.txt`, 7 of 7).
- TC-NOTATION moved the base's numbers by the lines it deleted, keeping
  whatever line each named at its base (its README: "each names the line it
  named at the base"; several were already stale there, as train 3 found).

Resolution, in both sides' meaning: `main`'s pointer (the code), moved by
TC-NOTATION's deletions above it. `resolve_refs.py` does it mechanically:
it refuses a hunk whose two sides differ in anything but `path:line`
numbers, takes `main`'s side, maps each number from `main`'s copy of the
cited file to the merge's own result (`git diff -U0 HEAD`), and requires the
cited line and two either side to be the same text at the new line
(`resolve-refs-2.txt`: 19 references, 0 problems).

| entry (field) | `main` | TC-NOTATION | merged | why |
|---|---|---|---|---|
| `call-row-raw-argument` (source), `call-row-page-edge` (xfx) | `toolgroup.rs:750` | `:656` | `:749` | TC-NOTATION deleted 1 line above (`toolgroup.rs` 551) |
| `call-row-source-cut` (source) | `toolgroup.rs:798` | `:704` | `:797` | the same |
| `call-row-arabic-letter-mark` (source) | `toolgroup.rs:912` | `:818` | `:911` | the same |
| `cancelled-marker` (xfx) | `toolgroup.rs:711` | `:617` | `:710` | the same |
| `submit-multi-card-whole` (why; `send`) | `shell.rs:12275` | `:12189` | `:12272` | 3 deleted above (10838-10840) |
| `session-record`, `transcript-page-offset`, `toolgroup-ctrlo-record-short`, `safe-page-edge`, `full-transcript-shift` (xfx) | `shell.rs:37458` | `:37238` | `:37424` | 34 deleted above |

The other numbers in those hunks (`docs/parity.md:238`,
`approval_view.rs:685`) were the same on both sides and are unchanged. Each
TC-NOTATION number differs from `main`'s by the same lines `main`'s base
frame differed, so the two moves agree: `-1`, `-3`, `-34` on each side.

**2. `src/tui/event_loop.rs`: a test on a type the other side deleted.**
Train 3's join test
`a_tool_rows_selector_glyph_is_drawn_again_in_the_frame_writers_bracket_and_accepted`
(on `main`) ran at both depths, `for depth in [Depth::Ansi256,
Depth::TrueColor]` through `shell_at(24, 80, depth)`, and checked three
greys only `if depth == Depth::Ansi256`. TC-NOTATION deleted `theme::Depth`
and `shell_at` (the palette is a mode and nothing else; its README,
"What changed in xfx"). Textually the merge was clean (77 lines apart) and
did not compile. Resolution: the test runs once on the one palette
(`shell_of(24, 80)`), every check kept, its `{depth:?}` labels dropped, the
three grey checks no longer gated (they are the palette's `38;5` greys,
which TC-NOTATION spells so at every depth), and its comment says why. This
is TC-NOTATION's own treatment of its two depth tests
(`a_truecolor_sessions_*`, rewritten to one palette). It passed at the first
run, and with it the other train 3 join test (`tui::frame`):
`cargo test --lib -- a_tool_rows_selector_glyph a_tool_rows_split_selector_cell`,
2 passed. `grep` over the merged `src` and `tests` found no other use of
`Depth`, `shell_at`, `depth_from_env`, `truecolor()` or `painter_of`, and
no other `main`-side line since `ec390074` that writes or expects `38;2`.

These two are of different kinds and are recorded apart: the ledger was a
textual conflict; the test was a semantic one (the merge's tree did not
build), put in the merge commit so that every commit of the train builds.

### Merge 3, FLAKE-HINT4

Four files both changed since `e6cde019` (`overlap.txt`): `capture.sh`
(nearest hunks 10 lines apart: MCP-E's `mcphttp` modes against FLAKE-HINT4's
verb checks), `sweep.py` (388), `test_capture.py` (3: MCP-E's `mcphttp`
refusal cases beside FLAKE-HINT4's `models2` refusal cases), `test_sweep.py`
(47). Auto-merged; `remerge-3.txt` is empty. The merged tree fails one
FLAKE-HINT4 test, `Capfile.test_no_snap_of_the_repos_scenarios_races_a_timer`
(run alone at `e3035c92`: FAILED, three MCP-E scenarios; at `d2e25b0a`: OK;
`lint.txt`): the join check below.

## The token check (`token_check.py`, `token-check.txt`, `token-self-test.txt`)

Train 3's `token_check.py`, with one rule added. For each merge, against
git's merge base: `docs/parity.md` row by row (whitespace tokens as a
multiset: the merged row is exactly the base's plus what each side added
minus what each side deleted), and the ledger entry by entry (an entry one
side changed is that side's, one a side deleted is gone, one a side added
is in, the rest the base's; every comment line a side added is in, every one
a side deleted is not). Added for train 4: an entry both sides changed
differently passes when the two sides and the merge are the same entry once
every `path:line` number is masked. The numbers themselves are not this
script's to check: `resolve_refs.py` and `repointed.py` check each against
the code it names.

**Result: all three merges OK.**

- Merge 1: `MCP client runtime` from MCP-E (846 tokens added, 91 deleted);
  `full transcript (C-o)` and `colored and hyperlinked TTY output` from
  `main`; MCP-E's three normalizations (`run-directory`, `capture-tmpdir`,
  `tmux-dead-pane`); `main`'s train 3 entries as `main` has them.
- Merge 2: TC-NOTATION's `colored and hyperlinked TTY output` (27 added,
  137 deleted); `main`'s six rows; `truecolor-grey-notation` deleted by
  TC-NOTATION and gone; `ctrl-o-diff-row-clipped` and
  `frame-variation-selector-anchor` deleted by `main` and gone; the 11
  entries of the ledger conflict, numbers alone; comment lines: 4 deleted
  (TC-NOTATION's note above `ctrl-o-diff-row-clipped`), none back.
- Merge 3: FLAKE-HINT4 changed neither file; everything is ours.

No `docs/parity.md` row was changed by both sides of any merge.

**The negative control** (`--self-test`): on each merge's own texts, a token
a side added dropped, a token a side deleted put back, an added ledger entry
dropped, a deleted one restored, a comment line dropped, and (new) one of
the numbers-only entries with a word of it changed and with a reference of
it dropped: 15 cases over the three merges, 15 found.

`docs/parity.md` is 288 lines at every base, side and head, and every side's
change is a row rewritten in place, so the ledger's `docs/parity.md:<line>`
citations still name the same rows (`refs.py` reads each by its row,
`= row`).

## The ledger's and the harness's moved refs (`a5b45e77`)

Train 3's `refs.py` (the ledger and `docs/parity.md`), four sides: `main`
`e6cde019`, MCP-E `d56ab017`, TC-NOTATION `e1d4016b`, FLAKE-HINT4
`31a6ef79`. At merge 3's head `e3035c92` (`refs.txt`): 66 references, 3
`MOVED`, all MCP-E's re-points into `capture.sh`, which FLAKE-HINT4 grew
above them. Widened (`refs_all.py`: every unpinned `path:line` in every
tracked file outside `.prd/`; `refs_short.py`: the same for `name.rs:N`
with no directory, into a file some side changed), one more moved by this
train: FLAKE-HINT4's citation of `warning_shown` in `capfile.py`, moved by
TC-NOTATION's three deleted `shell.rs` lines. `a5b45e77` re-points all five;
`repointed.py` checks each of train 4's 11 re-points (the six above and
these five) against the code in the frame it was written in, the cited lines
and two either side: **11 of 11 the same code** (`repointed.txt`).

| who cites it | cited (frame) | now | moved by |
|---|---|---|---|
| `run-directory` (source) | `capture.sh:430` (MCP-E) | `:462` | FLAKE-HINT4 (+32 above) |
| `capture-tmpdir` (source) | `capture.sh:266-268` (MCP-E) | `:291-293` | FLAKE-HINT4 (+25) |
| `tmux-dead-pane` (source) | `capture.sh:794` (MCP-E) | `:878` | FLAKE-HINT4 (+84) |
| `capfile.py` `TIMERS`, `warning_shown` | `shell.rs:13708-13733` (FLAKE-HINT4) | `:13705-13730` | TC-NOTATION (-3) |
| `capfile.py` `TIMERS`, the full transcript | `shell.rs:13726-13728` (FLAKE-HINT4) | `:13723-13725` | TC-NOTATION (-3) |

At the head (`refs-head.txt`): 66 references, 0 moved. `refs-all.txt` holds
the widened scans at the head. What they still flag is not moved by this
train: on every one `main`'s own copy maps to the same line at the head (the
flags are against MCP-E's and TC-NOTATION's older frames of files train 3
changed), or it sits in a historical plan under `docs/superpowers/plans/`
that names other lines on every side (TC-NOTATION left the same plan's
`theme.rs:333` as it is, for that reason). One of them is stale on `main`
itself and was before this train: `smoke-tui.sh`'s `check.rs:1084` (since
`9acf15d1`, 2026-09-12). Not re-pointed here: neither merge moved it.

## Join checks

### FLAKE-HINT4's timer-window lint x every other unit's scenarios (`lint.py`, `lint.txt`)

`lint.py` runs the train's `capfile.races` (the check
`test_sweep.py`'s `Capfile.test_no_snap_of_the_repos_scenarios_races_a_timer`
runs, which `test-sweep.sh` and CI run) over each commit's scenarios.

| commit | scenarios | races |
|---|---|---|
| `main` `e6cde019` (before FLAKE-HINT4) | 313 | 79, in 20 scenarios: the 20 FLAKE-HINT4 converted |
| MCP-E `d56ab017` | 325 | 85, in 23: those 20, and 3 of MCP-E's |
| TC-NOTATION `e1d4016b` | 311 | 79, in the same 20 |
| FLAKE-HINT4 `31a6ef79` | 313 | 0 |
| merge 3 `e3035c92` | 331 | **6, in 3 of MCP-E's** |
| `d2e25b0a` | 331 | 0 |
| head | 332 | 0 |

At merge 3 the repository check fails: `p5-mcp-discover-http-modern`,
`-http-notimpl` and `-stdio-modern` run in full access (`fx: env
FX_PERMISSION_MODE=full-access`, `xfx: env XFX_PERMISSION_MODE=yolo`), and
typed `/mcp show fixture` 6.0 s after launch, inside the warning's window (3
to 7 s): a typed `/` can take the warning's row and stop its clock
(`capfile.py`'s `pauses`). Neither unit could see this alone: FLAKE-HINT4's
lint was not on MCP-E's branch, and MCP-E's scenarios were not on
FLAKE-HINT4's.

**Converted (`d2e25b0a`)**, each as FLAKE-HINT4 converted `p5-mcp-turn` (its
own MCP fixture scenario): `until shown Full access` before the scenario's
`wait 6`, `until gone Full access` after it, then `/mcp show fixture`. The
`wait 6` stays: it is the server's time to start, not a count against the
warning. Nothing else in the three files changed. The lint is unchanged.

| scenario | before | after |
|---|---|---|
| `p5-mcp-discover-http-modern` | `wait 6` / `type /mcp show fixture` | `until shown Full access` / `wait 6` / `until gone Full access` / `type ...` |
| `p5-mcp-discover-http-notimpl` | the same | the same |
| `p5-mcp-discover-stdio-modern` | the same | the same |

How long each `until` waited and the sweep of the three, base against train,
are under Sweep below.

**`models2`'s old form** (`models2-grep.txt`): `git grep` over every tracked
file outside `.prd/` for the timed form (`models2 <catalog.json> <ms>`,
`FAKEGW_MODELS2_MS`, `models2ms`) finds one line, FLAKE-HINT4's refusal case
(`test_capture.py:155`, which pins that the old form is refused before
launch). The repository's one `models2` scenario line is
`h1-models2.cap`'s, in the new form (`lint.txt`). MCP-E's branch had
`main`'s old form (`capture.sh`, `fakegw.ts`, `h1-models2.cap`'s
`models2 h1-models-two.json 9000`) and changed none of those lines, so merge
3 took FLAKE-HINT4's; no MCP-E or TC-NOTATION scenario has a `models2`
line.

Not converted, and why: MCP-E's other discover scenarios snap near the MCP
startup quiet's 10 s (`p5-mcp-discover-stdio-escaped`'s `list`, `wait 8`
then `/mcp list`); that timer is not one the lint holds (FLAKE-HINT4's
"Timers found", reason 5, left to MCP-HOLD), so they are not lint
violations, and the brief converts lint violations. Named here; the sweep
below shows how they ran.

### TC-NOTATION x MCP-E: MCP surfaces in builtin greys (`mcp-greys.txt`, `4714be5e`, `3bd15c89`)

MCP-E changed no file under `src/tui`; its era reaches the screen through
three surfaces that draw in the palette's builtin greys (`mcp-greys.txt`):

- `/mcp show`'s version row (`  version    2026-07-28`,
  `src/mcp/command.rs:927`), plain text in a neutral notice the band paints
  in the notice grey (`notice.rs:70`, `Tone::Neutral => palette.notice()`);
- the `/mcp` menu's server screen, its `protocol` fact row
  (`mcp_menu.rs:1213`, `fact_row`'s label in `palette.dim()`); 41 lines of
  the menu and its argument rows call a palette accessor;
- an MCP tool group's rows, drawn by the tool group (`toolgroup.rs`).

The palette is a mode alone (`mod.rs:461-467`); `COLORTERM` is read once
(`mod.rs:459`), for the Ultra mark (`mod.rs:486-489`). With each file's test
module cut off, the one direct colour in `src/tui` is `ULTRA_TRUECOLOR`
(`theme.rs:817`).

No test covered these surfaces' spelling with an MCP server's era in them,
so **`4714be5e` adds one**:
`tui::event_loop::tests::a_2026_servers_version_and_protocol_rows_are_spelt_as_upstream_spells_its_greys`.
A real MCP runtime, its server MCP-E's own discover fixture in `modern`
mode (`tests/support/mcp_discover_fixture.sh`), served in the 2026-07-28
era (checked: `protocol_version` `2026-07-28`); a shell on it; `/mcp show
probe` written by the frame: the version row in `38;5;250`, no direct colour
in the frame; a light report: the row drawn again in `38;5;241`; `/mcp`,
Enter: the server screen, `protocol` in the light dim `38;5;247`, no direct
colour. fx's literals: `theme.zig:85`/`:128` (notice text 250/241) and
`:86`/`:129` (dim 245/247) at `481104fd`, as TC-NOTATION's README cites
them. Its first run failed in the test itself: it read the server's view
right after the startup reload, while the connection was still
`Connecting` (no version yet). The test now waits, up to 60 s, for the
connection to settle (as MCP-E's own `settled` helper does), and passed; no
`src` line outside the test changed.

**Does it discriminate?** `mutants.py`, on a `git clone --no-local` of
the train at `3bd15c89`, each mutant one exact replacement, the join test
run against it (`mutants.txt`): **5 of 5 caught**.

| mutant | where | caught at |
|---|---|---|
| A: the notice grey spelt direct (`38;2;188;188;188`, the dark 250) | `theme.rs` `notice` | the version row's `38;5;250` |
| B: the server screen's fact label spelt direct | `mcp_menu.rs` `fact_row` | `protocol`'s `38;5;247` |
| C: the menu's row drops the era's version | `mcp_menu.rs` `row` | no `protocol` row is ever written |
| D: `/mcp show` says no version | `mcp/command.rs` `show` | `2026-07-28` is never written |
| E: a theme switch leaves a notice row as it was | `theme.rs` `document_retint_in` | the light `38;5;241` |

**Under `COLORTERM=truecolor`, through the binary** (`3bd15c89`):
`p5-mcp-discover-stdio-modern-tc`, the twin of `-stdio-modern` with
`env COLORTERM=truecolor` (the other `-tc` twins' form): its `show` snap is
the version notice, its `turn` snap the MCP tool group. In the head sweep (`tc-vs-256.txt`,
TC-NOTATION's `tc_vs_256.py`): both snaps are **byte-identical to the
256-colour twin's**, on fx and on xfx; every SGR in them is `38;5` (xfx 30
and 46 escapes, fx 28 and 44, 0 direct; the same four greys on both, 245,
250, 252, 255); and both pairs are normalized-identical against fx, as the
twin's are. Over all eleven `-tc` scenarios at the head, xfx's truecolor
snaps are its 256-colour snaps: 33 of 36 byte-identical, the other 3 (the
`p5-toolgroup-ctrlo-tc` `full-*` pages) identical once digits are masked
(Ctrl-O's entry clocks); no row differs in its SGR sequences (fx: 31 and 3,
and `p5-toolgroup-live-tc`'s two `running-*` snaps, the activity row's
mark, as TC-NOTATION's README has them).

### Anything else the merges brought together

The conflicts and the near hunks (`overlap.txt`) name the places: the ledger
(the token check and `repointed.py`), the train 3 test on `Depth` (merge 2,
above), and merge 3's `capture.sh`/`test_capture.py` neighbours (MCP-E's
`mcphttp` modes and refusals beside FLAKE-HINT4's verbs and `models2`
refusals), which `test-capture.sh` runs whole in the gate. The new
combination at capture level -- FLAKE-HINT4's `until` driving an MCP-E
`mcphttp` scenario -- is what the three converted scenarios exercise; they
ran in the sweep (below).

## Gates

`gates.sh` (`gates.txt`): every step the brief lists, at `b7d47a93` (the
receipt's first part committed; the worktree clean before the run), one
after another, `TMPDIR` a short scratch directory. Both cargo test suites,
`test-capture.sh` (real captures) and `smoke-tui.sh` ran through the
machine's suite lock, which admits two heavy jobs from all the workers at
once (the coordinator's rule, 2026-11-08). The commits after `b7d47a93`
change only this directory (`git diff b7d47a93 <head> -- src tests scripts
docs Cargo.toml Cargo.lock build.rs` is empty).

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| the same `--features fault-injection` | ok |
| `cargo test --locked --no-fail-fast` | **5279 passed**, 0 failed, 7 ignored (15 binaries) |
| `cargo test --locked --features fault-injection --no-fail-fast` | **5316 passed**, 0 failed, 7 ignored |
| `scripts/check-xfx-identity.sh` / `check-no-stubs.sh` | ok (12920 tracked files) / ok |
| `scripts/check-no-secrets.sh` | ok (12920 tracked files) |
| `scripts/check-preview-contract.sh` / `test-preview-contract.sh` | ok / ok (24 mutants caught) |
| `scripts/check-tui-preflight-cost.sh` | ok (200x300 full 0.98 ms, update 0.64 ms, replay 1.19 ms; budget 32 ms) |
| `scripts/check-usage-clock.sh` | ok |
| `scripts/test-check-no-secrets.sh` / `test-gate-env.sh` | ok (13 cases) / ok (5 cases) |
| `scripts/fxcap/test-sweep.sh` | 66 + 26 OK |
| `scripts/fxcap/test-capture.sh` | 59 OK |
| `cargo build --locked --release`; `--release --features fault-injection --target-dir target/faulty` | ok; ok |
| `scripts/smoke.sh target/release/xfx <dir>` | 47 checks, 0 failures |
| `scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx <dir>` | 37 scenarios and the oracle, 1127 checks, 0 failures |

**Every side's tests are in.** `#[test]` attributes under `src` and
`tests` (`git grep -c`): `main` 5140; MCP-E +54 since its base, at merge 1
5194; merge 2 5192 (TC-NOTATION -2: its deleted depth tests); merge 3 5192
(FLAKE-HINT4 changes no Rust); the join test +1, 5193 at the head. `cargo
test` counts 5279 against `main`'s 5226 (train 3's gate): +53, the same
difference. The harness's (`def test_` in each file): `test_sweep.py` 66
(`main` 50, FLAKE-HINT4 +12, MCP-E +4) and `test_capture.py` 59 (`main` 42,
MCP-E +11, FLAKE-HINT4 +6) -- the counts the gate's two runs report.

With the receipt in its final form staged: `cargo fmt --check`,
`check-xfx-identity.sh`, `check-no-stubs.sh` and `check-no-secrets.sh`
again (`gates-final.txt`).

## Sweep

**Builds** (debug, as train 3's): the head `3bd15c89` (`x𝒇x v0.1.0-3bd15c8
[debug]` on row 1 of every xfx snap; the receipt commits after it change no
compiled file); `main` `e6cde019` and MCP-E `d56ab017`, each built in a
`git clone --no-local` scratch clone. fx: the v0.0.16 binary, sha256
`8e6cda2e…`, `FX_AUTO_UPGRADE=0`, checked by `capture.sh` around every run.
`scripts/fxcap/sweep.sh --fx <fx> --pin-sha 8e6cda2e… --xfx <bin> --jobs 2
--only ...`, 40x120.

**Each binary with its own tree's harness**, as the brief asks: the head's
sweep ran from the train's worktree (its scenarios, `capture.sh` and
ledger), `main`'s from its clone (`main`'s scenarios and harness: the old
timed `models2`, no `until`). This differs from train 3, whose two sweeps
both ran from the train's worktree (its README, Sweep). Every reference
sweep was then rescored with the train's ledger (`sweep.sh --rescore`, no
capture taken) before comparing, as train 3 did, so a verdict that moves
moved because a capture did.

**Selection, 108 scenarios at the head** (89 of them on `main`; the other
19 are MCP-E's 18 discover scenarios and the new `-tc` twin):
- every `p5-mcp-*` and `p5-input-mcp` (40: MCP-E's sweep selection, the 18
  discover scenarios among them, and `p5-mcp-discover-stdio-modern-tc`);
- every `*-tc` (11: TC-NOTATION's ten and the new one);
- TC-NOTATION's own selection, `p5-inline-code-*`, `p5-toolgroup-*`,
  `p5-toolstop-*`, `p5-transcript-*`;
- FLAKE-HINT4's 29 converted scenarios and `h1-models2` (its round 2's
  `models2`);
- the scenarios of every ledger entry the merges touched: the re-pointed
  `call-row-*` (`p5-call-row-*`), `cancelled-marker`, `session-record`,
  `transcript-page-offset`, `toolgroup-ctrlo-record-short`,
  `safe-page-edge`, `submit-multi-card-whole` (in the globs above) and
  `full-transcript-shift` (`fulltranscript*`, `links`, `read-failures`,
  `resume-tools`, `timeout`). `session-record`, `cancelled-marker` and
  `stats-bytes` name `scenarios = ["*"]`; as train 3 says, a re-point
  changes a line number the sweep does not read, so it moves no verdict.

**Result at the head** (`sweep-head.md`): 108 scenarios ran, 0 failed, 0
timed out; 526 pairs, **309 pass** (58.7%: 12 identical, 297
normalized-identical), 4 approved divergences, 213 unresolved; wall 51m29s,
load 7.2 at its start and 26.0 at its peak (other workers' sweeps).
The 24 discover pairs (`main` has none of them): 22 pass, and the two that
do not are `stdio-depth/list` and `stdio-escaped/list`, the `/mcp list`
header's SOURCE column that MCP-E's README names (`~/.xfx/mcp.json` a
character longer than `~/.fx/mcp.json`, not this unit's).

### The sweep's own trouble: four timeouts on `main`, outside the lock

`main`'s first sweep (`sweep-main-first.md`) ran before the suite lock
existed, beside four other workers' sweeps (load 4.5 at its start, 23.8 at
its peak, wall 83 min): four scenarios timed out (`p5-transcript-resize`
and `-tools` on fx, `p5-mcp-empty` and `compact` on xfx; 56 pairs). They
were run again under the lock (`sweep-main-rerun.md`: 4 of 4 ran, 0 timed
out), and their runs replace the first's in `sweep-main.md` (the first
sweep's directory with those four scenarios' runs swapped in, rescored).
A timeout there says nothing about the train: it is `main`'s run, and the
rerun passed.

### Against `main`

`compare-main.txt` (train 3's `compare.py`, which now reads a pair with no
rows, a timed-out one, as none), pair by pair over the 502 pairs both took:
**246 → 287 pass**; 46 verdicts moved, 45 up or between two passes, **1
down, a timing pass on `main`'s side** (below).

| pair(s) | `main` → train | what moved it |
|---|---|---|
| 25 `-tc` pairs: `p5-inline-code-retint-tc`, `-light-tc`, `-table-tc`, `-table-light-tc` (12), `p5-toolgroup-retint-tc`, `-light-tc` (6), `p5-toolgroup-live-tc` (4), `p5-toolgroup-ctrlo-tc` `dark`, `light`, `dark-again` (3) | unresolved → normalized-identical | TC-NOTATION: exactly its README's 25 (its Sweep: "25 pass on the head and not on the base") |
| `p5-mcp-menu-new` ×11 (`list`, `list-*`, `left-back`, `reopened`, `remove-*`, `server-remote`); `p5-mcp-menu-old` ×6 (`list`, `list-down`, `tools`, `resources`, `servers`, `details-1`) | unresolved → normalized-identical | MCP-E: the refused `remote` (127.0.0.1:9) reads `connecting` as fx's does, not `retrying` (every one in its `flips-base-head-120.txt`, "base fails, head passes") |
| `p5-transcript-submit` `filled`, `second` | identical → normalized-identical | **timing**, a pass both ways: xfx's turn summary reads `1s` where `main`'s run read `0s` (the turn's wall time; the `wall` normalization reads it) |
| `p5-transcript-submit-multi` `submit-0.15` | normalized-identical → identical | **timing**, a pass both ways: the activity row's mark caught in its other blink phase (`• Generating (1s)`), as train 3 names it |
| `h1-models2` `after` | normalized-identical → unresolved | **timing on `main`'s side** (below) |

**`h1-models2` `after` (`models2-runs.txt`).** `main`'s harness serves the
second catalog from 9 s after the fake gateway starts
(`models2 h1-models-two.json 9000`); FLAKE-HINT4's serves it from the
scenario's `models2` line, after the first `/model` is closed. In all three
of `main`'s runs here (the first sweep and two under the lock) the first
`/models` ask came 15 to 33 s after the gateway started, past the 9 s, so
every ask was answered with the second catalog: `before` showed two models
on both sides as well (the scenario means one there) and `after` matched.
In the head's three runs (the sweep and two under the lock) the first asks
got the first catalog, by order, and `after` is the difference FLAKE-HINT4
recorded on every one of its 7 runs, base and head: fx asks again at the
second `/model` and shows two models, xfx asks at launch only and shows one
(`flake-hint4/r2/sweep-compare.txt`, "fx asks again at the second /model,
xfx at launch only"). Why the first ask was that late: `capture.sh` (both
trees, the same lines) runs two freshly written shims once before launch,
and each first exec took about 10 s on this machine today (`bash -x` of
`main`'s `capture.sh` under the lock: gateway at 0 s, the first shim at
1 s, the second at 11 s, tmux and the launch at 21 s). So `main` passed
`after` only because its fixed clock ran out before launch; the pair is
not a drop of the train's code, and the harness that took it away is
FLAKE-HINT4's fix, working.

Two pairs changed differing rows without a verdict move, as on train 3:
`read-failures` `live-ctrl-o-up1` (the sweep's scratch path, a fresh name
per run) and FLAKE-HINT4's `p5-panels-sweep-a` (its own base against its
own head shows the same moved rows: `compare.py` on its `base-1`/`base-2`
and `head-3` sweeps, `feedback` 8 rows left and 13 new, `logout` 10 and
13); and `p5-mcp-verbs` (below).

**Screens that moved** (`screen-moves-main.txt`; train 3's
`screen_moves.py`, with the port of a run's HTTP MCP server, the checkout's
path and a saved session's resume id masked as run-local): 339 snaps
differ, 190 only in clock-like or run-local text. Of the rest, 29 moved on
fx at the same rows as on xfx (the run, not xfx's code: the checkout's path
in a stdio server's command line, `main`'s clone and the train's worktree
being paths of different length, so the row wraps elsewhere), and the
others are, by scenario: `p5-mcp-verbs` (the same path, at more rows on
xfx's side, plus MCP-E's `connecting`), `p5-mcp-menu-new`/`-old` and
`p5-mcp-config` (MCP-E's `connecting`), `p5-panels-sweep-a` (FLAKE-HINT4,
above), the `-tc` pairs (TC-NOTATION), `h1-models2` (above), the resume id
a saved session prints (`approvals`, `permissions-midturn`,
`resume-tools`, `settings-collapse`, `p5-transcript-diffask`), and fx's own
transient `press ctrl+c again to exit` on `p5-toolstop-retint-light`
`light` in `main`'s run, which FLAKE-HINT4's `until gone` now waits away.

### Against each unit's own head sweep

Each unit's sweep, rescored with the train's ledger (`sweep-unit-*.md`),
against the head's, over the pairs both took (`compare-<unit>.txt`):

| unit (reference build) | shared pairs | pass, unit → train | verdicts moved | dropped |
|---|---|---|---|---|
| MCP-E (`d96603f9`, its 39) | 177 | 131 → 133 | 2 | **0** |
| TC-NOTATION (`b8a4f952`, its 42) | 169 | 88 → 89 | 5 | **0** |
| FLAKE-HINT4 r1 (`ba242353`, its 29), 3 sweeps | 236 each | 92 → 105, each | 13, each | **0** |
| FLAKE-HINT4 r2 (`bf794f0d`, its 13), 2 sweeps | 93 each | 29 → 29 | 0 | **0** |
| FLAKE-HINT4 r2 `h1-models2` (`bf794f0d`), 3 sweeps | 2 each | 1 → 1 | 0 | **0** |

Each unit's sweep is of its unit's code: MCP-E's after `d96603f9` are its
round 3 (`7c2ceeea`, the HTTP admission and lifecycle, which no capture
scenario reaches, its README) and the merge of `main`; TC-NOTATION's after
`b8a4f952` are comments and the receipt; FLAKE-HINT4's after `bf794f0d`
are the `models2` line (swept in its `m2` sweeps), the `absent` settle and
two merges of `main`.

Every move:
- **MCP-E**: `p5-mcp-menu-new` `approve-confirm`, approved-divergence →
  normalized-identical: the approval's command line, `bun` and the
  scenario's path, wraps in MCP-E's sweep (its worktree's path is longer)
  and not in the train's, so the wrapped piece the divergence
  `mcp-approval-wrap-led` approves is not there. The checkout's path, not
  the code. `p5-mcp-menu-old` `prompts`, unresolved → normalized-identical:
  the coin flip on the 10 s probe MCP-E's README records and leaves to
  MCP-HOLD (`menu-old-reruns.txt`); it passes on `main`'s run too.
- **TC-NOTATION**: `p5-transcript-diffwide` `full` → normalized-identical
  and `-diffwide2` `full` → approved-divergence: DIFF-WRAP's reflow and
  ENGINE-VS-ANCHOR's anchor, which TC-NOTATION's base `ec390074` did not
  hold and `main` does (train 3's README, the same two pairs);
  `p5-transcript-submit` `filled`, `second` and `-submit-multi`
  `submit-0.15`: the timing moves above, passes both ways.
- **FLAKE-HINT4 r1**: the 11 `p5-mcp-menu-new` pairs (MCP-E's
  `connecting`), `approve-confirm` (the path, as above) and `p5-mcp-live`
  `full`, unresolved → normalized-identical, which passes on `main`'s
  sweep too: `main`'s own code since FLAKE-HINT4 r1's base `24a6b912`
  (train 3), not a unit of this train.

No pair that passes on a unit's own sweep fails on the train's.

**Screens that moved without a verdict** against the units
(`screen-moves-units.txt`, beyond clock-like and run-local text):
- TC-NOTATION: 3 snaps -- the resume id on `p5-transcript-diffask` `ask`,
  and `diffwide`/`diffwide2` `full` (the reflow above);
- FLAKE-HINT4 r2: 10 -- the resume id a saved session prints (`approvals`
  ×8, `permissions-midturn`), and fx's own `p5-panels-sweep-a` `logout`
  page;
- FLAKE-HINT4 r1: the resume id (`approvals` ×8, `permissions-midturn`,
  `settings-collapse`), the activity mark's blink on
  `p5-lifecycle-compactok` `c02-compacting`, `p5-mcp-live` `full` and the
  11 `p5-mcp-menu-new` pairs (the moves above), `p5-panels-sweep-a` (its
  own base-to-head moves, above), and 29 `p5-mcp-verbs` snaps: the
  checkout's path in a stdio server's command line, wrapped at other rows
  (fx's at the same rows on most), and MCP-E's `connecting`;
- MCP-E: 12 snaps beyond the 39 where fx moved at the same rows as xfx (the
  checkout's path): `p5-mcp-menu-old` `prompts` (above) and 11
  `p5-mcp-verbs` snaps -- the same path at more rows on xfx's side, and on
  `auth-remote` and `login-remote` the band two to three rows higher under
  the same notices (MCP-E's run kept three more rows of an earlier `/mcp
  list` above them). Unresolved on both sides, for the path, before and
  after.

### The join's scenarios, base against train

The three scenarios converted for the lint, MCP-E's head (their base: MCP-E
`d96603f9`'s sweep, its scenarios) against the train's (`compare-mcp-e.txt`
holds them): `p5-mcp-discover-http-modern` `show`, `turn`,
`-http-notimpl` `show`, `turn`, `-stdio-modern` `show`, `turn`: 6 of 6
normalized-identical on both, verdicts unchanged. Their `until` lines, in
every run of them here (the head sweep and a first sweep of the three at
`d2e25b0a`, each binary): `until shown Full access` returned 0.01 to 0.07 s
after its line, 0.03 to 0.10 s after launch; `until gone Full access` 0.01
to 0.03 s after the `wait 6`, 6.06 to 6.13 s after launch -- the warning had
gone, and `/mcp show` was typed 0.06 to 0.13 s later than before.

## Files

- `remerge-1.txt`, `remerge-2.txt`, `remerge-3.txt`: `git show
  --remerge-diff` of each merge.
- `overlap.py`, `overlap.txt`: every file both sides of a merge changed,
  and the nearest pair of hunks.
- `resolve_refs.py`, `resolve-refs-2.txt`: merge 2's ledger conflicts,
  resolved and checked against the code.
- `token_check.py`, `token-check.txt`, `token-self-test.txt`: train 3's
  token check with the numbers-only rule, and its negative control.
- `refs.py`, `refs.txt` (merge 3's head), `refs-head.txt` (the head): train
  3's reference map, four sides; `refs_all.py`, `refs_short.py`,
  `refs-all.txt`: the same over every tracked file outside `.prd/`, both
  forms of reference; `repointed.py`, `repointed.txt`: train 4's 11
  re-points checked against their code.
- `lint.py`, `lint.txt`: FLAKE-HINT4's timer-window lint over each side's
  scenarios and the train's; `models2-grep.txt`: the old `models2` form.
- `mcp-greys.txt`: the MCP surfaces, their greys, where `COLORTERM` is read.
- `mutants.py`, `mutants.txt`: the MCP-E x TC-NOTATION join test's mutants.
- `gates.sh`, `gates.txt`: the gate as it ran; `gates-final.txt`: the file
  checks with the receipt in its final form.
- `sweep-head.md`: the head's scoreboard; `sweep-main.md`: `main`'s (its
  first run with the four timed-out scenarios' rerun swapped in, rescored
  with the train's ledger), `sweep-main-first.md` and
  `sweep-main-rerun.md`: the two runs as they scored; `compare.py`,
  `compare-main.txt`: `main` against the head, pair by pair.
- `sweep-unit-mcp-e.md`, `-tc.md`, `-flake-r1-1.md`, `-flake-r2-3.md`,
  `-flake-m2-1.md`: each unit's own head sweep, rescored with the train's
  ledger (one of each set of reruns); `compare-<unit>.txt`: every one of
  them against the head.
- `screen_moves.py`, `screen-moves-main.txt`, `screen-moves-units.txt`: the
  screens that moved, clock-like and run-local text set aside.
- `models2-runs.txt`: `h1-models2`'s six runs, each `/models` ask and the
  catalog it got, and the shim warm-up trace.
- `tc_vs_256.py` (TC-NOTATION's), `tc-vs-256.txt`: each `-tc` snap against
  its 256-colour twin at the head, both sides, and the SGRs of the new twin.
- `redacted.py`: every sweep-derived file here went through
  `scripts/fxcap/redact.py`'s `Redactor.write` (it refuses a text with a
  local path left in it), checkout and account names first.
