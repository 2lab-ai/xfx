# MCP-M receipt -- `/mcp` menu v2 (B1-B13, B15) and no launch trust prompt (C1)

Unit MCP-M of the 0.0.16 delta (`.prd/tui-phase5/research/mcp.md` §5 unit 3 on
the research branch), epic 2, wave 3, the last MCP unit. Security-sensitive (S):
with the launch prompt gone, the menu (and the typed `/mcp approve`) is where a
project server is approved. Branch `feat/p5-mcp-menu`. Oracle fx v0.0.16
(`481104fd`, executable sha256 `8e6cda2efddb…`, checked by capture.sh and by the
sweep before and after every fx run).

## The base

`feat/p5-mcp-verbs` `6df717af` (MCP-V's closed head: MCP-C, MCP-W, MCP-T,
MCP-S, `ce5dccb3` and MCP-V over the H1 harness), with MCP-Ve `a2b0ea6b`
merged in (`--no-edit`) before the gates; nothing conflicted.

## What changed

- `src/tui/mcp_menu.rs` (+ `src/tui/mcp_menu/tests.rs`): replaced. fx v0.0.16's
  picker -- `src/core/mcp_host/menu.zig` (`choices` `:61-83`, `viewOf`
  `:275-296`, `filterChanged` `:306-312`, `settle` `:320-333`, `press`
  `:335-421`) and `src/ui/footer/mcp_host_menu_presentation.zig`
  (`menuRowCount` `:80-96`, `listRow` `:186-229`, `serverListRow` `:231-260`,
  `serverLine` `:278-392`, `confirmLines` `:442-481`, `hintVariants`
  `:507-522`, `question` `:526-535`) @481104fd. Rows are read from the runtime
  at most every 250 ms (`refreshHostMenu`, `app_mcp_runtime.zig:719-757`); a
  server's tools are what fx's `runtime.tools` would find (`:886-895`,
  `heldReason` `:2038-2053`): a held server's reason at once, never a start.
- `src/tui/shell.rs`: the menu's wiring. It is a view of the composer
  (`reconcile_picker`), takes only the keys fx binds (`offer_to_mcp`,
  `app_input_runtime.zig:994-995`, `:1186`, `:1396`, `:1498-1513`,
  `:1752-1831`, `input_completion_runtime.zig:424-429`) and keeps each
  screen's filter (`swap_mcp_filter`, `swapHostMenuFilter`,
  `app_mcp_runtime.zig:643-653`); a choice is a job on the verbs' own paths
  (`mcp_key`): Approve `Job::Trust { ApproveShown, body: "" }`, Reject
  `Job::Trust { Reject, body: "" }`, Remove `Job::Choice(["remove",
  "--profile"|"--project", NAME])`.
- **C1** (`ba0ea4a9`): the launch trust prompt is gone -- `Event::TrustPrompt`,
  `present_prompt`/`prompt_again`/`suppress_prompts`, the shell's
  `TrustPromptUp` and `offer_to_trust_prompt`, `what_it_runs`/`fitted`/
  `shown_whole`, and the transcript's `Gutter::Shown`/`Wrapped`, which only
  the prompt and `v` wrote. With M1's menu went its catalogs
  (`Job::Catalog`, `Event::Catalog`, `Event::MenuFeedback`), `r`
  (`Job::MenuReload`, `McpRuntime::reconnect`, `Origin::Menu`), the
  approve-all of a drawn list (`TrustAction::ApproveListed`) and `z`
  (`TrustAction::Reset`), none with a caller left.
