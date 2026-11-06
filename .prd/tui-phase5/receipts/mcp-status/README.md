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

## MCP-Sb (review round, `c729f5fd`)

Two engines reviewed `af2c3fbc`; their findings and what was done:

| finding | fix | RED on `af2c3fbc` | pin |
|---|---|---|---|
| a try's connection was local to `open()`, so a reload that dropped or replaced a server mid-handshake, or the session's end, left its process running until its startup timeout -- beside its replacement | the connection is the server's as soon as it starts (`Live::starting`); `stop()` closes it and waits out a try handing its process over (`Live::opening`, ≤ 3.5 s); the stopped check and the publish share the lock a stop takes -- fx's `Engine.stopping` / `Engine.deinit` (`src/mcp/engine.zig:145-161`, `:194-199@481104fd`) | `the old server outlived the reload that dropped it`; `removed, still running` | `a_server_held_mid_handshake_is_gone_before_its_replacement_starts` (the replacement itself reports `gone`), `…_when_a_reload_removes_it_or_the_session_ends` (pid and group) |
| a resource or prompt read during a startup backoff answered from default capabilities | `feature_server` waits through `Retrying` as through `Connecting` | `Named("McpResourcesUnsupported")` | `a_feature_request_during_a_startup_backoff_waits_for_the_next_try` |
| `/mcp reload` says `MCP reconnection started.` and kept a failed server failed | fx v0.0.16 has no reload verb (`verbs.zig:70`); the c1db9191 verb and the menu's `r` now also start a failed server again (`McpRuntime::reconnect`); reloads after add, remove and trust keep fx's config-only rule | -- | `an_explicit_reload_starts_a_failed_server_again_and_a_config_reload_does_not` |
| `worth_retrying` unpinned | -- | -- | `a_server_that_answers_401_is_asked_once_and_needs_a_login` |
| `exit_of`'s `NOWAIT` unpinned | -- | removing `NOWAIT` fails it | `a_servers_exit_is_read_without_collecting_it_and_close_collects_it_and_its_group` |
| per-line cut indistinguishable from the whole cut | -- | removing the per-line cut fails it | `HEAD…TAIL` sentinel in `a_failure_quotes_the_last_three_stderr_lines_…`; `a_servers_hostile_stderr_reaches_capability_search_spelled_and_bounded` (a real child) |
| stderr now reaches the model | recorded in `docs/parity.md` (`MCP client runtime`), with fx's same path: `unavailableText`, `src/core/mcp_host/model.zig:705-711`; `lastError`, `runtime.zig:1281`, `:2020-2036@481104fd` | -- | the `capability_search` case above |

Re-taken at `c729f5fd`: `p5-mcp-failed` ×4 and `p5-mcp-fail startup` still 0
rows from fx; `dismiss` ×2 status row still identical.

Gates at `c729f5fd`: fmt, both clippy runs ok; `cargo test` 4566 passed / 0
failed / 7 ignored, with `fault-injection` 4587 / 0 / 7; identity, no-stubs,
no-secrets ok; release and faulty release builds ok; `scripts/smoke.sh` 46
checks, 0 failures; `scripts/smoke-tui.sh` (full, `--faulty`) 37 scenarios +
the oracle, 1091 checks, 0 failures.

## MCP-Sc (review round, `3c89856e`)

| finding | fix | RED on `b31bd061` | pin |
|---|---|---|---|
| a failed try cleared `opening` before its handshake and took its process back before its stderr settle and close: a stop in that teardown found nothing, so a reload started the replacement beside the old child and `/quit` returned with it alive | review option (i): a try holds `opening` to its end, reads stderr and exit while the connection is still the server's (a stop closes it itself), and takes it back only to close it; `stop()` waits up to 20 s (a group's grace, a docker removal) -- one rule for every step of a try, rather than re-arming the flag on one path | `the reload returned with the old child alive`; `the session ended with the child alive` (startup timeout 200 ms, `trap '' TERM`, one try, the stop 350 ms after the pid) | `a_reload_during_a_failed_handshakes_teardown_returns_only_once_the_child_is_gone`, `the_end_of_a_session_during_a_failed_handshakes_teardown_waits_for_the_child` |
| a second try could overwrite the one `opening`/`starting` slot | `Opening::begin` refuses while a try holds the server; a call's restart is checked and counted under one lock | -- | -- |
| `shutdown()` did not wait for a reload under way | `closed` set first; a bounded wait on `reloading`; no reload starts a server after it | -- | -- |
| `reconnect` read the raw state, not the one every surface shows | `Server::observed`: a ready server whose process died is failed and `/mcp reload` starts it again | -- | `an_explicit_reload_starts_again_a_ready_server_whose_process_died` |
| the holding tests could race the startup reload | `start_and_wait` before acting | -- | -- |
| the `opening` wait and `hand_over`'s check unpinned | a `Hold` hook compiled into fault-injection tests only | removing `hand_over`'s stopped check fails `…held_before_its_hand_over…`; releasing `opening` before the handshake fails `…a_failed_try_closing_its_own_process` | both |
| the prompt read during the backoff rode on the resource read; the menu's `r` unrun | a prompt read of its own during the backoff; `Job::MenuReload` run | -- | `a_feature_request_during_a_startup_backoff_waits_for_the_next_try`, `an_explicit_reload_starts_a_failed_server_again_…` |

