"""Copies CALLROW-SAFE round 2's captures into the receipt, redacted, after
checking each xfx capture's banner names the build it must; writes pairs.txt
from the sweeps' classification and reasons.txt from the old/new reason sweep.

usage: python3 -I -B publish.py <repo> <scratch> <receipt-dir> <green-sha7> <red-sha7> <old-sha7>

<scratch> holds the round's sweeps (scripts/fxcap/sweep.sh --out), each
re-scored with the committed ledger (--rescore): sweep-green-40x120,
sweep-green-40x60, sweep-red-40x120, reasons-old, reasons-new.
"""
import glob
import json
import os
import re
import shutil
import sys

repo, scratch, out = (os.path.realpath(p) for p in sys.argv[1:4])
green_sha, red_sha, old_sha = sys.argv[4:7]
sys.dont_write_bytecode = True
sys.path.insert(0, os.path.join(repo, "scripts", "fxcap"))
from redact import Redactor  # noqa: E402

red = Redactor({repo: "<repo>", scratch: "<scratch>"})
BANNER = re.compile(r"^x\U0001d487x v0\.1\.0-([0-9a-f]{7}) \[debug\]")
SHOWN = re.compile(r"[\x00-\x08\x0b-\x1f\x7f-\x9f\u061c\u200b-\u200f\u2028-\u202e\u2060-\u206f\ufeff]")
# (sweep, side in it, where it goes, the banner it must carry)
COPIES = [
    ("sweep-green-40x120", "fx", "fx", None),
    ("sweep-green-40x120", "xfx", "xfx", green_sha),
    ("sweep-green-40x60", "fx", "fx", None),
    ("sweep-green-40x60", "xfx", "xfx", green_sha),
    ("sweep-red-40x120", "xfx", "red", red_sha),
]


# What differs from run to run on any build: clock times, latencies, the
# run directory's random suffix, session ids.
VOLATILE = [
    (re.compile(r"\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}(?:\.\d+)?"), "<time>"),
    (re.compile(r"\b\d+(?:\.\d+)?m?s\b"), "<dur>"),
    (re.compile(r"(?<=fxcap\.)[A-Za-z0-9]{1,6}"), "X"),
    (re.compile(r"\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b"), "<id>"),
]


def mask(row):
    for rx, to in VOLATILE:
        row = rx.sub(to, row)
    return row


def named(text):
    return SHOWN.sub(lambda m: "<U+%04X>" % ord(m.group(0)), text)


for name in ("fx", "xfx", "red"):
    shutil.rmtree(os.path.join(out, name), ignore_errors=True)
n, seen = 0, {}
for sweep, side, name, sha in COPIES:
    size = sweep.rsplit("-", 1)[1]
    for run in sorted(glob.glob(os.path.join(scratch, sweep, "runs", "*", side))):
        scenario = os.path.basename(os.path.dirname(run))
        for path in sorted(glob.glob(os.path.join(run, "*.txt")) + glob.glob(os.path.join(run, "*.ansi"))):
            text = open(path, encoding="utf-8").read()
            if sha and path.endswith(".txt"):
                m = BANNER.match(text.split("\n", 1)[0])
                if m:
                    assert m.group(1) == sha, (path, sha)
                    seen[(name, scenario, size)] = seen.get((name, scenario, size), 0) + 1
            dest = os.path.join(out, name, "%s-%s" % (scenario, size))
            os.makedirs(dest, exist_ok=True)
            red.write(os.path.join(dest, os.path.basename(path)), text)
            n += 1
for (name, scenario, size), count in sorted(seen.items()):
    print("%-4s %-20s %-7s %d snaps' row 1 is the banner of %s" % (
        name, scenario, size, count, green_sha if name == "xfx" else red_sha))
print("published", n, "files")


