# TUI Phase 4 — loop

Status: in-progress
Branch: `plan/tui-phase4` (worktree `.worktrees/plan-tui-phase4`), base `14ad042`
Reference binary: fx `c1db919` built with `zig 0.16.0 build -Doptimize=ReleaseSafe` (local, outside the repo)

## Build facts

- xfx gate: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` plus the fault build and `scripts/smoke-tui.sh` (as in `.prd/tui-phase3/loop.md`).
- Side-by-side capture: one isolated tmux server (`tmux -S <short socket>`), one session per binary, same size, `env -i` with an isolated `HOME`, `TERM=xterm-256color`, a fake `AI_GATEWAY_API_KEY`; fx also gets `FX_DISABLE_KEYCHAIN=1 FX_SKIP_ONBOARDING=1 FX_SOUND=0` (what fx's own harness sets, `tests/e2e/tmux-helpers.ts`). Never the user's default tmux server.
- Live turns for both binaries: both speak the Gateway's `/v3/ai/language-model` stream; fx accepts `FX_GATEWAY_BASE_URL` for loopback http only (`src/gateway/client.zig:623-627`).

## Work units, in order

Each closes on a fx↔xfx capture pair at 120×40 and 80×24 plus xfx tests; shared files mean they run in sequence on this branch.

| WU | Scope | Rows |
|---|---|---|
| A | default-on launch; fx header row; floating footer (`footer_layout.zig:39-96`); fx chrome: blank dividers, `┃ ` composer, hint `mode · model` | F1, divergences 1–3 |
| B | slash menu as fx's filterable popup between the two dividers | F2, divergence 4 |
| C | P-class commands with notices and menus: `/help` menu, `/status`, `/clear`, `/reset`, `/rename`, `/stats`, `/copy`, `/alias`, `/statusline`, `/sound`, `/permissions` | A1–4, 6, 10, 12, 16, 21, 26, 31, 32 |
| D | pickers: `/model`, `/resume`, `/settings`, `/provider` (provider rows only) | A5, 9, 15, 25; F8 |
| E | composer and `@` file picker | F3, F4 |
| F | transcript rendering, approvals and decision prompts, full-transcript screen, resize, interrupt | F5–F7, F9–F11 |
| G | B-class backends, each its own unit | A11, 13, 14, 17–20, 23, 24, 27–30, 33, 34 |

## Gap matrix

| Row | Status | Evidence |
|---|---|---|
| F1 | closed (R2) | `scripts/fxcap/capture.sh {fx,xfx} startup 40x120`: rows 1-5 the same (header text is xfx's own brand); band floats (fcc8e44) |
| F5 transcript look | closed (R2) | `answered` scenario 40x120: rows 2-24 byte-identical ANSI to fx (3efe232) |
| F5 streaming | matches fx | fx mid-stream shows only the activity row; xfx now shows lines once complete (presenter hold) |
| F6 activity row | open | fx `  Generating (2s) (↑4 ↓61)` with a blinking bullet; xfx `• Thinking  0s` |
| F5 links | open | fx renders markdown links as OSC 8 hyperlinks; xfx's painter (`src/tui/frame.rs` `tamed`, via `pacer::escape_at`) drops every non-colour escape, so a link reaches the screen as its text only. Fixing it widens `check.rs`'s vocabulary (OSC 8 open/close) — a decision, recorded here |
| F5 notices | open | fx notices are `<glyph> <topic>: <body>` (glyphs `* ✗ ! ✓ i ⊘`, 252/250, blank row either side, `transcript_blocks.zig:1758-1800`); fx does not echo slash lines; unknown command = `✗ command: Unknown command. Try /help.` with exact-match dispatch. xfx: `[shell] …` lines, echoes every submitted line |
| A2/A3/A4 | open | fx `/clear` = `/new` = `/reset`: one reset (`app_session_runtime.zig:2001` discards the policy) — push the transcript to scrollback, new session, repaint the welcome. xfx `/clear` erases screen+scrollback and keeps the conversation |
| F2 | open | R1 capture: 36 vs 7 commands, different shape |
| A1–A36 | open | inventory in `ssot.md` |
| F3–F11 | open | — |

## Rounds

### R1 — 2026-10-06

- Built fx `c1db919`; captured startup, `/`, overflow and 80×24 for both binaries (receipts to follow in `receipts/`).
- Read `footer_layout.zig`, `commands.zig:463-498`, `app_commands.zig:363-410`; wrote `ssot.md`.
- Next: WU-A.

### R2 — 2026-10-06

- Default-on launch, fx chrome, statusline and welcome (a50f8a0); markdown presenter port (c753e74); transcript block renderers, byte-matched against fx's own output (71bee29).
- Floating band (fcc8e44): `Geometry::floated`, appends fill the rows the band moved down from, resize pins the band at the bottom (Tension: fx re-renders its owned band on resize; xfx cannot know where a terminal reflowed its rows).
- Transcript look (3efe232): rail card, markdown answer in the two-column gutter, code/table/rule blocks, dim turn summary. Test fixtures being moved to the new truth on `fix/tui-phase4-tests`.
- Capture harness committed: `scripts/fxcap/` (isolated tmux socket, empty HOME, fake key, loopback fake gateway on an OS-picked port).
