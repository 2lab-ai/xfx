# flake-status-s1: the status-panel test that read its own session id in a path

`output::tests::the_panels_session_and_ultrafast_change_nothing_the_status_command_prints`
failed once on CI. The cause is in the test, not in `xfx status`. The fix
changes that test only (`src/output.rs`). Old line numbers are at `main` @
`88c7a5be`; new ones are at this branch's commit.

## The failure

- PR #66's CI, job `gate (aarch64-unknown-linux-gnu)`, step "Tests, with the
  fault injector", 2026-11-06 22:47Z: 3988 passed, 1 failed. PR #66 does not
  touch `src/output.rs`.
- It panicked at `src/output.rs:2056`:
  `assertion failed: !shown.render_json().contains("s1")`. The two
  assertions before it, `render_text` and `render_json` equal between the
  snapshot with a session and the one without, had passed.

## The cause

The test set the panel's session to `ShellSession::Open { id: "s1", .. }`
and then asked that `s1` not appear in the JSON. The JSON was equal to the
snapshot without a session, so the `s1` came from another field.

`StatusSnapshot`'s JSON at `88c7a5be`, printed by a scratch test on a
throwaway clone with `TMPDIR=/private/tmp/claude-501/fs1`:

```
{"kind":"status","model":"zai/glm-5.2","provider":"gateway","build_channel":"debug",
 "build_revision":"<12 hex>","auth":"AI_GATEWAY_API_KEY","auth_refreshable":false,
 "permission_mode":"auto","sandbox":"none",
 "workspace":"/private/tmp/claude-501/fs1/.tmpLWAWbk",
 "history_turns":0,"session_permission_grants":0,"agent_step_limit":25}
```

- `workspace` is `keyed_config()`'s `tempfile::tempdir()`
  (`src/output.rs:1796-1797`), shown as `config.workspace_root`
  (`src/output.rs:248`). tempfile 3.27.0 names it `.tmp` and six characters
  from `[A-Za-z0-9]` (`src/lib.rs:195`, `src/util.rs:15`).
- `build_revision` is hex (`src/build_meta.rs:59-65`) and cannot hold an `s`.
  Every other field is fixed by the test's configuration.
- So on CI, where the directory is `.tmp` and six random characters under
  the system temp, the random part spells `s1` in about 5 runs in 3844,
  about once in 770.

**Reproduced.** On the throwaway clone at `88c7a5be`, with
`TMPDIR=/private/tmp/claude-501/fs1`:
`panicked at src/output.rs:2056:9: assertion failed: !shown.render_json().contains("s1")`.
With `TMPDIR=/private/tmp/claude-501/fx0` the same test passes. The gate
`TMPDIR` the common brief names for this unit,
`/private/tmp/claude-501/flake-status-s1`, also holds `s1`: at `88c7a5be`
this test fails under it on every run (table below).

## The fix

`src/output.rs:2045-2071`. The test's claim is unchanged: the panel's
session and Ultrafast change nothing the `status` command prints.

- The id is `session-id-under-test-7f3a` (`:2050`). It has dashes, which are
  in neither a tempfile name nor a hex revision, and is 26 characters long.
- The two equalities stay (`:2059-2060`). **They are what catches a leak**: a
  snapshot without a session cannot print an id, so a JSON equal to its JSON
  holds none. On the old code the third assertion could fail only by the
  collision.
- The JSON is then parsed (`:2061-2070`): it must not hold the id, a `session`
  key or a key with `ultrafast` in it. The keys are matched whole or by
  `ultrafast`, because `session_permission_grants` is a real key.

## RED and GREEN (throwaway clone)

All on a `git clone --no-local` of the worktree in the scratchpad. Nothing
ran against the worktree but the gates.

