# MCP-T receipt -- MCP calls in the tool group

Unit MCP-T of the 0.0.16 delta (research `mcp.md` §5 unit 6: C6, the live
verb, C8 only if captured). Base `feat/p5-fxcap-h1` `d5425d93`. Oracle fx
v0.0.16 (`481104fd`, executable sha256 `8e6cda2efddb…`, checked by capture.sh
on every run). xfx `target/debug/xfx`: base `3db5c1437f5c…`, head
`e46655158d87…` (both read `x𝒇x v0.1.0-d5425d9 [debug]`; the head is the
base plus this unit's tree).

## What changed on the screen

| what | fx 0.0.13 / xfx base | fx 0.0.16 / xfx head | fx source (`@481104fd`) |
|---|---|---|---|
| header count of MCP calls | `2 commands` | `2 MCP calls` (`1 MCP call`), after `command` in a tie | `src/ui/transcript/tool_group_projection.zig:143-156`, `:542` |
| an MCP row's colour | `MCP` lifted to 252 as a shell word | the row's grey, 245, throughout | `tool_group_projection.zig:749` (`highlightCommandPhrase` only for `.command`) |
| a failed MCP call | `Failed MCP mcp_web_echo` (no reason: a command's row) | `Failed mcp_web_echo: <reason>` | same as the row's colour; `app_agent_runtime.zig:1376-1381` |
| a cancelled MCP call | xfx: `Cancelled MCP mcp_web_echo` | `Cancelled mcp_web_echo` (fx's since c1db9191) | `src/core/app/app_agent_runtime.zig:1376-1381` (`.denied => null`) |
| a refused MCP call | xfx: `Denied MCP …` | `Denied mcp_…` -- **code read, not captured** | `app_agent_runtime.zig:1324-1329` |
| the activity row while it runs | `• Running (Ns)` | unchanged: `• Running (Ns)` | `src/ui/footer/render_input.zig:692-694` |

**The live verb `calling` is not on the screen.** `activity_status.zig:102`
(`.mcp => "calling"`) and `:244` (`N MCP calls made`) exist, but the footer
builds its row with the call's kind taken off (`render_input.zig:692-694`),
and the one label that keeps it is read only to decide whether to animate
(`app_worker_runtime.zig:856`, `_ = activityShimmerLabel(…)`). The capture
agrees: `fx-p5-mcp-live-40x120-live` row 9 reads `Running (2s) (↑5 ↓80)`
while `tools/call` id 4 is held (`fx-p5-mcp-live-40x120-mcphttp.log`: asked at
+7202 ms, answered at +10221 ms). So `src/tui/activity.rs` changes only by a
doc note. C8 (progress notes) is in no capture and is not built.

How an MCP call is told apart: `Call::calls_mcp` in `src/tui/toolgroup.rs`
is "its presentation is `crate::tools::mcp::DYNAMIC`", the one every such call
is shown with. `Activity` stays `Command` for it, so a stop still reaches and
settles it as before (`ToolGroup::running_command`).

**Server text on the row.** A failed MCP call's row now carries its reason,
which can be the server's own words (an `isError` result). It is spelled as
fx's `encodeTerminalSafeInline` spells it (`text_utils.zig:397-417`): ASCII
white space flattened, then `crate::mcp::terminal_safe` -- controls and
invisible or bidirectional code points written out (`\x1b`, `\u{202e}`), at
most 256 bytes. The tool's name goes through the same helper. Test
`a_servers_words_in_a_failed_mcp_calls_row_are_written_out_not_obeyed`.

## Scenarios

- `p5-mcp-turn` (H1's, unchanged): search, two `mcp_fixture_echo` calls, text.
- `p5-mcp-live` (new; `-profile.json`, `-turns.json`; needs H1's `mcphttp`):
  the run's HTTP MCP server is ready at launch and told to `hang` before each
  prompt, so the turn's `tools/call` is held: `live` (the call running), `turn`
  (settled), `full` (Ctrl-O); a second turn stopped with Ctrl-C while held
  (`cancel`, `end`); a third with the server in `rpcerror` (`failed`).

## RED (xfx base, `red/`)

`python3 -I scripts/fxcap/cells.py cap/fx-… red/xfx-…`:

| pair | differing rows |
|---|---|
| `p5-mcp-turn/turn` | 5 (`2 commands`), 7-8 (`MCP` in 252) |
| `p5-mcp-live/live` | 5 (`1 command`), 7 (`MCP` in 252), 9 (the clock, normalized) |
| `p5-mcp-live/turn` | 5, 7 |
| `p5-mcp-live/cancel` | 5, 7, 15, 17 (`Cancelled MCP mcp_web_echo` in 252), 19 (glyph, U7) |
| `p5-mcp-live/failed` | the five above, 23 (`1 command`), 25 (`Failed MCP mcp_web_echo`, no reason) |

The head's unit tests with `calls_mcp` forced false: 4 of the MCP tests fail,
e.g. `left: ["● 1 tool call · 1 command", …] right: ["● 1 tool call · 1 MCP call", …]`.

## GREEN (xfx head, `cap/`)

| pair | raw `cells.py` | under the ledger's normalizations |
|---|---|---|
| `p5-mcp-turn/turn`, `/end.full` | 0 rows | 0 rows |
| `p5-mcp-live/live` | row 9: the clock (`2s`/`3s`) | 0 rows (`activity-pulse`, `activity-clock`) |
| `p5-mcp-live/turn` | 0 rows | 0 rows |
| `p5-mcp-live/cancel`, `/end` | row 19 col 0: `▪` vs `■` | cause `U7-cancelled-glyph` alone; 0 rows with fx's `▪` read as `■` |
| `p5-mcp-live/failed` | row 19 (glyph), row 25 from col 23: the reason | causes `U7-cancelled-glyph`, `mcp-call-error-text`; `└ Failed mcp_web_echo: ` the same |
| `p5-mcp-live/full` | 10 rows: 7 entry timestamps, 3 network latencies | cause `U5-transcript-local-time` alone (`KST` vs UTC); every MCP-group row the same |

The reason in `failed` is the tool's result, what the model is told: fx sends
`MCP protocol error -32603: fixture refuses`, xfx its `{server, tool, error}`
envelope (`src/tools/mcp.rs:601-607`, MCP-W's file).

`scripts/fxcap/sweep.sh --only p5-mcp-turn --only p5-mcp-live` against the
head, an independent run: 9 pairs, 5 normalized-identical, 4 unresolved, every
unresolved row covered by one of the three causes (added for `p5-mcp-live`
only).
