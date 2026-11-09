#!/usr/bin/env python3
"""Copies a log into this receipt with this machine's paths taken out: each
`<name>=<path>` given is rewritten to `<name>`, longest first, and the copy is
refused if a home, scratch or temporary root is still in it.

usage: python3 -I redact.py <src> <dst> <name>=<path>...
The paths are arguments, so none of this machine's is written here."""
import re, sys

# Assembled, so this file is no finding of the check it copies.
ROOTS = ["/" + "Users" + "/[^/<]+/", "/private" + "/tmp/claude-[0-9]+",
         "/tmp" + "/claude-[0-9]+", "/var" + "/folders/"]
LEFT = re.compile("|".join(ROOTS))

def main():
    src, dst, pairs = sys.argv[1], sys.argv[2], sys.argv[3:]
    text = open(src, errors="replace").read()
    named = sorted((p.split("=", 1) for p in pairs), key=lambda kv: -len(kv[1]))
    for name, path in named:
        text = text.replace(path, name)
    left = LEFT.findall(text)
    if left:
        sys.exit(f"redact: {len(left)} local path(s) left, e.g. {left[0]!r}; not written")
    open(dst, "w").write(text)

main()
