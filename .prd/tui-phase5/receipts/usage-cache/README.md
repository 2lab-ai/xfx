---
title: 0.0.16 delta -- P5-US-U2 receipt (the usage cache)
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, usage, security]
---

# P5-US-U2: the usage cache and its view

Spec: `research/tui-phase5:.prd/tui-phase5/research/usage.md` §5 U2, with
the §3 rules it cites (persistence C8, view). Oracle: fx v0.0.16 source at
`481104fd` (`src/usage/cache.zig`, `src/usage/view.zig`, with
`usage.zig`'s and `policy.zig`'s cache-side tests). Base: US-U1's closed
head `feat/p5-usage-report` @ `f5faa3b4`. Unit class: **S** (file writes
under the profile).

Code: `src/usage/cache.rs` (new) with its tests in `src/usage/cache/tests.rs`,
`src/usage/view.rs` (new), both declared from `src/usage.rs`;
`src/usage/report.rs` gains serde on `Totals`/`ModelRow`, fx's `Budget` and
`RefreshPeriod`, and `Question::identity`; one row in `docs/parity.md`
(Configuration and persistence, `AI Gateway usage cache`, `partial`).

**No UI pair.** This unit is a library with no caller and no screen, so the
paired-capture rule (`p5-unit-common.md` § Method 2) does not apply: there is
no fx screen it changes and no xfx screen to put beside one. RED is the
mutants below and the two races the two-process test found in the first
draft. The `/usage` screen pairs are U4's.

## What it exports (for U3/U4)

