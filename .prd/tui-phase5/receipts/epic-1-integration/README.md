# P5-EPIC1-INT receipt: epic 1 on `main`

Epic 1 of the 0.0.16 delta, three closed stacks merged onto `main` at
`564ddbac` (H1 #42, CI-SWEEP, PREVIEW-EPOCH, CLIP-STOP, two flake fixes),
in this order, each `git merge --no-edit`:

| merge | stack tip | PR | units |
|---|---|---|---|
| `12344109` | `feat/p5-permissions-early` @ `0398a598` | #41 | P-panel, P-status (#33), SESS-EARLY (#37), P-permissions (#40), PERM-EARLY (#41) |
| `8c3fa3a9` | `feat/p5-fork-command` @ `67483498` | #35 | FK1 (#32), FK2 (#35) |
| `59e4f9dd` | `feat/p5-effort-column` @ `a0e47e38` | #39 | EF1 (#38), EF2 (#39) |

No behaviour is added. The three stacks share the merge base `e0d97bd5`.
Of the 1,379 files the integration changes against `564ddbac`, 1,363 are
touched by one side only, and each is byte-for-byte that side's tip
(checked file by file). The other 16 are touched by two or more sides and
are listed below; those not named merged cleanly and were left as merged.

## Conflicts and their resolutions

### Scenarios (add/add)

- `p5-panels-keys.cap`, `p5-panels-permissions.cap` (merge 1): **the
  unit's side**, which is `main`'s research copy plus each product's own
  turn (`fx: gateway …-turn.json`, `xfx: gateway …-turn-xfx.json`; keys
  also `xfx: env XFX_PERMISSION_MODE=ask`). Nothing of `main`'s is lost.
- `p5-effort-fork-fork.cap`, `p5-effort-fork-fork-edge.cap` (merge 2):
  **`main`'s side, the larger one**. FK2's copies were `main`'s without the
  research run's `/status` step (`snap status`, `snap status-empty`). FK2
  left that step out only because P-status's panel was not on its base
  (its README says so). The panel is now on the base (merge 1), so the step
  is back. Both snaps are new pairs. They moved no later snap: `fork` and
  `fork-edge` keep every pair their unit passed.
- `p5-effort-fork-effort-menu.cap` (merge 3): **EF1's side**. Neither
  file is the other plus more. `main`'s research copy holds `/effort HIGH`
  (`level-upper`) and the `/effort ` column block (`col-space` …
  `col-enter`, `after-col`). EF1 moved both to EF2's
  `p5-effort-fork-effort-column.cap`, where they are verbatim (checked line
  by line). EF1 also added `slash-eff`, `menu-edit` and `level-low`. So
  every step of both sides is in the scenario set. Putting `main`'s block
  back would run the column twice and change the effort level of EF1's
  slow turn.

### `docs/parity.md`

Two hunks, both at the `/model` row. The merge was word by word. Merge 2
kept SESS-EARLY's first-save clause on `/model` and added FK2's new `/fork`
row above it. Merge 3 kept that clause, added EF2's closing sentence to
`/model` ("fx v0.0.16 opens the same effort column …") and put EF1's new
`/effort` row below it. The check covered `564ddbac` and the three tips.
For every line of a tip's `parity.md` that is not in the result verbatim,
each run of words the tip added since `e0d97bd5` is in the result's row of
the same name, and each run it deleted is gone from that row. It found
nothing.

### Code

| file:line (result) | side A wanted | side B wanted | kept |
|---|---|---|---|
| `src/tui/help_menu.rs:775`, `:784` | P-status: `/status` in the `SESSION` and `ses` search results | FK2: `/fork` after `/rename` | both, in registry order |
| `src/tui/worker.rs:1688-1695`, `:1789-1790` | P-status: `next_session`, `session_refused` (field and init) | FK2: `opened` | all three |
| `src/tui/shell.rs:12525`, `:12594`, `:12676`, `:12720`, `:12805` (`permissions`, `statusline`, `workspace`, `mcp`, `settings`), plus `usage` (`:12402`, clean) | P-panel: close `info_panel` | FK2: close `fork_menu`; EF1: close `effort_menu` (merge 3) | all three closes |
| `src/tui/shell.rs:32413` | P-status: `ses` lists `2 + 9` with `/status` | FK2: `2 + 9` with `/fork` | `2 + 10`, both named |
| `src/tui/shell.rs` test tail (`:40406`, `:40847`) | P-panel's panel tests | FK2's `/fork` tests | both |
| `src/tui/shell.rs:1139`/`:1162`, `:2085`/`:2088`, `:2312`, `:2717`/`:2727`, `:10542`, `:10632` | FK2: `fork_menu` field, `Slot::Fork`, init, hint arms, Escape disarm, trust-prompt guard | EF1: the same for `effort_menu` | both, Fork before Effort |
| `src/interactive.rs:2861` | FK2: `SLASH_COMMANDS.len() == 33` with `/fork` | EF1: 33 with `/effort` | **34**, both named |
| `src/tui/bridge.rs:2531`, `:2572` | FK2: fork prompts made inert (test) | EF1: `EffortApplied` made inert (test) | both |
| `tests/session_compat.rs:680`, `:797` | SESS-EARLY: section 4 | FK1: section 4 | both; FK1's renumbered `5.` |

Two hunks compiled on neither side as merged. Each was fixed in the merge
commit, with each test asserting what it did on its own branch:

- `src/tui/worker.rs:15986` (FK1b test): `state.grant_lines()` became
  `state.granted()`. P-permissions renamed the method, and it now returns
  one `Granted` per grant where the old one returned one line per grant
  (`Conversation::grant_lines`), so the count it asserts is the same.
- `src/tui/worker.rs:8958` (SESS-EARLY test): `change_speed(&mut state,
  …)` became `ran_speed(&mut state, …)`. EF1c made `change_speed` async and
  gave it the event sender; `ran_speed` is EF1's helper that runs it the
  way the loop does.

### Considered and left alone

- **The band's three new occupants.** `open_info_panel`
  (`src/tui/shell.rs:7037`), `fork_points` (`:6216`) and `effort_command`'s
  open (`:13184`) each close what their own unit listed, and none of them
  closes the other two. No close was added, because none can open while
  another is up. The info panel takes every typed key
  (`offer_to_info_panel`, `:6994`). Any edit to the composer closes the
  fork menu, drops a `/fork` still waiting for its answer (`fork_asked`)
  and closes the effort menu (`amended`, `:14212`). So no command can be
  typed, and no `ForkPoints` answer can land, while one of them is open.
  If two met anyway, `slot()` (`:3047`) ranks them Info, then Fork, then
  Effort, in upstream's order (`surface_frame.zig:406`, `:408@481104fd`).
- `src/session/store.rs`: PERM-EARLY's `create_with` seed and FK1's
  `create_private_dir` / `forked_from` are in different functions.
- `scripts/smoke-tui.sh` scenario 16 (EF1g/h) works out its `/se` and `/et`
  lists from the registry (`ranked_completions`), so it needed no edit for
  `/fork`.

## Receipts

### Tests

`cargo test --locked --no-fail-fast`: 4,717 passed, 0 failed, 7 ignored.
`--features fault-injection`: 4,746 passed, 0 failed, 7 ignored. Every
`#[test]` that a stack adds since `e0d97bd5` ran and passed (`... ok`):
82 from permissions-early, 49 from fork-command, 49 from effort-column, 2
from `main`. Eight EF1 tests are `fault-injection` only and passed in that
run.

### Pairs (fx v0.0.16, sha256 `8e6cda2e…`; xfx = this merge, `59e4f9dd`)

Both columns are classified with `status-panel/pairs.py` under this
branch's ledger. The unit column is each receipt's committed GREEN
captures, from the unit's latest round (EF2's for EF1's scenarios). The
integrated column is the sweep below at 40x120, plus `capture.sh` at the
units' own sizes: 14x120 and 12x120 on the debug build, and 24x50 on the
release build, as P-status took it (`xfx-debug-banner-wrap`). Counts are
identical-or-normalized / approved / total.

| scenario | unit | integrated |
|---|---|---|
| `p5-panels-keys` | 2 / 5 / 10 | 2 / 5 / 10 |
| `p5-panels-noauth` | 0 / 2 / 2 | 0 / 2 / 2 |
| `p5-panels-status` | 5 / 6 / 11 | 5 / 6 / 11 |
| `p5-panels-status-first` | 1 / 2 / 3 | 1 / 2 / 3 |
| `p5-panels-status-badsettings` | 0 / 3 / 3 | 0 / 3 / 3 |
| `p5-panels-status-short` 14x120 | 3 / 1 / 6 | 3 / 1 / 6 |
| `p5-panels-status-short` 24x50 | 1 / 5 / 6 | 1 / 5 / 6 |
| `p5-panels-permissions` | 16 / 1 / 20 | 16 / 1 / 20 |
| `p5-panels-permissions-saved` | 5 / 0 / 8 | 5 / 0 / 8 |
| `p5-permissions-early` | 11 / 0 / 11 | 11 / 0 / 11 |
| `p5-permissions-early-probe-ask-after` | 0 / 1 / 1 | 0 / 1 / 1 |
| `p5-session-early-prefs` | 5 / 0 / 5 | 5 / 0 / 5 |
| `p5-effort-fork-fork` | 14 / 0 / 15 | **15** / 1 / 16 |
| `p5-effort-fork-fork-edge` | 10 / 0 / 10 | 10 / 0 / 11 |
| `p5-effort-fork-fork-empty` | 4 / 0 / 4 | 4 / 0 / 4 |
| `p5-effort-fork-fork-continue` | 2 / 0 / 2 | 2 / 0 / 2 |
| `p5-effort-fork-effort-menu` | 18 / 0 / 18 | 18 / 0 / 18 |
| `p5-effort-fork-effort-column` | 17 / 0 / 17 | 17 / 0 / 17 |
| `p5-effort-fork-effort-{backtab,ctrlc,down,loading}` | 2 / 0 / 2 each | 2 / 0 / 2 each |
| `p5-effort-fork-effort-nolevels` | 5 / 0 / 5 | 5 / 0 / 5 |
| `p5-effort-fork-effort-short` 12x120 | 3 / 0 / 3 | 3 / 0 / 3 |
| `p5-effort-fork-slash-rank` | 3 / 0 / 4 | 3 / 0 / 4 |

No pair went down: none dropped from passing, and none from approved to
unresolved. One pair went up: `fork/slash-fo` is now normalized-identical,
because fx lists `/fork` and `/effort` for `/fo`, which FK2's receipt said
would match once both units were on one branch. Three pairs are new:

- `fork/status`: approved (aligned). It has only the `/status` panel's
  approved rows: Sandbox, Step limit, Updates.
- `fork-edge/status-empty`: **unresolved, with no ledger cause.** With the
  panel open over a fork made before the first prompt, fx has a blank row
  between `original session fx --resume …` and the panel's rule; xfx has
  none. Once the panel closes, the two screens are the same again
  (`fork-of-empty` passes). The lead: fx's footer frame gives a modal band
  `top_gap_rows = 1` (`src/ui/footer/surface_frame.zig:686@481104fd`).
  FK2's `raw_gap` (`src/tui/shell.rs:1155`) opens the next notice or card
  with that blank, but not the band. Neither unit could reach this screen
  on its own. Fixing it adds behaviour, so it is left for a unit; it is not
  a regression. `pairs.py` exits 1 on a directory that holds this pair (a
  `?` cause), but `ci-sweep` does not fail on it, because the base does not
  pass it either.

`p5-panels-status-short` and `p5-effort-fork-effort-short` also ran at
40x120 in the sweep, a size no unit receipt has.

### Sweep

The `sweep.yml` job, run locally:
`ci-sweep.sh --base 564ddbac --fx <fx v0.0.16> --pin v0.0.16 --xfx <head
debug> --base-xfx <564ddbac debug> --jobs 2`, `FX_AUTO_UPGRADE=0`. It ran
35 scenarios (10 core, 14 changed, 11 from receipts) and **231 snap pairs:
182 pass at the head, 60 new, 0 regressions**. Pairs not passing at the
head that the base also does not pass: `approvals` 0/10 and `h1-resize`
`small`/`large`, both known on `main` (`scripts/fxcap/ci-core.txt`). The
`effort-menu` snaps that the base takes and the head does not (`col-*`,
`level-upper`) are the steps EF2's `effort-column` scenario now takes
(above).

### Gates

Run at `59e4f9dd`, with a short `TMPDIR` for the smoke runs.

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| `cargo clippy … --features fault-injection -- -D warnings` | ok |
| `cargo test --locked --no-fail-fast` | 4,717 passed, 0 failed |
| `cargo test … --features fault-injection` | 4,746 passed, 0 failed |
| `scripts/check-xfx-identity.sh`, `check-no-stubs.sh`, `check-no-secrets.sh` | ok |
| `cargo build --locked --release`; `--release --features fault-injection --target-dir target/faulty` | ok |
| `scripts/smoke.sh target/release/xfx` | 46 checks, 0 failures |
| `scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx <dir>` | 37 scenarios and the oracle, 1,111 checks, 0 failures |
| `scripts/check-preview-contract.sh` | ok |
| `scripts/check-tui-preflight-cost.sh` | ok (every case under budget) |

No pty test failed, so none was re-run.
