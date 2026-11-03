# W1 receipt: /usage (/cost) and /credits (/balance)

Nine scenarios in `scripts/fxcap/scenarios/`, each run at 40x120 on both
binaries with the repo's driver, unmodified apart from the `credits` verb this
unit added (the fake gateway serves `GET /coding-agent/v1/credits` from a
scenario file, a `billing` turn streams what the Gateway sends for a turn it
billed, and a `deferred` turn streams a generation id and no cost, whose record
the fake gateway then serves from `GET /v1/generation?id=<gen>` after turning
the first `notYet` asks away with 404):

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
| `usage-deferred` | one turn whose bill comes later (`deferred-turns.json`, `notYet: 6`): `/usage`, Shift-Tab to the session while the bill is still being asked for (`pending`), the same screen once it has been answered (`settled`), `r` (`settled-r`), Tab to 30 days (`d30`); `home end`; `gwlog`. |
| `credits-busy` | `/credits` typed while a turn streams a chunk every 250 ms (`slow-turns.json`): `during`, a second after it ran, the turn still going; `after`, the turn over. |

The xfx binary is `target/debug/xfx` built at `435c4d7` (the follow-ups --
`/credits` off the work queue, an abandonable `/usage` reader, late bills
asked for by generation id -- with `plan/tui-phase4` ce5e782 merged), which is
what its banner says; every xfx screen here was retaken at it, the seven
earlier scenarios because the `/credits` and `/usage` paths under them
changed. The fx side of those seven was taken at `fccf131`'s run and is the
same binary either way; `usage-deferred` and `credits-busy` were taken on both
sides now. The xfx binary was run once before its first capture, so macOS's
first-launch assessment (6 s on this machine under load) was not inside one. The same
shims, `FX_NO_OPEN_BROWSER=1`, the fx sound guard, an isolated tmux socket and
an empty `HOME` as every capture. No file was edited after capture; `.files`
dumps write the run's scratch directory as `<scratch>` (the driver's own
rule). `.ansi` files are tmux's re-serialization of cell attributes
(`capture-pane -e`), not the programs' wire bytes (see `../c0/README.md`).

