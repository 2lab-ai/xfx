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
- Round 2 GREEN `a1fbfed4…`, built at `a38a413c` (banner `v0.1.0-a38a413`).
- L-U3c GREEN `821508c0…`, built at `9f0e4a66` (banner `v0.1.0-9f0e4a6`).
- L-U3d GREEN `c32f85d5…`, built at `d87d448b` (banner `v0.1.0-d87d448`).
- L-U3e first GREEN `f3064c38…`, built at `efcff6bf` (banner `v0.1.0-efcff6b`).
- L-U3e GREEN `c49e8889…`, built at `80cd9aef` (banner `v0.1.0-80cd9ae`).
- L-U3f GREEN `67635b2e…`, built at `fe8dd28e` (banner `v0.1.0-fe8dd28`).
- **GREEN `8cc4a4a3…`, built at `f83d96be` (banner `v0.1.0-f83d96b`), the merge of main `24a6b912`**: every capture now in `green/` and `fx/`, from the sweep that `green.txt` scores.

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

Round 1's sweep (`scripts/fxcap/sweep.sh --only 'p5-lifecycle-relhome*'`, 15 pairs). `green/` and `green.txt` now hold the latest sweep (the merge of main `24a6b912`, 24 pairs); this section is round 1's account.

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

fx: the turn keeps the bytes whether or not the session is saved. "The turn keeps the bytes (D44)" is fx's own doc comment (`imageSnapshotTempDir`, `src/core/session/session_store.zig:44-51@481104fd`). `captureImageAttachment` captures every image into the process's temporary folder, and inlines it into the turn only for a saved session (`src/core/app/app_session_runtime.zig:1332-1345@481104fd`).

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

## L-U3c: the note for a left-out image (opus-b SHOULDs)

- **What fx names.** Only its own copy of an image, never the file it was attached from (`savedImagePath`, `noteAttachmentLocations`, `src/core/images/image_attachments.zig:1050-1062@481104fd`). In an unsaved session, that copy is its temporary snapshot (`captureImageAttachment`, `app_session_runtime.zig:1332-1345`).
- **What xfx did.** In an unsaved session, turn 1 named nothing. Turn 2 and later named the attached file's source path.
- **What xfx does now** (`9f0e4a66`; `6edb0ab9` first wrote a no-copy divergence, which had no allowed reason and is withdrawn).
  - In an unsaved session `Shell::attach` copies an attached file into the private directory the paste copies use. The copy is `0600` in a `0700` directory and is removed with the shell, as fx captures every attachment (`captureImageAttachment`, `app_session_runtime.zig:1332-1345`; `createTempSnapshotDir`, `image_attachments.zig:248-275`).
  - `Image::own_copy` is the one rule for turn 1 (`user_message`) and the history (`Snapshot::Loaded`): a paste's copy, otherwise that temporary copy. So the note names the copy on every turn and never the source.
  - A copy that cannot be written leaves none, and the note names nothing, as a paste's does.
  - The badge, the `attached image: <name>` notice and the files the user named are unchanged; the workspace listings stay byte-identical with fx's (`green.txt`).
- **Tests** (`red-tests-r3.txt`, `red-tests-r4.txt`).
  - RED: `an_unsaved_sessions_image_is_named_the_same_on_every_turn_and_only_by_its_own_copy` fails on `ad670388`'s `src` at turn 1, with `[None, None]`.
  - The same test now expects the attached file's temporary copy on turn 1 and on every later turn, never the source, and no name when there is no copy.
  - `an_unsaved_sessions_attached_file_gets_a_private_temporary_copy`: the copy sits beside the paste copies, is `0600` in `0700`, holds the bytes, leaves the source as it was, and is removed with the shell. A saving session makes no such copy.
  - Mutants that fail: A (no copy at attach), B (`own_copy` ignores the copy), C (`own_copy` names the source); and the round-3 mutant (the history names the source path).
- **Wording fixes.**
  - `links.rs` and `docs/parity.md` now say that a git the terminal runs in another folder reads `<folder>/<HOME>/.gitconfig`, which the walk does not seed.
  - The fx citation for `activeSessionId` is now `:3036`.

## L-U3d: the copy on every route, under a trusted root, never through a link

