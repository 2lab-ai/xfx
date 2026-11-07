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
  `fff83c06`. Line numbers in the round 2 sections are at `fff83c06`.
- **Round 2b:** the tests at `582007b0` (on `d6760929`), the fixes at
  `4a95372f`. Line numbers in the round 2b section are at `4a95372f`.
- **Round 2c:** the tests at `c00d2bb3` (on `625abcf1`), the fix at
  `16c115c1`. Line numbers in the round 2c section are at `16c115c1`.

**The goal:** no environment or config a caller inherits can make a CI gate
read anything but the repository it is checking, pass a violation, or write
into any repository.

## What CI runs, and what of it runs git

`inventory.txt` lists every script the workflows run, from one-line and
block `run:` steps, and which of them run git.

- **Shell gates that run git:** `check-no-secrets.sh`,
  `check-xfx-identity.sh`, `check-preview-contract.sh`, and
  `test-check-no-secrets.sh`, which builds scratch repositories. Since 2b,
  CI runs that last one only inside `test-gate-env.sh` (below).
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

## Round 2b

Three items from the round 2 reviews: a vendored copy, the sparse-checkout
remedy, and the secrets harness running twice per CI row. Plus a linked
worktree case each for the identity scan and build.rs.

### A copy inside another repository

`vendored-probe.sh` puts the identity script in `vendor/xfx/scripts/` of a
parent repository that tracks `vendor/xfx/Cargo.lock` with the residue; the
copy has no `.git` of its own. As controls, the same files in a repository of
their own, and in a linked worktree of that (`.git` a file). Rows from
`runs-r2b-macos.txt` and `runs-r2b-linux.txt` (`the identity script at
...`), the same on both:

| identity script at | vendored | own repository | linked worktree |
|---|---|---|---|
| `45d99f52` (the unit base) | exit 1, the residue named | exit 1 | exit 1 |
| `2c8fbc64` (round 1) | exit 1, the residue named | exit 1 | exit 1 |
| `d6760929` (round 2) | **exit 0, `ok (3 tracked file(s) scanned)`** | exit 1 | exit 1 |
| `4a95372f` | exit 1, `not a git checkout of its own` | exit 1 | exit 1 |

Round 2's `--work-tree=.` made it: git found the parent's repository, its
paths resolved inside `vendor/xfx`, where none exists, and `git grep`
skipped each one. The 2b fix (`check-xfx-identity.sh:228-239` at
`4a95372f`): the discovery call for the checkout -- the first git call on
it; the controls run git earlier, on their own repositories -- ran with
`GIT_CEILING_DIRECTORIES` at the parent of the script's root. That held for
the layouts in this table: the copy reached by its own path is refused by
name, and a `.git` directory or file at the root still passes. It did not
hold for a root reached through a symlink, a `:` in the parent's path, or
a path in another case: round 2c below replaces it.

Cases (`test-gate-env.sh:492-510`): `identity-vendored` requires that
refusal, and `identity-linked-worktree` requires a linked worktree still to
be scanned and its residue named. A `[ -d .git ]` check would refuse the
second: the mutant `identity-requires-a-git-directory` is that check.

### The sparse-checkout remedy

The index-bit refusal now names a remedy per bit
(`check-xfx-identity.sh:241-276`). For skip-worktree, `git sparse-checkout
disable` first, and `git update-index --no-skip-worktree` only for a file the
working tree holds: on a file it lacks, clearing the bit leaves a deleted
tracked file, which the scan skips. For assume-unchanged, `git update-index
--no-assume-unchanged`. `identity-skip-worktree` now requires `git
sparse-checkout disable` in the refusal, and `identity-assume-unchanged` its
own bit's line (`test-gate-env.sh:479-491`).

### build.rs in a linked worktree

`build-script-linked-worktree` (`test-gate-env.sh:605-626`) adds a linked
worktree to the scratch checkout, commits there, and runs the compiled
build script in it under the decoy `GIT_DIR`, a `GIT_TRACE` and the HOME
that runs programs. It requires the worktree's HEAD stamped, the decoy
unchanged, nothing run, and `cargo:rerun-if-changed` lines for the
worktree's own HEAD (`.git/worktrees/<name>/HEAD`) and its branch's ref in
the main repository. That is what build.rs tells Cargo to watch; no Cargo
rebuild was run, so a rebuild after a commit in a linked worktree is not
measured here. The mutant `build-rs-watches-the-main-head` (HEAD taken from
the common directory) fails it.

### One full secrets-harness run per CI row

`ci.yml` no longer runs `test-check-no-secrets.sh` as a step of its own
(`ci.yml:127-144`). CI's one run of it is the nested one in
`test-gate-env.sh` (`:630-654`). That run happens under the decoy
`GIT_DIR`, a `GIT_TRACE` and the HOME that runs programs. It requires exit
0, the decoy unchanged, nothing run and no trace written, and on any failure
it now prints the whole log of the nested run. README and CONTRIBUTING list
it under `test-gate-env.sh`.