`crate::usage::cache`: `Identity` (`as_str`; built by
`report::Question::identity`, so the cache never holds the key), `Entry`
(`today`, `finished`, `budget`; `valid`, which is fx's `own`), `Today`,
`Finished`, `Budget`, `merge`, `finished_due` (the `finished` half of fx's
`policy.plan`), `Home` (`new`: `None` for an unset, empty or relative home),
`read`, `store`, `StoreError` (`Invalid` = fx's `error.InvalidCache`, which
U3 reports as `Failure::Cache`; `Refused`; `Io`), and the constants
`DIR_NAME`, `SCHEMA_VERSION`, `MAX_FILE_BYTES`.
`crate::usage::view`: `Period` (`ALL`, `label`, `next`, `previous`), `View`
(`empty`) and `build`. `crate::usage::report`: `Budget`, `RefreshPeriod`,
`Question::identity`.

Not added: `Failure::Cache`. fx's cache never builds it; its dashboard
runtime does, from `error.InvalidCache` (`usage.zig:705-708@481104fd`), so it
arrives with U3 from `StoreError::Invalid`. fx's `encode`/`decode` stay
private: nothing outside the module needs them.

## fx's tests, ported

| xfx test | fx source |
|---|---|
| `cache::tests::merge_keeps_the_newer_part_of_each_half` | `src/usage/cache.zig:310-325@481104fd` |
| `cache::tests::the_cache_file_round_trips_and_rejects_other_identities` | `src/usage/cache.zig:327-345@481104fd` |
| `cache::tests::a_decoded_entry_outlives_the_bytes_it_came_from` | `src/usage/cache.zig:347-355@481104fd` |
| `cache::tests::store_merges_under_the_lock_and_never_regresses_today` | `src/usage/cache.zig:357-376@481104fd` |
| `cache::tests::a_part_stored_while_the_clock_ran_ahead_yields_to_the_next_fetch` | `src/usage/cache.zig:378-416@481104fd` |
| `cache::tests::a_store_never_keeps_less_than_the_caller_holds` | `src/usage/cache.zig:418-459@481104fd` |
| `cache::tests::store_never_writes_an_entry_its_reader_would_reject` | `src/usage/cache.zig:461-481@481104fd` (`own` is `Entry::valid`) |
| `cache::tests::the_file_is_written_in_fxs_schema` | `Wire`/`encode`, `src/usage/cache.zig:172-195@481104fd` (fx has no byte test) |
| `cache::tests::a_finished_day_is_never_replaced_by_an_earlier_one` | `newer` by day, `src/usage/cache.zig:108,158-170@481104fd` |
| `cache::tests::finished_days_are_read_once_per_utc_day` | the cache side of `src/usage/policy.zig:133-143@481104fd` |
| `cache::tests::a_first_store_creates_a_private_profile` | the cache side of `src/usage/usage.zig:1165-1182@481104fd` |
| `cache::tests::a_cache_directory_that_is_a_file_refuses_the_store` | the cache side of `src/usage/usage.zig:1514-1532@481104fd` |
| `cache::tests::no_absolute_home_means_no_cache` | `openHome`, `src/usage/usage.zig:524-529@481104fd` (69a01a40) |
| `cache::tests::a_question_names_its_cache_entry_by_fxs_api_key_identity` | `src/core/app/usage_host.zig:105-111` with `src/usage/usage.zig:422-427@481104fd` |
| `cache::tests::a_fixture_seeded_for_fx_seeds_this_cache` | `research/tui-phase5:scripts/fxcap/scenarios/p5-usage-mkcache.py` (fx's identity for the harness key, Python's JSON) |
| `view::tests::seven_days_add_todays_numbers_to_the_finished_days_per_model` | `src/usage/view.zig:128-147@481104fd` (plus the month) |
| `view::tests::after_midnight_yesterdays_numbers_are_not_shown_as_today` | `src/usage/view.zig:149-165@481104fd` |
| `view::tests::periods_cycle_both_ways` | `src/usage/view.zig:167-171@481104fd` |
| `view::tests::no_entry_is_empty_and_equal_costs_sort_by_name` | `build` with no entry and `costDescending`, `src/usage/view.zig:68-116@481104fd` |

Not ported: `usage.zig`'s dashboard tests (`:1011-1636`) are U3's; the
Codex and Grok merges (`cache.zig:125-152`) are U9's.

## The five security properties

| # | Property | Test(s) |
|---|---|---|
| 1 | The key never reaches disk | `the_key_never_reaches_the_disk`: two stores (today, finished, budget; a second with `held`), then every name and every byte under the temp home scanned for every 12-byte run of the fake key, raw and in hex either case, and the whole key in base64 (standard, URL-safe) at all three alignments; a control first proves the scan finds the key embedded raw, in base64 at four offsets, in upper hex, and a 12-byte part in hex. `a_question_names_its_cache_entry_by_fxs_api_key_identity`: the identity is fx's hash, and its `Debug` holds no key |
| 2 | Server text is encoded, read back unchanged, never a path | `model_names_are_written_escaped_read_back_unchanged_and_never_name_a_path`: 11 names (`"` and `\`, ESC sequences, CR/LF/tab, NUL, bidi, zero-width, `../../../escaped`, `/an/absolute/path`, non-ASCII, DEL/C1/U+2028) stored, returned and re-read equal; no raw byte below 0x20 in the file; the cache directory holds exactly `<id>.json` and `<id>.lock`, and the home exactly four entries |
| 3 | Modes and links | `modes_are_owner_only_even_under_a_permissive_umask` (a copy of the test binary with `umask(0)`: `~/.xfx` and `usage-cache` 0700, `.json` and `.lock` 0600; and a 0755 directory with 0644 files made 0700/0600 by a store), `a_link_where_the_cache_directory_belongs_is_refused_not_followed` (a symlinked `usage-cache` to a directory holding a valid entry for the identity: `read` is `None`, `store` is `Refused`, the target gains nothing; a directory hard link cannot be made, `link(2)` refuses it), `a_link_where_a_cache_file_belongs_is_refused_not_followed` (a symlink and a hard link at `.json` to a valid decoy: not read, not replaced, decoy unchanged; a symlinked `.lock` to a file and to nowhere: refused, target unchanged or not created; a symlinked `.tmp`: removed, not written through), `something_that_is_not_a_regular_file_is_refused` (a directory or FIFO at `.json` and `.lock`, a directory at `.tmp`: `Refused`, each call bounded at 10 s) |
| 4 | Two processes, and a crash | `two_processes_storing_at_once_never_tear_or_regress_the_file` (two copies of the test binary, 200 stores each with interleaved `at_ms`, while this process reads without a lock: every store `Ok`, every read whole -- its row's count equals its `at_ms` -- and `at_ms` never goes back, the final file holds the maximum; bounded at 120 s, then killed by pid), `a_crash_between_the_stage_and_the_rename_leaves_the_old_file_whole` (a copy killed by `SIGKILL` after the stage is written and synced: `.json` byte-identical, the stage left holds the new entry, `read` gives the old; the next store succeeds and removes the stage), `two_processes_making_the_same_lock_at_once_both_store` (two copies create 40 locks at the same instants: every store `Ok`), `a_lock_held_past_the_deadline_is_a_failure_not_a_wait` |
| 5 | Hostile or oversized files, bounded as fx bounds them | `hostile_or_oversized_files_are_refused_whole`: 26 files -- over 4 MiB (exactly 4 MiB accepted), another identity, schemas 1 and 3, no identity, garbage, cut short, a key twice, unknown fields, negative, past-f64, negative, fractional and textual numbers, 257 models, empty and 257-byte names, a sum and a week-plus-today past f64, a negative budget, an unknown period, non-null `codex`/`grok`, deep nesting, one bad row among good ones -- each `read` as `None`, a view of it empty with zero totals, and a store over it holding exactly the new fetch |

## RED

The module is new, so "fails on the base" is "does not compile". RED is (a)
the two failures the two-process test gave on the first draft, and (b) each
guard taken out, one at a time, on a `git clone --no-local` copy in the
scratchpad (never this worktree), only its guarding test run, under
`gtimeout -k 10 600`; the unmutated copy passed first (26 passed on
`1f2f6425`; 27 on `85e18a25`, where M10 and M15 ran). Redacted output:
`red-mutants.txt`.

**Found by the two-process test on the first draft** (both fixed before
the first commit, `22100308`; M15 and M10 put them back):

- `a reader found the file unreadable`: the reader refused any file whose
  link count was not 1, and a file a store renames over after a reader
  opened it has a count of 0. Now only a count above 1 (a hard link) is
  refused.
- `every store succeeds: Io(Os { code: 2, kind: NotFound })` at the lock's
  `openat(O_CREAT)`: two processes creating the same name at once get
  `ENOENT` on macOS. Measured with a Python probe in the scratchpad: two
  processes, 2000 simultaneous creates each, 756/767 `ENOENT` with
  `O_NOFOLLOW` and 921/854 without; one process alone, 0. The lock's create
  now retries `ENOENT` (at most 50 times, 10 ms apart).

**Mutants** (all 16 killed, each on its first run against the test named):

| Mutant | Property | Failing test and assertion |
|---|---|---|
| M1 `flock` removed | 4 | `two_processes_storing_…`: a writer's store failed, `every store succeeds: Io(… NotFound …)` -- the other writer unlinked the shared stage `[inferred from the errno]` |
| M2a `O_NOFOLLOW` off the cache files | 3 | `a_link_where_a_cache_file_belongs_…`: `left: Some(Entry { today: … at_ms: 999 …` -- the decoy was read through the link |
| M2b `O_NOFOLLOW` off `usage-cache` | 3 | `a_link_where_the_cache_directory_belongs_…`: the decoy in the linked directory was read |
| M3 the hard-link check removed | 3 | `a_link_where_a_cache_file_belongs_…`: the hard-linked decoy was read |
| M4a `fchmod` removed | 3 | `modes_are_owner_only_…`: `left: 493` (0755 kept) |
| M4b std's default modes (0777/0666) and no `fchmod` | 3 | `modes_are_owner_only_…`: `left: 511` (0777 under `umask(0)`) |
| M5 the identity compare removed | 5 | `hostile_or_oversized_…`: another identity's file read as `Some` |
| M6 store writes without `valid` or the decode check | 5 | `store_never_writes_an_entry_…`: the store returned `Ok` |
| M7 the period-sum check removed | 5 | `hostile_or_oversized_…`: a sum past f64 read as `Some` |
| M8 both size bounds removed | 5 | `hostile_or_oversized_…`: the 4 MiB + 1 file read as `Some` |
| M9 a relative home accepted | 3 | `no_absolute_home_…`: `left: Some(Home { path: "" })` |
| M10 the lock-create `ENOENT` retry removed | 4 | `two_processes_making_the_same_lock_at_once_both_store`: `every store succeeds: Io(… NotFound …)` |
| M11 `O_NONBLOCK` removed | 3 | `something_that_is_not_a_regular_file_…`: `finished in time: Timeout` -- the read blocked on the FIFO, cut at the test's 10 s |
| M12 the stage opened following a link, truncating | 3 | `a_link_where_a_cache_file_belongs_…`: the link's target was overwritten with the entry |
| M13 the identity is the key's hex, not its hash | 1 | `the_key_never_reaches_the_disk`: `<local-path> names the key`, `left: Some("66616b652d6b65792d5a7137")` |
| M14 written in place, no stage | 4 | `a_crash_between_…`: the old `.json` was not byte-identical after the kill |
| M15 the first draft's `st_nlink != 1` | 4 | `two_processes_storing_…`: `a reader found the file unreadable` |

M10 first ran against `two_processes_storing_…` and **survived** five runs:
alone, the two writers rarely create the lock at the same instant (it had
failed only with the rest of the suite loading the machine). So the race got a
test of its own, `two_processes_making_the_same_lock_at_once_both_store` (two
copies, 40 locks each created at the same wall-clock moment), which M10 then
fails on the first run. Not mutated: property 2's guards are serde_json's
escaping and file names built from the identity alone; there is no guard of
this module's to remove short of adding a path made from a model.

## Decisions (each reversible)

- **fx's identity prefix is kept** (`fx-usage-cache-v1`), so
  `p5-usage-mkcache.py` seeds xfx's cache with only the directory changed
  (`a_fixture_seeded_for_fx_seeds_this_cache`), which U4's pairs need. The
  prefix is a hash input, not a name a user sees.
- **The key is hashed in `report::Question::identity`**, beside the only
  other reader of it, the transport. The cache module takes an `Identity`
  and cannot be handed the key.
- **A link, a hard-linked file or a non-regular file is refused, not
  replaced.** fx follows links on both read and write (Zig 0.16
  `OpenFileOptions`/`CreateFileOptions` have no `follow_symlinks`; parity
  row). Refusing leaves the thing for its owner and makes `store` fail, which
  U3 handles as fx handles an unwritable cache: the fetch is shown from
  memory (`unsaved`, `usage.zig:713-723@481104fd`). A garbage, oversized or
  unreadable *regular* file is replaced, as fx replaces it.
- **Modes are set on the descriptor** (`fchmod`) after each create, and an
  existing `usage-cache`, `.lock` or stage wider than 0700/0600 is narrowed.
  A pre-existing `~/.xfx` is left as it is: it is the profile, not this
  module's. A missing one is made 0700, as fx makes `~/.fx`.
- **The stage name is fx's fixed `<identity>.tmp`**, safe because it is only
  touched under the lock; whatever is there is unlinked first and the stage
  is created `O_EXCL | O_NOFOLLOW`.
- **The lock waits 5 s at most**, then `StoreError::Io(WouldBlock)`: fx's
  lock blocks without a bound (`cache.zig:276-280`), and a worker thread
  blocked for ever on a stuck process is worse than a fetch shown unsaved.
- **"Never regresses" is fx's rule, by key.** Today and the budget by fetch
  start, finished days by day, the fetch winning a tie; a finished part for
  an earlier day never replaces a later one. Two fetches for the same day
  with different finished totals: the later write wins, as in fx.
- **An entry whose period sum overflows to infinity is refused**, xfx's
  addition to fx's `validEntry`, so the view never shows an overflowed
  total. No real report comes near it.
- **Non-null `codex` or `grok` refuses the file.** xfx has neither sign-in,
  so such a file is not one xfx wrote. Both are written `null`, as fx writes
  every absent part.
- **serde reads a struct from a JSON array too** (`[5, 800, []]` for
  `today`), which fx's parse would refuse. It cannot change a total: the
  fields are the same fields, validated the same way. Not worth a hand-written
  `Deserialize`.
- **Non-unix**: `read` is `None` and `store` fails. xfx's CI targets are
  Linux and macOS only.

## Residuals

- `[unverified]` Whether the macOS simultaneous-create `ENOENT` also hits
  `mkdirat` or the `O_DIRECTORY` open of a `usage-cache` two processes make
  for the first time at once. Only the lock's create retries it. The failure
  would be `StoreError::Io`, which U3 shows as the unsaved fetch, not a wrong
  number.
- `[unverified]` Linux: the platform-dependent field widths (`st_nlink`,
  `st_mode`, `st_size`) are converted without casts, but clippy and the tests
  ran on macOS only (no Linux target installed); CI's Linux runners are the
  check.
- The race probe that measured the `ENOENT` used a shell `&` to start its two
  processes, against the brief's rule; it wrote only under the scratchpad.

## Gates

Run once, green, on `f00e6a28` (the code is `85e18a25`; only this receipt
changed after it), rustc 1.99.1 on macOS (aarch64), with
`TMPDIR` a short directory in the harness temp folder (`claude-501/p5uc`)
and `HOME` under it:

| Gate | Result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| `cargo clippy --locked --all-targets --features fault-injection -- -D warnings` | ok |
| `cargo test --locked --no-fail-fast` | 4599 passed, 0 failed, 7 ignored (14 binaries); the 27 `usage::cache` and 4 `usage::view` tests among them |
| `cargo test --locked --features fault-injection --no-fail-fast` | 4620 passed, 0 failed, 7 ignored |
| `scripts/check-xfx-identity.sh`, `check-no-stubs.sh`, `check-no-secrets.sh` | ok, ok, ok |
| `cargo build --locked --release`; `… --features fault-injection --target-dir target/faulty` | ok, ok |
| `scripts/smoke.sh target/release/xfx <dir>` | 46 checks, 0 failures |
| `scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx <dir>` | 37 scenarios + the oracle, 1107 checks, 0 failures |
| MSRV: 1.96.1 `cargo check --locked --all-targets` | ok (on `22100308`'s code; the later commits are tests and docs) |
| clippy and tests for `x86_64-unknown-linux-gnu` | not run here: no Linux target installed `[unverified]`; CI's Linux runners |

No pty test failed, so none was re-run. **A short `TMPDIR` is needed**: a
first run with `TMPDIR` in the session scratchpad (a 120-byte path) failed 12
tests this unit does not touch -- 11 `tui::shell` tests whose temp paths no
longer fit a 120-column row, and `image::tests::a_fifo_named_like_an_image_…`
(`path must be shorter than SUN_LEN`) -- and passed whole with the short one.
