---
title: P5-H3 receipt -- the capture harness for /usage (api_key scope)
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, usage, harness]
---

# H3 receipt -- the capture harness for `/usage`

Unit P5-H3, epic 3 "Usage": what US-U4's `/usage` screen pairs need, `api_key`
scope only. Base `main` `b8f6994c` (H1 is #42 there). Oracle fx v0.0.16
(`481104fd`, executable sha256 `8e6cda2efddb…`, checked by capture.sh before
and after every fx run: 15 of 15 `guards:` lines in `runs.txt`'s runs). xfx is
the debug build of this head (`target/debug/xfx`). No xfx `/usage` code: that is
U4.

## What H1 already gave, and what this adds

H1 already had: `GET /v1/report` answering `{"results":[rows]}` with rows by
`byRange` (`<start_date>..<end_date>`), `byStart` or `rows`; per-route
`status`, `body` and `delayMs`; one log line per request, unknown GETs
included, with the path, the query and which of `authorization`,
`x-vercel-ai-gateway-team`, `chatgpt-account-id`, `x-userid` were present
(names only); the `clock` verb for fx. This unit adds:

1. **xfx's usage clock**, `src/usage/clock.rs`: `crate::usage::clock::now_ms()`
   reads `XFX_E2E_USAGE_CLOCK_FILE` the way fx reads `FX_E2E_USAGE_CLOCK_FILE`
   (`src/core/app/usage_host.zig:20-22,205-219@481104fd`: an absolute path, at
   most 32 bytes, one integer of ms, blanks trimmed; the system clock for
   anything else), **only in a build with debug assertions** (below).
   capture.sh's `clock` verb now launches xfx with that variable naming the
   same file it gives fx.
2. **fakegw**: a bare `status` (no `json`/`body`) on `/v1/report` and
   `/ai-gateway/usage-report` now answers `fake status <n>`, as fakegw.ts's
   header always said. Before, a 429 there carried a 200's `{"results":[]}`.
3. **Tests** (`scripts/fxcap/test_capture.py`):
   `FakeGateway.test_a_fake_key_and_team_reach_no_line_of_the_log_and_no_body`
   sends `Bearer fake-key-h3-not-real` and a team header on ten requests (the
   report answered, held and refused, the budget, the catalog, the credits, a
   generation, the Codex catalog, an unknown path, a completion) and finds
   neither value, nor `Bearer`, in the log or the kept body, while it finds
   eight GET lines that name both headers. `test_it_listens_on_loopback_only`
   reads fakegw's LISTEN sockets with lsof: `127.0.0.1:<port>` and nothing
   else. `Verbs.test_clock` covers both binaries, each with its own variable
   only, and neither without a `clock` line.
4. **Scenarios** `p5-usage-cached`, `-budget`, `-many`, `-nokey`, `-turns`
   (they were on `main` as the research drafts, unchanged): each pins the clock
   at fx's e2e time `1791819000000` (2026-10-12 15:30 UTC,
   `tests/e2e/usage-fakes.ts:10@481104fd`), answers the report through a
   `usage` file, and seeds xfx's cache too:
   `xfx: homefile .xfx/usage-cache/<id>.json`, the path and identity US-U2
   reads (fx's formula, `cache.zig:22-35@481104fd`). The cache fixtures are
   regenerated for the pinned day, and `p5-usage-mkcache.py` now writes them
   for a clock it is given (default the same), stamping every part five
   minutes before it, so the files are the same whatever day they are
   written or read.
5. **Captures**: the five scenarios at 40x120, 40x50 and 24x80 on fx v0.0.16
   and on xfx, under `cap/<size>/`, with each run's fakegw log and HOME
   listing.

No `docs/parity.md` row changes: the clock is a test-only seam, not a surface.
Where xfx differs from fx -- fx honours its variable in every build, xfx only
with debug assertions -- is said in `src/usage/clock.rs`, capture.sh's `clock`
entry and here.

## How the clock is gated

The reader (`pinned`), the variable's name (`CLOCK_FILE_ENV`) and the branch
that consults them in `now_ms_with` are `#[cfg(debug_assertions)]`. A
`cargo build --release` -- every binary ci.yml, release.yml and preview.yml
publish; `[profile.release]` sets only `strip` -- compiles none of them, so
`now_ms()` there is `system_now_ms()` whatever the environment holds. The
debug build is what capture.sh runs by default and what the CI sweep builds
(`cargo build --locked`, sweep.yml).

Proof: `usage::clock::tests::the_pinned_clock_is_read_only_with_debug_assertions`
writes `1791819000000` to a file and names it. Under `cargo test` it asserts
the pinned value is read (moved, trimmed, cut at 32 bytes, refused through a
relative path) and prints `usage clock: honoured (debug assertions)`; under
`cargo test --release` it asserts the system clock (within a minute) and
prints `usage clock: ignored (release)`. `scripts/check-usage-clock.sh` runs it
three times -- debug, release, release with `fault-injection` (the faulty
build the restoration matrix drives) -- and requires `1 passed` and the marker
each time, so an empty filter cannot pass. ci.yml runs it after
`check-tui-preflight-cost.sh`, whose release test builds it reuses. This head:

```
check-usage-clock: debug: honoured (debug assertions)
check-usage-clock: release: ignored (release)
check-usage-clock: release, fault injection: ignored (release)
check-usage-clock: ok
```

(The release half runs `target/release/deps/xfx-…`, `Finished release
profile [optimized]`.)

**Mutant** (`red-clock-mutant.txt`): every `#[cfg(debug_assertions)]` removed
from `src/usage/clock.rs`, in a `git clone --no-local` copy with its own target
directory. `check-usage-clock.sh` there: debug still `honoured`; release and
release + fault-injection fail at `clock.rs:114`, `a release build ignores
XFX_E2E_USAGE_CLOCK_FILE: read 1791819000000`; `2 problem(s) found`, exit 1.

## RED on the base harness

`red-harness.txt`: the three new harness tests against `b8f6994c`'s
capture.sh and fakegw.ts. `test_clock` fails on xfx (`clock:::`: no file);
the key test fails on `(429, b'{"results":[]}') != (429, b'fake status 429')`;
the loopback test passes (a guard that already held).

## The scenarios on fx v0.0.16

Every report read is `GET /v1/report?start_date=…&end_date=…&group_by=model&api_key_id=self`
with `headers=authorization`: the `api_key` scope sends no team
(`.team = if (job.scope == .member) job.team else null`, `src/usage/usage.zig:758@481104fd`). `fake-key-not-real` is in none of the 30
committed gateway logs, nor anywhere in this receipt; those logs hold 39
`request GET /v1/report` lines.

| scenario | what is on screen (snap labels) | report GETs per run |
|---|---|---|
| `p5-usage-cached` | `updating` (the cache drawn, `updating…`), `open` (`updated just now`, today from the report: 16 requests, $0.4735); `down`, `detail`, `detail-down`, `detail-tab`, `back`; periods `tab-today`, `tab-7d`, `right-month`, `left-7d`, `btab-today`; `cj`, `ck`; `r` against each failure: `r-429` `rate limited, try again shortly, last updated just now`, `r-401` `AI Gateway refused this sign-in, …`, `r-500` `AI Gateway error, …`, `r-bad` (a 200 that is not JSON) `unexpected AI Gateway response, …`; `r-updating`, `r`; `closed` | 6: today only (the cache holds today's finished days), at open and on each `r` |
| `p5-usage-budget` | the same with `budget  size  used  avail  use%  resets` / `user  $316.00  $65.97  $250.03  21%  nov 1` (the month after the pinned clock's), kept through a fetch that brings no budget | 2 |
| `p5-usage-many` | `open`, `bottom` (`vendor-02` … `❯ vendor-11`, `2 more`), `last` (the window at the end: `❯ fake/model-a`), `top`, `closed` | 1 |
| `p5-usage-nokey` | `usage: not signed in to AI Gateway` / `(use /login, or set AI_GATEWAY_API_KEY)` at `open` and after `r` | 0: `r` on that screen sends nothing |
| `p5-usage-turns` | the two billed turns; `open` with `usage off`: the message-only `usage reports aren't available for this team` / `(press r to retry)`, unchanged by ↓, Enter, Tab, Shift-Tab, →, ←; `r` with the report answered: `fetching` (`fetching…` / `fetching usage from AI Gateway`), then `r`, `r-today`, `r-7d` (the report's counts of the two turns: today $0.4335, 2 requests); `/cost`, `/usage 7d`, `/stats`, `/credits` | 4: the 404, then on `r` today, `2026-10-06..2026-10-11` and `2026-10-01..2026-10-11` in that order, each held 1.5 s |

At 40x50 the heading drops a failure's status when it does not fit
(`composeHeading`, `usage_presentation.zig:389-416@481104fd`): `r-429` … `r-bad`
read `usage: API key (AI_GATEWAY_API_KEY)` alone, and the message-only
`p5-usage-turns` `open` keeps only `(press r to retry)`. Those 5 are the only
misses when 17 states' texts are looked for at each size (46 of 51 found).

## Pairs: xfx beside fx

`pairs.txt`, by `scripts/fxcap/cells.py`'s reading (row 1 left out): 192
pairs, 7 identical, and none of those 7 is a `/usage` screen (`p5-usage-turns`
`launch` and `turns` at 40x120 and 24x80, `esc` at 40x50; `p5-usage-many`
`closed` at 40x120 and 24x80). The rest differ because xfx still draws
Phase 4's ledger dashboard (`Usage  [30 days]  7 days  24 hours  Session` /
`Tracking has not started`) and reads no report: its 15 gateway logs have no
usage line. That is US-U4's RED.

## CI sweep

`sweep-report.md`: `scripts/fxcap/ci-sweep.sh --base b8f6994c --fx <fx v0.0.16>
--pin v0.0.16 --xfx <this head, debug> --base-xfx <b8f6994c, debug> --jobs 2`,
run here as sweep.yml runs it. 15 scenarios (the 10 core, and the five
p5-usage ones as changed, used and named by this receipt), 117 snap pairs at
40x120: 50 pass at the head, 14 new, **0 regressions**, exit 0. The p5-usage
pairs that do not pass did not pass at the base either (or are new labels):
US-U4's RED. `p5-usage-turns`'s `arg`, `cost`, `cost-typed`, `credits`, `esc`
and `stats` were re-run 3 times on each build (a ledger `nondeterminism`
cause covers them) and fail on both.

## Gates

Run once at `5a2f4c2d` (this head before the receipt-only commit), `TMPDIR` a
short path under the harness temp folder.

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| `cargo clippy --locked --all-targets --features fault-injection -- -D warnings` | ok |
| `cargo test --locked --no-fail-fast` | 14 result lines ok: 4556 passed, 0 failed |
| `cargo test --locked --features fault-injection --no-fail-fast` | 14 ok: 4577 passed, 0 failed |
| `scripts/check-xfx-identity.sh`, `check-no-stubs.sh`, `check-no-secrets.sh` | ok, ok, ok |
| `scripts/check-usage-clock.sh` (new) | ok: debug honoured, release and release + fault-injection ignored |
| `cargo build --locked --release`; `… --features fault-injection --target-dir target/faulty` | ok |
| `scripts/smoke.sh target/release/xfx <dir>` | 46 checks, 0 failures |
| `scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx <dir>` | 37 scenarios + the oracle, 1107 checks, 0 failures |
| `scripts/check-preview-contract.sh` | ok |
| `scripts/fxcap/test-capture.sh` | 36 tests OK |
| `scripts/fxcap/test-sweep.sh` | 24 tests OK |
| `scripts/fxcap/ci-sweep.sh` vs fx v0.0.16 | 0 regressions (above) |

## Files

- `cap/<size>/{fx,xfx}-p5-usage-<scenario>-<size>-<label>.{txt,ansi}`: the
  snaps; `…-gateway.log`: fakegw's log, every request; `…-end.files`: HOME
  after the run (`p5-usage-cached`, `-budget`, `-turns`). Request bodies are
  not kept. Snaps and listings were copied only if they held no local path
  (none did); the logs went through `scripts/fxcap/redact.py`.
- `runs.txt`: the 30 runs, each `exit=0`.
- `sweep-report.md`: the CI sweep's report (redacted by ci_sweep.py).
- `pairs.txt`: each pair's differing row count.
- `red-harness.txt`, `red-clock-mutant.txt`: the REDs above.
