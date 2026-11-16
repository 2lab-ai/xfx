"""usage: TMPDIR=<short dir> python3 -I mutants.py "$TMPDIR/mut/repo" <out.txt>
Applies each mutant to the clone alone, runs the TUI's unit tests there
(`cargo test --lib -- tui::`) under a per-mutant timeout, and restores the
file. A mutant is killed when a test fails. Never run against the worktree:
the clone must sit under `$TMPDIR/mut/`, and its build and temp files go
beside it there. MODIFIED-PAGE-KEYS' runner (TILDE-EMPTY-MODIFIER's before
it), with this unit's mutants: the base's reading put back at the full
transcript and at the question (M1, M2); each of `question::Act::of`'s new
arms wrong one at a time (M3-M6, M8-M10) and the field step taken as Left at
a choice (M14); the decoder's Shift before Alt again (M7); the screen binding
a composer move (M11, M12); and the merged fold losing an arm `unspelled`
had (M13)."""
import os, subprocess, sys, time
repo, out = sys.argv[1], sys.argv[2]
mut = os.path.join(os.environ["TMPDIR"].rstrip("/"), "mut")
assert os.path.realpath(repo).startswith(os.path.realpath(mut) + os.sep), \
    "mutants run only in the throwaway clone under $TMPDIR/mut/"
I = "src/tui/input.rs"
S = "src/tui/shell.rs"
F = "src/tui/full_transcript.rs"
Q = "src/tui/question.rs"
# The base's `unspelled`, as a match over the decoded action.
BASE = ("match action { Action::ComposerLeft => Action::Left, Action::ComposerRight => Action::Right, "
        "Action::AltF => Action::WordRight, "
        "Action::ComposerUp | Action::DraftStart { shifted: false } => Action::Up, "
        "Action::ComposerDown | Action::DraftEnd { shifted: false } => Action::Down, "
        "Action::DraftStart { shifted: true } => Action::SelectUp, "
        "Action::DraftEnd { shifted: true } => Action::SelectDown, other => other }")
SCREEN_KEY = "        match self.full.key(*action, cols, rows, self.palette) {\n"
QUESTION_OF = "            Input::Action(action) => match question::Act::of(action) {\n"
STEP = "            Action::ComposerLeft => Some(Self::StepLeft),\n"
RIGHT = "            Action::ComposerRight => Some(Self::Right),\n"
ALT_F = "            Action::WordRight | Action::AltF => Some(Self::WordRight),\n"
EDGE = "            Action::DraftStart { .. } => Some(Self::Home),\n"
UP = "            Action::Up => Some(Self::Up),\n"
ORDER = ("        b'A' if alt => Action::ComposerUp,\n"
         "        b'B' if alt => Action::ComposerDown,\n"
         "        b'A' if shifted => Action::SelectUp,\n"
         "        b'B' if shifted => Action::SelectDown,\n")
ARROW = "            Action::Up => moved(true, Some(ARROW_ROWS)),\n"
HELD = "            Action::Left | Action::Right | Action::Redraw => Reply::Held,\n"
FOLD_LEFT = "        Input::Action(Action::ComposerLeft) => Input::Action(Action::Left),\n"
RETREAT = "            Act::Left | Act::AltB if !drafting => {\n"
MUTANTS = [
    ("M1 the full transcript reads the arrows' composer spellings as the arrows (the base's reading)", S,
     SCREEN_KEY, SCREEN_KEY.replace("key(*action,", "key({ let action = *action; %s }," % BASE)),
    ("M2 the question reads the arrows' composer spellings as the arrows (the base's reading)", S,
     QUESTION_OF, QUESTION_OF.replace("Act::of(action)", "Act::of(%s)" % BASE)),
    ("M3 a question takes Shift-Left, Super-Left and Ctrl-B as Left", Q, STEP,
     STEP.replace("Self::StepLeft", "Self::Left")),
    ("M4 a question binds no Alt-f", Q, ALT_F, "            Action::WordRight => Some(Self::WordRight),\n"),
    ("M5 a question takes Super-Up as Up", Q, EDGE, EDGE.replace("Self::Home", "Self::Up")),
    ("M6 a question takes Ctrl- and Alt-Up as Up", Q, UP, "            Action::ComposerUp => Some(Self::Up),\n" + UP),
    ("M7 the decoder reads Shift before Alt on Up and Down (the base's order)", I, ORDER,
     ORDER.split("\n", 2)[2] + "\n".join(ORDER.split("\n", 2)[:2]) + "\n"),
    ("M8 a question binds no Super-Up", Q, EDGE, ""),
    ("M9 a question binds no Shift-Left, Super-Left or Ctrl-B", Q, STEP, ""),
    ("M10 a question binds no Shift-Right, Super-Right or Ctrl-F", Q, RIGHT, ""),
    ("M11 the full transcript scrolls on Ctrl- and Alt-Up", F, ARROW,
     "            Action::Up | Action::ComposerUp => moved(true, Some(ARROW_ROWS)),\n"),
    ("M12 the full transcript holds Shift-Left, Super-Left and Ctrl-B", F, HELD,
     "            Action::Left | Action::ComposerLeft | Action::Right | Action::Redraw => Reply::Held,\n"),
    ("M13 the merged fold leaves a composer move on Left as itself", S, FOLD_LEFT, ""),
    ("M14 Shift-Left, Super-Left and Ctrl-B go back a question at a choice", Q, RETREAT,
     "            Act::Left | Act::StepLeft | Act::AltB if !drafting => {\n"),
]
TESTS = [["cargo", "test", "--locked", "--lib", "--", "tui::"]]
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
                r = subprocess.run(cmd, cwd=repo, env=env, capture_output=True, text=True, timeout=1200)
            except subprocess.TimeoutExpired:
                verdict, why = "KILLED", "timeout"; break
            if r.returncode != 0:
                failed = [l.split()[1] for l in r.stdout.splitlines() if l.startswith("test ") and l.endswith("FAILED")]
                if not failed and "error[" in r.stderr:
                    verdict, why = "BUILD-ERROR", r.stderr.split("error[",1)[1][:120].replace("\n"," "); break
                verdict, why = "KILLED", ", ".join(
                    f.removeprefix("tui::input::tests::").removeprefix("tui::shell::tests::")
                    .removeprefix("tui::question::tests::")
                    for f in failed[:6])
                if len(failed) > 6:
                    why += f" (+{len(failed) - 6} more)"
                break
    finally:
        open(full, "w").write(src)
    say(f"{name}: {verdict} ({time.time()-t0:.0f}s) {why}")
status = subprocess.run(["git", "-C", repo, "status", "--porcelain"], capture_output=True, text=True).stdout
say(f"{sum(1 for l in lines if ': KILLED' in l)} of {len(MUTANTS)} killed; "
    f"the clone's tree after the run: {'clean' if not status.strip() else 'DIRTY'}")
open(out, "w").write("\n".join(lines) + "\n")
