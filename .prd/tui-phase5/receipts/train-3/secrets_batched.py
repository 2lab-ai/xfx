#!/usr/bin/env python3
"""A supplement to scripts/check-no-secrets.sh while it fails closed on E2BIG,
not the gate: the same nine patterns, read from the script's own text, run
over every tracked file in batches of 2000 names, so no `grep` is handed more
than ARG_MAX. Each pattern first finds a sample of its own shape (assembled
here, as the script assembles its own) or the run says MISSED.

    python3 -I -B secrets_batched.py      (from the repository root)
"""
import os
import subprocess

text = open("scripts/check-no-secrets.sh").read()
start = text.index("patterns=(")
block = text[start:text.index(")\n", start)]
patterns = [line.strip().strip('"') for line in block.splitlines()[1:] if line.strip().startswith('"')]
names = [n for n in subprocess.run(["git", "ls-files", "-z"], capture_output=True, check=True).stdout.decode().split("\0") if n]
files = ["./" + n for n in names if os.path.isfile(n) and not os.path.islink(n)]
print(f"{len(patterns)} patterns from scripts/check-no-secrets.sh; {len(names)} tracked names, {len(files)} regular files")
samples = {
    "OpenAI-style key": "sk-" + "A" * 24,
    "GitHub token": "gh" + "p_" + "A" * 24,
    "GitHub fine-grained token": "github" + "_pat_" + "A" * 24,
    "AWS access key id": "AK" + "IA" + "A" * 16,
    "Google API key": "AI" + "za" + "A" * 32,
    "Slack token": "xo" + "xb-" + "1" * 12,
    "private key block": "-----BEGIN RSA PRIVATE" + " KEY-----",
    "JSON web token": "ey" + "J" + "a" * 12 + ".ey" + "J" + "b" * 12 + ".x",
    "bearer literal": "Bea" + "rer " + "a" * 26,
}
total = 0
for entry in patterns:
    name, pattern = entry.split(":", 1)
    control = subprocess.run(["grep", "-c", "-E", "-e", pattern], input=samples[name] + "\n", capture_output=True, text=True)
    hits, worst = [], 0
    for i in range(0, len(files), 2000):
        run = subprocess.run(["grep", "-n", "-E", "-e", pattern, "--"] + files[i:i + 2000],
                             capture_output=True, text=True, stdin=subprocess.DEVNULL)
        worst = max(worst, run.returncode if run.returncode != 1 else 0)
        hits += [line for line in run.stdout.splitlines() if line]
    total += len(hits) + (worst >= 2)
    print(f"{name}: control {'found' if control.stdout.strip() == '1' else 'MISSED'}; "
          f"grep status {worst if worst else 'ok'}; {len(hits)} hit(s)")
    for hit in hits[:5]:
        print("   ", hit[:160])
print(f"total problems: {total}")
