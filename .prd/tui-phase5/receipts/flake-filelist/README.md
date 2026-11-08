# flake-filelist: the `@` file list was given 5 s, and its worker starts `git`

The epic-1 preview build for `xfx-macos-x86_64` (GitHub-hosted
`macos-15-intel`, run 42260843024, 2026-11-06 17:40Z) failed `cargo test`
with 3860 passed and 2 failed:

- `tui::shell::tests::a_choice_in_the_middle_of_a_line_reuses_the_space_after_it`
- `tui::shell::tests::a_directory_choice_keeps_the_list_up_on_that_directory`

Both panicked at `src/tui/shell.rs:31539:9`, `the file list never arrived`.
The cause is in the test. The fix changes `src/tui/shell.rs`'s test module
only. Line numbers are at the base, `main` @ `0b16d7e5`, unless they say
otherwise.

## The path the cases wait on

An `@` in the composer opens the picker, and a picker opening is an episode
(`src/tui/shell.rs:8222-8224`, `self.files.begin_episode()`). The episode
starts a walk unless one is running (`src/tui/file_index.rs:943-948`):

1. `start_walk` (`file_index.rs:970-989`) spawns the thread `xfx-file-index`
   (`:977`), which runs `load` (`:857`).
2. `load` calls `discover_roots` (`:810`), and that calls `discover`
   (`:211`). With a trusted git (`trusted_git`, `:197-202`: `/usr/bin/git`
   first, `:77`) its first act is `ls_files` (`:216-221`).
3. `ls_files` (`:403-432`) starts `git -c core.fsmonitor=false
   --no-optional-locks ls-files -z --cached --others --exclude-standard`
   through `crate::process::command` (`src/process.rs:67-75`). That
   command's signal reset is a `pre_exec`, so the start forks. It then reads
   git's output to its end (`:419-422`) and waits for git (`:423`).
4. The cases' workspace (`file_index::tests::seeded`, `:1175-1188`) is a
   temporary directory and not a work tree. git says no, no `.git` is above
   it (`:225`, `has_git_metadata`), and the walk lists it (`:278-297`).
5. `load` publishes the entries and `Ready` (`:868-882`). The case's
   `settle_band` polls them in (`shell.rs:9533` `settle_files`, `:7837-7841`,
   then `Files::poll`, `file_index.rs:993-1041`), and the picker stops
   showing `Loading`.

`listed()` (`shell.rs:31526-31540`) turns the loop until step 5, for at most
5 s (`:31527`), and then panics (`:31539`). So the deadline covers one
`git` start and run, and the thread start before it.

## How long that takes

Measured on this machine (18 cores, load 4-12), by an uncommitted timer in
`listed()` and around the start in `ls_files`:

| where | `listed()` | the `git` start (`spawn()` returning) |
|---|---|---|
| the two cases alone, 3 runs | 11-18 ms (6 waits) | 1-12 ms |
| the whole lib suite (17.6 s) | median 23.4 ms, at most 29.1 ms (15 waits) | median 3.2 ms, at most 10.2 ms (20 starts) |

The two cases took 25.5 ms and 29.1 ms under the whole suite. The CI run
took them past 5 s, more than 170 times as long.

## What the CI log says about the runner at that moment

From the run's log (`cargo test --locked --all-targets`, the lib suite of
3869 tests starting at 17:39:55):

- The `tui::file_index` cases around `a_work_tree_is_listed_by_git_and_its_ignore_rules_hold`
  finished at 17:40:32.41-17:40:32.57 (log lines 2526-2545). That case
  finished at 17:40:49.18 (line 3488). It has no deadline. It starts git four
  times (`git init` and `git add`, `file_index.rs:1367` and `:1379-1385`,
  then the two `ls_files` of `discover`, `:217` and `:252`) and otherwise
  writes seven small files.
  [추정] libtest starts its cases in name order and the log shows only
  their ends, so this case started at about 17:40:32 and took about 17 s.
- The two failures were reported at 17:40:43.81 and 17:40:44.11 (lines 3412
  and 3415). Each failed on its deadline, so its wait began about 5 s
  earlier [추정], inside that 17 s.
- The other ten cases whose first `listed()` waits on git (the RED below
  names all twelve) finished from 17:40:49.18 to 17:40:55.16 (lines
  3494-3865), and all passed.

