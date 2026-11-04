---
title: SESS-EARLY receipt -- settings chosen before the first prompt reach the session log
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, session, preferences]
---

# SESS-EARLY receipt: the first save records the settings in force

Found by the EF1 worker (`feat/p5-effort-menu`,
`receipts/effort-menu/{fx,xfx}/…-effort-menu-40x120-end.files`): xfx's
session log held no effort chosen before the session's first prompt, only a
later change. Base: `feat/p5-status-panel` at `d3bc63d3`, which allocates the
session id at launch and opens the session at the first prompt.

## What fx does (`vercel-labs/fx@481104fd`)

- A new session starts in memory with its seed, the preferences, permissions
  and language in force (`beginFreshV2Session`,
  `src/core/app/app_session_runtime.zig:1481-1500`; `Seed`,
  `src/core/session/session_adapter.zig:854-862`; `appendSeed`, `:1289-1299`).
- Every `/effort`, `/fast`, `/ultrafast` and `/model` writes the whole
  `prefs` again (`applyFastMode` `src/core/session/session_commands.zig:842`
  -> `:870`, `applyUltrafastMode` `:891` -> `:928`, `applyEffort` `:952` ->
  `:957`, `selectModelFromPicker` `:1039` -> `:1067`, `setResolvedModel`
  `:1311` -> `:1321`; `commitRuntimePreferences`,
  `app_session_runtime.zig:2861-2915`; `setPreferences`,
  `session_adapter.zig:2151-2155`).
