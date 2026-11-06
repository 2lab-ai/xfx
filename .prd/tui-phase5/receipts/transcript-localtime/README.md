# P5 T-U5 receipt: Ctrl-O entry times in the local zone

Epic 4 (Transcript), unit T-U5: fx v0.0.16 (`vercel-labs/fx@481104fd`) T9 of
`research/tui-phase5:.prd/tui-phase5/research/transcript.md` (§2 T9, §3 row
U5), fx commit `c9bc4876` "Show times in the local time zone". **S**: `TZ`
comes from the environment and reaches the terminal. Base: T-U1+U2's head
`6c99caa5` (`feat/p5-transcript-wrap`), which this branch is stacked on.

## What changed

- **`src/local_time.rs`** (new) ports `src/core/shared/local_time.zig@481104fd`:
  - `local` (`:62-68`): nothing outside 1970-9999; otherwise the C library's
    zone **at that moment** (`localZoneAt`, `:144-159`), so daylight saving
    time follows each timestamp's own date.
  - `in_zone` (`:70-100`): UTC for an offset of a day or more, or one that
    carries the moment out of 1970-9999. The calendar is xfx's own arithmetic
    (`trace::civil`); only `tm_gmtoff` and `tm_zone` are read from `struct tm`,
    `tm_gmtoff` through `i32::try_from` (UTC when it does not fit), so no
    `tm` value can panic it.
  - `write_zone_name`/`is_plain_zone_name` (`:102-124`): the abbreviation
    only when it is 1-10 of `A-Z a-z 0-9 + -`, otherwise `±HHMM`.
  - `tm_zone`'s bytes are copied **at most 11** (one past the limit), so a
    long name is refused by its length, never read to its end.
  - A test build reads a per-thread test zone, UTC unless a test sets it
    (`test_zone`, `:24-27`), so every existing ` UTC · ` assertion in the
    crate holds on any machine; `None` reads the C library, as release does.
- **`src/tui/full_transcript.rs` `stamp`** reads `local_time::local`
  (`formatHeader`, `src/core/output/full_transcript_metadata.zig:28-38@481104fd`).
  Only Ctrl-O is wired; the other surfaces are listed below.

## Security: what `TZ` can put on the screen

- **The filter is the only thing between `TZ` and the terminal**, and it is
  fx's. `TZ` can name any zone file (`TZ=/path`), whose abbreviation is
  whatever bytes the file holds. macOS's libc already rewrites bytes outside
  ` :+-._A-Za-z0-9` to `_` and cuts names at 16 (tzcode `scrub_abbrs`; seen:
  a file named `\x1b[2J` reads `__2J`, `<\x1b[2J>-5:30` reads `__2J`); glibc
  is not relied on to. Either way the filter refuses `_`, space, `:`, `.`.
- **Threads.** `localtime_r` is re-entrant, but `tm_zone` points into the
  library's storage, which a later conversion or `tzset` may overwrite (macOS
  `localtime(3)`). One `Mutex` holds every reading from the call to the copy
  of those bytes. Nothing else in xfx calls the libc zone code (no `chrono`,
  `time` or `jiff` in `Cargo.lock`), and xfx sets no environment at runtime
  (`set_var`/`remove_var`: 0 calls in `src/`).
