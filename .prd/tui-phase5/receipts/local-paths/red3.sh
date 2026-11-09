#!/usr/bin/env bash
# LOCAL-PATHS round 3: each finding of the review of round 2, shown on round
# 2's own code (a throwaway clone at <rev>), one line of verdict each. Paths
# it prints are written as placeholders.
#   red3.sh <clone at the round-2 head>
set -u
clone=$1
fx=$clone/scripts/fxcap
work=$(mktemp -d "${TMPDIR:-/tmp}/lp-red3.XXXXXX")
trap 'rm -rf "$work"' EXIT
say() { printf '%-34s %s\n' "$1" "$2"; }

# 1. A root wrapped at two columns reads the same after the rewrite.
python3 -I - "$fx" <<'EOF'
import sys
sys.dont_write_bytecode = True
sys.path.insert(0, sys.argv[1])
import roots
run = "/tmp/claude" + "-0/fxcapt.abcdefgh/fxcap.JTbdal"  # assembled: this file is scanned
rules = roots.rules_for([(run, "scratch")])
a = roots.rewrite(("x=%s\n  %s\n" % (run[:9], run[9:])).encode(), rules)[0]
b = roots.rewrite(("x=%s\n  %s\n" % (run[:12], run[12:])).encode(), rules)[0]
print("%-34s %s" % ("1 wrap at 9 vs 12:", "the same after the rewrite (RED)" if a == b else "differ"))
import json
try:
    json.loads(roots.rewrite(b'{"text":"<work>"}', rules)[0])
    print("%-34s %s" % ("NTH literal in JSON:", "valid"))
except ValueError:
    print("%-34s %s" % ("NTH literal in JSON:", "not JSON after the escape (RED)"))
home = "/ho" + "me/alice"
left = roots.survivors(("%s2/notes\n" % home).encode(), roots.rules_for([(home, "home")]), None)
print("%-34s %s" % ("3 a sibling of HOME:", "refused (RED)" if left else "left"))
work_rules = roots.rules_for([(run + "/work", "work"), (run, "scratch")])
out = roots.rewrite(("x \x1b(B%s/work\x1b(B y\n" % run).encode(), work_rules)[0]
print("%-34s %s" % ("NIT nF escape after a root:", "<work> read as <scratch>/work (RED)" if b"/work" in out else "rewritten whole"))
EOF

# 2 and 4. The check: a report capture's mark, and a placeholder home at the
# line's end after a longer home.
t=$work/check
mkdir -p "$t/scripts" "$t/w3"
cp "$clone/scripts/check-no-local-paths.sh" "$t/scripts/"
s=/
printf 'Workspace: %shome%sbob%swork  # local-path-ok\n' "$s" "$s" "$s" >"$t/w3/xfx-trace-40x120-report-1.md"
printf 'cp x %shome%subuntu%sy; cd %shome%su\n' "$s" "$s" "$s" "$s" "$s" >"$t/sentinel.sh"
GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git -C "$t" init -q
GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git -C "$t" add -A -f
out=$(cd "$t" && ./scripts/check-no-local-paths.sh 2>&1 </dev/null) || true
case "$out" in *"report-1.md:1:"*) say "2 a mark in a report capture:" "refused" ;; *) say "2 a mark in a report capture:" "passes (RED)" ;; esac
case "$out" in *"sentinel.sh:1:"*) say "4 a home cut at its first match:" "refused" ;; *) say "4 a home cut at its first match:" "passes (RED)" ;; esac

# 5. A TMPDIR through a link, with a trailing slash: a binary that prints its
# HOME normalized.
d=$work/tmp
mkdir -p "$d/r" "$d/scen"
ln -s "$d/r" "$d/l"
printf 'keep\nwait 1\ntype clear; printf "norm=%%s\\n" "$(printf %%s "$HOME" | tr -s /)"; echo DO""NE\nkey Enter\nuntil shown DONE\nsnap a\n' >"$d/scen/t.cap"
mkdir -p "$d/bin"
ln -s /bin/bash "$d/bin/it"
status=0
env -u TMUX -u TMUX_PANE FXCAP_SCENARIOS="$d/scen" TMPDIR="$d/l/" FX_BIN="$d/bin/it" \
	bash "$fx/capture.sh" fx t 30x200 "$d/out" >"$work/cap.log" 2>&1 </dev/null || status=$?
for sock in "$d"/l/fxcap.*/s "$d"/l//fxcap.*/s; do [ -S "$sock" ] && tmux -f /dev/null -S "$sock" kill-server 2>/dev/null; done
if [ "$status" -eq 3 ]; then say "5 trailing-slash TMPDIR:" "refused, exit 3 (RED)"; else say "5 trailing-slash TMPDIR:" "exit $status"; fi
