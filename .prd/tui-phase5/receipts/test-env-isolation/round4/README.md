---
title: TEST-ENV-ISOLATION round 4 receipt -- a literal-only macro checked by clippy itself, and whole-binary isolation
type: receipt
tags: [xfx, phase5, tests, security, environment, receipt]
---

# TEST-ENV-ISOLATION, round 4

The dispatcher's round-4 decision on the round-3 reviews: stop patching the
text scanner, and replace it with what clippy resolves itself. Every figure
below is from a file in this folder.

- **Round-3 head:** `a327dd8a` (code `b039c001`). **Round-4 code:**
  `686f2e5b` (part 1), `354c512d` (the build script's two starts),
  `9e14a458` (part 2), `dcb9c12a` (part 3), `b0d36ba4` (the self-test's
  name fixture); the receipt commits follow them.
- **Toolchain:** rustc 1.99.1 (58cf8f9c3 2026-10-13), clippy 0.1.99,
  aarch64-apple-darwin (`force-warn-probe/probe.txt`, first line).

## Part 1: one literal-only macro, and a check that is clippy's own report

### The proof the check rests on

`force-warn-probe/` is a crate whose reads hide behind every suppression the
dispatcher listed: `#![allow(clippy::all)]` and `#![allow(warnings)]` at the
crate level, `#[allow(clippy::style)]`, `#[expect(clippy::disallowed_methods)]`,
`#[cfg_attr(all(), allow(clippy::disallowed_methods))]`, a path reference
(`let f = std::env::var_os; f("D")`) and an import alias. `probe.txt`:

- Without `--force-warn`, clippy reports nothing: the crate-level allows hide
  every read, the plain ones too.
- With `-- --force-warn clippy::disallowed_methods`, every read is reported:
  the path reference on the `let` that names the function, the alias on its
  call. The same holds with `-A clippy::disallowed_methods` after it on the
  command line, with `RUSTFLAGS="-A clippy::disallowed_methods -A warnings"`,
  and under `--cap-lints allow`.
- A read inside an exported macro, invoked from an integration test, is
  reported with the macro's name, its call site in the test and its
  definition site in the library (`def_site_span`).

So the preferred form was possible, and it is the one taken: no source text
is parsed to find a read.

### The macro

`src/env_exception.rs` holds one `macro_rules!`, alone in its file,
`#[macro_export] #[doc(hidden)]` so `tests/` reach it as `xfx::env_exception!`:

- `env_exception!(var("NAME"))` and `env_exception!(var_os("NAME"))`, each
  `$name:literal`;
- `env_exception!(first_set_os(["TMPDIR", "TEMP", "TMP"]))`, a literal list,
  for the one site that takes the first set and non-empty of several.

Each expands to the call in the one arm of a `match ()` carrying the allow,
the shape `process_env!` already had. A constant or a variable does not
compile (fixture `a_constant_into_the_macro`).

**Two macros let a read through, not one.** `process_env!`'s build arms
(`src/lib.rs`) take a computed name by design -- `var(key)` over the keys a
caller passes, and `lookup`, the closure the MCP configuration expands
`${VAR}` with -- so they cannot be literal-only. They are listed with that
reason: in a unit test the `cfg(test)` twin reads the case's fake process and
never the real one (`src/test_env.rs`); in a build, and in an integration
test, they read the process (part 2 isolates that).

**The build script** keeps its crate-level allow. It is a process cargo runs
before the crate exists -- never a test, never the product -- that reads what
cargo and a publishing workflow set and starts `git`. Its four reads and its
two starts of `git` are listed by file, path and count, so a fifth read
fails.

### The check: `scripts/check-env-reads.py`

It runs `cargo clippy --locked --all-targets --message-format=json --
--force-warn clippy::disallowed_methods`, with the default features and with
`fault-injection`, and requires both to finish. `RUSTFLAGS`, the cargo and
rustc wrappers and `CLIPPY_CONF_DIR` are dropped from its environment.
Each report is counted once, whatever number of targets compiled it, and
refused unless it is one of:

1. **An `env_exception!` arm.** The report's innermost expansion is defined
   in `src/env_exception.rs` (by definition site, not by name: a local
   `macro_rules! env_exception` is refused, fixture
   `a_macro_of_the_same_name`), and the call is in the macro's body, not its
   argument. The only source bytes the check reads are the compiler's own
   span of the invocation, which must be exactly the literal form --
   `env_exception!(var_os("NAME"))`, a name matching `[A-Z][A-Z0-9_]*`, or the
   list form -- so a raw string, an escape or a `$name` from a wrapping macro
   is refused. Each name must be in `ALLOWED_NAMES` (25 at part 1; 28 after part 2
   adds `HOME`, `RUST_BACKTRACE` and `RUST_TEST_THREADS` for the copy; each
   with its reason;
   one that nothing reads is refused too), and each invocation's file and
   names an entry of `EXCEPTIONS`, compared both ways: an invocation added
   fails, and one that goes missing fails.
2. **A `process_env!` build arm**, defined in `src/lib.rs`.
3. **A call outside any macro listed in `PLAIN`** by file, path and count:
   `process::spawn` and `process::spawn_unless` (`Command::spawn`, 2) and
   `process::pipe` (`rustix::pipe::pipe`, 1) -- the start and the pipe every
   other goes through -- and the build script's four reads and its two
   `Command::output` calls (`git_path`, `git_revision`).

Exactly one definition site in each of the two macro files may appear. That
is the dispatcher's fact (a), realised: the only suppressions that let a read
through are those two macros' arms and the build script's listed four;
every start an allow lets through is one of the five. *(Round 5: this held
only for what `clippy.toml` named -- `Command::new` was a text check, and
`env!`/`option_env!` were unchecked; see [`../round5/README.md`](../round5/README.md).)* Fact (b): every
literal passed to `env_exception!` is in `ALLOWED_NAMES`.

"Their count equals the macro's invocation count" is the `EXCEPTIONS`
comparison: the list is the invocations, one entry each, so the count of
reports and the count of invocations are compared without counting text.

**Deleted** (`src/test_env/tests.rs`): `blank`, `unit_after`, `constant`,
`word_starts`, `readers_in`, `bound_reads`, `reader_imports`, `starts`,
`sources`, the `EXCEPTIONS` and `ALLOWED_NAMES` tables (moved to the script),
`every_read_outside_process_env_is_bound_to_its_variables` and
`the_binding_check_refuses_every_way_round_it`. From
`process::tests::no_child_process_is_made_any_other_way`, the text search for
an allow that lifts the start rule: the forced run holds it. Kept: the test
that `clippy.toml` still lists each of the six readers, the test that it
lists each start, and the `Command::new(` line check.

**CI** runs the check on every row next to the lints, and its self-test on
one row per OS with the other script tests (`.github/workflows/ci.yml`,
`CONTRIBUTING.md`'s gate).

### Every file the macro conversion touches

In `686f2e5b`, each read moved into an `env_exception!` (release code
marked *build*; the rest is test-only code):

- `src/interactive.rs` (*build*: `TMPDIR`, /trace's report)
- `src/tui/shell.rs` (*build*: `TMPDIR`)
- `src/tui/term.rs` (*build*: `TMUX`; its `match_single_binding` allow and
  hand-written `match ()` gone)
- `src/tui/mod.rs` (*build*: `COLORFGBG`, `COLORTERM`, `TERM_PROGRAM`) and
  `src/tui/theme.rs` (the three name constants, now unused, deleted; their
  upstream citations moved to the reads)
- `src/tui/fault.rs` (*build*, `fault-injection` only: `XFX_TUI_FAULT`;
  `FAULT_ENV` deleted, the allow and `match ()` gone)
- `src/mcp/docker.rs` (*build*: `first_set_os(["TMPDIR", "TEMP", "TMP"])`;
  the allow and `match ()` gone)
- `src/permission/gitconfig.rs` (*build*: `PATH`)
- `src/permission/authority.rs` (*build*: `PATH`)
- `src/test_env.rs` (`PATH`, `TMPDIR`), `src/test_net.rs`,
  `src/process.rs`, `src/tui/signals.rs`, `src/tui/drawn_wider.rs`,
  `src/usage/cache.rs` (its `cfg(test)` fault), `src/usage/cache/tests.rs`,
  `src/mcp/transport.rs` (two), `src/profile_home/tests.rs`,
  `src/session/store/fork/tests.rs`, `src/local_time/tests.rs`,
  `src/test_env/tests.rs` -- each a re-run copy's role, read by its literal;
  the constant stays where the parent sets it
- `tests/support/sandbox.rs`, `tests/cli.rs` (`PATH`, `TMPDIR`; part 2 moves
  `cli.rs` onto `sandbox::clear_env`), `tests/permissions.rs`
  (`CARGO_PKG_NAME`), `tests/support/isolated.rs` (the marker; rewritten in
  part 2)

and, around them: `src/env_exception.rs` (new), `src/lib.rs` (the module,
`process_env!`'s doc), `clippy.toml` (the rule's comment),
`scripts/check-env-reads.py` (new), `.github/workflows/ci.yml`,
`CONTRIBUTING.md`.

### RED: the reviewers' bypasses, refused

`self-test.txt` (`check-env-reads.py --self-test`, at `b0d36ba4`): a crate
that compiles `src/env_exception.rs` itself (`#[path]`), under xfx's own
`clippy.toml`, with one test file per fixture; the same classification
then runs on clippy's report. 24 fixtures, 0 not as expected; each line
says why it was refused, never a value.

Each bypass the reviews found in the text scanner, written as the reviews
wrote it, with an `env:` allow over it where it had one -- each refused as
`std::env::var_os outside env_exception! and process_env!` (clippy reports
it; the allow lifts nothing under `--force-warn`):

- `multi_line_reason` (a reason over two lines);
- `let_cut_at_a_brace` (a `let` whose unit ends at `}` and goes on in a
  chain);
- `quote_char_unit` (`'"'` before the read);
- `renamed_import` (`pub(crate) use std::env::var_os as v` in a module) and
  `braced_import` (`use std::env::{self as e, var_os as v}`, both readers);
- `closure_rebinding` (the read inside a closure over its argument);
- `shadowed_constant` (an inner `const NAME` shadowing the allowed one);
- `alias_through_alias` (`use std::env as e; use e::var_os as v`);
- `path_reference` (`let read = std::env::var_os; read("HOME")`: reported
  on the `let`);
- `libc_system_as_run` (`use libc::system as run`, a start under an `env:`
  allow: `libc::system outside ...`);
- `every_reader`: all six readers under one allow, six refusals.

The lifts that never name the lint -- `crate_level_allow_all`
(`#![allow(clippy::all)]` and `#![allow(warnings)]`), `group_allow`
(`clippy::style`), `expect`, `cfg_attr_allow` -- and
`a_start_under_an_env_allow` (`Command::status`): each refused the same
way. The macro's own edges: `unlisted_name` (`GIT_CONFIG_GLOBAL`: not an
allowed name), `home_outside_the_copy` (`HOME` read anywhere but
`isolated.rs`: unlisted), `unlisted_invocation`, `raw_string_name`
(`r"PATH"`: not the literal form), `a_constant_into_the_macro` (does not
compile: "no rules expected `NAME`"), `a_macro_of_the_same_name` (a local
`macro_rules! env_exception` taking an expression: refused by its
definition site), `a_wrapping_macro` (a `$name:tt` passed through: not the
literal form). `accepted` (a listed literal) is accepted.

The inherited marker is part 2's RED, below.

### Mutants

`mutants.txt` (`mutants-r4.py`, on a `--no-local` clone at `dcb9c12a`; no
value printed, every run searched for the sentinel's seven values,
`xfx-sentinel-git-token` among them): **32 entries, 32 killed, 0 runs with
a sentinel value in their output.**

- Rounds 1-3's 14 unit mutants at round 4's text (the guard, the refusal,
  each reader routed around `process_env!`, child isolation, the carry, the
  MCP fallback, the overlay, the homeless global config, both proxy
  rules): each killed by a failing case.
- The start check: round 2's bare `Command` in a test module, and
  `probe-bare`, the SIGCHLD probe back on a bare `Command::new` -- killed
  now that `process.rs`'s tests are read.
- The forced check (`envcheck`): an allow over a plain read, a crate-level
  `allow(clippy::all, warnings)`, `expect` with `cfg_attr(allow)`, a
  listed invocation reading `HOME` instead, an invocation the list does
  not hold, a listed one gone, an allow over a `Command::status` outside
  `process::spawn`, one read more in `build.rs`, and a `tests/*.rs` with
  no `mod support;` -- each refused, by name.
- `macro-takes-expr` (`var_os`'s arm taking `$name:expr`): the self-test
  fails -- `a_constant_into_the_macro` compiles.
- Part 2: the copy trusting its marker, taking a non-empty `HOME`, a
  `HOME` the marker does not name, a name it was not given, no constructor
  at all, and the copy handed the whole environment: each fails
  `tests/isolation.rs`. The last runs no case at all -- the copy refuses
  itself, which is the refusal working, and is counted killed because the
  suite does not pass.

The self-test's `unlisted_name` fixture read `HOME` until `b0d36ba4`:
part 2 allowed `HOME` for the copy, so at `dcb9c12a` that fixture was
refused only as unlisted and the self-test failed its own expectation.
`b0d36ba4` names `GIT_CONFIG_GLOBAL` there and adds
`home_outside_the_copy`; nothing else changed between the two.

### Release

`release-compare.txt`: main (`32afd307`) and `dcb9c12a`, built back to
back on the clone with `XFX_BUILD_REVISION=000000000000`, the same
toolchain. 11,668,096 bytes each, the same section table; 591 bytes
differ -- 414 in the code signature's page hashes, 16 in `LC_UUID`, and 161
in `__DATA_CONST,__const`, every one a panic `Location` whose line number
moved (`gateway/mod.rs`, `interactive.rs`, `mcp/runtime.rs`,
`mcp/transport.rs`, `tui/shell.rs`, `tui/theme.rs` -12, `tui/worker.rs`);
0 not explained that way. **`__text` is identical**: the macro, the
conversion of the nine reads a default build makes, and the hand-written
`match ()` wrappers it replaced change no instruction. The release build
compared is the default one; `fault-injection` (`tui/fault.rs`'s read) is
not a shipped build and was not compared. `b0d36ba4` changes only the
script.

### The gate

`gates.txt` (`../gates.sh` under the suite lock, on the clone at
`b0d36ba4`): fmt, clippy, clippy with `fault-injection`, the whole test
suite twice (default: lib 4640 passed, and every integration suite; with
`fault-injection`: lib 4663, tui 200), identity, stubs, secrets,
`check-env-reads.py` (36 invocations in 23 files, OK) and its self-test
(24 fixtures, 0 not as expected): every step exit 0.

## Part 2: every integration test binary runs in a clean copy of itself

**What replaced the 54 prologues.** `tests/support/isolated.rs` puts a
function in the binary's constructor list (`#[used]` static in
`.init_array` on Linux, `__DATA,__mod_init_func` on macOS; a
`compile_error!` anywhere else), so it runs before `main` and before libtest
reads its arguments. In the process the developer started it makes an empty
`HOME` (`$TMPDIR/xfx-test-home-<32 random hex>`, mode 0700) and `exec`s the
same binary with the same arguments, the environment built from nothing:
`PATH` and `TMPDIR` (`support::sandbox::clear_env`), that `HOME`, cargo's
`CARGO_PKG_NAME` (from `env!`, not inherited), `RUST_BACKTRACE` and
`RUST_TEST_THREADS` where set, and the marker `XFX_TEST_ISOLATED_COPY`
carrying the random name. `exec` and not a child: the process id, the
streams and the exit status stay the ones cargo and a `timeout` see, and no
copy outlives a killed parent. The copy removes its `HOME` at exit
(`atexit`); a copy killed by a signal leaves an empty-or-scratch directory
under `$TMPDIR`.

**Why a constructor and not a cargo runner.** A `[target.'cfg(all())'] runner`
is skipped when the binary is started directly and is overridden by a
caller's `CARGO_TARGET_<triple>_RUNNER`, so it cannot be the half that fails
closed. Not the `ctor` crate either: two link-section statics do what it
does, without a new dependency in a public repository.

**Fail closed.** The copy does not trust the marker. Before any case runs it
confirms (`confirm`):

1. its environment's names are a subset of those seven
   (`env_exception!(names())`, the names alone, never a value);
2. the marker is 32 hex digits, and `HOME`'s last component is
   `xfx-test-home-<marker>`;
3. `HOME` is a directory, not a link, owned by this user, mode 0700
   *(round 5: not with a trailing `/`, which makes `lstat` follow a link;
   the copy now refuses that, and compares the whole path)*;
4. `HOME` is empty.

Anything else, and it prints which (names and paths it made, never a
value) and exits 101 before `main`: no case runs.

**New files are covered.** `tests/cli.rs` and `tests/parity.rs` now declare
`mod support;` too (`cli.rs` drops its own copy of `clear_env`). A future
`tests/*.rs` that does not is refused by `check-env-reads.py`: every
integration test target must report the `env_exception!`s of
`tests/support/isolated.rs`, which only `mod support;` compiles into it
(mutant `escape`).

**Regression** (`tests/isolation.rs`, a new suite): the binary, started with
the marker already set, three ways --
the sentinel's shape (`HOME` named after the marker, holding a
`.gitconfig`), an empty `HOME` the marker does not name, and the right
`HOME` beside a name it is not given (`XFX_MODEL`) -- refuses each for its
reason, and the ignored case it was asked for, which leaves a mark in its
working directory (that `HOME`), leaves none. A control, started as a
developer starts it, makes its copy and the case leaves its mark: so "no
mark" means "no case ran".

**RED: the inherited marker** (`red-marker.txt`, `red-marker.py`). The
permissions binary, started with `XFX_TEST_ISOLATED_COPY` already set and a
sentinel `HOME` holding a hook and a token-carrying `insteadOf`, asked for
`auto_does_not_run_git_in_a_directory_whose_configuration_defines_a_hook`;
`process_env!`'s build arms count their reads (names only):

- at `a327dd8a`, with round 3's marker `1` and with a well-formed one: the
  case ran in that process -- 2 reads of `HOME` by
  `permission::authority::minimal_environment`, under the sentinel home --
  and failed, decided by the sentinel's git configuration;
- at `dcb9c12a`: no case ran and no read was made. Marker `1` is refused
  as "not a name the copy's parent made", the well-formed one because
  "HOME is not empty". Exit 101 both times; no sentinel value in any
  output.

**GREEN under the sentinel environment, at `dcb9c12a`.**

- `green-lib.txt`: the lib suite, 4640 passed, 0 failed, 7 ignored; no
  sentinel value in the output.
- `integration-green.txt`: all 13 integration suites -- the 12 of round 3
  and `isolation` -- 0 failed: cli 82, gateway 93, interactive 79,
  isolation 1 (and its ignored marking case), ledger_compat 5, llmux 103,
  mcp 5, parity 26, permissions 159 (none of them wrapped any more),
  session_compat 9, sessions 65, tool_loop 109, tui 186.

**The enumeration, once more** (`integration-reads.txt`,
`integration-reads.py`: round 3's, now recording `env_exception!`'s arms
too, and whether the marker was set when the read was made). 17,251 reads:
15,352 by the `xfx` binary the suites start, 1,815 in the clean copies,
and 84 in the processes the developer started -- **0 of them through
`process_env!`**. The 84 are `tests/support/isolated.rs`'s own, before
`main` (thread `?`, the constructor): per binary, the marker, `PATH`,
`TMPDIR` twice (its own and `clear_env`'s), `RUST_BACKTRACE` and
`RUST_TEST_THREADS` -- six; `isolation` twice, for its control. As in
round 3, the instrumented run shows one interactive and eight tui
failures that the uninstrumented runs do not (`integration-green.txt`);
round 3 saw the same at both of its revisions. The instrumented build is
the only difference between the two runs; which of its writes the nine
cases trip on was not looked into.

## Part 3: the rest

**The SIGCHLD probe** (`process::tests::a_child_is_collected_even_when_xfx_was_started_ignoring_sigchld`)
is made with `command`, so in a unit test its environment is cleared down to
`PATH` and `TMPDIR` (`test_env::isolate_child`) and its role is the one
variable set on it; the `pre_exec` that ignores `SIGCHLD` is kept, and runs
after `command`'s own signal reset. Its role is read with
`env_exception!(var_os("XFX_TEST_SIGCHLD_PROBE"))`.

**The start rule's line check no longer skips `src/process.rs`.** It skips
that file's code before its test module -- where `command` makes the one
`Command` every other comes from -- and reads its test module like any
other; the check's own patterns are spelled in halves so the module does
not hold them. `in_tests` is live again for that reason; `tests/` is no
longer read at all (it was never flagged: `in_tests` started `true` there),
and the doc says why. The "one exemption" sentence is now a list of the
two: `process.rs`'s non-test code, and `tui/signals.rs`'s bare `/bin/sleep`
with its environment cleared. Mutant `probe-bare` puts the probe back on a
bare `Command::new`, and the check refuses it.

**The sentinel holds a token.** `../sentinel-run.sh` adds
`[url "https://x-access-token:xfx-sentinel-git-token@sentinel.invalid/"]
insteadOf = https://github.com/` to the sentinel home's `.gitconfig`;
`xfx-sentinel-git-token` is among the values every mutant and RED run is
searched for (counted, never printed). `scripts/check-no-secrets.sh` passes
with it.

**Receipt corrections.**

- `../round3/README.md`: the title and "The check now fails closed" no
  longer say it failed closed; the crate-wide import claim (its lines
  116-120) is withdrawn.
- `../README.md`: round 3's line no longer says the check fails closed;
  round 4's line is added.
- `../round2/release-compare-r2.txt:11`: "with the same toolchain" is marked
  inferred (the toolchain file was recorded after both builds).
- The `[GUARDED]` link went with the table it was in; `isolated.rs`'s doc
  names the marker and every other name it sets; `clippy.toml`'s rule
  comment names closures and references, and no binding check.

### The check's boundary

*Round 5 corrects this paragraph: it omitted `env!`/`option_env!`,
`std::env::temp_dir`, `libc::_NSGetEnviron`, the release profile and the
built artifacts, and "not a link" did not hold for a `HOME` with a trailing
`/`. The boundary as it stands is in [`../round5/README.md`](../round5/README.md).*

Clippy resolves each call's path, so an import, a rename, an alias of an
alias, a closure over a reader and a reference to one are the reader they
name, and `--force-warn` reports them whatever allow sits over them.
`env_exception!` takes literals only, and the check reads nothing of the
source but the compiler's own span of each invocation, which must be the
literal form. What this does not see: a C function declared by hand
(`extern "C" { fn getenv(..) }`; `src/local_time.rs:236` declares `tzset`,
which reads `TZ` inside the C library, as `localtime_r` does), the
`environ` static (a static, not a method, so no `disallowed-methods` entry
can name it), a read of `/proc/self/environ`, and a dependency's own reads.
In an integration test those all meet an environment built from nothing;
in a unit test they would meet the developer's. The system git
configuration is read by design (`src/test_env.rs:240-242`, round 3's 243-247); the global one
is `/dev/null` for a homeless plan. Doctests are not isolated; the library
has one, `compile_fail`, which runs nothing.
