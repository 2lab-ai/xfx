# M1 receipt: `/mcp` and the MCP client (local leaves)

Four scenarios (`scenarios/`), each run at 40x120 on both binaries with the
repo's driver, unmodified, pointed at this directory's scenarios:

```
FXCAP_SCENARIOS=<this dir>/scenarios FX_BIN=<fx c1db919 build, 0.0.13> scripts/fxcap/capture.sh fx  <scenario> 40x120 <out>
FXCAP_SCENARIOS=<this dir>/scenarios                                   scripts/fxcap/capture.sh xfx <scenario> 40x120 <out>
```

| scenario | what it drives |
|---|---|
| `mcp-empty` | no server anywhere: `/mcp` (menu), Down, Right (Tools), Esc; `/mcp list`; `/mcp path`; `/mcp reload`; `/mcp auth`, `/mcp auth nope`, `/mcp logout nope`, `/mcp remove nope`, `/mcp add`, `/mcp resource list nope`, `/mcp prompt`, `/mcp trust`, `/mcp bogus`. |
| `mcp-fixture` | a workspace `.mcp.json` naming server `proj` (the trust prompt at launch, answered `3` = reject), then `/mcp add fixture <bun> <fixture>` (a profile server), the menu and its four sections, `/mcp list`, `resource list`/`resource read legacy://alpha`, `prompt list`/`prompt get review {"tone":"brief"}`, `auth`/`logout` on a stdio server, one turn (the request body is the advertisement with a ready server), `/mcp trust approve proj`, `/mcp remove fixture`, `/mcp trust reset`; `home` dumps after the add and at the end. |
| `mcp-wire` | `/mcp add fixture …`, then one turn of five steps (`mcp-wire-turns.json`): `capability_search` → `mcp_fixture_echo` → `mcp_features` → `mcp_select_tool` → text. The fixture logs every frame each client sends it (`*-mcp-wire.ndjson`); the fake gateway keeps every request body. |
| `mcp-menu-screens` | the menu's own screens: the workspace's `proj`, its trust prompt dismissed with Escape; `/mcp add --transport http remote http://127.0.0.1:9/mcp`, a loopback port nothing listens on; then `/mcp`, Enter on each server's details and Escape back, `c` (help), `p` and `z` and their confirmations, each left with Escape; Escape; `/mcp list`; `home` at the end. No server is started: `proj` is never admitted and `remote` refuses the connection. |

