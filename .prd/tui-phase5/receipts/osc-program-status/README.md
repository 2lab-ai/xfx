# P5 L-U1+U2 receipt: OSC 7501 program status

Epic: lifecycle. Unit L-U1+U2 of `research/tui-phase5:.prd/tui-phase5/research/lifecycle.md`
(rows L1/L2, § "L1/L2. OSC 7501 program status"): fx v0.0.16 (`vercel-labs/fx@481104fd`)
reports what the interactive shell is doing to the terminal, and every restore includes the
clear of that record.

**S**: terminal escapes, model- and tool-controlled text on the wire, the signal handlers' bytes.

- **Base:** `origin/main` `24a6b912`.
- **Code:** `7d8b2c10`, the commit every `xfx/` capture's banner names (`v0.1.0-7d8b2c1`). History:
  - `743c5381`, the unit's first commit;
  - `940878dd`, tests, comments and scenarios. **Its message says "a failure with no turn is an
    error"; the final rule is the opposite** (`eb90e677`: work that ran no turn has no result).
    The history is left as it is;
  - `eb90e677`, the work rule (round 1);
  - `ecd4ace2`, a test case;
  - `fefa5f25`, round 2's code: a refused report dropped, an automatic compaction's failure an
    error, a resume rewriting the record only after the stop handler's clear, the checker's
    decoded bound and canonical base64;
  - `7d8b2c10`, the smoke-tui oracle and one scenario (`autocompactfail`), no product code.

  The commits after it change this receipt only.
- **prev builds,** each kept for the one scenario it failed: `743c5381` (`v0.1.0-743c538`,
  `prev/`, `picker`) and `eb90e677` (`v0.1.0-eb90e67`, `prev2/`, `autocompactfail`).
- **RED build:** the base, `24a6b912` (`v0.1.0-24a6b91`), from a `--no-local` clone.
- **fx:** the v0.0.16 reference binary, sha256 `8e6cda2e…`; every run's guard line says it was
  unchanged. The fx captures of the eleven round-1 scenarios are reused (same binary, same
  scenario files); `autocompactfail` was captured on fx in round 2.

## fx's rule, read at the pin

