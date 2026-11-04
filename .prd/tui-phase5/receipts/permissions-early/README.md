---
title: PERM-EARLY receipt -- /permissions remember and revoke before a session's first prompt
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, permissions, session]
---

# PERM-EARLY receipt: rules confirmed before the first prompt

Found by P-permissions (`feat/p5-permissions-panel`,
`receipts/permissions-panel/README.md`, "Open: a rule before the first
prompt"): fx keeps a `/permissions remember` typed before the first prompt,
xfx refused it (`✗ permissions: saved-session permission rules require an
active saved session`), and nine pairs of `p5-panels-permissions` waited on
that (`saved-rules-before-first-prompt`, `-band`, `-shift`).

Base: `feat/p5-session-early-prefs` at `daff4834` (SESS-EARLY), with
`feat/p5-permissions-panel` at `6d495f1b` merged in (`9c6f4a1d`): the unit
needs SESS-EARLY's first save and P-permissions' panel, whose `Saved rules`
row it fills. Code `11ea2120`; docs, ledger and scenario `984894f3`.
P-permissions' final head `b60c673b` is merged at `17e80d33`; `699acf13`
puts the seed under its fail-closed reads (below).

## What fx does (`vercel-labs/fx@481104fd`)

- Its session is open from launch, so `remember` and `revoke` are taken
  before the first prompt (`handlePermissionRuleManagement`,
  `src/core/app/app_commands.zig:598-750`; refused only with no session at
  all, `:611-618`).
- **In force at once.** A confirmed change is committed, then put in the
  runtime's state (`commitPermissionState` then `replacePermissionStateOwned`,
  `src/core/app/input_approval_runtime.zig:277-293`), which every approval
  reads. Before the first turn the commit is a `set permissions` line held
  in memory (`setPermissions`, `src/core/session/session_adapter.zig:2205-2209`;
  `hold`, `src/core/session_manager/session.zig:415-429`), so the rule
  decides from the moment it is confirmed, before anything is on disk.
- **Written with the first save.** The first turn writes line 1, every held
  line in order (each `permissions` line is the whole state), then the turn,
  in one staged publish that makes the session visible (`writeFirstTurn`,
  `stage`, `session.zig:503-539`).
- **A failed first save runs nothing.** The publish failing fails the turn
  before any call runs: the running calls are appended first and the append
  is `try` (`appendRunningToolCalls`,
  `src/core/agent/runtime/orchestrator.zig:3894-3917`). The rule stays in
  memory; no tool can act under it unsaved.
- **`/new` and a resume drop what was held** (`SessionRuntime.reset` ->
  `clearPermissionState`, `src/core/session/session.zig:1430-1437`, from
  `applyIdleLiveSessionTransition`, `src/core/app/app_session_runtime.zig:1816`;
  `prepareLiveSessionResume`, `:1744-1746`). A launch that never prompts
  leaves nothing (D2).

## The fix

`tui::permissions::SavedRules` replaces the published path. It holds the
rules in memory for the session the next save opens, keyed by that
session's future `permissions.json` (the id `/status` names), or names the
open session's file. The shell's `remember`, `revoke`, the confirmation and
the bare panel all go through it, so a held rule is listed by the panel's
`named` + `Row::item` rows like a saved one. `Runtime::open_next` takes the
held state under the store's lock and writes it into the new session's
directory **before its log exists** (`SessionStore::create_with`,
`session_rules::seed`), then the file is the store. A confirmation that lands
during the open waits on the lock and goes to the file; generations carry
over, so its compare-and-swap holds.

Decisions (reversible; `docs/parity.md:94`):

1. **Failed first save: fx's choice, nothing runs.** Rules that cannot be
   written open no session; the prompt says `the saved-session permission
   rules cannot be written: <reason>`, runs no tool, and the rules stay held
   for the next attempt under the next id. xfx never lets a turn run with a
   held rule out of its store.
2. **A store that refused the last session** leaves held rules held and still
   managed (fx refuses with no session at all): xfx asks the store again at
   the next prompt, and a revoke must not be refused before that.
3. **`/new` forgets at the keystroke**, on the UI thread, as the title
   lineage moves there; a resume replaces what is held.
4. **What is written**: nothing if nothing was ever held; else the whole
   state, so a revoked id is never handed out again.
5. **A confirmation for a store this shell no longer has** (left by `/new`
   or a resume) is made nowhere: `! permissions: saved-session permission
   rule changed before confirmation`.
6. **The line shell still refuses** before its first prompt: a row in the
   Divergences of `docs/parity.md:94`.
7. **The seed fails closed as `load` does** (after P-permissions-c, which
   opens the store `O_NONBLOCK|O_NOCTTY` and refuses a non-regular file):
   anything already at the new session's `permissions.json` -- a FIFO, a
   directory, a file -- is refused, left as it was and never waited on, and
   what was written must read back through `load` as the state held; else no
   session opens and the rules stay held
   (`a_seed_refuses_whatever_is_already_at_the_store_and_reads_back_what_it_wrote`).
   The panel's facts keep P-permissions-c's rule that an `Unknown` stamp is
   read again (`StoreStamp::is_unknown`).

## RED

