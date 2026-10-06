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
| F2 slash popup | closed (WU-B, ede8707) | `receipts/wu-b/`: 5 snaps byte-identical band rows; the rest differ only by xfx's 7 commands vs fx's 36 and three summaries kept as xfx's own (`/clear`, `/model` — no reasoning effort sent, `/setup` — no sign-in), each a description xfx could not honour |
| F5 notices + dispatch | closed (C0, e727eaa + 51601e4) | `receipts/c0/`: notices byte-identical to fx except the version number; unknown-with-words → prompt as fx |
| A1 `/help` | closed (C1, c9b6c11) | `receipts/c1/`: `query-none` 7/7 and four snaps 3/3 identical; header packing same rule; Ctrl-C pending hint (`press ctrl+c again to exit`) belongs to F4 |
| F6 activity row | closed (ca044a1) | fx `  Generating (2s) (↑4 ↓61)` with a blinking bullet; xfx `• Thinking  0s` |
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
| 3 | F4 interrupt: `■ Cancelled · What can xfx do differently?`, Esc-Esc interrupts, Ctrl-C clears a draft first, exit hint (`press ctrl+c again to exit` on the hint row, every surface incl. the /help catalog); retry countdown row `⚠ Provider unavailable · HTTP n · … · retrying request in Ns` | F10 | C0 |
| 3 | F5 full-transcript screen (Ctrl-O): timestamps, args, outputs, diffs, notices; PgUp/PgDn/Esc | F7 | F1 |
| 3 | F6 composer keys (§5.2 of `surfaces.md`: Alt-b/f/d, `\`-Enter, PgUp/PgDn, Shift-Tab mode cycle, Ctrl-O, Ctrl-V, Ctrl-P = model catalog) and steer-while-busy (`┋` queued steer, Up retracts) | F3, F10 | D2 for Ctrl-P |
| 4 | G backends: `/usage`, `/credits`, `/undo`, `/workspace`, `/allowlist`, `/compact`, `/fast`, `/ultrafast`, `/image(s)`, `/paste`, `/trace`, `/mcp`, `/skills` | B rows | per row |
| — | X/G shown as nothing: `/feedback` (Vercel form), `/alias` (fx placeholder; a stub breaks `check-no-stubs`), `/shell` (fx login-shell snapshot), `/login` `/logout` (#88) | — | — |

### R4 — 2026-10-07

- Smoke harness ported (79c083c): one band reader from the hint row up, emulator accepts the product's text styles; 34/34 on the floating band before WU-B. Its two product findings fixed here (welcome retint, no summary under local commands).
- WU-B merged (ede8707). Smoke row 16 reads the old menu shape and fails on the new popup — to be restated in the smoke pass after the C0/C1/F1/F2 wave, which reshapes notices, `/help` and tool rows the harness also reads.
- `docs/parity.md` "full-screen TUI" row still says the prompt echo is not durable; true since fcc8e44/ca044a1 — fixed in the docs pass.

### R5 — 2026-10-07

- Merged into plan: C0, C1, C3, C4, C5, F2 (approvals); `build.rs` now follows a linked worktree's HEAD (b9c5591).
- In flight, one unit per branch `feat/tui-phase4-<name>`: F1 `toolgroup`, C2 `reset`, F3 `question`, F4 `interrupt` (cancel line, Esc-Esc, Ctrl-C draft/exit hint, steer presentation, retry countdown row), F5 `fulltranscript`, E1 `filepicker` (`@` picker + composer keys; Shift-Tab moved to D4a), D2a `model` (menu, gateway catalog endpoint, fuzzy router form, settings persistence), D3 `provider`, D4a `permissions` (+ Shift-Tab cycle), D4b `statusline`, D4c `sound` (BEL; fx chimes are X).
- Queued behind them: D1 `/resume` picker + replay and `/rename` (after C2), D2b `/model` inline columns + effort (after D2a), D4d `/settings` (after D4a–c), then Wave 4.
- All D units share one settings writer API (`profile::write_preference(path, key_path, value)`), named in every brief so the merges are mechanical.
- Merged: F1 tool group block (receipt `receipts/f1/`: 94/142 group rows byte-identical, the rest explained), C2 one reset for `/clear` `/new` `/reset` (receipt `receipts/c2/`: screens and full histories identical apart from the banner; divergences: landed rows of a cancelled answer go to scrollback, session opened lazily). Gates green at load 50 on c73c31c (lib 2050/2054, tui 90/102).
- 79f2443: two load-fooled test waits fixed (status-row wait; thread CPU time for the markdown linear-time tests).
- Opened T1 `fix/terminal-group-sweep`: `a_stopped_command_leaves_nothing_alive_in_its_process_group` escaped once at load 33 (pre-existing, `src/tools/terminal.rs` `stop` signals the group 3 times then only watches).
- D1 `resume` dispatched on c73c31c.
- Merged: F3 question panel (receipt `receipts/f3/`, cell-identical apart from five listed gaps — sent back to F3 to close: fx's Esc/Ctrl-C at a question, multi-row freeform draft with Shift arrows and paste, reverse-video caret cell, answer-row retint), D4b `/statusline` (rows 2-40 byte-identical in 11 snaps; context segment now off by default as fx; workspace segment shows fx's full path). Gates green on 2870b42 (lib 2113/2117, tui 90/102).
- 3894210: `build.rs` also follows a branch whose ref starts packed (banner revision was stale in D3/D4b worktrees).
- D3 `/provider` returned; its merge conflicted with D4b, sent back with two fx follow-ups (`Already using <name>.`, fx's busy predicate).
- Opened T2 `fix/tui-notice-midstream`: a notice written while an answer streams splits the answer and the continuation rows lose their last cell (D3 finding, reproduced on b9c5591).
- Docs pass owed after the wave: `README.md` command list, `docs/parity.md` intro counts.
- Merged: D2a `/model` menu + gateway catalog + fuzzy router form (16/20 snaps identical; D2b running on the same branch for the inline columns, effort, model change as a control message, catalog retry, serialized settings writes), T1 (terminal group sweep waits for the leader to exit; escapes 23/6912 → 0/13824 under induced load; follow-up `fix/terminal-settle` for false Stranded notices), F5 Ctrl-O full transcript (follow-ups sent: Home/End, hidden caret, unbound keys to the composer, session/network notices, a diff per change, resume feed, `ScreenOwner::Alternate`), D3 `/provider` (`Already using`, fx busy rule). Gates green on e50a8f0 (lib 2214/2218, tui 91/103).
- E1 `@` file picker returned; its merge conflicts with F5's page keys — sent back with fx follow-ups (Ctrl-P model catalog per divergence 8, Alt-Backspace word chars, `/help` paging, Super-A/C/X selection, progressive index).
- Merged: D4a `/permissions` + Shift-Tab (follow-ups sent: a mid-turn mode change gates that turn's later tool calls as fx, `reset` vs durable grants, shadowed-write notice), D4c `/sound` (BEL; fx's chimes are X), D1 `/resume` picker + replay and `/rename` (picker and replay screens byte-identical; follow-ups sent: replayed tool outcomes, fx replay order, cancellation replay, busy predicate, fx title derivation, rename as a control message, paging, Ctrl-O details after resume). Gates green on 652e3d3 (lib 2328/2332, tui 95/107).
- **Harness incident, fixed in 2f96658:** on 2026-10-06 at 23:09 and 23:14 a research copy of `capture.sh` let fx write `/tmp/fx-toggle.m4a` and `/tmp/fx-bloom.m4a` and play them through `/usr/bin/afplay` (absolute path, past the PATH shim; `FX_SOUND=0` only sets the launch level and the research scenario typed `/sound`). Every fx run now gets a TMPDIR whose chime names are directories, and the run fails if one is written; verified with the sound scenario (4 bells, 0 chimes, exit 0).
- Owed: `src/tui/copy.rs` spawns its thread on the UI thread after the signal install and does not join it before `signals::release` (D4c finding 4); smoke 23b ends on a BEL after the frame; README "Five keys are read" stale.
- Merged: F3 round 2 (fx Esc/Ctrl-C at a question = cancel the turn, model gets `aborted by user`; fx's freeform field with wrap/Shift-arrows/paste; SGR 7/27 reverse-video caret in questions and approval amendments; every document row records its kind at write time, so retint no longer guesses), T1 follow-up (a stranded notice only for a member that could still run: false notices 40/6912 → 0/13824 under induced load; the residual non-leader fork escape is ~0.26% of stops in a 144-commands-at-once nested-fork stress and is disclosed in 29/30 cases). Gates green on the T1-follow-up merge (lib 2363/2367, tui 95/107).
- Owed after merges: hide the hardware cursor while a reversed caret is up (needs F5's hidden-cursor frame); the question field's Alt-Backspace by word chars (after E1); the tool result's error flag and fx's `<turn_aborted>` history message after a cancelled question (sent to F4).
- Merged: F5 follow-ups (fx keys, hidden caret on the plane, session/network records, a diff per change, resumed sessions fill the record, one Response across notices), D2b (`/model` inline column + effort on the wire — gateway `reasoning`, llmux `output_config.effort`; gateway chat endpoint v3 → v4 as fx 09ec38c2; model change as a control message; settings writes serialized with fx's `settings.lock`), D4a follow-ups (mode change governs the running turn; durable reset; shadowed-save line). bc2f8c5: the D3/D2a e2e snapshot now waits for the model save (announce-before-save race since D2b, 1/8 → 16/16). cafab50: harness sound guard checks chime names only; `home` names binary files.
- Research `media-ext.md` done (`/image` `/images` `/paste` B, `/trace` P, `/mcp` `/skills` B, `/feedback` X, `/login` `/logout` G #88).
- In flight: F4, E1, D1 follow-ups, D2c (`/fast` `/ultrafast` + speed column), D4d `/settings`, W1 `/usage` `/credits`, W2 `/image` `/images` `/paste`, W3 `/trace`, W6 `/undo`, W8 `/allowlist` + `/permissions remember|revoke`. Queued: `/mcp`, `/skills`, `/compact`, `/workspace`, smoke pass, docs pass.
- Merged: E1 round 2 (`@` picker band rows byte-identical in 33 snaps; C-p opens the model catalog per divergence 8; Alt-Backspace by word chars; page keys in menus; Super+A/C/X selection in reverse video; the first index searched while it is published). Gates green (lib 2521/2525, tui 97/109).
- Queued W9 session titles: fx asks its gateway for a session title after the first prompt (`receipts/e1/…-bodies/001-title.json`) and shows it in the window title, the statusline session segment and the resume picker; xfx derives it from the first prompt. Also the `/settings` "Session titles" row D4d leaves out.
- Merged: F4 (interrupt `■ Cancelled · What can xfx do differently?`, Esc-Esc, Ctrl-C draft/exit hint, retry countdown row, waiting prompt in the band, Ctrl-D ignored mid-stream; the request after an interrupt carries fx's shapes — partial + `The previous response ended before completion.`, `aborted by user` error results, `<turn_aborted>`; every failed tool result goes as `error-text` / `is_error`; recovery records on the full transcript). Gates green (lib 2568/2572, tui 100/112). Round 2 sent: stop a running command at the keystroke, `execution-denied`, the band's recovered row, Esc pauses recovery, retry row wrap, steer semantics, `queued N` as fx.
- Dispatched W4 `/compact`, W5 `/workspace`.
