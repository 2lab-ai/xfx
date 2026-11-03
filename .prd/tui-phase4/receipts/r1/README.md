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

## The binaries

**xfx** is `target/debug/xfx` built at `dc093e4f` -- this branch with
`plan/tui-phase4` `4700d160` merged, so after T10 (`9054bb8c`), T8 r2, D2c r4
and W9 r5 -- and its banner says `0.1.0-dc093e4 [debug]`. Every xfx snap here
was taken from it on 2026-11-03. Against the xfx snaps of 2026-10-07 (banner
`0d26715`) they differ only in the session id inside the scope row and in the
timing rows: T10's command wrap and its control-byte spellings reach none of
these screens (no command band, no control byte, no row that wraps at 120
columns). The commit that carries this receipt differs from `dc093e4f` in
documentation, in comment lines and in `scripts/fxcap/capture.sh` (plan
`d91a9968`) only.

**fx** is the October capture (2026-10-07) from a c1db9191 build, banner
`𝒇x v0.0.13`. On 2026-11-03 the c1db919 reference binary the captures
are driven with turned out to have been replaced by fx's own auto-upgrade
with v0.0.16 (the upgrade runs unless `FX_AUTO_UPGRADE=0`,
`src/main.zig:727-733`). One probe run made through it that day was thrown
away. The rebuilt reference (sha256 `7b76319a871163ad…`, `--version` 0.0.13)
reproduces all 19 fx snaps of the four scenarios byte for byte under the
comparison below, and their request bodies but for the date in
`<fx-turn-context>` (`date_utc`), so the October snaps stay as the fx side.
The driver now launches fx with `FX_AUTO_UPGRADE=0` (plan `d91a9968`).

`before/` holds the same `rules-read` snaps from `target/debug/xfx` at
`0a1bb45` (`plan/tui-phase4` when this unit started), the state W8 left.

Sanitized: every run directory (`/private/tmp/claude-501/fxcap.XXXXXX`) is
written `<scratch>`, in the screens and in the request bodies. No file names a
key, a home or a user. Request bodies are kept for what the receipt is about
and nothing else: every completion of the two hidden-tool scenarios, and the
completion after the refusal (`003`) of the two read scenarios. The xfx bodies
of 2026-11-03 are byte-identical to the October ones after that substitution.

## How the pairs were compared

Each `.txt` and `.ansi` pair, row by row, after two normalisations and nothing
else: row 1 (the welcome line, `𝒇x v0.0.13` against `x𝒇x v0.1.0-dc093e4
[debug]`) is dropped, and trailing blanks are trimmed.

## The decision table, before and after

fx lines are `vercel-labs/fx@c1db9191`, `src/core/tooling/tool_admission.zig`
unless another file is named.

