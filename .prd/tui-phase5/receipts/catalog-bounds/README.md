# catalog-bounds: the catalog read whole within fx v0.0.16's 30 s and 16 MiB, or not at all

Unit U10 (I17, **security S**: network reads and a memory bound). fx v0.0.16
bounds every Gateway catalog read with a 30 s deadline over the whole request
and a 16 MiB cap past which the read fails; a timeout is a failure it says it
retries, a body too large a malformed one it does not. xfx had the 30 s, but
read the body through `read_bounded` -- written to quote a *failed* answer --
which cut a success body at 1 MiB, ended silently on a failed read (the
deadline mid-body among them) and decoded lossily: an oversized or stalled
answer became "not a model catalog", and a padded or cut-short one was taken
as a catalog.

Base `origin/main` `ccced1f5`. Every claim below is tied to a file in this folder.

| commit | what |
|---|---|
| `8a8c86b9` | RED: five catalog cases on the base's reader, and a test-only deadline (`red.txt`) |
| `c506f8d5` | the fix: `read_capped`, the Gateway's 16 MiB and its mapping, the llmux reader, usage's copy gone |
| `c6e2d511` | RED: `/credits` reads its answer cut to fit (`red-credits.txt`) |
| `b727f64b` | its fix; clippy's two findings and three line ranges |
| `e1ba18d9` | fxcap: `modelspad`, `modelsstall`, four scenarios, their harness test |
| `c9ebc953` | `docs/parity.md`; fakegw's shaped lines say when -- **the code head** (the captures' binary) |
| `80589da1` | this folder's `gates.sh` and `mutants.py` |
| `2f106a3e` | `docs/parity.md`: the failures xfx retries that fx does not |
| `4b586b42` | a test's margin -- **the gate's head** |
| `6dda7f8a` | the stall's snap waits for `Loading` to go; the llmux mutant builds |
| (this one) | this receipt |

The xfx captures here are from the binary built at `c9ebc953` (row 1:
`x𝒇x v0.1.0-c9ebc95 [debug]`, sha256 `0d36907bd7ad…`, `sweep-head.md`); the
base's from `ccced1f5` (`v0.1.0-ccced1f`, `e6d36b282f0a…`, `sweep-base.md`);
fx is the v0.0.16 release binary, sha256 `8e6cda2e…`, checked by the sweep
before and after each run. In round 1 the commits after `c9ebc953` change no
source outside tests: a parity sentence, a test's margin (`4b586b42`), the
stall scenario's wait and a mutant (`6dda7f8a`), and this folder. Round 2's
`1c74f828` does change source -- `src/provider/model.rs` and
`src/llmux/setup.rs`, the llmux catalog's failure mapping
(`round2/README.md`, item 3) -- and the captures stay valid: every scenario
here runs on the Gateway, and none reaches llmux.

## What fx v0.0.16 does (`fx-cites.txt`, every line quoted)

- **One function reads every Gateway catalog**: `fetchModelCatalogResponse` ->
  `fetchGatewayJsonCancellable` (`src/builtins/gateway.zig:2419-2451`,
  `src/gateway/client.zig:473-509`); "one-shot fetches pass no flag but still
  get the size cap and deadline" (fae2cfbe, `gateway.zig:2446`).
- **30 s over the whole request** (`gateway_json_fetch_timeout_ms`,
  `client.zig:486-488`), run by `runBoundedHttpOperation` on the `.awake`
  clock: connect, head and body alike end in `error.Timeout`.
- **16 MiB, refused past it, not cut** (`max_gateway_json_bytes`,
  `readGatewayJsonBody`, `client.zig:611-622`); an answer of exactly the cap is
  read (its test, `:624-635`: 8 bytes at a cap of 8 pass, 9 fail).
