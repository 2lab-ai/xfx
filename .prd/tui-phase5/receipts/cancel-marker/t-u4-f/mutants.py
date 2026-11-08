#!/usr/bin/env python3
"""T-U4-F's mutants, on a throwaway `git clone --no-local` of the fix,
never on the worktree: one textual change to `unit_at`'s new rule, the
TUI's lib tests (`cargo test --locked --lib tui::`), the failing tests
counted, the change undone. The runner is the unit's own (`../mutants.py`).

    python3 -I -B mutants.py <clone> <out.txt>
"""
import importlib.util
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location("unit_mutants", os.path.join(HERE, "..", "mutants.py"))
unit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(unit)

FIND = ("                .find(|&(at, character)| character == '\\\\' && spelling_at(&rest[at..]).is_some())\n"
        "                .map_or(whole, |(at, _)| at)")

unit.MUTANTS = [
    ("a cluster keeps the spelling it took in (5a7bb465's rule)", "src/tui/toolgroup.rs",
     FIND, "                .find(|_| false)\n                .map_or(whole, |(at, _): (usize, char)| at)"),
    ("a cluster ends before any `\\` it took in, spelling or not", "src/tui/toolgroup.rs",
     FIND, "                .find(|&(_, character)| character == '\\\\')\n                .map_or(whole, |(at, _)| at)"),
    ("a cluster ends before its second character, whatever it is", "src/tui/toolgroup.rs",
     FIND, "                .find(|_| true)\n                .map_or(whole, |(at, _)| at)"),
]

if __name__ == "__main__":
    unit.main()
