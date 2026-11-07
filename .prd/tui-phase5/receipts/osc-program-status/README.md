# P5 L-U1+U2 receipt: OSC 7501 program status

Epic: lifecycle. Unit L-U1+U2 of `research/tui-phase5:.prd/tui-phase5/research/lifecycle.md`
(rows L1/L2, § "L1/L2. OSC 7501 program status"): fx v0.0.16 (`vercel-labs/fx@481104fd`)
reports what the interactive shell is doing to the terminal, and every restore includes the
clear of that record.

**S**: terminal escapes, model- and tool-controlled text on the wire, the signal handlers' bytes.

- **Base:** `origin/main` `24a6b912`.
- **Code:** `75f70b03`, the commit every `xfx/` capture's banner names (`v0.1.0-75f70b0`). It
  contains `origin/main` `e6cde019` (train 3, #79: TG-REDRAW, ENGINE-VS-ANCHOR, DIFF-WRAP), merged
  in `2f6323a3` with two side-by-side conflicts, and before it `6c93f359` (#77 tui-unsaved, #78
  the batched secrets check), merged in `33bc3fc2` with none. History:
  - `743c5381`, the unit's first commit;
  - `940878dd`, tests, comments and scenarios. **Its message says "a failure with no turn is an
    error"; the final rule is the opposite** (`eb90e677`: work that ran no turn has no result).
    The history is left as it is;
  - `eb90e677`, the work rule (round 1);
  - `ecd4ace2`, a test case;
  - `fefa5f25`, round 2's code: a refused report dropped, an automatic compaction's failure an
    error, a resume rewriting the record only after the stop handler's clear, the checker's
    decoded bound and canonical base64;
  - `7d8b2c10`, the smoke-tui oracle and one scenario (`autocompactfail`), no product code;
  - `303f0b3d`, round 3: the launch's report after its title and the resume's after its frame,
    as fx's; a stop before the session began answering no later continue; an automatic
    compaction that compacted nothing leaving the turn its own result; smoke-tui's write start
    and its raw-mode wait's diagnostics;
  - `33dffa7c`, one test's expectation of the first update's lead (the title, then the launch's
    report), no product code;
  - `afe7bda4`, the pty resume case looking for the resume's frame past the stop's restore, which
    opens with `?2026l` itself -- a surviving mutant found it -- no product code;
  - `e2a0e442`, the L-U1+U2d batch: a keystroke's report after the frame that shows it, an
    automatic compaction's error at its settle, a report riding only a title the tick did not
    replace, the lead check counting title writes, the `commit_tick` comment;
  - `7133760f`, the parity row for it;
  - `33bc3fc2`, the merge of main;
  - `b65eeedc`, smoke-tui's settled-screen wait taking a report after the last frame as no
    append, as a turn-end bell already was: the gate on `33bc3fc2` failed `23-question-panel` on
    every run, the session settling on its second question with the report after the frame;
  - `caadee7f`, a prompt's `working` held until the frame that shows its `Thinking`: the
    captures at `b65eeedc` (`prev5/`) still had it after the keystroke's frame and ahead of the
    `Thinking` frame wherever the runtime's turn-start landed a tick late;
  - `ef52945a`, the parity row for it;
  - `b0fe0c09`, round 4's receipt;
  - `17ee8cd8`, round 5's RED tests, no product code;
  - `92666749`, round 5's code: a failed automatic compaction's turn settled until its end, a
    waiting prompt held whatever got it there, the launch's report taken before the keys typed
    ahead of the session;
  - `5a0a73d9`, smoke-tui's settled-screen wait quiet after a trailing report;
  - `bfe137cc`, the parity row for it;
  - `2f6323a3`, the merge of main;
  - `75f70b03`, the `interruptnext` scenario, no product code.

  The commits after it change this receipt only.
- **prev builds,** each kept for what it got wrong and compared on the scenarios it was captured
  on (`interruptnext` is round 5's): `743c5381` (`v0.1.0-743c538`, `prev/`,
  `picker`), `eb90e677` (`v0.1.0-eb90e67`, `prev2/`, `autocompactfail`), `7d8b2c10`
  (`v0.1.0-7d8b2c1`, `prev3/`, every scenario: the launch's and the resume's report placed where
  fx's are not) and `afe7bda4` (`v0.1.0-afe7bda`, `prev4/`, every scenario: a keystroke's `working`
  ahead of the frame that shows the keystroke, an automatic compaction's `error` behind the frame
  that shows its failure) and `b65eeedc` (`v0.1.0-b65eeed`, `prev5/`, every scenario: a prompt's
  `working` after the keystroke's frame but, where the turn started a tick later, ahead of the
  frame that shows its `Thinking`).
- **RED build:** the unit's base, `24a6b912` (`v0.1.0-24a6b91`), from a `--no-local` clone,
  on every scenario (`interruptnext` captured on it in round 5). **The sweep's base** is
  `e6cde019` (`v0.1.0-e6cde01`), the train-4 base, from a `--no-local` clone.
- **fx:** the v0.0.16 reference binary, sha256 `8e6cda2e…`; every run's guard line says it was
  unchanged. The fx captures of the eleven round-1 scenarios are reused (same binary, same
  scenario files); `autocompactfail` was captured on fx in round 2, `interruptnext` in round 5.

## fx's rule, read at the pin

Every report is `ESC ] 7501 ; state=<s> : app=fx ESC \` (`src/ui/terminal/program_status.zig:20-22`,
`:143-162@481104fd`).

| | fx v0.0.16 | xfx |
|---|---|---|
| `app=` | `fx` (`:22`) | `xfx`: the user's identity decision, "a separate identity" (`.prd/01-spec.md:56`), held by `scripts/check-xfx-identity.sh:5`. Recorded in `docs/parity.md`. |
| states | `idle`, `working`, `done`, `error`, `blocked:kind=permission\|question` (`:30-41`, `:143-149`) | the same |
| activity | approval prompt, then question prompt, then `busy`, else settled (`app_worker_runtime.zig:774-784`); `busy = !cancellation_stops_turn and (stream.active or processing or queued_count > 0)` (`:762-765`), where the worker processes a prompt or a compaction and nothing else (`WorkItem`, `worker_runtime.zig:167-170`) | a permission question (either plane, or a rule change's confirmation), then the model's question, then work -- a turn running, a prompt waiting, or a manual `/compact` in hand -- unless the user stopped it or an automatic compaction failed the turn, else settled (`Shell::take_program_status`). A prompt waiting is reported from the tick whose frame first shows its turn (`Shell::status_held`): fx's pending submission is not busy until it is queued (`main.zig:1234-1266`, `input_submit_runtime.zig:310-405`, `app_auth_runtime.zig:1829-1834`), and its stop stays requested until the worker takes the next item (`worker_runtime.zig:1512`), so a held prompt, or one typed under a stop, reports nothing until it runs, as xfx's. A provider switch, a `/resume` listing, a fork and a `/workspace` are runtime work in xfx and none in fx (its picker is polled, `pollSessionPicker`, `main.zig:2930`): they report nothing (`picker`) |
| settled value | `noteTurnStarted` resets to idle (`:972`, `:985`, `:1002`); `noteTurnFinished`: completed→done, failed→error, interrupted/paused→idle (`program_status.zig:62-71`); `noteFailure` on an error-tone `error_text` (`app_worker_runtime.zig:1214-1216`), whose production senders are a failed work item's `request failed` and `worker loop crashed` (`app_process_runtime.zig:70`, `:142`, `:164`); manual `/compact`: succeeded→done, failed→error, cancelled→idle, no_op/busy→nothing, once per operation (`program_status.zig:80-96`). An automatic compaction belongs to its turn (`:79`): its error leaves the turn (`orchestrator.zig:7864-7865`) and the turn is finished `.failed` (`:5615-5618`); only a cancelled one is an interrupted turn (`:7847-7861`); one that compacted nothing ends nothing -- the turn goes on, or an overflow it could not reduce fails it (`ContextCapacityExceeded`, `:7934-7937`) | `TurnStarted` resets; `TurnEnded` **of a turn** (`tally.started`; every prompt sends `TurnStarted` first, `worker.rs:3784`): a turn an automatic compaction failed → error (noted when the compaction settles, ahead of the frame that shows `Compaction failed.`, as fx's comes in the pass that puts it on the row -- `osc/o01-failed` -- and the turn's end notes nothing: `Shell::compaction_failed`, which only `TurnStarted` and `TurnEnded` clear, since the runtime gives the place back after it sends the end (`worker.rs:3835`, `:3524`); one that settled `NoOp` or `Busy` ended nothing, and the turn's own result stands), stopped → idle, failed → error, answered → done; a `TurnEnded` with no turn -- a provider switch, failed or not -- has no result; manual `/compact` as fx, its every failure `error` (fx's uncertain publication is `.failed`, `compaction_activity.zig:198`), once per operation |
| dedup | an unchanged report is not written again, except a new result; once a result is reported, the next `working`/`blocked` turns it back to idle (`:106-128`, `4ae9eeeb`) | the same, ported with fx's own test vectors |
| `msg` | the permission's label (`request.view().label`) or the question; folded: each run of ≤0x20 / 0x7f-0x9f one space, invalid UTF-8 dropped, no leading/trailing space; at most 256 bytes ending `...` on a character boundary; standard base64 with padding; omitted when empty (`:23-24`, `:152-160`, `:164-204`). Nothing is spelled first by the reporter. | the same fold, bound and encoding (`program_status::one_line`, `crate::image::base64`). The label is `<tool> <argument>` (fx's `formatPermissionLabel` shape, `tool_presentation.zig:723-757`): fx's command is `shell.run <command>`, xfx's `terminal <command>`. |
| order | between frames (`writeProgramStatus`, `transcript/runtime.zig:10416-10429`), from the worker tick inside `loopCollectFacts` (`main.zig:2983`), which runs before the loop commits its frame (`event_loop.zig:80`, `:116-117`); its titles are written when they are set -- the launch's by the bootstrap, before the loop runs (`main.zig:728`, `app_bootstrap_runtime.zig:701`), so the launch is the two `OSC 2`, then `idle`, then the first `?2026h` (`osc/w00-start`); a turn's after the tick reported `done` (`osc/w02-done`: `done`, `OSC 2`, the frame); and it handles the keys after it reports (`src/ui/event_loop.zig:78-117`), so what a key did is on that pass's frame and reported by the next (`osc/w02-done`: the prompt's `Thinking` frame, then `working`) | between frames, where fx's land (`event_loop::commit_tick`): on its own ahead of the tick's frame, a title the tick's own events asked for behind it -- the turn's end is `done`, `OSC 2`, the frame on both; **after a title the band already owed when the tick began** and the tick did not replace, riding that frame's lead between the title and its `?2026h` (`commit_primary`; `check::preflight_update` admits exactly one declared report there, and no title write after it) -- the launch's title is the band's before the first tick (`launch_title`), so the launch is `OSC 2`, `idle`, `?2026h`, as fx's, and the launch's report is taken before the keys typed ahead of the session are read (`Shell::hold_launch_status`), so it is `idle` whatever they did; **after the frame a keystroke's tick commits**, when this session's own input moved the status to work or to a question (`Shell::take_status_moved_by_input`), so the prompt's `Thinking` frame comes before `working`, as fx's. A stop is not one: fx shows the stop only when the turn has (`osc/i01-after-esc`: `idle`, then `Cancelled`), and xfx's keystroke writes that line, so the stop's `idle` stays ahead of it. The band stays the title's only writer (`frame.rs:824-829`). fx writes the launch title twice (both syncs), xfx once. On the other plane or a screen no band fits on the report does not ride: it goes out on its own ahead of the frame, as on any tick; only a tick whose band owes such a title and builds no frame at all writes it on its own after the frame. |
| clear | `ESC]7501;state=clear ESC\` (`:18`) right after `?2004l` in all four restores, normal and abnormal, with and without tmux (`app_lifecycle.zig:43-48`); the abnormal ones written by the SIGTERM/SIGHUP handler in one `write(2)` (`:59-75`, installed `:87-105`); suspend writes the normal restore (`:1157-1165`) | right after `?2004l` in all four restores (`term::RESTORE*`, `ABNORMAL_RESTORE*`); the abnormal ones are what `restore_pair` writes in one `write(2)` from the SIGTERM/SIGHUP/SIGINT handlers, the stop handler and the panic hook. A `const` assertion fails the build for a restore without it. The restores **include** it; a restore the descriptor takes only part of, or a failed handler `write(2)`, can lose it, as it can lose the rest of the restore. |
| resume | `defer program_status.invalidate()` around its own suspend, which repaints first (`main.zig:1050-1055`): the next report is written even if unchanged, after the resume's frame (`osc/s02-resumed`: the frame, then `idle`), and a result already reported comes back idle (`program_status.zig:98-104`) | the stop owes the invalidation to the resume's tick (`Shell::owe_program_status_resume`), **only when the stop handler wrote the abnormal restore** (`plane_restored`), so a bare `SIGCONT`, or a `SIGSTOP` and its `CONT`, leaves the record as it was; that tick writes no report ahead of its frame and the one it owes after it (`commit_tick`), so the resume is the frame, then the report, as fx's. A stop before the session began leaves no record for a later continue to read (`signals::install` takes it with the resume flag). |
| a refused write | the report is recorded as written before the write is tried (`program_status.zig:124-127`) and a failed write is only traced (`runtime.zig:10422-10428`): dropped | dropped, the same way: the report was recorded before the write, and the refusal is disposed of as any coordinate-free vector's (it counts against the frame budget; a torn one ends the session). A report riding a frame's lead shares that update's fate: dropped when it is refused; a tear in the lead is recovered, and the cleanup vector's `CAN` aborts the string |
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

Round 3 (`303f0b3d`), from the two reviews of `256eb7ac`:

- **The launch's report after its title, ahead of the first `?2026h`** (both reviews' MUST). fx's
  bootstrap writes the title before its loop runs (`main.zig:728`, `app_bootstrap_runtime.zig:701`).
  The launch's title is now the band's before the first tick (`event_loop::launch_title`), and a
  report whose tick begins with a title the band already owes rides that frame's lead, after the
  title (`commit_tick`, `commit_primary`). The band stays the title's only writer;
  `check::preflight_update` admits exactly one declared report after the title, and refuses one
  ahead of it, a second one, another state and anything else (`Update::status`;
  `a_lead_admits_one_declared_report_after_its_title_and_nothing_else`). A title the tick's own
  events asked for still follows the report, as fx's turn sets its title after reporting `done`
  (`osc/w02-done` on both): the general rule, not a launch special case. Tests:
  `the_launchs_report_follows_the_title_it_starts_with_and_leads_its_first_update`,
  `a_title_its_own_tick_asked_for_follows_that_ticks_report`, and the pty case
  `the_session_reports_its_status_between_frames_…` (`OSC 2`, then `idle`, then `?2026h`).
- **The resume's report after the frame it repaints** (opus's MUST). fx repaints inside
  `suspendToJobControl` and invalidates on the way out (`main.zig:1050-1055`; `osc/s02-resumed`).
  The stop owes the invalidation to the resume's tick (`Shell::owe_program_status_resume`), which
  writes no report ahead of its frame and the one it owes after it.
  `a_resumes_report_follows_the_frame_the_resume_repaints`, and the pty `sigtstp_…` case.
- **A stop before the session began answers no later continue** (`signals::install` takes the
  stop handler's restore record with the resume flag).
  `a_stop_before_the_session_began_gives_no_plane_back_to_a_later_continue`.
- **An automatic compaction that settles `NoOp` or `Busy` ends nothing** and the turn's own result
  stands, as fx's in-turn compaction that compacted nothing goes on or fails only for an overflow
  (`orchestrator.zig:7934-7937`). xfx's worker never settles an automatic compaction that way
  today (it settles one only with `TooLarge`, `Cancelled`, `Failed`, `Unconfirmed` or
  `Succeeded`: `worker.rs:4077-4083`, `:4471-4476`), so this is the rule made explicit, not a
  wire change. `an_automatic_compaction_that_compacted_nothing_leaves_the_turn_its_own_result`.
- **The torn report, replayed.** `a_report_the_terminal_took_part_of_…` feeds what the terminal
  took and then each restore to a parser's string states (`strings_ended`: `ESC` ends a string,
  `CAN`/`SUB` abort it, `BEL` ends an `OSC` -- the VT500 state machine's, a model and not a
  terminal): every restore ends the torn string, its clear is a string of its own, and the parser
  ends in ground. `a_report_torn_in_the_lead_of_an_update_is_ended_by_the_cleanup_and_dropped`:
  one torn in a lead is a recovered tear whose cleanup's `CAN` aborts it, and it is not offered
  again.
- **`scripts/smoke-tui.sh`**: the write start admits the launch's report after the title
  (`TITLE_LEAD`); `wait_until_raw`, when it gives up, records the child's process state, the bytes
  the master held unread and the last bytes on the wire (`never-raw.txt`) -- diagnostics for
  round 2's one `sigtstp-and-cont` failure, not a fix.
- `docs/parity.md`: the row's order sentence is now true for the launch, the ticks and the resume;
  the launch-order difference is gone; the torn-write table's program status row covers a report
  riding a lead.

Round 4 (L-U1+U2d: `e2a0e442`, `7133760f`; merged with main in `33bc3fc2`), from the two confirm
reviews of `46b2c6eb` (both APPROVE) and the dispatcher's follow-up:

- **A keystroke's report after the frame that shows the keystroke** (opus A; the dispatcher's
  decision). fx's loop reports while it collects facts and handles the keys after
  (`src/ui/event_loop.zig:78-117`), so what a key did is on that pass's frame and reported by the
  next: `osc/w02-done` has the prompt's `Thinking` frame, then `working`, where xfx wrote `working`
  first -- and the same for a permission or a question answered and a `/compact`. The shell now
  notes when its own input moves the status to work or to a question
  (`Shell::take_status_moved_by_input`, set across `consume`), and that tick writes its report after
  its frame (`commit_tick`). **A prompt waits one step more** (`caadee7f`): fx's keystroke frame
  already shows the prompt's `Thinking`, and xfx's band shows it once the runtime says the turn
  started, which lands in the keystroke's tick or a later one -- so a status a keystroke moved to
  work only through a prompt not yet started writes no report (`Keyed::NotYet`) until the tick
  whose frame first shows the turn, and follows that frame. **A stop is not one**: fx shows a stop only when the turn has
  (`osc/i01-after-esc`: `idle`, then `Cancelled`), and xfx's keystroke writes that `Cancelled`
  line, so a stop's `idle` stays ahead of its frame. Tests:
  `a_working_a_keystroke_caused_follows_the_frame_that_shows_the_keystroke`, the guard
  `a_stop_a_keystroke_asked_for_is_reported_ahead_of_the_frame_that_shows_it`, and the pty case
  (`Thinking` before `working`).
- **An automatic compaction's failure is the turn's `error` at the settle.** Round 3's `order`,
  which compared the kind of sequence nearest each report, showed xfx writing `Compaction
  failed.` and then `error`, where fx writes `error` and then the row: fx's failure and the turn's
  end come in one pass, and xfx's runtime says them apart. (Round 4's `osc.py` replaced that
  comparison and no longer checked this order; round 5's `Compaction failed` claim does.) The
  result is now noted when the compaction settles, and the turn's end notes nothing
  (`an_automatic_compaction_that_fails_inside_a_turn_is_that_turns_error`, changed).
- **A report rides a title the band already owed only if the tick did not replace it** (opus
  SHOULD 3, PLAUSIBLE, RED with a refused frame:
  `a_title_its_tick_asked_for_over_one_still_owed_follows_the_report`). xfx wrote the new title,
  the report, the frame; fx writes the report, the new title, the frame. The tick's own title
  (`Shell::owes_title`) now keeps the report on its own, ahead.
- **`osc.py`'s `order` compares which frame a report follows** (below), and fails round 3's wires.
- **The lead check counts title writes** (`TerminalModel::titles`), so an `OSC 2` with the same text
  inside the report's slice is refused (`a_lead_admits_one_declared_report_…`).
- **The `commit_tick` comment** says what the code does: the other plane and a screen no band fits
  on put the report ahead of the frame; only a tick that held it for a lead and built no update
  writes it after.
- **Main merged** (`33bc3fc2`): `origin/main` `6c93f359`, no conflict; the gate runs main's two new
  scripts, `scripts/test-check-no-secrets.sh` and `scripts/test-gate-env.sh`.
- **`scripts/smoke-tui.sh`** (`b65eeedc`): the gate on `33bc3fc2` failed `23-question-panel` on
  every run, alone too (`gates.txt`): its settled-screen wait (`wire_ends_on_a_frame`) wants the
  wire to end on a frame's close, and a session waiting on its second question now ends on the
  report the keystroke caused, after the frame. A whole `OSC 7501` report at the end of the wire is
  no append, as a turn-end bell already was.

Round 5 (L-U1+U2e: `17ee8cd8`, RED; `92666749`, the fix; `5a0a73d9`, smoke-tui; `bfe137cc`, the
parity row; `2f6323a3`, main; `75f70b03`, a scenario), from the two reviews of `b0fe0c09`:

- **An automatic compaction's failure stays the turn's `error` until the turn's end** (opus MUST,
  RED: `an_automatic_compaction_failure_whose_place_comes_back_before_the_turn_ends_is_one_error`,
  `left: Some("working")` on `ef52945a`, `red5-test.txt`). The runtime sends `TurnEnded`
  (`worker.rs:3835`) before it gives the turn's place back (`turn_loop`'s `fetch_sub`, `:3524`), so
  the band's settle can find no place held with the `TurnEnded` still in the channel. It cleared
  `cancelling` and `compaction_failed` there, the turn's activity still read as work, and the wire
  went `error`, `working`, `error`. The settle no longer clears `compaction_failed` (`TurnStarted`
  and `TurnEnded` do), and the status reads a turn it marks as settled; the activity row is not
  ended at the settle, so no cell changes. The comment over the settle said no event about
  cancelled work could still come; it now says what can.
- **A prompt typed after a stop, before the stopped turn ends, is `working` after the frame that
  shows its `Thinking`** (opus SHOULD 1, RED:
  `a_prompt_typed_after_a_stop_is_reported_after_the_frame_that_shows_its_thinking` -- the
  `working` was the first vector of the stopped turn's end, before any frame of the prompt's turn).
  The stop kept the status settled across the keystroke, so nothing marked the prompt as typed.
  fx's stop stays requested until its worker takes the next item (`takeNextWorkLocked`,
  `worker_runtime.zig:1512`; `finishProcessing` leaves it, `:1541-1563`), nothing is busy while it
  is (`app_worker_runtime.zig:785-788`, `:762-765`), and the prompt's keystroke frame shows its
  activity, which reads no cancel (`pendingPromptActivityVisible`, `app_render_runtime.zig:286-290`):
  the frame, then `working`. Work that is only a prompt waiting is now held whatever got it there
  (`Shell::status_held`), and the tick whose frame first shows the turn writes the report after it;
  a hold that ends without its turn owes no frame anything (`take_status_moved_by_input`). The exact
  window -- a prompt typed before the stopped turn's `TurnEnded` -- is a few milliseconds and no
  scenario can type into it; `interruptnext` types the second prompt 1.5 s after the stop, where
  fx's stop is still requested (the same fx mechanism) and xfx's stopped turn has ended: both write
  the frame, then `working`.
- **A prompt held behind other runtime work: no difference on the wire** (opus SHOULD 2, gpt6 NTH
  1). fx adopts a prompt at the keystroke and queues it only once it may run
  (`adoptPendingUserPrompt`, `finalizePendingSubmission`, `main.zig:1234-1266`), holding it while
  the provider is prepared (`collectPendingSubmissionFacts`, `input_submit_runtime.zig:310-405`,
  whose `.pending` returns before the queue; `collectPendingPromptCredential`'s
  `providerPreparationPending`, `app_auth_runtime.zig:1829-1834`); its `busy` counts a prompt once
  it is queued (`queued_count`, `app_worker_runtime.zig:762-765`). So fx reports nothing while it
  holds a prompt, and nothing at all if the prompt is dropped first -- as xfx's hold does. No
  scenario reaches either hold (fx's lasts as long as a provider's preparation; xfx's as long as a
  provider switch, a `/workspace` or a fork the runtime is running), so the tests pin every exit:
  `a_held_prompt_reports_nothing_until_its_turn_starts_and_nothing_if_it_never_does` -- its turn
  starting (the frame, then `working`), a stop, a reset (`/new`) and a replaced session before it
  starts (nothing), and nothing of the hold left for a later tick. It passes on `ef52945a` too: a
  pin, not a RED. The parity row no longer promises `working` for any waiting prompt; it says this.
- **The launch reports `idle` whatever was typed ahead of the session** (opus NIT, RED:
  `keys_typed_ahead_of_the_session_are_read_after_its_launch_reports_idle` -- no report at all on
  the first tick, then `working`). `session` routes the launch probe's leftovers before the first
  tick, where fx's first pass reports before it handles a key (`src/ui/event_loop.zig:78-117`).
  `launch_title` now takes the launch's report before those keys are read
  (`Shell::hold_launch_status`), and the first tick writes it on the first frame's lead whatever the
  keys did; what they did is reported as any keystroke's. A field, two methods and two lines in the
  loop.
- **`osc.py`'s which-frame claim asks three things, each counted** (opus SHOULD 3, gpt6 NTH 2): a
  `working` after a settled report and `Thinking`, as before; **a `working` after a `blocked`
  report and whether any frame lies between them**; and **an `error` and whether a frame since the
  report before it showed `Compaction failed`**. The last fails `prev4` (`afe7bda4`: 34 failing
  claims before it, 35 with it -- the error behind the frame that shows the failure, on
  `autocompactfail`'s `o01-failed`), `prev3` (72, 73) and `prev2` (5, 6: no error at all there);
  green passes it, on `ef52945a`'s wires and on `75f70b03`'s. Two wire mutants, one for each new
  claim, each failing exactly that claim.
- **smoke-tui's settled-screen wait** (opus SHOULD 4): a wire that ends on a frame only once a
  trailing report is set aside must also stay quiet for one more read (`REPORT_QUIET`, 50 ms, six
  of the loop's ticks) before the screen counts as settled.
- **Wording**: this README credited the tightened `order` with finding the automatic compaction's
  error order, which round 4's final `osc.py` did not check; it now says that round 3's
  nearest-kind comparison found it and this round's `Compaction failed` claim checks it. And
  `Keyed` and `StatusActivity` no longer sit between `/fast`'s doc comment and `SpeedCommand`
  (`caadee7f` put them there).
- **Main merged** (`2f6323a3`): `origin/main` `e6cde019` (train 3). Two conflicts, both additions
  side by side: `event_loop.rs`'s test module and `docs/parity.md`'s ui rows (this unit's
  full-screen TUI and program status rows, main's full transcript and coloured output rows).
- **Outside this unit, for a backlog unit**: the same window leaks a `✗ system:` line after a stop.
  A scratch test (`a stopped turn whose place comes back before its end is read`: Ctrl-C, the
  place given back, the settle, then `TurnEnded { failure: Some("xfx: the turn was cancelled") }`)
  fails on a `--no-local` clone of `75f70b03` with the document
  `["  a whole line", "", "■ Cancelled · What can xfx do differently?", "", "✗ system: the turn
  was cancelled", ""]`: the settle cleared `cancelling`, which is what keeps a stopped turn's
  failure silent. Not fixed here; the test is not committed
  (`system-leak.txt`).

## Decisions

- **`app=xfx`.** The user's identity decision (`.prd/01-spec.md:56`). `osc.py` normalizes the
  name, by name.
- **The clear right after `?2004l`.** xfx's restores are in a different order from fx's; the
  property kept on both sides is "the clear follows `?2004l`", which `osc.py` checks on both wires.
- **The report before the frame,** as fx's tick reports before it commits its frame -- except
  after a title the band already owed (the launch's), which fx has already written, after a
  resume's repaint, which fx makes before it invalidates, and after a keystroke's frame, which fx
  commits before the pass that reports what the key did.
- **The keystroke rule is by what the input did, not where it came from**: work or a question.
  A stop is the exception because xfx writes `Cancelled` at the keystroke where fx writes it with
  the turn's end; deferring the stop's `idle` would put it behind that line, where fx's is ahead.
- **The report rides the frame's lead rather than the title being written apart**: the band stays
  the title's only writer. Rounds 1 and 2 left the launch's order uncopied for that reason; the
  reviews held it was no allowed one, and the lead holds both.
- **`busy` is a turn, a waiting prompt or a manual `/compact`.** fx's worker runs a prompt or a
  compaction and nothing else, so its `busy` covers those alone. A waiting prompt counts from the
  frame that first shows its turn: fx's own prompt is not busy while it is held or while a stop is
  still requested, so reporting it earlier would be a `working` fx never writes there. `743c5381` counted all runtime
  work and wrote `working, idle` for a `/resume` listing where fx writes nothing (`picker`; fixed
  in `eb90e677`).
- **A result is a turn's.** A `TurnEnded` with no turn has no result; an automatic compaction's
  failure inside a turn is that turn's `error`; one that compacted nothing leaves the turn its own.
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
| `interruptnext` | the same, then a second prompt typed 1.5 s after the stop and held in its turn, then done; `/quit` |
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
ended), `inert` (every `msg` base64 decoding to at most 256 bytes of UTF-8 with no control), and
`order`, rebuilt in round 4 and widened in round 5. It asks two things. **Where** each report sits: the first
report is preceded, since the mode set (`?2004h`), by a window title and followed by the first
frame's `?2026h` -- the launch's `idle` after its title, right ahead of the first frame -- and
every report has a frame between it and the last mode set exactly when fx's does, so a resume's
report comes after the frame the resume repaints. And **which frame** a report follows, for three
kinds of report, each a claim of its own: a `working` after a settled report (`idle`, `done`, `error`) -- work a keystroke began -- and whether
a frame since the report before it showed `Thinking`; a `working` after a `blocked` report -- a
question or a permission answered -- and whether there was a frame between the two at all; and an
`error`, and whether a frame since the report before it showed `Compaction failed`. Each is what it
is on fx. Round 3's `order` compared the kind of sequence nearest each report; that passed round 3's
early `working` (both after a `?2026l`), and it failed round 4's correct wires wherever fx's timer
painted a `(0s)` frame between two reports and xfx's did not, so it is gone. A frame's ordinal is
not comparable either (fx writes frames xfx does not -- its cursor probe's, a timer's -- and xfx
writes a band in fewer), so a frame is told by what it showed. What the frame after an answer shows
is not compared: the activity row there is each product's own (xfx's names the running tool; fx
repaints the whole screen), and a compaction's progress row is on a frame only as long as its
request takes. A kept build (`prev*`) is compared on the scenarios it was captured on:
`interruptnext` is newer than `prev3` to `prev5`.

