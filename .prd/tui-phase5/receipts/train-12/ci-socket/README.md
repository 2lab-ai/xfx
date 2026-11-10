---
title: 0.0.16 delta -- P5 train 12, CI sweep job's socket path (TRAIN12-CI-SOCKET)
type: receipt
tags: [xfx, phase5, fx-0.0.16, train, fxcap]
---

# Train 12: the capture tests' tmux socket at the CI runner's TMPDIR

PR #90 (head `f53606d1`) failed the sweep job's first harness step,
`./scripts/fxcap/test-capture.sh`: 1 of 107 tests, `RunRoots`
`test_every_file_a_run_writes_names_its_roots_by_placeholder_at_any_tmpdir`
at `extra=26`, `error connecting to <tmpdir>/.../fxcap.XXXXXX/s (File name too
long)`. Harness work only, no `src/` change. Every figure below is in a file in
this folder. The logs have local paths written as `<repo>`, `<scratch>`,
`<tmpdir>`, `<home>`, `<user>` and `<oracle-dir>`.

| commit | what |
|---|---|
| `44bb45e6` | the fix: `capture.sh` refuses a TMPDIR too long for its socket; `test_capture.py` takes RunRoots' extras from the room that is left, plus a Refusals test |
| `ea772025` | the ledger's four `capture.sh` references the ten new lines moved (numbers only) |
| the next commit | this receipt |

## The cause: 105 bytes, against a limit of 103

- **The limit** (`limit-probe.txt`, tmux 3.7b here): a server on a socket
  path of 103 bytes starts, and one on 104 bytes is refused with `File name
  too long`. macOS's `sun_path` is 104 bytes with its NUL.
- **The CI run** (`ci-failure-excerpt.txt`, the job's failure lines with
  the runner's paths as placeholders; `lengths.txt` counts them from the
  log): the runner's TMPDIR is 49 bytes, and the socket tmux refused is 105
  bytes.
- **Why only CI.** The test's scratch dir is `<TMPDIR>fxcapt.XXXXXXXX` (64
  bytes at a 49-byte TMPDIR). `scratch()` falls back to `/tmp` only past 50
  bytes. The fixed extras `(0, 9, 26)` put the socket at 79, 88 and **105**
  bytes there. At the gate's 23-byte TMPDIR they put it at 54, 63 and 80
  bytes, so every local gate passed (`lengths.txt`).

## RED (harness at `f53606d1`, TMPDIR of the runner's length)

