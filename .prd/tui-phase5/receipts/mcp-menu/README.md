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
  "--profile"|"--project", NAME])` (since MCP-Mc `Job::Remove { name, file }`,
  below).
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
  verb run warnings-only, `.warnings_only`; since MCP-Mc `remove_chosen`), `safe_within` (`safe` with its
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

## MCP-Mc: the reviews' fixes

The six MCP-M reviews (gpt6 and opus, slices A-C) found one MUST (opus-B) and
the SHOULDs below; the coordinator listed them as items 1-10. Every fix has a
test that fails on `5632596c` (MCP-Mb's head) -- the new tests put on that
code, `bounded.py 1500 6144`: 13 of 38 failed in 4.9 s, peak 1832 MiB; the
restore test and the split command-line case after, on a throwaway clone at
`5632596c` -- and passes on the head.

| item | test | on `5632596c` | on the head |
|---|---|---|---|
| MUST: a one-row entry drew a wrapped piece four columns in, as a row of its own | `tui::mcp_menu::tests::a_one_row_address_never_draws_a_wrapped_piece_as_a_header_row` (`https://a.example/mcp?q=`, 300 spaces, `header: X-Team: blue`, which `validate_endpoint` takes; 40, 60, 120 columns) | `40: "    header: X-Team: blue" in ["    https://a.example/mcp?q=", "    header: X-Team: blue"]` | every piece after the first `  ↳ ` |
| | `…::a_one_row_command_line_never_draws_a_wrapped_piece_as_a_variable_row` (`/bin/sh server.sh env: PATH=./x`, 22-26 columns) | `22: ["    /bin/sh server.sh", "    env: PATH=./x"]` | `    /bin/sh server.sh` / `  ↳ env: PATH=./x` |
| 1: Remove and Reject answered a question no frame drew | `tui::mcp_menu::tests::remove_and_reject_take_no_answer_before_a_frame_painted_the_question` | `left: Run(Remove { name: "remote", file: Profile })`, `right: None` | Enter waits for a frame; a frame written over since (`forget_drawn`) is waited past too |
| | `tui::shell::tests::a_burst_of_enters_in_one_read_answers_no_question_it_never_painted` (`/mcp⏎⏎⏎⏎` in one `consume`) | `a question nobody saw was answered` (the profile emptied) | nothing removed |
| | `tui::shell::tests::a_held_enter_after_a_choice_runs_reaches_no_other_question` (Enter held, 40 reads 40 ms apart, after a Remove) | `assertion failed: case.drained().is_empty()` (a second Remove ran) | nothing more runs |
| 2: keys under Ctrl-O reached the menu | `tui::shell::tests::the_menu_takes_no_key_while_the_full_transcript_covers_it` (Remove, Reject and Approve each) | `Remove keep from ~/.xfx/mcp.json? was answered unseen` | no key reaches the menu while the full transcript is up; closed, the question is still asked |
| 3: a resize, a blind screen or a refused restore left the last frame "drawn" | `tui::shell::tests::an_approval_drawn_before_a_resize_is_not_taken_after_it` | `approved after a resize, unseen` (`settings.json` written) | nothing written, after a resize or on a blind screen |
| | `tui::event_loop::tests::a_menu_question_the_return_from_the_full_transcript_never_painted_takes_no_answer` (the band `restore_primary` composes, refused by its check and by the screen) | `a question no frame painted was answered (screen refuses: false)` | Enter answers only after the retry lands |
| 4: Remove re-parsed an argv and took the file at answer time | `tui::mcp_menu::tests::a_question_whose_server_moved_to_the_other_file_is_cancelled` | `left: Run(Remove { name: "proj", file: Profile })`, `right: None` | the question is cancelled |
| | `tui::shell::tests::remove_in_the_menu_removes_a_server_whose_name_reads_as_a_flag` (`--help` in `.mcp.json`) | `.mcp.json` still `{"mcpServers":{"--help":…}}` (Help, said to nobody) | removed |
| 5: a reload kept a server's shown entry across a respelling | `tui::shell::tests::a_project_entry_respelled_is_approved_from_the_menu` (`{"command"}` → `{"type":"stdio","command"}`, reload, approve) | `the respelled entry kept the old one's digest` (`sha256:cee8c958…`, the fresh read `sha256:af302ff9…`) | approved, the fresh digest saved |
| 6: `${VAR}` shown in single quotes, which a shell reads as not expanded | `mcp::runtime::tests::a_word_xfx_expands_is_shown_as_a_shell_reads_an_expansion` | ``command: /bin/node '${SECRET}' 'a\"b$c`d\\e${X:-d$f}' '$HOME'`` | ``command: /bin/node "${SECRET}" "a\"b\$c\`d\\e${X:-d\$f}" '$HOME'`` |
| | `mcp::runtime::tests::every_row_shows_the_entry_as_written_and_never_a_value_from_the_environment` (its expectation changed) | `command: '${TOOL}' --key '${SECRET}'…` | `command: "${TOOL}" --key "${SECRET}"…` |
| 7: a row cut with `…` by clusters still counted whole | `tui::mcp_menu::tests::a_line_wider_on_the_screen_than_it_was_measured_is_not_drawn_whole` (`❤️` ×40: one cell a character, two a cluster) | Enter approved (`assertion failed: matches!(menu.press(Key::Enter), Effect::Refused(..))`) | refused, `couldn't show all` |
| 8: `$` opened a skills menu behind the menu | `tui::shell::tests::a_dollar_typed_into_the_menus_filter_opens_no_skills_menu_behind_it` | `a skills menu opened behind /mcp` | none opens |

