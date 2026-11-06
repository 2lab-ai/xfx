# P5-EPIC2-INT2 receipt: epic 2 (MCP v2) on `main`

Epic 2 of the 0.0.16 delta, merged onto `main` @ `0e9e7ce8` (epic 1, H3,
FK-RULES, FK-GAP, PROC-SPAWN #60) as one tip, `git merge --no-edit`:

| merge | tip | holds |
|---|---|---|
| `8bf55a72` | `feat/p5-mcp-menu` @ `a342f056` | epic-2 part 1 (`08368feb`); MCP-C #44, MCP-W #45, MCP-T #46, MCP-S #47, MCP-C2 #48, MCP-V #50; MCP-M with Mb-Mf |

No behaviour is added. The merge base is `8464e981` (H1c). Of the 1,019
files the merge changes against `0e9e7ce8`, 1,010 are touched by one side
only, and each is byte-for-byte that side's tip (checked file by file). Of
the 15 files both sides touched, 6 come out as `main`'s (the spawn lock
below, and four files whose only MCP-side edit was that lock's first
version); the other 9 are below. Two commits follow the merge: the ledger's
citations (`a51bb94c`) and this receipt.

## Conflicts and their resolutions

### The spawn lock: `main`'s, wholesale

`src/process.rs`, `src/clipboard.rs`. The MCP side's only edits to either
since `8464e981` are the lock's first version, `6658b699` and `209d3ab5`;
`main` holds the same two (cherry-picked, `b3a8f6ef`, `7c3cf23a`) reworked by
PROC-SPAWNb/c. Both files are `main`'s (`git checkout --ours`).
`src/tui/signals.rs` and `clippy.toml` were touched by `main` only. The
lock's other first-version sites (`src/mcp/docker.rs`,
`src/permission/gitconfig.rs`, `src/tools/terminal.rs`,
`src/tui/file_index.rs`) merged clean and are byte-for-byte `main`'s.

### Starts and pipes on the MCP side

None to route. Clippy (`--all-targets`, both feature sets) on the merged
tree reports no `disallowed-methods` site, and
`process::tests::no_child_process_is_made_any_other_way` passes in both
suites. Every start the MCP side brings already goes through `process::*`:

| site | what it starts | through |
|---|---|---|
| `src/mcp/transport.rs:263` | a stdio MCP server (`StdioTransport::spawn`), so every `mcp::tests` case, `mcp::command::tests` and the new `transport` case that start one | `process::spawn` |
| `src/mcp/docker.rs:134` | the docker probe | `process::spawn` |
| `src/mcp/transport.rs:1576`, `:2300` | the test runner, `true` (tests) | `process::output`, `process::spawn` |
| `tests/mcp.rs` (`Session::spawn_without_taking_the_terminal`) | xfx on a pty | `tests/support/pty.rs:265`, `process::spawn` |

`src/mcp/runtime.rs:1290` is a thread (`std::thread::Builder::spawn`), not a
process. PROC-SPAWNb's MCP case
(`mcp::tests::a_call_the_server_answers_with_no_result_fails_as_fx_says_it_did`)
runs under the lock here for the first time outside a clone: ok in both
suites and 10 of 10 alone (0.19-0.23 s).

### Code

