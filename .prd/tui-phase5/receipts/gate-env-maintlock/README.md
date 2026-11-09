---
title: GATE-ENV-MAINTLOCK-F receipt -- the decoy's own commit left git maintenance running in it
type: receipt
tags: [xfx, phase5, ci, gates, git, flake, receipt]
---

# GATE-ENV-MAINTLOCK-F: auto-maintenance in test-gate-env.sh's decoy

Every claim below is from a file in this folder, cited as `file` or
`file: section`. A claim with no file here is marked *(not measured here)*
with its source.

- **Base:** origin/main `32afd307`. Branch `fix/p5-gate-env-maintlock`:
  the hold case at `4cac717d`, the fix at `d98ada7e`, the trace check at
  `2bc8fa55`, this receipt on top. Base line numbers (`:N` base) are at
  `32afd307`; head line numbers (`:N` head) are at `2bc8fa55`.
- **Machine:** macOS, git 2.56.0 (Homebrew), GNU `timeout`. `ulimit -v` and
  `-d` are refused by macOS, so every run is bounded by `timeout -k 10 900`
  and by the cases' own 10 s waits (`batch.sh`).
- **Scope:** `scripts/test-gate-env.sh` only. `check-xfx-identity.sh` and
  every other gate are unchanged.

## The failure *(not measured here)*

The dispatcher's gate on PASTE-GAP `f9328d4e` (2026-11-09) failed
`scripts/test-gate-env.sh` once: `identity-own-miscased: the gate wrote into
the decoy repository: ... ./objects/maintenance.lock`. A rerun of that step
on the same head passed 62 of 62. Source: this unit's brief.

## The cause

**The writer is the test's own setup, not the gate.**

1. `identity-own-miscased` (`:628` base) runs `check-xfx-identity.sh` on
   `$work/ILOCK`, the `ilock` fixture, with no `GIT_*`. That gate is never
   told where the decoy is.
2. Every `refuses` case (`:359-388` base) first calls `new_decoy`
   (`:307-324` base), whose last step is `sg ... commit -q -m decoy`
   (`:322-323` base). Then it takes the decoy's sums (`:363`), runs the gate
   (`:364`), takes the sums again (`:365`), and reports any difference as
   "the gate wrote into the decoy repository" (`:379`). `build_case`
   (`:747-752` base) and the nested `test-check-no-secrets.sh` run
   (`:807-812` base) do the same.
3. `git commit` starts `git maintenance run --auto --quiet --detach`
   (`calls-probe.log`: `commit -q -m <msg>`). On git 2.56.0 that detached run
   still holds `objects/maintenance.lock` after the commit returns, with
   nothing forced:
   - in 46 of 50 plain decoys (`lock-probe.log`: `mode=plain`);
   - in 61 of 100 at load 37, for a median 0.38 ms past the commit's return,
     p90 0.76 ms, max 8.53 ms (`lifetime-probe.log`).

   So the sums before a gate can catch the lock and the sums after it miss
   it. That is the reported diff line.
4. **Why it is new.** Since git 2.55, maintenance that goes to the
   background keeps its lock (`git-docs.txt`: `RelNotes/2.55.0.adoc:327-330`).
   git's own `t7900-maintenance.sh` was deflaked in 2.56 for "flaky races
   with concurrent background maintenance jobs" (`RelNotes/2.56.0.adoc:810-814`).
5. **Why it is rare.** The window is under 9 ms here. 300 plain decoys,
   with sums 0.2 s apart, showed no difference (`natural-probe-base.log`).
   The sums land inside the window only on a loaded machine, like the
   dispatcher's gate with other suites beside it.

**Which git calls can start it.** `calls-probe.log` runs every subcommand
the decoy cases use once, under `GIT_TRACE`, in a repository whose own config
asks for maintenance at every chance. Only `commit` starts any.

