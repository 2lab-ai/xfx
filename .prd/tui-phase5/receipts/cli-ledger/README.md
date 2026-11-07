---
title: 0.0.16 delta -- P5 L-U4 receipt (parity ledger for fx v0.0.16's CLI changes)
type: receipt
tags: [xfx, phase5, fx-0.0.16, lifecycle, parity, cli]
---

# P5 L-U4: the parity ledger for fx v0.0.16's CLI changes

Spec: `research/tui-phase5:.prd/tui-phase5/research/lifecycle.md` §L3, §L4,
§L5, §4 rows L3-L5 and §5 U4. Docs and tests only: no `src/` file changes.
Base: `docs/p5-cli-ledger` @ `origin/main` `b0a78fea`. The ledger, script and
test change is `3f251ef9`; `c21212ba` moves the alias rule to the end of the
ledger (no row moves) and corrects the script's header; this README is the
commit after them. Oracle: fx
v0.0.16 (`481104fd`, executable sha256 `8e6cda2efddb…`, checked by
`capture.sh` before and after its run).

## What changed

| where | change |
|---|---|
| `docs/parity.md:270-288`, § Upstream command aliases | at the end of the file, so no row a line citation points at moves (`scripts/fxcap/parity-ledger.toml` cites twelve): an upstream alias of a command is named on its command's row as `Alias:` and the backticked name, and shares its status; every one today is on a deferred row |
| `docs/parity.md:63` (`upgrade`) | `Alias:` `update`, with what fx v0.0.16 does with it (below) |
| `docs/parity.md:61` (`credits`) | `Alias:` `balance`, the same class at both pins (`commands.zig:219@481104fd`, `:237@c1db9191`) |
| `docs/parity.md:48` (`sessions`) | `sessions convert` not advertised (xfx's store has one format); `--sessions-v2`, `session migrate`, `ask --continue-recovery` never in xfx |
| `docs/parity.md:237` (full-screen TUI) | L3, retry on 5xx, re-verified against `481104fd` |
| `scripts/check-no-stubs.sh:223` `parity_command_aliases`, `:336` §2d, `:435` | reads the marker (the notes rejoined, because the field split also cuts at `\|`); an alias of a deferred row must not be advertised; an `Alias:` on any other command row, or naming nothing, fails; aliases join the one-row-per-surface multiset |
| `tests/parity.rs:103` `aliases_in`, `:162`, `:212`, `:264`, `:281` | the same grammar; aliases in the uniqueness test; aliases only on deferred command rows; the deferred-name test covers aliases |
| `tests/parity.rs:337` `every_deferred_command_and_alias_is_refused_like_an_unknown_name` | runs each deferred command name and alias through the built binary (empty profile, `env_clear`) and requires exit 1, empty stdout, and the stderr of an unknown name with the name taken out and clap's `tip:` paragraph dropped (`refusal_shape`, `:320`; clap adds a tip to `permissions` and `teams` because they are close to real names) |
| `tests/cli.rs:301` | `sessions convert [--json]`, `session migrate <id>`, `ask --continue-recovery`, `ask --sessions-v2`, `--sessions-v2`: each `error: unexpected argument`, exit 1, no profile created |
| `scripts/fxcap/scenarios/p5-cli-ledger-update-help.cap` | fx's `update --help`, `upgrade --help`, `help update` (no local path; release host = the fake gateway) |

`balance` was not in the brief. It is the same case as `update` (an upstream
alias of a deferred command), the marker covers it for free, and leaving it out
would leave one alias of that class unchecked.

## fx v0.0.16: what `update` does (`fx/`)

- `h00-update-help`: `fx update --help` prints upgrade's own page and exits 0:
  `fx upgrade`, `Upgrade 𝒇x on the selected release channel`,
  `Usage: fx upgrade [--channel <stable|dev>] [--json]`, the two options.
  `h01-upgrade-help` is the same page for `fx upgrade --help` (the `.full`
  files are cumulative tmux history).
- `h02-help-update`: `fx help update` prints the top-level help, whose row reads
  `upgrade|update                     Upgrade fx on the selected release channel`.
- The gateway log has only the first launch's model catalog GET: no
  `/latest.txt`, nothing fetched or installed.
- 0.0.13: `fx update` is `fx: unknown subcommand: update`, exit 1 (research
  capture `old/update/u00`).

Source: `.aliases = &.{"update"}`, `src/builtins/commands.zig:242@481104fd`;
help row `upgrade|update`, `:320`; parsed as `.upgrade` with its flags,
`src/core/cli/cli_surface.zig:3664-3667`; early threaded I/O,
`src/main.zig:3531`; commit `c61fc618` (an ancestor of `481104fd`).

## L3: retry on 5xx, re-verified at `481104fd`

| rule | fx `@481104fd` | xfx | same? |
|---|---|---|---|
| label | `recoveryLabel`, `src/core/shared/types.zig:539-606` (vs `:530-596@c1db9191`: the only difference is a new `.delivering_response` action, not on the retry path) | `src/tui/retry.rs:369-381` | yes |
| count rounds up | `format_retry_label`, `src/core/output/worker_status.zig:201-219`, `:212` (API renames only since c1db9191) | `src/tui/retry.rs:384-391` | yes |
| causes | `types.zig:542`, `:545`, `:546` | `src/tui/retry.rs:252-261` | yes |
| statuses retried | 429/500/502/503/504: `fromHttpStatus`, `src/core/agent/stream_provider.zig:480-492`; `isRetryableModelFailure`, `src/core/agent/runtime/orchestrator.zig:4496-4501` | `src/gateway/mod.rs:994-995` | yes |
| budget and back-off | none ends the turn; 250 ms, 1 s, 2 s … 30 s; once a minute after 15 min (`src/core/agent/runtime/model_response_recovery.zig:5-9`, `:270-282`; the file differs from c1db9191 by one added test) | 3 attempts, 250 then 500 ms, Retry-After capped at 5 s (`src/gateway/mod.rs:59`, `src/agent/machine.rs:78-85`, `:199-207`) | no, as already written on the row ("the retry budget is xfx's own three attempts") |

0.0.16's one change on this path, `fe7925b2`, deletes a `surface_stall`
branch from the status-retry loop (`orchestrator.zig`, 22 lines) that a status
failure could never reach, and pins that with a test. Nothing a retry shows
changed, so the parity row `transport-owned retry and team routing` is
unchanged.

## RED

**The gap, on the base.** Mutant `mutant-alias.patch`: `#[command(alias =
"update")]` on `Status`, so `xfx update` quietly runs `status`. On the base
(`red-base-mutant.txt`) every gate passes: `tests/cli.rs` 81/81,
`tests/parity.rs` 24/24, `check-no-stubs: ok`, and `xfx update` exits 0.
`parser_command_names` reads only `get_name()`, and the script reads text.

**The same mutant on this branch** (`red-new-mutant.txt`):
`every_deferred_command_and_alias_is_refused_like_an_unknown_name` fails,
`xfx update must exit 1 as an unknown name does`, left `Some(0)`, right
`Some(1)`, with status's report as its stdout. The mutant was run in the
worktree (it removes no guard) and reverted with `git checkout -- src/cli.rs`.

**The script** (`stubs-mutants.txt`, each on a copy in the scratchpad):

| mutant | base script | new script |
|---|---|---|
| M1: `"update"` in `ADVERTISED_COMMANDS` plus an implemented `update` row (advertising it honestly) | `ok` | fails: `update` is an alias of the deferred `upgrade` but advertised; `update` has two rows |
| M2: the marker without backticks | -- | fails: an `Alias:` that names no backticked command |
| M3: `Alias:` `stat` on the implemented `status` row | -- | fails: only a deferred command's alias can be reconciled here |
| M4: `credits`' alias changed to `usage`, a row of its own | -- | fails: `usage` has more than one row |

Deleting the `Alias:` `update` from the ledger fails `deferred_command_aliases`,
which requires `update`, so the binary-run test cannot pass by checking nothing.

## GREEN

The cargo gates ran on `3f251ef9` with a clean tree, one at a time after the
idle wait, `TMPDIR=/private/tmp/claude-501/cli-ledger`. `c21212ba` changes no
Rust (`git diff 3f251ef9 c21212ba -- src tests build.rs Cargo.toml Cargo.lock`
is empty), only the ledger's text and a script comment, so `tests/cli.rs` and
`tests/parity.rs`, which read the ledger, ran again on it (`green-dev.txt`:
82/82 and 26/26), as did the script mutants above. The three scripts ran on the
tree with this README committed.

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok (0 warnings) |
| `cargo clippy --locked --all-targets --features fault-injection -- -D warnings` | ok (0 warnings) |
| `cargo test --locked --no-fail-fast` | ok: 4882 passed, 0 failed |
| `cargo test --locked --features fault-injection --no-fail-fast` | ok: 4911 passed, 0 failed |
| `scripts/check-xfx-identity.sh` | ok (9497 tracked files), with this README committed |
| `scripts/check-no-stubs.sh` | `check-no-stubs: ok`, with this README committed |
| `scripts/check-no-secrets.sh` | ok (9497 tracked files), with this README committed |

Every file here went through `scripts/fxcap/redact.py`.
