"""ENGINE-VS-ANCHOR's mutants: each takes one rule out of the code, and the
tests that pin it must fail; and one per claim of `check.py`, on its captures.

usage: python3 -I -B mutants.py code <clone> <out>
       python3 -I -B mutants.py oracle <repo> <scratch>
       python3 -I -B mutants.py check <repo> <receipt-dir> <scratch>

`code` edits files in <clone> -- a throwaway `git clone --no-local` of the
commit under test, never the worktree -- one mutant at a time (the list is
checked against the clone before anything runs), runs the tests
named beside it there (`cargo test --locked --lib`, TMPDIR short), restores
the file, and writes what each run said to <out>. `oracle` does the same to
`scripts/smoke-tui.sh`'s emulator, extracted into <scratch> with its
falsification tests. `check` copies the receipt's captures into <scratch> and
changes one row. Exit 0 only when every mutant is caught.
"""

import os
import re
import shutil
import subprocess
import sys

TESTS = [
    "selector_glyph", "wide_glyph_with", "narrow_glyph_with", "glyph_without_a_selector",
    "erase_after_a_selector", "wide_unit_with", "wide_unit_without", "the_whole_surface_puts",
    "full_transcripts_read_row", "document_row_is_written", "band_row_ending", "whole_band_repaint_and",
    "cleared_and_reanchored", "anchor_inside_a_link", "autowrap_bracket", "anchor_off_the_screen",
    "clear_past_the_margin", "clear_takes_the_whole",
    # Review round 1.
    "split_off", "split_cluster", "painters_own_shape", "split_off_its_glyph",
]

CODE = [
    ("no anchor: put_unit never writes CHA", "src/tui/grid.rs",
     "    if disputed && more && after < cols {", "    if false && disputed && more && after < cols {"),
    ("the anchor one column early", "src/tui/grid.rs",
     'let _ = write!(out, "\\u{1b}[{}G", after + 1);', 'let _ = write!(out, "\\u{1b}[{}G", after);'),
    ("no clear: put_unit never writes ECH 2", "src/tui/grid.rs",
     "    if disputed && width == 2 {", "    if false && disputed && width == 2 {"),
    ("U+FE0F not a selector", "src/tui/grid.rs",
     "suffix.contains(['\\u{fe0e}', '\\u{fe0f}'])", "suffix.contains(['\\u{fe0e}'])"),
    ("no margin guard", "src/tui/grid.rs",
     "let guarded = !suffix.is_empty() && width == 2 && column + width == cols;",
     "let guarded = false && !suffix.is_empty() && width == 2 && column + width == cols;"),
    ("the guard only for a selector, not any suffix", "src/tui/grid.rs",
     "let guarded = !suffix.is_empty() && width == 2 && column + width == cols;",
     "let guarded = disputed && width == 2 && column + width == cols;"),
    ("the diff's EL does not count as following", "src/tui/grid.rs",
     "let more = index + width < stop || erases;", "let more = index + width < stop;"),
    ("a row written whole anchors after its last unit too", "src/tui/grid.rs",
     "let more = last_unit.is_some_and(|last| at < last);",
     "let more = last_unit.is_some();"),
    ("the alternate plane's paint writes the row raw", "src/tui/frame.rs",
     "            write_row(bytes, row, geometry.cols);",
     "            bytes.extend_from_slice(row_text(row, geometry.cols).as_bytes());"),
    ("the band's whole repaint and the return from the other plane write the row raw", "src/tui/frame.rs",
     "    super::grid::write_row_text(buffer, &row_text(row, cols), cols);",
     "    buffer.extend_from_slice(row_text(row, cols).as_bytes());"),
    ("a document row anchored as a frame's row is", "src/tui/frame.rs",
     "    buffer.extend_from_slice(text.as_bytes());\n    if super::wrap::width(&text) < geometry.cols {",
     "    super::grid::write_row_text(buffer, &text, geometry.cols);\n    if super::wrap::width(&text) < geometry.cols {"),
    ("check: CHA refused under a link", "src/tui/check.rs",
     "        (b' ', b'G') => {\n            let Caret::Known((row, _)) = model.caret else {",
     "        (b' ', b'G') if model.link.is_some() => {\n            return Err(Reject::new(\"an erase under an open hyperlink\", at));\n        }\n        (b' ', b'G') => {\n            let Caret::Known((row, _)) = model.caret else {"),
    ("check: any ?7h/?7l pair gives the mode back", "src/tui/check.rs",
     "                Some(bracket) if bracket.painters() => bracket.before,",
     "                Some(bracket) => bracket.before,"),
    ("check: ECH takes no half of a wide cluster", "src/tui/check.rs",
     "        for cell in &mut cells[lead.min(start)..tail] {", "        for cell in &mut cells[start..end] {"),
    ("check: ECH past the last column accepted", "src/tui/check.rs",
     "            if usize::from(column) + usize::from(count) - 1 > usize::from(model.screen_ref().cols) {",
     "            if false {"),
    # Review round 1: the row's order, the split cell, the bracket's shape.
    ("a row written whole moves a joined cluster ahead of its sequence (round 1's head)", "src/tui/grid.rs",
     "                    for piece in span {\n"
     "                        let (Piece::Raw(bytes) | Piece::Unit { cluster: bytes, .. }) = piece;\n"
     "                        out.extend_from_slice(bytes.as_bytes());\n"
     "                    }",
     "                    out.extend_from_slice(grapheme.as_bytes());\n"
     "                    for piece in span {\n"
     "                        if let Piece::Raw(bytes) = piece {\n"
     "                            if super::pacer::escape_at(bytes).is_some() {\n"
     "                                out.extend_from_slice(bytes.as_bytes());\n"
     "                            }\n"
     "                        }\n"
     "                    }"),
    ("a row written whole decides on a cluster joined to a two-cell unit", "src/tui/grid.rs",
     "                grapheme,\n                width: 1,\n                end,\n                ..\n            }) = last_unit",
     "                grapheme,\n                end,\n                ..\n            }) = last_unit"),
    ("the diff writes a split one-cell cell together", "src/tui/grid.rs",
     "    if usize::from(super::wrap::width(grapheme)) == width {",
     "    if true {"),
    ("the diff splits it with nothing between", "src/tui/grid.rs",
     "    out.extend_from_slice(if pen.is_empty() { RESET } else { pen }.as_bytes());\n", ""),
    ("check: the bracket takes a unit of any width", "src/tui/check.rs",
     "bracket.margin_unit = width == 2 && !suffix.is_empty() && column + width - 1 == cols;",
     "bracket.margin_unit = !suffix.is_empty() && column + width - 1 == cols;"),
    ("check: the bracket takes a unit with no suffix", "src/tui/check.rs",
     "bracket.margin_unit = width == 2 && !suffix.is_empty() && column + width - 1 == cols;",
     "bracket.margin_unit = width == 2 && column + width - 1 == cols;"),
    ("check: the bracket takes a clear of any count", "src/tui/check.rs",
     "            (1, 2) => true,", "            (1, _) => true,"),
    ("check: the bracket takes a clear whatever the selector", "src/tui/check.rs",
     "self.units == 1 && self.margin_unit && clear == self.selector && self.before != Some(true)",
     "self.units == 1 && self.margin_unit && self.before != Some(true) && (clear || !clear)"),
    ("check: CHA off the screen accepted", "src/tui/check.rs",
     "            if column == 0 || column > model.screen_ref().cols {\n                return Err(Reject::at_cell(\n                    \"a caret placed off the screen\",\n                    at,\n                    row,\n                    column,\n                ));\n            }\n            model.caret = Caret::Known((row, column));",
     "            model.caret = Caret::Known((row, column.max(1)));"),
]

