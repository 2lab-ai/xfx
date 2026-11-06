---
title: MCP-S receipt -- fx v0.0.16's runtime status
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, mcp, receipt]
---

# MCP-S receipt -- retrying before failed, the status line's MCP text, in-place reload, the stderr tail

Unit MCP-S of `.prd/tui-phase5/ssot.md` (research branch), epic 2 (MCP v2);
its spec is `.prd/tui-phase5/research/mcp.md` §5 unit 4 -- the status words,
C2, C3, C7, A13. Security-sensitive (S): a child process's stderr reaches the
screen. Base `feat/p5-fxcap-h1` `d5425d93` (main `93f516b5` + CI-SWEEP + H1),
branch `feat/p5-mcp-status`. Oracle fx v0.0.16 (`481104fd`, executable sha256
`8e6cda2efddb…`, checked by capture.sh before and after every fx run). xfx:
the head build (`target/debug/xfx`, sha256 `a97a7efe6533…`, the source of
`4227a0b3`) and the base, built from `git archive d5425d93` (sha256
`378590ed10ae…`). Every capture is `scripts/fxcap/capture.sh <bin>
<scenario> 40x120`, redacted (`<scenarios>`, `<bun>`).

## What changed

| | fx v0.0.16 | xfx base | xfx head |
|---|---|---|---|
| a server that fails to start | `retrying`, tried again after 1, 2, 4, 8, 16, 30, 30 s, `failed` after the 8th failure in a row (`src/mcp/core/engine.zig:24-28`, `:193-216`; `src/mcp/core/request.zig:76-79`) | one restart (`restart_limit`), then `Failed` | fx's: `Connection::Retrying`, `Retry::FX`; a sign-in, an unset header variable or an unsupported transport fails at once (fx holds those back) |
| why it is down | `its process exited with code 3; its last stderr lines: "boom-stderr"` (`src/core/mcp_host/runtime.zig:2277-2308`; `src/mcp/io/process.zig:188-190`, `:721-756`) | `McpInitFailed` | fx's words, `xfx` for `fx`; each stderr line through `terminal_safe` |
| the status line's right slot | `X is waiting for approval · /mcp`, `X failed · /mcp`, `N MCP need attention · /mcp`, `MCP config has N problem(s) · /mcp list`; silent while servers started < 10 s ago still come up; a prompt sent leaves what was seen to `/mcp` (`src/core/mcp_host/menu.zig:443-557`; `src/core/app/app_mcp_runtime.zig:136-197`; `input_submit_runtime.zig:1108-1109`) | nothing | fx's (`src/tui/statusline.rs` `McpStatus`), last after the Escape cues and the full-access warning (`input_presentation.zig:539-550`) |
| the launch notice | none | `* mcp: MCP startup: 1 server needs attention, 1 failed. Use /mcp list for details.` | none |
| a reload | in place: a same-config server keeps running whatever its state, a changed one starts fresh; nothing said; one warning when the config cannot be read (`runtime.zig:628-642`; `src/mcp/engine.zig:167-186`; `app_commands.zig:340-345`) | waits for every new connection, then `MCP configuration reloaded …` | fx's |

## Pairs

`python3 -I .prd/tui-phase4/receipts/m1/cells.py fx/<f>.ansi xfx-*/<f>.ansi`
(row 1, the banner, not compared). "Status row" is the band's last row, byte
for byte.

