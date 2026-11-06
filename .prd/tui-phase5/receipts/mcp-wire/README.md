---
title: 0.0.16 delta -- MCP-W receipt (what the model is told, the MCP child process)
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, mcp]
---

# MCP-W: what the model is told, and the stdio child

Spec: `research/tui-phase5:.prd/tui-phase5/research/mcp.md` §5 unit 5 (D1-D5,
E2; E3/E4 are a user decision). Oracle: fx v0.0.16, sha256 `8e6cda2e...`,
captured with `scripts/fxcap/capture.sh` at 40x120. Base: `d5425d93`.

## Request bodies (`bodies_compare.py`)

Every completion request, fx's and xfx's paired in order (title requests set
apart). For each pair the comparator checks, byte for byte as JSON and with
nothing normalized: the MCP tools advertised (the meta-tools and every
`mcp_*` tool, whole), the `<mcp_servers>` system block, and every tool call
and tool result in the prompt. The two advertisement differences M1 recorded
stay and are reported by key: `capability_search`'s description (no skills
in xfx) and `mcp_select_tool`'s closed schema.

| scenario | base `d5425d93` | this branch |
|---|---|---|
| `p5-mcp-turn` | `red-turn-bodies.txt`: `mcp_features` advertised (D1); `tools="1"` in every request (D3); both echo results lack `"isError":false` (D4) | `turn-bodies.txt`: identical, 4 of 4 requests |
| `p5-mcp-direct` | `red-direct-bodies.txt`: one request, not two -- the call to an unselected tool ended the turn (D2) | `direct-bodies.txt`: identical, 2 of 2; the echo schema is advertised in the second |
| `p5-mcp-known` (new) | -- | `known-bodies.txt`: identical, 3 of 3; no `tools=` in turn one, `tools="1"` in turn two (D3, the positive half) |
| `p5-mcp-unavailable` (new) | -- | `unavailable-bodies.txt`: the `mcp_unavailable` answer has fx's shape and place (D5); two differences, both the runtime's words, not this unit's: `bad`'s reason (fx `its process exited with code 3; its last stderr lines: &quot;boom-stderr&quot;`, xfx `McpInitFailed`, C7) and its block state (fx `discovering`, xfx `failed`) -- MCP-S |

## Wire

`wire-new-frames.txt` (`p5-mcp-wire-new`, the fixture's log of what each
client sent): unchanged by this unit. It differs in the four places of the
open E3/E4 question -- `server/discover` with `2026-07-28` first, ids from
1, `_meta.progressToken` on `initialize` and `tools/list`, `"params":{}` on
`notifications/initialized` -- plus M1's two approved ones (`clientInfo`,
and `capabilities` `{}`). `docs/parity.md`'s MCP client runtime row holds the
question.

`wire-old-files.txt` (`p5-mcp-wire-old`, the log path only in the process
environment): neither fx v0.0.16 nor xfx wrote the log, because neither
passes its own environment on beyond `HOME LOGNAME PATH SHELL TERM USER` (E2,
already on `main` since `3ed37c8c`).

## Screens

`screens.txt` and the `*-turn.txt` pairs: `turn`, `direct` and `known`
differ in row 1 (the banner) and row 5 only, `N MCP call(s)` against
`N command(s)` (C6, MCP-T). `scripts/fxcap/cells.py` also finds the
`Ran MCP ...` rows' `MCP` at `38;5;245` on fx and `252` on xfx: v0.0.16
gives an unregistered `mcp_` name the activity kind `.mcp` rather than
`.command` (`src/core/tooling/tool_dispatch.zig:574-578@481104fd`), so its
row is no longer shell-highlighted -- C6 too. `unavailable` also shows xfx's
`MCP startup:` notice, which v0.0.16 dropped (C1, MCP-S/MCP-M).

## Unit tests

- `mcp::tests::an_unselected_call_meets_every_admission_rule_a_selected_one_does`:
  3 modes x 8 rule tables x 4 answers, each call once selected and once
  unselected through the machine's two steps; the same questions, results
  and `tools/call` frames at the server.
- `mcp::tests::the_servers_block_counts_a_servers_tools_only_once_the_model_has_read_them`,
  `mcp::tests::an_open_search_that_finds_nothing_names_a_server_it_could_not_look_in`,
  `mcp::catalog::tests::a_call_answer_is_v0016s_content_structured_content_and_is_error`,
  `mcp::catalog::tests::an_open_search_that_found_nothing_names_what_it_could_not_look_in`,
  `tests/mcp.rs` `a_tool_the_model_calls_by_name_runs_without_a_search_and_its_count_is_known_next_turn`.
- E2: `mcp::transport::tests::a_server_is_given_six_of_xfxs_variables_then_its_own_env`
  and `a_server_and_its_docker_cleanup_see_only_what_they_are_given` (a
  child started with fake `AI_GATEWAY_API_KEY`, `VERCEL_OIDC_TOKEN` and
  `GITHUB_TOKEN` in its parent's environment sees none of them).

Every capture-derived file here went through `scripts/fxcap/redact.py`.
