"""Copies a local log into this receipt with this machine's paths named.

usage: python3 -I redacted.py <src> <dest> [<path>=<name>]...

Each <path> given is written as its <name> (longest first); then
scripts/fxcap/redact.py names any other local path `<local-path>` and refuses
the write if a local root, the home directory or the retired product name
survives. The pairs are this unit's: the scratch directory (`<scratch>`), the
worktree (`<worktree>`), the base clone (`<base>`), the binary copies
(`<bins>`), fx v0.0.16 (`<fx>`) and the short TMPDIR (`<tmp>`).
"""

import os
import sys

sys.dont_write_bytecode = True
HERE = os.path.dirname(os.path.realpath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "..", "..", "..", "scripts", "fxcap"))
from redact import Redactor  # noqa: E402


def main(argv):
    src, dest = argv[1], argv[2]
    known = dict(pair.split("=", 1) for pair in argv[3:])
    with open(src, encoding="utf-8", errors="replace") as f:
        text = f.read()
    Redactor(known).write(dest, text)


if __name__ == "__main__":
    main(sys.argv)
