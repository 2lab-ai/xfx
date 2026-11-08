#!/usr/bin/env python3
"""Train 8, join check 1: does the join test discriminate? Mutants planted on
a `git clone --no-local` of the train (never the worktree), one textual
change each, the file restored (`git checkout -- <file>`) after each and the
clone clean at the end; GRID-ROW-COST's mutants.py's runner (each run bounded
at LIMIT seconds, its process group killed past it: "KILLED (hang)").

Three of GRID-ROW-COST's own grid mutants (its mutants.py's text, unchanged)
and two on STOP-NEXT-2's hold, each run against the join test and
GRID-ROW-COST's session test.

    suite-lock.sh python3 -I -B join_mutants.py <clone>
"""
import os
import signal
import subprocess
import sys

LIMIT = 600
TESTS = [
    "tui::event_loop::tests::a_stops_held_rows_over_a_groups_long_rows_are_written_as_their_text_lays",
    "tui::event_loop::tests::a_groups_long_rows_stay_what_their_text_lays_through_scrolls_themes_and_a_resize",
]
SHORT = {TESTS[0]: "join", TESTS[1]: "session"}
GR = "src/tui/grid.rs"
SH = "src/tui/shell.rs"
MUTANTS = [
    ("G1 a row laid from any text answers for every text (GRID-ROW-COST's G1)", GR,
     "            laid.cols == self.cols && laid.screen == geometry.cols && *laid.text == *row",
     "            laid.cols == self.cols && laid.screen == geometry.cols"),
    ("G4 an erased row keeps its text (GRID-ROW-COST's G4)", GR,
     "        if let Some(slot) = self.slot(line) {\n            *slot = Record::default();\n        }",
     "        if let Some(slot) = self.slot(line) {\n            *slot = Record { laid: slot.laid.take(), ..Record::default() };\n        }"),
    ("G7 a placement remembers no text (GRID-ROW-COST's G7)", GR,
     "            slot.laid = Some(Laid {\n                text: std::sync::Arc::from(row),",
     "            slot.laid = None;\n            let _ = Some(Laid {\n                text: std::sync::Arc::from(row),"),
    ("S1 the stop's held tool rows dropped, never written", SH,
     "                Late::Tool(mark) => self.mark(mark),",
     "                Late::Tool(_) => {}"),
    ("S2 a tool row read behind held text written at once, ahead of it", SH,
     "    fn mark_tool(&mut self, mark: Mark) {\n        if self.late.is_empty() {",
     "    fn mark_tool(&mut self, mark: Mark) {\n        if true || self.late.is_empty() {"),
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
    r = run(clone, "cargo", "test", "--locked", "--lib", "--no-fail-fast", "--", "--exact", *TESTS,
            limit=LIMIT)
    out = r.stdout + r.stderr
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
    return r.returncode, verdict, said, out


def main():
    clone = sys.argv[1]
    assert run(clone, "git", "status", "--porcelain").stdout == "", "the clone is not clean"
    rc, verdict, said, out = tests(clone)
    print(f"# {run(clone, 'git', 'log', '--oneline', '-1').stdout.strip()[:80]}; limit {LIMIT} s a mutant")
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
