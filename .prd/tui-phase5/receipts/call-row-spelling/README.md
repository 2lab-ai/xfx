# P5 CALLROW-SAFE receipt: a call's own row spells its argument as fx spells a command's

Security unit (S): terminal injection on a call's own row -- the transcript's
tool row and Ctrl-O's heading. Base: `151dde9f`, the head of T-U1-SAFE
(`fix/p5-ctrlo-safe-rows`). On `fix/p5-call-row-spelling`:

- `239a0a53` the row, the channel, Ctrl-O's argument rule, the tests, the
  scenario;
- `3207a64f` the reason crosses spelled too; fx citations corrected;
- `04e8dd33` ledger and `docs/parity.md`;
- `c3ac7120` Ctrl-O looks for an argument in fx's 120-byte status phrase;
- `29e8f8d3` its `docs/parity.md` clause and the ledger's moved lines;
- this receipt on top.

Oracle: fx v0.0.16 (`vercel-labs/fx@481104fd`), sha256 `8e6cda2e…`, checked by
the sweep before and after each run.

**Builds.** Every `xfx/` capture is from a debug build of `29e8f8d3`, whose
code is `c3ac7120`'s (nothing under `src/` changes after it); every `red/`
capture from a debug build of the base `151dde9f` in a `git clone --no-local`.
`publish.py` checked, as it copied them in, that each snap whose row 1 is the
banner reads `x𝒇x v0.1.0-29e8f8d [debug]` (xfx) or `v0.1.0-151dde9` (red).
Captures are from `scripts/fxcap/sweep.sh --only p5-call-row-safe --only
p5-transcript-safe` at 40x120 and at 40x60, one sweep per build and size,
re-scored with the ledger as committed (`--rescore`).

## The finding, on the base

A call's argument reached its row through two blanks and no spelling: the UI
channel made every control but CR and LF a space (`src/tui/bridge.rs:496` and
`:929` at `151dde9f`, `inert_owned`), and the row made every control a space and
left the marks as they were (`src/tui/toolgroup.rs:538` `spelled`). So on
`p5-call-row-safe` (`red/`) the command's row reads
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
| projection | `projectRunCommandActivitySource`, `tool_presentation.zig:256-311` | of the command's first 14 400 bytes (`max_run_command_activity_source_bytes`, `:22`), a CR or LF ends a line, the spaces and tabs opening the next go with it, and one space stands for the break once something came before it and something follows -- so a bare CR is a space and a trailing one nothing; the bytes are copied one by one, so a character the 14 400 splits is read in part. (Also `cd . &&` dropped and the workspace root written `.`: `stripNoopCurrentDirectoryPrefix`, `workspaceRootMatchesAt`, `:313-329`.) |
| encoder | `encodeTerminalSafe`, `src/core/shared/text_utils.zig:353-390`; tokens `terminalSafeToken`, `:643-667` | C0 and DEL `\xNN` (`writeByteEscape`; a tab `\x09`); a byte that is no whole UTF-8 sequence `\xNN` (`:652-657`); C1 and `isNonPrintingCodepoint` (`:703-709`: `U+200B-200F`, `U+2028-202E`, `U+2060-206F`, `U+FEFF`) `\u{NNNN}`; everything else as itself -- **U+061C included** |
| length | `encodeTerminalSafe`, `:353-390`; bound `max_run_command_reflow_bytes - 1` = 14 399 (`tool_presentation.zig:23`, `:441`) | on what is written, not read: once a token would pass the bound the text is cut back to the last whole token that leaves room for the marker, and `...` (three dots, `:358`) is put there -- never inside an escape |
| inline row | `formatGroupBlock`, `tool_group_projection.zig:558-668`: `normalizeCanonicalStatus` (`:317-347`), the reprojection, `highlightCommandPhrase`, `clipSummary` (`:384-400`) | cut at the screen's width with `…`, by cells, wherever that falls (inside an escape's text too: `fx/…-40x60-end.txt` row 8, `\u{0…`) |
| Ctrl-O heading | `formatExpandedChild`, `:788-820` | the same phrase, never cut, wrapped under its rail |
| a file change's path | `fileMutationDisplayPath`, `src/core/agent/runtime/tool_presentation.zig:736-753` | `encodeTerminalSafe(path, diff.max_encoded_label_bytes = 4 096)` (`src/core/output/diff.zig:10`), no projection |
| any other argument (read, list, pattern, ...) | `formatToolAction`, `src/core/app/app_agent_runtime.zig:1248-1310`, `formatToolActionValue`, `:1340-1342` | **as the model sent it**; `normalizeCanonicalStatus` then drops each byte below `0x20` and ends the row at its first CR or LF -- the marks and the C1 controls reach the terminal as themselves (`fx/p5-call-row-safe-40x120/…-end.txt` rows 9-10: `Matched n<U+202E>o<U+2066>p<U+061C>q7rs<U+009B>t`, and the read's row cut at its CR with `   )` painted past it, on every run) |
| a failure's reason | `failureStatusDetail`, `src/core/agent/runtime/tool_presentation.zig:1445-1493` | `encodeTerminalSafeInline(masked, 256)` (`text_utils.zig:397-417`): every run of ASCII white space (space, `\t`-`\r`) one space, none at either end, then the encoder at 256 bytes |
| an argument's own row on Ctrl-O | `argumentVisibleInToolHeading`, `src/ui/full_transcript_screen.zig:6594-6619` | left out when the **raw** value is in the first line of the status entry's own bytes (escape sequences skipped, `containsIgnoringSgr`, `:6574-6592`) and that status fits the screen. So a value holding anything the encoder spells is never found -- it keeps its row (`fx/p5-call-row-safe-40x120/…-full.txt`: `command:`, `pattern:` and `path:` all shown) -- and since a command's status is its first 120 bytes spelled, not the command the heading shows whole, a command past 120 bytes keeps its row too |

## What changed (lines at `29e8f8d3`)

- `src/tui/toolgroup.rs:598` `spelled(target, activity)`: a command projected
  as fx projects it, then `Spelling` (`:715`, fx's `encodeTerminalSafe`:
  `crate::tools::question::terminal_safe` per character, U+061C `\u{061c}` too)
  at `COMMAND_TARGET_BYTES` = 14 399 (`:64`; `command_spelled`, `:635`); any
  other argument `Spelling` as it is at `TARGET_BYTES` = 4 096 (`:76`). The
  bound counts what is written, cuts at a whole token and ends `...` (`CUT`,
  `:85`); a character the 14 400-byte source bound splits is `\xNN` per byte
  read, as fx's. Idempotent (no line break, nothing it would spell, nothing past
  its bound).
