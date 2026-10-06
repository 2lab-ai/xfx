# W8 receipt: `/allowlist`, `/permissions remember|revoke`, configured rules

Five scenarios, each run at 40x120 on both binaries with the repo's own driver:

```
TMPDIR=/private/tmp/claude-501 FX_BIN=<fx c1db919 build, 0.0.13> scripts/fxcap/capture.sh fx  <scenario> 40x120 <dir>
TMPDIR=/private/tmp/claude-501                                    scripts/fxcap/capture.sh xfx <scenario> 40x120 <dir>
```

for `allowlist`, `allowlist2`, `allowlist3`, `allowlist-turn` and
`allowlist-remember` (`scripts/fxcap/scenarios/`). The first three replay the
research scenarios of `appearance-security-ext.md` §`/allowlist`;
`allowlist-remember` replays `permissions3.cap` of §`/permissions` with a turn
in front (xfx opens its saved session at the first prompt, fx at launch -- the
turn makes the two comparable and is in both runs); `allowlist-turn` is new:
a rule added in `ask` mode is in force for the very next tool call.

The xfx binary is `target/debug/xfx` built from `41cf01d` (this branch with
`plan/tui-phase4` at `acdb2b5` merged), which is what its banner says. The
driver, its shims, `FX_NO_OPEN_BROWSER=1` and its fx sound guard are
unchanged; every fx run passed the guard. Sanitized: the only run path in any
file is the workspace, already written `<scratch>/work` by `capture.sh`; no
capture names a scratch directory, a key or a home.

## How the pairs were compared

Each `.txt` and `.ansi` pair, row by row, after two normalisations and nothing
else: row 1 (the welcome line, `𝒇x v0.0.13` against `x𝒇x v0.1.0-41cf01d
[debug]`) is dropped, and trailing blanks are trimmed. 20 of the 25 snaps are
identical in text **and** SGR under that (10 + 2 + 1 + 0 + 7 across the five
scenarios); the five that are not are explained per snap below, and none of
them is a row this unit draws. One README for all of them, with a row per
snap, as the earlier receipts of this workstream have.

## Per snap

### `allowlist` (10 snaps, all identical)

| snap | what it shows | result |
|---|---|---|
| `bare` | `/allowlist` on an empty profile: `* allowlist: effective persistent allow rules: (none)` | identical |
| `view` | `/allowlist view`, the same | identical |
| `add-cmd` | `/allowlist add command "git *"`: `* allowlist: added command: "git *" (scope=local)` | identical |
| `user-add-url` | `/allowlist user add url https://example.com/*` while a local table exists: `…; user rules shadowed by local settings` | identical |
| `view-effective` | only the local table, `    commands:` / `      git *`, then `  user rules are shadowed by local settings` (continuation rows indented two cells by the notice, each in 250) | identical |
| `view-local` | the local table only | identical |
| `view-user` | `    urls:` / `      https://example.com/*` and the shadow line | identical |
| `remove-cmd` | `removed command: "git *" (scope=local)` | identical |
| `local-reset` | `✗ usage: /allowlist reset [commands\|tools\|urls\|web-fetch-domains\|all]` -- glyph 252, body 250, no topic | identical |
| `bogus` | `✗ usage: /allowlist [view [effective\|local\|user]\|[local\|user] add\|remove\|reset ...]` | identical |

`files-mid` / `files` (`.files`): the same document, in the same key order --
`workspaces` then `permission`, the entry keyed by the canonical workspace --
and after the remove the local table is `{}`, which still shadows, in both.
**Differs:** fx writes compact JSON and keeps a backup of every prior version
under `~/.fx/backups/`; xfx's profile writer writes pretty JSON and keeps no
backups. Both are the profile writer's (`provider::profile`), shared by every
settings write since D-units, not this command's; what this unit changed in it
is that a write keeps the file's key order (below).

### `allowlist2` (4 snaps; 2 identical, 2 offset by the frame)

