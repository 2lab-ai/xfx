# M1 receipt: `/mcp` and the MCP client (local leaves)

Three scenarios (`scenarios/`), each run at 40x120 on both binaries with the
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
and in one more row, the turn's wall time (`1s` / `0s`): each run's own clock.
Compared with `sgr.py` row by row in text and SGR, row 1 removed, run
directories and `.fx`/`.xfx` normalized.

| scenario | snaps | verdict |
|---|---|---|
| `mcp-empty` | `menu`, `menu-down`, `menu-tools`, `menu-esc`, `list`, `path`, `reload`, `auth-bare`, `auth-nope`, `logout-nope`, `remove-nope`, `add-bare`, `resource-nope`, `prompt-bare`, `trust-bare`, `bogus` | identical (row 1 only) |
| `mcp-fixture` | `startup` (trust prompt), `rejected`, `list-rejected`, `added`, `menu`, `menu-down`, `menu-tools`, `menu-resources`, `menu-prompts`, `list`, `resource-list`, `resource-read`, `prompt-list`, `prompt-get`, `auth-stdio`, `logout-stdio`, `turn`, `trust-approve`, `removed`, `trust-reset`, `end.full` | identical (row 1 only) |
| `mcp-wire` | `added` | identical (row 1 only) |
| `mcp-wire` | `turn`, `end.full` | row 1, and the summary's wall time `1s`/`0s` (row 18) -- the `├ Ran MCP mcp_fixture_echo` row, its emphasis on `MCP`, and every tool row and result identical |

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
completion request; the title requests, the same on both sides, and fx's
classifier request, which xfx does not send, set aside): the `<mcp_servers>` system block is identical in every request
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
