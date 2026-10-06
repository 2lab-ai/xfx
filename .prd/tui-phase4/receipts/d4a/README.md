# D4a receipt: `/permissions` and Shift-Tab

Two scenarios, run at 40x120 on both binaries:

```
FX_BIN=<fx c1db919 build, 0.0.13> scripts/fxcap/capture.sh fx  permissions      40x120 <dir>
                                  scripts/fxcap/capture.sh xfx permissions      40x120 <dir>
FX_BIN=<fx c1db919 build, 0.0.13> scripts/fxcap/capture.sh fx  permissions-btab 40x120 <dir>
                                  scripts/fxcap/capture.sh xfx permissions-btab 40x120 <dir>
```

`permissions.cap` types a bare `/permissions`, Down, Escape, then
`/permissions auto`, `full-access`, `ask`, `reset`, `bogus` and a second bare
`/permissions`, snapping after each. `permissions-btab.cap` presses Shift-Tab
(tmux `BTab`) from the harness's `auto` to full access, waits 4.5 s, then
presses it three more times round to full access again, and ends on a bare
`/permissions`. Both start from an empty HOME, so the full-access warning is
owed the first time.

The new `home <label>` verb of `capture.sh` writes every file under the run's
HOME -- path relative to HOME, size, and bytes (locks and logs by size only) --
to `<bin>-<scenario>-40x120-<label>.files`, with the run's own directory
written as `<scratch>`. It is taken after every write: `auto`, `full`, `ask`,
`reset` in the first scenario, `btab-full`, `btab-full-later`, `btab-ask`,
`end` in the second. The shims (`open`, `pbcopy`, `pbpaste`, `afplay`,
`osascript`), the fake `pbcopy`'s pipe and `FX_NO_OPEN_BROWSER=1` are
unchanged.

The xfx binary is `target/debug/xfx` built from this branch's merge commit; its
banner says `b9c5591` because `build.rs` recorded the revision the worktree was
created at. After `plan/tui-phase4` was merged again (D4b `/statusline`, F3),
both xfx scenarios were retaken on `c904947`, on `8dffe21` after D2a `/model`
and T1, and on `6a70012` after F5, D3 and T2: every `.txt`, `.ansi` and
`.files` is byte-identical to the ones here apart from the welcome line's
revision -- except, on `6a70012` only, `esc.ansi`, whose hint row no longer
carries `esc again to clear`. That cue lives for `CLEAR_WINDOW` (500 ms,
`src/tui/gesture.rs:67`) and the scenario snaps 0.5 s after the key, so
whether it is still up is a race the snap loses under load; no code on the
Escape path changed. Sanitized: the only run path in any file is the workspace root in
fx's `history.jsonl` and `session.json`, already `<scratch>/work`; no capture
names a scratch directory.

## How the pairs were compared

Each `.txt` and `.ansi` pair, row by row, after three normalisations and
nothing else: row 1 (the welcome line, `𝒇x v0.0.13` against
`x𝒇x v0.1.0-b9c5591 [debug]`) is dropped; fx's two usage rows naming
`remember` and `revoke` are dropped; trailing blanks are trimmed. Under that,
13 of the 16 snaps are identical in text **and** SGR. The three that are not
are the full-access warning (two snaps, one column) and the Escape snap,
explained below.

## Per snap

