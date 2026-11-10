# image-loss-copy-fx: a lost history image is fx's -- a refused resume, a held-back image, a failed request, a note to the model

HISTORY-NOTICES-COPY-FX (714f6283, this unit's base) kept xfx's four image read-back notices, N1,
N2, N5 and N6, and queued this unit: fx v0.0.16 (pin `481104fd`) has no such line, and it does not
go on quietly either. This unit copies what fx does in each case, deletes the four lines and the
once-per-conversation ledger that said them, and records the harder failure as a decision for the
user's veto. Round 2 acts on the four reviews of `dba6007b`. Every claim is tied to a file in this
folder. Every fx line cited is in `fx-src.txt`, printed by `fx_src.sh`, and
`fx_src.sh <fx> --check README.md` finds each citation inside a printed range, as it does each
one this unit added to `docs/parity.md`, the code and the scenarios (`new_cites.py`, `checks.txt`).

| commit | what |
|---|---|
| `f61bcf1a` | round 1 RED, tests only (`r1-red.txt`) |
| `0f0b6789` | round 1 fix, its docs, the ledger refs |
| `67a80e48` | round 1 scenario `p5-image-loss-gone` and its seeds |
| `dba6007b` | round 1 receipt |
| `3a17da54` | round 2 RED, tests only (`red2.txt`) |
| `2336c989` | round 2 fix and `docs/parity.md`: **the code head** |
| `f290f697` | round 2 scenarios: the seeds corrected, `p5-image-loss-whole` and `-changed`; the binary of every head capture and of the head sweep (`binaries.txt`) |
| `f42a68ab` | round 2 test: the TUI's per-request test resumes a second time, so the read moved back to the resume fails it (`mutants-r2.txt`) |
| (the receipt commits) | this folder; the last records the rollback check (`rollback.txt`) |

## fx's rule, case by case (`fx-src.txt`, and the captures of round 2)

| xfx's case | what fx does at the pin | what the user sees | the log, the model |
|---|---|---|---|
| a prompt image's copy **not there, not a regular file, or under a folder the `stat` cannot search** | the reader `stat`s each blob as it reads the conversation, without following a link; anything but a regular file is `InvalidSessionFormat` or the I/O error, and the session does not open (`session_adapter.zig:2871-2893`, `session_adapter.zig:4320-4327`; `api.zig:433-442`; `session.zig:1666-1681`; `storage.zig:341-346`) | launch: `fx: saved session is unreadable. Run \`fx doctor\`; if it is recoverable, use \`fx session recover <id>\`.`, exit 1, once it has given the terminal back (`app_entry_runtime.zig:278-285`; `fx/`, `wire.txt`); picker: `Unable to resume this session.` (`app_input_runtime.zig:91-99`, `app_input_runtime.zig:1644-1662`, `resume_menu_presentation.zig:463-467`); `ask --resume`: `fx: InvalidSessionFormat`, or the name as `--json`'s error (`cli_ask.zig:926-936`, `cli_ask.zig:1189-1197`, `cli_ask.zig:1306-1326`, `cli_ask.zig:8063-8079`, `app_entry_runtime.zig:158-166`, `app_entry_runtime.zig:563-567`) | fx's refused launch appends a `workspace` line and a `closed` line (`logs.txt`) |
| a resumed copy that is a file: **unopenable, no size in its header, or past 5 MiB encoded** (so past the 20 MiB limit too) | before the build, on the native route, an image whose file it cannot vouch for -- its size from `stat`, its dimensions from its header -- is left out with a note, and the request goes (`image_attachments.zig:814-850`, `image_attachments.zig:960-988`, `image_attachments.zig:1011-1045`, `image_attachments.zig:1050-1054`; `orchestrator.zig:7612-7632`). The request budget does not hold one back alone: 30 MiB, and 20 MiB is 28 MiB encoded (`request_image_budget.zig:1-22`, `request_image_budget.zig:76-92`). So the build's `ImageTooLarge` (`image_attachments.zig:24`, which `app_process_runtime.zig:99-136` would show as `request failed: image exceeds the 20 MiB limit (ImageTooLarge)`) is not reached | nothing on screen | the model gets `[Image #N not sent: its dimensions could not be verified. The original is saved at <blob>. …]` or `[Image #N not sent: <type> is <W>x<H> pixels. …]` in front of the prompt |
| a resumed copy that is a file with a size and a header, **its bytes not the digest's** | the build reads it for every request and fails with a `try`: `ImageSnapshotCorrupt` (`image_attachments.zig:1260-1320`, `image_attachments.zig:1861-1877`, `vercel_protocol.zig:873-890`, `orchestrator.zig:7712-7716`), after the image route's own refusal (`orchestrator.zig:7520-7525`) | TUI: the prompt's card, `0s (↑4 ↓0)`, `✗ system: request failed: ImageSnapshotCorrupt` (`app_process_runtime.zig:62-74`, `app_process_runtime.zig:99-136`; `fx/fx-p5-image-loss-changed-40x120-failed.txt`); `ask`: `fx: ImageSnapshotCorrupt` (`cli_ask.zig:1802-1823`) | the turn is recorded -- `turn_started`, the prompt, a `turn_end`, `turn_committed` (`logs.txt`); nothing is sent |
| a prompt image **never read** (no bytes, no file) | `imageFile` finds none; no size, so left out with the note that names no file (`image_attachments.zig:1213-1243`, `image_attachments.zig:960-988`) | nothing | `[Image #N not sent: its dimensions could not be verified. No local image file is available; ask the user for a smaller copy.]` |
| a tool's image **whose copy fails its checks** | a result's images load whole or not at all, per request (`execution_memory.zig:975-991`, `result_store.zig:286-302`, `result_store.zig:584-595`); `NotFound`, `Corrupt`, `Io` become `ResultHandleNotFound`, `BlobDamaged`, `BlobStoreFailed` (`session.zig:1642-1665`, `session_adapter.zig:1134-1160`, `result_store.zig:728-742`, `session_child_store.zig:61-76`, `session_child_store.zig:832-836`) | nothing (`execution_memory.zig:993-1022`) | `[Stored tool image unavailable: <Error>]\n` in front of the result, cut to fit 64 KiB beside it (`execution_memory.zig:1013-1018`, `execution_memory.zig:1071-1076`, `tool_result_limits.zig:7`), native route only (`orchestrator.zig:7612-7632`) |
| a tool's image **never read** | the handle with nothing to load it from (`execution_memory.zig:1009-1011`) | nothing | `[Stored tool image is unavailable in this session.]\n` |

## The change

**Round 1 (`0f0b6789`).** The four notices, `ReplayedHistory::notices`, the ledger
(`Conversation::unsaid`, `said`, `say_unsaid`), the TUI's and `ask`'s notice loops and
`MAX_NOTICE_BYTES` go, with the four tests that pinned them. A prompt image's missing copy refuses
the resume with `SessionError::Unreadable` -- after the replay, before anything writes -- printed
`saved session is unreadable.` (the launch, the picker's row) and `InvalidSessionFormat` (`ask`).
A tool image that cannot go is told to the model. The launch line drops fx's two pointers: xfx's
`doctor` reads no image and xfx has no `session recover`.

**Round 2 (`2336c989`).**
- **The resume checks only that each prompt image's copy is a file** (`read_snapshots`,
  `src/session/store.rs`): an `lstat` of the images folder and of the copy, as fx's `blobExists`
  `stat`s them. Not there, not a file, or a `stat` that fails (an unsearchable folder) refuses the
  resume. Every image becomes `Snapshot::Saved(path)`: nothing is read at the resume.
- **The bytes are read when a turn's request is built** (`turn_messages`): at most one byte past
  the limit, and none when the copy cannot be opened. `crate::image::withhold` then leaves out
  what fx would not vouch for, with fx's note: no size from its header, or past 5 MiB encoded.
  That covers a copy past 20 MiB, one cut short, one unreadable, and one never read.
- **Every request reads the copies again** (`CopiesChecked`, a provider wrapper in the TUI's turn
  and in `ask`, outside the network record). A copy whose bytes are not the digest's fails the
  request with `ProviderError::Image("ImageSnapshotCorrupt")`, shown as `request failed:
  ImageSnapshotCorrupt`. The turn's prompt was recorded first, as fx records it, and the model is
  not asked. A copy that matches goes with the bytes just read, so a repaired copy is sent again.
- `ask` names the error as fx's `ask` does: `xfx: ImageSnapshotCorrupt`, or with `--json` the name
  (`CopyErrorNamed`, `src/app.rs`).
- Tool images are read when a turn's request is built.
- The line shell's check goes. It continues no saved session, so its history holds no copy from
  disk. That settles the review's "move it or delete it": deleted.
- `unsendable`, `Loss::request_error` (and its guessed `Unexpected`) and `Snapshot::Unusable` are
  gone.
- `docs/parity.md`'s `/image` and `session resume` rows say all of it, with the rollback.
- Source outside tests, this round: +334 / -209, net +125. The unit from 714f6283: +486 / -242,
  net +244 (`size.txt`).

## The captures (`fx/`, `xfx/`, `base-xfx/`, `green.txt`, `logs.txt`, `wire.txt`)

**Round 1's pair proved nothing about the blob, and is replaced.** Round 2's positive control
showed that fx v0.0.16 refused round 1's seeds -- and the unchanged `p5-effort-fork-v2` source and
fork -- as unreadable whatever their images. fx had written them with a `lock` beside the session
and an `index.lock`, and the seeds had neither. A session fx made itself (`/image shot.png`, a
prompt) showed the layout: the two locks, and an image item that lists its blob in `blobs`. The
seeds now carry both (`f290f697`; `seed_fx.py` reseals each line's CRC32C).

Three scenarios, each on fx (sha256 `8e6cda2e…`, guarded by `capture.sh`), on the head
(`x𝒇x v0.1.0-f290f69`) and on the base (`x𝒇x v0.1.0-714f628`), at 40x120:
- **`p5-image-loss-whole`** (the control: the copy whole). fx resumes the session. So do the head
  and the base.
- **`p5-image-loss-gone`** (the copy not there). fx refuses (`fx: saved session is unreadable. …`)
  and appends `workspace` and `closed` to the log (6039 bytes from 5749; `logs.txt`). The head
  refuses (`xfx: saved session is unreadable.`) and leaves both files at their seeded sizes. The
  base resumes, and its log grows.
  - fx vs head: 3 rows differ. fx takes the terminal at the relaunch and gives it back before it
    writes the line (`wire.txt`), so its line is on row 3 and xfx's, written before the terminal
    is taken, on row 2. The line drops the two pointers. Row 42 is tmux's dead-pane clock.
  - `quit`: the clock alone.
- **`p5-image-loss-changed`** (the copy there, one byte changed). fx resumes, then the next prompt
  fails: its card, `0s (↑4 ↓0)`, `✗ system: request failed: ImageSnapshotCorrupt`. The log gains
  the turn (`logs.txt`). The head resumes, the prompt fails with the same `✗` line, and its log
  gains the prompt and an `interrupted` conclusion. The base resumes, prints N1 and sends the
  request without the image.
- **Not identical, and not this unit's:**
  - the seeds' histories differ: fx's has three turns with recorded usage, xfx's one, so the
    resumed rows and the turn summaries differ (11-14 rows in `green.txt`);
  - fx paints the failed turn's summary `0s (↑4 ↓0)` above the `✗` line. xfx's TUI paints no
    summary under any failed turn: the `(Some(failure), _)` arm of `TurnEnded`, `src/tui/shell.rs`,
    a rule of its own that this unit does not change.

## RED and GREEN

- **Round 1** (`r1-red.txt`): at `f61bcf1a`, 11 of 12 new or flipped tests fail on the base.
- **Round 2** (`red2.txt`): at `3a17da54`, 5 tests fail on round 1's code:
  - a copy changed before the resume and put back stays unsendable (read once);
  - a copy past the limit, cut short or unreadable fails the request (`ImageTooLarge`,
    `ImageSnapshotCorrupt`, `AccessDenied`) where fx holds it back;
  - one never read fails with `MissingImageSnapshot`;
  - an unsearchable images folder lets the resume go on;
  - `ask` does not record the failed turn.

  The two store tests round 1 dropped by mistake come back and pass: tool images sent again after a
  resume, and the reducer's tool-image invariants (orphaned, failed result, wrong call,
  duplicate). The launch test's snapshot is now recursive, with modes and a nested sentinel.
- **GREEN, round 2:** at `f42a68ab`, the lib (4622 passed), `--test sessions` (67), `--test parity`
  (26), `--test tool_loop` (109) and the image and resume pty tests (21) pass, as do fmt and both
  clippy runs, each run again with the crate roots touched so every target is checked
  (`checks.txt`). The full gate is the dispatcher's.

## Data safety, scoped

**The image refusals this unit adds come before any write.** `resume_within` returns
`Unreadable` after the replay and before the log's tail is cut or a `workspace_rebound` is
written. Four things pin it:
- the store test hashes every file under the session (path, mode, bytes) before and after, for a
  removed copy, a link and a directory, through both entry points, with an unpublished tail on the
  log; and for an unsearchable folder (restored before the second hash);
- `ask --resume` and the launch's pty test compare the session's files before and after,
  recursively, with modes and a nested sentinel;
- the `gone` captures leave xfx's files at their seeded sizes;
- `refusal-after-truncate` is killed in all three targets (`r1-mutants.txt`, `mutants-r2.txt`).

It is not a guarantee for every failed resume. Two later paths predate this unit and stay outside
it:
- `set_len` succeeds and `sync_data` then fails: the resume errors with the tail already cut;
- a cross-workspace resume appends `workspace_rebound`, and the publish then fails.

xfx's image refusal is stricter than fx's refused launch, which writes two lines (`logs.txt`).

A request a changed copy fails writes the turn's prompt and its conclusion, as fx records the
turn. It writes nothing else: no image, and the copy is untouched (`xfx/…-changed-…-failed.files`:
73 bytes; the TUI and `ask` tests check the recorded prompt).

## Mutants (`mutants-r2.txt`, the runner `mutants.py`)

On a `git clone --no-local` copy, never on the worktree, each run in one suite-lock slot, 900 s per
test target, the clone reset after each. `mutants-r2.txt` holds three runs: all 13 mutants at
`f290f697`; the request mutants again at `f42a68ab`, after the test that `read-at-resume` survived
in the TUI gained a second resume; and `oversized-without-detail` at `f42a68ab`. Every run ends
with 0 changed files in the clone.

| mutant | what it breaks | lib | sessions | tui |
|---|---|---|---|---|
| `notice-kept-tui` | a notice line kept on the TUI's screen | killed | | |
| `notice-kept-ask` | a notice line kept on `ask`'s stderr | | killed | |
| `resume-goes-on` | the resume goes on past a lost prompt image | killed | killed | killed |
| `refusal-after-truncate` | the refusal comes after the log's tail is cut | killed | killed | killed |
| `stat-failure-resumed` (brief) | a `stat` that fails resumes the session | killed | | |
| `read-at-resume` (brief) | the read moved back to the resume: a copy whole then is held in memory | killed (both the store test and, at `f42a68ab`, the TUI's) | SURVIVED | |
| `oversized-fails` | a copy past the limit fails the request where upstream holds it back | killed | | |
| `oversized-without-detail` (brief: ImageTooLarge without its detail) | held back with the no-dimensions note in place of the type, pixels and limits | killed | | |
| `request-goes-on` | a changed copy goes | killed | killed | |
| `request-goes-on-tui` | the TUI's turn has no copies to check | killed | | |
| `ask-name-not-copied` | `ask` prints the TUI's words, not `ImageSnapshotCorrupt` | | killed | |
| `tool-note-missing` | the tool image's note not put in front | killed | killed | |
| `tool-note-changed` | another error's name in the note | killed | killed | |
| `tool-images-kept` | the note said, and the result's other images still sent | killed | SURVIVED | |

The two survivors are tests that cannot tell, not untested rules:
- `read-at-resume [sessions]`: `ask --resume` runs one turn in one process, so its resume and its
  request read the copy moments apart, with nothing to change it between them. The TUI's test
  resumes, changes the copy between turns and resumes again; it kills the mutant.
- `tool-images-kept [sessions]`: that test's result carries one image, so there is no other image
  to send. The store test's result carries two; it kills the mutant.

There is no ImageTooLarge text to mutate: upstream's build never reaches it (above), and the
detail it would have carried is in upstream's note, which `oversized-without-detail` removes.

## Sweep (`sweep-base.md`, `sweep-head.md`, `sweep-compare.txt`)

The 19 scenarios that resume a session or carry an image (`--only resume* image* undo-resume
p5-lifecycle-relhome-image p5-lifecycle-relhome-resume p5-lifecycle-v1resume`), at 40x120,
`--jobs 4`, each sweep in one suite-lock slot. The head is the binary built at `f290f697`: the
commit after it changes a test only.
- **The base (`714f6283`) and the head** each pass 86 of 110 pairs; none drops (`cmp_sweeps.py`).
  No pair's unattributed rows or named causes move (`pair_rows.py`).
- **No image-loss line or note** appears in either sweep's 608 files except upstream's own
  held-back note in `image-withheld`'s request bodies, the same on both sides. The same pattern
  finds 9 files in this receipt's captures (the positive control).
- The two ledgers differ only by round 1's six line refs, `src/tui/shell.rs:39046` to `:39041`
  (`sweep-compare.txt`).

## The rollback (`rollback.txt`)

Recorded by the commit after this folder's first round-2 commit: the exact set, checked on a
throwaway clone at this one (`rollback.txt`).

## Open

- **A turn's later requests** use the bytes its first one read. Each request reads the copy again
  and fails when its bytes changed. A copy that disappears between two requests of one turn still
  goes as the turn read it, where fx's next request would hold it back. Tool images are read once a
  turn, where fx reads them for every request.
- **The failed turn's summary row** (above): xfx's TUI rule, not this unit's.
- **The non-native route** (a recorded exception): fx loads stored tool images only on the native
  route. On a Gateway model the catalog does not tag for images, fx says
  `[Tool images were retained but not sent: …]` for such a result (`gateway_step.zig:256-283`), and
  xfx says the loss note. Copying it means the history build knowing the route, which
  `ToolImageGate` decides per request: not small, so recorded on the `/image` row.
- **A failed publish right after `ImagesAttached`** leaves the live prompt image `Unread`, a
  recorder failure reported after the turn. Later turns hold it back with the no-file note, where
  fx sends the bytes it holds in memory.
- **fx's refused launch writes** `workspace` and `closed`. xfx writes nothing, as this unit's brief
  requires.
