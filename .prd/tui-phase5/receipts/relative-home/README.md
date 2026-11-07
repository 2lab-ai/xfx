# relative-home (P5 L-U3): a relative or empty `HOME` counts as unset

The unit is L11 of the lifecycle cluster
(`research/tui-phase5:.prd/tui-phase5/research/lifecycle.md` §L10/L11, §4 row
L11, §5 U3), from fx v0.0.16 `21a55a45` "Treat a relative HOME as unset".
Security-sensitive: file writes. Base `origin/main` @ `b0a78fea`.

Commits: `4a1f2398` (the rule, the readers, the tests, `docs/parity.md`),
`94fa1228` (the capture harness's `keep`, the ledger causes), `75cba0d6` (the
made-home scenario's folder name), `0ebade1b` (the usage cache test that
pinned the old reading). Binaries (sha256 prefix of `target/debug/xfx`): RED
`99e8d458…` built at `b0a78fea` (banner `v0.1.0-b0a78fe`), GREEN `63574837…`
built at `0ebade1b` (its revision string `0ebade1b6b0d`; the GREEN TUI shows
no banner, it refuses before drawing one).
fx: v0.0.16, sha256 `8e6cda2e…`, checked by `capture.sh` before and after
each run.

## fx's rule

- One reader, `homeDir`: `HOME` when `std.Io.Dir.path.isAbsolute`, else null
  (`src/core/shared/io.zig:475-483@481104fd`). Its test: `""`,
  `relative/home`, `./home`, `~` are null, the raw value still there for
  children (`:1214-1232`). Every profile reader goes through it (71 of them,
  per the commit message), e.g. the session store's `openFromEnv` →
  `error.HomeNotSet` (`src/core/session/session_adapter.zig:211-213`).
- `fx sessions` / `fx session`: `fx <kind>: HOME is not set`, exit 1
  (`writeLookupFailure`, `src/core/cli/cli_surface.zig:2923-2927`).
- TUI: the prompt history is `.unavailable` with no home
  (`prompt_history_runtime.zig:49`) and says
  `! history: durable prompt history unavailable`
  (`app_bootstrap_runtime.zig:667-673`); the turn runs, nothing is saved.
- `doctor`: `HomeNotSet` is a warning (`doctor_runtime.zig:255-261`).
- Children keep `HOME` unchanged.

## The readers (`var_os("HOME")`, `var("HOME")`, `home_dir`)

| Site (base) | What it did | Now |
|---|---|---|
| `src/config.rs:800` `Environment::from_process` | dropped an empty `HOME`, kept a relative one | `config::home_dir`: absolute only; `Environment::new` the same rule |
| `src/tui/shell.rs:2309` file index `~` | a second `var_os("HOME")`, empty and relative kept | `config.home()` (derived from the rule) |
| `src/app.rs:842-848` `read_only_store` | no profile → a store rooted at a relative `.xfx`, the workspace's project directory, for `sessions`, `session`, `status`, `doctor` | `Option`: `xfx sessions: HOME is not set` / `xfx session: …`, exit 1; `status` no session; `doctor` warns |
| `src/permission/authority.rs:1469` `minimal_environment` | passes `HOME` to a command | unchanged, as fx |
| `src/mcp/transport.rs:187` `INHERITED_NAMES` | passes `HOME` to an MCP server | unchanged, as fx |
| `src/permission/gitconfig.rs:244` `query_environment` | the context's home (derived) | follows the rule, as fx's `git_config.zig:319` uses `homeDir()` |
| `src/usage/cache.rs:430-435` `Home::new` | already absolute only (69a01a40) | unchanged |

`home_dir` itself has no other callers (no `dirs`, no `std::env::home_dir`);
its other grep hits are test fixtures named `_home_dir`. The PROFILE-DIR (#63)
writer table (`src/profile_home/tests/scan.rs`: the session store, the
settings writers in `src/provider/profile.rs`, the MCP profile, the prompt
history, `/skills create` and `/skills add`) takes every path from
`RuntimeConfig::profile_dir` / `user_settings_path` / `home()`, which all come
from `Environment::home()`: an empty or relative `HOME` now reaches each as
no home, the path a missing one takes.

## RED (the base)

`red/`, compared in `red.txt`. The security RED is
`p5-lifecycle-relhome-made` (new): its workspace holds `relative/homedir/`,
the folder `HOME=relative/homedir` names, and one prompt is sent.

- **The base wrote the profile into the workspace**
  (`red/xfx-p5-lifecycle-relhome-made-40x120-work-h03.txt`):
  `relative/homedir/.xfx/history.jsonl`, `history.lock`,
  `sessions/<id>/events.jsonl`, `sessions/<id>/session.json` -- the prompt,
  the answer and the workspace path. fx wrote nothing but the fixture's README.
- `xfx sessions` exited 0 and listed that session (`h02-sessions`; fx: `HOME is
  not set`, exit 1). On the research's `p5-lifecycle-relhome`, whose folder is
  not there, the base TUI refused (`the home directory relative/home is not
  there`) and `xfx sessions` exited 0 with `count=0`, read from a store rooted
  at a relative path.
- `red-tests.txt`: the new binary tests fail on the base's `src/` (the
  `sessions` refusal; the line shell opens on `relative/home`).