**Three normalizations**, each named in its header: `app-name`; `command-tool-name` (`shell.run`
/ `terminal`); `question-breaks` (xfx's question tool spells a line feed and a tab before the
reporter sees them). `question-breaks` **expires with** the `docs/parity.md` row that records the
difference (owner unit **QUESTION-BREAKS**).

## RED

- **Base `24a6b912`:** `python3 -I -B osc.py . red 24a6b91` (`red.txt`): 43 wire pairs, **51
  failing claims** -- the base writes no `OSC 7501` at all (`same` fails on all 43, and `last` on
  the eight wires that end a session, which carry no clear).
- **`743c5381`, `picker`:** `osc.py . prev 743c538 p5-lifecycle-osc-picker` (`prev.txt`): 4 wire
  pairs, **5 failing** -- `same` on three, for the `working, idle` it wrote for the `/resume`
  listing, and `order` twice on the fourth: the launch's `idle` ahead of its title, and the
  prompt's `working` ahead of its `Thinking` (where the report counts differ, `same` has already
  failed and `order` is not asked).
- **`eb90e677`, `autocompactfail`:** `osc.py . prev2 eb90e67 p5-lifecycle-osc-autocompactfail`
  (`prev2.txt`): 2 wire pairs, **6 failing** -- `same` on the one where it reported `idle` where
  fx reports `error`, `order` twice on each (the launch's placement and the prompts' `working`),
  and round 5's `Compaction failed` claim on that one: no `error` where fx's comes ahead of the
  frame that shows the failure.
