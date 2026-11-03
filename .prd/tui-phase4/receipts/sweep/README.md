# Parity sweep receipt: fx `c1db919` against xfx

The first full run of `scripts/fxcap/sweep.sh` (SWEEP, epic 0 of
`.prd/07-fx-parity-program.md`): every scenario in `scripts/fxcap/scenarios/`
on fx and on xfx at 40x120, every snap pair compared cell by cell, classified
against `scripts/fxcap/parity-ledger.toml`. Taken 2026-11-03.

```
scripts/fxcap/sweep.sh --fx <c1db919 fx> \
  --pin-sha 7b76319a871163ada64d74539ebee592e0e216041cd28465127457500a9be969 \
  --jobs 10 --out <out>                 # with TMPDIR=<a short scratch dir>
scripts/fxcap/sweep.sh --rescore --out <out>     # the ledger below, same captures
scripts/fxcap/sweep.sh --fx <v0.0.16 fx> \
  --pin-sha 8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2 \
  --inventory-only --out <out2>
```

- **fx:** pin `c1db919`, the local zig 0.16.0 build of vercel-labs/fx
  `c1db9191` (0.0.13), sha256 checked before the first run and after the last;
  each capture.sh run also guards it (`FX_AUTO_UPGRADE=0`, the sha256 upgrade
  guard, the sound guard).