The MCP-W worker's lead -- a stdio server `Connecting` past its startup
timeout, once in ~20 runs of seven servers with one 65 KB env value -- did not
reproduce here in 120 bounded runs: 60 with the answering fixture (each run
checked within 4.5 s of a 2 s timeout) and 60 with servers that never answer
(one try, each checked `Failed` within 3.5 s of a 1 s timeout). Nothing is
committed for it.

Re-taken at `3c89856e`: `p5-mcp-failed` ×4 and `p5-mcp-fail startup` 0 rows
from fx; `dismiss` ×2 status row identical.

Gates at `3c89856e`: fmt, both clippy runs ok; `cargo test` 4569 / 0 / 7
ignored, with `fault-injection` 4592 / 0 / 7; identity, no-stubs, no-secrets
ok; release and faulty release builds ok; `scripts/smoke.sh` 46 checks, 0
failures; `scripts/smoke-tui.sh` (full, `--faulty`) 37 scenarios + the oracle,
1091 checks, 0 failures.

## MCP-Sd (review round, `0f66543e`)

| finding | fix | RED on `e47b9c5c` | pin |
|---|---|---|---|
| MCP-Sc's stop waited for the try, but a close does not end every handshake: an HTTP request in flight keeps its reply after `close`, and a stdio server whose stdout a process outside its group holds gives no end of output -- a reload or `/quit` waited min(startup timeout, 20 s) | each server's `CancelToken`, cancelled first by a stop; `initialize`, `notifications/initialized` (`Transport::notify` takes a cancel) and `tools/list` wait on it | silent HTTP endpoint: reload 5.99 s, shutdown 6.00 s; stdio stdout held by a `setsid` grandchild: 5.97 s | `a_reload_ends_an_http_handshake_against_a_silent_endpoint_at_once`, `the_end_of_a_session_ends_an_http_handshake_…`, `a_stop_ends_a_stdio_handshake_whose_output_a_stray_grandchild_holds` (all < 2 s) |
| `reload_once` put the new set in force before stopping what it dropped: a session end whose wait ran out reached neither | every dropped server marked stopped before any is closed and kept on `State::retiring` until its stop is done; `shutdown` stops that list too; concurrent stops of one server wait for each other's close (`Live::closing`) | -- (mutation: `shutdown` ignoring `retiring` fails the pin) | `the_end_of_a_session_stops_what_a_slow_reload_was_still_retiring` (three TERM-ignoring servers, a 50 ms wait) |
| a restart's credit was spent apart from its try: with `restart_limit >= 2` a second call spent one and got the dead connection | the credit and the try are one step (`Opening::claim`); a call that finds a try under way waits for it and takes what it opened | -- (mutation: no wait fails the pin) | `two_calls_that_find_a_server_gone_spend_one_restart_and_start_one_process` (`Hold::BeforeHandOver`), `one_try_at_a_time_holds_a_server_and_none_once_it_is_stopped` |
| `closed` unpinned | -- | -- (mutation: ignoring `closed` fails both) | `a_reload_after_the_end_of_a_session_starts_nothing`, `a_reload_queued_behind_one_the_end_of_a_session_outwaited_starts_nothing` |
| `exit()` spun after a close took the child; the stop bound was understated | `exit()` returns at once; `STOP_WAIT`'s doc gives the real bound (one stop's own closes, then ≤ 20 s; stops of a reload or a session end run one after another) | -- | -- |

Re-taken at `0f66543e`: `p5-mcp-failed` ×4 and `p5-mcp-fail startup` 0 rows
from fx; `dismiss` ×2 status row identical.

Gates at `0f66543e`: fmt, both clippy runs ok; `cargo test` 4575 / 0 / 7
ignored, with `fault-injection` 4600 / 0 / 7; identity, no-stubs, no-secrets
ok; release and faulty release builds ok; `scripts/smoke.sh` 46 checks, 0
failures; `scripts/smoke-tui.sh` (full, `--faulty`) 37 scenarios + the oracle,
1091 checks, 0 failures.
