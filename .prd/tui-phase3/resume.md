# Phase 3 — current execution map

Status: in-progress. Refreshed 2026-09-17 from the working tree and the controller's directly opened logs.

Branch `plan/tui-phase3`, HEAD `d02fda5a7959c125c743d7e98b9cbd5be290f016`. Mission and the fixed thirteen-row universe live in [`ssot.md`](ssot.md); the full chronological ledger and the work-unit plan live in [`loop.md`](loop.md). This file is the map, not the history: it says where the work actually stands and what each claim rests on.

Coordinates and prior reports are leads, not truth. Revalidate against the primary before changing the code they describe.

## The one-paragraph state

Most product slices are implemented locally and reviewed per unit; nothing is published. **Nothing has shipped** — `main` is still at the base, issue 20 is open, there is no PR, no preview, no install and no real-profile receipt. Item 21 is explicitly not whole: recovery and retention are **bounded partials**. The hardest single obstacle is the blocked independent real-terminal recovery QA — an exact `termios` comparison failed on one bit and the **cause is unknown**, with product, harness and environment all still live candidates and nothing exonerating any of them, after the two-attempt automation cap was reached. That blocker does not absorb the rest: P3-KEYS and the remaining QA are open on their own terms.

## Delivered / local / unverified

- **Delivered to users: nothing from Phase 3.** No commit on this branch has reached `main`; no release or tap carries any of it.
- **Local and externally reviewed, per unit:** editing history, question panel and tool, approval readiness, approval amendment drafts, layout settlement, live theme with semantic roles and visible retint, activity role colour, wrap memoization, the stateless output preflight, counted delivery containment, primary-band recovery, the give-up diagnostic, the retained-prefix traversal and its source-freshness tests.
- **Unverified / open:** independent real-terminal recovery qualification (blocked, see the stop boundary), the full CSI-u matrix condition, a fresh independent QA pass over the current tree, the whole delivery chain, and any claim that the current dirty tree *as a whole* has been reviewed.

## Working tree

