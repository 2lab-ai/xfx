---
title: CI-TOOLCHAIN-PIN receipt
type: receipt
tags: [xfx, ci, toolchain, phase5]
---

# CI-TOOLCHAIN-PIN: the CI gate builds with Rust 1.99.1, not whatever stable is

Branch `fix/p5-ci-toolchain-pin` on main `afeaea7c`. The change is commit
`92a3d906`, `.github/workflows/ci.yml` only.

## Why

- **The step floated.** ci.yml's step "Install the pinned toolchain" ran
  `rustup toolchain install stable ...` and `rustup default stable`, so the
  gate's compiler was whatever GitHub's stable was that day.
- **Stable moved to 1.100.0, and the gate went red on main's own code.** Run
  43019503559 attempt 2 (2026-11-16), job 133180137697,
  `gate (aarch64-apple-darwin)`: the toolchain step updated stable "to rustc
  1.100.0 (0ef1bc386 2026-11-09) (from rustc 1.99.1 (58cf8f9c3 2026-10-13))",
  then step 9 "Lints" failed `-D warnings` with `clippy::needless_range_loop`
  at `src/mcp/era.rs:449:18`. That loop is on main, not in the PR.
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
- **A green run on 1.99.1.** Preview run 43019311819 at `afeaea7c`
  (2026-11-11) ran fmt, clippy and tests on all four build rows, each
  printing `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, and published
  (`preview-and-bases.txt`).
- **Local gates already use 1.99.1:** `rustc 1.99.1 (58cf8f9c3 2026-10-13)`.
- Evidence: `ci-run-43019503559.txt`, read from the per-job logs API.

## The change

```
-          rustup toolchain install stable --profile minimal --component rustfmt,clippy
-          rustup default stable
+          rustup toolchain install 1.99.1 --profile minimal --component rustfmt,clippy
+          rustup default 1.99.1
```

- `git show --numstat 92a3d906` is `2 2 .github/workflows/ci.yml`. Both lines
  are edited in place: no line is added or removed, the `rustc --version` line
  stays, and every other line is byte for byte main's.
- So the line numbers and step order cited elsewhere hold: the
  LOCAL-PATHS-EXIT-TRAP receipt's ci.yml lines and test-gate-env.sh's
  `ci-bash32` (see the merge-trees below).
- There is no `rust-toolchain` or `rust-toolchain.toml` in the tree to override
  `rustup default`.
- actionlint 1.7.12 passes ci.yml and every workflow (`actionlint.txt`).

## Left on stable, and why

| file | why it stays | runs clippy `-D warnings`? |
|---|---|---|
| `preview.yml` | `scripts/check-preview-contract.sh:604` requires `rustup toolchain install stable` in its build job. | yes, the build job's "Lints" (`:299-300`) |
| `release.yml` | It moves with TOOLCHAIN-1100, together with preview.yml. | yes, "Lints" (`:69-70`) |
| `sweep.yml`, `sweep-full.yml` | they build for the parity sweep with no clippy | no |

**The preview's gates run clippy.** `CONTRACT_GATES`
(`check-preview-contract.sh:114-119`) lists
`cargo clippy --locked --all-targets -- -D warnings`. The contract requires
each gate to run before `cargo build --locked --release` in the build job, and
preview.yml's build job does so at `:299-300`. That is the default-feature
invocation, the same as ci.yml's "Lints", the step that failed on 1.100.

So the next preview epoch bump, on stable 1.100 and with `era.rs:449`
unchanged, fails closed. Each build row fails at "Lints". `publish`
(`needs: [preflight, build]`, and an `if:` with no status function, to which
GitHub's documented default adds `success()`) is skipped, and nothing is
tagged or pushed to the tap. A `v*` tag would fail the same way in
release.yml.

The next bump will be the first preview on 1.100. #98's own bump, at
`afeaea7c`, published on 2026-11-11 in preview run 43019311819.
- All four build rows printed `rustc 1.99.1 (58cf8f9c3 2026-10-13)`.
- `publish` succeeded, its steps "Publish the prerelease" and "Put this
  preview in the Homebrew tap" among them.
- That was the day before 1.100.0's rustup release date of 2026-11-12, which
  the attempt-2 log prints.
- Evidence: `preview-and-bases.txt`.

## Follow-up: TOOLCHAIN-1100

The move to 1.100 is a separate unit, and it must land before the next
preview epoch bump:

1. On every tree in flight, fix the lints 1.100 raises, both feature sets.
2. Then bump ci.yml's pin and the preview contract's stable requirement
   together, along with preview.yml and release.yml.

## The readers of ci.yml, run at `92a3d906`

What they are:
- `grep -rn 'ci\.yml' scripts tests`;
- `grep -rln 'toolchain' scripts tests`;
- `grep -rln '\.github/workflows' scripts tests` (for readers that glob
  `*.yml`);
- and every script test ci.yml's `script_tests` row runs.

Each was run with `TMPDIR=<scratch>`, under `<scratch>/bin/suite-lock.sh`, one
at a time (`readers-head.txt`, one output file each):

| reader | why it is one | result |
|---|---|---|
| `scripts/test-gate-env.sh` | reads ci.yml's matrix and checkout step (`:824`, `:862`), every workflow's git steps (`:806`); runs `test-check-no-secrets.sh` nested | ok, 66 cases (`ci-script-tests-per-os`, `ci-checkout-assertion`, `workflow-steps` among them; nested test-check-no-secrets 13 cases) |
| `scripts/test-preview-contract.sh` | `script_tests` row; a mutant names `./.github/workflows/ci.yml` (`:251`) | ok, 28 mutants caught |
| `scripts/test-check-no-local-paths.sh` | `script_tests` row | ok, 31 cases |
| `scripts/check-preview-contract.sh` | reads every workflow (`:195`), and holds `rustup toolchain install stable` in preview.yml (`:604`) | ok |
| `scripts/fxcap/test-sweep.sh` (runs `test_ci_sweep.py`) | `toolchain` hit at `test_ci_sweep.py:590`; it exercises sweep.yml, not ci.yml | ok, 79 + 28 tests |

Not run:
- `scripts/check-tui-preflight-cost.sh`. Its only hit is the word
  "toolchain" in a comment (`:64`), and it is a release-mode timing gate, not
  a script test.
- `tests/tool_loop.rs:1417` and `tests/permissions.rs:2022`. They write a
  file named `ci.yml` into a scratch tree as fixture data and never read this
  one.

**The same grep on the merge-tree sides.** It was widened to `rustup`,
`+stable`, `RUSTUP_TOOLCHAIN` and `rustup run`, and run read-only on the head
and on each of the four sides (`readers-refs.txt`).
- No ref has a script or test that picks its own toolchain.
- The only hit asserting `stable` on any ref is `check-preview-contract.sh:604`,
  which reads preview.yml.
- `d8e2eeeb` adds test-gate-env.sh's `ci-bash32`. It reads ci.yml's matrix
  key `bash32` and the steps under it, not the rustup lines.
- The other three sides have the head's hits; only fixture line numbers
  differ.

## Cargo at `92a3d906`, local 1.99.1

Each ran under `suite-lock.sh` (`cargo.txt`):
- `cargo fmt --check`: exit 0;
- `cargo clippy --locked --all-targets -- -D warnings`: exit 0;
- `cargo clippy --locked --all-targets --features fault-injection -- -D warnings`:
  exit 0.

No Rust changed, so there is no `cargo test` here.

## Merge-trees (read-only)

`git merge-tree --write-tree` of the head against four refs, with
`train/p5-1116` resolved once (`merge-trees.txt`):
- `d8e2eeeb`, EXIT-TRAP, which changes ci.yml;
- `866747ff`, train 15;
- `ef595df2`, CI-PTY-WAIT;
- train 16 at `2b78aab6`, which has the ci.yml comment commit `3d1376b2`.

No tree was checked out. `merge-trees.txt` is of `92a3d906`. At the receipt
commits, which add only files under this folder, the same four merges are
clean, with both rustup lines at 1.99.1 (re-run before handback).

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
  descends from `afeaea7c`. Each one's merge-base with it is main's parent
  `e2fc7e94` (`preview-and-bases.txt`). So the
  merge brings in #98's epoch line, and the two files the merge changes
  against each side are `.github/preview-epoch` and `.github/workflows/ci.yml`.

## The real receipt

This unit's real receipt is the CI run of its PR. The dispatcher reads it.
- In every row, the toolchain step's `rustc --version` must print
  `rustc 1.99.1 (58cf8f9c3 2026-10-13)`.
- "Lints" must pass on every row.
