# R1 receipt: the three permission-rule divergences W8 left

Four scenarios, each run at 40x120 on both binaries with the repo's own driver:

```
TMPDIR=<tmp> FX_BIN=<fx c1db919 build, 0.0.13> scripts/fxcap/capture.sh fx  <scenario> 40x120 <dir>
TMPDIR=<tmp>                                    scripts/fxcap/capture.sh xfx <scenario> 40x120 <dir>
```

for `rules-read`, `rules-read-auto`, `rules-hidden` and `rules-hidden-all`
(`scripts/fxcap/scenarios/`, new in this unit, with the workspace
`rules-ws/` and the profiles `rules-read-settings.json`,
`rules-hidden-settings.json`, `rules-hidden-all-settings.json`). Each binary
reads the same profile shape: `{"permission": …}` at the top of its own
`settings.json`, the user table.

## The binaries

**xfx** is `target/debug/xfx` built at `494f9a8d`, banner `0.1.0-494f9a8
[debug]`: this branch after round 2, with `plan/tui-phase4` merged up to
`bcda7a5d` (M1's MCP client, S1's skills, X1). Every xfx snap here was taken
from it on 2026-11-03. The commit that carries this receipt differs from
`494f9a8d` in documentation only.

**fx** is the c1db9191 reference, sha256 `7b76319a871163ad…`, `--version`
0.0.13, banner `𝒇x v0.0.13`, run through the guarded driver, which launches it
with `FX_AUTO_UPGRADE=0` (plan `d91a9968`; fx installs a newer release over
itself otherwise, `src/main.zig:727-733`). The `rules-read` and
`rules-read-auto` fx snaps were taken again on 2026-11-03 beside the xfx ones.
They match the October fx snaps byte for byte under the comparison below, but
for one timing row (the grep turn's `7s` then, `6s` now). The `rules-hidden`
and `rules-hidden-all` fx snaps are October's; the same reference reproduced
them byte for byte, and their bodies but for `date_utc`. (One probe made on
2026-11-03 through a reference fx had silently upgraded to v0.0.16 was thrown
away.)

`before/` holds the same `rules-read` snaps from `target/debug/xfx` at
`0a1bb45` (`plan/tui-phase4` when this unit started), the state W8 left.

Sanitized: every run directory (`/private/tmp/claude-<uid>/fxcap.XXXXXX`) is
written `<scratch>`, in the screens and in the request bodies. No file names a
key, a home or a user. Request bodies are kept for what the receipt is about
and nothing else: every completion of the two hidden-tool scenarios, and the
completion after the refusal (`003`) of the two read scenarios.

## How the pairs were compared

Each `.txt` and `.ansi` pair, row by row, after two normalisations and nothing
else: row 1 (the welcome line) is dropped, and trailing blanks are trimmed.

## The decision table, before and after

fx lines are `vercel-labs/fx@c1db9191`, `src/core/tooling/tool_admission.zig`
unless another file is named. "Round 1" is `19927ce8`; "round 2" is this head.

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
| ask rule, answered `3` amended | `grep_files` | auto | **deny**: `execution-denied`, the sentence after the result, drawn as a prompt card | -- | same, and the same card (round 2) |
| ask rule, Escape | `read_file` | auto | turn stops, `■ Cancelled secret.txt` | -- | same |
| `/permissions remember allow read_file {…}` | `read_file` | auto | prepared, `$ read_file secret.txt` (`preparePermissionStateAction`, `:2257-2267`) | refused (`asks for no permission`) | prepared, the same band |
| remembered allow, ask rule | `read_file` | auto | **allow** before the ask (`:1405-1424`) | -- | **allow** before the ask |
| saved-session rules unavailable | any | ask, auto | **fails the call**: the copy of the in-memory rules taken before every decision (`requestPermissionOutcome`, `:1286-1291`; `snapshotPermissionState`, `session.zig:1816-1823`) | read unaffected | **deny**: xfx reads the file at every decision, and a file that does not read refuses |
| same | any | full access | the copy is still taken first (`:1286-1291` runs before `:1369`), then nothing in it is consulted | -- | reads nothing, consults nothing |
| saved rules present, the call's key cannot be made | any | ask, auto | saved rules skipped (`permissionStateKeyForCall(…) catch null`, `:1406`) | -- | **deny** (W8's, `src/permission/policy.rs`); stricter than fx |
| `*: deny`, or a deny on the name | `ask_user_question`, `capability_search`, `mcp_select_tool`, `mcp_features` | ask, auto | **deny** at admission (`toolHasPermissionContract`, `:101-103`; the name its one target, `permissions.zig:226`) | **allow**: never judged | round 1 the same; round 2 **deny**, nobody asked |
| an ask rule on the name | same | ask, auto | **ask**, the generic band, `ask_user_question` / `capability_search <query>` (`formatPermissionLabel`, `tool_presentation.zig:683-718`) | allow | round 2 **ask** |
| none decides | same | any | **allow** (`.requires_approval = false`, `:1123-1125`) | allow | allow |
| `skill: {"<skill>": "deny"}` | `skill` with a resource of it | ask, auto | **deny**, at the skill, every form of the call (`permissions.zig:181-185`; `tool_admission.zig:3840-3870`) | n/a (S1's tool) | S1 as merged judged at the resource: **allow**; round 2 **deny** at the skill |
| `mcp_<server>_*: deny` written mid-turn | a selected MCP tool | any, full access too | gone from the next request; a call to it is `unknown tool` (`snapshotToolDefinition`, `mcp_runtime.zig:2093`; `tool_dispatch.zig:845`) | n/a (M1's) | M1 as merged kept advertising it; round 2 the same as fx |
| an ask rule on an MCP tool | a selected MCP tool | auto | **ask**, before auto's hold | n/a | **ask** (refused, naming the rule, with nobody to ask) |
| `edit: {"*": "deny"}` | request `tools` | ask, auto | **hidden**: no `write_file`, `edit_file` (`appendBuiltinTool`, `tool_projection.zig:563-566`; `rulesDenyAllTargetsForTool`, `permissions.zig:1138-1166`) | offered | **hidden**: no `write_file`, `edit_file`, `create_folder` |
| `"permission": "deny"` | request `tools` | ask | **hidden** but `web_fetch` (never hidden, `permissions.zig:1139`) and `vision` (returns before any rule, `tool_projection.zig:562`) | offered | **hidden**: `[]`, `toolChoice` `auto` |
| a deny on an MCP meta-tool's name | request `tools` | ask, auto | **hidden** (`:563-566`) | n/a | **hidden**; and none of the three is offered where no MCP client runs |
| any deny | request `tools` | full access | offered: everything (`tool_projection.zig:563`) | everything | everything |
| `bash: deny` | `shell` / `terminal` | ask | offered (`shell` maps to no category, `permissions.zig:1455-1464`) | offered | offered; every command it runs **deny** |
| `shell: deny` | `shell` / `terminal` | ask | **hidden** | offered | **hidden**; a call anyway is judged as `bash`/`run_command`, so **ask** |
| rule changed between turns | request `tools` | any | the next job's projection (`app_agent_runtime.zig:1048-1051`) | n/a | the next turn's list |
| rule changed mid-turn | request `tools` | any | that job keeps its builtin projection (`orchestrator.zig:7269-7278`); its selected MCP tools are re-read every step | n/a | the same |
| `url` / `web-fetch-domain` | listing; any tool | any | listed the same with or without web tools (`session_commands.zig:987-1045`, `:1489-1545`); a `url` rule decides nothing for `web_fetch` (`permissions.zig:2562-2586`) | listed, decides nothing | unchanged: listed, decides nothing (no xfx tool reaches a URL) |

## Per snap

### `rules-read` (7 snaps; 4 identical, 3 differ by what is listed below)

`ask` mode, `read: {"secret*": "ask", …}`. Four turns read `secret.txt`.

| snap | what it shows | result |
|---|---|---|
| `ask` | the band: `Permission needed · Choose one` / `Tool`, `Would you like to allow this action?`, `Reason: This action needs approval before … can continue.` (`Reason:` in 245), `read_file secret.txt`, `❯ 1. Yes` / `2. Yes, and don't ask again for this request` / `3. No`, the hint at column 0 in 245; above it `● 1 tool call · 1 read` / `└ Reading secret.txt` | **differs** in two places, both on every band xfx draws: `fx` → `xfx` in the reason row, and xfx's dim `2 = allow every future read_file of \`secret.txt\` …` scope row under the choices (`scope_rows`, `src/tui/approval_view.rs`), two rows that leave two fewer blank rows under the hint. The scope row is recorded, below |
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
| `grep-no` | `└ Denied secret`, `1 denied`, and the amendment read back as fx writes it, the user's card `┃ look in notes.txt instead` in bold with a blank row under it (`write_user_prompt`, `app_worker_runtime.zig:1020-1026`), row for row and SGR for SGR; **differs** on the grep turn's summary only, `7s` / `6s` (how long the band was up, which the fx runs themselves disagree on) |
| `read-ask` | the read band in auto; as `rules-read`'s `ask`, plus the `7s` / `6s` above |
| `read-esc` | Escape: `● 1 tool call · 1 read · 1 cancelled` / `└ Cancelled secret.txt` / `■ Cancelled secret.txt · What can … do differently?`; differs by `fx` → `xfx` (F4's row) and `7s` / `6s` |
| `remember-prompt` | `Remember allow for this saved session?`, `$ read_file secret.txt`, `❯ 1. Confirm` / `2. Cancel`: the band's rows identical; differs above it only as `read-esc` |
| `remember-done` | `* permissions: saved-session permission rule updated`; as above |
| `remembered` | the same read, no question, `└ Read secret.txt`; as above |
| `listed` | `saved-session permission rules (1):` / `1 allow read_file secret.txt` in both; as above, and in the `.ansi` only the row's trailing colour: fx leaves the blank cells after `secret.txt` in colour 250, xfx ends the row with `\e[0m\e[K`. No glyph differs (renderer-wide, the t4 class) |

The wire: the amendment is a user message after the `execution-denied` result on both (`003-completion.json`).

### `rules-hidden` (2 snaps; 2 identical) and its bodies

`ask`, `edit: {"*": "deny"}` in the profile; a turn; `/allowlist add tool write_file` (a local table, which shadows the profile's whole); a second turn.

Request `tools`, by name:

| request | fx | xfx |
|---|---|---|
| first turn | `read_file glob_files grep_files shell subagent capability_search skill install_skill mcp_select_tool mcp_features ask_user_question web_fetch exa_search read_tool_result vision` -- no `edit_file`, `write_file` | `list_files glob_files grep_files read_file terminal capability_search skill mcp_select_tool mcp_features ask_user_question` -- no `write_file`, `edit_file`, `create_folder` |
| second turn | `edit_file`, `write_file` back | `write_file`, `edit_file`, `create_folder` back |

`toolChoice` is `{"type":"auto"}` in all four.

### `rules-hidden-all` (1 snap; identical) and its body

`"permission": "deny"`: fx's `tools` is `web_fetch` and `vision` -- `rulesDenyAllTargetsForTool` never hides `web_fetch` (`permissions.zig:1139`), and `appendBuiltinTool` returns before any rule for `vision` (`tool_projection.zig:562`; where the request then gets it was not traced, since xfx has no such tool) -- and xfx, which has neither, sends `"tools":[]` with `"toolChoice":{"type":"auto"}` (fx sends `toolChoice` over any list, `vercel_protocol.zig:363-368`).

## Not matched, and why

- **"Always" for a read.** fx grants `edit`, `read`, `glob` and `grep` on the whole workspace tree (`suggestedSessionGrants`, `permissions.zig:1202-1225`, the path grants at `:1217-1224`): answering `2` about one read authorizes every later write in the workspace for the session. xfx keeps the grant it keeps for every question -- this tool at this resolved file (a search: this tool below this directory) -- so a grant never authorizes more than the question showed. Narrower, never wider.
- **The scope row** (`2 = …`, `ask`, `ask2`, `grep-*`, `read-ask`) is recorded: it was added with xfx's approval layout (`4fce8e8b`, F2, 2026-10-07; `scope_rows`, `src/tui/approval_view.rs`), and `.prd/tui-phase4/ssot.md` Tensions keeps the reason (`5e91eacf`): "**Approval scope wording.** fx's choice 2 is "workspace file access"; xfx's grant is this tool on this file and outlives the process into a resumed session, so choice 2 says that and a dim `2 = …` row states the scope whole (the readiness gate requires the scope disclosed)."
- **The refusal's reason on the wire** is xfx's sentence, fx's is a JSON envelope; the same for every refusal xfx makes. **`xfx` for `fx`** is the product's name.
- **Where an amendment's card lands in a step of several calls.** fx writes it as soon as that call's own result is in (`appendPermissionFeedbackAfterToolResult`, `orchestrator.zig:5360-5380`, from `:11379-11387`), before the step's later calls have run; xfx writes it when the runtime reports the sentence delivered, after the step's last result, so the transcript never shows a sentence a later cancellation kept from the model. In a step of one call -- every capture here -- the place is the same.
- **The saved-session rules.** fx keeps them in memory and copies them before every decision; xfx reads `permissions.json` at every decision, so where fx fails a call on a copy it cannot take, xfx fails it on a file that does not read. Where fx cannot make a call's key it skips the saved rules (`:1406`); xfx refuses the call (W8's). In ask and auto each difference only refuses more; in full access neither consults a saved rule.
- **A saved-session key** is xfx's encoding of the same facts (the tool, its decoded input field by field or its whole arguments, the resolved target) where fx keys the raw arguments JSON (`permissionStateKeyForCall`, `tool_admission.zig:2189-2232`); a store is not shared between the products either way (the `/permissions` row).
- **A `skill` call names more candidates than fx's.** fx judges at the skill the location resolves to, else the call's `name` (`permissions.zig:181-185`); xfx judges at the catalog's name for the location, the `name` the call gave and the location's last component, a deny at any refusing -- so a call that names a denied skill some other way is still refused. Only ever refuses more.
- **`terminal` is hidden by `shell: deny`, not by `bash: deny`.** That is fx's behaviour for its `shell` (`permissionNameForTool` maps `shell` to no category, `permissions.zig:1455-1464`), ported as it is. A call to the terminal is still judged as `bash`/`run_command`, so `bash: deny` refuses every command it would run.

## Found here, not changed

- **The band for an MCP tool.** fx asks about a dynamic MCP call in words of its own -- `Allow this MCP tool call?` and `Reason: This MCP tool needs approval before fx can send the request.` (`approvalQuestion` / `approvalReasonLine`, `src/ui/footer/approval_ui.zig:1864`, `:1901-1907`) -- where xfx's MCP band (M1's `Kind::Other`) says `Would you like to allow this action?` with no reason row. M1's surface; no fx capture of it exists yet.

## What the captures do not show

Pinned by tests rather than screens: fx's three `rulesDenyAllTargetsForPermission` cases and the ask/allow ways back through a global deny (`src/permission/rules.rs`); the offered list per mode, table and MCP client, and `terminal` as `shell` (`src/tools/mod.rs`); every spec judged by its executor or by its name, and the by-name tools refused, asked and free in each mode, a skill denied at every form of its call (`src/tools/mod.rs`); a selected MCP tool dropped mid-turn in every mode, an ask rule on one in auto, the meta-tools under `*: deny` (`src/mcp/tests.rs`); the list kept for a whole turn and taken afresh by the next (`src/agent/machine.rs`); a read's and a by-name tool's verdict in every mode, the keys (`src/permission/policy.rs`); the tools end to end -- ask in ask and auto, free reads, full access past a deny, No, deny over ask and grant, the exact always, nobody to ask, remembered allow and deny, a broken store, preparing a read and a question, a hidden tool called anyway, a question a rule covers (`tests/permissions.rs`); the amendment card live, resumed and in C-o, under an open group, a row per line (`src/tui/shell.rs`); the band and the hidden request on a real pseudoterminal (`tests/tui.rs`), and the line shell's `y/a/n` for a read (`tests/interactive.rs`).
