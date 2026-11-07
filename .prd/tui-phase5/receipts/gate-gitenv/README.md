---
title: GATE-GITENV receipt -- no gate reads, passes or writes by git's environment
type: receipt
tags: [xfx, phase5, ci, gates, git, receipt]
---

# GATE-GITENV: the gates and git's environment

Every claim below is from a log in this folder, cited as `file: section`.
A claim with no log is marked *(not measured here)* with its source. Line
numbers are at the code head, `5d33c5df`. The base is `45d99f52`, the
SECRETS-ARGMAX head with origin/main merged in.

**The goal:** no environment or config a caller inherits can make a CI gate
read anything but the repository it is checking, pass a violation, or write
into any repository.

## What CI runs, and what of it runs git

`inventory.txt` lists every script the workflows run, from one-line and
block `run:` steps, and which of them run git.

- **Shell gates that run git:** `check-no-secrets.sh`,
  `check-xfx-identity.sh`, `check-preview-contract.sh`.
- **Python, run by sweep.yml:** the parity sweep's `ci_sweep.py` and
  `sweep.py`. They are reached through `ci-sweep.sh` and `test-sweep.sh`.
- **No git calls:** `check-no-stubs.sh`, `check-tui-preflight-cost.sh`,
  `check-usage-clock.sh`, `test-preview-contract.sh`, `smoke.sh` and
  `smoke-tui.sh`.
  - `smoke.sh` and `smoke-tui.sh` start the binary they drive in an
    environment built from nothing, so no `GIT_*` reaches it (`smoke.sh:432-433`,
    `smoke-tui.sh:3223-3229`).
  - The two cargo gates build `build.rs`. It runs only `git rev-parse`
    (`build.rs:61,64` through its helper at `:98`, and `:111-112`), which
    reads.
- **Hooks:** nothing in `scripts`, `.github`, `Cargo.toml` or `build.rs`
  installs or names a git hook (`inventory.txt`: `hooks`).

## RED

`gitenv-sweep.sh` runs each git-reading gate on a fixture that holds one
violation:
- `ipath`: a tracked path carrying the retired name, for identity;
- `ilock`: the retired name inside a tracked `Cargo.lock`, for identity;
- `keyed`: a key in `b.txt`, for no-secrets;
- `readme`: a README without its install command, for preview-contract.

Each run sets one variable at a decoy repository, or puts one attribute file
in the clone. A row is correct when the gate exits 1 with its violation named
and every file under the decoy's `.git` is unchanged.

The rows that are wrong at the base (`runs-macos.txt` and `runs-linux.txt`:
`the sweep at 45d99f52`):

| variable or file | identity (`ipath`, `ilock`) | no-secrets | preview-contract |
|---|---|---|---|
| `GIT_DIR` | **exit 0**, decoy written | exit 1 | exit 128 (macOS) or 1 (Linux), **decoy written** |
| `GIT_DIR` + `GIT_WORK_TREE` | exit 1 (a control), **decoy written** | exit 1 | **decoy written** |
| `GIT_INDEX_FILE` (an index without the violation) | **exit 0** | **exit 0, `ok (2 tracked file(s))`** | exit 1 (a git error) |
| `GIT_OBJECT_DIRECTORY` | exit 1, **decoy written** | exit 1, **decoy written** | exit 128, **decoy written** |
| `GIT_COMMON_DIR` | macOS: **decoy written**; Linux: exit 1, a control | macOS: **decoy written**; Linux: correct | macOS: exit 128, **decoy written**; Linux: exit 1, a git error |
| `GIT_CONFIG_GLOBAL`, `GIT_CONFIG_SYSTEM`, `GIT_CONFIG_COUNT`, `GIT_CONFIG_PARAMETERS` → `core.attributesFile` with `*.lock -diff` | `ilock` **exit 0** | - | - |
| `GIT_ATTR_SOURCE` (a tree whose `.gitattributes` says the same) | `ilock` **exit 0** on macOS (git 2.54); correct on Linux (git 2.39) | - | - |
| `.git/info/attributes`, `.git/config core.attributesFile`, an untracked `.gitattributes` | `ilock` **exit 0** | - | - |

