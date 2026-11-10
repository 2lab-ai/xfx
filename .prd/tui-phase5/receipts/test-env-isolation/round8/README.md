---
title: TEST-ENV-ISOLATION round 8 receipt -- cfg_select! arms checked; the instrumented run's failures were its own log
type: receipt
tags: [xfx, phase5, tests, security, environment, receipt]
---

# TEST-ENV-ISOLATION, round 8

The round-7 reviews' MUST, SHOULDs and NITs, in the same design. Every
figure below is from a file in this folder.

- **Round-7 head:** `217d793a` (code `3de3d2dd`). **Code:** `92b6ebca`.
- **Toolchain:** rustc 1.99.1, clippy 0.1.99, aarch64-apple-darwin.
- **Linux: not yet run.** The ubuntu CI rows are its first run, and must be
  green before merge.

## MUST: `cfg_select!` (opus M1)

**The hole.** `cfg_select!` (std, stable since Rust 1.95; the MSRV is 1.96)
picks the first arm whose cfg predicate holds. An arm `clippy => {}` ahead
of `_ => { .. }` hides the second arm's code from clippy exactly as
`#[cfg(not(clippy))]` does, and round 7's `CFG_CALL` matched only `cfg`,
`cfg!` and `cfg_attr`.

**The fix** (`cfg_select_predicates`, `scripts/check-env-reads.py`).
`\bcfg_select\s*!\s*[({\[]` is matched on the lexed code, so a comment or a
string never matches; each arm's predicate -- the text before its `=>` at
the macro body's top level -- is tested with `CLIPPY_WORD`; an arm's body,
braced or an expression up to the next top-level `,`, is skipped, so
`#[allow(clippy::x)]` inside an arm is not a predicate. `clippy.toml`,
`src/lib.rs` (`process_env!`'s doc) and this README name `cfg_select!`
beside `cfg`, `cfg!` and `cfg_attr`.

**Fixtures** (self-test), each refused: `cfg_select_braces`,
`cfg_select_parentheses`, `cfg_select_brackets`, and
`cfg_select_expression_arms` (`cfg_select! { unix => 1, not(clippy) => 2,
_ => 3 }`); accepted: `cfg_select_naming_a_clippy_lint_in_an_arm`
(`#[allow(clippy::needless_return)]` in an arm's body).

**RED** (`red.txt`, `red.py` on the clone): opus's plant -- `cfg_select! {
clippy => {} _ => { #[test] fn .. { std::env::var_os("HOME"); } } }` in
`src/test_env/tests.rs` -- passes clippy `-D warnings` and the check at
`217d793a`, and is refused at `92b6ebca`: "src/test_env/tests.rs:32: a
cfg_select! arm whose predicate names clippy, under which code is never
linted". clippy passes it at both, because it never compiles the hidden arm.

## SHOULD 1: the instrumented run's three failed suites (opus S1)

Round 7's `integration-reads.txt` failed interactive 1, isolation 1 and
tui 8, and round 7's README did not say so. **Cause: the instrumentation's
own log, confirmed by running it** (opus's [가설], confirmed and wider than
the one case). Round 4's instrument appends each read to
`$TMPDIR/xfx-env-reads.log`, with the reading process's TMPDIR. Every
failing case gives a process a TMPDIR of its own and then checks what is in
it.

`integration-reads-under-tmpdir.txt` (round 4's log place, at `92b6ebca`)
fails with the same per-suite counts as round 7 -- interactive 1, isolation
1, tui 8; round 7's file names no case -- and names these ten:

- **isolation:** `a_parent_under_a_relative_tmpdir_makes_no_home_and_runs_no_case`
  -- "a home was made under a relative TMPDIR". The case makes
  `relative-tmp` first and requires it empty afterwards
  (`tests/isolation.rs:163`). The parent's constructor reads TMPDIR through
  `env_exception!` (`tests/support/isolated.rs:91`), whose instrumented arm
  then wrote
  `relative-tmp/xfx-env-reads.log`. The refusal itself held: the case's
  first assertion ("TMPDIR is not an absolute path") passed.
- **interactive:** `slash_trace_writes_a_private_report_about_this_shells_conversation`
  -- `reports_into` sets the xfx binary's TMPDIR to the reports folder
  (`tests/support/fake_clipboard.rs:94`), `reports()` lists every entry
  in it (`:99`), and the case asserts one report (`tests/interactive.rs:1235`):
  the report and the log were two.
- **tui, the same report count:** `slash_trace_writes_a_private_report_and_copies_the_file_rather_than_its_text`,
  `a_quit_right_after_slash_trace_leaves_the_whole_report_and_no_clipboard_tool_behind`,
  `a_reply_a_terminal_draws_wider_is_named_on_the_screen_and_kept_as_written_everywhere_else`,
  `compact_with_no_credential_is_not_started_and_asks_nobody`,
  `every_slash_command_is_answered_by_the_session_rather_than_by_the_model`
  ("/trace wrote no report", the same one-report assertion at
  `tests/tui.rs:5111`), and
  `trace_records_a_failed_compaction_and_the_one_after_it`. That last one
  timed out: it counts the reports before `/trace` (`tests/tui.rs:12533`),
  the log among them, and waits for one notice more than that (`:12536`).
- **tui, an empty TMPDIR:** `a_clipboard_read_goes_under_the_real_tmp_whatever_tmpdir_says`
  ("TMPDIR=..: the read wrote under it", `tests/tui.rs:12030`) and
  `with_no_usable_home_an_attached_files_copy_never_lands_under_a_relative_tmpdir`
  ("the copy landed in the sandbox", `tests/tui.rs:1354`).

**The fix, in the receipt script only** (`integration-reads.py`): the log
goes to one absolute path fixed when the clone is built
(`env!("XFX_ENV_READS_LOG")` in the instrumented code), so no process's
TMPDIR holds it. `--log-under-tmpdir` keeps round 4's place, and each
failing case is named with its first panic line.
`integration-reads.txt` (the fixed log, `92b6ebca`): **all 13 suites
pass**, isolation 4 and its ignored marking case, tui 186. 17,737 reads:
15,785 by the `xfx` binary the suites start, 1,864 in clean copies, and 88
in the processes the developer started (the constructor's own), **0 of
them through `process_env!`**. The 10 reads more than round 4's place
counted are all in isolation (the two files' tables differ there only):
the parents its cases start run under TMPDIRs of their own, where round 4's
place wrote, and lost, their lines. The other totals rise too (15,785
against 15,350 by the `xfx` binary, 1,864 against 1,829 in copies): round
4's place reads back only `$TMPDIR/xfx-env-reads.log` of the script's own
TMPDIR, so every total in rounds 4-7, all taken there, was a lower bound.

## SHOULD 2: lexer fixtures (opus S2, gpt6 SHOULD 3)

- Refused: `cfg_after_a_lifetime` -- `pub fn f<'a>(x: &'a u8) -> &'a u8 {
  #[cfg(not(clippy))] let _ = 'b'; x }`. A lexer that took `'a` for the
  start of a char literal would blank up to each next quote, the cfg among
  it.
- Accepted: `cfg_in_a_comment_or_a_string` now also holds `/* /* */
  cfg(not(clippy)) */` and `b")"`.
- `code_of` alone, with no cargo build ("lexed comments, literals and
  lifetimes"). Nine texts, each compared with its exact expected output,
  and each keeping its length and every newline's offset: nested comments,
  a line comment, a byte string and a byte char, an escaped quote in a
  string, `'\u{1F600}'`, `'\''`, a lifetime, a raw string holding `")`,
  and newlines inside a string and a comment.

## NITs

- `round7/README.md` names `3de3d2dd` beside `8fd9d62c`, and says its
  instrumented run failed three suites. This README names those failures.
- `round7/mutants-first-run.txt` and `round7/mutants-rerun.txt` begin with
  the revision they ran at. `mutants.txt` here does too.
- A source compiled from outside the repository is reported by its path
  relative to the repository (`../outside/y.rs` in the fixture), not its
  base name.
- The uplift trace: an `os.stat` that fails, on the uplifted file or on a
  candidate, is no answer. The artifact is then refused as "built with no
  dep-info", where it used to raise.

## Mutants

`mutants.txt` (`mutants-r8.py` at `92b6ebca`, the cfg rules only; the
baseline passing first). `cfg-select-dropped` (no `cfg_select!` matched),
`cfg-select-braces-only` (only `{`), `cfg-select-scans-bodies` (an arm's
body read as its predicate), `cfg-select-plant` (opus's plant) and
`cfg-check-gone` (re-anchored to round 8's text): **5 of 5 killed**, each
for its reason. The braces-only mutant is refused by the parentheses and
brackets fixtures, and the body mutant by the accepted lint-in-an-arm
fixture. No run had a sentinel value in its output. The other 67 entries
are round 7's, which round 8 does not touch. They apply at `92b6ebca` (the
harness asserts every edit applies) and were not rerun.

## Gate

`gates.txt` (`../gates.sh` under the lock, on the clone at `92b6ebca`):
fmt, both clippy runs, both test runs (lib 4638, and 4661 with
`fault-injection`; every integration suite, isolation 4 and its ignored
marking case), identity, stubs, secrets, `check-env-reads.py` (39
invocations in 26 files, 12 `process_env!` call sites, 14 plain calls, 70
compile-time reads of 6 names, 52 test binaries started) and its self-test
(62 fixtures, 0 not as expected; the lexer's nine texts count as one of
the 62, and `gates.sh` does not keep that check's own line): every step
exit 0. No release build was compared: outside tests and the script, the
code change is comments in `clippy.toml` and `src/lib.rs`.
