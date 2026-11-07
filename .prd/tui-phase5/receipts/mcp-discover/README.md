---
title: MCP-E receipt -- fx v0.0.16's version detection (server/discover, 2026-07-28, no 2024-11-05)
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, mcp, security, receipt]
---

# MCP-E receipt -- `server/discover` first, the 2026 era, 2024-11-05 refused

Unit MCP-E of the 0.0.16 delta (`research/tui-phase5:.prd/tui-phase5/research/mcp.md`
E3/E4, the open question MCP-W left in `docs/parity.md`). **The user's
decision** (2026-11-07): the MCP handshake follows fx fully. Security-relevant
(S): it changes the protocol and what crosses the network. Base `58b463c8`
(train 2's head), branch `feat/p5-mcp-discover`. Oracle fx v0.0.16 (`481104fd`,
executable sha256 `8e6cda2efddb…`, checked by capture.sh around every fx run).
xfx: the base built from a `--no-local` clone at `58b463c8`
(`v0.1.0-58b463c`), and the head build at `d4183be1` (`v0.1.0-d4183be`; the
two later commits change comments, the parity row and a harness test only:
`a8de6c57`, `f16ae62c`). Every
capture is `scripts/fxcap/capture.sh <bin> <scenario> 40x120`, redacted
(`scripts/fxcap/redact.py`: `<scenarios>`, `<bun>`, `<repo>`, `<local-path>`).

## What fx v0.0.16 does (read at `481104fd`)

The core is `src/mcp/core/era.zig`, pure: events in, effects out
(`Detector.step`, `:189-308`); `src/mcp/server.zig` drives it.

- **Message order.** Every start -- each new stdio process (`processUp`,
  `server.zig:824-831`), each HTTP detection run (`detect`, `:395-399`) --
  sends `server/discover` first, with the first preferred version and the
  2026 `_meta`: `progressToken` (= the id), `io.modelcontextprotocol/protocolVersion`,
  `clientCapabilities`, `clientInfo` (`writeDiscover`, `era.zig:476-479`;
  `meta`, `:459-474`). Over HTTP the probe POST carries `Accept`,
  `MCP-Protocol-Version: 2026-07-28` and `Mcp-Method: server/discover`, no
  session (`requestHeaders`, `src/mcp/protocol/http_headers.zig:289-307`).
  - a result listing a preferred version: ready in that era, **no
    `initialize`**; every later request carries the 2026 `_meta` (and over
    HTTP the version, `Mcp-Method`, `Mcp-Name` for a call, no session);
  - a result listing only a legacy version, any other error, an error
    without an id, a stdio reply that is not valid JSON-RPC (it fails the
    request with -32600, `onRefusedLine`, `server.zig:769-790`), an HTTP body
    with no JSON-RPC answer whatever its status (`run`, `src/mcp/io/http.zig:417-487`;
    `endAttempt`, `server.zig:1242-1257`), or silence until the probe timer:
    a 2025 server -- `initialize` (`_meta.progressToken` only, `2025-11-25`
    asked; over HTTP `Accept` alone, `sendInitialize`, `:1472-1486`), then
    `notifications/initialized` with `"params":{}` (`writeInitialized`,
    `era.zig:493-496`);
  - -32022: another listed preferred version, else initialize if a legacy one
    is listed, else `no_shared_version`; -32020/-32021: `rejected`; over HTTP
    404 with -32601: a 2026 server without discovery, served at the probe's
    version (`classifyDiscover`, `classifyDiscoverHttp`, `era.zig:386-415`).
- **Versions.** Offered: `2026-07-28`; `initialize` asks `2025-11-25` and
  accepts `2025-11-25`, `2025-06-18`, `2025-03-26` -- not `2024-11-05`
  (`Config`, `era.zig:19-26`; `classifyInitialize`, `:417-444`).
