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

### R3 — 2026-10-06

- Tests moved to the transcript truth (6228b88); defects it surfaced fixed (e153ff3): summary out count at arrival, bold on every card line, retint of dim rows and markdown styles.
- Activity row is fx's (ca044a1): `• Generating (2s) (↑4 ↓61)`, no blank above (the document's own gap), one below. 30x100 streaming pair matches row for row.
- Harness: shims `open`/`pbcopy`/`pbpaste`/`afplay`/`osascript` and sets `FX_NO_OPEN_BROWSER=1` — a research run before this opened two sign-in tabs in the user's browser (nothing entered; reported to the user).
- Command research done (read-only, captures in the session scratchpad): `research/general-session.md`, `account-model.md`, `appearance-security-ext.md`.

## Unit plan (dispatch order; shared files force the waves)

| Wave | Unit | Rows | Depends on |
|---|---|---|---|
| 1 | WU-B slash popup | F2 | activity geometry |
| 1 | C0 notices + dispatch (no slash echo, exact match, `✗ command: Unknown command. Try /help.`) + OSC 2 title | F5 notices, A12 shape | — |
| 2 | C1 `/help` catalog footer (All + category tabs, filter, Enter runs) | A1 | WU-B |
| 2 | C2 `/clear` = `/new` = `/reset`: one reset — transcript to scrollback, new session, welcome | A2–A4 | C0 |
| 2 | C3 `/status`, `/version`, `/stats` notices | A10, A12, A35 | C0 |
| 2 | C4 `/copy` (pbcopy, raw markdown of the last reply) | A21 | C0 |
| 2 | C5 `/quit` resume line, title cleared on exit | A36 | C0 |
| 3 | D1 `/resume` picker + replay; `/rename` with a stored title | A5, A6, F8 | C1, C2 |
| 3 | D2 `/model` picker (tabs, facts, effort) + `/model <q>` fuzzy | A15 | C1 |
| 3 | D3 `/provider` picker rows (sign-in columns gated #88) | A9 | C1 |
| 3 | D4 `/settings`, `/statusline`, `/sound` (bell), `/permissions` | A16, A25, A31, A32 | C1 |
| 3 | E `@` file picker, composer contracts | F3, F4 | WU-B |
| 3 | F1 tool group block: `● N tool calls · k read · …` header, `├`/`└` rows per call spanning steps until assistant text; outcomes (`Exited 3`, `Failed x: reason`, `Denied …`, `Cancelled …`); live `└ Running <cmd>` row; activity row hidden under a modal (research `surfaces.md` §1) | F5 | C0 |
| 3 | F2 approvals: file review (diff, `Review change`, `Edit · +1 -1`, `Apply once / Apply + allow … / Don't apply`), command approval (`Would you like to run…`, `$ # shell.run …`), keys per divergence 7 | F6 | C0 |
| 3 | F3 ask_user_question presentation (`1) label  desc`, `Question i of n`, freeform Other, summary rows) | F6 | F2 |
| 3 | F4 interrupt: `■ Cancelled · What can xfx do differently?`, Esc-Esc interrupts, Ctrl-C clears a draft first, exit hint; retry countdown row `⚠ Provider unavailable · HTTP n · … · retrying request in Ns` | F10 | C0 |
| 3 | F5 full-transcript screen (Ctrl-O): timestamps, args, outputs, diffs, notices; PgUp/PgDn/Esc | F7 | F1 |
| 3 | F6 composer keys (§5.2 of `surfaces.md`: Alt-b/f/d, `\`-Enter, PgUp/PgDn, Shift-Tab mode cycle, Ctrl-O, Ctrl-V, Ctrl-P = model catalog) and steer-while-busy (`┋` queued steer, Up retracts) | F3, F10 | D2 for Ctrl-P |
| 4 | G backends: `/usage`, `/credits`, `/undo`, `/workspace`, `/allowlist`, `/compact`, `/fast`, `/ultrafast`, `/image(s)`, `/paste`, `/trace`, `/mcp`, `/skills` | B rows | per row |
| — | X/G shown as nothing: `/feedback` (Vercel form), `/alias` (fx placeholder; a stub breaks `check-no-stubs`), `/shell` (fx login-shell snapshot), `/login` `/logout` (#88) | — | — |
