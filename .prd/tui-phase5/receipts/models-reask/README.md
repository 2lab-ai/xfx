# models-reask: xfx reads the model catalog again as fx v0.0.16's model cache does

Train 4 recorded that in `h1-models2` fx asked the Gateway's catalog at
832 ms and again at the second `/model` (12913 ms), and xfx only once
(`../train-4/README.md:498-501`, `../fxcap-h1/README.md:84`); train 5, that
after a catalog outage fx says `Could not reach AI Gateway; fx retries
automatically.` where xfx said `…; retry /model.` and did not ask again by
itself (`../train-5/README.md:754-755`). This unit copies fx v0.0.16's whole
rule for when the catalog is read, and its words.

Base `main` `690aa505`. Every claim below is tied to a file in this folder.

| commit | what |
|---|---|
| `d6cd1a1b` | RED: fourteen cases, alone (`red.txt`) |
| `17ae48eb` | the fix: the rule, the protocol, the triggers, the menu, the turn's wait |
| `e64a65da` | a read that could not start says fx's third sentence |
| `0a14906f` | the column says the failure while the retry behind it reads -- **the code head** |
| `c6900765` | `docs/parity.md`, `parity-ledger.toml`, `scenarios/model-column-hold.cap` |
| `765bbc4d` | a test only: a prompt's ask over held rows (for mutant R, below) |
| (this one) | this receipt |

The xfx captures here are from the binary built at `0a14906f` (row 1:
`x𝒇x v0.1.0-0a14906 [debug]`, sha256 `23719b94f33f…`); the base's from
`690aa505` (`v0.1.0-690aa50`, `e44245759efc…`); fx is the v0.0.16 release
binary, sha256 `8e6cda2e…`, checked by the sweep before and after each run.

## What fx v0.0.16 does (every line quoted in `fx-cites.txt`)

fx keeps one model cache, and one function decides every read of it:
`beginLoad` (`src/core/app/model_cache_runtime.zig:570-648@481104fd`). What
asks is a `LoadTrigger`, and "every trigger is user activity, so an idle
session never polls the models endpoint" (`:27-35`).

1. **At launch**, and after a credential or provider change: an activity
   load (`startModelCacheWarmup`, `src/main.zig:2007-2009`; from
   `app_entry_runtime.zig:340` and `app_auth_runtime.zig:2015-2024`, which
   resets the cache first, clearing the backoff, `model_cache_runtime.zig:
   665-670`).
2. **Each time `/model` opens**: `openModelMenu` asks first and opens the
   menu on what the cache holds then (`app_input_runtime.zig:1897-1902`,
   `revalidateModelCache`, `src/main.zig:2011-2015`). Over a catalog with
   rows it reads again once the last read began five seconds ago
   (`picker_revalidate_floor_ms`, `model_cache_runtime.zig:37-38`; `:614`).
3. **On other activity**: a submitted prompt or steer
   (`noteModelCatalogActivity`, `src/main.zig:1225-1232`, from
   `enqueuePromptWithSkillBindings`, `:1193-1201`, whose admission takes a
   steer too, `:1214-1217`, and which every submission reaches,
   `input_submit_runtime.zig:1103-1121`), and every frame that
   draws `/model`'s inline column (`app_render_runtime.zig:559-568` ->
   `modelCompletions` -> `ensureModelCache`, `src/main.zig:2167-2175`,
   `:2043-2045`).
4. **Only when stale or failed**: over rows, activity reads again only once
   they are the provider's refresh interval old -- the Gateway's is five
   minutes (`builtins/gateway.zig:2348`) -- or once a refresh of them failed
   in a way asking again could change (`retry_due`, `model_cache_runtime.zig:
   612-616`, `:623`). A first load and a failed catalog are read on any
   trigger (`:622`), retryable failure or not.
5. **The backoff**: after the n-th failure in a row no trigger reads for
   5 s x 2^(n-1), five minutes at most (`failureBackoffMs`, `:40-54`; fx's own
   schedule, `:1531-1535`), counted from the failure's arrival
   (`completeFetch`, `:948-951`); a success clears it (`:942-945`).
