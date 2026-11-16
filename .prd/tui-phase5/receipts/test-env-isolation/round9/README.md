---
title: TEST-ENV-ISOLATION round 9 receipt -- cfg_select! predicates read without finding where an arm ends
type: receipt
tags: [xfx, phase5, tests, security, environment, receipt]
---

# TEST-ENV-ISOLATION, round 9

The round-8 reviews' MUST and opus's NITs, in the same design. Every figure
below is from a file in this folder.

- **Round-8 head:** `09e9d822` (code `92b6ebca`). **Code:** `1490f498`.
- **Toolchain:** rustc 1.99.1, clippy 0.1.99, aarch64-apple-darwin.
- **Linux: not yet run.** The ubuntu CI rows are its first run, and must be
  green before merge.

## MUST: one rule for `cfg_select!` arms

**The two defects, one root.** Round 8 found where an expression arm ends
by scanning to the next top-level `,`:

- **A bypass** (the dispatcher's r8-probe `d.rs`, compiles on 1.99.1). An
  arm needs no comma after `if .. else { }`. In `windows => if true { 0 }
  else { 0 } not(clippy) => .., _ => 7`, the scan swallowed `not(clippy)`.
- **A false refusal** (gpt6's MUST, r8-probe `b.rs`, compiles). The comma
  of `Result::<u8, u16>` ended the arm early. So `allow(clippy::..)` in its
  body was read as the next predicate.

**The fix** (`cfg_select_predicates`, `scripts/check-env-reads.py:912`).
It no longer looks for where an arm ends. In the macro's body, after
lexing, every nested `()`, `[]` and `{}` group is blanked. Each `=>` left
is then at the top level, because `=>` never stands at the top level of an
expression (opus). The text before it, back to the previous `=>` or the
body's start, is one segment, and a `clippy` word in it is refused. The
segment's last group, when it ends the segment, is the predicate's own
(`not(..)`, `all(..)`) and is read as written. No comma, brace or generic
can hide a predicate. A `clippy` identifier at the top level of an
expression arm would be refused too, which fails closed. `allow(clippy::x)`
in a body sits in a blanked group and is still accepted.

**Fixtures** (self-test, each compiled by rustc):

- refused: `cfg_select_after_an_if_else_arm` (d.rs's shape) and
  `cfg_select_after_a_braced_arm` (opus's SHOULD: `(` as the delimiter, an
  earlier braced arm nesting `(` and `[`, no comma);
- accepted: `cfg_select_a_generic_comma_in_an_arm` (b.rs's shape) and
  `cfg_select_a_clippy_name_in_a_braced_arm` (`pub fn clippy()` inside a
  body).

Round 8's five `cfg_select` fixtures keep their verdicts. In `gates.txt` the
self-test lists every one.

**RED** (`red.txt`, `red.py` on the clone):

- **d.rs's shape** over a `HOME` read in `src/test_env/tests.rs`: passes
  clippy `-D warnings` and the check at `09e9d822`, and is refused at
  `1490f498` ("src/test_env/tests.rs:34: a cfg_select! arm whose predicate
  names clippy"). At both revisions, `--list` lists
  `test_env::tests::where_clippy_never_looks` in the lib's unit-test build.
  Run alone under `../sentinel-run.sh`, it passes. It asserts that the
  arm's `HOME` read returned 1, so the arm clippy never compiles is the one
  the test build took.
- **b.rs's shape** (`#[allow(clippy::needless_return)]` inside
  `Result::<u8, u16>::Ok({ .. })`): refused at `09e9d822`, the false
  refusal, and accepted at `1490f498`. clippy passes both at both
  revisions.

## NITs (opus)

- **`../gates.sh`:** keeps `^lexed|NOT LEXED`, and keeps every line of
  the self-test step instead of the last 60. So `gates.txt` holds every
  fixture line.
- **round8/README.md:** "the same per-suite counts", not "the same ten
  cases" (round 7's file names no case). Also, from the code: round 4's
  instrument wrote to each process's own TMPDIR
  (`round4/integration-reads.py:32`) but read back only the script's
  (`:72`). So every total in rounds 4-7 was a lower bound, and the
  `xfx`-binary and copy totals rose too.
- **The uplift trace** (`check-env-reads.py:377`, `:385`): a dangling
  candidate is skipped (`continue`), because it can match neither the
  inode nor the bytes; before, it gave up on the whole artifact. A
  candidate `same_bytes` cannot open compares unequal instead of raising.
  The self-test's "dep-info by artifact kind" now has a copy beside a
  dangling link and an unreadable file of its size, all three named like
  its original; it must resolve to the readable original.
- **`red.py`:** `--list` and the run alone, above.

## Mutants

`mutants.txt` (`mutants-r9.py` at `1490f498`): every entry whose name holds
`cfg-`, plus `lexer-gone`, `compiled-files-unread`, `uplift` and
`same-bytes`. The baselines passed first, then **21 of 21 were killed**,
each for its reason. No run had a sentinel value in its output.

Round 9's own:

| mutant | killed by |
|---|---|
| `cfg-select-comma-arm-end` (round 8's scanner back) | after_an_if_else_arm not refused; generic_comma refused |
| `cfg-select-brace-as-expression` (`{}` not a group) | a_clippy_name_in_a_braced_arm refused |
| `cfg-select-closing-braces-only` (`closing()` counts only `{}`) | after_a_braced_arm not refused |
| `cfg-select-predicate-group-blanked` (`not(..)` left blank) | expression_arms, after_an_if_else_arm not refused |
| `cfg-select-if-else-plant` (d.rs's shape over `HOME`) | the check, tests.rs:34 |
| `uplift-dangling-gives-up` | NOT RESOLVED, the dangling-and-unreadable copy |
| `same-bytes-raises` (catches only `FileNotFoundError`) | the self-test raises (PermissionError, reproduced offline): exit 1 |

`cfg-select-scans-bodies` was re-anchored to round 9's yield and is killed
by all three accepted fixtures. Round 8's other `cfg_select` mutants and
every earlier cfg, lexer and compiled-file mutant were killed again. So was
`expect-and-cfg-attr`, which the `cfg-` filter picked up by its name. The
harness asserts that all 79 entries (round 8's 72 and these 7) apply at
`1490f498`. The other 58 were not rerun: round 9 does not touch their
rules.

## Gate

`gates.txt` (`../gates.sh` under the lock, on the clone at `1490f498`):
fmt, both clippy runs, both test runs (lib 4638, and 4661 with
`fault-injection`; every integration suite, isolation 4 and its ignored
marking case), identity, stubs, secrets, `check-env-reads.py` (39
invocations in 26 files, 12 `process_env!` call sites, 14 plain calls, 70
compile-time reads of 6 names, 52 test binaries started) and its self-test:
66 checks, 0 not as expected, and every line is now kept -- 61 fixtures,
then the dep-info cut, `lexed`, `resolved`, the test binaries and the
clippy configuration. Every step exit 0. No release build was compared:
round 9 changes nothing outside the script and the receipts.