Why nothing is lost: the two runs differ only in their environment.
`test-check-no-secrets.sh` drops every `GIT_*` first and reads no `HOME`.
It builds each repository with no global or system config (`sg`). The one
thing left to the caller's config is the fixture listing of
`check-no-secrets.sh`, whose git pins `core.fsmonitor` off. So the
standalone run differed only in reading the runner's global git config
instead of the test's, and its config is not what that test is about. The
behavioural assertions stay. Both secrets-harness mutants
(`test-check-no-secrets-seven-names`, `-reads-home`) and the two no-secrets
config mutants are still caught by the nested case (`mutants-r2b.txt`).

**Time per row** (`row-time.txt`). "Before" is the two steps at
`d6760929`, "after" is `test-gate-env.sh` alone at `4a95372f`, alternated:

| where | before | after | the step removed |
|---|---|---|---|
| Linux container (aarch64) | 93 s, 106 s | 54 s, 62 s | 41 s, 43 s |
| macOS (arm64), beside other sessions | 296 s, 496 s, 594 s | 288 s, 486 s, 265 s | 68 s, 85 s, 94 s |

On Linux the row saves the removed step's time, and the three new cases
cost a few seconds. The macOS totals moved with the machine's load (load
average 6 to 9 from other work) more than with the change. What they do
show is the step removed: 68 to 94 s. No CI runner was measured, the Intel
one included.

### RED, GREEN and mutants

- `test-gate-env.sh` case counts: 5 at the unit base `45d99f52`, 45 at
  `d6760929`, 48 at `4a95372f`, all passing on macOS and Linux
  (`runs-r2b-*.txt`).
- At `582007b0`, the new cases against the gates at `d6760929`: 3 problems
  on both platforms, `3 problem(s), 45 case(s) passed`. `identity-vendored`
  exits 0. `identity-skip-worktree` and `identity-assume-unchanged` fail by
  message. The linked worktree cases pass there: they hold the fix to not
  refusing a linked worktree, and build.rs already handled one.
- `mutants-r2b.txt` (Linux, a `--no-local` clone at `4a95372f`, after a
  control run that passed): 19 of 19 caught, each by exactly its own
  cases. The 15 of round 2, with `build-rs-keeps-git-env` now also failing
  `build-script-linked-worktree`, and `identity-reads-flagged-entries`
  editing the refusal's new condition. And four new ones:
  - `identity-walks-up-to-a-parent` (no ceiling) → `identity-vendored`;
  - `identity-requires-a-git-directory` (`[ -d .git ]`) →
    `identity-linked-worktree`;
  - `identity-sparse-remedy-clears-the-bit` (the old remedy) →
    `identity-skip-worktree`;
  - `build-rs-watches-the-main-head` → `build-script-linked-worktree`.

## Round 2c

Both round 2b reviews found the same leftover: the ceiling is compared by
path, and git's path is not the script's. `discovery-probe.sh` runs the
identity script on the vendored copy reached four ways, on an own checkout
reached four ways, and on an entry with both index bits
(`runs-r2c-*.txt`: `the identity script at ...`).

| at `625abcf1` | macOS | Linux |
|---|---|---|
| vendored, by its own path | exit 1, refused | exit 1, refused |
| vendored, through a symlink to its root | **exit 0** | **exit 0** |
| vendored, a `:` in the parent's path | **exit 0** | **exit 0** |
| vendored, by a path in another case | **exit 0** | no such path (the file system does not fold case) |
| own checkout, by its path, a symlink, another case, a linked worktree | exit 1, the residue named | the same, without the case row |
| both index bits (`ls-files -v` tag `s`) | only the skip-worktree remedy | the same |