ORACLE = [
    ("CHA unmodelled", "        if final == \"G\":\n            self._column(params, match)\n            return match.end()\n", ""),
    ("ECH unmodelled", "        if final == \"X\":\n            self._erase_characters(params, match)\n            return match.end()\n", ""),
    ("CHA past the last column accepted", "if not params.isdigit() or not 1 <= int(params) <= self.cols:",
     "if not params.isdigit():"),
    ("ECH keeps half a cluster", "        while start > 0 and self.cells[self.row][start] == \"\":\n            start -= 1\n", ""),
]


def run(cmd, cwd, env=None):
    p = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, env=env)
    return p.returncode, p.stdout + p.stderr


def code(clone, out):
    env = dict(os.environ, TMPDIR=os.path.join(os.environ.get("TMPDIR") or "/tmp", "evsa-m"))
    os.makedirs(env["TMPDIR"], exist_ok=True)
    for name, path, old, _ in CODE:
        count = open(os.path.join(clone, path), encoding="utf-8").read().count(old)
        assert count == 1, (name, count)
    caught = True
    with open(out, "w", encoding="utf-8") as log:
        code_, text = run(["cargo", "test", "--locked", "--lib", "--"] + TESTS, clone, env)
        line = [l for l in text.splitlines() if l.startswith("test result")]
        log.write("# unchanged: %s\n" % (line[-1] if line else text[-400:]))
        caught &= code_ == 0
        for name, path, old, new in CODE:
            file = os.path.join(clone, path)
            source = open(file, encoding="utf-8").read()
            assert source.count(old) == 1, (name, source.count(old))
            open(file, "w", encoding="utf-8").write(source.replace(old, new))
            try:
                code_, text = run(["cargo", "test", "--locked", "--lib", "--"] + TESTS, clone, env)
            finally:
                open(file, "w", encoding="utf-8").write(source)
            failed = [l.split()[1] for l in text.splitlines() if l.startswith("test ") and l.endswith("FAILED")]
            ok = code_ != 0 and bool(failed)
            caught &= ok
            log.write("\n# mutant: %s (%s)\n" % (name, path))
            log.write("caught: %s\n" % ("yes" if ok else "NO"))
            for test in failed:
                log.write("  FAILED %s\n" % test)
            if not failed:
                log.write(text[-600:] + "\n")
    print(open(out, encoding="utf-8").read())
    return caught


