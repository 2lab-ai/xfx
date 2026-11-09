---
title: TEST-ENV-ISOLATION round 3 receipt -- integration cases in a clean copy, a binding check that fails closed
type: receipt
tags: [xfx, phase5, tests, security, environment, receipt]
---

# TEST-ENV-ISOLATION, round 3

The round-2 reviews' two MUSTs and opus's SHOULDs. Every figure below is
from a file in this folder.

- **Round-2 head:** `a74073dd`. **Code head:** `b039c001`; the receipt
  commit follows it.
- **Toolchain:** `toolchain.txt` (rustc 1.99.1, LLVM 23.1.3, aarch64-apple-darwin).
- All runs used `../sentinel-run.sh`: `env -i`, a temp `HOME` whose
  `.gitconfig` and XDG git config each define a hook, `xfx-sentinel-*`
  values, and, new this round, `CARGO_PKG_NAME`, which cargo sets in every
  test process. Mutants, REDs and the integration runs used a `git clone
  --no-local` throwaway.

## MUST 1: integration cases ran git under the developer's HOME

An integration test links the library as a build does, so `process_env!`
there is `std::env` itself.

**Which cases.** `integration-reads.py` makes each build arm of
`process_env!` append one record per read: binary, thread (libtest names
it after the case), reader and variable name, never a value. Every
integration suite then runs. `integration-reads-base.txt`, at `a74073dd`:
92 reads in a test's own process, from 53 cases:

- 49 in `permissions.rs` (`minimal_environment`'s `HOME`, through
  `CommandPlan::prepare`);
