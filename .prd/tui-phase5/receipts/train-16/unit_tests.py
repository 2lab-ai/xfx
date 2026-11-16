#!/usr/bin/env python3
"""Train 16: train 15's unit_tests.py with one change: a step's log is the
one named `<nn>-<step>.log` exactly (train 15's `*-test.log` matched this
train's `05-check-env-reads-self-test.log` too, and two matches read as none).
Train 15 (new; join checks 1 and 3): each unit's own tests, read in the
gate's two full `cargo test` runs at the head. A unit's tests are the test
functions (a `fn` under `#[test]` or `#[tokio::test]`, as tests_kept.py
finds them) its range added or changed -- present at its tip and absent from
its base, or present at both with another body (its text, sha256) -- in a
`.rs` file under `src/` or `tests/`. Each is looked up at the head (the same
file, by name) and its result read in both runs (`test <path>::<name> ...
ok`); a name the head does not hold is listed as gone, with whether the
head's file holds the name at all. A test that fails, or is in neither run,
makes the exit 1. Each test is listed by name under its unit.

    python3 -I -B unit_tests.py <repo> <head> <gate-log-dir> <label>=<base>..<tip>[@<top>]...

`@<top>`: the unit's stack top (the tip the train merged). A test gone at
the head that the top does not hold either went in the unit's own line (a
later unit renamed or folded it), and is listed so; one the top holds and
the head does not, the train lost (LOST, exit 1) -- unless a `--gone
<file>:<name>=<why>` names it: a resolution that dropped it on purpose,
listed with its why and not counted.
"""
import glob
import hashlib
import os
import re
import subprocess
import sys

repo, head, logs = sys.argv[1], sys.argv[2], sys.argv[3]
rest = sys.argv[4:]
GONE = {}
specs = []
i = 0
while i < len(rest):
    if rest[i] == "--gone":
        k, why = rest[i + 1].split("=", 1)
        f, n = k.rsplit(":", 1)
        GONE[(f, n)] = why
        i += 2
    else:
        specs.append(rest[i])
        i += 1
ATTR = re.compile(r"^\s*#\[(?:tokio::)?test(?:\(.*\))?\]\s*$")
FN = re.compile(r"^(\s*)(?:pub(?:\([a-z]+\))? )?(?:async )?fn ([A-Za-z0-9_]+)\b")


def git(*a):
    return subprocess.run(["git", "-C", repo, *a], capture_output=True, text=True).stdout


def tests(commit):
    out = {}
    files = [f for f in git("ls-tree", "-r", "--name-only", commit).split("\n")
             if f.endswith(".rs") and (f.startswith("src/") or f.startswith("tests/"))]
    for f in files:
        lines = git("show", f"{commit}:{f}").split("\n")
        armed = False
        for i, line in enumerate(lines):
            if ATTR.match(line):
                armed = True
                continue
            if armed and line.strip().startswith("#["):
                continue
            m = FN.match(line)
            if armed and m:
                ind = m.group(1)
                end = next((j for j in range(i + 1, len(lines)) if lines[j] == ind + "}"), len(lines) - 1)
                out[(f, m.group(2))] = hashlib.sha256("\n".join(lines[i:end + 1]).encode()).hexdigest()[:12]
            armed = False
    return out


runs = {}
for step in ("test", "test-fault"):
    hits = [h for h in glob.glob(os.path.join(logs, f"*-{step}.log"))
            if re.fullmatch(r"[0-9]+-" + re.escape(step) + r"\.log", os.path.basename(h))]
    runs[step] = open(hits[0], encoding="utf-8", errors="replace").read() if len(hits) == 1 else ""
cache = {}


def at(c):
    if c not in cache:
        cache[c] = tests(c)
    return cache[c]


H = at(head)
bad, total = 0, 0
for spec in specs:
    label, rng = spec.split("=", 1)
    rng, _, top = rng.partition("@")
    base, tip = rng.split("..")
    top = top or tip
    B, T = at(base), at(tip)
    mine = sorted(k for k in T if k not in B or B[k] != T[k])
    added = sum(1 for k in mine if k not in B)
    ok = gone = 0
    lines = []
    for f, n in mine:
        if (f, n) not in H:
            gone += 1
            if (f, n) in GONE:
                lines.append(f"    gone at the head on purpose: {f} {n}: {GONE[(f, n)]}")
            elif (f, n) in at(top):
                bad += 1
                lines.append(f"    LOST: {f} {n} (at its top {top[:8]}, not at the head)")
            else:
                lines.append(f"    gone at the head and at its top {top[:8]} too: {f} {n}")
            continue
        res = []
        for step, text in runs.items():
            m = re.findall(r"^test (?:\S*::)?" + re.escape(n) + r" \.\.\. (\w+)$", text, re.M)
            res.append(m)
        if all(m and all(r == "ok" for r in m) for m in res):
            ok += 1
            lines.append(f"    ok in both runs: {f} {n}")
        else:
            bad += 1
            lines.append(f"    NOT OK: {f} {n}: " + "; ".join(f"{s} {m or 'not found'}" for s, m in zip(runs, res)))
    total += len(mine)
    print(f"{label} ({base[:8]}..{tip[:8]}): {len(mine)} tests ({added} added, {len(mine) - added} changed); "
          f"ok in both runs {ok}; gone at the head {gone}; not ok {len(mine) - ok - gone}")
    print("\n".join(lines)) if lines else None
print(f"# {total} (unit, test) pairs; {bad} not ok or not found in a run")
sys.exit(1 if bad else 0)
