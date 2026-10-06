# W1 receipt: /usage (/cost) and /credits (/balance)

Seven scenarios in `scripts/fxcap/scenarios/`, each run at 40x120 on both
binaries with the repo's driver, unmodified apart from the `credits` verb this
unit added (the fake gateway serves `GET /coding-agent/v1/credits` from a
scenario file, and a `billing` turn streams what the Gateway sends for a turn
it billed):

```
FX_BIN=<fx c1db919 build, 0.0.13> scripts/fxcap/capture.sh fx  <scenario> 40x120 .prd/tui-phase4/receipts/w1
                                  scripts/fxcap/capture.sh xfx <scenario> 40x120 .prd/tui-phase4/receipts/w1
```

| scenario | what it drives |
|---|---|
| `usage` | fresh home: `/usage` typed (menu row), Enter (nothing counted yet), Esc; one turn the fake gateway does **not** bill (`markdown-turn.json`, no billing metadata); `/usage`, Right, Left, Down, Tab, Shift-Tab, Esc; `/cost` typed and run; `/usage 7d`. |
| `usage-billed` | two turns the fake gateway bills (`billed-turns.json`: `response-metadata` with the time, then a finish with `usage.{inputTokens.{total,cacheRead},outputTokens.{total,reasoning}}` and `providerMetadata.gateway.{routing.canonicalSlug,cost,generationId}`); `/usage`, Enter, Down, Enter, Tab x3 to the session, `r`, Tab round to 30 days; `home end` dumps each product's profile. |
| `usage-session` | one unbilled turn, `/usage`, Shift-Tab straight to the session, Enter and Down (no model to act on), `r`, Tab round to 30 days; `home end`. |
| `credits-ok` | `{"balance":"42.17","used":"7.83","plan":"pro"}`: `/credits` typed (menu row), run, then `/balance`. |
| `credits-partial` | `{"balance":"42.17","total_used":"7.83"}`. |
| `credits-empty` | `{}`. |
| `credits-404` | no credits file: the fake gateway's `404 not found`; `/balance`; `/credits now`. |

The xfx binary is `target/debug/xfx` built at `fccf131` (this branch with
`plan/tui-phase4` 2b5967f merged), which is what its banner says. The same
shims, `FX_NO_OPEN_BROWSER=1`, the fx sound guard, an isolated tmux socket and
an empty `HOME` as every capture. No file was edited after capture; `.files`
dumps write the run's scratch directory as `<scratch>` (the driver's own
rule). `.ansi` files are tmux's re-serialization of cell attributes
(`capture-pane -e`), not the programs' wire bytes (see `../c0/README.md`).

**Result: 34 snap pairs, every row of every pair identical in text and in SGR
except the ones listed below**, each for a reason named there (a row-by-row
diff of each pair's `.txt` and of its `.ansi`; the two differ on exactly the
same rows).

## Per snap

| snap | identical | differs, and why |
|---|---|---|
| `usage-typed`, `usage-cost-typed`, `credits-ok-typed` | The completion menu: `Commands 1`, the row `  /usage   … Account` (and `/credits   show gateway credits balance … Account` byte for byte), its SGR (`ESC[1mESC[38;5;255m  /usage   ESC[38;5;245m…`), the key help. `/cost` lists under its own typed name. | Row 1 is each product's banner (every receipt). **`/usage`'s description** reads `show local xfx tokens, models, and spend` where fx's says `fx` (`commands.zig:474`): what it shows is xfx's own spend, and one column narrower padding keeps `Account` at the same column. |
| `usage-empty` | Rows 3-7: the rule, `Usage  [30 days]  7 days  24 hours  Session` (title and shown window bold 255, others 245), `Tracking has not started` (245), the rule, the key help `tab scope     ↑↓ model     enter expand     r refresh     esc close` (245). No composer row. | Row 1 only. |
| `usage-answered`, `usage-esc` | The answer, the composer back after Esc, the hint row. | Row 20, the turn summary's seconds (`1s` / `2s`): wall time of the fake turn, not a rule. |
| `usage-open`, `-right1`, `-left`, `-down`, `-tab`, `-btab`, `-cost-open` | Rows 22-34: the dashboard after a turn nobody billed -- `Partial data · some usage may be missing`, `  0 tokens        $0.00 spent      0 requests`, `  0 input         0 output         Reasoning unavailable`, `  0 cache read    0 cache write`, `Models 0` (bold 252), `  Model    Tokens    Share    Spend`, `  No model usage in this scope.`, every SGR run. Right from 30 days stays put; Left lands on `[7 days]` (to its right in the header); Down with no model does nothing; Tab moves to `[24 hours]`, Shift-Tab back to `[7 days]`; `/cost` is the same dashboard. | Row 20 as above. |
| `usage-arg` | `✗ command: Unknown command. Try /help.` for `/usage 7d`, SGR as C0's. | Row 20 as above. |
| `usage-billed-turns` | Both answers and their summaries (`0s (↑4 ↓800)`, `0s (↑4 ↓3.1k)`). | Row 1. |
| `usage-billed-d30`, `-d30-expand`, `-d30-down`, `-d30-down-expand`, `-wrap-30` | The whole populated dashboard: `  67.9K tokens      $0.43 spent      2 requests`, `  64K input         3.9K output      900 reasoning`, `  34K cache read    0 cache write`, `Models 2`, the column header, `❯ anthropic/claude-opus-5    55.1K     81.1%    $0.42` (name bold 252, numbers 245), `  fake/model-a               12.8K     18.9%    $0.01`, Enter's `Input 52K · Output 3.1K · Cache 30K/0 · Reasoning 900 · Requests 1 · $0.42`, Down moving the mark under the open line, Enter on the second model opening `Input 12K · Output 800 · Cache 4K/0 · Reasoning 0 · Requests 1 · $0.01` -- and, after the session and back, the mark and the open line kept on `fake/model-a` by name. | Row 1. |
| `usage-billed-session`, `-session-r` | `Usage  30 days  7 days  24 hours  [Session]`, the overview, `Session activity` (bold 252), `Code +0 · -0`, the models with the mark and open line kept. | Row 17: `Local xfx activity` where fx's is `Local fx activity` -- the product's own name in the sentence that says whose activity this is. Row 24: `API 0s · Wall 16s` / `Wall 15s` (and 18s / 17s after `r`): the wall clock, one second apart -- each product starts its session's clock at its own launch. |
| `usage-session-session`, `-session-expand`, `-session-down`, `-session-r`, `-wrap-30` | A session whose only turn was not billed: `Partial data · some usage may be missing`, `  0 input         0 output         0 reasoning` (the session's reasoning starts at nought where a window's is `Reasoning unavailable`), `API 1s`, `Code +0 · -0`, `No model usage in this scope.`; Enter and Down change nothing; Tab from the session wraps to `[30 days]`. | Row 20 (turn seconds), and row 31 in `-session-r`: `Wall 13s` / `12s`, as above. |
| `credits-ok-after`, `-balance` | `* credits: balance=42.17`, `  used=7.83`, `  plan=pro` -- one neutral run per row, `ESC[38;5;250m`, the continuation two columns in -- then the same again for `/balance`. | Row 1. |
| `credits-partial-after` | `* credits: balance=42.17` and nothing for `total_used`. | Row 1. |
| `credits-empty-after` | `* credits: no data returned by gateway`. | Row 1. |
| `credits-404-after`, `-balance`, `-arg` | `✗ credits: HTTP 404: not found` (`ESC[38;5;252m✗ credits:ESC[38;5;250m HTTP 404: not found`) twice, then `✗ command: Unknown command. Try /help.` for `/credits now`. | Row 1. |

