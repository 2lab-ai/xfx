# history-notices-copy-fx: the history notices fx does not have go; the image ones are left open

The dispatcher's decision (reversible, the user's copy-fx pattern): xfx's history notices are xfx's
own rule, and where fx says nothing, xfx says nothing. This unit lists every notice the history
rebuild produces, reads what fx v0.0.16 (pin `481104fd`) does in the same situation, deletes the
ones fx does not have, and stops at the ones that guard a data-loss case.

Base: `32afd307` (origin/main). Every claim below is tied to a file in this folder.

| commit | what |
|---|---|
| `aa36330c` | RED, tests only (`red.txt`) |
| `3f96a4b7` | the fix, its docs, the ledger, the scenario headers: the binary of every head capture and of the head sweep (`binaries.txt`) |
| `81ee1733` | the ledger test made to compare one line across conversations (mutant run 1, below) -- **the code head**, gated |
| (the receipt commits) | this folder |

`81ee1733` changes only `mod tests` of `src/tui/worker.rs` (`git diff --stat 3f96a4b7 81ee1733`:
that one file, +12/-2, inside `the_history_notice_ledger_goes_with_its_conversation`), so the
binary built at `3f96a4b7` is the code head's binary. After the code head nothing under `src/`,
`tests/`, `build.rs` or `Cargo.*` changes.

## Every notice the history rebuild produces (base `32afd307`)

`DurableState::history_messages` rebuilds the history for every request and pushes a line into
`ReplayedHistory::notices` in `turn_messages` (`src/session/store.rs`). Three surfaces print them:
the TUI (`src/tui/worker.rs:6249`, once per conversation through `Conversation::unsaid`, shown as
`! session: …`), the line shell (`src/interactive.rs:1914`, `say_unsaid`, once per conversation,
`xfx: …` on stderr) and `xfx ask --resume` (`src/app.rs:442`, every line, no ledger). So a line
deleted at the store is gone from all three.

| # | store.rs | condition | text | fx at the pin (`fx-src.txt`) | decision |
|---|---|---|---|---|---|
| N1 | `:900-904` | a prompt image whose saved copy failed its checks on resume (`Snapshot::Unusable`) | `image #N of this session could not be read back (<reason>), so it was not sent again` | not silent: the session is unreadable (H2) or the request fails (H3) | **kept, open** |
| N2 | `:905-909` | a prompt image never read, or of a type xfx cannot name | `image #N of this session was not read back, so it was not sent again` | as N1 | **kept, open** |
| N3 | `:993-997` | a step's sealed reasoning, same wire, another model (`sealed_by` set and `on_wire` not empty) | `this session recorded reasoning from <model> and is continuing with <model>, so that reasoning was not carried over` | silent (H1) | **deleted** |
| N4 | `:998-1004` | a step's sealed reasoning on another wire (or with no model on record) | `this session recorded reasoning on the <wire> wire and is continuing on <wire>, so that reasoning was not carried over` (ledger cause `P1-carry-notice`) | silent (H1) | **deleted** |
| N5 | `:1033-1036` | an image a tool returned whose saved copy failed its checks | ``an image `<tool>` returned could not be read back (<reason>), so it was not sent again`` | told to the model, not the screen (H4) | **kept, open** |
| N6 | `:1037-1040` | an image a tool returned, never read | ``an image `<tool>` returned was not read back, so it was not sent again`` | as N5 | **kept, open** |

**fx's `[context]` lines are no counterpart of any of them.** The once-a-session line claimed past
both stop checks (`orchestrator.zig:5950`) is the skill catalog's; the mid-turn lines a tool run
pushes (`:11345`, `:12354`, H5) are the tool run's own: a skill call's notice and diagnostic
(`skillPreparationFailure`, `orchestrator.zig:2435-2450`), what a dispatch hands its sink, an MCP
tool's changed definition (`tool_runtime.zig:929`, `:1083`, `:1102`; H6). None speaks of the
history.