So for about 17 s around 17:40:32-17:40:49, starts of `git` on that runner
took seconds, and both cases that waited 5 s inside that window failed.

**Why the starts were slow is not known.** [가설] The trusted git is
`/usr/bin/git`, the `xcrun` shim, which finds the developer tools on its
first runs and caches the answer; a cold cache or a loaded runner would make
those runs slow. This machine cannot show it: the cache is in
`DARWIN_USER_TEMP_DIR`, not `TMPDIR`, and is shared with the other workers
here, so it was not cleared.

**The other cause considered.** PROC-SPAWN (PR #60) found a start that never
returns: macOS makes a pipe and only then marks it close-on-exec, and a
`fork` on another thread in between hands the pipe to a child that keeps it
(its receipt, `.prd/tui-phase5/receipts/spawn-lock/README.md` on
`fix/p5-spawn-lock`). Such a start waits until that other child exits. One
leaked pipe holds one start. The log shows three slow cases at once in one
17 s window: the work-tree case, whose four starts run one after another,
and the two failures. Three separate leaks in that window are less likely
than one slow window. The fix below holds for both causes for as long as the
start is shorter than 20 s; #60 closes the second one.

## RED

`delay.patch` (here, never committed) makes `load` sleep 8 s before
discovery when it has a git. That is the CI condition: the worker's first
step, the `git` start, takes longer than 5 s. Only the worker's git path
pays it: `file_index`'s own cases set `git = None` or list a directory, and
the work-tree case calls `discover` without `load`.

On the base, `0b16d7e5` with `delay.patch`:

```
TMPDIR=<tmp>/flake-filelist timeout -k 10 600 \
  cargo test --locked --lib -- --exact \
  tui::shell::tests::a_choice_in_the_middle_of_a_line_reuses_the_space_after_it \
  tui::shell::tests::a_directory_choice_keeps_the_list_up_on_that_directory
```

```
thread 'tui::shell::tests::a_directory_choice_keeps_the_list_up_on_that_directory' (…) panicked at src/tui/shell.rs:31539:9:
the file list never arrived
thread 'tui::shell::tests::a_choice_in_the_middle_of_a_line_reuses_the_space_after_it' (…) panicked at src/tui/shell.rs:31539:9:
the file list never arrived
test result: FAILED. 0 passed; 2 failed; … finished in 5.01s
```

That is the CI failure, at the same line, with the same words. `cargo test
--lib tui::` with the patch fails 12 cases, every one on that panic: the
two above, and `a_page_key_moves_the_list_a_page_and_wraps_past_its_end`,
`a_query_narrows_the_list_and_bolds_what_it_matched`,
`a_question_dismisses_the_file_list`,
`a_space_typed_right_after_a_completion_is_the_one_it_added`,
`an_at_opens_the_file_list_under_the_composer_and_leaves_the_hint_row`,
`control_j_and_k_walk_the_list_and_alt_enter_is_still_a_newline`,
`an_unmatched_mention_is_sent_by_enter`,
`escape_closes_the_list_until_the_caret_leaves_the_word`,
`enter_puts_the_path_in_and_only_a_second_enter_sends_it` and
`one_undo_takes_a_completion_back_to_what_was_typed` (2490 passed). The
other `listed()` cases drive the index by hand and do not wait on git.

## Fix

`PATIENCE` (`shell.rs:14801-14809` at the head) is 20 s, the integration
suites' `WAIT` (`tests/support/pty.rs:31`). There was no constant of that
name; `WAIT` is what "the other suites" use.

| wait (head line) | was | now | waits on |
|---|---|---|---|
| `listed()`, `shell.rs:31538` | 5 s | `PATIENCE` | the index's worker, which starts `git` |
| `/usage` read started, `shell.rs:37500` | 5 s | `PATIENCE` | the usage reader's thread taking its first ask (a thread, no child; raised because the brief named it) |

Each still fails loudly on its deadline, and now names it: `the file list
never arrived in 20s`, `the read never started in 20s`.

**Waits read and left alone**, in `shell.rs`'s test module:

- the `/usage` case's `quitting.elapsed() < 1 s` (`:37502`) and its
  reader's 30 s (`:37477`): one is what the case measures, the other is the
  slow read it measures against;
- the MCP waits (`:15280`-`:16016`, 60 s), `copy_settled` (`:22485`, 60 s),
  `copies_settled` (`:32173`, 60 s) and `read_settled` (`:36675`, 60 s):
  already past 20 s. Their 60 s is for fresh scripts, which macOS assesses
  on their first run (`:22481-22483`); `git` is a system binary, so this
  unit does not copy it;
- `saved_as` and `save_settled` (`:22869`, `:22884`, 10 s): a settings save
  on a thread, with no child;
- the FIFO watchdogs (`:23573`, `:36914`, 10 s): they bound a wait that must
  not happen;
- the title answers (`:27333`, `:27355`, 10 s): an in-process desk;
- `file_index::tests::settled` (`file_index.rs:1563-1573`, 2 s): every
  caller sets `git = None` or lists a directory, so no child is started.

## GREEN

The fix with `delay.patch`: both cases pass, in 8.03 s; `cargo test --lib
tui::` passes 2502, fails 0.

The whole lib suite, `--features fault-injection`, with `delay.patch`, 50
runs one at a time (`fifty.sh`, here):

All 50 runs passed: 3877 passed and 0 failed in each, and the two cases
`ok` in every one. A run took 19-57 s (mean 32.5 s) at a 1-minute load of
7.1-22.5; for part of the loop another worker's full-suite loop ran beside
it, against this machine's rule of one full suite at a time; the
coordinator capped the loop at 10 runs when it was at run 49, and the 50th
had ended before the stop. One row per run is in `fifty.tsv` (here). These
runs and the two above used the fix before `PATIENCE`'s comment was
reworded; the rewording was amended into `5898145e`, and the code is the
same.

## Look ahead

- **#60's lock makes these waits tighter.** `SPAWNING` (`src/process.rs:80`
  on `fix/p5-spawn-lock` @ `d162342d`, taken in `spawn`, `:109`, and in
  `pipe`, `:166`) is held by every start from before its pipes are made
  until `exec` closes the status pipe, and by every pipe xfx makes. With
  it, the index's `git` start (`file_index.rs:417` there,
  `crate::process::spawn`) queues behind every other start in the test
  process. A wait then covers up to (starts in flight) x (one start's time
  under the lock). On a runner of 4 vCPUs [추정: GitHub's size for its macOS
  Intel runners; the log does not say], four test threads in a window of
  about 4 s a start make about 16 s against 20 s. The log cannot say
  whether the slow part of a CI start was inside the lock (the `fork` and
  `exec`) or after it (the shim finding git, after its own `exec`); only
  the first adds up under the lock. If #60 lands and this flakes again,
  the next step is 60 s, `copy_settled`'s, not a second 20 s.
