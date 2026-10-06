# D2A receipt: `/model`'s menu and router form, fx c1db919 beside xfx

Captured on 2026-10-07 with the repository's harness. Each run had its own tmux socket, an empty HOME, the fake key, and a loopback fake gateway serving the research catalog (`scenarios/models.json`: `fake/model-a`, `openai/gpt-6-astra`, `anthropic/claude-opus-5`, `xai/grok-4.7`, and an embedding row neither binary may list):

```
scripts/fxcap/capture.sh {fx,xfx} model        40x120 <dir>   # the menu
scripts/fxcap/capture.sh {fx,xfx} model-router 40x120 <dir>   # /model <query>, an unknown id, two words, an exact id
scripts/fxcap/capture.sh {fx,xfx} model-busy   40x120 <dir>   # /model while a turn streams
scripts/fxcap/capture.sh {fx,xfx} model-empty  40x120 <dir>   # a catalog with no language model
```

All four scenarios are new in this unit, and so are four harness verbs (`capture.sh`'s header): `models <file>` (the fake gateway serves it as `GET /coding-agent/v1/models` and logs `bearer=yes|no`, never the bearer), `seed` now writes the binary's **own** profile owner-only (`~/.fx` or `~/.xfx`), `nomodel` (no `FX_MODEL`/`XFX_MODEL`, so the model is read from the seeded profile and no process override shadows a save), and `home <label>` (the HOME's file list and every JSON/JSONL in it). The shims (`open`, `pbcopy`, `pbpaste`, `afplay`, `osascript`) and `FX_NO_OPEN_BROWSER=1` are unchanged.

`fx` is the c1db919 build. `xfx` is `target/debug/xfx` built from this branch's working tree just before the commit, so its banner names the parent commit (`v0.1.0-b9c5591`). Each snap has a `.txt` (text) and an `.ansi` (`capture-pane -e`). "Rows identical" counts the non-blank screen rows **after the banner** that are byte-equal in the two `.ansi` files; the banner is xfx's brand line (WU-A) and differs in every snap.

**Sanitized**: every file was scanned for the run directory, home and temp paths, and the fake key. The run directory appeared only in fx's and xfx's own session/history JSON inside the `home-*` dumps (workspace roots); each occurrence of `/private/tmp/claude-501/fxcap.<id>` was replaced by `<scratch>`. The key appears nowhere: the gateway logs say `bearer=yes`.

## Verdicts, 40x120

| Snap | Rows identical | What differs, and why |
|---|---|---|
| `model typed` | 5 of 6 | The slash popup's `/model` summary: xfx's registry text (`show the model, or `/model <id>` to use another one from now on`) against fx's `choose what model and reasoning effort to use`. The summary is the registry's (WU-B), not this unit's. |
| `model open` | **10 of 10** | none: `Models 4  [All]  Anthropic  OpenAI  xAI  Others`, the gap, astra (marked, facts at column 27), opus, model-a, grok in fx's featured order, the gap, the API-key note, both rules, `↑↓ navigate     tab provider     enter use     esc close` |
| `model down1` | **10 of 10** | none |
| `model down5` | **10 of 10** | none: four more Downs from row 2 wrap round to row 2 |
| `model up-wrap` | **10 of 10** | none: Up from row 1 wraps to the last row |
| `model tab1` | **7 of 7** | none: `Models 1  All  [Anthropic] …`, the menu one row shorter |
| `model tab2` | **7 of 7** | none: `[OpenAI]` |
| `model query` | **7 of 7** | none: `gro` on OpenAI is `Models 0` and `No models found.` above the note, the search in the composer |
| `model btab` | **7 of 7** | none: Shift-Tab to `[Anthropic]`, the search kept |
| `model esc` | **2 of 2** | none: the menu and its search gone, `auto · model-a` back |
| `model query-grok` | **7 of 7** | none: reopened over the rows already in hand, `grok` typed |
| `model enter` | **3 of 3** | none: `* Switched to xai/grok-4.7 (effort: default, speed: normal)`, footer `auto · grok-4.7` |
| `model-router opus` | **3 of 3** | none: `/model opus` resolves to `anthropic/claude-opus-5`; footer `auto · opus 5` |
| `model-router unknown` | **4 of 4** | none: `nonexistent-xyz` applied as typed, no caveat |
| `model-router usage` | 5 of 8 | `\e[38;5;252m✗ \e[38;5;250musage: /model <id> <effort> [normal|fast|ultrafast]` and the composer keeping `/model fake/model-a high` are byte-equal; fx also shows its inline model column under the composer (two rules and `no matching models`), which is the later unit's surface (see below). |
| `model-router exact` | **6 of 6** | none: `XAI/GROK-4.7` is `xai/grok-4.7` |
| `model-empty open` | **7 of 7** | none: `Models 0  [All]`, `No models available.`, the note |
| `model-empty esc` | **2 of 2** | none |
| `model-busy during` | 2 of 5 | fx says `* Next turn will use xai/grok-4.7 …` at once and its footer already reads `grok-4.7`; xfx's footer reads `queued 1 · auto · model-a` -- the change waits on the work queue behind the running turn. |
| `model-busy after` | 6 of 9 | Same line, `* Next turn will use xai/grok-4.7 (effort: default, speed: normal)`, byte-equal -- fx's above the answer, xfx's under it, when the turn ended and the change was applied. The two remaining rows differ in one character: see "Found" below. |

## Settings after each write

fx writes one line; xfx writes the profile's pretty JSON through `provider::profile::write_preference` (staged `0600` file and a rename), keeping every other key -- here the seeded `provider`.

| After | fx `~/.fx/settings.json` | xfx `~/.xfx/settings.json` |
|---|---|---|
| `model enter` (grok from the menu) | `{"models":{"gateway":"xai/grok-4.7"},"provider":"gateway","fast_mode":false,"ultrafast_mode":false,"fast_mode_model_bound":true}` | `{\n  "models": {\n    "gateway": "xai/grok-4.7"\n  },\n  "provider": "gateway"\n}\n` |
| `model-router opus` | `{"models":{"gateway":"anthropic/claude-opus-5"},…}` | `"gateway": "anthropic/claude-opus-5"` |
| `model-router unknown` | `{"models":{"gateway":"nonexistent-xyz"},…}` | `"gateway": "nonexistent-xyz"` |
| `model-router` end (exact grok) | `{"models":{"gateway":"xai/grok-4.7"},…}` | `"gateway": "xai/grok-4.7"` |
| `model-busy` end | `{"models":{"gateway":"xai/grok-4.7"},…}` | `"gateway": "xai/grok-4.7"`, and the session log's `preferences_changed` (seq 7) lands after `turn_concluded` (seq 6) |

The usage line writes nothing in either. fx's `fast_mode`, `ultrafast_mode` and `fast_mode_model_bound` are not written: xfx has no Fast or Ultra mode to record.

Gateway logs: one `GET /coding-agent/v1/models bearer=yes` per xfx run; fx asks once in `model` and twice in `model-router` (its startup warm-up and a refresh).

## What does not match, and why

- **`Next turn will use` arrives late, and the footer waits** (`model-busy`). fx applies the change on the spot while the stream runs (`announceModelSelection`, `session_commands.zig:1266-1279`). xfx's model lives on the runtime thread, which applies work in order, so the change is queued behind the running turn (`queued 1`), applied between turns -- still for the next turn, as the sentence says -- and announced then. A Ctrl-C that interrupts the turn drops the queued change with everything else queued.
- **The inline model column** under `/model ` and a space (`usage`): fx's `beginModelPickerFlow`/`submitModelPicker` (`input_completion_runtime.zig:1161-1258`), and the effort and speed columns a menu Enter continues into for a model that has them (`menu-enter` in the research captures). That is the later unit's; here every row is applied at once, as fx applies a model with neither (`beginModelPickerOptions`, `:1234-1239`). The scenarios pick grok, which has neither, so both binaries take the same path.
- **The `/model` summary** in the slash popup (`typed`): the registry's (WU-B).
- **A Gateway with no credential** has no catalog in xfx (`No models available.`); fx asks the public catalog and says `Using the public model catalog; …` (`model_menu_presentation.zig:403-417`). Not captured: the harness always has the fake key.
- **A failed load** says `Unable to load models.` in xfx whatever the failure; fx says `Could not reach AI Gateway; retry /model.` for a retryable one (`retryableFailureText`, `:435-443`). xfx loads a provider's catalog once per process and does not retry, so "retry" would be a promise it does not keep. Not captured (the fake gateway always answers).
- **The banner**: xfx's brand line (WU-A).

## Re-captured after merging `plan/tui-phase4` (2c09164)

Every xfx scenario was captured again from the merged tree (`5559524`) and compared with the files here: all 20 xfx snaps are byte-identical after the banner, except `model-busy`'s activity row (`• Generating` against ` Generating`, the marker's blink phase) and its elapsed seconds (`6s` against `7s`). The files kept are the pre-merge ones.

## Found, not fixed

`model-busy after`: xfx's answer rows end one cell short -- `… word31 word3` where fx has `… word31 word32` at column 120, and the same at `word4`/`word49`. Reproduced with `plan/tui-phase4`'s own binary over `slow-line-turn.json` alone, so it is not this unit's: a full-width answer row loses its last cell. Reported to the dispatcher.

The banner of the merged-tree build still read `v0.1.0-b9c5591`: `build.rs`'s `watch_git_head` watches the branch's loose ref only when it exists, and a worktree branch created on a packed ref has none until its first commit -- so the build script that ran then watched HEAD alone, which a commit on the branch never changes. Also reported, not fixed.

Verdict: **the menu, its keys, its states, the router form and its persistence match fx byte for byte in every snap where both binaries take the same path** (16 snaps fully identical after the banner). The differences are the queued mid-turn change, the later unit's inline column, the registry summary and the brand line.