**Why N1, N2, N5 and N6 stay (the brief's stop branch).** fx has no line for them, but it is not
silent either. A prompt image's blob that is gone or misnamed makes the session unreadable
(`session_adapter.zig:2876`, `:2881-2893`); damaged bytes fail the request that would send them
(`vercel_protocol.zig:877-887` -> `image_attachments.zig:1869-1875`, a `try`); a tool's stored
image it cannot load is told to the model, `[Stored tool image unavailable: <error>]`, in front of
the result (`execution_memory.zig:1013-1018`). xfx goes on without the copy and says so. Deleting
its line would leave xfx quieter than fx about a copy the session lost: a data-loss warning, not a
UI choice. It is the open question below, recorded on the `/image` row of `docs/parity.md`.

**Why N3 and N4 go.** fx's `projectProviderReplay` drops the state from the request and says
nothing; the Gateway's encoder writes a debug trace (H1). Nothing is lost: xfx's log keeps the
state and a resume back onto the authority that sealed it replays it (`a_dropped_state_is_not_deleted_from_the_log`,
`tests/sessions.rs`, unchanged). The request does not change either: the store still sends the
step's text and calls; the flipped tests' assertions on the request bodies are unchanged and
pass.

## RED (`red.txt`: `aa36330c`, the base plus tests only, in a `git clone --no-local`)

Nine tests fail on the base, each on the deleted line:
- lib: `a_switched_session_goes_to_the_gateway_without_the_thinking_llmux_sealed` (N4 twice) and
  `a_model_change_on_llmux_sends_the_steps_another_model_sealed_as_text_and_calls` (N3 twice);
- `tests/sessions.rs`: the four wire-mismatch tests (N4) and
  `ask_resume_never_adopts_a_model_with_a_control_character_or_prints_one` (`xfx ask --resume`
  printed N4, its hostile wire made inert);
- `tests/tui.rs` (pty): `a_switch_off_llmux_sends_its_text_and_calls_and_leaves_the_signed_thinking_behind`
  and `after_a_model_change_on_llmux_no_block_the_earlier_model_signed_is_sent` (the screen
  said N4, N3).

The six re-based once-per-conversation tests pass on the base (the same file): they pin a
mechanism this unit keeps.

The capture pairs are the same RED on the real binary (below).

## The fix (`3f96a4b7`)

- **`turn_messages`** (`src/session/store.rs`): the 14 lines that pushed N3 and N4 are gone. The
  request is built as before: a state the active provider and model did not seal is left out, and
  the step goes as its text and calls. The docs of `turn_messages` and
  `ReplayedHistory::notices` say what still has a line (an image) and what does not (reasoning).
- **Comments only** elsewhere in `src/`: `Conversation::unsaid` (`src/interactive.rs`; the
  SHOULDs below) and the two `xfx ask` comments on what a notice quotes (`src/app.rs`).
- **`docs/parity.md`**: the `/provider` row says the drop is silent, as fx's; the
  once-per-conversation paragraph moves to the `/image` row, with fx's answer to a lost image and
  the open question; the `session event log` row drops "a one-line notice on stderr naming both
  wires"; the `session resume` row says what `xfx ask --resume`'s notices are now.
- **`.prd/04-providers.md`** §Provenance: the replay table's four "drop with notice" cells are
  "drop", and "A drop is never silent" becomes the fx rule, with the date it changed.
- **`scripts/fxcap/parity-ledger.toml`**: cause `P1-carry-notice` is retired. It was the only cause
  whose rows the deletion ends (`grep -n 'xfx_row.*session:'` found it alone).
- **`provider-carry-back.cap`, `provider-carry-back-stop.cap`**: their headers say neither side
  speaks of the reasoning. Their steps are unchanged.
- Source outside tests: `src/session/store.rs` +11/-20, `src/interactive.rs` +14/-11, `src/app.rs`
  +7/-5 (`git diff --numstat 32afd307 3f96a4b7`): 14 lines of code removed, the rest comments.

The tests (`aa36330c`):
- **Flipped, the RED:** the two pty tests of a switch off llmux and a model change on llmux
  (`tests/tui.rs`), `xfx ask --resume` with a hostile wire (`tests/sessions.rs`), the store's four
  wire-mismatch tests (`tests/sessions.rs`, one renamed `..._dropped_without_a_word_...`) and two
  carry tests (`src/session/store/carry_tests.rs`). Each still asserts the drop itself (no raw
  blocks in the request, the text and calls sent) and now asserts no line about it.
- **Re-based, not deleted:** HISTORY-NOTICE-STOP's once-per-conversation tests drove the
  mechanism with sealed reasoning. They now drive it with N6: `unreadable_tool_images`
  (`src/tui/worker.rs`) commits a turn whose two `read_file` results returned the same image with
  no saved copy, so the rebuild finds the same line twice (and, after a resume reads the copy
  back and finds it gone, N5 twice). The screen test's synthetic notice is N1's text
  (`src/tui/shell.rs`). They pass on the base and on the head (`red.txt`, `gates.txt`).

## The capture pairs (`green.txt`, written by `pairs.py`; `fx/`, `base-xfx/`, `xfx/`)

`provider-carry-back` (snaps `start`, `turn1`, `open`, `switched`, `turn2`, `turn3`) and
`provider-carry-back-stop` (`turn1`, `stopped`, `next`) at 40x120, one `capture.sh` run per
binary. Row 1: fx `𝒇x v0.0.16` (sha256 `8e6cda2e…`, checked before use), the base
`x𝒇x v0.1.0-32afd30`, the head `x𝒇x v0.1.0-3f96a4b`.

- **RED (fx vs base):** the base alone shows `! session: this session recorded reasoning on the
  anthropic_messages wire and is continuing on vercel_gateway, so that` / `  reasoning was not
  carried over` -- `turn2` and `turn3` rows 18-19, `next` rows 22-23. fx shows no such row in any
  snap.
- **GREEN (fx vs head):** no snap of the head has those words. The head differs from the base in
  those rows and the blank above them (the rows below move up), and in `1s`/`0s` on the first
  answer's elapsed row; `start`, and `turn1` and `stopped` of the stop scenario, are identical.
- **Not identical to fx, and not this unit's:** what is left of fx vs head is the switch's own
  divergence, the same rows on the base and on the head (the sweep below finds them unattributed
  on both, row for row): fx's gated sign-in line `* auth: Switched credential to AI_GATEWAY_API_KEY.`, fx's switch waiting and
  being cancelled where xfx's commits (`* provider: Provider preparation cancelled. …` vs
  `▪ Cancelled · …`), the footer's model (`gpt-5.6-sol` on codex vs `model-a`), and the rows that
  shift with them.

