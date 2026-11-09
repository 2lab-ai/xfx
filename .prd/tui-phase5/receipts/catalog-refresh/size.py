#!/usr/bin/env python3
"""How much source outside tests the unit changes, base..head: every changed
file under src/, cut at its `#[cfg(test)] mod tests` (and, in
catalog_clock.rs, without its `#[cfg(test)]` items), counted twice -- every
line, and code alone (no blank or `//` line).

usage: python3 -I size.py <base> <head>   (run in the checkout)"""
import difflib, subprocess, sys

def show(rev, path):
    return subprocess.run(["git", "show", f"{rev}:{path}"], capture_output=True, text=True).stdout

def cut(text, path):
    i = text.find("\n#[cfg(test)]\nmod tests {")
    text = text if i < 0 else text[:i]
    if not path.endswith("catalog_clock.rs"):
        return text
    out, lines, i = [], text.splitlines(), 0
    while i < len(lines):
        if lines[i].startswith("#[cfg(test)]"):
            while i < len(lines) and not lines[i].startswith("}"):
                i += 1
        else:
            out.append(lines[i])
        i += 1
    return "\n".join(out)

base, head = sys.argv[1], sys.argv[2]
files = subprocess.run(["git", "diff", "--name-only", base, head, "--", "src"], capture_output=True, text=True).stdout.split()
total = [0, 0, 0, 0]
for f in files:
    a = d = ca = cd = 0
    for line in difflib.unified_diff(cut(show(base, f), f).splitlines(), cut(show(head, f), f).splitlines(), lineterm="", n=0):
        if line[:3] in ("+++", "---") or line.startswith("@@"):
            continue
        body = line[1:].strip()
        code = 1 if body and not body.startswith("//") else 0
        if line.startswith("+"):
            a, ca = a + 1, ca + code
        elif line.startswith("-"):
            d, cd = d + 1, cd + code
    total = [total[0] + a, total[1] + d, total[2] + ca, total[3] + cd]
    print(f"{f}: every line +{a} -{d}; code +{ca} -{cd}")
print(f"total outside tests: every line +{total[0]} -{total[1]}; code +{total[2]} -{total[3]}")
