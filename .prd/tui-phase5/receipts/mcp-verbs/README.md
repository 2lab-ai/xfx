# MCP-V receipt -- `/mcp` verbs v2 (A1-A6, A8-A12, A14-A15)

Unit MCP-V of the 0.0.16 delta (`.prd/tui-phase5/research/mcp.md` §5 unit 1 on
the research branch), epic 2, wave 3. Security-sensitive (S): file writes into
the repository (`--project`). Branch `feat/p5-mcp-verbs`. Oracle fx v0.0.16
(`481104fd`, executable sha256 `8e6cda2efddb…`, checked by capture.sh and by
the sweep before and after every fx run).

## The base

`feat/p5-fxcap-h1` `8464e981` with, in order, `feat/p5-mcp-config` `9c83b7e9`,
`feat/p5-mcp-wire` `4bdfa90b`, `feat/p5-mcp-toolgroup` `ced90405` and MCP-S's
closed head `e47b9c5c`, each `--no-edit`. Only `docs/parity.md` conflicted, in
the profile, project, runtime and server-tools rows; each was merged sentence
by sentence (`git merge-file` over the rows split at `. ` and `; `), every
sentence of both sides kept (MCP-W's rows 1-3 and MCP-C's, MCP-W's and
MCP-T's server-tools sentences, MCP-W's and MCP-S's runtime sentences).

**The text merged; the code did not compile**: `src/mcp/catalog.rs:115`,
MCP-W's `note_read`, matched `Status` before MCP-S added `Retrying`
(`E0004`). `ce5dccb3` puts `Retrying` beside `Connecting` (still coming up:
fx's `tools()` waits through it). Base gate on `ce5dccb3`: `cargo test
--locked --no-fail-fast` 3735 lib + every integration binary, 0 failures;
the first full run had one failure under load,
`mcp::tests::a_reload_restarts_only_the_server_whose_config_changed_and_says_nothing`,
which passed 3 times of 3 alone.

## What changed

- `src/mcp/command.rs` (+ `src/mcp/command/tests.rs`): fx v0.0.16's
  `src/core/mcp_host/verbs.zig` for the shell surface -- the lexer
  (`command_lex.zig:140-310`), `parse` (`verbs.zig:60-174`), the overview and
  usages (`:188-230`), `list` (`:349-426`), `show` (`:483-575`), `add`
  (`:577-597`), `remove` (`:599-629`), `approve`/`reject` (`:631-680`),
  `login`/`logout` refusals (`:689-806`), `statusText` (`:369-388`),
  `commandLine` (`config.zig:67-83`) and the completion list
  (`app_mcp_runtime.zig:459-515`). Output: a verb's stdout one neutral
  notice, its stderr one warning (`app_mcp_runtime.zig:585-602`).
- `src/tui/mcp_args.rs` (new) and `src/tui/shell.rs`: A14, `/mcp `'s
  argument completions under the argument (the `/provider` column's kind).
- `src/tui/shell.rs`: only a bare `/mcp` opens the menu (`app_commands.zig:
  927-945`); every MCP notice under `mcp` (`:340-354`). `src/interactive.rs`:
  the summary `manage MCP servers and their logins` (`commands.zig:463`).
- `src/mcp/config.rs`: `render_profile`, `add_profile_server`,
  `remove_profile_server`, `edit_profile`, `parse_profile_strictly` under
  `#[cfg(test)]`, not deleted: `src/mcp/tests.rs` calls them 41 times (37 `add_profile_server`, 4
  `remove_profile_server`) to seed whole `ServerConfig`s, and a parallel MCP-S
  round may add more.
- Call sites the removed verbs broke, outside this unit's files: `shell.rs`
  tests (5 `path` probes now `remove nope`, 4 `/mcp reload` now
  `Job::MenuReload`), `src/tui/mcp_menu.rs` tests (3 `reload`), `src/mcp/
  tests.rs` (reload, `auth`, `remove` words, the feature-command test now
  asserts the moved sentences), `tests/mcp.rs` (`/mcp reload` is now an
  `add`), `src/mcp/runtime.rs` (doc comments only).

## RED: the base

`sweep.sh --only p5-mcp-empty --only p5-mcp-verbs` with the base binary
(`git archive ce5dccb3`, debug, sha256 `7ea12815f5cb…`): **1 / 52** pairs
normalized-identical (`scoreboard-base.md`). Notice by notice, in order
(`notices.py` over `end.full.txt`): `p5-mcp-empty` **0 of fx's 22**,
`p5-mcp-verbs` **0 of 24** -- `* usage: /mcp [list|resource|…]` for every
new verb, `path` printing a path, `reload` reconnecting, `/mcp list` opening
the menu. `verbs-after-add.files`: the base's `~/.xfx/mcp.json` is c1db9191's
one line, 908 bytes, with `two` saved as the program `--` (`xfx-base/`).

## GREEN: the head

Same sweep, head build (`9034e3b9`, debug, sha256 `ea0e6b380f0f…`; the head
after it changes comments and docs only): `scoreboard-head.md`.

| | fx v0.0.16 vs xfx head |
|---|---|
| `p5-mcp-empty`, 24 pairs | **21 normalized-identical**; the 3 left are `menu`, `menu-down`, `menu-right` -- M1's menu (MCP-M, B1/B3) |
| `p5-mcp-empty` notices | **22 of 22** identical, in order |
| `p5-mcp-verbs` notices | **20 of 24** identical, in order. The 4: the two `list` tables (SOURCE a cell wider; `remote`/`web2` `retrying` where fx's are `connecting`, MCP-S's runtime row); `show remote` printed in typed order, before `show proj`, where fx printed it last (A12, its rows identical); `login remote` gated. xfx adds the launch trust prompt and its dismissal (C1, MCP-M) |
| `p5-mcp-verbs`, 28 pairs | 0 positionally: the C1 prompt's 4 rows and its notice push every later row. Aligned as sequences (`pairs.json` `aligned`), every row left is one of the five causes above |
| A14, `complete-args` / `complete-show` | the popup's rows identical (col 7, widths, bold 255 / dim 245, six rows padded, status line under it), but `show remote   retrying` |
| `/mcp remove web` (only `web2` there) | `Removed web2 …`, then `No MCP server named web2 …`, as fx's Enter on the marked completion does |
| `verbs-after-add.files`, `verbs-end.files` | `~/.xfx/mcp.json` byte-identical to fx's both times (410, 220 bytes); `settings.json` the same value (xfx's settings writer is two-space); fx's `backups/settings.json.backup.*` and `mcp-credentials/credentials-v3.lock` are absent (settings backups, sign-in gated) |