## Mutants (`mutants.txt`, `mutants-r2.txt`; the runner `mutants.py`)

Each on a `git clone --no-local` copy, never on the worktree, in one suite-lock slot, 900 s per
test target, the clone reset after each (`mutants.txt` line 2: 0 changed files after the run).
Run 1 was on `3f96a4b7`.

| mutant | what it does | targets: verdict (killed by) |
|---|---|---|
| `reasoning-notice-back` | the deleted lines back, both texts | lib, sessions, tui: killed (the 2 carry tests; all 5 sessions tests; both pty tests) |
| `reasoning-notice-back-wire-only` | only N4's text, for every drop | lib, sessions, tui: killed (as above) |
| `reasoning-notice-back-model-only` | only N3, for a model change on the same wire | lib: killed (the model-change carry test); sessions: SURVIVED; tui: killed (the model-change pty test) |
| `line-shell-marks-after-every-line` | HISTORY-NOTICE-STOP's NTH: the line shell marks only once every line is written | lib: killed, by the per-line test alone (`the_line_shell_leaves_unsaid_only_the_line_it_could_not_write`) |
| `tui-said-when-found`, `tui-said-even-if-refused`, `tui-never-said`, `no-dedup-in-one-rebuild`, `screen-drops-notice-under-stop` | HISTORY-NOTICE-STOP's mechanism mutants, against the re-based tests | lib: killed |
| `ledger-outlives-its-conversation` | one ledger for the process | run 1: killed, but **not by the ledger test** (see below) |

- **`reasoning-notice-back-model-only` survives the sessions target by design:** every
  sessions test is a wire mismatch or a record with no model, which N3 never covers. Two other
  targets kill it.
- **The re-based ledger test was weaker than its original, and is fixed (`81ee1733`).** In run 1,
  `ledger-outlives-its-conversation` was killed by the stop test, the compaction test and
  `the_line_shell_says_each_history_notice_once_and_only_once_it_is_written`, which share the
  mutant's static ledger, and not by `the_history_notice_ledger_goes_with_its_conversation`
  (`mutants.txt`). The live conversation's tool image is never read (N6), and the resumed one
  finds its copy gone (N5): two different lines, so a ledger for the whole process said both too.
  The test now resumes the session once before its first prompt, so both conversations say N5, and
  it checks that the two lines said are one and the same.
- **Run 2 (`mutants-r2.txt`), the seven mechanism mutants on `81ee1733`:** all killed, none
  `build-failed`; `ledger-outlives-its-conversation` is now killed by the ledger test as well.

Ten mutants over the two runs: nine killed in every target they ran, and
`reasoning-notice-back-model-only` in two of its three. None survived everywhere.

## Sweep (`sweep-base.md`, `sweep-head.md`, `sweep-compare.txt`)

The retired cause covered `provider*`. The sweep is every `provider*` scenario, and with them the
ones a history notice could show in: every `resume*` and `image*`, `undo-resume`, and the
`p5-lifecycle` resumes and image (`relhome-image`, `relhome-resume`, `v1resume`). That is 26
scenarios, 154 snap pairs, at 40x120 with `--jobs 4`, each sweep in one lock slot. The head ran
first: the binary built at `3f96a4b7`, with this branch's ledger and scenarios. The base ran
second, from a `git archive` of `32afd307`: its binary, ledger and scenarios. fx's sha was checked
before and after each.

- **Verdicts:** base 95/154, head 96/154, **dropped 0**. One pair changed verdict:
  `resume-paging/page1`, from unresolved to normalized-identical. Its one row is the picker's age
  column (` 1m` vs `now`), under cause `session-age`, kind `nondeterminism` (a clock read about a
  minute after the sessions were made). It is not this unit's.
