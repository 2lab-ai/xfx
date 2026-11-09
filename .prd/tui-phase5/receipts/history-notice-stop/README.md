# history-notice-stop: a history notice is said once it is shown, not when it is found

Opus's STOP-IN-CATALOG-WAIT review, SHOULD (a): `Conversation::unsaid`
marked every notice it returned as said, while `bridge::send_ui` refuses a
stopped turn's events. A turn stopped before its request therefore used up
the conversation's once-only notice about what the history could not carry
(sealed reasoning from another wire or model, an image not read back), and
nobody ever saw it. It happened before in the default model's window wait.
Since STOP-IN-CATALOG-WAIT, stopped Ultra and image turns reach it too.
Both engines also asked for a reworded comment at the turn-start catalog
read.

Base: `05a0a857` (STOP-IN-CATALOG-WAIT, closed). Every claim below is tied
to a file in this folder.

| commit | what |
|---|---|
| `350e88b1` | RED, test only (`red.txt`) |
| `b5bf9b8b` | the fix, the comment, `docs/parity.md` -- **the code head** |
| `3899a8ba` | `scripts/fxcap/scenarios/provider-carry-back-stop.cap` and its turns: **the captures' and the sweep's xfx binary** |
| `16cffa83` | the scenario's header says what fx does (its steps unchanged); parity cites the drain's whole drop arm |
| `f3cb88a7` | this folder, but the gate |
| `f16a62ab` | fx's claim-then-drop race cites the push and the drain (`docs/parity.md`, `fx-src.txt` H5, this README): **the gated commit** |
| `ce8dc75c` | `gates.txt` and the gate section (round 1's last) |
| `53ada840` | the dispatcher's: the scratch root in three receipt lines written as `<scratch>` |
| `dec785aa` | round 2: the fx claimers named exactly (`docs/parity.md`, `Conversation::unsaid`'s doc, the RED's comment, the mark site's comment) and three tests -- **round 2's code head and its sweep's xfx binary** |
| (round 2's receipt commits) | this folder: `fx-src.txt` H8, `mutants.py`, the `-r2` files, the round 2 section; then `gates-r2.txt` |

After the code head nothing under `src/`, `build.rs` or `Cargo.*` changes
(`git diff --stat b5bf9b8b f16a62ab -- src build.rs Cargo.toml Cargo.lock`
is empty). Outside this folder, `git diff --stat b5bf9b8b f16a62ab -- .
':!.prd/tui-phase5/receipts'` lists only the scenario's two files (`+65`)
and `docs/parity.md`'s one row (`2 +-`, the citation text). So the
binary built at `3899a8ba` is the code head's code.

## What fx v0.0.16 does (every line quoted in `fx-src.txt`)

1. **fx has no notice about the history.** A step's sealed replay that does
   not match the provider and the model is dropped from the request, and the
   encoder writes a debug trace, not a notice (H1:
   `types.zig:1668-1680`, `builtins/gateway.zig:178-180`). An image blob the
   log names that is gone or damaged makes the session invalid: no notice,
   no turn (H2: `session_adapter.zig:2870-2891`). xfx's two history notices
   are xfx's own rule (`.prd/04-providers.md` §Provenance: "a drop is never
   silent"), already a documented divergence (`docs/parity.md`, the
   `/provider` row; ledger cause `P1-carry-notice`).
2. **fx's lines said once a session are its `[context]` notices.** The
   session keeps a ledger of the ones said (`claimContextNotice`,
   `session.zig:1551-1561`). Round 1 named one claimer and called it the
   only one; that was wrong (opus, round 1). Every call at the pin, and when
   each runs (`fx-src.txt` H8, H3, H4):
   - **A project file's line: at submission, painted in the same step.**
     `enqueuePromptWithSkillBindings` calls `refreshProjectContext` first
     (`main.zig:1193-1200`). That claims each snapshot notice and writes it
     with `writeDomainNotice` (`app_agent_runtime.zig:906-938`), which is the
     shell's own write, with no worker queue (`main.zig:2411-2413`). The job
     carries a copy of the same snapshot (`main.zig:1544-1548`, `:1574`).
     `processQueuedPrompt` claims those lines again before the orchestrator
     (`app_agent_runtime.zig:976-996`, `:1180-1192`) and finds them said.
   - **The skill catalog's line: past the turn's stop checks, then queued.**
     `prepareSkillCatalog` (`orchestrator.zig:5772-5791`) runs at `:5950`,
     after the capability resolve's `Cancelled` (`:5893-5919`) and the
     cancel flag (`:5923-5948`), each of which persists the turn interrupted
     and returns (H4). Its push claims, then queues
     (`app_callbacks.zig:1294-1307`).
   - **The MCP catalog's line: later still, then queued.** The request's
     stable prefix is built at `:6001`. Its static context
     (`:6444-6446`; `app_callbacks.zig:310`, `:879-884`) claims, then
     queues, in `pushMcpModelCatalogNotice` (`app_agent_runtime.zig:845-867`).
   - **Skill diagnostics: at launch and by `/skills`, written in the same
     step** (`app_bootstrap_runtime.zig:292-308`, `app_commands.zig:1083-1098`).
     No turn is involved.
   - Tool executions push theirs mid-turn, after a request
     (`orchestrator.zig:11345`, `:12354`). `cli_ask.zig:3218` and
     `acp/prompt.zig:3222` are `fx ask`'s and ACP's own pushes.
3. **So a turn stopped before its request loses no `[context]` line in fx.**
   A project file's line was painted at submission, before the turn
   existed. The two catalogs' lines are not claimed until the turn is past
   both stop checks, so the next turn claims and shows them.
4. **fx can still lose a catalog line, in a race (code-traced, not
   captured).** The push queues the line as a `.semantic_notice` worker
   event (H5: `pushSemanticNotice`, `app_worker_runtime.zig:649-651`). The
   drain runs every event of an interrupted segment through
   `cancelledEventAdmission` (`:922-950`), which drops it (`:344-354`). A
   stop that lands after a catalog line's claim and before the drain paints
   it loses the line for the rest of the session. That is not "a turn
   stopped before its request", and xfx does not copy it (below).

**Decision: fix, not keep (unchanged by round 2).** The brief's keep branch
is "fx can observably lose the notice too". fx has no history notice to
lose. Its once-a-session lines are all kept by a turn stopped before its
request (item 3), because a project file's is painted at submission and the
catalogs' are claimed later. One is lost only by a stop after its claim
(item 4), which is a race. So xfx now says a history notice once it is
handed to the screen, and `docs/parity.md` says so and names item 4 as not
copied.

## RED (`red.txt`: `350e88b1`, the base plus the test)

`a_turn_stopped_before_its_request_leaves_the_history_notice_to_the_next_turn`
(`src/tui/worker.rs:18033`) puts into the conversation a turn llmux
answered, both of whose steps sealed their reasoning (`sealed_on_llmux`).
On the Gateway the rebuilt history therefore carries the same notice twice;
the helper asserts that before anything runs. It then runs two prompts
through `turn_loop` (STOP-IN-CATALOG-WAIT's `first_of_two_and_a_stop`), with
a Ctrl-C quoting the first, 300 ms into its catalog wait, and without it,
for two waits that reach the replay with the turn's token stopped:

- the default model's window behind a retry (the case before
  STOP-IN-CATALOG-WAIT);
- a prompt image behind the first load (a case since).

It counts, per turn, the `! session: … not carried over` notices the UI was
handed. The base gives `[0, 0]` with the stop in both waits: the notice was
used up and never shown. Expected is `[0, 1]`: the stopped turn shows
nothing, and the next shows it once. Without the stop both give `[1, 0]` on
base and head. Ultra's waits are left out on purpose: their second turn is
refused before the replay (`request_options`), so it reads no notice
either way.

## The fix (`b5bf9b8b`)

- **`Conversation::unsaid`** (`src/interactive.rs:1190`) now only finds the
  unsaid lines, each once and in order: a line the rebuild repeats, one per
  sealed step, is found once, as before. **`Conversation::said`** (`:1200`)
  records a line shown.
- **The TUI** (`src/tui/worker.rs:6214`) records a line once `send_ui`
  hands it to the UI. That is an enqueue, not a paint acknowledgement
  (`src/tui/bridge.rs:1498-1509`, round 2, gpt6). What makes it sound is a
  pair of invariants on the consumer and on the conversation's lifetime:
  - **The screen paints every `Notice` it takes, a stopped turn's too.**
    `UiEvent::Notice` goes to `announce`, then `mark`, with no `cancelling`
    check (`src/tui/shell.rs` `apply_event`). Under `cancelling` only Delta
    and Operational are held back (kept as late text, or dropped), and
    Retrying and Recovered set no row. ToolStart and ToolResult are still
    marked; only their activity row is skipped. A Ctrl-C runs the marks
    still due (`abandon_answer`).
  - **The two paths that drop a taken `Notice` drop its conversation too.**
    A reset drops it: its drain drops one that arrives before the fence
    (`drained`), and its screen clear drops one still owed
    (`reset_session`). The runtime drops the conversation
    (`drop_conversation`). A resume's or a fork's replay clears
    the marks still owed (`hydrate_as`), after the runtime's `switch_to` has
    bound another conversation, whose ledger starts empty. So no ledger
    keeps a line its screen never painted, and a later conversation of the
    same session says it again.
  - Pinned by round 2's two lifecycle tests (below). A refused send (a turn
    stopped before it began, or a UI that is gone) leaves the line for the
    next turn.
- **The line shell** (`say_unsaid`, `src/interactive.rs:1833`) records a
  line once its stderr line is written. The bytes it writes are unchanged:
  the same lines, in the same order, once each. Pinned by
  `the_line_shell_says_each_history_notice_once_and_only_once_it_is_written`
  (`:3945`): found is not said, a write that fails leaves the line unsaid,
  and a written line is not said again. `xfx ask` prints `replay.notices`
  for its one turn (`src/app.rs:439-447`, from `:704-710`), with no
  ledger, so it is not touched.
- **The comment both engines asked for** (`src/tui/worker.rs:6048-6059`).
  A prompt with images reads the catalog at the turn's start on every
  provider, as fx's rule ignores the provider (`orchestrator.zig:5665`,
  `:5885-5886`). On the Gateway that read is the image route's own, moved
  earlier; on llmux the route reads nothing. A settled catalog answers
  without a request (`ModelSelector::ensure_catalog`, `src/provider/model.rs:
  1840-1842`). The comment no longer says image prompts cost no socket.
