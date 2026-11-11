# PERMISSION-MODE-CASE

fx v0.0.16 reads every permission mode a person writes with one parse: `ask`,
`auto`, and `full-access`, `full access` or `yolo` for full access, in any
case, not trimmed. xfx read its settings key and `XFX_PERMISSION_MODE`
exactly, so `"permission_mode": "ASK"` was dropped with an `invalid_value`
line and xfx ran its default, `auto`: a looser mode than the one written
(PROFILE-REFUSE-TYPED-KEYS r1-opus SHOULD 4; its `fx-edge.txt`, `mode-upper`).
This unit copies fx's parse at every surface that reads a mode, fx's
`--full-access` flag, and fx's word for full access on the `status` and
`doctor` lines.

- **Base** 9c64ddd3 (PROFILE-REFUSE-TYPED-KEYS's closed head). **Branch**
  `fix/p5-permission-mode-case`.
- **Code** 15d36581 (the parse, the flag) and **9b37d774** (the words on the
  lines). 10c45b17 changes tests only; 052fd2c9 to d708ffb8 are docs,
  scenarios and this receipt. **Round 2** (§9): code **5891efea**, then
  83ed41a0 (docs and the receipt's scripts) and the receipt.
- The probes and captures ran binaries built at 9b37d774 (head) and 9c64ddd3
  (base): `build.txt`, each binary's sha256 and the `build_revision=` its
  `status` reports. The gate, RED and mutants ran at the heads their files
  name.
- **Source outside tests** (`count.py` → `count.txt`): 51 added lines, 23
  of them comments.
- **Refs** (`refs.py` → `refs.txt`): nothing this unit moved; the one
  by-hand reference, `parity-ledger.toml`'s `docs/parity.md:96`, is still the
  `/permissions` row, which this unit extended.

## 1. fx's rule (`fx_src.sh` → `fx-src.txt`, every range checked: 0 out of bounds)

- **The parse** (§1): `PermissionMode.parse` (`types.zig:2375-2382`)
  compares with `eqlIgnoreCase` against `ask`, `auto`, `full-access`,
  `full access`, `yolo`; nothing is trimmed. fx's own table
  (`config_runtime.zig:3149-3162`) reads `ASK`, `Auto`, `YOLO`,
  `Full Access`, and refuses `full`, `" full-access"`, `"full-access "`.
  `parsePermissionMode` (`:1057-1059`) is the one wrapper.
- **Every reader** (§2), each through `parsePermissionMode`:
  - the settings key, in the global object and this workspace's entry
    (`config_runtime.zig:1688-1692`); a value that does not parse is
    `InvalidPermissionMode`, so the layer fails (TYPED-KEYS: the launch
    refuses);
  - the startup status record (`:1497-1500`, `:1563-1569`);
  - `FX_PERMISSION_MODE` (`app_lifecycle.zig:1556-1560`, `doctor_runtime.zig:366-370`):
    `getenv`, untrimmed; one that does not parse leaves the configured mode;
    used by the status snapshot (`app_lifecycle.zig:579`), the interactive
    launch (`:768`) and doctor (`doctor_runtime.zig:223`);
  - the settings validator before a write (`settings_store.zig:1932-1936`).
- **`/permissions`** (§3): the **same parse**, after the router trims the
  line of spaces and tabs (`command_router.zig:92-94`, `:114`;
  `app_commands.zig:586-596`; `session_commands.zig:464-489`); `full` is
  refused there as everywhere, and the settings menu's mode goes the same way
  (`app_commands.zig:3394`).
- **The CLI flag** (§4): its **own grammar**, exact (`std.mem.eql`):
  `--auto`, and `--full-access` or `--yolo` for full access; a second mode
  flag of any kind is `InvalidAskArgs` (`cli_ask.zig:3579-3584`, its test
  `:4994-5012`, help `commands.zig:35-37`).
- **No parse** (§5): Shift-Tab cycles a mode value (`app_permission_runtime.zig:127-142`);
  the six approval files name no `PermissionMode`, `parsePermissionMode` or
  `toggleMode` (0 lines each).
- **What fx writes back** (§6): a saved mode is `@tagName` -- `ask`, `auto`,
  `yolo` -- never the spelling read (`settings_store.zig:1053`;
  `app_permission_runtime.zig:233-243`). It **says** full access as `full
  access` (`permissionModeDisplayLabel`, `permissions.zig:109-115`) on the
  `/status` row (`output_contracts.zig:765`), the `[status]` line (`:839`),
  the `[doctor]` line (`:1806`) and doctor's startup check
  (`doctor_runtime.zig:237-240`, `:529-535`); its `--json` says `yolo`
  (`output_contracts.zig:944-945`, `:1851-1852`).

Run against the oracle (`probe.sh` → `fx-probe.txt`; FX_AUTO_UPGRADE=0, the
sha256 `8e6cda2efddb` checked before and after): every row of §2 above holds
-- e.g. `key-upper-ask` is `ask` at `permissions`, `status`, `--json`,
`doctor` and its startup check; `env-full-access-space` is `full access`
(`yolo` in `--json`); `key-padded`, `key-full`, `key-invalid` refuse at
`permissions` and `ask` (`InvalidProfileConfiguration`) while `status`
and `doctor` report `auto`; `flag-upper-full-access`, `flag-cap-yolo` and both
two-flag fixtures are `usage: fx ask ...`.

## 2. xfx, surface by surface (head)

| Surface | fx's parse there | xfx before | xfx now |
|---|---|---|---|
| settings key, global and entry (`parse_layer`, `src/config.rs:1730`) | the shared parse | `PermissionMode::parse`, exact (trimmed) | `parse_choice`, fx's parse |
| `XFX_PERMISSION_MODE` (`parse_environment`, `:2021-2036`) | the shared parse, untrimmed | exact, trimmed | `parse_choice` on the value as set; blank still ignored |
| status record (`StatusSnapshot`, from `RuntimeConfig`) | the shared parse | as the two above | as the two above |
| launch refusal (`fx_first_error_refuses`, `:2375`) | the shared parse | its own copy (`fx_permission_mode`) | `parse_choice`; the copy deleted |
| `/permissions` (`slash::Request::parse`, `src/permission/slash.rs:327`) | the shared parse after a trim | `parse_choice`, which trimmed | unchanged: the line trimmed here, then `parse_choice` |
| `ask` flags (`src/cli.rs`'s `RawCommand::Ask`) | its own: `--auto`, `--full-access`, `--yolo`, exact | `--auto`, `--yolo` | `--full-access` added (round 1 as clap's alias; round 2 its own flag, so `--help` prints fx's two lines) |
| session log (`PermissionMode::parse`) | -- (xfx's own record) | exact | unchanged: xfx's wire |
| a settings write's result (`provider/profile.rs`'s `write_preference`) | fx's validator over the whole candidate (`validateCandidate`, `settings_store.zig:536-539`, `:1849-1859`, `:1932-1936`) | no check | **not copied** (round 2, §9) |

`PermissionMode::parse_choice` (`src/config.rs:408`) is fx's parse word for
word. **What xfx writes back** is unchanged and is fx's: the wire word
`mode.label()` (`src/tui/worker.rs:2082-2084`, `src/interactive.rs:2373-2376`),
`ask`, `auto` or `yolo`, whatever was read -- the capture
`sweep/head/xfx-p5-mode-case-key-40x120-full-access-upper.files` holds `"yolo"` after
`/permissions FULL-ACCESS`, as fx's does. **What it says** now follows fx:
`full access` on the `[status]` and `[doctor]` lines and the startup check
(`display_mode`, `src/output.rs:159`; `src/app.rs:1010`), `yolo` in `--json`.

## 3. RED → GREEN

- **The probe** (`outcomes.py` → `outcomes.txt`): 35 fixtures, every surface
  with `ASK`, `Auto`, `FULL-ACCESS`, `Full Access`, `YOLO`, `yolo`, an
  invalid word, `" ask"` and `full`, plus 8 flag spellings. **Base
  (`xfx-base-probe.txt`): 14 of 35 report fx's mode at every entry.** E.g.
  `key-upper-ask`: fx `ask`, xfx `auto`; `env-full-access-space`: fx full
  access, xfx `ask`; `env-padded`: fx `auto`, xfx `ask` (trimmed);
  `key-lower-yolo`: fx `full access`, xfx `yolo` on the lines;
  `flag-full-access`: fx ran in full access, xfx's parser refused.
  **Head (`xfx-head-probe.txt`): 35 of 35.**
- **Tests** (`red.sh` → `red.txt`, the head's tests over the base's src on a
  clone): `a_mode_in_any_of_fxs_spellings_is_the_mode_that_runs`
  (`"ASK"`: left `auto`, right `ask`), `full_access_is_said_in_fxs_word_and_recorded_in_the_wire_word`,
  `ask_takes_fxs_two_spellings_of_full_access_exactly` (`unexpected argument
  '--full-access'`) and `ask_help_advertises_only_the_implemented_flags`
  fail; the two older mode tests pass on both. The unit tests inside `src/`
  (`config::tests::a_mode_is_read_with_fxs_parse_wherever_a_person_writes_one`,
  `a_typed_mode_reads_upstreams_words_in_any_case`, `cli::tests::the_permission_flags_select_a_mode_and_cannot_be_combined`,
  `permission::slash::tests::an_argument_is_read_in_upstreams_order_and_case`)
  cannot run over the old src; the mutants stand for them (§4).
- **Captures** (the sweep, §5): base 0 of 7 pairs pass, head 5 of 7, the
  other 2 being `p5-mode-case-full`'s, a recorded difference (§8).

## 4. Mutants (`mutants.py` → `mutants.txt`)

On a `git clone --no-local` throwaway (its head c2fb8b89, whose `src/` and
`tests/` are 10c45b17's), one edit each, then the tests that must catch it:
**14 mutants, 14 KILLED**.

- **One per surface**: M1 the settings key back to the exact wire parse
  (killed by the unit test and the CLI test); M2 `XFX_PERMISSION_MODE` back
  to it; M3 the variable trimmed again (both tests: `" ask"` lies over
  `auto`, so a trim would apply `ask`); M4 the launch refusal's check back to
  the wire parse (the refusal table: `"ASK"` would refuse); M5 `ask` without
  `--full-access` (the flag unit test, the CLI test, the help test); M6, M7,
  M8 the `[status]` line, the `[doctor]` line and the startup check back to
  `yolo`; M14 `/permissions` no longer trimming its line (the slash test).
- **One per alias**, in the parse they all share: M9 no `full-access`, M10
  no `full access`, M11 no `yolo`, M12 compared with case, M13 trimmed
  (each: the parse's table and the CLI test).

The queued run waited about 72 minutes for a slot of the lock; at the
dispatcher's word it was stopped and run again with `SUITE_LOCK_SLOTS=4`,
the same arguments and TMPDIR.

## 5. Sweep

`scripts/fxcap/sweep.sh --only 'p5-mode-case-*'` under the lock, fx v0.0.16
(sha256 checked before and after) against each xfx binary; scoreboard, summary
and per-pair verdicts in `sweep/head/` and `sweep/base/`.

- **Head (xfx at 9b37d774; `sweep/head/verdicts.txt`): 5 of 7 pairs
  `normalized-identical`** -- `p5-mode-case-key` `start` (a profile saying
  `ASK`: the hint row says `ask · model-a` on both), `full-access-upper`,
  `auto-cap`, `full-access-space`, and `p5-mode-case-env` `start` (each
  product's variable set to `FULL-ACCESS`: `full access` on both, after the
  warning's four seconds). The normalisation is the sweep's own (the product
  name in the warning, `fx` and `xfx`, its column taken from the padding).
  The other 2, `p5-mode-case-full` `typed` and `full`, are
  `unresolved-defect`: fx's argument menu (§8). No ledger rule covers them;
  none is added, since the difference is not one outside xfx.
- **Base (xfx at 9c64ddd3; `sweep/base/verdicts.txt`): 0 of 7.** Every
  `p5-mode-case-key` pair differs at row 3, `* config: 1 configuration issue`
  (the `invalid_value` for `ASK`), and its `start` says `auto · model-a`
  (`sweep/base/xfx-p5-mode-case-key-40x120-start.txt`); `p5-mode-case-env`
  **failed**: `XFX_PERMISSION_MODE=FULL-ACCESS` did not read, the run stayed
  in `auto`, and its `until shown Full access` failed the run after 30 s, as
  capture.sh's `until` does (`sweep/base/env-capture-tail.txt`).
- **What each saves** (`sweep/head/*-full-access-upper.files`): both
  profiles hold `"permission_mode"` `"yolo"` after `/permissions FULL-ACCESS`.

**TYPED-KEYS's probe, again** (`tk_same.sh` → `tk-sweep.txt`): its 20
fixtures that write `permission_mode`, xfx at base (`tk-base.txt`) and head
(`tk-head.txt`), same order. 4 lines differ, all `mode-upper`'s: its `ask`
and line shell no longer print `config user: invalid_value;
key=permission_mode`. Against TYPED-KEYS's fx probes both are 20 of 20 on
refusal and 18 of 20 on how each entry ended -- the two left
(`link-profile`, `dir-group-writable-torn`) are TYPED-KEYS's own, unchanged.
`tk_same.sh` is TYPED-KEYS's `same.sh` reading past stdout's byte count too:
the two runs' roots differ in length, and `status` prints the workspace
path.

## 6. Gate (`gates.sh` under the suite lock → `gate.txt`)

At **052fd2c9** (code 9b37d774 and the parity, scenario and receipt files
before this README), every CI step the brief lists, one hold of the lock:
**22 steps, 0 failed**, the tree clean before and after (`gate.txt`). The
plain `cargo test` passed 5677, the fault-injection one 5714, none failed --
no catalog-wait failure to rerun. The unit's tests, and the two older mode
tests beside them, from that log: `green.txt`.

## 7. Parity and records

- `docs/parity.md`: `config diagnostics` (the queued line closed; the rule
  stated at the grammar sentence), `environment overrides`, `ask`, `status`,
  `doctor`, `/permissions`, `tool permission modes`.
- PROFILE-REFUSE-TYPED-KEYS's README §7: its `"permission_mode": "ASK"`
  bullet now points here; PERMISSION-MODE-CASE is no longer an owner.

## 8. Not copied (recorded)

- **fx's argument completion** (`full/`, `p5-mode-case-full`): typing
  `/permissions full`, fx's composer shows its argument menu (`full-access`)
  and Enter takes it, so fx sets full access and saves the history line as
  `/permissions full-access` (`full/fx-p5-mode-case-full-40x120-end.files`;
  the menu under the typed word, `full/fx-p5-mode-case-full-40x120-typed.txt`); xfx
  has none (the `full-screen TUI` row), so `full` is the usage, which is fx's
  parse's own answer. The same completion is why fx's history says
  `full-access` and `auto` for a typed `FULL-ACCESS` and `Auto`, where
  xfx's keeps what was typed (`sweep/head/*-key-40x120-end.files`); the
  mode, the notice and the screen are the same on both.
- **The headless warning**: `xfx ask --yolo` (or `--full-access`) prints
  xfx's own sentence on every run; fx prints `Full access enabled: fx
  permission checks disabled` until it has saved `yolo_acknowledged`
  (`cli_ask.zig:1486-1488`, `:3675-3699`). Older than this unit; recorded in
  the `tool permission modes` row.
- **The diagnostic**: an `XFX_PERMISSION_MODE` or settings value that does
  not read still adds xfx's `invalid_value` line, which fx does not print;
  the applied mode is fx's.

## 9. Round 2 (reviews r1-gpt6 and r1-opus of d708ffb8, no MUST)

Code 5891efea; the NITs that left a doc untrue or a difference unrecorded.

- **NIT 1, the trim.** `parse_choice`'s doc (`src/config.rs:401-405`) and the
  slash test now say it: xfx trims a `/permissions` line of every Unicode
  blank (`str::trim`; `src/interactive.rs`'s `classify`,
  `src/permission/slash.rs`'s `Request::parse`), fx's router of space and
  tab only (`command_router.zig:92-94`, `fx-src.txt` §2). So `/permissions
  yolo` ending in a no-break space sets full access here and is the usage in
  fx. Behaviour unchanged; recorded in the `/permissions` row.
- **NIT 2, `ask --help`.** The flag is two clap flags now, `--full-access`
  then `--yolo`, so `--help` prints fx's two lines (`help.txt`: fx, then xfx
  before and after); the parse is the same -- any two mode flags refused
  (`src/cli.rs`'s flag table: `--full-access --full-access`, `--yolo
  --full-access`, `--yolo --auto` added) -- and the probe at the round-2
  head is still 35 of 35 (`xfx-head-probe-round2.txt`, `outcomes.txt`;
  `build.txt`: `build_revision=83ed41a00b31`). `--auto`'s own description
  stays xfx's. Pinned by `ask_help_advertises_only_the_implemented_flags`.
- **NIT 3, fx's check of what a write leaves: not copied.** Every fx
  settings write parses the file it would leave and refuses it when a known
  key, `permission_mode` among them, is one its parse refuses
  (`validateCandidate`, `settings_store.zig:536-539`, `:1849-1859`;
  `fx-src.txt` §2). xfx's writer checks no key of what it leaves -- the
  class of the key-twice write TYPED-KEYS recorded -- and a check of the
  mode alone would be half of fx's. Recorded in `config diagnostics`.
- **NIT 4.** `scripts/fxcap/capfile.py`'s launch arm reads `full access`
  too; a race case for `XFX_PERMISSION_MODE=FULL-ACCESS` in `test_sweep.py`.
- **NIT 5.** The trim has its own test,
  `the_line_is_trimmed_before_the_modes_parse_which_trims_nothing`, which M14
  now names; M15 covers the blank-variable filter; RED is rerun with the
  final tests (`red-round2.txt`). Mutants: `mutants-round2.txt`.
- **NIT 6.** The `tool permission modes` row adds the side effect: fx's
  headless run saves `yolo_acknowledged`, so its TUI's warning stops after an
  `fx ask --full-access`; xfx's `ask` saves nothing.
- **NIT 7.** Doctor's own `permissions` check (fx's doctor has none,
  `doctor_runtime.zig` at the pin) says `mode=full access`, the word of the
  two lines beside it (`src/app.rs:1139`; pinned in
  `full_access_is_said_in_fxs_word_and_recorded_in_the_wire_word`).
- NIT 8 (the xfx-only `[shell]` and `[session]` lines keep `yolo`) stays.

**Results, round 2.** RED with the final tests (`red-round2.txt`): the same
four fail over the base's src, the two controls pass. Mutants
(`mutants-round2.txt`): **18 of 18 KILLED**, M14 by its new test, M15 by
the blank-variable assertion. Gate on a `git clone --no-local` at 83ed41a0
(`gate-round2.txt`): **22 steps, 0 failed**, tree clean before and after;
`cargo test` 5678 passed, fault-injection 5715, none failed. The sweep's
scenarios are TUI-only and the round changed no TUI code, so they were not
rerun.

### Open, for the train join (record only)
- When this line joins SLASH-ARG-SKILLS, `docs/parity.md:96` (the
  `/permissions` row's argument-completion clause) and §8's first bullet
  change: Enter on `/permissions full` would then take the menu's
  `full-access`.
- The two `p5-mode-case-full` snaps then belong to the slash line (a cause
  of kind `b`, owner SLASH-ARG-SKILLS).
- Re-sweep `p5-mode-case-full` on the joined head.

## Files

- fx: `fx_src.sh` → `fx-src.txt`; `fx-probe.txt`.
- The probe: `probe.sh`, `outcomes.py` → `outcomes.txt`; `xfx-base-probe.txt`,
  `xfx-head-probe.txt`; `build.txt`.
- RED: `red.sh` → `red.txt`. Mutants: `mutants.py` → `mutants.txt`.
- Captures: `sweep/head/`, `sweep/base/` (scoreboard, summary, verdicts,
  the key and env pairs at head, base's key start and env failure), and
  `full/` (`p5-mode-case-full`, fx and head xfx by `capture.sh`); the
  scenarios are `scripts/fxcap/scenarios/p5-mode-case-{key,env,full}.cap`.
- TYPED-KEYS's probe, again: `tk-base.txt`, `tk-head.txt`, `tk_same.sh` →
  `tk-sweep.txt`.
- Round 2: `help.txt`, `xfx-head-probe-round2.txt`, `red-round2.txt`,
  `mutants-round2.txt`, `gate-round2.txt`.
- Gate: `gates.sh` → `gate.txt`, `green.txt`. Size and refs: `count.py` →
  `count.txt`, `refs.py` → `refs.txt`.
