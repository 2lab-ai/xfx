"""LOCAL-PATHS: writes the harness's scratch root out of every tracked file,
once, and says what it wrote where.

usage: python3 -I .prd/tui-phase5/receipts/local-paths/scrub.py [--apply] <manifest.tsv>

Without --apply it changes nothing and writes the manifest of what it would
change. It matches the shape of a harness root -- `/tmp/claude-<digits>`,
with or without `/private` in front, ending whole -- never one machine's
own; this file names no machine for the reason scripts/check-no-local-paths.sh
gives. What precedes a root is not read: in a capture's .ansi a coloured path
follows its SGR sequence's `m`.

Two ways of writing a root, as the file's own sentences need:

- A sentence that names the root as a thing -- "the run directory
  `<root>/fxcap.XXXXXX` is written `<scratch>`", a history of what a scan
  found -- keeps its meaning with the root as its generic shape,
  `/private/tmp/claude-<uid>`. Those lines are `DESCRIBED`, each by its file
  and a phrase it holds, and must hold a root.
- Everywhere else (a capture's rows, a request body, a command's `TMPDIR=`,
  a path a README quotes) the placeholders capture.sh writes since
  LOCAL-PATHS, the longest first: `<root>/.../fxcap.XXXXXX/work` is `<work>`,
  `<root>/.../fxcap.XXXXXX` is `<scratch>`, and any other `<root>` is
  `<tmp>`.

A line marked `local-path-ok` is a fixture that keeps its path on purpose and
is left as it is. The manifest has one row per changed file: its path, then
how many roots were written each way (described, work, scratch, tmp).
"""

import os
import re
import subprocess
import sys

EDGE = rb"A-Za-z0-9._~+-"
ROOT = rb"(?P<private>/private)?/tmp/claude-[0-9]+(?![0-9])"
RUN = rb"(?:/[A-Za-z0-9._+-]+)*?/fxcap\.[A-Za-z0-9]{6}"
RULES = (
    ("work", re.compile(ROOT + RUN + rb"/work(?![" + EDGE + rb"])"), b"<work>"),
    ("scratch", re.compile(ROOT + RUN + rb"(?![" + EDGE + rb"])"), b"<scratch>"),
    ("tmp", re.compile(ROOT), b"<tmp>"),
)
GENERIC = rb"\g<private>/tmp/claude-<uid>"
MARK = b"local-path-ok"

# (file, a phrase of the line): the sentences that name the root as a thing.
DESCRIBED = (
    (".prd/tui-phase4/loop.md", "harness scratch paths ("),
    (".prd/tui-phase4/loop.done.md", "capture scratch paths"),
    (".prd/tui-phase4/receipts/d1/README.md", "temporary directory ("),
    (".prd/tui-phase4/receipts/d4b/README.md", "Sanitized: the run directory"),
    (".prd/tui-phase4/receipts/r1/README.md", "Sanitized: every run directory"),
    (".prd/tui-phase4/receipts/s1/README.md", "the same length, so no row moves"),
    (".prd/tui-phase4/receipts/t5/README.md", "**Sanitized**: the run directory"),
    (".prd/tui-phase4/receipts/w2/README.md", "**Sanitized**: the run directory"),
    (".prd/tui-phase4/receipts/w3/README.md", "Sanitized: the run directory"),
    (".prd/tui-phase4/receipts/w5/README.md", "Sanitized: the run directory"),
    (".prd/tui-phase4/receipts/w6/README.md", "The run directory ("),
    (".prd/tui-phase5/receipts/fork-one-surface/README.md", "is `<tmp>`."),
    (".prd/tui-phase5/receipts/fork-status-gap/README.md", "is `<tmp>`. `.ansi`"),
    (".prd/tui-phase5/receipts/info-panel/README.md", "Sanitized:"),
    (".prd/tui-phase5/receipts/permissions-early/README.md", "is written `<tmp>`"),
    (".prd/tui-phase5/receipts/permissions-panel/README.md", "the run's tmp names differ"),
    (".prd/tui-phase5/receipts/status-panel/README.md", "Sanitized:"),
)


def git_env():
    return {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}


def tracked(root):
    out = subprocess.run(["git", "-C", root, "ls-files", "-z"], capture_output=True, check=True, env=git_env()).stdout
    return [p.decode() for p in out.split(b"\0") if p]


def scrub(path, data, described):
    counts = {"described": 0, "work": 0, "scratch": 0, "tmp": 0}
    out = []
    for line in data.splitlines(keepends=True):
        if MARK in line:
            out.append(line)
            continue
        if any(phrase in line for phrase in described):
            line, n = re.subn(ROOT, GENERIC, line)
            if not n:
                raise SystemExit("%s: a described line holds no root" % path)
            counts["described"] += n
        for name, rule, placeholder in RULES:
            line, n = rule.subn(placeholder, line)
            counts[name] += n
        out.append(line)
    return b"".join(out), counts


def main():
    args = sys.argv[1:]
    apply = "--apply" in args
    args = [a for a in args if a != "--apply"]
    if len(args) != 1:
        sys.exit(__doc__)
    manifest = args[0]
    root = os.path.realpath(os.path.join(os.path.dirname(__file__), "..", "..", "..", ".."))
    described = {}
    for f, phrase in DESCRIBED:
        described.setdefault(f, []).append(phrase.encode())
    rows, totals, seen = [], {"described": 0, "work": 0, "scratch": 0, "tmp": 0}, set()
    for rel in tracked(root):
        path = os.path.join(root, rel)
        if os.path.islink(path) or not os.path.isfile(path):
            continue
        with open(path, "rb") as f:
            data = f.read()
        new, counts = scrub(rel, data, described.get(rel, []))
        if counts["described"]:
            seen.add(rel)
        if new == data:
            continue
        rows.append((rel, counts))
        for k in totals:
            totals[k] += counts[k]
        if apply:
            with open(path, "wb") as f:
                f.write(new)
    missing = sorted(set(described) - seen)
    if missing:
        sys.exit("described lines not found: %s" % ", ".join(missing))
    with open(manifest, "w", encoding="utf-8") as f:
        f.write("file\tdescribed\twork\tscratch\ttmp\n")
        for rel, c in rows:
            f.write("%s\t%d\t%d\t%d\t%d\n" % (rel, c["described"], c["work"], c["scratch"], c["tmp"]))
        f.write("TOTAL %d files\t%d\t%d\t%d\t%d\n" % ((len(rows),) + tuple(totals[k] for k in ("described", "work", "scratch", "tmp"))))
    print("%s %d files: %s" % ("rewrote" if apply else "would rewrite", len(rows), totals))


if __name__ == "__main__":
    main()
