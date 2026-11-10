"""usage: TMPDIR=<short dir> python3 -I mutants.py "$TMPDIR/mut/repo" <out.txt>
Applies each mutant to the clone alone, runs the unit's tests there under a
per-mutant timeout, and restores the file. A mutant is killed when a test
fails. Never run against the worktree: the clone must sit under
`$TMPDIR/mut/`, and its build and temp files go beside it there.
APPROVAL-VERTICAL-SPELLINGS' mutants.py, with this unit's mutants."""
import os, subprocess, sys, time
repo, out = sys.argv[1], sys.argv[2]
mut = os.path.join(os.environ["TMPDIR"].rstrip("/"), "mut")
assert os.path.realpath(repo).startswith(os.path.realpath(mut) + os.sep), \
    "mutants run only in the throwaway clone under $TMPDIR/mut/"
R = "src/tui/approval_readiness.rs"
S = "src/tui/shell.rs"
CHANGED = "    fn approval_changed(&mut self) {\n        self.readiness.changed();\n    }\n"
GATE = ("        if matches!(answer, ApprovalAnswer::Once | ApprovalAnswer::Always)\n"
        "            && !self.asked_id.is_some_and(|id| self.approval_ready(id))\n")
SILENT = ("            if self.asked_id.is_some_and(|id| self.approval_unshown(id)) {\n"
          "                self.render.request(Reason::Modal);\n                return;\n            }\n")
SWALLOW = "                | Action::HistoryNext,\n            ) => return,\n"
WALK = "                if screen.scroll_by(delta, cols, rows) {\n                    self.approval_changed();\n                }\n"
MUTANTS = [
    # The brief's five.
    ("M1 the condition removed (a receipt answers whatever changed since)", R,
     "            .is_some_and(|receipt| receipt.revision == self.revision)\n",
     "            .is_some()\n"),
    ("M2 the condition restricted to file approvals (fx's subject check)", S, CHANGED,
     "    fn approval_changed(&mut self) {\n"
     "        let file = (self.panel.as_ref().map(Panel::request))\n"
     "            .or(self.alternate.as_ref().map(ApprovalScreen::request))\n"
     "            .is_some_and(|r| matches!(r.subject, crate::permission::ApprovalSubject::Path(_)));\n"
     "        if file {\n            self.readiness.changed();\n        }\n    }\n"),
    ("M3 Deny gated too (a refusal read with a move waits for the frame)", S, GATE,
     "        if (matches!(answer, ApprovalAnswer::Once | ApprovalAnswer::Always)\n"
     "            || self.asked_id.is_some_and(|id| self.approval_unshown(id)))\n"
     "            && !self.asked_id.is_some_and(|id| self.approval_ready(id))\n"),
    ("M4 pending never cleared (no frame after a change ever answers)", R,
     "                self.pending = Some(Intent {\n                    revision: self.revision,\n",
     "                self.pending = Some(Intent {\n                    revision: 0,\n"),
    ("M5 a no-op key marks the question changed", S, SWALLOW,
     "                | Action::HistoryNext,\n            ) => {\n                self.approval_changed();\n"
     "                return;\n            }\n"),
    # And what the condition rests on.
    ("M6 a repaint that wrote nothing keeps the receipt's old revision", R,
     "            {\n                self.receipt = pending;\n            }\n",
     "            {}\n"),
    ("M7 the refusal of an unshown change is told as 'not on the screen yet'", S, SILENT, ""),
    ("M8 a review-plane walk that moved holds nothing back", S, WALK,
     "                let _ = screen.scroll_by(delta, cols, rows);\n"),
    ("M9 a review-plane walk that could not move holds the Enter back", S, WALK,
     "                let _ = screen.scroll_by(delta, cols, rows);\n                self.approval_changed();\n"),
    ("M10 a frame composed before the change settles it (stamped at the write)", R,
     "            Outcome::Painted => self.provisional = pending,\n",
     "            Outcome::Painted => {\n                self.provisional = pending.map(|intent| Intent {\n"
     "                    revision: self.revision,\n                    ..intent\n                })\n            }\n"),
]
TESTS = [
    ["cargo", "test", "--locked", "--lib", "--", "approval", "amendment", "alt_b", "alt_f", "the_fold",
     "horizontal", "draft_left", "up_and_down_move", "the_arrows_leave", "leaving_a_filled_draft",
     "revokes", "vertical", "super_up_and_down", "one_read", "until_a_frame", "refusal_read",
     "a_walk_of_the_review", "enter_takes_the_marked", "question::", "input::tests", "editor::tests"],
]
env = dict(os.environ, CARGO_TARGET_DIR=os.path.join(mut, "target"), TMPDIR=os.path.join(mut, "tmp"))
os.makedirs(env["TMPDIR"], exist_ok=True)
lines = []
def say(text):
    print(text, flush=True); lines.append(text)
for name, path, old, new in MUTANTS:
    full = os.path.join(repo, path)
    src = open(full).read()
    if src.count(old) != 1:
        say(f"{name}: NOT APPLIED (pattern found {src.count(old)} times)"); continue
    open(full, "w").write(src.replace(old, new))
    verdict, why = "SURVIVED", ""
    t0 = time.time()
    try:
        for cmd in TESTS:
            try:
                r = subprocess.run(cmd, cwd=repo, env=env, capture_output=True, text=True, timeout=900)
            except subprocess.TimeoutExpired:
                verdict, why = "KILLED", "timeout"; break
            if r.returncode != 0:
                failed = [l.split()[1] for l in r.stdout.splitlines() if l.startswith("test ") and l.endswith("FAILED")]
                if not failed and "error[" in r.stderr:
                    verdict, why = "BUILD-ERROR", r.stderr.split("error[",1)[1][:120].replace("\n"," "); break
                verdict, why = "KILLED", f"{len(failed)} failed: " + ", ".join(failed)
                break
    finally:
        open(full, "w").write(src)
    say(f"{name}: {verdict} ({time.time()-t0:.0f}s) {why}")
status = subprocess.run(["git", "-C", repo, "status", "--porcelain"], capture_output=True, text=True).stdout
say(f"{sum(1 for l in lines if ': KILLED' in l)} of {len(MUTANTS)} killed; "
    f"the clone's tree after the run: {'clean' if not status.strip() else 'DIRTY'}")
open(out, "w").write("\n".join(lines) + "\n")