- **`docs/parity.md`**, the `/provider` row: the notice is said once it is
  handed to the screen, which paints every notice it takes except where a
  reset or a resume drops its conversation too; fx's `[context]` lines are
  kept by a turn stopped before its request (fx item 3, every claimer named
  in round 2); fx's claim-then-queue race (fx item 4) is not copied.
- Source outside tests: 61 lines added and 21 removed (`git diff --numstat
  350e88b1 b5bf9b8b` gives `src/interactive.rs` 88/7 and `src/tui/worker.rs`
  25/14; 52 of interactive.rs's added lines are the line-shell pin), most
  of them comments; far under the 600-line split bar.

## GREEN (`green-unit.txt`)

At the head, the RED passes with both waits `[0, 1]` with the stop and
`[1, 0]` without it. So does the line-shell pin, and both of
STOP-IN-CATALOG-WAIT's tests (4 passed). The pty test that pins "once"
for a switch with no stop
(`a_switch_off_llmux_sends_its_text_and_calls_and_leaves_the_signed_thinking_behind`,
`tests/tui.rs`) also passes. The capture pair is in the next sections.

## Mutants (`mutants.txt`, the runner `mutants.py`)

Six mutants, each on a `git clone --no-local` copy of `3899a8ba`, under the
lock, 600 s each, the clone reset after each. All six are killed:

| mutant | what it does | killed by |
|---|---|---|
| `tui-said-when-found` | the base's rule: said before the send | the RED |
| `tui-said-even-if-refused` | said after the send, whatever it returned | the RED |
| `tui-never-said` | never said: the notice repeats every turn | the RED (`[1, 1]`) |
| `no-dedup-in-one-rebuild` | a repeated line is found twice | both tests |
| `line-shell-never-said` | the line shell records nothing | the line-shell pin |
| `line-shell-said-before-written` | the line shell records before writing | the line-shell pin |

## The captures (`green.txt`; `fx/`, `xfx/`, `base-xfx/`)

`provider-carry-back-stop` is `provider-carry-back` with the first prompt
after `/provider` stopped. The Gateway's catalog answers 10 s after it is
asked (`modelsdelay 10000`), and Ctrl-C lands 0.7 s after the prompt. The
next prompt comes once the catalog is in. Snaps: `turn1`, `stopped`,
`next`. Binaries (`green.txt` header): fx v0.0.16 `8e6cda2efddb…`; xfx built
at `3899a8ba` (row 1 `x𝒇x v0.1.0-3899a8b [debug]`, `8af7b8ecfafc…`); the base
built at `05a0a857` (row 1 `v0.1.0-05a0a85`, `7ab7a35b0420…`).

- **xfx, base and head.** The switch commits at once (`* provider: Switched
  to Vercel AI Gateway with fake/model-a.`). The prompt is a turn waiting
  at its start for the Gateway's first load, and the stop ends it there:
  `▪ Cancelled · What can xfx do differently?`. The gateway logs show one
  completion after the switch, the next prompt's
  (`completion /v4/ai/language-model -> text`). So the stopped turn sent
  nothing on both. `turn1` and `stopped` are identical, base vs head, rows
  2 on.
- **`next`, base vs head:** the head alone shows, under `┃ and after the
  stop?`, `! session: this session recorded reasoning on the
  anthropic_messages wire and is continuing on vercel_gateway, so that` /
  `  reasoning was not carried over`, then the answer
  `MARKER-AFTER-STOP: still Thursday.` The base shows the answer with no
  notice. These three rows (the notice's two and a blank) are the only
  difference.
- **fx.** fx says nothing about the reasoning (fx item 1), so its `next` has
  no notice. Its stop also lands somewhere else: fx's switch is a
  preparation that waits for the same catalog, with the prompt waiting
  behind it. Ctrl-C cancels the switch (`* provider: Provider preparation
  cancelled. The current provider is unchanged.`, H7), the prompt is never
  sent, and the next prompt is answered on codex (fx's log: the codex
  answer after the catalog read). So fx's pair cannot show the notice's
  turn. What it does show is that fx's stopped prompt sent nothing, as
  xfx's did.
- **Not this unit's: fx's switch waits, xfx's commits.** That is the
  `/provider` row's existing divergence (`provider-carry-back`'s `start`,
  `open`, `switched` pairs are unresolved on the base too, `sweep-base.md`).
  This scenario only makes it visible as a stop's target.

## Sweep (`sweep-base.md`, `sweep-head.md`, `sweep-compare.txt`)

38 stop and history scenarios at 40x120, `--jobs 4`, each sweep in one lock
slot: every scenario that presses Ctrl-C to stop a turn (`interrupt*`, the
`p5-*` stop and tool-stop ones, `permissions-midturn`,
`p5-effort-fork-effort-ctrlc`), every `provider*` (the new one included)
and `resume*`, `undo-resume`, and the two `p5-lifecycle` resumes. The base
ran with the binary built at `05a0a857`, the head with the one built at
`3899a8ba`. fx's sha was checked before and after each.

- **Verdicts:** base 115/182, head 115/182, **dropped 0**. No pair changed
  verdict (`cmp_sweeps.py`).
- **Rows:** over all 182 pairs, `pair_rows.py` compares the rows no ledger
  entry covers and the causes named. **One pair moved:**
  `provider-carry-back-stop/next`, which on the head names `P1-carry-notice`
  (the ledger's cause for xfx's notice row, `scenarios = ["provider*"]`),
  with the same eight unattributed rows as the base. Every other pair is
  classified the same, row for row.
- **Why the target pair is not identical, and is not meant to be.** The
  notice is xfx's documented rule, which fx does not have (fx item 1). Its
  ledger cause stays P1's to settle. The other eight rows are the switch's
  existing divergence (the capture section). So the target pair moves from
  "the notice silently lost" to "the notice shown, under its named cause".
  It cannot move to identical: on the base it matched fx's silence only by
  losing the notice. The sweep's own captures agree with the standalone
  ones: the head's `next` has the notice and the base's does not, and every
  side answers the next prompt (`MARKER-AFTER-STOP`).
- Load was high on both runs (13.4 and 17.6 at the peaks, `sweep-*.md`
  line 5). No pair's verdict depended on it.

## Gate (`gates.txt`, the script `gates.sh`)

`gates.sh` ran at `f16a62ab`, the gated commit, in one lock slot, with a
clean tree before and after. It runs every step of `ci.yml` and the two
test scripts of `sweep.yml`. Step 00 (`false`) reads `rc=1`; every other
step reads `rc=0`:

- fmt, clippy, clippy with fault-injection;
- `cargo test --all-targets`: 5449 passed, 0 failed. With fault-injection:
  the `tui` test, 195 passed; the lib, 4556 passed;
- check-no-stubs, check-no-secrets, test-check-no-secrets, test-gate-env,
  check-xfx-identity, check-preview-contract, test-preview-contract,
  check-tui-preflight-cost, check-usage-clock;
- fxcap test-sweep and test-capture;
- both release builds, smoke, and smoke-tui (37 scenarios + the oracle,
  1168 checks, 0 failures).

**Run 1, the same commit, is kept below run 2 in `gates.txt`.** Its `TMPDIR`
root was longer, and step 16 failed on that alone: five of
`scripts/fxcap/test_sweep.py`'s tests refused the root as "too long for a tmux socket
path under it" (`scripts/fxcap/sweep.py:159-161`; 100 bytes with the
socket's suffix). Every other step read `rc=0`. Under the short root the
step passes, in run 2 and alone: 97 tests, `test_sweep.py`'s 70 and
`test_ci_sweep.py`'s 27 (the count line under run 2 in `gates.txt`, added
in round 2).

