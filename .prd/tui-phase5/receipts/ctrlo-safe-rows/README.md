# P5 T-U1-SAFE receipt: Ctrl-O's argument, read and command rows hand the terminal nothing it acts on

Security unit (S): terminal injection on the full transcript. Base:
`train/p5-1107` at `56979c59` (PR #73's train, which holds T-U1+U2's
tool-row wrap). Fix: `127f0502`, and `28f3fda5` (clippy's rename of the
decoder's state to fx's `EscapeState`; two shell tests read the spelled rows)
on `fix/p5-ctrlo-safe-rows`. Oracle: fx
v0.0.16 (`vercel-labs/fx@481104fd`).

**Builds.** Every `xfx/` capture is from a debug build of `28f3fda`, the
code commit under this receipt; every `red/` capture from a debug build of the base
`56979c5`. `publish.py`'s check as the captures were copied in: each snap whose
row 1 is the banner reads `x𝒇x v0.1.0-28f3fda [debug]` (xfx) or
`v0.1.0-56979c5` (red). The seven scenarios `check.txt` re-reads were captured
from the same `28f3fda` build (row 1 of each: `v0.1.0-28f3fda`). fx is the
v0.0.16 reference binary, sha256 `8e6cda2e…`, checked before and after each
sweep (`sweep-*.md`).

## The finding, on the base

`Detail::made_inert` passed every detail through `bridge::inert`, which makes
a control a space and leaves everything else: on `p5-transcript-safe`
(`red/`), the argument row, the command's rows and the read's rows show the
right-to-left override, the zero-width space and the byte-order mark as
themselves, `ESC[2J` as ` [2J`, the C1 `U+009B` as a space, and drop the
argument's last carriage return. A bare carriage return stayed in the rows
the screen built (`red-tests.txt`: `"│ over\r"` for the command, `"│  2
over\rX"` for the read); the frame drops it as it places a row, so the
captures show `over` / `X` and `overX` where fx shows `X` and `over\x0dX`.

## fx's rule, row by row (`@481104fd`)

| row | path | spelling |
|---|---|---|
| argument | `appendFullSemanticArguments`, `src/ui/full_transcript_screen.zig:6621-6699` → `appendTerminalSafeToolOutputInterruptible`, `:6527-6572` | `TerminalSafeIndentedWriter`, `:5147-5194`: a line feed ends the row (`:5167-5170`), a tab is four spaces (`:5171-5172`), the rest goes through `IncrementalTerminalSafeEncoder` (`src/core/shared/text_utils.zig:112-171`) and so `terminalSafeToken` (`:643-667`): C0 and `DEL` `\xNN` (`writeByteEscape`, `:675-686`) -- a carriage return too, at the end or not -- and `isNonPrintingCodepoint` (`:703-708`: C1, `U+200B-200F`, `U+2028-202E`, `U+2060-206F`, `U+FEFF`) `\u{NNNN}` (`writeCodepointEscape`, `:688-701`) |
| read (result) | stored: `appendStoredResultContent`, `full_transcript_screen.zig:5196-5229` (`StoredResultSourceStream.appendPage`, `:5087-5093`); inline: `:6856-6864` after `contentForDisplay` (`src/core/tooling/tool_result_display.zig:6-16`) | the same `TerminalSafeIndentedWriter` |
| command output | `appendInlineCommandResult`, `full_transcript_screen.zig:6960-7008` → `canonicalizeForegroundResult` (`src/core/tooling/command_output_content.zig:473`) → `renderCommandOutputRecordWithPrimaryGutter` (`src/ui/transcript/command_output_runtime.zig:1985-1999`) → `wrapLiteralCommandOutput` (`src/ui/render_engine/assistant_wrap.zig:528-546`) | its `Decoder`, `command_output_content.zig:52-282`: escape sequences dropped (`:196-253`; one cut short written back, `:264-275`), a bare carriage return starts the line again and CRLF ends it (`:86-95`, `replaceLine` `:424-429`), a tab kept for the wrap's eight-column screen stops, other C0 and `DEL` `\xNN`, C1 `\u{00NN}` (`:113-176`), **everything else as itself** -- the marks included (`:171-173`) |

`appendTerminalSafeLine` (`text_utils.zig:419-442`), named in the brief, is
the diff rows' (`src/core/output/diff.zig:1273`), not these rows'.

fx's capture (`fx/p5-transcript-safe-40x120/…-full.txt`) shows each:
`│  command: printf '%s\n' 'A\u{202e}B\u{200b}C\u{feff}D\x1b[2JE\u{009b}F    G' >/dev/null; cat safe.txt # \x0d`;
`│ a<U+202E>b<U+200B>c<U+FEFF>de\u{009b}f        g` and `│ X`;
`│  1    a\u{202e}b\u{200b}c\u{feff}d\x1b[2Je\u{009b}f    g\x0d` and `│  2    over\x0dX`.

