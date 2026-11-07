# P5 CALLROW-SAFE receipt: a call's own row spells its argument as fx spells a command's

Security unit (S): terminal injection on a call's own row -- the transcript's
tool row and Ctrl-O's heading. Base: `151dde9f`, the head of T-U1-SAFE
(`fix/p5-ctrlo-safe-rows`). On `fix/p5-call-row-spelling`:

- round 1: `239a0a53` the row, the channel, Ctrl-O's argument rule, the tests,
  the scenario; `3207a64f` the reason crosses spelled too; `04e8dd33` ledger and
  `docs/parity.md`; `c3ac7120` the 120-byte status rule; `29e8f8d3` its docs;
  `9f93f8a5` the round-1 receipt (now under `round1/`);
- round 2 (the review's batch): `1789e2a6` the status phrase made from the raw
  call, the source-bound cut marked, the tests, the scenarios; `57af20f1` the
  ledger and `docs/parity.md`; `f52e97a9` its receipt;
- round c (opus-b's after-merge SHOULDs): `696700bb` the status phrase never
  marked for a command read in part, the tests; the ledger, `docs/parity.md` and
  this receipt on top.

Oracle: fx v0.0.16 (`vercel-labs/fx@481104fd`), sha256 `8e6cda2e…`, checked by
the sweep before and after each run.

**Builds.** Every `xfx/` capture is from a debug build of `696700bb`, the last
code commit (the commits on top touch no `src/`); every `red/` capture from a
debug build of `9f93f8a5`, round 2's base, in a `git clone --no-local`.
`publish.py` checked, as it copied them in, that each snap whose row 1 is the
banner reads `x𝒇x v0.1.0-696700b [debug]` (xfx) or `v0.1.0-9f93f8a` (red).
The reason sweep (`reasons.txt`) is round 2's, from `1789e2a6` and `151dde9f`:
round c changes only the status phrase, which no row draws.
Captures are from `scripts/fxcap/sweep.sh --only …` (one sweep per build and
size), re-scored with the ledger as committed (`--rescore`). Round 1's
captures, pairs, RED tests, mutants and gates (builds `29e8f8d3` against
`151dde9f`) are under `round1/`, as they were.

## The finding, on the base

A call's argument reached its row through two blanks and no spelling: the UI
channel made every control but CR and LF a space (`src/tui/bridge.rs:496` and
`:929` at `151dde9f`, `inert_owned`), and the row made every control a space and
left the marks as they were (`src/tui/toolgroup.rs:538` `spelled`). So on
`p5-call-row-safe` (`round1/red/`) the command's row reads
`Ran printf '%s' 'A<U+202E>B<U+2066>C<U+061C>D [2JE F G H' >/dev/null`, the
pattern's `Matched n<U+202E>o<U+2066>p<U+061C>q 7r s t u*.md` and the read's
`Failed m<U+202E>…: Path not found: m<U+202E>…` -- the right-to-left override,
the isolate and the Arabic letter mark handed to the terminal on both views,
the reason quoting them too (it crossed blanked and the shell cut it at 120
bytes, `src/tui/shell.rs:4033` at `151dde9f`).

## fx's rule (`@481104fd`)

| part | where | rule |
|---|---|---|
| a command's row: producer | `formatRunCommandDetailBounded(command, workspace_root, max_run_command_reflow_bytes)`, `src/core/app/app_callbacks.zig:544-549`, the display stored beside the call; the status phrase itself is the 120-byte one (`formatRunCommandActivity`, `src/core/tooling/tool_presentation.zig:393-416`, `max_run_command_activity_bytes`, `:21`) and the group puts the stored display in its place when it ends `...` (`reprojectTruncatedCommandPhrase`, `src/ui/transcript/tool_group_projection.zig:701-715`) | |
| projection | `projectRunCommandActivitySource`, `tool_presentation.zig:256-311` | of the command's first 14 400 bytes (`max_run_command_activity_source_bytes`, `:22`; `source_limit`, `:268`), a CR or LF ends a line, the spaces and tabs opening the next go with it, and one space stands for the break once something came before it and something follows -- so a bare CR is a space and a trailing one nothing; the bytes are copied one by one, so a character the 14 400 splits is read in part; **what is left past the 14 400 is dropped with no marker** (the only marker is the encoder's, `:445`, when what was read is too long to write). (Also `cd . &&` dropped and the workspace root written `.`: `stripNoopCurrentDirectoryPrefix`, `workspaceRootMatchesAt`, `:313-329`.) |
| encoder | `encodeTerminalSafe`, `src/core/shared/text_utils.zig:353-390`; tokens `terminalSafeToken`, `:643-667` | C0 and DEL `\xNN` (`writeByteEscape`; a tab `\x09`); a byte that is no whole UTF-8 sequence `\xNN` (`:652-657`); C1 and `isNonPrintingCodepoint` (`:703-709`: `U+200B-200F`, `U+2028-202E`, `U+2060-206F`, `U+FEFF`) `\u{NNNN}`; everything else as itself -- **U+061C included** |
| length | `encodeTerminalSafe`, `:353-390`; bound `max_run_command_reflow_bytes - 1` = 14 399 (`tool_presentation.zig:23`, `:441`) | on what is written, not read: once a token would pass the bound the text is cut back to the last whole token that leaves room for the marker, and `...` (three dots, `:358`) is put there -- never inside an escape |
| inline row | `formatGroupBlock`, `tool_group_projection.zig:558-668`: `normalizeCanonicalStatus` (`:317-347`), the reprojection, `highlightCommandPhrase`, `clipSummary` (`:384-400`) | cut at the screen's width with `…`, by cells, wherever that falls (inside an escape's text too: `fx/p5-call-row-safe-40x60/…-end.txt` row 8, `\u{0…`) |
| Ctrl-O heading | `formatExpandedChild`, `:788-820` | the same phrase, never cut, wrapped under its rail |
| a file change's path | `fileMutationDisplayPath`, `src/core/agent/runtime/tool_presentation.zig:736-753` | `encodeTerminalSafe(path, diff.max_encoded_label_bytes = 4 096)` (`src/core/output/diff.zig:10`), no projection |
| any other argument (read, list, pattern, ...) | `formatToolAction`, `src/core/app/app_agent_runtime.zig:1248-1310`, `formatToolActionValue`, `:1340-1342` | **as the model sent it**; `normalizeCanonicalStatus` then drops each byte below `0x20` and ends the row at its first CR or LF -- the marks and the C1 controls reach the terminal as themselves (`fx/p5-call-row-safe-40x120/…-end.txt` rows 9-10: `Matched n<U+202E>o<U+2066>p<U+061C>q7rs<U+009B>t`, and the read's row cut at its CR with `   )` painted past it, on every run) |
| a failure's reason | `failureStatusDetail`, `src/core/agent/runtime/tool_presentation.zig:1445-1493` | a tool's own `status_detail` **as it is** when it has one that is not `preflight failed` (`:1457-1458`); only the preflight and MCP reasons go through `encodeTerminalSafeInline(masked, 256)` (`text_utils.zig:397-417`: every run of ASCII white space -- space, `\t`-`\r` -- one space, none at either end, then the encoder at 256 bytes). xfx spells **every** reason that way, which is stricter. A resumed failed call shows no reason (`app_session_runtime.zig:4296-4312`: `Failed <target>`, a detail only for a command's outcome; capture `fx-resume-tools-40x120-resumed`), as xfx's |
| an argument's own row on Ctrl-O | `argumentVisibleInToolHeading`, `src/ui/full_transcript_screen.zig:6594-6619` | left out when the **raw** value is in the first line of the status entry's own bytes (escape sequences skipped, `containsIgnoringSgr`, `:6574-6592`) and that status fits the screen. So a value holding anything the encoder spells is never found -- it keeps its row (`fx/p5-call-row-safe-40x120/…-full.txt`: `command:`, `pattern:` and `path:` all shown) -- a command past 120 bytes keeps its row too, and the 120-byte cut is the encoder's on the **raw** command, so it never falls inside an escape |

## What changed (lines at `696700bb`)

- `src/tui/toolgroup.rs:600` `spelled(target, activity)`: a command projected
  as fx projects it, then `Spelling` (`:741`, fx's `encodeTerminalSafe`:
  `crate::tools::question::terminal_safe` per character, U+061C `\u{061c}` too)
  at `COMMAND_TARGET_BYTES` = 14 399 (`:64`; `command_spelled`, `:648`); any
  other argument `Spelling` as it is at `TARGET_BYTES` = 4 096 (`:76`). The
  bound counts what is written, cuts at a whole token and ends `...` (`CUT`,
  `:85`); a character the 14 400-byte source bound splits is `\xNN` per byte
  read, as fx's -- and where fx says nothing of a command it stopped reading at
  that bound with anything left to show, the row is marked cut, `...` (`:663`,
  `:685`; ledger `call-row-source-cut`). Idempotent (no line break, nothing it
  would spell, nothing past its bound).
- `toolgroup.rs:709` `flattened`: fx's `encodeTerminalSafeInline` at 256 bytes
  (`REASON_BYTES`, `:81`), ASCII white space as Zig's `isWhitespace` (VT
  included). A reason empty once flattened adds no `: `.
- `toolgroup.rs:631` `status`: fx's status phrase for a call -- a command's
  first 120 bytes spelled (`COMMAND_STATUS_BYTES`, `:71`), with no `...` for a
  command read only in part, as fx's has none (`command_spelled(.., false)`,
  round c), `None` for any other call -- made from the **raw** call.
