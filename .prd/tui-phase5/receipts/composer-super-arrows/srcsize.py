# usage: python3 -I srcsize.py <repo> <base> <head>: the +/- lines of `git diff -U0 <base> <head> -- src` above each file's `mod tests`
import re, subprocess, sys
repo, base, head = sys.argv[1:4]
files = subprocess.run(["git","-C",repo,"diff","--name-only",base] + ([] if head=="WORKTREE" else [head]) + ["--","src"],capture_output=True,text=True,check=True).stdout.split()
def marker(text):
    lines = text.split("\n")
    for i,l in enumerate(lines):
        if l.startswith("mod tests {") and i>0 and lines[i-1].startswith("#[cfg(test)]"):
            return i  # 1-based line of the attribute = i
    return len(lines)+1
tot_a=tot_r=0
for f in files:
    old = subprocess.run(["git","-C",repo,"show",f"{base}:{f}"],capture_output=True,text=True).stdout
    new = open(f"{repo}/{f}").read() if head=="WORKTREE" else subprocess.run(["git","-C",repo,"show",f"{head}:{f}"],capture_output=True,text=True).stdout
    om, nm = marker(old), marker(new)
    d = subprocess.run(["git","-C",repo,"diff","-U0",base]+([] if head=="WORKTREE" else [head])+["--",f],capture_output=True,text=True).stdout
    a=r=0
    for h in re.finditer(r"^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@", d, re.M):
        os_,oc,ns,nc = int(h[1]), int(h[2] or 1), int(h[3]), int(h[4] or 1)
        if oc and os_ < om: r += min(oc, om-os_)
        if nc and ns < nm: a += min(nc, nm-ns)
    print(f"{f}: +{a}/-{r}")
    tot_a+=a; tot_r+=r
print(f"total: +{tot_a}/-{tot_r}")