Every report is `ESC ] 7501 ; state=<s> : app=fx ESC \` (`src/ui/terminal/program_status.zig:20-22`,
`:143-162@481104fd`).

| | fx v0.0.16 | xfx |
|---|---|---|
| `app=` | `fx` (`:22`) | `xfx`: the user's identity decision, "a separate identity" (`.prd/01-spec.md:56`), held by `scripts/check-xfx-identity.sh:5`. Recorded in `docs/parity.md`. |
| states | `idle`, `working`, `done`, `error`, `blocked:kind=permission\|question` (`:30-41`, `:143-149`) | the same |
| activity | approval prompt, then question prompt, then `busy`, else settled (`app_worker_runtime.zig:774-784`); `busy = !cancellation_stops_turn and (stream.active or processing or queued_count > 0)` (`:762-765`), where the worker processes a prompt or a compaction and nothing else (`WorkItem`, `worker_runtime.zig:167-170`) | a permission question (either plane, or a rule change's confirmation), then the model's question, then work -- a turn running, a prompt waiting, or a manual `/compact` in hand -- unless the user stopped it, else settled (`Shell::take_program_status`). A provider switch, a `/resume` listing, a fork and a `/workspace` are runtime work in xfx and none in fx (its picker is polled, `pollSessionPicker`, `main.zig:2930`): they report nothing (`picker`) |
| settled value | `noteTurnStarted` resets to idle (`:972`, `:985`, `:1002`); `noteTurnFinished`: completed→done, failed→error, interrupted/paused→idle (`program_status.zig:62-71`); `noteFailure` on an error-tone `error_text` (`app_worker_runtime.zig:1214-1216`), whose production senders are a failed work item's `request failed` and `worker loop crashed` (`app_process_runtime.zig:70`, `:142`, `:164`); manual `/compact`: succeeded→done, failed→error, cancelled→idle, no_op/busy→nothing, once per operation (`program_status.zig:80-96`). An automatic compaction belongs to its turn (`:79`): its error leaves the turn (`orchestrator.zig:7864-7865`) and the turn is finished `.failed` (`:5615-5618`); only a cancelled one is an interrupted turn (`:7847-7861`) | `TurnStarted` resets; `TurnEnded` **of a turn** (`tally.started`; every prompt sends `TurnStarted` first, `worker.rs:3784`): a turn an automatic compaction failed → error (`Shell::compaction_failed`, read first), stopped → idle, failed → error, answered → done; a `TurnEnded` with no turn -- a provider switch, failed or not -- has no result; manual `/compact` as fx, its every failure `error` (fx's uncertain publication is `.failed`, `compaction_activity.zig:198`), once per operation |
| dedup | an unchanged report is not written again, except a new result; once a result is reported, the next `working`/`blocked` turns it back to idle (`:106-128`, `4ae9eeeb`) | the same, ported with fx's own test vectors |
| `msg` | the permission's label (`request.view().label`) or the question; folded: each run of ≤0x20 / 0x7f-0x9f one space, invalid UTF-8 dropped, no leading/trailing space; at most 256 bytes ending `...` on a character boundary; standard base64 with padding; omitted when empty (`:23-24`, `:152-160`, `:164-204`). Nothing is spelled first by the reporter. | the same fold, bound and encoding (`program_status::one_line`, `crate::image::base64`). The label is `<tool> <argument>` (fx's `formatPermissionLabel` shape, `tool_presentation.zig:723-757`): fx's command is `shell.run <command>`, xfx's `terminal <command>`. |
| order | between frames (`writeProgramStatus`, `transcript/runtime.zig:10416-10429`), from the worker tick inside `loopCollectFacts` (`main.zig:2983`), which runs before the loop commits its frame (`event_loop.zig:80`, `:116-117`): the launch's `idle` is after the two `OSC 2` titles and before the first `?2026h` | before the tick's frame (`event_loop::commit_program_status`), so the launch's `idle` is after the mode set and before the first `?2026h` -- and **before** the window title. Not copied, and why: xfx's title is the lead of its first frame's own vector (`Band::retitle`, `src/tui/frame.rs:1381-1394`), and a write of the title apart from the frame is the second writer that module exists to forbid (`frame.rs:824-829`); putting the report inside the frame's vector after the title would tie it to the frame's refusal and move the partial-frame fault's tear point. Neither the title nor the report moves a cell, and a terminal applies each on its own. Recorded in `docs/parity.md`. |
| clear | `ESC]7501;state=clear ESC\` (`:18`) right after `?2004l` in all four restores, normal and abnormal, with and without tmux (`app_lifecycle.zig:43-48`); the abnormal ones written by the SIGTERM/SIGHUP handler in one `write(2)` (`:59-75`, installed `:87-105`); suspend writes the normal restore (`:1157-1165`) | right after `?2004l` in all four restores (`term::RESTORE*`, `ABNORMAL_RESTORE*`); the abnormal ones are what `restore_pair` writes in one `write(2)` from the SIGTERM/SIGHUP/SIGINT handlers, the stop handler and the panic hook. A `const` assertion fails the build for a restore without it. The restores **include** it; a restore the descriptor takes only part of, or a failed handler `write(2)`, can lose it, as it can lose the rest of the restore. |
| resume | `defer program_status.invalidate()` around its own suspend (`main.zig:1050-1055`): the next report is written even if unchanged, and a result already reported comes back idle (`program_status.zig:98-104`) | `adopt_resume` invalidates **only when the stop handler wrote the abnormal restore** (`plane_restored`), so a bare `SIGCONT`, or a `SIGSTOP` and its `CONT`, leaves the record as it was |
| a refused write | the report is recorded as written before the write is tried (`program_status.zig:124-127`) and a failed write is only traced (`runtime.zig:10422-10428`): dropped | dropped, the same way: the report was recorded before the write, and the refusal is disposed of as any coordinate-free vector's (it counts against the frame budget; a torn one ends the session) |
| non-TUI | `program_status` has no reader but the interactive `App` (`git grep program_status 481104fd`) | the reporter lives in `tui::Shell` only; `ask`, subcommands and the line shell write none (tests below) |
| support query | none (`:1-6`) | none |

## What changed

Round 1 (`743c5381`..`ecd4ace2`):

- `src/tui/program_status.rs` (new): fx's `Reporter` -- the settled value, the
  once-per-result dedup, `invalidate` -- the `msg` fold (`one_line`), its bound and its base64,
  `permission_label`, the rule-confirmation labels (`rule_label`) and the `/compact` outcome map
  (`of_compaction`). Its tests are fx's own vectors (`program_status.zig:220-361`), fx's hostile
  string byte for byte included, plus a security case over controls, bidi overrides and
  isolates, zero-width marks and a cut text.
- `src/tui/shell.rs`: the reporter is the shell's. `TurnStarted` resets it; `TurnEnded` **of a
  turn** reads the cancel before it is taken (stopped → idle, failed → error, answered → done), and
  one with no turn has no result; a manual `/compact`'s ending is noted once, whether the runtime
  says it or the shell settles a stopped one. `take_program_status` is fx's `reportProgramStatus`
  precedence, with fx's `busy`. fx's `app_worker_runtime.zig:2019-2104` case is ported
  (`program_status_follows_decision_prompts_to_the_turn_result`), with more: a stop (one with a
  call still running included), work that is no turn, a resume, a rule confirmation outside a
  turn, `/compact`.
- `src/tui/event_loop.rs`: `commit_program_status`, before the frame in `commit_counted`, through
  the output check. `adopt_resume` invalidates the reporter.
- `src/tui/check.rs`: `OSC 7501` is in the output check's alphabet only in xfx's shapes
  (`program_status_command`, a parser of its own). `Intent::ProgramStatus(state)` declares a
  report; `Intent::Modes` says whether the vector clears the record, so the restore check
  requires the clear and the mode set's check forbids one; a frame, a bell, a query or a frame's
  title lead carrying a report is refused.
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

Round 2 (`fefa5f25`, `7d8b2c10`), from the two reviews of `4e41f51c` and the dispatcher's gate:

- **A refused report is dropped** (dispatcher's decision: copy fx). `Reporter::refused` and its
  re-arm are gone; `a_refused_program_status_is_dropped_as_upstreams_is` pins it -- nothing on the
  next tick, the next change written.
- **An automatic compaction that fails inside a turn reports `error`**, as fx's turn finishes
  `.failed`. `Shell::compaction_failed` is set beside `cancelling` (which still keeps the
  conclusion silent) unless the compaction was cancelled, read before it at `TurnEnded`, and the
  turn stays `working` until it ends. `an_automatic_compaction_that_fails_inside_a_turn_is_that_turns_error`,
  and the `autocompactfail` capture on fx and xfx (`prev2/`: `eb90e677` reported `idle`).
- **A resume rewrites the record only after the stop handler's clear** (`plane_restored`).
  `a_continue_no_handler_answered_leaves_the_record_as_it_was`.
- **The output check**: a message past 256 decoded bytes is refused (`QUFB`×86 decodes to 258),
  and so is a non-canonical base64 tail (`YR==`).
- `every_report_the_encoder_makes_of_hostile_text_passes_the_output_check`: the encoder's reports
  of hostile text, both kinds, all admitted by `check_program_status` -- the encoder and the
  checker agree.
- `a_report_the_terminal_took_part_of_ends_the_session_and_the_restore_ends_the_string`: a torn
  report ends the session, and every restore opens with `ESC`, which ends the string sequence the
  terminal was left in.
- **`scripts/smoke-tui.sh`** (the gate round 1 did not run): its emulator admits `OSC 7501` in
  exactly `check.rs`'s shapes and keeps each in `statuses`, one inside an open update being a
  finding; its oracle falsification gains the product's shapes and nineteen refusals; its restore
  literal carries the clear.
- `docs/parity.md`: the `terminal program status (OSC 7501)` row (the `app=xfx` citation, the
  launch order, "includes the clear", the continue rule); a `a question's line breaks in a
  program status message` row (owner unit **QUESTION-BREAKS**); owner units **CTRL-Z** (the
  `full-screen TUI` row) and **RETRY-BUDGET** (the `transport-owned retry` row, which now says the
  status reads `error` where fx's reads `working`); a row in the torn-write table.
- `scripts/fxcap/scenarios/p5-lifecycle-osc-*`: twelve wire-only scenarios (below).

## Decisions

- **`app=xfx`.** The user's identity decision (`.prd/01-spec.md:56`). `osc.py` normalizes the
  name, by name.
- **The clear right after `?2004l`.** xfx's restores are in a different order from fx's; the
  property kept on both sides is "the clear follows `?2004l`", which `osc.py` checks on both wires.
- **The report before the frame,** as fx's tick reports before it commits its frame.
- **The launch's `idle` before the title.** Not copied; the reason is in the rule table (`order`)
  and the parity row.
- **`busy` is a turn, a waiting prompt or a manual `/compact`.** fx's worker runs a prompt or a
  compaction and nothing else, so its `busy` covers those alone. `743c5381` counted all runtime
  work and wrote `working, idle` for a `/resume` listing where fx writes nothing (`picker`; fixed
  in `eb90e677`).
- **A result is a turn's.** A `TurnEnded` with no turn has no result; an automatic compaction's
  failure inside a turn is that turn's `error`.
- **Suspend is xfx's `SIGTSTP`.** A typed Ctrl-Z binds nothing in xfx; binding it is signal-path
  work of its own (xfx's stop handler raises `SIGSTOP`, which an orphaned group does not discard,
  where fx's `raise(SIGTSTP)` is discarded there): owner unit **CTRL-Z**. The suspend scenario
  stops each product its own way.

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
| `autocompactfail` | two turns (the first reads a file), then a third the provider refuses as too large (`provider_overflow`), whose automatic compaction's notes request is refused: `Compaction failed.` on both screens |
| `ask` | `/quit`, then the same pane relaunched as `ask hello` |
| `picker` | a turn, then `/resume`'s picker, Escape, `/quit` |

## The checker (`osc.py`)

Its own parser, sharing nothing with the code. It reads reports **from every introducer** on the
wire (`ESC ] 7501` and the 8-bit `0x9d 7501`), so a report cut short, ended by another terminator
or carrying a raw byte fails `intact` rather than being skipped. Claims, per wire pair: `build`
(each xfx wire carries the named build's banner and no other), `intact`, `same` (the reports, in
order, decoded and normalized, are fx's), `between` (no report inside an update), `clear` (every
clear right after `?2004l`), `last` (no introducer at all after the last clear of a session that
ended), `inert` (every `msg` base64 decoding to at most 256 bytes of UTF-8 with no control).

**Three normalizations**, each named in its header: `app-name`; `command-tool-name` (`shell.run`
/ `terminal`); `question-breaks` (xfx's question tool spells a line feed and a tab before the
reporter sees them). `question-breaks` **expires with** the `docs/parity.md` row that records the
difference (owner unit **QUESTION-BREAKS**).

## RED

- **Base `24a6b912`:** `python3 -I -B osc.py . red 24a6b91` (`red.txt`): 38 wire pairs, **45
  failing claims** -- the base writes no `OSC 7501` at all (`same` fails on all 38, and `last` on
  the seven wires that end a session, which carry no clear).
- **`743c5381`, `picker`:** `osc.py . prev 743c538 p5-lifecycle-osc-picker` (`prev.txt`): 4 wire
  pairs, **3 failing** -- it wrote `working, idle` for the `/resume` listing.
- **`eb90e677`, `autocompactfail`:** `osc.py . prev2 eb90e67 p5-lifecycle-osc-autocompactfail`
  (`prev2.txt`): 2 wire pairs, **1 failing** -- it reported `idle` where fx reports `error`.
- **Tests:** `tests/tui.rs` (unchanged since `743c5381`) against the base's code
  (`red-test.txt`): 5 of 7 fail -- `the_session_reports_its_status_…` (no launch `idle`), and
  `a_normal_exit_…`, `sigterm_…`, `sighup_…`, `sigtstp_…` (the restore with its clear never
  arrives). The two guards (`an_ask_on_a_terminal_…`, `zero_is_the_line_shell`) pass on the base
  too, as they should: the base writes no report anywhere.

## GREEN (`7d8b2c10`)

`python3 -I -B osc.py . xfx 7d8b2c1` (`green.txt`): **38 wire pairs, 0 failing claims**; every xfx
wire carries `v0.1.0-7d8b2c1` and no other banner.

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
| `autocompactfail` | 2 | idle, working, done, working, done, working, error |
| `ask` | 2 | idle, clear -- and after the clear, the answer `ANSWER-ONE.` with no introducer, on both |
| `picker` | 4 | idle, working, done, clear -- nothing for the listing or its Escape |

## Security

- **Foreign text reaches the terminal only as base64.** The `hostile` pair: the label's bidi marks
  and tab, and the question's ESC, BEL, NEL, CSI and bidi mark, are in `msg` as base64 only, on
  both products; decoded, both are one line with no control (`inert`). Both products hand their
  reporter text a tool already spelled -- the controls arrive as `\x1b`-style text (fx's
  `encodeTerminalSafe`; xfx's command label and `terminal_safe` question), which is why the decoded
  messages above read that way -- so the fold and the encoding are the second line, and the unit
  tests drive them with raw controls: `a_message_reaches_the_terminal_as_base64_and_nothing_else`,
  fx's hostile vector, and `every_report_the_encoder_makes_of_hostile_text_passes_the_output_check`.
- **The output check refuses every other shape**, by name: a raw `msg`, a `BEL` terminator, a
  foreign `app`, bad or non-canonical base64, a decoded ESC, C1 or non-UTF-8 byte, a message past
  344 characters or 256 decoded bytes, a message on a state that carries none
  (`a_program_status_reaches_the_terminal_only_in_the_shapes_this_crate_writes`). smoke-tui's
  oracle restates the same rules and its falsification refuses the same shapes, plus a report
  inside an update and an unterminated one.
- **The handlers stay async-signal-safe.** The clear is inside the four `'static` literals;
  `restore_pair` still writes one of them in one `write(2)` and calls `tcsetattr` -- no
  allocation, no lock, no new call. A `const` assertion ties each literal to the clear at build
  time; `every_restore_clears_the_program_status_right_after_bracketed_paste` holds the bytes;
  the pty SIGINT, SIGTERM, SIGHUP and SIGTSTP cases wait for the abnormal restore with its clear,
  and the SIGTERM, SIGHUP and SIGTSTP cases check that nothing follows it. The restore *includes*
  the clear; a write the descriptor takes only part of can lose it.
