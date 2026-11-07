"""L-U1+U2 mutants, run on a --no-local clone (never the worktree).

usage: python3 -I -B mutants.py <clone> <target-dir> <out-file>

Each mutant is one exact replacement in one file. The clone is restored with
`git checkout -- .` after each. A mutant is killed when its command fails.
"""
import os
import subprocess
import sys

clone, target, out = sys.argv[1:4]
env = dict(os.environ, CARGO_TARGET_DIR=target, TMPDIR="/private/tmp/claude-501/osc")
for name in ("GIT_DIR", "GIT_INDEX_FILE", "GIT_WORK_TREE"):
    env.pop(name, None)

UNIT = [
    "tui::program_status",
    "tui::check::tests::a_program_status",
    "tui::check::tests::a_restore_that_lost",
    "tui::term",
    "tui::event_loop::tests::a_program_status",
    "tui::event_loop::tests::a_refused_program",
    "tui::event_loop::tests::a_resume_writes",
    "tui::shell::tests::program_status",
    "tui::shell::tests::a_turn_the_user_stopped",
    "tui::shell::tests::a_resume_reports",
    "tui::shell::tests::a_rule_confirmation_is",
    "tui::shell::tests::a_manual_compaction_reports",
    "tui::shell::tests::an_automatic_compaction_that_fails",
    "tui::event_loop::tests::a_continue_no_handler",
    "tui::event_loop::tests::every_report_the_encoder",
    "tui::event_loop::tests::a_report_the_terminal_took_part",
    "tui::event_loop::tests::the_launchs_report",
    "tui::event_loop::tests::a_title_its_own_tick",
    "tui::event_loop::tests::a_resumes_report",
    "tui::event_loop::tests::a_report_torn_in_the_lead",
    "tui::check::tests::a_lead_admits",
    "tui::signals::tests::a_stop_before_the_session_began",
    "tui::shell::tests::an_automatic_compaction_that_compacted_nothing",
    "tui::event_loop::tests::a_working_a_keystroke",
    "tui::event_loop::tests::a_stop_a_keystroke",
    "tui::event_loop::tests::a_title_its_tick_asked",
    "tui::shell::tests::an_automatic_compaction_failure_whose_place",
    "tui::event_loop::tests::a_prompt_typed_after_a_stop",
    "tui::event_loop::tests::a_held_prompt_reports_nothing",
    "tui::event_loop::tests::keys_typed_ahead",
]
TEST = ["cargo", "test", "--locked", "--lib", "--"] + UNIT
BUILD = ["cargo", "build", "--locked"]
# The real binary on a real pseudoterminal, for what only the session's own
# loop does (`event_loop::session`'s `launch_title`).
PTY = ["cargo", "test", "--locked", "--test", "tui", "--",
       "the_session_reports_its_status", "sigtstp_really_stops"]

