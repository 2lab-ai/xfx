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
| the next | this receipt |

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
FLAKE-HINT4 test, which is the join check below.

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
them. Passed at its first complete run.

MUTANTS_PLACEHOLDER

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

GATES_PLACEHOLDER

SWEEP_PLACEHOLDER

## Files

FILES_PLACEHOLDER
