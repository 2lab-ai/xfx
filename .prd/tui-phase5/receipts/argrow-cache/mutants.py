#!/usr/bin/env python3
"""ARGROW-CACHE's tests against mutants planted on a `git clone --no-local` of
the head (never the worktree): each mutant is one textual change, the clone
restored (`git checkout -- <file>`) after each, the tree clean at the end. Each
mutant's test run is bounded: `timeout` kills its process group after LIMIT
seconds, and a mutant that loops is "KILLED (hang)".

    suite-lock.sh python3 -I mutants.py <clone>
"""
import os
import signal
import subprocess
import sys

LIMIT = 600

TESTS = [
    "tui::full_transcript::tests::ctrl_o_wraps_a_calls_arguments_once_for_a_width_and_not_again_for_a_join_or_a_theme",
    "tui::shell::tests::ctrl_o_with_arguments_through_its_caches_is_the_session_laid_out_from_nothing_after_every_event",
    "tui::full_transcript::tests::rows_kept_unpainted_are_the_rows_railed_painted_at_once",
    "tui::wrap::tests::painting_reads_the_clusters_graphemes_reads",
    "tui::wrap::tests::a_printable_ascii_character_is_one_cell",
    "tui::wrap::tests::a_run_stepped_over_is_the_clusters_graphemes_reads_there",
    "tui::prose::tests::stepping_over_what_the_wrap_only_counts_changes_no_row",
    "tui::prose::tests::a_wrap_of_megabytes_of_one_token_steps_a_row_at_a_time",
    "tui::full_transcript::tool_status::tests::counting_a_run_at_once_changes_no_break",
    # the tests that held these paths before this unit
    "tui::full_transcript::tests::ctrl_o_builds_a_groups_block_once_and_lays_each_heading_out_once_for_what_it_shows",
    "tui::shell::tests::ctrl_o_names_what_a_terminal_draws_wider_and_its_caches_follow_the_palette_and_the_width",
    "tui::full_transcript::tool_status::tests::keeping_the_end_of_a_run_of_blanks_changes_no_break",
]
SHORT = {
    TESTS[0]: "count", TESTS[1]: "equal", TESTS[2]: "railed", TESTS[3]: "painting",
    TESTS[4]: "ascii1", TESTS[5]: "runs", TESTS[6]: "prose", TESTS[7]: "prose-mb",
    TESTS[8]: "scan", TESTS[9]: "tu4-count", TESTS[10]: "join-1", TESTS[11]: "scan-run",
}
FT = "src/tui/full_transcript.rs"
TS = "src/tui/full_transcript/tool_status.rs"
PR = "src/tui/prose.rs"
WR = "src/tui/wrap.rs"
MUTANTS = [
    # the cache
    ("M1 an argument's rows wrapped again on every layout (no cache)", FT,
     "            if wrapped.cols == cols {\n                return Rc::clone(wrapped);",
     "            if false {\n                return Rc::clone(wrapped);"),
    ("M2 an argument's rows kept across a resize (width not in the key)", FT,
     "            if wrapped.cols == cols {", "            if true {"),
    ("M3 a call's new arguments take the old ones' rows", FT,
     "|call| call.arguments = arguments.iter().map(CallArgument::of).collect(),",
     "|call| { let new: Vec<CallArgument> = arguments.iter().map(CallArgument::of).collect(); for (n, o) in new.iter().zip(&call.arguments) { *n.wrapped.borrow_mut() = o.wrapped.borrow().clone(); } call.arguments = new; },"),
    ("M4 an argument painted in the dark palette whatever the entry's", FT,
     "Run::Railed(railed, palette) => railed.row(index, *palette).into(),",
     "Run::Railed(railed, _) => railed.row(index, Palette { mode: Mode::Dark }).into(),"),
    ("M5 the argument's trailing break not left off", FT,
     "        let len = line.strip_suffix('\\n').unwrap_or(line).len();",
     "        let len = line.len();"),
    ("M6 a window starts a row late in each entry", FT,
     "            rows.copy(start.saturating_sub(at), end - at, &mut out);",
     "            rows.copy(start.saturating_sub(at) + 1, end - at, &mut out);"),
    ("M7 a window of runs ignores where each run starts", FT,
     "            let (lo, hi) = (from.max(at), to.min(at + len));",
     "            let (lo, hi) = (from, to.min(at + len));"),
    ("M8 a kept row keeps its trailing spaces", FT,
     "        let piece = piece.trim_end_matches([' ', '\\n']);\n        let piece = clip(piece, width);\n        format!(",
     "        let piece = clip(&piece, width);\n        format!("),
    ("M9 a blank line's row is not the blank row", FT,
     "            if line.is_empty() {\n                rows.push(RailRow {\n                    start: at,\n                    end: at,\n                    blank: true,",
     "            if line.is_empty() {\n                rows.push(RailRow {\n                    start: at,\n                    end: at,\n                    blank: false,"),
    # the wraps' fast paths
    ("W1 painting: any ASCII first byte is a cluster alone (CR LF split)", WR,
     "            let cluster = first_cluster(rest).unwrap_or(rest);",
     "            let cluster = if rest.as_bytes()[0].is_ascii() && rest.len() > 1 && rest.as_bytes()[1].is_ascii() { &rest[..1] } else { first_cluster(rest).unwrap_or(rest) };"),
    ("W2 one byte of DEL counted a cell", WR,
     "    if let [byte] = cluster.as_bytes() {\n        if (0x20..=0x7e).contains(byte) {",
     "    if let [byte] = cluster.as_bytes() {\n        if (0x20..=0x7f).contains(byte) {"),
    ("W3 single_cells: a space counts as a cell of the run", WR,
     "        .position(|byte| !(0x21..=0x7e).contains(byte))",
     "        .position(|byte| !(0x20..=0x7e).contains(byte))"),
    ("W4 single_cells: the last before a non-ASCII byte kept", WR,
     "    if run > 0 && bytes.get(at + run).is_some_and(|byte| !byte.is_ascii()) {",
     "    if false {"),
    ("W5 repeats: the last copy counted too", WR,
     "            .is_some_and(|two| two[..len] == *unit && two[len..] == *unit)",
     "            .is_some_and(|two| two[..len] == *unit)"),
    ("P1 prose: a run stepped over past the margin", PR,
     "        let room = (self.cols + 1).saturating_sub(self.col);",
     "        let room = (self.cols + 2).saturating_sub(self.col);"),
    ("P2 prose: a run stepped over inside a cluster cut into characters", PR,
     "                if self.fast && whole {", "                if self.fast {"),
    ("P3 prose: the final-word test steps over a space", PR,
     "        if fast && !passed {", "        if fast {"),
    ("S1 scan: a run counted past what is left of the row", TS,
     "            let room = max_width - width;", "            let room = max_width - width + 1;"),
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
            msg = next((l for l in body[body.index(panic) + 1:] if l.strip()), "") if panic in body else ""
            said[t] = msg.strip()[:110]
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
        killed += state.startswith("KILLED")
        print(f"{name}: {state}; " + "; ".join(f"{SHORT[t]} {verdict[t]}" for t in TESTS))
        for t in failed:
            print(f"    {SHORT[t]} said: {said.get(t, '')}")
    print(f"# {killed} of {len(MUTANTS)} killed; clone clean after: {run(clone, 'git', 'status', '--porcelain').stdout == ''}")


if __name__ == "__main__":
    main()
