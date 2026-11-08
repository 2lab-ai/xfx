# stop-in-catalog-wait: a stop in a turn's catalog wait ends it as the stop, its prompt kept

Opus's train 7 resolution review found it (PLAUSIBLE, so RED first): every
catalog wait races the turn's token, but nothing checks the token after the
turn-start wait, so a turn in Ultra with no catalog row is refused
(`request_options`) after a Ctrl-C, and the band shows `error` and a
`✗ system:` line under `▪ Cancelled`. gpt6 (NTH) asked that the train 7 join
test (`85b13f84`) also check the second turn's end.

Base: train 7's head `9298583e`. Every claim below is tied to a file in this
folder.

| commit | what |
|---|---|
| `cbcfa241` | RED, test only (`red.txt`) |
| `0b4d89be` | the fix and `docs/parity.md` -- **the code head** |
| `baaf8e0b` | `scripts/fxcap/scenarios/p5-stop-in-catalog-wait.cap` and its seed |
| (this one) | this receipt |

The xfx captures come from the binary built at `0b4d89be` (row 1
`x𝒇x v0.1.0-0b4d89b [debug]`, sha256 `0c1173b7e958…`, `sweep-head.md`); the
base's from `9298583e` (`v0.1.0-9298583`, `fd4d26b52417…`, `sweep-base.md`);
fx is the v0.0.16 release, sha256 `8e6cda2efddb…`, checked by the sweep before
and after each run.

## What fx v0.0.16 does (every line quoted in `fx-src.txt`)

1. **Which turns wait.** At its start a turn resolves the model's
   capabilities when Ultra is on, always, or when the prompt has images, the
   window is unknown, or a level or Fast is one the fallback cannot vouch for
   (F1, `orchestrator.zig:5658-5670`, `:5884-5891`). In the TUI only the
   prompt's images count: `authorized_image_catalog` is ACP's (F6,
   `acp/prompt.zig:1103-1113`).
2. **The wait ends on the stop.** `resolveForRequest` waits while the
   catalog is `.loading` (a first load) or failed with a retry in flight (2 s
   at most), and returns `error.Cancelled` when the turn's cancel flag is set
   (F2, `model_cache_runtime.zig:843-866`, `:855-865`). The flag is the
   worker's stop (F3, `main.zig:2068-2071`); Fast and Ultra check it first as
   well (`app_callbacks.zig:912-928`).
3. **The turn ends interrupted, prompt kept.** A `Cancelled` there, or a stop
   seen right after the wait, goes to `persistInterruptedTurnOnce` and
   returns (F1, `orchestrator.zig:5893-5919`, `:5923-5948`). The interrupted
   turn keeps `.user = { prompt, images }` and finishes `.interrupted` (F4,
   `interruption.zig:21-49`, `:196-224`). Both checks come before the skill
   catalog and before any request.
4. **Ultra's refusal belongs to the request.** `resolveUltrafastProviderOptions`
   runs at `orchestrator.zig:7657`, inside the request loop (F5), so fx never
   refuses a turn that was stopped in the wait.
5. **What the user sees.** One `Cancelled` line with no entry under it, and
   the program goes idle (`../stop-next/fx-src.txt`, `app_session_runtime.zig:
   2581-2595`, `:2635-2645`; `program_status.zig:57-71`). Captured:
   `fx/fx-p5-stop-in-catalog-wait-40x120-settled.txt` has
   `▪ Cancelled · What can fx do differently?` and nothing below it.
   `osc.txt` (from `fx/…-stopped.wire`) shows `idle`, `working`, `idle`, then
   the title `say something`.
6. **The image route is not copied.** The image route's own resolve at the
   first request is a plain `try` (F7, `orchestrator.zig:7500-7516`). Its
   `Cancelled` ends the turn failed and keeps nothing without execution
   (`:6092-6135`, `finalization.zig:226-241`, `:5616-5626`), and the worker
   loop says `request failed: Cancelled` under `system`
   (`app_process_runtime.zig:124-136`, `:150-172`). A cancelled drain still
   shows `error_text` (`../stop-next/fx-src.txt`, `app_worker_runtime.zig:386`).

## RED (`red.txt`: `cbcfa241`, the base plus the tests)

`a_stop_in_a_turns_catalog_wait_ends_it_as_a_stop_and_records_it_as_one`
(`src/tui/worker.rs`) runs two prompts through `turn_loop`. A Ctrl-C quoting
the first lands 300 ms into the first prompt's wait. Each case is checked
against the same run without the stop, for four waits. The test records the
starts, both ends, the timing, and what the session log says of the turns.
On the base:

- **Ultra behind a retry** (opus's recipe): both turns end in
  `Ultra mode is unavailable…`, the stopped one included, and the log is
  empty. Expected: the stop's end, then the prompt kept as
  `interrupted cancelled`.
- **Ultra behind a first load** (the `Gated` fetcher, landing 1.5 s in): the
  same as the retry case.
- **A prompt image behind a retry:** the end is the stop's, but the prompt
  and its image are not kept. The second turn has no image in its history
  and answers. Expected: kept, so the second turn's image route refuses.
- **The default model's window behind a retry** is the control. It already
  ends as the stop and keeps the prompt, and it passes on the base.

The join test now compares each second turn's end with its no-stop control.
It passes on the base (`red.txt`, `1 passed`).

The capture reproduces the RED. `base-xfx/xfx-p5-stop-in-catalog-wait-40x120-stopped.txt`
row 7 reads
`✗ system: Ultra mode is unavailable for this model; run /ultrafast off or choose a model that offers it`,
and `osc.txt` (base) ends in `state=error` and sets no title.

## The fix (`0b4d89be`)

- **`one_turn`** (`src/tui/worker.rs`): once the turn's catalog waits are
  over, a stopped turn is refused nothing on its way to the machine. The
  image refusal is not asked, and Ultra's refusal gives way to the stop
  (`RequestOptions::default()`; nothing is sent). The machine then ends the
  turn as the stop, as it already did for the window case: the prompt is
  kept with its images, the conclusion is `interrupted cancelled`, and the
  end is `TURN_CANCELLED`. This is upstream's order (fx item 3).
- **`run_journaled`** (`src/agent/machine.rs`): a turn stopped before it
  began records the prompt and the stop, and measures no compaction.
  Upstream's check at `:5923-5948` comes before any compaction, and without
  this a stopped turn at the compaction point would begin one. Pinned by
  `a_turn_stopped_before_it_began_is_asked_and_stopped_and_measures_nothing`.
- **Why not the brief's candidate** (a bare `return TURN_CANCELLED` after the
  wait): it records nothing, so the window case, which keeps its prompt
  today, would lose it, unlike fx item 3. Mutant `candidate-bare-return` is
  killed below.
- **`docs/parity.md`**, the `/model` row, now says what a stop in either wait
  does. It also records the one wait not copied (fx item 6): xfx ends that
  turn as the stop, keeps nothing as fx does, and shows no `request failed`
  line. The code site says the same (`image_refusal`).

## GREEN

- `green-unit.txt`: the four tests pass at `baaf8e0b`.
- `green.txt`: on `p5-stop-in-catalog-wait` at 40x120 the head pairs
  `before`, `stopped` and `settled` are `normalized-identical`. Row 5 differs
  only in the product name in `What can fx do differently?`, and the
  ledger's normalization covers it. On the base, `stopped` and `settled` are
  `unresolved-defect` (row 7, the Ultra line).
- `osc.txt`: the head's wire matches fx's (`idle`, `working`, `idle`, title
  `say something`). The base adds `state=error` and sets no title.
  [inferred] The title comes from the kept prompt, as fx's comes from the
  interrupted turn it keeps.

## Mutants (`mutants.py` → `mutants.txt`)

Each mutant ran on a `git clone --no-local` copy (`mut/` at `0b4d89be`,
`ctl/` at `9298583e`), never on the worktree, under `suite-lock.sh`, with a
600 s process-group limit.

| mutant | where | result |
|---|---|---|
| `ultra-refusal-not-yielded` | `one_turn`: Ultra's refusal does not give way to the stop | killed (the new test) |
| `image-refusal-asked-when-stopped` | `one_turn`: the image refusal is asked of a stopped turn | killed (the new test) |
| `candidate-bare-return` | the brief's candidate in place of the fix | killed (the new test) |
| `no-machine-guard` | `run_journaled` measures a compaction for a stopped turn | killed (the machine test) |
| `stop-leaks-to-next-turn` | `Cancellation::turn` does not reset the mirror | killed on the head, at the join test's second-turn check (`worker.rs:17530`); **survives** the join test as `85b13f84` left it (`ctl/`) |

## Sweep (`sweep-base.md`, `sweep-head.md`, `sweep-compare.txt`)

The sweep covered 18 scenarios (`model-*`, `p5-lifecycle-osc-stopnext`,
`p5-input-model*`, `p5-stop-in-catalog-wait`): 73 pairs, `--jobs 4`, under
the lock. The base passes 63/73, the head 62/73. Two pairs are gained, this
unit's. Three are lost, `model-column-idle`'s, and the cause is fx's clock,
not the change. In the head's run fx's launch read came at 6529 ms, so its
second read (12650 ms) landed after the 9 s outage, while xfx's reads (864,
6964 ms) both landed inside it. That is FLAKE-MODELSDOWN's skew
(`../models-reask/README.md`, the timing table and the paragraph after it).
Two reruns of that scenario on the head pass 3/3, with both sides' reads
inside the outage (`sweep-head-idle-rerun1.md`, `-rerun2.md`,
`sweep-compare.txt`). The other eight unresolved pairs are the same on both
sides.

## Gate (`gates.sh` → `gates.txt`, logs not kept)

See `gates.txt`. Every CI step and sweep.yml's two test scripts ran one at a
time in the worktree, inside one suite-lock slot.