- **Fallbacks and failures.** A modern server that falls silent and listed no
  legacy version: `went_quiet` (`legacyAnswer`, `:348-352`); an `initialize`
  error that hints at a modern server probes once more (`:252-262`); words
  (`describeFailure`, `src/core/mcp_host/runtime.zig:2290-2297`):
  `it supports no MCP version fx speaks`, `it refused fx's MCP version`,
  `it stopped answering while starting`,
  `it answered with an MCP version fx doesn't support (fx speaks 2025-03-26 and newer)`,
  `it refused or never answered fx's initialize request`. Each is retried as
  any start is (`.era => … .down`, `src/mcp/engine.zig:430-445`).
- **Timeouts.** The probe: 10 s (`probe_timeout_ms`, `era.zig:25`), cut to
  what is left of the start (`startLeft`, `server.zig:966-971`); the start is
  the server's `startup_timeout_ms` (`start_ms`, `src/mcp/core/request.zig:68-70`;
  `src/core/mcp_host/config.zig:150`); `initialize` gets what is left. A broken
  HTTP probe is posted once more (`max_attempts = 2`,
  `src/mcp/core/http_modern.zig:21-24`) and then waits out the timer. Ids come
  from one request table per server, so a new process's ids follow the last
  one's (`table`, `server.zig:264`).

## What changed in xfx

`src/mcp/era.rs` is `era.zig` ported whole -- the detector, the classifiers,
`meta` -- with fx's own tests: every answer sequence to depth 7 and 2,000
random long runs against the model's invariants (ProbesBounded,
NoFallbackAfterModern, EraSticky, ChosenSupported, LegacyOrder, ReachesEra),
then fx's cases one by one. `runtime.rs`'s handshake drives it one blocking
exchange at a time; `transport.rs` puts `_meta` first in every request, gives
both transports a `detect` that returns a reply as it came (status and
frame) and an era (`set_era`) that sets a request's `_meta` and HTTP headers,
starts ids at 1 from a counter the server's every try shares, drops a 2026
stdio server's own requests, routes an error without an id to the probe in
flight, takes an `initialize`'s session id only once its answer is accepted,
and ends every HTTP exchange at its caller's deadline.

## Pairs

`pairs.py` (this directory): cell by cell, row 1 (the banner) left out, raw
and after the ledger's normalizations -- each product's name and the HTTP
fixture's port (`mcphttp-port`, new in `scripts/fxcap/parity-ledger.toml`:
capture.sh picks a loopback port per run). `head-pairs.txt`, `red-pairs.txt`.

| scenario | snap | base vs fx (normalized rows) | head vs fx |
|---|---|---|---|
| `p5-mcp-discover-stdio-modern` | `show` | 10: `retrying`, `it refused or never answered xfx's initialize request` | **0**: `ready, 1 tool`, `version 2026-07-28` |
| | `turn` | 20 | **0**: the search and both calls |
| `p5-mcp-discover-stdio-silent` | `probing` (4 s) | 11: base answered `/mcp show` at once | **0**: both still probing |
| | `show` (14 s) | 0 | **0** |
| `p5-mcp-discover-stdio-old2024` | `show` | 10: `ready`, `version 2024-11-05` | **0**: `retrying`, fx's words |
| `p5-mcp-discover-stdio-malformed` | `show` | 0 | **0** |
| `p5-mcp-discover-stdio-hostile` | `show` | 10: `ready` | **0**: `it supports no MCP version xfx speaks` |
| `p5-mcp-discover-http-modern` | `show` | 9 | **0** |
| | `turn` | 19 | **0** |
| `p5-mcp-discover-http-notimpl` | `show` | 9 | **0** |
| | `turn` | 19 | **0** |
| `p5-mcp-discover-http-old2024` | `show` | 1: the words without fx's parenthesis | **0** |
| `p5-mcp-discover-http-malformed` | `show` | 0 | **0** |
| `p5-mcp-discover-http-hostile` | `show` | 9: `ready` | **0** |

Raw, the head differs only where the ledger says it may: `fx`/`xfx` in the
words (`stdio-old2024`, `stdio-hostile`, `http-old2024`, `http-hostile`) and
the port in an HTTP server's `url` row.

