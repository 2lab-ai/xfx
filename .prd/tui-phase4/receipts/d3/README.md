# D3 receipt: the `/provider` picker, fx c1db919 beside xfx

Captured on 2026-10-07 with the repository's harness, one run per binary, each on its own tmux socket with an empty HOME, the fake key and a loopback fake gateway:

```
scripts/fxcap/capture.sh {fx,xfx} provider 40x120 <dir>   # scenario added in this unit
```

`fx` is the c1db919 build (`v0.0.13`). `xfx` is `target/debug/xfx` built from commit `041f94c` (its banner says `v0.1.0-041f94c`). Each snap has a `.txt` (text) and an `.ansi` (`capture-pane -e`) file. "Band rows" are the rows from the last composer row (`┃ …`) down to the hint row, compared byte for byte in the `.ansi` files at the same offset from that composer row.

**Scenario setup** (`scripts/fxcap/scenarios/provider.cap`): both profiles are seeded with `{"models":{"gateway":"fake/model-a"},"provider":"gateway"}` (`provider-settings.json`, written to `~/.fx/` or `~/.xfx/` by the harness's `seed`), and `nomodel` launches without `FX_MODEL`/`XFX_MODEL`, so the profile's model is the one in force and a settings write is not shadowed by a process override. Without `nomodel`, xfx's commit would add `! provider: XFX_MODEL outranks the profile, …` before the switch notice -- a true xfx caveat, not a mismatch. The harness gained three things for this: a per-binary `seed`, `nomodel`, and `home <label>` (the HOME file list, then each JSON/JSONL file).

**Sanitized**: every file was scanned for the run directory (`fxcap.*`), `/private/tmp`, `/Users/` and the fake key. Only `fx-provider-40x120-home-after-commit.txt` matched (fx's history and session record the workspace path); its run-directory prefix is replaced with `<scratch>`. Nothing else was edited.

**What is compared.** fx's providers are `vercel`, `codex`, `grok`; xfx's are `gateway`, `llmux` (`ProviderId::ALL`). fx's sign-in columns after the provider row (method, key source, team, key field) and its Codex/Grok sign-in leaves are gated for xfx (zbrain open-loop #88) and are absent, so xfx's provider row is a whole choice and commits. The comparison is about the column: anchor, colours, height, keys, the empty row, the refusals and what a commit writes.

## Verdicts, 40x120

| Snap | Band rows identical | What differs, and why |
|---|---|---|
| `typed` | **12 of 12** | none: `/provider` typed shows the completion menu, `Commands 1`, the row `/provider   choose the model provider and how it signs in   Account` (upstream's registry row, now xfx's too). |
| `open` | 7 of 10; **10 of 10 after the label map** | `/provider` + Enter: composer reseeded `┃ /provider `, the column at column 12, marked row `\e[1m\e[38;5;255mgateway\e[0m\e[38;5;245m · current\e[39m`, unmarked `\e[38;5;245mllmux\e[39m`, blank rows to a six-row body, hint row `auto · model-a`. The three differing rows are the labels (`vercel/codex/grok` against `gateway/llmux/blank`); with `vercel→gateway`, `codex→llmux` and the `grok` row blanked every byte is equal. |
| `down1` | 7 of 10; **10 of 10 after the label map** | Down marks the second row; the unmarked current row is all dim (`gateway · current` in 245). |
| `down2-wrap` | 7 of 10; 8 of 10 after the label map | Two Downs: xfx has two rows, so it has wrapped back to `gateway`; fx has three and is on `grok` (fx wraps on its third Down, research capture `fx-provider-40x120-down3-wrap`). Same rule, different count. |
| `esc` | **3 of 3** | none: Escape closes the column and keeps `┃ /provider`; the hint row is back. |
| `setup-typed` | **12 of 12** | none: `/setup` typed shows the completion menu under the alias's own name. |
| `setup-open` | 7 of 10; **10 of 10 after the label map** | `/setup` + Enter reseeds the canonical `/provider ` (column 12), as fx does. |
| `setup-space` | 7 of 10; **10 of 10 after the label map** | `/setup ` typed with its space keeps that spelling and opens the column at column 9. |
| `arg-filter` | 8 of 10; **10 of 10 after the label map** | The query is the provider's own letters (`/provider co` in fx, `/provider ll` in xfx): one marked row, `\e[1m\e[38;5;255mllmux\e[0m`. |
| `arg-tab` | 8 of 10; **10 of 10 after the label map** | Tab writes `/provider codex` / `/provider llmux`, nothing committed, the column keeps the one marked row. |
| `arg-nomatch` | **10 of 10** | none: `/provider zz` shows `␠×12\e[38;5;245mno matching providers\e[39m`. |
| `arg-nomatch-enter` | **3 of 3** | none in the band. The document row above it is byte-equal too: `\e[38;5;252m✗ command:\e[38;5;250m Unknown command. Try /help.\e[39m`. |
| `commit` | **3 of 3** | Band: none. Document: fx `* auth: Switched credential to AI_GATEWAY_API_KEY.`; xfx `* provider: Preparing Vercel AI Gateway.` then `* provider: Switched to Vercel AI Gateway with fake/model-a. The next prompt starts a fresh conversation.` (all `\e[38;5;250m`). Different leaves: fx's commit walked its gated sign-in columns to the env key (Enter, Down, Enter, Enter) and switched a **credential** on the provider already active; xfx's commit is the provider row and runs `TurnWork::Setup`, which is a provider switch -- so it says it in fx's provider-switch words (`Preparing {route}.`, `app_auth_runtime.zig:1239`; `Switched to {route} with {model}.`, `:1385`, topic `provider`, `:1410-1414`) plus xfx's fact that the conversation was dropped. |
| `busy` | **3 of 3** | Band: none. Document: the refusal row is byte-equal, `\e[38;5;250m* provider: Provider switching is unavailable until active and queued work finishes.\e[39m`, and no column opened. Above it xfx shows the answer's first complete row (`MARKER-HEAD … word13`) where fx shows none yet; the activity row's bullet is in the other blink phase. Neither is this unit's (streaming presentation, F5). |
| `busy-after` | **3 of 3** | Band: none. Document: see "What does not match" -- xfx's answer is split around the refusal. |

Composer rows: fx's `┃ /provider ` and xfx's are the same cells (the trailing space is not visible in a capture).

## Settings after each write

fx (`~/.fx/settings.json`, one line, plus a backup under `~/.fx/backups/`):

```
{"models":{"gateway":"fake/model-a"},"provider":"gateway","credential_source":"ai_gateway_api_key"}
```

xfx (`~/.xfx/settings.json`, 128 bytes, `0600`, written by the runtime thread's setup transaction, the same `profile::document_for` bytes `xfx setup gateway` writes):

```
{
  "backend": "gateway",
  "model": "fake/model-a",
  "models": {
    "gateway": "fake/model-a"
  },
  "provider": "gateway"
}
```

The difference is each product's own profile format: xfx keeps the legacy `backend`/`model` pair in step for an older binary (`profile.rs` module doc), and has no `credential_source` (the Gateway credential is the environment's). fx's `history.jsonl` records `/provider`, `/setup`, `/provider zz` and `/provider` -- the submitted lines -- and not the commit; xfx's picker commit records no history entry either.

## What does not match, and why

- **Provider rows and their count** (`vercel/codex/grok` against `gateway/llmux`): xfx's providers. Every other byte of the column is fx's.
- **The commit leaf.** fx's provider row on `vercel` opens its sign-in method column (`oauth`, `api-key`), then a key-source column; those are fx sign-in and gated (#88). xfx commits on the provider row. Right at the end of the text is Enter in both; xfx keeps that only because its commit opens no browser and starts no sign-in.
- **The answer split around a mid-stream notice** (`busy-after`). xfx writes the refusal at once (`Shell::write_notice`, the way C0 writes every keystroke refusal), and `write_notice` closes the answer that is still streaming: the rows after the notice are a second paragraph, and its rows lose their 120th cell (`word30` shows as `word3`). The same split and the same lost cell reproduce on `b9c5591` (before this unit) with `/notacommand` typed mid-stream, so it is the notice path's and the presenter's, not this picker's; reported for the owner of those.
- **The banner** (`x𝒇x v0.1.0-041f94c [debug]`) is xfx's brand line.

Verdict: **every band row matches fx byte for byte once the provider labels are mapped**, except `down2-wrap`, where the two lists wrap at their own lengths. The refusals (`no matching providers`, `Unknown command`, the busy sentence) are byte-equal. The commit differs by design (provider switch against fx's credential switch), in fx's own words for a provider switch.