- **MUST (opus-c).** `0412de84` made the copy only on `/image` (`Shell::attach`). A pasted or dropped path (`paste_paths`) and a path typed into the prompt (`attach_typed`) built `Image::new` and attached with no copy, so the note named nothing there.
  - fx captures on all three routes: `insertImageAtCursor` (`src/core/images/image_commands.zig:140-143@481104fd`), the paste route (`input_paste_runtime.zig:491-494`) and the typed-path route (`captureExtractedImages`, `input_submit_runtime.zig:1219-1224`).
  - Fix: `Shell::captured` is the one step, called from all three routes.
  - RED (`red-tests-r5.txt`): `an_unsaved_pasted_or_dropped_path_makes_the_files_copy` and `an_unsaved_typed_path_makes_the_files_copy_as_the_prompt_is_sent` fail on `0412de84`'s shell with "image #1 has no temporary copy"; `an_unsaved_slash_image_makes_the_files_copy` passes there. Mutant D (the two routes skip `captured`) fails both.
- **A failed copy refuses the attachment**, with `✗ images: failed to attach image: <name>`. (L-U3e corrects the reason: fx passes the write error up rather than refusing with a notice; the notice is xfx's.)
  - `/image` puts no badge in, a pasted path stays words, and a typed path stops the prompt with the draft kept, discarding the copies already made for it.
  - Pinned by `an_unsaved_copy_that_cannot_be_written_refuses_the_attachment_on_every_route`. Mutant E (the base's `.ok()`) fails it.
- **Collision fallback.** A name already there is removed and made again with `create_new`, so a planted symbolic or hard link is unlinked and its target left alone. A failed write removes its file.
  - Pinned by `a_link_planted_at_a_copys_name_is_unlinked_and_its_target_left_alone`; the base's write-and-truncate reopen fails it ("image-2.bin is still a link").
- **Temp root.** The copies' root is fx's, `canonicalize("/tmp")` (`createTempSnapshotDir`, `image_attachments.zig:248-275`), not `$TMPDIR` as given. `private_directory_in` refuses a relative base.
  - Pinned by `the_copies_live_under_the_real_tmp_whatever_tmpdir_says_and_never_under_a_relative_base` and the TUI test `with_no_usable_home_an_attached_files_copy_never_lands_under_a_relative_tmpdir`. That one checks the tree while the session holds the copy, and fails on the base root with "the copy landed in the sandbox".
  - The clipboard read's own scratch directory keeps `$TMPDIR`, now refused when relative. (L-U3e moves it under the same root.)
- **Cleanup** is best effort: on a normal exit or a panic's unwind. A `SIGINT`, `SIGTERM` or `SIGHUP` re-raises without destructors, and the copies stay `0600` in their `0700` directory, as paste copies always have (`docs/parity.md`).
- The stale comments in `machine.rs` and `store.rs` now say the `own_copy` rule.
- **GREEN** (`green.txt`): the same 15 of 17 normalized-identical pairs, the five workspace listings byte-identical, one image per request as fx's.
- A clipboard paste whose copy cannot be written keeps its existing behaviour: the badge with its name and no link. Only the attach routes refuse. (L-U3e refuses it too.)

## L-U3e: the clipboard read's folder and script, a failed clipboard copy, the refusal's words (opus-d SHOULDs)

Code: `efcff6bf`, then `80cd9aef`. RED, GREEN and mutants: `red-tests-r6.txt`, on a throwaway clone.

- **Item 1, the clipboard read.** It made its folder under `$TMPDIR` as given. A relative `TMPDIR` refused the paste (`failed to paste clipboard image: InvalidPath`), and `TMPDIR` went into the AppleScript unescaped.
  - fx makes it with `createTempSnapshotDir` (`loadClipboardImageAttachment`, `image_attachments.zig:1987-2030@481104fd`), under `realpath("/tmp")`.
  - Now: `snapshot_root()` + `private_directory_in`. The `$TMPDIR` helper `private_directory` is gone. The path goes into the script with `\` and `"` escaped; a control character or a non-UTF-8 path is refused as `InvalidPath`.
  - RED: `a_clipboard_read_goes_under_the_real_tmp_whatever_tmpdir_says` (TUI, `TMPDIR` = a folder named `tmp"quoted`, then `relative-tmp`) fails on `73137e00`. The screen there says `failed to paste clipboard image: FileNotFound`, because the `"` ended the script's string. GREEN on `efcff6bf`: the script names `<realpath /tmp>/xfx-clipboard-…/clipboard.png`, nothing `TMPDIR` names is in it or under it, and the read's folder is gone. Mutant M3 (the read under `$TMPDIR` again) fails it.
  - `the_clipboard_reads_path_is_one_string_in_its_script_or_none` pins the escaping and the refusals. It does not compile on `73137e00`, whose `clipboard_script` returned a `String`, so its RED is mutants M1 (no escaping) and M2 (control characters let through).
- **Item 2, the words.** At `481104fd` only `ImageTooLarge` and `ImagePreparationFailed` get a notice and `.rejected` (`captureRejectionNotice`, `captureImageAttachmentForAdmission`, `image_attachments.zig:323-347`); a write error is passed up out of the command (`orelse return err`). The comments in `shell.rs` and `docs/parity.md` now say that fx attaches nothing, and that the notice and "a pasted path stays words" are xfx's, in fx's words (`image_commands.zig:39`).
- **Item 3, a clipboard image whose copy cannot be written.** It got a badge with no link. fx never inserts a temporary image without its snapshot (`image_commands.zig:144-145`, `orelse return error.MissingImageSnapshot`).
  - Now: no badge, no number spent, and `✗ images: failed to paste clipboard image: <name>` (fx's words for a failed paste, `:92`).
  - RED: `a_clipboard_image_whose_copy_cannot_be_written_puts_no_badge_in` (`/paste` and Ctrl-V) fails on `73137e00` with no notice at all (`left: []`). GREEN on `efcff6bf`. Mutant M4 (the base's badge with no link) fails it.
- **Item 4, nits.** `attach`'s doc comment is back on `attach`. `Image::copy`'s doc no longer says a failed copy leaves `None` on an attached image. A typed path with no room for its badge now removes the copy made for it then, not at exit: `an_unsaved_typed_path_with_no_room_for_its_badge_leaves_no_copy` fails on `73137e00` (the copy is still in the directory) and under mutant M5.
  - The same leak was on the three other routes, which the review did not name: a clipboard image, `/image` and a pasted path each made their copy and then, with no room for the badge, left it until exit. fx checks for room before it captures (`image_commands.zig:128-130`) and discards an attachment it does not insert, snapshot and all (`discardImageAttachment`, `:114`).
  - `80cd9aef`: `attach` removes the copy made for an image it could not place, and `paste_paths` removes its images' copies when the composer refuses the paste.
  - `an_image_the_composer_has_no_room_for_leaves_no_copy_on_any_route` fails on `efcff6bf` (the clipboard image's copy is left). Mutants M6 (`attach` keeps it) and M7 (a refused paste keeps its copies) fail it.
- **Citations.** fx line numbers I had counted with `nl -v`, which skips blank lines, are now the pin's real lines: `session_store.zig:44-51`, `app_session_runtime.zig:1332-1345`, `image_attachments.zig:248-275`, `:331-347` and `:349-369`, `image_commands.zig:140-143` and `:39`, `input_paste_runtime.zig:491-494`, `input_submit_runtime.zig:1219-1224`.
- **GREEN** at `80cd9aef` (`green.txt`, `scripts/fxcap/sweep.sh --only 'p5-lifecycle-relhome*' --only image-paste`; the same scores as the sweep at `efcff6bf`): 22 of 24 pairs normalized-identical. The two others are the standing `u02-status` cascade and the `r00-resume` decision. `image-paste`'s 7 pairs, which read the clipboard through the harness's `osascript`, are all normalized-identical. The five workspace listings are byte-identical, and each request carries the same number of images as fx's (1 and 1; 2 for `image-paste`).

## L-U3f: room before the copy, the escaping through the real interpreter, the last stale cites (opus-e, gpt6-e SHOULDs)

Code: `fe8dd28e`. RED, GREEN and mutants: `red-tests-r7.txt`, on a throwaway clone.

- **1. Order (opus-e).** With no room and a copy that cannot be written, the clipboard route said `failed to paste clipboard image: X` and `/image` said `failed to attach image: X`.
  - fx asks `canInsert` before it captures (`insertImageAtCursor`, `image_commands.zig:128-130` before `:140@481104fd`, read with `git show … | sed -n`), and says `input is full; image not attached`.
  - Now both routes ask `Shell::badge_fits` before any copy. With no room they say fx's words and make no copy. `Editor::can_insert` is the one room rule, and `insert` uses it.
  - A pasted path keeps fx's paste order: it captures first (`handlePastedBytes`, `input_paste_runtime.zig:491`) and checks room after (`:554-560`). It still removes its copies (L-U3e).
  - RED: `with_no_room_a_clipboard_image_or_slash_image_says_so_before_any_copy` fails on `8d578f11` with `failed to paste clipboard image: FileNotFound` where it expects `input is full; image not attached`.
  - GREEN on `fe8dd28e`. Mutants M10 (the clipboard copy before the room check) and M11 (`/image` the same) fail it, with the old message on each route.
  - The test also checks that with copies that could be written, neither route makes one (`paste_copies.dir()` stays `None`).
- **2. Real interpreter (gpt6-e).** `applescript_string` is now the one escaper. `a_hostile_path_is_one_string_to_the_real_applescript_interpreter` (macOS only, skipped when `/usr/bin/osascript` is missing) runs the real `osascript`.
  - The path holds `\`, `"`, `\"` and `" & (do shell script "touch <canary>") & "`. It comes back from `return <literal>` exactly as it went in.
  - The clipboard script, with its read replaced by `set pngData to "probe"`, writes exactly that file, and the canary never appears.
  - It does not compile on `8d578f11` (no `applescript_string` there), so its RED is two mutants:
    - M8 (no escaping): the interpreter ran the `do shell script` and returned the path without it.
    - M9 (quotes escaped before backslashes): `osascript` refused the script.
- **3. Stale cites (opus-e).** Re-counted at the pin with `git show 481104fd:<path> | sed -n`.
  - `clipboard_image.rs`'s doc block: `writeImageBadge` `1544-1548`, `insertImageAtCursor` `140-148`. The temporary folder is `imageSnapshotStorageDir` `app_session_runtime.zig:1347-1351` → `session_store.zig:44-51` → `createTempSnapshotDir` `248-275`, and it is removed at `:1274-1277`.
  - That block also said fx captured into a session's `images/`. At the pin every capture goes to the temporary folder, and the block now says so.
  - `image.rs:158` and the `/image` row of `docs/parity.md` now cite `140-148`, and that row's `session_store.zig:102-118` is now `44-51`.
- **GREEN** at `fe8dd28e` (`green.txt`, the same sweep as L-U3e's): the same scores as at `80cd9aef`. 22 of 24 pairs are normalized-identical, `image-paste` is 7/7, the five workspace listings are byte-identical, and each request carries the same number of images as fx's.

## Merged main `24a6b912` for the PR

`f83d96be` merges main `24a6b912` (train 2, PR #76) into this branch.
- **The one conflict.** `docs/parity.md`'s `profile home ~/.xfx` row. Main's `/mcp --project` and verb-error sentences and this branch's refusal wording were merged clause by clause; every fragment of both sides is kept.
- **Moved `file:line` references,** found by mapping each one from the side that wrote it to the merged file.
  - The ledger's `src/tui/shell.rs:12192` is now `12267`, and `:37272` is now `37356` (5 rows): this branch's lines above them moved them, and the code is the same.
  - `docs/parity.md`'s `src/tools/mcp.rs:601-607` is now `422-428`: main shrank `mcp.rs`, and the error arm is the same.
  - No reference this branch's commits wrote moved.
- **Gate at `f83d96be`.** fmt, both clippies, identity, stubs and secrets pass. `cargo test` is 5164/0, the fault-injection suite 5201/0, and `test-sweep` and `test-capture` pass.
- **GREEN** (`green.txt`). The sweep from a build at `f83d96be` scores the same as at `fe8dd28e`, line for line:
  - 22 of 24 pairs are normalized-identical. The other two are the `u02-status` cascade (`status-panel-sandbox`, `-updates`, `-step-limit`) and `r00-resume` (`launch-refusal-row`), causes the train kept.
  - `image-paste` is 7/7.
  - The five workspace listings are byte-identical.
  - Each request carries the same number of images as fx's.

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

The scenarios are `p5-lifecycle-relhome`, `-made`, `-unsaved` (new) and `-resume` (new), with the harness at `83d06be6`. L-U3e's sweep adds `image-paste`, the clipboard read. Every file here went through `scripts/fxcap/redact.py`.