- **The same class outside the shell.**
  `tui::worker::tests::a_hostile_log_reaches_the_ui_inert_at_launch_and_from_the_picker`
  retries a resume for 5 s (`src/tui/worker.rs:8510`) while a sibling
  case's forked child holds the session lock's descriptor until it execs
  (`:8506-8509`). That is the same `fork`-to-`exec` time that was seconds
  on the runner. It is outside this unit's TUI shell tests, so it is left
  for the dispatcher. The fix is the same one line: 20 s. #60's lock does
  not change it: the lock file is not opened under `SPAWNING`.

## FLAKE-FILELISTb: a minute for a wait on a start, after #60

#60 (PROC-SPAWN) merged; `efcf9dcb` merges `origin/main` @ `0e9e7ce8` into
this branch. The look-ahead above says #60's lock can stack starts to about
16 s against 20 s. **Dispatcher decision (reversible): a test wait on work
that starts a child gets 60 s**, the MCP and clipboard waits' minute; a wait
is an upper bound and costs nothing unless it fails.

`crate::process::START_PATIENCE` (`src/process.rs:102-109`, test builds on
unix only) is that one constant. It lives beside the lock every start takes
because its users are in two modules. Each wait that uses it names it when
it fails. Lines are at the head:

| wait | was | waits on |
|---|---|---|
| `listed()`, `src/tui/shell.rs:31540` | `PATIENCE` (5 s at the base) | the index's worker, which starts `git` |
| `copy_settled`, `shell.rs:22493` | 60 s literal | a fresh fake tool's start |
| `copies_settled`, `shell.rs:32186` | 60 s literal | the same |
| `read_settled`, `shell.rs:36688` | 60 s literal | the same |
| the FIFO paste case, `shell.rs:36922` | 60 s literal | a shell built on its own thread, then `mkfifo` started |
| `a_hostile_log_reaches_the_ui_inert_at_launch_and_from_the_picker`, `src/tui/worker.rs:8511` | 5 s | a sibling's start: its forked child holds the session lock's descriptor until it execs (`:8506-8510`), so this is a start's time, and gets the start constant rather than 20 s |

