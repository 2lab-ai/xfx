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
Q = "src/tools/question.rs"
P = "src/tui/question.rs"
MUTANTS = [
    ("M1 cap back to 4", Q, "pub const MAX_QUESTIONS: usize = 20;", "pub const MAX_QUESTIONS: usize = 4;"),
    ("M2 cap 21", Q, "pub const MAX_QUESTIONS: usize = 20;", "pub const MAX_QUESTIONS: usize = 21;"),
    ("M3 cap 19", Q, "pub const MAX_QUESTIONS: usize = 20;", "pub const MAX_QUESTIONS: usize = 19;"),
    ("M4 parse bound off by one (>=)", Q, "questions.len() > MAX_QUESTIONS {", "questions.len() >= MAX_QUESTIONS {"),
    ("M5 refusal words left at 4", Q, '"provide {MIN_QUESTIONS} to {MAX_QUESTIONS} questions"', '"provide 1 to 4 questions"'),
    ("M6 description left at 1 to 4", Q, '"Ask the user 1-20 multiple-choice questions in interactive runs', '"Ask the user 1 to 4 multiple-choice questions in interactive runs'),
    ("M7 schema maxItems left at 4", Q, "            min_items: MIN_QUESTIONS,\n            max_items: MAX_QUESTIONS,", "            min_items: MIN_QUESTIONS,\n            max_items: 4,"),
    ("M8 questions property text left at 4", Q, '"1 to 20 questions to ask in order', '"1 to 4 questions to ask in order'),
    ("M9 worst-case bound left at 36 977", Q, "pub const MAX_ENCODED_RESULT_BYTES: usize = 184_881;", "pub const MAX_ENCODED_RESULT_BYTES: usize = 36_977;"),
    ("M10 counter air 2 -> 1", P, "if used + counted + 2 <= budget {", "if used + counted + 1 <= budget {"),
    ("M11 Tab stops at the last question", P, "self.current = (self.current + 1) % self.entries.len();", "self.current = (self.current + 1).min(self.entries.len() - 1);"),
    ("M12 Left wraps past the first", P, "                if self.current == 0 {\n                    return Answered::Nothing;\n                }\n                self.current -= 1;", "                if self.current == 0 {\n                    self.current = self.entries.len() - 1;\n                    return Answered::Redraw;\n                }\n                self.current -= 1;"),
    ("M13 resolution indent fixed at 5", P, 'let indent = " ".repeat(usize::from(width(&prefix)));', 'let indent = " ".repeat(5);'),
]
TESTS = [
    ["cargo", "test", "--locked", "--lib", "question"],
    ["cargo", "test", "--locked", "--test", "tool_loop"],
    ["cargo", "test", "--locked", "--test", "tui", "--", "twenty_questions_are_one_panel", "six_questions_open_the_panel"],
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
                verdict, why = "KILLED", f"timeout in {' '.join(cmd[3:5])}"; break
            if r.returncode != 0:
                failed = [l.split()[1] for l in r.stdout.splitlines() if l.startswith("test ") and l.endswith("FAILED")]
                if not failed and "error[" in r.stderr:
                    verdict, why = "BUILD-ERROR", r.stderr.split("error[",1)[1][:120].replace("\n"," "); break
                verdict, why = "KILLED", f"{' '.join(cmd[3:5])}: " + ", ".join(failed[:4]); break
    finally:
        open(full, "w").write(src)
    say(f"{name}: {verdict} ({time.time()-t0:.0f}s) {why}")
subprocess.run(["git", "-C", repo, "diff", "--stat"], check=False)
open(out, "w").write("\n".join(lines) + "\n")
