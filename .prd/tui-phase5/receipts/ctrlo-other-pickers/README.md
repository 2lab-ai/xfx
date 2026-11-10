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

| commit | what |
|---|---|
| `67c8d0d1` | RED: one case per picker, the menus that open under the screen, the `@` list's arrows with the transcript's offset, Shift-Up/Down; the ten scenarios (`red-unit.txt`) |
| `bab45335` | the fix: `Shell::reconcile_full_transcript` (group A) and the three guards (group B) |
| `1faed29d` | `docs/parity.md`: each picker's row and the full transcript's (§7) |
| `a0634606` | the ledger's 16 refs re-pointed (`refs.txt`) |
| `9545abe4` | RED: the `/provider` column's anchor off the caret's row (`red-unit.txt`, its second part) -- `provider-ctrlo cj` showed it |
| `9ffaef29` | the anchor fix, and the two parity rows that name it -- **the code head** |
| `91c46731` | the ledger's refs past the anchor's lines re-pointed -- **the head's build** for the captures, the sweep and the mutants' clone |
| `76a5052d` | this folder's runners, fx quotes, captures, unit runs and refs -- **the gate's commit** |
| `0ab399ed` | two parity wordings (`/settings`' pointer, `/provider`'s anchor sentence); `check-no-stubs` and `tests/parity.rs` run again after it (§8) |
| (receipt) | this README, `gate.txt`, `fxcites.txt`, `mutants.txt`, the sweep's files (`sweep-base.md`, `sweep-head.md`, `sweep-compare.txt`, `remote-rerun.txt`), `equivalent-mutants.txt` corrected (§5) |

Every fx line cited here is printed in `fx-src.txt` (`fx_src.sh`, at
`481104fd`); "fx-src.txt:N" is its line. `fxcites.py` (CTRLO-AT-KEYS', with
this unit's ten scenarios as its fourth input) checks every fx citation in
this README, in the parity rows' changed text, in the lines this unit added
under `src/` and in the scenarios' headers against `fx_src.sh`'s ranges
(`fxcites.txt`: 159 citations, 0 failed; its self-test, 10 cases, 0 wrong).

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
  refusal in `open_full_transcript`, which fx does not have and which would
  miss a menu opened under the screen.
- **Group B: `Shell::composer_control`** -- `C-j` or `C-k` with the screen
  up -- returned `false` early in `offer_to_picker`, `offer_to_mcp_args` and
  `offer_to_provider_picker`, so the key goes on to `act`: `ControlJ` is
  `type_character('\n')`, `ControlK` the editor's kill, the path
  CTRLO-AT-KEYS gave the `@` list.
- **The anchor** (`9ffaef29`): `Shell::anchor_at`, which hangs the
  `/provider` column and `/mcp `'s completions, now copies
  `projectedAnchorColumn` (`visual_layout.zig:705-713`): under the
  argument while it starts on the caret's row, from the start of the
  caret's row otherwise -- the `@` list's column already did
  (`Shell::file_picker_column`). `provider-ctrlo cj` showed it: after C-j
  the column's `no matching providers` was ten cells right of fx's.

Source outside the tests: 60 lines added and 3 removed in
`src/tui/shell.rs`, about half of them doc comments;
`src/tui/full_transcript.rs` gains only a test-only accessor (§9).

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

## 4. Pairs (`pairs.py`, `pairs.txt`; `fx/`, `xfx/`, `xfx-base/`)

The ten scenarios, each step a key between a Ctrl-O and its return (group
A's `held` is Ctrl-O alone, then a second Ctrl-O, a no-op in fx and the
head, the base's close), captured with `scripts/fxcap/capture.sh` at 40x120:
fx v0.0.16 (its sha256 printed unchanged after the runs), the base's debug
build (`v0.1.0-cb7c9c1`) and the head's (`v0.1.0-91c4673`), each banner in
`pairs.txt`'s first lines. `skills-ctrlo*` and `provider-ctrlo` are named
for their families so the ledger rows those share apply in the sweep.

**63 pairs. Base: 11 identical. Head: 38 identical, 23 differing only in
the rows their scenario's `open` snap differs in, 2 in the product's name.**

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
  both rules and the one window narrower than fx's (§10).
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
  this head a transcript that does not scroll fails it.
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
- **Narrower than fx's, by a frame** (in the full transcript's parity row):
  a `C-j` or `C-k` that arrives in the same read as a Ctrl-O over a group A
  menu, before its frame, moves the menu here; fx's screen is still up for
  it and its composer takes it (`app_input_runtime.zig:1093-1116`; a new
  line, or nothing while `/skills`', `/model`'s or the session menu is up,
  `:514`). No capture can hold that window: each key is its own read.
- **Queue proposals for the dispatcher (read, not run):**
  - INSERT-NEWLINE-MENUS: Alt-Enter or Shift-Enter with `/skills`',
    `/model`'s menu or the session picker up. fx's `insert_newline` adds
    nothing then (`app_input_runtime.zig:514`; Shift-Enter is that
    shortcut, `shortcuts.zig:43-65`); xfx's `act` puts a line break in the
    menu's search (`ModelAction::of(InsertNewline)` is `None`,
    `src/tui/model_menu.rs`, so it reaches `type_character('\n')`).
  - MODEL-COLUMN-CJ-CK: with the screen closed, fx's `C-j`/`C-k` move the
    `/model ` column (`hasModelQuery` -> `navigateModelPicker`,
    `input_completion_runtime.zig:411-414`); xfx's `offer_to_column` binds
    neither, so they are a new line and a kill. SETTINGS-CJ-CK, already
    queued, is the same for the settings menu (`:397`).

## 11. Open questions

None that needs a user decision.
