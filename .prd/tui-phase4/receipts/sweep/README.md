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

## Scoreboard (c1db919)

- fx: pin `c1db919` (fx 0.0.13), sha256 `7b76319a871163ada64d74539ebee592e0e216041cd28465127457500a9be969`, checked before and after the run
- xfx: `x𝒇x v0.1.0-1a94cdf [debug] · Run /help for commands`, sha256 `c2c431c164744903413aedaa446c9ea7bc44b9080030a99e1c69cc1068ddbe42`
- size 40x120, `--jobs 10`, 161 scenarios, wall 11m07s, load 20.0 at the start and 36.3 at the peak (1-minute average, sampled every 5 s)
- ledger sha256 `49b77a56856d65520b5178841733ebc02cead1b226e3f6bb23f6ecf88fd992be`: 13 normalizations; 11 divergences approved for this pin

fx's inventory (c1db919): 43 surfaces -- 36 commands, 7 key hints. Covered by a scenario: 38. **Uncovered: 5** -- `/login`, `/logout`, `/shell`, `/mcp`, `/feedback`.
Tokens typed as commands that are no command of this inventory (aliases, partial names, refusals): `/al`, `/balance`, `/cost`, `/de`, `/ex`, `/exit`, `/img`, `/mo`, `/nosuch`, `/notacommand`, `/q`, `/re`, `/setup`, `/vers`.

Scenarios: 161 -- ran 161, failed 0, timeout 0. **pass = 624 / 861 = 72.5%**

| total | ran | identical | normalized-identical | approved-divergence | replaced-in-next-pin | unresolved-defect | failed | timeout |
|---|---|---|---|---|---|---|---|---|
| 861 | 861 | 17 | 607 | 66 | 2 | 169 | 0 | 0 |

The unit is the snap pair: one `snap` or `snapfull` label of a scenario, read from its .cap file, so a scenario that failed or timed out counts every snap it would have taken. pass = (identical + normalized-identical) / total. Groups are the leading letters of a scenario's name (`model-anchor`, `model2` and `model` are `model`). A scenario counts under every surface it exercises, so the surface rows overlap.

