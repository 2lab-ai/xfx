---
title: TEST-ENV-ISOLATION round 5 receipt -- Command::new, compile-time reads, every profile, the built artifacts, HOME exactly
type: receipt
tags: [xfx, phase5, tests, security, environment, receipt]
---

# TEST-ENV-ISOLATION, round 5

The round-4 reviews' MUSTs and SHOULDs, closed with round 4's mechanism:
clippy's resolved lints under `--force-warn`, and what cargo says it built.
No new text scanning. Every figure below is from a file in this folder.

- **Round-4 head:** `4050be81` (code `b0d36ba4`).
- **Toolchain:** rustc 1.99.1, clippy 0.1.99, aarch64-apple-darwin.
- **Linux: not yet run.** The constructor is linked into `.init_array`
  there; the ubuntu CI rows are its first run.

## Part 1: `Command::new` is a disallowed method (opus M1)

**The hole.** A unit test's child gets the case's fake process only when it
is made by `process::command`; `spawn`, `output` and `status` take any
`Command`. Round 4 guarded `Command::new` with the one text check that
survived, `line.contains("Command::new(")`, so
`use std::process::Command as Git; crate::process::output(&mut Git::new("git")...)`
in a src test module, `.map(Command::new)` and `<Command>::new(` all ran a
child with the developer's environment.

**The fix.**

- `clippy.toml`'s start rule now names `std::process::Command::new` and
  tokio's, `CommandExt::exec`, `libc::execv`, `execve`, `execvp`, `execvpe`,
  `fexecve`, `execl`, `execle`, `execlp` and `clone` (SHOULD: process
  replacement; `allow-invalid` where libc lacks one on some target).
- What clippy is told to let through, listed in `PLAIN` by file, path and
  count: `process::command`'s one `Command::new`; `tui/signals.rs`'s bare
  `/bin/sleep`, the case about what a bare `Command`'s child inherits, its
  environment cleared; `build.rs`'s; `tests/support/mod.rs`'s new
  `command`, which every integration test's child is now made with (27 call
  sites moved onto it); and `tests/support/isolated.rs`'s one `exec`.
- A start let through in `tests/` is accepted only in a test binary that
  runs in a clean copy (the script refuses its file otherwise); part 4 holds
  that on the built artifacts.
- **Deleted:** `process::tests::no_child_process_is_made_any_other_way` --
  the line check and its list of `clippy.toml` entries -- and
  `test_env::tests::clippy_disallows_every_reader_of_the_environment`. The
  script now asserts, from `clippy.toml` itself (`tomllib`), that every
  reader and every start it knows of is there, and refuses a `.clippy.toml`,
  which clippy reads in place of `clippy.toml` (SHOULD).

**Fixtures** (self-test): `aliased_command` (`use std::process::Command as Git; Git::new("git")`),
`command_new_as_a_function` (`.map(std::process::Command::new)`),
`command_new_by_qualified_path` (`<std::process::Command>::new("git")`) and
`exec_and_execv`, each refused as the start it is; and `clippy
configuration` -- a `.clippy.toml`, and a `clippy.toml` that lost
`std::env::var_os` and `Command::new` -- refused for all three.

## Part 2: compile-time reads (gpt6 check MUST 1, opus M2c)

**The hole.** `env!` and `option_env!` are compiler macros, not calls: a
`#[test]` that does `black_box(option_env!("GIT_CONFIG_GLOBAL"))` bakes the
compiling shell's value into the test binary, and nothing in round 4 saw
it. The tree uses them 76 times (`env!("` lines: `CARGO_MANIFEST_DIR` 42,
`CARGO_PKG_VERSION` 19, `CARGO_BIN_EXE_xfx` 12, `CARGO_PKG_NAME`,
`XFX_BUILD_REVISION`, `XFX_BUILD_CHANNEL` once each; no `option_env!`).

**Two records of the same read, both the compiler's.**

1. `clippy.toml` disallows `std::env`, `std::option_env` and the `core::`
   paths as macros, and the check forces `clippy::disallowed_macros` on
   beside `disallowed_methods`. Each report's span is the invocation; it
   must be `env!("NAME")` / `option_env!("NAME")` with a literal name in
   `COMPILE_TIME_NAMES` (6, each with its reason: what cargo and `build.rs`
   set). The lint is off in the ordinary run (`Cargo.toml`
   `[lints.clippy] disallowed_macros = "allow"`), where the 76 approved uses
   would fail it; a forced lint ignores that, as it ignores every allow.
2. rustc's dep-info: every crate the clippy runs compile from this
   manifest has a `.d` file whose `# env-dep:NAME=...` lines are the
   compiler's own record of each compile-time read. The check takes the
   names (never the values) and refuses any not in `COMPILE_TIME_NAMES`,
   less clippy-driver's own two (`CLIPPY_ARGS`, `CLIPPY_CONF_DIR`).

The second is needed, not a belt: clippy's macro lint does not see an
`env!` nested in `concat!` -- the tree's run reports 70 compile-time reads
of 6 names, and `gateway/mod.rs:86` and `session/store/fork/tests.rs:1802`
are such nestings -- while dep-info records every one.

**Fixtures:** `option_env_of_a_key` (`option_env!("XFX_GATEWAY_KEY")` in a
test module: refused by both records) and `env_inside_concat`
(`concat!("by ", env!("CARGO_PKG_AUTHORS"))`: clippy is silent, dep-info
refuses it). The configuration fixture also holds `disallowed-macros`.

## Part 3: every profile and feature set (gpt6 check MUST 2, opus M2d)

**The hole.** Round 4 ran clippy in the dev profile only, with the default
features and with `fault-injection` named by hand. A
`#[cfg(all(test, not(debug_assertions)))]` test that reads `HOME` was never
seen, though `cargo test --release` runs it; the tree has such code
(`usage/clock.rs`).

**The fix.** Four passes, their reports one union: dev and `--release`,
each with the default features and with `--all-features` (which replaces
the hand-named feature, so a new one is covered too). The check runs on
every CI row that lints; it includes the release pass, so the release part
of a split row needs no run of its own (`ci.yml`'s comment says so). Both
CI steps now run `python3 -I` (NIT). The four passes take 89 s on a warm
cache here (`time -p`, real).

**Fixture:** `release_only_read` -- a read under
`#[cfg(not(debug_assertions))]` -- refused; the self-test compiles its
fixtures in both profiles.
