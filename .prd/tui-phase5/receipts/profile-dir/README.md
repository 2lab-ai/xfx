# profile-dir: every writer that can make `~/.xfx` makes it private

The US-U2 review left this residual: "A skills command that creates a missing
`~/.xfx` with `create_dir_all` makes it 0755. The session store and the usage
cache then refuse it until the user runs `chmod`." The session store refuses
a `~/.xfx` that is a link, not a directory, another account's, or open to
group or other (`verify_store_dir`). Every other writer in the profile made
the directory its own way, and none of them held an existing one to that rule.

Base: `main` @ `5ca0bc18`. Binaries: base `f2b1844c…`, fix `31502f88…`
(sha256 prefixes, `target/release/xfx` of each tree).

## Every site, traced

| Site (base) | What it did | Now |
|---|---|---|
| `src/skills/command.rs:188` `create` | `create_dir_all(~/.xfx/skills/<name>)`: `~/.xfx`, `skills/`, `<name>/` all `0755` under umask 022; an existing open or linked `~/.xfx` written into | `install::ensure_managed` → `profile_home::ensure` |
| `src/skills/install.rs:198`, `:285` | `create_dir_all(managed)`: same | `ensure_managed(managed, managed)` |
| `src/skills/install.rs:317` `transaction_root` | `fs::create_dir`, umask | `profile_home::make_new` (`0700`) |
| `src/skills/install.rs:358` `lock` | `DirBuilder.mode(0o700)` for `.skill-install-locks`, no home check | `profile_home::ensure(parent, &dir)` |
| `src/skills/install.rs:406` `copy_tree` | `create_dir_all`, umask, for every copied directory | `profile_home::make_new` (`0700`) |
| `src/provider/profile.rs:645-663` `create_private_dir` (5 callers: preferences, `/allowlist`, `edit_document`, `rewrite_document`, `write_document`) | recursive `0700` when missing, but an existing `~/.xfx` passed `is_dir()`: a link was followed and an open one written into | deleted; the 5 callers call `profile_home::ensure(dir, dir)` |
| `src/mcp/config.rs:707-723` `create_private_dir` | same as above | deleted; `profile_home::ensure`, refusals in upstream's error names |
| `src/session/prompt_history.rs:340-356` `create_private_dir` | same as above | deleted; `profile_home::ensure` |
| `src/session/store.rs:3376` `ensure_private_dir` | `create_dir_all(parent)`: a missing home made with the umask's mode, and -- for `sessions` or `sessions.staged` with `~/.xfx` gone under a running session -- `~/.xfx` itself made `0755` | the parent step is deleted; `SessionStore::open` makes `~/.xfx` through `profile_home::ensure`; `verify_store_dir` and `InsecureParent`'s sentence are `profile_home::problem` / `refusal` |

Found and left alone, each with its reason:

- `src/session/store.rs:1788` (a session's directory) and `:2834` (its
  `images/`): `fs::create_dir` then `chmod 0700`, inside `sessions/`, which is
  already checked private; `fork.rs` makes its folders `0700` from creation
  (pinned by `every_folder_a_fork_makes_is_made_private_from_creation`).
- `src/usage.rs:505` `start_tracking`: a file (`create_new`, `0600`), never a
  directory; it fails when `~/.xfx` is missing.
- `src/trace.rs`, `src/clipboard.rs:483`, `src/tui/clipboard_image.rs` (through `clipboard::private_directory`):
  `$TMPDIR`, not the profile.
- `src/tools/mutate.rs:1436`, `:1460`: the model's write tool, in the
  workspace; any path through a `.xfx` is refused
  (`PROTECTED_WRITE_DIRECTORY_NAMES`). Workspace `.xfx/` directories share no
  code path with these writers, so they are out of scope.
- The usage cache (US-U2) is not on `main` yet; its own
  `open_or_create_profile` already makes `~/.xfx` `0700` and refuses an open
  one.

`no_product_code_but_this_module_makes_a_directory_tree` now fails the build
for any product `create_dir_all` outside `src/profile_home.rs`.

## RED on the base

`red_profile_home.rs` is the harness, added to a `git clone --no-local` of
`5ca0bc18` (it calls each writer as base has it). `red-unit.txt` is its run:

- **A missing `~/.xfx`**: `/skills create` leaves `755 .xfx`, `755
  .xfx/skills`, `755 .xfx/skills/demo`, and `SessionStore::open` then refuses
  with `<HOME>/.xfx cannot hold session state: it is mode 755; …`;
  `/skills add` the same (only `.skill-install-locks` was `700`). The other
  writers made `700` already.
- **An existing `0755` `~/.xfx`**: the session store refuses it; the settings
  writers, the MCP profile, the prompt history, `/skills create` and
  `/skills add` all write into it (1–2 entries each).
- **A linked `~/.xfx`**: the store refuses it; the same six write where the
  link points.