**Result: 40 snap pairs; in 29 the only row that differs is row 1 (each
product's banner), and every row of every pair is identical in text and in SGR
except the ones listed below**, each for a reason named there (a row-by-row
diff of each pair's `.txt` and of its `.ansi`; the two differ on exactly the
same rows).

## Per snap

| snap | identical | differs, and why |
|---|---|---|
| `usage-typed`, `usage-cost-typed`, `credits-ok-typed` | The completion menu: `Commands 1`, the row `  /usage   … Account` (and `/credits   show gateway credits balance … Account` byte for byte), its SGR (`ESC[1mESC[38;5;255m  /usage   ESC[38;5;245m…`), the key help. `/cost` lists under its own typed name. | Row 1 is each product's banner (every receipt). **`/usage`'s description** reads `show local xfx tokens, models, and spend` where fx's says `fx` (`commands.zig:474`): what it shows is xfx's own spend, and one column narrower padding keeps `Account` at the same column. |
| `usage-empty` | Rows 3-7: the rule, `Usage  [30 days]  7 days  24 hours  Session` (title and shown window bold 255, others 245), `Tracking has not started` (245), the rule, the key help `tab scope     ↑↓ model     enter expand     r refresh     esc close` (245). No composer row. | Row 1 only. |
| `usage-answered`, `usage-esc` | The answer, its summary `  1s (↑6 ↓40)`, the composer back after Esc, the hint row. | Row 1. (At `fccf131` the summary read `2s` once: wall time of the fake turn, not a rule.) |
| `usage-open`, `-right1`, `-left`, `-down`, `-tab`, `-btab`, `-cost-open` | Rows 2-34: the dashboard after a turn nobody billed -- `Partial data · some usage may be missing`, `  0 tokens        $0.00 spent      0 requests`, `  0 input         0 output         Reasoning unavailable`, `  0 cache read    0 cache write`, `Models 0` (bold 252), `  Model    Tokens    Share    Spend`, `  No model usage in this scope.`, every SGR run. Right from 30 days stays put; Left lands on `[7 days]` (to its right in the header); Down with no model does nothing; Tab moves to `[24 hours]`, Shift-Tab back to `[7 days]`; `/cost` is the same dashboard. | Row 1. |
| `usage-arg` | `✗ command: Unknown command. Try /help.` for `/usage 7d`, SGR as C0's. | Row 1. |
| `usage-billed-turns` | Both answers and their summaries (`0s (↑4 ↓800)`, `0s (↑4 ↓3.1k)`). | Row 1. |
| `usage-billed-d30`, `-d30-expand`, `-d30-down`, `-d30-down-expand`, `-wrap-30` | The whole populated dashboard: `  67.9K tokens      $0.43 spent      2 requests`, `  64K input         3.9K output      900 reasoning`, `  34K cache read    0 cache write`, `Models 2`, the column header, `❯ anthropic/claude-opus-5    55.1K     81.1%    $0.42` (name bold 252, numbers 245), `  fake/model-a               12.8K     18.9%    $0.01`, Enter's `Input 52K · Output 3.1K · Cache 30K/0 · Reasoning 900 · Requests 1 · $0.42`, Down moving the mark under the open line, Enter on the second model opening `Input 12K · Output 800 · Cache 4K/0 · Reasoning 0 · Requests 1 · $0.01` -- and, after the session and back, the mark and the open line kept on `fake/model-a` by name. | Row 1. |
| `usage-billed-session`, `-session-r` | `Usage  30 days  7 days  24 hours  [Session]`, the overview, `Session activity` (bold 252), `API 0s · Wall 16s` in `-session`, `Code +0 · -0`, the models with the mark and open line kept. | Row 17: `Local xfx activity` where fx's is `Local fx activity` -- the product's own name in the sentence that says whose activity this is. Row 24: `Wall 16s` / `Wall 15s`, and `18s` / `17s` after the 1 s `r` -- each product starts its session's clock at its own launch, and the two launches are not the same instant. (At `fccf131`, before the clock started at launch, xfx's read 2.5 s short: commit `4ac1509`.) |
| `usage-session-session`, `-session-expand`, `-session-down`, `-session-r`, `-wrap-30` | A session whose only turn was not billed: `Partial data · some usage may be missing`, `  0 input         0 output         0 reasoning` (the session's reasoning starts at nought where a window's is `Reasoning unavailable`), `API 1s · Wall 10s`, `Code +0 · -0`, `No model usage in this scope.`; Enter and Down change nothing; Tab from the session wraps to `[30 days]`. | Row 1, and row 20: the turn's summary `1s (↑6 ↓40)` / `3s (↑6 ↓40)` -- the fake turn's wall time, taken at `435c4d7` on a machine at load 30 (the `fccf131` take read `1s` on both). |
| `usage-deferred-pending`, `-settled` | The session scope while the bill is asked for: `Partial data · some usage may be missing`, `  0 tokens        $0.00 spent      0 requests`, `API 0s · Wall 5s`, `No model usage in this scope.` -- and **the same screen after the bill arrived**: neither product repaints the session scope until it is asked to. | Row 1. |
| `usage-deferred-settled-r` | `r` reads the session again and the bill is in it: `  12.8K tokens     $0.01 spent      1 request`, `  12K input        800 output       0 reasoning`, `  4K cache read    0 cache write`, `❯ fake/model-a    12.8K     100.0%    $0.01`, every SGR run. | Row 1; row 11 `Local xfx activity` (the product's name, as above); row 18 `Wall 12s` / `Wall 11s` (each product's clock from its own launch). |
| `usage-deferred-d30` | Tab to 30 days shows the read taken when `/usage` opened, before the bill: `Partial data`, `0 requests` -- on both. | Row 1. |
| `credits-busy-during` | `* credits: balance=42.17`, `  used=7.83`, `  plan=pro` above `Generating (2s) (↑6 ↓21)`: answered while the turn runs, ahead of its text. | Row 1; row 9, the activity row's first cell -- `•` in xfx's, blank in fx's -- which is the dot's blink phase at the instant of the snap (both products blink it; the receipts hold both phases for both). |
| `credits-busy-after` | The notice where it was, the whole answer under it, `8s (↑6 ↓40)`. | Row 1. |
| `credits-ok-after`, `-balance` | `* credits: balance=42.17`, `  used=7.83`, `  plan=pro` -- one neutral run per row, `ESC[38;5;250m`, the continuation two columns in -- then the same again for `/balance`. | Row 1. |
| `credits-partial-after` | `* credits: balance=42.17` and nothing for `total_used`. | Row 1. |
| `credits-empty-after` | `* credits: no data returned by gateway`. | Row 1. |
| `credits-404-after`, `-balance`, `-arg` | `✗ credits: HTTP 404: not found` (`ESC[38;5;252m✗ credits:ESC[38;5;250m HTTP 404: not found`) twice, then `✗ command: Unknown command. Try /help.` for `/credits now`. | Row 1. |

## What each product called and kept

