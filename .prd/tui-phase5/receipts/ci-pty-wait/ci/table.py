#!/usr/bin/env python3
"""One row per aarch64-apple-darwin gate job: step times, tests/tui.rs run times, failing tests."""
import json, re, sys
rows = json.load(open(sys.argv[1]))
out = []
for r in rows:
    if r["status"] != "completed" or r["conclusion"] not in ("success", "failure"):
        continue
    try:
        text = open(f"logs/{r['job']}.log", errors="replace").read()
    except FileNotFoundError:
        continue
    # tests/tui.rs runs, in order: the plain step's, then the fault injector's
    tui = []
    for m in re.finditer(r"Running tests/tui\.rs \(", text):
        # the binary's own result: the last one before the next binary starts
        # (a case's child harness prints one of its own inside the run)
        rest = text[m.end():]
        nxt = re.search(r"Running (unittests|tests/)|##\[group\]Run ", rest)
        body = rest[: nxt.start()] if nxt else rest
        res = list(re.finditer(r"test result: (ok|FAILED)\. (\d+) passed; (\d+) failed.*?finished in ([\d.]+)s", body))
        tui.append((res[-1].group(1), int(res[-1].group(2)) + int(res[-1].group(3)), float(res[-1].group(4))) if res else None)
    failed = sorted(set(re.findall(r"^\S+ thread '([a-z0-9_:]+)' \(\d+\) panicked at (tests/[a-z_/]+\.rs:\d+)", text, re.M)))
    s = r["steps"]
    out.append({
        "run": r["run"], "attempt": r["attempt"], "job": r["job"], "sha": r["sha"],
        "conclusion": r["conclusion"],
        "tests_step_s": s.get("Tests", {}).get("secs"),
        "fault_step_s": s.get("Tests, with the fault injector", {}).get("secs"),
        "tui_plain": tui[0] if len(tui) > 0 else None,
        "tui_fault": tui[1] if len(tui) > 1 else None,
        "failed": failed,
        "failed_step": [k for k, v in s.items() if v["conclusion"] == "failure"],
    })
json.dump(out, sys.stdout, indent=1)
