# xfx — Upstream delta

xfx is pinned to `vercel-labs/fx@580a0c5d` (`UPSTREAM.md`). Upstream has since moved to `ef1d0d0`
(release 0.0.5), which `UPSTREAM.md` §"Upstream has moved since the pin" records as a fact about the
evidence base rather than as a backlog. This document is the read of that gap: what changed, what it
means for this port, and what to port first. It does **not** move the pin — `docs/parity.md` still
cites `580a0c5d`, and advancing it means re-reading every citation at the new commit.

Condensed from [`research/upstream-delta.md`](research/upstream-delta.md), which was read against a
local clone at `ef1d0d0` on 2026-08-24. `[추정]` marks are that note's and are preserved.

**Scope caveat, carried forward:** the clone is `depth-1`, so it cannot be established by git where
the pin sits relative to the 0.0.4/0.0.5 boundary. The whole 0.0.5 section is treated as delta, plus
two named 0.0.2–0.0.4 lineages (`/permissions remember|revoke`, the auto-mode review layer). Rows
xfx's ledger already accounts for are marked "수렴" (converged).

## 1. Delta table

| # | Upstream behavior (evidence) | xfx status (`docs/parity.md`) | Relevance |
|---|---|---|---|
| B1 | **Command sandbox retired.** Approved captured/background/monitor commands run as ordinary host subprocesses; the sandbox setting, `status` field, and command are gone (CHANGELOG 0.0.5 Breaking; `config_runtime.zig:3011` "legacy sandbox keys are inert unknown data") | **수렴 — xfx arrived first.** "OS command sandbox — deferred"; `UPSTREAM.md` deviation #2 | **HIGH.** Upstream joined xfx at "report honestly that there is no sandbox". The remaining work is the ledger, not the code: deviation #2 is no longer a deviation |
| B2 | **Provider switching moved into `/setup`; `/provider` slash removed** (CHANGELOG Breaking; `auth_runtime.zig:493` "Switch provider" picker; README) | absent — `provider` deferred, `setup` implemented but llmux-only | **HIGH.** Settles the target surface for free: do not build `/provider`; the `/setup` picker is canonical. Top-level `fx provider` CLI survives |
| N1 | **Codex subscription login** `fx login codex` — OAuth, session file, catalog-valid model, `/fast` = priority tier (`src/core/auth/chatgpt_session.zig`) | absent — "Codex / ChatGPT subscription — deferred" | **HIGH.** Already a planned axis; the delta pins the acceptance criteria. See [`04-providers.md`](04-providers.md) |
| N2 | **Grok subscription login** `fx login grok` — xAI OAuth, effort levels, Responses API (`grok_session.zig`) | absent, same deferred group | **HIGH.** Same family; low marginal cost after N1, and the origin of effort levels as a first-class catalog concept |
| N3 | **Workspace status line** — opt-in via `/settings`, `/statusline workspace`, `statusLine.workspace`; path + git branch (`config_runtime.zig:58,529,2962-2967`) | absent — "notifications and status line — deferred" | **MED-HIGH.** Direct input to the TUI status row. Default-hidden-and-opt-in is part of the contract, not a detail |
| N4 | **Native workspace skills** — `.fx/skills` discovered ahead of compatibility roots; rank managed 0 < workspace/shared 1 < compat roots 2 (`skill_runtime.zig:911-928`) | absent — skills deferred | **MED.** When xfx gets skills the native root is `.xfx/skills`; port the ranking from the start |
| N5 | **External skill symlink authorities** — a colon-separated allowlist of absolute roots (`skill_runtime.zig:448-454`) | absent | **LOW.** Arrives with skills, not before |
| I1 | **Provider setup polish** — catalog-valid model activated after login, re-auth a logged-out provider from `/setup`, clickable auth link | absent (N1/N2 group) | **MED.** Absorbed as N1/N2 acceptance criteria |
| I2 | **`/model` shows the catalog** — provider-advertised models, context windows, effort levels (`model_menu_presentation.zig:279-286`, `picker_presentation.zig:456,520`) | partial — `/model` implemented but "does not browse a catalog"; `models` deferred | **HIGH.** xfx already *reads* a catalog in `setup llmux`; only the interactive surface is missing. Best feel-per-line in the TUI epic |
| I3 | **Session list UX** — saved names, readable UTC timestamps, singular turn counts | partial — `sessions` implemented, but no session **name** concept (`/rename` deferred) `[추정: xfx's timestamp format was not compared]` | **MED.** Rendering plus a rename event; the store is already solid |
| I4 | **Session cache reads** stay responsive while another session defers publishing | 수렴 by another design — staged manifest + RAII, wait-not-refuse under contention | **LOW.** Not a 1:1 port; useful only as a concurrency acceptance criterion |
| I5 | **Terminal tab title** — session (or workspace) name + active model, tracked across rename/resume/model change, cleared on exit, never sent when non-interactive (`app_session_runtime.zig:2975-3026`) | absent — no ledger row; xfx emits no OSC at all beyond `/clear`'s erase pair | **MED-HIGH.** Tens of lines, high felt value, amplified under a multiplexer |
| I6 | **Terminal activity row** holds a command until completion; distinguishes graceful from forced close; hides a `cd . &&` prefix | absent — durable terminals deferred | **MED.** The UX contract to honour when durable terminals land |
| I7 | **Terminal action arguments** — advertise only the fields the selected action uses; unsaved `ask` sessions get `terminal.exec` only | 수렴 — xfx is always `exec`-only | **LOW.** Remember the schema-narrowing pattern when adding actions |
| I8 | **Auto-mode reads** — routine read-only commands and hardened git inspection run directly, without a review (`command_effect.zig:1297`) | 수렴, more narrowly — xfx has no reviewer at all, and hardens git the same way | **HIGH.** See §3: the two-layer structure is the contract |
| I9 | **Automatic-denial recovery** — destructive actions return to the agent for replanning; repeated no-progress denials end the turn as ordinary assistant output instead of a prompt (`tool_admission.zig:4938,4984,5023`) | absent — no automatic review, so the whole state machine is missing | **HIGH.** See §3. Inseparable from I8 |
| I10 | **One-off subagents** — visible while active, one final delivery, retired after; persistent ones are reusable (`subagent/domain.zig:26,763,1877-1889`) | absent — subagents deferred | **MED.** Port the dual lifecycle from the start, whenever that start is |
| I11 | **Startup preferences** — saved effort and Fast mode shown while capabilities load | absent | **LOW.** Only a problem once capability loading is async |
| I12 | **Dev build identity** — commit + `[dev]` marker in the welcome header (`render.zig:173-184,923`) | partial-수렴 — xfx reports `build_channel` + a 12-char revision already; it has no welcome header | **LOW.** One line when the TUI header exists |
| I13 | MCP reload feedback, help layout, binary size | absent / n.a. | **LOW.** |
| I14 | **Stable upgrades + Ctrl+G** — forward-only version ordering; Ctrl+G applies a prepared upgrade over any modal (`app_input_runtime.zig:77,1148-1165`) | absent, deliberately — `upgrade` deferred, `UPSTREAM.md` deviation #3 | **LOW.** Meaningless until xfx has an updater |
| F1 | **Refuse non-regular files in `read_file`** — a FIFO is rejected before it can block (CHANGELOG Bug Fixes) | **absent — a real gap candidate.** `src/tools/read.rs:841-847` refuses directories only (`metadata.is_dir()`); no FIFO guard `[추정: opening a FIFO through xfx's read tool can block]` | **HIGH, cheap.** See §4 |
| F2 | **Malformed tool loop** — end the turn after three consecutive malformed-only tool batches; reset after a valid one | absent `[추정: no "malformed" hit under src/agent]` | **MED.** Provider-independent loop robustness, local and cheap |
| F3 | **Credential fallback** — continue to a stored API key when login credentials cannot load or refresh; keep the failure as a diagnostic | absent (stored credentials themselves deferred) | **MED.** The ladder to honour when OAuth lands: OAuth → stored key → env |
| F4 | Oversized images, corrupt memory store, vision retry, thinking indicator, terminal helper compatibility, WASM, idle theme polling | absent — all bound to deferred surfaces | **LOW.** |
| S1 | **Command approval patterns** — wildcard allows restricted to static shell words; destructive shell commands and file deletion stay outside automatic review (CHANGELOG Security) | partial-수렴 — xfx has no glob grants at all (exact `tool`+`target` only), and destructive commands are excluded by the `auto` grammar | **HIGH.** See §3: the safety floor for widening rules |
| S2 | **macOS Keychain** for native login sessions, with migration/refresh/restart/logout verified | absent | **MED.** A storage decision for N1/N2 — and the evidence says Keychain does *not* cover the subscription sessions ([`04-providers.md`](04-providers.md)) |
| S3 | **Provider response limits** — reject oversized catalogs, streams, tool data, replay state, keeping later input | 수렴 as house style — xfx's decoders are already bounded per frame, per completion, and by tracked-block count | **MED.** Confirmation that new clients inherit the same discipline |
| S4 | MCP config atomic write, MCP session retirement, ACP permission validation | absent (MCP/ACP deferred) | **LOW.** |
| P1 | **`/permissions remember\|revoke`** (0.0.2+) — store an exact rule without executing, list by stable id, revoke regardless of workspace or file state (`session_commands.zig:24`; README) | partial — in-memory exact rules and durable session-scoped "always" grants exist; "configured rules are still not read from or written to settings"; both `/permissions` surfaces deferred | **HIGH.** See §3 |
| P2 | **`/feedback`** — opens the upstream feedback form | absent | **LOW. Do not port** — it is upstream's product form, not somewhere this port should point |
| P3 | **`/trace`** — a private Markdown diagnostic of logs, session context, runtime state, permissions and recent activity; clipboard copy on macOS (`commands.zig:443`) | absent | **MED.** Same culture as `doctor`; cheap as its interactive extension |

