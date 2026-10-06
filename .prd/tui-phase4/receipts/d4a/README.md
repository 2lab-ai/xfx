# D4a receipt: `/permissions` and Shift-Tab

Three scenarios, run at 40x120 on both binaries (the third added with the
follow-ups: a change made while a turn runs):

```
FX_BIN=<fx c1db919 build, 0.0.13> scripts/fxcap/capture.sh fx  permissions      40x120 <dir>
                                  scripts/fxcap/capture.sh xfx permissions      40x120 <dir>
FX_BIN=<fx c1db919 build, 0.0.13> scripts/fxcap/capture.sh fx  permissions-btab 40x120 <dir>
                                  scripts/fxcap/capture.sh xfx permissions-btab 40x120 <dir>
FX_BIN=<fx c1db919 build, 0.0.13> scripts/fxcap/capture.sh fx  permissions-midturn 40x120 <dir>
                                  scripts/fxcap/capture.sh xfx permissions-midturn 40x120 <dir>
```

All three were retaken together on `8826458` (plan `0d41a22` merged, with its
fx sound guard in `capture.sh`); the xfx files of the first two are
byte-identical to the earlier takes apart from the welcome line's revision, and
fx's differ only in the timestamps, session ids and backup names fx writes.

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

## A change while a turn is running (`permissions-midturn`)

Turns `scenarios/permissions-midturn.json`: the model writes `first.txt`, then
-- its second step held open 4.5 s (`hangMs`) -- asks to write `second.txt`,
then says `Done writing.`. The scenario sends the prompt, snaps the turn after
the first write (`first-done`), presses Shift-Tab twice -- the harness's
`auto`, full access, `ask` -- while the second step is still held (`switched`),
snaps when the second write's approval is up (`second-asks`), refuses it with
Ctrl-C (`after-refusal`), and lists the workspace (`work-end`) and HOME
(`end.files`).

| snap | fx | xfx | matches / differs |
|---|---|---|---|
| `first-done` | `└ Wrote first.txt +1`; the activity row; footer `\e[38;5;252mauto\e[38;5;245m · model-a` | same | The first write ran unasked in `auto` on both. Row 8, the activity row, differs in two things that are not this unit's: the blinking marker's phase at the snap (`•` against a blank) and the output count (`↓40` against `↓0` -- the activity row's token accounting). |
| `switched` | footer `\e[38;5;245mask · model-a` | same | **The footer says `ask` while the turn is running**, on both. Row 8 as above. |
| `second-asks` | the approval band for `second.txt` (`Write · +1 -0`, `1 Apply once` …) | same band | **The running turn's second write is put to the person** on both: the switch governs that turn's next call. The band's option 2 and the explanation row under it are xfx's own approval wording (F2's surface), two rows longer. |
| `after-refusal` | `● 2 tool calls · 2 write · 1 denied`, `├ Wrote first.txt +1`, `└ Denied file`, `Done writing.` | same | Identical but the summary's output count (`↓120` against `↓40`, as above). `work-end`: only `first.txt` on both. |

xfx's `end.files` shows the record that followed: the turn's events, then --
between pieces of work, once the turn had ended -- `preferences_changed` to
`yolo` and to `ask`, in the order the keys were pressed, and `settings.json`
`{"permission_mode": "ask", "yolo_acknowledged": true}`. The mode governed the
turn from the keystroke; the log and the profile record it afterwards.

## What the receipt does not show

- **A reset during a turn, and its durable record.** A reset takes every
  earlier "always" out of force at once and the runtime writes
  `permission_grants_cleared` so a resume does not restore them; no scenario
  here holds a grant. `src/tui/worker.rs`
  `a_reset_mid_turn_takes_the_grants_away_from_that_turns_next_call` and
  `a_reset_is_written_down_so_a_resume_does_not_bring_the_grants_back`, and
  `src/session/store.rs` `a_reset_withdraws_every_grant_before_it_and_none_after_it`.
- **The shadow line.** The harness pins `XFX_MODEL` and no permission
  override, so no save is outranked here. `src/tui/worker.rs`
  `a_mode_a_higher_layer_outranks_is_saved_and_says_so_in_upstreams_words`
  (TUI) and `tests/interactive.rs`
  `slash_permissions_says_when_a_higher_layer_still_decides_a_fresh_session`
  (line shell).
- **Grants in the bare listing.** No turn runs in either scenario, so both list
  `session grants: (none)`. The listing of real grants (` - write_file -> a.txt`,
  workspace-relative, made inert) is covered by
  `a_bare_permissions_lists_the_grants_the_runtime_published` and
  `a_grant_is_listed_inert`.
- **The line-oriented shell** (`XFX_TUI=0`), which answers the same forms in
  fx's `[permissions] …` text lines and writes the same key:
  `tests/interactive.rs` `slash_permissions_says_the_mode_changes_it_and_records_it`.