- **Rows:** over all 154 pairs, `pair_rows.py` compares the rows no cause covers and the causes
  named. **Three pairs moved, as expected:** `provider-carry-back/turn2`, `/turn3` and
  `provider-carry-back-stop/next` named `P1-carry-notice` on the base and name nothing in its
  place on the head, **with the same unattributed rows** (2, 2 and 8: fx's sign-in line and the
  switch's waiting/cancel divergence). No row the cause covered became unattributed. So the
  cause's rows are gone, not moved. The fourth pair in the list is `resume-paging/page1` above.
- **The sweeps' own captures:** the deleted line's words appear in the base's xfx captures of
  `provider-carry-back` (`turn2`, `turn3`) and `provider-carry-back-stop` (`next`), and in no head
  capture (`sweep-compare.txt`, last lines).
- Load was high: 33.4 at the head's start and 39.2 at its peak, and 13.6 and 33.8 on the base.
  Other workers' gates and sweeps shared the machine. No failed or timed-out pair on either side.

## Gate (`gates.txt`, the script `gates.sh`)

`gates.sh` (HISTORY-NOTICE-STOP's, its TMPDIR leaf renamed) ran at `81ee1733`, the code head, in
one suite-lock slot (16:19-16:51 UTC), with a clean tree before and after. It runs every step of
`ci.yml` and the two test scripts of `sweep.yml`. Step 00 (`false`) reads `rc=1`; every other step
reads `rc=0`:

- fmt, clippy, clippy with fault-injection;
- `cargo test --all-targets`: 5538 passed, 0 failed. With fault-injection: the `tui` test, 200
  passed; the lib, 4640 passed;
- check-no-stubs, check-no-secrets, test-check-no-secrets, test-gate-env, check-xfx-identity,
  check-preview-contract, test-preview-contract, check-tui-preflight-cost, check-usage-clock;
- fxcap test-sweep (70 + 27 tests) and test-capture (79 tests);
- both release builds, smoke, and smoke-tui (37 scenarios + the oracle, 1167 checks, 0 failures).

The receipt commits after it add only this folder; the file checks over them (no-secrets,
identity, no-stubs, and the redaction grep) were run again at the receipt's head (`checks.txt`).

## HISTORY-NOTICE-STOP's optional SHOULDs (opus r2)

The once-per-conversation mechanism survives for N1, N2, N5 and N6, so none of the four lapses.
The paragraph that carried them moved from the `/provider` row (whose notice is gone) to the
`/image` row.

- **Name the mid-turn `[context]` lines tool runs push** (`orchestrator.zig:11345`, `:12354`,
  claimed then queued by `app_callbacks.zig:1294-1307`): done in `Conversation::unsaid`'s doc
  (`src/interactive.rs`) and the `/image` row. "A catalog line" there is now "a line claimed
  mid-turn".
- **"A later conversation that sends that history"** in place of "a later conversation of the
  same session": done in the `/image` row (a fork is another session; a reset's conversation has
  nothing to say).
- **A "mark only after every line is written" mutant**: `line-shell-marks-after-every-line`
  (below), killed by the per-line test alone.
- **The queued-then-Ctrl-C gap.** No single test drives a notice the worker has queued and then a
  Ctrl-C through to the screen. The two halves meet at `UiEvent::Notice`: the worker tests count
  what `one_turn` queues (`a_turn_stopped_before_its_request_leaves_the_history_notice_to_the_next_turn`,
  `the_history_notice_ledger_goes_with_its_conversation`), and the shell test feeds that event
  after a Ctrl-C (`a_history_notice_the_screen_takes_is_painted_after_a_stop_and_dropped_only_with_its_conversation`,
  case 1). The shell's and the worker's fixtures are private to their modules, so a test across the
  channel would need one of them moved. That gap is unchanged by this unit.

## Open question for the user

**The image notices N1, N2, N5, N6: keep xfx's line, or copy fx's behaviour?** They warn that a
copy the session saved is gone or damaged, and that the model goes on without it. fx has no line,
but it does not go on quietly either (H2-H4). The choices:

1. **Keep** (now): xfx goes on without the copy and says so once per conversation.
2. **Copy fx**: a prompt image's copy that is gone or damaged refuses the resume (or fails the
   turn); a tool's image it cannot load is told to the model, `[Stored tool image unavailable:
   <error>]`, in front of the result, and nothing is said on screen. This changes what a resume
   and a request do, not only what the screen says.
3. **Delete the line alone**: not recommended. xfx would then be quieter than fx about lost data.

Until it is decided, the `/image` row of `docs/parity.md` records the divergence with its owner.