| file:line (result) | `main` wanted | MCP wanted | kept |
|---|---|---|---|
| `src/tui/shell.rs:10883` (the event loop) | the M1 menu swallowing every key, then the project trust prompt's branch, now also guarded by `fork_menu`/`effort_menu` (epic 1) | MCP-M: `offer_to_mcp(&event, held)`, the keys it binds, the rest to the composer; no trust prompt (fx `ba0ea4a9`, "Remove the MCP v1 client") | MCP's. The guards guarded a prompt MCP-M deleted, with every other use of `mcp_prompt` (the merge removed them cleanly); nothing is left for them to guard |
| `src/tui/shell.rs:12999-13002` (`mcp`, bare) | P-panel: `info_panel = None`; FK-GAPc: `close_fork_menu()` (every opener drops a waiting `/fork`) | MCP-M: `McpMenu::open(rows, notes, now)` | both: the two closes, then MCP-M's open. The `/mcp` menu is a slot surface: `slot_occupants` (`:41972`, `:41984`) counts `mcp_menu`, and FK-GAPc's `every_command_that_opens_a_surface_drops_a_fork_on_its_way` (bare `/mcp`) passes |
| `src/tui/shell.rs:14510-14516` (`recall`) | FK-GAPb: a recall drops a waiting `/fork` (`close_fork_menu`) | MCP-V: a recalled `/mcp VERB NAME` keeps its completions closed (`recalled_slash` also reads `mcp_args::query_of`) | both: MCP-V's `recalled_slash`, then the close |
| `src/tui/shell.rs:40741-40775` (the skills fixture) | the flake fix on `main` (`62d4173d`): `skilled_in(root)` draws the root again while its name holds a word a case searches for (`SEARCHED`); `skilled_at` goes through it | MCP-Me: `skilled_under(rows, cols, prefix)`, a root named `prefix…` | both, split three ways: `skilled_at` -> `skilled_in` (draws again) -> `skilled_over` (builds); `skilled_under` -> `skilled_over` on `root_named(prefix)`, **not** drawn again, because its `.tmpzZ` case is there to hold a root whose name holds `zZ` |
| `src/tui/shell.rs:40926-40983` (`a_space_ends_a_dollar_search_and_a_word_nothing_matches_shows_no_menu`) | `$zz` over `skilled()` and over a held `.tmpzz` root (drawn again) | `$z:` over `.tmp` and `.tmpzZ` roots, after asserting no skill's name, description, source or path holds `z:` | both: the helper `no_menu_for_a_word_nothing_matches` takes the word and names the root in its message; `main`'s two calls with `zz`, then MCP-Me's loop with `z:`. Its comment now says which fixture draws again |
| `scripts/fxcap/parity-ledger.toml:238-348` | P-status's six `[[divergence]]` entries (one for H1's scenario) and PERM-EARLY's one | MCP-Mc's `mcp-approval-wrap-led` | all eight, `main`'s first; no id repeats |

One hunk compiled on neither side as merged, and was fixed in the merge
commit:

- `src/tui/shell.rs:3536` `write_raw_rows`. MCP-M deleted it, with its last
  MCP callers (the trust prompt's rows, `v`'s entry) and the
  `Gutter::Shown`/`Gutter::Wrapped` gutter they used. FK2's `forked`
  (`:3508`) still writes its `original session xfx --resume <id>` line with
  it. It is back, the same body less the `strip_prefix("  ")` branch, so
  every line goes behind `Gutter::None`. That is what `main` wrote for the
  fork's line too: the line is `format!("original session xfx --resume
  {SessionId}")`, which never starts with two spaces, so `main`'s branch to
  `Gutter::Shown` never ran for it. The gutter is not restored.

### Merged clean, read

- `docs/parity.md`: no conflict. Every line either side changed since
  `8464e981` is in the result verbatim (checked line by line, both tips),
  and no row repeats. MCP-M's `/mcp` row says there is no trust prompt; no
  other row names one.
- `src/interactive.rs`, `src/tui/router.rs`, `src/tui/mod.rs`,
  `src/mcp/transport.rs`, `tests/permissions.rs`, `tests/tool_loop.rs`:
  different hunks; each side's tests pass on the result (below). MCP-W's
  removal of `mcp_features` (`867dea76`) from both test files and `main`'s
  edits there are in different lines.
- **Shared counts.** No count either side pins moved, so none was edited:
  `SLASH_COMMANDS.len() == 34` (`src/interactive.rs:2861`), the help
  menu's tallies and the tool list's pass as merged, and `check-no-stubs.sh`
  is ok.

### The ledger's citations (`a51bb94c`)

