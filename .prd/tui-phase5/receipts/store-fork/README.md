# P5-FK1 — the session-store fork primitive

Unit 3 of the effort-fork cluster (`research/tui-phase5:.prd/tui-phase5/research/effort-fork.md` §6,
change F5, §4 "Durable shape"), on `plan/tui-phase4` at `e0d97bd5`. Store only: no command reaches it
until `/fork` (FK2).

## What it is

`SessionStore::fork(source, before_turn, workspace) -> WritableSession` (`src/session/store/fork.rs`),
upstream's `Session.fork` → `openFork` (`src/core/session/session_adapter.zig:2436-2457@481104fd`,
`src/core/session_manager/session.zig:1714-1868@481104fd`):

- `before_turn` 0 is refused (`session_adapter.zig:2437`); 1 is a zero-turn session holding what was set
  before the first prompt (`session.zig:1787`, `:1827`); `n` keeps through the frame that **ended** turn
  `n-1` (`findForkPoint`, `:1780-1830`) -- a kept turn with no conclusion is refused; nothing the source
  wrote after that frame is copied.
- Line 1: the source's `session_started` with a new id and `forked_from {id, seq}`
  (`schema.zig:113`, `:136`, `:183-186`), written last and only on a fork.
- Lines 2..S: the source's events **re-framed** -- the fork's generation, fresh event ids, the source's
  `seq` and `timestamp_ms` -- because an xfx frame names its log; then `session_renamed` with the
  source's current title marked ` (fork)` once (`forkTitle`, `:5096-5100`), cut to the 240-byte limit;
  then a `workspace_rebound` when the fork is made in another workspace.
- Images: each one a kept frame names, checked against its digest, hard-linked from the source
  (`linkBlobs`, `session.zig:611-628`, D6) or copied `0600` when the link is refused or is not a private
  file of this account; a missing or damaged one refuses the fork (`session.zig:1803-1809`).
- Built in `~/.xfx/sessions.staged/<id>` (`0700`), log `0600` under the writer's flock, manifest
  published there, then renamed into `sessions/<id>`; a failure removes the stage. The writer unlocks in
  `Drop` like every `WritableSession`.
- The source is read as `detail` reads it -- published boundary, digest, cross-check, no lock -- and never
  written. A hard link does raise its image file's link count and `ctime`, as upstream's D6 links do;
  no byte, `mtime`, mode or name of the source changes.
- The stage is under the profile rather than in `sessions/`, because a listing counts every name there
  that is not a session. If `sessions/` were a mount of its own the rename would fail `EXDEV` and the
  fork would be refused, stage removed. A stage a killed process leaves is inert, and `doctor`'s
  leftover count (one level under `sessions/`) does not see it.

## RED (base `e0d97bd5`)

`cargo test --locked --lib session::store::fork`, with the tests and an empty `fork` module:

```
error[E0599]: no method named `fork` found for struct `store::SessionStore` in the current scope
error[E0599]: no variant named `InvalidForkPoint` found for enum `store::SessionError`
error[E0026]: variant `session::event::SessionEvent::SessionStarted` does not have a field named `forked_from`
error[E0559]: variant `session::event::SessionEvent::SessionStarted` has no field named `forked_from`
error[E0432]: unresolved import `crate::session::event::ForkOrigin`
error[E0432]: unresolved imports `super::carry_image`, `super::FORK_STAGE_DIR`
error: could not compile `xfx` (lib test) due to 9 previous errors
```

## GREEN

