#!/usr/bin/env python3
"""The train head's three join tests against mutants planted on a
`git clone --no-local` of the head: each mutant is one textual change, the
clone restored (`git checkout -- <file>`) after each, the tree clean at the end.

    suite-lock.sh python3 -I join_mutants.py <clone>
"""
import subprocess
import sys

TESTS = [
    "tui::shell::tests::ctrl_o_names_what_a_terminal_draws_wider_and_its_caches_follow_the_palette_and_the_width",
    "tui::shell::tests::a_theme_report_while_ctrl_o_is_open_recolours_a_named_label_and_a_cancelled_line_and_back",
    "tui::toolgroup::tests::the_transcripts_row_keeps_what_a_terminal_draws_wider_whole_at_every_width",
]
SHORT = {TESTS[0]: "join-1 (Ctrl-O, caches)", TESTS[1]: "join-3 (retint, open)", TESTS[2]: "join-1 (row's cut)"}
FT, TG = "src/tui/full_transcript.rs", "src/tui/toolgroup.rs"
MUTANTS = [
    ("JM1 T-U4's group block kept across a palette", FT,
     "if kept.revision == revision && kept.palette == palette {", "if kept.revision == revision {"),
    ("JM2 T-U4's heading kept across a resize", FT,
     "let same = heading.cols == cols\n", "let same = true\n"),
    ("JM3 CTRLO-WIDER's code panel label not named", FT,
     "block.language = named(std::mem::take(&mut block.language));", ""),
    ("JM4 CTRLO-WIDER's file and tool lines not named", FT,
     "out.push_str(&drawn_wider_named(&spelled_line));", "out.push_str(&spelled_line);"),
    ("JM5 T-U4's retint of a Ctrl-O block off (drawn in one mode, shown as drawn)", FT,
     "if *drawn_in == palette.mode {", "if true {"),
    ("JM6 T-U4's row cut: a unit is a character, not a cluster", TG,
     "        None => cluster(rest),", "        None => rest.chars().next().map_or(rest.len(), char::len_utf8),"),
    ("JM7 T-U4's row cut: a spelling is not a unit", TG,
     "        Some(len) => len - 1 + cluster(&rest[len - 1..]),", "        Some(_) => cluster(rest),"),
]


def run(clone, *args):
    return subprocess.run(args, cwd=clone, capture_output=True, text=True)


def tests(clone):
    r = run(clone, "cargo", "test", "--locked", "--lib", "--no-fail-fast", "--", "--exact", *TESTS)
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
    print(f"# {run(clone, 'git', 'log', '--oneline', '-1').stdout.strip()[:80]}")
    print(f"unchanged: exit {rc}; " + "; ".join(f"{SHORT[t]} {verdict[t]}" for t in TESTS))
    killed = 0
    for name, path, old, new in MUTANTS:
        text = open(f"{clone}/{path}", encoding="utf-8").read()
        assert text.count(old) == 1, (name, text.count(old))
        open(f"{clone}/{path}", "w", encoding="utf-8").write(text.replace(old, new, 1))
        rc, verdict, said, out = tests(clone)
        run(clone, "git", "checkout", "--", path)
        failed = [t for t in TESTS if verdict[t] == "FAILED"]
        state = "KILLED" if failed else ("SURVIVED" if rc == 0 else f"OTHER (exit {rc})")
        if state == "OTHER (exit 101)" or (state.startswith("OTHER") and "error[" in out):
            state = "OTHER: " + next((l for l in out.splitlines() if l.startswith("error")), "")[:100]
        killed += state == "KILLED"
        print(f"{name}: {state}; " + "; ".join(f"{SHORT[t]} {verdict[t]}" for t in TESTS))
        for t in failed:
            print(f"    {SHORT[t]} said: {said.get(t, '')}")
    print(f"# {killed} of {len(MUTANTS)} killed; clone clean after: {run(clone, 'git', 'status', '--porcelain').stdout == ''}")


main()
