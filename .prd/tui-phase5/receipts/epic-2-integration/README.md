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
     (`:42544`, renamed in INT2c) fails with `/mcp: the fork's prompt went into the search`
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
   first cell, any other line by its text), and the 16 more that no longer
   named it were re-pointed, each to its one match (18 changed lines with
   the two notes; corrected in INT2c, which first said 17): #57's three new parity rows
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

## INT2c: the held prompt and a session switch (opus MUST), C-p, the pins

Both engines reviewed INT2b. gpt6 had no MUST; opus had one, a regression
INT2b made, traced in its review (`EPIC2-INT2b/reviews/opus.md`).

- **MUST: a held fork prompt followed a resume into another session.**
  `/resume` recalled and sent while the fork was on its way opens the
  picker, a search, so the fork's prompt was held. The user resumes another
  session; `UiEvent::Resumed` closes the picker with `close_sessions(true)`,
  which on an empty search skips `amended`, and `hydrate_as` did not clear
  `fork_prompt_held`. So `settle_fork_prompt` wrote the forked
  conversation's prompt into the resumed session's composer on the next
  tick, or before the next key. Before INT2b the prompt went into the
  picker's search and was dropped with it.
  - Fix (`7e20b1a2`): `hydrate_as` clears `fork_prompt_held` with the rest
    of the replaced conversation (`src/tui/shell.rs:3610`). Every way a
    different session becomes current, and each now drops it:
    - a `/resume` from the picker (`UiEvent::Resumed` -> `hydrate` ->
      `hydrate_as`);
    - a launch's `--resume` (`hydrate`; nothing can be held yet);
    - a fork (`UiEvent::Forked` -> `forked_into` -> `hydrate_as`, which then
      holds that fork's own prompt);
    - `/clear`, `/new`, `/reset` (`reset_session`, `:14139`, as before).
  - RED, the new test on `ce7ee7db`'s code:
    `a_fork_prompt_held_under_the_session_picker_does_not_follow_a_resume_into_another_session`
    (`:42754`): the listing, another session chosen, `Resumed`,
    `settle_input` -> `left: "second prompt beta", right: ""`. GREEN on the
    head: the composer is empty and nothing is sent.
- **SHOULD 1: C-p's model menu lost the prompt** (not a regression). C-p
  lends even an empty draft, and giving it back went through `amended`,
  which dropped a prompt held under the menu, or one still on its way.
  `give_back_model_draft` (`:6928`) now keeps both through its `amended`:
  giving the draft back is no edit of the user's, and an edit in the menu's
  search has already dropped them. A draft given back non-empty is the
  user's text, which `settle_fork_prompt` leaves the prompt out of.
  - RED: `a_ctrl_p_model_menu_opened_and_closed_with_no_edit_keeps_the_forks_prompt`
    (`:42709`), both cases. Landed under the menu: `left: "", right:
    "second prompt beta"`. Landed after it (run first): the same. GREEN on
    the head.
- **SHOULD 2: pins.**
  - `a_fork_that_lands_under_a_search_keeps_its_prompt_out_of_it`
    (`:42559`, INT2b's `/mcp` test, widened): all six surfaces of
    `composer_searched` (`/mcp`, `/help`, `/settings`, `/model`, `/resume`,
    `/skills`), over a fixture with skills.
  - `a_key_read_with_the_close_of_a_search_lands_after_the_forks_prompt`
    (`:42613`): Ctrl-C closing `/mcp`'s menu and `x` in one read give
    `second prompt betax`, the settle before a key (`:10830`).
  - `a_held_forks_prompt_never_replaces_text_and_a_reset_drops_it`
    (`:42629`): a recalled `/version` lent to C-p comes back over the held
    prompt (the empty-composer guard, `:6426`), and `reset_session` drops
    one held (`:14139`).
  - Mutants on a throwaway clone, each caught by its test: no clear in
    `hydrate_as`; no keeping through C-p's give-back; no settle before a
    key; no empty-composer guard; no reset clear; each of the six surfaces
    out of `composer_searched`: 11 of 11 fail (`mutants_int2c.py`, run
    with a `git clone --no-local` at `1c761e50` holding the head's
    `src/tui/shell.rs` as its argument).
- **SHOULD 3:** the INT2b count above, corrected to 16 (18 lines with the
  notes).
- **The ledger** (`652e9c0a`): the session record's two `xfx` citations
  follow this round's tests, `shell.rs:36757` -> `:36772`. The read of
  every citation on the head finds none stale.

**Pairs** (the INT2c build, debug, 40x120): every scenario that touches
`/resume`, `/fork` or `/mcp` -- all 16 `p5-effort-fork-*`, all 20
`p5-mcp-*`, `p5-input-mcp`, `p5-panels-sweep-a` and `-sweep-b` (`/resume`)
-- pair for pair as the INT2 CI sweep's head but one scenario's run:
`p5-effort-fork-*` at the counts above, `p5-input-mcp` 9 / 0 / 9,
`p5-panels-sweep-a` 5 / 0 / 33, `-sweep-b` 4 / 0 / 11, `p5-mcp-*` 85 / 1 /
146 once `p5-mcp-menu-old` is taken again. In the sweep (load 9 at its
peak) that scenario was 8 / 0 / 16: `closed` and `end.full` read `┃ xd` and
`esc again to clear` after its two Escapes, where fx's second Escape had
cleared the composer -- the second Escape outside the double-Escape window,
the timing `end.full` already moves on (above). The menu is not up at
either snap (its `d-key` snap is `┃ xd` on both sides). Taken again three
times on the same build: 10 / 0 / 16 each, both pairs normalized-identical.

**Gates** on `652e9c0a`'s code (the receipt commit after it changes this
directory only), `TMPDIR=/private/tmp/claude-501/ep2`. Each full suite
started after `pgrep -x cargo` and `pgrep -x 'xfx-[0-9a-f]+'` had both come
back empty twice, 45 s apart:

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| `cargo clippy … --features fault-injection -- -D warnings` | ok |
| `cargo test --locked --no-fail-fast` | 5,025 passed, 0 failed, 7 ignored |
| `cargo test … --features fault-injection --no-fail-fast` | 5,062 passed, 0 failed, 7 ignored |
| `scripts/check-xfx-identity.sh`, `check-no-stubs.sh`, `check-no-secrets.sh` | ok |
| `cargo build --locked --release`; `--release --features fault-injection --target-dir target/faulty` | ok |
| `scripts/smoke.sh target/release/xfx <dir>` | 47 checks, 0 failures |
| `scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx <dir>` | 37 scenarios and the oracle, 1,111 checks, 0 failures |
| `scripts/check-preview-contract.sh` | ok |
| `scripts/check-tui-preflight-cost.sh` | ok (every case under budget) |
| `scripts/check-usage-clock.sh` | ok |

No test failed, so none was re-run.

## INT3: `main` @ `46c4efa6` (#63 PROFILE-DIR, #62 usage U4-U6), opus's INT2c SHOULD 1

INT2c closed with no MUST from either engine (`EPIC2-INT2c/reviews/`).

### The merge

`118d4b4e`, `git merge --no-edit 46c4efa6` (#63 `f71f7ea8`, #62): no
conflict, and `git show --remerge-diff 118d4b4e` prints nothing (0 lines). It
did not compile, though: one semantic seam, below, fixed in `3cf5075e`. So
`118d4b4e` alone does not build; every commit after it does.

### The seams

- **PROFILE-DIR and `/mcp`'s writer.** #63 deleted `create_private_dir` from
  `src/mcp/config.rs` and moved the whole-file profile writer (`edit_profile`)
  to `profile_home::ensure`. MCP-V's `update` -- the writer of `/mcp add`,
  `/mcp remove` and their `--project` form -- still called it
  (`E0425`, `src/mcp/config.rs:1916` as merged). Fix (`3cf5075e`):
  - `update` makes the profile home private or refuses it, never repaired
    (`profile_home::ensure(dir, dir)`, `src/mcp/config.rs:1932`), for both
    files: the lock is the profile's whichever file is edited, as upstream's
    `profileDir` opens it verified private for the lock
    (`src/core/mcp_host/config_file.zig:147-155@481104fd`). A refusal is said
    in the session store's words after `Couldn't add <name> to <file>: `
    (`EditError::Refused`, `:1645`); upstream says `something went wrong`
    there (`describe`, `src/core/mcp_host/verbs.zig:313-326@481104fd`).
  - **The workspace's `.mcp.json` is not profile state.** It is the
    project's shared file, upstream's rule for it: "A project's `.mcp.json`
    is shared, so it keeps its mode", a link or a file with another hard
    link refused, the file replaced atomically where it is (`write`,
    `src/core/mcp_host/config_file.zig:157-171@481104fd`). xfx's `replace`
    already does that (the mode kept, `Symlink`/`HardLink`/`ReadOnly`
    refused); only the lock goes through the profile home. `docs/parity.md`'s
    profile-home row says both now, and that the verbs say the store's
    sentence (the error names it listed are the test-only writer's).
  - **The writer table** (`src/profile_home/tests.rs`, "every product
    writer"): its MCP entry ran `add_profile_server`, the whole-file writer
    MCP-V made test-only (`9034e3b9`). It runs `/mcp add` for the profile and
    with `--project` instead (`:148`, `:152`), so every case of the table
    (missing home, open home, linked home, umask) covers the verbs' writer.
  - **The scan passes for the right reason.** `src/mcp/config.rs`'s product
    code makes no directory (the deleted helper's `DirBuilder::new(` is gone
    with it) and holds `crate::profile_home::ensure(dir, dir)` once, in
    `update` (`edit_profile`'s is test-only and cut). Mutants (a throwaway
    clone, `mutants_int3.py`): `update` making the home with
    `fs::create_dir_all` first fails
    `no_product_code_makes_a_directory_but_on_the_lines_allowed`; `update`
    with no check fails the open-home writer case (`/mcp add` wrote into a
    `0755` home) and `every_profile_writer_makes_its_directories_through_the_helper`.
  - **Every other MCP write in the profile** is a settings write (a project
    server's approval or rejection, `save_action` ->
    `provider::profile::edit_document`), which #63 already routes.
  - **Test fixtures.** The MCP side's cases that made `~/.xfx` with the
    umask's mode, or put it under a home that was not there, are refused now,
    as #63's own fixtures were; they make it as xfx does
    (`profile_home::made`, a home that exists), as #63 did for main's:
    `scratch_repo` (a home), `mcp::config::tests` x3, `mcp::command::tests`'
    `write_profile`, the shell's `mcp_args_shell` (`3cf5075e`) and `McpCase`
    (`189173da`, 13 MCP-M cases). No assertion changed.
- **#62's `/usage` screen and `/mcp`'s menu.** `/mcp` already closes the
  screen and its runtime (`close_usage`, `src/tui/shell.rs:13101`, merged
  clean), and drops a waiting `/fork` (`:13105`). `/usage` closed every other
  surface but not `/mcp`'s menu. It does now (`:12786`, `3cf5075e`); neither
  can open over the other by a typed line (the menu takes Enter and its
  filter is the composer), and upstream's `commandShowUsage` closes the
  skills, model, help, settings and inline menus, not the MCP host menu
  (`src/core/app/app_commands.zig:808-830@481104fd`); the close keeps the
  slot's one-surface rule for any other way the handler is reached, as
  FK-GAPc's opener table does. RED: `the_usage_screen_and_the_mcp_menu_close_each_other`
  (`:42576`) -> `/usage over /mcp: two surfaces in the slot`, `left: 2`.
  `slot_occupants` counts both (`:42329`, `:42331`); FK-GAPc's opener table
  (`:42540`) passes with `/usage` and `/mcp` in it; `usage()` still drops a
  waiting `/fork`.
- **PROC-SPAWN.** Clippy on the merged tree, both feature sets: no
  `disallowed-methods` site, so nothing #62 or #63 brings starts a child or makes a pipe outside `process::*`.

### Opus's INT2c SHOULD 1: Escape before the resume lands

- The fork lands under the session picker (the prompt held), another
  session is chosen (`resume_selected`), and Escape takes the picker down
  while the resume runs: `close_sessions(true)` on an empty search skips
  `amended`, and the settle put the prompt in the composer, which the resumed
  session kept. A fork that lands after the resume was asked for, with the
  picker taken down, did the same -- on `main` too.
- Fix (`3cf5075e`): `resume_selected`'s `Ok` arm drops `fork_prompt` and
  `fork_prompt_held` (`src/tui/shell.rs:6295`): another session is on its
  way in, and both prompts are the replaced conversation's.
- RED, before the fix: `a_fork_prompt_held_under_the_session_picker_is_dropped_by_the_resume_an_escape_left_running`
  (`:43007`) -> `the forked conversation's prompt came back over a resume in
  flight`, `left: "second prompt beta"`;
  `a_fork_still_on_its_way_when_another_session_is_resumed_brings_no_prompt_to_it`
  (`:43054`) -> `left: "second prompt beta"`. GREEN after. Mutant (clone):
  without the two drops, both fail.
- **Residuals, as decided (opus SHOULD 2), each a prompt lost rather than
  carried into another conversation:** C-p, or `/model`'s menu going on into
  a model's effort column, drops a held prompt; and a resume that was asked
  for and then fails (`ResumeFailed`) leaves the forked conversation current
  with its prompt already dropped by `resume_selected`. `main` lost the
  prompt on both paths too (on the second, the picker's search held it and
  Escape's `close_sessions(true)` threw it away).

### Ledger

`d25f1d83`, `52fdf178`: #63's and #62's rows moved `docs/parity.md` `:209` ->
`:210`, `:215` -> `:216`, `:237` -> `:238`; #62 moved
`src/gateway/mod.rs:86` -> `:59` and `src/session/store.rs:1059` -> `:992`;
INT3's tests moved `src/tui/shell.rs:1575` -> `:1584` and `:36772` ->
`:36815`. The read of every citation on the head finds none stale.

### Pairs (fx v0.0.16, sha256 `8e6cda2e…`)

Every capture is from a binary built at `52fdf178`, the INT3 code head
(this receipt's commit after it adds this directory's text and
`mutants_int3.py` only): the debug build, whose row 1 reads `x𝒇x
v0.1.0-52fdf17 [debug]`; the release build for 24x50, and a debug build
stamped `XFX_BUILD_CHANNEL=release` for the `/usage` pairs (#62's receipt
took them that way: the plain debug banner, 52 cells, wraps at 50 columns and
moves every row, `xfx-debug-banner-wrap`); both banners read `x𝒇x v0.1.0`,
and both binaries hold the revision `52fdf178a05a`.

**Epic 1 and epic 2**, every scenario at 40x120 in the CI sweep below, and
the units' own sizes (14x120, 12x120, 40x60 by `sweep.sh`; 24x50 by
`capture.sh` on the release build). Each at its unit count or above:

| scenario | unit (INT2c) | INT3 |
|---|---|---|
| `p5-mcp-*` (20), 40x120 | 85 / 1 / 146 | 85 / 1 / 146 |
| `p5-mcp-menu-new` 40x60 | 22 / 1 / 34 | 22 / 1 / 34 |
| `p5-input-mcp` | 9 / 0 / 9 | 9 / 0 / 9 |
| `p5-effort-fork-*` (16) | as in the table above | each the same |
| `p5-effort-fork-effort-short` 12x120 | 3 / 0 / 3 | 3 / 0 / 3 |
| `p5-panels-status-short` 14x120 | 3 / 1 / 6 | 3 / 1 / 6 |
| `p5-panels-status-short` 24x50 | 1 / 5 / 6 | 1 / 5 / 6 |
| `p5-panels-*`, `p5-permissions-*`, `p5-session-early-prefs` | as in the table above | each the same, but `p5-panels-help` 9 -> **10** / 0 / 17 and `p5-panels-slash` 2 -> **5** / 0 / 15 |

The four that went up (`help` `query-cost`, `slash` `cost-run`,
`filter-co`, `filter-st`) pass at the base too: #62's, not this merge's.

**`/usage`** (#62's receipt, `usage-screen` and `usage-ledger-retire`), every
`p5-usage-*` scenario at #62's three sizes, xfx stamped as #62's was; 40x120
and 24x80 classified by the sweep, 40x50 by `status-panel/pairs.py` (the
sweep's fx inventory does not parse at 50 columns, as #62 found):

| scenario | #62 (each size) | INT3 40x120 | 40x50 | 24x80 |
|---|---|---|---|---|
| `p5-usage-cached` | 21 / 0 / 21 | 21 / 0 / 21 | 21 / 0 / 21 | 21 / 0 / 21 |
| `p5-usage-budget` | 14 / 0 / 14 | 14 / 0 / 14 | 14 / 0 / 14 | 14 / 0 / 14 |
| `p5-usage-many` | 5 / 0 / 5 | 5 / 0 / 5 | 5 / 0 / 5 | 5 / 0 / 5 |
| `p5-usage-nokey` | 0 / 4 / 4 (`usage-no-sign-in`) | 0 / 4 / 4 | 0 / 4 / 4 | 0 / 4 / 4 |
| `p5-usage-turns` | 18 / 0 / 20, 2 `stats-bytes` | 18 / 0 / 20, `stats`, `credits` `stats-bytes` | the same | the same |
| `p5-usage-noledger` | 4 rows-identical (40x120) | 4 / 0 / 4 | 4 / 0 / 4 | 4 / 0 / 4 |
| `p5-usage-codex`, `p5-usage-oidc` (research, no unit count) | base `46c4efa6`, 40x120: 0 / 0 / 4 and 3 / 0 / 3 | 0 / 0 / 4, 3 / 0 / 3 | the same | the same |

### Sweep

`sweep.yml`'s job, run locally: `ci-sweep.sh --base 46c4efa6 --fx <fx
v0.0.16> --pin v0.0.16 --xfx <52fdf178 debug> --base-xfx <46c4efa6 debug>
--jobs 2`, `FX_AUTO_UPGRADE=0`. 63 scenarios (10 core, 9 changed, the rest
named by the receipts), **468 snap pairs: 304 pass at the head, 25 new, 0
regressions**; 2,235 s. The 25 new pairs are the MCP side's edited
scenarios' snaps the base's `.cap` files do not take (`p5-mcp-config` 3,
`-config-ignored` 4, `-config-vars` 4, `-errdata` 1, `-failed` 4, `-known`
1, `-live` 6, `-rpcerror` 1, `-unavailable` 1), as against `0e9e7ce8`. Every
pair not passing at the head does not pass at the base either.

### Gates

On `52fdf178`, `TMPDIR=/private/tmp/claude-501/ep2`, each full suite started
after `pgrep -x cargo` and `pgrep -x 'xfx-[0-9a-f]+'` both came back empty
twice, 45 s apart:

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| `cargo clippy … --features fault-injection -- -D warnings` | ok |
| `cargo test --locked --no-fail-fast` | 5,042 passed, 0 failed, 7 ignored |
| `cargo test … --features fault-injection --no-fail-fast` | 5,079 passed, 0 failed, 7 ignored |
| `scripts/check-xfx-identity.sh`, `check-no-stubs.sh`, `check-no-secrets.sh` | ok |
| `cargo build --locked --release`; `--release --features fault-injection --target-dir target/faulty` | ok |
| `scripts/smoke.sh target/release/xfx <dir>` | 47 checks, 0 failures |
| `scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx <dir>` | 37 scenarios and the oracle, 1,111 checks, 0 failures |
| `scripts/check-preview-contract.sh` | ok |
| `scripts/check-tui-preflight-cost.sh` | ok (every case under budget) |
| `scripts/check-usage-clock.sh` | ok |

The first default-suite run, on `d25f1d83`, failed 13 MCP-M shell cases
(`McpCase`'s open `~/.xfx`, above); `189173da` fixed the fixture, and the
runs above are the whole suites again, not the 13 alone.