Where base and fx already look alike on screen (`malformed`, `silent`'s
`show`), the wire is the RED.

## The wire

`wire.txt` (`wire_compare.py`, this directory: stdio frames in order, compact
JSON, `clientInfo`/`clientCapabilities` -- M1's two approved differences --
masked, everything else compared) and `http.txt` (the HTTP fixture's log, its
times stripped: method, id, session, `MCP-Protocol-Version`, `Mcp-Method`,
`Mcp-Name`, status).

| scenario | base vs fx | head vs fx |
|---|---|---|
| `p5-mcp-wire-new` (MCP-W's) | differs: no discover, ids from 0, no `_meta` on `initialize`/`tools/list`, no `params` on `initialized` | **identical**, 6 of 6 |
| `stdio-silent` | differs | **identical**, 4 of 4 (discover, then initialize at 10 s) |
| `stdio-old2024` | differs | **identical**, 6 of 6 (three tries; ids 1-6 run on) |
| `stdio-malformed` | differs | **identical**, 4 of 4 |
| `stdio-hostile` | differs | **identical**, 3 of 3 (three probes, no initialize) |
| `stdio-modern` | differs | discover and every request's `_meta` identical; fx adds `subscriptions/listen` and two more `tools/list` (below) |
| `http-old2024` | differs, and base `DELETE`s a session after each refused try | **identical**: 4 x (discover 404, initialize), no `DELETE` |
| `http-hostile` | differs | **identical**: 4 probes |
| `http-malformed` | differs | identical but fx's listening `GET` (405) after `initialized` -- a 2025 session's listen, which xfx has never opened (not this unit's) |
| `http-modern`, `http-notimpl` | differs | discover and every request's headers identical, no session; fx adds `subscriptions/listen` and two more `tools/list` |

## RED

- Unit: `red-unit.txt` -- this branch's `src/mcp/tests/discover.rs` (the
  `set_probe_timeout` lines taken out, which base does not have) and
  `tests/support/mcp_discover_fixture.sh` on a `--no-local` clone at
  `58b463c8`: **13 of 13 fail**, each on its first assertion (wire frames,
  `Ready` where fx fails, `Failed` where fx is ready, the words without fx's
  parenthesis, no `Connecting` while the probe waits).
- Captures: the base column above, and the base side of `wire.txt` and
  `http.txt`.

## GREEN

- The 14 pairs above: 0 rows after normalization at `d4183be1`.
- `cargo test --lib mcp::` 207 passed (13 new in `mcp::tests::discover`, 15 in
  `mcp::era::tests`); the wire test of `a_profile_server_is_started_initialized_listed_and_called`
  and the HTTP fixture test now hold fx's frames.

## Security

- **No new network target.** The probe goes to the configured URL and
  nowhere else; a redirect is a status without an answer and is never
  followed; the proxy rules are the existing ones (`bearer_rules`).
- **Bounded.** The probe waits at most 10 s, cut to what is left of the start;
  `initialize` gets what is left; every HTTP exchange is dropped at its
  caller's deadline (new: a probe a server holds open cannot hold up the
  `initialize` behind it); an HTTP body is read up to 16 MiB, a stdio line up to
  16 MiB, and JSON nesting stops at serde_json's 128 levels.
- **A hostile answer** cannot crash, hang or reach the screen:
  a version list is only compared with xfx's own strings (`listed_in`, a mask,
  never kept); a failure is fx's fixed words; the version shown is xfx's
  own string. `a_hostile_discover_answer_neither_crashes_nor_hangs_a_start_nor_reaches_the_screen`
  runs nine answers -- a non-object result of escapes, near-miss versions with
  escape instructions, a raw escape line that is not JSON, an error without
  an id, 4,096 levels of nesting, -32021 with an escape in its message,
  -32022 listing only a legacy version, a modern server that falls silent,
  2024-11-05 -- and holds each to fx's end state and words, to its time (the
  probes it waits out, plus at most 2.5 s), and to a view and a `/mcp show`
  with no control character and nothing of the server's text. The
  `p5-mcp-discover-*-malformed` and `-hostile` captures show it on screen.
