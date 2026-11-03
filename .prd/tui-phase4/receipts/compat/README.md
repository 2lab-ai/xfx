# COMPAT — what the Phase 4 build does to sessions the preview wrote, and back

Part of the receipt for shipping Phase 4 to main (`.prd/07-fx-parity-program.md`, Phase 4 close-out):
before the new build runs on a real profile, what happens to the sessions already in it.

- **old** = the installed preview, `xfx-preview 2026.10.06.115638.37459773015.1`
  (`/opt/homebrew/bin/xfx`, `--version` `0.1.0`); its `status` says `build_channel=preview`,
  `build_revision=14ad04202fe5` -- main at Phase 3, the tag
  `preview-2026-10-06-115638-37459773015-1-14ad04202fe5`.
- **new** = `plan/tui-phase4` at `de6618de`, `cargo build --locked --release`; `status` says
  `build_revision=de6618debf54`.

Every run was in a scratch `HOME` (never the real `~/.xfx`), with a fake key, against the loopback
fake gateway (`scripts/fxcap/fakegw.ts` under `bun`, which also answers as a llmux daemon), the TUIs on
a private tmux socket. Before and after every operation the whole `HOME` was snapshotted -- every file
and directory: path, size, SHA-256, `mtime` in ns, mode -- and diffed.

## The answer

1. **No path rewrites a byte the preview wrote.** Across every read, resume, TUI open, continued turn
   and quit below, each old `events.jsonl` after the operation **starts with its exact old bytes**;
   anything new is appended, and `session.json` is replaced atomically (stage + rename), as every
   append of either binary has always done. The one operation that removes bytes is the crash-tail
   cut on a writable open (`file.set_len(manifest.event_log_bytes)`, new `src/session/store.rs:2180`,
   the same as old `store.rs:1321`): it removes only bytes past the published boundary, which no
   reader ever shows -- measured, a 33-byte torn tail removed and the published 4 467 bytes kept.
