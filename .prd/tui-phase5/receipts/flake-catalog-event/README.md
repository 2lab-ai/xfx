# flake-catalog-event: the title case's extra `CatalogUnread` (registry #19, 19th sighting)

`tui::worker::tests::a_title_that_comes_back_between_turns_is_written_and_then_shown`
failed in a dispatcher gate on a commit that changed no src, in both
`cargo test` and `cargo test --features fault-injection`, with one extra
event ahead of the expected one:

```
left: [CatalogUnread { provider: Gateway, retry: None, attempted: .. }, SessionTitled("Between turns")]
right: [SessionTitled("Between turns")]
```

The cause is the test's fixture, not the product. The fix changes
`src/tui/worker.rs`'s test module only. Line numbers are at `main` @
`884497c0` unless named.

## Commits

| commit | what |
|---|---|
| `8f82e4a3` | RED: `a_title_runtime_starts_no_catalog_read_that_the_loop_would_tell_the_band` |
| `b03074c0` | the fix: `titled_runtime`'s catalog answered, by an in-process stand-in |
| (this one) | this receipt |

## The trace (`trace.txt`: each hop's code, excerpted at the base)

1. **The fixture holds a Gateway key** (H1, `worker.rs:14785`). `Runtime::new`
   builds the selector from that configuration (H2, `:1774`).
2. **A key makes the selector's catalog the Gateway's, unread** (H3,
   `model.rs:1465-1471`; H4, `catalog_for`, `model.rs:991-999`). Its
   endpoint is `Endpoint::from_process()` (H5, `gateway/mod.rs:357-361`):
   `XFX_GATEWAY_URL` from the *process* environment, not the configuration's
   map. Under `cargo test` it is unset, so the endpoint is the default
   `https://ai-gateway.vercel.sh/...` (H5b, `gateway/mod.rs:45`).
3. **`turn_loop`'s first tick begins reading it** (H6, `worker.rs:3291`,
   run before the `select!` on every tick) through `begin_catalog_read` (H7,
   `:4109-4113`) and `begin_load` (H8, `model.rs:1555-1567`): `NotLoaded`
   and nothing pending, so a `PendingLoad` starts. It is a first-tick ask,
   not a timer or a retry deadline.
4. **The read runs on a thread of its own** (`xfx-catalog`, H9,
   `model.rs:1320-1359`): an authenticated `GET` to the Gateway, then the
   telling, each raced against the stop. A failed read becomes
   `CatalogUnread` (H10, `worker.rs:3205-3224`), sent by
   `bridge::send_terminal` into the case's own events channel (H7,
   `:4131`).
5. **The title path, meanwhile** (H12, `:3351`, `:3401-3404`): the desk
   answers at once (`TitleDesk::answering`, H12c, `titles.rs:430-442`), the loop
   writes `session_renamed` and sends `SessionTitled`. The stopper polls the
   log every 5 ms and then sends `Shutdown` (H13, `:15408-15417`); the
   biased control arm takes it (`:3302`, `:3407`), `turn_loop` returns, and
   dropping the `Runtime` drops the `PendingLoad`, which sends the stop and
   joins the thread (H14, `model.rs:1289-1297`). Nothing is told after that.
6. **So the extra event lands iff the read answers inside that window.** The
   clock is the wall-clock time of the read's round trip on the
   `xfx-catalog` thread against the title path's time to `Shutdown`.

Where the failure's read went: through a loopback logger set as
`HTTPS_PROXY` (`bearer_rules` honours an https proxy from the environment,
H15, `gateway/mod.rs:120-141`), every run of either case logged
`CONNECT ai-gateway.vercel.sh:443` (`red.txt`, last section).

The gate's `retry: None` rules out a transport failure, which reads
`Some(Unreachable)` (H11b, `model.rs:953-962`); `None` is what an HTTP
status other than 429 or 5xx gives (`CatalogRetry::for_status`, H11,
`model.rs:129-135`), or a body that is not a catalog. That the gate's read
reached the remote host and was refused there (a 401 for the fake key `key`)
is an inference from that one field [추정]: no body was seen. On `main`,
every run of the case begins this read; `red.txt`'s `CONNECT` lines show
where it goes.