| scenario | snap | base vs fx | head vs fx |
|---|---|---|---|
| `p5-mcp-failed` (new, this unit's) | `retrying` (13 s) | 3 rows: `* mcp: MCP startup: …` | **0 rows** |
| | `still-retrying` (84 s) | 3 rows | **0 rows** |
| | `failed` (100 s) | 3 rows; status row differs | **0 rows**: `bad failed · /mcp` |
| | `after-prompt` | 6 rows | **0 rows**: cleared |
| `p5-mcp-fail` | `startup` (13 s) | 3 rows | **0 rows** |
| `p5-mcp-dismiss` | `settled` (13 s) | 6 rows; status row differs | 6 rows, **status row identical** (`proj is waiting for approval · /mcp`) |
| | `after-prompt` | 13 rows | 13 rows, **status row identical** (cleared) |
| `p5-mcp-menu-new` | `startup` (4 s) | 7 rows | 6 rows, **status row identical** (silent: `remote` still retrying) |
| | `startup-settled` (12 s) | 7 rows; status row differs | 6 rows, **status row identical** (`proj is waiting for approval · /mcp`) |

The rows the head still differs on in `dismiss` and `menu-new` are all the
launch trust prompt's: xfx writes `Project MCP server 'proj' is defined in
.mcp.json.`, the command rows and `[1] approve …` into the transcript, which
moves the composer and the band down four rows, and Escape in `dismiss` writes
`* mcp: Project MCP approval prompts dismissed for this process.` That prompt
is C1, MCP-M's (wave 3), and this unit leaves it as it is. The rest of
`p5-mcp-fail` (`list`, `server-*`) is the `/mcp` menu (MCP-M) and `show-bad`
is `/mcp show` (MCP-V); the head's menu already shows C7's words on the bad
server's details, `Error  its process exited with code 3; its last stderr
lines: "boom-stderr"`, byte for byte fx's `fail-server-1` `error` value.

`p5-mcp-failed` exists because `dismiss` and `menu-new` cannot pass with C1
still in place: a profile with `fixture` and `bad` (`p5-mcp-fail-profile.json`),
no project server, no menu, so every row is this unit's. fx turns `bad` to
`failed` between 90 and 92 s (below); the snaps sit 6 s before and 8 s after.

## The research's Unknowns, measured on fx v0.0.16

Taken with throwaway fx-only scenarios (not committed: one runs six and a half
minutes); the snaps are in `measure/`.

1. **A refused loopback HTTP server does reach `failed`, after minutes.**
   Profile `{"mcpServers":{"remote":{"type":"http","url":"http://127.0.0.1:9/mcp"}}}`,
   `/mcp list` every ~4 s from launch (`measure/fx-p5-mcp-measure-refused2-40x120-early.full.txt`):
   `connecting` ×5 (to ~20 s), `retrying`, `connecting` ×2, `retrying`,
   `connecting` ×3, `retrying` ×2, `connecting` ×2 -- each try lasts 10-20 s,
   not the instant a refused connect takes. It still said `retrying` at 110 s
   (an earlier run) and `remote failed · /mcp` at 270 s
   (`…-refused2-40x120-t270.txt`, `…-end.full.txt`: `remote … http failed`).
   xfx's HTTP try against a refused port ends at once, so xfx says `remote
   failed · /mcp` about 91 s after launch: recorded in `docs/parity.md`
   (`MCP client runtime`) as a transport-timing difference.
2. **`/mcp show bad` printed nothing because it waits through the retries.**
   With `p5-mcp-fail-profile.json`, the status line was empty at 84, 88 and
   90 s and said `bad failed · /mcp` at 92 s (`measure/fx-p5-mcp-measure-failed-40x120-t8*.txt`,
   `-t9*.txt`); a `/mcp show bad` typed at ~101 s answered at once
   (`…-failed-40x120-end.full.txt`): `status failed`, `command /bin/sh -c
   'echo boom-stderr >&2; exit 3'`, `error its process exited with code 3;
   its last stderr lines: "boom-stderr"`. In the research's run it was typed
   at ~25 s, while `bad` was still retrying, and the run ended before the
   eighth failure.

## Tests

| test | what it pins |
|---|---|
| `mcp::tests::a_server_that_cannot_start_is_retrying_between_its_tries_and_failed_after_the_last` | `Retrying` with fx's words between tries, `Failed` after the last, the tries counted, no notice |
| `mcp::tests::fx_tries_a_server_eight_times_waiting_one_two_four_eight_sixteen_thirty_and_thirty_seconds` | `Retry::FX`'s waits (91 s) and eight starts |
| `mcp::tests::a_reload_restarts_only_the_server_whose_config_changed_and_says_nothing` | the same process kept, the changed one restarted, no notice, the clock restarted |
| `mcp::tests::a_failed_server_whose_config_is_the_same_stays_failed_across_a_reload` | a reload is not a retry |
| `mcp::tests::a_config_that_cannot_be_read_again_is_one_warning_and_the_servers_stay` | fx's one warning |
| `mcp::tests::a_server_waiting_out_its_backoff_is_stopped_at_once_by_a_reload_or_the_end` | a 1-30 s wait never outlives a reload or `/quit` |
| `mcp::tests::an_http_server_nothing_listens_on_fails_in_fxs_words_with_upstreams_counts` | `it refused or never answered xfx's initialize request` |
| `mcp::runtime::tests::a_failure_quotes_the_last_three_stderr_lines_joined_and_cut_from_the_front`, `…what_a_server_writes_to_stderr_reaches_the_screen_spelled_never_obeyed`, `…a_failure_is_said_in_fxs_words_with_how_the_process_ended` | the tail's shape, its inertness (ESC, BEL, U+202E spelled), the words table |
| `tui::statusline::tests::the_mcp_text_*`, `…a_name_from_a_repository_is_spelled_and_the_text_stops_at_120_bytes`, `…a_prompt_sent_*` | the words, the settle, the seen set, the 250 ms rebuild, the 120-byte cap, the name spelled |
| `tui::shell::tests::the_mcp_text_is_the_right_slots_last_word_in_the_dim_grey_at_the_last_column` | the slot's place and colour, the Escape cue over it, a notice beside it |
| `tests/mcp.rs::a_project_server_asks_first_and_three_rejects_it_into_settings` | the real binary says `proj is waiting for approval · /mcp` |

Changed because the reload's sentence is gone: `mcp::command::tests::reload_says_it_started_and_the_reload_itself_says_nothing`,
`mcp::tests::removing_a_server_stops_its_process_and_the_reload_says_nothing_of_its_own`,
`mcp::tests::a_project_server_waits_for_approval_and_the_choice_is_kept_in_settings`,
the shell's trust-prompt cases (`until_prompted` reads `McpRuntime::reloading`),
and `tests/mcp.rs`'s two prompt cases.

## Gates

At `4227a0b3` (the receipt commit changes no code):

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| `cargo clippy --locked --all-targets --features fault-injection -- -D warnings` | ok |
| `cargo test --locked --no-fail-fast` | 4559 passed, 0 failed, 7 ignored |
| `cargo test --locked --features fault-injection --no-fail-fast` | 4580 passed, 0 failed, 7 ignored |
| `scripts/check-xfx-identity.sh`, `check-no-stubs.sh`, `check-no-secrets.sh` | ok, ok, ok |
