# P5 L-U1+U2 receipt: OSC 7501 program status

Epic: lifecycle. Unit L-U1+U2 of `research/tui-phase5:.prd/tui-phase5/research/lifecycle.md`
(rows L1/L2, § "L1/L2. OSC 7501 program status"): fx v0.0.16 (`vercel-labs/fx@481104fd`)
reports what the interactive shell is doing to the terminal, and every restore clears it.

**S**: terminal escapes, model- and tool-controlled text on the wire, the signal handlers' bytes.

- **Base:** `origin/main` `24a6b912`.
- **Code:** `743c5381` (the commit every xfx capture's banner names, `v0.1.0-743c538`); the commits
  after it change comments, the parity row's citations and this receipt.
- **RED build:** the base, `24a6b912` (banner `v0.1.0-24a6b91`), from a `--no-local` clone.
- **fx:** the v0.0.16 reference binary, sha256 `8e6cda2e…`; every run's guard line says it was
  unchanged.

## fx's rule, read at the pin

Every report is `ESC ] 7501 ; state=<s> : app=fx ESC \` (`src/ui/terminal/program_status.zig:20-22`,
`:143-162@481104fd`).

| | fx v0.0.16 | xfx |
|---|---|---|
| `app=` | `fx` (`:22`) | `xfx` -- the record names the program that owns the terminal; xfx's window title and banner say `xfx` (`src/tui/frame.rs` title). Recorded in `docs/parity.md`. |
| states | `idle`, `working`, `done`, `error`, `blocked:kind=permission\|question` (`:30-41`, `:143-149`) | the same |
| activity | approval prompt, then question prompt, then `busy`, else settled (`app_worker_runtime.zig:774-784`); `busy = !cancellation_stops_turn and (stream.active or processing or queued_count > 0)` (`:762-765`) | a permission question (either plane, or a rule change's confirmation), then the model's question, then `!cancelling && (turn running \|\| work in hand)`, else settled (`Shell::take_program_status`) |
| settled value | `noteTurnStarted` resets to idle (`:972`, `:985`, `:1002`); `noteTurnFinished`: completed→done, failed→error, interrupted/paused→idle (`program_status.zig:62-71`); `noteFailure` on an error-tone `error_text` (`app_worker_runtime.zig:1214-1216`); manual `/compact`: succeeded→done, failed→error, cancelled→idle, no_op/busy→nothing, once per operation (`program_status.zig:80-96`) | `TurnStarted` resets; `TurnEnded`: stopped→idle, a failure→error (a turn's, or `/model`-class work's: fx's production `error_text` events are its `request failed` notice and its `worker loop crashed`, `app_process_runtime.zig:70`, `:142`, `:164`), answered→done; manual `/compact` as fx, its every failure `error` (fx's uncertain publication is `.failed`, `compaction_activity.zig:198`), once per operation |
| dedup | an unchanged report is not written again, except a new result; once a result is reported, the next `working`/`blocked` turns it back to idle (`:106-128`, `4ae9eeeb`) | the same, ported with fx's own test vectors |
| `msg` | the permission's label (`request.view().label`) or the question; folded: each run of ≤0x20 / 0x7f-0x9f one space, invalid UTF-8 dropped, no leading/trailing space; at most 256 bytes ending `...` on a character boundary; standard base64 with padding; omitted when empty (`:23-24`, `:152-160`, `:164-204`). Nothing is spelled first by the reporter. | the same fold, bound and encoding (`program_status::one_line`, `crate::image::base64`). The label is `<tool> <argument>` (fx's `formatPermissionLabel` shape, `tool_presentation.zig:723-757`): fx's command is `shell.run <command>`, xfx's `terminal <command>`. |
| order | between frames (`writeProgramStatus`, `transcript/runtime.zig:10416-10429`), from the worker tick inside `loopCollectFacts` (`main.zig:2983`), which runs before the loop commits its frame (`event_loop.zig:80`, `:116-117`): the launch's `idle` is after the two `OSC 2` titles and before the first `?2026h` | before the tick's frame (`event_loop::commit_program_status`), so the launch's `idle` is after the mode set and before the first `?2026h` |
| clear | `ESC]7501;state=clear ESC\` (`:18`) right after `?2004l` in all four restores, normal and abnormal, with and without tmux (`app_lifecycle.zig:43-48`); the abnormal ones written by the SIGTERM/SIGHUP handler in one `write(2)` (`:59-75`, installed `:87-105`); suspend writes the normal restore (`:1157-1165`) | right after `?2004l` in all four restores (`term::RESTORE*`, `ABNORMAL_RESTORE*`); the abnormal ones are what `restore_pair` writes in one `write(2)` from the SIGTERM/SIGHUP/SIGINT handlers, the stop handler and the panic hook. A `const` assertion fails the build for a restore without it. |
| resume | `defer program_status.invalidate()` (`main.zig:1050-1055`): the next report is written even if unchanged, and a result already reported comes back idle (`program_status.zig:98-104`) | `adopt_resume` calls `Shell::program_status_resumed`, the same `invalidate` |
| a refused write | traced, never retried (`runtime.zig:10422-10428`) | offered again next tick, as xfx's bells and theme query are; `Partial` ends the session, as every torn coordinate-free vector does |
| non-TUI | `program_status` has no reader but the interactive `App` (`git grep program_status 481104fd`) | the reporter lives in `tui::Shell` only; `ask`, subcommands and the line shell write none (tests below) |
| support query | none (`:1-6`) | none |

## What changed

- `src/tui/program_status.rs` (new): fx's `Reporter` -- the settled value, the
  once-per-result dedup, `invalidate` -- the `msg` fold (`one_line`), its bound and its base64,
  `permission_label`, the rule-confirmation labels (`rule_label`) and the `/compact` outcome map
  (`of_compaction`). Its tests are fx's own vectors (`program_status.zig:220-361`), fx's hostile
  string byte for byte included, plus a security case over controls, bidi overrides and
  isolates, zero-width marks and a cut text.
- `src/tui/shell.rs`: the reporter is the shell's. `TurnStarted` resets it; `TurnEnded` reads the
  cancel **before** it is taken (stopped → idle, a failure → error, an answered turn → done,
  work with no turn and no failure → no result); a manual `/compact`'s ending is noted once,
  whether the runtime says it or the shell settles a stopped one. `take_program_status` is fx's
  `reportProgramStatus` precedence. fx's `app_worker_runtime.zig:2019-2104` case is ported
  (`program_status_follows_decision_prompts_to_the_turn_result`), with four more: a stop and
  `/model`-class work, a resume, a rule confirmation outside a turn, `/compact`.
- `src/tui/event_loop.rs`: `commit_program_status`, before the frame in `commit_counted`, through
  the output check, disposed of as the bells are. `adopt_resume` -- the one seam every continue
  goes through -- invalidates the reporter.
- `src/tui/check.rs`: `OSC 7501` is in the output check's alphabet only in xfx's shapes
  (`program_status_command`, a parser of its own: `ESC \` only, the state words, `app=xfx`, a
  `msg` only on a blocked state, strict base64, at most 344 characters, decoding to UTF-8 with no
  control). `Intent::ProgramStatus(state)` declares a report; `Intent::Modes` says whether the
  vector clears the record, so the restore check requires the clear and the mode set's check
  forbids one; a frame, a bell, a query or a frame's title lead carrying a report is refused.
- `src/tui/term.rs`: the clear after `?2004l` in `RESTORE`, `RESTORE_TMUX`, `ABNORMAL_RESTORE`,
  `ABNORMAL_RESTORE_TMUX`, and a `const` assertion that fails the build when any of the four
  lost it or moved it. `restore_pair` and the handlers are unchanged: one `write(2)` of the
  `'static` constant, then `tcsetattr`.