**Item 9, the pins a revert left green.** Each guard below removed on a
throwaway clone of the head (`mutants_mc.py <clone> [names]`, which refuses a path outside a scratchpad: never the worktree), and the test
that should fail run:

| guard removed | test that failed |
|---|---|
| `width >= APPROVAL_MIN_COLS` | `an_approval_below_twenty_columns_approves_nothing` -- rewritten: the first version (a long entry) stayed green, since below 20 columns the row-width term refuses a long entry anyway; it now uses `/bin/sh`, which fits 19 columns, refused at 19 and approved at 20 |
| `*self.drawn.borrow_mut() = None` when an approval opens | `opening_an_approval_again_forgets_the_one_drawn_before` |
| `drawn.name == row.name` | `an_approval_drawn_for_one_server_approves_no_other` |
| the tool name's spelling | `every_name_target_error_and_tool_is_spelled_…` (fixture `t\x1b]0;x\x07`, an OSC no colour check accepts; the old `t\x1b[1m` was a valid SGR) |
| `Some(file)` in `remove_chosen` (both files hold the name) | `remove_in_the_menu_removes_from_the_file_its_question_named_when_both_hold_the_name` |
| ↑↓ flipping the answer | `up_and_down_on_a_question_leave_its_answer` -- rewritten: the first version pressed ↑ then ↓ before reading, and two flips cancel; it now reads after every key |
| a printable key answering yes | `remove_and_reject_ask_in_place_and_anything_but_yes_changes_nothing_on_disk`, which now compares the bytes and existence of `~/.xfx/mcp.json`, `settings.json` and `.mcp.json` before and after |

The `!shortened` term in `whole` is deleted rather than pinned: `confirmed`
refuses a shortened entry before it reads `whole`, and
`a_row_too_long_to_show_is_shortened_saying_so_and_approves_nothing` pins
that branch's exact message.

The new guards removed the same way, each caught: the `↳` rule reverted to
`led` (both one-row tests); the question's drawn gate
(`remove_and_reject_take_no_answer_…`; the shell burst test stays green on
that mutant, because the held-Enter rule also refuses Enters of one read --
each test pins its own guard); `question_drawn` not cleared by `forget_drawn`;
the identity check; the held-Enter rule; Ctrl-O's swallow; the forget on
resize, and on a blind screen, separately (`an_approval_drawn_before_a_resize_…`);
each `restore_primary` arm's forget, separately (the event-loop test); the
fresh-digest term in `kept`; the double-quoted expansion; the cluster term
(`fits`). The skills fix is two guards -- the `$` check and
`reconcile_picker` clearing `skills_menu` -- either of which alone keeps the
menu closed: removed one at a time the test stays green, removed together it
fails.

