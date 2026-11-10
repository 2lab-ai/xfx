# SLASH-MIXED-SKILLS receipt: the skills a slash line finds past its blank (part 2 of SLASH-ARG-SKILLS)

SLASH-ARG-SKILLS copied fx's argument lists past the blank after six
commands and split this off (its receipt's README, "The split" and "The
second part's probe"): past the blank after a slash word fx does not know,
or one that takes an argument and has no list of its own, fx's popup goes on
and lists the skills the line finds, after the commands. This unit copies it.

The base is SLASH-ARG-SKILLS' closed head `b9b11596`. fx is the v0.0.16
release (pin `481104fd`), sha256 `8e6cda2e…`, run with `FX_AUTO_UPGRADE=0`;
`capture.sh` checked its bytes after every run (`caps.sh`, one suite-lock
slot, every run exit 0). No `gwlog` step, so no request body is in the
receipt.

| commit | what |
|---|---|
| `6ff30667` | the paired scenario `p5-slash-mixed-skills` and its workspace `slash-mixed-ws` (five skills in `skills/`, the same for both sides), `fx_src.sh` and `fx-src.txt`; no `src/` change |
| `fb014b08` | **the code**: `src/tui/picker.rs`, `src/tui/arg_lists.rs`, `src/tui/shell.rs`, their tests, `docs/parity.md` |
| `83dae68e` | **the code head**: the 14 ledger refs `fb014b08` moved, re-pointed (`repoint.py`, `repoints.txt`); numbers only. The captures' head binary was built here, and the gate and the mutants ran here |
| the commits after | this receipt (`.prd/`) only |

The non-test source is 106 lines added (61 of them comments) and 42 removed
(`count.py`, `count.txt`), under the brief's 300: no split.

The captures in `xfx/` and `probe/head-*` come from the binary built at
`83dae68e` (row 1 `x𝒇x v0.1.0-83dae68 [debug]`); those in `red/` and
`probe/base-*` from the base (`v0.1.0-b9b1159`); `fx/` and `probe/fx-*`
from fx.

## What fx v0.0.16 does (`fx-src.txt`, made by `fx_src.sh`; 28 ranges, each range and needle checked, exit 0)

- **M1, which lines.** `slashCompletionPrefix` (`command_specs.zig:568-576`):
  the draft past its leading ` \t\r\n`, if it begins with `/`; one word,
  always; past a blank, the whole line while its first word is no command
  (`registry.lookup`, exact and case kept, a name or an alias,
  `src/core/mods/registry.zig:42-55`) or one that `has_args`; for one that
  takes none, nothing.
- **M2, anchored never.** `mixedSlashCompletionCount`
  (`picker_presentation.zig:1170-1175`) is the commands alone while
  `argCompletionAnchor` is not zero -- `/mcp` and the five tables, then a
  space or a tab (`command_specs.zig:636-644`, `:909-915`) -- and the
  commands then the skills otherwise (`:1177-1189`, `:1213-1220`,
  `prepareSlashMenu` `:1139-1168`): the skills `skillMenuSkillAtQuery` finds
  on `.all` for the prefix without its slash.
- **M3, no command past a blank.** A command matches the whole prefix
  (`completionCommandMatchRank`, `:520-526`; `slashCompletionCount`,
  `:578-608`); no name has a blank, so none does.
- **M4, the ranking.** `skillMatchRank` (`skill_runtime.zig:1380-1392`)
  trims ` \t\r\n/` from both ends of the query: empty matches every skill;
  then the name begins with it, has it, or the description, the short source
  or the path has it, case ignored -- one string, not word by word. The menu
  orders by that rank, then the source group, then discovery order
  (`SkillMenuView`, `:1330-1376`).
- **M5, the rows.** `Results N` once a skill is listed, `· type to filter`
  only for the bare `/` (`composeSlashMenuHeaderRow`, `:1229-1245`); a
  skill row is its name, description and short source (`slashMenuRowContent`,
  `:1280-1302`).
- **M6, the kind.** The popup's count is the mixed one while the draft is
  the `.slash` kind (`input_completion_runtime.zig:82-109`); a `/model` or
  `/provider` line is its own kind first (`inlinePickerTriggerKind`,
  `picker_state.zig:287-296`).
- **M7, Tab.** The ghost first (`visibleInlineCompletion`), then the marked
  row: a skill is bound, a command written (`app_input_runtime.zig:1304-1353`;
  the ghost is a mid-line slash word's, `input_completion_runtime.zig:653-667`).
- **M8, Enter.** `submitSlashPickerSelection` (`app_input_runtime.zig:2366-2396`,
  asked at `:1391-1400`) binds a marked skill unless `matchExact` holds on
  the prefix -- the name or an alias, or one with spaces and tabs after it
  (`registry.zig:47-55`, `:69-78`); then the submit resolves the line to the
  marked command, or with none to resolve to, sends it as typed
  (`resolvedSlashSubmission`, `resolveSlashSubmission`,
  `input_submit_runtime.zig:1098-1101`, `:1818-1823`).
- **M9, the bind.** The skill's token over the whole prefix, the blanks
  before it kept (`bindSelectedSlashSkill`, `app_input_runtime.zig:2451-2476`).
- **M10, the dismissal.** One `.slash` kind, whose Escape lasts while the
  kind is the same (`visibleInlinePickerKind`,
  `input_completion_runtime.zig:133-169`; `suppressionAfterEdit`,
  `picker_state.zig:515-532`).

The oracle (`fx/`, 26 snaps) shows each: `/git log` lists `release-notes`
by its description under `Results 1` (`-git-log`); `/notes ` lists
`notes-index`, `release-notes`, `tidy` -- named, holding, saying `notes`
(`-notes`); `/ ` lists all five, `Results 5` (`-slash-space`); `/Help ` --
no command, case kept -- lists `pointer` and `tidy` (`-cap-help`);
`/rename the help` lists `tidy` (`-rename-the-help`); `/help `,
`/clear the notes`, `/statusline bell` list nothing, and `/statusline `
lists its table alone, not `chime` (`-help`, `-clear-the-notes`,
`-statusline-bell`, `-statusline`); Tab and Enter on `/git log` bind
`release-notes` (`-git-log-tab`, `-git-log-enter`), past leading blanks too
(`-lead-tab`), after Down the second row (`-notes-down-tab`); Enter on
`/rename the help` binds `tidy`, Enter on `/rename ` submits it, `usage:
/rename <title>` (`-rename-the-help-enter`, `-rename`, `-rename-enter`);
`/x /he` shows the ghost `lp` and the list, Tab takes the ghost, Enter binds
`pointer` (`-ghost-list`, `-ghost-list-tab`, `-ghost-list-enter`); Escape
over `/git log` keeps it closed past ` x` (`-git-log-esc`, `-git-log-esc-x`);
and part 1's `/skills x`, `/help x`, `/rename notes` list nothing.

## What xfx had, what is shared, what is new

At the base `picker::query_of` ended the query at the first blank, so no
line past a blank opened the slash menu; Enter then sent `/git log` to the
model as a prompt and ran `/rename the help` (`red/`).

**Shared, already fx's and unchanged**: the skills' matching and order --
`skills_menu::rank` (`skillMatchRank`), `listed` (`SkillMenuView`) and
`matching` (`skills_menu.rs:149-200`), the `$` menu's and the slash menu's
before the blank; the menu's rows and header (`picker::Entry`,
`Picker::header`, `item_row`); the bind (`Shell::bind_skill_token`); the
dismissal (`picker::Dismissed`); and the shell's order, which settles the
`/provider` column, `/mcp `'s list, the `/model`/`/effort` column and the
tables before the slash menu (`Shell::reconcile_picker`, `completion_kind`).

**New** (`fb014b08`):

- `picker::query_of` / `prefix_in` (`picker.rs:203-224`): fx's prefix (M1),
  and never an anchored line's (`arg_lists::anchored`, `arg_lists.rs:120-122`,
  M2). The shell's order already keeps a table or a `/mcp ` line from the
  slash menu (the tables open first, and one with no row is still the
  table's); `anchored` makes the query say so itself, so `Picker::open` on
  such a line is `None` whoever asks.
- `Picker::over` matches the commands and the skills with that prefix,
  which is what puts skills, and no command, past a blank (M3).
- `names_exactly` and `Picker::exact` (`picker.rs:231-238`): fx's
  `matchExact` on the line, for Enter (M8); the old test, a command row of
  rank `Exact`, could not see `/rename ` with its blank.
- `PickerOutcome::Typed` (`picker.rs:398`; `shell.rs:8852`): Enter on a skill
  row of such a line with no command row goes on and submits it as typed;
  the old arm took the mark modulo a command count of zero.
- Tab yields to the ghost while one shows (`shell.rs:8822-8828`, M7).

## Per rule

| rule | fx | here | evidence |
|---|---|---|---|
| a name nobody has lists past its blank | M1 | copied | `-git-log`, `-cap-help`, `-slash-space`; `past_the_blank_of_a_name_nobody_has_the_menu_lists_the_skills_the_line_finds`; picker `past_a_blank_the_query_goes_on_…`; mutants M1, M5 |
| a name with an argument and no list lists past its blank | M1 | copied | `-rename-the-help`, `-rename`; `a_name_with_no_argument_…`; M2, M6 |
| a name with no argument lists nothing past it | M1 | already so (the base's query ended at the blank); kept | `-help`, `-clear-the-notes`, `-help-x`; `a_name_with_no_argument_…`; M3 |
| an anchored line lists its argument, never a skill | M2 | already so (the shell's order); kept, and said by the query | `-statusline`, `-statusline-bell`; `a_name_with_no_argument_…`; picker case; M4 |
| no command past a blank; the line matched whole | M3, M4 | copied (the prefix) / shared (the ranking) | `-git-log`, `-notes`; M7, M8 |
| `Results N`, a skill's three columns | M5 | shared | every listing snap |
| Tab binds the marked skill over the whole line, blanks kept | M7, M9 | copied (the query) / shared (the bind) | `-git-log-tab`, `-lead-tab`, `-notes-down-tab`; `tab_and_enter_bind_…`; M12 |
| Tab takes the ghost first | M7 | copied | `-ghost-list`, `-ghost-list-tab`; `tab_takes_a_slash_words_ghost_…`; M13 |
| Enter binds unless the line is a command's name; a name and a blank is submitted as typed | M8 | copied | `-git-log-enter`, `-rename-the-help-enter`, `-rename-enter`, `-ghost-list-enter`; `tab_and_enter_bind_…`; picker `enter_on_a_skill_row_…`; M9, M10, M11 |
| one Escape lasts while the line is a slash line | M10 | shared | `-git-log-esc`, `-git-log-esc-x`; `escape_over_the_skills_past_a_blank_…` |

## RED (`red.txt`, `red.sh`)

- **The shell cases**: the five, with their helpers `mixed` and
  `slash_rows`, as committed at `fb014b08`, put onto the base's
  `src/tui/shell.rs` in a throwaway `git clone --no-local` (`red.sh`): 5 of 5
  fail -- no rows past the blank (`left: []`), `/git log` and Tab left as
  typed, and `/git log` no menu to dismiss. picker.rs' two new cases name
  `names_exactly` and `PickerOutcome::Typed`, which the base has not got.
- **The paired scenario on the base** (`red/`, `pairs.sh red`): 0 of 26
  identical. `-git-log` and `-git-log-tab` differ by the list and the bind;
  from `-git-log-enter` on, every base snap also holds what its Enters did:
  `/git log` went to the model (the fake gateway's HTTP 500), `/rename the
  help` renamed the session, `/x /he` went to the model.

## GREEN (`green.txt`)

26 of 26 pairs identical at the head (`pairs.sh xfx`), no normalization
used.

## Part 1's probe (`probe/`, `probe.txt`, `probe.sh`)

SLASH-ARG-SKILLS' `probe-mixed.cap`, over `skills-ws`, run again on fx, the
base and the head (`caps.sh`): `-git-log` and `-git-log-tab` now the same as
fx at the head (`Results 1`, `release-notes`; bound), different at the base;
`-skills-x`, `-help-x`, `-rename-notes` the same on all three.

MUTANTS-SECTION

## Sweep (`sweeps.sh`, `sweeps.txt`, `sweep-base.md`, `sweep-head.md`, `sweep-compare.txt`, `cmp_sweeps.py`)

`scripts/fxcap/sweep.sh --only 'slash*' --only 'p5-slash*' --only 'skills*'
--only 'p5-skills*' --only 'p5-input-skill*'` at 40x120, `--jobs 3`, with
this branch's scenarios and ledger (at `83dae68e`), once with the base
binary and once with the head's, in one suite-lock slot. `p5-skills*` and
`p5-input-skill*` name no scenario on this branch. 12 scenarios, 124 pairs.
`cmp_sweeps.py` is SLASH-ARG-SKILLS' copy, unchanged.

- **base 68 / 124, head 94 / 124; drops 0** (`sweep-compare.txt`). The 26
  that moved are this unit's scenario's, `unresolved-defect` on the base and
  normalized-identical at the head; nothing outside it changed. Pairs with
  uncaused rows: 44 on the base, 18 at the head -- the base's 44 less this
  scenario's 26.
- These families are every scenario a skill can be listed in: the only
  workspaces with a `SKILL.md` are `slash-mixed-ws` and `skills-ws-*`, used
  by `p5-slash-mixed-skills`, `p5-slash-leading-ws-skill` and the four
  `skills*`, and the only scenario that makes a skill mid-run is `skills.cap`
  (`/skills create`, `/skills add`). Past a blank with no skill, no menu
  opens, as at the base.

## Gate (`gate.sh`, `gate.txt`)

At `83dae68e`, the code head, on a clean `git clone --no-local` of it
(porcelain 0), in one suite-lock slot with a short `TMPDIR`: `cargo fmt
--check`, both clippys with `-D warnings`, `cargo test --locked
--no-fail-fast` (5715 passed, 0 failed, 7 ignored; the lib 4791), the same
with `--features fault-injection` (5752 passed, 0 failed, 7 ignored; the lib
4814), `check-xfx-identity`, `check-no-stubs`, `check-no-secrets` and
`check-no-local-paths` -- every step exit 0, nothing re-run; no catalog-wait
case failed this time. The base's counts (SLASH-ARG-SKILLS' round-2 gate)
were 5708 passed (lib 4784) and, with fault injection, 5742 passed and the
three catalog-wait cases that failed on timing there (lib 4804 and those
three): seven cases more on each, picker.rs' two and the shell's five. The
commits after it are `.prd/` only, after which the four check scripts ran
again (`checks-final.txt`).

## References (`repoint.py`, `repoints.txt`, `refs.txt`)

`repoint.py` (SLASH-ARG-SKILLS' with this unit's files) re-pointed the 14
ledger refs `fb014b08` moved, each to the one line its text stands on now,
numbers only (`repoints.txt`). `refs.txt`: train 13's `refs_content.py` at
`83dae68e` -- the ledger 115 SAME, 38 SAME-ROW, 1 SAME-LINES, 1 WRONG, as at
the base; `docs/parity.md` 25 SAME, 0 WRONG, and seven refs more than at the
base, all fx's (`other product`, the TUI row's new citations). The one WRONG (ledger:28, into
`.prd/07-fx-parity-program.md`) is the base's.

## `docs/parity.md`

The full-screen TUI row: the sentence that left the skills past a blank to
"SLASH-ARG-SKILLS' second part" now says what holds -- which lines list them,
how they are matched and ordered (the `$` menu's ranking, shared), the
header, Tab and Enter, the ghost first, the one Escape -- with the fx
citations and `fx-p5-slash-mixed-skills-*`; the sentence on the query
before it says the draft past its blanks is the query. No owner is left. It
records one registry difference this rule makes visible: fx's `/feedback`
takes no argument and xfx has no `/feedback`, so past its blank xfx lists
the skills the line finds where fx lists nothing (fx's `/logout` and
`/shell` take one, and xfx, which has neither, lists them as fx does; fx's
`/login ` line is the provider kind, which the row already records).

## Not copied

Nothing of the rule. The registry differences above are each registry's,
recorded, not this unit's to change.
