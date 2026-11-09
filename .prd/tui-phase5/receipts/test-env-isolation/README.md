---
title: TEST-ENV-ISOLATION receipt -- a unit test cannot read the process environment
type: receipt
tags: [xfx, phase5, tests, security, environment, receipt]
---

# TEST-ENV-ISOLATION

Every claim below cites a file in this folder. Line numbers are at the base
`32afd307` unless they say head.

- **Base:** origin/main `32afd307`.
- **RED:** `0d7e89f2` (the base plus `src/test_env/tests.rs`'s first eight
  cases, and nothing else).
- **Fix:** `e54b8f45`, with `e1a127a4` (a child's names read from `env` and
  not through a shell) and `8db95597` (a doc comment in the test-only
  `src/test_env.rs`). The receipt commit follows them.

**Why:** a CATALOG-REFRESH case built its runtime with `Runtime::new`, whose
environment is `Environment::from_process()`; a reload then read the real
`~/.xfx/settings.json` and the process's xfx variables (2026-11-09). This unit
makes that a failing test instead of a brief's rule.

## How every claim here was measured without the real environment

`sentinel-run.sh` runs a copy of the lib test binary under `env -i` with
`PATH`, `TMPDIR`, a temp `HOME` holding a `.gitconfig` and an XDG git
`config` that each define a hook, a profile, and xfx and credential variables
set to `xfx-sentinel-*` values. So "the real process environment" a case
could reach in these runs is that one. No run here read, printed or copied
the developer's home, profile, variables or git configuration.

## 1. The audit

`audit-static.sh` lists, from code at a revision, every `std::env` read in
`src` and every caller of each reader: `audit-static-base.txt` (base),
`audit-static-head.txt` (head). The dynamic half -- which cases reach each
reader -- is the guard itself: `discovery-guard.txt` is the whole lib suite
with the guard in and no case converted, and `discovery-round2.txt` the
modules it hit, rerun after the first conversions (a runtime built with
`with_environment` then reached the readers the first panic had hidden).

**Readers of the process that name the home, the profile, an xfx variable, a
credential or git's configuration** -- 10 read sites in 8 functions, plus
`with_environment`'s test-only fallback (the two sites that only hand a value
to a child are in the next list):

| Reader (base) | Reads | Cases that reached it |
|---|---|---|
| `config::Environment::from_process` (`config.rs:800`, reads `:818`, `:822`) | `HOME`, the `XFX_*` knobs, `AI_GATEWAY_API_KEY`, `VERCEL_OIDC_TOKEN`, `LLMUX_CONFIG`, `XDG_CONFIG_HOME` | 120: 118 `tui::worker` (every `Runtime::new`, 33 call sites), 1 `tui::event_loop` (through `worker::recording`), 1 `app` (`load_config`). `RuntimeConfig::load` (`config.rs:994`) has no caller in `src`. |
| `gateway::Endpoint::from_process` (`gateway/mod.rs:357`) | `XFX_GATEWAY_URL` | 82: `tui::worker` (`usage_question` at launch, `title_asker`, `Bundle::select`), 2 `tui::shell` and 1 `credits` (`credits::question`), 1 `provider::model` (`catalog_for` with a key and no test catalog) |
| `permission::LinkBase::from_environment` (`links.rs:144`) | `XDG_CONFIG_HOME`, `ZDOTDIR`, `HOME` | 15 `tui::worker` (every turn, `worker.rs:6288`) |
| `usage::clock::now_ms` (`usage/clock.rs:56`) | `XFX_E2E_USAGE_CLOCK_FILE`, then the file it names | 8: `tui::shell` (a test shell's `/usage` clock), `tui::usage_menu`, `usage::clock` |
| `mcp::McpRuntime::new`'s lookup (`mcp/runtime.rs:799`) | any `${VAR}`, `header_env`, `bearer_token_env` a config names -- a credential included | 1 `mcp::config` |
| `mcp::McpRuntime::with_environment` (`mcp/runtime.rs:806`, test-only) | fell back to the real process for any name not given (`:817`) | 3 `mcp::command` |
| `permission::GitEnvironment::from_process` (`gitconfig.rs:90`) | `GIT_CONFIG_GLOBAL`, `GIT_CONFIG_NOSYSTEM`, `GIT_CONFIG_SYSTEM`, `XDG_CONFIG_HOME` | none by panic: its only callers are the turn path (`worker.rs:6289`) and the two shells' launch (`app.rs:548`, `interactive.rs:1890`), which the cases above stopped before |
| `tui::should_run` (`tui/mod.rs:65`), `tui::hold` (`:399`) | `XFX_TUI`, `XFX_THEME` | none |

Distinct cases that reached at least one: **158** (71 of them two or more),
counted from `discovery-guard.txt` and `discovery-round2.txt`.

**Values handed on to a child, and children that inherit the process:**

- `authority::minimal_environment`'s `HOME` (`authority.rs:1469`) is the
  `HOME` every planned command runs with, and the one
  `gitconfig::configured_hooks` asks git under when a `git` command is
  planned (`authority.rs:1195`). So every case that plans a `git` read the
  developer's global git configuration -- the file with the `insteadOf`
  token. `discovery-base.txt`: under the sentinel `.gitconfig`, which
  defines a hook, **9 `permission::authority` cases fail on the base**, each
  one a git plan the sentinel's configuration decided (and the RED case
  `a_git_plan_reads_no_git_configuration_under_the_process_home`). The tenth
  non-RED failure there is the RED commit's own fixture (a bare
  `Command::new` the process scan refuses), fixed in `e54b8f45`.
