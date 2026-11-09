#!/usr/bin/env bash
# LOCAL-PATHS round 5: each finding of the review of round 4, shown on a
# tree's own code (a throwaway clone at a revision, or the worktree), one line
# of verdict each. Paths it prints are written as placeholders or stand-ins.
#   red5.sh <tree>
set -u
tree=$1
python3 -I - "$tree" <<'EOF'
import os, sys
sys.dont_write_bytecode = True
tree = sys.argv[1]
sys.path.insert(0, os.path.join(tree, "scripts", "fxcap"))
import roots, cells, ledger
def say(a, b):
    print("%-50s %s" % (a, b))

# 1. capture-tmpdir's suffix: the name itself, or a sibling's name.
led = ledger.load(os.path.join(tree, "scripts", "fxcap", "parity-ledger.toml"), tree)
def same(fx, xfx):
    a = cells.normalize(cells.parse(fx), ledger.rules_for(led, "fx"))
    b = cells.normalize(cells.parse(xfx), ledger.rules_for(led, "xfx"))
    return not cells.differing(a, b)
run = "<scratch______________>"
for fx, xfx in (("a %s/fxtmp/b.md", "a %s/xfxtmp/b.md"), ("TMPDIR=%s/fxtmp", "TMPDIR=%s/xfxtmp"), ('"%s/fxtmp" x', '"%s/xfxtmp" x')):
    say("1 %s:" % (fx % "<scratch>"), "normalized" if same(fx % run, xfx % run) else "compared (RED)")
for name in ("fxtmp-old/a", "fxtmp.bak", "fxtmp_x", "fxtmpx", "fxtmp~", "fxtmp+1"):
    say("1 <scratch>/%s:" % name, "normalized (RED)" if same("a %s/%s" % (run, name), "a %s/x%s" % (run, name)) else "compared")

# 2. A root with a gutter character the view drops at a row's edge.
names = {" ": "a space", "\t": "a tab", "│": "U+2502"}
for ch, label in names.items():
    root = "/tmp/fxt" + ch
    rules = roots.rules_for([(root, "tmp")])
    for data, where in ((("x %s\nnext\n" % root).encode(), "a row's end"), (("x %s" % root).encode(), "the text's end")):
        left = roots.survivors(roots.rewrite(data, rules)[0], rules, None)
        say("2 a root ending in %s, at %s:" % (label, where), "refused" if left else "passes (RED)")
rules = roots.rules_for([("/tmp/a b", "tmp")])
for data, label in ((b"x /tmp/a \nb/notes\n", "where its space ends a row"), (b"x /tmp/a\n b/notes\n", "where its space opens a row"),
                    (b"x /tmp/a\t\n  \xe2\x94\x82 b/notes\n", "behind a box-drawing gutter")):
    left = roots.survivors(roots.rewrite(data, rules)[0], rules, None)
    say("2 /tmp/a b, broken %s:" % label, "refused" if left else "passes (RED)")
got = roots.rewrite(b"x /tmp/a b/notes\n", rules)[0]
say("2 /tmp/a b inside a row:", "written, nothing survives" if not roots.survivors(got, rules, None) and b"/tmp/a b" not in got else "not written (RED)")

# 3. The compound case: a head an escape took, beside a styled sibling.
home = "/ho" + "me/alice"  # assembled: this file is scanned
rules = roots.rules_for([(home, "home")])
left = roots.survivors(b"\x1b" + home.encode() + b"\x1b[1m2/notes\n", rules, None)
say("3 ESC <HOME> ESC[1m 2/notes:", "refused (conservative)" if left else "left")
EOF