| Receipt (brief) | Test |
|---|---|
| replays equal to the source's first N-1 turns | `fork::tests::a_fork_replays_as_the_source_did_before_the_prompt_it_was_made_at` (turns, grants, usage, tokens, mode, roots equal to the source's state at S; a model change after S not kept; each copied frame's `seq`, time and event equal, generation and id the fork's) |
| source bytes unchanged | `forking_never_writes_the_source_even_while_it_is_open` (every file's bytes, mtime and mode, with the source's writer holding its lock; four forks; the writer then appends a turn) |
| a checkpoint in the copied range validates | `a_checkpoint_the_fork_keeps_still_validates` (between turns, and inside the last kept turn; replayed, cross-checked, resumed and appended to; one written after S is not kept) |
| a fork of a fork carries one ` (fork)` | `a_fork_of_a_fork_is_marked_once`; title rules in `a_fork_is_titled_by_the_source_as_it_is_called_now_and_fits_the_title_limit` |
| a fork before turn 1 refused or empty, as fx | `a_fork_before_the_first_prompt_is_empty_and_there_is_no_turn_zero`, `a_turn_that_has_not_ended_cannot_be_kept` |
| an older xfx can still open the source | `tests/session_compat.rs` `forking_a_session_the_preview_wrote_leaves_it_as_the_preview_left_it`; `event::tests::a_fork_origin_is_written_on_a_fork_alone_and_an_older_build_reads_past_it`; the measurement below |
| `0700`/`0600`, links stay inside the store | `a_fork_is_private_and_links_its_images_from_inside_the_store`, `an_image_that_will_not_link_or_is_not_private_is_copied_privately` |
| fail closed, nothing half there | `a_fork_whose_source_image_is_gone_or_not_its_own_is_refused_and_leaves_nothing` (missing, a symlink out of the store, a symlinked folder) |
| flock held, unlocked in `Drop` | `a_fork_holds_its_own_lock_until_it_is_dropped` |
| an origin id that could escape is refused | `a_fork_origin_xfx_would_not_have_written_is_refused` |
| the fork stands without its source | `a_pasted_image_is_named_by_the_forks_own_copy`; `a_fork_belongs_to_the_workspace_it_is_made_in` |

Two mutants, each killed: the stage's removal on failure deleted (2 tests fail); the link's
owner-and-mode check deleted (2 tests fail).

## COMPAT, measured with older binaries

A scratch `HOME` seeded with the five preview sessions of `tests/fixtures/preview-14ad04202fe5`
(workspaces substituted, manifests recomputed), each forked before its last turn by this build's
library, then read by:

- **base** = `plan/tui-phase4` debug build, `status` `build_revision=e0d97bd5c59c`;
- **preview** = the installed `xfx-preview`, `build_revision=14ad04202fe5`.

Every source entry under `sessions/` -- size, SHA-256, `mtime` in ns, mode -- was identical before and
after the five forks; the reads below changed nothing under `HOME`.

| binary | `session --id <source>` | `session --id <fork>` | `sessions --all --json` |
|---|---|---|---|
| base `e0d97bd5` | exit 0 ×5 | exit 0 ×5, `history_turns` 2/1/0/1/1 as kept | 10 rows, `skipped_invalid=0`, 5 titled `… (fork)` |
| preview `14ad0420` | exit 0 ×5 | exit 1 ×5: `unknown variant \`session_renamed\`` | 10 rows, `skipped_invalid=0`, 5 titled `… (fork)` |

The base build reads past `forked_from`. The preview refuses a fork for its title frame -- the
refusal Phase 4's COMPAT recorded for every renamed session (`.prd/tui-phase4/receipts/compat/README.md`,
point 4) -- and still opens every source.

## FK1b — hardening after the security review (`edc64171`, MUST none)

Each test was run first against `edc64171`'s behaviour (for the folder helper, `create_private_dir` stubbed
as `edc64171`'s plain `fs::create_dir`):

| Item | Test | On `edc64171` |
|---|---|---|
| a grant withdrawn after S stays withdrawn | `a_grant_the_source_withdrew_after_the_fork_point_stays_withdrawn` | `left: [terminal ls, write_file /w/a.txt]`, `right: [terminal ls]` |
| the title takes C1 too (`char::is_control`) | `a_forks_title_takes_no_control_character_c1_included` | `left: Some("a\u{9b}2Jb (fork)")`, `right: Some("a 2Jb (fork)")` |
| folders `0700` from creation | `a_forks_folders_are_private_from_creation_under_any_umask` (a copy of the test binary under `umask 0`) | `left: 511` (`0o777`), `right: 448` (`0o700`) |
| a `sessions.staged` that is a link, open, or a file | `a_stage_folder_that_is_not_the_stores_own_is_refused_and_left_alone` | passes: coverage the review found missing; a mutant that skips the stage-root check forks through the link |

A `sessions.staged` another account owns needs root to plant: the test tries `chown` and, refused (uid 501
here), says so and does not count it.