- **xfx:** `target/debug/xfx` built with `cargo build` at `1a94cdff` (the
  plan's head this branch started from; its banner reads `v0.1.0-1a94cdf
  [debug]`). The branch was merged with `plan/tui-phase4` afterwards; the
  scoreboard is `1a94cdff`'s.
- **v0.0.16:** pin `v0.0.16`, sha256 `8e6cda2e…c6a2`, the `fx` entry of the
  release asset `fx-macos-aarch64.tar.gz` (sha256 `2158a8d4…90d6`, as its
  `.sha256` says); the entry was hashed out of the tarball and matches the
  binary. Inventory only: no scenario ran on it.

The captures, diffs and full scoreboard stay on the machine that took them
(`<out>`: `scoreboard.md` has the per-scenario and per-surface tables); this
file is the redacted summary the sweep writes, and the coverage of both pins.

**Round 2** (same day, same captures, `--rescore`): the ledger re-filed under
the program's four reasons (`.prd/07-fx-parity-program.md:23`), the cascade
column, and a cause for every unresolved snap. What moved is under "Round 2"
below; the scoreboard here is round 2's.

## Scoreboard (c1db919, round 2)

- fx: pin `c1db919` (fx 0.0.13), sha256 `7b76319a871163ada64d74539ebee592e0e216041cd28465127457500a9be969`, checked before and after the run
- xfx: `x𝒇x v0.1.0-1a94cdf [debug] · Run /help for commands`, sha256 `c2c431c164744903413aedaa446c9ea7bc44b9080030a99e1c69cc1068ddbe42`
- size 40x120, `--jobs 10`, 161 scenarios, wall 11m07s, load 20.0 at the start and 36.3 at the peak (1-minute average, sampled every 5 s)
- ledger sha256 `fb79c23f2a404a810e16301aec31be07ae275181b55f7fb1a4d77b071a57f0ae`: 13 normalizations; 5 divergences approved for this pin

fx's inventory (c1db919): 43 surfaces -- 36 commands, 7 key hints. Covered by a scenario: 38. **Uncovered: 5** -- `/login`, `/logout`, `/shell`, `/mcp`, `/feedback`.
Tokens typed as commands that are no command of this inventory (aliases, partial names, refusals): `/al`, `/balance`, `/cost`, `/de`, `/ex`, `/exit`, `/img`, `/mo`, `/nosuch`, `/notacommand`, `/q`, `/re`, `/setup`, `/vers`.

Scenarios: 161 -- ran 161, failed 0, timeout 0. **pass = 624 / 861 = 72.5%**

| total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout |
|---|---|---|---|---|---|---|---|---|---|
| 861 | 861 | 17 | 607 | 22 | 15 | 2 | 198 | 0 | 0 |

The unit is the snap pair: one `snap` or `snapfull` label of a scenario, read from its .cap file, so a scenario that failed or timed out counts every snap it would have taken. pass = (identical + normalized-identical) / total. approved-divergence-cascade is a pair that differs by position beyond its approved rows (a row they pushed down) and whose every row left without a match, the two sides aligned as sequences, is approved; the positional diff stays in the raw output. Groups are the leading letters of a scenario's name (`model-anchor`, `model2` and `model` are `model`). A scenario counts under every surface it exercises, so the surface rows overlap.

| group | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| alias | 1 | 11 | 11 | 0 | 9 | 0 | 0 | 0 | 2 | 0 | 0 | 81.8% |
| allowlist | 5 | 25 | 25 | 0 | 23 | 0 | 0 | 0 | 2 | 0 | 0 | 92.0% |
| answered | 1 | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| approvals | 3 | 18 | 18 | 0 | 2 | 0 | 0 | 0 | 16 | 0 | 0 | 11.1% |
| card | 1 | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| cmd | 1 | 4 | 4 | 0 | 1 | 0 | 0 | 0 | 3 | 0 | 0 | 25.0% |
| compact | 7 | 25 | 25 | 1 | 24 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| composer | 2 | 22 | 22 | 0 | 19 | 0 | 0 | 0 | 3 | 0 | 0 | 86.4% |
| copy | 1 | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| credits | 5 | 10 | 10 | 0 | 10 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| fast | 6 | 22 | 22 | 0 | 22 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| filepicker | 3 | 20 | 20 | 0 | 20 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| fulltranscript | 2 | 19 | 19 | 0 | 7 | 0 | 0 | 0 | 12 | 0 | 0 | 36.8% |
| help | 1 | 19 | 19 | 0 | 9 | 0 | 0 | 0 | 10 | 0 | 0 | 47.4% |
| image | 9 | 52 | 52 | 3 | 47 | 0 | 0 | 0 | 2 | 0 | 0 | 96.2% |
| interrupt | 12 | 61 | 61 | 0 | 36 | 0 | 0 | 0 | 25 | 0 | 0 | 59.0% |
| links | 2 | 9 | 9 | 1 | 7 | 0 | 0 | 0 | 1 | 0 | 0 | 88.9% |
| model | 14 | 69 | 69 | 0 | 67 | 0 | 0 | 0 | 2 | 0 | 0 | 97.1% |
| notice | 3 | 8 | 8 | 0 | 8 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| notices | 1 | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| paste | 3 | 17 | 17 | 1 | 16 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| permissions | 3 | 20 | 20 | 0 | 19 | 0 | 0 | 0 | 1 | 0 | 0 | 95.0% |
| popup | 1 | 4 | 4 | 0 | 3 | 0 | 0 | 0 | 1 | 0 | 0 | 75.0% |
| prose | 1 | 2 | 2 | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| provider | 6 | 41 | 41 | 0 | 9 | 16 | 3 | 0 | 13 | 0 | 0 | 22.0% |
| question | 3 | 26 | 26 | 0 | 26 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| quit | 2 | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| read | 1 | 6 | 6 | 0 | 5 | 0 | 0 | 0 | 1 | 0 | 0 | 83.3% |
| recall | 1 | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% |
| rename | 2 | 8 | 8 | 2 | 5 | 0 | 0 | 0 | 1 | 0 | 0 | 87.5% |
| reset | 2 | 12 | 12 | 0 | 12 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| resume | 6 | 49 | 49 | 7 | 25 | 0 | 0 | 0 | 17 | 0 | 0 | 65.3% |
| search | 1 | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| settings | 12 | 72 | 72 | 1 | 26 | 0 | 0 | 0 | 45 | 0 | 0 | 37.5% |
| skills | 4 | 44 | 44 | 0 | 23 | 6 | 12 | 0 | 3 | 0 | 0 | 52.3% |
| slash | 2 | 15 | 15 | 0 | 9 | 0 | 0 | 0 | 6 | 0 | 0 | 60.0% |
| sound | 2 | 13 | 13 | 0 | 4 | 0 | 0 | 0 | 9 | 0 | 0 | 30.8% |
| startup | 1 | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| status | 1 | 8 | 8 | 0 | 0 | 0 | 0 | 2 | 6 | 0 | 0 | 0.0% |
| statusline | 2 | 11 | 11 | 0 | 11 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| stream | 1 | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| streaming | 1 | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| table | 1 | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| timeout | 1 | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0.0% |
| titles | 6 | 15 | 15 | 0 | 12 | 0 | 0 | 0 | 3 | 0 | 0 | 80.0% |
| toolgroup | 3 | 17 | 17 | 0 | 8 | 0 | 0 | 0 | 9 | 0 | 0 | 47.1% |
| torn | 1 | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| trace | 2 | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| undo | 3 | 7 | 7 | 0 | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| usage | 4 | 30 | 30 | 0 | 30 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| workspace | 2 | 13 | 13 | 0 | 13 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |

Causes of the unresolved snaps:

198 unresolved snaps; a snap counts under every cause of one of its rows (`only`: that cause alone).
Kinds: a = awaiting a user decision, b = a unit in flight, c = the capture (nondeterminism, or the scenario script), d = a defect.

| cause | kind | snaps | only | what | owner |
|---|---|---|---|---|---|
| sound-max | d | 45 | 38 | `fx's sound has a third level, 'max'; xfx's (the terminal bell) has none: Sound level is '…` | `unowned -- xfx src/sound.rs:37; src/tui/settings.rs:361 / fx src/core/config/settings_catalog.zig:280@c1db9191; src/core/app/app_commands.z…` |
| approval-grant-scope | a | 32 | 16 | `What an 'always' answer grants is a security choice: fx grants workspace file access for …` | `user decision approval grant scope (not yet on the program's decision list)` |
| menu-count | a+b | 23 | 6 | `The menus' count: fx 36, xfx 32 -- /login, /logout, /feedback awaiting decisions, /shell …` | `user decision Codex/Grok sign-in (#88) for /login and /logout; user decision /feedback destination; C8 feat/tui-phase4-shell-reload 76ede61…` |
| full-transcript-shift | d | 18 | 1 | `Every Ctrl-O row under fx's taller session record and its shell calls' argument rows stan…` | `unowned (follows session-record and tool-schema) -- xfx src/tui/shell.rs:32298; src/tui/approval_view.rs:685 / fx src/core/app/app_bootstra…` |
| stream-position | c | 18 | 17 | `The fake gateway streams a chunk every 'delayMs'; a snap or an Escape at a fixed second l…` | `scenario -- fix: the turns file delivers its chunks with no delay and holds the finish ('hangMs'), and a snap that shows streamed lines wai…` |
| menu-shift | a+b | 17 | 0 | `Command rows that only moved: a window without the four absent rows shows two others, and…` | `user decision Codex/Grok sign-in (#88); user decision /feedback destination; C8 feat/tui-phase4-shell-reload 76ede613` |
| tool-schema | d | 17 | 0 | `fx's command tool is 'shell' ('action: run', per-call 'timeout_ms', run in the user's she…` | `unowned (decision needed? C8 runs commands in the login shell but keeps the header) -- xfx src/tui/approval_view.rs:685 / fx src/core/execu…` |
| C7-help-entries | b | 16 | 3 | `xfx's /model summary is the line shell's; C7 r2 gives every registry row fx's help entry.` | `C7 r2 feat/tui-phase4-alias 8d83f961` |
| grant-scope-push | a | 14 | 8 | `The scope rows make xfx's approval two or more rows taller (a long command is repeated in…` | `user decision approval grant scope (not yet on the program's decision list)` |
| settings-downs | c | 10 | 3 | `The per-binary Down counts predate xfx's Fast and Ultra rows: xfx's Right moved another r…` | `scenario (fixed in this branch) -- fix: one Down count for both binaries (fx's): xfx's /settings has fx's 14 rows since the speed rows land…` |
| provider-label | d | 9 | 8 | `The picker names the Gateway 'gateway'; fx's row is 'vercel'.` | `unowned (P1's picker) -- xfx src/provider/mod.rs:116 / fx src/core/auth/provider_catalog.zig:19@c1db9191` |
| session-record | d | 9 | 0 | `Ctrl-O's '* session:' record: fx's has 'system prompt: ready · N bytes', 'skills: loading…` | `unowned (S1 item 2 owns the skill notices, not this record) -- xfx src/tui/shell.rs:32298 / fx src/core/app/app_bootstrap_runtime.zig:71@c1…` |
| stats-bytes | d | 9 | 9 | `'/stats' counts each product's own bytes to the terminal (fx 3499, xfx 2675 for the same …` | `unowned -- xfx src/tui/shell.rs:1436 / fx src/core/app/app_commands.zig:1014@c1db9191` |
| K-recover | b | 6 | 6 | `Stream-failure recovery: fx's '⚠ Provider unavailable · … retrying request in 1s' row, it…` | `K-recover fix/tui-stream-recovery f11a23d2` |
| R1-amendment-card | b | 5 | 2 | `An amended approval's words read back as '[you] …'; fx shows them as its prompt card '┃ ……` | `R1 r2 feat/tui-phase4-rules, merged into plan/tui-phase4 at b6392eae after the swept build` |
| session-age | c | 5 | 5 | `The picker's age column reads the run's clock; the snaps fall about a minute after the se…` | `scenario -- fix: seed the saved sessions with fixed timestamps (homefile), or take the picker snaps well clear of a minute after the first …` |
| C7-help-search | b | 4 | 3 | `/help's search: fx matches a query against each command's help entry (helpCatalogSpecMatc…` | `C7 r2 feat/tui-phase4-alias 8d83f961` |
| C8-shell | b | 3 | 0 | `fx's /shell row in /help and the popup; xfx has no /shell until C8 lands.` | `C8 /shell reload feat/tui-phase4-shell-reload 76ede613` |
| P1-carry-notice | b | 2 | 2 | `After a switch back to the Gateway xfx says once that the llmux turn's reasoning was not …` | `P1 r3/r4 feat/tui-phase4-provider-session 8facfb6d` |
| provider-credential-column | a | 2 | 2 | `'/provider vercel' opens fx's credential column (OAuth or API key, the gated sign-in) and…` | `user decision Codex/Grok sign-in (#88)` |
| vision-route | a | 2 | 2 | `For a model without native image input fx forces a 'vision' call answered by a second mod…` | `user decision second-model image route (not yet on the program's decision list)` |
| approval-scroll | d | 2 | 0 | `A long review scrolls with the mouse wheel in fx and Ctrl-P/Ctrl-N in xfx, which asks for…` | `unowned -- xfx src/tui/approval_view.rs:106 / fx src/ui/approval_screen.zig:564@c1db9191` |
| allowlist-band | d | 2 | 2 | `Once /allowlist's notices fill the screen fx's band stands three rows above the bottom an…` | `unowned (not traced to the line) -- xfx src/tui/layout.rs:86 / fx src/ui/render_engine/footer_layout.zig:39-60@c1db9191` |
| activity-row-gap | d | 2 | 2 | `With the whole answer delivered and the turn held open, fx leaves a blank row between the…` | `unowned (not traced past the band's top) -- xfx src/tui/layout.rs:86 / fx src/ui/render_engine/footer_layout.zig:50@c1db9191` |
| idle-connection | c | 2 | 2 | `fx's title call goes out on the connection it warmed at launch, which the fake reset afte…` | `fakegw -- fix: the fake gateway keeps an unused connection open for fx's 30 s pool TTL (fakegw.ts idleTimeout), not a scenario file` |
| help-login-args | a | 1 | 0 | `Enter on the Account tab's first row opens fx's /login picker; xfx's first row there is /…` | `user decision Codex/Grok sign-in (#88)` |
| catalog-window | c | 1 | 1 | `'modelsdown 6000' is wall-clock from the gateway's start: under load fx's /model asked af…` | `scenario + fakegw -- fix: an outage counted in catalog requests rather than milliseconds since the gateway started ('modelsdown', a fakegw/…` |

Most unresolved snaps (scenario, unresolved of total, its first unresolved row):

- `resume-tools` 11/18 -- cmd-ask row 19: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched)
- `approvals` 10/10 -- edit-ask row 19: `    2  Apply + allow workspace file access for th…` / `    2  Apply + allow future edits to this file fo…` (aligned: 3/3 rows unmatched)
- `help` 10/19 -- open row 5: `Commands 36  [All]  General  Session  Account  Mo…` / `Commands 32  [All]  General  Session  Account  Mo…` (aligned: 4/4 rows unmatched)
- `fulltranscript` 9/15 -- cmd-ask row 16: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched)
- `interrupt2` 9/9 -- mid row 20: `  line 15 of the streamed answer` / `~ Generating (<s>) (↑4 ↓136)` (aligned: 2/2 rows unmatched)
- `settings2` 9/12 -- tab3 row 7: `  Sound level    off  on  max` / `  Sound level    off  on` (aligned: 1/1 rows unmatched)
- `sound` 9/12 -- max row 13: `* sound: max` / `✗ usage: /sound [on\|off]` (aligned: 1/1 rows unmatched)
- `toolgroup` 9/13 -- cmd-ask row 19: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched)
- `settings-effort` 8/8 -- bare row 18: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched)
- `provider` 6/15 -- open row 5: `            vercel · current` / `            gateway · current` (aligned: 3/3 rows unmatched)