| Rule | Tool | Mode | fx (c1db919) | xfx before (0a1bb45) | xfx after |
|---|---|---|---|---|---|
| `read: {"secret*": "ask"}` | `read_file secret.txt` | ask | **ask** (`configured_ask`, `:1426-1442`) | **deny**: `… asks before \`read_file\` reads …, and xfx does not ask about reads` (`before/…-ask`) | **ask** |
| same | same | auto | **ask** (the ask comes before the mode) | **deny** | **ask** |
| same | same | full access | **allow** (yolo returns before any rule, `:1369-1376`) | allow | allow |
| same | same | no approval channel (`xfx ask` piped) | **deny**: `permission_required`, `configured_rule` (`:1426-1431`) | deny | **deny**, naming the rule |
| `grep: {"*": "ask"}` | `grep_files secret` | auto | **ask** | deny | **ask** |
| none decides | any read tool | any | **allow**, `.once` (`:1123-1125`) | allow | allow |
| `read: deny`, with a grant or a later ask | `read_file` | ask, auto | **deny**, nobody asked (`:1389-1391`) | deny | **deny**, nobody asked |
| ask rule, answered `1` | `read_file` | ask, auto | allow once | -- | allow once, no grant |
| ask rule, answered `2` | `read_file` | ask, auto | allow; grants `edit read glob grep -> <workspace>/**` (`suggestedSessionGrants`, `permissions.zig:1217-1224`) | -- | allow; grants `read_file -> <that file>` only (Not matched, below) |
| ask rule, answered `3` amended | `grep_files` | auto | **deny**: `execution-denied`, the sentence after the result | -- | same |
| ask rule, Escape | `read_file` | auto | turn stops, `■ Cancelled secret.txt` | -- | same |
| `/permissions remember allow read_file {…}` | `read_file` | auto | prepared, `$ read_file secret.txt` (`preparePermissionStateAction`, `:2257-2267`) | refused (`asks for no permission`) | prepared, the same band |
| remembered allow, ask rule | `read_file` | auto | **allow** before the ask (`:1405-1424`) | -- | **allow** before the ask |
| saved-session rules unavailable | any | ask, auto | **fails the call**: the copy of the in-memory rules taken before every decision (`requestPermissionOutcome`, `:1286-1291`; `snapshotPermissionState`, `session.zig:1816-1823`) | read unaffected | **deny**: xfx reads the file at every decision, and a file that does not read refuses |
| same | any | full access | the copy is still taken first (`:1286-1291` runs before `:1369`), then nothing in it is consulted | -- | reads nothing, consults nothing |
| saved rules present, the call's key cannot be made | any | ask, auto | saved rules skipped (`permissionStateKeyForCall(…) catch null`, `:1406`) | -- | **deny** (W8's, `src/permission/policy.rs:1107`); stricter than fx |
| `edit: {"*": "deny"}` | request `tools` | ask, auto | **hidden**: no `write_file`, `edit_file` (`appendBuiltinTool`, `tool_projection.zig:563-566`; `rulesDenyAllTargetsForTool`, `permissions.zig:1138-1166`) | offered: all nine | **hidden**: no `write_file`, `edit_file`, `create_folder` |
| `"permission": "deny"` | request `tools` | ask | **hidden** but `web_fetch` (never hidden, `permissions.zig:1139`) and `vision` (returns before any rule, `tool_projection.zig:562`) | offered: all nine | **hidden**: `[]`, `toolChoice` `auto` |
| any deny | request `tools` | full access | offered: everything (`tool_projection.zig:563`) | everything | everything |
| `bash: deny` | `shell` / `terminal` | ask | offered (`shell` maps to no category, `permissions.zig:1455-1464`) | offered | offered; every command it runs **deny** |
| `shell: deny` | `shell` / `terminal` | ask | **hidden** | offered | **hidden**; a call anyway is judged as `bash`/`run_command`, so **ask** |
| rule changed between turns | request `tools` | any | the next job's projection (`app_agent_runtime.zig:1048-1051`) | n/a | the next turn's list |
| rule changed mid-turn | request `tools` | any | that job keeps its projection (`orchestrator.zig:7269-7278`); the next call is judged by the new rule | n/a | the same |
| `url` / `web-fetch-domain` | listing; any tool | any | listed the same with or without web tools (`session_commands.zig:987-1045`, `:1489-1545`); a `url` rule decides nothing for `web_fetch` (`permissions.zig:2562-2586`) | listed, decides nothing | unchanged: listed, decides nothing (no xfx tool reaches a URL) |

## Per snap

### `rules-read` (7 snaps; 4 identical, 3 differ by what is listed below)

`ask` mode, `read: {"secret*": "ask", …}`. Four turns read `secret.txt`.

| snap | what it shows | result |
|---|---|---|
| `ask` | the band: `Permission needed · Choose one` / `Tool`, `Would you like to allow this action?`, `Reason: This action needs approval before … can continue.` (`Reason:` in 245), `read_file secret.txt`, `❯ 1. Yes` / `2. Yes, and don't ask again for this request` / `3. No`, the hint at column 0 in 245; above it `● 1 tool call · 1 read` / `└ Reading secret.txt` | **differs** in two places, both xfx's own and both on every band it draws: `fx` → `xfx` in the reason row, and xfx's dim `2 = allow every future read_file of \`secret.txt\` …` scope row under the choices (`scope_rows`, `src/tui/approval_view.rs`; the same row is under every xfx band, see `d1/xfx-resume-tools-40x120-cmd-ask`), two rows that leave two fewer blank rows under the hint |
| `no` | `3`: `● 1 tool call · 1 read · 1 denied` / `└ Denied secret.txt`, the answer | identical |
| `ask2` | the second turn's band | as `ask` |
| `once` | `1`: `└ Read secret.txt`, the answer | identical |
| `always` | `2` on the third turn | identical |
| `granted` | the fourth turn reads `secret.txt` and `notes.txt` without a question | identical |
| `listed` | bare `/permissions` | **differs** by the grant `2` kept: fx ` - edit -> **` / ` - read -> **` / ` - glob -> **` / ` - grep -> **`, xfx ` - read_file -> secret.txt` (Not matched, below); the rows scrolled off above follow from it |

