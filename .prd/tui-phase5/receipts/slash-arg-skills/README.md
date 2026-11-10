# SLASH-ARG-SKILLS receipt: fx's argument tables past the blank (part 1 of 2)

SLASH-LEADING-WS copied fx's slash kind, query, Tab, history, rows and
dismissal, and recorded what it left as F9 (its receipt's README, §F9 and
"Not copied"): past the blank after a slash word fx goes on completing --
the argument of six commands, and the skills an unknown name's words find.
This unit copies the first: fx's argument lists past `/statusline `,
`/sound `, `/permissions `, `/workspace ` and `/allowlist `, beside
`/mcp `'s, which xfx had (MCP-VERBS), and fx's one Escape over all of them.

**The split.** The brief's line: stop past about 300 lines of non-test
source. The argument lists alone came to 309 added (112 of them comments) and
29 removed, outside `#[cfg(test)]` (`count.txt`), so the second part -- the
skills an unanchored line's words find past its blank
(`mixedSlashCompletionCount`, `picker_presentation.zig:1170-1175`) -- is not
built here. Its fx evidence is in `probe/` (below) for the unit that takes it.

The base is SLASH-LEADING-WS' head `4bb8af77` (it carries train 13's head
`afd891af`). fx is the v0.0.16 release (pin `481104fd`), sha256 `8e6cda2e…`,
run with `FX_AUTO_UPGRADE=0`; `capture.sh` checked its bytes after every run.

| commit | what |
|---|---|
| `08db59ec` | the paired scenario `p5-slash-arg-skills`, `fx_src.sh` and `fx-src.txt`, `pairs.sh`, `probe-mixed.cap`; no `src/` change |
| `da9222bc` | **the code**: `src/tui/arg_lists.rs` (new), `src/tui/mcp_args.rs`, `src/tui/shell.rs`, `src/tui/mod.rs`, their tests, `docs/parity.md` |
| `f0399486` | the 14 ledger refs `da9222bc` moved, re-pointed (`repoint.py`); numbers only. The captures' head binary was built here |
| `f7baf771` | a test only: `/mcp `'s list stays closed under a slash dismissal made over `/mc` (mutant A11 survived round 1) |
| `245e99ec` | **the code head**: the six ledger refs `f7baf771` moved, re-pointed; numbers only. The gate ran here |
| the commits after | this receipt (`.prd/`), and one wording fix in `docs/parity.md` (which `/allowlist` lines Tab follows by a space) |

The captures in `xfx/`, `probe/head-*` and the head sweep come from the
binary built at `f0399486` (row 1 `x𝒇x v0.1.0-f039948 [debug]`, sha256
`277e2dcea525…`; its src is `da9222bc`'s, and the code head's but for the test
`f7baf771` adds); those in `red/`, `probe/base-*` and the base sweep from the
base (`v0.1.0-4bb8af7`, sha256 `7342529ddb57…`); `fx/` and `probe/fx-*` from
fx. One suite-lock slot ran all six captures (`caps.sh`, `caps.txt`: exit 0
but the base's paired run, below). No `gwlog` step, so no request body is in
the receipt.

## What fx v0.0.16 does (`fx-src.txt`, made by `fx_src.sh`; every range checked)

- **A1, the prefix.** Past the first blank the popup's prefix goes on for a
  name with arguments and stops for one without (`slashCompletionPrefix`,
  `command_specs.zig:568-576`).
- **A2, the anchored commands.** `/mcp` (its verbs on: `mcpVerbsOn` is
  `true`, `:871-874`), `/statusline`, `/sound`, `/permissions`, `/workspace`,
  and `/allowlist`'s anchor (`argCompletionAnchor`, `:636-644`).
- **A3-A5, the lists.** `/mcp`'s is the app's run-time list; the others are
  tables (`nthSlashCompletion`, `:610-631`; the words, `:750-841`). A row's
  label is its line past the command (`nthSlashCompletionLabel`,
  `:646-671`); a table row has no description and no category (`:673-690`).
- **A6, the filter.** The command, then a space or a tab; the four simple
  tables' query trimmed at both ends, `/allowlist`'s at its start only
  (`:909-943`); a word matched by its start, case ignored, an empty query
  matching every row (`argCompletionMatches`, `:1031-1034`).
- **A7, `/allowlist`.** It walks `view`, `local`/`user` (no `view` under a
  scope), `add`/`remove` and past them `tool`, and `reset`, to the word being
  typed; a word it does not know past its blank lists nothing; its anchor is
  each arm's head (`:1042-1276`).
- **A8, Tab.** The marked line replaces the composer, with a space when more
  is to come (`app_input_runtime.zig:1331-1353`; `slashCompletionHasArgs`,
  `:692-748`: `/workspace add`, `/workspace remove`, and the `/allowlist`
  actions, scopes and kinds).
- **A9, Enter.** While the list shows, the line submitted is the marked row,
  not what was typed (`resolvedSlashSubmission`,
  `input_submit_runtime.zig:719-729`, `:1098-1101`;
  `resolveSlashSubmission`, `:1818-1823`), and the history records that line
  (`:1009-1016`, `:1059-1064`).
- **A10, the rows.** Under the anchor, past the draft's leading blanks
  (`slashRawAnchor`, `input_presentation.zig:287-292`); an anchored prefix has
  no slash layout, so no header (`picker_presentation.zig:710-732`), and is
  painted by the option-row path (`paint_plan.zig:911-923`,
  `picker_presentation.zig:1446-1486`) in the list picker's rows
  (`surface_frame.zig:659-666`).
- **A11, one kind.** The popup, the argument lists and the dim ghost are all
  `.slash` (`inlinePickerTriggerKind`, `picker_state.zig:287-296`); Escape
  dismisses the visible kind, and that lasts while the kind is the same
  (`visibleInlinePickerKind`, `input_completion_runtime.zig:133-169`;
  `suppressionAfterEdit`, `picker_state.zig:515-532`).
- **A12, the ghost.** The dim rest after the caret is a mid-line slash
  word's (or a `$word`'s), never an argument's (`visibleInlineSlashCompletion`,
  `input_completion_runtime.zig:653-667`; `findInlineSlashQuery`,
  `picker_state.zig:613-624`).