## How to read it

- **The unit is the snap pair**, counted from the .cap files (861: the 863
  snap lines less the two labels `compact-inturn-fx` and `-xfx` share). A
  scenario that fails or times out counts all of its snaps as `failed` /
  `timeout`, so `total` never shrinks; here all 161 ran. **pass =
  (identical + normalized-identical) / total.**
- **identical** is equal cell for cell with nothing rewritten: 17, the snaps
  whose banner had scrolled away. **normalized-identical** is equal after the
  ledger's normalizations: the banner, the product name and its paths
  (`fx`→`xfx`, `.fx`→`.xfx`, the extra column taken from the padding beside
  it when that is what makes the row equal), the product's version and
  build, and what each run owns: capture.sh's run directory and per-binary
  TMPDIR, a turn's wall time, the activity row's pulse and seconds, the full
  transcript's timestamps, tmux's dead-pane clock, session ids, `/trace`'s
  file name, a network record's milliseconds. Each cites the line that
  recorded it.
- **approved-divergence** / **replaced-in-next-pin**: every differing row,
  by position, has each side's text covered by a ledger entry for this pin.
  A regex speaks for its own side only, so an approved xfx row never vouches
  for the fx row beside it.
- **approved-divergence-cascade**: the pair differs by position beyond its
  approved rows, and with the two sides aligned as sequences every row left
  without a match is approved -- a blank row only as the separator of an
  approved row in the same unmatched run. A row pushed off the screen, or any
  other unmatched row, keeps the pair unresolved. The diff in `<out>` keeps
  both readings.