MUTANTS = [
    ("abnormal restore without its clear", "src/tui/term.rs",
     'pub(crate) const ABNORMAL_RESTORE: &str =\n    "\\x1b[?2026l\\x1b[?1049l\\x1b[23;2t\\x1b[>4;0m\\x1b[<u\\x1b[?2004l\\x1b]7501;state=clear\\x1b\\\\',
     'pub(crate) const ABNORMAL_RESTORE: &str =\n    "\\x1b[?2026l\\x1b[?1049l\\x1b[23;2t\\x1b[>4;0m\\x1b[<u\\x1b[?2004l', BUILD),
    ("tmux restore with its clear moved to the end", "src/tui/term.rs",
     '\\x1b[?2004l\\x1b]7501;state=clear\\x1b\\\\\\x1b[?2031l\\x1b[?7h\\x1b[?25h";\n\n/// The restore sequence for an exit',
     '\\x1b[?2004l\\x1b[?2031l\\x1b[?7h\\x1b[?25h\\x1b]7501;state=clear\\x1b\\\\";\n\n/// The restore sequence for an exit', BUILD),
    ("msg written raw, not base64", "src/tui/program_status.rs",
     "bytes.push_str(&crate::image::base64(&message));",
     "bytes.push_str(&String::from_utf8_lossy(&message));", TEST),
    ("msg not folded to one line", "src/tui/program_status.rs",
     "let message = one_line(message.as_bytes());",
     "let message = message.as_bytes().to_vec();", TEST),
    ("msg not bounded", "src/tui/program_status.rs",
     "if out.len() + separator + len > MAX_MESSAGE_BYTES {",
     "if out.len() + separator + len > usize::MAX / 2 {", TEST),
    ("an unchanged report written again", "src/tui/program_status.rs",
     "if !new_result && self.last.as_deref() == Some(report.bytes.as_str()) {",
     "if false && !new_result && self.last.as_deref() == Some(report.bytes.as_str()) {", TEST),
    ("a second result in a row not reported", "src/tui/program_status.rs",
     "new_result = !self.settled_reported && self.settled != Settled::Idle;",
     "new_result = false && !self.settled_reported && self.settled != Settled::Idle;", TEST),
    ("a reported result repeated after work that ran no turn", "src/tui/program_status.rs",
     "            Activity::Working | Activity::Blocked { .. } => {\n                if self.settled_reported {",
     "            Activity::Working | Activity::Blocked { .. } => {\n                if false && self.settled_reported {", TEST),
    ("a resume does not report again", "src/tui/event_loop.rs",
     "        shell.owe_program_status_resume();\n    }\n",
     "    }\n", TEST),
    ("any continue rewrites the record (round 1's rule)", "src/tui/event_loop.rs",
     "fn adopt_resume(shell: &mut Shell, band: &mut Band, plane_restored: bool) {\n    if plane_restored {",
     "fn adopt_resume(shell: &mut Shell, band: &mut Band, plane_restored: bool) {\n    shell.owe_program_status_resume();\n    if plane_restored {", TEST),
    ("a refused report offered again (round 1's rule)", "src/tui/event_loop.rs",
     "            } else {\n                write_program_status(&report, out, failures, now)?;\n            }",
     "            } else if let Err(emit) = out.emit(report.bytes.as_bytes()) {\n                shell.program_status_resumed();\n                if let Some(fatal) = disposed(emit, failures, now) {\n                    return Err(fatal);\n                }\n            }", TEST),
    ("an automatic compaction's failure reported idle (round 1's rule)", "src/tui/shell.rs",
     "                        self.program_status\n                            .turn_finished(program_status::Outcome::Failed);\n",
     "", TEST),
    ("the turn's end reports the compaction's failure again", "src/tui/shell.rs",
     "                let outcome = (self.tally.started.is_some() && !noted).then(|| {",
     "                let outcome = self.tally.started.is_some().then(|| {", TEST),
    ("a turn whose compaction failed still working until it ends (rounds 2-3's rule)", "src/tui/shell.rs",
     "        } else if !self.cancelling\n            && !self.compaction_failed\n",
     "        } else if (!self.cancelling || self.compaction_failed)\n", TEST),
    ("a failed compaction's turn working again once its place is back (round 4's race)", "src/tui/shell.rs",
     "            && !self.compaction_failed\n",
     "", TEST),
    ("the band's settle clears the failed compaction's mark (round 4's race)", "src/tui/shell.rs",
     "        if self.cancelling && self.work.outstanding() == 0 {\n            self.cancelling = false;\n",
     "        if self.cancelling && self.work.outstanding() == 0 {\n            self.cancelling = false;\n            self.compaction_failed = false;\n", TEST),
    ("the report written after the frame", "src/tui/event_loop.rs",
     "            } else {\n                write_program_status(&report, out, failures, now)?;\n            }",
     "            } else {\n                lead = Some(report);\n            }", TEST),
    ("the launch's report ahead of its title (rounds 1-2's order)", "src/tui/event_loop.rs",
     "            let rides = band.title_owed().is_some()",
     "            let rides = false && band.title_owed().is_some()", TEST),
    ("the launch's title not handed to the band before the first tick", "src/tui/event_loop.rs",
     "    launch_title(shell, band);\n",
     "", PTY),
    ("a resume's report ahead of its frame (rounds 1-2's order)", "src/tui/event_loop.rs",
     "    let resumed = shell.take_program_status_resume();",
     "    let resumed = shell.take_program_status_resume() && false;", TEST),
    ("the same, on a real terminal", "src/tui/event_loop.rs",
     "    let resumed = shell.take_program_status_resume();",
     "    let resumed = shell.take_program_status_resume() && false;", PTY),
    ("a keystroke's working ahead of the frame that shows it (rounds 1-3's order)", "src/tui/event_loop.rs",
     "    let keyed = shell.take_status_moved_by_input();",
     "    let keyed = {\n        let _ = shell.take_status_moved_by_input();\n        super::shell::Keyed::No\n    };", TEST),
    ("the same, on a real terminal", "src/tui/event_loop.rs",
     "    let keyed = shell.take_status_moved_by_input();",
     "    let keyed = {\n        let _ = shell.take_status_moved_by_input();\n        super::shell::Keyed::No\n    };", PTY),
    ("a prompt's working before any frame shows its turn", "src/tui/shell.rs",
     "            return Keyed::NotYet;",
     "            self.status_moved_by_input = false;\n            return Keyed::AfterFrame;", TEST),
    ("a prompt held only when typed: one typed after a stop reported ahead of its Thinking (round 4's)", "src/tui/shell.rs",
     "        if working && !self.activity.working() && !compacting {",
     "        if self.status_moved_by_input && working && !self.activity.working() && !compacting {", TEST),
    ("a held prompt's start reported ahead of the frame that shows it", "src/tui/shell.rs",
     "        let started = std::mem::take(&mut self.status_held) && working;",
     "        let started = {\n            self.status_held = false;\n            false\n        };", TEST),
    ("a held prompt's hold left over for a later tick", "src/tui/shell.rs",
     "        let started = std::mem::take(&mut self.status_held) && working;",
     "        let started = self.status_held && working;", TEST),
    ("a held prompt reported while it waits (b65eeedc's)", "src/tui/shell.rs",
     "            self.status_held = true;\n            return Keyed::NotYet;",
     "            self.status_held = true;\n            return Keyed::AfterFrame;", TEST),
    ("the launch's report taken after the keys typed ahead of it (round 4's)", "src/tui/event_loop.rs",
     "    shell.hold_launch_status();\n",
     "", TEST),
    ("the launch's report held back by a held first tick", "src/tui/event_loop.rs",
     "    if launch || (!after_frame && !held) {",
     "    if !after_frame && !held {", TEST),
    ("a stop's idle held behind the frame that shows it", "src/tui/shell.rs",
     "        if after != before && after != StatusActivity::Settled {",
     "        if after != before {", TEST),
    ("a report riding a title the tick's own events replaced", "src/tui/event_loop.rs",
     "                && !shell.owes_title()\n",
     "", TEST),
    ("a stop before the session began answers a later continue", "src/tui/signals.rs",
     "    let _ = take_plane_restored();\n    Ok(Held {",
     "    Ok(Held {", TEST),
    ("an automatic compaction that compacted nothing fails its turn", "src/tui/shell.rs",
     "                    compaction::Feedback::Succeeded\n                        | compaction::Feedback::NoOp\n                        | compaction::Feedback::Busy",
     "                    compaction::Feedback::Succeeded", TEST),
    ("the lead check admits a report ahead of its title", "src/tui/check.rs",
     "        if title_end == 0 || model.shown_title.as_deref() != update.title {",
     "        if false {", TEST),
    ("the lead check admits a report it was not told of", "src/tui/check.rs",
     "        if model.statuses != [StatusRecord::Reported(state)] || model.titles != titles {",
     "        if model.titles != titles {", TEST),
    ("the lead check admits a title written again in the report's slice", "src/tui/check.rs",
     "        if model.statuses != [StatusRecord::Reported(state)] || model.titles != titles {",
     "        if model.statuses != [StatusRecord::Reported(state)] {", TEST),
    ("a stopped turn still reported working", "src/tui/shell.rs",
     "        } else if !self.cancelling\n",
     "        } else if true\n", TEST),
    ("a waiting prompt not reported working", "src/tui/shell.rs",
     "                || !self.prompts.is_empty()\n",
     "", TEST),
    ("a manual /compact not reported working", "src/tui/shell.rs",
     "                    .is_some_and(|compaction| compaction.active() && !compaction.stopped()))",
     "                    .is_some_and(|compaction| false && compaction.active() && !compaction.stopped()))", TEST),
    ("any runtime work reported working (743c5381's rule)", "src/tui/shell.rs",
     "                || !self.prompts.is_empty()\n",
     "                || !self.prompts.is_empty()\n                || self.work.outstanding() > 0\n", TEST),
    ("work with no turn given a result", "src/tui/shell.rs",
     "                let outcome = (self.tally.started.is_some() && !noted).then(|| {",
     "                let outcome = (!noted).then(|| {", TEST),
    ("a stopped turn reported as its failure", "src/tui/shell.rs",
     "                    if self.cancelling {\n                        program_status::Outcome::Interrupted",
     "                    if false {\n                        program_status::Outcome::Interrupted", TEST),
    ("check admits a message carrying a control", "src/tui/check.rs",
     "    if message.chars().any(char::is_control) {",
     "    if false && message.chars().any(char::is_control) {", TEST),
    ("check admits a BEL-terminated report", "src/tui/check.rs",
     "    } else if let Some(report) = body.strip_prefix(\"7501;\") {\n        if terminator != 2 {",
     "    } else if let Some(report) = body.strip_prefix(\"7501;\") {\n        if terminator == 0 {", TEST),
    ("check admits a non-canonical base64 tail", "src/tui/check.rs",
     "        if leftover != 0 {",
     "        if false && leftover != 0 {", TEST),
    ("check admits a message past 256 decoded bytes", "src/tui/check.rs",
     "    if message.len() > super::program_status::MAX_MESSAGE_BYTES {",
     "    if false && message.len() > super::program_status::MAX_MESSAGE_BYTES {", TEST),
    ("check does not require the restore's clear", "src/tui/check.rs",
     "            statuses: if *clears_status {\n                vec![StatusRecord::Cleared]",
     "            statuses: if false && *clears_status {\n                vec![StatusRecord::Cleared]", TEST),
]

lines = []
for title, rel, old, new, command in MUTANTS:
    path = os.path.join(clone, rel)
    text = open(path, encoding="utf-8").read()
    count = text.count(old)
    if count != 1:
        lines.append("SKIP  %-55s %s: the original occurs %d times" % (title, rel, count))
        continue
    open(path, "w", encoding="utf-8").write(text.replace(old, new))
    run = subprocess.run(command, cwd=clone, env=env, capture_output=True, text=True, timeout=1800)
    subprocess.run(["git", "checkout", "--", "."], cwd=clone, env=env, check=True)
    tail = (run.stdout + run.stderr).strip().splitlines()
    failed = [l for l in tail if l.startswith("test ") and l.endswith("FAILED")]
    err = [l for l in tail if l.startswith("error") and "could not compile" not in l][:1]
    verdict = "KILLED" if run.returncode != 0 else "SURVIVED"
    detail = "; ".join(f[5:-10] for f in failed[:3]) or (err[0] if err else "exit %d" % run.returncode)
    lines.append("%-8s %-55s %s" % (verdict, title, detail))
    print(lines[-1], flush=True)
open(out, "w").write("\n".join(lines) + "\n")
