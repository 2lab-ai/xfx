#!/usr/bin/env python3
"""Train 9, join check 2: do the two join tests discriminate? Mutants planted
on a `git clone --no-local` of the train (never the worktree), one textual
change each, the file restored (`git checkout -- <file>`) after each and the
clone clean at the end; train 8's join_mutants.py's runner (each cargo run
bounded at LIMIT seconds, its process group killed past it: "KILLED (hang)").

Each mutant's text is its unit's own (QUESTIONS-20's mutants.py M1;
STOP-IN-CATALOG-WAIT's mutants.py, its single-edit ones) but P1, on the
question requester's stop arm. Each is run against the two join tests and
one test of each unit: STOP-IN-CATALOG-WAIT's catalog-wait test and
QUESTIONS-20's 20-question pty test.

    suite-lock.sh python3 -I -B join_mutants.py <clone>
"""
import os
import signal
import subprocess
import sys

LIMIT = 600
LIB = [
    "tui::worker::tests::a_stop_in_the_catalog_wait_of_a_turn_that_would_ask_twenty_questions_ends_it_as_the_stop",
    "tui::worker::tests::a_stop_in_a_turns_catalog_wait_ends_it_as_a_stop_and_records_it_as_one",
]
TUI = [
    "a_stop_at_a_twenty_question_panel_is_one_cancelled_row_and_the_next_turn_carries_its_prompt",
    "twenty_questions_are_one_panel_and_their_answers_reach_the_next_request_in_order",
]
TESTS = LIB + TUI
SHORT = {LIB[0]: "join-wait", TUI[0]: "join-panel", LIB[1]: "stop-unit", TUI[1]: "q20-unit"}
Q = "src/tools/question.rs"
P = "src/tui/question.rs"
W = "src/tui/worker.rs"
M = "src/agent/machine.rs"
B = "src/tui/bridge.rs"
MUTANTS = [
    ("Q1 the cap back to 4 (QUESTIONS-20's M1)", Q,
     "pub const MAX_QUESTIONS: usize = 20;", "pub const MAX_QUESTIONS: usize = 4;"),
    ("S1 a prompt's images do not wait for the first load (STOP-IN-CATALOG-WAIT's prompt-images-first-load-not-waited)", W,
     "    if !images.is_empty()\n        || !state.settings.effort.is_default()\n",
     "    if !state.settings.effort.is_default()\n"),
    ("S2 the image refusal asked of a stopped turn (STOP-IN-CATALOG-WAIT's image-refusal-asked-when-stopped)", W,
     "    if carries_images && !turn.token.is_cancelled() {\n", "    if carries_images {\n"),
    ("S3 the machine measures a stopped turn's compaction (STOP-IN-CATALOG-WAIT's no-machine-guard)", M,
     "        if self.request.cancel.is_cancelled() {\n            stopped = Some(TurnError::Cancelled);\n        } else if self.compactor.is_some() {\n",
     "        if self.compactor.is_some() {\n"),
    ("S4 a stop outlives its turn (STOP-IN-CATALOG-WAIT's stop-leaks-to-next-turn)", B,
     "        self.mirror.reset();\n", ""),
    ("P1 a stop at the panel answers the batch declined, not aborted", P,
     "                        self.control.give_back(stop);\n                        return QuestionReply::Interrupted;",
     "                        self.control.give_back(stop);\n                        return QuestionReply::Declined;"),
]


def run(clone, *args, limit=None):
    if limit is None:
        return subprocess.run(args, cwd=clone, capture_output=True, text=True)
    process = subprocess.Popen(args, cwd=clone, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                               text=True, start_new_session=True)
    try:
        out, _ = process.communicate(timeout=limit)
        return subprocess.CompletedProcess(args, process.returncode, out, "")
    except subprocess.TimeoutExpired:
        # Only this run's own process group: its cargo and the test binary it ran.
        os.killpg(process.pid, signal.SIGKILL)
        out, _ = process.communicate()
        return subprocess.CompletedProcess(args, None, out or "", "")


def tests(clone):
    out, codes = "", []
    for target, names in ((["--lib"], LIB), (["--test", "tui"], TUI)):
        r = run(clone, "cargo", "test", "--locked", *target, "--no-fail-fast", "--", "--exact", *names,
                limit=LIMIT)
        codes.append(r.returncode)
        out += r.stdout + r.stderr
    verdict = {}
    for t in TESTS:
        verdict[t] = "ok" if f"test {t} ... ok" in out else ("FAILED" if f"test {t} ... FAILED" in out else "?")
    said = {}
    for t in TESTS:
        marker = f"---- {t} stdout ----"
        if marker in out:
            body = out.split(marker, 1)[1].splitlines()
            panic = next((l for l in body if "panicked at" in l), "")
            rest = body[body.index(panic) + 1:] if panic in body else []
            said[t] = " / ".join(l.strip() for l in rest[:3] if l.strip())[:220]
    rc = None if None in codes else max(codes)
    return rc, verdict, said, out


def main():
    clone = sys.argv[1]
    assert run(clone, "git", "status", "--porcelain").stdout == "", "the clone is not clean"
    rc, verdict, said, out = tests(clone)
    print(f"# {run(clone, 'git', 'log', '--oneline', '-1').stdout.strip()[:80]}; limit {LIMIT} s a cargo run")
    print(f"unchanged: exit {rc}; " + "; ".join(f"{SHORT[t]} {verdict[t]}" for t in TESTS))
    killed = 0
    for name, path, old, new in MUTANTS:
        text = open(f"{clone}/{path}", encoding="utf-8").read()
        assert text.count(old) == 1, (name, text.count(old))
        open(f"{clone}/{path}", "w", encoding="utf-8").write(text.replace(old, new, 1))
        rc, verdict, said, out = tests(clone)
        run(clone, "git", "checkout", "--", path)
        failed = [t for t in TESTS if verdict[t] == "FAILED"]
        if rc is None:
            state = "KILLED (hang)"
        elif failed:
            state = "KILLED"
        elif rc == 0:
            state = "SURVIVED"
        else:
            error = next((l for l in out.splitlines() if l.startswith("error")), "")
            state = f"OTHER (exit {rc}): {error[:100]}"
        killed += "KILLED" in state
        print(f"{name}: {state}; " + "; ".join(f"{SHORT[t]} {verdict[t]}" for t in TESTS))
        for t in failed:
            print(f"    {SHORT[t]} said: {said.get(t, '')}")
        sys.stdout.flush()
    print(f"# {killed} of {len(MUTANTS)} killed; clone clean after: {run(clone, 'git', 'status', '--porcelain').stdout == ''}")


if __name__ == "__main__":
    main()
