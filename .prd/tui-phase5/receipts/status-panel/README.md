---
title: P-status receipt -- the /status panel
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, status, info-panel]
---

# P-status receipt: `/status` opens the info panel

Unit 2 of `research/panels.md` §5 (U-status), on P-panel's branch
(`feat/p5-info-panel` at `1dcbad65`), which ships inside this PR: P-panel had
no opener a user could reach. Spec: fx `481104fd`
`src/core/output/output_contracts.zig:681-783` (`StatusSnapshot`,
`panelRows`), `src/core/session/session_commands.zig:283-351`, `:404-408`
(`openStatusSnapshot`, `refreshedStatusSnapshot`, `statusFacts`,
`shellSession`), `src/core/app/app_commands.zig:522-552` (`commandShowStatus`,
`openInfoPanel`, `collectInfoPanelFacts`), `src/main.zig:2884-2886`,
`src/builtins/commands.zig:455`.

Code: `src/output.rs` (`StatusSnapshot::panel_rows`, `ShellSession`, the
panel-only `session` and `ultrafast_requested`, both `#[serde(skip)]` so
`xfx status` prints what it printed), `src/config.rs`
(`unreadable_profile_settings`), `src/interactive.rs` (`open_conversation_as`,
the R1 summary), `src/tui/worker.rs` (the launch-time id, `open_next`,
`shell_session`, the board published as a session opens),
`src/tui/info_panel.rs` (`Panel::status`, `Facts::of_status`),
`src/tui/shell.rs` (`status`, `status_in_force`, `settle_info_panel`).

## Decisions (the dispatcher's, reversible; recorded in `docs/parity.md:113` and the ledger)

1. **The Session row from launch.** `worker::spawn` allocates the id in memory
   (`SessionId::generate`) and seeds the board with `<id> · not saved yet`; the
   first prompt's session is created under it (`Runtime::open_next`), and the
   board says `saved` as the open lands, while that first turn runs
   (`Runtime::opened`). Nothing is written before the first prompt
   (`status_names_the_session_the_first_prompt_opens_before_the_store_holds_it`
   checks the sessions directory is empty, then holds that id). Each open,
   taken or refused, allocates the next id, so a reset shows the next
   session's and a half-made one is never asked for again. A store that
   refuses the session is `none · this shell can't save sessions` until one
   opens (`a_store_that_refuses_the_session_is_a_shell_that_cannot_save_until_one_opens`).
2. **xfx-only facts.** `provider`/`provider_url` are `Provider`/`Endpoint`,
   shown off the Gateway only. **`provider_rejected` is not `Connected`**: fx's
   row lists the providers signed in; xfx's field is a provider selection it
   could not read. No `Connected` row; the rejection is said by
   `Provider  rejected` and by `Auth help`, which quotes the value
   (`the_status_panel_names_a_provider_that_is_not_the_gateway_and_its_endpoint`).
   A `Sandbox` row after `Permissions`: ledger `status-panel-sandbox`.
3. **`Updates`** omitted: ledger `status-panel-updates`.

## Captures

```
FX_BIN=<fx v0.0.16, sha256 8e6cda2e…> scripts/fxcap/capture.sh fx  <scenario> <size> <dir>
XFX_BIN=<this branch's debug build>   scripts/fxcap/capture.sh xfx <scenario> <size> <dir>
python3 -I .prd/tui-phase5/receipts/status-panel/pairs.py
```

for `p5-panels-status`, `-status-first`, `-noauth` (40x120; the research
scenarios, unchanged), and P-panel's `p5-panels-status-short` (14x120, 24x50)
and `p5-panels-keys` (40x120), both sides taken again here. The xfx binary is
`target/debug/xfx` at `7b4579a2`, whose code is this unit's first commit's.
Sanitized: `/private/tmp/claude-501` is `<tmp>` (`scripts/fxcap/redact.py`).
`pairs.py` runs the sweep's own classification (`ledger.classify`,
`ledger.cascade`, `sweep.attribute`) against the v0.0.16 pin.

**RED** on the base (`1dcbad65`), `cells.py`: `status-open` 16 rows,
`status-after-turn` 36, `status-first-open-first` 18, `noauth-open` 18 -- the
`* status:` notice where fx has the panel.

## Verdicts, pair by pair

| pair | verdict | why |
|---|---|---|
| `status-{turn,status-now,esc-closed,enter-closed,closed-final}`, `status-first-closed`, `keys-after-ctrl-c-esc`, `status-short-14x120-{open,down1,esc}` | normalized-identical | -- |
| `status-{open,down,typed,after-turn,open-midturn,open-after-midturn}`, `status-first-{open-first,after-first}`, `keys-{btab,tab-pgdn,paste,ctrl-c}` | approved (aligned) | Sandbox row, no Updates row, Step limit `25` |
| `noauth-open` | approved (aligned) | the same, and `Auth help` without `/login` (sign-in, #88) |
| `noauth-launch` | approved | the hint row's `run /login` (sign-in, #88): not this unit's |
| `status-short-14x120-up1` | approved (aligned) | the window holds Updates and Step limit |
| `status-short-14x120-{ctrl-k,down-end}` | differs | the scrolled window: xfx's Sandbox stands where nothing does in fx's, so the two-line window shows the line before fx's (cause `status-panel-scroll-window`) |
| `status-short-24x50-{open,down1}` | differs | xfx's 30-character id wraps at the 30-column value column, so the panel overflows by one (`↑↓ scroll`, Build out of view); and a debug banner wraps at 50 columns (`status-panel-session-id-wrap`, `xfx-debug-banner-wrap`) |
| `status-short-24x50-{up1,ctrl-k,down-end}` | differs | the same overflow scrolls xfx's panel where fx's fits |
| `status-short-24x50-esc` | differs | the debug banner's wrap only |
| `keys-{perm-btab,open-before-approval,approval-arrives,approval-esc,end}` | differs | bare `/permissions` is still notices, so Shift-Tab moves the mode and no approval arrives: U-permissions (`U-permissions-panel`) |

Of P-panel's 22 pairs: 9 are the same or approved now (`14x120-{open,down1,esc,up1}`,
`keys-{btab,tab-pgdn,paste,ctrl-c,after-ctrl-c-esc}`), 8 differ for the id,
the banner and the scrolled windows above, 5 wait for U-permissions.

## Rows waiting on H1 (no harness verb on this base), pinned by unit tests

- **Settings unreadable after launch** (H1):
  `status_says_settings_it_cannot_read_on_the_gateway_with_no_credential_once`
  (read at open on the Gateway with no credential only, a rebuild reads no
  file), `the_status_panel_shows_settings_it_could_not_read_where_the_credential_was`,
  `profile_settings_that_do_not_read_now_say_why_and_a_missing_file_is_none`.
- **A non-Gateway provider** (H2): `the_status_panel_names_a_provider_that_is_not_the_gateway_and_its_endpoint`.
- **An unwritable store** (H3): `a_store_that_refuses_the_session_is_a_shell_that_cannot_save_until_one_opens`,
  `status_names_a_session_this_shell_cannot_save`.

## Residuals

- `Session grants` after an "always" answered mid-turn reaches the panel at the
  turn's end, when the runtime republishes; fx's reads its live engine the
  moment the approval closes. Written in `docs/parity.md:113`; not captured.
- `Facts::Permissions` and `Row::item` carry `#[cfg_attr(not(test),
  expect(dead_code))]`: U-permissions deletes both attributes when it uses them
  (the build fails until it does).
