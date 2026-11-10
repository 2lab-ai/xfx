# PROFILE-REFUSE-AT-LAUNCH

fx v0.0.16 refuses to launch with a profile it cannot trust; xfx launched
and said `config: 1 configuration issue`. This unit copies the refusal:
its conditions, its entry points, its text and its status.

- **Base** 3943d667 (train 12's PR head). **Branch**
  `fix/p5-profile-refuse-at-launch`. **Code** a57ba760; the commits after
  it are this receipt only, so every capture and probe here is of a binary
  built at a57ba760 (banner `v0.1.0-a57ba76`) or at the base (`3943d66`).
- Source outside tests: 125 added lines (`src/config.rs` 99, `src/app.rs`
  12, `src/interactive.rs` 7, `src/tui/mod.rs` 7); `src/config.rs`'s other
  126 are its unit tests.

## 1. fx's rule (`fx_src.sh` → `fx-src.txt`, every range checked: 0 out of bounds)

Read at the pin, then run against the oracle (`probe.sh` → `fx-fixtures.txt`
for every fixture, `fx-entries.txt` for every entry with one bad and one
good profile).

- **The check** (`fx-src.txt:3-31`, `app_lifecycle.zig:689-703`): in the
  local auth mode, a diagnostic of the **user layer** with cause
  `malformed_settings`, `settings_too_large` or `invalid_model_id` returns
  `error.InvalidProfileConfiguration`; `durable_path_unsafe` returns it
  unless the load's credential mode is `.stored`.
- **The causes, as they arise** (`fx-src.txt:77-482`):
  - the file is not valid JSON, or not an object (`.invalid`); over 64 KiB
    (`.oversized`); a read error other than an unsafe path or open
    permissions (`:1038-1043`) -- all user-layer `malformed_settings` /
    `settings_too_large`;
  - `workspaces`, or this workspace's entry in it, is not an object
    (`:589-597`): `malformed_settings`;
  - a `model` (in the object or the entry) that is not a string, or that
    fx's `validateModel` refuses -- empty, past 1024 bytes, a blank of
    ` \t\r\n` around it, an ASCII control in it (`settings_store.zig:894-902`):
    `malformed_settings` / `invalid_model_id`. A `skill_match_fuzzy` key fails
    the object first, with a cause no launch refuses (`:1645`);
  - `~/.fx` a link or not a directory, the settings file a link or a
    directory (`settings_store.zig:351-367`, `:721-729`): `durable_path_unsafe`;
  - **any other key of the wrong type** fails the whole user layer as
    `malformed_settings` too (`:1009-1036`; e.g. `"fast_mode": "yes"`, run:
    `fx-fixtures.txt` `typed-key`); and a bad `provider` or `models` stops fx
    with the parse error's own name instead (the salvage, `:898-911`; run:
    `provider` → `fx: UnknownConfiguredProvider`, `models-bad` →
    `fx: InvalidModelValue`). Section 6 says why those two are not copied.
  - Before the profile check, a bad `FX_ULTRAFAST` stops every mode
    (`error.InvalidUltrafastOverride`, `fx-src.txt:13-16`); a bad
    `FX_AUTH_MODE` is its own refusal (`main.zig:3281-3284`).
- **Layers**: the profile only -- its object and this workspace's entry.
  The project file's diagnostics are layer `.project` and never refuse
  (`fx-src.txt:145-184`; run: `project-malformed` starts).
- **Auth modes**: `local` only, fx's default (`credentials.zig:28-29`).
  `host-managed` skips the check.
- **Entry points** (`fx-src.txt:483-782`; run: `fx-entries.txt`): the TUI
  (`loadInteractiveStartupState`, `.stored`: refuses all but an unsafe
  path), `ask` (`loadStartupStateForRun`, `.refresh_if_needed`: refuses all
  five), and `permissions`/`models`/`workspace`/`credits`/`usage`/`upgrade`
  (no such xfx commands). `status`, `doctor`, `session`, `sessions`,
  `setup`, `--version` and help never refuse. fx has no line shell: with no
  terminal it says `fx requires an interactive terminal (TTY).` first
  (`fx-src.txt:806-813`).
- **Text and status** (`fx-src.txt:783-906`): `fx: InvalidProfileConfiguration`
  and a newline on stderr, exit 1, stdout empty; `ask --json` puts the
  JSON result with `"error":"InvalidProfileConfiguration"` on stdout and
  nothing on stderr (`fx-entries.txt:4-6`).
- **Before the refusal** (`fx-src.txt:907-941`; run: every refusal in
  `fx-fixtures.txt` has `tty=0B` and `touched:` empty): the TTY check and
  the termios capture are reads; the load comes before raw mode, the
  resize signal and the layout query. Nothing is written to the terminal,
  the home or the workspace.
