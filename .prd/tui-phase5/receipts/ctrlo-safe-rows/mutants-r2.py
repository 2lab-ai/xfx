"""T-U1-SAFE round 2's mutants: each applied alone to a throwaway clone, the tests that
pin it run, then the file put back. usage: python3 -I -B mutants-r2.py <clone> <out>"""
import subprocess, sys, os
clone, out = sys.argv[1:3]
FT = os.path.join(clone, "src/tui/full_transcript.rs")
LP = os.path.join(clone, "scripts/fxcap/ledger.py")
MUTANTS = [
    ("U+061C not named (divergence b reverted, full_transcript.rs:497-498)", FT,
     """            if spelled.contains('\\u{061c}') {
                out.push_str(&spelled.replace('\\u{061c}', "\\\\u{061c}"));
            } else {
                out.push_str(&spelled);
            }""",
     """            out.push_str(&spelled);""",
     ["cargo", "test", "--locked", "--lib", "--", "tui::full_transcript::tests::an_argument_row", "tui::full_transcript::tests::a_read_row",
      "tui::full_transcript::tests::a_command_row_is", "tui::full_transcript::tests::no_detail_row"]),
    ("keep() not sticky (full_transcript.rs:738-740)", FT,
     """            if self.overflow {
                return;
            }
            if self.kept.len()""",
     """            if self.kept.len()""",
     ["cargo", "test", "--locked", "--lib", "--", "tui::full_transcript::tests::a_commands_stream_is_read"]),
    ("a command's tab measured again where it lands (railed, as at 28f3fda5)", FT,
     """        for piece in super::prose::literal_drawn(line, width, rail) {
            let piece = piece.trim_end_matches([' ', '\\n']);""",
     """        for row in super::prose::literal(line, width, rail) {
            let piece = super::prose::expand_tabs(&line[row.start..row.end], usize::from(rail) + 1, usize::from(cols));
            let piece = piece.trim_end_matches([' ', '\\n']);""",
     ["cargo", "test", "--locked", "--lib", "--", "tui::full_transcript::tests::a_command_lines_tab", "tui::full_transcript::tests::a_command_row_is"]),
    ("a cause past its date still covers (ledger.py live_causes)", LP,
     """    return [c for c in ledger["causes"] if c["expiry_date"] is None or today <= c["expiry_date"]]""",
     """    return list(ledger["causes"])""",
     ["python3", "-I", "-B", "-c", "import sys; sys.dont_write_bytecode=True; sys.path.insert(0,'scripts/fxcap'); import unittest, test_sweep; r=unittest.TextTestRunner(verbosity=2).run(unittest.TestLoader().loadTestsFromName('test_sweep.Ledger.test_a_cause_past_its_date_covers_nothing')); sys.exit(0 if r.wasSuccessful() else 1)"]),
]
lines = []
for name, path, old, new, cmd in MUTANTS:
    text = open(path, encoding="utf-8").read()
    assert text.count(old) == 1, name
    open(path, "w", encoding="utf-8").write(text.replace(old, new))
    try:
        r = subprocess.run(["perl", "-e", "alarm 1500; exec @ARGV"] + cmd, cwd=clone, capture_output=True, text=True)
    finally:
        open(path, "w", encoding="utf-8").write(text)
    got = [l for l in (r.stdout + r.stderr).split("\n") if l.startswith("test ") or l.startswith("test_") or "test result" in l or l.startswith("FAILED") or l.startswith("OK")]
    lines.append("## %s\n# %s\n# exit %d\n%s\n" % (name, " ".join(cmd[:4] + ["..."]), r.returncode, "\n".join(got)))
open(out, "w").write("\n".join(lines))
print(open(out).read())
