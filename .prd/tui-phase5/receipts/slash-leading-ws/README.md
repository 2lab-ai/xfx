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
| `1ae99e6d` | round 1's code head: two comments the broad kind made false; comments only |
| `b1e896a0`, `1b20e3bb` | round 1's receipt: the `/he` scenario and its probes, `xfx/` at `1ae99e6d`, F9, the mutants, the sweeps, the gate, this file; no `src/` change |
| `7eb2eac2` | round 2 (both reviews of `1b20e3bb` approve, no MUST): the `/mcp ` latch, the ghost and the mark pinned by shell cases, the `classify` difference by a router case; the parity row's cites and two differences; `picker::BLANKS` shared, the dead `Trigger::of` in `Picker::over` gone, a `debug_assert!` in `EntitySnapshot::earlier`, four comments |
| `9af11724` | **the code head**: the 11 ledger refs `7eb2eac2` moved; numbers only. The gate ran here |
| the commits after | round 2's receipt: `xfx/` and `green.txt` at `9af11724`, `red.txt`'s round-2 part, `fx_src.sh` F4/F5, `repoints.txt`, the mutants, the gate, this file; `.prd/` only |

The xfx captures in `xfx/` and `probe/head-*` come from the binary built at
the code head `9af11724` (row 1 `x𝒇x v0.1.0-9af1172 [debug]`, sha256
`98f135cf1b53…`); the head sweep's (`sweep-head.md`) from round 1's
`1ae99e6d` (`20d930fe6f9c…`); those in
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
  (`leadingWhitespaceLen`, `picker_state.zig:584-588`). A line that begins
  `/provider `, `/login ` or `/setup ` (`provider_picker_prefixes`,
  `:54-61`; `providerPickerPrefixLen`, `:645-651`) is the `.provider` kind
  whatever follows: at the provider stage `rawProviderPickerQuery` returns a
  query for any argument (`:336-366`, `if (stage == .provider) return`), and
  the kind asks it before the slash fallback (F1). So a slash dismissal is
  forgotten at `/provider x` -- and at `/login x`, which xfx, having no
  `/login`, does not copy (below).
- **F5, the latch.** Escape dismisses the visible kind
  (`dismissVisibleInlinePicker`), and the dismissal lasts while the kind is
  the same (`suppressionAfterEdit`, `picker_state.zig:520-532`); every edit
  zeroes the mark (`reconcileInlinePickerAfterEdit`, `:228-236`). The ghost
  asks for the same kind and an unsuppressed query (`activeInlineSlashQuery`,
  `:274-277`), so a dismissal made over `/he` also hides the ghost of a
  later slash word in that line (`/he x /ve`) while the line still begins
  with `/`.
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

## The change (`bca985c9`, `057e3d1e`, `1ae99e6d`, `7eb2eac2`)

- `picker.rs`: `Trigger::of` is F1's last line (the blanks `" \t\r\n"`,
  `BLANKS`); `query_of` is F3 restricted to what xfx has: the trimmed word
  while it is the rest of the draft. `Picker` is keyed by the whole draft (a
  blank edited is an edit, F5) and keeps where its word starts; the header and
  the skills' re-read use the word. `row_budget`, `height` and `rows` take
  `input_extra` (F8).
- `shell.rs`: the band asks the menu's height per composer height it tries,
  and paints it for the one solved; a skill row binds from the word's start
  (F6/F7); a `/provider ` line forgets a slash dismissal (its kind is
  `.provider`, F4), a `/mcp ` line keeps it (its kind is `.slash`, F1); the skills
  landing re-reads the menu only for a `query_of` draft; a command is recorded
  in the history left-trimmed (F7). `EntitySnapshot::earlier` moves its runs.
- `crate::interactive::classify` is unchanged: the TUI's `router::classify`
  already left-trims (`src/tui/router.rs`, its doc's `input_submit_runtime`
  line), so `\n/help` was already run as a command.
- Round 2 (`7eb2eac2`) changes no behaviour. It pins three rules with shell
  cases -- a dismissal over `/mc` lasting across `/mcp x`, a dismissed slash
  line hiding the ghost of a later slash word (F5), a blank deleted before the
  word putting the mark back on the first row while a caret move does not
  (F5) -- and the `classify` difference (below) with a router case. The history's
  trim uses `picker::BLANKS`; `Picker::over` drops a `Trigger::of` that
  `query_of` already implies; `EntitySnapshot::earlier` asserts
  `start <= end` (`new` takes the span as given); four comments say what
  holds now.

## RED (`red.txt`)