- **A13, `/mcp`'s list.** The eight verbs, then each server's lines
  (`app_mcp_runtime.zig:459-468`, `:487-513`; `runtimeArgQuery`,
  `command_specs.zig:876-907`).
- **S1, not this unit.** An unanchored prefix lists the skills its words find
  after the commands (`picker_presentation.zig:1170-1227`; `skillMatchRank`,
  `skill_runtime.zig:1380-1392`; bound over the whole prefix by Tab or Enter,
  `app_input_runtime.zig:2366-2396`, `:2451-2476`).

## Per command

Each anchored command of `argCompletionAnchor` (A2), its list, and what this
unit did with it. "Copied" means the rows, Tab, Enter and Escape above, the
pair identical (`green.txt`) and a shell case pinning it.

| command | fx's list (`command_specs.zig@481104fd`) | here | evidence |
|---|---|---|---|
| `/mcp ` | the eight verbs, then each server's lines (A13) | **already copied** (MCP-VERBS): the list is unchanged; its Escape is now fx's one `.slash` dismissal (A11), where it kept a latch of its own | `-mcp` same at the base and the head; `one_dismissal_closes_the_slash_menu_and_every_argument_list_for_the_line` |
| `/statusline ` | `context`, `session`, `workspace` (`:750-754`) | **copied**; Enter on `/statusline ` flips `context`, as fx's does | `-statusline`, `-statusline-s`, `-statusline-ws`, `-statusline-enter`; `statusline_lists_its_items_under_the_argument_and_enter_runs_the_marked_one` |
| `/sound ` | `on`, `off`, `max` (`:756-760`) | **copied, `max` recorded**: listed as fx lists it; xfx has not got `max` (the `/sound` row's "Not ported"), so it is refused as before | `-sound`, `-sound-on-enter`; `sound_lists_fxs_levels_and_enter_runs_the_marked_one` |
| `/permissions ` | `ask`, `auto`, `remember`, `revoke`, `full-access`, `reset` (`:762-769`) | **copied**; Enter on a prefix runs the marked form: `/permissions ` sets `ask`, `/permissions full` sets full access, as fx's do | `-permissions`, `-permissions-re`, `-permissions-enter`, `-permissions-full-enter`; `permissions_lists_its_six_forms_and_tab_writes_the_marked_one`, `an_argument_nobody_understands_is_the_usage_line_and_changes_nothing` |
| `/workspace ` | `list`, `add`, `remove`, `clear` (`:771-776`) | **copied**; Tab writes `add `/`remove ` with their space, Enter on `/workspace ` is `list` | `-workspace`, `-workspace-a-tab` (and its `.cursor`, 17,2 on both), `-workspace-enter`; `workspace_lists_its_verbs_and_tab_leaves_room_for_a_path` |
| `/allowlist ` | the walk (`:778-841`, `:1042-1276`) | **copied, two tools recorded**: `install_skill` and `subagent` are fx's tools, not xfx's; listed as fx lists them and refused by `/allowlist add tool` as before (`ADVERTISED_TOOLS`) | `-allowlist`, `-allowlist-add`, `-allowlist-add-tool` (six rows in view of eight, on both), `-allowlist-local`; `allowlist_walks_its_words_under_the_one_being_typed`, `arg_lists::tests` |

The dim ghost (A12) is no argument's: `/statusline /co` shows `/copy`'s rest
and no list on both sides, the same at the base (`-statusline-ghost`).
Escape over `/statusline ` and a Backspace leaves `/statusline` with no menu
(`-esc-back`), where the base, with no list to dismiss, opened the slash menu.

## What xfx did at the base

`picker::query_of` ended the slash query at the first blank, and only `/mcp `
had a list past it (`mcp_args`, with its own Escape latch,
`mcp_args_dismissed`, that lasted while the line was a `/mcp ` one). So no
list after any of the five other names (`red/`), Enter sent what was typed,
and Escape over `/mcp ` and a Backspace to `/mcp` opened the slash menu that
fx keeps closed.

## The change (`da9222bc`)

- `arg_lists.rs` (new): fx's five tables and `/allowlist`'s walk; `of`
  returns the head (fx's spelling of the command and the words before the one
  being typed, its space included) and the rows the query finds, each with
  whether Tab adds a space.
- `mcp_args.rs`: `McpArgs::fixed` holds a table in the same struct, so a
  table has `/mcp `'s rows, keys, height and place: a label offset per list,
  no description, never read again (`due`), and `completion` for Tab.
- `shell.rs`: past the `@` word and the dismissal, a table line opens its list
  and no slash menu; Tab writes `completion`; Escape over any argument list
  dismisses the slash kind (`Self::dismissed`), and `/mcp `'s list asks that
  dismissal too -- `mcp_args_dismissed` is gone. Enter's path is `/mcp `'s,
  unchanged.
- Four older shell cases typed lines the list now resolves (`/sound ON`,
  `/statusline Context`, `/permissions full`, and `/permissions \t` with the
  tab as a key, which now completes): each sends its line with nothing to
  resolve -- a word nothing begins, or the list closed by Escape and the tab
  pasted -- and the resolution is pinned beside it. The parser's own case
  rules stay pinned in `crate::sound` and `crate::interactive`.

## RED (`red.txt`)

The six shell cases, as committed at `da9222bc`, put onto the base's
`src/tui/shell.rs` in a throwaway `git clone --no-local`: 6 of 6 fail -- no
rows for any of the five commands (`left: []`), and `/mcp ` Escape and a
Backspace bringing the slash menu back. `arg_lists`' three cases name a module
the base has not got. The paired scenario on the base (`red/`): 17 of 20
pairs differ and one is missing -- the base's `/statusline ` and Enter opened
the bare picker, which took every key after it, so its run stopped at the
last `until shown` (`red.txt` quotes the screen it waited on); the two the
same are the ones this unit does not change: `/mcp `'s list (`-mcp`) and the
ghost (`-statusline-ghost`).

## GREEN (`green.txt`)

19 of 20 pairs identical at the head (`pairs.sh xfx`); the 20th,
`-permissions-full-enter`, differs only by the product's name in the
full-access warning (`fx permission checks` / `xfx permission checks`), which
the ledger's `product-name` normalization reads as the same: the head sweep
has all 20 of the scenario's pairs normalized-identical (`sweep-head.md`,
its `p5-slash-arg-skills` row). 

