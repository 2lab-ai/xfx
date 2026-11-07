---
title: GATE-GITENV receipt -- no gate reads, passes or writes by git's environment
type: receipt
tags: [xfx, phase5, ci, gates, git, receipt]
---

# GATE-GITENV: the gates and git's environment

Every claim below is from a log in this folder, cited as `file: section`.
A claim with no log is marked *(not measured here)* with its source.

- **Base:** `45d99f52`, the SECRETS-ARGMAX head with origin/main merged in.
- **Round 1:** code head `5d33c5df`, receipt head `2c8fbc64`.
- **Round 2:** the tests at `692aea34` (on `2c8fbc64`), the fixes at
  `fff83c06`. Line numbers below are at `fff83c06`.

**The goal:** no environment or config a caller inherits can make a CI gate
read anything but the repository it is checking, pass a violation, or write
into any repository.

## What CI runs, and what of it runs git

`inventory.txt` lists every script the workflows run, from one-line and
block `run:` steps, and which of them run git.

- **Shell gates that run git:** `check-no-secrets.sh`,
  `check-xfx-identity.sh`, `check-preview-contract.sh`, and
  `test-check-no-secrets.sh`, which builds scratch repositories.
- **Workflow steps that run git themselves:** ten, in `ci.yml`,
  `preview.yml`, `sweep-full.yml` and `sweep.yml` (`runs-r2-*.txt`:
  `workflow-steps` at `692aea34` lists them). `release.yml` runs none.
- **Python, run by sweep.yml:** the parity sweep's `ci_sweep.py` and
  `sweep.py`, reached through `ci-sweep.sh` and `test-sweep.sh`.
- **`build.rs`**, built by every cargo step. It runs `git rev-parse`
  (`build.rs:61,64,129`), all through `git()` at `:96-112`.
- **No git calls:** `check-no-stubs.sh`, `check-tui-preflight-cost.sh`,
  `check-usage-clock.sh`, `test-preview-contract.sh`, `smoke.sh` and
  `smoke-tui.sh`. The two smoke scripts start the binary in an environment
  built from nothing (`smoke.sh:432-433`, `smoke-tui.sh:3223-3229`).
- **Hooks:** nothing in `scripts`, `.github`, `Cargo.toml` or `build.rs`
  installs or names one (`inventory.txt`: `hooks`).
- **`.gitattributes`:** the tree tracks none, at any depth
  (`inventory.txt`: `.gitattributes`).

## Round 1

### RED

`gitenv-sweep.sh` runs each git-reading gate on a fixture that holds one
violation:
- `ipath`: a tracked path carrying the retired name, for identity;
- `ilock`: the retired name inside a tracked `Cargo.lock`, for identity;
- `keyed`: a key in `b.txt`, for no-secrets;
- `readme`: a README without its install command, for preview-contract.

Each run sets one variable at a decoy repository, or puts one attribute file
in the clone. A row is correct when the gate exits 1 with its violation named
and the decoy unchanged. "Unchanged" there means every regular file under
the decoy's `.git` keeps its checksum, and none is added or removed; that
sweep does not compare directories, modes or symlinks. (Round 2's test does:
see below.)

The rows that are wrong at the base (`runs-macos.txt` and `runs-linux.txt`:
`the sweep at 45d99f52`):

| variable or file | identity (`ipath`, `ilock`) | no-secrets | preview-contract |
|---|---|---|---|
| `GIT_DIR` | **exit 0**, decoy written | exit 1, **the key not named**: no line the sweep recognises. A decoy tracking the fixture's clean files: **exit 0**, `ok (2 tracked file(s) scanned)` (`r2-base-macos.txt`) | exit 128 (macOS) or 1 (Linux), **decoy written** |
| `GIT_DIR` + `GIT_WORK_TREE` | exit 1 (a control), **decoy written** | exit 1, **the key not named**: `d.txt is tracked and cannot be read`, the decoy's listing | **decoy written** |
| `GIT_INDEX_FILE` (an index without the violation) | **exit 0** | **exit 0, `ok (2 tracked file(s))`** | exit 1 (a git error) |
| `GIT_OBJECT_DIRECTORY` | exit 1, **decoy written** | exit 1, **decoy written** | exit 128, **decoy written** |
| `GIT_COMMON_DIR` | macOS: **decoy written**; Linux: exit 1, a control | macOS: **decoy written**; Linux: correct | macOS: exit 128, **decoy written**; Linux: exit 1, a git error |
| `GIT_CONFIG_GLOBAL`, `GIT_CONFIG_SYSTEM`, `GIT_CONFIG_COUNT`, `GIT_CONFIG_PARAMETERS` → `core.attributesFile` with `*.lock -diff` | `ilock` **exit 0** | - | - |
| `GIT_ATTR_SOURCE` (a tree whose `.gitattributes` says the same) | `ilock` **exit 0** on macOS (git 2.54); correct on Linux (git 2.39) | - | - |
| `.git/info/attributes`, `.git/config core.attributesFile`, an untracked `.gitattributes` | `ilock` **exit 0** | - | - |