The TMPDIR is `<tmpdir>/` plus a 32-byte name plus `/`: 49 bytes, through
the `/tmp` link, so it resolves to 57 bytes (`lengths.txt`). The runner's
TMPDIR also goes through a link (`/var`): CI's roots line reads `<tmp>=90
<tmp:real>=98`, and so does this one (`ci-failure-excerpt.txt`,
`red-one-long.txt`). `runs.sh` holds every command below.

- `red-one-long.txt`: the one test fails at `extra=26`. It prints the
  roots line CI printed (`<work>=108 <work:real>=116 <scratch>=103
  <scratch:real>=111 <scenarios>=108 ... <tmp>=90 <tmp:real>=98`; only
  `<bun>` and `<home>` differ, `ci-failure-excerpt.txt`) and the same tmux
  error.
- `red-full-long.txt`: `Ran 107 tests`, `FAILED (failures=1)`. That one
  failure is the same subtest.

## The fix (`44bb45e6`)

- **The test** (`test_capture.py`: `SOCKET_MAX`, `headroom()`, and the
  RunRoots test). The extras are `0`, half the room, and all of it, with
  room = 103 - len(scratch dir + `/fxcap.XXXXXX/s`). So the longest run
  puts its socket at exactly 103 bytes on any machine: 79/91/103 at the
  runner's length, 54/78/103 at the gate's (`lengths.txt`). The purpose
  stays: three different root lengths. If the room is under 4 bytes, the
  test fails and names the limit (it does not skip).
- **capture.sh.** Right after it strips the trailing slashes and before
  `mktemp`, it counts the bytes of `<TMPDIR>/fxcap.XXXXXX/s`. Past 103, it
  exits 2 with `TMPDIR is too long for this run's tmux socket: ... would be
  N bytes, and a socket path holds at most 103 ...: a TMPDIR of at most M
  bytes, not K`. This happens before the gateway, any run directory or the
  roots line, and the path itself is not printed.
- **`Refusals.test_a_tmpdir_too_long_for_the_socket`** pins the refusal at
  104 bytes, with and without a trailing slash: exit 2, the message, empty
  stdout, no `fxcap.*` directory. RunRoots' longest extra pins the 103-byte
  side.
- **Mutants.**
  - `mutant-guard-removed.txt`: with the guard taken out, the refusal test
    fails both subtests on tmux's `File name too long`.
  - `mutant-test-only.txt`: the new test file against `f53606d1`'s
    capture.sh. RunRoots passes on the test change alone, and Refusals
    fails.
- **Rejected: a short socket dir outside TMPDIR.** It would add a root that
  LOCAL-PATHS's `roots.py` and `check-no-local-paths.sh` would have to
  redact. It would also change where `sweep.py` `stop_servers()` looks for
  sockets (`<jobtmp>/fxcap.*/s`). That is more surface than the bug needs.

## GREEN at both lengths (harness at `44bb45e6`)

| TMPDIR | targeted (RunRoots + the refusal) | whole `test-capture.sh` |
|---|---|---|
| 49 bytes, the runner's length | `green-targeted-long.txt`: 2 OK | `green-full-long.txt`: `Ran 108 tests`, `OK` |
| 23 bytes, the gate's | `green-targeted-short.txt`: 2 OK | `green-full-short.txt`: `Ran 108 tests`, `OK` |

The suite has 108 tests: the 107 from before plus the refusal test.

## Look-ahead: the sweep job's later steps at the 49-byte TMPDIR

- **`test-sweep.sh`** (`sweep-tests-long.txt`): 72 and 27 tests OK. It was
  run again on `ea772025`'s ledger (`sweep-tests-long-ledger.txt`): OK.
- **The build** (`build-long.txt`): `cargo build --locked` passed.
- **The sweep step.** `ci-sweep.sh` ran as `sweep.yml` runs it: `--jobs 2`,
  a short `--tmp` (CI passes `$RUNNER_TEMP`), the long TMPDIR in the
  environment, the head's build as both `--xfx` and `--base-xfx`, and
  `--core sample-core.txt`.
  - The sample (`sample-core.txt`) has 11 scenarios: the four p5 scenarios
    LOCAL-PATHS swept, six of the nine scenarios SWEEP-WALLCLOCK-F's
    `7d6b552a` edited when it ended the catalog outage at a scenario line
    (what retired `catalog-window`), and `startup`.
  - Result (`ci-sweep-sample-long.txt`, `sweep-report.md`,
    `sweep-result.json`): every run ran (23 at the head, 7 at the base),
    `failed 0, timeout 0` in both, 65 pairs, 53 pass, exit 0. That shows the
    sweep step runs at the long TMPDIR, and nothing more. It ran with
    `--base HEAD` and one binary as both `--xfx` and `--base-xfx`, so it
    compared the head with itself. Its "0 regression(s)" is no evidence,
    and this receipt does not rest on it.
  - The log calls the head sweep's binary `<base-xfx-bin>`
    (`ci-sweep-sample-long.txt:4`). The two paths were the same, and the
    redactor's map keeps the last name given for a path
    (`scripts/fxcap/ci_sweep.py:595-598`).
  - **What the look-ahead rests on** (`sample-vs-train12-head.txt`):
    each scenario's pass count matches train 12's own head sweep
    (`../sweep-head.md`), 11 of 11, so the sample read the same at the long
    TMPDIR as train 12's sweep did.
  - The fx oracle's sha256 was `8e6cda2e...` before and after the sweep,
    which is the pin (`oracle-sha.txt`). `FX_AUTO_UPGRADE=0` was set.
- **What else breaks only at the longer TMPDIR: nothing found.** In the
  sweep step, capture.sh gets `TMPDIR=<--tmp>/fxsw.<id>/NNN`, not the
  runner's TMPDIR. `sweep.py` already refuses a `--tmp` whose socket would
  pass 100 bytes. So the runner's TMPDIR reaches only the harness tests.
- **The ledger** (`ea772025`). Its line references to `capture.sh` below the
  insertion now point at the lines they cited: `:671` -> `:681`, `:970` ->
  `:980`, `:977` -> `:987`, `:1147` -> `:1157`.

## Open

- **CI's tmux version is not in the job log this unit had.** CI installs
  tmux with `brew install tmux`. 103 is the limit on this machine's tmux
  3.7b (`limit-probe.txt`). The code's limit also comes from macOS's
  104-byte `sun_path`, which does not depend on the tmux version.
- **The limit is macOS's.** On Linux (`sun_path` 108), capture.sh would
  refuse a path of 104 to 107 bytes that would have worked. No runner or
  gate uses Linux.

## Files

`ci-failure-excerpt.txt` · `runs.sh` (every run, in order, under the suite lock) · `lengths.py` →
`lengths.txt` · `limit-probe.sh` → `limit-probe.txt` · `mutant.sh` →
`mutant-guard-removed.txt` · `mutant-test-only.sh` → `mutant-test-only.txt`
· `red-one-long.txt` · `red-full-long.txt` · `green-targeted-long.txt` ·
`green-targeted-short.txt` · `green-full-long.txt` · `green-full-short.txt`
· `sweep-tests-long.txt` · `sweep-tests-long-ledger.txt` · `build-long.txt`
· `sample-core.txt` · `ci-sweep-sample-long.txt` · `sweep-report.md` ·
`sweep-result.json` · `sample_vs_head.py` → `sample-vs-train12-head.txt` ·
`oracle-sha.txt`