| where (base `32afd307`) | call | starts maintenance |
|---|---|---|
| `test-gate-env.sh:322-323` (`new_decoy`) | `sg commit` | **yes**, in the decoy |
| `test-gate-env.sh:618`, `:636` (ilock), `:733` (checkout), `:786` (linked) | `sg commit` | **yes**, in those repositories |
| `test-gate-env.sh`: `init` `:252,261,271,311,567,580,589,604,639,730`; `add` `:253,262,272,276,278,321,552,568,581,590,605,732,785`; `config` `:449,451,459,461,468,472,497`; `hash-object` `:284`; `mktree` `:285`; `update-index` `:511,521,522`; `worktree add` `:619,783`; `submodule add` `:640`; `rev-parse` `:735,746` | everything but `commit` | no |
| `check-xfx-identity.sh:175`, `:188`, `:218` (`scratch_git`) | `init`, `add -A -f` | no |
| `check-xfx-identity.sh:117`, `:130`, `:144`, `:248`, `:253`, `:274`, `:316` (`scan_git`, `git --git-dir`) | `grep`, `ls-files`, `rev-parse`, `diff-files` | no |

So no gate needs a change: none of them commits in the decoy.
`check-preview-contract.sh` commits and fetches in scratch repositories of
its own (`:1061-1099`); maintenance started there writes only there, and no
case takes sums of those repositories.

## The fix (`d98ada7e`)

`sg`, the git of the test's own scratch repositories (`:103-106` head), now
passes `-c maintenance.auto=false -c gc.auto=0` on every call. A `-c` on the
command line outranks the repository's own config, and `sg` already reads
no global or system config. So the guard holds whatever a repository's config
says, and it covers `new_decoy`'s commit and the four other commits above.

- **`maintenance.auto=false` is the flag that holds** (`git-docs.txt`:
  git-config `maintenance.auto`). When the repository's own config sets
  `maintenance.auto=true`, `-c gc.auto=0` alone still lets the commit start
  maintenance (`flag-probe.log`: `gc-hook gc.auto=0 starts=1`). That run then
  does its foreground work: 2 s waiting on a held ref lock
  (`reflog-probe.log`: `gc.auto=0`).
- **`gc.auto=0` is kept because the brief prescribes it.** It also stops a
  `gc --auto` (`git-docs.txt`: git-config `gc.auto`), which is what a commit
  ran on a git from before `git maintenance` (2.29,
  `RelNotes/2.29.0.adoc:83-85`). On git 2.56 it does nothing that
  `maintenance.auto=false` does not already do: see mutant 3.

## The cases

Both live in the section "the decoy's own setup" (`:419-482` head). Both
build the decoy with `decoy_forced=1`, which makes `new_decoy`
(`:324-333` head) give the decoy's own config:
- `maintenance.auto=true`;
- the gc task (`maintenance.strategy=gc`; the default since 2.54 is
  geometric, `RelNotes/2.54.0.adoc:58`);
- `gc.auto=1`, with two names in `objects/17`, which is all that git's
  estimate of the loose objects reads;
- `core.hooksPath` naming a `pre-auto-gc` hook outside the decoy.

**`decoy-setup-maintenance`** (`:463` head) reproduces the reported failure
without a race. The hook's first call is made before the run detaches, and
lets gc go on. Its second call, from the background `git gc --auto`, waits
for the case's go and then exits 1, so gc stops before it reads the names as
objects (`git-docs.txt`: githooks `pre-auto-gc`). The sums before are taken
while that call waits. `after_maintenance` (`:453-461` head) then gives the
go, waits for the lock to go, and runs the gate (`check-xfx-identity.sh` on
`ilock`). Both waits stop at 10 s. `hook-probe2.log` shows the mechanism
alone: two hook calls, the second under `git gc --auto` after the commit
returned, and the lock held until the go.

**`decoy-setup-starts-nothing`** (`:464-481` head) is added because the
first case holds a run only through gc, and `-c gc.auto=0` alone turns gc
off. It runs the same forced `new_decoy` under `GIT_TRACE` and requires that
the trace names no `git maintenance run` and no `git gc --auto`. Timing does
not enter into it.

## RED, GREEN and mutants

