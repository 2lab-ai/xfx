"""usage: TMPDIR=<short dir> python3 -I mutants.py "$TMPDIR/mut/repo" <out.txt>
Applies each mutant to the clone alone, runs the unit's tests there under a
per-mutant timeout, and restores the file. A mutant is killed when a test
fails. Never run against the worktree: the clone must sit under
`$TMPDIR/mut/`, and its build and temp files go beside it there.
APPROVAL-HORIZONTAL-KEYS' mutants.py, with this unit's mutants."""
import os, subprocess, sys, time
repo, out = sys.argv[1], sys.argv[2]
mut = os.path.join(os.environ["TMPDIR"].rstrip("/"), "mut")
assert os.path.realpath(repo).startswith(os.path.realpath(mut) + os.sep), \
    "mutants run only in the throwaway clone under $TMPDIR/mut/"
E = "src/tui/editor.rs"
I = "src/tui/input.rs"
S = "src/tui/shell.rs"
UP = "            Input::Action(Action::Up) => approval::Action::Up,\n"
EDGE = "                approval::Action::Edit(edge)\n"
SWALLOW = "                | Action::HistoryNext,\n            ) => return,\n"
MUTANTS = [
    # The brief's six.
    ("M1 a modified arrow (Ctrl- or Alt-Up) moves the choice again", S, UP,
     UP.replace("Action::Up)", "Action::Up | Action::ComposerUp)")),
    ("M2 C-p moves the choice again", S, UP,
     UP.replace("Action::Up)", "Action::Up | Action::ControlP)")),
    ("M3 Super-Up and -Down in an amendment end it (and move the choice)", S, EDGE,
     "                approval::Action::Up\n"),
    ("M4 Shift-Up in an amendment moves the caret (to the line's start)", S, UP,
     UP + "            Input::Action(Action::SelectUp) if drafting => approval::Action::Edit(Action::Home),\n"),
    ("M5 a no-op spelling revokes the readiness receipt", S, SWALLOW,
     "                | Action::HistoryNext,\n            ) => {\n                self.invalidate_approval();\n"
     "                return;\n            }\n"),
    ("M6 a reshape no longer revokes the receipt", S,
     "                // the next genuinely disclosed frame re-earns it.\n"
     "                self.invalidate_approval();\n",
     "                // the next genuinely disclosed frame re-earns it.\n"),
    # And the decoder, the fold and the draft's edges they rest on.
    ("M7 Super-Up decodes as the arrow again (and moves the choice)", I,
     "        b'A' if draft => Action::DraftStart { shifted },",
     "        b'A' if draft => Action::Up,"),
    ("M8 Ctrl and Alt on Up and Down decode as the arrows again", I,
     "    let word = mask & (ALT | CTRL) != 0;\n    let shifted",
     "    let word = mask & (ALT | CTRL) != 0 && letter != b'A' && letter != b'B';\n    let shifted"),
    ("M9 Shift wins over Super (Super-Shift-Up is SelectUp, as before this unit)", I,
     "    let draft = mask & SUPER != 0;", "    let draft = mask & SUPER != 0 && mask & SHIFT == 0;"),
    ("M10 the fold gives the question Up for Super-Shift-Up (it saw SelectUp)", S,
     "        Input::Action(Action::DraftStart { shifted: true }) => Input::Action(Action::SelectUp),",
     "        Input::Action(Action::DraftStart { shifted: true }) => Input::Action(Action::Up),"),
    ("M11 the fold leaves ComposerUp for the question (it saw Up)", S,
     "        Input::Action(Action::ComposerUp | Action::DraftStart { shifted: false }) => {",
     "        Input::Action(Action::DraftStart { shifted: false }) => {"),
    ("M12 Super-Up in a draft goes to the line's start, not the draft's", E,
     "            Action::DraftStart { .. } => {\n                self.move_to(0);",
     "            Action::DraftStart { .. } => {\n                self.move_to(self.line_start());"),
    ("M13 the plain Up no longer ends a draft (it stays in it)", S, UP,
     "            Input::Action(Action::Up) if !drafting => approval::Action::Up,\n"),
    # Round 2 (MUST 3): a key the one-read Enter cases cover, revoking the
    # receipt where the caret is no part of it.
    ("M14 Super-Up and -Down in a draft revoke the readiness receipt", S, EDGE,
     "                self.invalidate_approval();\n" + EDGE),
]
TESTS = [
    ["cargo", "test", "--locked", "--lib", "--", "approval", "amendment", "alt_b", "alt_f", "the_fold",
     "the_full_transcript_holds", "horizontal", "draft_left", "up_and_down_move", "the_arrows_leave",
     "leaving_a_filled_draft", "revokes", "vertical", "super_up_and_down", "one_read",
     "question::", "input::tests", "editor::tests"],
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
                verdict, why = "KILLED", ", ".join(failed[:4]); break
    finally:
        open(full, "w").write(src)
    say(f"{name}: {verdict} ({time.time()-t0:.0f}s) {why}")
status = subprocess.run(["git", "-C", repo, "status", "--porcelain"], capture_output=True, text=True).stdout
say(f"{sum(1 for l in lines if ': KILLED' in l)} of {len(MUTANTS)} killed; "
    f"the clone's tree after the run: {'clean' if not status.strip() else 'DIRTY'}")
open(out, "w").write("\n".join(lines) + "\n")
