# S1 receipt: `/skills` and the `$` skill token

Two scenarios, each run at 40x120 on both binaries through the repo harness:

```
FX_BIN=<fx c1db919 build, 0.0.13> scripts/fxcap/capture.sh fx  <scenario> 40x120 <dir>
                                  scripts/fxcap/capture.sh xfx <scenario> 40x120 <dir>
```

with `TMPDIR=/private/tmp/claude-501`.

- `skills.cap` (turns `skills-turn.json`): `/skills` and its menu, Down,
  Escape, `list`, `show demo`, `show nope`, `path`, `create made`,
  `create ../escape`, `add ./skill-src`, the home directory after it,
  `remove made`, `remove demo`, `bogus`, then `$`, `dem`, Enter, ` run it`,
  Enter, and the request the turn sent. It is the research scenario
  (`research/media-ext/scenarios/skills.cap`) keystroke for keystroke.
- `skills-paste.cap`: a bracketed paste `use $rel please`, which opens the menu
  on `rel`, Enter on it, Ctrl-U, and a paste `no $zzz here`, which opens
  nothing (`maybeOpenPastedSkillMenu`, `input_paste_runtime.zig:634-663`).

The workspaces are the same skills under each product's root:
`scenarios/skills-ws-fx/` has `.fx/skills/demo`, `scenarios/skills-ws-xfx/` has
`.xfx/skills/demo`, and both have `skills/release-notes` and the local source
`skill-src/foo` (each scenario's `fx: work` / `xfx: work` line). fx is the
c1db919 build (banner `v0.0.13`). xfx is `target/debug/xfx` built from this
branch's tree on `plan/tui-phase4` `8891d04` (banner `v0.1.0-8891d04`).

Sanitized: each run directory's random suffix is written as `XXXXXX`
(`/private/tmp/claude-501/fxcap.XXXXXX`), the same length, so no row moves. No
home path, key or bearer appears in any file. The fake key
(`fake-key-not-real`) appears nowhere.

## Screens

Compared on the text and on the SGR stream, after mapping the product's own
words: the banner row, `fx` → `xfx` in the scope column, the tab and the two
`path` sentences, `.fx/skills` → `.xfx/skills`, the run directory, and the
seconds a turn took.

| Label | Text | SGR | What differs |
|---|---|---|---|
| `menu`, `menu-down`, `menu-esc`, `list` | same | same | -- |
| `show-demo`, `show-nope` | same but tabs | same but tabs | fx's tabs go on past `Workspace` with `Claude Codex Agents OpenCode Claw` (decided default 1) |
| `path` | one row fewer | same otherwise | fx's third line, `compatibility roots are auto-discovered from workspace and home (.opencode/.codex/.claude/.agents/.claw).`, is not said (decided default 1) |
| `create`, `create-bad`, `add-local`, `remove-made`, `remove-workspace`, `bogus` | same but the `path` row | same but the `path` row | that row, carried in the scrollback |
| `dollar`, `dollar-dem` | same but tabs and the `path` row | same, plus one reset | fx's tmux capture puts the marked row's reset at the start of the next row, xfx's at the end of its own row: the same cells |
| `dollar-enter` | same but the `path` row | same | -- |
| `sent` | same but the `path` row | same | `* 1 requested skill loaded` / `└ Loaded skill demo` with the branch under the glyph, and `(↑4 ↓40)` |
| `paste`, `paste-enter`, `paste-nomatch` | same | same | -- |

Two things in `sent` were wrong in the first xfx take and were fixed on this
branch, test first:

- `└ Loaded skill demo` stood two columns in. fx's notice puts a body row that
  opens with `├ ` or `└ ` under the glyph (`noticeContinuationIndent`,
  `transcript_blocks.zig:1718-1725`). xfx's notice now gives such a row its
  own gutter (`Gutter::Branch`). The test uses fx's own cases
  (`a_notice_rows_tree_branch_stands_under_its_glyph`).
- The summary said `↑3`. fx counts the prompt as sent, `$demo run it`
  (`TurnSummaryAccumulator.init` over `job.prompt`, `telemetry.zig:87-96`,
  `orchestrator.zig:5482-5485`), and xfx had counted the card's `demo run it`.

## Home directory after `add`

`*-after-add.files`: `skills/foo/SKILL.md` (90 bytes), `skills/made/SKILL.md`
(113 bytes) and `history.jsonl` are byte for byte the same on both, apart from
the timestamps. `.skill-install-locks/foo` is there on both, under `~/.fx` and
`~/.xfx`. fx also has a `sessions/<id>/` directory: fx opens its session at
launch, and xfx opens its session with the first prompt.

## Request bodies

`*-bodies/001-completion.json`, the request the `$demo run it` turn sent:

- **The catalog block** (`Skills provide task instructions. …
  <available_skills> … </available_skills>`): the same bytes on both, apart
  from the roots' paths and the namespace that hashes them. Recomputing
  SHA-256(`fx-skill-locations` ‖ LE-u64-length-prefixed name and path, per
  entry) over each file's own entries gives `1aef8ffe9ac369a7` for fx's and
  `7abb5ab5d814e8a4` for xfx's, the values each one sent.
- **The explicit block** (`Explicitly invoked skill content for this query:` …
  `<skill_content name="demo" location="<work>/.xfx/skills/demo"
  resource="SKILL.md" complete="true">` + the file + `</skill_content>`): the
  same bytes, apart from that path.
- **The user message**: `[{"type":"text","text":"$demo run it"}]` on both.
- **The `skill` tool**: the same object, description and schema.
- **Order**: on both, the catalog comes before the workspace's project-rules
  message and the explicit block after it. fx sends the catalog as its third
  system message, after its identity prompt and a web-search note, and the
  explicit block as its sixth. xfx sends no identity prompt, and this
  workspace has no project instructions, so xfx's two blocks are its only
  system messages.
- **The tools**: fx advertises `install_skill`, and xfx does not (decided
  default 2). The other differences in the tool list are the products' own
  tools and are outside this slice.

## Decided defaults (for review)

1. **xfx reads only its own roots.** These are `.xfx/skills` and `skills/` in
   the workspace and its ancestors, then `~/.xfx/skills`. fx also reads
   `.opencode/`, `.codex/`, `.claude/`, `.agents/` and `.claw/skills`
   (`src/builtins/skills.zig:16-38`). Reading those roots would put another
   agent's instructions into xfx's prompt.
2. **`add`/`install` take only local sources.** Any source that is not an
   existing directory gets fx's failed-clone sentence (`Failed to install.
   Check the source path or URL and try again.`), and nothing reaches the
   network. fx would run `git clone --depth 1` (`installFromGitHub`,
   `skills.zig:265-280`). `install_skill` is not advertised.
