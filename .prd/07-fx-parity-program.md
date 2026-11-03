# xfx — The fx parity program

Status: **in-progress** — adopted 2026-11-03 after a three-engine review (grok-4.7, gpt-6-astra, fable), unanimous at round 5 (APPROVE-WITH-CHANGES; the changes are folded in below). It replaces the one-off phase plan (Phase 4 at `c1db9191`, then a Phase 5) with a release-tracking program.

## The goal, read again

> 최신 fx랑 비교해서 ui쪽으로 모든 기능 구현하고 기본적으로 최신 fx ui를 그대로 구현해줘

- **"최신 fx" moves.** fx released 0.0.13 (2026-10-04), 0.0.14 (10-16), 0.0.15 (10-27) and 0.0.16 (10-29); its `main` moved 337 commits past 0.0.16 in the next five days. "Latest" is a property of **cadence** — close each pin before the next release matters — not of an acceptance check on ship day, which would never go green. Unreleased `main` is never the target; it is research only.
- **"ui쪽으로 모든 기능"** is every surface a user of the fx TUI reaches — commands, menus, screens, keys, notices, what the wire carries for them — including those gated on a decision (sign-in, OAuth, the self-updater). fx's behaviour specifies the UI; it is **not** approval to use credentials, open new network access or delete data — those stay the user's, asked where they block.
- **"그대로"** keeps its meaning (`tui-phase4/ssot.md`): the running fx binary is the oracle, side by side at the same size; a surface is done when its capture pair matches. The program adds a **measured** parity number with a closed denominator.

## Pins

- A delta opens on one fx **release tag + commit + executable sha256**; that pin is its ship bar. A newer release does not move it — it is the next delta. One pin in flight. No silent re-pin; an explicit re-baseline only for a safety problem or a delta made functionally obsolete, with what was dropped written down.
- Every ship reports its result against its pin **and** its lag behind the newest release. The lag is a label, not a failure.
- `c1db9191` (sha256 `7b76319a…`) is Phase 4's pin and, after it ships, the regression floor for code still current in 0.0.16 (KEEP). `0.0.16` (`481104fd`, executable sha256 `8e6cda2e…`) is the next delta's oracle.
- **Tripwire:** if the backlog (units left on the pin plus new surfaces the watcher found) grows over two consecutive pins, the next delta is held and the program re-judged — reversible levers first (delete, merge, split, parallelize). The user is asked only if the goal sentence itself would have to change, or for a security or irreversible choice.

## Measurement (epic 0)

