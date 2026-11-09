"""The parity ledger's source refs this unit re-pointed, checked by text: each
`src/tui/shell.rs:<n>` the ledger names at <head> holds the line it named at
<base>, where the tests this unit added above it had not yet moved it.

usage: python3 -I refs.py <base> <head>   (run in the repo)
"""

import re
import subprocess
import sys

sys.dont_write_bytecode = True
MOVED = {39046: 39279}


def show(rev, path):
    return subprocess.run(["git", "show", f"{rev}:{path}"], capture_output=True, text=True, check=True).stdout.split("\n")


def main():
    base, head = sys.argv[1], sys.argv[2]
    ledger = "scripts/fxcap/parity-ledger.toml"
    old_refs = re.findall(r"src/tui/shell\.rs:(\d+)", "\n".join(show(base, ledger)))
    new_refs = re.findall(r"src/tui/shell\.rs:(\d+)", "\n".join(show(head, ledger)))
    b, h = show(base, "src/tui/shell.rs"), show(head, "src/tui/shell.rs")
    wrong = 0
    for o, n in zip(old_refs, new_refs):
        o, n = int(o), int(n)
        ok = b[o - 1] == h[n - 1] and MOVED.get(o, o) == n
        wrong += not ok
        print(f"shell.rs:{o} -> :{n}  {'same text' if ok else 'WRONG'}  {h[n - 1].strip()[:70]!r}")
    print(f"refs: {len(new_refs)} (base {len(old_refs)}); wrong: {wrong}")


if __name__ == "__main__":
    main()