Why each one passed (git's handling of the ceiling from its documentation
and the reviewers' reading of its source; the rows are what was measured):
- Git compares the ceiling with its physical working directory. Through a
  symlink to the root, the ceiling is the link's parent, which lies above
  the parent repository, so the walk reaches that repository first.
- With a path in another case, the ceiling has the case the path was given
  in. Git's working directory comes from `getcwd`, which has the disk's
  case (the probe's last line). The exit 0 row shows that the two did not
  match.
- `GIT_CEILING_DIRECTORIES` is colon-separated, with no escaping.

**Why not `pwd -P`.** It fixes the symlink row, but not the case row:
bash's `pwd -P` keeps the case it was given, while the system's `getcwd`
gives the disk's (`discovery-probe.sh`'s last two lines, macOS). And no
spelling of the path fixes the `:` row.

**The fix** (`check-xfx-identity.sh:228-250`): no ceiling, and a check
that does not compare paths. Git discovers the repository from the root
(`rev-parse --absolute-git-dir`). That repository must be the same file
(`-ef`: device and inode) as the one the root's own `.git` names
(`git --git-dir=<root>/.git rev-parse --absolute-git-dir`). A linked
worktree's or submodule's `.git` file names its own repository. A copy with
no `.git` fails the second command and is refused by name. The refusal
prints the repository git found.

An entry with both bits is now listed under both refusals (`:275-279`).
Setting the two bits takes two `update-index` calls: one call with both
options set only the last (`identity-both-bits`'s comment).

| at `16c115c1` | macOS | Linux |
|---|---|---|
| vendored, every row | exit 1, `not a git checkout of its own` | the same, without the case row |
| own checkout, every row | exit 1, the residue named | the same |
| both index bits | both remedies | both remedies |

Cases (`test-gate-env.sh:498-551`):
- `identity-vendored-symlink`, `identity-vendored-colon` and
  `identity-vendored-miscased` require the refusal.
- `identity-own-symlink` and `identity-own-miscased` require an own
  checkout reached those ways still to be scanned.
- `identity-both-bits` requires `git update-index --no-assume-unchanged` in
  the refusal.
- The two `-miscased` cases need a file system that folds case. Elsewhere
  they print `skip` and are not counted.

`test-gate-env.sh` (`runs-r2c-*.txt`):

| at | macOS | Linux |
|---|---|---|
| `625abcf1` | 48 pass | 48 pass |
| `c00d2bb3` (the new cases, the gates at `625abcf1`) | `4 problem(s), 50 case(s) passed`: both-bits, symlink, colon, miscased | `3 problem(s), 49 case(s) passed`, the two miscased skipped |
| `16c115c1` | 54 pass | 52 pass, the two miscased skipped |

The `identity-own-*` cases pass at `625abcf1` too: they hold the fix to
scanning an own checkout however it is reached.

Mutants, each on a `--no-local` clone at `16c115c1`, after a control run
that passed:
- `mutants-r2c-linux.txt` (Linux): 22 mutants. 21 ran: the miscased cases
  skip there, so `identity-own-check-by-path`, which only they catch, is
  skipped too. 20 caught, 1 OTHER.
  - The OTHER was `identity-reads-flagged-entries`. Taking the refusal out
    also failed `identity-both-bits`, which its expected set did not name.
  - With the set widened (the reason is in `mutants-r2c.py`), that mutant
    alone was run again: caught (`mutants-r2c-linux-rerun.txt`).
- `mutants-r2c-macos.txt` (macOS, case folded): 3 of 3, the ones the
  miscased cases decide:
  - `identity-no-own-check` → the four `identity-vendored*` cases;
  - `identity-ceiling-instead` (2b's fix put back) → symlink, colon and
    miscased, but not `identity-vendored`, which 2b's fix did hold;
  - `identity-own-check-by-path` (`!=` on the two paths in place of `-ef`)
    → `identity-own-miscased`: compared as paths, an own checkout reached
    in another case would be refused.
- The new mutants beside those: `identity-both-bits-one-list` →
  `identity-both-bits`. `identity-walks-up-to-a-parent` (2b's) is gone with
  the ceiling it took out.

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

Round 2b, `gates-code-r2b.txt` (the worktree at `4a95372f`), all exit 0: the
same list; `check-no-secrets` and `check-xfx-identity` over 12 140 files,
`test-gate-env` 48 cases, `test-preview-contract` 24 mutants, `test-sweep.sh`
51 and 27 tests, actionlint over five workflows. `test-check-no-secrets` is
still run there on its own, as a local gate. `gates-receipt-r2b.txt` is the
tree-reading gates at the 2b receipt commit.

Round 2c, the four tree-reading gates (`check-no-secrets`,
`check-xfx-identity`, `check-no-stubs`, `check-preview-contract`): exit 0 at
`16c115c1` (`gates-code-r2c.txt`) and at the receipt commit
(`gates-receipt-r2c.txt`). `test-gate-env.sh` at `16c115c1` is in
`runs-r2c-*.txt`.

## Files

- **Tools:** `inventory.sh`, `gitenv-sweep.sh`, `hook-probe.sh`, `runs.sh`,
  `py-runs.sh`, `mutants.py`, `mutants-py.py` (round 1); `r2-probes.sh`,
  `runs-r2.sh`, `mutants-r2.py` (round 2); `vendored-probe.sh`,
  `runs-r2b.sh`, `mutants-r2b.py`, `row-time.sh`, `row-time-rounds.sh`
  (round 2b); `discovery-probe.sh`, `runs-r2c.sh`, `mutants-r2c.py` (round
  2c); `gates.sh` and `publish.py`.
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