| code | TMPDIR | result |
|---|---|---|
| `88c7a5be` | `/private/tmp/claude-501/fs1` | FAILED at `src/output.rs:2056` |
| `88c7a5be` | `/private/tmp/claude-501/flake-status-s1` | FAILED at `src/output.rs:2056` |
| `88c7a5be` | `/private/tmp/claude-501/fx0` | ok |
| fixed | `/private/tmp/claude-501/sw-s1` | ok |
| fixed | `/private/tmp/claude-501/flake-status-s1` | ok |
| fixed | `/private/tmp/claude-501/fx0` | ok |
| fixed + leak mutant | `/private/tmp/claude-501/fx0` | FAILED at the `render_json` equality |

The leak mutant made `StatusSnapshot::render_json` put
`"session":"<id>"` first in the document when the session is open (it
compiles; `ShellSession` has no `Serialize`, so removing `#[serde(skip)]`
would not). The new test failed with
`left: "{\"session\":\"session-id-under-test-7f3a\",\"kind\":\"status\",...`.

## The sweep

`negscan.pl` (here) lists negated `.contains(...)` of a literal of 1 to 4
characters in `src/` and `tests/`, across lines: 75 hits. 32 are `'\n'`,
`'\r'`, `'\t'` or `'\0'`, which no path, revision, time or pid holds. Of the
other 43:

**Not assertions on output, left.** Production code:
`src/markdown/code_highlight_languages.rs:1080`,
`src/permission/command.rs:155`, `src/skills/catalog.rs:449`,
`src/skills/contract.rs:293`, `src/skills/invocation.rs:1393`,
`src/skills/mod.rs:97`. Waits until a value appears:
`src/mcp/transport.rs:2266`, `src/tui/worker.rs:7403`. Constants:
`src/tui/term.rs:968`, `:970`. Fixed inputs: `src/process.rs:939` (the
crate's own source text), `src/tui/picker.rs:1068` (a fixed list of names).

**Assertions on output: run by `sweep.sh` (here).** Each test ran with
`TMPDIR` named after its own literals, against binaries built with
`XFX_BUILD_REVISION=1049997a7b7c`, a revision that spells `1049`, `997` and
`7`. A test whose output holds a temporary path or the revision fails there.
Their fixtures all make their directories with `tempfile`, which reads
`TMPDIR` (for example `tests/tool_loop.rs:81` and
`tests/support/sandbox.rs:46`).
Output, on the fixed clone:

```
sw-s1              ok. 1 passed output::tests::the_panels_session_and_ultrafast_change_nothing_the_status_command_prints
sw-T3              ok. 1 passed agent::compact::tests::the_newest_exchanges_stay_and_the_older_ones_of_their_turn_are_compacted
sw-[2J-next        ok. 1 passed credits::tests::an_unreadable_provider_asks_nobody_and_says_so_in_the_turns_words
sw-six             ok. 1 passed interactive::tests::the_unknown_command_refusal_counts_the_registry_rather_than_a_literal
sw-evil            ok. 1 passed provider::model::tests::a_catalog_id_with_a_control_character_is_refused_when_a_query_resolves_to_it
sw-evil            ok. 1 passed tui::shell::tests::the_band_never_announces_or_sends_a_model_id_xfx_would_not_have_written
sw-pw@             FAILED. 0 passed trace::tests::a_daemon_url_is_reported_without_its_credentials
sw-#:              ok. 1 passed tui::blocks::tests::table_wraps_long_cells_inside_the_grid_instead_of_stacking_fields
sw-sk-             ok. 1 passed tui::check::tests::a_reject_carries_no_row_text
sw-7-accepted      ok. 1 passed tui::diagnostic::tests::a_marked_partial_reports_its_reason_without_the_original_sentence
sw-opus-ask-yolo   ok. 1 passed tui::hint::tests::a_notice_overrides_the_left_side_without_losing_the_right
sw-opus-ask-yolo   ok. 1 passed tui::hint::tests::every_mode_is_said_in_the_word_upstreams_row_says_it_in
sw-opus-ask-yolo   ok. 1 passed tui::shell::tests::left_steps_back_to_the_model_column_with_the_mark_on_the_model
sw-of 2-row0       ok. 1 passed tui::question::tests::one_question_is_visible_at_a_time_with_ordinals_and_a_freeform_slot
sw-of 2-row0       ok. 1 passed tui::question::tests::an_answer_taller_than_its_share_of_the_screen_keeps_the_caret_row_shown
sw-1049            ok. 1 passed tui::shell::tests::a_question_the_other_plane_owns_is_painted_there_and_not_in_the_band
sw-high            ok. 1 passed tui::shell::tests::effort_with_a_word_applies_it_or_says_which_levels_there_are
sw-;3[3[4          ok. 1 passed tui::usage_menu::tests::the_current_model_is_bold_and_colourless
sw-%               ok. 1 passed tui::shell::tests::usage_and_max_context_drive_one_context_meter
sw-%               ok. 1 passed tui::shell::tests::a_turn_that_published_no_usage_leaves_the_meter_off
sw-%               ok. 1 passed tui::shell::tests::a_new_session_clears_the_meter_it_measured_and_keeps_the_window
sw-%               ok. 1 passed tui::hint::tests::the_meter_is_the_first_segment_a_narrow_screen_gives_up
sw-%               ok. 1 passed tui::hint::tests::a_window_nobody_published_leaves_the_numerator_on_its_own
sw-xfx:            FAILED. 0 passed an_empty_line_asks_nothing_and_prompts_again
sw-one             ok. 1 passed read_file_honors_an_explicit_line_range
sw-997             ok. 1 passed a_palette_the_user_named_is_not_overruled_by_the_terminal
```

- **Fixed: one.** `src/output.rs:2056` (`"s1"`), above.
- **Output holds a path, literal not reachable from random data: two, left.**
  - `src/trace.rs:2863`, `!report.contains("pw@")`. The report has a
    `workspace: <tempdir>` line; under `sw-pw@` it read
    `workspace: /private/tmp/claude-501/sw-pw@/.tmp…`.
  - `tests/interactive.rs:243`, `!text.contains("xfx:")`. The shell prints
    `[shell] workspace=<tempdir>/workspace`.

  `@` and `:` are not in a tempfile name's `[A-Za-z0-9]`, a hex revision, a
  time or a pid. Only a `TMPDIR` someone names with `pw@` or `xfx:` in it
  fails these.
- **Output holds no path and no revision: the other 23, left.** Every other
  input to them is the test's own fixture. The footer hint holds no path in
  these fixtures: its workspace segment is off unless a setting turns it on
  (`src/config.rs:1318`). The diagnostic report is four fixed fields
  (`src/tui/diagnostic.rs:147-159`), so `'7'` cannot come from a time or a
  pid. In `a_palette_the_user_named_is_not_overruled_by_the_terminal` no
  turn runs, so no clock is painted [추정].
- Negated `.contains(name)` where `name` is bound to a short literal
  (`let`/`const` in the 60 lines before): none. The same scan with the bound
  raised to 10 characters finds `src/interactive.rs:3527` and `:3529`
  (`"/models"`), so the scan works.

## Gates

Run in the worktree once, one at a time, each after
`until ! pgrep -x cargo >/dev/null && ! pgrep -x 'xfx-[0-9a-f]+' >/dev/null; do sleep 30; done`,
with `TMPDIR=/private/tmp/claude-501/flake-status-s1`. That directory holds
`s1`, so both test runs are also a run of the fixed test under the
collision (`the_panels_session_and_ultrafast_change_nothing_the_status_command_prints ... ok`
in both).

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| `cargo clippy --locked --all-targets --features fault-injection -- -D warnings` | ok |
| `cargo test --locked --no-fail-fast` | 4879 passed, 0 failed (15 binaries) |
| `cargo test --locked --features fault-injection --no-fail-fast` | 4908 passed, 0 failed (15 binaries) |
| `scripts/check-xfx-identity.sh` | ok |
| `scripts/check-no-stubs.sh` | ok |
| `scripts/check-no-secrets.sh` | ok, re-run after the commit so the receipt is scanned |

No `docs/parity.md` row, registry count, help entry or `check-no-stubs` row
changes: the unit is one test.
