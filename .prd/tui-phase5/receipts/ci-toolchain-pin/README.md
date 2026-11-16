---
title: CI-TOOLCHAIN-PIN receipt
type: receipt
tags: [xfx, ci, toolchain, phase5]
---

# CI-TOOLCHAIN-PIN: the CI gate builds with Rust 1.99.1, not whatever stable is

Branch `fix/p5-ci-toolchain-pin` on main `afeaea7c`. The code change is
commit `92a3d906`, to `.github/workflows/ci.yml` only. Every later commit
touches only this folder (`numstat.txt`).

## Why

- **The step floated.** ci.yml's step "Install the pinned toolchain" ran
  `rustup toolchain install stable ...` and `rustup default stable`, so the
  gate's compiler was whatever GitHub's stable was that day.
- **Stable moved to 1.100.0, and the gate went red on main's own code.** This
  happened in run 43019503559 attempt 2 (2026-11-16), job 133180137697,
  `gate (aarch64-apple-darwin)`.
  - The toolchain step logged `stable-aarch64-apple-darwin updated - rustc
    1.100.0 (0ef1bc386 2026-11-09) (from rustc 1.99.1 (58cf8f9c3 2026-10-13))`.
  - Then step 9 "Lints" failed `-D warnings` with `clippy::needless_range_loop`
    at `src/mcp/era.rs:449:18`.
  - "Step 9" is that run's numbering, from its head `d8e2eeeb`'s ci.yml, which
    adds the steps "The gate scripts run /bin/bash" and "Which bash the gate
    scripts run" (`era-449.txt`).
  - The loop is main's: it is at `afeaea7c:src/mcp/era.rs:449`, and the PR
    does not change that file (`era-449.txt`).
- **1.99.1 passed the same step.** Attempt 1 (2026-11-11), job 130518847044,
  is the same row on rustc 1.99.1 (58cf8f9c3); its backtraces are under
  `/rustc/58cf8f9c3`.
  - Step 9 "Lints" passed.
  - The job itself failed, at step 11 ("Tests, with the fault injector"). Its
    fault-injection clippy finished, then one `--test tui` case failed:
    `the_session_reports_its_status_between_frames_and_clears_it_on_the_way_out`,
    an assertion at `tests/tui.rs:852`.
  - That is a test assertion, not a lint, so attempt 1 shows Lints green on
    1.99.1, not a green run.