Round 1 called both no-secrets `GIT_DIR` cells correct. Neither was: each
exit 1 named no key (`runs-macos.txt`, `runs-linux.txt`: the `keyed GIT_DIR`
rows). Round 1's drop fixed both: they name the key in `the sweep at
08f9025b`, and round 2's `no-secrets-git-dir` refuses it at `2c8fbc64`.

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

### The design point: which index, inside a hook

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

### The fix

- **`GIT_*` dropped** before any git runs in `check-no-secrets.sh:24-46`,
  `check-xfx-identity.sh:26-46` and `check-preview-contract.sh:49-67`: every
  variable whose name starts `GIT_`. Each gate then checks the checkout its
  script is in.
- **Identity's content scan judges binary by content**
  (`check-xfx-identity.sh:92-138`).
  - `git grep -I` takes "binary" from the `diff` attribute, and attributes
    come from config, `.git/info/attributes` and untracked `.gitattributes`.
    Dropping the environment does not reach those.
  - So the scan runs `git grep -a --no-color -l -z` to list the files with a
    hit. It passes over one only if it has a NUL in its first 8000 bytes,
    git's rule when no attribute speaks. Then it prints that file's hits.
  - `--no-color`, because a `color.grep=always` config would colour the
    names it reads.
  - The tree tracks no `.gitattributes` (`inventory.txt`: `.gitattributes`),
    so its own scan is unchanged: `ok (12131 tracked file(s) scanned)`
    (`gates-code.txt`).
- **The sweep:** `ci_sweep.py:103-109` (`git()`) and `sweep.py:103-117,996`
  (`git_env()`) run git with the environment minus `GIT_*`.

### GREEN and mutants

- `test-gate-env.sh` at `08f9025b`: 26 cases, all pass on macOS and Linux
  (`runs-*.txt`). At `a67881dd`, the gates as at the base: 20 problems on
  macOS, 18 on Linux, where git 2.39 makes `no-secrets-common-dir` and
  `attributes-attr-source` pass anyway.
- The sweep at the head: every row exit 1 with the violation named and the
  decoy unchanged, both platforms (`runs-*.txt`: `the sweep at 08f9025b`).
- `test_sweep.py` and `test_ci_sweep.py` each gained one test. Both fail at
  `4ac0667e` and pass at `5d33c5df` on macOS (`py-runs-macos.txt`). On Linux
  (`py-runs-linux-a.txt`, `-b.txt`), the only tests that change between the
  base and the head are those two; the image's missing `ruby`/`zip` and one
  container-dependent test fail at every commit, the base included.
- Mutants, each fix taken out on a `--no-local` clone: 5 of 5 shell
  (`mutants.txt`) and 2 of 2 python (`mutants-py.txt`), each caught by
  exactly its own cases.

## Round 2

### RED

Two sources. `r2-probes.sh` runs each entry point on a fixture, printing the
exit status, what it said, and what ran or was written; at the base
(`r2-base-macos.txt`) and at `2c8fbc64` (`runs-r2-*.txt`: `r2-probes.sh at
2c8fbc64`). And `test-gate-env.sh` at `692aea34`, the new cases against the
gates at `2c8fbc64` (`runs-r2-*.txt`: `test-gate-env.sh at 692aea34`).

At `2c8fbc64`, on both platforms unless a row says otherwise:

| entry point | under | result |
|---|---|---|
| ci.yml's checkout assertion | `GIT_DIR` at a decoy whose HEAD is the commit expected, `GIT_TRACE=<file>` | **exit 0**, `gate is running on <the decoy's HEAD>`, trace **written** |
| `build.rs`, compiled alone | `GIT_DIR` at a decoy | stamps **the decoy's HEAD** |
| | `GIT_TRACE=<file>` | trace **written** |
| `test-check-no-secrets.sh` | `GIT_TRACE=<file>`, a HOME config naming an fsmonitor | trace **written**, the fsmonitor **ran** |
| identity | `.git/config` `grep.patternType=bogus`, `grep.extendedRegexp=notabool`, `grep.threads=x` | **exit 0**: git's `fatal:` is the only sign |
| | `Cargo.lock` (the residue) mode 000 | **exit 0** after `error: failed to stat` |
| | `.git/config core.worktree` naming a clean tree with the same names | **exit 0** |
| | `Cargo.lock` marked skip-worktree, or assume-unchanged, with a working copy that has a NUL | **exit 0**: git grep passes a skip-worktree entry over, and reads an assume-unchanged one from the index while the probe reads the working copy |
| | `.git/index` cut to 4 bytes | exit 128 |
| | HOME config `core.quotePath=notabool` | exit 128 |
| | `.git/config core.quotePath=notabool` | exit 1, `not a git checkout`, git's reason dropped |
| identity, no-secrets | a HOME config naming an fsmonitor and a hooks path | refused, but the fsmonitor **ran** |
| ten workflow steps | - | run git with no `GIT_*` drop |