- Seams in `src/mcp/command.rs` (named for the dispatcher): `choose` (a
  verb run warnings-only, `.warnings_only`), `safe_within` (`safe` with its
  cut as a parameter, for fx's 4096-byte footer spelling), `typed` and
  `first_line` made `pub(crate)`. `runtime.rs`: `Job::Choice`; a trust job
  with an empty body says nothing. `statusline.rs`: `needs_attention`
  `pub(crate)`, the one definition the list's groups use.
- `docs/parity.md`: the `/mcp` row (menu, approval, keys, gated choices,
  what reaches the screen), `MCP project servers and trust` (`/mcp list` says
  why), `MCP client runtime` (no reload restarts a failed server).

## Decisions

- `Log in` / `Log out` are not offered (sign-in gated); a server that needs a
  login says `Interactive MCP authentication is unavailable here.` on its
  screen.
- Remove runs `remove --profile|--project NAME`, the file its question named;
  fx runs `remove NAME`, which refuses a name both files hold.
- The approval's screen grows past fx's 13 rows to show all it trusts, and
  refuses (a warning) when a frame did not draw it whole or the entry changed
  since; it shows what the child is given, every variable and header with
  its value as written (MCP-Mb, below).
- A server's tool list follows fx's waiter (`runtime.zig:886-895`): a held
  server's reason at once, tools once ready, a `failed` start's reason.
  **[추정]** a stdio server `retrying` with a failure ends it with that
  reason, an HTTP one `retrying` keeps it loading -- read off two fx captures
  (`p5-mcp-fail` `server-1`, `p5-mcp-menu-new` `server-remote`), not off fx's
  engine; both pairs are identical.
- **[추정]** Page Up / Page Down move the menu one row, as fx's
  `routeComposerVertical` hands a page to `routeNonSlashPickerMove`; not
  captured.

## RED: the base

`sweep.sh --only p5-mcp-menu-new --only p5-mcp-empty --only p5-mcp-verbs`
(40x120) and `--only p5-mcp-menu-new --size 40x60`, the base binary
(`6df717af`, debug, sha256 `b9e952df6da0…`), each run under `bounded.py`
(`../mcp-verbs/bounded.py`: 2400 s and a 4 GiB resident cap over the run's
process group; peak 36 MiB). Scoreboards: `scoreboard-base-120.md`,
`scoreboard-base-60.md`.

| scenario | pairs normalized-identical | first differing row |
|---|---|---|
| `p5-mcp-menu-new` 40x120 | **0 / 34** | `startup` row 3: fx `┃`, xfx `Project MCP server 'proj' is defined in .mcp.json.` -- C1's prompt, then M1's tabbed menu |
| `p5-mcp-menu-new` 40x60 (B15) | **0 / 34** | the same |
| `p5-mcp-empty` 40x120 | 21 / 24 | `menu`, `menu-down`, `menu-right` row 5: fx `  MCP servers  none yet`, xfx `MCP 0  [Servers]  Tools  Resources  Prompts` |
| `p5-mcp-verbs` 40x120 | 0 / 28 | `startup` row 3: the prompt; its four rows and its dismissal push every later row (32 rows left unmatched by the prompt once aligned) |

## GREEN: the head

Same sweeps, head binary (`ce55177a`, debug, sha256 `13dcdd48b970…`; the
head after it adds this directory only). Scoreboards: `scoreboard-head-120.md`,
`scoreboard-head-60.md`. Pairs compared as the sweep compares them: cell by
cell by `scripts/fxcap/cells.py` (which `.prd/tui-phase4/receipts/m1/cells.py`
runs), after the ledger's normalizations (the banner; `fx` read as `xfx`).

| scenario | pairs normalized-identical | the rest |
|---|---|---|
| `p5-mcp-menu-new` 40x120 | **23 / 34** | 11 differ **only** in `remote`'s status: fx `connecting` where xfx says `retrying` (and so shows its `error` row) -- the HTTP try against a refused port, which takes fx 10-20 s and xfx an instant, MCP-S's recorded transport-timing difference (`docs/parity.md`, MCP client runtime; `../mcp-status/README.md`). `remote_only.py`: with `retrying` read as `connecting` and that `error` row out, 11 of 11 identical, 0 other |
| `p5-mcp-menu-new` 40x60 (B15) | **23 / 34** | the same 11, the same way: 11 of 11, 0 other |
| `p5-mcp-empty` 40x120 | **24 / 24** | -- |
| `p5-mcp-verbs` 40x120 | 1 / 28 (`startup`) | **0 rows from the prompt.** Positionally the banner and every row the prompt pushed are back in place; aligned as sequences, every row left unmatched is one of MCP-V's own causes (`verbs_causes.py` over the sweep's `pairs.json`): the `list` table a cell wider (140 rows), `remote`/`web2` `retrying` (45), `show` printed in typed order (A12, 91), `login remote` gated (22), and the blank rows and two `fixture runs a command` rows that ordering moves (24). On the base the same tally adds 32 prompt rows and the banner |

