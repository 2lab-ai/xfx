# Phase 3 continuation record

Status: in-progress

Recorded: 2026-09-14. Source checkpoint: `plan/tui-phase3` @ `9acf15d1562a7fa33097eb5831d1583547d6611c`.
Coordinates and prior reports are leads, not truth; the reading session must revalidate current source and primary evidence.

## Authority

Mission: “xfx TUI Phase 3와 Phase 2에서 명시적으로 넘긴 범위를 구현해 successor carrier를 닫아라.”

Scope and original user request: `.prd/tui-phase3/ssot.md`. Product and QA contracts: `.prd/03-tui-port.md`, `.prd/06-qa-harness.md`. Successor: https://github.com/2lab-ai/xfx/issues/20 . Detailed implementation history: `.prd/tui-phase3/loop.md`.

## Stop boundary

Independent terminal recovery QA is not qualified. The normal control displayed TUI input/clear and exited with status 0, but exact termios comparison failed: lflag `0x200005cb` before versus `0x5cb` after, XOR `0x20000000` (PENDIN). Other recorded stty fields matched. Neither product causation nor harness causation has been established. Independent once-recovery and fatal-recovery trials were not run.

After the two-attempt limit, the human selected “원인 조사 후 제한적 재시도”. That permitted limited cause isolation followed by corrected QA, not masking flags, weakening equality, or waiving shipping gates.

The subsequent no-product isolation probes were inconclusive:
- Interactive shell plus `/usr/bin/true`: PENDIN 1→1 in that observation.
- Direct execution plus `/usr/bin/true`: PENDIN 0→0 in that observation.
- The raw/restore probe changed only lflag, omitting the product input flags, CS8 and VMIN/VTIME transform. Its observed lflag equality is not a full-product round-trip proof.
- A later child in the interactive pane read PENDIN 0 at entry; the original within-wrapper 1→0 transition was not reproduced. Shell/kernel activity between reads was not traced.

The following read-only Darwin/XNU investigation returned no finding because the API reported “Sonnet 5's safeguards flagged this message”, category `[cyber]`, request `req_011Cf2esiJkyQBGtmW7XZBEp`. Task #37 recorded BLOCKED-EXTERNAL. This is a historical API result, not a technical judgment about PENDIN. Do not route that blocked investigation through another model, agent, tool or manual handoff to bypass the safeguard. Automatic goal/stop notifications are not new human authorization.

## Local work and evidence boundaries

The source checkpoint contains primary-band recovery, semantic transcript roles, visible-document retint and tracked recovery/theme smoke scenarios. The preceding integrated controller receipt recorded 2250 normal tests, 1580 fault-library tests, 91 fault PTY tests, CLI 46 checks and TUI 33 scenarios/797 checks. These are historical receipts, not a claim about a fresh run or installed preview.

At this record's creation, `src/tui/event_loop.rs`, `src/tui/diagnostic.rs` and `.prd/tui-phase3/loop.md` had uncommitted changes. Preserve them. The runtime delta retains the current Partial error and records reason `partial` when an earlier failure has already exhausted the budget; it does not attempt recovery after the deadline. Its regression checks the current BrokenPipe/accepted-byte evidence and exact four-field diagnostic JSON. A later header edit was comments-only. The session reported a full gate pass for that delta but did not reaggregate its final test count; do not reuse 2250 as that run's count.

Final trinity verdict provenance still needs reconciliation from existing reports. The session summary contains an Anthropic final approval but also a later pending-seat treatment. Do not infer unanimity or dispatch a duplicate whole-branch review merely from this record. Every recorded shipping disposition remained blocked.

Two draft plans and the worktree's `scratch/` and `scratchpad/` directories were untracked. They are not canonical artifacts and were not included in this checkpoint. Preserve existing evidence; do not delete them as cleanup without inspecting their contents and authorization.

## Remaining acceptance

Reconcile requirement rows, including the conditional full kitty CSI-u scope, against the successor issue and current contracts. Local approval is distinct from independent terminal qualification. After an authorized unblock and qualifying evidence, complete the project gates, independent subagent TUI QA, review consensus, CI, preview delivery, installation and actual-profile TUI receipt before closing the successor. Phase 3 remote integration, preview installation and successor closure had not been performed at this record's creation. Do not repeat completed Phase 2 delivery.

Stable releases, regular `v*` tags and production deployment need separate approval. Do not terminate or restart the user's llmux daemon or modify/terminate the user's default tmux server. Use fake credentials and loopback in fixtures; never copy real credentials into automated fixtures or public reports. A writer and controller gates must not mutate/test the same tree concurrently; workers may not commit concurrently. Preserve unrelated zbrain changes.
