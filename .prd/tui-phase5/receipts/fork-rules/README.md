# P5-FK-RULES receipt: a fork keeps the source's remembered denies

A security unit (S) on the epic-1 integration head, `integrate/p5-epic-1`
@ `cf25feaf` (PR #53), branch `fix/p5-fork-rules`. Two reviews of FK1/FK2
noted that `SessionStore::fork` does not copy the source's
`permissions.json`; FK2 graded that fail-closed, reasoning about allows
only. This unit checks the deny side.

## Facts

**(a) xfx's fork, base `cf25feaf`.** `SessionStore::fork`
(`src/session/store/fork.rs:95-309`) writes the stage's log, manifest and
images only; nothing reads or writes `permissions.json`
(`session_rules::FILE_NAME`, `src/permission/session_rules.rs:44`;
`path_in`, `:524`). The fork's directory has no store.

**(b) fx v0.0.16's fork.** fx keeps a session's rules in its v2 log, as
`set` lines with the key `permissions` (`schema.zig:111@481104fd`;
`setPermissions`, `src/core/session/session_adapter.zig:2205-2208@481104fd`,
called by `commitPermissionState`, `app_session_runtime.zig:4961-4969`),
folded into `state.permissions` (`session_manager/fold.zig:70`, `:201`).
`openFork` copies lines 2..S verbatim and folds each
(`session_manager/session.zig:1744-1747`; `copyFolded`, `:1837-1868`), so
the fork holds the last `set permissions` at or before S: allows **and**
denies, **as they stood at the cut**. The fork is installed as a resume
(`installResumedV2Session`, `app_session_runtime.zig:2130-2146`):
`durableState` → `settingsFrom` (`session_adapter.zig:4113`) →
`restoreWithPermissionState` (`app_session_runtime.zig:2179`). The
`permissions.json` of `session_log.zig:28` is v1's file, read only when a
v1 session is converted (`session_adapter.zig:593-595`).

**(c) What a lost rule changes in xfx.** The fork is bound to its own
store (`bind_conversation`, `src/interactive.rs:1439`); a missing file
loads as an empty store (`session_rules.rs:596`), and an empty store is
skipped (`src/permission/policy.rs:1352`). Every decision reads the store
unless the mode is full access (`policy.rs:1504`, `:1332`). Without the
remembered deny:

- `auto`: a write the workspace admits (`policy.rs:631`) and a read-only
  command (`:639`) are allowed by the mode (`:1436`) -- **fail-open**;
- every mode but full access, `ask` included: a read is free once no rule
  decides it (`:1420`) -- **fail-open, with no question**;
- `ask`: a write or a command asks (`:1445`) where the deny refused;
- full access: no rule is consulted, before or after.

A lost remembered *allow* asks again (`ask`) or is refused when `auto`
would not admit it: fail-closed, which is what FK2 saw.

## RED (base `cf25feaf`)

Two pty tests in `tests/tui.rs`, driving the real binary with a fake
gateway: the deny remembered before the first prompt, turn 2's model makes
the call and the rule refuses it (the control: the key matches), `/fork`
before turn 2, the refilled prompt sent, the fork's model makes the same
call.

- `a_remembered_deny_still_refuses_its_write_in_a_fork_in_auto`: the fork's
  result is `{"type":"text","value":"wrote b.txt (1 bytes)"}`, not
  `execution-denied`.
- `a_remembered_deny_still_refuses_its_read_in_a_fork_in_ask`: the fork's
  result is `<content>\n1\thunter2\n</content>`: the file went to the model.

Both fail on their `fork:` assertion and pass their `source:` one
(`red.txt`: the base's code with this branch's two tests, bounded by
`timeout 300` and each test's own 20-second waits; the scripted gateway
answers five replies and nothing more).

## Fix: the rule

The fork carries **each deny the source holds now, and no allow**, with
the rules' ids and generations and the source's `next_generation`
(`carried_rules`, `src/session/store/fork.rs`). Why not fx's rule: xfx's
store is a file beside the log with no cut, so "as of S" cannot be read.
A deny only narrows, so every current deny is carried. An allow could have
been made after the cut, which fx's fork would not hold and which xfx's own
grant rule already refuses (an "always" given after the cut is not the
fork's), so none is carried. The divergence is written in `docs/parity.md`
(`/fork`, `session fork`, `/permissions`): a deny remembered after the cut
is carried (fx: not), a deny revoked after the cut is not (fx: is, but the
source no longer holds it either), an allow made before the cut is not
(fx: is; xfx asks again).

**This departs from the brief's literal rule** ("carry allows too only if
fx does"). fx does carry allows, but as of the cut. Carrying xfx's current
allows could give the fork an allow made after the cut, which fx's fork does
not have. Carrying the source's current allows would be a one-line change in
`carried_rules` (drop the filter).

How it lands:

- Read before the stage is made, with `O_NOFOLLOW`
  (`session_rules::load_not_through_a_link`). A store that cannot be read,
  or is a link (dangling or not), a folder or a FIFO, refuses the fork as
  `Corrupt` (`✗ session: Unable to fork the session.`): the source refuses
  every call while its store cannot be read (`policy.rs:1503-1520`), so a
  fork that dropped it would be the fail-open.
- Written into the fork's stage through `session_rules::seed`: a `0600`
  `O_NOFOLLOW` staged file, published with `link(2)`, read back; renamed
  into `sessions/` with the rest, or removed with the stage.
- No store, or no deny in it: no file.

Unit tests (`src/session/store/fork/tests.rs`):
`a_fork_carries_the_sources_remembered_denies_and_no_allow`,
`a_fork_of_a_session_with_no_deny_has_no_rules_file`,
`a_fork_whose_source_rules_cannot_be_read_or_are_a_link_is_refused_and_leaves_nothing`.

## Mutants (a `git clone --no-local` in scratch, never the worktree)

1. **The carry removed** (the `session_rules::seed` call in `fork`
   replaced by nothing): both pty tests fail on `fork:` exactly as on the
   base (`mutant-carry-tui.txt`), and
   `a_fork_carries_the_sources_remembered_denies_and_no_allow` fails
   (`mutant-carry-lib.txt`).
2. **`O_NOFOLLOW` removed** from `load_not_through_a_link`: the refusal
   test fails on `a link to a store: forked <id>` (`mutant-nofollow.txt`).

## Gates (short `TMPDIR`)

The clippy, test, build and smoke rows ran at `13ec0d34`. The fix commit
`0f165e63` differs from it only in four comment and doc lines, where an fx
line citation was corrected (`session.zig:1742-1743` to `:1744-1747`), so its
compiled code is the same. The fmt and check rows ran again at the receipt
commit, where the checks scan 7,669 tracked files (7,663 plus these six).

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| `cargo clippy … --features fault-injection -- -D warnings` | ok |
| `cargo test --locked --no-fail-fast` | 4,722 passed, 0 failed, 7 ignored (base 4,717 + 5 new) |
| `cargo test … --features fault-injection` | 4,751 passed, 0 failed, 7 ignored (base 4,746 + 5) |
| `scripts/check-xfx-identity.sh`, `check-no-stubs.sh`, `check-no-secrets.sh` | ok |
| `cargo build --locked --release`; `--release --features fault-injection --target-dir target/faulty` | ok |
| `scripts/smoke.sh target/release/xfx` | 46 checks, 0 failures |
| `scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx <dir>` | 37 scenarios and the oracle, 1,111 checks, 0 failures |

The first full run found one failure of this branch's own making:
`process::tests::no_child_process_is_made_any_other_way` read the FIFO case's
`std::process::Command::new("mkfifo")` in `fork/tests.rs` (a test file with
no `#[cfg(test)]` line of its own). It now goes through
`crate::process::command`, and the table is the run after that. No pty
test failed, so none was re-run.

## Pairs (the `p5-*fork*` scenarios must not move)

Every `p5-effort-fork-*` scenario the epic-1 receipt paired, re-taken with
`scripts/fxcap/capture.sh` (fx v0.0.16, sha256 `8e6cda2e…`, unchanged after
the runs; xfx = this branch's `target/debug/xfx`), classified with
`status-panel/pairs.py` (`pairs.txt`). Identical-or-normalized / approved /
total, against the epic-1 integration's column:

| scenario | epic-1 | this branch |
|---|---|---|
| `fork` | 15 / 1 / 16 | 15 / 1 / 16 |
| `fork-edge` | 10 / 0 / 11 | 10 / 0 / 11 |
| `fork-empty` | 4 / 0 / 4 | 4 / 0 / 4 |
| `fork-continue` | 2 / 0 / 2 | 2 / 0 / 2 |
| `effort-menu` | 18 / 0 / 18 | 18 / 0 / 18 (one fx retake, below) |
| `effort-column` | 17 / 0 / 17 | 17 / 0 / 17 |
| `effort-{backtab,ctrlc,down,loading}` | 2 / 0 / 2 each | 2 / 0 / 2 each |
| `effort-nolevels` | 5 / 0 / 5 | 5 / 0 / 5 |
| `effort-short` 12x120 | 3 / 0 / 3 | 3 / 0 / 3 |
| `slash-rank` | 3 / 0 / 4 | 3 / 0 / 4 |

No pair moved. The two left unresolved are epic-1's: `fork-edge/status-empty`
(its `?`, the band's top gap) and `slash-rank/e` (`menu-count`).
`effort-menu/midturn` was unresolved on the first run under
`stream-position`: fx's run showed `Generating (2s) (↑4 ↓6)` and xfx's
`↓7`. Two more xfx runs said `↓7`, and two more fx runs said `↓7` and pair
normalized-identical. That is the ledger's nondeterminism cause (the
streamed chunk a fixed-second snap lands on), and on fx's side; this
branch changes nothing a turn without a fork reaches.

None of these scenarios remembers a rule, so a pair could not show the
carry. The pty tests above are its receipt. No fx capture was taken of a
remembered deny in a fork: fx v0.0.16's TUI applies a saved rule to a read
and not to a write (`docs/parity.md`, `/permissions` row), and its fork
carries its rules as of the cut, which xfx cannot read; the code citations
in (b) are the fx fact.
