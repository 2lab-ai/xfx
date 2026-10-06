# D2b receipt: `/model`'s inline columns, the reasoning effort, and D2a's gaps, fx c1db919 beside xfx

Captured on 2026-10-07 with the repository's harness. Each run had its own tmux socket, an empty HOME, the fake key, and a loopback fake gateway. Two catalogs: the research one (`scenarios/models.json`: `fake/model-a`, `openai/gpt-6-astra` with levels `low medium high xhigh` and Fast and Ultra, `anthropic/claude-opus-5` with `low medium high max` and Fast, `xai/grok-4.7`, and an embedding row), and a new `scenarios/models-effort.json` that adds `zai/glm-5.2` with levels `low high` and **no Fast**. fx commits from its effort column only for a model with no Fast (`submitModelPicker`, `input_completion_runtime.zig:1183`), and xfx has no speed column yet, so that model is where the two binaries take the same path all the way to a turn.

```
scripts/fxcap/capture.sh {fx,xfx} model-column     40x120 <dir>   # the column, the filter, the effort column, Left, Escape, a menu row into it
scripts/fxcap/capture.sh {fx,xfx} model-effort     40x120 <dir>   # an effort chosen and sent, the explicit form, trap 1
scripts/fxcap/capture.sh {fx,xfx} model-router     40x120 <dir>   # D2a's router scenario, now with xfx's column dismissed too
scripts/fxcap/capture.sh {fx,xfx} model-busy       40x120 <dir>   # /model while a turn streams, no catalog read yet
scripts/fxcap/capture.sh {fx,xfx} model-busy-known 40x120 <dir>   # the same, after the menu has read the catalog
```

`model-column`, `model-effort` and `model-busy-known` are new in this unit. `model-router` and `model-busy` now press Escape for both binaries (xfx has the column too). One harness change: `capture.sh` points xfx at the fake gateway's `/v4/ai/language-model`, as it always pointed fx, because xfx's default chat route is now v4 (`gateway::DEFAULT_CHAT_URL`). The shims (`open`, `pbcopy`, `pbpaste`, `afplay`, `osascript`) and `FX_NO_OPEN_BROWSER=1` are unchanged.

`fx` is the c1db919 build. `xfx` is `target/debug/xfx` built from this branch after merging `plan/tui-phase4` (`b06a889`) -- merge `3c32971`, the banner's revision. The xfx set was taken first at merge `0e9fb64` (plan `f0b1caa`) and again here, after plan's F5 merge: byte-identical after the banner. Each snap has a `.txt` and an `.ansi` (`capture-pane -e`). The settings dumps (`*.files`) are plan's `home <label>` verb: every file under the run's HOME, its size, and the bytes of the settings files. "Rows identical" counts the non-blank screen rows **after the banner** that are byte-equal in the two `.ansi` files; the banner is xfx's brand line (WU-A). `wire.txt` lists every request body's keys other than `prompt` and `tools`, from the `gwlog` of `model-effort` and `model-router`; the bodies themselves are not kept. Both binaries were started once before the captured set: the first run of a freshly built xfx in an earlier set took longer than the scenario's 2.5 s to come up and lost `/model` to it -- re-run, it matched; the files here are the warmed set. The same five scenarios were captured on the branch before the merge (`dc0c137` plus this unit): every snap below read the same then, except the two `model-busy-known` snaps and `model-busy after`, which carried plan's since-fixed notice split and last-cell loss (T2).

**Sanitized**: every file was scanned for the run directory (`fxcap.*`), home and temp paths, and the fake key. None matched; the harness's own `.log` files, which name the output paths, are not copied.

## Verdicts, 40x120

