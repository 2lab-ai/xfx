---
title: 0.0.16 delta -- CTRLO-OTHER-PICKERS (the pickers under Ctrl-O)
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, input, full-transcript, pickers]
---

# CTRLO-OTHER-PICKERS

Base: `cb7c9c18` (CTRLO-AT-KEYS, closed, not yet on `main`). The brief's
gap: under Ctrl-O -- the full transcript -- nine pickers offered before or
after the `@` list take `C-j`/`C-k` as moves, where fx's composer takes
them. **fx's rule at the pin (`481104fd`, executable sha256 `8e6cda2e…`)
is two rules, and the brief's gap holds for three pickers of the nine:**

- **Group A -- the help catalog, `/model`'s menu, the session picker,
  `/fork`'s and `/effort`'s menus, the skills menu (`/skills`' and a `$`
  word's) and, the seventh on the same fx line, the settings menu: fx never
  shows one under Ctrl-O.** Before every frame fx closes the full
  transcript while one is up (`reconcileBeforeFrameRender`,
  `app_render_runtime.zig:1206-1215`), so Ctrl-O over one opens the screen
  and its frame shuts it, and every key after is the menu's own, as without
  Ctrl-O. xfx held the screen over them: Up, Down and Escape went to the
  transcript.
- **Group B -- the slash menu, the `/provider` column and `/mcp `'s
  argument completions: the screen holds, and `C-j`/`C-k` are the
  composer's** (`routePickerControlByte`, `app_input_runtime.zig:1093-1116`),
  as the brief said. xfx moved the picker.

**Round 2** (on `69491c9a`, from gpt6's and opus' reviews of it; §11): fx's
question and approval terms are shown redundant by a test, not copied (gpt6
MUST 1); a `C-j` or `C-k` in the burst a Ctrl-O came in over a group A menu
is now the composer's, as fx's (gpt6 MUST 2, the dispatcher's call) -- one
check in `consume_inputs` where four guards were; one anchor function (opus
SHOULD 1); the wordings opus asked for, here and in `docs/parity.md`.

| commit | what |
|---|---|
| `67c8d0d1` | RED: one case per picker, the menus that open under the screen, the `@` list's arrows with the transcript's offset, Shift-Up/Down; the ten scenarios (`red-unit.txt`) |
| `bab45335` | the fix: `Shell::reconcile_full_transcript` (group A) and the three guards (group B) |
| `1faed29d` | `docs/parity.md`: each picker's row and the full transcript's (§7) |
| `a0634606` | the ledger's 16 refs re-pointed (`refs.txt`) |
| `9545abe4` | RED: the `/provider` column's anchor off the caret's row (`red-unit.txt`, its second part) -- `provider-ctrlo cj` showed it |
| `9ffaef29` | the anchor fix, and the two parity rows that name it -- round 1's code head |
| `91c46731` | the ledger's refs past the anchor's lines re-pointed -- round 1's build (`v0.1.0-91c4673`) for the captures, the sweep and the mutants' clone |
| `76a5052d` | this folder's runners, fx quotes, captures, unit runs and refs -- round 1's gate commit |
| `0ab399ed` | two parity wordings (`/settings`' pointer, `/provider`'s anchor sentence); `check-no-stubs` and `tests/parity.rs` run again after it (§8) |
| `e46116ed`, `69491c9a` | round 1's receipt: this README, `gate.txt`, `fxcites.txt`, `mutants.txt`, the sweep's files (`sweep-base.md`, `sweep-head.md`, `sweep-compare.txt`, `remote-rerun.txt`), `equivalent-mutants.txt` corrected (§5) |
| `69743117` | round 2's RED: the modal invariant, C-j and C-k in Ctrl-O's burst over each group A menu, the turn's plane (`red-unit.txt`, its third part); the burst steps in `p5-ctrlo-help` and `p5-ctrlo-fork`, Backspace and Alt-Enter in `provider-ctrlo` |
| `3369622a` | round 2's fix: one check in `consume_inputs`, the composer's `C-j`, one anchor; the settings menu's burst -- **the code head** |
| `13ed6a79` | round 2's parity wording: the full transcript row's burst, the settings row's `C-j` |
| `13646dfa` | the ledger's 13 refs past round 2's lines re-pointed -- **round 2's build** (`v0.1.0-13646df`) for the captures and the sweep |
| `5d6c4cbb` | round 2's runners: `fx_src.sh`'s fifth part, `mutants.py` -- **round 2's gate commit** and the mutants' clone |
| (receipt, two commits) | round 2's receipt: this README's §11 and the corrections it lists, `xfx-r1/` (round 1's captures, moved), `xfx/` and three scenarios' `fx/` again, `pairs.txt`, `gate-r2.txt`, `mutants-r2.txt`, `sweeps-r2.sh`, `fxcites.txt`, `refs.txt`, `red-unit.txt`, `green-unit.txt` and `equivalent-mutants.txt` extended; then the sweep's files and §11.7 |

Every fx line cited here is printed in `fx-src.txt` (`fx_src.sh`, at
`481104fd`); "fx-src.txt:N" is its line. `fxcites.py` (CTRLO-AT-KEYS', with
this unit's scenarios -- ten, thirteen since round 3 -- as its fourth
input) checks every fx citation in
this README, in the parity rows' changed text, in the lines this unit added
under `src/` and in the scenarios' headers against `fx_src.sh`'s ranges
(`fxcites.txt`: round 1 159 citations, round 2 205, round 3 234, 0 failed; its
self-test, 10 cases, 0 wrong).

## 1. fx's rule

### Group A: the frame closes the screen

- Ctrl-O is `toggle_full_transcript` (`escape_parser.zig:270`,
  fx-src.txt:104). It reaches `routeComposerAction`
  (`app_input_runtime.zig:997`; `input_full_transcript_runtime.zig:54-59`),
  and `transitionScreen` opens the screen (`:95-111`) and logs
  `depth_transition ... trigger=ctrl_o` (`:121-126`).
- Every frame first calls `reconcileBeforeFrameRender`
  (`app_render_runtime.zig:1254`), which closes the screen while
  `settingsMenuActive`, `helpMenuActive`, `skillsMenuActive`,
  `modelMenuActive`, `sessionMenuActive`, `forkMenuActive` or
  `effortMenuActive` (`:1206-1215`, fx-src.txt:16-25), the seven tests at
  `app_render_runtime.zig:2001-2033` -- the skills menu while it shows
  (`menuVisible`, `:2002`), `/model`'s whether opened by `/model` or by the settings menu's Model
  row (`model_cache.menu.active`, `:2007`). `closeFullTranscriptIfActive`
  (`app_lifecycle.zig:1277-1286`) -> `closeFullTranscript`, which logs
  `close_full_transcript restore=` (`:1396`).
- **fx's own trace** (`fx/fx-p5-ctrlo-help-40x120-end.files`, the scenario
  run with `FX_TRACE_LOG` and `FX_TRACE_SCOPES=full_transcript`,
  `debug_trace.zig:413-434`): eleven Ctrl-Os, each an `open_request
  state=ready`, a `depth_transition from=inline to=full ... trigger=ctrl_o`
  and a `close_full_transcript restore=exact` in the same millisecond, and
  no `close_screen trigger=` line, which an Escape or Ctrl-C close would
  print (`input_full_transcript_runtime.zig:129-134`) -- so the close is
  the frame's.
- With the screen gone the next key is the menu's, as without Ctrl-O:
  `C-j`/`C-k` through `routePickerControlByte` (`app_input_runtime.zig:
  1093-1116`) into `routeNonSlashPickerMove`, each menu's move
  (`input_completion_runtime.zig:395-406`); Up and Down through
  `routeComposerVertical` the same way (`:230`); Tab and Enter the
  composer's bytes (`app_input_runtime.zig:1304-1311`, `:1392-1404`); a
  lone Escape through `flushPendingEscape` (`:658-677`) into
  `resolveEscape`, which asks the transcript first (`:2688`, not up) and
  then closes the menu (`:2765`).
- A menu that opens while the screen is up -- `$` typed there, `/help` and
  Enter (the composer's, `:1392-1395`), a session list or a `/fork`
  answer arriving -- is shut out the same way at the next frame.

### Group B: under the screen, C-j and C-k are the composer's

A raw byte goes to the transcript (`app_input_runtime.zig:872`; it binds
3 and 12 only, `input_full_transcript_runtime.zig:147-154`), a modal
(`app_input_runtime.zig:873`), the picker control bytes (`:879`) --
`routePickerControlByte` returns false while the screen is up
(`composerPickerSurfaceVisible`, `:1110-1116`) -- and then the composer:
`C-k` is `delete_to_line_end` and `C-j` `insert_newline`
(`shortcuts.zig:19`, `:26`), an editing byte (`isComposerEditingByte`,
`app_input_runtime.zig:1593-1607`) that no group B picker owns
(`activeCatalogMenuOwnsByte`, `:1609-1619`), so it closes nothing that is
up (`dismissActiveMenusForComposerEdit`, `:1984-1990`) and is
`routeComposerShortcutAction`'s (`:1442-1445`): a kill to the line's end
(`:479-488`), a new line (`:513-521`). `/mcp `'s argument completions are
fx's slash completion (`verb_completions`, `app_mcp_runtime.zig:459-468`;
moved by `routeSlashCompletionMove`, `input_completion_runtime.zig:175-180`),
so their trace is the slash menu's. The one picker that asks about the
screen itself is the `@` list (`filePickerEligible`,
`input_completion_runtime.zig:783`; CTRLO-AT-KEYS'). The `/model ` and
`/effort ` columns (fx's model query, `:411-414`) are not on the frame's
list either, so the screen holds over them too, and xfx's `offer_to_column`
binds neither key: no change there.

### Per picker, per key (fx at the pin)

Group A, under "Ctrl-O" -- there is no screen at the key:

| picker | C-j / C-k | Up / Down | Tab | Enter | Escape |
|---|---|---|---|---|---|
| help catalog | its move (`input_completion_runtime.zig:398`) | its move | next category (`app_input_runtime.zig:1305`) | `submitHelpMenuSelection` (`:1395`) | closes it (`:2765`) |
| `/model`'s menu | its move (`input_completion_runtime.zig:399`) | its move | next vendor (`app_input_runtime.zig:1311`) | `submitModelMenuSelection` (`:1398`) | closes it |
| session picker | its move (`input_completion_runtime.zig:404`) | its move | the scope (`app_input_runtime.zig:1309`) | `submitSessionPickerSelection` (`:1402`) | closes it |
| `/fork`'s menu | its move (`input_completion_runtime.zig:405`) | its move | nothing: no branch of `app_input_runtime.zig:1305-1353` holds over an empty composer | `submitForkMenuSelection` (`app_input_runtime.zig:1403`) | closes it |
| `/effort`'s menu | its move (`input_completion_runtime.zig:400`) | its move | nothing, the same | `submitEffortMenuSelection` (`app_input_runtime.zig:1393`) | closes it |
| skills menu (`/skills`, `$`) | its move (`input_completion_runtime.zig:402`) | its move | next source (`app_input_runtime.zig:1311`) | `submitSkillsMenuSelection` (`:1399`) | closes it |
| settings menu | its move (`input_completion_runtime.zig:397`) | its move | next category (`app_input_runtime.zig:1305`) | `submitSettingsMenuSelection` (`:1394`) | closes it |

Group B, under Ctrl-O -- the screen is up:

| picker | C-j | C-k | Up / Down / PgUp / PgDn | Tab | Enter | Escape |
|---|---|---|---|---|---|---|
| slash menu | a new line: `/` and a line break is no slash word, the menu goes | a kill of nothing at the line's end: the mark stays | the screen scrolls (`input_full_transcript_runtime.zig:161-166`, `:188-195`), the mark stays | the completion, as without the screen (`app_input_runtime.zig:1331-1352`) | `submitSlashPickerSelection` (`:1400`) | the screen closes, the key goes no further (`:915-919`; a lone one, `:2688`) |
| `/provider` column | a new line: the column stays, `no matching providers`, hung from the caret's row (`visual_layout.zig:705-713`) | a kill of nothing: the mark stays | the screen's | `autocomplete` (`app_input_runtime.zig:1322-1323`) | the column's submit (`:1416-1418`) | the screen's |
| `/mcp ` arguments | a new line: no completion answers it | a kill of nothing: the mark stays | the screen's | the slash completion's (`:1331-1352`) | the line submitted (`:1440`) | the screen's |

Tab and Enter are bytes 9 and 13: no composer shortcut, below 32, so not
editing bytes (`app_input_runtime.zig:1593-1607`), and `handleComposerByte`
runs them exactly as with the screen closed. xfx's do the same: the
transcript's map leaves both unbound (`src/tui/full_transcript.rs`,
`FullTranscript::key`), and they go to the same `offer_*` as without it.
They are traced, not captured: Enter runs a command, uses a model,
resumes, forks, applies a level or commits a provider, and that is not
this unit's to capture.

**fx's own screens** (`fx/`, 40x120): for group A the `held` snap, taken
after Ctrl-O alone, is the composer's plane with the menu, and `down`, `ck`
and `cj` move the mark as the menu's own keys; `esc` closes the menu. For
group B, `down` and `esc` keep the mark, `ck` keeps it, `cj` is a new line
(`/`, `/provider `, `/mcp ` over an empty second row).

## 2. The copy

- **Group A: `Shell::reconcile_full_transcript`**, called once a tick from
  `Shell::settle_band`, after the input, the events and the answers that
  open menus (`settle_skills` among them) and before the frame
  (`super::event_loop` calls `settle_band` right before
  `collect_facts` and `commit_or_hold`): the full transcript closes while
  the settings menu, the catalog, the skills menu while it shows
  (`SkillsMenu::visible`, fx's `menuVisible`), `/model`'s menu, the session
  picker, `/fork`'s or `/effort`'s menu is up -- `close_full_transcript`,
  which gives the plane back and asks for a frame, as fx's
  `requestNormalViewportRecovery` does. One mechanism, fx's shape: no
  refusal over a menu in `open_full_transcript` (its one refusal, while
  `Shell::modal`, is older than this unit), which fx does not have and
  which would miss a menu opened under the screen.
- **Group B: `Shell::composer_control`** -- `C-j` or `C-k` with the screen
  up -- returned `false` early in `offer_to_picker`, `offer_to_mcp_args` and
  `offer_to_provider_picker`, so the key goes on to `act`: `ControlJ` is
  `type_character('\n')`, `ControlK` the editor's kill, the path
  CTRLO-AT-KEYS gave the `@` list. (Round 2 makes the three guards and the
  `@` list's copy one check in `consume_inputs`, §11.)
- **The anchor** (`9ffaef29`): `Shell::anchor_at`, which hangs the
  `/provider` column and `/mcp `'s completions, now copies
  `projectedAnchorColumn` (`visual_layout.zig:705-713`): under the
  argument while it starts on the caret's row, from the start of the
  caret's row otherwise -- the `@` list's column already did
  (`Shell::file_picker_column`). `provider-ctrlo cj` showed it: after C-j
  the column's `no matching providers` was ten cells right of fx's. (Round
  2: `Shell::column_anchor`, the same function for the inline model
  column (`Shell::column_rows`), is gone, and the `@` list's column hangs
  from `anchor_at` too, §11.)

Source outside the tests, round 1: 60 lines added and 3 removed in
`src/tui/shell.rs`, about half of them doc comments;
`src/tui/full_transcript.rs` gains only a test-only accessor (§9). Round
2's are in §11.

## 3. RED (`red-unit.txt`) and GREEN (`green-unit.txt`)

At `67c8d0d1` (the base's code with the cases), the sixteen run alone:
**14 failed, 2 passed**; at `9545abe4` the anchor case failed alone (its
`left: 12, right: 2`). At `91c46731` (the code head's code, the ledger
re-pointed): 17 passed.

| case | base | head (fx) |
|---|---|---|
| `ctrl_o_over_the_help_catalog_holds_no_frame_and_its_keys_stay_its` | **RED**: the transcript held over the menu for a frame | shut at the tick; Down, C-k, C-j move the mark; Escape closes it |
| `ctrl_o_over_the_model_menu_holds_no_frame_and_its_keys_stay_its` | **RED**, the same | the same |
| `ctrl_o_over_the_session_picker_holds_no_frame_and_its_keys_stay_its` | **RED**, the same | the same |
| `ctrl_o_over_the_fork_menu_holds_no_frame_and_its_keys_stay_its` | **RED**, the same | the same, Up first |
| `ctrl_o_over_the_effort_menu_holds_no_frame_and_its_keys_stay_its` | **RED**, the same | the same |
| `ctrl_o_over_the_skills_menu_holds_no_frame_and_its_keys_stay_its` | **RED**, the same (`/skills`, then `$`) | the same, both |
| `ctrl_o_over_the_settings_menu_holds_no_frame` | **RED**: the transcript held | shut, the menu kept |
| `a_menu_that_opens_under_the_full_transcript_takes_the_plane_back` | **RED**: held over `/help` | `/help` sent, `$` typed, `/fork` answered under the screen: each shuts it; `$zz` (no skill matches, so no menu shows) does not |
| `c_k_under_the_full_transcript_over_the_slash_menu_moves_no_mark` | **RED**: the mark moved (`Some(0)`) | `/`, the mark on row 1 |
| `c_j_under_the_full_transcript_over_the_slash_menu_is_a_new_line` | **RED**: `/` | `/\n`, the menu gone, nothing sent |
| `c_k_under_the_full_transcript_over_the_provider_column_moves_no_mark` | **RED**: `Gateway` | `/provider `, `Llmux` still marked |
| `c_j_under_the_full_transcript_over_the_provider_column_is_a_new_line` | **RED**: `/provider ` | `/provider \n`, the column up with no row |
| `c_k_under_the_full_transcript_over_the_mcp_arguments_moves_no_mark` | **RED**: `/mcp list` | `/mcp show` still marked |
| `c_j_under_the_full_transcript_over_the_mcp_arguments_is_a_new_line` | **RED**: `/mcp ` | `/mcp \n`, none shown |
| `a_column_hangs_under_its_argument_only_on_the_carets_row` | **RED** (at `9545abe4`): 12 | 2 after C-j, 12 again after Backspace |
| `the_arrows_and_page_keys_under_the_full_transcript_are_its_own` | pin (CTRLO-AT-KEYS', made stronger, §9) | the offset after every key |
| `shift_up_and_down_under_the_full_transcript_scroll_nothing_and_move_no_bar` | pin (§9) | no scroll, no move |

The mcp-arguments C-j case asserted `mcp_args.is_none()` at `67c8d0d1`; at
`bab45335` it asserts that none is **shown**: xfx keeps the completions of
a `/mcp ` line no row answers as an unshown list (`McpArgs::visible`), and
the screen is fx's either way (`p5-ctrlo-mcp-args cj`, §4). The base fails
it on its first assertion either way.

Group A's base `ck` and `cj` were not wrong by themselves: under the screen
the base took them as the menu's moves, fx's visible result. What the base
got wrong is the screen holding -- `held`, then Up, Down and Escape going to
it.

## 4. Pairs (`pairs.py`, `pairs.txt`; `fx/`, `xfx-r1/`, `xfx-base/`)

The ten scenarios, each step a key between a Ctrl-O and its return (group
A's `held` is Ctrl-O alone, then a second Ctrl-O, a no-op in fx and the
head, the base's close), captured with `scripts/fxcap/capture.sh` at 40x120:
fx v0.0.16 (its sha256 printed unchanged after the runs), the base's debug
build (`v0.1.0-cb7c9c1`) and round 1's (`v0.1.0-91c4673`, now `xfx-r1/`),
each banner in `pairs.txt`'s first lines. `skills-ctrlo*` and
`provider-ctrlo` are named for their families so the ledger rows those
share apply in the sweep. This section is round 1's 63 pairs; `pairs.txt`
is round 2's run, whose `round 1` column holds them (§11).

**63 pairs. Base: 11 identical. Round 1: 38 identical, 23 differing only
in the rows their scenario's `open` snap differs in, and 2 others:
`p5-ctrlo-help tab`, row 10 (`show the fx version` / `xfx`), and
`skills-ctrlo tab`, two rows -- the source tabs, where fx has five more
agents' (`Claude`, `Codex`, `Agents`, `OpenCode`, `Claw`) and its own
name, and the skill's source, `fx` / `xfx`.** (Round 1 said "2 in the
product's name", half wrong: opus' NIT 5.)

| scenario | base differs | head |
|---|---|---|
| `p5-ctrlo-model`, `-sessions`, `-fork`, `-effort` (28 pairs) | `held` (9-32 rows: the transcript's screen), the moves under it (2 rows: the mark), `esc` (6-9: the menu still up); sessions' `ck`, `cj` and `tab` and model's `tab` already identical | identical, all 28 |
| `p5-ctrlo-help` (7) | `held` 28, `down` 11, `ck` 21, `cj` 11, `esc` 9 | `esc` identical; `open` to `cj` the 9 rows `open` differs in -- the catalog's command count (`Commands 38` / `34`), `/version`'s wording and the commands only one side has; `tab` row 10, `show the fx version` / `xfx` |
| `skills-ctrlo`, `-dollar` (13) | `held` 10, the rest the tabs | `esc` identical; the rest the 3 rows `open` differs in -- the source tabs (`fx` and the other agents' roots, xfx's `xfx`), and `tab`'s 2 of them |
| `p5-ctrlo-slash` (5) | `ck` 3, `cj` 11 | `cj` identical; the rest `open`'s 1 row, `Commands 38` / `34` |
| `provider-ctrlo` (5) | `cj` 6 | `cj` identical; the rest `open`'s 3 rows, the gated sign-in providers (`vercel`, `codex`, `grok` / `gateway`, `llmux`) |
| `p5-ctrlo-mcp-args` (5) | `ck` 2, `cj` 9 | identical, all 5 |

`cells.py` compares each cell's character and attributes, so a mark's row is
compared too. Each difference left on the head is outside this unit and the
same one its scenario shows before any Ctrl-O; the sweep's ledger classifies
every pair (§6).

## 5. Mutants (`mutants.py`, `mutants.txt`, `equivalent-mutants.txt`)

`mutants.py` on a `git clone --no-local` throwaway at `91c46731`, one at a
time, `cargo test --locked --lib -- tui::shell tui::event_loop
tui::full_transcript tui::file_picker tui::file_index`, a short TMPDIR, the
whole run in one hold of the suite lock, 900 s each. None failed to build;
the clone was clean after (its last line, 0). **16 run, 15 killed, 1
survived: CTRLO-AT-KEYS' equivalent one, run again on purpose.**

| mutant | what it breaks | killed by |
|---|---|---|
| `a-settings-dropped` | the settings menu off the frame's list | the settings case, alone |
| `a-help-dropped` | the catalog off it | the catalog's case and the opens-under case (`/help`) |
| `a-skills-dropped` | the skills menu off it | the skills case and the opens-under case (`$`) |
| `a-model-dropped` | `/model`'s menu off it | the model case |
| `a-sessions-dropped` | the session picker off it | the session picker's case |
| `a-fork-dropped` | `/fork`'s menu off it | the fork case and the opens-under case (the answer) |
| `a-effort-dropped` | `/effort`'s menu off it | the effort case |
| `a-skills-unshown-too` | the skills menu counted when it shows nothing (`is_some`, not fx's `menuVisible`) | the opens-under case's `$zz`, alone |
| `a-reconcile-gone` | no reconcile before the frame: the group A rule reverted whole | every group A case |
| `b-slash-unguarded` | the slash menu moves on C-j/C-k under the screen again | its C-j and C-k cases |
| `b-mcp-args-unguarded` | `/mcp `'s completions, the same | their C-j and C-k cases |
| `b-provider-unguarded` | the `/provider` column, the same | its C-j and C-k cases and the anchor case |
| `b-cj-only` | only C-j handed on (half reverted) | the three C-k cases |
| `b-ck-only` | only C-k handed on | the three C-j cases and the anchor case |
| `anchor-row-check-gone` | the column under its argument on any row (the base's anchor) | the anchor case, alone |
| `at-cj-ck-handed-on-whenever-ineligible` | CTRLO-AT-KEYS' survivor: the `@` list's hand-on keyed on "not eligible" | **SURVIVED**: equivalent (`equivalent-mutants.txt`, with the citations its review asked for) |

`equivalent-mutants.txt` cites `src/tui/shell.rs` at `91c46731`; the copy
committed in `76a5052d` named `a0634606`'s numbers, which the anchor's seven
lines had moved, and is corrected in the receipt commit.

## 6. Sweep (`sweeps.sh`; `sweep-base.md`, `sweep-head.md`, `sweep-compare.txt`)

`sweep.sh` against fx v0.0.16 (sha256 `8e6cda2e...`, checked before and
after each run), the base's xfx (`v0.1.0-cb7c9c1`) and the head's
(`v0.1.0-91c4673`), each banner and sha256 in its sweep's header; 40x120,
`--jobs 2`, `--only 'p5-ctrlo*' 'help*' 'model*' 'resume*'
'p5-effort-fork-*' 'skills*' 'settings*' 'slash*' 'provider*'
'p5-mcp-menu*' 'p5-mcp-verbs' 'fulltranscript*'`: 85 scenarios, 606 pairs,
both sweeps inside one hold of the suite lock (base 1855 s, head 1880 s),
the head's scenarios and ledger for both. The brief's `sessions*` names no
scenario; the session picker's are `resume*`. `settings*` and
`fulltranscript*` are added: the frame's list holds the settings menu, and
the rule is the full transcript's.

- **base 356/606 -> head 382/606: 27 moved to passing, 1 dropped, 1 other
  change** (`sweep-compare.txt`).
- The 27 are this unit's: group A's `held`, the moves and `esc` (effort 6,
  fork 6, model 5, sessions 3, help, skills and the `$` menu's `esc`), and
  group B's `cj` and `ck` (`p5-ctrlo-mcp-args` 2, `p5-ctrlo-slash cj`,
  `provider-ctrlo cj`).
- **The one dropped, `p5-mcp-menu-new list-remote`**, is the remote
  server's status word, timed: row 7 `remote  connecting` on the head's
  xfx where fx's says `retrying`. The scenario's own header says the server
  is `connecting/retrying, never failed`; nothing in this unit reaches the
  MCP runtime or its status. Captured again under the lock, three times
  each, alternating, the head's build and the base's both say `retrying`,
  as fx does (`remote-rerun.txt`): a status sampled at a moment, not a
  change.
- The other change, `settings-collapse-open after-esc`, moved within
  passing (normalized-identical to identical).
- Of the unit's 63 pairs the head passes 39; the 24 it does not are the
  rows each scenario's `open` already differs in (§4) -- the catalog's and
  the slash menu's `Commands 38` / `34`, the provider rows, the skills
  tabs -- the same rows the families' own scenarios leave unresolved on
  both sides (`help open`, `slash slash`, `provider open`, `skills menu`
  in `sweep-head.md`), not this unit's.

## 7. The parity rows and the ledger

- `docs/parity.md` (`1faed29d`, `9ffaef29`, `0ab399ed`): the `/help`,
  `/resume`, `/fork`, `/model`, `/effort`, `/skills` and `/settings` rows say Ctrl-O over
  the menu shows nothing and every key stays its own; the `/provider` and
  `/mcp` rows say C-j and C-k are the composer's under the screen, and both
  name the anchor (§2); the full-screen TUI row's slash menu, which named
  five keys, names C-k and C-j and the screen; the full transcript row says
  both rules and the one window narrower than fx's (§10) -- round 2
  (`13ed6a79`) replaced that sentence with the burst, now fx's (§11).
- The ledger (`a0634606`, `91c46731`): 16 refs past the new lines
  re-pointed by `refs.py` (CTRLO-AT-KEYS', with `full_transcript.rs` among
  its files -- three of its refs moved with the test-only accessor), then 13
  past the anchor's lines: `refs.txt`, at the head against the base, **22
  checked, 0 wrong**.
- No help entry, registry count or `check-no-stubs` row names these keys.

## 8. Gate (`gates.sh`, `gate.txt`)

`gates.sh` (CTRLO-AT-KEYS', unchanged but for its first comment line) in a
`--no-local` clone at `76a5052d`, the whole script in one hold of the suite
lock, TMPDIR a short scratch directory: **20 steps, 0 failed**, the tree
clean before and after.

- fmt, clippy and clippy with `fault-injection`;
- `cargo test`: **5561 passed, 0 failed**; with `fault-injection`: **5598
  passed, 0 failed** (summed over each run's 15 test binaries). That is
  CTRLO-AT-KEYS' 5545 and 5582 (`../ctrlo-at-keys/gate.txt`) and this
  unit's sixteen new cases (the arrows case was changed, not added);
- the identity, stubs, secrets, preview-contract, preflight-cost and
  usage-clock checks, and their self-tests; the fxcap sweep and capture
  self-tests; both release builds;
- smoke, and smoke-tui: 37 scenarios and the oracle, 1168 checks, 0
  failures.

`0ab399ed` (two parity wordings) and the receipt commit come after the
gate's commit. The two steps that read `docs/parity.md`'s rows were run
again after `0ab399ed`, outside the gate: `check-no-stubs`, exit 0, and
`cargo test --test parity`, 26 passed.

## 9. From CTRLO-AT-KEYS' reviews

- **gpt6's transcript-offset assertions.** `FullTranscript::offset`
  (test-only, `src/tui/full_transcript.rs`) reads the screen's first row as
  `FullTranscript::key` measures it; the arrows case runs over a record of
  60 paragraphs on a 40-row screen and asserts, after each of Up, Down,
  PgUp and PgDn, the offset (toward the head, back to the tail) and the
  bar. CTRLO-AT-KEYS' `down-not-the-transcripts` survived that case; at
  this head a transcript that does not scroll would fail it on
  `offsets[0] < tail` (read, not run: that mutant is not in this unit's
  `mutants.py`).
- **Shift-Up/Down.** The `@` list's pin: fx's composer move with
  `extend_selection` (`escape_parser.zig:66-71`, `:99-107`), unbound in the
  screen's map (`input_full_transcript_runtime.zig:167-170`), reaches
  `routeComposerVertical` (`input_completion_runtime.zig:230`), where the
  file query takes it and, not eligible, moves nothing (`:407-410`). The
  case asserts no scroll, no move, no edit.
- **The equivalent-mutant citations** opus asked for (point 17): in this
  folder's `equivalent-mutants.txt`, with CTRLO-AT-KEYS' survivor run again
  at this head (§5).

## 10. Look-ahead, and what is not this unit's

- **The next break on the path.** C-j handed to the composer under the
  `/provider` column put the caret on a second row, and the column's anchor
  was the next break: fixed here (§2). Under the slash menu and `/mcp `'s
  completions nothing is left to hang (none shown).
- **The same burst -- closed in round 2** (§11). Round 1 left, as
  narrower than fx's by a frame, a `C-j` or `C-k` that arrives in the same
  burst as a Ctrl-O over a group A menu, before its frame: it moved the
  menu here, where fx's screen is still up for it and its composer takes it
  (`app_input_runtime.zig:1093-1116`; a new line, or nothing while
  `/skills`', `/model`'s or the session menu is up, `:514`). Round 1 also
  said no capture could hold that window, each key its own read; that is
  withdrawn: one `send-keys` of the two bytes is one burst, on fx and on
  xfx (§11), and round 1's "the same read" is "the same burst".
- **Queue proposals for the dispatcher (read, not run):**
  - INSERT-NEWLINE-MENUS: Alt-Enter or Shift-Enter with `/skills`',
    `/model`'s menu or the session picker up. fx's `insert_newline` adds
    nothing then (`app_input_runtime.zig:514`; Shift-Enter is that
    shortcut, `shortcuts.zig:43-65`); xfx's `act` puts a line break in the
    menu's search (`ModelAction::of(InsertNewline)` is `None`,
    `src/tui/model_menu.rs`, so it reaches `type_character('\n')`). Round
    2 gave the composer's `C-j` fx's rule (`Shell::composer_newline`) and
    left `InsertNewline` as it was: with the screen closed those three
    menus take `C-j` as their move, so only under the screen -- the burst
    -- does `C-j` reach the composer over one, and Alt-Enter's path is
    this proposal's. Since round 2 it is a one-line change,
    `Action::InsertNewline => self.composer_newline()` in `Shell::act`
    (opus' round 2 NIT 7), with its own RED and pairs.
  - MODEL-COLUMN-CJ-CK: with the screen closed, fx's `C-j`/`C-k` move the
    `/model ` column (`hasModelQuery` -> `navigateModelPicker`,
    `input_completion_runtime.zig:411-414`); xfx's `offer_to_column` binds
    neither, so they are a new line and a kill. SETTINGS-CJ-CK, already
    queued, is the same for the settings menu (`:397`).

## 11. Round 2

The reviews of `69491c9a`: gpt6 REJECT, two MUSTs; opus
APPROVE-WITH-FIXES, two SHOULDs and six NITs. The dispatcher's brief: pin
MUST 1's invariant, close MUST 2 here (its call), with a stop rule; opus'
SHOULD 1 and the wordings.

**Where round 2's runs ran** (opus' round 2 review asked; the dispatcher
confirmed both): the gate, the mutants and GREEN at `5d6c4cbb`, the sweep
and the captures on round 2's build of `13646dfa`; `git diff --stat
3369622a 5d6c4cbb -- src` and `git diff --stat 3369622a 13646dfa -- src`
are both empty, so each ran the code head's source.

### 11.1 fx's question and approval terms (gpt6 MUST 1)

fx's frame shuts the screen over the seven menus only with no question
and no approval up (`!question_active and approval == null`,
`app_render_runtime.zig:1206`, fx-src.txt:16). xfx's reconcile has neither
term and gets none: the screen is never up with one. An approval or a
question closes it as it arrives (`Shell::full_transcript_sees`), and
a Ctrl-O while one is up never reaches `Shell::act`: `consume_inputs`'
`ask` and `asking` arms take every key first. (Round 2 named
`Shell::open_full_transcript`'s refusal while `Shell::modal` as that
barrier; it is the rule restated, not reached from Ctrl-O at a question --
opus' round 2 NIT 1, corrected in round 3.) The case
`a_question_over_a_menu_closes_the_full_transcript_and_ctrl_o_opens_none_over_it`
puts the catalog up, the screen over it, then an approval -- and, in a new
shell, a question -- with no tick between: each closes the screen, Ctrl-O
over each opens nothing. It passed at the RED commit, as a pin does. The
reconcile's doc names fx's two terms and those two lines (opus NIT 3).

### 11.2 The burst (gpt6 MUST 2)

**fx.** Its loop takes up to `max_input_reads_per_fact_collection` reads
before one frame (`src/ui/event_loop.zig:90-120`, fx-src.txt:987-1023), so
a `C-j` or `C-k` that comes with a Ctrl-O over a group A menu finds the
screen still up: `composerPickerSurfaceVisible` gives no picker the byte
(`app_input_runtime.zig:1110-1116`) and the composer takes it
(`app_input_runtime.zig:881-893`):

- `C-k` is a kill to the line's end (`app_input_runtime.zig:479-488`);
  `C-j` is `insert_newline` (`app_input_runtime.zig:513-521`), which adds
  nothing while `/skills`' menu, `/model`'s menu or the session menu is up
  (`app_input_runtime.zig:514`).
- An editing byte closes `/fork`'s and `/effort`'s menus
  (`dismissActiveMenusForComposerEdit`, `app_input_runtime.zig:1984-1990`),
  never a catalog menu. `insert_newline` syncs none, so the catalog's and
  the settings menu's marks stay where they were, and a line break is no
  word of the catalog's search (`helpCatalogSpecMatches` splits at `" \t\r\n"`,
  `command_specs.zig:460`): the same list.
- Then the frame shuts the screen over whichever menu is left; with
  `/fork`'s or `/effort`'s gone, the screen stays.

**The copy** (`3369622a`):

- **One check** in `Shell::consume_inputs`, after `offer_to_mcp`: a `C-j`
  or `C-k` with the screen up (`Shell::composer_control`) skips every
  picker (`Shell::offer_to_pickers`, the chain the loop had inline) and
  goes to `act`. It replaces the three per-picker guards and the `@` list's
  inline copy (opus NIT 1); over the seven menus it is the burst, over the
  slash menu, the `/provider` column, `/mcp `'s completions and the `@`
  list it is round 1's rule.
- **The composer's `C-j`**, `Shell::composer_newline`: nothing while
  `/skills`' menu (not a `$` word's), `/model`'s menu or the session picker
  is up; otherwise `type_character('\n')`, which closes `/fork`'s and
  `/effort`'s menus as any edit does, and the catalog's and the settings
  menu's marks put back (`HelpMenu::mark`/`keep`, `SettingsMenu::mark`/
  `keep`). `C-k` is the editor's kill, as before.
- **The stop rule did not fire.** `InsertNewline` (Alt-Enter) is left as
  it was: `C-j` gets fx's rule in a function of its own, so
  INSERT-NEWLINE-MENUS is not needed first (§10). Source outside the tests,
  round 2: 123 lines added and 105 removed (`src/tui/shell.rs` +94/-105,
  most of it the chain moved into `offer_to_pickers`; `help_menu.rs` +15,
  `settings.rs` +14), 62 of the 123 doc comments.
- **One side effect, outside the burst:** the settings menu binds neither
  key (`src/tui/settings.rs`), so with the screen closed its `C-j` reaches
  `composer_newline` too, which now keeps the mark where round 1 reset it.
  fx's menu moves there (`input_completion_runtime.zig:397`); both are
  SETTINGS-CJ-CK's, already queued, and the settings row says so.

**Captures** (`p5-ctrlo-help`, `p5-ctrlo-fork`: `burst-ck`, `burst-cj`).
`key -H 0f 0b` and `key -H 0f 0a` are each one `tmux send-keys` of two
bytes (`capture.sh`'s `key` step, `scripts/fxcap/capture.sh:891-892`).
They landed in one burst on fx: in
`p5-ctrlo-help`, after Down puts the mark on `/clear`, fx's `burst-ck`
keeps it on `/clear`, where its `ck` -- a C-k after the frame -- moves the
menu up; `burst-cj` keeps it too, one row lower under the composer's new
line. In `p5-ctrlo-fork`, fx's `burst-ck` and `burst-cj` show the
transcript's screen: the key closed the menu, so the frame kept the
screen -- after the frame the menu would have moved instead. fx's trace
(`fx/fx-p5-ctrlo-help-40x120-end.files`) has thirteen
`close_full_transcript restore=exact` lines now, one per Ctrl-O over the
catalog. xfx's round 2 build shows the same screens; round 1's moved the
catalog's mark and kept `/fork`'s menu.

### 11.3 One anchor (opus SHOULD 1, NIT 2)

`Shell::column_anchor` is gone; the inline model column calls
`Shell::anchor_at`, and `Shell::file_picker_column` is
`anchor_at(..).saturating_add(1).min(cols)` -- fx hangs the model,
provider, file and slash columns from the one `projectedAnchorColumn`
(`input_presentation.zig:383-386`, `visual_layout.zig:705-713`).
`provider_anchor`'s doc now says it places the argument's first byte as
`anchor_at` places any column. The pairs that show those columns did not
move (11.5): `p5-ctrlo-model`, `provider-ctrlo` (with a Backspace and an
Alt-Enter snap now, NIT 6) and the sweep's `model*`, `provider*`,
`filepicker*` and `p5-input-at*`.

### 11.4 RED and GREEN

- At `69743117` (round 1's code, the cases added), the six run alone:
  **4 failed, 2 passed** (`red-unit.txt`, its third part). Failed: the
  catalog's burst (`0xb moved the mark`), `/model`'s, the session picker's
  and `/skills`' (`/model, 0xa moved the mark`), a `$` word's (`explain $`,
  no line break), `/fork`'s and `/effort`'s. Passed, as pins: the modal
  invariant (11.1) and the event loop's turn (NIT 4,
  `ctrl_o_over_the_catalog_writes_no_plane_change_in_its_turn`: Ctrl-O
  over the catalog, then the loop's tick and frame, writes neither
  `?1049h` nor `?1049l`).
- The settings menu's burst came with the fix, which adds the
  `SettingsMenu::mark` it reads; its RED is the mutant
  `burst-settings-mark-reset` (11.6).
- GREEN, at `5d6c4cbb`, from the gate's own `cargo test`: the unit's 23
  cases, both rounds', pass (`green-unit.txt`, its second part).

### 11.5 Pairs (`pairs.txt`; `fx/`, `xfx/`, `xfx-r1/`, `xfx-base/`)

`p5-ctrlo-help`, `p5-ctrlo-fork` and `provider-ctrlo` captured again on fx
and on round 1's build with their new snaps; all ten on round 2's build
(`v0.1.0-13646df`, `xfx/`). Base, round 1 and round 2, each against fx:

**69 pairs. Identical: base 11, round 1 39, round 2 39. Round 2's other
30: 25 differ only in their scenario's `open` rows, and 5 more.** The 5:
`p5-ctrlo-help tab` and `skills-ctrlo tab` (§4); `p5-ctrlo-help burst-cj`,
`open`'s catalog rows one row lower under the new line; `p5-ctrlo-fork
burst-ck` and `burst-cj`, 12 rows each, the record's timestamps and the
network notice's latency, nothing else -- round 1's 32 there were its
menu over the screen. The new snaps: help's `burst-ck` round 1 11 rows,
round 2 `open`'s 9; help's `burst-cj` 26 -> 9; fork's 32 -> 12; provider's
`bspace` `open`'s 3 on both; `alt-enter` identical on both. No pair round
1 had identical differs in round 2.

### 11.6 Mutants (`mutants.py`, `mutants-r2.txt`)

`mutants.py` (round 2's list) on a `--no-local` clone at `5d6c4cbb`, the
same tests, limit and lock as §5. None failed to build; the clone was clean
after. **19 run, 19 killed.** Round 1's group A mutants run again; round
1's three guard mutants are one now (`b-check-gone`), and CTRLO-AT-KEYS'
equivalent survivor has no line left (`equivalent-mutants.txt`, its round
2 note).

| mutant | what it breaks | killed by |
|---|---|---|
| `a-settings-dropped` ... `a-effort-dropped` (7) | one menu off the frame's list | as §5, and now burst cases too: settings and the catalog by the catalog's burst case, `/model`, the session picker and `/skills` by theirs, `/skills` by a `$` word's as well |
| `a-skills-unshown-too` | the skills menu counted unshown | the opens-under case's `$zz`, alone |
| `a-reconcile-gone` | no reconcile at all | 12 cases, the event loop's turn (NIT 4) among them |
| `b-check-gone` | no check: every picker takes `C-j`/`C-k` under the screen again | 13 cases: every burst case but the modal one, group B's six, the anchor case and CTRLO-AT-KEYS' two for the `@` list -- the copy this check replaced |
| `b-cj-only` / `b-ck-only` | one of the two keys past the pickers | 8 / 9 cases, each the other key's |
| `burst-held-gone` | `C-j` a line break over `/skills`', `/model`'s and the session menu (no `app_input_runtime.zig:514`) | the model-session-skills burst case |
| `burst-held-no-skills`, `-no-model`, `-no-sessions` | one menu off `app_input_runtime.zig:514`'s three | the same case, each at its menu |
| `burst-catalog-mark-reset`, `burst-settings-mark-reset` | the line break resets the mark (round 1's path, `type_character('\n')`) | the catalog's burst case, each at its menu |
| `anchor-row-check-gone` | the base's anchor, now under every column | the anchor case and the inline model column's own (`a_column_over_a_line_the_caret_has_left_hangs_from_the_caret_rows_start`): both columns hang from one function now |

### 11.7 Sweep (`sweeps-r2.sh`; `sweep-r1.md`, `sweep-r2.md`, `sweep-compare-r2.txt`)

`sweeps-r2.sh`: round 1's build (`v0.1.0-91c4673`) and round 2's
(`v0.1.0-13646df`) against fx v0.0.16 (sha256 checked before and after
each run), §6's list and the `@` list's families (`filepicker*`,
`p5-input-at*`), which the one check and the merged anchor reach: 95
scenarios, 665 pairs, both sweeps in one hold of the suite lock (2079 s,
2059 s), round 2's scenarios and ledger for both.

- **Round 1 427/665 -> round 2 427/665** (`sweep-compare-r2.txt`): 3
  moved to passing, 3 dropped, 1 other change.
- Of this unit's: `p5-ctrlo-fork burst-ck` and `burst-cj` moved to
  passing (their timestamps normalized). `p5-ctrlo-help burst-ck` and
  `burst-cj` stay where `p5-ctrlo-help open` is, unresolved on both, for
  the catalog's rows (11.5).
- **The 3 dropped, `model-column-idle` `col-down`, `col-idle` and
  `col-typed`, and the one up, `model-busy during`, are fx's screens
  moving, not xfx's.** Round 2's xfx drew the same rows as round 1's in
  both sweeps; fx -- the same binary -- drew different ones. In
  `model-column-idle` the catalog is down for its first 9000 ms: in round
  1's sweep fx's second read came at 12214 ms and listed the models, in
  round 2's at 7562 ms, still down, so fx said `unable to load models`
  three times while xfx listed them. In `model-busy during` fx's
  `Next turn will use` landed after the snap in round 1's sweep and before
  it in round 2's. Rows and read times are in `sweep-compare-r2.txt`'s
  last lines. The column's rows start in the same cell on both sides --
  round 2's sweep prints both, nine blanks before `unable to load models`
  and before `openai/gpt-6-astra` (`sweep-r2.md:108`), and round 1's
  passed the pair -- so the merged anchor did not move it. (Round 2 said
  "cell 9, as `cells.py` counts", a figure no receipt file held: opus'
  round 2 NIT 8.)
- The other change, `settings-collapse-open after-esc`, moved within
  passing again (normalized-identical to identical), as in §6.
- `filepicker*` and `p5-input-at*`: the same on both builds, 20 of 20 and
  24 of 33 passing; the 9 left are the ones round 1's build leaves too.

### 11.8 Gate (`gate-r2.txt`)

`gates.sh` (round 1's) in a `--no-local` clone at `5d6c4cbb` -- after
every commit that touches `src/`, `docs/parity.md`, the scenarios or the
ledger; the receipt commit after it touches only `.prd/` -- the whole
script in one hold of the suite lock: **20 steps, 0 failed**, the tree
clean before and after. `cargo test` **5567 passed, 0 failed**; with
`fault-injection` **5604 passed, 0 failed** (15 binaries each): round 1's
5561 and 5598 and round 2's six cases. smoke-tui: 37 scenarios and the
oracle, 1168 checks, 0 failures. `check-no-stubs` and `tests/parity.rs`
ran inside it, after `13ed6a79`'s parity rows: `ok`, and 26 passed
(`gate-r2.txt`'s last lines, added in round 3 from the gate's own logs,
opus' round 2 NIT 8).

### 11.9 The rest

- `docs/parity.md` (`13ed6a79`): the full transcript row's last sentence
  is the burst, round 1's "No capture can hold that window" gone; the
  settings row says its `C-j` keeps the mark (SETTINGS-CJ-CK).
- The ledger (`13646dfa`): 13 refs past round 2's lines; `refs.txt` at the
  head against the base, **22 checked, 0 wrong**.
- `fxcites.txt`, run again after this README's last edit: every fx
  citation in the README, the parity rows this unit changed, the source
  lines it added and its scenarios' headers, against `fx_src.sh`'s ranges
  (its fifth part is round 2's: `event_loop.zig`, `command_specs.zig`,
  `input_presentation.zig`; its sixth round 3's, what a kill syncs).
- `equivalent-mutants.txt`: its survivor's line, the `@` list's inline
  copy, is gone at this head (11.2), so round 2 runs no mutant there.
- Review points not taken: none. opus SHOULD 2 (queue the burst) is
  superseded by the dispatcher's call to close it here.

## 12. Round 3

The reviews of `ec0dfe00`: gpt6 no MUST, two SHOULDs; opus
APPROVE-WITH-FIXES, one SHOULD and eight NITs. **No behaviour changed**:
past `ec0dfe00` the source's non-test lines that changed are doc comments
only, and the burst C-k case the round added found no gap.

| commit | what |
|---|---|
| `c2452375` | four doc comments that said what the code does not do (12.1) |
| `bcb4e211` | the round's cases and three burst-edit scenarios -- **the code head** |
| `ee3a7417` | the ledger's 12 refs past the new lines re-pointed (`refs.txt`) |
| `c5c177a3` | the round's runners (`mutants.py`, `fx_src.sh` part 6, `pairs.py`, `fxcites.py`) and the burst-edit captures -- **round 3's gate commit** and the mutants' clone |
| (receipt) | this section, `gate-r3.txt`, `mutants-r3.txt`, `gate-r2.txt`'s parity lines, `green-unit.txt`, `refs.txt`, `fxcites.txt`; §10, §11.1, §11.7 and §11.8 corrected |

### 12.1 The doc comments (opus SHOULD 1, gpt6 SHOULD 2, opus NIT 1)

- `Shell::composer_newline`: with the screen closed `C-j` reaches it over
  three menus that bind none -- the settings menu (so its `keep` is for
  that too, where fx's menu moves: SETTINGS-CJ-CK), the inline model
  column (MODEL-COLUMN-CJ-CK) and a `$` menu that shows nothing -- and the
  rest take `C-j` themselves; round 2 said only the burst reached it.
- `Shell::act`'s `C-j` comment: it can add nothing (`app_input_runtime.zig:514`'s menus) and
  puts the catalog's and the settings menu's marks back.
- `Shell::composer_control`: `/mcp`'s menu is asked first and takes every
  key while the screen covers it (`Shell::offer_to_mcp`, opus-C); what
  restated the reconcile's doc is gone.
- `Shell::reconcile_full_transcript`: the barrier against a Ctrl-O at a
  question is `consume_inputs`' `ask`/`asking` arms; the refusal in
  `open_full_transcript` is the rule restated (§11.1 corrected).

### 12.2 The cases (`bcb4e211`)

Each passes on `ec0dfe00`'s code, run there: a `--no-local` clone at
`ec0dfe00` with `bcb4e211`'s cases applied, the six by name under the lock,
**6 passed** (`green-unit.txt`, its last part). So none has a RED: each
pins what the head already did. GREEN at the gate commit is the third
part.

- **`keep` over a scrolled window** (gpt6 SHOULD 1, opus NIT 2):
  `c_j_in_ctrl_os_burst_inside_a_search_word_keeps_a_scrolled_catalogs_mark_and_window`
  and `..._settings_mark_and_window`, at 16x80. `e`, Home, `s` types the
  search `se` with the caret between its letters; Down until the window
  has scrolled, then Up until the mark is the window's first row (a mark
  and a window a reset would not give back). Ctrl-O and C-j in one burst
  make it `s` and `e`, a different search (`command_specs.zig:460`): both
  indices are kept and the band's first item row is the marked one -- for
  the catalog, the command at the kept index of what `s` and `e` list.
- **C-k in the burst with text after the caret** (opus NIT 6):
  `c_k_in_ctrl_os_burst_with_text_after_the_caret_kills_it_and_each_menus_search_follows`,
  over every menu whose list a kill changes: the catalog, the settings
  menu, `/model`'s menu, the session picker, `/skills`' and a `$` word's.
  A search typed, the caret taken back (Home, or Left inside the `$`
  word), the mark moved, then Ctrl-O and C-k: the kill, and each menu
  synced as fx's kill syncs it (`syncCatalogMenus`,
  `app_input_runtime.zig:2478-2544`, from `:485`) -- the catalog and the
  settings menu back to the top (`resetForQuery`,
  `command_specs.zig:209-212`, `settings_catalog.zig:208-211`), `/model`'s
  menu and the session picker too (`model_cache_runtime.zig:209-215`,
  `app_session_runtime.zig:496-503`), the skills menu's mark on its index,
  held to what is listed (`skill_runtime.zig:2603-2623`). In xfx the kill
  goes `act` -> `amended` -> `reconcile_picker`, each menu's `set_query`
  or `sync`, the path a Backspace takes. fx's durable session picker
  lists again for the query (`searchSessionPicker`,
  `app_session_runtime.zig:2271-2283`) where xfx's filters what it holds;
  its mark is the first row either way, and that is the session picker's
  own unit's, not this one's.
- **`/effort` alone** (opus NIT 2): the fork and effort burst case is two
  cases, so `edit-leaves-effort-up` is seen failing in `/effort`'s.
- **The `@` list's anchor** (opus NIT 4):
  `the_at_lists_column_hangs_from_the_carets_row_once_its_word_has_wrapped`,
  at 40x24: `ab @src` hangs the list under the `@`; 24 more letters wrap
  the word, the `@` on the row above the caret and not at its start, and
  the list hangs from the start of the caret's row.

### 12.3 The burst edits against fx (`pairs.txt`; `p5-ctrlo-burst-*`)

Three scenarios, each `typed`, then `key -H 0f 0a` (`burst-cj`) and `key -H
0f 0b` (`burst-ck`), on the fx oracle (sha256 `8e6cda2e...`, printed
unchanged after the runs) and on round 2's build (`v0.1.0-13646df`, the
head's code but for comments and cases). **9 pairs: `/model`'s 3
identical; the settings menu's 3 differ in one row, Sound level's `max`,
which fx offers and xfx's settings do not, already in `typed`; the
catalog's 3 in the rows fx's catalog has and xfx's has not (`/login`,
`/logout`, `Commands N`, `/version`'s name), `typed` 13 rows, the bursts 9
each.** `cells.py` compares attributes too, so each marked row is the
same on both sides: the catalog's mark on `/new` (`typed`), on `/status`
after C-j -- the third row of what `s` and `e` list, its index kept -- and
on `/help` after C-k, the first; the settings menu's on Model, Status line
workspace, Status line context; `/model`'s stays put on C-j (`app_input_runtime.zig:514`) and
goes to the first row on C-k. So fx's kill and xfx's sync the same, and
both took the two bytes in one burst: after the frame, C-j and C-k would
have moved the catalog's mark instead.

All pairs: **78; identical: base 11, round 1 39, head 42; the head's other
36: 27 differ only in their scenario's first snap's rows, 9 more** --
round 2's five (§11.5) and the catalog's and settings menu's bursts
above.

No sweep ran this round: the source changed in comments and cases only.
The three scenarios fall under the sweep's `p5-ctrlo*`, so the next sweep
has nine more pairs: `/model`'s three pass, and the catalog's and the
settings menu's six fall where `help open` and the settings scenarios'
Sound level row already are -- not a regression.

### 12.4 Mutants (`mutants.py`, `mutants-r3.txt`)

`mutants.py` (round 3's list) on a `--no-local` clone at `c5c177a3`, the
same tests, limit and lock as §5 (one normal slot). None failed to build;
the clone was clean after. **24 run, 24 killed**: round 2's 19 again and
the five opus' round 2 NIT 2 asked for.

| mutant | what it breaks | killed by |
|---|---|---|
| `burst-held-mention-too` | a `$` menu held as `/skills`' is (`.is_some()` for the mention test) | the `$` burst case, alone |
| `sees-close-gone` | an approval or a question leaves the screen up | the modal case (11.1) and the older `a_question_closes_the_full_transcript_and_a_large_change_takes_the_plane_over` |
| `keep-help-selected-only`, `keep-settings-selected-only` | `keep` puts the mark back and leaves the window reset | each menu's scrolled-window case, alone -- before round 3 none scrolled, and both survived |
| `edit-leaves-effort-up` | `/effort`'s menu, and only it, stays up through an edit | `/effort`'s own burst case and two older edit cases |

The rest moved with the new cases: `b-check-gone` 13 -> 16,
`b-cj-only` 8 -> 10, `b-ck-only` 9 -> 11, `a-reconcile-gone` 12 -> 15, the
two `burst-*-mark-reset` 1 -> 2 each (the scrolled-window cases), and
`anchor-row-check-gone` now the `@` list's case too
(`the_at_lists_column_hangs_from_the_carets_row_once_its_word_has_wrapped`),
beside the provider and model columns' (opus' round 2 NIT 4).

### 12.5 Gate (`gate-r3.txt`)

`gates.sh` in a `--no-local` clone at `c5c177a3` -- after every commit
that touches `src/`, the scenarios or the ledger; the receipt commit after
it touches only `.prd/` -- the whole script in one hold of one normal slot
of the suite lock: **20 steps, 0 failed**, the tree clean before and after.
`cargo test` **5572 passed, 0 failed**; with `fault-injection` **5609
passed, 0 failed** (15 binaries each): round 2's 5567 and 5604 and
round 3's five cases (four new, and the fork and effort case made two).
smoke-tui: 37 scenarios and the oracle, 1168 checks, 0 failures.
`tests/parity.rs`, 26 passed, and `check-no-stubs`, `ok`, inside it
(`gate-r3.txt`'s last lines). GREEN for the unit's 28 cases is the gate's
own run (`green-unit.txt`, its third part).

### 12.6 The timing pairs in the ledger (opus NIT 5)

- **`model-column-idle`: already there.** The ledger's `catalog-window`
  cause (`kind = "nondeterminism"`, owner FLAKE-MODELSDOWN, registry #19,
  `scripts/fxcap/parity-ledger.toml`) lists the scenario and its `col-*`
  snaps, and round 2's sweep printed the three pairs under it
  (`sweep-r2.md:213`). A cause approves nothing, so the verdict stays
  `unresolved-defect` with its owner named; nothing to add.
- **`model-busy during`: no cause.** The ledger's note above its timing
  causes (`parity-ledger.toml:1028-1035`) keeps a cause off a moment in
  time -- one for where a stream stood covered every token count of every
  scenario -- and has the scenario wait on an event instead. `model-busy`
  holds its stream already (`held`); what moved is whether fx had acted on
  `/model grok`, Escape and Enter when `snap during` came 0.8 s later: its
  composer still holds `/model grok` in round 1's sweep and is empty under
  `Next turn will use` in round 2's (`sweep-compare-r2.txt`). A cause on
  `during` would cover the whole snap. **What would fix it:** `until shown
  Next turn will use` before `snap during` (capture.sh's `until`) in
  `scripts/fxcap/scenarios/model-busy.cap` and `model-busy-known.cap`, a
  change for those scenarios' owner -- a queue proposal, MODEL-BUSY-UNTIL.

## 13. Open questions

None that needs a user decision.
