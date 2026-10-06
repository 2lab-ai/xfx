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
7. Approval keys: xfx's Esc = deny / Ctrl-C = interrupt the turn → fx's Esc = cancel the turn / Ctrl-C = deny this call (`app_input_runtime.zig:3139-3143`, `src/ui/input/runtime.zig:61`, pinned by fx `tests/e2e/tui-permissions.test.ts:1099`).
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

- `UPSTREAM.md` pins `580a0c5`; this workstream cites `c1db919` for every UI claim. The pin advances to `c1db919` for the UI ledger in this workstream rather than leaving two revisions implicit.
- **Resize.** fx re-solves and repaints the frame it owns after a resize (`resize_runtime.zig`, `frame_layout.solveWithPreservedRowRelease`). xfx pins its band to the bottom of the resized screen instead of floating it (`Shell::resize` sets the document past any screen): the rows above the band are the terminal's own document after a reflow, and xfx has no way to know where a terminal put them, so a band floated under a guess would paint over them. Floating resumes after the next `/clear`. Captures at a fixed size match; a resize capture differs by where the band sits until the screen has filled.
- **Partial lines while streaming.** fx shows an answer's line once the markdown presenter has its end (captured mid-stream: only the activity row is visible). xfx does the same since 3efe232; xfx's earlier per-character pacing of partial lines is gone with it.
- **Tool backend is not UI.** xfx's `edit_file` refuses a file the session has not read (`src/tools/mutate.rs:547`); fx edits directly. That is a safety rule of xfx's tool layer, not a screen, so this workstream keeps it; paired captures of an edit without a prior read diverge for that reason and the receipts say so.
- **The window title on exit.** fx clears it (`ESC]2;BEL`; an empty pane title after `/quit`). xfx clears it and then pops the title it pushed at launch (`term::RESTORE`), so a terminal with a title stack gets the user's own title back and one without ends cleared, as fx's does. Kept: giving back what was borrowed is restore hygiene, not a screen this workstream copies.