- **`7d8b2c10` (round 2), every scenario it was captured on:** `osc.py . prev3 7d8b2c1`
  (`prev3.txt`): 38 wire pairs, **73 failing**, all `order`: the placement on all 38 (the launch's
  `idle` against the mode set, ahead of its title; on the two `suspend` wires past the stop, the
  resume's `idle` too), the prompts' `working` ahead of their `Thinking` on 34, and the automatic
  compaction's `error` behind the frame that shows `Compaction failed.` on `o01-failed`. The
  reports themselves were fx's.
- **`afe7bda4` (round 3), every scenario it was captured on:** `osc.py . prev4 afe7bda`
  (`prev4.txt`): 38 wire pairs, **35 failing**, all `order`'s which-frame half: every pair with a
  prompt's `working` has it ahead of the frame that shows its `Thinking` -- round 3's reviews' item
  A -- and, round 5's claim, `o01-failed` has the automatic compaction's `error` behind the frame
  that shows `Compaction failed.`, where fx's is ahead of it (34 before that claim). The placement
  holds.
- **`b65eeedc` (round 4's first build), every scenario it was captured on:** `osc.py . prev5
  b65eeed` (`prev5.txt`): 38 wire pairs, **14 failing**, the same half: the `working` after the
  keystroke's frame, but ahead of the `Thinking` frame wherever the runtime's turn-start landed a
  tick later -- what `caadee7f` holds back.
- **Round 3's tests** on round 2's code with only the `launch_title` helper they call added
  (`red3-test.txt`): the launch, the resume, the startup stop and the compaction that compacted
  nothing fail, 4 of 5; the fifth -- a title the tick's own events asked for follows the report
  -- is the regression guard for the turn's end, which already matched fx, and passes there too.
- **Round 4's tests** on round 3's code (`red4-test.txt`): the keystroke's `working`, the
  replaced title and the automatic compaction's `error` at its settle fail, 3 of 4; the fourth --
  a stop's `idle` ahead of the frame that shows `Cancelled` -- is the guard for the stop, which
  already matched fx, and passes there too.
- **Round 5's tests** on `ef52945a`'s code (`17ee8cd8` adds tests only; `red5-test.txt`): the
  automatic compaction's place coming back before its turn's end (`left: Some("working")`), the
  prompt typed after a stop (the `working` the first vector of the stopped turn's end) and the
  keys typed ahead of the session (no report on the first tick) fail, 3 of 4; the fourth -- a held
  prompt's every exit -- passes there too: xfx's hold already matched fx's, and the test pins it.
- **Tests:** `tests/tui.rs` (unchanged since `743c5381`) against the base's code
  (`red-test.txt`): 5 of 7 fail -- `the_session_reports_its_status_…` (no launch `idle`), and
  `a_normal_exit_…`, `sigterm_…`, `sighup_…`, `sigtstp_…` (the restore with its clear never
  arrives). The two guards (`an_ask_on_a_terminal_…`, `zero_is_the_line_shell`) pass on the base
  too, as they should: the base writes no report anywhere.

## GREEN (`75f70b03`)

`python3 -I -B osc.py . xfx 75f70b0` (`green.txt`): **43 wire pairs, 0 failing claims**, every
`order` claim included; every xfx wire carries `v0.1.0-75f70b0` and no other banner. The reports
are the ones round 2 wrote; round 3 moved the launch's after its title and the resume's after the
frame it repaints, round 4 moved every prompt's `working` after the frame that shows its
`Thinking` and the automatic compaction's `error` ahead of the frame that shows `Compaction
failed.`; round 5's changes are in states no scenario can type into, and `interruptnext` pins the
one next to them (a prompt typed under fx's still-requested stop).

| scenario | wires | the reports (normalized), the last wire |
|---|---|---|
| `states` | 10 | idle, working, done, working, done, working, error, working, blocked:permission `<command-tool> echo hi > out.txt`, working, done, working, blocked:question `Which branch?`, working, done, clear |
| `hostile` | 3 | idle, working, blocked:permission `<command-tool> echo \u{202e}evil\u{202c}\x09done > out.txt`, working, done, working, blocked:question `Pick\x1b]2;owned\x07 a\u{0085}branch\u{202e}? now \u{009b}31m`, working, done, clear |
| `interrupt` | 3 | idle, working, idle, clear |
| `interruptnext` | 5 | idle, working, idle, working, done, clear |
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
- **A torn report** written on its own ends the session, and the restore that follows opens with
  `ESC`: replayed through a parser's string states, what the terminal took and each restore end
  the torn string, the restore's clear is a string of its own, and the parser ends in ground
  (`a_report_the_terminal_took_part_of_…`). One torn in a frame's lead is a recovered tear whose
  cleanup opens with `CAN`, which aborts the string; it is dropped
  (`a_report_torn_in_the_lead_of_an_update_…`). The parser is a model of the VT500 state
  machine's string states, not a terminal.
- **No status outside the TUI.** `ask` on a terminal, a subcommand, the line shell (with a whole
  turn) and every refused launch write no `ESC]7501;` (`TUI_BYTES`, the two new assertions, and
  the `ask` capture on both products). fx's `program_status` has no reader but its interactive
  `App`.

## Recorded differences, each with an owner

| difference | where | owner |
|---|---|---|
| `app=xfx` | `terminal program status (OSC 7501)` row | the user's identity decision, `.prd/01-spec.md:56` |
| a typed Ctrl-Z binds nothing; only `SIGTSTP` stops | `full-screen TUI` row | **CTRL-Z** (queued) |
| a question's line feed and tab reach the reporter spelled (`…?\x0a\x09now…` on xfx, `…? now…` on fx) | `a question's line breaks in a program status message` row | **QUESTION-BREAKS** (queued); `osc.py`'s `question-breaks` expires with it |
| a run of 5xx ends xfx's turn after three attempts, about a second in; the status reads `error` where fx's reads `working`, which is why `retry` reads the retrying turn at 0.3 s only and the stop's `idle` is `interrupt`'s | `transport-owned retry` row | **RETRY-BUDGET** (queued) |