| group | scenarios | total | ran | identical | normalized-identical | approved-divergence | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|
| alias | 1 | 11 | 11 | 0 | 9 | 0 | 0 | 2 | 0 | 0 | 81.8% |
| allowlist | 5 | 25 | 25 | 0 | 23 | 0 | 0 | 2 | 0 | 0 | 92.0% |
| answered | 1 | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| approvals | 3 | 18 | 18 | 0 | 2 | 0 | 0 | 16 | 0 | 0 | 11.1% |
| card | 1 | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| cmd | 1 | 4 | 4 | 0 | 1 | 0 | 0 | 3 | 0 | 0 | 25.0% |
| compact | 7 | 25 | 25 | 1 | 24 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| composer | 2 | 22 | 22 | 0 | 19 | 0 | 0 | 3 | 0 | 0 | 86.4% |
| copy | 1 | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| credits | 5 | 10 | 10 | 0 | 10 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| fast | 6 | 22 | 22 | 0 | 22 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| filepicker | 3 | 20 | 20 | 0 | 20 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| fulltranscript | 2 | 19 | 19 | 0 | 7 | 0 | 0 | 12 | 0 | 0 | 36.8% |
| help | 1 | 19 | 19 | 0 | 9 | 0 | 0 | 10 | 0 | 0 | 47.4% |
| image | 9 | 52 | 52 | 3 | 47 | 0 | 0 | 2 | 0 | 0 | 96.2% |
| interrupt | 12 | 61 | 61 | 0 | 36 | 2 | 0 | 23 | 0 | 0 | 59.0% |
| links | 2 | 9 | 9 | 1 | 7 | 0 | 0 | 1 | 0 | 0 | 88.9% |
| model | 14 | 69 | 69 | 0 | 67 | 0 | 0 | 2 | 0 | 0 | 97.1% |
| notice | 3 | 8 | 8 | 0 | 8 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| notices | 1 | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| paste | 3 | 17 | 17 | 1 | 16 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| permissions | 3 | 20 | 20 | 0 | 19 | 0 | 0 | 1 | 0 | 0 | 95.0% |
| popup | 1 | 4 | 4 | 0 | 3 | 0 | 0 | 1 | 0 | 0 | 75.0% |
| prose | 1 | 2 | 2 | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| provider | 6 | 41 | 41 | 0 | 9 | 20 | 0 | 12 | 0 | 0 | 22.0% |
| question | 3 | 26 | 26 | 0 | 26 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| quit | 2 | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| read | 1 | 6 | 6 | 0 | 5 | 0 | 0 | 1 | 0 | 0 | 83.3% |
| recall | 1 | 3 | 3 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% |
| rename | 2 | 8 | 8 | 2 | 5 | 0 | 0 | 1 | 0 | 0 | 87.5% |
| reset | 2 | 12 | 12 | 0 | 12 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| resume | 6 | 49 | 49 | 7 | 25 | 0 | 0 | 17 | 0 | 0 | 65.3% |
| search | 1 | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| settings | 12 | 72 | 72 | 1 | 26 | 31 | 0 | 14 | 0 | 0 | 37.5% |
| skills | 4 | 44 | 44 | 0 | 23 | 6 | 0 | 15 | 0 | 0 | 52.3% |
| slash | 2 | 15 | 15 | 0 | 9 | 0 | 0 | 6 | 0 | 0 | 60.0% |
| sound | 2 | 13 | 13 | 0 | 4 | 6 | 0 | 3 | 0 | 0 | 30.8% |
| startup | 1 | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| status | 1 | 8 | 8 | 0 | 0 | 0 | 2 | 6 | 0 | 0 | 0.0% |
| statusline | 2 | 11 | 11 | 0 | 11 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| stream | 1 | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| streaming | 1 | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| table | 1 | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| timeout | 1 | 2 | 2 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0.0% |
| titles | 6 | 15 | 15 | 0 | 12 | 1 | 0 | 2 | 0 | 0 | 80.0% |
| toolgroup | 3 | 17 | 17 | 0 | 8 | 0 | 0 | 9 | 0 | 0 | 47.1% |
| torn | 1 | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| trace | 2 | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| undo | 3 | 7 | 7 | 0 | 7 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| usage | 4 | 30 | 30 | 0 | 30 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| workspace | 2 | 13 | 13 | 0 | 13 | 0 | 0 | 0 | 0 | 0 | 100.0% |

Most unresolved snaps (scenario, unresolved of total, its first unresolved row):

- `skills` 12/18 -- path row 8: (blank) / `┃` (aligned: 1/1 rows unmatched; under approved row 7, skills-roots)
- `resume-tools` 11/18 -- cmd-ask row 27: `1–3 choose now    ↑↓ options    tab amend    ente…` / (blank) (aligned: 3/3 rows unmatched; under approved row 26, approval-grant-scope)
- `approvals` 10/10 -- edit-ask row 23: `  1–3 choose now    ↑↓ options    tab amend    en…` / (blank) (aligned: 3/3 rows unmatched; under approved row 22, approval-grant-scope)
- `help` 10/19 -- open row 5: `Commands 36  [All]  General  Session  Account  Mo…` / `Commands 32  [All]  General  Session  Account  Mo…` (aligned: 4/4 rows unmatched)
- `fulltranscript` 9/15 -- cmd-ask row 24: `1–3 choose now    ↑↓ options    tab amend    ente…` / (blank) (aligned: 3/3 rows unmatched; under approved row 23, approval-grant-scope)
- `interrupt2` 9/9 -- mid row 20: `  line 15 of the streamed answer` / `~ Generating (<s>) (↑4 ↓136)` (aligned: 2/2 rows unmatched)
- `toolgroup` 9/13 -- cmd-ask row 27: `1–3 choose now    ↑↓ options    tab amend    ente…` / (blank) (aligned: 3/3 rows unmatched; under approved row 26, approval-grant-scope)
- `settings2` 6/12 -- down-sound row 19: `  Ultra mode               off` / `  Ultra mode               off` (aligned: 2/2 rows unmatched)
- `status-version-stats` 6/8 -- stats row 18: `* stats: ansi_bytes=6142, redraws=0, debounced_re…` / `* stats: ansi_bytes=4770, redraws=0, debounced_re…` (aligned: 4/4 rows unmatched; under approved row 14, status-notice)
- `interrupt-steer` 5/5 -- steer-stream row 5: `  MARKER-HEAD word0 word1 word2 word3 word4 word5…` / `  MARKER-HEAD word0 word1 word2 word3 word4 word5…` (aligned: 2/2 rows unmatched)