- **A missing home**: the store made it `755` (with `~/.xfx` `700`).

`bin-base.txt` is the same on the base binary, through its real surfaces
(`bin_pairs.sh`, `drive_tui.py`: a pty, 40 s alarm, `/quit`, `HOME` a fresh
directory per case, umask 022). `xfx setup gateway` writes `settings.json`
into an open `~/.xfx` and through a linked one, then `xfx sessions` refuses
both. In the TUI the session store opens first, so `~/.xfx` is `700` and
only `skills/` and `skills/demo` are `755`; `~/.xfx` itself comes out `755`
when it is removed under the running session and `/skills create` makes it
again (`tui-gone`), after which `xfx sessions` refuses it.

## GREEN

`bin-fix.txt`, the same script on the fix binary: a fresh home is `700` all
the way down for every surface, `tui-gone` included, and `xfx sessions` opens
it; `xfx setup gateway` on an open `~/.xfx` exits 1 with `cannot write
…/settings.json: …/.xfx cannot hold session state: it is mode 755; …`, the
home still `drwxr-xr-x` and empty; on a linked one it exits 1 with the
store's link sentence and nothing appears where the link points.

The unit pins are `src/profile_home/tests.rs`: every writer from a missing
home (umask 022 and, in a child test binary, umask 0), an open home refused by
every writer in the store's words and never chmodded, a linked home refused
with nothing written where it points, a file in its place, another account's
directory, a missing home, existing levels below kept as they are, nothing
made outside the profile, and the source pins.

## Mutants

On the clone (`mutants.py`, `mutants.txt`), each restored after its run:

| Mutant | Fails |
|---|---|
| M1 drop `.mode(0o700)` from the one-level make | 5 tests, e.g. `every_writer_makes_the_profile_home_private_under_any_umask` |
| M2 `/skills create` bypasses the helper (`create_dir_all`) | 7, e.g. `every_writer_makes_a_missing_profile_home_private_and_the_store_opens_it`, `every_profile_writer_makes_its_directories_through_this_module` |
| M3 the settings document writer bypasses the helper | 7, e.g. `an_open_profile_home_is_refused_by_every_writer_in_the_stores_words_and_never_chmodded` |
| M4 an existing home is not checked | 3, e.g. `a_linked_profile_home_is_refused_by_every_writer_and_nothing_is_made_where_it_points` |
| M5 drop `.mode(0o700)` from the ancestors' make | `a_missing_home_and_every_level_below_the_profile_home_are_made_private` |

## fx v0.0.16

- `openOrCreateVerifiedPrivateChild`, `src/core/shared/io.zig:712-742@481104fd`:
  creates `0700`, then `chmod`s an existing directory to `0700` and checks it.
  xfx refuses instead (the session store's rule); `docs/parity.md`, the
  `profile home ~/.xfx` row.
- `Store.initFromHome`, `src/core/config/settings_store.zig:340-381@481104fd`:
  a missing home is an error (`:343-347`); a writable open chmods `~/.fx`
  (`:370-373`).
- `createSkillTemplate`, `src/builtins/skills.zig:247-252@481104fd`,
  `createInstallTransaction` `:669`, `ensureDir` `:786-798`, `makeDirRecursive`
  `src/core/shared/io.zig:1077-1093@481104fd`: `.default_dir`, the umask's mode.
- `profileDir`, `src/core/mcp_host/config_file.zig:147-155@481104fd`: the MCP
  profile goes through `openOrCreateVerifiedPrivateDir`, whose errors are the
  names `/mcp` now says.

## Gates

On the tree of this commit, with `TMPDIR=<tmp>`.

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| `cargo clippy … --features fault-injection -- -D warnings` | ok |
| `cargo test --locked --no-fail-fast` | 4,769 passed, 1 failed, 7 ignored; the failure, `mcp::tests::an_http_server_nothing_listens_on_fails_as_connection_refused_with_upstreams_counts` (`McpConnectionFailed` for `ConnectionRefused`: a loopback port released and taken again while other gates ran), passed alone 3 of 3 |
| `cargo test … --features fault-injection --no-fail-fast` | 4,799 passed, 0 failed, 7 ignored |
| `scripts/check-xfx-identity.sh`, `check-no-stubs.sh`, `check-no-secrets.sh` | ok |
| `cargo build --locked --release`; `--release --features fault-injection --target-dir target/faulty` | ok |
| `scripts/smoke.sh target/release/xfx` | 47 checks, 0 failures |
| `scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx <dir>` | 37 scenarios and the oracle, 1,111 checks, 0 failures |

Every file here went through `scripts/fxcap/redact.py` (this one through its
check): `<tmp>` is the gates' short `TMPDIR`, `profile-dir` in the harness
temp folder; a unit test's HOME is `<bench-HOME>`. The binary pairs name each
case's HOME directory `HOME`.
