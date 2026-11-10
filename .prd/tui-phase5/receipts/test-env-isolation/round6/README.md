---
title: TEST-ENV-ISOLATION round 6 receipt -- cfg(clippy) refused, the exec family whole, missing dep-info refused
type: receipt
tags: [xfx, phase5, tests, security, environment, receipt]
---

# TEST-ENV-ISOLATION, round 6

The round-5 reviews' MUSTs and SHOULDs, in round 5's design: no redesign.
Every figure below is from a file in this folder.

- **Round-5 head:** `0877f461` (code `cbab9aba`).
- **Toolchain:** rustc 1.99.1, clippy 0.1.99, aarch64-apple-darwin.
- **Linux: not yet run.** The ubuntu CI rows are its first run, and must
  be green before merge.

## MUST 1: a cfg that names clippy (gpt6-check MUST 1, opus M1)

**The hole.** clippy compiles with `cfg(clippy)` set, so code under
`not(clippy)` is never linted -- and the library's unit-test binary, which
the artifact probe exempts because it runs in the developer's process by
design, runs it. `#[cfg(all(test, not(clippy)))]` around a `HOME` read or a
`Command::new("git")` passed every gate.

**The fix** (`cfg_problems`, the dispatcher's decision). Any cfg predicate
that names `clippy` is refused in `src/`, `tests/` and `build.rs`: the whole
of `cfg(..)` and `cfg!(..)`, and the predicate (the first argument) of
`cfg_attr(..)` -- so `cfg_attr(test, allow(clippy::x))`, which names a clippy
lint in the attribute it applies, is not. A text check, narrow by design:
one token in one place, comment lines skipped, across lines. The tree has
none. What it cannot see: a cfg a macro builds from its arguments.

**The boundary, now said** (`src/lib.rs`'s `process_env!` doc,
`clippy.toml`'s rule comment): the library's unit-test binary runs in the
developer's process, held by the forced lints and the fake process alone;
every integration test binary runs in a clean copy of itself.

**Fixtures:** `cfg_not_clippy`, `cfg_attr_not_clippy` (its predicate on a
line of its own), `cfg_macro_clippy` -- each refused as "a cfg predicate
that names clippy" -- and `cfg_attr_naming_a_clippy_lint`, accepted.

## MUST 2: the exec family (opus M2)

`clippy.toml` and `STARTS` add `libc::execvP` (Apple; `allow-invalid`),
`libc::execveat` (Linux with glibc; `allow-invalid`) and `libc::syscall`,
through which `execve`, `fork` or `clone` can be called by number. The
start rule's comment now says what it covers -- "nothing replaces the
process through a call named here, the raw `syscall` included" -- and what
it does not: inline `asm!`, and a dependency's own wrapper. Fixtures:
`exec_v_capital_p` (refused on macOS; on Linux it compiles to nothing and
is expected to be accepted) and `raw_syscall`.

## MUST 3: missing dep-info (gpt6-check MUST 2, opus S1)

**The holes.** `compile_time_names` skipped a missing `.d` file with
`continue` -- missing evidence read as no reads -- and stripped `lib` from
every file's name, an executable's too.

**The fix** (`dep_info`, `record_compiled`, `compiled_problems`).

- The dep-info file is found by artifact kind: `lib<crate>-<hash>.rlib` or
  `.rmeta` -> `<crate>-<hash>.d`; an executable `<crate>-<hash>` -> itself
  `.d`, a leading `lib` kept; an executable cargo uplifted
  (`target/debug/xfx`, a build script's `build-script-build`) -> traced to
  the hashed original it is the same bytes as -- a hard link on Linux, a
  copy on macOS (`target/debug/xfx` here has its own inode and a link
  count of 1), so by inode, else by size and content.
- An artifact from this manifest with no dep-info found is refused: "built
  with no dep-info to say what it read at compile time".
- `CLIPPY_ARGS` and `CLIPPY_CONF_DIR` are dropped only from the clippy
  passes' dep-info, not the `cargo test` builds' (opus NIT).

**Regressions** (self-test): `dep-info by artifact kind` -- an executable
named `library-<hash>`, a `libxfx-<hash>.rlib`, an executable uplifted by a
hard link and one uplifted by a copy, each resolved to its own `.d`; and
the `built` fixture crate, whose `tests/plain.rs` now holds
`#[cfg(not(clippy))] const _: &str = env!("CARGO_PKG_AUTHORS");` -- refused
from the `cargo test --no-run` builds' dep-info (clippy never compiles
it) -- and then, with every `.d` under its target removed (cargo calls the
builds fresh and writes none again), refused as built with no dep-info,
`tests/plain.rs` and `src/main.rs` both.

## SHOULDs

- **The user database** (opus S3): `libc::getpwuid`, `getpwuid_r`,
  `getpwnam` and `getpwnam_r` are readers now, as `home_dir` is: they give
  the real home with no read of `HOME`. The tree uses none. Fixture
  `user_database`.
- **One matrix** (opus S2, gpt6-check NTH 1): `PASSES` and `TEST_BUILDS`
  are one `MATRIX` -- dev and release, each with no feature and with every
  feature -- so the test builds gain `--release --all-features`. The
  "every profile and feature set" comment is gone; the matrix is every
  feature set only while the manifest has one feature and no default,
  which `matrix_problems` now holds (a second feature is refused until the
  matrix grows). Cost: 52 test binaries started instead of 39; the whole
  check took 325 s here, warm from the previous run (`time -p`, real).
- **A dependency's macro** (gpt6-check NTH 2): the self-test's fixture
  crate depends on a scratch `macro_dep` whose `start!()` expands to
  `::std::process::Command::new("git")`. clippy reports it, inside the
  external macro, and the check refuses it as "inside macro_dep::start!
  (defined in ...)" -- now printing a path only if it is relative, else "a
  file outside this package", so no registry or home path reaches a
  receipt. Fixture `dependency_macro`.
- **The isolation constructor** (gpt6-isolation NTH 1-2, opus NIT):
  - a relative `TMPDIR` is refused, by the parent before it makes a home
    and by a copy handed a home under one ("TMPDIR is not an absolute
    path"): `remove_home` would otherwise resolve it against whatever
    directory a case had moved to. Regression
    `a_relative_tmpdir_makes_no_home_and_runs_no_case`: started from a
    scratch directory with `TMPDIR=relative-tmp`, it refuses, and nothing
    is made there; a copy handed `relative-tmp/xfx-test-home-<marker>`
    refuses too.
  - `current_exe()` now runs before the home is made, so no failure leaves
    it behind: past its creation only the `exec` can fail, and that path
    removes it. There is no regression for this one: `current_exe()` cannot
    be made to fail from a test on either platform; the order is what holds
    it.
- **CLIPPY_* strip scope**: part of MUST 3 above.
- **mutants.txt:47** (`inherits` scored with no result line): the round-6
  harness prints the copy's own refusal when there is no result line, its
  list of names cut ("refused: the environment holds N name(s) the copy is
  not given").

**Recorded, not changed:** `doctest = false` stays (for the user's veto);
moving the `TurnJournal` `compile_fail` example into the fixture crate would
need xfx as the fixture's dependency, a full extra build of xfx per
self-test -- not cheap, so not done.