6. **While a retry reads, and after**: only a first load shows as
   `.loading`; "a revalidation or retry leaves the state readers see alone"
   (`:485-486`, `:633-634`). The menu says `Loading models…` for a retry it
   opened over (`fetch_in_flight`, `:1046-1050`), the inline column goes on
   saying `unable to load models` (it reads the state:
   `app_render_runtime.zig:750-753`, `src/main.zig:2047-2054`,
   `picker_presentation.zig:914-921`). A failed refresh over rows keeps them
   (`applyFetchLocked`, `:969-975`) and the menu says the failure where its
   note goes (`loadedCatalogStatusText`, `model_menu_presentation.zig:
   403-404`); a revalidation that lands under an open menu keeps its tab and
   its selected model (`hydrateMenuSnapshot`, `:1121-1146`).
7. **The sentence**: `Could not reach AI Gateway; fx retries automatically.`
   for transport and 500/502/503/504, `AI Gateway rate limited model
   discovery; fx retries automatically.` for 429, `Could not refresh model
   catalog; fx retries automatically.` for any other retryable failure
   (`retryableFailureText`, `model_menu_presentation.zig:435-443`), which
   includes a fetch thread it could not spawn (`.runtime`, retryable,
   `model_cache_runtime.zig:537-546`).
8. **A turn waits for the retry, briefly**: a request whose capabilities
   need the catalog (`requiresResolvedRequestCapabilities`,
   `orchestrator.zig:5658-5670`, from `:5884-5893`) waits up to 2 s for a
   failed catalog's retry on its way, such as the one its prompt started
   (`resolveForRequest`, `model_cache_runtime.zig:843-866`;
   `failed_catalog_retry_wait_ms`, `:45-47`).

At `c1db9191` (0.0.13, the code xfx was ported from) a failed catalog was
read again whenever asked a second after the read that failed began, rows
once stale (or a second after a retryable refresh failure), a prompt asked
nothing, and every load set the cache
`.loading` (`beginLoad`, `model_cache_runtime.zig:434-458@c1db9191`); the
sentence said `retry /model.` (`model_menu_presentation.zig:439-441
@c1db9191`).

## What xfx does now (the code head `0a14906f`)

- **One rule, in the selector** (`src/provider/model.rs`): `LoadTrigger`
  (`Activity`, `PickerOpen`), `LoadClock::due` -- fx's `beginLoad`, line for
  line: no read inside a failure's hold; a first load and a failure past it
  on any trigger; rows on the menu five seconds after the last read began,
  on anything once five minutes old or once a refresh of them failed
  retryably -- and `LoadClock::after`: the streak, and a hold of
  `failure_backoff(streak)` (5 s doubling to five minutes) counted from the
  answer's arrival, which the read's own thread stamps (`LoadView::
  answered_at`), so a selector held by a turn does not slide the hold.
  `Applied` is `applyFetchLocked`: a failure over rows keeps them as a
  refresh that failed (`refresh_retry`), an empty answer over rows keeps
  them, anything else is the catalog's state. A retry reads behind the
  failure (`begin_load_on`: the state is untouched until the answer is
  taken).
