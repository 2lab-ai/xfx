"""usage: TMPDIR=<short dir> python3 -I mutants.py "$TMPDIR/mut/repo" <out.txt>
Applies each mutant to the clone alone, runs the unit's tests there under a
per-mutant timeout, and restores the file. A mutant is killed when a test
fails. Never run against the worktree: the clone must sit under
`$TMPDIR/mut/`, and its build and temp files go beside it there.
APPROVAL-READINESS-SETTLED's mutants.py, with this unit's mutants (M12-M14 and
the filter's last two names added in round 2)."""
import os, subprocess, sys, time
repo, out = sys.argv[1], sys.argv[2]
mut = os.path.join(os.environ["TMPDIR"].rstrip("/"), "mut")
assert os.path.realpath(repo).startswith(os.path.realpath(mut) + os.sep), \
    "mutants run only in the throwaway clone under $TMPDIR/mut/"
A = "src/tui/approval.rs"
S = "src/tui/shell.rs"
KILL_TO_START = "            | Key::KillToStart\n"
def put_back(key):
    return KILL_TO_START + "            | Key::%s\n" % key
EDIT = "            drafts.edit(editing, width(open));\n            Reply::Moved\n"
SWALLOW = ("                Input::Action(key) if approval::edits(key) => approval::Action::Edit(key),\n"
           "                _ => return,\n")
EDGE = "                approval::Action::Edit(edge)\n"
DRAFT_ARM = "                Input::Action(key) if approval::edits(key) => approval::Action::Edit(key),\n"
def as_key(old, new):
    return ("                Input::Action(key) if approval::edits(key) => approval::Action::Edit(\n"
            "                    if key == Action::%s { Action::%s } else { key },\n"
            "                ),\n" % (old, new))
MUTANTS = [
    # Each removed key put back in the draft's keys.
    ("M1 Undo put back in approval::edits", A, KILL_TO_START, put_back("Undo")),
    ("M2 Redo put back in approval::edits", A, KILL_TO_START, put_back("Redo")),
    ("M3 Yank put back in approval::edits", A, KILL_TO_START, put_back("Yank")),
    # The Tab rule.
    ("M4 Tab in a draft is Ignored again (holds no Enter)", A,
     "        Action::Tab if drafts.editing() => Reply::Moved,\n",
     "        Action::Tab if drafts.editing() => Reply::Ignored,\n"),
    # A kept key removed, one per kind.
    ("M5 C-u and Super-Backspace removed (KillToStart)", A, KILL_TO_START, ""),
    ("M6 Delete removed", A, "            | Key::Delete\n", ""),
    ("M7 Home, C-a and their spellings removed", A, "            | Key::Home\n", ""),
    ("M8 Alt-Backspace removed (DeleteWordBackward)", A, "            | Key::DeleteWordBackward\n", ""),
    ("M9 Super-Up and -Down in a draft swallowed", S, EDGE, "                return\n"),
    # The hold rule, both ways.
    ("M10 a draft edit holds no Enter (Ignored)", A, EDIT,
     "            drafts.edit(editing, width(open));\n            Reply::Ignored\n"),
    ("M11 a key the draft does not take holds the Enter (marks a change)", S, SWALLOW,
     "                Input::Action(key) if approval::edits(key) => approval::Action::Edit(key),\n"
     "                _ => {\n                    self.approval_changed();\n                    return;\n                }\n"),
    # Round 2: the word deletes told apart, and a pending hold kept past a swallowed key.
    ("M12 Alt-Backspace deletes C-w's whitespace word", S, DRAFT_ARM, as_key("DeleteWordBackward", "DeleteWordLeft")),
    ("M13 Alt-d kills to the line's end, as C-k", S, DRAFT_ARM, as_key("DeleteWordRight", "KillToEnd")),
    ("M14 a swallowed key settles a pending change (a frame taken as shown)", S, SWALLOW,
     "                Input::Action(key) if approval::edits(key) => approval::Action::Edit(key),\n"
     "                _ => {\n"
     "                    if self.asked_id.is_some_and(|id| self.approval_unshown(id)) {\n"
     "                        self.intend_approval();\n"
     "                        self.approval_landed(Outcome::Unchanged);\n"
     "                    }\n"
     "                    return;\n"
     "                }\n"),
]
TESTS = [
    ["cargo", "test", "--locked", "--lib", "--", "approval", "amendment", "alt_b", "alt_f", "the_fold",
     "horizontal", "draft_left", "up_and_down_move", "the_arrows_leave", "leaving_a_filled_draft",
     "revokes", "vertical", "super_up_and_down", "one_read", "until_a_frame", "refusal_read",
     "a_walk_of_the_review", "enter_takes_the_marked", "writes_nothing", "ctrl_l",
     "undo_redo_and_yank", "tab_in_a_draft", "every_key_fxs", "the_word_deletes", "an_edit_then_a_key"],
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
