# tui-unsaved (P5 L-U3b): with no usable `HOME` the TUI runs unsaved, as fx does

This is L-U3's follow-up (`../relative-home/`). The coordinator decided to follow fx: xfx's
`NO_STORE` refusal came from `0a0808f6`, with no user decision behind it.
The unit is security-sensitive (file writes). Base: `feat/p5-tui-unsaved` @ `771b428b`.

Commits:
- `8e920a5c`: the unsaved store, the TUI, the parity text, the scenarios, and the ledger causes dropped.
- `dee4060f` and `914e897e`: the relative-`HOME` link base and its parity sentence.
- `83d06be6`: the pins.
- `f04090b9`: the `launch-refusal-row` cause.
- `a38a413c` (round 2): an unsaved session's images go out again, no title is asked, the pins, and the link base's limit stated.

Binaries (sha256 prefix of `target/debug/xfx`):
- RED `715a65de…`, built at `771b428b` (banner `v0.1.0-771b428`).
- Round 1 GREEN `6c3d4d49…`, built at `83d06be6` (banner `v0.1.0-83d06be`). Round 2 RED for the image pair is this binary: `src/` is unchanged from `83d06be6` to `82919cc0` (`git diff --stat 83d06be6 82919cc0 -- src tests Cargo.toml Cargo.lock build.rs` is empty).
- **GREEN `a1fbfed4…`, built at `a38a413c` (banner `v0.1.0-a38a413`)**: every capture in `green/`.

fx is v0.0.16 (`8e6cda2e…`), checked by `capture.sh` around every run.

## 1. The session with no store

What fx does with `HOME` unset, empty or relative, at `481104fd`:

| Surface | fx | Source |
|---|---|---|
| launch | the TUI opens; `! history: durable prompt history unavailable` | `initializePersistence`, `src/core/app/app_session_runtime.zig:1438-1449` (optional: no store, no error); `app_bootstrap_runtime.zig:667-673`; `prompt_history_runtime.zig:49` |
| a turn | runs; nothing written | `v2_store == null` |
| `/status` | `Session  none · this shell can't save sessions` | `src/core/session/session_commands.zig:404-408`, `src/core/output/output_contracts.zig:734` |
| `/resume` | `✗ session: unable to list saved sessions: SessionStoreUnavailable`, over a picker reading `Unable to load sessions.` | `writeSessionPickerError`, `app_session_runtime.zig:2436-2447`; `pickerSource` null, `:2237-2242` |
| `/fork` | `! session: /fork needs a saved session`, asked before the busy check | `openForkMenu`, `:2072-2081` |
| `/rename <t>` | `✗ session: no active session to rename`; the title is not cached | `renameActiveSession`, `:3141-3146`; `app_commands.zig:3031` |
| `/permissions remember` | `saved-session permission rules require an active saved session` | `activeSessionId`, `:2917-2921`; `app_commands.zig:610-617` |
| `/new` | another unsaved session; the history line is not repeated | capture `u06`/`u07` |
| `/quit` | no `--resume` line | capture `u08` |
| `fx --resume-last` | `fx: HomeNotSet`, exit 1 | `initializePersistence(required)`, `:1443-1445` |

What xfx does now:
- `SessionStore::unsaved` (`src/session/store.rs`). A session it opens is appended to and reduced in memory. Its disk half is `None`, so it has no directory, log, lock, manifest, image copies or rules file. Its paths sit under `/dev/null/.xfx`, below a file, so any write that reached one would fail instead of landing in the working folder.
- Listing, reading, resuming and forking say `SessionStoreUnavailable`.
- `WritableSession`'s `dir`/`file` became `disk: Option<Disk>`. Every writer of a session directory now has to say what it does without one. Images are recorded by their source path, with no copy.
- The TUI worker uses that store when `profile_dir` is `None`. The shell gives every row in the table above in fx's words.
- `bind_conversation` gives an unsaved session neither a durable id nor a rules path, so an "always" approval lasts the process.
- The line shell's refusal (`interactive.rs:795`) is unchanged; fx has no line shell. `docs/parity.md` says this.

## 2. A relative `HOME` and the auto-mode link check

