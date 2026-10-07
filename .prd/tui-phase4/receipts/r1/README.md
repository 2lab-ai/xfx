# R1 receipt: the three permission-rule divergences W8 left

Four scenarios, each run at 40x120 on both binaries with the repo's own driver:

```
TMPDIR=/private/tmp/claude-501 FX_BIN=<fx c1db919 build, 0.0.13> scripts/fxcap/capture.sh fx  <scenario> 40x120 <dir>
TMPDIR=/private/tmp/claude-501                                    scripts/fxcap/capture.sh xfx <scenario> 40x120 <dir>
```

for `rules-read`, `rules-read-auto`, `rules-hidden` and `rules-hidden-all`
(`scripts/fxcap/scenarios/`, new in this unit, with the workspace
`rules-ws/` and the profiles `rules-read-settings.json`,
`rules-hidden-settings.json`, `rules-hidden-all-settings.json`). Each binary
reads the same profile shape: `{"permission": …}` at the top of its own
`settings.json`, the user table.

The xfx binary is `target/debug/xfx` built from this branch with
`plan/tui-phase4` merged (its banner says `0.1.0-0d26715 [debug]`). `before/`
holds the same `rules-read` snaps from `target/debug/xfx` at `0a1bb45`
(`plan/tui-phase4` when this unit started), the state W8 left. The driver,
its shims and its fx sound guard are unchanged; every fx run passed the
guard.

Sanitized: every run directory (`/private/tmp/claude-501/fxcap.XXXXXX`) is
written `<scratch>`, in the screens and in the request bodies. No file names a
key, a home or a user. Request bodies are kept for what the receipt is about
and nothing else: every completion of the two hidden-tool scenarios, and the
completion after the refusal (`003`) of the two read scenarios.

## How the pairs were compared

Each `.txt` and `.ansi` pair, row by row, after two normalisations and nothing
else: row 1 (the welcome line, `𝒇x v0.0.13` against `x𝒇x v0.1.0-0d26715
[debug]`) is dropped, and trailing blanks are trimmed.

## The decision table, before and after

| Case (mode ask or auto unless said) | fx (c1db919) | xfx before (0a1bb45) | xfx after |
|---|---|---|---|
| read, `read: {"secret*": "ask"}` | band asks (`tool_admission.zig:1430-1444`) | refused: `… asks before \`read_file\` reads …, and xfx does not ask about reads` (`before/…-ask`) | band asks |
| same, in auto | asks | refused | asks |
| same, full access | reads (`:1362-1370`) | reads | reads |
| same, nobody to ask (`xfx ask` piped) | `permission_required`, `configured_rule` (`:1430-1436`) | refused | refused, naming the rule |
| read no rule decides, any mode | reads, `.once` (`:1124-1126`) | reads | reads |
| read, `read: deny` (with a grant, with a later ask) | refused (`:1386-1390`) | refused | refused, nobody asked |
| answered `1` | reads | -- | reads, no grant |
| answered `2` | grants `edit read glob grep -> <workspace>/**` (`suggestedSessionGrants`, `permissions.zig:1202-1225`) | -- | grants `read_file -> <that file>` only |
| answered `3` (amended) | `execution-denied`, sentence after the result | -- | `execution-denied`, sentence after the result |
| Escape | turn stops, `■ Cancelled secret.txt` | -- | same |
| `/permissions remember allow read_file {…}` | prepared, `$ read_file secret.txt` (`:2261-2267`) | refused (`asks for no permission`) | prepared, same band |
| remembered read | admitted before the ask | -- | admitted before the ask |
| saved-session store unreadable, any read | call fails (`:1278-1309`) | read unaffected | read refused |
| `edit: {"*": "deny"}`, request `tools` | no `write_file`/`edit_file` (`tool_projection.zig:563-566`) | all nine tools | no `write_file`/`edit_file`/`create_folder` |
| `"permission": "deny"` | only `web_fetch` and `vision`, neither of which the rule hides nor xfx has | all nine tools | `[]`, `toolChoice` `auto` |
| full access with any deny | everything (`:563`) | everything | everything |
| `bash: deny` | `shell` still offered | `terminal` offered | `terminal` offered |
| `shell: deny` | `shell` hidden | `terminal` offered | `terminal` hidden |
| rule changed between turns | next job's projection (`app_agent_runtime.zig:1048-1052`) | n/a | next turn's list |
| rule changed mid-turn | that job keeps its projection (`orchestrator.zig:7270-7278`) | n/a | that turn keeps its list; the next call is judged by the new rule |
| `url` / `web-fetch-domain` rules | listed the same with or without web tools (`session_commands.zig:987-1045`, `:1489-1545`); a `url` rule decides nothing for `web_fetch` (`permissions.zig:2565-2583`) | listed, decide nothing | unchanged: listed, decide nothing (no xfx tool reaches a URL) |

