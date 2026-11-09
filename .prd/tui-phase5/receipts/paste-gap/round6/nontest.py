"""Non-test source lines added between two commits: the added lines before
each changed file's `#[cfg(test)] mod tests`. python3 -I -B nontest.py <from> <to>"""
import re, subprocess, sys
a, b = sys.argv[1], sys.argv[2]
files = subprocess.run(["git","diff","--name-only",a,b,"--","src"],capture_output=True,text=True).stdout.split()
total = 0
for f in files:
    new = subprocess.run(["git","show",f"{b}:{f}"],capture_output=True,text=True).stdout.split("\n")
    cut = len(new) + 1
    for i, line in enumerate(new, 1):
        if line.strip() == "#[cfg(test)]" and i < len(new) and re.match(r"\s*mod tests", new[i]):
            cut = i; break
    diff = subprocess.run(["git","diff","-U0",a,b,"--",f],capture_output=True,text=True).stdout.split("\n")
    n = 0; at = 0
    for line in diff:
        m = re.match(r"@@ -\S+ \+(\d+)(?:,(\d+))? @@", line)
        if m: at = int(m.group(1)); continue
        if line.startswith("+") and not line.startswith("+++"):
            if at < cut: n += 1
            at += 1
    total += n
    print(f"{f}: {n}")
print("total", total)