`PATIENCE` (`shell.rs:14801-14805`) stays 20 s, now for the `/usage`
read-start wait alone (`:37500`): that waits on a thread, not a child.

The MCP waits (`shell.rs:15288`-`:16024`) keep their 60 s literals. Their
value is the constant's, but they are not all waits on a start: several,
`until_prompted` (`:15336-15360`) among them, wait for the trust prompt,
which comes before any server is started.

**RED and GREEN on the merged tree,** with `delay.patch` (it still applies
there):

- RED: `origin/main`'s `src/tui/shell.rs` (`listed()` at 5 s) on the merged
  tree, the two cases: both panic at `src/tui/shell.rs:31537:9`, `the file
  list never arrived`, in 5.02 s.
- GREEN: the head, the two cases and the worker case: 3 passed, in 8.04 s.

## Gates

The first round's gates, then FLAKE-FILELISTb's.


Run once on the fix, `5898145e`, with this receipt committed above it
before this table was added (that commit was then amended),
`TMPDIR=<tmp>/flake-filelist`. The suites ran only when no
other worker's xfx lib suite was running on this machine.

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| `cargo clippy --locked --all-targets --features fault-injection -- -D warnings` | ok |
| `cargo test --locked --no-fail-fast` | 14 binaries, 4757 passed, 0 failed (lib 3862) |
| `cargo test --locked --features fault-injection --no-fail-fast` | 14 binaries, 4786 passed, 0 failed (lib 3877) |
| `scripts/check-xfx-identity.sh` | ok, 8570 tracked files |
| `scripts/check-no-stubs.sh` | ok |
| `scripts/check-no-secrets.sh` | ok, 8570 tracked files |
| `cargo build --locked --release` | ok |
| `cargo build --locked --release --features fault-injection --target-dir target/faulty` | ok |
| `scripts/smoke.sh target/release/xfx` | 47 checks, 0 failures |
| `scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx` | 37 scenarios + the oracle, 1111 checks, 0 failures |

FLAKE-FILELISTb, on the merged head:

Run once on `62224285` (the code head; this table was then amended into
the receipt commit). The suites started at 05:24 KST, when no `cargo` and no
`xfx-*` test binary was running here.

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| `cargo clippy --locked --all-targets --features fault-injection -- -D warnings` | ok |
| `cargo test --locked --no-fail-fast` | 14 binaries, 4762 passed, 0 failed (lib 3867) |
| `cargo test --locked --features fault-injection --no-fail-fast` | 14 binaries, 4791 passed, 0 failed (lib 3882) |
| `scripts/check-xfx-identity.sh` | ok |
| `scripts/check-no-stubs.sh` | ok |
| `scripts/check-no-secrets.sh` | ok |
| `cargo build --locked --release` | ok |
| `cargo build --locked --release --features fault-injection --target-dir target/faulty` | ok |
| `scripts/smoke.sh target/release/xfx` | 47 checks, 0 failures |
| `scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx` | 37 scenarios + the oracle, 1111 checks, 0 failures |

## After #62 (2026-11-07)
#62 (US-U4) replaced the old `/usage` dashboard. Its tests now drive the screen on a simulated clock, and none of them waits on a wall clock for a read to start. Merging `main` @ `46c4efa6` therefore takes `main`'s side of the one conflict, which removes the old dashboard tests together with the `/usage` read-start wait in the table above. That left `PATIENCE` (20 s) with no user, so it is deleted. Every wait this unit changed now uses `START_PATIENCE` (60 s): `listed()`, `copy_settled`, and the worker's `worker.rs` wait.