**Not a product defect.** Reading the catalog in the background at launch
and telling the band when it lands is upstream's warm-up
(`startModelCacheWarmup`; `pollLoadTransition`), cited in the code at H6 and
H7. The fault is a unit-test fixture that leaves a live remote catalog in a
runtime whose events a case counts.

## The window, measured (`window.txt`)

Through a loopback logger that holds each answer *d* ms (a stand-in for the
round trip; nothing leaves the machine), 50 runs of the case on the RED tree:

| stand-in round trip *d* | idle: failed / 50 | 18 `yes` spinners: failed / 50 |
|---|---|---|
| 0 ms (403 at once) | 50 | 50 |
| 10 ms | 5 | 1 |
| 30 ms | 0 | 0 |
| 100 ms | 0 | 0 |

So, alone on this machine, the case loses only when the read answers within
about 10 ms of the loop's start: past 30 ms it passed 100 of 100, idle or
beside 18 spinners. How long the real read takes here was not measured, to
keep every run on loopback. That the gate's two full suites (load average
7-10) stretched the title path past the real round trip is a hypothesis
[가설]; the spinners alone did not.

## RED (`red.txt`)

`a_title_runtime_starts_no_catalog_read_that_the_loop_would_tell_the_band`
takes the same fixture as the failing case (`ready`, then a title asked),
runs the loop's first tick (`begin_catalog_read`), lets whatever read it
began answer (`settle_load`), and asserts that the band was told nothing.
That is the race's late side made to win, so it fails every time on the
RED tree: 3/3 in `red.txt`, 50/50 idle and 50/50 loaded in `loops.txt`. The
original case fails the same way whenever the read answers fast (3/3 in
`red.txt`).

## The fix

`titled_runtime` now:
- asks an in-process stand-in (`Answering`, no rows) instead of the
  Gateway, so no path from this runtime can open a socket -- the
  `CatalogAsks::Reading` reader a `/model` mid-turn would use included;
- pins its catalog as loaded (`set_catalog_for_test(Loaded(vec![]))`), so
  `begin_load` returns at once (`model.rs:1560-1562`) and the first tick
  starts nothing.

The exact-list assertion is unchanged. Pinning `Loaded(vec![])` tells the
UI the same thing an unread or absent catalog does (`from_state` and
`catalog_event_of`, H10: `NotLoaded`/`Unavailable` -> no rows), and no title
case reads the catalog's rows.

## Loops (`loops.txt`)

Each run is one invocation of the lib test binary over the 12 cases that
use `titled_runtime` (the RED case among them), `--test-threads` at the
default, the loop under the suite lock; "loaded" puts one `yes` per core (18)
beside it, started and killed by `loaded.py`. "Forced" sets `HTTPS_PROXY` to
the logger answering 403 at once: the fastest answer a read can get, which is
the schedule that loses.

| binary | schedule | idle: passed / 50 | loaded: passed / 50 |
|---|---|---|---|
| RED (`8f82e4a3`) | forced | failing case 0, RED case 0, the other 10 cases 50 | the same: 0, 0, 50 |
| GREEN (`b03074c0`) | forced | all 12 cases 50 | all 12 cases 50 |
| GREEN (`b03074c0`) | plain environment, no proxy | all 12 cases 50 | all 12 cases 50 |

The GREEN forced loops logged no `CONNECT` (`connects= 0` in `loops.txt`): the
fixed fixture opens no socket. The RED rows are the forced schedule, not the
field rate; the field rate is the window table above.

## Class sweep (`sweep.txt`)

- **`turn_loop`'s callers** (7 in tests, plus `spawn`'s thread): six run on
  `recording()`/`config()`, whose configuration has no credential, so
  `catalog_for` returns `None` (`model.rs:997`), the catalog is `Unavailable`
  and `begin_load` returns at once (`model.rs:1560-1562`). Three of the six
  assert an exact list (`a_reset_that_drops_the_conversation_takes_its_reply_with_it`,
  `a_reset_between_turns_drops_what_waits_and_then_says_the_conversation_is_gone`,
  `between_turns_a_rename_is_written_into_the_session_it_was_typed_in`);
  every input they give the loop is a control message queued before it runs,
  and no title desk answers on that fixture, so nothing timed can land.
  `spawn`'s two test callers use `config()` too. The seventh is the flake.