## 2. Porting priority (15)

Premise: the TUI port, the provider architecture, and this documentation set are already planned.
This is the ranked list of *absent* behaviors that converge xfx on fx's current experience.

**Items 1–4 follow the release order and its dependency argument in
[`04-providers.md`](04-providers.md) §"MVS order", which is the single place that reasoning is
written down.** In short: catalogs before OAuth, because a credential has nowhere to be selected until
the provider frame exists and nothing to be spent on until a catalog can name a model — and because
1–2 have no external gate while 3 does.

1. **Provider frame + `/setup` as the switching surface** (B2, I1) — and do not build `/provider`.
   Making llmux a peer picker entry folds xfx's own backend into upstream's UX for free. No network,
   no OAuth, and the precondition for everything below it.
2. **`/model` catalog browse with context window and effort** (I2) — the catalog source already
   exists in xfx; this is the surface users touch daily, and it still needs no OAuth.
3. **Codex OAuth login** (N1) — gated on the policy decision in
   [`04-providers.md`](04-providers.md) §"Policy risks"; with the delta's acceptance criteria:
   catalog-valid model on login, clickable authorization link, the F3 fallback ladder.
4. **Grok OAuth login** (N2) — low marginal cost after 3, and the source of the effort-level concept.
5. **Auto-mode automatic safety review** (I8, §3) — one narrow review per unresolved action; `clear`
   authorizes *that action only*; `caution`/`unavailable` hold without prompting.