Some variables made no wrong row on either platform: `GIT_WORK_TREE` alone
(exit 128, git refuses it), `GIT_ALTERNATE_OBJECT_DIRECTORIES`,
`GIT_NAMESPACE`, `GIT_CEILING_DIRECTORIES`, `GIT_CONFIG_NOSYSTEM`, and
`GIT_EXTERNAL_DIFF` with `GIT_DIFF_OPTS` (no gate runs the external diff).
A `.git/config` `color.grep=always` was refused correctly at the base.

**The sweep's own python** (`py-runs-*.txt`, at `4ac0667e`, before its fix):
- `ci_sweep.py --select` with `GIT_DIR` at a decoy cloned at the base, where
  base and HEAD are one commit, printed only the core set: the first tuple in
  the assertion. The plain run's selection is the second.
- `sweep.py`'s `--out` guard asks git for the main worktree; under `GIT_DIR`
  it named the decoy.

## The design point: which index, inside a hook

`hook-probe.sh` gives a scratch repository a pre-commit hook. The hook
records what git exports and runs `check-no-secrets.sh`, then lets the
commit through. Both runs are in `runs-*.txt`, under `the hook probe`:

| commit | `GIT_INDEX_FILE` in the hook | check at 45d99f52 | check at the head |
|---|---|---|---|
| `git commit`, a key staged | `.git/index` (relative) | exit 1 | exit 1 |
| `git commit -a`, a key edited in | `.git/index.lock` | exit 1 | exit 1 |
| `git commit -- c.txt` while `b.txt` with a key is staged (partial: b.txt not committed) | `.git/next-index-<pid>.lock` | exit 0 | **exit 1**: the checkout's own index still holds b.txt |
| from a linked worktree (`GIT_DIR=.git/worktrees/linked`) | that worktree's index | exit 0 | exit 0, the same 4 files |
| a key staged, then the working-tree file cleaned | `.git/index` | exit 0 | exit 0, **while HEAD now holds the key** |

**The decision: the checkout's own index, and no `GIT_*` at all.**
- These gates read file *content* from the working tree, never from an
  index. The last row shows it: the commit publishes a key that both
  versions miss. So honouring `GIT_INDEX_FILE` would not make them commit
  checks. It would only change which names they list.
- A hook's `GIT_INDEX_FILE` and a stale exported one are the same thing to
  the gate. The second made identity and no-secrets pass (RED above).
- In a linked worktree, dropping `GIT_DIR` loses nothing: discovery through
  the worktree's `.git` file finds the same repository and index.
- The cost shows in the partial-commit row. A key that is staged but not in
  this commit is refused now. That is stricter, not looser.
- Nothing installs a hook; CI runs the gates on a fresh checkout.

The last row is a separate, pre-existing limit, not fixed here: run as a
pre-commit hook, `check-no-secrets.sh` does not check what the commit
publishes. A commit gate would have to read the index's blobs.

## The fix

- **`GIT_*` dropped** before any git runs in `check-no-secrets.sh:24-42`,
  `check-xfx-identity.sh:26-41` and `check-preview-contract.sh:49-63`: every
  exported variable whose name starts `GIT_`. Each gate then checks the
  checkout its script is in. `check-preview-contract.sh`'s `scratch_git`
  keeps pinning `GIT_CONFIG_GLOBAL`/`GIT_CONFIG_NOSYSTEM` for its own scratch
  repositories.
- **Identity's content scan judges binary by content**
  (`check-xfx-identity.sh:66-83`).
  - `git grep -I` takes "binary" from the `diff` attribute, and attributes
    come from config, `.git/info/attributes` and untracked `.gitattributes`.
    Dropping the environment does not reach those.
  - So the scan runs `git grep -a --no-color -l -z` to list the files with a
    hit. It passes over one only if it has a NUL in its first 8000 bytes,
    git's rule when no attribute speaks. Then it prints that file's hits as
    before.
  - `--no-color`, because a `color.grep=always` config would colour the
    names it reads.
  - The repository tracks no `.gitattributes` (`inventory.txt`), so its own
    scan is unchanged: `ok (12111 tracked file(s) scanned)` (`gates-code.txt`).
- **The sweep:** `ci_sweep.py:103-109` (`git()`) and `sweep.py:103-117,996`
  (`git_env()`) run git with the environment minus `GIT_*`.

## GREEN