- `src/tui/approval.rs`, `approval_screen.rs`, `question.rs`: `request()` and `question()`
  accessors.
- `tests/tui.rs`: the restore constants carry the clear, so every existing restore assertion --
  the planned exit, tmux, SIGINT/TERM/HUP, the stop -- now requires it. New:
  `the_session_reports_its_status_between_frames_and_clears_it_on_the_way_out` (idle after the
  mode set and before the first `?2026h`, working, done, every report outside an update, nothing
  after the restore), `an_ask_on_a_terminal_reports_no_program_status`; the SIGTERM, SIGHUP and
  SIGTSTP cases check that nothing follows the abnormal restore, and the stop/continue case that
  the resume writes `idle` again after the stop's clear. `TUI_BYTES` gains `ESC]7501;`, so the
  line shell, a subcommand and the refused launches are held to writing none.
- `tests/interactive.rs`: a line-shell turn writes no report.
- `docs/parity.md`: a `terminal program status (OSC 7501)` row (implemented, with its three
  differences), and a row in the torn-write table.
- `scripts/fxcap/scenarios/p5-lifecycle-osc-*`: ten wire-only scenarios (below), each product's
  own command tool where a command asks.

## Decisions

- **`app=xfx`.** The record names the program that owns the terminal; xfx's window title and
  banner both say `xfx`. `osc.py` normalizes the name, by name.
