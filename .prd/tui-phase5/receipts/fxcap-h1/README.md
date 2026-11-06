# H1 receipt -- the capture verbs the 0.0.16 delta needs

Unit H1 of `.prd/tui-phase5/ssot.md` (research branch), Wave 0: harness files
only (`scripts/fxcap/`), no `src/` change. Base `ci/p5-sweep` `f422fc1d`.
Oracle fx v0.0.16 (`481104fd`, executable sha256 `8e6cda2efddb…`, checked by
capture.sh before and after every fx run); xfx `target/debug/xfx` built from
the base (`x𝒇x v0.1.0-f422fc1`, sha256 `5c092a85df33…`).

## What was already on the base, and what this adds

`resize`, `signal`/`jobctl`, `rehomefile`, `clock`, `usage`, `models2`,
`modelsdelay`, `mcphttp`, `sockets`, fakegw's line per request (unknown GETs
included), `reset`/`refuse` turns and the usage routes were on the base
(`32517e22`), with scenarios `h1-*.cap` and no tests. This unit adds:

- `@SCENARIOS@` (the scenario directory the run reads, resolved) and `@BUN@`
  (the bun that runs fakegw.ts) beside `@PORT@`, `@HOME@`, `@MCPPORT@`; all
  five written into seeded files, `work`/`dir` copies (text files only), `env`
  values and `type`/`paste`/`relaunch` text. A path outside `A-Za-z0-9._/+-`
  is refused, exit 2, only when the scenario names it.
- `mcphttp methodnotfound`: `server/discover` gets HTTP 200 and JSON-RPC
  -32601, which fx reads as a legacy answer (vercel-labs/fx@481104fd
  src/mcp/core/era.zig:387-402; a bare 404 is `classifyDiscoverHttp`,
  :408-413). Scenario `h1-mcphttp-methodnotfound.cap`.
- `scenarios/p5-mcp-stdio.ts`: this harness's stdio MCP server, written as
  mcphttp.ts is. It answers like the default mode of fx's
  tests/e2e/fixtures/mcp-legacy-stdio.mjs@481104fd and copies none of its
  source. Its wire log has no pid.
- The ten p5-mcp scenarios CI-SWEEP left out, rewritten to `@BUN@
  @SCENARIOS@/p5-mcp-stdio.ts`, with xfx given the same profile.
- ci_sweep.py follows `@SCENARIOS@/<path>` one level when it selects the
  scenarios a change touches.
- `test-capture.sh` / `test_capture.py`, a test per verb.

## Tests

| test | head | base `f422fc1d` |
|---|---|---|
| `scripts/fxcap/test-capture.sh` (28 tests: capture.sh over a link to /bin/bash or /bin/cat on its own tmux socket; fakegw, mcphttp and the stdio server on 127.0.0.1) | OK | 19 failures, 4 errors: the placeholders, `methodnotfound`, the stdio server, the new refusals. The base's own verbs pass. |
| `scripts/fxcap/test-sweep.sh` (with the new `test_a_file_named_through_the_scenario_directory_selects_its_users`) | OK | the new test fails: `'viaprofile\tuses server.mjs\n' not found` |

Each verb and its test:

| verb | test |
|---|---|
| `resize` | `Verbs.test_resize`: `stty size` reads `24 80`, then `20 90` |
| `signal`, `jobctl` | `test_signal_stop_continue_and_end_under_jobctl` (T+, S+, pane dead by HUP), `test_a_stop_is_discarded_without_jobctl_and_a_term_ends_it`, `test_a_pid_whose_executable_is_not_the_binary_is_refused_and_the_guards_still_run` |
| `rehomefile` | `test_rehomefile`: written mid-run, `@PORT@` filled, 700/700/600 |
| `clock` | `test_clock`: the env var names the file, each line rewrites it, 600; xfx gets nothing |
| `usage` | `test_usage_is_in_place_at_launch_and_swapped_on_cue`; `FakeGateway.test_the_usage_routes_their_shapes_and_knobs` (7 paths, byRange/byStart/rows, status, body, delayMs, unknown without the file) |
| `models2`, `modelsdelay` | `test_models2_and_modelsdelay`; `FakeGateway.test_models2_swaps_the_catalog_with_the_times_logged` |
| fakegw request log | `test_every_request_is_a_line_unknown_gets_too_and_never_a_header_value` |
| reset / refused turn | `test_a_reset_turn_cuts_the_stream_and_the_port_listens_again`, `test_a_refused_turn_cuts_then_refuses_then_listens` |
| `mcphttp` (+ `methodnotfound`) | `test_mcphttp_on_cue`; `McpHttp.*` (ready, methodnotfound, hang, fail, rpcerror) |
| `sockets` | `test_sockets` |
| placeholders | `Placeholders.*`; `Refusals.test_a_scenario_directory_no_placeholder_can_name`, `test_mcpport_without_an_mcp_server`, `test_such_a_directory_still_runs_a_scenario_that_names_no_placeholder` |
| argument refusals | `Refusals.test_each_verbs_arguments` (18 cases) |
| stdio server | `McpStdio.*` |

