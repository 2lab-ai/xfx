"""usage: TMPDIR=<short dir> python3 -I mutants.py "$TMPDIR/mut/repo" <out.txt> [M<n>...]
Applies each mutant to the clone alone, runs the TUI's unit tests there
(`cargo test --lib -- tui::`) under a per-mutant timeout, and restores the
files. A mutant is killed when a test fails. Never run against the worktree:
the clone must sit under `$TMPDIR/mut/`, and its build and temp files go
beside it there. COMPOSER-SUPER-ARROWS' runner, with this unit's mutants:
the amendment's rule half undone one byte class at a time -- LF, Tab, the
CR normalization, a run of breaks merged into one space (M1-M4); the budget
judged on the raw bytes again, or on the decoded text before it is
normalized (M5, M6); and the rule put where fx does not put it, the
question's field and the composer (M7, M8). Mutant names after <out.txt>
run those alone."""
import os, subprocess, sys, time
repo, out = sys.argv[1], sys.argv[2]
mut = os.path.join(os.environ["TMPDIR"].rstrip("/"), "mut")
assert os.path.realpath(repo).startswith(os.path.realpath(mut) + os.sep), \
    "mutants run only in the throwaway clone under $TMPDIR/mut/"
A = "src/tui/approval_amendment.rs"
Q = "src/tui/question.rs"
P = "src/tui/paste.rs"
RULE = "    super::transcript::normalize(text).replace(['\\n', '\\t'], \" \")\n"
BOUND = "        if self.buffer.len() >= super::paste::MAX_PASTE_BYTES {\n"
FINISH = ("        let text = one_line(&String::from_utf8_lossy(&bytes));\n"
          "        fits(held, text.len()).then_some(text)\n")
QUESTION = ("    String::from_utf8_lossy(&kept)\n"
            "        .replace(\"\\r\\n\", \"\\n\")\n"
            "        .replace('\\r', \"\\n\")\n")
COMPOSER = "        let text = transcript::normalize(&String::from_utf8_lossy(&bytes));\n"
MUTANTS = [
    ("M1 an LF stays an LF (only the Tab is a space)", [(A, RULE,
     "    super::transcript::normalize(text).replace('\\t', \" \")\n")]),
    ("M2 a Tab stays a Tab (only the LF is a space)", [(A, RULE,
     "    super::transcript::normalize(text).replace('\\n', \" \")\n")]),
    ("M3 no CR normalization (CR LF and a lone CR kept raw)", [(A, RULE,
     "    text.replace(['\\n', '\\t'], \" \")\n")]),
    ("M4 a run of breaks and tabs is one space", [(A, RULE,
     "    let normalized = super::transcript::normalize(text).replace(['\\n', '\\t'], \" \");\n"
     "    let mut out = String::new();\n"
     "    for c in normalized.chars() {\n"
     "        if !(c == ' ' && out.ends_with(' ')) {\n"
     "            out.push(c);\n"
     "        }\n"
     "    }\n"
     "    out\n")]),
    ("M5 the paste is refused at its 4097th raw byte (the base's bound)", [(A, BOUND,
     "        if self.buffer.len() >= MAX_FEEDBACK_BYTES {\n")]),
    ("M6 the budget is judged on the decoded text before it is normalized", [(A, FINISH,
     "        let decoded = String::from_utf8_lossy(&bytes).into_owned();\n"
     "        let fit = fits(held, decoded.len());\n"
     "        let text = one_line(&decoded);\n"
     "        fit.then_some(text)\n")]),
    ("M7 the question's field takes the amendment's rule (its breaks spaces)", [(Q, QUESTION,
     QUESTION + "        .replace('\\n', \" \")\n")]),
    ("M8 the composer takes the amendment's rule (its breaks and tabs spaces)", [(P, COMPOSER,
     "        let text = transcript::normalize(&String::from_utf8_lossy(&bytes)).replace(['\\n', '\\t'], \" \");\n")]),
]
only = sys.argv[3:]
if only:
    MUTANTS = [m for m in MUTANTS if m[0].split()[0] in only]
TESTS = [["cargo", "test", "--locked", "--lib", "--", "tui::"]]
env = dict(os.environ, CARGO_TARGET_DIR=os.path.join(mut, "target"), TMPDIR=os.path.join(mut, "tmp"))
os.makedirs(env["TMPDIR"], exist_ok=True)
lines = []
def say(text):
    print(text, flush=True); lines.append(text)
for name, edits in MUTANTS:
    saved, applied = {}, True
    for path, old, new in edits:
        full = os.path.join(repo, path)
        src = saved.setdefault(full, open(full).read())
        now = open(full).read()
        if now.count(old) != 1:
            say(f"{name}: NOT APPLIED ({path}: pattern found {now.count(old)} times)"); applied = False; break
        open(full, "w").write(now.replace(old, new))
    if not applied:
        for full, src in saved.items():
            open(full, "w").write(src)
        continue
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
                    f.removeprefix("tui::approval_amendment::tests::").removeprefix("tui::shell::tests::")
                    .removeprefix("tui::question::tests::").removeprefix("tui::paste::tests::")
                    for f in failed[:6])
                if len(failed) > 6:
                    why += f" (+{len(failed) - 6} more)"
                break
    finally:
        for full, src in saved.items():
            open(full, "w").write(src)
    say(f"{name}: {verdict} ({time.time()-t0:.0f}s) {why}")
status = subprocess.run(["git", "-C", repo, "status", "--porcelain"], capture_output=True, text=True).stdout
say(f"{sum(1 for l in lines if ': KILLED' in l)} of {len(MUTANTS)} killed; "
    f"the clone's tree after the run: {'clean' if not status.strip() else 'DIRTY'}")
open(out, "w").write("\n".join(lines) + "\n")
