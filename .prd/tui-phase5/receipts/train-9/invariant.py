#!/usr/bin/env python3
"""Train 9, join check 6: PASTE-AFTER-END's security invariant at the head.
`src/tui/input.rs`'s `Decoder::feed` -- whose first statement is the early
return for a byte behind a paste's end marker -- read from the unit's tip and
from the head, byte for byte: the function's text, its sha256, and the
whole file's; then the early return's four lines found once in each.

    python3 -I -B invariant.py <repo> <unit-tip> <head>
"""
import hashlib
import subprocess
import sys

PATH = "src/tui/input.rs"
START = "    pub(crate) fn feed(&mut self, byte: u8, now: Instant, out: &mut Vec<Input>) {\n"
EARLY = (
    "        if let Some(behind) = self.behind.as_mut() {\n"
    "            *behind = behind.saturating_add(1);\n"
    "            return;\n"
    "        }\n"
)


def show(repo, commit):
    return subprocess.run(["git", "-C", repo, "show", f"{commit}:{PATH}"], check=True,
                          capture_output=True).stdout.decode("utf-8")


def function(text):
    i = text.index(START)
    j = text.index("\n    }\n", i) + len("\n    }\n")
    return text[i:j]


def main():
    repo, unit, head = sys.argv[1:4]
    texts = {name: show(repo, commit) for name, commit in (("unit", unit), ("head", head))}
    funcs = {name: function(text) for name, text in texts.items()}
    for name, commit in (("unit", unit), ("head", head)):
        f = funcs[name]
        line = texts[name][: texts[name].index(START)].count("\n") + 1
        print(f"{name} {commit}: {PATH}:{line}-{line + f.count(chr(10)) - 1} `feed`, "
              f"{len(f.encode())} bytes, sha256 {hashlib.sha256(f.encode()).hexdigest()}; "
              f"the early return found {f.count(EARLY)} time(s), "
              f"before the paste's own branch and any decoding: {f.index(EARLY) < f.index('if self.pasting')}; "
              f"the whole file sha256 {hashlib.sha256(texts[name].encode()).hexdigest()}")
    same_fn = funcs["unit"] == funcs["head"]
    same_file = texts["unit"] == texts["head"]
    print(f"`feed` byte for byte the same: {same_fn}; {PATH} byte for byte the same: {same_file}")
    print("the function at the head:")
    print(funcs["head"].rstrip("\n"))
    sys.exit(0 if same_fn else 1)


main()
