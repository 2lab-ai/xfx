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
