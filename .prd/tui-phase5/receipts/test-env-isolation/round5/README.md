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

## Part 4: the built test binaries, not only clippy's compile (gpt6 isolation MUST 3)

**The hole.** Round 4 inferred that a test binary was isolated from
clippy's reports of `tests/support/isolated.rs`'s reads. A file with
`#[cfg(clippy)] mod support;` reports them under clippy, while `cargo test`
builds it without `support` and runs its cases in the developer's process.

**The fix** (`artifact_problems`). For each of three builds -- dev, dev
`--all-features`, `--release` -- `cargo test --no-run --all-targets
--message-format=json` lists every executable cargo built for a test
profile. Each is held to a rule by what it does when run:

- **kind `test`** (an integration test binary): started with `--list` and
  `XFX_TEST_ISOLATED_COPY=not-a-marker`, `PATH` its only other name. A
  binary whose constructor is linked and runs refuses itself before `main`
  (exit 101, "no case runs"); one without it lists its cases and is
  refused. This proves on every run, in every build, that the constructor
  is in the binary and runs.
- **kind `lib`**: the unit-test binary, the one that runs in the
  developer's process by design -- every read there goes through
  `process_env!` to the case's fake process (`src/test_env.rs`).
- **any other kind** (a binary's harness, a bench, an example): must list
  no case at all. `[[bin]] test = false` keeps plain `cargo test` from
  building the binary's harness, but `--all-targets` (CI's) still builds
  it; it holds no test, and one added there would be refused.

**Why a run and not `nm`.** `cargo test --release` builds with the release
profile's `strip = true`: the release `parity` test binary's symbol table
holds 150 entries, all imports, while the dev one has the constructor's
(`r5-art` probe, `nm`). A symbol check would refuse every correct release
binary, and a run shows the constructor runs, not only that it is there.

The same builds' dep-info is held to `COMPILE_TIME_NAMES` too (part 2):
these are the real test binaries, not clippy's check builds.

**Doctests** (SHOULD): `[lib] doctest = false`. A doctest runs in the
developer's process as rustdoc builds it, before any isolation, and
rustdoc's `--list` does not say which are `compile_fail` (it printed the
library's one as `src/agent/types.rs - agent::types::TurnJournal (line 70): test`),
so refusing the other kinds would mean reading the source. The cost: that
`compile_fail,E0046` example -- a journal wrapper that forwards only
`record` does not build -- is no longer compiled; the trait still has no
default method, which is what it showed.

**Measured:** 39 test binaries started (13 per build), each refused itself;
the whole check took 373 s here with two of the three test builds cold
(`time -p`, real).

## Part 5: HOME, exactly (gpt6 isolation MUST 1 and 2, opus M3)

**The holes.**

- The copy compared only `HOME`'s last component with the marker. With
  `HOME=<...>/xfx-test-home-<marker>/` -- a trailing slash -- on a link to
  an empty private directory, `file_name()` still matched, `lstat` followed
  the link, and the copy accepted it; at exit `remove_dir_all` would empty
  the link's target.
- The parent's creation error printed `home.display()`, which is `TMPDIR`'s
  value.

**The fix** (`tests/support/isolated.rs`).

- One function, `home_for(marker)`, makes the path -- `TMPDIR` (else
  `/tmp`) joined with `xfx-test-home-<marker>` -- for the parent and for
  the copy. The copy refuses a `HOME` whose bytes end in `/`, then any
  `HOME` whose bytes are not exactly `home_for(marker)` (the `OsStr`, not a
  `Path` comparison, which would ignore a trailing slash), then a link, a
  directory not its own or not 0700, and a non-empty one.
- Every refusal names what it refused and, for an I/O error, only its kind
  (`err.kind()`); none prints a path built from the environment.
- The doc states the boundary -- the copy confirms what its parent made,
  against a mistake or an inherited marker, not against a process that can
  already write as this user -- and that tool variables (`LLVM_PROFILE_FILE`,
  `NEXTEST_*`) are not handed on (NIT).

**Regressions** (`tests/isolation.rs`, each case started with its own
`TMPDIR`): the sentinel home at the exact path, non-empty; an empty home
elsewhere; the right home beside `XFX_MODEL`; the exact path as a link to an
empty private directory; and that path with a trailing `/` -- each refused
for its reason, and no mark left in the home or in the link's target. And
`a_copy_that_cannot_be_made_says_so_without_a_value_of_the_environment`:
`TMPDIR=<scratch>/xfx-sentinel-tmpdir-value/gone`, where no home can be
made -- the binary refuses ("under TMPDIR"), and the sentinel is not in
anything it printed.

The case list now also holds a home with the marker's name in another
directory (`elsewhere/xfx-test-home-<marker>`): round 4's last-component
comparison accepted it; the exact path refuses it.

## Part 6: the boundary and the claims (opus M2a, M2b), and the SHOULDs