The merge moved `mcp-approval-wrap-led`'s `/mcp` row (`docs/parity.md:97`
-> `:99`: epic 1's `/fork` and `/effort` rows are above it) and left code
lines that one side moved (`toolgroup.rs:528` -> `:583`, `layout.rs:86` ->
`:91`). The other `docs/parity.md` citations had drifted on `main` since
epic 1 by the same rows. Every citation now names the row or line its entry
named when it was written (found by that line's text at the commit that
wrote it; each had one match): all 23 `docs/parity.md` citations and 9 of
the 15 code ones moved. `ledger.py` loads the result; the classifier reads
no citation (`_sources` checks only that the file holds the line).
**Corrected in INT2b:** this pass read `src/` and `docs/parity.md` only and
missed a bare second row (`sound-max`'s `:109, :114`); INT2b re-reads every
citation, `.prd/` and `scripts/` included (below).

## Receipts

### Tests

`cargo test --locked --no-fail-fast`: **4,916 passed, 0 failed, 7 ignored**.
`--features fault-injection`: **4,953 passed, 0 failed, 7 ignored**. Every
`#[test]` a side adds since `8464e981` ran and passed in one of the two
runs: 228 from `main` (epic 1, H3, FK-RULES, FK-GAP, PROC-SPAWN and the
rest), 190 from the MCP side (MCP-C, -W, -T, -S, -C2, -V, -M); 6 and 8 of
them are `fault-injection` only. Neither run had a failure, so none was
re-run.

### Pairs (fx v0.0.16, sha256 `8e6cda2e…`; xfx = the merge, `8bf55a72`, debug)

Counts are identical-or-normalized / approved / total, classified by the
ledger at this branch's head (`sweep.py`; `status-panel/pairs.py` for the
`capture.sh` pairs). "Unit" is each unit's last paired round: for every
`p5-mcp-*` scenario, MCP-M's MCP-Me sweep (`209d3ab5`, 40x120, 85 of 146;
MCP-Mf ran none), MCP-Mc's for 40x60 and `p5-input-mcp`; for epic 1,
FK-GAPb/c's `p5-effort-fork-*` table and FK-GAP's `p5-panels-*`,
`p5-permissions-*` and `p5-session-*` table. "Integrated" is a sweep of
every one of these scenarios at 40x120 on this build, the same as the
CI sweep's head below pair for pair but one (`p5-mcp-menu-old` `end.full`,
below), plus the units' own sizes: 14x120, 12x120 and 40x60 (sweep, debug), and
24x50 with `capture.sh` on the release build, as P-status took it. At 12x120
the sweep takes both captures and then stops at fx's inventory (`a help snap
shows no menu`), so those three pairs are
classified with `pairs.py`.

| scenario | unit | integrated |
|---|---|---|
| `p5-mcp-config` | 1 / 0 / 3 | 1 / 0 / 3 |
| `p5-mcp-config-ignored` | 0 / 0 / 4 | 0 / 0 / 4 |
| `p5-mcp-config-vars` | 2 / 0 / 4 | 2 / 0 / 4 |
| `p5-mcp-direct` | 2 / 0 / 2 | 2 / 0 / 2 |
| `p5-mcp-dismiss` | 2 / 0 / 2 | 2 / 0 / 2 |
| `p5-mcp-empty` | 24 / 0 / 24 | 24 / 0 / 24 |
| `p5-mcp-errdata` | 1 / 0 / 1 | 1 / 0 / 1 |
| `p5-mcp-fail` | 4 / 0 / 6 | 4 / 0 / 6 |
| `p5-mcp-failed` | 4 / 0 / 4 | 4 / 0 / 4 |
| `p5-mcp-known` | 1 / 0 / 1 | 1 / 0 / 1 |
| `p5-mcp-live` | 2 / 0 / 6 | 2 / 0 / 6 |
| `p5-mcp-menu-new` | 22 / 1 / 34 | 22 / 1 / 34 |
| `p5-mcp-menu-new` 40x60 | 22 / 1 / 34 | 22 / 1 / 34 |
| `p5-mcp-menu-old` | 10 / 0 / 16 | 10 / 0 / 16 (one run of four 9) |
| `p5-mcp-notes` | 2 / 0 / 4 | 2 / 0 / 4 |
| `p5-mcp-rpcerror` | 1 / 0 / 1 | 1 / 0 / 1 |
| `p5-mcp-turn` | 3 / 0 / 3 | 3 / 0 / 3 |
| `p5-mcp-unavailable` | 1 / 0 / 1 | 1 / 0 / 1 |
| `p5-mcp-verbs` | 1 / 0 / 28 | 1 / 0 / 28 |
| `p5-mcp-wire-new`, `p5-mcp-wire-old` | 1 / 0 / 1 each | 1 / 0 / 1 each |
| **`p5-mcp-*`, 40x120** | **85 / 1 / 146** | **85 / 1 / 146** |
| `p5-input-mcp` | 9 / 0 / 9 | 9 / 0 / 9 |
| `p5-effort-fork-fork` | 15 / 1 / 16 | 15 / 1 / 16 |
| `p5-effort-fork-fork-edge` | 10 / 1 / 11 | 10 / 1 / 11 |
| `p5-effort-fork-fork-empty` | 4 / 0 / 4 | 4 / 0 / 4 |
| `p5-effort-fork-fork-continue` | 2 / 0 / 2 | 2 / 0 / 2 |
| `p5-effort-fork-effort-menu` | 18 / 0 / 18 | 18 / 0 / 18 |
| `p5-effort-fork-effort-column` | 17 / 0 / 17 | 17 / 0 / 17 |
| `p5-effort-fork-effort-{backtab,ctrlc,down,loading}` | 2 / 0 / 2 each | 2 / 0 / 2 each |
| `p5-effort-fork-effort-nolevels` | 5 / 0 / 5 | 5 / 0 / 5 |
| `p5-effort-fork-effort-short` 40x120 | 3 / 0 / 3 | 3 / 0 / 3 |
| `p5-effort-fork-effort-short` 12x120 | 3 / 0 / 3 | 3 / 0 / 3 |
| `p5-effort-fork-slash-rank` | 3 / 0 / 4 | 3 / 0 / 4 |
| `p5-effort-fork-oldread-src` | 0 / 0 / 3 | 0 / 0 / 3 |
| `p5-effort-fork-oldread-v1` | 2 / 0 / 4 | 2 / 0 / 4 |
| `p5-effort-fork-oldread-v2` | 2 / 0 / 7 | 2 / 0 / 7 |
| `p5-panels-keys` | 2 / 5 / 10 | 2 / 5 / 10 |
| `p5-panels-noauth` | 0 / 2 / 2 | 0 / 2 / 2 |
| `p5-panels-status` | 5 / 6 / 11 | 5 / 6 / 11 |
| `p5-panels-status-first` | 1 / 2 / 3 | 1 / 2 / 3 |
| `p5-panels-status-badsettings` | 0 / 3 / 3 | 0 / 3 / 3 |
| `p5-panels-status-short` 40x120 | 1 / 5 / 6 | 1 / 5 / 6 |
| `p5-panels-status-short` 14x120 | 3 / 1 / 6 | 3 / 1 / 6 |
| `p5-panels-status-short` 24x50 | 1 / 5 / 6 | 1 / 5 / 6 |
| `p5-panels-permissions` | 16 / 1 / 20 | 16 / 1 / 20 |
| `p5-panels-permissions-saved` | 5 / 0 / 8 | 5 / 0 / 8 |
| `p5-permissions-early` | 11 / 0 / 11 | 11 / 0 / 11 |
| `p5-permissions-early-probe-ask-after` | 0 / 1 / 1 | 0 / 1 / 1 |
| `p5-session-early-prefs` | 5 / 0 / 5 | 5 / 0 / 5 |

No pair went down. The research scenarios with no unit count, against the
base (`0e9e7ce8`) in the CI sweep below: `p5-panels-help` 8 -> **9** / 0 / 17
(`tab-extensions` is now normalized-identical: MCP-V's `/mcp` summary,
`manage MCP servers and their logins`, is fx v0.0.16's), `p5-panels-slash`
2 / 0 / 15,
`p5-panels-sweep-a` 5 / 0 / 33 and `p5-panels-sweep-b` 4 / 0 / 11 as the
base, and `p5-panels-badsettings` 0 / 0 / 2 on both: fx v0.0.16 itself exits
1 about four seconds in (its launch snap is blank) in all five runs here, so
the scenario fails on the fx side before xfx is compared.

Two readings the table needed:

- `p5-mcp-menu-old` `end.full` was unresolved in the first 40x120 run and
  normalized-identical in the next three (two re-takes and the CI sweep's
  head). It is the pair MCP-M's receipt names as moving between runs of
  either build: the full transcript reads the hint row while `esc again to
  clear` expires.
- `p5-panels-status-short` at 24x50 taken through `sweep.sh` is 1 / 3 / 6:
  the sweep's own temporary directory (`…/fxsw.<id>/001/fxcap.<id>/work`) is
  two levels deeper than `capture.sh`'s, so the panel's Workspace row wraps
  onto a third row on both sides and pushes the rest. Through `capture.sh`
  with `TMPDIR=/private/tmp/claude-501`, as P-status took it, it is 1 / 5 /
  6.

### Sweep

The `sweep.yml` job, run locally: `ci-sweep.sh --base 0e9e7ce8 --fx <fx
v0.0.16> --pin v0.0.16 --xfx <8bf55a72 debug> --base-xfx <0e9e7ce8 debug>
--jobs 2`, `FX_AUTO_UPGRADE=0`, head `4276a8f2` (a draft of this receipt
with the same scenario list, since amended; the code is the merge's). It ran 63 scenarios (10 core, 9 changed, the rest
named by the receipts) and **468 snap pairs: 300 pass at the head, 25 new,
0 regressions**; 2,196 s. The 25 new pairs are the snaps of the MCP side's
edited scenarios (`p5-mcp-config`, `-config-ignored`, `-config-vars`,
`-errdata`, `-failed`, `-known`, `-live`, `-rpcerror`, `-unavailable`) that
the base's `.cap` files do not take. Every other pair that does not pass at
the head does not pass at the base either: `h1-resize` `small`/`large`
(known on `main`, `scripts/fxcap/ci-core.txt`), the approved epic-1 pairs,
the research scenarios' unresolved ones, `p5-panels-badsettings` (fx's own
exit), and the MCP pairs their units' receipts leave unresolved (`remote`'s
`retrying`, the `list` table's cell, `show`'s order, `login` gated). 40 pairs
went from not passing at the base to passing (`p5-mcp-menu-new` 22,
`-menu-old` 10, `-fail` 4, `-notes` 2, `-verbs` 1, `p5-panels-help` 1), and
`p5-mcp-menu-new` `approve-confirm` from unresolved to approved
(`mcp-approval-wrap-led`). `p5-permissions-early-probe-ask-after`
was retried (a `nondeterminism` cause) and is approved at both.

### Gates

On `8bf55a72`'s code (the commits after it change the ledger's citations
and this directory only), `TMPDIR=/private/tmp/claude-501/ep2`:

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| `cargo clippy … --features fault-injection -- -D warnings` | ok |
| `cargo test --locked --no-fail-fast` | 4,916 passed, 0 failed, 7 ignored |
| `cargo test … --features fault-injection --no-fail-fast` | 4,953 passed, 0 failed, 7 ignored |
| `scripts/check-xfx-identity.sh`, `check-no-stubs.sh`, `check-no-secrets.sh` | ok (on the head) |
| `cargo build --locked --release`; `--release --features fault-injection --target-dir target/faulty` | ok |
| `scripts/smoke.sh target/release/xfx <dir>` | 47 checks, 0 failures |
| `scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx <dir>` | 37 scenarios and the oracle, 1,111 checks, 0 failures |
| `scripts/check-preview-contract.sh` | ok |
| `scripts/check-tui-preflight-cost.sh` | ok (every case under budget) |
| `scripts/check-usage-clock.sh` | ok (debug honoured; both release builds ignored) |

The full suites ran one at a time, each started only when no other `cargo test`
or xfx test binary was running on the machine.

## Not resolved

Nothing. No unit's behaviour had to change for the merge to hold.

## INT2b: the reviews' three SHOULDs, on `1c761e50`

The dispatcher merged `origin/main` `d314549e` (#61 FLAKE-MDTIME, #57 usage
U1-U3) into this branch as `1c761e50`, clean. Then, from the reviews of the
merge (opus SHOULDs 1-3):

1. **No inline column under `/mcp`'s menu** (`14274af9`).
   `offer_to_column` skipped the catalog, the model menu and the settings
   menu, whose search the draft is, but not `/mcp`'s menu, which passes Tab
   and a space on to the composer (`offer_to_mcp`) and is drawn in the slot
   over any column (`slot`). Tab on a bare `/effort` filter opened the
   `/effort ` column and wrote `/effort ` into the filter; Tab and a space on
   `/effort ` or `/model ` went to a column nobody could see. The MCP side
   had the `/model ` half before the merge. Fix: `mcp_menu.is_some()` joins
   the skip (`src/tui/shell.rs:6594`).
   - RED, a `git clone --no-local` at `1c761e50` with the new tests only:
     `a_column_line_typed_into_the_mcp_filter_is_the_filter_not_a_column`
     (`:33915`) fails on all five cases, `[("/effort", "\t", "/effort "),
     ("/effort ", "\t", "/effort default"), ("/effort ", " ", "/effort "),
     ("/model ", "\t", "/model openai/gpt-6-astra"), ("/model ", " ",
     "/model ")]`. GREEN on the head; with the menu closed, the same Tab still
     opens the column.
2. **A fork that lands under a search waits for it** (`14274af9`). The fork
   is the runtime's, so a command recalled and sent between the Enter that
   chose it and the fork landing opens its surface first; upstream forks in
   the Enter's own call (`forkBeforeSelectedPrompt`,
   `app_session_runtime.zig:2101-2127@481104fd`, then `replace`,
   `app_input_runtime.zig:1686`), so it has no such window. `forked_into`
   wrote the prompt into the empty composer, which was `/mcp`'s filter (or
   the catalog's search), and Escape cleared it. Now the prompt is held
   while a surface's search has the composer (`composer_searched`, `:6441`:
   `/mcp`'s menu, the catalog, the settings, the model menu, the session
   picker, `/skills`' search -- the surfaces `reconcile_picker` hands the
   draft to) and goes into the composer, if it is empty, once none has it
   (`settle_fork_prompt`, `:6421`, after every input as `settle_model_draft`
   is, `:10815`, `:10992`, and when the fork lands, `:6412`). Unchanged: an
   edit drops it, a search's text included, as one before the fork lands
   already did (`amended`, `:14596`), and a reset drops it (`:14124`).
   - RED, the same clone:
     `a_fork_that_lands_under_the_mcp_menu_keeps_its_prompt_out_of_the_filter`
     (`:42544`) fails with `/mcp: the fork's prompt went into the search`
     (`left: "second prompt beta"`), and with the `/mcp` case taken out,
     `/help: the fork's prompt went into the search`. GREEN on the head: the
     composer stays empty under each, the prompt is there once Escape closes
     it, nothing is sent, and a search typed into after the fork landed
     leaves the prompt out.
