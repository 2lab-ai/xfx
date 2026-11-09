# catalog-bounds round 2: the reviews' fixes

Round 1 ended at `fb8d0249`; both reviews (gpt6, opus) had no MUST and
named the fixes below. Every claim here is tied to a file in this folder or
to a line of the branch.

| commit | what |
|---|---|
| `4e44149f` | RED: a llmux answer past 1 MiB, and one not UTF-8, are malformed -- one test each (`red-llmux.txt`) |
| `1c74f828` | the fix (`fetch_loopback_bounded` says which failure; `fetch_catalog` maps it); a `/credits` non-UTF-8 test -- **the source head** |
| `c9c7aab7` | fakegw holds `modelspad`/`modelsstall` to their bounds itself, and refuses a catalog longer than its padding; two harness tests |
| `b56660d1` | `docs/parity.md` and the round-1 receipt corrected |
| `fc0cb949` | `mutants.py` -- **the gate's head** |
| (next) | `mutants.py` builds outside the watchdog and counts only a named failing test |
| (this one) | this receipt |

## 1. The `/model` row's stale sentence

`docs/parity.md` said "a deadline met while the body is read is the input
cluster's U10" beside this unit's own sentence. It now says the retry sentence
is said "for a read whose send times out, or whose body is still coming at the
deadline (`GatewayCatalog::fetch`, CATALOG-BOUNDS)" (`b56660d1`).

## 2. What the 16 MiB bounds

The cap is on the body's accumulated length as it is read off the wire, not on
memory: the buffer can grow to just under twice it (`Vec` doubling), and the
parsed document (`serde_json::Value`, `parse_gateway_catalog`,
`src/provider/model.rs:286`) is a multiple of the body -- as in fx, which also
parses the whole answer. Said so in parity.md's `/model` row and in round 1's
Security bullet (`b56660d1`).

## 3. A llmux answer that is no document is malformed

Round 1 mapped every `fetch_loopback_bounded` failure to `Unavailable`, whose
sentence ends "check the daemon is running" -- wrong for a daemon that
answered. `fetch_loopback_bounded` now returns `LoopbackFailure`: `NoAnswer`
(no connection, a failed status, a body cut short) or `NotADocument` (past the
cap, not UTF-8). `fetch_catalog` maps them to `Unavailable` and `Malformed`
(`... check the daemon is an llmux version compatible with xfx, or rerun
\`xfx setup llmux\``). `xfx setup llmux`'s root probe still quotes only the
detail (`.map_err(|err| err.to_string())`, `src/llmux/setup.rs:305-307`); its
`/models` probe already wraps `Malformed` as "`GET /models` did not answer a
model catalog: <detail>" (`setup.rs:286-300`, unchanged).

| test | at `4e44149f` (`red-llmux.txt`) | at the head (`green.txt`) |
|---|---|---|
| `a_llmux_catalog_past_1_mib_is_malformed_not_cut_to_fit` | `got Err(Unavailable { detail: "the answer is larger than 1 MiB" })` | ok |
| `a_llmux_catalog_that_is_not_utf8_is_malformed` | `got Err(Unavailable { detail: "the answer is not UTF-8 text" })` | ok |
| `a_llmux_catalog_of_1_mib_is_read_and_one_cut_short_is_no_answer` (moved, unchanged) | ok | ok |
| `credits::...an_answer_that_is_not_utf8_is_invalid_json` (item 6, a pin) | -- | ok |

## 4. Round 1's figures, each now at a file

- The head's stall times are its capture log's: 1409 -> 31401 ms
  (`../captures/head-stall-gateway.log`); round 1's README had another run's.
- The test counts: round 1's step logs were not kept, so that README no
  longer states one; this gate's are in `gate-tests.txt`.
- The 5737 ms: the sweep's raw `runs/` that held it was not kept; round 1's
  README and `reruns.txt` no longer state it, and say why.
- The 64 MiB and 120 s: round 1 says they were capture.sh's
  (`scripts/fxcap/capture.sh:633-636`); from `c9c7aab7` fakegw holds them too.

## 5. fakegw's own bounds (`scripts/fxcap/fakegw.ts`, `modelsPad`, `modelsStallMs`)

Run on its own, fakegw refuses to start -- `[fakegw] refused: ...`, exit 2,
before it listens -- on a `FAKEGW_MODELS_PAD` that is not
`<whole bytes up to 67108864> <head|tail>`, a `FAKEGW_MODELS_STALL_MS` that is
not whole milliseconds up to 120000, or a `FAKEGW_MODELS` file already longer
than the padding. A catalog longer than the padding at request time gets a
500 and a `refused` log line, never a body longer than asked.
`test_capture.py`: `test_modelspad_and_modelsstall_out_of_their_bounds_stop_it_before_it_listens`
(seven values out of bounds, and both bounds themselves accepted) and
`test_a_catalog_longer_than_its_padding_is_refused_never_served_longer`.

## Mutants (`mutants.py`, `mutants.txt`)

Nine, on a `git clone --no-local` of the branch at `fc0cb949` in the scratch
directory, each under `../bounded.py` (300 s, 3 GiB). Four on the llmux
mapping (no document as unavailable, too large or not UTF-8 as no answer, a
cut-short body as no document) and five on fakegw (no padding bound, no stall
bound, a padding that is not a whole number, no start-up size check, a catalog
past its padding served). All nine killed by a named test. The first run built
the crate inside the watchdog, which killed the four Rust builds -- not
counted; `mutants.py` (as committed) builds outside it and counts only a named
failing test, and its rerun of those four killed each (`mutants.txt` has both
runs).

## Gate (`../gates.sh`, `gate.txt`, `gate-tests.txt`)

At `fc0cb949` (the source head `1c74f828`, the harness `c9c7aab7`), under the
suite lock: 20 steps, 0 failed. `cargo test` 5469 passed, 0 failed;
`--features fault-injection` 5506 passed, 0 failed (`gate-tests.txt`, every
`test result:` line). "tree clean after: no" is `mutants.py`, edited while
the gate ran (the build-outside-the-watchdog fix above), a receipt script only.

## Sweep

Not rerun: the only source change outside tests is item 3, the llmux
mapping, which no model scenario reaches (none of `model*`, `p5-input-model*`,
`h1-models*`, `settings-model*` names llmux: `grep -il llmux` finds none); fakegw
answers as before for every value the scenarios give it.