- **Sweep:** one local command runs every scenario on fx and xfx at the same size, after checking both binaries' sha256, and writes a scoreboard. Columns: total · ran · identical · normalized-identical · approved-divergence · replaced-in-next-pin · unresolved-defect · unverified · failed · timeout. Raw diffs are kept; every local path is redacted before anything is committed or published.
- **Ledger** (reviewed in PRs): *normalizations* — a fixed mechanical list (row 1's banner, the product name); *approved divergences* — only for a reason outside xfx: a user decision, a reproduced fx defect or behaviour that cannot be copied safely, an explicit interim decision for a surface the next pin replaces, a real platform limit — each with an owner, a resolution condition and an expiry (none without an expiry; review rounds or implementation difficulty are never a reason); everything else is an *unresolved defect*.
- **Denominator:** everything stays in it. The ship gate's pass rate leaves out only `replaced-in-next-pin` and awaiting-decision rows (still printed); unresolved, unverified, failed and timeout rows fail the gate. Coverage and pass rate are reported separately. The denominator is closed: the pinned fx binary's commands, help entries, keys and screens are mapped to scenarios; uncovered surfaces are counted.
- **Invariant:** no unapproved regression on the same pin and the same scenario set. Denominator changes and new findings are separate columns, never penalties.
- **Watcher** (after the sweep): a new release is fetched by checksum, pinned by sha256, and diffed release to release — the sweep old-vs-new plus the surface inventory diff, so new commands, keys and screens are found, not only changed ones.
- **CI:** Phase 4's ship uses the local sweep plus the existing CI. Before the first 0.0.16 feature PR, the sweep runs in CI on one fixed capture runner (macOS arm64) against the 0.0.16 release binary: per PR the gates plus the changed surfaces and a core set; the full sweep in the merge queue and at each epic preview. CI green never replaces the local re-run.

## Phase 4 close-out (P2)

**Preconditions** — decided by the safety and dependencies of the code that ships, not by unit names:
1. R1 round 2's security half: every registered tool judged against the permission table at admission (fail-closed), M1's MCP admission merged with read-ask and hidden-tool rules.
2. S1 round 3's resource identity re-check (skills are on by default; the alternative is skills discovery made inert, with a regression test).
3. Execution evidence for the resize and signal behaviour scenarios at the final integration SHA (the harness verbs exist, H1).
4. Half-applied edits on these paths are finished or reverted; nothing ships fail-open.

**After P2, as their own PRs:** the visual halves of R1 and S1, X1 round 2, P1 rounds 3–4 (the held-prompt resend is new code; its acceptance: a held prompt is never sent without the user's confirmation, or exactly as the pinned fx does), D4e, C7 round 2, C8 `/shell reload` (a new execution surface with its own security review), and the KEEP gaps the harness found (resize leaves a stale row, Ctrl-Z, external SIGTSTP, stream-failure recovery, MCP status words).

**Receipt** at the final SHA: the repo gates re-run; CI green; SM1 smoke on release and fault-injection builds with the behaviour list (paste+Enter, cancel, approval allow/deny, resize, signal, restart/restore, session-log read by an older binary); the local sweep against `c1db9191`; an external review of the unreviewed delta, the unit seams and the safety list (fail-closed rules, session-log back-compat, OSC 8 inert, inert hydration, approval-command escaping, inherited signal dispositions, MCP and skills paths that are reachable) — the PR body says the 702 commits were not reviewed line by line and names what was; representative frames read by the dispatcher and attached to that review; a public-suitability scan of every file and the history (local paths, credentials); RED/GREEN for the paste-suffix defect and the reference-binary guard.

The sweep at the final SHA is **run and reported, not passed**: Phase 4's open KEEP gaps are the after-P2 list above, so its ship does not require the pass gate. Its scoreboard (totals, the cascade column, the root-cause table) goes in the PR body and becomes the `c1db9191` floor for the Invariant; the pass gate applies from the first 0.0.16 ship. (Decided 2026-11-03 from §P2's preconditions and §Pins' "regression floor"; reversible.)

**Ship:** PR → `main` → preview release and tap update → a run on the real profile after its session directory is backed up and compatibility is checked (an in-place rewrite of existing sessions would be the one irreversible step to ask about). Labelled **"Phase 4 integration preview — fx c1db919 baseline, before the 0.0.16 transition"** plus the lag; never a latest-parity claim. No `v*` tag, no stable release.

**KEEP/REPLACE:** after the merge, no more `c1db9191` visual rounds on surfaces 0.0.16 replaced (`/mcp` menu, `/status` and `/permissions` as panels, `/usage`, `/cost`); reachable REPLACE code still gets safety and behaviour checks.

## The 0.0.16 delta

Per-unit PRs straight to `main` (one unit = one worker = one worktree = one PR), each with its own green receipt; a preview per epic, at epic done, never mid-epic; no long-lived integration branch. A unit that changes a default-visible surface merges dark or waits until its epic's slice is coherent. Shared hot files have one owner epic at a time. Every unit's done includes its scenarios, its smoke rows, its docs and the public-suitability scan.

Order — the surfaces a preview user sees as stale go first: **panels, `/effort`, `/fork`** → **MCP v2** (the host menu without sign-in) → **Usage** (first decision point: the local ledger) → **Transcript** → **Lifecycle** → **`@` picker** → **Catalog and questions** → **Sign-in and the updater** (defaults off; only as far as the user's decisions open them). Each epic: at most seven units, a dependency order, preconditions, done evidence and open decisions (detail: `tui-phase5/ssot.md`).

**Review rounds:** a safety finding has no cap and blocks the merge. A parity finding still open after two rounds on the same diff is fixed, re-split into a new unit, or left unresolved (which blocks that claim's merge) — never turned into an approved divergence by fatigue. Round counts are a throughput metric. **Termination** (adopted 2026-11-04 at round 5 of the P2 security review): reviewers read a frozen tree at the reviewed head, and the brief states the bar and the decided items. A unit closes when both engines return MUST none on the same head in the same round. A MUST needs a precondition the model can create, on a path to the same sink; a precondition only the user can create is a SHOULD unless failing closed is a one-line change. An inferred item is fixed only after a failing test reproduces it, and one that does not reproduce returns to its reviewer. If a unit keeps drawing new plausible items after a self-contained brief, the next step is to put a ship-with-recorded-residual question to the user (a security decision), not another follow-up.

## Decisions

Asked only at the point they block, in one batch per point (at most seven, the recommended option first, multiple choice). Nothing blocks Phase 4's close-out or the sweep. The first point is the Usage epic (retiring xfx's local usage ledger loses the llmux usage view). Credentials and phoning home (Codex/Grok/MCP sign-in, the self-updater), network and other agents' disks (skill roots, network skill install, MCP `server/discover`), and making an inert channel live (the OSC 8 allowlist, a saved `@` index) are asked when their epic is dispatched. Reversible implementation choices are decided by the worker.

## Done

1. Phase 4 is on `main` and in a preview release, run on the real profile.
2. The sweep against the delta's pinned release passes its gate (above) with the coverage reported, and the ship reports its lag behind the newest release.
3. Every gated surface is decided; the decided ones are built.
4. A preview release at that pin is installed through the tap and run on the real profile.

## Constraints (unchanged)

Preview channel only (no `v*` tag, no stable). Fake credentials and loopback in fixtures; no real credentials in fixtures or public reports. Never restart the user's llmux daemon or touch the user's default tmux server. Captures never reach the browser, clipboard or speakers; fx always runs with `FX_AUTO_UPGRADE=0`. Kill by PID only. Research captures that name local paths stay off pushed branches until sanitized.
