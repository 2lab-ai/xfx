#!/usr/bin/env python3
"""GRID-ROW-COST's tests against mutants planted on a `git clone --no-local` of
the head (never the worktree): ARGROW-CACHE's mutants.py with this unit's tests
and mutants. Each mutant is one textual change, the clone restored
(`git checkout -- <file>`) after each, the tree clean at the end. Each mutant's
test run is bounded: `timeout`-style, its process group is killed after LIMIT
seconds, and a mutant that loops is "KILLED (hang)".

    suite-lock.sh python3 -I -B mutants.py <clone> [<mutant id>,...]

A mutant the unit's tests let through is run again against the modules whose
tests pinned that code before this unit (WIDER), and reported as killed by them
or as surviving them too. G3 is declared equivalent before the run: a grid's
width changes only through `Grid::resize`, which forgets every record, so the
width kept with a row's text always equals the grid's.
"""
import os
import re
import signal
import subprocess
import sys

LIMIT = 600

TESTS = [
    # RED (1078ff37, c3f6fa48)
    "tui::grid::tests::a_row_this_grid_placed_is_matched_again_without_reading_it",
    "tui::frame::tests::an_append_that_leaves_a_long_row_where_it_is_reads_none_of_it_again",
    "tui::transcript::tests::a_block_with_three_long_rows_drawn_again_cuts_none_of_them_again",
    "tui::toolgroup::tests::a_call_row_with_a_long_argument_is_held_at_its_length",
    "tui::full_transcript::tests::an_arguments_rows_are_held_at_their_count",
    # equality with a fresh layout
    "tui::grid::tests::a_rows_text_moves_with_its_cells_and_goes_with_any_change_to_them",
    "tui::grid::tests::a_retint_leaves_a_row_that_answers_as_a_fresh_placement",
    "tui::grid::tests::a_row_laid_for_a_narrower_screen_is_not_the_row_this_screen_lays",
    "tui::check::tests::a_kept_replay_lays_the_cells_a_fresh_one_lays_and_reads_none_of_the_row",
    "tui::check::tests::a_kept_replay_is_found_only_for_its_text_at_its_widths_and_for_one_vector",
    "tui::check::tests::a_kept_replay_still_refuses_a_screen_that_does_not_hold_the_declared_row",
    "tui::event_loop::tests::a_groups_long_rows_stay_what_their_text_lays_through_scrolls_themes_and_a_resize",
    "tui::transcript::tests::a_long_row_the_block_stopped_writing_is_let_go_by_the_write_after",
]
SHORT = {
    TESTS[0]: "red-grid", TESTS[1]: "red-frame", TESTS[2]: "red-thrash", TESTS[3]: "red-row",
    TESTS[4]: "red-rail", TESTS[5]: "moves", TESTS[6]: "retint", TESTS[7]: "narrower",
    TESTS[8]: "kept-cells", TESTS[9]: "kept-key", TESTS[10]: "kept-refuses",
    TESTS[11]: "session", TESTS[12]: "let-go",
}
GR = "src/tui/grid.rs"
CK = "src/tui/check.rs"
TR = "src/tui/transcript.rs"
TG = "src/tui/toolgroup.rs"
FT = "src/tui/full_transcript.rs"
MUTANTS = [
    # the grid's record of what a row was laid from
    ("G1 a row laid from any text answers for every text", GR,
     "            laid.cols == self.cols && laid.screen == geometry.cols && *laid.text == *row",
     "            laid.cols == self.cols && laid.screen == geometry.cols"),
    ("G2 the screen's width left out of the key", GR,
     "            laid.cols == self.cols && laid.screen == geometry.cols && *laid.text == *row",
     "            laid.cols == self.cols && *laid.text == *row"),
    ("G3 the grid's width left out of the key (declared equivalent)", GR,
     "            laid.cols == self.cols && laid.screen == geometry.cols && *laid.text == *row",
     "            laid.screen == geometry.cols && *laid.text == *row"),
    ("G4 an erased row keeps its text", GR,
     "        if let Some(slot) = self.slot(line) {\n            *slot = Record::default();\n        }",
     "        if let Some(slot) = self.slot(line) {\n            *slot = Record { laid: slot.laid.take(), ..Record::default() };\n        }"),
    ("G5 a retinted row keeps its text", GR,
     "            if here > 0 {\n                if let Some(slot) = self.slot(line) {\n                    slot.laid = None;",
     "            if here > usize::MAX - 1 {\n                if let Some(slot) = self.slot(line) {\n                    slot.laid = None;"),
    ("G6 a writer's record forgets the text", GR,
     "            slot.kind = kind;\n            slot.redraw = redraw;",
     "            slot.kind = kind;\n            slot.redraw = redraw;\n            slot.laid = None;"),
    ("G7 a placement remembers no text", GR,
     "            slot.laid = Some(Laid {\n                text: std::sync::Arc::from(row),",
     "            slot.laid = None;\n            let _ = Some(Laid {\n                text: std::sync::Arc::from(row),"),
    # the check's kept replays
    ("C1 the screen's width left out of a kept replay's key", CK,
     "|kept: &Replayed| kept.cols == cols && kept.screen == screen && kept.text == text;",
     "|kept: &Replayed| kept.cols == cols && kept.text == text;"),
    ("C2 the plane's width left out of a kept replay's key", CK,
     "|kept: &Replayed| kept.cols == cols && kept.screen == screen && kept.text == text;",
     "|kept: &Replayed| kept.screen == screen && kept.text == text;"),
    ("C3 a kept replay found for any text at its widths", CK,
     "|kept: &Replayed| kept.cols == cols && kept.screen == screen && kept.text == text;",
     "|kept: &Replayed| kept.cols == cols && kept.screen == screen && kept.text.len() == text.len();"),
    ("C4 a kept replay found trusts the row the screen holds", CK,
     "            for (cell, kept) in cells.iter_mut().zip(&kept.cells) {\n                cell.clone_from(kept);\n            }",
     "            let _ = kept;"),
    ("C5 a vector's replays never kept for the next", CK,
     "        self.kept = std::mem::take(&mut self.laid);",
     "        self.laid.clear();"),
    ("C6 every vector's replays kept for good", CK,
     "        self.kept = std::mem::take(&mut self.laid);",
     "        let laid = std::mem::take(&mut self.laid);\n        self.kept.extend(laid);"),
    ("C7 a kept replay found is not carried into this vector's", CK,
     "        let found = self.kept.swap_remove(at);\n        self.laid.push(found);\n        self.laid.last()",
     "        self.kept.get(at)"),
    ("C8 no row is long enough to keep", CK,
     "const REPLAY_KEPT_BYTES: usize = 4 * 1024;",
     "const REPLAY_KEPT_BYTES: usize = usize::MAX;"),
    # the transcript's kept cuts
    ("T1 a block's write never turns its cuts over", TR,
     "        self.turn_kept_rows();\n",
     ""),
    ("T2 every write's cuts kept for good", TR,
     "        rows.kept = std::mem::take(&mut rows.cut);",
     "        let cut = std::mem::take(&mut rows.cut);\n        rows.kept.extend(cut);"),
    ("T3 a kept cut found is not carried into this write's", TR,
     "            let found = rows.kept.swap_remove(at);\n            let written = found.written();\n            rows.cut.push(found);\n            return written;",
     "            return rows.kept[at].written();"),
    ("T4 a cut row kept as the row it was cut from", TR,
     "            written: (written != row).then(|| written.clone()),",
     "            written: None,"),
    ("T5 a row that fits kept twice", TR,
     "            written: (written != row).then(|| written.clone()),",
     "            written: Some(written.clone()),"),
    # the buffers held at their size
    ("B1 a call's row built by format!", TG,
     "    let mut row =\n        String::with_capacity(text.len() + connector.len_utf8() + 1 + painted.len() + reset.len());\n    row.push_str(text);\n    row.push(connector);\n    row.push(' ');\n    row.push_str(painted);\n    row.push_str(reset);\n    row",
     "    format!(\"{text}{connector} {painted}{reset}\")"),
    ("B2 an argument's rows left as the growth left them", FT,
     "        rows.shrink_to_fit();\n        tabs.shrink_to_fit();\n",
     ""),
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


WIDER = ["tui::grid::", "tui::frame::", "tui::check::", "tui::transcript::", "tui::toolgroup::",
         "tui::full_transcript::", "tui::event_loop::tests::a_theme"]


def wider(clone):
    r = run(clone, "cargo", "test", "--locked", "--lib", "--no-fail-fast", "--", *WIDER, limit=LIMIT)
    out = r.stdout + r.stderr
    failed = sorted(set(re.findall(r"^test (\S+) \.\.\. FAILED", out, re.M)))
    result = next((l for l in out.splitlines() if l.startswith("test result:")), "")
    return r.returncode, failed, result


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
            msg = next((l for l in body[body.index(panic) + 1:] if l.strip()), "") if panic in body else ""
            said[t] = msg.strip()[:110]
    return r.returncode, verdict, said, out


def main():
    clone = sys.argv[1]
    chosen = set(sys.argv[2].split(",")) if len(sys.argv) > 2 else None
    assert run(clone, "git", "status", "--porcelain").stdout == "", "the clone is not clean"
    rc, verdict, said, out = tests(clone)
    print(f"# {run(clone, 'git', 'log', '--oneline', '-1').stdout.strip()[:80]}; limit {LIMIT} s a mutant")
    print(f"unchanged: exit {rc}; " + "; ".join(f"{SHORT[t]} {verdict[t]}" for t in TESTS))
    killed = 0
    mutants = [m for m in MUTANTS if chosen is None or m[0].split()[0] in chosen]
    for name, path, old, new in mutants:
        text = open(f"{clone}/{path}", encoding="utf-8").read()
        assert text.count(old) == 1, (name, text.count(old))
        open(f"{clone}/{path}", "w", encoding="utf-8").write(text.replace(old, new, 1))
        rc, verdict, said, out = tests(clone)
        wide = wider(clone) if rc == 0 else None
        run(clone, "git", "checkout", "--", path)
        failed = [t for t in TESTS if verdict[t] == "FAILED"]
        if rc is None:
            state = "KILLED (hang)"
        elif failed:
            state = "KILLED"
        elif rc == 0 and wide[0] is None:
            state = "SURVIVED the unit's tests; KILLED (hang) by the modules' tests"
        elif rc == 0 and wide[1]:
            state = f"SURVIVED the unit's tests; KILLED by the modules' tests ({len(wide[1])} failed)"
        elif rc == 0:
            state = f"SURVIVED, the modules' tests too ({wide[2]})"
        else:
            error = next((l for l in out.splitlines() if l.startswith("error")), "")
            state = f"OTHER (exit {rc}): {error[:100]}"
        killed += "KILLED" in state
        print(f"{name}: {state}; " + "; ".join(f"{SHORT[t]} {verdict[t]}" for t in TESTS))
        for t in failed:
            print(f"    {SHORT[t]} said: {said.get(t, '')}")
        if wide and wide[1]:
            for t in wide[1][:6]:
                print(f"    failed: {t}")
            print(f"    {wide[2]}")
        sys.stdout.flush()
    print(f"# {killed} of {len(mutants)} killed; clone clean after: {run(clone, 'git', 'status', '--porcelain').stdout == ''}")


if __name__ == "__main__":
    main()