`fx/`, `xfx-head/`: every snap of both scenarios, redacted (`<scenarios>`,
`<bun>`, `<local-path>`); `xfx-base/`: the base's `.txt` and `.files`.

## Security (S)

| property | test (`src/mcp/command/tests.rs`) |
|---|---|
| `--project` with an `Authorization` header (any case, as the reader matches it) is refused before `update`, nothing written | `a_header_or_env_value_is_never_said_back_and_never_planted_in_the_project` |
| a `--header`/`--env` value is never said back, in `add`'s sentences or in `list`/`show`/`login`/`logout` -- a `${VAR:-default}` whose variable is unset (as written, or expanded once MCP-C2's profile expansion lands) and a plain value | that test; `show_and_list_never_print_an_env_or_header_value_raw_or_expanded` |
| a name, URL, command or typed word reaches the screen through `terminal_safe`, cut to fx's 240 bytes at a whole token | `every_mistake_is_fxs_sentence`, `a_name_or_a_target_reaches_the_screen_spelled` |
| a linked `.mcp.json` is refused through MCP-C's writer, its target untouched | `a_link_where_the_project_file_should_be_is_refused_in_fxs_words` |
| a gated `add slack` writes nothing | `with_no_servers_every_verb_answers_in_fxs_words_and_tone` |
| every file a case writes is under the run's TMPDIR, in a scratch repository, asserted outside the checkout | `Scratch::new` |

`show` prints an HTTP server's URL as fx does (`verbs.zig:528`), so a URL
that carries a token is printed by both.

## Gates

On `3c09b481` (the head after it changes comments, docs and this
directory only):

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings`; `--features fault-injection` | ok; ok |
| `cargo test --locked --no-fail-fast` | 4640 passed, 0 failed (lib 3757) |
| `cargo test --locked --features fault-injection --no-fail-fast` | 4663 passed, 0 failed (lib 3766) |
| `scripts/check-xfx-identity.sh`; `check-no-stubs.sh`; `check-no-secrets.sh` | ok; ok; ok (after this directory was added too) |
| `cargo build --locked --release`; `--release --features fault-injection --target-dir target/faulty` | ok; ok |
| `scripts/smoke.sh target/release/xfx` | 46 checks, 0 failures |
| `scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx` | 37 scenarios + the oracle, 1091 checks, 0 failures |

## Reproduce

```
FX_BIN=<fx v0.0.16> TMPDIR=/private/tmp/claude-501 scripts/fxcap/sweep.sh --fx <fx v0.0.16> \
  --pin-sha 8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2 \
  --only p5-mcp-empty --only p5-mcp-verbs --jobs 2 --out <dir outside the repo>
python3 -I .prd/tui-phase5/receipts/mcp-verbs/notices.py <fx end.full.txt> <xfx end.full.txt>
```
