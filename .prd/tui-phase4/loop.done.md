# TUI Phase 4 — done (append-only)

Loop: ./loop.md · Each round close appends one block at the bottom; nothing above it ever changes (`loop-lint.py` C2 compares against git). Rows here are snapshots. A later pass is a `re-verified` line in a loop.md round section; a regression reopens as a new loop.md row with Source `regression:<ID>`.

## R1–R20 (legacy single-file driver, 2026-10-06 → 2026-11-03) — closed 2026-11-03

- result: the first split of the legacy single-file `loop.md`: its whole text as it stood at `9aa666c7`, gap matrix (a snapshot; several rows it calls open were closed by later units, see the batch lines), unit plan and the batch ledger through batch 20 (D2c round 6). · commit: the commit that adds this file

### Round section (as it was in loop.md)

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
- Merged: W6 `/undo` (process-local journal as fx's — fx clears it on resume, `app_session_runtime.zig:1999`; restore/delete/nothing/unknown rows byte-identical; stricter read-proof than fx). Gates green (lib 2595/2599, tui 102/114). Follow-up sent: pin the undo target at the keystroke when typed mid-turn.
- Merged: W1 `/usage` `/credits` (29/34 snaps identical beyond the banner; spend from the gateway's own `providerMetadata.gateway.cost`; follow-ups sent), W3 `/trace` (copied / unknown / could-not-create byte-identical; follow-ups sent: fx notice wrap at `/`, join trace/copy threads at exit, typed-ahead input at startup), D4d `/settings` (243/272 setting rows byte-identical, the rest `max` and the absent startup-scrollback row; prompt history on disk in fx's format; follow-ups sent), W6 follow-up (mid-turn `/undo` pinned at the keystroke). c537950: `/help` catalog tests now expect fx's 20-row cap (21 commands). Gates green on 357eb5e except the recurring reset e2e under load.
- **Repeated failure → T4 `fix/tui-reset-flake`:** `a_reset_hands_the_screen_to_scrollback_and_starts_a_fresh_conversation` failed in full-suite runs in D4a, F3, F5, D4d and plan (load-dependent, 40/40 alone); rate tracks UI-thread busy time around a reset. T4 decides product vs test race from evidence and fixes it durably.
- Merged: W2 image attachments (13/13 notice snaps byte-identical; `images_attached` session event; gateway v4 `file` part, llmux `image` block; follow-ups sent: path auto-attach, send-time withholding), W8 configured permission rules (`/allowlist`, `/permissions remember|revoke`; fx's last-match-wins rule language; **`serde_json` `preserve_order` crate-wide** so a settings write never reorders a rule table; 20/25 snaps identical). c838223 `/copy` test waits 60 s; 6301452 one deferred-commands row after W2+W8. Gates green on 6301452 (lib 2851/2855, tui 111/123, permissions 117).
- **For the final review (security semantics):** W8's rule matching and fail-closed cases; W5 `/workspace` admits writes in an added root in auto mode as fx does (`tool_admission.zig:665-679`) and `--add-dir` is no longer read-only — sensitive targets (`.zshrc`, `.git/hooks`, …) still refused in auto; D2b gateway endpoint v3 → v4; session-log events older binaries refuse (`images_attached`, `session_renamed`, `tool_feedback` precedent).
- W5 back for a merge against W8 (rules vs added roots). Opened T5 OSC 8 hyperlinks, T6 multi-step turn summary `↓` count.
- Merged: W4 `/compact` (activity rows byte-identical; typed `context_checkpoint` event; notes request with fx's prompt; automatic compaction at turn start; follow-ups sent: meter as fx, mid-turn/overflow points, records, ledger, trace section), W3 follow-ups (fx's notice wrap — `/` `\` breaks, no trailing space; clipboard/report threads joined before the terminal restore; **raw mode entered with TCSANOW, not TCSAFLUSH — typed-ahead input was discarded at startup**; round 3 sent: IGNCR/INLCR, atomic report publish). Gates green on e1ad990 (lib 2909/2913, tui 120/132).
- W5 merged plan 6301452 (rules address an added root by absolute path, as fx `permissions.zig:932-943`; 3 tests); asked to merge e1ad990.
- Merged: W1 follow-ups (`/credits` on its own unsignalled thread, never queued; abandonable `/usage` reader; late bills asked by generation id at fx's trusted origins only), W5 `/workspace` (rules address an added root by absolute path; remember works in an added root as fx). Gates green on 311f7ff (lib 2977/2981, tui 122/134, permissions 121). Registry: 27 commands.
- Dispatched W9 generated session titles. Queued: `/mcp`, `/skills` (B, large), smoke pass, docs pass, then the delivery chain.
- Merged as one gated batch (integration branch, gates run before plan moved): D2c `/fast` `/ultrafast` (fx's operational lines "Ultrafast was requested, but Gateway did not confirm…" and "Fast mode is unavailable…" in the answer; saves as `persistPreferenceTargets`; restored Fast binding on model+fast_mode — provider term and persist-then-announce order sent as round 2), W1 round 4 (`process::command`: every child starts with an empty signal mask and SIGPIPE default; `timeout 1 sleep 5` 5.03 s → 1.05 s; a scan test forbids `Command::new` elsewhere), T4 (the reset e2e was a **test** race — it read the frame before the reset's own; 24/90 → 0/90 red at utility priority; follow-up sent: fx's synchronized-update bracket for the handoff, the mid-turn reset test, a stop-before-first-frame exit), T6 (the summary's ↓ sums every request as fx's `reconcileTokenRequest`, live and resumed via an optional `usage_recorded.turn_output_tokens`), T5 OSC 8 hyperlinks (one inert gate `tui/hyperlink.rs` + an independent checker parser; fx's link ids; scheme allowlist http/https/file/mailto is stricter than fx). 73544fc: T5's test used T6's removed field. Gates green on 73544fc (lib 3080/3084, tui 125/138, tool_loop 97). Registry 29.
- Opened T7 `fix/tui-prose-wrap`: fx's assistant wrap moves the word before a paragraph-final overflow word down too (`assistant_wrap.zig:219-298`, `isFinalWordOnLine`); xfx's `wrap.rs` doesn't (D2c finding, Ultra line at 120 cols).
- Returned and queued behind their conflicts: F4 round 2 (mid-turn prompt = steer as fx, no `queued N`), W2 follow-ups (typed/pasted image paths, send-time withholding; round 3 sent), D1 round 2 (replayed tool rows byte-identical, stopped-turn replay, paging, inert hydration on both resume paths; round 3 sent), W4 follow-ups (meter, per-step compaction, overflow resend, `/usage`, `/trace` section; records deferred to Tensions; round 3 sent: in-turn cut), W9 generated titles (round 2 sent: a 17 s first-token wait near the 15 s title timeout must be root-caused before merge).
- Merged: F4 round 2 (a prompt typed mid-turn steers the turn at fx's boundaries — `src/agent/steering.rs`, one lock so Up's retract is atomic — and the hint row has no `queued N`; a running command stops at the keystroke; the band's recovered and wrapped retry rows byte-identical; a request a steer cuts short settles its streamed estimate into ↓). Escape pausing a recovery stays a Tension (needs fx's unbudgeted connectivity wait). Gates green on e17dd02 (lib 3097/3101, tui 126/139, tool_loop 103).
- Returned, re-merging e17dd02 (they conflict with F4 and each other; first to report lands): D4d follow-ups (blank rows after an approved command were an append count past `content_bottom`; Up/Down caret edge; startup scrollback; Model row inline list; collapse of on-screen groups; Fast/Ultra rows — 308/341 settings rows byte-identical, the rest fx's `max`), W2 rounds 3–4 (`read_file` returns images on both wires — new `tool_images_attached` event; `[Image #N]` renumbering; Ctrl-U/Y and recall bring images back; fx's `✗ input:` limit notices; clipboard badges link a paste-time copy; cards linked by sent token), D1 rounds 3–4 (`cancellation_origin` as fx; Ctrl-O trailing rows; per-result stamps and `terminal_reason` in flight).
- Merged: D4d follow-ups (`/settings` 308/341 rows byte-identical; startup scrollback as fx's launch origin; Up/Down caret edge before history, then F4's steer retract on an empty composer), T4 follow-ups (a reset's handoff is one `?2026` block as fx's; the stop/continue tests wait for the `CLD_STOPPED` notice — a macOS stop race in the tests, 2/4560 → 0/10800), D1 round 4 (replay stamps per result, summary-less turns inherit; `terminal_reason` and `cancellation_origin` as fx's; `read_file` refusals in fx's words). Gates green on 166eed4 (lib 3139/3143, tui 128/141, tool_loop 106).
- The batch with D4d first failed `process::tests::every_owned_signal_reaches_a_child…` once per build at load ≈36; 8/8 full lib runs passed after, and the regate was green. Repeated-failure rule → T9 `fix/process-signal-flake` root-causes it.
- Opened T8 `fix/tui-sync-appends`: fx paints document growth inside the frame's `?2026` block; xfx writes each append before the band frame (T4 receipt).
- Merged: T7 (`src/tui/prose.rs`, a port of fx's `assistant_wrap.zig` for answer rows and Ctrl-O tool detail: no orphan word on either path, continuation leads for lists/quotes/footnotes, 8-column tabs; every t7 receipt row byte-equal, D2c's Ultra line now 8/8), D2c rounds 2–3 (the provider recorded on session frames and compared in the Fast binding; `/fast` `/ultrafast` and `/sound` say a save's outcome before the change as fx; a reset keeps a held save's words for the old screen; every model pick saves fx's fields). Gates green on 4819e626 (lib 3175/3179, tui 128/141).
- T9 lead (T7): the batch-3 process-test failure followed the gate being launched as a shell `&`/`nohup` job — SIGINT ignored and inherited; `process::command` resets the mask, not ignored dispositions. Gates now run only in the foreground or the tool's own background mode.
- Opened T10 `fix/tui-card-wrap`: fx wraps the user card with `wrapCut`, approval commands with `CommandSegmentIterator`, table cells with `wrapTableCell`; xfx uses `wrap.rs` for all three.
- Merged: W2 rounds 3–6 (`read_file` returns an image on both wires, saved and re-sent — new `tool_images_attached` event; typed `[Image #N]` lookalikes renumbered as fx's `mapSubmittedImages`; Ctrl-U/Y and recall bring images back; fx's `✗ input:` limit notices, per paste; clipboard badges link a paste-time copy; cards linked by sent token; an edit over a badge is an undo boundary as fx's; the `StoppedAtApproval` journal wrapper now forwards tool images). Gates green on 9f8ee201 (lib 3214/3218, tui 133/146). Round 7 sent: the same boundary for pasted-text blocks.
- Merged: W2 round 7 (a pasted-text block is an undo boundary and a killed one yanks back as its text, as fx's kill ring holds text; round 8 sent: `1 line`, fx's composer wrap), T9 (the process-test failure was the gate itself: launched as `nohup … &`, SIGINT/SIGHUP ignored and inherited; xfx keeps fx's spawn rule — no SETSIGDEF in fx's `darwin_process_spawn.zig:74-83` — and its stop is SIGKILL to the group; the cases now undo their runner's ignores and a new case runs them under ignores). Gates green on 104ccaa2 (lib 3215/3219, tui 133/146).
- Dispatched M1 `/mcp` (stdio + HTTP without OAuth; `auth`/`logout`/OAuth/browser leaves G) and S1 `/skills` + `$` (xfx's own roots only — fx also reads other agents' roots, not done here; no network `add`). Both defaults are for the user's review.
- Merged: W9 rounds 1–3 (generated session titles — asked at submit, steers included; fx's install rule against D1's one derived title; a question runs on across `/new`/`/resume` and is dropped as `session_changed`; `/trace` `## Session Title`; a late title never overrides a `/rename`; the line-shell "ok" wait made unambiguous), W2 round 8 (the composer lays out by fx's `visual_layout.zig` — long word broken on its own row, badges/blocks as units, 8-column tabs, Up/Down in front of a wide glyph; a caret after an exactly full row ended the session — fixed; `1 line`), D1 round 5 (a stopped command gets SIGTERM, SIGKILL after 700 ms as fx's `terminationSignalPlan`, Ctrl-O shows `signal 15`; read_file in-call failures in fx's envelope, the row says no reason; replay stamps at the turn's conclusion). Gates green on c96d9266 (lib 3265/3269, tui 137/150, tool_loop 108, permissions 122).
- Dispatched SM1 `fix/smoke-tui`: CI runs `scripts/smoke-tui.sh` (`ci.yml:161-166`) and many rows assert the pre-parity product.
- Merged: W2 round 9 (a sent prompt's card shows its pasted text, live, steered and resumed), W9 round 4 (title calls drain like fx's pooled read and appear in `/trace` Network Calls; undo e2e needles read wrapped notices), D1 round 6 (glob/grep take a file root; fx's envelopes and Zig error names; a timed-out command's end on Ctrl-O; `wrote|edited <path> (<n> bytes)`), W4 rounds 3–5 (compaction cuts inside a turn — `open` on the checkpoint, old binaries refuse such a log; the catalog read at launch; `calibrateProviderRequest`; fx's `ContextCapacityExceeded` texts; steer grouping by `after_tool_step_count`; the notes request after the conversation; every `TurnJournal` method required — a compile_fail doctest holds it), T8 (a frame's document growth inside its `?2026` update: 0 cell-writing bytes outside an update, one write per frame). Gates green on 1d05d6cf (lib 3314/3318, tui 148/161, doctest 1). Round 2 sent to T8 (exit restore closes an open update, /resume wipe inside, title outside).
- Dispatched R1 (read ask, hidden tool on global deny, url rules — W8's divergences). Owed: D4d collapse-off re-expand; copy test at load ≥ 60.
- 56f20fcc: W4's masking tests used a 38-char `sk-` literal that CI's `check-no-secrets.sh` reads as a real key (the batch gate didn't run that check; it does now, with `check-preview-contract.sh`). Merged SM1 (`scripts/smoke-tui.sh` against the fx-parity product: 23/34 → 0 failures, 1022 checks; the emulator now holds `check.rs`'s rules — SGR 7/27/48/49, OSC 8 by `wire_link`, sync blocks, off-screen CUP; `CANONICAL_COMMANDS` read from the registry; the fixture answers the catalog GET apart from scripted turns). SM1 re-runs at the end against the final plan.
- Merged: T10 (the user card wraps by fx's `wrapCut`, an approval command by `CommandSegmentIterator`; control bytes in an approval command are now spelled as fx's — `\x1b`, `\x09`, `\x0d`, `\u{NNNN}` — where the bridge used to blank them; table cells were already fx's), P1 (a provider switch keeps the conversation, its clock and title lineage as fx's `/provider`; tool-call ids projected as fx's `fx_<hex>_<n>`; llmux's signed reasoning dropped on the way to the Gateway; round 2 sent: replay keyed by provider+model, resume on the recorded provider). Gates green on aa916857 (lib 3340/3344, tui 152/165) with check-no-secrets and check-preview-contract now in the batch gate.
- Returned and in round 2: S1 `/skills` + `$` (20/20 screens; request bytes as fx's; round 2: fx's inline ghost completion for `$` and `/`, skills in the slash popup, window-sized catalog budget), D2c round 4 (every catalog ask is a control message; a re-ask after a failure reads at once mid-turn; round 5: fx's 1 s floor, a read not held by a parked question).
- **Paused 2026-10-07 17:0x (user).** plan = 9054bb8c, gates green. In flight and stopped, branches kept as they are: T8 round 2 (`fix/tui-sync-appends` d362cbde, committed, gates not rerun), R1 (`feat/tui-phase4-rules` cf8b7dfe, committed, re-capturing the read band after T10), M1 `/mcp` (`feat/tui-phase4-mcp` 2bc6d8bd + untracked receipts), S1 round 2 (`feat/tui-phase4-skills` f3f4f822 + uncommitted WIP), P1 round 2 (`feat/tui-phase4-provider-session` 2b3966e2 + uncommitted WIP), D2c round 4 (`feat/tui-phase4-model-r4` cebbd1c, done, not landed) → round 5, W9 round 5 (`feat/tui-phase4-titles` + untracked receipts); SM1's final smoke run waits for all of them. Resume notes live outside this repo (the orchestrator's handoff).
- **Resumed 2026-11-03 (user: "작업 진행해줘").** `plan/tui-phase4` pushed to origin (backup, no PR). Merged T8 round 2 (every restore closes an open `?2026` update; /resume's reset inside its update; the title ahead of it; a band-tear fault halfway into the band) and D2c round 4 (every catalog ask a control message). Rust 1.99 deprecates `fetch_update`: the permission policy's two sites get the MSRV-keeping allow (148825a1). Gates on dd8b3f77 + 148825a1: lib 3347/3351, tui 152/166 (fault-injection 3354, 166), clippy 0 both, every check ok. W9 round 5 closed without a product change (fx's missing title = the fake resetting the connection fx warmed at launch; receipt 4e24a4a6). S1 r2, P1 r2, M1 re-dispatched. Scope: the fx v0.0.16 delta is the next phase (ssot Tensions).
- d91a9968: fx's auto-upgrade had replaced the c1db919 reference with v0.0.16 in place (11-03 19:24; it copies the latest release over its own executable 10 s after launch unless `FX_AUTO_UPGRADE=0`). capture.sh now launches every fx run with `FX_AUTO_UPGRADE=0` last in its environment and fails when the binary's bytes change; the reference bin directories are read-only; the reference was rebuilt with a fresh zig cache (sha256 `7b76319a…`). October's fx receipts stand (no newer release existed then); fx captures taken after 19:24 were redone.
- Merged M1 `/mcp` (stdio and HTTP without OAuth; 53 snap pairs cell-identical but row 1; auth/logout/OAuth/browser gated) and P1 round 2 (sealed reasoning replays only to the provider and model that sealed it; a TUI resume continues on the recorded provider; with no credential it stays there, warns once and refuses the prompt). Gates green on 668995e8 (lib 3396, tui 154; fault-injection 3403, 168). R1 conflicts with M1 in the permission code → R1 round 2 merges it (read ask and hidden tools must cover MCP tools as fx's do). Dispatched X1 (fx refuses a paste followed by more input in the same read: `✗ input: Paste was not applied because extra input followed its end marker.` — xfx had no such rule) and D4e (collapse off/on re-renders the visible screen as fx's).
- Merged D2c round 5 (every catalog read on its own `xfx-catalog` thread, never held by a parked approval or question; fx's one-second floor after a failed read; round 6 sent: the inline column re-asks on redraw) and S1 round 2 (fx's inline ghost completion for `$` and `/`; skill rows in the slash popup; the catalog budget sized by the model's window; discovery off the UI thread; the `skill` tool's legacy `name`/`offset` form with chunking; `skill_symlink_authorities`, deny unless authorized, fail closed; round 3 sent: resource identity re-check, context notices). Gates green on 1785efd2 (lib 3473, tui 156; fault-injection 3480, 170). Registry check: xfx lacks fx c1db919's `/alias` (P) and `/shell reload` (B) → units C7 and C8; `/login`, `/logout` (G #88) and `/feedback` (X) stay as classified. P1 round 3 (`! auth:`/`✗ auth:` refusals, fx's any-credential hint lead) waits for round 4 (hold a refused prompt until a credential arrives; no lead during an approval).

## R21–R22 — closed 2026-11-03

- result: batch 21 (C7 `/alias`, `1a94cdff`) and batch 22 (R1 round 2 `0eabf266` + H1 `2522c026`, `2ff00a09`) landed; gates on `2ff00a09`: lib 3509, tui 160, permissions 139; fault-injection lib 3516, tui 174; 0 failed; clippy 0 both. Two of Phase 4's P2 preconditions closed. · closed rows: R1-sec, H1 · commit: `2ff00a09`

### Closed rows

| ID | Status | Scope | Branch | Next |
|---|---|---|---|---|
| R1-sec | ✅ | every registered tool judged at admission; MCP admission merged with read-ask/hidden-tool; `skill` judged at the skill (fail-open found and fixed) | `feat/tui-phase4-rules` | landed batch 22 (`b6392eae`) |
| H1 | ✅ | resize, signal (+ jobctl, request log, cut/refused turns, usage routes, loopback MCP) | `feat/fxcap-verbs` | landed batch 22 (`2ff00a09`) |

### Round section (as it was in loop.md)

- R1 round 2: every registered tool judged against the table at admission (`ask_user_question`, the MCP search tools, `skill` at the skill — S1's `skill` failed open on a per-skill deny and still refused ask rules; fixed with a RED test); a selected MCP tool re-checked against live rules each step; M1's saved-rule key built from the whole arguments, not the 2 KiB display copy; the amendment drawn as fx's prompt card (drawn on delivery, not before the step ends); the read band's scope row kept (recorded Tension, F2).
- H1: `resize`, `jobctl`, `signal`, `rehomefile`, `clock`, `usage`, `models2`, `mcphttp`, `sockets`; `@HOME@`/`@MCPPORT@`; fakegw request log, `reset`/`refuse` turns, catalog swap, usage routes; guards moved into the EXIT trap. Found (not fixed, now rows K-*): stale row after resize, Ctrl-Z/SIGTSTP, stream-failure recovery, MCP status words.

## R23 — closed 2026-11-03

- result: batch 23 (S1 round 3 item 1 `414205f9`, `65f43ded`) landed; gates on `65f43ded`: lib 3513, tui 160, permissions 139; fault-injection lib 3520, tui 174; 0 failed; clippy 0 both. The last code precondition of P2 closed. · closed rows: S1-sec · commit: `65f43ded`

### Closed rows

| ID | Status | Scope | Branch | Next |
|---|---|---|---|---|
| S1-sec | ✅ | skill resource identity re-check, both load modes (skills on by default) | `feat/tui-phase4-skills` | report → merge |

### Round section (as it was in loop.md)

- S1 round 3 item 1: the skill directory held as `O_DIRECTORY|O_NOFOLLOW`; `SKILL.md` and every resource component opened `openat` + NOFOLLOW from it; a resource read is bracketed by length+mtime and, for anything but `SKILL.md`, by re-reading the held `SKILL.md` from offset 0 and re-parsing its name (fx `revalidatePrimaryIdentity`, `skill_invocation.zig:1316-1329`; `readSkillResourceFile`, `:1371-1409`), in the whole and the named/chunked forms. Gap shared with fx: a `SKILL.md` renamed over the held one after the check (no inode comparison on either side).

## R24 — closed 2026-11-03

- result: batch 24 (merge commits are labelled "batch 23" by mistake: `37b53989` SEC-D1, `eefd3c18` SIGINT + SM1) landed; gates on `eefd3c18`: lib 3517, tui 161; fault-injection lib 3524, tui 175; 0 failed; clippy 0 both. First two of the P2 security fixes in.

### Closed rows

| ID | Status | Scope | Branch | Next |
|---|---|---|---|---|
| SEC-D1 | ✅ | Gateway completion and title never follow a redirect; no proxy over plain http | `fix/p2sec-gw-redirect` @ `3383cbe0` | landed batch 23 (`eefd3c18`) |
| SM1 | ✅ | smoke on release + fault-injection with the behaviour list: 37 scenarios + oracle, 1091/1091 (3f job control, 3g inherited ignores, 17b quit-and-resume new); SIGINT inherited as ignored stays ignored, as fx (`878186b5`) | `fix/smoke-tui` → `fix/p2-sigint-inherit` | landed batch 23 (`eefd3c18`) |

### Round section (as it was in loop.md)

- P2 security review (four groups × gpt-6-astra and opus at `de6618de`): union of MUST-FIX = 11 SEC units; SM1-final found the launcher's SIGINT ignore replaced by xfx's handler (fx keeps it) and fixed it as its own unit; COMPAT found the resume write-on-open (G-resume-prefs).

## R25 — closed 2026-11-03

- result: batch 25 (`8133dee2`): SEC-A1/A1b, A2, A4/A4b, B1/B1b, C1+C2/C2b, C3/C3b, D2, G-resume-prefs + COMPAT, the batch's resume-test restatement, the check-scripts pipefail fix, sweep tooling r2. Gates on `8133dee2`: lib 3552, tui 162, session_compat 6; fault-injection lib 3559, tui 176; 0 failed; clippy 0 both; test-sweep OK. Merge resolutions: `docs/parity.md` rows (rowmerge), `selection` doc (both rules), `transport.rs` imports (union).

### Closed rows

| ID | Status | Scope | Branch | Next |
|---|---|---|---|---|
| SEC-A1 | ✅ | the TTY question spells every field (`command_text`); on a terminal the line shell's and `ask`'s answer is inert (no state left to hide the question) | `fix/p2sec-tty-escape` @ `796d69cd` | landed batch 25 (`8133dee2`) |
| SEC-A2 | ✅ | a failed `/allowlist` view keeps the table in force; an unreadable table at launch refuses every tool; config diagnostics said at launch as fx | `fix/p2sec-malformed-table` @ `1754afde` | landed batch 25 |
| SEC-A4 | ✅ | every default-ignorable spelled out in approval and question fields; content keeps only RGI emoji sequences — no row can be drawn past the margin (tmux, Ghostty, fx widths) | `fix/p2sec-ignorable-cp` @ `f33a9747` | landed batch 25 |
| SEC-B1 | ✅ | a catalog/log model id with a control is refused at `apply` and not restored at resume; store errors and the line shell's `/model` listing quote no controls | `fix/p2sec-resume-model-inert` @ `ece5fa11` | landed batch 25 |
| SEC-C1 | ✅ | MCP stdio env = `HOME LOGNAME PATH SHELL TERM USER` + the server's `env` (fx 0.0.15); Docker cleanup the same; other children lose xfx's credentials | `fix/p2sec-mcp-env` @ `5d741708` | landed batch 25 (via `fix/p2sec-mcp-ws-cred`) |
| SEC-C2 | ✅ | workspace MCP entries cannot resolve or name xfx's credentials; an unset `${VAR}` stops the entry with fx's sentence | `fix/p2sec-mcp-ws-cred` @ `f003b378` | landed batch 25 |
| SEC-C3 | ✅ | stdin writes deadline-bound and non-blocking; a write cut mid-way ends the connection (c1db9191's taint rule); close never waits on a writer; group signals before the leader is reaped | `fix/p2sec-mcp-stdin` @ `431f3bd6` | landed batch 25 |
| SEC-D2 | ✅ | pasted/typed paths and `/image` obey the read table's denies (every mode); stat before open, non-regular files refused | `fix/p2sec-paste-image-read` @ `96e8b7b1` | landed batch 25 |
| COMPAT | ✅ | session-log back-compat vs the installed preview (6 tests); opening a session and quitting writes nothing; the session keeps its model (G-resume-prefs, as fx) | `test/session-compat` → `fix/p2-resume-prefs` @ `3652549c` | landed batch 25 |
| G-resume-prefs | ✅ | resume wrote the launch's model and mode over the session's | closed by COMPAT/G-resume-prefs, batch 25 |

### Round section (as it was in loop.md)

- Follow-ups the units found and closed in the same round: hidden tails in every approval field (A4b), model text leaving terminal state before the TTY question (A1b), the line shell's `/model` listing (B1b), an unset `${VAR}` now stops the entry (C2b), group signals after reaping (C3b), the check scripts' SIGPIPE false results — two failed open.

## R26 — closed 2026-11-03

- result: batch 26 (`ad4220bc`): SEC-C5 + C5b, SEC-C4 + C4b, the C4 test restated over C5 (a person's own `edit` rule admits the write; the changed entry still asks). Gates on `ad4220bc`: lib 3564, tui 162, permissions 144; fault-injection lib 3571, tui 176; 0 failed; clippy 0 both. Round 2 of the security review opened follow-ups on C3, C4, C5, D2, A1 (blockers) and A2, D1, SIGINT (cheap should-fixes); rows stay open in loop.md until those land.

### Closed rows

| ID | Status | Scope | Branch | Next |
|---|---|---|---|---|

### Round section (as it was in loop.md)

- Batch 26 landed the last two first-round fixes; no row closed this round because round 2 reopened both.

## R27–R29 — closed 2026-11-04

- result: batch 27 (`41ad4c21`: SEC-D1b, D2b, SIGINTb, A1c, A2b, C2c), batch 28 (`7c6f0c73`: RESUMEb, A4c, C5c, C3c), batch 29 (`552c04fb`: B1c, C4c). Gates: `41ad4c21` lib 3579 tui 163 / FI 3586 177; `7c6f0c73` lib 3598 tui 163 / FI 3605 177; `552c04fb` lib 3608 tui 163 / FI 3615 177; 0 failed, clippy 0 throughout. Round-2 reviews (two engines per unit) and their follow-ups; round 3 under way.

### Closed rows

| ID | Status | Scope | Branch | Next |
|---|---|---|---|---|

### Round section (as it was in loop.md)

- Measured on this Mac during the follow-ups: `std::env::var("HOME=")` returns HOME's value (C2c, a real credential read through `${AI_GATEWAY_API_KEY=}`); tmux and herdr draw U+FF9E/U+A8FA/U+0D4E one cell where xfx counted zero (A4c); `ps -E` lets a same-user child read xfx's launch environment (C2c docs); the TUI file index ran a repo's fsmonitor hook (C5c).

## R30–R40 — closed 2026-11-04

- result: batches 30–40 (`d4275500` … `ceae9b35`): SEC-C3d, C4d–C4h, C5d–C5i, A4d–A4h landed, every batch gated green (lib 3615→3646 / FI 3622→3653, permissions 149→158, tui 163 / FI 177, 0 failed, clippy 0 both; one load flake in batch 36's default run, alone 3× green). SEC-C5 closed in review round 6.

### Closed rows

| ID | Status | Scope | Branch | Next |
|---|---|---|---|---|
| SEC-C5 | ✅ | identity through links; auto git selects no driver, writes no trace/index, runs no hook; `.git` at any depth and every git dir by contents; include graph from git's own origins + byte-exact walk (C5f); the write that completes a git dir asks (C5g); per-seed include bound and pins (C5h); `commondir` arm pinned, dangling git-dir links count (C5i) — batches 34–40 | `fix/p2sec-exec-config-writes-d` @ `76776c21` | **closed round 6**: gpt-6 and opus MUST none on `e8d6da61` (C5i adds tests and asks only) |

### Round section (as it was in loop.md)

- 2026-11-04: batch 30 (`d4275500`: SEC-C3d, C4d, C5d). Gates on `d4275500`: lib 3615 / FI 3622, tui 163 / FI 177, 0 failed, clippy 0 both, fmt, identity, stubs, secrets ok. Round 3: A1, B1, C2, C3, D2 closed by both engines; A4 → A4d, C4 → C4e, C5 → C5e.
- 2026-11-04: batch 31 (`a414b8b2`: SEC-C4e, C5e) lib 3618 / FI 3625, tui 163 / FI 177; batch 32 (`76a0160b`: SEC-A4d) lib 3620 / FI 3627, tui 163 / FI 177; 0 failed, clippy 0 both, checks ok. Public-suitability pre-scan at `9aede930`: one personal path (already public on `main`) removed; unpushed history adds no personal path or credential (the `sk-` hits are fixtures); capture scratch paths `/private/tmp/claude-501/fxcap.*` only.
- 2026-11-04: batch 33 (`1abcf082`: SEC-A4e) lib 3623 / FI 3630, tui 163 / FI 177, 0 failed, clippy 0 both. (the review termination rule adopted this round is in `07-fx-parity-program.md` § Review rounds).
- 2026-11-04: batch 34 (`1f8310d6`: SEC-C5f), batch 35 (`6174a55e`: SEC-C4f), batch 36 (`d7cf0fcb`: SEC-A4f): lib 3629→3633 / FI 3636→3640, permissions 154, tui 163 / FI 177, clippy 0. Batch 36 default run: `tests/mcp.rs` `a_project_server_the_model_rewrites_after_approval_is_shown_and_asked_about_again` failed once at load 18 (binary 20.6 s); it passed in the same gate's fault-injection run and alone 3× (1.6–2.2 s) → counted as a load flake (G-copy-flake class).
- 2026-11-04: batch 37 (`4517d22e`: SEC-C5g, C5h, A4g) lib 3637 / FI 3644, permissions 158; batch 38 (`c4d8fd63`: SEC-C4g) lib 3642 / FI 3649; tui 163 / FI 177, 0 failed, clippy 0 both. C5g reproduced the staged bare-repo bootstrap (every step auto-admitted) before fixing it.
- 2026-11-04: batch 39 (`4423df8e`: SEC-C4h, A4h) lib 3646 / FI 3653, permissions 158; batch 40 (`ceae9b35`: SEC-C5i); tui 163 / FI 177, 0 failed, clippy 0 both. SEC-C5 closed in round 6.
- 2026-11-04: rounds 3–6 of the P2 security review. Reports and per-round status: zbrain `workflow/2lab-ai/repos/xfx-tui-phase4/p2-sec/round{3,4,5,6}/`. SEC-C5 closed in round 6 (gpt-6 and opus MUST none on `e8d6da61`); SEC-A4d and SEC-C4 continue.

## R41 — closed 2026-11-04

- result: SEC-A4d closed in review round 6 (both engines MUST none); its last product commit `92593b85` landed in batch 37, A4h (tests and docs) in batch 39.

### Closed rows

| ID | Status | Scope | Branch | Next |
|---|---|---|---|---|
| SEC-A4d | ✅ | follow-ups to SEC-A4: cluster pass (A4d), modifier/oversized/lone-indicator (A4e), full two-terminal sweep table — 1,111,999 scalars, 3,892 drawn wider (A4f), the invariant as the final pass + seeded property test (A4g), pins and version tie (A4h) — batches 32–39 | `fix/p2sec-ignorable-cp-d` @ `84c5be13` | **closed round 6**: gpt-6 MUST none on `92593b85`, opus MUST none on `84c5be13` (A4h tests and docs only) |

### Round section (as it was in loop.md)

- A4's review ran four more rounds after round 3 (A4d–A4h); the fixes moved from naming instances to a measured allowlist (every scalar on both terminals) enforced as a postcondition with a property test. Reports: zbrain `p2-sec/round{4,5,6}/reviews/A4*`.
