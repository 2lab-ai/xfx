---
title: P-permissions receipt -- the bare /permissions panel
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, permissions, info-panel]
---

# P-permissions receipt: a bare `/permissions` opens the info panel

Unit 3 of `research/panels.md` §5 (U-permissions), on P-status's branch
(`feat/p5-status-panel` at `d3bc63d3`, draft PR #33), which carries P-panel's
occupant. Spec: fx `481104fd` `src/core/app/app_commands.zig:586-596`
(`commandHandlePermissions`), `src/core/session/session_commands.zig:356-400`
(`openPermissionsSnapshot`, `refreshedPermissionsSnapshot`, `permissionsFacts`,
`savedPermissionRules`), `src/core/output/output_contracts.zig:1036-1060`
(`PermissionsSnapshot.panelRows`) and `:576-583` (`safeText`, `countText`).

Code: `src/tui/info_panel.rs` (`permissions_rows`, `Facts::Permissions` with
the reset count; both `cfg_attr(not(test), expect(dead_code))` attributes are
gone), `src/tui/shell.rs` (`permissions`, `permissions_in_force`,
`permissions_panel`, `settle_permissions_panel`), `src/tui/permissions.rs`
(`Granted`; the two bare notices deleted), `src/tui/worker.rs` (`granted`,
`WorkHandle::resets`), `src/permission/slash.rs` (`grant_target`, which the
line shell's `grant_line` now calls), `src/tui/approval_view.rs` (`named`,
now crate-visible). Row: `docs/parity.md:94`, the **Bare** clause.

## Decisions (reversible; recorded in `docs/parity.md:94`)

1. **The authority in force.** `Mode` is the mode this shell holds, the hint
   row's, which `choose_mode` puts in the live authority before the runtime
   records it. `Session grants` is the runtime's last publication, and none
   at all once the live authority's reset count has passed the count it was
   published under. `Configured rules` is the live table every decision
   reads. The facts are upstream's three plus that reset count, so a reset
   taken from the live authority rebuilds an open panel on the next tick
   (`an_open_permissions_panel_says_the_authority_as_it_moves_and_keeps_its_scroll`).
   The count is read first: a reset between the reads leaves the facts older
   than the rows, never newer.
2. **Items are named, past upstream.** Each item goes through
   `approval_view::named`, then `Row::item` (`terminal_safe`). A control, a
   line break, a bidi control or a default-ignorable code point in a pattern,
   a command or a saved rule's action is written as its code point, as the
   approval band writes what an "always" grants. Upstream's `safeText` spells
   controls only. Printable text is the same on both, so no capture differs
   (`a_permissions_item_names_what_a_model_or_a_file_put_in_it`,
   `a_bare_permissions_panel_names_what_a_model_or_a_file_put_in_it`).
3. **The grants board carries `(tool, target)`.** It used to carry finished
   ` - tool -> target` lines, made inert. The panel is its one reader, and the
   panel's rows are where text is made safe now
   (`a_grant_is_published_as_it_is_held_for_the_panel_to_spell`). The line
   shell keeps its own `[permissions]` lines, byte for byte.
4. **A store that cannot be read.** Opening the panel then shows nothing and
   says `✗ permissions: <reason>`, as the listing did, since upstream's
   open fails with the read. A store that stops reading while the panel is
   up closes it the same way. No rule is shown that cannot be read.
5. **`Saved rules` is always a row.** The TUI is a host that keeps saved
   sessions. The row says `none` until the first prompt opens one.

## Captures

```
FX_BIN=<fx v0.0.16, sha256 8e6cda2e…> scripts/fxcap/capture.sh fx  <scenario> 40x120 <dir>
XFX_BIN=<this branch's debug build>   scripts/fxcap/capture.sh xfx <scenario> 40x120 <dir>
python3 -I .prd/tui-phase5/receipts/status-panel/pairs.py .prd/tui-phase5/receipts/permissions-panel
```

- `p5-panels-permissions`: the research scenario with one change. Each
  product gets its own turn (`p5-panels-perm-turn.json`, fx's `shell`;
  `p5-panels-perm-turn-xfx.json`, xfx's `terminal`), as `p5-panels-keys`
  does. Both sides were taken here.
- `p5-panels-permissions-saved`: new. It remembers a rule after the first
  prompt, where both products have a saved session, so the `Saved rules` item
  is compared. Both sides were taken here.
- `p5-panels-keys`: P-panel's scenario. Only the five pairs that waited for
  this unit are here. Their fx side is P-status's receipt's, unchanged. The
  xfx side was taken again here.

The xfx binary is `target/debug/xfx` at `6d495f1b`, round P-permissions-b's code commit (its banner says `6d495f1`). Round 1's xfx sides, taken at `450c45ef`, gave the same verdicts; only the banner, the session ids and the run's tmp names differ between the two. `/private/tmp/claude-501`
is written `<tmp>` (`scripts/fxcap/redact.py`). Nothing else was edited.

**RED** on the base (`d3bc63d3`), `cells.py`, row 1 not compared:
`permissions-bare` 12 rows, `-down1` 12, `-esc` 9, `-auto` 10, `-full` 12,
`-ask` 14, `-reset` 16, `-bogus` 21, `-bare-after` 29, `-rules` 30. The xfx
side shows the two `* permissions:` notices where fx shows the panel, and the
notices stay in the document. The five keys pairs on the base: `perm-btab`
14, `open-before-approval` 31, `approval-arrives` 36, `approval-esc` 26,
`end` 26. Shift-Tab under the notices moves the mode, so the turn runs in
`auto` and no approval arrives.

## Verdicts, pair by pair (`pairs.py`, the v0.0.16 pin)

`green.txt` is `pairs.py`'s output for this directory, as it is, one line a
pair, exit 0. Re-run it and diff the two.

| pair | verdict | why |
|---|---|---|
| `permissions-{bare,down1,esc,auto,full,ask,reset,bogus,bare-after,rules}` | normalized-identical | -- |
| `permissions-saved-{after-grant,remember-prompt,remember-done,saved-esc,revoke-prompt}` | normalized-identical | -- |
| `permissions-saved-{saved,after-revoke}` | differs | the command grant's item: fx `• bash → @fx-terminal-env:user:8:/bin/zsh::echo hi`, xfx `• terminal → echo hi in <dir>` (`tool-schema`). The `Saved rules` rows are the same on both (`1` and its item in `saved`, `none` in `after-revoke`) |
| `permissions-saved-approval`, `permissions-approval` | differs | the command view (`tool-schema`) and xfx's scope rows (`approval-grant-scope`) |
| `permissions-{remember-prompt,remember-done,saved,after-grant,status-grants,revoke-prompt,revoke-done}` | differs | xfx refuses a rule before the first prompt (`saved-rules-before-first-prompt`, `-band`), below |
| `permissions-{grants,after-revoke}` | differs | the same, plus the grant's item (`tool-schema`) and the row it moves (`saved-rules-before-first-prompt-shift`) |
| `keys-perm-btab` | normalized-identical | -- |
| `keys-open-before-approval` | approved (aligned) | P-status's Sandbox, Updates and Step limit |
| `keys-approval-arrives` | differs | the command view (`tool-schema`) and the scope rows (`approval-grant-scope`) |
| `keys-{approval-esc,end}` | differs | `▪ Cancelled` against xfx's `■ Cancelled` (`cancelled-marker`, T-U4's). The Status panel under it is approved, as above |

Every row left is named by a cause in `scripts/fxcap/parity-ledger.toml`.
`U-permissions-panel` is retired. `tool-schema` now also covers the panel's
command grant. `cancelled-marker`, `saved-rules-before-first-prompt`, `-band`
and `-shift` are new.

## The approval band's command view

It is not a v0.0.16 change. fx's `$ # shell.run profile=user shell=/bin/zsh`
is `src/core/execution/command_environment.zig:123@481104fd`, the same text
as `:128@c1db9191`. xfx's `$ # terminal.exec cwd=.` is
`src/tui/approval_view.rs:685`. The difference is Phase 4's unowned
`tool-schema` cause: xfx's command tool is `terminal`, fx's is `shell`.
xfx's `2 = allow every future run …` rows are a second cause,
`approval-grant-scope`, a pending user decision. No fx change in v0.0.16
needs a unit here. `keys-approval-arrives` holds on both causes.
`keys-approval-esc` and `-end` hold only on T-U4's marker.

## Open: a rule before the first prompt

fx's session is open from launch, so `/permissions remember` typed before the
first prompt keeps its rule. xfx opens its session at the first prompt and
refuses a rule before it (`✗ permissions: saved-session permission rules
require an active saved session`). P-status decision 1 lets `/rename` open the
session under the launch id. Whether `remember` and `revoke` should do the
same is a user decision. It changes a subcommand's path, so this unit did not
make it.

## Round P-permissions-b (review)

- **A store that moves on its own.** In round 1 an open panel re-read the
  saved-session store only when the mode, the grant count, the rule count or
  the reset count moved. A store broken or rewritten by another hand stayed
  on screen unchanged. Now `Facts::Permissions` also carries the store file's
  stamp (`session_rules::stamp`: absent, or device, inode, size and mtime).
  That is one `stat` a tick. A file that moved alone is read again: a broken
  one closes the panel with the error, and another valid rule set is the one
  shown. Two tests cover this:
  `a_saved_rule_store_that_breaks_while_the_panel_is_up_closes_it` breaks
  only the file now, and
  `an_open_permissions_panel_follows_a_saved_rule_store_another_hand_changed`
  is new. Both fail at `f80e831a`.
- **The reset count, pinned.** In
  `a_reset_alone_rebuilds_an_open_permissions_panel`, the mode stays `ask`
  and the grant count stays 1 across a reset. The panel must show the grant
  published after the reset. The test fails when the count is taken out of
  the facts.
- **`named`, pinned.** In
  `a_permissions_item_names_the_joiners_and_selectors_an_emoji_carries`, an
  item holds a ZWJ family, `❤` with `U+FE0F`, and a bell. Only `named` spells
  the joiners and the selector. The test fails with `tamed`.

## Round P-permissions-c (review)

- **An Unknown stamp counts as moved.** If the panel was built while `stat`
  failed, it reads the store again on every tick
  (`an_open_permissions_panel_whose_store_cannot_be_stamped_reads_it_again`,
  which fails at `dc006615`). A re-read that finds the same rows owes no
  frame.
- **A store that is not a regular file is refused.** `load` opens with
  `O_NONBLOCK|O_NOCTTY` and refuses anything its own `fstat` says is not a
  regular file. A FIFO at the store's path therefore closes the panel at
  once, instead of holding the terminal's thread while it waits for a
  writer. Two tests bound the wait with a watchdog:
  `a_saved_rule_store_that_turns_into_a_fifo_closes_the_panel_without_waiting`,
  which fails at `dc006615`, and
  `a_store_that_is_no_regular_file_is_refused_without_waiting_for_a_writer`.
- **The inode part of the stamp, pinned.** A store renamed over the first
  one, with the same size, mode and mtime, is read again
  (`a_store_replaced_by_another_of_the_same_size_and_time_is_read_again`). It
  fails when the stamp is cut down to size alone, and when only the inode is
  removed.

The captures were not taken again in this round. It changes nothing on
screen for a store that is a regular file, so `green.txt` still holds.

## Note on P-status's receipt

`U-permissions-panel` is retired from the ledger. P-status's directory still
holds its five stale xfx keys files (`keys-{perm-btab,open-before-approval,approval-arrives,approval-esc,end}`).
If someone runs `pairs.py` on that directory, those five pairs now show as
unattributed. This directory has the current ones. No gate runs `pairs.py`
on receipt directories.

## Residual

A grant answered "always" mid-turn reaches an open panel when the turn ends
and the runtime publishes. fx's panel reads its live engine when the approval
closes. `/status` has the same gap (`docs/parity.md:113`).
