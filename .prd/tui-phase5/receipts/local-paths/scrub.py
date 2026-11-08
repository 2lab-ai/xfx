"""LOCAL-PATHS: writes the harness's scratch root out of every tracked file,
once, and says what it wrote where.

usage: python3 -I .prd/tui-phase5/receipts/local-paths/scrub.py [--apply] [--round-2] <manifest.tsv>

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

Round 2 (LOCAL-PATHS review) added two passes, run after the line rules:

- **split**: a root a frame broke across rows, or a colour change splits, is
  found in the frame's visible text (`scripts/fxcap/roots.py`'s `View`: control
  sequences set aside, rows joined across their edge gutters) and written as
  capture.sh writes it now, the placeholder where its first character stood;
- **tail**: a quoted row (a comparison report quotes rows `'...'`) that opens
  with the tail of a harness root -- any end of `/private/tmp/claude-<digits>`
  -- running into a run directory is written from its start as `<work>` or
  `<scratch>`.

Their counts are the manifest's `split` and `tail` columns.
"""

import os
import re
import subprocess
import sys

sys.dont_write_bytecode = True
sys.path.insert(0, os.path.join(os.path.dirname(os.path.realpath(__file__)), "..", "..", "..", "..", "scripts", "fxcap"))
import roots  # noqa: E402

EDGE = rb"A-Za-z0-9._~+-"
ROOT = rb"(?P<private>/private)?/tmp/claude-[0-9]+(?![0-9])"
RUN = rb"(?:/[A-Za-z0-9._+-]+)*?/fxcap\.[A-Za-z0-9]{6}"
RULES = (
    ("work", re.compile(ROOT + RUN + rb"/work(?![" + EDGE + rb"])"), b"<work>"),
    ("scratch", re.compile(ROOT + RUN + rb"(?![" + EDGE + rb"])"), b"<scratch>"),
    ("tmp", re.compile(ROOT), b"<tmp>"),
)
GENERIC = rb"\g<private>/tmp/claude-<uid>"
# Every end of a harness root's head, the longest first, for a quoted tail.
HEAD = "/private/tmp/claude-"
TAIL = re.compile(
    rb"(?<=')(?P<lead>\s*)(?:%s)?[0-9]*(?:/[A-Za-z0-9._+-]+)*?/fxcap\.[A-Za-z0-9]{6}(?P<work>/work)?(?![A-Za-z0-9._~+-])"
    % b"|".join(re.escape(HEAD[i:]).encode() for i in range(len(HEAD)))
)
SPLIT = (
    (b"<work>", re.compile(ROOT + RUN + rb"/work")),
    (b"<scratch>", re.compile(ROOT + RUN)),
    (b"<tmp>", re.compile(ROOT)),
)
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


def marked_ranges(data):
    """The byte ranges of the lines marked `local-path-ok`."""
    out, pos = [], 0
    for line in data.splitlines(keepends=True):
        if MARK in line:
            out.append((pos, pos + len(line)))
        pos += len(line)
    return out


def split_pass(data):
    """The roots a row's end or a colour change splits, in the visible text;
    none that touches a marked line."""
    view = roots.View(data)
    marked = marked_ranges(data)
    edits, taken = [], []
    for name, rule in SPLIT:
        for m in rule.finditer(view.text):
            a, b = m.span()
            if not view.ends_whole(b) or any(a < y and x < b for x, y in taken):
                continue
            spans = view.spans(a, b)
            if any(x < my and mx < y for x, y in spans for mx, my in marked):
                continue
            taken.append((a, b))
            edits.append((spans[0][0], spans[0][1], name))
            edits.extend((x, y, b"") for x, y in spans[1:])
    if not edits:
        return data, 0
    out, pos = bytearray(), 0
    for x, y, new in sorted(edits):
        out += data[pos:x] + new
        pos = y
    return bytes(out + data[pos:]), len(taken)


def tail_pass(data):
    out, n = [], 0
    for line in data.splitlines(keepends=True):
        if MARK not in line:
            line, k = TAIL.subn(lambda m: m.group("lead") + (b"<work>" if m.group("work") else b"<scratch>"), line)
            n += k
        out.append(line)
    return b"".join(out), n


def scrub(path, data, described):
    counts = {"described": 0, "work": 0, "scratch": 0, "tmp": 0, "split": 0, "tail": 0}
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
    data = b"".join(out)
    marked = [line for line in data.splitlines() if MARK in line]
    data, counts["split"] = split_pass(data)
    data, counts["tail"] = tail_pass(data)
    if [line for line in data.splitlines() if MARK in line] != marked:
        raise SystemExit("%s: a marked line changed" % path)
    return data, counts


def main():
    args = sys.argv[1:]
    apply = "--apply" in args
    args = [a for a in args if a not in ("--apply", "--round-2")]
    if len(args) != 1:
        sys.exit(__doc__)
    manifest = args[0]
    root = os.path.realpath(os.path.join(os.path.dirname(__file__), "..", "..", "..", ".."))
    described = {}
    for f, phrase in DESCRIBED:
        described.setdefault(f, []).append(phrase.encode())
    rows, totals, seen = [], {"described": 0, "work": 0, "scratch": 0, "tmp": 0, "split": 0, "tail": 0}, set()
    for rel in tracked(root):
        path = os.path.join(root, rel)
        if os.path.islink(path) or not os.path.isfile(path):
            continue
        with open(path, "rb") as f:
            data = f.read()
        # Round 2 reads a tree round 1 already wrote: its described lines hold
        # no root any more.
        new, counts = scrub(rel, data, [] if "--round-2" in sys.argv else described.get(rel, []))
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
    if missing and "--round-2" not in sys.argv:
        sys.exit("described lines not found: %s" % ", ".join(missing))
    with open(manifest, "w", encoding="utf-8") as f:
        keys = ("described", "work", "scratch", "tmp", "split", "tail")
        f.write("file\t" + "\t".join(keys) + "\n")
        for rel, c in rows:
            f.write(rel + "".join("\t%d" % c[k] for k in keys) + "\n")
        f.write("TOTAL %d files" % len(rows) + "".join("\t%d" % totals[k] for k in keys) + "\n")
    print("%s %d files: %s" % ("rewrote" if apply else "would rewrite", len(rows), totals))


if __name__ == "__main__":
    main()