- **`tzset`.** fx calls none (`git grep tzset 481104fd -- src`: 0 hits). xfx
  calls it **once**, under the same lock, before the first `localtime_r`:
  POSIX asks `localtime` to behave as though it called `tzset` and asks less
  of `localtime_r` (Linux `ctime(3)`: "For portable code, tzset(3) should be
  called before localtime_r()"). No behaviour change on glibc or macOS, whose
  `localtime_r` initializes itself (macOS `localtime(3)`). No `setenv`.
- **Tests never set `TZ` in the test process.** Each C-library case runs in a
  copy of the test binary started with its own `TZ` (`Command::env`), so no
  test races another's read of the environment.

## Tests

| test | covers |
|---|---|
| `local_time::tests::a_reading_is_the_moment_in_the_given_zone` | fx `local time reads a timestamp in the given zone` (`:168-177`) |
| `…::a_zone_that_cannot_apply_reads_in_utc` | fx `:179-190`, plus +14 h past 9999, ±1 day, `i32::MAX`/`MIN`, a day less a second (`+2359`) |
| `…::a_zone_without_a_plain_name_shows_its_offset` | fx `:192-209` (`+0545` Nepal, empty, `\x1b[2J`, 11 bytes), 10 bytes shown |
| `…::a_hostile_abbreviation_never_reaches_the_screen` | 16 names: CSI erase/colour, OSC title, OSC 8 link, C1 CSI (UTF-8 and bare), C1 OSC, BEL, DEL, NUL, non-ASCII, Latin-1, space, `__2J`, 11 bytes, **300 bytes**: each `+0530`, every case reported |
| `…::no_offset_or_moment_makes_a_reading_fail` | 5 names × 11 offsets (`i32::MIN`..`i32::MAX`) × 9 moments: no panic, a plain zone, 1970-9999 |
| `…::a_reading_follows_the_test_zone` | fx `:211-216` |
| `…::the_c_library_reads_daylight_saving_time_on_each_moments_own_date` | libc: `EST5EDT,M3.2.0,M11.1.0` and `AEST-10AEDT,M10.1.0,M4.1.0/3`, the last ms before each change and the change |
| `…::the_c_library_reads_a_zone_without_a_usable_abbreviation_as_its_offset` | libc: `<+0545>-5:45`, `<ABCDEFGHIJK>-5:30`, a zone file with an empty name |
| `…::the_c_library_reads_a_zone_that_cannot_apply_in_utc` | libc: `PST8` at 2 ms, `<+14>-14` at 9999, `<XX>-25`, zone files at ±25 h and `i32::MAX` s, out-of-range moments |
| `…::a_hostile_zone_file_never_reaches_the_screen` | libc: a TZif v1 file per hostile name (the NUL and 300-byte ones aside), each `+0530`; a 300-byte name `+0530` or, where libc refuses the file (macOS, > 50 bytes of names), `UTC` |
| `tui::full_transcript::tests::a_header_is_upstreams_local_stamp_and_the_entrys_kind` | fx `full_transcript_metadata.zig:40-59`, EST, and an OSC name read as `+0545` |
| `tests/tui.rs` `ctrl_o_reads_its_headers_in_the_local_zone_and_never_prints_a_zone_files_escapes` | the real binary on a pty, `TZ=<zone file named \x1b]2;XQZ\x07>`: header ` +0530 · Tool`, `XQZ` nowhere in the output |

The C-library tests use POSIX `TZ` strings and zone files the test writes, so
they need no system tzdata. They ran on macOS here; **CI's two Linux runners
are their glibc receipt** (each assertion holds whether or not glibc scrubs).
`ctrl_o_shows_what_a_tool_read_…` now sets `TZ=UTC` for its ` UTC · Tool`.

## RED

- **The pairs differ on the base.** `red/` is the base `6c99caa5` (every
  banner-bearing snap of the run: `v0.1.0-6c99caa`, `banners.txt`), `fx/` is v0.0.16, at 40x120,
  `pair.py` (`pairs/red/`): `p5-transcript-tz` **6 of 41 rows differ** (5,
  10, 13, 16, 32, 35: `  XXXX… IST · Notice|Prompt|Response|Tool|Response|Usage`
  against `… UTC · …`), `p5-transcript-tzsys` **6 of 41** (the same rows, `KST`
  against `UTC`). The base's clocks are also wrong for the zone the run read
  (`clocks.txt`).
- **The pty test fails on the base** (`red-pty.txt`): the new
  `ctrl_o_reads_its_headers_…` with the base's `src/` in a throwaway clone
  panics at its header assertion; the screen says `2026-11-06 23:18:38.052 UTC · Tool`.
- **Each guard is RED when removed** (`mutants.txt`, a throwaway `git clone
  --no-local`; run on `1a4d9523`, this commit before an amend that changed
  only doc text -- `git diff 1a4d9523 29bbc118 -- src` is `//!` lines alone;
  the unmutated run first, all pass):

  | mutant | fails |
  |---|---|
  | filter removed (`is_plain_zone_name` → `true`) | 7 of 12 unit tests + the pty test; **every one of the 16 hostile names** in both hostile tests, each listed with what it printed (`\u{1b}[2J`, `\u{9b}2J`, `KST한`, …; macOS's libc gives `__2J`, `_2J`, `KST___`); 11 and 300 bytes by a slice panic |
  | length check only (charset removed) | 4 + the pty test (`__2_XQZ_` on the screen) |
  | charset check only (length removed) | 6 (the 11- and 300-byte names, the empty one) |
  | no day fallback | 2 (`±1 day`, `<XX>-25`, ±25 h zone files) |
  | no range fallback | 3 (`PDT` at 2 ms, +14 h past 9999) |

## GREEN

`xfx/` is this branch's code commit **`29bbc118`**, built after it was
committed (its `strings` carry `29bbc11812de`; every one of the run's 33
banner-bearing snaps reads `x𝒇x v0.1.0-29bbc11 [debug]`, `banners.txt` -- a
Ctrl-O snap has no banner row), against the same `fx/` snaps, 40x120:

```
FX_BIN=<fx v0.0.16, sha256 8e6cda2e…> TMPDIR=/private/tmp/claude-501 scripts/fxcap/capture.sh fx  p5-transcript-tz|tzsys 40x120 <dir>
XFX_BIN=<xfx at 29bbc118>             TMPDIR=/private/tmp/claude-501 scripts/fxcap/capture.sh xfx p5-transcript-tz|tzsys 40x120 <dir>
python3 -I -B pair.py <repo> fx/<snap>.ansi xfx/<snap>.ansi [--clock <IANA zone>]
```

capture.sh found fx's bytes unchanged after each run. `pair.py` compares
every cell (text and attributes) after the ledger's normalizations: row 1's
`network-ms`, and each header's clock (`transcript-timestamp`, which now keeps
the zone and is compared).

| pair | rows differing | headers |
|---|---|---|
| `p5-transcript-tz` (`TZ=Asia/Kolkata`) `full` | **0 of 41** (`pairs/green/`) | 6 × `IST` on both |
| `p5-transcript-tzsys` (no `TZ`: `/etc/localtime`, Asia/Seoul) `full` | **0 of 41** | 6 × `KST` on both |

The masked clocks are right too (`clocks.txt`, `--clock`, run on the fresh
captures): each of fx's and xfx's 12 headers is its zone's reading of a moment
5.8-6.1 s before its snap's file time, under the abbreviation that zone gives
it; each of the base's reads `WRONG`.

## Regressions

`regressions.txt`: every `p5-transcript-*` capture (13 runs, 65 snaps; tools
at 40x120 and 40x80) on the base and on `29bbc118`, `regress.py`, cell by
cell after the ledger's normalizations. **65 of 65: nothing differs but a
header's zone.** 57 are identical; the 8 Ctrl-O snaps of the scenarios that
set no `TZ` differ only in their headers, `UTC` → `KST` (`cancel` 9, `diffctl`
8, `md` 2 + 4, `md-light` 2 + 4, `tzsys` 6) or `UTC` → `IST` (`tz` 6), each such
row checked to be the same row with `UTC` written back. T-U1+U2's scenarios
set `TZ=UTC` and are identical.

`wrap-pairs.txt`: T-U1+U2's own pair check (their `pair.py`, their committed
`fx/` snaps, `PAGE_OFFSET=2`, causes `session-record` `tool-schema`) on this
branch's captures: **35 of 35 reports the same as theirs** in
`transcript-wrap/pairs/green/` (their old mask spelled `X{27}`, this ledger's
`X{23} UTC`; the invocation first reproduced their reports from their own
`xfx/` snaps, 35 of 35).