- 3 in `tool_loop.rs` (the same; one of them on a thread the case starts);
- 1 in `llmux.rs` (`Endpoint::from_process`'s `XFX_GATEWAY_URL`).

A 54th, `an_approved_command_runs_through_the_platform_shell_with_a_clean_environment`,
was hidden: under `env -i` its own `CARGO_PKG_NAME` precondition failed
before it planned anything. It was found by reading the failures, which is
why `sentinel-run.sh` now sets `CARGO_PKG_NAME`.

**RED** (`red-must1.txt`, `a74073dd`, every integration suite unpatched):
two cases change outcome with the process's `HOME`:

- `auto_does_not_run_git_in_a_directory_whose_configuration_defines_a_hook`
  gets `Denied(ConfiguredHooks)` where it expects `DirectReadOnly`;
- `a_pre_existing_hostile_git_config_does_not_execute_on_the_automatic_route`
  says "`git diff` should still run" -- the sentinel's hook refused it.

Every other suite passes.

**The fix: (b), a clean copy.**

- Each of the 54 cases first calls
  `support::isolated::ran_in_a_clean_copy("<its name>")`. In the test
  process the developer started, that reruns the case alone in a copy of
  its test binary. The copy is started with `PATH` and `TMPDIR`
  (`clear_env`), an empty temporary `HOME`, `CARGO_PKG_NAME` and its own
  role, and the case passes or fails as the copy does. In the copy, the
  case runs.
- A name that matches no case fails, because the copy then runs none.
- **Why not (a):** moving 54 cases and their integration fixtures (`Tree`,
  the scripted prompters, the fake gateway) into the library's test
  modules would be thousands of lines moved, and the cases would stop
  testing through the public API. (b) adds one call per case and reuses the
  pattern `test_net` and the sentinel runners already use.
- **No release code changes:** see "Release" below.

**GREEN:**

- `integration-reads-head.txt`, at `b039c001`: 0 reads in a test process
  the developer started, and 103 in clean copies.
- `integration-green.txt`: every integration suite passes uninstrumented
  under the sentinel environment -- cli 82, gateway 93, interactive 79,
  ledger_compat 5, llmux 103, mcp 5, parity 26, permissions 159,
  session_compat 9, sessions 65, tool_loop 109, tui 186, all 0 failed.
- The instrumented runs show one interactive and 8-9 tui failures, at both
  revisions; uninstrumented runs at the same revisions pass every case
  (`red-must1.txt`, `integration-green.txt`). I did not look further: the
  instrumentation is not the product.

**Look-ahead.** The enumeration is the list of every integration-test call
that reaches a build arm in its own process, as far as the suites exercise
it. All 54 are closed. Two classes of read are outside `process_env!` and
not enumerated here: reqwest's proxy reading in an integration test's own
process, and children those tests start without `clear_env`. The children
are listed in `../README.md` §3.

## MUST 2: the binding check failed open inside an allow

**RED** (`red-must2.txt`, `a74073dd`): each of the four bypasses the reviews
named passes round 2's binding check and the start rule's scan --
enumeration inside an allowed function, an unbound key (the name held in a
variable outside the allow), a function-level allow over a read the check
cannot name, and an aliased `Command` started with `.output()`.

**The check now fails closed** (`src/test_env/tests.rs`):

- An `env:` allow is accepted only on a `let` statement or a match arm.
  The three function-level allows (`tui/term.rs`, `mcp/docker.rs`,
  `tui/fault.rs`) now sit on the one arm of a `match ()`, as
  `process_env!`'s do; that compiles to the original code (see "Release").
  Each such function carries a `match_single_binding` style allow. Inner
  `#![allow]`s of the lint are refused.
- Under an allow, only `std::env::var` and `var_os` are accepted, each
  argument a string literal, a `&str` constant the check resolves, or a
  closure parameter over an array of string literals in the same unit (the
  docker case). `process_env!`'s own `$name` and `name` are accepted in
  `src/lib.rs` only. `vars`, `vars_os`, `home_dir`, `getenv`,
  `secure_getenv`, `environ`, an uncalled reader, any other argument and
  any macro call are refused.
- `STARTS` is now read from `clippy.toml`'s start rule. A method entry
  (`Command::output`, `status`, `spawn`, the socket `pair`s) is matched as
  `.name(` on any receiver and as `::name` through any alias. A module's
  function is matched by its path or as a bare imported call.
- Crate-wide, an import that renames a reader, a `std::env` glob, a
  `use std::env;`, `use libc::getenv`, an aliased `libc`, the word
  `environ`, and the process's environ file under `/proc` are refused. A
  function's `use libc::*` for errno names is allowed: a unit that names
  `getenv` or `environ` is refused itself.
- **Allowed names replace guarded ones.** `ALLOWED_NAMES` lists the 25
  names an exception may read, each with its reason, so a name nobody
  thought to forbid -- a token another tool reads -- is refused until
  someone adds it with a reason. `EXCEPTIONS` still binds each of the 33
  allows to its file and variables.

**Fixtures:** `the_binding_check_refuses_every_way_round_it` (synthetic
text), plus the real-tree fixtures in `mutants.txt`. All are refused, and
the messages name lines and variables, never values.

## SHOULDs

- **(1)** -- MUST 2 above.
- **(2)** The start rule's `Command::new(` rule now covers library test
  modules. 13 sites go through `crate::process::command`, which gives a
  unit test's child the case's fake process or nothing. The one exemption
  is the signal-mask case, whose bare `Command` is its subject; its
  environment is cleared. Integration tests (`tests/`) keep their own
  discipline (`clear_env`, `env_clear`).
- **(3)** `toolchain.txt` records `rustc -vV`.
  `../round2/release-compare-r2.txt` §1 and `../round2/README.md` now call
  the 6,470,042-byte difference between two builds of the same source
  unexplained.
- **(4)** `mutants-r3.py` also searches for `xfx-sentinel-profile-model`.
- **(5)** The round-1 git plan case now asserts where its git's
  configuration comes from (`GIT_CONFIG_GLOBAL=/dev/null`), so round 2's
  "replaced" is true now, and `../round2/README.md` says when it became
  so. `clippy.toml` and `process_env!`'s doc say what the check does
  (statement or arm; `var`/`var_os` with a bound argument; nothing from the
  start rule). `../README.md`'s false integration claim is corrected.

## Mutants and fixtures

`mutants.txt`, at `b039c001`: **32 entries, 32 killed, 0 survived; 0 runs
with a sentinel value in their output.**

- Round 2's 21, at round 3's text.
- Eight real-tree fixtures for MUST 2 and SHOULD 2: enumeration, an
  unbound key, a function-level allow, an inner allow, an aliased `Command`
  with `.output()`, a reader imported under another name, the C `environ`,
  and a bare `Command` in a test module.
- Three for MUST 1, each against `tests/permissions.rs`:
  `isolation-gone`, which never copies (2 failed); `prologue-gone`, one
  case unwrapped (1 failed); and `copy-keeps-home`, the copy handed the
  developer's `HOME` (2 failed).

The `environ` fixture first "did not build", because libc exports no
`environ` for Apple targets. Rerun with `environ` declared by hand, which
builds, the check refuses it (same file, last section).

## GREEN, release and gate

- `green-lib.txt`: the whole lib suite at `b039c001` under the sentinel
  environment -- 4642 passed, 0 failed, no refusal.
- `release-compare.txt`: `32afd307` and `b039c001` built back to back.
  Same size (11,668,096 bytes), identical section table, `__TEXT,__text`
  identical. The 558 differing bytes are 161 of panic `Location` line
  numbers (0 unexplained), 16 of `LC_UUID` and 381 of the code signature.
- `gates-r3.txt`, at `b039c001`, under the lock: all 8 steps exit 0.
  `cargo test` 15 suites 5564 passed 0 failed 7 ignored; with
  `fault-injection`, 5601 passed 0 failed 7 ignored.