3. **The ledger's notes** (`0802caaa`). `banner`'s `why` says
   `src/tui/shell.rs:185`, as its `source` does; `sound-max`'s
   `docs/parity.md:109, :116` (`/settings`, `/sound`). Then every citation
   of the ledger -- `source`, the `xfx` keys and the prose, `.prd/` and
   `scripts/` included -- was read against the line it named in the commit
   that wrote it into that entry (by the entry's id; a parity row by its
   first cell, any other line by its text), and the 17 that no longer named
   it were re-pointed, each to its one match: #57's three new parity rows
   (`:207` -> `:209`, `:212` -> `:215`, `:234` -> `:237`), the two shell.rs
   lines this round moved (`:1569` -> `:1575`, `:36659` -> `:36757`), and
   older drift (`capture.sh` `:155-157`/`:193`/`:420` -> `:252-254`/`:416`/
   `:756`, `.prd/07-fx-parity-program.md` `:43`/`:55` -> `:45`/`:57`,
   `src/session/store.rs:1044` -> `:1059`, `src/image.rs:95` -> `:101`,
   `src/tools/mcp.rs:601-607` -> `:422-428`). The same read on the head
   finds none. One it cannot place is left: the `c1db919` pin's `note`
   cites `.prd/07-fx-parity-program.md:19`, the "reference binary was
   replaced in place" item that the program no longer holds. The note is
   not read by `ledger.py`.