- `toolgroup.rs:683` `flattened`: fx's `encodeTerminalSafeInline` at 256 bytes
  (`REASON_BYTES`, `:81`), ASCII white space as Zig's `isWhitespace` (VT
  included). A reason empty once flattened adds no `: `.
- `toolgroup.rs:625` `status_shows`: a command's target spelled at fx's
  120-byte status bound (`COMMAND_STATUS_BYTES`, `:71`), where Ctrl-O looks for
  an argument the heading already shows.
- `src/tui/bridge.rs:954` `spelled_shown`, used for a live call (`:497`) and a
  resumed one (`:931`): the argument crosses the channel spelled by the row's
  own rule instead of blanked; the reason crosses through `flattened` (`:514`),
  and the shell hands it on as it came (`src/tui/shell.rs:4026`; its 120-byte
  `TOOL_DETAIL_BYTES` cut and its `safe_one_line` import gone).
- `src/tui/full_transcript.rs`, two call sites only (and the doc on
  `Argument::text`, and tests): `:624` in `arguments` -- `Argument::text` is
  true only for a JSON string the heading would show as written, no character
  the heading spells, U+061C included; `:1888` -- the one conjunct
  `status_shows`. So "shown already" (`:1884-1888`) never hides a value fx's
  raw comparison would not find.
- Not ported, and never there: `cd . &&` dropped and the workspace root written
  `.` (named in `docs/parity.md:238`). The width test still measures the whole
  heading, which for a command past 120 bytes is wider than fx's status: a
  short value inside a long command can keep a row fx would leave out -- the
  safe direction, said in `docs/parity.md:238`.

## RED

- **Tests** (`red-tests.txt`): this unit's tests put on the base alone
  (`split_tests.py`: the test-module hunks of `151dde9f..c3ac7120`): **10 of 10
  fail** -- the four toolgroup tests (rows `Ran printf '%s' 'A<U+202E>B…D [2JE F
  G H'`, `Matched n<U+202E>o…`, `Running echo a   echo b   echo c `, a bounded row
  1 039 bytes long where 14 407 are expected), the bridge's
  (`Some("…D [2JE\rF G H'…")`) and its send contract, Ctrl-O's heading at every
  width, the argument rule (a 121-byte command's `command:` row hidden on the
  base), the journal tee's flag and the resumed session's document.
