# image-loss-copy-fx: a lost history image is fx's -- a refused resume, a failed request, a note to the model

HISTORY-NOTICES-COPY-FX (714f6283, this unit's base) kept xfx's four image read-back notices, N1,
N2, N5 and N6, and queued this unit: fx v0.0.16 (pin `481104fd`) has no such line, and it does not
go on quietly either. This unit copies what fx does in each case, deletes the four lines and the
once-per-conversation ledger that said them, and records the harder failure as a decision for the
user's veto. Every claim below is tied to a file in this folder; every fx line it cites is in
`fx-src.txt`, printed by `fx_src.sh`, and `fx_src.sh <fx> --check README.md` finds each citation
inside a printed range (`checks.txt`).

| commit | what |
|---|---|
| `f61bcf1a` | RED, tests only (`red.txt`) |
| `0f0b6789` | the fix, its docs, the ledger refs: **the code head**, the binary of every xfx head capture and of the head sweep (`binaries.txt`) |
| `67a80e48` | the capture scenario `p5-image-loss-gone` and its seeds (no code) |
| (the receipt commits) | this folder |

## fx's rule, case by case (`fx-src.txt`)

| xfx's case | what fx does at the pin | surface: what the user sees | the model |
|---|---|---|---|
| N1, a prompt image whose copy is **not there or not a regular file** (removed, a link, a directory) | its reader reads every prompt image's blob path as it reads the conversation, and a blob that `stat`s (without following) as no file is `InvalidSessionFormat`: the session is unreadable (`session_adapter.zig:2871-2893`, `session_adapter.zig:4320-4327`; `api.zig:433-442`; `session.zig:1666-1681`; `storage.zig:341-346`) | launch: `fx: saved session is unreadable. Run \`fx doctor\`; if it is recoverable, use \`fx session recover <id>\`.`, exit 1 (`app_entry_runtime.zig:278-285`; captured, `fx/`); `/resume` picker: `Unable to resume this session.` (`app_input_runtime.zig:91-99`, `app_input_runtime.zig:1644-1662`, `resume_menu_presentation.zig:463-467`); `ask --resume`: no notice, so `fx: InvalidSessionFormat`, or the name as `--json`'s error (`cli_ask.zig:926-936`, `cli_ask.zig:1189-1197`, `cli_ask.zig:1306-1326`, `cli_ask.zig:8063-8079`, `app_entry_runtime.zig:158-166`, `app_entry_runtime.zig:563-567`) | nothing: no request |
| N1, a prompt image whose copy is there with **other bytes or another length**, past the limit, or refused | the blob exists, so the session reads; the request build reads and checks it and fails with a `try` (`image_attachments.zig:1260-1320`, `image_attachments.zig:1861-1877`, `vercel_protocol.zig:873-890`, `orchestrator.zig:7712-7716`), after the image route's own refusal (`orchestrator.zig:7520-7525`); the budget before the build raises nothing (`request_image_budget.zig:94-107`) | TUI: `request failed: <Error>` under `system` (`app_process_runtime.zig:62-74`, `app_process_runtime.zig:114-126`): `ImageSnapshotCorrupt`, `ImageTooLarge`, `AccessDenied`; `ask`: `fx: <Error>`, `--json` its name (`cli_ask.zig:1802-1823`) | nothing: no request |
| N2, a prompt image **never read**, or of a type xfx cannot name | no such state in fx: an attachment is read from its blob, its snapshot or its inline bytes, and one with none is `MissingImageSnapshot` (`image_attachments.zig:1213-1243`, `image_attachments.zig:1260-1320`) | as the row above, `request failed: MissingImageSnapshot` | nothing |
| N5, a tool's image whose copy **fails its checks** | a result's images are kept under one handle and load whole or not at all (`execution_memory.zig:975-991`, `result_store.zig:286-302`, `result_store.zig:584-595`); the blob read is `NotFound`, `Corrupt` or `Io` (`session.zig:1642-1665`), mapped to `ResultHandleNotFound`, `BlobDamaged`, `BlobStoreFailed` (`session_adapter.zig:1134-1160`, `result_store.zig:728-742`, `session_child_store.zig:61-76`, `session_child_store.zig:832-836`) | nothing on any screen (`materializeToolImages` pushes nothing, `execution_memory.zig:993-1022`) | `[Stored tool image unavailable: <Error>]\n` in front of the result, the result cut to fit 64 KiB beside it (`execution_memory.zig:1013-1018`, `execution_memory.zig:1071-1076`, `tool_result_limits.zig:7`) -- on the native image route only (`orchestrator.zig:7612-7622`) |
| N6, a tool's image **never read** | the handle with nothing to load it from (`execution_memory.zig:1009-1011`) | nothing | `[Stored tool image is unavailable in this session.]\n` in front of the result |

N2's "type xfx cannot name" and N6 are xfx's own states: a log naming a type xfx cannot name is
refused when it is read (`check_recorded`, `src/session/store.rs`), and no real path leaves a live
tool image unread (HISTORY-NOTICES-COPY-FX's README, "Why the fixture's image is `Unread`"). They
take fx's nearest branch rather than a line of xfx's own.

## The change (`0f0b6789`)

- **`src/session/store.rs`.** `Snapshot::Unusable` carries a `Loss` (missing, not a file, too
  large, changed, denied, failed) in place of a sentence; `read_snapshot` `lstat`s first, as fx's
  `stat`, so a pipe by that name is not waited on. `read_snapshots` says whether a prompt image
  makes the session unreadable, and `resume_within` refuses with `SessionError::Unreadable` **after
  the replay and before anything writes** (the log's tail cut, a `workspace_rebound`).
  `DurableState::unsendable` names the error the request fails with; `turn_messages` puts fx's note
  in front of a result whose images cannot all go and sends none of them, and N1, N2, N5 and N6 are
  gone with `ReplayedHistory::notices`. The compaction reads a turn without the note
  (`src/agent/compact.rs`): fx's note is the request's (`materializeToolImages`), not the history's.
- **The surfaces.** The TUI's turn ends `request failed: <Error>` right after `image_refusal`,
  before anything is written or asked (`src/tui/worker.rs`); `xfx ask` prints `xfx: <Error>`
  (`--json`: the name) for a refused resume and a failed request (`src/app.rs`); the line shell
  reports the same failure (`src/interactive.rs`). The launch and the picker needed no change: a
  launch's error is printed `xfx: <message>` before the terminal is taken, and the picker shows
  `Unable to resume this session.` for anything but busy.
- **Deleted:** the once-per-conversation ledger (`Conversation::unsaid`, `said`, `say_unsaid`), the
  TUI's notice loop, `ask`'s notice loop and `MAX_NOTICE_BYTES`, and the four tests that pinned them
  (two line-shell tests, the TUI's stopped-turn and ledger tests); the compaction join test keeps
  its compaction half, and the screen test keeps its screen half with a neutral notice. The store's
  two read-back tests (a copy changed or gone, a copy that is a link) became the refusal test and
  the request test; the fixture `unreadable_tool_images` stays, for the TUI's note test.
- **The words of the launch: a decision.** fx's line goes on `Run \`fx doctor\`; if it is
  recoverable, use \`fx session recover <id>\`.`; xfx's stops at `saved session is unreadable.`:
  its `doctor` reads no image and it has no `session recover`, so both pointers would send the user
  nowhere. Recorded on the `/image` row.
- Source outside tests: +323 / -204 (527 lines changed, net +119; `size.txt`), of which the
  deletions are the mechanism the brief deletes. See "Open" below.
- `docs/parity.md`: the `/image` row says what xfx does now, retires its IMAGE-LOSS-COPY-FX owner
  and expiry, and records the decision; the `session resume` row drops "prints its history
  notices". `.prd/tui-phase4/ssot.md` no longer says the notices stay. Two comments that compared
  themselves with "the history notices" lose the comparison. The ledger's six refs to
  `src/tui/shell.rs:39046` move to `:39041`, the line they named, five lines up after the screen
  test's edit (`scripts/fxcap/parity-ledger.toml`); every other line ref into a changed file still
  names its line.

## RED and GREEN

- **RED (`red.txt`):** at `f61bcf1a`, 11 of its 12 new or flipped tests fail on the base code,
  each on the behaviour: the resume goes on (store, picker, `ask`, the launch's pty test), the turn
  runs and asks the model (TUI, `ask`), and the tool result goes without the note while the screen
  says N5 or N6 (store, TUI) and `ask`'s request carries it bare. The one that passes pins what does not change:
  a copy with other bytes still resumes and is not sent. The fix widens it to name each error
  (`a_prompt_images_copy_that_cannot_go_resumes_and_names_what_its_request_fails_with`) and adds
  `a_prompt_image_never_read_back_names_upstreams_attachment_with_no_file`, both on the store's new
  `unsendable`, which the base does not have.
- **GREEN:** every one passes at `0f0b6789`, in the gate's full run (`gates.txt`).
- **The capture pair (`green.txt`, `fx/`, `xfx/`, `base-xfx/`, `wire.txt`).** `p5-image-loss-gone`
  seeds, for fx, its `p5-effort-fork-v2` session with image #1 in the first prompt as a blob that
  is not there (`seed_fx.py`: the line rewritten and its CRC32C resealed, every original checksum
  reproduced first), and for xfx a session with the same id, workspace and image written by xfx's
  own store, its copy not saved; launches, quits, and relaunches `--resume kH1xJ5Cm-oRk`. fx's
  oracle (sha256 `8e6cda2e…`, guarded by `capture.sh`) prints
  `fx: saved session is unreadable. Run \`fx doctor\`; if it is recoverable, use \`fx session recover <id>\`.`
  and exits 1; the head prints `xfx: saved session is unreadable.` and exits 1; the base resumes
  the session and opens it. After the run fx's log and xfx's two files have their seeded sizes,
  and the base's have grown (its `workspace_rebound`).
  - **fx vs head** (`refused`): three rows. fx's refusal is on row 3 and xfx's on row 2: fx takes
    the terminal at the relaunch and gives it back before it refuses (`wire.txt`), where xfx's
    launch refuses before it takes it, as it does every launch refusal (`docs/parity.md`, `session
    resume`); the line's own words differ by the two dropped pointers (the decision); row 42 is
    tmux's dead-pane clock. `quit`: the clock alone.
  - **fx vs base**: the base's resumed screen, eight rows.

## Data safety

A refused resume returns before the log's tail is cut and before any event is written
(`resume_within`). Pinned four ways, each with an unpublished tail on the log, which a resume that
went on would cut: the store test hashes every file of the session directory (path, mode, bytes)
before and after, for a removed copy, a link and a directory, through `resume` and
`resume_without_waiting`; `ask --resume` and the launch's pty test compare the session's files
before and after; and the oracle run leaves both binaries' seeded files at their sizes
(`green.txt`). The mutant that refuses only after the cut is killed by all three tests (below).

## Mutants (`mutants.txt`, the runner `mutants.py`)

Each on a `git clone --no-local` copy at `0f0b6789`, never on the worktree, in a suite-lock slot,
900 s per test target, the clone reset after each (`mutants.txt`: 0 changed files after the run).

Run r1 stopped at the second mutant, whose anchor in `src/app.rs` matched twice (the runner refuses
such a mutant before any test runs); r1b ran it, fixed, and the other eight.

| mutant (the brief's) | what it does | targets: verdict (killed by) |
|---|---|---|
| `notice-kept-tui` (a notice line kept) | the TUI says N5 under `session` for a lost tool image | lib: killed (`a_tool_image_that_cannot_go_is_told_to_the_model_and_the_screen_says_nothing`) |
| `notice-kept-ask` (a notice line kept) | `ask` prints the line on stderr | sessions: killed (`ask_resume_never_adopts_…`: "stderr spoke of the image") |
| `resume-goes-on` (the resume going on past a lost prompt image) | `read_snapshots`'s answer ignored | lib, sessions, tui: killed (the store's refusal test, the picker test; `ask`'s; the launch's pty test) |
| `refusal-after-truncate` (a refused resume touching the session file) | the refusal moved after the log's tail is cut | lib, sessions, tui: killed, each on "changed the session" |
| `request-goes-on` | `unsendable` never answers | lib, sessions: killed (both store tests, both TUI turn tests; `ask`'s) |
| `request-goes-on-tui` | the TUI's check skipped, the store's kept | lib: killed (both TUI turn tests) |
| `tool-note-missing` (the tool-image text missing) | the note not put in front | lib, sessions: killed (both store tests, the TUI test; `ask`'s) |
| `tool-note-changed` (the tool-image text changed) | `BlobNotFound` in place of `ResultHandleNotFound` | lib, sessions: killed |
| `tool-images-kept` | the note said and the result's other images still sent | lib: killed (the store's four-result test); sessions: **SURVIVED**, by design: `ask`'s result has one image, and it is the lost one |

Nine mutants: eight killed in every target they ran, one in one of its two.

## Sweep (`sweep-base.md`, `sweep-head.md`, `sweep-compare.txt`)

The scenarios HISTORY-NOTICES-COPY-FX swept that a history image could show in, without its
`provider*` ones (this unit retires no cause there): every `resume*` and `image*`, `undo-resume`, and
the `p5-lifecycle` resumes and image (`relhome-image`, `relhome-resume`, `v1resume`). 19 scenarios,
110 snap pairs, at 40x120 with `--jobs 4`, each sweep in one lock slot. The head ran first: the
binary built at `0f0b6789`, with this branch's ledger and scenarios; the base second, in a `git
clone --no-local` at `714f6283`: its binary, ledger and scenarios (`binaries.txt`). fx's sha was
checked before and after each.

- **Verdicts:** base 86/110, head 86/110, **no pair changed verdict** (`cmp_sweeps.py`, dropped 0).
- **Rows:** `pair_rows.py` over all 110 pairs finds no pair whose unattributed rows or named causes
  moved. No scenario of these resumes over a lost copy or a tool image, so none shows a loss line
  or note on either side (`sweep-compare.txt`, last lines): the sweep says the change moved nothing
  else; `p5-image-loss-gone` is the pair that shows it.
- Load was high: 11.6 at the head's start and 49.4 at its peak, 45.2 on the base. Other workers'
  gates and sweeps shared the machine. No failed or timed-out pair on either side.

## Gate (`gates.txt`, the script `gates.sh`)

`gates.sh` (HISTORY-NOTICES-COPY-FX's, its TMPDIR leaf renamed) ran at `67a80e48` -- `0f0b6789`,
the code head, plus the scenario commit -- in one suite-lock slot (22:44-23:06 UTC), with a clean
tree before and after. It runs every step of `ci.yml` and the two test scripts of `sweep.yml`. Step
00 (`false`) reads `rc=1`; every other step reads `rc=0`:

- fmt, clippy, clippy with fault-injection;
- `cargo test --all-targets`: 5542 passed, 0 failed. With fault-injection: the `tui` test, 201
  passed; the lib, 4641 passed;
- check-no-stubs, check-no-secrets, test-check-no-secrets, test-gate-env, check-xfx-identity,
  check-preview-contract, test-preview-contract, check-tui-preflight-cost, check-usage-clock;
- fxcap test-sweep (70 + 27 tests) and test-capture (79 tests);
- both release builds, smoke, and smoke-tui (37 scenarios + the oracle, 1167 checks, 0 failures).

The receipt commits after it add only this folder; the file checks over them (no-secrets, identity,
no-stubs, the redaction grep, and the citation check) were run again at the receipt's head
(`checks.txt`).

## Open

- **The size.** Source outside tests is +323 / -204, 527 lines changed: past the brief's "about
  500" by 27, of which 204 are the deletion the brief asks for. A split would ship the refusal, the
  request failure and the note first and the deletion second; the dispatcher's call.
- **The non-native route** (a SHOULD at most, no capture): fx loads stored tool images only on the
  native image route, so on a Gateway model the catalog does not tag for images it says its own
  `[Tool images were retained but not sent: …]` for such a result (`orchestrator.zig:7612-7622`);
  xfx says the loss note there too. Recorded on the `/image` row.
- **When the bytes are read:** fx reads a prompt image's blob at every request, xfx once at the
  resume, so a copy mended after the resume is seen only by the next one; and a request that fails
  this way records no turn in xfx, where fx's turn has started [추론: fx writes the prompt when its
  turn starts].
- **`fx session <id>`** may refuse such a session too (it reads the same log); xfx's `session`
  command does not read images. Not traced; not one of the brief's surfaces.