`.files` at the end of `p5-mcp-menu-new`: `~/.xfx/mcp.json` byte-identical to
fx's `~/.fx/mcp.json` (222 bytes, `remote` removed through the menu);
`settings.json` the same value, `{"workspaces":{"<scratch>/work":{"disabledMcpjsonServers":["proj"]}}}`
(xfx's settings writer is two-space; fx keeps `backups/`, xfx none).

**The other scenarios that open the menu or met the prompt** (not this
unit's receipt; `scoreboard-{base,head}-extra.md`): `p5-input-mcp` 0 -> **9 / 9**
(the input cluster's in-place questions), `p5-mcp-dismiss` 0 -> **2 / 2** (no
prompt, the status line), `p5-mcp-fail` 1 -> 4 / 6 (its two server screens now
fx's, `Its tools couldn't load: its process exited with code 3; …` included;
`show-bad` and `end` are `/mcp show`'s wait, MCP-V/MCP-S), `p5-mcp-notes`
1 -> 2 / 4 (the `list` table's cell, MCP-V).

`fx/`, `xfx-head/`: every snap of `p5-mcp-menu-new` at both sizes and of
`p5-mcp-empty`'s menu, `.txt` and `.ansi`, and `p5-mcp-verbs`' `.txt`;
`xfx-base/`: the base's `.txt`. Redacted (`<scenarios>`, `<bun>`,
`<local-path>`, `scripts/fxcap/redact.py`).

## Security (S): the trust decision

| property | test |
|---|---|
| 1. Nothing starts unapproved: a `.mcp.json` server with no approval for its digest is held at launch, on a reload, and through every screen of the menu (list, its screen, the approval opened and cancelled); its marker file is written only once it is approved (the positive control) | `tui::shell::tests::a_project_server_never_starts_unapproved_at_launch_on_reload_or_when_its_menu_opens`; `mcp::tests::a_project_server_waits_for_approval_and_the_choice_is_kept_in_settings` (nothing said at launch); `tests/mcp.rs` `a_project_server_waits_unasked_and_the_menu_rejects_it_into_settings` (no prompt on a real pty) |
| 2. Approve binds what xfx read: Enter approves the entry the approval's screen drew, by its digest (`ApproveShown`); a rewrite the runtime read after the frame approves nothing and says so; one on disk not yet read is saved as the entry shown, and the reload leaves the rewrite waiting, unstarted; a frame that could not show it all, or was written over, approves nothing | `tui::shell::tests::approve_in_the_menu_binds_the_entry_its_screen_drew_and_not_one_read_since`, `…::approve_in_the_menu_saves_the_digest_xfx_read_so_an_entry_rewritten_since_never_runs`, `tui::mcp_menu::tests::approve_binds_only_the_entry_its_screen_drew_whole`; `tests/mcp.rs` `a_project_server_the_model_rewrites_after_approval_waits_to_be_approved_again` |
| 2, the race | the menu still holds A and drew A, the runtime has read B: Enter's job approves A by its digest, so B stays waiting -- an approval by name would bind B | `tui::shell::tests::approve_in_the_menu_approves_the_entry_drawn_when_the_runtime_read_another_before_the_menu_did` |
| 3. Destructive choices ask first: Remove and Reject ask on their row; Esc, Ctrl-C, a printable key (`x`, even `y`), Backspace and `no` each leave `~/.xfx/mcp.json` and `settings.json` byte-for-byte as they were and put nothing in the composer; the approval's Esc and a printable key approve nothing; then yes does it and says nothing | `tui::shell::tests::remove_and_reject_ask_in_place_and_anything_but_yes_changes_nothing_on_disk`, `tui::mcp_menu::tests::remove_asks_on_its_own_row_and_only_yes_runs_it`. Reset: fx v0.0.16 has none, and M1's `z` went with its menu |
| 4. Display is cut: every name, address, command, error, stderr tail and tool reaches the row spelled (`safe_within`, `terminal_safe`; an approval's rows as the approval view spells a command); a name only as far as its first `=` (`typed`). What an approval shows is MCP-Mb's, below | `tui::mcp_menu::tests::every_name_target_error_and_tool_is_spelled_and_a_name_stops_at_its_equals_sign` |

**Each guard removed, on a throwaway clone** (`git clone --no-local` into the
scratchpad, never this worktree; `mutants.py`-style string swaps, each
restored after; bounded, 50 min and 12 GiB, peak 2.8 GiB), and the test that
then fails: the drawn digest unchecked -> `approve_binds_only_the_entry_its_screen_drew_whole`
and `approve_in_the_menu_binds_the_entry_its_screen_drew_and_not_one_read_since`;
`whole` unchecked -> `approve_binds_only_…`; Approve sent by name
(`TrustAction::Approve`) -> `approve_in_the_menu_approves_the_entry_drawn_when_the_runtime_read_another_before_the_menu_did`
(added after this mutant survived the first round); a printable key that
does not cancel -> `remove_and_reject_ask_in_place_…`; Remove run without
asking -> `remove_asks_on_its_own_row_…` and `remove_and_reject_ask_in_place_…`;
a list name not cut at `=` -> `every_name_…` (its name shortened after this
mutant survived: the old one was cut by the row's width first); the approval's
target unspelled -> `every_name_…`.

Key routing shared with the composer, every key fx binds pinned:
`tui::shell::tests::the_menu_takes_the_keys_fx_binds_and_the_composer_keeps_each_screens_filter`
(↑↓, `C-k`/`C-j`, ← and → on the list, with a filter, at the caret's end and
on a server's screen, Enter, Esc on each screen, Ctrl-C, Ctrl-P, each screen's
filter in the composer, no turn sent).


## MCP-Mb: the approval shows what the child is given

The coordinator settled the open question (MCP-M's brief meant S4's "values
never shown" for MCP-V's echo of typed values, not for an approval): keep
xfx's 2026-11-04 rule (`c4d252f5`, `docs/parity.md` `/mcp`, "Deliberately
different from upstream's"). The approval's screen now shows, from the entry
the runtime read (`ShownEntry`: its rows and the digest an approval binds,
from one read), the command line or address and one `env: NAME=value` /
`header: Name: value` row each (`header_env` as `X-Team: ${TEAM}`,
`bearer_token_env` as `Authorization: Bearer ${TOKEN}`), every value as
written, spelled as `approval_view::command_text` spells a command; a row over
1024 bytes is its first 512 and last 256 bytes with `[N bytes in all;
.mcp.json has every byte]` between, and such an entry is not approved;
continuation rows led by `↳ ` when the entry has rows beyond its command or
address. Values typed into `/mcp add` (`typed`) and a profile's in `list` and
`show` stay unsaid.

| | test | on `8f9104d8` (bounded: 900 s, 4 GiB) | on the head |
|---|---|---|---|
| RED | `tui::shell::tests::the_approval_shows_what_the_child_is_given_every_variable_and_header_as_written` (`PATH=./x:${PATH}`, `NODE_OPTIONS=--require ./evil.js`, a value with `U+202E` and a line break, an http entry's literal header) | fails: `"env: PATH=./x:${PATH}" is not on the screen`, the screen saying `It sets PATH, NODE_OPTIONS, ODD.` | passes: each row as written, `env: ODD=a\u{202e}b\x0ac`, `header: X-Team: blue \u{202e}x`, no `U+202E` on the screen |
| too tall | `…::an_entry_too_tall_for_the_screen_is_not_drawn_whole_and_approve_refuses_it` (40 variables, 40-row screen) | -- | Enter approves nothing, says `xfx couldn't show all of what tall runs on this screen, so nothing was approved.`, the entry waits |
| | `tui::mcp_menu::tests::a_row_too_long_to_show_is_shortened_saying_so_and_approves_nothing`, `…::an_approval_shows_each_variable_and_header_as_written_its_wrapped_rows_led`, `…::every_name_target_error_and_tool_is_spelled_and_a_name_stops_at_its_equals_sign` (the entry's rows from `runtime::entry_rows`) | -- | pass |

The pairs did not move (no scenario's entry sets a variable or a header):
`sweep.sh` on the head build (`51e51269`'s code, debug, sha256 `1eb6b5de1b71…`;
`scoreboard-head-mb-120.md`, `-60.md`) -- `p5-mcp-menu-new` 23 / 34 at 40x120
and at 40x60, the 11 left `remote`'s status only (`remote_only.py`: 11 of 11,
0 other, both sizes); `p5-mcp-empty` 24 / 24; `p5-input-mcp` 9 / 9;
`p5-mcp-dismiss` 2 / 2. `xfx-head/` holds this run's menu captures.

Guards removed on the throwaway clone, each caught: the shortened entry
approved -> `a_row_too_long_to_show_…`; the entry's rows after the first
dropped (values hidden again) -> `the_approval_shows_what_the_child_is_given_…`.

MCP-Mb gates on `51e51269` (the head after it changes this directory only):
fmt ok; clippy ok, both feature sets; `cargo test` 4646 / 0 and 4669 / 0
(fault-injection, lib 3772); identity, stubs, secrets ok; release and faulty
release builds ok; `smoke.sh` 46 / 0; `smoke-tui.sh` 37 scenarios + the
oracle, 1091 / 0; `tests/permissions.rs`
`a_command_past_its_deadline_is_killed_at_once_and_the_screen_says_it_timed_out`
alone 3 of 3 (0.30-0.32 s).

## smoke-tui and the pty cases

`scripts/smoke-tui.sh` has no scenario that meets the trust prompt (it opens
no `.mcp.json`); its run is below. The pty cases that expected the prompt are
`tests/mcp.rs`', changed with it: `a_project_server_asks_first_and_three_rejects_it_into_settings`
is now `a_project_server_waits_unasked_and_the_menu_rejects_it_into_settings`
(no prompt; the status line; Reject asked on its row, `x` a no, then yes),
`a_project_server_the_model_rewrites_after_approval_is_shown_and_asked_about_again`
is now `…_waits_to_be_approved_again` (approved on the menu's screen, the
model's rewrite waiting and shown there, rejected there), and
`a_long_command_or_address_reaches_a_narrow_screen_whole_before_the_choices`
is now `a_long_command_or_address_is_shown_whole_on_the_approval_screen_of_a_narrow_terminal`
(80 columns, read after a Ctrl-L redraw: the band is otherwise the cells that
changed). `wait_until_ready_in_the_menu` reads the new list.

## Gates

On the head (`ce55177a` and the test and receipt commit after it):

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings`; `--features fault-injection` | ok; ok |
| `cargo test --locked --no-fail-fast` | 4642 passed, 1 failed under load: `tests/permissions.rs` `a_command_past_its_deadline_is_killed_at_once_and_the_screen_says_it_timed_out` (1.83 s against its 0.9 s, the terminal tool's, not MCP), 3 of 3 alone (0.30 s) |
| `cargo test --locked --features fault-injection --no-fail-fast` | 4666 passed, 0 failed (lib 3769) |
| `scripts/check-xfx-identity.sh`; `check-no-stubs.sh`; `check-no-secrets.sh` | ok; ok; ok |
| `cargo build --locked --release`; `--release --features fault-injection --target-dir target/faulty` | ok; ok |
| `scripts/smoke.sh target/release/xfx` | 46 checks, 0 failures |
| `scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx <dir>` | 37 scenarios + the oracle, 1091 checks, 0 failures |

## Reproduce

```
FX_BIN=<fx v0.0.16> TMPDIR=/private/tmp/claude-501 python3 -I .prd/tui-phase5/receipts/mcp-verbs/bounded.py 2400 4096 -- \
  scripts/fxcap/sweep.sh --fx <fx v0.0.16> --pin-sha 8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2 \
  --only p5-mcp-menu-new --only p5-mcp-empty --only p5-mcp-verbs --jobs 2 --out <dir outside the repo>
# B15: the same with --only p5-mcp-menu-new --size 40x60
python3 -I .prd/tui-phase5/receipts/mcp-menu/remote_only.py <out>/runs/p5-mcp-menu-new/fx <out>/runs/p5-mcp-menu-new/xfx \
  p5-mcp-menu-new 40x120 scripts/fxcap/scenarios/p5-mcp-menu-new.cap
python3 -I .prd/tui-phase5/receipts/mcp-menu/verbs_causes.py <base out>/pairs.json <head out>/pairs.json
```