- **Pairs** (`pairs.txt`, `red/`): `151dde9` against fx under the committed
  ledger -- `p5-call-row-safe` `end` rows 8-10 and `full` rows 18, 22, 28 at
  40x120 (at 40x60 `end` 8-10 and `full` 11, 17, 25, 26) explained by nothing;
  `p5-transcript-safe` `end`, `end.full`, `full`, `full-pgup` each with the
  command's row unexplained.

## GREEN

- **Tests**: the ten above pass at `29e8f8d3` (`gate-test`, the full suite,
  `gates.txt`).
- **Pairs** (`pairs.txt`, `xfx/`), `29e8f8d` against fx, both sizes:
  - `p5-transcript-safe` `end`, `end.full`: **normalized-identical** -- the
    command's row is fx's cell for cell; `full`, `full-pgup`: only the causes
    T-U1-SAFE left (`tool-schema`, `session-record`, `safe-page-edge`), **no row
    unexplained**. `ctrl-o-call-row-spelling` is deleted: it covers nothing now.
  - `p5-call-row-safe` `end`: **approved-divergence**, `call-row-raw-argument`
    only; `full`: **no row unexplained** -- `call-row-raw-argument`,
    `tool-schema` (fx's `action: run`, `yield_time_ms`) and
    `call-row-page-edge` (the page's top rows those move).
- The command's row is fx's at both widths but for U+061C: at 40x60 both cut it
  with `…` at the screen's edge, xfx's 8 characters sooner for the name
  (`\x…` where fx's is `\u{0…`).

## Mutants (`mutants.txt`, `mutants.py`)

Each alone on a throwaway `git clone --no-local` of `29e8f8d3`, the ten tests run:

| mutant | what it does | fails |
|---|---|---|
| `no-mark-naming` | C0, DEL and C1 named, no mark (U+202E, U+2066, U+061C as themselves) | 6: the row, any-width, bound, bridge, Ctrl-O heading, resumed document |
| `no-control-spelling` | every control a space again, the marks named | 6: the row, line breaks, bridge, send contract, Ctrl-O heading, resumed document |
| `bridge-blanks` | the channel blanks the argument again | 3: bridge, send contract, resumed document |
| `no-text-flag` | every string value may be "shown already" | 2: the argument rule, the journal tee |
| `no-status-bound` | the whole heading searched, not fx's 120-byte status | 1: the argument rule |

## Ledger and parity

- `scripts/fxcap/parity-ledger.toml`: `ctrl-o-call-row-spelling` (cause)
  deleted; `call-row-raw-argument` (divergence, `unsafe-to-copy`, pin v0.0.16,
  `p5-call-row-safe`, owner CALLROW-SAFE, expires 2026-12-04): fx writes a
  read's, a list's and a pattern's row with the raw argument and U+061C as
  itself everywhere; xfx spells them as fx spells a command's, so they are
  longer, the read's carries its reason, and they are bounded at 4 096 where
  fx's raw rows are not; at a narrow width those rows wrap at other columns.
  `call-row-page-edge` (cause, follows `tool-schema`): the tail page's top three
  rows.
- `docs/parity.md:237` (full-screen TUI: each argument spelled) and `:238`
  (full transcript: the "Open" call-row note replaced by the rule, the argument
  row's raw-value and 120-byte status rules; the "open, T-U4" 1 024-byte length
  gap closed -- the heading now keeps a command to fx's 14 399 bytes).

## Gates (`gates.txt`)

At `29e8f8d3`, one step at a time: fmt, clippy with and without fault
injection, both test suites (**4949** and **4978** passed, 0 failed, 7 ignored
each; the ten above among them), identity, stubs, secrets, fxcap's test-sweep
(48 + 24 tests), the release and faulty builds, smoke (47 checks, 0 failures)
and smoke-tui (37 scenarios and the oracle, 1111 checks, 0 failures): every step
exit 0. The same list ran green at `04e8dd33` before `c3ac7120`.

## Files

`fx/`, `red/`, `xfx/` the captures, by scenario and size (redacted,
`scripts/fxcap/redact.py`); `pairs.txt` every snap pair of both builds and
sizes; `red-tests.txt`; `mutants.txt`; `gates.txt`; `publish.py`,
`split_tests.py`, `mutants.py` the scripts that made them. The scenario:
`scripts/fxcap/scenarios/p5-call-row-safe.cap` and its two turn files. In the
text files a mark or control is written `<U+XXXX>`; the captures hold them as
the screens did.