**Pairs** (the INT2b build, debug, 40x120): the scenarios the two fixes
come nearest, every pair at the count above -- `p5-effort-fork-fork` 15 / 1
/ 16, `-fork-edge` 10 / 1 / 11, `-fork-empty` 4 / 0 / 4, `-fork-continue`
2 / 0 / 2, `-effort-column` 17 / 0 / 17, `-effort-menu` 18 / 0 / 18,
`p5-mcp-menu-new` 22 / 1 / 34, `p5-mcp-empty` 24 / 0 / 24, `p5-input-mcp`
9 / 0 / 9, `p5-mcp-verbs` 1 / 0 / 28. No scenario opens `/mcp` over a
column line or forks under a search, so no pair moves.

**Gates** on `0802caaa`'s code (the receipt commit after it changes this
directory only), `TMPDIR=/private/tmp/claude-501/ep2`, each full suite
started only when no `cargo test` or xfx test binary was running (by name,
anchored):

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| `cargo clippy … --features fault-injection -- -D warnings` | ok |
| `cargo test --locked --no-fail-fast` | 5,021 passed, 0 failed, 7 ignored |
| `cargo test … --features fault-injection --no-fail-fast` | 5,058 passed, 0 failed, 7 ignored |
| `scripts/check-xfx-identity.sh`, `check-no-stubs.sh`, `check-no-secrets.sh` | ok |
| `cargo build --locked --release`; `--release --features fault-injection --target-dir target/faulty` | ok |
| `scripts/smoke.sh target/release/xfx <dir>` | 47 checks, 0 failures |
| `scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx <dir>` | 37 scenarios and the oracle, 1,111 checks, 0 failures |
| `scripts/check-preview-contract.sh` | ok |
| `scripts/check-tui-preflight-cost.sh` | ok (every case under budget) |
| `scripts/check-usage-clock.sh` | ok |

