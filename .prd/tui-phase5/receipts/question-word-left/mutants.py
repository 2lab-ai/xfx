"""usage: TMPDIR=<short dir> python3 -I mutants.py "$TMPDIR/mut/repo" <out.txt>
Applies each mutant to the clone alone, runs the unit's tests there under a
per-mutant timeout, and restores the file. A mutant is killed when a test
fails. Never run against the worktree: the clone must sit under
`$TMPDIR/mut/`, and its build and temp files go beside it there."""
import os, subprocess, sys, time
repo, out = sys.argv[1], sys.argv[2]
mut = os.path.join(os.environ["TMPDIR"].rstrip("/"), "mut")
assert os.path.realpath(repo).startswith(os.path.realpath(mut) + os.sep), \
    "mutants run only in the throwaway clone under $TMPDIR/mut/"
Q = "src/tui/question.rs"
I = "src/tui/input.rs"
S = "src/tui/shell.rs"
AT_A_CHOICE = "            Act::Left | Act::AltB if !drafting => {"
MUTANTS = [
    ("M1 Alt-b at a choice does nothing again", Q, AT_A_CHOICE,
     "            Act::Left if !drafting => {"),
    ("M2 Alt-b moves the marked choice up one instead of the question back", Q, AT_A_CHOICE,
     "            Act::AltB if !drafting => self.choose(true, cols, terminal_rows),\n"
     "            Act::Left if !drafting => {"),
    ("M3 the word arrows (Ctrl-Left, Alt-Left) go back a question too", Q, AT_A_CHOICE,
     "            Act::Left | Act::AltB | Act::WordLeft if !drafting => {"),
    ("M4 Alt-b in a draft is not the word move", Q,
     "                    Act::WordLeft | Act::AltB => draft.word_left(),",
     "                    Act::WordLeft => draft.word_left(),"),
    ("M5 the decoder reads ESC b as WordLeft again", I,
     "                out.push(Input::Action(Action::AltB));",
     "                out.push(Input::Action(Action::WordLeft));"),
    ("M6 the decoder reads kitty/xterm Alt-b as WordLeft again", I,
     "            Action::AltB\n        } else {",
     "            Action::WordLeft\n        } else {"),
    ("M7 the shell's fold lets Alt-b through to the other surfaces", S,
     "        Input::Action(Action::AltB) => Input::Action(Action::WordLeft),\n", ""),
]
TESTS = [
    ["cargo", "test", "--locked", "--lib", "--", "alt_b", "question::", "input::tests"],
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