L-U3 made a relative `HOME` no profile home. That removed the link check's home base (`tools/mutate.rs:588-591`) and its git-include walk (`links.rs:329-338`). Yet a command xfx runs is still handed the raw `HOME` (`authority.rs:1469`), and finds its dotfiles under it.

`LinkBase::from_environment` now adds `Home(cwd.join(HOME))` when `HOME` is relative and not empty. It is a `Home` base, not `Root`: `Root` only lists entries, and the git-include walk reads its home from a `Home` base.

fx has no such check. Auto mode refuses by the written path's own components (`pathContainsComponentSequence`, `tool_admission.zig:695-716`). Its git context takes `homeDir()` (`src/core/indexer/git_config.zig:319`), which hands a relative `HOME` to nobody.

So xfx keeps the safer behaviour on purpose, as a divergence of the unsafe-to-copy kind. It is written in `docs/parity.md` (the `ask --add-dir` row's link sentence) and not in the ledger, because no capture row shows it.

## RED (each item on `771b428b`)

`red/` holds the screens, compared in `red.txt`. Every TUI pair differs: the base TUI refuses at launch.

The test RED (`red-tests.txt`) ran on a `git clone --no-local` in the short `TMPDIR`. Each step reset `src/` and `tests/`.
- Items 1 and 2 fail on `771b428b`'s `src/`:
  - `with_no_usable_home_the_session_runs_unsaved_and_writes_nothing`: no READY; the TUI refused.
  - `with_no_usable_home_a_resume_has_nothing_to_continue`: the old refusal text.
  - `a_relative_home_still_has_its_links_checked_where_a_child_would_find_it`: the write to `dotfiles/zshrc`, which the relative home's `.zshrc` links to, succeeded.
- The link test passes on `b0a78fea` (before L-U3), so L-U3 is what lost it.
- Items 3 and 4 pin behaviour `771b428b` already had (`r771-sessions` passes). Their RED is a mutant each.

Mutants:
- **M1:** the unsaved store replaced by a store at `./.xfx`. **M1b:** the unsaved recorder appends `events.jsonl` in the current folder; the tree check names `workspace/events.jsonl`.
- **M2:** no relative-`HOME` base.
- **M3:** the file index reads the process `HOME`.
- **M4:** `--no-save` refused with no home.

Each fails its test.

## GREEN

`green/`, compared in `green.txt` (`scripts/fxcap/sweep.sh --only 'p5-lifecycle-relhome*'`, 15 pairs).

- **Workspace listings: all four byte-identical to fx's.**
- **13 pairs normalized-identical.** That covers every TUI pair of `p5-lifecycle-relhome`, `-made` and the new `-unsaved` (launch, turn, `/resume`, `/fork`, `/rename`, `/new`, the second turn, `/quit`'s scrollback), and both `xfx sessions` pairs. The `tui-without-home*` causes are dropped.
- `u02-status` is approved-divergence-cascade, from the ledger's standing `/status` panel divergences (Sandbox row, no Updates row, step limit). Its `Session` row is fx's.
- `p5-lifecycle-relhome-resume` `r00` is **unresolved, cause `launch-refusal-row`** (kind `decision`).
  - The words match: `xfx: HomeNotSet`.
  - fx takes the terminal before it opens the session `--resume` names. Its restore ends `ESC[1;1H ESC[J ESC[?25h \r\n` and then `fx: HomeNotSet\r\n` (wire, read off a probe run). That leaves a blank row above the message.
  - xfx opens the session before the terminal is touched (`src/tui/mod.rs:127-133`).
  - A probe with a missing id on an ordinary `HOME` shows the same row: `fx: saved session not found.` one row lower than `xfx: there is no session …`. So the row belongs to every refused resume, not to this unit.

## Round 2

### MUST: an unsaved session's images go out again

The defect: the unsaved branches of `attach_images` and `attach_tool_images` committed the record and never set `Snapshot::Loaded`. The reducer leaves a recorded image `Unread`, so `turn_messages` dropped it from the next turn's history. Its notice reads `! session: image #1 of this session was not read back, so it was not sent again`.

fx: the turn keeps the bytes whether or not the session is saved. "The turn keeps the bytes (D44)" is fx's own doc comment (`imageSnapshotTempDir`, `src/core/session/session_store.zig:43-50@481104fd`). `captureImageAttachment` captures every image into the process's temporary folder, and inlines it into the turn only for a saved session (`src/core/app/app_session_runtime.zig:1331-1344@481104fd`).

The fix: both unsaved branches hold the bytes they were handed, as `Snapshot::Loaded`. The path is the image's own: an attached file, or the paste's copy (`PasteCopies`).

- **RED** (`red-tests-r2.txt`): `an_unsaved_sessions_images_are_sent_again_on_every_later_turn` fails on round 1's `src` with `images_of(&live.messages[0])` = 0, against 2.
- **Capture** (`p5-lifecycle-relhome-image`, new): one image, then a second prompt.
  - RED (`red-r2/`): `again` shows the notice, and the second turn's request carries no image.
  - GREEN: both pairs are normalized-identical with fx, and each turn's request carries one image, as fx's do (`green.txt`, last block).
  - RED also sent `001-title.json`, which fx never sends. That led to the next fix.

### A title is asked only for a saved session

fx titles only an active, saved session (`activeSessionId(app) orelse return`, `maybeStartSessionTitleGeneration`, `src/core/app/app_session_runtime.zig:3036@481104fd`). xfx's unsaved session sent the first prompt's excerpt to the title model. `title_asker` now returns none for an unsaved store.

- RED: the base's `title_asker` fails `a_session_with_no_home_runs_unsaved_and_says_so`, at "an unsaved session asked for a title".

### SHOULDs

1. `with_no_usable_home_a_tool_runs_under_the_session_authority_alone` (relative and empty `HOME`): a `read_file` the model calls is answered with the file.
   - Mutant T binds the rules path again in an unsaved session, so every decision fails closed on `/dev/null/...`. It fails at "the read was not answered with the file".
2. The writes-nothing test now takes unset, relative and empty `HOME`. `tree` records each file's length, modification time and bytes. The relative home holds a seeded `.xfx/settings.json` and `history.jsonl`, and the workspace a `notes.txt`.
   - Mutant O rewrites the seeded `history.jsonl` in place under its own name. It fails at `HOME=None wrote into the sandbox`.
3. **The link base's one folder: stated, not widened.**
   - A write is checked before any command picks its folder. The folders a later command can name are every directory in scope, so a base per root would cover the roots but not their subdirectories, while reading as complete.
   - It also needs a link already planted under such a folder's relative home, which only the person can plant: auto refuses a write to `.zshrc` and the rest by name.
   - Written at `src/permission/links.rs` (`Base::from_environment`) and in `docs/parity.md` (the `ask --add-dir` row).

### GREEN, round 2

`green.txt`: 17 pairs.
- 15 are normalized-identical.
- `u02-status` is within the standing `/status` panel divergences.
- `r00-resume` is the `launch-refusal-row` decision.
- All five workspace listings are byte-identical with fx's raw captures.

## Pins (Opus SHOULD 2-4)

- `tui::shell::tests::a_tilde_under_no_usable_home_browses_nothing`: under an empty or relative `HOME`, `~/` is `Unavailable`; under an absolute one it is read.
- `tests/sessions.rs`: under the six relative or empty values, `ask --no-save` exits 0 and answers, and the tree is unchanged. `setup gateway` names `xfx cannot configure gateway because no home directory is set`.

## Noted, out of scope

- `src/llmux/setup.rs:373-375`: a relative `XDG_CONFIG_HOME` is used as it is. `xfx setup llmux` would read `llmux.json` relative to the current folder to pick a port. It is a read, and the URL it yields must still be loopback.
- `src/permission/authority.rs:1469`: an unset (or non-UTF-8) `HOME` reaches a command as `HOME=""` (`unwrap_or_default`), not as no `HOME`.

## How it was captured

```
FX_BIN=<fx v0.0.16> TMPDIR=<short tmp> scripts/fxcap/capture.sh fx  <scenario> 40x120 <dir>
XFX_BIN=<xfx>       TMPDIR=<short tmp> scripts/fxcap/capture.sh xfx <scenario> 40x120 <dir>
```

The scenarios are `p5-lifecycle-relhome`, `-made`, `-unsaved` (new) and `-resume` (new), with the harness at `83d06be6`. Every file here went through `scripts/fxcap/redact.py`.