- **`scripts/test-gate-env.sh`:** 26 cases at `08f9025b`, all pass on macOS
  and Linux (`runs-*.txt`: `test-gate-env.sh at 08f9025b`).
  - The 21 git cases (`:195-338`) are one per wrong row above, plus
    `identity-color-grep`.
  - Each requires the gate's own message and an unchanged decoy (`refuses`,
    `:257-277`).
  - At `a67881dd`, the gates as at the base: 20 problems on macOS and 18 on
    Linux, where git 2.39 makes `no-secrets-common-dir` and
    `attributes-attr-source` pass anyway.
  - The test drops every inherited `GIT_*` itself (`:57-63`). That is a
    superset of the 7 `test-check-no-secrets.sh` unsets.
- **The sweep:** every row at the head is exit 1 with the violation named and
  the decoy unchanged, on both platforms (`runs-*.txt`: `the sweep at
  08f9025b`).
- **The sweep's tests:** `test_sweep.py` and `test_ci_sweep.py` each gain one
  test (`test_sweep.py:765`, `test_ci_sweep.py:293`). Both fail at `4ac0667e`
  and pass at `5d33c5df` on macOS (`py-runs-macos.txt`).
- **The sweep's tests on Linux** take two runs, because this image lacks two
  tools and one test depends on how the container runs:
  - `py-runs-linux-a.txt` runs as uid 1000 without `ruby` or `zip`. Thirteen
    `test_ci_sweep` errors there come from the missing tools, at every
    commit, the base included.
  - `py-runs-linux-b.txt` installs both and runs under `su`. One
    `test_sweep` test (`test_failed_and_timed_out_scenarios_count_in_the_denominator`)
    fails there at every commit, the base included.
  - Across the two runs, the only tests that change between the base and the
    head are the two new ones.

## Mutants

Each fix was taken out on a `--no-local` clone, after a control run that
passed. Each is caught by exactly its own cases:
- `mutants.txt` (shell, the clone at `08f9025b`): 5 of 5:
  - `no-secrets-keeps-git-env` fails only the three `no-secrets-*` git cases;
  - `identity-keeps-git-env` fails only the five `identity-*` repository
    cases;
  - `preview-keeps-git-env` fails only the four `preview-*` cases;
  - `identity-binary-by-attributes` (`-a` back to `-I`) fails only
    `attributes-info`, `attributes-local-config` and `attributes-untracked`.
    The environment ones are blocked by the `GIT_*` drop as well.
  - `identity-coloured-names` (`--no-color` dropped) fails only
    `identity-color-grep`.
- `mutants-py.txt` (python, the clone at `5d33c5df`): 2 of 2.
  `ci-sweep-keeps-git-env` fails only the selection test, and
  `sweep-out-guard-keeps-git-env` fails only the out-guard test.

## Gates

`gates-code.txt` (`gates.sh`, the worktree at `5d33c5df`), all exit 0:
- `cargo fmt --check`;
- `check-no-secrets` and `check-xfx-identity`, 12 111 files each;
- `check-no-stubs` and `check-preview-contract`;
- `check-tui-preflight-cost` and `check-usage-clock`, both built and run;
- `test-check-no-secrets`, 13 cases; `test-gate-env`, 26 cases;
  `test-preview-contract`, 24 mutants;
- `scripts/fxcap/test-sweep.sh`, 51 and 27 tests;
- `actionlint` on `ci.yml` and `sweep.yml`.

No Rust source changed (`gates-code.txt`: `what changed`), so clippy and
`cargo test` were not run.
`gates-receipt.txt` runs again the gates that read the tracked tree, at the
receipt commit, which adds only this folder.

## Files

- **Tools:** `inventory.sh`, `gitenv-sweep.sh`, `hook-probe.sh`, `runs.sh`
  (the macOS and Linux RED and GREEN), `py-runs.sh`, `mutants.py`,
  `mutants-py.py`, `gates.sh` and `publish.py`.
- **Logs:** the `.txt` files.
- **Redaction:** the logs went through `scripts/fxcap/redact.py`, so `<tmp>`
  is the gate's `TMPDIR`, `<scratch>` the session scratchpad, `<copy>` a
  container's copy, and `<local-path>` any other local path. `publish.py` also
  writes the fake key as `<fake-key>` and the retired product name as
  `<retired>`, because either one committed would be a finding of a gate that
  reads this folder.