- Before the first turn a batch without `turn_started` may only hold
  settings, and they wait in memory, framed from seq 2 (`hold`,
  `src/core/session_manager/session.zig:415-429`). The first turn writes line
  1 (`session_created`, at the turn's time), then every held line with its own
  time, then the turn (`writeFirstTurn`, `:503-539`).
- A resume puts the effort and both switches back from the last `prefs`
  (`restoreRuntimePreferences`, `app_session_runtime.zig:5110-5158`), Fast
  bound only for the selection it ran under (`restoredFastModeModelBound`,
  `:5221-5231`).

`fx/…-first-save.files` shows it: seq 2-4 the seed (`effort":"auto"`,
`fast_mode":false`), seq 5 the `/model anthropic/claude-opus-5 high fast`
(`"effort":"high","fast_mode":true`), seq 6 `turn_started`.

## The fix

`Runtime::open_next` (`src/tui/worker.rs`) records the effort and both
switches in force (`record_settings`) as soon as the conversation is open:
one `preferences_changed` right behind `session_started`, ahead of the
prompt. `session_started` already carries the model, the provider and the
permission mode in force, so the frame carries none of them. It is written
whatever the settings are, as fx's seed is. `open_next` is the first prompt's
and a `/rename` before it's; nothing earlier writes, so a launch and
`/status` still write nothing.

xfx writes the state in force once, not one line per change as fx's held
lines are. The two fold to the same settings; xfx's log is its own format.

## RED (`d3bc63d3`)

- Unit, `src/tui/worker.rs` (`red-unit.txt`: the three tests added to `d3bc63d3`, the fix not yet):
  `settings_chosen_before_the_first_prompt_are_written_with_its_first_save`
  -- kinds `[session_started, project_context_recorded, user_message, …]`, no
  `preferences_changed`;
  `a_resume_puts_back_the_first_saves_settings_and_nothing_it_does_not_restore`
  -- `resumed.settings()` is `None`, not `low`+fast;
  `a_rename_before_the_first_prompt_opens_the_log_with_the_settings_in_force`
  -- one frame, not two.
- `tests/session_compat.rs`
  `a_session_the_tui_starts_holds_its_early_settings_in_frames_the_preview_reads`
  -- the same missing frame, through the binary in a pty (`red-pty.txt`: the
  test file at `65fa5a60` on `d3bc63d3`'s tree, `tests/session_compat.rs:756`):

  ```
  left: ["session_started", "project_context_recorded", "user_message", "assistant_message"]
  right: ["session_started", "preferences_changed", "project_context_recorded", "user_message"]
  ```
- Capture pair `p5-session-early-prefs` (`red/`, `red.txt`): `resumed` row
  11 and `continued` row 17 differ -- fx `auto · opus 5 · high`, xfx
  `auto · opus 5`. The request after the resume: fx
  `"reasoning":"high"`, `"speed":"fast"`; xfx neither
  (`red/…-bodies/003-completion.json`).

## GREEN

The xfx side (`xfx/`) is `target/debug/xfx` at `e93a55c`, its banner
`v0.1.0-e93a55c`; `red/` is the base's, banner `v0.1.0-d3bc63d`.

`green.txt`: all five pairs identical, row 1 left out -- `launch`,
`chosen`, `answered`, `resumed`, `continued`. The request after the resume
carries `"reasoning":"high"` and `"speed":"fast"` on both
(`fx/…-bodies/003-completion.json`, `xfx/…-bodies/003-completion.json`).
fx and xfx both resume Fast unbound: the reseeded profile holds no binding,
so neither footer shows `⚡︎` while both send Fast.

The scenario reseeds the profile between the two launches, so only the
session's log can carry what the first launch chose.

## Older binaries read the new log (`compat.txt`)

The frame is a `preferences_changed`, a kind every xfx log has held.
`model` and `permission_mode` are written as `null`; `effort`, `fast_mode`
and `ultrafast_mode` are fields the preview predates and reads past (no
`deny_unknown_fields`, `14ad04202fe5 src/session/event.rs:96-118`). Run on
the GREEN session, rebuilt into a scratch HOME from `xfx/…-end.files`
(`compat.txt`, a loopback fake gateway and a fake key):

- the preview, built from `git archive 14ad0420` and stamped as its publish
  workflow stamps it (`XFX_BUILD_CHANNEL=preview`, `XFX_BUILD_REVISION`,
  `.github/workflows/preview.yml:225-226`): its `status` says
  `build_channel=preview` and `build_revision=14ad04202fe5`, the revision
  `xfx-preview 2026.10.06.115638` carries, and the string is in the binary
  (`strings | grep -c 14ad04202fe5` = 3; `compat.txt:5-16`). `session --id`
  exit 0, the log's bytes
  unchanged; `ask --resume-id` exit 0, its frames appended behind the new
  build's bytes;
- the base, `d3bc63d3`: `session --id` exit 0;
- this build: `session --id` exit 0 on what the preview appended.

## Resume keeps what it does not restore

`a_resume_puts_back_the_first_saves_settings_and_nothing_it_does_not_restore`:
resumed under a profile on `high`, Normal, `yolo` and an `XFX_MODEL`, the
session comes back on its own `low` and Fast, the launch's `yolo`, and the
override's model.

## `/new` after a resume (SESS-EARLY-b)

`a_new_conversation_after_a_resume_records_the_settings_it_carries`: a session
saved in `low` and Fast, resumed, `/new`, one prompt. The new log's first save
says `low` and Fast, and the request is sent with `reasoning` `low` and Fast.
fx's `/new` keeps the resumed effort, Fast and selection in force too
(`newSession` -> `applyIdleLiveSessionTransition`,
`src/core/app/app_session_runtime.zig:1524-1526`, `:1788-1833@481104fd`), but
seeds the new log from the workspace's preferences (`beginFreshV2Session`,
`:1481-1487`), which a resume does not move. xfx's log names what is sent:
the recorded divergence in `docs/parity.md`'s `/new` row (owner the
dispatcher, expires `date:2026-12-04`). The test that was
`a_new_conversation_after_a_resume_carries_no_resumed_selection` resumed a
session with no settings, so it never checked that; it is now
`a_new_conversation_after_a_resume_starts_its_own_log_and_leaves_the_resumed_one`.

`.ansi` files are tmux's re-serialization of cell attributes. Local paths are
redacted with `scripts/fxcap/redact.py`.
