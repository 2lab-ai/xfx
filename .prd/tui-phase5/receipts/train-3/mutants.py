#!/usr/bin/env python3
"""The join test's mutants: each takes one rule out of the path a tool
group's redrawn row is written by, in a throwaway clone, and runs the two
join tests (`a_tool_rows_`) against it.

    python3 -I -B mutants.py <clone of the train head>

Each mutant is one exact replacement in src/tui/grid.rs, put back after its
run. A mutant is caught when at least one of the two tests fails.
"""
import subprocess
import sys

GRID = "src/tui/grid.rs"
MUTANTS = [
    ("A", "the anchor (CHA) after a selector unit not written",
     "    if disputed && more && after < cols {",
     "    if false && disputed && more && after < cols {"),
    ("B", "the margin bracket (?7h/?7l) not written",
     "    let guarded = !suffix.is_empty() && width == 2 && column + width == cols;",
     "    let guarded = false;"),
    ("C", "the clear (ECH 2) before a two-cell selector unit not written",
     "    if disputed && width == 2 {",
     "    if false {"),
    ("D", "a split cell written together",
     "    if usize::from(super::wrap::width(grapheme)) == width {",
     "    if true {"),
    ("E", "the retint ignores a row's drawings (colour matching only)",
     "            if let Some(redraw) = self.redraw(line).cloned() {",
     "            if let Some(redraw) = None::<Redraw> {"),
    ("F", "the diff writes a cell bare, not through put_unit",
     "                        put_unit(out, grapheme, index, width, margin, more, |out| {\n"
     "                            write_cell(out, grapheme, width, pen);\n"
     "                        });",
     "                        let _ = (more, margin);\n"
     "                        write_cell(out, grapheme, width, pen);"),
]


def run(clone):
    done = subprocess.run(
        ["cargo", "test", "--locked", "--lib", "a_tool_rows_"],
        cwd=clone, capture_output=True, text=True)
    text = done.stdout + done.stderr
    result = [line for line in text.splitlines() if line.startswith("test result") or " ... FAILED" in line]
    return done.returncode, result


def main():
    clone = sys.argv[1]
    path = f"{clone}/{GRID}"
    original = open(path).read()
    code, result = run(clone)
    print(f"unmutated: exit {code}; " + "; ".join(result))
    caught = 0
    for name, what, old, new in MUTANTS:
        assert original.count(old) == 1, name
        open(path, "w").write(original.replace(old, new))
        try:
            code, result = run(clone)
        finally:
            open(path, "w").write(original)
        failed = [line.split()[1] for line in result if " ... FAILED" in line]
        caught += code != 0 and bool(failed)
        print(f"{name} ({what}): {'CAUGHT' if failed else 'SURVIVED'} by {failed or result}")
    print(f"{caught} of {len(MUTANTS)} caught")


main()
