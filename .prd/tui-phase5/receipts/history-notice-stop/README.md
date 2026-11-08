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
| (receipt commits) | this folder |

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
2. **fx's one notice said once a session is a source limit's `[context]`
   line.** The session keeps a ledger of the ones said (`claimContextNotice`,
   `session.zig:1551-1561`). The TUI, `fx ask` and ACP each claim the line
   first and then hand it on (H3: `app_callbacks.zig:1294-1307`,
   `cli_ask.zig:3216-3228`, `acp/prompt.zig:3219-3227`). So fx marks it at
   the push, not when it is painted.
3. **A turn stopped before its request claims nothing.** The turn-start
   claimer is `prepareSkillCatalog` (`orchestrator.zig:5772-5791`), called
   at `:5950`. Both stop checks come before it: the capability resolve's
   `Cancelled` (`:5893-5919`) and the cancel flag right after it
   (`:5923-5948`). Each persists the turn interrupted and returns (H4). A
   turn stopped in its catalog wait therefore leaves the line unclaimed,
   and the next turn claims and shows it.
4. **fx can still lose a claimed line, in a race.** A cancelled turn's drain
   drops `.semantic_notice` (H5: `app_worker_runtime.zig:344-354`). A stop
   that lands after `:5950`'s claim and before the drain paints the event
   loses the line for the rest of the session. That is not "a turn stopped
   before its request", and xfx does not copy it (below).

**Decision: fix, not keep.** The brief's keep branch is "fx can observably
lose the notice too". fx has no history notice to lose. Its one
once-a-session notice is kept by a turn stopped before its request (item 3)
and lost only by a stop after its claim (item 4), a race. So xfx now says a
history notice when it is shown, and `docs/parity.md` says so and names
item 4 as not copied.

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
- **The TUI** (`src/tui/worker.rs:6214`) records a line when `send_ui`
  hands it to the UI. The UI paints every `Notice` it is handed, a stopped
  turn's too: `cancelling` filters only Delta, Operational, Retrying,
  Recovered, ToolStart and ToolResult (`src/tui/shell.rs` `apply_event`).
  A reset's drain does drop it (`drained`), but a reset opens a new
  conversation, with nothing said. So `Ok` means shown. A refused send (a
  turn stopped before it began, or a UI that is gone) leaves the line for
  the next turn.
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
  shown; fx's `[context]` line is kept by a turn stopped before its request
  (fx item 3); fx's claim-at-push race (fx item 4) is not copied.
- Source outside tests: 61 lines added and 21 removed (`git diff --numstat
  350e88b1 b5bf9b8b` gives `src/interactive.rs` 88/7 and `src/tui/worker.rs`
  25/14; 52 of interactive.rs's added lines are the line-shell pin), most
  of them comments; far under the 600-line split bar.

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