- **What each failure is** (`catalogRequestFailure`, `gateway.zig:2510-2518`):
  `error.Timeout` is a transport failure with `retryable = true`;
  `GatewayJsonResponseTooLarge` is `malformed_response` (`:2521-2537`), whose
  `retryable` keeps its default, `false` (`model_catalog.zig:38-42`). The set
  fx retries besides its deadline is three errors (`isRetryableGatewayError`,
  `client.zig:19-23`). The menu: a retryable transport failure says `Could not
  reach AI Gateway; fx retries automatically.`, anything not retryable `Unable
  to load models.` (`model_menu_presentation.zig:394-400,435-443`). A failed
  catalog, retryable or not, is read again by the next ask after its backoff
  (`model_cache_runtime.zig:40-54`; MODELS-REASK's receipt, point 4-5), so
  too large is not retried on its own.
- **The clock for age and backoff** counts through sleep (`clockMs`,
  `std.Io.Clock.boot`, `model_catalog.zig:72-79`; 47e23ddb, fae2cfbe).

## The fetch table

Every authenticated GET xfx sends the Gateway, and the loopback catalog beside
them (`grep` of `reqwest` bodies in `src/`: `bytes_stream`/`chunk` in
`gateway/mod.rs`, `llmux/mod.rs`, `mcp/transport.rs`, `usage/report.rs`; the
mcp and llmux-turn readers are streams and error quotes, not JSON answers).

| xfx | fx counterpart @481104fd | bounds before -> after | what each failure says |
|---|---|---|---|
| **Gateway catalog**, `GatewayCatalog::fetch` (`src/provider/model.rs`); the only Gateway catalog GET -- `catalog_for` -> the model selector, so the launch's load, `/model`, its inline column and `/model <q>` lookups all ask through it | `fetchModelCatalogResponse` -> `fetchGatewayJsonCancellable`, behind the refresh (`model_catalog_provider`, `gateway.zig:2344`), `fx models` (`fetchCliModelCatalog`, `:1035`) and the one-shots (`fetchModelIds[Cancellable]`, `session_commands.zig:1287`, `main.zig:1997`) -- **same constants** | 30 s total, 10 s connect, **1 MiB cut to fit** -> 30 s total, 10 s connect, **16 MiB refused past it** | timeout (connect, head, mid-body) -> `Could not reach AI Gateway; xfx retries automatically.`; past 16 MiB -> `Unable to load models.`; a read that stops partway, or bytes not UTF-8 -> `Unable to load models.`; no answer at all -> the retry sentence (MODELS-REASK's, wider than fx's three, `docs/parity.md` `/model`) |
| **llmux catalog**, `fetch_catalog` / `fetch_loopback_bounded`, and `xfx setup llmux`'s two probes | none: fx has no llmux | 3 s connect, 5 s total, **1 MiB cut to fit** -> the same numbers, **refused past them** | round 1: every failure `catalog is unavailable: ...; check the daemon is running`; from round 2, past 1 MiB or not UTF-8 `catalog is malformed: ...; check the daemon is an llmux version ...`, and only no whole answer (no connection, a failed status such as 404 or 503, a body cut short) unavailable; the menu `Unable to load models.` as before |
| **`/credits`**, `credits::ask_at` | `fetchGatewayGetResult` (`client.zig:438-471`): no deadline, no cap -- **not the same constants** | 64 KiB cut to fit -> 64 KiB refused past it (a refusal's body still only quoted) | past it `invalid JSON response from gateway`; cut short `failed to fetch credits from gateway` (fx's, `gateway.zig:813-815`) |
| **usage report**, `usage::report::get` | `fetchUsageGet` (`client.zig:284-309`): `usage_request_timeout_ms`, `usage_response_max_bytes` -- its own constants | unchanged (it already refused past its cap); its private reader is now the shared `read_capped` | unchanged: `TOO_LARGE`, `CUT_SHORT` |

After the change `read_bounded` is called only after a non-success status:
`gateway/mod.rs:885`, `llmux/mod.rs:183`, `credits.rs` (the `status != 200`
branch) -- `green.txt`'s last section, a `grep` at the head.

## RED -> GREEN

On the base (`red.txt`, `red-credits.txt`; each run under `bounded.py`, 150 s
and 3 GiB, because Darwin does not enforce `ulimit -v`; peak 97 MiB):

| test | base | head (`green.txt`) |
|---|---|---|
| `a_gateway_catalog_up_to_16_mib_loads` (2 MiB declared; 16 MiB declared and chunked; padded before) | `2 MiB, declared: ... "the answer is not a model catalog"` | ok |
| `a_gateway_catalog_one_byte_past_16_mib_fails_and_is_not_cut_to_fit` (padded after) | `declared: expected a refusal, got Ok([acme/one])` | ok: `the answer is larger than 16 MiB`, not retried |
| `a_gateway_catalog_past_its_deadline_is_a_failure_asked_again_about` (300 ms test deadline: no answer, half a body, a byte every 50 ms) | `half a body: ... "the answer is not a model catalog", retry: None` | ok: `Unreachable`, under 3 s |
| `a_gateway_catalog_cut_short_or_not_text_is_no_catalog` | `a body cut short: Ok([acme/one])` | ok |
| `a_llmux_catalog_past_1_mib_or_cut_short_is_no_catalog` | `expected a refusal, got Ok([fake/model-a])` | ok |
| `credits::...an_answer_past_64_kib_or_cut_short_is_not_read_as_one` | both read as `Credits { balance: "42.17", ... }` | ok |

## The oracle: four scenarios, fx against xfx (`captures/`, `cells.txt`)

fakegw gained two catalog modes, and capture.sh two verbs: `modelspad <bytes>
<head|tail>` (the catalog padded with spaces to exactly <bytes>, in memory) and
`modelsstall <ms>` (head and half the body, the rest <ms> later). In round 1
capture.sh alone held them to 64 MiB and 120 s (`scripts/fxcap/capture.sh:633-636`);
round 2 has fakegw hold them too, and refuse a catalog longer than its padding
(`round2/README.md`). `test_capture.py` `test_modelspad_and_modelsstall` and
the refusal rows test them (gate step 16). The 30 s stall is practical: the
scenario is about 35 s a binary.

| scenario, snap | fx | base `ccced1f5` | head `c9ebc953` |
|---|---|---|---|
| `p5-catalog-bounds-over` menu (16 MiB + 1, padded after) | `Models 0` / `Unable to load models.` | `Models 8`, the catalog cut to 1 MiB taken | identical |
| `p5-catalog-bounds-at` menu (exactly 16 MiB, padded before) | `Models 8` and its rows | `Models 0` / `Unable to load models.` | identical |
| `p5-catalog-bounds-2mib` menu (2 MiB, padded before) | `Models 8` and its rows | `Models 0` / `Unable to load models.` | identical |
| `p5-catalog-bounds-stall` loading | `Loading models…` | identical | identical |
| `p5-catalog-bounds-stall` timed-out | `Could not reach AI Gateway; fx retries automatically.` | `Unable to load models.` | normalized-identical (fx's name, ledger rule `product-name`) |

Each binary gave up the stalled read 30.0 s after it began, the base too
(`captures/{fx,head,base}-stall-gateway.log`, the `stalled after` and `the
client went` lines, ms since each gateway started: fx 791 -> 30794, head
1409 -> 31401, base 819 -> 30817): the base had fx's deadline, and read what it
cut off as no catalog. The stall pair is from its own sweep after
`6dda7f8a` (`sweep-stall-head.md`, `sweep-stall-base.md`); the other three
from the model sweeps.

## Sweep: the model scenarios, base against head

`--only p5-catalog-bounds-* p5-input-model* model* h1-models* settings-model*`,
24 scenarios, 97 pairs (`sweep-base.md`, `sweep-head.md`, `sweep-compare.txt`):
base 78 / 97, head 80 / 97. Up 4: the four catalog-bounds pairs above. Down
2: `model-column-reask` `col-down` and `col-back` -- the ledger's
`catalog-window` cause (FLAKE-MODELSDOWN: `modelsdown` counts from the
gateway's start, so a binary that asks late has its retry fall after the
outage; `sweep-compare.txt` shows fx with the rows where the base run had fx
fail too). The fx gateway log that showed the late ask was in the sweep's raw
`runs/`, which is machine-only and was not kept, so its time is not carried
here. Alone on the head binary three times under the lock, 3 / 3 each time,
fx asking at 762-952 ms (`reruns.txt`). The 15 that fail both are the same rows in both, outside this
unit (`/model`'s help line, fuzzy search, settings' sound level).

## Mutants (`mutants.py`, `mutants.txt`)

Thirteen, each on a `git clone --no-local` of the branch in the scratch
directory, the unit's tests under `bounded.py` (300 s, 3 GiB), the edit undone
after. All killed by a test, one after it was replaced: "llmux: past its cap
is no failure" did not build, so `6dda7f8a` lifts llmux's cap instead, and a
test kills that. Among them: no declared-length check, the exact cap refused,
no cap while reading, a failed read keeping its bytes, the Gateway back at
1 MiB, a mid-body deadline not retried, every failed read retried, too large
retried, lossy UTF-8, the test deadline ignored, and both `/credits` arms.

## Gate (`gates.sh`, `gate.txt`)

At `4b586b42`, under the suite lock: 20 steps, 0 failed -- fmt, both clippys,
`cargo test` and `--features fault-injection` (their pass counts were in the
step logs, not kept; round 2's gate carries its own, `round2/gate-tests.txt`), six
`check-*` scripts and three `test-*` ones, both fxcap self-tests, both release
builds, smoke and smoke-tui. "tree clean: no" is this folder's drafts,
untracked then. After it, `6dda7f8a` changed the stall scenario and
`mutants.py` only: the two stall sweeps ran that scenario (sweep.py refuses a
racy one before it runs), and `scripts/fxcap/test-sweep.sh` passes at the
receipt's head (27 tests, `test-sweep.txt`).

## Security

- No new credential read: the bearer is the one `GatewayCatalog` and
  `/credits` already held, still `set_sensitive(true)`; nothing new is printed
  from a body (an error is a fixed sentence, a size, or reqwest's own error,
  which never carries the header).
- The bound is on bytes read off the wire, now a refusal, not a cut: a
  declared length past the cap is refused before a byte is read, and the
  body's accumulated length never passes the cap while it is read (the
  chunk that would cross it is dropped). It is not a ceiling on memory: the
  buffer can grow to just under twice the cap (`Vec` doubling), and the
  parsed document (`serde_json::Value`, `src/provider/model.rs`
  `parse_gateway_catalog`) is a multiple of the body, as in fx, which also
  parses the whole answer.
- Every fake is loopback and finite: the test servers end by themselves
  within 10 s; fakegw's padding (64 MiB) and stall (120 s) were bounded by
  capture.sh in round 1 (`capture.sh:633-636`) and by fakegw itself from
  round 2.

## Divergences, named in `docs/parity.md`

- `/model`: the 10 s connect inside fx's 30; the failures xfx retries that fx
  does not (MODELS-REASK's, unchanged); llmux's own 3 s / 5 s / 1 MiB; and
  **the catalog's clock stops in sleep** (`Instant`), where fx's
  (`std.Io.Clock.boot`) counts it -- found here, not copied.
- `/credits`: 64 KiB, read whole or not at all, where fx reads with no cap
  and no deadline.
