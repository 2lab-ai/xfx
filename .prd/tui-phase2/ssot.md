# TUI Phase 2 — SSOT

Status: shipped
Date: 2026-08-26 (shipped 2026-08-29)
Base: main @ f1913a6be24f
Delivered on main: `09fa056` (PR #21) and `c5cbad56` (PR #22, test-only hotfix)
Carrier: https://github.com/2lab-ai/xfx/issues/19 (closed 2026-08-29)
Successor carrier: https://github.com/2lab-ai/xfx/issues/20

> `shipped` covers the whole chain, and every acceptance row below carries an observed receipt:
> the product contracts are in the binary, every Phase-1 and Phase-2 QA scenario is registered and
> green on a real terminal, every carrier row has a terminal disposition, the Phase-2 documents
> describe what the code does, and the merged commit reaches a real machine through the preview
> channel and answers a real prompt there. The receipts are in §Delivery.

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

Apply `rules/DEV.md` §4 plus all of these. Each row states the receipt that closes it.

| Row | Receipt |
|---|---|
| Every task in the Phase-2 implementation plan, including WU 7 checkpoints 7A/7B/7C, passes dual-persona external review unanimously | every WU and every WU 7 checkpoint carries its own unanimous panel; the full-branch panel and both hotfix scoped panels are unanimous approve, MUST-FIX none |
| Every Phase-1 and Phase-2 QA scenario passes on release binaries on native macOS and Linux runners | exact-head PR run **33269357061**, all four native targets success; merge CI **33269934848**, all four success |
| `scripts/smoke-tui.sh` exits 0 and prints an evidence directory containing raw logs, grid snapshots and termios captures | `23 scenarios + the oracle, 511 checks, 0 failures`, repeated locally and on every CI target |
| Issue #19 has a terminal disposition for every inherited item and closes | closed 2026-08-29; the spine in `loop.md` holds the dispositions, and the four re-deferrals are on issue #20 |
| The merged main commit publishes through the preview channel only | preview run **33269934844** success; prerelease tag `preview-2026-08-29-190455-33269934844-1-c5cbad561e62`; tap commit `4e7f5b19`. No `v*` tag and no stable release |
| `brew upgrade xfx-preview` installs that preview and a real-config `XFX_TUI=1 xfx` session produces a captured response and exits cleanly | Homebrew reports `2026.08.29.190455.33269934844.1`; `xfx status --json` reports preview revision `c5cbad561e62` and provider llmux on loopback; the session receipt is in §Delivery |

## Delivery

The chain, end to end, as observed:

1. **Merge.** PR #21 landed the Phase-2 branch at main `09fa056`. PR #22 landed a **test-only**
   hotfix at main `c5cbad56` for two CI harness races found after the first merge (§CI harness
   races below).
2. **CI.** Exact-head PR run **33269357061**: all four native targets success. Merge CI
   **33269934848**: all four success.
3. **Preview.** Run **33269934844** success, publishing prerelease tag
   `preview-2026-08-29-190455-33269934844-1-c5cbad561e62` and tap commit `4e7f5b19`. The preview
   channel is the only channel used; the non-goal against stable releases and `v*` tags holds.
4. **Install.** Homebrew upgraded to `2026.08.29.190455.33269934844.1`. `xfx status --json` on that
   install reports preview revision `c5cbad561e62` and provider llmux on loopback.
5. **Live session.** A real-config `XFX_TUI=1 xfx` in an empty workspace: `/model gpt-5.6-sol`, then
   a prompt whose answer had to carry a **computed** needle. The needle `582e266d66bc2d59f9dc4b30`
   appears exactly once, response-only — no cancellation notice — and the session exits 0.

The receipt carries no credentials, because the run produced none: the provider is llmux on
loopback and nothing in the capture is secret.

### CI harness races found after merge

Both were **test-only** and neither changed a product contract:

- A wait that returned on the alternate screen's *opening* bytes rather than on a complete frame, so
  an assertion could read a frame the terminal was still receiving. Fixed in `075ce7b`.
- A resize case that measured its "and then nothing happened" baseline from **before** the child
  could know the screen had changed, counting a frame composed for the screen the session still
  believed in. The baseline moved to the far side of that observation, and the claim is now provoked
  with a keystroke that owes a frame. Fixed in `e94af06`.

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

- Carrier: https://github.com/2lab-ai/xfx/issues/19 (closed 2026-08-29)
- Successor carrier and the four re-deferrals: https://github.com/2lab-ai/xfx/issues/20#issuecomment-5464059512
- Phase-1 state: https://github.com/2lab-ai/xfx/issues/17
- Phase-1 implementation: https://github.com/2lab-ai/xfx/pull/18
- Phase-2 implementation: https://github.com/2lab-ai/xfx/pull/21 (merge `09fa056`)
- Test-only hotfix: https://github.com/2lab-ai/xfx/pull/22 (merge `c5cbad56`)
- Canonical specs: `.prd/03-tui-port.md`, `.prd/06-qa-harness.md`, `docs/parity.md` on main
