---
title: TEST-ENV-ISOLATION round 2 receipt -- the reviews' eight items
type: receipt
tags: [xfx, phase5, tests, security, environment, receipt]
---

# TEST-ENV-ISOLATION, round 2

The eight items of the round-1 reviews (r1-gpt6, r1-opus, with the
dispatcher's decisions). Every figure below is from a file in this folder.

- **Round-1 head:** `e26e27dd`.
- **RED:** `945fc373` (items 2, 3 and 6: the cases and a test-only launch
  hook) and `red-1-5.py` on a throwaway clone of `e26e27dd` (items 1 and 5).
- **Fix:** `b61cc541`; `d00bf398` and `f4846c59` keep a release build's
  code as it was (item 1, "A build's code"). The code head is `f4846c59`.

How every run here was kept off the real environment: unit-test runs go
through `../sentinel-run.sh` (`env -i`, a temp home whose `.gitconfig` and
XDG git config define a hook, `xfx-sentinel-*` values for xfx's variables and
credentials). Mutants and fixtures run on a `git clone --no-local`
throwaway, never on the worktree.

## RED

`red-items-2-3-6.txt`, at `945fc373`, under `sentinel-run.sh`: 0 passed, 3
failed.

- item 3: `the worker's XFX_GATEWAY_URL is not the case's` (a launched
  worker lost the case's fake values once its runtime was built);
- item 2: `a git planned with no home read a global configuration file`
  (the sentinel hook, under whatever `HOME` git ends up with);
- item 6: `an https client a unit test built sent 1 request(s) to the
  process's proxy`.

`red-items-1-5.txt`, on the clone at `e26e27dd`:

- item 1: the round-1 scan **passed** each fixture -- an imported `var`, an
  aliased `std::env`, a key swap at an allowed site (`TMUX` to `HOME`);
- item 5: with the guard's test arms routed back to `std::env`
  (`guard-gone`), 10 of 17 `test_env` cases failed, and their output held
  2 of the 5 sentinel values (`xfx-sentinel-model` on 2 lines, the
  sentinel home on 1). That is the leak path: an `assert_eq!` printing what
  the real process held.

## The items

**1. clippy's environment rule.** `clippy.toml` disallows `std::env::var`,
`var_os`, `vars`, `vars_os`, `home_dir` and `libc::getenv`, the start
rule's form, checked on the resolved call. Each read that stays outside
`process_env!` carries `#[allow(clippy::disallowed_methods, reason = "env:
<variables> -- <why>")]` on its own statement, match arm or small function.
`src/test_env/tests.rs`:

- `clippy_disallows_every_reader_of_the_environment` keeps the list
  complete, as the start rule's own scan does for its entries;
- `every_read_outside_process_env_is_bound_to_its_variables` reads every
  `env:` allow in `src` and `tests`. It finds the variables its statement
  reads, resolving constants (same file first), and requires them to equal
  the reason's, and the file and variables to equal one of the 32 entries
  in `EXCEPTIONS`. It refuses an allow over a statement that reads nothing
  or that starts a child, and any exception that names a variable in
  `GUARDED`;
- `the_binding_check_refuses_a_swapped_variable_a_start_and_a_moved_constant`
  is the check's own fixtures;
- `process::tests::no_child_process_is_made_any_other_way` now accepts an
  allow of `disallowed_methods` outside `process.rs` only when its reason is
  `env: ...`.

`PATH` reads in unit tests go through `test_env::path()`, and in
integration tests through `support::sandbox::path()`. Each of those is one
bound exception. The negative fixtures are in `mutants.txt`.

**A build's code.** `release-compare-r2.txt` shows that a release build
here is not reproducible across hours: the base built at 22:24 and rebuilt
later differ by 6,470,042 bytes. Builds made back to back are
reproducible, so only back-to-back pairs count:

- with the allows moved into new `let` statements (`b61cc541`, then
  `{ let value = std::env::var(..); value }` in `process_env!` at
  `d00bf398`), 24,988 bytes of `__TEXT,__text` differ;
- with round 1's build arms, `__text` is identical, so the rest of the
  round-2 code compiles to the base's;
- `f4846c59` puts the allow on the one arm of `match () { ... }`.
  `release-compare-final.txt` (base and `f4846c59` back to back): same size
  (11,668,096 bytes), identical section table, `__TEXT,__text` identical.
  The 559 differing bytes are 161 of panic `Location` line numbers (0
  unexplained), 16 of `LC_UUID` and 382 of the code signature.

**2. A git planned with no home of its own.** It gets
`GIT_CONFIG_GLOBAL=/dev/null` (`test_env::without_a_global_git_configuration`,
called from `permission::authority` under `cfg(test)`). A case that gives a
home on purpose keeps it, and `GitFixture` now plans under its own home, so
its proofs still read the fixture's global configuration. System
configuration stays as a build reads it, because it is the machine's, not
the person's. The empty-`HOME` assertion is replaced by
`a_git_planned_with_no_home_reads_no_global_configuration_wherever_home_is`:
a sentinel hook in both global files under a home the case writes is not
read. Its control shows the sentinel is what git reads when nothing
excludes it.

**3. A launched worker keeps the case's fake.**
`FakeProcess::over_this_thread`: **the runtime's environment wins for what
it holds -- its `HOME` and each variable it sets -- and the thread's fake
process fills the rest; with none, the environment alone is the process.**
The regressions are `a_launched_worker_reads_the_whole_fake_process_it_was_launched_in`
(the real `spawn`, with `XFX_GATEWAY_URL`, `GIT_CONFIG_GLOBAL`,
`GIT_CONFIG_NOSYSTEM` and `ZDOTDIR` read back on the runtime thread) and
`an_environment_laid_over_a_fake_process_wins_where_it_holds_and_no_further`.

**4. The receipt's integration inventory.** `../README.md` §3 now lists
`PATH` and `TMPDIR` (in `clear_env` and `cli.rs`) and `CARGO_PKG_NAME`.

**5. No assertion prints a value.** Every comparison of a value read from
an environment in the unit's tests (`src/test_env/tests.rs`,
`src/tui/signals.rs`, the new worker case) is `assert!` with the
variable's name only (`same(equal, which)`). Pinned by `mutants.txt`: every
run is searched for the sentinel's five values.

**6. reqwest's proxies.** `bearer_rules` is the one place a client that may
send a credential is built with a proxy kept, for https only. Under
`cfg(test)` it now calls `test_env::proxies`:

- `no_proxy()`, then the fake process's `HTTPS_PROXY`/`HTTP_PROXY`/
  `ALL_PROXY` (either case), with its `NO_PROXY`;
- a copy of the test binary that `test_net` started keeps reqwest's own
  reading, since proving that reading is what that copy is for and its
  parent built its environment from nothing.

`https-clients.txt`: in the whole lib suite, 25 https bearer clients
are built, on 18 threads -- 16 `tui::worker` cases, the fake-proxy case
and one MCP HTTP thread. Before round 2, each of them built its client
with reqwest reading the process's proxy variables, the developer's when
the suite runs in a shell that sets one. `an_https_client_takes_the_fake_processs_proxy_and_its_exemptions`
is the way in.

**7. The bare git fixtures.** `src/tui/file_index.rs`'s
`a_work_tree_is_listed_by_git_and_its_ignore_rules_hold` now goes through
`test_env::isolate_child`, and `tests/permissions.rs`'s `git` fixture
through `support::sandbox::clear_env`. Both start from `PATH` and `TMPDIR`
only, so no `GIT_DIR`, `GIT_CONFIG_COUNT`/`KEY_n`/`VALUE_n` or
`GIT_CONFIG_PARAMETERS` reaches them. Both pass in the gate.

**8. CATALOG-REFRESH's fixture guard.** Kept; `../README.md`'s Overlap
section says why.

## GREEN

`green-r2.txt`: the whole lib suite at `b61cc541`, under `sentinel-run.sh`,
4642 passed, 0 failed, and no case failed with a refusal. Since then,
`d00bf398` and `f4846c59` only move allows, and the gate below ran at
`f4846c59`.

## Mutants and fixtures

`mutants.txt`, on the clone at `f4846c59`: **21 entries, 21 killed, 0
survived; 0 runs with a sentinel value in their output.**

- Round 1's ten mutants, at round 2's text: the guard's test arms, the
  refusal, the config reader's variables and `HOME`, the Gateway and
  git-selector readers, the `HOME` handed to git, the child isolation, the
  thread carry, the MCP fallback.
- Round 2's four: `overlay-gone` (item 3), `global-git-gone` (item 2),
  `proxy-rule-gone` and `fake-proxy-ignored` (item 6).
- Item 1's seven fixtures. Clippy fails on an imported `var`, an aliased
  `std::env` and a reader passed as a function (`use of a disallowed method
  std::env::var_os`). The binding check fails on a key swap at an allowed
  site, the same with its reason changed, the same with its `EXCEPTIONS`
  entry changed too (now a guarded name), and an `env:` allow over a
  `Command`.
- Item 5's pin: `guard-gone` failed 12 cases, and none of their output holds
  a sentinel value, where round 1's held 2 of 5 (`red-items-1-5.txt`).

## Gate

`gates-r2.txt` (at `b61cc541`): all 8 steps exit 0; `cargo test` 15 suites,
5564 passed, 0 failed, 7 ignored; with `fault-injection` 5601 passed, 0
failed, 7 ignored. `gates-final.txt` (at `f4846c59`, under the lock, short `TMPDIR`): all 8
steps exit 0 -- fmt, clippy and clippy with `fault-injection`, `cargo test`
15 suites 5564 passed 0 failed 7 ignored, with `fault-injection` 5601
passed 0 failed 7 ignored, `check-xfx-identity.sh`, `check-no-stubs.sh`,
`check-no-secrets.sh`.
