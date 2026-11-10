# PROFILE-REFUSE-AT-LAUNCH

fx v0.0.16 refuses to launch with a profile it cannot trust; xfx launched
and said `config: 1 configuration issue`. This unit copies the refusal:
its conditions, its entry points, its text and its status.

- **Base** 3943d667 (train 12's PR head). **Branch**
  `fix/p5-profile-refuse-at-launch`.
- **Round 1** code a57ba760 (reviewed at 98d3513b). **Round 2** code
  b209c786: the profile is read in fx's order, so a profile fx never reads
  refuses nothing. **Round 3** code 02c32c3f: the order kept on
  descriptors, as fx keeps it -- HOME opened first, `~/.xfx` and the file
  opened without following a link, fstat'ed, and the file read from the
  descriptor that was checked. **Round 4** code adaa56de: a file a rename
  left with no name after its open is read, as fx's read-only open reads
  it; every other layer looked at before it is opened; `O_NOCTTY`, and both
  descriptors made blocking after the check. The commits after adaa56de
  change no source: this receipt and `docs/parity.md`. Every xfx probe and
  capture here names the commit its binary was built at (its revision
  stamp).
- Source outside tests: 312 added lines at adaa56de -- as at 02c32c3f,
  counted the same way (lines added above each file's tests; round 3 said
  311) -- 81 of them comments (`src/config.rs` 286, `src/app.rs` 12,
  `src/interactive.rs` 7, `src/tui/mod.rs` 7): round 4's code is paid for
  by doc lines cut.

## 1. fx's rule (`fx_src.sh` → `fx-src.txt`, every range checked: 0 out of bounds)

Read at the pin, then run against the oracle (`probe.sh` → `fx-fixtures.txt`
for every fixture, `fx-entries.txt` for every entry with one bad and one
good profile; FX_AUTO_UPGRADE=0, the oracle's sha256 checked before and
after each run).

- **The check** (`fx-src.txt:3-31`, `app_lifecycle.zig:689-703`): in the
  local auth mode, a diagnostic of the **user layer** with cause
  `malformed_settings`, `settings_too_large` or `invalid_model_id` returns
  `error.InvalidProfileConfiguration`; `durable_path_unsafe` returns it
  unless the load's credential mode is `.stored`.
- **The order a read-only load opens the profile in** (`fx-src.txt:396-597`),
  every step on a descriptor:
  0. HOME, opened as a directory (`settings_store.zig:342-348`): missing is
     no profile, any other failure is returned, so `malformed_settings` --
     the oracle refuses a `0o300` HOME at every entry, with a profile or
     without one (`home-unlistable`; `home-unlistable-empty`,
     `fx-r4-fixtures.txt`);
  1. `~/.fx`, opened in it without following a link: a link or not a directory
     is `durable_path_unsafe`; another failed open is returned, so
     `malformed_settings` (`settings_store.zig:351-367`, `config_runtime.zig:1038-1043`);
  2. its mode: open to group or other (`mode & 0o022`, `:763-765`) is
     `private_state_permissions_unsupported`, a cause no launch refuses, and
     no store: the file is never opened (`:381-383`, `config_runtime.zig:467-474`);
  3. `settings.json`, looked at without following a link: a link, anything
     but a regular file, or a second hard link is `durable_path_unsafe`
     (`io.zig:213-286`) -- not a file with no link left, which a lookup
     racing a rename over it returns: a read-only open takes it
     (`:221-224`, `:281-286`); then opened for reading with `NOFOLLOW`,
     `NONBLOCK` and `NOCTTY` (`:238-254`) -- a failure is returned, so
     `malformed_settings`, before the mode is looked at -- the open
     descriptor checked again (`:260-261`) and made blocking (`:262`);
  4. the open descriptor's mode: open to group or other is again
     `private_state_permissions_unsupported`, and the layer is absent
     (`settings_store.zig:744-746`, `config_runtime.zig:477-484`);
  5. only then the size (past 65,536 bytes: `settings_too_large`, `:747`),
     the read of that same descriptor (`:748`), the parse (not JSON, not an
     object, or a key twice: `malformed_settings`, `:408-416`; the oracle
     refuses `{"permission_mode": "auto", "permission_mode": "ask"}`,
     `dup-key`), and the object:
     - `workspaces`, or this workspace's entry in it, not an object
       (`config_runtime.zig:589-597`): `malformed_settings`;
     - a `model` (in the object or the entry) that is not a string, or that
       `validateModel` refuses -- empty, past 1024 bytes, a blank of
       ` \t\r\n` around it, an ASCII control in it (`settings_store.zig:894-902`):
       `malformed_settings` / `invalid_model_id`. A `skill_match_fuzzy` key
       fails the object first, with a cause no launch refuses (`:1645`);
     - **any other key of the wrong type** fails the whole user layer as
       `malformed_settings` too (`:1009-1036`; run: `typed-key`); a bad
       `provider` or `models` stops fx with the parse error's own name
       instead (the salvage, `:898-911`; run: `provider` →
       `fx: UnknownConfiguredProvider`, `models-bad` → `fx: InvalidModelValue`).
       Section 7 says what is done with those.
- **The oracle on that order** (`fx-fixtures.txt`): torn JSON or
  `{"model":" a"}` in a `0o664` file, torn JSON in a `0o602` file, and torn
  JSON in a `0o770` `~/.fx` all start, each saying
  `config user: private_state_permissions_unsupported`; a `0o222` file and
  a `~/.fx` nobody may list (`0o300`) are refused at every entry; a
  hard-linked file -- one nobody may read too, unsafe on the look before an
  open that would fail (`hardlink-unreadable`, `fx-r4-fixtures.txt`) -- a
  FIFO and a file where `~/.fx` belongs are refused by `ask` alone; 65,536
  bytes start and 65,537 are refused.
- Before the profile check, a bad `FX_ULTRAFAST` stops every mode
  (`error.InvalidUltrafastOverride`, `fx-src.txt:13-16`); a bad
  `FX_AUTH_MODE` is its own refusal (`main.zig:3281-3284`).
- **Layers**: the profile only -- its object and this workspace's entry.
  The project file's diagnostics are layer `.project` and never refuse
  (`fx-src.txt:145-184`; run: `project-malformed` starts).
- **Auth modes**: `local` only, fx's default (`credentials.zig:28-29`).
  `host-managed` skips the check.
- **Entry points** (`fx-src.txt:598-955`; run: `fx-entries.txt`): the TUI
  (`loadInteractiveStartupState`, `.stored`: refuses all but an unsafe
  path), `ask` (`loadStartupStateForRun`, `.refresh_if_needed`: refuses
  all), and `permissions`/`models`/`workspace`/`credits`/`usage`/`upgrade`
  (no such xfx commands). `status` (`loadStartupStatus`, `:635-706`),
  `doctor` (`:800-849`), `session`/`sessions` (`:850-897`), `setup`
  (`runPasteSetup`, which reads a pasted key and no settings, `:898-955`),
  `--version` and help (`:752-776`) never refuse; `fx-entries.txt` runs all
  of them but `setup`. fx has no line shell: with no terminal it says
  `fx requires an interactive terminal (TTY).` first (`fx-src.txt:979-986`).
- **Text and status** (`fx-src.txt:956-1079`): `fx: InvalidProfileConfiguration`
  and a newline on stderr, exit 1, stdout empty; `ask --json` puts the
  JSON result with `"error":"InvalidProfileConfiguration"` on stdout and
  nothing on stderr (`fx-entries.txt`, `truncated`).
- **Before the refusal** (`fx-src.txt:1080-1114`; run: every refusal in
  `fx-fixtures.txt` has `tty=0B` and `touched:` empty): the TTY check and
  the termios capture are reads; the load comes before raw mode, the
  resize signal and the layout query.
- **A profile that goes bad after launch** still gives the `/status`
  panel's `Settings  couldn't be read: <error>` row (`fx-src.txt:1115-1133`):
  only launches load the startup state.

## 2. What xfx does now

- `ProfileRefusal` (`src/config.rs`), computed by every load, acted on only
  by the launches (`RuntimeConfig::refuses_launch(Launch)`), so a reload
  in a session never refuses. `profile_open` walks fx's order above on
  descriptors: HOME opened as a directory (missing: no profile; any other
  failure: Rejected); `~/.xfx` opened in it with `O_NOFOLLOW|O_DIRECTORY`
  and fstat'ed for kind and mode; the file looked at with
  `statat(SYMLINK_NOFOLLOW)`, then opened in that directory with
  `O_NOFOLLOW|O_NONBLOCK|O_NOCTTY|O_CLOEXEC` (`open_settings`), fstat'ed for
  kind and links (a second name unsafe; none, a file renamed over after the
  open, read as fx reads it), made blocking, then its mode. For
  `ProfileOpen::Read` that descriptor is handed back and the layer read
  from it (`read_user_layer`), never the path again; under any other
  verdict the refusal does not turn on what the file holds, and the layer
  is read by its path as any other. Every other layer (`read_layer`) is
  looked at first, following a link, so only a regular file is opened
  (fx's `readOptionalFile`), then opened with `O_NONBLOCK|O_NOCTTY`,
  fstat'ed, made blocking and read from that descriptor (`blocking`; the
  profile's descriptor is made blocking in `open_settings`, inside the
  match guard after its kind and links, as fx's `makeFileBlocking` comes
  before the mode, and `read_opened` does it again, a no-op). The verdicts:
  - `Unsafe` -- `~/.xfx` a link or not a directory; the file a link, not a
    regular file, or with a second hard link. Only `ask` refuses; the TUI
    and the line shell go on, as fx's stored-credential launch does --
    except that a `~/.xfx` that is a link or a file is still refused by the
    session store's own rule, in its own words (`src/profile_home.rs`,
    kept by `docs/parity.md`'s profile home row).
  - `Rejected` -- HOME, `~/.xfx` or the file does not open for reading (a
    `0o222` file too); or the file is opened, private, and is not one JSON object
    within 65,536 bytes, its `workspaces` or this workspace's entry is not
    an object, or a `model` in either fails fx's `validateModel`
    (`fx_refuses_model`, fx's rule, not xfx's own 200-byte one-word rule).
    Every launch refuses.
  - A `~/.xfx` or file open to group or other: not read for the refusal,
    so nothing refuses (its layer is still read by its path and applied, as
    at the base: section 7) -- but an interactive launch still meets the session store's rule for
    a `~/.xfx` open to group or other (`it is mode 770`).
- The TUI checks after `load_config` and before `session` (before the
  terminal is taken and before a resume's lookup); the line shell after its
  terminal check and before the session store; `ask` first, before the
  permission mode is resolved. Each writes `xfx: InvalidProfileConfiguration`
  and exits 1; `ask --json` writes
  `{"kind":"error","message":"InvalidProfileConfiguration"}` (xfx's JSONL
  error event) on stdout, stderr empty (`xfx-head-entries.txt`).
- **Fixture by fixture** (`xfx-head-fixtures.txt`, adaa56de, against
  `fx-fixtures.txt` and `fx-r4-fixtures.txt`; `outcomes.py` →
  `outcomes.txt`), two counts:
  - **by refusal** -- the launch refused over the profile, or not, at
    `ask`, the TUI and the line shell: **31 of 35**; the four that differ,
    `typed-key`, `provider`, `models-bad` and `dup-key`, are section 7's;
  - **by how each entry ended** -- exit status, or still running: **29 of
    35**; besides those four, `link-profile` and `dir-group-writable-torn`,
    where fx's interactive launch runs and xfx's session store refuses in
    its own words (as at the base, `xfx-base-fixtures.txt`). `profile-file`
    ends the same way in both (exit 1): fx later with `fx: NotDir`, xfx with
    the session store's words.
  Refused at all three entries with `tty=0B` and nothing
  touched: `truncated`, `nonobject`, `toolarge` (65,537 bytes),
  `model-space`, `model-nonstring`, `ws-nonobject`, `entry-nonobject`,
  `entry-model`, `unreadable`, `write-only`, `dir-unlistable`,
  `home-unlistable`, `home-unlistable-empty`. Refused by
  `ask` alone: `dir-settings`, `link-settings`, `link-profile`, `hardlink`,
  `hardlink-unreadable`, `profile-file`, `fifo`. Started: `at-ceiling` (65,536 bytes),
  `model-inner-space`, `model-long`, `project-malformed`,
  `retired-then-model`, `group-writable`, `group-writable-torn`,
  `group-writable-model`, `other-writable-torn`, `dir-group-writable-torn`
  (`ask`), `valid`; and, where fx refuses, `typed-key`, `provider`,
  `models-bad` and `dup-key`.
- **Round 1 against the same fixtures** (`xfx-r1-fixtures.txt`, a57ba760's
  code = 98d3513b's): round 1 refused `group-writable-torn`,
  `group-writable-model`, `other-writable-torn` and
  `dir-group-writable-torn` at all three entries, where fx starts (the
  MUST); refused `fifo` at all three, where fx refuses only `ask`; and
  started `dir-unlistable` and `hardlink`, which fx refuses. Round 2 fixes
  all seven.
- **Round 2 against the same fixtures** (`xfx-r2-fixtures.txt`, b209c786):
  started `home-unlistable`, which fx refuses (28 of 33 by refusal, 26 by
  how each entry ended, `outcomes.txt`); round 3 fixes it.
- **Round 3** (`xfx-r3-fixtures.txt`, 02c32c3f): 29 and 27 of the 33. Its
  33 against round 4's: the same verdict and the same files touched for
  each; only the line shell's still-running lines differ, by 2 TTY bytes
  (the run root, 2 characters shorter) and by how much of its last stderr
  line was written before the probe killed the pane. The probe now lists a
  directory its owner may not list (`listing`), which round 3's saw
  nothing in: `dir-unlistable` and `home-unlistable` were run again on the
  oracle (`fx-r4-fixtures.txt`), and nothing is touched in either product.

## 3. RED → GREEN

- **Round 1** (`red.txt`, at 3943d667's code): 5 tests fail --
  `ask_refuses_a_profile_fx_refuses`,
  `ask_says_what_the_configuration_could_not_use_and_that_every_tool_is_refused`,
  `ask_with_a_permission_table_that_does_not_read_runs_no_tool`,
  `the_shell_refuses_a_profile_fx_refuses_before_it_writes_anything`,
  `a_profile_fx_refuses_is_refused_before_the_terminal_is_taken`.
- **Round 2** (`red-r2.txt`, round 2's tests over 98d3513b's `src` in a
  throwaway clone): `ask_goes_on_with_a_profile_fx_starts_with` fails at its
  first new fixture, `torn JSON group may write: xfx:
  InvalidProfileConfiguration`, and `ask_refuses_a_profile_fx_refuses` at
  `a hard-linked file` (round 1 went on to the Gateway).
- **Round 3** (`red-r3.txt`, round 3's tests over b209c786's `src` in a
  throwaway clone): `ask_refuses_a_profile_fx_refuses` fails at `a HOME
  nobody may list` (round 2 went on to the Gateway), and
  `the_shell_refuses_a_profile_fx_refuses_before_it_writes_anything` and
  `a_profile_fx_refuses_is_refused_before_the_terminal_is_taken` find an
  xfx that does not exit -- round 2 started with that HOME
  (`xfx-r2-fixtures.txt` `home-unlistable`), and it is the one profile in
  REJECTED that round 2 starts. The descriptor tests call functions round 2
  does not have (`open_settings`, `read_user_layer`); the mutants M19-M22
  stand for their RED.
- **Round 4** (`red-r4.txt`): the zero-link test spliced over 38b891c3's
  `src` in a throwaway clone fails at `assert!(sole_regular(&stat), "a
  snapshot is not unsafe")`, once the line before has found the descriptor
  of a file renamed over after its open with no link left. With round 3's
  `snapshot` put back, `ask_refuses_a_profile_fx_refuses` fails at `a HOME
  nobody may list: the home is listed` (it saw nothing before or after).
  The look-before-open test and the hard-linked file nobody may read stand
  on M27 and M26.
- **Guards**: `the_commands_fx_answers_without_a_launch_answer_with_a_profile_it_refuses`,
  `the_shell_starts_with_a_profile_only_ask_refuses`,
  `a_profile_only_ask_refuses_starts_the_tui`, and
  `a_profile_that_goes_bad_after_the_launch_is_a_panel_row_not_a_refusal`
  (the `/status` row, no refusal, the session ends with 0).
- **Unit tests**: `config::tests::a_launch_refuses_the_models_fx_refuses_and_no_other`
  (each edge of `validateModel`),
  `config::tests::the_refusal_is_read_from_the_profile_and_this_workspaces_entry_only`,
  and `config::tests::the_profile_is_opened_in_fxs_order_before_what_it_holds_counts`
  (every step of `profile_open`: nothing, a HOME nobody may list, torn,
  torn open to group, to other, in a directory open to group, write-only,
  an unlistable directory, a second hard link, a FIFO, a file for the
  directory; the unlistable and write-only cases skip a reader the mode
  bits do not stop), and round 3's
  `config::tests::the_open_checks_what_it_opened_not_what_was_looked_at`
  (the open on its own, the path as it may stand by then: a link is not
  followed, a FIFO is not waited on -- the open runs on a thread, freed by
  a writer opened without waiting; a thread that ends without an answer
  fails at once -- a second name and an open mode are seen on the open
  descriptor) and
  `config::tests::the_profile_layer_is_read_from_the_descriptor_that_was_checked`
  (a torn file renamed into place after the open is not what is read);
  round 4's `config::tests::a_profile_renamed_over_after_its_open_is_read_and_a_linked_one_is_not`,
  `config::tests::a_project_file_is_looked_at_before_it_is_opened` (a
  socket in its place is `malformed_settings`, not an open that fails), and
  the open-order test's `a second hard link nobody may read` (unsafe, not
  an open that fails; skipped for a reader the mode bits do not stop).
- **GREEN** (`green.txt`): all 17 pass in the gate's `cargo test` at
  adaa56de's code; the whole suite there too (section 8). The integration
  fixtures gained, in round 3, an unlistable `~/.xfx` and an unlistable
  HOME in REJECTED and a FIFO in UNSAFE (`tests/support/profiles.rs`, with
  `unlock` so a sandbox can be removed); in round 4, a hard-linked file
  nobody may read in UNSAFE. The interactive UNSAFE loops skip a fixture
  the mode bits cannot make, and `snapshot` lists a directory its owner may
  not list (mode given back for the look, then put back), which the three
  refusal tests check.

## 4. Tests changed to the refusal

Every test that starts xfx with a profile fx refuses was found by grepping
the tests for seeded profiles (`write_user_settings`, `settings.json`,
`with_profile`, `profile(`) and for the diagnostics such a launch printed;
the profiles each seeds were read against the rule.

- `tests/cli.rs` `ask_says_what_the_configuration_could_not_use_and_that_every_tool_is_refused`:
  its `{ this is not json` half now expects `xfx: InvalidProfileConfiguration`,
  exit 1 (its `permission` half is a typed key: section 7).
- `tests/gateway.rs` `ask_with_a_permission_table_that_does_not_read_runs_no_tool`:
  three of its profiles -- an entry `["x"]`, `{ this is not json`, and a
  file nobody can read -- now expect the refusal, no request and no write;
  its other two keep the per-key table refusal.
- Round 2: both helpers that seed those profiles now set the profile
  `0o700` and the file `0o600` whatever the umask, since under a `0o002`
  umask the file would be one fx does not read.
- No other test launched with such a profile: the others seed valid
  profiles, or swap a bad one in after launch (`src/tui/shell.rs`'s
  `/status` tests, `tests/llmux.rs`'s `setup` over a torn file, which `setup`
  never refuses), or load configuration in-process (`RuntimeConfig::load_with`).
- fxcap: `seed_scan.py` → `seed-scan.txt`: of 381 scenarios, 63 seed a
  profile before launch and one seeds a profile fx refuses,
  `p5-panels-badsettings`, already a launch-only capture under `keep`
  (capture.sh writes every seed `0o600` in a `0o700` profile).

## 5. Captures, sweep and ledger

- `fx/`, `xfx/` (round 1, a57ba760) and `xfx-base/` (3943d667) hold
  `p5-panels-badsettings` and `p5-panels-status-badsettings` at 40x120.
  `cells.sh` → `cells.txt` (row 1 left out, no ledger rule): round 1's
  `p5-panels-badsettings` `launch` differs from fx's in one row, 40, against
  6 rows at the base (`cells.txt:1-4`, `:31-44`); the three
  `p5-panels-status-badsettings` snaps differ in the same rows at the head
  and the base. Round 2 changes nothing these seeds reach (a private
  profile): its sweep below runs both scenarios at b209c786.
- **Still different, and the harness's**: each dead pane's last row is
  tmux's `Pane is dead (status 1, <date>)`, the date the run's clock; the
  ledger's `tmux-dead-pane` normalization masks it. The refusal's own line
  is on stderr, which capture.sh does not put in the pane, so neither
  product's text is in the pair: `fx-fixtures.txt` and
  `xfx-head-fixtures.txt` carry it.
- **No sweep in round 4**: no scenario reaches a changed branch. A
  zero-link file is a race no capture makes; every profile seed is a
  private regular file with one link (`seed-scan.txt`, capture.sh), which
  the look before the open lets through to the same open and read;
  `O_NOCTTY` and the blocking read change nothing a regular file gives
  back. The project layer, the other one round 4 touches: no scenario
  seeds or names a `.xfx.json` or `.fx.json` (`grep -lE '\.x?fx\.json'`
  over `scenarios/*.cap` and `*.json`: 0 files; `find scenarios -name
  .xfx.json -o -name .fx.json`: none), nothing under `scenarios/` is a
  link, a FIFO or a socket (`find scenarios ! -type f ! -type d`: 0), and
  capture.sh makes no link or FIFO (no `ln`, `mkfifo`, `mknod`).
- **Sweep, round 3** (`sweep.sh` under the suite lock → `sweep-r3.txt`;
  each side's `summary.md` in `sweep-r3-head/`, `sweep-r3-base/`): the same
  selection, once with 02c32c3f's xfx and once with 3943d667's. 83
  scenarios, 523 pairs, every run `ran`; the oracle's sha256 the pin's
  before and after. Head 278/523, base 277/523; **moved to passing 1**
  (`p5-panels-badsettings` `launch`, normalized-identical); **dropped 0**;
  `p5-panels-status-badsettings` the same three verdicts on both sides; one
  passing pair moved on the run's clock (`settings-collapse-open`
  `after-esc`, `3s` against `4s`, masked by the ledger).
- **Sweep, round 2** (`sweep.sh` under the suite lock → `sweep-r2.txt`):
  every `p5-panels*`, `settings*`, `startup*` and `config*` scenario and
  every one that seeds a profile, with HEAD's scenarios and ledger, once
  with b209c786's xfx and once with 3943d667's. 83 scenarios, 523 pairs,
  every run `ran` on both sides; the oracle's sha256 the pin's before and
  after.
  - Head 278/523 passing, base 277/523; **moved to passing: 1**,
    `p5-panels-badsettings` `launch` (unresolved-defect → normalized-identical,
    its only difference row 40 under `tmux-dead-pane`); **dropped from
    passing: 0** (`sweep-r2.txt`; each side's `summary.md` in
    `sweep-r2-head/`, `sweep-r2-base/`).
  - `p5-panels-status-badsettings`: the same three verdicts at the head and
    the base (approved-divergence, -cascade, approved-divergence).
  - Three other pairs moved between two passing verdicts
    (`p5-panels-permissions` `revoke-done`, `revoke-prompt`;
    `settings-collapse-open` `after-esc`): each differed in one row, a
    turn's or an activity's clock (`3s` against `4s`), which the ledger
    masks -- the run's clock, not this change (`sweep-r2.txt`, last lines).
- **Sweep, round 1** (`sweep.txt`, a57ba760 against 3943d667): 83
  scenarios; head 278/523, base 277/523; moved to passing 1
  (`p5-panels-badsettings` `launch`), dropped 0; `rules-read` `granted`
  moved between two passing verdicts on the run's wall clock.
- **Ledger**: the cause `profile-refuse-at-launch` is retired; a comment in
  its place says what it was and points here. The two scenarios' comments
  say both products refuse now.
- **Parity**: `docs/parity.md`'s `config diagnostics` row says the refusal,
  fx's read order, its conditions and entry points, and each part of fx's
  rule not copied.
- **References** (`refs.py` → `refs.txt`): 10 `path:N` references into the
  files round 1 changed had moved and were re-pointed by text; at the head
  52 are checked, 0 moved. Of the 4 bare `<name>:N` it lists for a hand,
  none names a file this unit changed at a moved line: `mod.rs:346` is
  `src/tools/mod.rs`, `interactive.rs:457-464` is `src/interactive.rs`
  (the line above this unit's), and `cli.rs:311-312` names `src/cli.rs`,
  which this unit does not touch (already stale at the base).

## 6. Mutants (`mutants.py` → `mutants.txt`)

On a `git clone --no-local` of 419cd3a3 (code adaa56de) outside the
worktree, each test bounded at 900 s, under the suite lock. **27 of 27
killed**, every kill an assertion's panic location, none a timeout. Round
4 moved the texts of M16, M19-M21 and M23 to the new source (`<= 1`,
`O_NOCTTY`, the blocking guard) without changing what each breaks:

- each condition dropped -- the unreadable/malformed/too-large file (M1),
  `workspaces` or the entry not an object (M2), the profile's `model` (M3),
  the entry's `model` (M4) -- killed by `ask_refuses_a_profile_fx_refuses`
  and, but M3, a config unit test;
- fx's model rule loosened (M5, a blank allowed), its `skill_match_fuzzy`
  order dropped (M6), the project file counted (M12): killed;
- the unsafe path not refusing `ask` (M7), and refusing the interactive
  launches too (M8): killed, M8 by the two pty tests that start with it;
- the refusal after the terminal is taken (M9: moved into `session`, after
  raw mode), the line shell's after its store is opened (M10), `ask --json`
  writing it on stderr (M11): killed;
- fx's read order, on descriptors (round 3): the file's mode not checked
  (M13) and the directory's (M14) -- killed by
  `ask_goes_on_with_a_profile_fx_starts_with` and a unit test; a failed open
  taken as a file not read (M15, `0o222` and `0o000`), a second hard link
  no longer unsafe (M16), a `~/.xfx` that does not open (M17) or a HOME
  that does not open (M18) taken as no profile, `~/.xfx` opened following
  a link (M24) -- killed by `ask_refuses_a_profile_fx_refuses` and, but
  M24, a unit test;
- the open itself: the file opened following a link (M19), opened waiting
  on a FIFO (M20: the open-on-a-thread test reports `the open waited on a
  FIFO`, frees the thread with a writer opened without waiting and fails
  in seconds), the open descriptor not fstat'ed (M21), all killed by
  `the_open_checks_what_it_opened_not_what_was_looked_at`; the regular-file
  check dropped (M23: a FIFO is then read and found malformed, so
  Rejected -- `ask` refuses it either way, so its killers are that unit
  test and the two interactive starts with an UNSAFE profile,
  `the_shell_starts_with_a_profile_only_ask_refuses` and
  `a_profile_only_ask_refuses_starts_the_tui`); the layer read again by
  its path (M22), killed by
  `the_profile_layer_is_read_from_the_descriptor_that_was_checked`;
- round 4: a file with no name left unsafe again, `== 1` put back (M25),
  killed by `a_profile_renamed_over_after_its_open_is_read_and_a_linked_one_is_not`;
  the look before the open deleted (M26: a hard-linked file nobody may read
  becomes an open that fails, Rejected), killed by the open-order test's
  `a second hard link nobody may read` and the two interactive UNSAFE
  starts; another layer opened before it is looked at (M27: a socket
  there becomes `unreadable_settings`), killed by
  `a_project_file_is_looked_at_before_it_is_opened`.

xfx has no auth mode -- no `FX_AUTH_MODE`, no host-managed launch -- so the
brief's "refusal in a non-local auth mode" has no counterpart to mutate. M8
breaks the one mode split fx makes inside the local mode: an unsafe path
refused by the interactive launches too, which fx's stored-credential
launch does not do.

## 7. Not copied, and what is decided about each

- **(a) A key of the wrong type** (`"fast_mode": "yes"`, `"permission":
  {"bash": "perhaps"}`): fx drops the whole profile and refuses; xfx keeps
  its per-key diagnostic. **Decided by the dispatcher** (reversible, for
  the user's veto): copy fx's typed-key refusal for the keys both products
  read, in a separate unit, **PROFILE-REFUSE-TYPED-KEYS**; keys only xfx
  reads keep their per-key diagnostic.
  - **A key written twice** (`dup-key`): fx's JSON parse refuses the file
    (`fx-fixtures.txt`, every entry); serde_json keeps the last value and
    xfx starts (`xfx-head-fixtures.txt`). Recorded, as the brief asked, and
    put with PROFILE-REFUSE-TYPED-KEYS: both are what fx's parser rejects and
    xfx's takes.
- **(b) A bad `provider` or `models`** (recorded decision): fx stops with
  the parse error's name (`fx: UnknownConfiguredProvider`,
  `fx: InvalidModelValue`) at every surface that loads settings, `status`
  too. xfx refuses the turn for a bad `provider` (`provider=rejected`), and
  drops a bad `models` entry **silently**: no config line, and the turn
  uses the model it would without it (`xfx-head-fixtures.txt`
  `models-bad`). Queued: **MODELS-ENTRY-DIAGNOSTIC**, a config diagnostic
  for a dropped `models` entry. The provider sets differ (`llmux` is
  xfx's alone).
- **(c) An unsafe profile in an interactive launch** (recorded decision):
  fx goes on with model requests blocked, and never opens a linked
  settings file. xfx goes on with a linked settings file, a hard-linked
  one, a FIFO or a directory in its place, with no block -- and with a
  linked file it **reads the link's target and applies its settings**
  (`link-settings` starts like `valid`); a hard-linked file nobody may read
  is `config user: unreadable_settings` there, and every tool refused
  (`hardlink-unreadable`). With a linked `~/.xfx` or a file in its place
  its session store refuses the launch in its own words, a rule older than
  this unit (`src/profile_home.rs`).
- **A linked project file** (queued: **LAYER-LINK-POLICY**): fx reads
  `.fx.json` through the same open (`readOptionalFile`,
  `config_runtime.zig:1415-1429`; `io.zig:170-180`), so a link or a second
  hard link there is `durable_path_unsafe` and the layer is not read
  (`config_runtime.zig:525-535`). xfx follows the link and reads a
  hard-linked file, as at round 2; round 4 only restores the look before
  the open, so nothing but a regular file is opened.
- **A profile open to group or other** (queued: **PROFILE-WRITABLE-IGNORED**):
  fx ignores its settings (the layer absent) and says
  `config user: private_state_permissions_unsupported` (`fx-fixtures.txt`
  `group-writable`); xfx says nothing and reads and applies them, as it did
  at the base. The interactive launches and a saved `ask` are stopped by
  the session store's rule for a `~/.xfx` open to group or other, but
  `ask --no-save` opens no store, so it runs a turn under the settings of a
  `~/.xfx` another account may write to (`provider`, the permission tables,
  `permission_mode`). Only the refusal is copied here.
- `FX_AUTH_MODE` and `FX_ULTRAFAST` have no xfx counterpart.
- Seen in the oracle runs and older than this unit: fx's `status` writes
  `fx: config user: malformed_settings` on stderr for a refused profile
  (`fx-entries.txt`); xfx's `status` writes nothing there. fx's interactive
  launch with a file where `~/.fx` belongs fails later with `fx: NotDir`
  (`fx-fixtures.txt` `profile-file`).

## 8. Gate (`gates.sh` under the suite lock → `gate.txt`)

At 419cd3a3 (code adaa56de), the tree clean before and after, one hold of
the lock: **22 steps, 0 failed** -- fmt, both clippys, `cargo test` (5669
passed, 0 failed) and `--features fault-injection` (5706 passed, 0
failed), the identity, stubs, secrets and local-path checks and their
self-tests, the preview contract, the preflight cost, the usage clock,
`test-gate-env`, the fxcap harness tests, both release builds, `smoke.sh`
(47 checks) and `smoke-tui.sh` (37 scenarios and the oracle, 1167 checks,
0 failures). Rounds 1-3 (c782a6d8, 41ac3864, 7b72e92b; 22/22 each) are in
this file's history.

## Files

`fx_src.sh` → `fx-src.txt` · `probe.sh` → `fx-fixtures.txt`, `fx-entries.txt`,
`fx-r4-fixtures.txt`, `xfx-head-fixtures.txt` (adaa56de),
`xfx-head-entries.txt`, `xfx-r3-fixtures.txt` (02c32c3f),
`xfx-r2-fixtures.txt` (b209c786), `xfx-r1-fixtures.txt` (a57ba760),
`xfx-base-fixtures.txt` (3943d667) · `outcomes.py` → `outcomes.txt` ·
`red.txt`, `red-r2.txt`, `red-r3.txt`, `red-r4.txt`, `green.txt` · `seed_scan.py` → `seed-scan.txt` ·
`refs.py` → `refs.txt` · `fx/`, `xfx/`, `xfx-base/`, `cells.sh` →
`cells.txt` · `sweep.sh`, `compare.py` → `sweep.txt`, `sweep-r2.txt`,
`sweep-r3.txt`, `sweep-head/`, `sweep-base/`, `sweep-r2-head/`,
`sweep-r2-base/`, `sweep-r3-head/`, `sweep-r3-base/` ·
`mutants.py` → `mutants.txt` · `gates.sh` → `gate.txt`
