# image-loss-copy-fx: a lost history image is fx's -- a refused resume, a held-back image, a failed request, a note to the model

HISTORY-NOTICES-COPY-FX (714f6283, this unit's base) kept xfx's four image read-back notices, N1,
N2, N5 and N6, and queued this unit: fx v0.0.16 (pin `481104fd`) has no such line, and it does not
go on quietly either. This unit copies what fx does in each case, deletes the four lines and the
once-per-conversation ledger that said them, and records the harder failure as a decision for the
user's veto. Round 2 acts on the four reviews of `dba6007b`, round 3 on the four of `d079b843`,
round 4 on the three of `9131b35b`.
Every claim is tied to a file in this folder. Every fx line cited is in `fx-src.txt`, printed by `fx_src.sh`, and
`fx_src.sh <fx> --check README.md` finds each citation inside a printed range, as it does each
one this unit added to `docs/parity.md`, the code and the scenarios (`new_cites.py`, `checks.txt`).

| commit | what |
|---|---|
| `f61bcf1a` | round 1 RED, tests only (`r1-red.txt`) |
| `0f0b6789` | round 1 fix, its docs, the ledger refs |
| `67a80e48` | round 1 scenario `p5-image-loss-gone` and its seeds |
| `dba6007b` | round 1 receipt |
| `3a17da54` | round 2 RED, tests only (`red2.txt`) |
| `2336c989` | round 2 fix and `docs/parity.md` |
| `f290f697` | round 2 scenarios: the seeds corrected, `p5-image-loss-whole` and `-changed`; the binary of every head capture and of the head sweep (`binaries.txt`) |
| `f42a68ab` | round 2 test: the TUI's per-request test resumes a second time, so the read moved back to the resume fails it (`mutants-r2.txt`) |
| `b2f95c55` | round 2 receipt: this folder |
| `d079b843` | round 2: the rollback check (`rollback.txt`, `rollback.sh` keeping every log) |
| `de9a1b93` | round 3 RED, tests only (`red3.txt`) |
| `20a69fc2` | round 3 fix and `docs/parity.md`: **the code head** |
| `f273880b` | round 3 scenario `p5-image-loss-folder`, and the capture verb `homemode`, which refuses a link on its path, with its test |
| `9f4bf2cf` | round 3 test: `ask`'s unreadable-folder case puts the folder's own mode back |
| `9131b35b` | round 3 receipt |
| `44a6532a` | round 4 RED, tests and a test-only seam between a copy's open and its read (`red4.txt`) |
| `023430f4` | round 4 fix, `homemode` a folder only, and `docs/parity.md`: **the code head** |
| this README's commit, the unit's last | round 4 receipt |

## fx's rule, case by case (`fx-src.txt`, and the captures of rounds 2 and 3)

| xfx's case | what fx does at the pin | what the user sees | the log, the model |
|---|---|---|---|
| a prompt image's copy **not there, or not a regular file** | the reader opens the blob folder and `stat`s each blob as it reads the conversation, without following a link; not there, or anything but a regular file, is `InvalidSessionFormat`, and the session does not open (`session_adapter.zig:2871-2893`, `session_adapter.zig:4320-4327`; `api.zig:433-442`; `session.zig:1666-1681`; `storage.zig:341-346`) | launch: `fx: saved session is unreadable. Run \`fx doctor\`; if it is recoverable, use \`fx session recover <id>\`.`, exit 1, once it has given the terminal back (`app_entry_runtime.zig:278-285`; `fx/`, `wire.txt`); picker: `Unable to resume this session.` (`app_input_runtime.zig:91-99`, `app_input_runtime.zig:1644-1662`, `resume_menu_presentation.zig:463-467`); `ask --resume`: `fx: InvalidSessionFormat`, or the name as `--json`'s error (`cli_ask.zig:926-936`, `cli_ask.zig:1189-1197`, `cli_ask.zig:1306-1326`, `cli_ask.zig:8063-8079`, `app_entry_runtime.zig:158-166`, `app_entry_runtime.zig:563-567`) | fx's refused launch appends a `workspace` line and a `closed` line (`logs.txt`) |
| a prompt image's copy **in a folder that cannot be opened for reading, or is a link, or whose `stat` fails otherwise** | the folder is opened for reading without following a link (`storage.zig:135-143`); a failure other than NotFound is `Refused` or `Io` (`storage.zig:400-417`), which `notFoundOr` makes `Io` (`session.zig:1666-1681`), and `imageBlobFailed` hands `Io` up unchanged (`session_adapter.zig:2871-2893`) | launch: `fx: Io`, exit 1, once it has given the terminal back: no storage fault has a sentence for it (`app_entry_runtime.zig:286-301`, `app_entry_runtime.zig:537-556`; `fx/fx-p5-image-loss-folder-40x120-refused.full.txt`); picker: `Unable to resume this session.`, as above; `ask --resume`: `fx: Io`, or the name as `--json`'s error, by the same paths as above | the same two lines as above (`logs.txt`) |
| a resumed copy that is a file: **unopenable, no size in its header, or past 5 MiB encoded** (so past the 20 MiB limit too) -- at a turn's first request, or any later one | before every build, on the native route, an image whose file it cannot vouch for -- its size from `stat` on each request, its dimensions from its header as the turn first read it -- is left out with a note, and the request goes (`image_attachments.zig:805-813`, `image_attachments.zig:814-850`, `image_attachments.zig:960-988`, `image_attachments.zig:1011-1045`, `image_attachments.zig:1050-1054`; `orchestrator.zig:7612-7632`). The request budget does not hold one back alone: 30 MiB, and 20 MiB is 28 MiB encoded (`request_image_budget.zig:1-22`, `request_image_budget.zig:76-92`). So the build's `ImageTooLarge` (`image_attachments.zig:24`, which `app_process_runtime.zig:99-136` would show as `request failed: image exceeds the 20 MiB limit (ImageTooLarge)`) is not reached | nothing on screen | the model gets `[Image #N not sent: its dimensions could not be verified. The original is saved at <blob>. …]` or `[Image #N not sent: <type> is <W>x<H> pixels. …]` in front of the prompt |
| a resumed copy that is a file with a size and a header, **its bytes not the digest's** (or past the limit, or unreadable, as it is read) | the build reads it to its end for every request, sends only the bytes it has just hashed, and fails with a `try`: `ImageSnapshotCorrupt`, `ImageTooLarge` past 20 MiB, or the reader's error (`image_attachments.zig:1260-1320`, `image_attachments.zig:1861-1877`, `vercel_protocol.zig:873-890`, `orchestrator.zig:7712-7716`), after the image route's own refusal (`orchestrator.zig:7520-7525`) | TUI: the prompt's card, `0s (↑4 ↓0)`, `✗ system: request failed: ImageSnapshotCorrupt` (`app_process_runtime.zig:62-74`, `app_process_runtime.zig:99-136`; `fx/fx-p5-image-loss-changed-40x120-failed.txt`); `ask`: `fx: ImageSnapshotCorrupt` (`cli_ask.zig:1802-1823`) | the turn is recorded -- `turn_started`, the prompt, a `turn_end`, `turn_committed` (`logs.txt`); nothing is sent |
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
- Source outside tests, this round: +334 / -209, net +125 (`size.txt`).

**Round 3 (`20a69fc2`).**
- **No byte the history read is ever sent** (`CopiesChecked`). Round 2's wrapper sent the turn's
  bytes when its own read failed, and those bytes had never been held to the digest. Now every
  request opens each copy again, as fx's projection and build do for every request:
  - it cannot be opened, or its size is past 5 MiB encoded: it is left out of that request with
    fx's note, its pixel size as the turn first read it, and the request goes;
  - otherwise it is read whole and held to its size and digest: the bytes just checked are sent,
    and any other bytes fail the request, `ImageSnapshotCorrupt`;
  - a read that fails after the open fails it too, `ReadFailed`. That is the name fx gives a
    failed read elsewhere: its log says `err=ReadFailed` for one
    (`.prd/tui-phase4/receipts/w9/README.md:104`), and xfx's `read_file` already copies it
    (`src/tools/read.rs:1026-1037`). That `readSliceShort` in `loadVerifiedSnapshot` fails with
    that name comes from Zig's standard library, not from fx's tree, and is not read here [추정].
  So a copy changed and then removed sends nothing, a repair is sent by the next request of the
  same turn, and one gone or grown between two requests of a turn is held back from the later one.
  **A decision against the brief's wording, from the pin:** the brief says such a request fails
  (`MissingImageSnapshot`, `Io`). At the pin a copy fx cannot open never reaches the build that
  would fail: `withholdOversizedAttachments` runs before every request
  (`orchestrator.zig:7612-7632`), and `attachmentRawBytes` is null when the open fails
  (`image_attachments.zig:805-850`), so the image is held back with the note and the request
  goes. `MissingImageSnapshot` is the build's error only for an image with no file at all
  (`image_attachments.zig:1260-1320`), which the history never sends. Nothing of the image is
  sent either way.
- **The history reads a copy only as far as the hold-back needs** (`turn_messages`): its size from
  the file's metadata and at most one byte past what one request carries, 3.75 MiB, for the header.
  fx reads the header by positioned reads and the size by `stat`; xfx's header reader
  (`crate::image::dimensions`) takes one contiguous prefix, so the read is capped where any copy
  is too large to send rather than at a header length. What it read decides the first hold-back
  only, and is never sent.
- **A failed stat refuses with fx's `Io`** (`read_snapshots`). The images folder is opened for
  reading without following a link, as fx's `openDir` opens it, and each copy is `stat`ed in it
  without following one. Not there, or not a regular file, is `InvalidSessionFormat`; a folder that
  cannot be opened (no read permission, search only, a link) or a `stat` that fails otherwise is
  `Io`. The launch prints `xfx: Io`, `ask` `xfx: Io` or the name with `--json`, and the picker its
  one row for any failure but a busy session (`src/tui/worker.rs:2459-2460`).
  `SessionError::Unreadable` carries the name.
- `read_snapshot` judges the size from the file's metadata before it reads, as round 1 did.
- `ask` names an image error through the one helper that names a refusal (`named`, `src/app.rs`).
- `turn_messages` without copies (the compaction) carries no image at all, where a live image went
  before.
- Source outside tests, this round: +222 / -139, net +83 (`size.txt`).

**Round 4 (`023430f4`).**
- **A copy is read to its end** (`CopiesChecked`), as fx's `loadVerifiedSnapshot` reads it, no
  further than one byte past the 20 MiB limit. Round 3 read only the size the open found, so a
  copy appended to between the open and the read was sent as its checked prefix. Now a byte more
  is not the digest's, `ImageSnapshotCorrupt`, and past the limit is fx's `ImageTooLarge`, shown as
  fx's build shows it, `request failed: image exceeds the 20 MiB limit (ImageTooLarge)`
  (`app_process_runtime.zig:99-136`, `image_attachments.zig:24`; `xfx: ImageTooLarge` from `ask`).
  The size the open found is compared with the recorded one before any byte is read.
- **The history's bytes are taken out of the part before anything is decided:** the note's pixel
  size comes from them first, then the part holds nothing until the bytes just checked go in. An
  arm that neither checks nor fails sends no image bytes at all (`read-failure-goes-on`, below).
- **`homemode` takes a folder only:** a file may be a hard link to one outside the run, whose
  inode a chmod would change. A run that dies before the line that sets the mode back leaves it in
  the run directory, which `cleanup` keeps anyway (its comment says so).
- `ask` names a copy's failure from the value `CopiesChecked` failed with (a shared cell), not by
  matching the error's text.
- The request is cloned only when it carries an image from a copy.
- `turn_messages`'s unreachable arm is gone.
- Tests: a test-only seam between the open and the read grows the copy (by a byte, and past the
  limit) or swaps in a descriptor that cannot be read; the 5 MiB limit is written out in the
  boundary test; `homemode` refuses a file and a hard link to one outside the run.
- Source outside tests, this round: +83 / -36, net +47, the test-only seam's lines among them
  (`size.py` counts them as source). The unit from 714f6283: +639 / -265, net +374 (`size.txt`).

## The captures (`fx/`, `xfx/`, `base-xfx/`, `green.txt`, `logs.txt`, `wire.txt`)

**Round 1's pair proved nothing about the blob, and is replaced.** Round 2's positive control
showed that fx v0.0.16 refused round 1's seeds -- and the unchanged `p5-effort-fork-v2` source and
fork -- as unreadable whatever their images. fx had written them with a `lock` beside the session
and an `index.lock`, and the seeds had neither. A session fx made itself (`/image shot.png`, a
prompt) showed the layout: the two locks, and an image item that lists its blob in `blobs`. The
seeds now carry both (`f290f697`; `seed_fx.py` reseals each line's CRC32C).

Four scenarios, each on fx (sha256 `8e6cda2e…`, guarded by `capture.sh`), on the head and on the
base (`x𝒇x v0.1.0-714f628`), at 40x120. The head is `x𝒇x v0.1.0-f273880` for every xfx capture
(round 3 took them all again); round 2's base captures stand, and the base was captured on the new
scenario too:
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
- **`p5-image-loss-folder`** (round 3: the copy whole, its folder at mode 000 at the resume, set
  back before `home after`). fx prints `fx: Io` once it has given the terminal back, exits 1, and
  appends `workspace` and `closed` to the log (6039 bytes from 5826). The head refuses, `xfx: Io`,
  exit 1, before it takes the terminal, and leaves the session's files at their seeded sizes
  (`logs.txt`: 0 lines appended). fx vs head: 3 rows differ, as in `gone`: fx's line is on row 3
  and xfx's on row 2, and row 42 is the clock. The base resumes the session although its images
  folder cannot be read, and its log gains a `workspace_rebound` line.
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
- **Round 3** (`red3.txt`): `de9a1b93`'s tests on round 2's code; 5 fail:
  - a copy changed at the build and then removed sends its 73 unchecked bytes;
  - in one turn, a copy removed between two requests is sent again by the second, and one grown
    past the limit fails it where fx holds it back;
  - the history reads 20 MiB and one byte of a copy too large to send, not 3.75 MiB and one;
  - a folder that cannot be read refuses with `saved session is unreadable.`, and `ask` with
    `InvalidSessionFormat`, not `Io`.

  The rest pass on round 2's code too: the boundary test, and `ask`'s log test, which now also
  checks every line and file the failed request writes.
- **GREEN, round 3** (`r3-checks.txt`), under the a-z TMPDIR, one suite-lock slot at `f273880b`:
  fmt, both clippy runs, the lib (4625 passed), `--test parity` (26), `--test tool_loop` (109),
  the image and resume pty tests (21), and the capture harness's `homemode` test pass.
  `--test sessions` failed 1 of 67 there: `ask`'s new unreadable-folder case put the folder back
  at 0700 where its helper had made it 0755, so its own before-and-after comparison differed by
  that mode alone (the session's files and bytes were the same). `9f4bf2cf` puts the folder's own
  mode back, and `--test sessions` passes, 67 of 67. Both clippy runs also passed in a slot of
  their own just before, at the same code. The full gate is the dispatcher's.
- **Round 4** (`red4.txt`): `44a6532a`'s tests on round 3's code, 2 fail:
  - a copy that grows by a byte between the open and the read is sent as its 73-byte prefix
    where fx fails it, `ImageSnapshotCorrupt` (the store test's first failing case);
  - `homemode` changes the mode of a file and of a hard link to one outside the run.

  The boundary test passes on round 3's code too: its 5 MiB is now written out.
- **GREEN, round 4** (`checks.txt`), under the a-z TMPDIR, one suite-lock slot at `023430f4`:
  fmt, both clippy runs, `--test sessions` (67), `--test parity` (26), `--test tool_loop`
  (109), the image and resume pty tests (21) and `test-capture.sh` (80 tests) pass. The lib ran
  4623 of 4625: two TUI tests whose stop must end a turn between 300 ms and 1 s missed that window
  (`a_stop_in_the_catalog_wait_of_a_turn_that_would_ask_twenty_questions_ends_it_as_the_stop`,
  `a_stop_in_a_turns_catalog_wait_ends_it_as_a_stop_and_records_it_as_one`). The load was 63, from
  jobs outside the suite lock. Their sessions have no copy, so `CopiesChecked` returns at its
  first check. Run alone five times at the same head, under a load of 49 to 54, both pass 5 of 5
  (`flake4.txt`), and they passed at round 3's head. The full gate is the dispatcher's.
- **GREEN, round 2:** at `f42a68ab`, the lib (4622 passed), `--test sessions` (67), `--test parity`
  (26), `--test tool_loop` (109) and the image and resume pty tests (21) pass, as do fmt and both
  clippy runs, each run again with the crate roots touched so every target is checked
  (`r2-checks.txt`). The full gate is the dispatcher's.

## Data safety, scoped

**The image refusals this unit adds come before any write.** `resume_within` returns
`Unreadable` after the replay and before the log's tail is cut or a `workspace_rebound` is
written. Four things pin it:
- the store test hashes every file under the session (path, mode, bytes) before and after, for a
  removed copy, a link and a directory, through both entry points, with an unpublished tail on the
  log; and for a folder at modes 000, 100 and 400 (each restored before the second hash);
- `ask --resume` and the launch's pty test compare the session's files before and after,
  recursively, with modes and a nested sentinel;
- the `gone` captures leave xfx's files at their seeded sizes;
- `ask --resume` does so for the folder at mode 000 too (round 3);
- `refusal-after-truncate` is killed in all three targets (`r1-mutants.txt`, `mutants-r2.txt`,
  `mutants-r3.txt`).

It is not a guarantee for every failed resume. Two later paths predate this unit and stay outside
it:
- `set_len` succeeds and `sync_data` then fails: the resume errors with the tail already cut;
- a cross-workspace resume appends `workspace_rebound`, and the publish then fails.

xfx's image refusal is stricter than fx's refused launch, which writes two lines (`logs.txt`).

A request a changed copy fails writes the turn, as fx records a failed turn: its project context,
its prompt and its conclusion, and the manifest (`session.json`) the log's publish rewrites. It
writes nothing else: no image, no other file, and the copy is untouched. `ask`'s test checks
exactly those three lines past the published log, and every file of the session but the log and
the manifest (round 3); the TUI's test checks the prompt; `xfx/…-changed-…-failed.files` shows the
copy at 73 bytes. **This supersedes round 1's "a failed request writes nothing to the
log"**: fx records the turn, so xfx does.

## Mutants (`mutants-r4.txt`, `mutants-r3.txt`, `mutants-r2.txt`, the runner `mutants.py`)

**Round 4** (`mutants-r4.txt`): all 23 mutants in one run, on a `git clone --no-local` copy at
`023430f4`, in the GREEN slot, 900 s per test target, 0 changed files in the clone after it. The
runner has a `capture` target now: the harness's own test, by name. Round 3's 19 are moved onto
round 4's code; four are new, the brief's three among them:

| mutant | what it breaks | lib | sessions | tui | capture |
|---|---|---|---|---|---|
| `growth-accepted` (MUST 1) | the read stops at the size the open found: a copy grown since is sent as its prefix | killed | SURVIVED | | |
| `too-large-as-changed` | a copy past the limit as it is read fails as changed, not `ImageTooLarge` | killed | SURVIVED | | |
| `read-failure-goes-on` (S1) | a read that fails lets the request go: with the history's bytes taken first, the part goes with no bytes, and the test sees it | killed | SURVIVED | | |
| `homemode-on-a-file` (MUST 2) | `homemode` changes a file's mode, a hard link's included | | | | killed |

Every one of round 3's 19 has the verdicts it had in round 3. The `--test sessions` survivors are
the same kind as round 3's: `ask`'s test has one request whose copy is changed and stays there, so
it fails whichever rule is broken. The store test kills each of them, through its seam between
the open and the read.

**Round 3** (`mutants-r3.txt`): all 19 mutants in one run, on a `git clone --no-local` copy at
`9f4bf2cf`, in one suite-lock slot, 900 s per test target, the clone reset after each.
Round 2's mutants are moved onto round 3's code; `oversized-fails` is gone with the read it
mutated (the history's read no longer decides a request), and six are new: the brief's two, and
one for each new rule.

| mutant | what it breaks | lib | sessions | tui |
|---|---|---|---|---|
| `notice-kept-tui` | a notice line kept on the TUI's screen | killed | | |
| `notice-kept-ask` | a notice line kept on `ask`'s stderr | | killed | |
| `resume-goes-on` | the resume goes on past a lost prompt image | killed | killed | killed |
| `refusal-after-truncate` | the refusal comes after the log's tail is cut | killed | killed | killed |
| `stat-failure-resumed` | a `stat` that fails resumes the session | killed | | |
| `stat-failure-named-invalid` (S1) | a failed `stat` refused as `InvalidSessionFormat`, not `Io` | killed | killed | |
| `folder-followed` (S1) | the images folder followed when it is a link | killed | | |
| `folder-not-opened` (S1) | the folder not opened for reading, the copy `lstat`ed through its path, as round 2 did | killed | | |
| `read-at-resume` | the read moved back to the resume | killed | SURVIVED | |
| `build-reads-past-a-request` (S2) | the history reads a copy to 20 MiB and one byte | killed | | |
| `oversized-without-detail` | held back with the no-dimensions note in place of the type, pixels and limits | killed | | |
| `request-goes-on` | a changed copy sent with its bytes | killed | killed | |
| `catch-all-sends` (MUST 1) | a copy that cannot be opened again sends what the history read, with no note | killed | SURVIVED | |
| `swap-in-deleted` (MUST 1) | the bytes just checked not sent; the history's go | killed | SURVIVED | |
| `request-goes-on-tui` | the TUI's turn has no copies to check | killed | | |
| `ask-name-not-copied` | `ask` prints the TUI's words, not `ImageSnapshotCorrupt` | | killed | |
| `tool-note-missing` | the tool image's note not put in front | killed | killed | |
| `tool-note-changed` | another error's name in the note | killed | killed | |
| `tool-images-kept` | the note said, and the result's other images still sent | killed | SURVIVED | |

The run (`mutants-r3.txt`) stopped at `ask-name-not-copied`, whose text still named round 2's
`.iter()`: the runner refuses a mutant whose text does not match once, and applied nothing. With
the text corrected, that mutant and the three after it ran in a second slot on the same clone
(the end of `mutants-r3.txt`). The clone had 0 changed files between the runs, and the second
ends with 0.

The survivors are `ask` tests that cannot tell, not untested rules:
- `read-at-resume [sessions]`: `ask --resume` runs one turn, and its resume and its request read
  the copy moments apart (as in round 2). The store test and both TUI tests kill it.
- `catch-all-sends [sessions]` and `swap-in-deleted [sessions]`: `ask`'s test has one request,
  whose copy is changed and stays there, so it fails either way. The store test (a copy changed and
  then removed; repaired, then removed) and the TUI's one-turn test kill both.
- `tool-images-kept [sessions]`: that test's result carries one image (as in round 2). The store
  test's result carries two; it kills the mutant.

**Round 2:**

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

**Not run again in round 3.** Round 3 changes what xfx says or sends only when a resumed image's
folder cannot be read, or a copy changes, vanishes or grows during a session. None of the 19
scenarios does either: their copies stay whole, and the 2 files with a held-back note are the same
on both sides. An image too large to carry is held back with the same note either way. The round 2
sweep below stands. The control (`sweep-compare.txt`, its end): round 2's head frames against round
3's, cell by cell, for `gone`, `whole` and `changed`. 0 rows differ in `whole` and `changed`; in
`gone`, 1 row, tmux's dead-pane clock.

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

## The rollback

**The rule:** revert every commit in `714f6283..<the unit's head>`, newest first, in one
`git revert --no-edit 714f6283..<the unit's head>`. The dispatcher rehearses it on a
`git clone --no-local` copy at the final head and records it in the review folder. The commits,
oldest first:
1. `f61bcf1a` (round 1's RED);
2. `0f0b6789` (round 1's fix);
3. `67a80e48` (scenario);
4. `dba6007b` (round 1's receipt);
5. `3a17da54` (round 2's RED);
6. `2336c989` (round 2's fix);
7. `f290f697` (scenarios);
8. `f42a68ab` (test);
9. `b2f95c55` (round 2's receipt);
10. `d079b843` (round 2's rollback check);
11. `de9a1b93` (round 3's RED);
12. `20a69fc2` (round 3's fix);
13. `f273880b` (scenario and `homemode`);
14. `9f4bf2cf` (test);
15. `9131b35b` (round 3's receipt);
16. `44a6532a` (round 4's RED);
17. `023430f4` (round 4's fix);
18. this README's commit, the last.

`rollback.txt` is round 2's rehearsal, at `b2f95c55`: kept as it was run.

## Open

- **One open, where fx has two:** fx opens a copy for its hold-back and again in
  `loadVerifiedSnapshot`; xfx opens it once and reads the descriptor it checked. A copy removed or
  replaced between the two is held back or failed by fx's second open, and read from the open
  descriptor by xfx. The bytes sent still match the digest; only which of a race's outcomes a
  request sees differs.
- **The order of notes on one message:** a note `CopiesChecked` adds goes in front of one
  `crate::image::withhold` added to the same message, where fx orders notes by image. It needs a
  copy that vanishes or grows between two requests of a turn, on a message with another image
  already held back.
- **Tool images** are read once a turn, where fx reads them for every request; a tool image is
  verified when it is read, so none goes unchecked.
- **The pixel limit of a request with more than 20 images**, for a copy held back by a later
  request: xfx counts the images that request still carries, fx every image before its hold-back.
  They differ only when an earlier hold-back takes the count from past 20 to 20 or fewer.
- **No request-byte budget in xfx** (fx's `withholdImagesPastLimit`), which would leave the oldest
  images out of a request past its byte limit before they are loaded. It predates this unit; owner:
  REQUEST-IMAGE-BUDGET.
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
