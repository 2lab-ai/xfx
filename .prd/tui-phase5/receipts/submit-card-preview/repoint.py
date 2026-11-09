#!/usr/bin/env python3
"""SUBMIT-CARD-PREVIEW's re-point: every `src/tui/<file>.rs:N[-M]` reference
that cites a file this unit changed, in the files below as the checkout holds
them.

    python3 -I -B repoint.py <repo> <base> <head>          # check
    python3 -I -B repoint.py <repo> <base> <head> --write  # re-point, once

`--write` reads each reference as a number at <base>, maps it through the
diff's hunks to <head> and writes the new number in, when the cited lines'
text is the same at both. The check reads each reference as a number at
<head>, maps it back to <base> the same way, and holds the cited lines' text
at <head> to the text the reference was written about at <base>: a moved
reference is printed with its two numbers.
"""
import re
import subprocess
import sys

repo, base, head = sys.argv[1:4]
write = "--write" in sys.argv
CITING = ["scripts/fxcap/parity-ledger.toml", "scripts/fxcap/capfile.py", "tests/tui.rs", "docs/parity.md"]
CHANGED = ["src/tui/shell.rs", "src/tui/event_loop.rs", "src/tui/layout.rs", "src/tui/transcript.rs"]
REF = re.compile(r"(src/tui/(?:shell|event_loop|layout|transcript)\.rs):(\d+)(?:-(\d+))?")


def git(*args):
    return subprocess.run(["git", "-C", repo] + list(args), capture_output=True, text=True, check=True).stdout


def hunks(path):
    found = []
    for line in git("diff", "-U0", base, head, "--", path).split("\n"):
        m = re.match(r"@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@", line)
        if m:
            found.append(tuple(int(x) if x is not None else 1 for x in m.groups()))
    return found


def mapper(found, forward):
    """A line kept by the diff, from one side to the other; None for a changed one."""
    def go(line):
        shift = 0
        for o, on, n, nn in found:
            start, count, other = (o, on, nn) if forward else (n, nn, on)
            if count == 0:
                if line > start:
                    shift += other
                continue
            if line < start:
                break
            if line < start + count:
                return None
            shift += other - count
        return line + shift
    return go


maps = {p: (mapper(hunks(p), True), mapper(hunks(p), False)) for p in CHANGED}
text = {(p, r): git("show", "%s:%s" % (r, p)).split("\n") for p in CHANGED for r in (base, head)}
bad = moved = 0
for citing in CITING:
    with open("%s/%s" % (repo, citing)) as f:
        body = f.read()
    out, pos = [], 0
    for m in REF.finditer(body):
        path, a = m.group(1), int(m.group(2))
        b = int(m.group(3)) if m.group(3) else a
        if write:
            (oa, ob), (na, nb) = (a, b), (maps[path][0](a), maps[path][0](b))
        else:
            (na, nb), (oa, ob) = (a, b), (maps[path][1](a), maps[path][1](b))
        same = None not in (oa, ob, na, nb) and text[(path, base)][oa - 1:ob] == text[(path, head)][na - 1:nb]
        if not same:
            bad += 1
        if (oa, ob) != (na, nb) or not same:
            moved += 1
            print("%-8s %s: %s:%s at %s is :%s at %s  %r" % (
                "same" if same else "CHANGED", citing, path, "%s%s" % (oa, "-%s" % ob if m.group(3) else ""), base,
                "%s%s" % (na, "-%s" % nb if m.group(3) else ""), head,
                text[(path, head if na else base)][(na or oa) - 1].strip()[:60]))
        out.append(body[pos:m.start()])
        out.append("%s:%d%s" % (path, na, "-%d" % nb if m.group(3) else "") if (write and same) else m.group(0))
        pos = m.end()
    out.append(body[pos:])
    if write:
        with open("%s/%s" % (repo, citing), "w") as f:
            f.write("".join(out))
print("%d references moved, %d whose cited text changed" % (moved, bad))
sys.exit(1 if bad else 0)