## How to read it

- **The unit is the snap pair**, counted from the .cap files (861: the 863
  snap lines less the two labels `compact-inturn-fx` and `-xfx` share). A
  scenario that fails or times out counts all of its snaps as `failed` /
  `timeout`, so `total` never shrinks; here all 161 ran. **pass = (identical + normalized-identical) / total.**
- **identical** is equal cell for cell with nothing rewritten: 17, the snaps
  whose banner had scrolled away. **normalized-identical** is equal after the
  ledger's normalizations, which are the banner, the product name and its
  paths (`fx`→`xfx`, `.fx`→`.xfx`, the extra column taken from the padding
  beside it when that is what makes the row equal), the product's version
  and build, and what each run owns: capture.sh's run directory and per-binary
  TMPDIR, a turn's wall time, the activity row's pulse and seconds, the full
  transcript's timestamps, tmux's dead-pane clock, session ids, `/trace`'s
  file name, a network record's milliseconds. Each cites the receipt or
  ledger line that recorded it.
- **approved-divergence / replaced-in-next-pin** need every differing row
  covered by a ledger entry for this pin. Eleven are seeded, each from
  `ssot.md` Tensions or `docs/parity.md` (none from §"Deliberate xfx
  divergences this workstream reverses": those are divergences the phase
  removes). Only `/status` is `replaced-in-next-pin` (v0.0.16 moves it to the
  info panel; `/permissions` bare and `/usage` are equal already).
- **Comparison is by position.** A row one side has and the other has not --
  an approved one included -- moves every row under it, and those rows stay
  unresolved: the approval-scope rows (`2 = allow every future …`, two extra
  rows) push the approval footer in `approvals`, `resume-tools`, `toolgroup`
  and `fulltranscript`; the `/skills path` line fx has pushes the rest of the
  `skills` transcript. Each diff says how many rows differ once the two sides
  are aligned as sequences (`aligned: 1/1 rows unmatched`) and which approved
  row a cascade sits under.
- **Menus.** `/help`, the slash popup and `popup-full` read `Commands 36`
  against `Commands 32`: three of the four missing rows are approved
  (`/login`, `/logout`, `/feedback`), **`/shell` is a real gap** (the
  `/shell reload` unit is still in flight), and the count and every row after
  the first missing one differ, so those snaps are unresolved.
- **What else is in the unresolved bin:** timing (`interrupt2`,
  `interrupt-steer`, `streaming`-style snaps taken mid-stream, where how much
  had streamed or whether the turn had ended differs between the runs);
  `/stats`' `ansi_bytes=` (each product's byte count; no ledger record says
  it may differ); and product differences no record approves -- `/model`'s
  help entry is the line shell's summary (`show the model, or /model <id>…`,
  `src/interactive.rs:277`), a prompt typed during an approval shows as
  `[you] …` with a divider instead of fx's `┃ …`, `/provider`'s Return on the
  provider in force (`Already using` against fx's credential switch),
  `resume-paging`'s session age (`now` against `1m`), `titles-idle`'s footer
  title, `links`' full-transcript records.
- **Not compared:** `title`, `cursor`, `bells`, `clip`, `home`, `workls`,
  `gwlog`, `wire` and `reports` artifacts -- only snaps are.

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
