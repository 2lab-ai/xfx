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
| F1 | open | R1 capture: fx starts under the shell cursor with a header; xfx is bottom-anchored with a divider |
| F2 | open | R1 capture: 36 vs 7 commands, different shape |
| A1–A36 | open | inventory in `ssot.md` |
| F3–F11 | open | — |

## Rounds

### R1 — 2026-10-06

- Built fx `c1db919`; captured startup, `/`, overflow and 80×24 for both binaries (receipts to follow in `receipts/`).
- Read `footer_layout.zig`, `commands.zig:463-498`, `app_commands.zig:363-410`; wrote `ssot.md`.
- Next: WU-A.