## GREEN

`green/`, compared in `green.txt` (`scripts/fxcap/sweep.sh --only
'p5-lifecycle-relhome*'` against `scripts/fxcap/parity-ledger.toml`).

- **Workspace listings identical to fx's**, byte for byte: `work-h02` empty on
  both, `work-h03` the README alone on both.
- `xfx sessions` with the relative `HOME`: row 2 `xfx sessions: HOME is not
  set`, and the dead pane's `status 1`, equal to fx's under the ledger's
  product-name and dead-pane normalizations. Row 1 differs: what the first
  launch left (fx's banner; xfx's refusal) -- cause
  `tui-without-home-scrollback`.
- **The TUI snaps are not GREEN** (`h00-launch`, `h01-turn`; cause
  `tui-without-home`): fx opens and runs the turn unsaved; xfx's TUI refuses,
  `` xfx cannot record a conversation because no home directory is set; run
  `xfx ask --no-save <prompt>` … ``, exit 1 -- the refusal it already gave an
  unset `HOME` (`src/interactive.rs:795-797`, `src/tui/worker.rs:1524-1529`),
  now reached by an empty or relative one too. Running the TUI unsaved needs a
  session that writes nowhere; `SessionRecorder` wraps a `WritableSession`
  holding the log's `File` (`src/session/store.rs:2335-2431`), used through
  the TUI worker. That is the follow-up L-U3b; the divergence is written in
  `docs/parity.md` (`profile home` row). fx's
  `! history: durable prompt history unavailable` line is therefore not
  ported: no xfx session reaches it.

## Unit tests

- `config::tests::only_an_absolute_home_is_a_home`: `""`, `relative/home`,
  `./home`, `~`, `../home`, `a/../home`, `..` are no home (through
  `home_dir` and `Environment::new`); three absolute values -- fx's own test
  value, one with a `..` after its leading `/`, and `/` -- are kept as they
  are, fx's `isAbsolute`.
- `config::tests::a_relative_home_gives_no_profile_and_no_path_under_the_workspace`.
- `tests/sessions.rs` `a_relative_or_empty_home_is_no_home_and_nothing_is_written_anywhere`:
  six values, each naming a folder that is there (`../home` is the sandbox's
  own home); `sessions`, `session last`, `ask`, `setup gateway`, `doctor`,
  `status`; the sandbox's whole tree, directories included, is unchanged.
- `tests/interactive.rs`: the line shell's no-home refusal for unset, empty and
  relative `HOME`, with nothing made in the folder.
- The mutant that keeps a relative `HOME` (the base's rule) fails all four
  (`red-tests.txt`), run on a `git clone --no-local` in the scratch `TMPDIR`.

## Harness

`capture.sh` set `remain-on-exit` only when the scenario's `keep` line ran;
a binary that exits before that (the GREEN TUI, under the made scenario's
four comment lines, each a `for_this` subshell) took the server with it and
the run stopped at `keep` with `no server running`. A scenario with `keep`
now sets it before `new-session`. `test_capture.py`
`test_keep_holds_a_pane_whose_binary_is_gone_at_once`: fails 3/3 on the old
`capture.sh`, passes 3/3.

## How it was captured

```
FX_BIN=<fx v0.0.16> TMPDIR=<short tmp> scripts/fxcap/capture.sh fx  <scenario> 40x120 <dir>
XFX_BIN=<xfx>       TMPDIR=<short tmp> scripts/fxcap/capture.sh xfx <scenario> 40x120 <dir>
```

for `p5-lifecycle-relhome` (the research's) and `p5-lifecycle-relhome-made`,
with the harness at `0ebade1b` (`fx/` and `red/` at `75cba0d6`: the same harness and scenarios). Every file here went through
`scripts/fxcap/redact.py`; `relative<local-path>` in the research scenario's
RED sessions snap is its rewrite of the relative `HOME`'s `.xfx` path.