`test-gate-env.sh` at `692aea34` reports 16 problems on macOS
(`runs-r2-macos.txt`), one per row above, and ends `16 problem(s), 29
case(s) passed`. Linux reports the same 16 (`runs-r2-linux.txt`).

Rows the reviewers did not name: the index bits, found while documenting
the divergence they asked about; the corrupt index and the two
`core.quotePath` rows, added to reach each listing failure. The
`core.quotePath` row in `.git/config` is a RED by message only: exit 1 at
both commits.

**Not reproduced** (`r2-probes.sh at 2c8fbc64`), so not changed:
- `check-no-secrets.sh` under a checkout-local `core.worktree` naming a
  clean tree with the same names refuses the key (the last section).
- `check-preview-contract.sh` under the fsmonitor HOME: nothing ran. Its
  scratch git pinned global and system config off.

**`test-gate-env.sh` itself**, under that HOME, ran the fsmonitor and three
hooks from its decoy commits at `2c8fbc64` (`r2-probes.sh at 2c8fbc64`:
`test-gate-env.sh, HOME=<that config>`). Fixed below; a test cannot hold
itself to this, so the same probe row at `fff83c06` is the evidence.

### The fix

- **Every entry point drops `GIT_*`:**
  - the ten workflow steps, one line before their first git
    (`ci.yml:75-76`, and the same line in each step `workflow-steps`
    lists);
  - `build.rs:96-112`: `git()` removes every inherited `GIT_*` from the
    command it builds;
  - `test-check-no-secrets.sh:55-60`, the same loop as the gates, in place
    of seven names.
- **The drop is the shell's own list**, `for name in "${!GIT_@}"`, in every
  script and step, for the reviewers' finding that `$(compgen -e)` fails
  open *(not measured here)*: `compgen` is missing from a bash built without
  readline, and a substitution that fails in a `for` list does not stop
  `set -e`. A name that is no identifier is skipped: bash 3.2 lists one, and
  git reads none.
- **Identity fails closed** (`check-xfx-identity.sh:92-156`):
  - each `git grep` writes to a file, and a status above 1 or anything on
    its standard error is a failed scan;
  - a probe of a listed file's first bytes that fails is a failed scan;
  - `git ls-files` likewise, in `tracked_paths` and in the listing at
    `:241-245`; `rev-parse` failing prints git's reason (`:228-232`);
  - each caller turns a failed scan into a failure: `could not run in its
    own control`, or `could not finish, so it found nothing it can vouch
    for`.
  No case reaches the probe failure once the index bits are refused (below).
  The way left is a race, a file removed between the listing and the probe
  (from the code paths, not measured), so no case pins it.
- **Index bits refused** (`check-xfx-identity.sh:234-258`): an entry marked
  skip-worktree or assume-unchanged fails the scan, by name. No CI checkout
  sets either.
- **Config the drop leaves:**
  - identity's git runs with `--work-tree=.` and `core.fsmonitor=false`
    (`check-xfx-identity.sh:71-84`), keeping global config, so
    `safe.directory` still works;
  - no-secrets' git in the checkout pins `core.fsmonitor=false`
    (`check-no-secrets.sh:95-102`);
  - the scratch repositories in identity (`:86-90`), no-secrets (`:399-404`),
    `test-check-no-secrets.sh` (`:61-63`) and `test-gate-env.sh` (`:75-77`)
    run with no global or system config and no fsmonitor.