- `mcp::transport::StdioTransport::spawn` hands a server `HOME`, `LOGNAME`,
  `PATH`, `SHELL`, `TERM` and `USER` from the process (`transport.rs:372`).
- `process::command` (`process.rs:198`), the one way xfx makes a child,
  inherits everything but the credential: `tui::file_index`'s
  `git ls-files` (`file_index.rs:403`) and every other child a case starts
  saw the real `HOME`, `XDG_CONFIG_HOME` and `GIT_CONFIG_*`.

**Read, and not in scope (each kept, listed by the scan, with its reason in
`src/test_env/tests.rs`'s `LISTED`):** `PATH` (finding programs), `TMPDIR`,
`TEMP`, `TMP` (where `/trace` and a cidfile go), `TMUX`, `COLORFGBG`,
`COLORTERM`, `TERM_PROGRAM` (the terminal's own), the fault-injection points,
and the roles re-run test binaries are started with. None names the person,
a profile, a configuration, an xfx setting or a credential. No `dirs`-style
crate is a dependency and nothing calls `getpwuid` or `home_dir`
(`audit-static-base.txt`, first section).

## 2. The guard

Three parts. The `who`-naming panic is the default; the other two are the
explicit ways in.

1. **`process_env!` (`src/lib.rs`), at every reader above.** In a build its
   arms are `::std::env::var` and `::std::env::var_os` themselves. In this
   crate's unit tests they call `crate::test_env`, which, outside a fake
   process, **panics naming the function and the variable, never a value**:
   `config::Environment::from_process read the real process environment
   (`XFX_MODEL`) in a unit test. Give the case its own: ...`
   (`discovery-guard.txt`, `## by case`, and its panics). A panic and not a
   quietly empty environment, because empty makes a case that reached the
   process pass green, which is the bug class.
2. **`test_env::FakeProcess` -- the opt-in.** `run(body)` / `enter()` make a
   home and variables of the case's own the process of this thread;
   `spawn_unsignalled` carries it into every TUI thread
   (`test_env::carried`), and an MCP lookup captures the one it was made in
   (`test_env::lookup`). The cases that test a reader itself, and the two
   that launch the TUI's worker, use it.
3. **`Runtime::with_environment` makes its `env` the thread's process**
   (`#[cfg(test)]`, `worker.rs`, head). A runtime a case builds from an
   explicit environment reads that environment wherever it would read the
   process -- the Gateway's address, git's selectors, the link bases -- and an
   environment with no home and no variables is the fixed-empty case. This is
   what converted 118 worker cases at once: `Runtime::new` became `a_runtime`,
   `with_environment` over the home the case's own config was loaded from.

**Children** do not panic: a value only handed to a child is the fake's or
nothing (`process_env!`'s `child_var`/`child_var_os`), and a child of
`crate::process::command` in a unit test starts from the fake process, or
from only `PATH` and `TMPDIR` (`test_env::isolate_child`). Nothing of the real
process reaches the case or its child either way, and a child that needed
what it was not given fails on its own.

**What keeps it closed:** `every_other_read_of_the_environment_in_src_is_listed`
scans `src` for `env::var(`, `var_os(`, `vars(`, `vars_os(`, `home_dir(` and
`getenv(` at a path-segment start; a read outside `process_env!` must be on
its list with a reason, or the case fails.

**Production builds are unchanged** (`release-compare.txt`): base and head
(`e1a127a4`; `8db95597` touches only a `#[cfg(test)]` module's comment)
release binaries, built in a throwaway clone with `XFX_BUILD_REVISION` pinned,
are the same size (11 668 096 bytes) with identical section tables. Of the
466 bytes that differ, 132 are panic `Location` line numbers in
`__DATA_CONST,__const` that moved because a `#[cfg(test)]` line was added
above them (`worker.rs` +5, `mcp/transport.rs` +3, `mcp/runtime.rs` +6,
`tui/shell.rs` +2; 0 bytes unexplained), 16 are `LC_UUID`, and 318 are the
code signature's page hashes. `__TEXT,__text` is byte-identical.

## 3. Integration tests

`tests/*.rs` start the real binary, which `cfg(test)` does not reach. On the
base each suite removed a remembered list of names from an inherited
environment, and the lists differed (`audit-static-base.txt`, last
section): `tests/support/sandbox.rs` kept `XDG_CONFIG_HOME`, `LLMUX_CONFIG`,
`GIT_CONFIG_*` and `XFX_SKILL_SYMLINK_AUTHORITIES`; `cli.rs`, `permissions.rs`,
`sessions.rs` and `tool_loop.rs` also kept `XFX_GATEWAY_URL` or `XFX_SOUND`.
Two runs inherited nearly everything:

- `parity.rs`'s `every_doctor_check_the_binary_emits_is_documented` ran
  `xfx doctor --json` with the **developer's own `HOME`** (it removed only
  the two credentials), so it read the real profile. Now it goes through
  `run_xfx` (`env_clear`, temp `HOME`, `PATH`).
- `tui.rs`'s `session last` re-read set the sandbox's `HOME` and inherited
  every xfx variable. Now `sandbox.command()`.

At head, `tests/support/sandbox.rs`'s `clear_env` keeps only `PATH` and
`TMPDIR` (`INHERITED_VARS`), and every harness that starts xfx builds on it:
the shared `Sandbox` (used by `interactive`, `tui`, `mcp`, `ledger_compat`,
`session_compat`), and the own sandboxes of `gateway`, `llmux`,
`permissions`, `sessions` and `tool_loop`; `cli.rs`, which does not include
`support`, does the same inline. `parity.rs`'s `run_xfx` already did.

Children that do not start xfx, each safe as it stands
(`audit-static-head.txt`, last section):

- `permissions.rs`'s `git` fixture: `GIT_CONFIG_GLOBAL=/dev/null` and
  `GIT_CONFIG_SYSTEM=/dev/null`, so no global or XDG configuration is read;
  the two git runs at `:2652` and `:2817` are `env_clear` with a temp `HOME`.
- `parity.rs` runs `scripts/check-xfx-identity.sh`, which unsets every
  `GIT_*` and runs git with `GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1`
  (`check-xfx-identity.sh:44`, `:89`).
- `/bin/sh -c` with fixed scripts (`stty`, `echo`, `read`, `sleep`) in
  `interactive.rs` and `tui.rs`: a non-interactive `sh -c` reads no startup
  file; `cp`, `ps`, `kill`, `mkfifo`; the fake `osascript` in
  `fake_clipboard.rs`, a script in a temp directory.
- No integration test calls a reader in its own process: the only `std::env`
  reads in `tests/` are `PATH` and cargo's `CARGO_PKG_NAME`.

## 4. RED, then GREEN

**RED** (`red-base.txt`): the eight cases at `0d7e89f2`, under
`sentinel-run.sh` -- 0 passed, 8 failed. Six readers returned instead of
refusing (`... read the real process environment in a unit test`); the git
plan was decided by the sentinel home's hook (`configured hooks: true`); and
a child inherited six more variables than `PATH` and `TMPDIR`, `HOME` among
them.

**GREEN** (`green-head.txt`): the whole lib suite at `8db95597`, under
`sentinel-run.sh` -- 4635 passed, 0 failed, the eight RED cases among them,
and no case failed with a refusal. `gates.txt` is the gate, run plain.

## 5. Mutants

`mutants.py` on a `git clone --no-local` throwaway at `8db95597`, each run
through `sentinel-run.sh`, under the suite lock (`mutants.txt`). Eleven
mutants, each a part of the guard removed: the macro's test arms, the
refusal, the config reader's variables and its `HOME` (two mutants), the
Gateway and git-selector readers, the `HOME` handed to git, the child
isolation, the thread carry, the MCP fallback, and a new direct read the scan
must catch. Each runs the whole `test_env` module and the thread-carry case.
**11 killed, 0 survived.**

The first run (`mutants-r1.txt`) named one case per mutant. Its config
mutant survived: it unrouted the variables loop only, and the one case run
still panicked at the `HOME` read after the loop. Hence one shared case list,
and the config reader split into two mutants.

## 6. Gate

`gates.sh` at `8db95597`, under the suite lock with a short `TMPDIR`
(`gates.txt`): `cargo fmt --check`, both clippy runs, `cargo test` plain and
with `fault-injection`, `check-xfx-identity.sh`, `check-no-stubs.sh`,
`check-no-secrets.sh` -- every step exit 0. `cargo test`: 15 suites, 5557
passed, 0 failed, 7 ignored; with `fault-injection`: 5594 passed, 0 failed,
7 ignored.

## Overlap with CATALOG-REFRESH (train 11)

- Its `Environment::vars_for_test` (`config.rs` ~836) does not collide with
  this unit's `#[cfg(test)] impl Environment { fn fake_process }` (head,
  above `mod tests`).
- Its fixture guard `the_shared_fixtures_runtime_reads_only_its_own_home`
  (`worker.rs` ~10080) and its edit of `recording()` to `with_environment`
  sit where this unit replaced `recording()`'s `Runtime::new` with
  `a_runtime` -- a textual conflict on one line, with the same meaning. The
  refusal makes the guard's second half redundant (a fixture that took the
  process's variables now panics where it is built); its first half, that
  the fixture's home is the case's own, `a_runtime` holds by construction.
  Both pass together; the train decides whether to keep it.

## Size

`git diff --stat 32afd307..e1a127a4`: `src/test_env.rs` and
`src/test_env/tests.rs` are test-only (784 lines). In the other `src` files
318 lines added, 98 removed, almost all inside `#[cfg(test)]` modules and
blocks; what a release build compiles is 12 call sites that became
`process_env!` and the macro's `cfg(not(test))` arm -- the release
comparison above is the proof that it compiles to the same code.

## Open

- **A launched worker's fake process narrows to `Environment`'s keys.** In a
  case that launches the worker inside a `FakeProcess`, `carried` gives the
  runtime thread the case's fake; then `Runtime::new` ->
  `with_environment` -> `for_this_thread` replaces it with one made from the
  `Environment` -- `HOME` and the nine keys `from_process` takes
  (`config.rs:801-816`). `XFX_GATEWAY_URL`, `GIT_CONFIG_*` and `ZDOTDIR` are
  not among them, so that thread's `Endpoint::from_process`,
  `GitEnvironment::from_process` and `LinkBase::from_environment` would then
  see them unset where the case set them. Still fail-closed (the fake, never
  the real process), and no case today sets them and launches; a future one
  would silently get the compiled-default endpoint. The fix for the train:
  `for_this_thread` overlays the environment onto the thread's existing fake
  instead of replacing it.

- `minimal_environment` in a unit test hands a command `HOME=""` when the
  case gave no fake home. git takes an empty `HOME` as set and looks for
  `/.gitconfig`; that file exists on no runner this repository uses. The
  production path is the same for a process with no `HOME`.
