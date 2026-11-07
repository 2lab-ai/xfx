"""DIFF-WRAPc's mutants, of `walked` (C1-C3 the mark after a tab, W1-W7 DIFF-WRAPb's): each a one-place change to `src/tui/full_transcript.rs`
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
    ("C1 a mark after a tab kept as a cell of its own",
     "            if !after_tab {\n",
     "            if true {\n"),
    ("C2 no tab ever counted as the last cluster",
     "            column = stop;\n            after_tab = true;",
     "            column = stop;\n            after_tab = false;"),
    ("C3 DIFF-WRAPb's join: a mark after a tab joined to the cell before it",
     "            if !after_tab {\n                cells.push(WalkedCell {",
     "            if let Some(last) = cells.last_mut().filter(|last| last.styled) {\n                last.text.push_str(cluster);\n            } else if !after_tab {\n                cells.push(WalkedCell {"),
    ("W1 the walker's column kept past a row it runs on",
     "            rows.push(std::mem::take(&mut cells));\n            column = 1;",
     "            rows.push(std::mem::take(&mut cells));\n            column = column.max(1);"),
    ("W2 a tab stop past the last column",
     "                    .saturating_add(1)\n                    .min(cols)",
     "                    .saturating_add(1)\n                    .min(u16::MAX)"),
    ("W3 a tab on a full row moves nothing (DIFF-WRAP's departure 3)",
     "            let stop = if column >= cols {\n                cols\n",
     "            let stop = if column >= cols {\n                column\n"),
    ("W4 nothing written over the last cell",
     "        while end >= column {",
     "        while end > column {"),
    ("W5 no blank for a wide character's left cell",
     "            cells.extend((end + 1..column).map(|_| WalkedCell::blank()));",
     "            cells.extend((end + 1..end + 1).map(|_| WalkedCell::blank()));"),
    ("W6 a tab's cells styled",
     "            cells: 1,\n            styled: false,",
     "            cells: 1,\n            styled: true,"),
    ("W7 a character that reaches the last column run on",
     "        if u32::from(column) + u32::from(width) - 1 > u32::from(cols) {",
     "        if u32::from(column) + u32::from(width) - 1 >= u32::from(cols) {"),
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