## The other surfaces that show a time (not wired in this unit)

| xfx | today | fx v0.0.16 |
|---|---|---|
| `src/output.rs:845` `xfx sessions` | `updated_at_ms=<ms>`, no clock | local `YYYY-MM-DD HH:MM:SS.mmm <zone>` (`writeLocalTimestamp`, `src/core/output/output_contracts.zig:1349,1390@481104fd`) |
| `src/output.rs:1062` `xfx session` | `updated_at_ms <ms>`, no clock | (detail JSON: ms) |
| `src/tui/usage_menu.rs:814`, `:841` `/usage` | `no usage today (UTC)`, reset day `nov 7` | the same: usage periods stay UTC days (`src/ui/footer/usage_presentation.zig:508@481104fd`; c9bc4876's message) |
| `src/trace.rs:272`, `:279`, `:287` `/trace` | file name, `generated:` and each moment in UTC (`Z`) | the same (`writeTraceTimestampUtc`, `src/core/app/app_commands.zig:2653@481104fd`; `generated:` `:1456`) |
| — | xfx has no `usage` CLI and no `slack` | `fx usage` `updated`/`resets` (`writeLocalClock`, `output_contracts.zig:351`), `fx slack` expiry (`:3848-3851`) |

Owner of the `xfx sessions` row: the sessions cluster (its text format is
xfx's own today, not a UTC clock).

## `docs/parity.md` and the ledger

- `docs/parity.md`, the `full transcript (C-o)` row: the header is
  `<zone> · <Kind>` in the local zone; the filter, `±HHMM`, the UTC fallback,
  and the `tzset` difference, cited.
- `scripts/fxcap/parity-ledger.toml` `transcript-timestamp`: masks the clock
  and **keeps the zone** (`(?= [A-Za-z0-9+-]{1,10}(?: · |$))`), so a pair
  compares `KST`/`IST`/`+0530` instead of masking them away;
  `scripts/fxcap/test_sweep.py` checks both (same zone, other clock: equal;
  `KST` vs `UTC`, `IST` vs `+0530`: one row differs).
  - A sweep pins `v0.0.16` (`sweep.yml:150`, `sweep-full.yml:82`), where fx
    and xfx read the same zone. Against `c1db919`, whose fx always prints
    `UTC`, Ctrl-O headers now differ on a machine whose zone is not UTC: that
    is v0.0.16's change, not a regression.
- The scenarios `p5-transcript-tz` and `-tzsys`: `xfx: env
  XFX_PERMISSION_MODE=yolo` (the hint row, `full access` on both, as T-U1+U2
  did for theirs) and a corrected header (Asia/Kolkata **is** `IST` in
  macOS's tzdata; the research header predicted `+0530`).

## Gates

At `29bbc118` (the code commit), `TMPDIR=/private/tmp/claude-501/tlt`, one
at a time, each full suite and build after the idle wait (`until ! pgrep -x
cargo … && ! pgrep -x 'xfx-[0-9a-f]+' …`), macOS:

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| `cargo clippy --locked --all-targets --features fault-injection -- -D warnings` | ok |
| `cargo test --locked --no-fail-fast` | 4913 passed, 0 failed, 7 ignored |
| `cargo test --locked --features fault-injection --no-fail-fast` | 4942 passed, 0 failed, 7 ignored |
| `scripts/check-xfx-identity.sh`, `check-no-stubs.sh`, `check-no-secrets.sh` | ok, ok, ok (9765 tracked files; re-run with this receipt staged: ok, ok, ok, 9789 files; `test-sweep.sh` ok) |
| `scripts/fxcap/test-sweep.sh` (the ledger changed) | ok (24 tests + the sweep tests) |
| `cargo build --locked --release`; `--release --features fault-injection --target-dir target/faulty` | ok; ok |
| `scripts/smoke.sh target/release/xfx` | 47 checks, 0 failures |
| `scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx` | 37 scenarios + the oracle, 1111 checks, 0 failures |
