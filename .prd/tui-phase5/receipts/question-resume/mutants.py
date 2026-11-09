"""QUESTION-RESUME's mutants, each on a throwaway clone at the code head.

usage: python3 -I mutants.py <clone> <out>

Each mutant is one exact text substitution (asserted to match once), then the
unit's tests, then `git checkout -- .`. Killed = the tests fail; survived =
they pass. The clone is its own repository (`git clone --no-local`), so a
mutant acts on nothing but it.
"""
import os
import subprocess
import sys

TESTS = [
    "a_resumed_", "a_hostile_answer_document", "a_saved_", "anything_but_an_answer",
    "a_hostile_log_reaches_the_ui_inert", "an_ask_replays", "a_step_that_asked",
    "a_question_is_never_part_of_a_group",
]

H = "src/tui/hydration.rs"
Q = "src/tools/question.rs"
G = "src/tui/toolgroup.rs"
B = "src/tui/bridge.rs"

MUTANTS = [
    ("M1 the early return put back: the replay hands an ask to the group again, whose start drops it",
     [(H, "if tool == ASK_USER_QUESTION.name() {", "if false && tool == ASK_USER_QUESTION.name() {")]),
    ("M2 the group's own early return removed: a question becomes a group row",
     [(G, "if shown.presentation.activity == Activity::Ask {", "if false && shown.presentation.activity == Activity::Ask {")]),
    ("M3 the bound one past twenty", [(Q, "(MIN_QUESTIONS..=MAX_QUESTIONS)", "(MIN_QUESTIONS..=MAX_QUESTIONS + 1)")]),
    ("M4 the bound one short of twenty", [(Q, "(MIN_QUESTIONS..=MAX_QUESTIONS)", "(MIN_QUESTIONS..MAX_QUESTIONS)")]),
    ("M5 the bound one below one (an empty batch)", [(Q, "(MIN_QUESTIONS..=MAX_QUESTIONS)", "(0..=MAX_QUESTIONS)")]),
    ("M6 the bound one above one", [(Q, "(MIN_QUESTIONS..=MAX_QUESTIONS)", "(MIN_QUESTIONS + 1..=MAX_QUESTIONS)")]),
    ("M7 a malformed document drawn as answered: a missing field defaults",
     [(Q, "#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]\npub struct SavedAnswer",
       "#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]\n#[serde(default)]\npub struct SavedAnswer")]),
    ("M8 a failed call's document drawn as answered: `ok` ignored",
     [(H, "match ok.then(|| decode_answers(output)).flatten() {", "match decode_answers(output) {")]),
    ("M9 a document that does not decode drawn as an (empty) answered batch",
     [(H, "None => HydratedStep::Asked,", "None => HydratedStep::Answered(Vec::new()),")]),
    ("M10 a step that asked starts its calls together",
     [(H, "if asks {", "if false && asks {")]),
    ("M11 the decoded texts not made inert at the bridge",
     [(B, "question: inert_owned(saved.question),", "question: saved.question,"),
      (B, "answer: inert_owned(saved.answer),", "answer: saved.answer,")]),
]


def run(clone, out):
    lines = []
    for name, edits in MUTANTS:
        for path, old, new in edits:
            full = os.path.join(clone, path)
            text = open(full, encoding="utf-8").read()
            assert text.count(old) == 1, (name, path, text.count(old))
            open(full, "w", encoding="utf-8").write(text.replace(old, new))
        env = dict(os.environ)
        cmd = ["cargo", "test", "--locked", "--lib", "--"] + TESTS
        try:
            p = subprocess.run(cmd, cwd=clone, env=env, capture_output=True, text=True, timeout=1500)
            rc, text = p.returncode, p.stdout + p.stderr
        except subprocess.TimeoutExpired:
            rc, text = "timeout", ""
        failed = [l.split()[1] for l in text.splitlines() if l.startswith("test ") and l.endswith("FAILED")]
        compiled = "error[" not in text and "could not compile" not in text
        verdict = "KILLED" if failed else ("SURVIVED" if rc == 0 else "KILLED (no build: %s)" % rc)
        if not compiled:
            verdict = "INVALID (did not compile)"
        lines.append("%s: %s" % (name, verdict))
        for f in failed:
            lines.append("    failed: %s" % f)
        subprocess.run(["git", "checkout", "--", "."], cwd=clone, check=True)
        open(out, "w").write("\n".join(lines) + "\n")
        print(lines[-1 - len(failed)], flush=True)
    clean = subprocess.run(["git", "status", "--porcelain"], cwd=clone, capture_output=True, text=True).stdout
    lines.append("clone clean after the run: %s" % ("yes" if not clean.strip() else "NO"))
    open(out, "w").write("\n".join(lines) + "\n")


if __name__ == "__main__":
    run(sys.argv[1], sys.argv[2])
