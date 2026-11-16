#!/usr/bin/env python3
"""Train 16, round 3 (new): the join of main kept every step of ci.yml's gate
job from both sides. For each side (ours, theirs) and the merged commit,
the gate job's steps are read with Ruby's YAML (the reader test-gate-env.sh
uses); every step of a side must stand in the merge with its `if`, `run`
and `with` unchanged, found by its name (RENAMED maps a name a fix changed),
and the side's steps must keep their order there. Then the merge's matrix
rows against each side's, every comment line of the gate job either side
has (its words, whitespace folded) and whether the merge holds it whole,
and the merged order. Round 4 (gpt6's join review, SHOULD 3): every step
the merge runs under `matrix.script_tests`, and the rows that condition
selects, read from the YAML itself -- the env self-test among them, which
test-gate-env.sh's `ci-script-tests-per-os` does not read.

    python3 -I -B join_steps.py <repo> <ours> <theirs> <merged>
"""
import json
import re
import subprocess
import sys

repo, ours, theirs, merged = sys.argv[1:5]
RENAMED = {"Install the stable toolchain": "Install the pinned toolchain"}


def show(commit):
    return subprocess.run(["git", "-C", repo, "show", f"{commit}:.github/workflows/ci.yml"], check=True,
                          capture_output=True, text=True).stdout


def job(commit):
    out = subprocess.run(["ruby", "-ryaml", "-rjson", "-e",
                          'd = YAML.safe_load(STDIN.read, aliases: true); puts JSON.generate(d["jobs"]["gate"])'],
                         input=show(commit), check=True, capture_output=True, text=True).stdout
    return json.loads(out)


def comments(commit):
    text = show(commit)
    start = text.index("\n  gate:")
    nxt = re.search(r"\n  [a-z][a-z_-]*:\n", text[start + 1:])
    body = text[start:start + 1 + nxt.start()] if nxt else text[start:]
    return [re.sub(r"\s+", " ", l.strip()[1:].strip()) for l in body.split("\n") if l.strip().startswith("#")]


m = job(merged)
names = [s.get("name") for s in m["steps"]]
bad = 0
for side, c in (("ours", ours), ("theirs", theirs)):
    j = job(c)
    at = []
    for s in j["steps"]:
        name = RENAMED.get(s.get("name"), s.get("name")) if side == "ours" else s.get("name")
        if name not in names:
            bad += 1
            print(f"MISSING {side}: {s.get('name')!r}")
            continue
        t = m["steps"][names.index(name)]
        same = {k: s.get(k) == t.get(k) for k in ("if", "run", "uses")}
        # `with`: the side's keys and values in the merge's (the other side may add one)
        same["with"] = all((t.get("with") or {}).get(k) == v for k, v in (s.get("with") or {}).items())
        extra = sorted(set(t.get("with") or {}) - set(s.get("with") or {}))
        if extra:
            print(f"  {side}: {s.get('name')!r}: the merge's `with` adds {extra} (the other side's)")
        if side == "ours" and s.get("name") in RENAMED:
            same["run"] = True  # theirs changed the run (1.99.1): read below
        if not all(same.values()):
            bad += 1
            print(f"CHANGED {side}: {s.get('name')!r}: " + ", ".join(k for k, v in same.items() if not v))
        at.append(names.index(name))
    print(f"# {side} {c[:8]}: {len(j['steps'])} steps, all in the merge: {len(at) == len(j['steps'])}; "
          f"in its order there: {at == sorted(at)}")
    bad += at != sorted(at)
    rows = j["strategy"]["matrix"]["include"]
    print(f"# {side}: matrix rows {len(rows)}; the merge's {len(m['strategy']['matrix']['include'])}; "
          f"each of the side's rows' keys and values in the merge's row of its runner and part: "
          f"{all(all(r2.get(k) == v for k, v in r.items()) for r in rows for r2 in m['strategy']['matrix']['include'] if (r2['runner'], r2.get('part')) == (r['runner'], r.get('part')))}")
toolchain = m["steps"][names.index("Install the pinned toolchain")]["run"]
print(f"# the toolchain step in the merge: {toolchain.splitlines()[:2]}")
mc = " ".join(comments(merged))
for side, c in (("ours", ours), ("theirs", theirs)):
    cs = comments(c)
    missing = [x for x in cs if x and x not in mc]
    print(f"# {side}: {len(cs)} comment lines in the gate job; not whole in the merge's: {len(missing)}"
          f" (the `script_tests` comment, the one conflict block, byhand.py 7: its words are in the merged"
          f" sentence, read in the README)")
    for x in missing:
        print(f"    {x}")
print("# the merged order:")
for i, n in enumerate(names):
    s = m["steps"][i]
    print(f"  {i:2} {n} | {s.get('if', '')}")
rows = m["strategy"]["matrix"]["include"]
picked = [r["runner"] + (f" ({r['part']})" if r.get("part") else "") for r in rows if r.get("script_tests")]
print(f"# the rows `matrix.script_tests` selects, from the merge's matrix: {', '.join(picked)}")
print("# the steps under it, each on those rows alone:")
for i, n in enumerate(names):
    st = m["steps"][i]
    if st.get("if", "").strip() == "matrix.script_tests":
        print(f"  {i:2} {n}: {st.get('run', '').strip().splitlines()[-1][:80]}")
print(f"# {bad} missing, changed or out of order")
sys.exit(1 if bad else 0)