- A 2026 stdio server's own requests are not answered (fx drops them); an
  error without an id answers only the probe in flight.

## Mutants

`mutants.txt`: each on a `git clone --no-local` of the branch at `d4183be1`
in the scratchpad, never on the worktree, one at a time, with the tests that
should catch it.

| mutant | file | caught by | verdict |
|---|---|---|---|
| M1 `initialize` accepts a version not its own (2024-11-05) | `era.rs` | `a_stdio_server_that_speaks_only_2024_11_05_…` | killed |
| M2 the probe is not cut to what is left of the start | `runtime.rs` | `the_probe_and_initialize_share_the_start_…` | killed |
| M3 an HTTP exchange is not bounded by its caller's deadline | `transport.rs` | `an_http_probe_the_server_holds_open_…` | killed |
| M4 an error without an id does not answer the probe | `transport.rs` | `a_hostile_discover_answer_…` (`idless` waits) | killed |
| M5 a 2026 stdio server's own request is answered | `transport.rs` | `a_2026_stdio_server_is_served_…` | killed |
| M6 a request with no params carries no `_meta` | `transport.rs` | `a_stdio_server_is_asked_server_discover_first_…` | killed |
| M7 each try's ids start again at 1 | `runtime.rs` | `a_stdio_server_that_speaks_only_2024_11_05_…` | killed |
| M8 a 2026 HTTP POST carries no `Mcp-Method` | `transport.rs` | `a_2026_http_server_is_probed_…` | killed |
| M9 a stdio reply fx refuses is read as a version list | `runtime.rs` | `a_hostile_discover_answer_…` (`malformed`) | killed |
| M10 a modern server that falls quiet is initialized anyway | `era.rs` | `a_hostile_discover_answer_…` (`quiet`) | killed |
| M11 a 404 with -32601 is a legacy answer | `era.rs` | `a_404_with_method_not_found_…` | killed (applied by hand: rustfmt had wrapped the line the script looked for) |
| M12 the unsupported-version words lose fx's parenthesis | `era.rs` | `a_stdio_server_that_speaks_only_2024_11_05_…` | killed |
| M13 a version list matches a string that only contains a version | `era.rs` | `a_hostile_discover_answer_…` (`hostile`) | killed |
| M14 an HTTP body that is no JSON-RPC is a broken stream | `transport.rs` | `an_http_body_that_is_no_json_rpc_…` | killed |

14 of 14 killed; the clone's tree was clean after each.

## The MCP sweep