- **The epoch gate's pins, kept** (`check-preview-contract.sh:1083-1090,
  1105`). The workflow drop removed the `GIT_CONFIG_*` the contract check
  handed the step, and its `git diff` ran a HOME fsmonitor
  (`regression-epoch-gate.txt`: `preview-home-programs`). A `git` first on
  the step's PATH sets them again after the drop.
- `test_sweep.py:772-782`: skips outside a git checkout instead of failing,
  and its decoy `git init` runs without global config.

### GREEN

`test-gate-env.sh` at `fff83c06`: 45 cases, all pass on macOS and Linux
(`runs-r2-*.txt`: `test-gate-env.sh at fff83c06`).
- New: `identity-trace`, `no-secrets-git-dir` (the decoy tracks the
  fixture's clean files), the identity failure cases (`:434-480`), the
  three `*-home-programs` (`:482-486`), `workflow-steps` (`:488-515`),
  `ci-checkout-assertion` (`:517-534`), `build-script` (`:536-565`) and
  `test-check-no-secrets` (`:567-585`).
- Every case also fails if a program from config ran or a `GIT_TRACE` file
  was written (`nothing_ran`, `:317-332`).
- The decoy comparison (`decoy_sums`, `:305-315`) now covers every entry's
  type, every regular file's checksum and execute bit, and every symlink's
  target. That shows no lasting write; a write undone before the case ends
  is not seen.

`r2-probes.sh at fff83c06` (`runs-r2-*.txt`): every row refused with the
gate's own message, the revision is the checkout's, nothing ran and no trace
was written, `test-gate-env.sh` under the fsmonitor HOME included.

### Mutants

`mutants-r2.txt` (Linux, a `--no-local` clone at `fff83c06`, after a control
run that passed): 15 of 15 caught, each by exactly its own cases.
- MUST 1: `build-rs-keeps-git-env` → `build-script`;
  `ci-assertion-keeps-git-env` → `ci-checkout-assertion`, `workflow-steps`;
  `sweep-build-step-keeps-git-env` → `workflow-steps`;
  `test-check-no-secrets-seven-names` and `-reads-home` →
  `test-check-no-secrets`.
- MUST 2 and the index bits: `identity-listing-ignores-errors` → the three
  `grep.*` cases and `identity-unreadable`; `identity-names-ignore-errors` →
  `identity-core.quotePath-home`; `identity-count-ignores-errors` →
  `identity-index-corrupt`; `identity-reads-flagged-entries` →
  `identity-skip-worktree`, `identity-assume-unchanged`.
- MUST 3: `identity-reads-config-worktree` → `identity-core.worktree`;
  `identity-scan-runs-fsmonitor` → `identity-home-programs`;
  `identity-controls-read-home` → `identity-home-programs`,
  `identity-core.quotePath-home`; `no-secrets-listing-runs-fsmonitor` and
  `no-secrets-sample-reads-home` → `no-secrets-home-programs`,
  `test-check-no-secrets`; `preview-epoch-gate-reads-home` →
  `preview-home-programs`.

`mutants-r2-first.txt` is the first run. Three expected sets there were too
narrow: those three mutants failed the test, and each failed one case more
than its set named. `test-check-no-secrets` runs check-no-secrets.sh under
the same HOME. And the scratch pin is what lets identity's controls start
under a HOME config git cannot parse: without it their `init` exits 128.
The sets were widened, with those reasons in `mutants-r2.py`, and the whole
run repeated.

## Gates

`gates-code.txt` (`gates.sh`, the worktree at `fff83c06`), all exit 0:
- `cargo fmt --check`, `cargo build --locked`, and `cargo clippy --locked
  --all-targets -- -D warnings` (build.rs changed);
- `check-no-secrets` and `check-xfx-identity`, 12 131 files each;
  `check-no-stubs`, `check-preview-contract`, `check-tui-preflight-cost`,
  `check-usage-clock`;
- `test-check-no-secrets`, 13 cases; `test-gate-env`, 45 cases;
  `test-preview-contract`, 24 mutants;
- `scripts/fxcap/test-sweep.sh`, 51 and 27 tests, none skipped;
- `actionlint` over all five workflows.

`gates-receipt.txt` runs again the gates that read the tracked tree, at the
receipt commit, which adds only this folder.

## Files

- **Tools:** `inventory.sh`, `gitenv-sweep.sh`, `hook-probe.sh`, `runs.sh`,
  `py-runs.sh`, `mutants.py`, `mutants-py.py` (round 1); `r2-probes.sh`,
  `runs-r2.sh`, `mutants-r2.py` (round 2); `gates.sh` and `publish.py`.
- **Logs:** the `.txt` files. Round 2 rewrote `inventory.txt` and
  `gates-code.txt` at `fff83c06`; round 1's are in git at `2c8fbc64`.
  `r2-base-macos.txt` ran an earlier `r2-probes.sh`, before its index-bit,
  corrupt-index and no-secrets `core.worktree` rows.
- **Redaction:** the logs went through `scripts/fxcap/redact.py`, so `<tmp>`
  is the gate's `TMPDIR`, `<scratch>` the session scratchpad, `<copy>` a
  container's copy, and `<local-path>` any other local path. `publish.py` also
  writes the fake key as `<fake-key>` and the retired product name as
  `<retired>`, because either one committed would be a finding of a gate that
  reads this folder.
