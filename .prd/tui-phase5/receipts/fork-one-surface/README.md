# P5-FK-GAPb receipt: one surface in the slot; the approval receipt's row

Branch `fix/p5-fork-status-gap`, on FK-GAP (`f2dcc4c8`). Two findings:

1. **Two surfaces in the slot** (review EPIC1-INT, opus (b)). `/fork` asks
   the runtime for its points and opens its menu when the answer lands. In
   between, Up recalled `/status`, a bare `/permissions` or a bare
   `/effort`, which ran with the `/fork` still waiting (`recall` and
   `run_command` go through `edited`, not `amended`), and opened their
   surface without closing the fork menu. The answer then opened the fork
   menu beside it, and after the panel closed a typed-ahead Return forked.
2. **Two unpinned wirings of FK-GAP** (review FK-GAP, opus SHOULD 1): the
   question/ask/rule branch of `fit` (`self.placed`) and the readiness
   offset. Pinning the offset found it was wrong before FK-GAP as well.

## fx (`481104fd`)

- `/fork`'s menu opens **synchronously**: `openForkMenu`
  (`src/core/app/app_session_runtime.zig:2072-2096`) fills the menu in the
  command's own call. Every key after it reaches the menu first; Up and
  Down move in it.
- Anything that edits the composer closes it
  (`dismissActiveMenusForComposerEdit`,
  `src/core/app/app_input_runtime.zig:1984-1990`).
- `openInfoPanel` (`src/core/app/app_commands.zig:528-536`) and bare
  `/effort` (`commandHandleEffort`, `:570-584`, the call at `:579`) both call `closeInlineCommandMenusIfPresent`
  (`:1174-1182`), which closes the statusline, info panel, usage,
  workspace and effort menus: each of the two closes the other.
- The footer shows one occupant of the slot at a time: each `show_*` is
  false while a modal or an earlier menu is up
  (`src/ui/footer/surface_frame.zig:398-408`).

## The rule ported

Upstream's menu takes every key after `/fork`. Here the answer is
asynchronous, so the ask is dropped by whatever moves the composer on
from the `/fork`, as an edit closes upstream's menu. `Shell::close_fork_menu`
closes the menu and drops a waiting ask; it is called by:

- an edit (`amended`, as before);
- a recall (`recall`);
- any line sent (`submit`) -- `/fork`'s own line then asks again;
- opening the info panel (`open_info_panel`) or `/effort`'s menu
  (`effort_command`).

The panel and `/effort`'s menu also close each other, as
`closeInlineCommandMenusIfPresent` does. `fork_points` closes nothing more:
nothing else in the slot can be up while a `/fork` waits. The panel,
`/effort`'s and `/mcp`'s menus and a rule's confirmation open from a line
sent. A turn's questions open only inside a turn: `/fork` is refused while
one is in hand, and a later turn starts only from a line sent. The session
picker opens with `/resume`'s line, and its answer only fills an open
picker (`UiEvent::Sessions`). The comments the epic-1 merge made false
(`fork_points`, `open_info_panel`, the two in `slot()`) now say this.
`docs/parity.md`'s `/fork` row says it too.

**The readiness row.** A question has no turn's row (`tick_activity`:
"No row at all while a question is up"), and the band puts the rule that
heads the panel above it. So `band_top + activity + index` named the row
above the last choice, and FK-GAP's `+ gap` kept that error. `Panel::compose`
now takes the panel's first row, and the shell hands it
`Geometry::panel_first()` instead of counting what is above it.

## RED (FK-GAP, `f2dcc4c8`)

Four tests, added before the fix and run on `f2dcc4c8`'s code. All four
fail. `red-tests.txt` has the output. There is no timing in them: the
`/fork`'s answer is applied by hand after the recall.

| test | on `f2dcc4c8` |
|---|---|
| `a_fork_still_on_its_way_opens_nothing_over_a_panel_a_recalled_command_opened`: `/status` Esc, `/fork`, the work given back, Up to `/status`, Return, then `ForkPoints(Points)` | **2 surfaces** in the slot (info panel + fork menu) |
| `a_fork_still_on_its_way_opens_nothing_over_a_recalled_effort_menu`: the same with a recalled bare `/effort` | **2 surfaces** (effort menu + fork menu) |
| `a_fork_still_on_its_way_opens_nothing_once_a_recalled_prompt_has_gone`: Up to a prompt, then the answer | the fork menu opened ("the recall left the ask standing") |
| `opening_the_panel_or_the_effort_menu_closes_the_fork_menu` | the fork menu stayed up under the panel |

The readiness row, pinned by
`a_question_over_a_forks_plain_line_keeps_the_blank_row_and_names_its_controls_row`
(an approval right after a fork, at 40x120): `3  Don't apply` is painted on
row 14. The receipt named row 13 with FK-GAP's `band_top + activity + gap`,
and row 12 with the form before FK-GAP. Neither counted the rule above the
panel, so both were one row high on every question screen; the older form
was two rows high under FK-GAP's blank row. What the row feeds is
`Intent::capture`'s `last_control_row <= rows` guard, and a choice row
always has the scope, the rule and the hint under it, so no reachable
screen granted or refused differently. It was a wrong claim, not a wrong
verdict.