**Named now** (`clippy.toml`'s readers):

- `libc::_NSGetEnviron` (`allow-invalid`: libc has it on Apple targets
  only). Fixture `ns_get_environ`, compiled and refused on macOS; on Linux
  the fixture compiles to nothing and is expected to be accepted.
- `std::env::temp_dir`, which reads `TMPDIR`. Its three uses, all in unit
  tests (`tools/terminal.rs`, `mcp/config.rs`, `mcp/command/tests.rs`), are
  now `env_exception!(temp_dir())`, a fifth form of the macro, listed as
  reads of `TMPDIR` (EXCEPTIONS 39 in 26 files). Fixture `temp_dir_read`.

**The boundary, as it stands after this round.** Clippy resolves each
call's path, so an import, a rename, an alias of an alias, a closure over a
reader or a start and a reference to one are the call they name;
`--force-warn` reports them whatever allow sits over them, in the dev and
release profiles, with and without every feature. `env_exception!` takes
literals only, and `env!`/`option_env!` take only the six names cargo and
`build.rs` set -- held by clippy's macro lint and by rustc's dep-info. What
this does not see:

- a C function declared by hand (`extern "C" { fn getenv(..) }`;
  `src/local_time.rs:236` declares `tzset`, which reads `TZ` inside the C
  library, as `localtime_r` does);
- the `environ` static (a static, which no `disallowed-methods` entry can
  name), and a read of `/proc/self/environ`;
- a dependency's own reads, and a proc macro's that rustc does not track;
- the system git configuration, read by design
  (`src/test_env.rs:240-242`); the global one is `/dev/null` for a homeless
  plan;
- `PATH` and `TMPDIR`, which every test process keeps: a `git` early on
  `PATH` or a hostile temporary directory is the developer's own machine;
- a process that can already write as this user (part 5).

In an integration test all of these meet an environment built from
nothing; in a unit test they would meet the developer's. Doctests do not
run (part 4).

**Claims corrected:** `src/lib.rs`'s `process_env!` doc (it said every other
read "is refused by clippy"; it now says which readers, adds the compile-time
macros, and names what clippy cannot), `clippy.toml`'s rule comment (the
same), and round 4's README (its PLAIN sentence omitted `build.rs`'s two
`Command::output` calls; its boundary paragraph and its "not a link" line
now point here).

**SHOULDs.**

- Doctests: `[lib] doctest = false` (part 4 says why that one).
- Clippy configuration: a `.clippy.toml` is refused, and the script asserts
  `clippy.toml` holds every reader, start and macro it knows of (part 1).
- More starts: the exec family, `clone` and `CommandExt::exec` (part 1).
- CI checkout: `persist-credentials: false` in `ci.yml` -- no later step
  talks to the remote, and the token would otherwise sit in `.git/config`,
  which a fresh `HOME` does not hide.
- Linux: "not yet run", at the top of this file.
- NITs: build.rs's two `output` calls (round 4's README); the summary line
  says "12 process_env! call sites (over its build arms)", not "12 build
  arms"; both CI steps run `python3 -I`; `ci.yml`'s "Install the pinned
  toolchain" is "Install the stable toolchain" (it installs unpinned
  stable; `release.yml` and `preview.yml` keep their names, outside this
  unit); the tool variables not handed on are in `isolated.rs`'s doc.

## GREEN under the sentinel environment, and the enumeration

On the `--no-local` clone at `2cfe0ca7`, under `../sentinel-run.sh` (whose
`.gitconfig` now carries the token-shaped `insteadOf`):

- `green-lib.txt`: the lib suite, 4638 passed, 0 failed, 7 ignored (two
  fewer than round 4: the line check and the reader-list test are gone);
  no sentinel value in the output.
- `integration-green.txt`: all 13 integration suites, 0 failed -- cli 82,
  gateway 93, interactive 79, isolation 2 (and its ignored marking case),
  ledger_compat 5, llmux 103, mcp 5, parity 26, permissions 159,
  session_compat 9, sessions 65, tool_loop 109, tui 186.