- `src/tui/bridge.rs:955` `spelled_shown`, used for a live call (`:498`) and a
  resumed one (`:932`): the argument crosses the channel spelled by the row's
  own rule instead of blanked; the reason crosses through `flattened` (`:515`),
  and the shell hands it on as it came (`src/tui/shell.rs:4026`; its 120-byte
  `TOOL_DETAIL_BYTES` cut and its `safe_one_line` import gone). `made_inert`
  is `pub(crate)` (`:477`) for the shell's tests.
- `src/tui/full_transcript.rs`, one call site (and the doc on `Argument::text`,
  `:409-417`, and tests): `arguments` (`:617-636`), the one place the raw
  command and the raw values meet before anything is spelled, computes
  `status` from `spec.shown(input)` and makes `Argument::text` true only for a
  JSON string the heading would show as written -- no character the heading
  spells, U+061C included -- **and** that the raw-made status holds. The
  "shown already" test (`:1894-1897`) is the sibling's original.
- Not ported, and never there: `cd . &&` dropped and the workspace root written
  `.` (named in `docs/parity.md:238`). The width test still measures the whole
  heading, which for a command past 120 bytes is wider than fx's status: a
  short value inside a long command can keep a row fx would leave out -- the
  safe direction, said in `docs/parity.md:238`.

