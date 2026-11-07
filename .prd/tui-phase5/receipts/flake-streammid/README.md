# flake-streammid: snap a stream at a fixed event, not at a second

PR #74 changes only `.github/preview-epoch`, and its CI sweep failed one pair:
`streaming` `mid`, row 5, the activity row's out count. Four head runs gave
fx/xfx 59/63, 63/61, 63/61, 61/59; the base's one run was identical. The
scenario took `mid` three seconds after Enter, into a stream paced at 100 ms a
chunk. Under load each binary had read a different number of chunks by then.

The fix is at the source. The fake gateway can now hold a stream after a fixed
number of events, until the scenario lets it go (`holdAt` in a turns file,
`held` and `release` in a scenario). Every scenario that snaps or types while a
paced stream is still arriving now holds it. The ledger loses the wildcard
cause that let CI re-run any pair whose token counts differed. No product code
changed. Line numbers are at `fd3f5f02` unless marked "now"; fx's are at
`481104fd`.

## Why the count moved

- The scenario, `scripts/fxcap/scenarios/streaming.cap:4-6`: Enter,
  `wait 3`, `snap mid`.
- Its turn, `slow-line-turn.json`: 62 chunks (`MARKER-HEAD `, `word0 ` to
  `word59 `, `MARKER-TAIL`) and `delayMs: 100`. `fakegw.ts:480` (`sse`) sleeps
  that long after every event, so the stream runs for about 6.2 s from the
  moment the request arrives.
- The count is the received text's estimate on both sides. In fx,
  `consumeContent` and `tokenProgress`
  (`src/core/agent/runtime/telemetry.zig:116`, `:156-163`) count each span
  as `span_bytes / 4` rounded up (`src/core/shared/token_estimate.zig:29`). In
  xfx, `Tally::output` (`src/tui/shell.rs:2014`) uses `Estimator::consume`
  (`src/agent/tokens.rs:38`, `:61-63`) with the same rule. So `MARKER-HEAD`
  is 3 and each word is 2: after k words the row reads `↓(3 + 2k)`.
- `mid` shows no answer text: the answer is one line with no newline, and
  both binaries hold back a line until it ends. The activity row is the only
  row the stream moves.
- What decides the count is how many chunks each binary has read at 3.0 s
  after Enter. The two binaries run one after the other; under load each
  request starts a different number of milliseconds after its Enter. The
  values are odd, one to three chunks apart, and swap sides between runs.

The ledger already knew this kind of difference: the `stream-position` cause
(`parity-ledger.toml:686-695`) covered `(↑N ↓N)` rows in every scenario and
every snap. A cause approves nothing. It makes CI re-run the pair up to three
times (`ci_sweep.py` step 2), and here all four head runs still differed.
It also matched settled summary rows (`  <wall> (↑4 ↓40)`). In the Nov 4 full
sweep of main (`93f516b5`) it was attached to `h1-refuse/back`,
`h1-reset/after` and `timeout/live`, whose rows differ for reasons that are
not timing at all.

## The fix