## What changed (`127f0502` + `28f3fda5`, `src/tui/full_transcript.rs`; lines at `28f3fda5`)

- `Detail::made_inert` (`:424`) spells an argument's name and value, a
  result and a command's status with `detail_spelled` (`:486`): fx's
  `TerminalSafeIndentedWriter`, built on the existing
  `crate::tools::question::terminal_safe` (fx's `terminalSafeToken`) per
  line and per tab (`FOUR_CELLS`, `:458`); a command's output the same with
  its tab kept. A change's sides keep `bridge::inert` (T-U6's rows).
  Idempotent: a resumed session's details cross it twice
  (`bridge.rs:878`, `shell.rs:3647`), and what it writes is printable ASCII.
- `command_result` (`:653`) reads each stream through `command_stream`
  (`:709`), a port of fx's `Decoder` (per stream, before `bounded`, so a cut
  can never fall inside a sequence), and no longer turns tabs into four
  spaces.
- `railed` (`:2043`) draws a command's tab as the spaces to the stop the wrap
  measured it to (`prose::expand_tabs`, `:2059`), then trims and clips.
- **Two differences from fx, both made on purpose**: the marks fx leaves raw
  in a command's output are named there too, and the Arabic letter mark
  (`U+061C`, which fx writes as itself everywhere and `approval_view` counts
  as reordering) is named on all three rows -- ledger `ctrl-o-command-marks`,
  `class = "unsafe-to-copy"`, the command row only in the captures (no
  capture has a `U+061C`). Each named mark makes the row 8 cells wider;
  the wrap measures the spelled text.
- **Merge note:** T-U6 (`feat/p5-transcript-receipts`) adds
  `terminal_safe_line` and `fx_names` to this file; this unit adds
  `detail_spelled`, `command_stream`, `FOUR_CELLS` and `ESCAPE_KEPT_BYTES`
  -- no name in common.

## RED

- **Tests** (`red-tests.txt`): the four detail-row tests on the base, the fix
  not yet written: all four fail (the argument row, the read rows, the
  command rows `["│ a<U+202E>b<U+200B>c<U+FEFF>d [2Je f    g", "│ over\r"]`,
  and the no-control check at 10 columns).
- **Pair** (`pairs.txt`, `sweep-base.md`): `56979c5` against fx under the
  committed ledger: rows no cause or divergence explains on `full` (fx 22, 29,
  30; xfx 22-24, 29-30) and `full-pgup` (fx 31; xfx 28-30): every argument,
  command and read row.

## GREEN

- **Tests**: `cargo test --locked --lib -- tui::full_transcript` and the two
  resumed-session shell tests (`a_hostile_log_paints_no_sequence_of_its_own_on_the_document_or_the_full_transcript`,
  `a_resumed_turn_fills_the_full_transcript_with_its_calls_details_at_the_logs_times`,
  now reading a result spelled -- their every-escape-is-an-SGR checks
  unchanged): 77 passed, 0 failed -- the four above, `a_commands_stream_is_read_as_fxs_decoder_reads_it`
  (one case per decoder branch: CSI, OSC to BEL and ST, `ESC Fe`, a
  sequence cut by a control, a line feed inside a CSI, a sequence the stream
  ends in, `ESC ESC [`, CRLF, a bare CR, the last CR, `CR ESC[2K`, `\xNN`,
  C1, a sequence past 64 bytes, the streams' independence), and
  `a_detail_is_made_inert_before_it_reaches_a_row` restated for the new
  contract (spelled, not blanked; idempotent). The no-control test checks
  every row at 10, 24, 80 and 120 columns fits (`wrap::width <= cols`).
- **Pair** (`pairs.txt`, `sweep-new.md`): `28f3fda` against fx: **no row is
  unexplained**. The argument and read rows are fx's cell for cell; the
  command row is fx's but for the named marks (`ctrl-o-command-marks`, on
  both pages); what is left is the call's own row (below) and the rows
  `tool-schema`, `session-record` and the new `safe-page-edge` (those two
  causes four rows deep at the two page edges) already name.
- **`check.py`** (`check.txt`): the T-U1U2 receipt's checker, its own fx
  captures against new captures of the seven scenarios from `28f3fda`:
  **34 claims, 34 hold**. No claim changed: the report is
  `transcript-wrap/green.txt` line for line, every count the same (`diff`
  with the hex digits of the revisions masked: no difference).