- **A profile that goes bad after launch** still gives the `/status`
  panel's `Settings  couldn't be read: <error>` row (`fx-src.txt:942-960`):
  only launches load the startup state.

## 2. What xfx does now

`xfx-base-fixtures.txt` is the base (every launch went on); `xfx-head-fixtures.txt`
and `xfx-head-entries.txt` are a57ba760.

- `ProfileRefusal` (`src/config.rs`), computed by every load, acted on only
  by the launches (`RuntimeConfig::refuses_launch`), so a reload in a
  session never refuses:
  - `Rejected` -- the profile is there and is not one readable JSON object
    within 64 KiB; `workspaces` or this workspace's entry is not an object;
    a `model` in either fails fx's `validateModel` (`fx_refuses_model`,
    fx's rule, not xfx's own 200-byte one-word rule). Every launch refuses.
  - `Unsafe` -- `~/.xfx` or the file is a link, or the file a directory
    (`profile_path_unsafe`). Only `ask` refuses; the TUI and the line shell
    go on, as fx's stored-credential launch does.
- The TUI checks it after `load_config` and before `session` (before the
  terminal is taken and before a resume's lookup); the line shell after its
  terminal check and before the session store; `ask` first. Each writes
  `xfx: InvalidProfileConfiguration` and exits 1; `ask --json` writes
  `{"kind":"error","message":"InvalidProfileConfiguration"}` (xfx's JSONL
  error event) on stdout, stderr empty (`xfx-head-entries.txt:4-6`).
- Fixture by fixture the head matches fx on every refusal (`truncated`,
  `nonobject`, `toolarge`, `model-space`, `model-nonstring`, `ws-nonobject`,
  `entry-nonobject`, `entry-model`, `unreadable`: three entries each;
  `dir-settings`, `link-settings`, `link-profile`: `ask` only) with
  `tty=0B` and nothing touched; and starts where fx starts
  (`model-inner-space`, `model-long`, `project-malformed`,
  `retired-then-model`, `group-writable`, `valid`). The rest differ as
  section 6 says.

## 3. RED → GREEN (`red.txt`, `green.txt`)

- **RED at the base's code** (the tests below over 3943d667's `src`):
  `ask_refuses_a_profile_fx_refuses` (every refusing profile, text and
  `--json`, home and workspace snapshots unchanged),
  `ask_says_what_the_configuration_could_not_use_and_that_every_tool_is_refused`,
  `ask_with_a_permission_table_that_does_not_read_runs_no_tool`,
  `the_shell_refuses_a_profile_fx_refuses_before_it_writes_anything` and
  `a_profile_fx_refuses_is_refused_before_the_terminal_is_taken` (the exact
  pty text, no mode byte, the terminal's modes and both trees unchanged; a
  `--resume` too) all fail: 5 tests.
- **Guards, passing at the base and the head**: `ask_goes_on_with_a_profile_fx_starts_with`,
  `the_commands_fx_answers_without_a_launch_answer_with_a_profile_it_refuses`,
  `the_shell_starts_with_a_profile_only_ask_refuses`,
  `a_profile_only_ask_refuses_starts_the_tui`, and
  `a_profile_that_goes_bad_after_the_launch_is_a_panel_row_not_a_refusal`
  (the `/status` row, no refusal, the session ends with 0).
- **Unit tests** (new API, so not run at the base):
  `config::tests::a_launch_refuses_the_models_fx_refuses_and_no_other` (each
  edge of `validateModel`) and
  `config::tests::the_refusal_is_read_from_the_profile_and_this_workspaces_entry_only`.
- **GREEN**: all of them pass at a57ba760's code (`green.txt`); the whole
  suite, 5664 passed and 0 failed (`test-dev.txt`).

## 4. Tests changed to the refusal

Every test that starts xfx with a profile fx refuses was found by grepping
the tests for seeded profiles (`write_user_settings`, `settings.json`,
`with_profile`, `profile(`) and for the diagnostics such a launch printed;
the profiles each seeds were read against the rule.

- `tests/cli.rs` `ask_says_what_the_configuration_could_not_use_and_that_every_tool_is_refused`:
  its `{ this is not json` half now expects `xfx: InvalidProfileConfiguration`,
  exit 1 (its `permission` half is a typed key: section 6).
- `tests/gateway.rs` `ask_with_a_permission_table_that_does_not_read_runs_no_tool`:
  three of its profiles -- an entry `["x"]`, `{ this is not json`, and a
  file nobody can read -- now expect the refusal, no request and no write;
  its other two keep the per-key table refusal.
- No other test launched with such a profile: the others seed valid
  profiles, or swap a bad one in after launch (`src/tui/shell.rs`'s
  `/status` tests, `tests/llmux.rs`'s `setup` over a torn file, which `setup`
  never refuses), or load configuration in-process (`RuntimeConfig::load_with`).
- fxcap: `seed_scan.py` → `seed-scan.txt`: of 381 scenarios, 63 seed a
  profile before launch and one seeds a profile fx refuses,
  `p5-panels-badsettings`, already a launch-only capture under `keep`.

## 5. Captures, sweep and ledger

- `fx/`, `xfx/` (a57ba760) and `xfx-base/` (3943d667) hold
  `p5-panels-badsettings` and `p5-panels-status-badsettings` at 40x120.
  `cells.sh` → `cells.txt` (row 1 left out, no ledger rule): the head's
  `p5-panels-badsettings` `launch` differs from fx's in one row, 40, against
  6 rows at the base (`cells.txt:1-4`, `:31-44`); the three
  `p5-panels-status-badsettings` snaps differ in the same rows at the head
  and the base (1, 1 and 8: the hint row's `run /login`, and the panel's
  rows the ledger approves), so a profile gone bad after launch is
  unchanged.
- **Still different, and the harness's**: each dead pane's last row is
  tmux's `Pane is dead (status 1, <date>)`, the date the run's clock; the
  ledger's `tmux-dead-pane` normalization masks it. The refusal's own line
  is on stderr, which capture.sh does not put in the pane, so neither
  product's text is in the pair: `fx-fixtures.txt` and
  `xfx-head-fixtures.txt` carry it.
- **Sweep** (`sweep.sh` under the suite lock → `sweep.txt`): every
  `p5-panels*`, `settings*`, `startup*` and `config*` scenario and every one
  that seeds a profile, with HEAD's scenarios and ledger, once with the
  head's xfx and once with the base's. 83 scenarios, 523 pairs, every run
  `ran` on both sides; the oracle's sha256 the pin's before and after.
  - Head 278/523 passing, base 277/523; **moved to passing: 1**,
    `p5-panels-badsettings` `launch`, unresolved-defect → normalized-identical
    (its only difference, row 40, under `tmux-dead-pane`); **dropped from
    passing: 0** (`sweep.txt`; each side's `summary.md` in `sweep-head/`,
    `sweep-base/`).
  - One other verdict moved and both pass: `rules-read` `granted`,
    normalized-identical at the base → identical at the head. The base's run
    differed in one row, the turn's wall time (`0s` against `1s`), which the
    ledger's `turn-wall-time` masks: the run's clock, not this change
    (`sweep.txt`, last lines).
  - Under HEAD's ledger, which no longer holds the cause, the base's
    `p5-panels-badsettings` pair is an unresolved defect with 42 rows no
    cause covers: the base does not pass it, so it is no regression for the
    head either way.
- **Ledger**: the cause `profile-refuse-at-launch` is retired; a comment in
  its place says what it was and points here. The two scenarios' comments
  say both products refuse now.
- **Parity**: `docs/parity.md`'s `config diagnostics` row says the refusal,
  its conditions and entry points, and each part of fx's rule not copied.
- **References** (`refs.py` → `refs.txt`): 10 `path:N` references into the
  files this unit changed had moved and were re-pointed by text; at the
  head 52 are checked, 0 moved. Of the 4 bare `<name>:N` it lists for a
  hand, none names a file this unit changed at a moved line: `mod.rs:346`
  is `src/tools/mod.rs`, `interactive.rs:457-464` is `src/interactive.rs`
  (the line above this unit's), and `cli.rs:311-312` names `src/cli.rs`,
  which this unit does not touch (that one was already stale at the base).

## 6. Mutants (`mutants.py` → `mutants.txt`)

On a `git clone --no-local` of the head outside the worktree, each test
bounded at 900 s, under the suite lock. MUTANTS-RESULT

xfx has no auth mode -- no `FX_AUTH_MODE`, no host-managed launch -- so the
brief's "refusal in a non-local auth mode" has no counterpart to mutate. M8
breaks the one mode split fx makes inside the local mode: an unsafe path
refused by the interactive launches too, which fx's stored-credential
launch does not do.

## 7. Not copied, each a `docs/parity.md` sentence

- **A key of the wrong type** (`"fast_mode": "yes"`, `"permission": {"bash":
  "perhaps"}`): fx drops the whole profile and refuses; xfx keeps its
  per-key diagnostic (a reading that predates this unit,
  `parse_notifications`'s comment). Copying it means refusing every typed
  key's mistake at launch: a user decision.
- **A bad `provider` or `models`**: fx stops with the parse error's name
  (`fx: UnknownConfiguredProvider`, `fx: InvalidModelValue`) at every
  surface that loads settings, `status` too; xfx refuses the turn
  (`provider=rejected`) or drops the entry. The provider sets differ
  (`llmux` is xfx's alone).
- **An unsafe profile in an interactive launch**: fx goes on with model
  requests blocked; xfx goes on.
- `FX_AUTH_MODE` and `FX_ULTRAFAST` have no xfx counterpart.
- Seen in the oracle runs and older than this unit: fx's `status` writes
  `fx: config user: malformed_settings` on stderr for a refused profile
  (`fx-entries.txt`); xfx's `status` writes nothing there.

## 8. Gate (`gates.sh` under the suite lock → `gate.txt`)

GATE-RESULT

## Files

`fx_src.sh` → `fx-src.txt` · `probe.sh` → `fx-fixtures.txt`, `fx-entries.txt`,
`xfx-base-fixtures.txt`, `xfx-head-fixtures.txt`, `xfx-head-entries.txt` ·
`red.txt`, `green.txt`, `test-dev.txt` · `seed_scan.py` → `seed-scan.txt` ·
`refs.py` → `refs.txt` · `fx/`, `xfx/`, `xfx-base/` · `sweep.sh`,
`compare.py` → `sweep.txt` · `mutants.py` → `mutants.txt` · `gates.sh` →
`gate.txt`
