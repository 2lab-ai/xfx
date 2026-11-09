## Scoreboard (v0.0.16)

- fx: pin `v0.0.16` (fx 0.0.16), sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked before and after the run
- xfx: `x𝒇x v0.1.0-0b4d89b [debug] · Run /help for commands`, sha256 `0c1173b7e95886bbc04e83c1e6878746172e08dd3fb2a42e8e791df93673d9b6`
- size 40x120, `--jobs 4`, 18 scenarios -- **PARTIAL**: `--only model-* p5-lifecycle-osc-stopnext p5-input-model* p5-stop-in-catalog-wait`, wall 2m45s, load 7.6 at the start and 9.3 at the peak (1-minute average, sampled every 5 s)
- ledger sha256 `530ac71a1831b725c62f2afd16fc02fc6e7a8695c1f60a4af84cc6b85a358c56`: 14 normalizations; 19 divergences approved for this pin

fx's inventory (v0.0.16): 45 surfaces -- 38 commands, 7 key hints. Covered by a scenario: 45. **Uncovered: 0**.
Tokens typed as commands that are no command of this inventory (aliases, partial names, refusals): `/al`, `/balance`, `/co`, `/cost`, `/de`, `/e`, `/ef`, `/eff`, `/et`, `/ex`, `/exit`, `/f`, `/fo`, `/img`, `/mo`, `/nosuch`, `/notacommand`, `/q`, `/re`, `/se`, `/setup`, `/st`, `/vers`.

Scenarios: 18 -- ran 18, failed 0, timeout 0. **pass = 62 / 73 = 84.9%**

| total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout |
|---|---|---|---|---|---|---|---|---|---|
| 73 | 73 | 0 | 62 | 0 | 0 | 0 | 11 | 0 | 0 |

The unit is the snap pair: one `snap` or `snapfull` label of a scenario, read from its .cap file, so a scenario that failed or timed out counts every snap it would have taken. pass = (identical + normalized-identical) / total. approved-divergence-cascade is a pair that differs by position beyond its approved rows (a row they pushed down) and whose every row left without a match, the two sides aligned as sequences, is approved; the positional diff stays in the raw output. Groups are the leading letters of a scenario's name (`model-anchor`, `model2` and `model` are `model`). A scenario counts under every surface it exercises, so the surface rows overlap.

| group | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| model | 14 | 61 | 61 | 0 | 56 | 0 | 0 | 0 | 5 | 0 | 0 | 91.8% |
| p | 4 | 12 | 12 | 0 | 6 | 0 | 0 | 0 | 6 | 0 | 0 | 50.0% |

Causes of the unresolved snaps:

11 unresolved snaps; a snap counts under every cause of one of its rows (`only`: that cause alone).
Kinds: a = awaiting a user decision, b = a unit in flight, c = the capture (nondeterminism, or the scenario script), d = a defect.

| cause | kind | snaps | only | what | owner |
|---|---|---|---|---|---|
| catalog-window | c | 3 | 3 | `'modelsdown <ms>' is wall-clock from the gateway's start, and each binary launches a diff…` | `FLAKE-MODELSDOWN (registry #19): scenario + fakegw -- fix: an outage counted in catalog requests, or from a scenario line, rather than mill…` |
| activity-tokens-before-request | b | 1 | 1 | `fx v0.0.16 publishes a turn's first token progress when its first request is prepared ('p…` | `ACTIVITY-TOKENS (a new queue unit): the activity row's token suffix appears with the turn's first request` |

Rows no cause covers (7 snaps):
- `model-anchor/alt-enter` fx row 6: `  xai/grok-4.7`
- `p5-input-model/q-cs45` fx row 5: `Models 1  [All]  Anthropic  OpenAI  xAI  Z.AI  Others`
- `p5-input-model/q-anthropic-opus` fx row 5: `Models 1  [All]  Anthropic  OpenAI  xAI  Z.AI  Others`
- `p5-input-model/q-sonnet45` fx row 5: `Models 1  [All]  Anthropic  OpenAI  xAI  Z.AI  Others`
- `p5-input-model/q-zai-glm` fx row 5: `Models 1  [All]  Anthropic  OpenAI  xAI  Z.AI  Others`
- `p5-input-model/q-grok-fast` fx row 5: `Models 2  [All]  Anthropic  OpenAI  xAI  Z.AI  Others`
- `p5-input-model/inline-gpt6` fx row 5: `         openai/gpt-6-astra`

Most unresolved snaps (scenario, unresolved of total, its first unresolved row):

- `p5-input-model` 6/7 -- q-cs45 row 5: `Models 1  [All]  Anthropic  OpenAI  xAI  Z.AI  Ot…` / `Models 0  [All]  Anthropic  OpenAI  xAI  Z.AI  Ot…` (aligned: 2/2 rows unmatched)
- `model-column-idle` 3/3 -- col-down row 5: `         openai/gpt-6-astra` / `         unable to load models` (aligned: 4/4 rows unmatched)
- `model-anchor` 1/2 -- alt-enter row 6: `  xai/grok-4.7` / `  no matching models` (aligned: 1/1 rows unmatched)
- `model-parked` 1/5 -- menu-loading row 5: `~ Thinking (<s>)` / `~ Thinking (<s>) (↑4 ↓0)` (aligned: 1/1 rows unmatched)