## Unchanged for a scenario that names no new placeholder

startup, composer-wrap, question, model-column and settings-scrollback, on fx
v0.0.16 and on xfx, at 40x120. Two runs with the base's capture.sh and one
with this head's (`1011b405`): 136 snap files (`.txt` and `.ansi`). 128 are
the same bytes in all three runs. The other 8 already differ between the two
base runs, and the head's run differs from the base runs in the same places
only: `composer-wrap` `badge-71`/`badge-111` `.ansi` (the run's `fxcap.XXXXXX`
name inside an OSC 8 link, both binaries), and xfx's `question` `answered`
(`.txt`, `.ansi`, `.full.*`: an elapsed `5s`/`6s`). 0 regressions.

## Self-test: each verb as a pair, fx v0.0.16 beside xfx

`sweep.sh --only 'h1-*' --only 'p5-mcp-*' --jobs 3`: 26 scenarios, 52 runs,
ran 26, failed 0, timeout 0. Pass = 13 / 159 pairs, because xfx does not have
the 0.0.16 screens yet; those pairs are the RED for the later units. What
each verb did, on each side, from the capture logs:

| scenario | verb | fx v0.0.16 | xfx |
|---|---|---|---|
| `h1-resize.cap` | resize | `the pane is now 20x60`, then `40x120` | the same |
| `h1-signal.cap` | jobctl + signal | TSTP `T+`, CONT `S+`, TERM: pane dead, `term` | the same |
| `h1-signal-hup.cap` | signal | HUP: pane dead, `hup` | the same |
| `h1-rehomefile.cap` | rehomefile | `~/.fx/settings.json: mode 600, 29 bytes` | `~/.xfx/settings.json: mode 600, 29 bytes` |
| `h1-models2.cap` | models2 | asked at 832 ms (first catalog) and 12913 ms (`-> second catalog`) | asked once, at 6179 ms (first catalog) |
| `h1-refuse.cap`, `h1-refuse-pause.cap` | refuse turn | `refusing connections for 10000ms` / `15000ms`, then `accepting again` | the same |
| `h1-reset.cap` | reset turn | `reset after 1 event(s)`, `accepting again` | the same |
| `h1-usage.cap` | usage + clock | one unknown `GET /v1/report` 404, 3 × `200 rows=1 held 2500ms`, one 429 | no usage read (xfx reads none) |
| `h1-usage-codex.cap`, `h1-usage-grok.cap` | usage | the three wham paths 200; `/billing`, `/settings` 200 | no usage read |
| `h1-mcphttp.cap` | mcphttp | discover 404, initialize, initialized, tools/list | initialize, initialized, tools/list |
| `h1-mcphttp-fail.cap` | mcphttp | 500s in `fail`, then ready | fail, rpcerror, then ready |
| `h1-mcphttp-methodnotfound.cap` | methodnotfound | discover 200 (-32601), initialize, tools/list; `remote … ready, 1 tool` | initialize, tools/list; `remote Ready` |
| `h1-guards.cap` | sockets | 1, then 2 loopback sockets, no breach | 0, then 1 |

Every fx run printed its `guards:` line (sha256 `8e6cda2efddb` unchanged,
chime names still directories).

## The ten p5-mcp scenarios

`p5-mcp-direct`, `-dismiss`, `-fail`, `-menu-new`, `-menu-old`, `-notes`,
`-turn`, `-verbs`, `-wire-new` and `-wire-old` (`.cap` each) ran on both
binaries, with no local path in any input. The stdio server came up on both:
fx `fixture … ready, 1 tool`, xfx `fixture Ready stdio · Profile`, and both
ran `mcp_fixture_echo` twice in `p5-mcp-turn`.

The research captured eight of the ten on fx v0.0.16, with fx's own fixture
at a local path. Its 80 snaps were compared with this run's fx snaps of the
same labels, leaving out the rows that print the server's path. 79 are the
same. The other one, `menu-new` `approve-confirm`, differs only because the
shorter path fits on one row. `p5-mcp-wire-new` logs the same wire frames as
the research run, apart from the pid. One of the ten's 97 pairs passes,
`p5-mcp-turn` `ready`; the rest are the MCP units' RED.
