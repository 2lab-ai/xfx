# TUI Phase 2 — SSOT

Status: in-progress
Date: 2026-08-26
Base: main @ f1913a6be24f
Carrier: https://github.com/2lab-ai/xfx/issues/19

> 이 문서 속 좌표와 항목은 리드지 진실이 아니다. 실행 세션은 현재 repo에서 재검증한다.

## User command (verbatim)

> xfx 개선해줘
> 1. .prd 문서 업데이트해줘 (llmux 프로젝트의 .prd 참조해서 넣어줘)
> 2. 오리지널 fx의 tui와 최근 변경점 모두 반영해줘. (xfx니가 포팅한건 ui 개 병신임)
> 2. fx처럼 vercel, codex grok 설정 추가해주고 llmux도 같은 방식으로 추가할수 있도록해줘.
> 4. 반드시 서브에이전트 이용하여 포팅 내역에 대해서 tui 기준으로 작동하는지 QA하고 완료해줘

## Scope locked for this drive

Phase 1, the provider/config work, and the `.prd` foundation are shipped. This drive implements the release-quality Phase-2 contract in `.prd/03-tui-port.md` items 11–17 and proves `.prd/06-qa-harness.md` scenarios 13–21.

The product scope is:

1. Shadow-grid cell diff and no-op frame skip.
2. Debounced SIGWINCH resize with reflow, geometry recomputation and full repaint.
3. Slash registry, router and inline picker with dismissal memory.
4. Alt-screen file-diff approval with one-commit primary-screen restoration.
5. Prompt history with draft capture/restore.
6. Paste entity atomicity, span shifts, history renumbering and paste undo boundary.
7. OSC 2 title and kitty keyboard protocol with the mandatory tmux branch.

The carrier issue's additional findings are inputs, not automatic scope. Each must be implemented when it is part of items 11–17 or scenarios 13–21, or receive an explicit evidenced re-deferral recorded in issue #19 before closure. Scenarios 18 and 19 (`/setup` provider switching and `/model` catalog) are acceptance surfaces and therefore belong to this drive even though they extend item 13's slash system.

## Non-goals

- Phase 3 items 18–23 of `.prd/03-tui-port.md`.
- The explicitly deferred upstream breadth after the Phase-3 list.
- Stable releases, `v*` tags and production deployment.
- Rewriting Phase-1 architecture that already passed PR #18 unless a Phase-2 contract requires the seam.
- Source or branding duplication from upstream; this is behavioral parity on the pinned upstream contract.

## Acceptance

Apply `rules/DEV.md` §4 plus all of these:

- Every task in the Phase-2 implementation plan passes dual-persona external review unanimously.
- Every Phase-1 and Phase-2 QA scenario passes on release binaries on native macOS and Linux runners.
- `scripts/smoke-tui.sh` exits 0 and prints an evidence directory containing raw logs, grid snapshots and termios captures.
- Issue #19 has a terminal disposition for every inherited item and closes.
- The merged main commit publishes through the preview channel only.
- `brew upgrade xfx-preview` installs that preview and a real-config `XFX_TUI=1 xfx` session produces a captured response and exits cleanly.

## Inherited engineering standards

- RED first; every mutant actually runs and its raw failure is quoted.
- Gate receipts preserve the command's exit status (`cmd > log 2>&1 && echo OK`); no pipeline masking and no hand-summed counts.
- `usize` owns counts and indexes. Narrow only at terminal coordinates after clamping.
- PTY needles are response-only. Absence uses settled terminal state. State-transition tests enumerate every reachable sequence across asynchronous release windows.
- Tests pin policy literals rather than importing the value they claim to protect.
- UI oracles do not depend on animation phase or unmeasured kernel constants.
- Front-end semantic drift is fixed at shared boundaries, not by copying behavior between TUI and line shell.
- One writer owns the worktree at a time. Per-unit review is read-only.

## Durable pointers

- Carrier: https://github.com/2lab-ai/xfx/issues/19
- Phase-1 state: https://github.com/2lab-ai/xfx/issues/17
- Phase-1 implementation: https://github.com/2lab-ai/xfx/pull/18
- Canonical specs: `.prd/03-tui-port.md`, `.prd/06-qa-harness.md`, `docs/parity.md` on this branch
