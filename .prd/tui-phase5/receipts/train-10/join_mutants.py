#!/usr/bin/env python3
"""Train 10's join tests: do they discriminate? Mutants planted on a `git
clone --no-local` of the train at its head (never the worktree), each a list
of textual changes (each must match exactly once), the files restored (`git
checkout -- <file>`) after each and the clone clean at the end; train 9's
join_mutants.py's runner (each cargo run bounded at LIMIT seconds, its
process group killed past it: "KILLED (hang)").

Each mutant is its unit's own where the unit's receipt has one
(MODEL-FUZZY's mutants.py: F1, F2; PASTE-AFTER-END's 1 and 6, as train 9
ran them: X1, X6), or a single edit at the meeting the join test is about
(C1, N1, N2, Q1, Q1+Q2). Run against the four join tests and one test of
each unit on the path.

    suite-lock.sh python3 -I -B join_mutants.py <clone>
"""
import os
import signal
import subprocess
import sys

LIMIT = 900
LIB = [
    "tui::shell::tests::a_catalog_read_that_fails_too_large_or_stalled_then_loads_ranks_in_three_tiers_and_keeps_its_mark_by_id",
    "tui::worker::tests::a_stop_through_a_dropped_compaction_tells_it_once_and_leaves_the_history_notice_to_the_turn_that_runs",
    "tui::event_loop::tests::a_paste_into_the_at_word_with_an_enter_behind_its_marker_is_kept_trusted_and_sends_nothing",
    "tui::shell::tests::a_twenty_question_panel_over_an_open_at_list_has_its_tab_arrows_and_enter",
    "tui::model_menu::tests::better_matches_list_first_and_the_mark_keeps_its_model_through_a_revalidation",
    "tui::worker::tests::a_compaction_a_stop_drops_before_it_began_is_told_how_it_ended",
    "tui::worker::tests::a_turn_stopped_before_its_request_leaves_the_history_notice_to_the_next_turn",
    "tui::shell::tests::a_paste_with_input_behind_its_end_marker_reaches_the_answer_and_answers_nothing",
    "tui::event_loop::tests::a_new_at_words_arrows_wait_for_the_frame_that_shows_its_rows",
]
TUI = []
TESTS = LIB + TUI
SHORT = {LIB[0]: "join-catalog", LIB[1]: "join-compact", LIB[2]: "join-paste", LIB[3]: "join-question",
         LIB[4]: "fuzzy-unit", LIB[5]: "cnl-unit", LIB[6]: "hns-unit", LIB[7]: "paste-unit", LIB[8]: "at-unit"}
S = "src/tui/shell.rs"
W = "src/tui/worker.rs"
MS = "src/tui/model_search.rs"
MM = "src/tui/model_menu.rs"
I = "src/tui/input.rs"
MUTANTS = [
    ("F1 no ranks, catalog order (MODEL-FUZZY's tier-order mutant)",
     [(MS, "found.sort_by(|(left, _), (right, _)| right.cmp(left));", "")]),
    ("F2 the mark looked up in catalog order on a reload (MODEL-FUZZY's)",
     [(MM, ".and_then(|id| items.iter().position(|item| item.id == id))",
       ".and_then(|id| self.items.iter().position(|item| item.id == id))")]),
    ("C1 a failed refresh behind rows drops them (the band's CatalogRefreshFailed arm)",
     [(S, "menu.set_note(note);", "menu.failed(retry);")]),
    ("N1 a dropped compaction is not told (COMPACT-NOTICE-LATE's abandon)",
     [(W, "let told = !matches!(item, TurnWork::Compact)\n                || bridge::try_send_terminal(events, UiEvent::Compaction(Update::Dropped));",
       "let told = { let _ = events; true };")]),
    ("N2 a history notice is marked said whether or not it was handed on (HISTORY-NOTICE-STOP's one_turn)",
     [(W, "        if bridge::send_ui(events, &turn.token, UiEvent::Notice(shown))\n            .await\n            .is_ok()\n        {\n            conversation.said(notice);\n        }",
       "        let _ = bridge::send_ui(events, &turn.token, UiEvent::Notice(shown)).await;\n        conversation.said(notice);")]),
    ("X1 the bytes behind the marker are decoded (PASTE-AFTER-END's 1)",
     [(I, "        if let Some(behind) = self.behind.as_mut() {\n"
          "            *behind = behind.saturating_add(1);\n"
          "            return;\n"
          "        }\n", "")]),
    ("X6 no warning (PASTE-AFTER-END's 6)",
     [(S, "                if dropped > 0 {\n                    self.ignored_behind_paste();\n                }\n",
       "                let _ = dropped;\n")]),
    ("Q1 a question leaves the @ list up (ask_question's dismissal)",
     [(S, "        self.dismiss_picker();\n        self.ask = Some(panel);", "        self.ask = Some(panel);")]),
    ("Q1+Q2 and the list is offered the keys before the panel",
     [(S, "        self.dismiss_picker();\n        self.ask = Some(panel);", "        self.ask = Some(panel);"),
      (S, "            if self.ask.is_some() {\n                self.answer(event, now);",
       "            if self.offer_to_files(&event) {\n                continue;\n            }\n            if self.ask.is_some() {\n                self.answer(event, now);")]),
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