| Snap | Rows identical | What differs, and why |
|---|---|---|
| `model-column space` | **8 of 8** | none: `┃ /model`, the rule, the four models from column 10 -- astra marked in the bold hint grey, the others dim -- two blank rows, the rule, `auto · model-a` |
| `model-column filter-gpt` | **5 of 5** | none: `/model gpt` leaves `openai/gpt-6-astra` |
| `model-column gpt-enter` | 8 of 9 | the effort column is identical: `/model openai/gpt-6-astra`, `default low medium high xhigh` from column 29, `default` marked. fx's footer ends `· ⚡︎`: it previews Fast, preselected for a model that has it, which is the speed column xfx does not have yet. |
| `model-column effort-down` | 8 of 9 | `low` marked and previewed, `auto · gpt-6-astra · low` -- the same `⚡︎` |
| `model-column left` | **5 of 5** | none: back to `/model openai/gpt-6-astra` with that one model listed and marked |
| `model-column esc` | **2 of 2** | none: the column gone, the text kept |
| `model-column menu-enter` | 8 of 9 | a menu row with levels goes on into the effort column (`beginExactModelSelection`): `/model anthropic/claude-opus-5`, `default low medium high max` from column 34. The same `⚡︎`. |
| `model-column menu-enter-left` | **5 of 5** | none |
| `model-effort glm-enter` | **7 of 7** | none: `default low high` under `/model zai/glm-5.2`, footer `auto · glm-5.2` |
| `model-effort glm-down` | **7 of 7** | none: `auto · glm-5.2 · low` |
| `model-effort commit` | **3 of 3** | none: `* Switched to zai/glm-5.2 (effort: low, speed: normal)`, footer `auto · glm-5.2 · low` |
| `model-effort turn` | **6 of 6** | none -- and the request carried `"reasoning":"low"` from both (`wire.txt`) |
| `model-effort explicit-ok` | **7 of 7** | none: pasted `/model zai/glm-5.2 high` is `* Switched to zai/glm-5.2 (effort: high, speed: normal)` and the composer emptied |
| `model-effort explicit-bad` | **11 of 11** | none: pasted `/model fake/model-a high` is `✗ usage: /model <id> <effort> [normal|fast|ultrafast]`, composer kept |
| `model-effort trap` | **12 of 12** | none: `/model fake/model-a`, a space, `high`, Enter typed key by key is `* Switched to fake/model-a (effort: high, speed: normal)` and then `high` sent as a prompt (trap 1); the session's effort carried across the change and is on no request to a model with no levels (`wire.txt`) |
| `model-router opus` | **3 of 3** | none |
| `model-router unknown` | **4 of 4** | none |
| `model-router usage` | **8 of 8** | none -- D2a's 5 of 8: xfx now shows the inline column under the kept text too (`no matching models`) |
| `model-router exact` | **6 of 6** | none |
| `model-busy during` | 1 of 5 | no catalog read yet in xfx: the band cannot resolve `grok`, so the change goes as typed and the runtime announces it after the turn; the column's catalog request waits behind the turn (`queued 1`). fx announces at once. See below. |
| `model-busy after` | 3 of 10 | the same line, byte-equal, after the answer instead of above it; the answer rows themselves are byte-equal |
| `model-busy-known during` | **5 of 5** | none: with the catalog read (the menu opened and closed first), `* Next turn will use xai/grok-4.7 (effort: default, speed: normal)` at once above the streaming answer, footer `auto · grok-4.7`, nothing queued -- D2a's `queued 1 · auto · model-a` |
| `model-busy-known after` | **9 of 9** | none |

## The wire

`wire.txt`. The turn after `low` was chosen for `zai/glm-5.2` carries the top-level `"reasoning":"low"` after `"toolChoice"` from both binaries, on `/v4/ai/language-model`. The turn after the change to `fake/model-a` (no levels) carries none from either, with the session's effort still `high`. fx's bodies also carry `providerOptions.gateway.caching:"auto"` and a `headers.user-agent`, and fx asks for a session title: features of other units, unchanged here. llmux is not captured (fx has no llmux); `tests/tui.rs`'s `a_level_chosen_on_llmux_rides_as_output_config_effort` pins `"output_config":{"effort":"max"}` on its wire, the field llmux reads (`2lab-ai/llmux@4b3d4a4 src/provider/codex.rs:387-420`, `src/provider/grok.rs:327-345`, `src/proxy/forward.rs:488-505`).

## Settings after each write

Both homes hold a `settings.lock` (0 bytes) beside `settings.json` after the first write (`*-end.files`): the cross-process lock every xfx preference write now takes, under fx's name.

| After | fx `~/.fx/settings.json` | xfx `~/.xfx/settings.json` |
|---|---|---|
| `model-effort commit` | `{"models":{"gateway":"zai/glm-5.2"},"provider":"gateway","effort":"low",…}` | `"effort": "low"`, `"models": {"gateway": "zai/glm-5.2"}`, `"provider": "gateway"` |
| `model-effort explicit-ok` | `…"effort":"high"…` | `"effort": "high"` |
| `model-effort` end (after the trap) | `{"models":{"gateway":"fake/model-a"},…,"effort":"high",…}` | `"effort": "high"`, `"models": {"gateway": "fake/model-a"}` |

The effort is global and carried across the change to a model without levels, in both. fx's `fast_mode`, `ultrafast_mode` and `fast_mode_model_bound` are not written: xfx has no Fast or Ultra yet. xfx writes the model and the effort in one transaction (`provider::profile::write_preferences`) under its in-process lock and `settings.lock` beside the file, upstream's lock name.

## What does not match, and why

- **`⚡︎` in the footer's preview** (`gpt-enter`, `effort-down`, `menu-enter`): fx preselects Fast in its speed column for a model that offers it and previews it. xfx has no speed column, sends no speed, and so previews none. The speed column, `/fast` and `/ultrafast` are the next unit's (D2c).
- **A query typed mid-turn before any catalog is read** (`model-busy`): fx fetches the catalog on its UI thread to resolve the word and says `Next turn will use` at once. xfx's UI thread may not wait on a socket, so a word it cannot resolve goes to the runtime as typed, and the runtime -- busy with the turn -- resolves and announces it when the turn ends. The change itself is a control message now and takes no place in the queue; the `queued 1` is the column's catalog request, which is work and waits behind the turn. Once the catalog has been read (`model-busy-known`, or any session that has opened `/model` once) the change is resolved and announced at once, as fx's is. Recorded in `ssot.md` §Tensions.
- **The banner**: xfx's brand line (WU-A).
