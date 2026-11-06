# MCP-C2 receipt -- the three config deltas no unit owned

Unit MCP-C2 of the 0.0.16 delta: what MCP-C found under fx v0.0.16's
`loadHost` and left (`../mcp-config/README.md`). Security-sensitive (S): a
profile entry's `${VAR}` reads the environment into what a server is started
or reached with. Base `feat/p5-mcp-config` `9c83b7e9`. Oracle fx v0.0.16
(`481104fd`, executable sha256 `8e6cda2efddb…`, checked by capture.sh before
and after every run). xfx: `target/debug/xfx` of this branch's code commit
(`cap/xfx/`, `x𝒇x v0.1.0-68501d2 [debug]`) and of the base (`cap/xfx-base/`,
`x𝒇x v0.1.0-9c83b7e [debug]`).

## What changed

- `src/mcp/config.rs`: `prepare_to_start`, fx's `loadHost` tail
  (`src/builtins/mcp.zig:121-134@481104fd`) over the servers both files gave:
  1. **`${VAR}` in a profile server** (`expandProfileConfigs`,
     `src/core/mcp/project_config.zig:598-605@481104fd`): its command, each
     argument, each `env` value, each header value, from the whole
     environment; fx's template (`:698-733`: set-but-empty is a value, the
     default only when unset, one pass), fx's order of fields and first
     failure, fx's 1 MiB caps (`:663-664`, `:735-749`), the headers checked
     again after (`:786`). A server that can't be expanded is dropped with
     fx's sentence. **Only `Source::Profile`**: an approved workspace server
     was expanded already, credentials withheld, and must not be read again.
  2. **A name a terminal can't show** (`skipUnprintableNames`,
     `src/builtins/mcp.zig:99-116@481104fd`): dropped after the expansion,
     its note last; the predicate is `load_workspace`'s, now one `printable`.
  - Notes name a server cut at 160 bytes and a variable at 128 bytes, each
    to `...`, as `encodeTerminalSafe` cuts (`text_utils.zig:353-390@481104fd`);
    the workspace's missing-variable and withheld sentences now do too (they
    took the name whole and the variable by 128 characters).
- `src/mcp/runtime.rs` (the one call site, +3): `read_configuration` calls it
  after the merge, where fx calls both.
- `docs/parity.md`: rows "MCP profile `~/.xfx/mcp.json`" and "MCP project
  servers and trust".
- `scripts/fxcap/scenarios/p5-mcp-config-vars.cap` (+ `-profile.json`) and
  `p5-mcp-config-ignored.cap` (+ `-profile.json`). Fake values only
  (`P5_TAG=fake-p5-tag`); no local path in either.

## RED and GREEN