- **A green preview run of the default-feature gates on 1.99.1.** Preview run
  43019311819 at `afeaea7c` (2026-11-11) passed on all four build rows, each
  printing `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, and published
  (`preview-and-bases.txt`).
  - It ran the preview's gates: fmt, default-feature clippy, tests and three
    check scripts (`CONTRACT_GATES`, `workflows-at-pin.txt`).
  - That is not ci.yml's whole gate. It has no fault-injection clippy or tests.
- **Local gates already use 1.99.1:** `rustc 1.99.1 (58cf8f9c3 2026-10-13)
  (Homebrew)` (`cargo.txt`). That is Homebrew's build of the same upstream
  commit.
- Evidence: `ci-run-43019503559.txt`, read from the per-job logs API.

## The change

```
-          rustup toolchain install stable --profile minimal --component rustfmt,clippy
-          rustup default stable
+          rustup toolchain install 1.99.1 --profile minimal --component rustfmt,clippy
+          rustup default 1.99.1
```

- `git diff --numstat afeaea7c..<head>` is `2 2 .github/workflows/ci.yml`
  plus files in this folder. `numstat.txt` has that, `git diff afeaea7c
  92a3d906`, and `git show --stat --format=%h` of every commit.
- Both lines are edited in place: no line is added or removed, the
  `rustc --version` line stays, and every other line is byte for byte main's.
- So the line numbers and step order cited elsewhere hold: the
  LOCAL-PATHS-EXIT-TRAP receipt's ci.yml lines and test-gate-env.sh's
  `ci-bash32` (see the merge-trees below).
- The tree at `92a3d906` has no `rust-toolchain`, `rust-toolchain.toml` or
  `.cargo/config(.toml)` that could override `rustup default` (count 0,
  `readers-grep.txt`).
- **The `rustc --version` line is diagnostic only.** It prints the selected
  compiler; it does not fail the step if some higher-priority override
  selected another one. An assertion in the step belongs to TOOLCHAIN-1100,
  because this unit adds no line.
- actionlint 1.7.12 passes ci.yml and every workflow (`actionlint.txt`).

## Left on stable, and why

Every row below is read at `92a3d906` (`workflows-at-pin.txt`).

| file | trigger | why it stays | runs clippy `-D warnings`? |
|---|---|---|---|
| `preview.yml` | push to main changing `.github/preview-epoch` (`:1-4`) | `scripts/check-preview-contract.sh:604` requires `rustup toolchain install stable` in its build job | yes, the build job's "Lints" (`:299-300`) |
| `release.yml` | a `v*` tag (`:1-3`) | it moves with TOOLCHAIN-1100, together with preview.yml | yes, "Lints" (`:69-70`) |
| `sweep.yml` | **every pull request**, and push to main (`:1-4`) | it builds for the parity sweep only | no: only `cargo build --locked` (`:152`, `:154`), no clippy or fmt |
| `sweep-full.yml` | push to main changing `.github/preview-epoch`, and dispatch (`:1-5`) | as sweep.yml | no: only `cargo build --locked` (`:77`) |

- **sweep.yml floats on stable on every PR** (`:133-134`). It runs no lint and
  no fmt, so the 1.100 lint cannot fail a PR there. Only a hard compile error
  on a newer stable could.
- **Two steps are misnamed.** preview.yml's step at `:277` and release.yml's at
  `:47` are named "Install the pinned toolchain" but install `stable`
  (`:279-280`, `:49-50`). The contract checks run text, not step names, so
  pinning or renaming them is TOOLCHAIN-1100's.

**The preview's gates run clippy.**
- `CONTRACT_GATES` (`check-preview-contract.sh:114-119`) lists
  `cargo clippy --locked --all-targets -- -D warnings`.
- The contract requires each gate to run before `cargo build --locked
  --release` in the build job. preview.yml's build job runs it at `:299-300`.
- That is the default-feature invocation, the same as ci.yml's "Lints", the
  step that failed on 1.100.

**So the next preview epoch bump fails closed**, on stable 1.100 and with
`era.rs:449` unchanged.
- Each build row fails at "Lints".
- `publish` is skipped. It has `needs: [preflight, build]` and an `if:` with
  no status function (`:402-403`), to which GitHub's documented default adds
  `success()`.
- Nothing is tagged or pushed to the tap.
- A `v*` tag would fail the same way in release.yml.

The next bump will be the first preview attempt on 1.100. #98's own bump, at
`afeaea7c`, published on 2026-11-11 in preview run 43019311819.
- All four build rows printed `rustc 1.99.1 (58cf8f9c3 2026-10-13)`.
- `publish` succeeded, its steps "Publish the prerelease" and "Put this
  preview in the Homebrew tap" among them.
- That was the day before 1.100.0's rustup release date of 2026-11-12, which
  the attempt-2 log prints.
- Evidence: `preview-and-bases.txt`.

## Follow-up: TOOLCHAIN-1100

The move to 1.100 is a separate unit. **It must land before the next preview
epoch bump or the next `v*` release tag, whichever comes first.** Both
publish from stable and both run clippy `-D warnings`.

Its scope:
1. On every tree in flight, fix the lints 1.100 raises, in both feature sets.
2. Then bump ci.yml's pin and the preview contract's stable requirement
   (`check-preview-contract.sh:604`) together, along with preview.yml and
   release.yml.
3. Pin or rename the two steps named "Install the pinned toolchain" that float
   (preview.yml:277, release.yml:47).
4. Optionally, make the toolchain step assert the selected version instead of
   only printing it.

## The readers of ci.yml, run at `92a3d906`

They were found by three greps, run read-only at `92a3d906` over `scripts/`
and `tests/`. Their output is in `readers-grep.txt`.
- `git grep -n 'ci\.yml'`;
- `git grep -ln 'toolchain'`;
- `git grep -n '\.github/workflows'`, for readers that glob `*.yml`;
- and every script test ci.yml's `script_tests` row runs.

`how-run.txt` records how they ran: `TMPDIR=<scratch>`, each under
`<scratch>/bin/suite-lock.sh`, one at a time. Exits and timings are in
`readers-head.txt`.

| reader | why it is one | result (output file) |
|---|---|---|
| `scripts/test-gate-env.sh` | reads ci.yml's matrix and checkout step (`:824`, `:862`) and every workflow's git steps (`:806`); runs `test-check-no-secrets.sh` nested | ok, 66 cases, `ci-script-tests-per-os`, `ci-checkout-assertion` and `workflow-steps` among them; the nested test-check-no-secrets ran 13 cases (`test-gate-env.txt`) |
| `scripts/test-preview-contract.sh` | `script_tests` row; copies every workflow (`:45`); a mutant names `./.github/workflows/ci.yml` (`:251`) | ok, 28 mutants caught (`test-preview-contract.txt`) |
| `scripts/test-check-no-local-paths.sh` | `script_tests` row | ok, 31 cases (`test-check-no-local-paths.txt`) |
| `scripts/check-preview-contract.sh` | reads every workflow (`:195`), and holds `rustup toolchain install stable` in preview.yml (`:604`) | ok (`check-preview-contract.txt`) |
| `scripts/fxcap/test-sweep.sh` (runs `test_ci_sweep.py`) | `toolchain` hit at `test_ci_sweep.py:590`; it exercises sweep.yml, not ci.yml | ok, 79 + 28 tests (`test-sweep.txt`) |

Not run:
- `scripts/check-tui-preflight-cost.sh`. Its only hit is the word
  "toolchain" in a comment (`:64`), and it is a release-mode timing gate, not
  a script test.
- `tests/tool_loop.rs:1417` and `tests/permissions.rs:2022`. They write a
  file named `ci.yml` into a scratch tree as fixture data and never read this
  one.
- `scripts/fxcap/ci-core.txt:1` and `tests/permissions.rs:2017`. These are
  comments that name `.github/workflows`.

**The same grep on the merge-tree sides.** It was widened to `rustup`,
`+stable`, `RUSTUP_TOOLCHAIN` and `rustup run`, and run read-only on
`ca367a46` and on each of the four sides (`readers-refs.txt`).
- No ref has a script or test that picks its own toolchain.
- The only hit asserting `stable` on any ref is `check-preview-contract.sh:604`,
  which reads preview.yml.
- `d8e2eeeb` adds test-gate-env.sh's `ci-bash32`. It reads ci.yml's matrix
  key `bash32` and the steps under it, not the rustup lines.
- The other three sides have the head's hits; only fixture line numbers
  differ.

## Cargo at `92a3d906`, local 1.99.1

Each ran under `suite-lock.sh` (`cargo.txt`; setup in `how-run.txt`):
- `cargo fmt --check`: exit 0;
- `cargo clippy --locked --all-targets -- -D warnings`: exit 0;
- `cargo clippy --locked --all-targets --features fault-injection -- -D warnings`:
  exit 0.

No Rust changed, so there is no `cargo test` here.

## Merge-trees (read-only)

`git merge-tree --write-tree` was run against four refs, with
`train/p5-1116` resolved once:
- `d8e2eeeb`, EXIT-TRAP, which changes ci.yml;
- `866747ff`, train 15;
- `ef595df2`, CI-PTY-WAIT;
- train 16 at `2b78aab6`, which has the ci.yml comment commit `3d1376b2`.

No tree was checked out. The runs are in two files:
- `merge-trees.txt`, at the code commit `92a3d906`.
- `merge-trees-receipt.txt`, at round 2's receipt commit. That is the last
  commit before the files that cover it, and it changes nothing outside this
  folder.

The commit that adds `numstat.txt` and `merge-trees-receipt.txt` adds only
those two files.

| other side | clean | the merged ci.yml's rustup lines | merged ci.yml against its own |
|---|---|---|---|
| `d8e2eeeb` | yes | `:122-123`, 1.99.1 | only those 2 lines differ; 333 lines both |
| `866747ff` | yes | `:113-114`, 1.99.1 | only those 2 lines differ; 285 lines both |
| `ef595df2` | yes | `:113-114`, 1.99.1 | only those 2 lines differ; 285 lines both |
| `2b78aab6` | yes | `:119-120`, 1.99.1 | only those 2 lines differ; 315 lines both |

- **Line numbers and step order hold on every side.** In each case the merged
  ci.yml differs from that side's own in exactly the two rustup lines. This
  covers what EXIT-TRAP's receipt and `ci-bash32` cite, on the tree they cite
  it on.
- **Each merge also carries `.github/preview-epoch`.** None of the four
  descends from `afeaea7c`; each one's merge-base with it is main's parent
  `e2fc7e94` (`preview-and-bases.txt`).
  - So each merge brings in #98's epoch line.
  - The two files the merge changes against each side are
    `.github/preview-epoch` and `.github/workflows/ci.yml`.

## The real receipt

This unit's real receipt is the CI run of its PR, and the dispatcher reads it.
It passes when:
- **The toolchain:** on all 5 gate jobs, the toolchain step's `rustc
  --version` prints `rustc 1.99.1 (58cf8f9c3 2026-10-13)`. That step has no
  `if:`, so every job runs it.
- **The lint:** "Lints" passes on the 4 jobs that run it. The
  `x86_64-apple-darwin, release` job skips it by design
  (`if: matrix.part != 'release'`, ci.yml:131).
- **The whole run** is either green, or red only because of the known
  `tests/tui.rs:852` flake (CI-PTY-WAIT's). A red run must be read and stated
  as that, case named; Lints alone is not enough.
