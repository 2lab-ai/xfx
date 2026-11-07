"""Copies CALLROW-SAFE's captures into the receipt, redacted, after checking
each xfx capture's banner names the build it must, and writes pairs.txt from
the sweeps' classification.

usage: python3 -I -B publish.py <repo> <scratch> <receipt-dir> <green-sha7> <red-sha7>

<scratch> holds sweep-green-<size>/ and sweep-red-<size>/ (scripts/fxcap/sweep.sh
--out), each re-scored with the committed ledger (--rescore).
"""
import glob
import json
import os
import re
import shutil
import sys

repo, scratch, out = (os.path.realpath(p) for p in sys.argv[1:4])
green_sha, red_sha = sys.argv[4:6]
sys.dont_write_bytecode = True
sys.path.insert(0, os.path.join(repo, "scripts", "fxcap"))
from redact import Redactor  # noqa: E402

red = Redactor({repo: "<repo>", scratch: "<scratch>"})
SIZES = ["40x120", "40x60"]
SCENARIOS = ["p5-call-row-safe", "p5-transcript-safe"]
BANNER = re.compile(r"^x𝒇x v0\.1\.0-([0-9a-f]{7}) \[debug\]")
SHOWN = re.compile(r"[\x00-\x08\x0b-\x1f\x7f-\x9f\u061c\u200b-\u200f\u2028-\u202e\u2060-\u206f\ufeff]")


def named(text):
    return SHOWN.sub(lambda m: "<U+%04X>" % ord(m.group(0)), text)


for name in ("fx", "xfx", "red"):
    shutil.rmtree(os.path.join(out, name), ignore_errors=True)
n, seen = 0, {}
for size in SIZES:
    for build, sides in (("green", {"fx": ("fx", None), "xfx": ("xfx", green_sha)}),
                         ("red", {"xfx": ("red", red_sha)})):
        for scenario in SCENARIOS:
            for side, (name, sha) in sides.items():
                run = os.path.join(scratch, "sweep-%s-%s" % (build, size), "runs", scenario, side)
                files = sorted(glob.glob(os.path.join(run, "*.txt")) + glob.glob(os.path.join(run, "*.ansi")))
                assert files, run
                for path in files:
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
assert len(seen) == 2 * len(SCENARIOS) * len(SIZES), seen
print("published", n, "files")

lines = ["# Each snap pair, classified by scripts/fxcap/sweep.sh --rescore with the ledger as committed.",
         "# A character a terminal would act on or that reorders text is shown here as <U+XXXX>; the",
         "# captures under fx/, red/ and xfx/ hold it as the screen did. Each differing row is listed with the",
         "# ledger entries whose selectors match its pair (`-`: none); the `rows no cause or divergence",
         "# explains` line is the classification's own, side by side.", ""]
for build, sha in (("red", red_sha), ("green", green_sha)):
    for size in SIZES:
        pairs = json.load(open(os.path.join(scratch, "sweep-%s-%s" % (build, size), "pairs.json")))
        lines.append("== xfx %s (%s) against fx v0.0.16, %s" % ("base" if build == "red" else "fix", sha, size))
        for p in pairs:
            lines.append("%-18s %-6s %-20s causes=%s" % (p["scenario"], p["label"], p["verdict"],
                                                           ",".join(p.get("causes") or []) or "-"))
            un = p.get("unattributed") or []
            lines.append("    rows no cause or divergence explains: %s" % (
                ", ".join("%s %d" % (s, r) for s, r, _ in un) or "none"))
            for r in p.get("rows", []):
                for side in ("fx", "xfx"):
                    if r.get(side) is not None:
                        lines.append("    %-3s %2d %-22s %s" % (side, r["row"], ",".join(r.get("matched") or []) or "-",
                                                              named(r[side])))
        lines.append("")
red.write(os.path.join(out, "pairs.txt"), "\n".join(lines))
print("wrote pairs.txt")
