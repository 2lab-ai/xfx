#!/usr/bin/env bash
# LOCAL-PATHS round 4: each finding of the review of round 3, shown on a
# tree's own code (a throwaway clone at a revision, or the worktree), one line
# of verdict each. Paths it prints are written as placeholders.
#   red4.sh <tree>
set -u
tree=$1
work=$(mktemp -d "${TMPDIR:-/tmp}/lp-red4.XXXXXX")
trap 'rm -rf "$work"' EXIT
say() { printf '%-44s %s\n' "$1" "$2"; }

# 1. A sibling of an outer root, styled mid-name, and a root in a control
# sequence's bytes. 2. The binary's TMPDIR after the run directory only.
python3 -I - "$tree" <<'EOF'
import os, sys
sys.dont_write_bytecode = True
tree = sys.argv[1]
sys.path.insert(0, os.path.join(tree, "scripts", "fxcap"))
import roots, cells, ledger
home, tmp, scen = "/ho" + "me/alice", "/tmp/fxt", "/src/scen"  # assembled: this file is scanned
rules = roots.rules_for([(home, "home"), (tmp, "tmp"), (scen, "scenarios")])
def say(a, b):
    print("%-44s %s" % (a, b))
for root, name, rest in ((home, "home", "2/notes"), (tmp, "tmp", "2/x"), (scen, "scenarios", "-old/x")):
    for style, sname in ((b"\x1b[1m", "SGR"), (b"\x1b[38;5;245m", "256-colour SGR"), (b"\x1b(B", "nF")):
        data = b"x " + root.encode() + style + rest.encode() + b"\x1b[0m y\n"
        left = roots.survivors(data, rules, None)
        say("1 %s sibling, %s mid-name:" % (name, sname), "refused (RED)" if left else "left")
link = b"\x1b]8;;file://%s/shot.png\x1b\\shot\x1b]8;;\x1b\\\n"
say("1 HOME in an OSC 8 target:", "refused" if roots.survivors(link % home.encode(), rules, None) else "passes (RED)")
say("1 HOME's sibling in an OSC 8 target:", "refused (RED)" if roots.survivors(link % (home + "2").encode(), rules, None) else "left")
say("1 HOME whose head an nF escape took:", "refused" if roots.survivors(b"x \x1b" + home.encode() + b"/x\n", rules, None) else "passes (RED)")
led = ledger.load(os.path.join(tree, "scripts", "fxcap", "parity-ledger.toml"), tree)
def same(prefix):
    a = cells.normalize(cells.parse("saved at %s/fxtmp/a.md" % prefix), ledger.rules_for(led, "fx"))
    b = cells.normalize(cells.parse("saved at %s/xfxtmp/a.md" % prefix), ledger.rules_for(led, "xfx"))
    return not cells.differing(a, b)
for prefix in ("<scratch______________>", "<scratch:real_________>", "<scratch>", "/t/fxcap.Lq26sv"):
    say("2 %s/fxtmp:" % prefix, "normalized" if same(prefix) else "compared (RED)")
for prefix in ("<work___>", "<work:real__>", "<tmp______>", "<!scratch__>", "a_>", "<s__>", "<S__>"):
    say("2 %s/fxtmp:" % prefix, "normalized (RED)" if same(prefix) else "compared")
EOF

# 3. The check's reading of capture.sh's reserved names: a report renamed to
# an ordinary name, and an authored doc whose name matches the pattern.
t=$work/check
mkdir -p "$t/scripts" "$t/w3" "$t/docs"
cp "$tree/scripts/check-no-local-paths.sh" "$t/scripts/"
s=/
printf 'Workspace: %shome%sbob%swork\n' "$s" "$s" "$s" >"$t/w3/trace-report.md"
printf 'Workspace: %shome%sbob%swork  # local-path-ok\n' "$s" "$s" "$s" >"$t/w3/trace-report-marked.md"
printf 'cd %shome%sbob  # local-path-ok: an example\n' "$s" "$s" >"$t/docs/fx-guide-40x120-notes.md"
GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git -C "$t" init -q
GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git -C "$t" add -A -f
out=$(cd "$t" && ./scripts/check-no-local-paths.sh 2>&1 </dev/null) || true
for f in w3/trace-report.md w3/trace-report-marked.md docs/fx-guide-40x120-notes.md; do
	case "$out" in *"$f:1:"*) say "3 $f:" "refused" ;; *) say "3 $f:" "passes" ;; esac
done

# NIT: verify.py's visible width of a row holding an OSC 8 link (ESC \ ends it).
python3 -I - "$tree" <<'EOF'
import ast, os, re, sys
src = open(os.path.join(sys.argv[1], ".prd/tui-phase5/receipts/local-paths/verify.py")).read()
pattern = re.search(r'text = re\.sub\((rb"[^"]*")', src).group(1)
row = b"\x1b]8;;file://<work___>/a.png\x1b\\link\x1b]8;;\x1b\\"
n = len(re.sub(ast.literal_eval(pattern), b"", row))
print("%-44s %s" % ("NIT verify.py, an OSC 8 link's width:", "%d (the 4 of `link`)" % n if n == 4 else "%d (RED)" % n))
EOF