Nine tracked files are dirty as measured at this refresh (2026-09-17, after this unit's own edits landed); all are intentional and must be preserved:

| File | What is in it |
|---|---|
| `.prd/tui-phase3/ssot.md` | The 2026-09-17 continuation instruction, recorded verbatim beside the unchanged universe |
| `.prd/03-tui-port.md` | Item 21 reconciliation: preflight, counted delivery, primary-band recovery, and the bounded retained-prefix adaptation |
| `.prd/06-qa-harness.md` | QA rows for the tracked recovery and live-theme scenarios |
| `.prd/tui-phase3/loop.md` | Ledger, gap matrix, work-unit plan |
| `.prd/tui-phase3/resume.md` | This file |
| `src/tui/diagnostic.rs` | Give-up diagnostic: `Reason`, `mark`, `report` |
| `src/tui/event_loop.rs` | Recovery arm, budget policy, diagnostic disposition, source-freshness tests |
| `src/tui/frame.rs` | Retained-prefix traversal (`first_row`), band footprint |
| `src/tui/grid.rs` | `diff_from` traversal start |

Untracked and also preserved: `docs/superpowers/plans/2026-09-10-tui-phase3-write-recovery.md`, `docs/superpowers/plans/2026-09-11-tui-output-checking.md`, `scratch/`, `scratchpad/`. They are drafts and session artifacts, not canonical; do not delete them as cleanup without reading them and having authorization.

## Code units and the exact scope each review covered

Local approval is per unit. **No review covered the whole dirty tree**, and every seat that reviewed anything declared its reading sampled and named files it did not open.

| Unit | Where | Review scope |
|---|---|---|
| Output preflight | `src/tui/check.rs` | Stateless per-vector semantic check, committed at `bf9020f`; reviewed as its own slice |
| Counted delivery | `src/tui/deliver.rs` | Unbuffered counted sink, typed `Rejected`/`ZeroProgress`/`Partial`; reviewed as its own slice |
| Primary-band recovery | `src/tui/frame.rs`, `event_loop.rs` | Same-call cleanup and rebuild for the primary band's `Partial` only; reviewed, plus two budget-narrowing rounds |
| Give-up diagnostic | `src/tui/diagnostic.rs`, `event_loop.rs` | Reason marking and the bounded independent file; the expired-budget `Partial` classification fix was the last trinity MUST-FIX and is closed |
| Retained-prefix traversal | `src/tui/frame.rs`, `grid.rs` | Final reviewer `acf9d9c` APPROVE; traversal start only, preflight untouched |
| Source freshness | `src/tui/event_loop.rs` tests | Final reviewer `a8998b5` APPROVE; three added tests, product prefix unchanged against the pristine copy |

## Trinity verdict — what it covered, and what the tree is now

The three primary seats each returned CODE APPROVE, MUST-FIX none, **SHIP BLOCKED**. That verdict's object was base `9acf15d` plus the final diagnostic delta, and at that time the two-file diff was byte-identical to the reviewed `diagnostic-fix-final.diff`, SHA-256 `32abe863748dd8853ad75366b3c00767798b5be0314924771fe5b4d382719185`. **That identity is historical.** Source-freshness tests have since been added to `src/tui/event_loop.rs`, so the current whole two-file diff is *not* that hashed object.

Current state, controller-verified 2026-09-17: the reviewed production diagnostic patch is unchanged, anchored to git base `9acf15d` plus the reviewed patch hash `32abe863…19185`; the later test additions were reviewed separately (`a8998b5`). The comparison was made against a `retained-source-freshness/event_loop.rs.pristine` snapshot in session scratch — that pointer is **temporary provenance and may expire**, the durable anchors are the base commit and the reviewed hash. This creates no obligation to archive the snapshot. The current `git diff 9acf15d -- src/tui/event_loop.rs src/tui/diagnostic.rs` hashes to `e3f3d8447d14b997bf92fe8f23446fd948911347c00b60089ab6ce3d12b68df8`. That is an observation of today's tree, not a review verdict: no approval attaches to the whole current diff. Do not carry the old hash forward as a byte-identity claim, do not read the trinity verdict as approval of the dirty tree, and do not dispatch a duplicate whole-branch review merely from this record.

## Gate receipts — fresh versus historical

**Fresh (controller opened these logs directly, all exit 0), scoped unit and compiler gates only:**

- `b5lktjeuf`: fmt, diagnostic 10/0, the exact expired-budget `Partial` regression 1/0, default all-target clippy
- `bgb5zmv5k`: input 49/0, theme 10/0, pacer 32/0
- `bih6mhjx2`: fault-injection all-target clippy, diff check
- `bz80i5045`: grid 28/0, frame 109/0/1 ignored, check 37/0, fmt, both clippy modes, diff check
- `bbs9iu006`: event_loop 104/0/2 ignored, fmt, default clippy, diff check
- `b3awtx02l` (WU-3): compile checks in default and fault modes, both clippy modes, fmt, diff check
- `b26fdkr7e` (WU-3): the eight enumerated unit filters, 10+49+10+32+28+109+37+104 = 379 passed / 0 failed / 3 ignored

- `b8kelm1xa` (WU-3): all four contract scripts — no-stubs ok, no-secrets ok over 136 tracked files, xfx-identity ok over 136 tracked files, preview-contract ok against `.github/workflows/preview.yml`

WU-3's scoped non-PTY checks are now **complete**, and bounded: **no `tests/tui.rs` test case was executed**, and no full native suite, release build, PTY scenario or delivery step has run against this tree.

**Historical, not fresh:** the integrated receipts of 2250 normal tests, 1580 fault-library tests, 91 fault PTY tests, CLI 46 checks and TUI 33 scenarios / 797 checks predate the traversal, source-freshness and diagnostic changes now in the tree. Do not quote them as this tree's numbers. No full gate, release build, CLI smoke or PTY suite has been run against the current working tree.

## Current work table

`met` below means **historically qualified when that unit closed**, not re-qualified against today's tree. Every row's shipment belongs to P3-SHIP.

| ID | State | What exists now | Evidence type | Remaining acceptance |
|---|---|---|---|---|
| P3-EDIT | met (historical) | Delta undo/redo with shared 100-entry/1 MiB budget, single-slot kill ring, committed `a03d08f` | External review, PTY scenario 22 grids, mutation closure | Preview and real-profile receipt |
| P3-QUESTION | met (historical) | Real `ask_user_question` registry entry, ordinal panel, freeform Other, cancellation semantics | Six unit reviews, release scenarios 23/23b, decoded fixtures | Preview and real-profile receipt |
| P3-APPROVAL | met (historical) | Readiness commit gate with per-request ids, independent amendment drafts, feedback as later user context | Reviews, scenarios 24/25, fatal-sink RED | Preview and real-profile receipt |
| P3-COMMIT | partial | Preflight `bf9020f`, counted delivery, primary-band recovery, retained-prefix traversal + source freshness | Unit reviews, tracked scenario 3d, fresh scoped gates | Parser-recovery resync unproven; retention beyond the prefix; blocked independent terminal QA |
| P3-LAYOUT | met (historical) | Closed-form ownership settlement, draft growth and yielding | Unit/scenario/docs reviews, controller 31 scenarios / 711 checks | No upstream geometry parity claim; preview receipt |
| P3-THEME | met (historical) | Semantic transcript roles, visible-document retint, `Deferred` distinct from settled | Code reviews, tracked scenario 27, raw colour cells | Offscreen native scrollback is never retinted; preview receipt |
| P3-KEYS | to work | Decoder names the pinned legacy and tilde spellings; no u-form emission observed. The source binding audit is **done and bounded-negative**: no named binding requires a u-form receipt rather than an available C0/ASCII/cursor/tilde spelling | Measured-negative reread of 9 captures, negotiation `1b5b3f3175`; bounded source audit (not repo-exhaustive) | Limb B — *any* supported key arriving in u-form — is still unmeasured; no physical or protocol-speaking terminal receipt exists |
| P3-ACTIVITY | met (historical) | Neutral ongoing-status role, dark 252 / light 238 | Review plus actual row-21 attributes | Preview receipt |
| P3-DIAGNOSTIC | partial | Implemented locally in `src/tui/diagnostic.rs`: `mark` stamps a `Reason`, `report` writes one bounded record to an independent file under the profile home after a restoration attempt; the original error's `Display` text never lands in it | Source read, 10/0 unit tests, expired-budget regression 1/0 | Fresh independent qualification absent; no fault-injection receipt on the current tree |
| P3-WRAP | met (historical) | Bounded settled-row reuse; serial release cost gate 8 ms / 32 ms in CI | Benchmarks, cost-gate script review | Integrated preview measurement |
| P3-QA | partial | 33 tracked scenarios including recovery 3d and live theme 27 | Historical 797-check run | Independent real-terminal recovery QA blocked; no fresh pass on this tree |
| P3-DOCS | partial | `03`/`06`/parity reconciled through the retained-prefix adaptation | Docs reviews, final `a8998b5` | Final current-tense pass once the tree stops moving |
| P3-SHIP | to work | Nothing done | Public issue API | The entire delivery chain |

Accounting: 13 rows = met 7 + partial 4 + to work 2.

## Stop boundary — unchanged

Independent terminal recovery QA is not qualified. The normal control displayed TUI input/clear and exited with status 0, but exact termios comparison failed: lflag `0x200005cb` before versus `0x5cb` after, XOR `0x20000000` (PENDIN). Other recorded stty fields matched. Neither product causation nor harness causation has been established. Independent once-recovery and fatal-recovery trials were not run.

After the two-attempt limit, the human selected "원인 조사 후 제한적 재시도". That permitted limited cause isolation followed by corrected QA, not masking flags, weakening equality, or waiving shipping gates.

The subsequent no-product isolation probes were inconclusive:
- Interactive shell plus `/usr/bin/true`: PENDIN 1→1 in that observation.
- Direct execution plus `/usr/bin/true`: PENDIN 0→0 in that observation.
- The raw/restore probe changed only lflag, omitting the product input flags, CS8 and VMIN/VTIME transform. Its observed lflag equality is not a full-product round-trip proof.
- A later child in the interactive pane read PENDIN 0 at entry; the original within-wrapper 1→0 transition was not reproduced. Shell/kernel activity between reads was not traced.

The following read-only Darwin/XNU investigation returned no finding because the API reported "Sonnet 5's safeguards flagged this message", category `[cyber]`, request `req_011Cf2esiJkyQBGtmW7XZBEp`. Task #37 recorded BLOCKED-EXTERNAL. This is a historical API result, not a technical judgment about PENDIN. Do not route that blocked investigation through another model, agent, tool or manual handoff to bypass the safeguard. Automatic goal/stop notifications are not new human authorization.

## What happens next, in order

The executable plan with scope files, acceptance and proof gaps is the work-unit table in [`loop.md`](loop.md). The dependency shape:

1. **Done — each within a narrow scope, none of it qualification.** WU-1 returned bounded-negative: no named binding requires a u-form receipt, on coverage that was bounded rather than repo-exhaustive. WU-2 returned as a **source reconciliation only** — implementation criteria established, fault execution not reverified. WU-3's scoped non-PTY checks passed. What remains open at this level is **live measurement for P3-KEYS** (Limb B, any supported key arriving in u-form, has no terminal receipt) and a fresh execution of the four already-authored native diagnostic falsifiers. `cargo test --all-targets` was deliberately not that gate: it runs `tests/tui.rs` against an actual PTY, so the native suite stays with WU-5.
2. **Blocked:** WU-4, the independent recovery QA, under the exact stop boundary above — including the two-attempt cap and the untouched Darwin/XNU prohibition.
3. **After the blocker clears, in this order:** full native gate and independent QA on a release binary, review consensus over the *current* tree rather than the older base, CI, merge, preview, tap and install, a real-profile TUI receipt, then closing issue 20.

Steps 1 and 3's early items proceed without asking for per-task approval. Stable releases, regular `v*` tags and production deployment keep their user gates. Do not terminate or restart the user's llmux daemon or modify the user's default tmux server. Use fake credentials and loopback in fixtures; never copy real credentials into automated fixtures or public reports. One writer at a time: a controller gate must not run against a tree an agent is mutating, and workers do not commit. Preserve unrelated zbrain changes.