All runs are the whole `scripts/test-gate-env.sh`, one at a time, inside one
machine-wide suite-lock slot per batch (`batch.sh`, `batch2.sh`; as run,
except that `TMPDIR` was exported there and is taken from the caller here).
RED and the mutants ran in a `git clone --no-local` throwaway, never in the
worktree. Each line below is copied from `runs-r1/summary.txt` or
`runs-r2/summary.txt`, and each result from that run's log.

**RED** (`runs-r1/red-4cac717d.log`): the test at `4cac717d`, with `sg` as on
the base. Exit 1, `1 problem(s), 62 case(s) passed`, the one problem being:

```
test-gate-env: decoy-setup-maintenance: the gate wrote into the decoy repository:
22d21
< 4294967295 0 ./objects/maintenance.lock
```

That is the dispatcher's failure line, against a gate that was never given
the decoy. The trace check came after the fix. Its RED is mutants 1 and 2
below, which are the head with the fix taken out, and `trunc-runs.txt`:
the head's test cut after the trace check (`:482` head), with `sg` as at
`base`, which shows both cases failing, and as in mutant 2, which shows
`decoy-setup-starts-nothing` failing alone.

**GREEN** (`runs-r2/green-{1,2,3}.log`), the worktree at `2bc8fa55`: three
runs, each exit 0 and `test-gate-env: ok (64 cases)`, the base's 62 cases
and the two new ones. No line but `ok`, `skip` and the grep banner, in any
of them.

| run | head | exit | seconds | result |
|---|---|---|---|---|
| green-1 | `2bc8fa55` | 0 | 384 | `ok (64 cases)` |
| green-2 | `2bc8fa55` | 0 | 370 | `ok (64 cases)` |
| green-3 | `2bc8fa55` | 0 | 384 | `ok (64 cases)` |

**Mutants** of the one changed call, `sg` (`:105` head), on the clone at
`2bc8fa55` (`runs-r2/mutant-*.diff` and `.log`):

| mutant | drops | exit | result |
|---|---|---|---|
| 1 | `-c maintenance.auto=false -c gc.auto=0` (the whole guard) | 1 | **red**: both new cases, `2 problem(s), 62 case(s) passed` |
| 2 | `-c maintenance.auto=false` | 1 | **red**: `decoy-setup-starts-nothing` (`run_command: git maintenance run --auto --quiet --detach`), `1 problem(s), 63 case(s) passed` |
| 3 | `-c gc.auto=0` | 0 | **survives**, `ok (64 cases)` |

- Mutant 3 survives because on git 2.56 `maintenance.auto=false` already
  keeps the commit from starting anything (`flag-probe.log`: both forcings,
  `maintenance.auto=false` → `starts=0`). `gc.auto=0` is kept only for the
  reason under "The fix". It is not load-bearing on this machine's git, and
  no case here can make it so.
- Round 1 (`runs-r1/`) ran the same mutants against `d98ada7e`, before the
  trace check. Mutant 1 was red there in the same way as the RED.
  **Mutant 2 survived there** (exit 0, `runs-r1/summary.txt`). That is what
  added `decoy-setup-starts-nothing`. Round 1 was stopped during mutant 3,
  so it has no mutant 3 result.

## Gates on the head

`gates.txt`: `check-xfx-identity.sh`, `check-no-secrets.sh`, `bash -n` under
`/bin/bash` 3.2 and under the PATH's bash, `shellcheck -S warning`, and the
local-path grep over `git diff 32afd307..HEAD`. The tree has no
`check-no-local-paths.sh` at this base.

## Limits

- `decoy-setup-maintenance` is red on the base only with a git that keeps
  the lock in the background, 2.55 or later. With an older git *(not
  measured here)*, the base's commit still starts a run, so
  `decoy-setup-starts-nothing` should still fail. On the fixed code, neither
  case starts anything on any git, by design.
- Nothing in the test proves, on every run, that the forcing still forces:
  on the fixed code no maintenance starts. Mutants 1 and 2 are that proof
  for this receipt, on git 2.56.0.
- The natural race was not reproduced (`natural-probe-base.log`: 0 of 300).
  The cause rests on the lock outliving the commit (`lock-probe.log`,
  `lifetime-probe.log`) and on the forced reproduction above.