- `integration-reads.txt` (round 4's `integration-reads.py`): 17,251 reads
  -- 15,348 by the `xfx` binary the suites start, 1,825 in clean copies,
  78 in the processes the developer started, **0 of them through
  `process_env!`**. The 78 are the constructor's own six per binary, 13
  binaries; the isolation suite's own starts run under a `TMPDIR` of their
  own, so their reads land in that directory's log, not this one. As in
  rounds 3 and 4, the instrumented run alone shows one interactive and
  eight tui failures; the uninstrumented runs above pass them.

## RED: each hole, at the round-4 head and at this one

`red.py`, on the clone: each hole planted at `4050be81` and at the new
head, then that revision's own gates run -- `cargo clippy --all-targets -D
warnings` and `scripts/check-env-reads.py`. Every hole passes every gate at
`4050be81`; the new check refuses each, for its reason.

| hole | at `4050be81` | now (`check-env-reads.py`) |
|---|---|---|
| opus M1: `use std::process::Command as Git;` then `crate::process::output(&mut git)` in a src test module (`red.txt`) | clippy passes, check passes | refused: `std::process::Command::new outside ...` (clippy -D refuses it too) |
| gpt6 check MUST 1: `option_env!("XFX_GATEWAY_KEY")` in a src test (`red-first-run.txt`) | passes, passes | refused twice: dep-info "compiled with XFX_GATEWAY_KEY", and "a core::option_env! of XFX_GATEWAY_KEY" |
| gpt6 check MUST 2: a `HOME` read under `cfg(not(debug_assertions))` (`red-first-run.txt`, `red.txt`) | passes, passes | refused: `std::env::var_os outside ...`, by the release pass |
| gpt6 isolation MUST 3: `tests/escape.rs` with `#[cfg(clippy)] mod support;` (`red-first-run.txt`) | passes, passes | refused three times: "its test binary (dev / --all-features / --release) runs its cases without making a clean copy of itself (exit 0)" |

`red-first-run.txt` is the first run. Its M1 row is not used: that plant
wrote `&mut Git::new("git").args(..)`, which tripped `needless_borrow` at the
old head -- a style refusal that hid the hole -- and the run's HOME section
judged by the test's messages, which this round changed. `red.txt` reruns
both: the M1 plant builds its command in steps, and the HOME holes are
judged by what happened.

**The HOME holes, by what happened** (`red.txt`, second half): the
isolation binary started as `tests/isolation.rs` starts it, once with
`4050be81`'s `isolated.rs` and once with this one.

| started with a marker and | round 4's copy | this round's |
|---|---|---|
| a home with the marker's name in another directory | **ran the case**, and at exit **deleted that directory** | refused, nothing touched |
| a link to an empty private directory at the exact path | refused | refused |
| that link, `HOME` ending in `/` | **ran the case**, and at exit **deleted what the link points at** -- opus M3's prediction | refused, nothing touched |
| a `TMPDIR` no home can be made under, its value a sentinel | refused, and **printed the sentinel** | refused, sentinel not printed |

## Self-test, mutants, release, gate

- **Self-test** (`gates.txt`'s last step): 35 fixtures, 0 not as expected
  -- round 4's 24, and `aliased_command`, `command_new_as_a_function`,
  `command_new_by_qualified_path`, `exec_and_execv`, `option_env_of_a_key`,
  `env_inside_concat`, `release_only_read`, `temp_dir_read`,
  `ns_get_environ`, the configuration, and the test binaries cargo built (a
  test binary with no constructor and a binary harness with a case, each
  refused in all three builds). Its fixture crate now carries the tree's
  `[lints.clippy] disallowed_macros = "allow"` (`cbab9aba`): without it a
  check that stopped forcing the macro lint still passed (below).
- **Mutants** (`mutants.txt`, `mutants-r5.py`, at `f015d3c7`): **47, 47
  killed, 0 runs with a sentinel value in their output**, and each of the
  three narrowings of the check passes on the unmutated head first.
  - Rounds 1-4's 14 unit mutants, at this round's text.
  - The start rule (`envcheck`): round 2's bare `Command` in a test module,
    the bare SIGCHLD probe, opus M1's aliased `Git::new`, a
    `Command::new` in a test file, `clippy.toml` losing `Command::new`, and
    a `.clippy.toml`.
  - Compile-time reads: `option_env!("XFX_GATEWAY_KEY")` in a unit test,
    and an unlisted `env!` inside `concat!` (refused by dep-info alone).
  - Round 4's nine allow and listing mutants; a test file with no `mod
    support;`; a release-only read; a `#[cfg(clippy)] mod support;` file
    (refused on its built binary); a `#[test]` in `src/main.rs`.
  - The check's own code (`selftest`): the macro taking `$name:expr`, the
    self-test compiling its fixtures in dev only, dep-info not read, the
    probe never run, the macro lint not forced.
  - The copy: trusting its marker, taking a non-empty `HOME`, a name it was
    not given, no constructor, the whole environment handed on, a `HOME`
    ending in `/`, the round-4 last-component comparison, and the path in
    the creation error.

  The first run (`mutants-first-run.txt`, at `2cfe0ca7`) had six
  survivors, two causes, both fixed before the run above. Five
  isolation mutants were scored by any "test result: ok." in the
  output, which a failing case's message quotes from its copy (the
  harness's mistake; their suites had failed). `no-macro-lint` was a real
  gap in the self-test (the fixture crate's `[lints]`). Four unit-test plants
  sat between `#[track_caller]` and its `fn`; `env-in-concat` died of a
  compile error for it, so those four were rerun with the plant above the
  attribute (`mutants-plants-rerun.txt`): each killed by the check, for
  its reason.
- **Release** (`release-compare.txt`): main and `2cfe0ca7`, built back to
  back on the clone: 11,668,096 bytes each, the same section table, 591
  differing bytes -- 414 signature, 16 `LC_UUID`, 161 moved panic lines, 0
  unexplained. **`__text` is identical.** Everything after `2cfe0ca7`
  changes only the script and the receipt.
- **Gate** (`gates.txt`, `../gates.sh` under the lock, on the clone at
  `f015d3c7`): fmt, both clippy runs, both test runs (lib 4638; every
  integration suite), identity, stubs, secrets, `check-env-reads.py` (39
  invocations in 26 files, 12 `process_env!` call sites, 14 plain calls,
  70 compile-time reads of 6 names, 39 test binaries started) and its
  self-test: every step exit 0.
