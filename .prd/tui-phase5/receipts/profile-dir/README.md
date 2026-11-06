# profile-dir: every writer that can make `~/.xfx` makes it private

The US-U2 review left this residual: "A skills command that creates a missing
`~/.xfx` with `create_dir_all` makes it 0755. The session store and the usage
cache then refuse it until the user runs `chmod`." The session store refuses
a `~/.xfx` that is a link, not a directory, another account's, or open to
group or other (`verify_store_dir`). Every other writer in the profile made
the directory its own way, and none of them held an existing one to that rule.

Base: `main` @ `5ca0bc18`. Round 1 is `f38b6e98`; round b (PROFILE-DIRb, the
two-engine review's items) is this commit. Binaries: base `f2b1844c…`, head
`c5904eaa…` (sha256 prefixes of `target/release/xfx`).

## Every site, traced

| Site (base) | What it did | Now |
|---|---|---|
| `src/skills/command.rs:188` `create` | `create_dir_all(~/.xfx/skills/<name>)`: `~/.xfx`, `skills/`, `<name>/` all `0755` under umask 022; an existing open or linked `~/.xfx` written into | `install::ensure_managed` → `profile_home::ensure_content` |
| `src/skills/install.rs:198`, `:285` | `create_dir_all(managed)`: same | `ensure_managed(managed, managed)` |
| `src/skills/install.rs:317` `transaction_root` | `fs::create_dir`, umask | `profile_home::make_new` (`0700`) |
| `src/skills/install.rs:358` `lock` | `DirBuilder.mode(0o700)` for `.skill-install-locks`; an existing open one used | `profile_home::ensure(parent, &dir)`: private state |
| `src/skills/install.rs:406` `copy_tree` | `create_dir_all`, umask, for every copied directory | `profile_home::make_new` (`0700`) |
| `src/provider/profile.rs:645-663` `create_private_dir` (5 callers: preferences, `/allowlist`, `edit_document`, `rewrite_document`, `write_document`) | recursive `0700` when missing, but an existing `~/.xfx` passed `is_dir()`: a link was followed and an open one written into; a missing home made | deleted; the 5 callers call `profile_home::ensure(dir, dir)` before their lock |
| `src/mcp/config.rs:707-723` `create_private_dir` | same | deleted; `profile_home::ensure`, refusals in upstream's error names |
| `src/session/prompt_history.rs:340-356` `create_private_dir` | same | deleted; `profile_home::ensure` |
| `src/session/store.rs:3376` `ensure_private_dir` | `create_dir_all(parent)`: a missing home made `0755`, and -- for `sessions.staged` with `~/.xfx` gone under a running session -- `~/.xfx` itself made `0755` | the parent step is deleted; `SessionStore::open` makes `~/.xfx` through `profile_home::ensure`; `verify_store_dir` and `InsecureParent`'s sentence are `profile_home::problem` / `refusal` |

Found and left alone, each with its reason:

- `src/session/store.rs:1788` (a session's directory) and `:2834` (its
  `images/`): `fs::create_dir` then `chmod 0700`, inside `sessions/`, which is
  already checked private.
- `src/usage.rs:505` `start_tracking`: a file (`create_new`, `0600`), never a
  directory; it fails when `~/.xfx` is missing.
- `src/trace.rs`, `src/clipboard.rs:483`, `src/tui/clipboard_image.rs`
  (through `clipboard::private_directory`): `$TMPDIR`, not the profile.
- `src/tools/mutate.rs:1436`, `:1460`: the model's write tool, in the
  workspace; any path through a `.xfx` is refused
  (`PROTECTED_WRITE_DIRECTORY_NAMES`). Workspace `.xfx/` directories share no
  code path with these writers, so they are out of scope.
- The usage cache (US-U2) is not on `main` yet; its own
  `open_or_create_profile` already makes `~/.xfx` `0700` and refuses an open
  one.

## Round b: what changed after the review

1. **Private state and user content below `~/.xfx`**, split as fx splits
   them. `ensure` is for private state: an existing level is held to
   `~/.xfx`'s rule, and a link, a file, another account's or an open one is
   refused (fx: `openOrCreateVerifiedPrivateChild`, `follow_symlinks = false`).
   `.skill-install-locks/` goes through it. `ensure_content` is for the
   managed skills root and a skill's directory: an existing level is used as
   it is, and a link to a directory is followed (fx: `createSkillTemplate`
   → `makeDirRecursive`). So a `~/.xfx/skills` linked into dotfiles keeps
   working, and `/skills create` writes where it points, as fx does.
2. **A missing home is refused** (`NotFound`, `the home directory <path> is
   not there, and xfx does not make one`), as fx refuses it. The base made it,
   `0755` through the store and `/skills`, and `0700` through the other
   writers (`red-b-base.txt`); round 1 made it `0700`.
3. **The trust model is in the module doc**: the checks are path-based, with
   a trusted `$HOME` and stable ancestors. Named residuals: an opener that
   pins the directory it checked (`O_NOFOLLOW | O_DIRECTORY`, `fstat`, `*at`,
   as the usage cache's `open_dir_in`), and ACLs, which the mode bits do not
   show on macOS.
4. **An open home names its fix**: the sentence ends
   `` (`chmod 700 <path>` makes it private) ``, the path one shell word.
5. **The scan** (`src/profile_home/tests/scan.rs`) reads every product `.rs`
   file under `src/` without comments, literal contents, or any `#[cfg(test)]`
   / `#[cfg(all(test, …))]` item or statement, each cut to its own end, so
   code after an inline test module is read (fixture
   `a_test_module_is_cut_to_its_own_end_and_the_code_after_it_is_read`). A
   test module's file, and every module declared inside one, is skipped by
   its declaration, not its name. In what is left, `create_dir_all(`,
   `create_dir(`, `DirBuilder::new(`, `mkdir(` and `mkdirat(` appear only on
   the nine lines `ALLOWED` names, each exactly once. It does not see a call
   under another name: an import alias, a wrapper, or an unlisted crate.
6. **Behavioural pins** for what round 1 pinned only by source:
   - store: `a_profile_home_gone_before_the_stage_is_made_is_not_made_again`
     (a test-only hand in `fork` removes `~/.xfx` after the source is read);
   - `lock()`: `an_open_private_level_is_refused_before_a_lock_is_taken_in_it`
     and `a_private_level_that_is_a_link_is_refused_and_its_target_left_alone`;
   - refuse-before-lock: `/allowlist`, `edit_document` and `rewrite_document`
     joined the writer table, so the open and linked cases assert nothing,
     `settings.lock` included, is made in a refused home; since round c the
     missing-home case runs the whole writer table too, and asserts no home is
     made.
7. **Bounded child waits**: `crate::process::output_within` (test-only)
   kills and reaps a child past its limit
   (`a_child_past_its_limit_is_killed_and_reaped` checks `ESRCH`, so no
   zombie is left). It is used by this module's umask case and by
   `fork/tests.rs`'s.

Test fixtures that let their temporary home be dropped, or never made it,
and relied on xfx making it again now keep or make it
(`tui/event_loop.rs`'s `shell_of`, three `skills/install.rs` cases, one
`tui/shell.rs` case).

## Round c: the gate-only follow-up

1. **A relative path is quoted.** `shell_word` printed a path with a leading
   `~`, `=` or `-` as it was, so with a relative `HOME` the printed
   `chmod 700 ~x/.xfx` named another directory. Any path that does not start
   with `/` is now single-quoted and said from `./`, so `~` and `=` are not
   expanded and `-` is not read as an option: `'./~x/.xfx'`, `'./=x/.xfx'`,
   `'./-x/.xfx'`.
2. **The missing-home case runs every writer**
   (`a_missing_home_is_refused_by_every_writer_and_nothing_is_made`): each
   one is refused, in its surface's words, and no home is made. `/mcp` says
   `McpConfigPathInvalid` and `/skills add` says upstream's one install
   failure sentence, as every other failure there.
3. **`output_within` bounds its reads too.** Once the child has exited, its
   output is waited for only for what is left of the limit
   (`a_child_whose_output_outlives_it_is_given_up_on_at_the_limit`: a
   grandchild holds the pipes; the test ends it by the pid its parent
   wrote).

RED on round b's tree (`red-c.txt`, the new tests on a throwaway clone at
`e8753b8f`): the quoting case fails with ``chmod 700 ~x/.xfx`` unquoted; the
reader case waits the grandchild's full 30 s and returns `Ok`. The missing-home
loop passes there: the behaviour was right, the README sentence was not.

## Round c, item 4: the merge of `main` after #57

`737f7477` merges `origin/main` @ `d314549e` (#57 the usage cache, #60
PROC-SPAWN, #61 FLAKE-MDTIME). One conflict, the fork umask case's child
start: main's `process::output` against this branch's bounded
`process::output_within`. The bounded wait is kept, and `output_within` now
starts its child through `process::spawn`, under the one start lock, as
PROC-SPAWN's `clippy.toml` requires.

- **The scan, on the merged tree**, failed closed on the cache's two
  `rustix::fs::mkdirat` lines, as expected, and on two false positives:
  `open_or_create_dir(` read as `create_dir(`. The two lines are allowed,
  with the reason in `ALLOWED`: the cache's fd-based opener
  (`O_NOFOLLOW | O_DIRECTORY`, `open_dir_in`) refuses an open or linked
  profile, the stronger model this module's doc names as the residual. A
  banned spelling now has to be a call, not the tail of a longer name
  (`a_banned_spelling_is_a_call_and_not_the_tail_of_a_longer_name`).
- **The module doc and the `docs/parity.md` row** name the cache as the one
  writer that makes `~/.xfx` its own way: `mkdirat` `0700` on the home's
  descriptor, `fchmod`ed only when it made it, an existing one that is a link
  or not private refused, a missing home refused (`open_home`), its refusals
  in its own words. It is a library with no caller yet. The residual's
  `open_dir_in` now names a function on this tree (`src/usage/cache.rs`).

## RED

- **Round 1, on the base** (`red_profile_home.rs`, `red-unit.txt`, on a
  `git clone --no-local` of `5ca0bc18`): `/skills create` and `/skills add`
  leave `755 .xfx`, `755 .xfx/skills` and `755 .xfx/skills/demo`, and the
  store then refuses the home. Six of the seven writers write into a `0755`
  `~/.xfx` and through a linked one.
- **Round b, on the base** (`red-b-base.patch`, `red-b-base.txt`):
  - all nine writers make a missing home;
  - `/allowlist`, a settings edit and `xfx setup`'s rewrite write
    `settings.json` and `settings.lock` into an open home;
  - `/skills add` takes its lock in an open `.skill-install-locks`;
  - the store's refusal names no `chmod`;
  - a fork whose `~/.xfx` is removed before its stage is made makes it again
    at `755`. The patch adds the same test-only hand to the base's `fork`.
- **Round b, on round 1** (`red-b-round1.patch`, `red-b-round1.txt`, on
  `f38b6e98`): `ensure(~/.xfx, ~/.xfx/.skill-install-locks -> elsewhere)` is
  `Ok`, and round 1's cut keeps nothing after a test module. Through
  `/skills add`, a linked lock directory was already refused on the base and
  on round 1, by `lock()`'s own `symlink_metadata` check.
- **Binaries** (`bin-base.txt`, through `bin_pairs.sh` and `drive_tui.py`:
  a pty, a 40 s alarm, `/quit`, a fresh `HOME` per case, umask 022):
  - `xfx setup gateway` writes into an open home and through a linked one,
    and makes a missing `HOME`; `xfx sessions` then refuses the open and
    linked homes.
  - In the TUI the store opens first, so `~/.xfx` is `700` and only `skills/`
    and `skills/demo` are `755`.
  - `~/.xfx` itself comes out `755` when it is removed under the running
    session and `/skills create` makes it again (`tui-gone`).

## GREEN

- `bin-fix.txt`: a fresh home is `700` all the way down on every surface,
  `tui-gone` included, and `xfx sessions` opens it.
- `xfx setup gateway` exits 1 in three cases, each leaving the home as it
  was:
  - on an open home, with the store's sentence, which ends
    `` (`chmod 700 <path>` makes it private) ``; the home stays `drwxr-xr-x`
    and empty;
  - on a linked home, with the store's link sentence; nothing appears where
    the link points;
  - with no `HOME`, with `the home directory … is not there, and xfx does not
    make one`; `HOME` is still not there.
- The unit pins are `src/profile_home/tests.rs` (15 tests) and
  `tests/scan.rs` (4), with the fork pin and the three `output_within` tests.

## Mutants

On a throwaway clone (`mutants.py`, `mutants.txt`), each restored after its
run, every one fails a named test:

| Mutant | Fails, among others |
|---|---|
| M1 drop `.mode(0o700)` from the one-level make | `every_writer_makes_the_profile_home_private_under_any_umask` |
| M2 `/skills create` bypasses the helper | `every_writer_makes_a_missing_profile_home_private_and_the_store_opens_it` |
| M3 the settings document writer bypasses it | `an_open_profile_home_is_refused_by_every_writer_in_the_stores_words_and_never_chmodded` |
| M4 an existing home is not checked | `a_linked_profile_home_is_refused_by_every_writer_and_nothing_is_made_where_it_points` |
| M5 a missing home is made again | `a_missing_home_is_refused_and_nothing_is_made` |
| M6 a private level is followed like content | `a_private_level_that_is_a_link_is_refused_and_its_target_left_alone` |
| M7 `/allowlist` locks before the home is checked | `an_open_profile_home_is_refused_by_every_writer…` |
| M8 the store makes a missing parent again | `a_profile_home_gone_before_the_stage_is_made_is_not_made_again` |
| M9 the cut stops at the first test module | `a_test_module_is_cut_to_its_own_end_and_the_code_after_it_is_read` |
| M10 the bounded wait does not kill | `a_child_past_its_limit_is_killed_and_reaped` |

## fx v0.0.16

- `openOrCreateVerifiedPrivateChild`, `src/core/shared/io.zig:712-742@481104fd`:
  opens without following a link, creates `0700`, `chmod`s an existing
  directory to `0700` and checks it. xfx refuses instead (the session store's
  rule); `docs/parity.md`, the `profile home ~/.xfx` row.
- `Store.initFromHome`, `src/core/config/settings_store.zig:340-381@481104fd`:
  a missing home is an error (`:343-347`); a writable open chmods `~/.fx`
  (`:370-373`).
- `acquireSkillInstallLock`, `src/builtins/skills.zig:684-702@481104fd`: the
  lock directory through `openOrCreateVerifiedPrivateDirFromDir`.
- `createSkillTemplate`, `src/builtins/skills.zig:247-252@481104fd`,
  `createInstallTransaction` `:669`, `ensureDir` `:786-798`, `makeDirRecursive`
  `src/core/shared/io.zig:1077-1093@481104fd`: `.default_dir`, links followed.
- `profileDir`, `src/core/mcp_host/config_file.zig:147-155@481104fd`: the MCP
  profile goes through `openOrCreateVerifiedPrivateDir`, whose errors are the
  names `/mcp` says.

Every file here went through `scripts/fxcap/redact.py` (this one through its
check): `<tmp>` is the gates' short `TMPDIR`, `profile-dir` in the harness
temp folder; a unit test's HOME is `<bench-HOME>`. The binary pairs name each
case's HOME directory `HOME`. The scripts are evidence of what ran: redaction
left `tmp=<tmp>` in `bin_pairs.sh`, so it does not run as it is.

## Gates

Run on this commit's tree before this table was appended, with `TMPDIR=<tmp>`.

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| `cargo clippy … --features fault-injection -- -D warnings` | ok |
| `cargo test --locked --no-fail-fast` | 4,779 passed, 0 failed, 7 ignored |
| `cargo test … --features fault-injection --no-fail-fast` | 4,808 passed, 0 failed, 7 ignored |
| `scripts/check-xfx-identity.sh`, `check-no-stubs.sh`, `check-no-secrets.sh` | ok |
| `cargo build --locked --release`; `--release --features fault-injection --target-dir target/faulty` | ok |
| `scripts/smoke.sh target/release/xfx` | 47 checks, 0 failures |
| `scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx <dir>` | 37 scenarios and the oracle, 1,111 checks, 0 failures |

No test failed, so none was re-run.

Round c, on its own tree (`target/release/xfx` `27536f74…`), with `TMPDIR=<tmp>`:

| gate | result |
|---|---|
| `cargo fmt --check`; both clippies | ok |
| `cargo test --locked --no-fail-fast` | 4,779 passed, 1 failed, 7 ignored: `mcp::transport::tests::a_request_to_a_server_that_never_reads_stops_at_its_cancellation` (`!transport.alive()`, the run at load average 20.9 and twice its usual length) passed alone 3 of 3 |
| `cargo test … --features fault-injection --no-fail-fast` | 4,809 passed, 0 failed, 7 ignored |
| `scripts/check-xfx-identity.sh`, `check-no-stubs.sh`, `check-no-secrets.sh` | ok |
| release and faulty builds | ok |
| `scripts/smoke.sh target/release/xfx` | 47 checks, 0 failures |
| `scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx <dir>` | 37 scenarios and the oracle, 1,111 checks, 0 failures |

Round c, item 4, on the merged tree (`737f7477` and the doc edits of the commit after it; `target/release/xfx` `b9294018…`), with `TMPDIR=<tmp>`. Each step waited until no `cargo` or `xfx-<hash>` test binary ran on the machine (`pgrep -x`).

| gate | result |
|---|---|
| `cargo fmt --check`; both clippies (with PROC-SPAWN's `disallowed-methods`) | ok |
| `cargo test --locked --no-fail-fast` | 4,889 passed, 0 failed, 7 ignored |
| `cargo test … --features fault-injection --no-fail-fast` | 4,918 passed, 0 failed, 7 ignored |
| `scripts/check-xfx-identity.sh`, `check-no-stubs.sh`, `check-no-secrets.sh` | ok |
| release and faulty builds | ok |
| `scripts/smoke.sh target/release/xfx` | 47 checks, 0 failures |
| `scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx <dir>` | 37 scenarios and the oracle, 1,111 checks, 0 failures |
