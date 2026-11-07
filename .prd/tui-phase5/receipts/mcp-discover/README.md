---
title: MCP-E receipt -- fx v0.0.16's version detection (server/discover, 2026-07-28, no 2024-11-05), rounds 2 and 3
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, mcp, security, receipt]
---

# MCP-E receipt -- `server/discover` first, read as fx reads it

Unit MCP-E of the 0.0.16 delta. **The user's decision** (2026-11-07): the MCP
handshake follows fx fully. The dispatcher's (2026-11-07, reversible): fx's
post-ready 2026 behaviour -- `subscriptions/listen`, a re-list before each
call, `input_required` rounds, an operation's -32022 version switch,
`Mcp-Param-*` headers -- is unit **MCP-E2**'s, named in `docs/parity.md` as
not yet xfx's. Security-relevant (S). Base `58b463c8`, branch
`feat/p5-mcp-discover`. Round 2 answers both reviews of `f3a859c8`
(`MCP-E/reviews/gpt6.md`, `opus.md` in the dispatcher's tree).

**Two commits, each with its own evidence.** Round 2's code, `d96603f9`
(`v0.1.0-d96603f`): the head side of every capture and sweep, the wire, its
gate (`gates-d96603f9.txt`) and its 31 mutants (`mutants-d96603f9.txt`).
Round 3 (MCP-Ec, below), `7c2ceeea`: the gate (`gates.txt`, `gates/`) and the
mutants (`mutants.txt`, a `--no-local` clone checked out at it). The sweeps
were not run again at `7c2ceeea`, so the captures (`d96603f9`) are
historical. What it changes in `src/` (`ec-src-diff-stat.txt`): the HTTP
transport's admission and the order waiting messages start in, whose
saturation behaviour (32 messages in flight) no capture scenario reaches; a
guard every caller's message passes before its POST (a passed deadline or a
gone caller sends nothing); an answered probe no longer counted as open when
its answer is read, on every probe; test-only hooks, tests and comments. The
lifecycle changes are covered by the tests named in round 3 below, not by
captures. The
receipt's own commit changes files under `.prd/tui-phase5/receipts/mcp-discover/`
and nothing else: `diff-stat.txt` is `git diff --stat 7c2ceeea -- src tests scripts
docs Cargo.toml Cargo.lock`, taken just before that commit, and empty.
Oracle: fx v0.0.16 (`481104fd`), executable sha256 `8e6cda2efddb…`, checked
by capture.sh around every fx run. Base: `58b463c8` built in the clone
(`v0.1.0-58b463c`). Round 1's head: `f3a859c8` built in the clone
(`v0.1.0-f3a859c`), the RED for what round 2 changes.
`src-diff-stat.txt` is `git diff --stat 58b463c8..d96603f9 -- src tests`.

## What fx v0.0.16 does (read at `481104fd`)

Round 1's reading (`src/mcp/core/era.zig`'s detector, driven by
`src/mcp/server.zig`) stands: every start sends `server/discover` first with
`2026-07-28` and the 2026 `_meta`; a result listing it is that era with no
`initialize`; anything else, or silence until the 10 s probe timer (cut to
what is left of the start, `startLeft`, `server.zig:966-971`), is a 2025
server, which gets `initialize` -- `2025-11-25`, `2025-06-18` or `2025-03-26`
accepted, `2024-11-05` refused. Round 2 read what round 1 had not:

- **The decoder.** fx reads every message raw (`src/mcp/protocol/wire.zig`):
  a field is found by its key as written, and a wanted one named twice is an
  error (`objectFields`, `:142-174`); a version is compared as written, so
  one spelled with an escape is another version (`arrayHasString`,
  `:248-257`; `plainString`, `:274-279`; `era.zig:370-377`, `:425`); an id is
  the client's only as plain decimal digits, bare or quoted (`ownId`,
  `decimal`, `:305-326`); nesting stops at the scanner's 512-byte stack
  (`scan_stack_bytes`, `:100-101`) -- 2584 levels in all, found by
  `p5-mcp-discover-stdio-depth`'s four servers (`fx/…depth…list.txt`:
  126 and 2582 nested arrays list no version, `retrying`; 2583 and 4000 are
  no answer, the probe waits out its 10 s, `ready`).
