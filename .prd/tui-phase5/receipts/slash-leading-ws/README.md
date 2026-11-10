# SLASH-LEADING-WS receipt: the slash menu past leading spaces, tabs and line breaks

PAGE-KEYS-SETTINGS-COLUMN's opus review found the gap: fx v0.0.16 opens its
slash menu for a draft whose first character past leading blanks is `/`, and
xfx's `Trigger::of` asked for `/` first and no blank anywhere. That review's
receipt recorded it in `docs/parity.md` on its own branch
(`fix/p5-page-keys-settings-column`, `8e809d40`: "Uncaptured; owner
SLASH-LEADING-WS ... not fixed there"); that branch is not in this unit's
base, so the sentence is not here to replace. **When both merge, drop the
page-keys sentence**: this unit's sentence (below) says what holds now.

The base is train 13's head `afd891af`. fx is the v0.0.16 release (pin
`481104fd`), sha256 `8e6cda2e…`, run with `FX_AUTO_UPGRADE=0`; `capture.sh`
checked its bytes after every run.

| commit | what |
|---|---|
| `f6383779` | RED: the three scenarios, `fx/`, `red/`, the base's probe, `fx_src.sh` and `fx-src.txt`, `pairs.sh`, `red.txt`; no `src/` change |
| `bca985c9` | **the code**: `src/tui/picker.rs`, `src/tui/shell.rs`, `src/tui/entity.rs`, their tests, `docs/parity.md` |
| `6ee50b76` | the 14 ledger refs `bca985c9` moved, re-pointed (`repoint.py`); numbers only |
| `057e3d1e` | the header's `type to filter` read off the slash word (the probe found it), and its tests |
| `55c4811c` | the six ledger refs `057e3d1e` moved; numbers only |
| `136a7059` | round-1 GREEN at `55c4811c`, `mutants.py`; no `src/` change |
| `1ae99e6d` | **the code head**: two comments the broad kind made false; comments only |
| the commits after | the `/he` scenario and its probes, `xfx/` and `green.txt` at `1ae99e6d`, fx's lines for what is not copied, then the mutants, the sweeps, the gate and this file; no `src/` change |

The xfx captures in `xfx/` and `probe/head-*` come from the binary built at
the code head `1ae99e6d` (row 1 `x𝒇x v0.1.0-1ae99e6 [debug]`, sha256
`20d930fe6f9c…`, the one the head sweep names in `sweep-head.md`); those in
`red/` and `probe/base-*` from the base (`v0.1.0-afd891a`, sha256
`782cf10fe699…`). No `gwlog` step, so no request body is in the receipt.

## What fx v0.0.16 does (`fx-src.txt`, made by `fx_src.sh`; every range checked)

- **F1, the kind.** `inlinePickerTriggerKind` (`picker_state.zig:287-296`)
  asks the `/model` and `/provider` lines, an `@` word, a `$word`, a slash
  word at the caret (`findInlineSlashQuery`), and then: the draft's first byte
  past `" \t\r\n"` is `/` -> `.slash`. Nothing about what follows: `/help me`
  is `.slash`.
- **F2, the ghost.** `findInlineSlashQuery` (`:613-624`) is the last word,
  `/` and one byte more, with non-blank text before it -- the dim rest of a
  name after the caret (`visibleInlineSlashCompletion`,
  `input_completion_runtime.zig:653-661`), not a menu. xfx already had it
  (`inline_completion::slash_word`); `  /he` and `\n/he` are not it (nothing
  but blanks before the word), so they fall to F1's last line.
- **F3, the popup's query.** `slashCompletionPrefix`
  (`command_specs.zig:568-576`) is the draft past `" \t\r\n"`; at the first
  blank it stops for a name that takes nothing and goes on for one it does not
  know or one with arguments. The popup asks for the `.slash` kind and that
  prefix (`slashCompletionPickerPrefix`, `input_presentation.zig:269-276`;
  `slashCompletionQueryActive`, `input_completion_runtime.zig:97-109`).
- **F4.** The `/model` and `/provider` lines skip spaces and tabs only
  (`leadingWhitespaceLen`, `picker_state.zig:584-588`).
- **F5, the latch.** Escape dismisses the visible kind
  (`dismissVisibleInlinePicker`), and the dismissal lasts while the kind is
  the same (`suppressionAfterEdit`, `picker_state.zig:520-532`); every edit
  zeroes the mark (`reconcileInlinePickerAfterEdit`, `:228-236`).
- **F6, Tab.** A command row replaces the whole composer, blanks included
  (`textReplacementState().replace`, `app_input_runtime.zig:1342-1349`;
  `composer_text_replacement.zig:63-94`); a skill row binds its token over
  `leading_ws..leading_ws + prefix.len` and leaves the blanks
  (`bindSelectedSlashSkill`, `app_input_runtime.zig:2451-2476`).
