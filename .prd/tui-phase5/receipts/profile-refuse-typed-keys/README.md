# PROFILE-REFUSE-TYPED-KEYS

PROFILE-REFUSE-AT-LAUNCH left four fixtures where fx v0.0.16 refuses every
launch and xfx started (its receipt's `outcomes.txt`, section 7): a key of
the wrong type, a key written twice, a bad `provider` and a bad `models`.
This unit copies fx's parse of the profile for the keys both products read,
its refusal of a key written twice, and the stop a bad `provider` or
`models` makes at every surface that loads settings.

- **Base** 0a334851 (PROFILE-REFUSE-AT-LAUNCH's closed head). **Branch**
  `fix/p5-profile-refuse-typed-keys`. **Code** 1157fe3e. 92cbdde6 after it
  changes tests only -- `tests/*`, `tests/support/sandbox.rs`, and one case
  added to the parse table in `src/config.rs`'s test module -- and the
  receipt; the commits after that are this receipt only. The probes ran
  binaries built at 1157fe3e (each `status` line says
  `build_revision=1157fe3eb3bc`); the gate, RED and mutants ran at 92cbdde6.
- **Source outside tests** (`count.py` → `count.txt`): 297 added lines at
  1157fe3e, 58 of them comments (`src/config.rs` 285, `src/app.rs` 8,
  `src/interactive.rs` 2, `src/tui/mod.rs` 2), under the brief's ~300.

## 1. fx's rule (`fx_src.sh` → `fx-src.txt`, every range checked: 0 out of bounds)

Read at the pin, then run against the oracle (`probe.sh` → `fx-fixtures.txt`,
`fx-edge.txt`, `fx-entries.txt`; FX_AUTO_UPGRADE=0, the oracle's sha256
`8e6cda2efddb` checked before and after each run, each file's first and
last lines).

- **The refusal** (`fx-src.txt` §1, `app_lifecycle.zig:689-703`): a user-layer
  diagnostic `malformed_settings`, `settings_too_large` or `invalid_model_id`
  refuses every launch with `InvalidProfileConfiguration`.
- **A key of the wrong type** (§3-4): the global object (layer `.profile`)
  and this workspace's entry (`.profile_workspace`) are each parsed whole
  (`config_runtime.zig:563-632`) by `parseSettingsValueForLayer`
  (`:1611-1636`): `parseProfileOnlyFields` (`:1638-1860`), then
  `parseProjectSafeFields` (`:1898-1953`). The **first** error fails the
  layer; its cause decides (`diagnosticCauseForParseError`, `:1025-1037`):
  `InvalidModelValue` is `invalid_model_id`; `RetiredSkillMatchFuzzy`, a
  context-limits error and `InvalidSkillSymlinkAuthorities` have causes no
  launch refuses; every other error is `malformed_settings`. So a key both
  products read, of the wrong type or outside fx's grammar, refuses -- unless
  an earlier key in fx's order failed with a sparing cause. Run
  (`fx-fixtures.txt`, `fx-edge.txt`): refused -- `typed-key`
  (`"fast_mode": "yes"`), `typed-permission` (`"bash": "perhaps"`),
  `typed-entry`, `typed-steps`, `typed-steps-negative`, `steps-float`
  (`5.0`), `steps-huge` (past i64), `typed-effort`, `effort-bad`,
  `typed-mode`, `mode-padded` (`" ask"`: fx does not trim), `typed-notify-max`,
  `typed-statusline`, `typed-statusline-ws`, `typed-history`,
  `limits-ok-then-typed`, `limits-off-then-typed`,
  `limits-underscore-then-typed`, `limits-double-underscore-then-typed`
  (`"1__0"` is digits to `parseUnsigned`); started -- `entry-statusline`
  and `entry-statusline-ws` (an entry tolerates a non-object `statusLine`
  and never reads its `workspace`, `:1816-1835`), `typed-authorities`,
  `authorities-then-typed`, `limits-then-typed`, `limits-plus-then-typed`,
  `limits-negative-then-typed`, `limits-unknown-then-typed`,
  `limits-leading-underscore-then-typed`, `limits-empty-then-typed`,
  `mode-upper` (`"ASK"`), `additional-global`.
- **A key written twice** (§2): fx parses settings with `std.json`'s default
  options, which refuse a duplicate field at every depth -- the profile
  (`loadPrimary`, `settings_store.zig:400-420`: `.invalid`, so
  `malformed_settings`, `config_runtime.zig:463-521`) and the project file
  (`:523-562`: project `malformed_settings`, the layer not applied, never a
  refusal); `Registry.parse` states it as a precondition
  (`configured_provider.zig:109-129`) and `loadConfiguredProviders` passes
  it explicitly (`config_runtime.zig:357-368`). Keys are compared decoded.
  Run: `dup-key`, `dup-nested` (inside `permission`), `dup-unread` (inside
  a key fx never reads), `dup-entry`, `dup-escaped` (`"fast_mode"` beside `"fast\u005fmode"`)
  refused at every entry; `dup-project` starts with `config project:
  malformed_settings`.
- **A bad `provider` or `models`** (§3): a layer whose parse fails is parsed
  again for `provider`, `providers` and `models` alone (the salvage,
  `salvageProviderRouting`, `:888-925`); when that fails too,
  `mergeDetailedSettingsLayer` returns the layer's **first** error and the
  load fails with it (`:1011-1015`). After every layer, the provider is
  bound to the connections `providers` defines (`resolve_provider_selection`,
  `:350-355`; `bind`, `model_provider.zig:53-62`): a name `parse` makes a
  configured one (`model_provider.zig:106-114`, `validate_id`,
  `configured_provider.zig:290-299`) that no definition names is
  `UnknownConfiguredProvider`. Run: `provider` and `provider-entry` and
  `provider-typed-unknown` → `UnknownConfiguredProvider`; `provider-type` →
  `InvalidProviderType`; `provider-badid` and `models-key` →
  `InvalidProviderValue`; `models-bad` → `InvalidModelValue`; `models-type`,
  `models-value-type` → `InvalidModelType`; `model-then-models` →
  `InvalidModelValue` and `model-then-provider` → `InvalidModelValue`, not
  `InvalidProviderType` (the `model`'s, the layer's first error);
  `retired-then-provider` → `RetiredSkillMatchFuzzy`. Salvaged: `provider-salvaged` and
  `model-bad-with-provider` refuse with `InvalidProfileConfiguration`.
- **Where a failed load stops** (§5; `fx-entries.txt`, every entry for
  `typed-key`, `dup-key`, `provider`, `models-bad`, `provider-type`): `ask`
  (`ask --json`: the error name in the JSON result), `permissions`, the
  interactive launch and `status` (`cli_surface.zig:1336-1357`; `status
  --json` too) exit 1 with `fx: <Name>`; `doctor`, `sessions`, `session
  last`, `--version` and help answer as with a valid profile. A refused
  (not stopped) profile does not stop `status`: it exits 0 and writes
  `fx: config user: malformed_settings` on stderr.
- **Quoted for PROFILE-REFUSE-AT-LAUNCH** (its opus r4 NIT 2; §6):
  `readOptionalFile` (`config_runtime.zig:1415-1429`),
  `openExistingRegularFile`'s tail (`io.zig:255-265`) and `makeFileBlocking`
  (`io.zig:289-315`).

## 2. What xfx does now

- `fx_profile_refusal` (`src/config.rs`), computed by every load from the
  profile object it read, acted on only by the launches and `status`
  (`RuntimeConfig::launch_refusal(Launch)`): the global object, then this
  workspace's entry; for each, `fx_stop` (its routing keys do not parse:
  the layer's first error, the retired key or the `model` before the
  routing key's own) and `fx_first_error_refuses` (the first error, in fx's
  order, among the keys xfx reads and the two fx-only checks whose cause
  spares the launch -- `context_limits`, `skill_symlink_authorities` -- and
  the retired key). Then the provider: a configured name (`fx_provider`),
  not xfx's own `llmux`, in a profile with no `providers` key, stops with
  `UnknownConfiguredProvider`. Each value grammar is fx's, not xfx's:
  `fx_valid_model`, `fx_permission_mode` (any case, no trim), effort,
  `max_agent_steps` (an integer in i64, not negative), the permission table
  (`RuleSet::parse`, the same three errors), the context limits.
- `ProfileRefusal::Stopped(name)` is every launch's and `status`'s refusal,
  `xfx: <name>` on stderr (`ask --json`: the name as its error event), exit 1,
  before anything is written; `Rejected` stays `InvalidProfileConfiguration`
  at the launches and nothing at `status`.
- `read_opened` refuses a file naming a key twice (`KeysOnce`, a serde
  visitor over the same bytes, keys compared decoded), so the profile is
  `malformed_settings` (every launch refuses) and the project file is
  `config project: malformed_settings` and not applied; the `/status`
  panel says `InvalidSettingsFormat` for it, fx's name.
- Only content under `ProfileOpen::Read` counts: a profile fx does not read
  (unsafe, open to others, unopenable) never stops.

## 3. Outcomes, before and after

`probe.sh` is PROFILE-REFUSE-AT-LAUNCH's, with 55 fixtures of this unit
(90 in all) and `status` among the entries every run takes; `outcomes.py`
is its, comparing `status` too and the error's **name** at each entry, not
only whether one was named. The oracle: `fx-fixtures.txt` and
`fx-edge.txt`. xfx at the base (`xfx-base-fixtures.txt`, every `status`
line `build_revision=0a334851f337`) and at the code commit
(`xfx-head-fixtures.txt`, `build_revision=1157fe3eb3bc`):

- **Before** (`outcomes-base.txt`): **51 of 90** match by refusal at every
  entry, **48** by how every entry ended. Every class the brief names
  differs -- 40 fixtures: `typed-key` and 19 more typed ones, `dup-key`
  and 4 more, `provider` and 7 more, `models-bad` and 5 more (with
  `model-then-models` and `model-then-provider`), `retired-then-provider`
  -- plus the two older ones below (each line names the entries).
- **After** (`outcomes.txt`): **88 of 90** by refusal, **85** by how each
  entry ended. Each class's fixtures match at `ask`, `status`, the TUI and
  the line shell, with fx's error name, but the three section 7 does not
  copy. What still differs:
  - `typed-fx-only` and `provider-llmux` by refusal, and `provider-codex`
    by how it ended: not copied (section 7);
  - `link-profile` and `dir-group-writable-torn` by how they ended: older
    than both units (PROFILE-REFUSE-AT-LAUNCH, section 2).
- **Every entry** (`fx-entries.txt`, `xfx-head-entries.txt`, `typed-key`,
  `dup-key`, `provider`, `models-bad`, `provider-type`): `ask`, `ask --json`
  (`{"kind":"error","message":"<Name>"}`), `status`, the TUI and the line
  shell stop or refuse as fx's do, with `tty=0B` and nothing touched;
  `doctor`, `sessions`, `session last`, `--version` and help answer.
- **fxcap** (`seed_scan.py` → `seed-scan.txt`): each of the 63 scenarios
  that seed a profile was judged by the base binary and the head binary;
  **0 moved** (only `p5-panels-badsettings` is refused, by both, as
  before). No capture this unit could change was found, so there is no
  sweep: the refusal's text is on stderr, which capture.sh does not put in
  the pane (PROFILE-REFUSE-AT-LAUNCH, section 5).

## 4. RED → GREEN

- **The per-class, per-entry RED is the probe** (section 3): every class
  at `ask`, `status`, the TUI and the line shell, base against oracle
  (`outcomes-base.txt`, 51/90) and head against oracle (`outcomes.txt`,
  88/90).
- **The tests' RED** (`red.sh` → `red.txt`): the head's tests over the
  base's `src` in a throwaway clone (`git clone --no-local`), each run
  bounded at 900 s. They fail where the base starts:
  `ask_refuses_a_profile_fx_refuses` at `a switch that is not a bool` (the
  base says `config user: invalid_value; key=fast_mode` and goes to the
  Gateway), `status_stops_where_fxs_load_fails_and_the_rest_still_answer`
  at `a provider nothing defines ["status"]` (exit 0, not 1),
  `the_shell_refuses_a_profile_fx_refuses_before_it_writes_anything` and
  `a_profile_fx_refuses_is_refused_before_the_terminal_is_taken` (xfx did
  not exit), and `ask_with_a_permission_table_that_does_not_read_runs_no_tool`
  (the table fx stops at ran the turn: exit 0). The guard
  `ask_goes_on_with_a_profile_fx_starts_with` passes there, as it must:
  its neighbours start in both. The clone's `src` is the base's whole, so
  its unit tests are the base's; the mutants (section 5) stand for the
  RED of `a_launch_refuses_what_fxs_parse_of_the_profile_refuses` and
  `a_key_twice_is_a_file_that_does_not_parse_in_either_layer`.
- **GREEN** (`green.txt`): the unit's tests in the gate's `cargo test` at
  92cbdde6, from its log.
- **Unit tests**: `config::tests::a_launch_refuses_what_fxs_parse_of_the_profile_refuses`
  (58 profiles and a project file with a key twice, each with the refusal
  fx's oracle run gives it, or the reason it is not copied), `a_key_twice_is_a_file_that_does_not_parse_in_either_layer`
  (top level, nested in an array, decoded escapes; the same key in
  different objects is no duplicate), and the base's
  `the_refusal_is_read_from_the_profile_and_this_workspaces_entry_only`
  now asks `launch_refusal` at `ask`, the interactive launch and `status`.
- **Integration**: `tests/support/profiles.rs` gains five REJECTED
  profiles (a switch, a permission action, an entry's switch, a key twice,
  a key twice inside a table), a STOPPED list of five (with the name each
  stops with), and seven STARTED neighbours; the `ask`, line-shell and TUI
  refusal tests run every REJECTED and STOPPED profile and compare the
  exact `xfx: <Name>` line; `status_stops_where_fxs_load_fails_and_the_rest_still_answer`
  runs every STOPPED one through `status` and `status --json` (exit 1,
  stdout empty) and `--version`, help, `doctor`, `sessions`, `session last`
  (they answer).

## 5. Mutants (`mutants.py` → `mutants.txt`)

On a `git clone --no-local` of 92cbdde6 outside the worktree, each test
bounded at 900 s, under the suite lock. One or more per class:

- M1 (typed keys: a later key's error never refuses), M2 (a key twice:
  the check always passes), M3 (provider: the lookup never stops), M4
  (models: the routing check never reads `models`), M5 (`status` does not
  stop), M6 (the context limits' error refuses), M7 (the stop names the
  routing key's error, not the layer's first), M8 (an entry's non-object
  `statusLine` refuses).
- **8 of 8 killed** (`mutants.txt`), every kill an assertion's panic
  location, none a timeout: the parse table (`src/config.rs:4797`) kills
  M1-M4 and M6-M8, the key-twice test (`:4818`) M2, and the integration
  tests (`tests/cli.rs:1998`, `:2019`, `:2074`, `tests/support/pty.rs:430`
  for the shell and the TUI) each class's entries; M5 dies at `status`'s
  test alone.
- `mutants-r1.txt` is the same run at 1157fe3e, before 92cbdde6: M7
  survived there, because the only case with an earlier error, `{"model":
  " a", "models": {"gateway": " b"}}`, names `InvalidModelValue` either
  way. 92cbdde6 adds `{"model": " a", "provider": 5}` (fx:
  `InvalidModelValue`, `fx-edge.txt` `model-then-provider`), where the
  mutant says `InvalidProviderType`.

## 6. Carried from PROFILE-REFUSE-AT-LAUNCH's round 4

- **gpt6 SHOULD (the FIFO test's failure path):** `ended` (`src/config.rs`,
  in `the_open_checks_what_it_opened_not_what_was_looked_at`) waits at most
  5 s for the open's thread and joins it only once `is_finished`; the
  timeout branch holds the writer descriptor (`_writer`) while it waits,
  and the disconnected branch goes through the same bound. A thread still
  running is left and the test fails.
- **opus NIT 3:** `ProfileOpen::Unopenable`'s doc names the failed
  `makeFileBlocking`; `profile_open`'s doc names the "made blocking" step.
- **opus NIT 4:** `snapshot` (`tests/support/profiles.rs`) now panics where
  it cannot stat or list a directory instead of returning nothing, so the
  three "the home is listed" assertions (`tests/cli.rs`,
  `tests/interactive.rs`, `tests/tui.rs`) are gone: an empty HOME is a
  listing of its own, and a blind one is a panic in every caller.
- **opus NIT 5 (optional, done):** `read_opened` makes the descriptor
  blocking right after its kind, before the size check, as
  `openExistingRegularFile` does (`io.zig:260-262`).
- **opus NIT 2:** `fx-src.txt` §6 quotes `readOptionalFile`,
  `openExistingRegularFile`'s tail and `makeFileBlocking`.
- **Moved with the rule:** `tests/cli.rs`
  `ask_says_what_the_configuration_could_not_use_and_that_every_tool_is_refused`
  and `tests/gateway.rs` `ask_with_a_permission_table_that_does_not_read_runs_no_tool`
  seeded a `permission` table fx's parse stops at, which now refuses the
  launch: both now check that refusal, and keep the fail-closed table
  (every tool refused) on a profile fx starts with --
  `"skill_symlink_authorities": 5` before the same table, a first error no
  launch refuses (`authorities-then-typed`).

## 7. Not copied (`docs/parity.md`'s `config diagnostics` row)

- **A key only fx reads** (`typed-fx-only`, `"theme": 5`): fx refuses; xfx
  does not read the key, so it starts. The receipt's section 7 (a)
  decision covers the keys both products read; a key only xfx reads
  (`backend`, `llmux_url`; `typed-xfx-only`) keeps its per-key diagnostic
  and fx starts too. The fx-only checks that come before a shared key and
  spare the launch (`context_limits`) are copied, so xfx never refuses a
  profile fx starts with over an earlier fx-only error.
- **Providers xfx does not have** (a provider registry change, outside
  config): `llmux` is xfx's own (fx: `UnknownConfiguredProvider`,
  `provider-llmux`); `codex` and `grok` are fx's built-ins, and fx stops
  with `no Codex model is selected` where xfx refuses the turn
  (`provider=rejected`; `provider-codex`); a profile with a `providers` key
  defines fx's configured connections, which xfx does not read, so its
  provider keeps xfx's turn refusal.
- **A layer fx drops** at its first error is still applied key by key at
  `status`, `doctor` and a reload in a session (fx applies nothing of it
  but the salvaged routing). The launches refuse it, so this shows only
  where nothing refuses. fx's `status` writes each diagnostic on stderr,
  xfx's none (older than both units).
- **fx's `/status` panel** reads with `loadMergedSettingsFromPaths`
  (`fx-src.txt` §7), which names a mistyped key's error; xfx's names only a
  file that does not read (now including one with a key twice:
  `InvalidSettingsFormat`, fx's word).
- **A settings write** over a file with a key twice: fx's refuses
  (`settings_store.zig:514-518`), xfx's rewrites it.
- **A `models` entry fx takes and xfx's own rule does not**
  (`"openai/gpt 5"`, past 200 bytes): a `config user: invalid_value;
  key=models` line, and the entry not used (`models-inner-space`,
  `xfx-head-fixtures.txt`). The silent case MODELS-ENTRY-DIAGNOSTIC was
  queued for -- `models-bad`'s `" openai/gpt-5"`, which xfx trimmed and
  took without a word -- is one fx's `validateModel` refuses, so it now
  stops every launch with `xfx: InvalidModelValue`.

## 8. Gate (`gates.sh` under the suite lock → `gate.txt`)

At 92cbdde6 (code 1157fe3e), the tree clean before, one hold of the lock
(`gate.txt`): **21 of 22 steps passed** -- fmt, both clippys,
`--features fault-injection` `cargo test` (5709 passed, 0 failed), the
identity, stubs, secrets and local-path checks and their self-tests, the
preview contract, the preflight cost, the usage clock, `test-gate-env`,
the fxcap harness tests, both release builds, `smoke.sh` (47 checks) and
`smoke-tui.sh` (37 scenarios and the oracle, 1167 checks, 0 failures).

- **Step 04, plain `cargo test`: 5670 passed, 2 failed**, both
  `tui::worker` catalog-wait clock tests
  (`a_stop_in_the_catalog_wait_of_a_turn_that_would_ask_twenty_questions_ends_it_as_the_stop`,
  `a_stop_in_a_turns_catalog_wait_ends_it_at_once_and_the_next_start_keeps_its_number`):
  each found a stopped turn's end outside its 300 ms-1 s window. This unit
  changes 0 lines of `src/tui/worker.rs`, and those tests load a temp
  home with no settings file (`retry_never_lands`), so the new parse sees
  no profile there. It is the defect class
  CATALOG-WAIT-CLOCK (`fix/p5-catalog-wait-clock`, not in this base)
  moves those tests onto a paused clock for.
- **Rerun under one hold of the lock** (`rerun.sh`, same file): the full
  `cargo test` at 92cbdde6, **5672 passed, 0 failed**, and each of the
  three catalog-wait clock tests alone three times, **9 of 9 passed**.
  Unheld, the same three alone failed once in nine (`worker-alone.txt`,
  the third test's first run), which the brief says is not evidence
  either way.
- **Tree clean after: no** -- the one change during the run was
  `refs.txt`, this receipt's, written untracked; nothing tracked changed.

## Files

`fx_src.sh` → `fx-src.txt` · `probe.sh` → `fx-fixtures.txt`, `fx-edge.txt`,
`fx-entries.txt`, `xfx-base-fixtures.txt` (0a334851), `xfx-head-fixtures.txt`,
`xfx-head-entries.txt` (1157fe3e) · `outcomes.py` → `outcomes-base.txt`,
`outcomes.txt` · `seed_scan.py` → `seed-scan.txt` · `count.py` → `count.txt` ·
`refs.py` → `refs.txt` · `red.sh` → `red.txt` · `mutants.py` → `mutants.txt`,
`mutants-r1.txt` · `gates.sh` → `gate.txt`, `green.txt` · `rerun.sh` → `gate.txt` (its last block) · `worker-alone.txt`