## Per snap

### `rules-read` (7 snaps; 3 identical, 4 differ by what is listed below)

`ask` mode, `read: {"secret*": "ask", …}`. Four turns read `secret.txt`.

| snap | what it shows | result |
|---|---|---|
| `ask` | the band: `Permission needed · Choose one` / `Tool`, `Would you like to allow this action?`, `Reason: This action needs approval before … can continue.` (`Reason:` in 245), `read_file secret.txt`, `❯ 1. Yes` / `2. Yes, and don't ask again for this request` / `3. No`, the hint at column 0 in 245; above it `● 1 tool call · 1 read` / `└ Reading secret.txt` | **differs** in two places, both xfx's own and both on every band it draws: `fx` → `xfx` in the reason row, and xfx's dim `2 = allow every future read_file of \`secret.txt\` …` scope row under the choices (`scope_rows`, `src/tui/approval_view.rs`; the same row is under every xfx band, see `d1/xfx-resume-tools-40x120-cmd-ask`) |
| `no` | `3`: `● 1 tool call · 1 read · 1 denied` / `└ Denied secret.txt`, the answer | identical |
| `ask2` | the second turn's band | as `ask` |
| `once` | `1`: `└ Read secret.txt`, the answer | identical |
| `always` | `2` on the third turn | identical |
| `granted` | the fourth turn reads `secret.txt` and `notes.txt` without a question | **differs** on the dim summary row only: `0s` / `1s` (a debug build's timing) |
| `listed` | bare `/permissions` | **differs** by the grant `2` kept: fx ` - edit -> **` / ` - read -> **` / ` - glob -> **` / ` - grep -> **`, xfx ` - read_file -> secret.txt` (the divergence below); the scrolled-off row above follows from it |

`before/`: xfx at `0a1bb45` refused the read with no band (`before/xfx-rules-read-40x120-ask`: `└ Denied secret.txt` while fx asks), and `listed` had no grant.

Bodies (`003-completion.json`): the refused read's result is `execution-denied` on both. fx's `reason` is its JSON envelope (`tool_permission_denied`, `Permission denied by user`); xfx's is its sentence `read_file was not permitted: you declined \`read_file\` for \`<scratch>/work/secret.txt\``, as for every xfx refusal.

### `rules-read-auto` (9 snaps; 0 identical, each differing only as listed)

`auto` mode. A search a `grep: {"*": "ask"}` rule asks about, refused with `3` amended `look in notes.txt instead`; a read Escaped; then `/permissions remember allow read_file {"path":"secret.txt"}`, confirmed, and the same read runs unasked.

| snap | result |
|---|---|
| `grep-ask` | the band in auto, `grep_files secret` (a search's label is its pattern, `formatPermissionLabel`, `tool_presentation.zig:683-718`); differs as `rules-read`'s `ask` (`xfx`, the scope row: `2 = allow every future grep_files below \`.\` …, whatever it searches for`) |
| `grep-amend` | `❯ 3. No, look in notes.txt instead`; as `grep-ask` |
| `grep-no` | `└ Denied secret`, `1 denied`; **differs** in how the amendment is drawn: fx a user card `┃ look in notes.txt instead`, xfx `[you] look in notes.txt instead` -- xfx's amendment row on every band (`AMENDMENT_PREFIX`, `src/tui/shell.rs`; already in `f2/xfx-approvals-*`), not this unit's. The wire is the same: the sentence is a user message after the `execution-denied` result on both (`003-completion.json`) |
| `read-ask` | the read band in auto; as `rules-read`'s `ask`, plus the `[you]` row above |
| `read-esc` | Escape: `● 1 tool call · 1 read · 1 cancelled` / `└ Cancelled secret.txt` / `■ Cancelled secret.txt · What can … do differently?`; differs by `fx` → `xfx` (F4's row) and the `[you]` row |
| `remember-prompt` | `Remember allow for this saved session?`, `$ read_file secret.txt`, `❯ 1. Confirm` / `2. Cancel`: the band's rows identical; differs above it only by the `[you]` and `xfx` rows |
| `remember-done` | `* permissions: saved-session permission rule updated`; as above |
| `remembered` | the same read, no question, `└ Read secret.txt`; as above |
| `listed` | `saved-session permission rules (1):` / `1 allow read_file secret.txt` in both; as above |

### `rules-hidden` (2 snaps; 1 identical) and its bodies

`ask`, `edit: {"*": "deny"}` in the profile; a turn; `/allowlist add tool write_file` (a local table, which shadows the profile's whole); a second turn.

Request `tools`, by name:

| request | fx | xfx |
|---|---|---|
| first turn | `read_file glob_files grep_files shell subagent capability_search skill install_skill mcp_select_tool mcp_features ask_user_question web_fetch exa_search read_tool_result vision` -- no `edit_file`, `write_file` | `list_files glob_files grep_files read_file terminal ask_user_question` -- no `write_file`, `edit_file`, `create_folder` |
| second turn | `edit_file`, `write_file` back | `write_file`, `edit_file`, `create_folder` back |

`toolChoice` is `{"type":"auto"}` in all four. Screens: `first` identical; `second` differs on the summary row's seconds only (`0s` / `1s`).

### `rules-hidden-all` (1 snap) and its body

`"permission": "deny"`: fx's `tools` is `web_fetch` and `vision` -- `rulesDenyAllTargetsForTool` never hides `web_fetch` (`permissions.zig:1139`), and `appendBuiltinTool` returns before any rule for `vision` (`tool_projection.zig:562`; where the request then gets it was not traced, since xfx has no such tool) -- and xfx, which has neither, sends `"tools":[]` with `"toolChoice":{"type":"auto"}` (fx sends `toolChoice` over any list, `vercel_protocol.zig:363-368`). The screen differs on the summary row's seconds only.

## Not matched, and why

- **"Always" for a read.** fx grants `edit`, `read`, `glob` and `grep` on the whole workspace tree (`suggestedSessionGrants`, `permissions.zig:1202-1225`): answering `2` about one read authorizes every later write in the workspace for the session. xfx keeps the grant it keeps for every question -- this tool at this resolved file (a search: this tool below this directory) -- so a grant never authorizes more than the question showed. Narrower, never wider.
- **The refusal's reason on the wire** is xfx's sentence, fx's is a JSON envelope; the same for every refusal xfx makes, not this unit's.
- **The scope row, the `[you]` amendment row and `xfx` for `fx`** are xfx's on every band.
- **A saved-session key for a read** is xfx's encoding of the same facts (the tool, its decoded input field by field, the resolved target) where fx keys the raw arguments JSON (`permissionStateKeyForCall`, `tool_admission.zig:2219-2235`); a store is not shared between the products either way (the `/permissions` row).
- **A call to `ask_user_question` anyway, under a rule that covers it** (`*: deny`): the tool is left out of the request as upstream's is, but xfx has never judged that tool against the table (W8's rules decide reads, changes and commands), where upstream refuses it at admission -- it has a permission contract and its target is its own name (`toolHasPermissionContract`, `tool_admission.zig:101-103`; `.none`, `permissions.zig:227`). Found here, not changed by this unit.
- **`terminal` is hidden by `shell: deny`, not by `bash: deny`.** That is fx's behaviour for its `shell` (`permissionNameForTool` maps `shell` to no category, `permissions.zig:1455-1464`), ported as it is: the request carries what fx's would for the same settings file. A call to the terminal is still judged as `bash`/`run_command`, so `bash: deny` refuses every command it would run.

## What the captures do not show

Pinned by tests rather than screens: fx's three `rulesDenyAllTargetsForPermission` cases and the ask/allow ways back through a global deny (`src/permission/rules.rs`); the offered list per mode and table, and `terminal` as `shell` (`src/tools/mod.rs`); the list kept for a whole turn and taken afresh by the next, and full access offering everything (`src/agent/machine.rs`); a read's verdict in every mode, the exact grant, the question's words and the key (`src/permission/policy.rs`); the tools end to end -- ask in ask and auto, free reads in every mode, full access past a deny, No, deny over ask and grant, the exact always, nobody to ask, remembered allow and deny, a broken store, preparing a read, a hidden tool called anyway (`tests/permissions.rs`); the band and the hidden request on a real pseudoterminal (`tests/tui.rs`), and the line shell's `y/a/n` for a read (`tests/interactive.rs`).
