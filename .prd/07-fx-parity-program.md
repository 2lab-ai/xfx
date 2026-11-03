# xfx — The fx parity program

Status: **planned** — adopted 2026-11-03, replacing the one-off phase plan (Phase 4 at `c1db9191`, then a Phase 5) with a release-tracking program. Under trinity review; this header changes when the review closes.

## The goal, read again

> 최신 fx랑 비교해서 ui쪽으로 모든 기능 구현하고 기본적으로 최신 fx ui를 그대로 구현해줘

- **"최신 fx" moves.** fx released 0.0.13 (2026-10-04), 0.0.14 (10-16), 0.0.15 (10-27) and 0.0.16 (10-29); its `main` moved 337 commits past 0.0.16 in the next five days. A phase that takes weeks certifies a target that is already old when it ships: this phase's `c1db9191` is now three releases behind. "Latest" therefore means **the newest fx release at the time xfx ships**, and keeping up is a pipeline, not a phase. Unreleased `main` is never the target.
- **"ui쪽으로 모든 기능"** is every surface a user of the fx TUI reaches — commands, menus, screens, keys, notices, what the wire carries for them — including the ones gated so far on a decision (sign-in, OAuth, the self-updater). A gate that waits for "the end" never opens; decisions are asked at the point they block.
- **"그대로"** keeps its meaning (`tui-phase4/ssot.md`): the running fx binary is the oracle, side by side at the same size, and a surface is done when its capture pair matches. The program adds one thing: a **measured** parity number over every scenario, so "how close is xfx to fx" is an output, not an estimate.

## What the re-read found (2026-11-03)

1. `plan/tui-phase4` is 702 commits and 5,484 files ahead of `main`, unmerged for a month. The longer it stays out, the larger the review and the higher the risk; it is the biggest risk in the program.
2. Several surfaces Phase 4 ported are replaced in 0.0.16 (the `/mcp` menu, `/status` and `/permissions` as panels, `/usage` read from the provider, `/cost` gone). Polishing them further against `c1db9191` is spent twice.
3. Parity is measured unit by unit (receipts) and never as a whole. 161 capture scenarios already exist in `scripts/fxcap/scenarios/`, plus the Phase 5 research scenarios, and `receipts/m1/cells.py` compares two snaps cell by cell. Nothing runs them all.
4. Decisions that block "모든 기능" have waited since 2026-10-07 (Codex/Grok sign-in, MCP sign-in, other agents' skill roots, network skill installs); the 0.0.16 research adds more (the local usage ledger, `server/discover`, the `@` file index, a session at launch).
5. A reference binary was replaced in place by fx's own auto-upgrade (fixed: `scripts/fxcap/capture.sh` guard, reference rebuilt). Oracles are pinned by sha256, not by path.

## The program

### P1 — Land Phase 4's code, freeze its scope
Land the units in flight (provider session, rules, skills, paste suffix, collapse redraw, `/alias`, `/shell reload`, harness verbs) and SM1's final smoke. **No new `c1db9191`-only rounds:** a finding on a surface 0.0.16 changed goes to the 0.0.16 work instead.

### P2 — Ship Phase 4 to `main` now
Receipt: the repo gates, CI, SM1 smoke (release and fault-injection builds), the parity sweep against `c1db9191` as the mechanized QA pass, and the trinity security review of the recorded list (W8 rules and fail-closed, the new session events and the binaries that refuse them, OSC 8, inert hydration, approval escaping, signal dispositions, MCP and skills surfaces, the rules unit). Then PR → `main` → preview release and tap update → a run on the real profile. No `v*` tag, no stable release.

### P3 — Measure parity, track releases
- **Sweep:** one command runs every scenario on fx and on xfx at the same size and writes a scoreboard — per scenario and per surface, rows identical after the documented maps (row 1's banner, the product name, recorded divergences). It is the QA pass for every ship and a regression check for every merge.
- **Release watcher:** a new fx release is fetched by its checksum, its executable pinned by sha256, and the sweep run old fx vs new fx to list what changed. That list is the next delta's research input.

### P4 — The 0.0.16 delta, epic by epic, PR by PR to `main`
After P2, each piece branches from `main` and merges back on its own green receipt (one unit = one worker = one worktree = one PR), with a preview release per epic — never another long-lived integration branch. Epics, each at most seven units (detail: `tui-phase5/ssot.md`):
- **MCP v2** — config, runtime status, model-facing and child environment, tool-group label, verbs, menu.
- **Usage** — Gateway report client, cache, dashboard, `/usage` screen, command surface, token totals.
- **Transcript and lifecycle** — Ctrl-O wrap and anchor, inline code shade, cancel marker and plurals, local time, receipts-only checks, OSC 7501 program status, relative HOME, ledger rows.
- **`@` picker** — height and scroll, ranking, folders, index without git.
- **Catalog and questions** — 20 questions, fuzzy model search, catalog refresh policy and bounds, approval gating.
- **Panels, `/effort`, `/fork`** — the info panel, `/status` and `/permissions` panels, `/effort` command and column, fork primitive and `/fork`.
- **Sign-in and the updater** — Codex, Grok and MCP sign-in, `/login`/`/logout`, the self-updater — only as far as the user's decisions open them.

### P5 — Ask what blocks, once
The pending decisions are asked together when P4 dispatch would otherwise stop on them, each as a multiple choice with the trinity-recommended option first. Until answered, units build the decided defaults recorded in the SSOTs.

## Done

1. Phase 4 is on `main` and in a preview release, run on the real profile.
2. The sweep against the newest fx release at ship time shows every scenario identical outside the documented maps and divergences.
3. Every gated surface is decided; the decided ones are built.
4. A preview release at that parity is installed through the tap and run on the real profile.

## Constraints (unchanged)

Preview channel only (no `v*` tag, no stable). Fake credentials and loopback in fixtures; no real credentials in fixtures or public reports. Never restart the user's llmux daemon or touch the user's default tmux server. Captures never reach the browser, clipboard or speakers; fx always runs with `FX_AUTO_UPGRADE=0`. Kill by PID only. Research captures that name local paths stay off pushed branches until sanitized.