- **A torn report** ends the session, and the restore that follows opens with `ESC`, ending the
  string sequence the terminal was left in (`a_report_the_terminal_took_part_of_…`).
- **No status outside the TUI.** `ask` on a terminal, a subcommand, the line shell (with a whole
  turn) and every refused launch write no `ESC]7501;` (`TUI_BYTES`, the two new assertions, and
  the `ask` capture on both products). fx's `program_status` has no reader but its interactive
  `App`.

## Recorded differences, each with an owner

| difference | where | owner |
|---|---|---|
| `app=xfx` | `terminal program status (OSC 7501)` row | the user's identity decision, `.prd/01-spec.md:56` |
| the launch's `idle` before the title | the same row | this unit, with the reason above |
| a typed Ctrl-Z binds nothing; only `SIGTSTP` stops | `full-screen TUI` row | **CTRL-Z** (queued) |
| a question's line feed and tab reach the reporter spelled (`…?\x0a\x09now…` on xfx, `…? now…` on fx) | `a question's line breaks in a program status message` row | **QUESTION-BREAKS** (queued); `osc.py`'s `question-breaks` expires with it |
| a run of 5xx ends xfx's turn after three attempts, about a second in; the status reads `error` where fx's reads `working`, which is why `retry` reads the retrying turn at 0.3 s only and the stop's `idle` is `interrupt`'s | `transport-owned retry` row | **RETRY-BUDGET** (queued) |

## Mutants

**Code** (`mutants.py`, `mutants.txt`): twenty-six one-line mutants, run on a `git clone
--no-local` of `7d8b2c10` in the scratchpad (never the worktree), against the unit tests this unit
added or touched (a build, for the two restore literals). **All 26 killed.**

- the abnormal restore without its clear; the tmux restore with its clear moved to the end --
  **the build fails** (`const` assertion, `E0080`);
- `msg` raw instead of base64; not folded; not bounded -- fx's vectors, the security case and the
  encoder/checker agreement case fail;
- an unchanged report written again; a second result in a row not reported; a reported result
  repeated after work that ran no turn -- the ported dedup cases fail;
- a resume that does not report again; **any continue rewriting the record**, **a refused report
  offered again** and **an automatic compaction's failure reported idle** (round 1's rules, each
  now a mutant); the failed compaction read after the cancel; a turn whose compaction failed not
  `working` until it ends; the report after the frame -- the loop's and the shell's cases fail;
- a stopped turn still `working`; a waiting prompt or a manual `/compact` not `working`; any
  runtime work `working` (`743c5381`'s rule); work with no turn given a result; a stopped turn
  reported as its failure -- the shell's cases fail;
- the output check admitting a decoded control, a `BEL`-terminated report, a non-canonical base64
  tail, a message past 256 decoded bytes, or a restore with no clear -- the check's and the
  restore's cases fail.

Earlier runs, superseded: `743c5381` (16 mutants, one survivor: a failure with no turn not
reported -- that is now the rule) and `eb90e677` (19, one survivor: the `!cancelling` guard,
killed by `ecd4ace2`'s case).

**Checker** (`wire_mutants.py`, `wire-mutants.txt`): twelve defects written into a copy of the
receipt's xfx wires -- the exit's clear dropped, the clear moved after `?25h`, a report after the
clear, a report inside a frame, a raw label, a label decoding to ESC, a report lost, another
build's banner, **one wire with no banner while the others carry it**, **an unterminated report
after the exit's clear**, a report ended by `BEL`, an 8-bit introducer after the clear -- and
`osc.py` fails each with the claim named, and passes the unchanged copy.

## Sweep

`scripts/fxcap/sweep.sh --only 'p5-lifecycle-*' --only startup --only 'quit*' --only 'composer-*'
--jobs 3`, fx v0.0.16 against four xfx builds, each named by its banner in the run's scoreboard
(`gates.txt`, "the parity sweep"). The `p5-lifecycle-osc-*` scenarios run in it and take no snap,
so they add no pair: 97 snap pairs each time (30 scenarios on the base run, 33 on this one).

| xfx | normalized-identical | approved divergence | unresolved | failed / timeout |
|---|---|---|---|---|
| base `24a6b912` | 52 | 1 | 44 | 0 / 0 |
| `743c5381` | 53 | 1 | 43 | 0 / 0 |
| `eb90e677` | 53 | 1 | 43 | 0 / 0 |
| `7d8b2c10` (this receipt) | 53 | 1 | 43 | 0 / 0 |

Pair by pair against the base (`gates.txt`): **no pair lost.** The one that moved, the same in all
three later runs, is `p5-lifecycle-retryok k01-1.5s`: on the base run fx's own screen still said
`retrying request in 1s` at 1.5 s where xfx had recovered (the diff's lines are quoted); on the
others both had recovered. That is fx's timing in that run. The comparator reads snaps
(`capture-pane`), which carry no OSC, so the reports reach neither side's cells and needed no
normalization. `osc.py` compares the wires instead.

## Gates

At `7d8b2c10`, one step at a time, `TMPDIR` short; every line below is in `gates.txt`, which is
assembled from the logs with local paths redacted.

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings`, without and with `fault-injection` | ok, ok |
| `cargo test --locked --no-fail-fast` | 5186 passed, 0 failed, 7 ignored (15 binaries) |
| `cargo test --locked --features fault-injection --no-fail-fast` | 5223 passed, 0 failed, 7 ignored -- in full |
| `check-xfx-identity`, `check-no-stubs`, `check-no-secrets`, `check-preview-contract`, `check-tui-preflight-cost` | ok |
| `scripts/fxcap/test-sweep.sh` | 26 tests, OK |
| release and faulty release builds | ok; both binaries say `build_revision=7d8b2c106c85` |
| `scripts/smoke.sh` (release) | 47 checks, 0 failures |
| `scripts/smoke-tui.sh` (release, `--faulty`) | run 1: 1139 checks, **1 failure**; run 2: **1152 checks, 0 failures** |

