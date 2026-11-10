---
title: TEST-ENV-ISOLATION round 7 receipt -- the cfg check reads code in every compiled file; a .d must be the artifact's own
type: receipt
tags: [xfx, phase5, tests, security, environment, receipt]
---

# TEST-ENV-ISOLATION, round 7

The round-6 reviews' MUSTs and SHOULDs, in the same design. Every figure
below is from a file in this folder.

- **Round-6 head:** `9a4d5ef2` (code `3b6cac51`). **Code:** `8fd9d62c`.
- **Toolchain:** rustc 1.99.1, clippy 0.1.99, aarch64-apple-darwin.
- **Linux: not yet run.** The ubuntu CI rows are its first run, and must be
  green before merge.

## The boundary

The check guards against honest mistakes in ordinary Rust source. Outside
it, and said so in `src/lib.rs`, `clippy.toml` and here:

- a cfg a macro builds from its arguments;
- code a dependency runs on its own -- `ignore::WalkBuilder` reads the
  global gitignore under the real `HOME` unless the caller turns
  `git_global` off, as `src/tools/read.rs:1316` does;
- inline `asm!`.

Deliberate evasion inside the repository is review's job.

## MUST 1: the cfg check, read as code, in every compiled file

**The holes** (opus M1, gpt6-check MUSTs 1-2). Round 6 walked the raw
text: (a) a comment or a string holding `)` or `,` -- `#[cfg(/*)*/
all(test, not(clippy)))]`, `#[cfg /* c */ (not(clippy))]`, `feature =
"))"` -- ended the predicate early or kept `cfg` from matching; (b) every
line starting with `//` was blanked, even one that closes a block comment
and then holds code; (c) only `*.rs` under src/ and tests/ and the root
build.rs were read, so a `#[path]` file of any name, an `include!`d file
and a linked directory were not.

**The fix.**

- `code_of`, a small lexer: `//` and nested `/* */` comments, and plain,
  byte, C and raw string literals and char literals, are turned to spaces,
  newlines and offsets kept; a lifetime stays code. `cfg`, `cfg!` and
  `cfg_attr` are matched, and their parentheses walked, on what is left.
  The line blanking is gone.
- The files: every `.rs` under src/ and tests/ (now following links),
  build.rs, and every source the dep-info of every compilation in the
  matrix lists -- clippy's passes and the test builds -- inside the
  repository: a `#[path]` module, an `include!`d file, one under a linked
  directory, one a build script generates. A source this package compiles
  from outside the repository is refused outright. `Cargo.toml` and
  `clippy.toml`, which dep-info also lists, are not Rust and are not read.

**Fixtures** (self-test), each passing round 6's check:
`cfg_comment_holding_a_paren`, `cfg_comment_before_its_paren`,
`cfg_string_holding_a_paren` (a), `cfg_after_a_block_comment_line` (b),
`path_module_not_rs` (`#[path = "t.inc"]`), `include_outside_src_and_tests`
(`include!("../gen/hidden.rs")`), `under_a_linked_directory`
(`tests/linked` a link) (c), and `path_outside_the_repository` -- each
refused -- and `cfg_in_a_comment_or_a_string`, the same text in `//`, `/*
*/`, a string, a raw string and beside a `'('` char, accepted.

## MUST 2: a `.d` must be the artifact's own (gpt6-check MUST 3)

**The hole.** An existing `.d` counted as evidence before it was read, so
an empty or garbage one was "no compile-time reads".

**The fix** (`read_dep_info`). A `.d` is evidence only if it holds the
artifact's own rule -- `<the artifact's file>: <sources>`, the file
`<dir>/<crate>-<hash>` or `<dir>/lib<crate>-<hash>.rlib` and the rest --
with at least one source; otherwise the artifact is "built with no
dep-info". rustc writes its `# env-dep:` lines last, so a `.d` cut after
the rule keeps it: for that, a compile-time read clippy's macro lint saw
that the same pass's dep-info lacks is refused (`unrecorded_problems`) --
"dep-info without NAME, a compile-time read clippy saw: cut short or
rewritten". What that cannot see: a cut `.d` of a crate whose every
compile-time read is in code clippy never compiles (which the cfg check
refuses anyway).

**Fixtures** (self-test): the `built` crate's `.d` files emptied, then cut
inside their first line -- each refused as no dep-info -- and the fixture
crate's `.d` files stripped of their `# env-dep:` lines -- refused for
`XFX_GATEWAY_KEY`, which clippy saw in `option_env_of_a_key`.

## SHOULDs

- **build.rs sets no cfg** (opus S1): `rustc-cfg` in build.rs, or a file it
  compiles, is refused -- a cfg keyed off `CLIPPY_ARGS` would differ
  between the check's clippy and a test build without naming clippy.
  build.rs prints only `rustc-env` today. Fixture: the `built` crate's
  build.rs prints `cargo:rustc-cfg=hidden`.
- **Dependency readers named** (opus S2): in the boundary above.
- **`libc::getpwent`, and `getpwent_r` (`allow-invalid`, glibc)** (opus S3):
  readers. Fixture `user_database_walk`.
- **Features** (gpt6-check NTH 1): a `default` feature, even `default =
  []`, and an optional dependency (a feature of its own) are refused by
  the matrix check, which runs no `--no-default-features` build.
- **Uplift tracing** (gpt6-check NTH 2, opus NIT): the original is the one
  candidate with the same device and inode, else the one with the same
  bytes; two candidates either way is no dep-info.
- **The relative TMPDIR regression** (gpt6-isolation NTH): split into a
  parent case -- `relative-tmp` made first, so a parent without the
  refusal would make its home there; it must stay empty -- and a copy
  case.
- **NITs:** the round-7 mutant harness quotes a "NOT RESOLVED" line;
  `gates.sh` keeps the "accepted" and "resolved" lines.