## The second part's probe (`probe/`, `probe.txt`; `probe-mixed.cap`)

fx lists `release-notes` for `/git log` under `Results 1`, found by its
description (rank 2 of `skillMatchRank`), and Tab binds it over the whole
`/git log`; the base and the head show `/git log` with no menu. `/skills x`,
`/help x` and `/rename notes` show no menu on any side (no skill's name,
description or path holds the words; `/help` takes nothing). For the unit
that copies S1.

## Mutants (`mutants.py`, `mutants.txt`)

14 of 14 killed at the code head `245e99ec`, each on a throwaway
`git clone --no-local` (its source restored after each, and clean at the
end), against `tui::arg_lists::`, `tui::mcp_args::`, the six new shell cases
and five older `/mcp ` ones. Each takes one rule out: no table (A1, the base's
rule), the match with case (A2), the four tables' query not trimmed at its end
(A3), no tab boundary (A4), the rows not past the draft's blanks (A5),
`/allowlist`'s scopes (A6) and its `tool ` step (A7) not walked, Tab with no
space (A8), Enter sending what was typed (A9), Escape's own latch back (A10),
`/mcp `'s list ignoring the slash dismissal (A11), a table read again (A12),
a recalled table line opening its list (A13), a row showing its whole line
(A14). `mutants.txt` names the cases that caught each. Round 1, at
`f0399486`, killed 13: A11 survived (the older `/mcp ` case reads the slash
menu only), and `f7baf771` added the assertion that kills it.

## Sweep (`sweeps.sh`, `sweeps.txt`, `sweep-base.md`, `sweep-head.md`, `sweep-compare.txt`, `cmp_sweeps.py`)

