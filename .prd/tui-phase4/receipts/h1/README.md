# H1 receipt: capture-harness verbs for Phase 4 QA and Phase 5

H1 adds to `scripts/fxcap/capture.sh` and `fakegw.ts` the screens the
Phase 5 research (`research/tui-phase5`, each doc's `## Harness needs`)
could not reach: a resize, real job-control signals, a HOME file replaced
mid-run, a request log with times, a late and a swapped catalog, a cut and a
refused connection, fx v0.0.16's usage reads with a pinned clock, and an HTTP
MCP server that is ready or fails on cue. No product code changed.

Every scenario is `scripts/fxcap/scenarios/h1-*.cap`, taken on three
binaries, each into a directory of its own:

```
TMPDIR=<tmp> FX_BIN=<refs>/fx-c1db919/bin/fx scripts/fxcap/capture.sh fx  <scenario> <size> fx-c1db919/
TMPDIR=<tmp> FX_BIN=<refs>/fx-v0.0.16/bin/fx scripts/fxcap/capture.sh fx  <scenario> <size> fx-v0.0.16/
TMPDIR=<tmp>                                  scripts/fxcap/capture.sh xfx <scenario> <size> xfx/
```

`fx-c1db919` is sha256 `7b76319a…` (banner `v0.0.13`), `fx-v0.0.16` is
`8e6cda2e…` (stamp `481104fdbc24`). The xfx binary is `target/debug/xfx`
built at `72bf024`, this branch with `plan/tui-phase4` `bcda7a5d` merged, as
its banner says. Taken 2026-11-03 with the harness of that commit. Each
directory's `runs.log` is every run's own output, in order: the files it
wrote, the lines the new verbs print (`resize:`, `signal …`, `rehomefile …`,
`… socket(s)`), fx's `guards:` line, and the exit status.

Sanitized after the runs, the same way in every text file (`.txt`, `.ansi`,
`.log`, `.files`, `.sockets`, request bodies): each run directory is written
`<scratch>`, the reference binaries' directory `<refs>`, this checkout
`<repo>`, the scratch TMPDIR `<tmp>`. A row that named one of those is shorter
than it was on screen. `.wire` files are left as written. `.ansi` files are
tmux's re-serialization of cell attributes (`capture-pane -e`), not the wire.

## What each verb and route is proven by

Paths are relative to this directory; `<b>` is `fx` or `xfx`. "All three"
means the same lines in `fx-c1db919/`, `fx-v0.0.16/` and `xfx/`.

| verb or route | scenario | proof |
|---|---|---|
| `resize <rows>x<cols>` | `h1-resize` (30x100) | `runs.log`: `resize: the pane is now 20x60`, then `40x120`, on all three; the harness fails the run if tmux's pane is not that size. SIGWINCH reached each binary: the composer draft is wrapped again at each width -- 2 rows at 100 columns, 3 at 60 (`…-small.txt` rows 3-5 fx, 16-18 xfx), 2 at 120 with `eighteen` moved up a row (`…-large.txt`) -- a repaint none of them draws without asking the terminal its size again. |
| `jobctl` | `h1-signal` (24x80) | Ctrl-Z is fx's own suspend, and under `jobctl` its `raise(SIGTSTP)` really stops it. On both fx, `…-ctrlz.wire`, taken a second after the key, ends with the restore (`…\e[?2004l\e[<u\e[>4;0m…\e[3;1H\e[J\e[?25h\r\n`; v0.0.16 with `\e]7501;state=clear\e\\` before `\e[<u`) and nothing after it -- where the raise is discarded, the resume follows at once (`.prd/tui-phase5/research/lifecycle.md:130` on `research/tui-phase5`, its `suspend/w01`). The resume (`\e[>4;2m\e[>1u\e[?2004h\e[?7l…`) and a full repaint come only after `signal SIGCONT` (`…-stopped.wire` past the end of `…-ctrlz.wire`). |
| `signal SIGTSTP` / `SIGCONT` / `SIGTERM` | `h1-signal` | `runs.log` on all three: `signal SIGTSTP -> pid N: T+` (stopped), `signal SIGCONT -> pid N: S+`, `signal SIGTERM -> pid N: gone; pane dead, signal term` -- tmux's own dead status for the pane, so the signal ended the binary and not a wrapper (jobctl.pl dies by the same signal). `…-typed.txt` shows input reaching the binary after the second continue. |
| `signal SIGHUP` (no `jobctl`) | `h1-signal-hup` (24x80) | `runs.log` on all three: `signal SIGHUP -> pid N: gone; pane dead, signal hup` -- the pid is the pane's own, checked by lsof to run the binary. `…-end.wire` ends in each binary's abnormal restore; fx v0.0.16's carries `\e]7501;state=clear\e\\`, which the lifecycle research had from source only. |
| `rehomefile <path> <file>` | `h1-rehomefile` (30x100) | `runs.log`: `rehomefile ~/.fx/settings.json: mode 600, 29 bytes` (fx) and `~/.xfx/settings.json` (xfx); `…-swapped.files` shows the profile is now `{"statusLine":{"context":tru`; `…-status.txt` is `/status` drawn after it, the binary still running. |
| request log | every `*-gateway.log` | one `[fakegw] request <METHOD> <path?query> at=<asked>ms -> <status> at=<answered>ms headers=<names>` line per request, after the descriptive lines that were already there. An unknown path: `fx-v0.0.16/fx-h1-usage-30x100-gateway.log` line 4, `GET /v1/report?…&api_key_id=self … -> 404` (no `usage` file yet); `fakegw-selftest.txt`, `GET /.well-known/openid-configuration … -> 404`. Header values never: the self-test sends `authorization: Bearer not-a-real-token`, and the log has `headers=authorization,x-vercel-ai-gateway-team` and no value. |
| `models2 <file> <ms>` | `h1-models2` (30x100) | `fx-v0.0.16/fx-h1-models2-30x100-gateway.log`: `GET /coding-agent/v1/models -> second catalog` on the ask at `at=14558ms` (the swap was at 9000 ms), and `…-after.txt` lists `fake/model-released-later`; the ask at launch got the one-model catalog. fx c1db919 and xfx asked once, at launch, and list one model both times. |
| `modelsdelay <ms>` | -- | D2c's (`plan/tui-phase4` `32259684`), kept as merged, its `at=<ms>ms` catalog lines the format of the request lines here; `models2` decides its catalog after the delay. Its argument is now refused before launch like the others'. |
| `reset` turn | `h1-reset` (30x100) | gateway log on all three: `completion … -> reset`, `reset after 1 event(s)`, `accepting again on <port>`. Both fx: `[Response interrupted. Restarting.]` and the next turn, `the answer after the reset` (`…-after.txt` rows 7, 9; a second `completion … -> text`). xfx: `✗ system: the connection to the Gateway failed: error decoding response body`, and no second request. |
| `refuse <ms>` turn | `h1-refuse`, `h1-refuse-pause` (30x100) | gateway log: `cut, refusing 10000ms`, `refusing connections for 10000ms`, `accepting again`. Both fx: `⚠ Connection lost · waiting for connection · …s · esc to pause` (`…-refused.txt`), then `back after the refusal` from the next turn, asked at `at=23694ms` (c1db919) and `at=18607ms` (v0.0.16). `h1-refuse-pause`: Escape gives fx's `⚠ Connection lost · connection refused · recovery paused after 3 attempts` (c1db919) / `4 attempts` (v0.0.16). xfx: `✗ system: the connection to the Gateway failed: error sending request for url (…)` at once, in both. |
| `usage` + `clock`, `/v1/report` | `h1-usage` (30x100) | fx v0.0.16 gateway log: the three ranges of fx's own e2e test by the pinned clock (`2026-10-12..2026-10-12`, `2026-10-06..2026-10-11`, `2026-10-01..2026-10-11`, lines 6-10), each `held 2500ms` (`…-fetching.txt`: `fetching…`); `…-month.txt`: `today $0.5000 2`, `month $2.50 6`, the rows upstream's test expects; the swap to the 429 file, `r`: `rate limited, try again shortly` (`…-refused.txt`). `…-end.files`: `~/.fxcap-usage-clock` holds `1791819000000`. |
| `/backend-api/wham/*` (3) | `h1-usage-codex` (30x100) | fx v0.0.16 gateway log lines 5, 7, 9: `wham/usage`, `wham/profiles/me`, `wham/usage/daily-token-usage-breakdown?start_date=2026-10-01`, each `headers=authorization,chatgpt-account-id`; `…-codex.txt`: `usage: codex (ChatGPT), samdoe`, `42%`, `12.40`. |
| `/billing`, `/settings` | `h1-usage-grok` (30x100) | fx v0.0.16 gateway log: `GET /billing?format=credits`, `GET /settings`, each `headers=authorization,x-userid`; `…-grok.txt`: `SuperGrok Heavy`, `63%  in 2d 5h`, `$3.00 of $100.00`. |
| `/ai-gateway/usage-report` | `fakegw-selftest.sh` | no binary here reaches it (the member scope needs a Vercel sign-in, `[unverified]` in the usage research): `fakegw-selftest.txt` asks it with curl and gets the budget from `h1-usage-budget.json`, `held 200ms`. |
| usage on the other two | `h1-usage*` | fx c1db919 and xfx answer `/usage` from their own tracking (`Usage [30 days] … Tracking has not started`); their gateway logs have no usage path at all. |
| `mcphttp` ready on cue | `h1-mcphttp` (30x100) | `…-hanging.txt`: `remote Connecting` (c1db919), `connecting` (v0.0.16), `remote Reloading` (xfx); after `mcphttp ready`, `…-ready.txt`: `Ready` / `ready, 1 tool`. `*-mcphttp.log`: the held `initialize` answered when the mode changed (c1db919 asked at +4485ms, answered +10748ms), then `notifications/initialized` and `tools/list` with the session id and `version=2025-11-25`; v0.0.16 first `server/discover` (`version=2026-07-28`) -> 404, then the legacy handshake. |
| `mcphttp` failing on cue | `h1-mcphttp-fail` (30x100) | c1db919 and xfx: `Failed` from launch (`fail=1` request, 500), `Failed` after the reload in `rpcerror`, `Ready` after the reload in `ready`. fx v0.0.16 has no `/mcp reload` and retries on its own: `retrying` through eight 500s, then `ready, 1 tool` once the mode is `ready`; its retries happened to miss the `rpcerror` window. |
| guards, over 15 s | `h1-guards` (18.5 s of waits after launch), and every fx run | `runs.log` of both fx: 14 runs each, 14 `guards: <refs>/fx-…/bin/fx sha256 7b76319a8711` / `8e6cda2efddb unchanged; fx-{…}.m4a and fx-click.caf still directories`; `h1-refuse` (a request at 18-24 s) and `h1-refuse-pause` (23 s) are longer still. `*-h1-guards-*.sockets`: every internet socket the binary had at launch, mid-turn, at 12 s and at the end -- only `127.0.0.1:<port>->127.0.0.1:<gateway>`; `sockets` fails the run on any other address. The gateway log is every request the binary made of it: one catalog read and two completions (title, turn). |
| bad arguments | `refusals.sh` | `refusals.txt`: 26 bad lines across every new verb, each `2` with the reason, none launched. |

## What the harness still cannot do

- The fake OAuth issuer, the fake ChatGPT token endpoint and the fake
  release host (lifecycle research needs 3-5), and the MCP OAuth sign-in
  (mcp research need 2), are not built: not in this unit.
- `reset` and `refuse` on the llmux and Codex wires are shown by
  `fakegw-selftest.txt` only (empty reply, then refused, then the next turn);
  no scenario drives a binary through them.
- A cut closes every connection open at that moment (Bun.serve cannot close
  one), so a turn that cuts should not overlap another stream.
- `resize` keeps the launch size in later snaps' file names.

## What the captures show about the products (not changed here)

- **Resize.** xfx moves the band to the new bottom row and leaves the
  draft's old first row behind, cut at the new width, at row 2
  (`xfx/xfx-h1-resize-30x100-small.txt` row 2, `-large.txt` row 2); both fx
  repaint the band in place under the banner.
- **Ctrl-Z.** Both fx suspend on Ctrl-Z under a job-control parent; xfx
  ignores the key (`src/tui/input.rs`, `the_two_editing_control_bytes_are_bound_and_ctrl_z_is_not`).
- **An outside SIGTSTP.** xfx restores the terminal before it stops
  (`xfx/…-stopped.wire` ends `\e[?2026l\e[?1049l\e[23;2t\e[>4;0m\e[<u\e[?2004l…`);
  both fx write nothing and stop (their `…-stopped.wire` ends with the last
  frame).
- **A reset stream / a refused connection.** fx retries and recovers; xfx
  fails the turn at once (`h1-reset`, `h1-refuse`, `h1-refuse-pause`).
- **MCP.** A held `initialize` reads `Connecting` in fx c1db919's menu and
  `Reloading` in xfx's; the startup failure notice is `! mcp: MCP startup: …`
  in fx c1db919 and `* mcp: MCP startup: …` in xfx (`…-mcphttp-fail-…-failed.txt` row 3).
- **A model released mid-session.** fx v0.0.16 reads the catalog again at
  `/model` and lists the new model; fx c1db919 and xfx do not ask again.
