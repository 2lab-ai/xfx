---
title: 0.0.16 delta -- P5-US-U4 receipt (the /usage screen, Gateway half) and U6 (/cost)
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, usage, security]
---

# P5-US-U4 + U6: the `/usage` screen and its command surface

Spec: `research/tui-phase5:.prd/tui-phase5/research/usage.md` §2, §3 and §5 U4
and U6. Oracle: fx v0.0.16 (`481104fd`, executable sha256 `8e6cda2efddb…`,
checked by capture.sh before and after every run): the screen
`src/ui/footer/usage_presentation.zig` (the Gateway half, `:19-878` and the
formatters `:1558-1625`), its state `src/core/session/usage_menu.zig`, the
hint lists `src/ui/footer/input_presentation.zig:794-813`, the keys
`src/core/app/app_input_runtime.zig:1196-1211,2127-2129,2177,2203-2244,2825-2832`,
the command `src/core/app/app_commands.zig:808-875,1172-1180` and the registry
row `src/builtins/commands.zig:454`. Base: `feat/p5-usage-screen` @ `510d4ee1`
(the US-U1..U3d stack, PR #57, on `main` @ `a592c3a2` with H3, PR #56). Unit
class **S**: the wiring decides when the Gateway key is sent.

## What changed

- `src/tui/usage_menu.rs` is rewritten: fx's `State` (`Screen`: period kept
  across opens, selection, window, detail) and every Gateway row
  `usage_presentation.zig` draws -- the heading and its status, the period,
  budget and model tables, `N more`, the empty texts, the detail screen's
  period, tokens and perf tables, narrow column dropping, fx's formatters
  (ties rounded as Zig rounds them, through `crate::usage::fixed`), the three
  reachable hint lists, and the key map. Its tests are
  `src/tui/usage_menu/tests.rs` (fx's `:1766-1996` ported, and xfx's own).
- `src/tui/shell.rs`: `/usage` opens US-U3's `usage::dashboard::Dashboard`
  with the question the runtime last published, and draws its `Snapshot`;
  `settle_usage` ticks it **only while the screen is up** and redraws on
  `redraw_due`; `close_usage` is the one way the screen goes (nine call
  sites: Escape, `/usage` itself before it opens again, and the seven other
  surfaces that take the slot) and closes the runtime with it; `usage_sign_in_changed` runs on
  `UiEvent::ProviderSelected` and on a resume onto another provider;
  `leave_by` quits the runtime. The ledger reader (`Reader`, `show_usage`,
  `read_usage`, the old `settle_usage`, `usage_read`) is gone; the ledger's
  writes are not touched (U5).
- `src/tui/worker.rs`: `UsageBoard` replaces `SessionSpend` (the old session
  scope's only reader is gone with it, and so is `began_ms`, which fed only
  that scope): the runtime publishes `report::question(config, endpoint)` --
  the endpoint every turn is sent to -- beside `/credits`' question.
- `src/usage/dashboard.rs`: `wall_clock()` is `usage::clock::now_ms` (it was
  `system_now_ms`), and `Dashboard::sign_in_changed(current)`, below.
- U6: `src/interactive.rs` drops the `/cost` alias and takes fx's summary
  `show your provider's usage and spend`; `/cost` is
  `✗ command: Unknown command. Try /help.` in the TUI and the line shell's
  unknown-command line there. `docs/parity.md`: the `/usage` row, the alias
  count (four, thirty-six names), the three usage library rows (now with a
  caller) and the two ledger rows (no longer read by `/usage`).

## Decisions (reversible, mine)

- **The no-sign-in line** reads `(set AI_GATEWAY_API_KEY)`, not fx's `(use
  /login, or set AI_GATEWAY_API_KEY)`: xfx has no /login (#88), and
  `VERCEL_OIDC_TOKEN` is unsupported for `/usage` (the user's 2026-11-06
  decision), so the one variable it reads is the honest pointer. Ledger
  divergence `usage-no-sign-in`, user-decision under #88.
- **`ProviderRejected`'s texts**: `the provider settings can't be used` /
  `(send a message to see why, then reopen /usage)`, in the shape of fx's
  `refresh_needed` line (`usage_presentation.zig:496`). It also covers an
  `XFX_GATEWAY_URL` the bearer rule refuses (`worker::usage_question`): no
  turn can be sent then either, and the turn's refusal names why.