11 of 15 pairs differ at the base; the four the same are the ones this unit
does not change: no menu on either side (`esc-space`, `spaces-esc-grow`,
`vers-run`), or no blank before the word (`short-18x80-one`).
The five shell cases fail on the base (put onto its `shell.rs` in a throwaway
clone): no menu for `  /he`, the menu back after a space, `  /vers` not
completed, no room taken for a taller composer, the token not bound. The two
picker cases call the new API and do not compile there. Round 2's three shell
cases fail there too (`red.txt`'s round-2 part): the menu back at `/mcp`, the
ghost shown under the dismissal, and no menu at all for `  /e`. Its router
case records a difference the base already had, so it is not a RED.

## GREEN (`green.txt`)

15 of 15 pairs identical at `9af11724` (`pairs.sh xfx`; the version row of
`vers-run` and `vers-recall` read as the ledger's `product-version` rule
reads it); round 1's run at `1ae99e6d` gave the same. 38 tests pass. The probes (`probe.txt`): at the head the `/he`
drafts differ from fx only by fx's `/shell` row and the name column it
widens, and `four-all` only by the registry's size (38 against 34) -- its
range `1–3`, its `type to filter` and its three rows are fx's. That probe is
what found `057e3d1e`'s bug: at `6ee50b76` the header compared the whole draft
to `/` and dropped `type to filter` under `\n\n\n/`.

## Mutants (`mutants.py`, `mutants.txt`)

MUTANTS-R2-PLACEHOLDER

Round 1: 13 of 13 killed at `1ae99e6d`, on a throwaway `git clone --no-local` (the
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

Round 1's, at `1ae99e6d`; not run again, as round 2 changes no behaviour and
no scenario, and its pairs at `9af11724` are round 1's (15 of 15).
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

Round 2's, at `9af11724`, the code head; every commit after it is
`.prd/`-only. In one suite-lock slot: `cargo fmt --check`, both clippys with
`-D warnings`, `cargo test --locked --no-fail-fast` (5697 passed, 0 failed, 7
ignored; the lib 4773), the same with `--features fault-injection` (5734
passed, 0 failed, 7 ignored; the lib 4796), `check-xfx-identity`,
`check-no-stubs`, `check-no-secrets` and `check-no-local-paths` -- every step
exit 0, nothing re-run.

Round 1's, at `b1e896a0` (`1ae99e6d`'s src), in one suite-lock slot: `cargo fmt
--check`, both clippys with `-D warnings`, `cargo test --locked
--no-fail-fast` (5693 passed, 0 failed, 7 ignored; the lib 4769), the same
with `--features fault-injection` (5730 passed, 0 failed, 7 ignored; the lib
4792), `check-xfx-identity`, `check-no-stubs`, `check-no-secrets` and
`check-no-local-paths` -- every step exit 0, nothing re-run.

## References (`repoint.py`, `repoints.txt`, `refs.txt`)

`repoint.py` re-pointed the ledger refs the code moved (14 at `bca985c9`, six
at `057e3d1e`, 11 at `7eb2eac2`), each to the one line its text stands on
now, numbers only. `repoints.txt` is each round's output: the first two run
again at their commits on the throwaway clone, the third as written.
`refs.txt`: train 13's `refs_content.py` at `9af11724` -- the ledger 116 SAME,
37 SAME-ROW, 1 SAME-LINES, 1 WRONG; `docs/parity.md` 25 SAME, 0 WRONG -- the
same counts as at the base. The one WRONG (ledger:28, into
`.prd/07-fx-parity-program.md`) is the base's and not this unit's.

## `docs/parity.md`

In the full-screen TUI row, after "its argument completion are absent.": the
word may follow blanks, as fx's does (F1, F3); Tab, a skill row, the history
and the rows as above (F6-F8, `fx-p5-slash-leading-ws-*`); the latch across a
space after the word, the ghost hidden with it (F5); and what is not copied,
each with fx's line: fx's query going on past a blank (`argCompletionAnchor`,
F9), fx's one latch over `/mcp `'s list (`visibleInlinePickerKind`, F9),
fx's `/login ` line forgetting a slash dismissal (F4), and a line led by a
no-break space.

## Not copied, and why (not this unit's)

- **Past a blank after the word** (F9): fx's argument completion and its
  skills for an unknown name's words. A new unit if wanted (SLASH-ARG-SKILLS).
- **`/mcp `'s list under the slash latch** (F9): xfx's `mcp_args` keeps
  `mcp_args_dismissed`; the same at the base.
- **A line led by a no-break space** (or any Unicode blank past fx's four):
  fx's submit trims only `" \t\r\n"` (`input_submit_runtime.zig:719`), so it
  sends the line as a prompt. xfx's `crate::interactive::classify` trims
  every Unicode blank (`src/interactive.rs`'s `classify`) but `router::exact`
  only the four, so a command that takes nothing is refused as not an xfx
  command (`\u{a0}/version`) and one that takes an argument runs
  (`\u{a0}/model acme`): the router case
  `a_command_led_by_a_no_break_space_runs_with_an_argument_and_is_refused_without`.
  The same at the base.
- **`/login `** (F4): fx's provider prefixes include it, xfx's leave it out
  (`src/tui/provider_picker.rs`'s `PREFIXES`, "the gated command"). So a
  slash dismissal made over `/lo` survives `/login x` here, where fx forgets
  it -- and so did the base, by accident (its kind ended at the space). The
  dispatcher's decision: a divergence, owned by whichever unit enables
  `/login`; not changed here.
