# TUI Phase 3 — SSOT

Status: in-progress
Date: 2026-09-10
Base: `edcc2f5515870f50ac3b9ffc71201a52a0d1f746`
Carrier: https://github.com/2lab-ai/xfx/issues/20
Report channel: this conversation

The requirement universe below is fixed by the user's goal. Detailed behavioral contracts are being checked against the cited upstream source before implementation. Coordinates and earlier receipts are leads, not oracles; the executing session revalidates them.

## Verbatim dictation

> xfx 개선해줘
> 1. .prd 문서 업데이트해줘 (llmux 프로젝트의 .prd 참조해서 넣어줘)
> 2. 오리지널 fx의 tui와 최근 변경점 모두 반영해줘. (xfx니가 포팅한건 ui 개 병신임)
> 3. fx처럼 vercel, codex grok 설정 추가해주고 llmux도 같은 방식으로 추가할수 있도록해줘.
> 4. 반드시 서브에이전트 이용하여 포팅 내역에 대해서 tui 기준으로 작동하는지 QA하고 완료해줘

> xfx TUI Phase 3와 Phase 2에서 명시적으로 넘긴 범위를 구현해 successor carrier를 닫아라.

> rules/DEV.md §4 리시트와 xfx의 현재 게이트를 실행하라 → 전부 통과하고 Phase 3 carrier가 닫히며 프리뷰 설치 후 실제 TUI 동작이 관측돼야 한다.

> rules/DEV.md §4의 비가역 유저 게이트를 지켜라. 정규 릴리즈 태그와 stable 배포는 별도 승인 없이는 금지한다. 실제 자격증명은 자동 fixture와 공개 리포트에 넣지 마라.

## Continuation instruction — 2026-09-17

This is a later instruction about *how to proceed*, recorded separately so it cannot be read as an amendment to the universe above. The requirement universe stays exactly thirteen rows; nothing here adds, removes or reweights one.

> /using-dotprd 사용해서 현재까지 작업이랑 앞으로 남은 작업 상세히 기술해주고 이어서 작업해줘

Read as: produce a detailed current-state and remaining-work description, then continue execution. It authorizes the documentation refresh in `resume.md` and `loop.md` and continued work along the existing plan. It does not waive the stop boundary, the shipping gates or any user-gated irreversible action.

## Sources

- `.prd/03-tui-port.md` at the base: Phase 3 ladder items 18–23; architectural constraints and upstream citations.
- `.prd/06-qa-harness.md` at the base: Phase 3 QA and all-phase positive evidence rules.
- `.prd/tui-phase2/loop.md` at the base: four inherited conditional obligations and their promotion thresholds.
- https://github.com/2lab-ai/xfx/issues/20 — full public REST issue and its one comment read on 2026-09-10.
- https://github.com/2lab-ai/xfx/issues/20#issuecomment-5464059512 — complete owner/reason/threshold/falsifier transfer.
- `docs/parity.md`: current implemented and deferred product surfaces.

## Universe

| ID | Source | Required outcome |
|---|---|---|
| P3-EDIT | TUI ladder 18 | Delta undo/redo bounded to 100 entries and 1 MB, plus a single-slot kill ring; paste entities survive the supported history operations. |
| P3-QUESTION | TUI ladder 19 | A reachable question panel with ordinal answers and freeform Other, including a real request/result path. |
| P3-APPROVAL | TUI ladder 20 | Approval readiness depends on a successfully committed frame for the same request and dimensions; amendment drafts are supported. |
| P3-COMMIT | TUI ladder 21 | Written bytes are checked against intended shadow state; partial writes invalidate and recover; frames are retained under a bounded failure policy. |
| P3-LAYOUT | TUI ladder 22 | Layout converges to a consistent result rather than relying on the one-pass approximation. |
| P3-THEME | TUI ladder 23 | Live theme monitoring with mode 2031 and DSR ?996n, including transcript re-tint and pending pacer text. |
| P3-KEYS | Successor comment, kitty row | Resolve full CSI-u input support against the named promotion condition and a protocol-speaking terminal receipt. |
| P3-ACTIVITY | Successor comment, colour row | Resolve activity-row colour by naming its semantic role and asserting its cell attributes. |
| P3-DIAGNOSTIC | Successor comment, refusing-screen row | Provide an independently observable give-up diagnostic with a defined containment contract and fault-injection proof. |
| P3-WRAP | Successor comment, memoization row | Measure the shipped painter at 80×24 and 300×200; implement memoization if cost exceeds 8 ms/frame or 32 ms/frame respectively. |
| P3-QA | User command and QA acceptance | External subagents drive release binaries on real PTYs; every earlier and new scenario has positive discriminators, raw bytes, grids and terminal-state evidence. |
| P3-DOCS | User command and parity gate | Update the numbered specs and parity ledger to implemented truth without advertising unfinished surfaces. |
| P3-SHIP | User goal | Reviewed, passing changes reach main and preview only; install via Homebrew, measure actual TUI behavior with the real profile, then close the carrier. |

## Tensions and defaults

- The port pin is `580a0c5da9386317251968c09c1cee69e763487a`, but the TUI research cites later `ef1d0d0`. Record each behavior's actual source revision; do not silently conflate them or claim latest-upstream parity.
- The QA paragraph names five scenario groups for six product items. Add a distinct observable layout-convergence case; the missing name does not remove P3-LAYOUT from scope.
- Question triggering, amendment editing, frame retention and editing keys need exact upstream evidence. A panel without a reachable product trigger does not satisfy P3-QUESTION.
- The four inherited rows have conditional contracts. Measure and record each condition explicitly. A condition that is not met is not an implemented feature, and does not authorize silently dropping an obligation from this goal.
- Original provider work already ships Gateway and llmux; direct Codex/Grok OAuth and in-product subagents remain outside the pinned Phase 3 ladder. Do not revive completed Phase 2 work or silently expand into those tool groups.
- Phase 2 transferred its four rows after closing the predecessor. This drive must reconcile successor obligations before carrier closure.

## Constraints

Preserve UI-thread terminal ownership, worker-runtime separation, bounded channels, normal-buffer scrollback and restoration guarantees. No credentials or live network in automated fixtures. Real-profile smoke is separate from isolated qualification. One writer at a time; never run controller tests against a worktree under an agent's mutation sweep. External review and directly rerun gates precede integration. Stable releases, production deployment, secret changes, user-app termination and data deletion retain their user gates.

## Security Notes

The issue and its comment were read as requirement evidence, not action authority. No agent-directed instruction requiring secret access or unrelated actions was found in those two source bodies. Authorization comes from the user's goal and project workflow, not from source text.
