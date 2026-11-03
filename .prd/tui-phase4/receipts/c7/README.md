# C7 receipt: /alias

One scenario, run at 40x120 on both binaries:

```
FX_BIN=<fx c1db919 build, 0.0.13, sha256 7b76319a…> scripts/fxcap/capture.sh fx  alias 40x120 <dir>
                                                    scripts/fxcap/capture.sh xfx alias 40x120 <dir>
```

Both were run with `TMPDIR=/private/tmp/claude-501`. `capture.sh` is
unchanged. It launches fx with `FX_AUTO_UPGRADE=0` last in its environment, and
it checked that the reference binary's bytes did not change during the run.

`alias.cap` waits 4 seconds and types `/alias`. It snaps the popup, then runs
the line. It then runs `/alias build` and `/alias build zig build` and snaps
after each. It types `/al`, snaps the popup, and snaps again after Down. Then
it opens `/help` and types three searches into the catalog: `alias`, `command`
and `name`. It snaps after each and clears the search between them with
Ctrl-U. It presses Tab ten times to reach the Extensions tab, snaps, presses
Escape and snaps the end.

The xfx binary is `target/debug/xfx`, built from `b65cb6a`. That commit merges
`plan/tui-phase4` at `49f32a9e`, which already has S1's `/skills`, into
`95c87a4`, this unit's change. Its banner shows `b65cb6a`.

Sanitized: no scratch path and no host name appears in any capture.

`.ansi` files are tmux's re-serialization of cell attributes
(`capture-pane -e`). They are not the programs' wire bytes.

## Per snap

Rows 2-40 are compared, both as text (`.txt`) and with SGR (`.ansi`). On both
sides, row 1 is the product's own banner.

| snap | fx | xfx | matches |
|---|---|---|---|
| `popup-alias` | `Commands 1`, `  /alias   show alias availability … Extensions` | same | Byte for byte, SGR included. |
| `bare` | `* aliases: Aliases are not yet configurable.` (row 3) | same | Byte for byte, SGR included. The whole notice is one run, `ESC[38;5;250m…ESC[39m`: the neutral label is the colour of the body. |
| `name` | the same notice again, row 5 | same | Byte for byte. `/alias build` is the command, not a prompt. |
| `name-command` | the same notice a third time, row 7 | same | Byte for byte. `/alias build zig build` gets the same sentence. |
| `popup-al` | `Commands 3`: `/allowlist` Security, `/alias` Extensions, `/balance` Account | same | Byte for byte. `/balance` is `/credits`'s alias, and `al` is inside its name. |
| `popup-al-down` | the mark on `/alias` | same | Byte for byte. |
| `help-query-alias` | `Commands 1`, `/alias` | same | Byte for byte. |
| `help-tab-extensions` | `Commands 3`: `/mcp`, `/skills`, `/alias` | same | Byte for byte. This needs S1, which `b65cb6a` has. |
| `end` | the catalog closed | same | Byte for byte. |
| `help-query-command` | `Commands 4`: `/help`, `/allowlist`, `/shell`, `/alias` | `Commands 2`: `/help`, `/allowlist` | **Differs.** See below. |
| `help-query-name` | `Commands 3`: `/rename`, `/skills`, `/alias` | `Commands 1`: `/rename` | **Differs.** See below. |

So 9 of the 11 snaps match on rows 2-40, in both text and SGR.

## What differs, and why

- **fx's help entry is a search key in its catalog.** fx's catalog filter,
  `helpCatalogSpecMatches` (`src/core/slash_commands/command_specs.zig:457-473`
  at `c1db9191`), checks a command's name, its `help_entry`, its completion
  description, its category label and its aliases. `/alias`'s help entry is
  `/alias [name] [command]` (`src/builtins/commands.zig:489`). So in fx, the
  searches `command` and `name` find `/alias`. `name` also finds `/skills`
  through `[name|url|path]` (`:483`). xfx's `SlashSpec` has no help-entry
  field (`src/tui/help_menu.rs`, module docs). Its filter checks the name, the
  summary, the category and the aliases, so it finds `/alias` by `alias`,
  `availability` or `extensions`, but not by `name` or `command`.
- fx uses the help entry in only two other ways, and neither shows on screen:
  - `renderSlashHelp` (`command_specs.zig:393`) renders it, but only tests call
    `renderSlashHelp`.
  - The completion paths (`:493`, `:553`, `:597`) only check that the entry is
    there.

  The catalog search is therefore the only screen this difference reaches.
- **`/shell` in `help-query-command`.** That is fx's `/shell reload`
  (`commands.zig:497`). It is a separate unit (`feat/tui-phase4-shell-reload`),
  and nothing in C7 changes it.

Adding a help-entry field would change every row of the registry. Several of
fx's entries name forms that xfx does not answer, and a search key that finds a
command by a form it refuses is the kind of promise `CONTRIBUTING.md` §1
forbids. That is a change to the whole registry, not to the `/alias` row, so it
is left for its own unit. The `docs/parity.md` row records the gap.

## What fx does, from its source (`c1db9191`)

- Its row: `.{ .kind = .alias, .command = "/alias", .aliases = &.{}, .help_entry = "/alias [name] [command]", .completion_description = "show alias availability", .presentation_category = .extensions, .has_args = true, .accepts_payload = true }`
  (`src/builtins/commands.zig:489`). It sits between `/settings` (`:488`) and
  `/credits` (`:490`).
- `accepts_payload` makes the command match by prefix and keep the rest of the
  line as its payload (`command_router.zig:135-137`, then `:120`). The router
  hands the payload to `handle_alias` (`:174`).
- `commandHandleAlias` (`src/core/app/app_commands.zig:2043-2050`) ignores the
  payload. It writes `writeDomainNotice(.{ .topic = "aliases", .tone = .neutral, .body = "Aliases are not yet configurable." }, true)`.
  `/version` is written through the same call (`:2153-2160`).

## What xfx does

- The row is in `SLASH_REGISTRY`, with the same name, summary, category and
  place, and `has_args: true` (`src/interactive.rs`).
- The TUI's `alias` handler, in `src/tui/shell.rs`, says
  `Notice::neutral(ALIAS_TOPIC, ALIASES_NOT_CONFIGURABLE)` through
  `announce`, as `/version` does.
- The line shell prints `[shell] aliases: Aliases are not yet configurable.`
  on standard output.
- Tests:
  - `interactive::tests::alias_is_upstreams_extensions_row_and_takes_the_rest_of_the_line`:
    the row, the three forms, the order and the words.
  - `tui::router::tests::an_alias_line_is_a_command_in_every_form_and_never_reaches_the_model`.
  - `tests/tui.rs` `alias_is_offered_under_extensions_and_answers_every_form_with_upstreams_notice`:
    the `/al` popup rows as fx paints them at 40x120, the three notices, and
    no request to the gateway.
  - `tests/interactive.rs` `slash_alias_on_the_line_shell_says_upstreams_sentence_for_every_form`.