- **F7, Enter.** A skill row binds the same way
  (`submitSlashPickerSelection`, `:2366-2396`); a command line is
  left-trimmed, resolved to the marked row, run, and recorded as resolved
  (`input_submit_runtime.zig:719-729`, `:1009-1016`, `:1059-1064`,
  `:1098-1101`).
- **F8, the rows.** The popup's budget takes the composer's rows past its
  first off the screen (`inlinePickerRowBudgetCapped`,
  `picker_presentation.zig:793-803`, handed `geometry.input_extra` by
  `surface_frame.zig:543-551`).
- **F9, not copied here.** Past a blank, F3's prefix is fx's argument
  completion (`argCompletionAnchor`, `command_specs.zig:636-644`, `/mcp`'s
  among it) and lists the skills an unknown name's words find
  (`mixedSlashCompletionCount`, `picker_presentation.zig:1170-1175`); the
  `/mcp ` list is the slash popup there, so one `.slash` dismissal closes both
  (`visibleInlinePickerKind`, `input_completion_runtime.zig:142-169`).

fx's own screens agree (`fx/`): the menu for `  /help`, `\n/help` and a pasted
`\t/help` (`spaces`, `newline`, `tab`); Escape over `  /hel` then `p` stays
closed (`spaces-esc-grow`), and so does `/help`, Escape, Space, Backspace
(`esc-space`, `esc-space-back`); Tab on `\n/help` leaves `/help` on one row
(`newline-tab`); `say /he` shows the ghost, no menu (`mid`); `  /version`,
Escape, Enter runs it and Up recalls `/version` (`vers-run`, `vers-recall`);
at 18x80 the menu is eight rows under a one-row composer, seven under two,
five under four (`short-18x80-one`, `-two`, `-four`); a skill row's Tab and
Enter keep the blanks before the token (`skill-40x120-dem-tab`,
`-dem-enter`). The brief's own drafts, `  /he`, `\n/he` and `say /he`, are in
`probe/fx-p5-slash-leading-ws-he-*`: fx lists `/shell` for `he`, which xfx has
not got until C8 lands (the ledger's cause for fx's `/shell` rows,
`scripts/fxcap/parity-ledger.toml:606-613`), so the paired scenario asks for
`/help`.

## What xfx did at the base

`Trigger::of` was `text.starts_with('/') && !text.contains(char::is_whitespace)`
(`src/tui/picker.rs:165-167` at `afd891af`): the kind and the query in one,
both ending at any blank. So no menu for any of the drafts above (`red/`) and
no skill row to bind from one; a dismissal forgotten at the space after
`/help` (`esc-space-back` reopened it); a command recorded with its blanks
(`vers-recall`); and a menu whose height assumed a one-row composer (its
`layout` doc said so), which only a line break before the word breaks.

## The change (`bca985c9`, `057e3d1e`, `1ae99e6d`)

- `picker.rs`: `Trigger::of` is F1's last line (the blanks `" \t\r\n"`,
  `BLANKS`); `query_of` is F3 restricted to what xfx has: the trimmed word
  while it is the rest of the draft. `Picker` is keyed by the whole draft (a
  blank edited is an edit, F5) and keeps where its word starts; the header and
  the skills' re-read use the word. `row_budget`, `height` and `rows` take
  `input_extra` (F8).
- `shell.rs`: the band asks the menu's height per composer height it tries,
  and paints it for the one solved; a skill row binds from the word's start
  (F6/F7); a `/provider ` line forgets a slash dismissal (its kind is
  `.provider`, F1), a `/mcp ` line keeps it (its kind is `.slash`); the skills
  landing re-reads the menu only for a `query_of` draft; a command is recorded
  in the history left-trimmed (F7). `EntitySnapshot::earlier` moves its runs.
- `crate::interactive::classify` is unchanged: the TUI's `router::classify`
  already left-trims (`src/tui/router.rs`, its doc's `input_submit_runtime`
  line), so `\n/help` was already run as a command.

## RED (`red.txt`)

11 of 15 pairs differ at the base; the four the same are the ones this unit
does not change: no menu on either side (`esc-space`, `spaces-esc-grow`,
`vers-run`), or no blank before the word (`short-18x80-one`).
The five shell cases fail on the base (put onto its `shell.rs` in a throwaway
clone): no menu for `  /he`, the menu back after a space, `  /vers` not
completed, no room taken for a taller composer, the token not bound. The two
picker cases call the new API and do not compile there.

## GREEN (`green.txt`)