`before/`: xfx at `0a1bb45` refused the read with no band (`before/xfx-rules-read-40x120-ask`: `└ Denied secret.txt` while fx asks), and `listed` had no grant.

Bodies (`003-completion.json`): the refused read's result is `execution-denied` on both. fx's `reason` is its JSON envelope (`tool_permission_denied`, `Permission denied by user`); xfx's is its sentence `read_file was not permitted: you declined \`read_file\` for \`<scratch>/work/secret.txt\``, as for every xfx refusal.

### `rules-read-auto` (9 snaps; 0 identical, each differing only as listed)

`auto` mode. A search a `grep: {"*": "ask"}` rule asks about, refused with `3` amended `look in notes.txt instead`; a read Escaped; then `/permissions remember allow read_file {"path":"secret.txt"}`, confirmed, and the same read runs unasked.

| snap | result |
|---|---|
| `grep-ask` | the band in auto, `grep_files secret` (a search's label is its pattern, `formatPermissionLabel`, `tool_presentation.zig:683-718`); differs as `rules-read`'s `ask` (`xfx`, the scope row: `2 = allow every future grep_files below \`.\` …, whatever it searches for`) |
| `grep-amend` | `❯ 3. No, look in notes.txt instead`; as `grep-ask` |
| `grep-no` | `└ Denied secret`, `1 denied`; **differs** in how the amendment is drawn: fx writes it as a user card, `┃ look in notes.txt instead` and a blank row under it (`write_user_prompt`, `app_worker_runtime.zig:1020-1026`), xfx as one `[you] look in notes.txt instead` row -- xfx's amendment row on every band (`AMENDMENT_PREFIX`, `src/tui/shell.rs`; the full-screen TUI row of `docs/parity.md`, already in `f2/xfx-approvals-*`), not this unit's -- so every row under it sits one higher; and the grep turn's summary says `6s` where fx's says `7s` (the turn's time, the band's included) |
| `read-ask` | the read band in auto; as `rules-read`'s `ask`, plus the `[you]` row and the `6s`/`7s` above |
| `read-esc` | Escape: `● 1 tool call · 1 read · 1 cancelled` / `└ Cancelled secret.txt` / `■ Cancelled secret.txt · What can … do differently?`; differs by `fx` → `xfx` (F4's row), the `[you]` row and `6s`/`7s` |
| `remember-prompt` | `Remember allow for this saved session?`, `$ read_file secret.txt`, `❯ 1. Confirm` / `2. Cancel`: the band's rows identical; differs above it only as `read-esc` |
| `remember-done` | `* permissions: saved-session permission rule updated`; as above |
| `remembered` | the same read, no question, `└ Read secret.txt`; as above |
| `listed` | `saved-session permission rules (1):` / `1 allow read_file secret.txt` in both; as above, and in the `.ansi` only: fx's row has no `\e[39m` after `secret.txt`, xfx's has. No glyph differs: the blank cells after the text keep colour 250 in fx's terminal because fx's last frame writes the row as `\e[36;3H1 allow read_file secret.txt` with the colour carried from before and leaves the rest of the row as an earlier frame left it (a `wire` of the restored reference), where xfx writes `\e[38;5;250m1 allow read_file secret.txt\e[0m\e[K`. fx's own capture of the same row in `w8/fx-allowlist-remember-40x120-listed.ansi` ends in `\e[39m`, so it is not a rule of fx's for that row |

The wire: the amendment is a user message after the `execution-denied` result on both (`003-completion.json`).

### `rules-hidden` (2 snaps; 2 identical) and its bodies

`ask`, `edit: {"*": "deny"}` in the profile; a turn; `/allowlist add tool write_file` (a local table, which shadows the profile's whole); a second turn.

Request `tools`, by name:

| request | fx | xfx |
|---|---|---|
| first turn | `read_file glob_files grep_files shell subagent capability_search skill install_skill mcp_select_tool mcp_features ask_user_question web_fetch exa_search read_tool_result vision` -- no `edit_file`, `write_file` | `list_files glob_files grep_files read_file terminal ask_user_question` -- no `write_file`, `edit_file`, `create_folder` |
| second turn | `edit_file`, `write_file` back | `write_file`, `edit_file`, `create_folder` back |

`toolChoice` is `{"type":"auto"}` in all four. Screens: `first` and `second` identical.

### `rules-hidden-all` (1 snap; identical) and its body

`"permission": "deny"`: fx's `tools` is `web_fetch` and `vision` -- `rulesDenyAllTargetsForTool` never hides `web_fetch` (`permissions.zig:1139`), and `appendBuiltinTool` returns before any rule for `vision` (`tool_projection.zig:562`; where the request then gets it was not traced, since xfx has no such tool) -- and xfx, which has neither, sends `"tools":[]` with `"toolChoice":{"type":"auto"}` (fx sends `toolChoice` over any list, `vercel_protocol.zig:363-368`). The screen is identical.

## Not matched, and why

- **"Always" for a read.** fx grants `edit`, `read`, `glob` and `grep` on the whole workspace tree (`suggestedSessionGrants`, `permissions.zig:1202-1225`, the path grants at `:1217-1224`): answering `2` about one read authorizes every later write in the workspace for the session. xfx keeps the grant it keeps for every question -- this tool at this resolved file (a search: this tool below this directory) -- so a grant never authorizes more than the question showed. Narrower, never wider.
- **The refusal's reason on the wire** is xfx's sentence, fx's is a JSON envelope; the same for every refusal xfx makes, not this unit's.
- **The scope row, the `[you]` amendment row and `xfx` for `fx`** are xfx's on every band. fx draws an amendment as a user card (`app_worker_runtime.zig:1020-1026`); xfx's `[you]` row is written when the runtime reports the delivery, which is why it is its own row (the full-screen TUI row).
- **The saved-session rules.** fx keeps them in memory and copies them before every decision; xfx reads `permissions.json` at every decision, so where fx fails a call on a copy it cannot take, xfx fails it on a file that does not read. Where fx cannot make a call's key it skips the saved rules (`:1406`); xfx refuses the call (W8's). In ask and auto each difference only refuses more; in full access neither consults a saved rule.
- **A saved-session key for a read** is xfx's encoding of the same facts (the tool, its decoded input field by field, the resolved target) where fx keys the raw arguments JSON (`permissionStateKeyForCall`, `tool_admission.zig:2189-2232`); a store is not shared between the products either way (the `/permissions` row).
- **A call to `ask_user_question` anyway, under a rule that covers it** (`*: deny`): the tool is left out of the request as upstream's is, but xfx has never judged that tool against the table (W8's rules decide reads, changes and commands), where upstream refuses it at admission -- it has a permission contract and its target is its own name (`toolHasPermissionContract`, `tool_admission.zig:101-103`; `.none`, `permissions.zig:226`). Found here, not changed by this unit.
- **`terminal` is hidden by `shell: deny`, not by `bash: deny`.** That is fx's behaviour for its `shell` (`permissionNameForTool` maps `shell` to no category, `permissions.zig:1455-1464`), ported as it is: the request carries what fx's would for the same settings file. A call to the terminal is still judged as `bash`/`run_command`, so `bash: deny` refuses every command it would run.

## What the captures do not show

Pinned by tests rather than screens: fx's three `rulesDenyAllTargetsForPermission` cases and the ask/allow ways back through a global deny (`src/permission/rules.rs`); the offered list per mode and table, and `terminal` as `shell` (`src/tools/mod.rs`); the list kept for a whole turn and taken afresh by the next, and full access offering everything (`src/agent/machine.rs`); a read's verdict in every mode, the exact grant, the question's words and the key (`src/permission/policy.rs`); the tools end to end -- ask in ask and auto, free reads in every mode, full access past a deny, No, deny over ask and grant, the exact always, nobody to ask, remembered allow and deny, a broken store, preparing a read, a hidden tool called anyway (`tests/permissions.rs`); the band and the hidden request on a real pseudoterminal (`tests/tui.rs`), and the line shell's `y/a/n` for a read (`tests/interactive.rs`).
