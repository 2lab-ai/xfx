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
]
TEST = ["cargo", "test", "--locked", "--lib", "--"] + UNIT
BUILD = ["cargo", "build", "--locked"]

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
     "    shell.program_status_resumed();\n",
     "", TEST),
    ("the report written after the frame", "src/tui/event_loop.rs",
     "    let committed = commit_program_status(shell, &mut counted, failures, now)\n        .and_then(|()| commit_frame(shell, band, &mut counted, failures, now, reconciled))",
     "    let committed = commit_frame(shell, band, &mut counted, failures, now, reconciled)\n        .and_then(|()| commit_program_status(shell, &mut counted, failures, now))", TEST),
    ("a stopped turn still reported working", "src/tui/shell.rs",
     "} else if !self.cancelling && (self.activity.working() || self.work.outstanding() > 0) {",
     "} else if self.activity.working() || self.work.outstanding() > 0 {", TEST),
    ("a submission in hand not reported working", "src/tui/shell.rs",
     "} else if !self.cancelling && (self.activity.working() || self.work.outstanding() > 0) {",
     "} else if !self.cancelling && self.activity.working() {", TEST),
    ("a failure with no turn not reported", "src/tui/shell.rs",
     "                } else if failure.is_some() {\n                    Some(program_status::Outcome::Failed)",
     "                } else if failure.is_some() && self.tally.started.is_some() {\n                    Some(program_status::Outcome::Failed)", TEST),
    ("check admits a message carrying a control", "src/tui/check.rs",
     "    if message.chars().any(char::is_control) {",
     "    if false && message.chars().any(char::is_control) {", TEST),
    ("check admits a BEL-terminated report", "src/tui/check.rs",
     "    } else if let Some(report) = body.strip_prefix(\"7501;\") {\n        if terminator != 2 {",
     "    } else if let Some(report) = body.strip_prefix(\"7501;\") {\n        if terminator == 0 {", TEST),
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
