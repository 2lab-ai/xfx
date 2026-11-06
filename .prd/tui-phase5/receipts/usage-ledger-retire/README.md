---
title: 0.0.16 delta -- P5-US-U5 receipt (retire the local usage ledger)
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, usage, security]
---

# P5-US-U5: retire the local usage ledger

Spec: `research/tui-phase5:.prd/tui-phase5/research/usage.md` §5 U5 and the §4
C1-removal row. Oracle: fx v0.0.16 (`481104fd`, executable sha256
`8e6cda2efddb…`, checked by capture.sh before and after its run), whose
`a41fc2d4` *Read /usage from provider usage reports* says: "The session and
profile usage ledgers, the usage sidecar file, Gateway generation lookups, and
provider billing outcomes are removed. Saved sessions with old usage data still
load and resume; old usage fields and files are left in place and ignored."
User decision (2026-11-06): "Retire it, copy fx"; the llmux
`$0.00 llmux/<model>` accounting goes with the ledger. Base:
`feat/p5-usage-ledger-retire` @ `120f7b20` (US-U4's closed head), with US-U4b
(`feat/p5-usage-screen` @ `b81cbb6b`) taken in with `git merge --no-edit`,
which fast-forwarded: `120f7b20` is its ancestor and this branch had no commit
of its own yet, so there is no merge commit and nothing to resolve; this
unit's commits sit on `b81cbb6b`. Unit class **S**.

fx citations, all `@481104fd` unless marked:
- old usage accepted and not kept: `src/core/session/session_codec.zig:1464-1469`
  (a state's `usage` skipped in place), `src/core/session/session_event.zig:849-852`
  (`UsageCheckpointed`, "fx no longer writes them"), `:1549-1552` (replays as
  the session's last update only), `:1808` (a session's `usage` "accepted and
  not kept");
- no generation lookup: `src/gateway/generation_usage.zig`,
  `src/core/session/session_usage.zig`, `usage_report.zig` and
  `profile_usage_store.zig` are not in the tree, and `src/gateway/client.zig`
  holds no `/v1/generation` (it was `client.zig:323@c1db9191`);
- the compaction trace still writes cache counts, off its usage:
  `src/core/agent/runtime/text_completion.zig:271-273`.

## Reader trace: what is removed, what stays

Every writer of the ledger, and every reader it had at the base.

| removed | written by (base) | read by (base) | why it can go |
|---|---|---|---|
| `usage_recorded.requests` (`RequestUsage`) | `agent::machine::request_usage` (each completed step), `agent::compact::Spent` (each notes request) | `session::store::apply` → `RecordedUsage.requests` → `usage::Ledger::from_sessions` / `usage::session_snapshot` (the old `/usage`) and `tui::worker::ask_for_bills` (pending → reconciler) | the old `/usage` was unread since US-U4; the reconciler goes below |
| `usage_recorded.unaccounted` | `machine` / `compact` via `may_have_billed` | `RecordedUsage.unaccounted_at_ms` → `Ledger` completeness | the ledger only; the fact survives inside the turn as `spent` (below) |
| `usage_recorded.lines_added/lines_removed` | `machine` (`self.lines`, from each tool result's changes) | `RecordedUsage.lines_*` → `usage::session_snapshot` (`Code +N · -N`) | not in the brief's list, but its one reader is the old session scope, gone in US-U4, and fx dropped `recordCommittedLines` with `session_usage.zig` in `a41fc2d4`. A `tool_result`'s own `lines` stays: the transcript replays it |
| the `settled` / `rejected` frames | `tui::reconcile::Answered::frame` via `worker::settle_bill` | `RecordedUsage::settle` | nothing asks for a bill any more |
| `GET /v1/generation` | `gateway::generation::lookup` ← `tui::reconcile::Reconciler` ← `worker::ask_for_bills` (after every piece of work and on a resume) | -- | deleted: `src/gateway/generation.rs`, `src/tui/reconcile.rs`, `bills`/`ask_for_bills`/`settle_bill`/`Taken::Bill`, `is_trusted_generation_origin`, `DEFAULT_ORIGIN`, `Bill::Deferred`/`Deferred`, `SseReader::from_origin` |
| `~/.xfx/usage.json` | `SessionRecorder::commit` → `usage::start_tracking` | `usage::tracking_started` ← `usage::read_ledger` (unread since US-U4) | neither written nor read; an existing one is **left alone** (S4) |
| the window arithmetic | -- | `usage::{Scope, Coverage, Completeness, Totals, ModelUsage, SessionActivity, Snapshot, Ledger, session_snapshot, compact, money, share, duration}`, `SessionStore::usage_since`/`UsageScan`, `DurableState.usage` | no caller after US-U4 (grep at the base: none outside `src/usage.rs` and its own tests) |
| a compaction-between-turns frame | `worker::run_compact` (`Spent::frame`) | the ledger | it would now carry nothing |

| stays | read by | why |
|---|---|---|
| `usage_recorded.input_tokens/output_tokens` | `store::apply` → `DurableState.total_*` (the context meter, `/status`, `xfx session`), an old frame's replayed summary | not the ledger; fx keeps its token totals too |
| `usage_recorded.turn_output_tokens` | `store::apply` → `turn.timing.output_tokens` (a replayed summary row) | the summary row a resume shows |
| when the frame is written | `machine::run_journaled`: `result.is_ok() \|\| self.spent` | **unchanged**: `spent` is the old `!requests.is_empty() \|\| unaccounted` as one bool (a step completed, a compaction's notes request completed or was lost, a request was lost), so a failed turn's row still replays. `may_have_billed` stays for it. Two tests pin the two wirings the old list carried (below) |
| `Completion.usage` | the footer's context, `ask --json`, the context meter, compaction's calibration (`compactor.answered`) | untouched |
| `Billing`, the Gateway SSE `parse_billing`, llmux's `bill()` | `agent::compact::send`'s trace line `compaction model call … cache_read_tokens= cache_write_tokens=` | the one carrier of cache counts until U7 moves them onto `Usage` (upstream reads them off its usage, `text_completion.zig:271-273`). Nothing adds a bill up: the `$0.00 llmux/<model>` row is gone with the ledger. `Option<Bill>` is `Option<Billing>`; a stream with an id and no cost has no bill |
| `usage::fixed`, `usage::grouped` | `tui::usage_menu` (the screen) | the formatters the new screen uses |
| the old fields, on read | `SessionEvent::UsageRecorded` decode | read past: the variant has no `deny_unknown_fields` (the envelope alone has it), so an old frame decodes, replays as its totals, and a fork writes it again without them |

## S properties, each with its test

| property | tests |
|---|---|
| **S1** no `GET /v1/generation`, ever | `tests/ledger_compat.rs` `the_binary_holds_no_bill_route_and_no_ledger_file_name` (the built binary's bytes hold neither `/v1/generation` nor `usage.json`); `the_tui_asks_for_no_bill_on_a_resume_a_turn_or_usage` (pty, `FakeGateway` logs every request: a TUI resume of the fixture, whose pending request's `origin` is this gateway, then a deferred turn, then `/usage`: 0 generation GETs, 1.5 s waited after each). fxcap `p5-usage-noledger` 40x120 (two deferred turns, `/usage`, exit, `relaunch --resume`, a third, `/usage`): `cap/*-gateway.log` -- **fx v0.0.16 0, head 0, base 25** generation GETs, each with 3 POSTs and 2 `/v1/report` |
| **S2** an old log loads, resumes, replays and appends | fixture `tests/fixtures/ledger-120f7b20/sessions/1793983613244-206c9ac7f3059b4a` (written by the base's TUI through a pty, `gen-fixture.rs`: a billed turn, a deferred one settled, one rejected (`/v1/generation` answered 400), one cut after its text (`unaccounted`), one left pending (401), its origin `@ORIGIN@`; 29 frames, 7 `usage_recorded`); `the_fixture_holds_every_frame_the_ledger_wrote`; `the_ledger_builds_session_resumes_replays_and_appends_without_its_fields` (`session --id --json` reads 15600/920 and writes nothing; `ask --resume-id` twice: the five prompts and answers go back in order, the old bytes stay the head of the log, the appended `usage_recorded` frames carry only the three kept keys); lib: `session::store::tests::a_session_the_ledger_build_wrote_replays_its_turns_and_totals` (5 turns, 15600/920, rows `800 40 40 6 40`), `session::event::tests::a_usage_frame_the_ledger_wrote_reads_as_its_totals` |
| **S3** nothing new carries the removed fields | `tests/sessions.rs` `a_saved_turn_records_its_tokens_and_no_usage_ledger` (a fully billed `ask` turn: the frame is exactly `{kind,input_tokens,output_tokens,turn_output_tokens}`, no `usage.json`); `session::event::tests::a_frame_the_ledger_wrote_decodes_and_encodes_without_the_ledger` (every fixture frame decodes; each `usage_recorded` re-encodes with the three keys only); `session::store::fork::tests::a_fork_of_a_session_the_ledger_build_wrote_carries_none_of_the_ledger` (a fork before turn 5 re-frames 6 old `usage_recorded`, settled/rejected among them, with none of the fields); the pty and resume tests above check every appended frame |
| **S4** no stale `usage.json` read or trusted: **left alone**, as fx | `an_old_usage_json_is_left_as_it_was` (bytes, mtime and `0600` unchanged through a turn); nothing can open it: the binary does not hold the name (S1's first test). Not deleted: fx leaves its old files in place (`a41fc2d4`) |

The turn's frame condition, the one place the ledger's list did work a bool now
does: `tests/tool_loop.rs` `a_turn_that_failed_after_a_request_still_writes_its_summary_frame`
(a completed tool step, then a `Status 400` that reached no model: the frame is
still written) and `agent::machine::tests::a_turn_a_compaction_spent_on_writes_its_frame_though_its_request_is_refused`
(a compaction's notes request, then a 400; without the compaction, no frame).

## Tests the ledger had, and what became of them

- Deleted with their code: `src/gateway/generation.rs`'s and
  `src/tui/reconcile.rs`'s own tests; `tui::worker`'s two bill tests;
  `session::store`'s settle test; `usage.rs`'s window tests (its formatter
  tests stay); `tests/tui.rs` `a_bill_that_comes_later_is_asked_for_by_generation_until_answered`
  (its claim is now the opposite, held by S1's tests); `tests/tool_loop.rs`'s
  request-record and pending-record tests.
- Rewritten to the retired shape: `tests/sessions.rs` (S3), `tests/tool_loop.rs`
  `a_turn_records_its_totals_and_none_of_the_usage_ledger`,
  `a_turn_that_failed_after_a_request_still_writes_its_summary_frame`,
  `a_turn_the_endpoint_refused_writes_no_usage_frame`; `tests/tui.rs`
  `a_session_resumed_under_an_unreadable_provider_asks_the_gateway_nothing`
  (was `a_pending_bill_resumed_…`: the first session no longer asks either),
  `compact_replaces_the_older_turn_in_every_later_request_and_a_resume_keeps_it`
  (no frame before the checkpoint; one `usage_recorded` per turn),
  `ctrl_c_stops_an_automatic_compaction_and_its_turn_and_the_log_says_the_turn_stopped`
  (the turn's frame, no `unaccounted`); `gateway::sse`'s bill tests (an id and
  no cost is no bill); `agent::compact`'s `Spent` test; the fork tests' usage
  frame; `llmux`'s bill test renamed
  `a_llmux_answer_carries_its_cache_counts_in_a_bill_nothing_adds_up`.

## RED on the base

`red-base.txt`: the final test files dropped into a `git clone --no-local` at
`120f7b20`. `the_binary_holds_no_bill_route_and_no_ledger_file_name`: *the
binary holds `/v1/generation`*; `the_tui_asks_for_no_bill_on_a_resume_a_turn_or_usage`:
*the resume asked for a bill* `["/v1/generation?id=gen_01JABCDEFGHJKMNPQRSTVWXYZ3"]`;
`the_ledger_builds_session_resumes_replays_and_appends_without_its_fields`:
`left: ["requests"]`; `a_saved_turn_records_its_tokens_and_no_usage_ledger`:
the frame holds `requests`. The lib tests on the base code, added before any
source edit: the event and fork tests fail on `["requests"]`. The fixture,
replay and `usage.json` tests pass there too (the base also left an existing
`usage.json` alone: `create_new`). fxcap: base's gateway log, 25 generation GETs
across both turns, the resume and both `/usage` opens.

## Mutants

`mutants.txt` (driver `mutants.py`): eight, on a `git clone --no-local` at
`270e0a09`, each reverted before the next. All killed: M1 the completed step
not `spent`, M2 a compaction's notes request not `spent`, M3 the frame only on
success, M4 a lost notes request not `spent`, M5 the event enum denying unknown
fields (three tests), M6 the binary holding `/v1/generation`, M7 a commit
writing `usage.json` (two), M8 a commit rewriting an existing one.

## Pairs

`p5-usage-noledger` (new, `scripts/fxcap/scenarios/`), 40x120, fx v0.0.16 vs
head and vs base, `cells.py`: `turns`, `usage`, `resumed`, `usage-resumed` --
0 rows differ in each, both builds. The screens never showed the ledger's
asking; the gateway log is the receipt.

## Decisions (reversible, mine)

- **`lines_added`/`lines_removed` go too** (not in the brief's list): their
  only reader was the retired session scope, and fx dropped them in the same
  commit. Old frames' values are read past.
- **The frame-write condition is kept exactly** (`spent`), rather than fx's
  per-reply `set usage`, which is U7's.
- **`Billing` and llmux's `bill()` stay** as the compaction trace's cache-count
  carrier until U7; their cost and model fields are read by nothing.
- **An existing `usage.json` is left alone**, not deleted, as fx leaves its
  files.

## Consequence to note (not decided here)

The ledger shipped in two previews: `preview-2026-11-03-…-93f516b537ad` and
`preview-2026-11-06-…-2e266236cc57` hold its first commit (`8b07dee5`, `git
merge-base --is-ancestor`); `14ad0420` and `v0.1.0` do not. One of those two
previews, reading a session this build continued, sees `usage_recorded` frames
with totals only, which its ledger counts as recorded before requests were
(`legacy`), so its `/usage` says `Partial data` over those windows. The log
opens and resumes there (a known variant with fewer fields). Mixed installs are
what `tests/session_compat.rs` guards. This follows from copying fx, and
whether it matters is the dispatcher's call.

## Gates

At `22abd5c0` (the code and tests; later commits touch only this receipt), on
`b81cbb6b` (US-U4b fast-forwarded in), short `TMPDIR`, `FX_AUTO_UPGRADE=0`.

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings`, and `--features fault-injection` | ok, ok |
| `cargo test --locked --no-fail-fast` | 15 result lines ok: 4827 passed, 0 failed |
| `cargo test --locked --features fault-injection --no-fail-fast` | 15 ok: 4856 passed, 0 failed |
| `scripts/check-xfx-identity.sh`, `check-no-stubs.sh`, `check-no-secrets.sh` | ok (9387 files), ok, ok (9387 files) |
| `cargo build --locked --release`; `… --features fault-injection --target-dir target/faulty` | ok, ok; `/v1/generation` and `usage.json` 0 times in each binary's bytes |
| `scripts/smoke.sh target/release/xfx <dir>` | 47 checks, 0 failures |
| `scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx <dir>` | 37 scenarios + the oracle, 1111 checks, 0 failures |
| `scripts/check-usage-clock.sh` | ok: debug honoured, release and release + fault-injection ignored |
| `scripts/check-preview-contract.sh` | ok |
| `scripts/fxcap/test-capture.sh`, `test-sweep.sh` | 36 tests OK (one run of four had 1 error, not reproduced in the next three; the harness change here is a comment in `fakegw.ts`), OK |
| `scripts/fxcap/ci-sweep.sh --base 120f7b20` (`sweep-report.md`; head build `f36ad428`, whose binary `22abd5c0` does not change: it edits `tests/tui.rs` only) | 11 scenarios, 57 pairs: 55 pass, 4 new (`p5-usage-noledger`), **0 regressions**, exit 0; `h1-resize` `small`/`large` unresolved at base and head, as the core list says |

The first full run at `f36ad428` failed four `tests/tui.rs` cases that still
held the ledger (two bill tests, two compaction frames); `22abd5c0` updates
them (above), and the table is the run after.