def pair_lines(sweep, title):
    pairs = json.load(open(os.path.join(scratch, sweep, "pairs.json")))
    lines = ["== " + title]
    for p in pairs:
        lines.append("%-20s %-10s %-20s causes=%s" % (p["scenario"], p["label"], p["verdict"],
                                                      ",".join(p.get("causes") or []) or "-"))
        un = p.get("unattributed") or []
        lines.append("    rows no cause or divergence explains: %s" % (
            ", ".join("%s %d" % (s, r) for s, r, _ in un) or "none"))
        for r in p.get("rows", []):
            for side in ("fx", "xfx"):
                if r.get(side) is not None:
                    lines.append("    %-3s %2d %-22s %s" % (side, r["row"], ",".join(r.get("matched") or []) or "-",
                                                          named(r[side])))
    return lines + [""]


head = ["# Each snap pair, classified by scripts/fxcap/sweep.sh --rescore with the ledger as committed.",
        "# A character a terminal would act on or that reorders text is shown here as <U+XXXX>; the",
        "# captures under fx/, red/ and xfx/ hold it as the screen did. Each differing row is listed with the",
        "# ledger entries whose selectors match its pair (`-`: none); the `rows no cause or divergence",
        "# explains` line is the classification's own, side by side.", ""]
lines = list(head)
lines += pair_lines("sweep-red-40x120", "xfx base of round 2 (%s) against fx v0.0.16, 40x120" % red_sha)
lines += pair_lines("sweep-green-40x120", "xfx fix (%s) against fx v0.0.16, 40x120" % green_sha)
lines += pair_lines("sweep-green-40x60", "xfx fix (%s) against fx v0.0.16, 40x60" % green_sha)
red.write(os.path.join(out, "pairs.txt"), "\n".join(lines))

# The reason sweep: every scenario both builds ran, its verdict on each, and
# every row whose text differs between the two builds' xfx captures.
old = {(p["scenario"], p["label"]): p for p in json.load(open(os.path.join(scratch, "reasons-old", "pairs.json")))}
new = {(p["scenario"], p["label"]): p for p in json.load(open(os.path.join(scratch, "reasons-new", "pairs.json")))}
lines = ["# p5-transcript-*, p5-tool*, p5-mcp-* and *fail* at 40x120: the base %s and the fix %s, each" % (old_sha, green_sha),
         "# against fx v0.0.16 under the committed ledger. Per snap: both verdicts and the rows no cause or",
         "# divergence explains on each; then every row whose text differs between the two xfx captures.", ""]
for key in sorted(set(old) | set(new)):
    o, w = old.get(key), new.get(key)
    def short(p):
        if p is None:
            return "absent"
        un = p.get("unattributed") or []
        return "%s [%s]" % (p["verdict"], ", ".join("%s %d" % (s, r) for s, r, _ in un) or "none")
    lines.append("%-26s %-12s old %s | new %s" % (key[0], key[1], short(o), short(w)))
lines.append("")
for scenario in sorted({k[0] for k in new}):
    for path in sorted(glob.glob(os.path.join(scratch, "reasons-new", "runs", scenario, "xfx", "*.txt"))):
        base = os.path.basename(path)
        other = os.path.join(scratch, "reasons-old", "runs", scenario, "xfx", base)
        if not os.path.exists(other):
            lines.append("%s: no old capture" % base)
            continue
        a = [mask(row) for row in red(open(other, encoding="utf-8").read()).split("\n")]
        b = [mask(row) for row in red(open(path, encoding="utf-8").read()).split("\n")]
        rows = [i for i in range(max(len(a), len(b)))
                if (a[i] if i < len(a) else None) != (b[i] if i < len(b) else None)
                and not (i == 0 and BANNER.match(a[0] if a else "") and BANNER.match(b[0] if b else ""))]
        if not rows:
            continue
        lines.append("-- %s: rows %s" % (base, ", ".join(str(i + 1) for i in rows)))
        for i in rows[:12]:
            lines.append("   old %3d %s" % (i + 1, named(a[i] if i < len(a) else "(none)")))
            lines.append("   new %3d %s" % (i + 1, named(b[i] if i < len(b) else "(none)")))
red.write(os.path.join(out, "reasons.txt"), "\n".join(lines))
print("wrote pairs.txt, reasons.txt")
