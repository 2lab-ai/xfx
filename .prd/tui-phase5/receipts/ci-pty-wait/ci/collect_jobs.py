#!/usr/bin/env python3
"""Every aarch64-apple-darwin gate job of the given runs, every attempt, with step timings."""
import json, subprocess, sys
from datetime import datetime

def api(path):
    out = subprocess.run(["gh", "api", path], capture_output=True, text=True, check=True).stdout
    return json.loads(out)

def secs(a, b):
    if not a or not b:
        return None
    f = "%Y-%m-%dT%H:%M:%SZ"
    return int((datetime.strptime(b, f) - datetime.strptime(a, f)).total_seconds())

rows = []
for line in open(sys.argv[1]):
    run_id, attempts = line.split("\t")[:2]
    for n in range(1, int(attempts) + 1):
        jobs = api(f"repos/2lab-ai/xfx/actions/runs/{run_id}/attempts/{n}/jobs?per_page=50")["jobs"]
        for job in jobs:
            if "aarch64-apple-darwin" not in job["name"]:
                continue
            steps = {s["name"]: {"conclusion": s["conclusion"], "secs": secs(s.get("started_at"), s.get("completed_at"))}
                     for s in job["steps"]}
            rows.append({"run": int(run_id), "attempt": n, "job": job["id"], "sha": job["head_sha"][:8],
                         "conclusion": job["conclusion"], "status": job["status"],
                         "runner": job.get("runner_name"), "labels": job.get("labels"),
                         "steps": steps})
json.dump(rows, sys.stdout, indent=1)