- **Every worker test that drains `UiEvent`s with `try_recv`** (35):
  10 have no `Runtime`, 15 a `Runtime` with no fetcher, 4 an in-process fake
  fetcher whose catalog event is what they assert, 1 no `Runtime` at all, and
  5 are on `titled_runtime` -- fixed here; only the flake ran `turn_loop`.
- **Bridge and the other tui files:** `bridge.rs` drains nothing; the
  others drain the prompter's, the shell's or their own channels and never
  build a selector.
- **Mechanically** (`proxy-sweep.txt`): the whole lib binary at the fix,
  through the logger: 4368 passed, 0 failed, and no `CONNECT` to the
  Gateway. Two went to `w.example:443`, from
  `mcp::command::tests::a_header_or_env_value_is_never_said_back_and_never_planted_in_the_project`:
  an MCP probe of a name reserved never to resolve, asserting no event list.
  Outside this class; reported, not changed.
- Fixed: 1 fixture (`titled_runtime`). Latent, not racing today: a keyed
  fixture's selector is rebuilt with the remote catalog by `adopt`
  (`worker.rs:2885`) -- `a_prompt_held_for_a_switch_asks_of_the_conversation_it_will_run_in`
  calls it on `titled_runtime` -- and `a_runtime_that_can_open_one` holds a
  key; no case on either reads the catalog or runs `turn_loop`.

## Gate at the fix commit (`gate.txt`)

Every step exited 0 at `b03074c0`, 2026-11-08 05:59-06:19Z; the heavy steps
each held a slot of the machine's suite lock.

| step | result |
|---|---|
| `cargo fmt --check` | 0 |
| `cargo clippy --locked --all-targets -- -D warnings`, and with `--features fault-injection` | 0, 0 |
| `cargo test --locked --no-fail-fast` | 5280 passed, 0 failed |
| `cargo test --locked --features fault-injection --no-fail-fast` | 5317 passed, 0 failed |
| `check-no-stubs`, `check-no-secrets`, `test-check-no-secrets`, `test-gate-env`, `check-xfx-identity`, `check-preview-contract`, `test-preview-contract`, `check-tui-preflight-cost`, `check-usage-clock` | 0 each |
| `fxcap/test-sweep.sh` | OK |
| `fxcap/test-capture.sh` | 59 tests, OK (the known `test_capture.py:394` race did not fire) |
| release build, fault-injection release build | 0, 0 |
| `smoke.sh` | 47 checks, 0 failures |
| `smoke-tui.sh` | 37 scenarios + the oracle, 1127 checks, 0 failures |

The RED case and the failing case passed in both test runs (`gate.txt`). The
receipt commit after `b03074c0` adds only this directory; the tracked-file
checks were run again on it.

## Files

- `trace.txt` -- each hop's code, excerpted from the base with line numbers.
- `red.txt` -- the RED case and the failing case, 3 runs each on the RED
  tree, and the logger's `CONNECT` lines.
- `window.txt` -- the failing case against stand-in round trips of 10, 30
  and 100 ms.
- `loops.txt` -- the 50-run loops before and after, idle and loaded.
- `sweep.txt` -- the class sweep, case by case.
- `proxy-sweep.txt` -- the whole lib binary at the fix through the logger.
- `gate.txt` -- the gate at the fix commit.
- `connect_logger.py`, `loop.sh`, `loaded.py`, `pertest.sh` -- the harness:
  the logger (logs a connection's first line, answers 403, optionally after
  a delay, forwards nothing), the loop, the spinners, and the per-case count.

Every file here went through `scripts/fxcap/redact.py`.

## Disclosed

Before the logger bound an ephemeral port, three runs of the failing case
went to a fixed loopback port that another local service already held; the
three `CONNECT ai-gateway.vercel.sh:443` lines went to that service, which
refused them. A `CONNECT` carries no credential (the key would follow only
inside TLS after a `200`), and nothing was forwarded.