**Unit tests** (`cargo test --lib mcp::config`, the new ones beside MCP-C's).
RED with `prepare_to_start` emptied to the base's behaviour: 9 of the 11 new
tests fail, e.g. `the_vars_scenario_…` `left: ["fixture", "fallback",
"unset", "remote", "badref", "evil\u{1b}]0;x\u{7}", "rtl\u{202e}txt"]`
`right: ["fixture", "fallback"]`. RED with only the `Source::Profile` filter
removed: `only_a_profile_server_is_expanded_…` fails with `("proj",
["fake-gateway-key", "fake-p5-tag"])` -- a repository's approved entry whose
`${FOO}` the environment answers with `${AI_GATEWAY_API_KEY}` would get the
(fake) key -- and `the_expansion_is_capped_…` with a workspace server
dropped. GREEN: 58 of 58.

**fx's sentences are pinned.** `the_vars_scenario_starts_what_fx_v0_0_16_…`
reads every `note:` row of `cap/fx/fx-p5-mcp-config-vars-40x120-list.txt`
(continuation rows joined, `/.fx/` read as `/.xfx/`, `fx won't print` as
`xfx won't print`) and asserts xfx's five notes are them, in order:

```
~/.xfx/mcp.json server 'unset' field argument requires environment variable 'P5_UNSET_TOKEN'; set it or use ${P5_UNSET_TOKEN:-default}.
~/.xfx/mcp.json server 'remote' field http_header requires environment variable 'P5_UNSET_HEADER'; set it or use ${P5_UNSET_HEADER:-default}.
~/.xfx/mcp.json server 'badref' was skipped: a ${...} in it isn't valid.
~/.xfx/mcp.json server 'evil\x1b]0;x\x07' was skipped: its name has a character xfx won't print.
~/.xfx/mcp.json server 'rtl\u{202e}txt' was skipped: its name has a character xfx won't print.
```

**Captures**, cell by cell with `.prd/tui-phase4/receipts/m1/cells.py` (row
1 not compared, `/.fx/` read as `/.xfx/`):

| pair | fx vs xfx base | fx vs xfx head | rows left, and the unit that closes them |
|---|---|---|---|
| vars `startup` | 3 rows: `MCP startup: 7 servers need attention, 7 failed.` | **1 row**: row 5's right side, `MCP config has 5 problems · /mcp list` | MCP-S (status line) |
| vars `menu` | 15 rows: all 7 `Failed`, `evil\x1b]0;x\x07` and `rtl\u{202e}txt` listed and started | 5 rows: `fixture` and `fallback` `Ready`, the other five gone, `5 project MCP configuration errors…` -- in M1's menu, config order (fx's sorts, `5 config notes: /mcp list shows them`) | MCP-M |
| vars `list` | 15 rows | 13 rows: xfx's `/mcp list` still opens M1's menu | MCP-V (A4 table, `note:` rows) |
| vars `home end` | no `~/p5-wire-fake-p5-tag.log` | **the file is there**, as in fx: the stdio fixture found its script through `${P5_DIR}` and wrote where its expanded `env` said (766 and 342 bytes: what each client sends is the wire's, not this unit's) | -- |
| ignored `menu`, `list` | 4, 8 rows | 4, 8 rows, unchanged: fx's two `which fx ignores` notes not given, by decision | the parity row |

## Security (S)

| property | test |
|---|---|
| a workspace server is never expanded by the profile's pass, so a value the workspace's expansion wrote in (`${AI_GATEWAY_API_KEY}` as `FOO`'s value) is never read again; a pending one stays as written | `only_a_profile_server_is_expanded_so_a_repository_never_reads_a_credential` (fake key, never in the workspace servers' `Debug`) |
| fx's fields only: command, arguments, `env` values, header values; never the URL, `header_env`, `bearer_token_env`, `oauth`, a name | `a_profile_servers_variables_expand_where_fxs_do_and_nowhere_else` |
| one pass: a value or a default holding `${…}` is written in as it is | `a_variable_is_its_value_even_empty_and_its_default_only_when_unset` (19 templates) |
| an expanded header holds to a written one's rules: a line break, `DEL` or `NUL` drops the server, the value in no note | `an_expanded_header_is_held_to_a_written_headers_rules` |
| bounded: 1 MiB per value and per call, as fx | `the_expansion_is_capped_as_fxs_is` (600 KiB fake values) |
| no note names a value; names reach a note only through `terminal_safe`, cut | `the_vars_scenario_…`, `a_server_left_without_a_value_…` |
| the file keeps `${…}`: the reader and the in-place writer see the text, not the expansion | `the_profile_on_disk_keeps_its_variables_as_written` |
| the runtime's read goes through it, and starts nothing it drops | `the_runtime_starts_only_what_the_expansion_and_the_name_check_leave` (a variable no environment sets; the profile's bytes unchanged) |

## Reproduce

```
FX_BIN=<fx v0.0.16> TMPDIR=/private/tmp/claude-501 scripts/fxcap/capture.sh fx  p5-mcp-config-vars    40x120 <out>
                    TMPDIR=/private/tmp/claude-501 scripts/fxcap/capture.sh xfx p5-mcp-config-vars    40x120 <out>
FX_BIN=<fx v0.0.16> TMPDIR=/private/tmp/claude-501 scripts/fxcap/capture.sh fx  p5-mcp-config-ignored 40x120 <out>
                    TMPDIR=/private/tmp/claude-501 scripts/fxcap/capture.sh xfx p5-mcp-config-ignored 40x120 <out>
cargo test --locked --lib mcp::config
```
