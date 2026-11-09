## Scoreboard (v0.0.16)

- fx: pin `v0.0.16` (fx 0.0.16), sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked before and after the run
- xfx: `x𝒇x v0.1.0-ccced1f [debug] · Run /help for commands`, sha256 `e6d36b282f0ac117d6283aa5a72d79ee6a8fbd4f2beef8d5bbeeafd444ae87c0`
- size 40x120, `--jobs 3`, 24 scenarios -- **PARTIAL**: `--only p5-catalog-bounds-* p5-input-model* model* h1-models* settings-model*`, wall 4m16s, load 7.4 at the start and 7.4 at the peak (1-minute average, sampled every 5 s)
- ledger sha256 `3821cfc8eec3c9d710d109d3b0e47b4cea1a5fd965ea2540c9f517b08a97c0e3`: 14 normalizations; 19 divergences approved for this pin

fx's inventory (v0.0.16): 45 surfaces -- 38 commands, 7 key hints. Covered by a scenario: 45. **Uncovered: 0**.
Tokens typed as commands that are no command of this inventory (aliases, partial names, refusals): `/al`, `/balance`, `/co`, `/cost`, `/de`, `/e`, `/ef`, `/eff`, `/et`, `/ex`, `/exit`, `/f`, `/fo`, `/img`, `/mo`, `/nosuch`, `/notacommand`, `/q`, `/re`, `/se`, `/setup`, `/st`, `/vers`.

Scenarios: 24 -- ran 24, failed 0, timeout 0. **pass = 78 / 97 = 80.4%**

| total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout |
|---|---|---|---|---|---|---|---|---|---|
| 97 | 97 | 0 | 78 | 0 | 0 | 0 | 19 | 0 | 0 |

The unit is the snap pair: one `snap` or `snapfull` label of a scenario, read from its .cap file, so a scenario that failed or timed out counts every snap it would have taken. pass = (identical + normalized-identical) / total. approved-divergence-cascade is a pair that differs by position beyond its approved rows (a row they pushed down) and whose every row left without a match, the two sides aligned as sequences, is approved; the positional diff stays in the raw output. Groups are the leading letters of a scenario's name (`model-anchor`, `model2` and `model` are `model`). A scenario counts under every surface it exercises, so the surface rows overlap.

| group | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| h | 1 | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| model | 15 | 73 | 73 | 0 | 70 | 0 | 0 | 0 | 3 | 0 | 0 | 95.9% |
| p | 6 | 14 | 14 | 0 | 4 | 0 | 0 | 0 | 10 | 0 | 0 | 28.6% |
| settings | 2 | 8 | 8 | 0 | 2 | 0 | 0 | 0 | 6 | 0 | 0 | 25.0% |

Causes of the unresolved snaps:

19 unresolved snaps; a snap counts under every cause of one of its rows (`only`: that cause alone).
Kinds: a = awaiting a user decision, b = a unit in flight, c = the capture (nondeterminism, or the scenario script), d = a defect.

| cause | kind | snaps | only | what | owner |
|---|---|---|---|---|---|
| sound-max | d | 6 | 6 | `fx's sound has a third level, 'max'; xfx's (the terminal bell) has none: Sound level is '…` | `unowned -- xfx src/sound.rs:37; src/tui/settings.rs:361 / fx src/core/config/settings_catalog.zig:280@c1db9191; src/core/app/app_commands.z…` |
| C7-help-entries | b | 1 | 1 | `xfx's /model summary is the line shell's; C7 r2 gives every registry row fx's help entry.` | `C7 r2 feat/tui-phase4-alias 8d83f961` |
| activity-tokens-before-request | b | 1 | 1 | `fx v0.0.16 publishes a turn's first token progress when its first request is prepared ('p…` | `ACTIVITY-TOKENS (a new queue unit): the activity row's token suffix appears with the turn's first request` |

Rows no cause covers (11 snaps):
- `model-anchor/alt-enter` fx row 6: `  xai/grok-4.7`
- `p5-catalog-bounds-2mib/menu` fx row 5: `Models 8  [All]  Anthropic  OpenAI  xAI  Z.AI  Others`
- `p5-catalog-bounds-at/menu` fx row 5: `Models 8  [All]  Anthropic  OpenAI  xAI  Z.AI  Others`
- `p5-catalog-bounds-over/menu` fx row 5: `Models 0  [All]`
- `p5-catalog-bounds-stall/timed-out` fx row 7: `Could not reach AI Gateway; xfx retries automatically.`
- `p5-input-model/q-cs45` fx row 5: `Models 1  [All]  Anthropic  OpenAI  xAI  Z.AI  Others`
- `p5-input-model/q-anthropic-opus` fx row 5: `Models 1  [All]  Anthropic  OpenAI  xAI  Z.AI  Others`
- `p5-input-model/q-sonnet45` fx row 5: `Models 1  [All]  Anthropic  OpenAI  xAI  Z.AI  Others`
- `p5-input-model/q-zai-glm` fx row 5: `Models 1  [All]  Anthropic  OpenAI  xAI  Z.AI  Others`
- `p5-input-model/q-grok-fast` fx row 5: `Models 2  [All]  Anthropic  OpenAI  xAI  Z.AI  Others`
- `p5-input-model/inline-gpt6` fx row 5: `         openai/gpt-6-astra`

Most unresolved snaps (scenario, unresolved of total, its first unresolved row):

- `p5-input-model` 6/7 -- q-cs45 row 5: `Models 1  [All]  Anthropic  OpenAI  xAI  Z.AI  Ot…` / `Models 0  [All]  Anthropic  OpenAI  xAI  Z.AI  Ot…` (aligned: 2/2 rows unmatched)
- `settings-model-list` 4/5 -- list row 17: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched)
- `settings-model` 2/3 -- marked row 18: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched)
- `model` 1/12 -- typed row 7: `  /model   choose what model and reasoning effort…` / `  /model   show the model, or '/model <id>' to us…` (aligned: 1/1 rows unmatched)
- `model-anchor` 1/2 -- alt-enter row 6: `  xai/grok-4.7` / `  no matching models` (aligned: 1/1 rows unmatched)
- `model-parked` 1/5 -- menu-loading row 5: `~ Thinking (<s>)` / `~ Thinking (<s>) (↑4 ↓0)` (aligned: 1/1 rows unmatched)
- `p5-catalog-bounds-2mib` 1/1 -- menu row 5: `Models 8  [All]  Anthropic  OpenAI  xAI  Z.AI  Ot…` / `Models 0  [All]` (aligned: 11/11 rows unmatched)
- `p5-catalog-bounds-at` 1/1 -- menu row 5: `Models 8  [All]  Anthropic  OpenAI  xAI  Z.AI  Ot…` / `Models 0  [All]` (aligned: 11/11 rows unmatched)
- `p5-catalog-bounds-over` 1/1 -- menu row 5: `Models 0  [All]` / `Models 8  [All]  Anthropic  OpenAI  xAI  Z.AI  Ot…` (aligned: 11/11 rows unmatched)
- `p5-catalog-bounds-stall` 1/2 -- timed-out row 7: `Could not reach AI Gateway; xfx retries automatic…` / `Unable to load models.` (aligned: 1/1 rows unmatched)