## Round 2 (opus APPROVE-WITH-FIXES, gpt6 MUST none)

The code was right. One sentence about fx was not, and that sentence was
in three shipped texts.

- **The MUST (opus): "claimed only by a turn past its stop checks" was
  false.** At the pin, `claimContextNotice` has six callers in the app and
  the TUI, not one (`fx-src.txt` H8). fx item 2 above now names each, with
  where it runs relative to a turn's stop. Rewritten in:
  - `docs/parity.md`, the `/provider` row;
  - `Conversation::unsaid`'s doc (`src/interactive.rs:1183-1193` at
    `dec785aa`);
  - the RED's comment (`src/tui/worker.rs:18037-18052`), whose "claimed
    where it is shown" contradicted the row's "claimed when it is pushed";
  - fx items 2-4 and the decision above.
  **The decision is unchanged.** fx paints a project file's line at
  submission and claims the catalogs' lines only past the stop checks, so a
  turn stopped before its request loses none in fx either (opus's
  expectation, confirmed at the pin: `main.zig:1193-1200`, `:2411-2413`;
  `app_agent_runtime.zig:906-938`).
- **The SHOULDs.**
  - The "97 tests" now has its count line in `gates.txt` (`test_sweep.py`
    70, `test_ci_sweep.py` 27, from run 2's step-16 log).
  - `cancelling`'s list is corrected: ToolStart and ToolResult are still
    marked; only their activity row is skipped.
  - The parity row names the screen's drops (a reset, a resume's or a
    fork's replay).
  - "Ok means shown" is replaced by the consumer and lifecycle invariants
    (the fix section). The mark site's comment says `send_ui` queues
    (`src/tui/worker.rs:6204-6212`).
- **Three tests (gpt6).** Each pins an invariant the fix relies on that was
  already true at `b5bf9b8b`; the screen and ledger tests' invariants were
  true on the base as well. So there is no RED commit for them. Each one
  fails under the mutant that breaks its invariant (below), and all pass at
  `dec785aa` (`green-unit-r2.txt`, with the RED, STOP-IN-CATALOG-WAIT's two
  and the pty pin).
  - `a_history_notice_the_screen_takes_is_painted_after_a_stop_and_dropped_only_with_its_conversation`
    (`src/tui/shell.rs:22883`), the consumer. It has three cases:
    - a notice taken after the Ctrl-C is painted once, under the stop's
      line;
    - one owed behind a paced answer is cleared by a resume's replay;
    - the old conversation's notice is dropped by a reset's drain, and the
      next conversation's is painted.
  - `the_history_notice_ledger_goes_with_its_conversation`
    (`src/tui/worker.rs:18088`), the lifecycle. It runs one, two, a resume
    away, a resume back, three, a reset, four. Per turn: `[1, 0, 1, 0]`,
    with 2 `Resumed` and 1 `Fresh`. The resumed conversation of the same
    session says the notice again; the reset's has nothing to say.
  - `the_line_shell_leaves_unsaid_only_the_line_it_could_not_write`
    (`src/interactive.rs:4001`): stderr takes the first notice and refuses
    the second. Only the second stays unsaid, and the next turn writes it
    alone.
  - A test across the channel itself, one shell fed by one worker, would
    need the shell's and the worker's test fixtures in one module, and they
    are private to theirs. The pair above meets at the channel: what the
    worker queues (`UiEvent::Notice`) is what the shell takes.
- **Mutants (`mutants-r2.txt`):** all 11 killed, none `build-failed`, on a
  `git clone --no-local` copy of `dec785aa`, 600 s each, under the lock.
  The runner now reports a mutant that does not compile as `build-failed`,
  not as a kill.

  | mutant | what it does | killed by |
  |---|---|---|
  | round 1's six | as above | as above, again |
  | `line-shell-said-all-when-found` | the line shell's base rule: every line found is recorded, then written | both line-shell tests (`:3990`, `:4048`) |
  | `ledger-outlives-its-conversation` | one ledger for the process | the ledger test (`[1, 0, 0, 0]`), and the RED and the per-line test, which share that ledger |
  | `screen-drops-notice-under-stop` | the screen skips a notice under `cancelling` | the screen test's case 1 (`:22920`, the head's `:22916` plus the mutant's 4 lines) |
  | `reset-drain-keeps-notice` | a reset's drain paints the old conversation's notice | case 3 (`:22956`) |
  | `replay-keeps-owed-marks` | a resume's replay keeps the marks it owed | case 2 (`:22944`, the head's `:22945` less 1) |
- **Sweep (`sweep-r2-base.md`, `sweep-r2-head.md`, `sweep-compare-r2.txt`).**
  The same 38 scenarios, base (the binary built at `05a0a857`) then head
  (built at `dec785aa`, row 1 `v0.1.0-dec785a`), back to back in one slot.
  - Verdicts: 115/182 on both, dropped 0. One pair's rows moved,
    `provider-carry-back-stop/next`: it gained `P1-carry-notice`, with the
    same eight unattributed rows, as in round 1.
  - Each sweep's own `next` capture: the head has the notice, the base has
    none, and both answer the next prompt.
  - Load was 16.3 and 22.4 at the peaks.
- **Size:** outside tests, round 2 changes only comments
  (`src/interactive.rs` +11/-7, `src/tui/worker.rs` +9/-6,
  `git diff -U0 53ada840 dec785aa -- src`). The tests add 52, 80 and 115
  lines.