- `fakegw.ts` (now): a turn's `holdAt: n` holds the Gateway stream after its
  first n events, and the gateway logs `hold <k> after <n> event(s)`. The
  stream goes on when the file `FAKEGW_RELEASE` names has k lines, or when the
  client goes (the stream's `cancel`), or after 120 s. A hold the stream
  cannot make is logged as `holdAt <n> not held: <why>`: n outside 1 to
  (events − 1), a reset turn, or the llmux and Codex wires, which send their
  body at once.
- `capture.sh` (now): `held` waits up to 30 s for the next hold in the
  gateway's log, and fails the run (exit 1) without it. `release` writes the
  line. Both refuse an argument (exit 2, before launch).
- Fourteen scenarios: `held` where a fixed wait used to be, and `release`
  after the mid-stream snap or keystroke. The table below gives each one's
  hold.
- The ledger: the `stream-position` cause is deleted, and a comment at its
  place says why.

The only timing left is the `wait 1` (or the keystrokes) between `held` and
the snap. That wait does not race the stream: by then every byte the snap
counts is already in the binary's socket, and nothing more is coming. Both
binaries redraw the activity row on their own clock, and fx's pacer emits
what is complete on its next tick (`src/ui/assistant/pacer.zig:257`, `:288`).

## RED and GREEN

- RED: `red.log`. The base harness, scenario and binary
  (`v0.1.0-fd3f5f0`), seven local pairs at load 7.6 to 13.6: 0 of 7 the
  same (59/61, 59/61, 61/59 four times, 55/59). CI: 0 of 4.
- GREEN: `runs.log`. At `3ed24db2`, with the binary built there
  (`v0.1.0-3ed24db`), ten pairs at load 9.8 to 25.9: 10 of 10 `↓61`/`↓61`. The
  same ten again before the commit: 10 of 10. Every capture log has the
  hold.
- Sweep, the fourteen scenarios, `--jobs 3`, under the same load:

| | base scenarios | held, run 1 | held, run 2 | held at 3ed24db2, with the CI core |
|---|---|---|---|---|
| scenarios | 15 | 14 | 14 | 23 |
| pairs | 87 | 82 | 82 | 133 |
| passing | 59 | 61 | 61 | 110 |
| not passing | 28 | 21 | 21 | 23 |

  Base minus held: `streaming/mid` and the five `interrupt-steer` snaps
  (`steer-stream`, `steer-stream-done`, `steer-tool`, `steer-done`,
  `end.full`), all of them the stream's position; and `model-parked`, which
  was not run held (see below). Held minus base: none. The 21 left are the
  same pairs in both held runs and at the head, each for a cause outside
  timing:
  `cancelled-marker` (U7), `provider-label`, `session-record`,
  `full-transcript-shift`, an approved cascade, and `p5-transcript-cancel/mid`'s
  footer (fx runs `FX_PERMISSION_MODE=yolo`, xfx `auto`). At the head the
  core adds `h1-resize` small and large, which `ci-core.txt` names as not
  passing yet; `streaming` mid and end are normalized-identical.

## A real count difference is still caught

- Mid-stream, live: a scenario copy with fx held at 30 events and xfx at 31.
  The sweep gives `mid` unresolved-defect, row 5, `↓61` / `↓63`, no cause.
  `end` is normalized-identical.
- Settled, rescored: the same sweep with xfx's `end` row edited from `↓40` to
  `↓41`. `--rescore` gives `end` unresolved-defect, row 10, no cause.
- The tests (now): `test_ci_sweep.py` `TokenCounts` checks that no
  nondeterminism cause of the repo's ledger covers a token count, mid-stream
  or settled (`streaming` mid and end, `notice-midstream` version,
  `credits-busy` during, `h1-reset` after). `test_sweep.py`
  `test_each_runs_own_clock_and_ids` gains a live row,
  `• Generating (3s) (↑4 ↓61)` / `  Generating (4s) (↑4 ↓63)`, that must
  still differ.
- Mutants, in `mutants/ledger-mutants.patch`, each run on a scratch copy of
  the ledger:
  - `m1`, a normalization that masks every `↓N`: `test_each_runs_own_clock_and_ids`
    fails on the settled `  2s (↑6 ↓40)` / `  1s (↑6 ↓41)`.
  - `m1b`, one that masks only a live turn's `↓N`, the narrow fallback the
    brief allowed: the same test fails on the new live row.
  - `m2`, the `stream-position` cause restored: `TokenCounts` fails all five
    subtests. On the committed ledger before this change, it failed the same
    five.

## Scope: every mid-stream snap and keystroke

The candidates came from two lists. The first is the activity rows of every
snap in the Nov 4 full sweep of main. The second is a static walk of every
`.cap` against its turns: a snap or keystroke while a paced turn is still
arriving.

| scenario | snaps or keys mid-stream | turns file, `holdAt` | before | held |
|---|---|---|---|---|
| streaming | `mid` | slow-line-turn, 30 | `mid` 0/7 | identical 10/10 |
| model-busy, model-busy-known | `/model` then `during` | slow-line-turn, 30 | passing | passing |
| provider | `/provider` then `busy` | slow-line-turn, 30 | other causes | same |
| resume-empty | `/resume` then `while-streaming` | slow-line-turn, 30 | passing | passing |
| notice-midstream | `nosuch`, `version` | notice-midstream-turn, 30 | `version` 87/85 on Nov 4 | passing |
| notice-wire | `/version` then `version` | notice-midstream-turn, 30 | passing | passing |
| fulltranscript-notice | the two notices | notice-midstream-turn, 30 | other causes | same |
| credits-busy | `/credits` then `during` | slow-turns, 10 | passing | passing |
| rename-midturn | `/rename` then `during` | rename-midturn-turns, 4 | passing | passing |
| interrupt-steer | the steer, `steer-stream` | interrupt-steer-fx/-xfx, 16 | 5 snaps failing | passing |
| p5-transcript-cancel | `mid`, the Escape | p5-transcript-cancel-turns, 25 | footer, `cancelled-marker` | same |
| p5-effort-fork-effort-menu | `/effort high`, `midturn` | p5-effort-fork-slow-turns, 4 | passing | passing |
| p5-effort-fork-fork-edge | `/fork`, `midturn` | p5-effort-fork-slow-turns, 4 | passing | passing |

"Passing" means it passed in that run. Those pairs read the stream at a
second, so a loaded runner could still move them, as `notice-midstream`
version on Nov 4 and `fork-edge/midturn` in an earlier receipt
(`fork-status-gap`). In `interrupt-steer` and `p5-transcript-cancel` both
binaries end the held stream themselves (the steer and the Escape), and the
gateway logs `hold 1: stream closed by client`.

Not held, and why:

- `stream-wire`: 7 chunks at 150 ms is about 1.05 s, and its one snap, `end`,
  is 4 s after Enter. `wire` is a byte file, not a pair.
- `interrupt`, `interrupt2`: their turns already send every chunk at once and
  hold the finish (`hangMs`), so their snaps read a complete answer.
  `model-reask` and `model-column-reask` do the same (`↓6` fixed).
- The `Running (↑2 ↓40)` rows (`interrupt-cmd`, `interrupt-recovered`,
  `interrupt-steer-retract`, `settings-collapse*`, `toolgroup`,
  `permissions-midturn`) count a settled request while a tool runs. That count
  does not move. `interrupt-recovered/rec-0.5` (left out of the CI core) is
  timed against a tool's recovery, which is a different class.
- `model-parked/menu-loading`: fx `Thinking (<s>)` with no counts, xfx
  `Generating (<s>) (↑4 ↓3)`. The difference is the row's kind, not a count,
  and it was the same on Nov 4 and in today's base run. It is not where the
  stream stands. The wildcard cause matched it only through xfx's side. It is
  now unresolved with no cause, and still owned by nobody.

## Gates (the code at 3ed24db2, run with this receipt committed, with a short `TMPDIR`)

| gate | result |
|---|---|
| `scripts/fxcap/test-sweep.sh` | test_sweep 48 OK, test_ci_sweep 25 OK (1 new) |
| `scripts/fxcap/test-capture.sh` | 41 OK (5 new) |
| `cargo fmt --check` | ok |
| `scripts/check-xfx-identity.sh` | ok |
| `scripts/check-no-stubs.sh` | ok |
| `scripts/check-no-secrets.sh` | ok |

No file under `src/` changed, so the cargo test and clippy gates were not
run.