- Unit (`red-unit.txt`): the three shell tests on `daff4834`'s tree fail at
  the first assertion, the refusal in the document. The three worker tests
  need the new store to build; with the write taken out they fail ("the held
  deny let the first turn's write by").
- Pty (`red-pty.txt`): `tests/tui.rs`
  `a_rule_remembered_before_the_first_prompt_decides_that_prompts_call_without_a_question`
  on `daff4834` times out waiting for the confirmation; the screen says the
  refusal.
- Captures (`red.txt`, sides in `red/`, xfx at `9c6f4a1d`): the nine waiting
  pairs and every pair of `p5-permissions-early` but `new-forgets` differ.

## GREEN (`green.txt`, `pairs.py`, the v0.0.16 pin)

Every xfx side is `699acf13`'s (banner `699acf1`), the merged head; the
verdicts are those `11ea2120`'s sides had.

The nine pairs, row counts RED -> GREEN (`red.txt`):

| pair | verdict |
|---|---|
| `permissions-{remember-prompt,remember-done,saved,after-grant}` | normalized-identical |
| `permissions-{revoke-prompt,revoke-done}` | identical |
| `permissions-status-grants` | approved-divergence-cascade: P-status's Sandbox, Updates, Step limit |
| `permissions-{grants,after-revoke}` | differs, one row: the command grant's item (`tool-schema`) |

The rest of `p5-panels-permissions` is as P-permissions left it; `approval`
differs only by `tool-schema` and `approval-grant-scope`.

`p5-panels-permissions-saved` (a rule remembered after the first prompt, the
open store's path this unit rerouted) is as P-permissions left it:
`remember-prompt`, `remember-done`, `saved-esc`, `revoke-prompt`,
`after-grant` normalized-identical; `saved` and `after-revoke` one row each
(`tool-schema`); `approval` `tool-schema` and `approval-grant-scope`. Its xfx
side, like the rest, is `699acf13`'s.

`p5-permissions-early` (new): two rules, a revoke, the panel, the first
prompt's read decided by the held deny, `/new`. All eleven pairs
normalized-identical. `home before-prompt` holds no session on either side;
`home first-save` holds fx's held lines (seq 3 the seed's empty `set permissions`,
seq 5-7 the two remembers and the revoke, seq 8 `turn_started`) and xfx's
`permissions.json` (`next_generation` 4, rule 2 `deny read_file a.txt`).

## What fx does not do: a remembered file change

fx v0.0.16 confirms `remember deny write_file …` and then asks about that
very write (in `auto`, makes it): its TUI admits file changes through
`requestPreparedFileMutationPermissionOutcome`
(`src/core/app/app_agent_runtime.zig:684-692`), whose input never resolves
the session-state provider that only `requestPermissionOutcome` reads
(`src/core/tooling/tool_admission.zig:1278-1292`), so no saved rule decides a
write (`state_may_decide`, `:510-513`). The same happens after the first
prompt, so it is not this unit's lifecycle. xfx applies it: the
`unsafe-to-copy` divergence `saved-write-rule-applied`, over the probe
`p5-permissions-early-probe-ask-after` (`approved-divergence`). The capture's
held deny is therefore a read's, which fx applies.

## Older binaries read the session (`compat.txt`, `compat.sh`)

The session `p5-permissions-early` left, copied from the capture's HOME:
`permissions.json` was made before `events.jsonl` (birth times), in the
shape every reader since Phase 4 reads. The log is unchanged in kind.

- the preview, built from `git archive 14ad0420`, stamped
  `build_channel=preview`, `build_revision=14ad04202fe5`: `session --id`
  exit 0 with log, manifest and `permissions.json` unchanged; `sessions`
  lists it; `ask --resume-id` exit 0, its frames appended behind this build's
  bytes. It predates saved-session rules and neither reads nor writes the
  file.
- `9c6f4a1d`, the branch's merge parent (`daff4834` with P-permissions,
  without this change): `session --id` exit 0; `ask --resume-id` whose model
  calls `read_file a.txt` is refused by the rule this build wrote before the
  first prompt (`read_file was not permitted: a saved-session permission rule
  denies`).
- this build: `session --id` exit 0 on what both appended.

## Captures

```
FX_BIN=<fx v0.0.16> TMPDIR=/private/tmp/claude-501 scripts/fxcap/capture.sh fx  <scenario> 40x120 <dir>
XFX_BIN=<target/debug/xfx> TMPDIR=/private/tmp/claude-501 scripts/fxcap/capture.sh xfx <scenario> 40x120 <dir>
python3 -I .prd/tui-phase5/receipts/status-panel/pairs.py .prd/tui-phase5/receipts/permissions-early
```

xfx is `target/debug/xfx` at `699acf13` (banner `699acf1`); `red/` is
`9c6f4a1d`'s. `compat.txt` was run on `11ea2120`'s session, whose files the
seed's checks do not change. `/private/tmp/claude-501` is written `<tmp>`
(`scripts/fxcap/redact.py`).

`receipts/permissions-panel/` still holds the refusing xfx side of the nine
pairs; with its three causes retired, `pairs.py` on that directory shows them
unattributed. This directory has the current ones. No gate runs `pairs.py` on
receipt directories.