- **Not compared:** `title`, `cursor`, `bells`, `clip`, `home`, `workls`,
  `gwlog`, `wire` and `reports` artifacts -- only snaps are.

## Round 2

**The ledger, re-filed.** The eleven entries of round 1 were checked against
the program's reasons -- a user decision, an fx defect or behaviour that
cannot be copied safely, an interim decision for a surface the next pin
replaces, a real platform limit -- each now with an owner, a resolution and
an expiry, all expiring at the v0.0.16 pin. A user-decision entry needs the
decision recorded as the user's (program :55, ssot.md's G/X rows, open-loop
#88); a decision nobody has asked yet is a cause, not an approval. `ledger.py` refuses an entry
without them, the old `deliberate` class, an owner that is neither a unit ID
nor `user decision <topic>`, and an expiry that is not `pin:` or `date:`.

| round 1 | round 2 |
|---|---|
| `deferred-sign-in-and-feedback` | `sign-in` (/login, /logout) and `feedback`: user decisions (#88; /feedback destination) |
| `provider-set` | `sign-in-providers`: user decision #88 (Codex/Grok rows, the credential column, llmux as xfx's stand-in); the `gateway` label is a defect |
| `approval-grant-scope` | **unresolved** (`approval-grant-scope`, a): a security choice the repo records as xfx's, not yet asked |
| `skills-roots` | `skill-roots`: user decision "other agents' skill roots" (program :18, :55) |
| `status-notice` | replaced-in-next-pin, owner P-status |
| `sound-has-no-max` | **unresolved** (`sound-max`, d): no reason outside xfx |
| `terminal-tool`, `terminal-tool-deadline` | **unresolved** (`tool-schema`, d) |
| `session-record-lines` | **unresolved** (`session-record`, d) |
| `retry-budget` | **unresolved** (`K-recover`, b) |
| `image-vision-route` | **unresolved** (`vision-route`, a): likewise not yet asked |

**Causes.** `[[cause]]` entries in the ledger name, for each row that stays
unresolved, why and whose fix it is; they approve nothing. Every one of the
198 unresolved snaps is attributed (table above: a = awaiting a user
decision, b = a unit in flight, c = the capture, d = a defect with both
source lines). `menu-count` is the `Commands 36` / `Commands 32` row:
/login, /logout and /feedback await decisions, /shell is C8's. R1's
amendment card merged into the plan after the swept build (`1a94cdff`).

**Scenario fixes applied** (scenario files only), each retaken twice on both
binaries (xfx at plan `de6618de`), with the same verdicts both times:

- `settings-perm`, `settings2`: one Down count for both binaries. The
  per-binary counts predated xfx's Fast and Ultra rows, so xfx's Right moved
  another row. Now the permission notices and the footer match; what is left
  is `sound-max` alone.
- `interrupt` (`interrupt-words.json`): the streamed turns arrive whole and
  hold their finish (`hangMs`): `mid2` and `esc1` are equal.
- `interrupt2` (`interrupt-lines.json`, and its `mid` snap at 6 s so both
  pacers have released the lines): every snap but `mid` and `esc1` is equal;
  those two show a defect the timing hid, `activity-row-gap` -- fx keeps a
  blank row between the answer and the activity row, xfx does not.

Not applied: a content wait (`waitfor <text>`) is a capture.sh verb, not a
scenario file; `interrupt-steer`, `provider`, `rename-midturn` and
`resume-empty` time a keystroke against the stream on purpose; the
catalog outage (`modelsdown`) and the fake's 10 s idle reset
(`idle-connection`, the W9 finding) are fakegw's.

## Coverage

- fx's inventory (c1db919): 43 surfaces -- 36 commands, 7 key hints. Covered by a scenario: 38. **Uncovered: 5** -- `/login`, `/logout`, `/shell`, `/mcp`, `/feedback`.
- fx's inventory (v0.0.16): 45 surfaces -- 38 commands, 7 key hints. Covered by a scenario: 38. **Uncovered: 7** -- `/fork`, `/login`, `/logout`, `/effort`, `/shell`, `/mcp`, `/feedback`.

Both lines are over the 161 scenarios swept (`1a94cdff`'s). The merge with
`plan/tui-phase4` at `de6618de` brought 18 more (`h1-*`, `rules-*`; 179 in
all), not swept here; over those, `/mcp` is covered too (`h1-mcphttp`), so
c1db919 has 4 uncovered (`/login`, `/logout`, `/shell`, `/feedback`) and
v0.0.16 6 (those and `/fork`, `/effort`).

Covered means a scenario types the command (`/model`, `/model gro`) or, for a
key hint, opens the menu and sends that key; an alias or partial name is not
the command. `/mcp`'s scenarios live in `receipts/m1/scenarios/` and need
upstream's fixture, so the sweep does not run them. A surface v0.0.16 changed
(`/status`, `/usage`, `/mcp`) is covered by scenarios written against
`c1db919`, which say nothing yet about its new screen.

v0.0.16 against `c1db919`, from the two inventories (`inventory-*.json`
here): **added** `/fork` (Session) and `/effort` (Model); **removed** none
(`/cost`, an alias, no longer answers: it is an unmapped token now); **help
entry changed** `/status` (`show runtime configuration and the session ID`),
`/usage` (`show your provider's usage and spend`), `/mcp` (`manage MCP
servers and their logins`); the key hints are the same.

## Wall time and load

161 scenarios, 322 capture runs, 6416 s of captures (fx 3210 s, xfx 3206 s):
**11 min 07 s** at `--jobs 10`, no timeout and no failure. The machine (18
cores) was at a load of 20.0 when the sweep started and peaked at 36.3. The
longest scenario (`provider-carry`, both runs) takes 180 s, which bounds any
`--jobs`. 10 ran clean here; 8 (the default) leaves room for the machine's
other work at its usual load of 15-20. Rescoring the captures with an edited
ledger takes seconds.

## What a CI run would need

- **tmux** 3.x (the driver uses `-f /dev/null`, `pipe-pane`, the
  `alert-bell` hook, `capture-pane -e`), **bun** (the fake gateway,
  `fakegw.ts`), **python 3.11+** (`tomllib`), `shasum`, and the xfx build
  the run measures.
- **The c1db919 artifact.** fx `c1db9191` is not a release: the pin is a local
  zig 0.16.0 build for macOS aarch64, so CI needs that exact file (refused if
  its sha256 differs) from an artifact store, or a build step that reproduces
  it bit for bit. A build for another platform is another file, so another
  pin, with no divergence approved for it until the ledger names it. v0.0.16
  is a release asset and can be fetched and checked by its `.sha256`.
- **macOS.** capture.sh's guards assume it: the sound guard is fx's macOS
  chime path (`afplay` by absolute path), the clipboard shims stand in for
  `pbcopy` and `osascript`. A Linux runner needs those guards re-derived
  first.
- **Runner time:** 6416 s of captures, so about 6416 / jobs seconds plus the
  180 s floor: about 14 min at 8 jobs, 27 min at 4. The captures are
  timing-sensitive (`wait` lines), so a loaded runner adds unresolved snaps
  of the timing kind rather than failures.
- **The public log:** everything the sweep prints and the scoreboard files
  go through `scripts/fxcap/redact.py`, which writes no file with a local
  path, the home directory or the checkout's retired name left in it
  (`scripts/fxcap/test-sweep.sh` proves it). The captures and diffs in
  `<out>` are not redacted and are not for a public log.