- **Endpoint.** Both: `GET /coding-agent/v1/credits` with a bearer, at the
  origin turns go to (`*-credits-*-gateway.log`: `GET /coding-agent/v1/credits
  bearer=yes`, once per `/credits` or `/balance`; none for `/credits now`).
  fx also fetched its model catalog at launch; xfx does that only for `/model`.
- **A deferred bill.** Both: `GET /v1/generation?id=gen_01JABCDEFGHJKMNPQRSTVWXYZ2`
  with a bearer, at the origin the turn went to, six times answered 404 and a
  second apart, the seventh answered with the record, and not again
  (`*-usage-deferred-40x120-gateway.log`).
- **Durable data** (`*-usage-billed-40x120-end.files`, `*-usage-session-…`).
  fx keeps a profile ledger `~/.fx/usage.jsonl` (`coverage`, `pending`,
  `generation` records) and a per-session `usage-v2.json`. xfx keeps each
  request in its turn's `usage_recorded` frame of `events.jsonl` --
  `"requests":[{"at_ms":…,"duration_ms":…,"model":"fake/model-a","input_tokens":12000,"output_tokens":800,"cache_read_tokens":4000,"cache_write_tokens":0,"reasoning_tokens":0,"cost":0.0125,"generation_id":"gen_01JABCDEFGHJKMNPQRSTVWXYZ0"}]`
  -- and `~/.xfx/usage.json` `{"tracking_started_at_ms":…}`, fx's `coverage`
  record. The rolling windows are a read of every session log the widest
  window can see.
- **Durable data for a deferred bill** (`*-usage-deferred-40x120-end.files`).
  fx writes `pending` then `generation` records to `~/.fx/usage.jsonl`. xfx
  writes the request pending in the turn's frame --
  `{"at_ms":…,"duration_ms":6,"model":"fake/model-a","input_tokens":12000,"output_tokens":800,"cache_read_tokens":0,"cache_write_tokens":0,"generation_id":"gen_…","origin":"http://127.0.0.1:<port>"}`
  -- and, once answered, a frame of the runtime's own:
  `{"usage_recorded","input_tokens":null,"output_tokens":null,"settled":[{"at_ms":…,"duration_ms":0,"model":"fake/model-a","input_tokens":12000,"output_tokens":800,"cache_read_tokens":4000,"cache_write_tokens":0,"reasoning_tokens":0,"cost":0.0125,"generation_id":"gen_…"}]}`,
  which takes the pending request's place under its id when the log is read.

## Deliberate divergences

1. `show local xfx tokens, models, and spend` and `Local xfx activity`: the
   product's own name.
2. **`/credits` is asked off the UI thread.** fx asks on its UI thread and
   blocks it (`commandShowCredits`, `app_commands.zig:2052-2082`); xfx asks at
   the same moment on a short-lived thread of its own, so the screen stays
   live while the Gateway answers. What the user sees is fx's: answered while a
   turn runs (`credits-busy-during`), never queued, never refused by a full
   queue, never dropped by a Ctrl-C -- the shell case
   `credits_with_a_turn_running_and_the_queue_full_is_asked_and_survives_a_ctrl_c`
   and the TUI e2e
   `credits_while_a_turn_runs_with_the_queue_full_is_answered_after_a_ctrl_c`.
   (Until `e7d6889` xfx asked on the runtime thread, behind the turn in
   flight; that was a divergence and is gone.)
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
- **A deferred bill rejected or left pending** (401/403, an answer that is not
  the generation's record, thirty rounds without one, an untrusted origin):
  `src/gateway/generation.rs` and `src/tui/reconcile.rs` unit tests,
  `a_later_bill_settles_its_pending_request_and_a_rejection_leaves_it_missing`
  in `src/session/store.rs`; the TUI e2e
  `a_bill_that_comes_later_is_asked_for_by_generation_and_partial_data_clears`.
- **Quitting during a long `/usage` read**: the shell case
  `quit_returns_promptly_with_a_slow_usage_read_in_flight` (a 30 s read; the
  exit returns as soon as the read looks at its cancel flag).
- **Narrow screens** (under 110 and 48 columns): `src/tui/usage_menu.rs`'s
  `a_narrow_screen_folds_the_overview_and_the_header`,
  `a_short_panel_keeps_the_mark_in_view`.
- **`Refresh failed · showing previous data` and `Usage unavailable · press r
  to retry`**: shell tests `a_failed_read_keeps_what_is_shown_and_says_it_is_old`
  and `usage_opens_the_dashboard_in_place_of_the_composer_and_writes_nothing`.