## Mutants

**Code** (`mutants.py`, `mutants.txt`): forty-eight one-line mutants, run on a `git clone
--no-local` of `75f70b03` in the scratchpad (never the worktree), against the unit tests this unit
added or touched (a build, for the two restore literals; the pty cases, for what only the session's
own loop does). **All 48 killed**, each of round 5's eight by a round-5 test.

- the abnormal restore without its clear; the tmux restore with its clear moved to the end --
  **the build fails** (`const` assertion, `E0080`);
- `msg` raw instead of base64; not folded; not bounded -- fx's vectors, the security case and the
  encoder/checker agreement case fail;
- an unchanged report written again; a second result in a row not reported; a reported result
  repeated after work that ran no turn -- the ported dedup cases fail;
- a resume that does not report again; **any continue rewriting the record** and **a refused
  report offered again** (round 1's rules); the report written after the frame; **the launch's
  report ahead of its title** and **a resume's report ahead of its frame** (rounds 1-2's orders,
  the second on a real terminal too); the launch's title not handed to the band before the first
  tick (pty) -- the loop's cases fail;
- round 5's: **a failed compaction's turn `working` again once its place is back** and **the
  band's settle clearing the failed compaction's mark** (round 4's race, the opus MUST); **a prompt
  held only when typed**, so one typed after a stop is reported ahead of its `Thinking` (round 4's);
  a held prompt's start reported ahead of the frame that shows it; a held prompt's hold left over
  for a later tick; a held prompt reported while it waits (`b65eeedc`'s); **the launch's report
  taken after the keys typed ahead of it** (round 4's) and held back by a held first tick -- the
  new cases fail;
- round 4's: **a keystroke's `working` ahead of the frame that shows it** (rounds 1-3's order,
  on a real terminal too); a prompt's `working` before any frame shows its turn; a stop's `idle`
  held behind the frame that shows `Cancelled`; a report riding a title the tick's own events
  replaced; **an automatic compaction's failure reported idle** (round 1's rule), the turn's end
  reporting it again, and the turn still `working` until it ends (rounds 2-3's rule) -- the loop's
  and the shell's cases fail;
- a stop before the session began answering a later continue -- the signal case fails; an
  automatic compaction that compacted nothing failing its turn -- the shell's case fails;
- a stopped turn still `working`; a waiting prompt or a manual `/compact` not `working`; any
  runtime work `working` (`743c5381`'s rule); work with no turn given a result; a stopped turn
  reported as its failure -- the shell's cases fail;
- the output check admitting a decoded control, a `BEL`-terminated report, a non-canonical base64
  tail, a message past 256 decoded bytes, a restore with no clear; the lead check admitting a
  report ahead of its title, one it was not told of, or a title written again in the report's
  slice -- the check's and the restore's cases fail.

Earlier runs, superseded: `743c5381` (16 mutants, one survivor: a failure with no turn not
reported -- that is now the rule), `eb90e677` (19, one survivor: the `!cancelling` guard, killed
by `ecd4ace2`'s case), `7d8b2c10` (26, all killed), `33dffa7c` (34, one survivor: the vacuous pty
resume case `afe7bda4` fixed), `afe7bda4` (34, all killed) and `ef52945a` (40, all killed).

**Checker** (`wire_mutants.py`, `wire-mutants.txt`): seventeen defects written into a copy of the
receipt's xfx wires (`75f70b0`'s) -- the exit's clear dropped, the clear moved after `?25h`, a
report after the clear, a report inside a frame, a raw label, a label decoding to ESC, a report
lost, another build's banner, **one wire with no banner while the others carry it**, **an
unterminated report after the exit's clear**, a report ended by `BEL`, an 8-bit introducer after
the clear, **the launch's `idle` ahead of its title**, **a resume's report ahead of the frame it
repaints**, round 4's **keystroke's `working` ahead of the frame that shows it** (still right after
a frame's `?2026l`, so only `order`'s which-frame half can tell) and, round 5's, **an automatic
compaction's `error` behind the frame that shows its failure** and **an answer's `working` with no
frame after its question** -- and `osc.py` fails each with the claim named (the last two with
exactly the new `Compaction failed` and `a frame` claims, one failing claim each), and passes the
unchanged copy.

## Sweep

**Every scenario**, round 5: `scripts/fxcap/sweep.sh --jobs 3` with no `--only`, fx v0.0.16
against the train-4 base's debug build (`e6cde019`, from a `--no-local` clone) and then the head's
(`75f70b03`), each named by its banner in its scoreboard (`gates.txt`, "the parity sweep"). The
`p5-lifecycle-osc-*` scenarios run in it and take no snap, so they add no pair.

| xfx | identical | normalized-identical | approved (and cascade) | unresolved | failed / timeout |
|---|---|---|---|---|---|
| base `e6cde019` | 39 | 998 | 13 + 26 | 614 | 2 / 0 |
| head `75f70b03` | 36 | 981 | 13 + 25 | 602 | 2 / 33 |
| the five scenarios below, locked: base / head | 30 pass / 30 pass | | | 24 / 24 | 0 / 0 |

1692 snap pairs on each side (`pass = 1037 / 1692` on the base, `1017 / 1692` on the head). The
two failed pairs are the same on both: `p5-panels-badsettings`, on fx's side (`fx: exit 1`).

Pair by pair against the base (`gates.txt`, "the parity sweep"): **6 gained**
(`model-column-reask/col-back`, `model-parked/menu-loading`, `model-reask/menu-failed`,
`p5-input-model-fail/fail-open`, `resume-paging/query-ve`, `resume-paging/query-ve-down3`, each
unresolved to normalized-identical), **2 moved within a pass** (`p5-transcript-submit/filled` and
`/second`, identical to normalized-identical), and **26 lost plus 9 other changes, all from five
scenarios**: 31 pairs became `timeout` -- `image-composer`, `p5-effort-fork-fork` and
`rules-read-auto`, three xfx runs one after another (runs 40-42 of 653) that passed their deadlines
(110-115 s) while fx's own runs of the same three scenarios took 78-102 s against 38-46 s on the
base run -- and two became unresolved on a timed snap: `interrupt-retry/r0.15` (at 0.15 s xfx
still reads `Thinking` where fx already shows the retry notice) and `p5-transcript-resize/start`
(xfx's `Full access enabled` toast still up where fx's is not). The machine is shared, and the
sweep ran before the suite lock existed. **Those five scenarios again, under the suite lock, on
both builds**: 55 pairs, 30 pass on each, **0 gained, 0 lost, 0 changed** against the base's
rerun and against the base's full run. No pair drops.

The comparator reads snaps (`capture-pane`), which carry no OSC, so the reports reach neither
side's cells and needed no normalization; `osc.py` compares the wires instead.

## Gates

At `75f70b03`, every step of `ci.yml` and `sweep.yml`'s two test scripts, one step at a time,
`TMPDIR` short, the heavy ones under the machine's suite lock (two heavy jobs at a time); every
line below is in `gates.txt`, which is assembled from the logs with local paths redacted.

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings`, without and with `fault-injection` | ok, ok |
| `cargo test --locked --all-targets --no-fail-fast` (locked) | 5268 passed, 0 failed, 7 ignored (14 binaries) |
| `cargo test --locked --features fault-injection --no-fail-fast` (locked) | 5306 passed, 0 failed, 7 ignored (15 binaries) -- in full |
| `check-no-stubs`, `check-no-secrets`, `check-xfx-identity`, `check-preview-contract` | ok |
| `test-check-no-secrets`, `test-gate-env`, `test-preview-contract` | ok (13 cases, 3473 tracked names past `ARG_MAX`), ok (5 cases), ok (24 mutants caught) |
| `check-tui-preflight-cost`, `check-usage-clock` | ok, ok |
| `scripts/fxcap/test-sweep.sh`, `scripts/fxcap/test-capture.sh` (locked) | 50 + 26 tests OK, 42 tests OK |
| release and faulty release builds | ok; `xfx status` of each says `build_revision=75f70b03d260` |
| `scripts/smoke.sh` (release, locked) | 47 checks, 0 failures |
| `scripts/smoke-tui.sh` (release, `--faulty`, locked) | 37 scenarios + the oracle, 1168 checks, 0 failures |

- **Round 4's gate records** (the merge `33bc3fc2` failing smoke-tui's `23-question-panel`, which
  `b65eeedc` fixed; round 1's one fault-injection failure in `tui::copy`, with its isolated
  passes) are in `gates.txt` as of `b0fe0c09`; `src/tui/copy.rs` is still unchanged since the base.
- **Mutants** ran on a `--no-local` clone of `75f70b03` (above); **every `osc` scenario** was
  captured from a debug build of `75f70b03` (`xfx/`, every wire's banner `v0.1.0-75f70b0`,
  `green.txt`), and `interruptnext` on fx and on the RED build too.