- **The clear right after `?2004l`.** xfx's restores are in a different order from fx's; the
  property kept on both sides is "the clear follows `?2004l`", which `osc.py` checks on both wires.
- **The report before the frame.** fx's worker tick reports inside `loopCollectFacts`, before the
  loop commits its frame; the launch's `idle` ahead of the first `?2026h` is what that order
  leaves on fx's wire (`osc/w00-start`, and `w00-start` here).
- **A refused report is offered again**, recomputed, as xfx's bells and theme query are; fx
  traces and moves on. Observable only on a terminal that refused a write.
- **`busy` is `!cancelling && (turn running || work in hand)`**: fx's `processing ||
  queued_count > 0` is xfx's `work.outstanding() > 0` (a submitted prompt or any piece of runtime
  work), and its `cancellation_stops_turn` is xfx's `cancelling`. A launch with nothing to do has
  nothing in hand: the launch writes one `idle` (`w00-start`, and the pty test counts exactly one).
- **A failure with no turn is `error`**, as fx's `noteFailure` on its `request failed` notice is.
- **Suspend is xfx's `SIGTSTP`.** A typed Ctrl-Z binds nothing in xfx (the `full-screen TUI`
  row); binding it is signal-path work of its own (xfx's stop handler raises `SIGSTOP`, which an
  orphaned group does not discard, where fx's `raise(SIGTSTP)` is discarded there) and is not in
  this unit. The suspend scenario stops each product its own way.

## Scenarios (`scripts/fxcap/scenarios/p5-lifecycle-osc-*`)

| scenario | what it drives |
|---|---|
| `states` | launch; a turn held mid-stream (`holdAt`/`held`/`release`); done; done again; a 400; a command in ask mode, answered once; a question, answered; `/quit` |
| `hostile` | a command whose label carries U+202E/U+202C and a tab; a question carrying ESC, BEL, an OSC, NEL, a CSI, U+202E, a line feed and a tab |
| `interrupt` | a turn held mid-stream, stopped with two Escapes; `/quit` |
| `retry` | a turn whose every request answers 502, read at 0.3 s |
| `suspend` | under `jobctl`: a turn, then fx's Ctrl-Z / xfx's `SIGTSTP`, then `SIGCONT`, then a turn |
| `term`, `hup` | a turn, then `SIGTERM` / `SIGHUP` |
| `compact` | `/compact` with nothing to compact, two turns, `/compact` |
| `compactfail` | two turns, a `/compact` whose notes request is refused twice |
| `ask` | `/quit`, then the same pane relaunched as `ask hello` |

## RED (base `24a6b912`)

- **Wires:** `python3 -I -B osc.py . red 24a6b91` (`red.txt`): 32 wire pairs, **39 failing
  claims** -- the base writes no `OSC 7501` at all (`same` fails on all 32, and `last` on the
  seven wires that end a session, which carry no clear).
- **Tests:** this branch's `tests/tui.rs` against the base's code (`red-test.txt`): 5 fail --
  `the_session_reports_its_status_…` (`the launch's idle`: none), and `a_normal_exit_…`,
  `sigterm_…`, `sighup_…`, `sigtstp_…` (the restore with its clear never arrives). The two guards
  (`an_ask_on_a_terminal_…`, `zero_is_the_line_shell`) pass on the base too, as they should: the
  base writes no report anywhere.