- **smoke-tui run 1** could not drive `3-restore-matrix` at `sigtstp-and-cont`: after `SIGCONT`
  the harness waited its 20 s for raw mode and the terminal was still cooked. Run alone three
  times on the same binaries, the scenario passed each time (48 checks, no problem), and the whole
  gate, run again, passed. The loop re-enters raw mode (`super::resume`, `mod.rs:735-743`, called
  at `event_loop.rs:628`) before anything this unit added runs (`adopt_resume`, `:639`), and
  `signals.rs` is unchanged since the base. It is reported, not explained: one failure in the
  runs this unit made, with no cause found in this change.
- **The fault-injection suite** passed in full this round. Round 1's run (on `ecd4ace2`'s tree)
  failed `tui::copy::tests::an_ending_session_lets_a_copy_that_answers_land` once; its failing
  line, its panic (`copy.rs:248`, the fake tool still holding the first copy's text) and three
  isolated passes are in `gates.txt`. It is unrelated to this unit because the test uses only
  `Copies` and a fake tool that sleeps 0.2 s (`copy.rs:238-249`), with no `Shell` and no
  `commit_counted`, and `src/tui/copy.rs` is unchanged since the base.
- **Mutants** ran on a `--no-local` clone of `7d8b2c10` (above); **the eleven round-1 `osc`
  scenarios and `autocompactfail`** were captured from a debug build of `7d8b2c10` (`xfx/`, every
  wire's banner `v0.1.0-7d8b2c1`, `green.txt`).