6. **Automatic-denial recovery state machine** (I9, §3) — one body with 5: introducing a reviewer
   without the recovery contract produces stalls.
7. **`/permissions remember|revoke` — rules that outlive the process** (P1, §3) — half a step from
   what xfx has; stable rule ids and state-independent revoke are the core.
8. **Static-shell-word limit on wildcard allows + destructive commands excluded from review** (S1,
   §3) — the floor that must be designed *with* 7, not bolted on after.
9. **Terminal tab title via OSC** (I5) — tens of lines, disproportionate feel, especially under a
   multiplexer.
10. **Workspace status line** (N3) — part of the TUI status-row spec; default-hidden with three opt-in
    paths is the contract.
11. **Refuse non-regular files in `read_file`** (F1) — the one place this port may be *less* safe than
    upstream. A few lines of Rust; see §4.
12. **Session list UX + `/rename`** (I3) — names, readable UTC, turn counts. The store already holds.
13. **Malformed tool-loop cutoff** (F2) — end the turn after three consecutive malformed-only batches.
14. **`/trace` diagnostic** (P3) — `doctor`'s interactive extension; directly helps bug reports.
15. **One-off subagent lifecycle** (I10) — fx's *in-product* subagents, which stay deferred; port the
    dual model whenever they arrive. Not to be confused with the epic's external QA harness, which
    also drives agents but ships nothing into the binary ([`06-qa-harness.md`](06-qa-harness.md)).

Explicitly **next tier, not this cycle**: skills discovery ranking (N4) and symlink authorities (N5)
arrive as a set with skills; Keychain (S2) is absorbed into 2/3's storage decision; upgrades and
Ctrl+G (I14) stay out while xfx declares no updater; `/feedback` (P2) is not ported at all.

