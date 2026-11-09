#!/usr/bin/env python3
"""Train 11's join tests: do they discriminate? Mutants planted on a `git
clone --no-local` of the train at its head (never the worktree), each a list
of textual changes (each must match exactly once), the files restored (`git
checkout -- <file>`) after each and the clone clean at the end; train 10's
join_mutants.py's runner (each cargo run bounded at LIMIT seconds, its
process group killed past it: "KILLED (hang)"), its list this train's.

Each mutant is a single edit at a hop the join tests cross, one unit's:
P1 (PASTE-AFTER-END's, on main), P2 (PASTE-GAP's), S1 (SUBMIT-CARD-PREVIEW's),
C1 and C2 (CATALOG-REFRESH's). Run against the two join tests and one test
of each unit on the path.

    suite-lock.sh python3 -I -B join_mutants.py <clone>
"""
import os
import signal
import subprocess
import sys

LIMIT = 900
LIB = [
    "tui::event_loop::tests::a_tall_paste_with_enters_behind_its_marker_and_in_its_quarantine_previews_once_after_it",
    "tui::event_loop::tests::a_submit_that_previews_asks_a_due_catalog_once_and_writes_the_bytes_it_writes_without_one",
    "tui::event_loop::tests::a_delivery_past_the_32nd_read_goes_on_into_the_next_turn_unsettled",
    "tui::event_loop::tests::a_readiness_gap_inside_one_paste_does_not_turn_the_rest_of_it_into_keys",
    "tui::event_loop::tests::the_tick_that_shows_a_preview_writes_the_card_in_the_next_frame",
    "tui::shell::tests::a_prompt_past_a_failed_catalogs_backoff_asks_for_it_and_one_inside_asks_nothing",
]
TUI = []
TESTS = LIB + TUI
SHORT = {LIB[0]: "join-paste", LIB[1]: "join-catalog", LIB[2]: "pae-unit", LIB[3]: "gap-unit",
         LIB[4]: "scp-unit", LIB[5]: "cr-unit"}
S = "src/tui/shell.rs"
I = "src/tui/input.rs"
MUTANTS = [
    ("P1 the bytes behind a paste's end marker are decoded (PASTE-AFTER-END's)",
     [(I, "        if let Some(behind) = self.behind.as_mut() {\n",
       "        if let Some(behind) = self.behind.as_mut().filter(|_| false) {\n")]),
    ("P2 no quarantine after a settled paste (PASTE-GAP's)",
     [(I, "                quiet_since: Some(now),\n            });\n            return Delivery::Pasted { dropped };\n",
       "                quiet_since: Some(now),\n            });\n            self.quarantine = None;\n"
       "            return Delivery::Pasted { dropped };\n")]),
    ("S1 no preview: the card whole at once (SUBMIT-CARD-PREVIEW's)",
     [(S, "if let Some(room) = self.preview_room() {", "if let Some(room) = self.preview_room().filter(|_| false) {")]),
    ("C1 a submitted prompt is no catalog activity (CATALOG-REFRESH's send)",
     [(S, "        // `super::worker`'s `await_failed_catalog_retry`).\n"
          "        let _ = self.note_catalog_activity(self.catalog_clock.now());\n",
       "        // `super::worker`'s `await_failed_catalog_retry`).\n")]),
    ("C2 a failure's hold holds nothing (CATALOG-REFRESH's note_catalog_activity)",
     [(S, "            CatalogKnown::Unread(_, retry_at) => retry_at.is_due(now),\n",
       "            CatalogKnown::Unread(_, _retry_at) => true,\n")]),
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
        if not names:
            continue
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
    for name, edits in MUTANTS:
        for path, old, new in edits:
            text = open(f"{clone}/{path}", encoding="utf-8").read()
            assert text.count(old) == 1, (name, path, text.count(old))
            open(f"{clone}/{path}", "w", encoding="utf-8").write(text.replace(old, new, 1))
        rc, verdict, said, out = tests(clone)
        for path in {path for path, _, _ in edits}:
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