| snap | result |
|---|---|
| `adds` | identical: `added tool read: "*"`, `added tool edit: "*"` (for `write_file`), the add usage line for `tool nonsense`, `added web-fetch-domain: "domain:example.com"`, and `added command: "git *"` twice |
| `view-local` | identical: `tools:` with `read: workspace` and `edit: workspace`, `commands:`, `web-fetch domains:` |
| `after-reset` | **every row identical one row lower**: fx row *i* = xfx row *i+1* for rows 2-39, text and SGR. The document had filled the screen; fx's footer sits one row higher than xfx's then (fx row 37 hint, xfx row 38). That is the floating footer's pinning under a full document (F1, `footer_layout.zig:39-96`), not anything `/allowlist` draws |
| `final` | the same rows in the same order and colours, at a different height: fx's footer ends at row 35 with blank rows under it, xfx's is pinned at the last row. Same cause as `after-reset` |

`files-mid` / `files`: as `allowlist`'s; after `local reset all` the entry is
`{}` in both (`reset` drops an emptied table), and `user reset all` drops the
user table.

### `allowlist3` (1 snap, identical)

`after`: an add and a remove leave the local table `{}`; a user URL is then
added (`…; user rules shadowed by local settings`), and `view effective` says
`effective persistent allow rules: (none)` with the shadow line, and `view
user` lists the URL with it. Identical. `empty-table.files`: `{"workspaces":
{"<scratch>/work": {"permission": {}}}}` in both, pretty against compact.

### `allowlist-turn` (3 snaps; content identical, the turn summary differs)

`ask` mode. A first turn opens the session; `/allowlist add command "printf *"`
(`added`); then a turn whose model runs `printf allowed-by-rule` (`ran`): no
question in either binary, `● 1 tool call · 1 command` / `└ Ran printf
allowed-by-rule`, and the answer; then a bare `/permissions` (`listed`):
`configured rules:` / ` - allow bash -> printf *` in both.

**Differs**, on the dim turn-summary rows only (rows 7 and 18): the elapsed
seconds (`0s` / `1s`, a debug build's timing) and, for the two-step turn, the
output-token count (`↓80` in fx, `↓40` in xfx). That summary is the
transcript's (F5), not this unit's; the second is recorded under "found" in the
report.

### `allowlist-remember` (7 snaps, all identical)

| snap | what it shows | result |
|---|---|---|
| `remember-prompt` | `/permissions remember allow write_file {"path":"a.txt","content":"x"}`: the confirmation band -- `Permission needed · Choose one` with `Permission rule` on the right (245), `Remember allow for this saved session?` bold, the explanation in 245, `$ write_file a.txt preimage=absent after=2d711642b726b044` in bold 255, `❯ 1. Confirm` / `2. Cancel`, hint `1–2 choose    enter confirm    esc cancel` | identical |
| `remember-esc` | Escape closes it and says nothing | identical |
| `remember-confirmed` | a remembered deny, confirmed with Enter: `* permissions: saved-session permission rule updated` | identical |
| `listed` | bare `/permissions`: the three-row usage, then `saved-session permission rules (1):` / `1 deny write_file b.txt preimage=absent after=a1fce4363854ff88` | identical |
| `revoke-prompt` | `/permissions revoke 1`: `Revoke this saved-session permission rule?` / `Confirm revocation of this stored rule. No current action will be prepared or run.` | identical |
| `revoke-done` | confirmed: `rule updated` again | identical |
| `refusals` | `revoke 99`: `! permissions: saved-session permission rule not found`; bare `remember`: the two-row `✗ usage: /permissions remember …` | identical |

`files.files`: the session's `permissions.json` is byte-identical in both,
`{"schema_version":2,"next_generation":3,"rules":[]}` (one rule made, one
revoked, two generations spent). xfx also leaves an empty
`permissions.lock` beside it, the lock its compare-and-swap is taken under.

## What the captures do not show

The rule language is pinned by tests rather than by screens: last-match-wins in
document order, the static-command narrowing of a wildcard `bash` allow,
`dir/**`, case and symlink spellings, a deny on a directory a write creates,
the read tools' rules, a saved rule's exact-action key, the generation
compare-and-swap, and a store that does not read refusing the call
(`src/permission/rules.rs`, `src/permission/policy.rs`,
`src/permission/session_rules.rs`, `src/permission/allowlist.rs`,
`tests/permissions.rs`).
