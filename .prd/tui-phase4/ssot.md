# TUI Phase 4 — SSOT

Status: in-progress
Date: 2026-10-06
Base: `14ad04202fe54ff266fca0ca94c93a74a510e927` (main after PR #28)
UI reference: vercel-labs/fx `c1db9191` (`v0.0.13`+33, upstream `main` HEAD on 2026-10-06), built locally with zig 0.16.0
Report channel: this conversation

## Verbatim dictation

> 최신 fx랑 비교해서 ui쪽으로 모든 기능 구현하고 기본적으로 최신 fx ui를 그대로 구현해줘

Carried constraints from the earlier goals in this conversation, unchanged: preview channel only (no `v*` tag, no stable release); fake credentials and loopback in fixtures; do not restart the user's llmux daemon or touch the user's default tmux server; the Codex/Grok OAuth decision (zbrain open-loop #88) is the user's.

## What "그대로" means here

The oracle is the **running fx binary at `c1db919`**, side by side with xfx in the same terminal size, not a reading of fx's source. Source is read to find the rule behind what the screen shows. Every UI round closes on a fx↔xfx capture pair taken on an isolated tmux socket.

The first capture (120×40 and 80×24, isolated `HOME`, fake key) established the frame architecture:

| | fx `c1db919` | xfx `14ad042` |
|---|---|---|
| launch | bare `fx` on a terminal is the TUI | needs `XFX_TUI=1` |
| buffer | normal buffer, native scrollback | same |
| start position | the footer starts on the row under the shell's cursor and floats under the transcript; once the screen fills it pins to the bottom and the transcript scrolls into native scrollback | the band is anchored to the last row from the first frame |
| header | `𝒇x v0.0.13 · Run /help for commands` (`𝒇x` bold 255, rest 245) as the first transcript row | none |
| footer (idle) | `[top divider, composer, picker divider, hint]`; dividers blank when idle; composer `┃ ` (255); hint `auto · grok-4.7` (`auto` 252, rest 245) | `[divider ─── 240, composer "> ", hint 255]` |
| slash menu | dividers become `────`; `Commands 36 · type to filter` + range `1–6`; rows `  /cmd  description  Category`; footer `↑↓ navigate     enter use     esc close` | a flat list of 7 rows above the divider |
| `/help` | opens the help menu | prints a list into the transcript |
| `/status` | `* status: model=… update_channel=… …` domain notice | refused as unknown |

The fx footer rule is `src/ui/render_engine/footer_layout.zig:39-96`: `top = min(transcript cursor + activity offset, rows - reserved)`; `input_base = top_divider + composer_top_chrome_rows + input_extra`; `picker_divider = input_base + 1`; `bottom_divider = picker_divider` with no picker, else `picker_divider + 1 + picker_rows`; `hint = bottom_divider + 1`.

## Deliberate xfx divergences this workstream reverses

Recorded so the weekly review sees them; "그대로" authorizes each.

1. Opt-in `XFX_TUI=1` → the TUI is the default on a terminal; pipes and subcommands keep the line shell/commands.
2. A band anchored to the last row → fx's floating footer.
3. The `───` divider and `> ` prompt → fx's blank dividers, `┃ ` composer and hint row.
4. The 7-row slash list → fx's filterable menu.
5. `/help` as transcript text → fx's help menu.
6. `/clear` as "clear the screen, keep the conversation" → fx's "start a fresh conversation while keeping managed processes" (`app_commands.zig:667`).
7. Approval keys: xfx's Esc = deny / Ctrl-C = interrupt the turn → fx's Esc = cancel the turn / Ctrl-C = deny this call (`app_input_runtime.zig:3139-3143`, `src/ui/input/runtime.zig:61`, pinned by fx `tests/e2e/tui-permissions.test.ts:1099`). The model's own questions (`ask_user_question`) likewise: xfx's Esc = decline the question, the model told so with the sentinel `(user cancelled the question)` and the turn going on → fx's Esc at an empty field = cancel the turn (`app_input_runtime.zig:3137` → `cancelQuestionPrompt`, `input_question_runtime.zig:59-83`), and Ctrl-C = the same (`src/ui/input/runtime.zig:138` `3 => .cancel` → `question_prompt.zig:327`); the call answers `aborted by user` (`session.zig:64`) and the model never receives the sentinel (fx e2e `tui-decision-prompts.test.ts:2454`). Esc at a freeform answer with text in it arms a clear instead (`app_input_runtime.zig:3120-3135`).
8. Ctrl-P: xfx's history-previous → fx's model catalog (`escape_parser.zig:271`); history stays on Up/Down as in fx.
9. Slash lines are not echoed into the transcript, commands with no argument match exactly, and anything else is `✗ command: Unknown command. Try /help.` (fx `input_submit_runtime.zig:707-717`, `command_router.zig:121-133`).

## Requirement universe

### A. Slash commands (fx registry `src/builtins/commands.zig:463-498`, 36 rows)

Class: **P** portable on xfx's existing backend · **B** needs a local backend xfx does not have yet, buildable here · **G** gated outside engineering (named gate) · **X** an fx-product surface with no xfx counterpart (named reason).

| # | Command | fx category | xfx today | Class |
|---|---|---|---|---|
| 1 | `/help` | General | transcript list | P |
| 2 | `/clear` | General | screen clear | P |
| 3 | `/new` | Session | yes | P |
| 4 | `/reset` | Session | no | P |
| 5 | `/resume` | Session | `ask --resume` only | P |
| 6 | `/rename <title>` | Session | no | P |
| 7 | `/login` | Account | no | G #88 |
| 8 | `/logout [vercel\|codex\|grok]` | Account | no | G #88 |
| 9 | `/provider` (`/setup`) | Account | `/setup <gateway\|llmux>` | P for the provider picker; sign-in rows G #88 |
| 10 | `/stats` | Account | no | P |
| 11 | `/usage` (`/cost`) | Account | no | P |
| 12 | `/status` | General | `status` subcommand only | P |
| 13 | `/image <path>` (`/img`) | Media | no | B (image parts) |
| 14 | `/images [clear]` | Media | no | B |
| 15 | `/model <id-or-query>` | Model | yes, no picker | P |
| 16 | `/permissions [ask\|auto\|full-access\|reset]` | Security | mode exists | P |
| 17 | `/allowlist …` | Security | in-memory rules | B (persisted) |
| 18 | `/undo` | Session | no | B (file-op journal) |
| 19 | `/mcp …` | Extensions | no | B (MCP client) |
| 20 | `/skills …` | Extensions | no | B (skills) |
| 21 | `/copy` | Session | no | P |
| 22 | `/feedback` | Product | no | X (opens Vercel's fx form) |
| 23 | `/trace` | Product | no | B (private trace) |
| 24 | `/compact` | Session | no | B (summarizing turn) |
| 25 | `/settings` | Appearance | no | P |
| 26 | `/alias` | Extensions | no | P (fx answers "Aliases are not yet configurable.") |
| 27 | `/credits` (`/balance`) | Account | no | B (Gateway credits endpoint) |
| 28 | `/paste` | Media | no | B (clipboard image) |
| 29 | `/fast` | Model | no | B (provider options) |
| 30 | `/ultrafast [on\|off\|status]` | Model | no | B |
| 31 | `/statusline [context\|session\|workspace]` | Appearance | no | P |
| 32 | `/sound [on\|off\|max]` | Appearance | no | P |
| 33 | `/workspace […]` | Workspace | per-invocation roots | B (persisted roots) |
| 34 | `/shell reload` | Workspace | no | B |
| 35 | `/version` | General | yes | P |
| 36 | `/quit` (`/exit`) | General | yes | P |

### B. Frame and interaction surfaces (fx `tests/e2e/tui-*.test.ts` are the behavioural spec read, not run)

| # | Surface | fx spec file |
|---|---|---|
| F1 | default launch, header, footer chrome, floating start | `tui-startup.test.ts` |
| F2 | slash menu: filter, paging, categories, keys | `tui-slash-menu.test.ts`, `tui-slash-commands.test.ts`, `tui-slash-extra.test.ts` |
| F3 | composer editing, navigation, line delete, keybindings | `tui-composer-edit-contracts.test.ts`, `tui-input-navigation.test.ts`, `tui-input-line-delete.test.ts`, `tui-keybindings.test.ts` |
| F4 | `@` file picker | `tui-file-picker.test.ts` |
| F5 | transcript rendering: user card, assistant text, tool groups, activity | `tui-agent.test.ts`, `tui-terminal-tool.test.ts`, `tui-compaction-activity.test.ts` |
| F6 | approvals and decision prompts | `tui-permissions.test.ts`, `tui-command-permissions.test.ts`, `tui-decision-prompts.test.ts` |
| F7 | full-transcript screen | `tui-full-transcript-brutal.test.ts` |
| F8 | resume picker and resume rendering | `tui-resume.test.ts`, `tui-resume-brutal.test.ts` |
| F9 | resize | `tui-resize.test.ts` |
| F10 | interrupt and stream recovery | `tui-interrupt-recovery.test.ts`, `tui-gateway-stream-lifecycle.test.ts` |
| F11 | native clear recovery | `tui-native-clear-recovery.test.ts` |

## Convergence

Done when every row above carries a status in `loop.md`'s gap matrix; every P and B row is met with a fx↔xfx capture pair plus an xfx test; every G and X row names its gate and shows **nothing** in xfx (no stub menu entry — `.prd/04-providers.md` §Policy risks); and the delivery chain of `.prd/06-qa-harness.md` holds (gates, independent subagent tmux QA against the fx binary, trinity, merge, preview, brew install, real-profile receipt).

## Tensions

- **The Gateway's model catalog without a credential.** fx falls back to the public catalog when it has no key (`fetchWithPublicFallback`, `model_catalog.zig`; the menu then says `Using the public model catalog; sign in or use an API key for team-private models.`). xfx asks for no catalog without a key (`provider::model::catalog_for` answers `None`): it does not contact a remote host on its own initiative, and a Gateway session with no credential cannot run a turn either, so `/model` there lists nothing (`No models available.`) and applies an id as typed. Kept: the fallback would buy a list of models the session cannot use, for an unsolicited request to a remote host.
- **The model catalog is read when something needs it, not at launch.** fx preloads its catalog when the session starts (`model_cache_runtime.zig`), so its footer settles on whether the model in force takes the session's effort within a second. xfx reads the catalog when `/model`, its column or a turn with a non-default effort first needs it (a launch makes no network call), so until then the footer shows the effort the way fx's does while its catalog is loading (`active_capabilities_pending`), and a typed `/model <word>` before any read is resolved -- and announced -- by the runtime rather than at once.
- **The catalog load is work, not control.** Opening `/model`'s column or menu mid-turn asks the runtime for a catalog it has not read; the runtime reads it after the turn (it is one socket, on the thread that owns the turn), so the column says `loading models...` until then. A model change itself is a control message and is never held up this way.

- `UPSTREAM.md` pins `580a0c5`; this workstream cites `c1db919` for every UI claim. The pin advances to `c1db919` for the UI ledger in this workstream rather than leaving two revisions implicit.
- **Resize.** fx re-solves and repaints the frame it owns after a resize (`resize_runtime.zig`, `frame_layout.solveWithPreservedRowRelease`). xfx pins its band to the bottom of the resized screen instead of floating it (`Shell::resize` sets the document past any screen): the rows above the band are the terminal's own document after a reflow, and xfx has no way to know where a terminal put them, so a band floated under a guess would paint over them. The next document row xfx writes lands on the row above the band, and from then on the document is counted as ending there (`commit_frame` counts what an append leaves **on** the screen, not what it scrolled off), so a band that shrinks afterwards keeps its top row and leaves the rows it gave back blank under the hint, as a floating band does -- it never moves up onto a row it did not hold. Captures at a fixed size match; a resize capture differs by where the band sits until the screen has filled.
- **Partial lines while streaming.** fx shows an answer's line once the markdown presenter has its end (captured mid-stream: only the activity row is visible). xfx does the same since 3efe232; xfx's earlier per-character pacing of partial lines is gone with it.
- **Tool backend is not UI.** xfx's `edit_file` refuses a file the session has not read (`src/tools/mutate.rs:547`); fx edits directly. That is a safety rule of xfx's tool layer, not a screen, so this workstream keeps it; paired captures of an edit without a prior read diverge for that reason and the receipts say so.
- **The window title on exit.** fx clears it (`ESC]2;BEL`; an empty pane title after `/quit`). xfx clears it and then pops the title it pushed at launch (`term::RESTORE`), so a terminal with a title stack gets the user's own title back and one without ends cleared, as fx's does. Kept: giving back what was borrowed is restore hygiene, not a screen this workstream copies.
- **Approval review scrolling.** fx scrolls a long review with the mouse wheel; xfx requests no mouse reports (the terminal keeps its own selection and scrollback), so its review walks with Ctrl-P/Ctrl-N and the hint says so. A draft's caret is fx's reverse-video cell (`approval_ui.zig:1652-1687`, `question_ui.zig:323-399`): the painter's vocabulary admits SGR 7/27 for it, and for the composer's Super+A selection (`pacer::is_palette_sgr`, `check::wire_color`). fx also hides the terminal's own cursor while a question or approval owns the footer (`paint_plan.zig:989`); xfx's frame ends by showing it (`frame::END_FRAME`, checked by `check`'s cursor-visibility rule), so it stands on the reversed cell.
- **Approval scope wording.** fx's choice 2 is "workspace file access"; xfx's grant is this tool on this file and outlives the process into a resumed session, so choice 2 says that and a dim `2 = …` row states the scope whole (the readiness gate requires the scope disclosed).
- **An image for a model without native image input.** fx has two routes for an attached image (`visionPolicy`, `orchestrator.zig:5971-5986`): inline `file` parts for a catalog row tagged `vision` and `file-input`, and for any other listed row a forced `vision` tool call answered by a second model, `google/gemini-2.5-flash` (`image_provider.zig:12`; captured in `research/media-ext/cap/fx-image-vision-40x120-bodies/`). xfx ports the inline route and the refusal of a model the catalog does not list (`Unable to verify image support for this model, …`, no request sent), and refuses the second route's models with fx's own words for that route when it is unavailable, `request failed: SubscriptionNativeImageUnavailable` (`orchestrator.zig:7152-7155`). The check covers the history's images as well as the prompt's, as fx's does (`collect_image_catalog`, `app_session_runtime.zig:3129`), so once a session has sent an image, every later turn on such a model is refused -- on fx too, for an unlisted model; for a listed one fx would route the history through the vision tool instead. Kept: the second route is a second model, a tool xfx does not advertise (`vision` is deferred in `docs/parity.md`) and a forced tool call that fails the turn when the model will not make it (`RequiredVisionToolCallMissing`, `fx-image-nonnative-40x120-sent`). llmux's catalog has no capability field, so there the image is sent and the daemon's 400 is the answer.
- **What an image attachment does not carry from fx.** The `[Image N]` badge is not an OSC 8 hyperlink to its file (`writeImageBadge`, `image_attachments.zig:1336-1340`) -- the painter draws no hyperlinks yet (the open F5 links row in `loop.md`). An image path typed or pasted into a prompt is the words it is; fx turns a pasted path into a badge (`input_paste_runtime.zig:369-376`, `:446-500`) and attaches typed ones at submit (`extractInlineImageAttachments`, `input_submit_runtime.zig:756-776`). An image over 5 MiB encoded or over fx's per-side pixel limit is sent, where fx withholds it with a note in the prompt (`withholdOversizedAttachments`, `image_attachments.zig:841-920`); the Gateway's own refusal is the answer then. `/paste` on Linux reads `xclip -t image/png`, where fx says nothing (`error.Unsupported`, `image_commands.zig:91`); an advertised command that silently did nothing would be a stub. Deleting a badge removes the image and leaves no file behind (fx keeps the snapshot it took, `research/media-ext.md` §2.3); xfx takes no snapshot file until the prompt is sent, then keeps it with the session.
- **The `@` file picker: what is left as it is.** Each of these is a difference from fx (`c1db919`) that this workstream records rather than closes:
  - *The prompt text is trimmed.* A chosen file ends `@path ` with a space, which fx sends as typed (`"text":"@docs/guide.md "`); xfx trims every prompt it sends (`src/interactive.rs` `classify`), so the request carries `@docs/guide.md`. Not a screen; the receipts show it in the request bodies (`receipts/e1/README.md`).
  - *No on-disk index cache.* fx paints a warm index from `<home>/.fx/file-index/` (`file_index_cache.zig:3`) before discovery and saves one after (`file_index.zig:803-821`, `:829-832`; `file_index_cache.zig:66`, `:141`). xfx discovers every time and keeps the index in memory only; its first index is searched as it is published, as fx's is, but it is never painted from a previous process's walk.
  - *One index root.* fx indexes the primary directory and every active additional directory (`activeRootsAlloc`, `file_index.zig:857-876`). xfx indexes the workspace only; extra roots come with `/workspace`.
  - *No `$` skills picker.* fx opens an inline skills list on a `$` word at the end of the draft (`findInlineSkillQuery`, `picker_state.zig:524-541`). xfx has no skills, so `$` is a character until `/skills`.
- **Interrupting a running command.** fx kills a running shell command on Ctrl-C at once; xfx's running tool holds the runtime thread and the stop is read when it returns (`src/tui/worker.rs` `raced_against_control`), so the row shows the real result under the interrupt notice rather than a `Cancelled` row for a stop that did not happen. Runtime behaviour, not a screen; recorded for the backend.
- **`/settings` · Session titles (left out).** fx's row (`settings_catalog.zig:272`) switches the generated session title on and off: `applySettingsCatalogChange` sets `session_title_generation` (`app_commands.zig:4150-4166`), which gates the title request fx sends after the first prompt (`session_title_generation.zig:23`, "Generate a short title for a conversation…"; read at launch, `app_lifecycle.zig:811`). xfx has no title generator -- a session's title is its first prompt's first line (`src/session/store.rs` `title_of`) -- so both values of the row would draw the same footer and the same `/resume` list. A switch with nothing behind it is a stub under `scripts/check-no-stubs.sh`, so the row is absent; it comes with a generator.
- **`/settings` · Fast mode and Ultra mode (left out).** fx's rows (`settings_catalog.zig:269-270`) toggle the speed tier a request asks for, with choices only when the model advertises one (`optionCount`, `:314-315`; applied by `toggleFast`/`handleUltrafast`, `app_commands.zig:4182-4196`). xfx sends no speed tier: D2b's `/model` refuses `fast` and `ultrafast` in its explicit form and has no speed column yet (D2c). A switch with no request field behind it would be inert, so both rows are absent until a speed is sent. `Reasoning effort` is in the menu: D2b's effort is in plan (87c9ed9).
- **`/settings` · Collapse tool calls switched off does not expand the groups on the screen.** fx re-renders the visible transcript both ways (`markTranscriptStructureDirty`, `app_commands.zig:4120-4133`). xfx re-projects the rows it wrote and can still describe: switched **on**, each group on the screen loses its call rows and cancelled lines where they stand -- a group whose header has already left the top of the screen too -- and the rows under them move up (`Band::collapse_groups`, read off the rows' recorded kinds; no scroll, nothing made up), as fx's does (`receipts/d4d/*-settings-collapse-40x120-collapse-on`); the group the turn is still writing is rewritten as its header and gives the rows it lost back, so what comes next lands under its blank row (`*-settings-collapse-open-*`). Switched **off**, only the open group is drawn again expanded: a group written collapsed never had its call rows on the screen, and the band holds no closed group's calls to put back -- a closed group is the terminal's document. And rows in scrollback are never touched: where fx re-renders a full screen and pulls older rows down into the space a collapse frees, xfx's top rows stay where they are and the freed rows are the band's (`*-settings-collapse-open-40x120-after-esc`: the same rows one row lower).
- **`/undo`: what is left as it is.** W6 ports fx's file-op journal (`ChangeTracker`, `src/core/workspace/change_tracker.zig:30-99`) in memory, as fx's is; four differences are deliberate:
  - *The read-proof rule holds for an undo.* fx puts the preimage back over whatever is at the path (`undoLast` → `restoreContent` → `writeFileAtomic`, `change_tracker.zig:53-81`, `:91-93`), and its own test replaces a symbolic link standing there with the preimage (`:501-537`). xfx holds an undo to the rule every one of its writes obeys: the file must still be what the change left there, or what the session last read, so a file edited by someone else since, or a link at the name, is `Could not undo <path>` and left as it is (`src/tools/mutate.rs` `take_back`). The change is spent either way, as fx spends it (`:53-57`).
  - *A byte bound beside the count.* fx keeps 100 entries whatever their size (`max_stack_size`, `change_tracker.zig:33`; `pushOperation`, `:45-51`). A preimage is a whole file of up to 10 MiB here (`ToolLimits::max_read_bytes`), so xfx also drops the oldest past 64 MiB of preimage, always keeping the newest (`src/tools/undo.rs` `MAX_TRACKED_BYTES`).
  - *A provider switch leaves the journal behind.* fx clears its tracker only on the live-session transition `/clear`, `/new`, `/reset` and `/resume` share (`app_session_runtime.zig:1966-2001`, the clear at `:1999`; reached from `resumeSelectedSession` by `prepareLiveSessionResume`, `:1922`); its `/provider` (`runProviderCommand`, `app_auth_runtime.zig:387`) keeps the session and the tracker. xfx's switch drops the conversation (`Runtime::adopt`, `src/tui/worker.rs`), and the journal lives in the conversation's tool session, so it goes with it. Follows whatever the "provider-switch session carry" decision in `loop.md` makes of the conversation.
  - *Mid-turn, the undo waits for the turn but is pinned to the keystroke.* fx runs `commandUndoLast` from its command handler at once whatever the worker is doing (`app_commands.zig:366-392`, `:1242-1290`). xfx's runtime thread does it once the running work has said its last word, never between one call's commit and the next call's read; the band sends the journal's position at the keystroke (`tools::undo::Mark`), so the change taken back is the one newest then, as fx's is, and a later change to the same file makes it `Could not undo` rather than a restore that discards that change. What differs from fx is only when the notice appears and the file changes: at the turn's end, not at the keystroke.