## GREEN (`743c5381`)

`python3 -I -B osc.py . xfx 743c538` (`green.txt`): **32 wire pairs, 0 failing claims** -- every
report, in order, fx's; none inside a frame; every clear right after `?2004l`; nothing after the
last clear of a session that ended; every `msg` base64 decoding to one line without a control;
every xfx banner `v0.1.0-743c538`.

| scenario | wires | the reports (normalized), the last wire |
|---|---|---|
| `states` | 10 | idle, working, done, working, done, working, error, working, blocked:permission `<command-tool> echo hi > out.txt`, working, done, working, blocked:question `Which branch?`, working, done, clear |
| `hostile` | 3 | idle, working, blocked:permission `<command-tool> echo \u{202e}evil\u{202c}\x09done > out.txt`, working, done, working, blocked:question `Pick\x1b]2;owned\x07 a\u{0085}branch\u{202e}? now \u{009b}31m`, working, done, clear |
| `interrupt` | 3 | idle, working, idle, clear |
| `retry` | 1 | idle, working |
| `suspend` | 4 | idle, working, done, clear, idle, working, done |
| `term` / `hup` | 2 / 2 | idle, working, done, clear |
| `compact` | 3 | idle, working, done, working, done, working, done (the no-op wrote nothing) |
| `compactfail` | 2 | idle, working, done, working, done, working, error |
| `ask` | 2 | idle, clear -- and after the clear, the answer `ANSWER-ONE.` with no report, on both (771 bytes on fx, 69 on xfx) |

The three normalizations `osc.py` applies are named in its header: `app-name`,
`command-tool-name` (`shell.run` / `terminal`), and `question-breaks` (below).

## Security

- **Foreign text reaches the terminal only as base64.** The `hostile` pair: the label's bidi marks
  and tab, and the question's ESC, BEL, NEL, CSI and bidi mark, are in `msg` as base64 only, on
  both products; decoded, both are one line with no control (the `inert` claim). Both products
  hand their reporter text a tool already spelled -- the controls arrive as `\x1b`-style text
  (fx's `encodeTerminalSafe`; xfx's command label and `terminal_safe` question), which is why the
  decoded messages above read that way -- so the fold and the encoding are the second line, and
  the unit tests below drive them with raw controls. Unit tests:
  `a_message_reaches_the_terminal_as_base64_and_nothing_else`, fx's hostile vector, and the output
  check's `a_program_status_reaches_the_terminal_only_in_the_shapes_this_crate_writes` (a raw
  `msg`, a `BEL` terminator, a foreign `app`, bad base64, a decoded ESC, a decoded C1, a decoded
  non-UTF-8 byte, past the bound -- each refused by name).
- **The handlers stay async-signal-safe.** The clear is inside the four `'static` literals;
  `restore_pair` still writes one of them in one `write(2)` and calls `tcsetattr` -- no
  allocation, no lock, no new call. A `const` assertion ties each literal to the clear at build
  time; `every_restore_clears_the_program_status_right_after_bracketed_paste` holds the bytes;
  the pty SIGINT, SIGTERM, SIGHUP and SIGTSTP cases wait for the abnormal restore with its
  clear, and the SIGTERM, SIGHUP and SIGTSTP cases check that nothing follows it.
