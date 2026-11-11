---
title: WIAP-PANE-PID-RACE -- a dead pane's exit is tmux's word when its pid is not
type: receipt
tags: [xfx, tui, phase5, fxcap, ci]
---

# WIAP-PANE-PID-RACE

Since PR #94 (WORKSPACE-INDEX-AT-PLAN), main's Sweep has failed
`LaunchWait.test_a_binary_gone_at_once_is_final_under_keep_and_fails_the_run_without`:
the run with `keep` and `/bin/echo` ended with exit 4 where 0 was expected.
Every claim below names the file in this folder it rests on.

## 1. The cause

- **CI's tmux is 3.8; this machine's is 3.7b.** The Sweep job installs tmux
  3.8 (`ci-failure.txt`, its `tmux -V` and the bottle it pours).
- **The two answer `#{pane_pid}` differently for a dead pane**
  (`tmux-source.txt`):
  - 3.7b, `format.c:2282-2287`: the pid, always;
  - 3.8, `format.c:2580-2585`: the pid only `if ... ft->wp->fd != -1`,
    which is nothing once the pane's pty is closed.
- **So on CI** `/bin/echo` was gone before `own_pane` looked
  (`up: the pane is dead after 0.02 s`). The look printed no pid, and
  `own_pane` (`scripts/fxcap/capture.sh:551@753b0a72`) recorded
  `unknown, the pane pid could not be read`. Cleanup then failed the run, as
  the guard's rule says it must for an unknown record (`ci-failure.txt`).
- **It is a race.** PR #94's own Sweep looked before the pty closed, and
  passed.
- **Here,** tmux 3.7b keeps the pid of a dead pane (`tmux-probe.txt`), so
  the local gate could not see it.

## 2. The fix (99421b1a, worded in fc5e4523; `own_pane`, `scripts/fxcap/capture.sh:551-586@fc5e4523`)

The guard's rule stands: a process not confirmed gone fails the run. What
changes is what counts as confirmed.

- `own_pane` asks tmux `#{pane_pid}:#{pane_dead_status}#{pane_dead_signal}`
  in one look.
- **A pid:** as before (`kill -0`, the start time).
- **No pid, and an exit status or signal:** `gone`. tmux sets those only
  once it has reaped that very process: `server_child_exited` sets
  `PANE_STATUSREADY` for the pane whose `wp->pid` exited, and
  `format_cb_pane_dead_status` prints nothing without it
  (`tmux-source.txt`, both at 3.8). A real run of the formats is in
  `tmux-probe.txt`: no status while the pane is alive, `0` once it is dead.
- **Neither:** the pty is closed but the process is not yet reaped, or it
  lives on with its pty closed. `own_pane` looks again: 20 looks, 0.05 s
  apart. That is 1 s of sleeps plus the looks' own time, not a wall-clock
  deadline; a look that hangs is caught by the sweep's per-run timeout
  (round 2). Then it records `unknown`, which fails the run as before.
- **A status cannot outlive its process** (round 2, `tmux-source.txt`):
  - a respawn closes the old pty and clears PANE_STATUSREADY (spawn.c:349
    at 3.8, :265 at 3.7b) before it forks the new process (:478, :391);
  - `server_child_exited` sets the status only for the pane's current
    `wp->pid` (server.c:498-500 at 3.8);
  - tmux runs one command at a time, so no look falls between the clearing
    and the fork. After a relaunch a look sees the new pid, never the old
    process's status. A failed fork shows neither, which reads `unknown`.
- The ledger's four `capture.sh` references are re-pointed.

## 3. RED and GREEN

The `LaunchWait` tests (`scripts/fxcap/test_capture.py:1917-1996@fc5e4523`)
use a `tmux` shim on capture.sh's PATH that makes this machine's tmux answer
as 3.8 does: no pid once the pane is dead. The shim waits for that before
answering, and logs every answer to a look at the pid. Since round 2 the
tests assert those answers.

| test | the shim's answers | 753b0a72 | fc5e4523 |
|---|---|---|---|
| a pane dead before its pid is read | `:0` | exit 4, CI's own line | exit 0, record `gone` |
| a pane not yet reaped | `:`, `:`, `:`, then 3.8's `:0` | exit 4 | exit 0, record `gone` |
| neither, ever | `:` on every look | exit 4 | exit 4 |
| a relaunch after a dead pane (pin) | `:0`, then the new pid with no status | -- | exit 0, record the new pid |

The third test pins the safety rule, which held before as well.
`red.txt` has the run at 753b0a72.

Round 4's `test_a_pane_pid_that_cannot_be_read_fails_the_run` used a shim
that matched only the bare `#{pane_pid}` argument. It now matches any look
that asks for the pid (27d28fef), and its intent is unchanged.

## 4. Mutants

Round 1: 4 / 4 killed (`mutants.sh`, `mutants.txt`), on a `git clone
--no-local` at 99421b1a. Round 2: the same four against the new tests, and
one more, the record after a relaunch dropped, at fc5e4523
(`mutants2.sh`, `mutants2.txt`). The four are:
- a reaped pane read as unknown;
- no second look;
- no exit read as gone;
- the pid asked alone.

## 5. Gate

`scripts/fxcap/test-sweep.sh` and `scripts/fxcap/test-capture.sh`, under the
suite lock, on clones (`gate.txt`):
- **At 99421b1a:** test-sweep 79 + 28 OK. test-capture had 1 failure in 133:
  round 4's shim, which 27d28fef corrects.
- **At 27d28fef:** test-sweep 79 + 28 OK, test-capture 133 OK.
- **At fc5e4523 (round 2):** test-sweep 79 + 28 OK, test-capture 134 OK.

No other script changed.

## 6. Open

- The real tmux 3.8 path runs only on CI. This machine has 3.7b, and the
  user's tmux is not changed for a test. The shims stand in for 3.8 here;
  the next CI Sweep is the run on 3.8 itself.