## Gates (`gates.txt`)

At `8ff28356`, one at a time after the idle wait: fmt, clippy with and without
fault injection, both test suites (4941 and 4970 passed, 0 failed, 7
ignored each), identity, stubs, secrets, fxcap's test-sweep, the release and
faulty builds, smoke (47 checks) and smoke-tui (37 scenarios, 1111 checks):
every step exit 0.

## Mutant (`mutants.txt`)

On a throwaway `git clone --no-local` of `127f0502` (`28f3fda5` renames
nothing in the arm it mutates): an argument's value names C0 and C1 but no
mark. `an_argument_row_is_spelled_as_fxs_terminal_safe_writer_spells_it`
and `no_detail_row_hands_the_terminal_a_control_or_a_mark_at_any_width`
fail on it, at the argument row (`'A<U+202E>B<U+200B>C<U+FEFF>D\x1b…`); the
read and command tests pass -- the assertions are per row kind.

## Sweep: every other row this screen draws

The probe (`sweep-probe.txt`) gives one entry of each kind text carrying
`P<U+202E>Q<U+200B>R ESC[2J S <U+009B> T TAB U`, made inert as the channel
makes every UiEvent's text: **no row builder below names a mark**; what a
row shows is what its producer hands it.

| row | builder | text from | status |
|---|---|---|---|
| a call's own row (Ctrl-O's heading, the transcript's tool row) | `ToolGroup::expanded` → `src/tui/toolgroup.rs:538` `spelled` | the model's argument | **open**: a control is a space, the marks stay; fx spells it with `encodeTerminalSafe` (`src/core/tooling/tool_presentation.zig:432-447`, `text_utils.zig:353-390`). Not changed here: the row is the transcript's too and its projection is T-U4's; ledger `ctrl-o-call-row-spelling` (defect), `docs/parity.md:238` |
| answer prose | `answer`, `src/tui/full_transcript.rs:2256` | the model, through `UiEvent::Delta` → `bridge::inert` (`src/tui/bridge.rs:479`) | **open**: controls spaces, marks raw -- as in the transcript; fx's prose rule not read |
| notice | `notice_rows`, `full_transcript.rs:2247` | xfx's words, some with outside text (a provider's error, a path), through `bridge.rs:518` | **open where a producer carries outside text**: marks raw; MCP and skill names are spelled before (`crate::mcp::terminal_safe`, `src/mcp/mod.rs:100`) |
| records (`* session:`, `* network:`, `! recovery:`) | `notice_rows` | `session_record`/`network_record` (`full_transcript.rs:194`, `:260`): fixed words, the configured model; a recovery's diagnostic through `bridge.rs:549` | xfx's words safe; **a diagnostic's marks raw** |
| question resolution (`Body::Block`) | `run_on`, `full_transcript.rs` `content` | the model's question, spelled at parse (`crate::tools::question::terminal_safe`, `src/tools/question.rs:57`, through `bounded`, `:172-178`) | safe |
| prompt card | `card`, `full_transcript.rs:2224` | the user's own submission (`shell.rs:4711`) | the user's text, not the model's or a file's; marks raw |
| diff | `diff_rows` | a file, through the permission boundary's `bounded_diff_side` | T-U6 (`feat/p5-transcript-receipts`, not on this base) |
| entry header, group header, usage, footer, banner | `lay_out`, `ToolGroup::expanded`, `line` | xfx's own (stamps, counts, the zone name gated by `local_time`) | safe |

## Ledger (`scripts/fxcap/parity-ledger.toml`)

- `ctrl-o-command-marks` (divergence, `unsafe-to-copy`, pin v0.0.16,
  `p5-transcript-safe` `full*`): fx's raw marks on the command row against
  xfx's named ones.
- `ctrl-o-call-row-spelling` (cause, defect): the call's own row, open.
- `safe-page-edge` (cause, defect): `transcript-page-offset` for this
  scenario, four rows deep -- `full` rows 1-2, `full-pgup` rows 33-36.

## Files

`fx/`, `red/`, `xfx/` the captures (redacted, `scripts/fxcap/redact.py`);
`pairs.txt` every snap pair's classification, both builds; `sweep-base.md`,
`sweep-new.md` the sweeps' summaries; `red-tests.txt`; `check.txt`;
`mutants.txt`; `sweep-probe.txt`; `gates.txt`. The scenario:
`scripts/fxcap/scenarios/p5-transcript-safe.cap`, its turns and
`p5-transcript-safe-ws/safe.txt` (the characters, CRLF, and `over\rX`).
In these text files a mark or control is written `<U+XXXX>`; the captures
hold them as the screens did.
