# sweep-wallclock: the catalog outage ends at a scenario line, not on the gateway's clock

Unit SWEEP-WALLCLOCK-F (FLAKE-MODELSDOWN, registry #19): harness work, no
`src/` change. Base `3be49c15` (train 11's head). Every claim below is tied to
a file in this folder.

| commit | what |
|---|---|
| `7d6b552a` | the harness (`fakegw.ts`, `capture.sh`), its tests, the eight scenarios, the ledger -- the code head; the GREEN sweeps' xfx binary was built here (banner `v0.1.0-7d6b552`, `green/run-*-scoreboard.md`) |
| `80cdb3dc` | this receipt, but for the gate's files -- the gate's head |
| `20c7fbd9` | `gates.txt`, `gate-counts.txt`, `gate_counts.sh`, the stop-in-catalog-wait line of `states.py`, this README's edits -- round 1's head |
| `476829ad` | round 2: the column scenarios release the catalog after `until shown unable to load models`; `model-column-reask` `col-held`; `model-parked`'s comment -- round 2's code head (§ Round 2) |
| the next commit | round 2's receipt |

## The gap

- `modelsdown <ms>` answered 503 while `asked - started < ms`, `started` being
  the fake gateway's own start (`scripts/fxcap/fakegw.ts:110`, `:444-450@3be49c15`).
  capture.sh starts the gateway before it launches the binary, and a scenario's
  clock starts at the binary's first frame (`capfile.py:202`).
- **Where the launch read lands** (`probe-up.txt`, from a scratch copy of
  capture.sh that asks the gateway for `/probe-up` right after its launch wait
  sees the first frame: `probe.patch`, `probe-up.cap`): in 6 of 6 runs, 3 a
  binary, the launch's catalog read came **45 to 131 ms before the first
  frame**, while the first frame itself came 1.5 to 11.9 s after the gateway
  started (one xfx took 8.80 s to draw). So the launch read came before the
  first frame in 6 of 6 runs, within 45 to 131 ms of it, while its distance
  from the gateway's start ran over seconds. A scenario line counts from the
  first frame; the change removes the outage's dependence on how long the
  gateway was up before the launch. It does not make scheduling fixed: under
  load a line can still run late, which the scenarios below allow for where
  it matters (`until shown`, the holds' margins).
- **RED, the old forms on the base** (`sweeps.sh` x5 under the suite lock,
  `red/`; binary banner `v0.1.0-3be49c1`):
  - `red-verdicts.txt`: **3 of 25 pairs flipped** in 5 sweeps --
    `model-reask` `menu-failed` (UD in run 1, `catalog-window`),
    `model-parked` `menu-loading` (UD in run 1 with a row no cause covers),
    `p5-input-model-fail` `fail-open` (UD in run 3, a row no cause covers;
    `red/flips/`: fx says `Could not reach AI Gateway`, xfx lists 8 models).
  - The pairs that did not flip mostly did not test the outage
    (`red-states.txt`, `red-asks.txt`): each capture's first catalog read
    came 2.8 to 22.4 s after its gateway started, mostly past the outage's
    end, so `model-column-idle` and
    `model-column-reask` `col-down` -- the column over the failure -- **listed
    the rows on both binaries in 5 of 5 runs**; `model-reask` `menu-failed`
    showed the failure on one binary in one run; `p5-stop-in-catalog-wait`'s
    launch read was answered 200 in all 10 captures, so its turn never waited
    on a failed catalog's retry, the case it exists for.

## The mechanism (`7d6b552a`)

- `modelsdown` takes no argument: the catalog path answers 503 from the launch
  until a `modelsup` line, or for the whole run without one
  (`capture.sh:47-54`, `:594-596`). `modelsup` renames a file into the run
  directory (`capture.sh:886`, the FLAKE-MCPMODE rule), and fakegw.ts reads
  `FAKEGW_MODELS_UP` on every ask: 503, at once, until that file exists
  (`fakegw.ts:444-453`). The `-> 503 (down)` log line is unchanged.
- **`modelsdown <ms>` is retired**: refused at parse time, before anything
  launches, as `models2 <file> <ms>` was (`capture.sh:665`), so a scenario on
  another branch fails loudly instead of being read anew. `modelsup` with an
  argument or with no `modelsdown` line is refused too (`:663`, `:681`).
- No scenario needed wall time: `model-floor` (600 s) and
  `p5-effort-fork-effort-down` (60 s) meant "down throughout"; the other six
  meant "down at launch, back later".
- Not chosen: an outage counted in asks. Both binaries' asks are the same per
  scenario today (`green-verdicts.txt`, the ask columns), but a count would
  make an extra harmless ask during the outage change what a later snap
  shows; a line keeps the outage on the scenario's clock, where the probe
  puts the launch read.

## Per scenario (RED `red-*.txt`, GREEN `green-*.txt`; 5 sweeps each, fx and xfx)

| scenario | what its snaps show (GREEN) | form | RED flips | GREEN flips; asks, both binaries, every run |
|---|---|---|---|---|
| `model-column-idle` | `col-down` the column over the failure; `col-idle`, 11 s on and past the column read's 10 s hold, still the failure (idle: no frame asks); `col-typed` a keystroke's frame reads the rows | `modelsup` after `col-down` (round 2: after `until shown unable to load models`); `wait 6` -> `wait 11`; `until shown gpt-6-astra` for `col-typed` | 0 (failure never on screen) | 0; 503 503 200 (round 2: the same, 5 of 5) |
| `model-column-reask` | `col-down` the failure while a turn streams; round 2's `col-held`, 3 s after the release and inside the column read's 10 s hold, still the failure; `col-back` the rows, read by the animated column's first frame past the hold | `modelsup` after `col-down` (round 2: after `until shown unable to load models`); `until shown gpt-6-astra` for `col-back` | 0 (failure never on screen) | 0; 503 503 200 (round 2: the same, 5 of 5) |
| `model-reask` | `menu-failed` the sentence (inside the launch's 5 s hold: no read); `reask-midturn` the second `/model` reads the rows mid-turn | `modelsup` before the prompt; `until shown gpt-6-astra` for `reask-midturn` | 1 (`menu-failed`) | 0; 503 200 |
| `model-parked` | the prompt past the hold starts the read (answered 8 s later); `menu-loading` inside the turn's 2 s wait for it | `modelsup` at 6.5 s, before the prompt | 1 (`menu-loading`) | 0; 503 200 |
| `p5-input-model-fail` | `fail-open` the sentence; `after-outage` the rows | `modelsup` after `fail-open`; `until shown gpt-6-astra` for `after-outage` | 1 (`fail-open`) | 0; no gateway log kept |
| `p5-stop-in-catalog-wait` | the prompt's read (10 s) behind a failed launch read; Ctrl-C inside the turn's wait for it | `modelsup` at 8 s, before the prompt | 0 (its launch read was never refused) | 0; 503, then the prompt's read still on its way, no completion sent |
| `model-floor` | `/model` over an outage that never ends | down throughout | 0 | 0; 503 503 |
| `p5-effort-fork-effort-down` | `/effort` with no catalog | down throughout | 0 | 0; no gateway log kept |

`green-states.txt`: every failure snap above shows the failure and every
recovery snap the rows, on both binaries, in 5 of 5 runs. Its last line is
`p5-stop-in-catalog-wait`, whose three screens read the same whichever wait
its turn is in (`Cancelled`, then idle; `green/run-1/` as in the
STOP-IN-CATALOG-WAIT receipt's `fx/`): from the gateway logs, in GREEN the
launch read was refused, the prompt started one more read, and no completion
went, in 10 of 10 captures -- that receipt's own log, the case it is for; in
RED (`red-states.txt`) the launch read was answered 200 ten seconds later and
nothing else was read, so its turn waited on the first load instead.

One scenario says more than it did: `model-column-idle` waits 11 s, not 6,
between `col-down` and `col-idle`, so `col-idle` falls past the column read's
10 s hold -- "an idle column asks nothing even when it may" -- and the
keystroke after it reads the rows; at 6 s it fell inside the hold, where
neither binary may ask and `col-typed` could only show the failure. `model-column-hold`
already used a scenario line (`models2`); its comment no longer calls
`modelsdown` a gateway-clock outage.

## Round 2 (`476829ad`; reviews of `20c7fbd9`, opus SHOULD 1-4 and gpt6's line)

- **Three lines that were false, fixed.** `model-parked.cap`'s comment named
  `/model` as the read after the launch's; it is the prompt's, 6.6 s after
  the launch read (`green-asks.txt`: fx in run 1 asked at 7604 ms and
  14228 ms), and `/model` opens over that read. `gate_counts.sh` said it
  dropped each test binary's build path and did not for `tests/*.rs`; its
  sed now does (`gate-counts.txt`, regenerated from the same step logs). And
  § The gap said a line is a fixed distance from the launch read, which the
  probe does not show; it now says what the probe shows.
- **The release waits for the column on the screen.** In both column
  scenarios `modelsup` now follows `until shown unable to load models`, as
  `model-column-hold.cap:22` does, not only a wait after the column opens.
  The screen says the column is up over the failure; it cannot say the
  column's own read has gone, because the column says the same sentence
  while that read is on its way (`model-column-hold.cap`'s comment). That the
  read went before the release is the gateway logs': the column's ask
  answered 503 in every run (`green-r2-verdicts.txt`, the ask columns).
- **What the `until shown` conversions dropped, and what keeps it.** A fixed
  wait before a recovery snap also bounded how soon the rows came; `until
  shown <row>` waits up to 30 s, so a binary whose recovery comes late now
  passes `col-typed`, `col-back`, `reask-midturn` and `after-outage` where a
  fixed wait would have caught it. Two snaps keep a bound where it is the
  point: `model-column-idle` `col-idle` (idle past the 10 s hold: nothing
  reads) and, new, `model-column-reask` `col-held` (3 s after the release,
  inside the hold, while the turn's frames animate: nothing reads before the
  hold ends). The gateway logs record the rest, though no pair checks it: in
  `model-column-reask` the rows' read is the third ask, 10.0 to 10.1 s after
  the column's, on both binaries in every run (`green-r2-asks.txt`) -- the
  first frame past the 10 s hold.
- **The two scenarios, 5 sweeps more** (`green-r2/`, binary banner
  `v0.1.0-476829a`): 7 pairs, **0 flips**,
  all normalized-identical in 5 of 5 (`green-r2-verdicts.txt`); every
  failure snap, `col-held` included, shows the failure and every recovery
  snap the rows, on both binaries (`green-r2-states.txt`). Then, at
  `476829ad` under one hold of the suite lock (`r2-harness.sh`,
  `r2-harness-tests.txt`): `test-capture.sh` 81 tests OK, `test-sweep.sh`
  70 and 27 OK.

## The ledger

- **Retired: `catalog-window`** (nondeterminism, FLAKE-MODELSDOWN); a comment
  in its place says why (`parity-ledger.toml:1058-1066`). No sibling cause was
  the window's: `model-parked`'s is below.
- **Kept, now on every run: `activity-tokens-before-request`** (unit,
  ACTIVITY-TOKENS, `parity-ledger.toml:1069`). `model-parked` `menu-loading`
  is unresolved under that cause and no other in 5 of 5 GREEN runs
  (`green-verdicts.txt`): fx v0.0.16 shows no token suffix while the turn
  waits for the catalog's retry, xfx shows the prompt's estimate. In RED it
  passed in runs 2 to 5 only because both binaries' launch reads were
  answered (`red-verdicts.txt`, the ask columns), so neither turn waited on a
  failed catalog's retry. A real difference, registered, not hidden.
- **Added: none.** No other pair differs in any GREEN run.

## Tests and gate

- `red-harness-tests.txt`: the new tests over the base's capture.sh and
  fakegw.ts -- 5 failures: the gateway answers 200 where the file is absent,
  `modelsdown 6000` is accepted, `modelsup` is an unknown verb, and the end to
  end outage is not down.
- `test_capture.py`: `test_modelsdown_until_modelsup` (end to end over the
  stub binary: two asks 503 with a second between them, `modelsup`, one 200),
  `test_modelsdown_answers_503_until_its_file_is_in_place` (fakegw asked
  directly), and the three refusals in `test_each_verbs_arguments`.
- `gates.txt` (`gates.sh`, train 11's steps, one hold of the suite lock, at
  `80cdb3dc` with a clean tree): **20 steps, 0 failed**, the tree clean
  after. `gate-counts.txt` (`gate_counts.sh`, from the step logs):
  `fxcap-test-capture` ran 81 tests, OK (79 at train 11,
  `train-11/gate-counts.txt`, and this unit's 2 new); `fxcap-test-sweep` 70
  and 27, OK; every cargo test binary of `test` and `test-fault` 0 failed.

## Open

- `research/tui-phase5` (local, the source units copy `p5-*` scenarios
  from) keeps its own old-form `p5-input-model-fail.cap` and
  `p5-effort-fork-effort-down.cap` (`modelsdown 15000`, `60000`). A unit that
  copies one now gets the refusal at parse time, which names the new form.
  No open unit branch on this machine edits or adds a `modelsdown <ms>`
  scenario of its own since its merge-base; theirs are base's copies, which a
  merge of this one replaces.

## Files

`sweeps.sh` (runs the eight scenarios N times; the RED and the GREEN are the
same command over the base and the head), `verdicts.py` (`*-verdicts.txt`),
`states.py` (`*-states.txt`), `asklines.py` (`*-asks.txt`, every catalog line
the gateways logged), `red/` and `green/` (each sweep's scoreboard and the
lock's start and end times with the load; GREEN run 1's captures,
`green/run-1/`; RED's flipped pairs, `red/flips/`), `probe-*` (above);
round 2's `green-r2*` (the same scripts over its two scenarios) and
`r2-harness*`.