15 of 15 pairs identical at `1ae99e6d` (`pairs.sh xfx`; the version row of
`vers-run` and `vers-recall` read as the ledger's `product-version` rule
reads it). 34 tests pass. The probes (`probe.txt`): at the head the `/he`
drafts differ from fx only by fx's `/shell` row and the name column it
widens, and `four-all` only by the registry's size (38 against 34) -- its
range `1–3`, its `type to filter` and its three rows are fx's. That probe is
what found `057e3d1e`'s bug: at `6ee50b76` the header compared the whole draft
to `/` and dropped `type to filter` under `\n\n\n/`.

## Mutants (`mutants.py`, `mutants.txt`)

13 of 13 killed at `1ae99e6d`, on a throwaway `git clone --no-local` (the
clone's source restored after each, and clean at the end), each against
`tui::picker::` and the five shell cases. Each takes out one rule: the kind
skipping blanks (M1), the kind going on past a blank (M2, the base's rule
back), the query skipping blanks (M3), only fx's four blanks (M4, Unicode
`trim_start` instead), the commands matched against the word (M5), the menu
keyed by the draft (M6), the budget's `input_extra` (M7), the band asking
(M8) and painting (M9) for the taller composer, the token leaving the blanks
(M10), `/provider ` forgetting a slash dismissal (M11), the history's
left-trim (M12), the header reading the word (M13). `mutants.txt` names the
cases that caught each.

## Sweep (`sweep-base.md`, `sweep-head.md`, `sweep-compare.txt`, `cmp_sweeps.py`)

`cmp_sweeps.py` is APPROVAL-VERTICAL-SPELLINGS' copy, unchanged.

`scripts/fxcap/sweep.sh --only 'slash*' --only 'p5-slash*' --only
'composer*' --only 'p5-page-keys*'` at 40x120 with this branch's scenarios
and ledger, once with the base binary and once with the head's, in one
suite-lock slot. No `p5-page-keys*` scenario is in this tree (that unit's
branch is not in the base). Eight scenarios, 56 pairs:

- **base 32 / 56, head 44 / 56; drops 0**; uncaused rows 15 pairs at the
  base, 0 at the head. Twelve of this unit's pairs moved to
  normalized-identical; three more stay unresolved with their uncaused rows
  gone, 4 to 0 each (`sweep-compare.txt`). Nothing outside this unit's
  scenarios changed.
- The twelve not passing at the head all carry a ledger cause:
  `p5-slash-leading-ws-he`'s `spaces` and `newline` (`C8-shell`: fx's
  `/shell` row), `-short`'s `four-all` (`menu-count`: the registry's size),
  and nine in `slash`, `slash-filter` and `composer-keys` that are the same at
  the base (`C7-help-entries`, `menu-count`, `menu-shift`).

## Gate (`gate.txt`)

At `b1e896a0` (the code head's src), in one suite-lock slot: `cargo fmt
--check`, both clippys with `-D warnings`, `cargo test --locked
--no-fail-fast` (5693 passed, 0 failed, 7 ignored; the lib 4769), the same
with `--features fault-injection` (5730 passed, 0 failed, 7 ignored; the lib
4792), `check-xfx-identity`, `check-no-stubs`, `check-no-secrets` and
`check-no-local-paths` -- every step exit 0, nothing re-run.

## References (`repoint.py`, `refs.txt`)

`repoint.py` re-pointed the ledger refs the code moved (14 at `bca985c9`, six
at `057e3d1e`), each to the one line its text stands on now, numbers only.
`refs.txt`: train 13's `refs_content.py` at `1ae99e6d` -- the ledger 116 SAME,
37 SAME-ROW, 1 SAME-LINES, 1 WRONG; `docs/parity.md` 25 SAME, 0 WRONG -- the
same counts as at the base. The one WRONG (ledger:28, into
`.prd/07-fx-parity-program.md`) is the base's and not this unit's.

## `docs/parity.md`

In the full-screen TUI row, after "its argument completion are absent.": the
word may follow blanks, as fx's does (F1, F3); Tab, a skill row, the history
and the rows as above (F6-F8, `fx-p5-slash-leading-ws-*`); the latch across a
space after the word (F5); and the two things not copied, said there (F9):
fx's query going on past a blank, and fx's one latch over `/mcp `'s list,
which keeps its own Escape here.

## Not copied, and why (not this unit's)

- **Past a blank after the word** (F9): fx's argument completion and its
  skills for an unknown name's words. A new unit if wanted (SLASH-ARG-SKILLS).
- **`/mcp `'s list under the slash latch** (F9): xfx's `mcp_args` keeps
  `mcp_args_dismissed`; the same at the base.
- **`router::classify` trims every Unicode blank** (Rust's `trim`), fx's
  submit `" \t\r\n"` only: a draft beginning with a no-break space and `/` is
  a command here and a prompt in fx. The same at the base.
