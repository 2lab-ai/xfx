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

## 2. The fix (99421b1a; `own_pane`, `scripts/fxcap/capture.sh:551-582`)

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
  lives on with its pty closed. `own_pane` looks again, 20 times 0.05 s,
  then records `unknown`, which fails the run as before.
- The ledger's four `capture.sh` references are re-pointed.

## 3. RED and GREEN

Three `LaunchWait` tests (`scripts/fxcap/test_capture.py:1917-1969`) use a
`tmux` shim on capture.sh's PATH to make this machine's tmux answer as 3.8
does, at a moment the test chooses:

| test | the shim | 753b0a72 | 99421b1a |
|---|---|---|---|
| a pane dead before its pid is read | waits for `#{pane_dead}`, then blanks the pid | exit 4, CI's own line | exit 0 |
| a pane not yet reaped | blanks the pid and the status for three looks | exit 4 | exit 0 |
| neither, ever | blanks both for every look | exit 4 | exit 4 |

The third test pins the safety rule, which held before as well.
`red.txt` has the run at 753b0a72.

Round 4's `test_a_pane_pid_that_cannot_be_read_fails_the_run` used a shim
that matched only the bare `#{pane_pid}` argument. It now matches any look
that asks for the pid (27d28fef), and its intent is unchanged.

## 4. Mutants

4 / 4 killed (`mutants.sh`, `mutants.txt`), on a `git clone --no-local` at
99421b1a:
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

No other script changed.

## 6. Open

- The real tmux 3.8 path runs only on CI. This machine has 3.7b, and the
  user's tmux is not changed for a test. The shims stand in for 3.8 here;
  the next CI Sweep is the run on 3.8 itself.
