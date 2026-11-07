#!/usr/bin/env python3
"""T-U4's mutants, each on a throwaway `git clone --no-local` of the code
commit, never on the worktree: one textual change, the TUI's lib tests
(`cargo test --locked --lib tui::`), the failing tests counted, the change
undone.

    python3 -I -B mutants.py <clone> <out.txt>

A mutant is killed when at least one test fails. Each reverts or half-reverts
one of the unit's rules: the three writers of the cancelled mark one at a
time and the mark itself; the plural table's entries and its count rule;
the argument bounds (a file change's path cut at 4 096 bytes, every other
argument but a command whole); and a live cancel's line on Ctrl-O and in
/trace's timeline.
"""
import os
import re
import subprocess
import sys

MUTANTS = [
    ("the mark itself back to U+25A0", "src/tui/theme.rs",
     'pub(crate) const CANCELLED_MARK: &str = "\\u{25aa}";',
     'pub(crate) const CANCELLED_MARK: &str = "\\u{25a0}";'),
    ("a refused question batch's row writes U+25A0", "src/tui/question.rs",
     '"{}{CANCELLED_MARK}{} {CANCELLED}",',
     '"{}\\u{25a0}{} {CANCELLED}",'),
    ("a cancelled call's own line writes U+25A0", "src/tui/toolgroup.rs",
     '"{}{CANCELLED_MARK}{reset}{} Cancelled{reset} {base}{target}{reset}{CANCELLED_FOLLOW_UP}",',
     '"{}\\u{25a0}{reset}{} Cancelled{reset} {base}{target}{reset}{CANCELLED_FOLLOW_UP}",'),
    ("a cancelled turn's line writes U+25A0", "src/tui/shell.rs",
     '"{}{CANCELLED_MARK}{} {}Cancelled{}{CANCELLED_FOLLOW_UP}",',
     '"{}\\u{25a0}{} {}Cancelled{}{CANCELLED_FOLLOW_UP}",'),
    ("the table's `subagent` keeps its singular", "src/tui/toolgroup.rs",
     '        ("subagent", "subagents"),\n        (MCP_CALL, "MCP calls"),\n',
     '        ("subagent", "subagent"),\n        (MCP_CALL, "MCP calls"),\n'),
    ("the table's `MCP call` keeps its singular", "src/tui/toolgroup.rs",
     '        (MCP_CALL, "MCP calls"),\n    ];',
     '        (MCP_CALL, MCP_CALL),\n    ];'),
    ("the table's `command` keeps its singular", "src/tui/toolgroup.rs",
     '    const PLURALS: [(&str, &str); 3] = [\n        ("command", "commands"),',
     '    const PLURALS: [(&str, &str); 3] = [\n        ("command", "command"),'),
    ("a count of 1 takes the plural too", "src/tui/toolgroup.rs",
     '    if count == 1 {\n        return label;\n    }\n', ''),
    ("every label takes an `s` past 1 (the old rule widened)", "src/tui/toolgroup.rs",
     '        .map_or(label, |(_, plural)| plural)',
     '        .map_or("reads", |(_, plural)| plural)'),
    ("every argument but a command at a file change's bound (the base's rule)", "src/tui/toolgroup.rs",
     '    let mut spelling = Spelling::new(if file_change {\n        TARGET_BYTES\n    } else {\n        usize::MAX\n    });',
     '    let mut spelling = Spelling::new(TARGET_BYTES);\n    let _ = file_change;'),
    ("a file change's path unbounded", "src/tui/toolgroup.rs",
     '    let mut spelling = Spelling::new(if file_change {\n        TARGET_BYTES\n    } else {',
     '    let mut spelling = Spelling::new(if file_change {\n        usize::MAX\n    } else {'),
    ("edit_file's path unbounded (write_file's alone kept)", "src/tui/toolgroup.rs",
     '    let file_change = [mutate::WRITE_FILE, mutate::EDIT_FILE]',
     '    let file_change = [mutate::WRITE_FILE, mutate::WRITE_FILE]'),
    ("a live cancel's line is not recorded for Ctrl-O (the base's rule)", "src/tui/shell.rs",
     '        self.full.block(&[line]);\n', ''),
    ("/trace names a block `questions` again", "src/tui/full_transcript.rs",
     '                        _ => "raw",', '                        _ => "questions",'),
]


def run(clone, env):
    proc = subprocess.run(
        ["cargo", "test", "--locked", "-j", "4", "--lib", "tui::", "--no-fail-fast"],
        cwd=clone, env=env, capture_output=True, text=True)
    out = proc.stdout + proc.stderr
    failed = sorted(set(re.findall(r"^test (\S+) \.\.\. FAILED$", out, re.M)))
    result = re.findall(r"^test result: .*$", out, re.M)
    compile_error = "error[E" in out or "could not compile" in out
    return failed, result, compile_error


def main():
    clone, out_path = sys.argv[1], sys.argv[2]
    env = dict(os.environ)
    lines = []
    for name, path, old, new in MUTANTS:
        full = os.path.join(clone, path)
        text = open(full, encoding="utf-8").read()
        if text.count(old) != 1:
            lines.append("NOT APPLIED %s: the text is not there once (%d)" % (name, text.count(old)))
            continue
        open(full, "w", encoding="utf-8").write(text.replace(old, new))
        try:
            failed, result, compile_error = run(clone, env)
        finally:
            open(full, "w", encoding="utf-8").write(text)
        verdict = "KILLED" if failed or compile_error else "SURVIVED"
        lines.append("%s %s (%s): %d failed%s" % (
            verdict, name, path, len(failed), " (did not compile)" if compile_error else ""))
        lines.extend("    " + t for t in failed)
        lines.extend("    " + r for r in result)
        print(lines[-1 - len(failed) - len(result)], flush=True)
    clean = subprocess.run(["git", "-C", clone, "status", "--porcelain"], capture_output=True, text=True).stdout
    lines.append("clone after: %s" % ("clean" if not clean.strip() else clean))
    open(out_path, "w", encoding="utf-8").write("\n".join(lines) + "\n")


if __name__ == "__main__":
    main()