def extract(repo, scratch):
    s = open(os.path.join(repo, "scripts", "smoke-tui.sh"), encoding="utf-8").read()
    os.makedirs(scratch, exist_ok=True)
    for name in ("vt_grid.py", "oracle_test.py"):
        m = re.search(r"cat >\"\$helpers/" + re.escape(name) + r"\" <<'PYTHON'\n(.*?)\nPYTHON\n", s, re.S)
        open(os.path.join(scratch, name), "w", encoding="utf-8").write(m.group(1) + "\n")


def oracle_problems(scratch):
    _, text = run([sys.executable, "-I", "-B", os.path.join(scratch, "oracle_test.py")], scratch)
    # The capture-cut test needs the suite's `scenarios` module, which only
    # the whole suite writes; it is no claim of this unit's.
    return [l.strip() for l in text.splitlines() if l.startswith("    ") and "oracle claims checked" not in l
            and "a_wait_and_a_grid_cut" not in l]


def oracle(repo, scratch):
    extract(repo, scratch)
    vt = os.path.join(scratch, "vt_grid.py")
    source = open(vt, encoding="utf-8").read()
    caught = not oracle_problems(scratch)
    print("unchanged: %s" % ("no problem" if caught else oracle_problems(scratch)))
    for name, old, new in ORACLE:
        assert source.count(old) == 1, name
        open(vt, "w", encoding="utf-8").write(source.replace(old, new))
        try:
            problems = oracle_problems(scratch)
        finally:
            open(vt, "w", encoding="utf-8").write(source)
        caught &= bool(problems)
        print("%s mutant: %s" % ("caught" if problems else "NOT CAUGHT", name))
        for problem in problems:
            print("    " + problem)
    return caught


GAPPED = "half\uffa0\ufe0e\x1b[39m   \x1b[38;5;245mx zwj"
CLOSED = "half\uffa0\ufe0ex zwj"
CHECK = [
    # (what, side, snap, which occurrence, old, new, the FAIL line's head)
    ("xfx's read row without the anchor (x three cells left, as red)", "xfx", "full", 0, GAPPED, CLOSED,
     "FAIL anchor"),
    ("fx's diff row without the gap", "fx", "full", 1, GAPPED, CLOSED, "FAIL anchor"),
    ("an edited row changed", "xfx", "edited", 0, "Edited wide2.txt", "Edited wide3.txt", "FAIL inline"),
]


def check(repo, receipt, scratch):
    caught = True
    unit = "p5-transcript-diffwide2-40x120"
    for name, who, snap, which, old, new, fail in CHECK:
        root = os.path.join(scratch, "r")
        shutil.rmtree(root, ignore_errors=True)
        for side in ("fx", "xfx"):
            shutil.copytree(os.path.join(receipt, side), os.path.join(root, side))
        prefix = "fx" if who == "fx" else "xfx"
        file = os.path.join(root, who, unit, "%s-%s-%s.ansi" % (prefix, unit, snap))
        text = open(file, encoding="utf-8").read()
        at = -1
        for _ in range(which + 1):
            at = text.index(old, at + 1)
        open(file, "w", encoding="utf-8").write(text[:at] + new + text[at + len(old):])
        code_, out = run([sys.executable, "-I", "-B", os.path.join(receipt, "check.py"), repo, root, "xfx"], scratch)
        ok = code_ != 0 and any(line.startswith(fail) for line in out.splitlines())
        caught &= ok
        print("%s mutant: %s" % ("caught" if ok else "NOT CAUGHT", name))
        for line in out.splitlines():
            if "FAIL" in line:
                print("    " + line.strip()[:200])
    return caught


def main():
    mode = sys.argv[1]
    if mode == "code":
        ok = code(os.path.realpath(sys.argv[2]), os.path.realpath(sys.argv[3]))
    elif mode == "oracle":
        ok = oracle(os.path.realpath(sys.argv[2]), os.path.realpath(sys.argv[3]))
    else:
        ok = check(*(os.path.realpath(p) for p in sys.argv[2:5]))
    print("every mutant caught" if ok else "A MUTANT SURVIVED")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
