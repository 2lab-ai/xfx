"""usage: TMPDIR=<short dir> python3 -I mutants.py "$TMPDIR/mut/repo" <out.txt>
Applies each mutant to the clone alone, runs the unit's tests there under a
per-mutant timeout, and restores the file. A mutant is killed when a test
fails. Never run against the worktree: the clone must sit under
`$TMPDIR/mut/`, and its build and temp files go beside it there.
QUESTION-WORD-LEFT's mutants.py, with this unit's mutants."""
import os, subprocess, sys, time
repo, out = sys.argv[1], sys.argv[2]
mut = os.path.join(os.environ["TMPDIR"].rstrip("/"), "mut")
assert os.path.realpath(repo).startswith(os.path.realpath(mut) + os.sep), \
    "mutants run only in the throwaway clone under $TMPDIR/mut/"
A = "src/tui/approval.rs"
D = "src/tui/approval_amendment.rs"
I = "src/tui/input.rs"
S = "src/tui/shell.rs"
LEFT = "            Input::Action(Action::Left | Action::AltB) if !drafting => approval::Action::Up,\n"
RIGHT = "            Input::Action(Action::Right | Action::AltF) if !drafting => approval::Action::Down,\n"
UP = "            *selected = (*selected + CHOICES.len() - 1) % CHOICES.len();\n            Reply::Moved"
DOWN = "            *selected = (*selected + 1) % CHOICES.len();\n            Reply::Moved"
MUTANTS = [
    ("M1 Left and Right at a choice back to nothing (the base's behaviour)", S, LEFT + RIGHT, ""),
    ("M2 the decoder reads ESC f as Ctrl-Right again (WordRight)", I,
     "                out.push(Input::Action(Action::AltF));",
     "                out.push(Input::Action(Action::WordRight));"),
    ("M3 the decoder reads kitty/xterm Alt-f as Ctrl-Right again", I,
     "            Action::AltF\n        });",
     "            Action::WordRight\n        });"),
    ("M4 Shift-Left and Shift-Right decode as the arrows again (and move the choice)", I,
     "    let composer = mask & (SHIFT | SUPER) != 0;",
     "    let composer = mask & SUPER != 0;"),
    ("M5 Super-Left and Super-Right decode as the arrows (and move the choice)", I,
     "    let composer = mask & (SHIFT | SUPER) != 0;",
     "    let composer = mask & SHIFT != 0;"),
    ("M6 Ctrl-B decodes as Left again (and moves the choice)", I,
     "        0x02 => Action::ComposerLeft,", "        0x02 => Action::Left,"),
    ("M7 Ctrl-Left (the word arrow) moves the choice too", S, LEFT,
     LEFT.replace("Action::Left | Action::AltB)", "Action::Left | Action::AltB | Action::WordLeft)")),
    ("M8 the choice stops at the ends where fx wraps", A, UP + "\n", UP.replace(
     "(*selected + CHOICES.len() - 1) % CHOICES.len()", "selected.saturating_sub(1)") + "\n"),
    ("M9 the choice stops at the last where fx wraps (Down and Right)", A, DOWN + "\n        }\n        // Tab",
     DOWN.replace("(*selected + 1) % CHOICES.len()", "(*selected + 1).min(CHOICES.len() - 1)") + "\n        }\n        // Tab"),
    ("M10 row 1 after a move: a draft left behind is painted again (the base's rule)", D,
     "            if self.active != Some(choice) {",
     "            if self.active != Some(choice) && self.sides[side].editor.is_empty() {"),
    ("M11 a draft left behind does not travel with its choice", D,
     "        .map(|side| self.sides[side].editor.text().to_string())",
     "        .filter(|_| self.active.is_some())\n        .map(|side| self.sides[side].editor.text().to_string())"),
    ("M12 an approval's draft reads the spellings unfolded", S,
     "            Input::Action(editing) if drafting => match folded(Input::Action(editing)) {",
     "            Input::Action(editing) if drafting => match Input::Action(editing) {"),
    ("M13 the question reads the approval's spellings unfolded", S,
     "                self.answer(event, now);", "                self.answer(spelled.clone(), now);"),
    ("M14 the full transcript reads the approval's spellings unfolded", S,
     "            if self.full.is_open() && self.view_full_transcript(&event) {",
     "            if self.full.is_open() && self.view_full_transcript(&spelled) {"),
    # Round 2 (gpt6's MUST): the readiness revocation when a filled draft is left.
    ("M15 a reshape does not revoke the readiness receipt", S,
     "                // the next genuinely disclosed frame re-earns it.\n"
     "                self.invalidate_approval();\n",
     "                // the next genuinely disclosed frame re-earns it.\n"),
    ("M16 leaving a draft (an arrow) is not a reshape", A,
     "        Reply::Moved if shape(drafts, width) != before => Reply::Reshaped,",
     "        Reply::Moved if shape(drafts, width) != before\n"
     "            && !matches!(action, Action::Up | Action::Down) => Reply::Reshaped,"),
]
TESTS = [
    ["cargo", "test", "--locked", "--lib", "--", "approval", "amendment", "alt_b", "alt_f", "the_fold",
     "the_full_transcript_holds", "horizontal", "draft_left", "up_and_down_move", "the_arrows_leave",
     "leaving_a_filled_draft", "revokes",
     "question::", "input::tests"],
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