- **A sign-in switch while `/usage` is closed cancels the old key's fetch**
  (`Dashboard::sign_in_changed`): a no-op when the published sign-in is the
  one held (same identity and origin); otherwise the fetch is retired (its
  admission swapped, its token cancelled) and the sign-in let go of, floors
  kept per key. With the screen up the shell opens it again with the new
  question instead, keeping the screen. fx sees a switch only at `open`
  (`usage.zig:596-601`).
- **Model names are spelled** through `mcp::terminal_safe` (the binding
  constraint), where fx writes `?` (`safeText`, `:367-385`); the name column
  is measured on the spelled text. A `docs/parity.md` row, no ledger row (no
  fixture has such a name).
- **Only three hint lists**: the Codex-periods and plan lists
  (`input_presentation.zig:798-805`) need sign-ins xfx has not (U9).
- **The receipt's xfx build** is the head's debug build stamped
  `XFX_BUILD_CHANNEL=release`: debug assertions on, so `XFX_E2E_USAGE_CLOCK_FILE`
  pins the clock, and a banner of 37 cells, so at 50 columns it does not wrap
  onto row 2 (the plain debug banner, 52 cells, does, and pushes every row
  down: ledger cause `xfx-debug-banner-wrap`).

## S properties, each with its test

| property | tests (fake key, loopback port or fake gateway, injected or pinned clock) |
|---|---|
| **S1** no request at launch, none while `/usage` is closed (the 15-minute timer included), none after quit; quit and a sign-in switch cancel the fetch in flight | shell, injected clock and a spawner that holds each fetch: `tui::shell::tests::usage_asks_nothing_at_launch_while_closed_or_after_quit` (an hour of passes closed: 0 fetches; reopened past the floor on a counting loopback port, one fetch, which `r` and the timer do not double; after `/quit` that fetch, run, connects to the port 0 times -- the first, aimed at a closed port and retired by the reopen, is run too; `open`/`refresh` after it start nothing), `tui::shell::tests::a_sign_in_switch_cancels_the_fetch_the_old_key_has_in_flight` (`UiEvent::ProviderSelected` with `/usage` closed: the held fetch, run, connects 0 times; with it up the screen reopens on the new key); runtime: `usage::dashboard::tests::a_sign_in_switch_while_closed_cancels_the_fetch_unless_the_sign_in_is_the_same` (loopback gateway: the held request is hung up, 1 request total, no cache file for the old key, the same key keeps its fetch); debug pty with `XFX_E2E_USAGE_CLOCK_FILE` and `FakeGateway`: `tests/tui.rs` `usage_reads_the_gateways_report_only_while_open_and_draws_its_names_inert` (0 report GETs at launch; 3 on open, `authorization: Bearer <fake key>`; the clock moved an hour with the screen closed: still 3) and `quitting_with_a_usage_fetch_in_flight_cancels_it_and_exits_promptly` (held report; Esc, Ctrl-D: exit under 5 s, 1 GET, no cache file) |
| **S2** a hostile model name (ESC, CSI, OSC, C1, bidi, zero-width, line break) is inert on the table and the detail screen | `tui::usage_menu::tests::a_hostile_model_name_is_drawn_inert_on_the_table_and_on_the_detail_screen` (7 names x 9 widths x both screens: no raw ESC, BEL, C1, U+202E, U+200B, CR or LF in any row; every escape in a row is the palette's own CSI; the spelled name on the row), `the_name_column_is_measured_on_the_names_as_drawn`; pty: the report's `evil/<ESC>]0;pwned<BEL><ESC>[2Jname` is on screen as `evil/\x1b]0;pwned\x07\x1b[2Jname` and neither sequence is in the terminal's byte stream |
| **S3** no screen shows the key; only `API key (AI_GATEWAY_API_KEY)` is named | `tui::shell::tests::no_usage_screen_shows_the_key` (a real `Question` with the fake key: the failure heading, the table and the detail screen at 120, 50 and 40 columns, every band row and the `Debug` of the screen), `tui::usage_menu::tests::no_screen_names_the_key` (every status, both screens, 9 widths); pty: `!settled_text().contains(TEST_KEY)`; the 30 committed gateway logs name only `headers=authorization`, and no capture holds the fake key |

Every "now" is `usage::clock::now_ms` (`dashboard::wall_clock`); the screen reads
only `Snapshot::now_ms`. Arithmetic on it: `ago_text` takes
`now.saturating_sub(at)` and floors with `div_euclid`; the budget's reset day
is `report::utc_day` (`div_euclid`); no difference is cast `as u64`
(`tui::usage_menu::tests::any_clock_draws_without_overflowing`: `i64::MIN`,
`i64::MAX`, -1 and 0 against each other).

## RED

On the base the `/usage` screen is Phase 4's ledger dashboard: H3's pairs
(`../fxcap-h3/pairs.txt`, the same five scenarios at the three sizes, xfx at
`b8f6994c`, whose `src/tui/usage_menu.rs` and `/usage` wiring are
`510d4ee1`'s) are 192 pairs, 7 identical, none of them a `/usage` screen:
`Usage  [30 days]  7 days  24 hours  Session` / `Tracking has not started`
against fx's `usage: API key (AI_GATEWAY_API_KEY) … updated just now` and its
tables, and no `/v1/report` GET in xfx's 15 gateway logs. `red-base.txt` is
the base binary (`510d4ee1`, debug) on `p5-usage-cached` 40x120, taken here.
All 21 `p5-usage-cached` 40x120 pairs differ (3 to 16 rows each; `closed`
the 3), and the base asks no `/v1/report`.

## GREEN: the pairs

fx v0.0.16 and xfx (the head's debug build, release channel: above), the five
H3 scenarios at 40x120, 40x50 and 24x80, through `scripts/fxcap/sweep.sh
--only <the five> --size <size> --jobs 2`, classified by the ledger
(`scoreboard-<size>.md`); `pairs.txt` is `cells.py`'s reading of every pair
(row 1 left out).

| scenario | snaps | 40x120 | 40x50 | 24x80 |
|---|---|---|---|---|
| `p5-usage-cached` | 21 | 21 same | 21 same | 21 same |
| `p5-usage-budget` | 14 | 14 same | 14 same | 14 same |
| `p5-usage-many` | 5 | 5 same | 5 same | 5 same |
| `p5-usage-nokey` | 4 | 4 approved (`usage-no-sign-in`) | 4 approved | 4 approved |
| `p5-usage-turns` | 20 | 18 same, 2 `stats-bytes` | 18 same, 2 `stats-bytes` | 18 same, 2 `stats-bytes` |
| **all** | 64 | 58 / 4 / 2 | 58 / 4 / 2 | 58 / 4 / 2 |

192 pairs: 174 the same cell for cell, 12 an approved divergence, 6 an
unresolved defect the ledger already names.

- **`usage-no-sign-in`** (new, user-decision, #88): `nokey`'s row 6, fx `  (use
  /login, or set AI_GATEWAY_API_KEY)` / xfx `  (set AI_GATEWAY_API_KEY)` on
  `open` and `r`, and on `start` and `closed` the hint row's `run /login · `
  / `` run `xfx setup` · `` (the existing `status-panel-sign-in` text, scoped
  here to this scenario).
- **`stats-bytes`** (existing cause, defect, unowned): `turns`' `stats` and
  `credits` snaps hold `* stats: ansi_bytes=N`, each product's own byte count
  to the terminal (40x120: fx 15717, xfx 8700); everything else on those
  snaps is the same.

What each product asked (`gateway.txt`): every scenario at every size sends the
same `/v1/report` GETs in the same order -- `cached` 6 (today, at open and on
each `r`), `budget` 2, `many` 1, `nokey` 0 (`r` there sends nothing), `turns` 4
(the 404, then today, `2026-10-06..2026-10-11`, `2026-10-01..2026-10-11`),
each with `headers=authorization` only.

The 40x50 sweep's fx inventory did not parse (`a slash row is not a command
row`: `/help` wraps at 50 columns); its scoreboard was rescored
(`sweep.sh --rescore`) with the 40x120 run's inventory copied in, which feeds
only its Coverage section, not a pair's verdict.

## Parity sheets (`sheets/`, `render.py` + `parity-sheet.py`)

Each sheet is fx above, xfx below, drawn from the committed `.ansi` by
`render.py` (cells as `cells.py` reads them; Menlo, which has no 𝒇, so the
banner's glyph is a box on both). I read each one; the gaps:

| sheet | snap | gaps |
|---|---|---|
| `table.png` | `cached` 40x120 `open` | none but the banner (`𝒇x v0.0.16` / `x𝒇x v0.1.0`) |
| `detail.png` | `cached` 40x120 `detail` | banner only |
| `budget-50.png` | `budget` 40x50 `open` (heading `API key (AI_…WAY_API_KEY)  updated just now`, no `resets`, model table `spent  requests`) | banner only |
| `budget-50-detail.png` | `budget` 40x50 `detail` (narrow tokens table) | banner only |
| `many-bottom.png`, `many-bottom-24.png` | `many` `bottom` at 40x120 and 24x80 (`vendor-02` … `❯ vendor-11`, `2 more  $0.4735`) | banner only |
| `nokey.png` | `nokey` 40x120 `open` | the message row, `usage-no-sign-in` |
| `r-429.png` | `cached` 40x120 `r-429` (the failure in normal weight) | banner only |
| `u6-typed.png`, `u6-cost-typed.png`, `u6-cost.png` | `turns` 40x120 `typed`, `cost-typed`, `cost` | banner only |

## U6, side by side (`p5-usage-turns`, 40x120, all three pairs the same)

| snap | fx and xfx |
|---|---|
| `typed` | `/usage` in the composer, the slash popup `Commands 1` / `/usage   show your provider's usage and spend   Account` |
| `cost-typed` | `/cost` in the composer, no popup |
| `cost` | `✗ command: Unknown command. Try /help.` |

## Mutants (`mutants.txt`)

Each on a throwaway `git clone --no-local` of this branch, with its own target
directory under a short `TMPDIR`, the file restored after each; the tests named
run with `--exact`. All six are killed.

| mutant | killed by |
|---|---|
| M1 `settle_usage` ticks with no screen up, **and** `close_usage` leaves the runtime open | `usage_asks_nothing_at_launch_while_closed_or_after_quit` (the 15-minute timer fetched while closed) |
| M2 `leave_by` does not quit the runtime | `usage_asks_nothing_at_launch_while_closed_or_after_quit` (the held fetch, run after `/quit`, connected) |
| M3 `ProviderSelected` does not tell the runtime | `a_sign_in_switch_cancels_the_fetch_the_old_key_has_in_flight` |
| M4 `sign_in_changed` keeps the held fetch | that test and `a_sign_in_switch_while_closed_cancels_the_fetch_unless_the_sign_in_is_the_same` |
| M5 a model name drawn raw on the table | `a_hostile_model_name_is_drawn_inert_on_the_table_and_on_the_detail_screen` |
| M6 the detail heading's name drawn raw | the same |

M1 is the two guards together on purpose: either one alone holds (the shell
ticks the runtime only while the screen is up, and the runtime starts its
timer only while open), so each alone is not observable.

## Before and after: the CI sweep (`sweep-report.md`)

`scripts/fxcap/ci-sweep.sh --base 510d4ee1 --fx <fx v0.0.16> --pin v0.0.16
--xfx <head, debug> --base-xfx <510d4ee1, debug> --jobs 2`, as `sweep.yml`
runs it, at `aa49111f` (this branch with the draft receipt, so the five
scenarios are selected as the receipt's): 15 scenarios (the 10 core and the
five), 117 pairs, 109 pass at the head, 0 new, **0 regressions**, exit 0. Not
passing at the head, and not at the base either: `h1-resize` `small` and
`large` (core, known), `p5-usage-nokey`'s four (approved, `usage-no-sign-in`)
and `p5-usage-turns`' `stats` and `credits` (`stats-bytes`, four base runs
each).

## `docs/parity.md` and the ledger

- `docs/parity.md`: the `/usage` row (rewritten: v0.0.16's screen, U6's
  /cost, what is asked when, xfx's three differences); the alias paragraph
  (four aliases, thirty-six names, the line count kept so the ledger's line
  citations stand); `AI Gateway usage reports`, `… dashboard runtime` (with
  `sign_in_changed`) and `… usage cache` (each now with a caller); `generation
  usage and billing` and `/usage tracking start` (no longer read by `/usage`;
  U5 retires them).
- `scripts/fxcap/parity-ledger.toml`: divergence `usage-no-sign-in`
  (user-decision, #88, expires 2026-12-04). `stats-bytes` is the existing
  cause.

## Gates

At `aa49111f` (the code is the head's: the commits after it touch only this
receipt), short `TMPDIR`, `FX_AUTO_UPGRADE=0`.

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings`, and `--features fault-injection` | ok, ok |
| `cargo test --locked --no-fail-fast` | 14 result lines ok: 4848 passed, 0 failed |
| `cargo test --locked --features fault-injection --no-fail-fast` | 14 ok: 4877 passed, 0 failed |
| `scripts/check-xfx-identity.sh`, `check-no-stubs.sh`, `check-no-secrets.sh` | ok (9364 files), ok, ok (9364 files) |
| `cargo build --locked --release`; `… --features fault-injection --target-dir target/faulty` | ok, ok |
| `scripts/check-usage-clock.sh` | ok: debug honoured, release and release + fault-injection ignored |
| `scripts/smoke.sh target/release/xfx <dir>` | 47 checks, 0 failures; section 0: `XFX_E2E_USAGE_CLOCK_FILE` not in the release bytes |
| `scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx <dir>` | 37 scenarios + the oracle, 1111 checks, 0 failures |
| `scripts/check-preview-contract.sh` | ok |
| `scripts/fxcap/test-capture.sh`, `test-sweep.sh` (the ledger with `usage-no-sign-in`) | 36 tests OK, 24 tests OK |
| `scripts/fxcap/ci-sweep.sh --base 510d4ee1` | 117 pairs, 109 pass, 0 new, 0 regressions, exit 0 |

`now_ms()` is reached now, so the debug build carries the variable's name (1
occurrence in `target/debug/xfx`); `target/release/xfx` and
`target/faulty/release/xfx` carry it 0 times, which is what smoke.sh's section
0 holds the shipped bytes to.

## Files

- `cap/<size>/{fx,xfx}-p5-usage-<scenario>-<size>-<label>.{txt,ansi}`: the
  sweeps' snaps; `…-gateway.log`: fakegw's log; `…-end.files`: HOME after the
  run. Redacted through `scripts/fxcap/redact.py` (`Redactor.write`, which
  refuses a text with a local path left in it).
- `pairs.txt`: `cells.py`'s reading of every pair, the differing rows quoted;
  `gateway.txt`: the `/v1/report` GETs of each pair, compared.
- `scoreboard-<size>.md`: the three sweeps' scoreboards; `sweep-report.md`: the
  CI sweep's report.
- `sheets/*.png`: the parity sheets; `render.py`: what draws a capture.
- `red-base.txt`: the base on `p5-usage-cached` 40x120; `mutants.txt`: the six
  mutants.
