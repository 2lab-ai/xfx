"""LOCAL-PATHS: the tracked captures holding a zero-filled placeholder
(`<work>`, `<scratch>`, `<tmp>`, `<home>` and the rest, `:real` too, with no
`_`), at a revision and at the base, and what wrote them. Prints counts and
repository paths with line numbers, never a line's text.

usage: python3 -I zero-fill.py <base> <rev>
"""
import collections
import re
import subprocess
import sys

base, rev = sys.argv[1], sys.argv[2]
ZERO = r"<(work|scratch|scenarios|bun|tmp|home)(:real)?>"
# scripts/check-no-local-paths.sh's is_capture: by name, or by kind.
NAME = re.compile(r"(^|/)x?fx-[^/]*-[0-9]{1,4}x[0-9]{1,4}-")
KINDS = (".txt", ".ansi", ".wire", ".json", ".log", ".files", ".clip", ".clipfile", ".title", ".cursor", ".bells",
         ".sockets", ".port")


def git(*args):
    return subprocess.run(["git", "-c", "core.fsmonitor=false"] + list(args), capture_output=True, text=True).stdout


def holding(r):
    out = git("grep", "-l", "-E", ZERO, r, "--", ".")
    return {line.split(":", 1)[1] for line in out.split()}


def is_capture(path):
    return bool(NAME.search(path)) or "-bodies/" in path or path.endswith(KINDS)


now, then = holding(rev), holding(base)
caps = sorted(p for p in now if is_capture(p))
print("tracked captures holding a zero-filled placeholder at %s: %d" % (rev, len(caps)))
print("  of them, holding one at %s already: %d" % (base, sum(1 for p in caps if p in then)))
print("  not at %s (this unit's own receipt text): %s" % (base, ", ".join(p for p in caps if p not in then) or "none"))
for unit in ("w3", "w5", "w9"):
    print("  .prd/tui-phase4/receipts/%s: %d" % (unit, sum(1 for p in caps if p.startswith(".prd/tui-phase4/receipts/%s/" % unit))))
by = collections.Counter(p.split("/")[3] if p.startswith(".prd/") else p.split("/")[0] for p in caps)
print("  the most, by directory: %s" % ", ".join("%s %d" % kv for kv in by.most_common(5)))
print()
print("what wrote them (line numbers, the text left out):")
for r, path, pattern in ((base, "scripts/fxcap/capture.sh", r"<scratch>#g"),
                         (rev, "scripts/fxcap/sweep.py", r'"<tmp>"'),
                         (rev, "scripts/fxcap/ci_sweep.py", r'"<tmp>"')):
    out = git("grep", "-n", "-F", "-e", pattern, r, "--", path)
    lines = [line.split(":")[2] for line in out.splitlines()]
    print("  %s at %s: lines %s" % (path, r, ", ".join(lines) or "none"))