| snap | fx | xfx | matches / differs |
|---|---|---|---|
| `bare` | `* permissions: mode=auto`, `configured rules: (none)`, `session grants: (none)`, the 3-row usage; blank; `* permissions: saved-session permission rules: none`; footer `auto · model-a` | the same with a 1-row usage | Identical but the two usage rows (`/permissions remember …`, `/permissions revoke …`): xfx has no saved-session rule store, so it does not advertise the forms that manage one. Every row `\e[38;5;250m`; footer `\e[38;5;252mauto\e[38;5;245m · model-a\e[39m` on both. |
| `down1` | unchanged screen | unchanged screen | Identical (normalised). Down at an empty composer does nothing visible on either. |
| `esc` | unchanged; footer `auto · model-a` | footer `auto · model-a … esc again to clear` | **Differs, not this unit's surface**: xfx arms the double-Escape clear on an empty composer; fx does not (`!draftHasState` disarms and returns, `app_input_runtime.zig:3210-3213`). Pre-existing gesture behaviour; reported, not changed here. The snap is 0.5 s after the key and the cue's window is 500 ms, so a retake can catch it gone. |
| `auto` | `* permissions: mode set to auto` | same | Identical. `settings.json` after: fx `{"permission_mode":"auto"}` + `\n` (27 bytes); xfx `{\n  "permission_mode": "auto"\n}\n` (32 bytes). |
| `full` | `\e[38;5;252m! permissions:\e[38;5;250m mode set to full access`; footer `\e[38;5;252mfull access\e[38;5;245m · model-a\e[39m` + padding + `\e[38;5;252mFull access enabled: fx permission checks disabled`, flush with column 120 | the same with `xfx` in the warning, so it starts at column 70 instead of 71 | Notice identical. Footer: same mode accent, same 252 warning flush right, ending at column 120 on both; the sentence names this product (one column longer). One SGR nuance: fx closes the row colour (`\e[39m`) before the padding, xfx keeps 245 over the padding spaces -- no visible cell differs. `settings.json` after: fx `{"permission_mode":"yolo","yolo_acknowledged":true}` (52 bytes), xfx `{\n  "permission_mode": "yolo",\n  "yolo_acknowledged": true\n}\n` (61 bytes). Both recorded the acknowledgement with the first full-access frame. |
| `ask` | `* permissions: mode set to ask`; footer `\e[38;5;245mask · model-a\e[39m` | same | Identical. `settings.json`: fx 51 bytes, xfx 60 bytes, both `ask` + `yolo_acknowledged: true`. |
| `reset` | `* permissions: permissions reset to ask, session grants cleared`; footer `ask · model-a` | same | Identical. `settings.json` unchanged from `ask` on both (fx also keeps a backup per write under `~/.fx/backups/`; xfx keeps none -- its writer stages and renames, it does not version). |
| `bogus` | `\e[38;5;252m✗ \e[38;5;250musage: /permissions [ask\|auto\|full-access\|reset]` + 2 continuation rows | the first row only | Identical but the two `remember`/`revoke` rows, for the reason above. |
| `bare-after` | `mode=ask` listing | same | Identical (normalised). |
| `start` (btab) | footer `auto · model-a` | same | Identical. |
| `btab-full` | footer `full access · model-a` + the warning | same, `xfx` in the warning | As `full`: one column. No notice in the document on either -- Shift-Tab says it only on the row. `settings.json` `yolo` + `yolo_acknowledged: true` on both. |
| `btab-full-later` | 4.5 s later: `full access · model-a`, warning gone | same | Identical: the warning's 4 s of screen time are spent on both. |
| `btab-ask` | `\e[38;5;245mask · model-a` | same | Identical. `settings.json` `ask` on both. |
| `btab-auto` | `\e[38;5;252mauto\e[38;5;245m · model-a` | same | Identical. |
| `btab-full-again` | `full access · model-a`, **no** warning | same | Identical: shown once per profile, not once per entry. |
| `bare` (btab) | `mode=full access` listing | same | Identical but the two usage rows. `settings.json` ends `yolo` + acknowledged on both. |

## What the receipt does not show

- **A change while a turn is running.** xfx applies it between turns: the
  running turn finishes under the mode it started with and whatever was queued
  behind it starts under the new one (`src/tui/worker.rs` tests
  `a_change_sent_after_work_was_queued_is_applied_before_that_work_starts`,
  `a_change_lands_while_the_loop_is_idle_and_one_typed_before_leaving_is_not_lost`).
  fx applies it to the running turn's later tool calls too
  (`tests/e2e/yolo-permission-mode.test.ts:373-450`). While a turn runs, xfx's
  row therefore shows the mode of the next turn, not of the one in flight.
- **Grants in the bare listing.** No turn runs in either scenario, so both list
  `session grants: (none)`. The listing of real grants (` - write_file -> a.txt`,
  workspace-relative, made inert) is covered by
  `a_bare_permissions_lists_the_grants_the_runtime_published` and
  `a_grant_is_listed_inert`.
- **The line-oriented shell** (`XFX_TUI=0`), which answers the same forms in
  fx's `[permissions] …` text lines and writes the same key:
  `tests/interactive.rs` `slash_permissions_says_the_mode_changes_it_and_records_it`.
