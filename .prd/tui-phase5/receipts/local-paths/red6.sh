#!/usr/bin/env bash
# LOCAL-PATHS round 6: each finding of the review of round 5, shown on a
# tree's own code (a throwaway clone at a revision, or the worktree), one line
# of verdict each. Paths it prints are written as placeholders or stand-ins.
#   red6.sh <tree>
set -u
tree=$1
work=$(mktemp -d "${TMPDIR:-/tmp}/lp-red6.XXXXXX")
trap 'rm -rf "$work"' EXIT
python3 -I - "$tree" "$work" <<'EOF'
import os, subprocess, sys
sys.dont_write_bytecode = True
tree, work = sys.argv[1], sys.argv[2]
fx = os.path.join(tree, "scripts", "fxcap")
sys.path.insert(0, fx)
import roots, cells, ledger
def say(a, b):
    print("%-48s %s" % (a, b))

# 1. capture-tmpdir: what may follow `fxtmp`.
led = ledger.load(os.path.join(fx, "parity-ledger.toml"), tree)
def same(a, b):
    a = cells.normalize(cells.parse(a), ledger.rules_for(led, "fx"))
    b = cells.normalize(cells.parse(b), ledger.rules_for(led, "xfx"))
    return not cells.differing(a, b)
run = "<scratch______________>"
for name in ("fxtmpé/a", "fxtmp%C3%A9/a", "fxtmp中/a", "fxtmp@x", "fxtmp=x", "fxtmp#1", "fxtmp(1)", "fxtmp!",
             "fxtmp<x", "fxtmp-old/a", "fxtmp.bak", "fxtmp~", "fxtmp+1"):
    say("1 <scratch>/%s:" % name, "normalized (RED)" if same("a %s/%s" % (run, name), "a %s/x%s" % (run, name)) else "compared")
for after, label in (("/b.md", "/"), (" x", "a space"), ("\tx", "a tab"), ('" x', '"'), ("' x", "'"), (") x", ")"),
                     ("] x", "]"), ("} x", "}"), ("> x", ">"), (", x", ","), (": x", ":"), ("; x", ";"),
                     ("| x", "|"), ("", "the row's end")):
    say("1 <scratch>/fxtmp, then %s:" % label, "normalized" if same("a %s/fxtmp%s" % (run, after), "a %s/xfxtmp%s" % (run, after)) else "compared (RED)")

# 2. A root under another, broken at its space: roots.py as capture.sh runs it.
home = "/Users/someone"  # the check's own placeholder home
scen = home + "/My Docs/xfx/scripts/fxcap/scenarios"
data = ("x %s/My \nDocs/xfx/scripts/fxcap/scenarios/a.json\n" % home).encode()
path = os.path.join(work, "nested.txt")
with open(path, "wb") as f:
    f.write(data)
r = subprocess.run([sys.executable, "-I", os.path.join(fx, "roots.py"), "--root", home, "home", "--root", scen, "scenarios",
                    "--", path], capture_output=True, text=True, env={"PYTHONDONTWRITEBYTECODE": "1"})
with open(path, "rb") as f:
    out = f.read()
leaked = b"Docs/xfx/scripts" in out and r.returncode == 0
say("2 <home>/My Docs/... broken at its space:", "published as <home__>/My Docs/... (RED)" if leaked else "refused, exit %d" % r.returncode)
# And a root on its own, broken at its space: refused before any edit, or only by the survivor reading.
rules = roots.rules_for([("/tmp/a b", "tmp")])
try:
    roots.rewrite(b"x /tmp/a \nb/notes\n", rules)
    how = "rewritten, then refused by the survivor reading"
except roots.Refused:
    how = "refused before any edit"
say("2 /tmp/a b broken at its space:", how)
EOF