`sweep.sh --jobs 2 --only 'p5-mcp-*' --only p5-input-mcp` at 40x120, and
`--size 40x60 --only p5-mcp-menu-new`, fx v0.0.16, this branch's ledger (one
normalization added: `mcphttp-port`). Head: `f16ae62c` (`v0.1.0-f16ae62`,
whose `src/` is `d4183be1`'s but for comments). Base: `58b463c8` from the
clone, swept the same hour with the same ledger. Counts are
identical-or-normalized / approved / total, as epic 2's receipts count them.

| | train-2 receipt (`5f2d0b88`, its ledger) | base `58b463c8`, this run | head |
|---|---|---|---|
| `p5-mcp-*`, the 20 of epic 2, 40x120 | 87 / 0 / 146 | 84 / 1 / 146 | **103 / 1 / 146** |
| `p5-mcp-discover-*` (10, new), 40x120 | -- | 3 / 0 / 14 | **14 / 0 / 14** |
| `p5-input-mcp`, 40x120 | 9 / 0 / 9 | 9 / 0 / 9 | **9 / 0 / 9** |
| `p5-mcp-menu-new`, 40x60 | 22 / 1 / 34 | 22 / 1 / 34 | **33 / 1 / 34** |

- Pair by pair (`flips.py`, `flips-120.txt`, `flips-60.txt`): **no pair
  passes on the base and fails on the head**, at either size. 30 pairs pass
  on the head only at 40x120 and 11 at 40x60: the 11 new discover pairs, and
  19 of `p5-mcp-menu-new` / `p5-mcp-menu-old` whose `remote` (a profile
  server at `127.0.0.1:9`, which refuses) fx shows as `connecting` while
  xfx's base showed `retrying` with `it refused or never answered xfx's
  initialize request`. Each try now waits out its probe as fx's does, so the
  two read the same -- the parity row's "an HTTP try against a port that
  refuses ends at once" divergence is gone.
- This run's base reads 84, not train 2's 87: `menu-old` 8 and `menu-new` 22
  (+1 approved) where train 2 read 10 and 23 -- the first-run spread train
  2's receipt already notes for those two (8 and 3 on a first run of the
  epic-2 preview), from the same `remote` racing its first try. Every
  other scenario reads train 2's count on the base, and the head's count or
  more on the head.
- The one approved pair (`p5-mcp-menu-new/approve-confirm`,
  `mcp-approval-wrap-led`) is approved on both builds.

`sweep-head-120.md`, `sweep-head-60.md`, `sweep-base-120.md`,
`sweep-base-60.md`: the scoreboards (the sweep's own redaction).

## Gates

At `a8de6c57` (`TMPDIR=/private/tmp/claude-501/p5md-gate`), one step after
another; `test-capture.sh` then failed one case of its own (the on-cue
`mcphttp` regexes did not know the log's new `mcp-method=`/`mcp-name=`
fields), fixed in `f16ae62c` and re-run alone: 51 of 51.

| step | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| `cargo clippy --locked --all-targets --features fault-injection -- -D warnings` | ok |
| `cargo test --locked --no-fail-fast` | ok: 5,186 passed, 0 failed, 7 ignored (15 binaries) |
| `cargo test --locked --features fault-injection --no-fail-fast` | ok: 5,223 passed, 0 failed, 7 ignored |
| `scripts/check-xfx-identity.sh` | ok |
| `scripts/check-no-stubs.sh` | ok |
| `scripts/check-no-secrets.sh` | ok |
| `scripts/fxcap/test-sweep.sh` | ok |
| `scripts/fxcap/test-capture.sh` | ok at `f16ae62c` (51 tests) |

Since `d4183be1`, the build the pairs come from, `src/` changed in comments
only (`a8de6c57`: fx line ranges re-read).

## `docs/parity.md`

The MCP client runtime row: the legacy-only handshake and the open question
are replaced by fx's detection as above; the divergence "an HTTP try against
a port that refuses ends at once" is gone (each try now waits out its probe,
as fx's do). What of the 2026 era xfx does not do yet is listed there:
`subscriptions/listen` and the re-lists before each call (the
`-modern`/`-notimpl` wires), `input_required` rounds (`McpInputRequired`),
an operation's -32022 version switch, `Mcp-Param-*` headers, fx's stricter
decoder for escaped version strings and duplicate keys, a stdio answer nested
past serde's 128 levels (a line that is not JSON to xfx, so the probe waits
out its timer; fx falls back at once -- the same end, later), and the 60 s cancel
of an abandoned probe (xfx sends none on stdio and closes it over HTTP).

## Observed, not this unit's

fx answers `/mcp show` of a retrying stdio server only once its next try is
over; xfx answers at once. The stdio `old2024`/`hostile` scenarios give it
4 s, so both are on screen.

## Files

- `README.md`, `pairs.py`, `wire_compare.py`, `flips.py` (the receipt's own
  comparison scripts; `python3 -I` each).
- `fx/`, `xfx-base/`, `xfx-head/`: the 14 snaps' text.
- `head-pairs.txt`, `red-pairs.txt`, `wire.txt`, `http.txt`, `red-unit.txt`,
  `mutants.txt`, `sweep-*.md`, `flips.py`, `flips-*.txt`.