- **No status outside the TUI.** `ask` on a terminal, a subcommand, the line shell (with a whole
  turn) and every refused launch write no `ESC]7501;` (`TUI_BYTES`, the two new assertions). fx's
  `program_status` has no reader but its interactive `App`.

## What the captures showed beyond this unit

- **xfx's question tool spells a line feed and a tab** (`terminal_safe`,
  `src/tools/question.rs:57-73`) where fx v0.0.16 keeps them in the question; fx's fold makes
  the run one space, and xfx's reporter folds nothing because nothing is left to fold. The
  `hostile` question's `msg` is `…? now…` on fx and `…?\x0a\x09now…` on xfx; `question-breaks`
  compares them as one space. The question tool's rule, not this unit's.
- **xfx's turn gives up on a run of 502s** after its third attempt (`gateway::DEFAULT_MAX_ATTEMPTS`,
  the deferred `transport-owned retry` row), within the first second, where fx v0.0.16 keeps
  retrying: two Escapes at 0.5 s/0.7 s after Enter, and again 0.3 s/0.45 s after a wire read at
  0.3 s, found xfx's turn already failed (`error`) where fx's was stopped (`idle`). So `retry` reads the retrying turn
  at 0.3 s only, and the stop's `idle` is `interrupt`'s, a held stream on both.

## Mutants

**Code** (`mutants.py`, `mutants.txt`): sixteen one-line mutants, each run on a `git clone
--no-local` of `743c5381` in the scratchpad (never the worktree), against the unit tests this unit
added or touched (a build, for the two restore literals):

- the abnormal restore without its clear; the tmux restore with its clear moved to the end --
  **the build fails** (`const` assertion, `E0080`);
- `msg` raw instead of base64; not folded; not bounded -- fx's vectors and the security case fail;
- an unchanged report written again; a second result in a row not reported; a reported result
  repeated after work that ran no turn -- the ported dedup cases fail;
- a resume that does not report again; the report after the frame -- the loop's cases fail;
- a stopped turn reported `working`; a submission in hand not reported `working` -- the shell's
  cases fail;
- the output check admitting a decoded control, a `BEL`-terminated report, or a restore with no
  clear -- the check's and the restore's cases fail;
- **a failure with no turn not reported survived** on `743c5381` (16 mutants, 15 killed). In xfx that is a provider
  switch that failed (`TurnWork::Setup`, `worker.rs:4839`: the one piece of work that concludes
  with a failure and no turn). The shell case now ends with one, and the mutant fails it
  (`mutants.txt`, its rerun line).

**Checker** (`wire_mutants.py`, `wire-mutants.txt`): eight defects written into a copy of one xfx
wire -- the exit's clear dropped, the clear moved after `?25h`, a report after the clear, a report
inside a frame, a raw label, a label decoding to ESC, a report lost, another build's banner --
and `osc.py` fails each with the claim named, and passes the unchanged copy.

## Sweep

`scripts/fxcap/sweep.sh --only 'p5-lifecycle-*' --only startup --only 'quit*' --only 'composer-*'
--jobs 3`, fx v0.0.16 against the base binary (`24a6b912`) and then against the head's
(`743c5381`), every `p5-lifecycle-osc-*` scenario included except `ask`, added after (it takes no
snap): 97 snap pairs on both.

| | normalized-identical | approved divergence | unresolved | failed / timeout |
|---|---|---|---|---|
| base `24a6b912` | 52 | 1 | 44 | 0 / 0 |
| head `743c5381` | 53 | 1 | 43 | 0 / 0 |

**No pair dropped.** The one that moved is `p5-lifecycle-retryok k01-1.5s`, unresolved on the base
run and identical on the head run: on the base run fx's own screen still said `retrying request
in 1s` at 1.5 s where xfx had recovered, and on the head run both had -- fx's timing in that run,
not this change. The comparator reads snaps (`capture-pane`), which carry no OSC: the reports this
unit adds reach neither side's cells, as fx's own do not, so nothing needed normalizing; the wires
are compared by `osc.py` instead.

## Gates

(filled in below)