## Round 2 (the review's batch)

**MUST -- the status phrase from the raw call.** Round 1's `status_shows`
re-spelled the channel's already-spelled target at 120 bytes, so its cut could
fall inside an escape's spelling: 116 `a`, U+202E, `z` became 116 `a`, a literal
`\` and `...`, where fx's, on the raw command, backs up before the whole
`\u{202e}`. A text value ending in `\` was then "shown already" and hidden.
**Chosen:** compute the phrase from the raw value -- in `arguments`, where the
raw call is already in hand on both the live and the resumed path, so nothing
new has to cross the bridge and the 120-byte cut is fx's exactly -- but for the
`cd . &&` and workspace-root `.` projection xfx has never ported
(`docs/parity.md:238`) -- (and, since
round c, so is the absence of a source-cut marker in it); not the fallback of
hiding no value near an escape, which would still differ from fx.
- RED (`red-tests.txt`, on a clone of `9f93f8a5` with the round-2 test hunks and
  the one-line `pub(crate)` on `made_inert` they need):
  `a_resumed_calls_short_argument_is_looked_for_in_fxs_status_of_the_raw_command`
  -- the log, `bridge::inert_hydration`, the shell, Ctrl-O at 160 columns --
  **fails**: no `│  cwd: a\` row.
- GREEN: it passes at `1789e2a6`; mutant `status-from-spelled` (the bound on the
  spelled call, round 1's way) fails it.

**SHOULD 1 -- a command read in part says so.**
- RED: `a_command_read_only_in_part_says_so_where_fx_says_nothing` fails on
  `9f93f8a5` (`└ Running echo shown`, no marker). Pair `p5-call-row-long` on
  `9f93f8a` (`red/`): xfx's `└ Failed echo shown` is fx's unmarked row, and
  explained by nothing.
- GREEN: the test passes (three cases: `echo hidden` past 14 400 bytes of line
  breaks marked; a tail of white space alone not; a character the bound splits,
  `\xe2...`). Pair `p5-call-row-long` at `1789e2a`: the `...` is
  `call-row-source-cut` (divergence, unsafe-to-copy); the verb `Failed` and the
  header's `1 failed` are `command-size-bound` (cause): xfx refuses a command
  past 8 KiB (`src/permission/authority.rs:41`), where fx runs one up to 64 KiB,
  so the scenario's 14 421-byte command never runs on xfx. **No row
  unexplained.** Mutant `no-source-cut-marker` fails the test.

**SHOULD 2 -- reasons, live and resumed.**
`a_failed_calls_long_reason_is_spelled_and_bounded_alike_on_both_views`: a
failed read's reason of 100 `a`, white space across byte 120 (the old cut),
100 `b`, `ESC[2J`, 29 `c` and U+202E across byte 256: live, through
`made_inert`, the document's row and Ctrl-O's heading are both
`└ Failed gone.txt: Path not found: a… b…\x1b[2Jc......` (256 bytes, cut before
the whole `\u{202e}`), and at 120 columns the document's row is that row cut at
the edge; resumed, both views show `└ Failed gone.txt`, no reason, as fx's
replay. Mutants `reason-blanked` and `no-control-spelling` fail it.

**SHOULD 3 -- receipt text.** The table's reason row now names fx's raw
`status_detail` (`:1457-1458`) and says xfx is stricter; so does
`call-row-raw-argument`'s reason.

**SHOULD 4 -- the ledger regex.** `p5-call-row-safe`'s command carries no
U+061C now, and `call-row-raw-argument`'s selectors name the pattern's and the
path's rows only, by their own text (`n\u{202e}o`, `q\x1b7r`, `u*.md`,
`m\u{202e}n`, `t.txt`, `Path not found`, `├ Matched` and `└ Failed` alone on a
wrapped row). The command's heading and `command:` rows are compared with
nothing set aside, and they match fx's (`pairs.txt`: no command row listed).

**SHOULD 5 -- the reason sweep (`reasons.txt`).**

Every `p5-transcript-*`, `p5-tool*`, `p5-mcp-*` and `*fail*` scenario -- 31
scenarios, 215 snaps -- at 40x120 on the base `151dde9f` (before the shell's
120-byte cut was removed) and on `1789e2a6`, each against fx under the
committed ledger (`reasons.txt`: both verdicts per snap, then every row the two
builds' xfx captures differ in, run-to-run noise masked). Snaps passing: 34 on
the base, 35 here. Every difference is attributed there:
- `p5-transcript-safe` `end`, `end.full`, `full`, `full-pgup`: the command's own
  row, spelled -- this unit; now explained (`end`, `end.full`
  normalized-identical).
- `p5-transcript-records` `start`: the hint row's 4-second full-access warning
  against the scenario's `wait 4` -- timing, not this unit: re-run alone twice
  per build, it matched fx both times here and once of two on the base.
- `p5-transcript-cancel` `mid`, `p5-transcript-submit` `submit-0.15`: a
  streaming token count on a mid-turn snap; `p5-transcript-md` `answered`,
  `closed`: the turn's elapsed seconds (`1s`/`2s`) -- load, both passes.
No other row differs: no failure reason these scenarios capture reaches 120
bytes, so removing the cut changed no captured row; reasons past 120 and 256
bytes are SHOULD 2's test. Of the 140 snaps with a row unexplained here, 139
have one on the base too (143 there), in other units' surfaces; the 140th is
`p5-transcript-records` `start`, above.

## Round c (opus-b's after-merge SHOULDs)

**1 -- the status phrase inherited the source-cut marker.** `command_spelled`
set the row's `...` whatever its bound, and `status` went through it, so where a
command's first 14 400 bytes spell to 120 bytes or fewer and something visible
follows, xfx's status ended `...` and fx's 120-byte one, which marks only what
its encoder cuts (`tool_presentation.zig:441-445`), does not: a `.` was found
in it and `cwd: .` hidden. **Fix:** `command_spelled(target, max, mark_unread)`
(`toolgroup.rs:648`); the row passes `true` (`:610`), the status `false`
(`:636`).
- RED (`red-tests-c.txt`, f52e97a9's code with the test added):
  `a_command_read_only_in_part_hides_no_argument_fxs_status_would_show` -- `ls`,
  14 400 line breaks, `x`, with `cwd: "."` -- **fails**: `└ Ran ls...` is drawn
  and `│  cwd: .` is not.
- GREEN: it passes at `696700bb`. Mutant `status-marks-unread` (the status
  passing `true`) fails it, and only it.
- `call-row-source-cut`'s reason, `docs/parity.md:238` and the table above say
  the marker is the row's alone.

**2 -- the split-character marker is pinned.** The cut test gains a case where
the character the 14 400-byte bound splits ends the command (`echo shown`,
14 389 line breaks, U+202E): only the marker at the split (`toolgroup.rs:685`,
round 2's `:677`) can mark it. Mutant `no-split-cut-marker` (that line
reverted) now fails the cut test; it survived round 2's suite.

**3 -- `command-size-bound` reworded.** The 8 KiB bound is on the command once
trimmed (`authority.rs:1145-1149`) while the row's target is the raw command
(`src/tools/spec.rs:219`), so a command read only in part can be admitted --
with white space at its ends of at least 6 208 bytes more than its visible
tail past byte 14 400 (so 6 209 at the fewest; 6 208 alone with an 8 192-byte
body is read whole and not marked) it passes and runs, its row ending `...` -- and round 2's "only on a refused row" was wrong. The bound
is a deliberate choice of fx's planning bound (`authority.rs:38-40`): the cause
is now `kind = "unit"`, owner `CMD-SIZE`, and raising the bound is an S change.

## Pairs (`pairs.txt`)

- `p5-transcript-safe` `end`, `end.full`: **normalized-identical** at both sizes
  -- the command's row is fx's cell for cell; `full`, `full-pgup`: only
  `tool-schema`, `session-record`, `safe-page-edge`, **no row unexplained**.
- `p5-call-row-safe` `end`: **approved-divergence** (`call-row-raw-argument`
  only), both sizes; `full`: `call-row-raw-argument`, `tool-schema`,
  `call-row-page-edge`, **no row unexplained**.
- `p5-call-row-long` `end`: `call-row-source-cut`, `command-size-bound`, **no
  row unexplained**.
- From `696700b` (round c), as from `1789e2a` (round 2): round c changes only
  the status phrase, which no row draws, and every pair is classified as it was.

## Mutants (`mutants.txt`, `mutants.py`)

Each alone on a throwaway `git clone --no-local` of `696700bb`, fourteen tests run:

| mutant | what it does | fails |
|---|---|---|
| `no-mark-naming` | C0, DEL and C1 named, no mark | 7 |
| `no-control-spelling` | every control a space again, the marks named | 7 (the reason test among them) |
| `bridge-blanks` | the channel blanks the argument again | 3 |
| `no-text-flag` | every string value may be "shown already" | 4 |
| `no-status-bound` | no status phrase looked in | 3 |
| `status-from-spelled` | the 120-byte bound on the spelled call (round 1's way) | 2: the MUST test, round c's status test |
| `no-source-cut-marker` | the source bound says nothing of a visible tail | 2: the cut test, round c's status test |
| `no-split-cut-marker` | the split character says nothing (`:685` reverted) | 1: the cut test |
| `status-marks-unread` | the status carries the row's marker (before round c) | 1: round c's status test |
| `reason-blanked` | the channel blanks the reason again | 3 (the reason test among them) |

## Ledger and parity

- `scripts/fxcap/parity-ledger.toml`: `ctrl-o-call-row-spelling` (cause)
  deleted (round 1). `call-row-raw-argument` (divergence, `unsafe-to-copy`, pin
  v0.0.16, `p5-call-row-safe`, owner CALLROW-SAFE, expires 2026-12-04): fx
  writes a read's, a list's and a pattern's row with the raw argument and U+061C
  as itself everywhere, and a tool's own reason raw; xfx spells them -- its
  selectors now the pattern's and the path's rows only. `call-row-source-cut`
  (divergence, `unsafe-to-copy`, `p5-call-row-long`): fx's silent source cut,
  xfx's `...`. `command-size-bound` (cause, kind `unit`, owner CMD-SIZE): xfx's 8 KiB
  bound on the trimmed command, a deliberate choice; raising it is an S change.
  `call-row-page-edge` (cause): the Ctrl-O tail page's top rows.
- `docs/parity.md:237` (full-screen TUI: each argument spelled) and `:238`
  (full transcript: the rule; the argument row's raw-value and 120-byte status
  rules, judged on the raw call; the source cut; every reason spelled where fx
  writes a tool's `status_detail` raw; the "open, T-U4" 1 024-byte length gap
  closed).

## Gates (`gates.txt`)

At `a177d863` (code of `696700bb`), one step at a time: fmt, clippy with and
without fault injection, both test suites (**4953** and **4982** passed, 0
failed, 7 ignored each; the fourteen tests above among them), identity, stubs,
secrets, fxcap's test-sweep, the release and faulty builds, smoke (47 checks, 0
failures) and smoke-tui (37 scenarios and the oracle, 1111 checks, 0 failures):
every step exit 0. Round 2's list ran green at `57af20f1`, round 1's
(`round1/gates.txt`) at `29e8f8d3`.

## Files

`fx/`, `red/`, `xfx/` the captures by scenario and size -- `xfx/` from round
c's build `696700b`, `red/` from round 2's base `9f93f8a` -- (redacted,
`scripts/fxcap/redact.py`); `pairs.txt` their pairs; `reasons.txt` round 2's
reason sweep, both builds; `red-tests.txt` (round 2) and `red-tests-c.txt`
(round c); `mutants.txt`; `gates.txt`; `publish.py`,
`split_tests.py`, `mutants.py` the scripts that made them; `round1/` round 1's
receipt files as they were. The scenarios: `scripts/fxcap/scenarios/
p5-call-row-safe.cap`, `p5-call-row-long.cap` and their turn files. In the text
files a mark or control is written `<U+XXXX>`; the captures hold them as the
screens did.