- **A refused stdio line** (`onRefusedLine`, `server.zig:772-790`): one that
  names a request in flight (`responseId`, `wire.zig:135-140`) fails it with
  -32600 and `invalid reply (<why>): <the line's first 80 bytes>` (control
  bytes `?`, never a cut character, `...` when cut: `excerptOf`,
  `server.zig:60-66`; `maskedPrefix`, `src/mcp/io/process.zig:170-177`); any
  other -- a banner, a line too deep to find its id in -- is kept aside
  (`keepStray`, `server.zig:441-443`) and disturbs nothing. So a too-deep
  answer is no answer: the probe waits out its timer (opus's review read
  "fx falls back at once"; the depth capture and `responseId`'s `catch
  return null` say otherwise).
- **A server's requests** are answered only in a 2025 session (`.request =>
  if (r.legacyEra())`, `server.zig:1154`), which on stdio is the detector in
  `initializing` or `legacy` (`legacyEra`, `:888-891`): a `ping` while the
  probe waits is dropped, one while `initialize` waits is answered
  (`p5-mcp-discover-stdio-asks`'s wire).
- **HTTP.** Each request is a task of its own, at most 32 running and 32
  more waiting, oldest first, then refused (`max_in_flight`,
  `waiting_room`, `claim`, `src/mcp/io/http.zig:26`, `:106-108`,
  `:218-226`). An event stream is read event by event and each event is
  handed on as it comes (`readEvents`, `:643-692`); the first that answers
  the POST -- naming its id, or none: `if (resp.id) |id| if (id != key)
  return;`, `server.zig:1155-1158` -- is the answer, whatever the status. A
  JSON body's result under an error status is no answer (`run`,
  `http.zig:469-474`); redirects are not followed (`.redirect_behavior =
  .unhandled`, `:424`).
- **A probe given up on** stays in the request table (`stdioDelivered`
  closes nothing for `.discover`, `server.zig:871-872`) until its request
  times out: `idle_ms` = the server's `operation_timeout_ms`, 60 s by default
  (`src/core/mcp_host/config.zig:151`; `src/mcp/core/request.zig:64-66`).
  Then, on stdio, `notifications/cancelled` with its id (`timerFired`,
  `request.zig:311-318`; `send_cancel`, `server.zig:1986-1995`); over HTTP
  its stream is closed only if no 2025 session has begun -- in one the
  cancel goes to the session, which does not know the probe (`.timed_out =>
  if (r.legacy == null) … stepExchange(l, .cancel)`, `server.zig:1997-2000`;
  `http_legacy.zig:288-289`), so the POST stays open until the server ends
  it or the connection closes. `initialize` is never cancelled
  (`timerFired`'s `slot.kind != .initialize`, `request.zig:317`).

## What changed in xfx (round 2)

- `src/mcp/wire.rs` (new): `wire.zig`'s decoder, ported with fx's own
  decoder tests (`src/mcp/wire/tests.rs`) -- non-recursive, so no input can
  exhaust the stack, and cut at fx's 2584 levels. Detection reads every
  answer through it, raw: `era.rs`'s classifiers take `wire::Body`, the
  runtime keeps capabilities and instructions from the raw result (a field
  named twice says neither, as `keepInstructions`'s `catch {}` does).
- `src/mcp/era.rs` is the detection core ported; its operation events
  (`OperationSent`, `OperationVersionRejected`,
  `classify_operation_rejection`) are ported and tested but driven by
  nothing: MCP-E2's (round 1's README said "ported whole").
- stdio (`transport.rs`): a line routes by fx's decoder -- a response to its
  waiter, an error without an id to the probe in flight, a refused line that
  names a request to that request as fx's -32600, anything else to no one; a
  server's request answered only from `initialize` on; a probe given up on
  cancelled at `operation_timeout_ms` with fx's frame
  (`{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":N}}`).
- HTTP (`transport.rs`): every message a task on the connection's thread,
  32 running and 32 waiting; an event stream parsed as bytes come, held to
  16 MiB a line and an event; a probe given up on kept as fx keeps it; a
  request whose caller stops waiting has its POST dropped, as fx closes a
  timed-out request's stream outside a session.
- `runtime.rs`: the era each detection message goes out in (`Detecting`
  before a probe, `Initializing` before `initialize`, as fx's
  `send_discover`/`startSession` set `r.legacy`), the transports' request
  timeout from `operation_timeout_ms`, and the probe's wait as a pure
  function (`probe_wait`).
- Tests (`src/mcp/tests/discover.rs`): no elapsed-time upper bound is
  asserted. A wrong path that would wait is made to hang -- a ten-minute
  probe timer, cut to the start -- so it ends in other words; what is
  asynchronous is waited for by state (the fixture's connections ended, a
  cancel frame logged, `probes_open`) within `WAIT`; lower bounds stay.
  Round 2 still read two states that a timer of the case's own could end
  first, and kept a clock bound in configuration: round 3 below says what
  was fixed and what clock dependence is left.
- fxcap: `capture.sh` writes `<bin>-<scenario>-<size>-mcphttp.port`, and
  `sweep.py` (`run_rules`) rewrites only that run's port after
  `127.0.0.1:`; the ledger's global `mcphttp-port` rule is gone
  (`test_sweep.py` `McpPort`: a fixed port, the other side's port, longer
  digits, another host and the digits elsewhere stay different). New
  fixture modes: `ssehold`, `sseidless`, `ssestrid`, `ssefail` (mcphttp.ts),
  `banner`, `asks`, `escaped`, `doubled`, `deep:<n>` (the stdio servers),
  with self-tests in `test_capture.py`.

## The reviews, item by item

| item | what was wrong at `f3a859c8` | now | evidence |
|---|---|---|---|
| MUST 1 SSE | an event stream read to its end before any event was looked at; only `id == N` taken | parsed as bytes come; the first event that answers the POST taken -- its id, its id as digits in a string, or none -- under any status; 16 MiB a line and an event | `a_probe_answered_on_an_event_stream_is_read_as_its_events_come` (`ssehold`, `ssestrid`, `ssefail`, `sseidless`); `transport.rs` `a_stream_answer_…_as_it_comes`, `a_stream_event_answers_its_post_…`; M15-M18; `p5-mcp-discover-http-sse*` |
| MUST 2 HTTP exchange at the caller's deadline | the probe's POST closed when the caller stopped waiting | kept as fx keeps it (above), its own task, beside the requests after it | `an_http_probe_the_server_holds_open_stays_open_beside_the_requests_after_it`, `an_abandoned_http_probe_ends_at_its_request_timeout_only_outside_a_session`, `a_request_whose_caller_stops_waiting_has_its_post_closed`; M3, M3b, M20, M21 |
| MUST 3 stdio probe never cancelled | `cancel_on_wire = !probing` | `notifications/cancelled` at `operation_timeout_ms`, fx's frame | `a_stdio_probe_given_up_on_is_cancelled_at_its_request_timeout`; the silent test's `probes_open == 1`; M19 |
| MUST 4 escapes, duplicate keys | serde's reading | fx's decoder | `escaped`, `doubled` rows; `era::tests::an_escaped_version_is_another_version_and_a_doubled_field_no_answer`; `p5-mcp-discover-stdio-escaped`; M22, M23 |
| MUST 5 too deep | serde's 128 levels | fx's 2584; deeper is no answer and the probe waits, as fx's does | `deep:2582`, `deep:2583`, `deep` rows; `wire::tests` depth boundary; `p5-mcp-discover-stdio-depth`; M24, M25 |
| MUST 6 `mcphttp-port` global | one ledger regex over every scenario | the run's own port only (`sweep.py` `run_rules`), the ledger rule gone | `test_sweep.py` `McpPort` (4 cases); the sweeps below |
| MUST 7 wall clock (see round 3) | upper bounds and once-after-sleep checks | lower bounds only; a ten-minute probe for the paths that must not wait; state barriers within `WAIT`; `probe_wait` pure; one more call after the 2026 call instead of a 500 ms sleep | `src/mcp/tests/discover.rs`: every `took`/`elapsed()` comparison left is `>=`, but one `while started.elapsed() < idle * 3` that waits past a timeout before a barrier request; every `sleep` is a 10-20 ms step of a poll bounded by `WAIT`; M2, M4, M5, M14 killed below |
| MUST 8 receipt | claims without files | every number below names its file; one commit | this file |
| SHOULD 9 excerpt | from the re-serialized frame | from the raw line, 80 bytes, `?` for control bytes | `wire::tests::a_refused_reply_fails_with_why_and_the_start_of_the_line_as_it_came`; M30 |
| SHOULD 10 3xx | doc said a redirect's body is no answer | doc says what the code does; a 307's target is never asked | `a_probe_redirected_elsewhere_is_not_followed` |
| SHOULD 11 banner (PLAUSIBLE) | RED: see `p5-mcp-discover-stdio-banner` below | kept aside | `a_2026_server_that_writes_a_banner_before_its_answer_is_still_read`; M26 |
| SHOULD 12 requests while detecting (PLAUSIBLE) | RED: see `p5-mcp-discover-stdio-asks` below | answered only from `initialize` on | `a_servers_request_is_answered_only_once_initialize_has_begun_a_2025_session`; M27, M28 |
| wording | README "ported whole"; "requests a server sends are answered" | the operation events named MCP-E2's; "a 2025 server's requests on stdio" | `docs/parity.md` MCP client runtime row |

One pre-existing test changed: `transport::tests::a_server_that_asks_and_never_reads_the_answers_cannot_hold_close`
now sets a 2025 era and sends one line before its server floods `ping`s --
fx answers a server's request only in a 2025 session, so without it nothing
would be answered and the flood would test nothing.

## MCP-Ec (round 3): the round-2 reviews of `4deb7d89`

Code `7c2ceeea` (`git diff --stat d96603f9..7c2ceeea -- src tests docs`:
`ec-src-diff-stat.txt`). Reviews: `MCP-E/reviews/r2-gpt6.md`,
`r2-opus.md` in the dispatcher's tree.

| item | at `d96603f9` | now | evidence |
|---|---|---|---|
| MUST 1 silent stdio | `probes_open == 1` read after `settled`, while the probe's 60 s request timeout ran on its own | the server's `operation_timeout_ms` is an hour, past every bounded wait of the case, so the probe cannot have been cancelled when it is looked at | `a_stdio_server_that_never_answers_the_probe_is_initialized_once_the_probe_times_out` |
| MUST 1, the stdio cancel case | needed `tools/list` answered within the 2 s it set as the request timeout | the server answers nothing (`mute`) and the start is an hour, so the connection is up, `initialize` waiting, when the cancel goes; only a lower bound, polls, and that the first frame read is the probe (MCP-Ed: no longer that `initialize` came before the cancel) | `a_stdio_probe_given_up_on_is_cancelled_at_its_request_timeout` |
| MUST 2 abandoned HTTP probe | 100 ms caller wait and 300 ms idle started before the fixture had the POST | request timeout an hour off; the case waits for the fixture to log the POST, then fires the timeout itself (`HttpTransport::time_out_probes`, test builds only), then waits until the transport has dealt with it (`probes_timed_out`) | `an_abandoned_http_probe_ends_at_its_request_timeout_only_outside_a_session`; M21 |
| SHOULD 3 | `start_once` on a 10 s start | the 30 s default | `start_once` |
| SHOULD 4 (PLAUSIBLE → RED) | a message that waited for a slot was polled once when it started, its caller gone or not | not polled at all if its deadline has passed or its caller has gone; waiting messages start oldest first, as fx's `startWaiting` | RED (`red-waiting-job.txt`): on `d96603f9` a first version of the case saw 2 and 5 extra connections in 2 of 3 runs; the committed case with the guard taken out sees 12-20 of its 32 given-up calls begin a POST, 5 of 5 runs; `a_waiting_request_whose_caller_has_gone_never_reaches_the_server` (begun POSTs = 33, fixture connections = 33, no `tools/call` logged); M32 |
| SHOULD 5 | the caller-stop case covered the deadline only | a `CancelToken` case: an hour's deadline, cancelled once the fixture has the POST; the POST's connection ends and the next request goes | `a_request_whose_caller_stops_waiting_has_its_post_closed_and_the_next_goes`; M3, M3c |
| SHOULD 6 admission | an unbounded channel before the 32 + 32 | at most 64 messages per connection from the send to the end of the message (`admit`, fx's `claim`), the 65th `McpTooManyInFlight` wherever it would have waited | `past_32_exchanges_in_flight_…`; M29, M31 |
| SHOULD 7 | `Life::Caller` said fx closes a timed-out or cancelled stream | outside a 2025 session at once; in one, fx first posts `notifications/cancelled` through the session (`post_cancel`, `server.zig:1402-1411`) and closes the stream when the session ends the request (`deliver`, `:1441-1448`); xfx drops the connection in both eras without that POST | `transport.rs` `Life::Caller` |
| SHOULD 8 | `decimal` claimed fx's `max_request_id` cap | fx's `decimal` reads any 64-bit integer (`wire.zig:315-327`); `max_request_id` (2^53 - 1, `:15-17`) bounds the ids fx issues (`request.zig:233`) | `wire.rs` `decimal` |
| owner | two 2025-session HTTP gaps without one | unit MCP-SEAMS (dispatcher, 2026-11-07), named in `docs/parity.md` | the MCP client runtime row |

Who can make a caller. Every message on an HTTP connection comes from a
runtime call: the start's handshake, `tools/list` paging, a tool call, a
resource or prompt read, a completion (`runtime.rs` `handshake`,
`call_tool`, `feature_request`). None comes from the server: xfx answers no
request a 2025 server sends over HTTP and follows no notification (an event
stream's other events are passed over, `EventStream`), so no server-driven
path creates one.

What still leans on a clock in `src/mcp/tests/discover.rs`, and how much:

- A handshake that must succeed has the start to do it in: the 30 s
  default (`start_once`, the HTTP cases), and the probe timer where the
  case sets one.
- `a_probe_whose_connection_breaks_…`: the second POST must go within the
  probe's 5 s timer, and the first break is at once.
- `settled` and every barrier wait at most `WAIT` (60 s) for a state that
  correct code reaches in milliseconds.
- Request timeouts and starts the cases set an hour off (the silent stdio
  case's requests, the stdio cancel case's start, the held HTTP transport)
  are past the sum of the case's own bounded waits.

No case reads a state that a timer of its own can end before the case looks
at it. The lower bounds stay (`took >= …`).

## MCP-Ed: one test, the receipt, and main merged in

- `8ea201e9` (tests and docs only): the stdio cancel case no longer asserts
  that `initialize` was written before the 2 s cancel; it checks that the
  first frame read is the probe (`{"method":"server/discover","id":1}`), and
  the lower bound and the `probes_open` poll carry the rest. The paragraph
  on what the sweeps cover now says the captures are historical and which
  `src/` changes the tests, not the captures, cover. `docs/parity.md` names a
  third MCP-SEAMS gap (a given-up waiting message keeps its admission place
  until popped; fx frees it at the cancel, `http.zig:252-263`).
- `7ff9864b`: `Merge origin/main 6c93f359 (#78 landed) into
  feat/p5-mcp-discover for train 4`, no conflicts.
- The gate at `7ff9864b` (`gates-ed.txt`, `gates-ed/`), one step after
  another, the tree clean before and after: build, the MCP tests (232
  passed), fmt, both clippy runs, `cargo test` (5,218 passed, 0 failed, 7
  ignored) and with fault-injection (5,255 passed, 0 failed, 7 ignored),
  identity, no-stubs, no-secrets, test-sweep, `scripts/test-check-no-secrets.sh`
  (13 cases) and `scripts/test-gate-env.sh` (5 cases): exit 0.
  **`scripts/fxcap/test-capture.sh` exits 1** on one case of fxcap's own,
  `Verbs.test_models2_and_modelsdelay`, in the gate and run again alone:
  its fake gateway serves a second catalog 5,000 ms after it starts, and the
  case's first `curl` reached it at 9,042 ms (`gates-ed/models2-gateway.txt`)
  on this machine at a load average of 9-19, so the first catalog was never
  served. Alternated in the clone, the case fails on `7c2ceeea` -- before the
  merge -- as on `7ff9864b`, four of four (`gates-ed.txt`); it passed at
  `7c2ceeea` in round 3's gate (`gates.txt`: test-capture exit 0). Neither
  this branch nor the merge changes it (`scripts/fxcap/test_capture.py`,
  `fakegw.ts` and `capture.sh` are the same at `7c2ceeea` and `7ff9864b`).

## RED: round 1's head against round 2's cases

`sweeps/red-120-*` (`v0.1.0-f3a859c`, the 18 discover scenarios and
`p5-mcp-wire-new`, round 2's scenarios and ledger), the f3a859c sections of
`wire.txt` and `http.txt`, and `snaps/xfx-f3a859c/`:

| scenario | fx v0.0.16 (`snaps/fx/`) | round 1 (`f3a859c`) | head (`d96603f`) |
|---|---|---|---|
| `http-ssehold` | ready at 2026-07-28 | `/mcp show` not answered by the snap; one discover POST, nothing after it (`http.txt`) | ready, discover then `tools/list` |
| `http-sseidless` | ready at 2025-11-25 | discover sent twice, no `initialize` | ready at 2025-11-25 |
| `http-ssestrid` | ready at 2026-07-28 | discover sent twice | ready at 2026-07-28 |
| `stdio-banner` | ready at 2026-07-28 | nothing by the snap | ready at 2026-07-28 |
| `stdio-asks` | `q1` unanswered, `q2` answered (`wire.txt`) | answers `q1` during the probe: DIFFERS | IDENTICAL |
| `stdio-depth` | d126, d2582 `retrying`; d2583, d4000 `ready` | all four `ready` | as fx |
| `stdio-escaped` | `escaped` `retrying`, `doubled` `ready` | the other way round | as fx |
| `http-ssefail` | ready | ready (round 1 never looked at a stream's status) | ready |

`flips-red-head-120.txt`: 4 pairs pass on the head only, none on round 1's
only. `stdio-depth` and `stdio-escaped` count as unresolved on both, for one
row each: the `/mcp list` header, whose SOURCE column is a cell wider in xfx
because `~/.xfx/mcp.json` is a character longer than `~/.fx/mcp.json` -- the
same row `p5-mcp-config`'s `list` differs in, not this unit's; every status
row is the same as fx's (`snaps/`).

Unit RED for what is not on screen: the mutants below that put round 1's
behaviour back (M15 stream read at its end, M19 no stdio cancel, M20 HTTP
probe closed at once, M26 banner, M27/M28 requests while detecting), each
killed by the test written for it.

## Pairs and the MCP sweep

`sweep.sh --jobs 2`, fx v0.0.16, this commit's ledger and scenarios, one
sweep after another (`sweeps/*-scoreboard.md`, `*-summary.md`: the sweep's
own redaction). 40x120: `--only 'p5-mcp-*' --only p5-input-mcp`; 40x60:
`--only p5-mcp-menu-new`. Counts are pass / approved / total.

| | base `58b463c` | round 1 `f3a859c` | head `d96603f` |
|---|---|---|---|
| the 20 `p5-mcp-*` of epic 2, 40x120 | 85 / 1 / 146 | -- | **102 / 1 / 146** |
| `p5-mcp-discover-*` (18), 40x120 | 5 / 0 / 22 | 16 / 0 / 22 | **20 / 0 / 22** |
| `p5-input-mcp`, 40x120 | 9 / 0 / 9 | -- | **9 / 0 / 9** |
| `p5-mcp-menu-new`, 40x60 | 22 / 1 / 34 | -- | **33 / 1 / 34** |

- The two discover pairs the head does not pass are `stdio-depth/list` and
  `stdio-escaped/list`, the header row above.
- `flips-base-head-120.txt`: 33 pairs pass on the head only; **one passes on
  the base only**, `p5-mcp-menu-old/prompts`: its `remote` (a profile server
  at `127.0.0.1:9`, which refuses) reads `retrying` on the head where fx
  reads `connecting`. Run three more times on each build (`sweeps/menu-old-reruns.txt`), the head passes `prompts` in runs 1 and 3 and fails it in run 2 (with `resources`); the base passes `prompts` every time and fails `resources` and `servers` every time. The three snaps fall in the second around the end of the 10 s probe, where fx's try and xfx's may each be on either side of it: the head flips with fx's timing, the base matches at `prompts` only by its own retry cadence. Round 1's receipt noted the same spread on this server.
- `flips-base-head-60.txt`: 11 pairs pass on the head only, none on the
  base only.
- The approved pair is `p5-mcp-menu-new/approve-confirm`
  (`mcp-approval-wrap-led`), on every build.

## The wire

`wire.txt` (`wire_compare.py`: a stdio fixture's frames in order, compact
JSON, `clientInfo`/`clientCapabilities` masked -- MCP-W's two approved
differences -- everything else compared) and `http.txt` (the HTTP fixture's
log, times stripped), fx against each build, every `p5-mcp-*` scenario that
logs one.

- Head IDENTICAL to fx: `stdio-asks`, `-hostile`, `-malformed`, `-old2024`,
  `-silent`, `p5-mcp-wire-new`, `p5-mcp-wire-old`.
- Head DIFFERS only by what MCP-E2 owns: `stdio-modern` and `stdio-banner`
  (fx's `subscriptions/listen` and its re-lists before each call); over HTTP
  the same for `-modern`, `-notimpl`, `-ssehold`, `-ssestrid`, `-ssefail`;
  and a 2025 session's listening `GET`, which xfx has never opened
  (`-sseidless`, `-malformed`, `p5-mcp-live`).

## Mutants

Round 3 (`7c2ceeea`): `mutants.py` on the clone checked out at it (its HEAD in
`mutants.txt`'s first lines): **34 of 34 killed, the clone clean after each**. Round 2's 31 on the code as it is
now -- M21 rewritten for its new form (the probe kept at its timeout outside
a session: the abandoned-probe case, whose timeout the case now fires) --
and three new: M3c (the caller's stop branch removed: the CancelToken case),
M31 (admission unbounded: `past_32_…`) and M32 (a waiting message whose
caller has gone is started, `d96603f9`'s behaviour: the waiting-message
case).

Round 2 (`d96603f9`): `mutants-d96603f9.py` (this directory) on a `git
clone --no-local` checked out at `d96603f9`, in the scratchpad, never the
worktree: one at a time, applied, the named test run, reverted (`git
checkout`). `mutants-d96603f9.txt`: **31 of 31 killed**, the clone clean after each. Round 1's 14 on the code as it is now
(M1-M14, M3 split into the request's wait and the probe's), M2, M3, M4,
M5 and M14 among them; and 16 new:

| mutant | caught by |
|---|---|
| M15 an event stream read only once it ends (round 1) | `a_probe_answered_on_an_event_stream_…` (`ssehold` hangs into `settled`) |
| M16 an answer without an id does not answer its POST | the same (`sseidless`) |
| M17 a string id is never the client's own | `transport::tests::a_stream_event_answers_its_post_…` (`"8"`) |
| M18 a stream's answer under an error status refused | `a_probe_answered_on_an_event_stream_…` (`ssefail`) |
| M19 a stdio probe given up on never cancelled (round 1) | `a_stdio_probe_given_up_on_is_cancelled_…` |
| M20 an HTTP probe given up on closed at once (round 1) | `an_http_probe_the_server_holds_open_…` (`probes_open`) |
| M21 an HTTP probe given up on outlives its timeout outside a session | `an_abandoned_http_probe_…` |
| M22 a version written with an escape matches | `a_hostile_discover_answer_…` (`escaped`) |
| M23 a field named twice taken | the same (`doubled`) |
| M24 / M25 the decoder reads deeper / less deep than fx's | the same (`deep:2583` / `deep:2582`) |
| M26 a line naming no request fails the probe (banner) | `a_2026_server_that_writes_a_banner_…` |
| M27 a server's request answered while detection runs | `a_servers_request_is_answered_only_once_initialize_…` |
| M28 no `Initializing` era before `initialize` | the same |
| M29 no waiting room past 32 in flight | `past_32_exchanges_in_flight_…` |
| M30 a refused line's excerpt cut at 40 bytes | `wire::tests::a_refused_reply_fails_with_why_…` |

## Security

- **No new network target.** The probe and `initialize` go to the configured
  URL only; a redirect is a status, its `Location` never asked
  (`a_probe_redirected_elsewhere_is_not_followed`: a 307 to a second
  listener, which then has nothing in its backlog); the proxy rules are the
  existing ones (`bearer_rules`, `Policy::none`).
- **Bounded.** A probe's wait is its 10 s timer cut to what is left of the
  start (`probe_wait`, `probe_wait_is_its_timer_cut_to_what_is_left_of_the_start`);
  `initialize` gets what is left. A probe given up on lives at most the
  server's `operation_timeout_ms` on stdio and outside a 2025 session over
  HTTP; inside one it lives until the server ends it or the connection
  closes, as fx's does -- one per try, counted (`probes_open`), and ended
  with the connection (`an_abandoned_http_probe_…`: the close ends it). At
  most 32 HTTP exchanges run and 32 wait; the 65th message is refused
  (`past_32_exchanges_in_flight_…`). An event stream's line and event, an
  HTTP body and a stdio line are each held to 16 MiB
  (`a_stream_event_or_line_longer_than_a_frame_is_too_large`). The decoder
  is iterative and stops at 2584 open containers, so no line can exhaust a
  thread's stack (`src/mcp/wire/tests.rs`: depth boundary and fuzz).
- **A hostile answer** cannot crash, hang or reach the screen:
  `a_hostile_discover_answer_neither_crashes_nor_hangs_a_start_nor_reaches_the_screen`
  runs fourteen -- a non-object result of escapes, near-miss versions with
  escape instructions, an error without an id, 2584 levels, -32021 with an
  escape in its message, -32022 listing only a legacy version, 2024-11-05, a
  server that asks `ping`, versions written with escapes, a field named
  twice; and, each waiting out one probe and no less, a raw escape line,
  2585 and 4098 levels, a modern server that falls silent -- and holds each to
  fx's end state and words and to a view and a `/mcp show` with no control
  character and nothing of the server's text. A version list only becomes a
  bit mask; the version shown is xfx's own string; a refused line's excerpt
  (`invalid_reply`) is only ever scanned for a preferred version and never
  shown.
- A server's request is answered only in a 2025 session; a 2026 server's,
  and any while detection runs, are dropped (`a_servers_request_is_answered_only_once_initialize_has_begun_a_2025_session`,
  `a_2026_stdio_server_is_served_in_its_era_with_no_initialize`).

## Gates

`gates.txt`: at `7c2ceeea`, one step after another, `TMPDIR=<tmp>`, the tree
clean before and after; each step's log under `gates/` (the two test suites
as their per-binary result lines, `gates/test-results.txt`,
`gates/test-fi-results.txt`). Round 2's, at `d96603f9`: `gates-d96603f9.txt`
(5,210 and 5,247 passed).

| step | result (file) |
|---|---|
| `cargo build --locked` | exit 0 (`gates.txt`) |
| `cargo fmt --check` | exit 0 (`gates/fmt.log`) |
| `cargo clippy --locked --all-targets -- -D warnings` | exit 0 (`gates/clippy.log`) |
| the same with `--features fault-injection` | exit 0 (`gates/clippy-fi.log`) |
| `cargo test --locked --no-fail-fast` | exit 0: 5,211 passed, 0 failed, 7 ignored, 15 binaries (`gates/test-results.txt`) |
| the same with `--features fault-injection` | exit 0: 5,248 passed, 0 failed, 7 ignored (`gates/test-fi-results.txt`) |
| `scripts/check-xfx-identity.sh` | exit 0 (`gates/identity.log`) |
| `scripts/check-no-stubs.sh` | exit 0 (`gates/no-stubs.log`) |
| `scripts/check-no-secrets.sh` | exit 0 (`gates/no-secrets.log`) |
| `scripts/fxcap/test-sweep.sh` | exit 0: 54 + 26 tests (`gates/test-sweep.log`) |
| `scripts/fxcap/test-capture.sh` | exit 0: 53 tests (`gates/test-capture.log`) |

## Observed, not this unit's

Both owned by unit MCP-SEAMS (dispatcher, 2026-11-07; `docs/parity.md`):

- In a 2025 HTTP session fx posts `notifications/cancelled` when a call is
  cancelled (`p5-mcp-live`'s fx log, `http.txt`); xfx's head and base do not.
- fx answers a 2025 server's request on a POST's event stream with a POST of
  its own (`answerServerRequest`, `server.zig:1521-1530`); xfx's HTTP
  transport never has (`58b463c8:src/mcp/transport.rs:1318-1320`).
- A message given up on while it waits for a slot keeps its admission place
  in xfx until a slot frees and it is dropped unsent; fx frees the place at
  the cancel (`cancel`, `src/mcp/io/http.zig:252-263`). Opus's round-3
  SHOULD 1, routed by the dispatcher.

`p5-mcp-menu-old`'s snaps on the edge of the 10 s probe: unit MCP-HOLD
(dispatcher, 2026-11-07).

## Files

- `gates-ed.txt`, `gates-ed/` (MCP-Ed, at `7ff9864b`).
- `README.md`; `wire_compare.py`, `flips.py`, `mutants.py`,
  `mutants-d96603f9.py` (`python3 -I` each; the mutant scripts' TMPDIR is
  redacted to `<tmp>`). Round 1's `pairs.py` is gone: the sweep compares every pair
  (`sweeps/`).
- `snaps/fx/`, `snaps/xfx-head/`, `snaps/xfx-f3a859c/`, `snaps/xfx-base/`:
  every discover snap and `p5-mcp-wire-new`'s, as text.
- `wire.txt`, `http.txt`, `sweeps/` (at `d96603f9`); `gates.txt`, `gates/`,
  `mutants.txt`, `red-waiting-job.txt`, `ec-src-diff-stat.txt` (round 3);
  `gates-d96603f9.txt`, `mutants-d96603f9.txt` (round 2); `diff-stat.txt`,
  `src-diff-stat.txt`.