## 3. The permission-model shift

Upstream's current `auto` contract, accumulated across 0.0.2 → 0.0.5:

1. **Two layers.** The lower layer runs **without any review**: routine read-only commands, hardened
   git inspection (pinned argv and environment, `command_effect.zig:1297`), prepared workspace edits,
   and reversible routine development commands including new-file creation (0.0.4). The upper layer is
   **exactly one narrow automatic safety review per unresolved action**, whose input is the current
   user request plus the precise pending action, and whose `clear` result **authorizes only that
   action** (`tool_admission.zig:4608` "mints matching one-call authority").
2. **Three review outcomes.** `clear` executes. `caution` and `unavailable` **neither open a prompt
   nor end the turn**: the action is held and advice is returned to the agent
   (`tool_admission.zig:4938`; `:943` "an unavailable or invalid automatic review never executes
   anything"). An invalid review also returns to the agent before any prompt (`:4984`).
3. **Denial recovery (0.0.5).** Destructive actions go back to the agent for replanning; repeated
   no-progress denials end the turn as ordinary assistant output rather than an approval prompt. The
   invariant is that **`auto` never calls a human** — human approval belongs to `ask` and to the
   0.0.4 TTY-only prompt flag, which automatic review never opens.
4. **Persistent rules (0.0.2+).** `remember <allow|deny> <tool> <args-json>` stores an exact rule
   without executing it, lists by stable id, and revokes regardless of the original workspace or file
   state. A stored rule **skips the automatic review** (`tool_admission.zig:5059`).
5. **Safety floor (0.0.5).** Wildcard command allows only over static shell words; destructive shell
   commands and file deletion stay outside automatic review whatever the rules say.
6. **Sandbox retired (0.0.5).** After approval, execution is an ordinary host subprocess — the
   decision layer is now the only defense.

What this means for xfx's engine:

- **xfx today is the lower layer alone** (`docs/parity.md` "automatic command grammar — partial";
  `UPSTREAM.md` deviation #9). Adding a reviewer is therefore **not** widening the grammar; it is
  adding an upper layer above an unchanged reporting-only floor, so that actions outside it receive
  *one review* instead of a refusal.
- **The one-use authority already fits.** xfx mints an immutable one-use authority spent immediately
  after revalidation; upstream's "clear authorizes only that action / one-call authority" is the same
  object. A reviewer is a new *issuer* of that authority, not a change to its contract.
- **The turn-termination contract does change.** Today an xfx refusal returns as a tool result and the
  agent copes. Upstream specifies (a) hold + advice, and (b) ordinary tool-free output on repeated
  no-progress. Its interaction with xfx's bounded-turn guarantee — **does a review hold consume a
  step?** — must be decided in the spec `[추정: upstream's step accounting was not established by
  this research]`.
- **Rule persistence is half a step.** xfx already has exact `tool`+`target` rules, cwd-keyed command
  grants, and session-scoped durable "always". The gap is (i) persisting to and loading from settings,
  (ii) stable rule ids, (iii) state-independent revoke, (iv) glob grants. Opening (iv) makes S1's
  static-shell-word limit a precondition, not a follow-up.
- **The sandbox retirement inverts xfx's own justification.** The argument was "upstream has an OS
  sandbox and xfx does not, so xfx's `auto` is narrower". Upstream's approval layer is now also its
  only defense, so that asymmetry is gone. Whether to widen remains a judgment — but the *reason*
  recorded in `UPSTREAM.md` deviations #2 and #9 no longer holds and must be rewritten.

## 4. Flagged defect — `read_file` and non-regular files

Upstream fixed it in 0.0.5: a non-regular `read_file` target (a FIFO being the motivating case) is
refused **before** the read can block.

xfx's read tool refuses directories only — `src/tools/read.rs:841-847` tests `metadata.is_dir()` —
and a grep for a FIFO guard finds nothing. `[추정]` Pointing xfx's `read_file` at a FIFO can therefore
block the turn.

This is the one identified place where this port may be **less safe than the thing it ports**, and it
is a few lines of Rust. The fix is a `FileType::is_file()` gate at the same site, and the RED test is
a `mkfifo` in a temp workspace with no writer, asserting a named refusal rather than a hang — an
acceptance test that must fail when the guard is reverted.