`scripts/fxcap/sweep.sh --only 'slash*' --only 'p5-slash*' --only 'p5-mcp*'
--only 'statusline*' --only 'permissions*' --only 'skills*'` at 40x120,
`--jobs 3`, with this branch's scenarios and ledger (at `f0399486`; the
later re-point changes numbers in `source` fields only), once with the base
binary and once with the head's, in one suite-lock slot. 56 scenarios, 300
pairs. `cmp_sweeps.py` is SLASH-LEADING-WS' copy, unchanged.

- **base 206 / 300, head 226 / 300; drops 0**; pairs with uncaused rows 59 on
  each side, the same pairs. The 20 that moved are this unit's scenario's,
  `failed` on the base (its run stopped, above) and normalized-identical at
  the head. Nothing outside it changed (`sweep-compare.txt`).

## Gate (`gate.txt`, `gate.sh`)

At `245e99ec`, the code head (src as `f7baf771`), the worktree clean, in one
suite-lock slot: `cargo fmt --check`, both clippys with `-D warnings`,
`cargo test --locked --no-fail-fast` (5706 passed, 0 failed, 7 ignored; the
lib 4782), the same with `--features fault-injection` (5743 passed, 0 failed,
7 ignored; the lib 4805), `check-xfx-identity`, `check-no-stubs`,
`check-no-secrets` and `check-no-local-paths` -- every step exit 0, nothing
re-run. The base's counts were 5697 and 5734 (SLASH-LEADING-WS' gate): nine
cases more, the six shell cases and `arg_lists`' three. The commits after it
are `.prd/` and one `docs/parity.md` wording fix (the Tab-space clause), after
which the four check scripts ran again (`checks-final.txt`).

## References (`repoint.py`, `repoints.txt`, `refs.txt`)

`repoint.py` (SLASH-LEADING-WS' with this unit's files) re-pointed the ledger
refs the code moved -- 14 at `da9222bc`, six at `f7baf771` -- each to the one
line its text stands on now, numbers only (`repoints.txt`). `refs.txt`: train
13's `refs_content.py` at the head -- the ledger 115 SAME, 38 SAME-ROW, 1
SAME-LINES, 1 WRONG; `docs/parity.md` 25 SAME, 0 WRONG. The base gave 116
SAME and 37 SAME-ROW: the ledger's ref at :61, to `docs/parity.md:118`
(the `/version` row, its own text unchanged), is now SAME-ROW: the `/sound`
and `/workspace` rows just above it, which this unit rewrote in place, are in
the window of two either side it compares. The one WRONG (ledger:28, into
`.prd/07-fx-parity-program.md`) is the base's.

## `docs/parity.md`

- The full-screen TUI row: SLASH-LEADING-WS' F9 sentence ("Past a blank after
  the word upstream's query goes on … which keeps its own Escape here") is
  replaced by what holds now -- the six lists, their rows, Tab, Enter on the
  marked line, the one Escape -- and by what is not copied yet (the skills, S1,
  owner SLASH-ARG-SKILLS' second part); "its argument completion are absent"
  is gone from the sentence before it; the page-keys paragraph names the tables
  beside `/mcp `'s list.
- A record in the `/permissions`, `/allowlist`, `/statusline`, `/sound` and
  `/workspace` rows (the TUI's list, Enter on a prefix, and `max`,
  `install_skill`, `subagent` listed and refused); the `/mcp` row's Escape.

## Not copied, and why

- **The skills past a blank** (S1): the split, above.
- **`max`, `install_skill`, `subagent`**: listed as fx lists them; xfx has not
  got them, so Enter on one is the refusal it was before. Building them is
  not this unit's (the brief: record, do not build).
- **Page keys over the lists**: fx pages its slash lists, these included;
  xfx binds no page key on `/mcp `'s list or the tables (the TUI row's page-key
  paragraph, owner PAGE-KEYS-SETTINGS-COLUMN). The same at the base for `/mcp `.


## For the dispatcher

- **Enter on a prefix runs the marked row, `/permissions` included.** fx's
  rule (A9), copied: `/permissions full` and Enter sets full access, `/permissions `
  alone sets `ask`, `/workspace ` alone lists -- each with the row marked in
  view before the key (`-permissions-full-enter`, `-permissions-enter`,
  `-workspace-enter`). It changes what four older cases typed; it adds no new
  way to reach full access (Shift-Tab and `/permissions full-access` were
  there), but a security review may want to rule on it.
- **`max`, `install_skill`, `subagent` are listed and refused.** Copied as fx
  lists them, so the screens pair; dropping them from the tables instead is a
  one-line change each in `arg_lists.rs` and a recorded divergence.
- **The second part** (S1, the skills past a blank) is the next unit: its fx
  evidence is `probe/`, its scenario `probe-mixed.cap`, its code site
  `picker::query_of` and `Picker::over` (both end the query at a blank today).