2. **LOUD -- opening an old session in the new TUI writes to it, before any prompt.** *Fixed on
   `fix/p2-resume-prefs` (G-resume-prefs): the open writes nothing -- no frame, no republish, not even
   the log's `mtime` -- and the TUI and `ask --resume` keep the session's model on the provider it
   recorded, as fx does (`app_session_runtime.zig:6147-6151`, `cli_ask.zig:1035-1042@c1db9191`); the
   delta below is written ahead of the first turn instead. What follows is `de6618de` as measured.* Picking a
   session in `/resume`, or launching `xfx --resume <id>`, and then quitting without typing anything,
   **appends one `preferences_changed` frame and republishes the manifest**
   (`src/tui/worker.rs:2376-2399`). It always carries `provider`, because no log the preview wrote
   names one; and it carries `permission_mode` and `model` too when the launch's differ from the
   session's -- **the TUI does not restore a session's model or mode on open, it records the
   launch's over them.** Measured with no `XFX_MODEL` override, the profile as the preview's own
   `setup` left it:

   | session (as the preview left it) | profile | appended on open, no prompt |
   |---|---|---|
   | plain (`fake/model-b`, `yolo`) | llmux, `fake/model-a` | `{"model":"fake/model-a","permission_mode":"auto","provider":"llmux"}` |
   | band (`fake/model-a`, `ask`) | llmux, `fake/model-a` | `{"model":null,"permission_mode":"auto","provider":"llmux"}` |
   | plain (`fake/model-b`, `yolo`) | gateway, `zai/glm-5.2` | `{"model":"zai/glm-5.2","permission_mode":"auto","provider":"gateway"}` |
   | band (`fake/model-a`, `ask`) | gateway, `zai/glm-5.2` | `{"model":"zai/glm-5.2","permission_mode":"auto","provider":"gateway"}` |
   | tools, failed, llmux (`fake/model-a`, `auto`) | llmux, `fake/model-a` | `{"model":null,"permission_mode":null,"provider":"llmux"}` |

   The frame is a kind the preview knows and a field it ignores, so **the preview still opens and
   continues every one of them** (checked: `session --id` exit 0 and `ask --resume-id` exit 0 on all
   five afterwards, and the new build reads what the preview appended after that). It is still a
   durable write on open, and the one irreversible step in this receipt: from that frame on, the
   session's recorded mode and model are the launch's (band's `ask` reads back as `auto`, plain's
   `fake/model-b` as the profile's model), for this build and the preview alike; undoing it means
   cutting the frame out of the log and republishing the manifest by hand. A second
   open of a session that already names its provider appends nothing; its `events.jsonl` keeps its
   bytes but its `mtime` moves (the same-length `set_len` + `sync_data`, measured on macOS/APFS).
   `ask --resume-id` writes the same frame (`src/app.rs:423-441`), but only in front of the turn it
   was asked for.
3. **A resumed old session asks for no generated title.** Five continued TUI turns under the
   gateway -- the provider titles are asked of -- one per old session, with the fake gateway ready to
   answer a title: zero title requests in its log, no `session_renamed` appended. In the same
   harness a fresh session's first prompt does get one (`titles.cap`: `{"kind":"session_renamed","title":"Fix the renderer loop","source":"generated"}`).
4. **New → old: a new event kind is refused whole, cleanly; a new field is read past.** For the six
   new kinds the preview prints, and exits 1,

   ```
   xfx: session `<id>` cannot be trusted and was not read: cannot decode an event frame: unknown variant `session_renamed`, expected one of `session_started`, `workspace_rebound`, `preferences_changed`, `project_context_recorded`, `user_message`, `assistant_message`, `tool_result`, `tool_feedback`, `permission_grant_recorded`, `usage_recorded`, `turn_concluded` at line 1 column 188
   ```

   for `session --id` and the same sentence for `ask --resume-id`, and **leaves the session byte- and
   `mtime`-identical** (its resume replays before it truncates, old `store.rs:1316-1323`). Its
   `sessions` still *lists* such a session (from the manifest, `skipped_invalid=0`) under the title the
   new build gave it -- so `xfx ask --resume last` on the preview, when that session is the newest,
   exits 1 with the same sentence rather than skipping it (measured; nothing changed). Never a partial
   read. Every new field on a kind it knows is ignored (no `deny_unknown_fields` on
   the variants, old `event.rs:96-98`): the preview shows and continues the session, appending frames
   without the field, and the new build reads the result.

## The table

"Written by" is what in the new build writes it, as measured. "Old reading new" is the preview on a
log that holds it. "New reading old" is this build on a log the preview wrote. "Rewrite" is whether
opening an existing log rewrites any of its bytes.

| event / field | written by (new) | old reading new log | new reading old log | in-place rewrite? |
|---|---|---|---|---|
| `session_started` | every session | reads | reads | no |
| `workspace_rebound` | `--resume-id` from another workspace | reads | reads | no |
| `preferences_changed` (`model`, `permission_mode`) | resume, `/model`, `/permissions` | reads | reads | no |
| `project_context_recorded` | every turn | reads | reads | no |
| `user_message` | every turn | reads | reads | no |
| `assistant_message` (incl. `raw_content` + `wire`) | every turn | reads | reads; llmux signatures sent back verbatim | no |
| `tool_result` (`call_id`, `tool`, `ok`, `output`) | every tool call | reads | reads | no |
| `tool_feedback` | an amended approval | reads | reads | no |
| `permission_grant_recorded` | an "always" | reads | reads | no |
| `usage_recorded` (`input_tokens`, `output_tokens`) | every answered turn | reads | reads | no |
| `turn_concluded` (`final`; `interrupted{reason}`) | every turn | reads | reads | no |
| **`images_attached`** | `/image` then a prompt (`image.cap`) | **refused whole**, exit 1, nothing changed | -- (old cannot write it) | no |
| **`tool_images_attached`** | `read_file` of a PNG (`image-readfile.cap`) | **refused whole** | -- | no |
| **`steered`** | a prompt typed into a running turn (`interrupt-steer.cap`) | **refused whole** | -- | no |
| **`permission_grants_cleared`** | `/permissions reset` | **refused whole** | -- | no |
| **`session_renamed`** | `/rename` (`rename.cap`) | **refused whole** | -- | no |
| `session_renamed.source` = `generated` | title generation (`titles.cap`) | refused whole (the kind, not the field) | -- | no |
| **`context_checkpoint`** | `/compact` (`compact.cap`) | **refused whole** | -- | no |
| `context_checkpoint.compacted.open` | a cut inside a running turn (`compact-inturn-xfx.cap`) | refused whole (the kind, not the field) | -- | no |
| `session_started.provider` | every session | read past; continues | -- (absent; reads as no provider) | no |
| `preferences_changed.provider` | **any resume of an old log, TUI open included** | read past; continues | -- | no (appended) |
| `preferences_changed.effort` / `fast_mode` / `ultrafast_mode` | `/model … high` then a turn (`model-effort.cap`) | read past; continues | -- | no |
| `assistant_message.model` | a turn with sealed state (llmux) | read past; continues | -- (absent; reads as the model then in force) | no |
| `tool_result.ending` (`exited`, `denied`) | a command that exits 1; a refused `rm` | read past; continues | -- (absent; plain row) | no |
| `tool_result.lines` / `change` | `edit_file` | read past; continues | -- | no |
| `usage_recorded.turn_output_tokens` / `requests` | every answered turn | read past; continues | -- (absent; legacy usage) | no |
| `usage_recorded.unaccounted` | a stopped turn, a steer | read past; continues | -- | no |
| `usage_recorded.lines_added` / `lines_removed` | a turn that edited | read past; continues | -- | no |
| `usage_recorded.settled` | a deferred bill answered (`usage-deferred.cap`) | read past; continues | -- | no |
| `usage_recorded.rejected` | *synthetic frame* (see below) | read past; continues | -- | no |
| `turn_concluded.terminal_reason` (`cancelled`, `failed`) | Ctrl-C; a `400` | read past; continues | -- (absent; told by `reason`) | no |
| `turn_concluded.cancellation_origin` = `compaction` | *synthetic frame* (see below) | read past; continues | -- | no |
| `created_at_ms` | not an event field: it is the manifest's, the same struct in both (`SessionManifest`, old `store.rs:607-630`, new `:1357-1380`) | reads | reads | no |

"Refused whole, exit 1, nothing changed" holds for every bold row: `session --id` and
`ask --resume-id` both exit 1 with the sentence above, and the session directory's bytes, `mtime`s and
modes are identical afterwards. "Read past; continues" means `session --id` exit 0 and
`ask --resume-id` exit 0, the preview appending its own turn after the new frames; the new build then
read each of those continued logs (`session --id` exit 0).

**Synthetic frames.** Two fields were not reached live and were spliced into a real log the new
build wrote (a Ctrl-C'd turn), manifest republished: `cancellation_origin:
"compaction"` beside the interrupted outcome's `terminal_reason` (the shape
`src/session/event.rs` encodes, `CancellationOrigin::Compaction`, written by
`src/agent/machine.rs:878`), and `rejected: ["gen_never_billed"]` on its usage frame. The new build
reads both (`session --id` exit 0) before the preview is asked.

**What the preview cannot write at all**, so the "new reading old" column is empty for those rows:
it has no `/image`, `/rename`, `/resume`, `/compact`, `/permissions` (its own test
`help_advertises_no_deferred_upstream_command`, `14ad0420 src/interactive.rs:1246-1266`), and its
`SessionEvent` has none of the six kinds (`14ad0420 src/session/event.rs:98-192`).

**The ledger's list, against this preview.** `images_attached`, `tool_images_attached` and any
checkpoint are refused, as the ledger says -- but the checkpoint's `open` and the rename's `source` are
moot, because this preview has no `context_checkpoint` and no `session_renamed` at all.
`usage_recorded.turn_output_tokens`, `cancellation_origin`, `terminal_reason`, `provider`, and
`tool_result` `ending`/`lines`/`change` are **not** refused by this preview: they are read past.
`created_at_ms` names no event field.

## What else the new build leaves in the profile

Beside `sessions/`, new files only, never a rewrite of one the preview wrote: `history.jsonl` and
`history.lock` (the TUI's prompt history -- the first thing submitted, slash commands included, so
typing `/resume` creates them), `usage.json` (`{"tracking_started_at_ms":…}`, at the first turn), and
`settings.lock`. `settings.json` changed only on a user's command (`/permissions`, a `/model` commit,
`setup`). The preview ran `sessions`, `session` and `ask --resume-id` in profiles holding all of them
(observed, not proven over every settings key).

## Old → new, step by step

- **Read-only, 16 commands** -- `sessions`, `sessions --all`, `sessions --all --json`, `session last`,
  `status`, `doctor`, and `session --id <id>` with and without `--json` for each of the five: **zero
  differences** in the snapshot, `mtime`s and modes included. Every one exit 0, every turn replayed.
- **TUI, no prompt** -- launch: nothing. `/resume` picker shown: only `history.jsonl`/`history.lock`
  created. A row picked: the `preferences_changed` above, the replay on screen (the band session's two
  writes, `keep it short`, `■ Cancelled`); `Ctrl-O`: the full transcript (contents of both writes, the
  llmux read's lines); `Esc`, `/quit`: nothing more. `xfx --resume <id>` on all five: the same.
- **Continued** -- `ask --resume-id` and the TUI with one prompt, under gateway and under llmux, on all
  five: appended `preferences_changed{provider}` (plus `model`/`permission_mode` where they differed),
  `project_context_recorded`, `user_message`, `assistant_message`, `usage_recorded`
  (with `turn_output_tokens` and `requests`), `turn_concluded` -- all kinds the preview knows, the old
  bytes the head of every log, and the preview reads every one of them after. On llmux the next
  request carried the preview's thinking blocks with their signatures (`sig-fake-one`, `-two`,
  `-three`); on gateway the llmux session's reasoning dropped with the notice the preview prints too.

## How the old sessions were made

With the preview, `HOME=<scratch>`, through `ask`, `ask --resume last`, `ask --resume-id` (one from a
second workspace), `ask --auto` with `read_file`, `edit_file`, `terminal` admitted and refused,
`glob_files`, `grep_files`, `list_files`, `write_file`, `create_folder`; a turn the gateway answered
`400`; `XFX_MODEL=fake/model-b … --yolo`; the `XFX_TUI=1` band in `ask` mode ("Yes, and don't ask
again", Tab + `keep it short`, Ctrl-C); and `setup llmux --url` with signed thinking. Together the
five logs hold all eleven kinds the preview knows. They are committed, workspace paths written
`@WS_A@`/`@WS_B@` and nothing else changed, as
`tests/fixtures/preview-14ad04202fe5/sessions/<id>/{events.jsonl,session.json}`.

## Regression tests

`tests/session_compat.rs`, six tests over those fixtures (the manifest's byte count and digest
recomputed after the paths are put in):

- `the_fixtures_hold_every_event_kind_the_preview_could_write` -- the fixture set is the eleven kinds,
  with a failed and a cancelled turn and signed llmux reasoning.
- `every_session_the_preview_wrote_lists_and_replays_whole` -- `sessions --all --json` lists five,
  none skipped; each `session --id --json` has every turn and outcome; the grant, the amendment, the
  cancel, the `400`, the model and mode change and the rebind read as the preview wrote them.
- `the_previews_signed_reasoning_goes_back_to_llmux_verbatim` -- a resumed llmux turn sends the three
  signatures back.
- `reading_a_session_the_preview_wrote_changes_nothing_on_disk` -- bytes, `mtime` and mode of every
  entry under `HOME`, unchanged after each of the 16 read-only commands.
- `continuing_a_session_the_preview_wrote_appends_only_frames_the_preview_reads` -- `ask --resume-id`
  on all five: old bytes the head of the log, every appended kind one of the preview's eleven, the
  boundary moved, the saved history sent first.
- `opening_a_session_the_preview_wrote_in_the_tui_and_leaving_it_writes_nothing` -- `xfx --resume`
  then `/quit`, on a real pty: every file of the session identical, `mtime`s included; no request, no
  title. (At `de6618de` it asserted the append-only property of the open's frame; G-resume-prefs
  tightened it to no write at all.)

Red, by mutation: with `preferences_changed` taken out of the test's list of the preview's kinds,
the guard, the continuation and the TUI-open tests fail -- ``appended a `preferences_changed`, which
the preview refuses with the whole log`` -- and restored, 6/6 pass.
The last two are what would catch a future build that writes a new kind into an old session on open
or on its next turn -- the change that would take that session away from a preview installed beside
it.