## What each product called and kept

- **Endpoint.** Both: `GET /coding-agent/v1/credits` with a bearer, at the
  origin turns go to (`*-credits-*-gateway.log`: `GET /coding-agent/v1/credits
  bearer=yes`, once per `/credits` or `/balance`; none for `/credits now`).
  fx also fetched its model catalog at launch; xfx does that only for `/model`.
- **Durable data** (`*-usage-billed-40x120-end.files`, `*-usage-session-…`).
  fx keeps a profile ledger `~/.fx/usage.jsonl` (`coverage`, `pending`,
  `generation` records) and a per-session `usage-v2.json`. xfx keeps each
  request in its turn's `usage_recorded` frame of `events.jsonl` --
  `"requests":[{"at_ms":…,"duration_ms":…,"model":"fake/model-a","input_tokens":12000,"output_tokens":800,"cache_read_tokens":4000,"cache_write_tokens":0,"reasoning_tokens":0,"cost":0.0125,"generation_id":"gen_01JABCDEFGHJKMNPQRSTVWXYZ0"}]`
  -- and `~/.xfx/usage.json` `{"tracking_started_at_ms":…}`, fx's `coverage`
  record. The rolling windows are a read of every session log the widest
  window can see.

## Deliberate divergences

1. `show local xfx tokens, models, and spend` and `Local xfx activity`: the
   product's own name.
2. **`/credits` waits for a running turn.** fx asks on its UI thread and blocks
   it (`commandShowCredits`, `app_commands.zig:2052-2082`); xfx asks on the
   runtime thread, so the notice comes when the turn in flight is done, and a
   full queue refuses it on the hint row. Not captured here (no turn runs
   during these scenarios); covered by
   `credits_with_the_queue_full_is_refused_where_the_user_reads_it` and the
   TUI e2e `usage_counts_the_billed_turn_and_credits_asks_the_gateway_off_the_ui_thread`.
3. **Spend is the Gateway's `cost`**, never a figure worked out from the
   catalog's prices -- which is what fx does too (`client.zig:3352-3359`).

## Not in this receipt

- **llmux.** No capture talks to a llmux daemon. `/credits` there says fx's
  `Credits are unavailable for the selected provider.` without asking it
  (`credits_on_llmux_asks_nobody_and_says_upstreams_sentence`,
  `slash_credits_with_no_credential_or_on_llmux_asks_nobody`); a llmux request
  counts its tokens at `$0.00` under `llmux/<model>`
  (`a_llmux_answer_is_billed_as_its_tokens_and_no_spend`).
- **A request lost mid-stream** (`unaccounted`, `Partial data`) and **a turn
  recorded before request records** (`legacy`): unit tests in `src/usage.rs`
  and `tests/tool_loop.rs`.
- **Narrow screens** (under 110 and 48 columns): `src/tui/usage_menu.rs`'s
  `a_narrow_screen_folds_the_overview_and_the_header`,
  `a_short_panel_keeps_the_mark_in_view`.
- **`Refresh failed · showing previous data` and `Usage unavailable · press r
  to retry`**: shell tests `a_failed_read_keeps_what_is_shown_and_says_it_is_old`
  and `usage_opens_the_dashboard_in_place_of_the_composer_and_writes_nothing`.