No test failed, so none was re-run.

## The scenarios this receipt names for the sweep

So the PR's own `sweep.yml` run takes the set paired here: every epic-2
scenario, every epic-1 scenario, and `p5-input-mcp` (MCP-M's extra):
`p5-effort-fork-effort-backtab.cap`, `p5-effort-fork-effort-column.cap`,
`p5-effort-fork-effort-ctrlc.cap`, `p5-effort-fork-effort-down.cap`,
`p5-effort-fork-effort-loading.cap`, `p5-effort-fork-effort-menu.cap`,
`p5-effort-fork-effort-nolevels.cap`, `p5-effort-fork-effort-short.cap`,
`p5-effort-fork-fork-continue.cap`, `p5-effort-fork-fork-edge.cap`,
`p5-effort-fork-fork-empty.cap`, `p5-effort-fork-fork.cap`,
`p5-effort-fork-oldread-src.cap`, `p5-effort-fork-oldread-v1.cap`,
`p5-effort-fork-oldread-v2.cap`, `p5-effort-fork-slash-rank.cap`,
`p5-input-mcp.cap`, `p5-mcp-config-ignored.cap`, `p5-mcp-config-vars.cap`,
`p5-mcp-config.cap`, `p5-mcp-direct.cap`, `p5-mcp-dismiss.cap`,
`p5-mcp-empty.cap`, `p5-mcp-errdata.cap`, `p5-mcp-fail.cap`,
`p5-mcp-failed.cap`, `p5-mcp-known.cap`, `p5-mcp-live.cap`,
`p5-mcp-menu-new.cap`, `p5-mcp-menu-old.cap`, `p5-mcp-notes.cap`,
`p5-mcp-rpcerror.cap`, `p5-mcp-turn.cap`, `p5-mcp-unavailable.cap`,
`p5-mcp-verbs.cap`, `p5-mcp-wire-new.cap`, `p5-mcp-wire-old.cap`,
`p5-panels-badsettings.cap`, `p5-panels-help.cap`, `p5-panels-keys.cap`,
`p5-panels-noauth.cap`, `p5-panels-permissions-saved.cap`,
`p5-panels-permissions.cap`, `p5-panels-slash.cap`,
`p5-panels-status-badsettings.cap`, `p5-panels-status-first.cap`,
`p5-panels-status-short.cap`, `p5-panels-status.cap`,
`p5-panels-sweep-a.cap`, `p5-panels-sweep-b.cap`,
`p5-permissions-early-probe-ask-after.cap`, `p5-permissions-early.cap`,
`p5-session-early-prefs.cap`.
