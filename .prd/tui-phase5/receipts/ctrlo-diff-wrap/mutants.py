"""DIFF-WRAP's mutants: each a one-place change to `src/tui/full_transcript.rs`
in a throwaway `git clone --no-local` of the branch, run against the module's
tests; each must make a named test fail.

usage: python3 -I mutants.py <clone> <out.txt>
"""
import os
import subprocess
import sys

clone, out = sys.argv[1:3]
path = os.path.join(clone, "src", "tui", "full_transcript.rs")
MUTANTS = [
    ("M1 no reflow: every numbered row left to the walker",
     "Some(room) if !number.is_empty() => reflowed(&text, room),",
     "Some(room) if !number.is_empty() && room == 0 => reflowed(&text, room),"),
    ("M2 the clip back in place of the walker's run-on",
     'run_on(&format!("{style}{head}{text}{reset}"), cols)',
     'vec![format!("{style}{}{reset}", clip(&format!("{head}{text}"), cols))]'),
    ("M3 break at the last cell (>= for >)",
     "if cells > 0 && used.saturating_add(cells) > room {",
     "if cells > 0 && used.saturating_add(cells) >= room {"),
    ("M4 a tab counted as its four cells in the reflow",
     'let cells = if cluster == "\\t" {',
     'let cells = if cluster == "\\u{0}" {'),
    ("M5 a continuation indented one cell more",
     'let under = format!("  \\u{2502} {:1$}", "", start - 4);',
     'let under = format!("  \\u{2502} {:1$}", "", start - 3);'),
    ("M6 a tab stop past the last column",
     "let blank = blank.min(last).saturating_sub(column);",
     "let blank = blank.saturating_sub(column);"),
    ("M7 a unit of no cells may start a row",
     "if cells > 0 && used.saturating_add(cells) > room {",
     "if used.saturating_add(cells) > room {"),
]
original = open(path, encoding="utf-8").read()
env = dict(os.environ)
for key in ("GIT_DIR", "GIT_INDEX_FILE", "GIT_WORK_TREE"):
    env.pop(key, None)
lines = []
failed_all = True
try:
    for name, old, new in MUTANTS:
        assert original.count(old) == 1, (name, original.count(old))
        open(path, "w", encoding="utf-8").write(original.replace(old, new))
        run = subprocess.run(
            ["timeout", "1200", "cargo", "test", "--locked", "--lib", "--", "tui::full_transcript"],
            cwd=clone, env=env, capture_output=True, text=True,
        )
        text = run.stdout + run.stderr
        failing = sorted({l.split()[1] for l in text.splitlines() if l.startswith("test ") and l.endswith("FAILED")})
        result = [l for l in text.splitlines() if l.startswith("test result")]
        killed = run.returncode != 0 and bool(failing)
        failed_all &= killed
        lines.append("%s %s" % ("KILLED  " if killed else "SURVIVED", name))
        lines.append("    - %s" % old)
        lines.append("    + %s" % new)
        lines += ["    fails %s" % f for f in failing] or ["    exit %d, no test failed" % run.returncode]
        lines += ["    %s" % r for r in result[-1:]]
        print("\n".join(lines[-(4 + max(1, len(failing)) + len(result[-1:])):]), flush=True)
finally:
    open(path, "w", encoding="utf-8").write(original)
lines.append("all killed" if failed_all else "SOME SURVIVED")
open(out, "w", encoding="utf-8").write("\n".join(lines) + "\n")
print(lines[-1])
sys.exit(0 if failed_all else 1)