## GREEN

All four one-surface tests pass, and so do the two pins:

- `a_question_over_a_forks_plain_line_keeps_the_blank_row_and_names_its_controls_row`:
  gap 1, `band_rows()[0]` blank, the rule under it, the panel at
  `band_top + 2`, and the receipt's row is the row the last choice is
  painted on.
- `a_rule_confirmation_over_a_forks_plain_line_keeps_the_blank_row`: a
  `/permissions remember` confirmation right after the fork; gap 1, the band
  opens blank then rule, and Escape takes the row back.

**Each revert fails** (on a throwaway clone, `git clone --no-local` under
`<tmp>`; neither revert removes a guard):

- `self.placed` in the question/ask/rule branch of `fit` back to
  `geometry.floated(self.document_rows)`: both pins fail, `gap` 0
  (`mutant-placed.txt`). FK-GAP's own two panel tests stay green, which is
  why this needed its own pin.
- The readiness row back to `band_top + activity + gap`: 13 against 14. Back
  to `band_top + activity`: 12 against 14 (`mutant-readiness.txt`).
  `approval::the_last_control_row_is_the_terminal_row_that_row_is_really_on`
  stays green either way, because it checks `compose`'s arithmetic, not
  where the band paints.

Left alone, as asked: the `restore_primary` one-frame transient (FK-GAP
opus SHOULD 2). No test fails on it.

## Pairs (fx v0.0.16; xfx = this branch)

The `sweep.yml` job, run locally:
`ci-sweep.sh --base f2dcc4c8 --fx <fx v0.0.16> --pin v0.0.16 --xfx <head
debug> --base-xfx <f2dcc4c8 debug> --jobs 2`, `FX_AUTO_UPGRADE=0`, head
`15e1b764` (this receipt's draft on `1dc3c37c`'s code). 43 scenarios (10
core, 33 named here), **275 snap pairs: 216 pass at the head, 0 new, 0
regressions**; 1,112 s. Every pair not passing at the head fails the same
way at the base.

Of the 129 xfx screens both builds took, every one is the same at the base
and the head once the run-variant text is masked (ids, workspace temp
names, the build sha). The ten that still differ differ only in a clock,
the spinner's phase, or tmux's `Pane is dead` timestamp. The fix changes no
screen: it closes a surface only in a state the scenarios do not reach, and
the readiness row is not painted.

Counts are identical-or-normalized / approved / total, with
`status-panel/pairs.py`. "FK-GAP" is that receipt's column.

| scenario | FK-GAP | this branch |
|---|---|---|
| `p5-effort-fork-fork` | 15 / 1 / 16 | 15 / 1 / 16 |
| `p5-effort-fork-fork-edge` | 10 / 1 / 11 | 10 / 1 / 11 |
| `p5-effort-fork-fork-empty` | 4 / 0 / 4 | 4 / 0 / 4 |
| `p5-effort-fork-fork-continue` | 2 / 0 / 2 | 2 / 0 / 2 |
| `p5-effort-fork-effort-menu` | 18 / 0 / 18 | 18 / 0 / 18 |
| `p5-effort-fork-effort-column` | 17 / 0 / 17 | 17 / 0 / 17 |
| `p5-effort-fork-effort-{backtab,ctrlc,down,loading}` | 2 / 0 / 2 each | 2 / 0 / 2 each |
| `p5-effort-fork-effort-nolevels` | 5 / 0 / 5 | 5 / 0 / 5 |
| `p5-effort-fork-effort-short` 40x120 | 3 / 0 / 3 | 3 / 0 / 3 |
| `p5-effort-fork-slash-rank` | 3 / 0 / 4 | 3 / 0 / 4 |
| `p5-effort-fork-oldread-src` | 0 / 0 / 3 | 0 / 0 / 3 |
| `p5-effort-fork-oldread-v1` | 2 / 0 / 4 | 2 / 0 / 4 |
| `p5-effort-fork-oldread-v2` | 2 / 0 / 7 | 2 / 0 / 7 |
| `p5-panels-status` | 5 / 6 / 11 | 5 / 6 / 11 |
| `p5-panels-keys` | 2 / 5 / 10 | 2 / 5 / 10 |
| `p5-panels-permissions` | 16 / 1 / 20 | 16 / 1 / 20 |
| `p5-panels-permissions-saved` | 5 / 0 / 8 | 5 / 0 / 8 |
| `p5-permissions-early` | 11 / 0 / 11 | 11 / 0 / 11 |

The approval scenarios, taken for the readiness change (base = head in
every pair): `allowlist` 10/0/10, `allowlist-remember` 7/0/7,
`allowlist-turn` 3/0/3, `allowlist2` 2/0/4, `allowlist3` 1/0/1, `approvals`
0/0/10, `approvals-amend-limit` 0/0/5, `approvals-big` 1/0/3, `permissions`
9/0/9, `permissions-btab` 7/0/7, `permissions-midturn` 3/0/4,
`interrupt-question` 1/0/3, `interrupt-next-question` 0/0/1.

Every file here went through `scripts/fxcap/redact.py`:
`/private/tmp/claude-501` is `<tmp>`.