- **The runtime** (`src/tui/worker.rs`): the launch's read stays the
  loop's only automatic one (`begin_catalog_read`, first load only); every
  other read is a trigger the band sends (`TurnControl::Catalog(CatalogAsk)`)
  and is started only when it is due (`ask_catalog`; mid-work `CatalogAsks`,
  decided against what the selector held when the work began and every
  answer since, `Seen`); the band is told the catalog as held only when it
  waits on an answer (`CatalogAsk::awaited`). A read's telling computes the
  answer as the selector will take it, so the band hears `retry_at` (the
  hold's end) or `CatalogRefreshFailed` (rows kept). A turn whose request
  wants the catalog waits up to 2 s for the retry behind a failure
  (`await_failed_catalog_retry`, before the turn's first catalog read).
- **The band** (`src/tui/shell.rs`, `src/tui/model_menu.rs`): `/model` over
  rows lists them and asks again behind them (`REVALIDATE`); over a failure
  inside its hold it opens on the failure and asks nothing; past it, the
  menu says `Loading models…` and asks (`MENU`). A submitted prompt or steer,
  and a frame of the column, ask when activity is due
  (`note_catalog_activity`: the failure's hold over, or rows five minutes
  old, or a failed refresh's hold over), once per answer. The column keeps
  saying `unable to load models` while a retry reads. A refresh that failed
  says the sentence in the note's place; new rows keep the menu's tab and
  mark. The sentences are `Could not reach AI Gateway; xfx retries
  automatically.`, `AI Gateway rate limited model discovery; xfx retries
  automatically.`, and, for a read that could get no thread or runtime,
  `Could not refresh model catalog; xfx retries automatically.`
  (`CatalogRetry::Runtime`).

### Decisions, each reversible

- **llmux is held to the Gateway's interval** (five minutes): fx has no
  llmux provider, so there is no line to copy; one rule for both providers.
- **The turn waits once**, before its first catalog read, where fx resolves
  a request's capabilities before each request (`orchestrator.zig:5885`): a
  retry still on its way after the first 2 s is not waited for again by the
  turn's later steps. Written in `docs/parity.md`'s `/model` row.
- **A prompt's ask is sent at submission, on the control channel**, ahead of
  the work: fx triggers at enqueue, behind a running turn too, and xfx's
  work queue is not read during a turn.
- **The timeout's sentence is not here**: a catalog read past a deadline is
  the input cluster's U10 (catalog fetch bounds); xfx's current bounds give
  `Could not reach AI Gateway` when the send times out.

## RED, then GREEN (`red.txt`, `green.txt`)

Fourteen cases, committed alone in `d6cd1a1b`, every one failing there
(`red.txt`, built at `d6cd1a1b`) and passing at the code head (`green.txt`,
built at `0a14906f`). They ask
through small seams in their own test modules (`would_read`, `start_read`,
`ask_with`, `unread_at`, `catalog_asks`, ...), so the fix changed the seams
and not the cases -- with one exception, said below.

| case | fx behaviour | on the base |
|---|---|---|
| `provider::model` `a_failure_holds_every_read_off_for_upstreams_backoff_which_doubles_to_five_minutes` | 5. the backoff, 5 s to the cap | `failure 1 let Activity read 4 s on` |
| `a_held_catalog_is_read_again_by_the_menu_five_seconds_on_and_by_activity_five_minutes_on` | 2., 4. | `would_read(.., PickerOpen, at(5_000))` false |
| `a_refresh_that_fails_keeps_the_rows_says_so_and_is_tried_again_after_its_backoff` | 6. | no refresh read starts over rows |
| `a_retry_reads_behind_the_failure_it_retries` | 6. | `the retry hid the failure: NotLoaded` |
| `the_retryable_failure_sentences_say_xfx_retries_automatically` | 7. | `retry /model.` |
| `tui::model_menu` `a_revalidated_catalog_keeps_the_tab_and_the_marked_model` | 6. | `the mark jumped`: `xai/new` for `openai/b` |
| `tui::worker` `a_prompt_inside_a_failed_catalogs_backoff_reads_nothing_and_one_after_it_reads_again` | 3., 5. | `a prompt inside the backoff read the catalog`: 2 reads for 1 |
| `the_menu_over_a_held_catalog_reads_it_again_five_seconds_on_and_tells_the_new_rows` | 2. | `the menu did not read a held catalog again`: 1 read for 2 |
| `a_turn_waits_two_seconds_at_most_for_the_retry_behind_a_failed_catalog` | 8. | `the turn waited out a retry that never landed` (8 s) |
| `tui::shell` `the_menu_over_a_held_catalog_lists_its_rows_and_asks_the_runtime_to_read_it_again` | 2. | no ask |
| `a_model_inside_a_failures_backoff_opens_on_the_failure_says_xfx_retries_and_asks_nothing` | 5., 6., 7. | `Loading models…` -- read again -- two seconds after the failure |
| `a_refresh_that_failed_keeps_the_rows_and_says_xfx_retries_automatically_in_the_notes_place` | 6. | the rows gone: `Models 0`, `…; retry /model.` |
| `a_prompt_past_a_failed_catalogs_backoff_asks_for_it_and_one_inside_asks_nothing` | 3. | no ask |
| `the_columns_frames_wait_out_a_failed_catalogs_backoff` | 3., 5. | an ask two seconds after the failure |

**The exception** (`0a14906f`): the column case's last two assertions
expected `loading models...` while the column's retry reads -- what
`c1db9191` showed, which set its cache `.loading` for every load. fx v0.0.16
sets `.loading` for a first load only, and its column reads that state (6.
above), so they now expect `unable to load models` and an ask the band does
not wait on. The case's RED claim -- no ask inside the hold -- is unchanged.

Beside them, the cases the change moved were brought to the new rule in the
fix commit: the protocol (`CatalogAsk`, `retry_at`, `CatalogRefreshFailed`),
the backoff in place of the one-second floor (the old floor case is now
`the_clock_decides_a_read_as_upstreams_begin_load_does`, fx's own values),
and a `REVALIDATE` ask the menu over rows now sends (`controlled_past_reask`
in the shell's fixture). `a_read_that_could_not_start_says_upstreams_third_sentence`
pins `CatalogRetry::Runtime` (`e64a65da`), and
`a_prompt_over_a_held_catalog_reads_it_again_only_once_it_is_five_minutes_old`
the trigger the runtime passes through (`765bbc4d`, after the mutants).

## No socket off the machine (`connect-check.txt`)

Every new case that reads a catalog pins an in-process stand-in
(`set_fetcher_for_test`, as FLAKE-CATALOG-EVENT's `titled_runtime` does; the
selector's cases a `Script` or `Waiting` of their own), and so does every
worker case the change touched. Run one by one with FLAKE-CATALOG-EVENT's
`connect_logger.py` (on `fix/p5-flake-catalog-event`) as `HTTPS_PROXY` and
`https_proxy`: 20 cases (of 21 name filters; the 21st,
`catalog_told_while_parked`, is a helper and matched no case), then
`765bbc4d`'s case, then -- after the gate -- the two cases that helper
serves (`a_catalog_asked_for_while_a_question_has_parked_...`, `..._an_approval_...`):
the logger's log never written. The control, the
same logger under base's `a_title_that_comes_back_between_turns_is_written_and_then_shown`
(the case FLAKE-CATALOG-EVENT fixes, not on this base): `CONNECT
ai-gateway.vercel.sh:443`.

## Captures (`fx/`, `xfx/`, `base-xfx/`; `asks.txt`, `window.txt`)

The four scenarios the brief names and `model-column-hold`, at 40x120, from
the head sweep (`sweep-head.md`) -- `model-reask`'s from its fifth rerun at
the code head, below: fx and the code head's xfx, every snap's screen and
the gateway log; the base's xfx beside them (`base-xfx/`, from
`sweep-base.md`). Normalized as `scripts/fxcap/ledger.py` reads them (the
`product-name` rule reads fx's `fx` as `xfx`).

- **`h1-models2`**: `before` `Models 1` and `after` `Models 2` --
  `fake/model-released-later` listed -- on both. fx asked at 1810 ms and
  13870 ms (the second catalog), xfx at 1199 ms and 13230 ms (the second
  catalog); the base asked once, at 5124 ms, and its `after` said
  `Models 1` (`asks.txt`). The second `/model` came 12 s after the launch's
  read began, past the menu's five seconds; the first, under five seconds
  after it, and neither binary asked then.
- **`p5-input-model-fail`**: `fail-open` `Could not reach AI Gateway; xfx
  retries automatically.` on both (the base: `…; retry /model.`);
  `after-outage` `Models 8` on both. Normalized-identical, both snaps.
- **`model-reask`** (these captures: the code head's fifth run of it,
  `window.txt` `rerun5`): all four snaps normalized-identical.
  `menu-failed` is `Models 0`/`Could not reach AI Gateway; xfx retries
  automatically.` on both; neither asked again at the 2.5 s `/model` or at
  the prompt, both inside the 5 s hold, and both asked at the second
  `/model` (fx 2361 ms then 10475 ms, xfx 4360 ms then 12462 ms), whose
  menu took the rows mid-turn. The base asked again at its `/model` a second
  on (3907 ms, then 6434 ms, past the outage) and listed four models where
  fx said the failure (`base-xfx/`). `menu-failed` follows the window
  (below).
- **`model-parked`**: four snaps normalized-identical, `question-up` up from
  the base (whose turn went at once and was already `Generating`).
  `menu-loading`: both `Thinking (1s)`, `Models 0`, `Loading models…` -- the
  prompt started the retry (xfx 8566 ms, fx 8682 ms) and both turns waited
  2 s for it (turn requests at 10650 ms and 10683 ms) -- and the row differs
  only in xfx's token suffix: fx publishes a turn's first token progress
  when its first request is prepared (`orchestrator.zig:7938-7941@481104fd`),
  after the wait; xfx's band shows the prompt's estimate from `TurnStarted`.
  Not this unit's: `[[cause]] activity-tokens-before-request`.
- **`model-column-hold`** (mine): all four snaps normalized-identical. Both
  read three times: at launch (fx 1612 ms, xfx 1917 ms), at the column's
  first frame past 5 s (9721 ms, 10031 ms), and at the first past the second
  failure's 10 s hold (24804 ms, 25107 ms, the good catalog); the frame at
  11.5 s read nothing on either, and `col-retry` and `col-held` say `unable
  to load models` on both. The base never reached its first snap: the screen
  stayed `loading models...` for the 30 s the capture waited
  (`base-xfx/hold-base-xfx-capture.log`) -- its floor is a second from a
  read's start (`may_read_again`, `src/provider/model.rs:1218@690aa505`),
  each answer here came 1.5 s after its ask, so the frame that drew a
  failure asked again (`settle_column`, `src/tui/shell.rs:14289-14307
  @690aa505`) [inferred from the code; the run stopped before its gateway
  log was copied].

## The sweep, base against head (`sweep-compare.txt`)

Every `model*`, `h1-models*` and `p5-input-model*` scenario, 84 pairs (80
and `model-column-hold`'s 4, run apart for each binary): **base 65, head
72**. Up 9: `h1-models2` `after`, `model-floor` `inside-floor` and
`after-floor` (the sentence, under an outage that never ends), `model-parked`
`question-up`, `p5-input-model-fail` `fail-open`, `model-column-hold` ×4.
Ten fail on both: `p5-input-model` ×6 (its search queries; fuzzy model
search is the input cluster's U8) and `model-anchor` `alt-enter`, with the
same rows on the base, not this unit's; `model` `typed` (the help entry,
`C7-help-entries`); `model-parked` `menu-loading` (above); `model-reask`
`menu-failed` (below).

**Two pairs moved down in this sweep, and three moved down in the one
before it, all three `modelsdown` scenarios' -- and each of them passes on
the head in another run.** `modelsdown <ms>` counts from the fake gateway's
start, and each binary launches a different time after it (capture.sh runs
its shims first). Under fx v0.0.16's rule, which both binaries now follow, a
read after a failure comes 5 s after it, so whether a snap shows rows or the
failure turns on which side of the outage's end each binary's read lands.
`window.txt` lists every run: each side's catalog asks (ms after the
gateway started, the answer) beside each pair's verdict --

```
head-17ae48e   model-column-idle   down<9000ms | fx 2989:503, 9012:200        | xfx 781:503, 6805:503        | col-down FAIL, col-idle FAIL, col-typed FAIL
head-17ae48e   model-column-reask  down<9000ms | fx 930:503, 6478:503         | xfx 2465:503, 8020:503       | col-back pass, col-down pass
head           model-column-idle   down<9000ms | fx 6707:503, 12726:200       | xfx 13591:200                | col-down pass, col-idle pass, col-typed pass
head           model-column-reask  down<9000ms | fx 7223:503, 12766:200       | xfx 2954:503, 8502:503       | col-back FAIL, col-down FAIL
rerun1         model-column-idle   down<9000ms | fx 3061:503, 9079:200        | xfx 1181:503, 7168:503       | col-down FAIL, col-idle FAIL, col-typed FAIL
rerun1         model-column-reask  down<9000ms | fx 1884:503, 7430:503        | xfx 5722:503, 11321:200      | col-back FAIL, col-down FAIL
rerun2         model-column-idle   down<9000ms | fx 13308:200                 | xfx 12150:200                | col-down pass, col-idle pass, col-typed pass
rerun2         model-column-reask  down<9000ms | fx 8479:503, 14012:200       | xfx 6619:503, 12132:200      | col-back pass, col-down pass
```

-- a pair passes exactly when both sides' deciding reads landed on the same
side (both 503, or both 200), and fails when they straddle the end, in
either direction (xfx earlier in `head`, fx earlier in `rerun1`). On the
base both passed in its one run for the same reason: both sides' deciding
reads came after the outage's end (fx 10356 and 9967 ms, xfx 9659 and
9365 ms). `model-reask` `menu-failed`
passed where both landed inside the outage (`17ae48eb`'s sweep, and the code
head's `rerun5`, the captures above) and failed in the code head's five
other runs, each a straddle: one binary's first ask before 6 s, the other's
after it (fx 8874 ms / xfx 3306 ms, 4639 / 7039, 3828 / 7276, 3840 / 12435,
7847 / 3778). The three are
`[[cause]] catalog-window` (FLAKE-MODELSDOWN, registry #19); I did not edit
`modelsdown`. `model-column-hold` is the same column rule with no gateway
clock in it, and `model-floor` (an outage that never ends) the same menu
rule: both pass on the head and not on the base.
## Ledger and parity

- `docs/parity.md`, the `/model` row: the five claims the `c1db9191` rule
  made (`not within a second of the start of the read that failed`, `only
  /model ... asks again after a failure`, the column's `a second ago`, the
  launch-only read, both `retry /model.` sentences) are replaced by fx
  v0.0.16's rule as written above, with its cites at `481104fd`: the menu's
  five-second revalidation behind the rows and the kept tab and mark; the
  failed refresh's sentence in the note's place; the triggers; the backoff;
  the 2 s wait of a turn; the column's wording during a retry; the three
  sentences in xfx's name. The parts rewritten cite fx at `481104fd`.
- `scripts/fxcap/parity-ledger.toml`:
  - `[[cause]] catalog-window` (nondeterminism) now covers the three
    `modelsdown` scenarios whose pairs follow the launch's skew against the
    gateway's clock (`model-reask` `menu-failed`, `model-column-idle` and
    `model-column-reask` `col-*`), owned by FLAKE-MODELSDOWN (registry #19),
    with the times on record here.
  - `[[cause]] activity-tokens-before-request` (unit, not yet named):
    `model-parked` `menu-loading`, fx's activity row with no token suffix
    while its turn waits for the catalog's retry, xfx's with the prompt's
    estimate (below).
  - Four `src/tui/shell.rs` line refs this unit moved, re-pointed
    (`:185` -> `:187`, `:1644` -> `:1662`, `:12600` -> `:12656`,
    `:37758` -> `:37963`); and in `docs/parity.md` `worker.rs:3835`,
    `:3524` -> `:3840`, `:3528`.
- A scenario of my own, `scripts/fxcap/scenarios/model-column-hold.cap`
  (with `model-column-hold-bad.json`): the column's hold against each
  binary's own clock, no `modelsdown` in it.

## Mutants (`mutants.txt`, `mutants.py`)

On a `git clone --no-local` of the code head, each mutant one change of a
rule this unit added, `cargo test --locked --lib` filtered to the catalog's
cases (449 of them), a 900 s timeout each, the clone restored after each:
**22 mutants, 21 KILLED, one equivalent.** The first run (`0a14906f`) killed
20 of 21; the survivor, R, changed the trigger only in `ask_catalog`'s
precheck, which `begin_load_on` repeats with the ask's own trigger (it calls
`would_load(trigger, now)` itself) -- so R changes nothing a read does; the
precheck only spares cloning the catalog for a telling no read needs. Its
survival did show that nothing pinned the runtime passing the band's trigger
through, so `765bbc4d` adds the case that does, and R2 -- the runtime
reading every ask as the menu's, where the read is started -- is KILLED by
it (the second run, at `765bbc4d`).

| mutant | the rule it removes | | failing cases | the first |
|---|---|---|---|---|
| A-no-backoff | no read inside a failure's hold | KILLED | 6 | `a_failure_holds_every_read_off_for_upstreams_backoff_which_doubles_to_five_minutes` |
| B-no-doubling | 5 s, 10 s, 20 s ... | KILLED | 3 | the same |
| C-no-cap | five minutes at most | KILLED | 2 | the same |
| D-picker-no-floor | the menu's five seconds over rows | KILLED | 2 | `a_held_catalog_is_read_again_by_the_menu_five_seconds_on_and_by_activity_five_minutes_on` |
| E-never-stale | activity's five minutes over rows | KILLED | 2 | the same |
| F-failed-refresh-drops-rows | a failed refresh keeps the rows | KILLED | 1 | `a_refresh_that_fails_keeps_the_rows_says_so_and_is_tried_again_after_its_backoff` |
| G-backoff-from-start | the hold counts from the failure's arrival | KILLED | 3 | `a_background_read_counts_its_age_from_when_it_began_and_its_backoff_from_when_it_failed` |
| H-retry-hides-failure | a retry reads behind the failure | KILLED | 4 | `a_held_catalog_is_read_again_by_the_menu_...` |
| I-menu-no-revalidate | the menu over rows asks again | KILLED | 2 | `enter_on_a_menu_row_with_levels_goes_on_into_the_effort_column` |
| J-menu-asks-inside-hold | the menu inside a hold asks nothing | KILLED | 2 | `a_model_inside_a_failures_backoff_opens_on_the_failure_says_xfx_retries_and_asks_nothing` |
| K-prompt-no-activity | a submitted prompt asks | KILLED | 2 | `a_model_menu_asking_again_after_a_failed_read_mid_turn_...` |
| L-activity-ignores-hold | activity waits out the hold | KILLED | 3 | `a_prompt_past_a_failed_catalogs_backoff_asks_for_it_and_one_inside_asks_nothing` |
| T-retry-shows-loading | the column says the failure during a retry | KILLED | 3 | `a_model_menu_asking_again_after_a_failed_read_mid_turn_...` |
| U-unstartable-not-retryable | a read that could not start is retryable | KILLED | 1 | `a_read_that_could_not_start_says_upstreams_third_sentence` |
| M-refresh-note-ignored | the failed refresh's sentence in the note's place | KILLED | 1 | `a_refresh_that_failed_keeps_the_rows_and_says_xfx_retries_automatically_in_the_notes_place` |
| N-menu-resets-mark | new rows keep the tab and mark | KILLED | 1 | `a_revalidated_catalog_keeps_the_tab_and_the_marked_model` |
| O-turn-no-retry-wait | a turn waits for the retry | KILLED | 1 | `a_turn_waits_two_seconds_at_most_for_the_retry_behind_a_failed_catalog` |
| P-turn-unbounded-wait | two seconds at most | KILLED | 1 | the same |
| Q-work-asks-ignore-due | mid-work asks decide by the rule | KILLED | 1 | `the_sessions_own_read_failing_mid_work_is_read_again_once_its_backoff_is_up` |
| R-idle-ask-ignores-trigger | (the precheck only) | equivalent | 0 | -- |
| R2-idle-ask-reads-as-the-menu | the runtime passes the trigger through | KILLED | 1 | `a_prompt_over_a_held_catalog_reads_it_again_only_once_it_is_five_minutes_old` |
| S-old-sentence | `xfx retries automatically.` | KILLED | 8 | `a_refresh_that_fails_keeps_the_rows_says_so_...` |

## The gate (`gate.txt`, `gates.sh`)

Train 5's gate script unchanged, every CI step one after another inside one
hold of the suite lock, at the code-and-docs head with this receipt
untracked (staged for `check-no-secrets`), `TMPDIR` a short path:
| step | exit | s |
|---|---|---|
| fmt | 0 | 1 |
| clippy | 0 | 6 |
| clippy (fault-injection) | 0 | 16 |
| cargo test (5343 passed, 0 failed) | 0 | 105 |
| cargo test, fault-injection (5380 passed, 0 failed) | 0 | 121 |
| check-xfx-identity | 0 | 0 |
| check-no-stubs | 0 | 9 |
| check-no-secrets | 0 | 26 |
| check-preview-contract | 0 | 3 |
| check-tui-preflight-cost | 0 | 77 |
| check-usage-clock | 0 | 0 |
| test-check-no-secrets | 0 | 83 |
| test-gate-env | 0 | 62 |
| test-preview-contract | 0 | 45 |
| fxcap test-sweep | 0 | 13 |
| fxcap test-capture | 0 | 208 |
| build --release | 0 | 15 |
| build --release, fault-injection | 0 | 32 |
| smoke | 0 | 2 |
| smoke-tui (37 scenarios + the oracle, 1168 checks, 0 failures) | 0 | 65 |

**20 steps, 0 failed**, at `765bbc4d` (`# head 765bbc4d`; `tree clean: no`
is this receipt, staged and uncommitted, and nothing else). The test counts
are the sums of the `test result` lines in the steps' logs.

## How files got here (`receipt_copy.py`)

The captures, logs and run outputs were copied in from the session's
scratch directory, redacted by the repo's own `scripts/fxcap/redact.py`
(`Redactor.public`; `receipt_copy.py` is the copier): local paths read
`<repo>`, `<scratch>`, `<tmp>` and `<fx-bin>`, and no file here names a home
directory.