**Item 10.** Deleted: `spelled` and the `None` arm that drew a row no approval
could bind (an entry with no digest shows nothing); `ConfirmLine::Detail`
(only ever empty, now `Blank`); `Layout.window` (always `SERVER_WINDOW`);
`server_screen`'s second copy of `keep_visible` (`Layout::top`, one rule);
`Job::Choice` and `command::choose` (`Job::Remove { name, file }` and
`command::remove_chosen`). Kept: `cut_at`, with the cluster check (`fits`) that
item 7 offered in place of `wrap::wrap`.

**Decisions.** Every wrapped piece is led by `↳ `: its cost is the look of fx's
wrapped command line or address (`docs/parity.md` `/mcp`). The held-Enter rule
covers every confirmation, Approve's too, at 500 ms (`MCP_ENTER_HELD`): a key
held repeats every 30-100 ms after an initial delay of about 375 ms on macOS
and 660 ms on X11, and a person answering reads first. Yes stays chosen first,
as fx's (`menu.zig` `answer_no: bool = false`, presentation `:771-775`);
`offer_to_mcp`'s comment said `[y/N]`, and now says so. Ctrl-O swallows every
event while the menu is up, not only Enter. `tests/mcp.rs`' `press` waits 700
ms after an Enter (300 ms after any other key), and the narrow-terminal
approval case strips the leads and asserts one. The event-loop test sleeps
600 ms before each of its four Enters, two arms: about 5 s.

**The pairs.** `sweep.sh` on the head build (`562149d`'s code, which
`0dac6b0a` changes only in tests; debug, sha256 `e14fe347138794d7…`), the
five scenarios at 40x120 and `p5-mcp-menu-new` at 40x60, then re-scored with
the ledger entry below (`scoreboard-head-mc-120.md`, `-60.md`):

- `p5-mcp-menu-new`: 22 / 34 at both sizes, where MCP-Mb had 23.
  `approve-confirm` moved, from normalized-identical to approved-divergence:
  the waiting entry's command line (`<bun> <scenarios>/p5-mcp-stdio.ts`)
  wraps, and its second piece is `  ↳ …` where fx's is `    …` (row 9 at
  40x120, rows 8-9 at 40x60) -- the one pair that shows the `↳` rule. Before
  the entry it scored an unresolved defect. The other 11 are `remote`'s
  status alone (`remote_only.py`: 11 of 11 and `approve-confirm`, 0 other,
  both sizes), as before.
- `p5-mcp-empty` 24 / 24, `p5-input-mcp` 9 / 9, `p5-mcp-dismiss` 2 / 2: did
  not move.
- `p5-mcp-verbs` 1 / 28 positionally, did not move: aligned, its unmatched
  rows are MCP-V's own causes in MCP-M's numbers (`verbs_causes.py`: the
  `list` cell 140, typed order 91, `retrying` 45, `login` gated 22, the rows
  ordering moves 24).

`scripts/fxcap/parity-ledger.toml` gains `mcp-approval-wrap-led`, class
`unsafe-to-copy`, pin `v0.0.16`, `p5-mcp-menu-new`/`approve-confirm` only,
`fx_row = '^    \S'` and `xfx_row = '^  ↳ '` (each regex speaks for its own
side), owner MCP-M, expiring `date:2027-05-06`. This is the unit's call, not
a reviewer's: a reviewer may drop it, and the pair is then an unresolved
defect with this paragraph as its cause. `xfx-head/` holds this run's
captures, redacted (`redact_copy.py`).

MCP-Mc gates on `0dac6b0a` (the code head; the receipt commit after it changes
this directory and the ledger only): fmt ok; clippy ok, both feature sets;
`cargo test` 4665 / 0 (lib 3789); `--features fault-injection` 4688 / 0 (lib
3798) -- its first run was ended by a SIGTERM from outside its process group
part-way, all passing until then, and its second failed one test once,
`tui::shell::tests::a_space_ends_a_dollar_search_and_a_word_nothing_matches_shows_no_menu`
(`a word no skill matches showed the menu`), a skills test this unit does not
touch, which passed in the plain run, 5 of 5 alone and in the third, whole
run; identity, stubs, secrets ok; release and faulty release builds ok;
`smoke.sh` 46 / 0; `smoke-tui.sh` 37 scenarios + the oracle, 1091 / 0;
`tests/permissions.rs`
`a_command_past_its_deadline_is_killed_at_once_and_the_screen_says_it_timed_out`
alone 3 of 3 (0.31-0.33 s); `tests/mcp.rs` 5 / 0.

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