Both servers are upstream's own e2e fixture, `vercel-labs/fx@c1db9191
tests/e2e/fixtures/mcp-legacy-stdio.mjs`, byte-identical copy, run with `bun`
by absolute path. It is not copied into this repository: to retake these, put
that file in the scenario directory and write its absolute path where the
scenarios and `scenarios/mcp-ws/.mcp.json` say `<fixtures>/mcp-legacy-stdio.mjs`.

The fx captures of `mcp-empty` and `mcp-fixture` are the research captures
(`research/media-ext/cap/`, same binary, same scenarios); `mcp-wire` was taken
on both sides in this unit. The xfx binary is `target/debug/xfx` built at
`cda5004` -- the unit with `plan/tui-phase4` `276e805` merged, which is what
its banner says -- and every xfx file here was retaken with it. Same driver guards as every receipt: isolated tmux socket,
empty `HOME`, fake key, loopback fake gateway, browser/clipboard/sound shims.

**Sanitized after comparison**, the same way on both sides: the session
scratch directory is written `<session>`, the scenario directory `<fixtures>`,
the capture directory `<captures>`, each run directory `<scratch>`. A row that
named one of those is therefore shorter than it was on screen. Nothing else
was edited. `.ansi` files are tmux's re-serialization of cell attributes
(`capture-pane -e`), not the programs' wire bytes (see `../c0/README.md`).

## Result

**40 snap pairs; in 38 the only row that differs is row 1 (each product's
banner).** The other two (`mcp-wire` `turn` and `end.full`) differ in row 1
and in one more row, the turn's wall time (fx `1s`, xfx `2s` at this take, `0s` at an earlier one): each run's own clock.
Compared with `sgr.py` row by row in text and SGR, row 1 removed, run
directories and `.fx`/`.xfx` normalized.

| scenario | snaps | verdict |
|---|---|---|
| `mcp-empty` | `menu`, `menu-down`, `menu-tools`, `menu-esc`, `list`, `path`, `reload`, `auth-bare`, `auth-nope`, `logout-nope`, `remove-nope`, `add-bare`, `resource-nope`, `prompt-bare`, `trust-bare`, `bogus` | identical (row 1 only) |
| `mcp-fixture` | `startup` (trust prompt), `rejected`, `list-rejected`, `added`, `menu`, `menu-down`, `menu-tools`, `menu-resources`, `menu-prompts`, `list`, `resource-list`, `resource-read`, `prompt-list`, `prompt-get`, `auth-stdio`, `logout-stdio`, `turn`, `trust-approve`, `removed`, `trust-reset`, `end.full` | identical (row 1 only) |
| `mcp-wire` | `added` | identical (row 1 only) |
| `mcp-wire` | `turn`, `end.full` | row 1, and the summary's wall time, fx `1s` / xfx `2s` (row 18) -- the `├ Ran MCP mcp_fixture_echo` row, its emphasis on `MCP`, and every tool row and result identical |

## The menu's own screens (`mcp-menu-screens`)

fx: the files are the take of 2026-10-07, with the c1db919 build (banner
`v0.0.13`). A second take on 2026-11-03 gave the same 13 snaps byte for byte.
It used another c1db919 build, which carries the `c1db919112c5` stamp and
started as `v0.0.13`. xfx: taken
2026-11-03 with `target/debug/xfx` built at `2bc6d8b` (its banner). Neither
side needed sanitizing; `end.files` writes the run directory `<scratch>`, as
the driver always does.

Compared with `cells.py` (this directory), which compares **cells**, not the
text of the `.ansi` file: every printed character, its column and the
attributes on it, with the SGR state carried from row to row as tmux writes
it. Blank cells after a row's last printed character are not compared: tmux
trims them, and a space's colour and weight do not show. `/.fx/` is read as
`/.xfx/`, the product's profile directory, as the banner is. Run over every
pair in this directory, it finds no row that differs except the `mcp-wire`
wall time above.

**Nothing in the menu needed a fix at this take.** The two differences this
scenario found were fixed in `2bc6d8b`, the commit before it: the
`Capabilities` fact of a server that is not ready, and `ConnectionRefused` as
the `Error` of an HTTP server nothing listens on (`server_views.zig:130-150`,
`catalog_state.zig:23-66`).

| snap | the screen | verdict |
|---|---|---|
| `prompt` | the trust prompt at launch, `Project MCP server 'proj' is defined in .mcp.json.` and its key row | identical |
| `dismissed` | Escape: `* mcp: Project MCP approval prompts dismissed for this process.` | identical |
| `http-added` | `* mcp: Saved MCP server 'remote'.`, the reconnection line, then the warning `! mcp: MCP configuration reloaded, but server 'remote' is unavailable. Run /mcp list for details.` | identical |
| `menu` | `MCP 2  [Servers]  Tools  Resources  Prompts`; `remote  Failed  HTTP · Profile` marked (bold 255), `proj  Pending trust  stdio · Project` (245); the root hint row with `s add Slack` | identical cells (the `.ansi` text differs, below) |
| `details-1` | `remote`: `Failed`, `Profile · ~/.xfx/mcp.json`, `http`, `optional`, Protocol `unavailable`, `tools=0 resources=0 templates=0 prompts=0`, Error `ConnectionRefused`; hint `d remove  c help  esc back` | identical (`~/.xfx/` for `~/.fx/`) |
| `details-2` | `proj`: `Pending trust`, `Project · .mcp.json`, `stdio`, no Error row; hint `a approve  x reject  c help  esc back` | identical |
| `details-esc` | Escape: the list, `proj` still marked | identical cells (the `.ansi` text differs, below) |
| `info` | `c`: `Profile config ~/.xfx/mcp.json`, `Project config <workspace>/.mcp.json` (upstream's literal), the five key rows; hint `esc back` | identical (`~/.xfx/` for `~/.fx/`) |
| `confirm-approve` | `p`: `Approve all pending project MCP servers? press enter to confirm.`; hint `enter confirm     esc cancel` | identical |
| `confirm-approve-esc` | Escape: the list; nothing approved | identical |
| `confirm-reset` | `z`: `Reset all project MCP choices? press enter to confirm.` | identical |
| `confirm-reset-esc` | Escape: the list; nothing reset | identical cells (the `.ansi` text differs, below) |
| `list` | after Escape closed the menu, `/mcp list` opens the same menu | identical |
| `end.files` | `~/.xfx/mcp.json` is fx's `~/.fx/mcp.json` byte for byte (136 bytes: `remote`, `"type":"http"`, its URL, `enabled`, `startup_timeout_ms` 30000, `operation_timeout_ms` 60000) beside a `mcp.lock`; `history.jsonl` holds the same three lines | identical but for fx's session directory, which fx opens at launch and xfx with the first prompt |

**Different bytes, same cells**, the class `../t4/README.md` describes for
the clear. In `menu`, `details-esc` and `confirm-reset-esc` the `.ansi` text of
the first server row differs: xfx's file ends the row's run with `\e[0m` on
that row, fx's at the head of the next. That is where tmux writes the reset,
not what is on the screen. The terminal bytes of those three frames
(`capture.sh`'s `wire` verb, taken separately) show the reason. When the menu
opens and when Escape leaves the details, fx repaints the whole screen with
`CUP`+`2K` on each of the 40 rows (`terminal_diff.zig:619-631`, `:1160-1166`).
Its later diffs write only as far as the last changed column
(`engine.zig:2369-2444`). So row 17 holds no cell after `Profile`. xfx ended
that row with `\e[0m\e[K` when the menu replaced the 120-column `/mcp`
completion row (`src/tui/grid.rs:698-711`), and it never empties a row with
`2K`. tmux keeps the cells an `EL` cleared, and empties a row on `2K`, so
`capture-pane -e` writes the reset before the first cell it holds after the
text. On both sides that cell is blank and has the default attributes.

## Wire

`wire-frames-compare.txt` (the fixture's log of what each client sent, pid
removed): `notifications/initialized` (no params on stdio), `tools/list`
`{}` with id 1, and `tools/call`
`{"_meta":{"progressToken":2},"name":"echo","arguments":{"text":"hi"}}` with
id 2 are byte-identical. `initialize` differs in two places: `clientInfo`
(`xfx`/`0.1.0`, the product's own identity) and `capabilities` -- `{}` where
fx sends `{"elicitation":{"form":{},"url":{}}}`: xfx answers no elicitation
request, so it does not claim to (`elicitation.zig:109-130`).

`wire-bodies-compare.txt` and `wire-fixture-bodies-compare.txt` (every
completion request, set apart from the title requests -- in `mcp-wire` one
on each side with the same prompt, in `mcp-fixture` one from xfx and none in
fx's earlier research take (session titles are the plan's, not this unit's)
-- and from fx's classifier request, which xfx does not send): the `<mcp_servers>` system block is identical in every request
(including `<server name="proj" state="disabled" />` for the rejected project
server); the meta-tools are advertised from the first request and
`mcp_fixture_echo` from the step after `capability_search` selected it, with
the server's schema verbatim; every tool result the model is sent
(`capability_search`'s combined JSON, the echo envelope
`{"server":"fixture","tool":"mcp_fixture_echo","result":{…}}`, `mcp_features`'
envelope, `mcp_select_tool`'s sentence) is byte-identical. Two advertisement
differences, both deliberate:

- `capability_search`'s description drops upstream's two skill clauses
  ("Find installed skills and …", "Use returned skill locations with skill",
  `vercel-labs/fx@c1db9191 src/builtins/tools.zig:161-162`): xfx has no skills,
  and the combined result's `skills` is always `[]`.
- `mcp_select_tool`'s schema is closed (`additionalProperties: false`), where
  upstream's is open (`tools.zig:635-662`): every xfx schema is closed, which
  `tests/tool_loop.rs` holds.

## Not matched, and why

| what | upstream | xfx |
|---|---|---|
| handshake | `server/discover` probe and the modern protocol first, legacy `initialize` on fallback (`protocol_negotiation.zig:7-56`, `server_transport.zig:150-175`) | legacy `initialize` only (the brief's default); the version answered must be one upstream accepts |
| `type: "sse"` servers | connected | read and listed, never connected (`McpTransportUnsupported`) |
| `notifications/*/list_changed` | followed (`tool_subscription.zig:48-50`) | not subscribed; `/mcp reload` refreshes |
| `auto` mode and a server's tool | cleared by upstream's reviewer (`tool_admission.zig:1116-1157`) | held: there is no reviewer, so a call is admitted by a configured rule (`{"permission":{"mcp_fixture_echo":"allow"}}`, what `mcp-wire` seeds on the xfx side) or asked in `ask` |
| menu `a` | the add form, its preview and argument screens | composes `/mcp add ` in the composer |
| menu `s`, `/mcp auth`, `/mcp logout`, `/mcp add slack`, HTTP OAuth | browser sign-in and token storage | gated (G): upstream's own words for a leaf that is unavailable -- `Interactive MCP authentication is unavailable here.`, `MCP logout is unavailable here.` -- identical on screen (`auth-*`, `logout-*`, the menu feedback) |
| Ctrl-C / Ctrl-D at the trust prompt | consumed by the prompt (`app_input_runtime.zig:884-937`) | pass through to the shell's own handling |
| meta-tools outside the TUI | `fx ask` advertises them too, when its mode allows `mcp_features` and no rule denies it (`cli_ask.zig:4116-4117`) | only where an MCP client runs (the TUI); `xfx ask` starts no MCP server and advertises the nine built-ins |
| settings backups | `settings.json.backup.*`, five kept (`settings_store.zig:20`, `:668-700`) | none: xfx's settings writer keeps none for any key, trust choices included |
